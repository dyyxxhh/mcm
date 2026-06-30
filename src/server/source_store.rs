//! Filesystem-backed source index + blob store for source-mode servers.
//!
//! The operator populates `data_dir/source-index.json` and
//! `data_dir/source-blobs/<slug>` manually. The server serves these
//! read-only. The store never generates or mutates them. Any computer
//! can run source mode — just point `MCM_SHARE_DATA_DIR` at a directory
//! containing the index + blobs.
//!
//! # Paths
//! - Index: `data_dir/source-index.json` (a `SourceIndex` JSON document).
//! - Blob:  `data_dir/source-blobs/<slug>` (raw artifact bytes).
//!
//! # Missing files
//! - Missing index → `get_index()` returns `Ok(None)` → handler 404.
//! - Missing blob  → `get_blob()` returns `Ok(None)` → handler 404.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::source_index::{parse_source_index, SourceIndex, SourcePackage};

const INDEX_FILE: &str = "source-index.json";
const BLOBS_DIR: &str = "source-blobs";

#[derive(Clone)]
pub(crate) struct SourceStore {
    inner: Arc<SourceStoreInner>,
}

struct SourceStoreInner {
    data_dir: PathBuf,
}

impl SourceStore {
    pub(crate) fn new(data_dir: PathBuf) -> Self {
        Self {
            inner: Arc::new(SourceStoreInner { data_dir }),
        }
    }

    fn index_path(&self) -> PathBuf {
        self.inner.data_dir.join(INDEX_FILE)
    }

    fn blobs_dir(&self) -> PathBuf {
        self.inner.data_dir.join(BLOBS_DIR)
    }

    fn blob_path(&self, slug: &str) -> Result<PathBuf> {
        let clean = sanitize_blob_slug(slug)?;
        Ok(self.blobs_dir().join(clean))
    }

    /// Read and parse the operator-authored source index. `Ok(None)` if the
    /// index file does not exist (handler returns 404). Parse errors bubble
    /// up as `Err` so the handler can surface a 500.
    pub(crate) fn get_index(&self) -> Result<Option<SourceIndex>> {
        let path = self.index_path();
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("read source index {}", path.display()))?;
        let index = parse_source_index(&text)?;
        Ok(Some(index))
    }

    /// Find a package by slug (package id) in the index. `Ok(None)` if the
    /// index is absent or the slug is not declared.
    pub(crate) fn get_package(&self, slug: &str) -> Result<Option<SourcePackage>> {
        let Some(index) = self.get_index()? else {
            return Ok(None);
        };
        Ok(index.packages.into_iter().find(|pkg| pkg.id == slug))
    }

    /// Read raw blob bytes for `slug`. `Ok(None)` if the blob file does not
    /// exist (handler returns 404).
    pub(crate) fn get_blob(&self, slug: &str) -> Result<Option<Vec<u8>>> {
        let path = self.blob_path(slug)?;
        if !path.exists() {
            return Ok(None);
        }
        let bytes =
            std::fs::read(&path).with_context(|| format!("read source blob {}", path.display()))?;
        Ok(Some(bytes))
    }
}

/// Validate a blob slug for safe filesystem joining. Rejects empty, path
/// separators, `..`, absolute prefixes, and Windows drive letters — same
/// discipline as `mcm_package::validate_asset_path` but trimmed for slugs
/// (which are `[a-z0-9-]` source package ids).
fn sanitize_blob_slug(slug: &str) -> Result<String> {
    if slug.is_empty()
        || slug.contains('/')
        || slug.contains('\\')
        || slug.contains("..")
        || slug.contains('\0')
        || slug.starts_with('/')
    {
        anyhow::bail!("invalid blob slug {slug:?}");
    }
    let bytes = slug.as_bytes();
    let has_drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if has_drive {
        anyhow::bail!("invalid blob slug {slug:?}");
    }
    Ok(slug.to_owned())
}

/// Used by tests that need to pre-populate the blobs dir.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn blobs_dir_for(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join(BLOBS_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_index_returns_none_when_file_absent() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let store = SourceStore::new(tmp.path().to_path_buf());
        assert!(store.get_index().expect("ok").is_none());
    }

    #[test]
    fn get_index_parses_valid_file() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let json = r#"{"schema_version":1,"source_id":"s","packages":[]}"#;
        std::fs::write(tmp.path().join(INDEX_FILE), json).expect("write");
        let store = SourceStore::new(tmp.path().to_path_buf());
        let index = store.get_index().expect("ok").expect("some");
        assert_eq!(index.source_id, "s");
    }

    #[test]
    fn get_package_finds_by_id() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let json = r#"{"schema_version":1,"source_id":"s","packages":[
            {"id":"alpha","title":"A","versions":[]},
            {"id":"beta","title":"B","versions":[]}
        ]}"#;
        std::fs::write(tmp.path().join(INDEX_FILE), json).expect("write");
        let store = SourceStore::new(tmp.path().to_path_buf());
        let pkg = store.get_package("beta").expect("ok").expect("found");
        assert_eq!(pkg.title, "B");
    }

    #[test]
    fn get_package_returns_none_for_unknown_slug() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let json = r#"{"schema_version":1,"source_id":"s","packages":[]}"#;
        std::fs::write(tmp.path().join(INDEX_FILE), json).expect("write");
        let store = SourceStore::new(tmp.path().to_path_buf());
        assert!(store.get_package("nope").expect("ok").is_none());
    }

    #[test]
    fn get_blob_returns_none_when_absent() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let store = SourceStore::new(tmp.path().to_path_buf());
        assert!(store.get_blob("missing").expect("ok").is_none());
    }

    #[test]
    fn get_blob_reads_file() {
        let tmp = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir_all(tmp.path().join(BLOBS_DIR)).expect("dir");
        std::fs::write(tmp.path().join(BLOBS_DIR).join("alpha"), b"bytes").expect("write");
        let store = SourceStore::new(tmp.path().to_path_buf());
        let bytes = store.get_blob("alpha").expect("ok").expect("some");
        assert_eq!(bytes, b"bytes");
    }

    #[test]
    fn sanitize_rejects_traversal() {
        assert!(sanitize_blob_slug("").is_err());
        assert!(sanitize_blob_slug("..").is_err());
        assert!(sanitize_blob_slug("a/b").is_err());
        assert!(sanitize_blob_slug(r"a\b").is_err());
        assert!(sanitize_blob_slug("/abs").is_err());
        assert!(sanitize_blob_slug("C:foo").is_err());
        assert!(sanitize_blob_slug("good-id").is_ok());
    }
}

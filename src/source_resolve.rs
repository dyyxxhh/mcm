//! Resolve a package slug via imported custom sources.
//!
//! When `pkg install <slug>` receives a bare slug (not a `.mcm` path or URL),
//! this module checks the user's imported sources. If a source declares the
//! slug, the declared version's artifact is turned into a synthetic
//! [`McmLock`] with a `mod.install` step carrying the source-declared
//! `download_url` (external) or the source service's blob endpoint
//! (source-hosted), plus the declared `sha256` hash. The existing
//! [`crate::pkg_install`] apply path then downloads, hash-verifies, and
//! installs the artifact.
//!
//! Imported sources are trusted — a hash mismatch is a corruption error,
//! NOT a hostile-source warning.

use std::path::Path;

use anyhow::{Context, Result};

use crate::app::App;
use crate::download::{download_file, DownloadOptions, HttpFetcher};
use crate::i18n;
use crate::mcm_package::{LockStep, McmLock, StepPermission};
use crate::safety::sanitize_filename;
use crate::source_index::{fetch_source_index, source_blob_url, SourceVersion};

impl App {
    pub(crate) fn resolve_from_sources(&self, target: &str) -> Result<Option<McmLock>> {
        if target.starts_with("http") || target.ends_with(".mcm") {
            return Ok(None);
        }
        let config = self.load_config()?;
        for url in config.sources.keys() {
            if !url.starts_with("http") {
                continue;
            }
            match fetch_source_index(url) {
                Ok(index) => {
                    if let Some(pkg) = find_package(url, &index.packages, target) {
                        return Ok(Some(pkg));
                    }
                }
                Err(e) => {
                    eprintln!(
                        "{}",
                        i18n::source_unavailable(self.lang, url, &e.to_string())
                    );
                }
            }
        }
        Ok(None)
    }
}

/// Locate a package by slug, pick the first version, and synthesize a
/// `McmLock` with a single `mod.install` step pointing at the source artifact.
fn find_package(
    index_url: &str,
    packages: &[crate::source_index::SourcePackage],
    slug: &str,
) -> Option<McmLock> {
    let pkg = packages.iter().find(|p| p.id == slug)?;
    let ver = pkg.versions.first()?;
    let download_url = artifact_url(index_url, ver);
    let step = LockStep {
        op: "mod.install".to_owned(),
        permission: StepPermission::Install,
        args: serde_json::json!({
            "id": pkg.id,
            "provider": "source",
            "version": ver.version,
            "filename": ver.filename,
            "download_url": download_url,
            "sha256": ver.sha256,
        }),
        source_line: None,
    };
    Some(McmLock {
        schema_version: 2,
        kind: "mcm-lock".to_owned(),
        identity: crate::mcm_package::LockIdentity {
            name: pkg.id.clone(),
            version: ver.version.clone(),
            description: pkg.description.clone(),
        },
        author: crate::mcm_package::LockAuthor::default(),
        permissions: crate::mcm_package::LockPermissions {
            install: true,
            do_permitted: false,
            full: false,
        },
        game: Some(crate::mcm_package::LockGame {
            game: None,
            version: ver.mc_versions.first().cloned(),
            loader: ver.loaders.first().cloned(),
        }),
        steps: vec![step],
        artifacts: Vec::new(),
        created_at: crate::mcm_package::now_rfc3339(),
        generator: Some("mcm-source-resolve".to_owned()),
    })
}

/// Resolve the download URL for a source version: external `download_url`
/// wins, otherwise the source service's `/blob/{slug}` endpoint (derived
/// from the index URL via `blob_ref`).
fn artifact_url(index_url: &str, ver: &SourceVersion) -> String {
    if let Some(dl) = ver.download_url.as_deref() {
        return dl.to_owned();
    }
    ver.blob_ref
        .as_deref()
        .map(|r| source_blob_url(index_url, r))
        .unwrap_or_default()
}

/// Download a source-resolved mod artifact to `mods_dir/<filename>` via the
/// retryable download engine. Source URLs are trusted (the user imported the
/// source), so HTTP + local hosts are allowed — the CDN allowlist does NOT
/// apply. The declared `sha256` is enforced; a mismatch is a corruption
/// error ("integrity check failed"), NOT a hostile-source warning. Returns
/// the actual sha256 of the downloaded file on success.
pub(crate) fn install_source_mod(step: &LockStep, mods_dir: &Path) -> Result<String> {
    let lang = crate::i18n::Lang::default();
    let args = &step.args;
    let download_url = args
        .get("download_url")
        .and_then(|v| v.as_str())
        .with_context(|| i18n::source_mod_no_download_url(lang, step.op.as_str()))?;
    let filename = args
        .get("filename")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown.jar");
    let sha256 = args
        .get("sha256")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let filename = sanitize_filename(filename)?;
    let dest = mods_dir.join(&filename);
    let fetcher = HttpFetcher::new(download_url);
    let opts = DownloadOptions {
        expected_sha256: sha256,
        backoff_base_ms: 100,
        ..Default::default()
    };
    let outcome = download_file(&dest, &fetcher, &opts).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("hash mismatch") {
            anyhow::anyhow!("{}", i18n::integrity_check_failed(lang, download_url, &msg))
        } else {
            e
        }
    })?;
    Ok(outcome.sha256)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_index::{SourcePackage, SourceVersion};

    fn ver(filename: &str, url: Option<&str>, blob: Option<&str>) -> SourceVersion {
        SourceVersion {
            version: "1.0.0".to_owned(),
            mc_versions: vec!["1.20.1".to_owned()],
            loaders: vec!["fabric".to_owned()],
            side: Some("both".to_owned()),
            filename: filename.to_owned(),
            download_url: url.map(str::to_owned),
            blob_ref: blob.map(str::to_owned),
            sha256: Some("abc".to_owned()),
            size: Some(1),
            deps: Vec::new(),
        }
    }

    #[test]
    fn find_package_returns_none_for_empty() {
        assert!(find_package("http://x/index", &[], "x").is_none());
    }

    #[test]
    fn find_package_synthesizes_lock_with_external_url() {
        let pkg = SourcePackage {
            id: "alpha".to_owned(),
            title: "Alpha".to_owned(),
            description: None,
            versions: vec![ver("a.jar", Some("https://cdn/x.jar"), None)],
        };
        let lock = find_package("http://x/index", &[pkg], "alpha").expect("found");
        assert_eq!(lock.identity.name, "alpha");
        assert_eq!(lock.steps.len(), 1);
        assert_eq!(lock.steps[0].op, "mod.install");
        let dl = lock.steps[0]
            .args
            .get("download_url")
            .and_then(|v| v.as_str());
        assert_eq!(dl, Some("https://cdn/x.jar"));
    }

    #[test]
    fn find_package_uses_blob_ref_when_no_download_url() {
        let pkg = SourcePackage {
            id: "beta".to_owned(),
            title: "Beta".to_owned(),
            description: None,
            versions: vec![ver("b.jar", None, Some("beta-blob"))],
        };
        let lock =
            find_package("http://127.0.0.1:9999/api/source/index", &[pkg], "beta").expect("found");
        let url = lock.steps[0]
            .args
            .get("download_url")
            .and_then(|v| v.as_str())
            .expect("url");
        assert_eq!(url, "http://127.0.0.1:9999/api/source/blob/beta-blob");
    }

    #[test]
    fn find_package_returns_none_when_no_versions() {
        let pkg = SourcePackage {
            id: "empty".to_owned(),
            title: "Empty".to_owned(),
            description: None,
            versions: vec![],
        };
        assert!(find_package("http://x/index", &[pkg], "empty").is_none());
    }
}

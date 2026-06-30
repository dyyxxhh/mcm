//! `SourceProvider` — a [`Provider`] backed by a manually imported source
//! index.
//!
//! The provider lazy-fetches the index from the source URL on first use,
//! parses it through the boundary parser ([`crate::source_index::parse_source_index`]),
//! and resolves projects/artifacts from the in-memory catalog. Downloads from
//! source-hosted blob references are verified against the declared sha256
//! hash. External `download_url` entries are passed through verbatim (the
//! existing `validate_download_url` allowlist applies at the install layer).
//!
//! Actions declared in the index metadata are parsed and stored on the
//! [`SourceIndex`] but are NEVER auto-executed by this provider.

use anyhow::{Context, Result};
use std::sync::Mutex;

use crate::config::{Profile, Side};
use crate::provider::{Artifact, Candidate, Dependency, DependencyKind, Project, Provider};
use crate::source_index::{SourceDependency, SourceIndex, SourcePackage, SourceVersion};

/// Provider backed by a manually imported source index. Standalone API —
/// not yet wired into the composite provider dispatch (future task). The
/// `#[allow(dead_code)]` mirrors the future-task API pattern from
/// `confirmation.rs` (task 7).
#[allow(dead_code)]
pub(crate) struct SourceProvider {
    index_url: String,
    index: Mutex<Option<SourceIndex>>,
    client: reqwest::blocking::Client,
}

#[allow(dead_code)]
impl SourceProvider {
    pub(crate) fn new(index_url: &str) -> Self {
        Self {
            index_url: index_url.to_owned(),
            index: Mutex::new(None),
            client: reqwest::blocking::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("source HTTP client"),
        }
    }

    /// Build a `SourceProvider` from an already-parsed index (used by tests
    /// and by callers that fetch+parse independently, e.g. `source info`).
    pub(crate) fn with_index(index_url: &str, index: SourceIndex) -> Self {
        Self {
            index_url: index_url.to_owned(),
            index: Mutex::new(Some(index)),
            client: reqwest::blocking::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("source HTTP client"),
        }
    }

    /// Lazy-fetch and parse the index on first access. Uses a `Mutex` so the
    /// fallible fetch+parse can run under `&self` (the `Provider` trait
    /// requires `&self`). On success the parsed index is cached; subsequent
    /// calls return the cached value without re-fetching.
    fn index(&self) -> Result<SourceIndex> {
        let mut guard = self.index.lock().expect("source index mutex poisoned");
        if let Some(ref index) = *guard {
            return Ok(index.clone());
        }
        let body = self
            .client
            .get(&self.index_url)
            .header("User-Agent", "mcm/0.1.0 (Minecraft mod manager)")
            .send()
            .with_context(|| format!("fetch source index {}", self.index_url))?
            .error_for_status()
            .with_context(|| format!("source index {} returned error", self.index_url))?
            .text()
            .context("read source index body")?;
        let parsed = crate::source_index::parse_source_index(&body)?;
        *guard = Some(parsed.clone());
        Ok(parsed)
    }

    /// Resolve a blob reference to an absolute URL relative to the index URL.
    fn blob_url(&self, blob_ref: &str) -> Result<String> {
        let base = self.index_url.rsplit_once('/').map(|(head, _)| head);
        let base = base.unwrap_or(&self.index_url);
        Ok(format!("{base}/blobs/{blob_ref}"))
    }

    /// Fetch bytes from a URL via the provider's blocking client.
    fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let response = self
            .client
            .get(url)
            .header("User-Agent", "mcm/0.1.0 (Minecraft mod manager)")
            .send()
            .with_context(|| format!("fetch artifact {url}"))?
            .error_for_status()
            .with_context(|| format!("artifact {url} returned error"))?;
        Ok(response.bytes()?.to_vec())
    }
}

impl Provider for SourceProvider {
    fn search(&self, query: &str, _profile: &Profile) -> Result<Vec<Project>> {
        let index = self.index()?;
        let q = query.to_lowercase();
        let mut out = Vec::new();
        for pkg in &index.packages {
            if pkg.id.contains(&q) || pkg.title.to_lowercase().contains(&q) {
                out.push(project_from_package(pkg));
            }
        }
        Ok(out)
    }

    fn get(&self, query: &str, _profile: &Profile) -> Result<Project> {
        let index = self.index()?;
        index
            .packages
            .iter()
            .find(|pkg| pkg.id == query)
            .map(project_from_package)
            .with_context(|| format!("project {query} not found in source index"))
    }

    fn download(&self, artifact: &Artifact) -> Result<Vec<u8>> {
        let url: String = if let Some(dl) = artifact.download_url.as_deref() {
            dl.to_owned()
        } else {
            let blob_ref = artifact
                .file_id
                .strip_prefix("blob:")
                .with_context(|| format!("no download URL or blob ref for {}", artifact.file_id))?;
            self.blob_url(blob_ref)?
        };
        let bytes = self.fetch_bytes(&url)?;
        // Hash verification on every download — a present hash MUST match.
        if let Some(expected) = artifact.sha256.as_deref() {
            let actual = crate::util::sha256_hex(&bytes);
            if actual != expected {
                anyhow::bail!(
                    "hash mismatch for {}: expected {expected}, got {actual}",
                    artifact.filename
                );
            }
        }
        Ok(bytes)
    }
}

// ---------------------------------------------------------------------------
// Mapping helpers
// ---------------------------------------------------------------------------

#[allow(dead_code)]
fn project_from_package(pkg: &SourcePackage) -> Project {
    Project {
        logical_id: pkg.id.clone(),
        title: pkg.title.clone(),
        description: pkg.description.clone().unwrap_or_default(),
        candidates: vec![Candidate {
            provider: "source".to_owned(),
            project_id: pkg.id.clone(),
            artifacts: pkg.versions.iter().map(artifact_from_version).collect(),
        }],
    }
}

#[allow(dead_code)]
fn artifact_from_version(ver: &SourceVersion) -> Artifact {
    let file_id = ver
        .blob_ref
        .as_deref()
        .map(|r| format!("blob:{r}"))
        .unwrap_or_else(|| format!("source:{}", ver.filename));
    Artifact {
        file_id,
        version: ver.version.clone(),
        release: crate::provider::ReleaseKind::Stable,
        mc_versions: ver.mc_versions.clone(),
        loaders: ver.loaders.clone(),
        side: side_from_str(ver.side.as_deref()),
        filename: ver.filename.clone(),
        download_url: ver.download_url.clone(),
        sha256: ver.sha256.clone(),
        download_count: ver.size,
        deps: ver.deps.iter().map(dependency_from_source).collect(),
        owner_id: None,
    }
}

#[allow(dead_code)]
fn side_from_str(side: Option<&str>) -> Side {
    match side {
        Some("client") => Side::Client,
        Some("server") => Side::Server,
        _ => Side::Both,
    }
}

#[allow(dead_code)]
fn dependency_from_source(dep: &SourceDependency) -> Dependency {
    Dependency {
        logical_id: dep.id.clone(),
        kind: match dep.kind.as_str() {
            "required" => DependencyKind::Required,
            "optional" => DependencyKind::Optional,
            "embedded" => DependencyKind::Embedded,
            "incompatible" => DependencyKind::Incompatible,
            _ => DependencyKind::Unknown,
        },
    }
}

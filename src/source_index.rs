// SIZE_OK: non-test source is 130 LOC; the rest is the `#[cfg(test)] mod
// tests` block (parse/validate boundary tests) which stays with the code it
// exercises.
//! Schema-versioned source index format and boundary parser.
//!
//! A source index is JSON fetched from a manually imported source URL. It
//! declares the source identity, capabilities (`mods`, `packages`, `games`,
//! `loaders`, `java`), and a catalog of packages with versions/hashes/sizes/
//! compatibility. Optionally it may declare actions — these are parsed and
//! stored but NEVER auto-executed by MCM.
//!
//! Untrusted JSON is parsed once at the boundary into typed values, mirroring
//! the [`crate::mcm_package`] boundary discipline: size/depth/secret checks
//! run on a `serde_json::Value` before typed deserialization.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;
const MAX_JSON_BYTES: usize = 10 * 1024 * 1024;
const MAX_DEPTH: usize = 64;

/// Case-insensitive substrings that mark a key as secret. Same markers as
/// `mcm_package.rs` so the trust boundary is uniform.
const SECRET_MARKERS: &[&str] = &["token", "secret", "password", "credential", "api_key"];

// ---------------------------------------------------------------------------
// Schema types
// ---------------------------------------------------------------------------

/// A parsed source index. All fields are typed — no `serde_json::Value`
/// leaks into domain logic. `actions` is parsed and stored but never
/// auto-executed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceIndex {
    pub schema_version: u32,
    pub source_id: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub packages: Vec<SourcePackage>,
    /// Parsed and stored only — MCM never auto-executes declared actions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<SourceAction>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourcePackage {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub versions: Vec<SourceVersion>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceVersion {
    pub version: String,
    #[serde(default)]
    pub mc_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    pub filename: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    /// Blob reference for source-hosted artifacts. Resolved relative to the
    /// index URL as `{index_base}/blobs/{blob_ref}` by the provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default)]
    pub deps: Vec<SourceDependency>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceDependency {
    pub id: String,
    /// Kind string mapping to `DependencyKind` (`required`/`optional`/
    /// `embedded`/`incompatible`/`unknown`).
    pub kind: String,
}

/// A declared action from index metadata. Parsed and stored only — MCM
/// NEVER auto-executes actions declared in a source index.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceAction {
    pub kind: String,
    pub description: String,
}

// ---------------------------------------------------------------------------
// Boundary parser
// ---------------------------------------------------------------------------

/// Parse a source index JSON document into a typed [`SourceIndex`].
///
/// Enforces: size (≤ 10 MB), nesting depth (≤ 64), secret-field rejection,
/// schema version (currently 1), and source-id normalization. This is the
/// single boundary — callers receive typed values and never touch raw
/// `serde_json::Value`.
pub fn parse_source_index(json: &str) -> Result<SourceIndex> {
    if json.len() > MAX_JSON_BYTES {
        bail!("source index JSON exceeds {MAX_JSON_BYTES} bytes");
    }
    let value: serde_json::Value =
        serde_json::from_str(json).context("invalid source index JSON")?;
    let depth = json_depth(&value);
    if depth > MAX_DEPTH {
        bail!("source index JSON nesting depth {depth} exceeds {MAX_DEPTH}");
    }
    scan_for_secrets(&value)?;
    let index: SourceIndex =
        serde_json::from_value(value).context("source index schema mismatch")?;
    if index.schema_version != SCHEMA_VERSION {
        bail!(
            "unsupported source index schema version {}",
            index.schema_version
        );
    }
    validate_source_id(&index.source_id)?;
    Ok(index)
}

/// Validate that a source id is in normalized form: lowercase ASCII
/// `[a-z0-9-]`, 1–64 chars, starts/ends alphanumeric, no consecutive hyphens.
/// Mirrors the package-name normalization in `mcm_package.rs` minus the
/// reserved-name exclusion (source ids are not filenames).
pub fn validate_source_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 64 {
        bail!("source_id must be 1-64 characters");
    }
    if !id.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-')) {
        bail!("source_id must contain only [a-z0-9-]");
    }
    let first = id.chars().next();
    let last = id.chars().last();
    if !first.is_some_and(|c| c.is_ascii_alphanumeric())
        || !last.is_some_and(|c| c.is_ascii_alphanumeric())
    {
        bail!("source_id must start and end with an alphanumeric character");
    }
    if id.contains("--") {
        bail!("source_id must not contain consecutive hyphens");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Fetch + URL helpers
// ---------------------------------------------------------------------------

/// Fetch and parse a source index from a URL. Uses a blocking reqwest client
/// with no redirect following so a malformed remote cannot silently redirect
/// to a different host. Reused by `source info` and `pkg install` source
/// resolution.
pub fn fetch_source_index(url: &str) -> Result<SourceIndex> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("build HTTP client")?;
    let body = client
        .get(url)
        .header("User-Agent", "mcm/0.1.0 (Minecraft mod manager)")
        .send()
        .with_context(|| format!("fetch source index {url}"))?
        .error_for_status()
        .with_context(|| format!("source index {url} returned error status"))?
        .text()
        .context("read source index body")?;
    parse_source_index(&body)
}

/// Resolve a blob reference to an absolute URL relative to the index URL.
/// The blob endpoint path is `/blob/{blob_ref}` (singular), matching the
/// source service's `GET /api/source/blob/{slug}` route.
pub fn source_blob_url(index_url: &str, blob_ref: &str) -> String {
    let base = index_url
        .rsplit_once('/')
        .map(|(head, _)| head)
        .unwrap_or(index_url);
    format!("{base}/blob/{blob_ref}")
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Recursively scan a JSON value for keys that look like secret fields.
fn scan_for_secrets(value: &serde_json::Value) -> Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                let lower = key.to_ascii_lowercase();
                if SECRET_MARKERS.iter().any(|m| lower.contains(m)) {
                    bail!("source index contains secret field: {key}");
                }
                scan_for_secrets(val)?;
            }
        }
        serde_json::Value::Array(arr) => {
            for val in arr {
                scan_for_secrets(val)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Maximum nesting depth of a JSON value (scalar = 0, object/array = 1 + max child).
fn json_depth(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(map) => map.values().map(json_depth).max().unwrap_or(0) + 1,
        serde_json::Value::Array(arr) => arr.iter().map(json_depth).max().unwrap_or(0) + 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_MINIMAL: &str = r#"{
        "schema_version": 1,
        "source_id": "my-source",
        "packages": []
    }"#;

    const VALID_FULL: &str = r#"{
        "schema_version": 1,
        "source_id": "trusted-source",
        "capabilities": ["mods", "packages"],
        "packages": [
            {
                "id": "coolmod",
                "title": "Cool Mod",
                "description": "A cool mod",
                "versions": [
                    {
                        "version": "1.0.0",
                        "mc_versions": ["1.20.1"],
                        "loaders": ["fabric"],
                        "side": "both",
                        "filename": "coolmod-1.0.0.jar",
                        "download_url": "https://cdn.modrinth.com/data/coolmod/1.0.0.jar",
                        "sha256": "abc123",
                        "size": 12345,
                        "deps": [{"id": "fabric-api", "kind": "required"}]
                    }
                ]
            }
        ],
        "actions": [{"kind": "shell", "description": "post-install hook"}]
    }"#;

    #[test]
    fn parses_valid_minimal_index() {
        let index = parse_source_index(VALID_MINIMAL).expect("minimal index");
        assert_eq!(index.schema_version, 1);
        assert_eq!(index.source_id, "my-source");
        assert!(index.packages.is_empty());
        assert!(index.actions.is_none());
    }

    #[test]
    fn parses_valid_full_index_with_capabilities_and_actions() {
        let index = parse_source_index(VALID_FULL).expect("full index");
        assert_eq!(index.source_id, "trusted-source");
        assert_eq!(index.capabilities, vec!["mods", "packages"]);
        assert_eq!(index.packages.len(), 1);
        let pkg = &index.packages[0];
        assert_eq!(pkg.id, "coolmod");
        assert_eq!(pkg.title, "Cool Mod");
        assert_eq!(pkg.versions.len(), 1);
        let ver = &pkg.versions[0];
        assert_eq!(ver.version, "1.0.0");
        assert_eq!(ver.filename, "coolmod-1.0.0.jar");
        assert_eq!(ver.sha256.as_deref(), Some("abc123"));
        assert_eq!(ver.size, Some(12345));
        assert_eq!(ver.deps.len(), 1);
        assert_eq!(ver.deps[0].id, "fabric-api");
        assert_eq!(ver.deps[0].kind, "required");
        // Actions parsed but never executed — we only store them.
        let actions = index.actions.as_ref().expect("actions present");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind, "shell");
    }

    #[test]
    fn rejects_unknown_schema_version() {
        let json = r#"{"schema_version": 2, "source_id": "x", "packages": []}"#;
        let err = parse_source_index(json).unwrap_err();
        assert!(format!("{err}").contains("schema version 2"));
    }

    #[test]
    fn rejects_missing_required_fields() {
        let json = r#"{"schema_version": 1}"#;
        assert!(parse_source_index(json).is_err());
    }

    #[test]
    fn rejects_secret_field_at_top_level() {
        let json = r#"{"schema_version": 1, "source_id": "x", "packages": [], "api_key": "leak"}"#;
        let err = parse_source_index(json).unwrap_err();
        assert!(format!("{err}").contains("secret field"));
    }

    #[test]
    fn rejects_secret_field_nested_in_package() {
        let json = r#"{
            "schema_version": 1, "source_id": "x", "packages": [
                {"id": "m", "title": "M", "versions": [], "token": "leak"}
            ]
        }"#;
        let err = parse_source_index(json).unwrap_err();
        assert!(format!("{err}").contains("secret field"));
    }

    #[test]
    fn rejects_oversized_index() {
        let mut json = String::from(r#"{"schema_version": 1, "source_id": "x", "packages": ["#);
        // Push past 10 MB.
        let pad = "1,".repeat(MAX_JSON_BYTES);
        json.push_str(&pad);
        json.push_str("null]}");
        let err = parse_source_index(&json).unwrap_err();
        assert!(format!("{err}").contains("exceeds"));
    }

    #[test]
    fn rejects_excessive_depth() {
        let mut json = String::new();
        for _ in 0..(MAX_DEPTH + 1) {
            json.push_str("{\"a\":");
        }
        json.push('1');
        for _ in 0..(MAX_DEPTH + 1) {
            json.push('}');
        }
        // Embed in a valid-ish wrapper so size is fine but depth is too high.
        let wrapped =
            format!(r#"{{"schema_version": 1, "source_id": "x", "packages": [], "deep": {json}}}"#);
        let err = parse_source_index(&wrapped).unwrap_err();
        assert!(format!("{err}").contains("depth"));
    }

    #[test]
    fn rejects_malformed_json() {
        let json = "{ not valid json";
        assert!(parse_source_index(json).is_err());
    }

    #[test]
    fn rejects_invalid_source_id_uppercase() {
        let json = r#"{"schema_version": 1, "source_id": "BadID", "packages": []}"#;
        assert!(parse_source_index(json).is_err());
    }

    #[test]
    fn rejects_invalid_source_id_consecutive_hyphens() {
        let json = r#"{"schema_version": 1, "source_id": "a--b", "packages": []}"#;
        assert!(parse_source_index(json).is_err());
    }

    #[test]
    fn validates_source_id_boundary_cases() {
        assert!(validate_source_id("a").is_ok());
        assert!(validate_source_id("my-source-1").is_ok());
        assert!(validate_source_id("").is_err());
        assert!(validate_source_id("-ab").is_err());
        assert!(validate_source_id("ab-").is_err());
        assert!(validate_source_id("UPPER").is_err());
        assert!(validate_source_id("has space").is_err());
        assert!(validate_source_id(&"a".repeat(65)).is_err());
    }
}

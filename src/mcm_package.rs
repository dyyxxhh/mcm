//! Schema-versioned types for `.mcm` v2 JSON locks, the boundary parser,
//! path/permission validation, and dyyl source generation. All `.mcm` format
//! concerns live here: types, parsing, validation, and export.
// allow: SIZE_OK — single-module ownership of the `.mcm` format; splitting
// would create awkward cross-module type dependencies.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 2;
const MAX_JSON_BYTES: usize = 10 * 1024 * 1024;
const MAX_DEPTH: usize = 64;

/// Case-insensitive substrings that mark a key as secret.
const SECRET_MARKERS: &[&str] = &["token", "secret", "password", "credential", "api_key"];

// ---------------------------------------------------------------------------
// v2 lock schema types
// ---------------------------------------------------------------------------

/// A parsed `.mcm` v2 JSON lock. This is the shared package file format and
/// installable lock format produced by `mcm build`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McmLock {
    pub schema_version: u32,
    pub kind: String,
    pub identity: LockIdentity,
    #[serde(default)]
    pub author: LockAuthor,
    pub permissions: LockPermissions,
    #[serde(default)]
    pub game: Option<LockGame>,
    #[serde(default)]
    pub steps: Vec<LockStep>,
    #[serde(default)]
    pub artifacts: Vec<LockArtifact>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LockIdentity {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LockAuthor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LockPermissions {
    pub install: bool,
    #[serde(rename = "do", default)]
    pub do_permitted: bool,
    #[serde(default)]
    pub full: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LockGame {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LockStep {
    pub op: String,
    pub permission: StepPermission,
    /// Operation-specific arguments. Domain logic must not interpret raw
    /// `Value` — step executors destructure the relevant fields.
    #[serde(default)]
    pub args: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_line: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StepPermission {
    Install,
    Do,
    Full,
}

impl StepPermission {
    pub fn is_install_permitted(self) -> bool {
        matches!(self, StepPermission::Install)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            StepPermission::Install => "install",
            StepPermission::Do => "do",
            StepPermission::Full => "full",
        }
    }
}

impl std::fmt::Display for StepPermission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LockArtifact {
    pub id: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

// ---------------------------------------------------------------------------
// Boundary parser
// ---------------------------------------------------------------------------

/// Parse a `.mcm` v2 JSON lock into a typed [`McmLock`].
///
/// Enforces: size (≤ 10 MB), nesting depth (≤ 64), secret-field rejection,
/// schema version (exactly 2), kind ("mcm-lock"), package-name normalization,
/// and step permission validation. This is the single boundary — callers
/// receive typed values and never touch raw [`serde_json::Value`].
pub fn parse_mcm_lock(json: &str) -> Result<McmLock> {
    if json.len() > MAX_JSON_BYTES {
        bail!("lock JSON exceeds {MAX_JSON_BYTES} bytes");
    }
    let value: serde_json::Value = serde_json::from_str(json).context("invalid lock JSON")?;
    let depth = json_depth(&value);
    if depth > MAX_DEPTH {
        bail!("lock JSON nesting depth {depth} exceeds {MAX_DEPTH}");
    }
    scan_for_secrets(&value)?;
    if let Some(v) = value.get("schema_version").and_then(|v| v.as_u64()) {
        if v == 1 {
            bail!(
                "v1 .mcm files are no longer supported; \
                 rebuild from dyyl source with `mcm build <file.dyyl>`"
            );
        }
        if v != SCHEMA_VERSION as u64 {
            bail!("unsupported schema version {v}");
        }
    }
    let lock: McmLock = serde_json::from_value(value).context("lock schema mismatch")?;
    if lock.kind != "mcm-lock" {
        bail!("expected kind \"mcm-lock\", got \"{}\"", lock.kind);
    }
    validate_package_name(&lock.identity.name)?;
    validate_lock_step_paths(&lock)?;
    Ok(lock)
}

/// Legacy entry point that rejects v1 with actionable error.
///
/// Any code that previously called `parse_mcm_package` now goes through
/// `parse_mcm_lock`. This wrapper ensures v1 files produce a clear
/// "rebuild from dyyl" error.
pub fn parse_mcm_package(json: &str) -> Result<McmLock> {
    let value: serde_json::Value = serde_json::from_str(json).context("invalid JSON")?;
    if let Some(v) = value.get("schema_version").and_then(|v| v.as_u64()) {
        if v == 1 {
            bail!(
                "v1 .mcm files are no longer supported; \
                 rebuild from dyyl source with `mcm build <file.dyyl>`"
            );
        }
    }
    parse_mcm_lock(json)
}

#[allow(dead_code)]
pub fn all_steps_install_only(lock: &McmLock) -> bool {
    lock.steps
        .iter()
        .all(|s| s.permission.is_install_permitted())
}

#[allow(dead_code)]
pub fn install_permitted_steps(lock: &McmLock) -> Vec<&LockStep> {
    lock.steps
        .iter()
        .filter(|s| s.permission.is_install_permitted())
        .collect()
}

// ---------------------------------------------------------------------------
// Validation helpers (shared by lock parser and server validation)
// ---------------------------------------------------------------------------

/// Validate that a package name is in normalized form: lowercase ASCII
/// `[a-z0-9-]`, 1–64 chars, starts/ends alphanumeric, no consecutive hyphens,
/// not a reserved name.
pub fn validate_package_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 64 {
        bail!("package name must be 1-64 characters");
    }
    if !name
        .chars()
        .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-'))
    {
        bail!("package name must contain only [a-z0-9-]");
    }
    let first = name.chars().next();
    let last = name.chars().last();
    if !first.is_some_and(|c| c.is_ascii_alphanumeric())
        || !last.is_some_and(|c| c.is_ascii_alphanumeric())
    {
        bail!("package name must start and end with an alphanumeric character");
    }
    if name.contains("--") {
        bail!("package name must not contain consecutive hyphens");
    }
    if is_reserved_package_name(name) {
        bail!("package name {name} is reserved");
    }
    Ok(())
}

/// Reserved package names: `mcm` plus Windows reserved names.
fn is_reserved_package_name(name: &str) -> bool {
    name == "mcm" || is_windows_reserved_stem(name)
}

/// Check if a name (possibly with an extension) has a Windows-reserved stem.
fn is_windows_reserved_stem(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || upper
            .strip_prefix("COM")
            .and_then(|s| s.parse::<u8>().ok())
            .is_some_and(|n| (1..=9).contains(&n))
        || upper
            .strip_prefix("LPT")
            .and_then(|s| s.parse::<u8>().ok())
            .is_some_and(|n| (1..=9).contains(&n))
}

/// Validate an asset path: no empty, null bytes, `..`, absolute paths,
/// backslashes, or Windows-reserved path components.
pub fn validate_asset_path(path: &str) -> Result<()> {
    if path.is_empty() || path.contains('\0') {
        bail!("asset path must not be empty or contain null bytes");
    }
    if path.contains("..") || path.starts_with('/') || path.contains('\\') {
        bail!("asset path must not be absolute or traverse: {path}");
    }
    for component in path.split('/') {
        if is_windows_reserved_stem(component) {
            bail!("asset path component {component} is a reserved name");
        }
    }
    Ok(())
}

/// Validate a step destination path for `file.copy`, `file.write`, and
/// `net.download`. Paths must be version-root relative: no empty, no null
/// bytes, no `..` traversal, no absolute paths, no backslashes.
pub fn validate_step_dest_path(path: &str) -> Result<()> {
    validate_asset_path(path)
}

/// Validate that a lock is upload-safe (install-only) for server sharing.
/// Rejects locks containing any step with `do` or `full` permission.
pub fn validate_lock_install_only(lock: &McmLock) -> Result<()> {
    for step in &lock.steps {
        if !step.permission.is_install_permitted() {
            bail!(
                "non-install step (permission: {}, op: {}) is not allowed in shared packages; \
                 use `mcm do` for full-power execution",
                step.permission,
                step.op
            );
        }
    }
    Ok(())
}

/// Validate all path arguments in lock steps. For `file.copy`, `file.write`,
/// and `net.download`, the `dest` argument must be version-root relative.
/// For `net.download`, the `url` must be non-empty.
pub fn validate_lock_step_paths(lock: &McmLock) -> Result<()> {
    for step in &lock.steps {
        match step.op.as_str() {
            "file.copy" | "file.write" => {
                if let Some(dest) = step.args.get("dest").and_then(|v| v.as_str()) {
                    validate_step_dest_path(dest)?;
                }
            }
            "net.download" => {
                if let Some(dest) = step.args.get("dest").and_then(|v| v.as_str()) {
                    validate_step_dest_path(dest)?;
                }
                if let Some(url) = step.args.get("url").and_then(|v| v.as_str()) {
                    if url.is_empty() {
                        bail!("net.download step requires a non-empty url");
                    }
                } else {
                    bail!("net.download step is missing required 'url' argument");
                }
            }
            "shell.run" => {
                // shell.run cwd must be version-root relative if specified.
                if let Some(cwd) = step.args.get("cwd").and_then(|v| v.as_str()) {
                    validate_step_dest_path(cwd)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Recursively scan a JSON value for keys that look like secret fields.
/// Shared by `.mcm` and standard modpack format parsers (`modpack_import`).
pub(crate) fn scan_for_secrets(value: &serde_json::Value) -> Result<()> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                let lower = key.to_ascii_lowercase();
                if SECRET_MARKERS.iter().any(|m| lower.contains(m)) {
                    bail!("package contains secret field: {key}");
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

// ---------------------------------------------------------------------------
// dyyl source generation (for `mcm make`)
// ---------------------------------------------------------------------------

/// Generate dyyl source text from a v2 lock.
///
/// `mcm make <out.dyyl>` calls this to export the current instance state
/// as human-readable dyyl source. Each step becomes a dyyl command line.
pub fn lock_to_dyyl(lock: &McmLock) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# dyyl source exported by mcm make\n# name: {}\n# version: {}\n\n",
        lock.identity.name, lock.identity.version
    ));
    if let Some(game) = &lock.game {
        if let Some(name) = &game.game {
            let version_str = game
                .version
                .as_deref()
                .map(|v| format!(", {v}"))
                .unwrap_or_default();
            let loader_str = game
                .loader
                .as_deref()
                .map(|l| format!(", {l}"))
                .unwrap_or_default();
            out.push_str(&format!(
                "mcm.game.choose(\"{name}\"{version_str}{loader_str});\n"
            ));
        }
    }
    for step in &lock.steps {
        let args_str = match &step.args {
            serde_json::Value::Object(map) => {
                let pairs: Vec<String> = map
                    .iter()
                    .map(|(k, v)| {
                        let val_str = match v {
                            serde_json::Value::String(s) => format!("\"{s}\""),
                            other => format!("{other}"),
                        };
                        format!("{k}: {val_str}")
                    })
                    .collect();
                pairs.join(", ")
            }
            serde_json::Value::Null => String::new(),
            other => format!("{other}"),
        };
        if let Some(src) = &step.source_line {
            out.push_str(&format!("# {src}\n"));
        }
        out.push_str(&format!("{}({args_str});\n", step.op));
    }
    out
}

// ---------------------------------------------------------------------------
// v2 lock construction helpers (for `mcm build` and `mcm make`)
// ---------------------------------------------------------------------------

/// Create an empty v2 lock with the given identity.
pub fn new_lock(name: &str, version: &str) -> McmLock {
    McmLock {
        schema_version: SCHEMA_VERSION,
        kind: "mcm-lock".to_owned(),
        identity: LockIdentity {
            name: name.to_owned(),
            version: version.to_owned(),
            description: None,
        },
        author: LockAuthor::default(),
        permissions: LockPermissions {
            install: true,
            do_permitted: false,
            full: false,
        },
        game: None,
        steps: Vec::new(),
        artifacts: Vec::new(),
        created_at: now_rfc3339(),
        generator: Some("mcm".to_owned()),
    }
}

/// Create a lock step.
pub fn new_step(op: &str, permission: StepPermission, args: serde_json::Value) -> LockStep {
    LockStep {
        op: op.to_owned(),
        permission,
        args,
        source_line: None,
    }
}

/// Format current UTC time as RFC 3339.
pub(crate) fn now_rfc3339() -> String {
    let dt = time::OffsetDateTime::now_utc();
    let format = time::format_description::well_known::Rfc3339;
    dt.format(&format)
        .unwrap_or_else(|_| format!("{}T00:00:00Z", dt.date()))
}

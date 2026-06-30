//! Server configuration loaded from environment variables.
//!
//! PM2-friendly: all config comes from the environment so the process can be
//! managed by PM2 without flags beyond the CLI `--mode` / `--bind`. Validation
//! here is config-shape only — actual storage init is task 13, OIDC is task 14.

use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

/// The three server modes. Source and share route sets are independently
/// enabled/disabled by mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ServeMode {
    /// Public download of `.mcm` packages + authenticated publish/update/delete.
    Share,
    /// Serve a manually imported source index and artifact blobs.
    Source,
    /// Both route sets enabled in one process.
    Both,
}

impl ServeMode {
    pub(crate) fn share_enabled(self) -> bool {
        matches!(self, ServeMode::Share | ServeMode::Both)
    }

    pub(crate) fn source_enabled(self) -> bool {
        matches!(self, ServeMode::Source | ServeMode::Both)
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ServeMode::Share => "share",
            ServeMode::Source => "source",
            ServeMode::Both => "both",
        }
    }
}

/// Authentication mode: mock (no network, for tests/dev) or real OIDC.
///
/// Determined by `MCM_AUTH_MODE` env var. Default is `Mock` — safe by default;
/// a missing env var never triggers a real OIDC flow. Set `MCM_AUTH_MODE=real`
/// and provide all four `MCM_OIDC_*` vars for production.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AuthMode {
    Mock,
    Real,
}

impl AuthMode {
    pub(crate) fn is_real(self) -> bool {
        matches!(self, AuthMode::Real)
    }
}

/// Secret string whose `Debug` impl is redacted. Wraps OIDC client secrets
/// (and any other sensitive material) so a stray `eprintln!("{state:?}")`
/// can never leak them. The inner value is accessed only via [`Self::as_str`].
#[derive(Clone)]
pub(crate) struct SecretString(String);

impl SecretString {
    pub(crate) fn new(s: String) -> Self {
        Self(s)
    }
    #[allow(dead_code, reason = "used by real OIDC token exchange (not mock mode)")]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Server configuration parsed from the environment. The server is PM2-managed
/// and reads everything it needs from env vars — no interactive prompts.
///
/// # Validation
/// - `MCM_SHARE_DATA_DIR` MUST NOT be under `/x` (per plan: refuse to start).
/// - OIDC fields are read through only; actual OIDC logic is task 14.
///
/// `Debug` is implemented manually so the OIDC client secret (a `SecretString`)
/// is rendered as `<redacted>` and can never leak through `eprintln!`,
/// `tracing`, or error chains.
pub(crate) struct ServerConfig {
    /// Where package blobs live. Default `/var/lib/mcm-share`. Read by
    /// `Storage::open` in task 13 to create the DB + blobs dir.
    pub(crate) data_dir: PathBuf,
    /// Directory containing `index.html`, `app.js`, `styles.css`.
    /// Resolved by [`resolve_web_dir`]: `MCM_WEB_DIR` env var, then
    /// ancestor walk from the binary, then cwd-relative `web/` fallback.
    pub(crate) web_dir: PathBuf,
    /// Mock (default) or real OIDC. Determined by `MCM_AUTH_MODE`.
    pub(crate) auth_mode: AuthMode,
    /// OIDC issuer base URL (e.g. `https://auth.dyyapp.com`).
    pub(crate) oidc_issuer: Option<String>,
    /// OIDC client id.
    pub(crate) oidc_client_id: Option<String>,
    /// OIDC client secret. NEVER logged — wrapped in `SecretString` whose
    /// `Debug` impl is `<redacted>`.
    pub(crate) oidc_client_secret: Option<SecretString>,
    /// OIDC redirect URL (e.g. `https://mc.dyyapp.com/api/auth/oidc/callback`).
    pub(crate) oidc_redirect_url: Option<String>,
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerConfig")
            .field("data_dir", &self.data_dir)
            .field("web_dir", &self.web_dir)
            .field("auth_mode", &self.auth_mode)
            .field("oidc_issuer", &self.oidc_issuer)
            .field("oidc_client_id", &self.oidc_client_id)
            .field("oidc_client_secret", &self.oidc_client_secret)
            .field("oidc_redirect_url", &self.oidc_redirect_url)
            .finish()
    }
}

impl ServerConfig {
    /// Load from environment, validating the data-dir constraint.
    ///
    /// - `MCM_SHARE_DATA_DIR` (default `/var/lib/mcm-share`).
    /// - `MCM_OIDC_ISSUER`, `MCM_OIDC_CLIENT_ID`, `MCM_OIDC_CLIENT_SECRET`.
    pub(crate) fn from_env() -> Result<Self> {
        let data_dir = env::var_os("MCM_SHARE_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/var/lib/mcm-share"));
        Self::validate_data_dir(&data_dir)?;
        let web_dir = resolve_web_dir();
        let auth_mode = parse_auth_mode()?;
        let oidc_issuer = env::var("MCM_OIDC_ISSUER").ok();
        let oidc_client_id = env::var("MCM_OIDC_CLIENT_ID").ok();
        let oidc_client_secret = env::var("MCM_OIDC_CLIENT_SECRET")
            .ok()
            .map(SecretString::new);
        let oidc_redirect_url = env::var("MCM_OIDC_REDIRECT_URL").ok();
        let config = Self {
            data_dir,
            web_dir,
            auth_mode,
            oidc_issuer,
            oidc_client_id,
            oidc_client_secret,
            oidc_redirect_url,
        };
        config.validate_oidc()?;
        log_oidc_presence(&config);
        Ok(config)
    }

    /// Refuse to start if the data directory is under `/x`. The plan requires
    /// server package/blob storage to default outside `/x`. This is a
    /// config-shape check only — creating the dir is task 13.
    fn validate_data_dir(dir: &std::path::Path) -> Result<()> {
        // Normalize without touching the filesystem: only reject paths that
        // lexically start with `/x` (as an ancestor or exact match). We do not
        // canonicalize because the dir may not exist yet (task 13 creates it).
        let starts_with_x = dir.ancestors().any(|a| a == std::path::Path::new("/x"));
        if starts_with_x {
            return Err(anyhow!(
                "MCM_SHARE_DATA_DIR must not be under /x (got {}); \
                 server storage must live outside /x per the plan",
                dir.display()
            ));
        }
        Ok(())
    }

    /// In real auth mode, all four OIDC fields must be set. Mock mode skips
    /// this check entirely.
    fn validate_oidc(&self) -> Result<()> {
        if !self.auth_mode.is_real() {
            return Ok(());
        }
        let mut missing = Vec::new();
        if self.oidc_issuer.is_none() {
            missing.push("MCM_OIDC_ISSUER");
        }
        if self.oidc_client_id.is_none() {
            missing.push("MCM_OIDC_CLIENT_ID");
        }
        if self.oidc_client_secret.is_none() {
            missing.push("MCM_OIDC_CLIENT_SECRET");
        }
        if self.oidc_redirect_url.is_none() {
            missing.push("MCM_OIDC_REDIRECT_URL");
        }
        if missing.is_empty() {
            Ok(())
        } else {
            bail!(
                "MCM_AUTH_MODE=real but missing OIDC config: {}. \
                 Set all four MCM_OIDC_* env vars or use MCM_AUTH_MODE=mock",
                missing.join(", ")
            )
        }
    }
}

/// Parse `MCM_AUTH_MODE` env var. Default is `Mock` — safe by default.
fn parse_auth_mode() -> Result<AuthMode> {
    match env::var("MCM_AUTH_MODE") {
        Ok(ref v) if v == "real" => Ok(AuthMode::Real),
        Ok(ref v) if v == "mock" || v.is_empty() => Ok(AuthMode::Mock),
        Ok(v) => bail!("invalid MCM_AUTH_MODE {v:?}; expected mock or real"),
        Err(env::VarError::NotPresent) => Ok(AuthMode::Mock),
        Err(env::VarError::NotUnicode(_)) => {
            bail!("MCM_AUTH_MODE contains non-UTF-8 bytes; expected mock or real")
        }
    }
}

/// Print OIDC config presence to stderr. Values are NEVER logged — only
/// `<present>` or `<missing>` per field. Runs once at startup.
fn log_oidc_presence(config: &ServerConfig) {
    eprintln!(
        "OIDC config: issuer={} client_id={} client_secret={} redirect_url={}",
        opt_presence(&config.oidc_issuer),
        opt_presence(&config.oidc_client_id),
        secret_presence(&config.oidc_client_secret),
        opt_presence(&config.oidc_redirect_url),
    );
}

fn opt_presence(val: &Option<String>) -> &'static str {
    if val.is_some() {
        "<present>"
    } else {
        "<missing>"
    }
}

fn secret_presence(val: &Option<SecretString>) -> &'static str {
    if val.is_some() {
        "<present redacted>"
    } else {
        "<missing>"
    }
}

/// Resolve the directory containing `index.html`, `app.js`, `styles.css`.
///
/// Priority order:
/// 1. `MCM_WEB_DIR` environment variable (explicit override).
/// 2. Walk ancestors of `current_exe()` looking for a `web/` subdirectory.
/// 3. Cwd-relative `web/` fallback — prints a warning because static files
///    will break if the working directory changes (e.g. PM2 `cwd` override).
pub(crate) fn resolve_web_dir() -> PathBuf {
    // 1. Explicit env var.
    if let Ok(dir) = env::var("MCM_WEB_DIR") {
        let path = PathBuf::from(&dir);
        if is_valid_web_dir(&path) {
            return path;
        }
        eprintln!("MCM_WEB_DIR={dir} does not contain index.html — falling back");
    }

    // 2. Walk up from the binary location.
    if let Ok(exe) = env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let mut candidate = exe_dir.to_path_buf();
            // Search up to 5 ancestors (covers target/debug/mcm → repo root).
            for _ in 0..5 {
                let web = candidate.join("web");
                if is_valid_web_dir(&web) {
                    return web;
                }
                if !candidate.pop() {
                    break;
                }
            }
        }
    }

    // 3. Cwd-relative fallback.
    eprintln!(
        "WARNING: web/ directory not found relative to binary; \
         falling back to cwd-relative path. Static files will not be \
         served if the working directory changes (e.g. PM2 cwd override). \
         Set MCM_WEB_DIR to fix."
    );
    PathBuf::from("web")
}

fn is_valid_web_dir(dir: &Path) -> bool {
    dir.join("index.html").is_file()
}

/// Parse a `mode` string from the CLI into a `ServeMode`.
pub(crate) fn parse_mode(s: &str) -> Result<ServeMode> {
    match s {
        "share" => Ok(ServeMode::Share),
        "source" => Ok(ServeMode::Source),
        "both" => Ok(ServeMode::Both),
        other => Err(anyhow!(
            "invalid mode {other:?}; expected share|source|both"
        ))
        .with_context(|| "parse --mode"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_mode_enables_share_only() {
        assert!(ServeMode::Share.share_enabled());
        assert!(!ServeMode::Share.source_enabled());
    }

    #[test]
    fn source_mode_enables_source_only() {
        assert!(!ServeMode::Source.share_enabled());
        assert!(ServeMode::Source.source_enabled());
    }

    #[test]
    fn both_mode_enables_both() {
        assert!(ServeMode::Both.share_enabled());
        assert!(ServeMode::Both.source_enabled());
    }

    #[test]
    fn parse_mode_roundtrips() {
        assert_eq!(parse_mode("share").unwrap(), ServeMode::Share);
        assert_eq!(parse_mode("source").unwrap(), ServeMode::Source);
        assert_eq!(parse_mode("both").unwrap(), ServeMode::Both);
        assert!(parse_mode("nonsense").is_err());
    }

    #[test]
    fn secret_string_debug_is_redacted() {
        let s = SecretString::new("super-secret-value".to_string());
        let dbg = format!("{s:?}");
        assert!(dbg.contains("<redacted>"), "debug: {dbg}");
        assert!(
            !dbg.contains("super-secret-value"),
            "leaked in debug: {dbg}"
        );
    }

    #[test]
    fn server_config_debug_redacts_secret() {
        let cfg = ServerConfig {
            data_dir: PathBuf::from("/tmp/mcm-test"),
            web_dir: PathBuf::from("/tmp/mcm-test/web"),
            auth_mode: AuthMode::Mock,
            oidc_issuer: Some("https://auth.example".to_string()),
            oidc_client_id: Some("cid".to_string()),
            oidc_client_secret: Some(SecretString::new("leak-me".to_string())),
            oidc_redirect_url: Some("https://example.com/callback".to_string()),
        };
        let dbg = format!("{cfg:?}");
        assert!(dbg.contains("<redacted>"), "debug: {dbg}");
        assert!(!dbg.contains("leak-me"), "secret leaked: {dbg}");
        assert!(dbg.contains("auth_mode"), "auth_mode missing: {dbg}");
        assert!(dbg.contains("Mock"), "auth_mode Mock missing: {dbg}");
    }

    #[test]
    fn parse_auth_mode_default_is_mock() {
        let temp = env::var("MCM_AUTH_MODE");
        env::remove_var("MCM_AUTH_MODE");
        let result = parse_auth_mode();
        assert_eq!(result.unwrap(), AuthMode::Mock);
        if let Ok(v) = temp {
            env::set_var("MCM_AUTH_MODE", v);
        }
    }

    #[test]
    fn parse_auth_mode_explicit_mock() {
        let temp = env::var("MCM_AUTH_MODE");
        env::set_var("MCM_AUTH_MODE", "mock");
        assert_eq!(parse_auth_mode().unwrap(), AuthMode::Mock);
        if let Ok(v) = temp {
            env::set_var("MCM_AUTH_MODE", v);
        } else {
            env::remove_var("MCM_AUTH_MODE");
        }
    }

    #[test]
    fn parse_auth_mode_real() {
        let temp = env::var("MCM_AUTH_MODE");
        env::set_var("MCM_AUTH_MODE", "real");
        assert_eq!(parse_auth_mode().unwrap(), AuthMode::Real);
        if let Ok(v) = temp {
            env::set_var("MCM_AUTH_MODE", v);
        } else {
            env::remove_var("MCM_AUTH_MODE");
        }
    }

    #[test]
    fn parse_auth_mode_invalid() {
        let temp = env::var("MCM_AUTH_MODE");
        env::set_var("MCM_AUTH_MODE", "production");
        assert!(parse_auth_mode().is_err());
        if let Ok(v) = temp {
            env::set_var("MCM_AUTH_MODE", v);
        } else {
            env::remove_var("MCM_AUTH_MODE");
        }
    }

    fn test_config(auth_mode: AuthMode) -> ServerConfig {
        ServerConfig {
            data_dir: PathBuf::from("/tmp/mcm-test"),
            web_dir: PathBuf::from("/tmp/mcm-test/web"),
            auth_mode,
            oidc_issuer: None,
            oidc_client_id: None,
            oidc_client_secret: None,
            oidc_redirect_url: None,
        }
    }

    #[test]
    fn validate_oidc_skips_in_mock_mode() {
        let cfg = test_config(AuthMode::Mock);
        assert!(cfg.validate_oidc().is_ok());
    }

    #[test]
    fn validate_oidc_real_requires_all_fields() {
        let cfg = test_config(AuthMode::Real);
        let err = cfg.validate_oidc().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("MCM_OIDC_ISSUER"), "missing issuer: {msg}");
        assert!(
            msg.contains("MCM_OIDC_CLIENT_ID"),
            "missing client_id: {msg}"
        );
        assert!(
            msg.contains("MCM_OIDC_CLIENT_SECRET"),
            "missing secret: {msg}"
        );
        assert!(
            msg.contains("MCM_OIDC_REDIRECT_URL"),
            "missing redirect: {msg}"
        );
    }

    #[test]
    fn validate_oidc_real_passes_with_all_fields() {
        let cfg = ServerConfig {
            data_dir: PathBuf::from("/tmp/mcm-test"),
            web_dir: PathBuf::from("/tmp/mcm-test/web"),
            auth_mode: AuthMode::Real,
            oidc_issuer: Some("https://auth.dyyapp.com".to_string()),
            oidc_client_id: Some("client-id".to_string()),
            oidc_client_secret: Some(SecretString::new("secret".to_string())),
            oidc_redirect_url: Some("https://mc.dyyapp.com/callback".to_string()),
        };
        assert!(cfg.validate_oidc().is_ok());
    }

    #[test]
    fn validate_oidc_real_partial_fields_fails() {
        let cfg = ServerConfig {
            data_dir: PathBuf::from("/tmp/mcm-test"),
            web_dir: PathBuf::from("/tmp/mcm-test/web"),
            auth_mode: AuthMode::Real,
            oidc_issuer: Some("https://auth.dyyapp.com".to_string()),
            oidc_client_id: None,
            oidc_client_secret: None,
            oidc_redirect_url: None,
        };
        let err = cfg.validate_oidc().unwrap_err();
        let msg = err.to_string();
        assert!(
            !msg.contains("MCM_OIDC_ISSUER"),
            "issuer should not be listed as missing: {msg}"
        );
        assert!(
            msg.contains("MCM_OIDC_CLIENT_ID"),
            "missing client_id: {msg}"
        );
    }
}

//! Helper functions for storage: slug normalization, payload validation,
//! `/x` refusal, time formatting.

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};

use crate::mcm_package::{scan_for_secrets, validate_package_name};

use super::meta::ReservationRow;

#[derive(Debug)]
pub(super) struct ContentMetadata {
    pub name: String,
    pub description: String,
}

pub(super) fn normalize_slug(slug: &str) -> Result<String> {
    validate_package_name(slug)?;
    Ok(slug.to_ascii_lowercase())
}

pub(super) fn validate_payload(version: &str, content: &[u8]) -> Result<ContentMetadata> {
    if version.is_empty() {
        return Err(anyhow!("version must not be empty"));
    }
    let value: serde_json::Value =
        serde_json::from_slice(content).context("package content is not valid JSON")?;
    scan_for_secrets(&value)?;
    validate_install_only(&value)?;
    let name = value
        .get("identity")
        .and_then(|i| i.get("name"))
        .and_then(|v| v.as_str())
        .or_else(|| value.get("name").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let description = value
        .get("identity")
        .and_then(|i| i.get("description"))
        .and_then(|v| v.as_str())
        .or_else(|| value.get("description").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    Ok(ContentMetadata { name, description })
}

pub(super) fn refuse_under_x(dir: &Path) -> Result<()> {
    let starts_with_x = dir.ancestors().any(|a| a == Path::new("/x"));
    if starts_with_x {
        return Err(anyhow!(
            "MCM_SHARE_DATA_DIR must not be under /x (got {}); \
             server storage must live outside /x per the plan",
            dir.display()
        ));
    }
    Ok(())
}

pub(super) fn is_expired(res: &ReservationRow, now_unix: i64) -> bool {
    res.reserved_until_unix <= now_unix
}

pub(super) fn format_rfc3339(now: time::OffsetDateTime) -> String {
    use time::format_description::well_known::Rfc3339;
    now.format(&Rfc3339)
        .unwrap_or_else(|_| format!("{}+00:00", now.date()))
}

pub(super) fn format_rfc3339_from_unix(unix: i64) -> Result<String> {
    let dt = time::OffsetDateTime::from_unix_timestamp(unix).context("invalid unix timestamp")?;
    Ok(format_rfc3339(dt))
}

fn validate_install_only(content: &serde_json::Value) -> Result<()> {
    let Some(obj) = content.as_object() else {
        return Ok(());
    };
    // v1 check: reject v1 schema with actionable error.
    if let Some(v) = obj.get("schema_version").and_then(|v| v.as_u64()) {
        if v == 1 {
            bail!(
                "v1 .mcm files are no longer supported; \
                 rebuild from dyyl source with `mcm build <file.dyyl>`"
            );
        }
    }
    // v2 validation: check that all steps are install-permitted.
    if let Some(steps) = obj.get("steps").and_then(|v| v.as_array()) {
        for step in steps {
            if let Some(perm) = step.get("permission").and_then(|v| v.as_str()) {
                if perm != "install" {
                    bail!(
                        "non-install step (permission: {perm}) is not allowed in shared packages; \
                         use `mcm do` for full-power execution"
                    );
                }
            }
        }
    }
    // Legacy v1 field checks (for backwards compatibility during migration).
    if let Some(actions) = obj.get("actions") {
        if !actions.is_null() {
            if let Some(arr) = actions.as_array() {
                if !arr.is_empty() {
                    bail!("non-install content: actions are not allowed in shared packages");
                }
            } else {
                bail!("non-install content: actions must be an array");
            }
        }
    }
    if let Some(launch) = obj.get("launch") {
        if !launch.is_null() {
            bail!("non-install content: launch config is not allowed in shared packages");
        }
    }
    if let Some(local) = obj.get("local") {
        if !local.is_null() {
            bail!("non-install content: local data is not allowed in shared packages");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn refuse_under_x_rejects_x_paths() {
        assert!(refuse_under_x(Path::new("/x")).is_err());
        assert!(refuse_under_x(Path::new("/x/mcm-share")).is_err());
        assert!(refuse_under_x(Path::new("/x/foo/bar")).is_err());
        assert!(refuse_under_x(Path::new("/var/lib/mcm-share")).is_ok());
        assert!(refuse_under_x(Path::new("/tmp/mcm-test")).is_ok());
    }

    #[test]
    fn normalize_slug_lowercases_and_validates() {
        assert_eq!(normalize_slug("my-pkg").unwrap(), "my-pkg");
        assert!(normalize_slug("UPPER").is_err());
        assert!(normalize_slug("").is_err());
        assert!(normalize_slug("mcm").is_err(), "reserved");
        let _ = PathBuf::from("/");
    }

    #[test]
    fn validate_payload_rejects_secrets() {
        let json = br#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"x","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z","token":"leak"}"#;
        assert!(validate_payload("1", json).is_err());
        let ok = br#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"x","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z"}"#;
        assert!(validate_payload("1", ok).is_ok());
        assert!(validate_payload("", ok).is_err());
    }

    #[test]
    fn validate_payload_rejects_v1() {
        let json = br#"{"schema_version":1,"name":"x","version":"1"}"#;
        let err = validate_payload("1", json).unwrap_err();
        assert!(format!("{err}").contains("v1 .mcm files are no longer supported"));
    }

    #[test]
    fn validate_payload_rejects_non_install_steps() {
        let json = br#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"x","version":"1"},"permissions":{"install":true},"steps":[{"op":"shell.run","permission":"do","args":{"command":"rm -rf /"}}],"created_at":"2024-01-01T00:00:00Z"}"#;
        assert!(validate_payload("1", json).is_err());
        let json = br#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"x","version":"1"},"permissions":{"install":true},"steps":[{"op":"mod.install","permission":"install","args":{}}],"created_at":"2024-01-01T00:00:00Z"}"#;
        assert!(validate_payload("1", json).is_ok());
    }

    #[test]
    fn validate_payload_rejects_actions() {
        let json =
            br#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"x","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z","actions":[{"name":"run","kind":"shell","command":"rm -rf /"}]}"#;
        assert!(validate_payload("1", json).is_err());
        let json = br#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"x","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z","actions":[]}"#;
        assert!(validate_payload("1", json).is_ok());
        let json = br#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"x","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z","actions":null}"#;
        assert!(validate_payload("1", json).is_ok());
    }

    #[test]
    fn validate_payload_rejects_launch() {
        let json = br#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"x","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z","launch":{"game":"1.20.1"}}"#;
        assert!(validate_payload("1", json).is_err());
        let json = br#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"x","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z","launch":null}"#;
        assert!(validate_payload("1", json).is_ok());
    }

    #[test]
    fn validate_payload_rejects_local() {
        let json = br#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"x","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z","local":{"settings":{}}}"#;
        assert!(validate_payload("1", json).is_err());
    }

    #[test]
    fn validate_payload_extracts_metadata() {
        let json = br#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"my-pkg","version":"1.0.0","description":"A cool pack"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z"}"#;
        let meta = validate_payload("1", json).unwrap();
        assert_eq!(meta.name, "my-pkg");
        assert_eq!(meta.description, "A cool pack");
    }

    #[test]
    fn validate_payload_defaults_missing_metadata() {
        let json = br#"{"schema_version":2,"kind":"mcm-lock","identity":{"version":"1.0.0"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z"}"#;
        let meta = validate_payload("1", json).unwrap();
        assert_eq!(meta.name, "");
        assert_eq!(meta.description, "");
    }
}

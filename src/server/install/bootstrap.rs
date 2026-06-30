//! `/install` bootstrap script route (task 17).
//!
//! `GET /install` returns a POSIX shell script that downloads, verifies, and
//! installs the MCM binary for Linux x86_64. Other OS/arch combinations are
//! detected and exit with an explicit message.
//!
//! The script lives in the sibling file `bootstrap-script.sh` and is embedded
//! at compile time via `include_str!`. This keeps the Rust source file compact
//! while preserving the script as an editable shell file.
//!
//! env overrides (all optional, passed through to the script):
//! - `MCM_INSTALL_PREFIX` — install directory (default: `~/.local/bin`)
//! - `MCM_RELEASE_BASE_URL` — base URL for release artifacts
//! - `MCM_INSTALL_OS` — override OS detection (for testing)
//! - `MCM_INSTALL_ARCH` — override arch detection (for testing)
//! - `MCM_INSTALL_DRY_RUN` — if `true`, prints actions without executing

use axum::http::{header, StatusCode};
use axum::response::IntoResponse;

/// Static bootstrap shell script, loaded from the sibling `.sh` file at
/// compile time.
///
/// # Security properties
/// - Downloads to a temp directory first (never piped to shell).
/// - Verifies SHA-256 checksum before extraction or install.
/// - If checksum mismatches, deletes the temp dir and exits with non-zero.
/// - Supports dry-run via `MCM_INSTALL_DRY_RUN` — never modifies disk.
/// - Offers `sudo`/`pkexec` only when the target prefix is not writable.
/// - Non-interactive system install prints the exact `sudo` command.
pub(crate) const BOOTSTRAP_SCRIPT: &str = include_str!("bootstrap-script.sh");

/// `GET /install` handler.
///
/// Returns a POSIX shell bootstrap script with `Content-Type: text/x-shellscript`.
pub(crate) async fn install_script() -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        BOOTSTRAP_SCRIPT,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_starts_with_shebang() {
        assert!(
            BOOTSTRAP_SCRIPT.starts_with("#!/bin/bash"),
            "script should start with bash shebang"
        );
    }

    #[test]
    fn script_contains_checksum_verification() {
        assert!(
            BOOTSTRAP_SCRIPT.contains("sha256"),
            "script must verify sha256 checksum"
        );
        assert!(
            BOOTSTRAP_SCRIPT.contains("_checksum_ok"),
            "script must have checksum result variable"
        );
        assert!(
            BOOTSTRAP_SCRIPT.contains("checksum verification failed"),
            "script must abort on checksum mismatch"
        );
    }

    #[test]
    fn script_detects_unsupported_os() {
        assert!(
            BOOTSTRAP_SCRIPT.contains("unsupported OS"),
            "script should detect unsupported OS"
        );
    }

    #[test]
    fn script_detects_unsupported_arch() {
        assert!(
            BOOTSTRAP_SCRIPT.contains("unsupported architecture"),
            "script should detect unsupported arch"
        );
    }

    #[test]
    fn script_uses_staged_writes() {
        assert!(
            BOOTSTRAP_SCRIPT.contains("mktemp"),
            "script should use a temp directory"
        );
        assert!(
            BOOTSTRAP_SCRIPT.contains("trap") && BOOTSTRAP_SCRIPT.contains("rm"),
            "script should clean up temp directory via trap"
        );
    }

    #[test]
    fn script_supports_env_overrides() {
        assert!(
            BOOTSTRAP_SCRIPT.contains("MCM_INSTALL_DIR"),
            "script should support MCM_INSTALL_DIR"
        );
        assert!(
            BOOTSTRAP_SCRIPT.contains("MCM_RELEASE_BASE_URL"),
            "script should support MCM_RELEASE_BASE_URL"
        );
        assert!(
            BOOTSTRAP_SCRIPT.contains("MCM_INSTALL_DRY_RUN"),
            "script should support MCM_INSTALL_DRY_RUN"
        );
        assert!(
            BOOTSTRAP_SCRIPT.contains("MCM_INSTALL_OS"),
            "script should support MCM_INSTALL_OS"
        );
        assert!(
            BOOTSTRAP_SCRIPT.contains("MCM_INSTALL_ARCH"),
            "script should support MCM_INSTALL_ARCH"
        );
    }

    #[test]
    fn script_prints_sudo_command_for_unwritable_path() {
        // The script should NOT run sudo automatically. It should print the
        // exact sudo command for the user to run manually.
        assert!(
            BOOTSTRAP_SCRIPT.contains("sudo install"),
            "script should print sudo install command for unwritable paths"
        );
        // Verify the script does NOT automatically escalate privileges
        // by checking it doesn't have "sudo cp" or "pkexec cp" (old behavior)
        assert!(
            !BOOTSTRAP_SCRIPT.contains("sudo cp"),
            "script should not automatically run sudo cp"
        );
        assert!(
            !BOOTSTRAP_SCRIPT.contains("pkexec"),
            "script should not reference pkexec"
        );
    }

    #[test]
    fn script_does_not_pipe_unverified_bytes_to_shell() {
        let dangerous = ["| sh", "| bash", "| /bin/sh", "| /bin/bash"];
        for line in BOOTSTRAP_SCRIPT.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                continue;
            }
            for pat in &dangerous {
                if trimmed.contains(pat) {
                    panic!("Dangerous pipe pattern '{pat}' found in: {trimmed}");
                }
            }
        }
    }

    #[test]
    fn script_dry_run_preview_supported() {
        assert!(
            BOOTSTRAP_SCRIPT.contains("[DRY-RUN]"),
            "script should have dry-run output format"
        );
    }
}

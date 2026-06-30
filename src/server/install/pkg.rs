//! `/install/pkg/{slug}` package install script route (task 18).
//!
//! `GET /install/pkg/{slug}` returns a POSIX shell script that ensures MCM is
//! installed (reusing /install bootstrap semantics), downloads the named .mcm
//! package via the service's public download URL, and delegates to
//! `mcm install --yes`.
//!
//! The script is generated at runtime with the validated slug. Env overrides:
//! - `MCM_PACKAGE_BASE_URL` — service base URL for package downloads
//! - `MCM_INSTALL_DRY_RUN` — if `true`, prints actions without executing

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use super::super::ServerState;
use crate::mcm_package::validate_package_name;

/// `GET /install/pkg/{slug}` handler.
///
/// Validates the package name at the boundary, checks the package exists in
/// storage, and returns a POSIX shell script that delegates to
/// `mcm install <url> --yes`.
pub(crate) async fn pkg_install_script(
    State(state): State<ServerState>,
    Path(slug): Path<String>,
) -> Response {
    // Validate slug at the boundary — only [a-z0-9-] passes.
    if let Err(e) = validate_package_name(&slug) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "invalid package name",
                "reason": e.to_string()
            })),
        )
            .into_response();
    }

    match state.storage().get_content(&slug) {
        Ok(Some(_content)) => {
            let script = generate_pkg_script(&slug);
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8"),
                    (header::CACHE_CONTROL, "no-cache"),
                ],
                script,
            )
                .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "package not found",
                "slug": slug
            })),
        )
            .into_response(),
        Err(e) => {
            eprintln!("storage error during package lookup: {e:#}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal", "what": "lookup package" })),
            )
                .into_response()
        }
    }
}

/// Generate a POSIX shell script that installs a named MCM package.
///
/// The script:
/// 1. Supports dry-run via `MCM_INSTALL_DRY_RUN`.
/// 2. Bootstraps MCM via the trusted `/install` endpoint if `mcm` is not found.
/// 3. Delegates to `mcm install <package-download-url> --yes`.
///
/// # Security
/// - The `slug` has already been validated by [`validate_package_name`] so only
///   `[a-z0-9-]` characters reach this function.
/// - The slug is embedded inside single quotes in the `SLUG=` assignment and
///   referenced via shell variable `$SLUG` elsewhere.
/// - No raw untrusted input reaches shell execution context.
fn generate_pkg_script(slug: &str) -> String {
    format!(
        r#"#!/bin/bash
# MCM Package Installer
# Installs a published MCM package permanently.
#
# Usage:
#   curl -fsSL https://mc.dyyapp.com/install/pkg/{slug} | bash
#
# Env overrides (all optional):
#   MCM_PACKAGE_BASE_URL    Service base URL (default: https://mc.dyyapp.com)
#   MCM_INSTALL_DRY_RUN     If "true", print actions without executing

set -euo pipefail

SLUG='{slug}'
PACKAGE_BASE_URL="${{MCM_PACKAGE_BASE_URL:-https://mc.dyyapp.com}}"
DRY_RUN="${{MCM_INSTALL_DRY_RUN:-false}}"

# ---- Dry-run / preview ----
if [ "${{DRY_RUN}}" = "true" ]; then
    echo "[DRY-RUN] Package:          ${{SLUG}}"
    echo "[DRY-RUN] Download URL:     ${{PACKAGE_BASE_URL}}/api/share/pkg/${{SLUG}}"
    echo "[DRY-RUN] Install command:  mcm install ${{PACKAGE_BASE_URL}}/api/share/pkg/${{SLUG}} --yes"
    exit 0
fi

# ---- Ensure MCM is installed ----
if ! command -v mcm >/dev/null 2>&1; then
    echo "MCM not found. Bootstrapping MCM from ${{PACKAGE_BASE_URL}}..."
    curl -fsSL "${{PACKAGE_BASE_URL}}/install" | bash
    echo "MCM installed."
fi

# ---- Install package via MCM (handles download + --yes mode) ----
echo "Installing package '${{SLUG}}'..."
mcm install "${{PACKAGE_BASE_URL}}/api/share/pkg/${{SLUG}}" --yes

echo ""
echo "Package '${{SLUG}}' installed successfully."
"#,
        slug = slug
    )
}

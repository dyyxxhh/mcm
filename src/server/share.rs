//! Share route set — public download of `.mcm` packages plus authenticated
//! publish / update / delete.
//!
//! # Routes
//! - `GET    /api/share/list`        — list all packages (public).
//! - `GET    /api/share/pkg/{slug}`  — download package bytes (public).
//! - `POST   /api/share/pkg`         — publish (requires `AuthedOwner`).
//! - `PUT    /api/share/pkg/{slug}`  — update (requires `AuthedOwner`, owner match).
//! - `DELETE /api/share/pkg/{slug}`  — delete (requires `AuthedOwner`, owner match).
//!
//! # Publish policy (enforced here, before `Storage::publish`/`update`/`delete`)
//! - Max 5 existing packages per user → 6th publish: 409 `{"error":"package limit reached","limit":5}`.
//! - 1 push/day (publish OR update counts). Second push same day: 429 `{"error":"daily push limit reached"}`.
//! - Delete does NOT count as a push and does NOT reset the daily limit.
//! - Body > 10 MB: 413. Content-Type not `application/json`: 415.
//! - No admin token, no Turnstile anywhere here. Auth is OIDC only (`AuthedOwner`).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use tower_http::limit::RequestBodyLimitLayer;

use super::auth::AuthedOwner;
use super::storage::{DeleteOutcome, PublishOutcome, UpdateOutcome};
use super::ServerState;

/// 10 MB body limit on publish/update. The `.mcm` schema caps packages at
/// 10 MB, so this is the natural ceiling.
const BODY_LIMIT_BYTES: usize = 10 * 1024 * 1024;
const MAX_PACKAGES_PER_USER: i64 = 5;
const SECS_PER_DAY: i64 = 86_400;

pub(crate) fn routes() -> axum::Router<ServerState> {
    axum::Router::new()
        .route("/list", get(list_packages))
        .route("/mine", get(mine_packages))
        .route(
            "/pkg/{slug}",
            get(download_package)
                .put(update_package)
                .delete(delete_package),
        )
        .route("/pkg/{slug}/install-command", get(install_command))
        .route("/pkg", axum::routing::post(publish_package))
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT_BYTES))
}

async fn list_packages(State(state): State<ServerState>) -> Response {
    match state.storage().list() {
        Ok(pkgs) => (StatusCode::OK, Json(json!({ "packages": pkgs }))).into_response(),
        Err(e) => internal_error("list packages", e),
    }
}

async fn mine_packages(
    State(state): State<ServerState>,
    AuthedOwner(owner): AuthedOwner,
) -> Response {
    match state.storage().list_by_owner(&owner) {
        Ok(pkgs) => (StatusCode::OK, Json(json!({ "packages": pkgs }))).into_response(),
        Err(e) => internal_error("list mine packages", e),
    }
}

async fn download_package(State(state): State<ServerState>, Path(slug): Path<String>) -> Response {
    match state.storage().get_content(&slug) {
        Ok(Some(bytes)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            bytes,
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"package not found","slug":slug})),
        )
            .into_response(),
        Err(e) => internal_error("download package", e),
    }
}

async fn install_command(State(state): State<ServerState>, Path(slug): Path<String>) -> Response {
    match state.storage().get_content(&slug) {
        Ok(Some(_)) => {
            let cmd = format!(
                "curl -fsSL https://mc.dyyapp.com/install/pkg/{} | bash",
                slug
            );
            (
                StatusCode::OK,
                Json(json!({ "slug": slug, "install_command": cmd })),
            )
                .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"package not found","slug":slug})),
        )
            .into_response(),
        Err(e) => internal_error("install command", e),
    }
}

#[derive(Deserialize)]
struct PublishBody {
    slug: String,
    version: String,
    content: serde_json::Value,
}

async fn publish_package(
    State(state): State<ServerState>,
    AuthedOwner(owner): AuthedOwner,
    Json(body): Json<PublishBody>,
) -> Response {
    let content = match serde_json::to_vec(&body.content) {
        Ok(b) => b,
        Err(e) => return internal_error("serialize body", e.into()),
    };
    if let Some(resp) = check_push_policy(&state, &owner, false) {
        state
            .auth()
            .audit("publish", &owner, &body.slug, audit_code(&resp));
        return resp;
    }
    let resp = match state
        .storage()
        .publish(&body.slug, &body.version, &content, &owner)
    {
        Ok(PublishOutcome::Created { slug }) => {
            let _ = state.storage().record_push(&owner);
            state.auth().audit("publish", &owner, &slug, "ok");
            (
                StatusCode::CREATED,
                Json(json!({"slug":slug,"status":"created"})),
            )
                .into_response()
        }
        Ok(PublishOutcome::Conflict { reason }) => (
            StatusCode::CONFLICT,
            Json(json!({"error":"conflict","reason":reason})),
        )
            .into_response(),
        Err(e) => validation_or_500("publish package", e),
    };
    if resp.status() != StatusCode::CREATED {
        state
            .auth()
            .audit("publish", &owner, &body.slug, audit_code(&resp));
    }
    resp
}

async fn update_package(
    State(state): State<ServerState>,
    AuthedOwner(owner): AuthedOwner,
    Path(slug): Path<String>,
    Json(body): Json<PublishBody>,
) -> Response {
    let content = match serde_json::to_vec(&body.content) {
        Ok(b) => b,
        Err(e) => return internal_error("serialize body", e.into()),
    };
    if let Some(resp) = check_push_policy(&state, &owner, true) {
        state
            .auth()
            .audit("update", &owner, &slug, audit_code(&resp));
        return resp;
    }
    let resp = match state
        .storage()
        .update(&slug, &body.version, &content, &owner)
    {
        Ok(UpdateOutcome::Ok { slug }) => {
            let _ = state.storage().record_push(&owner);
            state.auth().audit("update", &owner, &slug, "ok");
            (
                StatusCode::OK,
                Json(json!({"slug":slug,"status":"updated"})),
            )
                .into_response()
        }
        Ok(UpdateOutcome::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"package not found","slug":slug})),
        )
            .into_response(),
        Ok(UpdateOutcome::Forbidden) => (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"owner mismatch","slug":slug})),
        )
            .into_response(),
        Err(e) => validation_or_500("update package", e),
    };
    if resp.status() != StatusCode::OK {
        state
            .auth()
            .audit("update", &owner, &slug, audit_code(&resp));
    }
    resp
}

async fn delete_package(
    State(state): State<ServerState>,
    AuthedOwner(owner): AuthedOwner,
    Path(slug): Path<String>,
) -> Response {
    let resp = match state.storage().delete(&slug, &owner) {
        Ok(DeleteOutcome::Ok) => {
            state.auth().audit("delete", &owner, &slug, "ok");
            (
                StatusCode::OK,
                Json(json!({"slug":slug,"status":"deleted"})),
            )
                .into_response()
        }
        Ok(DeleteOutcome::NotFound) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"package not found","slug":slug})),
        )
            .into_response(),
        Ok(DeleteOutcome::Forbidden) => (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"owner mismatch","slug":slug})),
        )
            .into_response(),
        Err(e) => internal_error("delete package", e),
    };
    if resp.status() != StatusCode::OK {
        state
            .auth()
            .audit("delete", &owner, &slug, audit_code(&resp));
    }
    resp
}

/// Check the publish policy before calling `Storage::publish`/`update`.
/// Returns `Some(response)` if the policy denies the push, `None` if allowed.
///
/// - `is_update`: if true, the package-count limit is NOT checked (the
///   package already exists, so updating it doesn't add a new one).
/// - Daily push limit: `now_unix - (now_unix % 86400)` is midnight UTC today.
///   If the owner's last push is `>=` that midnight, deny with 429.
fn check_push_policy(state: &ServerState, owner: &str, is_update: bool) -> Option<Response> {
    if !is_update {
        match state.storage().count_packages_by_owner(owner) {
            Ok(count) if count >= MAX_PACKAGES_PER_USER => {
                return Some(
                    (
                        StatusCode::CONFLICT,
                        Json(
                            json!({"error":"package limit reached","limit":MAX_PACKAGES_PER_USER}),
                        ),
                    )
                        .into_response(),
                );
            }
            Ok(_) => {}
            Err(e) => return Some(internal_error("policy count", e)),
        }
    }
    match state.storage().last_push_unix(owner) {
        Ok(Some(last)) => {
            let now = state.storage().now_unix();
            if last >= now - now.rem_euclid(SECS_PER_DAY) {
                return Some(
                    (
                        StatusCode::TOO_MANY_REQUESTS,
                        Json(json!({"error":"daily push limit reached"})),
                    )
                        .into_response(),
                );
            }
        }
        Ok(None) => {}
        Err(e) => return Some(internal_error("policy last push", e)),
    }
    None
}

fn audit_code(resp: &Response) -> &'static str {
    match resp.status().as_u16() {
        s if s < 300 => "ok",
        409 => "409",
        403 => "403",
        429 => "429",
        _ => "err",
    }
}

fn internal_error(what: &str, e: anyhow::Error) -> Response {
    eprintln!("share storage error during {what}: {e:#}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "internal", "what": what })),
    )
        .into_response()
}

fn validation_or_500(what: &str, e: anyhow::Error) -> Response {
    if is_validation_error(&e) {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "validation", "reason": e.to_string() })),
        )
            .into_response()
    } else {
        internal_error(what, e)
    }
}

fn is_validation_error(e: &anyhow::Error) -> bool {
    let msg = e.to_string().to_ascii_lowercase();
    msg.contains("package name")
        || msg.contains("secret")
        || msg.contains("version")
        || msg.contains("json")
        || msg.contains("non-install")
}

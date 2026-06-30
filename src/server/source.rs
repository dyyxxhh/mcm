//! Source route set — serve a manually imported source index plus metadata
//! and artifact blobs. Any computer can run source mode.
//!
//! Routes (read-only, no auth — sources are public catalogs):
//! - `GET /api/source/index`         — the `SourceIndex` JSON.
//! - `GET /api/source/meta/{slug}`   — metadata for one package.
//! - `GET /api/source/blob/{slug}`   — raw artifact bytes.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use serde_json::json;

use super::ServerState;

pub(crate) fn routes() -> axum::Router<ServerState> {
    axum::Router::new()
        .route("/index", get(index))
        .route("/meta/{slug}", get(meta))
        .route("/blob/{slug}", get(blob))
}

async fn index(State(state): State<ServerState>) -> Response {
    match state.source_store().get_index() {
        Ok(Some(idx)) => (StatusCode::OK, Json(json!(idx))).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"source index not configured"})),
        )
            .into_response(),
        Err(e) => internal_error("read source index", e),
    }
}

async fn meta(State(state): State<ServerState>, Path(slug): Path<String>) -> Response {
    match state.source_store().get_package(&slug) {
        Ok(Some(pkg)) => (StatusCode::OK, Json(json!(pkg))).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"package not found","slug":slug})),
        )
            .into_response(),
        Err(e) => internal_error("read source meta", e),
    }
}

async fn blob(State(state): State<ServerState>, Path(slug): Path<String>) -> Response {
    match state.source_store().get_blob(&slug) {
        Ok(Some(bytes)) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
            bytes,
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"blob not found","slug":slug})),
        )
            .into_response(),
        Err(e) => internal_error("read source blob", e),
    }
}

fn internal_error(what: &str, e: anyhow::Error) -> Response {
    eprintln!("source store error during {what}: {e:#}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error":"internal","what":what})),
    )
        .into_response()
}

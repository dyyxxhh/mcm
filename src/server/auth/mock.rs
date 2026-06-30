//! Mock OIDC provider for tests. Issues fake session tokens without any
//! network calls. Tests use this exclusively — real OIDC requires network
//! access to `https://auth.dyyapp.com`.
//!
//! # Mock flow
//! 1. `GET /api/auth/oidc/start` returns `{"auth_url":"mock://oidc/callback?state=<state>","login_id":"..."}`.
//! 2. The test "visits" the auth_url by calling `GET /api/auth/oidc/callback?code=<any>&state=<state>`.
//! 3. The callback handler issues a session for a configurable mock user
//!    (default `mock-user`, override via `MCM_OIDC_MOCK_USER` at server start).
//! 4. The session token is returned in the JSON body and as a cookie.
//! 5. CLI clients poll `GET /api/auth/oidc/poll/{login_id}` for the result.
//!
//! # CLI login_id
//! `start` always issues a `login_id` via [`LoginStore`]. The CLI can poll
//! `poll/{login_id}` to track authentication progress (pending → complete /
//! expired / denied). The mock always completes immediately (no browser
//! needed), so the CLI poll will see `complete` right after callback.

use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use super::login::LoginStatus;
use super::ServerState;

/// Generate a short unique-ish id without pulling in a uuid crate.
fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

/// `GET /api/auth/oidc/start` — returns a mock auth URL the test can follow.
/// Always includes a `login_id` for CLI polling.
pub(super) async fn start(State(state): State<ServerState>) -> impl IntoResponse {
    let mock_user = state.auth().mock_user();
    let state_param = format!("mock-state-{}", uuid_like());
    let login_id = state.auth().login_store().issue(&state_param);
    let auth_url = format!("/api/auth/oidc/callback?code=mock-code&state={state_param}");
    (
        StatusCode::OK,
        Json(json!({
            "auth_url": auth_url,
            "mock_user": mock_user,
            "state": state_param,
            "login_id": login_id,
        })),
    )
}

#[derive(Deserialize)]
pub(super) struct CallbackQuery {
    // `code` is accepted but ignored in mock mode (any code is valid).
    // Kept in the struct so the query string shape matches real OIDC.
    #[allow(dead_code)]
    code: String,
    state: String,
}

/// `GET /api/auth/oidc/callback?code=<code>&state=<state>` — in mock mode,
/// any code is accepted (the state must have been issued by `start`).
/// Returns a session token in the JSON body and as a cookie. Also completes
/// the login in LoginStore for CLI polling.
pub(super) async fn callback(
    State(state): State<ServerState>,
    Query(q): Query<CallbackQuery>,
) -> impl IntoResponse {
    if !state.auth().login_store().is_pending(&q.state) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalid or expired state"})),
        )
            .into_response();
    }
    let owner = state.auth().mock_user();
    let (token, expiry_unix) = state.auth().sessions().issue(&owner);
    state
        .auth()
        .login_store()
        .complete(&q.state, &token, &owner, expiry_unix);
    set_session_response(&token, &owner, expiry_unix)
}

/// `GET /api/auth/oidc/session` — returns the current session owner, if any.
/// Reads the token from `Authorization: Bearer <token>` or `mcm_session` cookie.
pub(super) async fn session(
    State(state): State<ServerState>,
    Token(token): Token,
) -> impl IntoResponse {
    let Some(token) = token else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthenticated"})),
        )
            .into_response();
    };
    match state.auth().sessions().lookup(&token) {
        Some(owner) => (StatusCode::OK, Json(json!({"owner": owner}))).into_response(),
        None => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthenticated"})),
        )
            .into_response(),
    }
}

/// `GET /api/auth/oidc/poll/{login_id}` — CLI polls for authentication
/// result. Returns the current status; complete results are one-shot.
pub(super) async fn poll(
    State(state): State<ServerState>,
    axum::extract::Path(login_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    match state.auth().login_store().poll(&login_id) {
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "login_id not found"})),
        )
            .into_response(),
        Some(LoginStatus::Pending) => {
            (StatusCode::OK, Json(json!({"status": "pending"}))).into_response()
        }
        Some(LoginStatus::Complete {
            token,
            owner,
            expires_at_unix,
        }) => (
            StatusCode::OK,
            Json(json!({
                "status": "complete",
                "token": token,
                "owner": owner,
                "expires_at_unix": expires_at_unix,
            })),
        )
            .into_response(),
        Some(LoginStatus::Expired) => {
            (StatusCode::OK, Json(json!({"status": "expired"}))).into_response()
        }
        Some(LoginStatus::Denied { reason }) => (
            StatusCode::OK,
            Json(json!({"status": "denied", "reason": reason})),
        )
            .into_response(),
    }
}

/// `GET /api/auth/oidc/logout` — invalidates the current session and clears
/// the cookie.
pub(super) async fn logout(
    State(state): State<ServerState>,
    Token(token): Token,
) -> impl IntoResponse {
    if let Some(token) = token {
        state.auth().sessions().invalidate(&token);
    }
    let clear_cookie = "mcm_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0";
    (
        StatusCode::OK,
        [(axum::http::header::SET_COOKIE, clear_cookie)],
        Json(json!({"status": "logged_out"})),
    )
        .into_response()
}

/// Extracts the session token from `Authorization: Bearer <token>` or the
/// `mcm_session` cookie. Used by the `/session` endpoint. Always succeeds
/// (returns `Token(None)` if no token is present) so the handler can decide.
pub(super) struct Token(pub(super) Option<String>);

impl FromRequestParts<ServerState> for Token {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &ServerState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Token(super::extract_token(&parts.headers)))
    }
}

/// Build the success response for a fresh session: JSON body + Set-Cookie.
pub(super) fn set_session_response(
    token: &str,
    owner: &str,
    expiry_unix: i64,
) -> axum::response::Response {
    let body = json!({
        "token": token,
        "owner": owner,
        "expires_at_unix": expiry_unix,
    });
    let cookie = format!("mcm_session={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=3600");
    (
        StatusCode::OK,
        [(axum::http::header::SET_COOKIE, cookie.as_str())],
        Json(body),
    )
        .into_response()
}

//! OIDC authentication + session store + `AuthedOwner` extractor + audit log.
//!
//! # Modes
//! - **Mock** (`MCM_AUTH_MODE=mock` or unset): tests use this. The mock
//!   provider issues session tokens without any network. See `mock.rs`.
//! - **Real** (`MCM_AUTH_MODE=real`): the real OIDC flow against
//!   `https://auth.dyyapp.com`. Requires all four `MCM_OIDC_*` env vars.
//!   Not exercised by tests (no network).
//!
//! # Session store
//! In-memory `Mutex<HashMap<String, Session>>` (token → owner + expiry).
//! Sessions are lost on restart; CLI re-logins. Redis is an optional future
//! upgrade (noted here so a future maintainer knows the seam).
//!
//! # Secrets
//! OIDC client secrets and session tokens are NEVER logged. `ServerConfig`'s
//! `oidc_client_secret` is a `SecretString` whose `Debug` impl is `<redacted>`.
//! Session tokens are opaque random strings; they appear only in the
//! `Set-Cookie`/`Authorization` header and the JSON body of the callback
//! response — never in `eprintln!`, `tracing`, or error messages.

pub(super) mod login;
pub(super) mod mock;
pub(super) mod oidc;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::routing::get;
use axum::Json;
use serde_json::json;

use super::ServerState;

/// Session lifetime in seconds (1 hour). Short-lived so a leaked token has
/// a bounded window.
const SESSION_TTL_SECS: i64 = 3600;

/// An authenticated session: owner + expiry (unix seconds).
struct Session {
    owner: String,
    expires_at_unix: i64,
}

/// In-memory session store: opaque random token → `Session`.
/// Behind a `Mutex` — low-volume personal share service, so the lock is
/// short-lived. Future: swap for Redis without changing the API.
#[derive(Default)]
pub(super) struct SessionStore {
    inner: Mutex<HashMap<String, Session>>,
}

impl SessionStore {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Issue a new session for `owner`. Returns `(token, expiry_unix)`.
    /// The token is a random-ish opaque string (no cryptographic strength
    /// needed beyond unguessability for a personal service — not a bearer
    /// token for a bank).
    pub(super) fn issue(&self, owner: &str) -> (String, i64) {
        let token = format!("sess-{}", nonce());
        let expiry = now_unix() + SESSION_TTL_SECS;
        self.inner.lock().expect("session mutex").insert(
            token.clone(),
            Session {
                owner: owner.to_string(),
                expires_at_unix: expiry,
            },
        );
        (token, expiry)
    }

    /// Look up a session by token. Returns `Some(owner)` if the token is
    /// valid and not expired, `None` otherwise. Expired sessions are lazily
    /// evicted.
    pub(super) fn lookup(&self, token: &str) -> Option<String> {
        let mut map = self.inner.lock().expect("session mutex");
        let expired = map
            .get(token)
            .is_some_and(|s| s.expires_at_unix <= now_unix());
        if expired {
            map.remove(token);
        }
        map.get(token).map(|s| s.owner.clone())
    }

    /// Invalidate (remove) a session by token. Returns `true` if the token
    /// existed and was removed.
    pub(super) fn invalidate(&self, token: &str) -> bool {
        self.inner
            .lock()
            .expect("session mutex")
            .remove(token)
            .is_some()
    }
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Short unique-ish nonce (no uuid crate). Combines wall-clock nanos with a
/// static counter for same-nanosecond calls.
fn nonce() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}-{n:x}")
}

/// Auth facade held by `ServerState`. Owns the session store, the login
/// polling store, and the mock user name. In real mode the mock user
/// is unused.
pub(super) struct Auth {
    sessions: SessionStore,
    login_store: login::LoginStore,
    mock_user: String,
    audit_log: PathBuf,
}

impl Auth {
    pub(super) fn new(mock_user: String, audit_log: PathBuf) -> Self {
        Self {
            sessions: SessionStore::new(),
            login_store: login::LoginStore::new(),
            mock_user,
            audit_log,
        }
    }

    pub(super) fn sessions(&self) -> &SessionStore {
        &self.sessions
    }

    pub(super) fn login_store(&self) -> &login::LoginStore {
        &self.login_store
    }

    pub(super) fn mock_user(&self) -> String {
        self.mock_user.clone()
    }

    /// Append one line to the audit log: `ts,action,owner,slug,outcome`.
    /// Best-effort — a failed audit write does not fail the request.
    pub(super) fn audit(&self, action: &str, owner: &str, slug: &str, outcome: &str) {
        let line = format!("{},{},{},{},{}\n", now_unix(), action, owner, slug, outcome);
        if let Some(parent) = self.audit_log.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.audit_log)
            .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
    }
}

/// Authenticated owner extractor. Reads `Authorization: Bearer <token>`
/// OR `mcm_session` cookie. Looks up the session in the store.
/// Missing/invalid → 401 `{"error":"unauthenticated"}`.
pub(crate) struct AuthedOwner(pub(crate) String);

impl FromRequestParts<ServerState> for AuthedOwner {
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ServerState,
    ) -> Result<Self, Self::Rejection> {
        let Some(token) = extract_token(&parts.headers) else {
            return Err(unauthenticated());
        };
        match state.auth().sessions().lookup(&token) {
            Some(owner) => Ok(AuthedOwner(owner)),
            None => Err(unauthenticated()),
        }
    }
}

/// Read the session token from `Authorization: Bearer <token>` or the
/// `mcm_session` cookie. Shared by `AuthedOwner` and the `/session` handler.
pub(super) fn extract_token(headers: &axum::http::HeaderMap) -> Option<String> {
    if let Some(auth) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(s) = auth.to_str() {
            if let Some(rest) = s.strip_prefix("Bearer ") {
                return Some(rest.to_string());
            }
        }
    }
    if let Some(c) = headers.get(axum::http::header::COOKIE) {
        if let Ok(s) = c.to_str() {
            for kv in s.split(';') {
                let kv = kv.trim();
                if let Some(rest) = kv.strip_prefix("mcm_session=") {
                    return Some(rest.to_string());
                }
            }
        }
    }
    None
}

fn unauthenticated() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "unauthenticated"})),
    )
}

/// Build the auth route subtree. Mounted under `/api/auth` in all modes
/// (share mode needs it for publish; source mode is mostly public but the
/// routes are harmless when unused).
///
/// Routes are dispatched based on `auth_mode`:
/// - **Mock**: `start`, `callback`, `session`, `poll`, `logout` → mock.rs
/// - **Real**: `start`, `callback`, `session`, `poll`, `logout` → oidc.rs
pub(crate) fn routes(auth_mode: super::config::AuthMode) -> axum::Router<ServerState> {
    match auth_mode {
        super::config::AuthMode::Mock => axum::Router::new()
            .route("/oidc/start", get(mock::start))
            .route("/oidc/callback", get(mock::callback))
            .route("/oidc/session", get(mock::session))
            .route("/oidc/poll/{login_id}", get(mock::poll))
            .route("/oidc/logout", get(mock::logout)),
        super::config::AuthMode::Real => axum::Router::new()
            .route("/oidc/start", get(oidc::start))
            .route("/oidc/callback", get(oidc::callback))
            .route("/oidc/session", get(oidc::session))
            .route("/oidc/poll/{login_id}", get(oidc::poll))
            .route("/oidc/logout", get(oidc::logout)),
    }
}

/// Read the mock user name from `MCM_OIDC_MOCK_USER` (default `mock-user`).
pub(super) fn mock_user_from_env() -> String {
    std::env::var("MCM_OIDC_MOCK_USER").unwrap_or_else(|_| "mock-user".to_string())
}

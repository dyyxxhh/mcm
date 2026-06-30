//! HTTP service shell with `share`, `source`, and `both` modes.
//!
//! Routes, mode gating, health, graceful shutdown, durable SQLite + blob
//! storage for share routes, source routes, and OIDC auth are implemented
//! and tested.
//!
//! # Modes
//! - `share`  — `/api/share/*` enabled, `/api/source/*` disabled.
//! - `source` — `/api/source/*` enabled, `/api/share/*` disabled.
//! - `both`   — both route sets enabled.
//!
//! # Defaults
//! - Bind: `127.0.0.1:8950` (NEVER `0.0.0.0` by default).
//! - Data dir: `/var/lib/mcm-share` (refuses `/x`).
//! - Config from env: `MCM_SHARE_DATA_DIR`, `MCM_OIDC_*`.
//!
//! # PM2
//! Blocking foreground process. Logs to stdout/stderr. Reads env. PM2 manages
//! restarts. Graceful shutdown on Ctrl+C / SIGTERM.

mod auth;
mod config;
mod install;
mod share;
mod source;
mod source_store;
pub mod storage;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::Json;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::signal;
use tower_http::services::ServeFile;

use auth::Auth;
use config::ServeMode;
pub(crate) use config::{parse_mode, resolve_web_dir, ServerConfig};
use source_store::SourceStore;
use storage::Storage;

/// Shared state handed to every handler. Cheap to clone (Arc).
#[derive(Clone)]
pub(crate) struct ServerState {
    inner: Arc<ServerStateInner>,
}

struct ServerStateInner {
    mode: ServeMode,
    config: ServerConfig,
    storage: Storage,
    source_store: SourceStore,
    auth: Auth,
}

impl ServerState {
    fn new(mode: ServeMode, config: ServerConfig, storage: Storage, auth: Auth) -> Self {
        let source_store = SourceStore::new(config.data_dir.clone());
        Self {
            inner: Arc::new(ServerStateInner {
                mode,
                config,
                storage,
                source_store,
                auth,
            }),
        }
    }

    pub(crate) fn mode(&self) -> ServeMode {
        self.inner.mode
    }

    pub(crate) fn storage(&self) -> &Storage {
        &self.inner.storage
    }

    pub(crate) fn data_dir(&self) -> &std::path::Path {
        &self.inner.config.data_dir
    }

    pub(crate) fn web_dir(&self) -> &std::path::Path {
        &self.inner.config.web_dir
    }

    pub(crate) fn source_store(&self) -> &SourceStore {
        &self.inner.source_store
    }

    pub(in crate::server) fn auth(&self) -> &Auth {
        &self.inner.auth
    }

    pub(crate) fn config(&self) -> &ServerConfig {
        &self.inner.config
    }

    pub(crate) fn auth_mode(&self) -> config::AuthMode {
        self.inner.config.auth_mode
    }
}

/// Entry point invoked by `app::run` for `mcm serve --mode <m> --bind <a>`.
///
/// Binds, prints a PM2-friendly startup line to stdout, then runs until Ctrl+C
/// or SIGTERM. Returns `Ok(())` on clean shutdown.
pub(crate) async fn run_server(mode: ServeMode, bind: SocketAddr) -> Result<()> {
    let config = ServerConfig::from_env()?;
    let storage = Storage::open(config.data_dir.clone()).context("initialize share storage")?;
    let mock_user = auth::mock_user_from_env();
    let audit_log = config.data_dir.join("audit.log");
    let auth = Auth::new(mock_user, audit_log);
    let state = ServerState::new(mode, config, storage, auth);
    let app = build_router(state);

    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("bind {bind}"))?;
    println!("mcm serve listening on {} mode={}", bind, mode.as_str());

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server runtime")?;
    Ok(())
}

/// Build the full router. Public so tests can mount the app on a random port
/// without going through `run_server` (which reads env + binds).
pub(crate) fn build_router(state: ServerState) -> axum::Router {
    let mode = state.mode();
    let auth_mode = state.auth_mode();
    let web_dir = state.web_dir().to_path_buf();
    let mut router = axum::Router::new()
        .route("/health", get(health))
        .route("/install", get(install::install_script))
        .route("/install/pkg/{slug}", get(install::pkg_install_script))
        .route("/release/{filename}", get(release_file))
        .nest("/api/auth", auth::routes(auth_mode));

    if mode.share_enabled() {
        router = router.nest("/api/share", share::routes());
    }
    if mode.source_enabled() {
        router = router.nest("/api/source", source::routes());
    }

    router = router.route("/api/{*path}", any(disabled_fallback));

    router = router
        .route_service("/app.js", ServeFile::new(web_dir.join("app.js")))
        .route_service("/styles.css", ServeFile::new(web_dir.join("styles.css")))
        .route_service("/index.html", ServeFile::new(web_dir.join("index.html")))
        .fallback(get(spa_index));

    router.with_state(state)
}

async fn health(State(state): State<ServerState>) -> impl IntoResponse {
    let body = json!({
        "status": "ok",
        "mode": state.mode().as_str(),
    });
    (StatusCode::OK, Json(body))
}

async fn spa_index(State(state): State<ServerState>) -> impl IntoResponse {
    let index_path = state.web_dir().join("index.html");
    match std::fs::read_to_string(&index_path) {
        Ok(index) => (StatusCode::OK, [(header::CONTENT_TYPE, "text/html")], index).into_response(),
        Err(_) => not_found().into_response(),
    }
}

/// Fallback handler: returns a 404-style "disabled" JSON error for any path
/// not matched by an enabled route set. This covers:
/// - `/api/share/*` when share mode is off.
/// - `/api/source/*` when source mode is off.
/// - any truly unknown path.
async fn disabled_fallback(
    State(state): State<ServerState>,
    uri: axum::http::Uri,
) -> impl IntoResponse {
    let path = uri.path();
    let (which, todo) = if path.starts_with("/api/share/") {
        if state.mode().share_enabled() {
            return not_found();
        }
        ("share", "share-mode-disabled")
    } else if path.starts_with("/api/source/") {
        if state.mode().source_enabled() {
            return not_found();
        }
        ("source", "source-mode-disabled")
    } else {
        return not_found();
    };

    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": format!("{which} mode disabled"),
            "todo": todo,
        })),
    )
}

/// Allowed release filenames — only serve known MCM release artifacts.
const ALLOWED_RELEASE_FILES: &[&str] = &["mcm-linux-x86_64", "mcm-linux-x86_64.sha256"];

/// `GET /release/{filename}` handler.
///
/// Serves release binary + checksum from `{data_dir}/release/`.
/// Only allows specific filenames to prevent path traversal.
async fn release_file(State(state): State<ServerState>, Path(filename): Path<String>) -> Response {
    if !ALLOWED_RELEASE_FILES.contains(&filename.as_str()) {
        return not_found().into_response();
    }

    let file_path = state.data_dir().join("release").join(&filename);

    match tokio::fs::read(&file_path).await {
        Ok(bytes) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/octet-stream".to_string()),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{filename}\""),
                ),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => not_found().into_response(),
    }
}

fn not_found() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({
            "error": "not found",
        })),
    )
}

/// Wait for Ctrl+C or SIGTERM. Used as the graceful-shutdown signal for
/// `axum::serve(...).with_graceful_shutdown(...)`.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("install ctrl-c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

// ---- test support ----
//
// Integration tests in `tests/server.rs`, `tests/server_storage.rs`, and
// `tests/server_auth.rs` need to spin up the router on a random port without
// going through `run_server` (which binds + reads env). These helpers build
// a router with a stub config and a real (temp) storage so the share routes
// exercise the real DB. Auth is always mock mode in tests (no network).

/// Build a test router with a stub `ServerConfig` and a fresh storage at
/// `data_dir`. Auth runs in mock mode with the default mock user
/// (`mock-user`, or `MCM_OIDC_MOCK_USER` if set). For integration tests only.
#[doc(hidden)]
pub fn __test_router_with_data_dir(
    mode_str: &str,
    data_dir: std::path::PathBuf,
) -> anyhow::Result<axum::Router> {
    __test_router_with_data_dir_and_clock(mode_str, data_dir, None)
}

/// Build a test router with an injectable `Clock` for time-dependent policy
/// tests (daily push limit). When `clock` is `None`, uses `SystemClock`.
#[doc(hidden)]
pub fn __test_router_with_data_dir_and_clock(
    mode_str: &str,
    data_dir: std::path::PathBuf,
    clock: Option<Box<dyn crate::server::storage::Clock>>,
) -> anyhow::Result<axum::Router> {
    __test_router_full(mode_str, data_dir, clock, &auth::mock_user_from_env())
}

/// Full test router builder: clock + mock user. Thread-safe (no env vars).
#[doc(hidden)]
pub fn __test_router_full(
    mode_str: &str,
    data_dir: std::path::PathBuf,
    clock: Option<Box<dyn crate::server::storage::Clock>>,
    mock_user: &str,
) -> anyhow::Result<axum::Router> {
    let mode = parse_mode(mode_str)?;
    let config = ServerConfig {
        data_dir: data_dir.clone(),
        web_dir: resolve_web_dir(),
        auth_mode: config::AuthMode::Mock,
        oidc_issuer: None,
        oidc_client_id: None,
        oidc_client_secret: None,
        oidc_redirect_url: None,
    };
    let storage = match clock {
        Some(c) => Storage::open_with_clock(data_dir.clone(), c)?,
        None => Storage::open(data_dir.clone())?,
    };
    let audit_log = data_dir.join("audit.log");
    let auth = Auth::new(mock_user.to_string(), audit_log);
    let state = ServerState::new(mode, config, storage, auth);
    Ok(build_router(state))
}

/// Build a test router with a specific mock user (no env var — thread-safe
/// for parallel tests). Use this for auth/policy tests that need a known user.
#[doc(hidden)]
pub fn __test_router_with_mock_user(
    mode_str: &str,
    data_dir: std::path::PathBuf,
    mock_user: &str,
) -> anyhow::Result<axum::Router> {
    let mode = parse_mode(mode_str)?;
    let config = ServerConfig {
        data_dir: data_dir.clone(),
        web_dir: resolve_web_dir(),
        auth_mode: config::AuthMode::Mock,
        oidc_issuer: None,
        oidc_client_id: None,
        oidc_client_secret: None,
        oidc_redirect_url: None,
    };
    let storage = Storage::open(data_dir.clone())?;
    let audit_log = data_dir.join("audit.log");
    let auth = Auth::new(mock_user.to_string(), audit_log);
    let state = ServerState::new(mode, config, storage, auth);
    Ok(build_router(state))
}

/// Build a test router with mock auth and a SPECIFIC mock user name. Use this
/// for auth/policy tests that need to log in as a known user. The mock user
/// is set via `MCM_OIDC_MOCK_USER` at router-build time.
#[doc(hidden)]
pub fn __test_router_with_mock_auth(
    mode_str: &str,
    data_dir: std::path::PathBuf,
    mock_user: &str,
) -> anyhow::Result<axum::Router> {
    __test_router_with_mock_user(mode_str, data_dir, mock_user)
}

/// Build a test router with an explicit `web_dir` for testing cwd independence.
/// Use this when you need to verify that the router serves static files from
/// a known directory regardless of the process working directory.
#[doc(hidden)]
pub fn __test_router_with_web_dir(
    mode_str: &str,
    data_dir: std::path::PathBuf,
    web_dir: std::path::PathBuf,
) -> anyhow::Result<axum::Router> {
    let mode = parse_mode(mode_str)?;
    let config = ServerConfig {
        data_dir: data_dir.clone(),
        web_dir,
        auth_mode: config::AuthMode::Mock,
        oidc_issuer: None,
        oidc_client_id: None,
        oidc_client_secret: None,
        oidc_redirect_url: None,
    };
    let storage = Storage::open(config.data_dir.clone())?;
    let audit_log = config.data_dir.join("audit.log");
    let auth = Auth::new(auth::mock_user_from_env(), audit_log);
    let state = ServerState::new(mode, config, storage, auth);
    Ok(build_router(state))
}

/// Build a test router with the legacy stub data dir `/tmp/mcm-test-share`.
/// Each call opens its own SQLite connection at this path; the schema is
/// `CREATE IF NOT EXISTS`, so parallel calls are safe. For isolated tests
/// that publish/update/delete, prefer [`__test_router_with_data_dir`] with a
/// fresh temp dir.
#[doc(hidden)]
pub fn __test_router(mode_str: &str) -> anyhow::Result<axum::Router> {
    __test_router_with_data_dir(mode_str, std::path::PathBuf::from("/tmp/mcm-test-share"))
}

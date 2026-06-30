//! Integration tests for the HTTP service shell (task 12).
//!
//! Each test spins up the router on a random local port (`127.0.0.1:0`) and
//! drives it with a real HTTP client. No mocks — the real Axum router + real
//! TCP listener. Stub handlers return 501; the assertions here check mode
//! gating, the health endpoint, and the disabled-route fallback, NOT the
//! downstream task 13/14/15 behavior.

use std::net::SocketAddr;

use axum::Router;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

struct TestServer {
    addr: SocketAddr,
    _handle: JoinHandle<()>,
}

impl TestServer {
    async fn start(mode: &str) -> Self {
        let app: Router = mcm::__test_router(mode).expect("build test router");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server run");
        });
        Self {
            addr,
            _handle: handle,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("client")
}

#[tokio::test]
async fn health_returns_ok_and_mode_share() {
    let server = TestServer::start("share").await;
    let url = server.url("/health");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get health");
        (resp.status().as_u16(), resp.text().expect("body"))
    });
    let (status, body) = handle.await.expect("join");
    assert_eq!(status, 200);
    assert!(body.contains(r#""status":"ok""#), "body: {body}");
    assert!(body.contains(r#""mode":"share""#), "body: {body}");
}

#[tokio::test]
async fn health_returns_ok_and_mode_source() {
    let server = TestServer::start("source").await;
    let url = server.url("/health");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get health");
        (resp.status().as_u16(), resp.text().expect("body"))
    });
    let (status, body) = handle.await.expect("join");
    assert_eq!(status, 200);
    assert!(body.contains(r#""mode":"source""#), "body: {body}");
}

#[tokio::test]
async fn health_returns_ok_and_mode_both() {
    let server = TestServer::start("both").await;
    let url = server.url("/health");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get health");
        (resp.status().as_u16(), resp.text().expect("body"))
    });
    let (status, body) = handle.await.expect("join");
    assert_eq!(status, 200);
    assert!(body.contains(r#""mode":"both""#), "body: {body}");
}

#[tokio::test]
async fn share_mode_share_routes_enabled_return_empty_list() {
    let server = TestServer::start("share").await;
    let url = server.url("/api/share/list");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get share list");
        (resp.status().as_u16(), resp.text().expect("body"))
    });
    let (status, body) = handle.await.expect("join");
    assert_eq!(status, 200);
    assert!(
        body.contains(r#""packages":"#),
        "body should contain packages field: {body}"
    );
}

#[tokio::test]
async fn share_mode_source_routes_disabled() {
    let server = TestServer::start("share").await;
    let url = server.url("/api/source/index");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get source index");
        (resp.status().as_u16(), resp.text().expect("body"))
    });
    let (status, body) = handle.await.expect("join");
    assert_eq!(status, 404);
    assert!(
        body.contains(r#""error":"source mode disabled""#),
        "body: {body}"
    );
}

#[tokio::test]
async fn source_mode_source_routes_enabled_return_404_when_unconfigured() {
    let server = TestServer::start("source").await;
    let url = server.url("/api/source/index");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get source index");
        (resp.status().as_u16(), resp.text().expect("body"))
    });
    let (status, body) = handle.await.expect("join");
    assert_eq!(status, 404);
    assert!(
        body.contains(r#""error":"source index not configured""#),
        "body: {body}"
    );
}

#[tokio::test]
async fn source_mode_share_routes_disabled() {
    let server = TestServer::start("source").await;
    let url = server.url("/api/share/list");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get share list");
        (resp.status().as_u16(), resp.text().expect("body"))
    });
    let (status, body) = handle.await.expect("join");
    assert_eq!(status, 404);
    assert!(
        body.contains(r#""error":"share mode disabled""#),
        "body: {body}"
    );
}

#[tokio::test]
async fn both_mode_both_route_sets_enabled() {
    let server = TestServer::start("both").await;

    let url1 = server.url("/api/share/list");
    let url2 = server.url("/api/source/index");
    let handle = tokio::task::spawn_blocking(move || {
        let r1 = client().get(&url1).send().expect("share list");
        let r2 = client().get(&url2).send().expect("source index");
        (
            r1.status().as_u16(),
            r1.text().expect("body1"),
            r2.status().as_u16(),
            r2.text().expect("body2"),
        )
    });
    let (s1, b1, s2, b2) = handle.await.expect("join");
    assert_eq!(s1, 200);
    assert!(b1.contains(r#""packages":"#), "body1: {b1}");
    assert_eq!(s2, 404);
    assert!(
        b2.contains(r#""error":"source index not configured""#),
        "body2: {b2}"
    );
}

#[tokio::test]
async fn spa_dashboard_route_serves_index_html() {
    let server = TestServer::start("both").await;
    let url = server.url("/dashboard");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get dashboard");
        (resp.status().as_u16(), resp.text().expect("body"))
    });
    let (status, body) = handle.await.expect("join");
    assert_eq!(status, 200);
    assert!(body.contains(r#"<div id="app"></div>"#), "body: {body}");
    assert!(body.contains(r#"/app.js"#), "body: {body}");
}

#[tokio::test]
async fn unknown_api_path_returns_json_not_found() {
    let server = TestServer::start("both").await;
    let url = server.url("/api/no-such-path");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get unknown api");
        (resp.status().as_u16(), resp.text().expect("body"))
    });
    let (status, body) = handle.await.expect("join");
    assert_eq!(status, 404);
    assert!(body.contains(r#""error":"not found""#), "body: {body}");
}

#[tokio::test]
async fn share_pkg_download_unknown_slug_returns_404() {
    let server = TestServer::start("share").await;
    let url = server.url("/api/share/pkg/does-not-exist");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get pkg");
        (resp.status().as_u16(), resp.text().expect("body"))
    });
    let (status, body) = handle.await.expect("join");
    assert_eq!(status, 404);
    assert!(
        body.contains(r#""error":"package not found""#),
        "body: {body}"
    );
}

#[tokio::test]
async fn share_publish_without_auth_returns_401() {
    let server = TestServer::start("share").await;
    let url = server.url("/api/share/pkg");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().post(&url).send().expect("post pkg");
        resp.status().as_u16()
    });
    let status = handle.await.expect("join");
    assert_eq!(status, 401, "publish without auth should 401, got {status}");
}

#[tokio::test]
async fn static_files_served_regardless_of_cwd() {
    let data_dir = tempfile::tempdir().expect("data dir");

    // Resolve web/ relative to the source file (repo root).
    let web_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");
    assert!(
        web_dir.join("index.html").is_file(),
        "repo web/ must exist for this test: {}",
        web_dir.display()
    );

    // Build router with explicit web_dir — cwd does not matter.
    let app: Router =
        mcm::__test_router_with_web_dir("both", data_dir.path().to_path_buf(), web_dir)
            .expect("build test router");

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let _handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server run");
    });

    let base = format!("http://{addr}");

    // /index.html must return 200 with the HTML content.
    let url = format!("{base}/index.html");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get index.html");
        (resp.status().as_u16(), resp.text().expect("body"))
    });
    let (status, body) = handle.await.expect("join");
    assert_eq!(status, 200, "GET /index.html should be 200");
    assert!(body.contains(r#"<div id="app"></div>"#), "body: {body}");

    // /dashboard (SPA fallback) must also return 200 with index.html content.
    let url = format!("{base}/dashboard");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get dashboard");
        (resp.status().as_u16(), resp.text().expect("body"))
    });
    let (status, body) = handle.await.expect("join");
    assert_eq!(status, 200, "GET /dashboard should be 200");
    assert!(body.contains(r#"<div id="app"></div>"#), "body: {body}");
}

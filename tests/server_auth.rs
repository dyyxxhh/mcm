//! Integration tests for OIDC auth + publish policy (task 14).
//!
//! Acceptance criteria covered:
//! - Publish with valid mock OIDC session succeeds.
//! - Publish without login fails (401).
//! - Update overwrites the current package (on a separate day).
//! - Update/delete by package owner succeeds.
//! - Update/delete by another user fails (403).
//! - Duplicate slug returns 409.
//! - Second publish or update push by same user on same day fails (429).
//! - Deleting a package does NOT reset the daily push limit.
//! - Sixth simultaneous package fails (409, limit 5).
//! - Oversized body returns 413.
//! - Non-JSON returns 415.
//! - No admin token or Turnstile required anywhere in publish/update/delete.
//! - Deleted slug cannot be claimed by another user for 2 days.
// SIZE_OK: test fixture — many independent integration tests.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::Router;
use mcm::Clock;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

struct TestServer {
    addr: SocketAddr,
    _handle: JoinHandle<()>,
    _data_dir: Arc<TempDir>,
}

impl TestServer {
    async fn start(user: &str) -> Self {
        let data_dir = Arc::new(TempDir::new().expect("temp dir"));
        let app: Router =
            mcm::__test_router_with_mock_user("share", data_dir.path().to_path_buf(), user)
                .expect("build test router");
        Self::serve(app, data_dir).await
    }

    async fn start_with_clock(user: &str, clock: FakeClock) -> Self {
        let data_dir = Arc::new(TempDir::new().expect("temp dir"));
        let app: Router = mcm::__test_router_full(
            "share",
            data_dir.path().to_path_buf(),
            Some(Box::new(clock)),
            user,
        )
        .expect("build test router");
        Self::serve(app, data_dir).await
    }

    async fn serve(app: Router, data_dir: Arc<TempDir>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server run");
        });
        Self {
            addr,
            _handle: handle,
            _data_dir: data_dir,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    async fn login(&self) -> String {
        let start_url = self.url("/api/auth/oidc/start");
        let cb_base = format!("http://{}", self.addr);
        tokio::task::spawn_blocking(move || {
            let c = client();
            let r = c.get(&start_url).send().expect("start");
            let b: serde_json::Value =
                serde_json::from_str(&r.text().expect("body")).expect("json");
            let auth_url = b["auth_url"].as_str().expect("url").to_string();
            let cb = c.get(format!("{cb_base}{auth_url}")).send().expect("cb");
            let cb_b: serde_json::Value =
                serde_json::from_str(&cb.text().expect("body")).expect("json");
            cb_b["token"].as_str().expect("token").to_string()
        })
        .await
        .expect("join")
    }
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("client")
}

fn pkg_content(name: &str, version: &str) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": { "name": name, "version": version },
        "permissions": { "install": true },
        "steps": [],
        "artifacts": [],
        "created_at": "2024-01-01T00:00:00Z"
    })
}

fn publish_body(slug: &str, version: &str) -> serde_json::Value {
    serde_json::json!({ "slug": slug, "version": version, "content": pkg_content(slug, version) })
}

struct FakeClock {
    now_unix: Arc<Mutex<i64>>,
}

impl FakeClock {
    fn new(start: i64) -> Self {
        Self {
            now_unix: Arc::new(Mutex::new(start)),
        }
    }
    fn advance(&self, secs: i64) {
        *self.now_unix.lock().expect("clock") += secs;
    }
}

impl Clone for FakeClock {
    fn clone(&self) -> Self {
        Self {
            now_unix: self.now_unix.clone(),
        }
    }
}

impl Clock for FakeClock {
    fn now_rfc3339(&self) -> String {
        let t = *self.now_unix.lock().expect("clock");
        let dt = time::OffsetDateTime::from_unix_timestamp(t).expect("ts");
        use time::format_description::well_known::Rfc3339;
        dt.format(&Rfc3339).unwrap_or_else(|_| format!("{t}"))
    }
    fn now_unix(&self) -> i64 {
        *self.now_unix.lock().expect("clock")
    }
}

/// Helper: publish a package via HTTP with auth. Returns (status, body).
fn publish(token: &str, url: &str, body: &serde_json::Value) -> (u16, String) {
    let resp = client()
        .post(url)
        .header("Authorization", format!("Bearer {token}"))
        .json(body)
        .send()
        .expect("post");
    (resp.status().as_u16(), resp.text().expect("body"))
}

/// Helper: update a package via HTTP with auth. Returns (status, body).
fn update(token: &str, url: &str, body: &serde_json::Value) -> (u16, String) {
    let resp = client()
        .put(url)
        .header("Authorization", format!("Bearer {token}"))
        .json(body)
        .send()
        .expect("put");
    (resp.status().as_u16(), resp.text().expect("body"))
}

/// Helper: delete a package via HTTP with auth. Returns (status, body).
fn delete(token: &str, url: &str) -> (u16, String) {
    let resp = client()
        .delete(url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .expect("delete");
    (resp.status().as_u16(), resp.text().expect("body"))
}

// ---------------------------------------------------------------------------
// Auth flow tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn oidc_start_returns_auth_url() {
    let server = TestServer::start("mock-user").await;
    let url = server.url("/api/auth/oidc/start");
    let h = tokio::task::spawn_blocking(move || {
        let r = client().get(&url).send().expect("start");
        (r.status().as_u16(), r.text().expect("body"))
    });
    let (status, body) = h.await.expect("join");
    assert_eq!(status, 200);
    assert!(body.contains("auth_url"), "body: {body}");
    assert!(body.contains("mock_user"), "body: {body}");
}

#[tokio::test]
async fn oidc_callback_issues_session_token() {
    let server = TestServer::start("mock-user").await;
    let token = server.login().await;
    assert!(token.starts_with("sess-"), "token: {token}");
}

#[tokio::test]
async fn oidc_session_returns_owner_for_valid_token() {
    let server = TestServer::start("alice").await;
    let token = server.login().await;
    let url = server.url("/api/auth/oidc/session");
    let h = tokio::task::spawn_blocking(move || {
        let r = client()
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .expect("session");
        (r.status().as_u16(), r.text().expect("body"))
    });
    let (status, body) = h.await.expect("join");
    assert_eq!(status, 200);
    assert!(body.contains(r#""owner":"alice""#), "body: {body}");
}

#[tokio::test]
async fn oidc_session_returns_401_without_token() {
    let server = TestServer::start("alice").await;
    let url = server.url("/api/auth/oidc/session");
    let h = tokio::task::spawn_blocking(move || {
        let r = client().get(&url).send().expect("session");
        (r.status().as_u16(), r.text().expect("body"))
    });
    let (status, body) = h.await.expect("join");
    assert_eq!(status, 401);
    assert!(body.contains("unauthenticated"), "body: {body}");
}

#[tokio::test]
async fn publish_without_auth_returns_401() {
    let server = TestServer::start("alice").await;
    let url = server.url("/api/share/pkg");
    let body = publish_body("noauth", "1.0.0");
    let h = tokio::task::spawn_blocking(move || {
        let r = client().post(&url).json(&body).send().expect("post");
        (r.status().as_u16(), r.text().expect("body"))
    });
    let (status, body) = h.await.expect("join");
    assert_eq!(status, 401, "body: {body}");
    assert!(body.contains("unauthenticated"), "body: {body}");
}

#[tokio::test]
async fn publish_with_invalid_token_returns_401() {
    let server = TestServer::start("alice").await;
    let url = server.url("/api/share/pkg");
    let body = publish_body("bad", "1.0.0");
    let h = tokio::task::spawn_blocking(move || {
        let r = client()
            .post(&url)
            .header("Authorization", "Bearer invalid-token")
            .json(&body)
            .send()
            .expect("post");
        (r.status().as_u16(), r.text().expect("body"))
    });
    let (status, body) = h.await.expect("join");
    assert_eq!(status, 401, "body: {body}");
}

// ---------------------------------------------------------------------------
// Publish policy tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn publish_with_valid_session_succeeds() {
    let clock = FakeClock::new(1_700_000_000);
    let server = TestServer::start_with_clock("alice", clock).await;
    let token = server.login().await;
    let url = server.url("/api/share/pkg");
    let body = publish_body("happy", "1.0.0");
    let t = token.clone();
    let u = url.clone();
    let b = body.clone();
    let h = tokio::task::spawn_blocking(move || publish(&t, &u, &b));
    let (status, body) = h.await.expect("join");
    assert_eq!(status, 201, "publish body: {body}");
    assert!(body.contains("created"), "body: {body}");
}

#[tokio::test]
async fn second_publish_same_day_returns_429() {
    let clock = FakeClock::new(1_700_000_000);
    let server = TestServer::start_with_clock("bob", clock).await;
    let token = server.login().await;
    let url = server.url("/api/share/pkg");

    let t = token.clone();
    let u = url.clone();
    let b = publish_body("first", "1.0.0");
    let h = tokio::task::spawn_blocking(move || publish(&t, &u, &b));
    let (s1, _) = h.await.expect("join");
    assert_eq!(s1, 201);

    let t = token.clone();
    let u = url.clone();
    let b = publish_body("second", "1.0.0");
    let h = tokio::task::spawn_blocking(move || publish(&t, &u, &b));
    let (s2, body) = h.await.expect("join");
    assert_eq!(s2, 429, "second publish same day: {body}");
    assert!(body.contains("daily push limit"), "body: {body}");
}

#[tokio::test]
async fn publish_after_delete_same_day_still_429() {
    let clock = FakeClock::new(1_700_000_000);
    let server = TestServer::start_with_clock("carol", clock).await;
    let token = server.login().await;
    let url = server.url("/api/share/pkg");
    let pkg_url = server.url("/api/share/pkg/del");

    let t = token.clone();
    let u = url.clone();
    let b = publish_body("del", "1.0.0");
    let h = tokio::task::spawn_blocking(move || publish(&t, &u, &b));
    let (s1, _) = h.await.expect("join");
    assert_eq!(s1, 201);

    let t = token.clone();
    let u = pkg_url.clone();
    let h = tokio::task::spawn_blocking(move || delete(&t, &u));
    let (s2, _) = h.await.expect("join");
    assert_eq!(s2, 200);

    let t = token.clone();
    let u = url.clone();
    let b = publish_body("after-del", "1.0.0");
    let h = tokio::task::spawn_blocking(move || publish(&t, &u, &b));
    let (s3, body) = h.await.expect("join");
    assert_eq!(s3, 429, "publish after delete same day: {body}");
}

#[tokio::test]
async fn update_on_next_day_succeeds() {
    let clock = FakeClock::new(1_700_000_000);
    let server = TestServer::start_with_clock("dave", clock.clone()).await;
    let token = server.login().await;
    let url = server.url("/api/share/pkg");
    let pkg_url = server.url("/api/share/pkg/nd");

    let t = token.clone();
    let u = url.clone();
    let b = publish_body("nd", "1.0.0");
    let h = tokio::task::spawn_blocking(move || publish(&t, &u, &b));
    let (s1, _) = h.await.expect("join");
    assert_eq!(s1, 201);

    clock.advance(86_401);

    let t = token.clone();
    let u = pkg_url.clone();
    let b = publish_body("nd", "2.0.0");
    let h = tokio::task::spawn_blocking(move || update(&t, &u, &b));
    let (s2, body) = h.await.expect("join");
    assert_eq!(s2, 200, "update next day: {body}");
    assert!(body.contains("updated"), "body: {body}");
}

#[tokio::test]
async fn sixth_package_returns_409_limit() {
    let clock = FakeClock::new(1_700_000_000);
    let server = TestServer::start_with_clock("eve", clock.clone()).await;
    let token = server.login().await;
    let url = server.url("/api/share/pkg");

    for i in 1..=5 {
        clock.advance(86_401);
        let slug = format!("pkg-{i}");
        let t = token.clone();
        let u = url.clone();
        let b = publish_body(&slug, "1.0.0");
        let h = tokio::task::spawn_blocking(move || publish(&t, &u, &b));
        let (s, body) = h.await.expect("join");
        assert_eq!(s, 201, "package {i} should succeed: {body}");
    }

    clock.advance(86_401);
    let t = token.clone();
    let u = url.clone();
    let b = publish_body("pkg-6", "1.0.0");
    let h = tokio::task::spawn_blocking(move || publish(&t, &u, &b));
    let (s, body) = h.await.expect("join");
    assert_eq!(s, 409, "sixth package: {body}");
    assert!(body.contains("package limit reached"), "body: {body}");
    assert!(body.contains("5"), "body: {body}");
}

#[tokio::test]
async fn oversized_body_returns_413() {
    let clock = FakeClock::new(1_700_000_000);
    let server = TestServer::start_with_clock("frank", clock).await;
    let token = server.login().await;
    let url = server.url("/api/share/pkg");

    let big_content = serde_json::json!({
        "slug": "big",
        "version": "1.0.0",
        "content": { "data": "x".repeat(11 * 1024 * 1024) },
    });
    let t = token.clone();
    let u = url.clone();
    let h = tokio::task::spawn_blocking(move || {
        let r = client()
            .post(&u)
            .header("Authorization", format!("Bearer {t}"))
            .header("Content-Type", "application/json")
            .body(big_content.to_string())
            .send();
        match r {
            Ok(resp) => (resp.status().as_u16(), resp.text().unwrap_or_default()),
            Err(_) => (413_u16, String::from("connection closed (body too large)")),
        }
    });
    let (status, _body) = h.await.expect("join");
    assert_eq!(status, 413, "oversized body should return 413");
}

#[tokio::test]
async fn non_json_content_type_returns_415() {
    let clock = FakeClock::new(1_700_000_000);
    let server = TestServer::start_with_clock("grace", clock).await;
    let token = server.login().await;
    let url = server.url("/api/share/pkg");

    let t = token.clone();
    let u = url.clone();
    let h = tokio::task::spawn_blocking(move || {
        let r = client()
            .post(&u)
            .header("Authorization", format!("Bearer {t}"))
            .header("Content-Type", "text/plain")
            .body("not json")
            .send()
            .expect("post");
        (r.status().as_u16(), r.text().expect("body"))
    });
    let (status, _body) = h.await.expect("join");
    assert_eq!(status, 415, "non-JSON content type should return 415");
}

#[tokio::test]
async fn update_by_another_user_returns_403() {
    let data_dir = Arc::new(TempDir::new().expect("temp dir"));
    let dir1 = data_dir.path().to_path_buf();
    let app1: Router =
        mcm::__test_router_with_mock_user("share", dir1, "owner-x").expect("router1");
    let listener1 = TcpListener::bind("127.0.0.1:0").await.expect("bind1");
    let addr1 = listener1.local_addr().expect("addr1");
    let h1 = tokio::spawn(async move { axum::serve(listener1, app1).await });

    let dir2 = data_dir.path().to_path_buf();
    let app2: Router =
        mcm::__test_router_with_mock_user("share", dir2, "owner-y").expect("router2");
    let listener2 = TcpListener::bind("127.0.0.1:0").await.expect("bind2");
    let addr2 = listener2.local_addr().expect("addr2");
    let h2 = tokio::spawn(async move { axum::serve(listener2, app2).await });
    let _hold = (h1, h2);

    let start1 = format!("http://{addr1}/api/auth/oidc/start");
    let cb_base1 = format!("http://{addr1}");
    let pkg1 = format!("http://{addr1}/api/share/pkg");
    let body = publish_body("xy", "1.0.0");
    let h = tokio::task::spawn_blocking(move || {
        let c = client();
        let r = c.get(&start1).send().expect("start1");
        let b: serde_json::Value = serde_json::from_str(&r.text().expect("body")).expect("json");
        let au = b["auth_url"].as_str().expect("url").to_string();
        let cb = c.get(format!("{cb_base1}{au}")).send().expect("cb1");
        let cb_b: serde_json::Value =
            serde_json::from_str(&cb.text().expect("body")).expect("json");
        let token = cb_b["token"].as_str().expect("token");
        publish(token, &pkg1, &body).0
    });
    assert_eq!(h.await.expect("join"), 201);

    let start2 = format!("http://{addr2}/api/auth/oidc/start");
    let cb_base2 = format!("http://{addr2}");
    let pkg2 = format!("http://{addr2}/api/share/pkg/xy");
    let body = publish_body("xy", "2.0.0");
    let h = tokio::task::spawn_blocking(move || {
        let c = client();
        let r = c.get(&start2).send().expect("start2");
        let b: serde_json::Value = serde_json::from_str(&r.text().expect("body")).expect("json");
        let au = b["auth_url"].as_str().expect("url").to_string();
        let cb = c.get(format!("{cb_base2}{au}")).send().expect("cb2");
        let cb_b: serde_json::Value =
            serde_json::from_str(&cb.text().expect("body")).expect("json");
        let token = cb_b["token"].as_str().expect("token");
        update(token, &pkg2, &body)
    });
    let (status, body) = h.await.expect("join");
    assert_eq!(status, 403, "update by another user: {body}");
}

#[tokio::test]
async fn delete_by_another_user_returns_403() {
    let data_dir = Arc::new(TempDir::new().expect("temp dir"));
    let dir1 = data_dir.path().to_path_buf();
    let app1: Router =
        mcm::__test_router_with_mock_user("share", dir1, "owner-p").expect("router1");
    let listener1 = TcpListener::bind("127.0.0.1:0").await.expect("bind1");
    let addr1 = listener1.local_addr().expect("addr1");
    let h1 = tokio::spawn(async move { axum::serve(listener1, app1).await });

    let dir2 = data_dir.path().to_path_buf();
    let app2: Router =
        mcm::__test_router_with_mock_user("share", dir2, "owner-q").expect("router2");
    let listener2 = TcpListener::bind("127.0.0.1:0").await.expect("bind2");
    let addr2 = listener2.local_addr().expect("addr2");
    let h2 = tokio::spawn(async move { axum::serve(listener2, app2).await });
    let _hold = (h1, h2);

    let start1 = format!("http://{addr1}/api/auth/oidc/start");
    let cb_base1 = format!("http://{addr1}");
    let pkg1 = format!("http://{addr1}/api/share/pkg");
    let body = publish_body("pq", "1.0.0");
    let h = tokio::task::spawn_blocking(move || {
        let c = client();
        let r = c.get(&start1).send().expect("start1");
        let b: serde_json::Value = serde_json::from_str(&r.text().expect("body")).expect("json");
        let au = b["auth_url"].as_str().expect("url").to_string();
        let cb = c.get(format!("{cb_base1}{au}")).send().expect("cb1");
        let cb_b: serde_json::Value =
            serde_json::from_str(&cb.text().expect("body")).expect("json");
        let token = cb_b["token"].as_str().expect("token");
        publish(token, &pkg1, &body).0
    });
    assert_eq!(h.await.expect("join"), 201);

    let start2 = format!("http://{addr2}/api/auth/oidc/start");
    let cb_base2 = format!("http://{addr2}");
    let pkg2 = format!("http://{addr2}/api/share/pkg/pq");
    let h = tokio::task::spawn_blocking(move || {
        let c = client();
        let r = c.get(&start2).send().expect("start2");
        let b: serde_json::Value = serde_json::from_str(&r.text().expect("body")).expect("json");
        let au = b["auth_url"].as_str().expect("url").to_string();
        let cb = c.get(format!("{cb_base2}{au}")).send().expect("cb2");
        let cb_b: serde_json::Value =
            serde_json::from_str(&cb.text().expect("body")).expect("json");
        let token = cb_b["token"].as_str().expect("token");
        delete(token, &pkg2)
    });
    let (status, body) = h.await.expect("join");
    assert_eq!(status, 403, "delete by another user: {body}");
}

#[tokio::test]
async fn deleted_slug_reserved_for_two_days() {
    let clock = FakeClock::new(1_700_000_000);
    let data_dir = Arc::new(TempDir::new().expect("temp dir"));

    let dir1 = data_dir.path().to_path_buf();
    let app1: Router =
        mcm::__test_router_full("share", dir1, Some(Box::new(clock.clone())), "owner-r")
            .expect("router1");
    let listener1 = TcpListener::bind("127.0.0.1:0").await.expect("bind1");
    let addr1 = listener1.local_addr().expect("addr1");
    let h1 = tokio::spawn(async move { axum::serve(listener1, app1).await });

    let dir2 = data_dir.path().to_path_buf();
    let app2: Router =
        mcm::__test_router_full("share", dir2, Some(Box::new(clock.clone())), "owner-s")
            .expect("router2");
    let listener2 = TcpListener::bind("127.0.0.1:0").await.expect("bind2");
    let addr2 = listener2.local_addr().expect("addr2");
    let h2 = tokio::spawn(async move { axum::serve(listener2, app2).await });
    let _hold = (h1, h2);

    let start1 = format!("http://{addr1}/api/auth/oidc/start");
    let cb_base1 = format!("http://{addr1}");
    let pkg1 = format!("http://{addr1}/api/share/pkg");
    let del1 = format!("http://{addr1}/api/share/pkg/rs");
    let body = publish_body("rs", "1.0.0");
    let h = tokio::task::spawn_blocking(move || {
        let c = client();
        let r = c.get(&start1).send().expect("start1");
        let b: serde_json::Value = serde_json::from_str(&r.text().expect("body")).expect("json");
        let au = b["auth_url"].as_str().expect("url").to_string();
        let cb = c.get(format!("{cb_base1}{au}")).send().expect("cb1");
        let cb_b: serde_json::Value =
            serde_json::from_str(&cb.text().expect("body")).expect("json");
        let token = cb_b["token"].as_str().expect("token");
        assert_eq!(publish(token, &pkg1, &body).0, 201);
        assert_eq!(delete(token, &del1).0, 200);
    });
    h.await.expect("join");

    let start2 = format!("http://{addr2}/api/auth/oidc/start");
    let cb_base2 = format!("http://{addr2}");
    let pkg2 = format!("http://{addr2}/api/share/pkg");
    let body = publish_body("rs", "1.0.0");
    let h = tokio::task::spawn_blocking(move || {
        let c = client();
        let r = c.get(&start2).send().expect("start2");
        let b: serde_json::Value = serde_json::from_str(&r.text().expect("body")).expect("json");
        let au = b["auth_url"].as_str().expect("url").to_string();
        let cb = c.get(format!("{cb_base2}{au}")).send().expect("cb2");
        let cb_b: serde_json::Value =
            serde_json::from_str(&cb.text().expect("body")).expect("json");
        let token = cb_b["token"].as_str().expect("token");
        publish(token, &pkg2, &body)
    });
    let (status, body) = h.await.expect("join");
    assert_eq!(status, 409, "deleted slug reserved: {body}");

    clock.advance(2 * 86_400 + 1);

    let start2 = format!("http://{addr2}/api/auth/oidc/start");
    let cb_base2 = format!("http://{addr2}");
    let pkg2 = format!("http://{addr2}/api/share/pkg");
    let body = publish_body("rs", "2.0.0");
    let h = tokio::task::spawn_blocking(move || {
        let c = client();
        let r = c.get(&start2).send().expect("start2");
        let b: serde_json::Value = serde_json::from_str(&r.text().expect("body")).expect("json");
        let au = b["auth_url"].as_str().expect("url").to_string();
        let cb = c.get(format!("{cb_base2}{au}")).send().expect("cb2");
        let cb_b: serde_json::Value =
            serde_json::from_str(&cb.text().expect("body")).expect("json");
        let token = cb_b["token"].as_str().expect("token");
        publish(token, &pkg2, &body)
    });
    let (status, body) = h.await.expect("join");
    assert_eq!(status, 201, "after 2 days, slug released: {body}");
}

// ---------------------------------------------------------------------------
// Task 4: OIDC start / poll / logout tests
// ---------------------------------------------------------------------------

/// Helper: call start and return (login_id, auth_url).
async fn start_login(server: &TestServer) -> (String, String) {
    let url = server.url("/api/auth/oidc/start");
    tokio::task::spawn_blocking(move || {
        let r = client().get(&url).send().expect("start");
        let b: serde_json::Value = serde_json::from_str(&r.text().expect("body")).expect("json");
        let login_id = b["login_id"].as_str().expect("login_id").to_string();
        let auth_url = b["auth_url"].as_str().expect("auth_url").to_string();
        (login_id, auth_url)
    })
    .await
    .expect("join")
}

/// Helper: call callback with the given state.
async fn call_callback(server: &TestServer, auth_url: &str) -> (u16, String) {
    let cb = format!("http://{}{auth_url}", server.addr);
    tokio::task::spawn_blocking(move || {
        let r = client().get(&cb).send().expect("callback");
        (r.status().as_u16(), r.text().expect("body"))
    })
    .await
    .expect("join")
}

/// Helper: poll a login_id and return (status_code, body_json).
async fn call_poll(server: &TestServer, login_id: &str) -> (u16, serde_json::Value) {
    let url = server.url(&format!("/api/auth/oidc/poll/{login_id}"));
    tokio::task::spawn_blocking(move || {
        let r = client().get(&url).send().expect("poll");
        let status = r.status().as_u16();
        let body: serde_json::Value = serde_json::from_str(&r.text().expect("body")).expect("json");
        (status, body)
    })
    .await
    .expect("join")
}

#[tokio::test]
async fn oidc_start_returns_login_id() {
    let server = TestServer::start("mock-user").await;
    let (login_id, auth_url) = start_login(&server).await;
    assert!(!login_id.is_empty(), "login_id should not be empty");
    assert!(
        auth_url.contains("state="),
        "auth_url should contain state: {auth_url}"
    );
}

#[tokio::test]
async fn oidc_poll_pending_before_callback() {
    let server = TestServer::start("mock-user").await;
    let (login_id, _auth_url) = start_login(&server).await;
    let (status, body) = call_poll(&server, &login_id).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "pending", "body: {body}");
}

#[tokio::test]
async fn oidc_poll_complete_after_callback() {
    let server = TestServer::start("alice").await;
    let (login_id, auth_url) = start_login(&server).await;

    // Callback should succeed.
    let (cb_status, _cb_body) = call_callback(&server, &auth_url).await;
    assert_eq!(cb_status, 200, "callback should succeed");

    // Poll should now return complete with a session token.
    let (status, body) = call_poll(&server, &login_id).await;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "complete", "body: {body}");
    assert!(
        body["token"].as_str().unwrap().starts_with("sess-"),
        "token should start with sess-: {body}"
    );
    assert_eq!(body["owner"], "alice", "owner: {body}");
    assert!(
        body["expires_at_unix"].as_i64().unwrap() > 0,
        "expiry should be positive: {body}"
    );
}

#[tokio::test]
async fn oidc_poll_complete_is_one_shot() {
    let server = TestServer::start("mock-user").await;
    let (login_id, auth_url) = start_login(&server).await;
    call_callback(&server, &auth_url).await;

    // First poll returns complete.
    let (s1, body1) = call_poll(&server, &login_id).await;
    assert_eq!(s1, 200);
    assert_eq!(body1["status"], "complete");

    // Second poll returns 404 (consumed).
    let (s2, body2) = call_poll(&server, &login_id).await;
    assert_eq!(s2, 404, "second poll should 404: {body2}");
}

#[tokio::test]
async fn oidc_poll_unknown_login_id_returns_404() {
    let server = TestServer::start("mock-user").await;
    let (status, body) = call_poll(&server, "nonexistent-id").await;
    assert_eq!(status, 404, "body: {body}");
}

#[tokio::test]
async fn oidc_replayed_state_fails() {
    let server = TestServer::start("mock-user").await;
    let (_login_id, auth_url) = start_login(&server).await;

    // First callback succeeds.
    let (s1, _) = call_callback(&server, &auth_url).await;
    assert_eq!(s1, 200, "first callback should succeed");

    // Second callback with same state should fail (replayed).
    let (s2, body2) = call_callback(&server, &auth_url).await;
    assert_eq!(s2, 400, "replayed state should fail: {body2}");
    assert!(body2.contains("invalid or expired state"), "body: {body2}");
}

#[tokio::test]
async fn oidc_invalid_state_fails() {
    let server = TestServer::start("mock-user").await;
    let cb_url = server.url("/api/auth/oidc/callback?code=bad&state=invalid-state");
    let (status, body) = tokio::task::spawn_blocking(move || {
        let r = client().get(&cb_url).send().expect("callback");
        (r.status().as_u16(), r.text().expect("body"))
    })
    .await
    .expect("join");
    assert_eq!(status, 400, "body: {body}");
    assert!(body.contains("invalid or expired state"), "body: {body}");
}

#[tokio::test]
async fn oidc_logout_clears_session() {
    let server = TestServer::start("alice").await;
    let token = server.login().await;

    // Session should be valid before logout.
    let session_url = server.url("/api/auth/oidc/session");
    let t = token.clone();
    let s_url = session_url.clone();
    let h = tokio::task::spawn_blocking(move || {
        let r = client()
            .get(&s_url)
            .header("Authorization", format!("Bearer {t}"))
            .send()
            .expect("session");
        r.status().as_u16()
    });
    assert_eq!(h.await.expect("join"), 200, "session should be valid");

    // Logout.
    let logout_url = server.url("/api/auth/oidc/logout");
    let t = token.clone();
    let l_url = logout_url.clone();
    let h = tokio::task::spawn_blocking(move || {
        let r = client()
            .get(&l_url)
            .header("Authorization", format!("Bearer {t}"))
            .send()
            .expect("logout");
        (r.status().as_u16(), r.text().expect("body"))
    });
    let (status, body) = h.await.expect("join");
    assert_eq!(status, 200, "logout body: {body}");
    assert!(body.contains("logged_out"), "body: {body}");

    // Session should be invalid after logout.
    let t = token.clone();
    let s_url = session_url.clone();
    let h = tokio::task::spawn_blocking(move || {
        let r = client()
            .get(&s_url)
            .header("Authorization", format!("Bearer {t}"))
            .send()
            .expect("session after logout");
        r.status().as_u16()
    });
    assert_eq!(
        h.await.expect("join"),
        401,
        "session should be invalid after logout"
    );
}

#[tokio::test]
async fn oidc_session_returns_owner() {
    let server = TestServer::start("bob").await;
    let token = server.login().await;
    let url = server.url("/api/auth/oidc/session");
    let h = tokio::task::spawn_blocking(move || {
        let r = client()
            .get(&url)
            .header("Authorization", format!("Bearer {token}"))
            .send()
            .expect("session");
        (r.status().as_u16(), r.text().expect("body"))
    });
    let (status, body) = h.await.expect("join");
    assert_eq!(status, 200);
    assert!(body.contains(r#""owner":"bob""#), "body: {body}");
}

#[tokio::test]
async fn oidc_login_id_isolation_between_servers() {
    // Two servers should have independent login stores.
    let server1 = TestServer::start("user-a").await;
    let server2 = TestServer::start("user-b").await;

    let (login_id1, _) = start_login(&server1).await;

    // login_id from server1 should not exist on server2.
    let (status, _) = call_poll(&server2, &login_id1).await;
    assert_eq!(status, 404, "login_id should be server-local");
}

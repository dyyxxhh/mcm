//! Integration tests for source-mode service routes + `pkg install` from
//! imported sources.
//!
//! Covers:
//! - `GET /api/source/index` serves the operator-authored index JSON.
//! - `GET /api/source/meta/{slug}` serves package metadata.
//! - `GET /api/source/blob/{slug}` serves raw artifact bytes.
//! - Missing index → 404; missing blob → 404; missing meta → 404.
//! - `source add <local-service-url> --yes` imports the local source.
//! - `pkg install <slug> --yes` resolves + installs from an external-URL
//!   artifact declared in the source.
//! - `pkg install <slug> --yes` resolves + installs from a source-hosted
//!   blob (no external download_url).
//! - Hash mismatch from a trusted source aborts as an integrity/corruption
//!   error, NOT a hostile-source warning.
//! - Source routes work in `both` mode alongside share routes.
//!
//! Test shape: start the source-mode server on a random port with a temp
//! data_dir containing a `source-index.json` + blob files, then drive the
//! CLI via `assert_cmd` against the live server URL.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use assert_cmd::Command;
use axum::Router;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

struct TestServer {
    addr: SocketAddr,
    _handle: JoinHandle<()>,
    _data_dir: Arc<TempDir>,
}

impl TestServer {
    async fn start(mode: &str, data_dir: PathBuf) -> Self {
        let app: Router =
            mcm::__test_router_with_data_dir(mode, data_dir).expect("build test router");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server run");
        });
        Self {
            addr,
            _handle: handle,
            _data_dir: Arc::new(TempDir::new().expect("temp")),
        }
    }

    fn index_url(&self) -> String {
        format!("http://{}/api/source/index", self.addr)
    }

    fn blob_url(&self, slug: &str) -> String {
        format!("http://{}/api/source/blob/{}", self.addr, slug)
    }

    fn meta_url(&self, slug: &str) -> String {
        format!("http://{}/api/source/meta/{}", self.addr, slug)
    }
}

fn blocking_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("client")
}

struct TestHome {
    #[allow(dead_code)]
    root: TempDir,
    config: PathBuf,
    state: PathBuf,
    mods: PathBuf,
}

impl TestHome {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let config = root.path().join("config");
        let state = root.path().join("state");
        let mods = root.path().join("mods");
        std::fs::create_dir_all(&mods).expect("mods dir");
        Self {
            root,
            config,
            state,
            mods,
        }
    }
}

fn run_cli(args: &[&str], config: &Path, state: &Path) -> std::process::Output {
    Command::cargo_bin("mcm")
        .expect("mcm")
        .args([
            "--config-dir",
            config.to_str().unwrap(),
            "--state-dir",
            state.to_str().unwrap(),
            "--provider",
            "mock",
        ])
        .args(args)
        .output()
        .expect("run mcm")
}

fn setup_profile(config: &Path, state: &Path, mods: &Path) {
    let out = run_cli(
        &[
            "mods",
            "add",
            "dev",
            "--mods-dir",
            mods.to_str().unwrap(),
            "--mc-version",
            "1.20.1",
            "--loader",
            "fabric",
        ],
        config,
        state,
    );
    assert!(out.status.success(), "profile setup failed");
}

fn write_index(data_dir: &std::path::Path, json: &str) {
    std::fs::write(data_dir.join("source-index.json"), json).expect("write index");
}

fn write_blob(data_dir: &std::path::Path, slug: &str, bytes: &[u8]) {
    let blobs = data_dir.join("source-blobs");
    std::fs::create_dir_all(&blobs).expect("blobs dir");
    std::fs::write(blobs.join(slug), bytes).expect("write blob");
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

fn index_with_external_url(slug: &str, hash: &str, download_url: &str) -> String {
    format!(
        r#"{{
    "schema_version": 1,
    "source_id": "test-source",
    "capabilities": ["mods"],
    "packages": [
        {{
            "id": "{slug}",
            "title": "Test Mod",
            "versions": [
                {{
                    "version": "1.0.0",
                    "mc_versions": ["1.20.1"],
                    "loaders": ["fabric"],
                    "side": "both",
                    "filename": "{slug}-1.0.0.jar",
                    "download_url": "{download_url}",
                    "sha256": "{hash}",
                    "size": 42
                }}
            ]
        }}
    ]
}}"#
    )
}

fn index_with_blob_ref(slug: &str, blob_ref: &str, hash: &str) -> String {
    format!(
        r#"{{
    "schema_version": 1,
    "source_id": "test-source",
    "capabilities": ["mods"],
    "packages": [
        {{
            "id": "{slug}",
            "title": "Hosted Mod",
            "versions": [
                {{
                    "version": "1.0.0",
                    "mc_versions": ["1.20.1"],
                    "loaders": ["fabric"],
                    "side": "both",
                    "filename": "{slug}-1.0.0.jar",
                    "blob_ref": "{blob_ref}",
                    "sha256": "{hash}",
                    "size": 42
                }}
            ]
        }}
    ]
}}"#
    )
}

// ---------------------------------------------------------------------------
// Route tests: index / meta / blob
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_index_serves_configured_index_json() {
    let data_dir = TempDir::new().expect("temp");
    write_index(
        data_dir.path(),
        r#"{"schema_version":1,"source_id":"s","packages":[]}"#,
    );
    let server = TestServer::start("source", data_dir.path().to_path_buf()).await;
    let url = server.index_url();
    let (status, source_id) = tokio::task::spawn_blocking(move || {
        let resp = blocking_client().get(&url).send().expect("get");
        let status = resp.status();
        let body: serde_json::Value = resp.json().expect("json");
        (status, body["source_id"].as_str().unwrap_or("").to_owned())
    })
    .await
    .expect("join");
    assert_eq!(status, 200);
    assert_eq!(source_id, "s");
}

#[tokio::test]
async fn get_index_returns_404_when_not_configured() {
    let data_dir = TempDir::new().expect("temp");
    let server = TestServer::start("source", data_dir.path().to_path_buf()).await;
    let url = server.index_url();
    let (status, error) = tokio::task::spawn_blocking(move || {
        let resp = blocking_client().get(&url).send().expect("get");
        let status = resp.status();
        let body: serde_json::Value = resp.json().expect("json");
        (status, body["error"].as_str().unwrap_or("").to_owned())
    })
    .await
    .expect("join");
    assert_eq!(status, 404);
    assert_eq!(error, "source index not configured");
}

#[tokio::test]
async fn get_meta_returns_package_metadata() {
    let data_dir = TempDir::new().expect("temp");
    write_index(
        data_dir.path(),
        r#"{"schema_version":1,"source_id":"s","packages":[
            {"id":"alpha","title":"Alpha","versions":[]}
        ]}"#,
    );
    let server = TestServer::start("source", data_dir.path().to_path_buf()).await;
    let url = server.meta_url("alpha");
    let (status, id, title) = tokio::task::spawn_blocking(move || {
        let resp = blocking_client().get(&url).send().expect("get");
        let status = resp.status();
        let body: serde_json::Value = resp.json().expect("json");
        (
            status,
            body["id"].as_str().unwrap_or("").to_owned(),
            body["title"].as_str().unwrap_or("").to_owned(),
        )
    })
    .await
    .expect("join");
    assert_eq!(status, 200);
    assert_eq!(id, "alpha");
    assert_eq!(title, "Alpha");
}

#[tokio::test]
async fn get_meta_returns_404_for_unknown_slug() {
    let data_dir = TempDir::new().expect("temp");
    write_index(
        data_dir.path(),
        r#"{"schema_version":1,"source_id":"s","packages":[]}"#,
    );
    let server = TestServer::start("source", data_dir.path().to_path_buf()).await;
    let url = server.meta_url("nope");
    let status = tokio::task::spawn_blocking(move || {
        blocking_client().get(&url).send().expect("get").status()
    })
    .await
    .expect("join");
    assert_eq!(status, 404);
}

#[tokio::test]
async fn get_blob_serves_raw_bytes() {
    let data_dir = TempDir::new().expect("temp");
    write_blob(data_dir.path(), "alpha", b"jar-bytes-here");
    let server = TestServer::start("source", data_dir.path().to_path_buf()).await;
    let url = server.blob_url("alpha");
    let (status, content_type, bytes) = tokio::task::spawn_blocking(move || {
        let resp = blocking_client().get(&url).send().expect("get");
        let status = resp.status();
        let content_type = resp
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or("").to_owned())
            .unwrap_or_default();
        let bytes = resp.bytes().expect("body").to_vec();
        (status, content_type, bytes)
    })
    .await
    .expect("join");
    assert_eq!(status, 200);
    assert_eq!(content_type, "application/octet-stream");
    assert_eq!(bytes, b"jar-bytes-here");
}

#[tokio::test]
async fn get_blob_returns_404_for_missing() {
    let data_dir = TempDir::new().expect("temp");
    let server = TestServer::start("source", data_dir.path().to_path_buf()).await;
    let url = server.blob_url("ghost");
    let status = tokio::task::spawn_blocking(move || {
        blocking_client().get(&url).send().expect("get").status()
    })
    .await
    .expect("join");
    assert_eq!(status, 404);
}

#[tokio::test]
async fn source_routes_disabled_in_share_mode() {
    let data_dir = TempDir::new().expect("temp");
    write_index(
        data_dir.path(),
        r#"{"schema_version":1,"source_id":"s","packages":[]}"#,
    );
    let server = TestServer::start("share", data_dir.path().to_path_buf()).await;
    let url = server.index_url();
    let (status, error) = tokio::task::spawn_blocking(move || {
        let resp = blocking_client().get(&url).send().expect("get");
        let status = resp.status();
        let body: serde_json::Value = resp.json().expect("json");
        (status, body["error"].as_str().unwrap_or("").to_owned())
    })
    .await
    .expect("join");
    assert_eq!(status, 404);
    assert!(error.contains("source mode disabled"));
}

#[tokio::test]
async fn source_routes_work_in_both_mode() {
    let data_dir = TempDir::new().expect("temp");
    write_index(
        data_dir.path(),
        r#"{"schema_version":1,"source_id":"s","packages":[]}"#,
    );
    let server = TestServer::start("both", data_dir.path().to_path_buf()).await;
    let url = server.index_url();
    let (status, source_id) = tokio::task::spawn_blocking(move || {
        let resp = blocking_client().get(&url).send().expect("get");
        let status = resp.status();
        let body: serde_json::Value = resp.json().expect("json");
        (status, body["source_id"].as_str().unwrap_or("").to_owned())
    })
    .await
    .expect("join");
    assert_eq!(status, 200);
    assert_eq!(source_id, "s");
}

// ---------------------------------------------------------------------------
// CLI integration: source add + pkg install from source
// ---------------------------------------------------------------------------

#[tokio::test]
async fn source_add_local_service_index_succeeds() {
    let data_dir = TempDir::new().expect("temp");
    write_index(
        data_dir.path(),
        r#"{"schema_version":1,"source_id":"s","packages":[]}"#,
    );
    let server = TestServer::start("source", data_dir.path().to_path_buf()).await;
    let url = server.index_url();
    let home = Arc::new(TestHome::new());
    let config = home.config.clone();
    let state = home.state.clone();
    let url_clone = url.clone();
    tokio::task::spawn_blocking(move || {
        let out = run_cli(&["source", "add", &url_clone, "--yes"], &config, &state);
        assert!(out.status.success(), "source add failed");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("added source"));
        let out = run_cli(&["source", "list"], &config, &state);
        assert!(out.status.success());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains(&url_clone));
    })
    .await
    .expect("join");
    let _ = home;
}

#[tokio::test]
async fn pkg_install_from_source_external_url_succeeds() {
    let blob = b"mock mcm jar\nid=extmod\nversion=1.0.0\n";
    let hash = sha256_hex(blob);
    let data_dir = TempDir::new().expect("temp");
    let slug = "extmod";
    let server = TestServer::start("source", data_dir.path().to_path_buf()).await;
    let download_url = server.blob_url(slug);
    let index_json = index_with_external_url(slug, &hash, &download_url);
    write_index(data_dir.path(), &index_json);
    write_blob(data_dir.path(), slug, blob);

    let url = server.index_url();
    let home = Arc::new(TestHome::new());
    let config = home.config.clone();
    let state = home.state.clone();
    let mods = home.mods.clone();
    let slug_owned = slug.to_owned();
    tokio::task::spawn_blocking(move || {
        setup_profile(&config, &state, &mods);
        let out = run_cli(&["source", "add", &url, "--yes"], &config, &state);
        assert!(out.status.success(), "source add failed");
        let out = run_cli(&["pkg", "install", &slug_owned, "--yes"], &config, &state);
        assert!(out.status.success(), "pkg install failed");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("installed package"));
        assert!(mods.join(format!("{slug_owned}-1.0.0.jar")).exists());
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn pkg_install_from_source_hosted_blob_succeeds() {
    let blob = b"mock mcm jar\nid=hostmod\nversion=1.0.0\n";
    let hash = sha256_hex(blob);
    let data_dir = TempDir::new().expect("temp");
    let slug = "hostmod";
    let blob_ref = "hostmod-blob";
    let index_json = index_with_blob_ref(slug, blob_ref, &hash);
    write_index(data_dir.path(), &index_json);
    write_blob(data_dir.path(), blob_ref, blob);

    let server = TestServer::start("source", data_dir.path().to_path_buf()).await;
    let url = server.index_url();
    let home = Arc::new(TestHome::new());
    let config = home.config.clone();
    let state = home.state.clone();
    let mods = home.mods.clone();
    let slug_owned = slug.to_owned();
    tokio::task::spawn_blocking(move || {
        setup_profile(&config, &state, &mods);
        let out = run_cli(&["source", "add", &url, "--yes"], &config, &state);
        assert!(out.status.success(), "source add failed");
        let out = run_cli(&["pkg", "install", &slug_owned, "--yes"], &config, &state);
        assert!(out.status.success(), "pkg install failed");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("installed package"));
        assert!(mods.join(format!("{slug_owned}-1.0.0.jar")).exists());
    })
    .await
    .expect("join");
}

#[tokio::test]
async fn pkg_install_hash_mismatch_aborts_as_corruption_not_hostile() {
    let real_blob = b"real bytes\n";
    let wrong_hash = sha256_hex(b"different bytes entirely");
    let data_dir = TempDir::new().expect("temp");
    let slug = "badmod";
    let blob_ref = "badmod-blob";
    let index_json = index_with_blob_ref(slug, blob_ref, &wrong_hash);
    write_index(data_dir.path(), &index_json);
    write_blob(data_dir.path(), blob_ref, real_blob);

    let server = TestServer::start("source", data_dir.path().to_path_buf()).await;
    let url = server.index_url();
    let home = Arc::new(TestHome::new());
    let config = home.config.clone();
    let state = home.state.clone();
    let mods = home.mods.clone();
    let slug_owned = slug.to_owned();
    let stderr = tokio::task::spawn_blocking(move || {
        setup_profile(&config, &state, &mods);
        let out = run_cli(&["source", "add", &url, "--yes"], &config, &state);
        assert!(out.status.success(), "source add failed");
        let out = run_cli(&["pkg", "install", &slug_owned, "--yes"], &config, &state);
        assert!(!out.status.success(), "pkg install should have failed");
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(
            !mods.join(format!("{slug_owned}-1.0.0.jar")).exists(),
            "corrupted jar should not be written"
        );
        stderr
    })
    .await
    .expect("join");
    let lower = stderr.to_ascii_lowercase();
    assert!(
        lower.contains("integrity")
            || lower.contains("hash")
            || lower.contains("mismatch")
            || lower.contains("corrupt"),
        "stderr should mention integrity/hash/corruption, got: {stderr}"
    );
    assert!(
        !lower.contains("untrusted"),
        "stderr must NOT say 'untrusted', got: {stderr}"
    );
    assert!(
        !lower.contains("hostile"),
        "stderr must NOT say 'hostile', got: {stderr}"
    );
}

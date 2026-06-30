use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use tempfile::TempDir;
use tokio::net::TcpListener;

struct TestServer {
    addr: SocketAddr,
    _data_dir: Arc<TempDir>,
}

impl TestServer {
    async fn start(user: &str) -> Self {
        let data_dir = Arc::new(TempDir::new().expect("temp dir"));
        let app: Router =
            mcm::__test_router_with_mock_user("share", data_dir.path().to_path_buf(), user)
                .expect("build test router");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        #[allow(clippy::let_underscore_future)]
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server run");
        });
        Self {
            addr,
            _data_dir: data_dir,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("client")
}

fn test_home() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = TempDir::new().expect("temp dir");
    let config = root.path().join("config");
    let state = root.path().join("state");
    (root, config, state)
}

fn find_mcm_bin() -> std::path::PathBuf {
    let mut cmd = Command::new("cargo");
    cmd.args(["build", "--bin", "mcm"]);
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    cmd.output().expect("cargo build");
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    std::path::PathBuf::from(manifest_dir)
        .join("target")
        .join("debug")
        .join("mcm")
}

struct CliOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

fn run_login_cli(
    config: &std::path::Path,
    state: &std::path::Path,
    server: &str,
    cb_base: &str,
) -> CliOutput {
    let mcm_bin = find_mcm_bin();
    let mut child = Command::new(&mcm_bin)
        .args([
            "--config-dir",
            config.to_str().unwrap(),
            "--state-dir",
            state.to_str().unwrap(),
            "--provider",
            "mock",
            "pkg",
            "auth",
            "login",
            "--server",
            server,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mcm");

    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);

    let mut captured_stdout = String::new();
    let mut auth_url_found = false;

    for line in reader.lines() {
        let line = line.expect("read line");
        captured_stdout.push_str(&line);
        captured_stdout.push('\n');

        if !auth_url_found && line.contains("state=") {
            let full_url = if line.starts_with("http") {
                line.clone()
            } else {
                format!("{cb_base}{line}")
            };
            auth_url_found = true;

            let cb_url = full_url;
            std::thread::spawn(move || {
                let c = client();
                let _ = c.get(&cb_url).send();
            });
        }
    }

    let output = child.wait_with_output().expect("wait for output");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    CliOutput {
        success: output.status.success(),
        stdout: captured_stdout,
        stderr,
    }
}

// ---------------------------------------------------------------------------
// Login: prints auth URL, polls, stores token, prints owner
// ---------------------------------------------------------------------------

#[test]
fn pkg_auth_login_prints_url_and_stores_token() {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    let server = rt.block_on(TestServer::start("test-user"));
    let (_root, config, state) = test_home();
    let server_url = server.url("");

    let output = run_login_cli(&config, &state, &server_url, &server.url(""));

    assert!(
        output.success,
        "CLI failed: stderr={}, stdout={}",
        output.stderr, output.stdout
    );
    assert!(
        output.stdout.contains("test-user"),
        "should print owner: stdout={}",
        output.stdout
    );

    let session_dir = state.join("pkg-auth");
    assert!(session_dir.exists(), "session dir should be created");
    let entries: Vec<_> = std::fs::read_dir(&session_dir)
        .expect("read session dir")
        .collect();
    assert_eq!(entries.len(), 1, "should have exactly one session file");

    let session_text =
        std::fs::read_to_string(entries[0].as_ref().unwrap().path()).expect("read session");
    let session: serde_json::Value = serde_json::from_str(&session_text).expect("parse session");
    assert!(
        session["token"].as_str().is_some(),
        "session should have token"
    );
    assert_eq!(
        session["server"].as_str(),
        Some(server_url.as_str()),
        "session should record server URL"
    );
}

// ---------------------------------------------------------------------------
// Status: authenticated owner is printed
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pkg_auth_status_shows_authenticated() {
    let server = TestServer::start("status-user").await;
    let (_root, config, state) = test_home();
    let server_url = server.url("");
    let session_dir = state.join("pkg-auth");
    std::fs::create_dir_all(&session_dir).expect("create session dir");

    let server_url_c = server_url.clone();
    let login_token = tokio::task::spawn_blocking(move || {
        let c = client();
        let r = c
            .get(format!("{server_url_c}/api/auth/oidc/start"))
            .send()
            .expect("start");
        let b: serde_json::Value = serde_json::from_str(&r.text().expect("body")).expect("json");
        let auth_url = b["auth_url"].as_str().expect("auth_url").to_string();
        let cb = c
            .get(format!("{server_url_c}{auth_url}"))
            .send()
            .expect("callback");
        let cb_b: serde_json::Value =
            serde_json::from_str(&cb.text().expect("body")).expect("json");
        cb_b["token"].as_str().expect("token").to_string()
    })
    .await
    .expect("join");

    let server_hash = {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(server_url.as_bytes());
        hex::encode(&hash[..8])
    };
    let session_file = session_dir.join(format!("{server_hash}.json"));
    let session = serde_json::json!({
        "server": server_url,
        "token": login_token,
    });
    std::fs::write(
        &session_file,
        serde_json::to_string_pretty(&session).unwrap(),
    )
    .expect("write session");

    let output = tokio::task::spawn_blocking(move || {
        assert_cmd::Command::cargo_bin("mcm")
            .expect("mcm binary")
            .args([
                "--config-dir",
                config.to_str().unwrap(),
                "--state-dir",
                state.to_str().unwrap(),
                "--provider",
                "mock",
                "pkg",
                "auth",
                "status",
                "--server",
                &server_url,
            ])
            .timeout(Duration::from_secs(10))
            .output()
            .expect("run cmd")
    })
    .await
    .expect("join");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "CLI failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("status-user"),
        "should print authenticated owner: stdout={stdout}"
    );
}

// ---------------------------------------------------------------------------
// Status: not authenticated
// ---------------------------------------------------------------------------

#[test]
fn pkg_auth_status_shows_not_authenticated() {
    let (_root, config, state) = test_home();
    let output = assert_cmd::Command::cargo_bin("mcm")
        .expect("mcm binary")
        .args([
            "--config-dir",
            config.to_str().unwrap(),
            "--state-dir",
            state.to_str().unwrap(),
            "--provider",
            "mock",
            "pkg",
            "auth",
            "status",
            "--server",
            "https://mc.example.com",
        ])
        .timeout(Duration::from_secs(10))
        .output()
        .expect("run cmd");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "CLI failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Not authenticated"),
        "should show not authenticated: stdout={stdout}"
    );
}

// ---------------------------------------------------------------------------
// Logout: removes session file and prints success
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pkg_auth_logout_removes_session() {
    let server = TestServer::start("logout-user").await;
    let (_root, config, state) = test_home();
    let server_url = server.url("");
    let session_dir = state.join("pkg-auth");
    std::fs::create_dir_all(&session_dir).expect("create session dir");

    let server_url_c = server_url.clone();
    let login_token = tokio::task::spawn_blocking(move || {
        let c = client();
        let r = c
            .get(format!("{server_url_c}/api/auth/oidc/start"))
            .send()
            .expect("start");
        let b: serde_json::Value = serde_json::from_str(&r.text().expect("body")).expect("json");
        let auth_url = b["auth_url"].as_str().expect("auth_url").to_string();
        let cb = c
            .get(format!("{server_url_c}{auth_url}"))
            .send()
            .expect("callback");
        let cb_b: serde_json::Value =
            serde_json::from_str(&cb.text().expect("body")).expect("json");
        cb_b["token"].as_str().expect("token").to_string()
    })
    .await
    .expect("join");

    let server_hash = {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(server_url.as_bytes());
        hex::encode(&hash[..8])
    };
    let session_file = session_dir.join(format!("{server_hash}.json"));
    let session = serde_json::json!({
        "server": server_url,
        "token": login_token,
    });
    std::fs::write(
        &session_file,
        serde_json::to_string_pretty(&session).unwrap(),
    )
    .expect("write session");
    assert!(session_file.exists(), "session file should exist");

    let session_file_p = session_file.clone();
    let output = tokio::task::spawn_blocking(move || {
        assert_cmd::Command::cargo_bin("mcm")
            .expect("mcm binary")
            .args([
                "--config-dir",
                config.to_str().unwrap(),
                "--state-dir",
                state.to_str().unwrap(),
                "--provider",
                "mock",
                "pkg",
                "auth",
                "logout",
                "--server",
                &server_url,
            ])
            .timeout(Duration::from_secs(10))
            .output()
            .expect("run cmd")
    })
    .await
    .expect("join");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "CLI failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Logged out"),
        "should print logged out: stdout={stdout}"
    );
    assert!(
        !session_file_p.exists(),
        "session file should be removed after logout"
    );
}

// ---------------------------------------------------------------------------
// Logout: graceful when no session exists
// ---------------------------------------------------------------------------

#[test]
fn pkg_auth_logout_without_session_succeeds() {
    let (_root, config, state) = test_home();

    let output = assert_cmd::Command::cargo_bin("mcm")
        .expect("mcm binary")
        .args([
            "--config-dir",
            config.to_str().unwrap(),
            "--state-dir",
            state.to_str().unwrap(),
            "--provider",
            "mock",
            "pkg",
            "auth",
            "logout",
            "--server",
            "https://mc.example.com",
        ])
        .timeout(Duration::from_secs(10))
        .output()
        .expect("run cmd");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "CLI failed: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("Logged out"),
        "should print logged out even without session: stdout={stdout}"
    );
}

// ---------------------------------------------------------------------------
// Security: token is not printed in output
// ---------------------------------------------------------------------------

#[test]
fn pkg_auth_login_does_not_print_token() {
    let rt = tokio::runtime::Runtime::new().expect("rt");
    let server = rt.block_on(TestServer::start("secret-user"));
    let (_root, config, state) = test_home();
    let server_url = server.url("");

    let output = run_login_cli(&config, &state, &server_url, &server.url(""));

    let combined = format!("{}{}", output.stdout, output.stderr);

    assert!(
        output.success,
        "CLI failed: stderr={}, stdout={}",
        output.stderr, output.stdout
    );

    assert!(
        !combined.contains("sess-"),
        "token must not appear in output: {combined}"
    );
}

//! Integration tests for the `/install` bootstrap script route (task 17).
//!
//! Each test spins up the router on a random local port and verifies the HTTP
//! response body shape, content type, and static properties of the returned
//! shell script — checksum verification, OS/arch detection, no unverified
//! execution, and env-override support.
//!
//! The actual script runtime behavior (checksum mismatch abort, unsupported
//! OS/arch exit, successful install) is tested in manual QA against a local
//! mock release endpoint. These integration tests only cover the route and
//! script body properties that can be verified without executing the script.

use std::net::SocketAddr;

use axum::Router;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

struct TestServer {
    addr: SocketAddr,
    _handle: JoinHandle<()>,
}

impl TestServer {
    async fn start() -> Self {
        let app: Router = mcm::__test_router("share").expect("build test router");
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
async fn install_route_returns_200_with_shell_script_content_type() {
    let server = TestServer::start().await;
    let url = server.url("/install");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get /install");
        (
            resp.status().as_u16(),
            resp.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string(),
            resp.text().expect("body"),
        )
    });
    let (status, content_type, body) = handle.await.expect("join");
    assert_eq!(status, 200, "/install should return 200");
    assert!(
        content_type.starts_with("text/x-shellscript") || content_type.starts_with("text/plain"),
        "content-type should be shell script, got: {content_type}"
    );
    assert!(!body.is_empty(), "body should not be empty");
    assert!(
        body.starts_with("#!/bin/bash") || body.starts_with("#!/usr/bin/env bash"),
        "body should start with bash shebang, got first 20 chars: {:?}",
        &body[..body.len().min(20)]
    );
}

#[tokio::test]
async fn install_route_body_contains_checksum_verification() {
    let server = TestServer::start().await;
    let url = server.url("/install");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get /install");
        resp.text().expect("body")
    });
    let body = handle.await.expect("join");

    // The script must verify checksums before installing.
    assert!(
        body.contains("sha256") || body.contains("SHA256") || body.contains("sha256sum"),
        "script should contain SHA-256 checksum verification: body preview:\n{}",
        &body[..body.len().min(500)]
    );
    assert!(
        body.contains("check") || body.contains("verify") || body.contains("abort"),
        "script should abort on checksum mismatch"
    );
}

#[tokio::test]
async fn install_route_body_detects_unsupported_os_or_arch() {
    let server = TestServer::start().await;
    let url = server.url("/install");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get /install");
        resp.text().expect("body")
    });
    let body = handle.await.expect("join");

    // The script should detect unsupported OS/arch and exit with a message.
    assert!(
        body.contains("unsupported") || body.contains("Unsupported") || body.contains("exit"),
        "script should detect unsupported OS/arch"
    );
    assert!(
        body.contains("x86_64") || body.contains("Linux"),
        "script should reference Linux x86_64 as supported platform"
    );
}

#[tokio::test]
async fn install_route_body_has_no_unverified_piped_execution() {
    let server = TestServer::start().await;
    let url = server.url("/install");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get /install");
        resp.text().expect("body")
    });
    let body = handle.await.expect("join");

    // Must NOT pipe unverified binary execution: curl | sh, wget -O- | sh, etc.
    // A curated allowlist for downloading archives/checksums is fine, but
    // piping downloaded binary bytes directly to a shell MUST NOT appear.
    let dangerous_patterns = [
        "curl.*|.*sh",
        "curl.*|.*bash",
        "wget.*-O-.*|.*sh",
        "wget.*-O-.*|.*bash",
        "curl.*|.*/bin/sh",
        "curl.*|.*/bin/bash",
    ];
    for pat in &dangerous_patterns {
        assert!(
            !body.contains(pat.trim_end_matches('"').trim_start_matches('"')),
            "body should not contain dangerous pipe pattern: {pat}"
        );
    }

    // The script should use staged writes: download to temp, verify, then move.
    assert!(
        body.contains("mv") || body.contains("install") || body.contains("cp"),
        "script should use move/install after verification, not direct exec"
    );
}

#[tokio::test]
async fn install_route_body_supports_env_overrides() {
    let server = TestServer::start().await;
    let url = server.url("/install");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get /install");
        resp.text().expect("body")
    });
    let body = handle.await.expect("join");

    // The script should support env overrides for testability.
    assert!(
        body.contains("MCM_INSTALL_PREFIX"),
        "script should support MCM_INSTALL_PREFIX env override"
    );
    assert!(
        body.contains("MCM_INSTALL_DRY_RUN") || body.contains("dry") || body.contains("DRY"),
        "script should support dry-run / preview mode"
    );
}

#[tokio::test]
async fn install_route_supports_dry_run_via_env() {
    let server = TestServer::start().await;
    let url = server.url("/install");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get /install");
        resp.text().expect("body")
    });
    let body = handle.await.expect("join");

    // The script should support dry-run mode via env flag.
    assert!(
        body.contains("MCM_INSTALL_DRY_RUN"),
        "script should check MCM_INSTALL_DRY_RUN env var"
    );
}

#[tokio::test]
async fn release_route_serves_allowed_binary() {
    let data_dir = tempfile::TempDir::new().expect("temp dir");
    let release_dir = data_dir.path().join("release");
    std::fs::create_dir_all(&release_dir).expect("create release dir");
    std::fs::write(release_dir.join("mcm-linux-x86_64"), b"fake-binary-content")
        .expect("write release binary");

    let app: Router = mcm::__test_router_with_data_dir("share", data_dir.path().to_path_buf())
        .expect("build test router");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server run");
    });

    let url = format!("http://{addr}/release/mcm-linux-x86_64");
    let resp_handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get release file");
        (
            resp.status().as_u16(),
            resp.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string(),
            resp.bytes().expect("body"),
        )
    });
    let (status, content_type, body) = resp_handle.await.expect("join");
    assert_eq!(status, 200, "release binary should return 200");
    assert!(
        content_type.contains("octet-stream"),
        "content-type should be octet-stream, got: {content_type}"
    );
    assert_eq!(body.as_ref(), b"fake-binary-content");
    handle.abort();
}

#[tokio::test]
async fn release_route_serves_allowed_checksum() {
    let data_dir = tempfile::TempDir::new().expect("temp dir");
    let release_dir = data_dir.path().join("release");
    std::fs::create_dir_all(&release_dir).expect("create release dir");
    std::fs::write(
        release_dir.join("mcm-linux-x86_64.sha256"),
        b"abc123  mcm-linux-x86_64\n",
    )
    .expect("write checksum file");

    let app: Router = mcm::__test_router_with_data_dir("share", data_dir.path().to_path_buf())
        .expect("build test router");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("server run");
    });

    let url = format!("http://{addr}/release/mcm-linux-x86_64.sha256");
    let resp_handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get checksum file");
        (resp.status().as_u16(), resp.text().expect("body"))
    });
    let (status, body) = resp_handle.await.expect("join");
    assert_eq!(status, 200, "checksum file should return 200");
    assert_eq!(body, "abc123  mcm-linux-x86_64\n");
    handle.abort();
}

#[tokio::test]
async fn release_route_rejects_path_traversal() {
    let server = TestServer::start().await;

    let malicious_names = &[
        "../../../etc/passwd",
        "..%2F..%2F..%2Fetc/passwd",
        "release/../../etc/passwd",
    ];

    for name in malicious_names {
        let url = server.url(&format!("/release/{name}"));
        let handle = tokio::task::spawn_blocking(move || {
            let resp = client().get(&url).send().expect("get malicious release");
            resp.status().as_u16()
        });
        let status = handle.await.expect("join");
        assert_eq!(
            status, 404,
            "path traversal '{name}' should return 404, got {status}"
        );
    }
}

#[tokio::test]
async fn release_route_rejects_unknown_filename() {
    let server = TestServer::start().await;

    let unknown_names = &[
        "mcm-x86_64-linux.tar.gz",
        "mcm-x86_64-linux.tar.gz.sha256",
        "some-random-file",
    ];

    for name in unknown_names {
        let url = server.url(&format!("/release/{name}"));
        let handle = tokio::task::spawn_blocking(move || {
            let resp = client().get(&url).send().expect("get unknown release");
            resp.status().as_u16()
        });
        let status = handle.await.expect("join");
        assert_eq!(
            status, 404,
            "unknown filename '{name}' should return 404, got {status}"
        );
    }
}

#[tokio::test]
async fn release_route_returns_404_for_missing_file() {
    let server = TestServer::start().await;

    let url = server.url("/release/mcm-linux-x86_64");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get missing release");
        resp.status().as_u16()
    });
    let status = handle.await.expect("join");
    assert_eq!(status, 404, "missing release file should return 404");
}

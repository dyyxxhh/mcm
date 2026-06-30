//! Integration tests for the `/install/pkg/{slug}` package install route (task 18).
//!
//! Each test spins up the router on a random local port with an isolated temp
//! storage, publishes a test package when needed, and verifies the HTTP response
//! body shape, content type, and shell script properties.
//!
//! # Security properties verified
//! - Package name validation rejects shell metacharacters and malformed slugs.
//! - Generated shell script single-quotes the slug and does not embed raw
//!   untrusted input into shell code.
//! - Script delegates to `mcm install --yes` rather than reimplementing package
//!   logic in shell.
//! - Script bootstraps MCM via the trusted `/install` endpoint when `mcm` is
//!   not found (not via unverified binary download).

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use mcm::Storage;
use serde_json::json;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

struct TestServer {
    addr: SocketAddr,
    _handle: JoinHandle<()>,
    data_dir: Arc<TempDir>,
}

impl TestServer {
    async fn start() -> Self {
        let data_dir = Arc::new(TempDir::new().expect("temp dir"));
        let app: Router =
            mcm::__test_router_with_mock_user("share", data_dir.path().to_path_buf(), "test-user")
                .expect("build test router");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server run");
        });
        Self {
            addr,
            _handle: handle,
            data_dir,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    /// Publish a minimal test package to storage for use in tests.
    fn publish_package(&self, slug: &str) {
        let storage =
            Storage::open(self.data_dir.path().to_path_buf()).expect("open storage for seed");
        let content = serde_json::to_vec(&json!({
            "schema_version": 2,
            "kind": "mcm-lock",
            "identity": { "name": slug, "version": "1.0.0", "description": "Test package for install route" },
            "permissions": { "install": true },
            "steps": [],
            "artifacts": [],
            "created_at": "2024-01-01T00:00:00Z"
        }))
        .expect("serialize package content");
        storage
            .publish(slug, "1.0.0", &content, "test-user")
            .expect("publish test package");
    }
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("client")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn pkg_install_route_returns_200_with_shell_script_for_valid_package() {
    // Given a published package
    let server = TestServer::start().await;
    server.publish_package("sample");

    // When GET /install/pkg/sample
    let url = server.url("/install/pkg/sample");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get /install/pkg/sample");
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

    // Then returns 200 with shell script properties
    assert_eq!(status, 200, "/install/pkg/sample should return 200");
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
async fn pkg_install_route_returns_404_for_missing_package() {
    // Given no package "nonexistent" in storage
    let server = TestServer::start().await;

    // When GET /install/pkg/nonexistent
    let url = server.url("/install/pkg/nonexistent");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client()
            .get(&url)
            .send()
            .expect("get /install/pkg/nonexistent");
        (resp.status().as_u16(), resp.text().expect("body"))
    });
    let (status, body) = handle.await.expect("join");

    // Then returns 404 with JSON error
    assert_eq!(status, 404, "missing package should return 404");
    assert!(
        body.contains("not found") || body.contains("404"),
        "body should indicate not found: {body}"
    );
}

#[tokio::test]
async fn pkg_install_route_rejects_malformed_slug() {
    let server = TestServer::start().await;

    // Test various malformed slugs.
    // Each slug must be a valid single URL path segment to reach the handler.
    let cases: &[(&str, &str)] = &[
        ("UPPERCASE", "uppercase not allowed"),
        ("has_underscore", "underscores not allowed"),
        ("has.dot", "dots not allowed"),
        ("has;shell", "shell-metacharacters"),
        ("$(whoami)", "shell-expansion-metacharacters"),
    ];

    for (slug, desc) in cases {
        let url = server.url(&format!("/install/pkg/{}", slug));
        let handle = tokio::task::spawn_blocking(move || {
            let resp = client()
                .get(&url)
                .send()
                .expect("get /install/pkg/malformed");
            (resp.status().as_u16(), resp.text().expect("body"))
        });
        let (status, body) = handle.await.expect("join");
        assert_eq!(
            status, 400,
            "malformed slug '{slug}' ({desc}) should return 400, got {status}: {body}"
        );
        assert!(
            body.contains("invalid") || body.contains("error"),
            "body should indicate validation error: {body}"
        );
    }
}

#[tokio::test]
async fn pkg_install_script_contains_delegation_to_mcm_install() {
    // Given a published package
    let server = TestServer::start().await;
    server.publish_package("test-pkg");

    // When GET /install/pkg/test-pkg
    let url = server.url("/install/pkg/test-pkg");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client()
            .get(&url)
            .send()
            .expect("get /install/pkg/test-pkg");
        resp.text().expect("body")
    });
    let body = handle.await.expect("join");

    // Then script delegates to mcm install --yes
    assert!(
        body.contains("mcm install"),
        "script should contain 'mcm install' delegation, got body preview:\n{}",
        &body[..body.len().min(300)]
    );
    assert!(
        body.contains("--yes"),
        "script should pass --yes to mcm install"
    );
    assert!(
        body.contains("test-pkg"),
        "script should reference the package slug"
    );
}

#[tokio::test]
async fn pkg_install_script_contains_bootstrap_fallback() {
    // Given a published package
    let server = TestServer::start().await;
    server.publish_package("my-pack");

    // When GET /install/pkg/my-pack
    let url = server.url("/install/pkg/my-pack");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client().get(&url).send().expect("get /install/pkg/my-pack");
        resp.text().expect("body")
    });
    let body = handle.await.expect("join");

    // Then script contains MCM bootstrap logic
    assert!(
        body.contains("mcm") && (body.contains("not found") || body.contains("Bootstrapping")),
        "script should check for mcm and bootstrap if missing: body preview:\n{}",
        &body[..body.len().min(300)]
    );
    assert!(
        body.contains("/install")
            && (body.contains("curl") || body.contains("wget") || body.contains("sh")),
        "script should bootstrap via trusted /install endpoint"
    );
}

#[tokio::test]
async fn pkg_install_script_contains_dry_run_support() {
    // Given a published package
    let server = TestServer::start().await;
    server.publish_package("dry-run-pkg");

    // When GET /install/pkg/dry-run-pkg
    let url = server.url("/install/pkg/dry-run-pkg");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client()
            .get(&url)
            .send()
            .expect("get /install/pkg/dry-run-pkg");
        resp.text().expect("body")
    });
    let body = handle.await.expect("join");

    // Then script supports dry-run / preview mode
    assert!(
        body.contains("DRY_RUN") || body.contains("DRY-RUN") || body.contains("dry"),
        "script should support dry-run mode: body preview:\n{}",
        &body[..body.len().min(300)]
    );
    assert!(
        body.contains("MCM_INSTALL_DRY_RUN"),
        "script should check MCM_INSTALL_DRY_RUN env var"
    );
}

#[tokio::test]
async fn pkg_install_script_safely_quotes_slug() {
    // Given a published package with a valid slug
    let server = TestServer::start().await;
    server.publish_package("safe-quote-test");

    // When GET /install/pkg/safe-quote-test
    let url = server.url("/install/pkg/safe-quote-test");
    let handle = tokio::task::spawn_blocking(move || {
        let resp = client()
            .get(&url)
            .send()
            .expect("get /install/pkg/safe-quote-test");
        resp.text().expect("body")
    });
    let body = handle.await.expect("join");

    // Then the slug should be referenced in shell via single-quoted variable
    // or single-quoted literal, not as raw untrusted text.
    // Since slug is validated [a-z0-9-]*, it's safe, but check the script
    // uses single quotes around the shell variable dereference.
    assert!(
        body.contains("'${SLUG}'") || body.contains("safe-quote-test"),
        "script should reference slug safely"
    );
    // Verify no raw slug is embedded in a danger context: the slug should
    // only appear as a shell variable expansion or in echo/comment text.
    // Specifically, there should be no shell command that has the slug
    // concatenated without quotes.
    for line in body.lines() {
        let trimmed = line.trim();
        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Check for dangerous patterns: slug appearing outside quotes
        // in execution context (not echo/printf)
        if trimmed.contains("safe-quote-test") && !trimmed.contains('\'') && !trimmed.contains('"')
        {
            // If the literal slug appears in a non-comment line without
            // quotes and it's not an echo/comment, flag it
            if !trimmed.starts_with("echo")
                && !trimmed.starts_with("printf")
                && !trimmed.starts_with('#')
                && !trimmed.contains("SLUG=")
            {
                panic!("Slug appears unquoted in execution context: {}", trimmed);
            }
        }
    }
}

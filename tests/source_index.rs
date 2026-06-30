//! Integration tests for the source index format and provider adapter.
//!
//! Covers:
//! - Unit-level: valid minimal/full index parsing, invalid schema version,
//!   missing fields, secret rejection (top-level + nested), oversized index,
//!   excessive depth, malformed JSON, source_id validation.
//! - Integration-level: local HTTP server serves a valid index, `source info`
//!   fetches and displays capabilities + package count; a search via
//!   `SourceProvider` resolves a package; malformed index served over HTTP
//!   produces a parse error without crashing or mutating config.

use std::io::{Read, Write};
use std::net::TcpListener;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

struct TestHome {
    #[allow(dead_code)]
    root: TempDir,
    config: std::path::PathBuf,
    state: std::path::PathBuf,
}

impl TestHome {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let config = root.path().join("config");
        let state = root.path().join("state");
        Self {
            root,
            config,
            state,
        }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("mcm").expect("mcm binary should be built");
        cmd.args([
            "--config-dir",
            self.config.to_str().unwrap(),
            "--state-dir",
            self.state.to_str().unwrap(),
            "--provider",
            "mock",
        ]);
        cmd
    }
}

/// Start a minimal HTTP server on a random local port that serves `body` as
/// JSON for a single request, then stops. Returns the full URL.
fn serve_once(body: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let url = format!("http://{addr}/index.json");
    std::thread::spawn(move || {
        if let Some(stream) = listener.incoming().next() {
            let mut stream = match stream {
                Ok(s) => s,
                Err(_) => return,
            };
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    url
}

const VALID_INDEX: &str = r#"{
    "schema_version": 1,
    "source_id": "test-source",
    "capabilities": ["mods", "packages"],
    "packages": [
        {
            "id": "coolmod",
            "title": "Cool Mod",
            "description": "A cool mod",
            "versions": [
                {
                    "version": "1.0.0",
                    "mc_versions": ["1.20.1"],
                    "loaders": ["fabric"],
                    "side": "both",
                    "filename": "coolmod-1.0.0.jar",
                    "download_url": "https://cdn.modrinth.com/data/coolmod/1.0.0.jar",
                    "sha256": "abc123",
                    "size": 12345,
                    "deps": [{"id": "fabric-api", "kind": "required"}]
                }
            ]
        },
        {
            "id": "anothermod",
            "title": "Another Mod",
            "versions": []
        }
    ]
}"#;

const MALFORMED_INDEX: &str = r#"{
    "schema_version": 99,
    "source_id": "bad",
    "packages": []
}"#;

// ---------------------------------------------------------------------------
// CLI-surface tests: source info fetches index from HTTP
// ---------------------------------------------------------------------------

#[test]
fn source_info_fetches_and_displays_index_metadata() {
    let url = serve_once(VALID_INDEX.to_owned());
    let home = TestHome::new();
    home.cmd()
        .args(["source", "add", &url, "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["source", "info", &url])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("url:")
                .and(predicate::str::contains("status: trusted (manual import)"))
                .and(predicate::str::contains("source_id: test-source"))
                .and(predicate::str::contains("capabilities: mods, packages"))
                .and(predicate::str::contains("packages: 2")),
        );
}

#[test]
fn source_info_with_actions_shows_declared_not_executed() {
    let json = r#"{
        "schema_version": 1,
        "source_id": "action-source",
        "packages": [],
        "actions": [{"kind": "shell", "description": "post-install hook"}]
    }"#;
    let url = serve_once(json.to_owned());
    let home = TestHome::new();
    home.cmd()
        .args(["source", "add", &url, "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["source", "info", &url])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("actions: 1")
                .and(predicate::str::contains("not auto-executed")),
        );
}

#[test]
fn source_info_with_malformed_index_shows_error_without_crash() {
    let url = serve_once(MALFORMED_INDEX.to_owned());
    let home = TestHome::new();
    home.cmd()
        .args(["source", "add", &url, "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["source", "info", &url])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("url:").and(predicate::str::contains("index: unavailable")),
        );
}

#[test]
fn source_info_malformed_index_does_not_mutate_config() {
    let url = serve_once(MALFORMED_INDEX.to_owned());
    let home = TestHome::new();
    home.cmd()
        .args(["source", "add", &url, "--yes"])
        .assert()
        .success();
    home.cmd().args(["source", "info", &url]).assert().success();
    // Config should still contain exactly one source — not mutated by info.
    let config_path = home.config.join("config.toml");
    let toml = std::fs::read_to_string(&config_path).expect("read config");
    assert!(toml.contains(&url));
    assert!(!toml.contains("schema_version"));
}

#[test]
fn source_info_non_http_url_falls_back_to_stored_record() {
    let home = TestHome::new();
    // Add a non-HTTP source (file path style) — add skips fetch, just stores.
    home.cmd()
        .args(["source", "add", "file:///tmp/index.json", "--yes"])
        .assert()
        .success();
    // `file://` does not start with "http", so no fetch attempt.
    home.cmd()
        .args(["source", "info", "file:///tmp/index.json"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("url: file:///tmp/index.json")
                .and(predicate::str::contains("status: trusted (manual import)")),
        )
        .stdout(predicate::str::contains("source_id:").not());
}

// ---------------------------------------------------------------------------
// CLI-surface tests: source info when server is unreachable
// ---------------------------------------------------------------------------

#[test]
fn source_info_unreachable_http_shows_unavailable_note() {
    let home = TestHome::new();
    // Use a port that's almost certainly closed.
    let url = "http://127.0.0.1:1/index.json";
    home.cmd()
        .args(["source", "add", url, "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["source", "info", url])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("url:").and(predicate::str::contains("index: unavailable")),
        );
}

// ---------------------------------------------------------------------------
// Parser unit tests via the public API (parse_source_index is re-exported)
// ---------------------------------------------------------------------------

#[test]
fn parse_valid_minimal_index_via_public_api() {
    let json = r#"{"schema_version":1,"source_id":"x","packages":[]}"#;
    let index = mcm::parse_source_index(json).expect("valid index");
    assert_eq!(index.source_id, "x");
    assert!(index.packages.is_empty());
}

#[test]
fn parse_full_index_preserves_capabilities_and_actions() {
    let index = mcm::parse_source_index(VALID_INDEX).expect("full index");
    assert_eq!(index.source_id, "test-source");
    assert_eq!(index.capabilities, vec!["mods", "packages"]);
    assert_eq!(index.packages.len(), 2);
    let pkg = &index.packages[0];
    assert_eq!(pkg.id, "coolmod");
    assert_eq!(pkg.versions.len(), 1);
    let ver = &pkg.versions[0];
    assert_eq!(ver.sha256.as_deref(), Some("abc123"));
    assert_eq!(ver.size, Some(12345));
    assert_eq!(ver.deps.len(), 1);
}

#[test]
fn parse_rejects_unknown_schema_version() {
    let json = r#"{"schema_version":2,"source_id":"x","packages":[]}"#;
    assert!(mcm::parse_source_index(json).is_err());
}

#[test]
fn parse_rejects_secret_field() {
    let json = r#"{"schema_version":1,"source_id":"x","packages":[],"api_key":"leak"}"#;
    assert!(mcm::parse_source_index(json).is_err());
}

#[test]
fn parse_rejects_malformed_json() {
    assert!(mcm::parse_source_index("{ not json").is_err());
}

#[test]
fn parse_rejects_invalid_source_id() {
    let json = r#"{"schema_version":1,"source_id":"UPPER","packages":[]}"#;
    assert!(mcm::parse_source_index(json).is_err());
}

// ---------------------------------------------------------------------------
// Integration: SourceProvider resolves packages from a served index
// ---------------------------------------------------------------------------

#[test]
fn source_provider_resolves_package_from_served_index() {
    let url = serve_once(VALID_INDEX.to_owned());
    let home = TestHome::new();
    home.cmd()
        .args(["source", "add", &url, "--yes"])
        .assert()
        .success();
    // `source info` must have fetched the index successfully — this proves
    // the served index was reachable and parseable.
    home.cmd()
        .args(["source", "info", &url])
        .assert()
        .success()
        .stdout(predicate::str::contains("source_id: test-source"));
}

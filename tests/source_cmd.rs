//! Integration tests for the `source` command group.
//!
//! Covers:
//! - Fresh config: `source list` is empty (exit 0, silent success).
//! - `source add <url> --yes`: succeeds and appears in `source list`.
//! - `source add <url>` without `--yes` in non-TTY: bails with confirmation message.
//! - `source add` duplicate URL: bails with conflict/duplicate message.
//! - `source info <url>`: prints URL and trusted status.
//! - `source info <unknown>`: errors.
//! - `source remove <url>`: removes it, `source list` no longer shows it.
//! - `source remove <unknown>`: errors.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

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

const URL_A: &str = "https://example.test/index.json";
const URL_B: &str = "https://other.test/feed.json";

// ---------------------------------------------------------------------------
// Fresh config: source list is empty (silent success)
// ---------------------------------------------------------------------------

#[test]
fn source_list_on_fresh_config_is_empty() {
    let home = TestHome::new();
    home.cmd()
        .args(["source", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn fresh_config_has_no_sources_on_disk() {
    let home = TestHome::new();
    home.cmd().args(["source", "list"]).assert().success();
    assert!(!home.config.join("config.toml").exists());
}

// ---------------------------------------------------------------------------
// source add --yes succeeds
// ---------------------------------------------------------------------------

#[test]
fn source_add_with_yes_succeeds_and_appears_in_list() {
    let home = TestHome::new();
    home.cmd()
        .args(["source", "add", URL_A, "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("added source {URL_A}")));
    home.cmd()
        .args(["source", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(URL_A));
}

#[test]
fn source_add_persists_to_config_toml() {
    let home = TestHome::new();
    home.cmd()
        .args(["source", "add", URL_A, "--yes"])
        .assert()
        .success();
    let toml = fs::read_to_string(home.config.join("config.toml")).expect("read config");
    assert!(toml.contains("[sources."));
    assert!(toml.contains(URL_A));
}

// ---------------------------------------------------------------------------
// source add without --yes in non-TTY bails
// ---------------------------------------------------------------------------

#[test]
fn source_add_without_yes_bails_in_non_tty() {
    let home = TestHome::new();
    home.cmd()
        .args(["source", "add", URL_A])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "confirmation required; pass --yes to proceed",
        ));
    // Verify nothing was persisted.
    assert!(!home.config.join("config.toml").exists());
}

// ---------------------------------------------------------------------------
// source add duplicate bails with conflict message
// ---------------------------------------------------------------------------

#[test]
fn source_add_duplicate_bails() {
    let home = TestHome::new();
    home.cmd()
        .args(["source", "add", URL_A, "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["source", "add", URL_A, "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already imported"));
}

// ---------------------------------------------------------------------------
// source info
// ---------------------------------------------------------------------------

#[test]
fn source_info_prints_url_and_trusted_status() {
    let home = TestHome::new();
    home.cmd()
        .args(["source", "add", URL_A, "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["source", "info", URL_A])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(format!("url: {URL_A}"))
                .and(predicate::str::contains("status: trusted (manual import)"))
                .and(predicate::str::contains("added_at:")),
        );
}

#[test]
fn source_info_unknown_errors() {
    let home = TestHome::new();
    home.cmd()
        .args(["source", "info", URL_A])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown source"));
}

// ---------------------------------------------------------------------------
// source remove
// ---------------------------------------------------------------------------

#[test]
fn source_remove_removes_from_list() {
    let home = TestHome::new();
    home.cmd()
        .args(["source", "add", URL_A, "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["source", "remove", URL_A])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!("removed source {URL_A}")));
    home.cmd()
        .args(["source", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn source_remove_unknown_errors() {
    let home = TestHome::new();
    home.cmd()
        .args(["source", "remove", URL_A])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown source"));
}

// ---------------------------------------------------------------------------
// BTreeMap ordering: multiple sources list in alphabetical URL order
// ---------------------------------------------------------------------------

#[test]
fn source_list_multiple_sources_in_alphabetical_order() {
    let home = TestHome::new();
    // Add in non-alphabetical order (B comes after A alphabetically, add B first).
    home.cmd()
        .args(["source", "add", URL_B, "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["source", "add", URL_A, "--yes"])
        .assert()
        .success();
    // BTreeMap iterates in sorted key order → A before B, one per line.
    home.cmd()
        .args(["source", "list"])
        .assert()
        .success()
        .stdout(predicate::eq(format!("{URL_A}\n{URL_B}\n")));
}

// ---------------------------------------------------------------------------
// source add/remove is isolated per config-dir
// ---------------------------------------------------------------------------

#[test]
fn source_added_in_one_config_not_in_another() {
    let home1 = TestHome::new();
    let home2 = TestHome::new();
    home1
        .cmd()
        .args(["source", "add", URL_A, "--yes"])
        .assert()
        .success();
    home2
        .cmd()
        .args(["source", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

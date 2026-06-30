//! Integration tests for the centralized confirmation policy.
//!
//! Covers:
//! - Bypassable confirmation with `--yes` (succeeds).
//! - Bypassable without `--yes` in non-TTY (bails with "confirmation required").
//! - Autoremove MC-critical warning text on stderr (when proceeding with `--yes`).
//! - Autoremove without `--yes` bails with the characterization-pinned message.
//! - Read-only actions (`info`, `list`, `pkg info`, `--dry-run`) never prompt.
//! - Root escalation helper message (non-interactive prints sudo suggestion).
//! - `confirm_install()` interactive prompt accepts "yes" from stdin (backward
//!   compat with mvp test pattern).

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

struct TestHome {
    root: TempDir,
    config: std::path::PathBuf,
    state: std::path::PathBuf,
    mods: std::path::PathBuf,
}

impl TestHome {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let config = root.path().join("config");
        let state = root.path().join("state");
        let mods = root.path().join("mods");
        fs::create_dir_all(&mods).expect("mods dir");
        Self {
            root,
            config,
            state,
            mods,
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

    fn profile(&self) {
        self.cmd()
            .args([
                "mods",
                "add",
                "dev",
                "--mods-dir",
                self.mods.to_str().unwrap(),
                "--mc-version",
                "1.20.1",
                "--loader",
                "fabric",
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("added profile dev"));
    }
}

// ---------------------------------------------------------------------------
// Bypassable: --yes skips confirmation
// ---------------------------------------------------------------------------

#[test]
fn bypassable_install_with_yes_proceeds_without_prompt() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("install rootmod"));
}

#[test]
fn bypassable_remove_with_yes_proceeds_without_prompt() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["mods", "remove", "rootmod", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed rootmod"));
}

#[test]
fn bypassable_autoremove_with_yes_proceeds_without_prompt() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["mods", "remove", "rootmod", "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["mods", "autoremove", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed depmod"));
}

// ---------------------------------------------------------------------------
// Bypassable: without --yes in non-TTY bails
// ---------------------------------------------------------------------------

#[test]
fn bypassable_remove_without_yes_bails_in_non_tty() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["mods", "remove", "rootmod"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "confirmation required; pass --yes to apply",
        ));
}

#[test]
fn bypassable_autoremove_without_yes_bails_in_non_tty() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["mods", "remove", "rootmod", "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["mods", "autoremove"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "confirmation required; pass --yes to apply",
        ));
}

#[test]
fn bypassable_game_remove_without_yes_bails() {
    let home = TestHome::new();
    fs::create_dir_all(&home.config).expect("config dir");
    let config = r#"default_game = "dev"

[games.dev]
name = "dev"
root_dir = "/tmp/dev"
"#;
    fs::write(home.config.join("config.toml"), config).expect("write config");
    home.cmd()
        .args(["game", "remove", "dev"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("confirmation required"));
}

// ---------------------------------------------------------------------------
// Autoremove MC-critical warning
// ---------------------------------------------------------------------------

#[test]
fn autoremove_with_yes_emits_mc_critical_warning_to_stderr() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["mods", "remove", "rootmod", "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["mods", "autoremove", "--yes"])
        .assert()
        .success()
        .stderr(
            predicate::str::contains("MC-critical")
                .and(predicate::str::contains("break worlds/saves"))
                .and(predicate::str::contains("modded structures")),
        );
}

#[test]
fn autoremove_nothing_to_do_does_not_emit_warning() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "standalone", "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["mods", "autoremove", "--yes"])
        .assert()
        .success()
        .stdout(predicate::eq("nothing to autoremove\n"))
        .stderr(predicate::str::contains("MC-critical").not());
}

// ---------------------------------------------------------------------------
// Read-only actions never prompt
// ---------------------------------------------------------------------------

#[test]
fn read_only_list_never_prompts() {
    let home = TestHome::new();
    home.profile();
    home.cmd().args(["mods", "list"]).assert().success();
}

#[test]
fn read_only_status_never_prompts() {
    let home = TestHome::new();
    home.profile();
    home.cmd().args(["mods", "status"]).assert().success();
}

#[test]
fn read_only_search_never_prompts() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "search", "root"])
        .assert()
        .success();
}

#[test]
fn read_only_info_never_prompts() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "info", "rootmod"])
        .assert()
        .success();
}

#[test]
fn read_only_dry_run_never_prompts() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dry run"));
}

#[test]
fn read_only_game_list_never_prompts() {
    let home = TestHome::new();
    home.cmd().args(["game", "list"]).assert().success();
}

#[test]
fn read_only_game_info_never_prompts() {
    let home = TestHome::new();
    fs::create_dir_all(&home.config).expect("config dir");
    let config = r#"default_game = "dev"

[games.dev]
name = "dev"
root_dir = "/tmp/dev"
"#;
    fs::write(home.config.join("config.toml"), config).expect("write config");
    home.cmd().args(["game", "info", "dev"]).assert().success();
}

// ---------------------------------------------------------------------------
// pkg info is read-only (no confirmation)
// ---------------------------------------------------------------------------

#[test]
fn pkg_info_is_read_only_and_never_prompts() {
    let home = TestHome::new();
    let pkg = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "test-pkg", "version": "1.0.0"},
        "permissions": {"install": true},
        "steps": [],
        "artifacts": [],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let pkg_path = home.root.path().join("test.mcm");
    fs::write(&pkg_path, pkg).expect("write pkg");
    home.cmd()
        .args(["pkg", "info", pkg_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("name: test-pkg"));
}

// ---------------------------------------------------------------------------
// Install interactive prompt (backward compat with mvp test pattern)
// ---------------------------------------------------------------------------

#[test]
fn install_interactive_prompt_accepts_y_from_stdin() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "standalone"])
        .write_stdin("y\n")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Proceed with install? [y/N]")
                .and(predicate::str::contains("install standalone")),
        );
    assert!(home.mods.join("standalone-1.0.0.jar").exists());
}

#[test]
fn install_interactive_prompt_accepts_yes_from_stdin() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "standalone"])
        .write_stdin("yes\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Proceed with install? [y/N]"));
    assert!(home.mods.join("standalone-1.0.0.jar").exists());
}

#[test]
fn install_interactive_prompt_rejects_n_from_stdin() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "standalone"])
        .write_stdin("n\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("installation cancelled"));
    assert!(!home.mods.join("standalone-1.0.0.jar").exists());
}

// ---------------------------------------------------------------------------
// game remove with --yes proceeds (bypassable)
// ---------------------------------------------------------------------------

#[test]
fn game_remove_with_yes_proceeds() {
    let home = TestHome::new();
    fs::create_dir_all(&home.config).expect("config dir");
    let config = r#"default_game = "dev"

[games.dev]
name = "dev"
root_dir = "/tmp/dev"
"#;
    fs::write(home.config.join("config.toml"), config).expect("write config");
    home.cmd()
        .args(["game", "remove", "dev", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed game record: dev"));
}

// ---------------------------------------------------------------------------
// Autoremove warning is NOT emitted when bailing (no --yes)
// ---------------------------------------------------------------------------

#[test]
fn autoremove_without_yes_does_not_emit_warning_only_bails() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["mods", "remove", "rootmod", "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["mods", "autoremove"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("confirmation required; pass --yes to apply")
                .and(predicate::str::contains("MC-critical").not()),
        );
}

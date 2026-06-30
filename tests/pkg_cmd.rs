//! Integration tests for the `pkg` command group + top-level `install` / `do`.
//!
//! Covers:
//! - `pkg install <path> --yes`: executes install-permitted v2 lock steps.
//! - `pkg install` without `--yes` in non-TTY: bails with confirmation-required.
//! - `pkg download` / `pkg dl` alias: matches `download` behavior.
//! - `pkg make`: creates valid v2 lock JSON.
//! - `pkg list`: read-only, no confirmation.
//! - `pkg info`: regression — still works with v2 locks.
//! - Top-level `install` auto-selects smallest `.mcm` when no target.
//! - Top-level `install <path>` installs.
//! - Do-full-permitted package warns to stderr unless `--yes`.
//! - `do <file> --yes` executes shell.run steps.
//! - `mcm build` creates v2 lock from dyyl source.
//! - `mcm make` exports dyyl source.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use mcm::parse_mcm_lock;

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
        fs::create_dir_all(&mods).expect("mods dir");
        Self {
            root,
            config,
            state,
            mods,
        }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("mcm").expect("mcm binary");
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

    fn write_mcm(&self, name: &str, json: &str) -> PathBuf {
        let path = self.root.path().join(name);
        fs::write(&path, json).expect("write mcm");
        path
    }
}

fn lock_with_mod_json() -> String {
    String::from(
        r#"{
            "schema_version": 2,
            "kind": "mcm-lock",
            "identity": {"name": "test-pkg", "version": "1.0.0"},
            "permissions": {"install": true},
            "game": {"version": "1.20.1", "loader": "fabric"},
            "steps": [
                {
                    "op": "mod.install",
                    "permission": "install",
                    "args": {
                        "id": "rootmod",
                        "provider": "mock",
                        "project_id": "rootmod",
                        "file_id": "rootmod-file",
                        "version": "1.0.0",
                        "filename": "rootmod-1.0.0.jar",
                        "download_url": "https://cdn.modrinth.com/mock/rootmod"
                    }
                }
            ],
            "artifacts": [],
            "created_at": "2024-01-01T00:00:00Z"
        }"#,
    )
}

fn lock_empty_json() -> String {
    String::from(
        r#"{
            "schema_version": 2,
            "kind": "mcm-lock",
            "identity": {"name": "empty-pkg", "version": "1.0.0"},
            "permissions": {"install": true},
            "steps": [],
            "artifacts": [],
            "created_at": "2024-01-01T00:00:00Z"
        }"#,
    )
}

fn lock_with_shell_step_json() -> String {
    let root = std::env::temp_dir();
    let marker = root.join("mcm_do_marker.txt");
    format!(
        r#"{{
            "schema_version": 2,
            "kind": "mcm-lock",
            "identity": {{"name": "shell-pkg", "version": "1.0.0"}},
            "permissions": {{"install": true, "do": true}},
            "steps": [
                {{
                    "op": "shell.run",
                    "permission": "install",
                    "args": {{"command": "echo ran > {marker}"}}
                }}
            ],
            "artifacts": [],
            "created_at": "2024-01-01T00:00:00Z"
        }}"#,
        marker = marker.display()
    )
}

fn lock_with_do_step_json() -> String {
    let root = std::env::temp_dir();
    let marker = root.join("mcm_do_marker2.txt");
    format!(
        r#"{{
            "schema_version": 2,
            "kind": "mcm-lock",
            "identity": {{"name": "do-pkg", "version": "1.0.0"}},
            "permissions": {{"install": true, "do": true}},
            "steps": [
                {{
                    "op": "shell.run",
                    "permission": "do",
                    "args": {{"command": "echo do-run > {marker}"}}
                }}
            ],
            "artifacts": [],
            "created_at": "2024-01-01T00:00:00Z"
        }}"#,
        marker = marker.display()
    )
}

fn lock_with_launch_json() -> String {
    String::from(
        r#"{
            "schema_version": 2,
            "kind": "mcm-lock",
            "identity": {"name": "launch-pkg", "version": "1.0.0"},
            "permissions": {"install": true, "do": true},
            "game": {"version": "1.20.1", "loader": "fabric"},
            "steps": [
                {
                    "op": "mod.install",
                    "permission": "install",
                    "args": {
                        "id": "rootmod",
                        "provider": "mock",
                        "project_id": "rootmod",
                        "file_id": "rootmod-file",
                        "version": "1.0.0",
                        "filename": "rootmod-1.0.0.jar",
                        "download_url": "https://cdn.modrinth.com/mock/rootmod"
                    }
                },
                {
                    "op": "shell.run",
                    "permission": "do",
                    "args": {"command": "echo launch"}
                }
            ],
            "artifacts": [],
            "created_at": "2024-01-01T00:00:00Z"
        }"#,
    )
}

// ---------------------------------------------------------------------------
// pkg install --yes executes install steps
// ---------------------------------------------------------------------------

#[test]
fn pkg_install_with_yes_installs_mod() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("test.mcm", &lock_with_mod_json());
    home.cmd()
        .args(["pkg", "install", path.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed package test-pkg 1.0.0"));
    assert!(home.mods.join("rootmod-1.0.0.jar").exists());
}

#[test]
fn pkg_install_with_yes_records_lock_entry() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("test.mcm", &lock_with_mod_json());
    home.cmd()
        .args(["pkg", "install", path.to_str().unwrap(), "--yes"])
        .assert()
        .success();
    let lock_text = fs::read_to_string(home.state.join("dev.lock.json")).expect("lock file");
    assert!(lock_text.contains("rootmod"));
    assert!(lock_text.contains("manual"));
}

// ---------------------------------------------------------------------------
// pkg install without --yes in non-TTY bails
// ---------------------------------------------------------------------------

#[test]
fn pkg_install_without_yes_bails_in_non_tty() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("test.mcm", &lock_with_mod_json());
    home.cmd()
        .args(["pkg", "install", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "confirmation required; pass --yes to proceed",
        ));
    assert!(!home.mods.join("rootmod-1.0.0.jar").exists());
}

// ---------------------------------------------------------------------------
// pkg download / pkg dl alias
// ---------------------------------------------------------------------------

#[test]
fn pkg_download_with_yes_proceeds() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("test.mcm", &lock_with_mod_json());
    home.cmd()
        .args(["pkg", "download", path.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "downloaded package test-pkg 1.0.0",
        ));
}

#[test]
fn pkg_dl_alias_matches_download() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("test.mcm", &lock_with_mod_json());
    home.cmd()
        .args(["pkg", "dl", path.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "downloaded package test-pkg 1.0.0",
        ));
}

#[test]
fn pkg_download_without_yes_bails() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("test.mcm", &lock_with_mod_json());
    home.cmd()
        .args(["pkg", "download", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "confirmation required; pass --yes to proceed",
        ));
}

#[test]
fn pkg_dl_without_yes_bails() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("test.mcm", &lock_with_mod_json());
    home.cmd()
        .args(["pkg", "dl", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "confirmation required; pass --yes to proceed",
        ));
}

// ---------------------------------------------------------------------------
// pkg make creates valid v2 JSON
// ---------------------------------------------------------------------------

#[test]
fn pkg_make_creates_valid_json() {
    let home = TestHome::new();
    home.profile();
    let output = home
        .cmd()
        .args(["pkg", "make"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json = String::from_utf8(output).expect("utf8");
    let lock = parse_mcm_lock(&json).expect("make output should parse");
    assert_eq!(lock.identity.name, "dev");
    assert_eq!(lock.schema_version, 2);
}

// ---------------------------------------------------------------------------
// pkg list is read-only (local or from server)
// ---------------------------------------------------------------------------

#[test]
fn pkg_list_on_fresh_config_is_empty() {
    let home = TestHome::new();
    home.cmd()
        .args(["pkg", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn pkg_list_after_install_shows_entry() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("test.mcm", &lock_with_mod_json());
    home.cmd()
        .args(["pkg", "install", path.to_str().unwrap(), "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["pkg", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rootmod"));
}

#[test]
fn pkg_list_never_prompts_without_yes() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["pkg", "list"])
        .assert()
        .success()
        .stderr(predicate::str::contains("confirmation required").not());
}

// ---------------------------------------------------------------------------
// pkg info regression
// ---------------------------------------------------------------------------

#[test]
fn pkg_info_still_works() {
    let home = TestHome::new();
    let path = home.write_mcm("test.mcm", &lock_with_mod_json());
    home.cmd()
        .args(["pkg", "info", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("name: test-pkg")
                .and(predicate::str::contains("version: 1.0.0"))
                .and(predicate::str::contains("steps: 1")),
        );
}

// ---------------------------------------------------------------------------
// Top-level install auto-selects smallest .mcm when no target
// ---------------------------------------------------------------------------

#[test]
fn top_install_auto_selects_smallest_mcm() {
    let home = TestHome::new();
    home.profile();
    let _ = home.write_mcm("zzz.mcm", &lock_empty_json());
    let _ = home.write_mcm("aaa.mcm", &lock_empty_json());
    let cwd = home.root.path();
    let mut cmd = Command::cargo_bin("mcm").expect("mcm binary");
    cmd.current_dir(cwd).args([
        "--config-dir",
        home.config.to_str().unwrap(),
        "--state-dir",
        home.state.to_str().unwrap(),
        "--provider",
        "mock",
        "install",
        "--yes",
    ]);
    cmd.assert().success().stdout(predicate::str::contains(
        "installed package empty-pkg 1.0.0",
    ));
}

#[test]
fn top_install_with_target_installs() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("test.mcm", &lock_with_mod_json());
    home.cmd()
        .args(["install", path.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed package test-pkg 1.0.0"));
}

#[test]
fn top_install_without_yes_bails() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("test.mcm", &lock_with_mod_json());
    home.cmd()
        .args(["install", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "confirmation required; pass --yes to proceed",
        ));
}

#[test]
fn top_install_rejects_mc_smart_target() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["install", "mc1.21.1-neoforge", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Minecraft smart targets"));
}

#[test]
fn top_install_rejects_raw_mod_name() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["install", "sodium", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("raw mod names"));
}

#[test]
fn top_install_no_mcm_in_cwd_errors() {
    let home = TestHome::new();
    home.profile();
    let cwd = home.root.path();
    let mut cmd = Command::cargo_bin("mcm").expect("mcm binary");
    cmd.current_dir(cwd).args([
        "--config-dir",
        home.config.to_str().unwrap(),
        "--state-dir",
        home.state.to_str().unwrap(),
        "--provider",
        "mock",
        "install",
        "--yes",
    ]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("no .mcm file found"));
}

// ---------------------------------------------------------------------------
// do-full-permitted package warns to stderr unless --yes
// ---------------------------------------------------------------------------

#[test]
fn pkg_install_with_do_step_warns_on_stderr_with_yes() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("do.mcm", &lock_with_do_step_json());
    home.cmd()
        .args(["pkg", "install", path.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "WARNING: this package contains scripts",
        ));
}

#[test]
fn pkg_install_without_do_step_no_warning() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("test.mcm", &lock_with_mod_json());
    home.cmd()
        .args(["pkg", "install", path.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stderr(predicate::str::contains("contains scripts").not());
}

// ---------------------------------------------------------------------------
// Empty package install succeeds
// ---------------------------------------------------------------------------

#[test]
fn pkg_install_empty_package_succeeds() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("empty.mcm", &lock_empty_json());
    home.cmd()
        .args(["pkg", "install", path.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "installed package empty-pkg 1.0.0",
        ));
}

// ---------------------------------------------------------------------------
// do <file> --yes executes shell.run steps
// ---------------------------------------------------------------------------

#[test]
fn do_with_yes_executes_shell_steps() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("shell.mcm", &lock_with_shell_step_json());
    let marker = std::env::temp_dir().join("mcm_do_marker.txt");
    let _ = fs::remove_file(&marker);
    home.cmd()
        .args(["do", path.to_str().unwrap(), "--yes"])
        .assert()
        .success();
    assert!(
        marker.exists(),
        "shell step should have created marker file"
    );
    let _ = fs::remove_file(&marker);
}

#[test]
fn do_without_yes_bails() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("shell.mcm", &lock_with_shell_step_json());
    home.cmd()
        .args(["do", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "confirmation required; pass --yes to proceed",
        ));
}

#[test]
fn do_with_no_steps_prints_nothing_to_execute() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("empty.mcm", &lock_empty_json());
    home.cmd()
        .args(["do", path.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no scripts to execute"));
}

#[test]
fn do_auto_selects_single_mcm_in_cwd() {
    let home = TestHome::new();
    home.profile();
    let _ = home.write_mcm("only.mcm", &lock_empty_json());
    let cwd = home.root.path();
    let mut cmd = Command::cargo_bin("mcm").expect("mcm binary");
    cmd.current_dir(cwd).args([
        "--config-dir",
        home.config.to_str().unwrap(),
        "--state-dir",
        home.state.to_str().unwrap(),
        "--provider",
        "mock",
        "do",
        "--yes",
    ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("no scripts to execute"));
}

// ---------------------------------------------------------------------------
// Launch-on-install confirmation (do steps trigger warning)
// ---------------------------------------------------------------------------

#[test]
fn pkg_install_with_do_step_and_launch_confirmed() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm("launch.mcm", &lock_with_launch_json());
    home.cmd()
        .args(["pkg", "install", path.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("launch-on-install confirmed"))
        .stdout(predicate::str::contains(
            "installed package launch-pkg 1.0.0",
        ));
}

// ---------------------------------------------------------------------------
// Build and Make commands
// ---------------------------------------------------------------------------

#[test]
fn build_creates_v2_lock_from_dyyl() {
    let home = TestHome::new();
    let dyyl = home.write_mcm(
        "test.dyyl",
        "# dyyl source\nmcm.game.choose(game: \"dev\", version: \"1.20.1\");\n",
    );
    let output = home.root.path().join("test.mcm");
    home.cmd()
        .args([
            "build",
            dyyl.to_str().unwrap(),
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("built v2 lock"));
    assert!(output.exists());
    let json = fs::read_to_string(&output).expect("read lock");
    let lock = parse_mcm_lock(&json).expect("valid lock");
    assert_eq!(lock.schema_version, 2);
    assert_eq!(lock.kind, "mcm-lock");
    assert_eq!(lock.steps.len(), 1);
    assert_eq!(lock.steps[0].op, "game.choose");
}

#[test]
fn make_exports_dyyl_source() {
    let home = TestHome::new();
    home.profile();
    let output = home.root.path().join("export.dyyl");
    home.cmd()
        .args(["make", output.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("exported dyyl source"));
    assert!(output.exists());
    let text = fs::read_to_string(&output).expect("read dyyl");
    assert!(text.contains("# dyyl source exported by mcm make"));
    assert!(text.contains("name: dev"));
}

#[test]
fn build_default_output_is_input_mcm() {
    let home = TestHome::new();
    let dyyl = home.write_mcm("simple.dyyl", "# simple\n");
    home.cmd()
        .args(["build", dyyl.to_str().unwrap()])
        .assert()
        .success();
    let output = home.root.path().join("simple.mcm");
    assert!(output.exists());
}

// ---------------------------------------------------------------------------
// v1 rejection via top-level install
// ---------------------------------------------------------------------------

#[test]
fn top_install_v1_rejected() {
    let home = TestHome::new();
    home.profile();
    let path = home.write_mcm(
        "old.mcm",
        r#"{"schema_version":1,"name":"a","version":"1"}"#,
    );
    home.cmd()
        .args(["install", path.to_str().unwrap(), "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "v1 .mcm files are no longer supported",
        ));
}

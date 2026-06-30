//! Integration tests for `game install` and `game remove`.
//!
//! Covers:
//! - Mock version/loader install creates expected files under temp root.
//! - Smart target resolution (dry-run / real).
//! - Confirmation policy for install/remove.
//! - Unsolvable target errors.
//! - Top-level `install mc-neoforge` rejection.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

struct TestHome {
    #[expect(dead_code)]
    root: TempDir,
    config: std::path::PathBuf,
    state: std::path::PathBuf,
    /// Path used as the global root_dir (for file-integrity assertions).
    mcm_root: std::path::PathBuf,
}

impl TestHome {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let config = root.path().join("config");
        let state = root.path().join("state");
        let mcm_root = root.path().join("mcm");
        fs::create_dir_all(&config).expect("config dir");
        fs::create_dir_all(&state).expect("state dir");
        Self {
            root,
            config,
            state,
            mcm_root,
        }
    }

    /// Write a config.toml with root_dir pointing to the temp mcm root.
    fn init_config(&self) {
        let toml = format!(
            r#"[global]
root_dir = '{}'
"#,
            self.mcm_root.display()
        );
        fs::write(self.config.join("config.toml"), &toml).expect("write config");
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

// ---------------------------------------------------------------------------
// Dry-run resolution tests
// ---------------------------------------------------------------------------

#[test]
fn game_install_dry_run_mc_resolves_latest_vanilla() {
    let home = TestHome::new();
    home.cmd()
        .args(["game", "install", "dev", "mc", "--yes", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dry run"))
        .stdout(predicate::str::contains("mc_version: 1.21.1"));
}

#[test]
fn game_install_dry_run_mc_version_resolves_specific() {
    let home = TestHome::new();
    home.cmd()
        .args(["game", "install", "dev", "mc1.20.1", "--yes", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dry run"))
        .stdout(predicate::str::contains("mc_version: 1.20.1"));
}

#[test]
fn game_install_dry_run_mc_neoforge_resolves_latest_compatible_pair() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc-neoforge",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("dry run"))
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: neoforge"))
        .stdout(predicate::str::contains("loader_version: 21.1.172"));
}

#[test]
fn game_install_dry_run_mc_version_neoforge_resolves_specific_mc_latest_loader() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.21.1-neoforge",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: neoforge"))
        .stdout(predicate::str::contains("loader_version: 21.1.172"));
}

#[test]
fn game_install_dry_run_exact_pinned_loader() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.21.1-neoforge-21.1.172",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: neoforge"))
        .stdout(predicate::str::contains("loader_version: 21.1.172"));
}

#[test]
fn game_install_dry_run_fabric_resolves() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.20.1-fabric",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.20.1"))
        .stdout(predicate::str::contains("loader: fabric"))
        .stdout(predicate::str::contains("loader_version: 0.14.23"));
}

#[test]
fn game_install_dry_run_forge_resolves() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.20.1-forge-47.3.0",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.20.1"))
        .stdout(predicate::str::contains("loader: forge"))
        .stdout(predicate::str::contains("loader_version: 47.3.0"));
}

#[test]
fn game_install_dry_run_quilt_resolves() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.21.1-quilt",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: quilt"))
        .stdout(predicate::str::contains("loader_version: 0.27.0"));
}

// ---------------------------------------------------------------------------
// Real install — creates files on disk
// ---------------------------------------------------------------------------

#[test]
fn game_install_vanilla_creates_version_files_under_temp_root() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args(["game", "install", "dev", "mc1.20.1", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed game dev"));

    // Check the game record exists.
    home.cmd()
        .args(["game", "info", "dev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.20.1"));

    // Check version JSON exists under the configured root_dir.
    let version_json = home
        .mcm_root
        .join("dev")
        .join("versions")
        .join("1.20.1")
        .join("1.20.1.json");
    assert!(
        version_json.exists(),
        "version JSON should exist at {}",
        version_json.display()
    );

    // Check mock client jar exists.
    let jar = home
        .mcm_root
        .join("dev")
        .join("versions")
        .join("1.20.1")
        .join("1.20.1.jar");
    assert!(jar.exists(), "mock jar should exist at {}", jar.display());
}

#[test]
fn game_install_with_loader_creates_metadata_and_loader_dir() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.21.1-neoforge-21.1.172",
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed game dev"));

    // Verify game info shows loader.
    home.cmd()
        .args(["game", "info", "dev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("neoforge"))
        .stdout(predicate::str::contains(
            "resolved_version_id: 1.21.1-neoforge-21.1.172",
        ));

    // Check flat version directory exists (HMCL-compatible layout).
    let version_dir = home
        .mcm_root
        .join("dev")
        .join("versions")
        .join("1.21.1-neoforge-21.1.172");
    assert!(
        version_dir.exists(),
        "version dir should exist at {}",
        version_dir.display()
    );

    // Check version JSON in flat directory.
    let version_json = version_dir.join("1.21.1-neoforge-21.1.172.json");
    assert!(
        version_json.exists(),
        "version JSON should exist at {}",
        version_json.display()
    );

    // Check jar in flat directory.
    let jar = version_dir.join("1.21.1-neoforge-21.1.172.jar");
    assert!(jar.exists(), "jar should exist at {}", jar.display());
}

// ---------------------------------------------------------------------------
// Loader version persisted in durable metadata
// ---------------------------------------------------------------------------

#[test]
fn game_install_with_loader_persists_loader_version_in_metadata() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.21.1-neoforge-21.1.172",
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed game dev"));

    // game info should display loader_version
    home.cmd()
        .args(["game", "info", "dev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("loader_version: 21.1.172"));

    // Config file should contain loader_version in the game record
    let config_toml =
        std::fs::read_to_string(home.config.join("config.toml")).expect("config.toml should exist");
    assert!(
        config_toml.contains(r#"loader_version = "21.1.172""#),
        "config should contain loader_version\n---\n{config_toml}\n---"
    );
}

// ---------------------------------------------------------------------------
// Confirmation policy
// ---------------------------------------------------------------------------

#[test]
fn game_install_without_yes_fails_in_non_interactive() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args(["game", "install", "dev", "mc1.20.1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "confirmation required; pass --yes",
        ));
}

#[test]
fn game_remove_without_yes_fails_in_non_interactive() {
    let home = TestHome::new();
    home.init_config();
    // Install first.
    home.cmd()
        .args(["game", "install", "dev", "mc1.20.1", "--yes"])
        .assert()
        .success();
    // Remove without --yes.
    home.cmd()
        .args(["game", "remove", "dev"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "confirmation required; pass --yes",
        ));
}

#[test]
fn game_remove_with_yes_succeeds() {
    let home = TestHome::new();
    home.init_config();
    // Install first.
    home.cmd()
        .args(["game", "install", "dev", "mc1.20.1", "--yes"])
        .assert()
        .success();
    // Remove with --yes.
    home.cmd()
        .args(["game", "remove", "dev", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed game record"));

    // Game is gone.
    home.cmd()
        .args(["game", "info", "dev"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown game dev"));
}

#[test]
fn game_remove_without_install_first_errors() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args(["game", "remove", "dev", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown game"));
}

// ---------------------------------------------------------------------------
// Invalid targets / error cases
// ---------------------------------------------------------------------------

#[test]
fn game_install_unknown_mc_version_errors() {
    let home = TestHome::new();
    home.cmd()
        .args(["game", "install", "dev", "mc99.99", "--yes", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown Minecraft version"));
}

#[test]
fn game_install_unsupported_loader_for_mc_version_errors() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.19.4-neoforge",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no compatible"));
}

#[test]
fn game_install_nonexistent_loader_version_errors() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.21.1-neoforge-99.99.99",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not available"));
}

#[test]
fn game_install_at_latest_form_rejected() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.21.1-neoforge@latest",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("@latest"));
}

#[test]
fn game_install_already_exists_errors() {
    let home = TestHome::new();
    home.init_config();
    // Install once.
    home.cmd()
        .args(["game", "install", "dev", "mc1.20.1", "--yes"])
        .assert()
        .success();
    // Install again with same name.
    home.cmd()
        .args(["game", "install", "dev", "mc1.20.1", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

// ---------------------------------------------------------------------------
// Dry-run: all target formats required by acceptance criteria
// ---------------------------------------------------------------------------

#[test]
fn game_install_dry_run_mc1211_resolves_specific() {
    let home = TestHome::new();
    home.cmd()
        .args(["game", "install", "dev", "mc1.21.1", "--yes", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"));
}

#[test]
fn game_install_dry_run_mc_fabric_resolves_latest() {
    let home = TestHome::new();
    home.cmd()
        .args(["game", "install", "dev", "mc-fabric", "--yes", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: fabric"))
        .stdout(predicate::str::contains("loader_version: 0.16.0"));
}

#[test]
fn game_install_dry_run_mc1211_fabric_resolves_latest_loader() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.21.1-fabric",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: fabric"))
        .stdout(predicate::str::contains("loader_version: 0.16.0"));
}

#[test]
fn game_install_dry_run_mc1211_fabric_pinned_loader() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.21.1-fabric-0.15.0",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader_version: 0.15.0"));
}

#[test]
fn game_install_dry_run_mc_forge_resolves_latest() {
    let home = TestHome::new();
    home.cmd()
        .args(["game", "install", "dev", "mc-forge", "--yes", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: forge"))
        .stdout(predicate::str::contains("loader_version: 52.0.0"));
}

#[test]
fn game_install_dry_run_mc1211_forge_resolves_latest_loader() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.21.1-forge",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: forge"))
        .stdout(predicate::str::contains("loader_version: 52.0.0"));
}

#[test]
fn game_install_dry_run_mc1211_forge_pinned_loader() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.21.1-forge-52.0.0",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader_version: 52.0.0"));
}

#[test]
fn game_install_dry_run_mc_neoforge_resolves_latest() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc-neoforge",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: neoforge"))
        .stdout(predicate::str::contains("loader_version: 21.1.172"));
}

#[test]
fn game_install_dry_run_mc_quilt_resolves_latest() {
    let home = TestHome::new();
    home.cmd()
        .args(["game", "install", "dev", "mc-quilt", "--yes", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: quilt"))
        .stdout(predicate::str::contains("loader_version: 0.27.0"));
}

#[test]
fn game_install_dry_run_mc1211_quilt_resolves_latest_loader() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.21.1-quilt",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: quilt"))
        .stdout(predicate::str::contains("loader_version: 0.27.0"));
}

#[test]
fn game_install_dry_run_mc1211_quilt_pinned_loader() {
    let home = TestHome::new();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.21.1-quilt-0.26.0",
            "--yes",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader_version: 0.26.0"));
}

// ---------------------------------------------------------------------------
// Real install — all loaders with file creation verification
// ---------------------------------------------------------------------------

fn assert_game_files_exist(home: &TestHome, name: &str, version_id: &str) {
    let base = home.mcm_root.join(name).join("versions").join(version_id);

    let version_json = base.join(format!("{version_id}.json"));
    assert!(
        version_json.exists(),
        "version JSON should exist at {}",
        version_json.display()
    );

    let jar = base.join(format!("{version_id}.jar"));
    assert!(jar.exists(), "mock jar should exist at {}", jar.display());
}

fn assert_version_json_has_mojang_fields(home: &TestHome, name: &str, version_id: &str) {
    let path = home
        .mcm_root
        .join(name)
        .join("versions")
        .join(version_id)
        .join(format!("{version_id}.json"));
    let text = fs::read_to_string(&path).expect("read version JSON");
    let json: serde_json::Value = serde_json::from_str(&text).expect("parse version JSON");

    assert_eq!(json["id"], version_id);
    assert_eq!(json["type"], "release");
    assert!(json["mainClass"].is_string(), "mainClass must be present");
    assert!(json["libraries"].is_array(), "libraries must be an array");
    assert!(json["arguments"].is_object(), "arguments must be present");
    assert!(
        json["arguments"]["jvm"].is_array(),
        "jvm arguments must be present"
    );
    assert!(
        json["arguments"]["game"].is_array(),
        "game arguments must be present"
    );
    assert!(json["assetIndex"].is_object(), "assetIndex must be present");
    assert!(
        json["downloads"]["client"].is_object(),
        "client download must be present"
    );
}

#[test]
fn game_install_fabric_creates_metadata_and_loader_files() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args([
            "game",
            "install",
            "fabric-dev",
            "mc1.21.1-fabric-0.15.0",
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed game fabric-dev"));

    home.cmd()
        .args(["game", "info", "fabric-dev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: fabric"))
        .stdout(predicate::str::contains("loader_version: 0.15.0"));

    assert_game_files_exist(&home, "fabric-dev", "1.21.1-fabric-0.15.0");
    assert_version_json_has_mojang_fields(&home, "fabric-dev", "1.21.1-fabric-0.15.0");
}

#[test]
fn game_install_forge_creates_metadata_and_loader_files() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args([
            "game",
            "install",
            "forge-dev",
            "mc1.21.1-forge-52.0.0",
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed game forge-dev"));

    home.cmd()
        .args(["game", "info", "forge-dev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: forge"))
        .stdout(predicate::str::contains("loader_version: 52.0.0"));

    assert_game_files_exist(&home, "forge-dev", "1.21.1-forge-52.0.0");
    assert_version_json_has_mojang_fields(&home, "forge-dev", "1.21.1-forge-52.0.0");
}

#[test]
fn game_install_quilt_creates_metadata_and_loader_files() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args([
            "game",
            "install",
            "quilt-dev",
            "mc1.21.1-quilt-0.27.0",
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed game quilt-dev"));

    home.cmd()
        .args(["game", "info", "quilt-dev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: quilt"))
        .stdout(predicate::str::contains("loader_version: 0.27.0"));

    assert_game_files_exist(&home, "quilt-dev", "1.21.1-quilt-0.27.0");
    assert_version_json_has_mojang_fields(&home, "quilt-dev", "1.21.1-quilt-0.27.0");
}

#[test]
fn game_install_latest_loader_creates_metadata_and_loader_files() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args(["game", "install", "neo-latest", "mc-neoforge", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed game neo-latest"));

    home.cmd()
        .args(["game", "info", "neo-latest"])
        .assert()
        .success()
        .stdout(predicate::str::contains("mc_version: 1.21.1"))
        .stdout(predicate::str::contains("loader: neoforge"))
        .stdout(predicate::str::contains("loader_version: 21.1.172"));

    assert_game_files_exist(&home, "neo-latest", "1.21.1-neoforge-21.1.172");
    assert_version_json_has_mojang_fields(&home, "neo-latest", "1.21.1-neoforge-21.1.172");
}

#[test]
fn game_install_vanilla_creates_version_json_with_mojang_fields() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args(["game", "install", "vanilla", "mc1.21.1", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed game vanilla"));

    assert_version_json_has_mojang_fields(&home, "vanilla", "1.21.1");
}

#[test]
fn game_install_vanilla_no_loader_dir_created() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args(["game", "install", "vanilla", "mc1.21.1", "--yes"])
        .assert()
        .success();

    let version_dir = home
        .mcm_root
        .join("vanilla")
        .join("versions")
        .join("1.21.1");
    let entries: Vec<_> = fs::read_dir(&version_dir)
        .expect("version dir exists")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        !entries
            .iter()
            .any(|e| e == "fabric" || e == "forge" || e == "neoforge" || e == "quilt"),
        "vanilla install must not create loader dirs: {entries:?}"
    );
}

#[test]
fn game_install_persists_all_fields_in_config() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args([
            "game",
            "install",
            "fulltest",
            "mc1.21.1-fabric-0.15.0",
            "--yes",
        ])
        .assert()
        .success();

    let config_toml =
        fs::read_to_string(home.config.join("config.toml")).expect("config.toml exists");
    assert!(
        config_toml.contains(r#"name = "fulltest""#),
        "name in config"
    );
    assert!(
        config_toml.contains(r#"mc_version = "1.21.1""#),
        "mc_version in config"
    );
    assert!(
        config_toml.contains(r#"loader = "fabric""#),
        "loader in config"
    );
    assert!(
        config_toml.contains(r#"loader_version = "0.15.0""#),
        "loader_version in config"
    );
    assert!(
        config_toml.contains(r#"resolved_version_id = "1.21.1-fabric-0.15.0""#),
        "resolved_version_id in config"
    );
}

#[test]
fn game_install_vanilla_version_json_persists() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args(["game", "install", "vanilla-persist", "mc1.20.4", "--yes"])
        .assert()
        .success();

    let path = home
        .mcm_root
        .join("vanilla-persist")
        .join("versions")
        .join("1.20.4")
        .join("1.20.4.json");
    let text = fs::read_to_string(&path).expect("read version JSON");
    let json: serde_json::Value = serde_json::from_str(&text).expect("parse version JSON");
    assert_eq!(json["id"], "1.20.4");
    assert!(json["libraries"].is_array());
    assert!(json["arguments"]["game"].is_array());
}

// ---------------------------------------------------------------------------
// Top-level `install` rejects mc smart targets
// ---------------------------------------------------------------------------

#[test]
fn top_level_install_rejects_mc_smart_target() {
    let home = TestHome::new();
    home.cmd()
        .args(["install", "mc-neoforge"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("game install"));
}

#[test]
fn top_level_install_rejects_raw_mod_name() {
    let home = TestHome::new();
    home.cmd()
        .args(["install", "sodium"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mods install"));
}

// ===========================================================================
// RED tests — compliance gap exposés (Plan 1 Todo 20, Plan 2 Todo 10/14)
//
// These tests MUST FAIL on current code. They prove the prior plans'
// acceptance criteria are NOT met. Do NOT make them pass by modifying
// production code in this todo.
// ===========================================================================

/// Compliance: Plan 2 Todo 10 — canonical loader version layout.
///
/// Prior plan requires `versions/<resolved-id>/<resolved-id>.json` and
/// `<resolved-id>.jar` with a durable version id (e.g. `1.21.1-neoforge-21.1.172`).
/// Current code creates the nested layout `versions/<mc>/<loader>/<loader>-<lv>.jar`.
/// This test asserts the HMCL-compatible flat layout and MUST FAIL.
#[test]
fn canonical_loader_version_layout() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args([
            "game",
            "install",
            "dev",
            "mc1.21.1-neoforge-21.1.172",
            "--yes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed game dev"));

    let resolved_id = "1.21.1-neoforge-21.1.172";
    let version_dir = home.mcm_root.join("dev").join("versions").join(resolved_id);

    assert!(
        version_dir.exists(),
        "canonical version dir should exist at {} (HMCL-compatible flat layout)",
        version_dir.display()
    );

    let version_json = version_dir.join(format!("{resolved_id}.json"));
    assert!(
        version_json.exists(),
        "canonical version JSON should be at {} (not nested under mc_version/loader/)",
        version_json.display()
    );

    let jar = version_dir.join(format!("{resolved_id}.jar"));
    assert!(
        jar.exists(),
        "canonical jar should be at {} (not nested under mc_version/loader/)",
        jar.display()
    );

    let nested_loader_dir = home
        .mcm_root
        .join("dev")
        .join("versions")
        .join("1.21.1")
        .join("neoforge");
    assert!(
        !nested_loader_dir.exists(),
        "nested loader dir {} should NOT exist as primary layout",
        nested_loader_dir.display()
    );
}

/// Compliance: Plan 2 Todo 11 — libraries, assets, and natives materialization.
///
/// Prior plan requires game install to materialize `libraries/` (with downloaded
/// jar artifacts), `assets/indexes/<id>.json`, and `assets/objects/` under the
/// game root. Current install creates none of these. This test asserts their
/// existence and MUST FAIL.
#[test]
fn game_install_materializes_libraries_assets_and_natives() {
    let home = TestHome::new();
    home.init_config();
    home.cmd()
        .args(["game", "install", "vanilla", "mc1.21.1", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed game vanilla"));

    let game_root = home.mcm_root.join("vanilla");

    let libraries_dir = game_root.join("libraries");
    assert!(
        libraries_dir.exists(),
        "libraries/ directory must exist under game root at {}",
        libraries_dir.display()
    );
    let has_library_jar = walk_for_extension(&libraries_dir, "jar");
    assert!(
        has_library_jar,
        "libraries/ must contain at least one .jar artifact under {}",
        libraries_dir.display()
    );

    let assets_indexes = game_root.join("assets").join("indexes");
    assert!(
        assets_indexes.exists(),
        "assets/indexes/ directory must exist at {}",
        assets_indexes.display()
    );
    let has_index_json = walk_for_extension(&assets_indexes, "json");
    assert!(
        has_index_json,
        "assets/indexes/ must contain at least one index JSON under {}",
        assets_indexes.display()
    );

    let assets_objects = game_root.join("assets").join("objects");
    assert!(
        assets_objects.exists(),
        "assets/objects/ directory must exist at {}",
        assets_objects.display()
    );
}

/// Compliance: Plan 2 Todo 10 / Plan 1 Todo 20 — production install must not
/// use mock manifests.
///
/// Prior plan requires real Mojang API calls for `--provider all|modrinth|curseforge`.
/// Current `get_manifests()` ALWAYS returns mock data regardless of provider.
/// This test uses `--provider modrinth` and asserts the install fails with a
/// real network error (not silently succeeding with mock data). MUST FAIL
/// because the current code silently uses mock manifests even for modrinth.
#[test]
fn production_install_does_not_use_mock_manifests() {
    let home = TestHome::new();
    home.init_config();

    let mut cmd = Command::cargo_bin("mcm").expect("mcm binary should be built");
    cmd.args([
        "--config-dir",
        home.config.to_str().unwrap(),
        "--state-dir",
        home.state.to_str().unwrap(),
        "--provider",
        "modrinth",
    ]);
    cmd.args(["game", "install", "prod", "mc1.21.1", "--yes"]);

    let output = cmd.output().expect("run mcm");

    assert!(
        !output.status.success(),
        "install with --provider modrinth should NOT succeed without real Mojang API \
         (current code silently uses mock data — this is the compliance gap we're proving)"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("connection")
            || stderr.contains("network")
            || stderr.contains("connect")
            || stderr.contains("dns")
            || stderr.contains("resolve")
            || stderr.contains("request")
            || stderr.contains("http")
            || stderr.contains("mock"),
        "stderr should mention network/connection error, got: {stderr}"
    );
}

/// Compliance: Plan 1 Todo 14 — dyyl build host protocol.
///
/// Prior plan requires `build_dyyl()` to spawn dyyl with `--host-json` and
/// collect NDJSON streaming protocol events with `source_line` metadata.
/// Current implementation uses a simplified text parser that does NOT produce
/// `source_line` metadata and is NOT deterministic in the host protocol sense.
/// This test asserts the output contains `source_line` metadata and is
/// deterministic. MUST FAIL because the current parser doesn't produce it.
#[test]
fn dyyl_build_produces_host_protocol_output() {
    let home = TestHome::new();
    let tmp = tempfile::tempdir().expect("temp dir");

    let dyyl_path = tmp.path().join("test.dyyl");
    let dyyl_content = "\
# Simple test package
mcm.game.choose(\"dev\", \"1.21.1\");
mcm.mod.install(\"sodium\");
";
    fs::write(&dyyl_path, dyyl_content).expect("write dyyl");

    let output_path = tmp.path().join("test.mcm");

    home.cmd()
        .args([
            "build",
            dyyl_path.to_str().unwrap(),
            "-o",
            output_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    let mcm_content = fs::read_to_string(&output_path).expect("read mcm output");
    let json: serde_json::Value = serde_json::from_str(&mcm_content).expect("parse mcm JSON");

    let steps = json.get("steps").expect("steps must exist");
    let steps_array = steps.as_array().expect("steps must be an array");
    assert!(!steps_array.is_empty(), "steps array must not be empty");

    for (i, step) in steps_array.iter().enumerate() {
        assert!(
            step.get("source_line").is_some(),
            "step {i} must have `source_line` metadata from dyyl host protocol, got: {step}"
        );
    }

    let output_path2 = tmp.path().join("test2.mcm");
    home.cmd()
        .args([
            "build",
            dyyl_path.to_str().unwrap(),
            "-o",
            output_path2.to_str().unwrap(),
        ])
        .assert()
        .success();

    let mcm_content2 = fs::read_to_string(&output_path2).expect("read mcm output 2");
    let json2: serde_json::Value = serde_json::from_str(&mcm_content2).expect("parse mcm JSON 2");

    assert_eq!(
        json.get("steps"),
        json2.get("steps"),
        "build must be deterministic: same .dyyl input must produce identical steps"
    );
}

fn walk_for_extension(dir: &std::path::Path, ext: &str) -> bool {
    if !dir.exists() {
        return false;
    }
    for entry in fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_dir() {
            if walk_for_extension(&path, ext) {
                return true;
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some(ext) {
            return true;
        }
    }
    false
}

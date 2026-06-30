//! Integration tests for the `game` command group and the new typed config
//! model (`GameRecord`, `GameConfig`, `GlobalConfig`, profile→game migration).
//!
//! Test-isolation style mirrors `tests/mvp.rs`: `--config-dir` + `--state-dir`
//! under a `tempfile::TempDir`, with `assert_cmd::Command` + `predicates`.
//!
//! `game install` is stubbed (task 20), so tests seed game records by writing
//! a `config.toml` directly (the same TOML shape `save_config` produces).

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

struct TestHome {
    root: TempDir,
    config: std::path::PathBuf,
    state: std::path::PathBuf,
}

impl TestHome {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let config = root.path().join("config");
        let state = root.path().join("state");
        fs::create_dir_all(&config).expect("config dir");
        fs::create_dir_all(&state).expect("state dir");
        Self {
            root,
            config,
            state,
        }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("mcm").expect("mcm binary should be built by cargo");
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

    /// Write a config.toml with the given TOML body (under `[games.*]` etc.).
    fn write_config(&self, toml_body: &str) {
        fs::write(self.config.join("config.toml"), toml_body).expect("write config");
    }
}

const GAME_DEV_TOML: &str = "\
[global]
root_dir = '/tmp/mcm-test-root'

[games.dev]
name = 'dev'
root_dir = '/tmp/mcm-test-root/dev'
mc_version = '1.20.1'
loader = 'fabric'

[games.dev.version_config]
";

const GAME_DEV_AND_PROD_TOML: &str = "\
[global]
root_dir = '/tmp/mcm-test-root'

[games.dev]
name = 'dev'
root_dir = '/tmp/mcm-test-root/dev'
mc_version = '1.20.1'
loader = 'fabric'

[games.prod]
name = 'prod'
root_dir = '/tmp/mcm-test-root/prod'
mc_version = '1.21.1'
loader = 'neoforge'
";

// ---------------------------------------------------------------------------
// game default
// ---------------------------------------------------------------------------

#[test]
fn game_default_no_arg_no_default_prints_no_default_game() {
    let home = TestHome::new();
    home.cmd()
        .args(["game", "default"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no default game"));
}

#[test]
fn game_default_no_arg_with_default_prints_default_name() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_TOML);
    // Set default first.
    home.cmd()
        .args(["game", "default", "dev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("default game dev"));
    // Query it.
    home.cmd()
        .args(["game", "default"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dev"));
}

#[test]
fn game_default_set_unknown_game_errors() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_TOML);
    home.cmd()
        .args(["game", "default", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown game nonexistent"));
}

#[test]
fn game_default_set_persists_and_shows_in_list() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_TOML);
    home.cmd()
        .args(["game", "default", "dev"])
        .assert()
        .success();
    home.cmd()
        .args(["game", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("* dev"));
}

// ---------------------------------------------------------------------------
// game list
// ---------------------------------------------------------------------------

#[test]
fn game_list_empty_is_silent_success() {
    let home = TestHome::new();
    home.cmd()
        .args(["game", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn game_list_marks_default_with_star() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_AND_PROD_TOML);
    home.cmd()
        .args(["game", "default", "prod"])
        .assert()
        .success();
    home.cmd()
        .args(["game", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("  dev").and(predicate::str::contains("* prod")));
}

#[test]
fn game_list_no_default_shows_no_star() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_AND_PROD_TOML);
    home.cmd()
        .args(["game", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("  dev").and(predicate::str::contains("  prod")));
}

// ---------------------------------------------------------------------------
// game info
// ---------------------------------------------------------------------------

#[test]
fn game_info_shows_root_version_loader_config_paths() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_TOML);
    home.cmd()
        .args(["game", "info", "dev"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("name: dev")
                .and(predicate::str::contains("root_dir: /tmp/mcm-test-root/dev"))
                .and(predicate::str::contains("mc_version: 1.20.1"))
                .and(predicate::str::contains("loader: fabric"))
                .and(predicate::str::contains("java_path: (unset)")),
        );
}

#[test]
fn game_info_unknown_game_errors() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_TOML);
    home.cmd()
        .args(["game", "info", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown game nonexistent"));
}

#[test]
fn game_info_unset_mc_version_and_loader_shows_unset() {
    let home = TestHome::new();
    home.write_config(
        "\
[global]
root_dir = '/tmp/mcm-test-root'

[games.bare]
name = 'bare'
root_dir = '/tmp/mcm-test-root/bare'
",
    );
    home.cmd()
        .args(["game", "info", "bare"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("mc_version: (unset)")
                .and(predicate::str::contains("loader: (unset)")),
        );
}

// ---------------------------------------------------------------------------
// game rename
// ---------------------------------------------------------------------------

#[test]
fn game_rename_updates_config_and_default_pointer() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_TOML);
    home.cmd()
        .args(["game", "default", "dev"])
        .assert()
        .success();
    home.cmd()
        .args(["game", "rename", "dev", "dev2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("renamed game dev -> dev2"));
    // Default pointer should follow the rename.
    home.cmd()
        .args(["game", "default"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dev2"));
    // Old name is gone.
    home.cmd()
        .args(["game", "info", "dev"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown game dev"));
    // New name exists.
    home.cmd()
        .args(["game", "info", "dev2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("name: dev2"));
}

#[test]
fn game_rename_does_not_touch_unrelated_games() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_AND_PROD_TOML);
    home.cmd()
        .args(["game", "rename", "dev", "dev-renamed"])
        .assert()
        .success();
    // prod must still exist unchanged.
    home.cmd()
        .args(["game", "info", "prod"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("name: prod")
                .and(predicate::str::contains("mc_version: 1.21.1")),
        );
}

#[test]
fn game_rename_unknown_old_errors() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_TOML);
    home.cmd()
        .args(["game", "rename", "nonexistent", "whatever"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown game nonexistent"));
}

#[test]
fn game_rename_to_existing_name_errors() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_AND_PROD_TOML);
    home.cmd()
        .args(["game", "rename", "dev", "prod"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("game prod already exists"));
}

// ---------------------------------------------------------------------------
// game config (show)
// ---------------------------------------------------------------------------

#[test]
fn game_config_shows_version_scoped_fields() {
    let home = TestHome::new();
    home.write_config(
        "\
[global]
root_dir = '/tmp/mcm-test-root'

[games.dev]
name = 'dev'
root_dir = '/tmp/mcm-test-root/dev'
mc_version = '1.20.1'
loader = 'fabric'

[games.dev.version_config]
java_path = '/usr/bin/java'
jvm_args = '-Xmx4G'
extra_args = '-Dfoo=bar'
[games.dev.version_config.env]
JAVA_HOME = '/opt/java'
",
    );
    home.cmd()
        .args(["game", "config", "dev"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("game: dev")
                .and(predicate::str::contains("java_path: /usr/bin/java"))
                .and(predicate::str::contains("jvm_args: -Xmx4G"))
                .and(predicate::str::contains("extra_args: -Dfoo=bar"))
                .and(predicate::str::contains("env: JAVA_HOME=/opt/java")),
        );
}

#[test]
fn game_config_unset_fields_show_unset() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_TOML);
    home.cmd()
        .args(["game", "config", "dev"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("java_path: (unset)")
                .and(predicate::str::contains("jvm_args: (unset)"))
                .and(predicate::str::contains("extra_args: (unset)"))
                .and(predicate::str::contains("env: (none)")),
        );
}

#[test]
fn game_config_unknown_game_errors() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_TOML);
    home.cmd()
        .args(["game", "config", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown game nonexistent"));
}

// ---------------------------------------------------------------------------
// game remove
// ---------------------------------------------------------------------------

#[test]
fn game_remove_without_yes_errors() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_TOML);
    home.cmd()
        .args(["game", "remove", "dev"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("confirmation required"));
}

#[test]
fn game_remove_with_yes_removes_record_and_deletes_dir() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_TOML);
    home.cmd()
        .args(["game", "remove", "dev", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed game record: dev"));
    // Game is gone from config.
    home.cmd()
        .args(["game", "info", "dev"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown game dev"));
}

#[test]
fn game_remove_default_clears_default_pointer() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_TOML);
    home.cmd()
        .args(["game", "default", "dev"])
        .assert()
        .success();
    home.cmd()
        .args(["game", "remove", "dev", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("default game cleared"));
    home.cmd()
        .args(["game", "default"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no default game"));
}

#[test]
fn game_remove_unknown_game_errors() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_TOML);
    home.cmd()
        .args(["game", "remove", "nonexistent", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown game nonexistent"));
}

#[test]
fn game_remove_does_not_touch_unrelated_games() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_AND_PROD_TOML);
    home.cmd()
        .args(["game", "remove", "dev", "--yes"])
        .assert()
        .success();
    home.cmd()
        .args(["game", "info", "prod"])
        .assert()
        .success()
        .stdout(predicate::str::contains("name: prod"));
}

// ---------------------------------------------------------------------------
// One-way profile → game migration
// ---------------------------------------------------------------------------

#[test]
fn migration_profiles_appear_as_games_when_games_empty() {
    let home = TestHome::new();
    // Create a profile via the legacy `mods add` path.
    let mods_dir = home.root.path().join("mods");
    fs::create_dir_all(&mods_dir).expect("mods dir");
    home.cmd()
        .args([
            "mods",
            "add",
            "legacy",
            "--mods-dir",
            mods_dir.to_str().unwrap(),
            "--mc-version",
            "1.20.1",
            "--loader",
            "fabric",
        ])
        .assert()
        .success();

    // `game list` should see the migrated profile as a game.
    home.cmd()
        .args(["game", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("legacy"));

    // `game info` should show migrated mc_version and loader.
    home.cmd()
        .args(["game", "info", "legacy"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("mc_version: 1.20.1")
                .and(predicate::str::contains("loader: fabric")),
        );

    // Default game should be set from active_profile.
    home.cmd()
        .args(["game", "default"])
        .assert()
        .success()
        .stdout(predicate::str::contains("legacy"));
}

#[test]
fn migration_does_not_delete_old_profile_data() {
    let home = TestHome::new();
    let mods_dir = home.root.path().join("mods");
    fs::create_dir_all(&mods_dir).expect("mods dir");
    home.cmd()
        .args([
            "mods",
            "add",
            "legacy",
            "--mods-dir",
            mods_dir.to_str().unwrap(),
            "--mc-version",
            "1.20.1",
            "--loader",
            "fabric",
        ])
        .assert()
        .success();

    // `mods profile-list` must still work (old profile data preserved).
    home.cmd()
        .args(["mods", "profile-list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("legacy"));
}

#[test]
fn migration_skipped_when_games_already_exist() {
    let home = TestHome::new();
    // Write a config with BOTH a profile and a game record.
    home.write_config(
        "\
[global]
root_dir = '/tmp/mcm-test-root'

[profiles.legacy]
name = 'legacy'
mods_dir = '/tmp/mcm-test-root/mods'
mc_version = '1.20.1'
loader = 'fabric'
side = 'both'

[games.explicit]
name = 'explicit'
root_dir = '/tmp/mcm-test-root/explicit'
mc_version = '1.21'
loader = 'neoforge'
",
    );
    // `game list` should show ONLY the explicit game, not the migrated profile.
    home.cmd()
        .args(["game", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("explicit").and(predicate::str::contains("legacy").not()));
}

// ---------------------------------------------------------------------------
// Fresh install
// ---------------------------------------------------------------------------

#[test]
fn fresh_install_has_no_games_and_no_default() {
    let home = TestHome::new();
    home.cmd()
        .args(["game", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    home.cmd()
        .args(["game", "default"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no default game"));
}

// ---------------------------------------------------------------------------
// game install requires confirmation (no longer a stub)
// ---------------------------------------------------------------------------

#[test]
fn game_install_valid_target_requires_confirmation() {
    let home = TestHome::new();
    home.cmd()
        .args(["game", "install", "dev", "mc1.21.1-neoforge-21.1.172"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "confirmation required; pass --yes",
        ));
}

#[test]
fn game_install_invalid_target_errors_before_stub() {
    let home = TestHome::new();
    home.cmd()
        .args(["game", "install", "dev", "mc1.21.1-neoforge@latest"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("@latest"));
}

// ===========================================================================
// RED test — compliance gap: game config write support (Plan 1 Todo 5)
// ===========================================================================

/// Compliance: Plan 1 Todo 5 — `game config <name> set <key> <value>` write support.
///
/// Prior plan requires `game config` to support setting fields (java_path,
/// jvm_args, extra_args, env). Current `game_config_show()` is read-only.
/// This test asserts the config is updated after a set command and MUST FAIL
/// because the CLI does not accept `set` arguments.
#[test]
fn game_config_supports_setting_fields() {
    let home = TestHome::new();
    home.write_config(GAME_DEV_TOML);

    home.cmd()
        .args([
            "game",
            "config",
            "dev",
            "set",
            "java_path",
            "/usr/bin/java21",
        ])
        .assert()
        .success();

    home.cmd()
        .args(["game", "config", "dev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("java_path: /usr/bin/java21"));

    let config_toml =
        fs::read_to_string(home.config.join("config.toml")).expect("config.toml exists");
    assert!(
        config_toml.contains("java_path") && config_toml.contains("java21"),
        "config.toml should persist java_path after set command\n---\n{config_toml}\n---"
    );
}

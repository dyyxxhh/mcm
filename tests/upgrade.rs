use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

struct TestHome {
    _root: TempDir,
    config: std::path::PathBuf,
    state: std::path::PathBuf,
    game_root: std::path::PathBuf,
}

impl TestHome {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temp dir");
        let config = root.path().join("config");
        let state = root.path().join("state");
        let game_root = root.path().join("games");
        fs::create_dir_all(&config).expect("config dir");
        fs::create_dir_all(&state).expect("state dir");
        Self {
            _root: root,
            config,
            state,
            game_root,
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

    fn game_dev_toml(&self) -> String {
        format!(
            "[global]\nroot_dir = '{root}'\n\n\
             [games.dev]\nname = 'dev'\nroot_dir = '{root}/dev'\n\
             mc_version = '1.20.1'\nloader = 'fabric'\n\n\
             [games.dev.version_config]\n",
            root = self.game_root.display()
        )
    }

    fn game_dev_and_prod_toml(&self) -> String {
        format!(
            "[global]\nroot_dir = '{root}'\n\n\
             [games.dev]\nname = 'dev'\nroot_dir = '{root}/dev'\n\
             mc_version = '1.20.1'\nloader = 'fabric'\n\n\
             [games.prod]\nname = 'prod'\nroot_dir = '{root}/prod'\n\
             mc_version = '1.20.1'\nloader = 'fabric'\n\n\
             default_game = 'dev'\n",
            root = self.game_root.display()
        )
    }

    fn write_config(&self, toml_body: &str) {
        fs::write(self.config.join("config.toml"), toml_body).expect("write config");
    }

    fn write_lock(&self, profile_name: &str, lock_json: &str) {
        fs::write(
            self.state.join(format!("{profile_name}.lock.json")),
            lock_json,
        )
        .expect("write lock");
    }

    fn read_lock(&self, profile_name: &str) -> String {
        fs::read_to_string(self.state.join(format!("{profile_name}.lock.json"))).unwrap_or_default()
    }

    fn lock_single(&self, version: &str, reason: &str, owner: Option<&str>) -> String {
        let mut lock = format!(
            "{{\"installed\":{{\"rootmod\":{{\"logical_id\":\"rootmod\",\
             \"provider\":\"mock\",\"project_id\":\"rootmod\",\
             \"file_id\":\"rootmod-file\",\"version\":\"{version}\",\
             \"filename\":\"rootmod-{version}.jar\",\"sha256\":\"abc123\",\
             \"reason\":\"{reason}\",\"required_deps\":[\"depmod\"],\
             \"profile\":{{\"mc_version\":\"1.20.1\",\"loader\":\"fabric\",\"side\":\"both\"}},\
             \"installed_at\":\"2026-01-01T00:00:00Z\"}},\
             \"depmod\":{{\"logical_id\":\"depmod\",\
             \"provider\":\"mock\",\"project_id\":\"depmod\",\
             \"file_id\":\"depmod-file\",\"version\":\"1.0.0\",\
             \"filename\":\"depmod-1.0.0.jar\",\"sha256\":\"def456\",\
             \"reason\":\"auto\",\"required_deps\":[],\
             \"profile\":{{\"mc_version\":\"1.20.1\",\"loader\":\"fabric\",\"side\":\"both\"}},\
             \"installed_at\":\"2026-01-01T00:00:00Z\"}}}}}}"
        );
        if let Some(o) = owner {
            let needle = "\"installed_at\":\"2026-01-01T00:00:00Z\"";
            let replacement =
                format!("\"installed_at\":\"2026-01-01T00:00:00Z\",\"owner_id\":\"{o}\"");
            lock = lock.replace(needle, &replacement);
        }
        lock
    }

    fn lock_with_dep(&self, root_version: &str, dep_version: &str) -> String {
        format!(
            "{{\"installed\":{{\
             \"rootmod\":{{\"logical_id\":\"rootmod\",\
             \"provider\":\"mock\",\"project_id\":\"rootmod\",\
             \"file_id\":\"rootmod-file\",\"version\":\"{root_version}\",\
             \"filename\":\"rootmod-{root_version}.jar\",\"sha256\":\"abc123\",\
             \"reason\":\"manual\",\"required_deps\":[\"depmod\"],\
             \"profile\":{{\"mc_version\":\"1.20.1\",\"loader\":\"fabric\",\"side\":\"both\"}},\
             \"installed_at\":\"2026-01-01T00:00:00Z\"}},\
             \"depmod\":{{\"logical_id\":\"depmod\",\
             \"provider\":\"mock\",\"project_id\":\"depmod\",\
             \"file_id\":\"depmod-file\",\"version\":\"{dep_version}\",\
             \"filename\":\"depmod-{dep_version}.jar\",\"sha256\":\"def456\",\
             \"reason\":\"auto\",\"required_deps\":[],\
             \"profile\":{{\"mc_version\":\"1.20.1\",\"loader\":\"fabric\",\"side\":\"both\"}},\
             \"installed_at\":\"2026-01-01T00:00:00Z\"}}}}}}"
        )
    }

    fn lock_without_dep(&self, root_version: &str) -> String {
        format!(
            "{{\"installed\":{{\"rootmod\":{{\"logical_id\":\"rootmod\",\
             \"provider\":\"mock\",\"project_id\":\"rootmod\",\
             \"file_id\":\"rootmod-file\",\"version\":\"{root_version}\",\
             \"filename\":\"rootmod-{root_version}.jar\",\"sha256\":\"abc123\",\
             \"reason\":\"manual\",\"required_deps\":[],\
             \"profile\":{{\"mc_version\":\"1.20.1\",\"loader\":\"fabric\",\"side\":\"both\"}},\
             \"installed_at\":\"2026-01-01T00:00:00Z\"}}}}}}"
        )
    }

    fn lock_with_incompatible(
        &self,
        root_version: &str,
        badmod_id: &str,
        badmod_version: &str,
    ) -> String {
        format!(
            "{{\"installed\":{{\
             \"rootmod\":{{\"logical_id\":\"rootmod\",\
             \"provider\":\"mock\",\"project_id\":\"rootmod\",\
             \"file_id\":\"rootmod-file\",\"version\":\"{root_version}\",\
             \"filename\":\"rootmod-{root_version}.jar\",\"sha256\":\"abc123\",\
             \"reason\":\"manual\",\"required_deps\":[\"depmod\"],\
             \"profile\":{{\"mc_version\":\"1.20.1\",\"loader\":\"fabric\",\"side\":\"both\"}},\
             \"installed_at\":\"2026-01-01T00:00:00Z\"}},\
             \"depmod\":{{\"logical_id\":\"depmod\",\
             \"provider\":\"mock\",\"project_id\":\"depmod\",\
             \"file_id\":\"depmod-file\",\"version\":\"1.0.0\",\
             \"filename\":\"depmod-1.0.0.jar\",\"sha256\":\"def456\",\
             \"reason\":\"auto\",\"required_deps\":[],\
             \"profile\":{{\"mc_version\":\"1.20.1\",\"loader\":\"fabric\",\"side\":\"both\"}},\
             \"installed_at\":\"2026-01-01T00:00:00Z\"}},\
             \"{badmod_id}\":{{\"logical_id\":\"{badmod_id}\",\
             \"provider\":\"mock\",\"project_id\":\"{badmod_id}\",\
             \"file_id\":\"{badmod_id}-file\",\"version\":\"{badmod_version}\",\
             \"filename\":\"{badmod_id}-{badmod_version}.jar\",\"sha256\":\"ghi789\",\
             \"reason\":\"manual\",\"required_deps\":[],\
             \"profile\":{{\"mc_version\":\"1.20.1\",\"loader\":\"fabric\",\"side\":\"both\"}},\
             \"installed_at\":\"2026-01-01T00:00:00Z\"}}}}}}"
        )
    }
}

#[test]
fn upgrade_one_game_old_mods_upgraded() {
    let home = TestHome::new();
    home.write_config(&home.game_dev_toml());
    home.write_lock("dev", &home.lock_single("0.9.0", "manual", None));

    home.cmd()
        .args(["upgrade", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("upgrade plan"));

    let lock = home.read_lock("dev");
    let parsed: serde_json::Value = serde_json::from_str(&lock).expect("parse lock");
    assert_eq!(
        parsed["installed"]["rootmod"]["version"].as_str().unwrap(),
        "1.0.0"
    );
    assert_eq!(
        parsed["installed"]["rootmod"]["reason"].as_str().unwrap(),
        "manual"
    );
}

#[test]
fn full_upgrade_two_games_both_upgraded() {
    let home = TestHome::new();
    home.write_config(&home.game_dev_and_prod_toml());
    home.write_lock("dev", &home.lock_single("0.9.0", "manual", None));
    home.write_lock("prod", &home.lock_single("0.8.0", "manual", None));

    home.cmd()
        .args(["full-upgrade", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dev"))
        .stdout(predicate::str::contains("prod"));

    let dev_lock = home.read_lock("dev");
    let prod_lock = home.read_lock("prod");
    let dev_parsed: serde_json::Value = serde_json::from_str(&dev_lock).expect("parse dev lock");
    let prod_parsed: serde_json::Value = serde_json::from_str(&prod_lock).expect("parse prod lock");

    assert_eq!(
        dev_parsed["installed"]["rootmod"]["version"]
            .as_str()
            .unwrap(),
        "1.0.0"
    );
    assert_eq!(
        prod_parsed["installed"]["rootmod"]["version"]
            .as_str()
            .unwrap(),
        "1.0.0"
    );
}

#[test]
fn upgrade_without_yes_prints_plan_and_bails() {
    let home = TestHome::new();
    home.write_config(&home.game_dev_toml());
    home.write_lock("dev", &home.lock_single("0.9.0", "manual", None));

    home.cmd()
        .arg("upgrade")
        .assert()
        .failure()
        .stdout(predicate::str::contains("upgrade plan"))
        .stderr(predicate::str::contains("confirmation"));
}

#[test]
fn upgrade_already_up_to_date() {
    let home = TestHome::new();
    home.write_config(&home.game_dev_toml());
    home.write_lock("dev", &home.lock_single("1.0.0", "manual", None));

    home.cmd()
        .args(["upgrade", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("already up to date"));
}

#[test]
fn upgrade_no_game_configured_errors() {
    let home = TestHome::new();
    home.write_config("");

    home.cmd()
        .arg("upgrade")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no default game"));
}

#[test]
fn owner_mismatch_refused() {
    let home = TestHome::new();
    home.write_config(&home.game_dev_toml());
    home.write_lock(
        "dev",
        &home.lock_single("0.9.0", "manual", Some("original-author")),
    );

    let lock_before = home.read_lock("dev");

    home.cmd()
        .args(["upgrade", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("owner mismatch"))
        .stdout(predicate::str::contains("refusing"));

    let lock_after = home.read_lock("dev");
    assert_eq!(
        lock_before, lock_after,
        "lock must not change on owner mismatch"
    );
}

#[test]
fn upgrade_preserves_install_reasons() {
    let home = TestHome::new();
    home.write_config(&home.game_dev_toml());
    home.write_lock("dev", &home.lock_single("0.9.0", "manual", None));

    home.cmd().args(["upgrade", "--yes"]).assert().success();

    let lock = home.read_lock("dev");
    let parsed: serde_json::Value = serde_json::from_str(&lock).expect("parse lock");
    assert_eq!(
        parsed["installed"]["rootmod"]["reason"].as_str().unwrap(),
        "manual"
    );
}

#[test]
fn required_dep_missing_skips_upgrade() {
    let home = TestHome::new();
    home.write_config(&home.game_dev_toml());
    home.write_lock("dev", &home.lock_without_dep("0.9.0"));

    let lock_before = home.read_lock("dev");

    home.cmd()
        .args(["upgrade", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("skipped"))
        .stdout(predicate::str::contains("required"))
        .stdout(predicate::str::contains("depmod"));

    let lock_after = home.read_lock("dev");
    assert_eq!(
        lock_before, lock_after,
        "lock must not change when required dep is missing"
    );
}

#[test]
fn incompatible_dep_installed_blocks_upgrade() {
    let home = TestHome::new();
    home.write_config(&home.game_dev_toml());
    home.write_lock(
        "dev",
        &home.lock_with_incompatible("0.9.0", "badmod", "1.0.0"),
    );

    let lock_before = home.read_lock("dev");

    home.cmd()
        .args(["upgrade", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("skipped"))
        .stdout(predicate::str::contains("incompatible"));

    let lock_after = home.read_lock("dev");
    assert_eq!(
        lock_before, lock_after,
        "lock must not change when incompatible dep is installed"
    );
}

#[test]
fn upgrade_dependency_satisfied_proceeds() {
    let home = TestHome::new();
    home.write_config(&home.game_dev_toml());
    home.write_lock("dev", &home.lock_with_dep("0.9.0", "1.0.0"));

    home.cmd()
        .args(["upgrade", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rootmod"));

    let lock = home.read_lock("dev");
    let parsed: serde_json::Value = serde_json::from_str(&lock).expect("parse lock");
    assert_eq!(
        parsed["installed"]["depmod"]["version"].as_str().unwrap(),
        "1.0.0"
    );
}

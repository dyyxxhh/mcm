//! Integration tests for `mcm run [--dry-run]`.
//!
//! Covers:
//! - Happy path: install game + managed Java → `run --dry-run` prints command
//! - Missing default game → actionable error
//! - Missing install → actionable error with game install guidance
//! - Missing runtime → actionable error with game runtime install guidance
//! - Real launch (no --dry-run) → safe not-implemented message
//! - Auth fields in dry-run output
//! - Offline auth mode (default): stable UUID, redacted token
//! - Online auth mode: configured account fields
//! - Config switch between offline/online

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

struct TestHome {
    root: TempDir,
    config: std::path::PathBuf,
    state: std::path::PathBuf,
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

    fn init_config(&self) {
        let toml = format!(
            r#"[global]
root_dir = '{}'
"#,
            self.mcm_root.display()
        );
        fs::write(self.config.join("config.toml"), &toml).expect("write config");
    }

    fn init_config_with_toml(&self, toml: &str) {
        fs::write(self.config.join("config.toml"), toml).expect("write config");
    }

    fn with_no_system_java(&self, cmd: &mut Command) {
        let bindir = self.root.path().join("nopath");
        fs::create_dir_all(&bindir).expect("create nopath dir");
        cmd.env("PATH", bindir.to_str().unwrap());
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

    fn install_game(&self, name: &str, target: &str) {
        self.cmd()
            .args(["game", "install", name, target, "--yes"])
            .assert()
            .success();
    }

    fn install_java(&self, name: &str) {
        self.cmd()
            .args(["game", "runtime", "install", name, "--yes"])
            .assert()
            .success();
    }

    fn set_default(&self, name: &str) {
        self.cmd()
            .args(["game", "default", name])
            .assert()
            .success();
    }
}

// ---------------------------------------------------------------------------
// S1: Happy path — dry-run with game + Java installed (offline default)
// ---------------------------------------------------------------------------

#[test]
fn run_dry_run_prints_launch_command_for_installed_game() {
    let home = TestHome::new();
    home.init_config();
    home.install_game("dev", "mc1.20.1");
    home.set_default("dev");
    home.install_java("dev");

    home.cmd()
        .args(["run", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("java"))
        .stdout(predicate::str::contains("-cp"))
        .stdout(predicate::str::contains("net.minecraft.client.main.Main"))
        .stdout(predicate::str::contains("--username"))
        .stdout(predicate::str::contains("Player"));
}

#[test]
fn run_dry_run_offline_mode_uses_stable_uuid_and_zero_token() {
    let home = TestHome::new();
    home.init_config();
    home.install_game("dev", "mc1.20.1");
    home.set_default("dev");
    home.install_java("dev");

    home.cmd()
        .args(["run", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--uuid"))
        .stdout(predicate::str::contains(
            "a01e3843-e521-3998-958a-f459800e4d11",
        ))
        .stdout(predicate::str::contains("--accessToken"))
        .stdout(predicate::str::contains("0"))
        .stdout(predicate::str::contains("--userType"))
        .stdout(predicate::str::contains("Mojang"));
}

#[test]
fn run_dry_run_with_loader_shows_loader_main_class() {
    let home = TestHome::new();
    home.init_config();
    home.install_game("dev", "mc1.21.1-fabric-0.16.0");
    home.set_default("dev");
    home.install_java("dev");

    let output = home
        .cmd()
        .args(["run", "--dry-run"])
        .output()
        .expect("run should succeed");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("net.minecraft.client.main.Main"),
        "version JSON main class should appear: {stdout}"
    );
    assert!(
        stdout.contains("-cp"),
        "classpath flag should be present: {stdout}"
    );
    assert!(
        stdout.contains("fabric"),
        "classpath should contain fabric resolved version path: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// S2: Missing default game → actionable error
// ---------------------------------------------------------------------------

#[test]
fn run_dry_run_without_default_game_errors_actionably() {
    let home = TestHome::new();
    home.init_config();

    home.cmd()
        .args(["run", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("game install"));
}

// ---------------------------------------------------------------------------
// S4: Default points to non-existent game → actionable error
// ---------------------------------------------------------------------------

#[test]
fn run_dry_run_with_missing_default_game_errors_actionably() {
    let home = TestHome::new();
    home.init_config();
    home.install_game("dev", "mc1.20.1");
    home.set_default("dev");
    home.cmd()
        .args(["game", "remove", "dev", "--yes"])
        .assert()
        .success();

    home.cmd()
        .args(["run", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("default game"));
}

// ---------------------------------------------------------------------------
// S3: Missing runtime → actionable error with runtime install guidance
// ---------------------------------------------------------------------------

#[test]
fn run_dry_run_without_java_errors_actionably() {
    let home = TestHome::new();
    home.init_config();
    home.install_game("dev", "mc1.20.1");
    home.set_default("dev");

    let mut cmd = home.cmd();
    home.with_no_system_java(&mut cmd);
    cmd.args(["run", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("runtime install"));
}

// ---------------------------------------------------------------------------
// S5: Real launch — spawns fake Java, records argv and exits 0
// ---------------------------------------------------------------------------

#[test]
fn run_without_dry_run_spawns_java_and_records_argv() {
    let home = TestHome::new();
    home.init_config();
    home.install_game("dev", "mc1.20.1");
    home.set_default("dev");

    let managed_java_dir = home.mcm_root.join("runtimes/java/java17/bin");
    fs::create_dir_all(&managed_java_dir).expect("create managed java dir");
    let arg_log = home.mcm_root.join("argv.log");
    let java_path = managed_java_dir.join("java");
    let fake_java = format!(
        "#!/bin/bash\nfor arg in \"$@\"; do echo \"$arg\" >> \"{}\"; done\n",
        arg_log.display()
    );
    fs::write(&java_path, &fake_java).expect("write fake java");
    fs::write(managed_java_dir.join("java.version"), "17\n").expect("write marker");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&java_path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    home.cmd().args(["run"]).assert().success();

    let logged_args = fs::read_to_string(&arg_log).expect("read arg log");
    let lines: Vec<&str> = logged_args.lines().collect();
    assert!(!lines.is_empty(), "fake java should have recorded args");
    assert!(
        lines.iter().any(|l| l.starts_with("-Djava.library.path=")),
        "argv should include natives path: {lines:?}"
    );
    assert!(
        lines.contains(&"net.minecraft.client.main.Main"),
        "argv should include main class: {lines:?}"
    );
    assert!(
        lines.contains(&"--username"),
        "argv should include --username: {lines:?}"
    );
    assert!(
        lines.contains(&"-cp"),
        "argv should include -cp flag: {lines:?}"
    );
}

// ---------------------------------------------------------------------------
// S6: Offline auth mode — custom username produces stable offline UUID
// ---------------------------------------------------------------------------

#[test]
fn run_dry_run_offline_custom_username() {
    let home = TestHome::new();
    let toml = format!(
        r#"[global]
root_dir = '{}'

[launch_auth]
mode = "offline"

[launch_auth.online]
username = "CustomPlayer"
uuid = "deadbeef-dead-beef-dead-beefdeadbeef"
access_token = "should-be-ignored"
user_type = "microsoft"
"#,
        home.mcm_root.display()
    );
    home.init_config_with_toml(&toml);
    home.install_game("dev", "mc1.20.1");
    home.set_default("dev");
    home.install_java("dev");

    home.cmd()
        .args(["run", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--username"))
        .stdout(predicate::str::contains("CustomPlayer"))
        .stdout(predicate::str::contains("--accessToken"))
        .stdout(predicate::str::contains("0"));
}

// ---------------------------------------------------------------------------
// S7: Online auth mode — fails clearly without real provider
// ---------------------------------------------------------------------------

#[test]
fn run_dry_run_online_mode_fails_without_real_provider() {
    let home = TestHome::new();
    let toml = format!(
        r#"[global]
root_dir = '{}'

[launch_auth]
mode = "online"

[launch_auth.online]
username = "OnlinePlayer"
uuid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
access_token = "ms-access-token-123"
user_type = "microsoft"
"#,
        home.mcm_root.display()
    );
    home.init_config_with_toml(&toml);
    home.install_game("dev", "mc1.20.1");
    home.set_default("dev");
    home.install_java("dev");

    // Online mode now uses a real MicrosoftAuthProvider instead of a mock.
    // With no refresh token, the provider reports that the session has
    // expired and points the user at `mcm auth login`.
    home.cmd()
        .args(["run", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mcm auth login"));
}

// ---------------------------------------------------------------------------
// S8: Online mode without account → actionable error
// ---------------------------------------------------------------------------

#[test]
fn run_dry_run_online_without_account_errors_actionably() {
    let home = TestHome::new();
    let toml = format!(
        r#"[global]
root_dir = '{}'

[launch_auth]
mode = "online"
"#,
        home.mcm_root.display()
    );
    home.init_config_with_toml(&toml);
    home.install_game("dev", "mc1.20.1");
    home.set_default("dev");
    home.install_java("dev");

    home.cmd()
        .args(["run", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mcm auth login"));
}

// ---------------------------------------------------------------------------
// S9: No YY-ID coupling — offline mode doesn't require server auth
// ---------------------------------------------------------------------------

#[test]
fn run_dry_run_offline_mode_works_without_server_auth() {
    let home = TestHome::new();
    home.init_config();
    home.install_game("dev", "mc1.20.1");
    home.set_default("dev");
    home.install_java("dev");

    let output = home
        .cmd()
        .args(["run", "--dry-run"])
        .output()
        .expect("run should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("oidc"),
        "offline mode should not reference OIDC: {stdout}"
    );
    assert!(
        !stdout.contains("yyid"),
        "offline mode should not reference YY-ID: {stdout}"
    );
    assert!(
        !stdout.contains("session_token"),
        "offline mode should not expose session tokens: {stdout}"
    );
    assert!(output.status.success());
}

// ---------------------------------------------------------------------------
// S10: Complete installed layout — launch uses all layout components
// ---------------------------------------------------------------------------

#[test]
fn run_uses_complete_installed_layout() {
    let home = TestHome::new();
    home.init_config();
    home.install_game("dev", "mc1.20.1");
    home.set_default("dev");

    let managed_java_dir = home.mcm_root.join("runtimes/java/java17/bin");
    fs::create_dir_all(&managed_java_dir).expect("create managed java dir");
    fs::write(managed_java_dir.join("java.version"), "17\n").expect("write marker");
    let java_path = managed_java_dir.join("java");
    fs::write(&java_path, "#!/bin/bash\nexit 0\n").expect("write placeholder java");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&java_path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    let output = home
        .cmd()
        .args(["run", "--dry-run"])
        .output()
        .expect("run should succeed");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("java"), "Java executable: {stdout}");
    assert!(
        stdout.contains("-Djava.library.path="),
        "JVM natives path: {stdout}"
    );
    assert!(stdout.contains("natives"), "natives dir in path: {stdout}");
    assert!(stdout.contains("-cp"), "classpath flag present: {stdout}");
    assert!(
        stdout.contains("net.minecraft.client.main.Main"),
        "main class: {stdout}"
    );
    assert!(stdout.contains("--username"), "auth username: {stdout}");
    assert!(stdout.contains("--uuid"), "auth uuid: {stdout}");
    assert!(
        stdout.contains("--accessToken"),
        "auth access token: {stdout}"
    );
    assert!(stdout.contains("--gameDir"), "game directory: {stdout}");
    assert!(stdout.contains("--assetsDir"), "assets directory: {stdout}");
    assert!(stdout.contains("--userType"), "user type arg: {stdout}");
    assert!(
        stdout.contains("-Dminecraft.launcher.brand=mcm"),
        "launcher brand: {stdout}"
    );
    assert!(
        stdout.contains("-Dminecraft.launcher.version=0.2.0"),
        "launcher version: {stdout}"
    );

    let arg_log = home.mcm_root.join("argv2.log");
    let java_path = home.mcm_root.join("runtimes/java/java17/bin/java");
    let fake_java = format!(
        "#!/bin/bash\nfor arg in \"$@\"; do echo \"$arg\" >> \"{}\"; done\npwd >> \"{}\"\n",
        arg_log.display(),
        arg_log.display()
    );
    fs::write(&java_path, &fake_java).expect("overwrite fake java");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&java_path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    home.cmd().args(["run"]).assert().success();

    let logged = fs::read_to_string(&arg_log).expect("read log");
    let lines: Vec<&str> = logged.lines().collect();
    assert!(!lines.is_empty(), "fake java should have recorded output");

    assert!(
        lines.iter().any(|l| l.starts_with("-Djava.library.path=")),
        "spawned argv should include natives path: {lines:?}"
    );
    assert!(
        lines.contains(&"net.minecraft.client.main.Main"),
        "spawned argv should include main class: {lines:?}"
    );
    assert!(
        lines.contains(&"--username"),
        "spawned argv should include auth: {lines:?}"
    );
    assert!(
        lines.contains(&"-cp"),
        "spawned argv should include -cp: {lines:?}"
    );

    let cwd_line = lines.last().expect("should have cwd line").to_string();
    let expected_cwd = home.mcm_root.join("dev").to_string_lossy().to_string();
    assert_eq!(
        cwd_line, expected_cwd,
        "process cwd should be game root dir"
    );
}

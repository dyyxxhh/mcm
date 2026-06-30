//! Integration tests for Java runtime discovery/install.
//!
//! Covers:
//! - Compatibility matrix selection via `game runtime info`.
//! - Managed Java install through download engine via `game runtime install`.
//! - Confirmation policy: install without `--yes` bails.
//! - Root-required system-wide install error path.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Create a mock java executable that prints the given version string to stderr
/// (mimicking `java -version`), and returns the path. The script is `chmod +x`.
fn make_mock_java(dir: &Path, version_stdout: &str) -> PathBuf {
    let script = format!(
        "#!/bin/sh\necho '{}' >&2\n",
        version_stdout.replace('\'', "'\\''")
    );
    let path = dir.join("java");
    std::fs::write(&path, &script).expect("write mock java");
    let _ = std::process::Command::new("chmod")
        .args(["+x", path.to_str().unwrap()])
        .output();
    path
}

struct TestHome {
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

    /// Create a minimal bin directory with no java, then set PATH to it.
    /// This prevents system Java from being found during managed install tests.
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
}

// ---------------------------------------------------------------------------
// Game runtime info — compatibility matrix discovery
// ---------------------------------------------------------------------------

#[test]
fn game_runtime_info_mc_1_20_1_requires_java_17() {
    let home = TestHome::new();
    home.init_config();

    // Install a game for MC 1.20.1 (requires Java 17).
    home.cmd()
        .args(["game", "install", "dev", "mc1.20.1", "--yes"])
        .assert()
        .success();

    // Runtime info should show the compatibility requirement.
    home.cmd()
        .args(["game", "runtime", "info", "dev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("java required: 17"));
}

#[test]
fn game_runtime_info_mc_1_21_1_requires_java_21() {
    let home = TestHome::new();
    home.init_config();

    home.cmd()
        .args(["game", "install", "dev", "mc1.21.1", "--yes"])
        .assert()
        .success();

    home.cmd()
        .args(["game", "runtime", "info", "dev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("java required: 21"));
}

#[test]
fn game_runtime_info_missing_game_errors() {
    let home = TestHome::new();
    home.init_config();

    home.cmd()
        .args(["game", "runtime", "info", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown game"));
}

#[test]
fn game_runtime_info_no_mc_version_shows_unknown() {
    let home = TestHome::new();

    // Create a game record manually with no mc_version.
    let toml = format!(
        r#"[global]
root_dir = '{}'

[games.test]
name = "test"
root_dir = '{}'
"#,
        home.mcm_root.display(),
        home.mcm_root.display()
    );
    fs::write(home.config.join("config.toml"), &toml).expect("write config");

    // No mc_version is not an error — we gracefully report unknown.
    home.cmd()
        .args(["game", "runtime", "info", "test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(unknown - no mc_version)"));
}

// ---------------------------------------------------------------------------
// Runtime install
// ---------------------------------------------------------------------------

#[test]
fn game_runtime_install_with_yes_creates_managed_java() {
    let home = TestHome::new();
    home.init_config();

    // Install a game for MC 1.20.1 (requires Java 17).
    home.cmd()
        .args(["game", "install", "dev", "mc1.20.1", "--yes"])
        .assert()
        .success();

    // Block system Java from being found so the managed install path runs.
    let mut install_cmd = home.cmd();
    home.with_no_system_java(&mut install_cmd);

    // Install managed Java for that game.
    install_cmd
        .args(["game", "runtime", "install", "dev", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed managed Java 17"));

    // Verify the managed java binary exists.
    let java_bin = home
        .mcm_root
        .join("runtimes")
        .join("java")
        .join("java17")
        .join("bin")
        .join("java");
    assert!(
        java_bin.exists(),
        "managed java should exist at {}",
        java_bin.display()
    );

    // Runtime info should now show managed Java found.
    home.cmd()
        .args(["game", "runtime", "info", "dev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("java required: 17"))
        .stdout(predicate::str::contains("status: found"));
}

#[test]
fn game_runtime_install_without_yes_fails_in_non_interactive() {
    let home = TestHome::new();
    home.init_config();

    home.cmd()
        .args(["game", "install", "dev", "mc1.20.1", "--yes"])
        .assert()
        .success();

    // Block system Java so the install path (not "already available") is hit.
    let mut install_cmd = home.cmd();
    home.with_no_system_java(&mut install_cmd);

    install_cmd
        .args(["game", "runtime", "install", "dev"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "confirmation required; pass --yes",
        ));

    // Verify no managed java was written.
    let java_bin = home
        .mcm_root
        .join("runtimes")
        .join("java")
        .join("java17")
        .join("bin")
        .join("java");
    assert!(
        !java_bin.exists(),
        "no java should be installed without --yes"
    );
}

// ---------------------------------------------------------------------------
// Wrong-major Java rejection
// ---------------------------------------------------------------------------

#[test]
fn game_runtime_install_rejects_wrong_major_user_config() {
    let home = TestHome::new();
    home.init_config();

    // Install a game for MC 1.20.1 (requires Java 17).
    home.cmd()
        .args(["game", "install", "dev", "mc1.20.1", "--yes"])
        .assert()
        .success();

    // User configures java_path to a Java 21 executable.
    let java21 = make_mock_java(home.root.path(), r#"openjdk version "21.0.2" 2024-01-16"#);
    let toml = format!(
        r#"[global]
root_dir = '{}'

[games.dev]
name = "dev"
root_dir = '{}'
mc_version = "1.20.1"

[games.dev.version_config]
java_path = '{}'
"#,
        home.mcm_root.display(),
        home.mcm_root.display(),
        java21.display().to_string().replace("'", "''"),
    );
    fs::write(home.config.join("config.toml"), &toml).expect("write config");

    // Runtime info should show the wrong-major error.
    home.cmd()
        .args(["game", "runtime", "info", "dev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("configured java"));

    // Runtime install should also error.
    home.cmd()
        .args(["game", "runtime", "install", "dev", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("configured java"));
}

#[test]
fn game_runtime_install_rejects_wrong_major_system_java_and_proceeds_with_managed() {
    let home = TestHome::new();
    home.init_config();

    // Install a game for MC 1.20.1 (requires Java 17).
    home.cmd()
        .args(["game", "install", "dev", "mc1.20.1", "--yes"])
        .assert()
        .success();

    // Place a mock Java 8 on PATH so system Java is wrong major.
    let java8_dir = home.root.path().join("java8path");
    fs::create_dir_all(&java8_dir).expect("create java8path dir");
    make_mock_java(&java8_dir, r#"java version "1.8.0_402""#);

    let mut install_cmd = home.cmd();
    install_cmd.env("PATH", java8_dir.to_str().unwrap());

    // Managed install should proceed despite Java 8 on PATH.
    install_cmd
        .args(["game", "runtime", "install", "dev", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed managed Java 17"));

    // Verify managed java binary with version marker exists.
    let marker = home
        .mcm_root
        .join("runtimes")
        .join("java")
        .join("java17")
        .join("bin")
        .join("java.version");
    assert!(marker.exists(), "version marker should exist");
    let marker_content = std::fs::read_to_string(&marker).expect("read marker");
    assert_eq!(marker_content.trim(), "17");
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn game_runtime_install_unknown_game_errors() {
    let home = TestHome::new();
    home.init_config();

    home.cmd()
        .args(["game", "runtime", "install", "nonexistent", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown game"));
}

#[test]
fn game_runtime_install_no_mc_version_errors() {
    let home = TestHome::new();
    let toml = format!(
        r#"[global]
root_dir = '{}'

[games.test]
name = "test"
root_dir = '{}'
"#,
        home.mcm_root.display(),
        home.mcm_root.display()
    );
    fs::write(home.config.join("config.toml"), &toml).expect("write config");

    home.cmd()
        .args(["game", "runtime", "install", "test", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no mc_version"));
}

#[test]
fn game_runtime_install_unknown_mc_version_errors() {
    let home = TestHome::new();
    home.init_config();

    // Create a game with an unknown MC version.
    let toml = format!(
        r#"[global]
root_dir = '{}'

[games.test]
name = "test"
root_dir = '{}'
mc_version = "99.99"
"#,
        home.mcm_root.display(),
        home.mcm_root.display()
    );
    fs::write(home.config.join("config.toml"), &toml).expect("write config");

    home.cmd()
        .args(["game", "runtime", "install", "test", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown MC version"));
}

// ---------------------------------------------------------------------------
// Root-required / system-wide error path
// ---------------------------------------------------------------------------

#[test]
fn root_system_wide_install_bails_with_sudo_command() {
    let home = TestHome::new();
    home.init_config();

    // First install a game so it exists.
    home.cmd()
        .args(["game", "install", "dev", "mc1.20.1", "--yes"])
        .assert()
        .success();

    // --system flag triggers the root escalation helper path,
    // printing the sudo command and bailing with "not implemented".
    home.cmd()
        .args(["game", "runtime", "install", "dev", "--system", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("sudo"));
}

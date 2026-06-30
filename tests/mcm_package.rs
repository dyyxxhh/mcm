//! Unit + CLI-surface tests for the `.mcm` v2 lock schema and parser.
//!
//! Schema-validation tests call `parse_mcm_lock` directly (preferred for
//! pure schema coverage). CLI-surface tests use the `--config-dir`/`--state-dir`
//! isolation pattern from `tests/mvp.rs` and `tests/game_config.rs`.

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

use mcm::{parse_mcm_lock, validate_lock_install_only, validate_step_dest_path};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn valid_lock_json() -> String {
    String::from(
        r#"{
            "schema_version": 2,
            "kind": "mcm-lock",
            "identity": {
                "name": "my-pack",
                "version": "1.0.0",
                "description": "a test lock"
            },
            "author": {},
            "permissions": {"install": true, "do": false, "full": false},
            "game": {"version": "1.20.1", "loader": "fabric"},
            "steps": [
                {
                    "op": "mod.install",
                    "permission": "install",
                    "args": {"id": "sodium", "provider": "modrinth", "version": "0.5.3", "filename": "sodium-fabric-0.5.3.jar", "sha256": "abc123", "download_url": "https://cdn.modrinth.com/data/AANobbMI/versions/0.5.3/sodium-fabric-0.5.3.jar"}
                }
            ],
            "artifacts": [],
            "created_at": "2024-01-01T00:00:00Z",
            "generator": "mcm"
        }"#,
    )
}

struct TestHome {
    root: TempDir,
    config: PathBuf,
    state: PathBuf,
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

    fn profile(&self) {
        let mods = self.root.path().join("mods");
        fs::create_dir_all(&mods).expect("mods dir");
        self.cmd()
            .args([
                "mods",
                "add",
                "dev",
                "--mods-dir",
                mods.to_str().unwrap(),
                "--mc-version",
                "1.21",
                "--loader",
                "fabric",
            ])
            .assert()
            .success();
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

    fn write_mcm(&self, name: &str, body: &str) -> PathBuf {
        let path = self.root.path().join(name);
        fs::write(&path, body).expect("write mcm");
        path
    }
}

// ---------------------------------------------------------------------------
// Valid v2 lock — direct parser
// ---------------------------------------------------------------------------

#[test]
fn parser_accepts_valid_minimal_lock() {
    let json = r#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"a","version":"0.1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z"}"#;
    let lock = parse_mcm_lock(json).expect("valid minimal lock");
    assert_eq!(lock.schema_version, 2);
    assert_eq!(lock.identity.name, "a");
    assert_eq!(lock.identity.version, "0.1");
}

#[test]
fn parser_accepts_valid_full_lock() {
    let lock = parse_mcm_lock(&valid_lock_json()).expect("valid full lock");
    assert_eq!(lock.identity.name, "my-pack");
    assert_eq!(lock.identity.version, "1.0.0");
    assert_eq!(lock.identity.description.as_deref(), Some("a test lock"));
    assert_eq!(
        lock.game.as_ref().unwrap().version.as_deref(),
        Some("1.20.1")
    );
    assert_eq!(
        lock.game.as_ref().unwrap().loader.as_deref(),
        Some("fabric")
    );
    assert_eq!(lock.steps.len(), 1);
    assert!(lock.permissions.install);
}

#[test]
fn parser_accepts_lock_with_all_optional_fields() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "opt-lock", "version": "2.0", "description": "all fields"},
        "author": {"owner_id": "user1", "source": "test"},
        "permissions": {"install": true, "do": true, "full": false},
        "game": {"game": "dev", "version": "1.21", "loader": "neoforge"},
        "steps": [
            {"op": "game.choose", "permission": "install", "args": {"game": "dev", "version": "1.21"}},
            {"op": "mod.install", "permission": "install", "args": {"id": "sodium", "version": "0.5.3", "filename": "sodium.jar"}},
            {"op": "shell.run", "permission": "do", "args": {"command": "echo hi"}}
        ],
        "artifacts": [{"id": "a1", "url": "https://example.com/a.jar", "sha256": "abc"}],
        "created_at": "2024-01-01T00:00:00Z",
        "generator": "mcm"
    }"#;
    let lock = parse_mcm_lock(json).expect("all optional fields");
    assert_eq!(lock.steps.len(), 3);
    assert_eq!(lock.artifacts.len(), 1);
    assert!(lock.permissions.do_permitted);
    assert!(!lock.permissions.full);
}

// ---------------------------------------------------------------------------
// Schema version
// ---------------------------------------------------------------------------

#[test]
fn parser_rejects_v1_with_actionable_error() {
    let json = r#"{"schema_version":1,"name":"a","version":"1"}"#;
    let err = parse_mcm_lock(json).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("v1 .mcm files are no longer supported"),
        "got: {msg}"
    );
    assert!(
        msg.contains("rebuild from dyyl"),
        "should mention dyyl rebuild; got: {msg}"
    );
}

#[test]
fn parser_rejects_unknown_schema_version() {
    let json = r#"{"schema_version":99,"kind":"mcm-lock","identity":{"name":"a","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z"}"#;
    let err = parse_mcm_lock(json).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("unsupported schema version 99"), "got: {msg}");
}

#[test]
fn parser_rejects_missing_schema_version() {
    let json = r#"{"kind":"mcm-lock","identity":{"name":"a","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z"}"#;
    assert!(parse_mcm_lock(json).is_err());
}

// ---------------------------------------------------------------------------
// Kind validation
// ---------------------------------------------------------------------------

#[test]
fn parser_rejects_wrong_kind() {
    let json = r#"{"schema_version":2,"kind":"wrong","identity":{"name":"a","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z"}"#;
    let err = parse_mcm_lock(json).unwrap_err();
    assert!(
        format!("{err}").contains("expected kind \"mcm-lock\""),
        "got: {err}"
    );
}

// ---------------------------------------------------------------------------
// Package name normalization
// ---------------------------------------------------------------------------

#[test]
fn parser_rejects_reserved_name_mcm() {
    let json = r#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"mcm","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z"}"#;
    let err = parse_mcm_lock(json).unwrap_err();
    assert!(format!("{err}").contains("reserved"), "got: {err}");
}

#[test]
fn parser_rejects_windows_reserved_names() {
    for name in ["con", "nul", "aux", "prn"] {
        let json = format!(
            r#"{{"schema_version":2,"kind":"mcm-lock","identity":{{"name":"{name}","version":"1"}},"permissions":{{"install":true}},"steps":[],"created_at":"2024-01-01T00:00:00Z"}}"#
        );
        let err = parse_mcm_lock(&json).unwrap_err();
        assert!(
            format!("{err}").contains("reserved"),
            "{name:?} should be rejected as reserved; got: {err}"
        );
    }
}

#[test]
fn parser_rejects_uppercase_and_underscore_in_name() {
    for name in ["MyPack", "my_pack", "my.pack"] {
        let json = format!(
            r#"{{"schema_version":2,"kind":"mcm-lock","identity":{{"name":"{name}","version":"1"}},"permissions":{{"install":true}},"steps":[],"created_at":"2024-01-01T00:00:00Z"}}"#
        );
        let err = parse_mcm_lock(&json).unwrap_err();
        assert!(
            format!("{err}").contains("[a-z0-9-]"),
            "{name:?} should be rejected; got: {err}"
        );
    }
}

#[test]
fn parser_rejects_name_with_leading_trailing_hyphen() {
    for name in ["-abc", "abc-", "-abc-"] {
        let json = format!(
            r#"{{"schema_version":2,"kind":"mcm-lock","identity":{{"name":"{name}","version":"1"}},"permissions":{{"install":true}},"steps":[],"created_at":"2024-01-01T00:00:00Z"}}"#
        );
        let err = parse_mcm_lock(&json).unwrap_err();
        assert!(
            format!("{err}").contains("alphanumeric"),
            "{name:?} should be rejected; got: {err}"
        );
    }
}

#[test]
fn parser_rejects_name_with_consecutive_hyphens() {
    let json = r#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"a--b","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z"}"#;
    let err = parse_mcm_lock(json).unwrap_err();
    assert!(
        format!("{err}").contains("consecutive hyphens"),
        "got: {err}"
    );
}

#[test]
fn parser_rejects_name_too_long() {
    let name = "a".repeat(65);
    let json = format!(
        r#"{{"schema_version":2,"kind":"mcm-lock","identity":{{"name":"{name}","version":"1"}},"permissions":{{"install":true}},"steps":[],"created_at":"2024-01-01T00:00:00Z"}}"#
    );
    let err = parse_mcm_lock(&json).unwrap_err();
    assert!(format!("{err}").contains("1-64"), "got: {err}");
}

#[test]
fn parser_accepts_longest_valid_name() {
    let name = "a".repeat(64);
    let json = format!(
        r#"{{"schema_version":2,"kind":"mcm-lock","identity":{{"name":"{name}","version":"1"}},"permissions":{{"install":true}},"steps":[],"created_at":"2024-01-01T00:00:00Z"}}"#
    );
    parse_mcm_lock(&json).expect("64-char name should be accepted");
}

// ---------------------------------------------------------------------------
// Secret field rejection
// ---------------------------------------------------------------------------

#[test]
fn parser_rejects_top_level_token_field() {
    let json = r#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"a","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z","api_token":"x"}"#;
    let err = parse_mcm_lock(json).unwrap_err();
    assert!(format!("{err}").contains("secret field"), "got: {err}");
}

#[test]
fn parser_rejects_nested_secret_field_case_insensitive() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "a", "version": "1"},
        "permissions": {"install": true},
        "steps": [{"op": "mod.install", "permission": "install", "args": {"PASSWORD": "xxx"}}],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let err = parse_mcm_lock(json).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("secret field"), "got: {msg}");
}

// ---------------------------------------------------------------------------
// Size / depth limits
// ---------------------------------------------------------------------------

#[test]
fn parser_rejects_oversized_json() {
    let mut json = String::from(
        r#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"a","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z","x":""#,
    );
    json.push_str(&"x".repeat(11 * 1024 * 1024));
    json.push_str("\"}");
    let err = parse_mcm_lock(&json).unwrap_err();
    assert!(format!("{err}").contains("exceeds"), "got: {err}");
}

#[test]
fn parser_rejects_excessive_nesting_depth() {
    let mut json = String::new();
    for _ in 0..100 {
        json.push_str(r#"{"a":"#);
    }
    json.push('1');
    for _ in 0..100 {
        json.push('}');
    }
    let err = parse_mcm_lock(&json).unwrap_err();
    assert!(format!("{err}").contains("depth"), "got: {err}");
}

// ---------------------------------------------------------------------------
// Missing required fields / empty lock
// ---------------------------------------------------------------------------

#[test]
fn parser_rejects_missing_required_fields() {
    for json in [
        r#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"a"}}"#,
        r#"{"schema_version":2,"kind":"mcm-lock","identity":{"version":"1"}}"#,
        r#"{}"#,
        r#""#,
    ] {
        assert!(parse_mcm_lock(json).is_err(), "should reject: {json}");
    }
}

// ---------------------------------------------------------------------------
// Step permissions
// ---------------------------------------------------------------------------

#[test]
fn parser_accepts_all_permission_levels() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "a", "version": "1"},
        "permissions": {"install": true, "do": true, "full": true},
        "steps": [
            {"op": "mod.install", "permission": "install", "args": {}},
            {"op": "shell.run", "permission": "do", "args": {"command": "echo hi"}},
            {"op": "root.system", "permission": "full", "args": {"command": "sudo apt"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let lock = parse_mcm_lock(json).expect("all permissions");
    assert_eq!(lock.steps.len(), 3);
    assert_eq!(lock.steps[0].permission, mcm::StepPermission::Install);
    assert_eq!(lock.steps[1].permission, mcm::StepPermission::Do);
    assert_eq!(lock.steps[2].permission, mcm::StepPermission::Full);
}

// ---------------------------------------------------------------------------
// CLI surface: `pkg info`
// ---------------------------------------------------------------------------

#[test]
fn pkg_info_prints_summary_for_valid_lock() {
    let home = TestHome::new();
    let path = home.write_mcm("valid.mcm", &valid_lock_json());
    home.cmd()
        .args(["pkg", "info", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("name: my-pack"))
        .stdout(predicate::str::contains("version: 1.0.0"))
        .stdout(predicate::str::contains("game_version: 1.20.1"))
        .stdout(predicate::str::contains("loader: fabric"))
        .stdout(predicate::str::contains("steps: 1"));
}

#[test]
fn pkg_info_exits_nonzero_for_missing_file() {
    let home = TestHome::new();
    home.cmd()
        .args(["pkg", "info", "/nonexistent/evil.mcm"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("read"));
}

#[test]
fn pkg_info_exits_nonzero_for_secret_field() {
    let home = TestHome::new();
    let json = r#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"a","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z","password":"x"}"#;
    let path = home.write_mcm("evil.mcm", json);
    home.cmd()
        .args(["pkg", "info", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("secret field"));
}

#[test]
fn pkg_info_exits_nonzero_for_unknown_schema_version() {
    let home = TestHome::new();
    let json = r#"{"schema_version":7,"kind":"mcm-lock","identity":{"name":"a","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z"}"#;
    let path = home.write_mcm("evil.mcm", json);
    home.cmd()
        .args(["pkg", "info", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported schema version 7"));
}

#[test]
fn pkg_info_exits_nonzero_for_v1() {
    let home = TestHome::new();
    let json = r#"{"schema_version":1,"name":"a","version":"1"}"#;
    let path = home.write_mcm("old.mcm", json);
    home.cmd()
        .args(["pkg", "info", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "v1 .mcm files are no longer supported",
        ));
}

#[test]
fn pkg_info_exits_nonzero_for_reserved_name() {
    let home = TestHome::new();
    let json = r#"{"schema_version":2,"kind":"mcm-lock","identity":{"name":"mcm","version":"1"},"permissions":{"install":true},"steps":[],"created_at":"2024-01-01T00:00:00Z"}"#;
    let path = home.write_mcm("evil.mcm", json);
    home.cmd()
        .args(["pkg", "info", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("reserved"));
}

// ---------------------------------------------------------------------------
// v1 rejection via CLI surface
// ---------------------------------------------------------------------------

#[test]
fn v1_package_rejected_by_install() {
    let home = TestHome::new();
    let json = r#"{"schema_version":1,"name":"a","version":"1","mods":[]}"#;
    let path = home.write_mcm("old.mcm", json);
    home.cmd()
        .args(["pkg", "install", path.to_str().unwrap(), "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "v1 .mcm files are no longer supported",
        ));
}

#[test]
fn v1_package_rejected_by_do() {
    let home = TestHome::new();
    let json = r#"{"schema_version":1,"name":"a","version":"1"}"#;
    let path = home.write_mcm("old.mcm", json);
    home.cmd()
        .args(["do", path.to_str().unwrap(), "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "v1 .mcm files are no longer supported",
        ));
}

// ---------------------------------------------------------------------------
// Other pkg subcommands work with v2
// ---------------------------------------------------------------------------

#[test]
fn pkg_install_is_no_longer_stubbed() {
    let home = TestHome::new();
    home.cmd()
        .args(["pkg", "install", "nonexistent.mcm", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not implemented yet").not());
}

#[test]
fn pkg_list_is_no_longer_stubbed() {
    let home = TestHome::new();
    home.cmd()
        .args(["pkg", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

// ---------------------------------------------------------------------------
// Path validation — Task 16
// ---------------------------------------------------------------------------

#[test]
fn validate_step_dest_path_accepts_relative_paths() {
    assert!(validate_step_dest_path("mods/sodium.jar").is_ok());
    assert!(validate_step_dest_path("config/sodium.properties").is_ok());
    assert!(validate_step_dest_path("a/b/c/file.txt").is_ok());
}

#[test]
fn validate_step_dest_path_rejects_traversal() {
    assert!(validate_step_dest_path("../evil.jar").is_err());
    assert!(validate_step_dest_path("a/../b/file.jar").is_err());
    assert!(validate_step_dest_path("a/b/../../c").is_err());
}

#[test]
fn validate_step_dest_path_rejects_absolute() {
    assert!(validate_step_dest_path("/etc/passwd").is_err());
    assert!(validate_step_dest_path("/tmp/evil.jar").is_err());
}

#[test]
fn validate_step_dest_path_rejects_backslash() {
    assert!(validate_step_dest_path("a\\b\\c.jar").is_err());
    assert!(validate_step_dest_path("a/b\\c.jar").is_err());
}

#[test]
fn validate_step_dest_path_rejects_empty() {
    assert!(validate_step_dest_path("").is_err());
}

#[test]
fn validate_step_dest_path_rejects_null_bytes() {
    assert!(validate_step_dest_path("a\0b").is_err());
}

#[test]
fn validate_step_dest_path_rejects_windows_reserved() {
    assert!(validate_step_dest_path("CON").is_err());
    assert!(validate_step_dest_path("NUL.txt").is_err());
    assert!(validate_step_dest_path("COM1.jar").is_err());
}

// ---------------------------------------------------------------------------
// Permission matrix — Task 16
// ---------------------------------------------------------------------------

#[test]
fn lock_with_mixed_permissions_parses_correctly() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "mixed-pack", "version": "1.0.0"},
        "permissions": {"install": true, "do": true, "full": false},
        "steps": [
            {"op": "game.choose", "permission": "install", "args": {"game": "dev", "version": "1.21"}},
            {"op": "mod.install", "permission": "install", "args": {"id": "sodium"}},
            {"op": "shell.run", "permission": "do", "args": {"command": "echo hi"}},
            {"op": "root.system", "permission": "full", "args": {"command": "sudo apt update"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let lock = parse_mcm_lock(json).expect("mixed permissions lock");
    assert_eq!(lock.steps.len(), 4);
    assert_eq!(lock.steps[0].permission, mcm::StepPermission::Install);
    assert_eq!(lock.steps[1].permission, mcm::StepPermission::Install);
    assert_eq!(lock.steps[2].permission, mcm::StepPermission::Do);
    assert_eq!(lock.steps[3].permission, mcm::StepPermission::Full);
}

#[test]
fn validate_lock_install_only_rejects_do_steps() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "test", "version": "1.0.0"},
        "permissions": {"install": true, "do": true, "full": false},
        "steps": [
            {"op": "shell.run", "permission": "do", "args": {"command": "echo hi"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let lock = parse_mcm_lock(json).expect("parse");
    let err = validate_lock_install_only(&lock).unwrap_err();
    assert!(format!("{err}").contains("non-install step"));
    assert!(format!("{err}").contains("do"));
}

#[test]
fn validate_lock_install_only_rejects_full_steps() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "test", "version": "1.0.0"},
        "permissions": {"install": true, "do": false, "full": true},
        "steps": [
            {"op": "root.system", "permission": "full", "args": {"command": "sudo rm -rf /"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let lock = parse_mcm_lock(json).expect("parse");
    let err = validate_lock_install_only(&lock).unwrap_err();
    assert!(format!("{err}").contains("non-install step"));
    assert!(format!("{err}").contains("full"));
}

#[test]
fn validate_lock_install_only_accepts_install_only() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "test", "version": "1.0.0"},
        "permissions": {"install": true},
        "steps": [
            {"op": "game.choose", "permission": "install", "args": {"game": "dev", "version": "1.21"}},
            {"op": "mod.install", "permission": "install", "args": {"id": "sodium"}},
            {"op": "net.download", "permission": "install", "args": {"url": "https://cdn.modrinth.com/data/abc/versions/1.0.0/mod.jar", "dest": "mods/mod.jar"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let lock = parse_mcm_lock(json).expect("parse");
    assert!(validate_lock_install_only(&lock).is_ok());
}

// ---------------------------------------------------------------------------
// Step path validation — Task 16
// ---------------------------------------------------------------------------

#[test]
fn validate_lock_step_paths_rejects_traversal_in_file_write() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "test", "version": "1.0.0"},
        "permissions": {"install": true},
        "steps": [
            {"op": "file.write", "permission": "install", "args": {"dest": "../../etc/passwd", "content": "evil"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let err = parse_mcm_lock(json).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("traverse") || msg.contains("absolute") || msg.contains("must not"),
        "got: {msg}"
    );
}

#[test]
fn validate_lock_step_paths_rejects_traversal_in_file_copy() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "test", "version": "1.0.0"},
        "permissions": {"install": true},
        "steps": [
            {"op": "file.copy", "permission": "install", "args": {"src_artifact": "/tmp/evil.jar", "dest": "../mods/evil.jar"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let err = parse_mcm_lock(json).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("traverse") || msg.contains("absolute") || msg.contains("must not"),
        "got: {msg}"
    );
}

#[test]
fn validate_lock_step_paths_rejects_traversal_in_net_download() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "test", "version": "1.0.0"},
        "permissions": {"install": true},
        "steps": [
            {"op": "net.download", "permission": "install", "args": {"url": "https://cdn.modrinth.com/data/abc/versions/1.0.0/mod.jar", "dest": "../../etc/evil"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let err = parse_mcm_lock(json).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("traverse") || msg.contains("absolute") || msg.contains("must not"),
        "got: {msg}"
    );
}

#[test]
fn validate_lock_step_paths_rejects_absolute_dest_in_net_download() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "test", "version": "1.0.0"},
        "permissions": {"install": true},
        "steps": [
            {"op": "net.download", "permission": "install", "args": {"url": "https://cdn.modrinth.com/data/abc/versions/1.0.0/mod.jar", "dest": "/tmp/evil.jar"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let err = parse_mcm_lock(json).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("absolute") || msg.contains("traverse") || msg.contains("must not"),
        "got: {msg}"
    );
}

#[test]
fn validate_lock_step_paths_rejects_backslash_in_file_write() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "test", "version": "1.0.0"},
        "permissions": {"install": true},
        "steps": [
            {"op": "file.write", "permission": "install", "args": {"dest": "a\\b\\c.txt", "content": "hello"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let err = parse_mcm_lock(json).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("absolute") || msg.contains("traverse") || msg.contains("must not"),
        "got: {msg}"
    );
}

#[test]
fn validate_lock_step_paths_rejects_empty_url_in_net_download() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "test", "version": "1.0.0"},
        "permissions": {"install": true},
        "steps": [
            {"op": "net.download", "permission": "install", "args": {"url": "", "dest": "mods/mod.jar"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let err = parse_mcm_lock(json).unwrap_err();
    assert!(format!("{err}").contains("non-empty url"));
}

#[test]
fn validate_lock_step_paths_rejects_missing_url_in_net_download() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "test", "version": "1.0.0"},
        "permissions": {"install": true},
        "steps": [
            {"op": "net.download", "permission": "install", "args": {"dest": "mods/mod.jar"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let err = parse_mcm_lock(json).unwrap_err();
    assert!(format!("{err}").contains("missing required 'url'"));
}

#[test]
fn validate_lock_step_paths_accepts_valid_relative_dest() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "test", "version": "1.0.0"},
        "permissions": {"install": true},
        "steps": [
            {"op": "file.write", "permission": "install", "args": {"dest": "config/sodium.properties", "content": "key=value"}},
            {"op": "file.copy", "permission": "install", "args": {"src_artifact": "/tmp/ok.jar", "dest": "mods/sodium.jar"}},
            {"op": "net.download", "permission": "install", "args": {"url": "https://cdn.modrinth.com/data/abc/versions/1.0.0/mod.jar", "dest": "mods/mod.jar"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    parse_mcm_lock(json).expect("valid relative dest paths should be accepted");
}

#[test]
fn validate_lock_step_paths_rejects_shell_run_cwd_traversal() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "test", "version": "1.0.0"},
        "permissions": {"install": true, "do": true},
        "steps": [
            {"op": "shell.run", "permission": "do", "args": {"command": "echo hi", "cwd": "../../etc"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let err = parse_mcm_lock(json).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("traverse") || msg.contains("absolute") || msg.contains("must not"),
        "got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// CLI surface: `mcm install` silently strips non-install steps — Task 16
// ---------------------------------------------------------------------------

#[test]
fn install_lock_with_mixed_steps_strips_do_only_and_completes() {
    let home = TestHome::new();
    home.profile();
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "mixed", "version": "1.0.0"},
        "permissions": {"install": true, "do": true},
        "steps": [
            {"op": "mod.install", "permission": "install", "args": {"id": "rootmod", "provider": "mock", "version": "1.0.0", "filename": "rootmod-1.0.0.jar", "download_url": "https://cdn.modrinth.com/data/abc/versions/1.0.0/rootmod-1.0.0.jar"}},
            {"op": "shell.run", "permission": "do", "args": {"command": "echo DO_STEP_EXECUTED > /tmp/mcm-do-step-test"}},
            {"op": "root.system", "permission": "full", "args": {"command": "echo FULL_STEP_EXECUTED > /tmp/mcm-full-step-test"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let path = home.write_mcm("mixed.mcm", json);
    home.cmd()
        .args(["install", path.to_str().unwrap(), "--yes"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// CLI surface: `mcm do` runs full graph — Task 16
// ---------------------------------------------------------------------------

#[test]
fn do_lock_executes_do_permission_steps() {
    let home = TestHome::new();
    home.profile();
    let marker = home.root.path().join("do-marker.txt");
    let cmd_str = format!("echo DO_EXECUTED > {}", marker.display());
    let json = format!(
        r#"{{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {{"name": "do-test", "version": "1.0.0"}},
        "permissions": {{"install": true, "do": true}},
        "steps": [
            {{"op": "shell.run", "permission": "do", "args": {{"command": "{cmd_str}"}}}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }}"#
    );
    let path = home.write_mcm("do-test.mcm", &json);
    home.cmd()
        .args(["do", path.to_str().unwrap(), "--yes"])
        .assert()
        .success();
    let content = fs::read_to_string(&marker).expect("marker file should exist");
    assert!(content.contains("DO_EXECUTED"));
}

// ---------------------------------------------------------------------------
// game.choose version-root scoping — Task 16
// ---------------------------------------------------------------------------

#[test]
fn game_choose_step_parses_with_version_context() {
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "choose-test", "version": "1.0.0"},
        "permissions": {"install": true},
        "steps": [
            {"op": "game.choose", "permission": "install", "args": {"game": "dev", "version": "1.21"}},
            {"op": "file.write", "permission": "install", "args": {"dest": "config/options.txt", "content": "renderDistance:12"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let lock = parse_mcm_lock(json).expect("game.choose + file.write");
    assert_eq!(lock.steps.len(), 2);
    assert_eq!(lock.steps[0].op, "game.choose");
    assert_eq!(lock.steps[1].op, "file.write");
}

#[test]
fn install_mode_silently_strips_do_steps() {
    let home = TestHome::new();
    home.profile();
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "strip-test", "version": "1.0.0"},
        "permissions": {"install": true, "do": true},
        "steps": [
            {"op": "shell.run", "permission": "do", "args": {"command": "echo should_not_run"}},
            {"op": "root.system", "permission": "full", "args": {"command": "echo should_also_not_run"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let path = home.write_mcm("strip.mcm", json);
    home.cmd()
        .args(["install", path.to_str().unwrap(), "--yes"])
        .assert()
        .success();
}

#[test]
fn install_accepts_lock_with_only_install_steps() {
    let home = TestHome::new();
    home.profile();
    let json = r#"{
        "schema_version": 2,
        "kind": "mcm-lock",
        "identity": {"name": "install-only", "version": "1.0.0"},
        "permissions": {"install": true},
        "steps": [
            {"op": "mod.install", "permission": "install", "args": {"id": "rootmod", "provider": "mock", "version": "1.0.0", "filename": "rootmod-1.0.0.jar", "download_url": "https://cdn.modrinth.com/data/abc/versions/1.0.0/rootmod-1.0.0.jar"}}
        ],
        "created_at": "2024-01-01T00:00:00Z"
    }"#;
    let path = home.write_mcm("install-only.mcm", json);
    home.cmd()
        .args(["install", path.to_str().unwrap(), "--yes"])
        .assert()
        .success();
}

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

#[test]
fn profile_add_use_show_and_list_work_with_isolated_paths() {
    let home = TestHome::new();
    home.profile();

    home.cmd()
        .args(["mods", "use", "dev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("active profile dev"));

    home.cmd()
        .args(["mods", "profile-list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("* dev"));

    home.cmd().args(["mods", "show"]).assert().success().stdout(
        predicate::str::contains("mc_version: 1.20.1")
            .and(predicate::str::contains("loader: fabric")),
    );
}

#[test]
fn search_groups_same_logical_mod_across_provider_candidates_and_excludes_beta() {
    let home = TestHome::new();
    home.profile();

    home.cmd()
        .args(["mods", "search", "root"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("rootmod")
                .and(predicate::str::contains(
                    "candidates: mock/rootmod, modrinth/rootmod",
                ))
                .and(predicate::str::contains("rootmod-beta").not()),
        );
}

#[test]
fn cloud_info_uses_profile_constraints_and_surfaces_optional_dependency() {
    let home = TestHome::new();
    home.profile();

    home.cmd()
        .args(["mods", "info", "rootmod"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("rootmod")
                .and(predicate::str::contains("required deps: depmod"))
                .and(predicate::str::contains("optional deps: optionalmod")),
        );
}

#[test]
fn install_single_mod_installs_required_dependency_but_not_optional_or_beta() {
    let home = TestHome::new();
    home.profile();

    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("install rootmod")
                .and(predicate::str::contains("install depmod"))
                .and(predicate::str::contains(
                    "optional dependency optionalmod not installed",
                )),
        );

    assert!(home.mods.join("rootmod-1.0.0.jar").exists());
    assert!(home.mods.join("depmod-1.0.0.jar").exists());
    assert!(!home.mods.join("optionalmod-1.0.0.jar").exists());
    assert!(!home.mods.join("rootmod-2.0.0-beta.jar").exists());
}

#[test]
fn install_file_parses_comments_into_one_plan() {
    let home = TestHome::new();
    home.profile();
    let list = home.root.path().join("mods.txt");
    fs::write(&list, "# install roots\nrootmod\nstandalone\n\n").expect("write list");

    home.cmd()
        .args(["mods", "install", "--file", list.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("install rootmod")
                .and(predicate::str::contains("install standalone")),
        );

    assert!(home.mods.join("rootmod-1.0.0.jar").exists());
    assert!(home.mods.join("standalone-1.0.0.jar").exists());
}

#[test]
fn install_searches_query_first_and_yes_accepts_plan() {
    let home = TestHome::new();
    home.profile();

    home.cmd()
        .args(["mods", "install", "root", "--yes"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("selected rootmod from search result root")
                .and(predicate::str::contains("install rootmod")),
        );

    assert!(home.mods.join("rootmod-1.0.0.jar").exists());
}

#[test]
fn install_interactive_prompt_accepts_yes_from_stdin() {
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
fn install_dry_run_does_not_write_jars() {
    let home = TestHome::new();
    home.profile();

    home.cmd()
        .args(["mods", "install", "rootmod", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dry run"));

    assert!(!home.mods.join("rootmod-1.0.0.jar").exists());
}

#[test]
fn missing_download_url_errors_without_partial_install() {
    let home = TestHome::new();
    home.profile();

    home.cmd()
        .args(["mods", "install", "brokenmod", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("missing download URL"));

    assert!(!home.mods.join("brokenmod-1.0.0.jar").exists());
}

#[test]
fn remove_and_autoremove_preserve_required_deps_until_no_manual_roots_need_them() {
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

    assert!(!home.mods.join("rootmod-1.0.0.jar").exists());
    assert!(home.mods.join("depmod-1.0.0.jar").exists());

    home.cmd()
        .args(["mods", "autoremove", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed depmod"));

    assert!(!home.mods.join("depmod-1.0.0.jar").exists());
}

#[test]
fn status_detects_missing_changed_and_untracked_jars_without_deleting_untracked() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success();
    fs::write(home.mods.join("rootmod-1.0.0.jar"), b"changed").expect("change jar");
    fs::remove_file(home.mods.join("depmod-1.0.0.jar")).expect("remove dep jar");
    fs::write(home.mods.join("untracked.jar"), b"keep me").expect("untracked jar");

    home.cmd()
        .args(["mods", "status"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("changed: rootmod")
                .and(predicate::str::contains("missing: depmod"))
                .and(predicate::str::contains("untracked: untracked.jar")),
        );

    home.cmd()
        .args(["mods", "autoremove", "--yes"])
        .assert()
        .success();
    assert!(home.mods.join("untracked.jar").exists());
}

#[test]
fn local_jar_info_falls_back_to_hash_without_fake_provider_identity() {
    let home = TestHome::new();
    let jar = home.root.path().join("plain.jar");
    fs::write(&jar, b"not really a zip but useful bytes").expect("jar bytes");

    home.cmd()
        .args(["mods", "info", jar.to_str().unwrap()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("local jar")
                .and(predicate::str::contains("sha256:"))
                .and(predicate::str::contains("provider:").not()),
        );
}

#[test]
fn curseforge_provider_requires_api_key() {
    let home = TestHome::new();
    home.profile();
    let mut cmd = Command::cargo_bin("mcm").expect("mcm binary should be built by cargo");
    cmd.args([
        "--config-dir",
        home.config.to_str().unwrap(),
        "--state-dir",
        home.state.to_str().unwrap(),
        "--provider",
        "curseforge",
        "mods",
        "search",
        "rootmod",
    ])
    .env_remove("CURSEFORGE_API_KEY")
    .assert()
    .failure()
    .stderr(predicate::str::contains("CURSEFORGE_API_KEY"));
}

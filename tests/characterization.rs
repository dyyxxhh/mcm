//! Baseline characterization tests for the mcm CLI.
//!
//! These tests pin the EXACT observable behavior of the mod-manager command
//! grammar (now under `mods`/`mod`: `add`/`use`/`search`/`info`/`install`/
//! `list`/`status`/`remove`/`uninstall`/`autoremove`/`show`/`profile-list`)
//! after the Task 4 command-grammar refactor. They assert behavior — quirks
//! included — so a later refactor cannot silently regress it.
//!
//! Per the plan (`.omo/plans/mcm-minecraft-manager-expansion.md`):
//! - Old top-level spelling compatibility is NOT required; tests migrated to
//!   new `mods` spelling in Task 4.
//! - All cloud behavior uses `--provider mock`; no real network is hit.
//!
//! Test-isolation style mirrors `tests/mvp.rs`: `--config-dir` + `--state-dir`
//! under a `tempfile::TempDir`, with `assert_cmd::Command` + `predicates`.

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

    /// Build a mock-provider command with isolated config/state dirs.
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

    /// Add the default `dev` profile and make it active (the current `add`
    /// semantics: adding a profile also sets it active).
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
// Profile command characterization
// ---------------------------------------------------------------------------

#[test]
fn profile_add_sets_new_profile_active_and_creates_mods_dir() {
    let home = TestHome::new();
    // `add` prints exactly this and exits 0.
    home.cmd()
        .args([
            "mods",
            "add",
            "dev",
            "--mods-dir",
            home.mods.to_str().unwrap(),
            "--mc-version",
            "1.20.1",
            "--loader",
            "fabric",
        ])
        .assert()
        .success()
        .stdout(predicate::eq("added profile dev\n"));

    // Adding a profile makes it the active profile (current behavior quirk).
    home.cmd()
        .args(["mods", "profile-list"])
        .assert()
        .success()
        .stdout(predicate::eq("* dev\n"));

    // The mods dir is created by `add`.
    assert!(home.mods.exists());
}

#[test]
fn profile_add_defaults_side_to_both_and_show_uses_debug_format() {
    let home = TestHome::new();
    home.profile();

    // `side` defaults to `both` and is printed with Debug formatting (`Both`).
    home.cmd().args(["mods", "show"]).assert().success().stdout(
        predicate::str::contains("name: dev\n")
            .and(predicate::str::contains("mods_dir: "))
            .and(predicate::str::contains("mc_version: 1.20.1\n"))
            .and(predicate::str::contains("loader: fabric\n"))
            .and(predicate::str::contains("side: Both\n")),
    );
}

#[test]
fn profile_use_switches_active_and_list_marks_active_with_star() {
    let home = TestHome::new();
    home.profile();
    // Add a second profile; `add` makes it active.
    home.cmd()
        .args([
            "mods",
            "add",
            "prod",
            "--mods-dir",
            home.mods.to_str().unwrap(),
            "--mc-version",
            "1.20.1",
            "--loader",
            "fabric",
        ])
        .assert()
        .success()
        .stdout(predicate::eq("added profile prod\n"));

    // BTreeMap ordering: `dev` before `prod` alphabetically; prod active.
    home.cmd()
        .args(["mods", "profile-list"])
        .assert()
        .success()
        .stdout(predicate::eq("  dev\n* prod\n"));

    // Switch back to dev.
    home.cmd()
        .args(["mods", "use", "dev"])
        .assert()
        .success()
        .stdout(predicate::eq("active profile dev\n"));

    home.cmd()
        .args(["mods", "profile-list"])
        .assert()
        .success()
        .stdout(predicate::eq("* dev\n  prod\n"));
}

#[test]
fn profile_use_unknown_profile_errors() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "use", "nope"])
        .assert()
        .failure()
        .stderr(predicate::eq("Error: unknown profile nope\n"));
}

#[test]
fn profile_show_named_unknown_errors() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "show", "nope"])
        .assert()
        .failure()
        .stderr(predicate::eq("Error: unknown profile nope\n"));
}

#[test]
fn profile_list_empty_is_silent_success() {
    let home = TestHome::new();
    // No profiles: `list` prints nothing and exits 0.
    home.cmd()
        .args(["mods", "profile-list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

// ---------------------------------------------------------------------------
// No-active-profile failure path (pinned for Task 1 QA evidence)
// ---------------------------------------------------------------------------

#[test]
fn list_without_active_profile_errors_with_actionable_message() {
    let home = TestHome::new();
    home.cmd()
        .args(["mods", "list"])
        .assert()
        .failure()
        .stderr(predicate::eq(
            "Error: no active profile; run profile add or profile use\n",
        ));
}

#[test]
fn status_without_active_profile_errors() {
    let home = TestHome::new();
    home.cmd()
        .args(["mods", "status"])
        .assert()
        .failure()
        .stderr(predicate::eq(
            "Error: no active profile; run profile add or profile use\n",
        ));
}

#[test]
fn search_without_active_profile_errors() {
    let home = TestHome::new();
    home.cmd()
        .args(["mods", "search", "root"])
        .assert()
        .failure()
        .stderr(predicate::eq(
            "Error: no active profile; run profile add or profile use\n",
        ));
}

#[test]
fn info_cloud_without_active_profile_errors() {
    let home = TestHome::new();
    // `mods info rootmod` does not end with `.jar` and path does not exist, so it
    // takes the cloud branch and requires an active profile.
    home.cmd()
        .args(["mods", "info", "rootmod"])
        .assert()
        .failure()
        .stderr(predicate::eq(
            "Error: no active profile; run profile add or profile use\n",
        ));
}

// ---------------------------------------------------------------------------
// Search characterization (mock provider)
// ---------------------------------------------------------------------------

#[test]
fn search_groups_duplicate_candidates_by_logical_id() {
    let home = TestHome::new();
    home.profile();
    // rootmod has two candidates (mock/rootmod and modrinth/rootmod) that are
    // merged under one logical id. Beta/alpha artifacts are filtered out.
    home.cmd()
        .args(["mods", "search", "root"])
        .assert()
        .success()
        .stdout(predicate::eq(
            "rootmod - Root Mod\n  A root mod with required and optional dependencies\n  candidates: mock/rootmod, modrinth/rootmod\n",
        ));
}

#[test]
fn search_no_match_is_silent_success() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "search", "zzznomatch"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn search_matches_title_substring_case_insensitively() {
    let home = TestHome::new();
    home.profile();
    // "standalone" matches the title "Standalone" via case-insensitive contains.
    home.cmd()
        .args(["mods", "search", "stand"])
        .assert()
        .success()
        .stdout(predicate::eq(
            "standalone - Standalone\n  A standalone mod\n  candidates: mock/standalone\n",
        ));
}

// ---------------------------------------------------------------------------
// Cloud info characterization (mock provider)
// ---------------------------------------------------------------------------

#[test]
fn cloud_info_prints_selected_artifact_and_all_dependency_kinds() {
    let home = TestHome::new();
    home.profile();
    // `mods info` prints the selected stable artifact and surfaces required/optional
    // deps plus warnings for embedded/incompatible/unknown kinds (Debug format
    // for the dep kind in warnings).
    home.cmd()
        .args(["mods", "info", "rootmod"])
        .assert()
        .success()
        .stdout(predicate::eq(
            "rootmod - Root Mod\nA root mod with required and optional dependencies\ncandidates: mock/rootmod, modrinth/rootmod\nselected: rootmod-file 1.0.0\nrequired deps: depmod\noptional deps: optionalmod\nwarning: Embedded dependency embeddedlib not installed\nwarning: Incompatible dependency badmod not installed\nwarning: Unknown dependency mysterymod not installed\n",
        ));
}

#[test]
fn cloud_info_for_standalone_has_no_dep_lines() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "info", "standalone"])
        .assert()
        .success()
        .stdout(predicate::eq(
            "standalone - Standalone\nA standalone mod\ncandidates: mock/standalone\nselected: standalone-file 1.0.0\n",
        ));
}

#[test]
fn cloud_info_for_brokenmod_succeeds_without_download_url() {
    let home = TestHome::new();
    home.profile();
    // `mods info` only reads project metadata; the missing download URL does not
    // affect info (it would fail at install time).
    home.cmd()
        .args(["mods", "info", "brokenmod"])
        .assert()
        .success()
        .stdout(predicate::eq(
            "brokenmod - Broken Mod\nA mod with missing download URL\ncandidates: mock/brokenmod\nselected: brokenmod-file 1.0.0\n",
        ));
}

// ---------------------------------------------------------------------------
// Install characterization (mock provider)
// ---------------------------------------------------------------------------

#[test]
fn install_without_query_or_file_errors() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install"])
        .assert()
        .failure()
        .stderr(predicate::eq(
            "Error: install requires a query or --file <PATH>\n",
        ));
}

#[test]
fn install_search_query_not_found_errors() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "zzznotfound", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::eq(
            "Error: mod zzznotfound not found by search\n",
        ));
}

#[test]
fn install_file_missing_errors_on_read() {
    let home = TestHome::new();
    home.profile();
    let missing = home.root.path().join("nope.txt");
    home.cmd()
        .args([
            "mods",
            "install",
            "--file",
            missing.to_str().unwrap(),
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("read ").and(predicate::str::contains("nope.txt")));
}

#[test]
fn install_rootmod_emits_all_four_warning_kinds_in_order() {
    let home = TestHome::new();
    home.profile();
    // Plan order is BTreeMap key order: depmod (Auto) before rootmod (Manual).
    // Warnings are emitted in the dependency iteration order of rootmod's
    // artifact: optional, embedded, incompatible, unknown.
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success()
        .stdout(predicate::eq(
            "selected rootmod from search result rootmod\ninstall depmod 1.0.0 Auto\ninstall rootmod 1.0.0 Manual\nwarning: optional dependency optionalmod not installed\nwarning: embedded dependency embeddedlib not installed\nwarning: incompatible dependency badmod not installed\nwarning: unknown dependency mysterymod not installed\n",
        ));

    assert!(home.mods.join("rootmod-1.0.0.jar").exists());
    assert!(home.mods.join("depmod-1.0.0.jar").exists());
    // Optional dep is NOT installed.
    assert!(!home.mods.join("optionalmod-1.0.0.jar").exists());
    // Beta artifact is NOT selected.
    assert!(!home.mods.join("rootmod-2.0.0-beta.jar").exists());
    // Alpha artifact is NOT selected.
    assert!(!home.mods.join("rootmod-3.0.0-alpha.jar").exists());
}

#[test]
fn install_dry_run_prints_plan_without_writing_jars() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::eq(
            "selected rootmod from search result rootmod\ndry run\ninstall depmod 1.0.0 Auto\ninstall rootmod 1.0.0 Manual\nwarning: optional dependency optionalmod not installed\nwarning: embedded dependency embeddedlib not installed\nwarning: incompatible dependency badmod not installed\nwarning: unknown dependency mysterymod not installed\n",
        ));

    // Dry run writes nothing.
    assert!(!home.mods.join("rootmod-1.0.0.jar").exists());
    assert!(!home.mods.join("depmod-1.0.0.jar").exists());
}

#[test]
fn install_missing_download_url_errors_and_leaves_no_partial_jar() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "brokenmod", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("missing download URL"));

    // No partial jar is left behind.
    assert!(!home.mods.join("brokenmod-1.0.0.jar").exists());
    // Lock is not written (no lock file created).
    assert!(!home.state.join("dev.lock.json").exists());
}

#[test]
fn install_file_parses_comments_and_blank_lines_into_one_plan() {
    let home = TestHome::new();
    home.profile();
    let list = home.root.path().join("mods.txt");
    fs::write(&list, "# install roots\nrootmod\nstandalone\n\n").expect("write list");

    home.cmd()
        .args(["mods", "install", "--file", list.to_str().unwrap(), "--yes"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("selected rootmod from search result rootmod")
                .and(predicate::str::contains(
                    "selected standalone from search result standalone",
                ))
                .and(predicate::str::contains("install rootmod 1.0.0 Manual"))
                .and(predicate::str::contains("install standalone 1.0.0 Manual")),
        );

    assert!(home.mods.join("rootmod-1.0.0.jar").exists());
    assert!(home.mods.join("standalone-1.0.0.jar").exists());
}

// ---------------------------------------------------------------------------
// List characterization
// ---------------------------------------------------------------------------

#[test]
fn list_prints_installed_mods_alphabetically_with_reason_and_identity() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success();

    // Format: `{logical_id} {version} {reason:?} {provider}/{file_id}`
    // BTreeMap order: depmod before rootmod. reason is Debug-cased (Auto/Manual).
    home.cmd()
        .args(["mods", "list"])
        .assert()
        .success()
        .stdout(predicate::eq(
            "depmod 1.0.0 Auto mock/depmod-file\nrootmod 1.0.0 Manual mock/rootmod-file\n",
        ));
}

#[test]
fn list_empty_is_silent_success() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

// ---------------------------------------------------------------------------
// Status characterization
// ---------------------------------------------------------------------------

#[test]
fn status_reports_ok_for_intact_owned_jars() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success();

    home.cmd()
        .args(["mods", "status"])
        .assert()
        .success()
        .stdout(predicate::eq("ok: depmod\nok: rootmod\n"));
}

#[test]
fn status_reports_missing_and_changed_and_untracked_without_claiming_untracked() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success();
    // Mutate owned jars.
    fs::write(home.mods.join("rootmod-1.0.0.jar"), b"changed").expect("change jar");
    fs::remove_file(home.mods.join("depmod-1.0.0.jar")).expect("remove dep jar");
    // Add an untracked jar.
    fs::write(home.mods.join("untracked.jar"), b"keep me").expect("untracked jar");

    home.cmd()
        .args(["mods", "status"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("changed: rootmod (rootmod-1.0.0.jar)")
                .and(predicate::str::contains(
                    "missing: depmod (depmod-1.0.0.jar)",
                ))
                .and(predicate::str::contains("untracked: untracked.jar")),
        );

    // autoremove must not claim/delete the untracked jar.
    home.cmd()
        .args(["mods", "autoremove", "--yes"])
        .assert()
        .success();
    assert!(home.mods.join("untracked.jar").exists());
}

// ---------------------------------------------------------------------------
// Remove / uninstall characterization
// ---------------------------------------------------------------------------

#[test]
fn remove_requires_yes_flag() {
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
        .stderr(predicate::eq(
            "Error: confirmation required; pass --yes to apply\n",
        ));
}

#[test]
fn remove_auto_dependency_is_rejected_with_autoremove_hint() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success();

    home.cmd()
        .args(["mods", "remove", "depmod", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::eq(
            "Error: depmod is automatic; use autoremove when no roots require it\n",
        ));
}

#[test]
fn remove_unknown_mod_errors() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "remove", "nothing", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::eq("Error: nothing is not installed\n"));
}

#[test]
fn uninstall_is_alias_for_remove() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success();

    // `uninstall` behaves identically to `remove`.
    home.cmd()
        .args(["mods", "uninstall", "rootmod", "--yes"])
        .assert()
        .success()
        .stdout(predicate::eq("removed rootmod\n"));

    assert!(!home.mods.join("rootmod-1.0.0.jar").exists());
    // Auto dep remains until autoremove.
    assert!(home.mods.join("depmod-1.0.0.jar").exists());
}

#[test]
fn remove_manual_root_keeps_auto_required_dep_until_autoremove() {
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
        .stdout(predicate::eq("removed rootmod\n"));

    assert!(!home.mods.join("rootmod-1.0.0.jar").exists());
    assert!(home.mods.join("depmod-1.0.0.jar").exists());

    home.cmd()
        .args(["mods", "autoremove", "--yes"])
        .assert()
        .success()
        .stdout(predicate::eq("removed depmod\n"));

    assert!(!home.mods.join("depmod-1.0.0.jar").exists());
}

// ---------------------------------------------------------------------------
// Autoremove characterization
// ---------------------------------------------------------------------------

#[test]
fn autoremove_requires_yes_when_removable() {
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

    // depmod is now unreachable but autoremove without --yes refuses.
    home.cmd()
        .args(["mods", "autoremove"])
        .assert()
        .failure()
        .stderr(predicate::eq(
            "Error: confirmation required; pass --yes to apply\n",
        ));
}

#[test]
fn autoremove_nothing_to_do_is_silent_success() {
    let home = TestHome::new();
    home.profile();
    // Install standalone (no deps); nothing is removable.
    home.cmd()
        .args(["mods", "install", "standalone", "--yes"])
        .assert()
        .success();

    home.cmd()
        .args(["mods", "autoremove", "--yes"])
        .assert()
        .success()
        .stdout(predicate::eq("nothing to autoremove\n"));
}

#[test]
fn autoremove_keeps_required_dep_while_manual_root_still_needs_it() {
    let home = TestHome::new();
    home.profile();
    home.cmd()
        .args(["mods", "install", "rootmod", "--yes"])
        .assert()
        .success();

    // rootmod (manual) still requires depmod; autoremove must keep depmod.
    home.cmd()
        .args(["mods", "autoremove", "--yes"])
        .assert()
        .success()
        .stdout(predicate::eq("nothing to autoremove\n"));

    assert!(home.mods.join("rootmod-1.0.0.jar").exists());
    assert!(home.mods.join("depmod-1.0.0.jar").exists());
}

// ---------------------------------------------------------------------------
// Provider selection / dispatch characterization (no real network)
// ---------------------------------------------------------------------------

#[test]
fn provider_mock_works_offline_for_search() {
    let home = TestHome::new();
    home.profile();
    // Explicitly mock; no network is contacted.
    home.cmd()
        .args(["mods", "search", "root"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rootmod"));
}

#[test]
fn provider_curseforge_requires_api_key_env() {
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
    .stderr(predicate::eq(
        "Error: CurseForge provider requires CURSEFORGE_API_KEY\n",
    ));
}

#[test]
fn provider_curseforge_info_also_requires_api_key() {
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
        "info",
        "rootmod",
    ])
    .env_remove("CURSEFORGE_API_KEY")
    .assert()
    .failure()
    .stderr(predicate::eq(
        "Error: CurseForge provider requires CURSEFORGE_API_KEY\n",
    ));
}

// ---------------------------------------------------------------------------
// Local jar info characterization (fabric.mod.json / mods.toml / mcmod.info /
// no-metadata fallback with SHA-256)
// ---------------------------------------------------------------------------

/// Compute a CRC-32 of `data` (ISO 3309 / zlib polynomial), used by the
/// hand-rolled stored-zip builder below. The `zip` crate is a private
/// dependency of mcm and not a dev-dependency, so integration tests build
/// minimal valid jar archives byte-for-byte without it.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc ^ 0xFFFF_FFFF
}

/// Build a minimal valid ZIP archive (no compression, method = stored) holding
/// the given named entries. The result is readable by `zip::ZipArchive` (which
/// mcm's `local_jar_info` uses) and by standard unzip tooling.
fn build_stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let mut offset: u32 = 0;
    for (name, data) in entries {
        let crc = crc32(data);
        let name_bytes = name.as_bytes();
        let name_len = u16::try_from(name_bytes.len()).unwrap();
        let data_len = u32::try_from(data.len()).unwrap();

        // Local file header (30 bytes + name).
        buf.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]); // signature
        buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
        buf.extend_from_slice(&0u16.to_le_bytes()); // flags
        buf.extend_from_slice(&0u16.to_le_bytes()); // method = stored
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
        buf.extend_from_slice(&0u16.to_le_bytes()); // mod date
        buf.extend_from_slice(&crc.to_le_bytes());
        buf.extend_from_slice(&data_len.to_le_bytes()); // compressed size
        buf.extend_from_slice(&data_len.to_le_bytes()); // uncompressed size
        buf.extend_from_slice(&name_len.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // extra len
        buf.extend_from_slice(name_bytes);
        buf.extend_from_slice(data);

        // Central directory header (46 bytes + name).
        central.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]); // signature
        central.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central.extend_from_slice(&0u16.to_le_bytes()); // flags
        central.extend_from_slice(&0u16.to_le_bytes()); // method
        central.extend_from_slice(&0u16.to_le_bytes()); // mod time
        central.extend_from_slice(&0u16.to_le_bytes()); // mod date
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&data_len.to_le_bytes());
        central.extend_from_slice(&data_len.to_le_bytes());
        central.extend_from_slice(&name_len.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra len
        central.extend_from_slice(&0u16.to_le_bytes()); // comment len
        central.extend_from_slice(&0u16.to_le_bytes()); // disk number
        central.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name_bytes);

        offset += 30 + u32::from(name_len) + data_len;
    }
    let cd_offset = u32::try_from(buf.len()).unwrap();
    buf.extend_from_slice(&central);
    let cd_size = u32::try_from(central.len()).unwrap();
    let entry_count = u16::try_from(entries.len()).unwrap();

    // End of central directory record (22 bytes).
    buf.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]); // signature
    buf.extend_from_slice(&0u16.to_le_bytes()); // disk number
    buf.extend_from_slice(&0u16.to_le_bytes()); // disk with CD
    buf.extend_from_slice(&entry_count.to_le_bytes()); // entries on this disk
    buf.extend_from_slice(&entry_count.to_le_bytes()); // total entries
    buf.extend_from_slice(&cd_size.to_le_bytes());
    buf.extend_from_slice(&cd_offset.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes()); // comment len
    buf
}

#[test]
fn local_jar_info_reads_fabric_mod_json_id_and_version() {
    let home = TestHome::new();
    let jar = home.root.path().join("fabric.jar");
    let bytes = build_stored_zip(&[(
        "fabric.mod.json",
        br#"{"id":"fabricmod","version":"1.2.3","name":"Fabric Mod"}"#,
    )]);
    fs::write(&jar, &bytes).expect("write jar");

    home.cmd()
        .args(["mods", "info", jar.to_str().unwrap()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("local jar: ")
                .and(predicate::str::contains("sha256: "))
                .and(predicate::str::contains("size: 184"))
                .and(predicate::str::contains("metadata: fabric.mod.json"))
                .and(predicate::str::contains("id: fabricmod"))
                .and(predicate::str::contains("version: 1.2.3"))
                .and(predicate::str::contains("provider:").not()),
        );
}

#[test]
fn local_jar_info_reads_mods_toml_modid_and_version_lines() {
    let home = TestHome::new();
    let jar = home.root.path().join("toml.jar");
    let toml_content: &[u8] = concat!(
        "modId=\"tomlmod\"\n",
        "version=\"2.0.0\"\n",
        "displayName=\"Toml Mod\"\n",
    )
    .as_bytes();
    let bytes = build_stored_zip(&[("META-INF/mods.toml", toml_content)]);
    fs::write(&jar, &bytes).expect("write jar");

    home.cmd()
        .args(["mods", "info", jar.to_str().unwrap()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("metadata: mods.toml")
                .and(predicate::str::contains("modId=\"tomlmod\""))
                .and(predicate::str::contains("version=\"2.0.0\"")),
        );
}

#[test]
fn local_jar_info_reads_mcmod_info_array_fields() {
    let home = TestHome::new();
    let jar = home.root.path().join("mcmod.jar");
    let bytes = build_stored_zip(&[(
        "mcmod.info",
        br#"[{"modid":"legacy","name":"Legacy","version":"3.0.0"}]"#,
    )]);
    fs::write(&jar, &bytes).expect("write jar");

    home.cmd()
        .args(["mods", "info", jar.to_str().unwrap()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("metadata: mcmod.info")
                .and(predicate::str::contains("id: legacy"))
                .and(predicate::str::contains("version: 3.0.0"))
                .and(predicate::str::contains("name: Legacy")),
        );
}

#[test]
fn local_jar_info_falls_back_to_hash_when_metadata_unavailable() {
    let home = TestHome::new();
    let jar = home.root.path().join("plain.jar");
    let content = b"not really a zip but useful bytes";
    fs::write(&jar, content).expect("write jar");

    // The exact SHA-256 of the file content is printed (pinned).
    let expected_hash = sha256_hex(content);
    home.cmd()
        .args(["mods", "info", jar.to_str().unwrap()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("local jar: ")
                .and(predicate::str::contains(format!("sha256: {expected_hash}")))
                .and(predicate::str::contains("size: 33"))
                .and(predicate::str::contains("metadata: unavailable"))
                .and(predicate::str::contains("provider:").not()),
        );
}

#[test]
fn local_jar_info_zip_without_known_metadata_reports_unavailable() {
    let home = TestHome::new();
    let jar = home.root.path().join("nometa.jar");
    let bytes = build_stored_zip(&[("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\n")]);
    fs::write(&jar, &bytes).expect("write jar");

    home.cmd()
        .args(["mods", "info", jar.to_str().unwrap()])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("local jar: ")
                .and(predicate::str::contains("sha256: "))
                .and(predicate::str::contains("metadata: unavailable")),
        );
}

#[test]
fn local_jar_info_nonexistent_jar_path_errors_on_read() {
    let home = TestHome::new();
    let missing = home.root.path().join("ghost.jar");
    // Ends with `.jar`, so `mods info` takes the local-jar branch even though the
    // path does not exist; `local_jar_info` then fails reading the file.
    home.cmd()
        .args(["mods", "info", missing.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("read ").and(predicate::str::contains("ghost.jar")));
}

// ---------------------------------------------------------------------------
// Local helper: SHA-256 hex (mirrors mcm's internal `sha256_hex`).
// ---------------------------------------------------------------------------

fn sha256_hex(bytes: &[u8]) -> String {
    // Minimal SHA-256 implementation to avoid pulling in a dev-dependency.
    // Deterministic and standard; used only to pin the fallback-hash output.
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    let mut padded = bytes.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter()
        .flat_map(|w| w.to_be_bytes())
        .collect::<Vec<_>>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_lists_all_top_level_commands_and_description() {
    let required_help = predicate::str::contains("Like a Linux package manager for Minecraft mods")
        .and(predicate::str::contains("install"))
        .and(predicate::str::contains("upgrade"))
        .and(predicate::str::contains("full-upgrade"))
        .and(predicate::str::contains("source"))
        .and(predicate::str::contains("pkg"))
        .and(predicate::str::contains("game"))
        .and(predicate::str::contains("do"))
        .and(predicate::str::contains("run"))
        .and(predicate::str::contains("config"))
        .and(predicate::str::contains("mods"));

    Command::cargo_bin("mcm")
        .expect("mcm binary should be built by cargo")
        .arg("--help")
        .assert()
        .success()
        .stdout(required_help);
}

#[test]
fn help_does_not_list_old_top_level_mod_manager_commands() {
    Command::cargo_bin("mcm")
        .expect("mcm binary should be built by cargo")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("profile").not());
}

#[test]
fn mods_install_help_lists_file_option_for_mod_list() {
    let required_help = predicate::str::contains("--file")
        .and(predicate::str::contains("-f"))
        .and(predicate::str::contains("<PATH>"))
        .and(predicate::str::contains("mod list file"));

    Command::cargo_bin("mcm")
        .expect("mcm binary should be built by cargo")
        .args(["mods", "install", "--help"])
        .assert()
        .success()
        .stdout(required_help);
}

#[test]
fn mod_is_alias_for_mods() {
    Command::cargo_bin("mcm")
        .expect("mcm binary should be built by cargo")
        .args(["mod", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Mod-manager command group"));
}

#[test]
fn pkg_dl_is_alias_for_download() {
    Command::cargo_bin("mcm")
        .expect("mcm binary should be built by cargo")
        .args(["pkg", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("download")
                .and(predicate::str::contains("dl"))
                .and(predicate::str::contains("Alias for")),
        );
}

#[test]
fn game_install_help_describes_smart_targets() {
    Command::cargo_bin("mcm")
        .expect("mcm binary should be built by cargo")
        .args(["game", "install", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("mc1.21.1-neoforge-21.1.172")
                .and(predicate::str::contains("fabric/forge/quilt")),
        );
}

#[test]
fn top_level_install_help_accepts_only_mcm_target_and_yes() {
    Command::cargo_bin("mcm")
        .expect("mcm binary should be built by cargo")
        .args(["install", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("[TARGET]")
                .and(predicate::str::contains("-y"))
                .and(predicate::str::contains("--yes")),
        );
}

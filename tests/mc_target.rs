//! Integration tests for the `game install` smart target parser.
//!
//! These tests exercise the public `parse_mc_target` API through the binary
//! surface (`mcm game install <name> <target>`) and also call the library
//! function directly for exhaustive grammar coverage.

use assert_cmd::Command;
use predicates::prelude::*;

use mcm::{parse_mc_target, Loader, McTarget};

// ---------------------------------------------------------------------------
// Direct parser unit tests (library surface)
// ---------------------------------------------------------------------------

#[test]
fn parser_mc_alone_means_latest_vanilla() {
    assert_eq!(
        parse_mc_target("mc").unwrap(),
        McTarget::Vanilla { mc_version: None }
    );
}

#[test]
fn parser_mc_with_version_means_specific_vanilla() {
    assert_eq!(
        parse_mc_target("mc1.21.1").unwrap(),
        McTarget::Vanilla {
            mc_version: Some("1.21.1".into())
        }
    );
}

#[test]
fn parser_mc_neoforge_means_latest_mc_latest_neoforge() {
    assert_eq!(
        parse_mc_target("mc-neoforge").unwrap(),
        McTarget::WithLoader {
            mc_version: None,
            loader: Loader::NeoForge,
            loader_version: None,
        }
    );
}

#[test]
fn parser_mc_version_neoforge_means_specific_mc_latest_neoforge() {
    assert_eq!(
        parse_mc_target("mc1.21.1-neoforge").unwrap(),
        McTarget::WithLoader {
            mc_version: Some("1.21.1".into()),
            loader: Loader::NeoForge,
            loader_version: None,
        }
    );
}

#[test]
fn parser_mc_version_neoforge_version_means_exact() {
    assert_eq!(
        parse_mc_target("mc1.21.1-neoforge-21.1.172").unwrap(),
        McTarget::WithLoader {
            mc_version: Some("1.21.1".into()),
            loader: Loader::NeoForge,
            loader_version: Some("21.1.172".into()),
        }
    );
}

#[test]
fn parser_fabric_grammar_matches_neoforge_grammar() {
    assert_eq!(
        parse_mc_target("mc-fabric").unwrap(),
        McTarget::WithLoader {
            mc_version: None,
            loader: Loader::Fabric,
            loader_version: None,
        }
    );
    assert_eq!(
        parse_mc_target("mc1.20.1-fabric-0.16.0").unwrap(),
        McTarget::WithLoader {
            mc_version: Some("1.20.1".into()),
            loader: Loader::Fabric,
            loader_version: Some("0.16.0".into()),
        }
    );
}

#[test]
fn parser_forge_grammar_matches() {
    assert_eq!(
        parse_mc_target("mc1.20.1-forge-47.3.0").unwrap(),
        McTarget::WithLoader {
            mc_version: Some("1.20.1".into()),
            loader: Loader::Forge,
            loader_version: Some("47.3.0".into()),
        }
    );
}

#[test]
fn parser_quilt_grammar_matches() {
    assert_eq!(
        parse_mc_target("mc1.20.1-quilt-0.26.0").unwrap(),
        McTarget::WithLoader {
            mc_version: Some("1.20.1".into()),
            loader: Loader::Quilt,
            loader_version: Some("0.26.0".into()),
        }
    );
}

#[test]
fn parser_rejects_at_latest_suffix() {
    assert!(parse_mc_target("mc1.21.1-neoforge@latest").is_err());
    assert!(parse_mc_target("mc@latest").is_err());
    assert!(parse_mc_target("mc-neoforge@latest").is_err());
}

#[test]
fn parser_rejects_non_mc_prefix() {
    assert!(parse_mc_target("sodium").is_err());
    assert!(parse_mc_target("1.21.1").is_err());
    assert!(parse_mc_target("fabric").is_err());
}

#[test]
fn parser_rejects_unknown_loader() {
    assert!(parse_mc_target("mc-badloader").is_err());
    assert!(parse_mc_target("mc1.21.1-risugamis").is_err());
}

#[test]
fn parser_loader_case_insensitive() {
    assert_eq!(
        parse_mc_target("mc-NEOFORGE").unwrap(),
        McTarget::WithLoader {
            mc_version: None,
            loader: Loader::NeoForge,
            loader_version: None,
        }
    );
    assert_eq!(
        parse_mc_target("mc-Fabric").unwrap(),
        McTarget::WithLoader {
            mc_version: None,
            loader: Loader::Fabric,
            loader_version: None,
        }
    );
}

// ---------------------------------------------------------------------------
// CLI surface tests: `mcm game install` validates the target via the parser
// ---------------------------------------------------------------------------

#[test]
fn game_install_valid_target_requires_confirmation() {
    Command::cargo_bin("mcm")
        .expect("mcm binary should be built by cargo")
        .args(["game", "install", "dev", "mc1.21.1-neoforge-21.1.172"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "confirmation required; pass --yes",
        ));
}

#[test]
fn game_install_invalid_target_errors_before_stub() {
    Command::cargo_bin("mcm")
        .expect("mcm binary should be built by cargo")
        .args(["game", "install", "dev", "mc1.21.1-neoforge@latest"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("@latest"));
}

#[test]
fn top_level_install_rejects_mc_smart_target() {
    Command::cargo_bin("mcm")
        .expect("mcm binary should be built by cargo")
        .args(["install", "mc-neoforge"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("game install"));
}

#[test]
fn top_level_install_rejects_raw_mod_name() {
    Command::cargo_bin("mcm")
        .expect("mcm binary should be built by cargo")
        .args(["install", "sodium"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("mods install"));
}

#[test]
fn top_level_install_rejects_extra_option() {
    Command::cargo_bin("mcm")
        .expect("mcm binary should be built by cargo")
        .args(["install", "sample.mcm", "--extra"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--extra").or(predicate::str::contains("unexpected")));
}

//! Centralized trusted-source confirmation policy.
//!
//! Imported sources are trusted, but actionable operations require
//! confirmation. This module owns the single classification table and the
//! interactive/non-interactive prompts — callers must not scatter ad-hoc
//! `bail!("confirmation required; ...")` prompts elsewhere.
//!
//! Policy summary:
//! - `Harmless` — read-only / list / info / dry-run / help. Never prompts.
//! - `Bypassable` — install / download / delete / remove / autoremove / game
//!   remove / package install / runtime install / source action / script
//!   execution / launch-on-install. Skips with `--yes`. In a TTY, prompts
//!   interactively (typed "yes" for MC-critical ops, `[y/N]` otherwise). In a
//!   non-TTY without `--yes`, bails with a `confirmation required` error.
//! - `NonBypassable` — root/system changes. Always requires typed confirmation
//!   even with `--yes` (the `--yes` flag only suppresses the *bypassable*
//!   gate; root actions still ask). In a non-TTY, bails.
//!
//! MC-critical operations (`Autoremove`, `WorldOverwrite`, `WorldDelete`) emit
//! [`AUTOREMOVE_WARNING`] (or a world-specific variant) to stderr before any
//! confirmation prompt so the user understands worlds/saves may break.

use std::io::{self, IsTerminal, Write};

use anyhow::{bail, Result};

use crate::i18n::{self, Lang};

/// Confirmation policy for an operation kind.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfirmationPolicy {
    /// No confirmation needed (read-only / list / info / dry-run / help).
    Harmless,
    /// Confirmation required, but `--yes` skips it. Interactive prompt in a
    /// TTY; bail in a non-TTY.
    Bypassable,
    /// Always requires typed confirmation, even with `--yes`. Bail in a
    /// non-TTY (no way to type the confirmation).
    NonBypassable,
}

/// Kind of actionable operation, classified by [`classify`] into a
/// [`ConfirmationPolicy`].
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OperationKind {
    Install,
    Download,
    Delete,
    VersionRemoval,
    PackageInstall,
    RuntimeInstall,
    SourceAction,
    ScriptExecution,
    RootSystemChange,
    WorldOverwrite,
    WorldDelete,
    Autoremove,
    LaunchOnInstall,
    GameRemove,
    Upgrade,
}

/// Classify an operation into its confirmation policy.
pub(crate) fn classify(op: OperationKind) -> ConfirmationPolicy {
    match op {
        // Root/system changes are never bypassable — always typed-confirm.
        OperationKind::RootSystemChange => ConfirmationPolicy::NonBypassable,
        // Everything else actionable is bypassable with --yes.
        OperationKind::Install
        | OperationKind::Download
        | OperationKind::Delete
        | OperationKind::VersionRemoval
        | OperationKind::PackageInstall
        | OperationKind::RuntimeInstall
        | OperationKind::SourceAction
        | OperationKind::ScriptExecution
        | OperationKind::WorldOverwrite
        | OperationKind::WorldDelete
        | OperationKind::Autoremove
        | OperationKind::LaunchOnInstall
        | OperationKind::GameRemove
        | OperationKind::Upgrade => ConfirmationPolicy::Bypassable,
    }
}

/// True for MC-critical operations that can break worlds/saves/modded
/// structures. These emit a warning to stderr and use typed confirmation
/// (require "yes", not just "y") in interactive mode.
pub(crate) fn is_mc_critical(op: OperationKind) -> bool {
    matches!(
        op,
        OperationKind::Autoremove | OperationKind::WorldOverwrite | OperationKind::WorldDelete
    )
}

/// Emit the MC-critical warning for `op` to stderr, if `op` is MC-critical.
/// Callers should invoke this *after* the bypassable `--yes` gate passes but
/// *before* any destructive action, so the warning only appears when the
/// operation is actually proceeding.
pub(crate) fn emit_mc_critical_warning(op: OperationKind) {
    let lang = Lang::default();
    let msg = match op {
        OperationKind::Autoremove => Some(i18n::autoremove_warning(lang)),
        OperationKind::WorldOverwrite => Some(i18n::world_overwrite_warning(lang)),
        OperationKind::WorldDelete => Some(i18n::world_delete_warning(lang)),
        _ => None,
    };
    if let Some(m) = msg {
        eprintln!("{m}");
    }
}

/// Require confirmation for `op`. Returns `Ok(())` if bypassed (`--yes`) or
/// confirmed interactively; returns `Err` if the user declined or if a
/// non-bypassable operation ran in a non-TTY.
///
/// For `Bypassable` ops: `--yes` skips; otherwise TTY prompts (typed for
/// MC-critical, `[y/N]` for others); non-TTY bails.
/// For `NonBypassable` ops: TTY requires typed "yes" (even with `--yes`);
/// non-TTY bails.
/// For `Harmless` ops: always `Ok`.
#[allow(dead_code)]
pub(crate) fn require_confirmation(op: OperationKind, yes: bool) -> Result<()> {
    let lang = Lang::default();
    match classify(op) {
        ConfirmationPolicy::Harmless => Ok(()),
        ConfirmationPolicy::Bypassable => {
            if yes {
                return Ok(());
            }
            if io::stdin().is_terminal() {
                if is_mc_critical(op) {
                    emit_mc_critical_warning(op);
                    if confirm_typed(&typed_prompt(op, lang))? {
                        Ok(())
                    } else {
                        bail!("{}", i18n::confirmation_declined(lang));
                    }
                } else {
                    let confirmed = prompt_yes_no(&simple_prompt(op, lang))?;
                    if confirmed {
                        Ok(())
                    } else {
                        bail!("{}", i18n::confirmation_declined(lang));
                    }
                }
            } else {
                bail!("{}", i18n::confirmation_required_non_tty(lang));
            }
        }
        ConfirmationPolicy::NonBypassable => {
            if io::stdin().is_terminal() {
                emit_mc_critical_warning(op);
                if confirm_typed(&typed_prompt(op, lang))? {
                    Ok(())
                } else {
                    bail!("{}", i18n::confirmation_declined(lang));
                }
            } else {
                bail!("{}", i18n::confirmation_required_non_bypassable(lang));
            }
        }
    }
}

/// Interactive typed confirmation: prints `prompt` to stderr, reads a line
/// from stdin, returns `true` only if the user typed "yes" (case-insensitive,
/// trimmed). Anything else returns `false`.
#[allow(dead_code)]
pub(crate) fn confirm_typed(prompt: &str) -> Result<bool> {
    eprint!("{prompt} ");
    io::stderr().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("yes"))
}

/// Root-escalation helper. In interactive mode, offers elevation (prints the
/// command it would run). In non-interactive mode, prints the exact
/// `sudo`/`pkexec` command to stderr and bails with a nonzero error.
///
/// `action` is the full command string the user should run with elevation
/// (e.g. `"mcm game install mc1.21.1"``). On Linux/macOS the helper suggests
/// `sudo`; on other platforms it suggests `pkexec`.
#[allow(dead_code)]
pub(crate) fn root_escalation_helper(action: &str, interactive: bool) -> Result<()> {
    let lang = Lang::default();
    let elev = if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
        format!("sudo {action}")
    } else {
        format!("pkexec {action}")
    };
    if interactive && io::stdin().is_terminal() {
        eprintln!("{}", i18n::root_privileges_required_for(lang, action));
        eprintln!("{}", i18n::rerun_with(lang, &elev));
        bail!("{}", i18n::root_privileges_required(lang));
    } else {
        eprintln!("{}", i18n::root_privileges_required_action(lang));
        eprintln!("{}", i18n::rerun_with(lang, &elev));
        bail!("{}", i18n::root_privileges_required_pass_yes(lang));
    }
}

// --- private prompt helpers ----------------------------------------------

#[allow(dead_code)]
fn simple_prompt(op: OperationKind, lang: Lang) -> String {
    match op {
        OperationKind::Install => i18n::proceed_with_install(lang).to_owned(),
        OperationKind::Download => i18n::proceed_with_download(lang).to_owned(),
        OperationKind::Delete | OperationKind::GameRemove => {
            i18n::proceed_with_removal(lang).to_owned()
        }
        OperationKind::VersionRemoval => i18n::proceed_with_version_removal(lang).to_owned(),
        OperationKind::PackageInstall => i18n::proceed_with_package_install(lang).to_owned(),
        OperationKind::RuntimeInstall => i18n::proceed_with_runtime_install(lang).to_owned(),
        OperationKind::SourceAction => i18n::proceed_with_source_action(lang).to_owned(),
        OperationKind::ScriptExecution => i18n::proceed_with_script_execution(lang).to_owned(),
        OperationKind::LaunchOnInstall => i18n::proceed_with_launch_on_install(lang).to_owned(),
        _ => i18n::proceed(lang).to_owned(),
    }
}

#[allow(dead_code)]
fn typed_prompt(op: OperationKind, lang: Lang) -> String {
    match op {
        OperationKind::Autoremove => i18n::autoremove_typed_prompt(lang).to_owned(),
        OperationKind::WorldOverwrite => i18n::world_overwrite_typed_prompt(lang).to_owned(),
        OperationKind::WorldDelete => i18n::world_delete_typed_prompt(lang).to_owned(),
        OperationKind::RootSystemChange => i18n::root_system_typed_prompt(lang).to_owned(),
        _ => i18n::default_typed_prompt(lang).to_owned(),
    }
}

/// Read a line from stdin, return `true` for y/Y/yes/YES/Yes (trimmed).
/// Used by `Bypassable` non-MC-critical interactive prompts.
pub(crate) fn prompt_yes_no(prompt: &str) -> Result<bool> {
    print!("{prompt} ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(matches!(input.trim(), "y" | "Y" | "yes" | "YES" | "Yes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_harmless_ops_dont_exist_in_enum() {
        // Harmless ops (read-only/list/info/dry-run/help) do not appear in
        // OperationKind because they never reach require_confirmation. This
        // test documents that every OperationKind variant is actionable.
        let all_actionable = [
            OperationKind::Install,
            OperationKind::Download,
            OperationKind::Delete,
            OperationKind::VersionRemoval,
            OperationKind::PackageInstall,
            OperationKind::RuntimeInstall,
            OperationKind::SourceAction,
            OperationKind::ScriptExecution,
            OperationKind::RootSystemChange,
            OperationKind::WorldOverwrite,
            OperationKind::WorldDelete,
            OperationKind::Autoremove,
            OperationKind::LaunchOnInstall,
            OperationKind::GameRemove,
        ];
        for op in all_actionable {
            assert_ne!(
                classify(op),
                ConfirmationPolicy::Harmless,
                "{op:?} should not be Harmless"
            );
        }
    }

    #[test]
    fn classify_root_system_change_is_non_bypassable() {
        assert_eq!(
            classify(OperationKind::RootSystemChange),
            ConfirmationPolicy::NonBypassable
        );
    }

    #[test]
    fn classify_actionable_ops_are_bypassable() {
        let bypassable = [
            OperationKind::Install,
            OperationKind::Download,
            OperationKind::Delete,
            OperationKind::VersionRemoval,
            OperationKind::PackageInstall,
            OperationKind::RuntimeInstall,
            OperationKind::SourceAction,
            OperationKind::ScriptExecution,
            OperationKind::WorldOverwrite,
            OperationKind::WorldDelete,
            OperationKind::Autoremove,
            OperationKind::LaunchOnInstall,
            OperationKind::GameRemove,
        ];
        for op in bypassable {
            assert_eq!(
                classify(op),
                ConfirmationPolicy::Bypassable,
                "{op:?} should be Bypassable"
            );
        }
    }

    #[test]
    fn is_mc_critical_flags_autoremove_world_ops() {
        assert!(is_mc_critical(OperationKind::Autoremove));
        assert!(is_mc_critical(OperationKind::WorldOverwrite));
        assert!(is_mc_critical(OperationKind::WorldDelete));
        assert!(!is_mc_critical(OperationKind::Install));
        assert!(!is_mc_critical(OperationKind::Delete));
        assert!(!is_mc_critical(OperationKind::RootSystemChange));
    }

    #[test]
    fn autoremove_warning_contains_required_phrases() {
        let warning = i18n::autoremove_warning(i18n::Lang::En);
        assert!(warning.contains("MC-critical"));
        assert!(warning.contains("break worlds/saves"));
        assert!(warning.contains("modded structures"));
    }

    #[test]
    fn require_confirmation_bypassable_with_yes_succeeds_without_tty() {
        // --yes bypasses even in non-TTY.
        assert!(require_confirmation(OperationKind::Install, true).is_ok());
        assert!(require_confirmation(OperationKind::Autoremove, true).is_ok());
        assert!(require_confirmation(OperationKind::GameRemove, true).is_ok());
    }

    #[test]
    fn require_confirmation_bypassable_without_yes_bails_in_non_tty() {
        // No TTY in test runner → bail.
        let err = require_confirmation(OperationKind::Install, false).unwrap_err();
        assert!(
            err.to_string()
                .contains("confirmation required; pass --yes to proceed"),
            "got: {err}"
        );
    }

    #[test]
    fn require_confirmation_non_bypassable_bails_in_non_tty_even_with_yes() {
        let err = require_confirmation(OperationKind::RootSystemChange, true).unwrap_err();
        assert!(err.to_string().contains("non-bypassable"));
    }

    #[test]
    fn root_escalation_helper_non_interactive_bails_with_sudo_command() {
        let err = root_escalation_helper("mcm game install mc1.21.1", false).unwrap_err();
        let msg = err.to_string();
        // Non-interactive still prints the suggestion to stderr before bailing;
        // the bail message itself mentions root privileges.
        assert!(msg.contains("root privileges required"));
    }
}

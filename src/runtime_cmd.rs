//! `game runtime` command group implementations on [`App`].
//!
//! Provides `runtime_info` (discover and show Java status for a game) and
//! `runtime_install` (download a managed Java runtime through the retry
//! engine). System-wide install (`--system`) is handled as a stub that prints
//! the root escalation command via `root_escalation_helper`.

use anyhow::{bail, Context, Result};

use crate::app::App;
use crate::cli::RuntimeCommand;
use crate::confirmation::{require_confirmation, root_escalation_helper, OperationKind};
use crate::i18n;
use crate::runtime::{discover_java, install_managed_java, DiscoveryResult, JavaMajor};

impl App {
    pub(crate) fn game_runtime(&self, command: RuntimeCommand) -> Result<()> {
        match command {
            RuntimeCommand::Info { name } => self.runtime_info(&name),
            RuntimeCommand::Install { name, yes, system } => {
                self.runtime_install(&name, yes, system)
            }
        }
    }

    fn runtime_info(&self, name: &str) -> Result<()> {
        let config = self.load_config()?;
        let game = config
            .games
            .get(name)
            .with_context(|| i18n::unknown_game(self.lang, name))?;

        let mc_version = game.mc_version.as_deref().unwrap_or("(unset)");
        println!("game: {name}");
        println!("mc_version: {mc_version}");

        let required = match &game.mc_version {
            Some(v) => JavaMajor::from_mc_version(v)
                .with_context(|| i18n::unknown_mc_version_for_java(self.lang, v))?,
            None => {
                println!("{}", i18n::java_required_unknown(self.lang));
                return Ok(());
            }
        };
        println!(
            "{}",
            i18n::java_required(self.lang, required.display_version())
        );

        let global_root = &config.global.root_dir;
        match discover_java(game, global_root) {
            Ok(DiscoveryResult::Found(runtime)) => {
                println!("{}", i18n::status_found(self.lang));
                println!(
                    "{}",
                    i18n::java_version(self.lang, runtime.major.display_version())
                );
                println!(
                    "{}",
                    i18n::java_path(self.lang, &runtime.path.display().to_string())
                );
                println!(
                    "{}",
                    i18n::java_source(self.lang, &describe_source(&runtime.source, self.lang))
                );
            }
            Ok(DiscoveryResult::InstallPlan {
                required: _,
                managed_path,
            }) => {
                println!("{}", i18n::status_not_found(self.lang));
                println!(
                    "{}",
                    i18n::install_plan(self.lang, &managed_path.display().to_string())
                );
                println!("{}", i18n::run_install_command(self.lang, name));
            }
            Err(e) => {
                println!("{}", i18n::status_error(self.lang, &e.to_string()));
            }
        }

        Ok(())
    }

    fn runtime_install(&self, name: &str, yes: bool, system: bool) -> Result<()> {
        let config = self.load_config()?;
        let game = config
            .games
            .get(name)
            .with_context(|| i18n::unknown_game(self.lang, name))?;

        let global_root = &config.global.root_dir;

        if system {
            let action = format!("mcm game runtime install {name} --yes");
            let _ = root_escalation_helper(&action, false);
            bail!("{}", i18n::system_java_not_implemented(self.lang));
        }

        let result = discover_java(game, global_root)?;

        let (required, managed_path) = match result {
            DiscoveryResult::Found(_) => {
                println!("{}", i18n::java_runtime_already_available(self.lang, name));
                return Ok(());
            }
            DiscoveryResult::InstallPlan {
                required,
                managed_path,
            } => (required, managed_path),
        };

        require_confirmation(OperationKind::RuntimeInstall, yes)?;

        println!(
            "{}",
            i18n::installing_managed_java(self.lang, required.display_version(), name)
        );

        std::fs::create_dir_all(&managed_path).with_context(|| {
            i18n::create_dir_error(self.lang, &managed_path.display().to_string())
        })?;

        let java_path = install_managed_java(&managed_path, required)
            .with_context(|| format!("install managed Java {}", required.display_version()))?;

        println!(
            "{}",
            i18n::installed_managed_java(self.lang, required.display_version())
        );
        println!("  path: {}", java_path.display());

        Ok(())
    }
}

fn describe_source(source: &crate::runtime::JavaSource, lang: crate::i18n::Lang) -> String {
    match source {
        crate::runtime::JavaSource::UserConfig(p) => {
            i18n::user_config_source(lang, &p.display().to_string())
        }
        crate::runtime::JavaSource::Managed(p) => {
            i18n::managed_source(lang, &p.display().to_string())
        }
        crate::runtime::JavaSource::System => i18n::system_path_source(lang).to_owned(),
    }
}

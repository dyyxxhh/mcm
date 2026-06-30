//! `game` command group implementations on [`App`].
//!
//! All subcommands except `install` and `remove` are implemented here.
//! `game install` and `game remove` live in [`crate::game_install`].

use anyhow::{bail, Context, Result};

use crate::app::App;
use crate::cli::{GameCommand, GameConfigSubcommand};
use crate::i18n;

impl App {
    pub(crate) fn game(&self, command: GameCommand) -> Result<()> {
        match command {
            GameCommand::Default { name } => self.game_default(name),
            GameCommand::Install {
                name,
                target,
                dry_run,
                yes,
            } => self.game_install(&name, &target, dry_run, yes),
            GameCommand::Remove { name, yes } => self.game_remove(&name, yes),
            GameCommand::Info { name } => self.game_info(&name),
            GameCommand::Rename { old, new } => self.game_rename(&old, &new),
            GameCommand::Config { name, command } => match command {
                Some(GameConfigSubcommand::Set { key, value }) => {
                    self.game_config_set(&name, &key, &value)
                }
                Some(GameConfigSubcommand::Show) | None => self.game_config_show(&name),
            },
            GameCommand::Runtime { command } => self.game_runtime(command),
            GameCommand::List => self.game_list(),
        }
    }

    fn game_default(&self, name: Option<String>) -> Result<()> {
        let mut config = self.load_config()?;
        match name {
            None => match &config.default_game {
                Some(g) => println!("{g}"),
                None => println!("{}", i18n::no_default_game(self.lang)),
            },
            Some(g) => {
                if !config.games.contains_key(&g) {
                    bail!("{}", i18n::unknown_game(self.lang, &g));
                }
                config.default_game = Some(g.clone());
                self.save_config(&config)?;
                println!("{}", i18n::default_game(self.lang, &g));
            }
        }
        Ok(())
    }

    fn game_list(&self) -> Result<()> {
        let config = self.load_config()?;
        for name in config.games.keys() {
            let marker = if config.default_game.as_deref() == Some(name.as_str()) {
                "*"
            } else {
                " "
            };
            println!("{marker} {name}");
        }
        Ok(())
    }

    fn game_info(&self, name: &str) -> Result<()> {
        let config = self.load_config()?;
        let game = config
            .games
            .get(name)
            .with_context(|| i18n::unknown_game(self.lang, name))?;
        println!("name: {}", game.name);
        println!("root_dir: {}", game.root_dir.display());
        println!(
            "mc_version: {}",
            game.mc_version.as_deref().unwrap_or("(unset)")
        );
        println!("loader: {}", game.loader.as_deref().unwrap_or("(unset)"));
        println!(
            "loader_version: {}",
            game.loader_version.as_deref().unwrap_or("(unset)")
        );
        println!(
            "resolved_version_id: {}",
            game.resolved_version_id.as_deref().unwrap_or("(unset)")
        );
        let vc = &game.version_config;
        println!(
            "java_path: {}",
            vc.java_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(unset)".into())
        );
        println!("jvm_args: {}", vc.jvm_args.as_deref().unwrap_or("(unset)"));
        println!(
            "extra_args: {}",
            vc.extra_args.as_deref().unwrap_or("(unset)")
        );
        if vc.env.is_empty() {
            println!("env: (none)");
        } else {
            for (k, v) in &vc.env {
                println!("env: {k}={v}");
            }
        }
        Ok(())
    }

    fn game_rename(&self, old: &str, new: &str) -> Result<()> {
        let mut config = self.load_config()?;
        if !config.games.contains_key(old) {
            bail!("{}", i18n::unknown_game(self.lang, old));
        }
        if config.games.contains_key(new) {
            bail!("{}", i18n::game_already_exists(self.lang, new));
        }
        let mut game = config
            .games
            .remove(old)
            .context(i18n::game_removed_mid_rename(self.lang))?;
        game.name = new.to_owned();
        config.games.insert(new.to_owned(), game);
        if config.default_game.as_deref() == Some(old) {
            config.default_game = Some(new.to_owned());
        }
        self.save_config(&config)?;
        println!("{}", i18n::renamed_game(self.lang, old, new));
        Ok(())
    }

    /// `game config <name>`: show version-scoped config fields.
    fn game_config_show(&self, name: &str) -> Result<()> {
        let config = self.load_config()?;
        let game = config
            .games
            .get(name)
            .with_context(|| i18n::unknown_game(self.lang, name))?;
        let vc = &game.version_config;
        println!("game: {}", game.name);
        println!(
            "java_path: {}",
            vc.java_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(unset)".into())
        );
        println!("jvm_args: {}", vc.jvm_args.as_deref().unwrap_or("(unset)"));
        println!(
            "extra_args: {}",
            vc.extra_args.as_deref().unwrap_or("(unset)")
        );
        if vc.env.is_empty() {
            println!("env: (none)");
        } else {
            for (k, v) in &vc.env {
                println!("env: {k}={v}");
            }
        }
        Ok(())
    }

    /// `game config <name> set <key> <value>`: write a version-scoped field.
    fn game_config_set(&self, name: &str, key: &str, value: &str) -> Result<()> {
        let mut config = self.load_config()?;
        let game = config
            .games
            .get_mut(name)
            .with_context(|| i18n::unknown_game(self.lang, name))?;
        match key {
            "java_path" => {
                game.version_config.java_path = Some(value.into());
            }
            "jvm_args" => {
                game.version_config.jvm_args = Some(value.to_owned());
            }
            "extra_args" => {
                game.version_config.extra_args = Some(value.to_owned());
            }
            _ => {
                bail!("unknown config key '{key}'; valid keys: java_path, jvm_args, extra_args");
            }
        }
        self.save_config(&config)?;
        println!("set {key} = {value}");
        Ok(())
    }
}

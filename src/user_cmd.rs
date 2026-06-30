use anyhow::{bail, Context, Result};

use crate::app::App;
use crate::cli::UserCommand;
use crate::i18n;

impl App {
    pub(crate) fn user(&self, command: UserCommand) -> Result<()> {
        match command {
            UserCommand::Config { key, value } => self.user_config(&key, &value),
        }
    }

    fn user_config(&self, key: &str, value: &str) -> Result<()> {
        let lang = self.lang;
        let mut config = self.load_config()?;
        match key {
            k if k.starts_with("source.weight.") => {
                let provider = k.strip_prefix("source.weight.").unwrap_or(k);
                if provider.is_empty() {
                    bail!("{}", i18n::user_config_invalid_source_weight_key(lang, key));
                }
                let weight: f64 = value
                    .parse()
                    .with_context(|| i18n::user_config_invalid_number(lang, value))?;
                if weight <= 0.0 {
                    bail!(
                        "{}",
                        i18n::user_config_weight_must_be_positive(lang, weight)
                    );
                }
                config
                    .user
                    .source_weights
                    .insert(provider.to_owned(), weight);
                self.save_config(&config)?;
                println!(
                    "{}",
                    i18n::user_config_source_weight_set(lang, provider, weight)
                );
            }
            _ => {
                bail!("{}", i18n::user_config_unknown_key(lang, key));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::UserCommand;
    use crate::config::Config;

    #[test]
    fn user_config_sets_source_weight() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        let config = Config::default();
        std::fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();

        let app = App {
            config_dir: dir.path().to_path_buf(),
            state_dir: dir.path().to_path_buf(),
            provider_choice: crate::cli::ProviderChoice::Mock,
            lang: crate::i18n::Lang::default(),
        };

        app.user(UserCommand::Config {
            key: "source.weight.modrinth".to_owned(),
            value: "2.0".to_owned(),
        })
        .expect("set weight");

        let config = app.load_config().unwrap();
        assert_eq!(config.user.source_weights.get("modrinth"), Some(&2.0));
    }

    #[test]
    fn user_config_rejects_old_naming() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        let config = Config::default();
        std::fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();

        let app = App {
            config_dir: dir.path().to_path_buf(),
            state_dir: dir.path().to_path_buf(),
            provider_choice: crate::cli::ProviderChoice::Mock,
            lang: crate::i18n::Lang::default(),
        };

        let result = app.user(UserCommand::Config {
            key: "source.user.weight".to_owned(),
            value: "2.0".to_owned(),
        });
        assert!(result.is_err());
    }
}

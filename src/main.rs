use std::env;
use std::path::PathBuf;

use clap::Parser;
use directories::ProjectDirs;
use serde::Deserialize;

/// Minimal config representation for reading `lang` at startup.
/// Only deserializes the `lang` field; all other config keys are ignored.
#[derive(Deserialize, Default)]
struct StartupConfig {
    lang: Option<String>,
}

/// Load the persisted language preference from config.toml.
///
/// Determines the config directory using the same priority as
/// `App::new`: CLI `--config-dir` → `MCM_CONFIG_DIR` env → `ProjectDirs`.
/// Returns `None` if no config file exists, if it cannot be read/parsed,
/// or if no `lang` key is set.
fn load_lang_from_config(config_dir: Option<PathBuf>) -> Option<String> {
    let dir = config_dir
        .or_else(|| env::var_os("MCM_CONFIG_DIR").map(PathBuf::from))
        .or_else(|| {
            ProjectDirs::from("dev", "lucky", "mcm").map(|p| p.config_dir().to_path_buf())
        })?;

    let path = dir.join("config.toml");
    if !path.exists() {
        return None;
    }

    let text = std::fs::read_to_string(&path).ok()?;
    let config: StartupConfig = toml::from_str(&text).ok()?;
    config.lang
}

fn main() {
    let cli = mcm::Cli::parse();

    // Language priority: CLI --lang > MCM_LANG env > config.toml lang > default (en).
    let lang_str: String = if let Some(cli_lang) = cli.lang {
        match cli_lang {
            mcm::LangChoice::En => "en".to_string(),
            mcm::LangChoice::Zh => "zh".to_string(),
        }
    } else if let Ok(env_lang) = std::env::var("MCM_LANG") {
        env_lang
    } else {
        load_lang_from_config(cli.config_dir.clone()).unwrap_or_else(|| "en".to_string())
    };

    let lang = mcm::i18n::Lang::from_input(&lang_str).unwrap_or_default();

    if let Err(error) = mcm::run(cli, lang) {
        eprintln!("{}{error}", mcm::i18n::error_prefix(lang));
        std::process::exit(1);
    }
}

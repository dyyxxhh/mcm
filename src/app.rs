use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use directories::ProjectDirs;

use crate::cli::{Cli, Command, ModsCommand, ProviderChoice};
use crate::config::Config;
use crate::i18n::{self, Lang};
use crate::lock::LockState;
use crate::provider::{
    CompositeProvider, CurseForgeProvider, MockProvider, ModrinthProvider, Provider,
};

pub(crate) struct App {
    pub(crate) config_dir: PathBuf,
    pub(crate) state_dir: PathBuf,
    pub(crate) provider_choice: ProviderChoice,
    pub(crate) lang: Lang,
}

impl App {
    pub(crate) fn new(cli: &Cli, lang: Lang) -> Result<Self> {
        let project_dirs = ProjectDirs::from("dev", "lucky", "mcm")
            .context(i18n::could_not_resolve_project_dirs(lang))?;
        let config_dir = cli
            .config_dir
            .clone()
            .or_else(|| env::var_os("MCM_CONFIG_DIR").map(PathBuf::from))
            .unwrap_or_else(|| project_dirs.config_dir().to_path_buf());
        let state_dir = cli
            .state_dir
            .clone()
            .or_else(|| env::var_os("MCM_STATE_DIR").map(PathBuf::from))
            .unwrap_or_else(|| project_dirs.data_dir().to_path_buf());
        Ok(Self {
            config_dir,
            state_dir,
            provider_choice: cli.provider,
            lang,
        })
    }

    pub(crate) fn config_path(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub(crate) fn lock_path(&self, profile: &str) -> PathBuf {
        self.state_dir.join(format!("{profile}.lock.json"))
    }

    pub(crate) fn load_config(&self) -> Result<Config> {
        let path = self.config_path();
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let mut config: Config =
            toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        // One-way in-memory migration: if old profile data exists and no games
        // have been recorded yet, derive game records from profiles so `game`
        // commands see them. Old profile data is preserved on disk; the
        // migrated games are not persisted (that would race with `mods add`
        // which still operates on the legacy profile model).
        crate::game_model::migrate_profiles_to_games(&mut config);
        Ok(config)
    }

    pub(crate) fn save_config(&self, config: &Config) -> Result<()> {
        fs::create_dir_all(&self.config_dir)?;
        crate::util::atomic_write(
            &self.config_path(),
            toml::to_string_pretty(config)?.as_bytes(),
        )
    }

    pub(crate) fn active_profile(&self) -> Result<crate::config::Profile> {
        let config = self.load_config()?;
        let name = config
            .active_profile
            .as_deref()
            .context("no active profile; run profile add or profile use")?;
        config
            .profiles
            .get(name)
            .cloned()
            .with_context(|| format!("active profile {name} is missing"))
    }

    pub(crate) fn load_lock(&self, profile: &crate::config::Profile) -> Result<LockState> {
        let path = self.lock_path(&profile.name);
        if !path.exists() {
            return Ok(LockState::default());
        }
        let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
    }

    pub(crate) fn save_lock(
        &self,
        profile: &crate::config::Profile,
        lock: &LockState,
    ) -> Result<()> {
        fs::create_dir_all(&self.state_dir)?;
        crate::util::atomic_write(
            &self.lock_path(&profile.name),
            serde_json::to_string_pretty(lock)?.as_bytes(),
        )
    }

    pub(crate) fn provider(&self) -> Result<Box<dyn Provider>> {
        match self.provider_choice {
            ProviderChoice::All => Ok(Box::new(CompositeProvider::default()?)),
            ProviderChoice::Mock => Ok(Box::new(MockProvider::new())),
            ProviderChoice::Modrinth => Ok(Box::new(ModrinthProvider::new())),
            ProviderChoice::Curseforge => Ok(Box::new(CurseForgeProvider::new()?)),
        }
    }
}

pub(crate) fn run(cli: Cli, lang: Lang) -> Result<()> {
    match &cli.command {
        Some(Command::Language {
            target: Some(lang_input),
        }) => {
            if let Some(new_lang) = Lang::from_input(lang_input) {
                println!("{}", i18n::language_set(new_lang));
                println!("{}", i18n::current_language(new_lang));
                let temp_app = App::new(&cli, new_lang)?;
                let mut config = temp_app.load_config()?;
                config.lang = Some(new_lang.name().to_string());
                temp_app.save_config(&config)?;
            } else {
                println!("{}", i18n::unknown_language(lang, lang_input));
            }
            return Ok(());
        }
        Some(Command::Language { target: None }) => {
            println!("{}", i18n::current_language(lang));
            return Ok(());
        }
        _ => {}
    }

    let app = App::new(&cli, lang)?;
    match cli.command {
        Some(Command::Install { target, yes }) => app.top_install(target, yes),

        Some(Command::Upgrade { yes }) => app.upgrade(yes),
        Some(Command::FullUpgrade { yes }) => app.full_upgrade(yes),
        Some(Command::Source { command }) => app.source(command),
        Some(Command::Pkg { command }) => app.pkg(command),
        Some(Command::Game { command }) => app.game(command),
        Some(Command::Do { file, yes }) => app.do_file(file, yes),
        Some(Command::Build { input, output }) => app.build_dyyl(&input, output.as_deref()),
        Some(Command::Make { output }) => app.make_dyyl(&output),
        Some(Command::Run { dry_run }) => app.run_cmd(dry_run),
        Some(Command::Config) => Err(anyhow!(i18n::config_not_implemented_yet(lang))),
        Some(Command::User { command }) => app.user(command),

        // Mod-manager group (`mods` / `mod` alias).
        Some(Command::Mods { command }) => app.mods_command(command),

        // HTTP service (`serve`). Async (tokio+axum); run on a dedicated
        // multi-thread runtime so the blocking CLI path stays unchanged.
        Some(Command::Serve { mode, bind }) => {
            let serve_mode = crate::server::parse_mode(&mode)?;
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context(i18n::build_tokio_runtime(lang))?;
            rt.block_on(crate::server::run_server(serve_mode, bind))
        }

        None | Some(Command::Language { .. }) => Ok(()),
    }
}

impl App {
    #[expect(dead_code)]
    pub(crate) fn not_implemented(&self, name: &str) -> Result<()> {
        Err(anyhow!(i18n::not_implemented_yet(self.lang, name)))
    }

    /// `pkg info <path>`: read a `.mcm` v2 lock file, parse it, and print a
    /// normalized summary. Read-only — installs nothing.
    pub(crate) fn pkg_info(&self, path: &std::path::Path) -> Result<()> {
        let text = fs::read_to_string(path)
            .with_context(|| i18n::read_file_error(self.lang, &path.display().to_string()))?;
        let lock = crate::mcm_package::parse_mcm_lock(&text)?;
        println!("name: {}", lock.identity.name);
        println!("version: {}", lock.identity.version);
        if let Some(desc) = &lock.identity.description {
            println!("description: {desc}");
        }
        if let Some(game) = &lock.game {
            if let Some(v) = &game.version {
                println!("game_version: {v}");
            }
            if let Some(l) = &game.loader {
                println!("loader: {l}");
            }
        }
        println!("schema_version: {}", lock.schema_version);
        println!("kind: {}", lock.kind);
        println!("steps: {}", lock.steps.len());
        println!("artifacts: {}", lock.artifacts.len());
        println!(
            "permissions: install={}, do={}, full={}",
            lock.permissions.install, lock.permissions.do_permitted, lock.permissions.full
        );
        if let Some(gen) = &lock.generator {
            println!("generator: {gen}");
        }
        Ok(())
    }

    /// Dispatch the mod-manager command group (`mods` / `mod`).
    fn mods_command(&self, command: ModsCommand) -> Result<()> {
        match command {
            ModsCommand::Add {
                name,
                mods_dir,
                mc_version,
                loader,
                side,
            } => self.profile_add(name, mods_dir, mc_version, loader, side),
            ModsCommand::Use { name } => self.profile_use(&name),
            ModsCommand::ProfileList => self.profile_list(),
            ModsCommand::Show { name } => self.profile_show(name),
            ModsCommand::Search { query } => self.search(&query),
            ModsCommand::Info { query } => self.info(&query),
            ModsCommand::Install {
                query,
                file,
                dry_run,
                yes,
            } => self.install(query, file, dry_run, yes),
            ModsCommand::List => self.list(),
            ModsCommand::Status => self.status(),
            ModsCommand::Remove { logical_id, yes }
            | ModsCommand::Uninstall { logical_id, yes } => self.remove(&logical_id, yes),
            ModsCommand::Autoremove { yes } => self.autoremove(yes),
        }
    }
}

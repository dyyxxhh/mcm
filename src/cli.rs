use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

use crate::config::Side;

#[derive(Debug, Parser)]
#[command(
    name = "mcm",
    about = "Like a Linux package manager for Minecraft mods and game instances"
)]
pub struct Cli {
    #[arg(long, global = true, value_name = "DIR", env = "MCM_CONFIG_DIR")]
    pub config_dir: Option<PathBuf>,

    #[arg(long, global = true, value_name = "DIR", env = "MCM_STATE_DIR")]
    pub state_dir: Option<PathBuf>,

    #[arg(
        long,
        global = true,
        default_value = "all",
        value_enum,
        env = "MCM_PROVIDER"
    )]
    pub provider: ProviderChoice,

    #[arg(long, global = true, value_enum)]
    pub lang: Option<LangChoice>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ProviderChoice {
    All,
    Mock,
    Modrinth,
    Curseforge,
}

#[derive(Clone, Copy, Debug, ValueEnum, Default)]
pub enum LangChoice {
    #[default]
    En,
    Zh,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Switch display language (en/zh).
    Language {
        /// Language code: en or zh (omit to show current)
        target: Option<String>,
    },
    /// Low-power package installer: install a `.mcm` file path or URL.
    Install {
        /// Optional `.mcm` file path or URL. If omitted, selects the
        /// lexicographically smallest `*.mcm` in the current directory.
        target: Option<String>,

        #[arg(short, long)]
        yes: bool,
    },

    /// Upgrade the current/default game only.
    Upgrade {
        #[arg(short, long)]
        yes: bool,
    },

    /// Upgrade all configured games.
    FullUpgrade {
        #[arg(short, long)]
        yes: bool,
    },

    /// Manually manage imported sources.
    Source {
        #[command(subcommand)]
        command: SourceCommand,
    },

    /// Package download/share/install/make/info flows.
    Pkg {
        #[command(subcommand)]
        command: PkgCommand,
    },

    /// Game/version/instance management.
    Game {
        #[command(subcommand)]
        command: GameCommand,
    },

    /// Execute a `.mcm` v2 lock file (higher-power executor).
    Do {
        /// Optional `.mcm` or `.dyyl` file path. Without argument, uses
        /// the single matching file in the current directory.
        file: Option<PathBuf>,

        #[arg(short, long)]
        yes: bool,
    },

    /// Build a `.mcm` v2 JSON lock from a `.dyyl` source file.
    Build {
        /// Input `.dyyl` source file.
        input: PathBuf,

        /// Output `.mcm` lock file path. Defaults to `<input>.mcm`.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Export current instance state as `.dyyl` source.
    Make {
        /// Output `.dyyl` file path.
        output: PathBuf,
    },

    /// Launch the default game.
    Run {
        #[arg(long)]
        dry_run: bool,
    },

    /// Interactive global config editor (non-interactive subcommands may be
    /// added later).
    Config,

    /// Global user configuration.
    User {
        #[command(subcommand)]
        command: UserCommand,
    },

    /// Mod-manager command group (alias: `mods`).
    #[command(alias = "mod")]
    Mods {
        #[command(subcommand)]
        command: ModsCommand,
    },

    /// Run the HTTP service (share / source / both modes). PM2-friendly:
    /// blocking foreground process, logs to stdout/stderr, config from env.
    Serve {
        /// Route mode: `share`, `source`, or `both`.
        #[arg(long, default_value = "both")]
        mode: String,

        /// Bind address. Defaults to `127.0.0.1:8950` — never `0.0.0.0`.
        #[arg(long, default_value = "127.0.0.1:8950")]
        bind: std::net::SocketAddr,
    },
}

#[derive(Debug, Subcommand)]
pub enum SourceCommand {
    /// Add a manually imported source (trusted after import).
    Add {
        url: String,
        #[arg(short, long)]
        yes: bool,
    },
    /// Remove an imported source.
    Remove { url: String },
    /// Show info about an imported source.
    Info { url: String },
    /// List all imported sources.
    List,
}

#[derive(Debug, Subcommand)]
pub enum UserCommand {
    /// Write a user configuration key-value pair (e.g. `mcm user config source.weight.modrinth 2.0`).
    Config {
        /// Configuration key (e.g. `source.weight.modrinth`).
        key: String,
        /// Configuration value (e.g. `2.0`).
        value: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum PkgCommand {
    /// Show package info from a `.mcm` file.
    Info { path: PathBuf },
    /// Install a package from a `.mcm` file, share slug, or URL.
    Install {
        target: String,
        /// Share server URL. When set with a slug, downloads from server
        /// before installing.
        #[arg(long)]
        server: Option<String>,
        #[arg(short, long)]
        yes: bool,
    },
    /// Download a package without installing.
    Download {
        target: String,
        /// Share server URL. When set with a slug, downloads from server.
        #[arg(long)]
        server: Option<String>,
        /// Output file path (default: print to stdout).
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(short, long)]
        yes: bool,
    },
    /// Alias for `download`.
    Dl {
        target: String,
        #[arg(long)]
        server: Option<String>,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(short, long)]
        yes: bool,
    },
    /// Create a `.mcm` package from the current game state.
    Make {
        #[arg(short, long)]
        yes: bool,
        /// Output format: `mcm` (default), `mrpack` (Modrinth `.mrpack`), or
        /// `curseforge` (CurseForge-compatible manifest zip).
        #[arg(long, default_value = "mcm")]
        format: MakeFormat,
    },
    /// Publish/share a package via OIDC-authenticated flow.
    Share {
        /// `.mcm` file path to publish.
        target: String,
        /// Share server URL.
        #[arg(long, default_value = "https://mc.dyyapp.com")]
        server: String,
        #[arg(short, long)]
        yes: bool,
    },
    /// List packages (local or from server).
    List {
        /// Share server URL. List public packages from server.
        #[arg(long)]
        server: Option<String>,
        /// List packages owned by current authenticated session.
        #[arg(long)]
        mine: bool,
    },
    /// Update an owned package on the server.
    Update {
        /// Package slug to update.
        slug: String,
        /// `.mcm` file path with updated content.
        file: String,
        /// Share server URL.
        #[arg(long)]
        server: String,
        #[arg(short, long)]
        yes: bool,
    },
    /// Delete an owned package from the server.
    Delete {
        /// Package slug to delete.
        slug: String,
        /// Share server URL.
        #[arg(long)]
        server: String,
        #[arg(short, long)]
        yes: bool,
    },
    /// Manage OIDC authentication sessions.
    Auth {
        #[command(subcommand)]
        command: PkgAuthCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum PkgAuthCommand {
    /// Log in via OIDC browser flow. Prints the auth URL, then polls
    /// until the browser authentication completes.
    Login {
        /// Share server URL (e.g. `https://mc.dyyapp.com`).
        #[arg(long)]
        server: String,
    },
    /// Show the current auth status for a server.
    Status {
        /// Share server URL (e.g. `https://mc.dyyapp.com`).
        #[arg(long)]
        server: String,
    },
    /// Log out: remove the local session and invalidate it on the server.
    Logout {
        /// Share server URL (e.g. `https://mc.dyyapp.com`).
        #[arg(long)]
        server: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
pub enum MakeFormat {
    Mcm,
    Mrpack,
    Curseforge,
}

#[derive(Debug, Subcommand)]
pub enum GameCommand {
    /// Show or set the default game.
    Default { name: Option<String> },
    /// Install a Minecraft game version, optionally with a loader.
    /// Smart targets: `mc`, `mc1.21.1`, `mc-neoforge`, `mc1.21.1-neoforge`,
    /// `mc1.21.1-neoforge-21.1.172` (same grammar for fabric/forge/quilt).
    Install {
        /// Game name to create.
        name: String,
        /// Smart install target (e.g. `mc1.21.1-neoforge-21.1.172`).
        target: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(short, long)]
        yes: bool,
    },
    /// Remove a game version/instance.
    Remove {
        name: String,
        #[arg(short, long)]
        yes: bool,
    },
    /// Show info about a game.
    Info { name: String },
    /// Rename a game.
    Rename { old: String, new: String },
    /// Show or set version-scoped config for a game.
    Config {
        name: String,
        #[command(subcommand)]
        command: Option<GameConfigSubcommand>,
    },
    /// List all games.
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommand,
    },
    /// List all games.
    List,
}

#[derive(Debug, Subcommand)]
pub enum RuntimeCommand {
    /// Show Java runtime info for a game (detected / configured / required).
    Info { name: String },
    /// Install managed Java runtime for a game.
    Install {
        name: String,
        #[arg(short, long)]
        yes: bool,
        /// Install system-wide (requires root). Prints exact sudo/pkexec
        /// command in non-interactive mode.
        #[arg(long)]
        system: bool,
    },
}

/// Subcommands for `game config <name>`.
#[derive(Debug, Subcommand)]
pub enum GameConfigSubcommand {
    /// Show version-scoped config (default when no subcommand given).
    Show,
    /// Set a version-scoped config field.
    Set {
        /// Field name: java_path, jvm_args, or extra_args.
        key: String,
        /// Field value to set.
        #[arg(allow_hyphen_values = true)]
        value: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ModsCommand {
    /// Add a profile (legacy profile-add semantics).
    Add {
        name: String,
        #[arg(long)]
        mods_dir: PathBuf,
        #[arg(long)]
        mc_version: String,
        #[arg(long)]
        loader: String,
        #[arg(long, default_value = "both", value_enum)]
        side: Side,
    },
    /// Set the active profile.
    Use { name: String },
    /// Search for mods.
    Search { query: String },
    /// Show mod info (cloud or local jar).
    Info { query: String },
    /// Install a mod by logical ID or search query.
    Install {
        query: Option<String>,
        #[arg(
            short,
            long,
            value_name = "PATH",
            help = "Install mods from a mod list file"
        )]
        file: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        #[arg(short, long)]
        yes: bool,
    },
    /// List installed mods.
    List,
    /// Show status of installed mods.
    Status,
    /// Remove an installed mod (alias: `uninstall`).
    Remove {
        logical_id: String,
        #[arg(short, long)]
        yes: bool,
    },
    /// Alias for `remove`.
    Uninstall {
        logical_id: String,
        #[arg(short, long)]
        yes: bool,
    },
    /// Remove auto-installed mods no longer required by any manual root.
    Autoremove {
        #[arg(short, long)]
        yes: bool,
    },
    /// Show the active or named profile.
    Show { name: Option<String> },
    /// List profiles.
    ProfileList,
}

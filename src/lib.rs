//! mcm — apt-like Minecraft mod manager and game instance CLI.
//!
//! Module map:
//! - `cli` — Clap derive structs (`Cli`, `Command`, `ModsCommand`, `ProviderChoice`, ...)
//! - `config` — `Side`, `Config`, `Profile`, `ProfileSnapshot` (TOML persistence types)
//! - `game_model` — `GameRecord`, `GameConfig`, `GlobalConfig`, profile→game migration
//! - `lock` — `LockState`, `InstalledMod`, `InstallReason` + reachability/removal helpers
//! - `provider` — `Provider` trait, shared types (`Project`/`Artifact`/...), `CompositeProvider`
//!   + submodules: `mock`, `modrinth`, `curseforge`, `curseforge_dto`
//! - `safety` — filename sanitization, download-URL allowlist, install confirmation
//! - `confirmation` — centralized trusted-source confirmation policy (Harmless/Bypassable/NonBypassable)
//! - `jar_info` — local jar metadata reader (fabric.mod.json / mods.toml / mcmod.info)
//! - `install` — install planning (`build_plan`, `select_artifact`, `read_mod_list`)
//! - `mc_target` — `game install` smart target parser (`mc`, `mc1.21.1-neoforge-21.1.172`, ...)
//! - `mcm_package` — schema-versioned `.mcm` package types + boundary parser
//! - `source_index` — schema-versioned custom source index types + boundary parser
//! - `app` — `App` struct, config/lock IO, provider dispatch, `run()` entry point
//! - `profile_cmd` — `mods add`/`use`/`show`/`profile-list` implementations on `App`
//! - `game_cmd` — `game default/list/info/rename/config` implementations on `App`
//! - `game_install` — `game install/remove` with version/loader resolution and disk operations
//! - `loader_install` — loader jar URL computation (Fabric/Quilt/Forge/NeoForge)
//! - `source_cmd` — `source add/remove/info/list` implementations on `App`
//! - `pkg_cmd` — `pkg install/download/make/share/list` + top-level `install` / `do` on `App`
//! - `pkg_install` — package apply logic (mod jars + assets + script execution)
//! - `modpack_import` — import/export for standard modpack formats (Modrinth `.mrpack`, CurseForge `.zip`)
//! - `queries` — `search`/`info`/`list`/`status` command implementations on `App`
//! - `lifecycle` — `install`/`remove`/`autoremove` command implementations on `App`
//! - `util` — `atomic_write`, `sha256_hex`
//! - `auth` — Microsoft/Mojang auth session types, `OnlineSessionProvider` trait
//! - `auth_microsoft` — real Microsoft OAuth2 device code flow → XBL → XSTS → MC token
//! - `auth_cmd` — `mcm auth login/status/logout` Microsoft account management
//! - `launch` — typed launch command builder with explicit stages
//! - `run_cmd` — `run` command dispatch on `App`
//! - `upgrade` — `upgrade`/`full-upgrade` semantics with owner-mismatch and
//!   dependency checks
//! - `download` — retryable, resumable download engine (`download_file`, `Fetcher` trait,
//!   `HttpFetcher`, `ProviderFetcher`, `DownloadOptions`, `DownloadOutcome`)
//! - `server` — HTTP service shell (`share` / `source` / `both` modes), Axum-based,
//!   PM2-friendly, graceful shutdown. Auth via OIDC (real) or mock provider.

mod app;
mod auth;
mod auth_cmd;
mod auth_microsoft;
mod cli;
mod config;
mod confirmation;
pub mod download;
mod game_cmd;
mod game_install;
mod game_model;
pub mod i18n;
mod install;
mod jar_info;
mod launch;
mod lifecycle;
mod loader_install;
mod lock;
mod mc_target;
mod mcm_package;
mod modpack_import;
mod pkg_auth;
mod pkg_cmd;
mod pkg_install;
mod profile_cmd;
mod provider;
mod queries;
mod run_cmd;
mod runtime;
mod runtime_cmd;
mod safety;
mod server;
mod share_client;
mod source_cmd;
mod source_index;
mod source_resolve;
mod upgrade;
mod upgrade_deps;
mod user_cmd;
mod util;
mod version_json;
mod version_manifest;
mod version_resolver;

pub use cli::{
    AuthCommand, Cli, Command, GameCommand, GameConfigSubcommand, LangChoice, MakeFormat,
    ModsCommand, PkgAuthCommand, PkgCommand, ProviderChoice, SourceCommand, UserCommand,
};
pub use config::Side;
pub use mc_target::{parse_mc_target, Loader, McTarget};
pub use mcm_package::{
    parse_mcm_lock, parse_mcm_package, validate_lock_install_only, validate_lock_step_paths,
    validate_step_dest_path, McmLock, StepPermission,
};
pub use source_index::{fetch_source_index, parse_source_index, source_blob_url, SourceIndex};

// Test-only re-exports (hidden from docs). Used by `tests/server.rs` to spin
// up the router on a random port without going through `run_server`.
#[doc(hidden)]
pub use server::__test_router;
#[doc(hidden)]
pub use server::__test_router_full;
#[doc(hidden)]
pub use server::__test_router_with_data_dir;
#[doc(hidden)]
pub use server::__test_router_with_data_dir_and_clock;
#[doc(hidden)]
pub use server::__test_router_with_mock_auth;
#[doc(hidden)]
pub use server::__test_router_with_mock_user;
#[doc(hidden)]
pub use server::__test_router_with_web_dir;

#[doc(hidden)]
pub use server::storage::{
    Clock, DeleteOutcome, PackageMeta, PublishOutcome, Storage, SystemClock, UpdateOutcome,
};

pub fn run(cli: Cli, lang: i18n::Lang) -> anyhow::Result<()> {
    app::run(cli, lang)
}

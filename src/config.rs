use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Global user configuration. Written by `mcm user config <key> <value>`.
/// Priority: below version-scoped config, above built-in defaults.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub(crate) struct UserConfig {
    /// Per-provider source weights. Key is provider name (e.g. "modrinth",
    /// "curseforge", or a custom source ID). Weight multiplies
    /// `max(download_count, 1)` to compute effective downloads for artifact
    /// selection. Absent or default weight is 1.0.
    #[serde(default)]
    pub(crate) source_weights: BTreeMap<String, f64>,
}

use crate::auth::{LaunchAuthMode, OnlineAccount};
use crate::game_model::{GameRecord, GlobalConfig};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Client,
    Server,
    Both,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub(crate) struct Config {
    pub(crate) active_profile: Option<String>,
    #[serde(default)]
    pub(crate) profiles: BTreeMap<String, Profile>,
    // New game model (coexists with legacy profiles). All fields default so
    // old config.toml files without these keys deserialize cleanly.
    #[serde(default)]
    pub(crate) games: BTreeMap<String, GameRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) default_game: Option<String>,
    #[serde(default)]
    pub(crate) global: GlobalConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) lang: Option<String>,
    // Manually imported custom sources (URL → record). Fresh config starts
    // empty — no author source is preinstalled. Defaulted so old config.toml
    // files without this key deserialize cleanly.
    #[serde(default)]
    pub(crate) sources: BTreeMap<String, SourceRecord>,
    #[serde(default)]
    pub(crate) launch_auth: LaunchAuthConfig,
    #[serde(default)]
    pub(crate) user: UserConfig,
}

/// Launch auth: offline (default) or online (Microsoft/Mojang). YY-ID is
/// never used for game launch.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct LaunchAuthConfig {
    #[serde(default)]
    pub(crate) mode: LaunchAuthMode,
    /// Online account credentials. Access token is never serialized to disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) online: Option<OnlineAccount>,
}

/// A manually imported custom source. Importing makes it trusted; actionable
/// operations on sources still require confirmation via the centralized policy.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SourceRecord {
    pub(crate) url: String,
    /// Human-readable name. Auto-generated as "Source 1", "Source 2", … when
    /// the caller does not supply an explicit `--name`.
    #[serde(default)]
    pub(crate) name: String,
    /// ISO-8601 UTC timestamp of when the source was added.
    pub(crate) added_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Profile {
    pub(crate) name: String,
    pub(crate) mods_dir: PathBuf,
    pub(crate) mc_version: String,
    pub(crate) loader: String,
    pub(crate) side: Side,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ProfileSnapshot {
    pub(crate) mc_version: String,
    pub(crate) loader: String,
    pub(crate) side: Side,
}

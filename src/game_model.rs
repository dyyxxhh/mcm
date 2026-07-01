//! Typed game model: game records, version-scoped config, global config.
//!
//! Coexists with the legacy `Profile` model. One-way migration from profiles
//! to game records happens in [`migrate_profiles_to_games`]; old profile data
//! is preserved (never deleted).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::Config;

/// Version-scoped configuration for a single game (java path, jvm args, env).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GameConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) java_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) jvm_args: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) extra_args: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) env: BTreeMap<String, String>,
    /// Whether mcm should auto-compute `-Xmx` from system RAM at launch.
    ///
    /// Defaults to `true` (matches HMCL/PCL GUI auto-allocation). When
    /// `false`, the JVM heap must be set via `jvm_args` or the version
    /// JSON template. When `true`, the auto-computed `-Xmx` overrides
    /// any `-Xmx` present in the version JSON template, but is itself
    /// overridden by an explicit `-Xmx` in `jvm_args`.
    #[serde(default = "default_auto_memory")]
    pub(crate) auto_memory: bool,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            java_path: None,
            jvm_args: None,
            extra_args: None,
            env: BTreeMap::new(),
            auto_memory: default_auto_memory(),
        }
    }
}

fn default_auto_memory() -> bool {
    true
}

/// A game record (instance/version entry).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GameRecord {
    pub(crate) name: String,
    pub(crate) root_dir: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mc_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) loader: Option<String>,
    /// Exact loader version (e.g. "21.1.172" for NeoForge). Persisted in
    /// config so downstream Tasks 21/22 can read it for runtime/launch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) loader_version: Option<String>,
    /// Canonical version directory id. For vanilla this equals `mc_version`
    /// (e.g. `"1.21.1"`); for loader installs it is
    /// `"{mc_version}-{loader}-{loader_version}"` (e.g.
    /// `"1.21.1-neoforge-21.1.172"`). Used for the HMCL-compatible flat
    /// version layout: `versions/<resolved_version_id>/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) resolved_version_id: Option<String>,
    #[serde(default)]
    pub(crate) version_config: GameConfig,
}

/// Global configuration: default root for games and future path overrides.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct GlobalConfig {
    #[serde(default = "default_root_dir")]
    pub(crate) root_dir: PathBuf,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            root_dir: default_root_dir(),
        }
    }
}

/// Platform-appropriate default root: `~/mcm`.
fn default_root_dir() -> PathBuf {
    directories::UserDirs::new()
        .map(|d| d.home_dir().join("mcm"))
        .unwrap_or_else(|| PathBuf::from("mcm"))
}

/// One-way migration: if `profiles` is non-empty and `games` is empty,
/// derive a [`GameRecord`] per profile. Old profile data is NOT deleted.
///
/// Each profile's `mods_dir` parent becomes the game `root_dir`; if the
/// parent cannot be determined, the game is placed under the global root.
/// `default_game` is set from `active_profile`.
pub(crate) fn migrate_profiles_to_games(config: &mut Config) {
    if config.profiles.is_empty() || !config.games.is_empty() {
        return;
    }
    for (name, profile) in &config.profiles {
        let root_dir = profile
            .mods_dir
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| config.global.root_dir.join(name));
        let resolved_version_id = Some(profile.mc_version.clone());
        let record = GameRecord {
            name: name.clone(),
            root_dir,
            mc_version: Some(profile.mc_version.clone()),
            loader: Some(profile.loader.clone()),
            loader_version: None,
            resolved_version_id,
            version_config: GameConfig::default(),
        };
        config.games.insert(name.clone(), record);
    }
    config.default_game = config.active_profile.clone();
}

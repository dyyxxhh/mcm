//! Minecraft version and loader install/remove logic.
//!
//! Provides `game_install` and `game_remove` methods on `App`. Uses the
//! version/loader manifests and resolver to turn smart targets into concrete
//! versions, then creates the game directory structure and config record.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::app::App;
use crate::cli::ProviderChoice;
use crate::confirmation::{require_confirmation, OperationKind};
use crate::game_model::{GameConfig, GameRecord};
use crate::i18n;
use crate::mc_target::{parse_mc_target, Loader};
use crate::version_manifest::{FabricGameVersion, LoaderVersions, PromosSlim, VersionManifest};
use crate::version_resolver::{resolve_target, ResolvedTarget};

// ---------------------------------------------------------------------------
// GameManifestSource trait — abstraction for fetching version/loader manifests
// ---------------------------------------------------------------------------

/// Abstraction for fetching Minecraft version and loader manifests.
///
/// Two implementations:
/// - [`FixtureGameManifestSource`] — deterministic mock data for tests and
///   `--provider mock`.
/// - [`HttpGameManifestSource`] — real HTTP calls to Mojang and loader APIs
///   for `--provider modrinth|curseforge|all`.
pub(crate) trait GameManifestSource {
    /// Fetch the Mojang version manifest (version list + latest pointers).
    fn version_manifest(&self) -> Result<VersionManifest>;
    /// Fetch available loader versions for a specific loader type.
    fn loader_versions(&self, loader: Loader) -> Result<LoaderVersions>;
}

// ---------------------------------------------------------------------------
// Fixture implementation — deterministic mock data for tests
// ---------------------------------------------------------------------------

/// Returns deterministic mock data. Used by `--provider mock` and unit tests.
struct FixtureGameManifestSource;

impl GameManifestSource for FixtureGameManifestSource {
    fn version_manifest(&self) -> Result<VersionManifest> {
        Ok(mock_version_manifest())
    }

    fn loader_versions(&self, loader: Loader) -> Result<LoaderVersions> {
        Ok(match loader {
            Loader::Fabric => mock_fabric_versions(),
            Loader::Forge => mock_forge_versions(),
            Loader::NeoForge => mock_neoforge_versions(),
            Loader::Quilt => mock_quilt_versions(),
        })
    }
}

// ---------------------------------------------------------------------------
// HTTP implementation — real Mojang/loader API calls
// ---------------------------------------------------------------------------

/// Real HTTP implementation fetching from Mojang and loader meta APIs.
///
/// Fails clearly on network errors instead of silently using mock data.
struct HttpGameManifestSource {
    client: reqwest::blocking::Client,
}

impl HttpGameManifestSource {
    fn new() -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("mcm/0.2.0 (Minecraft manager)")
            .build()
            .context("build http client for game manifest fetch")?;
        Ok(Self { client })
    }

    /// GET a URL and deserialize JSON response.
    fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let resp = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("send http request to {url}"))?;
        if !resp.status().is_success() {
            bail!("http request to {url} returned status {}", resp.status());
        }
        resp.json()
            .with_context(|| format!("parse http json response from {url}"))
    }

    /// Fetch Fabric loader versions from the Fabric meta API.
    fn fetch_fabric_versions(&self) -> Result<LoaderVersions> {
        let entries: Vec<FabricGameVersion> =
            self.get_json("https://meta.fabricmc.net/v2/versions/loader")?;
        let mut map = BTreeMap::new();
        for gv in &entries {
            let loader_entries: Vec<_> = gv
                .loader
                .iter()
                .map(|l| crate::version_manifest::LoaderEntry {
                    version: l.version.clone(),
                    stable: l.stable,
                })
                .collect();
            map.insert(gv.game_version.clone(), loader_entries);
        }
        Ok(LoaderVersions { entries: map })
    }

    /// Fetch Quilt loader versions (same API shape as Fabric).
    fn fetch_quilt_versions(&self) -> Result<LoaderVersions> {
        let entries: Vec<FabricGameVersion> =
            self.get_json("https://meta.quiltmc.org/v3/versions/loader")?;
        let mut map = BTreeMap::new();
        for gv in &entries {
            let loader_entries: Vec<_> = gv
                .loader
                .iter()
                .map(|l| crate::version_manifest::LoaderEntry {
                    version: l.version.clone(),
                    stable: l.stable,
                })
                .collect();
            map.insert(gv.game_version.clone(), loader_entries);
        }
        Ok(LoaderVersions { entries: map })
    }

    /// Fetch NeoForge loader versions from maven promotions_slim.json.
    fn fetch_neoforge_versions(&self) -> Result<LoaderVersions> {
        let promos: PromosSlim = self.get_json(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/promotions_slim.json",
        )?;
        Ok(promos_to_loader_versions(&promos))
    }

    /// Fetch Forge loader versions from maven promotions_slim.json.
    fn fetch_forge_versions(&self) -> Result<LoaderVersions> {
        let promos: PromosSlim = self.get_json(
            "https://files.minecraftforge.net/maven/net/minecraftforge/forge/promotions_slim.json",
        )?;
        Ok(promos_to_loader_versions(&promos))
    }
}

impl GameManifestSource for HttpGameManifestSource {
    fn version_manifest(&self) -> Result<VersionManifest> {
        let url = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
        let resp = self
            .client
            .get(url)
            .send()
            .context("send http request for Mojang version manifest")?;
        if !resp.status().is_success() {
            bail!(
                "http request for Mojang version manifest returned {}",
                resp.status()
            );
        }
        resp.json()
            .context("parse Mojang version manifest http response")
    }

    fn loader_versions(&self, loader: Loader) -> Result<LoaderVersions> {
        match loader {
            Loader::Fabric => self.fetch_fabric_versions(),
            Loader::Quilt => self.fetch_quilt_versions(),
            Loader::NeoForge => self.fetch_neoforge_versions(),
            Loader::Forge => self.fetch_forge_versions(),
        }
    }
}

/// Convert a `promos_slim.json` response into `LoaderVersions`.
///
/// The promos format is `"MC_VERSION-latest"` → `"LOADER_VERSION"`.
/// We extract the MC version prefix and group loader versions by MC version,
/// deduplicating by taking the highest version per MC version.
fn promos_to_loader_versions(promos: &PromosSlim) -> LoaderVersions {
    let mut map: BTreeMap<String, Vec<crate::version_manifest::LoaderEntry>> = BTreeMap::new();

    for (key, loader_version) in &promos.promos {
        // Keys are like "1.21.1-latest" or "1.20.4-recommended".
        let mc_version = match key
            .strip_suffix("-latest")
            .or_else(|| key.strip_suffix("-recommended"))
        {
            Some(mc) => mc.to_owned(),
            None => continue,
        };

        let entry = crate::version_manifest::LoaderEntry {
            version: loader_version.clone(),
            stable: key.ends_with("-recommended"),
        };

        map.entry(mc_version).or_default().push(entry);
    }

    // Deduplicate: for each MC version, keep only one entry per loader version,
    // mark as stable if any entry for that version was recommended.
    let mut deduped: BTreeMap<String, Vec<crate::version_manifest::LoaderEntry>> = BTreeMap::new();
    for (mc_version, entries) in &map {
        let mut by_ver: BTreeMap<String, bool> = BTreeMap::new();
        for e in entries {
            by_ver
                .entry(e.version.clone())
                .and_modify(|stable| *stable = *stable || e.stable)
                .or_insert(e.stable);
        }
        let mut unique: Vec<_> = by_ver
            .into_iter()
            .map(|(version, stable)| crate::version_manifest::LoaderEntry { version, stable })
            .collect();
        unique.sort_by(|a, b| a.version.cmp(&b.version));
        deduped.insert(mc_version.clone(), unique);
    }

    LoaderVersions { entries: deduped }
}

// ---------------------------------------------------------------------------
// Mock manifest data — only used in fixture/test paths
// ---------------------------------------------------------------------------

use crate::version_manifest::{
    mock_fabric_versions, mock_forge_versions, mock_neoforge_versions, mock_quilt_versions,
    mock_version_manifest,
};

// ---------------------------------------------------------------------------
// App::get_manifests — dispatches to fixture or HTTP source
// ---------------------------------------------------------------------------

impl App {
    /// `game install <name> <target> [--dry-run] [--yes]`
    ///
    /// Resolves the smart target, then (unless dry-run) creates the game
    /// directory and writes version metadata.
    pub(crate) fn game_install(
        &self,
        name: &str,
        target: &str,
        dry_run: bool,
        yes: bool,
    ) -> Result<()> {
        let parsed = parse_mc_target(target).map_err(|e| anyhow::anyhow!("{e}"))?;

        let (mc_manifest, loader_manifests) = self.get_manifests()?;
        let resolved = resolve_target(parsed, &mc_manifest, &loader_manifests)?;

        if dry_run {
            Self::print_resolution(&resolved);
            return Ok(());
        }

        require_confirmation(OperationKind::Install, yes)?;

        // Load config and check the game doesn't already exist.
        let mut config = self.load_config()?;
        if config.games.contains_key(name) {
            bail!("{}", i18n::game_already_exists(self.lang, name));
        }

        // Determine the game root directory and resolved version id.
        let root_dir = config.global.root_dir.join(name);
        let resolved_version_id = match (&resolved.loader, &resolved.loader_version) {
            (Some(lt), Some(lv)) => {
                format!("{}-{}-{}", resolved.mc_version, lt.as_str(), lv)
            }
            _ => resolved.mc_version.clone(),
        };
        let version_dir = root_dir.join("versions").join(&resolved_version_id);
        fs::create_dir_all(&version_dir).with_context(|| {
            i18n::create_dir_error(self.lang, &version_dir.display().to_string())
        })?;

        // Write version metadata (mock version JSON).
        write_version_json(&version_dir, &resolved, &resolved_version_id)?;

        // Write minecraft.jar via the download engine.
        // Non-mock providers need real HTTP artifact fetching which is not yet
        // implemented — fail clearly rather than silently writing mock bytes.
        let jar_path = version_dir.join(format!("{resolved_version_id}.jar"));
        if self.provider_choice == ProviderChoice::Mock {
            let jar_url = format!("mock://game/{resolved_version_id}/client");
            let jar_content = mock_jar_bytes(&resolved_version_id);
            let jar_fetcher = MockGameFetcher {
                url: jar_url,
                bytes: jar_content.clone(),
            };
            download_game_artifact(
                &jar_path,
                &jar_fetcher,
                Some(jar_content.len() as u64),
                None,
            )
            .with_context(|| i18n::write_file_error(self.lang, &jar_path.display().to_string()))?;
        } else {
            bail!(
                "real game artifact download is not yet implemented; \
                 use --provider mock for fixture installs, or wait for \
                 Mojang/Forge/Fabric artifact download support"
            );
        }

        // If there's a loader, write the loader jar into the same flat
        // version directory (HMCL-compatible layout).
        let loader_version = resolved.loader_version.clone();
        let loader_type = resolved.loader;
        if let (Some(lt), Some(lv)) = (&loader_type, &loader_version) {
            let loader_jar = version_dir.join(format!("{}-{}.jar", lt.as_str(), lv));
            let loader_url = format!("mock://game/{resolved_version_id}/{}-{}", lt.as_str(), lv);
            let loader_content = mock_loader_bytes(lt, lv);
            let loader_fetcher = MockGameFetcher {
                url: loader_url,
                bytes: loader_content.clone(),
            };
            download_game_artifact(
                &loader_jar,
                &loader_fetcher,
                Some(loader_content.len() as u64),
                None,
            )
            .with_context(|| {
                i18n::write_file_error(self.lang, &loader_jar.display().to_string())
            })?;
        }

        // Materialize libraries, assets, and natives under game root.
        install_game_assets(
            &root_dir,
            &version_dir,
            &resolved_version_id,
            self.provider_choice,
        )
        .context("install game assets (libraries, assets, natives)")?;

        // Create the game record in config, persisting loader_version for
        // downstream Tasks 21/22 (runtime/launch) to read.
        let loader_name = loader_type.as_ref().map(|l| l.as_str()).map(String::from);
        let record = GameRecord {
            name: name.to_owned(),
            root_dir,
            mc_version: Some(resolved.mc_version.clone()),
            loader: loader_name,
            loader_version: resolved.loader_version.clone(),
            resolved_version_id: Some(resolved_version_id.clone()),
            version_config: GameConfig::default(),
        };
        config.games.insert(name.to_owned(), record);
        self.save_config(&config)?;

        println!("{}", i18n::installed_game(self.lang, name));
        println!("  resolved_version_id: {resolved_version_id}");
        println!("  mc_version: {}", resolved.mc_version);
        if let Some(lt) = &loader_type {
            println!("  loader: {}", lt.as_str());
        }
        if let Some(lv) = &resolved.loader_version {
            println!("  loader_version: {lv}");
        }
        Ok(())
    }

    /// `game remove <name> [--yes]`
    ///
    /// Removes the game record from config and optionally deletes the game
    /// directory from disk (unless the directory was never created).
    pub(crate) fn game_remove(&self, name: &str, yes: bool) -> Result<()> {
        require_confirmation(OperationKind::VersionRemoval, yes)?;

        let mut config = self.load_config()?;
        let game = config
            .games
            .remove(name)
            .with_context(|| i18n::unknown_game(self.lang, name))?;

        let was_default = config.default_game.as_deref() == Some(name);
        if was_default {
            config.default_game = None;
        }
        self.save_config(&config)?;

        // Delete the game directory from disk.
        let root_dir = &game.root_dir;
        if root_dir.exists() {
            fs::remove_dir_all(root_dir)
                .with_context(|| i18n::remove_error(self.lang, &root_dir.display().to_string()))?;
            println!(
                "{}",
                i18n::deleted_path(self.lang, &root_dir.display().to_string())
            );
        }

        println!("{}", i18n::removed_game_record(self.lang, &game.name));
        if was_default {
            println!("{}", i18n::default_game_cleared(self.lang));
        }
        Ok(())
    }

    /// Build manifests using the appropriate source for the current provider.
    ///
    /// - `--provider mock` → fixture data (deterministic, no network).
    /// - `--provider modrinth|curseforge|all` → real HTTP calls to Mojang
    ///   and loader meta APIs. Fails clearly on network errors.
    fn get_manifests(&self) -> Result<(VersionManifest, BTreeMap<Loader, LoaderVersions>)> {
        let source: Box<dyn GameManifestSource> = match self.provider_choice {
            ProviderChoice::Mock => Box::new(FixtureGameManifestSource),
            _ => Box::new(HttpGameManifestSource::new()?),
        };

        let mc_manifest = source
            .version_manifest()
            .context("http request for Mojang version manifest failed")?;
        let mut loader_manifests = BTreeMap::new();
        for &loader in &[
            Loader::Fabric,
            Loader::Forge,
            Loader::NeoForge,
            Loader::Quilt,
        ] {
            match source.loader_versions(loader) {
                Ok(lv) => {
                    loader_manifests.insert(loader, lv);
                }
                Err(e) => {
                    // For mock source this should never fail. For HTTP, fail
                    // clearly instead of silently falling back to mock data.
                    bail!(
                        "http request for {loader} loader versions failed: {e}",
                        loader = loader.as_str()
                    );
                }
            }
        }
        Ok((mc_manifest, loader_manifests))
    }

    fn print_resolution(resolved: &ResolvedTarget) {
        println!("dry run");
        println!("  mc_version: {}", resolved.mc_version);
        if let Some(lt) = &resolved.loader {
            println!("  loader: {}", lt.as_str());
        }
        if let Some(lv) = &resolved.loader_version {
            println!("  loader_version: {lv}");
        }
    }
}

/// Write a Mojang-format version JSON file.
///
/// Produces a realistic version manifest with libraries, arguments, asset
/// index, and download metadata. The structure mirrors real Mojang version
/// JSON so that downstream launcher logic (Task 11: classpath, assets,
/// natives) can parse it without special-casing mock data.
///
/// Libraries use mock coordinates but real Mojang structure. The mock
/// provider does not need real artifacts — these paths are placeholders
/// for the download engine (Task 11).
fn write_version_json(
    version_dir: &Path,
    resolved: &ResolvedTarget,
    version_id: &str,
) -> Result<()> {
    // Derive asset index id from the MC version (Mojang convention).
    let assets_id = match resolved.mc_version.as_str() {
        v if v.starts_with("1.21") => "12",
        v if v.starts_with("1.20.4") => "12",
        v if v.starts_with("1.20.1") => "12",
        v if v.starts_with("1.19.4") => "12",
        _ => "12",
    };

    let json = serde_json::json!({
        "id": version_id,
        "type": "release",
        "mainClass": "net.minecraft.client.main.Main",
        "inheritsFrom": null,
        "time": "2024-09-01T00:00:00+00:00",
        "releaseTime": "2024-09-01T00:00:00+00:00",
        "minimumLauncherVersion": 21,
        "assets": assets_id,
        "assetIndex": {
            "id": assets_id,
            "sha1": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "size": 456789,
            "totalSize": 1234567,
            "url": format!("https://launchermeta.mojang.com/v1/packages/{assets_id}/index.json")
        },
        "downloads": {
            "client": {
                "sha1": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
                "size": 25_000_000,
                "url": format!("https://launcher.mojang.com/v1/objects/client-{version_id}.jar")
            },
            "client_mappings": {
                "sha1": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "size": 5_000_000,
                "url": format!("https://piston-data.mojang.com/v1/objects/client-mappings-{version_id}.txt")
            },
            "server": {
                "sha1": "cccccccccccccccccccccccccccccccccccccccc",
                "size": 20_000_000,
                "url": format!("https://launcher.mojang.com/v1/objects/server-{version_id}.jar")
            }
        },
        "arguments": {
            "jvm": [
                "-Djava.library.path=${natives_directory}",
                "-Dminecraft.launcher.brand=${launcher_name}",
                "-Dminecraft.launcher.version=${launcher_version}",
                "-cp",
                "${classpath}"
            ],
            "game": [
                "--username",
                "${auth_player_name}",
                "--version",
                "${version_name}",
                "--gameDir",
                "${game_directory}",
                "--assetsDir",
                "${assets_root}",
                "--accessToken",
                "${auth_access_token}",
                "--uuid",
                "${auth_uuid}",
                "--userType",
                "${auth_user_type}",
                "--versionType",
                "${version_type}"
            ]
        },
        "libraries": [
            {
                "name": "net.minecraft:client:merged",
                "downloads": {
                    "artifact": {
                        "path": format!("net/minecraft/client/{version_id}/client-{version_id}.jar"),
                        "sha1": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
                        "size": 25_000_000,
                        "url": format!("https://libraries.minecraft.net/net/minecraft/client/{version_id}/client-{version_id}.jar")
                    }
                },
                "rules": [{"action": "allow"}]
            },
            {
                "name": "org.lwjgl:lwjgl:3.3.3",
                "natives": {"linux": "natives-linux", "windows": "natives-windows"},
                "downloads": {
                    "artifact": {
                        "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar",
                        "sha1": "dddddddddddddddddddddddddddddddddddddddd",
                        "size": 800_000,
                        "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar"
                    },
                    "classifiers": {
                        "natives-linux": {
                            "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar",
                            "sha1": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                            "size": 500_000,
                            "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar"
                        }
                    }
                },
                "rules": [{"action": "allow", "os": {"name": "linux"}}]
            }
        ]
    });
    let path = version_dir.join(format!("{version_id}.json"));
    fs::write(&path, serde_json::to_string_pretty(&json)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

/// Deterministic mock JAR bytes for a Minecraft version.
/// Used only in the fixture/mock provider path.
fn mock_jar_bytes(mc_version: &str) -> Vec<u8> {
    format!("mock minecraft jar\nversion={mc_version}\n").into_bytes()
}

/// Deterministic mock loader JAR bytes.
/// Used only in the fixture/mock provider path.
fn mock_loader_bytes(loader: &Loader, version: &str) -> Vec<u8> {
    format!("mock {} loader jar\nversion={}\n", loader.as_str(), version).into_bytes()
}

// ---------------------------------------------------------------------------
// Game asset installation — libraries, assets, natives
// ---------------------------------------------------------------------------

/// Install libraries, asset index, asset objects directory, and native
/// extraction directory under the game root after the version JSON and
/// client jar have been written.
///
/// Creates:
/// - `game_root/libraries/<artifact.path>` for each library with an artifact
/// - `game_root/assets/indexes/<assetIndex.id>.json` (mock asset index)
/// - `game_root/assets/objects/` directory
/// - `game_root/versions/<resolved_id>/natives/` directory
/// - Mock native classifier files in natives/ for libraries with a `natives` map
fn install_game_assets(
    game_root: &Path,
    version_dir: &Path,
    resolved_version_id: &str,
    provider_choice: ProviderChoice,
) -> Result<()> {
    use crate::version_json::{current_platform, native_jar_paths, parse_version_json};

    let version_json_path = version_dir.join(format!("{resolved_version_id}.json"));
    let vj = parse_version_json(&version_json_path)
        .context("parse version JSON for asset installation")?;

    let libraries_root = game_root.join("libraries");

    if provider_choice != ProviderChoice::Mock {
        bail!(
            "real library/asset download is not yet implemented; \
             use --provider mock for fixture installs, or wait for \
             Mojang/Forge/Fabric artifact download support"
        );
    }

    // --- Libraries (mock fixture bytes only) ---
    for lib in &vj.libraries {
        let Some(downloads) = &lib.downloads else {
            continue;
        };
        let Some(artifact) = &downloads.artifact else {
            continue;
        };
        let lib_path = libraries_root.join(&artifact.path);
        if let Some(parent) = lib_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create library dir: {}", parent.display()))?;
        }
        let content = mock_library_jar_bytes(&lib.name);
        fs::write(&lib_path, &content)
            .with_context(|| format!("write library: {}", lib_path.display()))?;
    }

    // --- Native classifier jars ---
    if let Some(platform) = current_platform() {
        let native_paths = native_jar_paths(&vj.libraries, &libraries_root, platform);
        let natives_dir = version_dir.join("natives");
        fs::create_dir_all(&natives_dir)
            .with_context(|| format!("create natives dir: {}", natives_dir.display()))?;
        for native_path in &native_paths {
            if let Some(parent) = native_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("create native jar dir: {}", parent.display()))?;
            }
            let content = mock_native_jar_bytes(&native_path.to_string_lossy());
            fs::write(native_path, &content)
                .with_context(|| format!("write native jar: {}", native_path.display()))?;
        }
    }

    // --- Asset index ---
    let assets_root = game_root.join("assets");
    let indexes_dir = assets_root.join("indexes");
    fs::create_dir_all(&indexes_dir).context("create assets/indexes dir")?;

    let assets_id = vj.assets.as_deref().unwrap_or("12");
    let asset_index_content = mock_asset_index_json(assets_id);
    let index_path = indexes_dir.join(format!("{assets_id}.json"));
    fs::write(&index_path, &asset_index_content).context("write asset index JSON")?;

    // --- Asset objects directory ---
    let objects_dir = assets_root.join("objects");
    fs::create_dir_all(&objects_dir).context("create assets/objects dir")?;

    Ok(())
}

/// Mock library JAR bytes — deterministic content for fixture providers.
fn mock_library_jar_bytes(library_name: &str) -> Vec<u8> {
    format!("mock library jar\nname={library_name}\n").into_bytes()
}

/// Mock native JAR bytes — deterministic content for fixture providers.
fn mock_native_jar_bytes(classifier_path: &str) -> Vec<u8> {
    format!("mock native jar\npath={classifier_path}\n").into_bytes()
}

/// Mock asset index JSON — produces a minimal valid asset index.
fn mock_asset_index_json(_assets_id: &str) -> Vec<u8> {
    let json = serde_json::json!({
        "objects": {
            "icons/icon_16x16.png": {
                "hash": "af67a45c8a3e4c8b8a1c0e8b2c4d6e8f0a1b2c3d",
                "size": 3665
            },
            "icons/icon_32x32.png": {
                "hash": "bf67a45c8a3e4c8b8a1c0e8b2c4d6e8f0a1b2c3e",
                "size": 5426
            }
        },
        "asset_map": {},
        "virtual": false,
        "map_to_resources": false
    });
    serde_json::to_vec_pretty(&json).expect("serialize asset index")
}

// ---------------------------------------------------------------------------
// Game artifact download helper — routes through download engine for staging
// ---------------------------------------------------------------------------

use crate::download::{
    download_file, DownloadOptions, FetchError, FetchOutcome, Fetcher, RangeServed,
};

/// A fetcher that returns deterministic in-memory bytes (no real download).
/// Used by [`download_game_artifact`] to route mock game/loader jar writes
/// through the retry/resume download engine (`.part` → rename, hash/size
/// validation) instead of direct `fs::write`.
struct MockGameFetcher {
    url: String,
    bytes: Vec<u8>,
}

impl Fetcher for MockGameFetcher {
    fn url(&self) -> &str {
        &self.url
    }

    fn fetch(&self, _range_start: Option<u64>) -> std::result::Result<FetchOutcome, FetchError> {
        Ok(FetchOutcome {
            bytes: self.bytes.clone(),
            total: Some(self.bytes.len() as u64),
            served: RangeServed::Full,
        })
    }
}

/// Download a game artifact (minecraft.jar or loader jar) through the retry
/// download engine. The caller provides a [`Fetcher`] — [`MockGameFetcher`] for
/// fixture mode, [`HttpFetcher`](crate::download::HttpFetcher) for production.
fn download_game_artifact(
    dest: &Path,
    fetcher: &dyn Fetcher,
    expected_size: Option<u64>,
    expected_sha256: Option<String>,
) -> Result<crate::download::DownloadOutcome> {
    let opts = DownloadOptions {
        expected_sha256,
        expected_size,
        ..Default::default()
    };
    download_file(dest, fetcher, &opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn mock_jar_bytes_are_deterministic() {
        let a = mock_jar_bytes("1.21.1");
        let b = mock_jar_bytes("1.21.1");
        assert_eq!(a, b);
        assert!(String::from_utf8_lossy(&a).contains("1.21.1"));
    }

    #[test]
    fn mock_loader_bytes_contain_loader_name_and_version() {
        let bytes = mock_loader_bytes(&Loader::Fabric, "0.16.0");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("fabric"));
        assert!(text.contains("0.16.0"));
    }

    // -----------------------------------------------------------------------
    // Download engine integration — proves game install routes through
    // download_file (`.part` → rename staging, hash/size validation).
    // These would PASS if someone bypassed `download_file` with `fs::write`
    // because `fs::write` has no hash/size verification — but the hash-
    // mismatch test would NOT pass unless the engine is used.
    // -----------------------------------------------------------------------

    #[test]
    fn download_game_artifact_writes_through_download_engine() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let dest = tmp.path().join("test.jar");
        let content = mock_jar_bytes("1.21.1");
        let expected_hash = hex::encode(Sha256::digest(&content));

        let fetcher = MockGameFetcher {
            url: "mock://game/test".to_owned(),
            bytes: content.clone(),
        };
        let outcome = download_game_artifact(
            &dest,
            &fetcher,
            Some(content.len() as u64),
            Some(expected_hash),
        )
        .expect("download should succeed");
        assert!(dest.exists(), "final artifact should exist");
        assert!(
            !dest.with_file_name("test.jar.part").exists(),
            ".part staging file must be cleaned up"
        );
        assert_eq!(outcome.bytes_written, content.len() as u64);
        assert_eq!(
            std::fs::read(&dest).expect("read dest"),
            content,
            "written content must match"
        );
    }

    #[test]
    fn download_game_artifact_rejects_hash_mismatch() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let dest = tmp.path().join("badhash.jar");
        let content = mock_jar_bytes("1.21.1");
        let wrong_hash =
            "0000000000000000000000000000000000000000dead00000000000000000000".to_owned();

        let fetcher = MockGameFetcher {
            url: "mock://game/badhash".to_owned(),
            bytes: content.clone(),
        };
        let err = download_game_artifact(
            &dest,
            &fetcher,
            Some(content.len() as u64),
            Some(wrong_hash),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("hash mismatch"),
            "error should mention hash mismatch: {err}"
        );
        assert!(!dest.exists(), "no file should be written on hash mismatch");
        assert!(
            !dest.with_file_name("badhash.jar.part").exists(),
            ".part must be cleaned up on failure"
        );
    }

    #[test]
    fn download_game_artifact_rejects_size_mismatch() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let dest = tmp.path().join("badsize.jar");
        let content = mock_jar_bytes("1.21.1");

        // Use a size that doesn't match the content length.
        let mismatched_opts = DownloadOptions {
            expected_sha256: None,
            expected_size: Some(content.len() as u64 + 999),
            ..Default::default()
        };

        let fetcher = MockGameFetcher {
            url: "mock://game/badsize".to_owned(),
            bytes: content,
        };
        let err = download_file(&dest, &fetcher, &mismatched_opts).unwrap_err();
        assert!(
            err.to_string().contains("size mismatch"),
            "error should mention size mismatch: {err}"
        );
        assert!(!dest.exists(), "no file on size mismatch");
    }

    #[test]
    fn game_record_includes_loader_version_field() {
        // Prove GameRecord has loader_version by constructing one and
        // round-tripping through serde JSON.
        let record = GameRecord {
            name: "test".to_owned(),
            root_dir: "/tmp".into(),
            mc_version: Some("1.21.1".to_owned()),
            loader: Some("neoforge".to_owned()),
            loader_version: Some("21.1.172".to_owned()),
            resolved_version_id: Some("1.21.1-neoforge-21.1.172".to_owned()),
            version_config: GameConfig::default(),
        };
        let json = serde_json::to_string(&record).expect("serialize");
        assert!(
            json.contains(r#""loader_version":"21.1.172""#),
            "GameRecord JSON must contain loader_version: {json}"
        );

        // Deserialize back (backward-compat with old configs that lack it).
        let back: GameRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.loader_version.as_deref(), Some("21.1.172"));

        // Old config without loader_version deserializes as None.
        let old_json =
            r#"{"name":"old","root_dir":"/old","mc_version":"1.20.1","loader":"fabric"}"#;
        let old: GameRecord = serde_json::from_str(old_json).expect("deserialize old");
        assert_eq!(old.loader_version, None, "old config should get None");
    }

    // -----------------------------------------------------------------------
    // FixtureGameManifestSource — prove fixture returns deterministic data
    // -----------------------------------------------------------------------

    #[test]
    fn fixture_source_returns_deterministic_manifests() {
        let source = FixtureGameManifestSource;
        let vm = source.version_manifest().expect("version manifest");
        assert_eq!(vm.latest.release, "1.21.1");
        assert!(vm.versions.iter().any(|v| v.id == "1.21.1"));

        let fabric = source.loader_versions(Loader::Fabric).expect("fabric");
        assert_eq!(fabric.latest_stable("1.21.1"), Some("0.16.0"));
    }

    // -----------------------------------------------------------------------
    // promos_to_loader_versions — unit test for the conversion helper
    // -----------------------------------------------------------------------

    #[test]
    fn promos_to_loader_versions_groups_by_mc_version() {
        let mut promos_map = BTreeMap::new();
        promos_map.insert("1.21.1-latest".to_owned(), "52.0.0".to_owned());
        promos_map.insert("1.21.1-recommended".to_owned(), "52.0.0".to_owned());
        promos_map.insert("1.20.4-latest".to_owned(), "49.0.24".to_owned());
        promos_map.insert("1.20.1-recommended".to_owned(), "47.3.0".to_owned());
        promos_map.insert("1.20.1-latest".to_owned(), "47.3.0".to_owned());

        let promos = PromosSlim { promos: promos_map };
        let lv = promos_to_loader_versions(&promos);

        assert!(lv.has_version("1.21.1", "52.0.0"));
        assert!(lv.has_version("1.20.4", "49.0.24"));
        assert!(lv.has_version("1.20.1", "47.3.0"));
        assert!(!lv.has_version("1.99", "1.0.0"));
    }
}

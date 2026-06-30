//! Minecraft version and loader install/remove logic.
//!
//! Provides `game_install` and `game_remove` methods on `App`. Uses the
//! version/loader manifests and resolver to turn smart targets into concrete
//! versions, then creates the game directory structure and config record.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};

use crate::app::App;
use crate::cli::ProviderChoice;
use crate::confirmation::{require_confirmation, OperationKind};
use crate::download::{download_file, DownloadOptions, FetchError, FetchOutcome, Fetcher, HttpFetcher, RangeServed};
use crate::game_model::{GameConfig, GameRecord};
use crate::i18n;
use crate::mc_target::{parse_mc_target, Loader};
use crate::version_json::{
    current_platform, library_applies, parse_version_json, AssetIndexContent, LibraryArtifact,
};
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

        // Confirm before doing any network/file work so non-interactive
        // callers fail fast without fetching remote manifests.
        if !dry_run {
            require_confirmation(OperationKind::Install, yes)?;
        }

        let (mc_manifest, loader_manifests) = self.get_manifests()?;
        let resolved = resolve_target(parsed, &mc_manifest, &loader_manifests)?;

        if dry_run {
            Self::print_resolution(&resolved);
            return Ok(());
        }

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

        // Find the version entry URL from the manifest. For real providers
        // this is a real Mojang URL we can fetch the per-version JSON from;
        // for mock providers it's a fixture URL (`https://mock-mojang.test/...`)
        // that we ignore — `write_version_json` fabricates a fixture JSON.
        let version_entry_url = mc_manifest
            .versions
            .iter()
            .find(|v| v.id == resolved.mc_version)
            .map(|v| v.url.as_str());

        // Write the Mojang-format version JSON. Real providers fetch the
        // actual version JSON from Mojang; mock providers fabricate a fixture.
        write_version_json(
            &version_dir,
            &resolved,
            &resolved_version_id,
            version_entry_url,
            self.provider_choice,
        )
        .context("write version JSON")?;

        // Write minecraft.jar (the client jar). Mock providers write fixture
        // bytes; real providers fetch from Mojang with SHA-1 + size verification.
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
            download_client_jar(&jar_path, &version_dir, &resolved_version_id)
                .context("download client jar from Mojang")?;
        }

        // If there's a loader, write the loader jar into the same flat
        // version directory (HMCL-compatible layout). Mock providers write
        // fixture bytes; real providers fetch from the loader's Maven repo.
        let loader_version = resolved.loader_version.clone();
        let loader_type = resolved.loader;
        if let (Some(lt), Some(lv)) = (&loader_type, &loader_version) {
            let loader_jar = version_dir.join(format!("{}-{}.jar", lt.as_str(), lv));
            if self.provider_choice == ProviderChoice::Mock {
                let loader_url =
                    format!("mock://game/{resolved_version_id}/{}-{}", lt.as_str(), lv);
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
            } else {
                download_loader_jar(&loader_jar, *lt, lv, &resolved.mc_version)
                    .with_context(|| format!("download {} loader jar {lv}", lt.as_str()))?;
            }
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
/// For real providers (`provider_choice != Mock`), fetches the actual
/// Mojang per-version JSON from the URL in the version manifest
/// (`version_entry_url`). For mock providers, fabricates a realistic-looking
/// fixture JSON so downstream code paths can be exercised without network.
fn write_version_json(
    version_dir: &Path,
    resolved: &ResolvedTarget,
    version_id: &str,
    version_entry_url: Option<&str>,
    provider_choice: ProviderChoice,
) -> Result<()> {
    let path = version_dir.join(format!("{version_id}.json"));

    // Real provider: fetch real Mojang version JSON. This gives us real
    // URLs/SHA-1/sizes for the client jar, libraries, and assets.
    if provider_choice != ProviderChoice::Mock {
        let url = version_entry_url.ok_or_else(|| {
            anyhow!(
                "no version manifest URL for mc_version `{}`; \
                 cannot fetch real version JSON",
                resolved.mc_version
            )
        })?;
        let client = http_client("fetch version JSON")?;
        let resp = client
            .get(url)
            .send()
            .with_context(|| format!("fetch version JSON from {url}"))?;
        if !resp.status().is_success() {
            bail!(
                "version JSON fetch from {url} returned status {}",
                resp.status()
            );
        }
        let text = resp.text().context("read version JSON response body")?;
        fs::write(&path, &text)
            .with_context(|| format!("write {}", path.display()))?;
        return Ok(());
    }

    // Mock provider: fabricate a fixture version JSON with fake but
    // structurally-valid URLs/SHA-1s. Downstream code (classpath, assets,
    // natives) parses this without needing real artifacts on disk.
    let assets_id = "12";

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
/// - `game_root/assets/indexes/<assetIndex.id>.json` (asset index JSON)
/// - `game_root/assets/objects/<hh>/<hash>` for each asset object
/// - `game_root/versions/<resolved_id>/natives/` directory with extracted
///   native libraries from each native classifier jar
///
/// For mock providers, libraries/assets/natives are written as fixture bytes
/// (no network). For real providers, every artifact is fetched via
/// [`HttpFetcher`](crate::download::HttpFetcher) with SHA-1 + size validation,
/// matching Mojang's published hashes.
fn install_game_assets(
    game_root: &Path,
    version_dir: &Path,
    resolved_version_id: &str,
    provider_choice: ProviderChoice,
) -> Result<()> {
    let version_json_path = version_dir.join(format!("{resolved_version_id}.json"));
    let vj = parse_version_json(&version_json_path)
        .context("parse version JSON for asset installation")?;

    let libraries_root = game_root.join("libraries");
    let platform = current_platform().context("detect current platform for native selection")?;

    // ---------------- Libraries + native classifier jars ----------------
    let mut native_jar_paths_to_extract: Vec<PathBuf> = Vec::new();
    for lib in &vj.libraries {
        if !library_applies(lib, platform) {
            continue;
        }
        let Some(downloads) = &lib.downloads else {
            continue;
        };

        // Main artifact (classpath entry).
        if let Some(artifact) = &downloads.artifact {
            let dest = libraries_root.join(&artifact.path);
            if provider_choice == ProviderChoice::Mock {
                write_mock_artifact(&dest, &lib.name)?;
            } else {
                download_artifact_via_http(&artifact, &dest)
                    .with_context(|| format!("download library {}", lib.name))?;
            }
        }

        // Native classifier jar for the current platform, if any.
        if let (Some(natives), Some(classifiers)) = (&lib.natives, &downloads.classifiers) {
            if let Some(classifier_suffix) = natives.get(platform.name) {
                if let Some(classifier_artifact) = classifiers.get(classifier_suffix) {
                    let dest = libraries_root.join(&classifier_artifact.path);
                    if provider_choice == ProviderChoice::Mock {
                        write_mock_artifact(&dest, &lib.name)?;
                    } else {
                        download_artifact_via_http(&classifier_artifact, &dest)
                            .with_context(|| format!("download native jar {}", lib.name))?;
                    }
                    native_jar_paths_to_extract.push(dest);
                }
            }
        }
    }

    // ---------------- Native extraction ----------------
    let natives_dir = version_dir.join("natives");
    fs::create_dir_all(&natives_dir)
        .with_context(|| format!("create natives dir: {}", natives_dir.display()))?;
    if provider_choice == ProviderChoice::Mock {
        // Mock fetchers wrote a single mock native file under natives/ for
        // back-compat with old tests that expected mock bytes there.
        let mock_native_path = natives_dir.join("mock-native.txt");
        fs::write(&mock_native_path, mock_native_jar_bytes("mock"))
            .with_context(|| format!("write mock native marker: {}", mock_native_path.display()))?;
    } else {
        for native_jar in &native_jar_paths_to_extract {
            extract_native_jar(native_jar, &natives_dir).with_context(|| {
                format!("extract native jar: {}", native_jar.display())
            })?;
        }
    }

    // ---------------- Asset index + asset objects ----------------
    let assets_root = game_root.join("assets");
    let indexes_dir = assets_root.join("indexes");
    let objects_dir = assets_root.join("objects");
    fs::create_dir_all(&indexes_dir).context("create assets/indexes dir")?;
    fs::create_dir_all(&objects_dir).context("create assets/objects dir")?;

    let assets_id = vj.assets.as_deref().unwrap_or("pre-1.6");
    let index_path = indexes_dir.join(format!("{assets_id}.json"));

    let asset_index_text: Vec<u8> = match (provider_choice, vj.asset_index.as_ref()) {
        (ProviderChoice::Mock, _) => mock_asset_index_json(assets_id),
        (_, Some(asset_ref)) => {
            let client = http_client("fetch asset index")?;
            let resp = client
                .get(&asset_ref.url)
                .send()
                .with_context(|| format!("fetch asset index from {}", asset_ref.url))?;
            if !resp.status().is_success() {
                bail!(
                    "asset index fetch from {} returned status {}",
                    asset_ref.url,
                    resp.status()
                );
            }
            resp.bytes()
                .map_err(|e| anyhow!("read asset index body: {e}"))?
                .to_vec()
        }
        (_, None) => bail!("version JSON has no assetIndex; cannot fetch asset index"),
    };
    fs::write(&index_path, &asset_index_text)
        .with_context(|| format!("write asset index: {}", index_path.display()))?;

    // Parse the asset index we just wrote, then materialize objects.
    let asset_index_text_str =
        std::str::from_utf8(&asset_index_text).context("asset index is not UTF-8")?;
    let asset_index: AssetIndexContent = serde_json::from_str(asset_index_text_str)
        .context("parse asset index JSON")?;

    for (name, obj) in &asset_index.objects {
        // Mojang stores assets at: <prefix>/<hash> where prefix = first 2 chars.
        let prefix: String = obj.hash.chars().take(2).collect();
        let dest = objects_dir.join(&prefix).join(&obj.hash);
        if dest.exists() {
            continue; // already materialized
        }
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create asset object dir: {}", parent.display()))?;
        }
        if provider_choice == ProviderChoice::Mock {
            fs::write(&dest, mock_asset_object_bytes(name))
                .with_context(|| format!("write mock asset: {name}"))?;
        } else {
            let url = format!(
                "https://resources.download.minecraft.net/{prefix}/{hash}",
                hash = obj.hash
            );
            let fetcher = HttpFetcher::new(&url);
            let opts = DownloadOptions {
                expected_sha1: Some(obj.hash.clone()),
                expected_size: Some(obj.size),
                ..Default::default()
            };
            download_file(&dest, &fetcher, &opts).with_context(|| {
                format!("download asset `{name}` from {url}")
            })?;
        }
    }

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

/// Mock asset object bytes — deterministic content for fixture providers.
fn mock_asset_object_bytes(name: &str) -> Vec<u8> {
    format!("mock asset\nname={name}\n").into_bytes()
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

/// Write a mock artifact file (and its parent dirs) using fixture bytes.
/// Only used in the `--provider mock` path; mirrors what the old code wrote
/// directly so existing tests continue to find files at the expected paths.
fn write_mock_artifact(dest: &Path, library_name: &str) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create artifact dir: {}", parent.display()))?;
    }
    let content = mock_library_jar_bytes(library_name);
    fs::write(dest, &content)
        .with_context(|| format!("write mock artifact: {}", dest.display()))?;
    Ok(())
}

/// Build a `reqwest::blocking::Client` with mcm's standard UA + timeout.
/// Used for one-off JSON fetches (version JSON, asset index) where routing
/// through the `download_file` engine is not necessary (no .part staging
/// needed for a small JSON file).
fn http_client(label: &str) -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("mcm/0.2.0 (Minecraft manager)")
        .build()
        .with_context(|| format!("build http client for {label}"))
}

/// Download a single Mojang library/native artifact via the retry-capable
/// download engine, verifying SHA-1 and size against the version JSON.
fn download_artifact_via_http(artifact: &LibraryArtifact, dest: &Path) -> Result<()> {
    let fetcher = HttpFetcher::new(&artifact.url);
    let opts = DownloadOptions {
        expected_sha1: artifact.sha1.clone(),
        expected_size: artifact.size,
        ..Default::default()
    };
    download_file(dest, &fetcher, &opts)
        .with_context(|| format!("download artifact from {}", artifact.url))?;
    Ok(())
}

/// Extract a native jar's `.so`/`.dylib`/`.dll` files into the natives dir.
/// Skips directories and metadata entries. Files are extracted flat — no
/// subdirectory structure is preserved — which matches the Minecraft launcher
/// convention for `--natives`.
fn extract_native_jar(jar_path: &Path, natives_dir: &Path) -> Result<()> {
    let file = fs::File::open(jar_path)
        .with_context(|| format!("open native jar: {}", jar_path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("read zip archive: {}", jar_path.display()))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .with_context(|| format!("read zip entry {i} from {}", jar_path.display()))?;
        let entry_name = entry.name().to_owned();
        // Skip directories and metadata.
        if entry.is_dir() || entry_name.starts_with("META-INF/") {
            continue;
        }
        // Only extract shared libraries; ignore license/readme files etc.
        let is_native = [".so", ".dylib", ".dll"]
            .iter()
            .any(|ext| entry_name.ends_with(ext));
        if !is_native {
            continue;
        }
        // Flatten: take only the basename so the JVM can find them via
        // `-Djava.library.path=<natives_dir>`.
        let basename = std::path::Path::new(&entry_name)
            .file_name()
            .ok_or_else(|| anyhow!("zip entry has no file name: {entry_name}"))?;
        let out_path = natives_dir.join(basename);
        let mut out = fs::File::create(&out_path)
            .with_context(|| format!("create native file: {}", out_path.display()))?;
        std::io::copy(&mut entry, &mut out)
            .with_context(|| format!("extract native file: {}", out_path.display()))?;
    }
    Ok(())
}

/// Download the Minecraft client jar (e.g. `<version_dir>/<id>.jar`).
///
/// Reads `downloads.client` from the just-written Mojang version JSON to
/// discover the URL, SHA-1, and size, then routes through the download
/// engine for retry/resume and hash validation.
fn download_client_jar(
    dest: &Path,
    version_dir: &Path,
    resolved_version_id: &str,
) -> Result<()> {
    let vj_path = version_dir.join(format!("{resolved_version_id}.json"));
    let vj = parse_version_json(&vj_path)
        .with_context(|| format!("parse version JSON: {}", vj_path.display()))?;

    let client = vj
        .downloads
        .as_ref()
        .and_then(|d| d.client.as_ref())
        .ok_or_else(|| {
            anyhow!(
                "version JSON for `{resolved_version_id}` has no \
                 `downloads.client` block; cannot fetch client jar"
            )
        })?;

    let fetcher = HttpFetcher::new(&client.url);
    let opts = DownloadOptions {
        expected_sha1: client.sha1.clone(),
        expected_size: client.size,
        ..Default::default()
    };
    download_file(dest, &fetcher, &opts)
        .with_context(|| format!("download client jar from {}", client.url))?;
    Ok(())
}

/// Download a loader jar (Fabric/Quilt/NeoForge/Forge) from the loader's
/// canonical Maven URL into the version directory.
///
/// Loader manifests don't expose SHA-1/size in our `LoaderVersions` type, so
/// we rely on Maven's HTTPS integrity and the JVM's jar verification at
/// launch time.
fn download_loader_jar(
    dest: &Path,
    loader: Loader,
    loader_version: &str,
    mc_version: &str,
) -> Result<()> {
    let url = crate::loader_install::loader_jar_url(loader, loader_version, mc_version)?;
    let fetcher = HttpFetcher::new(&url);
    let opts = DownloadOptions::default();
    download_file(dest, &fetcher, &opts)
        .with_context(|| format!("download loader jar from {url}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Game artifact download helper — routes through download engine for staging
// ---------------------------------------------------------------------------

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

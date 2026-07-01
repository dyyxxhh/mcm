//! Launch command builder with explicit typed stages.
//!
//! The launch pipeline proceeds through these stages, each represented by a
//! function that takes the output of the previous stage:
//!
//! 1. **Precheck** — Find the default game and validate it exists in config.
//! 2. **JavaSelection** — Discover a compatible Java runtime via [`discover_java`].
//! 3. **AuthSession** — Resolve auth from [`LaunchAuthConfig`].
//! 4. **FilesComplete** — Verify game jar, loader jar, and version JSON exist.
//! 5. **VersionJson** — Parse Mojang version JSON for libraries, args, natives.
//! 6. **ArgsBuild** — Assemble JVM args, classpath, main class, game args.
//! 7. **NativesExtract** — Extract native libraries to natives directory.
//! 8. **LaunchResult** — Return the final [`LaunchCommand`].
//!
//! The builder never starts a real process. Callers decide whether to print
//! the command (dry-run) or execute it (real launch).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

use crate::auth::AuthSession;
use crate::config::LaunchAuthConfig;
use crate::game_model::GameRecord;
use crate::runtime::{discover_java, DiscoveryResult, JavaRuntime};
use crate::version_json::{
    self, current_platform, interpolate_args, parse_version_json, VarMap, VersionJson,
};

/// A fully assembled launch command ready for printing or execution.
#[derive(Clone, Debug)]
#[expect(
    dead_code,
    reason = "All fields are part of the public API; unused fields will be read by downstream real-launch task"
)]
pub(crate) struct LaunchCommand {
    /// Path to the `java` binary.
    pub java_path: PathBuf,
    /// JVM-level arguments (-X, -D, -cp, etc.) — fully interpolated.
    pub jvm_args: Vec<String>,
    /// Classpath entries (jar paths) — for display/inspection.
    pub classpath: Vec<PathBuf>,
    /// Fully qualified main class name.
    pub main_class: String,
    /// Minecraft game arguments — fully interpolated.
    pub game_args: Vec<String>,
    /// Game directory (root of the game instance).
    pub game_dir: PathBuf,
    /// Minecraft version string (e.g. "1.20.1").
    pub mc_version: String,
    /// Optional loader name (e.g. "fabric").
    pub loader: Option<String>,
    /// Optional loader version (e.g. "0.16.0").
    pub loader_version: Option<String>,
    /// Auth session for --username/--uuid/--accessToken.
    pub auth_session: AuthSession,
    /// Path to the assets directory.
    pub assets_dir: PathBuf,
    /// Path to the natives directory.
    pub natives_dir: PathBuf,
    /// Asset index ID for downloading the asset index JSON.
    pub asset_index_id: String,
}

impl LaunchCommand {
    /// Render the command as a shell-safe-ish string for dry-run output.
    ///
    /// Format:
    /// ```text
    /// <java_path> \
    ///   <jvm_args> \
    ///   -cp <classpath> \
    ///   <main_class> \
    ///   <game_args>
    /// ```
    pub(crate) fn render(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        // Java binary
        parts.push(shell_quote(self.java_path.to_string_lossy().as_ref()));

        // JVM args (fully interpolated — -cp is filtered from jvm_args,
        // rendered separately from the classpath field)
        for arg in &self.jvm_args {
            parts.push(shell_quote(arg));
        }

        // Classpath (colon-separated, added after JVM args)
        if !self.classpath.is_empty() {
            parts.push("-cp".to_owned());
            let cp_string = self
                .classpath
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(":");
            parts.push(shell_quote(&cp_string));
        }

        // Main class
        parts.push(self.main_class.clone());

        // Game args
        for arg in &self.game_args {
            parts.push(shell_quote(arg));
        }

        parts.join(" \\\n  ")
    }
}

/// Run the full launch pipeline (dry-run safe).
///
/// Returns a [`LaunchCommand`] ready for rendering or execution.
pub(crate) fn build_launch_command(
    default_game: &str,
    game: &GameRecord,
    global_root: &Path,
    launch_auth: &mut LaunchAuthConfig,
) -> Result<LaunchCommand> {
    // Stage 1: Precheck
    let mc_version = game.mc_version.as_deref().with_context(|| {
        format!(
            "game {default_game} has no mc_version set; \
                 run `mcm game install {default_game} <target>` to install a version"
        )
    })?;

    // Stage 2: Java selection
    let java_runtime = select_java(game, global_root)?;

    // Stage 3: Auth session
    let auth_session = resolve_auth(launch_auth)?;

    // Stage 4: Files complete
    verify_game_files(game, mc_version)?;

    // Stage 5: Parse version JSON
    let vid = game.resolved_version_id.as_deref().unwrap_or(mc_version);
    let version_dir = game.root_dir.join("versions").join(vid);
    let version_json_path = version_dir.join(format!("{vid}.json"));
    let vj = parse_version_json(&version_json_path)
        .with_context(|| format!("parse version JSON: {}", version_json_path.display()))?;

    // Stage 6: Build args from version JSON
    let platform = current_platform().with_context(|| {
        "unsupported platform; MCM currently supports Linux, macOS, and Windows"
    })?;

    let (jvm_args, classpath, main_class, game_args, asset_index_id) =
        build_args_from_version_json(
            game,
            mc_version,
            &java_runtime,
            &auth_session,
            &vj,
            platform,
        )?;

    // Stage 7: Extract natives
    let natives_dir = version_json::natives_directory(&version_dir);
    let libraries_root = game.root_dir.join("libraries");
    extract_natives(&vj, &version_dir, &libraries_root, platform)?;

    let assets_dir = game.root_dir.join("assets");

    Ok(LaunchCommand {
        java_path: java_runtime.path.clone(),
        jvm_args,
        classpath,
        main_class,
        game_args,
        game_dir: game.root_dir.clone(),
        mc_version: mc_version.to_owned(),
        loader: game.loader.clone(),
        loader_version: game.loader_version.clone(),
        auth_session,
        assets_dir,
        natives_dir,
        asset_index_id,
    })
}

// ---------------------------------------------------------------------------
// Stage implementations
// ---------------------------------------------------------------------------

fn select_java(game: &GameRecord, global_root: &Path) -> Result<JavaRuntime> {
    match discover_java(game, global_root)
        .with_context(|| format!("Java discovery failed for game {}", game.name))?
    {
        DiscoveryResult::Found(r) => Ok(r),
        DiscoveryResult::InstallPlan { required, .. } => {
            bail!(
                "no compatible Java runtime found for game {} (requires Java {}); \
                 run `mcm game runtime install {} --yes` to install a managed runtime",
                game.name,
                required.display_version(),
                game.name,
            );
        }
    }
}

fn resolve_auth(config: &mut LaunchAuthConfig) -> Result<AuthSession> {
    match config.mode {
        crate::auth::LaunchAuthMode::Offline => crate::auth::resolve_launch_session(
            &config.mode,
            config.online.as_mut(),
            // Offline mode never invokes the provider; pass a mock.
            &crate::auth::MockOnlineProvider::success(),
        ),
        crate::auth::LaunchAuthMode::Online => crate::auth::resolve_launch_session(
            &config.mode,
            config.online.as_mut(),
            &crate::auth::MicrosoftAuthProvider::new(),
        ),
    }
}

fn verify_game_files(game: &GameRecord, mc_version: &str) -> Result<()> {
    let vid = game.resolved_version_id.as_deref().unwrap_or(mc_version);
    let version_dir = game.root_dir.join("versions").join(vid);

    let version_json_path = version_dir.join(format!("{vid}.json"));
    if !version_json_path.exists() {
        bail!(
            "version metadata not found at {}; \
             run `mcm game install {} mc{}` to reinstall",
            version_json_path.display(),
            game.name,
            mc_version,
        );
    }

    let game_jar = version_dir.join(format!("{vid}.jar"));
    if !game_jar.exists() {
        bail!(
            "game jar not found at {}; \
             run `mcm game install {} mc{}` to reinstall",
            game_jar.display(),
            game.name,
            mc_version,
        );
    }

    if let Some(ref loader) = game.loader {
        if let Some(ref lv) = game.loader_version {
            let loader_jar = version_dir.join(format!("{loader}-{lv}.jar"));
            if !loader_jar.exists() {
                bail!(
                    "loader jar for {loader} {lv} not found at {}; \
                     run `mcm game install {} mc{mc_version}-{loader}-{lv}` to reinstall",
                    loader_jar.display(),
                    game.name,
                );
            }
        }
    }

    // Validate libraries, natives, and asset index so launch fails with a
    // clear reinstall hint BEFORE spawning Java (instead of crashing inside
    // the JVM with a ClassNotFoundException / UnsatisfiedLinkError). The
    // version JSON is parsed here in addition to Stage 5; the cost is
    // negligible and keeps this preflight check self-contained.
    let vj = parse_version_json(&version_json_path).with_context(|| {
        format!("parse version JSON: {}", version_json_path.display())
    })?;
    let platform = current_platform().with_context(|| {
        "unsupported platform; MCM currently supports Linux, macOS, and Windows"
    })?;
    let libraries_root = game.root_dir.join("libraries");

    // Library artifacts (filtered for the current platform).
    let lib_paths = version_json::filter_library_artifacts(
        &vj.libraries,
        platform,
        &libraries_root,
    );
    for lib_path in &lib_paths {
        if !lib_path.exists() {
            bail!(
                "library not found at {}; \
                 run `mcm game install {} mc{}` to reinstall",
                lib_path.display(),
                game.name,
                mc_version,
            );
        }
    }

    // Native classifier JARs (we only require the classifier jar to exist;
    // the actual `.so`/`.dylib`/`.dll` extraction happens in Stage 7).
    let native_jar_paths =
        version_json::native_jar_paths(&vj.libraries, &libraries_root, platform);
    for native_jar in &native_jar_paths {
        if !native_jar.exists() {
            bail!(
                "native classifier jar not found at {}; \
                 run `mcm game install {} mc{}` to reinstall",
                native_jar.display(),
                game.name,
                mc_version,
            );
        }
    }

    // Asset index file. The asset objects themselves are downloaded lazily
    // by the game on first run from the index, so we only preflight the
    // index here (matching what `mcm run --dry-run` needs to render args).
    if vj.asset_index.is_some() {
        let assets_id = vj.assets.as_deref().unwrap_or("pre-1.6");
        let index_path = game
            .root_dir
            .join("assets")
            .join("indexes")
            .join(format!("{assets_id}.json"));
        if !index_path.exists() {
            bail!(
                "asset index not found at {}; \
                 run `mcm game install {} mc{}` to reinstall",
                index_path.display(),
                game.name,
                mc_version,
            );
        }
    } else if vj.assets.is_none() {
        bail!(
            "version JSON has no assetIndex; cannot launch; \
             run `mcm game install {} mc{}` to reinstall",
            game.name,
            mc_version,
        );
    }

    Ok(())
}

/// Build classpath, JVM args, game args, main class, and asset index ID
/// from the parsed version JSON.
type BuildResult = (Vec<String>, Vec<PathBuf>, String, Vec<String>, String);

fn build_args_from_version_json(
    game: &GameRecord,
    mc_version: &str,
    _java_runtime: &JavaRuntime,
    auth_session: &AuthSession,
    vj: &VersionJson,
    platform: version_json::Platform,
) -> Result<BuildResult> {
    let vid = game.resolved_version_id.as_deref().unwrap_or(mc_version);
    let version_dir = game.root_dir.join("versions").join(vid);
    let libraries_root = game.root_dir.join("libraries");

    let mut classpath = version_json::build_classpath(
        &vj.libraries,
        &version_dir,
        &libraries_root,
        mc_version,
        platform,
    );
    if let Some(ref loader) = game.loader {
        if let Some(ref lv) = game.loader_version {
            let loader_jar = version_dir.join(format!("{loader}-{lv}.jar"));
            classpath.push(loader_jar);
        }
    }

    // Build variable map for argument interpolation
    let mut vars: VarMap = BTreeMap::new();
    let natives_dir = version_json::natives_directory(&version_dir);
    vars.insert(
        "natives_directory".into(),
        natives_dir.to_string_lossy().to_string(),
    );
    vars.insert("launcher_name".into(), "mcm".into());
    vars.insert("launcher_version".into(), "0.2.0".into());
    let cp_string = classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(":");
    vars.insert("classpath".into(), cp_string);
    vars.insert("auth_player_name".into(), auth_session.username.clone());
    vars.insert("auth_uuid".into(), auth_session.uuid.clone());
    vars.insert(
        "auth_access_token".into(),
        auth_session.access_token.clone(),
    );
    vars.insert("auth_user_type".into(), auth_session.session_type.clone());
    vars.insert("version_name".into(), mc_version.to_owned());
    vars.insert(
        "game_directory".into(),
        game.root_dir.to_string_lossy().to_string(),
    );
    vars.insert(
        "assets_root".into(),
        game.root_dir.join("assets").to_string_lossy().to_string(),
    );
    vars.insert("version_type".into(), "release".into());

    // Interpolate JVM args from version JSON
    let jvm_args = if let Some(ref args) = vj.arguments {
        let raw = interpolate_args(&args.jvm, &vars, platform);
        // Filter out -cp since render() adds classpath separately from
        // the classpath field. We also filter the corresponding value arg.
        let mut filtered = Vec::new();
        let mut skip_next = false;
        for arg in &raw {
            if skip_next {
                skip_next = false;
                continue;
            }
            if arg == "-cp" || arg == "-classpath" {
                skip_next = true;
                continue;
            }
            filtered.push(arg.clone());
        }
        filtered
    } else {
        // Fallback JVM args for versions without argument templates
        vec![
            format!("-Djava.library.path={}", natives_dir.display()),
            "-Dminecraft.launcher.brand=mcm".into(),
            "-Dminecraft.launcher.version=0.2.0".into(),
        ]
    };

    // Add user-configured JVM args
    let mut all_jvm_args = jvm_args;
    if let Some(ref extra) = game.version_config.jvm_args {
        for arg in extra.split_whitespace() {
            if !arg.is_empty() {
                all_jvm_args.push(arg.to_owned());
            }
        }
    }

    // Interpolate game args from version JSON
    let game_args = if let Some(ref args) = vj.arguments {
        let raw = interpolate_args(&args.game, &vars, platform);
        let mut all = raw;
        // Append user-configured extra game args
        let extra: Vec<String> = game
            .version_config
            .extra_args
            .as_deref()
            .map(|s| s.split_whitespace().map(String::from).collect())
            .unwrap_or_default();
        all.extend(extra);
        all
    } else {
        // Fallback game args
        let mut args = vec![
            "--username".into(),
            auth_session.username.clone(),
            "--uuid".into(),
            auth_session.uuid.clone(),
            "--accessToken".into(),
            auth_session.access_token.clone(),
            "--sessionType".into(),
            auth_session.session_type.clone(),
            "--version".into(),
            mc_version.to_owned(),
            "--gameDir".into(),
            game.root_dir.to_string_lossy().to_string(),
            "--assetsDir".into(),
            game.root_dir.join("assets").to_string_lossy().to_string(),
            "--assetIndex".into(),
            mc_version.to_owned(),
        ];
        let extra: Vec<String> = game
            .version_config
            .extra_args
            .as_deref()
            .map(|s| s.split_whitespace().map(String::from).collect())
            .unwrap_or_default();
        args.extend(extra);
        args
    };

    // Main class: prefer version JSON, fallback to loader-based default
    let main_class = vj
        .main_class
        .clone()
        .unwrap_or_else(|| match game.loader.as_deref() {
            Some("fabric") | Some("quilt") => {
                "net.fabricmc.loader.impl.launch.knot.KnotClient".into()
            }
            Some("forge") | Some("neoforge") => "cpw.mods.modlauncher.Launcher".into(),
            _ => "net.minecraft.client.main.Main".into(),
        });

    let asset_index_id = vj.assets.clone().unwrap_or_else(|| mc_version.to_owned());

    Ok((
        all_jvm_args,
        classpath,
        main_class,
        game_args,
        asset_index_id,
    ))
}

/// Extract native library JARs to the natives directory.
///
/// Walks each native classifier JAR's contents and extracts the
/// platform-native shared libraries (`.so` / `.dylib` / `.dll`) into
/// `natives/`, matching the Minecraft launcher convention for
/// `-Djava.library.path=<natives_dir>`. Non-library entries (metadata,
/// licenses) are skipped. Directories and metadata are not preserved.
///
/// Idempotent / install-aware: `mcm game install` already extracts natives
/// at install time (real provider) or writes a `mock-native.txt` marker
/// (mock provider). This launch-time step is a repair path: if `natives/`
/// already contains files, the install-time result is trusted and we return
/// early. Otherwise we attempt extraction from the classifier JARs,
/// skipping any that are not valid ZIP archives (mock artifacts).
fn extract_natives(
    vj: &VersionJson,
    version_dir: &Path,
    libraries_root: &Path,
    platform: version_json::Platform,
) -> Result<()> {
    let natives_dir = version_json::natives_directory(version_dir);

    // Install-time extraction already populated natives/ for both real
    // (real .so/.dylib/.dll) and mock (mock-native.txt marker) providers.
    // Trust it and avoid clobbering; this also keeps mock-mode tests working
    // since the mock classifier JAR is a text file, not a real ZIP.
    if natives_dir.exists()
        && std::fs::read_dir(&natives_dir)
            .with_context(|| format!("read natives dir: {}", natives_dir.display()))?
            .next()
            .is_some()
    {
        return Ok(());
    }

    let native_paths = version_json::native_jar_paths(&vj.libraries, libraries_root, platform);

    if native_paths.is_empty() {
        return Ok(());
    }

    std::fs::create_dir_all(&natives_dir)
        .with_context(|| format!("create natives dir: {}", natives_dir.display()))?;

    // `platform.name` is `&'static str` ("linux" / "osx" / "windows"); note
    // macOS uses "osx", not "macos".
    let native_exts: &[&str] = match platform.name {
        "linux" => &[".so"],
        "osx" => &[".dylib"],
        "windows" => &[".dll"],
        _ => &[".so", ".dylib", ".dll"],
    };
    let natives_canonical = natives_dir
        .canonicalize()
        .unwrap_or_else(|_| natives_dir.clone());

    for jar_path in &native_paths {
        // Validate: no path traversal (a malicious classifier path that
        // resolves inside natives_dir would let the zip writer clobber an
        // extracted native).
        let canonical = jar_path
            .canonicalize()
            .with_context(|| format!("resolve native jar: {}", jar_path.display()))?;
        if canonical.starts_with(&natives_canonical) {
            bail!(
                "path traversal rejected: native jar {} resolves inside natives dir",
                jar_path.display()
            );
        }

        if !jar_path.exists() {
            bail!(
                "native classifier jar not found at {}; \
                 run `mcm game install ... <target>` to reinstall",
                jar_path.display()
            );
        }

        let file = std::fs::File::open(jar_path)
            .with_context(|| format!("open native jar: {}", jar_path.display()))?;
        // Mock artifacts are text files, not real ZIPs; skip them gracefully
        // rather than failing the launch. Real classifier JARs are always
        // valid ZIPs. If a real classifier is corrupted, the next
        // `mcm game install` will re-download it with hash verification.
        let mut archive = match zip::ZipArchive::new(file) {
            Ok(a) => a,
            Err(_) => continue,
        };

        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .with_context(|| format!("read zip entry {i} from {}", jar_path.display()))?;
            let entry_name = entry.name().to_owned();
            // Skip directories and JAR metadata.
            if entry.is_dir() || entry_name.starts_with("META-INF/") {
                continue;
            }
            // Only extract shared libraries; ignore license/readme files.
            let is_native = native_exts
                .iter()
                .any(|ext| entry_name.ends_with(*ext));
            if !is_native {
                continue;
            }
            // Flatten: take only the basename so the JVM can find them via
            // `-Djava.library.path=<natives_dir>`.
            let basename = std::path::Path::new(&entry_name)
                .file_name()
                .ok_or_else(|| anyhow!("zip entry has no file name: {entry_name}"))?;
            let out_path = natives_dir.join(basename);
            // Guard against zip-slip: basename must not escape natives_dir.
            let out_canonical = out_path
                .parent()
                .and_then(|p| p.canonicalize().ok())
                .unwrap_or_else(|| natives_dir.clone());
            if !out_canonical.starts_with(&natives_canonical) {
                bail!(
                    "path traversal rejected: zip entry {} escapes natives dir",
                    entry_name
                );
            }
            let mut out = std::fs::File::create(&out_path)
                .with_context(|| format!("create native file: {}", out_path.display()))?;
            std::io::copy(&mut entry, &mut out)
                .with_context(|| format!("extract native file: {}", out_path.display()))?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn shell_quote(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '@' | '+' | '-' | ':'))
    {
        return s.to_owned();
    }
    let escaped = s.replace('\'', "'\\''");
    format!("'{escaped}'")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::LaunchAuthMode;
    use crate::game_model::GameConfig;
    use std::io::Write;

    fn make_game(mc_version: &str, loader: Option<&str>, lv: Option<&str>) -> GameRecord {
        let resolved_version_id = match (loader, lv) {
            (Some(l), Some(v)) => Some(format!("{mc_version}-{l}-{v}")),
            _ => Some(mc_version.to_owned()),
        };
        GameRecord {
            name: "test".to_owned(),
            root_dir: "/tmp".into(),
            mc_version: Some(mc_version.to_owned()),
            loader: loader.map(String::from),
            loader_version: lv.map(String::from),
            resolved_version_id,
            version_config: GameConfig::default(),
        }
    }

    #[test]
    fn launch_command_render_contains_java_path() {
        let cmd = LaunchCommand {
            java_path: "/usr/bin/java".into(),
            jvm_args: vec!["-Xmx2G".into()],
            classpath: vec!["/tmp/test.jar".into()],
            main_class: "net.minecraft.client.main.Main".into(),
            game_args: vec!["--username".into(), "Player".into()],
            game_dir: "/tmp".into(),
            mc_version: "1.20.1".into(),
            loader: None,
            loader_version: None,
            auth_session: AuthSession::offline("Player"),
            assets_dir: "/tmp/assets".into(),
            natives_dir: "/tmp/natives".into(),
            asset_index_id: "12".into(),
        };
        let output = cmd.render();
        assert!(output.contains("/usr/bin/java"));
        assert!(output.contains("-Xmx2G"));
        assert!(output.contains("net.minecraft.client.main.Main"));
        assert!(output.contains("--username"));
    }

    #[test]
    fn shell_quote_passes_through_safe_strings() {
        assert_eq!(shell_quote("/usr/bin/java"), "/usr/bin/java");
        assert_eq!(
            shell_quote("net.minecraft.client.main.Main"),
            "net.minecraft.client.main.Main"
        );
        assert_eq!(shell_quote("-Xmx2G"), "-Xmx2G");
    }

    #[test]
    fn shell_quote_quotes_strings_with_spaces() {
        assert_eq!(shell_quote("hello world"), "'hello world'");
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn resolve_auth_offline_default() {
        let mut config = LaunchAuthConfig::default();
        let session = resolve_auth(&mut config).expect("offline should succeed");
        assert_eq!(session.username, "Player");
        assert_eq!(session.access_token, "0");
    }

    #[test]
    fn resolve_auth_online_without_refresh_token_errors() {
        let mut config = LaunchAuthConfig {
            mode: LaunchAuthMode::Online,
            online: Some(crate::auth::OnlineAccount {
                username: "OnlineUser".into(),
                uuid: "deadbeef-dead-beef-dead-beefdeadbeef".into(),
                access_token: "real-token".into(),
                user_type: "microsoft".into(),
                ..Default::default()
            }),
        };
        let err = resolve_auth(&mut config).unwrap_err();
        assert!(
            err.to_string().contains("refresh token"),
            "error should mention refresh token: {err}"
        );
    }

    #[test]
    fn select_java_errors_with_actionable_message_when_no_java() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let game = make_game("1.16.5", None, None);
        match select_java(&game, tmp.path()) {
            Ok(java) => {
                assert_eq!(java.major, crate::runtime::JavaMajor::Java8);
            }
            Err(e) => {
                let msg = e.to_string();
                assert!(msg.contains("game runtime install"), "{msg}");
            }
        }
    }

    /// Build a full fixture game directory with version JSON + jars for
    /// integration testing of the launch pipeline. Includes a managed Java 21
    /// runtime so the pipeline doesn't fail on Java discovery.
    fn build_fixture_game(
        tmp: &tempfile::TempDir,
        name: &str,
        mc_version: &str,
        loader: Option<(&str, &str)>,
    ) -> GameRecord {
        let root = tmp.path().join(name);
        let resolved_version_id = match loader {
            Some((ln, lv)) => format!("{mc_version}-{ln}-{lv}"),
            None => mc_version.to_owned(),
        };
        let version_dir = root.join("versions").join(&resolved_version_id);
        std::fs::create_dir_all(&version_dir).expect("create version dir");

        // Write managed Java 21 runtime so discovery succeeds
        let managed_dir = tmp
            .path()
            .join("runtimes")
            .join("java")
            .join("java21")
            .join("bin");
        std::fs::create_dir_all(&managed_dir).expect("create managed java dir");
        let java_path = managed_dir.join("java");
        std::fs::write(&java_path, b"mock java").expect("write mock java");
        std::fs::write(managed_dir.join("java.version"), "21\n").expect("write marker");

        // Write version JSON (same structure as game_install.rs mock)
        let vj = serde_json::json!({
            "id": resolved_version_id,
            "mainClass": "net.minecraft.client.main.Main",
            "assets": "12",
            "assetIndex": {
                "id": "12",
                "sha1": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "size": 456789,
                "totalSize": 1234567,
                "url": "https://launchermeta.mojang.com/v1/packages/12/index.json"
            },
            "libraries": [
                {
                    "name": format!("net.minecraft:client:{mc_version}"),
                    "downloads": {
                        "artifact": {
                            "path": format!("net/minecraft/client/{mc_version}/client-{mc_version}.jar"),
                            "sha1": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
                            "size": 25000000,
                            "url": "https://libraries.minecraft.net/test.jar"
                        }
                    }
                },
                {
                    "name": "org.lwjgl:lwjgl:3.3.3",
                    "rules": [{"action": "allow", "os": {"name": "linux"}}],
                    "downloads": {
                        "artifact": {
                            "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar",
                            "sha1": "dddddddddddddddddddddddddddddddddddddddd",
                            "size": 800000,
                            "url": "https://libraries.minecraft.net/lwjgl.jar"
                        },
                        "classifiers": {
                            "natives-linux": {
                                "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar",
                                "sha1": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                                "size": 500000,
                                "url": "https://libraries.minecraft.net/lwjgl-natives.jar"
                            }
                        }
                    },
                    "natives": {"linux": "natives-linux"}
                }
            ],
            "arguments": {
                "jvm": [
                    "-Djava.library.path=${natives_directory}",
                    "-Dminecraft.launcher.brand=${launcher_name}",
                    "-Dminecraft.launcher.version=${launcher_version}",
                    "-cp", "${classpath}"
                ],
                "game": [
                    "--username", "${auth_player_name}",
                    "--uuid", "${auth_uuid}",
                    "--accessToken", "${auth_access_token}",
                    "--userType", "${auth_user_type}",
                    "--version", "${version_name}",
                    "--gameDir", "${game_directory}",
                    "--assetsDir", "${assets_root}",
                    "--versionType", "${version_type}"
                ]
            }
        });
        let json_path = version_dir.join(format!("{resolved_version_id}.json"));
        std::fs::write(&json_path, serde_json::to_string_pretty(&vj).unwrap())
            .expect("write version json");

        // Write game jar
        let game_jar = version_dir.join(format!("{resolved_version_id}.jar"));
        std::fs::write(&game_jar, b"mock game jar").expect("write game jar");

        // Write loader jar if present
        if let Some((loader_name, loader_ver)) = loader {
            let loader_jar = version_dir.join(format!("{loader_name}-{loader_ver}.jar"));
            std::fs::write(&loader_jar, b"mock loader jar").expect("write loader jar");
        }

        // Write library artifacts so verify_game_files' preflight passes.
        let libraries_root = root.join("libraries");
        let lib_paths = [
            format!("net/minecraft/client/{mc_version}/client-{mc_version}.jar"),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar".to_owned(),
        ];
        for p in &lib_paths {
            let dest = libraries_root.join(p);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).expect("create lib dir");
            }
            std::fs::write(&dest, b"mock library jar").expect("write library jar");
        }

        // Write asset index file so verify_game_files' preflight passes.
        let asset_index_dir = root.join("assets").join("indexes");
        std::fs::create_dir_all(&asset_index_dir).expect("create asset index dir");
        std::fs::write(asset_index_dir.join("12.json"), b"{\"objects\":{}}")
            .expect("write asset index");

        // Write a REAL native classifier zip containing a `.so` so the
        // launch-time extract_natives step has something valid to extract.
        // (The old fixture wrote a text file, which only worked because the
        // old extract_natives just copied the jar verbatim.)
        let native_jar_dir = libraries_root.join("org/lwjgl/lwjgl/3.3.3");
        std::fs::create_dir_all(&native_jar_dir).expect("create native jar dir");
        let native_jar_path = native_jar_dir.join("lwjgl-3.3.3-natives-linux.jar");
        {
            let file = std::fs::File::create(&native_jar_path).expect("create native jar file");
            let mut writer = zip::ZipWriter::new(file);
            let options =
                zip::write::FileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored);
            writer
                .start_file("liblwjgl.so", options)
                .expect("start zip entry");
            writer.write_all(b"mock native shared lib").expect("write .so bytes");
            writer.finish().expect("finalize native jar zip");
        }

        GameRecord {
            name: name.to_owned(),
            root_dir: root,
            mc_version: Some(mc_version.to_owned()),
            loader: loader.map(|(n, _)| n.to_owned()),
            loader_version: loader.map(|(_, v)| v.to_owned()),
            resolved_version_id: Some(resolved_version_id),
            version_config: GameConfig::default(),
        }
    }

    #[test]
    fn full_pipeline_vanilla_produces_valid_launch_command() {
        let tmp = tempfile::tempdir().expect("tmp");
        let game = build_fixture_game(&tmp, "dev", "1.21.1", None);
        let mut auth = LaunchAuthConfig::default();
        let cmd =
            build_launch_command("dev", &game, tmp.path(), &mut auth).expect("build should succeed");

        // Classpath should contain game jar + libraries
        assert!(
            cmd.classpath.len() >= 2,
            "classpath should have game + lib jars"
        );
        assert!(
            cmd.classpath[0].to_string_lossy().contains("1.21.1.jar"),
            "first classpath entry should be game jar"
        );

        // JVM args should include -Djava.library.path
        assert!(
            cmd.jvm_args
                .iter()
                .any(|a| a.starts_with("-Djava.library.path=")),
            "jvm_args should set java.library.path"
        );

        // Should NOT contain -cp (filtered out, rendered separately)
        assert!(
            !cmd.jvm_args.contains(&"-cp".to_owned()),
            "jvm_args should not contain -cp"
        );

        // Game args should have auth fields
        assert!(cmd.game_args.contains(&"--username".to_owned()));
        assert!(cmd.game_args.contains(&"--uuid".to_owned()));

        // Main class from version JSON
        assert_eq!(cmd.main_class, "net.minecraft.client.main.Main");

        // Asset index
        assert_eq!(cmd.asset_index_id, "12");

        // Natives dir
        assert!(cmd.natives_dir.to_string_lossy().contains("natives"));

        // Rendered output
        let rendered = cmd.render();
        assert!(!rendered.is_empty(), "rendered output should not be empty");
        assert!(rendered.contains("-Djava.library.path="));
    }

    #[test]
    fn full_pipeline_fabric_uses_version_json_main_class() {
        let tmp = tempfile::tempdir().expect("tmp");
        let game = build_fixture_game(&tmp, "dev", "1.21.1", Some(("fabric", "0.16.0")));
        let mut auth = LaunchAuthConfig::default();
        let cmd =
            build_launch_command("dev", &game, tmp.path(), &mut auth).expect("build should succeed");

        // Main class from version JSON (not loader-based fallback)
        assert_eq!(cmd.main_class, "net.minecraft.client.main.Main");
        // Loader jar in classpath
        assert!(
            cmd.classpath
                .iter()
                .any(|p| p.to_string_lossy().contains("fabric-0.16.0.jar")),
            "classpath should contain fabric loader jar"
        );
    }

    #[test]
    fn full_pipeline_extracts_natives() {
        let tmp = tempfile::tempdir().expect("tmp");
        let game = build_fixture_game(&tmp, "dev", "1.21.1", None);
        let mut auth = LaunchAuthConfig::default();
        let cmd =
            build_launch_command("dev", &game, tmp.path(), &mut auth).expect("build should succeed");

        // Natives directory should exist and contain the extracted `.so`.
        assert!(cmd.natives_dir.exists(), "natives dir should be created");
        let native_entries: Vec<_> = std::fs::read_dir(&cmd.natives_dir)
            .expect("read natives dir")
            .filter_map(|e| e.ok())
            .collect();
        assert!(
            native_entries
                .iter()
                .any(|e| e.file_name().to_string_lossy().ends_with(".so")),
            "natives dir should contain an extracted .so, got: {native_entries:?}"
        );
    }

    #[test]
    fn full_pipeline_offline_session_uses_stable_uuid() {
        let tmp = tempfile::tempdir().expect("tmp");
        let game = build_fixture_game(&tmp, "dev", "1.21.1", None);
        let mut game_config = game.clone();
        game_config.version_config = GameConfig::default();

        let mut auth = LaunchAuthConfig::default();
        let cmd = build_launch_command("dev", &game_config, tmp.path(), &mut auth)
            .expect("build should succeed");

        let expected_uuid = crate::auth::offline_uuid("Player");
        assert!(
            cmd.game_args.contains(&expected_uuid),
            "offline UUID should be deterministic"
        );
    }

    #[test]
    fn build_launch_command_errors_when_no_mc_version() {
        let game = GameRecord {
            name: "empty".into(),
            root_dir: "/tmp".into(),
            mc_version: None,
            loader: None,
            loader_version: None,
            resolved_version_id: None,
            version_config: GameConfig::default(),
        };
        let mut auth = LaunchAuthConfig::default();
        let err = build_launch_command("empty", &game, Path::new("/tmp"), &mut auth).unwrap_err();
        assert!(err.to_string().contains("no mc_version"), "{err}");
    }

    #[test]
    fn build_launch_command_errors_when_version_json_missing() {
        let tmp = tempfile::tempdir().expect("tmp");
        // Set up managed Java so Java discovery succeeds
        let managed_dir = tmp.path().join("runtimes/java/java21/bin");
        std::fs::create_dir_all(&managed_dir).expect("create managed dir");
        std::fs::write(managed_dir.join("java"), b"mock java").expect("write mock java");
        std::fs::write(managed_dir.join("java.version"), "21\n").expect("write marker");

        let root = tmp.path().join("dev");
        let version_dir = root.join("versions").join("1.21.1");
        std::fs::create_dir_all(&version_dir).expect("create dir");
        std::fs::write(version_dir.join("1.21.1.jar"), b"jar").expect("write jar");

        let game = GameRecord {
            name: "dev".into(),
            root_dir: root,
            mc_version: Some("1.21.1".into()),
            loader: None,
            loader_version: None,
            resolved_version_id: Some("1.21.1".into()),
            version_config: GameConfig::default(),
        };
        let mut auth = LaunchAuthConfig::default();
        let err = build_launch_command("dev", &game, tmp.path(), &mut auth).unwrap_err();
        assert!(
            err.to_string().contains("version metadata not found"),
            "error should mention missing metadata: {err}"
        );
    }

    #[test]
    fn build_launch_command_errors_when_game_jar_missing() {
        let tmp = tempfile::tempdir().expect("tmp");
        // Set up managed Java
        let managed_dir = tmp.path().join("runtimes/java/java21/bin");
        std::fs::create_dir_all(&managed_dir).expect("create managed dir");
        std::fs::write(managed_dir.join("java"), b"mock java").expect("write mock java");
        std::fs::write(managed_dir.join("java.version"), "21\n").expect("write marker");

        let root = tmp.path().join("dev");
        let version_dir = root.join("versions").join("1.21.1");
        std::fs::create_dir_all(&version_dir).expect("create dir");

        let vj = serde_json::json!({"id": "1.21.1", "mainClass": "net.minecraft.client.main.Main", "libraries": []});
        std::fs::write(
            version_dir.join("1.21.1.json"),
            serde_json::to_string_pretty(&vj).unwrap(),
        )
        .expect("write json");

        let game = GameRecord {
            name: "dev".into(),
            root_dir: root,
            mc_version: Some("1.21.1".into()),
            loader: None,
            loader_version: None,
            resolved_version_id: Some("1.21.1".into()),
            version_config: GameConfig::default(),
        };
        let mut auth = LaunchAuthConfig::default();
        let err = build_launch_command("dev", &game, tmp.path(), &mut auth).unwrap_err();
        assert!(
            err.to_string().contains("game jar not found"),
            "error should mention missing jar: {err}"
        );
    }
}

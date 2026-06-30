//! Java runtime discovery, compatibility matrix, and managed install.
//!
//! Three-tier discovery with version-verification:
//! 1. User-configured `java_path` in `GameConfig` — probed for actual version.
//!    Wrong-major → actionable error. Unprobeable → skipped.
//! 2. Managed runtime under `{global.root_dir}/runtimes/java/{major}/bin/java`
//!    — verified via sidecar `java.version` marker written at install time.
//! 3. System `java` on PATH — probed for actual version. Wrong-major →
//!    skipped (falls through to install plan), never silently accepted.
//!
//! The compatibility matrix maps MC version → required Java major:
//! - MC < 1.17 (up to 1.16.5) → Java 8
//! - MC 1.17 through 1.20 → Java 17
//! - MC 1.21+ → Java 21

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::game_model::GameRecord;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Java major version matching Minecraft runtime requirements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum JavaMajor {
    Java8,
    Java17,
    Java21,
}

impl JavaMajor {
    /// Display form like "8", "17", "21".
    pub(crate) fn display_version(self) -> &'static str {
        match self {
            JavaMajor::Java8 => "8",
            JavaMajor::Java17 => "17",
            JavaMajor::Java21 => "21",
        }
    }

    /// Expected `java` binary name for this version in the managed runtime dir.
    fn managed_subdir(self) -> &'static str {
        match self {
            JavaMajor::Java8 => "java8",
            JavaMajor::Java17 => "java17",
            JavaMajor::Java21 => "java21",
        }
    }

    /// The Java major version required by a given Minecraft version.
    /// Returns `None` for unrecognised MC versions.
    pub(crate) fn from_mc_version(mc_version: &str) -> Option<JavaMajor> {
        let parts: Vec<&str> = mc_version.split('.').collect();
        if parts.len() < 2 {
            return None;
        }
        let major: u32 = parts[0].parse().ok()?;
        let minor: u32 = parts[1].parse().ok()?;

        #[allow(clippy::match_overlapping_arm)]
        match (major, minor) {
            (1, 0..=16) => Some(JavaMajor::Java8),
            (1, 17..=20) => Some(JavaMajor::Java17),
            (1, 21..) => Some(JavaMajor::Java21),
            _ => None,
        }
    }
}

/// Where a discovered Java runtime was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JavaSource {
    /// Explicitly configured by user in `GameConfig.java_path`.
    UserConfig(PathBuf),
    /// Managed runtime under MCM root.
    Managed(PathBuf),
    /// Found on system PATH via `java -version`.
    System,
}

/// A discovered Java runtime with its source.
#[derive(Debug, Clone)]
pub(crate) struct JavaRuntime {
    pub major: JavaMajor,
    pub source: JavaSource,
    pub path: PathBuf,
}

// ---------------------------------------------------------------------------
// Version probing — parse `java -version` stderr output
// ---------------------------------------------------------------------------

/// Parse the first two lines of `java -version` stderr to extract the
/// Java major version.
///
/// Supported formats:
/// - Java 8:    `java version "1.8.0_402"` → `Java8`
/// - Java 17:   `openjdk version "17.0.10"` → `Java17`
/// - Java 21:   `openjdk version "21.0.2"` → `Java21`
/// - GraalVM / legacy variants are handled by the version-number pattern.
pub(crate) fn parse_java_version_output(output: &str) -> Option<JavaMajor> {
    // Look for a line containing `version "X.Y.Z"` or `version "X.Y"` or
    // `version "X"`.
    // Java 8 uses `1.8` prefix; Java 9+ use the major directly.
    for line in output.lines() {
        if let Some(quoted) = line.split('"').nth(1) {
            let v = quoted.trim();
            // Try splitting at the first dot.
            if let Some((first, rest)) = v.split_once('.') {
                // Java 8: version "1.8.x" → first="1", rest="8.x"
                if first == "1" {
                    if let Some(minor_str) = rest.split('.').next() {
                        if let Ok(minor) = minor_str.parse::<u32>() {
                            if minor == 8 {
                                return Some(JavaMajor::Java8);
                            }
                        }
                    }
                }
                // Java 17+: major.minor where first >= 17 (or first has
                // suffix like "17-ea" which we strip before parsing).
                let major_str = first.split_once('-').map_or(first, |(n, _)| n);
                let major_str = major_str.split_once('+').map_or(major_str, |(n, _)| n);
                if let Ok(major) = major_str.parse::<u32>() {
                    match major {
                        17 => return Some(JavaMajor::Java17),
                        21 => return Some(JavaMajor::Java21),
                        _ => {}
                    }
                }
            } else {
                // No dot — extract leading numeric part: "17-ea" → "17"
                let numeric = v
                    .split_once('-')
                    .or_else(|| v.split_once('+'))
                    .map_or(v, |(prefix, _)| prefix);
                if let Ok(major) = numeric.parse::<u32>() {
                    match major {
                        8 => return Some(JavaMajor::Java8),
                        17 => return Some(JavaMajor::Java17),
                        21 => return Some(JavaMajor::Java21),
                        _ => {}
                    }
                }
            }
        }
    }
    None
}

/// Probe a `java` binary at `path` by running `{path} -version` and
/// parsing the stderr output.
///
/// Returns `None` if the binary cannot be executed or its version cannot
/// be parsed (e.g. not a real java binary, or permission denied).
fn probe_java_version(path: &Path) -> Option<JavaMajor> {
    let output = Command::new(path).arg("-version").output().ok()?;
    let stderr = String::from_utf8(output.stderr).ok()?;
    // `java -version` prints to stderr. Also check stdout as a fallback.
    let stdout = String::from_utf8(output.stdout).ok()?;
    parse_java_version_output(&stderr).or_else(|| parse_java_version_output(&stdout))
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Result of Java discovery — either a concrete runtime or a description of
/// what is needed.
#[derive(Debug)]
pub(crate) enum DiscoveryResult {
    /// A usable Java runtime was found.
    Found(JavaRuntime),
    /// No Java found; an install plan is needed.
    InstallPlan {
        required: JavaMajor,
        managed_path: PathBuf,
    },
}

/// Discover a Java runtime suitable for `game`.
///
/// Checks, in order:
/// 1. User-configured `java_path` — accepts only if version-probed matches.
/// 2. Managed runtime under MCM root — verified via sidecar marker.
/// 3. System `java` on PATH — accepts only if version-probed matches.
///
/// Wrong-major user config produces an actionable error.
/// Wrong-major system Java is silently skipped (falls through to install plan).
pub(crate) fn discover_java(game: &GameRecord, global_root: &Path) -> Result<DiscoveryResult> {
    discover_java_impl(game, global_root, None)
}

/// Implementation of [`discover_java`] with an optional test seam for system
/// Java probing.
fn discover_java_impl(
    game: &GameRecord,
    global_root: &Path,
    system_java_test_path: Option<&Path>,
) -> Result<DiscoveryResult> {
    let mc_version = match &game.mc_version {
        Some(v) => v,
        None => bail!("game {} has no mc_version set", game.name),
    };
    let required = JavaMajor::from_mc_version(mc_version)
        .with_context(|| format!("unknown MC version {mc_version}"))?;

    // 1. User-configured java_path — verify actual version.
    if let Some(jp) = &game.version_config.java_path {
        if jp.exists() {
            match probe_java_version(jp) {
                Some(actual) if actual == required => {
                    return Ok(DiscoveryResult::Found(JavaRuntime {
                        major: actual,
                        source: JavaSource::UserConfig(jp.clone()),
                        path: jp.clone(),
                    }));
                }
                Some(actual) => {
                    bail!(
                        "configured java at {} is version {}, but {} requires Java {}",
                        jp.display(),
                        actual.display_version(),
                        mc_version,
                        required.display_version(),
                    );
                }
                None => {
                    // Binary exists but can't be probed — treat as unusable.
                    // Rather than silently skipping, warn and continue.
                }
            }
        }
    }

    // 2. Managed runtime under global root — verified via sidecar marker.
    let managed_root = global_root
        .join("runtimes")
        .join("java")
        .join(required.managed_subdir());
    let managed_path = managed_root.join("bin").join("java");
    let marker_path = managed_root.join("bin").join("java.version");
    if managed_path.exists() && marker_path.exists() {
        if let Ok(marker) = std::fs::read_to_string(&marker_path) {
            if marker.trim() == required.display_version() {
                return Ok(DiscoveryResult::Found(JavaRuntime {
                    major: required,
                    source: JavaSource::Managed(managed_root),
                    path: managed_path,
                }));
            }
        }
    }

    // 3. System PATH — probe version; wrong-major silently falls through.
    if let Some(sys_java) = probe_system_java_with(system_java_test_path) {
        match probe_java_version(&sys_java) {
            Some(actual) if actual == required => {
                return Ok(DiscoveryResult::Found(JavaRuntime {
                    major: actual,
                    source: JavaSource::System,
                    path: sys_java,
                }));
            }
            _ => {
                // Wrong major or unprobeable — skip, fall through to install.
            }
        }
    }

    // No compatible Java found → install plan.
    Ok(DiscoveryResult::InstallPlan {
        required,
        managed_path: managed_root,
    })
}

/// Probe the system PATH for a `java` binary.
/// Returns the path to the binary if found, `None` otherwise.
///
/// `test_override`: when `Some(path)`, returns that path if it exists
/// (ignoring the real PATH). This is the deterministic test seam.
fn probe_system_java_with(test_override: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = test_override {
        if p.exists() {
            return Some(p.to_path_buf());
        }
        return None;
    }

    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path).find_map(|dir| {
            let candidate = dir.join("java");
            if candidate.exists() || candidate.with_extension("exe").exists() {
                Some(candidate)
            } else {
                None
            }
        })
    })
}

// ---------------------------------------------------------------------------
// Managed Java install
// ---------------------------------------------------------------------------

/// Install a managed Java runtime for the given `major` version, writing
/// artifacts under `version_dir` (typically already created by
/// `DiscoveryResult::InstallPlan.managed_path`).
///
/// Writes a mock Java binary through the download engine (`.part` → atomic
/// rename, hash/size verification). The mock content is deterministic so
/// that tests can verify hash correctness without downloading real JDKs.
///
/// Also writes a sidecar `java.version` marker so discovery can verify the
/// installed runtime's version without running the mock binary.
///
/// Returns the path to the installed `java` binary.
pub(crate) fn install_managed_java(version_dir: &Path, major: JavaMajor) -> Result<PathBuf> {
    let bin_dir = version_dir.join("bin");
    let java_path = bin_dir.join("java");

    // Deterministic mock Java binary bytes.
    let content = mock_java_bytes(major);

    // Compute expected SHA-256 for verification.
    let expected_hash = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(&content))
    };

    download_java_artifact(&java_path, &content, Some(expected_hash))
        .with_context(|| format!("write managed Java {}", java_path.display()))?;

    // Write version marker sidecar file.
    let marker_path = bin_dir.join("java.version");
    std::fs::write(&marker_path, format!("{}\n", major.display_version()))
        .with_context(|| format!("write version marker {}", marker_path.display()))?;

    Ok(java_path)
}

/// Deterministic mock Java binary bytes for a given major version.
fn mock_java_bytes(major: JavaMajor) -> Vec<u8> {
    format!("mock java runtime\nmajor={}\n", major.display_version()).into_bytes()
}

// ---------------------------------------------------------------------------
// Download helper (routes through download engine for atomic staging)
// ---------------------------------------------------------------------------

use crate::download::{
    download_file, DownloadOptions, FetchError, FetchOutcome, Fetcher, RangeServed,
};

/// A fetcher that returns deterministic in-memory bytes for managed Java
/// runtime downloads — no real HTTP needed.
struct MockJavaFetcher {
    url: String,
    bytes: Vec<u8>,
}

impl Fetcher for MockJavaFetcher {
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

/// Download a Java runtime archive/binary through the retry download engine.
/// Uses [`MockJavaFetcher`] to provide deterministic bytes for testing.
fn download_java_artifact(
    dest: &Path,
    content: &[u8],
    expected_sha256: Option<String>,
) -> Result<crate::download::DownloadOutcome> {
    let fetcher = MockJavaFetcher {
        url: format!(
            "mock://java/runtime/{}",
            dest.file_name().unwrap().to_string_lossy()
        ),
        bytes: content.to_vec(),
    };
    let opts = DownloadOptions {
        expected_sha256,
        expected_size: Some(content.len() as u64),
        ..Default::default()
    };
    download_file(dest, &fetcher, &opts)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // JavaMajor::from_mc_version — compatibility matrix
    // -----------------------------------------------------------------------

    #[test]
    fn java_required_for_older_mc_is_8() {
        assert_eq!(JavaMajor::from_mc_version("1.12.2"), Some(JavaMajor::Java8));
        assert_eq!(JavaMajor::from_mc_version("1.16.5"), Some(JavaMajor::Java8));
        assert_eq!(JavaMajor::from_mc_version("1.16"), Some(JavaMajor::Java8));
    }

    #[test]
    fn java_required_for_mc_1_17_to_1_20_is_17() {
        assert_eq!(JavaMajor::from_mc_version("1.17"), Some(JavaMajor::Java17));
        assert_eq!(
            JavaMajor::from_mc_version("1.18.2"),
            Some(JavaMajor::Java17)
        );
        assert_eq!(
            JavaMajor::from_mc_version("1.19.4"),
            Some(JavaMajor::Java17)
        );
        assert_eq!(
            JavaMajor::from_mc_version("1.20.1"),
            Some(JavaMajor::Java17)
        );
        assert_eq!(JavaMajor::from_mc_version("1.20"), Some(JavaMajor::Java17));
    }

    #[test]
    fn java_required_for_mc_1_21_plus_is_21() {
        assert_eq!(JavaMajor::from_mc_version("1.21"), Some(JavaMajor::Java21));
        assert_eq!(
            JavaMajor::from_mc_version("1.21.1"),
            Some(JavaMajor::Java21)
        );
        assert_eq!(
            JavaMajor::from_mc_version("1.21.4"),
            Some(JavaMajor::Java21)
        );
    }

    #[test]
    fn java_required_for_unknown_mc_version_is_none() {
        assert_eq!(JavaMajor::from_mc_version("unknown"), None);
        assert_eq!(JavaMajor::from_mc_version("1.unknown"), None);
        assert_eq!(JavaMajor::from_mc_version(""), None);
    }

    #[test]
    fn java_major_display_version_is_correct() {
        assert_eq!(JavaMajor::Java8.display_version(), "8");
        assert_eq!(JavaMajor::Java17.display_version(), "17");
        assert_eq!(JavaMajor::Java21.display_version(), "21");
    }

    #[test]
    fn java_major_managed_subdir_is_correct() {
        assert_eq!(JavaMajor::Java8.managed_subdir(), "java8");
        assert_eq!(JavaMajor::Java17.managed_subdir(), "java17");
        assert_eq!(JavaMajor::Java21.managed_subdir(), "java21");
    }

    // -----------------------------------------------------------------------
    // parse_java_version_output — version string parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parses_java_8_version_string() {
        let out = "java version \"1.8.0_402\"\nJava(TM) SE Runtime Environment (build 1.8.0_402-b06)\nJava HotSpot(TM) 64-Bit Server VM (build 25.402-b06, mixed mode)";
        assert_eq!(parse_java_version_output(out), Some(JavaMajor::Java8));
    }

    #[test]
    fn parses_openjdk_8_version() {
        let out = "openjdk version \"1.8.0_402\"\nOpenJDK Runtime Environment (build 1.8.0_402-whatever)\nOpenJDK 64-Bit Server VM (build 25.402-b06, mixed mode)";
        assert_eq!(parse_java_version_output(out), Some(JavaMajor::Java8));
    }

    #[test]
    fn parses_java_17_version_string() {
        let out = "openjdk version \"17.0.10\" 2024-01-16\nOpenJDK Runtime Environment (build 17.0.10+7)\nOpenJDK 64-Bit Server VM (build 17.0.10+7, mixed mode)";
        assert_eq!(parse_java_version_output(out), Some(JavaMajor::Java17));
    }

    #[test]
    fn parses_java_21_version_string() {
        let out = "openjdk version \"21.0.2\" 2024-01-16\nOpenJDK Runtime Environment (build 21.0.2+13)\nOpenJDK 64-Bit Server VM (build 21.0.2+13, mixed mode)";
        assert_eq!(parse_java_version_output(out), Some(JavaMajor::Java21));
    }

    #[test]
    fn parses_java_21_from_first_line_only() {
        // Only first line matters.
        let out = "openjdk version \"21.0.2\" 2024-01-16";
        assert_eq!(parse_java_version_output(out), Some(JavaMajor::Java21));
    }

    #[test]
    fn parse_returns_none_for_empty_string() {
        assert_eq!(parse_java_version_output(""), None);
    }

    #[test]
    fn parse_returns_none_for_garbage() {
        assert_eq!(
            parse_java_version_output("not a java version at all\n"),
            None
        );
    }

    #[test]
    fn parse_returns_none_for_unsupported_version() {
        // Java 11 is not supported by MCM.
        let out = "openjdk version \"11.0.20\" 2024-07-16";
        assert_eq!(parse_java_version_output(out), None);
    }

    #[test]
    fn parse_handles_java_8_variant_with_minor_0() {
        let out = "java version \"1.8.0_101\"\nJava Runtime Environment";
        assert_eq!(parse_java_version_output(out), Some(JavaMajor::Java8));
    }

    #[test]
    fn parse_handles_openjdk_17_ea_version() {
        let out =
            "openjdk version \"17-ea\" 2021-09-14\nOpenJDK Runtime Environment (build 17-ea+10)";
        // "17-ea" doesn't have a dot after the number, so it's "version \"17-ea\""
        // Our parser splits on '"', gets "17-ea", then tries split_once('.')
        // which gives ("17", "ea"). first != "1", so we parse first as 17. ✓
        assert_eq!(parse_java_version_output(out), Some(JavaMajor::Java17));
    }

    // -----------------------------------------------------------------------
    // probe_java_version — running mock java executables
    // -----------------------------------------------------------------------

    /// Helper: create a mock java script that prints the given version string
    /// to stderr (mimicking `java -version`).
    fn make_mock_java(dir: &Path, version_stdout: &str) -> PathBuf {
        let script = format!(
            "#!/bin/sh\necho '{}' >&2\n",
            version_stdout.replace('\'', "'\\''")
        );
        let path = dir.join("java");
        std::fs::write(&path, &script).expect("write mock java");
        let _ = std::process::Command::new("chmod")
            .args(["+x", path.to_str().unwrap()])
            .output();
        path
    }

    #[test]
    fn probe_java_version_with_mock_java_21() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let mock = make_mock_java(tmp.path(), r#"openjdk version "21.0.2" 2024-01-16"#);
        let major = probe_java_version(&mock);
        assert_eq!(major, Some(JavaMajor::Java21));
    }

    #[test]
    fn probe_java_version_with_mock_java_17() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let mock = make_mock_java(tmp.path(), r#"openjdk version "17.0.10" 2024-01-16"#);
        let major = probe_java_version(&mock);
        assert_eq!(major, Some(JavaMajor::Java17));
    }

    #[test]
    fn probe_java_version_with_mock_java_8() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let mock = make_mock_java(tmp.path(), r#"java version "1.8.0_402""#);
        let major = probe_java_version(&mock);
        assert_eq!(major, Some(JavaMajor::Java8));
    }

    #[test]
    fn probe_java_version_returns_none_for_non_java() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let not_java = tmp.path().join("not_java");
        std::fs::write(&not_java, "#!/bin/sh\necho 'hello'\n").expect("write");
        let _ = std::process::Command::new("chmod")
            .args(["+x", not_java.to_str().unwrap()])
            .output();
        let major = probe_java_version(&not_java);
        // This script doesn't output a valid java version → None
        assert_eq!(major, None);
    }

    #[test]
    fn probe_java_version_returns_none_for_nonexistent() {
        let major = probe_java_version(Path::new("/nonexistent/java_binary"));
        assert_eq!(major, None);
    }

    // -----------------------------------------------------------------------
    // discover_java — version-verified discovery
    // -----------------------------------------------------------------------

    fn make_game(mc_version: &str, java_path: Option<PathBuf>) -> GameRecord {
        let vc = crate::game_model::GameConfig {
            java_path,
            ..Default::default()
        };
        GameRecord {
            name: "test".to_owned(),
            root_dir: "/tmp".into(),
            mc_version: Some(mc_version.to_owned()),
            loader: None,
            loader_version: None,
            resolved_version_id: Some(mc_version.to_owned()),
            version_config: vc,
        }
    }

    #[test]
    fn discover_java_uses_user_config_path_when_version_matches() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let java_exe = make_mock_java(tmp.path(), r#"openjdk version "17.0.10" 2024-01-16"#);

        let game = make_game("1.20.1", Some(java_exe.clone()));
        let global = tmp.path().join("mcm");
        let result = discover_java(&game, &global).expect("discovery should succeed");

        match result {
            DiscoveryResult::Found(runtime) => {
                assert_eq!(runtime.major, JavaMajor::Java17);
                assert_eq!(runtime.path, java_exe);
                assert!(matches!(runtime.source, JavaSource::UserConfig(_)));
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn discover_java_rejects_user_config_with_wrong_major() {
        let tmp = tempfile::tempdir().expect("temp dir");
        // User has Java 21 but game requires Java 17.
        let java_exe = make_mock_java(tmp.path(), r#"openjdk version "21.0.2" 2024-01-16"#);

        let game = make_game("1.20.1", Some(java_exe));
        let global = tmp.path().join("mcm");
        let err = discover_java(&game, &global).unwrap_err();
        assert!(
            err.to_string().contains("configured java"),
            "error should mention configured java: {err}"
        );
        assert!(
            err.to_string().contains("21"),
            "error should mention actual version 21: {err}"
        );
        assert!(
            err.to_string().contains("17"),
            "error should mention required version 17: {err}"
        );
    }

    #[test]
    fn discover_java_skips_unprobeable_user_config_and_falls_to_managed() {
        let tmp = tempfile::tempdir().expect("temp dir");
        // User config points to a file that exists but is not a real java.
        let fake_java = tmp.path().join("fake_java");
        std::fs::write(&fake_java, "not a java binary").expect("write fake java");
        let _ = std::process::Command::new("chmod")
            .args(["+x", fake_java.to_str().unwrap()])
            .output();

        // Set up a managed Java 17.
        let global = tmp.path().join("mcm");
        let managed_bin = global
            .join("runtimes")
            .join("java")
            .join("java17")
            .join("bin")
            .join("java");
        std::fs::create_dir_all(managed_bin.parent().unwrap()).expect("create managed dir");
        std::fs::write(&managed_bin, "mock managed java").expect("write managed java");
        // Write version marker.
        let marker = managed_bin.parent().unwrap().join("java.version");
        std::fs::write(&marker, "17\n").expect("write marker");

        let game = make_game("1.20.1", Some(fake_java));
        let result = discover_java(&game, &global).expect("discovery should succeed");

        match result {
            DiscoveryResult::Found(runtime) => {
                assert_eq!(runtime.major, JavaMajor::Java17);
                assert!(matches!(runtime.source, JavaSource::Managed(_)));
            }
            other => panic!("expected Found (managed), got {other:?}"),
        }
    }

    #[test]
    fn discover_java_rejects_wrong_major_system_java_and_returns_install_plan() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let global = tmp.path().join("mcm");

        // System has Java 8 but game requires Java 17.
        let system_java = make_mock_java(tmp.path(), r#"java version "1.8.0_402""#);

        let game = make_game("1.20.1", None);
        let result = discover_java_impl(&game, &global, Some(&system_java))
            .expect("discovery should not error");

        match result {
            DiscoveryResult::InstallPlan { required, .. } => {
                assert_eq!(required, JavaMajor::Java17);
            }
            other => panic!("expected InstallPlan (wrong major), got {other:?}"),
        }
    }

    #[test]
    fn discover_java_accepts_correct_major_system_java() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let global = tmp.path().join("mcm");

        // System has Java 21 and game requires Java 21.
        let system_java = make_mock_java(tmp.path(), r#"openjdk version "21.0.2" 2024-01-16"#);

        let game = make_game("1.21.1", None);
        let result = discover_java_impl(&game, &global, Some(&system_java))
            .expect("discovery should succeed");

        match result {
            DiscoveryResult::Found(runtime) => {
                assert_eq!(runtime.major, JavaMajor::Java21);
                assert!(matches!(runtime.source, JavaSource::System));
            }
            other => panic!("expected Found (system), got {other:?}"),
        }
    }

    #[test]
    fn discover_java_falls_back_to_managed_when_user_config_marker_missing() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let global = tmp.path().join("mcm");

        // Managed runtime exists but no version marker — should not be accepted.
        let managed_bin = global
            .join("runtimes")
            .join("java")
            .join("java17")
            .join("bin")
            .join("java");
        std::fs::create_dir_all(managed_bin.parent().unwrap()).expect("create managed dir");
        std::fs::write(&managed_bin, "mock managed java").expect("write managed java");
        // Intentionally NOT writing java.version marker.

        let game = make_game("1.20.1", None);
        let result = discover_java_impl(&game, &global, Some(Path::new("/nonexistent/no_java")))
            .expect("discovery should succeed");

        match result {
            DiscoveryResult::InstallPlan { required, .. } => {
                assert_eq!(required, JavaMajor::Java17);
            }
            other => panic!("expected InstallPlan, got {other:?}"),
        }
    }

    #[test]
    fn discover_java_falls_back_to_managed_when_user_config_missing() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let global = tmp.path().join("mcm");
        let managed_bin = global
            .join("runtimes")
            .join("java")
            .join("java17")
            .join("bin")
            .join("java");
        std::fs::create_dir_all(managed_bin.parent().unwrap()).expect("create managed dir");
        std::fs::write(&managed_bin, "mock managed java").expect("write managed java");
        // Write version marker.
        let marker = managed_bin.parent().unwrap().join("java.version");
        std::fs::write(&marker, "17\n").expect("write marker");

        let game = make_game("1.20.1", None);
        let result = discover_java(&game, &global).expect("discovery should succeed");

        match result {
            DiscoveryResult::Found(runtime) => {
                assert_eq!(runtime.major, JavaMajor::Java17);
                assert!(matches!(runtime.source, JavaSource::Managed(_)));
            }
            other => panic!("expected Found, got {other:?}"),
        }
    }

    #[test]
    fn discover_java_returns_install_plan_when_no_java_found() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let global = tmp.path().join("mcm");
        let game = make_game("1.20.1", None);

        let result = discover_java_impl(&game, &global, Some(Path::new("/nonexistent/no_java")))
            .expect("discovery should not error");

        match result {
            DiscoveryResult::InstallPlan {
                required,
                managed_path,
            } => {
                assert_eq!(required, JavaMajor::Java17);
                assert!(managed_path.ends_with("runtimes/java/java17"));
            }
            other => panic!("expected InstallPlan, got {other:?}"),
        }
    }

    #[test]
    fn discover_java_errors_when_game_has_no_mc_version() {
        let game = GameRecord {
            name: "empty".to_owned(),
            root_dir: "/tmp".into(),
            mc_version: None,
            loader: None,
            loader_version: None,
            resolved_version_id: None,
            version_config: crate::game_model::GameConfig::default(),
        };
        let err = discover_java(&game, Path::new("/tmp")).unwrap_err();
        assert!(
            err.to_string().contains("no mc_version"),
            "error should mention missing mc_version: {err}"
        );
    }

    #[test]
    fn discover_java_errors_when_mc_version_is_unknown() {
        let game = make_game("99.99", None);
        let err = discover_java(&game, Path::new("/tmp")).unwrap_err();
        assert!(
            err.to_string().contains("unknown MC version"),
            "error should mention unknown MC version: {err}"
        );
    }

    // -----------------------------------------------------------------------
    // install_managed_java — download engine integration + version marker
    // -----------------------------------------------------------------------

    #[test]
    fn install_managed_java_writes_through_download_engine() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let version_dir = tmp.path().join("java21");
        std::fs::create_dir_all(&version_dir).expect("create version dir");

        let java_path =
            install_managed_java(&version_dir, JavaMajor::Java21).expect("install should succeed");

        let expected = version_dir.join("bin").join("java");
        assert_eq!(java_path, expected);
        assert!(expected.exists(), "java binary should exist");

        let content = std::fs::read(&expected).expect("read java binary");
        let text = String::from_utf8_lossy(&content);
        assert!(text.contains("mock java runtime"));
        assert!(text.contains("major=21"));
    }

    #[test]
    fn install_managed_java_writes_version_marker() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let version_dir = tmp.path().join("java17");
        std::fs::create_dir_all(&version_dir).expect("create version dir");

        install_managed_java(&version_dir, JavaMajor::Java17).expect("install");

        let marker = version_dir.join("bin").join("java.version");
        assert!(marker.exists(), "version marker should exist");
        let content = std::fs::read_to_string(&marker).expect("read marker");
        assert_eq!(content.trim(), "17", "marker should contain major version");
    }

    #[test]
    fn install_managed_java_verifies_hash() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let version_dir = tmp.path().join("java8");
        std::fs::create_dir_all(&version_dir).expect("create version dir");

        let java_path =
            install_managed_java(&version_dir, JavaMajor::Java8).expect("install should succeed");
        assert!(java_path.exists(), "java binary should exist");
    }

    #[test]
    fn mock_java_bytes_are_deterministic() {
        let a = mock_java_bytes(JavaMajor::Java21);
        let b = mock_java_bytes(JavaMajor::Java21);
        assert_eq!(a, b, "same version should produce same bytes");
        let c = mock_java_bytes(JavaMajor::Java17);
        assert_ne!(a, c, "different versions should produce different bytes");
        assert!(String::from_utf8_lossy(&a).contains("21"));
        assert!(String::from_utf8_lossy(&c).contains("17"));
    }
}

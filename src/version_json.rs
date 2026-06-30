//! Mojang version JSON types, parsing, platform filtering, and argument
//! interpolation.
//!
//! Handles the full Mojang launcher version manifest format: libraries with
//! OS/arch rules, argument templates with `${placeholder}` substitution,
//! asset index references, native classifiers, and download metadata.
//!
//! Platform detection targets Linux x86_64 as the first-class platform.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Mojang version JSON types
// ---------------------------------------------------------------------------

/// Top-level Mojang version JSON structure.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct VersionJson {
    #[allow(dead_code)]
    pub(crate) id: String,
    #[serde(default)]
    #[expect(
        dead_code,
        reason = "Version type (release/snapshot); read by callers for type checks"
    )]
    pub(crate) r#type: Option<String>,
    #[serde(default, rename = "mainClass")]
    pub(crate) main_class: Option<String>,
    /// Asset index ID (e.g. "12").
    #[serde(default)]
    pub(crate) assets: Option<String>,
    #[serde(default, rename = "assetIndex")]
    pub(crate) asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub(crate) libraries: Vec<Library>,
    #[serde(default)]
    pub(crate) arguments: Option<Arguments>,
    /// Top-level `downloads` block containing the client/server jar metadata.
    #[serde(default)]
    pub(crate) downloads: Option<VersionDownloads>,
}

/// Top-level `downloads` object in the Mojang version JSON.
///
/// Unlike library artifacts, the client/server entries here do NOT have a
/// `path` field — they only carry `sha1`, `size`, and `url`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct VersionDownloads {
    #[serde(default)]
    pub(crate) client: Option<VersionArtifact>,
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) server: Option<VersionArtifact>,
}

/// A top-level download artifact (client/server jar).
///
/// Same shape as [`LibraryArtifact`] minus the `path` field, since Mojang's
/// `downloads.client` doesn't carry one.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct VersionArtifact {
    #[serde(default)]
    pub(crate) sha1: Option<String>,
    #[serde(default)]
    pub(crate) size: Option<u64>,
    pub(crate) url: String,
}

/// Reference to the asset index JSON (URL + hash).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AssetIndexRef {
    #[allow(dead_code)]
    pub(crate) id: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) sha1: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) size: Option<u64>,
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) total_size: Option<u64>,
    pub(crate) url: String,
}

/// A library dependency entry.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Library {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) downloads: Option<LibraryDownloads>,
    /// OS/arch rules. If absent, the library is always included.
    #[serde(default)]
    pub(crate) rules: Option<Vec<Rule>>,
    /// Native classifier map: platform name → classifier suffix.
    #[serde(default)]
    pub(crate) natives: Option<BTreeMap<String, String>>,
}

/// Download info for a library.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LibraryDownloads {
    #[serde(default)]
    pub(crate) artifact: Option<LibraryArtifact>,
    #[serde(default)]
    pub(crate) classifiers: Option<BTreeMap<String, LibraryArtifact>>,
}

/// A single downloadable artifact.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LibraryArtifact {
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) sha1: Option<String>,
    #[serde(default)]
    pub(crate) size: Option<u64>,
    pub(crate) url: String,
}

/// An OS/arch rule.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Rule {
    pub(crate) action: String, // "allow" or "deny"
    #[serde(default)]
    pub(crate) os: Option<RuleOs>,
}

/// OS constraint within a rule.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RuleOs {
    pub(crate) name: String, // "linux", "osx", "windows"
}

/// JVM and game argument template lists.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Arguments {
    #[serde(default)]
    pub(crate) jvm: Vec<ArgEntry>,
    #[serde(default)]
    pub(crate) game: Vec<ArgEntry>,
}

/// A single argument entry: either a plain string or a conditional with rules.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum ArgEntry {
    Conditional { rules: Vec<Rule>, value: ArgValue },
    Simple(String),
}

/// Argument value: either a single string or a list.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum ArgValue {
    Single(String),
    Multiple(Vec<String>),
}

impl ArgValue {
    pub(crate) fn as_strings(&self) -> Vec<String> {
        match self {
            ArgValue::Single(s) => vec![s.clone()],
            ArgValue::Multiple(v) => v.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Asset index content types
// ---------------------------------------------------------------------------

/// Parsed asset index JSON (objects map).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AssetIndexContent {
    #[serde(default)]
    pub(crate) objects: BTreeMap<String, AssetObject>,
}

/// A single asset entry in the index.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AssetObject {
    pub(crate) hash: String,
    pub(crate) size: u64,
}

// ---------------------------------------------------------------------------
// Platform detection
// ---------------------------------------------------------------------------

/// Current platform for library/native rule matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Platform {
    pub(crate) name: &'static str, // "linux", "osx", "windows"
}

/// Detect the current platform. Returns `None` for unsupported platforms.
pub(crate) fn current_platform() -> Option<Platform> {
    match std::env::consts::OS {
        "linux" => Some(Platform { name: "linux" }),
        "macos" => Some(Platform { name: "osx" }),
        "windows" => Some(Platform { name: "windows" }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Library filtering and classpath
// ---------------------------------------------------------------------------

/// Check if a library's rules allow it for the given platform.
/// If no rules are present, the library is always allowed.
pub(crate) fn library_applies(library: &Library, platform: Platform) -> bool {
    let Some(rules) = &library.rules else {
        return true;
    };
    if rules.is_empty() {
        return true;
    }
    // Rules are evaluated in order. Default action is "deny" if no rule matches.
    let mut allowed = false;
    for rule in rules {
        let os_matches = rule.os.as_ref().is_none_or(|os| os.name == platform.name);
        if os_matches {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

/// Filter libraries for the current platform, returning artifact paths
/// relative to `libraries_root`.
pub(crate) fn filter_library_artifacts(
    libraries: &[Library],
    platform: Platform,
    libraries_root: &Path,
) -> Vec<PathBuf> {
    libraries
        .iter()
        .filter(|lib| library_applies(lib, platform))
        .filter_map(|lib| {
            lib.downloads
                .as_ref()
                .and_then(|d| d.artifact.as_ref())
                .map(|a| libraries_root.join(&a.path))
        })
        .collect()
}

/// Build classpath from filtered libraries + game client jar.
pub(crate) fn build_classpath(
    libraries: &[Library],
    version_dir: &Path,
    libraries_root: &Path,
    mc_version: &str,
    platform: Platform,
) -> Vec<PathBuf> {
    let mut cp = filter_library_artifacts(libraries, platform, libraries_root);
    let game_jar = version_dir.join(format!("{mc_version}.jar"));
    cp.insert(0, game_jar);
    cp
}

/// Get the natives directory path for a game version.
pub(crate) fn natives_directory(version_dir: &Path) -> PathBuf {
    version_dir.join("natives")
}

// ---------------------------------------------------------------------------
// Native extraction
// ---------------------------------------------------------------------------

/// Identify native jar paths for the current platform.
///
/// Libraries with a `natives` map have platform-specific classifier jars
/// (e.g. `natives-linux`) that must be extracted to the natives directory.
pub(crate) fn native_jar_paths(
    libraries: &[Library],
    libraries_dir: &Path,
    platform: Platform,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for lib in libraries {
        if !library_applies(lib, platform) {
            continue;
        }
        let Some(natives) = &lib.natives else {
            continue;
        };
        let Some(classifier_suffix) = natives.get(platform.name) else {
            continue;
        };
        // The classifier jar path replaces the classifier suffix in the
        // artifact path pattern: {base}-{classifier}.jar
        if let Some(downloads) = &lib.downloads {
            if let Some(classifiers) = &downloads.classifiers {
                if let Some(artifact) = classifiers.get(classifier_suffix) {
                    paths.push(libraries_dir.join(&artifact.path));
                }
            }
        }
    }
    paths
}

// ---------------------------------------------------------------------------
// Argument interpolation
// ---------------------------------------------------------------------------

/// Variable map for argument template substitution.
pub(crate) type VarMap = BTreeMap<String, String>;

/// Interpolate argument entries, replacing `${key}` placeholders and
/// filtering conditional entries by platform rules.
pub(crate) fn interpolate_args(
    entries: &[ArgEntry],
    vars: &VarMap,
    platform: Platform,
) -> Vec<String> {
    let mut result = Vec::new();
    for entry in entries {
        match entry {
            ArgEntry::Simple(s) => {
                result.push(interpolate_string(s, vars));
            }
            ArgEntry::Conditional { rules, value } => {
                // Minecraft launcher rule semantics: default deny, rules
                // evaluated in order, last matching rule wins.
                let mut allowed = false;
                for rule in rules {
                    let os_matches = rule.os.as_ref().is_none_or(|os| os.name == platform.name);
                    if os_matches {
                        allowed = rule.action == "allow";
                    }
                }
                if allowed {
                    for v in value.as_strings() {
                        result.push(interpolate_string(&v, vars));
                    }
                }
            }
        }
    }
    result
}

/// Replace `${key}` placeholders in a string.
fn interpolate_string(template: &str, vars: &VarMap) -> String {
    let mut result = template.to_owned();
    for (key, value) in vars {
        let placeholder = format!("${{{key}}}");
        result = result.replace(&placeholder, value);
    }
    result
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse a Mojang version JSON file from disk.
pub(crate) fn parse_version_json(path: &Path) -> Result<VersionJson> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("read version JSON: {}", path.display()))?;
    parse_version_json_str(&content)
}

/// Parse a Mojang version JSON from a string.
pub(crate) fn parse_version_json_str(content: &str) -> Result<VersionJson> {
    serde_json::from_str(content).context("parse version JSON")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_version_json() -> String {
        serde_json::json!({
            "id": "1.21.1",
            "type": "release",
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
                    "name": "net.minecraft:client:merged",
                    "downloads": {
                        "artifact": {
                            "path": "net/minecraft/client/1.21.1/client-1.21.1.jar",
                            "sha1": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
                            "size": 25000000,
                            "url": "https://libraries.minecraft.net/net/minecraft/client/1.21.1/client-1.21.1.jar"
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
                            "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar"
                        },
                        "classifiers": {
                            "natives-linux": {
                                "path": "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar",
                                "sha1": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                                "size": 500000,
                                "url": "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar"
                            }
                        }
                    },
                    "natives": {"linux": "natives-linux", "windows": "natives-windows"}
                },
                {
                    "name": "com.example:windows-only:1.0",
                    "rules": [{"action": "allow", "os": {"name": "windows"}}],
                    "downloads": {
                        "artifact": {
                            "path": "com/example/windows-only/1.0/windows-only-1.0.jar",
                            "sha1": "1111111111111111111111111111111111111111",
                            "size": 100,
                            "url": "https://example.com/windows-only-1.0.jar"
                        }
                    }
                }
            ],
            "arguments": {
                "jvm": [
                    "-Djava.library.path=${natives_directory}",
                    "-Dminecraft.launcher.brand=${launcher_name}",
                    "-cp",
                    "${classpath}"
                ],
                "game": [
                    "--username", "${auth_player_name}",
                    "--version", "${version_name}",
                    "--gameDir", "${game_directory}",
                    "--assetsDir", "${assets_root}",
                    "--accessToken", "${auth_access_token}",
                    "--uuid", "${auth_uuid}",
                    "--userType", "${auth_user_type}",
                    "--versionType", "${version_type}"
                ]
            }
        })
        .to_string()
    }

    #[test]
    fn parse_version_json_roundtrip() {
        let vj = parse_version_json_str(&sample_version_json()).expect("parse");
        assert_eq!(vj.id, "1.21.1");
        assert_eq!(vj.assets.as_deref(), Some("12"));
        assert_eq!(vj.libraries.len(), 3);
        assert!(vj.asset_index.is_some());
        let args = vj.arguments.as_ref().expect("args");
        assert_eq!(args.jvm.len(), 4);
        assert_eq!(args.game.len(), 16);
    }

    #[test]
    fn library_no_rules_always_applies() {
        let lib = Library {
            name: "test:lib:1.0".into(),
            downloads: None,
            rules: None,
            natives: None,
        };
        assert!(library_applies(&lib, Platform { name: "linux" }));
    }

    #[test]
    fn library_allow_linux_applies_on_linux() {
        let lib = Library {
            name: "test:lib:1.0".into(),
            downloads: None,
            rules: Some(vec![Rule {
                action: "allow".into(),
                os: Some(RuleOs {
                    name: "linux".into(),
                }),
            }]),
            natives: None,
        };
        assert!(library_applies(&lib, Platform { name: "linux" }));
        assert!(!library_applies(&lib, Platform { name: "windows" }));
    }

    #[test]
    fn library_allow_windows_not_applied_on_linux() {
        let vj = parse_version_json_str(&sample_version_json()).expect("parse");
        let linux = Platform { name: "linux" };
        let artifacts = filter_library_artifacts(&vj.libraries, linux, Path::new("/libs"));
        // windows-only library should be excluded
        assert!(
            !artifacts
                .iter()
                .any(|p| p.to_string_lossy().contains("windows-only")),
            "windows-only lib should be filtered out: {artifacts:?}"
        );
        // linux library and no-rules library should be included
        assert_eq!(artifacts.len(), 2);
    }

    #[test]
    fn build_classpath_includes_game_jar_and_libraries() {
        let vj = parse_version_json_str(&sample_version_json()).expect("parse");
        let tmp = tempfile::tempdir().expect("temp dir");
        let version_dir = tmp.path().join("versions").join("1.21.1");
        let cp = build_classpath(
            &vj.libraries,
            &version_dir,
            tmp.path(),
            "1.21.1",
            Platform { name: "linux" },
        );
        // Game jar + 2 filtered libraries (lwjgl + client merged)
        assert_eq!(cp.len(), 3);
        assert!(cp[0].to_string_lossy().contains("1.21.1.jar"));
    }

    #[test]
    fn interpolate_replaces_placeholders() {
        let mut vars = VarMap::new();
        vars.insert("auth_player_name".into(), "TestUser".into());
        vars.insert("version_name".into(), "1.21.1".into());
        let result = interpolate_string("${auth_player_name} --version ${version_name}", &vars);
        assert_eq!(result, "TestUser --version 1.21.1");
    }

    #[test]
    fn interpolate_args_filters_conditional() {
        let entries = vec![
            ArgEntry::Simple("-Djava.library.path=${natives_directory}".into()),
            ArgEntry::Conditional {
                rules: vec![Rule {
                    action: "allow".into(),
                    os: Some(RuleOs {
                        name: "windows".into(),
                    }),
                }],
                value: ArgValue::Single("-XX:HeapDumpPath=MojsStudios.hprof".into()),
            },
        ];
        let mut vars = VarMap::new();
        vars.insert("natives_directory".into(), "/tmp/natives".into());
        let linux = Platform { name: "linux" };
        let result = interpolate_args(&entries, &vars, linux);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "-Djava.library.path=/tmp/natives");
    }

    #[test]
    fn native_jar_paths_extracts_platform_natives() {
        let vj = parse_version_json_str(&sample_version_json()).expect("parse");
        let tmp = tempfile::tempdir().expect("temp dir");
        let linux = Platform { name: "linux" };
        let paths = native_jar_paths(&vj.libraries, tmp.path(), linux);
        assert_eq!(paths.len(), 1);
        assert!(
            paths[0].to_string_lossy().contains("natives-linux"),
            "should find natives-linux jar: {:?}",
            paths[0]
        );
    }

    #[test]
    fn current_platform_is_linux_or_known() {
        let p = current_platform();
        assert!(p.is_some(), "current platform should be supported");
    }

    #[test]
    fn parse_empty_libraries() {
        let json = r#"{"id":"test","libraries":[]}"#;
        let vj = parse_version_json_str(json).expect("parse");
        assert!(vj.libraries.is_empty());
    }

    #[test]
    fn arg_entry_deserializes_simple_and_conditional() {
        let json = r#"[
            "-Djava.library.path=${natives_directory}",
            {"rules": [{"action": "allow", "os": {"name": "linux"}}], "value": "-Xspecial"}
        ]"#;
        let entries: Vec<ArgEntry> = serde_json::from_str(json).expect("parse args");
        assert_eq!(entries.len(), 2);
        assert!(matches!(&entries[0], ArgEntry::Simple(_)));
        assert!(matches!(&entries[1], ArgEntry::Conditional { .. }));
    }
}

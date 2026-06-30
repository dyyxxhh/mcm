//! Mojang-style version manifest types and mock data for tests.
//!
//! Two manifest types:
//! - [`VersionManifest`] — vanilla Minecraft version list (Mojang format).
//! - [`LoaderVersions`] — for a single loader, maps MC version → available loader versions.
//!
//! Mock helpers provide deterministic test data without real Mojang network calls.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Mirrors the Mojang version manifest v2 `latest` object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LatestVersions {
    pub(crate) release: String,
    pub(crate) snapshot: String,
}

/// A single entry in the Mojang version manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VersionEntry {
    pub(crate) id: String,
    #[serde(rename = "type")]
    pub(crate) version_type: VersionType,
    pub(crate) url: String,
    pub(crate) time: String,
    #[serde(rename = "releaseTime")]
    pub(crate) release_time: String,
}

/// Version release type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VersionType {
    Release,
    Snapshot,
    #[serde(rename = "old_beta")]
    OldBeta,
    #[serde(rename = "old_alpha")]
    OldAlpha,
}

/// Mojang-style version manifest: `latest` pointers + `versions` list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct VersionManifest {
    pub(crate) latest: LatestVersions,
    pub(crate) versions: Vec<VersionEntry>,
}

/// A single loader version for a specific MC version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct LoaderEntry {
    /// Loader version string (e.g. `"0.16.0"` for Fabric).
    pub(crate) version: String,
    /// Whether this is a stable/production build.
    pub(crate) stable: bool,
}

/// Loader version mapping: MC version → available loader versions.
#[derive(Debug, Clone)]
pub(crate) struct LoaderVersions {
    pub(crate) entries: BTreeMap<String, Vec<LoaderEntry>>,
}

impl LoaderVersions {
    pub(crate) fn latest_stable(&self, mc_version: &str) -> Option<&str> {
        self.entries
            .get(mc_version)?
            .iter()
            .rfind(|e| e.stable)
            .map(|e| e.version.as_str())
    }

    pub(crate) fn has_version(&self, mc_version: &str, loader_version: &str) -> bool {
        self.entries
            .get(mc_version)
            .map(|v| v.iter().any(|e| e.version == loader_version))
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Mock manifest data for tests
// ---------------------------------------------------------------------------

/// Build a deterministic VersionManifest for tests.
pub(crate) fn mock_version_manifest() -> VersionManifest {
    VersionManifest {
        latest: LatestVersions {
            release: "1.21.1".to_owned(),
            snapshot: "1.21.2-pre1".to_owned(),
        },
        versions: vec![
            VersionEntry {
                id: "1.21.1".to_owned(),
                version_type: VersionType::Release,
                url: "https://mock-mojang.test/v1/1.21.1.json".to_owned(),
                time: "2024-09-01T00:00:00+00:00".to_owned(),
                release_time: "2024-09-01T00:00:00+00:00".to_owned(),
            },
            VersionEntry {
                id: "1.21".to_owned(),
                version_type: VersionType::Release,
                url: "https://mock-mojang.test/v1/1.21.json".to_owned(),
                time: "2024-06-15T00:00:00+00:00".to_owned(),
                release_time: "2024-06-15T00:00:00+00:00".to_owned(),
            },
            VersionEntry {
                id: "1.20.4".to_owned(),
                version_type: VersionType::Release,
                url: "https://mock-mojang.test/v1/1.20.4.json".to_owned(),
                time: "2024-02-01T00:00:00+00:00".to_owned(),
                release_time: "2024-02-01T00:00:00+00:00".to_owned(),
            },
            VersionEntry {
                id: "1.20.1".to_owned(),
                version_type: VersionType::Release,
                url: "https://mock-mojang.test/v1/1.20.1.json".to_owned(),
                time: "2023-09-15T00:00:00+00:00".to_owned(),
                release_time: "2023-09-15T00:00:00+00:00".to_owned(),
            },
            VersionEntry {
                id: "1.19.4".to_owned(),
                version_type: VersionType::Release,
                url: "https://mock-mojang.test/v1/1.19.4.json".to_owned(),
                time: "2023-04-01T00:00:00+00:00".to_owned(),
                release_time: "2023-04-01T00:00:00+00:00".to_owned(),
            },
            VersionEntry {
                id: "1.21.2-pre1".to_owned(),
                version_type: VersionType::Snapshot,
                url: "https://mock-mojang.test/v1/1.21.2-pre1.json".to_owned(),
                time: "2024-10-01T00:00:00+00:00".to_owned(),
                release_time: "2024-10-01T00:00:00+00:00".to_owned(),
            },
        ],
    }
}

/// Build a mock Fabric loader manifest for tests.
pub(crate) fn mock_fabric_versions() -> LoaderVersions {
    LoaderVersions {
        entries: BTreeMap::from([
            (
                "1.21.1".to_owned(),
                vec![
                    LoaderEntry {
                        version: "0.15.0".to_owned(),
                        stable: true,
                    },
                    LoaderEntry {
                        version: "0.16.0".to_owned(),
                        stable: true,
                    },
                ],
            ),
            (
                "1.21".to_owned(),
                vec![LoaderEntry {
                    version: "0.15.0".to_owned(),
                    stable: true,
                }],
            ),
            (
                "1.20.4".to_owned(),
                vec![
                    LoaderEntry {
                        version: "0.14.0".to_owned(),
                        stable: true,
                    },
                    LoaderEntry {
                        version: "0.15.0".to_owned(),
                        stable: true,
                    },
                ],
            ),
            (
                "1.20.1".to_owned(),
                vec![
                    LoaderEntry {
                        version: "0.14.0".to_owned(),
                        stable: true,
                    },
                    LoaderEntry {
                        version: "0.14.23".to_owned(),
                        stable: true,
                    },
                ],
            ),
        ]),
    }
}

/// Build a mock Forge loader manifest for tests.
pub(crate) fn mock_forge_versions() -> LoaderVersions {
    LoaderVersions {
        entries: BTreeMap::from([
            (
                "1.21.1".to_owned(),
                vec![LoaderEntry {
                    version: "52.0.0".to_owned(),
                    stable: true,
                }],
            ),
            (
                "1.20.4".to_owned(),
                vec![LoaderEntry {
                    version: "49.0.0".to_owned(),
                    stable: true,
                }],
            ),
            (
                "1.20.1".to_owned(),
                vec![
                    LoaderEntry {
                        version: "47.1.0".to_owned(),
                        stable: true,
                    },
                    LoaderEntry {
                        version: "47.3.0".to_owned(),
                        stable: true,
                    },
                ],
            ),
        ]),
    }
}

/// Build a mock NeoForge loader manifest for tests.
pub(crate) fn mock_neoforge_versions() -> LoaderVersions {
    LoaderVersions {
        entries: BTreeMap::from([
            (
                "1.21.1".to_owned(),
                vec![
                    LoaderEntry {
                        version: "21.1.0".to_owned(),
                        stable: true,
                    },
                    LoaderEntry {
                        version: "21.1.172".to_owned(),
                        stable: true,
                    },
                ],
            ),
            (
                "1.21".to_owned(),
                vec![LoaderEntry {
                    version: "21.0.0".to_owned(),
                    stable: true,
                }],
            ),
            (
                "1.20.4".to_owned(),
                vec![LoaderEntry {
                    version: "20.4.0".to_owned(),
                    stable: true,
                }],
            ),
        ]),
    }
}

/// Build a mock Quilt loader manifest for tests.
pub(crate) fn mock_quilt_versions() -> LoaderVersions {
    LoaderVersions {
        entries: BTreeMap::from([
            (
                "1.21.1".to_owned(),
                vec![
                    LoaderEntry {
                        version: "0.26.0".to_owned(),
                        stable: true,
                    },
                    LoaderEntry {
                        version: "0.27.0".to_owned(),
                        stable: true,
                    },
                ],
            ),
            (
                "1.20.1".to_owned(),
                vec![LoaderEntry {
                    version: "0.25.0".to_owned(),
                    stable: true,
                }],
            ),
        ]),
    }
}

// ---------------------------------------------------------------------------
// HTTP API DTOs for real manifest fetching
// ---------------------------------------------------------------------------

/// Fabric meta API response entry: one game version with its loader versions.
/// Endpoint: `https://meta.fabricmc.net/v2/versions/loader`
#[derive(Debug, Deserialize)]
pub(crate) struct FabricGameVersion {
    #[serde(rename = "gameVersion")]
    pub(crate) game_version: String,
    pub(crate) loader: Vec<FabricLoaderEntry>,
}

/// A single Fabric loader entry within a game version.
#[derive(Debug, Deserialize)]
pub(crate) struct FabricLoaderEntry {
    pub(crate) version: String,
    pub(crate) stable: bool,
}

/// Forge/NeoForge promotions_slim.json response.
/// Maps `"MC_VERSION-latest"` or `"MC_VERSION-recommended"` → loader version.
#[derive(Debug, Deserialize)]
pub(crate) struct PromosSlim {
    pub(crate) promos: std::collections::BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------
// Unit tests for manifest helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_manifest_has_realistic_versions() {
        let vm = mock_version_manifest();
        assert_eq!(vm.latest.release, "1.21.1");
        assert!(vm.versions.iter().any(|v| v.id == "1.21.1"));
        assert!(vm.versions.iter().any(|v| v.id == "1.20.1"));
        assert_eq!(
            vm.versions
                .iter()
                .find(|v| v.id == "1.21.2-pre1")
                .unwrap()
                .version_type,
            VersionType::Snapshot
        );
    }

    #[test]
    fn loader_latest_stable_returns_newest_stable() {
        let fabric = mock_fabric_versions();
        assert_eq!(fabric.latest_stable("1.21.1"), Some("0.16.0"));
        // Non-existent MC version returns None.
        assert_eq!(fabric.latest_stable("1.99"), None);
    }

    #[test]
    fn loader_has_version_checks_exact_version() {
        let neoforge = mock_neoforge_versions();
        assert!(neoforge.has_version("1.21.1", "21.1.172"));
        assert!(!neoforge.has_version("1.21.1", "99.99.99"));
        assert!(!neoforge.has_version("1.99", "21.1.0"));
    }

    #[test]
    fn forge_version_list_includes_expected_versions() {
        let forge = mock_forge_versions();
        assert!(forge.has_version("1.20.1", "47.3.0"));
        assert!(forge.has_version("1.20.1", "47.1.0"));
        assert!(!forge.has_version("1.21", "does-not-exist"));
    }

    #[test]
    fn quilt_version_list_includes_expected_versions() {
        let quilt = mock_quilt_versions();
        assert!(quilt.has_version("1.21.1", "0.27.0"));
    }
}

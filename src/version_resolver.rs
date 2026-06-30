//! Resolve a [`McTarget`] (smart target) into concrete Minecraft and loader
//! versions by consulting version/loader manifests.
//!
//! Resolution rules:
//! - `mc` → latest release from the version manifest.
//! - `mc1.21.1` → specific release (validated against manifest).
//! - `mc-neoforge` → latest release + latest stable NeoForge for that version.
//! - `mc1.21.1-neoforge` → specific MC + latest stable NeoForge.
//! - `mc1.21.1-neoforge-21.1.172` → specific MC + specific loader version.
//! - Same rules for fabric/forge/quilt.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};

use crate::mc_target::{Loader, McTarget};
use crate::version_manifest::{LoaderVersions, VersionManifest};

/// A fully resolved install target with concrete version strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedTarget {
    pub(crate) mc_version: String,
    pub(crate) loader: Option<Loader>,
    pub(crate) loader_version: Option<String>,
}

/// Resolve an [`McTarget`] using the provided manifests.
///
/// `loader_manifests` maps each [`Loader`] to its version index. An empty map
/// means no loaders are available (vanilla-only resolution works).
pub(crate) fn resolve_target(
    target: McTarget,
    mc_manifest: &VersionManifest,
    loader_manifests: &BTreeMap<Loader, LoaderVersions>,
) -> Result<ResolvedTarget> {
    match target {
        McTarget::Vanilla { mc_version: None } => {
            let id = mc_manifest.latest.release.clone();
            ensure_version_exists(mc_manifest, &id)?;
            Ok(ResolvedTarget {
                mc_version: id,
                loader: None,
                loader_version: None,
            })
        }
        McTarget::Vanilla {
            mc_version: Some(ref ver),
        } => {
            ensure_version_exists(mc_manifest, ver)?;
            Ok(ResolvedTarget {
                mc_version: ver.clone(),
                loader: None,
                loader_version: None,
            })
        }
        McTarget::WithLoader {
            mc_version,
            loader,
            loader_version,
        } => {
            let mc_ver = match mc_version {
                None => mc_manifest.latest.release.clone(),
                Some(v) => v,
            };
            ensure_version_exists(mc_manifest, &mc_ver)?;

            let versions = loader_manifests
                .get(&loader)
                .with_context(|| format!("{} loader versions not available", loader.as_str()))?;

            let lv = match loader_version {
                None => {
                    let latest = versions.latest_stable(&mc_ver).with_context(|| {
                        format!(
                            "no compatible {} version found for Minecraft {mc_ver}",
                            loader.as_str()
                        )
                    })?;
                    latest.to_owned()
                }
                Some(ref ver) => {
                    if !versions.has_version(&mc_ver, ver) {
                        // Check if the version exists at all (for better error messages)
                        bail!(
                            "{} version {ver} is not available for Minecraft {mc_ver}",
                            loader.as_str()
                        );
                    }
                    ver.clone()
                }
            };

            Ok(ResolvedTarget {
                mc_version: mc_ver,
                loader: Some(loader),
                loader_version: Some(lv),
            })
        }
    }
}

fn ensure_version_exists(manifest: &VersionManifest, version: &str) -> Result<()> {
    if !manifest.versions.iter().any(|v| v.id == version) {
        bail!("unknown Minecraft version: {version}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version_manifest::{
        mock_fabric_versions, mock_forge_versions, mock_neoforge_versions, mock_quilt_versions,
        mock_version_manifest,
    };

    fn mock_loader_manifests() -> BTreeMap<Loader, LoaderVersions> {
        BTreeMap::from([
            (Loader::Fabric, mock_fabric_versions()),
            (Loader::Forge, mock_forge_versions()),
            (Loader::NeoForge, mock_neoforge_versions()),
            (Loader::Quilt, mock_quilt_versions()),
        ])
    }

    #[test]
    fn resolve_latest_vanilla() {
        let vm = mock_version_manifest();
        let loaders = BTreeMap::new();
        let resolved =
            resolve_target(McTarget::Vanilla { mc_version: None }, &vm, &loaders).expect("resolve");
        assert_eq!(resolved.mc_version, "1.21.1");
        assert_eq!(resolved.loader, None);
    }

    #[test]
    fn resolve_specific_vanilla() {
        let vm = mock_version_manifest();
        let loaders = BTreeMap::new();
        let resolved = resolve_target(
            McTarget::Vanilla {
                mc_version: Some("1.20.1".into()),
            },
            &vm,
            &loaders,
        )
        .expect("resolve");
        assert_eq!(resolved.mc_version, "1.20.1");
        assert_eq!(resolved.loader, None);
    }

    #[test]
    fn resolve_unknown_vanilla_errors() {
        let vm = mock_version_manifest();
        let loaders = BTreeMap::new();
        let err = resolve_target(
            McTarget::Vanilla {
                mc_version: Some("1.99".into()),
            },
            &vm,
            &loaders,
        )
        .unwrap_err();
        assert!(err.to_string().contains("unknown Minecraft version"));
    }

    #[test]
    fn resolve_neoforge_latest_mc_latest_loader() {
        let vm = mock_version_manifest();
        let loaders = mock_loader_manifests();
        let resolved = resolve_target(
            McTarget::WithLoader {
                mc_version: None,
                loader: Loader::NeoForge,
                loader_version: None,
            },
            &vm,
            &loaders,
        )
        .expect("resolve");
        assert_eq!(resolved.mc_version, "1.21.1");
        assert_eq!(resolved.loader, Some(Loader::NeoForge));
        assert_eq!(resolved.loader_version.as_deref(), Some("21.1.172"));
    }

    #[test]
    fn resolve_neoforge_specific_mc_latest_loader() {
        let vm = mock_version_manifest();
        let loaders = mock_loader_manifests();
        let resolved = resolve_target(
            McTarget::WithLoader {
                mc_version: Some("1.20.4".into()),
                loader: Loader::NeoForge,
                loader_version: None,
            },
            &vm,
            &loaders,
        )
        .expect("resolve");
        assert_eq!(resolved.mc_version, "1.20.4");
        assert_eq!(resolved.loader, Some(Loader::NeoForge));
        assert_eq!(resolved.loader_version.as_deref(), Some("20.4.0"));
    }

    #[test]
    fn resolve_neoforge_exact_pinned_loader() {
        let vm = mock_version_manifest();
        let loaders = mock_loader_manifests();
        let resolved = resolve_target(
            McTarget::WithLoader {
                mc_version: Some("1.21.1".into()),
                loader: Loader::NeoForge,
                loader_version: Some("21.1.172".into()),
            },
            &vm,
            &loaders,
        )
        .expect("resolve");
        assert_eq!(resolved.mc_version, "1.21.1");
        assert_eq!(resolved.loader_version.as_deref(), Some("21.1.172"));
    }

    #[test]
    fn resolve_fabric_latest_mc_latest_loader() {
        let vm = mock_version_manifest();
        let loaders = mock_loader_manifests();
        let resolved = resolve_target(
            McTarget::WithLoader {
                mc_version: None,
                loader: Loader::Fabric,
                loader_version: None,
            },
            &vm,
            &loaders,
        )
        .expect("resolve");
        assert_eq!(resolved.mc_version, "1.21.1");
        assert_eq!(resolved.loader, Some(Loader::Fabric));
        assert_eq!(resolved.loader_version.as_deref(), Some("0.16.0"));
    }

    #[test]
    fn resolve_fabric_specific_mc_pinned_loader() {
        let vm = mock_version_manifest();
        let loaders = mock_loader_manifests();
        let resolved = resolve_target(
            McTarget::WithLoader {
                mc_version: Some("1.20.1".into()),
                loader: Loader::Fabric,
                loader_version: Some("0.14.23".into()),
            },
            &vm,
            &loaders,
        )
        .expect("resolve");
        assert_eq!(resolved.mc_version, "1.20.1");
        assert_eq!(resolved.loader_version.as_deref(), Some("0.14.23"));
    }

    #[test]
    fn resolve_forge_exact_pinned() {
        let vm = mock_version_manifest();
        let loaders = mock_loader_manifests();
        let resolved = resolve_target(
            McTarget::WithLoader {
                mc_version: Some("1.20.1".into()),
                loader: Loader::Forge,
                loader_version: Some("47.3.0".into()),
            },
            &vm,
            &loaders,
        )
        .expect("resolve");
        assert_eq!(resolved.mc_version, "1.20.1");
        assert_eq!(resolved.loader_version.as_deref(), Some("47.3.0"));
    }

    #[test]
    fn resolve_quilt_latest_mc_latest() {
        let vm = mock_version_manifest();
        let loaders = mock_loader_manifests();
        let resolved = resolve_target(
            McTarget::WithLoader {
                mc_version: Some("1.21.1".into()),
                loader: Loader::Quilt,
                loader_version: None,
            },
            &vm,
            &loaders,
        )
        .expect("resolve");
        assert_eq!(resolved.mc_version, "1.21.1");
        assert_eq!(resolved.loader_version.as_deref(), Some("0.27.0"));
    }

    #[test]
    fn resolve_incompatible_loader_errors() {
        let vm = mock_version_manifest();
        let loaders = mock_loader_manifests();
        let err = resolve_target(
            McTarget::WithLoader {
                mc_version: Some("1.19.4".into()),
                loader: Loader::NeoForge,
                loader_version: None,
            },
            &vm,
            &loaders,
        )
        .unwrap_err();
        assert!(err.to_string().contains("no compatible"));
    }

    #[test]
    fn resolve_nonexistent_loader_version_errors() {
        let vm = mock_version_manifest();
        let loaders = mock_loader_manifests();
        let err = resolve_target(
            McTarget::WithLoader {
                mc_version: Some("1.21.1".into()),
                loader: Loader::NeoForge,
                loader_version: Some("99.99.99".into()),
            },
            &vm,
            &loaders,
        )
        .unwrap_err();
        assert!(err.to_string().contains("not available"));
    }
}

//! Loader jar URL computation and download for game installs.
//!
//! Each loader (Fabric, Quilt, NeoForge, Forge) exposes its jar at a
//! well-known Maven coordinate. This module computes the URL given the
//! loader type and version, so [`game_install`] can fetch real loader jars
//! through the existing [`crate::download::HttpFetcher`] + [`download_file`]
//! engine (with SHA-1/size validation) instead of writing mock bytes.
//!
//! URL conventions:
//! - **Fabric**    — `https://maven.fabricmc.net/net/fabricmc/fabric-loader/{v}/fabric-loader-{v}.jar`
//! - **Quilt**     — `https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-loader/{v}/quilt-loader-{v}.jar`
//! - **NeoForge**  — `https://maven.neoforged.net/releases/net/neoforged/neoforge/{v}/neoforge-{v}-universal.jar`
//!   (newer NeoForge versions ship `neoforge-{v}.jar`; we try the universal
//!   variant first since it is consistent across releases.)
//! - **Forge**     — `https://maven.minecraftforge.net/net/minecraftforge/forge/{mc}-{v}/forge-{mc}-{v}-universal.jar`
//!   (Forge uses the `mc_version-forge_version` directory layout.)
//!
//! Hashes/size for loaders are not part of the manifest fetch in
//! [`HttpGameManifestSource`](crate::game_install::HttpGameManifestSource) —
//! we pass `None` and rely on Maven's HTTP-level integrity (the URL is
//! HTTPS; if the file is corrupted the JVM will refuse to load it at
//! launch time).

use anyhow::{anyhow, Result};

use crate::mc_target::Loader;

/// Compute the canonical loader jar download URL.
///
/// `mc_version` is only used by Forge (which embeds it in the path); the
/// other loaders' URLs are loader-version-only.
pub(crate) fn loader_jar_url(
    loader: Loader,
    loader_version: &str,
    mc_version: &str,
) -> Result<String> {
    match loader {
        Loader::Fabric => Ok(format!(
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/{v}/fabric-loader-{v}.jar",
            v = loader_version
        )),
        Loader::Quilt => Ok(format!(
            "https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-loader/{v}/quilt-loader-{v}.jar",
            v = loader_version
        )),
        Loader::NeoForge => Ok(format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{v}/neoforge-{v}-universal.jar",
            v = loader_version
        )),
        Loader::Forge => {
            if mc_version.is_empty() {
                return Err(anyhow!(
                    "forge loader URL requires the Minecraft version \
                     (e.g. 1.20.1-47.3.0); got empty mc_version"
                ));
            }
            Ok(format!(
                "https://maven.minecraftforge.net/net/minecraftforge/forge/{mc}-{v}/forge-{mc}-{v}-universal.jar",
                mc = mc_version,
                v = loader_version
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fabric_url_is_maven_fabricmc() {
        let url = loader_jar_url(Loader::Fabric, "0.16.0", "1.21.1").unwrap();
        assert_eq!(
            url,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.0/fabric-loader-0.16.0.jar"
        );
    }

    #[test]
    fn quilt_url_is_maven_quiltmc() {
        let url = loader_jar_url(Loader::Quilt, "0.27.0", "1.21.1").unwrap();
        assert_eq!(
            url,
            "https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-loader/0.27.0/quilt-loader-0.27.0.jar"
        );
    }

    #[test]
    fn neoforge_url_is_maven_neoforged_universal() {
        let url = loader_jar_url(Loader::NeoForge, "21.1.172", "1.21.1").unwrap();
        assert_eq!(
            url,
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.172/neoforge-21.1.172-universal.jar"
        );
    }

    #[test]
    fn forge_url_embeds_mc_version_in_path() {
        let url = loader_jar_url(Loader::Forge, "47.3.0", "1.20.1").unwrap();
        assert_eq!(
            url,
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.3.0/forge-1.20.1-47.3.0-universal.jar"
        );
    }

    #[test]
    fn forge_url_errors_without_mc_version() {
        let err = loader_jar_url(Loader::Forge, "47.3.0", "").unwrap_err();
        assert!(
            err.to_string().contains("requires the Minecraft version"),
            "missing mc_version should error: {err}"
        );
    }

    #[test]
    fn fabric_url_ignores_mc_version() {
        let a = loader_jar_url(Loader::Fabric, "0.16.0", "1.21.1").unwrap();
        let b = loader_jar_url(Loader::Fabric, "0.16.0", "1.20.1").unwrap();
        assert_eq!(a, b, "Fabric URL should be mc-version-independent");
    }
}

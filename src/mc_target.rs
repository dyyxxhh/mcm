//! Minecraft install target parser for `game install` smart targets.
//!
//! Grammar: `mc[<mc-version>][-<loader>[-<loader-version>]]`
//! - `mc` → latest vanilla MC
//! - `mc1.21.1` → vanilla MC 1.21.1
//! - `mc-neoforge` → latest MC + latest NeoForge
//! - `mc1.21.1-neoforge` → MC 1.21.1 + latest NeoForge
//! - `mc1.21.1-neoforge-21.1.172` → MC 1.21.1 + NeoForge 21.1.172
//! - Fabric/Forge/NeoForge/Quilt all use the same grammar.
//! - `@latest` is rejected; omission already means latest.

use std::fmt;

/// Supported mod loaders for smart install targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Loader {
    Fabric,
    Forge,
    NeoForge,
    Quilt,
}

impl Loader {
    pub fn as_str(self) -> &'static str {
        match self {
            Loader::Fabric => "fabric",
            Loader::Forge => "forge",
            Loader::NeoForge => "neoforge",
            Loader::Quilt => "quilt",
        }
    }
}

impl fmt::Display for Loader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Parsed Minecraft install target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McTarget {
    /// Vanilla Minecraft, optionally a specific version (`None` = latest).
    Vanilla { mc_version: Option<String> },
    /// Minecraft with a mod loader.
    WithLoader {
        mc_version: Option<String>,
        loader: Loader,
        loader_version: Option<String>,
    },
}

const LOADERS: &[(&str, Loader)] = &[
    ("fabric", Loader::Fabric),
    ("forge", Loader::Forge),
    ("neoforge", Loader::NeoForge),
    ("quilt", Loader::Quilt),
];

fn parse_loader(name: &str) -> Option<Loader> {
    LOADERS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, l)| *l)
}

/// Parse a `game install` smart target string into a typed [`McTarget`].
///
/// See module docs for the grammar. Returns `Err` for malformed targets,
/// including any target containing `@` (e.g. `@latest`).
pub fn parse_mc_target(target: &str) -> Result<McTarget, String> {
    if target.contains('@') {
        return Err(format!(
            "invalid target '{target}': '@latest' is not supported; omit the version to mean latest"
        ));
    }
    let Some(rest) = target.strip_prefix("mc") else {
        return Err(format!("invalid target '{target}': must start with 'mc'"));
    };
    if rest.is_empty() {
        return Ok(McTarget::Vanilla { mc_version: None });
    }

    // Determine MC version and the remainder (loader part).
    let (mc_version, loader_part) = if let Some(stripped) = rest.strip_prefix('-') {
        // `mc-<loader>...` — latest MC.
        (None, stripped)
    } else if rest.starts_with(char::is_numeric) {
        // `mc<version>...` — split at first `-`.
        match rest.split_once('-') {
            Some((ver, remainder)) => (Some(ver.to_owned()), remainder),
            None => (Some(rest.to_owned()), ""),
        }
    } else {
        return Err(format!(
            "invalid target '{target}': expected a version (digit) or '-<loader>' after 'mc'"
        ));
    };

    if loader_part.is_empty() {
        return Ok(McTarget::Vanilla { mc_version });
    }

    let (loader_name, loader_version) = match loader_part.split_once('-') {
        Some((name, ver)) => (name, Some(ver.to_owned())),
        None => (loader_part, None),
    };

    let loader = parse_loader(loader_name).ok_or_else(|| {
        format!(
            "invalid target '{target}': unknown loader '{loader_name}' (expected fabric, forge, neoforge, or quilt)"
        )
    })?;

    Ok(McTarget::WithLoader {
        mc_version,
        loader,
        loader_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mc_alone_means_latest_vanilla() {
        assert_eq!(
            parse_mc_target("mc").unwrap(),
            McTarget::Vanilla { mc_version: None }
        );
    }

    #[test]
    fn mc_with_version_means_specific_vanilla() {
        assert_eq!(
            parse_mc_target("mc1.21.1").unwrap(),
            McTarget::Vanilla {
                mc_version: Some("1.21.1".into())
            }
        );
    }

    #[test]
    fn mc_loader_means_latest_mc_latest_loader() {
        for (name, expected) in LOADERS {
            let target = format!("mc-{name}");
            assert_eq!(
                parse_mc_target(&target).unwrap(),
                McTarget::WithLoader {
                    mc_version: None,
                    loader: *expected,
                    loader_version: None,
                }
            );
        }
    }

    #[test]
    fn mc_version_loader_means_specific_mc_latest_loader() {
        assert_eq!(
            parse_mc_target("mc1.21.1-neoforge").unwrap(),
            McTarget::WithLoader {
                mc_version: Some("1.21.1".into()),
                loader: Loader::NeoForge,
                loader_version: None,
            }
        );
    }

    #[test]
    fn mc_version_loader_version_means_exact() {
        assert_eq!(
            parse_mc_target("mc1.21.1-neoforge-21.1.172").unwrap(),
            McTarget::WithLoader {
                mc_version: Some("1.21.1".into()),
                loader: Loader::NeoForge,
                loader_version: Some("21.1.172".into()),
            }
        );
    }

    #[test]
    fn at_latest_suffix_is_rejected() {
        assert!(parse_mc_target("mc1.21.1-neoforge@latest").is_err());
        assert!(parse_mc_target("mc@latest").is_err());
    }

    #[test]
    fn non_mc_prefix_is_rejected() {
        assert!(parse_mc_target("sodium").is_err());
        assert!(parse_mc_target("1.21.1").is_err());
    }

    #[test]
    fn unknown_loader_is_rejected() {
        assert!(parse_mc_target("mc-badloader").is_err());
    }

    #[test]
    fn loader_case_insensitive() {
        assert_eq!(
            parse_mc_target("mc-NeoForge").unwrap(),
            McTarget::WithLoader {
                mc_version: None,
                loader: Loader::NeoForge,
                loader_version: None,
            }
        );
    }
}

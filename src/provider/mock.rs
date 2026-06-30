use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};

use crate::config::{Profile, Side};
use crate::provider::{
    Artifact, Candidate, Dependency, DependencyKind, Project, Provider, ReleaseKind,
};

pub(crate) struct MockProvider {
    projects: BTreeMap<String, Project>,
}

impl MockProvider {
    pub(crate) fn new() -> Self {
        let mut projects = BTreeMap::new();
        for project in mock_projects() {
            projects.insert(project.logical_id.clone(), project);
        }
        Self { projects }
    }
}

impl Provider for MockProvider {
    fn search(&self, query: &str, profile: &Profile) -> Result<Vec<Project>> {
        let query = query.to_lowercase();
        Ok(self
            .projects
            .values()
            .filter(|project| {
                project.logical_id.contains(&query) || project.title.to_lowercase().contains(&query)
            })
            .map(|project| filter_project(project, profile))
            .collect())
    }

    fn get(&self, query: &str, profile: &Profile) -> Result<Project> {
        self.projects
            .get(query)
            .map(|project| filter_project(project, profile))
            .with_context(|| format!("mod {query} not found"))
    }

    fn download(&self, artifact: &Artifact) -> Result<Vec<u8>> {
        if artifact.download_url.is_none() {
            bail!("missing download URL for {}", artifact.file_id);
        }
        Ok(mock_jar_bytes(&artifact.file_id, &artifact.version))
    }
}

fn filter_project(project: &Project, profile: &Profile) -> Project {
    let mut project = project.clone();
    for candidate in &mut project.candidates {
        candidate.artifacts.retain(|artifact| {
            artifact.release == ReleaseKind::Stable
                && artifact
                    .mc_versions
                    .iter()
                    .any(|version| version == &profile.mc_version)
                && artifact
                    .loaders
                    .iter()
                    .any(|loader| loader == &profile.loader)
        });
    }
    project
        .candidates
        .retain(|candidate| !candidate.artifacts.is_empty());
    project
}

pub(crate) fn mock_jar_bytes(id: &str, version: &str) -> Vec<u8> {
    format!("mock mcm jar\nid={id}\nversion={version}\n").into_bytes()
}

pub(crate) fn artifact(
    file_id: &str,
    version: &str,
    filename: &str,
    download_url: Option<&str>,
    deps: Vec<Dependency>,
) -> Artifact {
    Artifact {
        file_id: file_id.into(),
        version: version.into(),
        release: ReleaseKind::Stable,
        mc_versions: vec!["1.20.1".into()],
        loaders: vec!["fabric".into()],
        side: Side::Both,
        filename: filename.into(),
        download_url: download_url.map(str::to_owned),
        sha256: None,
        download_count: None,
        deps,
        owner_id: None,
    }
}

pub(crate) fn artifact_beta(file_id: &str, version: &str, filename: &str) -> Artifact {
    Artifact {
        file_id: file_id.into(),
        version: version.into(),
        release: ReleaseKind::Beta,
        mc_versions: vec!["1.20.1".into()],
        loaders: vec!["fabric".into()],
        side: Side::Both,
        filename: filename.into(),
        download_url: Some("https://cdn.modrinth.com/mock/beta".into()),
        sha256: None,
        download_count: None,
        deps: vec![],
        owner_id: None,
    }
}

pub(crate) fn artifact_alpha(file_id: &str, version: &str, filename: &str) -> Artifact {
    Artifact {
        file_id: file_id.into(),
        version: version.into(),
        release: ReleaseKind::Alpha,
        mc_versions: vec!["1.20.1".into()],
        loaders: vec!["fabric".into()],
        side: Side::Both,
        filename: filename.into(),
        download_url: Some("https://cdn.modrinth.com/mock/alpha".into()),
        sha256: None,
        download_count: None,
        deps: vec![],
        owner_id: None,
    }
}

pub(crate) fn artifact_with_owner(
    file_id: &str,
    version: &str,
    filename: &str,
    download_url: Option<&str>,
    deps: Vec<Dependency>,
    owner_id: &str,
) -> Artifact {
    let mut a = artifact(file_id, version, filename, download_url, deps);
    a.owner_id = Some(owner_id.into());
    a
}

pub(crate) fn dep(logical_id: &str, kind: DependencyKind) -> Dependency {
    Dependency {
        logical_id: logical_id.into(),
        kind,
    }
}

// SIZE_OK: pure deterministic test-fixture data table. Each entry is a
// hand-crafted `Project` used by the offline mock provider. Moving it to
// a separate file would not reduce cognitive complexity — it is data, not
// logic. Grows linearly with the mock catalog; kept inline so the mock
// provider stays self-contained.
fn mock_projects() -> Vec<Project> {
    vec![
        Project {
            logical_id: "rootmod".into(),
            title: "Root Mod".into(),
            description: "A root mod with required and optional dependencies".into(),
            candidates: vec![
                Candidate {
                    provider: "mock".into(),
                    project_id: "rootmod".into(),
                    artifacts: vec![
                        artifact_with_owner(
                            "rootmod-file",
                            "1.0.0",
                            "rootmod-1.0.0.jar",
                            Some("https://cdn.modrinth.com/mock/rootmod"),
                            vec![
                                dep("depmod", DependencyKind::Required),
                                dep("optionalmod", DependencyKind::Optional),
                                dep("embeddedlib", DependencyKind::Embedded),
                                dep("badmod", DependencyKind::Incompatible),
                                dep("mysterymod", DependencyKind::Unknown),
                            ],
                            "test-owner",
                        ),
                        artifact_beta("rootmod-beta-file", "2.0.0-beta", "rootmod-2.0.0-beta.jar"),
                        artifact_alpha(
                            "rootmod-alpha-file",
                            "3.0.0-alpha",
                            "rootmod-3.0.0-alpha.jar",
                        ),
                    ],
                },
                Candidate {
                    provider: "modrinth".into(),
                    project_id: "rootmod".into(),
                    artifacts: vec![artifact(
                        "mr-rootmod-file",
                        "1.0.0",
                        "rootmod-1.0.0.jar",
                        Some("https://cdn.modrinth.com/mock/rootmod-mr"),
                        vec![],
                    )],
                },
            ],
        },
        Project {
            logical_id: "depmod".into(),
            title: "Dependency Mod".into(),
            description: "Required dependency".into(),
            candidates: vec![Candidate {
                provider: "mock".into(),
                project_id: "depmod".into(),
                artifacts: vec![artifact(
                    "depmod-file",
                    "1.0.0",
                    "depmod-1.0.0.jar",
                    Some("https://cdn.modrinth.com/mock/depmod"),
                    vec![],
                )],
            }],
        },
        Project {
            logical_id: "optionalmod".into(),
            title: "Optional Mod".into(),
            description: "Optional dependency".into(),
            candidates: vec![Candidate {
                provider: "mock".into(),
                project_id: "optionalmod".into(),
                artifacts: vec![artifact(
                    "optionalmod-file",
                    "1.0.0",
                    "optionalmod-1.0.0.jar",
                    Some("https://cdn.modrinth.com/mock/optionalmod"),
                    vec![],
                )],
            }],
        },
        Project {
            logical_id: "standalone".into(),
            title: "Standalone".into(),
            description: "A standalone mod".into(),
            candidates: vec![Candidate {
                provider: "mock".into(),
                project_id: "standalone".into(),
                artifacts: vec![artifact(
                    "standalone-file",
                    "1.0.0",
                    "standalone-1.0.0.jar",
                    Some("https://cdn.modrinth.com/mock/standalone"),
                    vec![],
                )],
            }],
        },
        Project {
            logical_id: "badmod".into(),
            title: "Bad Mod".into(),
            description: "An incompatible mod".into(),
            candidates: vec![Candidate {
                provider: "mock".into(),
                project_id: "badmod".into(),
                artifacts: vec![artifact(
                    "badmod-file",
                    "1.0.0",
                    "badmod-1.0.0.jar",
                    Some("https://cdn.modrinth.com/mock/badmod"),
                    vec![],
                )],
            }],
        },
        Project {
            logical_id: "brokenmod".into(),
            title: "Broken Mod".into(),
            description: "A mod with missing download URL".into(),
            candidates: vec![Candidate {
                provider: "mock".into(),
                project_id: "brokenmod".into(),
                artifacts: vec![artifact(
                    "brokenmod-file",
                    "1.0.0",
                    "brokenmod-1.0.0.jar",
                    None,
                    vec![],
                )],
            }],
        },
    ]
}

#[cfg(test)]
pub(crate) mod test_helpers {
    pub(crate) use super::{artifact, dep};

    pub(crate) fn test_profile() -> crate::config::Profile {
        crate::config::Profile {
            name: "test".to_owned(),
            mods_dir: std::path::PathBuf::from("mods"),
            mc_version: "1.20.1".to_owned(),
            loader: "fabric".to_owned(),
            side: crate::config::Side::Both,
        }
    }
}

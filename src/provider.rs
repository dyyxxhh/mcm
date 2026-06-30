use std::collections::BTreeMap;

use anyhow::Result;

use crate::config::Profile;

pub(crate) mod composite;
pub(crate) mod curseforge;
pub(crate) mod curseforge_dto;
pub(crate) mod mock;
pub(crate) mod modrinth;
pub(crate) mod source;

pub(crate) use composite::CompositeProvider;
pub(crate) use curseforge::CurseForgeProvider;
pub(crate) use mock::MockProvider;
pub(crate) use modrinth::ModrinthProvider;
#[allow(unused_imports)]
pub(crate) use source::SourceProvider;

pub(crate) trait Provider {
    fn search(&self, query: &str, profile: &Profile) -> Result<Vec<Project>>;
    fn get(&self, query: &str, profile: &Profile) -> Result<Project>;
    fn download(&self, artifact: &Artifact) -> Result<Vec<u8>>;
}

#[derive(Clone, Debug)]
pub(crate) struct Project {
    pub(crate) logical_id: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) candidates: Vec<Candidate>,
}

#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub(crate) provider: String,
    pub(crate) project_id: String,
    pub(crate) artifacts: Vec<Artifact>,
}

#[derive(Clone, Debug)]
pub(crate) struct Artifact {
    pub(crate) file_id: String,
    pub(crate) version: String,
    pub(crate) release: ReleaseKind,
    pub(crate) mc_versions: Vec<String>,
    pub(crate) loaders: Vec<String>,
    pub(crate) side: crate::config::Side,
    pub(crate) filename: String,
    pub(crate) download_url: Option<String>,
    pub(crate) sha256: Option<String>,
    pub(crate) download_count: Option<u64>,
    pub(crate) deps: Vec<Dependency>,
    /// Package owner's user ID (for upgrade owner-mismatch checks).
    pub(crate) owner_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReleaseKind {
    Stable,
    Beta,
    Alpha,
}

#[derive(Clone, Debug)]
pub(crate) struct Dependency {
    pub(crate) logical_id: String,
    pub(crate) kind: DependencyKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DependencyKind {
    Required,
    Optional,
    Embedded,
    Incompatible,
    Unknown,
}

pub(crate) fn group_projects(projects: Vec<Project>) -> Vec<Project> {
    let mut grouped: BTreeMap<String, Project> = BTreeMap::new();
    for project in projects {
        grouped
            .entry(project.logical_id.clone())
            .and_modify(|existing| existing.candidates.extend(project.candidates.clone()))
            .or_insert(project);
    }
    grouped.into_values().collect()
}

pub(crate) fn candidate_summary(candidates: &[Candidate]) -> String {
    candidates
        .iter()
        .map(|candidate| format!("{}/{}", candidate.provider, candidate.project_id))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn effective_downloads(source_weight: f64, raw_download_count: Option<u64>) -> f64 {
    source_weight * (raw_download_count.unwrap_or(0).max(1) as f64)
}

#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub(crate) installs: Vec<PlannedInstall>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct PlannedInstall {
    pub(crate) logical_id: String,
    pub(crate) candidate: Candidate,
    pub(crate) artifact: Artifact,
    pub(crate) reason: crate::lock::InstallReason,
    pub(crate) required_deps: Vec<String>,
}

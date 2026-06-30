// SIZE_OK: non-test source is ~230 LOC; the rest is the `#[cfg(test)] mod
// tests` block (JSON mapping regression test) which is test fixture and
// stays with the code it exercises.
use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::{Profile, Side};
use crate::provider::{
    Artifact, Candidate, Dependency, DependencyKind, Project, Provider, ReleaseKind,
};

const MODRINTH_API_BASE: &str = "https://api.modrinth.com/v2";

pub(crate) struct ModrinthProvider {
    pub(crate) client: reqwest::blocking::Client,
    base_url: String,
}

impl ModrinthProvider {
    pub(crate) fn new() -> Self {
        Self::with_base_url(MODRINTH_API_BASE)
    }

    pub(crate) fn with_base_url(base_url: &str) -> Self {
        Self {
            client: reqwest::blocking::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("Modrinth HTTP client"),
            base_url: base_url.trim_end_matches('/').to_owned(),
        }
    }

    fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        Ok(self
            .client
            .get(url)
            .header("User-Agent", "mcm/0.1.0 (Minecraft mod manager)")
            .query(params)
            .send()?
            .error_for_status()?
            .json()?)
    }

    fn versions_for(&self, project_id: &str, profile: &Profile) -> Result<Vec<ModrinthVersion>> {
        self.get_json(
            &format!("/project/{project_id}/version"),
            &[
                (
                    "loaders",
                    serde_json::to_string(&vec![profile.loader.as_str()])?,
                ),
                (
                    "game_versions",
                    serde_json::to_string(&vec![profile.mc_version.as_str()])?,
                ),
                ("include_changelog", "false".to_owned()),
            ],
        )
    }
}

impl Provider for ModrinthProvider {
    fn search(&self, query: &str, profile: &Profile) -> Result<Vec<Project>> {
        let facets = serde_json::to_string(&vec![
            vec!["project_type:mod".to_owned()],
            vec![format!("versions:{}", profile.mc_version)],
            vec![format!("categories:{}", profile.loader)],
        ])?;
        let response: ModrinthSearchResponse = self.get_json(
            "/search",
            &[
                ("query", query.to_owned()),
                ("facets", facets),
                ("limit", "20".to_owned()),
                ("index", "relevance".to_owned()),
            ],
        )?;
        response
            .hits
            .into_iter()
            .map(|hit| {
                let versions = self.versions_for(&hit.project_id, profile)?;
                Ok(modrinth_project_from_parts(hit.into(), versions))
            })
            .collect()
    }

    fn get(&self, query: &str, profile: &Profile) -> Result<Project> {
        let project: ModrinthProject = self.get_json(&format!("/project/{query}"), &[])?;
        let id = project.id.clone();
        let versions = self.versions_for(&id, profile)?;
        Ok(modrinth_project_from_parts(project, versions))
    }

    fn download(&self, artifact: &Artifact) -> Result<Vec<u8>> {
        let url = artifact
            .download_url
            .as_deref()
            .context("missing download URL")?;
        let response = self
            .client
            .get(url)
            .header("User-Agent", "mcm/0.1.0 (Minecraft mod manager)")
            .send()?
            .error_for_status()?;
        Ok(response.bytes()?.to_vec())
    }
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchResponse {
    hits: Vec<ModrinthProjectHit>,
}

#[derive(Debug, Deserialize)]
struct ModrinthProjectHit {
    slug: Option<String>,
    project_id: String,
    title: String,
    description: String,
    client_side: Option<String>,
    server_side: Option<String>,
}

impl From<ModrinthProjectHit> for ModrinthProject {
    fn from(hit: ModrinthProjectHit) -> Self {
        Self {
            id: hit.project_id,
            slug: hit.slug,
            title: hit.title,
            description: hit.description,
            client_side: hit.client_side,
            server_side: hit.server_side,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ModrinthProject {
    id: String,
    slug: Option<String>,
    title: String,
    description: String,
    client_side: Option<String>,
    server_side: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    id: String,
    version_number: String,
    version_type: String,
    game_versions: Vec<String>,
    loaders: Vec<String>,
    files: Vec<ModrinthFile>,
    #[serde(default)]
    dependencies: Vec<ModrinthDependency>,
}

#[derive(Debug, Deserialize)]
struct ModrinthFile {
    #[serde(default)]
    hashes: BTreeMap<String, String>,
    url: Option<String>,
    filename: String,
    primary: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ModrinthDependency {
    project_id: Option<String>,
    dependency_type: String,
}

fn modrinth_project_from_parts(
    project: ModrinthProject,
    versions: Vec<ModrinthVersion>,
) -> Project {
    let logical_id = project.slug.clone().unwrap_or_else(|| project.id.clone());
    let side = side_from_provider_support(
        project.client_side.as_deref(),
        project.server_side.as_deref(),
    );
    let artifacts = versions
        .into_iter()
        .filter_map(|version| modrinth_artifact_from_version(version, side))
        .collect();
    Project {
        logical_id,
        title: project.title,
        description: project.description,
        candidates: vec![Candidate {
            provider: "modrinth".to_owned(),
            project_id: project.id,
            artifacts,
        }],
    }
}

fn modrinth_artifact_from_version(version: ModrinthVersion, side: Side) -> Option<Artifact> {
    let file = version
        .files
        .iter()
        .find(|file| file.primary.unwrap_or(false))
        .or_else(|| version.files.first())?;
    Some(Artifact {
        file_id: version.id,
        version: version.version_number,
        release: release_from_modrinth(&version.version_type),
        mc_versions: version.game_versions,
        loaders: version.loaders,
        side,
        filename: file.filename.clone(),
        download_url: file.url.clone(),
        sha256: file.hashes.get("sha256").cloned(),
        download_count: None,
        deps: version
            .dependencies
            .into_iter()
            .filter_map(|dep| {
                dep.project_id.map(|logical_id| Dependency {
                    logical_id,
                    kind: dependency_from_modrinth(&dep.dependency_type),
                })
            })
            .collect(),
        owner_id: None,
    })
}

fn release_from_modrinth(value: &str) -> ReleaseKind {
    match value {
        "release" => ReleaseKind::Stable,
        "beta" => ReleaseKind::Beta,
        "alpha" => ReleaseKind::Alpha,
        _ => ReleaseKind::Alpha,
    }
}

fn dependency_from_modrinth(value: &str) -> DependencyKind {
    match value {
        "required" => DependencyKind::Required,
        "optional" => DependencyKind::Optional,
        "embedded" => DependencyKind::Embedded,
        "incompatible" => DependencyKind::Incompatible,
        _ => DependencyKind::Unknown,
    }
}

fn side_from_provider_support(client: Option<&str>, server: Option<&str>) -> Side {
    match (client, server) {
        (Some("required" | "optional"), Some("unsupported")) => Side::Client,
        (Some("unsupported"), Some("required" | "optional")) => Side::Server,
        _ => Side::Both,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::mock::test_helpers::test_profile;
    use std::collections::BTreeMap;

    #[test]
    fn modrinth_json_mapping_preserves_project_artifacts_dependencies_and_release_types() {
        let project_json = r#"{
            "id":"AABBCCDD",
            "slug":"logical-mod",
            "title":"Logical Mod",
            "description":"Mapped from Modrinth",
            "client_side":"required",
            "server_side":"optional"
        }"#;
        let versions_json = r#"[
            {
                "id":"version-release",
                "version_number":"1.0.0",
                "version_type":"release",
                "game_versions":["1.20.1"],
                "loaders":["fabric"],
                "files":[{"hashes":{"sha512":"abc123","sha1":"abc"},"url":"https://cdn.example/mod.jar","filename":"logical-mod.jar","primary":true}],
                "dependencies":[
                    {"project_id":"required-dep","dependency_type":"required"},
                    {"project_id":"optional-dep","dependency_type":"optional"},
                    {"project_id":"embedded-dep","dependency_type":"embedded"},
                    {"project_id":"bad-dep","dependency_type":"incompatible"}
                ]
            },
            {
                "id":"version-beta",
                "version_number":"2.0.0-beta",
                "version_type":"beta",
                "game_versions":["1.20.1"],
                "loaders":["fabric"],
                "files":[{"hashes":{},"url":"https://cdn.example/beta.jar","filename":"beta.jar","primary":true}],
                "dependencies":[]
            }
        ]"#;
        let project: ModrinthProject = serde_json::from_str(project_json).expect("project json");
        let versions: Vec<ModrinthVersion> =
            serde_json::from_str(versions_json).expect("versions json");
        let mapped = modrinth_project_from_parts(project, versions);

        assert_eq!(mapped.logical_id, "logical-mod");
        assert_eq!(mapped.candidates[0].provider, "modrinth");
        assert_eq!(mapped.candidates[0].project_id, "AABBCCDD");
        assert_eq!(mapped.candidates[0].artifacts.len(), 2);
        let release = mapped.candidates[0]
            .artifacts
            .iter()
            .find(|artifact| artifact.file_id == "version-release")
            .expect("release artifact");
        assert_eq!(release.release, ReleaseKind::Stable);
        assert_eq!(release.sha256, None);
        assert_eq!(
            release.download_url.as_deref(),
            Some("https://cdn.example/mod.jar")
        );
        assert_eq!(release.deps[0].kind, DependencyKind::Required);
        assert_eq!(release.deps[1].kind, DependencyKind::Optional);
        assert_eq!(release.deps[2].kind, DependencyKind::Embedded);
        assert_eq!(release.deps[3].kind, DependencyKind::Incompatible);
        let selected = crate::install::select_artifact(&mapped, &test_profile(), &BTreeMap::new())
            .expect("stable compatible selection");
        assert_eq!(selected.file_id, "version-release");
    }
}

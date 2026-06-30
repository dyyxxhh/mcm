// SIZE_OK: non-test source is ~220 LOC; the rest is the `#[cfg(test)] mod
// tests` block (select_artifact / build_plan / composite regression tests)
// which is test fixture and stays with the code it exercises.
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::config::{Profile, Side};
use crate::download::{download_file, DownloadOptions, ProviderFetcher};
use crate::i18n;
use crate::lock::{InstallReason, LockState};
use crate::provider::{
    effective_downloads, group_projects, Artifact, DependencyKind, Plan, PlannedInstall, Project,
    Provider,
};

pub(crate) fn search_install_roots(
    provider: &dyn Provider,
    profile: &Profile,
    roots: &[String],
) -> Result<Vec<String>> {
    let lang = i18n::Lang::default();
    let mut selected = Vec::new();
    for query in roots {
        let mut results = group_projects(provider.search(query, profile)?);
        if results.is_empty() {
            bail!("{}", i18n::mod_not_found_by_search(lang, query));
        }
        let project = results.remove(0);
        println!(
            "{}",
            i18n::selected_from_search(lang, &project.logical_id, query)
        );
        selected.push(project.logical_id);
    }
    Ok(selected)
}

pub(crate) fn deps_by_kind(artifact: &Artifact, kind: DependencyKind) -> Vec<String> {
    artifact
        .deps
        .iter()
        .filter(|dep| dep.kind == kind)
        .map(|dep| dep.logical_id.clone())
        .collect()
}

pub(crate) fn build_plan(
    provider: &dyn Provider,
    profile: &Profile,
    roots: &[String],
    lock: &LockState,
    source_weights: &BTreeMap<String, f64>,
) -> Result<Plan> {
    let lang = i18n::Lang::default();
    let mut planned: BTreeMap<String, PlannedInstall> = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut queue: VecDeque<(String, InstallReason)> = roots
        .iter()
        .cloned()
        .map(|root| (root, InstallReason::Manual))
        .collect();
    while let Some((query, reason)) = queue.pop_front() {
        let project = provider.get(&query, profile)?;
        let logical_id = project.logical_id.clone();
        if planned.contains_key(&logical_id) {
            if reason == InstallReason::Manual {
                if let Some(existing) = planned.get_mut(&logical_id) {
                    existing.reason = InstallReason::Manual;
                }
            }
            continue;
        }
        if let Some(existing) = lock.installed.get(&logical_id) {
            if existing.reason == InstallReason::Auto && reason == InstallReason::Manual {
                let mut candidate = project
                    .candidates
                    .first()
                    .cloned()
                    .with_context(|| i18n::project_has_no_candidates(lang))?;
                let artifact = select_artifact(&project, profile, source_weights)?;
                candidate.artifacts = vec![artifact.clone()];
                planned.insert(
                    logical_id.clone(),
                    PlannedInstall {
                        logical_id,
                        candidate,
                        artifact,
                        reason,
                        required_deps: Vec::new(),
                    },
                );
            }
            continue;
        }
        let artifact = select_artifact(&project, profile, source_weights)?;
        let candidate = project
            .candidates
            .iter()
            .find(|candidate| {
                candidate
                    .artifacts
                    .iter()
                    .any(|artifact_item| artifact_item.file_id == artifact.file_id)
            })
            .cloned()
            .or_else(|| project.candidates.first().cloned())
            .with_context(|| i18n::project_has_no_candidates(lang))?;
        let mut required_deps = Vec::new();
        for dep in &artifact.deps {
            match dep.kind {
                DependencyKind::Required => {
                    let dep_project = provider.get(&dep.logical_id, profile)?;
                    required_deps.push(dep_project.logical_id);
                    queue.push_back((dep.logical_id.clone(), InstallReason::Auto));
                }
                DependencyKind::Optional => warnings.push(i18n::optional_dependency_not_installed(
                    lang,
                    &dep.logical_id,
                )),
                DependencyKind::Embedded => warnings.push(i18n::embedded_dependency_not_installed(
                    lang,
                    &dep.logical_id,
                )),
                DependencyKind::Incompatible => warnings.push(
                    i18n::incompatible_dependency_not_installed(lang, &dep.logical_id),
                ),
                DependencyKind::Unknown => warnings.push(i18n::unknown_dependency_not_installed(
                    lang,
                    &dep.logical_id,
                )),
            }
        }
        planned.insert(
            logical_id.clone(),
            PlannedInstall {
                logical_id,
                candidate,
                artifact,
                reason,
                required_deps,
            },
        );
    }
    Ok(Plan {
        installs: planned.into_values().collect(),
        warnings,
    })
}

pub(crate) fn print_plan(plan: &Plan, dry_run: bool) {
    let lang = i18n::Lang::default();
    if dry_run {
        println!("{}", i18n::dry_run(lang));
    }
    for item in &plan.installs {
        println!(
            "{}",
            i18n::install_plan_item(
                lang,
                &item.logical_id,
                &item.artifact.version,
                &format!("{:?}", item.reason)
            )
        );
    }
    for warning in &plan.warnings {
        println!("{}", i18n::warning_message(lang, warning));
    }
}

pub(crate) fn select_artifact(
    project: &Project,
    profile: &Profile,
    source_weights: &BTreeMap<String, f64>,
) -> Result<Artifact> {
    let lang = i18n::Lang::default();
    project
        .candidates
        .iter()
        .flat_map(|candidate| {
            let provider = &candidate.provider;
            candidate
                .artifacts
                .iter()
                .map(move |artifact| (provider.as_str(), artifact))
        })
        .filter(|(_, artifact)| artifact.release == crate::provider::ReleaseKind::Stable)
        .filter(|(_, artifact)| {
            artifact
                .mc_versions
                .iter()
                .any(|version| version == &profile.mc_version)
        })
        .filter(|(_, artifact)| {
            artifact
                .loaders
                .iter()
                .any(|loader| loader == &profile.loader)
        })
        .filter(|(_, artifact)| {
            artifact.side == Side::Both
                || artifact.side == profile.side
                || profile.side == Side::Both
        })
        .fold(
            None,
            |selected: Option<(&str, &Artifact)>, (provider, artifact)| match selected {
                Some((cur_provider, current))
                    if !artifact_is_better(
                        artifact,
                        provider,
                        current,
                        cur_provider,
                        source_weights,
                    ) =>
                {
                    Some((cur_provider, current))
                }
                _ => Some((provider, artifact)),
            },
        )
        .map(|(_, artifact)| artifact.clone())
        .with_context(|| i18n::no_stable_compatible_artifact(lang, &project.logical_id))
}

fn artifact_is_better(
    candidate: &Artifact,
    candidate_provider: &str,
    current: &Artifact,
    current_provider: &str,
    source_weights: &BTreeMap<String, f64>,
) -> bool {
    let cand_weight = source_weights
        .get(candidate_provider)
        .copied()
        .unwrap_or(1.0);
    let curr_weight = source_weights.get(current_provider).copied().unwrap_or(1.0);
    let cand_eff = effective_downloads(cand_weight, candidate.download_count);
    let curr_eff = effective_downloads(curr_weight, current.download_count);

    if candidate.version == current.version {
        if cand_eff != curr_eff {
            return cand_eff > curr_eff;
        }
        return false;
    }
    match (
        parse_dotted_version(&candidate.version),
        parse_dotted_version(&current.version),
    ) {
        (Some(candidate_version), Some(current_version)) => {
            candidate_version > current_version
                || (candidate_version == current_version && cand_eff > curr_eff)
        }
        _ => false,
    }
}

pub(crate) fn parse_dotted_version(version: &str) -> Option<Vec<u64>> {
    let mut parts = Vec::new();
    for part in version.split('.') {
        if part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        parts.push(part.parse().ok()?);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts)
    }
}

pub(crate) fn read_mod_list(path: &Path) -> Result<Vec<String>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(text
        .lines()
        .map(|line| line.split('#').next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Download an artifact's bytes via the retryable engine. Uses
/// [`ProviderFetcher`] which delegates to `Provider::download` — this keeps
/// mock-provider tests working (deterministic in-memory bytes, no real HTTP)
/// while giving real providers (Modrinth/CurseForge) the engine's retry,
/// hash-verification, and staged-finalize semantics.
pub(crate) fn download_artifact(
    provider: &dyn Provider,
    artifact: &Artifact,
    dest: &Path,
) -> Result<String> {
    let fetcher = ProviderFetcher::new(provider, artifact);
    let opts = DownloadOptions {
        expected_sha256: artifact.sha256.clone(),
        ..Default::default()
    };
    let outcome = download_file(dest, &fetcher, &opts)?;
    Ok(outcome.sha256)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Profile, Side};
    use crate::lock::{test_installed_mod, LockState};
    use crate::provider::mock::test_helpers::{artifact, dep};
    use crate::provider::{Artifact, Candidate, DependencyKind, Project, Provider};
    use std::path::PathBuf;

    fn test_profile() -> Profile {
        Profile {
            name: "test".to_owned(),
            mods_dir: PathBuf::from("mods"),
            mc_version: "1.20.1".to_owned(),
            loader: "fabric".to_owned(),
            side: Side::Both,
        }
    }

    #[test]
    fn select_artifact_uses_numeric_versions_and_download_count_tiebreaker() {
        let mut low = artifact(
            "low",
            "1.9.0",
            "mod-1.9.0.jar",
            Some("https://cdn.example/low.jar"),
            vec![],
        );
        low.download_count = Some(1000);
        let mut high = artifact(
            "high",
            "1.10.0",
            "mod-1.10.0.jar",
            Some("https://cdn.example/high.jar"),
            vec![],
        );
        high.download_count = Some(1);
        let mut same_low = artifact(
            "same-low",
            "1.10.0",
            "mod-1.10.0-low.jar",
            Some("https://cdn.example/same-low.jar"),
            vec![],
        );
        same_low.download_count = Some(2);
        let mut same_high = artifact(
            "same-high",
            "1.10.0",
            "mod-1.10.0-high.jar",
            Some("https://cdn.example/same-high.jar"),
            vec![],
        );
        same_high.download_count = Some(20);
        let project = Project {
            logical_id: "versioned".into(),
            title: "Versioned".into(),
            description: String::new(),
            candidates: vec![Candidate {
                provider: "mock".into(),
                project_id: "versioned".into(),
                artifacts: vec![low, high, same_low, same_high],
            }],
        };

        let selected = select_artifact(&project, &test_profile(), &BTreeMap::new())
            .expect("selected artifact");
        assert_eq!(selected.file_id, "same-high");
    }

    #[test]
    fn build_plan_records_resolved_dependency_logical_id_for_reachability() {
        struct MismatchProvider;

        impl Provider for MismatchProvider {
            fn search(&self, _query: &str, profile: &Profile) -> Result<Vec<Project>> {
                self.get("root", profile).map(|project| vec![project])
            }

            fn get(&self, query: &str, _profile: &Profile) -> Result<Project> {
                match query {
                    "root" => Ok(Project {
                        logical_id: "root".into(),
                        title: "Root".into(),
                        description: String::new(),
                        candidates: vec![Candidate {
                            provider: "mock".into(),
                            project_id: "root".into(),
                            artifacts: vec![artifact(
                                "root-file",
                                "1.0.0",
                                "root.jar",
                                Some("https://cdn.example/root.jar"),
                                vec![dep("raw-dep-id", DependencyKind::Required)],
                            )],
                        }],
                    }),
                    "raw-dep-id" => Ok(Project {
                        logical_id: "resolved-dep".into(),
                        title: "Resolved Dep".into(),
                        description: String::new(),
                        candidates: vec![Candidate {
                            provider: "mock".into(),
                            project_id: "raw-dep-id".into(),
                            artifacts: vec![artifact(
                                "dep-file",
                                "1.0.0",
                                "dep.jar",
                                Some("https://cdn.example/dep.jar"),
                                vec![],
                            )],
                        }],
                    }),
                    _ => bail!("unexpected query {query}"),
                }
            }

            fn download(&self, _artifact: &Artifact) -> Result<Vec<u8>> {
                Ok(Vec::new())
            }
        }

        let plan = build_plan(
            &MismatchProvider,
            &test_profile(),
            &["root".into()],
            &LockState::default(),
            &BTreeMap::new(),
        )
        .expect("plan");
        let root = plan
            .installs
            .iter()
            .find(|item| item.logical_id == "root")
            .expect("root planned");
        assert_eq!(root.required_deps, vec!["resolved-dep"]);

        let mut lock = LockState::default();
        for item in plan.installs {
            lock.installed.insert(
                item.logical_id.clone(),
                test_installed_mod(
                    item.logical_id,
                    item.candidate.provider,
                    item.candidate.project_id,
                    item.artifact.file_id,
                    item.artifact.version,
                    item.artifact.filename,
                    item.reason,
                    item.required_deps,
                ),
            );
        }
        assert!(crate::lock::reachable_required_deps(&lock).contains("resolved-dep"));
    }

    #[test]
    fn composite_provider_merges_projects_from_multiple_sources() {
        struct StaticProvider {
            project: Project,
        }

        impl Provider for StaticProvider {
            fn search(&self, _query: &str, _profile: &Profile) -> Result<Vec<Project>> {
                Ok(vec![self.project.clone()])
            }

            fn get(&self, _query: &str, _profile: &Profile) -> Result<Project> {
                Ok(self.project.clone())
            }

            fn download(&self, _artifact: &Artifact) -> Result<Vec<u8>> {
                Ok(Vec::new())
            }
        }

        let provider = crate::provider::CompositeProvider::new(vec![
            Box::new(StaticProvider {
                project: Project {
                    logical_id: "same".into(),
                    title: "Same".into(),
                    description: "first".into(),
                    candidates: vec![Candidate {
                        provider: "one".into(),
                        project_id: "same-one".into(),
                        artifacts: vec![artifact(
                            "one-file",
                            "1.0.0",
                            "same.jar",
                            Some("https://cdn.example/one.jar"),
                            vec![],
                        )],
                    }],
                },
            }),
            Box::new(StaticProvider {
                project: Project {
                    logical_id: "same".into(),
                    title: "Same".into(),
                    description: "second".into(),
                    candidates: vec![Candidate {
                        provider: "two".into(),
                        project_id: "same-two".into(),
                        artifacts: vec![artifact(
                            "two-file",
                            "1.0.0",
                            "same.jar",
                            Some("https://cdn.example/two.jar"),
                            vec![],
                        )],
                    }],
                },
            }),
        ]);

        let found = provider
            .search("same", &test_profile())
            .expect("composite search");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].candidates.len(), 2);
    }

    #[test]
    fn effective_downloads_missing_count_treated_as_one() {
        let eff = crate::provider::effective_downloads(1.0, None);
        assert_eq!(eff, 1.0);
    }

    #[test]
    fn effective_downloads_zero_count_treated_as_one() {
        let eff = crate::provider::effective_downloads(1.0, Some(0));
        assert_eq!(eff, 1.0);
    }

    #[test]
    fn effective_downloads_multiplies_weight_by_max_count() {
        let eff = crate::provider::effective_downloads(2.5, Some(100));
        assert_eq!(eff, 250.0);
    }

    #[test]
    fn source_weight_changes_ordering() {
        let mut low_count = artifact(
            "low-count",
            "1.0.0",
            "mod-low.jar",
            Some("https://cdn.example/low.jar"),
            vec![],
        );
        low_count.download_count = Some(10);

        let mut high_count = artifact(
            "high-count",
            "1.0.0",
            "mod-high.jar",
            Some("https://cdn.example/high.jar"),
            vec![],
        );
        high_count.download_count = Some(1000);

        let project = Project {
            logical_id: "weighted".into(),
            title: "Weighted".into(),
            description: String::new(),
            candidates: vec![
                Candidate {
                    provider: "modrinth".into(),
                    project_id: "weighted-mr".into(),
                    artifacts: vec![low_count],
                },
                Candidate {
                    provider: "curseforge".into(),
                    project_id: "weighted-cf".into(),
                    artifacts: vec![high_count],
                },
            ],
        };

        let weights_default = BTreeMap::new();
        let selected_default = select_artifact(&project, &test_profile(), &weights_default)
            .expect("default selection");
        assert_eq!(selected_default.file_id, "high-count");

        let mut weights = BTreeMap::new();
        weights.insert("modrinth".to_owned(), 200.0);
        let selected_weighted =
            select_artifact(&project, &test_profile(), &weights).expect("weighted selection");
        assert_eq!(selected_weighted.file_id, "low-count");
    }

    #[test]
    fn ties_are_deterministic_first_candidate_wins() {
        let a = artifact(
            "a-file",
            "1.0.0",
            "mod-a.jar",
            Some("https://cdn.example/a.jar"),
            vec![],
        );

        let b = artifact(
            "b-file",
            "1.0.0",
            "mod-b.jar",
            Some("https://cdn.example/b.jar"),
            vec![],
        );

        let project = Project {
            logical_id: "tied".into(),
            title: "Tied".into(),
            description: String::new(),
            candidates: vec![
                Candidate {
                    provider: "alpha".into(),
                    project_id: "tied-alpha".into(),
                    artifacts: vec![a],
                },
                Candidate {
                    provider: "beta".into(),
                    project_id: "tied-beta".into(),
                    artifacts: vec![b],
                },
            ],
        };

        let weights = BTreeMap::new();
        let first = select_artifact(&project, &test_profile(), &weights).expect("first");
        let second = select_artifact(&project, &test_profile(), &weights).expect("second");
        assert_eq!(
            first.file_id, "a-file",
            "first candidate wins for equal effective_downloads"
        );
        assert_eq!(first.file_id, second.file_id, "deterministic same result");
    }

    #[test]
    fn select_artifact_respects_custom_source_candidate() {
        let source_artifact = artifact(
            "source-file",
            "2.0.0",
            "mod-source.jar",
            Some("https://cdn.example/source.jar"),
            vec![],
        );

        let mr_artifact = artifact(
            "mr-file",
            "1.0.0",
            "mod-mr.jar",
            Some("https://cdn.example/mr.jar"),
            vec![],
        );

        let project = Project {
            logical_id: "custom-src".into(),
            title: "Custom Source".into(),
            description: String::new(),
            candidates: vec![
                Candidate {
                    provider: "modrinth".into(),
                    project_id: "custom-src-mr".into(),
                    artifacts: vec![mr_artifact],
                },
                Candidate {
                    provider: "custom".into(),
                    project_id: "custom-src-cs".into(),
                    artifacts: vec![source_artifact],
                },
            ],
        };

        let weights = BTreeMap::new();
        let selected = select_artifact(&project, &test_profile(), &weights).expect("selected");
        assert_eq!(selected.file_id, "source-file");
    }

    #[test]
    fn user_config_source_weights_applied_in_build_plan() {
        struct DualProvider;

        impl Provider for DualProvider {
            fn search(&self, _query: &str, profile: &Profile) -> Result<Vec<Project>> {
                self.get("mod", profile).map(|p| vec![p])
            }

            fn get(&self, _query: &str, _profile: &Profile) -> Result<Project> {
                let mut low = artifact(
                    "low-count",
                    "1.0.0",
                    "mod-low.jar",
                    Some("https://cdn.example/low.jar"),
                    vec![],
                );
                low.download_count = Some(5);

                let mut high = artifact(
                    "high-count",
                    "1.0.0",
                    "mod-high.jar",
                    Some("https://cdn.example/high.jar"),
                    vec![],
                );
                high.download_count = Some(500);

                Ok(Project {
                    logical_id: "mod".into(),
                    title: "Mod".into(),
                    description: String::new(),
                    candidates: vec![
                        Candidate {
                            provider: "modrinth".into(),
                            project_id: "mod-mr".into(),
                            artifacts: vec![low],
                        },
                        Candidate {
                            provider: "curseforge".into(),
                            project_id: "mod-cf".into(),
                            artifacts: vec![high],
                        },
                    ],
                })
            }

            fn download(&self, _artifact: &Artifact) -> Result<Vec<u8>> {
                Ok(Vec::new())
            }
        }

        let mut weights = BTreeMap::new();
        weights.insert("modrinth".to_owned(), 200.0);

        let plan = build_plan(
            &DualProvider,
            &test_profile(),
            &["mod".into()],
            &LockState::default(),
            &weights,
        )
        .expect("plan");
        let mod_install = plan
            .installs
            .iter()
            .find(|i| i.logical_id == "mod")
            .expect("mod planned");
        assert_eq!(
            mod_install.artifact.file_id, "low-count",
            "modrinth weight 200 * 5 = 1000 > curseforge 1.0 * 500 = 500"
        );
    }
}

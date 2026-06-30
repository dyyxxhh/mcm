use std::path::Path;

use anyhow::Result;

use crate::i18n;
use crate::install::{deps_by_kind, select_artifact};
use crate::provider::{candidate_summary, group_projects, DependencyKind};

impl crate::app::App {
    pub(crate) fn search(&self, query: &str) -> Result<()> {
        let profile = self.active_profile()?;
        let provider = self.provider()?;
        for project in group_projects(provider.search(query, &profile)?) {
            println!("{} - {}", project.logical_id, project.title);
            println!("  {}", project.description);
            println!(
                "  {} {}",
                i18n::candidates_label(self.lang),
                candidate_summary(&project.candidates)
            );
        }
        Ok(())
    }

    pub(crate) fn info(&self, query: &str) -> Result<()> {
        let path = Path::new(query);
        if path.exists() || query.ends_with(".jar") {
            return crate::jar_info::local_jar_info(path);
        }
        let profile = self.active_profile()?;
        let config = self.load_config()?;
        let provider = self.provider()?;
        let project = provider.get(query, &profile)?;
        let artifact = select_artifact(&project, &profile, &config.user.source_weights)?;
        println!("{} - {}", project.logical_id, project.title);
        println!("{}", project.description);
        println!(
            "{} {}",
            i18n::candidates_label(self.lang),
            candidate_summary(&project.candidates)
        );
        println!(
            "{} {} {}",
            i18n::selected_label(self.lang),
            artifact.file_id,
            artifact.version
        );
        let required = deps_by_kind(&artifact, DependencyKind::Required);
        let optional = deps_by_kind(&artifact, DependencyKind::Optional);
        if !required.is_empty() {
            println!(
                "{} {}",
                i18n::required_deps_label(self.lang),
                required.join(", ")
            );
        }
        if !optional.is_empty() {
            println!(
                "{} {}",
                i18n::optional_deps_label(self.lang),
                optional.join(", ")
            );
        }
        for dep in artifact.deps.iter().filter(|dep| {
            dep.kind != DependencyKind::Required && dep.kind != DependencyKind::Optional
        }) {
            println!(
                "{} {:?} {}",
                i18n::warning_prefix(self.lang),
                dep.kind,
                dep.logical_id
            );
        }
        Ok(())
    }

    pub(crate) fn list(&self) -> Result<()> {
        let profile = self.active_profile()?;
        let lock = self.load_lock(&profile)?;
        for item in lock.installed.values() {
            println!(
                "{} {} {:?} {}/{}",
                item.logical_id, item.version, item.reason, item.provider, item.file_id
            );
        }
        Ok(())
    }

    pub(crate) fn status(&self) -> Result<()> {
        let profile = self.active_profile()?;
        let lock = self.load_lock(&profile)?;
        let mut owned = std::collections::BTreeSet::new();
        for item in lock.installed.values() {
            let filename = crate::safety::sanitize_filename(&item.filename)?;
            let target_path = profile.mods_dir.join(&filename);
            owned.insert(target_path.clone());
            if !target_path.exists() {
                println!(
                    "{}",
                    i18n::missing_file(self.lang, &item.logical_id, &item.filename)
                );
                continue;
            }
            let bytes = std::fs::read(&target_path)?;
            let actual = crate::util::sha256_hex(&bytes);
            if actual != item.sha256 {
                println!(
                    "{}",
                    i18n::changed_file(self.lang, &item.logical_id, &item.filename)
                );
            } else {
                println!("{}", i18n::ok_status(self.lang, &item.logical_id));
            }
        }
        if profile.mods_dir.exists() {
            for entry in std::fs::read_dir(&profile.mods_dir)? {
                let path = entry?.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("jar")
                    && !owned.contains(&path)
                {
                    if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                        println!("{}", i18n::untracked_file(self.lang, name));
                    }
                }
            }
        }
        Ok(())
    }
}

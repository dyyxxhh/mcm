use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use time::OffsetDateTime;

use crate::config::ProfileSnapshot;
use crate::confirmation::{emit_mc_critical_warning, OperationKind};
use crate::i18n;
use crate::install::{
    build_plan, download_artifact, print_plan, read_mod_list, search_install_roots,
};
use crate::lock::{reachable_required_deps, remove_owned_file, InstallReason, InstalledMod};
use crate::safety::{confirm_install, sanitize_filename, validate_download_url};

impl crate::app::App {
    pub(crate) fn install(
        &self,
        query: Option<String>,
        file: Option<PathBuf>,
        dry_run: bool,
        yes: bool,
    ) -> Result<()> {
        let mut roots = Vec::new();
        if let Some(query) = query {
            roots.push(query);
        }
        if let Some(file) = file {
            roots.extend(read_mod_list(&file)?);
        }
        if roots.is_empty() {
            bail!("{}", i18n::install_requires_query_or_file(self.lang));
        }
        let profile = self.active_profile()?;
        let config = self.load_config()?;
        let provider = self.provider()?;
        let mut lock = self.load_lock(&profile)?;
        let roots = search_install_roots(provider.as_ref(), &profile, &roots)?;
        let plan = build_plan(
            provider.as_ref(),
            &profile,
            &roots,
            &lock,
            &config.user.source_weights,
        )?;
        print_plan(&plan, dry_run);
        if !yes && !dry_run && !confirm_install()? {
            bail!("{}", i18n::installation_cancelled(self.lang));
        }
        if dry_run {
            return Ok(());
        }
        fs::create_dir_all(&profile.mods_dir)?;
        let mut staged = Vec::new();
        for item in &plan.installs {
            let url = item
                .artifact
                .download_url
                .as_deref()
                .context(i18n::missing_download_url(self.lang))?;
            validate_download_url(url)?;
            let filename = sanitize_filename(&item.artifact.filename)?;
            let target = profile.mods_dir.join(&filename);
            let hash = download_artifact(provider.as_ref(), &item.artifact, &target)?;
            staged.push((item, hash, filename));
        }
        for (item, hash, filename) in staged {
            lock.installed.insert(
                item.logical_id.clone(),
                InstalledMod {
                    logical_id: item.logical_id.clone(),
                    provider: item.candidate.provider.clone(),
                    project_id: item.candidate.project_id.clone(),
                    file_id: item.artifact.file_id.clone(),
                    version: item.artifact.version.clone(),
                    filename,
                    sha256: hash,
                    reason: item.reason,
                    required_deps: item.required_deps.clone(),
                    profile: ProfileSnapshot {
                        mc_version: profile.mc_version.clone(),
                        loader: profile.loader.clone(),
                        side: profile.side,
                    },
                    installed_at: OffsetDateTime::now_utc().to_string(),
                    owner_id: None,
                },
            );
        }
        self.save_lock(&profile, &lock)?;
        Ok(())
    }

    pub(crate) fn remove(&self, logical_id: &str, yes: bool) -> Result<()> {
        let profile = self.active_profile()?;
        let mut lock = self.load_lock(&profile)?;
        let Some(item) = lock.installed.get(logical_id).cloned() else {
            bail!("{}", i18n::mod_not_installed(self.lang, logical_id));
        };
        if item.reason != InstallReason::Manual {
            bail!("{}", i18n::mod_is_automatic(self.lang, logical_id));
        }
        if !yes {
            bail!("{}", i18n::confirmation_required_pass_yes(self.lang));
        }
        remove_owned_file(&profile, &item)?;
        lock.installed.remove(logical_id);
        self.save_lock(&profile, &lock)?;
        println!("{}", i18n::removed_mod(self.lang, logical_id));
        Ok(())
    }

    pub(crate) fn autoremove(&self, yes: bool) -> Result<()> {
        let profile = self.active_profile()?;
        let mut lock = self.load_lock(&profile)?;
        let needed = reachable_required_deps(&lock);
        let removable: Vec<String> = lock
            .installed
            .iter()
            .filter(|(id, item)| item.reason == InstallReason::Auto && !needed.contains(*id))
            .map(|(id, _)| id.clone())
            .collect();
        if removable.is_empty() {
            println!("{}", i18n::nothing_to_autoremove(self.lang));
            return Ok(());
        }
        if !yes {
            bail!("{}", i18n::confirmation_required_pass_yes(self.lang));
        }
        emit_mc_critical_warning(OperationKind::Autoremove);
        for id in removable {
            if let Some(item) = lock.installed.remove(&id) {
                remove_owned_file(&profile, &item)?;
                println!("{}", i18n::removed_mod(self.lang, &id));
            }
        }
        self.save_lock(&profile, &lock)?;
        Ok(())
    }
}

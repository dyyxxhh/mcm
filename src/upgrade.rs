use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};

use crate::app::App;
use crate::config::Config;
use crate::confirmation::{require_confirmation, OperationKind};
use crate::i18n;
use crate::install::{parse_dotted_version, select_artifact};
use crate::upgrade_deps::check_dependency_satisfaction;

pub(crate) struct UpgradePlan {
    pub(crate) game_name: String,
    pub(crate) items: Vec<UpgradeItem>,
    pub(crate) skipped: Vec<String>,
}

pub(crate) struct UpgradeItem {
    pub(crate) logical_id: String,
    pub(crate) old_version: String,
    pub(crate) new_version: String,
    pub(crate) reason: crate::lock::InstallReason,
}

impl App {
    pub(crate) fn upgrade(&self, yes: bool) -> Result<()> {
        let config = self.load_config()?;
        let game_name = resolve_game_for_upgrade(&config, self.lang)?;
        let plan = build_upgrade_plan_for_game(self, &config, &game_name)?;
        print_upgrade_plan(&plan, self.lang);
        if plan.items.is_empty() {
            return Ok(());
        }
        if !yes {
            bail!("{}", i18n::confirmation_required_pass_yes(self.lang));
        }
        require_confirmation(OperationKind::Upgrade, yes)?;
        apply_upgrade_plan(self, &config, &plan)?;
        Ok(())
    }

    pub(crate) fn full_upgrade(&self, yes: bool) -> Result<()> {
        let config = self.load_config()?;
        if config.games.is_empty() {
            bail!("{}", i18n::no_games_configured(self.lang));
        }
        let mut any_upgrades = false;
        let game_names: Vec<String> = config.games.keys().cloned().collect();
        for game_name in &game_names {
            let plan = build_upgrade_plan_for_game(self, &config, game_name)?;
            print_upgrade_plan(&plan, self.lang);
            if !plan.items.is_empty() {
                any_upgrades = true;
            }
        }
        if !any_upgrades {
            println!("{}", i18n::all_games_up_to_date(self.lang));
            return Ok(());
        }
        if !yes {
            bail!("{}", i18n::confirmation_required_pass_yes(self.lang));
        }
        require_confirmation(OperationKind::Upgrade, yes)?;
        for game_name in &game_names {
            let plan = build_upgrade_plan_for_game(self, &config, game_name)?;
            if !plan.items.is_empty() {
                apply_upgrade_plan(self, &config, &plan)?;
            }
        }
        Ok(())
    }
}

pub(crate) fn resolve_game_for_upgrade(config: &Config, lang: crate::i18n::Lang) -> Result<String> {
    config
        .default_game
        .clone()
        .or_else(|| config.games.keys().next().cloned())
        .with_context(|| i18n::no_games_configured(lang))
}

pub(crate) fn build_upgrade_plan_for_game(
    app: &App,
    config: &Config,
    game_name: &str,
) -> Result<UpgradePlan> {
    let game = config
        .games
        .get(game_name)
        .with_context(|| i18n::unknown_game(app.lang, game_name))?;
    let mc_version = game
        .mc_version
        .as_deref()
        .with_context(|| i18n::game_no_mc_version_for_upgrade(app.lang, game_name))?;
    let loader = game
        .loader
        .as_deref()
        .with_context(|| i18n::game_no_loader(app.lang, game_name))?;
    let profile = crate::config::Profile {
        name: game_name.to_owned(),
        mods_dir: game.root_dir.join("mods"),
        mc_version: mc_version.to_owned(),
        loader: loader.to_owned(),
        side: crate::config::Side::Both,
    };
    let lock = app.load_lock(&profile)?;
    let provider = app.provider()?;
    let mut items = Vec::new();
    let mut skipped = Vec::new();

    for (logical_id, installed) in &lock.installed {
        match provider.get(logical_id, &profile) {
            Ok(project) => match select_artifact(&project, &profile, &config.user.source_weights) {
                Ok(available) => {
                    if !version_is_newer(&available.version, &installed.version) {
                        continue;
                    }
                    if let Some(owner_mismatch) = check_owner_compatibility(
                        installed.owner_id.as_deref(),
                        available.owner_id.as_deref(),
                        logical_id,
                        app.lang,
                    ) {
                        skipped.push(owner_mismatch);
                        continue;
                    }
                    items.push(UpgradeItem {
                        logical_id: logical_id.clone(),
                        old_version: installed.version.clone(),
                        new_version: available.version.clone(),
                        reason: installed.reason,
                    });
                }
                Err(_) => {
                    skipped.push(i18n::no_compatible_artifact_available(app.lang, logical_id));
                }
            },
            Err(_) => {
                skipped.push(i18n::not_found_by_provider(app.lang, logical_id));
            }
        }
    }

    let planned_ids: BTreeSet<String> = items.iter().map(|i| i.logical_id.clone()).collect();
    let mut dep_skipped = Vec::new();
    items.retain(|item| match provider.get(&item.logical_id, &profile) {
        Ok(project) => match select_artifact(&project, &profile, &config.user.source_weights) {
            Ok(available) => {
                if let Some(dep_issue) =
                    check_dependency_satisfaction(&available, &lock, &planned_ids)
                {
                    dep_skipped.push(dep_issue);
                    false
                } else {
                    true
                }
            }
            Err(_) => true,
        },
        Err(_) => true,
    });
    skipped.extend(dep_skipped);

    items.sort_by(|a, b| a.logical_id.cmp(&b.logical_id));

    Ok(UpgradePlan {
        game_name: game_name.to_owned(),
        items,
        skipped,
    })
}

pub(crate) fn version_is_newer(available: &str, installed: &str) -> bool {
    match (
        parse_dotted_version(available),
        parse_dotted_version(installed),
    ) {
        (Some(avail), Some(inst)) => avail > inst,
        _ => false,
    }
}

pub(crate) fn check_owner_compatibility(
    installed_owner: Option<&str>,
    available_owner: Option<&str>,
    logical_id: &str,
    lang: crate::i18n::Lang,
) -> Option<String> {
    match (installed_owner, available_owner) {
        (Some(installed), Some(available)) if installed != available => {
            Some(i18n::owner_mismatch(lang, logical_id, installed, available))
        }
        _ => None,
    }
}

pub(crate) fn print_upgrade_plan(plan: &UpgradePlan, lang: crate::i18n::Lang) {
    if plan.items.is_empty() && plan.skipped.is_empty() {
        println!("{}", i18n::already_up_to_date(lang, &plan.game_name));
        return;
    }
    println!("{}", i18n::upgrade_plan_for(lang, &plan.game_name));
    for item in &plan.items {
        println!(
            "  {} -> {} {:?}",
            item.logical_id, item.new_version, item.reason
        );
    }
    for skip in &plan.skipped {
        println!("  {}", i18n::skipped(lang, skip));
    }
}

pub(crate) fn apply_upgrade_plan(app: &App, config: &Config, plan: &UpgradePlan) -> Result<()> {
    let game = config
        .games
        .get(&plan.game_name)
        .with_context(|| i18n::unknown_game(app.lang, &plan.game_name))?;
    let mc_version = game
        .mc_version
        .as_deref()
        .with_context(|| i18n::game_no_mc_version_for_upgrade(app.lang, &plan.game_name))?;
    let loader = game
        .loader
        .as_deref()
        .with_context(|| i18n::game_no_loader(app.lang, &plan.game_name))?;
    let profile = crate::config::Profile {
        name: plan.game_name.clone(),
        mods_dir: game.root_dir.join("mods"),
        mc_version: mc_version.to_owned(),
        loader: loader.to_owned(),
        side: crate::config::Side::Both,
    };
    let mut lock = app.load_lock(&profile)?;
    let provider = app.provider()?;
    let mut upgraded = Vec::new();

    for item in &plan.items {
        let project = provider.get(&item.logical_id, &profile)?;
        let artifact = select_artifact(&project, &profile, &config.user.source_weights)?;
        if let Some(url) = &artifact.download_url {
            crate::safety::validate_download_url(url)?;
        }
        let filename = crate::safety::sanitize_filename(&artifact.filename)?;
        let target = profile.mods_dir.join(&filename);
        std::fs::create_dir_all(&profile.mods_dir)?;
        let hash = crate::install::download_artifact(provider.as_ref(), &artifact, &target)?;
        let old = lock.installed.get(&item.logical_id).cloned();
        if let Some(mut entry) = old {
            entry.version = artifact.version.clone();
            entry.file_id = artifact.file_id.clone();
            entry.filename = filename;
            entry.sha256 = hash;
            entry.owner_id = artifact.owner_id.clone();
            lock.installed.insert(item.logical_id.clone(), entry);
        }
        upgraded.push(format!(
            "{}: {} -> {}",
            item.logical_id, item.old_version, artifact.version
        ));
    }
    app.save_lock(&profile, &lock)?;
    for msg in &upgraded {
        println!("upgraded {msg}");
    }
    Ok(())
}

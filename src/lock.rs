use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::ProfileSnapshot;
use crate::safety::sanitize_filename;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub(crate) struct LockState {
    pub(crate) installed: BTreeMap<String, InstalledMod>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct InstalledMod {
    pub(crate) logical_id: String,
    pub(crate) provider: String,
    pub(crate) project_id: String,
    pub(crate) file_id: String,
    pub(crate) version: String,
    pub(crate) filename: String,
    pub(crate) sha256: String,
    pub(crate) reason: InstallReason,
    pub(crate) required_deps: Vec<String>,
    pub(crate) profile: ProfileSnapshot,
    pub(crate) installed_at: String,
    /// Package author's user ID (for upgrade owner-mismatch checks).
    /// `None` when not available (backward-compatible deserialization).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) owner_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum InstallReason {
    Manual,
    Auto,
}

pub(crate) fn reachable_required_deps(lock: &LockState) -> BTreeSet<String> {
    let mut needed = BTreeSet::new();
    let mut queue: VecDeque<String> = lock
        .installed
        .values()
        .filter(|item| item.reason == InstallReason::Manual)
        .flat_map(|item| item.required_deps.clone())
        .collect();
    while let Some(id) = queue.pop_front() {
        if !needed.insert(id.clone()) {
            continue;
        }
        if let Some(item) = lock.installed.get(&id) {
            for dep in &item.required_deps {
                queue.push_back(dep.clone());
            }
        }
    }
    needed
}

pub(crate) fn remove_owned_file(
    profile: &crate::config::Profile,
    item: &InstalledMod,
) -> Result<()> {
    let filename = sanitize_filename(&item.filename)?;
    let target_path = profile.mods_dir.join(filename);
    if target_path.exists() {
        std::fs::remove_file(&target_path)
            .with_context(|| format!("remove {}", target_path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn test_installed_mod(
    logical_id: String,
    provider: String,
    project_id: String,
    file_id: String,
    version: String,
    filename: String,
    reason: InstallReason,
    required_deps: Vec<String>,
) -> InstalledMod {
    InstalledMod {
        logical_id,
        provider,
        project_id,
        file_id,
        version,
        filename,
        sha256: String::new(),
        reason,
        required_deps,
        profile: ProfileSnapshot {
            mc_version: "1.20.1".into(),
            loader: "fabric".into(),
            side: crate::config::Side::Both,
        },
        installed_at: String::new(),
        owner_id: None,
    }
}

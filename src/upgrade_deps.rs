use std::collections::BTreeSet;

use crate::lock::LockState;
use crate::provider::DependencyKind;

pub(crate) fn check_dependency_satisfaction(
    available: &crate::provider::Artifact,
    lock: &LockState,
    planned_ids: &BTreeSet<String>,
) -> Option<String> {
    for dep in &available.deps {
        match dep.kind {
            DependencyKind::Required => {
                if !lock.installed.contains_key(&dep.logical_id)
                    && !planned_ids.contains(&dep.logical_id)
                {
                    return Some(format!(
                        "{}: required dependency {} not satisfied — not installed and not in upgrade plan; refusing upgrade",
                        available.file_id, dep.logical_id
                    ));
                }
            }
            DependencyKind::Incompatible => {
                if lock.installed.contains_key(&dep.logical_id) {
                    return Some(format!(
                        "{}: incompatible dependency {} is installed; refusing upgrade",
                        available.file_id, dep.logical_id
                    ));
                }
            }
            DependencyKind::Unknown | DependencyKind::Embedded => {
                if lock.installed.contains_key(&dep.logical_id) {
                    return Some(format!(
                        "{}: {} dependency {} is installed — upgrade may be unsafe; refusing upgrade",
                        available.file_id,
                        match dep.kind {
                            DependencyKind::Unknown => "unknown",
                            DependencyKind::Embedded => "embedded",
                            _ => unreachable!(),
                        },
                        dep.logical_id
                    ));
                }
            }
            DependencyKind::Optional => {}
        }
    }
    None
}

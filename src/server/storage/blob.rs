//! Filesystem blob layer — atomic writes + reads for `.mcm` package bytes.

use std::path::Path;

use anyhow::{Context, Result};

/// Write `bytes` to `path` atomically: write to `<path>.tmp` then rename.
/// Creates parent directories. The rename is atomic on POSIX filesystems.
pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create parent {}", parent.display()))?;
    }
    let tmp = path.with_extension("mcm.tmp");
    std::fs::write(&tmp, bytes).with_context(|| format!("write tmp {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("rename {} to {}", tmp.display(), path.display()))
}

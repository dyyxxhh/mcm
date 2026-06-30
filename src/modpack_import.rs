//! Import/export for standard modpack formats: Modrinth `.mrpack` (v1) and
//! CurseForge manifest `.zip`. The hub dispatches based on archive root
//! entries; `import` and `export` submodules hold the format-specific logic.
//!
//! Safety: reuses `validate_asset_path` and `scan_for_secrets` from
//! `mcm_package` for the same guarantees as the `.mcm` parser. Resourcepacks,
//! shaderpacks, saves, config, and datapacks are treated as opaque bytes —
//! GLSL and NBT internals are never parsed or rewritten.

mod export;
mod import;
mod types;

use std::fs;
use std::io::Cursor;
use std::path::Path;

use anyhow::{bail, Context, Result};
use zip::ZipArchive;

use crate::app::App;
use crate::confirmation::{require_confirmation, OperationKind};
use crate::i18n;

/// Max total declared uncompressed size of archive entries (256 MB).
pub(crate) const MAX_TOTAL_SIZE: u64 = 256 * 1024 * 1024;
/// Max number of entries in an archive (10 000).
pub(crate) const MAX_ENTRY_COUNT: usize = 10_000;

impl App {
    pub(crate) fn import_modpack(&self, target: &str, yes: bool) -> Result<bool> {
        if !is_zip_path(target) {
            return Ok(false);
        }
        let bytes = fs::read(target).with_context(|| i18n::read_file_error(self.lang, target))?;
        let cursor = Cursor::new(bytes.as_slice());
        let mut archive = ZipArchive::new(cursor).with_context(|| {
            i18n::read_file_error(self.lang, &format!("{target} as zip archive"))
        })?;
        let format = detect_format(&mut archive)?;
        if format.is_none() {
            bail!("{}", i18n::not_a_modpack(self.lang, target));
        }
        require_confirmation(OperationKind::PackageInstall, yes)?;
        match format.unwrap() {
            types::ModpackFormat::Mrpack => self.import_mrpack(&mut archive)?,
            types::ModpackFormat::Curseforge => self.import_curseforge(&mut archive)?,
        }
        println!("{}", i18n::imported_modpack(self.lang));
        Ok(true)
    }

    /// Export the current game state as a Modrinth `.mrpack` zip.
    pub(crate) fn export_mrpack(&self, output: &Path) -> Result<()> {
        export::export_mrpack(self, output)
    }

    /// Export the current game state as a CurseForge modpack `.zip`.
    pub(crate) fn export_curseforge(&self, output: &Path) -> Result<()> {
        export::export_curseforge(self, output)
    }
}

fn is_zip_path(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    lower.ends_with(".mrpack") || lower.ends_with(".zip")
}

fn detect_format(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Option<types::ModpackFormat>> {
    enforce_limits(archive)?;
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name().to_owned();
        if name == "modrinth.index.json" {
            return Ok(Some(types::ModpackFormat::Mrpack));
        }
        if name == "manifest.json" {
            return Ok(Some(types::ModpackFormat::Curseforge));
        }
    }
    Ok(None)
}

fn enforce_limits(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<()> {
    let lang = crate::i18n::Lang::default();
    if archive.len() > MAX_ENTRY_COUNT {
        bail!(
            "{}",
            i18n::archive_too_many_entries(lang, archive.len(), MAX_ENTRY_COUNT)
        );
    }
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        total = total.saturating_add(entry.size());
        if total > MAX_TOTAL_SIZE {
            bail!("{}", i18n::archive_too_large(lang, MAX_TOTAL_SIZE));
        }
    }
    Ok(())
}

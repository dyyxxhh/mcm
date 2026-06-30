//! `.mrpack` export: build a `modrinth.index.json` from the active profile's
//! lock state, embed mod jars under `overrides/mods/`, and copy config /
//! shaderpack / resourcepack override files into the zip.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha512};
use time::OffsetDateTime;
use zip::ZipWriter;

use crate::app::App;
use crate::modpack_import::types::{MrpackFile, MrpackMcmMeta};

#[derive(Serialize)]
struct MrpackIndex {
    #[serde(rename = "format")]
    format: u32,
    game: String,
    #[serde(rename = "versionId")]
    version_id: String,
    dependencies: BTreeMap<String, String>,
    files: Vec<MrpackFile>,
}

pub(crate) fn export_mrpack(app: &App, output: &Path) -> Result<()> {
    let profile = app.active_profile()?;
    let lock = app.load_lock(&profile)?;
    let game_root = profile
        .mods_dir
        .parent()
        .map(std::path::Path::to_path_buf)
        .context("could not resolve game root from active profile mods_dir")?;
    let files: Vec<MrpackFile> = lock
        .installed
        .values()
        .filter_map(|m| build_mrpack_file(m, &game_root))
        .collect();
    let index = MrpackIndex {
        format: 1,
        game: "minecraft".to_owned(),
        version_id: format!("mcm-export-{}", OffsetDateTime::now_utc().unix_timestamp()),
        dependencies: build_deps(&profile),
        files,
    };
    let index_json = serde_json::to_string_pretty(&index)?;
    let opts = zip::write::FileOptions::default();
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("modrinth.index.json", opts)?;
    zip.write_all(index_json.as_bytes())?;
    for m in lock.installed.values() {
        let jar_path = game_root.join("mods").join(&m.filename);
        if let Ok(jar_bytes) = fs::read(&jar_path) {
            zip.start_file(format!("overrides/mods/{}", m.filename), opts)?;
            zip.write_all(&jar_bytes)?;
        }
    }
    copy_overrides(&mut zip, &game_root, &opts)?;
    let bytes = zip.finish()?.into_inner();
    crate::util::atomic_write(output, &bytes)?;
    println!("wrote {}", output.display());
    Ok(())
}

fn build_mrpack_file(m: &crate::lock::InstalledMod, game_root: &Path) -> Option<MrpackFile> {
    let jar_path = game_root.join("mods").join(&m.filename);
    let jar_bytes = fs::read(&jar_path).ok()?;
    let mut hashes = BTreeMap::new();
    hashes.insert("sha512".to_owned(), hex::encode(Sha512::digest(&jar_bytes)));
    Some(MrpackFile {
        path: format!("mods/{}", m.filename),
        hashes,
        downloads: Vec::new(),
        file_size: jar_bytes.len() as u64,
        mcm: Some(MrpackMcmMeta {
            logical_id: m.logical_id.clone(),
            provider: m.provider.clone(),
            project_id: m.project_id.clone(),
            file_id: m.file_id.clone(),
            version: m.version.clone(),
            sha256: m.sha256.clone(),
        }),
    })
}

fn build_deps(profile: &crate::config::Profile) -> BTreeMap<String, String> {
    let mut deps = BTreeMap::new();
    deps.insert("minecraft".to_owned(), profile.mc_version.clone());
    if !profile.loader.is_empty() {
        deps.insert(format!("{}-loader", profile.loader), String::new());
    }
    deps
}

fn copy_overrides(
    zip: &mut ZipWriter<Cursor<Vec<u8>>>,
    game_root: &Path,
    opts: &zip::write::FileOptions,
) -> Result<()> {
    for dir_name in ["config", "shaderpacks", "resourcepacks"] {
        let dir = game_root.join(dir_name);
        if !dir.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let rel = format!(
                "overrides/{dir_name}/{}",
                entry.file_name().to_string_lossy()
            );
            let bytes = fs::read(&path)?;
            zip.start_file(&rel, *opts)?;
            zip.write_all(&bytes)?;
        }
    }
    Ok(())
}

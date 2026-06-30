use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use anyhow::{Context, Result};
use zip::ZipArchive;

use crate::util::sha256_hex;

pub(crate) fn local_jar_info(path: &Path) -> Result<()> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    println!("local jar: {}", path.display());
    println!("sha256: {}", sha256_hex(&bytes));
    println!("size: {}", bytes.len());
    if let Ok(mut zip) = ZipArchive::new(Cursor::new(bytes)) {
        if let Ok(mut file) = zip.by_name("fabric.mod.json") {
            let mut text = String::new();
            file.read_to_string(&mut text)?;
            println!("metadata: fabric.mod.json");
            print_json_field(&text, "id");
            print_json_field(&text, "version");
            return Ok(());
        }
        if let Ok(mut file) = zip.by_name("META-INF/mods.toml") {
            let mut text = String::new();
            file.read_to_string(&mut text)?;
            println!("metadata: mods.toml");
            for line in text.lines().filter(|line| {
                line.trim_start().starts_with("modId") || line.trim_start().starts_with("version")
            }) {
                println!("{}", line.trim());
            }
            return Ok(());
        }
        if let Ok(mut file) = zip.by_name("mcmod.info") {
            let mut text = String::new();
            file.read_to_string(&mut text)?;
            println!("metadata: mcmod.info");
            print_mcmod_info_fields(&text);
            return Ok(());
        }
    }
    println!("metadata: unavailable");
    Ok(())
}

fn print_json_field(text: &str, field: &str) {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(found) = value.get(field).and_then(|value| value.as_str()) {
            println!("{field}: {found}");
        }
    }
}

fn print_mcmod_info_fields(text: &str) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    let entry = value
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or(&value);
    for (field, label) in [("modid", "id"), ("version", "version"), ("name", "name")] {
        if let Some(found) = entry.get(field).and_then(|value| value.as_str()) {
            println!("{label}: {found}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn mcmod_info_fields_are_read_from_generated_zip() {
        let mut bytes = Vec::new();
        {
            let cursor = Cursor::new(&mut bytes);
            let mut zip = zip::ZipWriter::new(cursor);
            zip.start_file("mcmod.info", zip::write::FileOptions::default())
                .expect("start mcmod.info");
            zip.write_all(br#"[{"modid":"legacy_mod","name":"Legacy Mod","version":"1.2.3"}]"#)
                .expect("write mcmod.info");
            zip.finish().expect("finish zip");
        }
        let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("zip archive");
        let mut file = archive.by_name("mcmod.info").expect("mcmod.info entry");
        let mut text = String::new();
        file.read_to_string(&mut text).expect("read mcmod.info");
        let value: serde_json::Value = serde_json::from_str(&text).expect("mcmod.info json");
        assert_eq!(value[0]["modid"], "legacy_mod");
        assert_eq!(value[0]["version"], "1.2.3");
    }
}

//! DTOs for Modrinth `.mrpack` (v1) and CurseForge manifest JSON, plus the
//! shared `PlannedInstall` / `PlannedMod` intermediate types used by both
//! importers and the apply step.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug)]
pub(crate) enum ModpackFormat {
    Mrpack,
    Curseforge,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct MrpackIndex {
    #[serde(rename = "format")]
    pub(crate) format: u32,
    pub(crate) game: String,
    #[serde(rename = "versionId")]
    pub(crate) version_id: String,
    #[serde(default)]
    pub(crate) dependencies: BTreeMap<String, String>,
    pub(crate) files: Vec<MrpackFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct MrpackFile {
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) downloads: Vec<String>,
    #[serde(rename = "fileSize", default)]
    pub(crate) file_size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mcm: Option<MrpackMcmMeta>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct MrpackMcmMeta {
    pub(crate) logical_id: String,
    pub(crate) provider: String,
    pub(crate) project_id: String,
    pub(crate) file_id: String,
    pub(crate) version: String,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CfManifest {
    pub(crate) minecraft: CfMinecraft,
    #[serde(default)]
    pub(crate) files: Vec<CfFileRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CfMinecraft {
    pub(crate) version: String,
    #[serde(default)]
    pub(crate) mod_loaders: Vec<CfModLoader>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CfModLoader {
    pub(crate) id: String,
    #[serde(default = "default_true")]
    pub(crate) primary: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CfFileRef {
    #[serde(rename = "projectID")]
    pub(crate) project_id: i64,
    #[serde(rename = "fileID")]
    pub(crate) file_id: i64,
    #[serde(default = "default_true")]
    pub(crate) required: bool,
}

fn default_true() -> bool {
    true
}

pub(crate) struct PlannedInstall {
    pub(crate) mods: Vec<PlannedMod>,
    pub(crate) overrides: Vec<(String, Vec<u8>)>,
}

pub(crate) struct PlannedMod {
    pub(crate) logical_id: String,
    pub(crate) provider: String,
    pub(crate) project_id: String,
    pub(crate) file_id: String,
    pub(crate) version: String,
    pub(crate) filename: String,
    pub(crate) sha256: Option<String>,
    pub(crate) bytes: Vec<u8>,
}

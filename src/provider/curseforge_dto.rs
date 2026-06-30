use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct CurseForgeListResponse<T> {
    pub(crate) data: Vec<T>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CurseForgeSingleResponse<T> {
    pub(crate) data: T,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CurseForgeMod {
    pub(crate) id: i64,
    pub(crate) slug: Option<String>,
    pub(crate) name: String,
    pub(crate) summary: Option<String>,
    #[serde(default, rename = "latestFiles")]
    pub(crate) latest_files: Option<Vec<CurseForgeFile>>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CurseForgeFile {
    pub(crate) id: i64,
    #[serde(rename = "displayName")]
    pub(crate) display_name: Option<String>,
    #[serde(rename = "fileName")]
    pub(crate) file_name: Option<String>,
    #[serde(rename = "downloadUrl")]
    pub(crate) download_url: Option<String>,
    #[serde(rename = "releaseType")]
    pub(crate) release_type: Option<i32>,
    #[serde(default, rename = "gameVersions")]
    pub(crate) game_versions: Vec<String>,
    #[serde(default)]
    pub(crate) hashes: Vec<CurseForgeHash>,
    #[serde(default)]
    pub(crate) dependencies: Vec<CurseForgeDependency>,
    #[serde(rename = "downloadCount")]
    pub(crate) download_count: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CurseForgeHash {
    pub(crate) algo: Option<i32>,
    pub(crate) value: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct CurseForgeDependency {
    #[serde(rename = "modId")]
    pub(crate) mod_id: i64,
    #[serde(rename = "relationType")]
    pub(crate) relation_type: i32,
}

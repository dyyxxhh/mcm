// SIZE_OK: non-test source is ~200 LOC; the bulk is the `#[cfg(test)] mod
// tests` block (JSON mapping, download-request, redirect-leak regression
// tests) which is test fixture and stays with the code it exercises.
use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::{Profile, Side};
use crate::provider::curseforge_dto::{
    CurseForgeDependency, CurseForgeFile, CurseForgeHash, CurseForgeListResponse, CurseForgeMod,
    CurseForgeSingleResponse,
};
use crate::provider::{
    Artifact, Candidate, Dependency, DependencyKind, Project, Provider, ReleaseKind,
};
use crate::safety::validate_download_url;

const CURSEFORGE_API_BASE: &str = "https://api.curseforge.com/v1";
const MINECRAFT_GAME_ID: i32 = 432;

pub(crate) struct CurseForgeProvider {
    pub(crate) api_key: String,
    pub(crate) client: reqwest::blocking::Client,
    base_url: String,
}

impl CurseForgeProvider {
    pub(crate) fn new() -> Result<Self> {
        let api_key = std::env::var("CURSEFORGE_API_KEY")
            .context("CurseForge provider requires CURSEFORGE_API_KEY")?;
        Ok(Self::with_base_url(api_key, CURSEFORGE_API_BASE))
    }

    pub(crate) fn with_base_url(api_key: String, base_url: &str) -> Self {
        Self {
            api_key,
            client: reqwest::blocking::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("CurseForge HTTP client"),
            base_url: base_url.trim_end_matches('/').to_owned(),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_tests(api_key: String, client: reqwest::blocking::Client) -> Self {
        Self {
            api_key,
            client,
            base_url: String::new(),
        }
    }

    fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        Ok(self
            .client
            .get(url)
            .header("Accept", "application/json")
            .header("x-api-key", &self.api_key)
            .query(params)
            .send()?
            .error_for_status()?
            .json()?)
    }

    pub(crate) fn curseforge_download_request(
        &self,
        url: &str,
    ) -> Result<reqwest::blocking::RequestBuilder> {
        validate_download_url(url)?;
        let parsed =
            reqwest::Url::parse(url).with_context(|| format!("invalid download URL {url}"))?;
        let host = parsed
            .host_str()
            .with_context(|| format!("download URL has no host: {url}"))?;
        let mut request = self.client.get(url).header("User-Agent", "mcm/0.1.0");
        if host.eq_ignore_ascii_case("edge.forgecdn.net") {
            request = request.header("x-api-key", &self.api_key);
        }
        Ok(request)
    }

    fn files_for(&self, mod_id: i64, profile: &Profile) -> Result<Vec<CurseForgeFile>> {
        let mut params = vec![
            ("gameVersion", profile.mc_version.clone()),
            ("pageSize", "50".to_owned()),
        ];
        if let Some(loader) = curseforge_loader_type(&profile.loader) {
            params.push(("modLoaderType", loader.to_string()));
        }
        let response: CurseForgeListResponse<CurseForgeFile> =
            self.get_json(&format!("/mods/{mod_id}/files"), &params)?;
        response
            .data
            .into_iter()
            .map(|mut file| {
                if file.download_url.is_none() {
                    let fallback: CurseForgeSingleResponse<String> = self.get_json(
                        &format!("/mods/{mod_id}/files/{}/download-url", file.id),
                        &[],
                    )?;
                    file.download_url = Some(fallback.data);
                }
                Ok(file)
            })
            .collect()
    }
}

impl Provider for CurseForgeProvider {
    fn search(&self, query: &str, profile: &Profile) -> Result<Vec<Project>> {
        let mut params = vec![
            ("gameId", MINECRAFT_GAME_ID.to_string()),
            ("searchFilter", query.to_owned()),
            ("gameVersion", profile.mc_version.clone()),
            ("pageSize", "20".to_owned()),
        ];
        if let Some(loader) = curseforge_loader_type(&profile.loader) {
            params.push(("modLoaderType", loader.to_string()));
        }
        let response: CurseForgeListResponse<CurseForgeMod> =
            self.get_json("/mods/search", &params)?;
        response
            .data
            .into_iter()
            .map(|module| {
                let files = self.files_for(module.id, profile).or_else(|_| {
                    Ok::<_, anyhow::Error>(module.latest_files.clone().unwrap_or_default())
                })?;
                Ok(curseforge_project_from_parts(module, files))
            })
            .collect()
    }

    fn get(&self, query: &str, profile: &Profile) -> Result<Project> {
        if let Ok(id) = query.parse::<i64>() {
            let response: CurseForgeSingleResponse<CurseForgeMod> =
                self.get_json(&format!("/mods/{id}"), &[])?;
            let files = self.files_for(response.data.id, profile)?;
            return Ok(curseforge_project_from_parts(response.data, files));
        }

        let mut params = vec![
            ("gameId", MINECRAFT_GAME_ID.to_string()),
            ("slug", query.to_owned()),
            ("gameVersion", profile.mc_version.clone()),
            ("pageSize", "1".to_owned()),
        ];
        if let Some(loader) = curseforge_loader_type(&profile.loader) {
            params.push(("modLoaderType", loader.to_string()));
        }
        let response: CurseForgeListResponse<CurseForgeMod> =
            self.get_json("/mods/search", &params)?;
        let module = response
            .data
            .into_iter()
            .next()
            .with_context(|| format!("mod {query} not found"))?;
        let files = self.files_for(module.id, profile)?;
        Ok(curseforge_project_from_parts(module, files))
    }

    fn download(&self, artifact: &Artifact) -> Result<Vec<u8>> {
        let url = artifact
            .download_url
            .as_deref()
            .context("missing download URL")?;
        let response = self
            .curseforge_download_request(url)?
            .send()?
            .error_for_status()?;
        Ok(response.bytes()?.to_vec())
    }
}

fn curseforge_project_from_parts(module: CurseForgeMod, files: Vec<CurseForgeFile>) -> Project {
    let logical_id = module.slug.clone().unwrap_or_else(|| module.id.to_string());
    Project {
        logical_id,
        title: module.name,
        description: module.summary.unwrap_or_default(),
        candidates: vec![Candidate {
            provider: "curseforge".to_owned(),
            project_id: module.id.to_string(),
            artifacts: files
                .into_iter()
                .map(curseforge_artifact_from_file)
                .collect(),
        }],
    }
}

fn curseforge_artifact_from_file(file: CurseForgeFile) -> Artifact {
    let filename = file
        .file_name
        .clone()
        .or(file.display_name.clone())
        .unwrap_or_else(|| format!("curseforge-{}.jar", file.id));
    Artifact {
        file_id: file.id.to_string(),
        version: file
            .display_name
            .clone()
            .unwrap_or_else(|| file.id.to_string()),
        release: release_from_curseforge(file.release_type),
        mc_versions: file.game_versions.clone(),
        loaders: curseforge_loaders_from_versions(&file.game_versions),
        side: Side::Both,
        filename,
        download_url: file.download_url,
        sha256: file.hashes.iter().find_map(|hash| match hash.algo {
            Some(3) => hash.value.clone(),
            _ => None,
        }),
        download_count: file.download_count,
        deps: file
            .dependencies
            .into_iter()
            .map(|dep| Dependency {
                logical_id: dep.mod_id.to_string(),
                kind: dependency_from_curseforge(dep.relation_type),
            })
            .collect(),
        owner_id: None,
    }
}

fn release_from_curseforge(value: Option<i32>) -> ReleaseKind {
    match value {
        Some(1) => ReleaseKind::Stable,
        Some(2) => ReleaseKind::Beta,
        Some(3) => ReleaseKind::Alpha,
        _ => ReleaseKind::Alpha,
    }
}

fn dependency_from_curseforge(value: i32) -> DependencyKind {
    match value {
        3 => DependencyKind::Required,
        2 => DependencyKind::Optional,
        1 | 6 => DependencyKind::Embedded,
        5 => DependencyKind::Incompatible,
        _ => DependencyKind::Unknown,
    }
}

fn curseforge_loader_type(loader: &str) -> Option<i32> {
    match loader.to_ascii_lowercase().as_str() {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" | "neo-forge" => Some(6),
        _ => None,
    }
}

fn curseforge_loaders_from_versions(game_versions: &[String]) -> Vec<String> {
    let mut loaders = Vec::new();
    for version in game_versions {
        let lower = version.to_ascii_lowercase();
        for loader in ["fabric", "forge", "quilt", "neoforge"] {
            if lower.contains(loader) && !loaders.iter().any(|found| found == loader) {
                loaders.push(loader.to_owned());
            }
        }
    }
    if loaders.is_empty() {
        loaders.push("fabric".to_owned());
        loaders.push("forge".to_owned());
        loaders.push("quilt".to_owned());
        loaders.push("neoforge".to_owned());
    }
    loaders
}

// Compile-time references to DTO fields so the re-export stays in sync.
#[allow(dead_code)]
fn _dto_compile_checks(hash: CurseForgeHash, dep: CurseForgeDependency) {
    let _ = (hash.algo, hash.value, dep.mod_id, dep.relation_type);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::mock::test_helpers::test_profile;
    use std::collections::BTreeMap;

    #[test]
    fn curseforge_json_mapping_preserves_slug_files_hashes_and_dependencies() {
        let module_json = r#"{
            "id":12345,
            "slug":"logical-mod",
            "name":"Logical Mod",
            "summary":"Mapped from CurseForge"
        }"#;
        let files_json = r#"[
            {
                "id":9001,
                "modId":12345,
                "displayName":"1.0.0",
                "fileName":"logical-mod.jar",
                "downloadUrl":"https://edge.forgecdn.net/files/logical-mod.jar",
                "releaseType":1,
                "gameVersions":["1.20.1","Fabric"],
                "hashes":[{"algo":1,"value":"def456"}],
                "dependencies":[
                    {"modId":222,"relationType":3},
                    {"modId":333,"relationType":2},
                    {"modId":444,"relationType":5}
                ]
            },
            {
                "id":9002,
                "modId":12345,
                "displayName":"2.0.0-beta",
                "fileName":"logical-mod-beta.jar",
                "releaseType":2,
                "gameVersions":["1.20.1","Fabric"],
                "hashes":[],
                "dependencies":[]
            }
        ]"#;
        let module: CurseForgeMod = serde_json::from_str(module_json).expect("module json");
        let files: Vec<CurseForgeFile> = serde_json::from_str(files_json).expect("files json");
        let mapped = curseforge_project_from_parts(module, files);

        assert_eq!(mapped.logical_id, "logical-mod");
        assert_eq!(mapped.candidates[0].provider, "curseforge");
        assert_eq!(mapped.candidates[0].project_id, "12345");
        let release = mapped.candidates[0]
            .artifacts
            .iter()
            .find(|artifact| artifact.file_id == "9001")
            .expect("release artifact");
        assert_eq!(release.release, ReleaseKind::Stable);
        assert_eq!(release.sha256, None);
        assert_eq!(
            release.download_url.as_deref(),
            Some("https://edge.forgecdn.net/files/logical-mod.jar")
        );
        assert!(release.loaders.iter().any(|loader| loader == "fabric"));
        assert_eq!(release.deps[0].logical_id, "222");
        assert_eq!(release.deps[0].kind, DependencyKind::Required);
        assert_eq!(release.deps[1].kind, DependencyKind::Optional);
        assert_eq!(release.deps[2].kind, DependencyKind::Incompatible);
        let selected = crate::install::select_artifact(&mapped, &test_profile(), &BTreeMap::new())
            .expect("stable compatible selection");
        assert_eq!(selected.file_id, "9001");
    }

    #[test]
    fn curseforge_download_sends_api_key_header_to_allowlisted_cdn() {
        let client = reqwest::blocking::Client::new();
        let provider = CurseForgeProvider::for_tests("RED-TEST-API-KEY".to_owned(), client);
        let request = provider
            .curseforge_download_request("https://edge.forgecdn.net/files/9001/mod.jar")
            .expect("allowlisted CurseForge CDN request")
            .build()
            .expect("built request");

        assert_eq!(request.url().scheme(), "https");
        assert_eq!(request.url().host_str(), Some("edge.forgecdn.net"));
        assert_eq!(
            request
                .headers()
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("RED-TEST-API-KEY")
        );
    }

    #[test]
    fn curseforge_download_refuses_when_url_host_not_allowlisted() {
        let client = reqwest::blocking::Client::new();
        let provider = CurseForgeProvider::for_tests("RED-TEST-API-KEY".to_owned(), client);
        let err = provider
            .curseforge_download_request("https://evil.example.com/mod.jar")
            .expect_err("non-allowlisted host should be rejected before request build");
        let chain = format!("{err:#}").to_lowercase();
        assert!(
            chain.contains("allowlist"),
            "expected allowlist rejection before any request could be sent, got: {err:#}"
        );
    }

    fn redirect_fixture() -> (
        String,
        std::sync::mpsc::Receiver<Vec<u8>>,
        std::thread::JoinHandle<()>,
        std::thread::JoinHandle<()>,
    ) {
        use std::io::{Read, Write as IoWrite};
        use std::net::TcpListener;
        use std::sync::mpsc;
        use std::thread;
        use std::time::{Duration, Instant};

        let target = TcpListener::bind("127.0.0.1:0").expect("bind redirect target");
        target
            .set_nonblocking(true)
            .expect("set target nonblocking");
        let target_addr = target.local_addr().expect("target addr");
        let (target_tx, target_rx) = mpsc::channel::<Vec<u8>>();
        let target_thread = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_millis(500);
            while Instant::now() < deadline {
                match target.accept() {
                    Ok((mut stream, _)) => {
                        let mut request = vec![0u8; 8192];
                        let n = stream.read(&mut request).expect("read target request");
                        request.truncate(n);
                        let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK");
                        target_tx.send(request).expect("send target request");
                        return;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("accept target redirect: {error}"),
                }
            }
        });

        let redirector = TcpListener::bind("127.0.0.1:0").expect("bind redirector");
        let redirector_addr = redirector.local_addr().expect("redirector addr");
        let redirector_thread = thread::spawn(move || {
            let (mut stream, _) = redirector.accept().expect("accept redirect request");
            let mut request = vec![0u8; 8192];
            let _ = stream.read(&mut request).expect("read redirect request");
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://{target_addr}/leak-check\r\nContent-Length: 0\r\n\r\n"
            );
            stream
                .write_all(response.as_bytes())
                .expect("write redirect");
        });

        (
            format!("http://{redirector_addr}/start"),
            target_rx,
            redirector_thread,
            target_thread,
        )
    }

    #[test]
    fn curseforge_http_client_does_not_follow_redirects_with_api_key_headers() {
        use std::time::Duration;

        let (url, target_rx, redirector_thread, target_thread) = redirect_fixture();
        let provider =
            CurseForgeProvider::with_base_url("RED-TEST-API-KEY".to_owned(), "http://unused");
        let response = provider
            .client
            .get(url)
            .header("x-api-key", "RED-TEST-API-KEY")
            .send()
            .expect("redirect response");

        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        assert!(
            target_rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "CurseForge HTTP client followed a redirect; custom x-api-key headers could leak cross-host"
        );
        redirector_thread.join().expect("redirector thread join");
        target_thread.join().expect("target thread join");
    }

    #[test]
    fn modrinth_http_client_does_not_follow_redirects_after_url_validation() {
        use std::time::Duration;

        let (url, target_rx, redirector_thread, target_thread) = redirect_fixture();
        let provider = super::super::ModrinthProvider::with_base_url("http://unused");
        let response = provider.client.get(url).send().expect("redirect response");

        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        assert!(
            target_rx.recv_timeout(Duration::from_millis(200)).is_err(),
            "Modrinth HTTP client followed a redirect; allowlisted download URLs could redirect to an unvalidated host"
        );
        redirector_thread.join().expect("redirector thread join");
        target_thread.join().expect("target thread join");
    }
}

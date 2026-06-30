use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use serde_json::Value;

use crate::i18n;
use crate::parse_mcm_package;

pub(crate) struct ShareClient {
    client: Client,
    base_url: String,
}

#[derive(Debug, Clone)]
pub(crate) struct PackageEntry {
    pub slug: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub owner: Option<String>,
}

impl ShareClient {
    pub(crate) fn new(server_url: &str) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(format!("mcm/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .context("build HTTP client")?;
        Ok(Self {
            client,
            base_url: server_url.trim_end_matches('/').to_string(),
        })
    }

    pub(crate) fn oidc_login(&self, lang: i18n::Lang) -> Result<String> {
        let start_url = format!("{}/api/auth/oidc/start", self.base_url);
        let start_resp: Value = self
            .client
            .get(&start_url)
            .send()
            .with_context(|| format!("connect to {}", self.base_url))?
            .json()
            .with_context(|| "parse OIDC start response")?;

        let auth_url = start_resp["auth_url"]
            .as_str()
            .context("missing auth_url in response")?;
        let login_id = start_resp["login_id"]
            .as_str()
            .context("missing login_id in response")?;

        let full_url = if auth_url.starts_with("http") {
            auth_url.to_string()
        } else {
            format!("{}{}", self.base_url, auth_url)
        };

        println!("{}", i18n::auth_login_print_url(lang, &full_url));
        println!("{}", i18n::auth_login_polling(lang));

        let poll_url = format!("{}/api/auth/oidc/poll/{}", self.base_url, login_id);
        let timeout = Duration::from_secs(600);
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                bail!("{}", i18n::auth_login_expired(lang));
            }
            std::thread::sleep(Duration::from_secs(2));
            let poll_resp: Value = match self.client.get(&poll_url).send() {
                Ok(r) => r.json().unwrap_or(serde_json::json!({"status":"error"})),
                Err(_) => continue,
            };
            match poll_resp["status"].as_str() {
                Some("complete") => {
                    let token = poll_resp["token"]
                        .as_str()
                        .context("missing token in poll response")?;
                    let owner = poll_resp["owner"].as_str().unwrap_or("unknown");
                    println!("{}", i18n::auth_login_success(lang, owner));
                    return Ok(token.to_string());
                }
                Some("expired") => bail!("{}", i18n::auth_login_expired(lang)),
                Some("denied") => {
                    let reason = poll_resp["reason"].as_str().unwrap_or("unknown");
                    bail!("{}", i18n::auth_login_denied(lang, reason));
                }
                Some("pending") | None => {}
                _ => {}
            }
        }
    }

    pub(crate) fn publish(
        &self,
        token: &str,
        slug: &str,
        version: &str,
        content: &Value,
    ) -> Result<String> {
        let url = format!("{}/api/share/pkg", self.base_url);
        let body = serde_json::json!({"slug": slug, "version": version, "content": content});
        let resp = self
            .client
            .post(&url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .context("send publish request")?;
        let status = resp.status();
        let resp_text = resp.text().unwrap_or_default();
        if status.is_success() {
            let v: Value = serde_json::from_str(&resp_text).unwrap_or_default();
            return Ok(v["slug"].as_str().unwrap_or(slug).to_string());
        }
        let v: Value = serde_json::from_str(&resp_text).unwrap_or_default();
        let error = v["error"].as_str().unwrap_or("unknown error");
        let reason = v["reason"].as_str().unwrap_or("");
        if !reason.is_empty() {
            bail!("publish failed: {error} ({reason})");
        }
        bail!("publish failed: {error}")
    }

    pub(crate) fn update(
        &self,
        token: &str,
        slug: &str,
        version: &str,
        content: &Value,
    ) -> Result<String> {
        let url = format!("{}/api/share/pkg/{}", self.base_url, slug);
        let body = serde_json::json!({"slug": slug, "version": version, "content": content});
        let resp = self
            .client
            .put(&url)
            .bearer_auth(token)
            .json(&body)
            .send()
            .context("send update request")?;
        let status = resp.status();
        let resp_text = resp.text().unwrap_or_default();
        if status.is_success() {
            return Ok(slug.to_string());
        }
        let v: Value = serde_json::from_str(&resp_text).unwrap_or_default();
        let error = v["error"].as_str().unwrap_or("unknown error");
        bail!("update failed: {error}")
    }

    pub(crate) fn delete(&self, token: &str, slug: &str) -> Result<()> {
        let url = format!("{}/api/share/pkg/{}", self.base_url, slug);
        let resp = self
            .client
            .delete(&url)
            .bearer_auth(token)
            .send()
            .context("send delete request")?;
        let status = resp.status();
        let resp_text = resp.text().unwrap_or_default();
        if status.is_success() {
            return Ok(());
        }
        let v: Value = serde_json::from_str(&resp_text).unwrap_or_default();
        let error = v["error"].as_str().unwrap_or("unknown error");
        bail!("delete failed: {error}")
    }

    pub(crate) fn list(&self) -> Result<Vec<PackageEntry>> {
        let url = format!("{}/api/share/list", self.base_url);
        let resp = self.client.get(&url).send().context("send list request")?;
        let status = resp.status();
        let resp_text = resp.text().unwrap_or_default();
        if !status.is_success() {
            let v: Value = serde_json::from_str(&resp_text).unwrap_or_default();
            let error = v["error"].as_str().unwrap_or("unknown error");
            bail!("list failed: {error}");
        }
        let v: Value = serde_json::from_str(&resp_text).unwrap_or_default();
        parse_package_list(&v)
    }

    pub(crate) fn list_mine(&self, token: &str) -> Result<Vec<PackageEntry>> {
        let url = format!("{}/api/share/mine", self.base_url);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(token)
            .send()
            .context("send list-mine request")?;
        let status = resp.status();
        let resp_text = resp.text().unwrap_or_default();
        if !status.is_success() {
            let v: Value = serde_json::from_str(&resp_text).unwrap_or_default();
            let error = v["error"].as_str().unwrap_or("unknown error");
            bail!("list-mine failed: {error}");
        }
        let v: Value = serde_json::from_str(&resp_text).unwrap_or_default();
        parse_package_list(&v)
    }

    pub(crate) fn download(&self, slug: &str) -> Result<String> {
        let url = format!("{}/api/share/pkg/{}", self.base_url, slug);
        let resp = self
            .client
            .get(&url)
            .send()
            .context("send download request")?;
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        if !status.is_success() {
            let v: Value = serde_json::from_str(&body).unwrap_or_default();
            let error = v["error"].as_str().unwrap_or("unknown error");
            bail!("download failed: {error}");
        }
        Ok(body)
    }

    pub(crate) fn download_to_file(&self, slug: &str, output: &Path) -> Result<String> {
        let json = self.download(slug)?;
        fs::write(output, &json)
            .with_context(|| format!("write package to {}", output.display()))?;
        Ok(json)
    }
}

pub(crate) fn resolve_slug(input: &str) -> &str {
    if input.starts_with("http://") || input.starts_with("https://") {
        input.split('/').rfind(|s| !s.is_empty()).unwrap_or(input)
    } else {
        input
    }
}

pub(crate) fn parse_package_json_to_value(json: &str) -> Result<Value> {
    parse_mcm_package(json)?;
    let v: Value = serde_json::from_str(json).context("parse package JSON")?;
    Ok(v)
}

fn parse_package_list(v: &Value) -> Result<Vec<PackageEntry>> {
    let packages = v["packages"]
        .as_array()
        .context("invalid packages response")?;
    Ok(packages
        .iter()
        .map(|p| PackageEntry {
            slug: p["slug"].as_str().unwrap_or("").to_string(),
            name: p["name"].as_str().unwrap_or("").to_string(),
            version: p["version"].as_str().unwrap_or("").to_string(),
            description: p["description"].as_str().map(String::from),
            owner: p["owner"].as_str().map(String::from),
        })
        .collect())
}

pub(crate) fn install_command_snippet(slug: &str) -> String {
    format!("curl -fsSL https://mc.dyyapp.com/install/pkg/{slug} | bash")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_slug_plain() {
        assert_eq!(resolve_slug("my-pkg"), "my-pkg");
    }

    #[test]
    fn resolve_slug_url() {
        assert_eq!(
            resolve_slug("https://mc.dyyapp.com/api/share/pkg/my-pkg"),
            "my-pkg"
        );
    }

    #[test]
    fn resolve_slug_url_trailing_slash() {
        assert_eq!(
            resolve_slug("https://mc.dyyapp.com/api/share/pkg/my-pkg/"),
            "my-pkg"
        );
    }

    #[test]
    fn install_command_snippet_format() {
        let cmd = install_command_snippet("my-pack");
        assert_eq!(
            cmd,
            "curl -fsSL https://mc.dyyapp.com/install/pkg/my-pack | bash"
        );
    }
}

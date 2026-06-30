use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::app::App;
use crate::cli::PkgAuthCommand;
use crate::i18n;

pub(crate) fn pkg_auth_impl(app: &App, command: PkgAuthCommand) -> Result<()> {
    match command {
        PkgAuthCommand::Login { server } => app.pkg_auth_login(&server),
        PkgAuthCommand::Status { server } => app.pkg_auth_status(&server),
        PkgAuthCommand::Logout { server } => app.pkg_auth_logout(&server),
    }
}

#[derive(Serialize, Deserialize)]
struct SessionFile {
    server: String,
    token: String,
}

impl App {
    fn pkg_auth_login(&self, server: &str) -> Result<()> {
        let server_url = normalize_server(server)?;
        let client = http_client()?;

        let start_url = format!("{server_url}/api/auth/oidc/start?client=cli");
        let resp = client
            .get(&start_url)
            .send()
            .with_context(|| i18n::auth_login_network_error(self.lang, "failed to reach server"))?;

        if !resp.status().is_success() {
            bail!(
                "{}",
                i18n::auth_login_network_error(
                    self.lang,
                    &format!("server returned {}", resp.status())
                )
            );
        }

        let body: serde_json::Value = resp.json().context("failed to parse server response")?;

        let auth_url = body["auth_url"]
            .as_str()
            .context("missing auth_url in server response")?;
        let login_id = body["login_id"]
            .as_str()
            .context("missing login_id in server response")?;

        println!("{}", i18n::auth_login_print_url(self.lang, auth_url));
        eprintln!("{}", i18n::auth_login_polling(self.lang));

        let poll_url = format!("{server_url}/api/auth/oidc/poll/{login_id}");
        let token = loop {
            std::thread::sleep(Duration::from_secs(2));

            let poll_resp = client.get(&poll_url).send().with_context(|| {
                i18n::auth_login_network_error(self.lang, "poll request failed")
            })?;

            if !poll_resp.status().is_success() {
                bail!(
                    "{}",
                    i18n::auth_login_network_error(
                        self.lang,
                        &format!("poll returned {}", poll_resp.status())
                    )
                );
            }

            let poll_body: serde_json::Value =
                poll_resp.json().context("failed to parse poll response")?;

            match poll_body["status"].as_str() {
                Some("pending") => continue,
                Some("complete") => {
                    let token = poll_body["token"]
                        .as_str()
                        .context("missing token in complete response")?
                        .to_string();
                    let owner = poll_body["owner"].as_str().unwrap_or("unknown");
                    println!("{}", i18n::auth_login_success(self.lang, owner));
                    break token;
                }
                Some("expired") => bail!("{}", i18n::auth_login_expired(self.lang)),
                Some("denied") => {
                    let reason = poll_body["reason"].as_str().unwrap_or("unknown");
                    bail!("{}", i18n::auth_login_denied(self.lang, reason));
                }
                other => bail!(
                    "{}",
                    i18n::auth_login_network_error(
                        self.lang,
                        &format!("unexpected poll status: {other:?}")
                    )
                ),
            }
        };

        save_session_file(
            &session_path(&self.state_dir, &server_url)?,
            &server_url,
            &token,
        )?;
        Ok(())
    }

    fn pkg_auth_status(&self, server: &str) -> Result<()> {
        let server_url = normalize_server(server)?;
        let path = session_path(&self.state_dir, &server_url)?;

        let session = load_session_file(&path).ok().flatten();

        let Some(session) = session else {
            println!("{}", i18n::auth_status_not_authenticated(self.lang));
            return Ok(());
        };

        let client = http_client()?;
        let session_url = format!("{}/api/auth/oidc/session", session.server);
        let resp = client
            .get(&session_url)
            .header("Authorization", format!("Bearer {}", session.token))
            .send()
            .with_context(|| i18n::auth_login_network_error(self.lang, "failed to reach server"))?;

        if resp.status().is_success() {
            let body: serde_json::Value =
                resp.json().context("failed to parse session response")?;
            let owner = body["owner"].as_str().unwrap_or("unknown");
            println!("{}", i18n::auth_status_authenticated(self.lang, owner));
        } else {
            let _ = std::fs::remove_file(&path);
            println!("{}", i18n::auth_status_not_authenticated(self.lang));
        }

        Ok(())
    }

    fn pkg_auth_logout(&self, server: &str) -> Result<()> {
        let server_url = normalize_server(server)?;
        let path = session_path(&self.state_dir, &server_url)?;

        let session = load_session_file(&path).ok().flatten();

        if let Some(session) = session {
            let client = http_client()?;
            let logout_url = format!("{}/api/auth/oidc/logout", session.server);
            let resp = client
                .get(&logout_url)
                .header("Authorization", format!("Bearer {}", session.token))
                .send();

            match resp {
                Ok(r) if r.status().is_success() => {}
                Ok(r) => {
                    eprintln!(
                        "{}",
                        i18n::auth_logout_error(
                            self.lang,
                            &format!("server returned {}", r.status())
                        )
                    );
                }
                Err(e) => {
                    eprintln!("{}", i18n::auth_logout_error(self.lang, &e.to_string()));
                }
            }

            let _ = std::fs::remove_file(&path);
        }

        println!("{}", i18n::auth_logout_success(self.lang));
        Ok(())
    }
}

fn normalize_server(server: &str) -> Result<String> {
    let trimmed = server.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        bail!("server URL must not be empty");
    }
    Ok(trimmed.to_string())
}

fn server_hash(server: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(server.as_bytes());
    hex::encode(&hash[..8])
}

fn session_path(state_dir: &Path, server: &str) -> Result<PathBuf> {
    Ok(state_dir
        .join("pkg-auth")
        .join(format!("{}.json", server_hash(server))))
}

fn save_session_file(path: &Path, server: &str, token: &str) -> Result<()> {
    let session = SessionFile {
        server: server.to_string(),
        token: token.to_string(),
    };
    let json = serde_json::to_string_pretty(&session)?;
    crate::util::atomic_write(path, json.as_bytes())
}

fn load_session_file(path: &Path) -> Result<Option<SessionFile>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read session file {}", path.display()))?;
    let session: SessionFile = serde_json::from_str(&text).context("parse session file")?;
    Ok(Some(session))
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build HTTP client")
}

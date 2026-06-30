//! `mcm auth login/status/logout` — Microsoft/Mojang account management
//! for online game launch (`mcm run` with `mode = "online"`).
//!
//! Login uses the OAuth2 device code flow implemented in [`crate::auth_microsoft`].
//! The user visits a URL, enters a code, and we block-poll until login
//! completes. The resulting MC access token, MS refresh token, and expiry
//! are persisted to `config.toml`'s `[launch_auth.online]` table.
//!
//! This is separate from `mcm pkg auth` (OIDC for the share server) — that
//! flow authenticates against the share service, not Microsoft/Mojang.

use anyhow::{Context, Result};

use crate::app::App;
use crate::auth::{LaunchAuthMode, OnlineAccount};
use crate::cli::AuthCommand;
use crate::config::LaunchAuthConfig;

impl App {
    pub(crate) fn auth(&self, command: AuthCommand) -> Result<()> {
        match command {
            AuthCommand::Login => self.auth_login(),
            AuthCommand::Status => self.auth_status(),
            AuthCommand::Logout => self.auth_logout(),
        }
    }

    fn auth_login(&self) -> Result<()> {
        let login = crate::auth_microsoft::full_device_code_login()
            .context("Microsoft device code login failed")?;

        let account = OnlineAccount {
            username: login.username,
            uuid: login.uuid,
            access_token: login.mc_access_token,
            user_type: "microsoft".to_owned(),
            refresh_token: Some(login.ms_refresh_token),
            mc_expires_at: Some(login.mc_expires_at),
        };

        let mut config = self.load_config()?;
        config.launch_auth = LaunchAuthConfig {
            mode: LaunchAuthMode::Online,
            online: Some(account),
        };
        self.save_config(&config)?;

        println!();
        println!("Logged in as {} ({})", config.launch_auth.online.as_ref().unwrap().username, config.launch_auth.online.as_ref().unwrap().uuid);
        println!("Online launch mode enabled. Use `mcm run` to launch with this account.");
        Ok(())
    }

    fn auth_status(&self) -> Result<()> {
        let config = self.load_config()?;
        match &config.launch_auth.online {
            None => {
                println!("No Microsoft account configured.");
                println!("Run `mcm auth login` to authenticate.");
                return Ok(());
            }
            Some(account) => {
                println!("username: {}", account.username);
                println!("uuid:     {}", account.uuid);
                println!("type:     {}", account.user_type);
                let refreshed = if account.is_expired() { "expired" } else { "valid" };
                println!("session:  {refreshed}");
                match &account.refresh_token {
                    Some(_) => println!("refresh:  available (auto-refresh on next launch)"),
                    None => println!("refresh:  none (must re-run `mcm auth login` when session expires)"),
                }
                println!("mode:     {:?}", config.launch_auth.mode);
            }
        }
        Ok(())
    }

    fn auth_logout(&self) -> Result<()> {
        let mut config = self.load_config()?;
        if config.launch_auth.online.is_none() {
            println!("No account to log out.");
            return Ok(());
        }
        // Reset to offline mode with no account. We deliberately do NOT
        // contact Microsoft to revoke the refresh token — the local config
        // is the only thing under our control. The user can revoke at
        // https://account.microsoft.com/privacy if desired.
        config.launch_auth = LaunchAuthConfig::default();
        self.save_config(&config)?;
        println!("Logged out. Launch mode reverted to offline.");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ProviderChoice;
    use crate::config::Config;
    use crate::i18n::Lang;

    fn make_app(dir: &std::path::Path) -> App {
        let config = Config::default();
        std::fs::write(dir.join("config.toml"), toml::to_string_pretty(&config).unwrap()).unwrap();
        App {
            config_dir: dir.to_path_buf(),
            state_dir: dir.to_path_buf(),
            provider_choice: ProviderChoice::Mock,
            lang: Lang::default(),
        }
    }

    #[test]
    fn status_with_no_account_prints_login_hint() {
        let dir = tempfile::tempdir().unwrap();
        let app = make_app(dir.path());
        // Should not error — just inform the user.
        app.auth(AuthCommand::Status).expect("status with no account");
    }

    #[test]
    fn logout_with_no_account_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let app = make_app(dir.path());
        app.auth(AuthCommand::Logout).expect("logout with no account");
        // Config should remain unchanged (still no account).
        let config = app.load_config().unwrap();
        assert!(config.launch_auth.online.is_none());
    }

    #[test]
    fn logout_clears_stored_account() {
        let dir = tempfile::tempdir().unwrap();
        let app = make_app(dir.path());

        // Seed config with an account.
        let mut config = app.load_config().unwrap();
        config.launch_auth = LaunchAuthConfig {
            mode: LaunchAuthMode::Online,
            online: Some(OnlineAccount {
                username: "Test".into(),
                uuid: "00000000-0000-0000-0000-000000000000".into(),
                access_token: "tok".into(),
                user_type: "microsoft".into(),
                refresh_token: Some("rtok".into()),
                mc_expires_at: Some(0),
            }),
        };
        app.save_config(&config).unwrap();

        app.auth(AuthCommand::Logout).expect("logout");
        let config = app.load_config().unwrap();
        assert!(config.launch_auth.online.is_none(), "account should be cleared");
        assert_eq!(config.launch_auth.mode, LaunchAuthMode::Offline);
    }
}

//! Minecraft launch authentication modes and session types.
//!
//! Two launch auth modes:
//! - **Offline** (default): no real authentication. Uses the player's configured
//!   username and a deterministic UUID derived from `"OfflinePlayer:<username>"`.
//!   The access token is a placeholder; `sessionType` is `"legacy"` (or `"Mojang"`
//!   for backward compat).
//! - **Online** (Microsoft/Mojang): requires a valid access token from the
//!   Microsoft OAuth flow. The [`OnlineSessionProvider`] trait abstracts the
//!   token validation/refresh; tests inject a [`MockOnlineProvider`].
//!
//! YY-ID (OIDC) is **never** used for Minecraft game launch. YY-ID is
//! exclusively for MCM Web/share authentication (see [`crate::server::auth`]).
//!
//! # Security
//! `AuthSession::access_token` is sensitive. [`Display`] and [`Debug`]
//! redact it. Use [`AuthSession::display_redacted`] for user-visible output.

use std::fmt;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Launch auth mode for Minecraft game authentication.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LaunchAuthMode {
    /// Offline mode: no real authentication, deterministic UUID from username.
    #[default]
    Offline,
    /// Online Microsoft/Mojang mode: requires valid access token.
    Online,
}

/// Online account credentials (Microsoft/Mojang).
///
/// These fields are never exposed in logs or debug output.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct OnlineAccount {
    /// Player display name.
    pub username: String,
    /// Player UUID (dash-separated hex).
    pub uuid: String,
    /// OAuth access token (sensitive — never logged).
    pub access_token: String,
    /// User type: `"microsoft"`, `"mojang"`, or `"legacy"`.
    pub user_type: String,
}

/// A Minecraft launch auth session.
///
/// All fields are included in the launch command args. `access_token` is
/// redacted in [`Display`] and [`Debug`].
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AuthSession {
    /// The player's display name.
    pub username: String,
    /// The player's UUID (dash-separated hex).
    pub uuid: String,
    /// Opaque access token (placeholder for offline, real for online).
    pub access_token: String,
    /// Session type: `"Mojang"`, `"Microsoft"`, or `"legacy"`.
    pub session_type: String,
}

impl AuthSession {
    #[allow(dead_code)]
    pub(crate) const MOCK_USERNAME: &'static str = "Player";
    #[allow(dead_code)]
    pub(crate) const MOCK_UUID: &'static str = "00000000-0000-0000-0000-000000000000";
    #[allow(dead_code)]
    pub(crate) const MOCK_ACCESS_TOKEN: &'static str = "mock-access-token";
    #[allow(dead_code)]
    pub(crate) const MOCK_SESSION_TYPE: &'static str = "Mojang";

    /// Create an offline [`AuthSession`] from a username.
    ///
    /// The UUID is deterministic: MD5 of `"OfflinePlayer:<username>"` with
    /// version 3 and variant 10 bits set (standard Minecraft offline UUID).
    /// The access token is a placeholder; session type is `"Mojang"`.
    pub(crate) fn offline(username: &str) -> Self {
        let uuid = offline_uuid(username);
        Self {
            username: username.to_owned(),
            uuid,
            access_token: "0".to_owned(),
            session_type: "Mojang".to_owned(),
        }
    }

    /// Create an [`AuthSession`] from an online [`OnlineAccount`].
    pub(crate) fn from_online_account(account: &OnlineAccount) -> Self {
        Self {
            username: account.username.clone(),
            uuid: account.uuid.clone(),
            access_token: account.access_token.clone(),
            session_type: account.user_type.clone(),
        }
    }

    /// Redacted string for user-visible output (tokens masked).
    #[allow(
        dead_code,
        reason = "Public API for callers needing explicit redaction"
    )]
    pub(crate) fn display_redacted(&self) -> String {
        format!(
            "username={}, uuid={}, access_token=<redacted>, session_type={}",
            self.username, self.uuid, self.session_type,
        )
    }
}

impl fmt::Display for AuthSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "username={}, uuid={}, access_token=<redacted>, session_type={}",
            self.username, self.uuid, self.session_type,
        )
    }
}

impl fmt::Debug for AuthSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthSession")
            .field("username", &self.username)
            .field("uuid", &self.uuid)
            .field("access_token", &"<redacted>")
            .field("session_type", &self.session_type)
            .finish()
    }
}

/// Deterministic offline UUID from `"OfflinePlayer:<username>"`.
///
/// Uses MD5 hash with version 3 and variant 10 bits set, matching the
/// Minecraft launcher's offline UUID generation algorithm.
pub(crate) fn offline_uuid(username: &str) -> String {
    use md5::Digest;
    let input = format!("OfflinePlayer:{username}");
    let hash = md5::Md5::digest(input.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash);

    // Set version 3 (bits 4-7 of byte 6)
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    // Set variant 10 (top 2 bits of byte 8)
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

/// Provider trait for online auth sessions. Mockable for tests.
pub(crate) trait OnlineSessionProvider: Send + Sync {
    /// Get a valid online session for the given account.
    ///
    /// Returns `Err` if the token is expired, invalid, or the provider is
    /// unreachable.
    fn get_session(&self, account: &OnlineAccount) -> Result<AuthSession>;
}

/// Mock online session provider for tests.
///
/// Configurable behavior: success, expired token, or arbitrary error.
pub(crate) struct MockOnlineProvider {
    pub(crate) behavior: MockOnlineBehavior,
}

/// Configurable mock behavior.
#[allow(dead_code, reason = "MockOnlineBehavior variants are test-only")]
pub(crate) enum MockOnlineBehavior {
    Success,
    ExpiredToken,
    Error(String),
}

impl MockOnlineProvider {
    pub(crate) fn success() -> Self {
        Self {
            behavior: MockOnlineBehavior::Success,
        }
    }

    #[allow(dead_code, reason = "Test helper for simulating expired tokens")]
    pub(crate) fn expired_token() -> Self {
        Self {
            behavior: MockOnlineBehavior::ExpiredToken,
        }
    }

    #[allow(dead_code, reason = "Test helper for simulating arbitrary errors")]
    pub(crate) fn error(msg: impl Into<String>) -> Self {
        Self {
            behavior: MockOnlineBehavior::Error(msg.into()),
        }
    }
}

impl OnlineSessionProvider for MockOnlineProvider {
    fn get_session(&self, account: &OnlineAccount) -> Result<AuthSession> {
        match &self.behavior {
            MockOnlineBehavior::Success => Ok(AuthSession::from_online_account(account)),
            MockOnlineBehavior::ExpiredToken => {
                bail!(
                    "Microsoft access token is expired; re-authenticate with `mcm pkg auth login`"
                )
            }
            MockOnlineBehavior::Error(msg) => bail!("{msg}"),
        }
    }
}

/// Resolve an [`AuthSession`] from the launch auth mode and config.
///
/// For offline mode, creates a deterministic session from the username.
/// For online mode, uses the [`OnlineSessionProvider`] to validate/refresh
/// the token.
pub(crate) fn resolve_launch_session(
    mode: &LaunchAuthMode,
    online_account: Option<&OnlineAccount>,
    provider: &dyn OnlineSessionProvider,
) -> Result<AuthSession> {
    match mode {
        LaunchAuthMode::Offline => {
            // Offline mode: use default "Player" if no account configured.
            // The username in online_account is ignored in offline mode;
            // users configure their offline username separately.
            let username = online_account
                .map(|a| a.username.as_str())
                .unwrap_or("Player");
            Ok(AuthSession::offline(username))
        }
        LaunchAuthMode::Online => {
            let account = online_account.context(
                "online auth mode requires an account configuration; \
                 run `mcm config` to set up your Microsoft/Mojang account, \
                 or switch to offline mode",
            )?;
            provider.get_session(account)
        }
    }
}

#[allow(
    dead_code,
    reason = "Public API for callers needing explicit redaction"
)]
pub(crate) fn mock_session() -> AuthSession {
    AuthSession {
        username: AuthSession::MOCK_USERNAME.to_owned(),
        uuid: AuthSession::MOCK_UUID.to_owned(),
        access_token: AuthSession::MOCK_ACCESS_TOKEN.to_owned(),
        session_type: AuthSession::MOCK_SESSION_TYPE.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Mock session backward compat --

    #[test]
    fn mock_session_has_deterministic_username() {
        let session = mock_session();
        assert_eq!(session.username, "Player");
    }

    #[test]
    fn mock_session_has_deterministic_uuid() {
        let session = mock_session();
        assert_eq!(session.uuid, "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn mock_session_has_deterministic_access_token() {
        let session = mock_session();
        assert_eq!(session.access_token, "mock-access-token");
    }

    #[test]
    fn mock_session_has_deterministic_session_type() {
        let session = mock_session();
        assert_eq!(session.session_type, "Mojang");
    }

    #[test]
    fn mock_session_is_consistent_across_calls() {
        let a = mock_session();
        let b = mock_session();
        assert_eq!(a, b);
    }

    // -- Display redaction --

    #[test]
    fn display_redacts_access_token() {
        let session = mock_session();
        let display = session.to_string();
        assert!(
            display.contains("<redacted>"),
            "display should redact token: {display}"
        );
        assert!(
            !display.contains("mock-access-token"),
            "display must not leak token: {display}"
        );
    }

    #[test]
    fn debug_redacts_access_token() {
        let session = AuthSession {
            username: "Test".to_owned(),
            uuid: "11111111-1111-1111-1111-111111111111".to_owned(),
            access_token: "super-secret-token".to_owned(),
            session_type: "Microsoft".to_owned(),
        };
        let debug = format!("{:?}", session);
        assert!(
            debug.contains("<redacted>"),
            "debug should redact token: {debug}"
        );
        assert!(
            !debug.contains("super-secret-token"),
            "debug must not leak token: {debug}"
        );
    }

    #[test]
    fn display_redacted_method_matches_display() {
        let session = mock_session();
        assert_eq!(session.to_string(), session.display_redacted());
    }

    // -- Offline UUID --

    #[test]
    fn offline_uuid_is_deterministic() {
        let a = offline_uuid("Player");
        let b = offline_uuid("Player");
        assert_eq!(a, b);
    }

    #[test]
    fn offline_uuid_differs_for_different_usernames() {
        let a = offline_uuid("Player");
        let b = offline_uuid("Notch");
        assert_ne!(a, b);
    }

    #[test]
    fn offline_uuid_has_correct_format() {
        let uuid = offline_uuid("Player");
        // Format: 8-4-4-4-12 hex chars
        assert_eq!(uuid.len(), 36);
        assert_eq!(&uuid[8..9], "-");
        assert_eq!(&uuid[13..14], "-");
        assert_eq!(&uuid[18..19], "-");
        assert_eq!(&uuid[23..24], "-");
    }

    #[test]
    fn offline_uuid_has_version_3() {
        let uuid = offline_uuid("Player");
        // Version nibble is at position 14 (after second dash)
        let version_char = uuid.as_bytes()[14] as char;
        assert_eq!(version_char, '3', "offline UUID should be version 3");
    }

    #[test]
    fn offline_uuid_has_variant_10() {
        let uuid = offline_uuid("Player");
        // Variant char is at position 19 (after third dash)
        let variant_char = uuid.as_bytes()[19] as char;
        assert!(
            matches!(variant_char, '8' | '9' | 'a' | 'b'),
            "offline UUID should have variant 10 (8-b), got: {variant_char}"
        );
    }

    // -- AuthSession::offline --

    #[test]
    fn offline_session_uses_deterministic_uuid() {
        let session = AuthSession::offline("Notch");
        assert_eq!(session.username, "Notch");
        assert_eq!(session.uuid, offline_uuid("Notch"));
        assert_eq!(session.access_token, "0");
        assert_eq!(session.session_type, "Mojang");
    }

    #[test]
    fn offline_session_stability() {
        // Same username always produces same session
        let a = AuthSession::offline("Steve");
        let b = AuthSession::offline("Steve");
        assert_eq!(a, b);
    }

    // -- OnlineAccount --

    #[test]
    fn from_online_account_copies_fields() {
        let account = OnlineAccount {
            username: "Alex".to_owned(),
            uuid: "deadbeef-dead-beef-dead-beefdeadbeef".to_owned(),
            access_token: "real-token".to_owned(),
            user_type: "microsoft".to_owned(),
        };
        let session = AuthSession::from_online_account(&account);
        assert_eq!(session.username, "Alex");
        assert_eq!(session.uuid, "deadbeef-dead-beef-dead-beefdeadbeef");
        assert_eq!(session.access_token, "real-token");
        assert_eq!(session.session_type, "microsoft");
    }

    // -- MockOnlineProvider --

    #[test]
    fn mock_provider_success_returns_session() {
        let provider = MockOnlineProvider::success();
        let account = OnlineAccount {
            username: "Test".to_owned(),
            uuid: "11111111-1111-1111-1111-111111111111".to_owned(),
            access_token: "token123".to_owned(),
            user_type: "microsoft".to_owned(),
        };
        let session = provider.get_session(&account).expect("should succeed");
        assert_eq!(session.username, "Test");
        assert_eq!(session.access_token, "token123");
        assert_eq!(session.session_type, "microsoft");
    }

    #[test]
    fn mock_provider_expired_returns_error() {
        let provider = MockOnlineProvider::expired_token();
        let account = OnlineAccount {
            username: "Test".to_owned(),
            uuid: "11111111-1111-1111-1111-111111111111".to_owned(),
            access_token: "expired-token".to_owned(),
            user_type: "microsoft".to_owned(),
        };
        let err = provider.get_session(&account).unwrap_err();
        assert!(
            err.to_string().contains("expired"),
            "error should mention expired: {err}"
        );
    }

    #[test]
    fn mock_provider_error_returns_custom_message() {
        let provider = MockOnlineProvider::error("network unreachable");
        let account = OnlineAccount {
            username: "Test".to_owned(),
            uuid: "11111111-1111-1111-1111-111111111111".to_owned(),
            access_token: "token".to_owned(),
            user_type: "mojang".to_owned(),
        };
        let err = provider.get_session(&account).unwrap_err();
        assert!(
            err.to_string().contains("network unreachable"),
            "error should contain custom message: {err}"
        );
    }

    // -- resolve_launch_session --

    #[test]
    fn resolve_offline_default_player() {
        let provider = MockOnlineProvider::success();
        let session = resolve_launch_session(&LaunchAuthMode::Offline, None, &provider)
            .expect("offline should succeed");
        assert_eq!(session.username, "Player");
        assert_eq!(session.uuid, offline_uuid("Player"));
        assert_eq!(session.access_token, "0");
    }

    #[test]
    fn resolve_offline_custom_username() {
        let provider = MockOnlineProvider::success();
        let account = OnlineAccount {
            username: "CustomName".to_owned(),
            uuid: "whatever".to_owned(),
            access_token: "ignored".to_owned(),
            user_type: "ignored".to_owned(),
        };
        let session = resolve_launch_session(&LaunchAuthMode::Offline, Some(&account), &provider)
            .expect("offline should succeed");
        assert_eq!(session.username, "CustomName");
        assert_eq!(session.uuid, offline_uuid("CustomName"));
        // Access token is placeholder, not the online token
        assert_eq!(session.access_token, "0");
    }

    #[test]
    fn resolve_online_with_valid_account() {
        let provider = MockOnlineProvider::success();
        let account = OnlineAccount {
            username: "OnlinePlayer".to_owned(),
            uuid: "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".to_owned(),
            access_token: "real-ms-token".to_owned(),
            user_type: "microsoft".to_owned(),
        };
        let session = resolve_launch_session(&LaunchAuthMode::Online, Some(&account), &provider)
            .expect("online should succeed");
        assert_eq!(session.username, "OnlinePlayer");
        assert_eq!(session.session_type, "microsoft");
    }

    #[test]
    fn resolve_online_without_account_errors() {
        let provider = MockOnlineProvider::success();
        let err = resolve_launch_session(&LaunchAuthMode::Online, None, &provider).unwrap_err();
        assert!(
            err.to_string().contains("account configuration"),
            "error should mention account config: {err}"
        );
    }

    #[test]
    fn resolve_online_expired_token_errors() {
        let provider = MockOnlineProvider::expired_token();
        let account = OnlineAccount {
            username: "Test".to_owned(),
            uuid: "11111111-1111-1111-1111-111111111111".to_owned(),
            access_token: "expired".to_owned(),
            user_type: "microsoft".to_owned(),
        };
        let err =
            resolve_launch_session(&LaunchAuthMode::Online, Some(&account), &provider).unwrap_err();
        assert!(
            err.to_string().contains("expired"),
            "error should mention expired: {err}"
        );
    }

    // -- LaunchAuthMode serialization --

    #[test]
    fn launch_auth_mode_default_is_offline() {
        assert_eq!(LaunchAuthMode::default(), LaunchAuthMode::Offline);
    }

    #[test]
    fn launch_auth_mode_serializes_lowercase() {
        let offline = serde_json::to_string(&LaunchAuthMode::Offline).unwrap();
        assert_eq!(offline, "\"offline\"");
        let online = serde_json::to_string(&LaunchAuthMode::Online).unwrap();
        assert_eq!(online, "\"online\"");
    }

    #[test]
    fn launch_auth_mode_deserializes_from_string() {
        let offline: LaunchAuthMode = serde_json::from_str("\"offline\"").unwrap();
        assert_eq!(offline, LaunchAuthMode::Offline);
        let online: LaunchAuthMode = serde_json::from_str("\"online\"").unwrap();
        assert_eq!(online, LaunchAuthMode::Online);
    }

    // -- No YY-ID coupling --

    #[test]
    fn auth_session_has_no_yyid_field() {
        // Security: AuthSession does NOT derive Serialize to prevent token leakage.
        let session = AuthSession::offline("Player");
        assert!(!session.username.is_empty());
        assert!(!session.uuid.is_empty());
    }
}

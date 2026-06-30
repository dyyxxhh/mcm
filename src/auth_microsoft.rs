//! Real Microsoft / Mojang authentication via OAuth2 device code flow.
//!
//! Flow:
//! 1. `request_device_code` — POST to Microsoft devicecode endpoint, get a
//!    `user_code` + `verification_uri` for the user to visit.
//! 2. `poll_for_token` — poll the token endpoint until the user completes
//!    login or the device code expires. Returns an MS access + refresh token.
//! 3. `exchange_for_xbl` — exchange the MS access token for an Xbox Live
//!    (XBL) token + user hash (uhs).
//! 4. `exchange_for_xsts` — exchange the XBL token for an XSTS token.
//! 5. `exchange_for_mc_token` — exchange XSTS + uhs for a Minecraft access
//!    token.
//! 6. `fetch_profile` — GET the Minecraft profile (username + UUID).
//!
//! Tokens are stored in `OnlineAccount` (config.toml). On launch, if the MC
//! access token is expired, the provider refreshes the MS access token using
//! the stored refresh token and re-runs the XBL/XSTS/MC chain. Refresh tokens
//! do not expire under normal use.
//!
//! # Client ID
//! Uses the well-known public Microsoft client ID `00000000402b5348` by
//! default (same one used by many third-party Minecraft launchers). Override
//! with `MCM_MS_CLIENT_ID`.

use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Default public Microsoft client ID (well-known, used by many MC launchers).
pub(crate) const DEFAULT_MS_CLIENT_ID: &str = "00000000402b5348";

/// Microsoft OAuth2 tenant for consumer accounts.
const MS_TENANT: &str = "consumers";

/// Scope requested for the device code flow. `XboxLive.signin` is required
/// to get a token that can be exchanged for an Xbox Live token.
const MS_SCOPE: &str = "XboxLive.signin offline_access";

/// HTTP client timeout for auth requests.
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

fn client_id() -> String {
    std::env::var("MCM_MS_CLIENT_ID").unwrap_or_else(|_| DEFAULT_MS_CLIENT_ID.to_owned())
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent("mcm/0.2.0 (Minecraft manager)")
        .build()
        .context("build http client for microsoft auth")
}

// ---------------------------------------------------------------------------
// Step 1: device code request
// ---------------------------------------------------------------------------

/// Response from the Microsoft device code endpoint.
#[derive(Debug, Deserialize)]
pub(crate) struct DeviceCodeResponse {
    pub(crate) device_code: String,
    pub(crate) user_code: String,
    pub(crate) verification_uri: String,
    pub(crate) expires_in: u64,
    pub(crate) interval: u64,
    /// Human-readable message Microsoft provides (shown to the user).
    pub(crate) message: Option<String>,
}

/// Request a device code from Microsoft. The user must visit
/// `verification_uri` and enter `user_code` within `expires_in` seconds.
pub(crate) fn request_device_code() -> Result<DeviceCodeResponse> {
    let client = http_client()?;
    let url = format!(
        "https://login.microsoftonline.com/{MS_TENANT}/oauth2/v2.0/devicecode"
    );
    let params = [
        ("client_id", client_id()),
        ("scope", MS_SCOPE.to_owned()),
    ];
    let resp = client
        .post(&url)
        .form(&params)
        .send()
        .context("send device code request")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        bail!("device code request failed ({status}): {body}");
    }
    resp.json::<DeviceCodeResponse>()
        .context("parse device code response")
}

// ---------------------------------------------------------------------------
// Step 2: poll for token
// ---------------------------------------------------------------------------

/// Result of polling the token endpoint.
pub(crate) enum PollOutcome {
    /// Login complete — MS access + refresh tokens obtained.
    Complete(MicrosoftTokens),
    /// User must authorize at the verification URI.
    Pending,
    /// The device code has expired.
    Expired,
    /// The user denied the request.
    Denied(String),
}

/// MS OAuth2 token response.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
    /// Error fields (present when the request hasn't completed yet).
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

/// Microsoft OAuth2 tokens (the result of a successful login).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MicrosoftTokens {
    pub(crate) access_token: String,
    pub(crate) refresh_token: String,
    /// Absolute expiry (unix seconds).
    pub(crate) expires_at: i64,
}

/// Poll the token endpoint once for a completed login.
pub(crate) fn poll_for_token(device_code: &str) -> Result<PollOutcome> {
    let client = http_client()?;
    let url = format!("https://login.microsoftonline.com/{MS_TENANT}/oauth2/v2.0/token");
    let params = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code".to_owned()),
        ("client_id", client_id()),
        ("device_code", device_code.to_owned()),
    ];
    let resp = client
        .post(&url)
        .form(&params)
        .send()
        .context("send token poll request")?;
    // Microsoft returns 400 with an error body while pending, so we parse the
    // JSON body regardless of status and inspect the error field.
    let body: TokenResponse = resp
        .json()
        .context("parse token poll response")?;
    if let Some(err) = &body.error {
        return Ok(match err.as_str() {
            "authorization_pending" => PollOutcome::Pending,
            "slow_down" => PollOutcome::Pending,
            "expired_token" => PollOutcome::Expired,
            "authorization_declined" => PollOutcome::Denied(
                body.error_description
                    .unwrap_or_else(|| "user declined".to_owned()),
            ),
            other => bail!(
                "token poll error: {other} — {}",
                body.error_description.unwrap_or_default()
            ),
        });
    }
    if body.access_token.is_empty() {
        return Ok(PollOutcome::Pending);
    }
    let expires_at = now_unix() + body.expires_in as i64;
    Ok(PollOutcome::Complete(MicrosoftTokens {
        access_token: body.access_token,
        refresh_token: body.refresh_token,
        expires_at,
    }))
}

/// Block-poll the token endpoint until the user completes login, the device
/// code expires, or `timeout` elapses. Prints the verification URI/message to
/// stdout so the user knows where to go.
pub(crate) fn poll_until_complete(device_code: &DeviceCodeResponse) -> Result<MicrosoftTokens> {
    let deadline = Instant::now() + Duration::from_secs(device_code.expires_in);
    let interval = Duration::from_secs(device_code.interval.max(1));
    loop {
        if Instant::now() >= deadline {
            bail!("device code expired before login completed");
        }
        match poll_for_token(&device_code.device_code)? {
            PollOutcome::Complete(tokens) => return Ok(tokens),
            PollOutcome::Pending => std::thread::sleep(interval),
            PollOutcome::Expired => bail!("device code expired before login completed"),
            PollOutcome::Denied(reason) => bail!("login denied: {reason}"),
        }
    }
}

/// Refresh the MS access token using a stored refresh token. Returns new
/// tokens (including a fresh refresh token). Errors if the refresh token is
/// invalid or revoked.
pub(crate) fn refresh_tokens(refresh_token: &str) -> Result<MicrosoftTokens> {
    let client = http_client()?;
    let url = format!("https://login.microsoftonline.com/{MS_TENANT}/oauth2/v2.0/token");
    let params = [
        ("grant_type", "refresh_token".to_owned()),
        ("client_id", client_id()),
        ("refresh_token", refresh_token.to_owned()),
        ("scope", MS_SCOPE.to_owned()),
    ];
    let resp = client
        .post(&url)
        .form(&params)
        .send()
        .context("send refresh request")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        bail!("token refresh failed ({status}): {body}");
    }
    let body: TokenResponse = resp.json().context("parse refresh response")?;
    let expires_at = now_unix() + body.expires_in as i64;
    Ok(MicrosoftTokens {
        access_token: body.access_token,
        refresh_token: body.refresh_token,
        expires_at,
    })
}

// ---------------------------------------------------------------------------
// Step 3: Xbox Live token
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct XblResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XblDisplayClaims,
}

#[derive(Debug, Deserialize)]
struct XblDisplayClaims {
    #[serde(rename = "xui")]
    xui: Vec<XblUserHash>,
}

#[derive(Debug, Deserialize)]
struct XblUserHash {
    #[serde(rename = "uhs")]
    uhs: String,
}

/// Exchange the MS access token for an Xbox Live token + user hash.
pub(crate) fn exchange_for_xbl(ms_access_token: &str) -> Result<(String, String)> {
    let client = http_client()?;
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={ms_access_token}")
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });
    let resp = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .context("send xbox live authenticate request")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        bail!("xbox live authenticate failed ({status}): {text}");
    }
    let parsed: XblResponse = resp.json().context("parse xbox live response")?;
    let uhs = parsed
        .display_claims
        .xui
        .into_iter()
        .next()
        .map(|x| x.uhs)
        .ok_or_else(|| anyhow!("xbox live response missing uhs"))?;
    Ok((parsed.token, uhs))
}

// ---------------------------------------------------------------------------
// Step 4: XSTS token
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct XstsResponse {
    #[serde(rename = "Token")]
    token: String,
}

/// Exchange the XBL token for an XSTS token (scoped to Minecraft).
pub(crate) fn exchange_for_xsts(xbl_token: &str) -> Result<String> {
    let client = http_client()?;
    let body = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token]
        },
        "RelyingParty": "rp://api.minecraft.com",
        "TokenType": "JWT"
    });
    let resp = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .context("send xsts authorize request")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        // 401 with XERR suffix usually means no Xbox account linked.
        if text.contains("2148916233") {
            bail!(
                "this Microsoft account does not have an Xbox profile; \
                 create one at https://account.xbox.com before logging in"
            );
        }
        bail!("xsts authorize failed ({status}): {text}");
    }
    let parsed: XstsResponse = resp.json().context("parse xsts response")?;
    Ok(parsed.token)
}

// ---------------------------------------------------------------------------
// Step 5: Minecraft access token
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct McTokenResponse {
    #[serde(default)]
    access_token: String,
    /// Microsoft sometimes returns 401 with this JSON shape instead.
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    error_message: Option<String>,
}

/// Exchange XSTS + uhs for a Minecraft access token.
pub(crate) fn exchange_for_mc_token(xsts_token: &str, uhs: &str) -> Result<String> {
    let client = http_client()?;
    let body = serde_json::json!({
        "identityToken": format!("XBL3.0 x={uhs};{xsts_token}")
    });
    let resp = client
        .post("https://api.minecraft.com/authenticate/xbox")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .context("send minecraft authenticate request")?;
    let status = resp.status();
    let parsed: McTokenResponse = resp.json().context("parse minecraft token response")?;
    if !status.is_success() || parsed.access_token.is_empty() {
        let err = parsed
            .error
            .or_else(|| parsed.error_message)
            .unwrap_or_else(|| format!("minecraft authenticate failed ({status})"));
        bail!("minecraft authenticate failed: {err}");
    }
    Ok(parsed.access_token)
}

// ---------------------------------------------------------------------------
// Step 6: Minecraft profile (username + UUID)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct McProfile {
    pub(crate) id: String,
    pub(crate) name: String,
}

/// Fetch the Minecraft profile (username + UUID) using a MC access token.
pub(crate) fn fetch_profile(mc_access_token: &str) -> Result<McProfile> {
    let client = http_client()?;
    let resp = client
        .get("https://api.minecraft.com/profile")
        .header("Authorization", format!("Bearer {mc_access_token}"))
        .send()
        .context("send minecraft profile request")?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        bail!("minecraft profile fetch failed ({status}): {text}");
    }
    resp.json::<McProfile>()
        .context("parse minecraft profile response")
}

// ---------------------------------------------------------------------------
// Full login orchestration: device code → tokens → MC session
// ---------------------------------------------------------------------------

/// Result of a successful full login: everything needed to construct an
/// `OnlineAccount` and a launch `AuthSession`.
pub(crate) struct FullLogin {
    pub(crate) username: String,
    /// Dash-separated UUID (canonical form).
    pub(crate) uuid: String,
    pub(crate) mc_access_token: String,
    pub(crate) ms_refresh_token: String,
    pub(crate) mc_expires_at: i64,
}

/// Run the full device code login flow. Blocks until the user completes
/// browser login. Prints instructions to stdout.
pub(crate) fn full_device_code_login() -> Result<FullLogin> {
    let dc = request_device_code()?;
    println!();
    if let Some(msg) = &dc.message {
        println!("{msg}");
    } else {
        println!("Open {} and enter code: {}", dc.verification_uri, dc.user_code);
    }
    println!("(waiting for login, expires in {}s)", dc.expires_in);
    println!();

    let ms_tokens = poll_until_complete(&dc)?;

    let (xbl_token, uhs) = exchange_for_xbl(&ms_tokens.access_token)?;
    let xsts_token = exchange_for_xsts(&xbl_token)?;
    let mc_access_token = exchange_for_mc_token(&xsts_token, &uhs)?;
    let profile = fetch_profile(&mc_access_token)?;

    Ok(FullLogin {
        username: profile.name,
        uuid: format_uuid(&profile.id),
        mc_access_token,
        ms_refresh_token: ms_tokens.refresh_token,
        // MC access tokens last ~24h; refresh the MS token at expiry.
        mc_expires_at: ms_tokens.expires_at,
    })
}

/// Refresh an expired Minecraft session using the stored MS refresh token.
/// Re-runs the XBL/XSTS/MC chain. Returns the refreshed MC access token +
/// new expiry + (possibly rotated) MS refresh token.
pub(crate) fn refresh_mc_session(
    ms_refresh_token: &str,
) -> Result<(String, i64, String)> {
    let ms_tokens = refresh_tokens(ms_refresh_token)?;
    let (xbl_token, uhs) = exchange_for_xbl(&ms_tokens.access_token)?;
    let xsts_token = exchange_for_xsts(&xbl_token)?;
    let mc_access_token = exchange_for_mc_token(&xsts_token, &uhs)?;
    Ok((
        mc_access_token,
        ms_tokens.expires_at,
        ms_tokens.refresh_token,
    ))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Format a 32-char hex string into the standard 8-4-4-4-12 dashed UUID form.
/// Mojang's profile API returns the un-dashed form; we normalize for launch.
fn format_uuid(raw: &str) -> String {
    let raw = raw.replace('-', "");
    if raw.len() != 32 {
        return raw.to_owned();
    }
    format!(
        "{}-{}-{}-{}-{}",
        &raw[0..8],
        &raw[8..12],
        &raw[12..16],
        &raw[16..20],
        &raw[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_uuid_dashes_correctly() {
        let input = "069a79f444e94726a5befca90e38aaf5";
        assert_eq!(
            format_uuid(input),
            "069a79f4-44e9-4726-a5be-fca90e38aaf5"
        );
    }

    #[test]
    fn format_uuid_passes_through_already_dashed() {
        let input = "069a79f4-44e9-4726-a5be-fca90e38aaf5";
        assert_eq!(format_uuid(input), input);
    }

    #[test]
    fn format_uuid_preserves_short_input() {
        assert_eq!(format_uuid("abc"), "abc");
    }

    #[test]
    fn client_id_default_is_well_known() {
        std::env::remove_var("MCM_MS_CLIENT_ID");
        assert_eq!(client_id(), DEFAULT_MS_CLIENT_ID);
    }

    #[test]
    fn client_id_env_override() {
        let prev = std::env::var("MCM_MS_CLIENT_ID");
        std::env::set_var("MCM_MS_CLIENT_ID", "test-id-123");
        assert_eq!(client_id(), "test-id-123");
        match prev {
            Ok(v) => std::env::set_var("MCM_MS_CLIENT_ID", v),
            Err(_) => std::env::remove_var("MCM_MS_CLIENT_ID"),
        }
    }
}

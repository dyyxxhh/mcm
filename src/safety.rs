use std::io::Read;
use std::net::IpAddr;

use anyhow::{bail, Context, Result};

use crate::confirmation::{classify, prompt_yes_no, ConfirmationPolicy, OperationKind};
use crate::i18n;

pub(crate) const DOWNLOAD_HOST_ALLOWLIST: &[&str] = &["cdn.modrinth.com", "edge.forgecdn.net"];

pub(crate) fn sanitize_filename(name: &str) -> Result<String> {
    let lang = i18n::Lang::default();
    if name.is_empty()
        || name.contains('\0')
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.starts_with('/')
        || has_windows_drive_prefix(name)
        || !name.ends_with(".jar")
        || has_unsafe_jar_stem(name)
    {
        bail!("{}", i18n::unsafe_filename(lang, name));
    }
    Ok(name.to_owned())
}

fn has_windows_drive_prefix(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn has_unsafe_jar_stem(name: &str) -> bool {
    let stem = name.strip_suffix(".jar").unwrap_or(name);
    stem.is_empty() || stem.starts_with('.') || is_windows_reserved_name(stem)
}

fn is_windows_reserved_name(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || upper
            .strip_prefix("COM")
            .and_then(|suffix| suffix.parse::<u8>().ok())
            .is_some_and(|value| (1..=9).contains(&value))
        || upper
            .strip_prefix("LPT")
            .and_then(|suffix| suffix.parse::<u8>().ok())
            .is_some_and(|value| (1..=9).contains(&value))
}

pub(crate) fn validate_download_url(url: &str) -> Result<()> {
    let lang = i18n::Lang::default();
    let parsed = reqwest::Url::parse(url).with_context(|| i18n::invalid_download_url(lang, url))?;
    if parsed.scheme() != "https" {
        bail!("{}", i18n::download_url_must_use_https(lang, url));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        bail!("{}", i18n::download_url_no_credentials(lang, url));
    }
    let host = parsed
        .host_str()
        .with_context(|| i18n::download_url_no_host(lang, url))?;
    let ip_host = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = ip_host.parse::<IpAddr>() {
        if is_blocked_ip(ip) {
            bail!("{}", i18n::download_url_host_private(lang, host));
        }
    }
    if !DOWNLOAD_HOST_ALLOWLIST
        .iter()
        .any(|allowed| host.eq_ignore_ascii_case(allowed))
    {
        bail!("{}", i18n::download_url_host_not_in_allowlist(lang, host));
    }
    Ok(())
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.octets()[0] == 0
        }
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local(),
    }
}

/// Interactive install confirmation. Routes through the centralized
/// confirmation policy: `Install` is `Bypassable` and uses a `[y/N]` prompt
/// in interactive mode. Kept as a thin wrapper so existing callers
/// (`lifecycle::install`) preserve the exact `Proceed with install? [y/N]`
/// prompt text that characterization tests assert on.
pub(crate) fn confirm_install() -> Result<bool> {
    match classify(OperationKind::Install) {
        ConfirmationPolicy::Harmless => Ok(true),
        ConfirmationPolicy::Bypassable => prompt_yes_no("Proceed with install? [y/N]"),
        ConfirmationPolicy::NonBypassable => Ok(false),
    }
}

// Used by jar_info to read jar entries; kept here so `safety` owns all
// low-level IO helpers. `Read` is required for `zip::ZipFile`.
#[allow(dead_code)]
pub(crate) fn read_to_string<R: Read>(reader: &mut R) -> Result<String> {
    let mut text = String::new();
    reader.read_to_string(&mut text)?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_filename_rejects_traversal_and_non_jars() {
        assert_eq!(
            sanitize_filename("safe-mod.jar").expect("safe jar"),
            "safe-mod.jar"
        );
        for name in [
            "",
            "../evil.jar",
            "nested/evil.jar",
            r"nested\\evil.jar",
            "/evil.jar",
            "C:evil.jar",
            "mod.txt",
        ] {
            assert!(
                sanitize_filename(name).is_err(),
                "{name} should be rejected"
            );
        }
    }

    #[test]
    fn sanitize_filename_rejects_null_drive_and_reserved_names() {
        assert_eq!(
            sanitize_filename("normal-mod-1.0.0.jar").expect("normal jar"),
            "normal-mod-1.0.0.jar"
        );
        for name in [
            "foo\0.jar",
            "D:foo.jar",
            "Z:foo.jar",
            ".jar",
            ".hidden.jar",
            "CON.jar",
            "NUL.jar",
            "COM1.jar",
            "LPT1.jar",
        ] {
            assert!(
                sanitize_filename(name).is_err(),
                "{name:?} should be rejected"
            );
        }
    }

    #[test]
    fn validate_download_url_requires_https_and_rejects_private_hosts() {
        assert!(validate_download_url("https://cdn.modrinth.com/mod.jar").is_ok());
        for url in [
            "http://cdn.modrinth.com/mod.jar",
            "mock://rootmod",
            "https://127.0.0.1/mod.jar",
            "https://10.0.0.1/mod.jar",
            "https://172.16.0.1/mod.jar",
            "https://192.168.1.1/mod.jar",
            "https://[::1]/mod.jar",
        ] {
            assert!(
                validate_download_url(url).is_err(),
                "{url} should be rejected"
            );
        }
    }

    #[test]
    fn validate_download_url_allowlists_only_known_cdn_hosts() {
        assert!(
            validate_download_url("https://cdn.modrinth.com/data/abc/versions/1.0.0/mod.jar")
                .is_ok(),
            "cdn.modrinth.com should be accepted"
        );
        assert!(
            validate_download_url("https://edge.forgecdn.net/files/1234/567/mod.jar").is_ok(),
            "edge.forgecdn.net should be accepted"
        );
        assert!(
            validate_download_url("https://evil.example.com/mod.jar").is_err(),
            "evil.example.com should be rejected"
        );
        assert!(
            validate_download_url("https://cdn.modrinth.com.evil.example/mod.jar").is_err(),
            "cdn.modrinth.com.evil.example should be rejected (subdomain takeover)"
        );
        assert!(
            validate_download_url("https://CDN.MODRINTH.COM:8443@evil.example/mod.jar").is_err(),
            "userinfo trick with @evil.example should be rejected"
        );
        assert!(
            validate_download_url("http://cdn.modrinth.com/mod.jar").is_err(),
            "non-https should still be rejected"
        );
    }
}

//! HTTP-based fetcher using `reqwest` blocking with Range/resume support.

use std::time::Duration;

use super::{FetchError, FetchOutcome, Fetcher, RangeServed};

/// HTTP-based fetcher using `reqwest` blocking. Supports Range/resume and
/// categorizes status codes into transient/permanent errors.
pub struct HttpFetcher {
    url: String,
    client: reqwest::blocking::Client,
}

impl HttpFetcher {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            client: reqwest::blocking::Client::builder()
                .redirect(reqwest::redirect::Policy::limited(5))
                .build()
                .expect("HTTP client"),
        }
    }
}

impl Fetcher for HttpFetcher {
    fn url(&self) -> &str {
        &self.url
    }

    fn fetch(&self, range_start: Option<u64>) -> std::result::Result<FetchOutcome, FetchError> {
        let mut req = self
            .client
            .get(&self.url)
            .header("User-Agent", "mcm/0.1.0 (Minecraft mod manager)");
        if let Some(start) = range_start {
            req = req.header("Range", format!("bytes={start}-"));
        }
        let response = req
            .send()
            .map_err(|e| FetchError::Transient(format!("send: {e}"), None))?;
        let status = response.status();
        match status.as_u16() {
            200 | 206 => {
                let total = response
                    .headers()
                    .get("Content-Range")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.split('/').nth(1))
                    .and_then(|v| v.parse().ok())
                    .or_else(|| {
                        response
                            .headers()
                            .get("Content-Length")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse().ok())
                    });
                let bytes = response
                    .bytes()
                    .map_err(|e| FetchError::Transient(format!("read body: {e}"), None))?
                    .to_vec();
                let served = if status.as_u16() == 206 {
                    RangeServed::PartialFrom(range_start.unwrap_or(0))
                } else {
                    RangeServed::Full
                };
                Ok(FetchOutcome {
                    bytes,
                    total,
                    served,
                })
            }
            429 => {
                let retry_after = response
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok());
                Err(FetchError::Transient(
                    "429 Too Many Requests".to_owned(),
                    retry_after.map(Duration::from_secs),
                ))
            }
            s if (500..600).contains(&s) => Err(FetchError::Transient(format!("HTTP {s}"), None)),
            s => Err(FetchError::Permanent(format!("HTTP {s}"))),
        }
    }
}

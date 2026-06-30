//! Retryable, resumable download engine.
//!
//! Public API: [`download_file`] — downloads to a destination path with
//! backoff retries, HTTP Range resume, hash/size validation, and staged
//! atomic finalize (`.part` → rename). The byte source is abstracted via
//! the [`Fetcher`] trait so mock providers can inject deterministic bytes
//! without real HTTP.
//!
//! Reused by: mod jar downloads, package asset URL fetches, and (future)
//! game/Java/runtime/installer downloads (tasks 20, 21, 17).

mod http;
mod provider_fetcher;

pub use http::HttpFetcher;
pub(crate) use provider_fetcher::ProviderFetcher;

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Result};
use sha2::{Digest, Sha256};

/// Maximum retry attempts (total requests = 1 + retries).
const MAX_ATTEMPTS: u32 = 5;
/// Base backoff in milliseconds; doubles each attempt.
const BACKOFF_BASE_MS: u64 = 1000;
/// Maximum backoff cap.
const BACKOFF_MAX_MS: u64 = 30_000;

/// Abstracts a single fetch attempt. Implemented by [`HttpFetcher`] for real
/// HTTP and by [`ProviderFetcher`] for mock/in-memory bytes.
pub trait Fetcher {
    fn url(&self) -> &str;
    /// Fetch content. If `range_start` is `Some(n)`, the fetcher SHOULD try
    /// to resume from byte `n` (HTTP Range). The outcome reports whether the
    /// server served partial or full content.
    fn fetch(&self, range_start: Option<u64>) -> std::result::Result<FetchOutcome, FetchError>;
}

/// Error from a fetch attempt. [`Transient`](FetchError::Transient) errors
/// are retried with backoff; [`Permanent`](FetchError::Permanent) errors
/// abort immediately.
#[derive(Debug)]
pub enum FetchError {
    /// Retryable: 5xx, 429, connection reset, timeout. The optional
    /// `Duration` overrides the computed backoff (e.g. `Retry-After`).
    Transient(String, Option<Duration>),
    /// Non-retryable: 4xx (except 429), hash mismatch, size mismatch.
    Permanent(String),
}

/// Outcome of a successful single fetch.
pub struct FetchOutcome {
    /// Bytes received in this fetch (full body or partial slice).
    pub bytes: Vec<u8>,
    /// Total resource size if known (from `Content-Range` or `Content-Length`).
    pub total: Option<u64>,
    /// How the server served the content.
    pub served: RangeServed,
}

/// How the server served the content for this fetch.
#[derive(Debug, Clone, Copy)]
pub enum RangeServed {
    /// Full content from byte 0 (HTTP 200 OK).
    Full,
    /// Partial content starting at `offset` (HTTP 206).
    PartialFrom(u64),
}

/// Progress callback: `(bytes_so_far, total_bytes)`.
pub type ProgressFn = dyn Fn(usize, Option<usize>);

/// Options for [`download_file`].
pub struct DownloadOptions {
    /// Expected SHA-256 hex digest. If set, the downloaded file is verified
    /// and a mismatch is a permanent error (`.part` deleted, no finalize).
    pub expected_sha256: Option<String>,
    /// Expected file size in bytes. If set, size mismatch is permanent.
    pub expected_size: Option<u64>,
    /// Maximum total attempts (1 + retries). Default 5.
    pub max_attempts: u32,
    /// Base backoff in ms; doubles each attempt, capped at 30s. Default 1000.
    /// Tests set this low (e.g. 1) for speed.
    pub backoff_base_ms: u64,
    /// Optional progress callback: `(bytes_so_far, total_bytes)`.
    pub progress: Option<Box<ProgressFn>>,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            expected_sha256: None,
            expected_size: None,
            max_attempts: MAX_ATTEMPTS,
            backoff_base_ms: BACKOFF_BASE_MS,
            progress: None,
        }
    }
}

/// Result of a successful download.
#[derive(Debug)]
pub struct DownloadOutcome {
    pub bytes_written: u64,
    pub final_path: PathBuf,
    /// SHA-256 hex digest of the finalized file.
    pub sha256: String,
}

/// Download a file using `fetcher`, writing to `<dest>.part` then atomically
/// renaming to `dest`. Retries transient errors with exponential backoff.
/// Resumes from an existing `.part` if the server supports Range. Validates
/// hash/size. On permanent failure, removes the `.part` file.
///
/// Confirmation: this function does NOT prompt. Confirmation happens ONCE at
/// the operation level (e.g. `pkg install` confirms before calling this).
pub fn download_file(
    dest: &Path,
    fetcher: &dyn Fetcher,
    opts: &DownloadOptions,
) -> Result<DownloadOutcome> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let part = part_path(dest);
    let mut last_error: Option<String> = None;
    for attempt in 1..=opts.max_attempts {
        match try_download(fetcher, opts, &part, dest) {
            Ok(outcome) => return Ok(outcome),
            Err(FetchError::Permanent(msg)) => {
                let _ = fs::remove_file(&part);
                bail!(
                    "download failed permanently (attempt {attempt}/{max}): {msg}",
                    max = opts.max_attempts
                );
            }
            Err(FetchError::Transient(msg, retry_after)) => {
                last_error = Some(msg);
                if attempt < opts.max_attempts {
                    let delay = retry_after.unwrap_or_else(|| {
                        Duration::from_millis(backoff_delay(attempt, opts.backoff_base_ms))
                    });
                    std::thread::sleep(delay);
                }
            }
        }
    }
    let _ = fs::remove_file(&part);
    bail!(
        "download failed after {attempts} attempts (url {url}): {err}",
        attempts = opts.max_attempts,
        url = fetcher.url(),
        err = last_error.unwrap_or_else(|| "unknown".to_owned())
    );
}

fn try_download(
    fetcher: &dyn Fetcher,
    opts: &DownloadOptions,
    part: &Path,
    dest: &Path,
) -> std::result::Result<DownloadOutcome, FetchError> {
    let existing = fs::metadata(part).map(|m| m.len()).ok();
    let outcome = fetcher.fetch(existing)?;
    let mut file = match outcome.served {
        RangeServed::Full => OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(part)
            .map_err(|e| FetchError::Transient(format!("open .part: {e}"), None))?,
        RangeServed::PartialFrom(offset) => {
            if existing == Some(offset) {
                OpenOptions::new()
                    .append(true)
                    .open(part)
                    .map_err(|e| FetchError::Transient(format!("open .part append: {e}"), None))?
            } else {
                let _ = fs::remove_file(part);
                return Err(FetchError::Transient(
                    format!("range offset mismatch: expected {existing:?}, got {offset}"),
                    None,
                ));
            }
        }
    };
    let mut written = existing.unwrap_or(0);
    for chunk in outcome.bytes.chunks(8192) {
        file.write_all(chunk)
            .map_err(|e| FetchError::Transient(format!("write .part: {e}"), None))?;
        written += chunk.len() as u64;
        if let Some(ref progress) = opts.progress {
            progress(written as usize, outcome.total.map(|t| t as usize));
        }
    }
    file.sync_all().ok();
    drop(file);

    let actual_hash =
        sha256_hex_file(part).map_err(|e| FetchError::Permanent(format!("hash read: {e}")))?;
    if let Some(ref expected) = opts.expected_sha256 {
        if &actual_hash != expected {
            let _ = fs::remove_file(part);
            return Err(FetchError::Permanent(format!(
                "hash mismatch: expected {expected}, got {actual_hash}"
            )));
        }
    }
    if let Some(expected_size) = opts.expected_size {
        let actual_size = fs::metadata(part).map(|m| m.len()).unwrap_or(0);
        if actual_size != expected_size {
            let _ = fs::remove_file(part);
            return Err(FetchError::Permanent(format!(
                "size mismatch: expected {expected_size}, got {actual_size}"
            )));
        }
    }
    fs::rename(part, dest).map_err(|e| FetchError::Transient(format!("rename: {e}"), None))?;
    Ok(DownloadOutcome {
        bytes_written: written,
        final_path: dest.to_path_buf(),
        sha256: actual_hash,
    })
}

fn sha256_hex_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    Ok(hex::encode(Sha256::digest(&bytes)))
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

fn backoff_delay(attempt: u32, base_ms: u64) -> u64 {
    (base_ms * 2u64.pow(attempt.saturating_sub(1))).min(BACKOFF_MAX_MS)
}

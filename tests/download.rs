//! Integration tests for the download engine.
//!
//! Uses a hand-rolled `TcpListener` HTTP/1.1 server — no mock-server crate.
//! Covers: success with hash, flaky-then-success (retries), permanent hash
//! mismatch (no finalized file), resume from `.part`, permanent failure
//! (always 500, no partial file).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;

use mcm::download::{
    download_file, DownloadOptions, FetchError, FetchOutcome, Fetcher, HttpFetcher, RangeServed,
};

/// A minimal HTTP/1.1 test server. Configurable via `ServerConfig`.
struct TestServer {
    addr: std::net::SocketAddr,
    _handle: thread::JoinHandle<()>,
}

struct ServerConfig {
    body: Vec<u8>,
    /// Fail the first N requests with 500, then succeed.
    fail_first: u32,
    /// Always return 206 Partial Content if a Range header is present.
    support_range: bool,
    /// Always return wrong body (for hash mismatch test).
    wrong_body: bool,
}

impl TestServer {
    fn start(cfg: Arc<ServerConfig>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let handle = thread::spawn(move || {
            let counter = AtomicU32::new(0);
            for stream in listener.incoming() {
                let stream = match stream {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let cfg = Arc::clone(&cfg);
                let count = counter.fetch_add(1, Ordering::SeqCst);
                handle_request(stream, cfg, count);
                if counter.load(Ordering::SeqCst) > 100 {
                    break;
                }
            }
        });
        Self {
            addr,
            _handle: handle,
        }
    }

    fn url(&self) -> String {
        format!("http://{}/file", self.addr)
    }
}

fn handle_request(mut stream: TcpStream, cfg: Arc<ServerConfig>, count: u32) {
    let mut buf = [0u8; 1024];
    let _ = stream.read(&mut buf);
    let req = String::from_utf8_lossy(&buf);
    let range_start: Option<u64> = req
        .lines()
        .find(|l| l.to_lowercase().starts_with("range:"))
        .and_then(|l| l.split(':').nth(1))
        .and_then(|v| v.trim().strip_prefix("bytes="))
        .and_then(|v| v.split('-').next())
        .and_then(|v| v.parse().ok());

    if count < cfg.fail_first {
        let _ =
            stream.write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n");
        return;
    }

    if cfg.wrong_body {
        let body = b"wrong content";
        let resp = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
        let _ = stream.write_all(resp.as_bytes());
        let _ = stream.write_all(body);
        return;
    }

    let body = &cfg.body;
    if cfg.support_range {
        if let Some(start_u64) = range_start {
            let start = start_u64 as usize;
            if start >= body.len() {
                let _ = stream
                    .write_all(b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\n\r\n");
                return;
            }
            let slice = &body[start..];
            let resp = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\n\r\n",
                slice.len(),
                start,
                body.len() - 1,
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.write_all(slice);
            return;
        }
    }
    let resp = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len());
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.write_all(body);
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

#[test]
fn successful_download_writes_correct_file_with_hash_match() {
    let body = b"hello mcm download engine";
    let cfg = Arc::new(ServerConfig {
        body: body.to_vec(),
        fail_first: 0,
        support_range: false,
        wrong_body: false,
    });
    let server = TestServer::start(cfg);
    let tmp = tempfile::tempdir().expect("tmp");
    let dest = tmp.path().join("out.bin");
    let fetcher = HttpFetcher::new(&server.url());
    let opts = DownloadOptions {
        expected_sha256: Some(sha256_hex(body)),
        expected_size: Some(body.len() as u64),
        ..Default::default()
    };
    let outcome = download_file(&dest, &fetcher, &opts).expect("download ok");
    assert_eq!(outcome.bytes_written, body.len() as u64);
    assert_eq!(outcome.sha256, sha256_hex(body));
    let written = std::fs::read(&dest).expect("read dest");
    assert_eq!(written, body);
    assert!(!dest.with_extension("bin.part").exists());
}

#[test]
fn flaky_server_fails_first_n_then_succeeds_on_retry() {
    let body = b"retried content";
    let cfg = Arc::new(ServerConfig {
        body: body.to_vec(),
        fail_first: 2,
        support_range: false,
        wrong_body: false,
    });
    let server = TestServer::start(cfg);
    let tmp = tempfile::tempdir().expect("tmp");
    let dest = tmp.path().join("retry.bin");
    let fetcher = HttpFetcher::new(&server.url());
    let opts = DownloadOptions {
        backoff_base_ms: 1,
        ..Default::default()
    };
    download_file(&dest, &fetcher, &opts).expect("download ok after retries");
    let written = std::fs::read(&dest).expect("read dest");
    assert_eq!(written, body);
}

#[test]
fn permanent_hash_mismatch_deletes_part_and_errors() {
    let cfg = Arc::new(ServerConfig {
        body: b"actual content".to_vec(),
        fail_first: 0,
        support_range: false,
        wrong_body: false,
    });
    let server = TestServer::start(cfg);
    let tmp = tempfile::tempdir().expect("tmp");
    let dest = tmp.path().join("hashmismatch.bin");
    let fetcher = HttpFetcher::new(&server.url());
    let opts = DownloadOptions {
        expected_sha256: Some(
            "0000000000000000000000000000000000000000000000000000000000000000".to_owned(),
        ),
        backoff_base_ms: 1,
        ..Default::default()
    };
    let err = download_file(&dest, &fetcher, &opts).unwrap_err();
    assert!(err.to_string().contains("hash mismatch"));
    assert!(!dest.exists(), "no finalized file on permanent failure");
    assert!(
        !dest.with_extension("bin.part").exists(),
        "no .part file left"
    );
}

#[test]
fn resume_continues_from_part_file_when_server_supports_206() {
    let body = b"resumable content for partial download test";
    let cfg = Arc::new(ServerConfig {
        body: body.to_vec(),
        fail_first: 0,
        support_range: true,
        wrong_body: false,
    });
    let server = TestServer::start(cfg);
    let tmp = tempfile::tempdir().expect("tmp");
    let dest = tmp.path().join("resume.bin");
    let part = dest.with_extension("bin.part");
    // Pre-write the first 10 bytes to simulate a partial download.
    std::fs::write(&part, &body[..10]).expect("write part");
    let fetcher = HttpFetcher::new(&server.url());
    let opts = DownloadOptions {
        backoff_base_ms: 1,
        ..Default::default()
    };
    download_file(&dest, &fetcher, &opts).expect("download ok with resume");
    let written = std::fs::read(&dest).expect("read dest");
    assert_eq!(written, body);
}

#[test]
fn permanent_failure_always_500_leaves_no_finalized_file() {
    let cfg = Arc::new(ServerConfig {
        body: Vec::new(),
        fail_first: 100,
        support_range: false,
        wrong_body: false,
    });
    let server = TestServer::start(cfg);
    let tmp = tempfile::tempdir().expect("tmp");
    let dest = tmp.path().join("always500.bin");
    let fetcher = HttpFetcher::new(&server.url());
    let opts = DownloadOptions {
        max_attempts: 3,
        backoff_base_ms: 1,
        ..Default::default()
    };
    let err = download_file(&dest, &fetcher, &opts).unwrap_err();
    assert!(err.to_string().contains("download failed"));
    assert!(!dest.exists(), "no finalized file on permanent failure");
}

#[test]
fn fetcher_trait_can_be_implemented_for_in_memory_bytes() {
    struct InMemoryFetcher {
        url: String,
        bytes: Vec<u8>,
    }
    impl Fetcher for InMemoryFetcher {
        fn url(&self) -> &str {
            &self.url
        }
        fn fetch(&self, _range_start: Option<u64>) -> Result<FetchOutcome, FetchError> {
            let len = self.bytes.len() as u64;
            Ok(FetchOutcome {
                bytes: self.bytes.clone(),
                total: Some(len),
                served: RangeServed::Full,
            })
        }
    }
    let bytes = b"in-memory fetcher works";
    let fetcher = InMemoryFetcher {
        url: "memory://test".to_owned(),
        bytes: bytes.to_vec(),
    };
    let tmp = tempfile::tempdir().expect("tmp");
    let dest = tmp.path().join("memory.bin");
    let opts = DownloadOptions {
        expected_sha256: Some(sha256_hex(bytes)),
        ..Default::default()
    };
    download_file(&dest, &fetcher, &opts).expect("download ok");
    let written = std::fs::read(&dest).expect("read dest");
    assert_eq!(written, bytes);
}

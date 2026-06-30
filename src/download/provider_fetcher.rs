//! [`ProviderFetcher`] — wraps a `Provider::download` call as a [`Fetcher`].

use super::{FetchError, FetchOutcome, Fetcher, RangeServed};

use crate::provider::{Artifact, Provider};

/// Wraps a [`Provider`] + [`Artifact`] as a [`Fetcher`]. Used when the
/// provider returns in-memory bytes (mock provider, or real providers whose
/// own HTTP layer handles the fetch). Range/resume is NOT supported — every
/// fetch returns full content. Errors are treated as transient so the engine
/// retries on provider failures.
pub(crate) struct ProviderFetcher<'a> {
    provider: &'a dyn Provider,
    artifact: &'a Artifact,
}

impl<'a> ProviderFetcher<'a> {
    pub(crate) fn new(provider: &'a dyn Provider, artifact: &'a Artifact) -> Self {
        Self { provider, artifact }
    }
}

impl<'a> Fetcher for ProviderFetcher<'a> {
    fn url(&self) -> &str {
        self.artifact.download_url.as_deref().unwrap_or("(no url)")
    }

    fn fetch(&self, _range_start: Option<u64>) -> std::result::Result<FetchOutcome, FetchError> {
        let bytes = self
            .provider
            .download(self.artifact)
            .map_err(|e| FetchError::Transient(e.to_string(), None))?;
        let total = Some(bytes.len() as u64);
        Ok(FetchOutcome {
            bytes,
            total,
            served: RangeServed::Full,
        })
    }
}

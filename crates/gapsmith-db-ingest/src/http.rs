//! Single shared HTTP client with retry, on-disk cache, and a global offline
//! switch. Every external HTTP request in gapsmith-db MUST go through this
//! module (plan.md global constraint).

use std::path::{Path, PathBuf};
use std::time::Duration;

use rand::Rng;
use reqwest::header::{
    ETAG, HeaderMap, HeaderValue, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::{IngestError, Result};

const USER_AGENT: &str = concat!(
    "gapsmith-db/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/bio-ontology-research-group/gapsmith-db)"
);

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_ATTEMPTS: u32 = 5;
const BASE_BACKOFF_MS: u64 = 500;

/// Build options for [`HttpClient`].
#[derive(Debug, Clone)]
pub struct HttpOptions {
    pub cache_root: PathBuf,
    pub offline: bool,
    pub max_attempts: u32,
}

impl HttpOptions {
    #[must_use]
    pub fn new(cache_root: impl Into<PathBuf>) -> Self {
        Self {
            cache_root: cache_root.into(),
            offline: offline_from_env(),
            max_attempts: MAX_ATTEMPTS,
        }
    }

    #[must_use]
    pub fn offline(mut self, offline: bool) -> Self {
        self.offline = offline;
        self
    }
}

#[must_use]
pub fn offline_from_env() -> bool {
    is_truthy(std::env::var("GAPSMITH_OFFLINE").ok().as_deref())
}

fn is_truthy(v: Option<&str>) -> bool {
    matches!(v, Some("1" | "true" | "TRUE" | "yes"))
}

/// Retry + caching + offline-aware HTTP client.
#[derive(Debug, Clone)]
pub struct HttpClient {
    inner: reqwest::Client,
    opts: HttpOptions,
}

impl HttpClient {
    pub fn new(opts: HttpOptions) -> Result<Self> {
        let inner = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .timeout(DEFAULT_REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::limited(10))
            .gzip(true)
            .build()?;
        std::fs::create_dir_all(&opts.cache_root)?;
        Ok(Self { inner, opts })
    }

    #[must_use]
    pub fn is_offline(&self) -> bool {
        self.opts.offline
    }

    /// Fetch `url` to `dest`. Returns true if bytes were written; false if
    /// the upstream returned 304 and we already had a cached copy at `dest`.
    pub async fn download(&self, url: &str, dest: &Path) -> Result<bool> {
        let cache_meta_path = self.cache_meta_path(url);
        let prior: Option<CachedHeaders> = read_meta(&cache_meta_path);

        if self.opts.offline {
            // Offline → only succeed if the target already exists on disk.
            if dest.exists() {
                info!(%url, "offline: using existing local copy");
                return Ok(false);
            }
            return Err(IngestError::OfflineMiss {
                url: url.to_string(),
            });
        }

        let mut attempt = 0_u32;
        loop {
            attempt += 1;
            let mut req = self.inner.get(url);
            if let Some(ref p) = prior {
                if let Some(etag) = p.etag.as_deref() {
                    req = req.header(IF_NONE_MATCH, etag);
                }
                if let Some(lm) = p.last_modified.as_deref() {
                    req = req.header(IF_MODIFIED_SINCE, lm);
                }
            }

            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) if attempt < self.opts.max_attempts && is_retryable(&e) => {
                    warn!(%url, attempt, error=%e, "transport error, retrying");
                    tokio::time::sleep(backoff(attempt)).await;
                    continue;
                }
                Err(e) => return Err(e.into()),
            };

            let status = resp.status();

            if status.as_u16() == 304 {
                if dest.exists() {
                    info!(%url, "304 not modified, keeping cached copy");
                    return Ok(false);
                }
                // Server says not-modified but we don't actually have the
                // artefact on disk — re-request without conditional headers.
                warn!(%url, "304 but no local copy — retrying without cache headers");
                let meta = CachedHeaders::default();
                write_meta(&cache_meta_path, &meta)?;
                continue;
            }

            if status.is_server_error() && attempt < self.opts.max_attempts {
                warn!(%url, %status, attempt, "server error, retrying");
                tokio::time::sleep(backoff(attempt)).await;
                continue;
            }

            if !status.is_success() {
                return Err(IngestError::Other(format!("{url}: HTTP {status}")));
            }

            // Capture cache headers BEFORE we consume the response body.
            let new_meta = CachedHeaders::from_headers(resp.headers());

            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let tmp = dest.with_extension("partial");
            let bytes = resp.bytes().await?;
            tokio::fs::write(&tmp, &bytes).await?;
            atomic_rename(&tmp, dest)?;
            write_meta(&cache_meta_path, &new_meta)?;
            debug!(%url, bytes = bytes.len(), "downloaded");
            return Ok(true);
        }
    }

    fn cache_meta_path(&self, url: &str) -> PathBuf {
        let digest = short_digest(url);
        self.opts.cache_root.join(format!("{digest}.meta.json"))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CachedHeaders {
    etag: Option<String>,
    last_modified: Option<String>,
}

impl CachedHeaders {
    fn from_headers(h: &HeaderMap) -> Self {
        Self {
            etag: h.get(ETAG).and_then(header_string),
            last_modified: h.get(LAST_MODIFIED).and_then(header_string),
        }
    }
}

fn header_string(v: &HeaderValue) -> Option<String> {
    v.to_str().ok().map(std::string::ToString::to_string)
}

fn read_meta(path: &Path) -> Option<CachedHeaders> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_meta(path: &Path, meta: &CachedHeaders) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string(meta)
        .map_err(|e| IngestError::Other(format!("cache meta serialise: {e}")))?;
    std::fs::write(path, text)?;
    Ok(())
}

fn is_retryable(e: &reqwest::Error) -> bool {
    e.is_timeout() || e.is_connect() || e.is_request()
}

fn backoff(attempt: u32) -> Duration {
    let pow = u32::min(attempt, 10);
    let base = BASE_BACKOFF_MS.saturating_mul(1_u64 << pow);
    let jitter = rand::rng().random_range(0..=base / 2);
    Duration::from_millis(base + jitter)
}

fn short_digest(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(&h.finalize()[..8])
}

fn atomic_rename(from: &Path, to: &Path) -> std::io::Result<()> {
    // std::fs::rename is atomic on Unix when both paths are on the same fs.
    std::fs::rename(from, to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthy_table() {
        for (v, want) in [
            (Some("1"), true),
            (Some("true"), true),
            (Some("yes"), true),
            (Some("0"), false),
            (Some("no"), false),
            (Some(""), false),
            (None, false),
        ] {
            assert_eq!(is_truthy(v), want, "value {v:?}");
        }
    }

    #[test]
    fn backoff_grows_monotonically_on_average() {
        let a = backoff(1).as_millis();
        let b = backoff(4).as_millis();
        assert!(b >= a, "backoff(4)={b} should be >= backoff(1)={a}");
    }
}

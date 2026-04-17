//! Source-specific fetchers and loaders for gapsmith-db.
//!
//! Every fetcher produces a [`FetchPlan`] (list of [`FetchStep`]s). The
//! fetch engine downloads each step to a temp path, verifies SHA256 against
//! the pin in `SOURCE.toml`, atomically renames into place, and writes a
//! `MANIFEST.json` with provenance. A global offline switch turns network
//! access off; dry-run prints the plan without touching the network.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod fetch;
pub mod hash;
pub mod http;
pub mod manifest;
pub mod merge;
pub mod parse;
pub mod source;
pub mod sources;

pub use fetch::{ExtractMode, FetchContext, FetchOutcome, FetchPlan, FetchStep, PinStatus};
pub use source::{SourceId, SourceSpec};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IngestError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("toml parse error at {path}: {source}")]
    Toml {
        path: std::path::PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("hash mismatch for {url}: expected {expected}, got {actual}")]
    HashMismatch {
        url: String,
        expected: String,
        actual: String,
    },
    #[error("source {0} is not pinned; re-run with --accept-first-run to record hashes")]
    UnpinnedSource(String),
    #[error("offline mode and no cache entry for {url}")]
    OfflineMiss { url: String },
    #[error("KEGG fetcher requires --i-have-a-kegg-licence")]
    KeggGated,
    #[error("unknown source: {0}")]
    UnknownSource(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, IngestError>;

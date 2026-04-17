//! Source-specific fetchers and loaders for gapsmith-db.
//!
//! Fetchers are implemented in Phase 1 per plan.md. Every fetcher must:
//! download to a temp path, verify SHA256 against a pinned value, atomically
//! move into place, and write a `MANIFEST.json` with provenance.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IngestError {
    #[error("not yet implemented: {0}")]
    Unimplemented(&'static str),
}

pub type Result<T> = std::result::Result<T, IngestError>;

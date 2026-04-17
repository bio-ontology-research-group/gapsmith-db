//! Qdrant HTTP retrieval backend.
//!
//! Phase-4 scope: wire-protocol stub. Real search needs an embedder that
//! turns the query text into a vector; that embedder is configured at
//! corpus-ingest time and is out of scope here. Until that's wired,
//! [`QdrantBackend::search`] returns an error explaining what's missing.

use serde::{Deserialize, Serialize};

use super::{Passage, RetrievalBackend, SearchQuery, filter_passages};
use crate::{DomainFilter, ProposeError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantConfig {
    pub url: String,
    pub collection: String,
    /// Optional API key. Read from env var by convention (e.g. QDRANT_API_KEY).
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct QdrantBackend {
    pub config: QdrantConfig,
    pub filter: DomainFilter,
}

impl QdrantBackend {
    #[must_use]
    pub fn new(config: QdrantConfig) -> Self {
        Self {
            config,
            filter: DomainFilter::default(),
        }
    }

    #[must_use]
    pub fn with_filter(mut self, f: DomainFilter) -> Self {
        self.filter = f;
        self
    }
}

impl RetrievalBackend for QdrantBackend {
    fn search(&self, _q: &SearchQuery) -> crate::Result<Vec<Passage>> {
        // Real implementation: embed q.text -> POST to /collections/<name>/
        // points/search -> map payload -> filter_passages as final guard.
        // Phase-4 stops here; the embedder is not configured.
        let _ = (&self.filter, filter_passages);
        Err(ProposeError::Retrieval(format!(
            "QdrantBackend search unimplemented (collection={}, url={}); \
             wire via the corpus-ingest embedder and re-enable.",
            self.config.collection, self.config.url,
        )))
    }
}

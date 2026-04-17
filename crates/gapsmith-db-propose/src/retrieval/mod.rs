//! Retrieval backend — returns literature passages for prompt construction.
//!
//! The production backend is Qdrant over an embedded Europe PMC OA +
//! bioRxiv corpus. For Phase 4 we ship:
//!
//! - [`QdrantBackend`] — HTTP client stub (wire-protocol shape in place,
//!   integration deferred; the `search` method returns an `Err` when no
//!   base URL is configured).
//! - [`InMemoryBackend`] — holds `Passage`s in memory; used by tests and
//!   the mock proposer.
//!
//! Every passage returned by any backend is filtered through
//! [`crate::DomainFilter`] as a final guard, regardless of whether the
//! corpus-ingest script already applied the same filter.

pub mod in_memory;
pub mod qdrant;

pub use in_memory::InMemoryBackend;
pub use qdrant::QdrantBackend;

use serde::{Deserialize, Serialize};

use crate::DomainFilter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Passage {
    /// Stable identifier (e.g. "europepmc:PMC1234567#p12").
    pub id: String,
    pub text: String,
    /// Source URL the passage was extracted from.
    pub source_url: String,
    /// Optional PMID; populated when the source has one.
    #[serde(default)]
    pub pmid: Option<String>,
    /// Title of the enclosing paper, if known.
    #[serde(default)]
    pub title: Option<String>,
    /// Relevance score returned by the backend (higher is better).
    #[serde(default)]
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub text: String,
    pub top_k: usize,
}

pub trait RetrievalBackend {
    fn search(&self, q: &SearchQuery) -> crate::Result<Vec<Passage>>;
}

/// Strip any passage whose source URL resolves to a denylisted domain.
/// This runs after the backend returns passages; it's the second belt.
#[must_use]
pub fn filter_passages(filter: &DomainFilter, passages: Vec<Passage>) -> Vec<Passage> {
    passages
        .into_iter()
        .filter(|p| filter.allows_url(&p.source_url))
        .collect()
}

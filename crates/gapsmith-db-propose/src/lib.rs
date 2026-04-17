//! LLM proposer scaffold for gapsmith-db.
//!
//! Phase-4 scope per plan.md: plumbing only, with a mock proposer emitting
//! hand-written fixture proposals. The flow is:
//!
//! 1. Retrieval backend fetches passages (domain-filtered at both ingest and
//!    query time — see LICENSING.md for the banned source list).
//! 2. The prompt template is rendered with target, passages, and versioned
//!    instructions.
//! 3. The LLM backend (OpenRouter by default) returns a JSON proposal.
//! 4. The proposal is strictly validated against the schema, content-hashed,
//!    and written to `proposals/pending/<hash>.json`.
//! 5. The verifier layer runs over a DB clone that incorporates the
//!    proposal. Passes → `for_curation/`. Failures → `rejected/` with the
//!    verifier report inlined.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod domain_filter;
pub mod llm;
pub mod prompt;
pub mod proposer;
pub mod retrieval;
pub mod router;
pub mod schema;

pub use domain_filter::DomainFilter;
pub use llm::{FixtureBackend, LlmBackend, OpenRouterBackend};
pub use prompt::{PROMPT_VERSION, PromptContext, PromptTemplate};
pub use proposer::{Proposer, ProposerOptions};
pub use retrieval::{InMemoryBackend, Passage, QdrantBackend, RetrievalBackend, SearchQuery};
pub use router::{ProposalDisposition, route_proposal};
pub use schema::{
    EnzymeRef, Proposal, ProposalCitation, ProposalEdge, ProposalReaction, ProposalTarget,
    ReactionRef, SCHEMA_VERSION,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProposeError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("schema violation: {0}")]
    Schema(String),
    #[error("domain {0} is on the forbidden list")]
    ForbiddenDomain(String),
    #[error("LLM backend: {0}")]
    Llm(String),
    #[error("retrieval backend: {0}")]
    Retrieval(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ProposeError>;

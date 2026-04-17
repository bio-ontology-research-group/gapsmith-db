//! LLM backend abstraction.
//!
//! The proposer does not know or care which model answers the prompt. All
//! it wants is a JSON blob that parses into a [`Proposal`]. That makes
//! swapping providers and backstopping with a mock trivial.

pub mod fixture;
pub mod openrouter;

pub use fixture::FixtureBackend;
pub use openrouter::{OpenRouterBackend, OpenRouterConfig};

use crate::schema::Proposal;

pub trait LlmBackend {
    /// Human-readable name (appears in `Proposal::model`).
    fn name(&self) -> &str;

    /// Produce a proposal given a rendered prompt. Backends are
    /// responsible for JSON-mode enforcement and retry-on-invalid-JSON;
    /// proposer-level retries sit above this trait.
    fn complete(&self, prompt: &str) -> crate::Result<Proposal>;
}

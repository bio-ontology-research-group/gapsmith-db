//! LLM proposer scaffold for gapsmith-db.
//!
//! Implemented in Phase 4 per plan.md. Model-agnostic interface with an
//! OpenRouter default backend. LLM outputs are proposals, not decisions.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProposeError {
    #[error("not yet implemented: {0}")]
    Unimplemented(&'static str),
}

pub type Result<T> = std::result::Result<T, ProposeError>;

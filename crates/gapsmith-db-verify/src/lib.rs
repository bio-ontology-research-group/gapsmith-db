//! Verifier layer: symbolic, constraint-based, thermodynamic, DL-consistency.
//!
//! Implemented in Phase 3 per plan.md. Verifiers are the judges; LLM outputs
//! are untrusted proposals. Fail-closed semantics.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("not yet implemented: {0}")]
    Unimplemented(&'static str),
}

pub type Result<T> = std::result::Result<T, VerifyError>;

//! Canonical schema and types for gapsmith-db.
//!
//! Populated in Phase 2 per plan.md. This Phase-0 stub declares the crate
//! and its error type so dependent crates compile.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("not yet implemented: {0}")]
    Unimplemented(&'static str),
}

pub type Result<T> = std::result::Result<T, CoreError>;

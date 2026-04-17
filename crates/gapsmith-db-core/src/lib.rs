//! Canonical schema and types for gapsmith-db.
//!
//! This crate defines the public contract consumed by the gapsmith Rust
//! gapseq port. See ADR 0001 for the split between this crate and the
//! Python verifier bridge. Changes here ripple into downstream consumers
//! — prefer extending to breaking.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod compartment;
pub mod compound;
pub mod database;
pub mod ec;
pub mod evidence;
pub mod ids;
pub mod pathway;
pub mod pmid;
pub mod reaction;
pub mod reversibility;
pub mod serde_io;
pub mod source;

pub use compartment::Compartment;
pub use compound::Compound;
pub use database::{Database, DatabaseError, DatabaseStats};
pub use ec::{EcNumber, EcParseError, EcWildcard};
pub use evidence::{Confidence, Evidence, MergeFlag};
pub use ids::{CompoundId, PathwayId, ReactionId};
pub use pathway::{OrganismScope, Pathway};
pub use pmid::Pmid;
pub use reaction::{MassBalanceStatus, Reaction, StoichiometryEntry};
pub use reversibility::Reversibility;
pub use source::Source;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),
    #[error("bincode error: {0}")]
    Bincode(#[from] Box<bincode::ErrorKind>),
    #[error("ec parse error: {0}")]
    EcParse(#[from] EcParseError),
    #[error("database invariant violated: {0}")]
    Invariant(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;

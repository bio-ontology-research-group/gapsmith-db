//! Per-source parsers producing shared IR.
//!
//! Each parser reads the artefacts landed by the Phase-1 fetch step and
//! emits a typed intermediate representation that the merge pass can
//! combine. Parsers are deliberately defensive: they skip rows they
//! cannot understand and log at WARN rather than abort, so an upstream
//! schema drift on one table does not halt ingestion of the rest.

pub mod chebi;
pub mod gapseq;
pub mod ir;
pub mod mnxref;
pub mod modelseed;
pub mod rhea;

pub use ir::{IngestBundle, ParsedCompound, ParsedReaction, ParsedReactionEntry};

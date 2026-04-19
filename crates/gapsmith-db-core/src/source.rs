//! Upstream source enum used as the key in `xrefs: BTreeMap<Source, Vec<String>>`.
//!
//! This is separate from `gapsmith_db_ingest::source::SourceId` because the
//! ingest-side enum is about "where did the file come from" (including the
//! KEGG gate and the data/<dir> name) while this enum is about "what kind
//! of external identifier is this xref". In practice they overlap but the
//! two concerns are decoupled so ingest-only sources can be added without
//! polluting the core schema.

use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Chebi,
    Modelseed,
    Mnxref,
    Rhea,
    Intenz,
    Uniprot,
    Reactome,
    Gapseq,
    Kegg,
    /// InChIKey-based cross-reference (canonical, not source-attributed).
    Inchikey,
    /// Pubchem CID / SID.
    Pubchem,
    /// Claim originated from an LLM-generated [`Proposal`](gapsmith_db_propose::Proposal)
    /// that has been accepted into the canonical DB. The supporting
    /// `Evidence` entry carries the proposal's content hash so the
    /// decision log entry can be cross-referenced.
    LlmProposal,
    /// Generic fallback; the identifier string carries its own namespace.
    #[default]
    Other,
}

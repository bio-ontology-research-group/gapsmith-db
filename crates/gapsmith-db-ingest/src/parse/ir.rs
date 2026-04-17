//! Intermediate representation shared by every source parser.
//!
//! The IR is a loose superset of the canonical schema: everything that
//! might be useful for the merge pass, in a form that is still easy to
//! produce from a TSV/RDF/JSON source. The merger in `merge.rs` collapses
//! this into the canonical [`gapsmith_db_core::Database`].

use std::collections::BTreeMap;

use gapsmith_db_core::source::Source;

/// A compound as asserted by ONE source. Multiple `ParsedCompound`s may
/// refer to the same canonical compound; the merge pass decides.
#[derive(Debug, Clone, Default)]
pub struct ParsedCompound {
    pub source: Source,
    /// Native identifier from this source (e.g. "CHEBI:15377", "cpd00001").
    pub native_id: String,
    pub formula: Option<String>,
    pub charge: Option<i16>,
    pub inchi: Option<String>,
    pub inchikey: Option<String>,
    pub smiles: Option<String>,
    pub mass: Option<f64>,
    pub names: Vec<String>,
    pub chebi_roles: Vec<String>,
    /// Additional xrefs discovered inline (e.g. MNXref's mapping rows).
    pub extra_xrefs: BTreeMap<Source, Vec<String>>,
}

impl ParsedCompound {
    #[must_use]
    pub fn new(source: Source, native_id: impl Into<String>) -> Self {
        Self {
            source,
            native_id: native_id.into(),
            ..Self::default()
        }
    }
}

/// A stoichiometric entry expressed in upstream terms (native compound
/// IDs). The merge pass resolves these to canonical [`CompoundId`]s.
#[derive(Debug, Clone)]
pub struct ParsedReactionEntry {
    pub native_compound: String,
    pub compound_source: Source,
    pub coefficient: f64,
    /// Compartment short code as the source spelled it. Resolved by the
    /// merge pass to [`Compartment`].
    pub compartment_code: String,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedReaction {
    pub source: Source,
    pub native_id: String,
    pub names: Vec<String>,
    pub stoichiometry: Vec<ParsedReactionEntry>,
    pub reversibility: Option<gapsmith_db_core::Reversibility>,
    pub ec_numbers: Vec<String>,
    pub rhea_id: Option<String>,
    pub seed_id: Option<String>,
    pub is_transport: bool,
    pub extra_xrefs: BTreeMap<Source, Vec<String>>,
}

impl ParsedReaction {
    #[must_use]
    pub fn new(source: Source, native_id: impl Into<String>) -> Self {
        Self {
            source,
            native_id: native_id.into(),
            ..Self::default()
        }
    }
}

/// Bundle of everything one source produces. A run of ingest collects
/// one `IngestBundle` per source, then the merge pass consumes them.
#[derive(Debug, Clone, Default)]
pub struct IngestBundle {
    pub source: Option<Source>,
    pub compounds: Vec<ParsedCompound>,
    pub reactions: Vec<ParsedReaction>,
    /// Cross-source compound xrefs (e.g. MNXref's "CHEBI:X = MNX:Y" rows).
    /// Key: canonical-ish native ID pair; value: xrefs to attach.
    pub compound_xrefs: Vec<CompoundXrefRow>,
    /// Cross-source reaction xrefs (Rhea ↔ SEED, MNXref links).
    pub reaction_xrefs: Vec<ReactionXrefRow>,
}

#[derive(Debug, Clone)]
pub struct CompoundXrefRow {
    pub from_source: Source,
    pub from_id: String,
    pub to_source: Source,
    pub to_id: String,
}

#[derive(Debug, Clone)]
pub struct ReactionXrefRow {
    pub from_source: Source,
    pub from_id: String,
    pub to_source: Source,
    pub to_id: String,
}

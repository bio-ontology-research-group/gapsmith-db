//! Compound — the "metabolite" entity.
//!
//! A compound carries chemistry (formula/charge/InChI/SMILES/mass), names,
//! ChEBI role annotations (for the DL consistency check in Phase 3), and a
//! cross-reference bag keyed by [`Source`]. Every compound has at least
//! one identifier — the internal [`CompoundId`] — plus typically several
//! upstream xrefs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::evidence::Evidence;
use crate::ids::CompoundId;
use crate::source::Source;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compound {
    pub id: CompoundId,

    #[serde(default)]
    pub formula: Option<String>,

    /// Integer charge. `None` when upstream declined to commit.
    #[serde(default)]
    pub charge: Option<i16>,

    /// Canonical InChI (full string, not hash).
    #[serde(default)]
    pub inchi: Option<String>,

    /// InChIKey. Used as the first dedup tier per plan.md.
    #[serde(default)]
    pub inchikey: Option<String>,

    #[serde(default)]
    pub smiles: Option<String>,

    /// Monoisotopic mass, Da.
    #[serde(default)]
    pub mass: Option<f64>,

    /// Cross-references to upstream identifiers. A compound may have
    /// multiple IDs from the same source (e.g. alternate ChEBI accessions).
    #[serde(default)]
    pub xrefs: BTreeMap<Source, Vec<String>>,

    /// Human-readable names; first is preferred display name.
    #[serde(default)]
    pub names: Vec<String>,

    /// ChEBI role annotations (e.g. `CHEBI:25212` = "metabolite"). Feeds the
    /// Phase-3 DL consistency checker.
    #[serde(default)]
    pub chebi_roles: Vec<String>,

    /// Provenance for every claim in this compound record.
    #[serde(default)]
    pub evidence: Vec<Evidence>,
}

impl Compound {
    #[must_use]
    pub fn new(id: CompoundId) -> Self {
        Self {
            id,
            formula: None,
            charge: None,
            inchi: None,
            inchikey: None,
            smiles: None,
            mass: None,
            xrefs: BTreeMap::new(),
            names: Vec::new(),
            chebi_roles: Vec::new(),
            evidence: Vec::new(),
        }
    }

    /// Add an xref. Silently dedups within a source's list.
    pub fn add_xref(&mut self, source: Source, id: impl Into<String>) {
        let id = id.into();
        let bucket = self.xrefs.entry(source).or_default();
        if !bucket.iter().any(|x| x == &id) {
            bucket.push(id);
        }
    }

    /// Count of distinct identifiers (internal + all xrefs). Property tests
    /// assert this is ≥ 1.
    #[must_use]
    pub fn identifier_count(&self) -> usize {
        1 + self.xrefs.values().map(Vec::len).sum::<usize>()
    }

    /// Does the compound have any cross-reference for the given source?
    #[must_use]
    pub fn has_source(&self, source: Source) -> bool {
        self.xrefs.get(&source).is_some_and(|v| !v.is_empty())
    }

    #[must_use]
    pub fn preferred_name(&self) -> Option<&str> {
        self.names.first().map(String::as_str)
    }
}

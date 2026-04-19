//! Pathway — a DAG of reactions.
//!
//! Structure and variants are carried in `reactions` (list) and
//! `variant_of` (parent pathway for alternative routes).

use serde::{Deserialize, Serialize};

use crate::evidence::Evidence;
use crate::ids::{PathwayId, ReactionId};

/// Organism scope for a pathway. Kept simple: a short list of NCBI taxon IDs
/// or the universal marker. Open to extension.
///
/// Externally-tagged for bincode compatibility (bincode does not support
/// internally-tagged enums).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganismScope {
    Universal,
    /// Applies to organisms covered by the given NCBI taxon IDs (including descendants).
    TaxonIds(Vec<u32>),
    /// Free-form scope (e.g. "chemolithoautotrophs"). Kept as an escape hatch;
    /// curators are encouraged to map to taxa when possible.
    Description(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pathway {
    pub id: PathwayId,
    pub name: String,
    /// Ordered list; order is the preferred canonical presentation.
    /// For upstream-ingested pathways the DAG is implicit in the
    /// reactions' stoichiometry; for LLM-proposed pathways it is made
    /// explicit via [`Pathway::dag`].
    #[serde(default)]
    pub reactions: Vec<ReactionId>,
    /// Directed edges between reactions in [`Pathway::reactions`].
    /// Populated when a proposal carries DAG structure that is not
    /// recoverable from stoichiometry alone (common for LLM proposals
    /// where the model asserts reaction ordering separately from the
    /// reaction definitions).
    #[serde(default)]
    pub dag: Vec<(ReactionId, ReactionId)>,
    /// Parent pathway if this is a variant / alternative route.
    #[serde(default)]
    pub variant_of: Option<PathwayId>,
    #[serde(default = "default_organism_scope")]
    pub organism_scope: OrganismScope,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
}

fn default_organism_scope() -> OrganismScope {
    OrganismScope::Universal
}

impl Pathway {
    #[must_use]
    pub fn new(id: PathwayId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            reactions: Vec::new(),
            dag: Vec::new(),
            variant_of: None,
            organism_scope: OrganismScope::Universal,
            evidence: Vec::new(),
        }
    }
}

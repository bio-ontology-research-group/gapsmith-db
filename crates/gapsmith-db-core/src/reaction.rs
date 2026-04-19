//! Reaction — stoichiometry, thermodynamics, provenance.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::compartment::Compartment;
use crate::ec::EcNumber;
use crate::evidence::Evidence;
use crate::ids::{CompoundId, ReactionId};
use crate::reversibility::Reversibility;
use crate::source::Source;

/// A single entry in a reaction's stoichiometry.
///
/// `coefficient` is positive for products, negative for substrates. This is
/// the convention COBRA and equilibrator use; staying consistent avoids a
/// class of sign-flip bugs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoichiometryEntry {
    pub compound: CompoundId,
    pub coefficient: f64,
    pub compartment: Compartment,
}

impl StoichiometryEntry {
    #[must_use]
    pub fn substrate(compound: CompoundId, amount: f64, compartment: Compartment) -> Self {
        debug_assert!(
            amount > 0.0,
            "substrate amount must be positive before sign flip"
        );
        Self {
            compound,
            coefficient: -amount,
            compartment,
        }
    }

    #[must_use]
    pub fn product(compound: CompoundId, amount: f64, compartment: Compartment) -> Self {
        debug_assert!(amount > 0.0, "product amount must be positive");
        Self {
            compound,
            coefficient: amount,
            compartment,
        }
    }

    #[must_use]
    pub fn is_substrate(&self) -> bool {
        self.coefficient < 0.0
    }

    #[must_use]
    pub fn is_product(&self) -> bool {
        self.coefficient > 0.0
    }
}

/// Mass-balance verdict. The `HydrogenOnly` variant matters because plan.md
/// says "tolerate explicit-proton ambiguity; flag (don't reject) hydrogen-only
/// imbalances".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MassBalanceStatus {
    Balanced,
    HydrogenOnly,
    Unbalanced,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reaction {
    pub id: ReactionId,

    #[serde(default)]
    pub stoichiometry: Vec<StoichiometryEntry>,

    pub reversibility: Reversibility,

    #[serde(default)]
    pub ec_numbers: Vec<EcNumber>,

    /// Rhea reaction ID if known.
    #[serde(default)]
    pub rhea_id: Option<String>,

    /// ModelSEED reaction ID if known.
    #[serde(default)]
    pub seed_id: Option<String>,

    /// Standard Gibbs free energy change: (value, uncertainty) in kJ/mol.
    /// Populated by the `ThermodynamicFeasibility` verifier (Phase 3).
    #[serde(default)]
    pub delta_g: Option<(f64, f64)>,

    #[serde(default)]
    pub is_transport: bool,

    #[serde(default)]
    pub status: MassBalanceStatus,

    #[serde(default)]
    pub xrefs: BTreeMap<Source, Vec<String>>,

    #[serde(default)]
    pub names: Vec<String>,

    /// Swiss-Prot accessions that catalyse this reaction. Populated when
    /// the reaction is merged in from an accepted LLM proposal, or from
    /// upstream annotation sources. An accession appearing here is a
    /// *claim* about catalysis; the verifier layer (not this field)
    /// decides whether the accession exists.
    #[serde(default)]
    pub enzymes: Vec<String>,

    #[serde(default)]
    pub evidence: Vec<Evidence>,
}

impl Reaction {
    #[must_use]
    pub fn new(id: ReactionId, reversibility: Reversibility) -> Self {
        Self {
            id,
            stoichiometry: Vec::new(),
            reversibility,
            ec_numbers: Vec::new(),
            rhea_id: None,
            seed_id: None,
            delta_g: None,
            is_transport: false,
            status: MassBalanceStatus::Unknown,
            xrefs: BTreeMap::new(),
            names: Vec::new(),
            enzymes: Vec::new(),
            evidence: Vec::new(),
        }
    }

    pub fn add_xref(&mut self, source: Source, id: impl Into<String>) {
        let id = id.into();
        let bucket = self.xrefs.entry(source).or_default();
        if !bucket.iter().any(|x| x == &id) {
            bucket.push(id);
        }
    }

    pub fn substrates(&self) -> impl Iterator<Item = &StoichiometryEntry> {
        self.stoichiometry.iter().filter(|s| s.is_substrate())
    }

    pub fn products(&self) -> impl Iterator<Item = &StoichiometryEntry> {
        self.stoichiometry.iter().filter(|s| s.is_product())
    }

    /// Compound IDs referenced anywhere in the stoichiometry.
    pub fn referenced_compounds(&self) -> impl Iterator<Item = &CompoundId> {
        self.stoichiometry.iter().map(|s| &s.compound)
    }
}

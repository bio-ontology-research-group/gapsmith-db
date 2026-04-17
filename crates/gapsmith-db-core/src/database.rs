//! Top-level `Database` container: compounds, reactions, pathways, plus
//! index maps for ID lookup.
//!
//! Invariant checks (`validate_*`) enforce the properties plan.md lists as
//! property tests. They're cheap enough to run as part of serialisation.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::compound::Compound;
use crate::ids::{CompoundId, PathwayId, ReactionId};
use crate::pathway::Pathway;
use crate::reaction::Reaction;

/// In-memory database. `IndexMap` preserves insertion order for
/// deterministic serialisation and stable human-diffable TSV output.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Database {
    pub compounds: IndexMap<CompoundId, Compound>,
    pub reactions: IndexMap<ReactionId, Reaction>,
    pub pathways: IndexMap<PathwayId, Pathway>,
}

#[derive(Debug, Clone, Error)]
pub enum DatabaseError {
    #[error("compound {0} has no identifiers (internal ID must be non-empty)")]
    CompoundHasNoIdentifiers(CompoundId),
    #[error("reaction {reaction} references unknown compound {compound}")]
    UnknownCompoundRef {
        reaction: ReactionId,
        compound: CompoundId,
    },
    #[error("pathway {pathway} references unknown reaction {reaction}")]
    UnknownReactionRef {
        pathway: PathwayId,
        reaction: ReactionId,
    },
    #[error("pathway {0} declares itself as its own variant parent")]
    PathwaySelfVariant(PathwayId),
    #[error("pathway {pathway} variant_of references unknown pathway {parent}")]
    UnknownPathwayParent {
        pathway: PathwayId,
        parent: PathwayId,
    },
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    pub compounds: usize,
    pub reactions: usize,
    pub pathways: usize,
    pub transport_reactions: usize,
    pub compounds_with_inchikey: usize,
    pub compounds_with_chebi: usize,
    pub reactions_with_ec: usize,
    pub reactions_with_rhea: usize,
}

impl Database {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_compound(&mut self, c: Compound) {
        self.compounds.insert(c.id.clone(), c);
    }

    pub fn insert_reaction(&mut self, r: Reaction) {
        self.reactions.insert(r.id.clone(), r);
    }

    pub fn insert_pathway(&mut self, p: Pathway) {
        self.pathways.insert(p.id.clone(), p);
    }

    #[must_use]
    pub fn stats(&self) -> DatabaseStats {
        use crate::source::Source;
        let mut s = DatabaseStats {
            compounds: self.compounds.len(),
            reactions: self.reactions.len(),
            pathways: self.pathways.len(),
            ..DatabaseStats::default()
        };
        for c in self.compounds.values() {
            if c.inchikey.is_some() {
                s.compounds_with_inchikey += 1;
            }
            if c.has_source(Source::Chebi) {
                s.compounds_with_chebi += 1;
            }
        }
        for r in self.reactions.values() {
            if r.is_transport {
                s.transport_reactions += 1;
            }
            if !r.ec_numbers.is_empty() {
                s.reactions_with_ec += 1;
            }
            if r.rhea_id.is_some() {
                s.reactions_with_rhea += 1;
            }
        }
        s
    }

    /// Check every invariant from plan.md. Called during serialisation and
    /// once at the end of ingestion.
    pub fn validate(&self) -> Result<(), DatabaseError> {
        self.validate_compounds_have_identifiers()?;
        self.validate_reaction_stoichiometry_references()?;
        self.validate_pathway_references()?;
        Ok(())
    }

    fn validate_compounds_have_identifiers(&self) -> Result<(), DatabaseError> {
        for c in self.compounds.values() {
            if c.id.as_str().is_empty() {
                return Err(DatabaseError::CompoundHasNoIdentifiers(c.id.clone()));
            }
            // identifier_count() is always ≥ 1 because of the internal ID,
            // but assert for clarity.
            debug_assert!(c.identifier_count() >= 1);
        }
        Ok(())
    }

    fn validate_reaction_stoichiometry_references(&self) -> Result<(), DatabaseError> {
        for r in self.reactions.values() {
            for s in &r.stoichiometry {
                if !self.compounds.contains_key(&s.compound) {
                    return Err(DatabaseError::UnknownCompoundRef {
                        reaction: r.id.clone(),
                        compound: s.compound.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_pathway_references(&self) -> Result<(), DatabaseError> {
        for p in self.pathways.values() {
            if let Some(parent) = &p.variant_of {
                if parent == &p.id {
                    return Err(DatabaseError::PathwaySelfVariant(p.id.clone()));
                }
                if !self.pathways.contains_key(parent) {
                    return Err(DatabaseError::UnknownPathwayParent {
                        pathway: p.id.clone(),
                        parent: parent.clone(),
                    });
                }
            }
            for rxn in &p.reactions {
                if !self.reactions.contains_key(rxn) {
                    return Err(DatabaseError::UnknownReactionRef {
                        pathway: p.id.clone(),
                        reaction: rxn.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compartment::Compartment;
    use crate::reaction::StoichiometryEntry;
    use crate::reversibility::Reversibility;

    fn simple_db() -> Database {
        let mut db = Database::new();
        let a = Compound::new(CompoundId::new("C1"));
        let b = Compound::new(CompoundId::new("C2"));
        db.insert_compound(a);
        db.insert_compound(b);

        let mut r = Reaction::new(ReactionId::new("R1"), Reversibility::Forward);
        r.stoichiometry.push(StoichiometryEntry::substrate(
            CompoundId::new("C1"),
            1.0,
            Compartment::Cytosol,
        ));
        r.stoichiometry.push(StoichiometryEntry::product(
            CompoundId::new("C2"),
            1.0,
            Compartment::Cytosol,
        ));
        db.insert_reaction(r);

        let mut p = Pathway::new(PathwayId::new("P1"), "test pathway");
        p.reactions.push(ReactionId::new("R1"));
        db.insert_pathway(p);
        db
    }

    #[test]
    fn valid_db_passes_all_checks() {
        simple_db().validate().unwrap();
    }

    #[test]
    fn dangling_reaction_compound_is_caught() {
        let mut db = simple_db();
        let r = db.reactions.get_mut(&ReactionId::new("R1")).unwrap();
        r.stoichiometry.push(StoichiometryEntry::product(
            CompoundId::new("C_MISSING"),
            1.0,
            Compartment::Cytosol,
        ));
        assert!(matches!(
            db.validate(),
            Err(DatabaseError::UnknownCompoundRef { .. })
        ));
    }

    #[test]
    fn self_variant_is_caught() {
        let mut db = simple_db();
        db.pathways
            .get_mut(&PathwayId::new("P1"))
            .unwrap()
            .variant_of = Some(PathwayId::new("P1"));
        assert!(matches!(
            db.validate(),
            Err(DatabaseError::PathwaySelfVariant(_))
        ));
    }

    #[test]
    fn stats_counts_correctly() {
        let s = simple_db().stats();
        assert_eq!(s.compounds, 2);
        assert_eq!(s.reactions, 1);
        assert_eq!(s.pathways, 1);
    }
}

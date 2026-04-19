//! Merge an accepted [`Proposal`] into a canonical [`Database`].
//!
//! Runs **after** `curate accept` records its decision-log entry. Every
//! claim carried into the DB receives an [`Evidence`] entry tagged
//! `Source::LlmProposal` with the proposal's content hash, so the
//! origin is traceable from any reaction/pathway back to the decision
//! record and the original JSON file.
//!
//! Resolution rules:
//!
//! - A [`ReactionRef::Rhea`] reference is resolved against existing
//!   reactions by `rhea_id` or `xrefs[Rhea]`. A hit reuses the existing
//!   `ReactionId`; a miss synthesises a stub reaction with the Rhea
//!   xref set but **no stoichiometry** (flagged in the report so the
//!   curator knows the Rhea tables should be re-ingested).
//! - A [`ReactionRef::ChebiEc`] reference always synthesises a new
//!   reaction (there is no canonical lookup key for an EC-plus-
//!   compound tuple). Compounds are resolved by ChEBI xref against the
//!   DB, and newly-minted `Compound`s are created for ChEBI IDs not
//!   found locally.
//! - Enzymes attach as [`Reaction::enzymes`] accessions. The verifier
//!   layer decides whether each accession exists; this function does
//!   not re-validate.
//! - The pathway itself is inserted as a new [`Pathway`] with a
//!   content-derived `PathwayId`, the DAG edges preserved on
//!   [`Pathway::dag`], and citations folded into [`Pathway::evidence`].
//!
//! The function refuses to run if any reaction reference is a
//! [`ReactionRef::ChebiEc`] with an empty substrate OR product list —
//! that is a degenerate proposal and the caller should reject it
//! rather than corrupt the DB.

use std::collections::BTreeMap;

use chrono::Utc;
use gapsmith_db_core::{
    Compartment, Compound, CompoundId, Confidence, Database, Evidence, OrganismScope, Pathway,
    PathwayId, Reaction, ReactionId, Reversibility, Source, StoichiometryEntry,
};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::schema::{EnzymeRef, Proposal, ReactionRef};
use crate::{ProposeError, Result};

/// Summary returned by [`merge_proposal`]. Intended for the CLI to
/// print and for the decision-log entry to reference.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MergeReport {
    pub pathway_id: String,
    pub reactions_linked: Vec<(String, String)>,
    pub reactions_created: Vec<(String, String)>,
    pub compounds_created: Vec<String>,
    pub enzymes_attached: usize,
    pub citations_attached: usize,
    pub warnings: Vec<String>,
}

impl MergeReport {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.warnings.is_empty()
    }
}

/// Merge `p` into `db` in place. See module docs for resolution rules.
///
/// `curator` is recorded on every produced `Evidence` entry.
pub fn merge_proposal(db: &mut Database, p: &Proposal, curator: &str) -> Result<MergeReport> {
    validate_for_merge(p)?;
    let mut report = MergeReport::default();
    let slug = slug(&p.target.pathway_name);
    let hash_tail = proposal_hash_tail(&p.proposal_id);

    let mut local_to_reaction: BTreeMap<String, ReactionId> = BTreeMap::new();
    for r in &p.reactions {
        let (reaction_id, was_new) = resolve_reaction(db, p, r, &slug, curator, &mut report);
        local_to_reaction.insert(r.local_id.clone(), reaction_id.clone());
        let label = format!("{} ({})", r.local_id, short_ref(&r.reference));
        if was_new {
            report
                .reactions_created
                .push((label, reaction_id.to_string()));
        } else {
            report
                .reactions_linked
                .push((label, reaction_id.to_string()));
        }
    }

    for enzyme in &p.enzymes {
        let n = attach_enzyme(db, enzyme, &local_to_reaction, p, curator, &mut report);
        report.enzymes_attached += n;
    }

    let pathway_id = PathwayId::new(format!("llm_{slug}_{hash_tail}"));
    let pathway = build_pathway(&pathway_id, p, &local_to_reaction, curator, &mut report);
    db.insert_pathway(pathway);
    report.pathway_id = pathway_id.to_string();

    Ok(report)
}

fn validate_for_merge(p: &Proposal) -> Result<()> {
    for r in &p.reactions {
        if let ReactionRef::ChebiEc {
            substrates,
            products,
            ..
        } = &r.reference
            && (substrates.is_empty() || products.is_empty())
        {
            return Err(ProposeError::Schema(format!(
                "reaction {} has empty substrates or products; cannot synthesise a Reaction",
                r.local_id
            )));
        }
    }
    Ok(())
}

fn resolve_reaction(
    db: &mut Database,
    p: &Proposal,
    r: &crate::schema::ProposalReaction,
    slug: &str,
    curator: &str,
    report: &mut MergeReport,
) -> (ReactionId, bool) {
    match &r.reference {
        ReactionRef::Rhea(rhea_id) => {
            if let Some(existing) = find_by_rhea(db, rhea_id) {
                attach_reaction_evidence(db, &existing, p, curator);
                (existing, false)
            } else {
                let id = ReactionId::new(format!("llm_{slug}_{}_rhea{rhea_id}", r.local_id));
                let mut rxn = Reaction::new(
                    id.clone(),
                    r.reversibility.unwrap_or(Reversibility::Forward),
                );
                rxn.rhea_id = Some(rhea_id.clone());
                rxn.add_xref(Source::Rhea, rhea_id);
                rxn.evidence
                    .push(proposal_evidence(p, curator, Confidence::clamp(0.5)));
                db.insert_reaction(rxn);
                report.warnings.push(format!(
                    "rhea:{rhea_id} not in DB; inserted stub without stoichiometry"
                ));
                (id, true)
            }
        }
        ReactionRef::ChebiEc {
            ec,
            substrates,
            products,
        } => {
            let id = ReactionId::new(format!("llm_{slug}_{}", r.local_id));
            let mut rxn = Reaction::new(
                id.clone(),
                r.reversibility.unwrap_or(Reversibility::Forward),
            );
            rxn.ec_numbers.push(*ec);
            for chebi in substrates {
                let cid = ensure_compound(db, chebi, p, curator, report);
                rxn.stoichiometry.push(StoichiometryEntry::substrate(
                    cid,
                    1.0,
                    Compartment::Cytosol,
                ));
            }
            for chebi in products {
                let cid = ensure_compound(db, chebi, p, curator, report);
                rxn.stoichiometry
                    .push(StoichiometryEntry::product(cid, 1.0, Compartment::Cytosol));
            }
            rxn.evidence
                .push(proposal_evidence(p, curator, Confidence::clamp(0.5)));
            db.insert_reaction(rxn);
            (id, true)
        }
    }
}

/// Locate an existing reaction by Rhea ID. Checks both the
/// `rhea_id` scalar and the `xrefs[Rhea]` bag so directional variants
/// or alternate-accession ingestions are both discoverable.
fn find_by_rhea(db: &Database, rhea_id: &str) -> Option<ReactionId> {
    db.reactions
        .values()
        .find(|r| {
            r.rhea_id.as_deref() == Some(rhea_id)
                || r.xrefs
                    .get(&Source::Rhea)
                    .is_some_and(|v| v.iter().any(|x| x == rhea_id))
        })
        .map(|r| r.id.clone())
}

fn ensure_compound(
    db: &mut Database,
    chebi: &str,
    p: &Proposal,
    curator: &str,
    report: &mut MergeReport,
) -> CompoundId {
    if let Some(existing) = find_compound_by_chebi(db, chebi) {
        return existing;
    }
    let id = CompoundId::new(format!("llm_{}", chebi.replace([':', ' '], "_")));
    let mut c = Compound::new(id.clone());
    c.add_xref(Source::Chebi, chebi);
    c.evidence
        .push(proposal_evidence(p, curator, Confidence::clamp(0.3)));
    db.insert_compound(c);
    report.compounds_created.push(id.to_string());
    debug!(chebi, internal_id = %id, "compound minted from proposal");
    id
}

fn find_compound_by_chebi(db: &Database, chebi: &str) -> Option<CompoundId> {
    db.compounds
        .values()
        .find(|c| {
            c.xrefs
                .get(&Source::Chebi)
                .is_some_and(|v| v.iter().any(|x| x == chebi))
        })
        .map(|c| c.id.clone())
}

fn attach_reaction_evidence(db: &mut Database, id: &ReactionId, p: &Proposal, curator: &str) {
    if let Some(r) = db.reactions.get_mut(id) {
        r.evidence
            .push(proposal_evidence(p, curator, Confidence::clamp(0.5)));
    }
}

fn attach_enzyme(
    db: &mut Database,
    e: &EnzymeRef,
    local_to_reaction: &BTreeMap<String, ReactionId>,
    p: &Proposal,
    curator: &str,
    report: &mut MergeReport,
) -> usize {
    let mut attached = 0;
    for local_id in &e.catalyses {
        let Some(reaction_id) = local_to_reaction.get(local_id) else {
            report.warnings.push(format!(
                "enzyme {} catalyses unknown local reaction {local_id}",
                e.uniprot
            ));
            continue;
        };
        let Some(rxn) = db.reactions.get_mut(reaction_id) else {
            continue;
        };
        if !rxn.enzymes.iter().any(|x| x == &e.uniprot) {
            rxn.enzymes.push(e.uniprot.clone());
        }
        rxn.add_xref(Source::Uniprot, &e.uniprot);
        rxn.evidence
            .push(proposal_evidence(p, curator, Confidence::clamp(0.4)));
        attached += 1;
    }
    attached
}

fn build_pathway(
    id: &PathwayId,
    p: &Proposal,
    local_to_reaction: &BTreeMap<String, ReactionId>,
    curator: &str,
    report: &mut MergeReport,
) -> Pathway {
    let mut pathway = Pathway::new(id.clone(), &p.target.pathway_name);
    pathway.organism_scope = p
        .target
        .organism_scope
        .as_deref()
        .map_or(OrganismScope::Universal, |s| {
            OrganismScope::Description(s.to_string())
        });
    for r in &p.reactions {
        if let Some(rid) = local_to_reaction.get(&r.local_id) {
            pathway.reactions.push(rid.clone());
        }
    }
    for edge in &p.dag {
        let (Some(from), Some(to)) = (
            local_to_reaction.get(&edge.from),
            local_to_reaction.get(&edge.to),
        ) else {
            report.warnings.push(format!(
                "DAG edge {}->{} references unknown local id",
                edge.from, edge.to
            ));
            continue;
        };
        pathway.dag.push((from.clone(), to.clone()));
    }
    // Pathway evidence carries the proposal hash + every cited PMID.
    pathway
        .evidence
        .push(proposal_evidence(p, curator, Confidence::clamp(0.5)));
    for cite in &p.citations {
        let mut e = proposal_evidence(p, curator, Confidence::clamp(0.4));
        e.citation = Some(cite.pmid.clone());
        pathway.evidence.push(e);
        report.citations_attached += 1;
    }
    pathway
}

fn proposal_evidence(p: &Proposal, curator: &str, confidence: Confidence) -> Evidence {
    let mut e = Evidence::from_source(Source::LlmProposal, confidence);
    e.proposal_hash = Some(p.proposal_id.clone());
    e.curator = Some(curator.to_string());
    e.verifier_log = Some(format!(
        "llm_merge at {} via {}",
        Utc::now().to_rfc3339(),
        p.model,
    ));
    e
}

fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "pathway".into()
    } else {
        trimmed
    }
}

fn proposal_hash_tail(proposal_id: &str) -> String {
    let hex = proposal_id.trim_start_matches("sha256:");
    hex.chars().take(12).collect()
}

fn short_ref(r: &ReactionRef) -> String {
    match r {
        ReactionRef::Rhea(id) => format!("rhea:{id}"),
        ReactionRef::ChebiEc { ec, .. } => format!("ec:{ec}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        EnzymeRef, Proposal, ProposalCitation, ProposalEdge, ProposalReaction, ProposalTarget,
        SCHEMA_VERSION,
    };
    use chrono::TimeZone;
    use gapsmith_db_core::{EcNumber, Pmid};

    fn proposal_rhea_only() -> Proposal {
        Proposal {
            schema_version: SCHEMA_VERSION.to_string(),
            proposal_id: String::new(),
            created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            model: "test/mock".into(),
            prompt_version: "test".into(),
            target: ProposalTarget {
                pathway_name: "Test pathway".into(),
                organism_scope: None,
                medium: None,
                notes: None,
            },
            reactions: vec![
                ProposalReaction {
                    local_id: "R1".into(),
                    reference: ReactionRef::Rhea("22636".into()),
                    reversibility: Some(Reversibility::Forward),
                    equation_hint: None,
                },
                ProposalReaction {
                    local_id: "R2".into(),
                    reference: ReactionRef::Rhea("24384".into()),
                    reversibility: Some(Reversibility::Forward),
                    equation_hint: None,
                },
            ],
            enzymes: vec![EnzymeRef {
                uniprot: "P23940".into(),
                catalyses: vec!["R1".into()],
                function: None,
            }],
            dag: vec![ProposalEdge {
                from: "R1".into(),
                to: "R2".into(),
            }],
            citations: vec![ProposalCitation {
                pmid: Pmid::new("24123366"),
                note: None,
            }],
            rationale: None,
        }
        .hashed()
    }

    fn proposal_chebi_ec() -> Proposal {
        let ec: EcNumber = "1.2.7.12".parse().unwrap();
        Proposal {
            schema_version: SCHEMA_VERSION.to_string(),
            proposal_id: String::new(),
            created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            model: "test/mock".into(),
            prompt_version: "test".into(),
            target: ProposalTarget {
                pathway_name: "EC fallback pathway".into(),
                organism_scope: Some("Methanothermobacter marburgensis".into()),
                medium: None,
                notes: None,
            },
            reactions: vec![ProposalReaction {
                local_id: "R1".into(),
                reference: ReactionRef::ChebiEc {
                    ec,
                    substrates: vec!["CHEBI:16526".into(), "CHEBI:17805".into()],
                    products: vec!["CHEBI:58435".into()],
                },
                reversibility: Some(Reversibility::Forward),
                equation_hint: None,
            }],
            enzymes: vec![],
            dag: vec![],
            citations: vec![],
            rationale: None,
        }
        .hashed()
    }

    #[test]
    fn rhea_miss_creates_stub_and_warns() {
        let mut db = Database::new();
        let report = merge_proposal(&mut db, &proposal_rhea_only(), "test").unwrap();
        assert_eq!(report.reactions_created.len(), 2);
        assert!(report.warnings.iter().any(|w| w.contains("rhea:22636")));
        assert_eq!(db.reactions.len(), 2);
        assert_eq!(db.pathways.len(), 1);
        // Enzyme attached to R1.
        let r1 = db.reactions.values().next().unwrap();
        assert!(r1.enzymes.contains(&"P23940".to_string()));
    }

    #[test]
    fn rhea_hit_links_without_stub() {
        let mut db = Database::new();
        let mut existing = Reaction::new(ReactionId::new("R_existing"), Reversibility::Reversible);
        existing.rhea_id = Some("22636".into());
        db.insert_reaction(existing);
        let mut existing2 =
            Reaction::new(ReactionId::new("R_existing2"), Reversibility::Reversible);
        existing2.rhea_id = Some("24384".into());
        db.insert_reaction(existing2);

        let report = merge_proposal(&mut db, &proposal_rhea_only(), "test").unwrap();
        assert_eq!(report.reactions_linked.len(), 2);
        assert_eq!(report.reactions_created.len(), 0);
        assert!(report.warnings.is_empty());
        // The existing reaction now has proposal evidence.
        let r = db.reactions.get(&ReactionId::new("R_existing")).unwrap();
        assert!(
            r.evidence
                .iter()
                .any(|e| e.source == Source::LlmProposal && e.proposal_hash.is_some())
        );
    }

    #[test]
    fn chebi_ec_synthesises_reaction_and_compounds() {
        let mut db = Database::new();
        let report = merge_proposal(&mut db, &proposal_chebi_ec(), "test").unwrap();
        assert_eq!(report.reactions_created.len(), 1);
        // 2 substrates + 1 product = 3 compounds minted.
        assert_eq!(report.compounds_created.len(), 3);
        assert_eq!(db.compounds.len(), 3);
        assert_eq!(db.reactions.len(), 1);
        // The reaction has 3 stoichiometry entries (2 subs, 1 prod).
        let rxn = db.reactions.values().next().unwrap();
        assert_eq!(rxn.substrates().count(), 2);
        assert_eq!(rxn.products().count(), 1);
        assert_eq!(rxn.ec_numbers.len(), 1);
        db.validate().unwrap();
    }

    #[test]
    fn chebi_ec_reuses_existing_compound_by_chebi_xref() {
        let mut db = Database::new();
        let mut c = Compound::new(CompoundId::new("C_water"));
        c.add_xref(Source::Chebi, "CHEBI:16526");
        db.insert_compound(c);

        let report = merge_proposal(&mut db, &proposal_chebi_ec(), "test").unwrap();
        // Only 2 new compounds (the other substrate + the product); water reused.
        assert_eq!(report.compounds_created.len(), 2);
        assert_eq!(db.compounds.len(), 3);
    }

    #[test]
    fn dag_edges_preserved_on_pathway() {
        let mut db = Database::new();
        merge_proposal(&mut db, &proposal_rhea_only(), "test").unwrap();
        let pathway = db.pathways.values().next().unwrap();
        assert_eq!(pathway.reactions.len(), 2);
        assert_eq!(pathway.dag.len(), 1);
    }

    #[test]
    fn empty_substrates_rejected() {
        let ec: EcNumber = "1.1.1.1".parse().unwrap();
        let mut bad = proposal_chebi_ec();
        bad.reactions[0].reference = ReactionRef::ChebiEc {
            ec,
            substrates: vec![],
            products: vec!["CHEBI:58435".into()],
        };
        let mut db = Database::new();
        let err = merge_proposal(&mut db, &bad, "test").unwrap_err();
        assert!(matches!(err, ProposeError::Schema(_)));
    }

    #[test]
    fn citations_become_pathway_evidence() {
        let mut db = Database::new();
        let report = merge_proposal(&mut db, &proposal_rhea_only(), "test").unwrap();
        assert_eq!(report.citations_attached, 1);
        let pathway = db.pathways.values().next().unwrap();
        assert!(pathway.evidence.iter().any(|e| {
            e.citation
                .as_ref()
                .is_some_and(|p| p.as_str() == "24123366")
        }));
    }

    #[test]
    fn slug_is_filesystem_safe() {
        assert_eq!(
            slug("Hydrogenotrophic methanogenesis"),
            "hydrogenotrophic_methanogenesis"
        );
        assert_eq!(slug("TCA / Krebs cycle"), "tca_krebs_cycle");
        assert_eq!(slug(""), "pathway");
    }
}

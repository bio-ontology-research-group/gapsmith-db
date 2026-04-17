//! Compound dedup + canonical ID assignment.
//!
//! Plan.md: "Deduplicate compounds via InChIKey first, then MNXref, then
//! name match (last resort, flagged)."
//!
//! The merger takes a list of `IngestBundle`s and produces a canonical
//! [`Database`] with stable internal IDs.

use std::collections::HashMap;

use gapsmith_db_core::{
    Compartment, Compound, CompoundId, Confidence, Database, Evidence, MergeFlag, Reaction,
    ReactionId, Reversibility, Source, StoichiometryEntry,
};
use tracing::debug;

use crate::parse::{IngestBundle, ParsedCompound, ParsedReaction};

/// Canonical compound key-key map after dedup.
struct CompoundIndex {
    /// Canonical internal ID per resolved compound.
    ids: Vec<CompoundId>,
    /// Source → native_id → canonical index.
    by_source: HashMap<(Source, String), usize>,
    /// InChIKey → canonical index.
    by_inchikey: HashMap<String, usize>,
    /// Lowercased preferred name → canonical index (last-resort match).
    by_name_lc: HashMap<String, usize>,
}

impl CompoundIndex {
    fn new() -> Self {
        Self {
            ids: Vec::new(),
            by_source: HashMap::new(),
            by_inchikey: HashMap::new(),
            by_name_lc: HashMap::new(),
        }
    }

    fn canonical(&self, source: Source, native: &str) -> Option<usize> {
        self.by_source.get(&(source, native.to_string())).copied()
    }
}

#[allow(clippy::too_many_lines)]
pub fn merge(bundles: &[IngestBundle]) -> Database {
    let mut db = Database::new();
    let mut idx = CompoundIndex::new();
    let mut next_cpd: u64 = 1;

    // Pass 1: absorb compounds with InChIKey first.
    for bundle in bundles {
        for pc in &bundle.compounds {
            if let Some(ik) = pc.inchikey.as_deref() {
                if let Some(&canon) = idx.by_inchikey.get(ik) {
                    absorb_into(&mut db, &idx.ids[canon], pc, MergeFlag::InchikeyMatched);
                    register_source(&mut idx, canon, pc);
                } else {
                    let cid = mint(&mut next_cpd);
                    let canon = insert_new(&mut db, &mut idx, cid, pc);
                    idx.by_inchikey.insert(ik.to_string(), canon);
                }
            }
        }
    }

    // Pass 2: compounds without InChIKey. Try MNXref cross-ref first, then
    // existing source-id match, then name, else mint fresh.
    for bundle in bundles {
        for pc in &bundle.compounds {
            if pc.inchikey.is_some() {
                continue;
            }
            if let Some(canon) = find_via_mnxref(&idx, bundles, pc) {
                absorb_into(&mut db, &idx.ids[canon], pc, MergeFlag::MnxrefMatched);
                register_source(&mut idx, canon, pc);
                continue;
            }
            if let Some(canon) = idx.canonical(pc.source, &pc.native_id) {
                absorb_into(&mut db, &idx.ids[canon], pc, MergeFlag::MnxrefMatched);
                continue;
            }
            if let Some(canon) = find_by_name(&idx, &pc.names) {
                absorb_into(&mut db, &idx.ids[canon], pc, MergeFlag::NameMatched);
                register_source(&mut idx, canon, pc);
                continue;
            }
            let cid = mint(&mut next_cpd);
            insert_new(&mut db, &mut idx, cid, pc);
        }
    }

    // Absorb standalone xref rows (MNXref chem_xref, gapseq corrections, etc.)
    for bundle in bundles {
        for xref in &bundle.compound_xrefs {
            if let Some(canon) = idx.canonical(xref.from_source, &xref.from_id)
                && let Some(c) = db.compounds.get_mut(&idx.ids[canon])
            {
                c.add_xref(xref.to_source, xref.to_id.clone());
            } else if let Some(canon) = idx.canonical(xref.to_source, &xref.to_id)
                && let Some(c) = db.compounds.get_mut(&idx.ids[canon])
            {
                c.add_xref(xref.from_source, xref.from_id.clone());
            }
        }
    }

    // Reactions: dedup by Rhea ID first, then SEED ID, else fresh.
    let mut next_rxn: u64 = 1;
    let mut rxn_by_source: HashMap<(Source, String), ReactionId> = HashMap::new();
    let mut rxn_by_rhea: HashMap<String, ReactionId> = HashMap::new();
    let mut rxn_by_seed: HashMap<String, ReactionId> = HashMap::new();

    for bundle in bundles {
        for pr in &bundle.reactions {
            let canonical_rxn = resolve_reaction(pr, &rxn_by_source, &rxn_by_rhea, &rxn_by_seed);
            let rid = canonical_rxn.unwrap_or_else(|| mint_rxn(&mut next_rxn));
            let is_new = !db.reactions.contains_key(&rid);
            if is_new {
                let mut new_r = Reaction::new(
                    rid.clone(),
                    pr.reversibility.unwrap_or(Reversibility::Reversible),
                );
                new_r.rhea_id.clone_from(&pr.rhea_id);
                new_r.seed_id.clone_from(&pr.seed_id);
                new_r.is_transport = pr.is_transport;
                new_r.names.clone_from(&pr.names);
                for ec in &pr.ec_numbers {
                    if let Ok(parsed) = ec.parse() {
                        new_r.ec_numbers.push(parsed);
                    }
                }
                new_r.add_xref(pr.source, &pr.native_id);
                new_r.stoichiometry = resolve_stoichiometry(&pr.stoichiometry, &idx);
                new_r
                    .evidence
                    .push(Evidence::from_source(pr.source, Confidence::CERTAIN));
                db.insert_reaction(new_r);
            } else if let Some(r) = db.reactions.get_mut(&rid) {
                r.add_xref(pr.source, &pr.native_id);
                for ec in &pr.ec_numbers {
                    if let Ok(parsed) = ec.parse()
                        && !r.ec_numbers.contains(&parsed)
                    {
                        r.ec_numbers.push(parsed);
                    }
                }
                r.evidence
                    .push(Evidence::from_source(pr.source, Confidence::CERTAIN));
            }
            rxn_by_source.insert((pr.source, pr.native_id.clone()), rid.clone());
            if let Some(rhea) = pr.rhea_id.as_deref() {
                rxn_by_rhea.insert(rhea.to_string(), rid.clone());
            }
            if let Some(seed) = pr.seed_id.as_deref() {
                rxn_by_seed.insert(seed.to_string(), rid.clone());
            }
        }
    }

    debug!(
        compounds = db.compounds.len(),
        reactions = db.reactions.len(),
        "merge complete"
    );
    db
}

fn mint(counter: &mut u64) -> CompoundId {
    let id = CompoundId::new(format!("C{:07}", *counter));
    *counter += 1;
    id
}

fn mint_rxn(counter: &mut u64) -> ReactionId {
    let id = ReactionId::new(format!("R{:07}", *counter));
    *counter += 1;
    id
}

fn insert_new(
    db: &mut Database,
    idx: &mut CompoundIndex,
    cid: CompoundId,
    pc: &ParsedCompound,
) -> usize {
    let mut c = Compound::new(cid.clone());
    apply_parsed(&mut c, pc);
    c.evidence
        .push(Evidence::from_source(pc.source, Confidence::CERTAIN));
    db.insert_compound(c);
    idx.ids.push(cid);
    let canon = idx.ids.len() - 1;
    register_source(idx, canon, pc);
    canon
}

fn register_source(idx: &mut CompoundIndex, canon: usize, pc: &ParsedCompound) {
    idx.by_source
        .insert((pc.source, pc.native_id.clone()), canon);
    for name in &pc.names {
        idx.by_name_lc.insert(name.to_ascii_lowercase(), canon);
    }
}

fn absorb_into(db: &mut Database, cid: &CompoundId, pc: &ParsedCompound, flag: MergeFlag) {
    let Some(c) = db.compounds.get_mut(cid) else {
        return;
    };
    if c.formula.is_none() {
        c.formula.clone_from(&pc.formula);
    }
    if c.charge.is_none() {
        c.charge = pc.charge;
    }
    if c.inchi.is_none() {
        c.inchi.clone_from(&pc.inchi);
    }
    if c.inchikey.is_none() {
        c.inchikey.clone_from(&pc.inchikey);
    }
    if c.smiles.is_none() {
        c.smiles.clone_from(&pc.smiles);
    }
    if c.mass.is_none() {
        c.mass = pc.mass;
    }
    for n in &pc.names {
        if !c.names.iter().any(|x| x == n) {
            c.names.push(n.clone());
        }
    }
    for r in &pc.chebi_roles {
        if !c.chebi_roles.iter().any(|x| x == r) {
            c.chebi_roles.push(r.clone());
        }
    }
    c.add_xref(pc.source, pc.native_id.clone());
    for (src, ids) in &pc.extra_xrefs {
        for id in ids {
            c.add_xref(*src, id.clone());
        }
    }
    c.evidence
        .push(Evidence::from_source(pc.source, Confidence::CERTAIN).with_flag(flag));
}

fn apply_parsed(c: &mut Compound, pc: &ParsedCompound) {
    c.formula.clone_from(&pc.formula);
    c.charge = pc.charge;
    c.inchi.clone_from(&pc.inchi);
    c.inchikey.clone_from(&pc.inchikey);
    c.smiles.clone_from(&pc.smiles);
    c.mass = pc.mass;
    c.names.clone_from(&pc.names);
    c.chebi_roles.clone_from(&pc.chebi_roles);
    c.add_xref(pc.source, pc.native_id.clone());
    for (src, ids) in &pc.extra_xrefs {
        for id in ids {
            c.add_xref(*src, id.clone());
        }
    }
}

fn find_via_mnxref(
    idx: &CompoundIndex,
    bundles: &[IngestBundle],
    pc: &ParsedCompound,
) -> Option<usize> {
    // If pc has an MNXref xref already, try that.
    if let Some(mnx_ids) = pc.extra_xrefs.get(&Source::Mnxref) {
        for mnx_id in mnx_ids {
            if let Some(canon) = idx.canonical(Source::Mnxref, mnx_id) {
                return Some(canon);
            }
        }
    }
    // Otherwise consult the chem_xref table from any MNXref bundle: does any
    // row map (pc.source, pc.native_id) to an MNX id we already have?
    for bundle in bundles {
        for xref in &bundle.compound_xrefs {
            if xref.from_source == pc.source
                && xref.from_id == pc.native_id
                && let Some(canon) = idx.canonical(xref.to_source, &xref.to_id)
            {
                return Some(canon);
            }
            if xref.to_source == pc.source
                && xref.to_id == pc.native_id
                && let Some(canon) = idx.canonical(xref.from_source, &xref.from_id)
            {
                return Some(canon);
            }
        }
    }
    None
}

fn find_by_name(idx: &CompoundIndex, names: &[String]) -> Option<usize> {
    for n in names {
        if let Some(&canon) = idx.by_name_lc.get(&n.to_ascii_lowercase()) {
            return Some(canon);
        }
    }
    None
}

fn resolve_stoichiometry(
    entries: &[crate::parse::ParsedReactionEntry],
    idx: &CompoundIndex,
) -> Vec<StoichiometryEntry> {
    entries
        .iter()
        .filter_map(|e| {
            let canon = idx.canonical(e.compound_source, &e.native_compound)?;
            let compartment = Compartment::from_code(&e.compartment_code);
            Some(StoichiometryEntry {
                compound: idx.ids[canon].clone(),
                coefficient: e.coefficient,
                compartment,
            })
        })
        .collect()
}

fn resolve_reaction(
    pr: &ParsedReaction,
    by_source: &HashMap<(Source, String), ReactionId>,
    by_rhea: &HashMap<String, ReactionId>,
    by_seed: &HashMap<String, ReactionId>,
) -> Option<ReactionId> {
    if let Some(rhea) = pr.rhea_id.as_deref()
        && let Some(rid) = by_rhea.get(rhea)
    {
        return Some(rid.clone());
    }
    if let Some(seed) = pr.seed_id.as_deref()
        && let Some(rid) = by_seed.get(seed)
    {
        return Some(rid.clone());
    }
    by_source.get(&(pr.source, pr.native_id.clone())).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{ParsedCompound, ParsedReaction};

    #[test]
    fn inchikey_dedup_merges_sources() {
        let mut seed = ParsedCompound::new(Source::Modelseed, "cpd00001");
        seed.names.push("H2O".into());
        seed.inchikey = Some("XLYOFNOQVPJJNP-UHFFFAOYSA-N".into());
        let mut chebi = ParsedCompound::new(Source::Chebi, "CHEBI:15377");
        chebi.names.push("water".into());
        chebi.formula = Some("H2O".into());
        chebi.inchikey = Some("XLYOFNOQVPJJNP-UHFFFAOYSA-N".into());

        let b1 = IngestBundle {
            source: Some(Source::Modelseed),
            compounds: vec![seed],
            ..Default::default()
        };
        let b2 = IngestBundle {
            source: Some(Source::Chebi),
            compounds: vec![chebi],
            ..Default::default()
        };
        let db = merge(&[b1, b2]);
        assert_eq!(db.compounds.len(), 1);
        let only = db.compounds.values().next().unwrap();
        assert!(only.has_source(Source::Chebi));
        assert!(only.has_source(Source::Modelseed));
        assert_eq!(only.formula.as_deref(), Some("H2O"));
    }

    #[test]
    fn rhea_dedup_merges_reaction_ec() {
        let mut r1 = ParsedReaction::new(Source::Rhea, "10001");
        r1.rhea_id = Some("10001".into());
        r1.ec_numbers = vec!["1.1.1.1".into()];
        let mut r2 = ParsedReaction::new(Source::Modelseed, "rxn00001");
        r2.rhea_id = Some("10001".into());
        r2.seed_id = Some("rxn00001".into());
        r2.ec_numbers = vec!["1.1.1.2".into()];

        let b = IngestBundle {
            reactions: vec![r1, r2],
            ..Default::default()
        };
        let db = merge(&[b]);
        assert_eq!(db.reactions.len(), 1);
        let only = db.reactions.values().next().unwrap();
        assert_eq!(only.ec_numbers.len(), 2);
    }

    #[test]
    fn name_match_is_flagged() {
        let mut a = ParsedCompound::new(Source::Chebi, "CHEBI:X");
        a.names.push("glucose".into());
        let mut b = ParsedCompound::new(Source::Modelseed, "cpd_glucose");
        b.names.push("Glucose".into()); // case-insensitive match
        let bundle = IngestBundle {
            compounds: vec![a, b],
            ..Default::default()
        };
        let db = merge(&[bundle]);
        assert_eq!(db.compounds.len(), 1);
        let c = db.compounds.values().next().unwrap();
        assert!(
            c.evidence
                .iter()
                .any(|e| e.flags.contains(&MergeFlag::NameMatched))
        );
    }
}

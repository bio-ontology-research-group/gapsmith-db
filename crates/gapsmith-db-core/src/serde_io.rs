//! Serialisation: human-diffable TSV + compact bincode binary.
//!
//! Contract per plan.md:
//! - TSV is one row per entity with list fields joined by `;` inside cells
//!   and records separated by `\n`. Each top-level table lands in a
//!   separate file so `git diff` stays local to the entity that changed.
//! - Binary is `bincode` over the `Database` for fast load (single file).
//!
//! `write_tsv_dir` emits a directory containing:
//!   - `compounds.tsv`
//!   - `reactions.tsv`
//!   - `pathways.tsv`
//!   - `evidence.tsv` (flat form; one row per (entity, evidence) pair)
//!   - `stats.json`

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::compound::Compound;
use crate::database::Database;
use crate::evidence::Evidence;
use crate::ids::{CompoundId, PathwayId, ReactionId};
use crate::pathway::Pathway;
use crate::reaction::Reaction;
use crate::{CoreError, Result};

/// Magic + version header for the binary format. A trailing one-byte bump
/// is enough for on-disk layout changes.
const BINARY_MAGIC: &[u8; 8] = b"GAPSMITH";
const BINARY_VERSION: u8 = 1;

// ---------------------------------------------------------------------------
// Binary (bincode) I/O
// ---------------------------------------------------------------------------

pub fn write_binary(db: &Database, path: &Path) -> Result<()> {
    db.validate()
        .map_err(|e| CoreError::Invariant(e.to_string()))?;
    let f = File::create(path)?;
    let mut w = BufWriter::new(f);
    w.write_all(BINARY_MAGIC)?;
    w.write_all(&[BINARY_VERSION])?;
    bincode::serialize_into(&mut w, db)?;
    w.flush()?;
    Ok(())
}

pub fn read_binary(path: &Path) -> Result<Database> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 9 {
        return Err(CoreError::Other("binary file too short for header".into()));
    }
    let (magic, rest) = bytes.split_at(8);
    if magic != BINARY_MAGIC.as_slice() {
        return Err(CoreError::Other(format!(
            "binary magic mismatch (got {magic:?}, want {BINARY_MAGIC:?})"
        )));
    }
    let (ver, payload) = rest.split_at(1);
    if ver[0] != BINARY_VERSION {
        return Err(CoreError::Other(format!(
            "binary version mismatch (got {}, want {BINARY_VERSION})",
            ver[0]
        )));
    }
    let db: Database = bincode::deserialize(payload)?;
    db.validate()
        .map_err(|e| CoreError::Invariant(e.to_string()))?;
    Ok(db)
}

// ---------------------------------------------------------------------------
// TSV I/O
// ---------------------------------------------------------------------------

pub fn write_tsv_dir(db: &Database, dir: &Path) -> Result<()> {
    db.validate()
        .map_err(|e| CoreError::Invariant(e.to_string()))?;
    std::fs::create_dir_all(dir)?;
    write_compounds_tsv(db, &dir.join("compounds.tsv"))?;
    write_reactions_tsv(db, &dir.join("reactions.tsv"))?;
    write_pathways_tsv(db, &dir.join("pathways.tsv"))?;
    write_evidence_tsv(db, &dir.join("evidence.tsv"))?;
    let stats = db.stats();
    let mut f = File::create(dir.join("stats.json"))?;
    f.write_all(serde_json::to_string_pretty(&stats)?.as_bytes())?;
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct CompoundRow {
    id: String,
    formula: String,
    charge: String,
    inchikey: String,
    inchi: String,
    smiles: String,
    mass: String,
    names: String,       // ;-joined
    chebi_roles: String, // ;-joined
    xrefs: String,       // source=id1,id2; ...
}

#[derive(Debug, Serialize, Deserialize)]
struct ReactionRow {
    id: String,
    reversibility: String,
    is_transport: String,
    status: String,
    stoichiometry: String, // coef*compound@compartment ; ...
    ec_numbers: String,    // ;-joined
    rhea_id: String,
    seed_id: String,
    delta_g: String,
    names: String,
    xrefs: String,
    #[serde(default)]
    enzymes: String, // ;-joined Swiss-Prot accessions
}

#[derive(Debug, Serialize, Deserialize)]
struct PathwayRow {
    id: String,
    name: String,
    reactions: String, // ;-joined
    variant_of: String,
    organism_scope: String, // JSON
    #[serde(default)]
    dag: String, // "from->to;from->to;..."
}

#[derive(Debug, Serialize, Deserialize)]
struct EvidenceRow {
    entity_kind: String,
    entity_id: String,
    source: String,
    citation: String,
    curator: String,
    proposal_hash: String,
    verifier_log: String,
    confidence: String,
    flags: String,
}

fn opt_string<T: std::fmt::Display>(o: Option<&T>) -> String {
    o.map_or_else(String::new, ToString::to_string)
}

fn join_semi<I, S>(iter: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    iter.into_iter()
        .map(|s| s.as_ref().to_string())
        .collect::<Vec<_>>()
        .join(";")
}

fn compound_to_row(c: &Compound) -> CompoundRow {
    CompoundRow {
        id: c.id.to_string(),
        formula: c.formula.clone().unwrap_or_default(),
        charge: c.charge.map_or_else(String::new, |n| n.to_string()),
        inchikey: c.inchikey.clone().unwrap_or_default(),
        inchi: c.inchi.clone().unwrap_or_default(),
        smiles: c.smiles.clone().unwrap_or_default(),
        mass: c.mass.map_or_else(String::new, |m| format!("{m:.6}")),
        names: join_semi(c.names.iter()),
        chebi_roles: join_semi(c.chebi_roles.iter()),
        xrefs: c
            .xrefs
            .iter()
            .map(|(s, ids)| format!("{}={}", source_code(*s), ids.join(",")))
            .collect::<Vec<_>>()
            .join(";"),
    }
}

fn reaction_to_row(r: &Reaction) -> ReactionRow {
    let stoichiometry = r
        .stoichiometry
        .iter()
        .map(|s| format!("{}*{}@{}", s.coefficient, s.compound, s.compartment))
        .collect::<Vec<_>>()
        .join(";");
    let xrefs = r
        .xrefs
        .iter()
        .map(|(s, ids)| format!("{}={}", source_code(*s), ids.join(",")))
        .collect::<Vec<_>>()
        .join(";");
    ReactionRow {
        id: r.id.to_string(),
        reversibility: format!("{:?}", r.reversibility).to_lowercase(),
        is_transport: r.is_transport.to_string(),
        status: format!("{:?}", r.status).to_lowercase(),
        stoichiometry,
        ec_numbers: join_semi(r.ec_numbers.iter().map(ToString::to_string)),
        rhea_id: r.rhea_id.clone().unwrap_or_default(),
        seed_id: r.seed_id.clone().unwrap_or_default(),
        delta_g: r
            .delta_g
            .map_or_else(String::new, |(v, u)| format!("{v};{u}")),
        names: join_semi(r.names.iter()),
        xrefs,
        enzymes: join_semi(r.enzymes.iter()),
    }
}

fn pathway_to_row(p: &Pathway) -> Result<PathwayRow> {
    let dag = p
        .dag
        .iter()
        .map(|(a, b)| format!("{a}->{b}"))
        .collect::<Vec<_>>()
        .join(";");
    Ok(PathwayRow {
        id: p.id.to_string(),
        name: p.name.clone(),
        reactions: join_semi(p.reactions.iter().map(ToString::to_string)),
        variant_of: opt_string(p.variant_of.as_ref()),
        organism_scope: serde_json::to_string(&p.organism_scope)?,
        dag,
    })
}

fn evidence_rows<'a>(
    entity_kind: &'a str,
    entity_id: &'a str,
    evidence: &'a [Evidence],
) -> impl Iterator<Item = EvidenceRow> + 'a {
    evidence.iter().map(move |e| EvidenceRow {
        entity_kind: entity_kind.to_string(),
        entity_id: entity_id.to_string(),
        source: source_code(e.source).to_string(),
        citation: opt_string(e.citation.as_ref()),
        curator: e.curator.clone().unwrap_or_default(),
        proposal_hash: e.proposal_hash.clone().unwrap_or_default(),
        verifier_log: e.verifier_log.clone().unwrap_or_default(),
        confidence: format!("{:.4}", e.confidence.value()),
        flags: e
            .flags
            .iter()
            .map(|f| format!("{f:?}"))
            .collect::<Vec<_>>()
            .join(","),
    })
}

fn source_code(s: crate::source::Source) -> &'static str {
    use crate::source::Source;
    match s {
        Source::Chebi => "chebi",
        Source::Modelseed => "modelseed",
        Source::Mnxref => "mnxref",
        Source::Rhea => "rhea",
        Source::Intenz => "intenz",
        Source::Uniprot => "uniprot",
        Source::Reactome => "reactome",
        Source::Gapseq => "gapseq",
        Source::Kegg => "kegg",
        Source::Inchikey => "inchikey",
        Source::Pubchem => "pubchem",
        Source::LlmProposal => "llm_proposal",
        Source::Other => "other",
    }
}

fn tsv_writer(path: &Path) -> Result<csv::Writer<BufWriter<File>>> {
    let f = File::create(path)?;
    Ok(csv::WriterBuilder::new()
        .delimiter(b'\t')
        .from_writer(BufWriter::new(f)))
}

fn write_compounds_tsv(db: &Database, path: &Path) -> Result<()> {
    let mut w = tsv_writer(path)?;
    for c in db.compounds.values() {
        w.serialize(compound_to_row(c))?;
    }
    w.flush()?;
    Ok(())
}

fn write_reactions_tsv(db: &Database, path: &Path) -> Result<()> {
    let mut w = tsv_writer(path)?;
    for r in db.reactions.values() {
        w.serialize(reaction_to_row(r))?;
    }
    w.flush()?;
    Ok(())
}

fn write_pathways_tsv(db: &Database, path: &Path) -> Result<()> {
    let mut w = tsv_writer(path)?;
    for p in db.pathways.values() {
        w.serialize(pathway_to_row(p)?)?;
    }
    w.flush()?;
    Ok(())
}

fn write_evidence_tsv(db: &Database, path: &Path) -> Result<()> {
    let mut w = tsv_writer(path)?;
    for c in db.compounds.values() {
        for row in evidence_rows("compound", c.id.as_str(), &c.evidence) {
            w.serialize(row)?;
        }
    }
    for r in db.reactions.values() {
        for row in evidence_rows("reaction", r.id.as_str(), &r.evidence) {
            w.serialize(row)?;
        }
    }
    for p in db.pathways.values() {
        for row in evidence_rows("pathway", p.id.as_str(), &p.evidence) {
            w.serialize(row)?;
        }
    }
    w.flush()?;
    Ok(())
}

// Silence unused-import warnings on types referenced only through macros /
// generics once all codepaths are exercised.
#[allow(dead_code)]
fn _refs() -> (CompoundId, ReactionId, PathwayId) {
    (CompoundId::new(""), ReactionId::new(""), PathwayId::new(""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compartment::Compartment;
    use crate::compound::Compound;
    use crate::evidence::{Confidence, Evidence};
    use crate::ids::{CompoundId, PathwayId, ReactionId};
    use crate::pathway::Pathway;
    use crate::reaction::{Reaction, StoichiometryEntry};
    use crate::reversibility::Reversibility;
    use crate::source::Source;

    fn sample_db() -> Database {
        let mut db = Database::new();
        let mut c = Compound::new(CompoundId::new("C1"));
        c.formula = Some("H2O".into());
        c.charge = Some(0);
        c.inchikey = Some("XLYOFNOQVPJJNP-UHFFFAOYSA-N".into());
        c.names.push("water".into());
        c.add_xref(Source::Chebi, "CHEBI:15377");
        c.evidence
            .push(Evidence::from_source(Source::Chebi, Confidence::CERTAIN));
        db.insert_compound(c);

        let mut c2 = Compound::new(CompoundId::new("C2"));
        c2.formula = Some("O".into());
        db.insert_compound(c2);

        let mut r = Reaction::new(ReactionId::new("R1"), Reversibility::Reversible);
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
        r.rhea_id = Some("10001".into());
        db.insert_reaction(r);

        let mut p = Pathway::new(PathwayId::new("P1"), "test pathway");
        p.reactions.push(ReactionId::new("R1"));
        db.insert_pathway(p);
        db
    }

    #[test]
    fn binary_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.gapsmith");
        let db = sample_db();
        write_binary(&db, &path).unwrap();
        let back = read_binary(&path).unwrap();
        assert_eq!(back.compounds.len(), db.compounds.len());
        assert_eq!(back.reactions.len(), db.reactions.len());
        assert_eq!(
            back.compounds[&CompoundId::new("C1")].inchikey,
            Some("XLYOFNOQVPJJNP-UHFFFAOYSA-N".into())
        );
    }

    #[test]
    fn tsv_dir_emits_four_tables() {
        let dir = tempfile::tempdir().unwrap();
        let db = sample_db();
        write_tsv_dir(&db, dir.path()).unwrap();
        for f in [
            "compounds.tsv",
            "reactions.tsv",
            "pathways.tsv",
            "evidence.tsv",
            "stats.json",
        ] {
            assert!(dir.path().join(f).exists(), "missing {f}");
        }
    }

    #[test]
    fn binary_rejects_bad_magic() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("bad");
        std::fs::write(&p, b"NOTMAGIC\x01more").unwrap();
        assert!(read_binary(&p).is_err());
    }
}

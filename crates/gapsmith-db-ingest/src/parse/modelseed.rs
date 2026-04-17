//! ModelSEED `Biochemistry/{compounds,reactions}.tsv` + subsystems.
//!
//! ModelSEED ships two big TSVs with a heading row. We consume:
//!   id, formula, charge, inchikey, smiles, mass, name, aliases, is_obsolete.
//!
//! Obsolete entries are skipped. Aliases are split on `|` and then
//! `:` (source:id) per ModelSEED convention.

use std::collections::BTreeMap;
use std::path::Path;

use gapsmith_db_core::source::Source;
use tracing::warn;

use super::ir::{IngestBundle, ParsedCompound, ParsedReaction, ParsedReactionEntry};
use crate::{IngestError, Result};

pub fn parse_dir(dir: &Path) -> Result<IngestBundle> {
    let mut bundle = IngestBundle {
        source: Some(Source::Modelseed),
        ..Default::default()
    };
    let compounds = find_one(dir, &["compounds.tsv", "Biochemistry/compounds.tsv"])?;
    if let Some(p) = compounds {
        parse_compounds(&p, &mut bundle)?;
    }
    let reactions = find_one(dir, &["reactions.tsv", "Biochemistry/reactions.tsv"])?;
    if let Some(p) = reactions {
        parse_reactions(&p, &mut bundle)?;
    }
    Ok(bundle)
}

fn find_one(dir: &Path, candidates: &[&str]) -> Result<Option<std::path::PathBuf>> {
    for cand in candidates {
        let p = dir.join(cand);
        if p.exists() {
            return Ok(Some(p));
        }
        // also look under any ModelSEEDDatabase-* dir (tar.gz extraction).
        for entry in std::fs::read_dir(dir)?.flatten() {
            let sub = entry.path().join(cand);
            if sub.exists() {
                return Ok(Some(sub));
            }
        }
    }
    Ok(None)
}

fn parse_compounds(path: &Path, bundle: &mut IngestBundle) -> Result<()> {
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| IngestError::Other(format!("modelseed compounds.tsv: {e}")))?;
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| IngestError::Other(format!("read headers: {e}")))?
        .iter()
        .map(str::to_string)
        .collect();

    for (i, rec) in rdr.records().enumerate() {
        let Ok(rec) = rec else {
            warn!(line = i + 2, "modelseed compounds: skipping bad row");
            continue;
        };
        let get = |key: &str| -> Option<String> {
            headers
                .iter()
                .position(|h| h == key)
                .and_then(|ix| rec.get(ix).map(str::to_string))
                .filter(|s| !s.is_empty() && s != "null")
        };
        let Some(id) = get("id") else { continue };
        if get("is_obsolete").as_deref() == Some("1") {
            continue;
        }
        let mut c = ParsedCompound::new(Source::Modelseed, id);
        c.formula = get("formula");
        c.charge = get("charge").and_then(|s| s.parse().ok());
        c.inchikey = get("inchikey");
        c.smiles = get("smiles");
        c.mass = get("mass").and_then(|s| s.parse().ok());
        if let Some(name) = get("name") {
            c.names.push(name);
        }
        if let Some(aliases) = get("aliases") {
            c.extra_xrefs = parse_aliases(&aliases);
        }
        bundle.compounds.push(c);
    }
    Ok(())
}

fn parse_reactions(path: &Path, bundle: &mut IngestBundle) -> Result<()> {
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| IngestError::Other(format!("modelseed reactions.tsv: {e}")))?;
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| IngestError::Other(format!("read headers: {e}")))?
        .iter()
        .map(str::to_string)
        .collect();

    for (i, rec) in rdr.records().enumerate() {
        let Ok(rec) = rec else {
            warn!(line = i + 2, "modelseed reactions: skipping bad row");
            continue;
        };
        let get = |key: &str| -> Option<String> {
            headers
                .iter()
                .position(|h| h == key)
                .and_then(|ix| rec.get(ix).map(str::to_string))
                .filter(|s| !s.is_empty() && s != "null")
        };
        let Some(id) = get("id") else { continue };
        if get("is_obsolete").as_deref() == Some("1") {
            continue;
        }
        let mut r = ParsedReaction::new(Source::Modelseed, id.clone());
        r.seed_id = Some(id);
        if let Some(name) = get("name") {
            r.names.push(name);
        }
        if let Some(dir) = get("direction") {
            r.reversibility = gapsmith_db_core::Reversibility::from_modelseed(&dir);
        }
        if let Some(ec) = get("ec_numbers") {
            r.ec_numbers = ec.split('|').map(str::trim).map(str::to_string).collect();
        }
        if let Some(eq) = get("stoichiometry") {
            r.stoichiometry = parse_stoichiometry_col(&eq);
        }
        if let Some(trn) = get("is_transport") {
            r.is_transport = trn == "1";
        }
        if let Some(aliases) = get("aliases") {
            r.extra_xrefs = parse_aliases(&aliases);
        }
        bundle.reactions.push(r);
    }
    Ok(())
}

/// ModelSEED `stoichiometry` column: entries separated by `;`, each
/// `coef:compound_id:compartment_index:compartment_code:name`.
fn parse_stoichiometry_col(s: &str) -> Vec<ParsedReactionEntry> {
    s.split(';')
        .filter_map(|chunk| {
            let parts: Vec<&str> = chunk.splitn(5, ':').collect();
            if parts.len() < 3 {
                return None;
            }
            let coef: f64 = parts[0].trim().parse().ok()?;
            let cpd = parts[1].trim().to_string();
            let compartment_code = parts.get(3).copied().unwrap_or("c").trim().to_string();
            Some(ParsedReactionEntry {
                native_compound: cpd,
                compound_source: Source::Modelseed,
                coefficient: coef,
                compartment_code,
            })
        })
        .collect()
}

/// ModelSEED alias column: `source1:id1|source2:id2|...`. Map common
/// source names onto our [`Source`] enum; unknowns go to `Source::Other`
/// with the source name preserved in the ID.
fn parse_aliases(s: &str) -> BTreeMap<Source, Vec<String>> {
    let mut out: BTreeMap<Source, Vec<String>> = BTreeMap::new();
    for chunk in s.split('|') {
        let Some((src, id)) = chunk.split_once(':') else {
            continue;
        };
        let source = match src.trim().to_ascii_lowercase().as_str() {
            "chebi" => Source::Chebi,
            "kegg" | "kegg_glycan" => Source::Kegg,
            "rhea" => Source::Rhea,
            "metanetx" | "mnx" => Source::Mnxref,
            "uniprot" => Source::Uniprot,
            "reactome" => Source::Reactome,
            _ => Source::Other,
        };
        out.entry(source).or_default().push(id.trim().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_minimal_fixture() {
        let dir = tempdir().unwrap();
        let p = dir.path();
        std::fs::create_dir_all(p.join("Biochemistry")).unwrap();
        std::fs::write(
            p.join("Biochemistry/compounds.tsv"),
            "id\tname\tformula\tcharge\tinchikey\tmass\tis_obsolete\taliases\n\
             cpd00001\tH2O\tH2O\t0\tXLYOFNOQVPJJNP-UHFFFAOYSA-N\t18.015\t0\tChEBI:15377|KEGG:C00001\n\
             cpd00002\tglucose\tC6H12O6\t0\t\t180.063\t0\t\n",
        )
        .unwrap();
        std::fs::write(
            p.join("Biochemistry/reactions.tsv"),
            "id\tname\tdirection\tstoichiometry\tec_numbers\tis_obsolete\tis_transport\taliases\n\
             rxn00001\tH2O exchange\t>\t1:cpd00001:0:c:water\t\t0\t0\t\n",
        )
        .unwrap();
        let bundle = parse_dir(p).unwrap();
        assert_eq!(bundle.compounds.len(), 2);
        assert_eq!(bundle.reactions.len(), 1);
        let h2o = &bundle.compounds[0];
        assert_eq!(h2o.native_id, "cpd00001");
        assert_eq!(h2o.inchikey.as_deref(), Some("XLYOFNOQVPJJNP-UHFFFAOYSA-N"));
        assert!(h2o.extra_xrefs.contains_key(&Source::Chebi));
        assert!(h2o.extra_xrefs.contains_key(&Source::Kegg));
    }
}

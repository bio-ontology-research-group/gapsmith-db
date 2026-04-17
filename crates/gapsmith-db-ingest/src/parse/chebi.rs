//! ChEBI parser. Phase-2 scope: `compounds.tsv` + `chemical_data.tsv` for
//! formula/charge/mass; `names.tsv` for synonyms; `chebi.obo` ontology
//! roles are extracted by a lightweight OBO walker (we don't pull a full
//! OWL reasoner here — that's Phase 3).

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

use gapsmith_db_core::source::Source;
use tracing::warn;

use super::ir::{IngestBundle, ParsedCompound};
use crate::Result;

pub fn parse_dir(dir: &Path) -> Result<IngestBundle> {
    let mut bundle = IngestBundle {
        source: Some(Source::Chebi),
        ..Default::default()
    };

    let compounds = existing(dir, "compounds.tsv");
    if let Some(p) = compounds.as_ref() {
        parse_compounds(p, &mut bundle)?;
    }

    // Enrich with chemical_data (formula/charge/mass).
    let chem = existing(dir, "chemical_data.tsv");
    if let Some(p) = chem.as_ref() {
        enrich_with_chemical_data(p, &mut bundle)?;
    }

    // Enrich names.
    let names = existing(dir, "names.tsv");
    if let Some(p) = names.as_ref() {
        enrich_with_names(p, &mut bundle)?;
    }

    // ChEBI ontology roles — scan an OBO file for `relationship: has_role` lines.
    let obo = existing(dir, "chebi.obo");
    if let Some(p) = obo.as_ref() {
        enrich_with_obo_roles(p, &mut bundle)?;
    }

    Ok(bundle)
}

fn existing(dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    let p = dir.join(name);
    p.exists().then_some(p)
}

fn parse_compounds(path: &Path, bundle: &mut IngestBundle) -> Result<()> {
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| crate::IngestError::Other(format!("chebi compounds.tsv: {e}")))?;
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| crate::IngestError::Other(format!("read headers: {e}")))?
        .iter()
        .map(str::to_string)
        .collect();
    for (i, rec) in rdr.records().enumerate() {
        let Ok(rec) = rec else {
            warn!(line = i + 2, "chebi compounds: skipping bad row");
            continue;
        };
        let get = |k: &str| -> Option<String> {
            headers
                .iter()
                .position(|h| h == k)
                .and_then(|ix| rec.get(ix).map(str::to_string))
                .filter(|s| !s.is_empty() && s != "null")
        };
        let Some(id) = get("CHEBI_ACCESSION").or_else(|| get("ID")) else {
            continue;
        };
        if get("STATUS").as_deref() == Some("C") {
            // C = checked, keep; other codes indicate deprecated/deleted.
        }
        let mut c = ParsedCompound::new(Source::Chebi, id.clone());
        if let Some(n) = get("NAME") {
            c.names.push(n);
        }
        bundle.compounds.push(c);
    }
    Ok(())
}

fn enrich_with_chemical_data(path: &Path, bundle: &mut IngestBundle) -> Result<()> {
    // Columns: ID, COMPOUND_ID, CHEMICAL_DATA, SOURCE, TYPE (FORMULA/MASS/CHARGE)
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| crate::IngestError::Other(format!("chebi chemical_data.tsv: {e}")))?;
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| crate::IngestError::Other(format!("read headers: {e}")))?
        .iter()
        .map(str::to_string)
        .collect();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for (i, c) in bundle.compounds.iter().enumerate() {
        index.insert(c.native_id.clone(), i);
    }

    for rec in rdr.records().flatten() {
        let get = |k: &str| -> Option<String> {
            headers
                .iter()
                .position(|h| h == k)
                .and_then(|ix| rec.get(ix).map(str::to_string))
                .filter(|s| !s.is_empty() && s != "null")
        };
        let Some(compound_id) = get("COMPOUND_ID") else {
            continue;
        };
        let Some(data) = get("CHEMICAL_DATA") else {
            continue;
        };
        let ty = get("TYPE").unwrap_or_default();
        let lookup_key = format!("CHEBI:{compound_id}");
        let Some(ix) = index.get(&lookup_key).copied() else {
            continue;
        };
        let target = &mut bundle.compounds[ix];
        match ty.as_str() {
            "FORMULA" => target.formula = Some(data),
            "MASS" | "MONOISOTOPIC MASS" => target.mass = data.parse().ok(),
            "CHARGE" => target.charge = data.parse().ok(),
            _ => {}
        }
    }
    Ok(())
}

fn enrich_with_names(path: &Path, bundle: &mut IngestBundle) -> Result<()> {
    // Columns: ID, COMPOUND_ID, TYPE, SOURCE, NAME, ADAPTED, LANGUAGE
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| crate::IngestError::Other(format!("chebi names.tsv: {e}")))?;
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| crate::IngestError::Other(format!("read headers: {e}")))?
        .iter()
        .map(str::to_string)
        .collect();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for (i, c) in bundle.compounds.iter().enumerate() {
        index.insert(c.native_id.clone(), i);
    }
    for rec in rdr.records().flatten() {
        let get = |k: &str| -> Option<String> {
            headers
                .iter()
                .position(|h| h == k)
                .and_then(|ix| rec.get(ix).map(str::to_string))
                .filter(|s| !s.is_empty() && s != "null")
        };
        let Some(compound_id) = get("COMPOUND_ID") else {
            continue;
        };
        let Some(name) = get("NAME") else { continue };
        let key = format!("CHEBI:{compound_id}");
        if let Some(ix) = index.get(&key).copied() {
            let target = &mut bundle.compounds[ix];
            if !target.names.iter().any(|n| n == &name) {
                target.names.push(name);
            }
        }
    }
    Ok(())
}

fn enrich_with_obo_roles(path: &Path, bundle: &mut IngestBundle) -> Result<()> {
    let reader = BufReader::new(std::fs::File::open(path)?);
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for (i, c) in bundle.compounds.iter().enumerate() {
        index.insert(c.native_id.clone(), i);
    }

    let mut current_id: Option<String> = None;
    let mut in_term = false;
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line == "[Term]" {
            in_term = true;
            current_id = None;
            continue;
        }
        if !in_term {
            continue;
        }
        if line.starts_with('[') {
            in_term = false;
            current_id = None;
            continue;
        }
        if let Some(rest) = line.strip_prefix("id: ") {
            current_id = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("relationship: has_role ")
            && let Some(ref cid) = current_id
        {
            let role = rest.split_whitespace().next().unwrap_or("").to_string();
            if !role.is_empty()
                && let Some(ix) = index.get(cid).copied()
            {
                let target = &mut bundle.compounds[ix];
                if !target.chebi_roles.iter().any(|r| r == &role) {
                    target.chebi_roles.push(role);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_compounds_and_enrichments() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("compounds.tsv"),
            "ID\tSTATUS\tCHEBI_ACCESSION\tNAME\n\
             1\tC\tCHEBI:15377\twater\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("chemical_data.tsv"),
            "ID\tCOMPOUND_ID\tCHEMICAL_DATA\tSOURCE\tTYPE\n\
             1\t15377\tH2O\tChEBI\tFORMULA\n\
             2\t15377\t0\tChEBI\tCHARGE\n\
             3\t15377\t18.015\tChEBI\tMASS\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("names.tsv"),
            "ID\tCOMPOUND_ID\tTYPE\tSOURCE\tNAME\tADAPTED\tLANGUAGE\n\
             1\t15377\tIUPAC NAME\tIUPAC\toxidane\t\ten\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("chebi.obo"),
            "[Term]\n\
             id: CHEBI:15377\n\
             name: water\n\
             relationship: has_role CHEBI:25212 ! metabolite\n",
        )
        .unwrap();
        let b = parse_dir(dir.path()).unwrap();
        let c = &b.compounds[0];
        assert_eq!(c.native_id, "CHEBI:15377");
        assert_eq!(c.formula.as_deref(), Some("H2O"));
        assert_eq!(c.charge, Some(0));
        assert!((c.mass.unwrap() - 18.015).abs() < 1e-6);
        assert!(c.names.iter().any(|n| n == "oxidane"));
        assert_eq!(c.chebi_roles, vec!["CHEBI:25212"]);
    }
}

//! MNXref `chem_xref.tsv`, `reac_xref.tsv`, `chem_prop.tsv`, `reac_prop.tsv`.
//!
//! MNXref rows look like:
//!     source:id<TAB>mnx_id<TAB>description
//!
//! `chem_prop.tsv`:
//!     mnx_id<TAB>name<TAB>formula<TAB>charge<TAB>mass<TAB>InChI<TAB>InChIKey<TAB>SMILES<TAB>...

use std::path::Path;

use gapsmith_db_core::source::Source;
use tracing::warn;

use super::ir::{CompoundXrefRow, IngestBundle, ParsedCompound, ReactionXrefRow};
use crate::Result;

pub fn parse_dir(dir: &Path) -> Result<IngestBundle> {
    let mut bundle = IngestBundle {
        source: Some(Source::Mnxref),
        ..Default::default()
    };
    if let Some(p) = existing(dir, "chem_xref.tsv") {
        parse_chem_xref(&p, &mut bundle)?;
    }
    if let Some(p) = existing(dir, "reac_xref.tsv") {
        parse_reac_xref(&p, &mut bundle)?;
    }
    if let Some(p) = existing(dir, "chem_prop.tsv") {
        parse_chem_prop(&p, &mut bundle)?;
    }
    Ok(bundle)
}

fn existing(dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    let p = dir.join(name);
    p.exists().then_some(p)
}

fn line_reader(path: &Path) -> Result<std::io::BufReader<std::fs::File>> {
    Ok(std::io::BufReader::new(std::fs::File::open(path)?))
}

fn classify_source(prefix: &str) -> Source {
    match prefix {
        "chebi" => Source::Chebi,
        "kegg.compound" | "kegg.reaction" | "kegg" => Source::Kegg,
        "rhea" | "rheaR" => Source::Rhea,
        "seed.compound" | "seed.reaction" | "seed" => Source::Modelseed,
        "uniprot" => Source::Uniprot,
        "reactome" => Source::Reactome,
        "pubchem.compound" | "pubchem" => Source::Pubchem,
        _ => Source::Other,
    }
}

fn parse_chem_xref(path: &Path, bundle: &mut IngestBundle) -> Result<()> {
    use std::io::BufRead;
    for (i, line) in line_reader(path)?.lines().enumerate() {
        let Ok(line) = line else { continue };
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 2 {
            warn!(line = i + 1, "mnxref chem_xref: short line");
            continue;
        }
        let xref = cols[0];
        let mnx = cols[1].to_string();
        let Some((prefix, id)) = xref.split_once(':') else {
            continue;
        };
        let src = classify_source(prefix);
        bundle.compound_xrefs.push(CompoundXrefRow {
            from_source: src,
            from_id: id.to_string(),
            to_source: Source::Mnxref,
            to_id: mnx,
        });
    }
    Ok(())
}

fn parse_reac_xref(path: &Path, bundle: &mut IngestBundle) -> Result<()> {
    use std::io::BufRead;
    for (i, line) in line_reader(path)?.lines().enumerate() {
        let Ok(line) = line else { continue };
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 2 {
            warn!(line = i + 1, "mnxref reac_xref: short line");
            continue;
        }
        let xref = cols[0];
        let mnx = cols[1].to_string();
        let Some((prefix, id)) = xref.split_once(':') else {
            continue;
        };
        let src = classify_source(prefix);
        bundle.reaction_xrefs.push(ReactionXrefRow {
            from_source: src,
            from_id: id.to_string(),
            to_source: Source::Mnxref,
            to_id: mnx,
        });
    }
    Ok(())
}

fn parse_chem_prop(path: &Path, bundle: &mut IngestBundle) -> Result<()> {
    use std::io::BufRead;
    for (i, line) in line_reader(path)?.lines().enumerate() {
        let Ok(line) = line else { continue };
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.is_empty() {
            warn!(line = i + 1, "mnxref chem_prop: empty row");
            continue;
        }
        let mnx = cols[0].to_string();
        let mut c = ParsedCompound::new(Source::Mnxref, mnx);
        if let Some(name) = cols.get(1).filter(|s| !s.is_empty()) {
            c.names.push((*name).to_string());
        }
        c.formula = cols
            .get(2)
            .filter(|s| !s.is_empty())
            .map(|s| (*s).to_string());
        c.charge = cols.get(3).and_then(|s| s.parse().ok());
        c.mass = cols.get(4).and_then(|s| s.parse().ok());
        c.inchi = cols
            .get(5)
            .filter(|s| !s.is_empty())
            .map(|s| (*s).to_string());
        c.inchikey = cols
            .get(6)
            .filter(|s| !s.is_empty())
            .map(|s| (*s).to_string());
        c.smiles = cols
            .get(7)
            .filter(|s| !s.is_empty())
            .map(|s| (*s).to_string());
        bundle.compounds.push(c);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_chem_xref_and_prop() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("chem_xref.tsv"),
            "#chem_xref\n\
             chebi:15377\tMNXM2\twater\n\
             kegg.compound:C00001\tMNXM2\twater\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("chem_prop.tsv"),
            "#chem_prop\n\
             MNXM2\twater\tH2O\t0\t18.015\tInChI=1S/H2O/h1H2\tXLYOFNOQVPJJNP-UHFFFAOYSA-N\tO\n",
        )
        .unwrap();
        let b = parse_dir(dir.path()).unwrap();
        assert_eq!(b.compounds.len(), 1);
        assert_eq!(b.compound_xrefs.len(), 2);
        assert_eq!(b.compound_xrefs[0].from_source, Source::Chebi);
        assert_eq!(b.compound_xrefs[0].from_id, "15377");
    }
}

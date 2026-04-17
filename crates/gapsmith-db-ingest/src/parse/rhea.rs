//! Rhea parser. Phase-2 scope: TSV tables only (`rhea2ec.tsv`,
//! `rhea2uniprot_sprot.tsv`, `rhea-directions.tsv`). Full RDF ingest is
//! deferred until Phase 3 / curation.

use std::path::Path;

use gapsmith_db_core::source::Source;
use tracing::warn;

use super::ir::{IngestBundle, ParsedReaction, ReactionXrefRow};
use crate::Result;

pub fn parse_dir(dir: &Path) -> Result<IngestBundle> {
    let mut bundle = IngestBundle {
        source: Some(Source::Rhea),
        ..Default::default()
    };
    if let Some(p) = existing(dir, "rhea2ec.tsv") {
        parse_rhea2ec(&p, &mut bundle)?;
    }
    if let Some(p) = existing(dir, "rhea-directions.tsv") {
        parse_rhea_directions(&p, &mut bundle)?;
    }
    Ok(bundle)
}

fn existing(dir: &Path, name: &str) -> Option<std::path::PathBuf> {
    let p = dir.join(name);
    p.exists().then_some(p)
}

/// Columns (since mid-2020): RHEA_ID  DIRECTION  MASTER_ID  ID  EC_NUMBER
fn parse_rhea2ec(path: &Path, bundle: &mut IngestBundle) -> Result<()> {
    use std::io::BufRead;
    let rdr = std::io::BufReader::new(std::fs::File::open(path)?);
    for (i, line) in rdr.lines().enumerate() {
        let Ok(line) = line else { continue };
        if i == 0 || line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 5 {
            warn!(line = i + 1, "rhea2ec: short line");
            continue;
        }
        let rhea_id = cols[0].to_string();
        let ec = cols[4].trim().to_string();
        let mut r = ParsedReaction::new(Source::Rhea, rhea_id.clone());
        r.rhea_id = Some(rhea_id);
        r.ec_numbers.push(ec);
        bundle.reactions.push(r);
    }
    Ok(())
}

/// rhea-directions.tsv: RHEA_ID_MASTER  RHEA_ID_LR  RHEA_ID_RL  RHEA_ID_BI
/// We emit xrefs linking the master to its directional variants.
fn parse_rhea_directions(path: &Path, bundle: &mut IngestBundle) -> Result<()> {
    use std::io::BufRead;
    let rdr = std::io::BufReader::new(std::fs::File::open(path)?);
    for (i, line) in rdr.lines().enumerate() {
        let Ok(line) = line else { continue };
        if i == 0 || line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 4 {
            continue;
        }
        let master = cols[0];
        for child in &cols[1..] {
            if !child.is_empty() && child != &master {
                bundle.reaction_xrefs.push(ReactionXrefRow {
                    from_source: Source::Rhea,
                    from_id: master.to_string(),
                    to_source: Source::Rhea,
                    to_id: (*child).to_string(),
                });
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
    fn parses_rhea2ec() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("rhea2ec.tsv"),
            "RHEA_ID\tDIRECTION\tMASTER_ID\tID\tEC_NUMBER\n\
             10001\tUN\t10000\t10000\t1.1.1.1\n",
        )
        .unwrap();
        let b = parse_dir(dir.path()).unwrap();
        assert_eq!(b.reactions.len(), 1);
        assert_eq!(b.reactions[0].rhea_id.as_deref(), Some("10001"));
        assert_eq!(b.reactions[0].ec_numbers, vec!["1.1.1.1"]);
    }
}

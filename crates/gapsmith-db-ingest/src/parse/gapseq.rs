//! gapseq `dat/` corrections. Only the SEED correction tables matter for
//! Phase-2 ingestion; transport tables and custom pathways feed Phase 5.

use std::path::Path;

use gapsmith_db_core::source::Source;
use tracing::warn;

use super::ir::{CompoundXrefRow, IngestBundle};
use crate::Result;

pub fn parse_dir(dir: &Path) -> Result<IngestBundle> {
    let mut bundle = IngestBundle {
        source: Some(Source::Gapseq),
        ..Default::default()
    };
    // Resolve to the dat/ directory even if the caller hands us the repo root.
    let candidates = [dir.to_path_buf(), dir.join("dat")];
    let dat_dir = candidates
        .iter()
        .find(|p| p.join("seed_reactions_corrected.tsv").exists())
        .or_else(|| {
            candidates
                .iter()
                .find(|p| p.join("seed_metabolites_edited.tsv").exists())
        });
    let Some(dat_dir) = dat_dir else {
        warn!(?dir, "gapseq: no dat/ directory found");
        return Ok(bundle);
    };

    if dat_dir.join("seed_reactions_corrected.tsv").exists() {
        parse_corrections_table(
            &dat_dir.join("seed_reactions_corrected.tsv"),
            Source::Modelseed,
            &mut bundle,
            "reaction",
        )?;
    }
    if dat_dir.join("seed_metabolites_edited.tsv").exists() {
        parse_corrections_table(
            &dat_dir.join("seed_metabolites_edited.tsv"),
            Source::Modelseed,
            &mut bundle,
            "metabolite",
        )?;
    }
    Ok(bundle)
}

fn parse_corrections_table(
    path: &Path,
    native_source: Source,
    bundle: &mut IngestBundle,
    kind: &str,
) -> Result<()> {
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| crate::IngestError::Other(format!("gapseq {kind}: {e}")))?;
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| crate::IngestError::Other(format!("gapseq {kind} headers: {e}")))?
        .iter()
        .map(str::to_string)
        .collect();
    let Some(id_ix) = headers
        .iter()
        .position(|h| h == "id" || h == "ID" || h == "seed_id")
    else {
        warn!(%kind, "gapseq: no id column");
        return Ok(());
    };
    for rec in rdr.records().flatten() {
        let Some(raw) = rec.get(id_ix) else { continue };
        let id = raw.trim().to_string();
        if id.is_empty() {
            continue;
        }
        bundle.compound_xrefs.push(CompoundXrefRow {
            from_source: Source::Gapseq,
            from_id: id.clone(),
            to_source: native_source,
            to_id: id,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_correction_tables() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("dat")).unwrap();
        std::fs::write(
            dir.path().join("dat/seed_metabolites_edited.tsv"),
            "id\tname\tformula\ncpd00002\tATP\tC10H12N5O13P3\n",
        )
        .unwrap();
        let b = parse_dir(dir.path()).unwrap();
        assert!(!b.compound_xrefs.is_empty());
        assert_eq!(b.compound_xrefs[0].from_source, Source::Gapseq);
    }
}

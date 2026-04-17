//! UniProtExistence — verify every UniProt xref on any entity resolves
//! to an accession in the local Swiss-Prot snapshot.
//!
//! The snapshot is the `swissprot_ec.json` file written by the UniProt
//! fetcher (Phase 1). We walk its JSON lazily, extracting `results[].primaryAccession`.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use gapsmith_db_core::{Database, Source};
use tracing::warn;

use crate::diagnostic::{Diagnostic, Target};
use crate::engine::Verifier;

pub const NAME: &str = "uniprot_existence";

pub struct UniProtExistence {
    pub snapshot: Option<PathBuf>,
    known: Option<HashSet<String>>,
}

impl UniProtExistence {
    #[must_use]
    pub fn new(snapshot: Option<PathBuf>) -> Self {
        Self {
            snapshot,
            known: None,
        }
    }

    fn ensure_loaded(&mut self) -> crate::Result<()> {
        if self.known.is_some() {
            return Ok(());
        }
        let Some(p) = self.snapshot.clone() else {
            self.known = Some(HashSet::new());
            return Ok(());
        };
        self.known = Some(load_accessions(&p)?);
        Ok(())
    }
}

impl Verifier for UniProtExistence {
    fn name(&self) -> &'static str {
        NAME
    }

    fn check(&mut self, db: &Database) -> crate::Result<Vec<Diagnostic>> {
        self.ensure_loaded()?;
        let known = self.known.as_ref().unwrap_or_else(|| unreachable!());
        let mut out = Vec::new();
        if known.is_empty() {
            out.push(Diagnostic::warn(
                NAME,
                Target::Database,
                "no_reference",
                "no Swiss-Prot snapshot configured; UniProt checks skipped",
            ));
            return Ok(out);
        }

        for c in db.compounds.values() {
            if let Some(ids) = c.xrefs.get(&Source::Uniprot) {
                for id in ids {
                    let target = Target::Compound(c.id.clone());
                    emit_lookup(&mut out, known, target, id);
                }
            }
        }
        for r in db.reactions.values() {
            if let Some(ids) = r.xrefs.get(&Source::Uniprot) {
                for id in ids {
                    let target = Target::Reaction(r.id.clone());
                    emit_lookup(&mut out, known, target, id);
                }
            }
        }
        Ok(out)
    }
}

fn emit_lookup(out: &mut Vec<Diagnostic>, known: &HashSet<String>, target: Target, id: &str) {
    if known.contains(id) {
        out.push(Diagnostic::info(NAME, target, "ok", format!("{id} exists")));
    } else {
        out.push(Diagnostic::error(
            NAME,
            target,
            "unknown_uniprot",
            format!("{id} not in Swiss-Prot snapshot"),
        ));
    }
}

fn load_accessions(path: &Path) -> crate::Result<HashSet<String>> {
    let mut f = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)?;
    let v: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "uniprot snapshot parse failed");
            return Ok(HashSet::new());
        }
    };
    let mut out = HashSet::new();
    if let Some(results) = v.get("results").and_then(|r| r.as_array()) {
        for entry in results {
            if let Some(acc) = entry.get("primaryAccession").and_then(|a| a.as_str()) {
                out.insert(acc.to_string());
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gapsmith_db_core::{Compound, CompoundId};
    use tempfile::tempdir;

    #[test]
    fn accession_match_reports_ok() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("swissprot_ec.json");
        std::fs::write(
            &path,
            r#"{"results":[{"primaryAccession":"P00001"},{"primaryAccession":"P00002"}]}"#,
        )
        .unwrap();
        let mut db = Database::new();
        let mut c = Compound::new(CompoundId::new("C1"));
        c.add_xref(Source::Uniprot, "P00001");
        c.add_xref(Source::Uniprot, "P99999"); // unknown
        db.insert_compound(c);

        let mut v = UniProtExistence::new(Some(path));
        let diags = v.check(&db).unwrap();
        assert!(diags.iter().any(|d| d.code.0 == "ok"));
        assert!(diags.iter().any(|d| d.code.0 == "unknown_uniprot"));
    }

    #[test]
    fn no_snapshot_yields_warning() {
        let mut v = UniProtExistence::new(None);
        let diags = v.check(&Database::new()).unwrap();
        assert_eq!(diags[0].code.0, "no_reference");
    }
}

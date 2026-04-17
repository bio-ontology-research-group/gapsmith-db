//! EcValidity — check that every EC number on a reaction exists in IntEnz.
//!
//! Accepts a Swiss-Prot-free, dependency-free `enzyme.dat` flat file as
//! its reference. Four-level-specified EC numbers are looked up exactly;
//! wildcard EC numbers (`1.2.-.-`) are emitted as `Info`.

use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use gapsmith_db_core::{Database, EcNumber};

use crate::diagnostic::{Diagnostic, Target};
use crate::engine::Verifier;

pub const NAME: &str = "ec_validity";

pub struct EcValidity {
    pub enzyme_dat: Option<PathBuf>,
    known: Option<HashSet<EcNumber>>,
}

impl EcValidity {
    #[must_use]
    pub fn new(enzyme_dat: Option<PathBuf>) -> Self {
        Self {
            enzyme_dat,
            known: None,
        }
    }

    fn ensure_loaded(&mut self) -> crate::Result<()> {
        if self.known.is_some() {
            return Ok(());
        }
        let Some(path) = self.enzyme_dat.clone() else {
            self.known = Some(HashSet::new());
            return Ok(());
        };
        self.known = Some(load_enzyme_dat(&path)?);
        Ok(())
    }
}

impl Verifier for EcValidity {
    fn name(&self) -> &'static str {
        NAME
    }

    fn check(&mut self, db: &Database) -> crate::Result<Vec<Diagnostic>> {
        self.ensure_loaded()?;
        let known = self.known.as_ref().unwrap_or_else(|| unreachable!());
        let mut out = Vec::new();
        let snapshot_empty = known.is_empty();

        if snapshot_empty {
            out.push(Diagnostic::warn(
                NAME,
                Target::Database,
                "no_reference",
                "no IntEnz snapshot configured; EC checks skipped",
            ));
        }

        for r in db.reactions.values() {
            for ec in &r.ec_numbers {
                if !ec.is_fully_specified() {
                    out.push(Diagnostic::info(
                        NAME,
                        Target::Reaction(r.id.clone()),
                        "wildcard",
                        format!("EC {ec} is not fully specified; skipped"),
                    ));
                    continue;
                }
                if snapshot_empty {
                    continue;
                }
                if known.contains(ec) {
                    out.push(Diagnostic::info(
                        NAME,
                        Target::Reaction(r.id.clone()),
                        "ok",
                        format!("EC {ec} exists"),
                    ));
                } else {
                    out.push(Diagnostic::error(
                        NAME,
                        Target::Reaction(r.id.clone()),
                        "unknown_ec",
                        format!("EC {ec} not in IntEnz snapshot"),
                    ));
                }
            }
        }
        Ok(out)
    }
}

fn load_enzyme_dat(path: &Path) -> crate::Result<HashSet<EcNumber>> {
    let f = std::fs::File::open(path)?;
    let r = BufReader::new(f);
    let mut out = HashSet::new();
    for line in r.lines() {
        let Ok(line) = line else { continue };
        // enzyme.dat records start with `ID   1.1.1.1`.
        let Some(rest) = line.strip_prefix("ID") else {
            continue;
        };
        let raw = rest.trim();
        if let Ok(ec) = raw.parse::<EcNumber>() {
            out.insert(ec);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gapsmith_db_core::{Reaction, ReactionId, Reversibility};
    use tempfile::tempdir;

    #[test]
    fn unknown_ec_is_error_when_snapshot_present() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("enzyme.dat");
        std::fs::write(&path, "ID   1.1.1.1\nDE   test\n//\n").unwrap();
        let mut db = Database::new();
        let mut r = Reaction::new(ReactionId::new("R1"), Reversibility::Reversible);
        r.ec_numbers.push("9.9.9.9".parse().unwrap());
        db.insert_reaction(r);
        let mut v = EcValidity::new(Some(path));
        let diags = v.check(&db).unwrap();
        assert!(diags.iter().any(|d| d.code.0 == "unknown_ec"));
    }

    #[test]
    fn known_ec_is_info_ok() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("enzyme.dat");
        std::fs::write(&path, "ID   1.1.1.1\n").unwrap();
        let mut db = Database::new();
        let mut r = Reaction::new(ReactionId::new("R1"), Reversibility::Reversible);
        r.ec_numbers.push("1.1.1.1".parse().unwrap());
        db.insert_reaction(r);
        let mut v = EcValidity::new(Some(path));
        let diags = v.check(&db).unwrap();
        assert!(diags.iter().any(|d| d.code.0 == "ok"));
    }

    #[test]
    fn wildcard_is_info_skipped() {
        let mut db = Database::new();
        let mut r = Reaction::new(ReactionId::new("R1"), Reversibility::Reversible);
        r.ec_numbers.push("1.2.-.-".parse().unwrap());
        db.insert_reaction(r);
        let mut v = EcValidity::new(None);
        let diags = v.check(&db).unwrap();
        assert!(diags.iter().any(|d| d.code.0 == "wildcard"));
    }
}

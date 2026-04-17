//! DlConsistencyCheck — stub per plan.md.
//!
//! Emits a minimal OWL signature composed of ChEBI roles (from compounds)
//! and any GO-BP references found in pathway evidence. Actual DL reasoning
//! (HermiT, ELK, or similar via a Python/Java helper) is a TODO; the stub
//! ships now so downstream pipeline integration can proceed.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;

use gapsmith_db_core::Database;
use serde_json::json;

use crate::diagnostic::{Diagnostic, Target};
use crate::engine::Verifier;

pub const NAME: &str = "dl_consistency";

pub struct DlConsistencyCheck {
    /// Optional output path; a minimal Turtle-like signature is written
    /// here for downstream reasoning (planned).
    pub signature_out: Option<PathBuf>,
}

impl DlConsistencyCheck {
    #[must_use]
    pub fn new(signature_out: Option<PathBuf>) -> Self {
        Self { signature_out }
    }
}

impl Verifier for DlConsistencyCheck {
    fn name(&self) -> &'static str {
        NAME
    }

    fn check(&mut self, db: &Database) -> crate::Result<Vec<Diagnostic>> {
        let mut roles: BTreeSet<String> = BTreeSet::new();
        for c in db.compounds.values() {
            for r in &c.chebi_roles {
                roles.insert(r.clone());
            }
        }

        let signature_size = roles.len();
        if let Some(path) = &self.signature_out {
            let mut f = std::fs::File::create(path)?;
            writeln!(f, "# gapsmith-db DL consistency signature (stub)")?;
            writeln!(f, "# TODO: wire to a proper OWL reasoner (ELK/HermiT).")?;
            writeln!(f, "@prefix chebi: <http://purl.obolibrary.org/obo/> .")?;
            for r in &roles {
                let s = r.replace("CHEBI:", "chebi:CHEBI_");
                writeln!(f, "{s} a owl:Class .")?;
            }
        }

        Ok(vec![Diagnostic::info(
            NAME,
            Target::Database,
            "stub",
            format!(
                "DL consistency not yet implemented; emitted signature with {signature_size} ChEBI roles"
            ),
        )
        .with_details(json!({
            "chebi_role_count": signature_size,
            "signature_out": self.signature_out.as_ref().map(|p| p.to_string_lossy().into_owned()),
            "todo": "wire OWL reasoner (ELK via Python bridge) + GO-BP integration",
        }))])
    }
}

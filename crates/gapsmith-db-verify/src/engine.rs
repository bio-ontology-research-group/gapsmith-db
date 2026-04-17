//! Verifier trait + batch engine.
//!
//! Fail-closed semantics: a verifier that errors out (e.g. Python env
//! missing) is recorded in `VerifierRun::run_error`; its `summary.error`
//! is bumped by one synthetic error diagnostic so the overall
//! `has_errors()` check picks it up.

use chrono::Utc;
use indexmap::IndexMap;
use tracing::{info, warn};

use gapsmith_db_core::Database;

use crate::diagnostic::{
    Diagnostic, Severity, Target, VerifierReport, VerifierRun, VerifierSummary,
};

/// A verifier consumes the whole database and returns diagnostics.
pub trait Verifier {
    fn name(&self) -> &'static str;
    fn check(&mut self, db: &Database) -> crate::Result<Vec<Diagnostic>>;
}

/// Run every verifier in `verifiers` sequentially, collecting diagnostics.
pub fn run_all(verifiers: &mut [Box<dyn Verifier>], db: &Database) -> VerifierReport {
    let names: Vec<&'static str> = verifiers.iter().map(|v| v.name()).collect();
    run_selected(verifiers, db, &names)
}

/// Run only the named verifiers. Unknown names are silently skipped with a
/// warning in the run's `run_error` slot.
pub fn run_selected(
    verifiers: &mut [Box<dyn Verifier>],
    db: &Database,
    only: &[&str],
) -> VerifierReport {
    let overall_start = Utc::now();
    let mut by_verifier: IndexMap<String, VerifierRun> = IndexMap::new();

    for v in verifiers.iter_mut() {
        if !only.contains(&v.name()) {
            continue;
        }
        let name = v.name().to_string();
        let started = Utc::now();
        info!(verifier = %name, "starting");
        let (diagnostics, run_error) = match v.check(db) {
            Ok(d) => (d, None),
            Err(e) => {
                warn!(verifier = %name, error = %e, "verifier failed to run");
                let synthetic = Diagnostic::error(
                    &name,
                    Target::Database,
                    "verifier_internal_error",
                    format!("{e}"),
                );
                (vec![synthetic], Some(e.to_string()))
            }
        };
        let finished = Utc::now();
        let summary = VerifierSummary::from_diagnostics(&diagnostics);
        by_verifier.insert(
            name,
            VerifierRun {
                summary,
                started_at: started,
                finished_at: finished,
                diagnostics,
                run_error,
            },
        );
    }

    let mut overall = VerifierSummary::default();
    for run in by_verifier.values() {
        overall.total += run.summary.total;
        overall.info += run.summary.info;
        overall.warning += run.summary.warning;
        overall.error += run.summary.error;
    }

    VerifierReport {
        started_at: overall_start,
        finished_at: Utc::now(),
        summary: overall,
        by_verifier,
    }
}

/// Convenience: filter a diagnostic list to just `Severity::Error`.
#[must_use]
pub fn errors_only(ds: &[Diagnostic]) -> Vec<&Diagnostic> {
    ds.iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

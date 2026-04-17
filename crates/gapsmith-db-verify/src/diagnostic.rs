//! Structured diagnostics emitted by every verifier.

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use gapsmith_db_core::{CompoundId, PathwayId, ReactionId};

/// Severity level for a diagnostic.
///
/// - `Info`: informational; does not block acceptance.
/// - `Warning`: merits curator attention; build proceeds.
/// - `Error`: fail-closed; blocks acceptance of the offending claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// What a diagnostic is talking about.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Compound(CompoundId),
    Reaction(ReactionId),
    Pathway(PathwayId),
    /// Whole-database scope (e.g. ATP cycle test).
    Database,
}

/// Short machine-readable diagnostic code. Grouped by verifier for
/// easy filtering.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiagnosticCode(pub String);

impl DiagnosticCode {
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Name of the verifier that emitted this.
    pub verifier: String,
    pub target: Target,
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
    /// Verifier-specific details (free-form JSON).
    #[serde(default)]
    pub details: serde_json::Value,
}

impl Diagnostic {
    #[must_use]
    pub fn info(
        verifier: impl Into<String>,
        target: Target,
        code: &str,
        message: impl Into<String>,
    ) -> Self {
        Self::new(verifier, target, Severity::Info, code, message)
    }

    #[must_use]
    pub fn warn(
        verifier: impl Into<String>,
        target: Target,
        code: &str,
        message: impl Into<String>,
    ) -> Self {
        Self::new(verifier, target, Severity::Warning, code, message)
    }

    #[must_use]
    pub fn error(
        verifier: impl Into<String>,
        target: Target,
        code: &str,
        message: impl Into<String>,
    ) -> Self {
        Self::new(verifier, target, Severity::Error, code, message)
    }

    #[must_use]
    pub fn new(
        verifier: impl Into<String>,
        target: Target,
        severity: Severity,
        code: &str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            verifier: verifier.into(),
            target,
            severity,
            code: DiagnosticCode::new(code),
            message: message.into(),
            details: serde_json::Value::Null,
        }
    }

    #[must_use]
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerifierSummary {
    pub total: usize,
    pub info: usize,
    pub warning: usize,
    pub error: usize,
}

impl VerifierSummary {
    #[must_use]
    pub fn from_diagnostics(d: &[Diagnostic]) -> Self {
        let mut s = Self::default();
        for diag in d {
            s.total += 1;
            match diag.severity {
                Severity::Info => s.info += 1,
                Severity::Warning => s.warning += 1,
                Severity::Error => s.error += 1,
            }
        }
        s
    }
}

/// A complete run record. One `VerifierReport` per `gapsmith-db verify`
/// invocation; each entry in `by_verifier` is one verifier's output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierReport {
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub summary: VerifierSummary,
    /// Per-verifier breakdown, keyed by verifier name. Using `IndexMap`
    /// keeps the JSON output deterministically ordered.
    pub by_verifier: IndexMap<String, VerifierRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierRun {
    pub summary: VerifierSummary,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub diagnostics: Vec<Diagnostic>,
    /// Non-fatal error message if the verifier itself failed to run end-to-end.
    #[serde(default)]
    pub run_error: Option<String>,
}

impl VerifierReport {
    /// Does the report contain any `Error`-severity diagnostic?
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.summary.error > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gapsmith_db_core::ReactionId;

    #[test]
    fn summary_counts() {
        let d = vec![
            Diagnostic::warn("t", Target::Database, "x", "hi"),
            Diagnostic::error("t", Target::Database, "x", "hi"),
            Diagnostic::error("t", Target::Database, "x", "hi"),
        ];
        let s = VerifierSummary::from_diagnostics(&d);
        assert_eq!(s.total, 3);
        assert_eq!(s.warning, 1);
        assert_eq!(s.error, 2);
    }

    #[test]
    fn diagnostic_json_shape() {
        let d = Diagnostic::info(
            "atom_balance",
            Target::Reaction(ReactionId::new("R1")),
            "ok",
            "fine",
        );
        let j = serde_json::to_value(&d).unwrap();
        assert_eq!(j["verifier"], "atom_balance");
        assert_eq!(j["severity"], "info");
        assert_eq!(j["code"], "ok");
    }
}

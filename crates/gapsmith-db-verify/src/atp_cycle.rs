//! AtpCycleTest. Per plan.md:
//!
//! > Build the universal model with cobrapy, close all exchanges, maximise
//! > ATP hydrolysis; assert ≤ epsilon. Regression: pin the value, fail CI
//! > on drift.
//!
//! The Rust side hands the universal-model path to the Python bridge and
//! receives `{ atp_flux: f64, epsilon: f64 }`. If the model file is
//! absent the verifier emits a single Warning.

use std::path::PathBuf;

use gapsmith_db_core::Database;
use serde::{Deserialize, Serialize};

use crate::diagnostic::{Diagnostic, Target};
use crate::engine::Verifier;
use crate::py_bridge::PyBridge;

pub const NAME: &str = "atp_cycle";

pub struct AtpCycleTest {
    pub python_project: PathBuf,
    pub universal_model: Option<PathBuf>,
    /// Tolerance; plan.md says "≤ epsilon". Default 1e-6 mmol/gDW/h.
    pub epsilon: f64,
}

impl AtpCycleTest {
    #[must_use]
    pub fn new(python_project: PathBuf, universal_model: Option<PathBuf>) -> Self {
        Self {
            python_project,
            universal_model,
            epsilon: 1e-6,
        }
    }
}

#[derive(Debug, Serialize)]
struct AtpReq {
    model_path: String,
    epsilon: f64,
}

#[derive(Debug, Deserialize)]
struct AtpResp {
    atp_flux: f64,
    epsilon: f64,
    passed: bool,
}

impl Verifier for AtpCycleTest {
    fn name(&self) -> &'static str {
        NAME
    }

    fn check(&mut self, _db: &Database) -> crate::Result<Vec<Diagnostic>> {
        let Some(model) = self.universal_model.clone() else {
            return Ok(vec![Diagnostic::warn(
                NAME,
                Target::Database,
                "no_model",
                "no universal model configured; ATP cycle test skipped",
            )]);
        };
        if !model.exists() {
            return Ok(vec![Diagnostic::warn(
                NAME,
                Target::Database,
                "model_missing",
                format!("universal model {} not found", model.display()),
            )]);
        }

        let bridge = PyBridge::new(self.python_project.clone());
        if !bridge.ping() {
            return Ok(vec![Diagnostic::warn(
                NAME,
                Target::Database,
                "bridge_unavailable",
                "Python cobra bridge not reachable",
            )]);
        }
        let req = AtpReq {
            model_path: model.to_string_lossy().into_owned(),
            epsilon: self.epsilon,
        };
        let resp: AtpResp = bridge.call("atp_cycle", &req)?;
        let details = serde_json::json!({
            "atp_flux": resp.atp_flux,
            "epsilon": resp.epsilon,
        });
        let diag = if resp.passed {
            Diagnostic::info(NAME, Target::Database, "passed", "ATP cycle ≤ ε")
                .with_details(details)
        } else {
            Diagnostic::error(
                NAME,
                Target::Database,
                "free_atp_production",
                format!(
                    "ATP flux {:.3e} exceeds epsilon {:.3e}",
                    resp.atp_flux, resp.epsilon
                ),
            )
            .with_details(details)
        };
        Ok(vec![diag])
    }
}

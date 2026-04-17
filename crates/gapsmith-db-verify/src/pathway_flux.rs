//! PathwayFluxTest — given a pathway and a medium definition, assert FBA
//! on the universal model yields a positive flux through the pathway's
//! terminal reaction.
//!
//! The universal-model path and the medium definition are configuration
//! inputs. When either is missing, the verifier emits a single Warning
//! rather than failing.

use std::path::PathBuf;

use gapsmith_db_core::Database;
use serde::{Deserialize, Serialize};

use crate::diagnostic::{Diagnostic, Target};
use crate::engine::Verifier;
use crate::py_bridge::PyBridge;

pub const NAME: &str = "pathway_flux";

pub struct PathwayFluxTest {
    pub python_project: PathBuf,
    pub universal_model: Option<PathBuf>,
    pub medium: Option<PathBuf>,
    pub min_flux: f64,
}

impl PathwayFluxTest {
    #[must_use]
    pub fn new(python_project: PathBuf) -> Self {
        Self {
            python_project,
            universal_model: None,
            medium: None,
            min_flux: 1e-4,
        }
    }
}

#[derive(Debug, Serialize)]
struct FluxReq<'a> {
    model_path: String,
    medium_path: String,
    pathway_id: &'a str,
    reactions: Vec<&'a str>,
    min_flux: f64,
}

#[derive(Debug, Deserialize)]
struct FluxResp {
    pathway_id: String,
    objective_flux: f64,
    passed: bool,
    note: Option<String>,
}

impl Verifier for PathwayFluxTest {
    fn name(&self) -> &'static str {
        NAME
    }

    fn check(&mut self, db: &Database) -> crate::Result<Vec<Diagnostic>> {
        let (Some(model), Some(medium)) = (self.universal_model.clone(), self.medium.clone())
        else {
            return Ok(vec![Diagnostic::warn(
                NAME,
                Target::Database,
                "config_missing",
                "universal model and/or medium not configured; test skipped",
            )]);
        };
        if !model.exists() || !medium.exists() {
            return Ok(vec![Diagnostic::warn(
                NAME,
                Target::Database,
                "config_missing",
                "model or medium file not found on disk",
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

        let mut out = Vec::new();
        for p in db.pathways.values() {
            let reactions: Vec<&str> = p.reactions.iter().map(AsRef::as_ref).collect();
            if reactions.is_empty() {
                continue;
            }
            let req = FluxReq {
                model_path: model.to_string_lossy().into_owned(),
                medium_path: medium.to_string_lossy().into_owned(),
                pathway_id: p.id.as_str(),
                reactions,
                min_flux: self.min_flux,
            };
            let resp: FluxResp = match bridge.call("pathway_flux", &req) {
                Ok(r) => r,
                Err(e) => {
                    out.push(Diagnostic::warn(
                        NAME,
                        Target::Pathway(p.id.clone()),
                        "bridge_error",
                        format!("{e}"),
                    ));
                    continue;
                }
            };
            let details = serde_json::json!({
                "objective_flux": resp.objective_flux,
                "note": resp.note,
            });
            if resp.passed {
                out.push(
                    Diagnostic::info(
                        NAME,
                        Target::Pathway(gapsmith_db_core::PathwayId::new(&resp.pathway_id)),
                        "feasible",
                        format!("pathway flux {:.3e}", resp.objective_flux),
                    )
                    .with_details(details),
                );
            } else {
                out.push(
                    Diagnostic::error(
                        NAME,
                        Target::Pathway(gapsmith_db_core::PathwayId::new(&resp.pathway_id)),
                        "infeasible",
                        "no flux through pathway on given medium",
                    )
                    .with_details(details),
                );
            }
        }
        Ok(out)
    }
}

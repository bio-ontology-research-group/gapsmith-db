//! ThermodynamicFeasibility — call eQuilibrator via the Python bridge
//! to compute ΔG for each reaction. If the bridge is unavailable, emit
//! a single `Warning` diagnostic and continue (fail-closed means the
//! claim is not accepted; it does not mean CI breaks).

use std::path::PathBuf;

use gapsmith_db_core::{Database, Reaction};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::diagnostic::{Diagnostic, Target};
use crate::engine::Verifier;
use crate::py_bridge::PyBridge;

pub const NAME: &str = "thermo";

pub struct ThermodynamicFeasibility {
    pub python_project: PathBuf,
    /// If true, reactions with no InChI on any compound are skipped with Info.
    pub skip_without_inchi: bool,
}

impl ThermodynamicFeasibility {
    #[must_use]
    pub fn new(python_project: PathBuf) -> Self {
        Self {
            python_project,
            skip_without_inchi: true,
        }
    }
}

#[derive(Debug, Serialize)]
struct ThermoRequest<'a> {
    reactions: Vec<ThermoRxn<'a>>,
}

#[derive(Debug, Serialize)]
struct ThermoRxn<'a> {
    id: &'a str,
    substrates: Vec<ThermoSpec<'a>>,
    products: Vec<ThermoSpec<'a>>,
}

#[derive(Debug, Serialize)]
struct ThermoSpec<'a> {
    compound_id: &'a str,
    coefficient: f64,
    inchi: &'a str,
}

#[derive(Debug, Deserialize)]
struct ThermoResponse {
    results: Vec<ThermoResult>,
}

#[derive(Debug, Deserialize)]
struct ThermoResult {
    id: String,
    delta_g: Option<(f64, f64)>,
    skipped_reason: Option<String>,
}

impl Verifier for ThermodynamicFeasibility {
    fn name(&self) -> &'static str {
        NAME
    }

    fn check(&mut self, db: &Database) -> crate::Result<Vec<Diagnostic>> {
        let bridge = PyBridge::new(self.python_project.clone());
        if !bridge.ping() {
            return Ok(vec![Diagnostic::warn(
                NAME,
                Target::Database,
                "bridge_unavailable",
                "Python thermo bridge not reachable; all reactions skipped",
            )]);
        }

        // Build request payload from reactions where every compound has an InChI.
        let mut payload = Vec::<ThermoRxn>::new();
        let mut skipped_ids = Vec::<String>::new();
        for r in db.reactions.values() {
            if let Some(rxn) = build_thermo_rxn(db, r, self.skip_without_inchi) {
                payload.push(rxn);
            } else {
                skipped_ids.push(r.id.to_string());
            }
        }

        let mut out: Vec<Diagnostic> = skipped_ids
            .into_iter()
            .map(|rid| {
                Diagnostic::info(
                    NAME,
                    Target::Reaction(gapsmith_db_core::ReactionId::new(rid)),
                    "skipped",
                    "one or more compounds lack InChI",
                )
            })
            .collect();

        if payload.is_empty() {
            return Ok(out);
        }

        let req = ThermoRequest { reactions: payload };
        let resp: ThermoResponse = match bridge.call("thermo", &req) {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "thermo bridge call failed");
                out.push(Diagnostic::warn(
                    NAME,
                    Target::Database,
                    "bridge_error",
                    format!("{e}"),
                ));
                return Ok(out);
            }
        };

        for res in resp.results {
            let target = Target::Reaction(gapsmith_db_core::ReactionId::new(&res.id));
            if let Some(reason) = res.skipped_reason {
                out.push(Diagnostic::info(NAME, target, "skipped", reason));
            } else if let Some((dg, unc)) = res.delta_g {
                out.push(
                    Diagnostic::info(
                        NAME,
                        target,
                        "computed",
                        format!("ΔG = {dg:.2} ± {unc:.2} kJ/mol"),
                    )
                    .with_details(serde_json::json!({ "delta_g": dg, "uncertainty": unc })),
                );
            } else {
                out.push(Diagnostic::warn(
                    NAME,
                    target,
                    "no_estimate",
                    "eQuilibrator returned no estimate",
                ));
            }
        }
        Ok(out)
    }
}

fn build_thermo_rxn<'a>(
    db: &'a Database,
    r: &'a Reaction,
    strict_inchi: bool,
) -> Option<ThermoRxn<'a>> {
    let mut subs = Vec::new();
    let mut prods = Vec::new();
    for s in &r.stoichiometry {
        let c = db.compounds.get(&s.compound)?;
        let inchi = c.inchi.as_deref().unwrap_or("");
        if strict_inchi && inchi.is_empty() {
            return None;
        }
        let spec = ThermoSpec {
            compound_id: c.id.as_str(),
            coefficient: s.coefficient.abs(),
            inchi,
        };
        if s.coefficient < 0.0 {
            subs.push(spec);
        } else {
            prods.push(spec);
        }
    }
    Some(ThermoRxn {
        id: r.id.as_str(),
        substrates: subs,
        products: prods,
    })
}

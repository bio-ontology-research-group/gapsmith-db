//! Build a universal SBML model from a [`Database`] via the Python bridge.
//!
//! Per plan.md: the ATP-cycle verifier requires a universal cobra model.
//! Rather than re-implement SBML writing in Rust, we hand the DB to
//! cobrapy through [`PyBridge`] and let cobra emit the SBML (Level 3 v1
//! + fbc v2).
//!
//! The builder also synthesises an `ATPM` reaction on request: free
//! ATP hydrolysis is the canonical regression handle for mass/energy-
//! balance errors in the universal model. The default ATP / ADP / Pi /
//! H2O / H+ metabolite IDs follow the BiGG-compatible naming that
//! cobrapy's FBA machinery prefers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gapsmith_db_core::{Compound, Database, Reversibility, Source};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::Result;
use crate::py_bridge::PyBridge;

/// The five canonical metabolite IDs an ATPM reaction needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtpmIds {
    pub atp: String,
    pub adp: String,
    pub pi: String,
    pub h2o: String,
    pub h: String,
}

impl Default for AtpmIds {
    /// BiGG-convention defaults. Adjust when the ingest uses another
    /// canonical metabolite naming scheme.
    fn default() -> Self {
        Self {
            atp: "atp_c".into(),
            adp: "adp_c".into(),
            pi: "pi_c".into(),
            h2o: "h2o_c".into(),
            h: "h_c".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildOptions {
    pub add_atpm: bool,
    #[serde(default)]
    pub atpm_ids: Option<AtpmIds>,
    #[serde(default)]
    pub atpm_lb: Option<f64>,
    #[serde(default)]
    pub atpm_ub: Option<f64>,
}

#[derive(Debug, Serialize)]
struct CompoundPayload {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    formula: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    charge: Option<i16>,
    compartment: String,
}

#[derive(Debug, Serialize)]
struct ReactionPayload {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    lb: f64,
    ub: f64,
    metabolites: BTreeMap<String, f64>,
}

#[derive(Debug, Serialize)]
struct BuildRequest<'a> {
    compounds: Vec<CompoundPayload>,
    reactions: Vec<ReactionPayload>,
    out_path: String,
    add_atpm: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    atpm_ids: Option<&'a AtpmIds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    atpm_lb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    atpm_ub: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct BuildOutcome {
    pub model_path: PathBuf,
    pub num_reactions: usize,
    pub num_metabolites: usize,
    pub atpm_added: bool,
    #[serde(default)]
    pub note: Option<String>,
}

fn bounds_for(rev: Reversibility) -> (f64, f64) {
    match rev {
        Reversibility::Forward => (0.0, 1000.0),
        Reversibility::Reverse => (-1000.0, 0.0),
        Reversibility::Reversible => (-1000.0, 1000.0),
    }
}

fn metabolite_id(base: &str, compartment: &str) -> String {
    // COBRA convention: `<metabolite>_<compartment>`. Our Database stores
    // the compartment as metadata on the stoichiometry entry rather than
    // folded into the ID; the builder folds it back in here.
    format!("{base}_{compartment}")
}

fn compound_formula(c: &Compound) -> Option<&str> {
    c.formula.as_deref().filter(|s| !s.is_empty())
}

fn compound_display_name(c: &Compound) -> Option<&str> {
    c.names
        .first()
        .map(String::as_str)
        .or_else(|| c.xrefs.get(&Source::Chebi)?.first().map(String::as_str))
}

/// Assemble the per-compartment metabolite table referenced by the
/// reactions. A compound appears once per compartment it's used in.
fn emit_compounds(db: &Database) -> Vec<CompoundPayload> {
    use std::collections::BTreeSet;
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut out = Vec::new();
    for r in db.reactions.values() {
        for s in &r.stoichiometry {
            let comp = s.compartment.short_code().to_string();
            let key = (s.compound.as_str().to_string(), comp.clone());
            if !seen.insert(key) {
                continue;
            }
            let Some(c) = db.compounds.get(&s.compound) else {
                continue;
            };
            out.push(CompoundPayload {
                id: metabolite_id(s.compound.as_str(), &comp),
                name: compound_display_name(c).map(str::to_string),
                formula: compound_formula(c).map(str::to_string),
                charge: c.charge,
                compartment: comp,
            });
        }
    }
    out
}

fn emit_reactions(db: &Database) -> Vec<ReactionPayload> {
    let mut out = Vec::with_capacity(db.reactions.len());
    for r in db.reactions.values() {
        let (lb, ub) = bounds_for(r.reversibility);
        let mut metabolites: BTreeMap<String, f64> = BTreeMap::new();
        for s in &r.stoichiometry {
            let comp = s.compartment.short_code();
            let mid = metabolite_id(s.compound.as_str(), comp);
            *metabolites.entry(mid).or_insert(0.0) += s.coefficient;
        }
        metabolites.retain(|_, v| v.abs() > f64::EPSILON);
        out.push(ReactionPayload {
            id: r.id.as_str().to_string(),
            name: r.names.first().cloned(),
            lb,
            ub,
            metabolites,
        });
    }
    out
}

/// Build the universal SBML at `out_path` by calling the Python bridge.
/// The caller is expected to have validated the DB already.
pub fn build_universal(
    bridge: &PyBridge,
    db: &Database,
    out_path: &Path,
    options: &BuildOptions,
) -> Result<BuildOutcome> {
    let compounds = emit_compounds(db);
    let reactions = emit_reactions(db);
    info!(
        num_compounds = compounds.len(),
        num_reactions = reactions.len(),
        "building universal SBML via python bridge"
    );
    let req = BuildRequest {
        compounds,
        reactions,
        out_path: out_path.to_string_lossy().into_owned(),
        add_atpm: options.add_atpm,
        atpm_ids: options.atpm_ids.as_ref(),
        atpm_lb: options.atpm_lb,
        atpm_ub: options.atpm_ub,
    };
    if tracing::enabled!(tracing::Level::DEBUG) {
        tracing::debug!(
            "build_universal request preview: add_atpm={}, atpm_ids={:?}",
            req.add_atpm,
            req.atpm_ids
        );
    }
    let resp: BuildOutcome = bridge.call("build_universal", &req)?;
    Ok(resp)
}

/// Baseline ATP-cycle flux pin. Committed under `verify/baselines/`; CI
/// re-runs the cycle test against a freshly-built universal and asserts
/// equality within `epsilon`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtpBaseline {
    pub atp_flux: f64,
    pub epsilon: f64,
    pub model_sha256: String,
    /// Release / commit / date at which this baseline was recorded.
    pub pinned_at: String,
    pub note: Option<String>,
}

impl AtpBaseline {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gapsmith_db_core::{Compartment, CompoundId, Reaction, ReactionId, StoichiometryEntry};

    fn tiny_db() -> Database {
        let mut db = Database::new();
        let mut a = Compound::new(CompoundId::new("glucose"));
        a.formula = Some("C6H12O6".into());
        a.names.push("D-glucose".into());
        db.insert_compound(a);
        let mut b = Compound::new(CompoundId::new("atp"));
        b.formula = Some("C10H16N5O13P3".into());
        b.charge = Some(-4);
        db.insert_compound(b);

        let mut r = Reaction::new(ReactionId::new("r1"), Reversibility::Reversible);
        r.stoichiometry.push(StoichiometryEntry::substrate(
            CompoundId::new("glucose"),
            1.0,
            Compartment::Cytosol,
        ));
        r.stoichiometry.push(StoichiometryEntry::product(
            CompoundId::new("atp"),
            1.0,
            Compartment::Cytosol,
        ));
        db.insert_reaction(r);
        db
    }

    #[test]
    fn reactions_reference_known_compounds() {
        let db = tiny_db();
        let payload_reactions = emit_reactions(&db);
        assert_eq!(payload_reactions.len(), 1);
        let r = &payload_reactions[0];
        assert_eq!(r.id.as_str(), "r1");
        assert!((r.lb + 1000.0).abs() < 1e-9);
        assert!((r.ub - 1000.0).abs() < 1e-9);
        assert!((r.metabolites.get("glucose_c").copied().unwrap_or(0.0) + 1.0).abs() < 1e-9);
        assert!((r.metabolites.get("atp_c").copied().unwrap_or(0.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn compounds_are_compartmentalised() {
        let db = tiny_db();
        let cs = emit_compounds(&db);
        assert!(cs.iter().any(|c| c.id == "glucose_c"));
        assert!(cs.iter().any(|c| c.id == "atp_c" && c.charge == Some(-4)));
    }

    #[test]
    fn bounds_respect_reversibility() {
        let (lb, ub) = bounds_for(Reversibility::Forward);
        assert!(lb.abs() < 1e-9 && (ub - 1000.0).abs() < 1e-9);
        let (lb, ub) = bounds_for(Reversibility::Reverse);
        assert!((lb + 1000.0).abs() < 1e-9 && ub.abs() < 1e-9);
        let (lb, ub) = bounds_for(Reversibility::Reversible);
        assert!((lb + 1000.0).abs() < 1e-9 && (ub - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn atp_baseline_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("baseline.json");
        let b = AtpBaseline {
            atp_flux: 0.0,
            epsilon: 1e-6,
            model_sha256: "sha256:deadbeef".into(),
            pinned_at: "2026-04-17".into(),
            note: Some("initial".into()),
        };
        b.save(&p).unwrap();
        let back = AtpBaseline::load(&p).unwrap();
        assert!((back.epsilon - 1e-6).abs() < 1e-12);
        assert_eq!(back.pinned_at, "2026-04-17");
    }
}

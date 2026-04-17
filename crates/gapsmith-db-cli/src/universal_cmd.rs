//! `gapsmith-db universal` — build a universal SBML model from the
//! ingested DB and optionally pin the ATP-cycle regression value.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use gapsmith_db_core::serde_io;
use gapsmith_db_verify::py_bridge::PyBridge;
use gapsmith_db_verify::universal_model::{AtpBaseline, AtpmIds, BuildOptions, build_universal};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::info;

#[derive(Debug, Args)]
pub struct UniversalArgs {
    #[command(subcommand)]
    pub action: UniversalAction,
}

#[derive(Debug, Subcommand)]
pub enum UniversalAction {
    /// Build an SBML universal model from the ingested DB.
    Build(BuildArgs),
    /// Measure the ATP-cycle flux on an SBML model and write a baseline pin.
    PinAtpCycle(PinArgs),
    /// Compare a live ATP-cycle measurement against the committed baseline.
    CheckAtpCycle(CheckArgs),
}

#[derive(Debug, Args)]
pub struct BuildArgs {
    #[arg(long)]
    pub db: PathBuf,
    #[arg(long)]
    pub out: PathBuf,
    /// Synthesise an `ATPM` reaction so the AtpCycleTest has a handle.
    #[arg(long)]
    pub add_atpm: bool,
    /// BiGG-style cytosolic IDs (atp_c, adp_c, pi_c, h2o_c, h_c) when
    /// --add-atpm is set; override if the DB uses a different convention.
    #[arg(long, value_parser = parse_atpm_ids)]
    pub atpm_ids: Option<AtpmIds>,
    #[arg(long, default_value = "python")]
    pub python_project: PathBuf,
}

#[derive(Debug, Args)]
pub struct PinArgs {
    /// SBML model to measure. Typically the output of `universal build`.
    #[arg(long)]
    pub model: PathBuf,
    /// Pin file to write.
    #[arg(long, default_value = "verify/baselines/atp_cycle.json")]
    pub out: PathBuf,
    /// Tolerance to record as `epsilon`.
    #[arg(long, default_value_t = 1e-6)]
    pub epsilon: f64,
    /// Free-text label for the pin (commit, release, date).
    #[arg(long)]
    pub pinned_at: String,
    #[arg(long, default_value = "python")]
    pub python_project: PathBuf,
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    #[arg(long)]
    pub model: PathBuf,
    #[arg(long, default_value = "verify/baselines/atp_cycle.json")]
    pub baseline: PathBuf,
    #[arg(long, default_value = "python")]
    pub python_project: PathBuf,
}

fn parse_atpm_ids(s: &str) -> std::result::Result<AtpmIds, String> {
    // Accept `atp=<id>,adp=<id>,pi=<id>,h2o=<id>,h=<id>`. All five keys
    // must be present — partial overrides against the BiGG default lead
    // to silent mismatches against DBs that use other conventions.
    let mut atp = None;
    let mut adp = None;
    let mut pi = None;
    let mut h2o = None;
    let mut h = None;
    for kv in s.split(',') {
        let (k, v) = kv
            .split_once('=')
            .ok_or_else(|| format!("expected key=value in {kv}"))?;
        let v = v.trim().to_string();
        match k.trim() {
            "atp" => atp = Some(v),
            "adp" => adp = Some(v),
            "pi" => pi = Some(v),
            "h2o" => h2o = Some(v),
            "h" => h = Some(v),
            other => return Err(format!("unknown ATPM key {other}")),
        }
    }
    match (atp, adp, pi, h2o, h) {
        (Some(atp), Some(adp), Some(pi), Some(h2o), Some(h)) => Ok(AtpmIds {
            atp,
            adp,
            pi,
            h2o,
            h,
        }),
        _ => Err("--atpm-ids requires all five keys (atp,adp,pi,h2o,h)".into()),
    }
}

pub fn run(args: UniversalArgs) -> Result<()> {
    match args.action {
        UniversalAction::Build(a) => run_build(a),
        UniversalAction::PinAtpCycle(a) => run_pin(a),
        UniversalAction::CheckAtpCycle(a) => run_check(a),
    }
}

fn run_build(a: BuildArgs) -> Result<()> {
    let db = serde_io::read_binary(&a.db)
        .with_context(|| format!("reading DB at {}", a.db.display()))?;
    let bridge = PyBridge::new(a.python_project);
    let opts = BuildOptions {
        add_atpm: a.add_atpm,
        atpm_ids: a.atpm_ids.or(a.add_atpm.then(AtpmIds::default)),
        ..BuildOptions::default()
    };
    let outcome = build_universal(&bridge, &db, &a.out, &opts)
        .context("build_universal bridge call failed")?;
    info!(
        out = %outcome.model_path.display(),
        reactions = outcome.num_reactions,
        metabolites = outcome.num_metabolites,
        atpm_added = outcome.atpm_added,
        "universal model written"
    );
    if let Some(note) = outcome.note {
        info!(note, "bridge note");
    }
    Ok(())
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
    #[allow(dead_code)]
    #[serde(default)]
    passed: bool,
    #[serde(default)]
    note: Option<String>,
}

fn call_atp_cycle(bridge: &PyBridge, model: &Path, epsilon: f64) -> Result<AtpResp> {
    let req = AtpReq {
        model_path: model.to_string_lossy().into_owned(),
        epsilon,
    };
    let resp: AtpResp = bridge
        .call("atp_cycle", &req)
        .map_err(|e| anyhow::anyhow!("atp_cycle bridge call: {e}"))?;
    Ok(resp)
}

fn sha256_of(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("hashing {}", path.display()))?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok(format!("sha256:{}", hex::encode(h.finalize())))
}

fn run_pin(a: PinArgs) -> Result<()> {
    let bridge = PyBridge::new(a.python_project);
    let resp = call_atp_cycle(&bridge, &a.model, a.epsilon)?;
    let hash = sha256_of(&a.model)?;
    let baseline = AtpBaseline {
        atp_flux: resp.atp_flux,
        epsilon: resp.epsilon,
        model_sha256: hash,
        pinned_at: a.pinned_at,
        note: resp.note,
    };
    baseline
        .save(&a.out)
        .map_err(|e| anyhow::anyhow!("write baseline: {e}"))?;
    info!(
        path = %a.out.display(),
        flux = baseline.atp_flux,
        epsilon = baseline.epsilon,
        "atp_cycle baseline recorded"
    );
    Ok(())
}

fn run_check(a: CheckArgs) -> Result<()> {
    let baseline =
        AtpBaseline::load(&a.baseline).map_err(|e| anyhow::anyhow!("load baseline: {e}"))?;
    let bridge = PyBridge::new(a.python_project);
    let resp = call_atp_cycle(&bridge, &a.model, baseline.epsilon)?;
    let observed_hash = sha256_of(&a.model)?;
    let drift = (resp.atp_flux - baseline.atp_flux).abs();
    if drift > baseline.epsilon {
        anyhow::bail!(
            "atp_cycle drift: observed {:.3e} vs baseline {:.3e} (tolerance {:.3e})",
            resp.atp_flux,
            baseline.atp_flux,
            baseline.epsilon
        );
    }
    if observed_hash != baseline.model_sha256 {
        info!(
            baseline = %baseline.model_sha256,
            observed = %observed_hash,
            "atp_cycle: model bytes differ from pin, but flux matches within ε"
        );
    }
    info!(
        flux = resp.atp_flux,
        drift = drift,
        "atp_cycle: within baseline tolerance"
    );
    Ok(())
}

//! Symbolic verifier layer — the heart of the system.
//!
//! LLM proposals are untrusted; these verifiers are the judges. Each
//! verifier runs standalone and as part of a batch, emitting a
//! [`VerifierReport`] that serialises to JSON. Semantics are fail-closed:
//! when a verifier cannot decide (missing data, Python env unavailable),
//! it emits a `Warning` or `Info` diagnostic rather than silently passing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod atom_balance;
pub mod atp_cycle;
pub mod charge_balance;
pub mod diagnostic;
pub mod dl_consistency;
pub mod ec_validity;
pub mod engine;
pub mod formula;
pub mod pathway_flux;
pub mod pmid_existence;
pub mod py_bridge;
pub mod thermo;
pub mod uniprot_existence;
pub mod universal_model;

pub use atom_balance::AtomBalance;
pub use atp_cycle::AtpCycleTest;
pub use charge_balance::ChargeBalance;
pub use diagnostic::{
    Diagnostic, DiagnosticCode, Severity, Target, VerifierReport, VerifierSummary,
};
pub use dl_consistency::DlConsistencyCheck;
pub use ec_validity::EcValidity;
pub use engine::{Verifier, run_all, run_selected};
pub use pathway_flux::PathwayFluxTest;
pub use pmid_existence::PmidExistence;
pub use thermo::ThermodynamicFeasibility;
pub use uniprot_existence::UniProtExistence;
pub use universal_model::{AtpBaseline, AtpmIds, BuildOptions, BuildOutcome, build_universal};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VerifyError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("python bridge error: {0}")]
    PyBridge(String),
    #[error("missing required data: {0}")]
    MissingData(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, VerifyError>;

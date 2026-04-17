//! Route a pending proposal to `for_curation/` or `rejected/` based on
//! the verifier report.
//!
//! Plan.md: "Proposals flow into the verifier automatically; failures →
//! `proposals/rejected/` with reasons; passes → `proposals/for_curation/`."
//!
//! The "DB-level" verifier pass that would surface issues specific to
//! the proposal's merged state needs the ingestion pipeline. For Phase
//! 4 the router uses whatever verifier you already have and lets the
//! caller decide the severity threshold.

use std::path::{Path, PathBuf};

use gapsmith_db_verify::{Severity, VerifierReport};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::schema::Proposal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalDisposition {
    ForCuration,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SidecarReport {
    proposal_id: String,
    disposition: ProposalDisposition,
    report: VerifierReport,
}

/// Move `pending/<id>.json` to the matching destination and write a
/// sidecar `<id>.report.json` next to it.
pub fn route_proposal(
    proposals_dir: &Path,
    proposal: &Proposal,
    report: &VerifierReport,
    threshold: Severity,
) -> crate::Result<(ProposalDisposition, PathBuf)> {
    let disposition = decide(report, threshold);
    let id = proposal
        .proposal_id
        .strip_prefix("sha256:")
        .unwrap_or(&proposal.proposal_id);
    let src = proposals_dir.join("pending").join(format!("{id}.json"));

    let subdir = match disposition {
        ProposalDisposition::ForCuration => "for_curation",
        ProposalDisposition::Rejected => "rejected",
    };
    let dst_dir = proposals_dir.join(subdir);
    std::fs::create_dir_all(&dst_dir)?;

    let dst = dst_dir.join(format!("{id}.json"));
    if src.exists() {
        std::fs::rename(&src, &dst)?;
    } else {
        // `pending/` may not hold the file yet — emit it fresh.
        std::fs::write(&dst, serde_json::to_string_pretty(proposal)?)?;
    }

    let sidecar = SidecarReport {
        proposal_id: proposal.proposal_id.clone(),
        disposition,
        report: report.clone(),
    };
    let sidecar_path = dst_dir.join(format!("{id}.report.json"));
    std::fs::write(&sidecar_path, serde_json::to_string_pretty(&sidecar)?)?;

    info!(
        id = %proposal.proposal_id,
        disposition = ?disposition,
        destination = %dst.display(),
        "proposal routed"
    );

    Ok((disposition, dst))
}

fn decide(report: &VerifierReport, threshold: Severity) -> ProposalDisposition {
    let too_severe = |s: Severity| -> bool {
        match threshold {
            Severity::Error => matches!(s, Severity::Error),
            Severity::Warning => matches!(s, Severity::Error | Severity::Warning),
            Severity::Info => true,
        }
    };
    for run in report.by_verifier.values() {
        for d in &run.diagnostics {
            if too_severe(d.severity) {
                return ProposalDisposition::Rejected;
            }
        }
    }
    ProposalDisposition::ForCuration
}

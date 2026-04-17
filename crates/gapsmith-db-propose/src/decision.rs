//! Curator decision log with a hash chain.
//!
//! Decisions are appended to `proposals/decisions.log.jsonl`, one JSON
//! object per line. Every entry carries `previous_decision_hash` pointing
//! at the prior decision's `decision_id`; the genesis pointer is
//! [`GENESIS_HASH`]. `decision_id` is itself the sha256 of the canonical
//! JSON of the decision body (fields except `decision_id`), so tampering
//! with any record breaks the chain at the next entry.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ProposeError, Result};

/// The pointer a genesis decision stores in `previous_decision_hash`.
pub const GENESIS_HASH: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionAction {
    Accept,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    /// sha256 of the body below. Set by [`Decision::finalised`].
    pub decision_id: String,
    pub previous_decision_hash: String,
    pub proposal_id: String,
    pub action: DecisionAction,
    pub curator: String,
    #[serde(default)]
    pub comment: Option<String>,
    pub timestamp: DateTime<Utc>,
    /// Optional: the sha256 of the VerifierReport bundled with the proposal
    /// at the time of the decision. Gives the curator's verdict a precise
    /// anchor even if the verifier is later rerun.
    #[serde(default)]
    pub verifier_report_hash: Option<String>,
}

impl Decision {
    /// Construct with `decision_id` left empty; call [`Self::finalised`]
    /// to fill it in.
    #[must_use]
    pub fn new(
        previous: &str,
        proposal_id: impl Into<String>,
        action: DecisionAction,
        curator: impl Into<String>,
        comment: Option<String>,
        verifier_report_hash: Option<String>,
    ) -> Self {
        Self {
            decision_id: String::new(),
            previous_decision_hash: previous.to_string(),
            proposal_id: proposal_id.into(),
            action,
            curator: curator.into(),
            comment,
            timestamp: Utc::now(),
            verifier_report_hash,
        }
    }

    /// Fill in `decision_id` with the canonical content hash.
    #[must_use]
    pub fn finalised(mut self) -> Self {
        self.decision_id = decision_hash(&self);
        self
    }
}

fn decision_hash(d: &Decision) -> String {
    let body = Decision {
        decision_id: String::new(),
        ..d.clone()
    };
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    let mut h = Sha256::new();
    h.update(&bytes);
    format!("sha256:{}", hex::encode(h.finalize()))
}

/// Append-only log at `proposals_dir/decisions.log.jsonl`.
#[derive(Debug, Clone)]
pub struct DecisionLog {
    pub path: PathBuf,
}

impl DecisionLog {
    #[must_use]
    pub fn at(proposals_dir: &Path) -> Self {
        Self {
            path: proposals_dir.join("decisions.log.jsonl"),
        }
    }

    /// The head of the chain: the `decision_id` of the last entry, or
    /// [`GENESIS_HASH`] if the log is empty / missing.
    pub fn head(&self) -> Result<String> {
        if !self.path.exists() {
            return Ok(GENESIS_HASH.to_string());
        }
        let mut last: Option<String> = None;
        for entry in self.read_all()? {
            last = Some(entry.decision_id);
        }
        Ok(last.unwrap_or_else(|| GENESIS_HASH.to_string()))
    }

    /// Read every decision in order.
    pub fn read_all(&self) -> Result<Vec<Decision>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let f = std::fs::File::open(&self.path)?;
        let r = BufReader::new(f);
        let mut out = Vec::new();
        for (i, line) in r.lines().enumerate() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let d: Decision = serde_json::from_str(&line).map_err(|e| {
                ProposeError::Other(format!("{}: line {}: {e}", self.path.display(), i + 1))
            })?;
            out.push(d);
        }
        Ok(out)
    }

    /// Append a finalised decision. The caller is expected to have set
    /// `previous_decision_hash` correctly; use [`Self::head`] to fetch it.
    pub fn append(&self, decision: &Decision) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if decision.decision_id.is_empty() {
            return Err(ProposeError::Other(
                "decision_id not set; call finalised()".into(),
            ));
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let line = serde_json::to_string(decision)?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        Ok(())
    }

    /// Walk the log, verifying every entry's hash and back-pointer.
    /// Returns the number of decisions checked.
    pub fn verify_chain(&self) -> Result<ChainVerification> {
        let mut prev = GENESIS_HASH.to_string();
        let mut count = 0_usize;
        let mut issues: Vec<ChainIssue> = Vec::new();
        for (i, d) in self.read_all()?.into_iter().enumerate() {
            count += 1;
            let computed = decision_hash(&d);
            if d.decision_id != computed {
                issues.push(ChainIssue::HashMismatch {
                    index: i,
                    expected: d.decision_id.clone(),
                    computed,
                });
            }
            if d.previous_decision_hash != prev {
                issues.push(ChainIssue::BrokenLink {
                    index: i,
                    expected_prev: prev.clone(),
                    actual_prev: d.previous_decision_hash.clone(),
                });
            }
            prev.clone_from(&d.decision_id);
        }
        Ok(ChainVerification {
            count,
            head: prev,
            issues,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ChainVerification {
    pub count: usize,
    pub head: String,
    pub issues: Vec<ChainIssue>,
}

impl ChainVerification {
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.issues.is_empty()
    }
}

#[derive(Debug, Clone)]
pub enum ChainIssue {
    HashMismatch {
        index: usize,
        expected: String,
        computed: String,
    },
    BrokenLink {
        index: usize,
        expected_prev: String,
        actual_prev: String,
    },
}

impl std::fmt::Display for ChainIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChainIssue::HashMismatch {
                index,
                expected,
                computed,
            } => write!(
                f,
                "entry {index}: decision_id {expected} does not match computed {computed}"
            ),
            ChainIssue::BrokenLink {
                index,
                expected_prev,
                actual_prev,
            } => write!(
                f,
                "entry {index}: previous_decision_hash {actual_prev} (expected {expected_prev})"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn empty_log_head_is_genesis() {
        let dir = tempdir().unwrap();
        let log = DecisionLog::at(dir.path());
        assert_eq!(log.head().unwrap(), GENESIS_HASH);
    }

    #[test]
    fn append_and_walk_chain() {
        let dir = tempdir().unwrap();
        let log = DecisionLog::at(dir.path());

        let d1 = Decision::new(
            &log.head().unwrap(),
            "sha256:aaaa",
            DecisionAction::Accept,
            "rh",
            Some("looks right".into()),
            None,
        )
        .finalised();
        log.append(&d1).unwrap();

        let d2 = Decision::new(
            &log.head().unwrap(),
            "sha256:bbbb",
            DecisionAction::Reject,
            "rh",
            None,
            None,
        )
        .finalised();
        log.append(&d2).unwrap();

        let v = log.verify_chain().unwrap();
        assert_eq!(v.count, 2);
        assert_eq!(v.head, d2.decision_id);
        assert!(v.is_ok(), "chain should be valid: {:?}", v.issues);
    }

    #[test]
    fn tampering_breaks_chain() {
        let dir = tempdir().unwrap();
        let log = DecisionLog::at(dir.path());
        let d1 = Decision::new(
            &log.head().unwrap(),
            "sha256:aaaa",
            DecisionAction::Accept,
            "rh",
            None,
            None,
        )
        .finalised();
        log.append(&d1).unwrap();

        // Tamper: rewrite log with a modified comment but keep decision_id.
        let mut tampered = d1.clone();
        tampered.comment = Some("inserted evidence".into());
        let line = serde_json::to_string(&tampered).unwrap();
        std::fs::write(&log.path, format!("{line}\n")).unwrap();

        let v = log.verify_chain().unwrap();
        assert!(
            !v.is_ok(),
            "tampering should break chain, got {:?}",
            v.issues
        );
    }
}

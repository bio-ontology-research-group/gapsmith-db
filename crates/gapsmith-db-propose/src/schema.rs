//! Strict JSON schema for a pathway proposal.
//!
//! Per plan.md: reactions (by ChEBI+EC or Rhea ID), enzymes (by UniProt),
//! DAG structure, citations (PMIDs). Proposals are content-addressed:
//! `proposal_id` is derived from the canonical JSON form of everything
//! except `proposal_id` itself and transient timestamps.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use gapsmith_db_core::{EcNumber, Pmid};

pub const SCHEMA_VERSION: &str = "1";

fn epoch() -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_else(Utc::now)
}

/// Deserialize a `DateTime<Utc>` that tolerates empty strings and null as
/// "unset" (→ epoch). Callers overwrite the field afterwards.
fn lenient_datetime<'de, D>(d: D) -> std::result::Result<DateTime<Utc>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Option<String> = Option::deserialize(d)?;
    match raw.as_deref() {
        None | Some("") => Ok(epoch()),
        Some(s) => DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(serde::de::Error::custom),
    }
}

/// The target of a proposal: which pathway we are trying to recover,
/// for which organism scope, and under which medium assumptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalTarget {
    pub pathway_name: String,
    #[serde(default)]
    pub organism_scope: Option<String>,
    #[serde(default)]
    pub medium: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Identification of a reaction in the proposal. Exactly one of the
/// variants should be present.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactionRef {
    /// Canonical: a Rhea ID.
    Rhea(String),
    /// Fall-back: EC + ChEBI IDs for substrates and products.
    ChebiEc {
        ec: EcNumber,
        substrates: Vec<String>,
        products: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalReaction {
    /// Proposer-local ID used in the DAG (e.g. "R1", "R2").
    pub local_id: String,
    pub reference: ReactionRef,
    #[serde(default)]
    pub reversibility: Option<gapsmith_db_core::Reversibility>,
    /// Free-text equation for human review. Not authoritative.
    #[serde(default)]
    pub equation_hint: Option<String>,
}

/// An enzyme that catalyses one of the proposed reactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnzymeRef {
    /// Swiss-Prot accession. Preferred.
    pub uniprot: String,
    /// Which local reactions this enzyme catalyses.
    pub catalyses: Vec<String>,
    #[serde(default)]
    pub function: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalCitation {
    pub pmid: Pmid,
    #[serde(default)]
    pub note: Option<String>,
}

/// A proposal for a pathway. `proposal_id` is assigned by [`Proposal::hashed`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub schema_version: String,
    /// Content hash ("sha256:..."). Always derived; any value on load is
    /// re-verified.
    #[serde(default)]
    pub proposal_id: String,
    /// `deserialize_with` tolerates empty strings / absent fields (LLMs
    /// often emit `"created_at": ""` as a placeholder; the proposer
    /// overwrites immediately after parse).
    #[serde(default = "epoch", deserialize_with = "lenient_datetime")]
    pub created_at: DateTime<Utc>,
    /// Model identifier (OpenRouter slug or the fixture name).
    pub model: String,
    /// Prompt template version used to generate this proposal.
    pub prompt_version: String,
    pub target: ProposalTarget,
    pub reactions: Vec<ProposalReaction>,
    #[serde(default)]
    pub enzymes: Vec<EnzymeRef>,
    #[serde(default)]
    pub dag: Vec<ProposalEdge>,
    #[serde(default)]
    pub citations: Vec<ProposalCitation>,
    /// Free-form rationale emitted by the model; not used by verifiers.
    #[serde(default)]
    pub rationale: Option<String>,
}

impl Proposal {
    /// Return a copy with `proposal_id` set to the canonical content hash.
    /// Idempotent: the hash excludes `proposal_id` and `created_at`.
    #[must_use]
    pub fn hashed(mut self) -> Self {
        self.proposal_id = canonical_hash(&self);
        self
    }

    /// Validate invariants beyond what serde enforces. Returns a list of
    /// violations (empty on success).
    pub fn validate(&self) -> crate::Result<()> {
        use std::collections::BTreeSet;
        if self.schema_version != SCHEMA_VERSION {
            return Err(crate::ProposeError::Schema(format!(
                "schema_version {} not supported (expected {SCHEMA_VERSION})",
                self.schema_version
            )));
        }
        let mut ids: BTreeSet<&str> = BTreeSet::new();
        for r in &self.reactions {
            if !ids.insert(&r.local_id) {
                return Err(crate::ProposeError::Schema(format!(
                    "duplicate reaction local_id {}",
                    r.local_id
                )));
            }
            match &r.reference {
                ReactionRef::Rhea(id) if id.trim().is_empty() => {
                    return Err(crate::ProposeError::Schema(format!(
                        "reaction {} has empty Rhea ID",
                        r.local_id
                    )));
                }
                ReactionRef::ChebiEc {
                    substrates,
                    products,
                    ..
                } if substrates.is_empty() || products.is_empty() => {
                    return Err(crate::ProposeError::Schema(format!(
                        "reaction {} ChEBI+EC needs substrates and products",
                        r.local_id
                    )));
                }
                _ => {}
            }
        }
        for e in &self.dag {
            if !ids.contains(e.from.as_str()) {
                return Err(crate::ProposeError::Schema(format!(
                    "dag edge references unknown reaction {}",
                    e.from
                )));
            }
            if !ids.contains(e.to.as_str()) {
                return Err(crate::ProposeError::Schema(format!(
                    "dag edge references unknown reaction {}",
                    e.to
                )));
            }
            if e.from == e.to {
                return Err(crate::ProposeError::Schema(format!(
                    "dag edge {} -> {} is a self-loop",
                    e.from, e.to
                )));
            }
        }
        for enz in &self.enzymes {
            if enz.uniprot.trim().is_empty() {
                return Err(crate::ProposeError::Schema("enzyme missing UniProt".into()));
            }
            for rid in &enz.catalyses {
                if !ids.contains(rid.as_str()) {
                    return Err(crate::ProposeError::Schema(format!(
                        "enzyme {} references unknown reaction {}",
                        enz.uniprot, rid
                    )));
                }
            }
        }
        if !verify_id_matches(self) {
            return Err(crate::ProposeError::Schema(format!(
                "proposal_id {} does not match content hash",
                self.proposal_id
            )));
        }
        Ok(())
    }

    /// Canonical content-hash (sha256 hex, `sha256:` prefix) of everything
    /// except `proposal_id` and `created_at`.
    #[must_use]
    pub fn content_hash(&self) -> String {
        canonical_hash(self)
    }
}

fn canonical_hash(p: &Proposal) -> String {
    let clone = Proposal {
        proposal_id: String::new(),
        created_at: DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_else(Utc::now),
        ..p.clone()
    };
    let mut h = Sha256::new();
    // serde_json without pretty-print is deterministic for a given struct.
    let bytes = serde_json::to_vec(&clone).unwrap_or_default();
    h.update(&bytes);
    format!("sha256:{}", hex::encode(h.finalize()))
}

fn verify_id_matches(p: &Proposal) -> bool {
    if p.proposal_id.is_empty() {
        return true; // not yet hashed; callers should call `.hashed()`.
    }
    p.proposal_id == canonical_hash(p)
}

/// JSON Schema for a `Proposal`, hand-rolled. We keep this inline rather
/// than depending on `schemars` — the schema is stable and small enough
/// to benefit from being reviewable in PRs.
#[must_use]
pub fn json_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://gapsmith-db/schema/proposal.json",
        "title": "Pathway proposal",
        "type": "object",
        "required": ["schema_version", "proposal_id", "created_at", "model",
                     "prompt_version", "target", "reactions"],
        "properties": {
            "schema_version": {"const": SCHEMA_VERSION},
            "proposal_id": {"type": "string", "pattern": "^sha256:[0-9a-f]{64}$"},
            "created_at": {"type": "string", "format": "date-time"},
            "model": {"type": "string"},
            "prompt_version": {"type": "string"},
            "target": {
                "type": "object",
                "required": ["pathway_name"],
                "properties": {
                    "pathway_name": {"type": "string"},
                    "organism_scope": {"type": ["string", "null"]},
                    "medium": {"type": ["string", "null"]},
                    "notes": {"type": ["string", "null"]}
                }
            },
            "reactions": {
                "type": "array",
                "items": {"$ref": "#/$defs/reaction"}
            },
            "enzymes": {"type": "array", "items": {"$ref": "#/$defs/enzyme"}},
            "dag": {"type": "array", "items": {"$ref": "#/$defs/edge"}},
            "citations": {"type": "array", "items": {"$ref": "#/$defs/citation"}},
            "rationale": {"type": ["string", "null"]}
        },
        "$defs": {
            "reaction": {
                "type": "object",
                "required": ["local_id", "reference"],
                "properties": {
                    "local_id": {"type": "string"},
                    "reference": {
                        "oneOf": [
                            {"type": "object", "required": ["rhea"],
                             "properties": {"rhea": {"type": "string"}}},
                            {"type": "object", "required": ["chebi_ec"],
                             "properties": {
                                 "chebi_ec": {
                                     "type": "object",
                                     "required": ["ec", "substrates", "products"],
                                     "properties": {
                                         "ec": {"type": "string"},
                                         "substrates": {"type": "array", "items": {"type": "string"}},
                                         "products": {"type": "array", "items": {"type": "string"}}
                                     }
                                 }
                             }}
                        ]
                    },
                    "reversibility": {"enum": ["forward", "reverse", "reversible", null]},
                    "equation_hint": {"type": ["string", "null"]}
                }
            },
            "enzyme": {
                "type": "object",
                "required": ["uniprot", "catalyses"],
                "properties": {
                    "uniprot": {"type": "string"},
                    "catalyses": {"type": "array", "items": {"type": "string"}},
                    "function": {"type": ["string", "null"]}
                }
            },
            "edge": {
                "type": "object",
                "required": ["from", "to"],
                "properties": {"from": {"type": "string"}, "to": {"type": "string"}}
            },
            "citation": {
                "type": "object",
                "required": ["pmid"],
                "properties": {
                    "pmid": {"type": "string"},
                    "note": {"type": ["string", "null"]}
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Proposal {
        Proposal {
            schema_version: SCHEMA_VERSION.into(),
            proposal_id: String::new(),
            created_at: Utc::now(),
            model: "mock/fixture".into(),
            prompt_version: "test".into(),
            target: ProposalTarget {
                pathway_name: "methanogenesis from CO2".into(),
                organism_scope: Some("Methanothermobacter".into()),
                medium: None,
                notes: None,
            },
            reactions: vec![
                ProposalReaction {
                    local_id: "R1".into(),
                    reference: ReactionRef::Rhea("10020".into()),
                    reversibility: Some(gapsmith_db_core::Reversibility::Forward),
                    equation_hint: None,
                },
                ProposalReaction {
                    local_id: "R2".into(),
                    reference: ReactionRef::ChebiEc {
                        ec: "1.12.98.1".parse().unwrap(),
                        substrates: vec!["CHEBI:15378".into()],
                        products: vec!["CHEBI:16183".into()],
                    },
                    reversibility: None,
                    equation_hint: None,
                },
            ],
            enzymes: vec![],
            dag: vec![ProposalEdge {
                from: "R1".into(),
                to: "R2".into(),
            }],
            citations: vec![ProposalCitation {
                pmid: Pmid::new("9461540"),
                note: None,
            }],
            rationale: None,
        }
    }

    #[test]
    fn hashed_roundtrip_validates() {
        let p = sample().hashed();
        p.validate().unwrap();
        let j = serde_json::to_string(&p).unwrap();
        let back: Proposal = serde_json::from_str(&j).unwrap();
        back.validate().unwrap();
    }

    #[test]
    fn dangling_dag_edge_fails() {
        let mut p = sample().hashed();
        p.dag.push(ProposalEdge {
            from: "R1".into(),
            to: "R_MISSING".into(),
        });
        let p = p.hashed(); // re-hash so content hash matches
        assert!(p.validate().is_err());
    }

    #[test]
    fn duplicate_local_id_fails() {
        let mut p = sample();
        p.reactions[1].local_id = "R1".into();
        let p = p.hashed();
        assert!(p.validate().is_err());
    }

    #[test]
    fn json_schema_has_required_fields() {
        let s = json_schema();
        let req = s["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "proposal_id"));
    }
}

//! Evidence carried alongside each compound / reaction / pathway.
//!
//! Every claim in the database has at least one `Evidence` entry so a
//! curator can trace its provenance: which source asserted it, which
//! PMID supports it, which LLM proposal the claim originated from,
//! and which deterministic verifier signed off on it.

use serde::{Deserialize, Serialize};

use crate::pmid::Pmid;
use crate::source::Source;

/// Bounded 0.0–1.0 confidence. Serde-transparent so it round-trips as a
/// bare float and rejects out-of-range values on load.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Confidence(f32);

impl Confidence {
    pub const CERTAIN: Self = Self(1.0);
    pub const UNKNOWN: Self = Self(0.5);
    pub const DOUBTFUL: Self = Self(0.25);

    /// Clamp to the valid range. Prefer this over `try_new` when the upstream
    /// is already numerical (e.g. a model likelihood).
    #[must_use]
    pub fn clamp(v: f32) -> Self {
        Self(v.clamp(0.0, 1.0))
    }

    pub fn try_new(v: f32) -> Result<Self, String> {
        if !v.is_finite() || !(0.0..=1.0).contains(&v) {
            return Err(format!("confidence {v} outside [0,1]"));
        }
        Ok(Self(v))
    }

    #[must_use]
    pub fn value(self) -> f32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Confidence {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = f32::deserialize(d)?;
        Confidence::try_new(v).map_err(serde::de::Error::custom)
    }
}

/// Flags that mark non-obvious provenance decisions. `NameMatched` is a
/// dedup last-resort flag (plan.md: "name match (last resort, flagged)").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeFlag {
    /// Two compounds merged because their InChIKeys matched.
    InchikeyMatched,
    /// Merged via MNXref cross-reference table.
    MnxrefMatched,
    /// Last-resort name match — curator review recommended.
    NameMatched,
    /// Charge or formula disagreement between merged sources; kept best guess.
    FormulaConflict,
    /// Present to flag that an explicit proton (`H+`) ambiguity was tolerated.
    ExplicitProtonAmbiguity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// Upstream source asserting the claim.
    pub source: Source,
    /// Supporting literature, if any.
    #[serde(default)]
    pub citation: Option<Pmid>,
    /// Human curator who last touched this claim (null if automated).
    #[serde(default)]
    pub curator: Option<String>,
    /// Hash of the LLM proposal JSON if this claim came from the proposer.
    #[serde(default)]
    pub proposal_hash: Option<String>,
    /// Path or inline log of the verifier run that signed off.
    #[serde(default)]
    pub verifier_log: Option<String>,
    /// Confidence in [0,1].
    pub confidence: Confidence,
    /// Optional flags for non-obvious provenance events.
    #[serde(default)]
    pub flags: Vec<MergeFlag>,
}

impl Evidence {
    #[must_use]
    pub fn from_source(source: Source, confidence: Confidence) -> Self {
        Self {
            source,
            citation: None,
            curator: None,
            proposal_hash: None,
            verifier_log: None,
            confidence,
            flags: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_flag(mut self, flag: MergeFlag) -> Self {
        self.flags.push(flag);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_rejects_out_of_range() {
        assert!(Confidence::try_new(-0.1).is_err());
        assert!(Confidence::try_new(1.1).is_err());
        assert!(Confidence::try_new(f32::NAN).is_err());
        assert!(Confidence::try_new(0.5).is_ok());
    }

    #[test]
    fn confidence_serde() {
        let c = Confidence::CERTAIN;
        let s = serde_json::to_string(&c).unwrap();
        assert_eq!(s, "1.0");
        let back: Confidence = serde_json::from_str(&s).unwrap();
        assert_eq!(back, c);
        assert!(serde_json::from_str::<Confidence>("2.0").is_err());
    }

    #[test]
    fn evidence_roundtrip() {
        let e = Evidence::from_source(Source::Rhea, Confidence::clamp(0.9))
            .with_flag(MergeFlag::InchikeyMatched);
        let j = serde_json::to_string(&e).unwrap();
        let back: Evidence = serde_json::from_str(&j).unwrap();
        assert_eq!(back.flags, vec![MergeFlag::InchikeyMatched]);
    }
}

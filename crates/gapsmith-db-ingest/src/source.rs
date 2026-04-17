//! `SOURCE.toml` parsing. One of these lives in every `data/<name>/` directory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{IngestError, Result};

/// Canonical identifier for a data source. Matches the `data/<id>/` dir name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceId {
    Modelseed,
    Mnxref,
    Rhea,
    Chebi,
    Intenz,
    Uniprot,
    Reactome,
    Gapseq,
    Kegg,
}

impl SourceId {
    pub const ALL: &'static [SourceId] = &[
        SourceId::Modelseed,
        SourceId::Mnxref,
        SourceId::Rhea,
        SourceId::Chebi,
        SourceId::Intenz,
        SourceId::Uniprot,
        SourceId::Reactome,
        SourceId::Gapseq,
        SourceId::Kegg,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SourceId::Modelseed => "modelseed",
            SourceId::Mnxref => "mnxref",
            SourceId::Rhea => "rhea",
            SourceId::Chebi => "chebi",
            SourceId::Intenz => "intenz",
            SourceId::Uniprot => "uniprot",
            SourceId::Reactome => "reactome",
            SourceId::Gapseq => "gapseq",
            SourceId::Kegg => "kegg",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        for id in Self::ALL {
            if id.as_str() == s {
                return Ok(*id);
            }
        }
        Err(IngestError::UnknownSource(s.to_string()))
    }
}

impl std::fmt::Display for SourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Typed view of `SOURCE.toml`. Fields mirror the TOML layout.
#[derive(Debug, Clone, Deserialize)]
pub struct SourceSpec {
    pub name: String,
    pub upstream_url: String,
    pub licence: String,
    #[serde(default)]
    pub licence_url: Option<String>,
    pub attribution: String,

    #[serde(default)]
    pub pinned_commit: Option<String>,
    #[serde(default)]
    pub pinned_release: Option<String>,
    #[serde(default)]
    pub pinned_date: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,

    #[serde(default)]
    pub artefacts: Vec<String>,

    /// Per-file SHA256 pins, keyed by the artefact filename as it appears on
    /// disk under `data/<source>/`. Multi-file sources should prefer this
    /// over the top-level `sha256` so that upstream drift is diagnosable at
    /// file-level granularity.
    #[serde(default)]
    pub file_hashes: BTreeMap<String, String>,

    #[serde(default)]
    pub notes: Option<String>,

    // Source-specific optional fields. Kept permissive with serde(default).
    #[serde(default)]
    pub ftp_url: Option<String>,
    #[serde(default)]
    pub rest_url: Option<String>,
    #[serde(default)]
    pub download_url: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub gpl_obligation: Option<bool>,
}

impl SourceSpec {
    /// Load `data/<id>/SOURCE.toml`.
    pub fn load(data_root: &Path, id: SourceId) -> Result<Self> {
        let path = data_root.join(id.as_str()).join("SOURCE.toml");
        let text = std::fs::read_to_string(&path)?;
        toml::from_str(&text).map_err(|e| IngestError::Toml { path, source: e })
    }

    /// `data/<id>/` — where fetched artefacts land.
    #[must_use]
    pub fn source_dir(data_root: &Path, id: SourceId) -> PathBuf {
        data_root.join(id.as_str())
    }

    /// The "pin" is whichever of commit / release / date is set. Empty strings
    /// are treated as unset so that a freshly-initialised `SOURCE.toml` reads
    /// as unpinned.
    #[must_use]
    pub fn pin(&self) -> Option<Pin> {
        fn nonempty(o: Option<&String>) -> Option<String> {
            o.filter(|s| !s.is_empty()).cloned()
        }
        if let Some(c) = nonempty(self.pinned_commit.as_ref()) {
            Some(Pin::Commit(c))
        } else if let Some(r) = nonempty(self.pinned_release.as_ref()) {
            Some(Pin::Release(r))
        } else {
            nonempty(self.pinned_date.as_ref()).map(Pin::Date)
        }
    }

    /// Hash pinned in `SOURCE.toml`, if any. Empty strings are treated as unset.
    #[must_use]
    pub fn pinned_hash(&self) -> Option<&str> {
        self.sha256.as_deref().filter(|s| !s.is_empty())
    }

    /// Per-file SHA256 lookup. Empty strings are treated as unset.
    #[must_use]
    pub fn file_hash(&self, name: &str) -> Option<&str> {
        self.file_hashes
            .get(name)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }

    /// Require a `pinned_commit`. In dry-run mode, a placeholder is returned
    /// so the plan template is still printable without a real pin.
    pub fn require_commit(&self, dry_run: bool) -> Result<String> {
        match self.pin() {
            Some(Pin::Commit(c)) => Ok(c),
            Some(other) => Err(IngestError::Other(format!(
                "{}: expected pinned_commit, got pinned_{}",
                self.name,
                other.kind()
            ))),
            None if dry_run => Ok("<PIN_TBD>".into()),
            None => Err(IngestError::UnpinnedSource(self.name.clone())),
        }
    }

    /// Require a `pinned_release`. See [`require_commit`] for dry-run behaviour.
    pub fn require_release(&self, dry_run: bool) -> Result<String> {
        match self.pin() {
            Some(Pin::Release(r)) => Ok(r),
            Some(other) => Err(IngestError::Other(format!(
                "{}: expected pinned_release, got pinned_{}",
                self.name,
                other.kind()
            ))),
            None if dry_run => Ok("<PIN_TBD>".into()),
            None => Err(IngestError::UnpinnedSource(self.name.clone())),
        }
    }

    /// Accept either `pinned_release` or `pinned_date`. See [`require_commit`].
    pub fn require_release_or_date(&self, dry_run: bool) -> Result<(String, &'static str)> {
        match self.pin() {
            Some(Pin::Release(r)) => Ok((r, "release")),
            Some(Pin::Date(d)) => Ok((d, "date")),
            Some(Pin::Commit(_)) => Err(IngestError::Other(format!(
                "{}: expected pinned_release or pinned_date",
                self.name
            ))),
            None if dry_run => Ok(("<PIN_TBD>".into(), "release")),
            None => Err(IngestError::UnpinnedSource(self.name.clone())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pin {
    Commit(String),
    Release(String),
    Date(String),
}

impl Pin {
    #[must_use]
    pub fn value(&self) -> &str {
        match self {
            Pin::Commit(s) | Pin::Release(s) | Pin::Date(s) => s,
        }
    }

    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Pin::Commit(_) => "commit",
            Pin::Release(_) => "release",
            Pin::Date(_) => "date",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_id_roundtrip() {
        for id in SourceId::ALL {
            assert_eq!(SourceId::parse(id.as_str()).unwrap(), *id);
        }
    }

    #[test]
    fn pin_variants() {
        let mut s = SourceSpec {
            name: "x".into(),
            upstream_url: "u".into(),
            licence: "l".into(),
            licence_url: None,
            attribution: "a".into(),
            pinned_commit: Some(String::new()),
            pinned_release: Some("v1".into()),
            pinned_date: None,
            sha256: None,
            artefacts: vec![],
            file_hashes: BTreeMap::new(),
            notes: None,
            ftp_url: None,
            rest_url: None,
            download_url: None,
            query: None,
            gpl_obligation: None,
        };
        assert_eq!(s.pin(), Some(Pin::Release("v1".into())));
        s.pinned_release = None;
        assert_eq!(s.pin(), None);
    }

    #[test]
    fn parses_phase0_gapseq_source_toml() {
        let text = r#"
name = "gapseq"
upstream_url = "https://example.invalid"
licence = "GPL-3.0-or-later"
attribution = "cite"
pinned_commit = ""
sha256 = ""
artefacts = ["dat/seed_reactions_corrected.tsv"]
gpl_obligation = true
"#;
        let spec: SourceSpec = toml::from_str(text).unwrap();
        assert_eq!(spec.name, "gapseq");
        assert_eq!(spec.gpl_obligation, Some(true));
        assert_eq!(spec.pin(), None);
        assert!(spec.pinned_hash().is_none());
    }

    #[test]
    fn per_file_hashes_roundtrip() {
        let text = r#"
name = "x"
upstream_url = "u"
licence = "CC-BY-4.0"
attribution = "a"
pinned_release = "1"

[file_hashes]
"chem_xref.tsv" = "abc"
"reac_xref.tsv" = ""
"#;
        let spec: SourceSpec = toml::from_str(text).unwrap();
        assert_eq!(spec.file_hash("chem_xref.tsv"), Some("abc"));
        assert_eq!(spec.file_hash("reac_xref.tsv"), None);
        assert_eq!(spec.file_hash("missing"), None);
    }
}

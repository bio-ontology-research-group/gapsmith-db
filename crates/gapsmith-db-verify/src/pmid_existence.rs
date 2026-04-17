//! PmidExistence — every PMID referenced in `Evidence::citation` must
//! resolve. Offline by default (local JSON cache); online mode hits
//! E-utilities behind a flag.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use gapsmith_db_core::{Database, Pmid};
use tracing::{info, warn};

use crate::diagnostic::{Diagnostic, Target};
use crate::engine::Verifier;

pub const NAME: &str = "pmid_existence";

pub struct PmidExistence {
    pub cache_path: Option<PathBuf>,
    pub online: bool,
    known: Option<HashSet<String>>,
}

impl PmidExistence {
    #[must_use]
    pub fn offline(cache_path: Option<PathBuf>) -> Self {
        Self {
            cache_path,
            online: false,
            known: None,
        }
    }

    #[must_use]
    pub fn with_online(mut self, online: bool) -> Self {
        self.online = online;
        self
    }

    fn ensure_cache(&mut self) {
        if self.known.is_some() {
            return;
        }
        let Some(p) = self.cache_path.clone() else {
            self.known = Some(HashSet::new());
            return;
        };
        self.known = Some(load_cache(&p).unwrap_or_default());
    }

    /// Online lookup is a single batched POST to NCBI E-utilities. Not
    /// called from `check()` by default; only when `online = true`.
    #[cfg(not(test))]
    async fn lookup_online(&self, ids: &[String]) -> crate::Result<HashSet<String>> {
        use crate::VerifyError;
        if ids.is_empty() {
            return Ok(HashSet::new());
        }
        let client = reqwest::Client::builder()
            .build()
            .map_err(|e| VerifyError::Other(format!("http client: {e}")))?;
        let url = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi";
        let joined = ids.join(",");
        let resp = client
            .post(url)
            .form(&[("db", "pubmed"), ("id", &joined), ("retmode", "json")])
            .send()
            .await
            .map_err(|e| VerifyError::Other(format!("esummary: {e}")))?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| VerifyError::Other(format!("esummary body: {e}")))?;
        let mut found = HashSet::new();
        if let Some(result) = body.get("result") {
            for id in ids {
                if result.get(id).is_some() {
                    found.insert(id.clone());
                }
            }
        }
        Ok(found)
    }

    #[cfg(test)]
    #[allow(clippy::unused_async)]
    async fn lookup_online(&self, _ids: &[String]) -> crate::Result<HashSet<String>> {
        Ok(HashSet::new())
    }
}

impl Verifier for PmidExistence {
    fn name(&self) -> &'static str {
        NAME
    }

    fn check(&mut self, db: &Database) -> crate::Result<Vec<Diagnostic>> {
        self.ensure_cache();
        let known = self.known.as_ref().unwrap_or_else(|| unreachable!());

        let mut pmids: Vec<(Target, String)> = Vec::new();
        for c in db.compounds.values() {
            for e in &c.evidence {
                if let Some(ref p) = e.citation {
                    pmids.push((Target::Compound(c.id.clone()), p.as_str().to_string()));
                }
            }
        }
        for r in db.reactions.values() {
            for e in &r.evidence {
                if let Some(ref p) = e.citation {
                    pmids.push((Target::Reaction(r.id.clone()), p.as_str().to_string()));
                }
            }
        }
        for p in db.pathways.values() {
            for e in &p.evidence {
                if let Some(ref pm) = e.citation {
                    pmids.push((Target::Pathway(p.id.clone()), pm.as_str().to_string()));
                }
            }
        }

        let mut out = Vec::new();
        let missing: Vec<String> = pmids
            .iter()
            .filter_map(|(_, p)| {
                if known.contains(p) {
                    None
                } else {
                    Some(p.clone())
                }
            })
            .collect();

        let mut resolved_online: HashSet<String> = HashSet::new();
        if self.online && !missing.is_empty() {
            info!(count = missing.len(), "PMID cache miss; querying online");
            match tokio::runtime::Runtime::new() {
                Ok(rt) => match rt.block_on(self.lookup_online(&missing)) {
                    Ok(s) => resolved_online = s,
                    Err(e) => {
                        warn!(error = %e, "online PMID lookup failed");
                    }
                },
                Err(e) => warn!(error = %e, "failed to create tokio runtime"),
            }
        }

        for (target, p) in pmids {
            if known.contains(&p) || resolved_online.contains(&p) {
                out.push(Diagnostic::info(
                    NAME,
                    target,
                    "ok",
                    format!("PMID {p} resolvable"),
                ));
            } else if self.online {
                out.push(Diagnostic::error(
                    NAME,
                    target,
                    "unknown_pmid",
                    format!("PMID {p} not found in cache or online"),
                ));
            } else {
                out.push(Diagnostic::warn(
                    NAME,
                    target,
                    "unresolved_pmid",
                    format!("PMID {p} not in local cache (rerun with --online to check)"),
                ));
            }
        }
        Ok(out)
    }
}

fn load_cache(path: &Path) -> std::io::Result<HashSet<String>> {
    let mut f = std::fs::File::open(path)?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    if let Ok(vec) = serde_json::from_str::<Vec<String>>(&s) {
        return Ok(vec.into_iter().collect());
    }
    // tolerate {pmid: metadata} object form too.
    if let Ok(obj) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&s) {
        return Ok(obj.keys().cloned().collect());
    }
    Ok(HashSet::new())
}

/// Helper used by tests / CLI to write a tiny cache file.
pub fn save_cache(
    path: &Path,
    ids: impl IntoIterator<Item = impl Into<String>>,
) -> crate::Result<()> {
    let v: Vec<String> = ids.into_iter().map(Into::into).collect();
    let s = serde_json::to_string(&v)?;
    std::fs::write(path, s)?;
    Ok(())
}

// Silence unused-import warnings.
#[allow(dead_code)]
fn _touch_pmid_type() -> Pmid {
    Pmid::new("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gapsmith_db_core::{Compound, CompoundId, Confidence, Evidence, Source};
    use tempfile::tempdir;

    #[test]
    fn cached_pmid_is_ok_offline() {
        let dir = tempdir().unwrap();
        let cache = dir.path().join("pmid_cache.json");
        save_cache(&cache, ["12345".to_string()]).unwrap();

        let mut db = Database::new();
        let mut c = Compound::new(CompoundId::new("C1"));
        let mut e = Evidence::from_source(Source::Chebi, Confidence::CERTAIN);
        e.citation = Some(Pmid::new("12345"));
        c.evidence.push(e);
        let mut e2 = Evidence::from_source(Source::Chebi, Confidence::CERTAIN);
        e2.citation = Some(Pmid::new("99999"));
        c.evidence.push(e2);
        db.insert_compound(c);

        let mut v = PmidExistence::offline(Some(cache));
        let diags = v.check(&db).unwrap();
        assert!(diags.iter().any(|d| d.code.0 == "ok"));
        // Offline-only → missing PMID is a warning, not an error.
        assert!(diags.iter().any(|d| d.code.0 == "unresolved_pmid"));
    }
}

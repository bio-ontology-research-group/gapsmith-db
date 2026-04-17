//! Fixture-backed mock LLM.
//!
//! Deterministic: given a prompt, selects the fixture whose file name
//! sorts first lexicographically in `fixture_dir`, or the one matching
//! `name_hint` if provided. Used for end-to-end tests and Phase-4
//! demonstrations without real API calls.

use std::path::{Path, PathBuf};

use super::LlmBackend;
use crate::schema::Proposal;

#[derive(Debug, Clone)]
pub struct FixtureBackend {
    pub fixture_dir: PathBuf,
    pub name_hint: Option<String>,
    /// Model string embedded into the returned [`Proposal::model`] field.
    pub model_name: String,
}

impl FixtureBackend {
    #[must_use]
    pub fn new(fixture_dir: impl Into<PathBuf>) -> Self {
        Self {
            fixture_dir: fixture_dir.into(),
            name_hint: None,
            model_name: "mock/fixture".into(),
        }
    }

    #[must_use]
    pub fn with_hint(mut self, name_hint: impl Into<String>) -> Self {
        self.name_hint = Some(name_hint.into());
        self
    }
}

impl LlmBackend for FixtureBackend {
    fn name(&self) -> &str {
        &self.model_name
    }

    fn complete(&self, _prompt: &str) -> crate::Result<Proposal> {
        let path = pick_fixture(&self.fixture_dir, self.name_hint.as_deref())?;
        let text = std::fs::read_to_string(&path)?;
        let mut p: Proposal = serde_json::from_str(&text)?;
        p.model.clone_from(&self.model_name);
        Ok(p.hashed())
    }
}

fn pick_fixture(dir: &Path, hint: Option<&str>) -> crate::Result<PathBuf> {
    if !dir.exists() {
        return Err(crate::ProposeError::Other(format!(
            "fixture dir {} does not exist",
            dir.display()
        )));
    }
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    candidates.sort();
    if let Some(hint) = hint
        && let Some(p) = candidates.iter().find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.contains(hint))
        })
    {
        return Ok(p.clone());
    }
    candidates.into_iter().next().ok_or_else(|| {
        crate::ProposeError::Other(format!("no .json fixtures in {}", dir.display()))
    })
}

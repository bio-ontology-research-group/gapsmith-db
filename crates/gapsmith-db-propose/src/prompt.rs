//! Prompt template rendering.
//!
//! The template lives at `prompts/pathway_proposal.md` and is loaded at
//! runtime; [`PROMPT_VERSION`] bumps whenever the template changes so
//! historical proposals can be traced back to the exact wording they were
//! generated under.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::retrieval::Passage;

/// Bump when `prompts/pathway_proposal.md` changes in any way that would
/// alter model output. Keep under semantic-versioning discipline:
/// MAJOR = schema-level changes to the required JSON shape; MINOR =
/// instruction tweaks; PATCH = typos, reordering.
pub const PROMPT_VERSION: &str = "0.2.1";

#[derive(Debug, Clone, Serialize)]
pub struct PromptContext<'a> {
    pub pathway_name: &'a str,
    pub organism_scope: Option<&'a str>,
    pub medium: Option<&'a str>,
    pub notes: Option<&'a str>,
    pub passages: Vec<Passage>,
}

#[derive(Debug, Clone)]
pub struct PromptTemplate {
    pub text: String,
    pub path: Option<PathBuf>,
}

impl PromptTemplate {
    /// Load `prompts/pathway_proposal.md` (or any other path).
    pub fn load(path: &Path) -> crate::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(Self {
            text,
            path: Some(path.to_path_buf()),
        })
    }

    /// Embed the template inline; useful for tests.
    #[must_use]
    pub fn from_string(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            path: None,
        }
    }

    /// Render using Mustache-ish `{{name}}` substitution. Supported keys:
    /// `pathway_name`, `organism_scope`, `medium`, `notes`,
    /// `passages_block`. Unknown keys are left as-is (intentional: let
    /// the model see the placeholder if something's misconfigured).
    #[must_use]
    pub fn render(&self, ctx: &PromptContext<'_>) -> String {
        let passages_block = render_passages(&ctx.passages);
        self.text
            .replace("{{pathway_name}}", ctx.pathway_name)
            .replace(
                "{{organism_scope}}",
                ctx.organism_scope.unwrap_or("(not specified)"),
            )
            .replace("{{medium}}", ctx.medium.unwrap_or("(not specified)"))
            .replace("{{notes}}", ctx.notes.unwrap_or(""))
            .replace("{{passages_block}}", &passages_block)
            .replace("{{prompt_version}}", PROMPT_VERSION)
    }
}

fn render_passages(passages: &[Passage]) -> String {
    use std::fmt::Write;
    if passages.is_empty() {
        return "(no retrieval hits; proceed from prior knowledge only)".into();
    }
    let mut out = String::new();
    for (i, p) in passages.iter().enumerate() {
        let pmid = p
            .pmid
            .as_deref()
            .map(|s| format!(" PMID:{s}"))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "[{idx}] {title}{pmid}\n  url: {url}\n  {text}\n",
            idx = i + 1,
            title = p.title.as_deref().unwrap_or("(untitled)"),
            url = p.source_url,
            text = p.text.replace('\n', " "),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_pathway_name() {
        let t = PromptTemplate::from_string("target: {{pathway_name}}");
        let out = t.render(&PromptContext {
            pathway_name: "glycolysis",
            organism_scope: None,
            medium: None,
            notes: None,
            passages: vec![],
        });
        assert_eq!(out, "target: glycolysis");
    }

    #[test]
    fn version_is_injected() {
        let t = PromptTemplate::from_string("v={{prompt_version}}");
        let out = t.render(&PromptContext {
            pathway_name: "",
            organism_scope: None,
            medium: None,
            notes: None,
            passages: vec![],
        });
        assert!(out.contains(PROMPT_VERSION));
    }

    #[test]
    fn passages_block_renders() {
        let t = PromptTemplate::from_string("refs:\n{{passages_block}}");
        let passages = vec![Passage {
            id: "a".into(),
            text: "methane production".into(),
            source_url: "https://europepmc.org/x".into(),
            pmid: Some("12345".into()),
            title: Some("A paper".into()),
            score: 1.0,
        }];
        let out = t.render(&PromptContext {
            pathway_name: "",
            organism_scope: None,
            medium: None,
            notes: None,
            passages,
        });
        assert!(out.contains("PMID:12345"));
        assert!(out.contains("europepmc"));
    }
}

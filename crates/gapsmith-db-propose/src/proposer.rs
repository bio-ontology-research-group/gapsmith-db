//! End-to-end proposer orchestration.
//!
//! 1. Build a [`PromptContext`] from the target + retrieval results.
//! 2. Render the template.
//! 3. Call the [`LlmBackend`] to obtain a [`Proposal`].
//! 4. Validate the proposal against the schema.
//! 5. Write it to `proposals/pending/<hash>.json`.

use std::path::{Path, PathBuf};

use chrono::Utc;
use tracing::{debug, info};

use crate::llm::LlmBackend;
use crate::prompt::{PROMPT_VERSION, PromptContext, PromptTemplate};
use crate::retrieval::{RetrievalBackend, SearchQuery};
use crate::schema::{Proposal, ProposalTarget};
use crate::{DomainFilter, ProposeError};

#[derive(Debug, Clone)]
pub struct ProposerOptions {
    pub proposals_dir: PathBuf,
    pub top_k: usize,
    pub filter: DomainFilter,
}

impl Default for ProposerOptions {
    fn default() -> Self {
        Self {
            proposals_dir: PathBuf::from("proposals"),
            top_k: 8,
            filter: DomainFilter::default(),
        }
    }
}

pub struct Proposer<'a, L: LlmBackend, R: RetrievalBackend> {
    pub llm: &'a L,
    pub retrieval: &'a R,
    pub template: &'a PromptTemplate,
    pub options: ProposerOptions,
}

impl<'a, L: LlmBackend, R: RetrievalBackend> Proposer<'a, L, R> {
    #[must_use]
    pub fn new(
        llm: &'a L,
        retrieval: &'a R,
        template: &'a PromptTemplate,
        options: ProposerOptions,
    ) -> Self {
        Self {
            llm,
            retrieval,
            template,
            options,
        }
    }

    /// Run one proposal cycle. Returns the written proposal path.
    pub fn propose(&self, target: &ProposalTarget) -> crate::Result<(Proposal, PathBuf)> {
        let query = SearchQuery {
            text: target.pathway_name.clone(),
            top_k: self.options.top_k,
        };
        let passages = self.retrieval.search(&query)?;
        debug!(
            pathway = %target.pathway_name,
            hits = passages.len(),
            "retrieval complete"
        );
        // Second-belt domain filter, regardless of what the backend did.
        let passages = crate::retrieval::filter_passages(&self.options.filter, passages);

        let ctx = PromptContext {
            pathway_name: &target.pathway_name,
            organism_scope: target.organism_scope.as_deref(),
            medium: target.medium.as_deref(),
            notes: target.notes.as_deref(),
            passages,
        };
        let prompt = self.template.render(&ctx);
        debug!(len = prompt.len(), "prompt rendered");

        let mut proposal = self.llm.complete(&prompt)?;
        proposal.prompt_version = PROMPT_VERSION.to_string();
        proposal.target = target.clone();
        if proposal.created_at.timestamp() == 0 {
            proposal.created_at = Utc::now();
        }
        proposal = proposal.hashed();
        proposal.validate()?;

        let path = write_pending(&self.options.proposals_dir, &proposal)?;
        info!(id = %proposal.proposal_id, path = %path.display(), "proposal written");
        Ok((proposal, path))
    }
}

fn write_pending(dir: &Path, p: &Proposal) -> crate::Result<PathBuf> {
    let out_dir = dir.join("pending");
    std::fs::create_dir_all(&out_dir)?;
    let id = p
        .proposal_id
        .strip_prefix("sha256:")
        .unwrap_or(&p.proposal_id);
    let path = out_dir.join(format!("{id}.json"));
    let body = serde_json::to_string_pretty(p)?;
    std::fs::write(&path, body)?;
    Ok(path)
}

/// Convenience: emit a fixture proposal into `pending/` without going
/// through retrieval or an LLM. Useful for CLI smoke tests.
pub fn ingest_fixture_proposal(
    proposals_dir: &Path,
    proposal: Proposal,
) -> crate::Result<(Proposal, PathBuf)> {
    let p = proposal.hashed();
    p.validate()?;
    let path = write_pending(proposals_dir, &p)?;
    Ok((p, path))
}

// Silence unused-import warnings on types referenced only through generics.
#[allow(dead_code)]
fn _unused() -> ProposeError {
    ProposeError::Other(String::new())
}

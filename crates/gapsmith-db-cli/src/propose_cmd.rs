//! `gapsmith-db propose` subcommand. Phase-4 default: mock proposer
//! driven by a fixture file; no real LLM calls.

use std::path::{Path, PathBuf};

use anyhow::Context;
use gapsmith_db_propose::llm::{FixtureBackend, LlmBackend, OpenRouterBackend, OpenRouterConfig};
use gapsmith_db_propose::retrieval::{InMemoryBackend, Passage, RetrievalBackend};
use gapsmith_db_propose::{
    DomainFilter, PromptTemplate, Proposer, ProposerOptions, schema::ProposalTarget,
};
use tracing::info;

use crate::ProposeArgs;

pub fn run(args: ProposeArgs) -> anyhow::Result<()> {
    let template = if args.prompt.exists() {
        PromptTemplate::load(&args.prompt)?
    } else {
        info!(path = %args.prompt.display(), "prompt file missing; using built-in default");
        PromptTemplate::from_string(include_str!("../../../prompts/pathway_proposal.md"))
    };

    let retrieval = load_retrieval(&args)?;

    let opts = ProposerOptions {
        proposals_dir: args.proposals_dir.clone(),
        top_k: args.top_k,
        filter: DomainFilter::default(),
    };

    let target = ProposalTarget {
        pathway_name: args
            .query
            .clone()
            .unwrap_or_else(|| "central carbon metabolism".to_string()),
        organism_scope: args.organism,
        medium: args.medium,
        notes: None,
    };

    if args.mock {
        let llm = FixtureBackend::new(&args.fixture_dir);
        let proposer = Proposer::new(&llm, &retrieval, &template, opts);
        let (p, path) = proposer.propose(&target)?;
        info!(id = %p.proposal_id, path = %path.display(), "fixture proposal written");
    } else {
        let cfg = OpenRouterConfig::new(
            args.model
                .clone()
                .ok_or_else(|| anyhow::anyhow!("--model NAME required without --mock"))?,
        );
        let llm = OpenRouterBackend::new(cfg);
        let proposer = Proposer::new(&llm, &retrieval, &template, opts);
        let (p, path) = proposer
            .propose(&target)
            .with_context(|| format!("openrouter model {}", llm.name()))?;
        info!(id = %p.proposal_id, path = %path.display(), "proposal written");
    }
    Ok(())
}

fn load_retrieval(args: &ProposeArgs) -> anyhow::Result<InMemoryBackend> {
    // Phase-4 default: no retrieval. Load from a JSON array if the file exists.
    let mut passages: Vec<Passage> = Vec::new();
    if let Some(path) = args.passages.as_ref() {
        let bytes = std::fs::read(path)
            .with_context(|| format!("reading passages from {}", path.display()))?;
        passages = serde_json::from_slice(&bytes)?;
    }
    Ok(InMemoryBackend::new(passages))
}

// Silence unused-import warnings.
#[allow(dead_code)]
fn _unused(_: &Path, _: &PathBuf, _: &dyn RetrievalBackend) {}

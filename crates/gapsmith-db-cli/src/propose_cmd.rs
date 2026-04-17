//! `gapsmith-db propose` subcommand — single pathway, mock or live.

use anyhow::Context;
use gapsmith_db_propose::llm::{FixtureBackend, LlmBackend, OpenRouterBackend, OpenRouterConfig};
use gapsmith_db_propose::{
    DomainFilter, PromptTemplate, Proposer, ProposerOptions, schema::ProposalTarget,
};
use tracing::info;

use crate::ProposeArgs;
use crate::retrieval_factory;

pub fn run(args: ProposeArgs) -> anyhow::Result<()> {
    let template = if args.prompt.exists() {
        PromptTemplate::load(&args.prompt)?
    } else {
        info!(path = %args.prompt.display(), "prompt file missing; using built-in default");
        PromptTemplate::from_string(include_str!("../../../prompts/pathway_proposal.md"))
    };

    let retrieval = retrieval_factory::build(&args.retrieval)?;

    let opts = ProposerOptions {
        proposals_dir: args.proposals_dir.clone(),
        top_k: args.retrieval.top_k,
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

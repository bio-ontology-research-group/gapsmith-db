//! `gapsmith-db propose-catalogue` — batch driver.
//!
//! Iterates a pathway-name seed (TSV produced in proposals/catalogue/),
//! calls the proposer per row, writes each proposal to
//! `proposals/pending/<hash>.json`, and records a per-run TSV log so
//! the operator can resume or audit.
//!
//! Throttling (`--throttle-ms`) and `--limit` keep free-tier rate limits
//! happy; `--resume` skips any pathway with an existing proposal (by
//! `target.pathway_name`).
#![allow(clippy::needless_pass_by_value, clippy::too_many_lines)]

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Context;
use chrono::Utc;
use gapsmith_db_propose::llm::{OpenRouterBackend, OpenRouterConfig};
use gapsmith_db_propose::{
    DomainFilter, PromptTemplate, Proposer, ProposerOptions, schema::ProposalTarget,
};
use tracing::{info, warn};

use crate::retrieval_factory::{self, RetrievalArgs};

#[derive(clap::Args, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct ProposeCatalogueArgs {
    /// One or more TSV seed files. Rows are concatenated.
    #[arg(long, required = true)]
    pub seed: Vec<PathBuf>,

    /// OpenRouter model slug (e.g. qwen/qwen3.6-plus).
    #[arg(long)]
    pub model: String,

    /// Restrict to rows whose `category` column matches.
    #[arg(long)]
    pub category: Option<String>,

    /// Stop after this many proposals in this run.
    #[arg(long)]
    pub limit: Option<usize>,

    /// Skip the first N matching rows (checkpoint restart).
    #[arg(long, default_value_t = 0)]
    pub skip: usize,

    /// Skip rows whose pathway_name already has a proposal on disk.
    #[arg(long)]
    pub resume: bool,

    /// Milliseconds to sleep between successful calls (rate-limit guard).
    #[arg(long, default_value_t = 0)]
    pub throttle_ms: u64,

    /// Print the plan without calling the LLM.
    #[arg(long)]
    pub dry_run: bool,

    /// Prompt template path.
    #[arg(long, default_value = "prompts/pathway_proposal.md")]
    pub prompt: PathBuf,

    /// Proposals output root.
    #[arg(long, default_value = "proposals")]
    pub proposals_dir: PathBuf,

    /// Per-run TSV log. Auto-named under `proposals/catalogue/` if unset.
    #[arg(long)]
    pub log: Option<PathBuf>,

    #[command(flatten)]
    pub retrieval: RetrievalArgs,
}

pub fn run(args: ProposeCatalogueArgs) -> anyhow::Result<()> {
    let rows = load_rows(&args.seed, args.category.as_deref())?;
    info!(
        count = rows.len(),
        category = args.category.as_deref().unwrap_or("(all)"),
        "catalogue rows loaded"
    );

    let existing = if args.resume {
        scan_existing(&args.proposals_dir)?
    } else {
        HashSet::new()
    };

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

    let log_path = args.log.clone().unwrap_or_else(|| {
        args.proposals_dir.join("runs").join(format!(
            "catalogue_{}.tsv",
            Utc::now().format("%Y%m%dT%H%M%SZ")
        ))
    });
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut log =
        File::create(&log_path).with_context(|| format!("creating log {}", log_path.display()))?;
    writeln!(log, "timestamp\tpathway\tstatus\tdetail")?;
    info!(log = %log_path.display(), "per-run log");

    let cfg = OpenRouterConfig::new(args.model.clone());
    let llm = OpenRouterBackend::new(cfg);
    let proposer = Proposer::new(&llm, &retrieval, &template, opts);

    let mut ok = 0_usize;
    let mut skipped = 0_usize;
    let mut failed = 0_usize;
    let mut processed = 0_usize;

    for (i, row) in rows.iter().enumerate() {
        if i < args.skip {
            continue;
        }
        if args.resume && existing.contains(&row.pathway_name) {
            writeln!(
                log,
                "{}\t{}\tskipped\texisting",
                Utc::now().to_rfc3339(),
                row.pathway_name
            )?;
            skipped += 1;
            continue;
        }
        if let Some(max) = args.limit
            && processed >= max
        {
            break;
        }
        processed += 1;
        info!(
            i,
            pathway = %row.pathway_name,
            category = %row.category,
            "proposing"
        );

        if args.dry_run {
            writeln!(
                log,
                "{}\t{}\tdry_run\t",
                Utc::now().to_rfc3339(),
                row.pathway_name
            )?;
            continue;
        }

        let target = ProposalTarget {
            pathway_name: row.pathway_name.clone(),
            organism_scope: row.organism_scope.clone(),
            medium: None,
            notes: row.notes.clone(),
        };
        let start = Instant::now();
        match proposer.propose(&target) {
            Ok((p, path)) => {
                ok += 1;
                writeln!(
                    log,
                    "{}\t{}\tok\t{} ({}ms, {})",
                    Utc::now().to_rfc3339(),
                    row.pathway_name,
                    p.proposal_id,
                    start.elapsed().as_millis(),
                    path.display(),
                )?;
            }
            Err(e) => {
                failed += 1;
                warn!(pathway = %row.pathway_name, err = %e, "proposal failed");
                writeln!(
                    log,
                    "{}\t{}\tfailed\t{e}",
                    Utc::now().to_rfc3339(),
                    row.pathway_name,
                )?;
            }
        }
        log.flush().ok();
        if args.throttle_ms > 0 {
            std::thread::sleep(Duration::from_millis(args.throttle_ms));
        }
    }

    info!(ok, failed, skipped, processed, "catalogue run complete");
    println!(
        "ok={ok} failed={failed} skipped={skipped} (log: {})",
        log_path.display()
    );
    Ok(())
}

#[derive(Debug, Clone)]
struct Row {
    pathway_name: String,
    category: String,
    organism_scope: Option<String>,
    notes: Option<String>,
}

fn load_rows(paths: &[PathBuf], category: Option<&str>) -> anyhow::Result<Vec<Row>> {
    let mut rows = Vec::new();
    for p in paths {
        let f = File::open(p).with_context(|| format!("opening {}", p.display()))?;
        let rdr = BufReader::new(f);
        for (i, line) in rdr.lines().enumerate() {
            let line = line?;
            if i == 0 && line.starts_with("pathway_name\t") {
                continue;
            }
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let cells: Vec<&str> = line.split('\t').collect();
            let pathway_name = cells.first().copied().unwrap_or("").trim().to_string();
            if pathway_name.is_empty() {
                continue;
            }
            let cat = cells.get(1).copied().unwrap_or("").to_string();
            if let Some(want) = category
                && cat != want
            {
                continue;
            }
            let organism_scope = cells
                .get(2)
                .map(|s| (*s).to_string())
                .filter(|s| !s.is_empty());
            let notes = cells
                .get(5)
                .map(|s| (*s).to_string())
                .filter(|s| !s.is_empty());
            rows.push(Row {
                pathway_name,
                category: cat,
                organism_scope,
                notes,
            });
        }
    }
    Ok(rows)
}

fn scan_existing(proposals_dir: &Path) -> anyhow::Result<HashSet<String>> {
    let mut seen = HashSet::new();
    for sub in ["pending", "for_curation", "rejected"] {
        let d = proposals_dir.join(sub);
        if !d.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&d)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            if let Some(name) = v
                .get("target")
                .and_then(|t| t.get("pathway_name"))
                .and_then(|n| n.as_str())
            {
                seen.insert(name.to_string());
            }
        }
    }
    Ok(seen)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn load_rows_skips_header_and_filters_category() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            tmp.path(),
            "pathway_name\tcategory\torganism_scope\tsource\tsource_id\tnotes\n\
             EMP\tcentral_carbon\tuniversal\ttextbook\t\tnet 2 ATP\n\
             ED\tcentral_carbon\tbacteria/archaea\ttextbook\t\t\n\
             Hydrogenotrophic methanogenesis\tmethanogenesis\tEuryarchaeota\ttextbook\t\t\n",
        )
        .unwrap();
        let rows = load_rows(&[tmp.path().to_path_buf()], None).unwrap();
        assert_eq!(rows.len(), 3);
        let rows = load_rows(&[tmp.path().to_path_buf()], Some("methanogenesis")).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].pathway_name, "Hydrogenotrophic methanogenesis");
    }
}

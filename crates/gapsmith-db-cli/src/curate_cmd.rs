//! `gapsmith-db curate …` subcommands.
#![allow(
    clippy::needless_pass_by_value,
    clippy::case_sensitive_file_extension_comparisons
)]

use std::path::{Path, PathBuf};

use anyhow::Context;
use gapsmith_db_propose::{
    ChainIssue, Decision, DecisionAction, DecisionLog, MergeReport, Proposal, merge_proposal,
};
use tracing::{info, warn};

use crate::CurateArgs;

pub fn run(args: CurateArgs) -> anyhow::Result<()> {
    match args.action {
        crate::CurateAction::List(a) => list(a),
        crate::CurateAction::Show(a) => show(a),
        crate::CurateAction::Accept(a) => record(a, DecisionAction::Accept),
        crate::CurateAction::Reject(a) => record(a, DecisionAction::Reject),
        crate::CurateAction::Log(a) => tail_log(a),
        crate::CurateAction::VerifyChain(a) => verify_chain(a),
    }
}

// --- list ------------------------------------------------------------------

pub fn list(args: crate::CurateListArgs) -> anyhow::Result<()> {
    let dir = &args.proposals_dir;
    let states: Vec<&str> = match args.state.as_deref() {
        Some("pending") => vec!["pending"],
        Some("for_curation") => vec!["for_curation"],
        Some("rejected") => vec!["rejected"],
        Some("all") | None => vec!["pending", "for_curation", "rejected"],
        Some(other) => anyhow::bail!("unknown --state {other}"),
    };
    for state in states {
        let sub = dir.join(state);
        if !sub.exists() {
            continue;
        }
        let mut entries: Vec<_> = std::fs::read_dir(&sub)?
            .filter_map(std::result::Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.extension().and_then(|s| s.to_str()) == Some("json")
                    && !p
                        .file_name()
                        .and_then(|s| s.to_str())
                        .is_some_and(|s| s.ends_with(".report.json"))
            })
            .collect();
        entries.sort();
        println!("== {state} ({}) ==", entries.len());
        for p in entries {
            match std::fs::read_to_string(&p) {
                Ok(t) => match serde_json::from_str::<Proposal>(&t) {
                    Ok(pr) => {
                        println!(
                            "  {}  {}  '{}'",
                            short(&pr.proposal_id),
                            pr.model,
                            pr.target.pathway_name,
                        );
                    }
                    Err(e) => println!("  {} (unparseable: {e})", p.display()),
                },
                Err(e) => println!("  {} (unreadable: {e})", p.display()),
            }
        }
    }
    Ok(())
}

// --- show ------------------------------------------------------------------

pub fn show(args: crate::CurateShowArgs) -> anyhow::Result<()> {
    let path = find_proposal_file(&args.proposals_dir, &args.id)?;
    let text = std::fs::read_to_string(&path)?;
    let p: Proposal = serde_json::from_str(&text)?;

    println!("proposal   : {}", p.proposal_id);
    println!("file       : {}", path.display());
    println!("model      : {}", p.model);
    println!("created    : {}", p.created_at);
    println!("target     : {}", p.target.pathway_name);
    if let Some(org) = &p.target.organism_scope {
        println!("organism   : {org}");
    }
    if let Some(m) = &p.target.medium {
        println!("medium     : {m}");
    }
    println!("reactions  :");
    for r in &p.reactions {
        match &r.reference {
            gapsmith_db_propose::ReactionRef::Rhea(id) => {
                println!("  - {} rhea:{id}", r.local_id);
            }
            gapsmith_db_propose::ReactionRef::ChebiEc {
                ec,
                substrates,
                products,
            } => {
                println!(
                    "  - {} ec:{ec} ({} -> {})",
                    r.local_id,
                    substrates.join(" + "),
                    products.join(" + "),
                );
            }
        }
    }
    if !p.enzymes.is_empty() {
        println!("enzymes    :");
        for e in &p.enzymes {
            println!(
                "  - uniprot:{} catalyses {}",
                e.uniprot,
                e.catalyses.join(",")
            );
        }
    }
    if !p.dag.is_empty() {
        println!("dag        :");
        for e in &p.dag {
            println!("  - {} -> {}", e.from, e.to);
        }
    }
    if !p.citations.is_empty() {
        println!("citations  :");
        for c in &p.citations {
            println!(
                "  - pmid:{}{}",
                c.pmid,
                c.note
                    .as_deref()
                    .map(|n| format!(" ({n})"))
                    .unwrap_or_default()
            );
        }
    }
    // If a DB is provided, annotate which reactions already exist.
    if let Some(db_path) = &args.db {
        let db = gapsmith_db_core::serde_io::read_binary(db_path)
            .with_context(|| format!("loading DB from {}", db_path.display()))?;
        diff_against_db(&p, &db);
    }
    Ok(())
}

fn diff_against_db(p: &Proposal, db: &gapsmith_db_core::Database) {
    use gapsmith_db_core::Source;
    println!("diff vs DB :");
    for r in &p.reactions {
        let (tag, hit) = match &r.reference {
            gapsmith_db_propose::ReactionRef::Rhea(id) => {
                let exists = db.reactions.values().any(|rr| {
                    rr.rhea_id.as_deref() == Some(id.as_str())
                        || rr
                            .xrefs
                            .get(&Source::Rhea)
                            .is_some_and(|v| v.iter().any(|x| x == id))
                });
                (format!("rhea:{id}"), exists)
            }
            gapsmith_db_propose::ReactionRef::ChebiEc { ec, .. } => {
                let exists = db.reactions.values().any(|rr| rr.ec_numbers.contains(ec));
                (format!("ec:{ec}"), exists)
            }
        };
        println!("  {} {} {}", if hit { "=" } else { "+" }, r.local_id, tag);
    }
}

// --- accept / reject -------------------------------------------------------

fn record(args: crate::CurateDecideArgs, action: DecisionAction) -> anyhow::Result<()> {
    let path = find_proposal_file(&args.proposals_dir, &args.id)?;
    let text = std::fs::read_to_string(&path)?;
    let p: Proposal = serde_json::from_str(&text)?;
    p.validate()?;

    let log = DecisionLog::at(&args.proposals_dir);
    let head = log.head()?;
    let d = Decision::new(
        &head,
        &p.proposal_id,
        action,
        &args.curator,
        args.comment.clone(),
        None,
    )
    .finalised();
    log.append(&d)?;

    // Merge into the canonical DB when accepting and --merge-into is set.
    // Runs *after* the decision log append so the chain head already
    // references this decision; if the merge itself fails we still have
    // a durable record of the accept.
    let merge_report = if matches!(action, DecisionAction::Accept) {
        merge_into_db(&args, &p)?
    } else {
        None
    };

    // Move the proposal file to `decisions/<id>.json` so its state is stable.
    let decisions_dir = args.proposals_dir.join("decisions");
    std::fs::create_dir_all(&decisions_dir)?;
    let new_path = decisions_dir.join(
        path.file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("unknown.json")),
    );
    if new_path != path {
        std::fs::rename(&path, &new_path)?;
    }
    info!(
        decision = %d.decision_id,
        proposal = %p.proposal_id,
        action = ?action,
        curator = %args.curator,
        "decision recorded"
    );
    println!("recorded: {} ({:?})", d.decision_id, action);
    if let Some(r) = merge_report {
        print_merge_report(&r);
    }
    Ok(())
}

fn merge_into_db(
    args: &crate::CurateDecideArgs,
    p: &Proposal,
) -> anyhow::Result<Option<MergeReport>> {
    let Some(db_path) = &args.merge_into else {
        return Ok(None);
    };
    let mut db = gapsmith_db_core::serde_io::read_binary(db_path)
        .with_context(|| format!("loading DB from {}", db_path.display()))?;
    let report = merge_proposal(&mut db, p, &args.curator)?;
    db.validate()
        .with_context(|| "post-merge DB failed invariant check")?;
    gapsmith_db_core::serde_io::write_binary(&db, db_path)
        .with_context(|| format!("writing DB to {}", db_path.display()))?;
    if let Some(tsv_out) = &args.tsv_out {
        gapsmith_db_core::serde_io::write_tsv_dir(&db, tsv_out)
            .with_context(|| format!("writing TSV to {}", tsv_out.display()))?;
    }
    for w in &report.warnings {
        warn!(warning = %w, "merge warning");
    }
    Ok(Some(report))
}

fn print_merge_report(r: &MergeReport) {
    println!("merged     : pathway {}", r.pathway_id);
    if !r.reactions_linked.is_empty() {
        println!("  linked   : {} reaction(s)", r.reactions_linked.len());
        for (label, id) in &r.reactions_linked {
            println!("    = {label} -> {id}");
        }
    }
    if !r.reactions_created.is_empty() {
        println!("  created  : {} reaction(s)", r.reactions_created.len());
        for (label, id) in &r.reactions_created {
            println!("    + {label} -> {id}");
        }
    }
    if !r.compounds_created.is_empty() {
        println!("  compounds: {} minted", r.compounds_created.len());
    }
    println!(
        "  enzymes  : {} attachment(s), citations: {}",
        r.enzymes_attached, r.citations_attached
    );
    if !r.warnings.is_empty() {
        println!("  warnings : {}", r.warnings.len());
        for w in &r.warnings {
            println!("    ! {w}");
        }
    }
}

// --- log / verify ----------------------------------------------------------

pub fn tail_log(args: crate::CurateLogArgs) -> anyhow::Result<()> {
    let log = DecisionLog::at(&args.proposals_dir);
    let entries = log.read_all()?;
    if entries.is_empty() {
        println!("(no decisions)");
        return Ok(());
    }
    let take = args.limit.unwrap_or(entries.len());
    for d in entries.iter().rev().take(take).rev() {
        println!(
            "{}  {:?}  curator={} proposal={} prev={}",
            d.timestamp,
            d.action,
            d.curator,
            short(&d.proposal_id),
            short(&d.previous_decision_hash)
        );
        if let Some(c) = &d.comment {
            println!("    comment: {c}");
        }
        println!("    id     : {}", d.decision_id);
    }
    Ok(())
}

pub fn verify_chain(args: crate::CurateVerifyArgs) -> anyhow::Result<()> {
    let log = DecisionLog::at(&args.proposals_dir);
    let v = log.verify_chain()?;
    println!("checked {} decision(s); head = {}", v.count, v.head);
    if v.is_ok() {
        println!("chain OK");
        return Ok(());
    }
    for issue in &v.issues {
        println!("  {issue}");
    }
    match issue_severity(&v.issues) {
        IssueSeverity::Fatal => anyhow::bail!("decision chain is broken"),
        IssueSeverity::None => Ok(()),
    }
}

enum IssueSeverity {
    None,
    Fatal,
}

fn issue_severity(issues: &[ChainIssue]) -> IssueSeverity {
    if issues.is_empty() {
        IssueSeverity::None
    } else {
        IssueSeverity::Fatal
    }
}

// --- helpers ---------------------------------------------------------------

fn short(s: &str) -> String {
    let tail = s.rsplit(':').next().unwrap_or(s);
    let take: String = tail.chars().take(12).collect();
    take
}

fn find_proposal_file(dir: &Path, id: &str) -> anyhow::Result<PathBuf> {
    let hex = id.trim_start_matches("sha256:");
    for sub in ["pending", "for_curation", "rejected", "decisions"] {
        let p = dir.join(sub).join(format!("{hex}.json"));
        if p.exists() {
            return Ok(p);
        }
    }
    // Prefix match, useful for abbreviated IDs.
    for sub in ["pending", "for_curation", "rejected", "decisions"] {
        let subdir = dir.join(sub);
        if !subdir.exists() {
            continue;
        }
        for entry in std::fs::read_dir(&subdir)?.flatten() {
            let name = entry.file_name().into_string().unwrap_or_default();
            if name.starts_with(hex) && name.ends_with(".json") && !name.ends_with(".report.json") {
                return Ok(entry.path());
            }
        }
    }
    anyhow::bail!("no proposal found for id {id} under {}", dir.display());
}

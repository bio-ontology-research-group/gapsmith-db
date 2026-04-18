//! `gapsmith-db verify-proposals` — run the subset of verifiers that
//! operate on a proposal JSON directly, without needing the full
//! ingested DB.
//!
//! What this covers:
//!
//! - EC validity: every `chebi_ec.ec` checked against IntEnz enzyme.dat.
//! - UniProt existence: every `enzymes[*].uniprot` checked against the
//!   local Swiss-Prot snapshot (primary + secondary accessions).
//! - PMID existence: every citation checked against a PMID cache; with
//!   `--online` unknown PMIDs are resolved via NCBI E-utils.
//!
//! What this does NOT cover (needs the ingested DB or Python bridge):
//!
//! - Atom balance / charge balance: need ChEBI compound formulas.
//! - ΔG feasibility (eQuilibrator).
//! - ATP-cycle regression + pathway flux (need universal SBML).
//!
//! The heavyweight verifiers run via `gapsmith-db verify --db …` once
//! the ingested DB is available. `verify-proposals` is the pre-ingest
//! gate — cheap enough to run on every proposal before curator review.
#![allow(clippy::needless_pass_by_value, clippy::too_many_lines)]

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use chrono::Utc;
use gapsmith_db_propose::schema::{Proposal, ReactionRef};
use gapsmith_db_propose::{ProposalDisposition, route_proposal};
use gapsmith_db_verify::diagnostic::VerifierRun;
use gapsmith_db_verify::{Diagnostic, Severity, Target, VerifierReport, VerifierSummary};
use indexmap::IndexMap;
use tracing::{info, warn};

const VERIFIER_EC: &str = "ec_validity";
const VERIFIER_UNIPROT: &str = "uniprot_existence";
const VERIFIER_PMID: &str = "pmid_existence";

#[derive(clap::Args, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct VerifyProposalsArgs {
    /// Proposals root (reads from `pending/`).
    #[arg(long, default_value = "proposals")]
    pub proposals_dir: PathBuf,

    /// Swiss-Prot snapshot (gzipped JSON produced by
    /// scripts/fetch_uniprot_snapshot.py).
    #[arg(long)]
    pub uniprot_snapshot: Option<PathBuf>,

    /// IntEnz / ExPASy enzyme.dat flat file.
    #[arg(long)]
    pub intenz_dat: Option<PathBuf>,

    /// PMID cache file (JSON object `{pmid: ...}` or JSON list of
    /// strings). Missing PMIDs are fatal unless --online is passed.
    #[arg(long)]
    pub pmid_cache: Option<PathBuf>,

    /// Resolve unknown PMIDs via NCBI E-utils (one request per
    /// proposal, batched).
    #[arg(long)]
    pub online_pmid: bool,

    /// Severity threshold that rejects a proposal. Default: `error`.
    #[arg(long, default_value = "error")]
    pub severity_threshold: String,

    /// Actually move proposals to `for_curation/` or `rejected/`
    /// with a sidecar report. Without this, verify-proposals only
    /// prints per-proposal diagnostics.
    #[arg(long)]
    pub route: bool,

    /// Only process this proposal ID (sha256 prefix or full hash).
    #[arg(long)]
    pub only: Option<String>,
}

pub fn run(args: VerifyProposalsArgs) -> anyhow::Result<()> {
    let threshold = parse_severity(&args.severity_threshold)?;

    let uniprot_accs = match args.uniprot_snapshot.as_ref() {
        Some(p) => load_uniprot(p)?,
        None => HashSet::new(),
    };
    let ec_set = match args.intenz_dat.as_ref() {
        Some(p) => load_intenz(p)?,
        None => HashSet::new(),
    };
    let mut pmid_cache: HashSet<String> = match args.pmid_cache.as_ref() {
        Some(p) => load_pmid_cache(p)?,
        None => HashSet::new(),
    };

    info!(
        uniprot = uniprot_accs.len(),
        ec = ec_set.len(),
        pmid_cache = pmid_cache.len(),
        "reference sets loaded"
    );

    let pending_dir = args.proposals_dir.join("pending");
    if !pending_dir.exists() {
        bail!(
            "no proposals/pending/ directory at {}",
            pending_dir.display()
        );
    }
    let mut summary_accept = 0_usize;
    let mut summary_reject = 0_usize;
    let mut summary_skip = 0_usize;

    for entry in std::fs::read_dir(&pending_dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Some(only) = args.only.as_deref() {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if !stem.starts_with(only.trim_start_matches("sha256:")) {
                continue;
            }
        }
        let bytes =
            std::fs::read(&path).with_context(|| format!("reading proposal {}", path.display()))?;
        let proposal: Proposal = serde_json::from_slice(&bytes)
            .with_context(|| format!("parsing {}", path.display()))?;

        let report = verify_one(
            &proposal,
            &uniprot_accs,
            &ec_set,
            &mut pmid_cache,
            args.online_pmid,
        );
        print_report(&proposal, &report);

        if args.route {
            let (disp, _) = route_proposal(&args.proposals_dir, &proposal, &report, threshold)?;
            match disp {
                ProposalDisposition::ForCuration => summary_accept += 1,
                ProposalDisposition::Rejected => summary_reject += 1,
            }
        } else {
            summary_skip += 1;
        }
    }

    if args.route {
        println!(
            "routed: {summary_accept} for_curation, {summary_reject} rejected (threshold={threshold:?})"
        );
    } else {
        println!("{summary_skip} proposals verified (dry-run; pass --route to move files)");
    }
    Ok(())
}

fn parse_severity(s: &str) -> anyhow::Result<Severity> {
    match s.to_ascii_lowercase().as_str() {
        "error" => Ok(Severity::Error),
        "warning" | "warn" => Ok(Severity::Warning),
        "info" => Ok(Severity::Info),
        other => bail!("unknown --severity-threshold {other} (use error/warning/info)"),
    }
}

fn print_report(proposal: &Proposal, report: &VerifierReport) {
    let name = &proposal.target.pathway_name;
    let s = &report.summary;
    println!(
        "\n{name}  [{}]\n  total={}  info={}  warning={}  error={}",
        proposal.proposal_id, s.total, s.info, s.warning, s.error
    );
    for run in report.by_verifier.values() {
        for d in &run.diagnostics {
            if matches!(d.severity, Severity::Info) {
                continue;
            }
            let sev = match d.severity {
                Severity::Error => "ERR ",
                Severity::Warning => "WARN",
                Severity::Info => "INFO",
            };
            println!("  [{sev}] {} — {}: {}", d.verifier, d.code, d.message);
        }
    }
}

fn verify_one(
    p: &Proposal,
    uniprot: &HashSet<String>,
    ec: &HashSet<String>,
    pmid_cache: &mut HashSet<String>,
    online: bool,
) -> VerifierReport {
    let start = Utc::now();
    let mut by_verifier: IndexMap<String, VerifierRun> = IndexMap::new();

    by_verifier.insert(VERIFIER_EC.into(), run_ec_validity(p, ec));
    by_verifier.insert(VERIFIER_UNIPROT.into(), run_uniprot(p, uniprot));
    by_verifier.insert(
        VERIFIER_PMID.into(),
        run_pmid_existence(p, pmid_cache, online),
    );

    let end = Utc::now();
    let all: Vec<Diagnostic> = by_verifier
        .values()
        .flat_map(|r| r.diagnostics.iter().cloned())
        .collect();
    let summary = VerifierSummary::from_diagnostics(&all);

    VerifierReport {
        started_at: start,
        finished_at: end,
        summary,
        by_verifier,
    }
}

fn run_ec_validity(p: &Proposal, known: &HashSet<String>) -> VerifierRun {
    let start = Utc::now();
    let mut diags: Vec<Diagnostic> = Vec::new();

    if known.is_empty() {
        diags.push(Diagnostic::warn(
            VERIFIER_EC,
            Target::Database,
            "no_reference",
            "no IntEnz enzyme.dat configured; EC checks skipped",
        ));
    } else {
        for r in &p.reactions {
            if let ReactionRef::ChebiEc { ec, .. } = &r.reference {
                let ec_str = ec.to_string();
                if known.contains(&ec_str) {
                    diags.push(Diagnostic::info(
                        VERIFIER_EC,
                        Target::Database,
                        "ok",
                        format!("R{} EC {ec_str}", r.local_id),
                    ));
                } else {
                    diags.push(Diagnostic::error(
                        VERIFIER_EC,
                        Target::Database,
                        "unknown_ec",
                        format!("R{}: EC {ec_str} not in IntEnz", r.local_id),
                    ));
                }
            }
        }
    }
    let end = Utc::now();
    VerifierRun {
        summary: VerifierSummary::from_diagnostics(&diags),
        started_at: start,
        finished_at: end,
        diagnostics: diags,
        run_error: None,
    }
}

fn run_uniprot(p: &Proposal, known: &HashSet<String>) -> VerifierRun {
    let start = Utc::now();
    let mut diags = Vec::new();
    if known.is_empty() {
        diags.push(Diagnostic::warn(
            VERIFIER_UNIPROT,
            Target::Database,
            "no_reference",
            "no Swiss-Prot snapshot configured; UniProt checks skipped",
        ));
    } else {
        for e in &p.enzymes {
            if known.contains(&e.uniprot) {
                diags.push(Diagnostic::info(
                    VERIFIER_UNIPROT,
                    Target::Database,
                    "ok",
                    format!("{} exists", e.uniprot),
                ));
            } else {
                diags.push(Diagnostic::error(
                    VERIFIER_UNIPROT,
                    Target::Database,
                    "unknown_uniprot",
                    format!("{} not in Swiss-Prot snapshot", e.uniprot),
                ));
            }
        }
    }
    let end = Utc::now();
    VerifierRun {
        summary: VerifierSummary::from_diagnostics(&diags),
        started_at: start,
        finished_at: end,
        diagnostics: diags,
        run_error: None,
    }
}

fn run_pmid_existence(p: &Proposal, cache: &mut HashSet<String>, online: bool) -> VerifierRun {
    let start = Utc::now();
    let mut diags = Vec::new();
    let pmids: Vec<String> = p
        .citations
        .iter()
        .map(|c| c.pmid.as_str().to_string())
        .collect();
    let unknown: Vec<String> = pmids
        .iter()
        .filter(|pm| !cache.contains(pm.as_str()))
        .cloned()
        .collect();
    if online && !unknown.is_empty() {
        match resolve_pmids_online(&unknown) {
            Ok(resolved) => {
                for pm in resolved {
                    cache.insert(pm);
                }
            }
            Err(e) => {
                diags.push(Diagnostic::warn(
                    VERIFIER_PMID,
                    Target::Database,
                    "online_lookup_failed",
                    format!("E-utils lookup failed: {e}"),
                ));
            }
        }
    }
    for pm in &pmids {
        if cache.contains(pm.as_str()) {
            diags.push(Diagnostic::info(
                VERIFIER_PMID,
                Target::Database,
                "ok",
                format!("PMID {pm} resolves"),
            ));
        } else {
            diags.push(Diagnostic::error(
                VERIFIER_PMID,
                Target::Database,
                "unknown_pmid",
                format!("PMID {pm} not in cache and --online not used"),
            ));
        }
    }
    let end = Utc::now();
    VerifierRun {
        summary: VerifierSummary::from_diagnostics(&diags),
        started_at: start,
        finished_at: end,
        diagnostics: diags,
        run_error: None,
    }
}

fn resolve_pmids_online(ids: &[String]) -> anyhow::Result<Vec<String>> {
    let query: String = ids.iter().map(String::as_str).collect::<Vec<_>>().join(",");
    let url = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id={query}&retmode=json"
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let resp = client
        .get(&url)
        .header("User-Agent", "gapsmith-db verify-proposals")
        .send()?
        .error_for_status()?;
    let body: serde_json::Value = resp.json()?;
    let mut resolved = Vec::new();
    if let Some(result) = body.get("result").and_then(|r| r.as_object()) {
        for pm in ids {
            if let Some(entry) = result.get(pm.as_str())
                && entry.get("error").is_none()
                && entry.get("title").is_some()
            {
                resolved.push(pm.clone());
            }
        }
    }
    Ok(resolved)
}

fn load_uniprot(path: &Path) -> anyhow::Result<HashSet<String>> {
    let mut f =
        File::open(path).with_context(|| format!("opening UniProt snapshot {}", path.display()))?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)?;
    let decoded = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&bytes[..]).read_to_end(&mut out)?;
        out
    } else {
        bytes
    };
    let v: serde_json::Value = serde_json::from_slice(&decoded)
        .with_context(|| format!("parsing UniProt snapshot {}", path.display()))?;
    let mut out = HashSet::new();
    if let Some(results) = v.get("results").and_then(|r| r.as_array()) {
        for e in results {
            if let Some(a) = e.get("primaryAccession").and_then(|a| a.as_str()) {
                out.insert(a.to_string());
            }
            if let Some(secs) = e.get("secondaryAccessions").and_then(|s| s.as_array()) {
                for s in secs {
                    if let Some(s) = s.as_str() {
                        out.insert(s.to_string());
                    }
                }
            }
        }
    }
    Ok(out)
}

fn load_intenz(path: &Path) -> anyhow::Result<HashSet<String>> {
    let f = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let reader = BufReader::new(f);
    let mut out = HashSet::new();
    for line in reader.lines() {
        let line = line?;
        if let Some(rest) = line.strip_prefix("ID   ") {
            let ec = rest.split_whitespace().next().unwrap_or("").trim();
            if ec
                .split('.')
                .all(|part| part.parse::<u32>().is_ok() || part == "-")
                && ec.split('.').count() == 4
            {
                out.insert(ec.to_string());
            }
        }
    }
    Ok(out)
}

fn load_pmid_cache(path: &Path) -> anyhow::Result<HashSet<String>> {
    let bytes = std::fs::read(path)?;
    let v: serde_json::Value = serde_json::from_slice(&bytes)?;
    let mut out = HashSet::new();
    match v {
        serde_json::Value::Array(a) => {
            for x in a {
                if let Some(s) = x.as_str() {
                    out.insert(s.to_string());
                }
            }
        }
        serde_json::Value::Object(o) => {
            for (k, _) in o {
                out.insert(k);
            }
        }
        _ => warn!("unexpected PMID cache shape; expected list or object"),
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn severity_parser() {
        assert!(matches!(parse_severity("error").unwrap(), Severity::Error));
        assert!(matches!(parse_severity("WARN").unwrap(), Severity::Warning));
        assert!(parse_severity("nonsense").is_err());
    }

    #[test]
    fn intenz_parser_picks_up_four_level_ids() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            tmp.path(),
            "ID   1.1.1.1\nDE  alcohol dehydrogenase\n//\nID   2.7.-.-\n",
        )
        .unwrap();
        let set = load_intenz(tmp.path()).unwrap();
        assert!(set.contains("1.1.1.1"));
        // 2.7.-.- has wildcards, still 4-part → accepted
        assert!(set.contains("2.7.-.-"));
    }
}

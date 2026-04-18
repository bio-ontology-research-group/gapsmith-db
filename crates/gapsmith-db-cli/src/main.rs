//! gapsmith-db command-line entry point.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod curate_cmd;
mod fetch_cmd;
mod ingest_cmd;
mod propose_catalogue_cmd;
mod propose_cmd;
mod release_cmd;
mod retrieval_factory;
mod universal_cmd;
mod verify_cmd;
mod verify_proposals_cmd;

#[derive(Parser, Debug)]
#[command(
    name = "gapsmith-db",
    version,
    about = "Open, licence-clean metabolic pathway curation pipeline."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Fetch pinned upstream data sources into `data/<source>/`.
    Fetch(FetchArgs),
    /// Ingest fetched data into the canonical schema.
    Ingest(IngestArgs),
    /// Run deterministic verifiers over the DB.
    Verify(VerifyArgs),
    /// Run proposal-local verifiers (EC, UniProt, PMID) against
    /// proposals/pending/. Doesn't need the ingested DB.
    VerifyProposals(verify_proposals_cmd::VerifyProposalsArgs),
    /// Run the LLM proposer (or the Phase-4 mock).
    Propose(ProposeArgs),
    /// Batch-run the proposer over a pathway-name catalogue (TSV seed).
    ProposeCatalogue(propose_catalogue_cmd::ProposeCatalogueArgs),
    /// Curator tools: list/show/accept/reject/log/verify-chain.
    Curate(CurateArgs),
    /// Build a release tarball (TSV + binary DB + MANIFEST + RECEIPT).
    Release(ReleaseArgs),
    /// Universal SBML model: build / pin-atp-cycle / check-atp-cycle.
    Universal(universal_cmd::UniversalArgs),
}

#[derive(clap::Args, Debug)]
pub struct CurateArgs {
    #[command(subcommand)]
    pub action: CurateAction,
}

#[derive(Subcommand, Debug)]
pub enum CurateAction {
    List(CurateListArgs),
    Show(CurateShowArgs),
    Accept(CurateDecideArgs),
    Reject(CurateDecideArgs),
    Log(CurateLogArgs),
    VerifyChain(CurateVerifyArgs),
}

#[derive(clap::Args, Debug)]
pub struct CurateListArgs {
    #[arg(long, default_value = "proposals")]
    pub proposals_dir: PathBuf,
    #[arg(long)]
    pub state: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct CurateShowArgs {
    pub id: String,
    #[arg(long, default_value = "proposals")]
    pub proposals_dir: PathBuf,
    #[arg(long)]
    pub db: Option<PathBuf>,
}

#[derive(clap::Args, Debug)]
pub struct CurateDecideArgs {
    pub id: String,
    #[arg(long, default_value = "proposals")]
    pub proposals_dir: PathBuf,
    #[arg(long)]
    pub curator: String,
    #[arg(long)]
    pub comment: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct CurateLogArgs {
    #[arg(long, default_value = "proposals")]
    pub proposals_dir: PathBuf,
    #[arg(long)]
    pub limit: Option<usize>,
}

#[derive(clap::Args, Debug)]
pub struct CurateVerifyArgs {
    #[arg(long, default_value = "proposals")]
    pub proposals_dir: PathBuf,
}

#[derive(clap::Args, Debug)]
pub struct ReleaseArgs {
    #[arg(long)]
    pub db: PathBuf,
    #[arg(long)]
    pub tsv_dir: PathBuf,
    #[arg(long, default_value = "data")]
    pub data_root: PathBuf,
    #[arg(long, default_value = "proposals")]
    pub proposals_dir: PathBuf,
    #[arg(long)]
    pub out: PathBuf,
    #[arg(long)]
    pub version: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct IngestArgs {
    /// Root of the data tree containing per-source subdirectories.
    #[arg(long, default_value = "data")]
    pub data_root: PathBuf,
    /// Emit human-diffable TSV tables to this directory.
    #[arg(long)]
    pub out_tsv: Option<PathBuf>,
    /// Emit compact bincode binary to this file.
    #[arg(long)]
    pub out_binary: Option<PathBuf>,
}

#[derive(clap::Args, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct VerifyArgs {
    /// Path to the bincode DB produced by `gapsmith-db ingest`.
    #[arg(long)]
    pub db: PathBuf,
    /// Restrict the run to these verifier names (default: all).
    #[arg(long)]
    pub only: Vec<String>,
    /// Write the full JSON report here instead of stdout.
    #[arg(long)]
    pub report: Option<PathBuf>,
    /// Do not exit non-zero on Error-severity diagnostics.
    #[arg(long)]
    pub allow_errors: bool,

    /// IntEnz enzyme.dat flat file.
    #[arg(long)]
    pub intenz_dat: Option<PathBuf>,
    /// Swiss-Prot JSON snapshot (from the UniProt fetcher).
    #[arg(long)]
    pub uniprot_snapshot: Option<PathBuf>,
    /// PMID cache file (JSON list or `{pmid: ...}` object).
    #[arg(long)]
    pub pmid_cache: Option<PathBuf>,
    /// Look up missing PMIDs via E-utilities.
    #[arg(long)]
    pub pmid_online: bool,

    /// Universal SBML model for FBA verifiers.
    #[arg(long)]
    pub universal_model: Option<PathBuf>,
    /// Medium JSON for PathwayFluxTest.
    #[arg(long)]
    pub medium: Option<PathBuf>,
    /// Tolerance for AtpCycleTest; default 1e-6.
    #[arg(long)]
    pub atp_epsilon: Option<f64>,

    /// Write the DL consistency signature to this Turtle-ish file.
    #[arg(long)]
    pub dl_signature_out: Option<PathBuf>,

    /// Python bridge project directory (containing pyproject.toml).
    #[arg(long, default_value = "python")]
    pub python_project: PathBuf,
}

#[derive(clap::Args, Debug)]
pub struct ProposeArgs {
    /// Use the fixture-backed mock proposer (Phase-4 default).
    #[arg(long)]
    pub mock: bool,

    /// OpenRouter model slug (required when --mock is not set).
    #[arg(long)]
    pub model: Option<String>,

    /// Target pathway description.
    #[arg(long)]
    pub query: Option<String>,

    /// Organism scope hint.
    #[arg(long)]
    pub organism: Option<String>,

    /// Medium description (free text).
    #[arg(long)]
    pub medium: Option<String>,

    /// Prompt template path.
    #[arg(long, default_value = "prompts/pathway_proposal.md")]
    pub prompt: PathBuf,

    /// Fixture directory (used with --mock).
    #[arg(long, default_value = "proposals/fixtures")]
    pub fixture_dir: PathBuf,

    /// Proposals output root (pending/, rejected/, for_curation/).
    #[arg(long, default_value = "proposals")]
    pub proposals_dir: PathBuf,

    #[command(flatten)]
    pub retrieval: retrieval_factory::RetrievalArgs,
}

#[derive(clap::Args, Debug)]
#[allow(clippy::struct_excessive_bools)]
struct FetchArgs {
    /// Source to fetch. Default: every declared source except KEGG.
    #[arg(long)]
    source: Option<String>,

    /// Root of the data tree (default: ./data).
    #[arg(long, default_value = "data")]
    data_root: PathBuf,

    /// Cache directory for conditional-GET bookkeeping.
    #[arg(long, default_value = ".cache/http")]
    cache_root: PathBuf,

    /// Print the plan but do not touch the network or disk.
    #[arg(long)]
    dry_run: bool,

    /// Re-hash files already on disk and compare to `SOURCE.toml` pins.
    #[arg(long)]
    verify_only: bool,

    /// Global offline switch. Equivalent to GAPSMITH_OFFLINE=1.
    #[arg(long)]
    offline: bool,

    /// Allow fetching sources whose SOURCE.toml has no pin yet; the engine
    /// prints the computed hash for the maintainer to commit.
    #[arg(long)]
    accept_first_run: bool,

    /// Gate for KEGG. See LICENSING.md and plan.md.
    #[arg(long)]
    i_have_a_kegg_licence: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Fetch(args) => fetch_cmd::run(args).await,
        Command::Ingest(args) => ingest_cmd::run(args),
        Command::Verify(args) => verify_cmd::run(args),
        Command::VerifyProposals(args) => verify_proposals_cmd::run(args),
        Command::Propose(args) => propose_cmd::run(args),
        Command::ProposeCatalogue(args) => propose_catalogue_cmd::run(args),
        Command::Curate(args) => curate_cmd::run(args),
        Command::Release(args) => release_cmd::run(args),
        Command::Universal(args) => universal_cmd::run(args),
    }
}

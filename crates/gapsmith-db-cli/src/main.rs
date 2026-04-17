//! gapsmith-db command-line entry point.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod fetch_cmd;

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
    /// Ingest fetched data into the canonical schema (Phase 2).
    Ingest,
    /// Run deterministic verifiers over the DB (Phase 3).
    Verify,
    /// Run the LLM proposer (Phase 4).
    Propose,
    /// Curator CLI: list, diff, accept/reject proposals (Phase 5).
    Curate,
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
        Command::Ingest | Command::Verify | Command::Propose | Command::Curate => {
            tracing::warn!("subcommand not yet implemented (see plan.md)");
            Ok(())
        }
    }
}

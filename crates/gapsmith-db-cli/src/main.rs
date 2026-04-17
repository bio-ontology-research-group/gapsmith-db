//! gapsmith-db command-line entry point.
//!
//! Subcommands are wired in later phases. Phase 0 provides a runnable binary
//! and confirms the workspace compiles end-to-end.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "gapsmith-db",
    version,
    about = "Open, licence-clean metabolic pathway curation pipeline."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    Fetch,
    Ingest,
    Verify,
    Propose,
    Curate,
}

// The bin grows fallible subcommands in later phases; the return type is
// intentionally `Result` now so the signature is stable.
#[allow(clippy::unnecessary_wraps)]
fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        None => {
            println!(
                "gapsmith-db {} — phase 0 scaffold",
                env!("CARGO_PKG_VERSION")
            );
            Ok(())
        }
        Some(cmd) => {
            tracing::warn!(?cmd, "subcommand not yet implemented (see plan.md)");
            Ok(())
        }
    }
}

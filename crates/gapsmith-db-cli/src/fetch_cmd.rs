//! `gapsmith-db fetch` subcommand.

use std::path::Path;

use anyhow::Context;
use gapsmith_db_ingest::fetch::{FetchContext, PinStatus, render_plan};
use gapsmith_db_ingest::http::{HttpClient, HttpOptions, offline_from_env};
use gapsmith_db_ingest::source::{SourceId, SourceSpec};
use gapsmith_db_ingest::sources::build_plan;
use tracing::{error, info, warn};

use crate::FetchArgs;

pub async fn run(args: FetchArgs) -> anyhow::Result<()> {
    let offline = args.offline || offline_from_env();
    let opts = HttpOptions::new(args.cache_root.clone()).offline(offline);
    let http = HttpClient::new(opts).context("building HTTP client")?;

    let ctx = FetchContext {
        http,
        data_root: args.data_root.clone(),
        dry_run: args.dry_run,
        verify_only: args.verify_only,
        accept_first_run: args.accept_first_run,
        kegg_acknowledged: args.i_have_a_kegg_licence,
    };

    let sources = select_sources(args.source.as_deref(), args.i_have_a_kegg_licence)?;
    let mut failures = 0_u32;

    for id in sources {
        match process(id, &ctx, &args.data_root).await {
            Ok(()) => {}
            Err(e) => {
                error!(source = %id, error = %e, "fetch failed");
                failures += 1;
            }
        }
    }

    if failures > 0 {
        anyhow::bail!("{failures} source(s) failed");
    }
    Ok(())
}

async fn process(id: SourceId, ctx: &FetchContext, data_root: &Path) -> anyhow::Result<()> {
    let spec =
        SourceSpec::load(data_root, id).with_context(|| format!("loading SOURCE.toml for {id}"))?;
    let plan = match build_plan(id, &spec, ctx) {
        Ok(p) => p,
        Err(gapsmith_db_ingest::IngestError::KeggGated) => {
            warn!(source = %id, "skipping KEGG (requires --i-have-a-kegg-licence)");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    if ctx.dry_run {
        print!("{}", render_plan(&plan, &spec, ctx));
        println!();
        return Ok(());
    }

    let outcome = gapsmith_db_ingest::fetch::execute(plan, &spec, ctx).await?;
    match &outcome.pin_status {
        PinStatus::Verified => info!(source = %id, "verified against pinned sha256"),
        PinStatus::RecordedFirstHash { sha256 } => info!(
            source = %id,
            sha256 = %sha256,
            "first run — commit this sha256 to SOURCE.toml",
        ),
        PinStatus::Unpinned { sha256, .. } => warn!(
            source = %id,
            sha256 = %sha256,
            "source has no pin — record pin + hash in SOURCE.toml",
        ),
    }
    for step in &outcome.steps {
        info!(
            source = %id,
            label = %step.label,
            sha256 = %step.sha256,
            wrote = step.bytes_written,
            target = %step.target.display(),
        );
    }
    Ok(())
}

fn select_sources(filter: Option<&str>, kegg: bool) -> anyhow::Result<Vec<SourceId>> {
    if let Some(name) = filter {
        let id =
            SourceId::parse(name).map_err(|e| anyhow::anyhow!("unknown --source {name}: {e}"))?;
        return Ok(vec![id]);
    }
    // Default: everything but KEGG unless the gate is unlocked.
    Ok(SourceId::ALL
        .iter()
        .copied()
        .filter(|id| kegg || *id != SourceId::Kegg)
        .collect())
}

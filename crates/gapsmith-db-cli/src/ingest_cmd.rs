//! `gapsmith-db ingest` subcommand.

use std::path::Path;

use anyhow::Context;
use gapsmith_db_core::serde_io;
use gapsmith_db_ingest::merge::merge;
use gapsmith_db_ingest::parse;
use tracing::{info, warn};

use crate::IngestArgs;

type ParseFn = fn(&Path) -> gapsmith_db_ingest::Result<parse::IngestBundle>;

pub fn run(args: IngestArgs) -> anyhow::Result<()> {
    let data_root = args.data_root;
    let mut bundles = Vec::new();

    let sources: &[(&str, ParseFn)] = &[
        ("chebi", parse::chebi::parse_dir),
        ("modelseed", parse::modelseed::parse_dir),
        ("mnxref", parse::mnxref::parse_dir),
        ("rhea", parse::rhea::parse_dir),
        ("gapseq", parse::gapseq::parse_dir),
    ];

    for (name, parser) in sources.iter().copied() {
        let dir = data_root.join(name);
        if !dir.exists() {
            warn!(%name, "ingest: source directory missing, skipping");
            continue;
        }
        let bundle = parser(&dir).with_context(|| format!("parsing source {name}"))?;
        info!(
            %name,
            compounds = bundle.compounds.len(),
            reactions = bundle.reactions.len(),
            xrefs = bundle.compound_xrefs.len() + bundle.reaction_xrefs.len(),
            "ingest: parsed"
        );
        bundles.push(bundle);
    }

    let db = merge(&bundles);
    db.validate()
        .with_context(|| "database invariant check after merge")?;
    let stats = db.stats();
    info!(
        compounds = stats.compounds,
        reactions = stats.reactions,
        pathways = stats.pathways,
        with_inchikey = stats.compounds_with_inchikey,
        "ingest: merged"
    );

    if let Some(ref p) = args.out_binary {
        serde_io::write_binary(&db, p)
            .with_context(|| format!("writing binary to {}", p.display()))?;
        info!(path = %p.display(), "wrote binary DB");
    }
    if let Some(ref d) = args.out_tsv {
        serde_io::write_tsv_dir(&db, d)
            .with_context(|| format!("writing TSV to {}", d.display()))?;
        info!(dir = %d.display(), "wrote TSV tables");
    }
    Ok(())
}

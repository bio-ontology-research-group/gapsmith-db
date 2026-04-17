//! MNXref — pinned to a release; four TSVs fetched individually.

use std::path::PathBuf;

use crate::Result;
use crate::fetch::{ExtractMode, FetchPlan, FetchStep};
use crate::source::{SourceId, SourceSpec};

const FILES: &[&str] = &[
    "chem_xref.tsv",
    "reac_xref.tsv",
    "chem_prop.tsv",
    "reac_prop.tsv",
];

pub fn plan(spec: &SourceSpec, dry_run: bool) -> Result<FetchPlan> {
    let release = spec.require_release(dry_run)?;

    // Per-file hashes live in the `[file_hashes]` table of SOURCE.toml.
    let steps = FILES
        .iter()
        .map(|name| FetchStep {
            url: format!("https://www.metanetx.org/ftp/{release}/{name}"),
            relative_path: PathBuf::from(name),
            expected_sha256: spec.file_hash(name).map(str::to_string),
            extract: ExtractMode::Raw,
            label: (*name).to_string(),
        })
        .collect();

    Ok(FetchPlan {
        source: SourceId::Mnxref,
        version_label: format!("release={release}"),
        steps,
    })
}

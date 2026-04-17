//! Reactome — pathway hierarchy + compound/protein cross-refs.

use std::path::PathBuf;

use crate::Result;
use crate::fetch::{ExtractMode, FetchPlan, FetchStep};
use crate::source::{SourceId, SourceSpec};

const FILES: &[&str] = &[
    "ReactomePathways.txt",
    "ReactomePathwaysRelation.txt",
    "ChEBI2Reactome_All_Levels.txt",
    "UniProt2Reactome_All_Levels.txt",
];

pub fn plan(spec: &SourceSpec, dry_run: bool) -> Result<FetchPlan> {
    let release = spec.require_release(dry_run)?;

    let base = format!("https://reactome.org/download/{release}");
    let steps = FILES
        .iter()
        .map(|name| FetchStep {
            url: format!("{base}/{name}"),
            relative_path: PathBuf::from(name),
            expected_sha256: spec.file_hash(name).map(str::to_string),
            extract: ExtractMode::Raw,
            label: (*name).to_string(),
        })
        .collect();

    Ok(FetchPlan {
        source: SourceId::Reactome,
        version_label: format!("release={release}"),
        steps,
    })
}

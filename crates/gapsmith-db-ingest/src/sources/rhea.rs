//! Rhea — pinned to a release number. TSV tables + ChEBI and EC mappings.

use std::path::PathBuf;

use crate::Result;
use crate::fetch::{ExtractMode, FetchPlan, FetchStep};
use crate::source::{SourceId, SourceSpec};

const TSV_FILES: &[&str] = &[
    "rhea-reactions.tsv",
    "rhea2ec.tsv",
    "rhea2uniprot_sprot.tsv",
    "rhea-directions.tsv",
];

pub fn plan(spec: &SourceSpec, dry_run: bool) -> Result<FetchPlan> {
    let release = spec.require_release(dry_run)?;

    let base = format!("https://ftp.expasy.org/databases/rhea/{release}");
    let mut steps: Vec<FetchStep> = TSV_FILES
        .iter()
        .map(|name| FetchStep {
            url: format!("{base}/tsv/{name}"),
            relative_path: PathBuf::from(name),
            expected_sha256: None,
            extract: ExtractMode::Raw,
            label: (*name).to_string(),
        })
        .collect();

    steps.push(FetchStep {
        url: format!("{base}/rdf/rhea.rdf.gz"),
        relative_path: PathBuf::from("rhea.rdf"),
        expected_sha256: spec.pinned_hash().map(str::to_string),
        extract: ExtractMode::Gzip,
        label: "rhea.rdf.gz".into(),
    });

    Ok(FetchPlan {
        source: SourceId::Rhea,
        version_label: format!("release={release}"),
        steps,
    })
}

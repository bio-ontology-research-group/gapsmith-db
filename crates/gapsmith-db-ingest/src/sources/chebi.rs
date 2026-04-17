//! ChEBI — pinned to a release number. Flat files + OBO ontology.

use std::path::PathBuf;

use crate::Result;
use crate::fetch::{ExtractMode, FetchPlan, FetchStep};
use crate::source::{SourceId, SourceSpec};

pub fn plan(spec: &SourceSpec, dry_run: bool) -> Result<FetchPlan> {
    let release = spec.require_release(dry_run)?;

    let archive = format!("https://ftp.ebi.ac.uk/pub/databases/chebi/archive/rel{release}");

    let steps = vec![
        FetchStep {
            url: format!("{archive}/Flat_file_tab_delimited/compounds.tsv.gz"),
            relative_path: PathBuf::from("compounds.tsv"),
            expected_sha256: spec.file_hash("compounds.tsv.gz").map(str::to_string),
            extract: ExtractMode::Gzip,
            label: "compounds.tsv.gz".into(),
        },
        FetchStep {
            url: format!("{archive}/Flat_file_tab_delimited/chemical_data.tsv"),
            relative_path: PathBuf::from("chemical_data.tsv"),
            expected_sha256: spec.file_hash("chemical_data.tsv").map(str::to_string),
            extract: ExtractMode::Raw,
            label: "chemical_data.tsv".into(),
        },
        FetchStep {
            url: format!("{archive}/Flat_file_tab_delimited/names.tsv.gz"),
            relative_path: PathBuf::from("names.tsv"),
            expected_sha256: spec.file_hash("names.tsv.gz").map(str::to_string),
            extract: ExtractMode::Gzip,
            label: "names.tsv.gz".into(),
        },
        FetchStep {
            url: format!("{archive}/ontology/chebi.obo.gz"),
            relative_path: PathBuf::from("chebi.obo"),
            expected_sha256: spec.file_hash("chebi.obo.gz").map(str::to_string),
            extract: ExtractMode::Gzip,
            label: "chebi.obo.gz".into(),
        },
    ];

    Ok(FetchPlan {
        source: SourceId::Chebi,
        version_label: format!("release={release}"),
        steps,
    })
}

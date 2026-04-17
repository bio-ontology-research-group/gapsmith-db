//! Rhea — pinned to a release number. TSV tables + ChEBI and EC mappings.

use std::path::PathBuf;

use crate::Result;
use crate::fetch::{ExtractMode, FetchPlan, FetchStep};
use crate::source::{SourceId, SourceSpec};

// Rhea used to ship `rhea-reactions.tsv`; since release ~135 the human-
// readable equation table has been dropped in favour of the RDF release.
// The Phase-2 parser only needs rhea2ec + rhea-directions (plus uniprot
// and reactome mappings for xref-building).
const TSV_FILES: &[&str] = &[
    "rhea2ec.tsv",
    "rhea2uniprot_sprot.tsv",
    "rhea-directions.tsv",
    "rhea2reactome.tsv",
];

pub fn plan(spec: &SourceSpec, dry_run: bool) -> Result<FetchPlan> {
    // The Rhea release number is metadata only; the FTP layout serves a
    // single live snapshot at /databases/rhea/{tsv,rdf}/ with no release
    // number in the path. Per-file sha256 pins in SOURCE.toml tie a build
    // to a specific snapshot.
    let release = spec.require_release(dry_run)?;

    let base = "https://ftp.expasy.org/databases/rhea";
    let mut steps: Vec<FetchStep> = TSV_FILES
        .iter()
        .map(|name| FetchStep {
            url: format!("{base}/tsv/{name}"),
            relative_path: PathBuf::from(name),
            expected_sha256: spec.file_hash(name).map(str::to_string),
            extract: ExtractMode::Raw,
            label: (*name).to_string(),
        })
        .collect();

    steps.push(FetchStep {
        url: format!("{base}/rdf/rhea.rdf.gz"),
        relative_path: PathBuf::from("rhea.rdf"),
        expected_sha256: spec.file_hash("rhea.rdf.gz").map(str::to_string),
        extract: ExtractMode::Gzip,
        label: "rhea.rdf.gz".into(),
    });

    Ok(FetchPlan {
        source: SourceId::Rhea,
        version_label: format!("release={release}"),
        steps,
    })
}

//! IntEnz — EC nomenclature XML + flat file release.

use std::path::PathBuf;

use crate::Result;
use crate::fetch::{ExtractMode, FetchPlan, FetchStep};
use crate::source::{SourceId, SourceSpec};

pub fn plan(spec: &SourceSpec, dry_run: bool) -> Result<FetchPlan> {
    // IntEnz publishes to a single "current" FTP path; tag the release in
    // SOURCE.toml by release number or date.
    let (tag, kind) = spec.require_release_or_date(dry_run)?;
    let base = "https://ftp.ebi.ac.uk/pub/databases/intenz";
    let steps = vec![
        FetchStep {
            url: format!("{base}/xml/intenz.xml.gz"),
            relative_path: PathBuf::from("intenz.xml"),
            expected_sha256: spec.pinned_hash().map(str::to_string),
            extract: ExtractMode::Gzip,
            label: "intenz.xml.gz".into(),
        },
        FetchStep {
            url: format!("{base}/flat/enzyme.dat"),
            relative_path: PathBuf::from("enzyme.dat"),
            expected_sha256: None,
            extract: ExtractMode::Raw,
            label: "enzyme.dat".into(),
        },
    ];

    Ok(FetchPlan {
        source: SourceId::Intenz,
        version_label: format!("{kind}={tag}"),
        steps,
    })
}

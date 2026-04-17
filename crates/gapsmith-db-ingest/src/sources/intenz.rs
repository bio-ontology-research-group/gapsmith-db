//! ENZYME (ExPASy) — EC nomenclature flat files.
//!
//! The EBI IntEnz FTP mirror
//! (ftp.ebi.ac.uk/pub/databases/intenz/) was last updated in 2022 and now
//! returns 404/403 for its flat files. ExPASy hosts the live IUBMB ENZYME
//! nomenclature at ftp.expasy.org/databases/enzyme/; the data is equivalent
//! from the EcValidity verifier's perspective.
//!
//! The SourceId is still `Intenz` for backwards compatibility with the
//! workspace-wide source enum.

use std::path::PathBuf;

use crate::Result;
use crate::fetch::{ExtractMode, FetchPlan, FetchStep};
use crate::source::{SourceId, SourceSpec};

pub fn plan(spec: &SourceSpec, dry_run: bool) -> Result<FetchPlan> {
    let (tag, kind) = spec.require_release_or_date(dry_run)?;
    let base = "https://ftp.expasy.org/databases/enzyme";
    let steps = vec![
        FetchStep {
            url: format!("{base}/enzyme.dat"),
            relative_path: PathBuf::from("enzyme.dat"),
            expected_sha256: spec.file_hash("enzyme.dat").map(str::to_string),
            extract: ExtractMode::Raw,
            label: "enzyme.dat".into(),
        },
        FetchStep {
            url: format!("{base}/enzclass.txt"),
            relative_path: PathBuf::from("enzclass.txt"),
            expected_sha256: spec.file_hash("enzclass.txt").map(str::to_string),
            extract: ExtractMode::Raw,
            label: "enzclass.txt".into(),
        },
    ];

    Ok(FetchPlan {
        source: SourceId::Intenz,
        version_label: format!("{kind}={tag}"),
        steps,
    })
}

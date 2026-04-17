//! ModelSEEDDatabase — pinned to a Git commit; we fetch the commit tarball
//! and let the engine extract it. Only three files feed ingestion, but we
//! keep the whole archive for reproducibility (its sha256 is the pin).

use std::path::PathBuf;

use crate::Result;
use crate::fetch::{ExtractMode, FetchPlan, FetchStep};
use crate::source::{SourceId, SourceSpec};

pub fn plan(spec: &SourceSpec, dry_run: bool) -> Result<FetchPlan> {
    let commit = spec.require_commit(dry_run)?;
    let url = format!("https://github.com/ModelSEED/ModelSEEDDatabase/archive/{commit}.tar.gz");
    let step = FetchStep {
        url,
        relative_path: PathBuf::from(format!("ModelSEEDDatabase-{commit}")),
        expected_sha256: spec.pinned_hash().map(str::to_string),
        extract: ExtractMode::TarGz,
        label: "ModelSEEDDatabase archive".into(),
    };
    Ok(FetchPlan {
        source: SourceId::Modelseed,
        version_label: format!("commit={commit}"),
        steps: vec![step],
    })
}

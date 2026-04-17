//! gapseq `dat/` — pinned to a Git commit. GPL-3.0-or-later (noted in
//! SOURCE.toml). We fetch the repository tarball and extract it.

use std::path::PathBuf;

use crate::Result;
use crate::fetch::{ExtractMode, FetchPlan, FetchStep};
use crate::source::{SourceId, SourceSpec};

pub fn plan(spec: &SourceSpec, dry_run: bool) -> Result<FetchPlan> {
    let commit = spec.require_commit(dry_run)?;
    let url = format!("https://github.com/jotech/gapseq/archive/{commit}.tar.gz");
    let step = FetchStep {
        url,
        relative_path: PathBuf::from(format!("gapseq-{commit}")),
        expected_sha256: spec.pinned_hash().map(str::to_string),
        extract: ExtractMode::TarGz,
        label: "gapseq archive".into(),
    };

    Ok(FetchPlan {
        source: SourceId::Gapseq,
        version_label: format!("commit={commit}"),
        steps: vec![step],
    })
}

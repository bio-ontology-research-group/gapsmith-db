//! UniProtKB/Swiss-Prot — EC-annotated reviewed subset via the REST API.
//!
//! UniProt exposes a cursor-paginated search endpoint. The Phase-1 plan
//! emits a single step pointing at page 1 and targets `swissprot_ec.json`.
//! When the cursor-walk is implemented it concatenates additional pages
//! into the same file (one big JSON; per user preference).

use std::path::PathBuf;

use crate::Result;
use crate::fetch::{ExtractMode, FetchPlan, FetchStep};
use crate::source::{SourceId, SourceSpec};

const PAGE_SIZE: u32 = 500;

pub fn plan(spec: &SourceSpec, dry_run: bool) -> Result<FetchPlan> {
    let release = spec.require_release(dry_run)?;
    let query = spec.query.as_deref().unwrap_or("reviewed:true AND ec:*");

    let encoded = urlencoding::encode(query);
    let url = format!(
        "https://rest.uniprot.org/uniprotkb/search?query={encoded}&format=json&size={PAGE_SIZE}"
    );

    let step = FetchStep {
        url,
        relative_path: PathBuf::from("swissprot_ec.json"),
        expected_sha256: spec.file_hash("swissprot_ec.json").map(str::to_string),
        extract: ExtractMode::Raw,
        label: "swissprot_ec.json (page 1; cursor-walk concatenated into one file)".into(),
    };

    Ok(FetchPlan {
        source: SourceId::Uniprot,
        version_label: format!("release={release}"),
        steps: vec![step],
    })
}

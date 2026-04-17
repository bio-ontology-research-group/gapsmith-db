//! UniProtKB/Swiss-Prot — EC-annotated reviewed subset via the REST API.
//!
//! UniProt exposes a cursor-paginated search endpoint. Shard URLs are
//! generated from the cursor returned in the `Link` header. For the
//! Phase-1 scaffold we emit a single step that points at page 1; the
//! actual cursor walk is implemented when the ingest pipeline is wired.

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
        relative_path: PathBuf::from("shards/page_001.json"),
        expected_sha256: None,
        extract: ExtractMode::Raw,
        label: "uniprot page 1".into(),
    };

    Ok(FetchPlan {
        source: SourceId::Uniprot,
        version_label: format!("release={release}"),
        steps: vec![step],
    })
}

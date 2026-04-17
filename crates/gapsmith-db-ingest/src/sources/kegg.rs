//! KEGG — gated stub. KEGG REST terms restrict bulk automated pulls; the
//! fetcher is never exercised in CI and KEGG-derived content is never
//! included in a public release artefact. Reaching this module requires
//! `--i-have-a-kegg-licence`; see plan.md.

use crate::fetch::FetchPlan;
use crate::source::SourceSpec;
use crate::{IngestError, Result};

pub fn plan(_spec: &SourceSpec, _dry_run: bool) -> Result<FetchPlan> {
    Err(IngestError::Other(
        "KEGG auto-fetch is intentionally not implemented. Populate data/kegg/ manually \
         in accordance with your KEGG licence, then re-run with ingest (not fetch)."
            .into(),
    ))
}

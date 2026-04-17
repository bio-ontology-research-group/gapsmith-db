//! Per-source plan builders. Each module exposes a `plan` function that
//! consumes a [`SourceSpec`] and returns a [`FetchPlan`]. The fetch engine
//! in [`crate::fetch`] is source-agnostic.

use crate::fetch::{FetchContext, FetchPlan};
use crate::source::{SourceId, SourceSpec};
use crate::{IngestError, Result};

pub mod chebi;
pub mod gapseq;
pub mod intenz;
pub mod kegg;
pub mod mnxref;
pub mod modelseed;
pub mod reactome;
pub mod rhea;
pub mod uniprot;

/// Build the plan for `id`. Returns an error if the source is gated and the
/// gate has not been unlocked (currently: KEGG). In dry-run mode, sources
/// with missing pins return a plan with `<PIN_TBD>` placeholders so the
/// plan template is still printable.
pub fn build_plan(id: SourceId, spec: &SourceSpec, ctx: &FetchContext) -> Result<FetchPlan> {
    let dry = ctx.dry_run;
    match id {
        SourceId::Modelseed => modelseed::plan(spec, dry),
        SourceId::Mnxref => mnxref::plan(spec, dry),
        SourceId::Rhea => rhea::plan(spec, dry),
        SourceId::Chebi => chebi::plan(spec, dry),
        SourceId::Intenz => intenz::plan(spec, dry),
        SourceId::Uniprot => uniprot::plan(spec, dry),
        SourceId::Reactome => reactome::plan(spec, dry),
        SourceId::Gapseq => gapseq::plan(spec, dry),
        SourceId::Kegg => {
            if !ctx.kegg_acknowledged {
                return Err(IngestError::KeggGated);
            }
            kegg::plan(spec, dry)
        }
    }
}

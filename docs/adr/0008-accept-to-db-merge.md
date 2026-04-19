# ADR 0008: Accept-to-DB merge path for LLM proposals

- **Status**: accepted
- **Date**: 2026-04-19
- **Phase**: 5 (follow-up)

## Context

Before this change, `gapsmith-db curate accept <proposal-id>` recorded
a decision-log entry and moved the proposal JSON to
`proposals/decisions/` — but nothing reached the canonical
`Database`. The 650 `for_curation/` proposals from the 2026-04-18
gpt-4o catalogue run were therefore a queue into nothing. The v0.1
release slice (see ROADMAP section d) explicitly requires that
accepted LLM proposals feed the same DB that upstream ingestion
populates.

The schema already had half of what's needed: `Evidence` carried an
optional `proposal_hash: Option<String>` and a `curator` field, so
provenance could be attributed per-claim. What it didn't have:

1. No `Source::LlmProposal` variant — accepted claims had no
   distinguishing tag.
2. No way to attach Swiss-Prot accessions to a reaction — the
   proposer-side `EnzymeRef` mapped `uniprot → [local_id]` with no
   corresponding fields on the core `Reaction`.
3. No way to preserve DAG edges — `Pathway.reactions` is an ordered
   list, which loses branches and alternate routes the proposer
   asserted explicitly.
4. No merge function — ingestion was batch-only (`merge(bundles) ->
   Database`).

## Decision

Additive schema extensions (no breakages for existing consumers):

- **`Source::LlmProposal`** — new enum variant. Every `Evidence`
  entry produced during a merge carries this source plus the
  `proposal_hash` and the curator name.
- **`Reaction::enzymes: Vec<String>`** — list of Swiss-Prot
  accessions asserted to catalyse this reaction. The verifier layer,
  not this field, decides whether each accession exists; the DB just
  records the claim.
- **`Pathway::dag: Vec<(ReactionId, ReactionId)>`** — explicit edges
  between reactions. Complements the ordered `reactions` list: for
  linear pathways `dag` is optional, but branched/cyclic ones need
  it.

New function: **`gapsmith_db_propose::merge_proposal(&mut Database,
&Proposal, curator) -> MergeReport`**. Resolution rules:

- `ReactionRef::Rhea(id)` → look up existing reaction by `rhea_id`
  or `xrefs[Rhea]`. Hit → reuse `ReactionId`; miss → synthesise a
  stub with the Rhea xref but **no stoichiometry**, and record a
  warning so the curator knows the Rhea tables should be re-ingested
  before trusting the pathway.
- `ReactionRef::ChebiEc` → always synthesise a new reaction (no
  canonical lookup key exists for an EC-plus-compound tuple).
  Substrate/product compounds resolved by ChEBI xref; missing
  compounds minted.
- Enzymes pushed onto the target reactions' `enzymes` field and
  cross-referenced via `xrefs[Uniprot]`.
- DAG edges translated from proposal local IDs to canonical
  `ReactionId`s and attached to the new `Pathway`.
- Citations become `Pathway.evidence` entries each with
  `citation: Some(pmid)`.

New CLI flag: **`gapsmith-db curate accept --merge-into
<db.gapsmith> [--tsv-out <dir>]`**. Runs the merge *after* the
decision-log append, so the chain head already references the accept
before DB mutation starts. If the merge fails, the decision is still
durable; the curator can retry with `--merge-into` against a fixed
DB.

## Consequences

- The v0.1 release slice (d.3 in ROADMAP) is no longer blocked on
  schema. Curator throughput is the only remaining gate.
- Rhea-miss stubs are load-bearing: when Rhea tables are re-ingested
  with `gapsmith-db ingest`, the existing `insert_reaction` call
  will *overwrite* the stub (same `ReactionId` collision). This
  needs care when merging Rhea ingestion with accepted LLM data —
  the LLM evidence should survive, the stoichiometry should come
  from Rhea. Revisit if we see corruption.
- `MergeReport.warnings` surfaces both the Rhea-miss case and DAG
  edges that reference unknown local IDs. The decision log doesn't
  store the report today; if we want "accept with warnings" to be
  auditable later, the report should be attached to the
  `Decision.metadata` field.
- The Swiss-Prot snapshot (798k accessions) is the authority for
  whether a given UniProt accession exists; the merge trusts the
  proposal's `enzymes` field blindly. Proposals with hallucinated
  accessions should be caught by `verify-proposals` *before* reaching
  `accept`.

## Alternatives considered

- **Build a new `Enzyme` type linking UniProt ↔ reactions.** Rejected
  for v0.1: the downstream gapsmith consumer reads `Reaction` and
  would need a contract change. A `Vec<String>` field is the
  smallest shape that carries the claim and can be promoted to a
  richer type later.
- **Require the curator to fetch + ingest Rhea before `accept`.**
  Rejected: the pipeline should route by evidence, not block on
  data availability. The warning + stub pattern lets the curator
  make progress and fix the ingestion gap later.
- **Merge synchronously inside the decision-log append.** Rejected:
  the chain-hash invariant must not depend on a DB write succeeding.
  Better to keep the log append atomic, then retry the merge
  idempotently.
- **Overwrite existing reactions when a proposal asserts them.**
  Rejected: existing upstream-ingested reactions carry
  formula/stoichiometry data we don't want to lose to a less-informed
  LLM claim. Link-only-on-hit preserves the upstream record and
  appends the LLM evidence.

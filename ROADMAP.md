# Roadmap

Status as of 2026-04-17 (post-Phase 5, first-real-fetch round).
Licence-clean by construction: no MetaCyc, no BioCyc
(see [`LICENSING.md`](./LICENSING.md)).

## (a) Implemented and reproducible now

- **Cargo workspace** with five crates: `gapsmith-db-{core, ingest, verify,
  propose, cli}` plus a Python subtree (`python/`, uv-managed, 3.12).
- **Data fetchers** (9 sources) with pinned commit/release + per-file SHA256,
  single shared HTTP client (retry + ETag cache + `GAPSMITH_OFFLINE`), and
  a hard gate on KEGG (`--i-have-a-kegg-licence`). Pins and sha256s are
  committed for modelseed, gapseq, rhea, chebi, reactome, uniprot, and
  intenz (now sourcing from ExPASy ENZYME — the EBI IntEnz mirror has
  been stale since 2022).
- **Canonical schema** (`gapsmith-db-core`): Compound, Reaction,
  Stoichiometry, Pathway, Evidence, EcNumber, Compartment, Reversibility,
  OrganismScope. Dual serialisation: human-diffable TSV + bincode binary
  with magic header. Property tests on invariants.
- **Ingestion** from ModelSEED + MNXref + Rhea (TSV) + ChEBI + gapseq
  corrections with three-tier compound dedup (InChIKey → MNXref → name,
  last-resort flagged) and reaction dedup (Rhea → SEED → source+native).
- **Verifier layer** (9 verifiers): atom balance (with hydrogen-only
  warning), charge balance, EC validity, UniProt existence, PMID existence
  (offline cache + `--online`), plus Python-bridged ΔG (eQuilibrator), ATP
  cycle, pathway flux. DL consistency emits a minimal OWL signature stub.
  All verifiers fail-closed; structured JSON report.
- **Proposer scaffold**: strict Proposal schema (Rhea | ChEBI+EC;
  UniProt; DAG; PMIDs), content-addressed `sha256:…`, OpenRouter client,
  fixture mock, in-memory retrieval, Qdrant HTTP stub, domain filter with
  subdomain matching, versioned prompt template.
- **Curator tooling**: `list / show / accept / reject / log / verify-chain`.
  Hash-chained decision log; tampering breaks the chain by design.
- **Release artefact**: `gapsmith-db release` emits a tarball with TSV +
  binary DB + MANIFEST.json + RECEIPT.json + CITATIONS.md + sidecar sha256.
- **CI** (GitHub Actions): fmt, clippy `-D warnings`, cargo test, licence-
  lint, ruff, python-bridge ping, end-to-end ingest → verify → universal
  SBML → ATP-cycle baseline check → propose → curate → release over a
  fixture. The licence-lint now scans only tracked files, so fetched
  upstream artefacts (which may contain banned-source xrefs as ID
  strings) don't trip it.
- **Universal SBML builder**: `gapsmith-db universal build` exports the
  ingested DB to an SBML Level 3 v1 + fbc v2 model via cobrapy. The
  builder can synthesise an `ATPM` reaction (`--add-atpm --atpm-ids ...`)
  so the ATP-cycle test always has a handle. Companion subcommands
  `universal pin-atp-cycle` and `universal check-atp-cycle` record and
  verify a regression pin at `verify/baselines/atp_cycle_*.json`.
- **Fixture proposals** for four first-release pathway families in
  `proposals/fixtures/`: methanogenesis (from Phase 4), glycolysis (EMP,
  10 steps), ethanol fermentation (2 steps), glutamate biosynthesis
  (GS/GOGAT). Each hand-authored with canonical Rhea IDs, Swiss-Prot
  enzyme accessions, DAG edges, and literature PMIDs.
- **Corpus ingest script**: `python/src/gapsmith_bridge/corpus_ingest.py`
  fetches Europe PMC OA papers, extracts paragraph-sized passages,
  embeds with sentence-transformers
  (`NeuML/pubmedbert-base-embeddings` default), and upserts into Qdrant.
  The embedder is invoked through the same Python bridge used by the
  verifiers (`--action embed`).
- **QdrantBackend** (`gapsmith-db-propose`): wired end-to-end — `search`
  embeds the query through the bridge, posts to
  `/collections/<name>/points/search`, maps payloads to `Passage`, and
  applies the `DomainFilter` as a belt-and-braces guard.

## (b) Stubbed (the shape is right; the innards are minimal)

- **DL consistency checker**: emits the OWL signature from ChEBI roles +
  a TODO diagnostic. Needs an OWL reasoner (ELK or HermiT via a small
  Python helper) and GO-BP integration.
- **Real universal metabolic model**: the CI fixture now exercises the
  universal builder + ATP-cycle regression end-to-end, but only on a
  toy 2-reaction DB. Once MNXref + ChEBI have been ingested at scale,
  build a production universal and re-pin. The baseline file at
  `verify/baselines/atp_cycle_ci_fixture.json` is the template.
- **MNXref fetch**: sizes are 1.5 GB+ across four files; the first
  fetch in-session hit "error decoding response body" twice, likely a
  CDN/connection reset on sustained long downloads. Pins in
  `data/mnxref/SOURCE.toml` are resolved (4.5) but per-file sha256s
  are still blank pending a successful run.
- **Rhea RDF ingest**: Phase 2 parses the TSV tables only; the RDF ship
  has the hook in `SOURCE.toml` but no parser.
- **UniProt cursor walk**: Phase 1 emits page 1 to `swissprot_ec.json`;
  subsequent pages need the cursor-walk implementation to concatenate.
- **IntEnz/ENZYME + Reactome ingestion into the canonical schema**
  (their parsers exist in `gapsmith-db-ingest::parse` but are not yet
  called from the default ingest pipeline).
- **Corpus + Qdrant deployment**: the ingest script and search path
  both exist and pass unit tests; no Qdrant collection has been
  populated in this session — requires the operator to stand up Qdrant
  and run `corpus_ingest` against a concrete pathway-family query.
- **GPG signing for release artefacts**: sidecar sha256 is written;
  detached GPG signature is a follow-up (no hard key dependency
  introduced until the release process is defined).

## (c) Requires human-curator throughput

- **Acceptance of LLM proposals into the canonical DB**. The pipeline
  routes to `for_curation/` on pass and `rejected/` on fail; an actual
  merge of accepted proposals back into the ingested DB needs a
  per-claim reviewer. Throughput is gated by curator availability, not
  code.
- **Resolving `<PIN_TBD>` markers** in `data/*/SOURCE.toml` to concrete
  versions and committing the `sha256` outputs from the first real fetch.
  A judgement call per source (which release? which commit?).
- **Hand-written fixture proposals** for the first-release slice (see
  below). The methanogenesis fixture is the seed; aa, fermentation, and
  central-carbon pathway fixtures follow.
- **Editing / re-validating rejected proposals**: currently the CLI
  records accept/reject; an `edit` subcommand (apply a JSON patch,
  re-hash, re-verify) is a natural extension.

## (d) First-release slice

Target: **central carbon, amino acids, common fermentations,
methanogenesis** — ~500–1000 pathways at parity with MetaCyc base
pathways. Evaluation against the held-out set supplied privately by
the maintainer.

Definition of done for v0.1:

1. `data/*/SOURCE.toml` pins are all concrete (`pinned_commit` / `pinned_release`
   resolved; `sha256` filled).
2. A universal SBML model is built from the ingested DB and the ATP-cycle
   regression value is pinned in CI.
3. Fixture proposals for the four target pathway families are hand-written
   and have passed the full verifier suite at least once.
4. `gapsmith-db release --version v0.1.0` produces a signed tarball and a
   RECEIPT.json that an independent third party can use to reproduce the
   build byte-for-byte modulo timestamps.
5. Measurement against the held-out set yields a comparable recall to
   MetaCyc base pathways on the target families. Numbers published in the
   release announcement.

## Downstream contract

The canonical types in `gapsmith-db-core` are the public contract with
[gapsmith](https://github.com/bio-ontology-research-group/gapsmith) (the
Rust gapseq reimplementation). Schema-breaking changes require a
coordinated update there. Additive changes — new optional fields, new
enum variants marked non-exhaustive — are safe.

# Roadmap

Status as of end of Phase 5 (2026-04-17). Licence-clean by construction:
no MetaCyc, no BioCyc (see [`LICENSING.md`](./LICENSING.md)).

## (a) Implemented and reproducible now

- **Cargo workspace** with five crates: `gapsmith-db-{core, ingest, verify,
  propose, cli}` plus a Python subtree (`python/`, uv-managed, 3.12).
- **Data fetchers** (9 sources) with pinned commit/release + per-file SHA256,
  single shared HTTP client (retry + ETag cache + `GAPSMITH_OFFLINE`), and
  a hard gate on KEGG (`--i-have-a-kegg-licence`).
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
  lint, ruff, python-bridge ping, end-to-end ingest → verify → propose →
  curate → release over a fixture.

## (b) Stubbed (the shape is right; the innards are minimal)

- **Qdrant retrieval**: HTTP-protocol skeleton in place; `search` returns
  a "wire the embedder" error. Needs: embedder choice, corpus-ingest
  script (Europe PMC OA + bioRxiv), and end-to-end integration test.
- **DL consistency checker**: emits the OWL signature from ChEBI roles +
  a TODO diagnostic. Needs an OWL reasoner (ELK or HermiT via a small
  Python helper) and GO-BP integration.
- **Universal metabolic model** (for ATP-cycle + pathway-flux verifiers):
  no model file bundled yet. Once built from the ingested DB, the
  ATP-cycle regression value can be pinned per plan.md.
- **Rhea RDF ingest**: Phase 2 parses the TSV tables only; the RDF ship
  has the hook in `SOURCE.toml` but no parser.
- **UniProt cursor walk**: Phase 1 emits page 1 to `swissprot_ec.json`;
  subsequent pages need the cursor-walk implementation to concatenate.
- **IntEnz + Reactome ingestion into the canonical schema** (their
  parsers exist in `gapsmith-db-ingest::parse` but are not yet called
  from the default ingest pipeline).
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

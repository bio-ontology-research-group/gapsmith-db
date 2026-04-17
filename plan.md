You are helping me bootstrap a project called `gapsmith-db` — an open, licence-clean, LLM-accelerated curation pipeline for metabolic pathway and reaction databases, intended as a drop-in replacement for the restrictively-licensed parts of MetaCyc/BioCyc when feeding a Rust reimplementation of gapseq.

Architecture (read before acting):
- Core engine and schema: Rust (interoperates with my existing Rust gapseq port).
- Bio ecosystem glue (cobrapy, equilibrator, PubMed, Europe PMC): Python, invoked as subprocess tools or via PyO3. Do not reimplement in Rust what exists mature in Python.
- LLM proposer: model-agnostic via an abstraction layer; default backend is OpenRouter (I already have OpenClaw + OpenRouter configured).
- Verifier layer is the heart of the system. LLM outputs are untrusted proposals; symbolic and constraint-based verifiers are the judges.
- Hard constraint: no MetaCyc, no BioCyc — not in code, data, corpus, training, retrieval, or prompts.

Proceed in phases. Stop and summarise at the end of each phase; wait for my "continue" before moving to the next. Ask before making any non-obvious design choice.

=== Phase 0: Workspace and project setup ===
- Initialise a Cargo workspace with crates: `gapsmith-db-core` (schema, types, serde), `gapsmith-db-ingest` (source-specific loaders), `gapsmith-db-verify` (deterministic verifiers), `gapsmith-db-propose` (LLM proposer client), `gapsmith-db-cli` (binary).
- Add a `python/` subtree with `pyproject.toml` (uv-managed): `cobra`, `equilibrator-api`, `requests`, `pydantic`, `pytest`.
- Add `data/` with subdirectories per source (`modelseed/`, `rhea/`, `chebi/`, `mnxref/`, `uniprot/`, `intenz/`, `reactome/`, `gapseq/`). Each must contain a `SOURCE.toml` declaring: upstream URL, licence, pinned commit/version/release date, SHA256 of the retrieved archive, attribution string.
- Add `LICENSING.md` enumerating every source, its licence, and redistribution status. Add `PROVENANCE.md` describing the commit-pinning and hash-verification policy.
- Create `justfile` with targets: `fetch`, `verify-hashes`, `ingest`, `test`, `bench`, `release`.
- Git init, sensible `.gitignore`, pre-commit hook running `cargo fmt`, `cargo clippy -- -D warnings`, `ruff`.

Confirm Rust toolchain target (stable), Python version (3.12), and that I want `uv` rather than poetry. Do not fetch data yet.

=== Phase 1: Data acquisition (pinned) ===
Implement `gapsmith-db-ingest` fetchers for, in this order:
1. ModelSEEDDatabase — `reactions.tsv`, `compounds.tsv`, `Pathways/ModelSEED_Subsystems.tsv`. Pin to a Git commit.
2. MNXref — `chem_xref.tsv`, `reac_xref.tsv`, `chem_prop.tsv`, `reac_prop.tsv` from metanetx.org. Pin to release version.
3. Rhea — RDF/TSV release plus ChEBI mappings. Pin to release number.
4. ChEBI — `chebi.obo` or the structured release.
5. IntEnz — XML enzyme nomenclature release.
6. UniProtKB/Swiss-Prot — EC-annotated subset via UniProt REST. Store raw JSON shards.
7. Reactome — lowest-level pathway definitions, open organisms only.
8. gapseq `dat/` — `seed_reactions_corrected.tsv`, `seed_metabolites_edited.tsv`, `seed_transporter*.tbl`, `custom_pwy.tbl`. Record the GPLv3 obligation.
9. KEGG — DO NOT auto-fetch; KEGG REST terms restrict bulk automated pulls. Stub a fetcher gated behind an explicit `--i-have-a-kegg-licence` flag.

Every fetcher must: download to temp, verify SHA256 against a pinned value (or print the hash on first run for me to commit), atomically move into place, write a `MANIFEST.json` with {source, version, retrieved_at, sha256, url}.

Add a CI lint that greps for "metacyc" and "biocyc" outside LICENSING.md and fails if found in code, data, or prompt paths.

Stop after fetchers compile and dry-run. Await confirmation before running a real fetch.

=== Phase 2: Core schema and ingestion ===
In `gapsmith-db-core`, define the canonical internal schema:
- `Compound { id, formula, charge, inchi, inchikey, smiles, mass, xrefs: BTreeMap<Source, Vec<String>>, names, chebi_roles }`
- `Reaction { id, stoichiometry: Vec<(CompoundId, f64, Compartment)>, reversibility, ec_numbers, rhea_id, seed_id, delta_g: Option<(f64, f64)>, is_transport, status: MassBalanceStatus, xrefs, evidence: Vec<Evidence> }`
- `Pathway { id, name, reactions, variant_of, organism_scope, evidence }`
- `Evidence { source, citation: Option<Pmid>, curator, proposal_hash, verifier_log, confidence }`
- Compartment, OrganismScope, Reversibility, MassBalanceStatus, EcNumber as strict enums/newtypes.

Implement ingestion from ModelSEED + MNXref + Rhea + ChEBI + gapseq-corrections. Deduplicate compounds via InChIKey first, then MNXref, then name match (last resort, flagged). Serialise to a human-diffable TSV and a compact binary (`rkyv` or `bincode`).

Property tests: every compound has ≥1 identifier; every reaction's stoichiometry references known compounds; every EC number parses.

=== Phase 3: Symbolic verifier layer ===
In `gapsmith-db-verify`, implement as independent checkers, each returning a typed diagnostic:
1. `AtomBalance` — per reaction, using ChEBI formulae; tolerate explicit-proton ambiguity; flag (don't reject) hydrogen-only imbalances.
2. `ChargeBalance` — using ChEBI charges.
3. `EcValidity` — parse against IntEnz; check four-level existence.
4. `UniProtExistence` — against the local Swiss-Prot snapshot.
5. `PmidExistence` — resolve PMIDs against a local E-utilities cache; offline by default, online behind a flag.
6. `ThermodynamicFeasibility` — call eQuilibrator via the Python bridge for ΔG.
7. `AtpCycleTest` — build the universal model with `cobrapy`, close all exchanges, maximise ATP hydrolysis; assert ≤ epsilon. Regression: pin the value, fail CI on drift.
8. `PathwayFluxTest` — given pathway + medium, assert FBA flux on the universal model.
9. `DlConsistencyCheck` — stub for ChEBI-role + GO-BP consistency via an OWL reasoner; emit a minimal OWL signature and a TODO.

Every verifier runs standalone and as batch. Output is a structured `VerifierReport` serialisable to JSON. Fail-closed.

=== Phase 4: LLM proposer scaffold ===
In `gapsmith-db-propose`:
- Strict JSON Schema for a pathway proposal: reactions (by ChEBI+EC or Rhea ID), enzymes (by UniProt), DAG structure, citations (PMIDs).
- OpenRouter client with model-agnostic interface, model in config.
- Retrieval backend: local Qdrant over an Europe PMC OA + bioRxiv corpus ingested by a separate script. Domain filter excludes biocyc.org, metacyc.org, and derivatives.
- Prompt template in `prompts/pathway_proposal.md`, versioned.
- Proposer emits JSON files to `proposals/pending/`, named by content hash.
- Proposals flow into the verifier automatically; failures → `proposals/rejected/` with reasons; passes → `proposals/for_curation/`.

No real LLM calls in this phase — plumbing only, with a mock proposer emitting hand-written fixture proposals.

=== Phase 5: Curator queue, provenance, CI ===
- CLI curator tool: list proposals, show diff against current DB, accept/edit/reject, record decision with a hash chain (previous-decision-hash → this decision).
- CI (GitHub Actions, but portable): cargo test, clippy, pytest, full verifier suite on fixture DB, ATP-cycle regression, licence lint, corpus domain-filter test.
- Release artefact: signed tarball with TSV + binary DB + MANIFEST.json + reproducibility receipt (all source versions + curator decisions replayed).

At the end of Phase 5, produce `ROADMAP.md` distinguishing (a) implemented and reproducible now, (b) stubbed, (c) requires human-curator throughput, (d) the scoped first-release slice: central carbon, amino acids, common fermentations, methanogenesis — target ~500–1000 pathways at parity with MetaCyc base pathways, measured on a held-out set I will supply privately.

Global constraints:
- Prefer `just` over `make`. Prefer `uv` for Python.
- No `unwrap()` in non-test Rust; use `thiserror` + `Result`.
- All external HTTP through one crate with retry, caching, and a global offline off-switch.
- Every data file has a `SOURCE.toml`.
- Brief ADRs in `docs/adr/` for any non-obvious choice.

Start with Phase 0. Stop at the end of Phase 0 and wait for me.

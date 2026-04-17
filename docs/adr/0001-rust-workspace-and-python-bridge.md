# ADR 0001: Rust workspace + Python bio-ecosystem bridge

- **Status**: accepted
- **Date**: 2026-04-17
- **Phase**: 0

## Context

The project has two conflicting pulls:

1. Rust core. gapsmith-db must interoperate with
   [gapsmith](https://github.com/bio-ontology-research-group/gapsmith) — the
   user's Rust gapseq reimplementation (sibling checkout at `../gapseq/gapseq-rs/`).
   gapsmith consumes the canonical schema, so Rust newtypes and rkyv/bincode
   serialisation are the contract.
2. Bio ecosystem. cobrapy, equilibrator-api, and the PubMed / Europe PMC HTTP
   clients are mature and well-maintained in Python. Reimplementing FBA,
   Component Contribution, or E-utilities in Rust is a year of yak-shaving.

## Decision

- Rust holds the schema, ingestion orchestration, verifiers' control flow,
  proposer client, and CLI. This is what other projects link against.
- Python is a subprocess/PyO3 bridge for the specific libraries listed above.
  Wire format is JSON matching pydantic models on the Python side and serde
  structs on the Rust side.
- No Rust reimplementation of what exists mature in Python.

## Consequences

- Two toolchains to keep current: Rust stable, Python 3.12 with `uv`.
- CI has both `cargo` and `uv` lanes.
- A stable JSON schema between the two sides is load-bearing; it lives in
  `gapsmith-db-core` (Rust) with a pydantic mirror in `python/src/gapsmith_bridge/`.
- Release artefacts are the Rust CLI + the canonical DB; the Python bridge
  is a build-time dependency of the verifier step, not a runtime dependency
  of consumers of the DB.
- The canonical schema in `gapsmith-db-core` is a public contract with
  gapsmith. Breaking changes require coordinated updates there.

## Alternatives considered

- Pure-Rust port of cobrapy + equilibrator. Rejected — estimated ~person-year
  of work with no offsetting benefit for a curation pipeline.
- Python-first with Rust only for performance-critical hot loops. Rejected —
  the existing Rust gapseq port is the primary downstream consumer.

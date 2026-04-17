# gapsmith-db

An open, licence-clean, LLM-accelerated curation pipeline for metabolic
pathway and reaction databases. Intended as a drop-in replacement for the
restrictively-licensed parts of MetaCyc/BioCyc when feeding
[gapsmith](https://github.com/bio-ontology-research-group/gapsmith), the
Rust reimplementation of gapseq.

**Status**: Phase 0 scaffold. See `plan.md` for the phased roadmap.

## Architecture

- **Core engine and schema**: Rust workspace (`crates/`).
- **Bio ecosystem glue**: Python (`python/`, uv-managed, py3.12) — cobra,
  equilibrator-api, requests, pydantic. Invoked as subprocess / PyO3.
- **LLM proposer**: model-agnostic; default backend OpenRouter.
- **Verifiers**: symbolic and constraint-based. LLM outputs are untrusted
  proposals; the verifier layer is the judge.
- **Hard constraint**: no MetaCyc, no BioCyc — anywhere. See
  [`LICENSING.md`](./LICENSING.md).

## Layout

```
crates/
  gapsmith-db-core/     schema, types, serde
  gapsmith-db-ingest/   source-specific loaders
  gapsmith-db-verify/   deterministic verifiers
  gapsmith-db-propose/  LLM proposer client
  gapsmith-db-cli/      binary (`gapsmith-db`)
python/                 cobra / equilibrator / PubMed bridge
data/                   per-source SOURCE.toml pins (artefacts fetched, not committed)
docs/adr/               architecture decision records
scripts/                git hooks, helpers
justfile                task runner
```

## Quick start

```sh
# One-time setup
just install-hooks
cd python && uv sync --extra dev && cd -

# Day-to-day
just check        # cargo check workspace
just test         # cargo test workspace
just py-test      # pytest
just lint         # fmt + clippy + ruff + licence-lint
just licence-lint # grep for forbidden sources
```

Data fetchers and ingestion are wired in Phase 1+.

## Licensing

Code: **GNU GPL-3.0-or-later** (see [`LICENSE`](./LICENSE)). Matches the
upstream gapseq licence.
Data obligations: per-source in [`LICENSING.md`](./LICENSING.md).

# gapsmith-db

An open, licence-clean, LLM-accelerated curation pipeline for metabolic
pathway and reaction databases. Intended as a drop-in replacement for the
restrictively-licensed parts of MetaCyc/BioCyc when feeding
[gapsmith](https://github.com/bio-ontology-research-group/gapsmith), the
Rust reimplementation of gapseq.

**Status**: Phases 0–5 complete; v0.1 ingest + verifier + proposer +
curator + release pipeline is operational end-to-end on a fixture.
See [`ROADMAP.md`](./ROADMAP.md) for what is implemented, stubbed, and
blocked on curator throughput.

## Architecture

- **Core engine and schema**: Rust workspace (`crates/`).
- **Bio ecosystem glue**: Python (`python/`, uv-managed, py3.12) — cobra,
  equilibrator-api, sentence-transformers, qdrant-client. Invoked via
  a uniform subprocess bridge (`gapsmith_bridge.verify --action <name>`).
- **LLM proposer**: model-agnostic; default backend OpenRouter. A
  fixture-backed mock lets CI run end-to-end without an API key.
- **Verifiers**: nine symbolic and constraint-based checks (atom balance,
  charge balance, EC validity, UniProt/PMID existence, ΔG via
  eQuilibrator, ATP-cycle regression, pathway flux, DL consistency stub).
  LLM outputs are untrusted proposals; the verifier layer is the judge.
- **Retrieval**: Qdrant over an Europe PMC Open Access corpus, embedded
  by sentence-transformers. A domain filter keeps banned sources out at
  both ingest and retrieval time.
- **Hard constraint**: no MetaCyc, no BioCyc — anywhere. See
  [`LICENSING.md`](./LICENSING.md).

## Layout

```
crates/
  gapsmith-db-core/     schema, types, serde
  gapsmith-db-ingest/   source-specific loaders, per-source SOURCE.toml
  gapsmith-db-verify/   deterministic verifiers, universal-SBML builder
  gapsmith-db-propose/  LLM proposer client, retrieval, decision log
  gapsmith-db-cli/      the `gapsmith-db` binary
python/src/gapsmith_bridge/
  verify.py             cobra / equilibrator / embed bridge
  corpus_ingest.py      Europe PMC -> Qdrant ingest script
data/<source>/          SOURCE.toml pin per source (artefacts fetched, not committed)
proposals/fixtures/     hand-authored pathway proposals for the first-release slice
verify/baselines/       pinned ATP-cycle regression values
docs/adr/               architecture decision records
scripts/                git hooks, helpers
justfile                task runner
```

## Quick start

One-time setup:

```sh
just install-hooks                                  # pre-commit: fmt/clippy/ruff/licence-lint
uv sync --project python                            # core Python deps
uv sync --project python --extra retrieval          # + sentence-transformers, qdrant-client
```

Day-to-day:

```sh
just check           # cargo check workspace
just test            # cargo test workspace
just py-test         # pytest (when there are tests)
just lint            # fmt + clippy + ruff + licence-lint
just licence-lint    # grep tracked files for forbidden sources
```

## Pipeline in anger

End-to-end on real data (lead-in: fetching can pull hundreds of MB per
source; MNXref is 1.5 GB):

```sh
# 1. Fetch pinned upstream data. Engine prints sha256 of each artefact.
gapsmith-db fetch                                    # all sources except KEGG
gapsmith-db fetch --source chebi                     # or one at a time

# 2. Ingest the canonical schema -> bincode DB + human-diffable TSV.
gapsmith-db ingest \
    --data-root data \
    --out-binary build/db.gapsmith \
    --out-tsv build/tsv

# 3. Run the deterministic verifier suite. Errors fail the run unless
#    --allow-errors; warnings are surfaced in the JSON report.
gapsmith-db verify \
    --db build/db.gapsmith \
    --report build/report.json \
    --intenz-dat data/intenz/enzyme.dat \
    --uniprot-snapshot data/uniprot/swissprot_ec.json

# 4. Propose a pathway via the fixture-backed mock (use --model <slug>
#    for a real OpenRouter call).
gapsmith-db propose --mock --query "methanogenesis from CO2" \
    --proposals-dir proposals \
    --fixture-dir proposals/fixtures

# 5. Curator: list/show/accept/reject; every decision is appended to a
#    hash-chained log.
gapsmith-db curate list
gapsmith-db curate accept <proposal_id> --curator rh --comment "looks right"
gapsmith-db curate verify-chain

# 6. Build a release tarball (TSV + binary DB + MANIFEST + RECEIPT).
gapsmith-db release \
    --db build/db.gapsmith \
    --tsv-dir build/tsv \
    --out dist/gapsmith-db-v0.1.0.tar.gz
```

### Universal SBML + ATP-cycle regression

The verifier layer needs a universal cobra model for the ATP-cycle and
pathway-flux checks. The builder delegates SBML writing to cobrapy:

```sh
# Build SBML from the ingested DB. --add-atpm synthesises an ATPM
# reaction so the regression test has a handle; pass the canonical IDs
# your DB actually uses for ATP/ADP/Pi/H2O/H+.
gapsmith-db universal build \
    --db build/db.gapsmith \
    --out build/universal.xml \
    --add-atpm \
    --atpm-ids "atp=atp_c,adp=adp_c,pi=pi_c,h2o=h2o_c,h=h_c"

# Pin the baseline once (check it in).
gapsmith-db universal pin-atp-cycle \
    --model build/universal.xml \
    --out verify/baselines/atp_cycle_<release>.json \
    --pinned-at v0.1.0

# CI re-runs and compares against the baseline.
gapsmith-db universal check-atp-cycle \
    --model build/universal.xml \
    --baseline verify/baselines/atp_cycle_<release>.json
```

### Retrieval corpus

Stand up Qdrant locally, then ingest an Europe PMC topic:

```sh
docker run --rm -p 6333:6333 qdrant/qdrant:latest &
uv run --project python --extra retrieval python \
    -m gapsmith_bridge.corpus_ingest \
    --query "methanogenesis from CO2" \
    --qdrant-url http://localhost:6333 \
    --collection gapsmith \
    --max-papers 200
```

`gapsmith-db propose` and `gapsmith-db propose-catalogue` then read
from that collection when `--qdrant-url` is set:

```sh
gapsmith-db propose \
    --model qwen/qwen3.6-plus \
    --query "methanogenesis from CO2" \
    --qdrant-url http://localhost:6333 \
    --qdrant-collection gapsmith
```

### Batch catalogue run

To propose over a whole pathway-name seed (`proposals/catalogue/`):

```sh
# Pilot: 10 rows only, 500ms throttle, dry-run first to sanity-check.
gapsmith-db propose-catalogue \
    --seed proposals/catalogue/microbial.tsv \
    --seed proposals/catalogue/reactome.tsv \
    --model qwen/qwen3.6-plus \
    --qdrant-url http://localhost:6333 \
    --limit 10 --throttle-ms 500 --dry-run

# Real run. --resume skips pathways already proposed on disk so the
# driver is restartable after rate-limit backoff.
gapsmith-db propose-catalogue \
    --seed proposals/catalogue/microbial.tsv \
    --model qwen/qwen3.6-plus \
    --qdrant-url http://localhost:6333 \
    --throttle-ms 3000 --resume

# Per-run TSV log lands in proposals/runs/catalogue_<timestamp>.tsv
# with one row per pathway: timestamp, pathway, status, detail.
```

Category filter is handy for piloting inside one family
(`--category methanogenesis`, `--category amino_acid_biosynthesis`, …).

## Licensing

Code: **GNU GPL-3.0-or-later** (see [`LICENSE`](./LICENSE)). Matches the
upstream gapseq licence.
Data obligations: per-source in [`LICENSING.md`](./LICENSING.md).
Hard exclusions: see ADR 0002.

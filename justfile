# gapsmith-db task runner. `just` preferred over `make`. See plan.md.

set shell := ["bash", "-euo", "pipefail", "-c"]
set dotenv-load := true

default:
    @just --list

install-hooks:
    ./scripts/install-hooks.sh

# -- Rust --------------------------------------------------------------------

fmt:
    cargo fmt --all

check:
    cargo check --workspace --all-targets

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace --all-targets

bench:
    cargo bench --workspace

# -- Python ------------------------------------------------------------------

py-sync:
    cd python && uv sync --extra dev

py-test: py-sync
    cd python && uv run pytest

py-lint:
    cd python && uv run ruff check .

py-fmt:
    cd python && uv run ruff format .

# -- Data pipeline -----------------------------------------------------------

# Fetch every pinned upstream source. Reads data/*/SOURCE.toml.
fetch *ARGS:
    cargo run --release -p gapsmith-db-cli -- fetch {{ARGS}}

# Re-verify SHA256 of everything already on disk.
verify-hashes:
    cargo run --release -p gapsmith-db-cli -- fetch --verify-only

# Run ingestion into the canonical schema.
ingest:
    cargo run --release -p gapsmith-db-cli -- ingest

# -- Licence + corpus lints --------------------------------------------------

# Fail if MetaCyc/BioCyc appear in code, data, or prompt paths. Hard
# constraint per plan.md. Documentation discussing the exclusion is fine.
licence-lint:
    #!/usr/bin/env bash
    set -euo pipefail
    hits=$(git grep -Iin -E 'metacyc|biocyc' -- \
        'crates/**/*.rs' \
        'python/src/**/*.py' \
        'python/tests/**/*.py' \
        'data/**/SOURCE.toml' \
        'data/**/*.tsv' \
        'data/**/*.json' \
        'prompts/**' \
        'corpus/**' \
        || true)
    if [ -n "$hits" ]; then
        echo "licence-lint: forbidden references in code/data/prompts:" >&2
        echo "$hits" >&2
        exit 1
    fi

# -- Aggregates --------------------------------------------------------------

lint: fmt clippy py-lint licence-lint

ci: check clippy test py-test licence-lint

release:
    cargo build --release --workspace
    @echo "Release artefact build is implemented in Phase 5."

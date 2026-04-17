# Provenance and reproducibility

Every byte that flows into a gapsmith-db release is traceable to a pinned
upstream artefact with a verified hash. Non-reproducible builds are a bug.

## Per-source pinning

Each `data/<source>/SOURCE.toml` declares:

- `upstream_url` — canonical source location.
- `licence` and `licence_url` — verbatim licence identifier and reference.
- `attribution` — citation string emitted into release `CITATIONS.md`.
- One of `pinned_commit`, `pinned_release`, `pinned_date` — the exact
  upstream version we depend on.
- `sha256` — hash of the retrieved archive (or of the tarball when an
  upstream source ships multiple files).
- `artefacts` — list of files consumed from the archive.

## Fetch policy

Every fetcher in `gapsmith-db-ingest`:

1. Downloads to a temp path under `data/<source>/.tmp/`.
2. Computes SHA256 of the retrieved bytes.
3. Verifies against `SOURCE.toml::sha256`. Mismatch → abort, preserve the
   temp file for inspection, do not overwrite the canonical location.
4. On first-ever fetch (when `sha256` is empty), prints the computed hash
   and exits — the maintainer commits the hash, then re-runs.
5. On success, atomically moves the archive into
   `data/<source>/<artefact>` and writes `data/<source>/MANIFEST.json`:
   ```json
   {
     "source": "Rhea",
     "version": "134",
     "retrieved_at": "2026-04-17T10:12:34Z",
     "sha256": "…",
     "url": "…"
   }
   ```

## HTTP policy

All external HTTP goes through one crate with:

- Retry with exponential backoff and jitter.
- On-disk response cache keyed by `(url, etag|last-modified)`.
- A global offline off-switch (env `GAPSMITH_OFFLINE=1` or
  `--offline`). When offline, any cache miss is a hard error.

## Release reproducibility receipt

Each release tarball contains a `RECEIPT.json` recording:

- Every `SOURCE.toml` snapshot (name, pin, sha256, retrieved_at).
- Every curator decision hash that went into the final DB (see Phase 5
  hash-chained decision log).
- The gapsmith-db commit, Rust toolchain, and Python interpreter version.
- An independent third party can re-run the pipeline against the pinned
  upstream versions and reproduce the artefact byte-for-byte modulo
  timestamps.

## What is NOT committed

- Raw upstream data artefacts (`data/*/*.tsv`, `data/*/*.json`, etc.).
- `MANIFEST.json` files (regenerated per fetch).
- LLM API keys, proposal cache, or model snapshots.

What IS committed: `SOURCE.toml` per source, hash pins, attribution
strings, curator decision log, `CITATIONS.md`.

# ADR 0006: Retrieval = Europe PMC OA + PubMedBERT embeddings + Qdrant

- **Status**: accepted
- **Date**: 2026-04-17
- **Phase**: 5 (follow-up)

## Context

plan.md (Phase 4) specifies "local Qdrant over an Europe PMC OA +
bioRxiv corpus ingested by a separate script" and "domain filter
excludes biocyc.org, metacyc.org, and derivatives". Three concrete
choices were left open at the end of Phase 4:

1. Which embedding model.
2. What corpus source(s) to pull.
3. Where the embedder runs (same bridge as verifiers? separate
   service?).

## Decision

1. **Default embedder**: `NeuML/pubmedbert-base-embeddings` (768-dim,
   sentence-transformers compatible, fine-tuned on PubMed). Overridable
   per `QdrantConfig`.
2. **Corpus source**: Europe PMC Open Access subset, filtered by
   `OPEN_ACCESS:Y AND HAS_FT:Y`. JATS XML is fetched per hit,
   paragraphs are extracted with a cheap regex (80-char floor), and
   each paragraph becomes one Qdrant point. bioRxiv is a follow-up
   (their API is stabler but the OA subset is smaller, so Europe PMC
   is a better default seed).
3. **Bridge unification**: the embedder runs through the same
   `python -m gapsmith_bridge.verify --action <name>` subprocess as
   the verifier actions. One new action, `embed`, takes `{text,
   model}` and returns `{vector, model, dim}`.

Implementation split:

- `python/src/gapsmith_bridge/corpus_ingest.py` is the standalone
  ingest script (Europe PMC → chunks → embed → Qdrant upsert).
- `python/src/gapsmith_bridge/verify.py` re-exports
  `corpus_ingest.action_embed` as the `embed` action.
- `crates/gapsmith-db-propose/src/retrieval/qdrant.rs` invokes that
  bridge for query-time embedding, then POSTs the vector to
  Qdrant's `/collections/<name>/points/search`, and maps the
  payload to `Passage`. Every returned passage is passed through
  `DomainFilter` regardless of upstream filtering.
- Optional dependencies (`sentence-transformers`, `qdrant-client`)
  live under a `retrieval` extra in `pyproject.toml`; users who
  never touch retrieval don't download the transformer stack.

## Consequences

- PubMedBERT has broad biomedical coverage and reasonable inference
  speed on CPU; GPU optional. ~440 MB of model weights on first
  download.
- Query-time cold-start: the first call after a fresh Python environment
  loads the transformer (~1–2 s). The bridge spawns a subprocess per
  call, so there is no model-cache warmth across queries. If this hurts
  in practice, a follow-up can add a resident embedding server; for
  now the latency is fine for curator-scale workloads.
- The ingest script has no `lxml` dependency — paragraph extraction is
  a regex. Good enough for OA JATS; a future iteration can swap in a
  proper XML parser when the corpus diversifies.
- Banned domains are enforced twice (ingest-time + retrieval-time),
  with the Python-side list a superset of the Rust `DomainFilter` to
  hedge against upstream aliases.

## Alternatives considered

- **`sentence-transformers/all-MiniLM-L6-v2`** (384-dim, ~80 MB).
  Lighter and faster, but general-purpose. Kept as a documented
  drop-in for users with tight disk budgets.
- **OpenAI `text-embedding-3-small` / Cohere embed-v3**. Rejected as
  the default for the same reason OpenRouter is not auto-enabled:
  hard dependency on a paid API key. Either is trivially swappable at
  the config layer if a deployer wants it.
- **Local Qdrant only vs. Qdrant Cloud**. Local by default
  (`http://localhost:6333`), `api_key` supported for cloud. No
  decision forced.
- **Embedder as a long-lived HTTP service**. Rejected for v0.1 —
  subprocess bridging is the simplest thing that works, matches the
  verifier pattern, and survives crashes cleanly. Revisit if query
  latency becomes a bottleneck.

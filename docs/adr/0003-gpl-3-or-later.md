# ADR 0003: Code licence — GNU GPL-3.0-or-later

- **Status**: accepted
- **Date**: 2026-04-17
- **Phase**: 0

## Context

gapsmith-db incorporates curation corrections from the gapseq `dat/`
directory, which is GPL-3.0-or-later. A permissive licence on gapsmith-db
itself would split the project into two build modes: a permissive
`--permissive-only` artefact without gapseq corrections, and a combined
artefact forced to GPL. That duality adds permanent complexity to the
release pipeline and to any downstream consumer's licensing analysis.

The other tabled option was BSD-3-Clause (permissive) — acceptable only
if we were willing to drop the gapseq corrections entirely.

## Decision

Licence the entire gapsmith-db code base under **GPL-3.0-or-later**,
matching the upstream gapseq licence. `LICENSE` at the repo root carries
the verbatim FSF text.

`-or-later` chosen rather than `-only` because gapseq itself is
`-or-later`; using `-only` here would be more restrictive than the
upstream and is the FSF-recommended default.

## Consequences

- One licence, one build mode. `CITATIONS.md` per release lists upstream
  sources and their licences; the combined artefact ships under GPL-3.0-or-later.
- Downstream code that statically links gapsmith-db crates inherits the
  GPL obligation. Code that merely consumes the released DB as data (no
  linking) is not a combined work for GPL purposes — the DB is a data
  artefact with attribution per `CITATIONS.md`.
- CC-BY-4.0 data sources (ModelSEED, MNXref, Rhea, ChEBI, IntEnz, UniProt,
  Reactome) are compatible with GPL redistribution when attribution is
  preserved.
- Consumers that need a permissive downstream (e.g. a Rust library using
  gapsmith-db-core types) would need a separate re-licensing of
  gapsmith-db-core specifically; not planned.

## Alternatives considered

- **Apache-2.0 OR MIT** (original Phase-0 default). Rejected in favour of
  single-licence simplicity.
- **BSD-3-Clause**. User's named fallback. Rejected for the same reason —
  would force dropping the gapseq corrections.
- **GPL-3.0-only**. Rejected — more restrictive than upstream gapseq.

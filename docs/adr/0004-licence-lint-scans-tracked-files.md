# ADR 0004: Licence-lint scans committed files only

- **Status**: accepted
- **Date**: 2026-04-17
- **Phase**: 5 (follow-up)

## Context

The licence-lint rule in plan.md is "grep for the two banned source
names in code, data, or prompt paths and fail if found". The original
implementation walked the filesystem under the repo root. That worked
while `data/` was empty, but on the first real fetch it tripped on
~180k legitimate cross-reference IDs inside gapseq's `dat/mnxref_reac_xref.tsv`
(rows of the form `metacyc:XYZ-RXN\tMNXR...`, where the `metacyc:XYZ`
string is an upstream-assigned identifier, not MetaCyc content).

Two things are simultaneously true:

1. We must not redistribute MetaCyc/BioCyc content.
2. A gitignored upstream artefact that names a banned source as an
   opaque ID in its own xref table is not redistribution on our part —
   we didn't make the file, and it isn't in our release tarball.

A filesystem-walk lint conflates the two.

## Decision

Switch both the cargo test (`crates/gapsmith-db-ingest/tests/licence_lint.rs`)
and the pre-commit hook (`scripts/pre-commit`) to scan **only files
tracked in git**. Everything else — fetched upstream artefacts, build
output, local caches, scratch proposals — is out of scope.

Implementation: the cargo test enumerates files via `git ls-files -z`;
the pre-commit hook uses `git diff --cached` with an explicit path
filter. Same needle set either way.

The `.gitignore` is the backstop: if a file isn't ignored, it's
redistributed, and the lint applies.

## Consequences

- Fetched upstream data may contain banned-source xref IDs without
  tripping CI. This is how upstream sources actually work; pretending
  otherwise was the bug.
- The lint now has a dependency on a git worktree. The test panics with
  a clear error if run outside one, which is the desired behaviour (CI
  runs inside a checkout; there is no sensible use for the lint
  outside one).
- A future mistake that causes fetched artefacts to accidentally be
  committed — e.g. a maintainer running `git add -A` on a populated
  `data/` tree — will still be caught, because as soon as the file
  enters git it becomes in-scope for the lint.

## Alternatives considered

- **Narrower filesystem walk** with a hard-coded exclusion for
  `data/<source>/` (the gitignored subtree). Rejected: the exclusion
  duplicates `.gitignore` and is easy to drift out of sync.
- **Strip banned-source xref rows at ingest time**, so the fetched
  files on disk are clean. Rejected: the ingest would discard
  legitimate upstream information (gapseq explicitly uses those xrefs
  to map pathway annotations), and the lint check is about
  *redistribution*, not disk state.

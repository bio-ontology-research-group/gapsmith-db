# ADR 0002: Hard exclusion of MetaCyc and BioCyc

- **Status**: accepted
- **Date**: 2026-04-17
- **Phase**: 0

## Context

The entire motivation for gapsmith-db is to replace the restrictively-licensed
parts of MetaCyc/BioCyc. Any leakage of those sources — into code, data,
retrieval corpus, training material, prompts, or even developer intuition
informed by copying their pathway maps — undermines the licence story and
destroys the project's reason to exist.

## Decision

1. No MetaCyc/BioCyc data at any layer: raw files, derived tables, vector
   embeddings, RAG corpus, prompt templates, few-shot examples.
2. The retrieval corpus domain filter explicitly excludes `biocyc.org`,
   `metacyc.org`, and known mirrors.
3. A CI lint (`just licence-lint`) greps the tree for the strings `metacyc`
   and `biocyc` outside `LICENSING.md` (and this ADR) and fails the build.
4. The pre-commit hook runs the same grep against staged changes.

## Consequences

- Pathway curation starts from a narrower knowledge base: ModelSEED
  subsystems, Rhea reaction groupings, Reactome (open organisms), gapseq
  pathway definitions, and the primary literature via Europe PMC OA.
- First-release parity target is explicitly scoped to core metabolism
  (central carbon, amino acids, common fermentations, methanogenesis) —
  where the above sources are already strong.
- Periodic audit required: whenever a new retrieval corpus is ingested,
  rerun the domain-filter test.

## Alternatives considered

None acceptable. A soft policy ("try not to use MetaCyc") is insufficient
because incidental reuse is easy and detection after the fact is hard.

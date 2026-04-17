# Licensing

gapsmith-db is an open, licence-clean curation pipeline.

**Code licence: GNU GPL-3.0-or-later.** Full text in [`LICENSE`](./LICENSE).
This aligns with the upstream gapseq project (also GPL-3.0-or-later) so
that combined builds incorporating gapseq `dat/` corrections raise no
additional licence questions.

## Hard exclusions

**No MetaCyc. No BioCyc.** Not in code, not in data, not in the retrieval
corpus, not in prompts, not in training material, not as inspiration for
curation decisions. Contributions that introduce MetaCyc- or BioCyc-derived
content — at any layer — will be rejected. A CI lint greps the tree for
these strings outside this file and fails the build if found.

Rationale: MetaCyc/BioCyc data is distributed under a restrictive commercial
licence incompatible with this project's redistribution goals. The licence
forbids creating derivative databases and redistributing extracted content.

## Upstream sources

Every data source below is listed with its licence, the subset we consume,
and the redistribution status. `SOURCE.toml` in the per-source directory
under `data/` carries the full attribution string and the pinned release
identifiers.

| Source                 | Licence              | Redistribute? | Notes                                           |
|------------------------|----------------------|---------------|-------------------------------------------------|
| ModelSEEDDatabase      | CC-BY-4.0            | Yes, w/ attr  | Primary SEED reactions/compounds.               |
| MetaNetX / MNXref      | CC-BY-4.0            | Yes, w/ attr  | Cross-reference mapping only.                   |
| Rhea                   | CC-BY-4.0            | Yes, w/ attr  | Canonical ChEBI-backed reactions.               |
| ChEBI                  | CC-BY-4.0            | Yes, w/ attr  | Compound formulae, charges, roles, InChI.       |
| IntEnz                 | CC-BY-4.0            | Yes, w/ attr  | EC nomenclature for EcValidity.                 |
| UniProtKB/Swiss-Prot   | CC-BY-4.0            | Yes, w/ attr  | EC-annotated reviewed subset.                   |
| Reactome               | CC-BY-4.0            | Yes, w/ attr  | Pathway definitions (open organisms only).      |
| gapseq `dat/`          | GPL-3.0-or-later     | **Copyleft**  | Combined works must ship under GPL-3.0-or-later.|
| KEGG                   | Proprietary          | **No**        | Fetch gated behind `--i-have-a-kegg-licence`.   |

## The gapseq GPL alignment

`data/gapseq/` incorporates corrections from the gapseq `dat/` directory.
gapseq is GPL-3.0-or-later. Because gapsmith-db is itself GPL-3.0-or-later,
combined builds are covered by a single licence — no dual-track build
artefact is needed. Downstream redistribution of a built DB that contains
gapseq `dat/` derivatives must follow GPL-3.0-or-later (source
availability, copyleft on modifications).

The CC-BY-4.0 data sources are compatible with redistribution under
GPL-3.0-or-later provided the attribution strings from each `SOURCE.toml`
are preserved in `CITATIONS.md`.

## KEGG

KEGG REST terms restrict bulk automated pulls. The KEGG fetcher is stubbed
and gated behind `--i-have-a-kegg-licence`. The KEGG data path is never
exercised in CI and KEGG-derived content is never included in a public
release artefact.

## Attribution

Each public release ships a `CITATIONS.md` generated from every
`SOURCE.toml` — one attribution line per source actually used in that
build, with the pinned release identifier and the retrieval date.

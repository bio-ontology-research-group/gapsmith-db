# Licensing

gapsmith-db is an open, licence-clean curation pipeline. The code in this
repository is dual-licensed **Apache-2.0 OR MIT** at the user's choice.

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

## The gapseq GPL obligation

`data/gapseq/` incorporates corrections from the gapseq `dat/` directory.
gapseq is GPL-3.0-or-later. The consequences:

- **Source code** in this repository is Apache-2.0 OR MIT and can always
  be reused under those terms.
- **Redistributing a built database artefact that contains gapseq `dat/`
  derivatives** (curated corrections, transporter tables, custom pathways)
  creates a combined work that must be distributed under GPL-3.0-or-later.
- A release-time flag (`--permissive-only`) will emit a build that omits
  gapseq-derived rows and relabels the artefact accordingly.

## KEGG

KEGG REST terms restrict bulk automated pulls. The KEGG fetcher is stubbed
and gated behind `--i-have-a-kegg-licence`. The KEGG data path is never
exercised in CI and KEGG-derived content is never included in a public
release artefact.

## Attribution

Each public release ships a `CITATIONS.md` generated from every
`SOURCE.toml` — one attribution line per source actually used in that
build, with the pinned release identifier and the retrieval date.

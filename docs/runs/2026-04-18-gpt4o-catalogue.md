# Full catalogue run — gpt-4o, prompt v0.4.0

- **Date**: 2026-04-18 / 2026-04-19
- **Model**: `openai/gpt-4o` via OpenRouter
- **Prompt version**: `0.4.0` (commit `eed0113`)
- **Seed**: `proposals/catalogue/microbial.tsv` (408) + `proposals/catalogue/reactome.tsv` (335) = **743 pathway names**
- **Retrieval**: Qdrant at `http://localhost:6333`, collection `gapsmith`, 798k Swiss-Prot + Europe PMC passages, top-k 8
- **Verification inputs**: Swiss-Prot snapshot pinned at 798,181 accessions (`data/uniprot/swissprot_ec.json`), IntEnz `enzyme.dat`, no online PMID/UniProt
- **Verifier policy commit**: `ed67c1d`

## Disposition

| disposition | count | share |
| ---         | ---:  | ---:  |
| **for_curation** | 650 | 88 % |
| **rejected**     | 89  | 12 % |
| pending          | 0   | —    |

`for_curation` is the curator queue. `rejected` means at least one
Error-level diagnostic fired.

## For-curation shape

- Reactions per proposal: mean **6.8**, min 1, max 11.
- Enzymes per proposal: mean **4.1**, min 1, max 12.
- 595 / 650 have at least one warning. Dominant warning codes:
  - **587 `no_reference`** — PMID cache empty and `--online-pmid` was
    off. Every proposal trips this; it is effectively "PMID check
    was skipped" rather than a real defect.
  - **15 `trembl_unreviewed`** — an enzyme entry references a
    UniProt accession that exists but is TrEMBL (unreviewed).
    Curator-visible, not rejected (see ADR 0006 follow-up /
    `verify-proposals` online logic).

## Rejections

All 89 rejections were driven by UniProt existence:

| code | count | meaning |
| --- | ---: | --- |
| `unknown_uniprot` | 185 | accession not in Swiss-Prot snapshot (likely hallucinated) |
| `unknown_ec`      |   1 | EC number not present in IntEnz |

Most rejected proposals had 2–3 bad accessions each. The one
`unknown_ec` is an outlier and worth a manual look before re-proposal.

Rejections concentrate in categories where Swiss-Prot coverage is
known to be thin:

- degradation (11), secondary metabolism (10), cofactor biosynthesis (9),
  Reactome "Metabolism of lipids" (6), metal/osmoprotectant/sulfur/
  isoprenoid/methanogenesis/signalling (2–3 each).

Central carbon, amino-acid biosynthesis, and well-studied cofactor
pathways are strongly over-represented in `for_curation` — matching
expectations and ADR 0002 ("release scope: central carbon, amino
acids, common fermentations, methanogenesis").

## Run mechanics

Three attempts were needed to complete the catalogue:

| attempt | wrote | outcome |
| --- | ---: | --- |
| 1 | 32 | main thread wedged in userspace spin after ~10 min; no I/O; not the watchdog-catchable kind yet — killed manually |
| 2 | 32 | same spin pattern on a different pathway; killed manually |
| 3 | 675 | completed cleanly under the 120 s per-pathway watchdog (commit `08fd34e`) |

All three attempts shared `--resume`, which skips pathways whose name
already has a proposal written on disk. The content-hash filenames
meant no manual deduplication was needed.

Zero watchdog timeouts fired during attempt 3; the fix was defensive
and did not have to catch anything. Rationale for the watchdog lives
in **ADR 0007**.

## What this does NOT validate

- No PMID ground-truth checking was done — the `no_reference` warning
  on 587 proposals is exactly that gap. To close it, re-run
  `verify-proposals --online-pmid` after either (a) adding the
  matching watchdog to `verify-proposals` or (b) pre-populating
  `data/pmid_cache.txt` with the union of all cited PMIDs via a
  one-shot batch E-utils fetch.
- No reaction-mass-balance or cofactor-stoichiometry check; Phase 3
  verifiers cover Rhea/ChEBI/EC validity only.
- No deduplication across near-synonym pathway names (e.g.
  "Glycolysis" vs "Embden-Meyerhof-Parnas glycolysis"). Curator work.

## Artefacts

- `proposals/full_run_gpt4o/for_curation/*.json` — 650 proposals +
  `*.report.json` sidecars.
- `proposals/full_run_gpt4o/rejected/*.json` — 89 proposals +
  sidecars; reason is in the sidecar's `by_verifier.uniprot_existence`.
- `proposals/full_run_gpt4o/runs/catalogue_*.tsv` — per-attempt
  append-only run logs (three files for the three attempts).
- Decision log (`proposals/DECISIONS.ndjson`) is **not** yet updated
  with these proposals. Committing them to the chain is a separate
  step.

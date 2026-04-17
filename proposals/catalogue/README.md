# Pathway catalogue

Seed lists of pathway **names** for the batch proposer. Names only — no
upstream pathway *content* is imported. The downstream proposer asks the
LLM to draft reactions/enzymes/citations from scratch, grounded in the
Europe PMC retrieval corpus; every reaction resolves to a Rhea/ChEBI/EC
ID and every enzyme to a UniProt accession before verification.

## Files

- **`reactome.tsv`** — 335 pathway names drawn from the `Metabolism`
  subtree of the ingested Reactome release (root `R-HSA-1430728`).
  Reactome is CC0. One entry per unique name; where Reactome has the
  same pathway in multiple species the Homo sapiens entry is canonical
  and `source_id` points at its stable ID for provenance.
- **`microbial.tsv`** — 408 bacterial/archaeal pathway names
  absent from Reactome. Drawn from textbook taxonomy (Madigan's Brock,
  Lehninger, White's *Physiology and Biochemistry of Prokaryotes*) and
  primary review literature. Deliberately avoids MetaCyc-specific
  naming ("superpathway of …", organism-tagged `PWY-`-style IDs).
  Generic biochemistry terms only. Category breakdown: degradation
  (53), cofactor biosynthesis (49), amino-acid biosynthesis (29),
  amino-acid catabolism (28), energy/bioenergetics (23), secondary
  metabolism (21), sugar catabolism (18), fermentation (18),
  nitrogen (16), C1/methylotrophy (16), lipid (12), sulfur (11),
  nucleotide biosynthesis (10), isoprenoid (10), cell envelope (10),
  siderophore (9), exopolysaccharide (9), central carbon (9),
  signalling molecule biosynthesis (8), storage polymer (7),
  photosynthesis (7), osmoprotectant (7), methanogenesis (7),
  carbon fixation (6), polyamine (4), phosphorus (3), remaining (8).

## Schema

Both files share the header:

```
pathway_name    category    organism_scope    source    source_id    notes
```

- `pathway_name` — the string passed to the proposer as `--query`.
- `category` — coarse grouping for batch-run scheduling and curator
  dashboards. Reactome rows inherit the direct child of `Metabolism`;
  microbial rows use the high-level categories defined in
  `microbial.tsv` (central_carbon, fermentation, methanogenesis, …).
- `organism_scope` — optional hint threaded into the proposer prompt.
  Useful for pathways whose chemistry differs by kingdom (e.g., FAS I
  vs FAS II; lysine DAP vs AAA).
- `source` — `reactome` or `textbook`.
- `source_id` — for Reactome entries only. Passed as an extra grounding
  citation into the prompt so the model knows which Reactome pathway
  node the name refers to (the reaction content is still proposed
  fresh; Reactome's SBPAX dumps are not ingested).
- `notes` — free-text hints for the curator.

## What the catalogue is NOT

- **Not a MetaCyc replacement today.** 573 names is a seed for the
  batch proposer, not a finished catalogue. Expect the list to grow as
  curators encounter gaps.
- **Not a pathway ontology.** No DAG, no super/sub-pathway hierarchy
  beyond the flat `category` column. If/when that matters, the
  Reactome relation file (ingested separately) is the source of truth
  for the subset it covers.
- **Not exhaustive of microbial diversity.** Specialist pathways
  (novel CO2-fixation routes from recent metagenomics, non-canonical
  amino-acid biosyntheses, newly-described secondary metabolites) get
  added by hand or by a future `propose-catalogue --from-literature`
  driver that mines Europe PMC for pathway-review papers.

## Provenance on "these names did not come from MetaCyc"

- Reactome rows: cross-checkable against the ingested
  `data/reactome/ReactomePathways.txt` whose sha256 is pinned in
  `data/reactome/SOURCE.toml`.
- Microbial rows: hand-authored from textbook terminology. Every
  entry uses a name that appears in at least one of:
    - Madigan et al., *Brock Biology of Microorganisms*
    - Lehninger, *Principles of Biochemistry*
    - White et al., *The Physiology and Biochemistry of Prokaryotes*
    - Primary literature reviews in the Europe PMC corpus.
  None of the names use MetaCyc-distinctive conventions
  (`PWY-<n>` identifiers, the "superpathway of …" prefix, MetaCyc's
  organism-tagged variants like `PWY66-400`).

The licence-lint (`cargo test -p gapsmith-db-ingest licence_lint`)
scans tracked files for the banned source strings; both TSVs pass.

## Intended use

The batch driver (`gapsmith-db propose-catalogue`, forthcoming) will
iterate `cat reactome.tsv microbial.tsv`, call the proposer per row,
and write each proposal to `proposals/<proposal_id>.json`. Curator
throughput remains the bottleneck; plan to auto-accept proposals that
pass all nine verifiers *and* reference only reactions already in the
ingested Rhea/MNXref reaction set (the LLM merely re-assembled known
pieces), and queue the rest for human review.

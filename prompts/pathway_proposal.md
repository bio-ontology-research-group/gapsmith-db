# gapsmith-db pathway proposal prompt (v{{prompt_version}})

You propose a metabolic pathway for **{{pathway_name}}** in
**{{organism_scope}}**, assuming medium: **{{medium}}**.

## Operating rules

1. You are *proposing*; deterministic verifiers will judge your output.
2. Every reaction MUST be referenced by a **Rhea ID** OR by **EC number +
   ChEBI IDs** for substrates and products. Reaction IDs from
   restrictively licensed sources must NOT appear.
3. Every enzyme cited in the `enzymes` array MUST be a real
   **Swiss-Prot UniProt accession** (reviewed). The verifier checks
   each accession against a UniProt snapshot and rejects proposals
   with made-up IDs.
4. **Do not invent UniProt accessions.** If you are not confident a
   specific accession exists for an enzyme, omit that enzyme from
   the `enzymes` array — but keep the reaction itself. A fabricated
   accession is worse than omission.
5. Every claim MUST be supported by at least one **real PubMed ID**
   (checked against NCBI E-utilities).
6. Output MUST be a single JSON object matching the gapsmith-db
   Proposal schema. No prose outside the JSON; no markdown fences.

## Pathway completeness (important)

A proposal must cover every canonical enzymatic step of the pathway,
not a summary sketch. Well-studied microbial pathways typically have
**5 to 15 reactions**:

- Hydrogenotrophic methanogenesis: 7 steps (Fwd, Ftr, Mch, Mtd, Mer,
  Mtr, Mcr) + Hdr heterodisulfide reductase.
- Glycolysis (EMP): 10 reactions.
- TCA cycle: 8 reactions.
- Aceticlastic methanogenesis: 5–6 reactions.
- Wood-Ljungdahl pathway: ~9 reactions across the methyl and
  carbonyl branches.
- Nitrogen fixation + assimilation: 3–5 reactions.

Do **NOT** truncate a multi-step pathway to 2–3 reactions. If the
canonical pathway has 8 steps, emit 8 reaction entries — even if you
can only name UniProt accessions for some of them. The `enzymes`
array is independent of `reactions`: it's fine to have 8 reactions
and 3 enzyme entries.

Failure modes we've seen from previous models and want to avoid:

- *Pathway truncation*: naming only 2 reactions for a pathway with 8
  canonical steps.
- *Blanket enzyme omission*: emitting a full 10-reaction pathway
  with zero entries in `enzymes`, when the literature clearly
  describes at least a few Swiss-Prot-reviewed enzymes.
- *UniProt fabrication*: inventing accessions in a plausible-looking
  format (e.g. sequential blocks like `P23947, P23948, P23949`) when
  those specific accessions do not exist.

A good proposal names **all canonical reactions** and names a
**Swiss-Prot accession for each enzymatic step where one exists in
UniProt**. When an enzyme has no Swiss-Prot entry (only TrEMBL, or
no reviewed entry at all), omit just that enzyme — don't drop the
reaction.

## Accession-quality heuristics

- Swiss-Prot accessions match `[OPQ][0-9][A-Z0-9]{3}[0-9]` or
  `[A-N,R-Z][0-9]([A-Z][A-Z0-9]{2}[0-9]){1,2}`.
- **Do not extrapolate accession numbers.** UniProt accessions are
  NOT assigned by biological proximity. If you recall that a gene
  cluster in *Methanothermobacter* has accessions P23940–P23945, that
  does NOT imply P23946 or P23947 exist — they might belong to a
  completely different organism, or not exist at all. Emit only
  accessions you can independently recall, not "the next slot" in an
  apparent sequence.
- Well-studied organisms (*E. coli*, *S. cerevisiae*, *M. tuberculosis*,
  *Methanosarcina acetivorans*, *Methanothermobacter marburgensis*,
  *Methanocaldococcus jannaschii*) have broad Swiss-Prot coverage —
  cite specific accessions from them when possible. For a canonical
  pathway the catalytic subunits of most enzymes are reviewed.
- For an obscure organism or a poorly-characterised enzyme, omit
  just that enzyme from the `enzymes` array and keep the reaction.
  Do NOT substitute an accession from a loosely-related organism.

## Additional notes

{{notes}}

## Retrieved literature passages

{{passages_block}}

## Output shape

```json
{
  "schema_version": "1",
  "proposal_id": "",
  "created_at": "",
  "model": "",
  "prompt_version": "{{prompt_version}}",
  "target": {
    "pathway_name": "{{pathway_name}}",
    "organism_scope": "{{organism_scope}}",
    "medium": "{{medium}}"
  },
  "reactions": [
    {"local_id": "R1", "reference": {"rhea": "22636"}, "reversibility": "forward"},
    {"local_id": "R2", "reference": {"rhea": "24384"}, "reversibility": "forward"},
    {"local_id": "R3", "reference": {"rhea": "15677"}, "reversibility": "forward"},
    {"local_id": "R4", "reference": {"rhea": "15953"}, "reversibility": "forward"},
    {"local_id": "R5", "reference": {"rhea": "16233"}, "reversibility": "forward"},
    {"local_id": "R6", "reference": {
      "chebi_ec": {
        "ec": "1.2.7.12",
        "substrates": ["CHEBI:16526", "CHEBI:17805"],
        "products":   ["CHEBI:58435"]
      }
    }, "reversibility": "forward"}
  ],
  "enzymes": [
    {"uniprot": "P23940", "catalyses": ["R1"], "function": "Formylmethanofuran dehydrogenase subunit A"},
    {"uniprot": "P27999", "catalyses": ["R4"], "function": "Methyl-coenzyme M reductase alpha subunit"}
  ],
  "dag": [
    {"from": "R1", "to": "R2"},
    {"from": "R2", "to": "R3"},
    {"from": "R3", "to": "R4"},
    {"from": "R4", "to": "R5"},
    {"from": "R5", "to": "R6"}
  ],
  "citations": [
    {"pmid": "24123366", "note": "review of the pathway"},
    {"pmid": "27159581", "note": "structural / mechanistic detail"}
  ],
  "rationale": "one paragraph on why this pathway fits the target"
}
```

Leave `proposal_id` and `created_at` as empty strings; gapsmith-db
fills them in. For the `enzymes` array, use fields exactly as shown:
`uniprot` (string, real Swiss-Prot accession), `catalyses` (array of
local reaction IDs like `"R1"`), `function` (optional short label).
Any other field name breaks validation.

The example above shows 6 reactions and 2 enzyme entries — this
asymmetry is intentional: name enzymes only for the steps where you
know a Swiss-Prot accession, but **always list every canonical
reaction**.

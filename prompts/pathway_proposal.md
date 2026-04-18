# gapsmith-db pathway proposal prompt (v{{prompt_version}})

You propose a metabolic pathway for **{{pathway_name}}** in
**{{organism_scope}}**, assuming medium: **{{medium}}**.

## Operating rules

1. You are *proposing*; deterministic verifiers will judge your output.
2. Every reaction MUST be referenced by a **Rhea ID** OR by **EC number +
   ChEBI IDs** for substrates and products. Reaction IDs from
   restrictively licensed sources must NOT appear.
3. Every enzyme MUST be a real **Swiss-Prot UniProt accession**
   (reviewed). The verifier checks each accession against a UniProt
   snapshot and rejects proposals with made-up IDs.
4. **Do not invent accessions.** If you are not confident a specific
   UniProt accession exists for an enzyme, omit that enzyme from the
   `enzymes` array. The `enzymes` field is optional (empty array is
   fine) and curators can fill in gaps. Emitting a plausible-looking
   but fabricated accession is worse than omission.
5. Every claim MUST be supported by at least one **real PubMed ID**
   (checked against NCBI E-utilities).
6. Output MUST be a single JSON object matching the gapsmith-db
   Proposal schema. No prose outside the JSON; no markdown fences.

## Accession-quality heuristics

- Swiss-Prot accessions match `[OPQ][0-9][A-Z0-9]{3}[0-9]` or
  `[A-N,R-Z][0-9]([A-Z][A-Z0-9]{2}[0-9]){1,2}`.
- Well-studied organisms (*E. coli*, *S. cerevisiae*, *M. tuberculosis*,
  *Methanosarcina acetivorans*, *Methanothermobacter marburgensis*,
  *Methanocaldococcus jannaschii*) have good UniProt coverage — cite
  specific accessions from them when possible.
- For an obscure organism or a poorly-characterised enzyme, emit an
  empty `enzymes` array and note the gap in `rationale`. Do NOT
  substitute an accession from a loosely related organism.

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
    {
      "local_id": "R1",
      "reference": {"rhea": "22636"},
      "reversibility": "forward"
    },
    {
      "local_id": "R2",
      "reference": {
        "chebi_ec": {
          "ec": "1.2.7.12",
          "substrates": ["CHEBI:16526", "CHEBI:17805"],
          "products": ["CHEBI:58435"]
        }
      }
    }
  ],
  "enzymes": [
    {"uniprot": "P23940", "catalyses": ["R1"], "function": "Formylmethanofuran dehydrogenase subunit A"}
  ],
  "dag": [{"from": "R1", "to": "R2"}],
  "citations": [{"pmid": "24123366", "note": "describes the pathway"}],
  "rationale": "one paragraph on why this pathway fits the target"
}
```

Leave `proposal_id` and `created_at` as empty strings; gapsmith-db
fills them in. For the `enzymes` array, use fields exactly as shown:
`uniprot` (string, real Swiss-Prot accession), `catalyses` (array of
local reaction IDs like `"R1"`), `function` (optional short label).
Any other field name breaks validation. Only populate an enzyme entry
if you can stand behind the accession — an empty array (`"enzymes":
[]`) is a valid and accepted output when uncertain.

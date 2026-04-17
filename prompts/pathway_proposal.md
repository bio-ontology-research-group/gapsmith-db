# gapsmith-db pathway proposal prompt (v{{prompt_version}})

You propose a metabolic pathway for **{{pathway_name}}** in
**{{organism_scope}}**, assuming medium: **{{medium}}**.

## Operating rules

1. You are _proposing_; deterministic verifiers will judge your output.
2. Every reaction MUST be referenced by a **Rhea ID** OR by **EC number +
   ChEBI IDs** for substrates and products. Reaction IDs from restrictively
   licensed sources must NOT appear.
3. Every enzyme MUST be a **Swiss-Prot UniProt accession** (reviewed).
4. Every claim MUST be supported by at least one **PubMed ID**.
5. Output MUST be a single JSON object matching the gapsmith-db Proposal
   schema. No prose outside the JSON; no markdown fences.

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
      "reference": {"rhea": "10020"},
      "reversibility": "forward"
    },
    {
      "local_id": "R2",
      "reference": {
        "chebi_ec": {
          "ec": "1.12.98.1",
          "substrates": ["CHEBI:15378"],
          "products": ["CHEBI:16183"]
        }
      }
    }
  ],
  "enzymes": [
    {"uniprot": "P0C3Z1", "catalyses": ["R1"], "function": "hydrogenase"}
  ],
  "dag": [{"from": "R1", "to": "R2"}],
  "citations": [{"pmid": "9461540", "note": "describes pathway"}],
  "rationale": "one paragraph on why this pathway fits the target"
}
```

Leave `proposal_id` and `created_at` as empty strings; gapsmith-db fills
them in.

# data/

One subdirectory per upstream source. Each subdirectory contains a
`SOURCE.toml` declaring the upstream URL, licence, pinned
commit/release, SHA256 of the retrieved archive, and attribution.

Raw artefacts are never committed to the repository — only the
`SOURCE.toml` pin and, after a successful fetch, a `MANIFEST.json` with
`{source, version, retrieved_at, sha256, url}`. See `../PROVENANCE.md`
for the pinning and hash-verification policy.

## Sources

| Dir          | Source                  | Licence         | Pin type     |
|--------------|-------------------------|-----------------|--------------|
| `modelseed/` | ModelSEEDDatabase       | CC-BY-4.0       | Git commit   |
| `mnxref/`    | MetaNetX / MNXref       | CC-BY-4.0       | Release      |
| `rhea/`      | Rhea                    | CC-BY-4.0       | Release no.  |
| `chebi/`     | ChEBI                   | CC-BY-4.0       | Release      |
| `intenz/`    | IntEnz                  | CC-BY-4.0       | Release      |
| `uniprot/`   | UniProtKB/Swiss-Prot    | CC-BY-4.0       | Release      |
| `reactome/`  | Reactome                | CC-BY-4.0       | Release      |
| `gapseq/`    | gapseq `dat/`           | GPL-3.0-or-later| Git commit   |

## Hard exclusions

No MetaCyc. No BioCyc. Not in code, data, corpus, training, retrieval,
or prompts. See `../LICENSING.md`.

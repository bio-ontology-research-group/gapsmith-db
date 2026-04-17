# gapsmith-bridge

Python bio-ecosystem glue for [gapsmith-db](../). Invoked from the Rust core
as a subprocess or via PyO3.

## Scope

- `cobra` — universal-model FBA, ATP-cycle regression, pathway flux test.
- `equilibrator-api` — ΔG estimates for `ThermodynamicFeasibility`.
- `requests` — PubMed / Europe PMC / UniProt REST clients.
- `pydantic` — typed wire-level schemas shared with Rust via JSON.

Do not reimplement in Rust what exists mature here.

## Setup

```sh
uv sync --extra dev
uv run pytest
```

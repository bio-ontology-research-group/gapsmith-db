# ADR 0005: Universal SBML is written through cobrapy, not a Rust writer

- **Status**: accepted
- **Date**: 2026-04-17
- **Phase**: 5 (follow-up)

## Context

The ATP-cycle verifier (plan.md Phase 3, item 7) requires a universal
metabolic model in SBML. cobrapy consumes SBML Level 3 v1 + fbc v2 and
uses libsbml under the hood. We already invoke cobrapy for the
`atp_cycle` and `pathway_flux` verifiers via the Python subprocess
bridge, so a cobra.Model is already in memory on the Python side of
the boundary during those checks.

A Rust SBML writer would be a large secondary project. libsbml-rs
exists but links against the C++ libsbml binary and carries its
build-system complexity; fbc annotations in particular would need
hand-rolled support. Reimplementation of a mature specification is the
worst kind of yak-shaving.

plan.md explicitly says: "Bio ecosystem glue (cobrapy, equilibrator,
PubMed, Europe PMC): Python, invoked as subprocess tools or via PyO3.
Do not reimplement in Rust what exists mature in Python."

## Decision

Add a `build_universal` action to the Python bridge
(`python/src/gapsmith_bridge/verify.py`). The Rust side (new module
`crates/gapsmith-db-verify/src/universal_model.rs`) converts the
canonical `Database` into a JSON payload of compounds + reactions and
hands it to the bridge; the Python handler constructs a
`cobra.Model`, optionally synthesises an `ATPM` reaction, and calls
`cobra.io.write_sbml_model`.

A corresponding CLI surface — `gapsmith-db universal {build,
pin-atp-cycle, check-atp-cycle}` — exposes the workflow. The CLI also
owns the regression-pin format at `verify/baselines/atp_cycle_*.json`.

## Consequences

- No C/C++ dependency in the Rust tree. The cost is a Python subprocess
  call per SBML write, which is negligible at release cadence.
- cobra's SBML writer is not byte-stable (timestamps, UUIDs, element
  ordering). The pin file records both `atp_flux` and `model_sha256`,
  but only `atp_flux` is load-bearing for drift detection;
  `model_sha256` is informational.
- Compound IDs in the emitted SBML follow BiGG's `<id>_<compartment>`
  convention, since cobra's FBA machinery assumes it. The conversion
  lives in `universal_model::metabolite_id`.
- The ATPM synthesis requires explicit canonical IDs for ATP/ADP/Pi/H2O/H+.
  Those IDs are reassigned by the ingest's dedup pass, so the
  `--atpm-ids` flag must be populated from a real DB inspection, not
  assumed. Phase-6 option: add a cross-reference resolver so
  `--atpm-ids "atp=seed:cpd00002"` works.

## Alternatives considered

- **Hand-roll SBML in Rust.** Rejected — specification scope + fbc2 is
  the kind of thing a small library should not own.
- **libsbml-rs via C++ bindings.** Rejected — adds a native build-system
  dependency and the C++ linkage is awkward to distribute.
- **PyO3 instead of subprocess.** Rejected for the same reason as the
  verifier bridge: subprocess isolation keeps the Python failure modes
  (import errors, cobra's scipy deps) from destabilising the Rust
  binary.

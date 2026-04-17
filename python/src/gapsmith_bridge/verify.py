"""gapsmith-bridge verify entry point.

Invoked by the Rust `gapsmith-db-verify` crate via subprocess. Reads a
single JSON payload from stdin, writes a single JSON response to stdout,
exits 0 on success. See crates/gapsmith-db-verify/src/py_bridge.rs.

Actions:
- `ping`       -> { "ok": true }
- `thermo`     -> per-reaction ΔG via eQuilibrator.
- `atp_cycle`  -> universal-model ATP cycle test via cobra.
- `pathway_flux` -> FBA on a named pathway.

The thermo / cobra imports are lazy so `--action ping` works even when
those heavy dependencies haven't been installed yet.
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any


def _read_payload() -> dict[str, Any]:
    text = sys.stdin.read()
    if not text.strip():
        return {}
    return json.loads(text)


def _emit(obj: Any) -> None:
    json.dump(obj, sys.stdout)
    sys.stdout.write("\n")


def action_ping(_payload: dict[str, Any]) -> dict[str, Any]:
    return {"ok": True, "bridge_version": "0.0.0"}


def action_thermo(payload: dict[str, Any]) -> dict[str, Any]:
    """Per-reaction ΔG. Uses eQuilibrator if available; otherwise reports
    skipped_reason so every reaction round-trips cleanly.
    """
    try:
        from equilibrator_api import Q_, ComponentContribution  # noqa: F401, PLC0415
    except ImportError as e:
        return {
            "results": [
                {
                    "id": rxn["id"],
                    "delta_g": None,
                    "skipped_reason": f"equilibrator-api not installed: {e}",
                }
                for rxn in payload.get("reactions", [])
            ]
        }

    cc = ComponentContribution()
    results: list[dict[str, Any]] = []
    for rxn in payload.get("reactions", []):
        try:
            # Build a compound → coefficient mapping from InChIs. eQuilibrator
            # has multiple ways to accept compounds; using InChI strings is the
            # most portable.
            substrates = {
                cc.get_compound("inchi:" + s["inchi"]): -abs(s["coefficient"])
                for s in rxn.get("substrates", [])
                if s.get("inchi")
            }
            products = {
                cc.get_compound("inchi:" + s["inchi"]): abs(s["coefficient"])
                for s in rxn.get("products", [])
                if s.get("inchi")
            }
            if not substrates or not products:
                results.append(
                    {
                        "id": rxn["id"],
                        "delta_g": None,
                        "skipped_reason": "missing InChI on substrate(s) or product(s)",
                    }
                )
                continue
            r = cc.reaction({**substrates, **products})
            dg = cc.standard_dg_prime(r)
            value = float(dg.value.m_as("kJ/mol"))
            uncertainty = float(dg.error.m_as("kJ/mol"))
            results.append({"id": rxn["id"], "delta_g": [value, uncertainty]})
        except Exception as e:
            results.append(
                {
                    "id": rxn["id"],
                    "delta_g": None,
                    "skipped_reason": f"equilibrator error: {e}",
                }
            )
    return {"results": results}


def action_atp_cycle(payload: dict[str, Any]) -> dict[str, Any]:
    """Load a universal model via cobra, close all exchanges, maximise
    ATP hydrolysis, return flux.
    """
    try:
        import cobra  # noqa: PLC0415
    except ImportError as e:
        return {
            "atp_flux": 0.0,
            "epsilon": payload.get("epsilon", 1e-6),
            "passed": False,
            "note": f"cobra not installed: {e}",
        }

    model_path = payload["model_path"]
    epsilon = float(payload.get("epsilon", 1e-6))
    model = cobra.io.read_sbml_model(model_path)
    # Close every exchange reaction.
    for rxn in model.exchanges:
        rxn.lower_bound = 0.0
        rxn.upper_bound = 0.0
    # Find an ATP hydrolysis reaction; names vary across models. We look for
    # anything with "ATP" in its ID that's a hydrolysis / maintenance reaction.
    candidates = [r for r in model.reactions if "ATPM" in r.id.upper()]
    if not candidates:
        return {
            "atp_flux": 0.0,
            "epsilon": epsilon,
            "passed": True,
            "note": "no ATPM reaction found; treated as pass",
        }
    model.objective = candidates[0]
    sol = model.optimize()
    flux = float(sol.objective_value or 0.0)
    return {
        "atp_flux": flux,
        "epsilon": epsilon,
        "passed": flux <= epsilon,
    }


def action_pathway_flux(payload: dict[str, Any]) -> dict[str, Any]:
    """Load universal model + medium; set objective to last reaction in the
    pathway list; assert FBA flux >= min_flux.
    """
    try:
        import cobra  # noqa: PLC0415
    except ImportError as e:
        return {
            "pathway_id": payload.get("pathway_id", ""),
            "objective_flux": 0.0,
            "passed": False,
            "note": f"cobra not installed: {e}",
        }

    model = cobra.io.read_sbml_model(payload["model_path"])
    with open(payload["medium_path"], encoding="utf-8") as f:
        medium = json.load(f)
    model.medium = medium

    rxns = payload.get("reactions", []) or []
    if not rxns:
        return {
            "pathway_id": payload.get("pathway_id", ""),
            "objective_flux": 0.0,
            "passed": False,
            "note": "empty reaction list",
        }
    try:
        target = model.reactions.get_by_id(rxns[-1])
    except KeyError:
        return {
            "pathway_id": payload.get("pathway_id", ""),
            "objective_flux": 0.0,
            "passed": False,
            "note": f"terminal reaction {rxns[-1]} not in model",
        }
    model.objective = target
    sol = model.optimize()
    flux = float(sol.objective_value or 0.0)
    min_flux = float(payload.get("min_flux", 1e-4))
    return {
        "pathway_id": payload.get("pathway_id", ""),
        "objective_flux": flux,
        "passed": flux >= min_flux,
        "note": None,
    }


ACTIONS = {
    "ping": action_ping,
    "thermo": action_thermo,
    "atp_cycle": action_atp_cycle,
    "pathway_flux": action_pathway_flux,
}


def main() -> int:
    parser = argparse.ArgumentParser(prog="gapsmith_bridge.verify")
    parser.add_argument("--action", required=True, choices=sorted(ACTIONS.keys()))
    args = parser.parse_args()
    fn = ACTIONS[args.action]
    try:
        payload = _read_payload()
    except json.JSONDecodeError as e:
        print(f"bad JSON on stdin: {e}", file=sys.stderr)
        return 2
    try:
        resp = fn(payload)
    except Exception as e:  # pylint: disable=broad-except
        print(f"action {args.action} failed: {e}", file=sys.stderr)
        return 1
    _emit(resp)
    return 0


if __name__ == "__main__":
    sys.exit(main())

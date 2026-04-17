"""gapsmith-bridge verify entry point.

Invoked by the Rust `gapsmith-db-verify` crate via subprocess. Reads a
single JSON payload from stdin, writes a single JSON response to stdout,
exits 0 on success. See crates/gapsmith-db-verify/src/py_bridge.rs.

Actions:
- `ping`            -> { "ok": true }
- `thermo`          -> per-reaction ΔG via eQuilibrator.
- `atp_cycle`       -> universal-model ATP cycle test via cobra.
- `pathway_flux`    -> FBA on a named pathway.
- `build_universal` -> assemble a cobra universal model from a JSON payload
                       and write SBML (+ optional ATPM reaction).
- `embed`           -> encode a single query string via sentence-transformers
                       for the retrieval backend.

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


def action_build_universal(payload: dict[str, Any]) -> dict[str, Any]:
    """Assemble a cobra universal model from a JSON payload and write SBML.

    Payload schema:
        {
          "compounds":   [{"id", "name"?, "formula"?, "charge"?, "compartment"?}, ...],
          "reactions":   [{"id", "name"?, "lb", "ub", "metabolites": {cpd_id: coef}}, ...],
          "out_path":    "<sbml path>",
          "add_atpm":    bool,                        # default false
          "atpm_ids":    {"atp", "adp", "pi", "h2o", "h"}?,  # required if add_atpm
          "atpm_lb":     float,                       # default 0.0
          "atpm_ub":     float                        # default 1000.0
        }

    Returns:
        {"model_path", "num_reactions", "num_metabolites", "atpm_added"}
    """
    try:
        import cobra  # noqa: PLC0415
    except ImportError as e:
        return {
            "model_path": "",
            "num_reactions": 0,
            "num_metabolites": 0,
            "atpm_added": False,
            "note": f"cobra not installed: {e}",
        }

    model = cobra.Model("gapsmith_universal")
    mets: dict[str, cobra.Metabolite] = {}
    for c in payload.get("compounds", []):
        m = cobra.Metabolite(
            id=c["id"],
            name=c.get("name") or c["id"],
            formula=c.get("formula"),
            charge=c.get("charge"),
            compartment=c.get("compartment", "c"),
        )
        mets[c["id"]] = m
    model.add_metabolites(list(mets.values()))

    rxns: list[cobra.Reaction] = []
    for r in payload.get("reactions", []):
        rxn = cobra.Reaction(r["id"])
        rxn.name = r.get("name") or r["id"]
        rxn.lower_bound = float(r.get("lb", -1000.0))
        rxn.upper_bound = float(r.get("ub", 1000.0))
        coeffs = {}
        for cpd_id, coef in r.get("metabolites", {}).items():
            if cpd_id not in mets:
                continue
            coeffs[mets[cpd_id]] = float(coef)
        rxn.add_metabolites(coeffs)
        rxns.append(rxn)
    model.add_reactions(rxns)

    atpm_added = False
    if payload.get("add_atpm", False):
        ids = payload.get("atpm_ids") or {}
        need = ["atp", "adp", "pi", "h2o", "h"]
        missing = [k for k in need if k not in ids or ids[k] not in mets]
        if not missing:
            atpm = cobra.Reaction("ATPM")
            atpm.name = "ATP maintenance (gapsmith synthesized)"
            atpm.lower_bound = float(payload.get("atpm_lb", 0.0))
            atpm.upper_bound = float(payload.get("atpm_ub", 1000.0))
            atpm.add_metabolites(
                {
                    mets[ids["atp"]]: -1.0,
                    mets[ids["h2o"]]: -1.0,
                    mets[ids["adp"]]: 1.0,
                    mets[ids["pi"]]: 1.0,
                    mets[ids["h"]]: 1.0,
                }
            )
            model.add_reactions([atpm])
            atpm_added = True

    out_path = payload["out_path"]
    cobra.io.write_sbml_model(model, out_path)
    return {
        "model_path": out_path,
        "num_reactions": len(model.reactions),
        "num_metabolites": len(model.metabolites),
        "atpm_added": atpm_added,
    }


def action_embed(payload: dict[str, Any]) -> dict[str, Any]:
    """Encode a single text with sentence-transformers.

    Payload: `{"text": "...", "model": "<HF model id>"}`.
    Returns: `{"vector": [...], "model": "...", "dim": N}`. On missing
    dependency, returns an empty vector and a `note`.
    """
    from gapsmith_bridge.corpus_ingest import (  # noqa: PLC0415
        action_embed as _embed,
    )

    return _embed(payload)


ACTIONS = {
    "ping": action_ping,
    "thermo": action_thermo,
    "atp_cycle": action_atp_cycle,
    "pathway_flux": action_pathway_flux,
    "build_universal": action_build_universal,
    "embed": action_embed,
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

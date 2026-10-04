#!/usr/bin/env python3
"""What the flow checker's strict loader reads from each of the design's
modules, as one JSON line a module: the reference the Rust core's loader is
held to (ADR-0132 decision 2, ADR-0153).

Each line names the module's file and, for each enumeration its members and
for each type its attributes, derived attributes, states, conditions,
invariants and transitions, each transition with its kind, its source and
target states, its guards and their enforcement, and its inputs.
"""
import importlib.util
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

_spec = importlib.util.spec_from_file_location("check_flows", ROOT / "scripts/check-flows.py")
flows = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(flows)


def names(v):
    if v is None:
        return []
    return [v] if isinstance(v, str) else list(v)


def summary(doc):
    types = {}
    for tn, t in (doc.get("types") or {}).items():
        transitions = {}
        for xn, x in (t.get("transitions") or {}).items():
            transitions[xn] = {
                "kind": x["kind"],
                "from": x["from"] if x.get("from") == "any" else names(x.get("from")),
                "to": names(x.get("to")),
                "guards": dict(x.get("guards") or {}),
                "required_inputs": names(x.get("required_inputs")),
                "optional_inputs": names(x.get("optional_inputs")),
                "inputs": list(x.get("inputs") or {}),
                "only_via": names(x.get("only_via")),
            }
        types[tn] = {
            "attributes": list(t.get("attributes") or {}),
            "derived": list(t.get("derived_attributes") or {}),
            "states": list(t.get("states") or {}),
            "conditions": list(t.get("conditions") or {}),
            "invariants": list(t.get("invariants") or {}),
            "transitions": transitions,
        }
    return {"module": doc["module"], "enumerations": {k: list(v) for k, v in (doc.get("enumerations") or {}).items()},
            "types": types}


def main():
    for path in sorted((ROOT / "docs/design/flow-format/examples").glob("*.yaml")):
        if path.name.endswith(".examples.yaml"):
            continue
        print(json.dumps({"file": path.name, "summary": summary(flows.load(path.read_text()))}))
    return 0


if __name__ == "__main__":
    sys.exit(main())

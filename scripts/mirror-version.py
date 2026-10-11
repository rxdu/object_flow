#!/usr/bin/env python3
"""Write the go-live version of a module: the version a store is first
published with, whose families another system's records are imported into as
mirrors, before a second publish, the module itself, cuts them over
(`publish-and-import.md` §7, ADR-0173 decision 4).

Each type named, or every type where none is, is declared as a mirror is
(`flow-format.md` §4.15): its attributes, states, invariants, derived
attributes, observations and metrics kept, every transition but an erasure
dropped with the conditions only those read, and the external identifier
`legacy_key` added where it declares none, as `replay-scenarios.py`'s
`port_version` ports a whole module. A type joined to one named is a mirror
too, since a mirror may not reference, extend or compose with a type the
store owns: what a mirror references, its bases and subtypes, and its whole
and parts. Each type added for that is said on stderr, and so is each
transition of a type kept owned that creates or calls a mirror, which the
checker refuses (check 53).

Usage: scripts/mirror-version.py <module.yaml> [<Type> ...] > <module>-golive.yaml
       the module read from standard input where its file is given as -

The YAML is written in block style without the module's comments; check it
with scripts/check-flows.py, and the module against it with
`--previous=<module>-golive.yaml`.
"""

import importlib.util
import pathlib
import re
import sys

import yaml

ROOT = pathlib.Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("replay_scenarios", ROOT / "scripts/replay-scenarios.py")
rs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rs)


def joined(types, tn):
    """The types a mirror of tn must bring with it: what it references, its base and subtypes, its whole and parts."""
    t = types.get(tn) or {}
    out = set()
    for spec_ in (t.get("attributes") or {}).values():
        ref = (spec_ or {}).get("reference")
        if ref:
            out.add(ref.rstrip("[]"))
    if t.get("extends"):
        out.add(t["extends"])
    out |= {sn for sn, s in types.items() if s.get("extends") == tn}
    # a whole composing tn
    for wn, w in types.items():
        for spec_ in (w.get("attributes") or {}).values():
            if (spec_ or {}).get("aggregation") == "composite" and (spec_ or {}).get("reference", "").rstrip("[]") == tn:
                out.add(wn)
    return {n for n in out if n in types}


def counts_dropped(m, dropped):
    """Whether a metric counts a transition the mirror no longer has: a measure over one, or a formula over the
    transitions naming one; the checker refuses either (D586)."""
    if m.get("transition") in dropped:
        return True
    if m.get("source") == "transitions":
        text = " ".join(str(m.get(k) or "") for k in ("filter", "dimensions", "expression"))
        return bool(set(re.findall(r'"([a-z_][a-z0-9_]*)"', text)) & dropped)
    return False


def main(argv):
    if len(argv) < 2 or argv[1] in ("-h", "--help"):
        print(__doc__.strip(), file=sys.stdout if len(argv) >= 2 else sys.stderr)
        return 0 if len(argv) >= 2 else 2
    path = pathlib.Path(argv[1])
    doc = rs.flows.load(sys.stdin.read() if argv[1] == "-" else path.read_text())
    types = doc.get("types") or {}
    named = argv[2:] or list(types)
    unknown = [n for n in named if n not in types]
    if unknown:
        print(f"mirror-version: {path.name} declares no type {', '.join(unknown)}", file=sys.stderr)
        return 2
    mirrored, todo = set(), list(named)
    while todo:
        tn = todo.pop()
        if tn in mirrored:
            continue
        mirrored.add(tn)
        for other in sorted(joined(types, tn) - mirrored):
            if other not in named:
                print(f"mirror-version: {other} is a mirror too, joined to {tn}", file=sys.stderr)
            todo.append(other)
    ported = rs.port_version(doc, doc.get("machines") or {})
    out = dict(doc)
    out["types"] = {tn: (ported["types"][tn] if tn in mirrored else t) for tn, t in types.items()}
    if not any(tn not in mirrored for tn in types):
        out.pop("machines", None)
    # a mirror's metrics over a transition it no longer has are left out, and the combined metrics reading them
    gone = set()
    for tn in sorted(mirrored):
        t = dict(out["types"][tn])
        dropped = set(types[tn].get("transitions") or {}) - set(t.get("transitions") or {})
        kept = {}
        for mn, m in (t.get("metrics") or {}).items():
            if isinstance(m, dict) and counts_dropped(m, dropped):
                gone.add(f"{tn}.{mn}")
                print(f"mirror-version: {tn}.{mn} counts a transition {tn} does not have as a mirror, and is left "
                      f"out of this version", file=sys.stderr)
            else:
                kept[mn] = m
        if "metrics" in t:
            t["metrics"] = kept
            if not kept:
                t.pop("metrics")
            out["types"][tn] = t
    combined = dict(out.get("metrics") or {})
    while True:
        reading = [mn for mn, m in combined.items()
                   if any(str(v) in gone for v in (m.get("input_metrics") or {}).values())]
        if not reading:
            break
        for mn in reading:
            gone.add(mn)
            combined.pop(mn)
            print(f"mirror-version: the combined metric {mn} reads one left out, and is left out too", file=sys.stderr)
    if "metrics" in out:
        out["metrics"] = combined
        if not combined:
            out.pop("metrics")
    # an owned type's transition may not write a mirror (flow-format.md §4.15, check 53)
    for tn, t in types.items():
        if tn in mirrored:
            continue
        for xn, x in (t.get("transitions") or {}).items():
            for st in x.get("effect") or []:
                made = (st.get("create") or {}).get("type")
                if made in mirrored:
                    print(f"mirror-version: {tn}.{xn} creates a {made}, a mirror in this version, which the "
                          f"checker refuses; leave the transition out of it, or keep {made} owned", file=sys.stderr)
    sys.stdout.write(yaml.safe_dump(out, sort_keys=False, allow_unicode=True, width=10_000))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

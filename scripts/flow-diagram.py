#!/usr/bin/env python3
"""Draw a type's lifecycle from its flow description, as a Mermaid state diagram.

A diagram in a document is drawn from the module it shows, never by hand, and
scripts/check-flow-docs.py holds each diagram in a document to the one this
draws (ADR-0121). The drawing follows UML's state machine notation:

- an initial transition is an edge from the initial pseudostate `[*]`, and a
  final state has an edge to the final pseudostate;
- an external transition is an edge labelled with its trigger, the
  transition's name, and its denying guards in brackets, `approve [approved]`;
- `from: any` is an edge from a dashed `any status` node, as Jira's workflow
  editor draws a global transition; it stands for every state that is not
  final;
- an internal transition is listed in a compartment under the name of each
  state it may be taken in, as `↻ name`, since it changes no state, and one
  from any state in the `any status` node's;
- an assertion is a note on the first state it may put the object in, and an
  erasure is not drawn, since it changes no state;
- a state is coloured by its category, `closed` always the same colour.

Run with a module file and a type to print its diagram.
"""
import importlib.util
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
_spec = importlib.util.spec_from_file_location("check_flows", ROOT / "scripts/check-flows.py")
flows = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(flows)

# fill and stroke per category, in the order the module lists its categories;
# `closed` is the built-in category of finished work (ADR-0106)
PALETTE = [("#e8eef7", "#6b7f99"), ("#dcebfb", "#2f6fb3"), ("#fdf0d8", "#b7791f"), ("#efe4f7", "#7b4fa0"),
           ("#fbe3e3", "#b04444"), ("#e3f4f4", "#2c7a7b")]
CLOSED = ("#e2f3e5", "#2f855a")


def lifecycle(doc, type_name):
    """The type's states and transitions, its machine's included."""
    t = (doc.get("types") or {})[type_name]
    return flows.bound(t, doc.get("machines") or {})


def label(name, x, suffix=""):
    guards = [g for g, mode in (x.get("guards") or {}).items() if mode == "deny"]
    return name + (f" [{', '.join(guards)}]" if guards else "") + suffix


def diagram(doc, type_name):
    """The Mermaid source of the type's state diagram, one line per element."""
    t = lifecycle(doc, type_name)
    states = t.get("states") or {}
    live = [s for s, v in states.items() if not (v or {}).get("final")]
    out = [f"%% {doc['module']}.{type_name}", "stateDiagram-v2", "    direction LR"]
    notes, inside, anywhere = [], {s: [] for s in states}, []
    for name, x in (t.get("transitions") or {}).items():
        kind = x.get("kind")
        if kind == "initial":
            out.append(f"    [*] --> {x['to']}: {label(name, x)}")
        elif kind == "external":
            src = x["from"]
            if src == "any":
                out.append(f"    any_status --> {x['to']}: {label(name, x)}")
            else:
                out += [f"    {s} --> {x['to']}: {label(name, x)}" for s in ([src] if isinstance(src, str) else src)]
        elif kind == "internal":
            src = x["from"]
            if src == "any":
                anywhere.append(f"    any_status: ↻ {label(name, x)}")
                continue
            for s in ([src] if isinstance(src, str) else src):
                inside[s].append(f"    {s}: ↻ {label(name, x)}")
        elif kind == "assertion":
            to = x["to"] if isinstance(x["to"], list) else [x["to"]]
            notes.append(f"    note right of {to[0]}: {name} puts the object in {', '.join(to)} from any state")
    # a state with a compartment is declared with its name, which Mermaid would
    # otherwise replace with the compartment; then the initial edges, where a reader starts
    named = [f'    state "{s}" as {s}' for s in states if inside[s]]
    if anywhere or any(l.startswith("    any_status -->") for l in out):
        named.append('    state "any status" as any_status')
    out = out[:3] + named + [l for l in out[3:] if l.startswith("    [*] -->")] + [l for l in out[3:] if not l.startswith("    [*] -->")]
    out += [f"    {s} --> [*]" for s, v in states.items() if (v or {}).get("final")]
    out += [line for s in states for line in inside[s]] + anywhere
    out += notes
    categories = [c for c in (doc.get("categories") or []) if c != "closed"]
    used = []
    for s, v in states.items():
        c = (v or {}).get("category")
        if c not in used:
            used.append(c)
    for c in used:
        fill, stroke = CLOSED if c == "closed" else PALETTE[(categories.index(c) if c in categories else len(categories)) % len(PALETTE)]
        out.append(f"    classDef {c} fill:{fill},stroke:{stroke},color:#1a202c")
    for c in used:
        members = [s for s, v in states.items() if (v or {}).get("category") == c]
        out.append(f"    class {','.join(members)} {c}")
    if any(l.startswith('    state "any status"') for l in out):
        out.append("    classDef any fill:none,stroke:#8795a8,stroke-dasharray:4 3,color:#4a5568")
        out.append("    class any_status any")
    return "\n".join(out) + "\n"


def main():
    if len(sys.argv) != 3:
        print("usage: flow-diagram.py <module.yaml> <Type>", file=sys.stderr)
        return 2
    doc = flows.load(pathlib.Path(sys.argv[1]).read_text())
    print(diagram(doc, sys.argv[2]), end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())

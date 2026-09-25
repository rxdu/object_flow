#!/usr/bin/env python3
"""Check the YAML trial of the flow format (docs/design/yaml-trial/).

The trial writes the service flow as structured YAML (authoring-flows.md §3).
To show it means exactly what the checked text language means, each module is:

1. loaded with a strict loader, under which only `true` and `false` are
   booleans, so a state named NO stays a name;
2. validated against flow.schema.json, which is what an editor or an agent
   would validate against;
3. checked for names the schema cannot see: every rule a transition lists
   is a rule of its type, and none is both required and on trial;
4. converted to the text language and run through check-syntax-doc.py,
   so every implemented publish check applies unchanged.

A self-test plants one mistake per step and requires the step to catch it.
"""
import json
import pathlib
import re
import subprocess
import sys
import tempfile

import jsonschema
import yaml

ROOT = pathlib.Path(__file__).resolve().parents[1]
TRIAL = ROOT / "docs/design/yaml-trial"
CHECKER = ROOT / "scripts/check-syntax-doc.py"


class StrictLoader(yaml.SafeLoader):
    """YAML 1.1 reads yes/no/on/off as booleans; only true and false are here."""


StrictLoader.yaml_implicit_resolvers = {
    k: [r for r in v if r[0] != "tag:yaml.org,2002:bool"]
    for k, v in yaml.SafeLoader.yaml_implicit_resolvers.items()}
StrictLoader.add_implicit_resolver(
    "tag:yaml.org,2002:bool", re.compile(r"^(?:true|false)$"), list("tf"))


def load(text):
    # StrictLoader is a SafeLoader: it builds only plain data, never Python objects
    return yaml.load(text, Loader=StrictLoader)


def schema_errors(doc):
    schema = json.loads((TRIAL / "flow.schema.json").read_text())
    v = jsonschema.Draft7Validator(schema)
    return ["/".join(str(p) for p in e.absolute_path) + ": " + e.message
            for e in sorted(v.iter_errors(doc), key=lambda e: list(e.absolute_path))]


def name_errors(doc):
    out = []
    for tn, t in (doc.get("types") or {}).items():
        rules = set(t.get("rules") or {})
        used = set()
        for xn, x in (t.get("transitions") or {}).items():
            for key in ("requires", "on_trial", "flags"):
                for r in x.get(key, []):
                    used.add(r)
                    if r not in rules:
                        out.append(f"{tn}.{xn}: {key} names '{r}', which is not a rule of {tn}")
            both = set(x.get("requires", [])) & (set(x.get("on_trial", [])) | set(x.get("flags", [])))
            for r in sorted(both):
                out.append(f"{tn}.{xn}: '{r}' is both required and on trial or a flag")
        for r in sorted(rules - used):
            out.append(f"{tn}: rule '{r}' is used by no transition")
    return out


# ── conversion to the text language ────────────────────────────────────────
def field_line(name, spec, record=False):
    if isinstance(spec, str):
        return f"field {name} : {spec}" if record else f"attr {name} {spec}"
    if "ref" in spec:
        t = spec["ref"] + ("?" if spec.get("optional") else "")
        if record:
            return f"field {name} : {t}"
        return f"ref {name} : {t}" + (" assignee" if spec.get("assignee") else "")
    t = spec["type"]
    if spec.get("optional") and not t.endswith("?"):
        t += "?"
    if record:
        return (f"field {name} : {t}" + (f' unit "{spec["unit"]}"' if spec.get("unit") else "")
                + (" personal" if spec.get("personal") else ""))
    marks = ([f"actor {spec['actor']}"] if spec.get("actor") else []) + \
            [m for m in ("unique", "indexed", "personal") if spec.get(m)]
    return f"attr {name} {t}" + ("".join(" " + m for m in marks))


def states_expr(v):
    return v if isinstance(v, str) else "{ " + ", ".join(v) + " }"


def rule_line(name, rule, marking=""):
    when = " ".join(rule["when"].split())
    return (f"require {name}: {when}" + (f" {marking}" if marking else "")
            + (f" because {rule['remedy']}" if rule.get("remedy") else ""))


def to_text(doc):
    out = [f"module {doc['module']}"]
    for mod, names in (doc.get("uses") or {}).items():
        out.append(f"use   {mod}.{{{', '.join(names)}}}")
    if doc.get("categories"):
        out.append(f"category {', '.join(doc['categories'])}")
    out.append("")
    for en, members in (doc.get("enums") or {}).items():
        out.append(f"enum {en} version 1 {{ {', '.join(members)} }}")
    after = []
    for tn, t in doc["types"].items():
        out.append("")
        out.append(f"# {t['says']}")
        out.append(f"type {tn} version 1 {{")
        out.append(f"  tracking {t.get('tracks', 'record')}")
        for sn, cat in t["states"].items():
            parts = [p.strip() for p in cat.split(",")]
            out.append(f"  state {sn} category {parts[0]}" + "".join(" " + p for p in parts[1:]))
        for fn, spec in (t.get("fields") or {}).items():
            out.append("  " + field_line(fn, spec))
        for iname, inv in (t.get("invariants") or {}).items():
            out.append(f"  invariant {iname}: {' '.join(inv['when'].split())}")
        rules = t.get("rules") or {}
        for xn, x in t["transitions"].items():
            if "at" in x:
                head = f"act {xn} at {states_expr(x['at'])}"
            elif "from" in x:
                head = f"do {xn} {states_expr(x['from'])} -> {x['to']}"
            else:
                head = f"create {xn} -> {x['to']}"
            if x.get("accepts"):
                head += " accepts " + ", ".join(x["accepts"])
            if x.get("backdatable"):
                head += f" backdatable within {x['backdatable']}"
            body = [f"input {k} : {v}" for k, v in (x.get("inputs") or {}).items()]
            body += [rule_line(r, rules[r]) for r in x.get("requires", []) if r in rules]
            body += [rule_line(r, rules[r], "observe") for r in x.get("on_trial", []) if r in rules]
            body += [rule_line(r, rules[r], "flag") for r in x.get("flags", []) if r in rules]
            body += list(x.get("then") or [])
            if body:
                out.append(f"  {head} {{")
                out += ["    " + b for b in body]
                out.append("  }")
            else:
                out.append(f"  {head} {{ }}")
        out.append("}")
        for coll, rec in (t.get("records") or {}).items():
            after.append("")
            after.append(f"# {rec['says']}")
            after.append(f"observation {rec['kind']} version 1 on {tn} as {coll} {{")
            after += ["  " + field_line(fn, spec, record=True) for fn, spec in rec["fields"].items()]
            if rec.get("late_by_up_to"):
                after.append(f"  occurred within {rec['late_by_up_to']}")
            for iname, inv in (rec.get("invariants") or {}).items():
                after.append(f"  invariant {iname}: {' '.join(inv['when'].split())}")
            after.append("}")
        for mn, m in (t.get("measures") or {}).items():
            after.append("")
            after.append(f"# {m['says']}")
            after.append(f"metric {mn} version 1 {{")
            after.append(f"  from      {m['from']}")
            if m.get("by"):
                after.append("  by        " + ", ".join(f"{k} = {v}" for k, v in m["by"].items()))
            if m.get("window"):
                after.append(f"  window on {m['window']}")
            after.append(f"  value     {m['value']}")
            for fname, cond in (m.get("flags") or {}).items():
                after.append(f"  flag      {fname} when {cond}")
            after.append("}")
    return "\n".join(out + after) + "\n"


def language_findings(texts):
    """Run the text language's checker over the converted modules, in import order."""
    doc = "# yaml trial\n\n" + "".join(f"```text\n{t}```\n\n" for t in texts)
    with tempfile.TemporaryDirectory() as d:
        p = pathlib.Path(d) / "yaml-trial.md"
        p.write_text(doc)
        r = subprocess.run([sys.executable, str(CHECKER), str(p)], capture_output=True, text=True)
    return [l.strip() for l in r.stdout.splitlines() if re.match(r"^check\d+\s", l.strip())]


def check(people_text, service_text):
    found = []
    docs = []
    for label, text in (("people", people_text), ("service", service_text)):
        doc = load(text)
        found += [f"{label}: schema: {e}" for e in schema_errors(doc)]
        found += [f"{label}: names: {e}" for e in name_errors(doc)]
        docs.append(doc)
    if not found:
        found += [f"language: {e}" for e in language_findings([to_text(d) for d in docs])]
    return found, docs


def self_test(people, service):
    planted = [
        ("a misspelt key", "schema", service.replace("requires: [engineer_active] }", "require: [engineer_active] }", 1)),
        ("a rule nobody declared", "names", service.replace("requires: [engineer_active] }", "requires: [engineer_is_active] }", 1)),
        ("a rule that reads who is asking", "check64", service.replace(
            "requires: [no_unresolved_failure]", "requires: [no_unresolved_failure, mine]").replace(
            "    rules:\n", "    rules:\n      mine:\n        says: Only the assigned engineer finishes.\n"
                           "        when: engineer.login == actor.id\n        remedy: delegable\n", 1)),
    ]
    ok = True
    for name, expect, text in planted:
        assert text != service, name
        got, _ = check(people, text)
        hit = any(expect in g for g in got)
        ok &= hit
        print(f"  planted {name}: {'caught' if hit else 'MISSED'} ({got[0] if got else 'nothing'})")
    no = "states:\n  NO: live\n"
    strict, loose = load(no)["states"], yaml.safe_load(no)["states"]
    hit = "NO" in strict and False in loose
    ok &= hit
    print(f"  planted a state named NO: strict loader keeps {sorted(strict)}, "
          f"a plain YAML loader reads {sorted(loose, key=str)}")
    return ok


def main():
    people = (TRIAL / "people.yaml").read_text()
    clean = True
    for version in ("service.yaml", "service-v2.yaml"):
        found, docs = check(people, (TRIAL / version).read_text())
        print(f"{version}: {'clean' if not found else str(len(found)) + ' finding(s)'}")
        for f in found:
            print("  " + f)
        clean &= not found
        if "--emit" in sys.argv and version == "service.yaml":
            for d in docs:
                print(to_text(d))
    ok = self_test(people, (TRIAL / "service.yaml").read_text())
    sys.exit(0 if clean and ok else 1)


main()

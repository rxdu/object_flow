#!/usr/bin/env python3
"""Check the YAML trial of the flow format (docs/design/yaml-trial/).

The trial writes flows as structured YAML (authoring-flows.md §3). To show a
module means exactly what the checked text language means, each one is:

1. loaded with a strict loader, under which only `true` and `false` are
   booleans, so a state named NO stays a name;
2. validated against flow.schema.json, which is what an editor or an agent
   would validate against;
3. checked for names the schema cannot see: every rule a transition lists is
   a rule of its type, no rule is listed under two of requires, on_trial and
   flags, and every rule is used;
4. converted to the text language and run through check-syntax-doc.py, so
   every implemented publish check applies unchanged.

Every finding is reported at the YAML file and line it comes from. Rules on
trial and flags are reported as notices, as the publish report lists them.

    check-yaml-trial.py                 the trial's modules, then a self-test
    check-yaml-trial.py A.yaml B.yaml   these modules, in import order
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


def line_index(text):
    """The line of every key and list item, by its path from the root."""
    idx = {}

    def walk(node, path):
        if isinstance(node, yaml.MappingNode):
            for k, v in node.value:
                idx[path + (k.value,)] = k.start_mark.line + 1
                walk(v, path + (k.value,))
        elif isinstance(node, yaml.SequenceNode):
            for i, v in enumerate(node.value):
                idx[path + (i,)] = v.start_mark.line + 1
                walk(v, path + (i,))
    walk(yaml.compose(text, Loader=StrictLoader), ())
    return idx


def line_of(idx, path):
    path = tuple(path)
    while path and path not in idx:
        path = path[:-1]
    return idx.get(path, 1)


# ── steps 2 and 3 ──────────────────────────────────────────────────────────
def schema_errors(doc):
    schema = json.loads((TRIAL / "flow.schema.json").read_text())
    v = jsonschema.Draft7Validator(schema)
    return [(tuple(e.absolute_path), "schema", e.message)
            for e in sorted(v.iter_errors(doc), key=lambda e: list(e.absolute_path))]


LISTS = ("requires", "on_trial", "flags")


def name_errors(doc):
    out = []
    for tn, t in (doc.get("types") or {}).items():
        rules = set(t.get("rules") or {})
        used = set()
        for xn, x in (t.get("transitions") or {}).items():
            base = ("types", tn, "transitions", xn)
            seen = {}
            for key in LISTS:
                for r in x.get(key, []):
                    used.add(r)
                    if r not in rules:
                        out.append((base + (key,), "names", f"{tn}.{xn} lists '{r}' under {key}, and {tn} has no such rule"))
                    if r in seen:
                        out.append((base + (key,), "names", f"{tn}.{xn} lists '{r}' under both {seen[r]} and {key}"))
                    seen.setdefault(r, key)
        for r in sorted(rules - used):
            out.append((("types", tn, "rules", r), "names", f"rule '{r}' of {tn} is used by no transition"))
    return out


def notices(doc):
    out = []
    for tn, t in (doc.get("types") or {}).items():
        for xn, x in (t.get("transitions") or {}).items():
            for r in x.get("on_trial", []):
                out.append((("types", tn, "rules", r), "on trial", f"{r} is on trial: it records would-be refusals and enforces nothing"))
            for r in x.get("flags", []):
                out.append((("types", tn, "rules", r), "flag", f"{r} is a flag: it is reported and never refuses"))
    return out


# ── step 4: conversion to the text language ────────────────────────────────
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
    return f"attr {name} {t}" + "".join(" " + m for m in marks)


def states_expr(v):
    return v if isinstance(v, str) else "{ " + ", ".join(v) + " }"


def rule_line(name, rule, marking=""):
    when = " ".join(rule["when"].split())
    return (f"require {name}: {when}" + (f" {marking}" if marking else "")
            + (f" because {rule['remedy']}" if rule.get("remedy") else ""))


def to_text(doc):
    """The module in the text language, and for each line the YAML path it came from."""
    lines, paths = [], []

    def emit(line, path=()):
        lines.append(line)
        paths.append(tuple(path))

    emit(f"module {doc['module']}", ("module",))
    for mod, names in (doc.get("uses") or {}).items():
        emit(f"use   {mod}.{{{', '.join(names)}}}", ("uses", mod))
    if doc.get("categories"):
        emit(f"category {', '.join(doc['categories'])}", ("categories",))
    for en, members in (doc.get("enums") or {}).items():
        emit(f"enum {en} version 1 {{ {', '.join(members)} }}", ("enums", en))
    tail = []
    for tn, t in doc["types"].items():
        T = ("types", tn)
        emit(f"# {t['says']}", T)
        emit(f"type {tn} version 1 {{", T)
        emit(f"  tracking {t.get('tracks', 'record')}", T)
        for sn, cat in t["states"].items():
            parts = [p.strip() for p in cat.split(",")]
            emit(f"  state {sn} category {parts[0]}" + "".join(" " + p for p in parts[1:]), T + ("states", sn))
        for fn, spec in (t.get("fields") or {}).items():
            emit("  " + field_line(fn, spec), T + ("fields", fn))
        for iname, inv in (t.get("invariants") or {}).items():
            emit(f"  invariant {iname}: {' '.join(inv['when'].split())}", T + ("invariants", iname))
        rules = t.get("rules") or {}
        for xn, x in t["transitions"].items():
            X = T + ("transitions", xn)
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
            body = [(f"input {k} : {v}", X + ("inputs", k)) for k, v in (x.get("inputs") or {}).items()]
            for key, mark in (("requires", ""), ("on_trial", "observe"), ("flags", "flag")):
                body += [(rule_line(r, rules[r], mark), T + ("rules", r)) for r in x.get(key, []) if r in rules]
            body += [(step, X + ("then",)) for step in (x.get("then") or [])]
            if body:
                emit(f"  {head} {{", X)
                for b, p in body:
                    emit("    " + b, p)
                emit("  }", X)
            else:
                emit(f"  {head} {{ }}", X)
        emit("}", T)
        for coll, rec in (t.get("records") or {}).items():
            R = T + ("records", coll)
            tail += [(f"# {rec['says']}", R), (f"observation {rec['kind']} version 1 on {tn} as {coll} {{", R)]
            tail += [("  " + field_line(fn, spec, record=True), R + ("fields", fn)) for fn, spec in rec["fields"].items()]
            if rec.get("late_by_up_to"):
                tail.append((f"  occurred within {rec['late_by_up_to']}", R + ("late_by_up_to",)))
            tail += [(f"  invariant {n}: {' '.join(i['when'].split())}", R + ("invariants", n))
                     for n, i in (rec.get("invariants") or {}).items()]
            tail.append(("}", R))
        for mn, m in (t.get("measures") or {}).items():
            M = T + ("measures", mn)
            tail += [(f"# {m['says']}", M), (f"metric {mn} version 1 {{", M), (f"  from      {m['from']}", M + ("from",))]
            if m.get("by"):
                tail.append(("  by        " + ", ".join(f"{k} = {v}" for k, v in m["by"].items()), M + ("by",)))
            if m.get("window"):
                tail.append((f"  window on {m['window']}", M + ("window",)))
            tail.append((f"  value     {m['value']}", M + ("value",)))
            tail += [(f"  flag      {f} when {c}", M + ("flags", f)) for f, c in (m.get("flags") or {}).items()]
            tail.append(("}", M))
    for line, path in tail:
        emit(line, path)
    return "\n".join(lines) + "\n", paths


def language_errors(converted):
    """Run the text checker over the converted modules, and map each finding back."""
    doc, starts, line = "# yaml trial\n\n", [], 3
    for text, _paths in converted:
        starts.append(line + 1)
        doc += f"```text\n{text}```\n\n"
        line += text.count("\n") + 3
    with tempfile.TemporaryDirectory() as d:
        p = pathlib.Path(d) / "yaml-trial.md"
        p.write_text(doc)
        r = subprocess.run([sys.executable, str(CHECKER), str(p)], capture_output=True, text=True)
    out = []
    for l in r.stdout.splitlines():
        m = re.match(r"^\s*check(\d+)\s+line\s+(\d+)\s+(.*)$", l)
        if not m:
            continue
        n = int(m.group(2))
        i = max(k for k, s in enumerate(starts) if s <= n)
        _text, paths = converted[i]
        k = n - starts[i]
        out.append((i, paths[k] if 0 <= k < len(paths) else (), f"check {m.group(1)}", m.group(3)))
    return out


def check(files):
    """Findings and notices over modules given in import order, as (file, line, severity, code, message)."""
    found, notes, loaded = [], [], []
    for f, text in files:
        idx = line_index(text)
        doc = load(text)
        loaded.append((f, text, idx, doc))
        found += [(f, line_of(idx, p), "fatal", c, m) for p, c, m in schema_errors(doc) + name_errors(doc)]
        notes += [(f, line_of(idx, p), "notice", c, m) for p, c, m in notices(doc)]
    if not found:
        converted = [to_text(doc) for _f, _t, _i, doc in loaded]
        for i, path, code, msg in language_errors(converted):
            f, _t, idx, _d = loaded[i]
            found.append((f, line_of(idx, path), "fatal", code, msg))
    return found, notes


def report(found, notes):
    for f, line, sev, code, msg in sorted(found + notes, key=lambda x: (x[0], x[1])):
        print(f"{f}:{line:<4} {sev:<7} {code:<9} {msg}")
    print(f"{len(found)} fatal · {len(notes)} notice{'s' if len(notes) != 1 else ''}")


def self_test(people, service):
    planted = [
        ("a misspelt key", "schema", service.replace("requires: [engineer_active] }", "require: [engineer_active] }", 1)),
        ("a rule nobody declared", "names", service.replace("requires: [engineer_active] }", "requires: [engineer_is_active] }", 1)),
        ("a rule both on trial and a flag", "names", service.replace("on_trial: [photo_attached] }", "on_trial: [photo_attached], flags: [photo_attached] }")),
        ("a rule that reads who is asking", "check 64", service.replace(
            "requires: [no_unresolved_failure]", "requires: [no_unresolved_failure, mine]").replace(
            "    rules:\n", "    rules:\n      mine:\n        says: Only the assigned engineer finishes.\n"
                           "        when: engineer.login == actor.id\n        remedy: delegable\n", 1)),
    ]
    ok = True
    for name, expect, text in planted:
        assert text != service, name
        found, _ = check([("people.yaml", people), ("service.yaml", text)])
        hit = [x for x in found if x[3] == expect]
        ok &= bool(hit)
        where = f"{hit[0][0]}:{hit[0][1]} {hit[0][4]}" if hit else f"MISSED ({found[:1]})"
        print(f"  planted {name}: {'caught' if hit else 'missed'} by {expect}, at {where}")
    no = "states:\n  NO: live\n"
    strict, loose = load(no)["states"], yaml.safe_load(no)["states"]
    hit = "NO" in strict and False in loose
    ok &= hit
    print(f"  planted a state named NO: the strict loader keeps {sorted(strict)}, "
          f"a plain YAML loader reads {sorted(loose, key=str)}")
    return ok


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    if args:
        files = [(pathlib.Path(a).name, pathlib.Path(a).read_text()) for a in args]
        found, notes = check(files)
        report(found, notes)
        sys.exit(1 if found else 0)
    people = (TRIAL / "people.yaml").read_text()
    clean = True
    for version in ("service.yaml", "service-v2.yaml"):
        found, notes = check([("people.yaml", people), (version, (TRIAL / version).read_text())])
        print(f"{version}: {'clean' if not found else str(len(found)) + ' finding(s)'}, "
              f"{len(notes)} notice{'s' if len(notes) != 1 else ''}")
        for x in found:
            print(f"  {x[0]}:{x[1]} {x[3]} {x[4]}")
        clean &= not found
    ok = self_test(people, (TRIAL / "service.yaml").read_text())
    sys.exit(0 if clean and ok else 1)


main()

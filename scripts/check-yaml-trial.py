#!/usr/bin/env python3
"""Check the YAML trial of the flow format (docs/design/yaml-trial/).

The trial writes flows as structured YAML (authoring-flows.md §3): each type
lists its states, then its moves, one block per arrow of the lifecycle, then
the rules the moves name, its fields and its measures. To show a module means
exactly what the checked text language means, each one is:

1. loaded with a strict loader, under which only `true` and `false` are
   booleans, so a state named NO stays a name;
2. validated against flow.schema.json, which is what an editor or an agent
   would validate against;
3. checked for what the schema cannot see: every arrow reads and names the
   type's states, every name a move lists is a rule or a field of its type,
   no rule is listed twice and every rule is used, every value a rule names
   is a state or an enum member, and every move into a state sets each
   field that state holds;
4. converted to the text language and run through check-syntax-doc.py, so
   every implemented publish check applies unchanged.

The shape adds shorthands, and each expands into the text language:

    move: A, B -> C             do <move> { A, B } -> C;  '-> A' is a create,
                                'stays in A, B' an act
    takes: [f]                  accepts f, and, where f is optional, a rule
                                f_given that it was given
    may take: [f]               accepts f
    checks / trial / flags      require, require … observe, require … flag
    copies: {a: b}, clears: [f] set a := b, clear f
    holds: [f] on state S       invariant s_holds: state != S or f is not null
    one of: [A, B] on field f   enum F { A, B }; a bare A in a rule is F.A
    is given, is not given      is not null, is null
    median time in: S           the intervals in S, median duration
    count of: m                 the transitions along m's arrow

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


# ── reading the shape ──────────────────────────────────────────────────────
STATES = r"[A-Z][A-Z0-9_]*(?:\s*,\s*[A-Z][A-Z0-9_]*)*"
ARROW = re.compile(rf"^\s*(?P<src>{STATES})?\s*->\s*(?P<dst>[A-Z][A-Z0-9_]*)\s*$")
STAYS = re.compile(rf"^\s*stays in\s+(?P<at>{STATES})\s*$")
RULE_LISTS = ("checks", "trial", "flags")


def types(doc):
    return [(k, v) for k, v in doc.items() if k[:1].isupper() and isinstance(v, dict)]


def parse_move(text):
    """('create', [], D), ('do', [A, …], D) or ('act', [A, …], None); None if unreadable."""
    split = lambda s: [x.strip() for x in s.split(",")]
    m = ARROW.match(text or "")
    if m:
        return ("do", split(m["src"]), m["dst"]) if m["src"] else ("create", [], m["dst"])
    m = STAYS.match(text or "")
    return ("act", split(m["at"]), None) if m else None


def camel(name):
    return "".join(p.capitalize() for p in name.split("_"))


def optional(spec):
    if isinstance(spec, str):
        return spec.endswith("?")
    return bool(spec.get("optional")) or str(spec.get("type", "")).endswith("?")


def given_rules(t, x):
    """The rules `takes` generates: one per optional field, that it was given."""
    fields = t.get("fields") or {}
    return [f for f in x.get("takes", []) if f in fields and optional(fields[f])]


def inline_enums(t):
    return {f: camel(f) for f, s in (t.get("fields") or {}).items()
            if isinstance(s, dict) and "one of" in s}


# ── steps 2 and 3 ──────────────────────────────────────────────────────────
def schema_errors(doc):
    schema = json.loads((TRIAL / "flow.schema.json").read_text())
    v = jsonschema.Draft7Validator(schema)
    return [(tuple(e.absolute_path), "schema", e.message)
            for e in sorted(v.iter_errors(doc), key=lambda e: list(e.absolute_path))]


def name_errors(doc):
    out = []
    enum_owner = {}
    for tn, t in types(doc):
        states = t.get("states") or {}
        fields = t.get("fields") or {}
        rules = set(t.get("rules") or {})
        holds = {s: set((v or {}).get("holds", [])) for s, v in states.items()}
        always = {f for f, s in fields.items() if not optional(s)}
        used = set()
        arrows = {}
        for s, v in states.items():
            for f in (v or {}).get("holds", []):
                if f not in fields:
                    out.append(((tn, "states", s, "holds"), "names", f"{s} holds '{f}', and {tn} has no such field"))
        for f, name in inline_enums(t).items():
            if name in enum_owner or name in (doc.get("enums") or {}):
                out.append(((tn, "fields", f), "names", f"the values of '{f}' would be the enum {name}, which is already declared; rename the field"))
            enum_owner[name] = (tn, f)
        seen_members = {}
        for f in inline_enums(t):
            for mbr in fields[f]["one of"]:
                if mbr in states:
                    out.append(((tn, "fields", f), "names", f"'{f}' may be {mbr}, which is also a state of {tn}, so a bare {mbr} in a rule is ambiguous"))
                elif mbr in seen_members:
                    out.append(((tn, "fields", f), "names", f"'{f}' and '{seen_members[mbr]}' may both be {mbr}, so a bare {mbr} in a rule is ambiguous"))
                seen_members.setdefault(mbr, f)
        for xn, x in (t.get("moves") or {}).items():
            base = (tn, "moves", xn)
            parsed = parse_move(x.get("move"))
            if parsed is None:
                out.append((base + ("move",), "names", f"{tn}.{xn} cannot read the arrow '{x.get('move')}': "
                                                      "write '-> A', 'A -> B', 'A, B -> C' or 'stays in A, B'"))
                continue
            kind, src, dst = parsed
            for s in src + ([dst] if dst else []):
                if s not in states:
                    out.append((base + ("move",), "names", f"{tn}.{xn} names the state {s}, and {tn} has no such state"))
            arrows[xn] = [(s, dst or s) for s in src] or [(None, dst)]
            seen = {}
            for key in RULE_LISTS:
                for r in x.get(key, []):
                    used.add(r)
                    if r not in rules:
                        out.append((base + (key,), "names", f"{tn}.{xn} lists '{r}' under {key}, and {tn} has no such rule"))
                    if r in seen:
                        out.append((base + (key,), "names", f"{tn}.{xn} lists '{r}' under both {seen[r]} and {key}"))
                    seen.setdefault(r, key)
            for f in given_rules(t, x):
                if f"{f}_given" in rules:
                    out.append((base + ("takes",), "names", f"{tn}.{xn} takes '{f}', which generates the rule {f}_given, and {tn} declares a rule of that name"))
            named = [(k, f) for k in ("takes", "may take", "clears") for f in x.get(k, [])]
            named += [("copies", f) for pair in (x.get("copies") or {}).items() for f in pair]
            for k, f in named:
                if f not in fields:
                    out.append((base + (k,), "names", f"{tn}.{xn} {k} '{f}', and {tn} has no such field"))
            for f in set(x.get("takes", [])) & set(x.get("may take", [])):
                out.append((base + ("may take",), "names", f"{tn}.{xn} lists '{f}' under both takes and may take"))
            for f in x.get("may take", []):
                if f in always:
                    out.append((base + ("may take",), "names", f"{tn}.{xn} may take '{f}', which is required by its type, so the move always takes it"))
            written = set(x.get("takes", [])) | set(x.get("may take", [])) | set(x.get("copies") or {})
            for f in written & set(x.get("clears", [])):
                out.append((base + ("clears",), "names", f"{tn}.{xn} both writes and clears '{f}'"))
            out += holds_errors(tn, xn, x, kind, src, dst, holds, always)
        for r in sorted(rules - used):
            out.append(((tn, "rules", r), "names", f"rule '{r}' of {tn} is used by no move"))
        out += value_errors(doc, tn, t)
        for mn, m in (t.get("measures") or {}).items():
            out += measure_errors(tn, t, mn, m, arrows)
    return out


def value_errors(doc, tn, t):
    """Every value a rule names is a state or an enum member; the text checker
    does not yet resolve enum members (check 19 is enforced only in part)."""
    fields = t.get("fields") or {}
    states = set(t.get("states") or {})
    enums = {n: set(m) for n, m in (doc.get("enums") or {}).items()}
    enums.update({camel(f): set(fields[f]["one of"]) for f in inline_enums(t)})
    local = {m for f in inline_enums(t) for m in fields[f]["one of"]}
    own = {k: set(v.get("states") or {}) for k, v in types(doc)}
    exprs = [((tn, sec, n, "when"), r["when"]) for sec in ("rules", "invariants") for n, r in (t.get(sec) or {}).items()]
    exprs += [((tn, "records", c, "invariants", n, "when"), i["when"])
              for c, rec in (t.get("records") or {}).items() for n, i in (rec.get("invariants") or {}).items()]
    out = []
    for path, text in exprs:
        for m in re.finditer(r"\b([A-Z][A-Za-z0-9]*)\.([A-Z][A-Z0-9_]*)\b", text):
            owner, value = m.groups()
            known = enums.get(owner, own.get(owner))
            if known is not None and value not in known:
                what = "enum" if owner in enums else "type"
                out.append((path, "names", f"{path[2]} names {owner}.{value}, and the {what} {owner} has no {value}"))
        for m in re.finditer(r"(?<![.\w])[A-Z][A-Z0-9_]*(?![\w.])", text):
            if m.group(0) not in states | local:
                out.append((path, "names", f"{path[2]} names {m.group(0)}, which is no state of {tn} and no value of its fields"))
    return out


def holds_errors(tn, xn, x, kind, src, dst, holds, always):
    """Every move into a state sets each field the state holds (step 3)."""
    base = (tn, "moves", xn)
    out = []
    targets = [dst] if dst else src
    taken = set(x.get("takes", [])) | always
    kept = set.intersection(*(holds.get(s, set()) for s in src)) if src else set()
    for s in targets:
        for f in sorted(holds.get(s, set()) & set(x.get("clears", []))):
            out.append((base + ("clears",), "holds", f"{tn}.{xn} clears '{f}', which {s} holds"))
    if kind == "act":
        return out
    for f in sorted(holds.get(dst, set())):
        copied = (x.get("copies") or {}).get(f)
        if f in taken or (f in kept and f not in x.get("clears", [])):
            continue
        if copied and (copied in taken or copied in kept):
            continue
        whence = f"hold it in {', '.join(src)}" if src else "a creation starts from no state"
        out.append((base + ("move",), "holds",
                    f"{tn}.{xn} enters {dst} without setting '{f}', which {dst} holds: take it, "
                    f"copy it from a field every starting state holds, or {whence}"))
    return out


def measure_errors(tn, t, mn, m, arrows):
    base = (tn, "measures", mn)
    out = []
    if "count of" in m:
        xn = m["count of"]
        if xn not in arrows:
            return [(base + ("count of",), "names", f"measure {mn} counts '{xn}', and {tn} has no such move")]
        twins = [o for o, a in arrows.items() if o != xn and set(a) & set(arrows[xn])]
        if twins:
            out.append((base + ("count of",), "names", f"measure {mn} counts {xn}, whose arrow {', '.join(twins)} shares; "
                                                      "the states cannot tell them apart, so use the long form"))
    if "median time in" in m and m["median time in"] not in (t.get("states") or {}):
        out.append((base + ("median time in",), "names", f"measure {mn} times the state {m['median time in']}, and {tn} has no such state"))
    if isinstance(m.get("by"), list):
        for d in m["by"]:
            if d == "who" and "count of" not in m:
                out.append((base + ("by",), "names", f"measure {mn} splits by who, which only a count of a move has"))
            elif d not in ("who", "month", "week") and d not in (t.get("fields") or {}):
                out.append((base + ("by",), "names", f"measure {mn} splits by '{d}', which is neither month, week, who nor a field of {tn}"))
    return out


def notices(doc):
    out = []
    for tn, t in types(doc):
        for xn, x in (t.get("moves") or {}).items():
            for r in x.get("trial", []):
                out.append(((tn, "rules", r), "on trial", f"{r} is on trial: it records would-be refusals and enforces nothing"))
            for r in x.get("flags", []):
                out.append(((tn, "rules", r), "flag", f"{r} is a flag: it is reported and never refuses"))
    return out


# ── step 4: conversion to the text language ────────────────────────────────
def field_line(name, spec, record=False, enum=None):
    if isinstance(spec, str):
        return f"field {name} : {spec}" if record else f"attr {name} {spec}"
    if "ref" in spec:
        t = spec["ref"] + ("?" if spec.get("optional") else "")
        if record:
            return f"field {name} : {t}"
        return f"ref {name} : {t}" + (" assignee" if spec.get("assignee") else "")
    t = enum if "one of" in spec else spec["type"]
    if spec.get("optional") and not t.endswith("?"):
        t += "?"
    if record:
        return (f"field {name} : {t}" + (f' unit "{spec["unit"]}"' if spec.get("unit") else "")
                + (" personal" if spec.get("personal") else ""))
    marks = ([f"actor {spec['actor']}"] if spec.get("actor") else []) + \
            [m for m in ("unique", "indexed", "personal") if spec.get(m)]
    return f"attr {name} {t}" + "".join(" " + m for m in marks)


def expression(text, members, states):
    """The text language for a `when`: `is given` spelt out, a bare enum member qualified."""
    text = " ".join(text.split())
    text = re.sub(r"\bis not given\b", "is null", text)
    text = re.sub(r"\bis given\b", "is not null", text)

    def qualify(m):
        word = m.group(0)
        if word in members and word not in states:
            return f"{members[word]}.{word}"
        return word
    return re.sub(r"(?<![.\w])[A-Z][A-Z0-9_]*(?![\w.])", qualify, text)


def to_text(doc):
    """The module in the text language, and for each line the YAML path it came from."""
    lines, paths = [], []

    def emit(line, path=()):
        lines.append(line)
        paths.append(tuple(path))

    emit(f"module {doc['module']}", ("module",))
    for mod, names in (doc.get("uses") or {}).items():
        emit(f"use   {mod}.{{{', '.join(names)}}}", ("uses", mod))
    cats = []
    for _tn, t in types(doc):
        for v in (t.get("states") or {}).values():
            if v["category"] not in cats:
                cats.append(v["category"])
    emit(f"category {', '.join(cats)}", ())
    for en, members in (doc.get("enums") or {}).items():
        emit(f"enum {en} version 1 {{ {', '.join(members)} }}", ("enums", en))
    for tn, t in types(doc):
        for f, en in inline_enums(t).items():
            emit(f"enum {en} version 1 {{ {', '.join(t['fields'][f]['one of'])} }}", (tn, "fields", f))
    tail = []
    for tn, t in types(doc):
        T = (tn,)
        fields = t.get("fields") or {}
        enums = inline_enums(t)
        members = {m: enums[f] for f in enums for m in fields[f]["one of"]}
        states = set(t.get("states") or {})
        rules = t.get("rules") or {}

        def rule_line(name, rule, marking=""):
            return (f"require {name}: {expression(rule['when'], members, states)}"
                    + (f" {marking}" if marking else "")
                    + f" because {rule.get('remedy', 'self_serviceable')}")

        if t.get("says"):
            emit(f"# {t['says']}", T)
        emit(f"type {tn} version 1 {{", T)
        emit(f"  tracking {t.get('tracks', 'record')}", T)
        for sn, v in t["states"].items():
            emit(f"  state {sn} category {v['category']}" + (" terminal" if v.get("final") else ""), T + ("states", sn))
        for fn, spec in fields.items():
            emit("  " + field_line(fn, spec, enum=enums.get(fn)), T + ("fields", fn))
        for sn, v in t["states"].items():
            held = v.get("holds") or []
            if held:
                cond = " and ".join(f"{f} is not null" for f in held)
                cond = f"({cond})" if len(held) > 1 else cond
                emit(f"  invariant {sn.lower()}_holds: state != {sn} or {cond}", T + ("states", sn, "holds"))
        for iname, inv in (t.get("invariants") or {}).items():
            emit(f"  invariant {iname}: {expression(inv['when'], members, states)}", T + ("invariants", iname))
        for xn, x in t["moves"].items():
            X = T + ("moves", xn)
            kind, src, dst = parse_move(x["move"])
            where = src[0] if len(src) == 1 else "{ " + ", ".join(src) + " }"
            head = {"create": f"create {xn} -> {dst}", "do": f"do {xn} {where} -> {dst}", "act": f"act {xn} at {where}"}[kind]
            accepts = x.get("takes", []) + x.get("may take", [])
            if accepts:
                head += " accepts " + ", ".join(accepts)
            if x.get("backdatable"):
                head += f" backdatable within {x['backdatable']}"
            body = [(f"require {f}_given: inputs.{f} is not null because self_serviceable", X + ("takes",))
                    for f in given_rules(t, x)]
            for key, mark in (("checks", ""), ("trial", "observe"), ("flags", "flag")):
                body += [(rule_line(r, rules[r], mark), T + ("rules", r)) for r in x.get(key, []) if r in rules]
            body += [(f"set {a} := {b}", X + ("copies", a)) for a, b in (x.get("copies") or {}).items()]
            body += [(f"clear {f}", X + ("clears",)) for f in x.get("clears", [])]
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
            tail += [(f"  invariant {n}: {expression(i['when'], members, states)}", R + ("invariants", n))
                     for n, i in (rec.get("invariants") or {}).items()]
            tail.append(("}", R))
        for mn, m in (t.get("measures") or {}).items():
            tail += measure_text(tn, t, mn, m)
    for line, path in tail:
        emit(line, path)
    return "\n".join(lines) + "\n", paths


def measure_text(tn, t, mn, m):
    M = (tn, "measures", mn)
    out = ([(f"# {m['says']}", M)] if m.get("says") else []) + [(f"metric {mn} version 1 {{", M)]
    if "from" in m:
        out.append((f"  from      {m['from']}", M + ("from",)))
        if m.get("by"):
            out.append(("  by        " + ", ".join(f"{k} = {v}" for k, v in m["by"].items()), M + ("by",)))
        if m.get("window"):
            out.append((f"  window on {m['window']}", M + ("window",)))
        out.append((f"  value     {m['value']}", M + ("value",)))
        out += [(f"  flag      {f} when {c}", M + ("flags", f)) for f, c in (m.get("flags") or {}).items()]
    else:
        tracked = {f for f, s in (t.get("fields") or {}).items() if isinstance(s, dict) and s.get("assignee")}
        if "median time in" in m:
            v, time = "i", "i.entered_at"
            out.append((f"  from      i in {tn}.intervals where i.state == {tn}.{m['median time in']}", M + ("median time in",)))
            value = "median(i.duration)"
        else:
            v, time = "t", "t.occurred_at"
            kind, src, dst = parse_move(t["moves"][m["count of"]]["move"])
            pairs = [(s, dst or s) for s in src] or [(None, dst)]
            conds = [(f"t.from_state == {tn}.{a}" if a else "t.from_state is null") + f" and t.to_state == {tn}.{b}" for a, b in pairs]
            cond = conds[0] if len(conds) == 1 else " or ".join(f"({c})" for c in conds)
            out.append((f"  from      t in {tn}.transitions where {cond}", M + ("count of",)))
            value = "count()"
        dims = []
        for d in m.get("by") or []:
            dims.append(f"{d} = " + {"month": f"month({time})", "week": f"week({time})", "who": "t.actor_id"}.get(
                d, f"{v}.held({d})" if d in tracked else f"{v}.object.{d}"))
        if dims:
            out.append(("  by        " + ", ".join(dims), M + ("by",)))
        out.append((f"  window on {time}", M))
        out.append((f"  value     {value}", M))
        for k, c in m.items():
            if k.startswith("flag "):
                op, amount = c.split(" ", 1)
                out.append((f"  flag      {k[5:]} when value {'>' if op == 'over' else '<'} {amount}", M + (k,)))
    out.append(("}", M))
    return out


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
        try:
            idx = line_index(text)
            doc = load(text)
        except yaml.YAMLError as e:
            mark = getattr(e, "problem_mark", None)
            found.append((f, mark.line + 1 if mark else 1, "fatal", "yaml", " ".join(str(getattr(e, "problem", e)).split())))
            continue
        loaded.append((f, text, idx, doc))
        errors = schema_errors(doc)
        found += [(f, line_of(idx, p), "fatal", c, m) for p, c, m in errors or name_errors(doc)]
        notes += [(f, line_of(idx, p), "notice", c, m) for p, c, m in notices(doc)]
    if not found and len(loaded) == len(files):
        converted = [to_text(doc) for _f, _t, _i, doc in loaded]
        for i, path, code, msg in language_errors(converted):
            f, _t, idx, _d = loaded[i]
            found.append((f, line_of(idx, path), "fatal", code, msg))
    return found, notes


def report(found, notes):
    for f, line, sev, code, msg in sorted(found + notes, key=lambda x: (x[0], x[1])):
        print(f"{f}:{line:<4} {sev:<7} {code:<9} {msg}")
    print(f"{len(found)} fatal · {len(notes)} notice{'s' if len(notes) != 1 else ''}")


def self_test(people, service, inventory):
    def plant(text, old, new):
        assert text.count(old) >= 1, old
        return text.replace(old, new, 1)
    mine = ("    no_unresolved_failure:\n",
            "    mine:\n      says:   Only the assigned engineer finishes.\n"
            "      when:   engineer.login == actor.id\n      remedy: delegable\n    no_unresolved_failure:\n")
    planted = [
        ("a misspelt key", "schema", "service.yaml", plant(service, "      checks: [engineer_active]", "      check: [engineer_active]")),
        ("a rule nobody declared", "names", "service.yaml", plant(service, "checks: [engineer_active]", "checks: [engineer_is_active]")),
        ("a rule both on trial and a flag", "names", "service.yaml",
         plant(service, "      trial:  [photo_attached]\n", "      trial:  [photo_attached]\n      flags:  [photo_attached]\n")),
        ("a rule that reads who is asking", "check 64", "service.yaml",
         plant(plant(service, "checks: [no_unresolved_failure]", "checks: [no_unresolved_failure, mine]"), *mine)),
        ("an arrow that does not read", "names", "service.yaml", plant(service, "move:   OPEN -> WORKING", "move:   OPEN => WORKING")),
        ("a misspelt value in a rule", "names", "inventory.yaml", plant(inventory, "condition != DAMAGED", "condition != DAMAGD")),
        ("a move into a state that does not set what it holds", "holds", "inventory.yaml",
         plant(inventory, "      copies: { reserved_for: sold_to }\n", "")),
    ]
    planted.append(("a `?` inside an inline map", "yaml", "service.yaml",
                    plant(service, "    photo:    file?\n", "    photo:    { type: file? }\n")))
    ok = True
    for name, expect, f, text in planted:
        files = [("people.yaml", people), (f, text)]
        found, _ = check(files)
        hit = [x for x in found if x[3] == expect]
        ok &= bool(hit)
        where = f"{hit[0][0]}:{hit[0][1]} {hit[0][4]}" if hit else f"MISSED ({found[:1]})"
        print(f"  planted {name}: {'caught' if hit else 'missed'} by {expect}, at {where}")
    no = "answers: [YES, NO]\n"
    strict, loose = load(no)["answers"], yaml.safe_load(no)["answers"]
    hit = "NO" in strict and False in loose
    ok &= hit
    print(f"  planted an enum member NO: the strict loader keeps {strict}, a plain YAML loader reads {loose}")
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
    for version in ("people.yaml", "inventory.yaml", "inventory-v2.yaml", "service.yaml", "service-v2.yaml"):
        files = [("people.yaml", people)] + ([(version, (TRIAL / version).read_text())] if version != "people.yaml" else [])
        found, notes = check(files)
        print(f"{version}: {'clean' if not found else str(len(found)) + ' finding(s)'}, "
              f"{len(notes)} notice{'s' if len(notes) != 1 else ''}")
        for x in found:
            print(f"  {x[0]}:{x[1]} {x[3]} {x[4]}")
        clean &= not found
    ok = self_test(people, (TRIAL / "service.yaml").read_text(), (TRIAL / "inventory.yaml").read_text())
    sys.exit(0 if clean and ok else 1)


if __name__ == "__main__":
    main()

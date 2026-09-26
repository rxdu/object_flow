#!/usr/bin/env python3
"""Check the YAML trial of the flow format (docs/design/yaml-trial/).

The trial writes flows as structured YAML (authoring-flows.md §3). A module
declares its imports, its state categories, its enumerations and its types, and each type declares
its attributes, observations, states, invariants, conditions, transitions and
metrics, in that order, so that every name is defined before it is used. To
show a module means exactly what the checked text language means, each one is:

1. loaded with a strict loader, under which only `true` and `false` are
   booleans, so a value named NO stays a name, and a key written twice in
   one mapping is an error rather than silently the last one;
2. validated against flow.schema.json, which is what an editor or an agent
   would validate against;
3. checked for what the schema cannot see: sections are in order, every
   state's category is declared, and every type a type references is
   declared before it or imported; no name is one the format keeps for
   itself (flow-format.md §3); every state,
   condition and attribute a transition names exists; every condition is
   used; every value an expression names is a state or an enumeration value;
   and every transition into a state sets each attribute the state requires;
4. converted to the text language and run through check-syntax-doc.py, so
   every implemented publish check applies unchanged.

The conversion is mechanical, and nothing in the YAML has a default:

    kind: creation, to: S                  create <name> -> S
    kind: state_change, from: [A, B], to: C  do <name> { A, B } -> C
    kind: action, from: [A, B]             act <name> at { A, B }
    required_inputs: [a]                   accepts a; where a is optional, also
                                           a guard a_provided: inputs.a is not null
    optional_inputs: [a]                   accepts a
    guards: {g: enforced|observed|flagged} require g: …  [observe|flag]
    outcome: copy {from: a, to: b}         set b := a
    outcome: clear [a]                     clear a
    required_attributes: [a] on state S    invariant s_attributes_present:
                                             state != S or a is not null
    measure: median_time_in_state          the intervals in the state
    measure: transition_count              the transitions along the
                                           transition's from and to states

Every finding is reported at the YAML file and line it comes from. Observed
and flagged guards are reported as notices, as the publish report lists them.

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

MODULE_ORDER = ["module", "imports", "categories", "enumerations", "types"]
# names the text language gives a meaning in the same position (declaration-syntax.md §9.2, check 33)
NOT_A_CATEGORY = {"any", "terminal", "superseding"}
NOT_A_TRANSITION = {"any"}
# a bare name in an expression resolves to these before an attribute (declaration-syntax.md §9.2), so no attribute may take one
NOT_AN_ATTRIBUTE = {"state", "inputs", "actor", "this", "now", "referrers", "this_event"}
# keys the text language has a form for on one kind of attribute only; elsewhere they would be dropped
ONLY_ON_OBSERVATIONS = {"unit"}
ONLY_ON_TYPES = {"actor_kind", "assignee", "unique", "indexed"}
TYPE_ORDER = ["description", "tracking", "attributes", "observations", "states",
              "invariants", "conditions", "transitions", "metrics"]


class StrictLoader(yaml.SafeLoader):
    """YAML 1.1 reads yes/no/on/off as booleans and keeps the last of two equal
    keys; here only true and false are booleans, and a repeated key is an error."""

    def construct_mapping(self, node, deep=False):
        seen = set()
        for key, _value in node.value:
            k = self.construct_object(key, deep=deep)
            if k in seen:
                raise yaml.constructor.ConstructorError(
                    None, None, f"the key '{k}' appears twice in one mapping", key.start_mark)
            seen.add(k)
        return super().construct_mapping(node, deep=deep)


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


# ── reading a module ───────────────────────────────────────────────────────
def types(doc):
    return list((doc.get("types") or {}).items())


def sources(x):
    f = x.get("from")
    return [] if f is None else [f] if isinstance(f, str) else list(f)


def pairs(x):
    """The (from, to) state pairs a transition takes an object along."""
    if x["kind"] == "creation":
        return [(None, x["to"])]
    return [(s, x.get("to", s)) for s in sources(x)]


def optional(spec):
    return bool(spec.get("optional"))


def provided_guards(t, x):
    """The guards required_inputs generates: one per optional attribute."""
    attrs = t.get("attributes") or {}
    return [a for a in x.get("required_inputs", []) if a in attrs and optional(attrs[a])]


def steps(x, verb):
    return [s[verb] for s in x.get("outcome", []) if verb in s]


def copies(x):
    return [(c["from"], c["to"]) for c in steps(x, "copy")]


def cleared(x):
    return [a for group in steps(x, "clear") for a in group]


# ── steps 2 and 3 ──────────────────────────────────────────────────────────
def schema_errors(doc):
    schema = json.loads((TRIAL / "flow.schema.json").read_text())
    v = jsonschema.Draft7Validator(schema)
    out = []
    for e in sorted(v.iter_errors(doc), key=lambda e: list(e.absolute_path)):
        # a oneOf over the kinds says only that no branch matched; name the kind's rule instead
        if e.validator == "oneOf" and isinstance(e.instance, dict) and "kind" in e.instance:
            rule = {"creation": "a creation declares `to` and no `from`",
                    "state_change": "a state_change declares `from` and `to`",
                    "action": "an action declares `from` and no `to`"}.get(e.instance["kind"], e.message)
            out.append((tuple(e.absolute_path), "schema", rule))
        elif e.validator == "oneOf" and isinstance(e.instance, dict) and "measure" in e.instance:
            out.append((tuple(e.absolute_path), "schema",
                        "a median_time_in_state metric names a `state`, and a transition_count names a `transition`"))
        else:
            out.append((tuple(e.absolute_path), "schema", e.message))
    return out


def order_errors(doc):
    """Every name is defined before it is used: sections in their order, and a
    type referenced by another type declared before it or imported."""
    out = []

    def in_order(keys, order, path, where):
        top = None
        for k in keys:
            if k not in order:
                continue
            if top is not None and order.index(k) < order.index(top):
                out.append((path + (k,), "order", f"{where}'{k}' comes after '{top}'; declare "
                                                  + ", ".join(o for o in order if o in keys) + " in that order"))
            elif top is None or order.index(k) > order.index(top):
                top = k
    in_order(list(doc), MODULE_ORDER, (), "")
    declared = {n for names in (doc.get("imports") or {}).values() for n in names}
    for tn, t in types(doc):
        in_order(list(t), TYPE_ORDER, ("types", tn), f"in {tn}, ")
        refs = [(("types", tn, "attributes", a), s["reference"]) for a, s in (t.get("attributes") or {}).items() if "reference" in s]
        refs += [(("types", tn, "observations", o, "attributes", a), s["reference"])
                 for o, ob in (t.get("observations") or {}).items() for a, s in ob["attributes"].items() if "reference" in s]
        for path, r in refs:
            if r != tn and r not in declared and r in (doc.get("types") or {}):
                out.append((path, "order", f"{tn} references {r}, which is declared after it; declare {r} first"))
        declared.add(tn)
    return out


def reserved_name_errors(doc):
    """Names the format keeps for itself: text-language collisions and generated names."""
    out = []
    for i, c in enumerate(doc.get("categories") or []):
        if c in NOT_A_CATEGORY:
            out.append((("categories", i), "names", f"'{c}' cannot name a category: the text language reserves it in that position"))
    for tn, t in types(doc):
        attrs = t.get("attributes") or {}
        states = t.get("states") or {}
        owned = [(("types", tn, "attributes", a), a) for a in attrs]
        owned += [(("types", tn, "observations", o, "attributes", a), a)
                  for o, ob in (t.get("observations") or {}).items() for a in ob["attributes"]]
        for path, a in owned:
            if a in NOT_AN_ATTRIBUTE:
                out.append((path, "names", f"'{a}' cannot name an attribute: in an expression it means something else first"))
        for a, spec in attrs.items():
            for k in sorted(ONLY_ON_OBSERVATIONS & set(spec)):
                out.append((("types", tn, "attributes", a, k), "names", f"'{k}' applies only to an observation's attribute, not to {tn}.{a}"))
        for o, ob in (t.get("observations") or {}).items():
            for a, spec in ob["attributes"].items():
                for k in sorted(ONLY_ON_TYPES & set(spec)):
                    out.append((("types", tn, "observations", o, "attributes", a, k), "names",
                                f"'{k}' applies only to a type's attribute, not to the observation attribute {o}.{a}"))
        for xn in (t.get("transitions") or {}):
            if xn in NOT_A_TRANSITION:
                out.append((("types", tn, "transitions", xn), "names", f"'{xn}' cannot name a transition: the text language reserves it"))
        for c in (t.get("conditions") or {}):
            if c.endswith("_provided") and c[:-len("_provided")] in attrs:
                out.append((("types", tn, "conditions", c), "names",
                            f"'{c}' is the name of the guard generated for a required input; choose another name"))
        for i in (t.get("invariants") or {}):
            if i.endswith("_attributes_present") and i[:-len("_attributes_present")].upper() in states:
                out.append((("types", tn, "invariants", i), "names",
                            f"'{i}' is the name of the invariant generated for a state's required_attributes; choose another name"))
    return out


def name_errors(doc):
    out = order_errors(doc) + reserved_name_errors(doc)
    for tn, t in types(doc):
        states = t.get("states") or {}
        attrs = t.get("attributes") or {}
        conds = set(t.get("conditions") or {})
        required = {s: set((v or {}).get("required_attributes", [])) for s, v in states.items()}
        always = {a for a, s in attrs.items() if not optional(s)}
        used = set()
        for s, v in states.items():
            if v["category"] not in (doc.get("categories") or []):
                out.append((("types", tn, "states", s, "category"), "names",
                            f"state {s} has the category '{v['category']}', which the module does not declare in categories"))
            for a in (v or {}).get("required_attributes", []):
                if a not in attrs:
                    out.append((("types", tn, "states", s, "required_attributes"), "names",
                                f"state {s} requires the attribute '{a}', which {tn} does not declare"))
        for xn, x in (t.get("transitions") or {}).items():
            base = ("types", tn, "transitions", xn)
            for s in sources(x) + ([x["to"]] if x.get("to") else []):
                if s not in states:
                    key = "to" if s == x.get("to") else "from"
                    out.append((base + (key,), "names", f"{tn}.{xn} names the state {s}, which {tn} does not declare"))
            for g in (x.get("guards") or {}):
                used.add(g)
                if g not in conds:
                    out.append((base + ("guards", g), "names", f"{tn}.{xn} guards on '{g}', which is not a condition of {tn}"))
            named = [(k, a) for k in ("required_inputs", "optional_inputs") for a in x.get(k, [])]
            named += [("outcome", a) for pair in copies(x) for a in pair] + [("outcome", a) for a in cleared(x)]
            for k, a in named:
                if a not in attrs:
                    out.append((base + (k,), "names", f"{tn}.{xn} names the attribute '{a}' in {k}, which {tn} does not declare"))
            for a in set(x.get("required_inputs", [])) & set(x.get("optional_inputs", [])):
                out.append((base + ("optional_inputs",), "names", f"{tn}.{xn} lists '{a}' as both a required and an optional input"))
            for a in x.get("optional_inputs", []):
                if a in always:
                    out.append((base + ("optional_inputs",), "names",
                                f"{tn}.{xn} lists '{a}' as an optional input, but the attribute is not optional, so the input is required"))
            written = set(x.get("required_inputs", [])) | set(x.get("optional_inputs", [])) | {b for _a, b in copies(x)}
            for a in sorted(written & set(cleared(x))):
                out.append((base + ("outcome",), "names", f"{tn}.{xn} both writes and clears '{a}'"))
            out += required_errors(tn, xn, x, required, always)
        for c in sorted(conds - used):
            out.append((("types", tn, "conditions", c), "names", f"condition '{c}' of {tn} is used by no transition"))
        out += value_errors(doc, tn, t)
        for mn, m in (t.get("metrics") or {}).items():
            out += metric_errors(tn, t, mn, m)
    return out


def required_errors(tn, xn, x, required, always):
    """Every transition into a state sets each attribute the state requires."""
    base = ("types", tn, "transitions", xn)
    out = []
    src = sources(x)
    targets = [x["to"]] if x.get("to") else src
    for s in targets:
        for a in sorted(required.get(s, set()) & set(cleared(x))):
            out.append((base + ("outcome",), "required", f"{tn}.{xn} clears '{a}', which state {s} requires"))
    if x["kind"] == "action":
        return out
    supplied = set(x.get("required_inputs", [])) | always
    kept = set.intersection(*(required.get(s, set()) for s in src)) if src else set()
    copied = {b: a for a, b in copies(x)}
    for a in sorted(required.get(x["to"], set())):
        if a in supplied or (a in kept and a not in cleared(x)):
            continue
        if a in copied and (copied[a] in supplied or copied[a] in kept):
            continue
        how = f"or require it in state {', '.join(src)}" if src else "since a creation starts from no state"
        out.append((base + ("to",), "required",
                    f"{tn}.{xn} enters {x['to']} without setting '{a}', which {x['to']} requires: "
                    f"make it a required input, copy it from an attribute every source state requires, {how}"))
    return out


def value_errors(doc, tn, t):
    """Every value an expression names is a state or an enumeration value. The
    text checker does not yet resolve enumeration values (check 19, in part)."""
    states = set(t.get("states") or {})
    enums = {n: set(m) for n, m in (doc.get("enumerations") or {}).items()}
    own = {k: set(v.get("states") or {}) for k, v in types(doc)}
    exprs = [(("types", tn, sec, n, "expression"), n, c["expression"])
             for sec in ("conditions", "invariants") for n, c in (t.get(sec) or {}).items()]
    exprs += [(("types", tn, "observations", o, "invariants", n, "expression"), n, i["expression"])
              for o, ob in (t.get("observations") or {}).items() for n, i in (ob.get("invariants") or {}).items()]
    out = []
    for path, name, text in exprs:
        for m in re.finditer(r"\b([A-Z][A-Za-z0-9]*)\.([A-Z][A-Z0-9_]*)\b", text):
            owner, value = m.groups()
            known = enums.get(owner, own.get(owner))
            if known is not None and value not in known:
                what = "enumeration" if owner in enums else "type"
                out.append((path, "names", f"{name} names {owner}.{value}, and the {what} {owner} has no {value}"))
        for m in re.finditer(r"(?<![.\w])[A-Z][A-Z0-9_]*(?![\w.])", text):
            if m.group(0) not in states:
                out.append((path, "names", f"{name} names {m.group(0)}, which is not a state of {tn}; "
                                           "write an enumeration value qualified, as <Enumeration>.<VALUE>"))
    return out


def metric_errors(tn, t, mn, m):
    base = ("types", tn, "metrics", mn)
    trans = t.get("transitions") or {}
    out = []
    if m.get("measure") == "transition_count":
        xn = m["transition"]
        if xn not in trans:
            return [(base + ("transition",), "names", f"metric {mn} counts '{xn}', which is not a transition of {tn}")]
        twins = [o for o, x in trans.items() if o != xn and set(pairs(x)) & set(pairs(trans[xn]))]
        if twins:
            out.append((base + ("transition",), "names",
                        f"metric {mn} counts {xn}, which moves between the same states as {', '.join(twins)}; "
                        "a count by states cannot tell them apart"))
    if m.get("measure") == "median_time_in_state" and m["state"] not in (t.get("states") or {}):
        out.append((base + ("state",), "names", f"metric {mn} measures the state {m['state']}, which {tn} does not declare"))
    for d in m.get("group_by") or []:
        if d == "actor" and m.get("measure") != "transition_count":
            out.append((base + ("group_by",), "names", f"metric {mn} groups by actor, which only a transition_count has"))
        elif d not in ("actor", "month", "week") and d not in (t.get("attributes") or {}):
            out.append((base + ("group_by",), "names",
                        f"metric {mn} groups by '{d}', which is not month, week, actor or an attribute of {tn}"))
    return out


def notices(doc):
    out = []
    for tn, t in types(doc):
        for xn, x in (t.get("transitions") or {}).items():
            for g, mode in (x.get("guards") or {}).items():
                path = ("types", tn, "transitions", xn, "guards", g)
                if mode == "observed":
                    out.append((path, "observed", f"{g} is observed: its failures are recorded and it refuses nothing"))
                elif mode == "flagged":
                    out.append((path, "flagged", f"{g} is flagged: a failure is reported with the result and refuses nothing"))
    return out


# ── step 4: conversion to the text language ────────────────────────────────
def attribute_line(name, spec, observation=False):
    if "reference" in spec:
        t = spec["reference"] + ("?" if optional(spec) else "")
        if observation:
            return f"field {name} : {t}"
        return f"ref {name} : {t}" + (" assignee" if spec.get("assignee") else "")
    t = spec["type"] + ("?" if optional(spec) else "")
    if observation:
        return (f"field {name} : {t}" + (f' unit "{spec["unit"]}"' if spec.get("unit") else "")
                + (" personal" if spec.get("personal") else ""))
    marks = ([f"actor {spec['actor_kind']}"] if spec.get("actor_kind") else []) + \
            [m for m in ("unique", "indexed", "personal") if spec.get(m)]
    return f"attr {name} {t}" + "".join(" " + m for m in marks)


def to_text(doc):
    """The module in the text language, and for each line the YAML path it came from."""
    lines, paths = [], []

    def emit(line, path=()):
        lines.append(line)
        paths.append(tuple(path))

    emit(f"module {doc['module']}", ("module",))
    for mod, names in (doc.get("imports") or {}).items():
        emit(f"use   {mod}.{{{', '.join(names)}}}", ("imports", mod))
    emit(f"category {', '.join(doc['categories'])}", ("categories",))
    for en, members in (doc.get("enumerations") or {}).items():
        emit(f"enum {en} version 1 {{ {', '.join(members)} }}", ("enumerations", en))
    tail = []
    for tn, t in types(doc):
        T = ("types", tn)
        conds = t.get("conditions") or {}
        emit(f"# {t['description']}", T)
        emit(f"type {tn} version 1 {{", T)
        emit(f"  tracking {t['tracking']}", T + ("tracking",))
        for sn, v in t["states"].items():
            emit(f"  state {sn} category {v['category']}" + (" terminal" if v.get("terminal") else ""), T + ("states", sn))
        for an, spec in (t.get("attributes") or {}).items():
            emit("  " + attribute_line(an, spec), T + ("attributes", an))
        for sn, v in t["states"].items():
            req = v.get("required_attributes") or []
            if req:
                cond = " and ".join(f"{a} is not null" for a in req)
                cond = f"({cond})" if len(req) > 1 else cond
                emit(f"  invariant {sn.lower()}_attributes_present: state != {sn} or {cond}",
                     T + ("states", sn, "required_attributes"))
        for iname, inv in (t.get("invariants") or {}).items():
            emit(f"  invariant {iname}: {' '.join(inv['expression'].split())}", T + ("invariants", iname))
        for xn, x in t["transitions"].items():
            X = T + ("transitions", xn)
            src = sources(x)
            where = src[0] if len(src) == 1 else "{ " + ", ".join(src) + " }"
            head = {"creation": f"create {xn} -> {x.get('to')}", "state_change": f"do {xn} {where} -> {x.get('to')}",
                    "action": f"act {xn} at {where}"}[x["kind"]]
            accepts = x.get("required_inputs", []) + x.get("optional_inputs", [])
            if accepts:
                head += " accepts " + ", ".join(accepts)
            if x.get("backdating_limit"):
                head += f" backdatable within {x['backdating_limit']}"
            body = [(f"require {a}_provided: inputs.{a} is not null because self_serviceable", X + ("required_inputs",))
                    for a in provided_guards(t, x)]
            for g, mode in (x.get("guards") or {}).items():
                if g in conds:
                    c = conds[g]
                    mark = {"enforced": "", "observed": " observe", "flagged": " flag"}[mode]
                    body.append((f"require {g}: {' '.join(c['expression'].split())}{mark} because {c['remedy']}",
                                 X + ("guards", g)))
            for i, s in enumerate(x.get("outcome", [])):
                if "copy" in s:
                    body.append((f"set {s['copy']['to']} := {s['copy']['from']}", X + ("outcome", i)))
                else:
                    body += [(f"clear {a}", X + ("outcome", i)) for a in s["clear"]]
            if body:
                emit(f"  {head} {{", X)
                for b, p in body:
                    emit("    " + b, p)
                emit("  }", X)
            else:
                emit(f"  {head} {{ }}", X)
        emit("}", T)
        for coll, ob in (t.get("observations") or {}).items():
            O = T + ("observations", coll)
            tail += [(f"# {ob['description']}", O), (f"observation {ob['kind']} version 1 on {tn} as {coll} {{", O)]
            tail += [("  " + attribute_line(an, spec, observation=True), O + ("attributes", an))
                     for an, spec in ob["attributes"].items()]
            if ob.get("max_recording_delay"):
                tail.append((f"  occurred within {ob['max_recording_delay']}", O + ("max_recording_delay",)))
            tail += [(f"  invariant {n}: {' '.join(i['expression'].split())}", O + ("invariants", n))
                     for n, i in (ob.get("invariants") or {}).items()]
            tail.append(("}", O))
        for mn, m in (t.get("metrics") or {}).items():
            tail += metric_text(tn, t, mn, m)
    for line, path in tail:
        emit(line, path)
    return "\n".join(lines) + "\n", paths


def metric_text(tn, t, mn, m):
    M = ("types", tn, "metrics", mn)
    out = [(f"# {m['description']}", M), (f"metric {mn} version 1 {{", M)]
    tracked = {a for a, s in (t.get("attributes") or {}).items() if s.get("assignee")}
    if m["measure"] == "median_time_in_state":
        v, time, value = "i", "i.entered_at", "median(i.duration)"
        out.append((f"  from      i in {tn}.intervals where i.state == {tn}.{m['state']}", M + ("state",)))
    else:
        v, time, value = "t", "t.occurred_at", "count()"
        conds = [(f"t.from_state == {tn}.{a}" if a else "t.from_state is null") + f" and t.to_state == {tn}.{b}"
                 for a, b in pairs(t["transitions"][m["transition"]])]
        cond = conds[0] if len(conds) == 1 else " or ".join(f"({c})" for c in conds)
        out.append((f"  from      t in {tn}.transitions where {cond}", M + ("transition",)))
    dims = [f"{d} = " + {"month": f"month({time})", "week": f"week({time})", "actor": "t.actor_id"}.get(
        d, f"{v}.held({d})" if d in tracked else f"{v}.object.{d}") for d in m.get("group_by") or []]
    if dims:
        out.append(("  by        " + ", ".join(dims), M + ("group_by",)))
    out.append((f"  window on {time}", M))
    out.append((f"  value     {value}", M))
    out += [(f"  flag      {f} when {c}", M + ("flag_when", f)) for f, c in (m.get("flag_when") or {}).items()]
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
    reads_actor = ("      no_unresolved_failure:\n",
                   "      assigned_engineer_only:\n        description: Only the assigned engineer finishes the job.\n"
                   "        expression: engineer.login == actor.id\n        remedy: delegable\n      no_unresolved_failure:\n")
    head, rest = inventory.split("    attributes:\n", 1)
    attributes, after = rest.split("\n    states:\n", 1)
    late_attributes = head + "    states:\n" + after.rstrip("\n") + "\n\n    attributes:\n" + attributes + "\n"
    planted = [
        ("a misspelt key", "schema", "service.yaml", plant(service, "        guards:\n", "        guard:\n")),
        ("a guard on no declared condition", "names", "service.yaml",
         plant(service, "          engineer_active: enforced", "          engineer_is_active: enforced")),
        ("a guard listed twice", "yaml", "service.yaml",
         plant(service, "          photo_attached: observed\n", "          photo_attached: observed\n          photo_attached: flagged\n")),
        ("an enforcement that is not one of the three", "schema", "service.yaml",
         plant(service, "photo_attached: observed", "photo_attached: warn")),
        ("a creation that names a source state", "schema", "service.yaml",
         plant(service, "        kind: creation\n        to: OPEN\n", "        kind: creation\n        from: WORKING\n        to: OPEN\n")),
        ("a condition that reads who is asking", "check 64", "service.yaml",
         plant(plant(service, "          no_unresolved_failure: enforced\n",
                     "          no_unresolved_failure: enforced\n          assigned_engineer_only: enforced\n"), *reads_actor)),
        ("attributes declared after the states that use them", "order", "inventory.yaml", late_attributes),
        ("a misspelt enumeration value", "names", "inventory.yaml",
         plant(inventory, "condition != Condition.DAMAGED", "condition != Condition.DAMAGD")),
        ("a transition into a state that does not set what the state requires", "required", "inventory.yaml",
         plant(inventory, "        required_inputs: [reserved_until]\n", "        optional_inputs: [reserved_until]\n")),
        ("an attribute named after an expression built-in", "names", "inventory.yaml",
         plant(inventory, "      model:             { type: string }\n", "      model:             { type: string }\n      state:             { type: string, optional: true }\n")),
        ("an invariant named like a generated one", "names", "inventory.yaml",
         plant(inventory, "    conditions:\n", "    invariants:\n      reserved_attributes_present:\n        description: A clash with a generated name.\n"
                                              "        expression: serial is not null\n\n    conditions:\n")),
        ("a unit on an attribute that is not an observation's", "names", "inventory.yaml",
         plant(inventory, "      list_price:        { type: money(SGD) }", "      list_price:        { type: money(SGD), unit: SGD }")),
        ("a `?` inside an inline mapping", "yaml", "service.yaml",
         plant(service, "      photo:    { type: file, optional: true }", "      photo:    { type: file? }")),
    ]
    ok = True
    for name, expect, f, text in planted:
        found, _ = check([("people.yaml", people), (f, text)])
        hit = [x for x in found if x[3] == expect]
        ok &= bool(hit)
        where = f"{hit[0][0]}:{hit[0][1]} {hit[0][4]}" if hit else f"MISSED ({found[:1]})"
        print(f"  planted {name}: {'caught' if hit else 'missed'} by {expect}, at {where}")
    no = "values: [YES, NO]\n"
    strict, loose = load(no)["values"], yaml.safe_load(no)["values"]
    hit = "NO" in strict and False in loose
    ok &= hit
    print(f"  planted an enumeration value NO: the strict loader keeps {strict}, a plain YAML loader reads {loose}")
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

#!/usr/bin/env python3
"""Check flow descriptions against the flow format (docs/design/flow-format.md).

A flow is written as structured YAML, the one written form (ADR-0116). A module
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

    kind: initial, to: S                   create <name> -> S
    kind: external, from: [A, B], to: C    do <name> { A, B } -> C
    kind: internal, from: [A, B]           act <name> at { A, B }
    required_inputs: [a]                   accepts a; where a is optional, also
                                           a guard a_provided: inputs.a is not null
    optional_inputs: [a]                   accepts a
    inputs: {r: {type: T, optional: true}} input r : T?  (read as inputs.r)
    guards: {g: deny|audit|warn}           require g: …  [observe|flag]
    effect: assign {location: b, expr: e}  set b := e
    effect: clear [a]                      clear a
    final: true on state S                 state S … terminal
    required_attributes: [a] on state S    invariant s_invariant:
                                             state != S or a is not null
    measure: median_time_in_state          the intervals in the state
    measure: transition_count              the transitions along the
                                           transition's from and to states

The terms are UML's, SCXML's and Kubernetes', as ADR-0115 records. Every
finding is reported at the YAML file and line it comes from. Audit and warn
guards are reported as notices, as the publish report lists them.

    check-flows.py                  the example modules, then a self-test
    check-flows.py A.yaml B.yaml    these modules, in import order
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
FORMAT = ROOT / "docs/design/flow-format"
EXAMPLES = FORMAT / "examples"
CHECKER = ROOT / "scripts/check-syntax-doc.py"

MODULE_ORDER = ["module", "imports", "categories", "enumerations", "machines", "types"]
MACHINE_ORDER = ["description", "requires", "states", "conditions", "transitions"]
# names the text language gives a meaning in the same position (declaration-syntax.md §9.2, check 33)
NOT_A_CATEGORY = {"any", "terminal", "superseding"}
NOT_A_TRANSITION = {"any"}
# a bare name in an expression resolves to these before an attribute (declaration-syntax.md §9.2), so no attribute may take one
NOT_AN_ATTRIBUTE = {"state", "inputs", "actor", "this", "now", "referrers", "this_event"}
# keys the text language has a form for on one kind of attribute only; elsewhere they would be dropped
ONLY_ON_OBSERVATIONS = {"unit"}
ONLY_ON_TYPES = {"actor_kind", "assignee", "unique", "indexed", "opposite", "stored", "aggregation", "cascade", "survives"}
TYPE_ORDER = ["description", "tracking", "state_machine", "attributes", "observations", "states",
              "derived_attributes", "invariants", "conditions", "transitions", "metrics"]


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


def machine_as_type(m):
    """A machine, checked as a type whose attributes are those it requires."""
    return {"description": m["description"], "tracking": "record",
            "attributes": (m.get("requires") or {}).get("attributes") or {},
            "states": m["states"], "conditions": m.get("conditions") or {}, "transitions": m["transitions"]}


def bound(t, machines):
    """A type with the machine it binds merged in: the machine's states, its
    conditions, and its transitions, of which the initial ones give way to a
    binder's own (declaration-syntax.md §2.1)."""
    m = machines.get(t.get("state_machine"))
    if m is None:
        return t
    own = t.get("transitions") or {}
    own_initial = any(x["kind"] == "initial" for x in own.values())
    merged = {k: v for k, v in m["transitions"].items() if not (own_initial and v["kind"] == "initial")}
    merged.update(own)
    conditions = dict(m.get("conditions") or {})
    conditions.update(t.get("conditions") or {})
    return dict(t, states=m["states"], transitions=merged, conditions=conditions)


def view(doc):
    """The module as the checks see it: each machine as a type, each binder
    merged with its machine; and a map back from each view path to the YAML."""
    machines = doc.get("machines") or {}
    kinds = {mn: machine_as_type(m) for mn, m in machines.items()}
    kinds.update({tn: bound(t, machines) for tn, t in (doc.get("types") or {}).items()})
    v = dict(doc, types=kinds)

    def back(path):
        """The YAML path a view path stands for, or None where it is a
        machine's part of a binder, which is reported at the machine."""
        if len(path) < 2 or path[0] != "types":
            return path
        name = path[1]
        if name in machines:
            rest = path[2:]
            if rest[:1] == ("attributes",):
                rest = ("requires",) + rest
            return ("machines", name) + rest
        t = (doc.get("types") or {}).get(name) or {}
        if t.get("state_machine") and len(path) >= 4 and path[2] in ("states", "transitions", "conditions"):
            if path[3] not in (t.get(path[2]) or {}):
                return None
        return path
    return v, back


def sources(x):
    f = x.get("from")
    return [] if f is None else [f] if isinstance(f, str) else list(f)


def pairs(x):
    """The (from, to) state pairs a transition takes an object along."""
    if x["kind"] == "initial":
        return [(None, x["to"])]
    return [(s, x.get("to", s)) for s in sources(x)]


def optional(spec):
    return bool(spec.get("optional"))


def taken(x):
    """Every input a transition takes: attribute inputs and declared inputs."""
    return set(x.get("required_inputs", [])) | set(x.get("optional_inputs", [])) | set(x.get("inputs") or {})


def required_input_names(x):
    """The inputs a request always has a value for: required attribute inputs,
    and declared inputs that are not optional or that carry a default."""
    declared = {i for i, spec in (x.get("inputs") or {}).items() if not spec.get("optional") or "default" in spec}
    return set(x.get("required_inputs", [])) | declared


INPUT_READ = re.compile(r"\binputs\.([a-z][a-z0-9_]*)")


def provided_guards(t, x):
    """The guards required_inputs generates: one per optional attribute."""
    attrs = t.get("attributes") or {}
    return [a for a in x.get("required_inputs", []) if a in attrs and optional(attrs[a])]


NAME = re.compile(r"[a-z][a-z0-9_]*")


def steps(x, verb):
    return [s[verb] for s in x.get("effect", []) if verb in s]


def assigns(x):
    """(expr, location) for each assign step."""
    return [(" ".join(a["expr"].split()), a["location"]) for a in steps(x, "assign")]


STEP_KINDS = ("assign", "clear", "add", "remove", "call", "create", "foreach")


def walk(steps, path=()):
    """Every effect step at any depth, with its path from the effect and its depth."""
    for i, st in enumerate(steps or []):
        kind = next(k for k in STEP_KINDS if k in st)
        yield st, kind, path + (i,), len(path) // 3
        if kind == "foreach":
            yield from walk(st["foreach"]["steps"], path + (i, "foreach", "steps"))


def step_expressions(st, kind):
    """The expressions a step reads."""
    body = st[kind]
    if kind in ("assign", "add", "remove"):
        return [body["expr"]]
    if kind == "call":
        return [body["target"]] + list((body.get("inputs") or {}).values())
    if kind == "create":
        return list((body.get("inputs") or {}).values())
    if kind == "foreach":
        return [body[k] for k in ("array", "range", "where") if k in body]
    return []


def cleared(x):
    return [a for group in steps(x, "clear") for a in group]


# ── steps 2 and 3 ──────────────────────────────────────────────────────────
def schema_errors(doc):
    schema = json.loads((FORMAT / "flow.schema.json").read_text())
    v = jsonschema.Draft7Validator(schema)
    out = []
    for e in sorted(v.iter_errors(doc), key=lambda e: list(e.absolute_path)):
        # a oneOf over the kinds says only that no branch matched; name the kind's rule instead
        if e.validator == "oneOf" and isinstance(e.instance, dict) and "kind" in e.instance:
            rule = {"initial": "an initial transition declares `to` and no `from`",
                    "external": "an external transition declares `from` and `to`",
                    "internal": "an internal transition declares `from` and no `to`"}.get(e.instance["kind"], e.message)
            out.append((tuple(e.absolute_path), "schema", rule))
        elif e.validator == "oneOf" and isinstance(e.instance, dict) and "tracking" in e.instance and "description" in e.instance:
            out.append((tuple(e.absolute_path), "schema",
                        "a type either declares its own states and transitions or binds a state_machine, and never both"))
        elif e.validator == "oneOf" and isinstance(e.instance, dict) and "measure" in e.instance:
            out.append((tuple(e.absolute_path), "schema",
                        "a median_time_in_state metric names a `state`, and a transition_count names a `transition`"))
        elif e.validator == "oneOf" and e.context:
            # for a step, report the error of the branch its own key names
            branch = None
            if isinstance(e.instance, dict) and len(e.instance) == 1:
                for n, alt in enumerate(e.validator_value):
                    if alt.get("required") == list(e.instance):
                        branch = n
            within = [c for c in e.context if branch is None or c.schema_path[0] == branch]
            best = jsonschema.exceptions.best_match(within or e.context)
            out.append((tuple(e.absolute_path) + tuple(best.absolute_path), "schema", best.message))
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
    for mn, m in (doc.get("machines") or {}).items():
        in_order(list(m), MACHINE_ORDER, ("machines", mn), f"in {mn}, ")
    for tn, t in types(doc):
        in_order(list(t), TYPE_ORDER, ("types", tn), f"in {tn}, ")
    return out


def references(t, tn):
    """Every type a type's attributes, observations and inputs name, with its path."""
    refs = [(("types", tn, "attributes", a), s["reference"].rstrip("[]")) for a, s in (t.get("attributes") or {}).items() if "reference" in s]
    refs += [(("types", tn, "observations", o, "attributes", a), s["reference"].rstrip("[]"))
             for o, ob in (t.get("observations") or {}).items() for a, s in ob["attributes"].items() if "reference" in s]
    refs += [(("types", tn, "transitions", xn, "inputs", i), s["reference"].rstrip("[]"))
             for xn, x in (t.get("transitions") or {}).items() for i, s in (x.get("inputs") or {}).items() if "reference" in s]
    return refs


def relationship_errors(doc, library):
    """The ends of a relationship name each other; a composite end's parts point
    back with a single, required end; a cascade names a transition of the part
    with its inputs; and every final transition of a whole is covered by a
    cascade or survived, never both (declaration-syntax.md §3.2, §3.3)."""
    out = []
    module = dict(types(doc))
    imported = {n for names in (doc.get("imports") or {}).values() for n in names}
    for tn, t in types(doc):
        for path, r in references(t, tn):
            if r not in module and r not in imported:
                out.append((path, "names", f"{tn} references {r}, which is neither declared in this module nor imported"))
        attrs = t.get("attributes") or {}
        finals = {sn for sn, v in (t.get("states") or {}).items() if v.get("final")}
        for an, spec in attrs.items():
            here = ("types", tn, "attributes", an)
            kind = spec.get("reference", "")
            target, many = kind.rstrip("[]"), kind.endswith("[]")
            if "opposite" in spec:
                other = module.get(target)
                if other is None:
                    out.append((here + ("opposite",), "names", f"{tn}.{an} names an opposite end in {target}, which this module does not declare; both ends of a relationship are in one module"))
                    continue
                back = (other.get("attributes") or {}).get(spec["opposite"])
                if back is None or back.get("reference", "").rstrip("[]") != tn or back.get("opposite") != an:
                    out.append((here + ("opposite",), "names", f"{tn}.{an}'s opposite {target}.{spec['opposite']} does not name {tn}.{an} back"))
                    continue
                if spec.get("stored") and back.get("stored"):
                    out.append((here + ("stored",), "names", f"{tn}.{an} and {target}.{spec['opposite']} are both marked stored; one end holds the value"))
            if spec.get("aggregation") == "composite":
                if "opposite" not in spec:
                    out.append((here, "names", f"{tn}.{an} is composite and names no opposite: a part names its whole"))
                    continue
                part = module.get(target, {})
                owner = (part.get("attributes") or {}).get(spec["opposite"], {})
                if owner.get("reference", "").endswith("[]") or owner.get("optional"):
                    out.append((here, "names", f"{target}.{spec['opposite']} MUST be single and required: a part of {tn}.{an} belongs to exactly one whole"))
                covered, clauses = set(), spec.get("cascade") or []
                for i, c in enumerate(clauses):
                    where = here + ("cascade", i)
                    for w in c["on"]:
                        if w not in (t.get("transitions") or {}):
                            out.append((where, "names", f"{tn}.{an} cascades on '{w}', which is not a transition of {tn}"))
                    covered |= set(c["on"])
                    called = (part.get("transitions") or {}).get(c["transition"])
                    if called is None or called["kind"] == "initial":
                        out.append((where, "names", f"{tn}.{an} cascades to {target}.{c['transition']}, which is not a transition of {target} that acts on a part"))
                        continue
                    given = set(c.get("inputs") or {})
                    out += [(where, "names", f"{tn}.{an} passes '{i}' to {target}.{c['transition']}, which takes no input '{i}'") for i in sorted(given - taken(called))]
                    out += [(where, "names", f"{tn}.{an} cascades to {target}.{c['transition']} without the input '{i}', which it requires")
                            for i in sorted(required_input_names(called) - given) if "default" not in ((called.get("inputs") or {}).get(i) or {})]
                survives = spec.get("survives")
                kept = set(t.get("transitions") or {}) if survives is True else set(survives or [])
                for w in sorted(covered & kept):
                    out.append((here, "names", f"{tn}.{an} both cascades on and survives '{w}'"))
                for xn, x in (t.get("transitions") or {}).items():
                    if x.get("to") in finals and xn not in covered | kept:
                        out.append((here, "names", f"{tn}.{xn} enters the final state {x['to']}, and {tn}.{an} neither cascades on it nor survives it"))
            elif "cascade" in spec or "survives" in spec:
                out.append((here, "names", f"{tn}.{an} has a cascade or survives, which only a composite end has"))
    return out


def machine_errors(doc):
    """A binder names a declared or imported machine and declares what it
    requires; a machine's states require no attributes, since the invariant
    they generate belongs to each binder, which declares it instead."""
    out = []
    machines = doc.get("machines") or {}
    imported = {n for names in (doc.get("imports") or {}).values() for n in names}
    for mn, m in machines.items():
        if mn in (doc.get("types") or {}):
            out.append((("machines", mn), "names", f"{mn} names both a machine and a type"))
        for sn, st in m["states"].items():
            if st.get("required_attributes"):
                out.append((("machines", mn, "states", sn, "required_attributes"), "names",
                            f"state {sn} of the machine {mn} requires attributes; declare them as an invariant of each type that binds {mn}"))
    for tn, t in types(doc):
        mn = t.get("state_machine")
        if not mn:
            continue
        m = machines.get(mn)
        if m is None:
            if mn not in imported:
                out.append((("types", tn, "state_machine"), "names", f"{tn} binds {mn}, which is neither a machine of this module nor imported"))
            continue
        req = m.get("requires") or {}
        for a, want in (req.get("attributes") or {}).items():
            have = (t.get("attributes") or {}).get(a)
            if have is None:
                out.append((("types", tn, "state_machine"), "names", f"{tn} binds {mn}, which requires the attribute '{a}', and {tn} does not declare it"))
                continue
            # check 16: the same type, optionality included
            sig = lambda s: (s.get("type"), s.get("reference"), bool(s.get("optional")))
            if sig(have) != sig(want):
                out.append((("types", tn, "attributes", a), "names", f"{tn} declares '{a}' as {attribute_line(a, have).split(chr(10))[0].strip()}, and its machine {mn} requires {attribute_line(a, want).split(chr(10))[0].strip()}"))
        for i in req.get("invariants") or []:
            if i not in (t.get("invariants") or {}):
                out.append((("types", tn, "state_machine"), "names", f"{tn} binds {mn}, which requires the invariant '{i}', and {tn} does not declare it"))
        for c in (t.get("conditions") or {}):
            if c in (m.get("conditions") or {}):
                out.append((("types", tn, "conditions", c), "names", f"{tn} declares the condition '{c}', which its machine {mn} also declares"))
        for x in (t.get("transitions") or {}):
            if x in m["transitions"] and m["transitions"][x]["kind"] != "initial":
                out.append((("types", tn, "transitions", x), "names", f"{tn} declares the transition '{x}', which its machine {mn} also declares"))
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
        owned += [(("types", tn, "derived_attributes", d), d) for d in (t.get("derived_attributes") or {})]
        members = set(attrs) | set(t.get("observations") or {})
        for d in (t.get("derived_attributes") or {}):
            if d in members:
                out.append((("types", tn, "derived_attributes", d), "names",
                            f"'{d}' names both a derived attribute and another member of {tn}; a member has one name"))
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
            if i.endswith("_invariant") and i[:-len("_invariant")].upper() in states:
                out.append((("types", tn, "invariants", i), "names",
                            f"'{i}' is the name of the invariant generated for a state's required_attributes; choose another name"))
    return out


def name_errors(doc, library=None, ordered=True):
    library = dict(library or {})
    library.update(dict(types(doc)))
    out = (order_errors(doc) if ordered else []) + reserved_name_errors(doc) + relationship_errors(doc, library)
    for tn, t in types(doc):
        states = t.get("states") or {}
        attrs = t.get("attributes") or {}
        derived = set(t.get("derived_attributes") or {})
        conds = set(t.get("conditions") or {})
        required = {s: set((v or {}).get("required_attributes", [])) for s, v in states.items()}
        always = {a for a, s in attrs.items() if not optional(s)}
        used = set()
        for s, v in states.items():
            if v["category"] not in (doc.get("categories") or []):
                out.append((("types", tn, "states", s, "category"), "names",
                            f"state {s} has the category '{v['category']}', which the module does not declare in categories"))
            for a in (v or {}).get("required_attributes", []):
                if a in derived:
                    out.append((("types", tn, "states", s, "required_attributes"), "names",
                                f"state {s} requires the derived attribute '{a}'; a state requires stored attributes, and an invariant may read a derived one"))
                elif a not in attrs:
                    out.append((("types", tn, "states", s, "required_attributes"), "names",
                                f"state {s} requires the attribute '{a}', which {tn} does not declare"))
        for xn, x in (t.get("transitions") or {}).items():
            base = ("types", tn, "transitions", xn)
            for s in sources(x) + ([x["to"]] if x.get("to") else []):
                if s not in states:
                    key = "to" if s == x.get("to") else "from"
                    out.append((base + (key,), "names", f"{tn}.{xn} names the state {s}, which {tn} does not declare"))
            conditions = t.get("conditions") or {}
            for g in (x.get("guards") or {}):
                used.add(g)
                if g not in conds:
                    out.append((base + ("guards", g), "names", f"{tn}.{xn} guards on '{g}', which is not a condition of {tn}"))
                    continue
                for i in sorted(set(INPUT_READ.findall(conditions[g]["expression"])) - taken(x)):
                    out.append((base + ("guards", g), "names",
                                f"{tn}.{xn} guards on '{g}', which reads inputs.{i}, and {xn} takes no input '{i}'"))
            for st, kind, path, _depth in walk(x.get("effect")):
                where = base + ("effect",) + path
                for e in step_expressions(st, kind):
                    for i in sorted(set(INPUT_READ.findall(e)) - taken(x)):
                        out.append((where, "names", f"{tn}.{xn}'s effect reads inputs.{i}, and {xn} takes no input '{i}'"))
                if kind in ("assign", "add", "remove") and st[kind]["location"] in derived:
                    out.append((where, "names", f"{tn}.{xn} writes '{st[kind]['location']}', a derived attribute, whose value is its expression"))
                elif kind in ("assign", "add", "remove") and st[kind]["location"] not in attrs:
                    out.append((where, "names", f"{tn}.{xn} writes '{st[kind]['location']}', which {tn} does not declare"))
                if kind in ("add", "remove") and st[kind]["location"] in attrs and not str(attrs[st[kind]["location"]].get("type", "")).endswith("[]"):
                    out.append((where, "names", f"{tn}.{xn} {kind}s to '{st[kind]['location']}', which is not a set"))
                if kind == "clear" and path[-1:] != () and len(path) > 1:
                    for a in st["clear"]:
                        if a not in attrs and a not in derived:
                            out.append((where, "names", f"{tn}.{xn} clears '{a}', which {tn} does not declare"))
                if kind == "create":
                    out += create_errors(doc, library, where, tn, xn, st["create"])
                if kind == "call":
                    out += call_errors(library, where, t, tn, xn, x, st["call"])
            for v in x.get("only_via", []):
                vt, vx = v.split(".")
                if vt in library and vx not in (library[vt].get("transitions") or {}):
                    out.append((base + ("only_via",), "names", f"{tn}.{xn} is only via {v}, which is not a transition of {vt}"))
                elif vt not in library and vt not in {n for ns in (doc.get("imports") or {}).values() for n in ns}:
                    out.append((base + ("only_via",), "names", f"{tn}.{xn} is only via {v}, and {vt} is neither declared in this module nor imported"))
            for i, spec in (x.get("inputs") or {}).items():
                if i in set(x.get("required_inputs", [])) | set(x.get("optional_inputs", [])):
                    out.append((base + ("inputs", i), "names", f"{tn}.{xn} declares the input '{i}' and also takes the attribute '{i}' as an input"))
                kind = spec.get("type") or spec.get("reference")
                if kind.endswith("[]") and spec.get("optional"):
                    out.append((base + ("inputs", i), "names", f"{tn}.{xn}'s input '{i}' is a set, which is never optional: an empty set is supplied instead"))
            named = [(k, a) for k in ("required_inputs", "optional_inputs") for a in x.get(k, [])]
            named += [("effect", a) for a in cleared(x)]   # an assign's location is checked with every step
            for k, a in named:
                if a in derived:
                    out.append((base + (k,), "names", f"{tn}.{xn} names the derived attribute '{a}' in {k}; its value is its expression, and nothing writes it"))
                elif a not in attrs:
                    out.append((base + (k,), "names", f"{tn}.{xn} names the attribute '{a}' in {k}, which {tn} does not declare"))
            for a in set(x.get("required_inputs", [])) & set(x.get("optional_inputs", [])):
                out.append((base + ("optional_inputs",), "names", f"{tn}.{xn} lists '{a}' as both a required and an optional input"))
            for a in x.get("optional_inputs", []):
                if a in always:
                    out.append((base + ("optional_inputs",), "names",
                                f"{tn}.{xn} lists '{a}' as an optional input, but the attribute is not optional, so the input is required"))
            written = set(x.get("required_inputs", [])) | set(x.get("optional_inputs", [])) | {loc for _e, loc in assigns(x)}
            for a in sorted(written & set(cleared(x))):
                out.append((base + ("effect",), "names", f"{tn}.{xn} both writes and clears '{a}'"))
            out += required_errors(tn, xn, x, required, always)
        for c in sorted(conds - used):
            out.append((("types", tn, "conditions", c), "names", f"condition '{c}' of {tn} is used by no transition"))
        out += value_errors(doc, tn, t)
        out += read_errors(tn, t) + indexed_errors(tn, t, library)
        for mn, m in (t.get("metrics") or {}).items():
            out += metric_errors(tn, t, mn, m)
    return out


def create_errors(doc, library, where, tn, xn, c):
    """A create names a type of the module, or an imported one, and an initial
    transition of it, and supplies that transition's required inputs."""
    imported = {n for names in (doc.get("imports") or {}).values() for n in names}
    if c["type"] not in (doc.get("types") or {}) and c["type"] not in imported:
        return [(where, "names", f"{tn}.{xn} creates a {c['type']}, which is neither declared in this module nor imported")]
    target = library.get(c["type"])
    if target is None:
        return []
    t = (target.get("transitions") or {}).get(c["transition"])
    if t is None or t["kind"] != "initial":
        return [(where, "names", f"{tn}.{xn} creates a {c['type']} by '{c['transition']}', which is not an initial transition of {c['type']}")]
    given = set(c.get("inputs") or {})
    out = [(where, "names", f"{tn}.{xn} passes '{i}' to {c['type']}.{c['transition']}, which takes no input '{i}'")
           for i in sorted(given - taken(t))]
    out += [(where, "names", f"{tn}.{xn} creates a {c['type']} without the input '{i}', which {c['type']}.{c['transition']} requires")
            for i in sorted(required_input_names(t) - given) if "default" not in ((t.get("inputs") or {}).get(i) or {})]
    return out


def call_errors(library, where, t, tn, xn, x, c):
    """A call whose target is an attribute or an input of a known type names a
    transition of that type that acts on an existing object, and passes its inputs.
    A target reached any other way is left to the publish checks."""
    target = " ".join(c["target"].split())
    spec = None
    m = re.fullmatch(r"(?:this\.)?([a-z][a-z0-9_]*)", target)
    if m and m.group(1) in (t.get("attributes") or {}):
        spec = t["attributes"][m.group(1)]
    m = re.fullmatch(r"inputs\.([a-z][a-z0-9_]*)", target)
    if m:
        spec = (x.get("inputs") or {}).get(m.group(1)) or (t.get("attributes") or {}).get(m.group(1))
    kind = (spec or {}).get("reference")
    if not kind or kind.endswith("[]") or kind not in library:
        return []
    other = library[kind]
    called = (other.get("transitions") or {}).get(c["transition"])
    if called is None:
        return [(where, "names", f"{tn}.{xn} calls {kind}.{c['transition']}, which is not a transition of {kind}")]
    if called["kind"] == "initial":
        return [(where, "names", f"{tn}.{xn} calls {kind}.{c['transition']}, an initial transition; create an object with create")]
    given = set(c.get("inputs") or {})
    out = [(where, "names", f"{tn}.{xn} passes '{i}' to {kind}.{c['transition']}, which takes no input '{i}'") for i in sorted(given - taken(called))]
    out += [(where, "names", f"{tn}.{xn} calls {kind}.{c['transition']} without the input '{i}', which it requires")
            for i in sorted(required_input_names(called) - given) if "default" not in ((called.get("inputs") or {}).get(i) or {})]
    return out


def present(expr, known, inputs):
    """Whether an assigned expression provably has a value: now, a literal, a
    qualified value, a known attribute or a required input. Anything else is
    not provable here, so it does not count."""
    if expr == "now" or re.fullmatch(r'-?\d+(\.\d+)?|"[^"]*"|[A-Z][A-Za-z0-9]*\.[A-Z][A-Z0-9_]*', expr):
        return True
    if NAME.fullmatch(expr) and expr not in NOT_AN_ATTRIBUTE:
        return expr in known
    m = re.fullmatch(r"inputs\.([a-z][a-z0-9_]*)", expr)
    return bool(m) and m.group(1) in inputs


def required_errors(tn, xn, x, required, always):
    """Every transition into a state sets each attribute the state requires."""
    base = ("types", tn, "transitions", xn)
    out = []
    src = sources(x)
    targets = [x["to"]] if x.get("to") else src
    for s in targets:
        for a in sorted(required.get(s, set()) & set(cleared(x))):
            out.append((base + ("effect",), "required", f"{tn}.{xn} clears '{a}', which state {s} requires"))
    if x["kind"] == "internal":
        return out
    supplied = set(x.get("required_inputs", [])) | always
    kept = set.intersection(*(required.get(s, set()) for s in src)) if src else set()
    assigned = {loc: e for e, loc in assigns(x)}
    for a in sorted(required.get(x["to"], set())):
        if a in supplied or (a in kept and a not in cleared(x)):
            continue
        if a in assigned and present(assigned[a], supplied | kept, required_input_names(x)):
            continue
        how = f"or require it in state {', '.join(src)}" if src else "since an initial transition starts from no state"
        out.append((base + ("to",), "required",
                    f"{tn}.{xn} enters {x['to']} without setting '{a}', which {x['to']} requires: "
                    f"make it a required input, assign it a value that is provably present, {how}"))
    return out


def value_errors(doc, tn, t):
    """Every value an expression names is a state or an enumeration value. The
    text checker does not yet resolve enumeration values (check 19, in part)."""
    states = set(t.get("states") or {})
    enums = {n: set(m) for n, m in (doc.get("enumerations") or {}).items()}
    own = {k: set(v.get("states") or {}) for k, v in types(doc)}
    exprs = [(("types", tn, sec, n, "expression"), n, c["expression"])
             for sec in ("conditions", "invariants", "derived_attributes") for n, c in (t.get(sec) or {}).items()]
    exprs += [(("types", tn, "transitions", xn, "effect") + path, xn, e) for xn, x in (t.get("transitions") or {}).items()
              for st, kind, path, _d in walk(x.get("effect")) for e in step_expressions(st, kind)]
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


KEYWORDS = {"and", "or", "not", "implies", "is", "null", "in", "where", "if", "then", "else",
            "true", "false", "distinct", "over", "last", "fresh"}
AGGREGATES = ("count", "sum", "min", "max", "all", "any", "none")
DURATION_UNITS = {"s", "min", "h", "days", "weeks"}
FLOW_DATA = {"intervals", "transitions", "labels"}
# the kinds of actor, a fixed enumeration expressions write bare (declaration-syntax.md §8.1)
ACTOR_KINDS = set(json.loads((FORMAT / "flow.schema.json").read_text())
                  ["definitions"]["attribute"]["properties"]["actor_kind"]["enum"])
OBSERVATION_MEMBERS = {"now", "subject", "corrects", "occurred_at", "recorded_at", "recorded_by_kind",
                       "declaration_version"}


def reads(expr):
    """The member names an expression reads: each lower-case name that begins a
    path, `this.<name>` included, less keywords, function names, the names an
    aggregate binds, duration units and a metric's own arguments."""
    text = re.sub(r'"[^"]*"', '""', " ".join(str(expr).split()))
    bound = set(re.findall(r"\b(?:%s)\(\s*(?:distinct\s+)?([a-z][a-z0-9_]*)\s+in\b" % "|".join(AGGREGATES), text))
    out = set()
    for m in re.finditer(r"(?<![\w.])(this\.)?([a-z][a-z0-9_]*)\b(?!\s*\()", text):
        name, before, after = m.group(2), text[:m.start()], text[m.end():]
        if name in KEYWORDS or name in ACTOR_KINDS or name in bound or re.match(r"\s*:=", after) or re.search(r"\bmetric\(\s*$", before):
            continue
        if name in DURATION_UNITS and re.search(r"\d\s*$", before):
            continue
        if m.group(1) and name in FLOW_DATA:
            continue
        out.add(name)
    return out


def read_errors(tn, t):
    """Every name an expression reads is a member of the object it is evaluated
    on: for a type, its attributes and observation kinds; for an observation
    kind's invariant, that kind's fields. An effect also reads the names its
    foreach and create steps bind."""
    derived = list(t.get("derived_attributes") or {})
    stored = set(t.get("attributes") or {}) | set(t.get("observations") or {})
    members = stored | set(derived) | NOT_AN_ATTRIBUTE
    exprs = [(("types", tn, sec, n, "expression"), n, c["expression"], members)
             for sec in ("conditions", "invariants") for n, c in (t.get(sec) or {}).items()]
    out = []
    for i, d in enumerate(derived):
        path = ("types", tn, "derived_attributes", d, "expression")
        read = reads(t["derived_attributes"][d]["expression"])
        for r in sorted(read & set(derived[i:])):
            out.append((path, "order", f"derived attribute '{d}' reads itself" if r == d else
                        f"derived attribute '{d}' reads '{r}', which is declared after it; a derived attribute reads only those above it"))
        for r in sorted(read & {"inputs", "this_event"}):
            out.append((path, "names", f"derived attribute '{d}' reads '{r}', which only a transition has"))
        exprs.append((path, d, t["derived_attributes"][d]["expression"], members | set(derived[i:])))
    for xn, x in (t.get("transitions") or {}).items():
        walked = list(walk(x.get("effect")))
        binds = {st["foreach"]["item"] for st, kind, _p, _d in walked if kind == "foreach"}
        binds |= {st["create"]["result"] for st, kind, _p, _d in walked if kind == "create" and st["create"].get("result")}
        exprs += [(("types", tn, "transitions", xn, "effect") + path, xn, e, members | binds)
                  for st, kind, path, _d in walked for e in step_expressions(st, kind)]
    for o, ob in (t.get("observations") or {}).items():
        fields = set(ob.get("attributes") or {}) | OBSERVATION_MEMBERS
        exprs += [(("types", tn, "observations", o, "invariants", n, "expression"), n, i["expression"], fields)
                  for n, i in (ob.get("invariants") or {}).items()]
    return out + [(path, "names", f"{name} reads '{r}', which {tn} does not declare"
                                  if "observations" not in path else
                                  f"{name} reads '{r}', which is not a field of the observation {path[3]}")
                  for path, name, text, scope in exprs for r in sorted(reads(text) - scope)]


def stores(spec, library):
    """Whether a single reference end holds the value (declaration-syntax.md §3.3):
    it does unless its opposite is also single and it is not the end marked stored."""
    other = ((library.get(spec["reference"]) or {}).get("attributes") or {}).get(spec.get("opposite"))
    return other is None or str(other.get("reference", "")).endswith("[]") or bool(spec.get("stored"))


def indexed_errors(tn, t, library):
    """An indexed derived attribute reads only this object's stored, indexed
    values: indexed attributes, stored single references and the state, and no
    clock, other object or flow data (the model's check 46)."""
    attrs = t.get("attributes") or {}
    out = []
    for d, spec in (t.get("derived_attributes") or {}).items():
        if not spec.get("indexed"):
            continue
        text = " ".join(spec["expression"].split())
        why = []
        for r in sorted(reads(text) - {"state", "this"}):
            a = attrs.get(r)
            if r == "now":
                why.append("reads now, the clock")
            elif a is None:
                why.append(f"reads '{r}', which is not a stored attribute")
            elif str(a.get("reference", "")).endswith("[]") or str(a.get("type", "")).endswith("[]"):
                why.append(f"reads '{r}', a set")
            elif "reference" in a and re.search(rf"(?<![\w.]){r}\s*\.", text):
                why.append(f"reads through '{r}' into another object")
            elif "reference" in a and not stores(a, library):
                why.append(f"reads '{r}', whose value the other end of the relationship stores")
            elif "reference" not in a and not a.get("indexed"):
                why.append(f"reads '{r}', which is not indexed")
        if re.search(r"\b(?:time_in|entered_at)\(|\bthis\.(?:intervals|transitions)\b", text):
            why.append("reads the object's flow data")
        out += [(("types", tn, "derived_attributes", d, "indexed"), "names", f"derived attribute '{d}' is indexed and {w}") for w in why]
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
                if mode == "audit":
                    out.append((path, "audit", f"{g} is audited: its failures are recorded and it refuses nothing"))
                elif mode == "warn":
                    out.append((path, "warn", f"{g} warns: a failure is reported with the result and refuses nothing"))
    return out


# ── step 4: conversion to the text language ────────────────────────────────
def attribute_line(name, spec, observation=False, module=None):
    if "reference" in spec:
        t = spec["reference"] + ("?" if optional(spec) else "")
        if observation:
            return f"field {name} : {t}"
        opp = spec.get("opposite")
        inverse = f" inverse {opp}" if opp else ""
        if spec.get("aggregation") == "composite":
            lines = [f"part {name} : {t}{inverse}"]
            one = lambda e: " ".join(e.split())
            for c in spec.get("cascade") or []:
                on = c["on"][0] if len(c["on"]) == 1 else "{ " + ", ".join(c["on"]) + " }"
                args = ", ".join(f"{k} := {one(v)}" for k, v in (c.get("inputs") or {}).items())
                lines.append(f"     cascade on {on} to {spec['reference'].rstrip('[]')}.{c['transition']}" + (f"({args})" if args else "") + f" limit {c['limit']}")
            sv = spec.get("survives")
            if sv is True:
                lines.append("     survives")
            elif sv:
                lines.append("     survives on { " + ", ".join(sv) + " }")
            return "\n  ".join(lines)
        other = (module or {}).get(spec["reference"].rstrip("[]"), {})
        if opp and ((other.get("attributes") or {}).get(opp) or {}).get("aggregation") == "composite":
            return f"owner {name} : {spec['reference']}{inverse}"
        return (f"ref {name} : {t}{inverse}" + (" stored" if spec.get("stored") else "")
                + (" assignee" if spec.get("assignee") else ""))
    t = spec["type"] + ("?" if optional(spec) else "")
    if observation:
        return (f"field {name} : {t}" + (f' unit "{spec["unit"]}"' if spec.get("unit") else "")
                + (" personal" if spec.get("personal") else ""))
    marks = ([f"actor {spec['actor_kind']}"] if spec.get("actor_kind") else []) + \
            [m for m in ("unique", "indexed", "personal") if spec.get(m)]
    return f"attr {name} {t}" + "".join(" " + m for m in marks)


def transition_lines(xn, x, t, X, T):
    """One transition in the internal form, each line with its YAML path; t
    supplies the attributes and conditions its guards name."""
    conds = t.get("conditions") or {}
    src = sources(x)
    where = src[0] if len(src) == 1 else "{ " + ", ".join(src) + " }"
    head = {"initial": f"create {xn} -> {x.get('to')}", "external": f"do {xn} {where} -> {x.get('to')}",
            "internal": f"act {xn} at {where}"}[x["kind"]]
    if x.get("only_via"):
        head += " only via " + ", ".join(x["only_via"])
    accepts = x.get("required_inputs", []) + x.get("optional_inputs", [])
    if accepts:
        head += " accepts " + ", ".join(accepts)
    if x.get("backdating_limit"):
        head += f" backdatable within {x['backdating_limit']}"
    body = []
    for i, spec in (x.get("inputs") or {}).items():
        kind = spec.get("type") or spec.get("reference")
        line = f"input {i} : {kind}" + ("?" if spec.get("optional") else "")
        if "default" in spec:
            line += f" default {' '.join(spec['default'].split())}"
        body.append((line + (" personal" if spec.get("personal") else ""), X + ("inputs", i)))
    body += [(f"require {a}_provided: inputs.{a} is not null because self_serviceable", X + ("required_inputs",))
             for a in provided_guards(t, x)]
    for g, mode in (x.get("guards") or {}).items():
        if g in conds:
            c = conds[g]
            mark = {"deny": "", "audit": " observe", "warn": " flag"}[mode]
            body.append((f"require {g}: {' '.join(c['expression'].split())}{mark} because {c['remedy']}", X + ("guards", g)))
    body += effect_lines(x.get("effect"), X + ("effect",), "")
    if not body:
        return [(f"  {head} {{ }}", X)]
    return [(f"  {head} {{", X)] + [("    " + b, p) for b, p in body] + [("  }", X)]


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
    machines = doc.get("machines") or {}
    for mn, m in machines.items():
        M = ("machines", mn)
        emit(f"# {m['description']}", M)
        emit(f"machine {mn} version 1 {{", M)
        for an, spec in ((m.get("requires") or {}).get("attributes") or {}).items():
            emit("  requires " + attribute_line(an, spec).split("\n")[0], M + ("requires", "attributes", an))
        for i in (m.get("requires") or {}).get("invariants") or []:
            emit(f"  requires invariant {i}", M + ("requires", "invariants"))
        for sn, st in m["states"].items():
            emit(f"  state {sn} category {st['category']}" + (" terminal" if st.get("final") else ""), M + ("states", sn))
        for xn, x in m["transitions"].items():
            for line, path in transition_lines(xn, x, machine_as_type(m), M + ("transitions", xn), M):
                emit(line, path)
        emit("}", M)
    for tn, t in types(doc):
        T = ("types", tn)
        conds = t.get("conditions") or {}
        emit(f"# {t['description']}", T)
        emit(f"type {tn} version 1 {{", T)
        emit(f"  tracking {t['tracking']}", T + ("tracking",))
        if t.get("state_machine"):
            emit(f"  machine  {t['state_machine']}", T + ("state_machine",))
        for sn, v in (t.get("states") or {}).items():
            emit(f"  state {sn} category {v['category']}" + (" terminal" if v.get("final") else ""), T + ("states", sn))
        for an, spec in (t.get("attributes") or {}).items():
            for line in attribute_line(an, spec, module=dict(types(doc))).split("\n"):
                emit(("  " + line) if not line.startswith("  ") else line, T + ("attributes", an))
        for dn, d in (t.get("derived_attributes") or {}).items():
            emit(f"  derive {dn} = {' '.join(d['expression'].split())}" + (" indexed" if d.get("indexed") else ""),
                 T + ("derived_attributes", dn))
        for sn, v in (t.get("states") or {}).items():
            req = v.get("required_attributes") or []
            if req:
                cond = " and ".join(f"{a} is not null" for a in req)
                cond = f"({cond})" if len(req) > 1 else cond
                emit(f"  invariant {sn.lower()}_invariant: state != {sn} or {cond}",
                     T + ("states", sn, "required_attributes"))
        for iname, inv in (t.get("invariants") or {}).items():
            emit(f"  invariant {iname}: {' '.join(inv['expression'].split())}", T + ("invariants", iname))
        merged = bound(t, machines)
        for xn, x in (t.get("transitions") or {}).items():
            for line, path in transition_lines(xn, x, merged, T + ("transitions", xn), T):
                emit(line, path)
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


def effect_lines(steps, P, indent):
    """The effect in the internal form, nested loops indented, each line with its YAML path."""
    one = lambda e: " ".join(e.split())
    args = lambda a: ", ".join(f"{k} := {one(v)}" for k, v in (a or {}).items())
    out = []
    for i, st in enumerate(steps or []):
        path = P + (i,)
        if "assign" in st:
            out.append((f"{indent}set {st['assign']['location']} := {one(st['assign']['expr'])}", path))
        elif "clear" in st:
            out += [(f"{indent}clear {a}", path) for a in st["clear"]]
        elif "add" in st or "remove" in st:
            k = "add" if "add" in st else "remove"
            out.append((f"{indent}{k} {st[k]['location']} := {one(st[k]['expr'])}", path))
        elif "call" in st:
            c = st["call"]
            out.append((f"{indent}call {one(c['target'])}.{c['transition']}({args(c.get('inputs'))})", path))
        elif "create" in st:
            c = st["create"]
            bound = f"{c['result']} = " if c.get("result") else ""
            out.append((f"{indent}create {bound}{c['type']}.{c['transition']}({args(c.get('inputs'))})", path))
        else:
            f = st["foreach"]
            over = one(f["array"]) if "array" in f else f"1..{one(f['range'])}"
            head = f"{indent}for {f['item']} in {over}" + (f" where {one(f['where'])}" if "where" in f else "") + f" limit {f['limit']} {{"
            out.append((head, path))
            out += effect_lines(f["steps"], path + ("foreach", "steps"), indent + "  ")
            out.append((f"{indent}}}", path))
    return out


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
    doc, starts, line = "# converted flow descriptions\n\n", [], 3
    for text, _paths in converted:
        starts.append(line + 1)
        doc += f"```text\n{text}```\n\n"
        line += text.count("\n") + 3
    with tempfile.TemporaryDirectory() as d:
        p = pathlib.Path(d) / "converted.md"
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
    found, notes, loaded, library = [], [], [], {}
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
        if errors:
            found += [(f, line_of(idx, p), "fatal", c, m) for p, c, m in errors]
            continue
        v, back = view(doc)
        seen = set()
        for p, c, m in order_errors(doc) + machine_errors(doc) + name_errors(v, library, ordered=False):
            p = back(p)
            if p is not None and p[0] == "machines" and "'" in m:
                # a machine's attributes, always quoted, are the ones it requires of its binders
                m = m.replace(f", which {p[1]} does not declare", f", which {p[1]} does not require")
            if p is not None and (p, m) not in seen:
                seen.add((p, m))
                found.append((f, line_of(idx, p), "fatal", c, m))
        library.update(dict(types(v)))
        notes += [(f, line_of(idx, back(p)), "notice", c, m) for p, c, m in notices(v) if back(p) is not None]
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


def self_test(people, service, inventory, delivery, approvals):
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
         plant(service, "          engineer_active: deny", "          engineer_is_active: deny")),
        ("a guard listed twice", "yaml", "service.yaml",
         plant(service, "          photo_attached: audit\n", "          photo_attached: audit\n          photo_attached: warn\n")),
        ("an enforcement that is not one of the three", "schema", "service.yaml",
         plant(service, "photo_attached: audit", "photo_attached: enforce")),
        ("an initial transition that names a source state", "schema", "service.yaml",
         plant(service, "        kind: initial\n        to: OPEN\n", "        kind: initial\n        from: WORKING\n        to: OPEN\n")),
        ("a guard that reads an input its transition does not take", "names", "service.yaml",
         plant(service, "        backdating_limit: 2 days\n", "        backdating_limit: 2 days\n        guards:\n          engineer_active: deny\n")),
        ("a call to a transition its target does not have", "names", "delivery.yaml",
         plant(delivery, "transition: deliver_externally, inputs", "transition: deliver_outside, inputs")),
        ("a create without an input its transition requires", "names", "delivery.yaml",
         plant(delivery, "inputs: { delivery: this, label: label } }", "inputs: { delivery: this } }")),
        ("a final transition of a whole that its parts neither cascade on nor survive", "names", "delivery.yaml",
         plant(delivery, "        survives: [complete]\n", "")),
        ("a relationship end whose opposite does not name it back", "names", "delivery.yaml",
         plant(delivery, "delivery: { reference: Delivery, opposite: checklist_items }", "delivery: { reference: Delivery, opposite: items }")),
        ("an only_via that names a missing transition", "names", "delivery.yaml",
         plant(delivery, "only_via: [Delivery.cancel]", "only_via: [Delivery.abort]")),
        ("a condition that reads an attribute the type does not declare", "names", "inventory.yaml",
         plant(inventory, "expression: condition != Condition.DAMAGED", "expression: grade != Condition.DAMAGED")),
        ("a machine condition that reads an attribute the machine does not require", "names", "approvals.yaml",
         plant(approvals, "expression: submitted_at is not null", "expression: decided_at is not null")),
        ("an observation invariant that reads what is not a field of its kind", "names", "service.yaml",
         plant(service, "expression: value is null or", "expression: voltage is null or")),
        ("a derived attribute that reads one declared after it", "order", "service.yaml",
         plant(service, "        expression: signed_off_by_user is not null\n", "        expression: handoffs > 0\n")),
        ("a transition that writes a derived attribute", "names", "service.yaml",
         plant(service, "          - assign: { location: signed_off_by_user, expr: inputs.signed_off_by }\n",
               "          - assign: { location: signed_off_by_user, expr: inputs.signed_off_by }\n"
               "          - assign: { location: handoffs, expr: \"0\" }\n")),
        ("an indexed derived attribute that reads an attribute that is not indexed", "names", "service.yaml",
         plant(service, "        expression: signed_off_by_user is not null\n", "        expression: photo is not null\n")),
        ("an indexed derived attribute that reads the clock", "names", "service.yaml",
         plant(service, "        expression: signed_off_by_user is not null\n", "        expression: engineer is not null and now > now\n")),
        ("a binder that lacks an attribute its machine requires", "names", "approvals.yaml",
         plant(approvals, "      submitted_at: { type: timestamp, optional: true }\n      item:", "      item:")),
        ("a binder whose attribute differs in type from what its machine requires", "names", "approvals.yaml",
         plant(approvals, "      submitted_at: { type: timestamp, optional: true }\n      item:", "      submitted_at: { type: date, optional: true }\n      item:")),
        ("a binder that redeclares a transition of its machine", "names", "approvals.yaml",
         plant(approvals, "      attach_receipt:\n", "      reject:\n        kind: external\n        from: DRAFT\n        to: REJECTED\n      attach_receipt:\n")),
        ("a condition that reads who is asking", "check 64", "service.yaml",
         plant(plant(service, "          no_unresolved_failure: deny\n",
                     "          no_unresolved_failure: deny\n          assigned_engineer_only: deny\n"), *reads_actor)),
        ("attributes declared after the states that use them", "order", "inventory.yaml", late_attributes),
        ("a misspelt enumeration value", "names", "inventory.yaml",
         plant(inventory, "condition != Condition.DAMAGED", "condition != Condition.DAMAGD")),
        ("a transition into a state that does not set what the state requires", "required", "inventory.yaml",
         plant(inventory, "        required_inputs: [reserved_until]\n", "        optional_inputs: [reserved_until]\n")),
        ("an attribute named after an expression built-in", "names", "inventory.yaml",
         plant(inventory, "      model:             { type: string }\n", "      model:             { type: string }\n      state:             { type: string, optional: true }\n")),
        ("an invariant named like a generated one", "names", "inventory.yaml",
         plant(inventory, "    conditions:\n", "    invariants:\n      reserved_invariant:\n        description: A clash with a generated name.\n"
                                              "        expression: serial is not null\n\n    conditions:\n")),
        ("a unit on an attribute that is not an observation's", "names", "inventory.yaml",
         plant(inventory, "      list_price:        { type: money(SGD) }", "      list_price:        { type: money(SGD), unit: SGD }")),
        ("a `?` inside an inline mapping", "yaml", "service.yaml",
         plant(service, "      photo:    { type: file, optional: true }", "      photo:    { type: file? }")),
    ]
    ok = True
    for name, expect, f, text in planted:
        before = [("inventory.yaml", inventory)] if f == "delivery.yaml" else []
        found, _ = check([("people.yaml", people)] + before + [(f, text)])
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
    people = (EXAMPLES / "people.yaml").read_text()
    clean = True
    needs = {"delivery.yaml": ["inventory.yaml"]}
    for version in ("people.yaml", "inventory.yaml", "inventory-v2.yaml", "service.yaml", "service-v2.yaml", "delivery.yaml", "approvals.yaml"):
        before = [(n, (EXAMPLES / n).read_text()) for n in needs.get(version, [])]
        files = [("people.yaml", people)] + before + ([(version, (EXAMPLES / version).read_text())] if version != "people.yaml" else [])
        found, notes = check(files)
        print(f"{version}: {'clean' if not found else str(len(found)) + ' finding(s)'}, "
              f"{len(notes)} notice{'s' if len(notes) != 1 else ''}")
        for x in found:
            print(f"  {x[0]}:{x[1]} {x[3]} {x[4]}")
        clean &= not found
    ok = self_test(people, (EXAMPLES / "service.yaml").read_text(), (EXAMPLES / "inventory.yaml").read_text(),
                   (EXAMPLES / "delivery.yaml").read_text(), (EXAMPLES / "approvals.yaml").read_text())
    sys.exit(0 if clean and ok else 1)


if __name__ == "__main__":
    main()

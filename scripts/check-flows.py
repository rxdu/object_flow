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

MODULE_ORDER = ["module", "imports", "categories", "enumerations", "sequences", "evaluators", "machines", "types", "migration"]
MACHINE_ORDER = ["description", "requires", "states", "conditions", "transitions"]
# names the text language gives a meaning in the same position (declaration-syntax.md §9.2, check 33)
NOT_A_CATEGORY = {"any", "terminal", "superseding"}
# every vocabulary has `closed`, whether or not a module declares it (declaration-syntax.md §1)
BUILT_IN_CATEGORIES = {"closed"}
NOT_A_TRANSITION = {"any"}
# a bare name in an expression resolves to these before an attribute (declaration-syntax.md §9.2), so no attribute may take one
NOT_AN_ATTRIBUTE = {"state", "inputs", "actor", "this", "now", "referrers", "this_event"}
# keys the text language has a form for on one kind of attribute only; elsewhere they would be dropped
ONLY_ON_OBSERVATIONS = {"unit"}
ONLY_ON_TYPES = {"actor_kind", "assignee", "unique", "indexed", "identifier", "external", "default", "opposite", "stored", "aggregation", "cascade",
                 "survives"}
METRIC_ORDER = ["description", "measure", "state", "transition", "source", "item", "filter", "dimensions",
                "group_by", "time_dimension", "expression", "flag_when"]
TYPE_ORDER = ["description", "abstract", "extends", "mirror", "tracking", "state_machine", "attributes", "observations", "states",
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


def resolve_any(x, states):
    """`from: any` is every state that is not final (declaration-syntax.md §4.2)."""
    if x.get("from") != "any":
        return x
    return dict(x, **{"from": [s for s, v in (states or {}).items() if not (v or {}).get("final")]})


INHERITED = ("attributes", "derived_attributes", "invariants")


def bases(tn, kinds):
    """The bases a type extends, nearest first; the walk stops at an unknown
    name or where it would come back to a type it has passed."""
    chain, seen = [], {tn}
    base = (kinds.get(tn) or {}).get("extends")
    while base and base not in seen and base in kinds:
        chain.append(base)
        seen.add(base)
        base = kinds[base].get("extends")
    return chain


def inherit(tn, t, kinds):
    """A type with what its bases give it: their attributes, derived
    attributes and invariants, and their tracking (declaration-syntax.md §2).
    `_from` names the base each inherited member is declared on."""
    chain = bases(tn, kinds)
    if not chain:
        return t
    merged, origin = dict(t), {}
    for key in INHERITED:
        members = {}
        for b in reversed(chain):
            for n, spec in (kinds[b].get(key) or {}).items():
                members[n] = spec
                origin[(key, n)] = b
        for n in t.get(key) or {}:
            origin.pop((key, n), None)
        members.update(t.get(key) or {})
        merged[key] = members
    if "tracking" not in t:
        tracking = next((kinds[b]["tracking"] for b in chain if "tracking" in kinds[b]), None)
        if tracking:
            merged["tracking"] = tracking
    merged["_from"] = origin
    return merged


def inherited(t, key, name):
    return (key, name) in (t.get("_from") or {})


def view(doc, library=None):
    """The module as the checks see it: each machine as a type, each binder
    merged with its machine, each subtype with its bases; and a map back from
    each view path to the YAML."""
    machines = doc.get("machines") or {}
    kinds = {mn: machine_as_type(m) for mn, m in machines.items()}
    kinds.update({tn: bound(t, machines) for tn, t in (doc.get("types") or {}).items()})
    kinds = {tn: dict(t, transitions={xn: resolve_any(x, t.get("states")) for xn, x in (t.get("transitions") or {}).items()})
             for tn, t in kinds.items()}
    everything = dict(library or {})
    everything.update(kinds)
    kinds = {tn: inherit(tn, t, everything) for tn, t in kinds.items()}
    v = dict(doc, types=kinds)

    def back(path):
        """The YAML path a view path stands for, or None where it is a
        machine's part of a binder, reported at the machine, or a member a
        subtype inherits, reported at the base that declares it."""
        if len(path) < 2 or path[0] != "types":
            return path
        name = path[1]
        origin = (kinds.get(name) or {}).get("_from") or {}
        if len(path) >= 4 and (path[2], path[3]) in origin:
            return None
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


def targets(x):
    t = x.get("to")
    return [] if t is None else [t] if isinstance(t, str) else list(t)


def pairs(x):
    """The (from, to) state pairs a transition takes an object along; an
    assertion and an erasure run from any state, so they have none."""
    if x["kind"] in ("assertion", "erasure"):
        return []
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


STEP_KINDS = ("assign", "clear", "add", "remove", "call", "create", "foreach", "supersede")


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
    if kind == "supersede":
        return [body]
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
                    "internal": "an internal transition declares `from` and no `to`",
                    "assertion": "an assertion declares as `to` the states it may put an object in, and no `from`: it runs from any state",
                    "erasure": "an erasure declares neither `from` nor `to`: it runs at any state"}.get(e.instance["kind"], e.message)
            out.append((tuple(e.absolute_path), "schema", rule))
        elif e.validator == "oneOf" and len(e.absolute_path) == 2 and e.absolute_path[0] == "types":
            out.append((tuple(e.absolute_path), "schema",
                        "a type declares its own states and transitions, binds a state_machine, is abstract, or is a mirror with states, "
                        "and only one of them; an abstract type has no lifecycle, conditions, observations or metrics"))
        elif e.validator == "anyOf" and len(e.absolute_path) == 2 and e.absolute_path[0] == "types":
            out.append((tuple(e.absolute_path), "schema",
                        "a type declares its tracking, extends a base that does, or is abstract"))
        elif e.validator == "oneOf" and e.absolute_path and e.absolute_path[-1] == "from":
            out.append((tuple(e.absolute_path), "schema", "`from` is a state, a list of states, or any, alone"))
        elif e.validator == "oneOf" and e.absolute_path and e.absolute_path[-1] == "unique":
            out.append((tuple(e.absolute_path), "schema",
                        "unique is true, in_scope, { with: [<attribute>, …] } or { where: <expression> }, one of them"))
        elif e.validator == "oneOf" and len(e.absolute_path) == 4 and e.absolute_path[2] == "metrics":
            # a metric is a fixed measure or a formula; report the error of the form it chose
            branch = 0 if isinstance(e.instance, dict) and "measure" in e.instance else 1
            within = [c for c in e.context if c.schema_path[0] == branch]
            plain = [c for c in within if c.validator != "oneOf"]
            if within and not plain:
                out.append((tuple(e.absolute_path), "schema",
                            "a median_time_in_state metric names a `state`, and a transition_count names a `transition`"))
            else:
                best = jsonschema.exceptions.best_match(plain or e.context)
                out.append((tuple(e.absolute_path) + tuple(best.absolute_path), "schema", best.message))
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
        for mn, m in (t.get("metrics") or {}).items():
            in_order(list(m), METRIC_ORDER, ("types", tn, "metrics", mn), f"in the metric {mn}, ")
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
                    if x["kind"] == "external" and x.get("to") in finals and xn not in covered | kept:
                        out.append((here, "names", f"{tn}.{xn} enters the final state {x['to']}, and {tn}.{an} neither cascades on it nor survives it"))
            elif "cascade" in spec or "survives" in spec:
                out.append((here, "names", f"{tn}.{an} has a cascade or survives, which only a composite end has"))
    return out


def family_errors(doc, library):
    """A base is declared and abstract, extending never comes back to itself,
    a subtype does not redeclare what it inherits, and a type with objects has
    a tracking mode, its own or inherited (the model's checks 33, 42 and 43)."""
    kinds = dict(library or {})
    kinds.update(dict(types(doc)))
    out = []
    for tn, t in types(doc):
        here = ("types", tn)
        base = t.get("extends")
        if base:
            b = kinds.get(base)
            if b is None:
                out.append((here + ("extends",), "names", f"{tn} extends {base}, which is neither declared in this module nor imported"))
            elif not b.get("abstract"):
                out.append((here + ("extends",), "names", f"{tn} extends {base}, which is not abstract; a base has no objects of its own"))
            step, seen = base, set()
            while step in kinds and step not in seen:
                seen.add(step)
                if step == tn:
                    out.append((here + ("extends",), "names", f"{tn} extends {base}, and following its bases comes back to {tn}"))
                    break
                step = kinds[step].get("extends")
        chain = bases(tn, kinds)
        for key in INHERITED:
            for n in t.get(key) or {}:
                owner = next((c for c in chain if n in (kinds[c].get(key) or {})), None)
                if owner:
                    out.append((here + (key, n), "names", f"{tn} declares '{n}', which it inherits from {owner}; a member has one declaration"))
        if not t.get("abstract") and "tracking" not in t and not any("tracking" in kinds[c] for c in chain):
            out.append((here, "names", f"{tn} has no tracking, declared or inherited"))
        members = dict(t.get("attributes") or {})
        for c in chain:
            members.update(kinds[c].get("attributes") or {})
        if t.get("mirror") and not any(a.get("external") for a in members.values()):
            out.append((here, "names", f"{tn} is a mirror and declares no external identifier, by which the import matches its objects"))
        for a, spec in (t.get("attributes") or {}).items():
            if spec.get("external") and "reference" in spec:
                out.append((here + ("attributes", a, "external"), "names", f"{tn}.{a} is a reference; an external identifier is a value the other system gives"))
    return out


BACKFILL_READS = {"state", "this"}   # a backfill reads the object's own members and literals (§6.6)


def migration_errors(doc, v):
    """What a migration maps is checked against this version: a mapping names a
    state, member or attribute this version no longer has and one it has, a
    backfill writes a stored attribute from the object's own members, and an
    admission names an invariant and a reason that exist (declaration-syntax.md §6.6)."""
    m = doc.get("migration")
    if not m:
        return []
    kinds = {tn: t for tn, t in types(v) if tn not in (doc.get("machines") or {})}
    enums = {n: set(x) for n, x in (doc.get("enumerations") or {}).items()}
    imported = {n for names in (doc.get("imports") or {}).values() for n in names}
    out = []
    for tn, maps in (m.get("removed_states") or {}).items():
        where = ("migration", "removed_states", tn)
        if tn not in kinds:
            out.append((where, "names", f"the migration moves objects of {tn}, which this module does not declare"))
            continue
        states = kinds[tn].get("states") or {}
        for old, new in maps.items():
            if old in states:
                out.append((where + (old,), "names", f"the migration maps {tn}.{old}, which this version still has; a mapping is for a state it removes"))
            if new not in states:
                out.append((where + (old,), "names", f"the migration moves {tn}.{old} to {new}, which is not a state of {tn}"))
    for en, maps in (m.get("removed_members") or {}).items():
        where = ("migration", "removed_members", en)
        if en not in enums:
            out.append((where, "names", f"the migration maps members of {en}, which this module does not declare"))
            continue
        for old, new in maps.items():
            if old in enums[en]:
                out.append((where + (old,), "names", f"the migration maps {en}.{old}, which this version still has"))
            if new not in enums[en]:
                out.append((where + (old,), "names", f"the migration maps {en}.{old} to {new}, which is not a member of {en}"))
    for tn, maps in (m.get("renamed_attributes") or {}).items():
        where = ("migration", "renamed_attributes", tn)
        if tn not in kinds:
            out.append((where, "names", f"the migration renames attributes of {tn}, which this module does not declare"))
            continue
        attrs = kinds[tn].get("attributes") or {}
        for old, new in maps.items():
            if new not in attrs:
                out.append((where + (old,), "names", f"the migration renames {tn}.{old} to {new}, which {tn} does not declare"))
            if old in attrs:
                out.append((where + (old,), "names", f"the migration renames {tn}.{old}, which this version still has"))
    for tn, maps in (m.get("backfill") or {}).items():
        where = ("migration", "backfill", tn)
        if tn not in kinds:
            out.append((where, "names", f"the migration backfills {tn}, which this module does not declare"))
            continue
        t = kinds[tn]
        members = set(t.get("attributes") or {}) | set(t.get("derived_attributes") or {}) | BACKFILL_READS
        for a, expr in maps.items():
            if a not in (t.get("attributes") or {}):
                out.append((where + (a,), "names", f"the migration backfills {tn}.{a}, which is not a stored attribute of {tn}"))
            for r in sorted(reads(expr) - members):
                out.append((where + (a,), "names", f"the backfill of {tn}.{a} reads '{r}'; a backfill reads the object's own members and literals"))
            for owner, value in re.findall(r"\b([A-Z][A-Za-z0-9]*)\.([A-Z][A-Z0-9_]*)\b", str(expr)):
                if owner in enums and value not in enums[owner]:
                    out.append((where + (a,), "names", f"the backfill of {tn}.{a} names {owner}.{value}, and {owner} has no {value}"))
    for i, adm in enumerate(m.get("admit") or []):
        where = ("migration", "admit", i)
        tn, inv = adm["invariant"].split(".")
        if tn not in kinds:
            out.append((where, "names", f"the migration admits {adm['invariant']}, and this module declares no type {tn}"))
        elif inv not in invariant_names(doc, tn, kinds[tn]):
            out.append((where, "names", f"the migration admits {adm['invariant']}, which is not an invariant of {tn}"))
        en, member = adm["reason"].split(".")
        if en in enums and member not in enums[en]:
            out.append((where, "names", f"the migration admits {adm['invariant']} because {adm['reason']}, and {en} has no {member}"))
        elif en not in enums and en not in imported:
            out.append((where, "names", f"the migration's reason {adm['reason']} names {en}, which is not an enumeration of this module"))
    return out


def migration_pair_errors(prev, doc, library):
    """This version against the one before it: every state it removes from a
    type is mapped, every mapping names what the previous version had, and a
    field added to an observation kind is optional (the model's checks 22 and
    23). What depends on the live objects, which a publish counts, is noticed:
    a new required attribute without a backfill, a removed member without a
    mapping, an attribute dropped where one of its type is added."""
    pv, _ = view(prev, library)
    v, _ = view(doc, library)
    machines = set(doc.get("machines") or {}) | set(prev.get("machines") or {})
    old, new = dict(types(pv)), dict(types(v))
    m = doc.get("migration") or {}
    fatal, notes = [], []
    for tn, t in new.items():
        if tn in machines or tn not in old or t.get("abstract"):
            continue
        ot = old[tn]
        here = ("types", tn)
        mapped = (m.get("removed_states") or {}).get(tn, {})
        for st in sorted(set(ot.get("states") or {}) - set(t.get("states") or {}) - set(mapped)):
            fatal.append((here, "names", f"{tn} no longer has the state {st}, and the migration maps it nowhere; its objects need a state to move to"))
        for st in sorted(set(mapped) - set(ot.get("states") or {})):
            fatal.append((("migration", "removed_states", tn, st), "names", f"the migration maps {tn}.{st}, which the previous version did not have"))
        renamed = (m.get("renamed_attributes") or {}).get(tn, {})
        for a in sorted(set(renamed) - set(ot.get("attributes") or {})):
            fatal.append((("migration", "renamed_attributes", tn, a), "names", f"the migration renames {tn}.{a}, which the previous version did not have"))
        added = set(t.get("attributes") or {}) - set(ot.get("attributes") or {}) - set(renamed.values())
        dropped = set(ot.get("attributes") or {}) - set(t.get("attributes") or {}) - set(renamed)
        filled = (m.get("backfill") or {}).get(tn, {})
        for a in sorted(added):
            spec = t["attributes"][a]
            kind = str(spec.get("reference", spec.get("type", "")))
            if optional(spec) or spec.get("identifier") or kind.endswith("[]"):
                continue
            if spec.get("aggregation") == "composite":
                notes.append((here + ("attributes", a), "migration", f"{tn}.{a} is a new required part: a publish over live objects of {tn} is refused, so declare it optional or a set"))
            elif a not in filled:
                notes.append((here + ("attributes", a), "migration", f"{tn}.{a} is new and required: a publish over live objects of {tn} needs a backfill for it"))
        for d in sorted(dropped):
            for a in sorted(added):
                if (ot["attributes"][d].get("type"), ot["attributes"][d].get("reference")) == (t["attributes"][a].get("type"), t["attributes"][a].get("reference")):
                    notes.append((here + ("attributes", a), "migration", f"{tn}.{d} is gone and {tn}.{a} is new, of the same type: if it is a rename, map it, or its values stay behind"))
        for o, ob in (t.get("observations") or {}).items():
            before = ((ot.get("observations") or {}).get(o) or {}).get("attributes")
            if before is None:
                continue
            for fld, spec in (ob.get("attributes") or {}).items():
                if fld not in before and not optional(spec):
                    fatal.append((here + ("observations", o, "attributes", fld), "names",
                                  f"{o}.{fld} is added to an observation kind and is required; an observation is born final, so nothing could fill it"))
    old_enums = {n: set(x) for n, x in (prev.get("enumerations") or {}).items()}
    new_enums = {n: set(x) for n, x in (doc.get("enumerations") or {}).items()}
    for en, members in new_enums.items():
        if en not in old_enums:
            continue
        mapped = (m.get("removed_members") or {}).get(en, {})
        for x in sorted(old_enums[en] - members - set(mapped)):
            notes.append((("enumerations", en), "migration", f"{en}.{x} is gone: a publish needs a mapping for it if live objects hold it, and is refused if an observation does"))
        for x in sorted(set(mapped) - old_enums[en]):
            fatal.append((("migration", "removed_members", en, x), "names", f"the migration maps {en}.{x}, which the previous version did not have"))
    return fatal, notes


def machine_errors(doc, kinds=None):
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
        full = (kinds or {}).get(tn, t)   # with what the binder inherits
        for a, want in (req.get("attributes") or {}).items():
            have = (full.get("attributes") or {}).get(a)
            if have is None:
                out.append((("types", tn, "state_machine"), "names", f"{tn} binds {mn}, which requires the attribute '{a}', and {tn} does not declare it"))
                continue
            # check 16: the same type, optionality included
            sig = lambda s: (s.get("type"), s.get("reference"), bool(s.get("optional")))
            if sig(have) != sig(want):
                out.append((("types", tn, "attributes", a), "names", f"{tn} declares '{a}' as {attribute_line(a, have).split(chr(10))[0].strip()}, and its machine {mn} requires {attribute_line(a, want).split(chr(10))[0].strip()}"))
        for i in req.get("invariants") or []:
            if i not in (full.get("invariants") or {}):
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
                if spec.get("type") == "counter":
                    out.append((("types", tn, "observations", o, "attributes", a), "names",
                                f"{o}.{a} is a counter, which an observation, born final, cannot have; a counter belongs to a type tracked by quantity"))
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
        minted = {a for a, s in attrs.items() if s.get("identifier") or "default" in s or s.get("type") == "counter"}
        used = set()
        for s, v in states.items():
            if v["category"] not in set(doc.get("categories") or []) | BUILT_IN_CATEGORIES:
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
            for s in sources(x) + targets(x):
                if s not in states:
                    key = "to" if s in targets(x) else "from"
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
            if x.get("proposable") and x.get("only_via"):
                out.append((base + ("proposable",), "names", f"{tn}.{xn} is proposable and only via other transitions; "
                                                           "no one may request it, so no proposal of it could be approved (check 14)"))
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
            out += required_errors(tn, xn, x, required, always | minted)
        for c in sorted(conds - used):
            out.append((("types", tn, "conditions", c), "names", f"condition '{c}' of {tn} is used by no transition"))
        out += value_errors(doc, tn, t)
        out += (read_errors(tn, t, set(doc.get("categories") or []) | BUILT_IN_CATEGORIES | evaluator_names(doc))
                + indexed_errors(tn, t, library) + identifier_errors(doc, tn, t, library) + evaluator_errors(doc, tn, t)
                + supersession_errors(doc, tn, t, library))
        out += override_errors(doc, tn, t, library)
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
    if x["kind"] in ("assertion", "erasure"):
        return out   # it is checked against the invariants when it applies, and admits what it breaks
    src = sources(x)
    into = [x["to"]] if x.get("to") else src
    for s in into:
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
    exprs += [(("types", tn, "attributes", a, "default"), f"{tn}.{a}'s default", spec["default"])
              for a, spec in (t.get("attributes") or {}).items() if "default" in spec]
    exprs += [(("types", tn, "attributes", a, "unique", "where"), a, spec["unique"]["where"])
              for a, spec in (t.get("attributes") or {}).items() if isinstance(spec.get("unique"), dict) and "where" in spec["unique"]]
    exprs += [(("types", tn, "transitions", xn, "effect") + path, xn, e) for xn, x in (t.get("transitions") or {}).items()
              for st, kind, path, _d in walk(x.get("effect")) for e in step_expressions(st, kind)]
    exprs += [(("types", tn, "observations", o, "invariants", n, "expression"), n, i["expression"])
              for o, ob in (t.get("observations") or {}).items() for n, i in (ob.get("invariants") or {}).items()]
    for mn, m in (t.get("metrics") or {}).items():
        if "source" in m:
            parts = [(k, m[k]) for k in ("filter", "time_dimension", "expression") if m.get(k)]
            parts += [(("dimensions", d), e) for d, e in (m.get("dimensions") or {}).items()]
            parts += [(("flag_when", f), c) for f, c in (m.get("flag_when") or {}).items()]
            exprs += [(("types", tn, "metrics", mn) + (k if isinstance(k, tuple) else (k,)), f"metric {mn}", e) for k, e in parts]
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
# what exists only while a request is being made: not readable by a derivation or a uniqueness condition
REQUEST_ONLY = {"inputs", "this_event"}
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


def read_errors(tn, t, categories=()):
    """Every name an expression reads is a member of the object it is evaluated
    on: for a type, its attributes and observation kinds; for an observation
    kind's invariant, that kind's fields. An effect also reads the names its
    foreach and create steps bind."""
    derived = list(t.get("derived_attributes") or {})
    stored = set(t.get("attributes") or {}) | set(t.get("observations") or {})
    members = stored | set(derived) | NOT_AN_ATTRIBUTE | set(categories)
    exprs = [(("types", tn, sec, n, "expression"), n, c["expression"], members)
             for sec in ("conditions", "invariants") for n, c in (t.get(sec) or {}).items()]
    exprs += [(("types", tn, "attributes", a, "unique", "where"), f"{tn}.{a}'s uniqueness condition", spec["unique"]["where"], members - REQUEST_ONLY)
              for a, spec in (t.get("attributes") or {}).items() if isinstance(spec.get("unique"), dict) and "where" in spec["unique"]]
    out = []
    for i, d in enumerate(derived):
        path = ("types", tn, "derived_attributes", d, "expression")
        read = reads(t["derived_attributes"][d]["expression"])
        for r in sorted(read & set(derived[i:])):
            out.append((path, "order", f"derived attribute '{d}' reads itself" if r == d else
                        f"derived attribute '{d}' reads '{r}', which is declared after it; a derived attribute reads only those above it"))
        exprs.append((path, f"derived attribute '{d}'", t["derived_attributes"][d]["expression"], (members - REQUEST_ONLY) | set(derived[i:])))
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
    return out + [(path, "names", f"{name} reads '{r}', which only a transition has" if r in REQUEST_ONLY else
                                  f"{name} reads labels, which are read only as a metric's source, source: labels (check 55)" if r == "labels" else
                                  f"{name} reads '{r}', which is not a field of the observation {path[3]}" if "observations" in path else
                                  f"{name} reads '{r}', which {tn} does not declare")
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


BUILT_IN_TYPES = ("string", "bool", "int", "decimal", "money", "timestamp", "duration", "identity", "file", "event")


def is_text(spec):
    """A string, or an enumeration, which a format fills with its member's name."""
    kind = re.match(r"[A-Za-z_]+", str(spec.get("type", ""))).group(0) if spec.get("type") else ""
    return not str(spec.get("type", "")).endswith("[]") and (kind == "string" or (kind[:1].isupper() and kind not in BUILT_IN_TYPES))


def creation_writes(t):
    """What every creation of the type writes, the set the model's check 8 uses:
    its required inputs, the attributes it assigns a provably present value,
    and those a default or a counter gives a value."""
    given = {a for a, s in (t.get("attributes") or {}).items() if "default" in s or s.get("type") == "counter"}
    sets = []
    for x in (t.get("transitions") or {}).values():
        if x["kind"] == "initial":
            req = set(x.get("required_inputs", []))
            sets.append(given | req | {loc for e, loc in assigns(x) if present(e, req, required_input_names(x))})
    return set.intersection(*sets) if sets else set()


def identifier_errors(doc, tn, t, library):
    """A minted identifier and the uniqueness forms (the model's check 44). An
    abstract type has no creations; each type extending it is checked instead,
    an inherited identifier reported at that type."""
    if t.get("abstract"):
        return []
    attrs = t.get("attributes") or {}
    imported = {n for names in (doc.get("imports") or {}).values() for n in names}
    written = creation_writes(t)
    out = []
    for a, spec in attrs.items():
        here = ("types", tn) if inherited(t, "attributes", a) else ("types", tn, "attributes", a)
        u = spec.get("unique")
        ident = spec.get("identifier")
        if isinstance(u, dict):
            for w in u.get("with", []):
                if w not in attrs:
                    out.append((here + ("unique",), "names", f"{tn}.{a} is unique with '{w}', which {tn} does not declare"))
        if u == "in_scope" and not (ident or {}).get("scope"):
            out.append((here + ("unique",), "names", f"{tn}.{a} is unique in scope, and has no identifier with a scope"))
        if spec.get("type") == "counter":
            if optional(spec):
                out.append((here, "names", f"{tn}.{a} is a counter, which is never absent, so it is not optional"))
            if "default" in spec:
                out.append((here, "names", f"{tn}.{a} is a counter, which starts at zero, so it has no default"))
        if "default" in spec:
            for r in sorted(reads(spec["default"]) - {"now"}):
                out.append((here + ("default",), "names", f"{tn}.{a}'s default reads '{r}'; a default is written before anything else, so it is a literal, a qualified value or now"))
        if not ident:
            continue
        where = here + ("identifier",)
        if spec.get("type") != "string":
            out.append((where, "names", f"{tn}.{a} has an identifier, which is minted as text, so its type is string"))
        if ident["sequence"] not in (doc.get("sequences") or {}) and ident["sequence"] not in imported:
            out.append((where + ("sequence",), "names", f"{tn}.{a} is minted from {ident['sequence']}, which is neither a sequence of this module nor imported"))
        sc = ident.get("scope")
        if sc is not None:
            ss = attrs.get(sc)
            if ss is None:
                out.append((where + ("scope",), "names", f"{tn}.{a} is scoped by '{sc}', which {tn} does not declare"))
            elif str(ss.get("reference", ss.get("type", ""))).endswith("[]"):
                out.append((where + ("scope",), "names", f"{tn}.{a} is scoped by '{sc}', a set; a scope is one value"))
            elif "reference" not in ss and not ss.get("indexed"):
                out.append((where + ("scope",), "names", f"{tn}.{a} is scoped by '{sc}', which is not indexed"))
            elif sc not in written:
                out.append((where + ("scope",), "names", f"{tn}.{a} is scoped by '{sc}', which not every creation of {tn} writes"))
        fmt = ident["format"]
        depth, numbered = 0, False
        for m in re.finditer(r"\[|\]|\{([^{}]*)\}", fmt):
            if m.group(0) == "[":
                depth += 1
                continue
            if m.group(0) == "]":
                if depth == 0:
                    out.append((where + ("format",), "names", f"{tn}.{a}'s format closes a ] it did not open"))
                depth = max(0, depth - 1)
                continue
            ph = m.group(1)
            if re.fullmatch(r"n(:.*)?", ph):
                numbered = True
                if not re.fullmatch(r"n(:\d+)?", ph):
                    out.append((where + ("format",), "names", f"{tn}.{a}'s format has {{{ph}}}, whose width is not a number of digits"))
                continue
            hops = ph.split(".")
            if not all(re.fullmatch(r"[a-z][a-z0-9_]*", h) for h in hops):
                out.append((where + ("format",), "names", f"{tn}.{a}'s format has {{{ph}}}, which is not {{n}}, {{n:<width>}}, {{<attribute>}} or {{<reference>.<attribute>}}"))
                continue
            if len(hops) > 2:
                out.append((where + ("format",), "names", f"{tn}.{a}'s format reads {ph}, more than one reference away; a placeholder reaches one hop"))
                continue
            first = attrs.get(hops[0])
            if first is None:
                out.append((where + ("format",), "names", f"{tn}.{a}'s format reads '{hops[0]}', which {tn} does not declare"))
                continue
            if len(hops) == 2 and ("reference" not in first or first["reference"].endswith("[]")):
                out.append((where + ("format",), "names", f"{tn}.{a}'s format reads through '{hops[0]}', which is not a single reference"))
                continue
            if hops[0] not in written:
                out.append((where + ("format",), "names", f"{tn}.{a}'s format reads '{hops[0]}', which not every creation of {tn} writes before the mint"))
                continue
            target = first
            if len(hops) == 2:
                other = library.get(first["reference"])
                if other is None:
                    continue
                target = (other.get("attributes") or {}).get(hops[1])
                if target is None:
                    out.append((where + ("format",), "names", f"{tn}.{a}'s format reads {ph}, and {first['reference']} has no attribute '{hops[1]}'"))
                    continue
            if not is_text(target):
                out.append((where + ("format",), "names", f"{tn}.{a}'s format reads {ph}, which is not a string or an enumeration"))
            elif optional(target) and depth == 0:
                out.append((where + ("format",), "names", f"{tn}.{a}'s format reads {ph}, which is optional, outside [ … ]; a segment that may be absent goes in brackets"))
        if depth:
            out.append((where + ("format",), "names", f"{tn}.{a}'s format leaves a [ open"))
        if not numbered:
            out.append((where + ("format",), "names", f"{tn}.{a}'s format has no {{n}} or {{n:<width>}}, so it would mint one value only"))
    return out


# the members of a row of each source, beside a type's or a kind's own (declaration-syntax.md §6.9)
OBJECT_ROW = {"id", "state", "open", "created_at", "created_by_kind", "recorded_from", "declaration_version",
              "entered_at", "time_in", "intervals", "transitions", "attempts"}
KIND_ROW = {"subject", "corrects", "occurred_at", "recorded_at", "recorded_by_kind", "declaration_version"}
DATASET_ROWS = {
    "intervals": {"object", "state", "entered_at", "left_at", "duration", "entered_by_kind", "declaration_version", "legacy", "held"},
    "transitions": {"object", "transition", "from_state", "to_state", "completes", "returns", "occurred_at", "recorded_at",
                    "actor_id", "actor_kind", "asserted", "overrides", "imported", "migrated", "redacted", "reason",
                    "declaration_version", "held"},
    "attempts": {"object", "transition", "verdict", "clause", "remedy", "unknown", "enforced", "flagged", "actor_id",
                 "actor_kind", "at", "declaration_version"},
    "attempt_counts": {"day", "object", "transition", "verdict", "clause", "remedy", "actor_kind", "enforced", "flagged",
                       "declaration_version", "count"},
    # every type carries labels without declaring any; a label's note is personal and not readable (§6.8)
    "labels": {"subject", "name", "subject_state", "occurred_at", "recorded_at", "recorded_by_kind", "declaration_version"},
}
# every metric has these dimensions unless it declares them (declaration-syntax.md §6.9)
STANDARD_DIMENSIONS = {"version", "actor_kind"}


def row_members(t, source):
    """The members a row of the source has, or None if the type has no such source."""
    m = re.fullmatch(r"intervals\(([a-z][a-z0-9_]*)\)", source)
    if m:
        return DATASET_ROWS["intervals"] - {"state"} | {"value"}
    if source in DATASET_ROWS:
        return DATASET_ROWS[source]
    if source == "objects":
        return set(t.get("attributes") or {}) | set(t.get("observations") or {}) | set(t.get("derived_attributes") or {}) | OBJECT_ROW
    ob = (t.get("observations") or {}).get(source)
    return None if ob is None else set(ob.get("attributes") or {}) | KIND_ROW


def formula_errors(tn, t, mn, m):
    """A metric in the metric language reads its rows through its item, and its
    flags read its value and its dimensions."""
    base = ("types", tn, "metrics", mn)
    source, item = m["source"], m["item"]
    members = row_members(t, source)
    out = []
    if members is None:
        out.append((base + ("source",), "names", f"metric {mn} reads '{source}', which is not objects, intervals, intervals(<member>), "
                                                 f"transitions, attempts, attempt_counts, labels or an observation kind of {tn}"))
    tracked = re.fullmatch(r"intervals\(([a-z][a-z0-9_]*)\)", source)
    if tracked and tracked.group(1) not in (t.get("attributes") or {}) and tracked.group(1) != "state":
        out.append((base + ("source",), "names", f"metric {mn} reads the intervals of '{tracked.group(1)}', which {tn} does not declare"))
    if item in NOT_AN_ATTRIBUTE | KEYWORDS | {"value"}:
        out.append((base + ("item",), "names", f"metric {mn} names its rows '{item}', which an expression reads as something else"))
    for d in (m.get("dimensions") or {}):
        if d == "value":
            out.append((base + ("dimensions", d), "names", f"metric {mn} has a dimension named 'value', which its flags read as the metric's value"))
    parts = [(("filter",), m.get("filter")), (("time_dimension",), m.get("time_dimension")), (("expression",), m["expression"])]
    parts += [(("dimensions", d), e) for d, e in (m.get("dimensions") or {}).items()]
    for key, text in parts:
        if text is None:
            continue
        for r in sorted(reads(text) - {item, "now"}):
            out.append((base + key, "names", f"metric {mn} reads '{r}'; a metric reads its rows through '{item}'"))
        for r in sorted(set(re.findall(rf"(?<![\w.]){item}\.([a-z][a-z0-9_]*)", " ".join(str(text).split())))):
            if members is not None and r not in members:
                out.append((base + key, "names", f"metric {mn} reads {item}.{r}, a label's note, which is personal and not readable"
                            if source == "labels" and r == "note" else f"metric {mn} reads {item}.{r}, and a row of {source} has no member '{r}'"))
    dims = set(m.get("dimensions") or {}) | STANDARD_DIMENSIONS
    for f, c in (m.get("flag_when") or {}).items():
        for r in sorted(reads(c) - dims - {"value", "now"}):
            out.append((base + ("flag_when", f), "names", f"metric {mn}'s flag {f} reads '{r}', which is neither value nor a dimension"))
    return out


def invariant_names(doc, tn, t):
    """The invariants of a type: declared, generated for a state's required
    attributes, and, for a machine, those it requires of its binders."""
    names = set(t.get("invariants") or {})
    names |= {f"{s.lower()}_invariant" for s, v in (t.get("states") or {}).items() if (v or {}).get("required_attributes")}
    machine = (doc.get("machines") or {}).get(tn)
    if machine:
        names |= set((machine.get("requires") or {}).get("invariants") or [])
    return names


def related(tn, library):
    """The types reachable from a type by relationships whose ends name each other."""
    seen, todo = set(), [tn]
    while todo:
        for spec in ((library.get(todo.pop()) or {}).get("attributes") or {}).values():
            other = str(spec.get("reference", "")).rstrip("[]")
            if spec.get("opposite") and other and other not in seen and other != tn:
                seen.add(other)
                todo.append(other)
    return seen


def erasure_calls(x, attrs):
    """(part attribute, transition) for each call an effect makes on a part:
    on the attribute itself, or on a foreach item over it."""
    items, out = {}, set()
    for st, kind, _path, _d in walk(x.get("effect")):
        if kind == "foreach" and "array" in st["foreach"]:
            items[st["foreach"]["item"]] = " ".join(str(st["foreach"]["array"]).split())
        if kind == "call":
            target = " ".join(st["call"]["target"].split())
            target = items.get(target, re.sub(r"^this\.", "", target))
            if target in attrs:
                out.add((target, st["call"]["transition"]))
    return out


def override_errors(doc, tn, t, library):
    """Assertions, erasures and corrections (declaration-syntax.md §6.3, §6.4):
    the model's checks 9, 27, 28, 29 and 39."""
    attrs = t.get("attributes") or {}
    enums = set(doc.get("enumerations") or {}) | {n for names in (doc.get("imports") or {}).values() for n in names}
    out = []
    for a, spec in attrs.items():
        if spec.get("personal") and not optional(spec):
            out.append((("types", tn, "attributes", a), "names", f"{tn}.{a} is personal and required; erasure writes absence, so a personal attribute is optional"))
    for o, ob in (t.get("observations") or {}).items():
        for a, spec in (ob.get("attributes") or {}).items():
            if spec.get("personal") and not optional(spec):
                out.append((("types", tn, "observations", o, "attributes", a), "names",
                            f"{o}.{a} is personal and required; erasure writes absence, so a personal field is optional"))
    own = invariant_names(doc, tn, t)
    reach = related(tn, library)
    for xn, x in (t.get("transitions") or {}).items():
        base = ("types", tn, "transitions", xn)
        kind = x["kind"]
        inputs = x.get("inputs") or {}
        reason = inputs.get("reason")
        needs = {"assertion": "an assertion", "erasure": "an erasure"}.get(kind) or ("a correction" if x.get("corrects") else None)
        if needs and reason is None:
            out.append((base, "names", f"{tn}.{xn} is {needs}, and declares no input 'reason'"))
        if kind == "assertion":
            if reason is not None and reason.get("type") not in enums:
                out.append((base + ("inputs", "reason"), "names", f"{tn}.{xn}'s reason is not an enumeration; an assertion's reasons are counted, so they are declared values"))
            for i in sorted({"to", "admits"} & set(inputs)):
                out.append((base + ("inputs", i), "names", f"{tn}.{xn} declares the input '{i}', which an assertion has already: the state it asks for, or the invariants it admits"))
        if x.get("may_admit") and kind != "assertion":
            out.append((base + ("may_admit",), "names", f"{tn}.{xn} may admit invariants, which only an assertion may"))
        for inv in x.get("may_admit") or []:
            if "." in inv:
                on, name = inv.split(".")
                if on not in reach:
                    out.append((base + ("may_admit",), "names", f"{tn}.{xn} may admit {inv}, and {on} is not reachable from {tn} by a relationship whose ends name each other"))
                elif name not in invariant_names(doc, on, library.get(on) or {}):
                    out.append((base + ("may_admit",), "names", f"{tn}.{xn} may admit {inv}, which is not an invariant of {on}"))
            elif inv not in own:
                out.append((base + ("may_admit",), "names", f"{tn}.{xn} may admit '{inv}', which is not an invariant of {tn}"))
        if x.get("corrects"):
            if kind not in ("internal", "external"):
                out.append((base + ("corrects",), "names", f"{tn}.{xn} corrects attributes, which only an internal or external transition may"))
            written = set(x.get("required_inputs", [])) | set(x.get("optional_inputs", []))
            written |= {st[k]["location"] for st, k, _p, _d in walk(x.get("effect")) if k in ("assign", "add", "remove")}
            written |= {a for st, k, _p, _d in walk(x.get("effect")) if k == "clear" for a in st["clear"]}
            for a in sorted(set(x["corrects"]) - written):
                out.append((base + ("corrects",), "names", f"{tn}.{xn} corrects '{a}' and does not write it"))
            for a in sorted(written - set(x["corrects"])):
                out.append((base + ("corrects",), "names", f"{tn}.{xn} writes '{a}', which it does not list in corrects; a correction writes exactly what it corrects"))
        if kind == "erasure":
            calls = erasure_calls(x, attrs)
            for a, spec in attrs.items():
                part = library.get(str(spec.get("reference", "")).rstrip("[]"))
                if spec.get("aggregation") != "composite" or part is None:
                    continue
                if not any(s.get("personal") for s in (part.get("attributes") or {}).values()):
                    continue
                erasures = {n for n, px in (part.get("transitions") or {}).items() if px["kind"] == "erasure"}
                if not any(c == a and n in erasures for c, n in calls):
                    out.append((base, "names", f"{tn}.{xn} erases {tn} and does not call an erasure of its parts in '{a}', "
                                               "which hold personal attributes"))
    return out


def supersession_errors(doc, tn, t, library):
    """A transition into a superseding state names its successor with a
    supersede step, and only such a transition does; the successor is an input
    or a name an earlier create binds, never the object itself; and a type with
    personal data that takes part in supersession declares an erasure, since the
    chain is erased through each of its members (declaration-syntax.md §6.1,
    §6.4; the model's checks 26, 49 and 60)."""
    states = t.get("states") or {}
    out = []
    for xn, x in (t.get("transitions") or {}).items():
        where = ("types", tn, "transitions", xn)
        into = states.get(x.get("to")) if isinstance(x.get("to"), str) else None
        steps = [(st, path) for st, kind, path, _d in walk(x.get("effect")) if kind == "supersede"]
        superseding = bool(into and into.get("superseding"))
        if steps and not superseding:
            out.append((where + ("effect",), "names", f"{tn}.{xn} supersedes, and does not enter a superseding state"))
        if superseding and not steps:
            out.append((where + ("to",), "names", f"{tn}.{xn} enters the superseding state {x['to']} and names no successor with supersede"))
        bound = []
        for st, kind, path, _d in walk(x.get("effect")):
            if kind == "create" and st["create"].get("result"):
                bound.append(st["create"]["result"])
            if kind != "supersede":
                continue
            who = " ".join(str(st["supersede"]).split())
            m = re.fullmatch(r"inputs\.([a-z][a-z0-9_]*)", who)
            if who == "this":
                out.append((where + ("effect",) + path, "names", f"{tn}.{xn} supersedes the object with itself"))
            elif m:
                spec = (x.get("inputs") or {}).get(m.group(1))
                if spec is not None and "reference" not in spec:   # an input it does not take is refused as any read of one is
                    out.append((where + ("effect",) + path, "names", f"{tn}.{xn} supersedes with inputs.{m.group(1)}, which is not an object"))
            elif who not in bound:
                out.append((where + ("effect",) + path, "names", f"{tn}.{xn} supersedes with {who}, which is neither an input nor a name an earlier create binds (check 49)"))
    takes_part = any(v.get("superseding") for v in states.values())
    for other in library.values():
        for x in (other.get("transitions") or {}).values():
            for st, kind, _p, _d in walk(x.get("effect")):
                if kind == "supersede":
                    m = re.fullmatch(r"inputs\.([a-z][a-z0-9_]*)", " ".join(str(st["supersede"]).split()))
                    ref = ((x.get("inputs") or {}).get(m.group(1)) or {}).get("reference") if m else None
                    made = [s2["create"]["type"] for s2, k2, _p2, _d2 in walk(x.get("effect")) if k2 == "create"
                            and s2["create"].get("result") == " ".join(str(st["supersede"]).split())]
                    takes_part |= ref == tn or tn in made
    personal = any(sp.get("personal") for sp in (t.get("attributes") or {}).values())
    erasable = any(x["kind"] == "erasure" for x in (t.get("transitions") or {}).values())
    if takes_part and personal and not erasable:
        out.append((("types", tn), "names", f"{tn} takes part in supersession and holds personal data, and declares no erasure, through which a superseded chain is erased (check 60)"))
    return out


def evaluator_names(doc):
    """The evaluators a module declares or imports: a guard calls one as <evaluator>.<function>(…)."""
    return set(doc.get("evaluators") or {}) | {n for names in (doc.get("imports") or {}).values() for n in names if n[:1].islower()}


def evaluator_calls(text, names):
    """(evaluator, function, number of arguments) for each call an expression makes to an evaluator."""
    text = " ".join(str(text).split())
    out = []
    for m in re.finditer(r"(?<![\w.])([a-z][a-z0-9_]*)\.([a-z][a-z0-9_]*)\(", text):
        if m.group(1) not in names:
            continue
        depth, i, commas = 1, m.end(), 0
        while i < len(text) and depth:
            depth += {"(": 1, ")": -1}.get(text[i], 0)
            commas += text[i] == "," and depth == 1
            i += 1
        body = text[m.end():i - 1].strip()
        out.append((m.group(1), m.group(2), 0 if not body else commas + 1))
    return out


def evaluator_errors(doc, tn, t):
    """A guard, and only a guard, asks an evaluator: the call names a declared
    function with its arguments, is the whole condition or its negation, and a
    condition marked eager or deferred makes one (declaration-syntax.md §6.2,
    §5.1; the model's checks 19, 24 and 45)."""
    evaluators = doc.get("evaluators") or {}
    names = evaluator_names(doc)
    out = []
    for cn, c in (t.get("conditions") or {}).items():
        where = ("types", tn, "conditions", cn)
        text = " ".join(str(c["expression"]).split())
        calls = evaluator_calls(text, names)
        for ev, fn, n in calls:
            if ev in evaluators:
                f = (evaluators[ev].get("functions") or {}).get(fn)
                if f is None:
                    out.append((where + ("expression",), "names", f"condition {cn} calls {ev}.{fn}, which {ev} does not declare"))
                elif n != len(f.get("arguments") or {}):
                    out.append((where + ("expression",), "names", f"condition {cn} passes {n} argument(s) to {ev}.{fn}, which takes {len(f.get('arguments') or {})}"))
        if calls and not re.fullmatch(r"(not\s+)?[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*\(.*\)", text):
            out.append((where + ("expression",), "names", f"condition {cn} calls an evaluator inside a larger expression; a verdict is the whole condition or its negation (check 24)"))
        if "evaluation" in c and not calls:
            out.append((where + ("evaluation",), "names", f"condition {cn} is marked {c['evaluation']}, and calls no evaluator (check 45)"))
    others = [(("types", tn, sec, n, "expression"), n, x["expression"]) for sec in ("invariants", "derived_attributes") for n, x in (t.get(sec) or {}).items()]
    others += [(("types", tn, "transitions", xn, "effect") + path, xn, e) for xn, x in (t.get("transitions") or {}).items()
               for st, kind, path, _d in walk(x.get("effect")) for e in step_expressions(st, kind)]
    for path, name, text in others:
        for ev, fn, _n in evaluator_calls(text, names):
            out.append((path, "names", f"{name} calls {ev}.{fn}; only a guard asks an evaluator"))
    return out


def metric_errors(tn, t, mn, m):
    if "source" in m:
        return formula_errors(tn, t, mn, m)
    base = ("types", tn, "metrics", mn)
    trans = t.get("transitions") or {}
    out = []
    if m.get("measure") == "transition_count":
        xn = m["transition"]
        if xn not in trans:
            return [(base + ("transition",), "names", f"metric {mn} counts '{xn}', which is not a transition of {tn}")]
        if trans[xn]["kind"] in ("assertion", "erasure"):
            return [(base + ("transition",), "names", f"metric {mn} counts '{xn}', an {trans[xn]['kind']}, which runs from any state; "
                                                      "count it with a formula over transitions, by t.transition")]
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
def markings(spec):
    """The markings an attribute or a reference carries after its type, in the
    model's words (declaration-syntax.md §3.1, §3.2)."""
    marks = [f'external "{spec["external"]}"'] if spec.get("external") else []
    if "default" in spec:
        marks.append("default " + " ".join(str(spec["default"]).split()))
    marks += [m for m in ("indexed", "personal") if spec.get(m)]
    u = spec.get("unique")
    if u is True:
        marks.append("unique")
    elif u == "in_scope":
        marks.append("unique in scope")
    elif isinstance(u, dict) and "with" in u:
        marks.append("unique with " + ", ".join(u["with"]))
    elif isinstance(u, dict) and "where" in u:
        marks.append("unique where " + " ".join(u["where"].split()))
    return "".join(" " + m for m in marks)


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
                + (" assignee" if spec.get("assignee") else "") + markings(spec))
    if spec["type"] == "counter" and not observation:
        return f"counter {name}" + markings(spec)
    t = spec["type"] + ("?" if optional(spec) else "")
    if observation:
        return (f"field {name} : {t}" + (f' unit "{spec["unit"]}"' if spec.get("unit") else "")
                + (" personal" if spec.get("personal") else ""))
    marks = [f"actor {spec['actor_kind']}"] if spec.get("actor_kind") else []
    ident = spec.get("identifier")
    if ident:
        marks.append(f"identifier from {ident['sequence']}" + (f" scoped by {ident['scope']}" if ident.get("scope") else "")
                     + f' format "{ident["format"]}"')
    return f"attr {name} {t}" + "".join(" " + m for m in marks) + markings(spec)


def transition_lines(xn, x, t, X, T):
    """One transition in the internal form, each line with its YAML path; t
    supplies the attributes and conditions its guards name."""
    conds = t.get("conditions") or {}
    src = sources(x)
    where = src[0] if len(src) == 1 else "{ " + ", ".join(src) + " }"
    into = targets(x)
    onto = (into[0] if len(into) == 1 else "{ " + ", ".join(into) + " }") if into else ""
    head = {"initial": f"create {xn} -> {onto}", "external": f"do {xn} {where} -> {onto}",
            "internal": f"act {xn} at {where}", "assertion": f"assert {xn} -> {onto}", "erasure": f"erase {xn}"}[x["kind"]]
    if x.get("only_via"):
        head += " only via " + ", ".join(x["only_via"])
    accepts = x.get("required_inputs", []) + x.get("optional_inputs", [])
    if accepts:
        head += " accepts " + ", ".join(accepts)
    if x.get("proposable"):
        head += " proposable"
    if x.get("backdating_limit"):
        head += f" backdatable within {x['backdating_limit']}"
    body = []
    for i, spec in (x.get("inputs") or {}).items():
        kind = spec.get("type") or spec.get("reference")
        line = f"input {i} : {kind}" + ("?" if spec.get("optional") else "")
        if "default" in spec:
            line += f" default {' '.join(spec['default'].split())}"
        body.append((line + (" personal" if spec.get("personal") else ""), X + ("inputs", i)))
    if x["kind"] == "assertion":
        # the target and the admissions are the request's inputs (declaration-syntax.md §6.3, check 38)
        body.append(("input to : state", X + ("to",)))
        if x.get("may_admit"):
            body.append(("input admits : invariant[]?", X + ("may_admit",)))
    body += [(f"require {a}_provided: inputs.{a} is not null because self_serviceable", X + ("required_inputs",))
             for a in provided_guards(t, x)]
    for g, mode in (x.get("guards") or {}).items():
        if g in conds:
            c = conds[g]
            mark = {"deny": "", "audit": " observe", "warn": " flag"}[mode]
            when = f" {c['evaluation']}" if c.get("evaluation") else ""
            body.append((f"require {g}: {' '.join(c['expression'].split())}{when}{mark} because {c['remedy']}", X + ("guards", g)))
    if x.get("may_admit"):
        body.append(("may admit " + ", ".join(x["may_admit"]), X + ("may_admit",)))
    if x.get("corrects"):
        body.append(("corrects " + ", ".join(x["corrects"]), X + ("corrects",)))
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
    for sq, spec in (doc.get("sequences") or {}).items():
        emit(f"# {spec['description']}", ("sequences", sq))
        emit(f"sequence {sq} version 1", ("sequences", sq))
    for en, ev in (doc.get("evaluators") or {}).items():
        E = ("evaluators", en)
        emit(f"# {ev['description']}", E)
        emit(f"evaluator {en} version 1 {{", E)
        for fn, f in ev["functions"].items():
            args = ", ".join(f"{a} : {s.get('type') or s.get('reference')}" for a, s in (f.get("arguments") or {}).items())
            emit(f"  fn {fn}({args})" + (f" fresh {f['fresh']}" if f.get("fresh") else ""), E + ("functions", fn))
        emit("}", E)
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
            emit(f"  state {sn} category {st['category']}" + (" terminal" if st.get("final") else "")
                 + (" superseding" if st.get("superseding") else ""), M + ("states", sn))
        for xn, x in m["transitions"].items():
            for line, path in transition_lines(xn, x, machine_as_type(m), M + ("transitions", xn), M):
                emit(line, path)
        emit("}", M)
    for tn, t in types(doc):
        T = ("types", tn)
        conds = t.get("conditions") or {}
        emit(f"# {t['description']}", T)
        emit(f"type {tn}" + (f" extends {t['extends']}" if t.get("extends") else "") + " version 1"
             + (" abstract" if t.get("abstract") else "") + (" mirror" if t.get("mirror") else "") + " {", T)
        if t.get("tracking"):
            emit(f"  tracking {t['tracking']}", T + ("tracking",))
        if t.get("state_machine"):
            emit(f"  machine  {t['state_machine']}", T + ("state_machine",))
        for sn, v in (t.get("states") or {}).items():
            emit(f"  state {sn} category {v['category']}" + (" terminal" if v.get("final") else "")
                 + (" superseding" if v.get("superseding") else ""), T + ("states", sn))
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
            tail += metric_text(tn, merged, mn, m)
    for line, path in tail:
        emit(line, path)
    mig = doc.get("migration") or {}
    for tn, maps in (mig.get("removed_states") or {}).items():
        for old, new in maps.items():
            emit(f"removed state  {old} -> {new}", ("migration", "removed_states", tn, old))
    for en, maps in (mig.get("removed_members") or {}).items():
        for old, new in maps.items():
            emit(f"removed member {en}.{old} -> {new}", ("migration", "removed_members", en, old))
    for tn, maps in (mig.get("renamed_attributes") or {}).items():
        for old, new in maps.items():
            emit(f"renamed attr   {old} -> {new}", ("migration", "renamed_attributes", tn, old))
    for tn, maps in (mig.get("backfill") or {}).items():
        for a, expr in maps.items():
            emit(f"backfill       {a} := {' '.join(str(expr).split())}", ("migration", "backfill", tn, a))
    for i, adm in enumerate(mig.get("admit") or []):
        emit(f"admit          {adm['invariant']} because {adm['reason']}", ("migration", "admit", i))
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
        elif "supersede" in st:
            out.append((f"{indent}supersede {one(st['supersede'])}", path))
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
    if "source" in m:
        one = lambda e: " ".join(str(e).split())
        src = m["source"]
        ob = (t.get("observations") or {}).get(src)
        rows = tn if src == "objects" else ob["kind"] if ob else f"{tn}.{src}"
        out.append((f"  from      {m['item']} in {rows}" + (f" where {one(m['filter'])}" if m.get("filter") else ""), M + ("source",)))
        if m.get("dimensions"):
            out.append(("  by        " + ", ".join(f"{d} = {one(e)}" for d, e in m["dimensions"].items()), M + ("dimensions",)))
        if m.get("time_dimension"):
            out.append((f"  window on {one(m['time_dimension'])}", M + ("time_dimension",)))
        out.append((f"  value     {one(m['expression'])}", M + ("expression",)))
        out += [(f"  flag      {f} when {one(c)}", M + ("flag_when", f)) for f, c in (m.get("flag_when") or {}).items()]
        out.append(("}", M))
        return out
    tracked = {a for a, s in (t.get("attributes") or {}).items() if s.get("assignee")}
    if m["measure"] == "median_time_in_state":
        v, time, value = "i", "i.entered_at", "median(i.duration)"
        out.append((f"  from      i in {tn}.intervals where i.state == {tn}.{m['state']}", M + ("state",)))
    else:
        v, time, value = "t", "t.occurred_at", "count()"
        conds = [(f"t.from_state == {tn}.{a}" if a else "t.from_state is null") + f" and t.to_state == {tn}.{b}"
                 for a, b in pairs(resolve_any(t["transitions"][m["transition"]], t.get("states")))]
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


def check(files, previous=None):
    """Findings and notices over modules given in import order, as (file, line,
    severity, code, message); previous maps a file to the text of the version
    before it, against which its migration is checked."""
    found, notes, loaded, library = [], [], [], {}
    for f, text in files:
        try:
            idx = line_index(text)
            doc = load(text)
        except yaml.YAMLError as e:
            mark = getattr(e, "problem_mark", None)
            problem = " ".join(str(getattr(e, "problem", e)).split())
            if problem == "mapping values are not allowed here":
                # an aggregate's body, sum(x in c: body), puts ": " inside a plain scalar
                problem += "; a value containing a colon and a space, such as an aggregate's body, is quoted or written after >-"
            found.append((f, mark.line + 1 if mark else 1, "fatal", "yaml", problem))
            continue
        loaded.append((f, text, idx, doc))
        errors = schema_errors(doc)
        if errors:
            found += [(f, line_of(idx, p), "fatal", c, m) for p, c, m in errors]
            continue
        v, back = view(doc, library)
        seen = set()
        pair, pair_notes = [], []
        if (previous or {}).get(f):
            pair, pair_notes = migration_pair_errors(load(previous[f]), doc, library)
        notes += [(f, line_of(idx, p), "notice", c, m) for p, c, m in pair_notes]
        for p, c, m in (order_errors(doc) + family_errors(doc, library) + machine_errors(doc, dict(types(v)))
                        + name_errors(v, library, ordered=False) + migration_errors(doc, v) + pair):
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


def self_test(people, service, inventory, delivery, approvals, customers, servicedesk, service_v2):
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
        ("an identifier minted from a sequence nobody declares", "names", "delivery.yaml",
         plant(delivery, "sequence: delivery_number, format", "sequence: delivery_serial, format")),
        ("an identifier scoped by what not every creation writes", "names", "delivery.yaml",
         plant(delivery, "sequence: delivery_number, format", "sequence: delivery_number, scope: courier, format")),
        ("an identifier whose format has no number", "names", "delivery.yaml",
         plant(delivery, 'format: "DLV-{n:6}"', 'format: "DLV"')),
        ("an identifier whose format reads an optional attribute outside brackets", "names", "delivery.yaml",
         plant(plant(delivery, "      label:    { type: string }\n", "      label:    { type: string, optional: true }\n"),
               'format: "{delivery.number}-{n:2}"', 'format: "{label}-{n:2}"')),
        ("a uniqueness in scope on an attribute with no scope", "names", "delivery.yaml",
         plant(delivery, "        unique: true\n", "        unique: in_scope\n")),
        ("a uniqueness that is none of its four forms", "schema", "delivery.yaml",
         plant(delivery, "        unique: true\n", "        unique: global\n")),
        ("a metric that reads a member its rows do not have", "names", "service.yaml",
         plant(service, "        filter: r.outcome != CheckOutcome", "        filter: r.outcom != CheckOutcome")),
        ("a metric that reads a name other than its item", "names", "service.yaml",
         plant(service, "        filter: r.outcome != CheckOutcome.NOT_APPLICABLE\n", "        filter: engineer is not null\n")),
        ("a flag that reads neither the value nor a dimension", "names", "service.yaml",
         plant(service, 'flag_when: { low: "value < 0.900" }', 'flag_when: { low: "value < threshold" }')),
        ("a metric over a source its type does not have", "names", "service.yaml",
         plant(service, "        source: inspections\n", "        source: comments\n")),
        ("a metric whose item is declared after its filter", "order", "service.yaml",
         plant(plant(service, "        item: r\n        filter: r.outcome", "        filter: r.outcome"),
               "        time_dimension: r.occurred_at\n", "        time_dimension: r.occurred_at\n        item: r\n")),
        ("an assertion whose reason is free text", "names", "customers.yaml",
         plant(customers, "          reason: { type: OverrideReason }\n", "          reason: { type: string }\n")),
        ("an assertion that may admit an invariant nobody declares", "names", "customers.yaml",
         plant(customers, "may_admit: [active_invariant]", "may_admit: [active_rule]")),
        ("an erasure that does not reach parts holding personal data", "names", "customers.yaml",
         plant(customers, "                - call: { target: c, transition: forget, inputs: { reason: inputs.reason } }\n",
               "                - call: { target: c, transition: discard }\n")),
        ("a correction that writes what it does not list", "names", "customers.yaml",
         plant(customers, "        corrects: [email]\n        required_inputs: [email]\n", "        corrects: [email]\n        required_inputs: [email, name]\n")),
        ("a personal attribute that is required", "names", "customers.yaml",
         plant(customers, "      name:  { type: string, optional: true, personal: true }\n", "      name:  { type: string, personal: true }\n")),
        ("an erasure that names a target state", "schema", "customers.yaml",
         plant(customers, "        kind: erasure\n        description: Erases the customer's", "        kind: erasure\n        to: CLOSED\n        description: Erases the customer's")),
        ("an indexed marking on a stored reference, which the conversion must carry to step 4", "check 7", "delivery.yaml",
         plant(delivery, "      unit:         { reference: InventoryItem }\n", "      unit:         { reference: InventoryItem, indexed: true }\n")),
        ("a source list that mixes any with a state", "schema", "customers.yaml",
         plant(customers, "        from: any\n        to: CLOSED\n", "        from: [any, ACTIVE]\n        to: CLOSED\n")),
        ("an aggregate's body written in a plain value", "yaml", "service.yaml",
         plant(service, "        expression: count(i in this.intervals(engineer) where i.value is not null) - 1\n",
               "        expression: count(i in this.intervals(engineer) where i.value is not null: i) - 1\n")),
        ("a type that extends one that is not abstract", "names", "servicedesk.yaml",
         plant(servicedesk, "  Laptop:\n    description: A laptop, assigned to the person who uses it.\n    extends: Asset\n", "  Laptop:\n    description: A laptop, assigned to the person who uses it.\n    extends: Alert\n")),
        ("a subtype that redeclares what it inherits", "names", "servicedesk.yaml",
         plant(servicedesk, "      assigned_to: { reference: Employee, optional: true }\n",
               "      assigned_to: { reference: Employee, optional: true }\n      name: { type: string }\n")),
        ("two abstract types that extend each other", "names", "servicedesk.yaml",
         plant(plant(servicedesk, "    abstract: true\n    tracking: serial\n", "    abstract: true\n    extends: Hardware\n    tracking: serial\n"),
               "  Laptop:\n", "  Hardware:\n    description: Hardware of any kind.\n    abstract: true\n    extends: Asset\n\n  Laptop:\n")),
        ("an inherited identifier whose scope a subtype's creation does not write", "names", "servicedesk.yaml",
         plant(servicedesk, 'identifier: { sequence: asset_key, format: "IT-{n}" }', 'identifier: { sequence: asset_key, scope: service, format: "IT-{n}" }')),
        ("a mirror that declares no external identifier", "names", "servicedesk.yaml",
         plant(servicedesk, "      employee_id: { type: string, external: hr, indexed: true }\n", "      employee_id: { type: string, indexed: true }\n")),
        ("an external identifier on a reference", "names", "servicedesk.yaml",
         plant(servicedesk, "      assigned_to: { reference: Employee, optional: true }\n", "      assigned_to: { reference: Employee, optional: true, external: hr }\n")),
        ("a mirror with a transition of its own", "check 53", "servicedesk.yaml",
         plant(servicedesk, "    transitions:\n      forget:\n        kind: erasure\n",
               "    transitions:\n      transfer:\n        kind: internal\n        from: EMPLOYED\n        optional_inputs: [department]\n      forget:\n        kind: erasure\n")),
        ("a state the new version removes, mapped nowhere", "names", "service-v2.yaml",
         plant(plant(plant(plant(plant(service_v2, "      WORKING: { category: live }\n", "      IN_PROGRESS: { category: live }\n"),
                                   "        from: OPEN\n        to: WORKING\n", "        from: OPEN\n        to: IN_PROGRESS\n"),
                             "        from: WORKING\n        to: DONE\n", "        from: IN_PROGRESS\n        to: DONE\n"),
                       "        from: [OPEN, WORKING]\n", "        from: [OPEN, IN_PROGRESS]\n"),
               "        state: WORKING\n", "        state: IN_PROGRESS\n")),
        ("a new required attribute that no backfill fills", "migration", "service-v2.yaml",
         plant(service_v2, "  backfill:\n    ServiceJob: { site: '\"HQ\"' }\n", "")),
        ("a backfill that reads the clock", "names", "service-v2.yaml",
         plant(service_v2, "    ServiceJob: { site: '\"HQ\"' }\n", "    ServiceJob: { site: now }\n")),
        ("a migration that admits no invariant", "names", "service-v2.yaml",
         plant(service_v2, "invariant: ServiceJob.photo_when_done,", "invariant: ServiceJob.photo_required,")),
        ("a required field added to an observation kind", "names", "service-v2.yaml",
         plant(service_v2, "          note:    { type: string, optional: true }\n", "          note:    { type: string, optional: true }\n          probe:   { type: string }\n")),
        ("a metric that reads a label's personal note", "names", "servicedesk.yaml",
         plant(servicedesk, "          name: l.name\n", "          name: l.note\n")),
        ("a guard that reads labels", "names", "servicedesk.yaml",
         plant(servicedesk, "        expression: entered_at(RESOLVED) + 3 days <= now\n", "        expression: entered_at(RESOLVED) + 3 days <= now and count(x in labels) == 0\n")),
        ("a counter that may be absent", "names", "servicedesk.yaml",
         plant(servicedesk, "      on_hand:       { type: counter, indexed: true }\n", "      on_hand:       { type: counter, indexed: true, optional: true }\n")),
        ("a default that reads an attribute", "names", "servicedesk.yaml",
         plant(servicedesk, '      reorder_level: { type: int, default: "5" }\n', "      reorder_level: { type: int, default: on_hand }\n")),
        ("a counter on a type tracked by record", "check 31", "servicedesk.yaml",
         plant(servicedesk, "    tracking: quantity\n", "    tracking: record\n")),
        ("a counter on an observation", "names", "servicedesk.yaml",
         plant(servicedesk, "          score: { type: int }\n", "          score: { type: counter }\n")),
        ("a call to a function the evaluator does not declare", "names", "customers.yaml",
         plant(customers, "        expression: xero.contact_exists(inputs.xero_contact_id)\n", "        expression: xero.contact_known(inputs.xero_contact_id)\n")),
        ("an evaluator's verdict inside a larger expression", "names", "customers.yaml",
         plant(customers, "        expression: xero.contact_exists(inputs.xero_contact_id)\n", "        expression: xero.contact_exists(inputs.xero_contact_id) and email is not null\n")),
        ("an evaluation marked on a condition that calls no evaluator", "names", "customers.yaml",
         plant(customers, "        expression: invoice_number is not null\n        remedy: self_serviceable\n",
               "        expression: invoice_number is not null\n        evaluation: eager\n        remedy: self_serviceable\n")),
        ("an evaluator called by a derived attribute", "names", "customers.yaml",
         plant(customers, "    conditions:\n      contact_in_xero:",
               "    derived_attributes:\n      in_xero:\n        description: Xero knows the contact.\n        expression: xero.contact_exists(xero_contact_id)\n\n    conditions:\n      contact_in_xero:")),
        ("a superseding state entered with no successor", "names", "customers.yaml",
         plant(customers, "        effect:\n          - supersede: inputs.successor\n", "")),
        ("an object that supersedes itself", "names", "customers.yaml",
         plant(customers, "        effect:\n          - supersede: inputs.successor\n", "        effect:\n          - supersede: this\n")),
        ("a successor that is neither an input nor a created object", "names", "customers.yaml",
         plant(customers, "        effect:\n          - supersede: inputs.successor\n", "        effect:\n          - supersede: survivor\n")),
        ("a supersede in a transition that does not end superseded", "names", "customers.yaml",
         plant(customers, "      lapse:\n        kind: external\n        from: ACTIVE\n        to: DORMANT\n",
               "      lapse:\n        kind: external\n        from: ACTIVE\n        to: DORMANT\n        inputs:\n          successor: { reference: Customer }\n        effect:\n          - supersede: inputs.successor\n")),
        ("personal data in supersession with no erasure", "names", "customers.yaml",
         plant(customers, "      forget:\n        kind: erasure\n        description: Erases the customer's",
               "      forget:\n        kind: internal\n        from: [PROSPECT]\n        description: Erases the customer's")),
        ("a proposable transition that only other transitions may take", "names", "delivery.yaml",
         plant(delivery, "        only_via: [Delivery.cancel]\n", "        only_via: [Delivery.cancel]\n        proposable: true\n")),
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
        found, notes = check([("people.yaml", people)] + before + [(f, text)], {"service-v2.yaml": service} if f == "service-v2.yaml" else None)
        hit = [x for x in found + (notes if expect == "migration" else []) if x[3] == expect]
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
    before = next((a.split("=", 1)[1] for a in sys.argv[1:] if a.startswith("--previous=")), None)
    if args:
        files = [(pathlib.Path(a).name, pathlib.Path(a).read_text()) for a in args]
        previous = {files[-1][0]: pathlib.Path(before).read_text()} if before else None
        found, notes = check(files, previous)
        report(found, notes)
        sys.exit(1 if found else 0)
    people = (EXAMPLES / "people.yaml").read_text()
    clean = True
    needs = {"delivery.yaml": ["inventory.yaml"]}
    versions_of = {"inventory-v2.yaml": "inventory.yaml", "service-v2.yaml": "service.yaml"}   # each checked against the one before
    for version in ("people.yaml", "inventory.yaml", "inventory-v2.yaml", "service.yaml", "service-v2.yaml", "delivery.yaml", "approvals.yaml",
                    "customers.yaml", "issues.yaml", "servicedesk.yaml"):
        before = [(n, (EXAMPLES / n).read_text()) for n in needs.get(version, [])]
        files = [("people.yaml", people)] + before + ([(version, (EXAMPLES / version).read_text())] if version != "people.yaml" else [])
        found, notes = check(files, {version: (EXAMPLES / versions_of[version]).read_text()} if version in versions_of else None)
        print(f"{version}: {'clean' if not found else str(len(found)) + ' finding(s)'}, "
              f"{len(notes)} notice{'s' if len(notes) != 1 else ''}")
        for x in found:
            print(f"  {x[0]}:{x[1]} {x[3]} {x[4]}")
        clean &= not found
    ok = self_test(people, (EXAMPLES / "service.yaml").read_text(), (EXAMPLES / "inventory.yaml").read_text(),
                   (EXAMPLES / "delivery.yaml").read_text(), (EXAMPLES / "approvals.yaml").read_text(),
                   (EXAMPLES / "customers.yaml").read_text(), (EXAMPLES / "servicedesk.yaml").read_text(),
                   (EXAMPLES / "service-v2.yaml").read_text())
    sys.exit(0 if clean and ok else 1)


if __name__ == "__main__":
    main()

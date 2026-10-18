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
    measure: transition_count              the transition's events, by name

The terms are UML's, SCXML's and Kubernetes', as ADR-0115 records. Every
finding is reported at the YAML file and line it comes from. Audit and warn
guards are reported as notices, as the publish report lists them.

    check-flows.py                  the example modules, then a self-test
    check-flows.py A.yaml B.yaml    these modules, in import order
"""
import hashlib
import json
import pathlib
import re
from fractions import Fraction
import subprocess
import sys
import tempfile
import time

import jsonschema
import yaml

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import flowexpr  # noqa: E402  the expression language's grammar, shared with the logic checks (ADR-0137)
import flowlogic  # noqa: E402  contradictions within one type (ADR-0137)
from types import SimpleNamespace  # noqa: E402  this module's own namespace, where it has no name (this_checker)

import flowrun  # noqa: E402  the reference runner of examples (ADR-0136)
import flowpublish  # noqa: E402  its publishing, which an example's `publish` step runs (ADR-0166)

ROOT = pathlib.Path(__file__).resolve().parents[1]
FORMAT = ROOT / "docs/design/flow-format"
EXAMPLES = FORMAT / "examples"
CHECKER = ROOT / "scripts/check-syntax-doc.py"

MODULE_ORDER = ["module", "imports", "publishing", "categories", "enumerations", "sequences", "evaluators", "machines", "types", "metrics", "migration"]
COMBINED_ORDER = ["description", "input_metrics", "group_by", "expression", "flag_when"]
MACHINE_ORDER = ["description", "requires", "states", "conditions", "transitions"]
# names the text language gives a meaning in the same position (declaration-syntax.md §9.2, check 33)
NOT_A_CATEGORY = {"any", "terminal", "superseding"}
# every vocabulary has `closed`, whether or not a module declares it (declaration-syntax.md §1)
BUILT_IN_CATEGORIES = {"closed"}
NOT_A_TRANSITION = {"any", "migrate", "import", "redact"}     # the last three name the engine's own events (ADR-0148)
# a bare name in an expression resolves to these before an attribute (declaration-syntax.md §9.2), so no attribute may take one
NOT_AN_ATTRIBUTE = {"state", "inputs", "actor", "this", "now", "referrers", "this_event"}
# what every object has beside its state, read by name as the state is (declaration-syntax.md §8),
# and what a mirror has besides; a member of one of the reserved names would shadow it (the model's check 33)
OBJECT_MEMBERS = {"id", "open", "created_at", "created_by_kind", "recorded_from", "declaration_version"}
# a member withdrawn before 1.0.0, its name kept against shadowing so that it may return with a use (ADR-0210
# decision 3): a mirror's `imported_at`, which no store kept
WITHDRAWN_MEMBERS = {"imported_at"}
SHADOWING = OBJECT_MEMBERS | WITHDRAWN_MEMBERS
# keys the text language has a form for on one kind of attribute only; elsewhere they would be dropped
ONLY_ON_OBSERVATIONS = {"unit"}
ONLY_ON_TYPES = {"actor_kind", "assignee", "unique", "indexed", "identifier", "external", "default", "opposite", "stored", "aggregation", "cascade",
                 "survives"}
METRIC_ORDER = ["description", "measure", "state", "transition", "source", "item", "split_by", "filter", "dimensions",
                "group_by", "time_dimension", "expression", "flag_when"]
TYPE_ORDER = ["description", "abstract", "extends", "mirror", "tracking", "state_machine", "taken_by", "attributes", "inherited_parts", "observations", "states",
              "derived_attributes", "summary", "invariants", "conditions", "transitions", "metrics"]


class StrictLoader(yaml.SafeLoader):
    """YAML 1.1 reads yes/no/on/off as booleans, 2026-03-01 as a date, and keeps
    the last of two equal keys; here only true and false are booleans, a date or
    a timestamp stays the text it is written as, as YAML 1.2's core schema and the
    core's loader read it (ADR-0143), and a repeated key is an error (D494)."""

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
    k: [r for r in v if r[0] not in ("tag:yaml.org,2002:bool", "tag:yaml.org,2002:timestamp")]
    for k, v in yaml.SafeLoader.yaml_implicit_resolvers.items()}
StrictLoader.add_implicit_resolver(
    "tag:yaml.org,2002:bool", re.compile(r"^(?:true|false)$"), list("tf"))


# The widest an identifier's number is padded, an int having at most 19 digits (tenth production review).
IDENTIFIER_WIDTH = 20
# The most derived attributes one typing goes through, as the core's evaluation goes through at most 64.
TYPING_DEPTH = 64

def load(text):
    # StrictLoader is a SafeLoader: it builds only plain data, never Python objects
    return yaml.load(text, Loader=StrictLoader)


# how deep a description's collections may nest, which no module comes near: a deeper one exhausted the composer's
# recursion here and the core's stack, aborting its service (YAML-DEPTH)
NESTING = 64


# YAML's printable set, which PyYAML's reader admits (YAML 1.2 §5.1, c-printable)
NON_PRINTABLE = re.compile("[^\x09\x0A\x0D\x20-\x7E\x85\xA0-\uD7FF\uE000-\uFFFD\U00010000-\U0010FFFF]")
SURROGATE = re.compile("[\uD800-\uDFFF]")
SURROGATE_ESCAPE = re.compile(r"\\u[dD][89a-fA-F][0-9a-fA-F]{2}|\\U0000[dD][89a-fA-F][0-9a-fA-F]{2}")


def shallow(text):
    """A description nesting its collections deeper than NESTING refused, read from the parser's events, which recurse on
    nothing, before anything composes it, as the core's loader refuses it; and a scalar an escape made hold a surrogate,
    which is no character, refused at the escape, as the core's loader refuses it, where it crashed a later step (the
    values audit's C2)."""
    depth = 0
    for event in yaml.parse(text, Loader=StrictLoader):
        if isinstance(event, yaml.ScalarEvent) and SURROGATE.search(event.value):
            start = event.start_mark.index
            escape = SURROGATE_ESCAPE.search(text, start, event.end_mark.index)
            at = escape.start() if escape else start
            line = text.count("\n", 0, at)
            raise yaml.MarkedYAMLError(problem="while parsing a quoted scalar, found invalid Unicode character escape code",
                                       problem_mark=yaml.Mark("<text>", at, line, 0, None, None))
        if isinstance(event, (yaml.SequenceStartEvent, yaml.MappingStartEvent)):
            depth += 1
            if depth > NESTING:
                raise yaml.MarkedYAMLError(problem=f"the description nests deeper than {NESTING} levels, which no flow needs", problem_mark=event.start_mark)
        elif isinstance(event, (yaml.SequenceEndEvent, yaml.MappingEndEvent)):
            depth -= 1


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
    """`from: any` is every state that is not final, and for an external
    transition every one but its target, so no transition leaves a state for
    itself (declaration-syntax.md §4.2, ADR-0122)."""
    if x.get("from") != "any":
        return x
    return dict(x, **{"from": [s for s, v in (states or {}).items() if not (v or {}).get("final") and s != x.get("to")]})


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


TAKEN = "_taken_by"     # on a member a transition's taken_by generates: the transition (ADR-0176)


def taken_by_names(xn):
    """The attribute and the invariant a transition's taken_by generates (ADR-0176 decision 1)."""
    return f"{xn}_by_kind", f"{xn}_taken_by"


def with_taken_by(t):
    """A type with what its transitions' taken_by generate (ADR-0176 decision 1): for each, an optional attribute
    of type actor_kind that its effect first assigns actor.kind, and an invariant holding that attribute absent or
    one of the kinds named, each after the type's own. A name the type declares itself is left as it is declared,
    which taken_by_errors reports."""
    # a transition declaring none takes its type's, a bound machine's among them (ADR-0182 decision 5)
    own = t.get("taken_by") or []
    marked = [(xn, dict(x, taken_by=x.get("taken_by") or own)) for xn, x in (t.get("transitions") or {}).items()
              if isinstance(x, dict) and (x.get("taken_by") or own)]
    if not marked:
        return t
    attributes, invariants = dict(t.get("attributes") or {}), dict(t.get("invariants") or {})
    transitions = dict(t["transitions"])
    for xn, x in marked:
        a, i = taken_by_names(xn)
        kinds = [str(k) for k in x["taken_by"]]
        if a not in attributes:
            attributes[a] = {"type": "actor_kind", "optional": True, TAKEN: xn,
                             "description": f"The kind of actor that took {xn}, recorded as it was taken."}
        if i not in invariants:
            invariants[i] = {"expression": " or ".join([f"{a} is null"] + [f"{a} == {k}" for k in kinds]), TAKEN: xn,
                             "description": f"{xn} is taken by {' or '.join(kinds)} only."}
        transitions[xn] = dict(x, effect=[{"assign": {"location": a, "expr": "actor.kind"}}] + list(x.get("effect") or []))
    return dict(t, attributes=attributes, invariants=invariants, transitions=transitions)


def taken_by_errors(doc):
    """A transition marked taken_by is one a request takes, with an effect: any but a machine's own; an erasure
    and an assertion included, since a request takes each and an agent must be kept from the erasure of a person
    (D579); and what it generates is not a name its type declares (ADR-0176 decision 1). Over the view."""
    out = []
    for tn, t in types(doc):
        for xn, x in (t.get("transitions") or {}).items():
            if not (isinstance(x, dict) and x.get("taken_by")):
                continue
            where = ("types", tn, "transitions", xn, "taken_by")
            if x.get("kind") not in ("initial", "external", "internal", "assertion", "erasure"):
                out.append((where, "names", f"{tn}.{xn} is an {x.get('kind')} transition, which taken_by does not "
                                            f"mark; it marks an initial, external, internal, assertion or erasure "
                                            f"one (ADR-0176, D579)"))
            a, i = taken_by_names(xn)
            for section, n, what in (("attributes", a, "attribute"), ("invariants", i, "invariant")):
                spec = (t.get(section) or {}).get(n)
                if isinstance(spec, dict) and spec.get(TAKEN) != xn:
                    out.append((where, "names", f"{tn}.{xn}'s taken_by generates the {what} {n}, which {tn} "
                                                f"declares itself (ADR-0176)"))
    return out


def column_errors(doc):
    """No attribute of a type is named `<reference>_id` beside a single reference of that name, its own or inherited:
    the store keeps the reference in a column of that name, so it could not store the type (storage-schema.md §3.2,
    D578). Over the view."""
    out = []
    for tn, t in types(doc):
        attrs = t.get("attributes") or {}
        for a in attrs:
            ref = attrs.get(a[:-len("_id")]) if a.endswith("_id") else None
            if isinstance(ref, dict) and isinstance(ref.get("reference"), str) and not ref["reference"].endswith("[]"):
                out.append((("types", tn, "attributes", a), "names",
                            f"{tn}.{a} is the column the store keeps the reference {a[:-len('_id')]} in; name the "
                            f"attribute otherwise (storage-schema.md §3.2, D578)"))
    return out


def view(doc, library=None):
    """The module as the checks see it: each machine as a type, each binder
    merged with its machine, each subtype with its bases; and a map back from
    each view path to the YAML."""
    machines = doc.get("machines") or {}
    kinds = {mn: machine_as_type(m) for mn, m in machines.items()}
    kinds.update({tn: bound(t, machines) for tn, t in (doc.get("types") or {}).items()})
    kinds = {tn: with_taken_by(dict(t, transitions={xn: resolve_any(x, t.get("states"))
                                                    for xn, x in (t.get("transitions") or {}).items()}))
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


def is_set_spec(spec):
    return str((spec or {}).get("type", (spec or {}).get("reference", ""))).endswith("[]")


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
    """The guards required_inputs generates: one per optional or defaulted
    attribute, so a required input stays required where the model's `accepts`
    would let it be left out (ADR-0122)."""
    attrs = t.get("attributes") or {}
    return [a for a in x.get("required_inputs", []) if a in attrs and (optional(attrs[a]) or "default" in attrs[a])]


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


def conditional_step_errors(tn, t):
    """An optional input, one the caller may leave out, is read in a step only
    as the whole of an expression, which is skipped when it is absent: inside
    a larger expression it would make the step conditional, which the model
    forbids (DESIGN.md §5.4; the model's check 48; ADR-0122)."""
    out = []
    for xn, x in (t.get("transitions") or {}).items():
        optional = {i for i, spec in (x.get("inputs") or {}).items() if spec.get("optional") and "default" not in spec}
        optional |= set(x.get("optional_inputs") or [])
        for st, kind, path, _d in walk(x.get("effect")):
            for e in step_expressions(st, kind):
                text = " ".join(str(e).split())
                for n in sorted(optional):
                    if text != f"inputs.{n}" and re.search(rf"\binputs\.{n}\b", text):
                        out.append((("types", tn, "transitions", xn, "effect") + path, "names",
                                    f"{tn}.{xn} reads the optional input '{n}' inside a larger expression of a step, which makes the step "
                                    "conditional; declare one transition per case, each taking the input it needs (DESIGN.md §5.4; the model's check 48)"))
    return out


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
                out.append((tuple(best.absolute_path), "schema", best.message))
        elif e.validator == "oneOf" and e.context:
            # for a step, report the error of the branch its own key names, at its
            # own path: a context error's absolute path already begins with the
            # oneOf's (jsonschema 3.2.0)
            branch = None
            if isinstance(e.instance, dict) and len(e.instance) == 1:
                for n, alt in enumerate(e.validator_value):
                    if alt.get("required") == list(e.instance):
                        branch = n
            within = [c for c in e.context if branch is None or c.schema_path[0] == branch]
            best = jsonschema.exceptions.best_match(within or e.context)
            out.append((tuple(best.absolute_path), "schema", best.message))
        else:
            out.append((tuple(e.absolute_path), "schema", e.message))
    return [(p, c, cut_value(m)) for p, c, m in out]


def cut_value(message):
    """A key that could not be a name, having a space or punctuation in it, is
    the rest of a value an inline mapping cut at a comma, which the author meant
    as one (§2 rule 4): the first such of the keys the message names, each as
    Python writes it, in double quotes where it holds an apostrophe, the
    hiring review's variant (P3-4)."""
    m = re.match(r"Additional properties are not allowed \((.*) (?:was|were) unexpected\)$", message)
    if not m:
        return message
    keys = [a or b for a, b in re.findall(r"'([^']*)'|\"([^\"]*)\"", m.group(1))]
    cut = [k for k in keys if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", k)]
    if not cut:
        return message
    return (f"'{cut[0]}' is not a key but the rest of a value cut at a comma: inside {{ … }}, "
            "a value containing a comma is quoted, as in { description: \"One, two.\" } (§2 rule 4)")


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
    for mn, m in (doc.get("metrics") or {}).items():
        in_order(list(m), COMBINED_ORDER, ("metrics", mn), f"in the metric {mn}, ")
    return out + derived_cycle_errors(doc)


def derived_cycle_errors(doc):
    """A derived attribute reads one of another type, through a relationship or a type scan, wherever that one is
    declared, or one of its own type's through a reference; such reads MUST NOT form a cycle (the model's check 3):
    `depth: parent.depth + 1` on a Node whose parent is a Node reads itself through each node above, and a cycle of
    parents or a chain of thousands overflowed the core's stack on a read (tenth production review). `.<name>` of a
    derived attribute the module declares is taken as a read of it: two types' derived attributes of one name are
    then both read, which can only add a cycle. An imported module cannot read back into this one, so none passes
    through it. The reads are found by each expression's names after a dot, and the search visits each attribute
    once, iteratively, so neither a long chain nor many paths makes it long."""
    order = [(tn, d) for tn, t in types(doc) for d in (t.get("derived_attributes") or {})]
    index = {node: i for i, node in enumerate(order)}
    by_name = {}
    for node in order:
        by_name.setdefault(node[1], []).append(node)
    edges = {}
    for tn, t in types(doc):
        for d, spec in (t.get("derived_attributes") or {}).items():
            text = " ".join(str(spec["expression"]).split())
            named = set(re.findall(r"\.(\w+)", text))
            edges[(tn, d)] = sorted((x for nm in named for x in by_name.get(nm, [])), key=index.get)
    out, reported, state = [], set(), {}
    for start in order:
        if state.get(start):
            continue
        trail, stack = [], [(start, 0)]
        state[start] = 1
        trail.append(start)
        while stack:
            node, k = stack[-1]
            nexts = edges.get(node, [])
            if k == len(nexts):
                stack.pop()
                trail.pop()
                state[node] = 2
                continue
            stack[-1] = (node, k + 1)
            nxt = nexts[k]
            if state.get(nxt) == 1:
                ring = trail[trail.index(nxt):] + [nxt]
                if not set(ring) <= reported:
                    reported.update(ring)
                    out.append((("types", nxt[0], "derived_attributes", nxt[1], "expression"), "order",
                                "derived attributes read each other in a cycle: " + " → ".join(f"{u}.{e}" for u, e in ring)))
            elif not state.get(nxt):
                state[nxt] = 1
                trail.append(nxt)
                stack.append((nxt, 0))
    return out


UNFOLD_LIMIT = 10_000   # the most reads of derived attributes a derived attribute may unfold to, repeats counted
DEPTH_LIMIT = 64        # the most derived attributes, each reading the next, one derived attribute's reads go through
NAMED = re.compile(r"(\.)?\b([A-Za-z_]\w*)\b")


def fan_out_errors(doc, exprs, outer):
    """No expression MAY unfold to more than UNFOLD_LIMIT reads of derived attributes, each counted as often as it is
    made: one reading another twice reads everything that one reads twice, so `d1: d0 + d0` on to `d40` unfolds to
    2^41 reads, and every check that reads a derived attribute through those it reads walked each again, forty holding
    a dry run's slot past six minutes; an invariant reading one a thousand times is inlined a thousand times (eleventh
    production review). A name after a dot reads every derived attribute of that name the module declares, as
    derived_cycle_errors reads it, and a bare name the one its own type declares; a read back into a cycle counts one,
    the cycle refused by its own check. Each derived attribute's count is found once, iteratively, and capped past the
    limit. The first derived attribute past it on each path is refused, and any other expression past it through none
    that is; a module holding one is refused before any later step walks it, as one that does not parse is.

    A name after a dot also reads the derived attributes of that name the modules checked before this one declare,
    `outer` holding the most each unfolds to: a chain split across a module and one importing it passed each module's
    bound and multiplied past it, a dry run's checks holding the slot to its deadline and this checker without end
    (twelfth production review). Returned beside the findings: what each of the module's derived attributes unfolds
    to, by name, which the caller adds to `outer` for the modules after it.

    Nor MAY a derived attribute's reads go more than DEPTH_LIMIT deep, each reading the next: an evaluation goes through
    at most as many, the engine's DERIVED_HOPS, past which a read is not run, where a chain of 9,500 that both checkers
    accepted answered all past the 65th as null (fourteenth production review N1). The depth is found in the same walk,
    a read back into a cycle counting one, the first derived attribute past it on each path refused, and `outer`
    carries each name's deepest beside its count, as the returned list does."""
    order = [(tn, d) for tn, t in types(doc) for d in (t.get("derived_attributes") or {})]
    index = {node: i for i, node in enumerate(order)}
    by_name = {}
    for i, node in enumerate(order):
        by_name.setdefault(node[1], []).append(i)
    typed = dict(types(doc))
    cap = UNFOLD_LIMIT + 1

    def reads_of(own, text):
        """An expression's reads, in its text's order, a repeated read repeated, a bare name read of the type given;
        and what its reads of the modules checked before unfold to, together, and the deepest of them."""
        to, before, deepest = [], 0, 0
        for m in NAMED.finditer(" ".join(str(text).split())):
            if m.group(1):
                to += by_name.get(m.group(2), [])
                n, deep = outer.get(m.group(2), (0, 0))
                before = min(cap, before + n)
                deepest = max(deepest, deep)
            elif (own, m.group(2)) in index:
                to.append(index[(own, m.group(2))])
        return to, before, deepest
    triples = [reads_of(tn, ((typed[tn].get("derived_attributes") or {}).get(d) or {}).get("expression", ""))
               for tn, d in order]
    reads, before, below = [p[0] for p in triples], [p[1] for p in triples], [p[2] for p in triples]
    size, depth, on = [None] * len(order), [1] * len(order), [False] * len(order)
    for start in range(len(order)):
        if size[start] is not None:
            continue
        stack = [[start, 0, min(cap, 1 + before[start]), 1 + below[start]]]
        on[start] = True
        while stack:
            node, k, total, deep = stack[-1]
            if k == len(reads[node]):
                stack.pop()
                on[node] = False
                size[node], depth[node] = total, deep
                if stack:
                    stack[-1][2] = min(cap, stack[-1][2] + total)
                    stack[-1][3] = max(stack[-1][3], 1 + deep)
                continue
            nxt = reads[node][k]
            add = (size[nxt], depth[nxt]) if size[nxt] is not None else ((1, 1) if on[nxt] else None)
            stack[-1][1] = k + 1
            if add is not None:
                stack[-1][2] = min(cap, total + add[0])
                stack[-1][3] = max(deep, 1 + add[1])
            else:
                on[nxt] = True
                stack.append([nxt, 0, min(cap, 1 + before[nxt]), 1 + below[nxt]])
    past = [s is not None and s > UNFOLD_LIMIT for s in size]
    unfolds = (f"unfolds to more than {UNFOLD_LIMIT} reads of derived attributes, each counted as often as it is made: "
               f"one reading another twice reads all that one reads twice")
    deep = [n > DEPTH_LIMIT for n in depth]
    out = []
    for i, (tn, d) in enumerate(order):
        if past[i] and not any(past[r] for r in reads[i]):
            out.append((("types", tn, "derived_attributes", d, "expression"), "order", f"{tn}.{d} {unfolds}"))
        if deep[i] and not any(deep[r] for r in reads[i]):
            out.append((("types", tn, "derived_attributes", d, "expression"), "order",
                        f"{tn}.{d} reads derived attributes more than {DEPTH_LIMIT} deep, each reading the next, past "
                        f"the most one evaluation goes through"))
    # every other expression, its reads unfolded as a derived attribute's
    for p, text in exprs:
        own = p[1] if len(p) > 1 and p[0] == "types" else None
        if own is not None and len(p) > 2 and p[2] == "derived_attributes":
            continue
        read, earlier, _ = reads_of(own, text)
        total = min(cap, 1 + earlier)
        for r in read:
            total = min(cap, total + (size[r] if size[r] is not None else 1))
        if total > UNFOLD_LIMIT and not any(past[r] for r in read):
            out.append((tuple(p), "order", f"{'.'.join(str(s) for s in p)} {unfolds}"))
    return out, [(d, size[i] if size[i] is not None else 1, depth[i]) for i, (_, d) in enumerate(order)]


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
            elif many and tn not in (doc.get("machines") or {}):
                # a machine's required end names no opposite: its binder's end does (§4.10)
                out.append((here, "names", f"{tn}.{an} is a set of references with no opposite, which has nowhere to be stored; "
                                           "an end with no opposite is single (§4.3; the model's check 41)"))
            if spec.get("aggregation") == "composite" and t.get("abstract") and ("cascade" in spec or "survives" in spec):
                # an abstract base has no transitions: each concrete subtype gives the part in inherited_parts (ADR-0133)
                out.append((here, "names", f"{tn}.{an} is declared on the abstract {tn}, which has no transitions to cascade on or survive; "
                                           "each concrete subtype gives its cascade and survives in inherited_parts (§4.14)"))
                continue
            if spec.get("aggregation") == "composite":
                if "opposite" not in spec:
                    out.append((here, "names", f"{tn}.{an} is composite and names no opposite: a part names its whole"))
                    continue
                part = module.get(target, {})
                owner = (part.get("attributes") or {}).get(spec["opposite"], {})
                if owner.get("reference", "").endswith("[]") or owner.get("optional"):
                    out.append((here, "names", f"{target}.{spec['opposite']} MUST be single and required: a part of {tn}.{an} belongs to exactly one whole"))
                out += part_coverage(tn, t, an, target, part, spec.get("cascade") or [], spec.get("survives"), here, finals)
            elif "cascade" in spec or "survives" in spec:
                out.append((here, "names", f"{tn}.{an} has a cascade or survives, which only a composite end has"))
        # a part declared on an abstract base: each concrete subtype says how it goes with each final
        # transition, in `inherited_parts`, and is held to the coverage rule (declaration-syntax.md §3.2, ADR-0133)
        kinds = dict(library or {})
        kinds.update(module)
        inherited = {an: spec for b in reversed(bases(tn, kinds)) for an, spec in (kinds[b].get("attributes") or {}).items()
                     if spec.get("aggregation") == "composite"}
        given = t.get("inherited_parts") or {}
        for an in given:
            if an not in inherited:
                out.append((("types", tn, "inherited_parts", an), "names", f"{tn} gives '{an}' in inherited_parts, which is not a part it inherits"))
        if t.get("abstract"):
            if given:
                out.append((("types", tn, "inherited_parts"), "names", f"{tn} is abstract and has no transitions to cascade on; its concrete subtypes give its parts"))
            continue
        for an, spec in inherited.items():
            target = spec.get("reference", "").rstrip("[]")
            ip = given.get(an) or {}
            out += part_coverage(tn, t, an, target, kinds.get(target, {}), ip.get("cascade") or [], ip.get("survives"),
                                 ("types", tn, "inherited_parts", an), finals)
    return out


def swap_members(t):
    """The members of a type the swap test exchanges: its attributes, derived attributes and observations, and what
    every object has, `state` and `referrers` among them."""
    return (set(t.get("attributes") or {}) | set(t.get("derived_attributes") or {}) | set(t.get("observations") or {})
            | OBJECT_MEMBERS | {"state", "referrers"})


def inline_derived(e, t, seen=frozenset()):
    """An expression with each derived attribute of its own object it reads, by its name or through `this`, replaced
    by that attribute's expression, however deep."""
    if not isinstance(e, tuple) or not e or not isinstance(e[0], str):
        return e
    derived = t.get("derived_attributes") or {}
    name = e[1] if e[0] == "name" else e[2] if e[0] == "path" and e[1] == ("name", "this") else None
    if isinstance(name, str) and name in derived and name not in seen:
        return inline_derived(flowexpr.parse(str((derived[name] or {}).get("expression", ""))), t, seen | {name})
    if e[0] == "agg":
        _, fn, distinct, var, coll, where, body = e
        inner = seen | ({var} if var else set())      # a bound name is not a member
        return ("agg", fn, distinct, var, inline_derived(coll, t, seen), inline_derived(where, t, inner),
                inline_derived(body, t, inner))
    if e[0] == "metric":
        return ("metric", inline_derived(e[1], t, seen), [(d, inline_derived(x, t, seen)) for d, x in e[2]],
                inline_derived(e[3], t, seen), inline_derived(e[4], t, seen))
    return (e[0],) + tuple([inline_derived(q, t, seen) for q in p] if isinstance(p, list) else inline_derived(p, t, seen)
                           for p in e[1:])


def swapped(e, var, members, bound=frozenset()):
    """An expression with the object it is evaluated on and the scan's bound object exchanged: `this` and the
    bound name, and each member read on one, read on the other (declaration-syntax.md §3.4)."""
    if not isinstance(e, tuple) or not e or not isinstance(e[0], str):
        return e
    if e[0] == "name":
        n = e[1]
        if n == "this":
            return ("name", var)
        if n == var and var in bound:
            return ("name", "this")
        if n in members and n not in bound:
            return ("path", ("name", var), n)
        return e
    if e[0] == "agg":
        _, fn, distinct, var2, coll, where, body = e
        inner = bound | ({var2} if var2 else set())
        return ("agg", fn, distinct, var2, swapped(coll, var, members, bound), swapped(where, var, members, inner),
                swapped(body, var, members, inner))
    if e[0] == "metric":
        return ("metric", swapped(e[1], var, members, bound), [(d, swapped(x, var, members, bound)) for d, x in e[2]],
                swapped(e[3], var, members, bound), swapped(e[4], var, members, bound))
    return (e[0],) + tuple([swapped(q, var, members, bound) for q in p] if isinstance(p, list) else swapped(p, var, members, bound)
                           for p in e[1:])


def normalised(e):
    """An expression under the swap test's four normalisations and no other (declaration-syntax.md §3.4, ADR-0063):
    the conjuncts of an `and` as a set, `==` and `!=` whatever their operands' order, `<`, `>`, `<=` and `>=` whatever
    their direction, and a member of `this` whatever its spelling."""
    if not isinstance(e, tuple) or not e or not isinstance(e[0], str):
        return e
    if e[0] == "path" and e[1] == ("name", "this"):
        return ("name", e[2])
    if e[0] == "bin" and e[1] == "and":
        conjuncts, todo = [], [e]
        while todo:
            x = todo.pop()
            if isinstance(x, tuple) and x[:2] == ("bin", "and"):
                todo += [x[2], x[3]]
            else:
                conjuncts.append(normalised(x))
        return ("and", frozenset(conjuncts))
    if e[0] == "bin" and e[1] in ("==", "!="):
        return (e[1], frozenset([normalised(e[2]), normalised(e[3])]))
    if e[0] == "bin" and e[1] in (">", ">="):
        return ("bin", {">": "<", ">=": "<="}[e[1]], normalised(e[3]), normalised(e[2]))
    if e[0] == "metric":
        return ("metric", normalised(e[1]), tuple((d, normalised(x)) for d, x in e[2]), normalised(e[3]), normalised(e[4]))
    return (e[0],) + tuple(tuple(normalised(q) for q in p) if isinstance(p, list) else normalised(p) for p in e[1:])


def sides(e, var, members, bound=frozenset()):
    """The members an expression reads on the scan's bound object and on its own, as two sets."""
    on_var, on_this = set(), set()

    def walk(x, bound):
        if not isinstance(x, tuple) or not x or not isinstance(x[0], str):
            return
        if x[0] == "path" and x[1] == ("name", var) and var in bound:
            on_var.add(x[2])
            return
        if x[0] == "path" and x[1] == ("name", "this"):
            on_this.add(x[2])
            return
        if x[0] == "name":
            if x[1] in members and x[1] not in bound:
                on_this.add(x[1])
            return
        if x[0] == "agg":
            _, _fn, _d, var2, coll, where, body = x
            walk(coll, bound)
            for q in (where, body):
                walk(q, bound | ({var2} if var2 else set()))
            return
        if x[0] == "metric":
            for q in [x[1]] + [y for _d, y in x[2]] + [x[3], x[4]]:
                walk(q, bound)
            return
        for p in x[1:]:
            for q in (p if isinstance(p, list) else [p]):
                walk(q, bound)
    walk(e, bound)
    return on_var, on_this


def scans(e, lib, bound=frozenset()):
    """Each type scan in an expression, as (its bound name, the type, its `where`), outermost first."""
    out = []
    if not isinstance(e, tuple) or not e or not isinstance(e[0], str):
        return out
    if e[0] == "agg":
        _, _fn, _d, var, coll, where, body = e
        if var and coll and coll[0] == "name" and coll[1] in lib and coll[1] not in bound:
            out.append((var, coll[1], where))
        out += scans(coll, lib, bound)
        for q in (where, body):
            out += scans(q, lib, bound | ({var} if var else set()))
        return out
    if e[0] == "metric":
        for q in [e[1]] + [y for _d, y in e[2]] + [e[3], e[4]]:
            out += scans(q, lib, bound)
        return out
    for p in e[1:]:
        for q in (p if isinstance(p, list) else [p]):
            out += scans(q, lib, bound)
    return out


def symmetry_errors(doc, library):
    """A type-scan invariant MUST be symmetric, its verdict the same whichever of the objects it relates is written,
    decided by the swap test of declaration-syntax.md §3.4 over the whole invariant, its own derived attributes read as
    their expressions: exchange `this` and each scan's bound object, and each member read on one with the same read on
    the other, and require the same predicate back under the four normalisations and no other (ADR-0063). A member of
    the object read outside the scan has no counterpart, so it is a member read on one side only, which the finding
    names. And a scan MUST NOT exclude the object itself, by its id or by `this`, which it never ranges over (ADR-0061)
    (the model's check 6)."""
    kinds = dict(library or {})
    kinds.update(dict(types(doc)))
    out = []
    for tn, t in types(doc):
        members = swap_members(t)
        for i, spec in (t.get("invariants") or {}).items():
            where_ = ("types", tn, "invariants", i, "expression")
            try:
                tree = inline_derived(flowexpr.parse(str((spec or {}).get("expression", ""))), t)
            except flowexpr.ParseError:
                continue                            # step 2 refuses what does not parse
            for var, scanned, condition in scans(tree, kinds):
                conjuncts, todo = [], [condition] if condition is not None else []
                while todo:
                    x = todo.pop()
                    if isinstance(x, tuple) and x[:2] == ("bin", "and"):
                        todo += [x[2], x[3]]
                    else:
                        conjuncts.append(normalised(x))
                selves = [frozenset([("path", ("name", var), "id"), ("name", "id")]), frozenset([("name", var), ("name", "this")])]
                if any(c[0] == "!=" and c[1] in selves for c in conjuncts if isinstance(c, tuple) and len(c) == 2):
                    out.append((where_, "names", f"{tn}'s invariant '{i}' excludes the object itself from its scan of "
                                                 f"{scanned}, which never ranges over it (the model's check 6)"))
                    continue
                if normalised(swapped(tree, var, members)) == normalised(tree):
                    continue
                on_var, on_this = sides(tree, var, members)
                one_sided = sorted(on_var ^ on_this)
                why = (f"it reads {', '.join(one_sided)} of one of the two objects only, so its verdict depends on which "
                       "is written" if one_sided else
                       "with the two objects exchanged its predicate is not the one written, under the four "
                       "normalisations of the swap test")
                out.append((where_, "names", f"{tn}'s invariant '{i}' scans {scanned} and is not symmetric: {why} "
                                             "(the model's check 6)"))
    return out


def transition_writes(t, tn, x):
    """What a transition of a type writes, as (type, member): what its effect assigns, adds, removes or clears and the
    attribute inputs it takes, each stored end's far end, whose set it changes, and its target type's `referrers`;
    its type's `state` where it moves or sets it; a new object, `*new`, where it creates one; and every personal
    attribute where it is an erasure, which writes absence."""
    attrs = t.get("attributes") or {}
    out = set()

    def wrote(m):
        out.add((tn, m))
        spec = attrs.get(m) or {}
        target = str(spec.get("reference", "")).rstrip("[]")
        if target:
            out.add((target, "referrers"))
            if "opposite" in spec:
                out.add((target, spec["opposite"]))
    for st, kind, _path, _depth in walk(x.get("effect")):
        if kind in ("assign", "add", "remove"):
            wrote(st[kind]["location"])
        elif kind == "clear":
            for a in st["clear"]:
                wrote(a)
    for m in list(x.get("required_inputs") or []) + list(x.get("optional_inputs") or []):
        if m in attrs:
            wrote(m)
    if x.get("kind") in ("initial", "external", "assertion"):
        out.add((tn, "state"))
    if x.get("kind") == "initial":
        out.add((tn, "*new"))
    if x.get("kind") == "erasure":
        for m, spec in attrs.items():
            if (spec or {}).get("personal"):
                wrote(m)
    return out


def invariant_reads(kinds, tn, text, seen=frozenset()):
    """What an invariant reads, as (type, member), as far as step 3's walker types it: each member through any path
    whose type is known, the state through `open` and a category, every object of a type it scans, `*new`, and what a
    derived attribute it reads reads, however deep."""
    try:
        tree = flowexpr.parse(str(text))
    except flowexpr.ParseError:
        return set()                            # step 2 refuses what does not parse
    walker = OptionalReads(kinds, tn)
    walker.through_tests = True
    walker.walk(tree, {})
    out = set(walker.members) | {(u, "*new") for u in walker.scanned}
    out |= {(u, "state") for u, m in walker.members if m == "open"}
    for u, d in sorted(walker.derived - seen):
        spec = (kinds[u].get("derived_attributes") or {})[d] or {}
        out |= invariant_reads(kinds, u, spec.get("expression", ""), seen | {(u, d)})
    return out


def could_violate(kinds, tn, text, machines=frozenset()):
    """The transitions, `<Type>.<transition>`, that could violate an invariant of a type, for a publish report to name
    (DESIGN.md §5.6): each whose writes meet what the invariant reads, of any type checked, a machine's named by the
    type binding it, and the type's own creations, which every invariant is checked on landing. An over-approximation,
    never an omission, as far as the reads can be typed."""
    reads = invariant_reads(kinds, tn, text) | {(tn, "*new")}
    return sorted(f"{u}.{xn}" for u, t in kinds.items() if u not in machines
                  for xn, x in (t.get("transitions") or {}).items() if transition_writes(t, u, x) & reads)


def invariant_form(kinds, tn, text, seen=frozenset()):
    """An invariant's form, which a publish report states (declaration-syntax.md §3.4, ADR-0055, ADR-0171):
    `type_scan` where it ranges over a type by name, else `traversal` where it reads through a reference or over
    `referrers`, reading other objects, else `local`; a derived attribute it reads counts as its own expression's form,
    however deep. No invariant reads an observation, which step 3 refuses."""
    try:
        tree = flowexpr.parse(str(text))
    except flowexpr.ParseError:
        return "local"                          # step 2 refuses what does not parse
    walker = OptionalReads(kinds, tn)
    walker.through_tests = True
    walker.walk(tree, {})
    if walker.scanned:
        return "type_scan"
    form = "traversal" if walker.traversed or any(m == "referrers" for _u, m in walker.members) else "local"
    for u, d in sorted(walker.derived - seen):
        spec = (kinds[u].get("derived_attributes") or {})[d] or {}
        inner = invariant_form(kinds, u, spec.get("expression", ""), seen | {(u, d)})
        if inner == "type_scan":
            return inner
        if inner == "traversal":
            form = inner
    return form


def reparent_errors(doc, library):
    """A re-parent, a `do` or an `act` writing a part's end back to its whole on an existing object, MUST NOT write an
    end whose type is abstract, since which source whole's invariants are re-checked would then not be known from the
    text (the model's check 36, ADR-0058). It writes the end by an `assign` step or by naming it among its inputs, which
    writes it; a creation writing it is not a re-parent. A part whose end back names another type than the whole is
    the model's check 36 too, refused as ends that do not name each other (relationship_errors)."""
    kinds = dict(library or {})
    kinds.update(dict(types(doc)))
    out = []
    for tn, t in types(doc):
        owners = {}                             # each end back to a whole of an abstract type, by the type
        for a, spec in (t.get("attributes") or {}).items():
            w = str(spec.get("reference", ""))
            whole = ((kinds.get(w) or {}).get("attributes") or {}).get(spec.get("opposite")) or {}
            if whole.get("aggregation") == "composite" and (kinds.get(w) or {}).get("abstract"):
                owners[a] = w
        for xn, x in (t.get("transitions") or {}).items():
            if x.get("kind") not in ("external", "internal"):
                continue
            base = ("types", tn, "transitions", xn)
            written = [(base + (key,), a) for key in ("required_inputs", "optional_inputs") for a in x.get(key) or []]
            written += [(base + ("effect",) + path, st[kind]["location"])
                        for st, kind, path, _depth in walk(x.get("effect")) if kind == "assign"]
            out += [(where, "names", f"{tn}.{xn} re-parents a part, writing '{a}', whose type {owners[a]} is abstract, so "
                                     "which whole's invariants it re-checks is not known from the text (the model's "
                                     "check 36, ADR-0058)")
                    for where, a in written if a in owners]
    return out


def personal_inputs(kinds, tn, x):
    """The inputs of a transition that are personal values (the model's check 10, ADR-0078): marked `personal`, an
    attribute input of a personal attribute, or read by a write of its effect into a personal attribute, as the
    reference runner's `input_personal` has it."""
    attrs = (kinds.get(tn) or {}).get("attributes") or {}
    out = {n for n, spec in (x.get("inputs") or {}).items() if (spec or {}).get("personal")}
    out |= {n for n in list(x.get("required_inputs") or []) + list(x.get("optional_inputs") or [])
            if (attrs.get(n) or {}).get("personal")}
    for st, kind, _path, _depth in walk(x.get("effect")):
        if kind in ("assign", "add") and (attrs.get(st[kind]["location"]) or {}).get("personal"):
            out |= set(INPUT_READ.findall(str(st[kind]["expr"]))) & taken(x)
    return out


def personal_errors(doc, library):
    """A personal value MUST NOT reach a non-personal attribute (the model's check 10, ADR-0078): by a write of the
    effect, by an argument to a `call` or a `create`, or a cascade clause's, that the transition it reaches writes to
    one, however many calls on, or by a migration's backfill. An expression carries a personal value where it reads,
    anywhere in it, a personal input, a personal attribute or observation field, of its own object or another reached
    through a path whose type is known, or a derived attribute that does; what reaches only a personal attribute, or an
    input the transition reached holds personal itself, is not a leak here, the latter checked where it is written."""
    kinds = dict(library or {})
    kinds.update(dict(types(doc)))
    fields = {o["kind"]: o for t in kinds.values() for o in (t.get("observations") or {}).values() if "kind" in o}

    def personal(u, m):
        spec = ((kinds.get(u) or fields.get(u) or {}).get("attributes") or {}).get(m)
        return bool((spec or {}).get("personal"))

    def why(tn, x, text, env, inputs, seen=frozenset()):
        """What personal value an expression carries, or None."""
        text = str(text)
        for n in sorted(set(INPUT_READ.findall(text)) & inputs):
            return f"the personal input '{n}'"
        own = kinds.get(tn) or {}
        for n in sorted(reads(text)):
            if personal(tn, n):
                return f"the personal {tn}.{n}"
        try:
            tree = flowexpr.parse(text)
        except flowexpr.ParseError:
            return None                         # step 2 refuses what does not parse
        walker = OptionalReads(kinds, tn, x)
        walker.through_tests = True
        walker.walk(tree, dict(env))
        for u, m in sorted(walker.members):
            if personal(u, m):
                return f"the personal {u}.{m}"
        derived = walker.derived | {(tn, n) for n in reads(text) if n in (own.get("derived_attributes") or {})}
        for u, d in sorted(derived):
            spec = (kinds[u].get("derived_attributes") or {})[d] or {}
            if (u, d) not in seen and why(u, None, spec.get("expression", ""), {}, set(), seen | {(u, d)}):
                return f"the personal value in the derived {u}.{d}"
        return None

    def parts(tn):
        """Each composite end of a type, its own or inherited, with its cascade clauses and where they are given."""
        t = kinds.get(tn) or {}
        out = [(spec, spec.get("cascade") or [], ("types", tn, "attributes", an))
               for an, spec in (t.get("attributes") or {}).items() if spec.get("aggregation") == "composite"]
        inherited = {an: spec for b in reversed(bases(tn, kinds)) for an, spec in (kinds[b].get("attributes") or {}).items()
                     if spec.get("aggregation") == "composite"}
        return out + [(inherited[an], (ip or {}).get("cascade") or [], ("types", tn, "inherited_parts", an))
                      for an, ip in (t.get("inherited_parts") or {}).items() if an in inherited]

    def passes(tn, x):
        """Each argument a transition passes, as (expression, its loops' items, target type, transition, input, where)."""
        t = kinds.get(tn) or {}
        out = []
        for st, kind, path, _depth in walk(x.get("effect")):
            if kind in ("call", "create"):
                body = st[kind]
                env = binders(kinds, tn, x, path)
                k = call_target(kinds, t, tn, x, body, env) if kind == "call" else body.get("type")
                for j, e in (body.get("inputs") or {}).items():
                    out.append((e, env, k, body.get("transition"), j, ("effect",) + path))
        return out

    def cascades(tn, xn):
        """Each argument a cascade clause on a transition passes, as passes gives it, where the clause is."""
        return [(e, {}, str(spec.get("reference", "")).rstrip("[]"), c.get("transition"), j, here + ("cascade", i))
                for spec, clauses, here in parts(tn) for i, c in enumerate(clauses) if xn in (c.get("on") or [])
                for j, e in (c.get("inputs") or {}).items()]

    def leak(k, ct, j, seen=frozenset()):
        """Where input j of k.ct reaches a non-personal attribute, or None."""
        t = kinds.get(k) or {}
        x = (t.get("transitions") or {}).get(ct)
        if x is None or (k, ct, j) in seen or j in personal_inputs(kinds, k, x):
            return None
        attrs = t.get("attributes") or {}
        if j in list(x.get("required_inputs") or []) + list(x.get("optional_inputs") or []) and j in attrs:
            return f"writes it to {k}.{j}, which is not personal"
        for st, kind, _path, _depth in walk(x.get("effect")):
            if kind in ("assign", "add") and j in INPUT_READ.findall(str(st[kind]["expr"])) \
                    and st[kind]["location"] in attrs and not personal(k, st[kind]["location"]):
                return f"writes it to {k}.{st[kind]['location']}, which is not personal"
        for e, _env, k2, ct2, j2, _where in passes(k, x) + cascades(k, ct):
            if k2 and j in INPUT_READ.findall(str(e)):
                inner = leak(k2, ct2, j2, seen | {(k, ct, j)})
                if inner:
                    return f"passes it to {k2}.{ct2}, which {inner}"
        return None

    out = []
    for tn, t in types(doc):
        attrs = t.get("attributes") or {}
        for xn, x in (t.get("transitions") or {}).items():
            base = ("types", tn, "transitions", xn)
            inputs = personal_inputs(kinds, tn, x)
            for st, kind, path, _depth in walk(x.get("effect")):
                loc = st[kind].get("location") if kind in ("assign", "add") else None
                if loc in attrs and not personal(tn, loc):
                    carried = why(tn, x, st[kind]["expr"], binders(kinds, tn, x, path), inputs)
                    if carried:
                        out.append((base + ("effect",) + path, "names",
                                    f"{tn}.{xn} writes {carried} to '{loc}', which is not personal; a personal value "
                                    "reaches only a personal attribute (the model's check 10)"))
            for e, env, k, ct, j, where in passes(tn, x):
                carried = why(tn, x, e, env, inputs) if k else None
                reached = leak(k, ct, j) if carried else None
                if reached:
                    out.append((base + where, "names",
                                f"{tn}.{xn} passes {carried} to {k}.{ct}'s input '{j}', which {reached}; a personal "
                                "value reaches only a personal attribute (the model's check 10)"))
            for e, _env, k, ct, j, where in cascades(tn, xn):
                carried = why(tn, x, e, {}, inputs)
                reached = leak(k, ct, j) if carried else None
                if reached:
                    out.append((where, "names",
                                f"{tn}.{xn} cascades {carried} to {k}.{ct}'s input '{j}', which {reached}; a "
                                "personal value reaches only a personal attribute (the model's check 10)"))
    for tn, given in ((doc.get("migration") or {}).get("backfill") or {}).items():
        for a, e in (given or {}).items():
            if not personal(tn, a):
                carried = why(tn, None, e, {}, set())
                if carried:
                    out.append((("migration", "backfill", tn, a), "names",
                                f"the backfill of {tn}.{a} reads {carried}, and {a} is not personal; a backfill is a "
                                "write; a personal value reaches only a personal attribute (the model's check 10)"))

    # a metric over objects reads no personal value through a reference or a derived attribute either, where check 56,
    # reading its item's own members, passed `d.owner.name`, `max(d.owner.salary)` and `d.owner_name`, and an agent's
    # metric read answered each person's name and salary (sixteenth production review N6, ADR-0203 decision 2)
    def chains(tree, item):
        """Each path read from the item, as its members, every path of the tree, outermost first."""
        found = []

        def visit(n):
            if isinstance(n, tuple) and n[:1] == ("path",):
                members, base = [], n
                while isinstance(base, tuple) and base[:1] == ("path",):
                    members.insert(0, base[2])
                    base = base[1]
                if base == ("name", item):
                    found.append(members)
            if isinstance(n, (tuple, list)):
                for c in n:
                    visit(c)
        visit(tree)
        return found

    # a copy of an identity is the identity, personal where an actor type marks its own so (ADR-0203 decision 1), where
    # a metric grouped by an approver set from actor.id answered each person's address (the audit's personal data R8)
    identities_personal = any(isinstance(spec, dict) and spec.get("actor_kind") and spec.get("personal")
                              for k in kinds.values() for spec in ((k or {}).get("attributes") or {}).values())

    def reaches(tn, members):
        """What personal value a path from an object of the type reaches past its first member, or None; a member of
        type identity at any hop, where identities are personal."""
        u = tn
        for i, mbr in enumerate(members):
            k = kinds.get(u) or {}
            spec = (k.get("attributes") or {}).get(mbr)
            if isinstance(spec, dict):
                if identities_personal and str(spec.get("type", "")).removesuffix("[]") == "identity":
                    return f"the identity {u}.{mbr}, personal as an actor's is"
                if spec.get("personal"):
                    return f"the personal {u}.{mbr}" if i > 0 else None
                if spec.get("reference"):
                    u = str(spec["reference"]).rstrip("[]")
                    continue
                return None
            d = (k.get("derived_attributes") or {}).get(mbr)
            if d is not None:
                carried = why(u, None, (d or {}).get("expression", ""), {}, set())
                return f"the derived {u}.{mbr}, which reads {carried}" if carried else None
            return None
        return None

    for tn, t in types(doc):
        for mn, m in (t.get("metrics") or {}).items():
            if not isinstance(m, dict) or m.get("source", "objects") != "objects" or not m.get("item"):
                continue
            parts = [(("expression",), m.get("expression")), (("filter",), m.get("filter"))]
            parts += [(("dimensions", d), e) for d, e in (m.get("dimensions") or {}).items()]
            for key, text in parts:
                if text is None:
                    continue
                try:
                    tree = flowexpr.parse(" ".join(str(text).split()))
                except flowexpr.ParseError:
                    continue                    # step 2 refuses what does not parse
                said = set()
                for members in chains(tree, m["item"]):
                    reached = reaches(tn, members)
                    if reached:
                        said.add(f"metric {mn} reads {m['item']}.{'.'.join(members)}, which reaches {reached}; a "
                                 "metric reads no personal value, through a reference or a derived attribute either "
                                 "(ADR-0203)")
                out += [(("types", tn, "metrics", mn) + key, "names", w) for w in sorted(said)]
    return out


def clock_read(e, spans=frozenset()):
    """What an expression reads of the clock itself, or None: `now`; `time_in`, which counts a current span up to now;
    or a span's `duration`, which does the same, in an aggregate over this object's intervals (declaration-syntax.md
    §6.9, §8.3). `entered_at` reads when a value was entered, not the clock."""
    if not isinstance(e, tuple) or not e or not isinstance(e[0], str):
        return None
    if e[0] == "name":
        return "now" if e[1] == "now" else None
    if e[0] == "call" and e[1] == ("name", "time_in"):
        return "time_in"
    if e[0] == "path" and e[2] == "duration" and e[1][0] == "name" and e[1][1] in spans:
        return "a span's duration"
    if e[0] == "agg" and e[3] is not None:
        _, _fn, _distinct, var, coll, where, body = e
        head = coll[1] if coll[0] == "call" else coll
        inner = spans | {var} if head == ("path", ("name", "this"), "intervals") else spans - {var}
        return clock_read(coll, spans) or clock_read(where, inner) or clock_read(body, inner)
    if e[0] == "metric":                    # its binds are (dimension, expression) pairs, not nodes
        return next((r for r in (clock_read(q, spans) for q in [e[1]] + [x for _d, x in e[2]] + [e[3], e[4]]) if r), None)
    for part in e[1:]:
        for q in (part if isinstance(part, list) else [part]):
            found = clock_read(q, spans)
            if found:
                return found
    return None


def clock_errors(doc, library):
    """An invariant MUST NOT read the clock, since the clock moves without a write and no check after one can enforce
    it: `now`, `time_in` or a span's duration, or a derived attribute that does, of its own type or of another
    reached through a path, however many derivations deep (the model's check 32, declaration-syntax.md §8.3). An
    observation kind's invariants are check 54's, which refuses `now` among what a kind may not read."""
    kinds = dict(library or {})
    kinds.update(dict(types(doc)))

    def why(tn, text, seen):
        try:
            tree = flowexpr.parse(text)
        except flowexpr.ParseError:
            return None                         # step 2 refuses what does not parse
        direct = clock_read(tree)
        if direct:
            return {"now": "reads now, the clock", "time_in": "reads time_in, which counts to now",
                    "a span's duration": "reads a span's duration, which a current span counts to now"}[direct]
        walker = OptionalReads(kinds, tn)
        walker.walk(tree, {})
        for u, d in sorted(walker.derived):
            if (u, d) not in seen and why(u, (kinds[u]["derived_attributes"][d] or {}).get("expression", ""), seen | {(u, d)}):
                return f"reads {u}.{d}, a derived attribute that reads the clock"
        return None

    out = []
    for tn, t in types(doc):
        for i, spec in (t.get("invariants") or {}).items():
            reason = why(tn, str((spec or {}).get("expression", "")), frozenset())
            if reason:
                out.append((("types", tn, "invariants", i, "expression"), "names",
                            f"{tn}'s invariant '{i}' {reason}; the clock moves without a write, so no check after one "
                            "can enforce it, and a rule about time is a guard (the model's check 32)"))
    return out


def cascade_cycle_errors(doc, library):
    """A transition causes others: by a `call` or a `create` among its effect's steps, a loop's included, and by a
    cascade clause on it of a part its type gives, its own or one it inherits. Transitions MUST NOT cause each other
    in a cycle, nor one cause itself, on its own object or another of its type (the model's check 12, ADR-0019). A
    transition no type of the module declares is left to the checks that name it, and an imported module cannot
    cause one of this module's, so no cycle passes through it. A cycle is reported at the step or the clause that
    closes it, once for each set of transitions a cycle passes through."""
    module = dict(types(doc))
    kinds = dict(library or {})
    kinds.update(module)

    def declares(k, xn):
        return k in module and xn in (module[k].get("transitions") or {})

    def causes(tn, xn):
        t = module[tn]
        x = t["transitions"][xn]
        out = []
        for st, kind, path, _depth in walk(x.get("effect")):
            where = ("types", tn, "transitions", xn, "effect") + path
            if kind == "call":
                k = call_target(kinds, t, tn, x, st["call"], binders(kinds, tn, x, path))
                if declares(k, st["call"]["transition"]):
                    out.append(((k, st["call"]["transition"]), where))
            elif kind == "create" and declares(st["create"]["type"], st["create"]["transition"]):
                out.append(((st["create"]["type"], st["create"]["transition"]), where))
        parts = [(an, spec, spec.get("cascade") or [], ("types", tn, "attributes", an))
                 for an, spec in (t.get("attributes") or {}).items() if spec.get("aggregation") == "composite"]
        inherited = {an: spec for b in reversed(bases(tn, kinds)) for an, spec in (kinds[b].get("attributes") or {}).items()
                     if spec.get("aggregation") == "composite"}
        parts += [(an, inherited[an], (ip or {}).get("cascade") or [], ("types", tn, "inherited_parts", an))
                  for an, ip in (t.get("inherited_parts") or {}).items() if an in inherited]
        for _an, spec, clauses, here in parts:
            part = str(spec.get("reference", "")).rstrip("[]")
            for i, c in enumerate(clauses):
                if xn in c["on"] and declares(part, c["transition"]):
                    out.append(((part, c["transition"]), here + ("cascade", i)))
        return out

    out, state, reported = [], {}, set()

    def visit(node, trail):
        state[node] = "open"
        trail.append(node)
        for nxt, where in causes(*node):
            if state.get(nxt) == "open":
                ring = trail[trail.index(nxt):] + [nxt]
                if frozenset(ring) not in reported:
                    reported.add(frozenset(ring))
                    out.append((where, "names", "a transition causes itself, by a call, a create or a cascade: "
                                + " → ".join(f"{u}.{e}" for u, e in ring)
                                + "; no transition may cause itself, on its own object or another (the model's check 12)"))
            elif nxt not in state:
                visit(nxt, trail)
        trail.pop()
        state[node] = "closed"

    for tn, t in types(doc):
        for xn in (t.get("transitions") or {}):
            if (tn, xn) not in state:
                visit((tn, xn), [])
    return out


def part_coverage(tn, t, an, target, part, clauses, survives, here, finals):
    """A composite end's cascade clauses name transitions of the whole and of the part, with the part's
    inputs, and every final transition of the whole is covered by a clause or survived, never both (check 37)."""
    out, covered = [], set()
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
        if called["kind"] == "assertion":
            out.append((where, "names", f"{tn}.{an} cascades to {target}.{c['transition']}, an assertion, which only a request "
                                        f"takes; an override is requested, never caused (ADR-0163)"))
            continue
        given = set(c.get("inputs") or {})
        out += [(where, "names", f"{tn}.{an} passes '{i}' to {target}.{c['transition']}, which takes no input '{i}'") for i in sorted(given - taken(called))]
        out += [(where, "names", f"{tn}.{an} cascades to {target}.{c['transition']} without the input '{i}', which it requires")
                for i in sorted(required_input_names(called) - given) if "default" not in ((called.get("inputs") or {}).get(i) or {})]
    kept = set(t.get("transitions") or {}) if survives is True else set(survives or [])
    for w in sorted(covered & kept):
        out.append((here, "names", f"{tn}.{an} both cascades on and survives '{w}'"))
    for xn, x in (t.get("transitions") or {}).items():
        # every transition into a final state, an assertion whose `to` lists one included (ADR-0146)
        into = ([x.get("to")] if x["kind"] == "external" else
                [s for s in (x.get("to") if isinstance(x.get("to"), list) else [x.get("to")]) if s] if x["kind"] == "assertion" else [])
        final = [s for s in into if s in finals]
        if final and xn not in covered | kept:
            out.append((here, "names", f"{tn}.{xn} enters the final state {final[0]}, and {tn}.{an} neither cascades on it nor survives it"))
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
        members_all = set(t.get("attributes") or {}) | set(t.get("derived_attributes") or {}) | {"state"}
        for c in chain:
            members_all |= set(kinds[c].get("attributes") or {}) | set(kinds[c].get("derived_attributes") or {})
        for m in t.get("summary") or []:
            if m not in members_all:
                out.append((here + ("summary",), "names", f"{tn}'s summary names '{m}', which is neither state nor a member of {tn}"))
        if not t.get("abstract") and "tracking" not in t and not any("tracking" in kinds[c] for c in chain):
            out.append((here, "names", f"{tn} has no tracking, declared or inherited"))
        members = dict(t.get("attributes") or {})
        for c in chain:
            members.update(kinds[c].get("attributes") or {})
        # a family is ported together: a mirror's base is a mirror, and so is every type extending a mirror (ADR-0148)
        for c in chain:
            if bool(kinds[c].get("mirror")) != bool(t.get("mirror")):
                out.append((here, "names", f"{tn} {'is' if t.get('mirror') else 'is not'} a mirror and extends {c}, which "
                                           f"{'is not' if t.get('mirror') else 'is'}; a family is ported together, every type "
                                           "of it a mirror or none (ADR-0148)"))
                break
        if t.get("mirror") and not t.get("abstract") and not any(a.get("external") for a in members.values()):
            out.append((here, "names", f"{tn} is a mirror and declares no external identifier, by which the import matches its objects"))
        if t.get("mirror"):
            # a type is cut over only after every type referencing it (ADR-0075), so a mirror stores no reference to a
            # type this store owns: one with no opposite, marked stored, or whose opposite end is a set
            for a, spec in (t.get("attributes") or {}).items():
                ref = str((spec or {}).get("reference", ""))
                target = kinds.get(ref.rstrip("[]")) if ref else None
                if target is None or target.get("mirror"):
                    continue
                far = (target.get("attributes") or {}).get(spec.get("opposite") or "") or {}
                stored = ("opposite" not in spec) or spec.get("stored") or (
                    not ref.endswith("[]") and str(far.get("reference", "")).endswith("[]"))
                if stored:
                    out.append((here + ("attributes", a), "names", f"{tn}.{a} is a mirror's reference to {ref.rstrip('[]')}, which this "
                                "store owns: a type is cut over only after every type that references it (ADR-0075), so a mirror "
                                "references only mirrors"))
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
            elif (states.get(new) or {}).get("superseding"):
                out.append((where + (old,), "names", f"the migration moves {tn}.{old} to {new}, a superseding state, which would "
                                                     "leave its objects superseded by nothing (ADR-0146)"))
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
    observed = {ob["kind"] for _tn, t in types(v) for ob in (t.get("observations") or {}).values()}
    admitted = set()
    for i, adm in enumerate(m.get("admit") or []):
        where = ("migration", "admit", i)
        tn, inv = adm["invariant"].split(".")
        if adm["invariant"] in admitted:
            out.append((where, "names", f"the migration admits {adm['invariant']} twice; one change admits an invariant once (ADR-0147)"))
        admitted.add(adm["invariant"])
        if tn in observed:
            out.append((where, "names", f"the migration admits {adm['invariant']}, an observation kind's invariant, which no "
                                        "migration reaches; its recorded observations are reported as a notice (ADR-0147)"))
        elif tn not in kinds:
            out.append((where, "names", f"the migration admits {adm['invariant']}, and this module declares no type {tn}"))
        elif inv not in invariant_names(doc, tn, kinds[tn]):
            out.append((where, "names", f"the migration admits {adm['invariant']}, which is not an invariant of {tn}"))
        en, member = adm["reason"].split(".")
        if en in enums and member not in enums[en]:
            out.append((where, "names", f"the migration admits {adm['invariant']} because {adm['reason']}, and {en} has no {member}"))
        elif en not in enums and en not in imported:
            out.append((where, "names", f"the migration's reason {adm['reason']} names {en}, which is not an enumeration of this module"))
    return out


def final_admission_notes(doc, v):
    """An admission of an invariant that can fail only in a final state can never be discharged, since nothing writes an
    object there but an erasure: a placeholder no one can update (ADR-0146)."""
    kinds = dict(types(v))
    out = []
    for i, adm in enumerate((doc.get("migration") or {}).get("admit") or []):
        tn, inv = adm["invariant"].split(".")
        t = kinds.get(tn) or {}
        states = t.get("states") or {}
        expr = ((t.get("invariants") or {}).get(inv) or {}).get("expression", "")
        m = re.match(r"\s*state\s*!=\s*([A-Z][A-Z0-9_]*)\s+or\b", str(expr))
        gen = re.fullmatch(r"([a-z0-9_]+)_invariant", inv)
        only = m.group(1) if m else (gen.group(1).upper() if gen and gen.group(1).upper() in states else None)
        if only and (states.get(only) or {}).get("final"):
            out.append((("migration", "admit", i), "migration", f"the migration admits {adm['invariant']}, which fails only in {only}, a "
                        "final state no transition leaves: nothing can ever discharge the admission"))
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
        for a, b in sorted(renamed.items()):
            before, after = (ot.get("attributes") or {}).get(a), (t.get("attributes") or {}).get(b)
            if before and after and (str(before.get("type", before.get("reference"))), optional(before)) != \
                    (str(after.get("type", after.get("reference"))), optional(after)):
                fatal.append((("migration", "renamed_attributes", tn, a), "names",
                              f"the migration renames {tn}.{a} to {b}, which changes its type or whether it is optional; a rename "
                              "carries the values and their history unchanged (ADR-0146)"))
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
        for a in sorted(set(t.get("attributes") or {}) & set(ot.get("attributes") or {})):
            now_spec, was_spec = t["attributes"][a] or {}, ot["attributes"][a] or {}
            if str(now_spec.get("type", now_spec.get("reference"))) != str(was_spec.get("type", was_spec.get("reference"))):
                notes.append((here + ("attributes", a), "migration", f"{tn}.{a} changed its type: that is a drop and an add of the "
                                                                    "same name, every object's value starting absent (ADR-0147)"))
            elif optional(was_spec) and not optional(now_spec) and a not in filled and not str(now_spec.get("reference", "")).endswith("[]"):
                notes.append((here + ("attributes", a), "migration", f"{tn}.{a} is now required, which counts as a new required "
                                                                    "attribute: a publish over objects lacking it needs a backfill (ADR-0147)"))
        for d in sorted(dropped):
            for a in sorted(added):
                if (ot["attributes"][d].get("type"), ot["attributes"][d].get("reference")) == (t["attributes"][a].get("type"), t["attributes"][a].get("reference")):
                    notes.append((here + ("attributes", a), "migration", f"{tn}.{d} is gone and {tn}.{a} is new, of the same type: if it is a rename, map it, or its values stay behind"))
        held_personal = any((sp or {}).get("personal") for sp in (ot.get("attributes") or {}).values()) or any(
            (sp or {}).get("personal") for x in (ot.get("transitions") or {}).values() for sp in (x.get("inputs") or {}).values())
        if held_personal and not any(x["kind"] == "erasure" for x in (t.get("transitions") or {}).values()):
            fatal.append((here, "names", f"{tn} held personal data in the version before and now declares no erasure; a type keeps "
                                         "its erasure once it has needed one, so the values written then can be erased "
                                         "(the model's check 60, ADR-0147)"))
        for o, ob in (t.get("observations") or {}).items():
            before = ((ot.get("observations") or {}).get(o) or {}).get("attributes")
            if before is None:
                continue
            for fld, spec in (ob.get("attributes") or {}).items():
                if fld not in before and not optional(spec):
                    fatal.append((here + ("observations", o, "attributes", fld), "names",
                                  f"{o}.{fld} is added to an observation kind and is required; an observation is born final, so nothing could fill it"))
    # a declared metric over transitions whose filter names a state this version reroutes (ADR-0129)
    def route(tv, tn, field, st):
        """The transitions a filter comparing `field` to `st` can match: those entering it, or those leaving it."""
        out = set()
        for xn, x in ((tv.get(tn) or {}).get("transitions") or {}).items():
            frm = x.get("from") if isinstance(x.get("from"), list) else [x.get("from")]
            to = x.get("to")
            if (field == "to_state" and to == st) or (field == "from_state" and to and to != st and (st in frm or "any" in frm)):
                out.add(xn)
        return out
    declared = [(("types", tn, "metrics", mn), mm) for tn, t in new.items() for mn, mm in (t.get("metrics") or {}).items()] \
        + [(("metrics", mn), mm) for mn, mm in (doc.get("metrics") or {}).items()]
    for where, mm in declared:
        if mm.get("source") != "transitions":
            continue
        for field, owner, st in sorted(set(re.findall(r"\.(from_state|to_state)\s*==\s*(\w+)\.(\w+)", str(mm.get("filter") or "")))):
            if owner in old and owner in new and route(old, owner, field, st) != route(new, owner, field, st):
                way = "into" if field == "to_state" else "out of"
                notes.append((where, "metric", f"{where[-1]}'s filter counts transitions {way} {owner}.{st}, and this version changes "
                                               "which transitions those are, so what the metric counts changes from this version on"))
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
    # who may take a transition, or approve a version, named no longer or widened (ADR-0189 decision 1)
    for tn, t in new.items():
        if tn in machines or tn not in old or t.get("abstract"):
            continue
        ot = old[tn]
        for xn, ox in (ot.get("transitions") or {}).items():
            x = (t.get("transitions") or {}).get(xn)
            if not isinstance(x, dict) or not isinstance(ox, dict):
                continue
            was, now = taken_kinds(ox, ot), taken_kinds(x, t)
            if was and not now:
                notes.append((("types", tn, "transitions", xn), "taken_by_dropped",
                              f"{tn}.{xn} was taken by [{', '.join(was)}], and this version names no kind, so every kind of "
                              f"actor may take it (ADR-0189)"))
            elif was and not set(now) <= set(was):
                notes.append((("types", tn, "transitions", xn), "taken_by_dropped",
                              f"{tn}.{xn} is taken by [{', '.join(now)}], where the previous version allowed "
                              f"[{', '.join(was)}] (ADR-0189)"))
    # an attribute no longer personal, whose values an erasure then no longer reaches (ninth production review N-11,
    # ADR-0196 decision 6)
    for tn, t in new.items():
        if tn in machines or tn not in old or t.get("abstract"):
            continue
        for an, oa in (old[tn].get("attributes") or {}).items():
            a = (t.get("attributes") or {}).get(an)
            if isinstance(oa, dict) and isinstance(a, dict) and oa.get("personal") is True and a.get("personal") is not True:
                notes.append((("types", tn, "attributes", an), "personal_dropped",
                              f"{tn}.{an} was personal, and this version drops it: an erasure no longer reaches its "
                              f"values, which stay where they are (ADR-0196)"))
    # the rules a version removes or rewrites, named to whoever approves it, so a draft making a guard always true
    # is seen (fifth production review REVIEW-GAP, ADR-0191 decision 2)
    for tn, t in new.items():
        if tn in machines or tn not in old or t.get("abstract"):
            continue
        said = rules_changed(old[tn], t)
        if said:
            notes.append((("types", tn), "rule_changed",
                          f"{tn}'s rules change against the previous version: {'; '.join(said)}; read each before approving "
                          f"(ADR-0191)"))
    was, now = (sorted(kinds_named((d.get("publishing") or {}).get("taken_by"))) for d in (prev, doc))
    if was and not now:
        notes.append((("publishing",), "taken_by_dropped",
                      f"publishing.taken_by was [{', '.join(was)}], and this version names no kind, so once it is "
                      f"installed any kind but agent may approve the next (ADR-0189)"))
    elif was and not set(now) <= set(was):
        notes.append((("publishing",), "taken_by_dropped",
                      f"publishing.taken_by is [{', '.join(now)}], where the previous version allowed "
                      f"[{', '.join(was)}] (ADR-0189)"))
    elif not was and "agent" in now:
        # named where none was, the default refusing agents (ninth production review N-1, ADR-0196 decision 5)
        notes.append((("publishing",), "taken_by_dropped",
                      f"publishing.taken_by is [{', '.join(now)}], where the previous version declared none, under "
                      f"which no agent approves: once it is installed an agent may approve the next (ADR-0196)"))
    return fatal, notes


def rules_changed(ot, t):
    """What a type's rules lose against the previous version, each a phrase: its conditions and invariants removed or
    rewritten, and its transitions' guards that refused and no longer do; none where nothing changes. What taken_by
    generates is taken_by_dropped's to say."""
    def text(spec):
        return " ".join(str((spec or {}).get("expression") or "").split()) if isinstance(spec, dict) else ""

    out = []
    for key in ("conditions", "invariants"):
        before = {n: x for n, x in (ot.get(key) or {}).items() if not (isinstance(x, dict) and TAKEN in x)}
        after = {n: x for n, x in (t.get(key) or {}).items() if not (isinstance(x, dict) and TAKEN in x)}
        gone = sorted(set(before) - set(after))
        rewritten = sorted(n for n in set(before) & set(after) if text(before[n]) != text(after[n]))
        if gone:
            out.append(f"{key} removed: {', '.join(gone)}")
        if rewritten:
            out.append(f"{key} rewritten: {', '.join(rewritten)}")
    lowered = []
    for xn, ox in sorted((ot.get("transitions") or {}).items()):
        x = (t.get("transitions") or {}).get(xn)
        if not isinstance(ox, dict) or not isinstance(x, dict):
            continue
        for g, mode in sorted((ox.get("guards") or {}).items()):
            if mode == "deny" and (x.get("guards") or {}).get(g) != "deny":
                lowered.append(f"{xn}.{g}")
    if lowered:
        out.append(f"guards no longer refusing: {', '.join(lowered)}")
    return out


def kinds_named(given):
    """The kinds of actor a `taken_by` names, its texts in the order given."""
    return [k for k in given if isinstance(k, str)] if isinstance(given, list) else []


def taken_kinds(x, t):
    """The kinds of actor that may take a transition, as the view holds it: its own `taken_by`, or its type's where it
    names none (ADR-0182 decision 5), sorted; none where neither names one."""
    return sorted(kinds_named(x.get("taken_by")) or kinds_named(t.get("taken_by")))


def agent_kinds(doc, v):
    """The identity attributes of a module's concrete types that declare an agent kind of actor, as `Type.attribute`."""
    machines = set(doc.get("machines") or {})
    return [f"{tn}.{an}" for tn, t in types(v) if tn not in machines and not t.get("abstract")
            for an, a in (t.get("attributes") or {}).items() if isinstance(a, dict) and a.get("actor_kind") == "agent"]


def agent_open(doc, v):
    """Each concrete type's count of the transitions a request may name that name no `taken_by`, its type's included:
    an agent may take each on the wire wherever the version declares an agent kind of actor (ADR-0189 decision 1)."""
    machines = set(doc.get("machines") or {})
    out = {}
    for tn, t in types(v):
        if tn in machines or t.get("abstract"):
            continue
        n = sum(1 for x in (t.get("transitions") or {}).values()
                if isinstance(x, dict) and not x.get("only_via") and not taken_kinds(x, t))
        if n:
            out[tn] = n
    return out


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
            elif a in WITHDRAWN_MEMBERS:
                out.append((path, "names", f"'{a}' cannot name a member: it is kept for a member withdrawn before 1.0.0, which may return with a use (ADR-0210 decision 3)"))
            elif a in SHADOWING:
                out.append((path, "names", f"'{a}' cannot name a member: every object has one of that name, which it would "
                                           "shadow (the model's check 33)"))
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
                out.append((("types", tn, "transitions", xn), "names", f"'{xn}' cannot name a transition: " + (
                    "the text language reserves it" if xn == "any" else "the engine's own events carry that name (ADR-0148)")))
        for c in (t.get("conditions") or {}):
            if c.endswith("_provided") and c[:-len("_provided")] in attrs:
                out.append((("types", tn, "conditions", c), "names",
                            f"'{c}' is the name of the guard generated for a required input; choose another name"))
            if c in GENERATED_GUARDS:
                out.append((("types", tn, "conditions", c), "names",
                            f"'{c}' is the name of a guard the engine generates, so a refusal naming it would not say which "
                            "it was; choose another name"))
        for i in (t.get("invariants") or {}):
            if i.endswith("_invariant") and i[:-len("_invariant")].upper() in states:
                out.append((("types", tn, "invariants", i), "names",
                            f"'{i}' is the name of the invariant generated for a state's required_attributes; choose another name"))
            if i.endswith("_unique") and i[:-len("_unique")] in attrs:
                out.append((("types", tn, "invariants", i), "names",
                            f"'{i}' is the name of the invariant a unique attribute or a one-to-one end generates; choose another name"))
            if i == "identity_taken":
                out.append((("types", tn, "invariants", i), "names",
                            f"'{i}' is the name of the invariant an actor's identity generates (ADR-0190); choose another name"))
    return out


def name_errors(doc, library=None, ordered=True):
    library = dict(library or {})
    library.update(dict(types(doc)))
    out = (order_errors(doc) if ordered else []) + reserved_name_errors(doc) + relationship_errors(doc, library) + combined_errors(doc) \
        + cascade_cycle_errors(doc, library) + clock_errors(doc, library) + personal_errors(doc, library) \
        + reparent_errors(doc, library) + symmetry_errors(doc, library) + argument_type_errors(doc, library) \
        + expression_type_errors(doc, library)
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
            # no transition leaves a state for itself: a change that keeps the state is internal (§4.8, ADR-0122, D518)
            if x.get("kind") == "external" and x.get("to") in sources(x):
                out.append((base + ("from",), "names",
                            f"{tn}.{xn} leaves {x['to']} for itself; an external transition changes the state, and a "
                            "change that keeps it is an internal transition"))
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
                    out += call_errors(library, where, t, tn, xn, x, st["call"], binders(library, tn, x, path))
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
                if a in always and "default" not in (attrs.get(a) or {}):
                    out.append((base + ("optional_inputs",), "names",
                                f"{tn}.{xn} lists '{a}' as an optional input, but the attribute is neither optional nor defaulted, so the input is required"))
            written = set(x.get("required_inputs", [])) | set(x.get("optional_inputs", [])) | {loc for _e, loc in assigns(x)}
            for a in sorted(written & set(cleared(x))):
                out.append((base + ("effect",), "names", f"{tn}.{xn} both writes and clears '{a}'"))
            out += required_errors(tn, xn, x, required, always | minted, library)
        for c in sorted(conds - used):
            out.append((("types", tn, "conditions", c), "names", f"condition '{c}' of {tn} is used by no transition"))
        out += value_errors(doc, tn, t)
        out += write_type_errors(doc, tn, t, library)
        out += conditional_step_errors(tn, t)
        out += (read_errors(tn, t, set(doc.get("categories") or []) | BUILT_IN_CATEGORIES | evaluator_names(doc))
                + indexed_errors(tn, t, library) + scan_errors(tn, t, library) + observation_read_errors(tn, t) + binder_errors(tn, t) + identifier_errors(doc, tn, t, library) + evaluator_errors(doc, tn, t)
                + supersession_errors(doc, tn, t, library))
        out += override_errors(doc, tn, t, library)
        for mn, m in (t.get("metrics") or {}).items():
            out += metric_errors(tn, t, mn, m)
    return out


def event_inputs(target, x):
    """The inputs of a transition that take an event: declared ones of type
    event, and attribute inputs whose attribute is one."""
    attrs = (target or {}).get("attributes") or {}
    named = {i for i, spec in (x.get("inputs") or {}).items() if spec.get("type") == "event"}
    named |= {a for a in x.get("required_inputs", []) + x.get("optional_inputs", []) if (attrs.get(a) or {}).get("type") == "event"}
    return named


def event_errors(where, tn, xn, target_name, x, given, target):
    """An event-typed input accepts only this_event, which stops an approval
    being recorded against an old event (declaration-syntax.md §6.7; the
    model's check 25)."""
    return [(where, "names", f"{tn}.{xn} passes {' '.join(str(given[i]).split())} to {target_name}'s event input '{i}', which accepts only this_event (check 25)")
            for i in sorted(event_inputs(target, x) & set(given)) if " ".join(str(given[i]).split()) != "this_event"]


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
    out += event_errors(where, tn, xn, f"{c['type']}.{c['transition']}", t, c.get("inputs") or {}, target)
    out += [(where, "names", f"{tn}.{xn} creates a {c['type']} without the input '{i}', which {c['type']}.{c['transition']} requires")
            for i in sorted(required_input_names(t) - given) if "default" not in ((t.get("inputs") or {}).get(i) or {})]
    return out


def binders(library, tn, x, path):
    """The type each loop enclosing an effect step binds its item to, where the loop ranges over objects of a known
    type: a set end, a set input, a type scan or a collection reached through a path."""
    env, steps = {}, x.get("effect") or []
    for depth in range(0, len(path) - 1, 3):
        spec = steps[path[depth]]["foreach"]
        at = OptionalReads(library, tn, x).walk(flowexpr.parse(spec["array"]), dict(env)) if "array" in spec else None
        if at is None and "array" in spec and spec["array"] in library:
            at = spec["array"] + "[]"           # a type scan
        env[spec["item"]] = at[:-2] if at and at.endswith("[]") else None
        steps = spec.get("steps") or []
    return env


def call_target(library, t, tn, x, c, env=None):
    """The type of a call's target, read from an attribute, an input, or a path from either or from a loop's item,
    the loops' items typed by what they range over; None where it cannot be known, or is a set or no type known."""
    target = " ".join(c["target"].split())
    spec = None
    m = re.fullmatch(r"(?:this\.)?([a-z][a-z0-9_]*)", target)
    if m and m.group(1) in (t.get("attributes") or {}):
        spec = t["attributes"][m.group(1)]
    m = re.fullmatch(r"inputs\.([a-z][a-z0-9_]*)", target)
    if m:
        spec = (x.get("inputs") or {}).get(m.group(1)) or (t.get("attributes") or {}).get(m.group(1))
    kind = (spec or {}).get("reference")
    if not kind:
        try:
            kind = OptionalReads(library, tn, x).walk(flowexpr.parse(target), dict(env or {}))
        except flowexpr.ParseError:
            kind = None                         # step 2 refuses what does not parse
    return None if not kind or kind.endswith("[]") or kind not in library else kind


def call_errors(library, where, t, tn, xn, x, c, env=None):
    """A call names a transition of its target's type that acts on an existing object, and passes its inputs; a
    target whose type cannot be known is left to the publish checks."""
    kind = call_target(library, t, tn, x, c, env)
    if kind is None:
        return []
    other = library[kind]
    called = (other.get("transitions") or {}).get(c["transition"])
    if called is None:
        return [(where, "names", f"{tn}.{xn} calls {kind}.{c['transition']}, which is not a transition of {kind}")]
    if called["kind"] == "initial":
        return [(where, "names", f"{tn}.{xn} calls {kind}.{c['transition']}, an initial transition; create an object with create")]
    if called["kind"] == "assertion":
        return [(where, "names", f"{tn}.{xn} calls {kind}.{c['transition']}, an assertion, which only a request takes; an "
                                 f"override is requested, never caused (ADR-0163)")]
    given = set(c.get("inputs") or {})
    out = [(where, "names", f"{tn}.{xn} passes '{i}' to {kind}.{c['transition']}, which takes no input '{i}'") for i in sorted(given - taken(called))]
    out += event_errors(where, tn, xn, f"{kind}.{c['transition']}", called, c.get("inputs") or {}, other)
    out += [(where, "names", f"{tn}.{xn} calls {kind}.{c['transition']} without the input '{i}', which it requires")
            for i in sorted(required_input_names(called) - given) if "default" not in ((called.get("inputs") or {}).get(i) or {})]
    return out


def present(expr, known, inputs):
    """Whether an assigned expression provably has a value: now, who acts, a literal, a qualified value, a known
    attribute or a required input. Anything else is not provable here, so it does not count."""
    if expr in ("now", "actor.id", "actor.kind") or re.fullmatch(r'-?\d+(\.\d+)?|"[^"]*"|[A-Z][A-Za-z0-9]*\.[A-Z][A-Z0-9_]*', expr):
        return True
    if NAME.fullmatch(expr) and expr not in NOT_AN_ATTRIBUTE:
        return expr in known
    m = re.fullmatch(r"inputs\.([a-z][a-z0-9_]*)", expr)
    return bool(m) and m.group(1) in inputs


def present_path(expr, tn, x, library, known, inputs):
    """Whether an assigned path provably has a value: from an attribute known to have one, or a required input,
    through references none of which is optional, to an attribute that is not optional (flow-format.md §4.5), as
    `cost_centre.fiscal_year` does. An attribute the path's types do not declare themselves, an inherited one or a
    machine's, is not counted, so nothing is passed that the walk cannot see."""
    parts = expr.split(".")
    start = 1 if parts[0] == "inputs" else 0
    if len(parts) < start + 2 or not all(NAME.fullmatch(p) for p in parts[start:]):
        return False
    name, rest = parts[start], parts[start + 1:]
    if start:
        if name not in inputs:
            return False
        spec = (x.get("inputs") or {}).get(name) or (library[tn].get("attributes") or {}).get(name)
    else:
        if name not in known or name in NOT_AN_ATTRIBUTE:
            return False
        spec = (library[tn].get("attributes") or {}).get(name)
    for seg in rest:
        kind = (spec or {}).get("reference")
        if not kind or kind.endswith("[]") or kind not in library:
            return False
        spec = (library[kind].get("attributes") or {}).get(seg)
        if spec is None or spec.get("optional"):
            return False
    return True


def required_errors(tn, xn, x, required, always, library=None):
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
        if a in assigned and library and present_path(assigned[a], tn, x, library, supplied | kept, required_input_names(x)):
            continue
        how = f"or require it in state {', '.join(src)}" if src else "since an initial transition starts from no state"
        out.append((base + ("to",), "required",
                    f"{tn}.{xn} enters {x['to']} without setting '{a}', which {x['to']} requires: "
                    f"make it a required input, assign it a value that is provably present, {how}"))
    return out


SIMPLE_TYPES = {"string", "int", "bool", "timestamp", "duration", "identity", "file", "date", "actor_kind"}


def written_kind(spec, enums):
    """What a write to an attribute must be, as far as a publish can tell: a reference to a type, a member of an
    enumeration, money in one currency, a decimal, or one of the simple types; None where it cannot be told here."""
    if not spec:
        return None
    if "reference" in spec:
        return None if spec["reference"].endswith("[]") else ("reference", spec["reference"])
    typ = str(spec.get("type", ""))
    if typ in enums:
        return ("enumeration", typ)
    m = re.fullmatch(r"money\((\w+)\)", typ)
    if m:
        return ("money", m.group(1))
    if re.fullmatch(r"decimal\(\d+,\s*\d+\)", typ):
        return ("decimal",)
    return ("int",) if typ == "counter" else ((typ,) if typ in SIMPLE_TYPES else None)


def value_kind(e, tn, t, x, enums):
    """The kind of an assigned value where a publish can tell it: a literal, now, an enumeration's member, an input,
    an attribute of this object, this object, or actor.id; None for anything else, which no finding is made on."""
    k = e[0]
    if k == "str":
        return ("string",)
    if k == "num":
        return ("number",)
    if k == "call" and e[1] == ("name", "date"):
        return ("date",)
    if k == "money":
        return ("money", e[1])
    if k == "bool":
        return ("bool",)
    attrs = t.get("attributes") or {}
    if k == "name":
        if e[1] == "now":
            return ("timestamp",)
        if e[1] == "this":
            return ("reference", tn)
        if e[1] in ACTOR_KINDS and e[1] not in attrs:
            return ("actor_kind",)
        return written_kind(attrs.get(e[1]), enums)
    if k == "path" and e[1][0] == "name":
        base, m = e[1][1], e[2]
        if base in enums:
            return ("enumeration", base)
        if base == "this":
            return written_kind(attrs.get(m), enums)
        if base == "actor" and m in ("id", "kind"):
            return ("identity",) if m == "id" else ("actor_kind",)
        if base == "inputs":
            spec = (x.get("inputs") or {}).get(m)
            if spec is None and m in set(x.get("required_inputs", [])) | set(x.get("optional_inputs", [])):
                spec = attrs.get(m)
            return written_kind(spec, enums)
    return None


def range_kind(e, tn, t, x, enums):
    """What a loop's range counts to, where a publish can tell: a number literal is an int only if it is whole, and an
    input or attribute declared as a set is a set, which value_kind leaves untold (ADR-0146)."""
    if e[0] == "num":
        return ("int",) if isinstance(e[1], int) else ("decimal",)
    attrs = t.get("attributes") or {}
    m = e[1] if e[0] == "name" else e[2] if e[0] == "path" and e[1] in (("name", "inputs"), ("name", "this")) else None
    spec = None
    if m is not None:
        spec = attrs.get(m) if e[0] == "name" or e[1] == ("name", "this") else (x.get("inputs") or {}).get(m) or attrs.get(m)
    if spec and str(spec.get("type", spec.get("reference", ""))).endswith("[]"):
        return ("set",)
    return value_kind(e, tn, t, x, enums)


def fits(target, value, library):
    """Whether a value of one kind may be written to a target of another (the model's check 17)."""
    if target == value:
        return True
    if value == ("number",) and target in (("int",), ("decimal",)):
        return True
    if target == ("decimal",) and value == ("int",):
        return True
    if target[0] == value[0] == "reference":       # a subtype fits where its base is written
        seen, kind = set(), value[1]
        while kind and kind not in seen:
            if kind == target[1]:
                return True
            seen.add(kind)
            kind = (library.get(kind) or {}).get("extends")
    return False


MEMBER_TYPES = {"open": ("bool",), "id": ("identity",), "created_at": ("timestamp",), "created_by_kind": ("actor_kind",),
                "recorded_from": ("timestamp",), "declaration_version": ("int",)}
SCALARS = ("int", "decimal", "number")


def attribute_type(spec, enums):
    """An attribute's or an input's type as an expression reads it, a decimal with its scale, a set of references as
    the objects it holds; None where it cannot be told."""
    if not spec:
        return None
    ref = str(spec.get("reference", ""))
    if ref:
        return ("objects", ref[:-2]) if ref.endswith("[]") else ("reference", ref)
    typ = str(spec.get("type", ""))
    if typ.endswith("[]"):
        # a set of values, of its element's type, where it typed as nothing, so a set added, multiplied, ordered or
        # negated published and the core did not run it (the differential battery, 2026-10-15)
        element = attribute_type({"type": typ[:-2]}, enums)
        return None if element is None else ("values", element)
    m = re.fullmatch(r"decimal\(\s*\d+\s*,\s*(\d+)\s*\)", typ)
    if m:
        return ("decimal", int(m.group(1)))
    return written_kind(spec, enums)


def type_text(k):
    """A type as a finding names it."""
    if k[0] == "decimal":
        return f"a decimal of scale {k[1]}" if k[1] is not None else "a decimal"
    if k[0] == "money":
        return f"money({k[1]})"
    if k[0] == "reference":
        return f"an object of {k[1]}"
    if k[0] == "objects":
        return f"a set of {k[1]}"
    if k[0] == "values":
        return f"a set of {k[1][1] if k[1][0] in ('enumeration', 'reference') else k[1][0]}"
    if k[0] == "enumeration":
        return f"a member of {k[1]}"
    if k[0] == "state":
        return f"a state of {k[1]}"
    return {"int": "an int", "identity": "an identity", "actor_kind": "an actor kind"}.get(k[0], f"a {k[0]}")


class Typing:
    """The types of a module's expressions as far as a publish can tell them, and each rule of declaration-syntax.md
    §8.3 an expression breaks (the model's check 24). A type is inferred from literals, members and inputs, paths
    through typed references, aggregates over typed collections and the functions §8.3 lists, and is unknown, None,
    wherever it cannot be told; a rule is decided only where every operand it reads is known, so nothing is refused for
    what the inference does not reach."""

    def __init__(self, kinds, enums, categories, evaluators, known=None):
        self.kinds, self.enums, self.categories, self.evaluators = kinds, enums, categories, evaluators
        self.broken = []
        # whether a flow function's argument is checked here: in the places check 58 does not reach through a
        # transition's body, a derived attribute, an invariant, a uniqueness condition and a backfill
        self.flow_names = False
        # each derived attribute's type, by its type and name, once typed: a chain of attributes each reading the one
        # before twice typed each 2^n times, forty holding the dry run's slot past six minutes (eleventh production
        # review)
        self.known = {} if known is None else known

    def member(self, tn, m, seen):
        t = self.kinds.get(tn) or {}
        if m == "state":
            return ("state", tn)
        if m in MEMBER_TYPES:
            return MEMBER_TYPES[m]
        spec = (t.get("attributes") or {}).get(m)
        if spec is not None:
            return attribute_type(spec, self.enums)
        derived = (t.get("derived_attributes") or {}).get(m)
        # a derived attribute typed through at most TYPING_DEPTH others, past which its type is unknown: each typed the
        # whole chain below it again, a chain of 1,000 taking minutes and one of 3,000 deeper than a stack (tenth
        # production review)
        if (tn, m) in self.known:
            return self.known[(tn, m)]
        if derived is not None and (tn, m) not in seen and len(seen) < TYPING_DEPTH:
            try:
                tree = flowexpr.parse(" ".join(str((derived or {}).get("expression", "")).split()))
            except flowexpr.ParseError:
                return None
            quiet = Typing(self.kinds, self.enums, self.categories, self.evaluators, self.known)
            ty = quiet.of(tree, tn, None, {}, seen | {(tn, m)})
            self.known[(tn, m)] = ty
            return ty
        return None

    def undeclared(self, tn, m):
        """Why a member read through a reference is none its type has: an attribute, a derived attribute, an
        observation kind, what every object has, its flow data among it, its state or its referrers; none where it is
        one, or where nothing declares the type (D695)."""
        t = self.kinds.get(tn)
        if t is None or m in ("state", "referrers") or m in MEMBER_TYPES or m in OptionalReads.BUILT_IN:
            return None
        if m in (t.get("attributes") or {}) or m in (t.get("derived_attributes") or {}) or m in (t.get("observations") or {}):
            return None
        if m in WITHDRAWN_MEMBERS:
            return (f"it reads {m} of {tn}, which is withdrawn before 1.0.0, no store keeping it, and may return with a "
                    "use (ADR-0210 decision 3)")
        return f"it reads {m} of {tn}, which {tn} does not declare"

    def fail(self, detail):
        self.broken.append(detail)

    def of(self, e, tn, x, env, seen=frozenset()):
        """The type of an expression evaluated on an object of type tn, in transition x where one is, with its
        loops' and aggregates' bound names in env; each rule it breaks noted in self.broken, in evaluation order."""
        k = e[0]
        if k == "num":
            if isinstance(e[1], int) and e[1] > 2 ** 63 - 1:
                # an int literal past 64 bits, which has no value (§8.1, the values audit's L3)
                self.fail(f"the literal {e[1]} is past an int's 64 bits")
                return ("int",)
            # a literal's scale is its digits after the point, which a product adds (§8.3); in a sum or a
            # comparison it takes the other operand's type
            return ("number", 0 if isinstance(e[1], int) else len(e[1].split(".", 1)[1]))
        if k == "str":
            return ("string",)
        if k == "bool":
            return ("bool",)
        if k == "money":
            # an amount in a currency the engine has no minor unit for, or with more places than its minor unit, is
            # one it cannot hold, where it was rounded (the audit's values L1 and L6)
            places = flowrun.MINOR_UNITS.get(e[1])
            if places is None:
                self.fail(f"{e[1]} is a currency whose minor unit the engine does not know")
            elif (Fraction(str(e[2])) * 10 ** places).denominator != 1:
                self.fail(f"{e[1]} {e[2]} has more places than {e[1]}'s minor unit")
            return ("money", e[1])
        if k == "dur":
            # a duration is whole seconds, where `1.5 s` was rounded to 2 s (the audit's values L2), and holds 64 bits of
            # them, where one past was not run (ADR-0202 decision 8)
            if e[2] in flowrun.SECONDS:
                seconds = Fraction(str(e[1])) * flowrun.SECONDS[e[2]]
                if seconds.denominator != 1:
                    self.fail(f"{e[1]} {e[2]} is no whole number of seconds")
                elif seconds > 2 ** 63 - 1:
                    self.fail(f"{e[1]} {e[2]} is past a duration's 64 bits of seconds")
            return ("duration",)
        if k == "name":
            n = e[1]
            if n in env:
                return env[n]
            if n == "now":
                return ("timestamp",)
            if n == "this":
                return ("reference", tn)
            if n in self.categories:
                return ("category",)
            if n in ((self.kinds.get(tn) or {}).get("states") or {}):
                return ("state", tn)
            # an enumeration's name is no value, and a name naming nothing is none, where each typed as nothing and
            # the core did not run it (the values audit's K-9); an all-capital name is a state's, the rule below's
            if n in self.enums:
                self.fail(f"{n} is an enumeration, which is no value: name one of its members, as {n}.<MEMBER>")
                return None
            got = self.member(tn, n, seen)
            if got is None and n in ACTOR_KINDS:
                return ("actor_kind",)
            if got is None and n[:1].isupper() and not n.isupper() and n not in self.kinds:
                self.fail(f"{n} names nothing: no type, enumeration or state of {tn} is called {n}")
                return None
            return got
        if k == "path":
            base, m = e[1], e[2]
            if base == ("name", "inputs"):
                spec = ((x or {}).get("inputs") or {}).get(m)
                if spec is None and m in set((x or {}).get("required_inputs") or []) | set((x or {}).get("optional_inputs") or []):
                    spec = ((self.kinds.get(tn) or {}).get("attributes") or {}).get(m)
                return attribute_type(spec, self.enums)
            if base == ("name", "actor") and "actor" not in env:
                # who acted, as an outcome value reads it (§8.1): `.id` an identity, `.kind` an actor kind, `.principal`
                # a string, where they typed as nothing, so `"by " + actor.id` passed and the core did not run it (D687)
                return {"id": ("identity",), "kind": ("actor_kind",), "principal": ("string",)}.get(m)
            if base[0] == "name" and base[1] not in env and base[1] in self.enums:
                return ("enumeration", base[1])
            if base[0] == "name" and base[1] not in env and base[1] in self.kinds and m.isupper():
                return ("state", base[1])
            # a type's member is one of its states, and a value with no members has none read, where each typed as
            # nothing and the core did not run it (the values audit's K-9)
            if base[0] == "name" and base[1] not in env and base[1][:1].isupper() and base[1] not in self.kinds:
                return None                     # an owner nothing declares, the qualified names' rule's
            if base[0] == "name" and base[1] not in env and base[1] in self.kinds:
                b = base[1]
                self.fail(f"{b}.{m} names no state of {b}")
                return None
            bt = self.of(base, tn, x, env, seen)
            if bt is not None and bt[0] == "reference":
                why = self.undeclared(bt[1], m)
                if why:
                    self.fail(why)
                    return None
                return self.member(bt[1], m, seen)
            if bt is not None and bt[0] == "state" and m == "category":
                return ("category",)
            if bt is not None and bt[0] == "state":
                self.fail(f"it reads {m} of a state, which has only category")
                return None
            if bt is not None and bt[0] in NO_MEMBERS:
                t = type_text(bt)
                self.fail(f"it reads {m} of {t}, which has no members")
                return None
            return None
        if k == "call":
            for a in e[2]:
                if a[0] != "name":
                    self.of(a, tn, x, env, seen)
            f = e[1]
            if f[0] == "path" and f[1][0] == "name" and f[1][1] in self.evaluators:
                return ("verdict",)
            # a function the language has, given one argument, where any other call typed as nothing and the core
            # did not run it (the values audit's K-7, K-8)
            if f[0] == "name" and f[1] not in FUNCTIONS and f[1] not in METRIC_ONLY_FUNCTIONS:
                name = f[1]
                self.fail(f"{name} is no function the language has: it has length, date, entered_at and time_in, and in a metric avg, median, percentile, day, week, month, quarter and year")
                return None
            if f[0] == "name" and f[1] in FUNCTIONS and len(e[2]) != 1:
                name, n = f[1], len(e[2])
                self.fail(f"{name} takes one argument, and is given {n}")
                return None
            if f[0] == "path" and f[2] not in MEMBER_FUNCTIONS:
                name = f[2]
                self.fail(f"{name} is no member function the language has: it has intervals, entered_at, time_in and a row's held")
                return None
            # a member function on what has it, `held` on a row of intervals or transitions, the others on an object,
            # `intervals(m)` of a member its type tracks; and a flow function names a state or a member, never a
            # literal; where each published and the core did not run it (the values audit's K-8)
            bt = self.of(f[1], tn, x, env, seen) if f[0] == "path" else None
            if bt is not None:
                if f[2] == "held":
                    self.fail(f"held is a row's, of intervals or transitions, and it is called on {type_text(bt)}")
                    return None
                if bt[0] != "reference":
                    self.fail(f"{f[2]} is an object's, and it is called on {type_text(bt)}")
                    return None
                if f[2] == "intervals" and len(e[2]) == 1 and e[2][0][0] == "name" and e[2][0][1] != "state":
                    ot = self.kinds.get(bt[1])
                    if ot is not None and e[2][0][1] not in tracked_attributes(ot, self.kinds):
                        self.fail(f"intervals({e[2][0][1]}) names neither the state nor a member {bt[1]} tracks")
                        return None
            called = f[1] if f[0] == "name" else f[2] if f[0] == "path" else None
            if called in ("time_in", "entered_at") and len(e[2]) == 1 and e[2][0][0] in ("str", "num", "bool", "money", "dur"):
                at = self.of(e[2][0], tn, x, env, seen)
                if at is not None:
                    self.fail(f"{called} names a state or a tracked member, and is given {type_text(at)}")
                    return None
            # a flow function names what its object's type has: `time_in` a state, `entered_at` a state or a tracked
            # member, wherever an expression is typed, as the model's check 58 has it for a transition's body (the
            # audit's values G2, where `time_in` of a member answered 0 s)
            fname = f[1] if f[0] == "name" else f[2] if f[0] == "path" else None
            if self.flow_names and fname in ("time_in", "entered_at") and len(e[2]) == 1 and e[2][0][0] == "name":
                on = tn
                if f[0] == "path":
                    bt = self.of(f[1], tn, x, env, seen)
                    on = bt[1] if bt is not None and bt[0] == "reference" else None
                ot = self.kinds.get(on) if on is not None else None
                if ot is not None:
                    arg, states = e[2][0][1], ot.get("states") or {}
                    if fname == "time_in" and arg not in states:
                        self.fail(f"time_in({arg}) names no state of {on}")
                    elif fname == "entered_at" and arg not in states and arg not in tracked_attributes(ot, self.kinds):
                        self.fail(f"entered_at({arg}) names neither a state of {on} nor a member it tracks")
            if f == ("name", "length"):
                arg = self.of(e[2][0], tn, x, env, seen) if len(e[2]) == 1 else None
                if arg is not None and arg[0] not in ("string", "values"):
                    self.fail(f"length takes a string or a set of values, and is given {type_text(arg)}")
                return ("int",)
            return {"entered_at": ("timestamp",), "time_in": ("duration",), "date": ("date",)}.get(f[1] if f[0] == "name" else None)
        if k == "changed":
            # the event it reads is one no type here names, so a value of a type it can tell is not one, where an int
            # was read as a log position (the values audit's G7)
            given = self.of(e[2], tn, x, env, seen)
            if given is not None:
                self.fail(f"changed_since reads an event, and is given {type_text(given)}")
            return ("bool",)
        if k == "isnull":
            self.of(e[1], tn, x, env, seen)
            return ("bool",)
        if k == "not":
            v = self.of(e[1], tn, x, env, seen)
            if v is not None and v[0] not in ("bool", "verdict"):
                self.fail(f"not takes a bool, and is given {type_text(v)}")
            return ("bool",)
        if k == "neg":
            # a negative of a number, an amount or a duration, where any other typed as its operand and the core did
            # not run it (the values audit's K-5)
            v = self.of(e[1], tn, x, env, seen)
            if v is not None and v[0] not in SCALARS + ("money", "duration"):
                self.fail(f"it negates {type_text(v)}, which has no negative")
                return None
            return v
        if k == "agg":
            _, fn, _distinct, var, coll, where, body = e
            if var is None:
                # a metric's own count, over its rows, where it typed as an int and the core did not run it (K-9)
                self.fail("count(where …) and count(distinct …) count a metric's rows, which no expression outside a metric has")
                return None
            inner = dict(env)
            if var is not None and coll[0] == "set":
                # a literal's elements share one type, its item's, where a mixed one typed as nothing and the core did
                # not run it (the values audit's K-10)
                known = [t for t in (self.of(item, tn, x, env, seen) for item in coll[1]) if t is not None]
                clash = next(((a, b) for a in known for b in known if not self.alike(a, b)), None)
                if clash is not None:
                    a, b = type_text(clash[0]), type_text(clash[1])
                    self.fail(f"{fn} ranges over a set of {a} and {b}, which share no type")
                    return None
                inner[var] = next((t for t in known if t[0] != "number"), known[0] if known else None)
            elif var is not None:
                ct = self.of(coll, tn, x, env, seen) if coll[0] != "name" or coll[1] in env or coll[1] not in self.kinds else ("objects", coll[1])
                element = self.value_set_element(tn, coll, env)
                if element is None and ct is not None and ct[0] == "values":
                    element = ct[1]                 # a set of values a path or an input names
                if ct is not None and ct[0] == "objects":
                    inner[var] = ("reference", ct[1])
                elif element is not None:
                    inner[var] = element          # a set of values, its item of their type (K-10)
                elif ct is not None:
                    # an aggregate ranges over a collection, where one over a single value was not run (K-10)
                    t = type_text(ct)
                    self.fail(f"{fn} ranges over {t}, which is no collection")
                    return None
                else:
                    inner[var] = None
            if where is not None:
                w = self.of(where, tn, x, inner, seen)
                if w is not None and w[0] != "bool":
                    self.fail(f"the condition of a {fn} is {type_text(w)}, not a bool")
            b = self.of(body, tn, x, inner, seen) if body is not None else None
            if fn == "count":
                # a count of elements counts those its body holds for, a condition, as `any`'s is (§8.3, the
                # audit's values G1); a distinct count counts its body's values
                if not _distinct and b is not None and b[0] != "bool":
                    self.fail(f"the body of count is {type_text(b)}, not a bool")
                return ("int",)
            if fn in ("all", "any", "none"):
                if b is not None and b[0] != "bool":
                    self.fail(f"the body of {fn} is {type_text(b)}, not a bool")
                return ("bool",)
            if fn in ("sum", "min", "max") and b is not None:
                if fn == "sum" and b[0] not in SCALARS + ("money", "duration"):
                    self.fail(f"sum adds {type_text(b)}, which does not add")
                elif b[0] == "enumeration":
                    # the least or the greatest member, of what has no order, which the core did not run and the
                    # reference runner answered by name (the values audit's R4, as K-4)
                    self.fail(f"{fn} orders {type_text(b)}, and an enumeration has no order")
                return b
            return None
        if k == "metric":
            for _d, v in e[2]:
                self.of(v, tn, x, env, seen)
            return None
        if k == "in":
            left = self.of(e[1], tn, x, env, seen)
            right = e[2]
            if right[0] == "name" and right[1] in self.enums and right[1] not in env:
                if left is not None and left != ("enumeration", right[1]):
                    self.fail(f"it asks whether {type_text(left)} is in {right[1]}")
            elif right[0] == "set":
                for item in right[1]:
                    it = self.of(item, tn, x, env, seen)
                    if left is not None and it is not None and not self.alike(left, it):
                        self.fail(f"it asks whether {type_text(left)} is among {type_text(it)}")
                        break
            else:
                # membership in a collection, where one in a single value was not run (the values audit's K-6)
                rt = self.of(right, tn, x, env, seen)
                if rt is not None and rt[0] not in ("objects", "values"):
                    self.fail(f"it asks for membership in {type_text(rt)}, which is no collection")
            return ("bool",)
        if k == "if":
            c = self.of(e[1], tn, x, env, seen)
            if c is not None and c[0] != "bool":
                self.fail(f"if takes a bool, and is given {type_text(c)}")
            a, b = self.of(e[2], tn, x, env, seen), self.of(e[3], tn, x, env, seen)
            if a is not None and b is not None and not self.alike(a, b):
                self.fail(f"if yields {type_text(a)} or {type_text(b)}, which share no type")
                return None
            return a if a is not None and a[0] != "number" else b
        if k == "bin":
            op, l, r = e[1], self.of(e[2], tn, x, env, seen), self.of(e[3], tn, x, env, seen)
            if op in ("and", "or", "implies"):
                for v in (l, r):
                    # a verdict inside a larger expression is check 24's evaluator rule's, refused there
                    if v is not None and v[0] not in ("bool", "verdict"):
                        self.fail(f"{op} takes bools, and is given {type_text(v)}")
                        break
                return ("bool",)
            if op in ("==", "!=", "<", "<=", ">", ">="):
                if l is not None and r is not None and not self.alike(l, r):
                    self.fail(f"it compares {type_text(l)} with {type_text(r)}")
                elif op not in ("==", "!=") and any(v is not None and v[0] == "enumeration" for v in (l, r)):
                    # an enumeration has no order, which the core did not run (the values audit's K-4)
                    self.fail(f"it orders {type_text(l if l is not None and l[0] == 'enumeration' else r)}, and an "
                              "enumeration has no order")
                elif op not in ("==", "!=") and any(v is not None and v[0] in ("state", "values") for v in (l, r)):
                    # nor has a state or a set, which the core did not run and the runner ordered by name or crashed
                    # on (the differential battery, 2026-10-15)
                    v = l if l is not None and l[0] in ("state", "values") else r
                    self.fail(f"it orders {type_text(v)}, and {'a state' if v[0] == 'state' else 'a set'} has no order")
                return ("bool",)
            # a literal zero divisor, which no value has, and a date moved by a literal part of a day, which no date is,
            # each refused at run time on every object (the differential battery)
            if op == "/" and e[3][0] == "num" and Fraction(str(e[3][1])) == 0:
                self.fail("it divides by zero, which has no value")
                return None
            if op in ("+", "-") and l is not None and l[0] == "date" and e[3][0] == "dur" and e[3][2] in flowrun.SECONDS \
                    and Fraction(str(e[3][1])) * flowrun.SECONDS[e[3][2]] % 86400:
                self.fail(f"it moves a date by {e[3][1]} {e[3][2]}, which is no whole number of days")
                return None
            if l is None or r is None:
                return None
            return self.arithmetic(op, l, r)
        if k == "set":
            for item in e[1]:
                self.of(item, tn, x, env, seen)
            return None
        return None

    def value_set_element(self, tn, coll, env):
        """The type of an element of a set of values the type declares, `tags` or `this.tags`, or None."""
        name = coll[1] if coll[0] == "name" and coll[1] not in env else \
            coll[2] if coll[0] == "path" and coll[1] == ("name", "this") else None
        spec = ((self.kinds.get(tn) or {}).get("attributes") or {}).get(name) if name else None
        typ = str((spec or {}).get("type", ""))
        return attribute_type({"type": typ[:-2]}, self.enums) if typ.endswith("[]") else None

    def alike(self, a, b):
        """Whether two types are one for a comparison or a conditional's branches: equal, a number literal and any
        number, two decimals, or references of one family; and two states, two references and an identity and a
        string, which the inference cannot always tell apart, given the benefit of the doubt."""
        if a == b or {a[0], b[0]} <= {"number", "int", "decimal"} and "number" in (a[0], b[0]):
            return True
        if a[0] == b[0] == "decimal":
            return True
        if a[0] == b[0] == "state" or {a[0], b[0]} == {"identity", "string"}:
            return True
        if a[0] == b[0] == "reference":
            return fits(("reference", a[1]), ("reference", b[1]), self.kinds) or fits(("reference", b[1]), ("reference", a[1]), self.kinds)
        return False

    def arithmetic(self, op, l, r):
        """The type of `l op r` by §8.3, or None, noting a rule it breaks."""
        def numeric(v):
            return v[0] in SCALARS
        if op in ("+", "-"):
            if numeric(l) and numeric(r):
                # a literal takes its partner's scale (§8.1), and one of more places than a decimal holds would lose
                # them, where `rate + 0.005` was rounded on its write (the audit's values W5)
                for a, b in ((l, r), (r, l)):
                    if a[0] == "number" and b[0] == "decimal" and b[1] is not None and a[1] > b[1]:
                        self.fail(f"it {'adds' if op == '+' else 'subtracts'} a number of {a[1]} places and a decimal of "
                                  f"scale {b[1]}, which holds fewer")
                        return None
                # two literals keep the more places, and a literal beside an int its own, as the core aligns them, where
                # `i + 1.5` was an int and written to one (the differential battery)
                if l[0] == r[0] == "number":
                    return ("number", max(l[1], r[1]))
                for a, b in ((l, r), (r, l)):
                    if a[0] == "number" and b[0] == "int":
                        return ("int",) if a[1] == 0 else ("decimal", a[1])
                if l[0] == "number":
                    return r
                if r[0] == "number":
                    return l
                if l[0] != r[0]:
                    self.fail(f"it {'adds' if op == '+' else 'subtracts'} {type_text(l)} and {type_text(r)}, two types")
                    return None
                if l[0] == "decimal" and l[1] is not None and r[1] is not None and l[1] != r[1]:
                    self.fail(f"it {'adds' if op == '+' else 'subtracts'} decimals of two scales, {l[1]} and {r[1]}")
                    return None
                return l
            if l[0] == r[0] == "money":
                if l[1] != r[1]:
                    self.fail(f"it {'adds' if op == '+' else 'subtracts'} money of two currencies, {l[1]} and {r[1]}")
                    return None
                return l
            if l[0] == r[0] == "duration":
                return l
            if l[0] == "timestamp" and r[0] == "duration" or op == "+" and l[0] == "duration" and r[0] == "timestamp":
                return ("timestamp",)
            if op == "-" and l[0] == r[0] == "timestamp":
                return ("duration",)
            # a date less a date is a duration, and a date plus or minus a duration a date, as §8.3's Dates says and
            # the core runs; every other with a date is an error, where it typed as nothing (the values audit's K-1)
            if op == "-" and l[0] == r[0] == "date":
                return ("duration",)
            if l[0] == "date" and r[0] == "duration":
                return ("date",)
            self.fail(f"it {'adds' if op == '+' else 'subtracts'} {type_text(l)} and {type_text(r)}")
            return None
        # * and /
        if numeric(l) and numeric(r):
            if op == "/":
                # an int divided by a decimal is not run, its rounding to an int being unsaid (ADR-0202 decision 8)
                if (l[0] == "int" or l == ("number", 0)) and (r[0] == "decimal" or r[0] == "number" and r[1] > 0):
                    self.fail(f"it divides {type_text(l)} by {type_text(r)}, which the engine does not run: divide a "
                              "decimal, such as n * 1.0, for the fraction")
                    return None
                return l                        # / yields its left operand's type
            if l[0] == r[0] == "number":
                return ("number", l[1] + r[1])
            if all(v[0] == "int" or v[0] == "number" and v[1] == 0 for v in (l, r)):
                return ("int",)
            # decimal(p1,s1) * decimal(p2,s2) is of scale s1+s2, and an int's scale is 0
            sl, sr = (0 if v[0] == "int" else v[1] for v in (l, r))
            return ("decimal", None if sl is None or sr is None else sl + sr)
        if op == "/" and l[0] == r[0] and l[0] in ("money", "duration"):
            if l[0] == "money" and l[1] != r[1]:
                self.fail(f"it divides money of two currencies, {l[1]} and {r[1]}")
                return None
            return ("decimal", 6)               # a quotient of two like quantities
        if l[0] in ("money", "duration") and numeric(r):
            return l
        if op == "*" and r[0] in ("money", "duration") and numeric(l):
            return r
        if op == "/" and numeric(l) and r[0] in ("money", "duration"):
            self.fail(f"it divides {type_text(l)} by {type_text(r)}, and / takes its scalar on the right")
            return None
        if "timestamp" in (l[0], r[0]) or "date" in (l[0], r[0]):
            # neither is multiplied nor divided, where it typed as nothing and the core did not run it (K-1, K-2)
            self.fail(f"it {'multiplies' if op == '*' else 'divides'} {type_text(l)} by {type_text(r)}, and a time or a "
                      "date is neither multiplied nor divided")
            return None
        self.fail(f"it {'multiplies' if op == '*' else 'divides'} {type_text(l)} {'by' if op == '*' else 'by'} {type_text(r)}, "
                  "and takes a scalar on at least one side")
        return None


def typed_kind(ty):
    """A type the typing infers as check 17's kinds spell it, None where they spell none: a number literal is a number
    where it is whole and a decimal where it has places."""
    if ty[0] == "number":
        return ("number",) if ty[1] == 0 else ("decimal",)
    if ty[0] in ("decimal", "int"):
        return (ty[0],)
    if ty[0] in ("money", "reference", "enumeration"):
        return ty
    if ty[0] in ("state", "category"):
        return (ty[0],)                 # held by no attribute, so written to none (the differential battery)
    return (ty[0],) if ty[0] in SIMPLE_TYPES else None


def typed_write_error(typing, t, step, tn, x, env, kinds, enums):
    """What check 17 refuses of an assign by the type the typing infers, where value_kind tells none: a value of
    another kind than its target's, as a decimal computed into an int (the audit's values W2 and W3); and a decimal of
    more places than its target's that is no product or quotient, which §8.3 alone rounds (W5)."""
    loc = step.get("location")
    spec = (t.get("attributes") or {}).get(loc)
    try:
        tree = flowexpr.parse(" ".join(str(step.get("expr", "")).split()))
    except flowexpr.ParseError:
        return None
    target = attribute_type(spec, enums)
    typing.broken = []
    got = typing.of(tree, tn, x, dict(env or {}))
    if target is None or got is None:
        return None
    if (target[0] == "values") != (got[0] == "values"):
        # a set holds a set and nothing else does, where either was written and the core refused it at run time (the
        # differential battery)
        return f"{type_text(got)}, to {loc}, which holds {type_text(target)}: a write's value must fit its target"
    product = tree[0] == "bin" and tree[1] in ("*", "/")
    if target[0] == "decimal" and target[1] is not None and got[0] in ("decimal", "number") and got[1] is not None \
            and got[1] > target[1] and not product:
        return (f"a decimal of scale {got[1]}, to {loc}, which holds a decimal of scale {target[1]}: only a product or "
                "a quotient is rounded to its target")
    want, kind = written_kind(spec, enums), typed_kind(got)
    told = value_kind(tree, tn, t, x, enums)
    if told is not None and not (told == ("number",) and kind == ("decimal",)):
        return None                     # check 17's own, decided by the kind value_kind tells
    if want is not None and kind is not None and not fits(want, kind, kinds):
        return f"{type_text(got)}, to {loc}, which holds {' '.join(want)}: a write's value must fit its target"
    return None


def typed_element_error(typing, t, step, tn, x, env, kinds, enums):
    """What check 17 refuses of an add or a remove: a value of another kind than its set's elements (values K-10)."""
    loc = step.get("location")
    spec = (t.get("attributes") or {}).get(loc) or {}
    typ, ref = str(spec.get("type", "")), str(spec.get("reference", ""))
    element = {"type": typ[:-2]} if typ.endswith("[]") else {"reference": ref[:-2]} if ref.endswith("[]") else None
    if element is None:
        return None
    try:
        tree = flowexpr.parse(" ".join(str(step.get("expr", "")).split()))
    except flowexpr.ParseError:
        return None
    typing.broken = []
    got = typing.of(tree, tn, x, dict(env or {}))
    want, kind = written_kind(element, enums), typed_kind(got) if got is not None else None
    if want is not None and kind is not None and not fits(want, kind, kinds):
        return f"{type_text(got)}, to {loc}, whose elements are {' '.join(want)}: an element must fit its set"
    return None


def typed_loop_error(typing, step, tn, x, env):
    """A loop's array of a type the typing tells that is no collection, a set of objects or of values (values K-10)."""
    try:
        tree = flowexpr.parse(" ".join(str(step.get("array", "")).split()))
    except flowexpr.ParseError:
        return None
    typing.broken = []
    got = typing.of(tree, tn, x, dict(env or {}))
    if got is None or got[0] in ("objects", "values") or typing.value_set_element(tn, tree, dict(env or {})) is not None:
        return None
    return f"{type_text(got)}, which is no collection"


def expression_type_errors(doc, library):
    """Each expression of a module that does not type by declaration-syntax.md §8.3, where the types it combines can
    be told: a condition, an invariant, a derived attribute, a uniqueness condition, an effect's value, argument, loop
    or range, and a backfill; a condition a bool or an evaluator's verdict and an invariant a bool, besides (the model's
    check 24). Of a metric's expressions, which read rows, its value alone, by `metric_type_errors`."""
    kinds = dict(library or {})
    kinds.update(dict(types(doc)))
    enums = set(doc.get("enumerations") or {}) | {n for names in (doc.get("imports") or {}).values() for n in names if n not in kinds}
    categories = set(doc.get("categories") or []) | BUILT_IN_CATEGORIES
    typing = Typing(kinds, enums, categories, evaluator_names(doc))
    out = []

    def check(where, name, text, tn, x, env=None, want=None):
        try:
            tree = flowexpr.parse(" ".join(str(text).split()))
        except flowexpr.ParseError:
            return                              # step 2 refuses what does not parse
        typing.broken = []
        # a transition's guards and effects are check 58's, through its body; any other place is the typing's
        typing.flow_names = where[2] not in ("conditions", "transitions") if where[0] == "types" else True
        got = typing.of(tree, tn, x, dict(env or {}))
        if not typing.broken and want and got is not None and got[0] not in want:
            typing.broken.append(f"it is {type_text(got)}, not a bool")
        if typing.broken:
            out.append((where, "names", f"{name} does not type: {typing.broken[0]} (the model's check 24)"))

    for tn, t in types(doc):
        base = ("types", tn)
        for cn, c in (t.get("conditions") or {}).items():
            check(base + ("conditions", cn, "expression"), f"condition {cn}", c["expression"], tn, None, want=("bool", "verdict"))
        for i, spec in (t.get("invariants") or {}).items():
            if not inherited(t, "invariants", i):
                check(base + ("invariants", i, "expression"), f"invariant {i}", spec["expression"], tn, None, want=("bool",))
        for d, spec in (t.get("derived_attributes") or {}).items():
            if not inherited(t, "derived_attributes", d):
                check(base + ("derived_attributes", d, "expression"), f"derived attribute {d}", spec["expression"], tn, None)
        for a, spec in (t.get("attributes") or {}).items():
            if isinstance(spec.get("unique"), dict) and "where" in spec["unique"] and not inherited(t, "attributes", a):
                check(base + ("attributes", a, "unique", "where"), f"{tn}.{a}'s uniqueness condition", spec["unique"]["where"], tn, None, want=("bool",))
        for xn, x in (t.get("transitions") or {}).items():
            for st, kind, path, _depth in walk(x.get("effect")):
                where = base + ("transitions", xn, "effect") + path
                env = {n: (("reference", k) if k else None) for n, k in binders(kinds, tn, x, path).items()}
                for e in step_expressions(st, kind):
                    check(where, f"{tn}.{xn}'s effect", e, tn, x, env)
                if kind == "assign":
                    misfit = typed_write_error(typing, t, st["assign"], tn, x, env, kinds, enums)
                    if misfit:
                        out.append((where, "names", f"{tn}.{xn} writes {st['assign']['expr']}, {misfit} (the model's check 17)"))
                # an element added or removed fits its set, and a loop ranges over a collection, where each published
                # and the core refused or did not run it (the values audit's K-10)
                if kind in ("add", "remove"):
                    misfit = typed_element_error(typing, t, st[kind], tn, x, env, kinds, enums)
                    if misfit:
                        verb = "adds" if kind == "add" else "removes"
                        out.append((where, "names", f"{tn}.{xn} {verb} {st[kind]['expr']}, {misfit} (the model's check 17)"))
                if kind == "foreach" and st["foreach"].get("array") is not None:
                    loose = typed_loop_error(typing, st["foreach"], tn, x, env)
                    if loose:
                        out.append((where, "names", f"{tn}.{xn} loops over {st['foreach']['array']}, {loose} (the model's check 24)"))
        out += metric_type_errors(typing, tn, t, enums)
    for tn, given in ((doc.get("migration") or {}).get("backfill") or {}).items():
        for a, e in (given or {}).items():
            check(("migration", "backfill", tn, a), f"the backfill of {tn}.{a}", e, tn, None)
    return out


# the type a member of a row of a source other than objects holds, for a metric's aggregate over `item.member`
# (declaration-syntax.md §6.9): its times, its duration, its counts, its texts and its flags; a row's state, object,
# value and what it held are its type's own (`row_member_type`)
ROW_MEMBER_TYPES = {
    **dict.fromkeys(("entered_at", "left_at", "occurred_at", "recorded_at", "at", "piece_start", "piece_end"),
                    ("timestamp",)),
    "duration": ("duration",), "day": ("date",),
    **dict.fromkeys(("count", "declaration_version", "corrected_by"), ("int",)),
    "actor_id": ("identity",),
    **dict.fromkeys(("actor_kind", "entered_by_kind", "recorded_by_kind"), ("actor_kind",)),
    **dict.fromkeys(("transition", "verdict", "clause", "remedy", "name"), ("string",)),
    **dict.fromkeys(("legacy", "migrated", "completes", "returns", "asserted", "overrides", "imported", "redacted",
                     "unknown", "enforced", "flagged"), ("bool",)),
}
# what each aggregate over a metric's rows computes over: a number, an amount or a duration, and for an order or a
# middle a time or a date besides (`metric.rs`, measured 2026-10-15)
METRIC_AGGREGATE_READS = {
    "sum": ("int", "decimal", "number", "money", "duration"),
    "avg": ("int", "decimal", "number", "money", "duration"),
    **dict.fromkeys(("median", "percentile", "min", "max"),
                    ("int", "decimal", "number", "money", "duration", "timestamp", "date")),
}


def row_member_type(tn, t, source, member, enums):
    """The type `item.member` holds on a row of a source other than objects, None where it cannot be told."""
    if member in ("state", "from_state", "to_state", "subject_state"):
        return ("state", tn)
    if member in ("object", "subject"):
        return ("reference", tn)
    tracked = re.fullmatch(r"intervals\(([a-z][a-z0-9_]*)\)", source)
    if member == "value" and tracked:
        return ("state", tn) if tracked.group(1) == "state" else \
            attribute_type((t.get("attributes") or {}).get(tracked.group(1)), enums)
    return ROW_MEMBER_TYPES.get(member)


def metric_arith(op, a, b):
    """The type of `a op b` in a metric's value, as `metric.rs` `arith_kind` gives it, which places its rounding
    (ADR-0128); a sum or a difference of two types is a string saying so, which the core does not compute where its
    value is fractional (values M2)."""
    scalar = ("int", "decimal", "number")
    if a[0] == "number" and b[0] == "decimal":     # a decimal literal takes the scale of what it is combined with (§8.1)
        a = b
    elif b[0] == "number" and a[0] == "decimal":
        b = a
    if op in ("*", "/") and any(k[0] in ("timestamp", "date") for k in (a, b)):
        # neither is multiplied nor divided, as K-1 refuses elsewhere, where a metric's was not run or answered a date's
        # seconds (the differential battery)
        t = a if a[0] in ("timestamp", "date") else b
        verb = "multiplies" if op == "*" else "divides"
        return f"it {verb} {type_text(t)}, and a time or a date is neither multiplied nor divided"
    if op in ("+", "-"):
        if op == "-" and (a[0] == b[0] == "timestamp" or a[0] == b[0] == "date"):
            return ("duration",)
        if (a[0], b[0]) == ("date", "duration"):
            return ("date",)
        if (a[0], b[0]) == ("timestamp", "duration") or op == "+" and (a[0], b[0]) == ("duration", "timestamp"):
            return ("timestamp",)
        if a[0] == b[0] == "number":
            return ("number", max(a[1], b[1]))
        if a == b:
            return a
        verb = f"adds {type_text(b)} to" if op == "+" else f"subtracts {type_text(b)} from"
        return f"it {verb} {type_text(a)}, and a metric's value adds values of one type, a decimal of one scale (§8.3)"
    if op == "*":
        if a[0] in scalar and b[0] in scalar:
            if a[0] == b[0] == "int":
                return ("int",)
            pa, pb = (0 if k[0] == "int" else k[1] for k in (a, b))
            return ("decimal", None if pa is None or pb is None else pa + pb)
        return b if a[0] in scalar else a if b[0] in scalar else None
    if a == b and a[0] in ("duration", "money"):
        return ("decimal", 6)                       # a quotient of two like quantities
    if b[0] in scalar:
        return ("decimal", a[1]) if a[0] == "number" else a
    return None


def metric_type_errors(typing, tn, t, enums):
    """What a metric's value breaks that the core's metric evaluator does not compute, where the types it combines can
    be told: an aggregate over its rows of what it does not compute over, a text, a bool, a member, an identity or a
    state, and a sum of times; and a sum or a difference of two types, a decimal and an int or two scales, whose
    fractional value the core cannot place (the values audit's M2, the model's check 24)."""
    out = []
    for mn, m in (t.get("metrics") or {}).items():
        if inherited(t, "metrics", mn):
            continue
        try:
            tree = flowexpr.parse(" ".join(str(m.get("expression", "")).split()))
        except flowexpr.ParseError:
            continue                            # step 2 refuses what does not parse
        source, item = str(m.get("source", "")), m.get("item")
        problems = []

        def body(b):
            if source == "objects":
                typing.broken, typing.flow_names = [], True
                got = typing.of(b, tn, None, {item: ("reference", tn)})
                return None if typing.broken else got
            if b[0] == "path" and b[1] == ("name", item):
                return row_member_type(tn, t, source, b[2], enums)
            if b[0] == "call" and b[1] == ("path", ("name", item), "held") and len(b[2]) == 1 and b[2][0][0] == "name":
                n = b[2][0][1]
                return ("state", tn) if n == "state" else attribute_type((t.get("attributes") or {}).get(n), enums)
            return None

        def kind(e):
            if e[0] == "call" and e[1][0] == "name" and e[1][1] in ROW_AGGREGATES or e[0] == "agg" and e[3] is None:
                # the body is the last argument, after a percentile's p
                fn, over = (e[1][1], e[2][-1] if e[2] else None) if e[0] == "call" else (e[1], e[6])
                if fn == "count":
                    return ("int",)
                b = body(over) if over is not None else None
                if b is None:
                    return None
                if b[0] not in METRIC_AGGREGATE_READS.get(fn, ()):
                    reads = "a number, an amount or a duration" + ("" if fn in ("sum", "avg") else ", a time or a date")
                    problems.append(f"{fn} reads {type_text(b)}, and {fn} over a metric's rows reads {reads}")
                    return None
                return ("decimal", 6) if fn == "avg" and b[0] == "int" else b
            if e[0] == "num":
                return ("int",) if isinstance(e[1], int) else ("number", len(e[1].split(".", 1)[1]))
            if e[0] == "dur":
                return ("duration",)
            if e[0] == "money":
                return ("money", e[1])
            if e == ("name", "now"):
                return ("timestamp",)
            if e[0] == "neg":
                return kind(e[1])
            if e[0] == "bin" and e[1] in ("+", "-", "*", "/"):
                a, b = kind(e[2]), kind(e[3])
                if a is None or b is None:
                    return None
                got = metric_arith(e[1], a, b)
                if isinstance(got, str):
                    problems.append(got)
                    return None
                return got
            return None
        kind(tree)
        if problems:
            out.append((("types", tn, "metrics", mn, "expression"), "names",
                        f"metric {mn}'s value does not type: {problems[0]} (the model's check 24)"))
    return out


def argument_type_errors(doc, library):
    """Each argument a `call` or a `create` passes whose value does not fit the input it is given to, where both kinds
    can be told, as a write's value must fit its target: an input the transition reached declares, or an attribute
    input, which writes the attribute of its name (the model's check 2, its types; check 17's fit)."""
    kinds = dict(library or {})
    kinds.update(dict(types(doc)))
    enums = set(doc.get("enumerations") or {})
    # a value no literal or name tells is typed as an expression is, where an
    # int passed to a time or an amount to an int was taken and the core
    # refused the call (the differential battery, 2026-10-16)
    typing = Typing(kinds, enums | {n for names in (doc.get("imports") or {}).values() for n in names if n not in kinds},
                    set(doc.get("categories") or []) | BUILT_IN_CATEGORIES, evaluator_names(doc))
    out = []
    for tn, t in types(doc):
        for xn, x in (t.get("transitions") or {}).items():
            for st, kind, path, _depth in walk(x.get("effect")):
                if kind not in ("call", "create"):
                    continue
                body = st[kind]
                k = call_target(kinds, t, tn, x, body, binders(kinds, tn, x, path)) if kind == "call" else body.get("type")
                callee = (kinds.get(k) or {}) if k else {}
                reached = (callee.get("transitions") or {}).get(body.get("transition"))
                if not reached:
                    continue                    # a transition no type declares is the names checks'
                for j, e in (body.get("inputs") or {}).items():
                    spec = (reached.get("inputs") or {}).get(j)
                    if spec is None and j in set(reached.get("required_inputs") or []) | set(reached.get("optional_inputs") or []):
                        spec = (callee.get("attributes") or {}).get(j)
                    target = written_kind(spec, enums)
                    try:
                        tree = flowexpr.parse(" ".join(str(e).split()))
                    except flowexpr.ParseError:
                        continue                # step 2 refuses what does not parse
                    value = value_kind(tree, tn, t, x, enums)
                    if value is None:
                        typing.broken, typing.flow_names = [], False
                        env = {n: (("reference", c) if c else None) for n, c in binders(kinds, tn, x, path).items()}
                        got = typing.of(tree, tn, x, env)
                        value = typed_kind(got) if got is not None and not typing.broken else None
                        # an identity and a string the typing gives the benefit of the doubt, as it does in a
                        # comparison, where `actor.principal` passed to an identity input was refused (alike)
                        if value and target and {value[0], target[0]} == {"identity", "string"}:
                            value = None
                    if target and value and not fits(target, value, kinds):
                        out.append((("types", tn, "transitions", xn, "effect") + path, "names",
                                    f"{tn}.{xn} passes {e}, {' '.join(value)}, to {k}.{body['transition']}'s input '{j}', "
                                    f"which takes {' '.join(target)}: an argument must fit its input (the model's check 2)"))
    return out


def write_type_errors(doc, tn, t, library):
    """Each assign whose value does not fit its target, where both kinds can be told: an enumeration's member
    written to a string, money of one currency to another, a timestamp to an int; and a write of an actor's identity
    after its creation, which the record names actors by (the model's check 17, ADR-0205 decision 5)."""
    enums = set(doc.get("enumerations") or {})
    out = []
    identity = next((a for a, spec in (t.get("attributes") or {}).items() if isinstance(spec, dict) and "actor_kind" in spec), None)
    for xn, x in (t.get("transitions") or {}).items():
        if identity and x.get("kind") not in ("initial", "erasure"):
            by_input = identity in (x.get("required_inputs") or []) + (x.get("optional_inputs") or [])
            by_step = any(k in ("assign", "clear") and (identity in st[k] if k == "clear" else st[k].get("location") == identity)
                          for st, k, _p, _d in walk(x.get("effect")))
            if by_input or by_step:
                out.append((("types", tn, "transitions", xn), "names",
                            f"{tn}.{xn} writes {identity}, the identity {tn}'s actors are named by, which is written only "
                            "at their creation (the model's check 17)"))
        for st, kind, path, _d in walk(x.get("effect")):
            if kind == "foreach" and "range" in st["foreach"]:
                # a range counts from 1 to an int, an absent or non-positive one running no step (ADR-0146)
                try:
                    expr = flowexpr.parse(" ".join(str(st["foreach"]["range"]).split()))
                except flowexpr.ParseError:
                    expr = None
                got = range_kind(expr, tn, t, x, enums) if expr else None
                if got and got != ("int",):
                    out.append((("types", tn, "transitions", xn, "effect") + path + ("foreach", "range"), "names",
                                f"{tn}.{xn} loops over a range of {' '.join(got)}, and a range is an int"))
            if kind != "assign":
                continue
            target = written_kind((t.get("attributes") or {}).get(st["assign"]["location"]), enums)
            try:
                value = value_kind(flowexpr.parse(" ".join(str(st["assign"]["expr"]).split())), tn, t, x, enums)
            except flowexpr.ParseError:
                continue                        # step 2 refuses what does not parse
            if target and value and not fits(target, value, library):
                out.append((("types", tn, "transitions", xn, "effect") + path, "names",
                            f"{tn}.{xn} writes {st['assign']['expr']}, {' '.join(value)}, to {st['assign']['location']}, which "
                            f"holds {' '.join(target)}: a write's value must fit its target (the model's check 17)"))
    return out


def value_errors(doc, tn, t):
    """Every value an expression names is a state or an enumeration value. The
    text checker does not yet resolve enumeration values (check 19, in part)."""
    states = set(t.get("states") or {})
    enums = {n: set(m) for n, m in (doc.get("enumerations") or {}).items()}
    own = {k: set(v.get("states") or {}) for k, v in types(doc)}
    imported = {n for names in (doc.get("imports") or {}).values() for n in names}
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
        # a string literal's text names nothing, where "EU" read as a state (twelfth production review)
        text = STRING_LITERAL.sub(lambda m: '"' + " " * (len(m.group(0)) - 2) + '"', str(text))
        for m in re.finditer(r"\b([A-Z][A-Za-z0-9]*)\.([A-Z][A-Z0-9_]*)\b", text):
            owner, value = m.groups()
            known = enums.get(owner, own.get(owner))
            if known is None and owner not in imported:
                out.append((path, "names", f"{name} names {owner}.{value}, and no enumeration or type {owner} is "
                                           "declared in this module or imported"))
            if known is not None and value not in known:
                what = "enumeration" if owner in enums else "type"
                out.append((path, "names", f"{name} names {owner}.{value}, and the {what} {owner} has no {value}"))
        for m in re.finditer(r"(?<![.\w])[A-Z][A-Z0-9_]*(?![\w.])", text):
            if re.fullmatch(r"[A-Z]{3}", m.group(0)) and re.match(r"\s+-?\d", text[m.end():]):
                continue                    # a money literal's currency, as in USD 19.99 (declaration-syntax.md §8)
            if m.group(0) not in states:
                out.append((path, "names", f"{name} names {m.group(0)}, which is not a state of {tn}; "
                                           "write an enumeration value qualified, as <Enumeration>.<VALUE>"))
    return out


STRING_LITERAL = re.compile(r'"(?:[^"\\]|\\.)*"')   # as flowexpr.py's lexer reads one


KEYWORDS = {"and", "or", "not", "implies", "is", "null", "in", "where", "if", "then", "else",
            "true", "false", "distinct", "over", "last", "fresh"}
AGGREGATES = ("count", "sum", "min", "max", "all", "any", "none")
DURATION_UNITS = flowexpr.UNITS          # the parser's own, singular day and week among them (D494)
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
    members = stored | set(derived) | NOT_AN_ATTRIBUTE | OBJECT_MEMBERS | set(categories)
    # an invariant is checked after a write, where no request's inputs or event are (the model's check 5)
    exprs = [(("types", tn, sec, n, "expression"), n, c["expression"], members - (REQUEST_ONLY if sec == "invariants" else set()))
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
                                  f"{name} reads '{r}', which is withdrawn before 1.0.0, no store keeping it, and may return with a use (ADR-0210 decision 3)" if r in WITHDRAWN_MEMBERS else
                                  f"{name} reads '{r}', which {tn} does not declare")
                  for path, name, text, scope in exprs for r in sorted(reads(text) - scope)]


def stores(spec, library):
    """Whether a single reference end holds the value (declaration-syntax.md §3.3):
    it does unless its opposite is also single and it is not the end marked stored."""
    other = ((library.get(spec["reference"]) or {}).get("attributes") or {}).get(spec.get("opposite"))
    return other is None or str(other.get("reference", "")).endswith("[]") or bool(spec.get("stored"))


def observation_read_errors(tn, t):
    """No invariant reads an observation collection, directly or through a derived attribute: a recorded fact matters to
    the flow only through a guard, and a recording re-checks only its own kind's invariants (ADR-0099, ADR-0146)."""
    kinds = set(t.get("observations") or {})
    if not kinds:
        return []
    derived = {d: reads(spec["expression"]) & kinds for d, spec in (t.get("derived_attributes") or {}).items()}
    out = []
    for n, inv in (t.get("invariants") or {}).items():
        names = reads(inv["expression"])
        hit = (names & kinds) | {k for d in names & set(derived) for k in derived[d]}
        if hit:
            out.append((("types", tn, "invariants", n), "names", f"invariant {n} reads the observations {', '.join(sorted(hit))}; "
                                                              "a recorded fact matters to the flow only through a guard, so no "
                                                              "invariant reads one"))
    return out


BINDER_RESERVED = {"state", "inputs", "actor", "this", "now", "referrers", "this_event", "id", "open", "created_at",
                   "created_by_kind", "recorded_from", "declaration_version"}


def binder_errors(tn, t):
    """A name an effect binds, a `create`'s `result` or a loop's `item`, shares no name with a member of the type or a
    word an expression resolves first or every object has, and a nested loop reuses no name its loop binds, since a later
    step reading it would otherwise read one where the author meant the other (ADR-0146, ADR-0147)."""
    members = set(t.get("attributes") or {}) | set(t.get("derived_attributes") or {}) | set(t.get("observations") or {})
    out = []

    def scan(steps, path, bound, xn):
        for i, st in enumerate(steps or []):
            kind = next(k for k in STEP_KINDS if k in st)
            name = st["create"].get("result") if kind == "create" else st["foreach"].get("item") if kind == "foreach" else None
            where = ("types", tn, "transitions", xn, "effect") + path + (i,)
            if name in members:
                out.append((where, "names", f"{tn}.{xn} binds '{name}', which is a member of {tn}; a bound name shares no name with a member"))
            elif name in BINDER_RESERVED:
                out.append((where, "names", f"{tn}.{xn} binds '{name}', a word an expression resolves first or every object has (ADR-0147)"))
            elif kind == "foreach" and name in bound:
                out.append((where, "names", f"{tn}.{xn} binds '{name}' in a loop inside one that binds it already (ADR-0147)"))
            if kind == "foreach":
                scan(st["foreach"].get("steps"), path + (i, "foreach", "steps"), bound | {name}, xn)
    for xn, x in (t.get("transitions") or {}).items():
        scan(x.get("effect"), (), set(), xn)
    return out


def scan_errors(tn, t, library):
    """A type-scan invariant or guard, one whose aggregate ranges over every object of a type, reads only what the
    store holds indexed on that type: indexed or unique attributes, references, the state, the id and indexed derived
    attributes, since a scan over anything else reads every row (the model's check 7)."""
    out = []

    def readable(tt, m):
        spec = (tt.get("attributes") or {}).get(m)
        if spec is not None:
            return bool(spec.get("indexed") or spec.get("unique") or "reference" in spec)
        derived = (tt.get("derived_attributes") or {}).get(m)
        return m in ("state", "id") or bool(derived and derived.get("indexed")) or derived is None and spec is None

    def walk(e, found):
        if isinstance(e, list):
            for c in e:
                walk(c, found)
        elif isinstance(e, tuple) and e:
            if e[0] == "agg" and isinstance(e[4], tuple) and e[4][0] == "name" and e[4][1] in library \
                    and e[4][1] not in (t.get("attributes") or {}):
                var, scanned = e[3], library[e[4][1]]

                def reads(n):
                    if isinstance(n, list):
                        for c in n:
                            reads(c)
                    elif isinstance(n, tuple) and n:
                        if n[0] == "path" and n[1] == ("name", var) and not readable(scanned, n[2]):
                            found.add((e[4][1], f"{var}.{n[2]}"))
                        for c in n[1:]:
                            reads(c)
                reads([e[5], e[6]])
            for c in e[1:]:
                walk(c, found)
    rules = [("invariant", n, i["expression"], ("types", tn, "invariants", n)) for n, i in (t.get("invariants") or {}).items()]
    rules += [("condition", n, c["expression"], ("types", tn, "conditions", n)) for n, c in (t.get("conditions") or {}).items()]
    for kind, name, text, path in rules:
        try:
            tree = flowexpr.parse(" ".join(str(text).split()))
        except flowexpr.ParseError:
            continue                            # step 2 refuses what does not parse
        found = set()
        walk(tree, found)
        for scanned, read in sorted(found):
            out.append((path, "names", f"{kind} {name} scans every {scanned} and reads {read}, which is not indexed: a type scan "
                                       "reads only indexed or unique attributes, references, the state and indexed derived "
                                       "attributes (the model's check 7)"))
    return out


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


BUILT_IN_TYPES = ("string", "bool", "int", "decimal", "money", "timestamp", "duration", "identity", "file", "event", "date",
                  "actor_kind")
ACTOR_KINDS = ("human", "agent", "service")    # the values of actor_kind, written bare (ADR-0145)
DECIMAL = re.compile(r"^decimal\(\s*(\d+)\s*,\s*(\d+)\s*\)(\[\])?$")


def type_name_errors(doc):
    """Every type a module declares an attribute, a field, an input, a requirement or an argument of is built in, an
    enumeration the module declares, or a name it imports (flow-format.md §4.3): an unknown name was taken as some
    type nothing checked (D493)."""
    known = set(BUILT_IN_TYPES) | {"counter"} | set(doc.get("enumerations") or {})
    known |= {n for names in (doc.get("imports") or {}).values() for n in names}
    out = []

    def one(spec, path):
        typ = (spec or {}).get("type") if isinstance(spec, dict) else None
        if isinstance(typ, str):
            base = re.match(r"[A-Za-z_][A-Za-z0-9_]*", typ)
            if base and base.group(0) not in known:
                out.append((path + ("type",), "names", f"{path[-1]} is of the type {typ}, which is neither built in, nor an "
                                                       "enumeration this module declares, nor a name it imports"))

    def members(section, path):
        for n, spec in (section or {}).items():
            one(spec, path + (n,))
    for kind in ("types", "machines"):
        for tn, t in (doc.get(kind) or {}).items():
            base = (kind, tn)
            members(t.get("attributes"), base + ("attributes",))
            members(t.get("requires") if isinstance(t.get("requires"), dict) else {}, base + ("requires",))
            for o, ob in (t.get("observations") or {}).items():
                members(ob.get("attributes"), base + ("observations", o, "attributes"))
            for xn, x in (t.get("transitions") or {}).items():
                members(x.get("inputs"), base + ("transitions", xn, "inputs"))
    for e, ev in (doc.get("evaluators") or {}).items():
        for fn, f in (ev.get("functions") or {}).items():
            members(f.get("arguments"), ("evaluators", e, "functions", fn, "arguments"))
    return out


def decimal_errors(doc):
    """Every decimal a module declares, wherever it declares a type, holds 1 to 18 digits, its scale within them, so
    it is a 64-bit integer on both backends and any product with an int fits the core's 128 bits (ADR-0143)."""
    out = []

    def walk(node, path):
        if isinstance(node, dict):
            m = DECIMAL.match(str(node.get("type", ""))) if isinstance(node.get("type"), str) else None
            if m and not (1 <= int(m.group(1)) <= 18 and int(m.group(2)) <= int(m.group(1))):
                out.append((path + ("type",), "names", f"{path[-1]} is a decimal({m.group(1)},{m.group(2)}); a decimal holds 1 to 18 "
                                                       "digits, and its scale is within them"))
            for k, v in node.items():
                walk(v, path + (k,))
        elif isinstance(node, list):
            for i, v in enumerate(node):
                walk(v, path + (i,))
    walk(doc, ())
    return out


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
    # what "any one of the type's creations does not write" asks of a type with none, a mirror, nothing (ADR-0169)
    creates = any(x.get("kind") == "initial" for x in (t.get("transitions") or {}).values())
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
            elif creates and sc not in written:
                out.append((where + ("scope",), "names", f"{tn}.{a} is scoped by '{sc}', which not every creation of {tn} writes"))
            # a scope is kept with its sequence, which no erasure reaches (the audit's personal data S27)
            if ss is not None and ss.get("personal"):
                out.append((where + ("scope",), "names", f"{tn}.{a} is scoped by '{sc}', which is personal; a sequence "
                                                         "keeps its scope's value, which no erasure reaches"))
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
                elif ph.startswith("n:") and int(ph[2:]) > IDENTIFIER_WIDTH:
                    # a width a number can fill, where one of a million panicked as each object was made (tenth
                    # production review)
                    out.append((where + ("format",), "names", f"{tn}.{a}'s format has {{{ph}}}, wider than the "
                                                              f"{IDENTIFIER_WIDTH} digits a number is padded to at most"))
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
            if creates and hops[0] not in written:
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
            # an identifier is kept past an erasure, so it holds no personal value (the audit's personal data S28)
            if target.get("personal"):
                out.append((where + ("format",), "names", f"{tn}.{a}'s format reads {ph}, which is personal; an "
                                                          "identifier is kept past an erasure"))
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
    "intervals": {"object", "state", "entered_at", "left_at", "duration", "entered_by_kind", "declaration_version", "legacy", "held",
                  "migrated", "corrected_by"},
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


PIECE_MEMBERS = {"period", "piece_start", "piece_end"}
TRANSITION_ROWS = {"transitions", "attempts", "attempt_counts"}   # the sources whose rows name a transition


def transition_literals(e, item):
    """The strings a row's .transition is compared with, by ==, != or in, each of which names a transition of the
    metric's type (flow-format.md §4.9): a misspelt one would otherwise count nothing, silently."""
    row, found = ("path", ("name", item), "transition"), set()

    def walk(n):
        if isinstance(n, list):
            for c in n:
                walk(c)
        elif isinstance(n, tuple) and n:
            if n[0] == "bin" and n[1] in ("==", "!=") and row in (n[2], n[3]):
                other = n[3] if n[2] == row else n[2]
                if other[0] == "str":
                    found.add(other[1])
            if n[0] == "in" and n[1] == row and n[2][0] == "set":
                found.update(m[1] for m in n[2][1] if m[0] == "str")
            for c in n[1:]:
                walk(c)
    walk(e)
    return found


ROW_AGGREGATES = ("count", "sum", "min", "max", "avg", "median", "percentile")


def row_aggregate(e):
    """Whether an expression is an aggregate over a metric's rows: a count of them, or one called on its body."""
    return e[0] == "agg" and e[3] is None or e[0] == "call" and e[1][0] == "name" and e[1][1] in ROW_AGGREGATES


def holds_row_aggregate(e):
    """Whether an expression holds an aggregate over a metric's rows anywhere."""
    if isinstance(e, list):
        return any(holds_row_aggregate(c) for c in e)
    if not (isinstance(e, tuple) and e and isinstance(e[0], str)):
        return False
    return row_aggregate(e) or any(holds_row_aggregate(c) for c in e[1:])


def metric_value_form(e):
    """The first form a metric's value holds that the core does not compute over its rows, or None (the values M2)."""
    k = e[0]
    if row_aggregate(e):
        parts = [e[4], e[5], e[6]] if k == "agg" else e[2]
        return "an aggregate over its rows within another" if holds_row_aggregate(parts) else None
    if k in ("agg", "num", "dur", "money", "metric") or k == "name" and e[1] == "now":
        return None
    if k == "bin":
        if e[1] in ("+", "-", "*", "/"):
            return metric_value_form(e[2]) or metric_value_form(e[3])
        return "a connective" if e[1] in ("and", "or", "implies") else "a comparison"
    if k == "neg":
        return metric_value_form(e[1])
    return {"not": "a negated condition", "if": "a choice", "in": "a membership", "isnull": "a presence test",
            "changed": "a change test", "name": "a row's member outside an aggregate",
            "path": "a row's member outside an aggregate", "call": "a call of what is no aggregate"}.get(
        k, "a literal that is no number")


def formula_errors(tn, t, mn, m):
    """A metric in the metric language reads its rows through its item, and its
    flags read its value and its dimensions."""
    base = ("types", tn, "metrics", mn)
    source, item = m["source"], m["item"]
    members = row_members(t, source)
    out = []
    # a span split across the buckets it covers: each row a piece, with its period and bounds (ADR-0142)
    if m.get("split_by"):
        if not re.fullmatch(r"intervals(\([a-z][a-z0-9_]*\))?", source):
            out.append((base + ("split_by",), "names", f"metric {mn} splits {source} by {m['split_by']}, and only a type's spans, "
                                                       "intervals or intervals(<member>), are split"))
        elif members is not None:
            members = members | PIECE_MEMBERS
        if m.get("time_dimension"):
            out.append((base + ("time_dimension",), "names", f"metric {mn} splits its spans, so its time is its pieces', and it "
                                                             "declares no time_dimension; a window clips the pieces"))
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
    for p in re.findall(r"\bpercentile\(\s*([-+]?[0-9.]+)\s*,", " ".join(str(m["expression"]).split())):
        if not 0 < Fraction(p) <= 1:
            out.append((base + ("expression",), "names", f"metric {mn} takes the percentile {p}, and a percentile's p is in (0, 1] (ADR-0146)"))
    # count(<expression>) over the rows answers nothing, which the core did not run (D687)
    squashed = " ".join(str(m["expression"]).split())
    for at in re.finditer(r"\bcount\(", squashed):
        rest = squashed[at.end():].lstrip()
        if not (rest.startswith(")") or COUNTED.match(rest)):
            out.append((base + ("expression",), "names", f"metric {mn} counts an expression over its rows, which none answers: count() counts the rows, count(where …) those a condition selects, count(distinct …) the values they hold"))
            break
    # a metric's value combines aggregates over its rows, literals and now by + - * / and a negative, which is all the
    # core computes over them (declaration-syntax.md §6.9); any other form published and was not run (values M2)
    try:
        held = metric_value_form(flowexpr.parse(squashed))
    except flowexpr.ParseError:
        held = None                             # step 2 refuses what does not parse
    if held is not None:
        out.append((base + ("expression",), "names", f"metric {mn}'s value holds {held}, which a metric does not compute: it combines aggregates over its rows, literals and now by + - * / and a negative"))
    for key, text in parts:
        if text is None:
            continue
        # .held(<member>) names a member of the type whose spans or transitions the rows are, not a name of the
        # metric's own (declaration-syntax.md §6.9, D550)
        held = set(re.findall(rf"(?<![\w.]){item}\.held\(\s*([a-z][a-z0-9_]*)\s*\)", " ".join(str(text).split())))
        for r in sorted(reads(text) - {item, "now"} - held):
            out.append((base + key, "names", f"metric {mn} reads '{r}'; a metric reads its rows through '{item}'"))
        for r in sorted(held - set(t.get("attributes") or {}) - {"state"}):
            out.append((base + key, "names", f"metric {mn} reads {item}.held({r}), and {tn} has no member '{r}'"))
        for r in sorted(set(re.findall(rf"(?<![\w.]){item}\.([a-z][a-z0-9_]*)", " ".join(str(text).split())))):
            if members is not None and r not in members:
                out.append((base + key, "names", f"metric {mn} reads {item}.{r}, a label's note, which is personal and not readable"
                            if source == "labels" and r == "note" else f"metric {mn} reads {item}.{r}, and a row of {source} has no member '{r}'"))
    if source in TRANSITION_ROWS:
        names = set(t.get("transitions") or {})
        for key, text in parts:
            try:
                tree = flowexpr.parse(" ".join(str(text).split())) if text is not None else None
            except flowexpr.ParseError:
                continue                        # step 2 refuses what does not parse
            for lit in sorted(transition_literals(tree, item) - names):
                out.append((base + key, "names", f'metric {mn} compares {item}.transition with "{lit}", which is not a '
                                                 f"transition of {tn}"))
    # a flag is evaluated on the groups of the dimensions the metric declares, so it reads version or actor_kind only
    # where the metric names it (ADR-0147)
    dims = set(m.get("dimensions") or {})
    for f, c in (m.get("flag_when") or {}).items():
        for r in sorted(reads(c) - dims - {"value", "now"}):
            out.append((base + ("flag_when", f), "names", f"metric {mn}'s flag {f} reads '{r}', which is neither value nor a dimension "
                                                          "the metric declares"))
    for p in re.findall(r"\bpercentile\(\s*([^,()]+?)\s*,", " ".join(str(m["expression"]).split())):
        if not re.fullmatch(r"[-+]?[0-9.]+", p):
            out.append((base + ("expression",), "names", f"metric {mn} takes the percentile {p}, and a percentile's p is a literal (ADR-0147)"))
    return out


def invariant_names(doc, tn, t):
    """The invariants of a type: declared, generated for a state's required
    attributes, identity_taken for an actor's identity, which a migration may
    admit for actors an earlier release took (ADR-0195 decision 2), and, for a
    machine, those it requires of its binders."""
    names = set(t.get("invariants") or {})
    names |= {f"{s.lower()}_invariant" for s, v in (t.get("states") or {}).items() if (v or {}).get("required_attributes")}
    if any((a or {}).get("type") == "identity" and (a or {}).get("actor_kind")
           for a in (t.get("attributes") or {}).values()):
        names.add("identity_taken")
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
        if spec.get("personal") and not optional(spec) and not is_set_spec(spec):
            # a personal set is emptied by an erasure (ADR-0148)
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
            on, _, name = inv.rpartition(".")
            spec = ((dict(types(doc)).get(on) or library.get(on) or {}) if on else t).get("invariants") or {}
            if isinstance(spec.get(name), dict) and spec[name].get(TAKEN):
                out.append((base + ("may_admit",), "names", f"{tn}.{xn} may admit {inv}, which taken_by generates: "
                                                            f"who may take a transition is never admitted (D579)"))
                continue
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
            # the record of who took it, which taken_by generates, is no value it corrects, as for an assertion or an
            # erasure (ADR-0181 decision 1, ADR-0190 decision 5)
            if x.get("taken_by"):
                written.discard(taken_by_names(xn)[0])
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
    """A transition into a superseding state names its successor with one
    supersede step, at the top level of its effect, and only such a transition
    does; the successor is a required input or a name an earlier top-level
    create binds, never the object itself, so every request of it names exactly
    one; and a type with personal data that takes part in supersession declares
    an erasure, since the chain is erased through each of its members
    (declaration-syntax.md §6.1, §6.4; the model's checks 26, 49 and 60)."""
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
        if len(steps) > 1:
            out.append((where + ("effect",) + steps[1][1], "names", f"{tn}.{xn} names more than one successor, and an object has one"))
        bound = []
        for st, kind, path, depth in walk(x.get("effect")):
            if kind == "create" and st["create"].get("result") and depth == 0:
                bound.append(st["create"]["result"])   # a create in a loop binds once per element, or never
            if kind != "supersede":
                continue
            if depth:
                out.append((where + ("effect",) + path, "names", f"{tn}.{xn} supersedes inside a foreach, which may name no "
                                                               "successor or several"))
            who = " ".join(str(st["supersede"]).split())
            m = re.fullmatch(r"inputs\.([a-z][a-z0-9_]*)", who)
            if who == "this":
                out.append((where + ("effect",) + path, "names", f"{tn}.{xn} supersedes the object with itself"))
            elif m:
                spec = (x.get("inputs") or {}).get(m.group(1))
                if spec is not None and "reference" not in spec:   # an input it does not take is refused as any read of one is
                    out.append((where + ("effect",) + path, "names", f"{tn}.{xn} supersedes with inputs.{m.group(1)}, which is not an object"))
                elif spec is not None and spec.get("optional"):
                    out.append((where + ("effect",) + path, "names", f"{tn}.{xn} supersedes with inputs.{m.group(1)}, which is "
                                                                   "optional, so a request leaving it out would name no successor"))
            elif who not in bound:
                out.append((where + ("effect",) + path, "names", f"{tn}.{xn} supersedes with {who}, which is neither an input nor a "
                                                               "name a create at the top level of the effect binds (check 49)"))
    # the group a supersession joins is erased together, each member's erasure receiving the request's inputs, so the
    # types it joins require the same inputs of their erasures (ADR-0146)
    def erasure_needs(tt):
        # the inputs a group's erasure passes by name: each one's name, type and optionality (ADR-0147)
        ex = [x for x in ((tt or {}).get("transitions") or {}).values() if x["kind"] == "erasure"]
        return None if not ex else {i: (str((sp or {}).get("type", (sp or {}).get("reference"))), bool((sp or {}).get("optional")))
                                    for i, sp in (ex[0].get("inputs") or {}).items()}
    for xn, x in (t.get("transitions") or {}).items():
        for st, kind, path, _d in walk(x.get("effect")):
            if kind != "supersede":
                continue
            who = " ".join(str(st["supersede"]).split())
            m = re.fullmatch(r"inputs\.([a-z][a-z0-9_]*)", who)
            ref = str(((x.get("inputs") or {}).get(m.group(1)) or {}).get("reference", "")) if m else ""
            made = [c["create"]["type"] for c, ck, _p, _dd in walk(x.get("effect")) if ck == "create" and c["create"].get("result") == who]
            other = ref.rstrip("[]") or (made[0] if made else "")
            mine, theirs = erasure_needs(t), erasure_needs(library.get(other))
            if other and other != tn and mine is not None and theirs is not None and mine != theirs:
                out.append((("types", tn, "transitions", xn, "effect") + path, "names",
                            f"{tn}.{xn} supersedes with a {other}, whose erasure takes {', '.join(sorted(theirs)) or 'no input'} "
                            f"where {tn}'s takes {', '.join(sorted(mine)) or 'no input'}; a group's erasures receive one "
                            "request's inputs, so they require the same, by name, type and optionality"))
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
    erasures = [xn for xn, x in (t.get("transitions") or {}).items() if x["kind"] == "erasure"]
    if len(erasures) > 1:
        out.append((("types", tn, "transitions", erasures[1]), "names",
                    f"{tn} declares the erasures {', '.join(erasures)}; a type declares at most one, which a group's erasure takes (ADR-0147)"))
    for xn, x in (t.get("transitions") or {}).items():
        to = x.get("to") if isinstance(x.get("to"), list) else [x.get("to")]
        if x["kind"] == "assertion" and any((states.get(st) or {}).get("superseding") for st in to):
            out.append((("types", tn, "transitions", xn, "to"), "names",
                        f"{tn}.{xn} asserts a superseding state, which would name no successor; a merge is a declared transition "
                        "(the model's check 26, ADR-0147)"))
    personal = any(sp.get("personal") for sp in (t.get("attributes") or {}).values())
    # a personal input is erased from the events that carried it only by its object's erasure (DESIGN.md §8, ADR-0134)
    taken = sorted(f"{xn}.{i}" for xn, x in (t.get("transitions") or {}).items()
                   for i, sp in (x.get("inputs") or {}).items() if (sp or {}).get("personal"))
    erasable = any(x["kind"] == "erasure" for x in (t.get("transitions") or {}).values())
    # every type holding a personal value declares its erasure, so PRD D8 holds everywhere (ADR-0133, ADR-0134)
    # a machine has no objects of its own: each type that binds it is held to the rule (§4.10)
    if (personal or taken) and not erasable and not t.get("abstract") and tn not in (doc.get("machines") or {}):
        why = ", and takes part in supersession, through which a superseded chain is erased" if takes_part else ""
        what = "personal data" if personal else f"the personal input {taken[0]}"
        out.append((("types", tn), "names", f"{tn} holds {what} and declares no erasure{why} (check 60)"))
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
    function with its arguments, and is the whole condition or its negation
    (declaration-syntax.md §6.2, §5.1; the model's checks 19 and 24). A guard
    that asks an evaluator is consulted when its transition is requested, and
    by no read, so it carries no marking of when (ADR-0133)."""
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
                else:
                    out += evaluator_argument_errors(where + ("expression",), cn, text, ev, fn, f, tn, t, doc)
        if calls and not re.fullmatch(r"(not\s+)?[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*\(.*\)", text):
            out.append((where + ("expression",), "names", f"condition {cn} calls an evaluator inside a larger expression; a verdict is the whole condition or its negation (check 24)"))
    others = [(("types", tn, sec, n, "expression"), n, x["expression"]) for sec in ("invariants", "derived_attributes") for n, x in (t.get(sec) or {}).items()]
    others += [(("types", tn, "transitions", xn, "effect") + path, xn, e) for xn, x in (t.get("transitions") or {}).items()
               for st, kind, path, _d in walk(x.get("effect")) for e in step_expressions(st, kind)]
    for path, name, text in others:
        for ev, fn, _n in evaluator_calls(text, names):
            out.append((path, "names", f"{name} calls {ev}.{fn}; only a guard asks an evaluator"))
    return out


def evaluator_argument_errors(where, cn, text, ev, fn, f, tn, t, doc):
    """Each argument of a condition's evaluator call whose value does not fit the argument the function declares, in
    order, where both kinds can be told (the model's check 2, its types); the call is the condition or its negation,
    check 24 refusing it elsewhere."""
    try:
        tree = flowexpr.parse(text)
    except flowexpr.ParseError:
        return []                               # step 2 refuses what does not parse
    if tree[0] == "not":
        tree = tree[1]
    if tree[0] != "call" or tree[1] != ("path", ("name", ev), fn):
        return []
    enums = set(doc.get("enumerations") or {})
    out = []
    for (a, spec), arg in zip((f.get("arguments") or {}).items(), tree[2]):
        target, value = written_kind(spec, enums), value_kind(arg, tn, t, {}, enums)
        if target and value and not fits(target, value, {}):
            out.append((where, "names", f"condition {cn} passes {' '.join(value)} to {ev}.{fn}'s argument '{a}', which "
                                        f"takes {' '.join(target)}: an argument must fit its input (the model's check 2)"))
    return out


AGGREGATE_CALL = re.compile(r"\b(count|sum|min|max|avg|median|percentile)\s*\(")
# the types whose values have no members (declaration-syntax.md §8.1)
NO_MEMBERS = {"number", "int", "decimal", "money", "duration", "timestamp", "date", "bool", "string", "identity",
              "actor_kind", "category", "enumeration"}
# the functions an expression may call, and the member functions (declaration-syntax.md §8.3)
FUNCTIONS = {"length", "date", "entered_at", "time_in"}
# the functions a metric alone calls, which check 56 refuses elsewhere
METRIC_ONLY_FUNCTIONS = {"avg", "median", "percentile", "day", "week", "month", "quarter", "year"}
MEMBER_FUNCTIONS = {"intervals", "entered_at", "time_in", "held"}
# what a metric's count( may begin with besides `)`: a condition, distinct values, or a binder over a row's collection
COUNTED = re.compile(r"(where\b|distinct\b|[a-z_][a-z0-9_]*\s+in\b)")


def dimensions_of(m):
    """The dimensions a metric has: those it declares or groups by, and the two every metric has."""
    if "source" in m:
        return set(m.get("dimensions") or {}) | STANDARD_DIMENSIONS
    return set(m.get("group_by") or []) | STANDARD_DIMENSIONS


def combined_errors(doc):
    """A module's metrics share one namespace, as the model's metric
    declarations do; a combined metric's inputs are metrics of the module's
    types or combined metrics above it, so it never reaches itself; it groups
    by their dimensions, and its value combines their values by name, with no
    rows to aggregate (declaration-syntax.md §6.9, ADR-0098; the model's
    checks 33 and 56)."""
    out, owner = [], {}
    for tn, t in types(doc):
        for mn in (t.get("metrics") or {}):
            if mn in owner:
                out.append((("types", tn, "metrics", mn), "names", f"the metric {mn} is declared by both {owner[mn]} and {tn}; a module's metrics share one namespace"))
            owner.setdefault(mn, tn)
    kinds = dict(types(doc))
    combined = doc.get("metrics") or {}
    names = list(combined)
    for i, (mn, m) in enumerate(combined.items()):
        base = ("metrics", mn)
        if mn in owner:
            out.append((base, "names", f"the metric {mn} is also declared by {owner[mn]}; a module's metrics share one namespace"))
        dims, held = set(STANDARD_DIMENSIONS), []
        for alias, ref in m["input_metrics"].items():
            where = base + ("input_metrics", alias)
            if "." in ref:
                tn, name = ref.split(".")
                target = ((kinds.get(tn) or {}).get("metrics") or {}).get(name)
                if tn not in kinds:
                    out.append((where, "names", f"metric {mn} combines {ref}, and the module declares no type {tn}"))
                elif target is None:
                    out.append((where, "names", f"metric {mn} combines {ref}, which is not a metric of {tn}"))
                else:
                    dims |= dimensions_of(target)
                    held.append(dimensions_of(target) | STANDARD_DIMENSIONS)
            elif ref in names[i:]:
                out.append((where, "order", f"metric {mn} combines {ref}, which is itself or declared after it; a combined metric combines those above it"))
            elif ref not in combined:
                out.append((where, "names", f"metric {mn} combines {ref}, which is neither a combined metric of this module nor written as <Type>.<metric>"))
            else:
                dims |= dimensions_of(combined[ref])
                held.append(dimensions_of(combined[ref]) | STANDARD_DIMENSIONS)
        for d in m.get("group_by") or []:
            if d not in dims:
                out.append((base + ("group_by",), "names", f"metric {mn} groups by '{d}', which none of the metrics it combines has as a dimension"))
        wanted = set(m.get("group_by") or [])
        if held and wanted <= dims and not any(wanted <= h for h in held):
            out.append((base + ("group_by",), "names", f"metric {mn} groups by {', '.join(sorted(wanted))}, which no one of the metrics it "
                                                       "combines has all of; one input has every dimension the combination names "
                                                       "(the model's check 56, ADR-0147)"))
        for r in sorted(reads(m["expression"]) - set(m["input_metrics"])):
            out.append((base + ("expression",), "names", f"metric {mn} reads '{r}', which is not one of the metrics it combines"))
        if AGGREGATE_CALL.search(str(m["expression"])):
            out.append((base + ("expression",), "names", f"metric {mn} aggregates, and a combined metric has no rows: its value combines the values of the metrics it names"))
        for f, c in (m.get("flag_when") or {}).items():
            for r in sorted(reads(c) - set(m.get("group_by") or []) - {"value", "now"}):
                out.append((base + ("flag_when", f), "names", f"metric {mn}'s flag {f} reads '{r}', which is neither value nor a dimension "
                                                              "the metric names"))
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
    if m.get("measure") == "median_time_in_state" and m["state"] not in (t.get("states") or {}):
        out.append((base + ("state",), "names", f"metric {mn} measures the state {m['state']}, which {tn} does not declare"))
    for d in m.get("group_by") or []:
        if d == "actor" and m.get("measure") != "transition_count":
            out.append((base + ("group_by",), "names", f"metric {mn} groups by actor, which only a transition_count has"))
        elif d not in ("actor", "month", "week") and d not in (t.get("attributes") or {}):
            out.append((base + ("group_by",), "names",
                        f"metric {mn} groups by '{d}', which is not month, week, actor or an attribute of {tn}"))
    return out


def unwritten(v, doc):
    """An optional attribute of an owned type that nothing writes, so it is always absent: no transition of the type, a
    subtype or its machine takes it as an input or sets it in an effect, and no migration backfills it; one with a
    default, a counter, an identifier, an external identifier or a relationship's end, which its far end writes, is
    written (D557)."""
    machines = set(v.get("machines") or {})
    tps = dict(types(v))
    backfilled = {(tn, a) for tn, given in ((doc.get("migration") or {}).get("backfill") or {}).items()
                  for a in (given or {})}

    def family(tn):
        out, grew = {tn}, True
        while grew:
            more = {n for n, t in tps.items() if t.get("extends") in out} - out
            grew, out = bool(more), out | more
        return out

    out = []
    for tn, t in tps.items():
        if tn in machines or t.get("mirror"):
            continue
        written = set()
        for fn in family(tn):
            for x in ((tps.get(fn) or {}).get("transitions") or {}).values():
                written |= set(x.get("required_inputs") or []) | set(x.get("optional_inputs") or [])
                for st, kind, _path, _depth in walk(x.get("effect")):
                    if kind in ("assign", "add", "remove"):
                        written.add(st[kind].get("location"))
        for a, spec in (t.get("attributes") or {}).items():
            spec = spec or {}
            if inherited(t, "attributes", a) or not spec.get("optional") or a in written or (tn, a) in backfilled:
                continue
            if any(k in spec for k in ("default", "identifier", "external", "opposite")) or spec.get("type") == "counter":
                continue
            out.append((("types", tn, "attributes", a), "unwritten",
                        f"{tn}.{a} is optional and nothing writes it: no transition takes it as an input or sets it, "
                        "and no migration backfills it, so it is always absent"))
    return out


def notices(doc):
    out = []
    for tn, t in types(doc):
        conds = t.get("conditions") or {}
        transitions = t.get("transitions") or {}
        erasable = any((x or {}).get("kind") == "erasure" for x in transitions.values())
        for xn, x in transitions.items():
            takes = x.get("required_inputs") or x.get("optional_inputs") or x.get("inputs")
            for g, mode in (x.get("guards") or {}).items():
                path = ("types", tn, "transitions", xn, "guards", g)
                if mode == "audit":
                    out.append((path, "audit", f"{g} is audited: its failures are recorded and it refuses nothing"))
                elif mode == "warn":
                    out.append((path, "warn", f"{g} warns: a failure is reported with the result and refuses nothing"))
                # "supply it" (DESIGN.md §5.5) has nothing to name on a transition that takes no input
                if not takes and (conds.get(g) or {}).get("remedy") == "self_serviceable":
                    out.append((path, "self_serviceable", f"{g}'s remedy is self_serviceable, 'supply it', "
                                f"and {xn} takes no input a caller could supply"))
            # a correction's reason is kept with its event, which an erasure of the object does not reach unless the
            # reason is personal: free text there keeps what a clerk typed (production review F7)
            reason = (x.get("inputs") or {}).get("reason") or {}
            if erasable and x.get("corrects") and reason.get("type") == "string" and not reason.get("personal"):
                out.append((("types", tn, "transitions", xn, "inputs", "reason"), "reason_kept",
                            f"{tn}.{xn}'s reason is free text, which its event keeps through an erasure of the {tn}: mark it personal, which keeps it off the record, or make it an enumeration, as an override's is"))
    return out


def hiding(c):
    """A character that shows nothing or reorders what follows: a bidirectional control, a zero-width or invisible
    mark, by its code point, as the core's checker reads one."""
    o = ord(c)
    return (o in (0x00AD, 0x061C, 0x180E, 0xFEFF) or 0x200B <= o <= 0x200F or 0x202A <= o <= 0x202E
            or 0x2060 <= o <= 0x2064 or 0x2066 <= o <= 0x2069)


def reach_bound(t, a):
    """The most of a type's set its invariants hold it to, `count(x in a) <= n` or `count(x in a) < n + 1`, the least
    where several do; none where none does."""
    most = None
    for spec in (t.get("invariants") or {}).values():
        text = " ".join(str((spec or {}).get("expression") or "").split())
        m = re.fullmatch(r"count\(\w+ in (?:this\.)?" + re.escape(a) + r"\) (<=|<) (\d+)", text)
        if m:
            n = int(m.group(2)) - (1 if m.group(1) == "<" else 0)
            most = n if most is None else min(most, n)
    return most


def reaches(doc):
    """An erasure's reach a limit may stop: a foreach of an erasure, or a cascade an erasure sets off, declaring a
    limit over a set of its type that no invariant of the type holds at or below it, so past the limit the erasure is
    refused over_limit and cannot be carried out (fifth production review ERASE-LIMIT, ADR-0190 decision 7). So is any
    other transition's: past its limit an object holding more can never take it, a claim of 201 lines never withdrawn,
    paid or erased (fourteenth production review N8, ADR-0201)."""
    out = []
    for tn, t in types(doc):
        if t.get("abstract"):
            continue
        attrs = t.get("attributes") or {}
        transitions = t.get("transitions") or {}
        erasures = [xn for xn, x in transitions.items() if isinstance(x, dict) and x.get("kind") == "erasure"]
        for a, spec in attrs.items():
            for c in ((spec or {}).get("cascade") or []) if isinstance(spec, dict) else []:
                n = c.get("limit") if isinstance(c, dict) else None
                on = [str(x) for x in (c.get("on") or [])] if isinstance(c, dict) else []
                erasing = [e for e in on if e in erasures]
                others = [e for e in on if e not in erasures]
                most = reach_bound(t, a)
                if not isinstance(n, int) or isinstance(n, bool) or (most is not None and most <= n):
                    continue
                if erasing:
                    found = (("types", tn, "attributes", a, "cascade"), "unbounded_reach",
                             f"{tn}.{a}'s cascade on {', '.join(erasing)} takes at most {n} of it, and no invariant of "
                             f"{tn} holds it to {n}: past {n}, the erasure is refused over_limit and cannot be carried "
                             f"out; declare one such as count(x in {a}) <= {n} (ADR-0190)")
                    if found not in out:
                        out.append(found)
                if others:
                    found = (("types", tn, "attributes", a, "cascade"), "unbounded_reach",
                             f"{tn}.{a}'s cascade on {', '.join(others)} takes at most {n} of it, and no invariant of "
                             f"{tn} holds it to {n}: past {n}, each is refused over_limit, and an object holding more "
                             f"can never take it; declare one such as count(x in {a}) <= {n} (ADR-0201)")
                    if found not in out:
                        out.append(found)
        for xn, x in transitions.items():
            if not isinstance(x, dict):
                continue
            erasure = x.get("kind") == "erasure"
            for st, k, _p, _d in walk(x.get("effect")):
                if k != "foreach":
                    continue
                f = st["foreach"] if isinstance(st.get("foreach"), dict) else {}
                # a loop with `where` bounds what it keeps, an author's rule on another transition, where one picking a
                # robot's open line was noticed as if it bounded every line (ADR-0201 decision 9)
                if not erasure and f.get("where") is not None:
                    continue
                m = re.fullmatch(r"(?:this\.)?(\w+)", str(f.get("array") or "").strip())
                n = f.get("limit")
                if not m or m.group(1) not in attrs or not isinstance(n, int) or isinstance(n, bool):
                    continue
                a = m.group(1)
                most = reach_bound(t, a)
                if most is None or most > n:
                    found = (("types", tn, "transitions", xn), "unbounded_reach",
                             f"{tn}.{xn} takes at most {n} of {a}, and no invariant of {tn} holds it to {n}: past {n}, "
                             + ("the erasure is refused over_limit and cannot be carried out" if erasure else
                                "it is refused over_limit, and an object holding more can never take it")
                             + f"; declare one such as count(x in {a}) <= {n} ("
                             + ("ADR-0190" if erasure else "ADR-0201") + ")")
                    if found not in out:
                        out.append(found)
    return out


def vetoes(doc, library):
    """An invariant that can only refuse other objects' changes: one a type declares whose every state is final, so
    no transition of its own can break it once it is created, reading through a reference or its referrers, so it is
    checked again whenever what it reads changes, and refuses that change (flow-format.md §4.6, ADR-0176 decision 5).
    A fact the record is to hold as it stood is copied onto it when it is made (write-a-flow.md, Patterns)."""
    out = []
    kinds = dict(library or {})
    kinds.update(dict(types(doc)))
    for tn, t in types(doc):
        states = t.get("states") or {}
        if not states or not all((s or {}).get("final") for s in states.values()):
            continue
        for i, spec in (t.get("invariants") or {}).items():
            if inherited(t, "invariants", i):
                continue
            text = (spec or {}).get("expression", "")
            if invariant_form(kinds, tn, text) != "traversal":
                continue
            try:
                tree = flowexpr.parse(str(text))
            except flowexpr.ParseError:
                continue
            walker = OptionalReads(kinds, tn)
            walker.through_tests = True
            walker.walk(tree, {})
            through = sorted(n for u, n in walker.traversed if u == tn)
            what = (", ".join(through) if through
                    else "its referrers" if any(m == "referrers" for _u, m in walker.members)
                    else "the derived attributes it reads")
            out.append((("types", tn, "invariants", i), "veto",
                        f"{tn}.{i} reads through {what}, and every state of {tn} is final, so no transition of its "
                        f"own can break it: it can only refuse a change to what it reads there. To hold a fact as it "
                        f"stood when the {tn} was made, copy it onto the {tn} then (write-a-flow.md, Patterns, ADR-0176)"))
    return out


class OptionalReads:
    """What an expression reads that may be absent, and what it tests with `is null` or `is not null`, over the
    types of the modules checked (declaration-syntax.md §8.2, ADR-0103 §6). An optional read is an optional attribute
    without a default, an optional input, or a derived attribute that can itself be unknown so, read through any path
    whose type is known; an attribute every state the transition may be taken in requires is present there."""

    BUILT_IN = {"id", "state", "open", "created_at", "created_by_kind", "category", "type", "recorded_at",
                "occurred_at", "subject", "intervals", "transitions"}

    def __init__(self, library, tn, x=None, derived_seen=()):
        self.lib, self.tn, self.x = library, tn, x or {}
        self.t = library.get(tn) or {}
        self.derived_seen = derived_seen
        states = self.t.get("states") or {}
        froms = sources(self.x) if self.x.get("kind") in ("external", "internal") else []
        required = [set((states.get(st) or {}).get("required_attributes") or []) for st in froms]
        self.present = set.intersection(*required) if required else set()
        self.reads, self.tested = {}, set()
        self.derived = set()        # each derived attribute read, as (type, name), through any path whose type is known
        self.members = set()        # each member read, as (type, name), through any path whose type is known
        self.through_tests = False  # whether `is null` walks what it tests, which reads it as any expression does
        self.traversed = set()      # each reference read through, to a member or as a collection, as (type, name)
        self.last = None            # the reference the walk last read, as (type, name), if its last read was one
        self.scanned = set()        # each type ranged over by name, a type scan

    def untested(self, tree):
        """Each optional read the expression never tests, with why it may be absent."""
        self.walk(tree, {})
        return {k: why for k, why in self.reads.items() if k not in self.tested}

    @staticmethod
    def key(e):
        """The text a read and a test are matched by; `this.x` and `x` are one read."""
        if e[0] == "name":
            return e[1]
        if e[0] == "path":
            k = OptionalReads.key(e[1])
            return None if k is None else (e[2] if k == "this" else f"{k}.{e[2]}")
        return None

    def attribute(self, tn, m, key, own):
        """The type of a member read, noting it if it may be absent."""
        self.members.add((tn, m))
        t = self.lib.get(tn) or {}
        spec = (t.get("attributes") or {}).get(m)
        if spec is not None:
            spec = spec or {}
            ref = str(spec.get("reference", ""))
            if spec.get("optional") and "default" not in spec and not (own and m in self.present):
                self.reads.setdefault(key, f"{tn}.{m} is optional")
            self.last = (tn, m) if ref else None
            return ref or None
        if m in (t.get("derived_attributes") or {}):
            self.derived.add((tn, m))
        if m in (t.get("derived_attributes") or {}) and (tn, m) not in self.derived_seen:
            inner = OptionalReads(self.lib, tn, derived_seen=self.derived_seen + ((tn, m),))
            why = inner.untested(flowexpr.parse(t["derived_attributes"][m]["expression"]))
            if why:
                self.reads.setdefault(key, f"{tn}.{m} can be unknown through {', '.join(sorted(why))}")
        return None

    def walk(self, e, env):
        """The type an expression's value has, where it names objects, as `T` or `T[]`; noting its optional reads."""
        k = e[0]
        if k == "isnull":
            key = self.key(e[1])
            if key is not None:
                self.tested.add(key)
            if self.through_tests:
                self.walk(e[1], env)
            return None
        if k == "name":
            n = e[1]
            if n in env:
                return env[n]
            if n == "this":
                return self.tn
            if n == "inputs":
                return "<inputs>"
            observed = (self.t.get("observations") or {}).get(n)
            if observed:
                return f"{observed['kind']}[]"      # a collection by its bare name, as through `this`
            return self.attribute(self.tn, n, n, own=True)
        if k == "path":
            if e[1] == ("name", "inputs"):
                m, x = e[2], self.x
                declared = (x.get("inputs") or {}).get(m)
                if declared is not None:
                    declared = declared or {}
                    if declared.get("optional") and "default" not in declared:
                        self.reads.setdefault(f"inputs.{m}", f"the input {m} is optional")
                    return str(declared.get("reference", "")) or None
                spec = ((self.t.get("attributes") or {}).get(m)) or {}
                if m in (x.get("optional_inputs") or []) and "default" not in spec:
                    self.reads.setdefault(f"inputs.{m}", f"the input {m} is optional")
                return str(spec.get("reference", "")) or None
            self.last = None
            base = self.walk(e[1], env)
            if self.last is not None and e[1][0] in ("name", "path"):
                self.traversed.add(self.last)
            if base is not None and not base.endswith("[]") and base != "<inputs>" and e[2] in self.BUILT_IN:
                self.members.add((base, e[2]))      # what every object has, read through a path
            if base is None or base.endswith("[]") or base == "<inputs>" or e[2] in self.BUILT_IN:
                observed = ((self.lib.get(base or "") or {}).get("observations") or {}).get(e[2])
                return f"{observed['kind']}[]" if observed else None
            observed = ((self.lib.get(base) or {}).get("observations") or {}).get(e[2])
            if observed:
                return f"{observed['kind']}[]"
            key = self.key(e)
            return self.attribute(base, e[2], key, own=e[1] == ("name", "this")) if key is not None else None
        if k == "agg":
            _, _kind, _distinct, var, coll, where, body = e
            if var is None:
                for part in (where, body):
                    if part is not None:
                        self.walk(part, env)
                return None
            self.last = None
            ct = self.walk(coll, env)
            if self.last is not None and coll[0] in ("name", "path"):
                self.traversed.add(self.last)
            if ct is None and coll[0] == "name" and coll[1] in self.lib and coll[1] not in env:
                ct = coll[1] + "[]"                 # a type scan: every object of the type
                self.scanned.add(coll[1])
            inner = dict(env, **{var: ct[:-2] if ct and ct.endswith("[]") else None})
            for part in (where, body):
                if part is not None:
                    self.walk(part, inner)
            return None
        if k == "in":
            self.walk(e[1], env)
            if not (e[2][0] == "name" and e[2][1] not in env):
                self.walk(e[2], env)
            return None
        if k == "metric":
            for _d, x in e[2]:
                self.walk(x, env)
            return None
        if k == "changed":
            self.walk(e[2], env)
            return None
        if k == "call":
            for a in e[2]:
                if a[0] != "name":
                    self.walk(a, env)
            if e[1][0] == "path":
                self.walk(e[1][1], env)
            return None
        for part in e[1:]:
            if isinstance(part, tuple) and part and isinstance(part[0], str):
                self.walk(part, env)
            elif isinstance(part, list):
                for q in part:
                    if isinstance(q, tuple):
                        self.walk(q, env)
        return None


def supplied_reads(tree):
    """The attributes an expression reads of its own object, bare or through `this`, and the inputs it reads."""
    attrs, inputs = set(), set()

    def walk(e, bound):
        k = e[0]
        if k == "name":
            if e[1] not in bound:
                attrs.add(e[1])
        elif k == "path" and e[1] == ("name", "inputs"):
            inputs.add(e[2])
        elif k == "path" and e[1] == ("name", "this"):
            attrs.add(e[2])
        elif k == "agg":
            _, _kind, _distinct, var, coll, where, body = e
            if coll is not None:
                walk(coll, bound)
            for part in (where, body):
                if part is not None:
                    walk(part, bound | ({var} if var else set()))
        else:
            for part in e[1:]:
                if isinstance(part, tuple) and part and isinstance(part[0], str):
                    walk(part, bound)
                elif isinstance(part, list):
                    for q in part:
                        if isinstance(q, tuple):
                            walk(q, bound)
    walk(tree, set())
    return attrs, inputs


def befores(v):
    """Each guard that reads an attribute its own transition takes as an input, bare or through `this`, without also
    reading that input: a guard runs before the transition's writes (DESIGN.md §6), so it sees the value the attribute
    had before the request. One that reads both compares them, as `points_changed` does, and is not reported."""
    out = []
    for tn, t in types(v):
        conds = t.get("conditions") or {}
        for xn, x in (t.get("transitions") or {}).items():
            supplied = set(x.get("required_inputs") or []) | set(x.get("optional_inputs") or [])
            for g in (x.get("guards") or {}):
                expr = (conds.get(g) or {}).get("expression")
                if expr is None:
                    continue
                attrs, inputs = supplied_reads(flowexpr.parse(expr))
                for a in sorted((attrs & supplied) - inputs):
                    had = ("the object it creates has none yet" if x["kind"] == "initial"
                           else f"it reads the value {a} had before the request")
                    out.append((("types", tn, "transitions", xn, "guards", g), "before",
                                f"{g} reads {a}, which {xn} takes as an input: a guard runs before the transition writes it, "
                                f"so {had}; inputs.{a} is the value supplied"))
    return out


def absents(v, library):
    """Each assignment whose value can be absent through an optional read it never tests, which refuses its request
    `value_in_range` when it is (ADR-0201 decision 7, D655), other than a plain read, a name or a path, assigned to an
    optional attribute, which clears it (ADR-0209 decision 5): read as a guard is for `unknown`, an attribute every
    state the transition may be taken in requires being present, and a read a deny guard of the transition tests
    being decided before the effect runs; an unsupplied optional input assigned as it is, which the decision exempts,
    is not noticed."""
    out = []
    for tn, t in types(v):
        conds = t.get("conditions") or {}
        for xn, x in (t.get("transitions") or {}).items():
            guarded = set()
            for g, mode in (x.get("guards") or {}).items():
                expr = (conds.get(g) or {}).get("expression")
                if mode == "deny" and expr is not None:
                    r = OptionalReads(library, tn, x)
                    r.walk(flowexpr.parse(expr), {})
                    guarded |= r.tested

            def steps(sts, path, env):
                for i, st in enumerate(sts or []):
                    kind = next(k for k in STEP_KINDS if k in st)
                    if kind == "foreach":
                        spec = st["foreach"]
                        at = OptionalReads(library, tn, x).walk(flowexpr.parse(spec["array"]), env) if "array" in spec else None
                        inner = dict(env, **{spec["item"]: at[:-2] if at and at.endswith("[]") else None})
                        steps(spec.get("steps"), path + (i, "foreach", "steps"), inner)
                        continue
                    if kind != "assign" or (st["assign"] or {}).get("expr") is None:
                        continue
                    tree = flowexpr.parse(str(st["assign"]["expr"]))
                    target = (((library.get(tn) or {}).get("attributes") or {}).get(st["assign"].get("location"))) or {}
                    if tree[0] in ("name", "path") and target.get("optional"):
                        continue                    # a plain read of an absent value clears it (ADR-0209 decision 5)
                    r = OptionalReads(library, tn, x)
                    r.walk(tree, env)
                    # an optional input assigned as it is leaves the attribute as it was, which the decision exempts,
                    # and inside a larger expression of a step it is already fatal
                    why = {k: w for k, w in r.reads.items()
                           if k not in r.tested and k not in guarded and not k.startswith("inputs.")}
                    if why:
                        out.append((path + (i, "assign"), "absent",
                                    f"{xn} assigns {st['assign'].get('location')} a value that can be absent, through "
                                    f"{'; '.join(f'{k} ({w})' for k, w in sorted(why.items()))}, which neither it nor "
                                    "a deny guard tests with `is null`; an absent value refuses the request value_in_range"))
            steps(x.get("effect"), ("types", tn, "transitions", xn, "effect"), {})
    return out


def unknowns(v, library):
    """Each guard, and each filter of a loop or a metric, that can be unknown through an optional it never tests
    (declaration-syntax.md §8.2, ADR-0103 §6): a guard that is unknown refuses, and a filter leaves out an element
    it cannot decide, whatever the author meant."""
    out = []

    def said(why):
        return "; ".join(f"{k} ({w})" for k, w in sorted(why.items()))
    for tn, t in types(v):
        conds = t.get("conditions") or {}
        for xn, x in (t.get("transitions") or {}).items():
            for g in (x.get("guards") or {}):
                expr = (conds.get(g) or {}).get("expression")
                if expr is None:
                    continue
                why = OptionalReads(library, tn, x).untested(flowexpr.parse(expr))
                if why:
                    # an unknown guard fails as a false one does: a deny guard refuses, an audit one is recorded and a
                    # warn one raised, and neither of those refuses (flow-format.md §4.8, §5)
                    does = {"deny": "a deny guard that is unknown refuses",
                            "audit": "an audit guard that is unknown is recorded as failing, and refuses nothing",
                            "warn": "a warn guard that is unknown is raised, and refuses nothing"}.get(x["guards"][g], "")
                    out.append((("types", tn, "transitions", xn, "guards", g), "unknown",
                                f"{g} can be unknown when {xn} is requested, through {said(why)}, which it never tests with "
                                f"`is null`; {does}"))

            def loops(steps, path, env):
                for i, st in enumerate(steps or []):
                    kind = next(iter(st))
                    if kind != "foreach":
                        continue
                    spec = st["foreach"]
                    reader = OptionalReads(library, tn, x)
                    at = reader.walk(flowexpr.parse(spec["array"]), env) if "array" in spec else None
                    inner = dict(env, **{spec["item"]: at[:-2] if at and at.endswith("[]") else None})
                    if spec.get("where"):
                        r = OptionalReads(library, tn, x)
                        r.walk(flowexpr.parse(spec["where"]), inner)
                        why = {k: w for k, w in r.reads.items() if k not in r.tested}
                        if why:
                            out.append((path + (i, "foreach", "where"), "unknown",
                                        f"the loop over {spec.get('array', spec.get('range'))} in {xn} filters by what can be "
                                        f"unknown, through {said(why)}, which it never tests; an element it cannot decide is left out"))
                    loops(spec.get("steps"), path + (i, "foreach", "steps"), inner)
            loops(x.get("effect"), ("types", tn, "transitions", xn, "effect"), {})
        for mn, m in (t.get("metrics") or {}).items():
            if not m.get("filter"):
                continue
            src = m.get("source")
            item = tn if src == "objects" else (((t.get("observations") or {}).get(src) or {}).get("kind"))
            if item is None:
                continue                          # a row of flow data, whose members are the model's own
            r = OptionalReads(library, tn)
            r.walk(flowexpr.parse(m["filter"]), {m["item"]: item})
            why = {k: w for k, w in r.reads.items() if k not in r.tested}
            if why:
                out.append((("types", tn, "metrics", mn, "filter"), "unknown",
                            f"{mn}'s filter can be unknown through {said(why)}, which it never tests; a row it cannot decide "
                            "is left out"))
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
            body.append((f"require {g}: {' '.join(c['expression'].split())}{mark} because {c['remedy']}", X + ("guards", g)))
    if x.get("may_admit"):
        body.append(("may admit " + ", ".join(x["may_admit"]), X + ("may_admit",)))
    if x.get("corrects"):
        body.append(("corrects " + ", ".join(x["corrects"]), X + ("corrects",)))
    body += effect_lines(x.get("effect"), X + ("effect",), "")
    if not body:
        return [(f"  {head} {{ }}", X)]
    return [(f"  {head} {{", X)] + [("    " + b, p) for b, p in body] + [("  }", X)]


def to_text(doc, library=None):
    """The module in the text language, and for each line the YAML path it came from; `library` holds the types
    of the modules checked before it, whose ends decide which of this module's references are tracked."""
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
        if t.get("summary"):
            emit("  summary  " + ", ".join(t["summary"]), T + ("summary",))
        for sn, v in (t.get("states") or {}).items():
            emit(f"  state {sn} category {v['category']}" + (" terminal" if v.get("final") else "")
                 + (" superseding" if v.get("superseding") else ""), T + ("states", sn))
        for an, spec in (t.get("attributes") or {}).items():
            for line in attribute_line(an, spec, module=dict(types(doc))).split("\n"):
                emit(("  " + line) if not line.startswith("  ") else line, T + ("attributes", an))
        family = dict(types(doc))
        for an, ip in (t.get("inherited_parts") or {}).items():
            spec = next(((family[b].get("attributes") or {}).get(an) for b in bases(tn, family)
                         if an in (family[b].get("attributes") or {})), None)
            if spec is None:
                continue
            one = lambda e: " ".join(str(e).split())
            for i, c in enumerate(ip.get("cascade") or []):
                on = c["on"][0] if len(c["on"]) == 1 else "{ " + ", ".join(c["on"]) + " }"
                args = ", ".join(f"{k} := {one(v)}" for k, v in (c.get("inputs") or {}).items())
                emit(f"  cascade {an} on {on} to {spec['reference'].rstrip('[]')}.{c['transition']}" + (f"({args})" if args else "")
                     + f" limit {c['limit']}", T + ("inherited_parts", an, "cascade", i))
            sv = ip.get("survives")
            if sv is True:
                emit(f"  survives {an}", T + ("inherited_parts", an, "survives"))
            elif sv:
                emit(f"  survives {an} on {{ {', '.join(sv)} }}", T + ("inherited_parts", an, "survives"))
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
            tail += metric_text(tn, merged, mn, m, dict(library or {}, **dict(types(doc))))
    for line, path in tail:
        emit(line, path)
    one = lambda e: " ".join(str(e).split())
    for mn, m in (doc.get("metrics") or {}).items():
        M = ("metrics", mn)
        emit(f"# {m['description']}", M)
        emit(f"metric {mn} version 1 {{", M)
        emit("  combine   " + ", ".join(f"{a} = {r.split('.')[-1]}" for a, r in m["input_metrics"].items()), M + ("input_metrics",))
        if m.get("group_by"):
            emit("  by        " + ", ".join(m["group_by"]), M + ("group_by",))
        emit(f"  value     {one(m['expression'])}", M + ("expression",))
        for f, c in (m.get("flag_when") or {}).items():
            emit(f"  flag      {f} when {one(c)}", M + ("flag_when", f))
        emit("}", M)
    mig = doc.get("migration") or {}
    for tn, maps in (mig.get("removed_states") or {}).items():
        for old, new in maps.items():
            emit(f"removed state  {tn}.{old} -> {new}", ("migration", "removed_states", tn, old))
    for en, maps in (mig.get("removed_members") or {}).items():
        for old, new in maps.items():
            emit(f"removed member {en}.{old} -> {new}", ("migration", "removed_members", en, old))
    for tn, maps in (mig.get("renamed_attributes") or {}).items():
        for old, new in maps.items():
            emit(f"renamed attr   {tn}.{old} -> {new}", ("migration", "renamed_attributes", tn, old))
    for tn, maps in (mig.get("backfill") or {}).items():
        for a, expr in maps.items():
            emit(f"backfill       {tn}.{a} := {' '.join(str(expr).split())}", ("migration", "backfill", tn, a))
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


def tracked_attributes(t, library):
    """The attributes whose every value is timed (ADR-0083, declaration-syntax.md §6.9): an enumeration attribute,
    and a singular reference that stores its value, one with no opposite, one marked stored, or one whose opposite
    end is a set, a part's end back to its whole among them."""
    out = set()
    for a, spec in (t.get("attributes") or {}).items():
        spec = spec or {}
        typ, ref = str(spec.get("type", "")), str(spec.get("reference", ""))
        if typ[:1].isupper() and not typ.endswith("[]"):
            out.add(a)
        elif ref and not ref.endswith("[]"):
            far = ((library.get(ref) or {}).get("attributes") or {}).get(spec.get("opposite") or "") or {}
            if "opposite" not in spec or spec.get("stored") or str(far.get("reference", "")).endswith("[]"):
                out.add(a)
    return out


def metric_text(tn, t, mn, m, library=None):
    M = ("types", tn, "metrics", mn)
    out = [(f"# {m['description']}", M), (f"metric {mn} version 1 {{", M)]
    if "source" in m:
        one = lambda e: " ".join(str(e).split())
        src = m["source"]
        ob = (t.get("observations") or {}).get(src)
        rows = tn if src == "objects" else ob["kind"] if ob else f"{tn}.{src}"
        out.append((f"  from      {m['item']} in {rows}" + (f" split by {m['split_by']}" if m.get("split_by") else "")
                    + (f" where {one(m['filter'])}" if m.get("filter") else ""), M + ("source",)))
        if m.get("dimensions"):
            out.append(("  by        " + ", ".join(f"{d} = {one(e)}" for d, e in m["dimensions"].items()), M + ("dimensions",)))
        if m.get("time_dimension"):
            out.append((f"  window on {one(m['time_dimension'])}", M + ("time_dimension",)))
        out.append((f"  value     {one(m['expression'])}", M + ("expression",)))
        out += [(f"  flag      {f} when {one(c)}", M + ("flag_when", f)) for f, c in (m.get("flag_when") or {}).items()]
        out.append(("}", M))
        return out
    tracked = tracked_attributes(t, library or {})       # read as held when the span began (ADR-0139 decision 1)
    if m["measure"] == "median_time_in_state":
        v, time, value = "i", "i.entered_at", "median(i.duration)"
        out.append((f"  from      i in {tn}.intervals where i.state == {tn}.{m['state']}", M + ("state",)))
    else:
        # the transition's events by name, with the standard metrics' exclusions (ADR-0146)
        v, time, value = "t", "t.occurred_at", "count()"
        cond = f't.transition == "{m["transition"]}" and not t.imported and not t.migrated and not t.redacted'
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


def literal(v):
    """A boolean or a number written as YAML's own, as the expression language writes that literal."""
    if isinstance(v, bool):
        return "true" if v else "false"
    return str(v) if isinstance(v, (int, float)) else v


def literals(doc):
    """Give each expression written as a YAML boolean or number its literal (§5).

    The schema says where an expression goes, so the walk follows it down the
    description, through references and alternatives, and converts a value
    only where an expression is expected and no alternative takes the value
    as YAML's own, as `final: true` and `limit: 50` are."""
    schema = json.loads((FORMAT / "flow.schema.json").read_text())
    defs, expression = schema["definitions"], "#/definitions/expression"

    def branches(sch):
        if "$ref" in sch:
            return [sch] if sch["$ref"] == expression else branches(defs[sch["$ref"].rsplit("/", 1)[1]])
        out = [sch]
        for k in ("anyOf", "oneOf", "allOf"):
            for b in sch.get(k, []):
                out += branches(b)
        for k in ("then", "else"):
            if k in sch:
                out += branches(sch[k])
        return out

    def native(sch):
        t = sch.get("type")
        types = set(t) if isinstance(t, list) else {t}
        return bool(types & {"boolean", "integer", "number"}) or isinstance(sch.get("const"), (bool, int, float)) \
            or any(isinstance(x, (bool, int, float)) for x in sch.get("enum", []))

    def convert(value, schemas):
        return (isinstance(value, (bool, int, float)) and any(c.get("$ref") == expression for c in schemas)
                and not any(native(c) for c in schemas))

    def walk(node, schemas):
        if isinstance(node, dict):
            for k in list(node):
                inner = []
                for sch in schemas:
                    if k in (sch.get("properties") or {}):
                        inner += branches(sch["properties"][k])
                    elif isinstance(sch.get("additionalProperties"), dict):
                        inner += branches(sch["additionalProperties"])
                if convert(node[k], inner):
                    node[k] = literal(node[k])
                else:
                    walk(node[k], inner)
        elif isinstance(node, list):
            inner = [b for sch in schemas if isinstance(sch.get("items"), dict) for b in branches(sch["items"])]
            for i, v in enumerate(node):
                if convert(v, inner):
                    node[i] = literal(v)
                else:
                    walk(v, inner)
    walk(doc, branches(schema))


# ── examples (flow-format.md §11, ADR-0136) ─────────────────────────────────
EXAMPLE_CLOCK = "2026-01-01T00:00:00Z"   # the instant every example's clock starts at
EXAMPLES_ORDER = ["examples_for", "setups", "examples"]
# the guards the engine generates, which an example may expect to refuse (DESIGN.md §6)
GENERATED_GUARDS = {"actor_known", "actor_active", "occurred_within", "whole_open", "not_erased", "subject_open", "corrects_current",
                    "subject_owned", "successor_not_superseded", "value_in_range", "proposal_pending"}
DATE = re.compile(r"^\d{4}-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])$")   # a date as an example writes it (ADR-0145)
BUILT_IN_MEMBERS = {"id", "open", "created_at", "created_by_kind", "recorded_from", "declaration_version"}
EXAMPLE_OUTCOMES = []      # each example step 6 ran, with its outcome, for the summary a file check prints
TIMESTAMP = re.compile(r"^(now( [+-] [0-9]+ (s|min|h|days?|weeks?))?|\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2})?Z)$")


def examples_schema_errors(doc):
    schema = json.loads((FORMAT / "examples.schema.json").read_text())
    out = []
    for e in sorted(jsonschema.Draft7Validator(schema).iter_errors(doc), key=lambda e: list(e.absolute_path)):
        if e.validator == "oneOf" and isinstance(e.instance, dict) and "transition" in e.instance:
            out.append((tuple(e.absolute_path), "schema", "a request names the `type` of a creation, with `as` for the alias it "
                                                          "gives the object, or the `object` it acts on by its alias, not both"))
        else:
            out.append((tuple(e.absolute_path), "schema", e.message))
    return out


def is_a(tn, want, kinds):
    return tn == want or want in bases(tn, kinds)


def marks_actor(tn, kinds):
    """A type whose objects are actors: one of its attributes, its own or inherited, is marked with an actor kind."""
    return any("actor_kind" in (spec or {}) for spec in ((kinds.get(tn) or {}).get("attributes") or {}).values())


def example_errors(doc, kinds, modules):
    """What an examples file names resolves against its module and the modules it imports: each setup and example
    starts from a named setup, each step requests a transition the flow declares, of an object an earlier step made,
    with inputs the transition takes, as an actor the store holds, and each expectation names a verdict the request
    can give, with the clause and remedy the rules declare (flow-format.md §11, ADR-0136)."""
    mod = doc["examples_for"]
    if mod not in modules:
        return [(("examples_for",), "names", f"examples for {mod}, which is not among the modules checked")], {}, set()
    # an observation kind is recorded by a request of its generated creation, `record`, taking each field and the
    # subject, and optionally the observation it corrects (declaration-syntax.md §6.8)
    kinds = dict(kinds)
    for m in modules.values():
        for tn, t in (m.get("types") or {}).items():
            for ob in (t.get("observations") or {}).values():
                fields = ob.get("attributes") or {}
                # its occurred and recorded times are read, as an expectation's values, and never given as inputs
                kinds[ob["kind"]] = {
                    "attributes": dict(fields, subject={"reference": tn}, corrects={"reference": ob["kind"], "optional": True},
                                       occurred_at={"type": "timestamp"}, recorded_at={"type": "timestamp"}),
                    "states": {"RECORDED": {"final": True}},
                    "invariants": ob.get("invariants") or {},
                    "transitions": {"record": {"kind": "initial", "to": "RECORDED",
                                               "required_inputs": ["subject"] + [a for a, f in fields.items() if not optional(f)],
                                               "optional_inputs": ["corrects"] + [a for a, f in fields.items() if optional(f)]}}}
                if any((f or {}).get("personal") for f in fields.values()):
                    # a kind with a personal field has a generated erasure, `forget`, a step may request (ADR-0147)
                    kinds[ob["kind"]]["transitions"]["forget"] = {"kind": "erasure", "inputs": {"reason": {"type": "string"}}}
    enums = {n: set(v) for m in modules.values() for n, v in (m.get("enumerations") or {}).items()}
    functions = {f"{e}.{fn}" for m in modules.values() for e, ev in (m.get("evaluators") or {}).items()
                 for fn in (ev.get("functions") or {})}
    machines = {n for m in modules.values() for n in (m.get("machines") or {})}
    sequences = {n for m in modules.values() for n in (m.get("sequences") or {})}
    setups = doc.get("setups") or {}
    # each transition an example requests under test, by the examples that do, and each a setup's or an example's
    # step takes or an expectation's `cascaded` names, which the coverage notice tells apart (ADR-0173 decision 6)
    out, envs, requested, exercised = [], {}, {}, set()
    keys = [k for k in doc if k in EXAMPLES_ORDER]
    if keys != [k for k in EXAMPLES_ORDER if k in doc]:
        out.append(((keys[0],), "order", f"an examples file declares, in this order, {', '.join(EXAMPLES_ORDER)}"))
    order = list(setups)

    def stubs_errors(stubs, path):
        return [(path + (k,), "names", f"stubs {k}, which no module checked declares as an evaluator's function")
                for k in (stubs or {}) if k not in functions]

    def value_errors(v, spec, env, path, name):
        ref = (spec or {}).get("reference")
        typ = (spec or {}).get("type")
        if ref:
            want, many = ref.rstrip("[]"), ref.endswith("[]")
            got = v if many and isinstance(v, list) else [v]
            if many and not isinstance(v, list):
                return [(path, "names", f"'{name}' is a set of {want}, given as a list of aliases")]
            return [(path, "names", f"'{name}' is a reference to {want}, and '{a}' is not an alias an earlier step gave one")
                    for a in got if not (isinstance(a, str) and a in env and is_a(env[a], want, kinds))]
        if typ in enums:
            if not (isinstance(v, str) and "." in v and v.split(".", 1)[0] == typ and v.split(".", 1)[1] in enums[typ]):
                return [(path, "names", f"'{name}' is a {typ}, written qualified as {typ}.<MEMBER> with a member it declares")]
        elif typ == "bool" and not isinstance(v, bool):
            return [(path, "names", f"'{name}' is a bool")]
        elif typ == "int" and (isinstance(v, bool) or not isinstance(v, int)):
            return [(path, "names", f"'{name}' is an int")]
        elif typ == "timestamp" and not (isinstance(v, str) and TIMESTAMP.match(v)):
            return [(path, "names", f"'{name}' is a timestamp, written in UTC as 2026-01-01T09:00Z, or as now, now + <duration> "
                                    "or now - <duration>")]
        elif typ == "date" and not (isinstance(v, str) and DATE.match(v)):
            return [(path, "names", f"'{name}' is a date, written as 2026-03-01")]
        elif typ == "actor_kind" and v not in ACTOR_KINDS:
            return [(path, "names", f"'{name}' is an actor_kind, written bare as human, agent or service")]
        return []

    def request_errors(r, env, path, applies):
        """A request's names; `applies` for a setup's step, which must apply."""
        errs = []
        if "type" in r:
            tn = r["type"]
            if tn not in kinds or tn in machines:
                return [(path + ("type",), "names", f"creates a {tn}, which no module checked declares as a type")], None, None, None
            if kinds[tn].get("abstract"):
                return [(path + ("type",), "names", f"creates a {tn}, which is abstract and has no objects")], None, None, None
        else:
            if r["object"] not in env:
                return [(path + ("object",), "names", f"acts on '{r['object']}', which no earlier step gave as an alias")], None, None, None
            tn = env[r["object"]]
        x = (kinds[tn].get("transitions") or {}).get(r["transition"])
        if x is None:
            return [(path + ("transition",), "names", f"requests {tn}.{r['transition']}, which {tn} does not declare")], None, None, None
        if ("type" in r) != (x["kind"] == "initial"):
            errs.append((path + ("transition",), "names", f"{tn}.{r['transition']} is {'a creation, requested with the type' if x['kind'] == 'initial' else 'requested on an object, named by its alias'}"))
        if applies and x.get("only_via"):
            errs.append((path + ("transition",), "names", f"a setup's step must apply, and {tn}.{r['transition']} is only via "
                                                          f"{', '.join(x['only_via'])}, so a direct request is not requestable"))
        if "occurred_at" in r:
            errs += value_errors(r["occurred_at"], {"type": "timestamp"}, env, path + ("occurred_at",), "occurred_at")
        actor = r.get("actor", "operator")
        if actor != "operator" and not (actor in env and marks_actor(env[actor], kinds)):
            errs.append((path + ("actor",), "names", f"names the actor '{actor}', which is neither `operator` nor an alias of an "
                                                     "object whose type marks an actor identity"))
        attrs = kinds[tn].get("attributes") or {}
        own = x.get("inputs") or {}
        builtin = {"to", "admits"} if x["kind"] == "assertion" else set()
        given = r.get("inputs") or {}
        for k, v in given.items():
            if k not in taken(x) | builtin:
                errs.append((path + ("inputs", k), "names", f"{tn}.{r['transition']} takes no input '{k}'"))
            elif k == "to":
                if v not in targets(x):
                    errs.append((path + ("inputs", k), "names", f"'{v}' is not a state {tn}.{r['transition']} may put an object in"))
            elif k != "admits":
                errs += value_errors(v, own.get(k) if k in own else attrs.get(k), env, path + ("inputs", k), k)
        left = required_input_names(x) - set(given)
        # a required input of an optional or defaulted attribute left out is refused by its generated guard, after the
        # generated guards before it; any other is invalid input (flow-format.md §4.8, §6)
        unprovided = [i for i in x.get("required_inputs", []) if i in left and i not in own
                      and ((attrs.get(i) or {}).get("optional") or "default" in (attrs.get(i) or {}))]
        missing = sorted(i for i in left - set(unprovided) if "default" not in (own.get(i) or {}))
        missing += ["to"] if x["kind"] == "assertion" and "to" not in given else []
        return errs, (tn, x), missing, unprovided

    def created_types(tn, x, seen):
        """The types a transition's effect creates at any depth: through its loops, the creations it makes and the
        transitions it calls, each of which may create in turn (§11.3, ADR-0173 decision 6)."""
        out, t = set(), kinds.get(tn) or {}
        for st, kind, path, _depth in walk(x.get("effect")):
            if kind == "create":
                nxt = st["create"]["type"], st["create"]["transition"]
                out.add(nxt[0])
            elif kind == "call":
                k = call_target(kinds, t, tn, x, st["call"], binders(kinds, tn, x, path))
                if k is None:
                    continue
                nxt = k, st["call"]["transition"]
            else:
                continue
            if nxt not in seen:
                seen.add(nxt)
                out |= created_types(nxt[0], ((kinds.get(nxt[0]) or {}).get("transitions") or {}).get(nxt[1]) or {}, seen)
        return out

    def raise_errors(rs, env, at):
        """A sequence an import raises is one the modules declare, given the scope it was minted in where it is
        scoped, and none where it is not (§11.3, publish-and-import.md §4)."""
        seq = rs["sequence"]
        if seq not in sequences:
            return [(at + ("sequence",), "names", f"raises the sequence {seq}, which no module checked declares")]
        scoped = sorted((tn, a, spec["identifier"]["scope"]) for tn, t in kinds.items()
                        for a, spec in (t.get("attributes") or {}).items()
                        if (spec.get("identifier") or {}).get("sequence") == seq and spec["identifier"].get("scope"))
        if "scope" in rs and not scoped:
            return [(at + ("scope",), "names", f"raises {seq} in a scope, and {seq} numbers one series, with no scope")]
        if "scope" not in rs and scoped:
            tn, a, scope = scoped[0]
            return [(at, "names", f"raises {seq} with no scope, and {tn}.{a} numbers it in each value of {scope}")]
        if scoped:
            tn, _a, scope = scoped[0]
            return value_errors(rs["scope"], (kinds[tn].get("attributes") or {}).get(scope), env, at + ("scope",), "scope")
        return []

    def decision_errors(d, env, path):
        """A decision names a proposal an earlier step filed, as an actor the store holds (DESIGN.md §9)."""
        errs = []
        if not str(env.get(d["proposal"], "")).startswith("Proposal:"):
            errs.append((path + ("proposal",), "names", f"decides '{d['proposal']}', which no earlier step filed as a proposal"))
        actor = d.get("actor", "operator")
        if actor != "operator" and not (actor in env and marks_actor(env[actor], kinds)):
            errs.append((path + ("actor",), "names", f"names the actor '{actor}', which is neither `operator` nor an alias of an "
                                                     "object whose type marks an actor identity"))
        return errs

    def steps_errors(steps, env, path):
        errs = []
        for i, st in enumerate(steps or []):
            here = path + (i,)
            if env.get(PUBLISHED):
                continue        # under the version a publish installed, which this check does not read (§11.3)
            if "publish" in st:
                actor = st["publish"].get("actor", "operator")
                if actor != "operator" and not (actor in env and marks_actor(env[actor], kinds)):
                    errs.append((here + ("publish", "actor"), "names", f"names the actor '{actor}', which is neither "
                                                                       "`operator` nor an alias of an object whose type "
                                                                       "marks an actor identity"))
                env[PUBLISHED] = True
                continue
            if "propose" in st:
                # a request filed for approval instead of executed, on a transition marked proposable (§11.3)
                r = st["propose"]
                e, found, _missing, _unprovided = request_errors(r, env, here + ("propose",), applies=False)
                errs += e
                if found and not found[1].get("proposable"):
                    errs.append((here + ("propose", "transition"), "names",
                                 f"{found[0]}.{r['transition']} is not proposable, so no proposal of it can be filed"))
                if r["proposal"] in env or r["proposal"] == "operator":
                    errs.append((here + ("propose", "proposal"), "names", f"'{r['proposal']}' is already an alias"))
                if found:
                    env[r["proposal"]] = f"Proposal:{found[0]}.{r['transition']}"
                continue
            verb = next((v for v in ("approve", "reject", "withdraw") if v in st), None)
            if verb:
                errs += decision_errors(st[verb], env, here + (verb,))
                filed = str(env.get(st[verb]["proposal"], ""))
                if verb == "approve" and filed.startswith("Proposal:"):
                    exercised.add(tuple(filed[len("Proposal:"):].split(".", 1)))
                continue
            if "request" in st:
                r = st["request"]
                e, found, missing, unprovided = request_errors(r, env, here + ("request",), applies=True)
                errs += e
                if found:
                    exercised.add((found[0], r["transition"]))
                left = (missing or []) + (unprovided or [])
                if found and left:
                    errs.append((here + ("request",), "names", f"a setup's step must apply, and it leaves out the required "
                                                               f"input{'s' if len(left) > 1 else ''} {', '.join(left)}"))
                if found and "as" in r:
                    if r["as"] in env or r["as"] == "operator":
                        errs.append((here + ("request", "as"), "names", f"'{r['as']}' is already an alias"))
                    env[r["as"]] = found[0]
                if found and r.get("creates"):
                    # the objects its effect creates, named in the order it creates them: a list, all of one type, or a
                    # mapping from each type to its objects (§11.3, ADR-0141, ADR-0173 decision 6)
                    made, named = created_types(found[0], found[1], set()), r["creates"]
                    if isinstance(named, dict):
                        pairs = []
                        for ct, aliases_of in named.items():
                            if ct not in made:
                                errs.append((here + ("request", "creates", ct), "names",
                                             f"names objects of {ct} {found[0]}.{r['transition']} creates, and its effect creates "
                                             + (f"only {', '.join(sorted(made))}" if made else "none")))
                            pairs += [(alias, ct) for alias in aliases_of]
                    else:
                        if len(made) != 1:
                            errs.append((here + ("request", "creates"), "names",
                                         f"names objects {found[0]}.{r['transition']} creates, and its effect creates "
                                         + (f"objects of more than one type, {', '.join(sorted(made))}" if made else "none")))
                        pairs = [(alias, next(iter(made)) if len(made) == 1 else None) for alias in named]
                    for alias, ct in pairs:
                        if alias in env or alias == "operator" or r.get("as") == alias:
                            errs.append((here + ("request", "creates"), "names", f"'{alias}' is already an alias"))
                        if ct is not None:
                            env[alias] = ct
            else:
                im = st["import"]
                tn = im["type"]
                if not (kinds.get(tn) or {}).get("mirror"):
                    errs.append((here + ("import", "type"), "names", f"imports a {tn}, and only a mirror type is written by the import"))
                    continue
                if im["state"] not in (kinds[tn].get("states") or {}):
                    errs.append((here + ("import", "state"), "names", f"{tn} declares no state {im['state']}"))
                attrs = kinds[tn].get("attributes") or {}
                for k, v in (im.get("values") or {}).items():
                    errs += ([(here + ("import", "values", k), "names", f"{tn} declares no attribute '{k}'")] if k not in attrs
                             else value_errors(v, attrs[k], env, here + ("import", "values", k), k))
                given = env.get(IMPORTED + im["as"])
                if given is not None and given != tn:
                    errs.append((here + ("import", "as"), "names", f"'{im['as']}' is an alias an import gave a {given}, "
                                                                   f"and a refresh imports its object as its own type"))
                elif given is None and (im["as"] in env or im["as"] == "operator"):
                    errs.append((here + ("import", "as"), "names", f"'{im['as']}' is already an alias"))
                # an alias an import gives may be imported again, a refresh of its object (ADR-0176 decision 4)
                env[im["as"]] = env[IMPORTED + im["as"]] = tn
                for j, rs in enumerate(im.get("sequences") or []):
                    errs += raise_errors(rs, env, here + ("import", "sequences", j))
        return errs

    def env_of(name, seen=()):
        """The aliases a setup leaves, built through the setup it is given."""
        if name in envs:
            return envs[name]
        if name in seen:
            out.append((("setups", name, "given"), "names", f"setup {name} is given, through others, itself"))
            return {}
        s = setups[name]
        env = {}
        if "given" in s:
            if s["given"] not in setups:
                out.append((("setups", name, "given"), "names", f"is given the setup {s['given']}, which this file does not declare"))
            elif order.index(s["given"]) > order.index(name):
                out.append((("setups", name, "given"), "order", f"is given the setup {s['given']}, declared after it; "
                                                                 "a setup is given one declared before it"))
            else:
                env = dict(env_of(s["given"], seen + (name,)))
        out.extend(stubs_errors(s.get("evaluators"), ("setups", name, "evaluators")))
        out.extend(steps_errors(s["steps"], env, ("setups", name, "steps")))
        envs[name] = env
        return env

    for name in setups:
        env_of(name)
    for name, ex in (doc.get("examples") or {}).items():
        path = ("examples", name)
        env = {}
        if "given" in ex:
            if ex["given"] not in setups:
                out.append((path + ("given",), "names", f"is given the setup {ex['given']}, which this file does not declare"))
            else:
                env = dict(envs.get(ex["given"], {}))
        out += stubs_errors(ex.get("evaluators"), path + ("evaluators",))
        out += steps_errors(ex.get("steps"), env, path + ("steps",))
        if env.get(PUBLISHED):
            continue            # the request runs under the version published, and is checked when it runs (§11.3)
        want = ex["expect"]
        verdict = want["verdict"]
        if "approve" in ex:
            # approving a proposal executes its request as the approver: the example expects what that request does, and
            # the state it leaves the proposal in, executed where it applied and pending where it was refused (§11.4)
            errs = decision_errors(ex["approve"], env, path + ("approve",))
            out += errs
            if errs:
                continue
            tn, _, xn = env[ex["approve"]["proposal"]][len("Proposal:"):].partition(".")
            x = kinds[tn]["transitions"][xn]
            found, missing, unprovided = (tn, x), [], []
            left = want.get("proposal")
            right = {"applied": {"executed"}, "unavailable": {"executed", "rejected", "withdrawn"}}.get(verdict, {"pending"})
            if left and left not in right:
                out.append((path + ("expect", "proposal"), "names",
                            f"an approval {'that applies' if verdict == 'applied' else 'refused as ' + verdict} leaves its proposal "
                            f"{' or '.join(sorted(right))}, not {left}"))
        else:
            errs, found, missing, unprovided = request_errors(ex["request"], env, path + ("request",), applies=False)
            out += errs
            if ex["request"].get("creates"):
                out.append((path + ("request", "creates"), "names", "the request under test names no objects it creates, since no step follows it"))
            if "proposal" in want:
                out.append((path + ("expect", "proposal"), "names", "only an approval leaves a proposal to state"))
            if not found:
                continue
            tn, x = found
            xn = ex["request"]["transition"]
        requested.setdefault((tn, xn), []).append(name)
        here = path + ("expect",)
        # a left-out input is refused as invalid_input, unless a refusal DESIGN.md §6 orders before it decides the request
        # first: the actor, a transition only via others, or a state the transition does not leave (D517)
        earlier = verdict in ("unavailable", "not_requestable") or (verdict == "unsatisfied" and want.get("clause") in ("actor_known", "actor_active"))
        if missing and verdict != "invalid_input" and not earlier:
            out.append((here + ("verdict",), "names", f"the request leaves out {', '.join(missing)}, which is refused as invalid_input, "
                                                      f"and the example expects {verdict}"))
        elif unprovided and verdict == "applied":
            out.append((here + ("verdict",), "names", f"the request leaves out {', '.join(unprovided)}, which is refused by "
                                                      f"{unprovided[0]}_provided, and the example expects it to apply"))
        if verdict == "applied":
            for k in ("clause", "remedy"):
                if k in want:
                    out.append((here + (k,), "names", f"an applied request names no {k}"))
            if "state" not in want:
                out.append((here, "names", "an applied request's example states the state its object reaches"))
            else:
                st = want["state"]
                allowed = ({x["to"]} if x["kind"] in ("initial", "external") else set(sources(x)) if x["kind"] == "internal"
                           else set(targets(x)) if x["kind"] == "assertion" else set(kinds[tn].get("states") or {}))
                if st not in allowed:
                    out.append((here + ("state",), "names", f"{tn}.{xn} does not leave an object in {st}"))
            attrs = kinds[tn].get("attributes") or {}
            for k in (want.get("values") or {}):
                # an attribute, a derived attribute, or a member every object has (flow-format.md §5, ADR-0147)
                if k not in attrs and k not in (kinds[tn].get("derived_attributes") or {}) and k not in BUILT_IN_MEMBERS \
                        and not (k == "recorded_by_kind" and "subject" in attrs):
                    out.append((here + ("values", k), "names", f"{tn} declares no attribute '{k}'"))
            if "occurred_at" in want:
                out += value_errors(want["occurred_at"], {"type": "timestamp"}, env, here + ("occurred_at",), "occurred_at")
            for c in want.get("cascaded") or []:
                ct, cx = c.split(".", 1)
                if cx not in ((kinds.get(ct) or {}).get("transitions") or {}):
                    out.append((here + ("cascaded",), "names", f"{c} names no transition a module checked declares"))
                else:
                    exercised.add((ct, cx))
            for c in want.get("warned") or []:
                # the transition's own warn guards by name, and a caused transition's qualified by its type
                ct, _, cn = c.rpartition(".")
                if not ct and (x.get("guards") or {}).get(cn) != "warn":
                    out.append((here + ("warned",), "names", f"{cn} is not a guard {tn}.{xn} warns by"))
                elif ct and cn not in ((kinds.get(ct) or {}).get("conditions") or {}):
                    out.append((here + ("warned",), "names", f"{c} names no condition of a type a module checked declares"))
            continue
        for k in ("state", "values", "cascaded", "warned", "occurred_at"):
            if k in want:
                out.append((here + (k,), "names", f"a request refused as {verdict} leaves no {'state' if k == 'state' else k} to state"))
        if verdict == "not_requestable" and not x.get("only_via"):
            out.append((here + ("verdict",), "names", f"{tn}.{xn} is not only via others, so it is never refused as not_requestable"))
        if verdict == "unsatisfied":
            clause, remedy = want.get("clause"), want.get("remedy")
            if clause is None or remedy is None:
                out.append((here, "names", "an unsatisfied request's example names the clause that refuses it and its remedy"))
                continue
            ct, cn = clause.split(".", 1) if "." in clause else (tn, clause)
            conds = (kinds.get(ct) or {}).get("conditions") or {}
            provided = {f"{a}_provided" for a in x.get("required_inputs", [])}
            def asks(cond):
                # an evaluator not answering, or answering too old, refuses with temporal whatever is declared (ADR-0148)
                text = " ".join(str((cond or {}).get("expression", "")).split())
                return remedy == "temporal" and any(re.search(rf"\b{re.escape(f)}\(", text) for f in functions)
            if ct == tn and cn in (x.get("guards") or {}):
                declared = conds.get(cn, {}).get("remedy")
                if declared and declared != remedy and not asks(conds.get(cn)):
                    out.append((here + ("remedy",), "names", f"{tn}.{cn} declares the remedy {declared}, not {remedy}"))
            elif ct != tn and cn in conds:
                if conds[cn].get("remedy") != remedy and not asks(conds[cn]):
                    out.append((here + ("remedy",), "names", f"{ct}.{cn} declares the remedy {conds[cn].get('remedy')}, not {remedy}"))
            elif cn not in GENERATED_GUARDS | provided:
                out.append((here + ("clause",), "names", f"{clause} is not a guard of {tn}.{xn}, a generated guard, "
                                                         "or a condition of a type its effect reaches"))
        elif verdict == "invariant_violated":
            clause = want.get("clause")
            if clause is None:
                out.append((here, "names", "an invariant_violated request's example names the invariant"))
                continue
            ct, cn = clause.split(".", 1) if "." in clause else (tn, clause)
            t = kinds.get(ct) or {}
            generated = {f"{s.lower()}_invariant" for s, v in (t.get("states") or {}).items() if v.get("required_attributes")}
            generated |= {f"{a}_unique" for a, v in (t.get("attributes") or {}).items() if v.get("unique")}
            # an actor's identity another actor holds (ADR-0190 decision 4)
            generated |= {"identity_taken"} if any(isinstance(v, dict) and v.get("actor_kind")
                                                   for v in (t.get("attributes") or {}).values()) else set()
            if cn not in (t.get("invariants") or {}) and cn not in generated:
                out.append((here + ("clause",), "names", f"{clause} is not an invariant of {ct}"))
        elif verdict == "call_refused" and "clause" in want:
            # the refused call's clause, qualified by the type the call or creation reached (DESIGN.md §5.5)
            ct, _, cn = want["clause"].partition(".")
            t = kinds.get(ct) or {}
            known = set(t.get("conditions") or {}) | set(t.get("invariants") or {}) | GENERATED_GUARDS
            if not cn or (cn not in known and not cn.endswith(("_provided", "_invariant", "_unique"))):
                out.append((here + ("clause",), "names", f"{want['clause']} is not a condition, invariant or generated guard "
                                                         "of a type, written <Type>.<clause>"))
        elif "clause" in want or ("remedy" in want and verdict != "call_refused"):
            out.append((here, "names", f"a request refused as {verdict} names no clause or remedy"))
    return out, requested, exercised


def uncovered(mdoc, kinds, requested, exercised):
    """The transitions of the examined module's types that no example requests under test, in two lists: those no
    example takes at all, and those a setup, a step or an expected cascade takes; reported, never required
    (ADR-0131 decision 5, ADR-0173 decision 6). A bound machine's transitions count for each type that binds it."""
    left = [(tn, xn) for tn in (mdoc.get("types") or {}) if not (kinds.get(tn) or {}).get("abstract")
            for xn in ((kinds.get(tn) or {}).get("transitions") or {}) if (tn, xn) not in requested]
    # a transition only via others is never a request under test, so taken it is covered (D567)
    only_via = {(tn, xn) for tn, xn in left if ((kinds[tn].get("transitions") or {}).get(xn) or {}).get("only_via")}
    return (sorted(f"{tn}.{xn}" for tn, xn in left if (tn, xn) not in exercised),
            sorted(f"{tn}.{xn}" for tn, xn in left if (tn, xn) in exercised and (tn, xn) not in only_via))


def coverage_notices(module, untaken, untested):
    """The coverage notice's two parts, each where it names a transition (flow-format.md §11.5)."""
    def n(left):
        return f"{len(left)} transition{'s' if len(left) != 1 else ''} of {module}"
    out = []
    if untaken:
        out.append(f"no example takes {n(untaken)}, by its request, a step, a setup or a cascade it expects: "
                   f"{', '.join(untaken)}")
    if untested:
        out.append(f"no example requests {n(untested)} as the request under test, which only a setup, a step or an "
                   f"expected cascade takes: {', '.join(untested)}")
    return out


def check(files, previous=None, analysed=None, examples=None, read=None, forms=None, violators=None):
    """Findings and notices over modules given in import order, as (file, line,
    severity, code, message); previous maps a file to the text of the version
    before it, against which its migration is checked. `analysed`, if given, is
    filled with each type's count of guard conditions step 5 decided in full,
    against all it read, and `examples` with each transition's, `<Type>.<transition>`,
    examples that request it, both of which the publish report carries
    (ADR-0136, ADR-0137). `read` reads a file beside the examples files by its
    name, for an example's `publish` step, which is not run without it
    (ADR-0166 decision 3)."""
    found, notes, loaded, library, declared, uses = [], [], [], {}, {}, {}
    machines = set()            # every machine declared, a type of the view that no object has
    modules, example_files = {}, []
    stored = {}                 # module -> its types, enumerations and machines, which one store holds together
    unfolded = {}               # derived attribute name -> the most it unfolds to, and its deepest, in the modules checked so far
    approvers = {}              # module -> the kinds that approve a version, where it declares them (ADR-0187)
    agents, opened = [], []     # the agent kinds of actor declared, and each module's transitions open to them (ADR-0189)
    for f, text in files:
        # a source shows what it says: a bidirectional control or an invisible formatting character can make it read
        # otherwise than it shows (fifth production review, ADR-0190 decision 9)
        hidden = next(((i, c) for i, c in enumerate(text) if hiding(c) and not (i == 0 and c == "\ufeff")), None)
        if hidden is not None:
            i, c = hidden
            found.append((f, text.count("\n", 0, i) + 1, "fatal", "yaml",
                          f"a bidirectional control or invisible formatting character, U+{ord(c):04X}, which can make a "
                          f"source read otherwise than it shows (ADR-0190)"))
            continue
        # a character outside YAML's printable set is refused before the reader refuses it at no line, as the core's
        # (the values audit's C2)
        unprintable = NON_PRINTABLE.search(text)
        if unprintable:
            found.append((f, text.count("\n", 0, unprintable.start()) + 1, "fatal", "yaml",
                          f"unacceptable character #x{ord(unprintable.group()):04x}: special characters are not allowed"))
            continue
        try:
            shallow(text)
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
        if isinstance(doc, dict) and "examples_for" in doc:
            example_files.append((f, idx, doc))     # checked once every module is (§11)
            continue
        loaded.append((f, text, idx, doc))
        errors = schema_errors(doc)
        if errors:
            found += [(f, line_of(idx, p), "fatal", c, m) for p, c, m in errors]
            continue
        literals(doc)
        bad = []
        exprs = list(flowexpr.expressions(doc))
        for p, text in exprs:
            try:
                flowexpr.parse(text)
            except flowexpr.ParseError as e:
                bad.append((f, line_of(idx, p), "fatal", "syntax", f"{e}: not an expression of declaration-syntax.md §8"))
        if bad:
            found += bad
            continue
        # a derived attribute unfolding past its bound is refused before any later step walks it (eleventh production
        # review)
        wide, sizes = fan_out_errors(doc, exprs, unfolded)
        if wide:
            found += [(f, line_of(idx, p), "fatal", c, m) for p, c, m in wide]
            continue
        for name, n, deep in sizes:
            most = unfolded.get(name, (0, 0))
            unfolded[name] = (max(most[0], n), max(most[1], deep))
        for mod, names in (doc.get("imports") or {}).items():
            if mod not in declared:
                notes.append((f, line_of(idx, ("imports", mod)), "notice", "imports",
                              f"{mod} is not among the files checked, so steps 2 and 3 cannot check what is imported from it"))
                continue
            for n in names:
                if n not in declared[mod]:
                    found.append((f, line_of(idx, ("imports", mod)), "fatal", "names",
                                  f"imports '{n}' from {mod}, which does not declare it"))
        mine = {n: k for k in ("enumerations", "sequences", "evaluators", "machines", "types") for n in (doc.get(k) or {})}
        # a name is declared once across the closure: the modules this one imports, and theirs (the model's check 33)
        closure, todo = set(), [m for m in (doc.get("imports") or {}) if m in declared]
        while todo:
            m = todo.pop()
            if m not in closure:
                closure.add(m)
                todo += [n for n in uses.get(m, ()) if n in declared]
        for m in sorted(closure):
            for n in sorted(set(mine) & declared[m]):
                found.append((f, line_of(idx, (mine[n], n)), "fatal", "names",
                              f"{n} is also declared by {m}, which is in this module's closure; a name is declared once across the closure (the model's check 33)"))
        # and across every module checked with it, since one store holds them all: a type, an enumeration or a
        # machine of a name once (ADR-0163 decision 5)
        # who approves a version is declared alike by every module that declares it (ADR-0187 decision 1)
        kinds = (doc.get("publishing") or {}).get("taken_by")
        if kinds is not None:
            kinds = sorted(kinds)
            for m, theirs in approvers.items():
                if theirs != kinds:
                    found.append((f, line_of(idx, ("publishing", "taken_by")), "fatal", "names",
                                  f"publishing.taken_by is [{', '.join(kinds)}], where {m} declares [{', '.join(theirs)}]; "
                                  f"a version's approvers are declared alike by every module that declares them (ADR-0187)"))
                    break
            approvers[doc.get("module")] = kinds
        held = {n for n, k in mine.items() if k in ("enumerations", "machines", "types")}
        for m in sorted(set(stored) - closure):
            for n in sorted(held & stored[m]):
                found.append((f, line_of(idx, (mine[n], n)), "fatal", "names",
                              f"{n} is also declared by {m}, which is checked with this module; a store holds one "
                              f"type, enumeration or machine of a name (ADR-0163)"))
        stored[doc["module"]] = held
        declared[doc["module"]] = set(mine)
        modules[doc["module"]] = doc
        uses[doc["module"]] = list(doc.get("imports") or {})
        v, back = view(doc, library)
        seen = set()
        pair, pair_notes = [], []
        if (previous or {}).get(f):
            pair, pair_notes = migration_pair_errors(load(previous[f]), doc, library)
        notes += [(f, line_of(idx, p), "notice", c, m) for p, c, m in pair_notes + final_admission_notes(doc, v)]
        for p, c, m in (order_errors(doc) + family_errors(doc, library) + machine_errors(doc, dict(types(v))) + decimal_errors(doc) + type_name_errors(doc)
                        + name_errors(v, library, ordered=False) + taken_by_errors(v) + column_errors(v)
                        + migration_errors(doc, v) + pair):
            p = back(p)
            if p is not None and p[0] == "machines" and "'" in m:
                # a machine's attributes, always quoted, are the ones it requires of its binders
                m = m.replace(f", which {p[1]} does not declare", f", which {p[1]} does not require")
            if p is not None and (p, m) not in seen:
                seen.add((p, m))
                found.append((f, line_of(idx, p), "fatal", c, m))
        library.update(dict(types(v)))
        if forms is not None:
            # each invariant a type declares, at the type declaring it, by its form (§3.4, ADR-0055)
            for tn, t in types(v):
                for i, spec in (t.get("invariants") or {}).items():
                    if not inherited(t, "invariants", i):
                        forms[f"{tn}.{i}"] = invariant_form(library, tn, (spec or {}).get("expression", ""))
        machines |= set(doc.get("machines") or {})
        if violators is not None:
            # each invariant a type declares, at the type declaring it, by the transitions that could violate it
            for tn, t in types(v):
                for i, spec in (t.get("invariants") or {}).items():
                    if not inherited(t, "invariants", i):
                        violators[f"{tn}.{i}"] = could_violate(library, tn, (spec or {}).get("expression", ""), machines)
        notes += [(f, line_of(idx, back(p)), "notice", c, m)
                  for p, c, m in notices(v) + unwritten(v, doc) + absents(v, library) + unknowns(v, library) + befores(v) + vetoes(v, library) + reaches(v)
                  if back(p) is not None]
        agents += agent_kinds(doc, v)
        opened.append((f, line_of(idx, ("module",)), agent_open(doc, v)))
    # where the version declares an agent kind of actor, each module's transitions an agent may take on the wire, which
    # name no taken_by; its generated tools offer them only for proposing (ADR-0189 decision 1)
    # and where none declares who approves a version, the default the engine applies, an agent refused and any other
    # kind taken (ADR-0187, ADR-0190 decision 10, ADR-0195 decision 4)
    if agents and not approvers and opened:
        f, line, _ = opened[0]
        notes.append((f, line, "notice", "publishing_open",
                      f"the version declares an agent kind of actor ({', '.join(sorted(agents))}) and no module "
                      f"declares publishing.taken_by, so an agent's approval of the next version is refused and any "
                      f"other kind's taken; declare publishing: {{ taken_by: [human] }} to say who approves (ADR-0195)"))
    if agents:
        for f, line, counts in opened:
            if counts:
                notes.append((f, line, "notice", "agent_open",
                              f"an agent may take on the wire the transitions here that name no taken_by: "
                              f"{', '.join(f'{tn} ({n})' for tn, n in sorted(counts.items()))}, since the version "
                              f"declares an agent kind of actor ({', '.join(sorted(agents))}); an agent's generated tools "
                              f"offer them only for proposing, where they may be proposed (ADR-0189)"))
    for f, idx, doc in example_files:
        errors = examples_schema_errors(doc)
        if not errors:
            errors, requested, exercised = example_errors(doc, library, modules)
            if examples is not None:
                for (tn, xn), names in requested.items():
                    examples.setdefault(f"{tn}.{xn}", []).extend(names)
            if not errors:
                untaken, untested = uncovered(modules[doc["examples_for"]], library, requested, exercised)
                notes += [(f, line_of(idx, ("examples",)), "notice", "coverage", m)
                          for m in coverage_notices(doc["examples_for"], untaken, untested)]
        found += [(f, line_of(idx, p), "fatal", c, m) for p, c, m in errors]
    if not found and len(loaded) + len(example_files) == len(files):
        converted = [to_text(doc, library) for _f, _t, _i, doc in loaded]
        for i, path, code, msg in language_errors(converted):
            f, _t, idx, _d = loaded[i]
            found.append((f, line_of(idx, path), "fatal", code, msg))
    if not found and len(loaded) + len(example_files) == len(files):
        # step 5: conditions within one type that can never hold together (ADR-0131 decisions 7 to 9, ADR-0137)
        enums = {n: list(v) for m in modules.values() for n, v in (m.get("enumerations") or {}).items()}
        for f, _t, idx, doc in loaded:
            for tn in doc.get("types") or {}:
                t = library.get(tn) or {}
                if t.get("abstract"):
                    continue
                results, whole, total = flowlogic.analyse_type(tn, t, enums)
                # for each type, the figure the publish report carries (ADR-0137 decision 3)
                if analysed is not None:
                    analysed[tn] = (whole, total)
                for path, severity, code, msg in results:
                    (found if severity == "fatal" else notes).append((f, line_of(idx, path), severity, code, msg))
    if not found and example_files and len(loaded) + len(example_files) == len(files):
        # step 6: each example run against the rules by the reference runner (flow-format.md §11.5, ADR-0136)
        for f, idx, doc in example_files:
            # the version checked, each file keyed by the name it declares, which a `publish` step's change is checked
            # against (ADR-0155)
            source = {flowpublish.declared_name(t) or n: t for n, t in files}
            for name, outcome, message in flowrun.run_examples(doc, library, modules, f, read, this_checker(), source):
                EXAMPLE_OUTCOMES.append((name, outcome))
                where = line_of(idx, ("examples", name))
                if outcome == "failed":
                    found.append((f, where, "fatal", "example", f"{name}: {message}"))
                elif outcome == "not run":
                    notes.append((f, where, "notice", "notrun", f"{name} is not run: {message}"))
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
        ("a condition named after a generated guard", "names", "service.yaml",
         service.replace("photo_attached:", "value_in_range:")),
        ("a guard listed twice", "yaml", "service.yaml",
         plant(service, "          photo_attached: audit\n", "          photo_attached: audit\n          photo_attached: warn\n")),
        ("a description nested past its bound", "yaml", "service.yaml",
         service.rstrip("\n") + "\nnested:\n  " + "- " * 70 + "deep\n"),
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
        ("an evaluator called by a derived attribute", "names", "customers.yaml",
         plant(customers, "    conditions:\n      contact_in_xero:",
               "    derived_attributes:\n      in_xero:\n        description: Xero knows the contact.\n        expression: xero.contact_exists(xero_contact_id)\n\n    conditions:\n      contact_in_xero:")),
        ("a superseding state entered with no successor", "names", "customers.yaml",
         plant(customers, "        effect:\n          - supersede: inputs.successor\n", "")),
        ("an object that supersedes itself", "names", "customers.yaml",
         plant(customers, "        effect:\n          - supersede: inputs.successor\n", "        effect:\n          - supersede: this\n")),
        ("a misspelt transition compared in a metric's filter", "names", "customers.yaml",
         plant(customers, "        filter: t.overrides\n", "        filter: 't.overrides and t.transition != \"corect_state\"'\n")),
        ("an enumeration's member written to a string", "names", "customers.yaml",
         plant(plant(customers, "      close:\n        kind: external\n        from: any\n        to: CLOSED\n",
                     "      close:\n        kind: external\n        from: any\n        to: CLOSED\n        effect:\n"
                     "          - assign: { location: note_kind, expr: OverrideReason.MIS_KEYED }\n"),
               "    attributes:\n", "    attributes:\n      note_kind: { type: string, optional: true }\n")),
        ("a type no module declares", "names", "customers.yaml",
         plant(customers, "    attributes:\n", "    attributes:\n      note_kind: { type: frobnicate, optional: true }\n")),
        ("the kind of actor written to a string", "names", "customers.yaml",
         plant(plant(customers, "      close:\n        kind: external\n        from: any\n        to: CLOSED\n",
                     "      close:\n        kind: external\n        from: any\n        to: CLOSED\n        effect:\n"
                     "          - assign: { location: note_kind, expr: actor.kind }\n"),
               "    attributes:\n", "    attributes:\n      note_kind: { type: string, optional: true }\n")),
        ("an invariant reading an observation collection", "names", "service.yaml",
         plant(service, "\n    conditions:\n", "\n    invariants:\n      few_failures:\n        description: A planted invariant.\n"
                        "        expression: count(r in inspections where r.outcome == CheckOutcome.FAIL) <= 2\n\n    conditions:\n")),
        ("an override into a final state its parts neither cascade on nor survive", "names", "customers.yaml",
         plant(customers, "        to: [PROSPECT, ACTIVE, DORMANT]\n", "        to: [PROSPECT, ACTIVE, DORMANT, CLOSED]\n")),
        ("a condition named as a generated guard", "names", "customers.yaml",
         plant(plant(customers, "      no_open_orders:\n", "      not_erased:\n"), "          no_open_orders: deny", "          not_erased: deny")),
        ("a successor a request may leave out", "names", "customers.yaml",
         plant(customers, "          successor: { reference: Customer }\n        effect:\n", "          successor: { reference: Customer, optional: true }\n        effect:\n")),
        ("a supersede inside a foreach", "names", "customers.yaml",
         plant(customers, "        effect:\n          - supersede: inputs.successor\n", "        effect:\n          - foreach: { item: c, array: this.contacts, limit: 5, steps: [ { supersede: inputs.successor } ] }\n")),
        ("two supersede steps", "names", "customers.yaml",
         plant(customers, "        effect:\n          - supersede: inputs.successor\n", "        effect:\n          - supersede: inputs.successor\n          - supersede: inputs.successor\n")),
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
        ("an event input given something other than this_event", "names", "delivery.yaml",
         plant(delivery, "at_event: this_event } }", "at_event: now } }")),
        ("a metric name two types declare", "names", "servicedesk.yaml",
         plant(servicedesk, "      problems_opened:\n", "      incidents_opened:\n")),
        ("a combined metric that combines itself", "order", "servicedesk.yaml",
         plant(servicedesk, "problems: Problem.problems_opened", "problems: problems_per_incident")),
        ("a combined metric grouped by a dimension none of its inputs has", "names", "servicedesk.yaml",
         plant(servicedesk, "    group_by: [month]\n    expression: problems", "    group_by: [week]\n    expression: problems")),
        ("a combined metric that aggregates", "names", "servicedesk.yaml",
         plant(servicedesk, "    expression: problems * 1.000 / incidents\n", "    expression: sum(problems) * 1.000 / incidents\n")),
        ("a summary naming no member", "names", "servicedesk.yaml",
         plant(servicedesk, "    summary: [summary, request_type, priority, state]\n", "    summary: [summary, request_kind, state]\n")),
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
        ("an import of a name its module does not declare", "names", "service.yaml",
         plant(service, "imports: { people: [User] }", "imports: { people: [User, Robot] }")),
        ("an import from a module not among the files checked", "imports", "service.yaml",
         plant(service, "imports: { people: [User] }", "imports: { staff: [User] }")),
        ("an invariant named like a generated uniqueness invariant", "names", "inventory.yaml",
         plant(inventory, "    conditions:\n", "    invariants:\n      serial_unique:\n        description: A clash with a generated name.\n"
                                              "        expression: serial is not null\n\n    conditions:\n")),
        ("a name a module in the closure also declares", "names", "service.yaml",
         plant(service, "enumerations:\n", "enumerations:\n  Service: [A, B]\n")),
        ("a call of an assertion", "names", "customers.yaml",
         plant(customers, "transition: merged_into, inputs: { successor: this } }",
               "transition: correct_state, inputs: { reason: OverrideReason.MIS_KEYED } }")),
        ("a cascade to an assertion", "names", "customers.yaml",
         plant(customers, "      discard:\n        kind: external\n        from: LISTED\n        to: REMOVED\n",
               "      discard:\n        kind: assertion\n        to: [REMOVED]\n        inputs:\n          reason: { type: OverrideReason }\n")),
        ("a name a module checked with it, outside its closure, also declares", "names", "servicedesk.yaml",
         plant(servicedesk, "enumerations:\n", "enumerations:\n  Service: [A, B]\n")),
        ("a set of references with no opposite", "names", "delivery.yaml",
         plant(delivery, "      price:        { type: money(SGD) }\n", "      price:        { type: money(SGD) }\n      helpers:      { reference: \"Courier[]\" }\n")),
        ("a sum with no body, whose value its body is (the model's check 47)", "syntax", "servicedesk.yaml",
         plant(servicedesk, "              and i.state != CANCELED and i.state != CLOSED: i.duration)\n",
               "              and i.state != CANCELED and i.state != CLOSED)\n")),
        ("a step made conditional on an optional input", "names", "service.yaml",
         plant(service, "          note:   { type: string, optional: true, personal: true }\n        guards:\n          engineer_active: deny\n",
               "          note:   { type: string, optional: true, personal: true }\n        guards:\n          engineer_active: deny\n"
               "        effect:\n          - assign: { location: photo, expr: \"if inputs.note is null then photo else photo\" }\n")),
        ("an optional input on an attribute neither optional nor defaulted", "names", "inventory.yaml",
         plant(inventory, "required_inputs: [serial, model, list_price, condition]", "required_inputs: [serial, list_price, condition]\n        optional_inputs: [model]")),
        ("an attribute named like a member every object has", "names", "inventory.yaml",
         plant(inventory, "      model:             { type: string }\n", "      model:             { type: string }\n      created_at:        { type: timestamp }\n")),
        ("an attribute named like the version every object records", "names", "inventory.yaml",
         plant(inventory, "      model:             { type: string }\n", "      model:             { type: string }\n      declaration_version: { type: int }\n")),
        ("a personal input on a type with no erasure", "names", "service.yaml",
         plant(service, "      forget:\n        kind: erasure\n        description: Erases the notes given when the job was reassigned, which are personal.\n"
                        "        inputs:\n          reason: { type: string }\n", "")),
        ("an external transition from a state to itself", "names", "service.yaml",
         plant(service, "      start:\n        kind: external\n        from: OPEN\n",
               "      start:\n        kind: external\n        from: [OPEN, WORKING]\n")),
        ("a `?` inside an inline mapping", "yaml", "service.yaml",
         plant(service, "      photo:    { type: file, optional: true }", "      photo:    { type: file? }")),
        ("a step's expression written as a mapping, reported at its own line", "schema", "service.yaml",
         plant(service, "          - assign: { location: signed_off_by_user, expr: inputs.signed_off_by }\n",
               "          - assign:\n              location: signed_off_by_user\n              expr: { value: inputs.signed_off_by }\n")),
    ]
    ok = True
    for name, expect, f, text in planted:
        before = [("inventory.yaml", inventory)] if f == "delivery.yaml" else []
        found, notes = check([("people.yaml", people)] + before + [(f, text)], {"service-v2.yaml": service} if f == "service-v2.yaml" else None)
        hit = [x for x in found + (notes if expect in ("migration", "imports") else []) if x[3] == expect]
        ok &= bool(hit)
        where = f"{hit[0][0]}:{hit[0][1]} {hit[0][4]}" if hit else f"MISSED ({found[:1]})"
        print(f"  planted {name}: {'caught' if hit else 'missed'} by {expect}, at {where}")
    # two modules of one version that declare its approvers differently (ADR-0187 decision 1)
    name = "two modules declaring their version's approvers differently"
    # people.yaml declares [human] itself (ADR-0190 decision 10)
    found, _ = check([("people.yaml", people),
                      ("service.yaml", service.replace("\ncategories:", "\npublishing: { taken_by: [agent, human] }\ncategories:", 1))])
    differ = [x for x in found if x[3] == "names" and "publishing.taken_by" in x[4]]
    ok &= len(differ) == 1 and not [x for x in found if x[3] != "names"]
    print(f"  planted {name}: {'caught, at ' + differ[0][0] + ':' + str(differ[0][1]) + ' ' + differ[0][4][:70] if differ else 'MISSED ' + str(found[:2])}")
    no = "values: [YES, NO]\n"
    strict, loose = load(no)["values"], yaml.safe_load(no)["values"]
    hit = "NO" in strict and False in loose
    ok &= hit
    print(f"  planted an enumeration value NO: the strict loader keeps {strict}, a plain YAML loader reads {loose}")
    others = resolve_any({"kind": "external", "from": "any", "to": "B"}, {"A": {}, "B": {}, "C": {"final": True}})["from"]
    ok &= others == ["A"]
    print(f"  from: any leaves every state that is not final but its target: {'yes' if others == ['A'] else 'NO ' + str(others)}")
    v1 = ("module: m\ncategories: [live, closed]\ntypes:\n  T:\n    description: A job.\n    tracking: record\n"
          "    states: { A: { category: live }, C: { category: closed, final: true } }\n"
          "    transitions: { t: { kind: initial, to: A }, go: { kind: external, from: A, to: C } }\n"
          "    metrics:\n      done:\n        description: Jobs finished from A.\n        source: transitions\n        item: x\n"
          "        filter: x.from_state == T.A and x.to_state == T.C\n        dimensions:\n          month: month(x.occurred_at)\n"
          "        expression: count()\n")
    v2 = v1.replace("C: { category: closed, final: true } }", "B: { category: live }, C: { category: closed, final: true } }") \
           .replace("go: { kind: external, from: A, to: C } }", "ready: { kind: external, from: A, to: B }, go: { kind: external, from: B, to: C } }")
    # the specification's example of a part on an abstract base, and the same with the subtype's clause taken out
    spec_text = (ROOT / "docs/design/flow-format.md").read_text()
    notes_module = spec_text[spec_text.index("```yaml\nmodule: notes") + len("```yaml\n"):]
    notes_module = notes_module[:notes_module.index("\n```\n") + 1]
    uncovered = notes_module.replace("    inherited_parts:\n      notes:\n        cascade:\n          - { on: [close], transition: file, limit: 100 }\n", "")
    missed = [x for x in check([("notes.yaml", uncovered)])[0] if "neither cascades on it nor survives it" in x[4]]
    clean = not check([("notes.yaml", notes_module)])[0]
    ok &= bool(missed) and clean
    print(f"  planted a subtype that leaves an inherited part uncovered: {'caught, ' + missed[0][4][:60] if missed else 'MISSED'};"
          f" the specification's own example is {'clean' if clean else 'NOT clean'}")
    on_base = notes_module.replace("        opposite: subject\n", "        opposite: subject\n        survives: true\n", 1)
    refused = [x for x in check([("notes.yaml", on_base)])[0] if "inherited_parts" in x[4] and "abstract" in x[4]]
    ok &= bool(refused)
    print(f"  planted a survives on an abstract base's part: {'caught, ' + refused[0][4][:60] if refused else 'MISSED'}")
    rerouted = [x for x in check([("m.yaml", v2)], {"m.yaml": v1})[1] if x[3] == "metric"]
    ok &= bool(rerouted)
    print(f"  planted a version that reroutes what a metric's filter names: {'noticed, ' + rerouted[0][4][:60] if rerouted else 'MISSED'}")
    cut = [x for x in check([("cut.yaml", "module: m\ncategories: [live]\ntypes:\n  T:\n    description: A type with one state.\n    tracking: record\n"
                                          "    states: { S: { category: live, description: One, two. }, E: { category: live, final: true } }\n"
                                          "    transitions: { t: { kind: initial, to: S }, e: { kind: external, from: S, to: E } }\n")])[0]]
    hit = bool(cut) and "§2 rule 4" in cut[0][4] and "'two.'" in cut[0][4]
    # and the variants the hiring review met (P3-4): a cut value holding an apostrophe, which the schema's message
    # writes in double quotes, and two cuts in one mapping, which it names together
    def cut_of(description):
        text = ("module: m\ncategories: [live]\ntypes:\n  T:\n    description: A type with one state.\n    tracking: record\n"
                f"    states: {{ S: {{ category: live, description: {description} }}, E: {{ category: live, final: true }} }}\n"
                "    transitions: { t: { kind: initial, to: S }, e: { kind: external, from: S, to: E } }\n")
        return [x[4] for x in check([("cut.yaml", text)])[0]]
    apostrophe = cut_of("Waits, for HR's approval.")
    plural = cut_of("One, two, three.")
    hit = (hit and len(apostrophe) == 1 and "§2 rule 4" in apostrophe[0] and "for HR's approval." in apostrophe[0]
           and len(plural) == 1 and "§2 rule 4" in plural[0] and "'three.'" in plural[0])
    ok &= hit
    print(f"  planted a comma in an unquoted inline value: "
          f"{'named as a cut value, one holding an apostrophe and two cut at once among them' if hit else 'MISSED ' + str((cut[:1], apostrophe, plural))}")
    bare = [x for x in check([("bare.yaml", "module: m\ncategories: [live]\ntypes:\n  T:\n    description: A type with two states.\n    tracking: record\n"
                                            "    states: { S: { category: live, description: Open. }, E: { category: live, final: true, description: Done. } }\n"
                                            "    conditions: { ready: { description: Ready., expression: 'true', remedy: self_serviceable } }\n"
                                            "    transitions: { t: { kind: initial, to: S }, e: { kind: external, from: S, to: E, guards: { ready: deny } } }\n")])[1]
            if x[3] == "self_serviceable"]
    ok &= len(bare) == 1
    print(f"  planted a self_serviceable guard on a transition with no input: {'noticed' if len(bare) == 1 else 'MISSED'}")
    # a correction's free-text reason on a type an erasure reaches is noticed; marked personal, or on a type no erasure
    # reaches, it is not (production review F7)
    corrected = ("module: m\ncategories: [live]\ntypes:\n  T:\n    description: A person on record.\n    tracking: record\n"
                 "    attributes:\n      email: { type: string, personal: true, optional: true }\n"
                 "    states: { S: { category: live, description: Open. }, E: { category: live, final: true, description: Done. } }\n"
                 "    transitions:\n      t: { kind: initial, to: S, required_inputs: [email] }\n"
                 "      fix: { kind: internal, from: S, corrects: [email], required_inputs: [email], inputs: { reason: { type: string } } }\n"
                 "      e: { kind: external, from: S, to: E }\n"
                 "      forget: { kind: erasure, inputs: { reason: { type: string } } }\n")
    marked = corrected.replace("required_inputs: [email], inputs: { reason: { type: string } } }",
                               "required_inputs: [email], inputs: { reason: { type: string, personal: true } } }")
    unerased = (corrected.replace("      forget: { kind: erasure, inputs: { reason: { type: string } } }\n", "")
                .replace("email: { type: string, personal: true, optional: true }", "email: { type: string, optional: true }"))
    kept = [[x for x in check([(f"{name}.yaml", text)])[1] if x[3] == "reason_kept"]
            for name, text in (("kept", corrected), ("marked", marked), ("unerased", unerased))]
    hit = [len(k) for k in kept] == [1, 0, 0]
    ok &= hit
    print(f"  planted a correction's free-text reason on a type an erasure reaches: "
          f"{'noticed, and neither marked personal nor without an erasure' if hit else 'MISSED ' + str(kept)}")
    # an optional attribute of an owned type nothing writes is noticed; one an input, a machine's assignment or a
    # backfill writes is not (D557)
    unwritten_t = ("module: m\ncategories: [live]\ntypes:\n  T:\n    description: A thing.\n    tracking: record\n"
                   "    attributes:\n      memo: { type: string, optional: true }\n"
                   "    states: { S: { category: live, description: Open. }, E: { category: live, final: true, description: Done. } }\n"
                   "    transitions:\n      t: { kind: initial, to: S }\n      e: { kind: external, from: S, to: E }\n")
    by_input = unwritten_t.replace("t: { kind: initial, to: S }", "t: { kind: initial, to: S, optional_inputs: [memo] }")
    by_machine = ("module: m\ncategories: [live]\nmachines:\n  Flow:\n    description: A flow.\n"
                  "    requires: { attributes: { memo: { type: string, optional: true } } }\n"
                  "    states: { S: { category: live, description: Open. }, E: { category: live, final: true, description: Done. } }\n"
                  "    transitions:\n      t: { kind: initial, to: S }\n"
                  "      e: { kind: external, from: S, to: E, effect: [ { assign: { location: memo, expr: '\"x\"' } } ] }\n"
                  "types:\n  T:\n    description: A thing.\n    tracking: record\n    state_machine: Flow\n"
                  "    attributes:\n      memo: { type: string, optional: true }\n")
    unwritten_seen = [[x for x in check([(f"{name}.yaml", text)])[1] if x[3] == "unwritten"]
                      for name, text in (("unwritten", unwritten_t), ("by_input", by_input), ("by_machine", by_machine))]
    hit = [len(k) for k in unwritten_seen] == [1, 0, 0]
    ok &= hit
    print(f"  planted an optional attribute nothing writes: "
          f"{'noticed, and neither one an input nor one a machine writes' if hit else 'MISSED ' + str(unwritten_seen)}")
    # an assignment whose value can be absent is noticed, a copy into a required attribute as arithmetic is, and under a
    # warn guard, which refuses nothing; tested in the expression or by a deny guard, required by the state it is taken
    # in, an optional input assigned as it is, or a plain read moved into an optional attribute, which clears it, it is
    # not (ADR-0201 decision 7, ADR-0209 decision 5)
    counter = ("module: m\ncategories: [live]\ntypes:\n  T:\n    description: A counter.\n    tracking: record\n"
               "    attributes:\n      count: { type: int, optional: true }\n      other: { type: int, optional: true }\n"
               "      total: { type: int, default: 0 }\n"
               "    states: { S: { category: live, description: Open. }, E: { category: live, final: true, description: Done. } }\n"
               "    transitions:\n      t: { kind: initial, to: S, optional_inputs: [count, other] }\n"
               "      bump: { kind: internal, from: S, inputs: { by: { type: int, optional: true } }, "
               "effect: [ { assign: { location: other, expr: 'count + 1' } } ] }\n"
               "      e: { kind: external, from: S, to: E }\n")
    looped = (counter.replace("      other: { type: int, optional: true }\n",
                              "      other: { type: int, optional: true }\n      nums: { type: \"int[]\" }\n")
              .replace("effect: [ { assign: { location: other, expr: 'count + 1' } } ]",
                       "effect: [ { foreach: { item: n, array: nums, limit: 10, "
                       "steps: [ { assign: { location: other, expr: 'n + count' } } ] } } ]"))
    counted = "    conditions: { counted: { description: Counted., expression: count is not null, remedy: unreachable_from_here } }\n"
    absent_cases = (
        ("summed", counter, 1),
        ("copied", counter.replace("location: other, expr: 'count + 1'", "location: total, expr: 'count'"), 1),
        ("moved", counter.replace("'count + 1'", "'count'"), 0),
        ("looped", looped, 1),
        ("warned", counter.replace("    transitions:", counted + "    transitions:")
                          .replace("effect: [", "guards: { counted: warn }, effect: ["), 1),
        ("tested", counter.replace("'count + 1'", "'if count is not null then count + 1 else 0'"), 0),
        ("denied", counter.replace("    transitions:", counted + "    transitions:")
                          .replace("effect: [", "guards: { counted: deny }, effect: ["), 0),
        ("required", counter.replace("S: { category: live, description: Open. }",
                                     "S: { category: live, description: Open., required_attributes: [count] }")
                            .replace("optional_inputs: [count, other]", "required_inputs: [count], optional_inputs: [other]"), 0),
        ("input", counter.replace("'count + 1'", "inputs.by"), 0),
    )
    absent_checked = {name: check([(f"{name}.yaml", text)]) for name, text, _ in absent_cases}
    absent_seen = {name: [x for x in notes if x[3] == "absent"] for name, (_, notes) in absent_checked.items()}
    # a quiet case means nothing if its module is refused, which is quiet too
    hit = all(len(absent_seen[name]) == want and not absent_checked[name][0] for name, _, want in absent_cases)
    ok &= hit
    print(f"  planted an assignment whose value can be absent: "
          f"{'noticed, summed, copied to a required attribute and under a warn guard, and not where it is tested, denied, required, an input or moved' if hit else 'MISSED ' + str({n: len(v) for n, v in absent_seen.items()})}")
    # where the version declares an agent kind of actor, a module's transitions that name no taken_by are noticed, once
    # for the module, whichever module checked declares the kind; a version declaring none is not (ADR-0189 decision 1)
    tasks = ("module: tasks\ncategories: [live]\ntypes:\n  Task:\n    description: A task.\n    tracking: record\n"
             "    states: { S: { category: live, description: Open. }, E: { category: live, final: true, description: Done. } }\n"
             "    transitions:\n      t: { kind: initial, to: S }\n      e: { kind: external, from: S, to: E, taken_by: [human] }\n")
    bots = ("module: bots\ncategories: [live]\ntypes:\n  Bot:\n    description: An agent.\n    tracking: record\n"
            "    taken_by: [human]\n    attributes:\n      bot_id: { type: identity, actor_kind: agent }\n"
            "    states: { S: { category: live, description: On. }, E: { category: live, final: true, description: Off. } }\n"
            "    transitions:\n      t: { kind: initial, to: S, required_inputs: [bot_id] }\n      e: { kind: external, from: S, to: E }\n")
    opened = [[(x[0], x[4]) for x in check(files)[1] if x[3] == "agent_open"]
              for name, files in (("agent_declared", [("tasks.yaml", tasks), ("bots.yaml", bots)]),
                                  ("none_declared", [("tasks.yaml", tasks)]))]
    hit = (len(opened[0]) == 1 and opened[0][0][0] == "tasks.yaml" and "Task (1)" in opened[0][0][1]
           and "(Bot.bot_id)" in opened[0][0][1] and opened[1] == [])
    ok &= hit
    print(f"  planted transitions naming no taken_by where a later module declares an agent kind: "
          f"{'noticed once, and not where none is declared' if hit else 'MISSED ' + str(opened)}")
    # against the previous version, a transition or a version's approval whose taken_by is dropped or widened is
    # noticed, and one narrowed is not (ADR-0189 decision 1)
    before = ("module: m\ncategories: [live]\npublishing: { taken_by: [human] }\ntypes:\n  T:\n    description: A record.\n"
              "    tracking: record\n"
              "    states: { S: { category: live, description: Open. }, E: { category: live, final: true, description: Done. } }\n"
              "    transitions:\n      t: { kind: initial, to: S, taken_by: [human] }\n"
              "      e: { kind: external, from: S, to: E, taken_by: [human] }\n"
              "      f: { kind: internal, from: S, taken_by: [human, service] }\n")
    after = (before.replace("publishing: { taken_by: [human] }\n", "")
             .replace("t: { kind: initial, to: S, taken_by: [human] }", "t: { kind: initial, to: S }")
             .replace("to: E, taken_by: [human] }", "to: E, taken_by: [agent, human] }")
             .replace("taken_by: [human, service] }", "taken_by: [human] }"))
    dropped = [sorted(x[4].split(" [")[0] for x in check([("m.yaml", text)], {"m.yaml": before})[1] if x[3] == "taken_by_dropped")
               for name, text in (("changed", after), ("unchanged", before))]
    hit = dropped == [["T.e is taken by", "T.t was taken by", "publishing.taken_by was"], []]
    ok &= hit
    print(f"  planted a version dropping or widening who takes a transition or approves a version: "
          f"{'noticed, and a narrowing not' if hit else 'MISSED ' + str(dropped)}")
    # a version naming agent in publishing where the previous declared none is noticed, and one naming only human is
    # not; an attribute no longer personal is noticed, and one staying personal is not (ADR-0196 decisions 5 and 6)
    plain = ("module: m\ncategories: [live]\ntypes:\n  T:\n    description: A record.\n    tracking: record\n"
             "    attributes:\n      who: { type: string, personal: true }\n      what: { type: string, personal: true }\n"
             "    states: { S: { category: live, description: Open. } }\n"
             "    transitions:\n      t: { kind: initial, to: S, taken_by: [human], required_inputs: [who, what] }\n"
             "      forget: { kind: erasure, inputs: { reason: { type: string } } }\n")
    widened = plain.replace("categories: [live]\n", "publishing: { taken_by: [agent, human] }\ncategories: [live]\n", 1)
    people = plain.replace("categories: [live]\n", "publishing: { taken_by: [human] }\ncategories: [live]\n", 1)
    exposed = plain.replace("who: { type: string, personal: true }", "who: { type: string }", 1)
    said = [sorted((x[3], x[4].split(" ")[0]) for x in check([("m.yaml", text)], {"m.yaml": plain})[1]
                   if x[3] in ("taken_by_dropped", "personal_dropped"))
            for text in (widened, people, exposed, plain)]
    hit = said == [[("taken_by_dropped", "publishing.taken_by")], [], [("personal_dropped", "T.who")], []]
    ok &= hit
    print(f"  planted a version letting agents approve where none was named, and one dropping personal: "
          f"{'noticed, and neither else' if hit else 'MISSED ' + str(said)}")
    # against the previous version, a condition or an invariant removed or rewritten and a guard no longer refusing
    # are noticed, once for the type, and a version changing none of them is not (ADR-0191 decision 2)
    ruled = ("module: m\ncategories: [live]\ntypes:\n  T:\n    description: A record.\n    tracking: record\n"
             "    attributes:\n      n: { type: int }\n"
             "    states: { S: { category: live, description: Open. }, E: { category: live, final: true, description: Done. } }\n"
             "    invariants:\n      small: { description: Small., expression: n < 10 }\n"
             "      kept: { description: Kept., expression: n >= 0 }\n"
             "    conditions:\n      big: { description: Big., expression: n > 5, remedy: self_serviceable }\n"
             "      odd: { description: Odd., expression: n > 1, remedy: self_serviceable }\n"
             "    transitions:\n      t: { kind: initial, to: S, required_inputs: [n] }\n"
             "      e: { kind: external, from: S, to: E, guards: { big: deny, odd: deny } }\n")
    loosened = (ruled.replace("expression: n > 5,", "expression: n > 5 or true,")
                .replace("      small: { description: Small., expression: n < 10 }\n", "")
                .replace("guards: { big: deny, odd: deny }", "guards: { big: deny, odd: warn }"))
    changed = [[x[4] for x in check([("m.yaml", text)], {"m.yaml": ruled})[1] if x[3] == "rule_changed"]
               for text in (loosened, ruled.replace("description: Big.", "description: Larger."))]
    hit = (len(changed[0]) == 1 and changed[1] == []
           and all(p in changed[0][0] for p in ("conditions rewritten: big", "invariants removed: small",
                                                 "guards no longer refusing: e.odd")))
    ok &= hit
    print(f"  planted a version loosening a type's rules: "
          f"{'noticed once, and not where only a description changes' if hit else 'MISSED ' + str(changed)}")
    # a source holding a bidirectional control is refused at its line, a byte-order mark at its start is not; and a
    # version with an agent kind and no publishing rule is noticed, one with the rule is not (ADR-0190 decisions 9, 10)
    shown = ("module: m\ncategories: [live]\ntypes:\n  T:\n    description: A record.\n    tracking: record\n"
             "    states: { S: { category: live, description: Open. }, E: { category: live, final: true, description: Done. } }\n"
             "    transitions:\n      t: { kind: initial, to: S }\n      e: { kind: external, from: S, to: E }\n")
    hid = shown.replace("A record.", "A record\u202e.")
    seen = [[(x[1], x[3]) for x in check([(f"{name}.yaml", text)])[0] if x[3] == "yaml"]
            for name, text in (("hid", hid), ("shown", "\ufeff" + shown))]
    hit = seen == [[(5, "yaml")], []]
    ok &= hit
    print(f"  planted a bidirectional control in a source: {'refused at its line, a byte-order mark not' if hit else 'MISSED ' + str(seen)}")
    # a character outside YAML's printable set is refused at its line, and so is a scalar an escape made hold a
    # surrogate, alike in both checkers, where the core read the first and this one crashed on the second (values C2)
    edges = [("ffff", shown.replace("A record.", "A record\uffff.")), ("del", shown.replace("A record.", "A record\x7f.")),
             ("pair", shown.replace("description: A record.", 'description: "A record \\ud83d\\udc4d."')),
             ("astral", shown.replace("description: A record.", 'description: "A record \\U0001F44D."'))]
    assert all(text != shown for _n, text in edges)
    seen = [[(x[1], x[3]) for x in check([(f"{name}.yaml", text)])[0] if x[3] == "yaml"] for name, text in edges]
    hit = seen == [[(5, "yaml")], [(5, "yaml")], [(5, "yaml")], []]
    ok &= hit
    print(f"  planted characters YAML does not print and an escaped surrogate: {'refused at their line, an astral escape not' if hit else 'MISSED ' + str(seen)}")
    agent = shown.replace("types:\n", "types:\n  Bot:\n    description: An agent.\n    tracking: record\n    taken_by: [human]\n"
                          "    attributes:\n      key: { type: identity, actor_kind: agent }\n"
                          "    states: { ON: { category: live, description: On. }, OFF: { category: live, final: true, description: Off. } }\n"
                          "    transitions:\n      issue: { kind: initial, to: ON, required_inputs: [key] }\n      off: { kind: external, from: ON, to: OFF }\n", 1)
    ruled = agent.replace("categories: [live]\n", "publishing: { taken_by: [human] }\ncategories: [live]\n", 1)
    open_ = [[x for x in check([(f"{name}.yaml", text)])[1] if x[3] == "publishing_open"]
             for name, text in (("agent", agent), ("ruled", ruled))]
    hit = len(open_[0]) == 1 and open_[1] == []
    ok &= hit
    print(f"  planted a version declaring an agent and no publishing rule: {'noticed, and not with the rule' if hit else 'MISSED ' + str(open_)}")
    # an invariant named as the one an actor's identity generates is refused (ADR-0190 decision 4)
    named = shown.replace("    tracking: record\n", "    tracking: record\n    invariants: { identity_taken: { description: Taken., expression: \"true\" } }\n", 1)
    taken = [x[4] for x in check([("named.yaml", named)])[0] if x[3] == "names"]
    hit = len(taken) == 1 and "an actor's identity generates" in taken[0]
    ok &= hit
    print(f"  planted an invariant named as an actor's identity's: {'refused' if hit else 'MISSED ' + str(taken)}")
    # a cascade or a foreach whose limit no invariant holds its set to is noticed, and one bounded at or below it is
    # not (fifth production review ERASE-LIMIT, ADR-0190 decision 7), an erasure's and any other transition's, which
    # past its limit an object holding more can never take (fourteenth production review N8)
    reach = ("module: m\ncategories: [live]\ntypes:\n  Claim:\n    description: A claim of lines.\n    tracking: record\n"
             "    attributes:\n      lines: { reference: \"Line[]\", aggregation: composite, opposite: claim,"
             " cascade: [ { on: [forget], transition: forget, limit: 20 }, { on: [close], transition: drop, limit: 20 } ] }\n"
             "      who: { type: string, optional: true, personal: true }\n"
             "    states: { S: { category: live, description: Open. }, E: { category: live, final: true, description: Done. } }\n"
             "    transitions:\n      t: { kind: initial, to: S }\n      close: { kind: external, from: S, to: E }\n"
             "      trim:\n        kind: internal\n        from: [S]\n"
             "        effect:\n          - foreach: { item: l, array: lines, where: l.note is null, limit: 1, steps: [ { call: { target: l, transition: touch } } ] }\n"
             "      forget:\n        kind: erasure\n        inputs: { reason: { type: string } }\n"
             "        effect:\n          - foreach: { item: l, array: lines, limit: 20, steps: [ { call: { target: l, transition: forget, inputs: { reason: inputs.reason } } } ] }\n"
             "  Line:\n    description: A line.\n    tracking: record\n"
             "    attributes:\n      claim: { reference: Claim, opposite: lines }\n      note: { type: string, optional: true, personal: true }\n"
             "    states: { L: { category: live, description: Listed. }, D: { category: live, final: true, description: Dropped. } }\n"
             "    transitions:\n      add: { kind: initial, to: L, required_inputs: [claim] }\n"
             "      drop: { kind: external, from: L, to: D, only_via: [Claim.close] }\n"
             "      touch: { kind: internal, from: [L], only_via: [Claim.trim] }\n"
             "      forget: { kind: erasure, inputs: { reason: { type: string } }, only_via: [Claim.forget] }\n")
    bounded = reach.replace("    states: { S:", "    invariants: { few: { expression: count(l in lines) <= 20 } }\n    states: { S:", 1)
    reached = [[x[4] for x in check([(f"{name}.yaml", text)])[1] if x[3] == "unbounded_reach"]
               for name, text in (("reach", reach), ("bounded", bounded))]
    hit = (len(reached[0]) == 3 and sum("can never take it" in m for m in reached[0]) == 1
           and any(m.startswith("Claim.lines's cascade on close") for m in reached[0])
           and not any(m.startswith("Claim.trim") for m in reached[0]) and reached[1] == [])
    ok &= hit
    print(f"  planted a cascade and an erasure's foreach over a set no invariant bounds: "
          f"{'noticed, and not once bounded' if hit else 'MISSED ' + str(reached)}")
    # a metric reads no personal value through a reference or a derived attribute either, where check 56 passed a
    # dimension `d.owner.name`, `max(d.owner.salary)` and `d.owner_name`; with nothing marked personal, none is found
    # (sixteenth production review N6, ADR-0203 decision 2)
    desks = (
        'module: pay\n'
        'categories: [live, closed]\n'
        'types:\n'
        '  Person:\n'
        '    description: A person.\n'
        '    tracking: record\n'
        '    attributes:\n'
        '      name: { type: string, personal: true, optional: true }\n'
        '      salary: { type: int, personal: true, optional: true }\n'
        '    states:\n'
        '      ON: { category: live, description: On. }\n'
        '      OFF: { category: closed, final: true, description: Off. }\n'
        '    derived_attributes:\n'
        '      rich: { description: Paid well., expression: salary > 100000 }\n'
        '    transitions:\n'
        '      join: { kind: initial, to: ON, optional_inputs: [name, salary] }\n'
        '      leave: { kind: external, from: ON, to: OFF }\n'
        '      forget: { kind: erasure, inputs: { reason: { type: string } } }\n'
        '  Desk:\n'
        '    description: A desk.\n'
        '    tracking: record\n'
        '    attributes:\n'
        '      owner: { reference: Person, optional: true }\n'
        '    states:\n'
        '      ON: { category: live, description: On. }\n'
        '      OFF: { category: closed, final: true, description: Off. }\n'
        '    derived_attributes:\n'
        "      owner_name: { description: Its owner's name., expression: owner.name }\n"
        '      owner_rich: { description: Its owner is paid well., expression: owner.rich }\n'
        '    transitions:\n'
        '      place: { kind: initial, to: ON, optional_inputs: [owner] }\n'
        '      clear: { kind: external, from: ON, to: OFF }\n'
        '    metrics:\n'
        '      by_owner:\n'
        '        description: Desks by owner.\n'
        '        source: objects\n'
        '        item: d\n'
        '        dimensions: { who: d.owner.name }\n'
        '        expression: max(d.owner.salary)\n'
        '      by_derived:\n'
        "        description: Desks by owner's name.\n"
        '        source: objects\n'
        '        item: d\n'
        '        dimensions: { who: d.owner_name }\n'
        '        expression: count()\n'
    )
    reach_found = [x[4] for x in check([("desks.yaml", desks)])[0] if x[3] == "names" and "which reaches" in x[4]]
    reach_clean = [x[4] for x in check([("plain.yaml", desks.replace("personal: true, ", ""))])[0]
                   if x[3] == "names" and "which reaches" in x[4]]
    hit = (len(reach_found) == 3 and reach_clean == []
           and any("d.owner_name, which reaches the derived Desk.owner_name" in w for w in reach_found)
           and any("d.owner.salary, which reaches the personal Person.salary" in w for w in reach_found))
    ok &= hit
    print(f"  planted metrics reading a person's name and salary through a reference and a derived attribute: "
          f"{'refused, and not once nothing is personal' if hit else 'MISSED ' + str((reach_found, reach_clean))}")
    # a correction taking taken_by, its own or its type's, is no correction writing more than it corrects (ADR-0190
    # decision 5)
    held = ("module: m\ncategories: [live]\ntypes:\n  T:\n    description: A record a person corrects.\n"
            "    tracking: record\n    taken_by: [human]\n"
            "    attributes:\n      amount: { type: int }\n"
            "    states: { S: { category: live, description: Open. }, E: { category: live, final: true, description: Done. } }\n"
            "    transitions:\n      t: { kind: initial, to: S, required_inputs: [amount] }\n"
            "      fix: { kind: internal, from: S, corrects: [amount], required_inputs: [amount], inputs: { reason: { type: string } } }\n"
            "      e: { kind: external, from: S, to: E }\n")
    refused = [x for x in check([("held.yaml", held)])[0] if x[3] == "names"]
    hit = refused == []
    ok &= hit
    print(f"  planted a correction a type's taken_by holds to people: {'taken' if hit else 'REFUSED ' + str(refused)}")
    # a type whose many transitions a person takes, each generating an invariant of who took it, still has its own
    # invariants' contradiction found: the generated ones, which nothing else reads, are left out of step 5 rather
    # than multiplying its cases past what it decides (ADR-0176)
    takers = "".join(f"      t{i}: {{ kind: internal, from: S }}\n" for i in range(14))
    taken = ("module: m\ncategories: [live]\ntypes:\n  T:\n    description: A record a person handles.\n    tracking: record\n"
             "    taken_by: [human]\n    attributes:\n      a: { type: int }\n"
             "    states: { S: { category: live, description: Open. }, E: { category: live, final: true, description: Done. } }\n"
             "    invariants:\n      big: { description: Big., expression: a > 5 }\n      small: { description: Small., expression: a < 3 }\n"
             "    transitions:\n      t: { kind: initial, to: S, required_inputs: [a] }\n" + takers +
             "      e: { kind: external, from: S, to: E }\n")
    decided = check([("taken.yaml", taken)])
    found = [x[3] for x in decided[0] + decided[1] if x[3] in ("contradiction", "undecided")]
    ok &= "contradiction" in found and "undecided" not in found
    print(f"  planted a type of many transitions a person takes, and two invariants that cannot hold together: "
          f"{'contradiction found, decided' if 'contradiction' in found and 'undecided' not in found else 'MISSED ' + str(found)}")
    # invariants expanding past the bound are noticed `undecided`, the notice saying that they are still enforced and
    # how to bring them within it (the reimbursement cold draft's sixteenth finding)
    wide = ("module: m\ncategories: [live]\ntypes:\n  T:\n    description: A thing.\n    tracking: record\n    attributes:\n"
            + "".join(f"      a{i}: {{ type: int, optional: true }}\n" for i in range(8))
            + "    states: { S: { category: live, description: Open. }, E: { category: live, final: true, description: Done. } }\n"
            + "    invariants:\n      wide: { description: Wide., expression: '"
            + " and ".join(f"(a{i} is null or a{i} > {i})" for i in range(8)) + "' }\n"
            + "    transitions:\n      t: { kind: initial, to: S }\n      e: { kind: external, from: S, to: E }\n")
    wide_found, wide_notes = check([("wide.yaml", wide)])
    undecided = [x[4] for x in wide_notes if x[3] == "undecided"]
    # one for each state the invariants are judged in
    hit = not wide_found and len(undecided) == 2 and all("still enforced" in u and "within it" in u for u in undecided)
    ok &= hit
    print(f"  planted invariants past the bound: {'noticed undecided, saying what to do' if hit else 'MISSED ' + str((wide_found, undecided))}")
    # a member read through a reference that its type does not declare is refused, where it typed as nothing and a
    # guard reading it refused every request (D695); a mirror's `imported_at`, withdrawn, is refused naming why, read
    # through a reference or by the mirror itself (ADR-0210 decision 3)
    reached = ("module: m\ncategories: [live, closed]\ntypes:\n  Customer:\n    description: A customer another system holds.\n"
               "    mirror: true\n    tracking: record\n    attributes:\n      code: { type: string, external: crm, indexed: true }\n"
               "    states: { ON: { category: live }, OFF: { category: closed, final: true } }\n"
               "  Order:\n    description: An order.\n    tracking: record\n    attributes:\n      customer: { reference: Customer }\n"
               "    states: { OPEN: { category: live }, DONE: { category: closed, final: true } }\n"
               "    conditions:\n      fresh: { description: Fresh., expression: 'customer.READ + 1 h >= now', remedy: dependent }\n"
               "    transitions:\n      place: { kind: initial, to: OPEN, required_inputs: [customer] }\n"
               "      ship: { kind: external, from: OPEN, to: DONE, guards: { fresh: deny } }\n")
    def reach(member):
        return [x[4] for x in check([("m.yaml", reached.replace("READ", member))])[0]]
    undeclared, withdrawn, declared = reach("nonexistent"), reach("imported_at"), reach("created_at")
    own = [x[4] for x in check([("m.yaml", reached.replace("customer.READ", "created_at").replace(
        "    states: { ON: { category: live }, OFF: { category: closed, final: true } }\n",
        "    states: { ON: { category: live }, OFF: { category: closed, final: true } }\n"
        "    derived_attributes:\n      stale: { description: Stale., expression: 'imported_at + 1 h < now' }\n"))])[0]]
    hit = (len(undeclared) == 1 and "nonexistent of Customer, which Customer does not declare" in undeclared[0]
           and len(withdrawn) == 1 and "imported_at of Customer, which is withdrawn" in withdrawn[0] and not declared)
    ok &= hit
    print(f"  planted a member through a reference its type does not declare: "
          f"{'refused, and a declared one taken' if hit else 'MISSED ' + str((undeclared, withdrawn, declared))}")
    hit = len(own) == 1 and "reads 'imported_at', which is withdrawn" in own[0]
    ok &= hit
    print(f"  planted a mirror reading its withdrawn imported_at: {'refused, naming why' if hit else 'MISSED ' + str(own)}")
    end = "    states:\n      S: { category: live, description: Open. }\n      E: { category: live, final: true, description: Done. }\n"
    ring = [x for x in check([("ring.yaml", "module: m\ncategories: [live]\ntypes:\n"
                                            "  A:\n    description: One end.\n    tracking: record\n"
                                            "    attributes:\n      b: { reference: B, optional: true, opposite: as_ }\n" + end +
                                            "    derived_attributes:\n      x: { description: Reads B's y., expression: b.y + 1 }\n"
                                            "    transitions: { t: { kind: initial, to: S }, e: { kind: external, from: S, to: E } }\n"
                                            "  B:\n    description: The other end.\n    tracking: record\n"
                                            "    attributes:\n      as_: { reference: \"A[]\", opposite: b }\n" + end +
                                            "    derived_attributes:\n      y: { description: Reads A's x., expression: count(a in as_ where a.x > 0) }\n"
                                            "    transitions: { t: { kind: initial, to: S }, e: { kind: external, from: S, to: E } }\n")])[0]
            if x[3] == "order" and "cycle" in x[4]]
    ok &= len(ring) == 1
    print(f"  planted derived attributes of two types that read each other: {'caught, ' + ring[0][4] if len(ring) == 1 else 'MISSED'}")
    # a derived attribute reading itself through a reference of its own type is a cycle, where it overflowed the core's
    # stack on a read; a chain of 1,500 derived attributes each reading the one above is checked clean without a deep
    # search; and
    # an identifier's width past 20 digits is refused (tenth production review)
    node = ("module: m\ncategories: [live]\ntypes:\n  Node:\n    description: A node.\n    tracking: record\n"
            "    attributes:\n      parent: { reference: Node, optional: true }\n" + end +
            "    derived_attributes:\n      depth: { description: Its depth., expression: parent.depth + 1 }\n"
            "    transitions: { t: { kind: initial, to: S }, e: { kind: external, from: S, to: E } }\n")
    selfring = [x for x in check([("node.yaml", node)])[0] if x[3] == "order" and "cycle" in x[4]]
    def chain(n):
        return ("module: m\ncategories: [live]\ntypes:\n  T:\n    description: A thing.\n    tracking: record\n"
                "    attributes:\n      n: { type: int }\n" + end + "    derived_attributes:\n"
                + "      d0: { description: The first., expression: this.n }\n"
                + "".join(f"      d{i}: {{ description: One., expression: this.d{i - 1} + 1 }}\n" for i in range(1, n))
                + "    transitions: { t: { kind: initial, to: S, required_inputs: [n] }, e: { kind: external, from: S, to: E } }\n")
    # a chain of derived attributes past the depth an evaluation goes through is refused at its first link past it,
    # where 9,500 were accepted and read null past the 65th; one at the bound checks clean; and one split across an
    # import is counted whole (fourteenth production review N1)
    deeper = (f"reads derived attributes more than {DEPTH_LIMIT} deep, each reading the next, past the most one "
              f"evaluation goes through")
    chained = [(x[3], x[4]) for x in check([("chain.yaml", chain(1500))])[0] if x[2] == "fatal"]
    level = check([("level.yaml", chain(DEPTH_LIMIT))])[0]
    da = ("module: da\ncategories: [live]\ntypes:\n  TA:\n    description: A thing.\n    tracking: record\n"
          "    attributes:\n      x: { type: int }\n" + end + "    derived_attributes:\n"
          + "      a0: { description: The first., expression: this.x }\n"
          + "".join(f"      a{i}: {{ description: One., expression: this.a{i - 1} + 1 }}\n" for i in range(1, 40))
          + "    transitions: { t: { kind: initial, to: S, required_inputs: [x] }, e: { kind: external, from: S, to: E } }\n")
    db = ("module: db\nimports:\n  da: [TA]\ncategories: [live]\ntypes:\n  TB:\n    description: A thing.\n    tracking: record\n"
          "    attributes:\n      r: { reference: TA, optional: true }\n" + end + "    derived_attributes:\n"
          + "      b0: { description: The first., expression: r.a39 + 1 }\n"
          + "".join(f"      b{i}: {{ description: One., expression: this.b{i - 1} + 1 }}\n" for i in range(1, 30))
          + "    transitions: { t: { kind: initial, to: S }, e: { kind: external, from: S, to: E } }\n")
    stacked = [(x[0], x[3], x[4]) for x in check([("da.yaml", da), ("db.yaml", db)])[0] if x[2] == "fatal"]
    # a read of a later type's chain finds it unwalked, so its depth is carried up the walk as each link is left: one
    # past the bound refused, one at it clean

    def forward(top):
        return ("module: m\ncategories: [live]\ntypes:\n  A:\n    description: A thing.\n    tracking: record\n"
                "    attributes:\n      b: { reference: B, optional: true }\n" + end + "    derived_attributes:\n"
                f"      a0: {{ description: The first., expression: b.d{top} + 1 }}\n"
                "    transitions: { t: { kind: initial, to: S }, e: { kind: external, from: S, to: E } }\n"
                "  B:\n    description: A thing.\n    tracking: record\n"
                "    attributes:\n      n: { type: int }\n" + end + "    derived_attributes:\n"
                + "      d0: { description: The first., expression: this.n }\n"
                + "".join(f"      d{i}: {{ description: One., expression: this.d{i - 1} + 1 }}\n" for i in range(1, 64))
                + "    transitions: { t: { kind: initial, to: S, required_inputs: [n] }, e: { kind: external, from: S, to: E } }\n")
    ahead = [(x[3], x[4]) for x in check([("ahead.yaml", forward(63))])[0] if x[2] == "fatal"]
    held = [(x[3], x[4]) for x in check([("held.yaml", forward(62))])[0] if x[2] == "fatal"]
    wide = [x[4] for x in check([("wide.yaml", "module: m\ncategories: [live]\nsequences:\n  s:\n    description: A series.\n"
                                              "types:\n  T:\n    description: A thing.\n    tracking: record\n"
                                              "    attributes:\n      code: { type: string, identifier: { sequence: s, format: \"T-{n:1000000}\" } }\n"
                                              + end + "    transitions: { t: { kind: initial, to: S }, e: { kind: external, from: S, to: E } }\n")])[0]
            if x[3] == "names" and "wider than" in x[4]]
    # a chain of derived attributes each reading the one before twice unfolds to 2^n reads, and every check reading
    # one walked each again, forty holding the dry run's slot past six minutes: refused at its first link past the
    # bound, before any later step walks it, as is an invariant whose reads unfold past it; within it, checked clean
    # (eleventh production review)
    def diamond(n, invariants=""):
        return ("module: m\ncategories: [live]\ntypes:\n  T:\n    description: A thing.\n    tracking: record\n"
                "    attributes:\n      x: { type: int }\n" + end + "    derived_attributes:\n"
                + "      d0: { description: The first., expression: this.x + this.x }\n"
                + "".join(f"      d{i}: {{ description: One., expression: this.d{i - 1} + this.d{i - 1} }}\n" for i in range(1, n))
                + invariants
                + "    transitions: { t: { kind: initial, to: S, required_inputs: [x] }, e: { kind: external, from: S, to: E } }\n")
    began = time.time()
    fanned = [(x[3], x[4]) for x in check([("diamond.yaml", diamond(40, "    invariants:\n      big: { description: Big., expression: d39 > 0 }\n"))])[0]
              if x[2] == "fatal"]
    twice = [(x[3], x[4]) for x in check([("twice.yaml", diamond(13, "    invariants:\n      big: { description: Big., expression: d12 + d12 > 0 }\n"))])[0]
             if x[2] == "fatal"]
    within = [x for x in check([("within.yaml", diamond(12, "    invariants:\n      big: { description: Big., expression: d11 > 0 }\n"))])[0]
              if x[2] == "fatal"]
    # and split across a module and one importing it, each within the bound, the chain read through a reference is
    # counted whole, where each module passed and an invariant on the far end held a dry run's slot to its deadline
    # and hung this checker (twelfth production review)
    ca = ("module: ca\ncategories: [live]\ntypes:\n  TA:\n    description: A thing.\n    tracking: record\n"
          "    attributes:\n      x: { type: int }\n" + end + "    derived_attributes:\n"
          + "      a0: { description: The first., expression: this.x + this.x }\n"
          + "".join(f"      a{i}: {{ description: One., expression: this.a{i - 1} + this.a{i - 1} }}\n" for i in range(1, 13))
          + "    transitions: { t: { kind: initial, to: S, required_inputs: [x] }, e: { kind: external, from: S, to: E } }\n")
    cb = ("module: cb\nimports:\n  ca: [TA]\ncategories: [live]\ntypes:\n  TB:\n    description: A thing.\n    tracking: record\n"
          "    attributes:\n      r: { reference: TA, optional: true }\n" + end + "    derived_attributes:\n"
          + "      b0: { description: The first., expression: r.a12 + r.a12 }\n"
          + "".join(f"      b{i}: {{ description: One., expression: this.b{i - 1} + this.b{i - 1} }}\n" for i in range(1, 13))
          + "    invariants:\n      big: { description: Big., expression: b12 > 0 }\n"
          + "    transitions: { t: { kind: initial, to: S }, e: { kind: external, from: S, to: E } }\n")
    split = [(x[0], x[3], x[4]) for x in check([("ca.yaml", ca), ("cb.yaml", cb)])[0] if x[2] == "fatal"]
    # upper-case text in a string literal names no state, where a default of "EU" was refused as one; a bare
    # upper-case word that is no state still is (twelfth production review)
    quoted = ("module: m\ncategories: [live]\ntypes:\n  T:\n    description: A thing.\n    tracking: record\n"
              "    attributes:\n      region: { type: string, default: '\"EU\"' }\n" + end
              + "    conditions:\n      home:\n        description: Home.\n        expression: region == \"HQ\"\n        remedy: self_serviceable\n"
              + "    transitions: { t: { kind: initial, to: S }, e: { kind: external, from: S, to: E, guards: { home: deny } } }\n")
    literal = [x[4] for x in check([("quoted.yaml", quoted)])[0] if x[3] == "names"]
    bare = [x[4] for x in check([("bare.yaml", quoted.replace('region == "HQ"', "region == HQ"))])[0]
            if x[3] == "names" and "HQ, which is not a state" in x[4]]
    quick = time.time() - began < 20
    unfolds = (f"unfolds to more than {UNFOLD_LIMIT} reads of derived attributes, each counted as often as it is made: "
               f"one reading another twice reads all that one reads twice")
    hit = (len(selfring) == 1 and chained == [("order", f"T.d64 {deeper}")] and level == []
           and stacked == [("db.yaml", "order", f"TB.b24 {deeper}")] and ahead == [("order", f"A.a0 {deeper}")]
           and held == [] and len(wide) == 1 and fanned == [("order", f"T.d13 {unfolds}")]
           and twice == [("order", f"types.T.invariants.big.expression {unfolds}")] and within == []
           and split == [("cb.yaml", "order", f"TB.b0 {unfolds}")] and literal == [] and len(bare) == 1 and quick)
    ok &= hit
    print(f"  planted a derived attribute reading itself through a reference, a long chain, a wide identifier, and "
          f"diamond chains: {'caught, the chains within the bound clean and all quick' if hit else 'MISSED ' + str((selfring, chained[:1], level[:1], stacked[:2], ahead[:2], held[:2], wide, fanned[:2], twice[:2], within[:1], split[:2], literal[:1], bare, quick))}")
    written = load("module: m\ntypes:\n  T:\n    description: d\n    tracking: record\n"
                   "    attributes: { a: { type: bool, default: false }, b: { type: int, default: 0, indexed: true } }\n"
                   "    states: { S: { category: live, final: true } }\n"
                   "    transitions:\n      t:\n        kind: initial\n        to: S\n        effect:\n"
                   "          - create: { type: U, transition: u, inputs: { flag: true, n: 2 } }\n"
                   "          - foreach: { item: i, range: 3, limit: 3, steps: [] }\n"
                   "migration: { backfill: { T: { c: true } } }\n")
    literals(written)
    t = written["types"]["T"]
    step, loop = t["transitions"]["t"]["effect"]
    hit = (t["attributes"]["a"]["default"] == "false" and t["attributes"]["b"] == {"type": "int", "default": "0", "indexed": True}
           and t["states"]["S"]["final"] is True and step["create"]["inputs"] == {"flag": "true", "n": "2"}
           and loop["foreach"]["range"] == "3" and loop["foreach"]["limit"] == 3
           and written["migration"]["backfill"]["T"]["c"] == "true")
    ok &= hit
    print(f"  expressions written as YAML's own boolean or number read as their literals, and nothing else: {'yes' if hit else 'NO ' + str(written)}")
    return ok


SYNTAX_MINI = (
    'module: mini\n'
    'categories: [live, closed]\n'
    'enumerations:\n'
    '  Reason: [ASKED]\n'
    'types:\n'
    '  Job:\n'
    '    description: A job.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      title: { type: string }\n'
    '      note:  { type: string, optional: true }\n'
    '    states:\n'
    '      OPEN: { category: live }\n'
    '      DONE: { category: closed, final: true }\n'
    '    conditions:\n'
    '      titled: { description: The job has a title., expression: title is not null, remedy: dependent }\n'
    '    transitions:\n'
    '      open:   { kind: initial, to: OPEN, required_inputs: [title], optional_inputs: [note] }\n'
    '      finish: { kind: external, from: [OPEN], to: DONE, guards: { titled: deny } }\n'
)
SYNTAX_PARTS = (
    'module: shop\n'
    'categories: [live, closed]\n'
    'types:\n'
    '  Order:\n'
    '    description: An order.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      lines: { reference: "Line[]", opposite: order, aggregation: composite, survives: true }\n'
    '    states:\n'
    '      OPEN: { category: live }\n'
    '      DONE: { category: closed, final: true }\n'
    '    transitions:\n'
    '      open:     { kind: initial, to: OPEN }\n'
    '      add_line: { kind: internal, from: [OPEN], effect: [ { create: { type: Line, transition: start, inputs: { order: this } } } ] }\n'
    '      finish:   { kind: external, from: [OPEN], to: DONE }\n'
    '  Line:\n'
    '    description: A line of an order.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      order: { reference: Order, opposite: lines }\n'
    '    states:\n'
    '      ACTIVE: { category: live }\n'
    '      GONE:   { category: closed, final: true }\n'
    '    transitions:\n'
    '      start: { kind: initial, to: ACTIVE, required_inputs: [order], only_via: [Order.add_line] }\n'
    '      drop:  { kind: external, from: [ACTIVE], to: GONE }\n'
)
# a part an abstract base declares, which each robot gives its cascade (check 12's self-test)
CYCLE_FLEET = (
    'module: fleet\n'
    'categories: [live, closed]\n'
    'types:\n'
    '  Asset:\n'
    '    description: An asset.\n'
    '    abstract: true\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      pieces: { reference: "Piece[]", opposite: asset, aggregation: composite }\n'
    '  Robot:\n'
    '    description: A robot.\n'
    '    extends: Asset\n'
    '    tracking: record\n'
    '    inherited_parts:\n'
    '      pieces: { cascade: [ { on: [retire], transition: drop, limit: 10 } ] }\n'
    '    states:\n'
    '      UP:   { category: live }\n'
    '      GONE: { category: closed, final: true }\n'
    '    transitions:\n'
    '      make:   { kind: initial, to: UP }\n'
    '      fit:    { kind: internal, from: [UP], effect: [ { create: { type: Piece, transition: fit, inputs: { asset: this, spare: this } } } ] }\n'
    '      retire: { kind: external, from: [UP], to: GONE }\n'
    '  Piece:\n'
    '    description: A piece of an asset.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      asset: { reference: Asset, opposite: pieces }\n'
    '      spare: { reference: Robot }\n'
    '    states:\n'
    '      IN:  { category: live }\n'
    '      OUT: { category: closed, final: true }\n'
    '    transitions:\n'
    '      fit:  { kind: initial, to: IN, required_inputs: [asset, spare], only_via: [Robot.fit] }\n'
    '      drop: { kind: external, from: [IN], to: OUT }\n'
)
# a whole cascading to parts of an abstract type, every member of whose family has the transition (ADR-0170)
ABSTRACT_PART = (
    'module: rack\n'
    'categories: [live, closed]\n'
    'types:\n'
    '  Rack:\n'
    '    description: A rack.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      slots: { reference: "Slot[]", opposite: rack, aggregation: composite, cascade: [ { on: [retire], transition: drop, limit: 10 } ] }\n'
    '    states:\n'
    '      UP:   { category: live }\n'
    '      GONE: { category: closed, final: true }\n'
    '    transitions:\n'
    '      make:   { kind: initial, to: UP }\n'
    '      fit:    { kind: internal, from: [UP], effect: [ { create: { type: Card, transition: fit, inputs: { rack: this } } } ] }\n'
    '      retire: { kind: external, from: [UP], to: GONE }\n'
    '  Slot:\n'
    '    description: Something in a rack.\n'
    '    abstract: true\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      rack: { reference: Rack, opposite: slots }\n'
    '  Card:\n'
    '    description: A card in a rack.\n'
    '    extends: Slot\n'
    '    tracking: record\n'
    '    states:\n'
    '      IN:  { category: live }\n'
    '      OUT: { category: closed, final: true }\n'
    '    transitions:\n'
    '      fit:  { kind: initial, to: IN, required_inputs: [rack], only_via: [Rack.fit] }\n'
    '      drop: { kind: external, from: [IN], to: OUT }\n'
)
# customers whose name is personal, and their visits, clean under check 10 (its self-test plants into it)
PERSONAL_CRM = (
    'module: crm\n'
    'categories: [live, closed]\n'
    'enumerations:\n'
    '  ErasureReason: [ASKED]\n'
    'types:\n'
    '  Customer:\n'
    '    description: A customer.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      name:  { type: string, optional: true, personal: true }\n'
    '      label: { type: string, optional: true }\n'
    '    states:\n'
    '      ACTIVE: { category: live }\n'
    '      GONE:   { category: closed, final: true }\n'
    '    derived_attributes:\n'
    '      called: { description: What the customer is called., expression: name }\n'
    '    transitions:\n'
    '      join:   { kind: initial, to: ACTIVE, optional_inputs: [name] }\n'
    '      tag:    { kind: internal, from: [ACTIVE], inputs: { text: { type: string } }, effect: [ { assign: { location: label, expr: inputs.text } } ] }\n'
    '      relay:  { kind: internal, from: [ACTIVE], inputs: { text: { type: string } }, effect: [ { call: { target: this, transition: tag, inputs: { text: inputs.text } } } ] }\n'
    '      rename: { kind: internal, from: [ACTIVE], inputs: { text: { type: string } }, effect: [ { assign: { location: name, expr: inputs.text } } ] }\n'
    '      leave:  { kind: external, from: [ACTIVE], to: GONE }\n'
    '      forget: { kind: erasure, inputs: { reason: { type: ErasureReason } } }\n'
    '  Visit:\n'
    '    description: A visit to a customer.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      customer: { reference: Customer }\n'
    '      summary:  { type: string, optional: true }\n'
    '      contact:  { type: string, optional: true, personal: true }\n'
    '      note:     { type: string, optional: true }\n'
    '      flagged:  { type: int, optional: true }\n'
    '    observations:\n'
    '      notes:\n'
    '        description: Notes taken at a visit.\n'
    '        kind: VisitNote\n'
    '        attributes:\n'
    '          text: { type: string, optional: true, personal: true }\n'
    '          urgent: { type: bool }\n'
    '    states:\n'
    '      OPEN: { category: live }\n'
    '      DONE: { category: closed, final: true }\n'
    '    transitions:\n'
    '      book:   { kind: initial, to: OPEN, required_inputs: [customer], optional_inputs: [note] }\n'
    '      close:  { kind: external, from: [OPEN], to: DONE }\n'
    '      forget: { kind: erasure, inputs: { reason: { type: ErasureReason } } }\n'
)
# pieces whose whole is a family's abstract base, each created by a robot (check 36's self-test)
REPARENT_KIT = (
    'module: kit\n'
    'categories: [live, closed]\n'
    'types:\n'
    '  Asset:\n'
    '    description: An asset, the base of every kind of asset.\n'
    '    abstract: true\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      pieces: { reference: "Piece[]", opposite: asset, aggregation: composite }\n'
    '  Robot:\n'
    '    description: A robot.\n'
    '    extends: Asset\n'
    '    inherited_parts:\n'
    '      pieces: { survives: true }\n'
    '    states:\n'
    '      UP:   { category: live }\n'
    '      GONE: { category: closed, final: true }\n'
    '    transitions:\n'
    '      make:   { kind: initial, to: UP }\n'
    '      fit:    { kind: internal, from: [UP], effect: [ { create: { type: Piece, transition: fit, inputs: { asset: this } } } ] }\n'
    '      retire: { kind: external, from: [UP], to: GONE }\n'
    '  Piece:\n'
    '    description: A piece of an asset.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      asset: { reference: Asset, opposite: pieces }\n'
    '    states:\n'
    '      IN:  { category: live }\n'
    '      OUT: { category: closed, final: true }\n'
    '    transitions:\n'
    '      fit:  { kind: initial, to: IN, required_inputs: [asset], only_via: [Robot.fit] }\n'
    '      drop: { kind: external, from: [IN], to: OUT }\n'
)
# an invariant of each form, of a customer and its visits (the report's forms, ADR-0055)
INVARIANT_FORMS = (
    'module: crm\n'
    'categories: [live, closed]\n'
    'enumerations:\n'
    '  ErasureReason: [ASKED]\n'
    'types:\n'
    '  Customer:\n'
    '    description: A customer.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      name:  { type: string, optional: true, personal: true }\n'
    '      label: { type: string, optional: true }\n'
    '    states:\n'
    '      ACTIVE: { category: live }\n'
    '      GONE:   { category: closed, final: true }\n'
    '    derived_attributes:\n'
    '      called: { description: What the customer is called., expression: name }\n'
    '    invariants:\n'
    '      labelled:      { description: A label is not blank., expression: \'label is null or label != ""\' }\n'
    '      called:        { description: A name is not blank., expression: \'called is null or called != ""\' }\n'
    '      few_referrers: { description: Few hold it., expression: count(r in referrers) < 100 }\n'
    '    transitions:\n'
    '      join:   { kind: initial, to: ACTIVE, optional_inputs: [name] }\n'
    '      leave:  { kind: external, from: [ACTIVE], to: GONE }\n'
    '      forget: { kind: erasure, inputs: { reason: { type: ErasureReason } } }\n'
    '  Visit:\n'
    '    description: A visit to a customer.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      customer: { reference: Customer }\n'
    '    observations:\n'
    '      notes:\n'
    '        description: Notes taken at a visit.\n'
    '        kind: VisitNote\n'
    '        attributes:\n'
    '          text: { type: string, optional: true, personal: true }\n'
    '    states:\n'
    '      OPEN: { category: live }\n'
    '      DONE: { category: closed, final: true }\n'
    '    derived_attributes:\n'
    "      customer_label: { description: Its customer's label., expression: customer.label }\n"
    '      similar:        { description: Visits to its customer., expression: count(v in Visit where v.customer == customer) }\n'
    '    invariants:\n'
    '      known_customer:    { description: Its customer is not blocked., expression: \'customer.label != "blocked"\' }\n'
    '      labelled_customer: { description: Its customer is not x., expression: \'customer_label != "x"\' }\n'
    '      one_open:          { description: One open visit per customer., expression: none(v in Visit where v.customer == customer and v.open and open) }\n'
    '      few_similar:       { description: Few visits per customer., expression: similar < 5 }\n'
    '    transitions:\n'
    '      book:   { kind: initial, to: OPEN, required_inputs: [customer] }\n'
    '      close:  { kind: external, from: [OPEN], to: DONE }\n'
    '      forget: { kind: erasure, inputs: { reason: { type: ErasureReason } } }\n'
)
# bookings of a room, whose one invariant check 6's self-test replaces
SYMMETRY_DESK = (
    'module: desk\n'
    'categories: [live, closed]\n'
    'types:\n'
    '  Room:\n'
    '    description: A room.\n'
    '    tracking: record\n'
    '    states:\n'
    '      FREE: { category: live }\n'
    '      GONE: { category: closed, final: true }\n'
    '    transitions:\n'
    '      make: { kind: initial, to: FREE }\n'
    '      drop: { kind: external, from: [FREE], to: GONE }\n'
    '  Booking:\n'
    '    description: A booking of a room.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      room:   { reference: Room }\n'
    '      starts: { type: timestamp, indexed: true }\n'
    '      ends:   { type: timestamp, indexed: true }\n'
    '      cap:    { type: int, optional: true }\n'
    '    states:\n'
    '      HELD: { category: live }\n'
    '      DONE: { category: closed, final: true }\n'
    '    derived_attributes:\n'
    '      same_room:   { description: Other bookings of its room., expression: count(b in Booking where b.room == room) }\n'
    '      others_held: { description: Other held bookings of its room., expression: count(b in Booking where b.room == room and b.open) }\n'
    '    invariants:\n'
    '      probe: { description: A probe., expression: none(b in Booking where b.room == room and b.starts < ends and b.ends > starts and b.open and open) }\n'
    '    transitions:\n'
    '      book:   { kind: initial, to: HELD, required_inputs: [room, starts, ends], optional_inputs: [cap] }\n'
    '      finish: { kind: external, from: [HELD], to: DONE }\n'
)
# an account of every type an expression combines, whose one derivation check 24's self-test replaces
TYPED_LEDGER = (
    'module: ledger\n'
    'categories: [live, closed]\n'
    'enumerations:\n'
    '  Tier: [GOLD, SILVER]\n'
    'types:\n'
    '  Account:\n'
    '    description: An account.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      usd:   { type: money(USD) }\n'
    '      sgd:   { type: money(SGD) }\n'
    '      rate:  { type: "decimal(8,2)" }\n'
    '      fine:  { type: "decimal(8,3)" }\n'
    '      count: { type: int }\n'
    '      since: { type: timestamp }\n'
    '      span:  { type: duration }\n'
    '      label: { type: string }\n'
    '      vip:   { type: bool }\n'
    '      tier:  { type: Tier }\n'
    '    states:\n'
    '      OPEN: { category: live }\n'
    '      SHUT: { category: closed, final: true }\n'
    '    derived_attributes:\n'
    '      probe: { description: A probe., expression: "usd + usd" }\n'
    '    transitions:\n'
    '      open: { kind: initial, to: OPEN, required_inputs: [usd, sgd, rate, fine, count, since, span, label, vip, tier] }\n'
    '      shut: { kind: external, from: [OPEN], to: SHUT }\n'
)
# a desk counting what names it, and a chair a bound machine seats at one (the report's violators)
MACHINE_OFFICE = (
    'module: office\n'
    'categories: [live, closed]\n'
    'machines:\n'
    '  Seating:\n'
    '    description: Where a chair stands.\n'
    '    requires:\n'
    '      attributes:\n'
    '        desk: { reference: Desk, optional: true }\n'
    '    states:\n'
    '      FREE:  { category: live }\n'
    '      TAKEN: { category: live }\n'
    '      GONE:  { category: closed, final: true }\n'
    '    transitions:\n'
    '      bring: { kind: initial, to: FREE }\n'
    '      sit:   { kind: external, from: [FREE], to: TAKEN, inputs: { at: { reference: Desk } }, effect: [ { assign: { location: desk, expr: inputs.at } } ] }\n'
    '      scrap: { kind: external, from: [FREE, TAKEN], to: GONE }\n'
    'types:\n'
    '  Desk:\n'
    '    description: A desk.\n'
    '    tracking: record\n'
    '    states:\n'
    '      UP:   { category: live }\n'
    '      DOWN: { category: closed, final: true }\n'
    '    invariants:\n'
    '      crowded: { description: Few things name a desk., expression: count(r in referrers) < 9 }\n'
    '    transitions:\n'
    '      make: { kind: initial, to: UP }\n'
    '      drop: { kind: external, from: [UP], to: DOWN }\n'
    '  Chair:\n'
    '    description: A chair.\n'
    '    tracking: record\n'
    '    state_machine: Seating\n'
    '    attributes:\n'
    '      desk: { reference: Desk, optional: true }\n'
)
SYNTAX_PAIR = (
    'module: pair\n'
    'categories: [live, closed]\n'
    'types:\n'
    '  Desk:\n'
    '    description: A desk.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      seat: { reference: Seat, optional: true, opposite: desk, stored: true }\n'
    '    states:\n'
    '      FREE: { category: live }\n'
    '      GONE: { category: closed, final: true }\n'
    '    transitions:\n'
    '      place:  { kind: initial, to: FREE }\n'
    '      remove: { kind: external, from: [FREE], to: GONE }\n'
    '  Seat:\n'
    '    description: A seat.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      desk: { reference: Desk, optional: true, opposite: seat }\n'
    '    states:\n'
    '      FREE: { category: live }\n'
    '      GONE: { category: closed, final: true }\n'
    '    transitions:\n'
    '      place:  { kind: initial, to: FREE }\n'
    '      remove: { kind: external, from: [FREE], to: GONE }\n'
)
SYNTAX_METRIC = (
    'module: mini\n'
    'categories: [live, closed]\n'
    'enumerations:\n'
    '  Reason: [ASKED]\n'
    'types:\n'
    '  Job:\n'
    '    description: A job.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      title: { type: string }\n'
    '      note:  { type: string, optional: true }\n'
    '    states:\n'
    '      OPEN: { category: live }\n'
    '      DONE: { category: closed, final: true }\n'
    '    conditions:\n'
    '      titled: { description: The job has a title., expression: title is not null, remedy: dependent }\n'
    '    transitions:\n'
    '      open:   { kind: initial, to: OPEN, required_inputs: [title], optional_inputs: [note] }\n'
    '      finish: { kind: external, from: [OPEN], to: DONE, guards: { titled: deny } }\n'
    '    metrics:\n'
    '      finished:\n'
    '        description: Jobs finished.\n'
    '        source: transitions\n'
    '        item: t\n'
    '        filter: t.to_state == Job.DONE\n'
    '        dimensions:\n'
    '          month: month(t.occurred_at)\n'
    '        time_dimension: t.occurred_at\n'
    '        expression: count()\n'
)
SYNTAX_ASSIGN = (
    'module: desk\n'
    'categories: [live, closed]\n'
    'types:\n'
    '  Agent:\n'
    '    description: An agent.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      login: { type: identity, actor_kind: human }\n'
    '    states:\n'
    '      ON:  { category: live }\n'
    '      OFF: { category: closed, final: true }\n'
    '    transitions:\n'
    '      join:  { kind: initial, to: ON, required_inputs: [login] }\n'
    '      leave: { kind: external, from: [ON], to: OFF }\n'
    '  Ticket:\n'
    '    description: A ticket.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      agent: { reference: Agent, optional: true, assignee: true }\n'
    '    states:\n'
    '      OPEN: { category: live }\n'
    '      DONE: { category: closed, final: true }\n'
    '    transitions:\n'
    '      open:   { kind: initial, to: OPEN, optional_inputs: [agent] }\n'
    '      finish: { kind: external, from: [OPEN], to: DONE }\n'
)
SYNTAX_CREW = (
    'module: crew\n'
    'categories: [live, closed]\n'
    'evaluators:\n'
    '  risk:\n'
    '    description: A risk service.\n'
    '    functions:\n'
    '      ok: { description: Whether the crew may start., arguments: { n: { type: string } } }\n'
    'types:\n'
    '  Job:\n'
    '    description: A job.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      note: { type: string, optional: true }\n'
    '    states:\n'
    '      OPEN: { category: live }\n'
    '      DONE: { category: closed, final: true }\n'
    '    transitions:\n'
    '      open:   { kind: initial, to: OPEN }\n'
    '      prep:   { kind: internal, from: [OPEN], effect: [ { assign: { location: note, expr: \'"x"\' } }, { create: { type: Crew, transition: start, inputs: { n: inputs.n } } } ], inputs: { n: { type: string } } }\n'
    '      finish: { kind: external, from: [OPEN], to: DONE }\n'
    '  Crew:\n'
    '    description: A crew.\n'
    '    tracking: record\n'
    '    states:\n'
    '      UP:   { category: live }\n'
    '      DOWN: { category: closed, final: true }\n'
    '    conditions:\n'
    '      safe: { description: The service says it is safe., expression: risk.ok(inputs.n), remedy: dependent }\n'
    '    transitions:\n'
    '      start: { kind: initial, to: UP, inputs: { n: { type: string } }, guards: { safe: deny } }\n'
    '      stop:  { kind: external, from: [UP], to: DOWN }\n'
)

def syntax_plants():
    """A YAML plant for each of the sixty-four checks the text checker enforces and step 3 does not, so each
    reaches step 4 from a flow description; the core's checker is held to them (ADR-0156 decision 4,
    docs/design/core-checker.md §3). Each changes one thing in a small module clean under every step."""
    def p(text, old, new):
        assert text.count(old) == 1, old
        return text.replace(old, new, 1)
    MINI, PARTS, PAIR, METRIC, ASSIGN, CREW = (SYNTAX_MINI, SYNTAX_PARTS, SYNTAX_PAIR, SYNTAX_METRIC, SYNTAX_ASSIGN,
                                               SYNTAX_CREW)
    FINISH = "      finish: { kind: external, from: [OPEN], to: DONE, guards: { titled: deny } }\n"
    OPEN = "      open:   { kind: initial, to: OPEN, required_inputs: [title], optional_inputs: [note] }\n"
    NOTE = "      note:  { type: string, optional: true }\n"
    DONE = "      DONE: { category: closed, final: true }\n"
    COND = "expression: title is not null"
    START = "      start: { kind: initial, to: ACTIVE, required_inputs: [order], only_via: [Order.add_line] }\n"
    OBSERVED = "    observations:\n      notes:\n        description: Notes on a job.\n        kind: JobNote\n        attributes:\n"
    STATES = "    states:\n      OPEN: { category: live }\n"
    LOGIN = "      login: { type: identity, actor_kind: human }\n"
    AGENT = "      agent: { reference: Agent, optional: true, assignee: true }\n"
    planted = [
        ("a creation that never writes a required attribute", "check 8", p(MINI, NOTE, NOTE + "      owner: { type: string }\n")),
        ("a state with no way out", "check 15", p(p(MINI, DONE, DONE + "      STUCK: { category: live }\n"), FINISH,
                                                  FINISH + "      stick: { kind: external, from: [OPEN], to: STUCK }\n")),
        ("a state nothing reaches", "check 15", p(p(MINI, DONE, DONE + "      LOST: { category: live }\n"), FINISH,
                                                  FINISH + "      leave: { kind: external, from: [LOST], to: DONE }\n")),
        ("a type with no final state", "check 15", p(MINI, DONE, "      DONE: { category: closed }\n")),
        ("a transition leaving a final state", "check 15", p(MINI, FINISH, FINISH + "      reopen: { kind: external, from: [DONE], to: OPEN }\n")),
        ("a type with no creation", "check 34", p(MINI, OPEN, "")),
        ("an internal transition at a final state", "check 40",
         p(MINI, FINISH, FINISH + "      annotate: { kind: internal, from: [DONE], optional_inputs: [note] }\n")),
        ("a clear of a required attribute", "check 17",
         p(MINI, FINISH, "      finish: { kind: external, from: [OPEN], to: DONE, guards: { titled: deny }, effect: [ { clear: [title] } ] }\n")),
        ("a default on an optional input", "check 20",
         p(MINI, FINISH, "      finish: { kind: external, from: [OPEN], to: DONE, guards: { titled: deny }, "
                         "inputs: { why: { type: string, optional: true, default: '\"none\"' } } }\n")),
        ("a counter on a type tracked by record", "check 31", p(MINI, NOTE, NOTE + "      count: { type: counter }\n")),
        ("a type tracked by quantity with no counter", "check 31", p(MINI, "    tracking: record\n", "    tracking: quantity\n")),
        ("an audited guard on an assertion", "check 57",
         p(MINI, FINISH, FINISH + "      force: { kind: assertion, to: [OPEN, DONE], guards: { titled: audit }, inputs: { reason: { type: Reason } } }\n")),
        ("a backdatable assertion", "check 58",
         p(MINI, FINISH, FINISH + "      force: { kind: assertion, to: [OPEN, DONE], backdating_limit: 2 days, inputs: { reason: { type: Reason } } }\n")),
        ("entered_at of a member that is not tracked", "check 58", p(MINI, COND, "expression: entered_at(note) < now")),
        ("an actor kind on a string", "check 59", p(MINI, "      title: { type: string }\n", "      title: { type: string, actor_kind: human }\n")),
        ("two actor identities on one type", "check 62",
         p(MINI, NOTE, NOTE + "      who:   { type: identity, actor_kind: human, optional: true }\n"
                              "      also:  { type: identity, actor_kind: agent, optional: true }\n")),
        ("the actor read in a condition", "check 64", p(MINI, COND, "expression: actor.id is not null")),
        # an expression past the parser's bounds, which once exhausted the core's stack (F1)
        ("an expression deeper than its bound", "syntax", p(MINI, COND, "expression: " + " or ".join(["title is not null"] * 200))),
        ("an expression nested past its bound", "syntax", p(MINI, COND, "expression: " + "(" * 70 + "title is not null" + ")" * 70)),
        ("a part created not only via its whole", "check 11",
         p(PARTS, START, "      start: { kind: initial, to: ACTIVE, required_inputs: [order] }\n")),
        ("an only via whose parent never reaches it", "check 13",
         p(PARTS, START, "      start: { kind: initial, to: ACTIVE, required_inputs: [order], only_via: [Order.finish] }\n")),
        # its converse: a creation from a transition the only via does not name (the audit's write path R-05)
        ("a creation from a transition an only via does not name", "check 13",
         p(PARTS, "      add_line: { kind: internal, from: [OPEN], effect: [ { create: { type: Line, transition: start, inputs: { order: this } } } ] }\n",
           "      add_line: { kind: internal, from: [OPEN], effect: [ { create: { type: Line, transition: start, inputs: { order: this } } } ] }\n"
           "      sneak: { kind: internal, from: [OPEN], effect: [ { create: { type: Line, transition: start, inputs: { order: this } } } ] }\n")),
        ("a part created without its owner", "check 18",
         p(p(PARTS, START, "      start: { kind: initial, to: ACTIVE, only_via: [Order.add_line] }\n"), "inputs: { order: this }", "inputs: {}")),
        ("two singular ends, neither stored", "check 41", p(PAIR, "opposite: desk, stored: true }", "opposite: desk }")),
        ("an observation's field named like a member every observation has", "check 54",
         p(MINI, STATES, OBSERVED + "          subject: { type: string }\n" + STATES)),
        ("a unit on an observation's string field", "check 54",
         p(MINI, STATES, OBSERVED + "          text: { type: string, unit: words }\n" + STATES)),
        ("a kind's invariant scanning its own kind", "check 54",
         p(MINI, STATES, OBSERVED + "          text: { type: string }\n        invariants:\n"
           "          few: { description: Few notes., expression: count(JobNote) < 3 }\n" + STATES)),
        ("a metric's dimension three hops away", "check 56",
         p(METRIC, "          month: month(t.occurred_at)\n", "          month: month(t.occurred_at)\n          far: t.object.a.b.c\n")),
        ("over last on a metric with no window", "check 56",
         p(p(METRIC, "        time_dimension: t.occurred_at\n", ""), COND, "expression: \"metric(finished, over last 30 days) > 0\"")),
        ("a guard binding what is no dimension of a metric", "check 56", p(METRIC, COND, "expression: \"metric(finished, colour := 1) > 0\"")),
        ("metric() outside a guard", "check 56", p(METRIC, "expression: count()", "expression: metric(finished)")),
        ("a metric counting an expression over its rows", "names", p(METRIC, "expression: count()", "expression: count(t)")),
        # a metric's value in a form the core computes over its rows (the audit's values M2)
        ("a metric's value that is a comparison", "names", p(METRIC, "expression: count()", "expression: count() > 1")),
        ("a metric's value reading a row outside an aggregate", "names", p(METRIC, "expression: count()", "expression: t.occurred_at")),
        ("a metric's value nesting an aggregate over its rows", "names",
         p(METRIC, "expression: count()", "expression: \"count(where count() > 1)\"")),
        ("a metric's value that is a choice", "names", p(METRIC, "expression: count()", "expression: if true then count() else 0")),
        # an aggregate over the rows of what it computes over, and a sum of one type (the audit's values M2)
        ("a metric averaging a text", "names", p(METRIC, "expression: count()", "expression: avg(t.held(title))")),
        ("a metric summing times", "names", p(METRIC, "expression: count()", "expression: sum(t.occurred_at)")),
        ("a metric's least flag", "names", p(METRIC, "expression: count()", "expression: min(t.completes)")),
        ("a metric's percentile of a text", "names", p(METRIC, "expression: count()", "expression: percentile(0.5, t.held(title))")),
        ("a metric subtracting an int from a decimal", "names", p(METRIC, "expression: count()", "expression: count() * 1.5 - count()")),
        ("a metric multiplying a time", "names", p(METRIC, "expression: count()", "expression: max(t.occurred_at) * 2")),
        ("a metric averaging a text over objects", "names",
         p(METRIC, "        source: transitions\n        item: t\n        filter: t.to_state == Job.DONE\n        dimensions:\n"
                   "          month: month(t.occurred_at)\n        time_dimension: t.occurred_at\n        expression: count()\n",
           "        source: objects\n        item: j\n        expression: avg(j.title)\n")),
        ("a metric-only function in a condition", "check 56", p(MINI, COND, "expression: median(title) > 0")),
        # on every line but a metric's own, where these were accepted (the values audit's G8)
        ("a time bucket in an attribute's default", "check 56",
         p(MINI, NOTE, "      note:  { type: string, optional: true, default: 'month(now)' }\n")),
        ("a time bucket in a creation's inputs", "check 56", p(CREW, "inputs: { n: inputs.n }", "inputs: { n: month(now) }")),
        ("a time bucket in a loop's filter", "check 56",
         p(MINI, FINISH, FINISH + "      tidy: { kind: internal, from: [OPEN], effect: [ { foreach: { item: k, range: 2, "
                         "where: 'year(now) == \"2026\"', limit: 2, steps: [ { assign: { location: note, expr: '\"x\"' } } ] } } ] }\n")),
        ("an assignee that does not store its value", "check 59",
         p(p(ASSIGN, AGENT, "      agent: { reference: Agent, optional: true, assignee: true, opposite: ticket }\n"), LOGIN,
           LOGIN + "      ticket: { reference: Ticket, optional: true, opposite: agent, stored: true }\n")),
        ("an assignee to a type marking no actor identity", "check 59", p(ASSIGN, LOGIN, "      login: { type: identity }\n")),
        ("a create binding an argument from what its outcome writes, read by an evaluator guard", "check 61",
         p(CREW, "inputs: { n: inputs.n }", "inputs: { n: note }")),
    ]
    ok = True
    # a creation naming its result reaches its only-via child as a bare one does (check 13, D551)
    named = p(PARTS, "inputs: { order: this } } }", "inputs: { order: this }, result: made } }")
    for base, text in (("the plants' modules", MINI), ("parts", PARTS), ("parts whose creation names its result", named), ("a pair", PAIR),
                       ("a metric", METRIC), ("an assignee", ASSIGN), ("an evaluator", CREW)):
        found, _notes = check([("plant.yaml", text)])
        ok &= not found
        if found:
            print(f"  the plants' base module of {base} is not clean: {found[0][3]} {found[0][4]}")
    for name, expect, text in planted:
        found, _notes = check([("plant.yaml", text)])
        hit = [x for x in found if x[3] == expect]
        ok &= bool(hit)
        print(f"  planted {name}: {'caught' if hit else 'missed'} by {expect}" + (f", at plant.yaml:{hit[0][1]}" if hit else f" ({found[:1]})"))
    return ok

def source_digest(modules):
    """The digest a flow change records beside its source reference (ADR-0135): over the modules of the
    change's source, in ascending order of their names' UTF-8 bytes, each as its name, a line feed, its
    length in bytes in decimal, a line feed and its bytes as submitted. A repository's side computes the
    same over the files at the reference, so a published version can be traced to it and checked."""
    names = [n for n, _b in modules]
    if len(set(names)) != len(names):
        raise ValueError("two files declare the same module; a change's source has one file per module")
    h = hashlib.sha256()
    for name, body in sorted(modules, key=lambda m: m[0].encode("utf-8")):
        h.update(name.encode("utf-8") + b"\n" + str(len(body)).encode("ascii") + b"\n" + body)
    return "sha256:" + h.hexdigest()


def module_files(paths):
    """Each file's name in the digest, with its bytes as they are on disk: a description by its `module`, and an
    examples file by the module it is for, followed by `.examples`, which no module's name can be (ADR-0136)."""
    out = []
    for p in paths:
        doc = load(p.read_text())
        out.append((doc["module"] if "module" in doc else doc["examples_for"] + ".examples", p.read_bytes()))
    return out


def examples_self_test(people, service, examples):
    """Each rule of §11 that step 2 or 3 holds an examples file to, proven by planting a mistake that breaks it."""
    ok = True
    plants = [
        ("a remedy the condition does not declare", "names", "declares the remedy",
         "clause: engineer_active, remedy: self_serviceable", "clause: engineer_active, remedy: dependent"),
        ("an alias no earlier step gave", "names", "no earlier step gave",
         "request: { object: ben, transition: leave }", "request: { object: bob, transition: leave }"),
        ("an input the transition does not take", "names", "takes no input",
         "inputs: { engineer: ben, reason: ReassignmentReason.WORKLOAD }", "inputs: { engineer: ben, why: ReassignmentReason.WORKLOAD }"),
        ("an enumeration value written unqualified", "names", "written qualified",
         "inputs: { engineer: ben, reason: ReassignmentReason.WORKLOAD }", "inputs: { engineer: ben, reason: WORKLOAD }"),
        ("an occurred time not written as a timestamp", "names", "'occurred_at' is a timestamp",
         "after: 25 h, occurred_at: now - 1 day }", "after: 25 h, occurred_at: yesterday }"),
        ("a state the transition does not leave its object in", "names", "does not leave an object in",
         "expect: { verdict: applied, state: OPEN }", "expect: { verdict: applied, state: WORKING }"),
        ("a setup that is not declared", "names", "which this file does not declare",
         "    given: two_engineers\n    request: { type: ServiceJob", "    given: three_engineers\n    request: { type: ServiceJob"),
        ("an actor whose type marks no actor identity", "names", "neither `operator` nor",
         "request: { object: job, transition: finish, actor: ana,", "request: { object: job, transition: finish, actor: battery,"),
        ("a clause that is not a guard of the transition", "names", "is not a guard of",
         "clause: no_unresolved_failure, remedy: dependent", "clause: photo_missing, remedy: dependent"),
        ("a request naming both a type and an object", "schema", "not both",
         "request: { object: ben, transition: leave }", "request: { object: ben, type: User, transition: leave }"),
        ("a setup step that leaves out a required input", "names", "leaves out the required input",
         "inputs: { login: ben, name: Ben }", "inputs: { login: ben }"),
    ]
    for name, code, words, old, new in plants:
        if examples.count(old) < 1:
            print(f"  planted {name}: NOT PLANTED, the examples file no longer has the text it replaces")
            ok = False
            continue
        found, _n = check([("people.yaml", people), ("service.yaml", service),
                           ("service.examples.yaml", examples.replace(old, new, 1))])
        hit = [x for x in found if x[3] == code and words in x[4]]
        ok &= bool(hit)
        print(f"  planted {name}: {'caught by ' + code + ', at ' + hit[0][0] + ':' + str(hit[0][1]) + ' ' + hit[0][4][:70] if hit else 'MISSED ' + str(found[:1])}")
    return ok


def example_rule_plants():
    """The rules of §11 the plants above leave unproven, each proven by planting a mistake that breaks it: one plant a
    rule, in the examples of the module it needs, with the module given an input of a kind none of its transitions
    takes where the rule needs one."""
    ok = True
    read = lambda n: (EXAMPLES / n).read_text()
    service = ["people.yaml", "service.yaml", "service.examples.yaml"]
    opens = "    request: { type: ServiceJob, transition: open, as: job, actor: ana, inputs: { engineer: ana } }\n    expect: { verdict: applied, state: OPEN }"
    refused = "    expect: { verdict: unsatisfied, clause: engineer_active, remedy: self_serviceable }"
    leave = "      - request: { object: ben, transition: leave }"
    reassigned = "inputs: { engineer: ben, reason: ReassignmentReason.WORKLOAD }"
    # optional inputs of a set of references and of an actor kind, for the rules on the values written for them
    inputs = ("          reason: { type: ReassignmentReason }\n",
              "          reason: { type: ReassignmentReason }\n          helpers: { reference: \"User[]\", optional: true }\n"
              "          seen_by: { type: actor_kind, optional: true }\n")
    plants = [
        ("examples for a module not checked", "names", "not among the modules checked", service,
         [("service.examples.yaml", "examples_for: service\n", "examples_for: services\n")]),
        ("an examples file's keys out of order", "order", "in this order", service,
         [("service.examples.yaml", "examples_for: service\n\nsetups:", "setups:"),
          ("service.examples.yaml", "\nexamples:\n", "\nexamples_for: service\nexamples:\n")]),
        ("a stub of a function no evaluator declares", "names", "as an evaluator's function", service,
         [("service.examples.yaml", opens, "    evaluators: { nowhere.check: satisfied }\n" + opens)]),
        ("a set of references given as one alias", "names", "given as a list of aliases", service,
         [("service.yaml",) + inputs, ("service.examples.yaml", reassigned, reassigned[:-2] + ", helpers: ana }")]),
        ("an actor kind written as a name", "names", "is an actor_kind", service,
         [("service.yaml",) + inputs, ("service.examples.yaml", reassigned, reassigned[:-2] + ", seen_by: robot }")]),
        ("a bool written as a word", "names", "' is a bool", ["servicedesk.yaml", "servicedesk.examples.yaml"],
         [("servicedesk.examples.yaml", "major: false, responders: [oncall-1] }", "major: maybe, responders: [oncall-1] }")]),
        ("an int written as a word", "names", "' is an int", ["servicedesk.yaml", "servicedesk.examples.yaml"],
         [("servicedesk.examples.yaml", "inputs: { name: Payments, tier: 1,", "inputs: { name: Payments, tier: one,")]),
        ("a creation of a type no module declares", "names", "as a type", service,
         [("service.examples.yaml", opens, opens.replace("type: ServiceJob,", "type: ServiceJobs,"))]),
        ("a creation of an abstract type", "names", "which is abstract", ["servicedesk.yaml", "servicedesk.examples.yaml"],
         [("servicedesk.examples.yaml", "request: { type: Server, transition: receive, inputs: { name: db-01 } }",
           "request: { type: Asset, transition: receive, inputs: { name: db-01 } }")]),
        ("a creation requested on an object", "names", "is a creation, requested with the type", service,
         [("service.examples.yaml", leave, "      - request: { object: ben, transition: add, inputs: { login: bob, name: Bob } }")]),
        ("a transition of an object requested as a creation", "names", "is requested on an object", service,
         [("service.examples.yaml", leave, "      - request: { type: User, transition: leave }")]),
        ("a target state an assertion cannot put its object in", "names", "may put an object in",
         ["people.yaml", "customers.yaml", "customers.examples.yaml"],
         [("customers.examples.yaml", "transition: correct_state, inputs: { to: ACTIVE, reason: OverrideReason.MIS_KEYED } }",
           "transition: correct_state, inputs: { to: NOWHERE, reason: OverrideReason.MIS_KEYED } }")]),
        ("an alias given twice", "names", "is already an alias", service,
         [("service.examples.yaml", "as: ben, inputs: { login: ben,", "as: ana, inputs: { login: ben,")]),
        ("a setup given itself", "names", "is given, through others, itself", service,
         [("service.examples.yaml", "  two_engineers:\n    description:", "  two_engineers:\n    given: two_engineers\n    description:")]),
        ("a setup given one declared after it", "order", "declared after it", service,
         [("service.examples.yaml", "  two_engineers:\n    description:", "  two_engineers:\n    given: job_in_progress\n    description:")]),
        ("a sequence raised that no module declares", "names", "raises the sequence asset_keys, which no module",
         ["servicedesk.yaml", "servicedesk.examples.yaml"],
         [("servicedesk.examples.yaml", "sequences: [{ sequence: asset_key, last: 41 }]", "sequences: [{ sequence: asset_keys, last: 41 }]")]),
        ("a scope given to a sequence numbered in one series", "names", "numbers one series, with no scope",
         ["servicedesk.yaml", "servicedesk.examples.yaml"],
         [("servicedesk.examples.yaml", "sequences: [{ sequence: asset_key, last: 41 }]", "sequences: [{ sequence: asset_key, last: 41, scope: dana }]")]),
        ("a scoped sequence raised with no scope", "names", "with no scope, and Asset.key numbers it in each value of service",
         ["servicedesk.yaml", "servicedesk.examples.yaml"],
         [("servicedesk.yaml", 'identifier: { sequence: asset_key, format: "IT-{n}" }', 'identifier: { sequence: asset_key, scope: service, format: "IT-{n}" }')]),
        ("a sequence's scope that is not an alias of its reference", "names", "'scope' is a reference to RegisteredService",
         ["servicedesk.yaml", "servicedesk.examples.yaml"],
         [("servicedesk.yaml", 'identifier: { sequence: asset_key, format: "IT-{n}" }', 'identifier: { sequence: asset_key, scope: service, format: "IT-{n}" }'),
          ("servicedesk.examples.yaml", "sequences: [{ sequence: asset_key, last: 41 }]", "sequences: [{ sequence: asset_key, last: 41, scope: nobody }]")]),
        ("objects named of a type the request does not create", "names", "and its effect creates only WorkItem, WorkItemLink",
         ["people.yaml", "issues.yaml", "issues.examples.yaml"],
         [("issues.examples.yaml", "creates: { WorkItem: [copy], WorkItemLink: [clone_link] }", "creates: { WorkItem: [copy], Subtask: [clone_link] }")]),
        ("objects named as the request under test creates them", "names", "since no step follows it", service,
         [("service.examples.yaml", opens, opens.replace("inputs: { engineer: ana } }", "inputs: { engineer: ana }, creates: [extra] }"))]),
        ("a proposal's state expected of a request", "names", "only an approval leaves a proposal", service,
         [("service.examples.yaml", opens, opens.replace("state: OPEN }", "state: OPEN, proposal: executed }"))]),
        ("an applied request naming a clause", "names", "an applied request names no clause", service,
         [("service.examples.yaml", opens, opens.replace("state: OPEN }", "state: OPEN, clause: engineer_active }"))]),
        ("an applied request with no state", "names", "states the state its object reaches", service,
         [("service.examples.yaml", opens, opens.replace("verdict: applied, state: OPEN }", "verdict: applied }"))]),
        ("a value of an attribute the type does not declare", "names", "declares no attribute", service,
         [("service.examples.yaml", "values: { signed_off_by_user: ana }", "values: { signed_by: ana }")]),
        ("a cascade to a transition no module declares", "names", "names no transition a module checked declares", service,
         [("service.examples.yaml", opens, opens.replace("state: OPEN }", "state: OPEN, cascaded: [ServiceJob.close] }"))]),
        ("a warning by a condition of no type", "names", "names no condition of a type", service,
         [("service.examples.yaml", opens, opens.replace("state: OPEN }", "state: OPEN, warned: [Nothing.quiet] }"))]),
        ("a refused request expected to leave a state", "names", "leaves no state to state", service,
         [("service.examples.yaml", refused, refused[:-2] + ", state: OPEN }")]),
        ("a refused request naming no clause", "names", "names the clause that refuses it", service,
         [("service.examples.yaml", refused, "    expect: { verdict: unsatisfied }")]),
        ("a request not only via others, expected to be not requestable", "names", "never refused as not_requestable", service,
         [("service.examples.yaml", opens, opens.replace("verdict: applied, state: OPEN }", "verdict: not_requestable }"))]),
        ("a violated invariant not named", "names", "names the invariant", service,
         [("service.examples.yaml", refused, "    expect: { verdict: invariant_violated }")]),
        ("a violated invariant the type does not declare", "names", "is not an invariant of", service,
         [("service.examples.yaml", refused, "    expect: { verdict: invariant_violated, clause: always_right }")]),
        ("a refused call naming no clause of a type", "names", "is not a condition, invariant or generated guard", service,
         [("service.examples.yaml", refused, "    expect: { verdict: call_refused, clause: nothing }")]),
        ("invalid input naming a clause", "names", "names no clause or remedy", service,
         [("service.examples.yaml", refused, "    expect: { verdict: invalid_input, clause: engineer_active }")]),
    ]
    for name, code, words, files, edits in plants:
        texts = {f: read(f) for f in files}
        planted = True
        for f, old, new in edits:
            planted &= texts[f].count(old) == 1
            texts[f] = texts[f].replace(old, new, 1)
        if not planted:
            print(f"  planted {name}: NOT PLANTED, a file no longer has the text it replaces")
            ok = False
            continue
        found, _notes = check([(f, texts[f]) for f in files])
        hit = [x for x in found if x[3] == code and words in x[4]]
        ok &= bool(hit)
        print(f"  planted {name}: {'caught by ' + code + ', at ' + hit[0][0] + ':' + str(hit[0][1]) + ' ' + hit[0][4][:70] if hit else 'MISSED ' + str(found[:2])}")
    # a publish its dry run refuses fails its example, the version beside the examples file given a guard on no
    # condition (§11.3, ADR-0166 decision 3)
    broken = read("service-v2.yaml").replace("photo_attached: deny", "photo_attachd: deny", 1)
    migration = ["people.yaml", "service.yaml", "service-migration.examples.yaml"]
    found, _notes = check([(f, read(f)) for f in migration], None, None, None,
                          lambda n: broken if n == "service-v2.yaml" else beside(EXAMPLES)(n))
    hit = [x for x in found if x[3] == "example" and "refused at submission" in x[4]]
    ok &= bool(hit)
    print(f"  planted a publish its dry run refuses: {'caught by example, at ' + hit[0][0] + ':' + str(hit[0][1]) if hit else 'MISSED ' + str(found[:2])}")
    return ok


def logic_plants(people, service):
    """Step 2's grammar and step 5's analysis, each shown on the service example by planting what it refuses."""
    ok = True
    plants = [
        ("an expression the grammar refuses", "syntax",
         "        expression: inputs.photo is not null\n", "        expression: inputs.photo is not null is null\n"),
        ("guards of one transition that cannot hold together", "contradiction",
         "          photo_attached: audit\n", "          photo_attached: deny\n          no_photo: deny\n"),
        ("a warning that fails whenever the transition is taken", "neverpasses",
         "          photo_attached: audit\n", "          photo_attached: deny\n          no_photo: warn\n"),
    ]
    extra = ("      no_photo:\n        description: No photo is attached.\n        expression: inputs.photo is null\n"
             "        remedy: self_serviceable\n")
    for name, code, old, new in plants:
        text = service.replace(old, new, 1)
        if code != "syntax":
            text = text.replace("      photo_attached:\n", extra + "      photo_attached:\n", 1)
        found, notes = check([("people.yaml", people), ("service.yaml", text)])
        hit = [x for x in found + notes if x[3] == code]
        ok &= bool(hit)
        print(f"  planted {name}: {'caught by ' + code + ', at ' + hit[0][0] + ':' + str(hit[0][1]) + ' ' + hit[0][4][:70] if hit else 'MISSED ' + str((found + notes)[:2])}")
    return ok


def runner_plants(people, service, examples):
    """Step 6 shown on the service example: an expectation the rules contradict fails, a guard reading a metric is
    decided over the example's own history, and a construct the runner does not yet execute is reported as not run,
    never passed."""
    ok = True

    def guarded(line, guard, expression, remedy):
        """The service module with one more guard on a transition: after the line of a guard it has, or opening its
        guards after the line of another of its keys."""
        added = f"          {guard}: deny\n" if line.startswith(" " * 10) else f"        guards:\n          {guard}: deny\n"
        text = service.replace(line, line + added, 1)
        return text.replace("      photo_attached:\n", f"      {guard}:\n        description: A planted guard.\n"
                                                     f"        expression: {expression}\n        remedy: {remedy}\n      photo_attached:\n", 1)
    plants = [
        ("a repeated check that fails again, expected to let the job finish", "example", None,
         ("outcome: CheckOutcome.PASS }", "outcome: CheckOutcome.FAIL }")),
        ("a signer the request does not write", "example", None,
         ("values: { signed_off_by_user: ana }", "values: { signed_off_by_user: ben }")),
        ("a setup whose second engineer reuses a unique login", "example", None,
         ("inputs: { login: ben, name: Ben }", "inputs: { login: ana, name: Ben }")),
        # the design's own pass-rate guard (declaration-syntax.md §6.9) on finish: a failure inspected again still counts,
        # so only the job that repeated its check falls below the threshold; a corrected failure no longer counts
        ("the pass-rate guard on finish, which a failure inspected again keeps below its threshold", "example",
         guarded("          photo_attached: audit\n", "pass_rate",
                 "metric(inspection_pass_rate, engineer := engineer, over last 30 days) >= 0.900", "dependent"), None),
        ("a guard reading a metric that has no rows yet, and so is unknown", "example",
         guarded("        backdating_limit: 2 days\n", "quick", "metric(time_working) < 30 days", "dependent"), None),
        # a guard reading a metric whose value is an amount in a currency whose minor unit neither the runner nor the
        # core knows, so neither can round it (declaration-syntax.md §8.3); the guard names no amount itself, which
        # the typing refuses in such a currency (the audit's values L6), and a metric's expression is not yet typed
        ("an example the runner cannot yet run", "notrun",
         guarded("        backdating_limit: 2 days\n", "calm", "metric(priced) <= metric(priced)", "dependent").replace(
             "    metrics:\n", "    metrics:\n      priced:\n        description: Jobs priced in pesos.\n"
                           "        source: objects\n        item: o\n        expression: count() * PHP 1.00\n", 1), None),
    ]
    for name, code, module, edit in plants:
        found, notes = check([("people.yaml", people), ("service.yaml", module or service),
                              ("service.examples.yaml", examples.replace(*edit, 1) if edit else examples)])
        hit = [x for x in found + notes if x[3] == code]
        ok &= bool(hit)
        print(f"  planted {name}: {'caught by ' + code + ', at ' + hit[0][0] + ':' + str(hit[0][1]) + ' ' + hit[0][4][:70] if hit else 'MISSED ' + str((found + notes)[:2])}")
        if name.startswith("the pass-rate guard"):
            # only the repeated check fails, and the correction, which leaves one result, passes
            alone = [x[4].split(":")[0] for x in found] == ["a_repeated_check_lets_the_job_finish"]
            ok &= alone
            print(f"  the pass-rate guard fails that example alone, and passes the one whose failure was corrected: {'yes' if alone else 'NO ' + str([x[4][:60] for x in found])}")
    return ok


def inventory_plants(people):
    """Slice 5 shown on the inventory and delivery examples: an expected warning the request does not raise, or one it
    raises and the example leaves out, a guard that does not warn named as warning, an invariant's remedy the rule does
    not give, a left-out input of an optional attribute, refused by its generated guard, expected to apply, a sign-off
    expected to hold after what it signed changed, and objects a step names that its request does not create."""
    ok = True
    v1, v2 = (EXAMPLES / "inventory.yaml").read_text(), (EXAMPLES / "inventory-v2.yaml").read_text()
    ex1, ex2 = (EXAMPLES / "inventory.examples.yaml").read_text(), (EXAMPLES / "inventory-v2.examples.yaml").read_text()
    plants = [
        ("a warning the example leaves out", "example", v2, ex2, "warned: [not_below_list_price] }", "warned: [] }"),
        ("a warning the request does not raise", "example", v2, ex2, "SGD 12000.00 }, warned: [] }",
         "SGD 12000.00 }, warned: [not_below_list_price] }"),
        ("a guard that denies, named as warning", "names", v2, ex2, "warned: [not_below_list_price] }", "warned: [not_damaged] }"),
        ("an invariant's remedy the rule does not give", "example", v1, ex1, "clause: serial_unique, remedy: self_serviceable }",
         "clause: serial_unique, remedy: dependent }"),
        ("a left-out input of an optional attribute, expected to apply", "names", v1, ex1,
         "expect: { verdict: unsatisfied, clause: recipient_provided, remedy: self_serviceable }",
         "expect: { verdict: applied, state: RESERVED }"),
    ]
    delivery, dex = (EXAMPLES / "delivery.yaml").read_text(), (EXAMPLES / "delivery.examples.yaml").read_text()
    stale = "      - request: { object: manual, transition: check }\n    request: { object: delivery, transition: complete }\n"
    plants += [
        ("a sign-off expected to hold after the checklist it signed changed", "example", delivery, dex,
         stale + "    expect: { verdict: unsatisfied, clause: signed_off, remedy: delegable }",
         stale + "    expect: { verdict: applied, state: DELIVERED }"),
        ("a step naming more objects than its request creates", "example", delivery, dex,
         "creates: [battery, manual]", "creates: [battery, manual, spare]"),
        ("a setup's step requesting a transition only via another", "names", delivery, dex,
         "      - request: { object: battery, transition: check }\n      - request: { object: manual, transition: check }\n      - request: { type: Courier",
         "      - request: { object: battery, transition: discard }\n      - request: { object: manual, transition: check }\n      - request: { type: Courier"),
        ("a step naming objects a transition that creates none creates", "names", delivery, dex,
         "      - request: { object: delivery, transition: assign_courier, inputs: { courier: courier } }\n\n  an_approved",
         "      - request: { object: delivery, transition: assign_courier, inputs: { courier: courier }, creates: [slip] }\n\n  an_approved"),
    ]
    # each way an expectation can be wrong that the plants above leave unreached, each refused by the runner
    service, sex = (EXAMPLES / "service.yaml").read_text(), (EXAMPLES / "service.examples.yaml").read_text()
    customers, cex = (EXAMPLES / "customers.yaml").read_text(), (EXAMPLES / "customers.examples.yaml").read_text()
    approvals, aex = (EXAMPLES / "approvals.yaml").read_text(), (EXAMPLES / "approvals.examples.yaml").read_text()
    plants += [
        ("a refusal expected as another verdict", "example", service, sex,
         "    expect: { verdict: unsatisfied, clause: engineer_active, remedy: self_serviceable }", "    expect: { verdict: unavailable }"),
        ("a cascade expected to reach fewer objects than it does", "example", delivery, dex,
         "cascaded: [ChecklistItem.discard, ChecklistItem.discard] }", "cascaded: [ChecklistItem.discard] }"),
        ("a rejected proposal expected withdrawn", "example", delivery, dex,
         "    expect: { verdict: unavailable, proposal: rejected }", "    expect: { verdict: unavailable, proposal: withdrawn }"),
        ("an amount expected other than the one recorded", "example", approvals, aex,
         "values: { amount: SGD 42.50 } }", "values: { amount: SGD 42.05 } }"),
        ("a start expected dated a day earlier than it is", "example", service, sex,
         "state: WORKING, occurred_at: now - 1 day }", "state: WORKING, occurred_at: now - 2 days }"),
        ("a recorded time expected a day earlier than it is", "example", service, sex,
         "values: { occurred_at: now - 2 days } }", "values: { occurred_at: now - 3 days } }"),
        ("a derived flag expected false of a job signed off", "example", service, sex,
         "values: { signed_off_by_user: ana } }", "values: { signed_off_by_user: ana, signed_off: false } }"),
        ("a derived count expected of a job never handed off", "example", service, sex,
         "inputs: { engineer: ana } }\n    expect: { verdict: applied, state: OPEN }",
         "inputs: { engineer: ana } }\n    expect: { verdict: applied, state: OPEN, values: { handoffs: 1 } }"),
        ("a metric expected to read another value", "example", service, sex,
         "ServiceJob.handoffs_per_job: [{ month: 2026-01, value: 1 }]", "ServiceJob.handoffs_per_job: [{ month: 2026-01, value: 2 }]"),
        ("a metric expected to read a row it does not", "example", service, sex,
         "ServiceJob.work_in_progress: [{ state: WORKING, value: 1 }]",
         "ServiceJob.work_in_progress: [{ state: WORKING, value: 1 }, { state: OPEN, value: 0 }]"),
        ("a metric expected that no module declares", "example", service, sex,
         "ServiceJob.transition_counts: 3", "ServiceJob.transition_count: 3"),
        ("an override expected to leave its object in another of its targets", "example", customers, cex,
         "admits: [active_invariant] }\n    expect: { verdict: applied, state: ACTIVE }",
         "admits: [active_invariant] }\n    expect: { verdict: applied, state: DORMANT }"),
    ]
    # one planted mistake in each of the other examples files, each refused by the runner
    for mod, old, new in [("approvals", "values: { receipt: taxi.pdf } }", "values: { receipt: other.pdf } }"),
                          ("issues", "expect: { verdict: applied, state: IN_PROGRESS, warned: [not_blocked] }",
                           "expect: { verdict: applied, state: IN_PROGRESS, warned: [] }"),
                          ("servicedesk", "clause: reserved_bounded, remedy: self_serviceable }", "clause: reserved_bounded, remedy: unreachable_from_here }")]:
        plants.append((f"a wrong expectation in the {mod} examples", "example", (EXAMPLES / f"{mod}.yaml").read_text(),
                       (EXAMPLES / f"{mod}.examples.yaml").read_text(), old, new))
    for name, code, module, examples, old, new in plants:
        assert examples.count(old) == 1, old
        found, _notes = check([("people.yaml", people)] + ([("inventory.yaml", v1)] if module is delivery else [])
                              + [("module.yaml", module), ("module.examples.yaml", examples.replace(old, new))])
        hit = [x for x in found if x[3] == code]
        ok &= bool(hit)
        print(f"  planted {name}: {'caught by ' + code + ', at ' + hit[0][0] + ':' + str(hit[0][1]) + ' ' + hit[0][4][:70] if hit else 'MISSED ' + str(found[:2])}")
    return ok


def unknown_plants(people):
    """The notice `unknown` shown on the issues and inventory examples: D481's and D482's guards as they stood are
    reported, and an optional attribute every source state requires is not."""
    ok = True
    issues, inventory = (EXAMPLES / "issues.yaml").read_text(), (EXAMPLES / "inventory.yaml").read_text()

    def unknown(module_name, text):
        _found, notes = check([("people.yaml", people), (module_name, text)])
        return [n for n in notes if n[3] == "unknown"]
    cases = [
        ("an estimate compared with one that may be absent (D481)", issues,
         "expression: story_points is null or inputs.story_points != story_points", "expression: inputs.story_points != story_points",
         "points_changed"),
        ("a date compared with one no transition writes (D482)", issues,
         "expression: inputs.due_date <= inputs.start_date + 3 days", "expression: inputs.due_date <= start_date + 3 days",
         "due_within_window"),
        ("a sale price read when a delivery is cancelled, which DEVELOPMENT does not require", inventory,
         "          reservation_ends_in_future: deny\n        effect:\n          - clear: [sale_price, development_use]\n",
         "          reservation_ends_in_future: deny\n          priced: deny\n        effect:\n          - clear: [sale_price, development_use]\n",
         "priced"),
    ]
    priced = "    conditions:\n      priced:\n        description: Planted.\n        expression: sale_price > list_price\n        remedy: dependent\n"
    for name, text, old, new, guard in cases:
        assert text.count(old) == 1, old
        planted = text.replace(old, new, 1)
        if guard == "priced":
            planted = planted.replace("    conditions:\n", priced, 1)
        hit = [n for n in unknown("module.yaml", planted) if n[4].startswith(guard)]
        ok &= bool(hit)
        print(f"  planted {name}: {'reported by unknown, at ' + str(hit[0][1]) + ' ' + hit[0][4][:70] if hit else 'MISSED'}")
    # the same read on accept_return, from SOLD alone, which requires the sale price: present, so not reported
    planted = inventory.replace("    conditions:\n", priced, 1).replace(
        "        effect:\n          - clear: [recipient, sale_price]\n",
        "        guards:\n          priced: deny\n        effect:\n          - clear: [recipient, sale_price]\n", 1)
    quiet = not [n for n in unknown("module.yaml", planted) if n[4].startswith("priced")]
    ok &= quiet
    print(f"  the same read on a transition from a state that requires the attribute is not reported: {'yes' if quiet else 'NO'}")
    return ok


def reference_plants(people):
    """A qualified name whose type nothing declares, a guard reading what its own transition writes, and a call whose
    target is a loop's item or a path from it, shown on the service and issues examples."""
    ok = True
    service, issues = (EXAMPLES / "service.yaml").read_text(), (EXAMPLES / "issues.yaml").read_text()
    active = "expression: inputs.engineer.state == User.ACTIVE"
    loop_call = "                - call: { target: w, transition: plan, inputs: { sprint: inputs.next } }\n"
    cases = [
        ("a state of a type nothing declares or imports", "names", service, active, "expression: inputs.engineer.state == Usr.ACTIVE"),
        ("a guard reading the engineer its transition assigns", "before", service, active, "expression: engineer.state == User.ACTIVE"),
        ("a call on a loop's item without the input its transition requires", "names", issues, loop_call,
         "                - call: { target: w, transition: plan }\n"),
        ("a call on a path from a loop's item to a transition its type lacks", "names", issues, loop_call,
         "                - call: { target: w.epic, transition: plan, inputs: { sprint: inputs.next } }\n"),
    ]
    for name, code, text, old, new in cases:
        assert text.count(old) == 1, old
        found, notes = check([("people.yaml", people), ("module.yaml", text.replace(old, new, 1))])
        hit = [x for x in found + notes if x[3] == code and x[0] == "module.yaml"]
        ok &= bool(hit)
        print(f"  planted {name}: {'caught by ' + code + ', at ' + str(hit[0][1]) + ' ' + hit[0][4][:70] if hit else 'MISSED'}")
    # comparing the attribute with the input it is given reads the old value on purpose, as points_changed does
    found, notes = check([("people.yaml", people), ("module.yaml", service.replace(
        active, "expression: engineer != inputs.engineer and inputs.engineer.state == User.ACTIVE", 1))])
    quiet = not [x for x in notes if x[3] == "before"] and not found
    ok &= quiet
    print(f"  a guard comparing an attribute with the input it is given is not noticed: {'yes' if quiet else 'NO'}")
    return ok


def split_plants(people):
    """A metric that splits its spans across months (ADR-0142), planted in the service example: clean as written, and
    refused with a time dimension beside the split, reading a piece's period without one, or splitting objects."""
    ok = True
    service = (EXAMPLES / "service.yaml").read_text()
    anchor = "      inspection_pass_rate:\n"
    split = ("      working_by_month:\n        description: Planted.\n        source: intervals\n        item: i\n        split_by: month\n"
             "        filter: i.state == WORKING\n        dimensions:\n          month: i.period\n        expression: sum(i.duration)\n")
    assert service.count(anchor) == 1
    planted = service.replace(anchor, split + anchor, 1)
    found, _notes = check([("people.yaml", people), ("service.yaml", planted)])
    clean = not found
    ok &= clean
    print(f"  a metric splitting its spans by month is checked clean: {'yes' if clean else [x[4][:80] for x in found]}")
    cases = [
        ("a split metric that declares a time dimension", planted.replace("        expression: sum(i.duration)\n",
                                                                          "        time_dimension: i.entered_at\n        expression: sum(i.duration)\n", 1)),
        ("a piece's period read without a split", planted.replace("        split_by: month\n", "", 1)),
        ("a split of objects", service.replace("        source: objects\n        item: o\n", "        source: objects\n        item: o\n        split_by: month\n", 1)),
    ]
    for name, text in cases:
        found, _notes = check([("people.yaml", people), ("service.yaml", text)])
        hit = [x for x in found if x[3] == "names"]
        ok &= bool(hit)
        print(f"  planted {name}: {'caught by names, at ' + str(hit[0][1]) + ' ' + hit[0][4][:70] if hit else 'MISSED ' + str(found[:1])}")
    return ok


# a requisition approved into a budget year read through its cost centre: what a path of required references
# reaches is provably present (question 8 of the procurement review, procurement-cold-draft.md §5)
BUDGET_PATH = (
    'module: budget\n'
    'categories: [live, closed]\n'
    'types:\n'
    '  CostCentre:\n'
    '    description: A cost centre.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      fiscal_year: { type: int }\n'
    '      note:        { type: string, optional: true }\n'
    '    states:\n'
    '      OPEN:   { category: live }\n'
    '      CLOSED: { category: closed, final: true }\n'
    '    transitions:\n'
    '      open:  { kind: initial, to: OPEN, required_inputs: [fiscal_year] }\n'
    '      close: { kind: external, from: [OPEN], to: CLOSED }\n'
    '  Requisition:\n'
    '    description: A requisition.\n'
    '    tracking: record\n'
    '    attributes:\n'
    '      cost_centre:   { reference: CostCentre }\n'
    '      backup_centre: { reference: CostCentre, optional: true }\n'
    '      budget_year:   { type: int, optional: true }\n'
    '      budget_note:   { type: string, optional: true }\n'
    '    states:\n'
    '      DRAFT:    { category: live }\n'
    '      APPROVED: { category: closed, final: true, required_attributes: [budget_year] }\n'
    '    transitions:\n'
    '      raise:   { kind: initial, to: DRAFT, required_inputs: [cost_centre] }\n'
    '      approve: { kind: external, from: [DRAFT], to: APPROVED, effect: [ { assign: { location: budget_year, expr: cost_centre.fiscal_year } } ] }\n'
)


# who approved, recorded as `actor.id` into an identity a metric reads by, whose login is also the user's alias in
# its examples (D565, D566, D568)
IDENTITY_FLOW = """module: approve
categories: [live, closed]

types:
  User:
    description: A person who may act.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states:
      ACTIVE: { description: Working., category: live }
      LEFT:   { description: Gone., category: closed, final: true }
    transitions:
      add:   { kind: initial, to: ACTIVE, required_inputs: [login] }
      leave: { kind: external, from: ACTIVE, to: LEFT }

  Request:
    description: A request a person approves.
    tracking: record
    attributes:
      title:       { type: string }
      approved_by: { type: identity, optional: true }
      approved_at: { type: timestamp, optional: true }
    states:
      OPEN:     { description: Waiting., category: live }
      APPROVED: { description: Approved., category: closed, final: true, required_attributes: [approved_by] }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [title] }
      approve:
        kind: external
        from: OPEN
        to: APPROVED
        effect:
          - assign: { location: approved_by, expr: actor.id }
          - assign: { location: approved_at, expr: now }
    metrics:
      approvals_by_approver:
        description: Approvals by who approved, by month.
        source: objects
        item: r
        filter: r.approved_by is not null
        dimensions:
          approver: r.approved_by
        time_dimension: r.approved_at
        expression: count()
"""
IDENTITY_EXAMPLES = """examples_for: approve

setups:
  ana_and_a_request:
    description: Ana, whose login is her alias, and an open request.
    steps:
      - request: { type: User, transition: add, as: ana, inputs: { login: ana } }
      - request: { type: Request, transition: open, as: request, inputs: { title: Laptop } }

examples:
  an_approval_is_counted_by_its_approver:
    description: Ana approving the request counts one approval under her login, which is also her alias.
    given: ana_and_a_request
    request: { object: request, transition: approve, actor: ana }
    expect:
      verdict: applied
      state: APPROVED
      values: { approved_by: ana }
      metrics:
        Request.approvals_by_approver: [{ approver: ana, value: 1 }]
"""


TAKEN_FLOW = """module: taken
categories: [live, closed]

types:
  User:
    description: A person who may act.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states:
      ACTIVE: { description: Working., category: live }
      LEFT:   { description: Gone., category: closed, final: true }
    transitions:
      add:   { kind: initial, to: ACTIVE, required_inputs: [login] }
      leave: { kind: external, from: ACTIVE, to: LEFT }

  Agent:
    description: An agent that drafts and proposes.
    tracking: record
    attributes:
      key: { type: identity, actor_kind: agent, unique: true }
    states:
      ACTIVE:  { description: Working., category: live }
      RETIRED: { description: Retired., category: closed, final: true }
    transitions:
      issue:  { kind: initial, to: ACTIVE, required_inputs: [key] }
      retire: { kind: external, from: ACTIVE, to: RETIRED }

  Request:
    description: A request a person decides.
    tracking: record
    attributes:
      title: { type: string }
    states:
      OPEN:     { description: Waiting., category: live }
      APPROVED: { description: Approved., category: closed, final: true }
      REJECTED: { description: Rejected., category: closed, final: true }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [title] }
      retitle: { kind: internal, from: OPEN, required_inputs: [title] }
      approve: { kind: external, from: OPEN, to: APPROVED, taken_by: [human], proposable: true }
      reject:  { kind: external, from: OPEN, to: REJECTED, taken_by: [human, service] }
"""

# TAKEN_FLOW with an erasure and an assertion a person takes (D579)
WHO_FLOW = TAKEN_FLOW.replace("categories: [live, closed]\n", "categories: [live, closed]\n\nenumerations:\n  Why: [ERROR]\n").replace(
    "      title: { type: string }",
    "      title: { type: string }\n      requester: { type: string, personal: true, optional: true }").replace(
    "      open: { kind: initial, to: OPEN, required_inputs: [title] }",
    "      open: { kind: initial, to: OPEN, required_inputs: [title], optional_inputs: [requester] }").replace(
    "      reject:  { kind: external, from: OPEN, to: REJECTED, taken_by: [human, service] }",
    "      reject:  { kind: external, from: OPEN, to: REJECTED, taken_by: [human, service] }\n"
    "      forget: { kind: erasure, taken_by: [human], inputs: { reason: { type: string } } }\n"
    "      force: { kind: assertion, to: [APPROVED], taken_by: [human], inputs: { reason: { type: Why } } }")

# TAKEN_FLOW with the request's transitions a person's unless one says otherwise: the type's taken_by, which retitle
# opens to agents and reject keeps its own (ADR-0182 decision 5)
TYPE_TAKEN_FLOW = TAKEN_FLOW.replace(
    "    description: A request a person decides.\n    tracking: record\n",
    "    description: A request a person decides.\n    tracking: record\n    taken_by: [human]\n").replace(
    "      retitle: { kind: internal, from: OPEN, required_inputs: [title] }",
    "      retitle: { kind: internal, from: OPEN, required_inputs: [title], taken_by: [human, agent] }")

TYPE_TAKEN_EXAMPLES = """examples_for: taken

setups:
  a_request:
    description: Ana, a person, the scout, an agent, and an open request.
    steps:
      - request: { type: User, transition: add, as: ana, inputs: { login: ana } }
      - request: { type: Agent, transition: issue, as: scout, inputs: { key: scout } }
      - request: { type: Request, transition: open, as: request, inputs: { title: Laptop } }

examples:
  the_agent_may_not_open_one:
    description: Opening a request declares no taken_by, so it takes the type's, a person's only.
    given: a_request
    request: { type: Request, transition: open, actor: scout, inputs: { title: Desk } }
    expect: { verdict: invariant_violated, clause: open_taken_by, remedy: delegable }

  the_agent_may_retitle:
    description: Retitling is opened to agents by its own taken_by.
    given: a_request
    request: { object: request, transition: retitle, actor: scout, inputs: { title: Desk } }
    expect: { verdict: applied, state: OPEN, values: { retitle_by_kind: agent } }

  a_service_rejects:
    description: Reject keeps its own taken_by, which a service may take; a person may too.
    given: a_request
    request: { object: request, transition: reject, actor: ana }
    expect: { verdict: applied, state: REJECTED, values: { reject_by_kind: human } }
"""

TAKEN_EXAMPLES = """examples_for: taken

setups:
  a_request:
    description: Ana, a person, the scout, an agent, and an open request.
    steps:
      - request: { type: User, transition: add, as: ana, inputs: { login: ana } }
      - request: { type: Agent, transition: issue, as: scout, inputs: { key: scout } }
      - request: { type: Request, transition: open, as: request, inputs: { title: Laptop } }

examples:
  a_person_approves:
    description: Ana approves the request, and the kind that took it is recorded.
    given: a_request
    request: { object: request, transition: approve, actor: ana }
    expect: { verdict: applied, state: APPROVED, values: { approve_by_kind: human } }

  the_agent_may_not_approve:
    description: The scout approving is refused by what taken_by generates, and told a person may.
    given: a_request
    request: { object: request, transition: approve, actor: scout }
    expect: { verdict: invariant_violated, clause: approve_taken_by, remedy: delegable }

  the_agent_proposes_and_a_person_approves:
    description: The scout proposes the approval, and Ana's approval executes it as hers.
    given: a_request
    steps:
      - propose: { object: request, transition: approve, actor: scout, proposal: decision }
    approve: { proposal: decision, actor: ana }
    expect: { verdict: applied, state: APPROVED, values: { approve_by_kind: human }, proposal: executed }

  the_agent_may_retitle:
    description: A transition with no taken_by is anyone's.
    given: a_request
    request: { object: request, transition: retitle, actor: scout, inputs: { title: Desk } }
    expect: { verdict: applied, state: OPEN }
"""

WHO_EXAMPLES = TAKEN_EXAMPLES.replace("inputs: { title: Laptop } }", "inputs: { title: Laptop, requester: Zed } }") + """
  the_agent_may_not_erase:
    description: The scout erasing the requester is refused, as only a person erases (D579).
    given: a_request
    request: { object: request, transition: forget, actor: scout, inputs: { reason: asked } }
    expect: { verdict: invariant_violated, clause: forget_taken_by, remedy: delegable }

  a_person_erases:
    description: Ana erases the requester, and the kind that erased is recorded.
    given: a_request
    request: { object: request, transition: forget, actor: ana, inputs: { reason: asked } }
    expect: { verdict: applied, state: OPEN, values: { requester: null, forget_by_kind: human } }

  the_agent_may_not_force:
    description: The scout's override is refused, as only a person overrides.
    given: a_request
    request: { object: request, transition: force, actor: scout, inputs: { reason: Why.ERROR, to: APPROVED } }
    expect: { verdict: invariant_violated, clause: force_taken_by, remedy: delegable }

  a_person_forces:
    description: Ana's override applies.
    given: a_request
    request: { object: request, transition: force, actor: ana, inputs: { reason: Why.ERROR, to: APPROVED } }
    expect: { verdict: applied, state: APPROVED }

  a_retired_agent_does_not_act:
    description: The scout, retired in the store, is refused what it could take before (ADR-0182 decision 6).
    given: a_request
    steps:
      - request: { object: scout, transition: retire }
    request: { object: request, transition: retitle, actor: scout, inputs: { title: Desk } }
    expect: { verdict: unsatisfied, clause: actor_active, remedy: dependent }
"""


def taken_by_plants():
    """A transition taken_by kinds of actor generates the record of who took it and the invariant over it: clean,
    with a person approving, the agent refused as delegable, and the agent's proposal executed as the approver's; an
    erasure and an assertion marked too, the agent's refused and a person's applied (D579); an assertion admitting
    what taken_by generates, a generated name declared, a reference's column declared (D578), and an example
    expecting the agent's approval to apply, each caught (ADR-0176 decision 1)."""
    ok = True
    found, _notes = check([("taken.yaml", TAKEN_FLOW), ("taken.examples.yaml", TAKEN_EXAMPLES)])
    ok &= not found
    print(f"  taken_by, a person approving and the agent refused or proposing: "
          f"{'clean' if not found else [x[4][:80] for x in found]}")
    found, _notes = check([("taken.yaml", WHO_FLOW), ("taken.examples.yaml", WHO_EXAMPLES)])
    ok &= not found
    print(f"  taken_by on an erasure and an assertion, the agent's refused and a person's applied: "
          f"{'clean' if not found else [x[4][:80] for x in found]}")
    found, _notes = check([("taken.yaml", TYPE_TAKEN_FLOW), ("taken.examples.yaml", TYPE_TAKEN_EXAMPLES)])
    ok &= not found
    print(f"  a type's taken_by, which a transition declaring none takes: "
          f"{'clean' if not found else [x[4][:80] for x in found]}")
    cases = [
        ("an assertion admitting what taken_by generates", WHO_FLOW.replace(
            "      force: { kind: assertion, to: [APPROVED], taken_by: [human], inputs: { reason: { type: Why } } }",
            "      force: { kind: assertion, to: [APPROVED], taken_by: [human], inputs: { reason: { type: Why } } }\n"
            "      lax: { kind: assertion, to: [APPROVED], may_admit: [approve_taken_by], "
            "inputs: { reason: { type: Why } } }"),
         None, "names"),
        ("a type's taken_by generating a name it declares", TYPE_TAKEN_FLOW.replace(
            "      title: { type: string }", "      title: { type: string }\n      open_by_kind: { type: actor_kind, optional: true }"),
         None, "names"),
        ("a type's taken_by out of its place", TYPE_TAKEN_FLOW.replace(
            "    tracking: record\n    taken_by: [human]\n", "    taken_by: [human]\n    tracking: record\n"),
         None, "order"),
        ("a reference's column declared", TAKEN_FLOW.replace(
            "      title: { type: string }",
            "      title: { type: string }\n      owner: { reference: User }\n"
            "      owner_id: { type: string, optional: true }"),
         None, "names"),
        ("a generated name declared", TAKEN_FLOW.replace(
            "      title: { type: string }", "      title: { type: string }\n      approve_by_kind: { type: actor_kind, optional: true }"),
         None, "names"),
        ("the agent's approval expected to apply", TAKEN_FLOW, TAKEN_EXAMPLES.replace(
            "expect: { verdict: invariant_violated, clause: approve_taken_by, remedy: delegable }",
            "expect: { verdict: applied, state: APPROVED }"), "example"),
    ]
    for name, text, examples, code in cases:
        assert text != TAKEN_FLOW or examples != TAKEN_EXAMPLES, name
        files = [("taken.yaml", text)] + ([("taken.examples.yaml", examples)] if examples else [])
        found, _notes = check(files)
        hit = [x for x in found if x[3] == code]
        ok &= bool(hit)
        print(f"  planted {name}: {'caught by ' + code if hit else 'MISSED ' + str(found[:2])}")
    return ok


SUPPLY_FLOW = """module: supply
categories: [live, closed]

types:
  Supplier:
    description: A supplier as the ERP records it; only the import writes it.
    mirror: true
    tracking: record
    attributes:
      code: { type: string, external: erp }
      name: { type: string, optional: true }
    states:
      ACTIVE:  { description: Trading., category: live }
      BLOCKED: { description: Blocked by the ERP., category: live }
      CLOSED:  { description: No longer a supplier., category: closed, final: true }

  Quote:
    description: A supplier's quote, accepted while the supplier may trade.
    tracking: record
    attributes:
      supplier: { reference: Supplier }
      amount:   { type: int }
    states:
      OPEN:     { description: Received., category: live }
      ACCEPTED: { description: Accepted., category: closed, final: true }
    conditions:
      supplier_trading:
        description: The ERP has not blocked the supplier.
        expression: supplier.state == Supplier.ACTIVE
        remedy: dependent
    transitions:
      receive: { kind: initial, to: OPEN, required_inputs: [supplier, amount] }
      accept:  { kind: external, from: OPEN, to: ACCEPTED, guards: { supplier_trading: deny } }
"""

SUPPLY_EXAMPLES = """examples_for: supply

setups:
  a_quote:
    description: A supplier the ERP sent, and its quote.
    steps:
      - import: { type: Supplier, as: acme, state: ACTIVE, values: { code: S-1, name: Acme } }
      - request: { type: Quote, transition: receive, as: quote, inputs: { supplier: acme, amount: 100 } }

examples:
  a_trading_suppliers_quote_is_accepted:
    description: The supplier may trade, so its quote is accepted.
    given: a_quote
    request: { object: quote, transition: accept }
    expect: { verdict: applied, state: ACCEPTED }

  a_supplier_the_erp_blocks_after_it_quoted:
    description: The ERP blocks the supplier after it quoted; the next import refreshes this store's copy, and the quote is no longer accepted.
    given: a_quote
    steps:
      - import: { type: Supplier, as: acme, state: BLOCKED, values: { name: Acme Ltd }, after: 1 day }
    request: { object: quote, transition: accept }
    expect: { verdict: unsatisfied, clause: supplier_trading, remedy: dependent }
"""


def refresh_plants():
    """An example's import naming an alias an earlier import gave refreshes that object, as import_batch refreshes an
    object it holds by its legacy key: clean, the refresh blocking a supplier's quote; an import naming an alias a
    request gave, one refreshing an imported alias as another type, and an example expecting the quote accepted after
    the refresh, each caught (ADR-0176 decision 4)."""
    ok = True
    found, _notes = check([("supply.yaml", SUPPLY_FLOW), ("supply.examples.yaml", SUPPLY_EXAMPLES)])
    ok &= not found
    print(f"  an import refreshing what an earlier one gave: {'clean' if not found else [x[4][:80] for x in found]}")
    carrier = SUPPLY_FLOW.replace("  Quote:\n", "  Carrier:\n    description: A carrier as the ERP records it.\n"
                                  "    mirror: true\n    tracking: record\n    attributes:\n"
                                  "      code: { type: string, external: erp }\n    states:\n"
                                  "      ACTIVE: { description: Trading., category: live }\n"
                                  "      CLOSED: { description: Gone., category: closed, final: true }\n\n  Quote:\n")
    cases = [
        ("an import naming an alias a request gave", SUPPLY_FLOW, SUPPLY_EXAMPLES.replace(
            "      - import: { type: Supplier, as: acme, state: BLOCKED, values: { name: Acme Ltd }, after: 1 day }",
            "      - import: { type: Supplier, as: quote, state: BLOCKED, values: { code: S-2 } }"), "names"),
        ("an imported alias refreshed as another type", carrier, SUPPLY_EXAMPLES.replace(
            "      - import: { type: Supplier, as: acme, state: BLOCKED, values: { name: Acme Ltd }, after: 1 day }",
            "      - import: { type: Carrier, as: acme, state: ACTIVE, values: { code: C-1 } }"), "names"),
        ("the quote expected accepted after the refresh", SUPPLY_FLOW, SUPPLY_EXAMPLES.replace(
            "    expect: { verdict: unsatisfied, clause: supplier_trading, remedy: dependent }",
            "    expect: { verdict: applied, state: ACCEPTED }"), "example"),
    ]
    for name, text, examples, code in cases:
        assert examples != SUPPLY_EXAMPLES and (code != "names" or name.startswith("an import") or text != SUPPLY_FLOW), name
        found, _notes = check([("supply.yaml", text), ("supply.examples.yaml", examples)])
        hit = [x for x in found if x[3] == code]
        ok &= bool(hit)
        print(f"  planted {name}: {'caught by ' + code if hit else 'MISSED ' + str(found[:2])}")
    return ok


VETO_FLOW = """module: approvals
categories: [live, closed]

types:
  Employee:
    description: A person, and their manager.
    tracking: record
    attributes:
      login:   { type: identity, actor_kind: human, unique: true }
      manager: { reference: Employee, optional: true }
    states:
      ACTIVE: { description: Working., category: live }
      LEFT:   { description: Gone., category: closed, final: true }
    transitions:
      add:   { kind: initial, to: ACTIVE, required_inputs: [login], optional_inputs: [manager] }
      move:  { kind: internal, from: ACTIVE, required_inputs: [manager] }
      leave: { kind: external, from: ACTIVE, to: LEFT }

  Requisition:
    description: A request to buy.
    tracking: record
    attributes:
      requester: { reference: Employee }
    states:
      OPEN:   { description: Raised., category: live }
      CLOSED: { description: Decided., category: closed, final: true }
    transitions:
      raise: { kind: initial, to: OPEN, required_inputs: [requester] }
      close: { kind: external, from: OPEN, to: CLOSED }

  Approval:
    description: An approval of a requisition, recorded once.
    tracking: record
    attributes:
      requisition: { reference: Requisition }
      approver:    { reference: Employee }
    states:
      GIVEN: { description: Given., category: closed, final: true }
    invariants:
      by_the_requesters_manager:
        description: The requester's manager approved it.
        expression: approver == requisition.requester.manager
    transitions:
      give: { kind: initial, to: GIVEN, required_inputs: [requisition, approver] }
"""


def veto_plants():
    """An invariant of a type whose every state is final, reading through a reference, can only refuse a change to
    what it reads, and is noticed, naming the reference; the same rule over a copy the record took when it was made is
    not, nor one of a type with a state that is not final (ADR-0176 decision 5)."""
    def vetoes_of(text):
        found, notes = check([("approvals.yaml", text)])
        return found, [n for n in notes if n[3] == "veto"]
    found, hit = vetoes_of(VETO_FLOW)
    ok = not found and len(hit) == 1 and "reads through requisition," in hit[0][4]
    print(f"  an approval's rule through its requisition, noticed: {'yes' if ok else 'NO ' + str((found, hit))}")
    copied = VETO_FLOW.replace("      approver:    { reference: Employee }\n",
                               "      approver:    { reference: Employee }\n      manager_then: { reference: Employee }\n").replace(
        "expression: approver == requisition.requester.manager", "expression: approver == manager_then").replace(
        "required_inputs: [requisition, approver] }", "required_inputs: [requisition, approver, manager_then] }")
    living = VETO_FLOW.replace("      GIVEN: { description: Given., category: closed, final: true }",
                               "      GIVEN: { description: Given., category: live }\n"
                               "      WITHDRAWN: { description: Withdrawn., category: closed, final: true }").replace(
        "      give: { kind: initial, to: GIVEN, required_inputs: [requisition, approver] }",
        "      give: { kind: initial, to: GIVEN, required_inputs: [requisition, approver] }\n"
        "      withdraw: { kind: external, from: GIVEN, to: WITHDRAWN }")
    for name, text in (("over a copy taken when it was made", copied), ("of a type with a state not final", living)):
        assert text != VETO_FLOW, name
        found, hit = vetoes_of(text)
        clean = not found and not hit
        ok &= clean
        print(f"  the rule {name}, not noticed: {'yes' if clean else 'NO ' + str((found, hit))}")
    return ok


def identity_plants():
    """An approval recorded from who acts sets what a state requires, and a metric by the identity it records reads
    in an example by that identity, where it is also an alias, clean and passing; one expecting another approver
    fails (D566, D568)."""
    ok = True
    found, _notes = check([("approve.yaml", IDENTITY_FLOW), ("approve.examples.yaml", IDENTITY_EXAMPLES)])
    ok &= not found
    print(f"  a state requiring who approved, and a metric by approver read in an example: "
          f"{'clean' if not found else [x[4][:80] for x in found]}")
    wrong = IDENTITY_EXAMPLES.replace("[{ approver: ana, value: 1 }]", "[{ approver: anna, value: 1 }]")
    assert wrong != IDENTITY_EXAMPLES
    found, _notes = check([("approve.yaml", IDENTITY_FLOW), ("approve.examples.yaml", wrong)])
    hit = [x for x in found if x[3] == "example"]
    ok &= bool(hit)
    print(f"  planted another approver expected of the metric: {'caught by example' if hit else 'MISSED ' + str(found[:2])}")
    return ok


def present_path_plants():
    """A value read through references none of which is optional, ending in an attribute that is not optional, is
    provably present (flow-format.md §4.5): from an attribute, and from a required input, it sets what a state requires;
    through an optional reference, or to an optional attribute, it does not."""
    ok = True
    via_input = BUDGET_PATH.replace(
        "to: APPROVED, effect: [ { assign: { location: budget_year, expr: cost_centre.fiscal_year } } ] }",
        "to: APPROVED, inputs: { centre: { reference: CostCentre } }, effect: [ { assign: { location: budget_year, expr: inputs.centre.fiscal_year } } ] }")
    # who acts is present on every request, so a state may require what records it (D566)
    by_actor = BUDGET_PATH.replace(
        "to: APPROVED, effect: [ { assign: { location: budget_year, expr: cost_centre.fiscal_year } } ] }",
        "to: APPROVED, effect: [ { assign: { location: budget_year, expr: cost_centre.fiscal_year } }, "
        "{ assign: { location: approved_by, expr: actor.id } } ] }").replace(
        "budget_year:   { type: int, optional: true }",
        "budget_year:   { type: int, optional: true }\n      approved_by:   { type: identity, optional: true }").replace(
        "required_attributes: [budget_year] }", "required_attributes: [budget_year, approved_by] }")
    for name, text in (("through a required reference", BUDGET_PATH), ("through a required input", via_input),
                       ("from who acts", by_actor)):
        found, _notes = check([("plant.yaml", text)])
        ok &= not found
        print(f"  a value read {name} to a required attribute is provably present: {'yes' if not found else [x[4][:80] for x in found]}")
    cases = [
        ("a value read through an optional reference", BUDGET_PATH.replace("expr: cost_centre.fiscal_year", "expr: backup_centre.fiscal_year")),
        ("a value read to an optional attribute", BUDGET_PATH.replace("budget_year:   { type: int, optional: true }", "budget_year:   { type: string, optional: true }")
         .replace("expr: cost_centre.fiscal_year", "expr: cost_centre.note")),
    ]
    for name, text in cases:
        found, _notes = check([("plant.yaml", text)])
        hit = [x for x in found if x[3] == "required"]
        ok &= bool(hit) and len(found) == 1
        print(f"  planted {name}, counted present: {'caught by required, at ' + str(hit[0][1]) if hit and len(found) == 1 else 'MISSED ' + str(found[:2])}")
    return ok


def held_plants(people):
    """A formula metric reading what a tracked member held over each span, `i.held(<member>)` (declaration-syntax.md
    §6.9), planted in the service example: clean as written, its argument a member of the type and no name of the
    metric's own (D550), and refused where it names no member of the type."""
    ok = True
    service = (EXAMPLES / "service.yaml").read_text()
    anchor = "      inspection_pass_rate:\n"
    held = ("      wait_by_engineer:\n        description: Planted.\n        source: intervals\n        item: i\n"
            "        dimensions:\n          engineer: i.held(engineer)\n        time_dimension: i.entered_at\n"
            "        expression: median(i.duration)\n")
    assert service.count(anchor) == 1
    planted = service.replace(anchor, held + anchor, 1)
    found, _notes = check([("people.yaml", people), ("service.yaml", planted)])
    clean = not found
    ok &= clean
    print(f"  a metric reading what a member held over each span is checked clean: {'yes' if clean else [x[4][:80] for x in found]}")
    found, _notes = check([("people.yaml", people), ("service.yaml", planted.replace("i.held(engineer)", "i.held(engneer)", 1))])
    hit = [x for x in found if x[3] == "names" and "has no member 'engneer'" in x[4]]
    ok &= bool(hit) and len(found) == 1
    print(f"  planted a held member the type does not declare: {'caught by names, at ' + str(hit[0][1]) + ' ' + hit[0][4][:70] if hit else 'MISSED ' + str(found[:1])}")
    return ok


def decimal_plant(people):
    """A decimal wider than 18 digits is refused (ADR-0143), and one of 18 is not."""
    inventory = (EXAMPLES / "inventory.yaml").read_text()
    old = "      sale_price:        { type: money(SGD), optional: true }\n"
    assert inventory.count(old) == 1
    verdict = []
    for p in (18, 19):
        text = inventory.replace(old, old + f"      margin:            {{ type: \"decimal({p},4)\", optional: true }}\n", 1)
        found, _ = check([("people.yaml", people), ("inventory.yaml", text)])
        verdict.append(bool([x for x in found if "a decimal holds 1 to 18 digits" in x[4]]))
    ok = verdict == [False, True]
    print(f"  a decimal of 19 digits is refused, and one of 18 is not: {'yes' if ok else 'NO ' + str(verdict)}")
    return ok


SCALE_FLOW = """module: dec
categories: [live, closed]
types:
  Line:
    description: A line priced in a currency.
    tracking: record
    attributes:
      rate: { type: "decimal(18,6)" }
      qty: { type: "decimal(18,6)" }
      total: { type: "decimal(18,6)", optional: true }
    states:
      OPEN: { category: live }
      DONE: { category: closed, final: true }
    transitions:
      add:
        kind: initial
        to: OPEN
        required_inputs: [rate, qty]
        effect:
          - assign: { location: total, expr: rate * qty }
      close: { kind: external, from: OPEN, to: DONE }
"""

SCALE_EXAMPLES = """examples_for: dec
examples:
  a_product_keeps_its_scale:
    description: Two decimals multiplied in an effect are kept at the column's scale.
    request: { type: Line, transition: add, inputs: { rate: 2, qty: 3 } }
    expect: { verdict: applied, state: OPEN, values: { total: 6.000000 } }
  a_third_squared:
    description: A product rounded to the column's scale.
    request: { type: Line, transition: add, inputs: { rate: 0.333333, qty: 0.333333 } }
    expect: { verdict: applied, state: OPEN, values: { total: 0.111111 } }
"""


def scale_plant():
    """A decimal an effect computes is written at its attribute's scale, a product rounding half to even to it
    (declaration-syntax.md §8.3), on both backends: the core once stored 2 x 3 as 6000000.000000, and the reference kept
    every place of 0.333333 squared (F4). Clean, and an example expecting the unrounded product caught."""
    found, _ = check([("dec.yaml", SCALE_FLOW), ("dec.examples.yaml", SCALE_EXAMPLES)])
    ok = not found
    wrong = SCALE_EXAMPLES.replace("values: { total: 0.111111 }", "values: { total: 0.111110888889 }")
    assert wrong != SCALE_EXAMPLES
    caught, _ = check([("dec.yaml", SCALE_FLOW), ("dec.examples.yaml", wrong)])
    ok &= any(x[3] == "example" for x in caught)
    print(f"  a decimal product written at its attribute's scale, rounded half to even: {'yes' if ok else 'NO ' + str(found)}")
    return ok


def cutover_plant(people):
    """A mirror that references a type this store owns is refused, since a type is cut over only after every type
    referencing it (ADR-0075); the service desk's own Laptop, owned, naming the mirror Employee is not."""
    sd = (EXAMPLES / "servicedesk.yaml").read_text()
    old = "      department:  { type: string, optional: true }\n"
    assert sd.count(old) == 1
    found, _ = check([("people.yaml", people), ("servicedesk.yaml", sd.replace(old, old + "      team:        { reference: Service, optional: true }\n", 1))])
    hit = [x for x in found if "a mirror's reference to Service" in x[4]]
    clean, _ = check([("people.yaml", people), ("servicedesk.yaml", sd)])
    ok = bool(hit) and not clean
    print(f"  a mirror referencing an owned type is refused, and an owned type referencing a mirror is not: {'yes' if ok else 'NO'}")
    return ok


def supersession_plant(people):
    """A successor already superseded is refused, so two requests cannot close a supersession chain on itself
    (ADR-0144). The customers module's merged_into is only via merge_in, whose successor is always the survivor, so
    it is made requestable here, as a flow that takes its successor from the request would have it."""
    module = (EXAMPLES / "customers.yaml").read_text()
    old = "        only_via: [Customer.merge_in]\n"
    assert module.count(old) == 1
    examples = (EXAMPLES / "customers.examples.yaml").read_text() + """
  a_chain_closed_on_itself_is_refused:
    description: Lee was merged into Dana, so Dana cannot then be merged into Lee.
    given: a_prospect
    steps:
      - request: { type: Customer, transition: register, as: lee, inputs: { name: D. Lee } }
      - request: { object: lee, transition: merged_into, inputs: { successor: dana } }
    request: { object: dana, transition: merged_into, inputs: { successor: lee } }
    expect: { verdict: unsatisfied, clause: successor_not_superseded, remedy: self_serviceable }
"""
    found, notes = check([("people.yaml", people), ("customers.yaml", module.replace(old, "", 1)),
                          ("customers.examples.yaml", examples)])
    ok = not found and not any(n[3] == "notrun" for n in notes)
    print(f"  a successor already superseded is refused, so two requests cannot close a chain on itself: {'yes' if ok else 'NO ' + str(found[:1])}")
    return ok


def proposal_plants(people):
    """The proposal steps of §11.3 held to their rules, on the delivery examples: a proposal only of a proposable
    transition, a decision only of a filed proposal, an expectation of the state the verdict leaves it in, and an
    approval refused as its request would be (DESIGN.md §9)."""
    inventory, delivery = (EXAMPLES / "inventory.yaml").read_text(), (EXAMPLES / "delivery.yaml").read_text()
    examples = (EXAMPLES / "delivery.examples.yaml").read_text()
    filed = "propose: { object: delivery, transition: complete, actor: scout, proposal: handover }"
    plants = [("a proposal of a transition that is not proposable", "names", "is not proposable", filed, filed.replace("complete", "approve")),
              ("an approval of a proposal no step filed", "names", "filed as a proposal",
               "    approve: { proposal: handover }\n    expect: { verdict: applied", "    approve: { proposal: handoff }\n    expect: { verdict: applied"),
              ("a proposal left pending by an approval that applies", "names", "leaves its proposal executed",
               "proposal: executed }", "proposal: pending }"),
              ("an approval before the sign-off, expected to apply", "example", "expected the request to apply",
               "expect: { verdict: unsatisfied, clause: signed_off, remedy: delegable, proposal: pending }",
               "expect: { verdict: applied, state: DELIVERED, proposal: executed }")]
    ok = True
    for name, code, words, old, new in plants:
        assert examples.count(old) >= 1, old
        found, _n = check([("people.yaml", people), ("inventory.yaml", inventory), ("delivery.yaml", delivery),
                           ("delivery.examples.yaml", examples.replace(old, new, 1))])
        hit = [f for f in found if f[3] == code and words in f[4]]
        ok &= bool(hit)
        print(f"  planted {name}: {'caught by ' + code if hit else 'MISSED ' + str(found[:1])}")
    return ok


def date_plant(people):
    """A date an example writes is a day, 2026-03-01, which the strict loader keeps as text (ADR-0145, D494); a
    timestamp in its place is refused."""
    module = (EXAMPLES / "reimbursement.yaml").read_text()
    examples = (EXAMPLES / "reimbursement.examples.yaml").read_text()
    old = "expense_date: 2025-12-31, receipt: blank.pdf"
    assert examples.count(old) == 1
    found, _ = check([("people.yaml", people), ("reimbursement.yaml", module),
                      ("reimbursement.examples.yaml", examples.replace(old, "expense_date: 2025-12-31T00:00Z, receipt: blank.pdf"))])
    ok = any("is a date, written as 2026-03-01" in f[4] for f in found) and load("d: 2026-03-01")["d"] == "2026-03-01"
    print(f"  a date written as a timestamp is refused, and the strict loader keeps a date as its text: {'yes' if ok else 'NO'}")
    return ok


def scan_plant(people):
    """A type scan reads only what the store holds indexed (the model's check 7): the service desk's freeze guard scans
    every ChangeWindow by its kind, which is indexed; unindexed, it is refused."""
    sd = (EXAMPLES / "servicedesk.yaml").read_text()
    old = "      kind:      { type: WindowKind, indexed: true }\n"
    assert sd.count(old) == 1
    found, _ = check([("people.yaml", people), ("servicedesk.yaml", sd.replace(old, "      kind:      { type: WindowKind }\n"))])
    clean, _ = check([("people.yaml", people), ("servicedesk.yaml", sd)])
    ok = any("scans every ChangeWindow and reads w.kind" in f[4] for f in found) and not clean
    print(f"  a type scan over an attribute that is not indexed is refused, and one over an indexed one is not: {'yes' if ok else 'NO'}")
    return ok


def group_erasure_plant(people):
    """The group a supersession joins is erased in one request, each member's erasure receiving its inputs, so the types
    it joins require the same inputs of their erasures (ADR-0146): a customer superseded by a contact whose erasure
    requires a ticket the customer's does not is refused."""
    cu = (EXAMPLES / "customers.yaml").read_text()
    old_ref = "          successor: { reference: Customer }\n"
    old_in = "        only_via: [Customer.forget]\n        inputs:\n          reason: { type: string }\n"
    assert cu.count(old_ref) == 1 and cu.count(old_in) == 1
    text = cu.replace(old_ref, "          successor: { reference: Contact }\n").replace(
        old_in, old_in + "          ticket: { type: string }\n")
    found, _ = check([("people.yaml", people), ("customers.yaml", text)])
    hit = any("whose erasure takes reason, ticket where Customer's takes reason" in f[4] for f in found)
    # the same names, one of another type, differ too (ADR-0147)
    typed = cu.replace(old_ref, "          successor: { reference: Contact }\n").replace(
        old_in, old_in.replace("reason: { type: string }", "reason: { type: OverrideReason }"))
    found, _ = check([("people.yaml", people), ("customers.yaml", typed)])
    hit &= any("whose erasure takes reason where Customer's takes reason" in f[4] for f in found)
    print(f"  a supersession joining types whose erasures take different inputs, by name or by type, is refused: {'yes' if hit else 'NO ' + str([f[4] for f in found][:2])}")
    return hit


def erasure_plants(people):
    """A type declares at most one erasure, an assertion enters no superseding state, and a type keeps its erasure once
    it has needed one (ADR-0147): a second erasure of a customer, an assertion into MERGED, and a version that drops a
    type's last personal attribute with its erasure are each refused."""
    cu = (EXAMPLES / "customers.yaml").read_text()
    old_forget = "      forget:\n        kind: erasure\n        description: Erases the customer's personal data, and their contacts'.\n"
    old_to = "        to: [PROSPECT, ACTIVE, DORMANT]\n        description: Puts the customer into the state they are really in"
    assert cu.count(old_forget) == 1 and cu.count(old_to) == 1
    two = cu.replace(old_forget, "      purge:\n        kind: erasure\n        description: A second erasure.\n" + old_forget)
    found, _ = check([("people.yaml", people), ("customers.yaml", two)])
    twice = any("declares the erasures" in f[4] for f in found)
    merged = cu.replace(old_to, old_to.replace("DORMANT]", "DORMANT, MERGED]"))
    found, _ = check([("people.yaml", people), ("customers.yaml", merged)])
    asserted = any("asserts a superseding state" in f[4] for f in found)
    v1 = ("module: m\ncategories: [live, closed]\ntypes:\n  T:\n    description: A planted type.\n    tracking: record\n    attributes:\n"
          "      p: { type: string, optional: true, personal: true }\n    states:\n      A: { category: live }\n    transitions:\n"
          "      add: { kind: initial, to: A, description: A planted step. }\n"
          "      forget: { kind: erasure, description: A planted erasure, inputs: { reason: { type: string } } }\n")
    v2 = v1.replace(", personal: true", "").replace("      forget: { kind: erasure, description: A planted erasure, inputs: { reason: { type: string } } }\n", "")
    found, _ = check([("m.yaml", v2)], {"m.yaml": v1})
    kept = any("keeps its erasure once it has needed one" in f[4] for f in found)
    ok = twice and asserted and kept
    print(f"  a second erasure, an assertion into a superseding state, and a dropped erasure once needed, are refused: "
          f"{'yes' if ok else 'NO ' + str((twice, asserted, kept))}")
    return ok


def binder_plant(people):
    """A loop's item named as a member of the type is refused (ADR-0146): the customers erasure's loop over its
    contacts, its item renamed to the member `email`."""
    cu = (EXAMPLES / "customers.yaml").read_text()
    old = "              item: c\n"
    assert cu.count(old) == 1
    found, _ = check([("people.yaml", people), ("customers.yaml", cu.replace(old, "              item: email\n"))])
    hit = any("binds 'email', which is a member of Customer" in f[4] for f in found)
    print(f"  a bound name that is a member of its type is refused: {'yes' if hit else 'NO'}")
    return hit


def abstract_summary_plant(people):
    """An abstract base declares no summary, each subtype declaring its own (ADR-0146): the service desk's Asset, given
    one, is refused at step 2."""
    sd = (EXAMPLES / "servicedesk.yaml").read_text()
    old = "    abstract: true\n    tracking: serial\n"
    assert sd.count(old) == 1
    found, _ = check([("people.yaml", people), ("servicedesk.yaml", sd.replace(old, old + "    summary: [key]\n"))])
    hit = any(f[3] == "schema" for f in found)
    clean, _ = check([("people.yaml", people), ("servicedesk.yaml", sd)])
    ok = hit and not clean
    print(f"  an abstract base declaring a summary is refused: {'yes' if ok else 'NO ' + str([f[4] for f in found][:1])}")
    return ok


def migration_plants(people):
    """A rename carries its attribute unchanged, so one that changes its type is refused, and a mapping into a
    superseding state would leave its objects superseded by nothing (ADR-0146)."""
    v1, v2 = (EXAMPLES / "service.yaml").read_text(), (EXAMPLES / "service-v2.yaml").read_text()
    old = "      completion_photo: { type: file, optional: true }\n"
    assert v2.count(old) == 1
    found, _ = check([("people.yaml", people), ("service-v2.yaml", v2.replace(old, "      completion_photo: { type: string, optional: true }\n"))],
                     {"service-v2.yaml": v1})
    renamed = any("changes its type or whether it is optional" in f[4] for f in found)
    cu = (EXAMPLES / "customers.yaml").read_text()
    text = cu + ("\nmigration:\n  description: A planted mapping.\n  removed_states:\n    Customer: { LAPSED: MERGED }\n")
    found, _ = check([("people.yaml", people), ("customers.yaml", text)])
    superseding = any("a superseding state, which would leave its objects superseded by nothing" in f[4] for f in found)
    print(f"  a rename that changes a type, and a mapping into a superseding state, are refused: {'yes' if renamed and superseding else 'NO ' + str((renamed, superseding))}")
    return renamed and superseding


def metric_plants(people):
    """A percentile's p is in (0, 1] (ADR-0146): a planted service metric taking the 1.5th is refused, and the 1st is not."""
    sv = (EXAMPLES / "service.yaml").read_text()
    old = "    metrics:\n"
    assert sv.count(old) == 1
    metric = ("      slowest:\n        description: A planted percentile.\n        source: intervals\n        item: i\n"
              "        expression: percentile({}, i.duration)\n")
    caught = []
    for p in ("1.5", "1"):
        found, _ = check([("people.yaml", people), ("service.yaml", sv.replace(old, old + metric.format(p)))])
        caught.append(any("a percentile's p is in (0, 1]" in f[4] for f in found))
    ok = caught == [True, False]
    print(f"  a percentile outside (0, 1] is refused, and the first is not: {'yes' if ok else 'NO ' + str(caught)}")
    return ok


def range_plant(people):
    """A range is an int (ADR-0146): the delivery's loop over its checklist, written as a range over the checklist, a
    set of strings, or of 2.5, is refused, and as a range of 3 is not."""
    dv = (EXAMPLES / "delivery.yaml").read_text()
    old = "              array: inputs.checklist\n"
    assert dv.count(old) == 1
    caught = []
    for rng in ("inputs.checklist", "2.5", "3"):
        found, _ = check([("people.yaml", people), ("delivery.yaml", dv.replace(old, f"              range: {rng}\n"))])
        caught.append(any("and a range is an int" in f[4] for f in found))
    ok = caught == [True, True, False]
    print(f"  a range over a set or a fraction is refused, and one of 3 is not: {'yes' if ok else 'NO ' + str(caught)}")
    return ok


def binder_more_plants(people):
    """A bound name is no reserved word and is not reused by a nested loop (ADR-0147): the customers erasure's loop
    item renamed to `now`, and the delivery's loop wrapped in a loop binding the same name, are refused."""
    cu = (EXAMPLES / "customers.yaml").read_text()
    old = "              item: c\n"
    assert cu.count(old) == 1
    found, _ = check([("people.yaml", people), ("customers.yaml", cu.replace(old, "              item: now\n"))])
    reserved = any("binds 'now', a word an expression resolves first" in f[4] for f in found)
    dv = (EXAMPLES / "delivery.yaml").read_text()
    old = "                - create: { type: ChecklistItem, transition: add, inputs: { delivery: this, label: label } }\n"
    assert dv.count(old) == 1
    nested = ("                - foreach:\n                    item: label\n                    array: inputs.checklist\n"
              "                    limit: 50\n                    steps:\n                      - create: { type: ChecklistItem, "
              "transition: add, inputs: { delivery: this, label: label } }\n")
    found, _ = check([("people.yaml", people), ("delivery.yaml", dv.replace(old, nested))])
    reused = any("in a loop inside one that binds it already" in f[4] for f in found)
    ok = reserved and reused
    print(f"  a bound name that is a reserved word, or reused by a nested loop, is refused: {'yes' if ok else 'NO ' + str((reserved, reused))}")
    return ok


def migration_more_plants(people):
    """Against the version before (ADR-0147): an attribute whose type changed, and one made required with no backfill, are
    notices; an invariant admitted twice, and an observation kind's invariant admitted, are refused."""
    v1, v2 = (EXAMPLES / "service.yaml").read_text(), (EXAMPLES / "service-v2.yaml").read_text()
    old = "      signed_off_by_user: { reference: User, optional: true }\n"
    adm = "    - { invariant: ServiceJob.photo_when_done, reason: MigrationReason.LEGACY_DATA }\n"
    assert v2.count(old) == 1 and v2.count(adm) == 1

    def run(text):
        found, notes = check([("people.yaml", people), ("service-v2.yaml", text)], {"service-v2.yaml": v1})
        return [f[4] for f in found], [n[4] for n in notes]
    _f, notes = run(v2.replace(old, "      signed_off_by_user: { type: string, optional: true }\n"))
    typed = any("changed its type" in n for n in notes)
    _f, notes = run(v2.replace(old, "      signed_off_by_user: { reference: User }\n"))
    required = any("is now required, which counts as a new required attribute" in n for n in notes)
    found, _n = run(v2.replace(adm, adm + adm))
    twice = any("admits ServiceJob.photo_when_done twice" in f for f in found)
    found, _n = run(v2.replace(adm, adm + "    - { invariant: InspectionResult.value_in_range, reason: MigrationReason.LEGACY_DATA }\n"))
    kind = any("an observation kind's invariant, which no migration reaches" in f for f in found)
    _f, clean = run(v2)
    quiet = not any("changed its type" in n or "is now required" in n for n in clean)
    ok = typed and required and twice and kind and quiet
    print(f"  a type changed and an attribute made required are noticed, and an invariant admitted twice or a kind's "
          f"admitted are refused: {'yes' if ok else 'NO ' + str((typed, required, twice, kind, quiet))}")
    return ok


def metric_more_plants(people):
    """A flag reads only the dimensions its metric declares, a percentile's p is a literal, and one input of a combined
    metric has every dimension it names (ADR-0147): a service flag reading actor_kind, a percentile of an expression, and
    the service desk's combined metric grouped by a dimension each input has only one of, are refused."""
    sv = (EXAMPLES / "service.yaml").read_text()
    old = 'flag_when: { low: "value < 0.900" }'
    assert sv.count(old) == 1
    found, _ = check([("people.yaml", people), ("service.yaml", sv.replace(old, 'flag_when: { low: "value < 0.900 and actor_kind == human" }'))])
    flagged = any("reads 'actor_kind', which is neither value nor a dimension the metric declares" in f[4] for f in found)
    old_m = "    metrics:\n"
    metric = ("      slowest:\n        description: A planted percentile.\n        source: intervals\n        item: i\n"
              "        expression: percentile(0.5 * 1, i.duration)\n")
    found, _ = check([("people.yaml", people), ("service.yaml", sv.replace(old_m, old_m + metric, 1))])
    literal = any("a percentile's p is a literal" in f[4] for f in found)
    sd = (EXAMPLES / "servicedesk.yaml").read_text()
    old_c = "      problems: Problem.problems_opened\n      incidents: Incident.incidents_opened\n    group_by: [month]\n"
    assert sd.count(old_c) == 1
    # each input given a second dimension the other lacks, so no one input has both the combination names
    inc = "          month: month(i.created_at)\n"
    prob = "          month: month(p.created_at)\n"
    assert sd.count(inc) == 1 and sd.count(prob) == 1
    split = sd.replace(inc, inc + "          day: day(i.created_at)\n").replace(prob, prob + "          year: year(p.created_at)\n")
    split = split.replace(old_c, old_c.replace("group_by: [month]", "group_by: [day, year]"))
    found, _ = check([("people.yaml", people), ("servicedesk.yaml", split)])
    combined = any("which no one of the metrics it combines has all of" in f[4] for f in found)
    ok = flagged and literal and combined
    print(f"  a flag reading an undeclared dimension, a percentile of an expression, and a combined metric's dimensions "
          f"split across its inputs, are refused: {'yes' if ok else 'NO ' + str((flagged, literal, combined))}")
    return ok


def cluster3_plants(people):
    """A personal set is accepted, emptied by an erasure; `redact` is a reserved transition name; and a family is ported
    together, an abstract base a mirror only with every subtype one (ADR-0148)."""
    cu = (EXAMPLES / "customers.yaml").read_text()
    old = "      email: { type: string, optional: true, personal: true }\n"
    assert cu.count(old) == 1
    found, _ = check([("people.yaml", people), ("customers.yaml", cu.replace(old, old + "      aliases: { type: \"string[]\", personal: true }\n"))])
    personal_set = not any("aliases" in f[4] for f in found)
    old_t = "      correct_email:\n"
    assert cu.count(old_t) == 1
    found, _ = check([("people.yaml", people), ("customers.yaml", cu.replace(old_t, old_t.replace("correct_email", "redact")))])
    reserved = any("redact" in f[4] and f[3] in ("names", "schema") for f in found)
    sd = (EXAMPLES / "servicedesk.yaml").read_text()
    old_a = "    abstract: true\n    tracking: serial\n"
    assert sd.count(old_a) == 1
    found, _ = check([("people.yaml", people), ("servicedesk.yaml", sd.replace(old_a, "    abstract: true\n    mirror: true\n    tracking: serial\n"))])
    family = any("a family is ported together" in f[4] for f in found) and not any(f[3] == "schema" for f in found)
    ok = personal_set and reserved and family
    print(f"  a personal set is accepted, a transition named redact is refused, and a family half ported as a mirror is refused "
          f"while the abstract mirror itself passes the schema: {'yes' if ok else 'NO ' + str((personal_set, reserved, family))}")
    return ok


def expression_type_plants(people):
    """An expression that does not type by declaration-syntax.md §8.3 is refused, saying which rule it breaks, where
    the types it combines are known (the model's check 24): money of two currencies, decimals of two scales, an int and
    a decimal, two timestamps added, money times money, a duration times a duration, an int over money, money of two
    currencies divided, a timestamp compared with a duration, a string with an int, a string asked of an enumeration,
    a connective and a `not` on what is no bool, a conditional of two types, `length` of an int, an invariant and a
    condition that are no bool, and a scale a product or a quotient gives (D549); and what types is accepted."""
    probe = "usd + usd"

    def ledger(expr, invariant=None, condition=None):
        text = TYPED_LEDGER.replace(f'expression: "{probe}"', f'expression: "{expr}"')
        if invariant:
            text = text.replace("    transitions:\n", f"    invariants:\n      check: {{ description: A check., expression: \"{invariant}\" }}\n    transitions:\n")
        if condition:
            text = text.replace("    transitions:\n", f"    conditions:\n      ready: {{ description: Ready., expression: \"{condition}\", remedy: dependent }}\n    transitions:\n")
            text = text.replace("shut: { kind: external, from: [OPEN], to: SHUT }", "shut: { kind: external, from: [OPEN], to: SHUT, guards: { ready: deny } }")
        return text

    def wrong(name, why):
        return f"{name} does not type: {why} (the model's check 24)"

    def tagged(expr):
        # the ledger with a set of strings of its own
        text = ledger(expr)
        assert text.count("      label: { type: string }\n") == 1
        return text.replace("      label: { type: string }\n", "      label: { type: string }\n      tags:  { type: \"string[]\", optional: true }\n")

    def stepping(steps):
        # the ledger with its set of strings, shut taking the steps given
        text = tagged(probe)
        assert text.count("shut: { kind: external, from: [OPEN], to: SHUT }") == 1
        return text.replace("shut: { kind: external, from: [OPEN], to: SHUT }",
                            "shut: { kind: external, from: [OPEN], to: SHUT, effect: [ " + steps + " ] }")

    def dated(expr):
        # the ledger with a date of its own
        text = ledger(expr)
        assert text.count("      span:  { type: duration }\n") == 1
        return text.replace("      span:  { type: duration }\n", "      span:  { type: duration }\n      due:   { type: date, optional: true }\n")

    def writing(location, expr):
        # a transition writing the value, on the ledger's shut
        return TYPED_LEDGER.replace(
            "shut: { kind: external, from: [OPEN], to: SHUT }",
            f"shut: {{ kind: external, from: [OPEN], to: SHUT, effect: [ {{ assign: {{ location: {location}, expr: \"{expr}\" }} }} ] }}")

    def misfit(expr, why):
        return f"Account.shut writes {expr}, {why} (the model's check 17)"
    cases = [
        ("money of two currencies", ledger("usd + sgd"), [wrong("derived attribute probe", "it adds money of two currencies, USD and SGD")]),
        ("decimals of two scales", ledger("rate + fine"), [wrong("derived attribute probe", "it adds decimals of two scales, 2 and 3")]),
        ("an int and a decimal", ledger("count + rate"), [wrong("derived attribute probe", "it adds an int and a decimal of scale 2, two types")]),
        ("two timestamps added", ledger("since + since"), [wrong("derived attribute probe", "it adds a timestamp and a timestamp")]),
        ("money times money", ledger("usd * usd"),
         [wrong("derived attribute probe", "it multiplies money(USD) by money(USD), and takes a scalar on at least one side")]),
        ("a duration times a duration", ledger("span * span"),
         [wrong("derived attribute probe", "it multiplies a duration by a duration, and takes a scalar on at least one side")]),
        ("an int over money", ledger("count / usd"),
         [wrong("derived attribute probe", "it divides an int by money(USD), and / takes its scalar on the right")]),
        ("money of two currencies divided", ledger("usd / sgd"), [wrong("derived attribute probe", "it divides money of two currencies, USD and SGD")]),
        ("a timestamp compared with a duration", ledger("since < span"), [wrong("derived attribute probe", "it compares a timestamp with a duration")]),
        ("a string compared with an int", ledger("label == count"), [wrong("derived attribute probe", "it compares a string with an int")]),
        ("a string asked of an enumeration", ledger("label in Tier"), [wrong("derived attribute probe", "it asks whether a string is in Tier")]),
        ("a connective on a string", ledger("vip and label"), [wrong("derived attribute probe", "and takes bools, and is given a string")]),
        ("not on an int", ledger("not count"), [wrong("derived attribute probe", "not takes a bool, and is given an int")]),
        ("a conditional of two types", ledger("if vip then usd else sgd"),
         [wrong("derived attribute probe", "if yields money(USD) or money(SGD), which share no type")]),
        ("length of an int", ledger("length(count) > 0"), [wrong("derived attribute probe", "length takes a string or a set of values, and is given an int")]),
        ("an invariant that is no bool", ledger(probe, invariant="count + 1"), [wrong("invariant check", "it is an int, not a bool")]),
        ("a condition that is no bool", ledger(probe, condition="label"), [wrong("condition ready", "it is a string, not a bool")]),
        ("a product's scale", ledger("rate * rate - rate"), [wrong("derived attribute probe", "it subtracts decimals of two scales, 4 and 2")]),
        ("a literal's scale in a product", ledger("rate * 1.0 - rate"),
         [wrong("derived attribute probe", "it subtracts decimals of two scales, 3 and 2")]),
        ("a quotient of its left operand's type", ledger("rate / fine + fine"),
         [wrong("derived attribute probe", "it adds decimals of two scales, 2 and 3")]),
        ("a sum of products", ledger("sum(a in Account: a.rate * a.rate) - rate"),
         [wrong("derived attribute probe", "it subtracts decimals of two scales, 4 and 2")]),
        # what an expression names or writes, its literals held exactly (the audit's values G2, L1, L2, W2, W3, W5)
        ("time_in of a member", ledger("time_in(label) > 1 h"), [wrong("derived attribute probe", "time_in(label) names no state of Account")]),
        ("entered_at of an untracked member", ledger("entered_at(label) < now"),
         [wrong("derived attribute probe", "entered_at(label) names neither a state of Account nor a member it tracks")]),
        ("time_in of a state and entered_at of a tracked member", ledger("time_in(OPEN) > 1 h and entered_at(tier) < now"), []),
        # a call of what the language has, where any other typed as nothing and the core did not run it (values K-7, K-8)
        ("a function the language has not", ledger("abs(count) > 0"), [wrong("derived attribute probe", "abs is no function the language has: it has length, date, entered_at and time_in, and in a metric avg, median, percentile, day, week, month, quarter and year")]),
        ("an aggregate called on values", ledger("max(count, count) > 0"), [wrong("derived attribute probe", "max is no function the language has: it has length, date, entered_at and time_in, and in a metric avg, median, percentile, day, week, month, quarter and year")]),
        ("a function given two arguments", ledger("length(label, label) > 0"),
         [wrong("derived attribute probe", "length takes one argument, and is given 2")]),
        # a member function on what has it, and a flow function naming a state or a member (values K-8)
        ("held of an object", ledger("this.held(count) > 0"),
         [wrong("derived attribute probe", "held is a row's, of intervals or transitions, and it is called on an object of Account")]),
        ("entered_at of a text", ledger("label.entered_at(OPEN) < now"),
         [wrong("derived attribute probe", "entered_at is an object's, and it is called on a string")]),
        ("the intervals of a member not tracked", ledger("count(i in this.intervals(label): true) > 0"),
         [wrong("derived attribute probe", "intervals(label) names neither the state nor a member Account tracks")]),
        ("time_in of a literal", ledger('time_in(\\"OPEN\\") > 1 h'),
         [wrong("derived attribute probe", "time_in names a state or a tracked member, and is given a string")]),
        ("the flow functions on what has them", ledger("this.time_in(OPEN) > 1 h and count(i in this.intervals(tier): true) > 0"), []),
        ("a member function the language has not", ledger("this.foo(count) > 0"),
         [wrong("derived attribute probe", "foo is no member function the language has: it has intervals, entered_at, time_in and a row's held")]),
        ("the functions the language has", ledger("length(label) > 0 and date(since) < date(now) and this.time_in(OPEN) > 1 h"), []),
        # an order, a negative and a membership only where the language has them (the audit's values K-4 to K-6)
        ("an enumeration ordered", ledger("tier > Tier.SILVER"),
         [wrong("derived attribute probe", "it orders a member of Tier, and an enumeration has no order")]),
        ("the greatest member of an enumeration", ledger("max(x in {tier, Tier.SILVER}: x) == Tier.SILVER"),
         [wrong("derived attribute probe", "max orders a member of Tier, and an enumeration has no order")]),
        ("a string negated", ledger("-label == label"), [wrong("derived attribute probe", "it negates a string, which has no negative")]),
        ("membership in a string", ledger("label in label"),
         [wrong("derived attribute probe", "it asks for membership in a string, which is no collection")]),
        ("what orders, negates and holds", ledger("-span < span and -usd < usd and count > -3 and tier == Tier.GOLD and tier in Tier"), []),
        # a date's and a time's arithmetic as §8.3 says and the core runs (the audit's values K-1, K-2)
        ("a duration plus a date", dated("span + due > due"), [wrong("derived attribute probe", "it adds a duration and a date")]),
        ("a date less a timestamp", dated("due - since > span"), [wrong("derived attribute probe", "it subtracts a date and a timestamp")]),
        ("a timestamp multiplied", ledger("since * 2 > since"),
         [wrong("derived attribute probe", "it multiplies a timestamp by a number, and a time or a date is neither multiplied nor divided")]),
        ("what dates do", dated("due - due >= span and due + 2 days > due and due - 1 week < due"), []),
        # a name or a member that resolves to a value (the audit's values K-9)
        ("an enumeration's name as a value", ledger("Tier == tier"),
         [wrong("derived attribute probe", "Tier is an enumeration, which is no value: name one of its members, as Tier.<MEMBER>")]),
        ("a name naming nothing", ledger("Foo == label"),
         [wrong("derived attribute probe", "Foo names nothing: no type, enumeration or state of Account is called Foo")]),
        ("a type's member that is no state", ledger("Account.foo == label"), [wrong("derived attribute probe", "Account.foo names no state of Account")]),
        ("a member of a string", ledger("label.foo == label"), [wrong("derived attribute probe", "it reads foo of a string, which has no members")]),
        ("a member of a state besides its category", ledger("state.foo == label"),
         [wrong("derived attribute probe", "it reads foo of a state, which has only category")]),
        ("a metric's count outside a metric", ledger("count(where vip) > 0"),
         [wrong("derived attribute probe", "count(where …) and count(distinct …) count a metric's rows, which no expression outside a metric has")]),
        ("names that resolve", ledger("tier == Tier.GOLD and state.category == live and state == Account.OPEN and state == OPEN"), []),
        # an aggregate over a collection of one type (the audit's values K-10)
        ("an aggregate over a text", ledger("count(x in label: true) > 0"),
         [wrong("derived attribute probe", "count ranges over a string, which is no collection")]),
        ("an aggregate over the object itself", ledger("count(x in this: true) > 0"),
         [wrong("derived attribute probe", "count ranges over an object of Account, which is no collection")]),
        ("a sum of strings", tagged("sum(t in tags: t) == label"), [wrong("derived attribute probe", "sum adds a string, which does not add")]),
        ("a set of two types", ledger("sum(x in {count, label}: x) > 0"),
         [wrong("derived attribute probe", "sum ranges over a set of an int and a string, which share no type")]),
        # an element fits its set, and a loop ranges over a collection (the audit's values K-10)
        ("a state written to an int", stepping("{ assign: { location: count, expr: state } }"),
         ["Account.shut writes state, a state of Account, to count, which holds int: a write's value must fit its target (the model's check 17)"]),
        # a set holds a set and nothing else does (the differential battery)
        ("an int written to a set", stepping("{ assign: { location: tags, expr: count } }"),
         ["Account.shut writes count, an int, to tags, which holds a set of string: a write's value must fit its target (the model's check 17)"]),
        ("a set written to a string", stepping("{ assign: { location: label, expr: tags } }"),
         ["Account.shut writes tags, a set of string, to label, which holds a string: a write's value must fit its target (the model's check 17)"]),
        ("an int added to a set of strings", stepping("{ add: { location: tags, expr: count } }"),
         ["Account.shut adds count, an int, to tags, whose elements are string: an element must fit its set (the model's check 17)"]),
        ("a loop over a string", stepping("{ foreach: { item: x, array: label, limit: 3, steps: [ { assign: { location: count, expr: 1 } } ] } }"),
         ["Account.shut loops over label, a string, which is no collection (the model's check 24)"]),
        ("an element that fits and a loop over a set", stepping(
            "{ add: { location: tags, expr: label } }, { foreach: { item: x, array: tags, limit: 3, steps: [ { assign: { location: count, expr: 1 } } ] } }"), []),
        # a set of values and a state each typed, so neither is added, ordered or negated (the differential battery)
        ("a number added to a set", tagged("count + tags > 0"),
         [wrong("derived attribute probe", "it adds an int and a set of string")]),
        ("a set compared with a number", tagged("tags == count"),
         [wrong("derived attribute probe", "it compares a set of string with an int")]),
        ("a set ordered", tagged("tags < tags"), [wrong("derived attribute probe", "it orders a set of string, and a set has no order")]),
        ("a set as a condition", tagged("not tags"), [wrong("derived attribute probe", "not takes a bool, and is given a set of string")]),
        ("an aggregate over an input's set", stepping('{ assign: { location: count, expr: "count(x in inputs.picked: x == label)" } }').replace(
            "to: SHUT, effect: [", 'to: SHUT, inputs: { picked: { type: "string[]" } }, effect: ['), []),
        ("a state ordered", ledger("state < OPEN"), [wrong("derived attribute probe", "it orders a state of Account, and a state has no order")]),
        ("a set's length, members and equality", tagged('length(tags) > 0 and \\"a\\" in tags and tags == tags and count(t in this.tags: true) > 0'), []),
        ("aggregates over collections", tagged('max(t in tags: t) == label and count(t in tags: t == \\"a\\") > 0 and sum(x in {1, 2}: x) > 0'), []),
        ("an amount past its minor unit", ledger("usd + USD 1.005 > usd"),
         [wrong("derived attribute probe", "USD 1.005 has more places than USD's minor unit")]),
        ("an amount in an unknown currency", ledger("XYZ 1 == XYZ 1"),
         [wrong("derived attribute probe", "XYZ is a currency whose minor unit the engine does not know")]),
        ("a fraction of a second", ledger("span + 1.5 s > span"), [wrong("derived attribute probe", "1.5 s is no whole number of seconds")]),
        # what the engine does not run, refused at publish (ADR-0202 decision 8)
        ("a duration past 64 bits", ledger("span + 9223372036854775808 s > span"),
         [wrong("derived attribute probe", "9223372036854775808 s is past a duration's 64 bits of seconds")]),
        ("a duration in minutes past 64 bits", ledger("span < 153722867280912931 min"),
         [wrong("derived attribute probe", "153722867280912931 min is past a duration's 64 bits of seconds")]),
        ("the widest durations", ledger("span < 9223372036854775807 s and span < 153722867280912930 min"), []),
        ("an int divided by a decimal", ledger("count / rate > rate"),
         [wrong("derived attribute probe", "it divides an int by a decimal of scale 2, which the engine does not run: divide a decimal, such as n * 1.0, for the fraction")]),
        ("an int literal divided by a decimal literal", ledger("7 / 2.0 > rate"),
         [wrong("derived attribute probe", "it divides a number by a number, which the engine does not run: divide a decimal, such as n * 1.0, for the fraction")]),
        ("a decimal dividend", ledger("count * 1.0 / rate > rate and rate / count > rate"), []),
        ("literals held exactly", ledger("usd + USD 1.50 > usd and span + 0.5 min > span"), []),
        ("an int literal past 64 bits", ledger("count + 9223372036854775808 > 0"),
         [wrong("derived attribute probe", "the literal 9223372036854775808 is past an int's 64 bits")]),
        ("the widest int literal", ledger("count < 9223372036854775807"), []),
        ("a count's body that is no condition", ledger("count(a in Account: a.count) > 0"),
         [wrong("derived attribute probe", "the body of count is an int, not a bool")]),
        ("a count's condition and a distinct count's value", ledger("count(a in Account: a.vip) > count(distinct a in Account: a.count)"), []),
        # an event, never an int read as a log position (the audit's values G7)
        ("changed_since given an int", ledger("changed_since([label], count)"),
         [wrong("derived attribute probe", "changed_since reads an event, and is given an int")]),
        ("changed_since given a literal", ledger("changed_since([label], 3)"),
         [wrong("derived attribute probe", "changed_since reads an event, and is given a number")]),
        ("changed_since given an event", ledger("changed_since([label], signed)").replace("      label: { type: string }\n", "      label: { type: string }\n      signed: { type: event, optional: true }\n", 1), []),
        # a literal's element typed (K-10), so the write the runner could not tell is refused at publish (W5)
        ("a narrower decimal through a literal", writing("rate", "max(x in {fine}: x)"),
         [misfit("max(x in {fine}: x)", "a decimal of scale 3, to rate, which holds a decimal of scale 2: only a product or a quotient is rounded to its target")]),
        ("a narrower decimal written", writing("rate", "fine"),
         [misfit("fine", "a decimal of scale 3, to rate, which holds a decimal of scale 2: only a product or a quotient is rounded to its target")]),
        ("a literal of more places than its partner", writing("rate", "rate + 0.005"),
         [wrong("Account.shut's effect", "it adds a number of 3 places and a decimal of scale 2, which holds fewer")]),
        # a literal's places kept beside an int or a literal, a zero divisor and a date moved by part of a day, each
        # refused on every object at run time (the differential battery)
        ("an int and a literal of places written to an int", writing("count", "count + 1.5"),
         [misfit("count + 1.5", "a decimal of scale 1, to count, which holds int: a write's value must fit its target")]),
        ("two literals written to a narrower decimal", writing("rate", "2.255 + 1.5"),
         [misfit("2.255 + 1.5", "a decimal of scale 3, to rate, which holds a decimal of scale 2: only a product or a quotient is rounded to its target")]),
        ("a division by zero written", writing("count", "count / 0"),
         [wrong("Account.shut's effect", "it divides by zero, which has no value")]),
        ("a date moved by an hour", dated("due + 1 h == due"),
         [wrong("derived attribute probe", "it moves a date by 1 h, which is no whole number of days")]),
        ("a date moved by days", dated("due + 2 days == due"), []),
        ("a decimal written to an int", writing("count", "rate * 2"),
         [misfit("rate * 2", "a decimal of scale 2, to count, which holds int: a write's value must fit its target")]),
        ("a literal with places written to an int", writing("count", "2.5"),
         [misfit("2.5", "a number, to count, which holds int: a write's value must fit its target")]),
        ("an int written to a string", writing("label", "if vip then count else count"),
         [misfit("if vip then count else count", "an int, to label, which holds string: a write's value must fit its target")]),
        ("writes that fit", writing("rate", "fine * rate"), []),
        # who acted typed, so a string joined to it is refused where the core did not run it (D687)
        ("a string joined to who acted", writing("label", '\\"by \\" + actor.id'),
         [wrong("Account.shut's effect", "it adds a string and an identity")]),
        ("a quotient and a wider decimal written", writing("fine", "rate / 3"), []),
        ("products and quotients that type", ledger("rate * fine - fine * rate > fine / rate and rate * 2 + rate > rate"), []),
        ("what types", ledger("if vip then usd / count + usd * 2 else usd - usd"), []),
        ("a duration of timestamps, a quotient of like amounts and a tier", ledger(
            "since - since < span and usd / usd > 1 and tier == Tier.GOLD and tier in Tier and label in {\\\"a\\\", \\\"b\\\"} and rate * count > rate"), []),
    ]
    ok = True
    for name, text, want in cases:
        found, _ = check([("people.yaml", people), ("ledger.yaml", text)])
        got = sorted(x[4] for x in found)
        hit = got == sorted(want) and all(x[3] == "names" for x in found)
        ok &= hit
        print(f"  check 24, {name}: {'as expected' if hit else 'NOT: ' + str(found)}")
    return ok


def identifier_personal_plants(people):
    """An identifier is kept past an erasure, and a sequence keeps its scope's value, so neither reads a personal
    value: a scope by a personal attribute and a format reading one are refused (the audit's personal data S27, S28),
    and an identifier from what is not personal is not."""
    def desk(ident):
        return ("module: desk\ncategories: [live, closed]\nsequences:\n  ticket: { description: Tickets. }\ntypes:\n"
                "  Ticket:\n    description: A ticket.\n    tracking: record\n    attributes:\n"
                "      requester_name: { type: string, personal: true, indexed: true, optional: true }\n"
                "      queue: { type: string, indexed: true }\n"
                f"      number: {{ type: string, identifier: {ident} }}\n"
                "    states:\n      OPEN: { category: live }\n      DONE: { category: closed, final: true }\n"
                "    transitions:\n      raise: { kind: initial, to: OPEN, required_inputs: [requester_name, queue] }\n"
                "      close: { kind: external, from: [OPEN], to: DONE }\n"
                "      forget: { kind: erasure, inputs: { reason: { type: string } } }\n")
    cases = [
        ("a scope by a personal attribute", desk('{ sequence: ticket, scope: requester_name, format: "T-{n}" }'),
         ["Ticket.number is scoped by 'requester_name', which is personal; a sequence keeps its scope's value, which no "
          "erasure reaches"]),
        ("a format reading a personal attribute", desk('{ sequence: ticket, format: "T-[{requester_name}-]{n}" }'),
         ["Ticket.number's format reads requester_name, which is personal; an identifier is kept past an erasure"]),
        ("an identifier from what is not personal", desk('{ sequence: ticket, scope: queue, format: "{queue}-{n}" }'), []),
    ]
    ok = True
    for name, text, want in cases:
        found, _ = check([("people.yaml", people), ("desk.yaml", text)])
        got = sorted(x[4] for x in found)
        hit = got == sorted(want)
        ok &= hit
        print(f"  an identifier, {name}: {'as expected' if hit else 'NOT: ' + str(found)}")
    return ok


def identity_metric_plants(people):
    """A metric reads no identity where an actor type marks its own personal, a copy of an identity being the identity:
    a dimension of type identity is refused there (the audit's personal data R8), and answered where identities are
    not personal."""
    def memos(personal):
        return ("module: memos\ncategories: [live, closed]\ntypes:\n  Writer:\n    description: A writer.\n"
                "    tracking: record\n    attributes:\n"
                f"      login: {{ type: identity, actor_kind: human, unique: true, personal: {str(personal).lower()}, optional: true }}\n"
                "    states:\n      ON: { category: live }\n      OFF: { category: closed, final: true }\n"
                "    transitions:\n      join: { kind: initial, to: ON, required_inputs: [login] }\n"
                "      leave: { kind: external, from: [ON], to: OFF }\n"
                "      forget: { kind: erasure, inputs: { reason: { type: string } } }\n"
                "  Memo:\n    description: A memo.\n    tracking: record\n    attributes:\n"
                "      author: { type: identity, optional: true }\n    states:\n      OPEN: { category: live }\n"
                "      DONE: { category: closed, final: true }\n    transitions:\n"
                "      write: { kind: initial, to: OPEN, effect: [ { assign: { location: author, expr: actor.id } } ] }\n"
                "      close: { kind: external, from: [OPEN], to: DONE }\n    metrics:\n"
                "      by_author: { description: Memos by author., source: objects, item: m, dimensions: { author: m.author }, expression: count() }\n")
    cases = [
        ("an identity dimension where identities are personal", memos(True),
         ["metric by_author reads m.author, which reaches the identity Memo.author, personal as an actor's is; a metric "
          "reads no personal value, through a reference or a derived attribute either (ADR-0203)"]),
        ("an identity dimension where they are not", memos(False), []),
    ]
    ok = True
    for name, text, want in cases:
        found, _ = check([("people.yaml", people), ("memos.yaml", text)])
        got = sorted(x[4] for x in found if x[3] == "names")
        hit = got == sorted(want)
        ok &= hit
        print(f"  a metric over identities, {name}: {'as expected' if hit else 'NOT: ' + str(found)}")
    return ok


def identity_write_plants(people):
    """An actor's identity is written only at its creation: a transition other than a creation or an erasure writing
    it, by an input or by a step, is refused (the model's check 17, ADR-0205 decision 5), and one writing it at
    creation is not."""
    crew = ("module: crew\ncategories: [live, closed]\ntypes:\n  Member:\n    description: A member.\n"
            "    tracking: record\n    attributes:\n      login: { type: identity, actor_kind: human }\n"
            "      alias: { type: identity, optional: true }\n    states:\n      ON: { category: live }\n"
            "      OFF: { category: closed, final: true }\n    transitions:\n"
            "      join: { kind: initial, to: ON, required_inputs: [login] }\n"
            "      leave: { kind: external, from: [ON], to: OFF }\n")
    wrong = ("Member.{} writes login, the identity Member's actors are named by, which is written only at their creation "
             "(the model's check 17)")
    cases = [
        ("by an input", crew + "      relogin: { kind: internal, from: [ON], required_inputs: [login] }\n", [wrong.format("relogin")]),
        ("by a step", crew + "      swap: { kind: internal, from: [ON], effect: [ { assign: { location: login, expr: alias } } ] }\n",
         [wrong.format("swap")]),
        ("at creation alone", crew, []),
    ]
    ok = True
    for name, text, want in cases:
        found, _ = check([("people.yaml", people), ("crew.yaml", text)])
        got = sorted(x[4] for x in found)
        hit = got == sorted(want)
        ok &= hit
        print(f"  an identity written, {name}: {'as expected' if hit else 'NOT: ' + str(found)}")
    return ok


def argument_type_plants(people):
    """An argument of a call or a create whose value does not fit the input it is given to is refused, an input the
    transition declares or an attribute input (the model's check 2, its types): a number to a string, the clock to an
    attribute input holding text, and an object of the wrong type to a reference; and arguments that fit are not."""
    def p(text, old, new):
        assert text.count(old) == 1, old
        return text.replace(old, new, 1)

    def step(text, line_start, steps):
        line = next(l for l in text.splitlines(keepends=True) if l.startswith(line_start))
        return p(text, line, line.rstrip("\n")[:-2] + ", effect: [ " + ", ".join(steps) + " ] }\n")

    def refused(what):
        return f"{what}: an argument must fit its input (the model's check 2)"
    LEAVE, CLOSE = "      leave:  {", "      close:  {"

    def asking(expr):
        """The customers asking an evaluator whether a label is known, in a guard on tagging them."""
        text = p(PERSONAL_CRM, "types:\n", "evaluators:\n  crmcheck:\n    description: A check of customers.\n"
                 "    functions:\n      known:\n        description: Whether a label is known.\n"
                 "        arguments:\n          n: { type: string }\ntypes:\n")
        text = p(text, "    transitions:\n      join:", "    conditions:\n      known: { description: The label is known., "
                 f"expression: '{expr}', remedy: dependent }}\n    transitions:\n      join:")
        return p(text, "inputs: { text: { type: string } }, effect: [ { assign: { location: label",
                 "inputs: { text: { type: string } }, guards: { known: deny }, effect: [ { assign: { location: label")
    cases = [
        ("a number to a string", step(PERSONAL_CRM, CLOSE, ["{ call: { target: customer, transition: tag, inputs: { text: 5 } } }"]),
         [refused("Visit.close passes 5, number, to Customer.tag's input 'text', which takes string")]),
        ("the clock to an attribute input holding text",
         step(PERSONAL_CRM, LEAVE, ["{ create: { type: Visit, transition: book, inputs: { customer: this, note: now } } }"]),
         [refused("Customer.leave passes now, timestamp, to Visit.book's input 'note', which takes string")]),
        ("an object of the wrong type to a reference",
         step(PERSONAL_CRM, CLOSE, ["{ create: { type: Visit, transition: book, inputs: { customer: this } } }"]),
         [refused("Visit.close passes this, reference Visit, to Visit.book's input 'customer', which takes reference Customer")]),
        ("an evaluator's argument of the wrong type", asking("crmcheck.known(5)"),
         [refused("condition known passes number to crmcheck.known's argument 'n', which takes string")]),
        ("a negated evaluator's argument of the wrong type", asking("not crmcheck.known(now)"),
         [refused("condition known passes timestamp to crmcheck.known's argument 'n', which takes string")]),
        ("a computed argument of another type",
         step(PERSONAL_CRM, CLOSE, ['{ call: { target: customer, transition: tag, inputs: { text: "now - now" } } }']),
         [refused("Visit.close passes now - now, duration, to Customer.tag's input 'text', which takes string")]),
        ("a computed argument that fits",
         step(PERSONAL_CRM, LEAVE, ['{ create: { type: Visit, transition: book, inputs: { customer: "if open then this else this" } } }']), []),
        # an identity and a string given the benefit of the doubt, as a comparison gives them
        ("a principal passed to an identity input",
         step(p(PERSONAL_CRM, "      tag:    {", "      note_by: { kind: internal, from: [ACTIVE], inputs: { who: { type: identity } } }\n      tag:    {"),
              CLOSE, ["{ call: { target: customer, transition: note_by, inputs: { who: actor.principal } } }"]), []),
        ("an evaluator's argument that fits", asking("crmcheck.known(label)"), []),
        ("arguments that fit", step(step(PERSONAL_CRM, CLOSE, ["{ call: { target: customer, transition: tag, inputs: { text: '\"vip\"' } } }"]),
                                    LEAVE, ["{ create: { type: Visit, transition: book, inputs: { customer: this } } }"]), []),
    ]
    ok = True
    for name, text, want in cases:
        found, _ = check([("people.yaml", people), ("crm.yaml", text)])
        got = sorted(x[4] for x in found)
        hit = got == sorted(want) and all(x[3] == "names" for x in found)
        ok &= hit
        print(f"  check 2, {name}: {'as expected' if hit else 'NOT: ' + str(found)}")
    return ok


def symmetry_plants(people):
    """A type-scan invariant is symmetric by the swap test of declaration-syntax.md §3.4, or refused naming the member
    read on one of the two objects only (the model's check 6, ADR-0061, ADR-0063): an overlap with its status filter on
    both objects, an unfiltered count and a derivation over a symmetric scan pass; a status filter on one object, a
    condition on the object outside the scan, a count bounded by its own member, an overlap with a direction wrong, a
    derivation over an asymmetric scan, and a scan excluding the object itself, by its id or by `this`, are refused."""
    probe = ("none(b in Booking where b.room == room and b.starts < ends and b.ends > starts and b.open and open)")

    def desk(expr):
        assert SYMMETRY_DESK.count(probe) == 1
        return SYMMETRY_DESK.replace(probe, expr)

    def asymmetric(members):
        why = (f"it reads {members} of one of the two objects only, so its verdict depends on which is written" if members
               else "with the two objects exchanged its predicate is not the one written, under the four normalisations "
                    "of the swap test")
        return f"Booking's invariant 'probe' scans Booking and is not symmetric: {why} (the model's check 6)"
    excluded = "Booking's invariant 'probe' excludes the object itself from its scan of Booking, which never ranges over it (the model's check 6)"
    cases = [
        ("an overlap filtered on both objects", probe, []),
        ("an unfiltered count", "count(b in Booking) < 1000", []),
        ("a derivation over a symmetric scan", "same_room < 3", []),
        ("a status filter on one object", "none(b in Booking where b.room == room and b.open)", [asymmetric("open")]),
        ("a condition on the object outside the scan", "open implies none(b in Booking where b.room == room)",
         [asymmetric("open")]),
        ("a count bounded by its own member", "cap is null or count(b in Booking where b.room == room) < cap",
         [asymmetric("cap")]),
        ("an overlap with a direction wrong", "none(b in Booking where b.room == room and b.starts < ends and b.ends < starts)",
         [asymmetric("")]),
        ("a derivation over an asymmetric scan", "others_held < 3", [asymmetric("open")]),
        ("a scan excluding the object by its id", "none(b in Booking where b.room == room and b.id != id)", [excluded]),
        ("a scan excluding the object as this", "none(b in Booking where b.room == room and b != this)", [excluded]),
    ]
    ok = True
    for name, expr, want in cases:
        text = desk(expr)
        found, _ = check([("people.yaml", people), ("desk.yaml", text)])
        line = next(i for i, l in enumerate(text.splitlines(), 1) if l.startswith("      probe:"))
        expected = [("desk.yaml", line, "fatal", "names", m) for m in want]
        hit = sorted(found) == sorted(expected)
        ok &= hit
        print(f"  check 6, {name}: {'as expected' if hit else 'NOT: ' + str(found)}")
    return ok


def could_violate_plants(people):
    """Each invariant a type declares is reported with the transitions that could violate it (DESIGN.md §5.6): those
    writing what it reads, its own members, through a derivation, a personal one an erasure writes, `referrers` that a
    reference to it writes, a member through a path, every object of a type it scans and the state through `open`, and
    its own type's creations; none that writes nothing it reads; and a machine's transition by its binder alone."""
    want = {"Customer.called": ["Customer.forget", "Customer.join"],
            "Customer.few_referrers": ["Customer.join", "Visit.book"],
            "Customer.labelled": ["Customer.join"],
            "Visit.few_similar": ["Visit.book"],
            "Visit.known_customer": ["Visit.book"],
            "Visit.labelled_customer": ["Visit.book"],
            "Visit.one_open": ["Visit.book", "Visit.close"]}
    violators = {}
    found, _ = check([("people.yaml", people), ("crm.yaml", INVARIANT_FORMS)], None, None, None, None, None, violators)
    ok = not found and beyond_people(violators) == want
    print(f"  each invariant with the transitions that could violate it: {'as expected' if ok else 'NOT: ' + str((found, violators))}")
    # a set end is written by the transition writing its far, stored end, and a scan by its type's creations
    counted = {}
    lined = SYNTAX_PARTS.replace("    transitions:\n      open:     { kind: initial, to: OPEN }\n",
                                 "    invariants:\n      few_lines: { description: Few lines., expression: count(l in lines) < 10 }\n"
                                 "      lines_anywhere: { description: Few lines anywhere., expression: count(o in Line) < 99 }\n"
                                 "    transitions:\n      open:     { kind: initial, to: OPEN }\n", 1)
    found, _ = check([("people.yaml", people), ("shop.yaml", lined)], None, None, None, None, None, counted)
    ends = not found and beyond_people(counted) == {"Order.few_lines": ["Line.start", "Order.open"],
                                     "Order.lines_anywhere": ["Line.start", "Order.open"]}
    print(f"  a set end by its far end's writer, a scan by its type's creations: {'as expected' if ends else 'NOT: ' + str((found, counted))}")
    ok &= ends
    # a machine's transition is named by the type binding it, never by the machine
    seated = {}
    found, _ = check([("people.yaml", people), ("office.yaml", MACHINE_OFFICE)], None, None, None, None, None, seated)
    bound = not found and beyond_people(seated) == {"Desk.crowded": ["Chair.sit", "Desk.make"]}
    print(f"  a machine's transition named by its binder alone: {'as expected' if bound else 'NOT: ' + str((found, seated))}")
    return ok and bound


def beyond_people(by_invariant):
    """What a check found of each invariant, but those of people.yaml's actor types, which a plant loads for its actors
    alone: each takes `taken_by`, generating an invariant per transition (ADR-0176), which the studio's publishing tests
    hold through both backends."""
    return {k: v for k, v in by_invariant.items() if k.split(".", 1)[0] not in ("User", "Agent", "Service")}


def invariant_form_plants(people):
    """Each invariant a type declares is reported by its form (declaration-syntax.md §3.4, ADR-0055, ADR-0171): local
    over its own members and a derivation over them; traversal through a reference, over `referrers`, and through a
    derivation that traverses; a type scan by name, and through a derivation that scans; and an invariant a subtype
    inherits only where its base declares it."""
    want = {"Customer.called": "local", "Customer.few_referrers": "traversal", "Customer.labelled": "local",
            "Visit.few_similar": "type_scan", "Visit.known_customer": "traversal", "Visit.labelled_customer": "traversal",
            "Visit.one_open": "type_scan"}
    forms = {}
    found, _ = check([("people.yaml", people), ("crm.yaml", INVARIANT_FORMS)], None, None, None, None, forms)
    ok = not found and beyond_people(forms) == want
    print(f"  each invariant by its form: {'as expected' if ok else 'NOT: ' + str((found, forms))}")
    pieces = "      pieces: { reference: \"Piece[]\", opposite: asset, aggregation: composite }\n"
    based = {}
    found, _ = check([("people.yaml", people), ("kit.yaml", REPARENT_KIT.replace(pieces, pieces +
                      "    invariants:\n      few_pieces: { description: Few pieces., expression: \"count(x in pieces) < 9\" }\n"))],
                     None, None, None, None, based)
    inherited_once = not found and beyond_people(based) == {"Asset.few_pieces": "traversal"}
    print(f"  an inherited invariant by its form, at its base alone: {'as expected' if inherited_once else 'NOT: ' + str((found, based))}")
    return ok and inherited_once


def invariant_read_plants(people):
    """An invariant reading a request's inputs or event is refused, since it is checked after a write, where neither is
    (the model's check 5: an invariant of none of the forms of declaration-syntax.md §3.4); one over `referrers`, a
    traversal every referring object's stored end finds, is not, and nor is a condition reading an input."""
    def p(text, old, new):
        assert text.count(old) == 1, old
        return text.replace(old, new, 1)

    def invariant(expr):
        return p(PERSONAL_CRM, "    transitions:\n      join:",
                 f"    invariants:\n      probe: {{ description: A probe., expression: \"{expr}\" }}\n    transitions:\n      join:")
    conditioned = p(p(PERSONAL_CRM, "    transitions:\n      join:",
                      "    conditions:\n      worded: { description: The text has words., expression: \"inputs.text != \\\"\\\"\", remedy: self_serviceable }\n"
                      "    transitions:\n      join:"),
                    "inputs: { text: { type: string } }, effect: [ { assign: { location: label",
                    "inputs: { text: { type: string } }, guards: { worded: deny }, effect: [ { assign: { location: label")
    cases = [
        ("an invariant reading inputs", invariant("inputs.text is null"), ["probe reads 'inputs', which only a transition has"]),
        ("an invariant reading this_event", invariant("this_event is not null"), ["probe reads 'this_event', which only a transition has"]),
        ("an invariant over referrers", invariant("count(r in referrers) < 5"), []),
        ("a condition reading an input", conditioned, []),
    ]
    ok = True
    for name, text, want in cases:
        found, _ = check([("people.yaml", people), ("crm.yaml", text)])
        line = next((i for i, l in enumerate(text.splitlines(), 1) if l.startswith("      probe:")), None)
        expected = [("crm.yaml", line, "fatal", "names", m) for m in want]
        hit = sorted(found) == sorted(expected)
        ok &= hit
        print(f"  check 5, {name}: {'as expected' if hit else 'NOT: ' + str(found)}")
    return ok


def reparent_plants(people):
    """A re-parent writing a part's end back to a whole of an abstract type is refused, a `do` or an `act`, by an
    `assign` or by an attribute input, required or optional (the model's check 36, ADR-0058); a creation writing it,
    and a re-parent to a concrete whole, are not; and a part whose end back names another type than its whole is
    refused as ends that do not name each other, check 36's other clause."""
    def p(text, old, new):
        assert text.count(old) == 1, old
        return text.replace(old, new, 1)
    DROP = "      drop: { kind: external, from: [IN], to: OUT }\n"

    def moving(line):
        return p(REPARENT_KIT, DROP, DROP + line)

    def refused(xn):
        return (f"Piece.{xn} re-parents a part, writing 'asset', whose type Asset is abstract, so which whole's "
                "invariants it re-checks is not known from the text (the model's check 36, ADR-0058)")
    racked = p(p(ABSTRACT_PART, ", cascade: [ { on: [retire], transition: drop, limit: 10 } ] }", ", survives: true }"),
               "      drop: { kind: external, from: [IN], to: OUT }\n",
               "      drop: { kind: external, from: [IN], to: OUT }\n"
               "      move: { kind: internal, from: [IN], inputs: { to: { reference: Rack } }, "
               "effect: [ { assign: { location: rack, expr: inputs.to } } ] }\n")
    cases = [
        ("an act assigning it", "kit.yaml",
         moving("      move: { kind: internal, from: [IN], inputs: { to: { reference: Asset } }, "
                "effect: [ { assign: { location: asset, expr: inputs.to } } ] }\n"),
         [("      move:", refused("move"))]),
        ("a do taking it as a required input", "kit.yaml",
         moving("      move: { kind: external, from: [IN], to: OUT, required_inputs: [asset] }\n"),
         [("      move:", refused("move"))]),
        ("an act taking it as an optional input, which the required end refuses as well", "kit.yaml",
         moving("      move: { kind: internal, from: [IN], optional_inputs: [asset] }\n"),
         [("      move:", refused("move")),
          ("      move:", "Piece.move lists 'asset' as an optional input, but the attribute is neither optional nor "
                          "defaulted, so the input is required")]),
        ("a creation writing it", "kit.yaml", REPARENT_KIT, []),
        ("a write of a reference to the abstract type that is no part's end back", "kit.yaml",
         p(p(moving("      lend: { kind: internal, from: [IN], optional_inputs: [spare] }\n"),
             "      pieces: { reference: \"Piece[]\", opposite: asset, aggregation: composite }\n",
             "      pieces: { reference: \"Piece[]\", opposite: asset, aggregation: composite }\n"
             "      spares: { reference: \"Piece[]\", opposite: spare }\n"),
           "      asset: { reference: Asset, opposite: pieces }\n",
           "      asset: { reference: Asset, opposite: pieces }\n      spare: { reference: Asset, optional: true, opposite: spares }\n"),
         []),
        ("a re-parent to a concrete whole", "rack.yaml", racked, []),
        ("a part whose end back names another type", "kit.yaml",
         p(REPARENT_KIT, "      asset: { reference: Asset, opposite: pieces }", "      asset: { reference: Robot, opposite: pieces }"),
         [("      pieces:", "Asset.pieces's opposite Piece.asset does not name Asset.pieces back")]),
    ]
    ok = True
    for name, f, text, want in cases:
        found, _ = check([("people.yaml", people), (f, text)])
        lines = text.splitlines()
        expected = [(f, next(i for i, l in enumerate(lines, 1) if l.startswith(at)), "fatal", "names", m) for at, m in want]
        hit = sorted(found) == sorted(expected)
        ok &= hit
        print(f"  check 36, {name}: {'as expected' if hit else 'NOT: ' + str(found)}")
    return ok


def personal_plants(people):
    """A personal value reaching a non-personal attribute is refused, at the step, clause or backfill that sends it,
    saying what it carries and where it lands (the model's check 10, ADR-0078): an own personal attribute, one and a
    derivation of its own as a function's argument, which the typed walker passes over, a marked
    input, an input written to a personal attribute as well, one through a path, one through a derivation, a test of
    its presence, an observation's personal field, a call's argument, one passed on by a second call, a creation's into
    an attribute input, a cascade's, and a backfill's; personal values reaching only personal attributes are not; and
    one passed to an input the transition reached holds personal itself is reported once, where that one writes it."""
    def p(text, old, new):
        assert text.count(old) == 1, old
        return text.replace(old, new, 1)

    def step(text, tn_line, steps):
        """The transition on the line beginning `tn_line` given an effect of the steps."""
        line = next(l for l in text.splitlines(keepends=True) if l.startswith(tn_line))
        return p(text, line, line.rstrip("\n")[:-2] + ", effect: [ " + ", ".join(steps) + " ] }\n")

    def leaks(what):
        return f"{what}; a personal value reaches only a personal attribute (the model's check 10)"
    LEAVE, CLOSE = "      leave:  {", "      close:  {"
    TAG = "      tag:    { kind: internal, from: [ACTIVE], inputs: { text: { type: string } },"
    ordered = p(p(p(p(p(SYNTAX_PARTS, "categories: [live, closed]\n", "categories: [live, closed]\nenumerations:\n  Reason: [ASKED]\n"),
                    "      finish:   { kind: external, from: [OPEN], to: DONE }\n",
                    "      finish:   { kind: external, from: [OPEN], to: DONE }\n"
                    "      forget:   { kind: erasure, inputs: { reason: { type: Reason } } }\n"),
                  "      lines: { reference:", "      buyer: { type: string, optional: true, personal: true }\n      lines: { reference:"),
                  "survives: true }", "cascade: [ { on: [finish], transition: drop, inputs: { why: buyer }, limit: 50 } ] }"),
                "      order: { reference: Order, opposite: lines }\n",
                "      order: { reference: Order, opposite: lines }\n      memo:  { type: string, optional: true }\n")
    ordered = p(ordered, "      drop:  { kind: external, from: [ACTIVE], to: GONE }\n",
                "      drop:  { kind: external, from: [ACTIVE], to: GONE, inputs: { why: { type: string } }, "
                "effect: [ { assign: { location: memo, expr: inputs.why } } ] }\n")
    sized = p(PERSONAL_CRM, "      label: { type: string, optional: true }\n",
              "      label: { type: string, optional: true }\n      size:  { type: int, optional: true }\n")
    aliased = p(PERSONAL_CRM, "      label: { type: string, optional: true }\n",
                "      label: { type: string, optional: true }\n      alias: { type: string, optional: true }\n") \
        + "migration:\n  description: Customers gain an alias.\n  backfill:\n    Customer: { alias: name }\n"
    cases = [
        ("an own personal attribute", "crm.yaml", step(PERSONAL_CRM, LEAVE, ["{ assign: { location: label, expr: name } }"]),
         [leaks("Customer.leave writes the personal Customer.name to 'label', which is not personal")]),
        ("an own personal attribute as a function's argument", "crm.yaml",
         step(sized, LEAVE, ["{ assign: { location: size, expr: length(name) } }"]),
         [leaks("Customer.leave writes the personal Customer.name to 'size', which is not personal")]),
        ("a derivation of its own as a function's argument", "crm.yaml",
         step(sized, LEAVE, ["{ assign: { location: size, expr: length(called) } }"]),
         [leaks("Customer.leave writes the personal value in the derived Customer.called to 'size', which is not personal")]),
        ("a marked input", "crm.yaml", p(PERSONAL_CRM, TAG, TAG.replace("{ type: string }", "{ type: string, personal: true }")),
         [leaks("Customer.tag writes the personal input 'text' to 'label', which is not personal")]),
        ("an input written to a personal attribute as well", "crm.yaml",
         p(PERSONAL_CRM, "effect: [ { assign: { location: label, expr: inputs.text } } ]",
           "effect: [ { assign: { location: name, expr: inputs.text } }, { assign: { location: label, expr: inputs.text } } ]"),
         [leaks("Customer.tag writes the personal input 'text' to 'label', which is not personal")]),
        ("one through a path", "crm.yaml", step(PERSONAL_CRM, CLOSE, ["{ assign: { location: summary, expr: customer.name } }"]),
         [leaks("Visit.close writes the personal Customer.name to 'summary', which is not personal")]),
        ("one through a derivation", "crm.yaml", step(PERSONAL_CRM, CLOSE, ["{ assign: { location: summary, expr: customer.called } }"]),
         [leaks("Visit.close writes the personal value in the derived Customer.called to 'summary', which is not personal")]),
        ("a test of its presence", "crm.yaml",
         step(PERSONAL_CRM, CLOSE, ["{ assign: { location: summary, expr: 'if customer.name is null then \"none\" else \"some\"' } }"]),
         [leaks("Visit.close writes the personal Customer.name to 'summary', which is not personal")]),
        ("an observation's personal field", "crm.yaml",
         step(PERSONAL_CRM, CLOSE, ["{ assign: { location: flagged, expr: 'count(n in notes where n.text == \"urgent\")' } }"]),
         [leaks("Visit.close writes the personal VisitNote.text to 'flagged', which is not personal")]),
        ("a call's argument", "crm.yaml",
         step(PERSONAL_CRM, CLOSE, ["{ call: { target: customer, transition: tag, inputs: { text: customer.name } } }"]),
         [leaks("Visit.close passes the personal Customer.name to Customer.tag's input 'text', which writes it to "
                "Customer.label, which is not personal")]),
        ("one passed on by a second call", "crm.yaml",
         step(PERSONAL_CRM, CLOSE, ["{ call: { target: customer, transition: relay, inputs: { text: customer.name } } }"]),
         [leaks("Visit.close passes the personal Customer.name to Customer.relay's input 'text', which passes it to "
                "Customer.tag, which writes it to Customer.label, which is not personal")]),
        ("a creation's into an attribute input", "crm.yaml",
         step(PERSONAL_CRM, LEAVE, ["{ create: { type: Visit, transition: book, inputs: { customer: this, note: name } } }"]),
         [leaks("Customer.leave passes the personal Customer.name to Visit.book's input 'note', which writes it to "
                "Visit.note, which is not personal")]),
        ("a cascade's", "shop.yaml", ordered,
         [leaks("Order.finish cascades the personal Order.buyer to Line.drop's input 'why', which writes it to "
                "Line.memo, which is not personal")]),
        ("a backfill's", "crm.yaml", aliased,
         [leaks("the backfill of Customer.alias reads the personal Customer.name, and alias is not personal; a backfill "
                "is a write")]),
        ("personal values reaching only personal attributes", "crm.yaml",
         step(PERSONAL_CRM, CLOSE, ["{ assign: { location: contact, expr: customer.name } }",
                                    "{ call: { target: customer, transition: rename, inputs: { text: customer.called } } }",
                                    "{ assign: { location: flagged, expr: 'count(n in notes where n.urgent)' } }",
                                    "{ create: { type: Customer, transition: join, inputs: { name: customer.name } } }"]), []),
        ("an input the transition reached holds personal itself, reported where it is written", "crm.yaml",
         step(p(PERSONAL_CRM, TAG, TAG.replace("{ type: string }", "{ type: string, personal: true }")), CLOSE,
              ["{ call: { target: customer, transition: tag, inputs: { text: customer.name } } }"]),
         [leaks("Customer.tag writes the personal input 'text' to 'label', which is not personal")]),
    ]
    ok = True
    for name, f, text, want in cases:
        previous = {f: PERSONAL_CRM} if name == "a backfill's" else None
        found, _ = check([("people.yaml", people), (f, text)], previous)
        got = sorted(x[4] for x in found)
        hit = got == sorted(want) and all(x[3] == "names" for x in found)
        ok &= hit
        print(f"  check 10, {name}: {'as expected' if hit else 'NOT: ' + str(found)}")
    return ok


def abstract_part_plants(people):
    """A cascaded part is of a concrete type: a whole cascading to parts of an abstract type is refused though every
    member of the family declares the transition, which subsumes the model's check 30, and the same end surviving
    the whole instead is not (flow-format.md §4.14, ADR-0170)."""
    survived = ABSTRACT_PART.replace(", cascade: [ { on: [retire], transition: drop, limit: 10 } ] }", ", survives: true }")
    assert survived != ABSTRACT_PART
    line = next(i for i, l in enumerate(ABSTRACT_PART.splitlines(), 1) if l.startswith("      slots:"))
    cases = [
        ("a cascade to an abstract part type", ABSTRACT_PART,
         [("rack.yaml", line, "fatal", "names", "Rack.slots cascades to Slot.drop, which is not a transition of Slot that acts on a part")]),
        ("the same part surviving its whole", survived, []),
    ]
    ok = True
    for name, text, want in cases:
        found, _ = check([("people.yaml", people), ("rack.yaml", text)])
        hit = sorted(found) == sorted(want)
        ok &= hit
        print(f"  ADR-0170, {name}: {'as expected' if hit else 'NOT: ' + str(found)}")
    return ok


def clock_plants(people):
    """An invariant reading the clock is refused, saying how it reads it (the model's check 32, declaration-syntax.md
    §8.3): `now`, `time_in`, a span's duration through a derivation, a derived attribute of its type, one through
    another derivation, and one of another type reached through a path; an invariant reading `entered_at` and a
    derivation that reads no clock is not."""
    def p(text, old, new):
        assert text.count(old) == 1, old
        return text.replace(old, new, 1)

    def mini(derived, invariants):
        return p(SYNTAX_MINI, "    conditions:\n", "".join(
            ["    derived_attributes:\n"] * bool(derived)
            + [f"      {n}: {{ description: Derived., expression: \"{e}\" }}\n" for n, e in derived]
            + ["    invariants:\n"]
            + [f"      {n}: {{ description: An invariant., expression: \"{e}\" }}\n" for n, e in invariants]) + "    conditions:\n")

    def refused(tn, i, reason):
        return (f"{tn}'s invariant '{i}' {reason}; the clock moves without a write, so no check after one can enforce "
                "it, and a rule about time is a guard (the model's check 32)")
    stale = ("stale", "created_at + 30 days < now")
    late = p(p(SYNTAX_PARTS,
               "      DONE: { category: closed, final: true }\n    transitions:\n",
               "      DONE: { category: closed, final: true }\n"
               "    derived_attributes:\n      late: { description: Open past a day., expression: \"created_at + 1 day < now\" }\n"
               "    transitions:\n"),
             "      GONE:   { category: closed, final: true }\n",
             "      GONE:   { category: closed, final: true }\n"
             "    invariants:\n      on_time: { description: Its order is not late., expression: not order.late }\n")
    cases = [
        ("now", "mini.yaml", mini([], [("recent", "created_at <= now")]),
         [refused("Job", "recent", "reads now, the clock")]),
        ("time_in", "mini.yaml", mini([], [("brief", "time_in(OPEN) < 30 days")]),
         [refused("Job", "brief", "reads time_in, which counts to now")]),
        ("a span's duration through a derivation", "mini.yaml",
         mini([("waited", "sum(i in this.intervals where i.state == OPEN: i.duration)")], [("short", "waited < 30 days")]),
         [refused("Job", "short", "reads Job.waited, a derived attribute that reads the clock")]),
        ("a derived attribute of its type", "mini.yaml", mini([stale], [("fresh", "not stale")]),
         [refused("Job", "fresh", "reads Job.stale, a derived attribute that reads the clock")]),
        ("one through another derivation", "mini.yaml",
         mini([stale, ("urgent", "stale and note is null")], [("calm", "not urgent")]),
         [refused("Job", "calm", "reads Job.urgent, a derived attribute that reads the clock")]),
        ("one of another type through a path", "shop.yaml", late,
         [refused("Line", "on_time", "reads Order.late, a derived attribute that reads the clock")]),
        ("entered_at and a derivation reading no clock", "mini.yaml",
         mini([("named", "title is not null")], [("entered", "entered_at(OPEN) >= created_at"), ("has_name", "named")]), []),
    ]
    ok = True
    for name, f, text, want in cases:
        found, _ = check([("people.yaml", people), (f, text)])
        got = [x[4] for x in found if x[3] == "names" and "check 32" in x[4]]
        hit = got == want and len(found) == len(want)
        ok &= hit
        print(f"  check 32, {name}: {'as expected' if hit else 'NOT: ' + str(found)}")
    return ok


def cascade_cycle_plants(people):
    """Transitions causing each other in a cycle are refused, each cycle once for the transitions it passes through,
    at the step or the clause closing it (the model's check 12, ADR-0019): by a call back to the whole whose cascade
    reached the part, by a call in a loop to the transition taking it, by a creation of the type creating, and through
    a part an abstract base declares; and a cascade whose part causes nothing back is not."""
    def p(text, old, new):
        assert text.count(old) == 1, old
        return text.replace(old, new, 1)

    def ring(*names):
        return ("a transition causes itself, by a call, a create or a cascade: " + " → ".join(names)
                + "; no transition may cause itself, on its own object or another (the model's check 12)")

    cascaded = p(SYNTAX_PARTS, "aggregation: composite, survives: true }",
                 "aggregation: composite, cascade: [ { on: [finish], transition: drop, limit: 50 } ] }")
    drop = "      drop:  { kind: external, from: [ACTIVE], to: GONE }\n"

    def dropping(*steps):
        return p(cascaded, drop, drop[:-3] + ", effect: [ " + ", ".join(steps) + " ] }\n")
    back = "{ call: { target: order, transition: finish } }"
    loop = "{ foreach: { item: l, array: order.lines, limit: 50, steps: [ { call: { target: l, transition: drop } } ] } }"
    made = p(SYNTAX_MINI, "optional_inputs: [note] }",
             "optional_inputs: [note], effect: [ { create: { type: Job, transition: open, inputs: { title: title } } } ] }")
    retired = p(CYCLE_FLEET, "      drop: { kind: external, from: [IN], to: OUT }\n",
                "      drop: { kind: external, from: [IN], to: OUT, effect: [ { call: { target: spare, transition: retire } } ] }\n")
    cases = [
        ("a call back to the whole whose cascade reached the part", "shop.yaml", dropping(back),
         [("      drop:", ring("Order.finish", "Line.drop", "Order.finish"))]),
        ("the same cycle closed twice, reported once", "shop.yaml", dropping(back, back),
         [("      drop:", ring("Order.finish", "Line.drop", "Order.finish"))]),
        ("a call in a loop to the transition taking it", "shop.yaml", dropping(loop),
         [("      drop:", ring("Line.drop", "Line.drop"))]),
        ("two cycles through one transition, each reported", "shop.yaml", dropping(back, loop),
         [("      drop:", ring("Order.finish", "Line.drop", "Order.finish")), ("      drop:", ring("Line.drop", "Line.drop"))]),
        ("a creation of the type creating", "mini.yaml", made, [("      open:", ring("Job.open", "Job.open"))]),
        ("through a part an abstract base declares", "fleet.yaml", retired,
         [("      drop:", ring("Robot.retire", "Piece.drop", "Robot.retire"))]),
        ("a cascade whose part causes nothing back", "shop.yaml", cascaded, []),
    ]
    ok = True
    for name, f, text, want in cases:
        found, _ = check([("people.yaml", people), (f, text)])
        lines = text.splitlines()
        expected = [(f, next(i for i, l in enumerate(lines, 1) if l.startswith(at)), "fatal", "names", m) for at, m in want]
        hit = sorted(found) == sorted(expected)
        ok &= hit
        print(f"  check 12, {name}: {'as expected' if hit else 'NOT: ' + str(found)}")
    return ok


def refusal_order_plant(people):
    """A request leaving out an input its transition requires, from a state the transition does not leave, is refused
    as unavailable, which DESIGN.md §6 orders before invalid_input; one expected to be refused later still is (D517)."""
    service = (EXAMPLES / "service.yaml").read_text()
    examples = (EXAMPLES / "service.examples.yaml").read_text().rstrip("\n")
    opened = ("    given: two_engineers\n"
              "    steps:\n"
              "      - request: { type: ServiceJob, transition: open, as: job, actor: ana, inputs: { engineer: ana } }\n"
              "    request: { object: job, transition: finish, actor: ana }\n")
    early = examples + ("\n\n  a_job_just_opened_is_not_finished:\n"
                        "    description: A job just opened cannot be finished, whatever the request leaves out.\n"
                        + opened + "    expect: { verdict: unavailable }\n")
    found, _ = check([("people.yaml", people), ("service.yaml", service), ("service.examples.yaml", early)])
    accepted = not found
    late = examples + ("\n\n  a_job_just_opened_is_refused_late:\n"
                       "    description: Expected refused by a guard, which a left-out input comes before.\n"
                       + opened + "    expect: { verdict: unsatisfied, clause: photo_attached, remedy: self_serviceable }\n")
    found, _ = check([("people.yaml", people), ("service.yaml", service), ("service.examples.yaml", late)])
    refused = any("is refused as invalid_input" in f[4] for f in found)
    ok = accepted and refused
    print(f"  a left-out input does not decide a request its state refuses first, and still decides one refused later: "
          f"{'yes' if ok else 'NO ' + str((accepted, refused))}")
    return ok


def conversion_self_test():
    """A fixed measure's attribute dimension converts as ADR-0139 reads it: a tracked one, such as an enumeration, as
    it was held when the span began, and any other as it is now."""
    inventory = load((EXAMPLES / "inventory.yaml").read_text())
    inventory["types"]["InventoryItem"]["metrics"]["time_available"]["group_by"] = ["condition", "model", "month"]
    text = to_text(inventory)[0]
    ok = "condition = i.held(condition)" in text and "model = i.object.model" in text
    print(f"  a fixed measure's enumeration dimension converts as held, and a plain attribute as it is now: {'yes' if ok else 'NO'}")
    return ok


def digest_self_test():
    """The digest names the modules and their bytes, and nothing else: not the order the files come in."""
    a, b = ("inventory", b"module: inventory\n"), ("people", b"module: people\n")
    same = source_digest([a, b]) == source_digest([b, a])
    byte = source_digest([a, b]) != source_digest([a, ("people", b"module: people \n")])
    moved = source_digest([("ab", b"c")]) != source_digest([("a", b"bc")])
    try:
        source_digest([a, a])
        twice = False
    except ValueError:
        twice = True
    ok = same and byte and moved and twice
    print(f"  the source digest ignores file order, sees one changed byte and a moved boundary, and refuses a module twice: "
          f"{'yes' if ok else 'NO'}")
    return ok


# ── the checks the self-test makes, exported for the core (ADR-0156 decision 4) ──
PLANTS = ROOT / "core/crates/objectflow-core/tests/plants"
IMPORTED = " imported "   # before an alias: the type the import that gave it wrote, so a later import may refresh it
PUBLISHED = " published"   # the aliases' mark of a `publish` step, after which nothing is checked here: no alias
                           # has a space
RECORDED = []      # each check the run makes: its label, files, previous versions, findings, step 5's counts and the files
                   # its examples' `publish` steps read


def recording(check):
    """`check`, recording each call with what it found, labelled by its caller and,
    where the caller names it, the plant (ADR-0156 decision 4)."""
    def recorded(files, previous=None, analysed=None, examples=None, read=None, forms=None, violators=None):
        counts = {} if analysed is None else analysed
        shapes = {} if forms is None else forms
        writers = {} if violators is None else violators
        reads = {}

        def reading(name):
            # each file a `publish` step read, recorded so the core reads the same (ADR-0166 decision 3)
            text = read(name)
            if text is not None:
                reads[name] = text
            return text
        found, notes = check(files, previous, counts, examples, reading if read is not None else None, shapes, writers)
        caller = sys._getframe(1)
        name = caller.f_locals.get("name")
        label = caller.f_code.co_name + (f": {name}" if isinstance(name, str) else "")
        RECORDED.append((label, list(files), dict(previous or {}), found, notes, dict(counts), reads, dict(shapes),
                         dict(writers)))
        return found, notes
    return recorded


def this_checker():
    """This module, as the checker a `publish` step's dry run calls back into, however it was loaded: run as a script,
    imported, or loaded by path under a name `sys.modules` does not hold, as the studio loads it; its `check` the one in
    force, the recording one while the corpus records."""
    return sys.modules.get(__name__) or SimpleNamespace(**globals())


def mirror_identifier_self_test():
    """A mirror may declare a minted identifier, which the import fills and nothing mints while it is one: check 44's
    clauses on what every creation writes ask nothing of a type with no creation (ADR-0169), and a type with one is
    still held to them, by the plant of an identifier scoped by what not every creation writes."""
    module = ("module: sites\ncategories: [live]\nsequences:\n  site_code:\n    description: Numbers sites, by region.\n"
              "types:\n  Site:\n    description: A site, as the legacy system holds it.\n    mirror: true\n"
              "    tracking: record\n    attributes:\n      region: { type: string, indexed: true }\n"
              "      code:\n        type: string\n"
              "        identifier: { sequence: site_code, scope: region, format: \"{region}-{n:3}\" }\n"
              "        unique: in_scope\n"
              "      legacy_key: { type: string, external: legacy, indexed: true }\n"
              "    states:\n      OPEN: { category: live }\n")
    found, _notes = check([("sites.yaml", module)])
    print(f"  a mirror's minted identifier, scoped and formatted by what it holds: "
          f"{'accepted' if not found else 'REFUSED ' + str(found[:2])}")
    return not found


def beside(directory):
    """A reader of the files in an examples file's directory, by plain name, which its `publish` steps name (ADR-0166
    decision 3); a name with a directory in it reads nothing."""
    def read(name):
        path = directory / name
        return path.read_text() if pathlib.Path(name).name == name and path.is_file() else None
    return read


def plants_export(recorded):
    """The recorded checks as the core's fixtures: each file's text once, under the
    start of its SHA-256, and per check its files in order, the previous versions,
    and every finding and notice by file, line, severity, code and message, with
    each type's count of the guard conditions step 5 decided in full."""
    blobs, cases = {}, []

    def blob(text):
        key = hashlib.sha256(text.encode()).hexdigest()[:16]
        blobs[key] = text
        return key
    for label, files, previous, found, notes, counts, reads, shapes, writers in recorded:
        cases.append({
            "label": label,
            "files": [[n, blob(t)] for n, t in files],
            "previous": {n: blob(t) for n, t in sorted(previous.items())},
            "findings": sorted([list(x) for x in found + notes]),
            "analysed": {tn: list(c) for tn, c in sorted(counts.items())},
            "forms": dict(sorted(shapes.items())),
            "could_violate": dict(sorted(writers.items())),
            # the files beside the examples files that their `publish` steps read, which the core reads the same
            **({"reads": {n: blob(t) for n, t in sorted(reads.items())}} if reads else {}),
        })
    return json.dumps(cases, indent=1, ensure_ascii=False) + "\n", blobs


def plants_written(cases, blobs):
    PLANTS.mkdir(parents=True, exist_ok=True)
    (PLANTS / "blobs").mkdir(exist_ok=True)
    for old in (PLANTS / "blobs").glob("*.yaml"):
        if old.stem not in blobs:
            old.unlink()
    for key, text in blobs.items():
        (PLANTS / "blobs" / f"{key}.yaml").write_text(text)
    (PLANTS / "cases.json").write_text(cases)


def plants_current(cases, blobs):
    """Whether the committed fixtures are what this run records."""
    here = PLANTS / "cases.json"
    if not here.exists() or here.read_text() != cases:
        return False
    held = {p.stem for p in (PLANTS / "blobs").glob("*.yaml")}
    return held == set(blobs) and all((PLANTS / "blobs" / f"{k}.yaml").read_text() == t for k, t in blobs.items())


USAGE = """usage: check-flows.py [--previous=<file>] <module.yaml> [<module.examples.yaml> ...]
       check-flows.py --digest <module.yaml> ...
       check-flows.py [--export-plants]

Checks flow descriptions as docs/design/flow-format.md §7 states: the named files, a module's examples with it, and
with --previous=<file> the last file against the version before it. With no file, it checks the design's own modules
and runs its self-tests, failing if the core's fixtures, every check those make with what it found, are not current;
--export-plants writes them, to core/crates/objectflow-core/tests/plants (ADR-0156). Exits 1 on a fatal finding, 0
otherwise; notices never fail it."""


def main():
    if {"-h", "--help"} & set(sys.argv[1:]):
        print(USAGE)
        sys.exit(0)
    if "--digest" in sys.argv[1:]:
        paths = [pathlib.Path(a) for a in sys.argv[1:] if not a.startswith("-")]
        print(source_digest(module_files(paths)))
        sys.exit(0)
    args = [a for a in sys.argv[1:] if not a.startswith("-")]
    before = next((a.split("=", 1)[1] for a in sys.argv[1:] if a.startswith("--previous=")), None)
    if args:
        files = [(pathlib.Path(a).name, pathlib.Path(a).read_text()) for a in args]
        previous = {files[-1][0]: pathlib.Path(before).read_text()} if before else None
        counts = {}
        EXAMPLE_OUTCOMES.clear()
        carrying = [pathlib.Path(a) for a in args if a.endswith(".examples.yaml")]
        found, notes = check(files, previous, counts, None, beside(carrying[0].resolve().parent) if carrying else None)
        report(found, notes)
        if any(n.endswith(".examples.yaml") for n, _ in files):
            ran = [o for _n, o in EXAMPLE_OUTCOMES]
            print(f"examples: {len(ran)} run: {ran.count('passed')} passed, {ran.count('failed')} failed, "
                  f"{ran.count('not run')} not run" if ran else
                  "examples: none run, since step 6 runs only when every earlier step finds nothing fatal")
        # only where some are left open, where 1 of 1 decided in full spoke of "the rest" (twelfth production review)
        for tn, (whole, total) in counts.items():
            if whole < total:
                print(f"{tn}: step 5 decided {whole} of {total} guard conditions in full; the rest read something outside "
                      "the type, such as another object, a metric or an evaluator, which it leaves open")
        sys.exit(1 if found else 0)
    # every function of this module calls check through the module's namespace
    globals()["check"] = recording(check)
    people = (EXAMPLES / "people.yaml").read_text()
    clean = True
    needs = {"delivery.yaml": ["inventory.yaml"]}
    versions_of = {"inventory-v2.yaml": "inventory.yaml", "service-v2.yaml": "service.yaml"}   # each checked against the one before
    for version in ("people.yaml", "inventory.yaml", "inventory-v2.yaml", "service.yaml", "service-v2.yaml", "delivery.yaml", "approvals.yaml", "bookings.yaml",
                    "customers.yaml", "issues.yaml", "servicedesk.yaml", "reimbursement.yaml"):
        before = [(n, (EXAMPLES / n).read_text()) for n in needs.get(version, [])]
        files = [("people.yaml", people)] + before + ([(version, (EXAMPLES / version).read_text())] if version != "people.yaml" else [])
        carried = EXAMPLES / version.replace(".yaml", ".examples.yaml")   # a module's examples, checked with it (§11)
        files += [(carried.name, carried.read_text())] if carried.exists() else []
        found, notes = check(files, {version: (EXAMPLES / versions_of[version]).read_text()} if version in versions_of else None,
                             None, None, beside(EXAMPLES))
        print(f"{version}: {'clean' if not found else str(len(found)) + ' finding(s)'}, "
              f"{len(notes)} notice{'s' if len(notes) != 1 else ''}")
        for x in found:
            print(f"  {x[0]}:{x[1]} {x[3]} {x[4]}")
        # the design's own examples prove the reference runner, so one it does not run fails the corpus, where a
        # description being written is only told (ADR-0136)
        unrun = [x for x in notes if x[3] == "notrun"]
        for x in unrun:
            print(f"  {x[0]}:{x[1]} notrun {x[4]}")
        clean &= not found and not unrun
    # an examples file a change never carries, whose `publish` steps read their files beside it, checked with its
    # module as a corpus file is (§11.3, ADR-0166 decision 3)
    for module, examples in (("service.yaml", "service-migration.examples.yaml"),):
        files = [("people.yaml", people), (module, (EXAMPLES / module).read_text()),
                 (examples, (EXAMPLES / examples).read_text())]
        found, notes = check(files, None, None, None, beside(EXAMPLES))
        unrun = [x for x in notes if x[3] == "notrun"]
        print(f"{examples}: {'clean' if not found else str(len(found)) + ' finding(s)'}, "
              f"{len(notes)} notice{'s' if len(notes) != 1 else ''}")
        for x in found + unrun:
            print(f"  {x[0]}:{x[1]} {x[3]} {x[4]}")
        clean &= not found and not unrun
    ok = self_test(people, (EXAMPLES / "service.yaml").read_text(), (EXAMPLES / "inventory.yaml").read_text(),
                   (EXAMPLES / "delivery.yaml").read_text(), (EXAMPLES / "approvals.yaml").read_text(),
                   (EXAMPLES / "customers.yaml").read_text(), (EXAMPLES / "servicedesk.yaml").read_text(),
                   (EXAMPLES / "service-v2.yaml").read_text())
    ok &= mirror_identifier_self_test()
    ok &= digest_self_test()
    ok &= flowexpr.main() == 0          # every expression in the design parses, and the grammar's own test
    ok &= flowlogic.self_test()
    ok &= flowrun.self_test()
    ok &= logic_plants(people, (EXAMPLES / "service.yaml").read_text())
    ok &= runner_plants(people, (EXAMPLES / "service.yaml").read_text(), (EXAMPLES / "service.examples.yaml").read_text())
    ok &= examples_self_test(people, (EXAMPLES / "service.yaml").read_text(), (EXAMPLES / "service.examples.yaml").read_text())
    ok &= inventory_plants(people)
    ok &= example_rule_plants()
    ok &= unknown_plants(people)
    ok &= reference_plants(people)
    ok &= split_plants(people)
    ok &= held_plants(people)
    ok &= present_path_plants()
    ok &= identity_plants()
    ok &= taken_by_plants()
    ok &= refresh_plants()
    ok &= veto_plants()
    ok &= conversion_self_test()
    ok &= decimal_plant(people)
    ok &= scale_plant()
    ok &= cutover_plant(people)
    ok &= supersession_plant(people)
    ok &= proposal_plants(people)
    ok &= date_plant(people)
    ok &= scan_plant(people)
    ok &= group_erasure_plant(people)
    ok &= binder_plant(people)
    ok &= abstract_summary_plant(people)
    ok &= migration_plants(people)
    ok &= metric_plants(people)
    ok &= range_plant(people)
    ok &= erasure_plants(people)
    ok &= binder_more_plants(people)
    ok &= migration_more_plants(people)
    ok &= metric_more_plants(people)
    ok &= cluster3_plants(people)
    ok &= refusal_order_plant(people)
    ok &= cascade_cycle_plants(people)
    ok &= clock_plants(people)
    ok &= abstract_part_plants(people)
    ok &= personal_plants(people)
    ok &= reparent_plants(people)
    ok &= invariant_read_plants(people)
    ok &= invariant_form_plants(people)
    ok &= could_violate_plants(people)
    ok &= symmetry_plants(people)
    ok &= argument_type_plants(people)
    ok &= identity_write_plants(people)
    ok &= identity_metric_plants(people)
    ok &= identifier_personal_plants(people)
    ok &= expression_type_plants(people)
    ok &= syntax_plants()
    cases, blobs = plants_export(RECORDED)
    if "--export-plants" in sys.argv[1:]:
        plants_written(cases, blobs)
        print(f"exported {len(RECORDED)} checks and {len(blobs)} files to {PLANTS.relative_to(ROOT)}")
    else:
        current = plants_current(cases, blobs)
        ok &= current
        print(f"the core's fixtures, {len(RECORDED)} checks and what each found: "
              f"{'current' if current else 'NOT current; run check-flows.py --export-plants'}")
    sys.exit(0 if clean and ok else 1)


if __name__ == "__main__":
    main()

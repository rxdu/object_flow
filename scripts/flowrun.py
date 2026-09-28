#!/usr/bin/env python3
"""The reference runner: a flow's examples executed against its rules (ADR-0136 decision 5).

It is an executable model of `docs/design/DESIGN.md` §6, a request's execution, and of the expression
language's meaning (`declaration-syntax.md` §8), written to be read against those sections rather than to be
fast: the store is a dictionary, and a request works on a copy that it keeps only if the request applies. It
is the Python side of the differential tests of ADR-0132 decision 2, so it shares no code with the engine, only
the parser of `flowexpr.py`.

What it cannot yet run it says so, never guessing: a construct outside the slices built so far raises
NotRunnable with the construct's name, and the example is reported as not run (`TODO.md`, the runner's slices).

Values: absence and unknown are one value, ABSENT (§8.2); a boolean is True, False or ABSENT; an int is an int;
a decimal, an amount, a duration in seconds and a timestamp in seconds since 1970 are Fractions; a state is
("state", type, name) and an enumeration value ("enum", enumeration, member); an object is named by its id; a
set is a tuple of values.
"""
import copy
import datetime
import re
from fractions import Fraction

import flowexpr

ABSENT = type("Absent", (), {"__repr__": lambda self: "absent"})()
SECONDS = {"s": 1, "min": 60, "h": 3600, "day": 86400, "days": 86400, "week": 604800, "weeks": 604800}
EPOCH = datetime.datetime(1970, 1, 1, tzinfo=datetime.timezone.utc)
CLOCK_START = "2026-01-01T00:00:00Z"            # flow-format.md §11.3
OPERATOR = "operator"                           # the alias, and the identity, of the store's operator (ADR-0126)


class NotRunnable(Exception):
    """A construct this slice of the runner does not execute; the example is reported, not passed."""


class ExampleError(Exception):
    """An example that cannot be what it claims, such as an evaluator with no stubbed answer."""


class Refused(Exception):
    def __init__(self, verdict, clause=None, remedy=None, detail=""):
        super().__init__(f"{verdict} {clause or ''} {remedy or ''} {detail}".strip())
        self.verdict, self.clause, self.remedy, self.detail = verdict, clause, remedy, detail


def timestamp(text):
    t = datetime.datetime.strptime(text.replace("Z", "+0000"), "%Y-%m-%dT%H:%M%z" if text.count(":") == 1 else "%Y-%m-%dT%H:%M:%S%z")
    return Fraction(int((t - EPOCH).total_seconds()))


def truth(v):
    """A value as the three-valued logic reads it: True, False or ABSENT (unknown)."""
    return v if v is True or v is False else ABSENT


class Obj:
    def __init__(self, oid, tn, state, created_at):
        self.id, self.type, self.state, self.attrs = oid, tn, state, {}
        self.created_at = created_at
        self.entered = {state: created_at}      # when it most recently entered each state it has been in
        self.version = 1
        self.id_order = 0                       # creation order, which ascending id order is (§5.1)


class Store:
    """The objects of one example, built from an empty store holding version 0 and its operator."""

    def __init__(self, kinds, enums):
        self.kinds, self.enums = kinds, enums
        self.now = timestamp(CLOCK_START)
        self.objects, self.counter = {}, 0
        op = Obj(OPERATOR, "Operator", "ACTIVE", self.now)
        op.attrs["identity"] = OPERATOR
        self.objects[OPERATOR] = op

    def new_id(self, tn):
        self.counter += 1
        return f"{tn.lower()}-{self.counter}"

    def type_of(self, oid):
        return self.objects[oid].type

    def is_a(self, tn, want):
        seen = set()
        while tn and tn not in seen:
            if tn == want:
                return True
            seen.add(tn)
            tn = (self.kinds.get(tn) or {}).get("extends")
        return False

    def actor_attr(self, tn):
        """The attribute holding an actor's identity, and its kind, if the type's objects are actors."""
        if tn == "Operator":
            return "identity", "human"
        for a, spec in ((self.kinds.get(tn) or {}).get("attributes") or {}).items():
            if "actor_kind" in (spec or {}):
                return a, spec["actor_kind"]
        return None

    def final(self, o):
        return bool(((self.kinds.get(o.type) or {}).get("states") or {}).get(o.state, {}).get("final"))


class Evaluator:
    """An expression's value over the store, for one object and one request (§8, §9.2)."""

    def __init__(self, store, tn, this=None, inputs=None, actor=None, stubs=None, filter_mode=False):
        self.s, self.tn, self.this = store, tn, this
        self.inputs, self.actor, self.stubs = inputs or {}, actor, stubs or {}
        self.filter_mode = filter_mode          # a filter leaves out an element it cannot decide (§8.2)
        self.t = store.kinds.get(tn) or {}

    # ── names ────────────────────────────────────────────────────────────────
    def name(self, n, env):
        if n in env:
            return env[n]
        if n == "now":
            return self.s.now
        if n == "this":
            return self.this if self.this is not None else ABSENT
        if n in ("inputs", "actor"):
            return ("marker", n)
        if n in ("referrers", "this_event"):
            raise NotRunnable(f"`{n}`")
        if n == "state" or n in (self.t.get("attributes") or {}) or n in (self.t.get("derived_attributes") or {}) \
                or n in (self.t.get("observations") or {}) or n in ("id", "open", "created_at", "created_by_kind"):
            return self.member(self.this, n) if self.this is not None else ABSENT
        if n in (self.t.get("states") or {}):
            return ("state", self.tn, n)
        if any(n == v.get("category") for v in (self.t.get("states") or {}).values()) or n == "closed":
            return ("category", n)
        return ("typeref", n)

    def member(self, base, m):
        if base is ABSENT:
            return ABSENT
        if isinstance(base, tuple) and base[0] == "marker":
            if base[1] == "inputs":
                return self.inputs.get(m, ABSENT)
            if self.actor is None:
                return ABSENT
            return {"id": self.actor["id"], "kind": self.actor["kind"], "principal": ABSENT}.get(m, ABSENT)
        if isinstance(base, tuple) and base[0] == "typeref":
            if base[1] in self.s.enums and m in self.s.enums[base[1]]:
                return ("enum", base[1], m)
            if m in ((self.s.kinds.get(base[1]) or {}).get("states") or {}):
                return ("state", base[1], m)
            raise NotRunnable(f"`{base[1]}.{m}`")
        if isinstance(base, tuple) and base[0] == "state" and m == "category":
            return ("category", ((self.s.kinds.get(base[1]) or {}).get("states") or {}).get(base[2], {}).get("category"))
        if not isinstance(base, str) or base not in self.s.objects:
            raise NotRunnable(f"a member `{m}` of a value that is not an object")
        o = self.s.objects[base]
        t = self.s.kinds.get(o.type) or {}
        if m == "id":
            return o.id
        if m == "state":
            return ("state", o.type, o.state)
        if m == "created_at":
            return o.created_at
        if m == "open":
            st = (t.get("states") or {}).get(o.state, {})
            return st.get("category") != "closed" and not st.get("final")
        attrs = t.get("attributes") or {}
        if m in attrs:
            spec = attrs[m] or {}
            if m in o.attrs:
                return o.attrs[m]
            if "reference" in spec and "opposite" in spec:
                return self.opposite_end(o, spec)
            return ABSENT
        if m in (t.get("derived_attributes") or {}):
            return Evaluator(self.s, o.type, o.id, stubs=self.stubs).value(
                flowexpr.parse(t["derived_attributes"][m]["expression"]), {})
        if m in (t.get("observations") or {}):
            kind = t["observations"][m]["kind"]
            corrected = {x.attrs.get("corrects") for x in self.s.objects.values() if x.type == kind}
            return tuple(x.id for x in sorted(self.s.objects.values(), key=lambda x: x.id_order)
                         if x.type == kind and x.attrs.get("subject") == o.id and x.id not in corrected)
        if m in ("recorded_at", "occurred_at", "subject", "corrects") and m in o.attrs:
            return o.attrs[m]
        raise NotRunnable(f"the member `{o.type}.{m}`")

    def opposite_end(self, o, spec):
        """An end the other side stores: the objects of the target type whose opposite end names this one."""
        target, many = spec["reference"].rstrip("[]"), spec["reference"].endswith("[]")
        found = tuple(x.id for x in sorted(self.s.objects.values(), key=lambda x: x.id_order)
                      if self.s.is_a(x.type, target) and x.attrs.get(spec["opposite"]) == o.id)
        return found if many else (found[0] if found else ABSENT)

    # ── expressions ─────────────────────────────────────────────────────────
    def value(self, e, env):
        k = e[0]
        if k == "num":
            return e[1] if isinstance(e[1], int) else Fraction(e[1])
        if k == "str":
            return e[1]
        if k == "bool":
            return e[1]
        if k == "dur":
            return Fraction(str(e[1])) * SECONDS[e[2]]
        if k == "money":
            return Fraction(e[2])
        if k == "name":
            return self.name(e[1], env)
        if k == "path":
            return self.member(self.value(e[1], env), e[2])
        if k == "set":
            return tuple(self.value(i, env) for i in e[1])
        if k == "not":
            v = truth(self.value(e[1], env))
            return ABSENT if v is ABSENT else not v
        if k == "neg":
            v = self.value(e[1], env)
            return ABSENT if v is ABSENT else -v
        if k == "isnull":
            v = self.value(e[1], env)
            return (v is ABSENT) != e[2]
        if k == "in":
            left, right = self.value(e[1], env), e[2]
            if left is ABSENT:
                return ABSENT
            if right[0] == "name" and right[1] in self.s.enums and right[1] not in env:
                return isinstance(left, tuple) and left[0] == "enum" and left[1] == right[1]
            coll = self.value(right, env)
            if coll is ABSENT:
                return ABSENT
            return left in coll
        if k == "bin":
            return self.binary(e, env)
        if k == "if":
            c = truth(self.value(e[1], env))
            if c is ABSENT:
                return ABSENT
            return self.value(e[2] if c else e[3], env)
        if k == "agg":
            return self.aggregate(e, env)
        if k == "call":
            return self.call(e, env)
        if k in ("metric", "changed"):
            raise NotRunnable("`metric()`" if k == "metric" else "`changed_since`")
        raise NotRunnable(f"the expression form {k}")

    def binary(self, e, env):
        op = e[1]
        if op in ("and", "or", "implies"):
            a = truth(self.value(e[2], env))
            if op == "implies":
                a = ABSENT if a is ABSENT else not a
            short = op == "and" and a is False or op != "and" and a is True
            if short:
                return a if op != "implies" else True
            b = truth(self.value(e[3], env))
            if op == "and":
                return False if b is False else (ABSENT if ABSENT in (a, b) else True)
            return True if b is True else (ABSENT if ABSENT in (a, b) else False)
        a, b = self.value(e[2], env), self.value(e[3], env)
        if a is ABSENT or b is ABSENT:
            return ABSENT
        if op == "==":
            return a == b
        if op == "!=":
            return a != b
        if op in ("<", "<=", ">", ">="):
            return {"<": a < b, "<=": a <= b, ">": a > b, ">=": a >= b}[op]
        if op == "+":
            return a + b
        if op == "-":
            return a - b
        if op == "*":
            return a * b
        if op == "/":
            if b == 0:
                return ABSENT
            if isinstance(a, int) and isinstance(b, int):
                return int(Fraction(a, b))          # int / int discards the remainder (§8.3)
            return Fraction(a) / Fraction(b)
        raise NotRunnable(f"the operator {op}")

    def aggregate(self, e, env):
        _, kind, distinct, var, coll, where, body = e
        if var is None:
            raise NotRunnable(f"a metric's own `{kind}`")
        items = self.value(coll, env)
        if items is ABSENT:
            return ABSENT
        if not isinstance(items, tuple):
            raise NotRunnable("an aggregate over a value that is not a collection")
        selected, undecided = [], False
        for it in items:
            inner = dict(env, **{var: it})
            f = True if where is None else truth(self.value(where, inner))
            if f is ABSENT:
                if self.filter_mode:
                    continue
                undecided = True
            elif f:
                selected.append(inner)
        if kind in ("any", "none", "all"):
            if kind == "all":
                results = [truth(self.value(body, i)) if body is not None else True for i in selected]
            else:
                results = [True for _ in selected]
            if kind == "any":
                return True if selected else (ABSENT if undecided else False)
            if kind == "none":
                return False if selected else (ABSENT if undecided else True)
            if False in results:
                return False
            return ABSENT if undecided or ABSENT in results else True
        if undecided:
            return ABSENT
        values = [self.value(body, i) for i in selected] if body is not None else [None] * len(selected)
        if kind == "count":
            return len(set(values)) if distinct else len(values)
        if any(v is ABSENT for v in values):
            return ABSENT
        if kind == "sum":
            return sum(values, 0)
        if kind in ("min", "max"):
            return (min if kind == "min" else max)(values) if values else ABSENT
        raise NotRunnable(f"`{kind}`")

    def call(self, e, env):
        fn, args = e[1], e[2]
        if fn[0] == "path" and fn[1][0] == "name" and f"{fn[1][1]}.{fn[2]}" in self.stubs:
            return self.stubs[f"{fn[1][1]}.{fn[2]}"] == "satisfied"
        if fn[0] == "path" and fn[1][0] == "name" and fn[1][1] not in env and fn[1][1] != "this" \
                and fn[1][1] not in (self.t.get("attributes") or {}):
            raise ExampleError(f"the evaluator function {fn[1][1]}.{fn[2]} is called, and the example stubs no answer for it")
        if fn == ("name", "entered_at") and len(args) == 1 and args[0][0] == "name":
            if self.this is None:
                return ABSENT
            return self.s.objects[self.this].entered.get(args[0][1], ABSENT)
        if fn == ("name", "length") and len(args) == 1:
            v = self.value(args[0], env)
            return ABSENT if v is ABSENT else len(v)
        raise NotRunnable(f"the function `{fn[1] if fn[0] == 'name' else fn[2]}`")


class Runner:
    """Requests against a store, as DESIGN.md §6 executes them, within this slice."""

    def __init__(self, kinds, enums):
        self.kinds, self.enums = kinds, enums

    def value_of(self, store, spec, v, aliases):
        """An example's written value as a value of the type its spec declares (flow-format.md §11.3)."""
        spec = spec or {}
        if v is None:
            return ABSENT
        if "reference" in spec:
            many = spec["reference"].endswith("[]")
            if many:
                return tuple(aliases[a] for a in v)
            return aliases[v]
        typ = spec.get("type", "")
        if typ in self.enums:
            e, m = v.split(".", 1)
            return ("enum", e, m)
        if typ == "timestamp":
            m = re.fullmatch(r"now(?: ([+-]) ([0-9]+) (\w+))?", v)
            if m:
                shift = Fraction(int(m.group(2))) * SECONDS[m.group(3)] if m.group(1) else 0
                return store.now + (shift if m.group(1) != "-" else -shift)
            return timestamp(v)
        if typ == "duration":
            n, unit = v.split()
            return Fraction(int(n)) * SECONDS[unit]
        if typ.startswith(("decimal", "money")):
            return Fraction(str(v))
        return v

    def attempt(self, store, tn, xn, oid, actor, raw_inputs, aliases, stubs):
        """The request's result, ("applied", object id, flags), or a Refused. The store changes only if it applies."""
        work = copy.deepcopy(store)
        # 1. the actor: an object of a type that marks an actor identity (DESIGN.md §6 step 1)
        if actor not in work.objects or work.actor_attr(work.type_of(actor)) is None:
            raise Refused("unsatisfied", "actor_known", "dependent")
        attr, akind = work.actor_attr(work.type_of(actor))
        who = {"id": work.objects[actor].attrs.get(attr, ABSENT), "kind": akind}
        t = self.kinds.get(tn) or {}
        x = (t.get("transitions") or {}).get(xn)
        if x is None or (x["kind"] == "initial") != (oid is None):
            raise Refused("unknown_transition", remedy="self_serviceable")
        if x["kind"] in ("assertion", "erasure") or x.get("corrects"):
            raise NotRunnable(f"a{'n' if x['kind'][0] in 'ae' else ''} {x['kind'] if not x.get('corrects') else 'correction'}")
        # 4. not requestable, unavailable, invalid input, in that order
        if x.get("only_via"):
            raise Refused("not_requestable", remedy="unreachable_from_here")
        o = work.objects.get(oid) if oid else None
        if o is not None:
            froms = x.get("from")
            froms = [froms] if isinstance(froms, str) else list(froms or [])
            if o.state not in froms:
                raise Refused("unavailable", remedy="unreachable_from_here", detail=f"it is in {o.state}")
        attrs = t.get("attributes") or {}
        declared = x.get("inputs") or {}
        taken = set(x.get("required_inputs", [])) | set(x.get("optional_inputs", [])) | set(declared)
        inputs = {}
        for k, v in (raw_inputs or {}).items():
            if k not in taken:
                raise Refused("invalid_input", remedy="self_serviceable", detail=f"no input {k}")
            spec = declared.get(k) if k in declared else attrs.get(k)
            value = self.value_of(work, spec, v, aliases)
            if value is not ABSENT and "reference" in (spec or {}):
                want = spec["reference"].rstrip("[]")
                for ref in (value if isinstance(value, tuple) else (value,)):
                    if ref not in work.objects or not work.is_a(work.type_of(ref), want):
                        raise Refused("invalid_input", remedy="self_serviceable", detail=f"{k} names no {want}")
            if value is not ABSENT:
                inputs[k] = value
        for k in x.get("required_inputs", []):
            spec = attrs.get(k) or {}
            if k not in inputs and not spec.get("optional") and "default" not in spec:
                raise Refused("invalid_input", remedy="self_serviceable", detail=f"{k} left out")
        for k, spec in declared.items():
            if k not in inputs and "default" in (spec or {}):
                inputs[k] = Evaluator(work, tn, oid, inputs, who, stubs).value(flowexpr.parse(spec["default"]), {})
            elif k not in inputs and not (spec or {}).get("optional"):
                raise Refused("invalid_input", remedy="self_serviceable", detail=f"{k} left out")
        # the generated guards: for a recording, the subject is open and its corrected datapoint current
        if xn == "record" and "subject" in inputs:
            subject = work.objects[inputs["subject"]]
            if "corrects" in inputs:
                raise NotRunnable("a correction of a recorded datapoint, whose guard corrects_current this slice does not evaluate")
            if (self.kinds.get(subject.type) or {}).get("mirror"):
                raise NotRunnable("a recording on a mirror's object, whose guard subject_owned this slice does not evaluate")
            if work.final(subject):
                raise Refused("unsatisfied", "subject_open", "unreachable_from_here")
        for k in inputs:
            spec = attrs.get(k) or {}
            if k in taken - set(declared) and "reference" in spec and self.owner_end(spec):
                raise NotRunnable(f"writing {tn}.{k}, a part's owner, whose whole this slice does not re-check")
        for k in x.get("required_inputs", []):
            spec = attrs.get(k) or {}
            if k not in inputs and (spec.get("optional") or "default" in spec):
                raise Refused("unsatisfied", f"{k}_provided", "self_serviceable")
        # the transition's own guards, in their order
        flags = []
        conditions = t.get("conditions") or {}
        for g, mode in (x.get("guards") or {}).items():
            ok = truth(Evaluator(work, tn, oid, inputs, who, stubs).value(flowexpr.parse(conditions[g]["expression"]), {}))
            if ok is not True:
                if mode == "deny":
                    raise Refused("unsatisfied", g, conditions[g].get("remedy"))
                flags.append((g, mode))
        # 5. the outcome: the new state and the writes
        if o is None:
            if any("identifier" in (spec or {}) for spec in attrs.values()):
                raise NotRunnable(f"creating a {tn}, whose identifier a sequence mints")
            o = Obj(work.new_id(tn), tn, x["to"], work.now)
            o.id_order = work.counter
            work.objects[o.id] = o
            for a, spec in attrs.items():
                if (spec or {}).get("type") == "counter":
                    o.attrs[a] = 0
                elif "default" in (spec or {}):
                    o.attrs[a] = Evaluator(work, tn, o.id, inputs, who, stubs).value(flowexpr.parse(str(spec["default"])), {})
            if xn == "record" and "subject" in inputs:
                o.attrs["recorded_at"] = work.now
                o.attrs["occurred_at"] = work.now
        elif x["kind"] == "external":
            o.state = x["to"]
            o.entered[o.state] = work.now
        for k in x.get("required_inputs", []) + x.get("optional_inputs", []):
            if k in inputs:
                o.attrs[k] = inputs[k]
        for st in x.get("effect") or []:
            if "assign" in st:
                v = Evaluator(work, tn, o.id, inputs, who, stubs).value(flowexpr.parse(st["assign"]["expr"]), {})
                if v is ABSENT:
                    o.attrs.pop(st["assign"]["location"], None)
                else:
                    o.attrs[st["assign"]["location"]] = v
            elif "clear" in st:
                for a in st["clear"]:
                    o.attrs.pop(a, None)
            else:
                raise NotRunnable(f"the effect step `{next(iter(st))}`")
        self.cascades(work, t, xn, o)
        o.version += 1
        # 7. the invariants of the written object
        failing = self.failing_invariants(work, o)
        if failing:
            raise Refused("invariant_violated", failing[0], detail=", ".join(failing[1:]) and "and " + ", ".join(failing[1:]))
        store.__dict__.update(work.__dict__)
        return ("applied", o.id, flags)

    def cascades(self, store, t, xn, o):
        for an, spec in list((t.get("attributes") or {}).items()) + list((t.get("inherited_parts") or {}).items()):
            clauses = (spec or {}).get("cascade") or []
            if any(xn in c.get("on", []) for c in clauses):
                parts = Evaluator(store, o.type, o.id).member(o.id, an) if an in (t.get("attributes") or {}) else ()
                if parts and parts is not ABSENT:
                    raise NotRunnable("a cascade")

    def owner_end(self, spec):
        """Whether a reference is a part's end back to its whole: its opposite is a composite end."""
        target = self.kinds.get(spec["reference"].rstrip("[]")) or {}
        back = ((target.get("attributes") or {}).get(spec.get("opposite") or "") or {})
        return back.get("aggregation") == "composite"

    def failing_invariants(self, store, o):
        t = self.kinds.get(o.type) or {}
        out = []
        for a, spec in (t.get("attributes") or {}).items():
            spec = spec or {}
            unique = spec.get("unique")
            if unique is None and "reference" in spec and "opposite" in spec and not spec["reference"].endswith("[]"):
                back = ((self.kinds.get(spec["reference"]) or {}).get("attributes") or {}).get(spec["opposite"]) or {}
                unique = True if back.get("reference") and not back["reference"].endswith("[]") else None
            if unique is None:
                continue
            if unique is not True:
                raise NotRunnable(f"the uniqueness of {o.type}.{a} in a scope or under a condition")
            value = o.attrs.get(a, ABSENT)
            if value is not ABSENT and any(x.id != o.id and x.attrs.get(a) == value and (store.is_a(x.type, o.type) or store.is_a(o.type, x.type))
                                           for x in store.objects.values()):
                out.append(f"{a}_unique")
        for n, inv in (t.get("invariants") or {}).items():
            if truth(Evaluator(store, o.type, o.id).value(flowexpr.parse(inv["expression"]), {})) is False:
                out.append(n)
        st = (t.get("states") or {}).get(o.state) or {}
        if any(a not in o.attrs for a in st.get("required_attributes") or []):
            out.append(f"{o.state}_invariant")
        return out


def observation_kinds(modules):
    """Each observation kind as the type it expands to: a record creation taking its fields and the subject."""
    kinds = {}
    for m in modules.values():
        for tn, t in (m.get("types") or {}).items():
            for ob in (t.get("observations") or {}).values():
                fields = ob.get("attributes") or {}
                kinds[ob["kind"]] = {
                    "attributes": dict(fields, subject={"reference": tn}, corrects={"reference": ob["kind"], "optional": True},
                                       recorded_at={"type": "timestamp"}, occurred_at={"type": "timestamp"}),
                    "states": {"RECORDED": {"category": "closed", "final": True}},
                    "invariants": ob.get("invariants") or {},
                    "transitions": {"record": {"kind": "initial", "to": "RECORDED",
                                               "required_inputs": ["subject"] + [a for a, f in fields.items() if not f.get("optional")],
                                               "optional_inputs": ["corrects"] + [a for a, f in fields.items() if f.get("optional")]}}}
    return kinds


def run_examples(doc, library, modules):
    """Each example's outcome: (name, "passed" | "failed" | "not run", message)."""
    enums = {n: list(v) for m in modules.values() for n, v in (m.get("enumerations") or {}).items()}
    kinds = dict(library)
    kinds.update(observation_kinds(modules))
    kinds["Operator"] = {"attributes": {"identity": {"type": "identity", "actor_kind": "human"}},
                         "states": {"ACTIVE": {"category": "live"}, "RETIRED": {"category": "closed", "final": True}}}
    runner = Runner(kinds, enums)
    setups = doc.get("setups") or {}
    results = []

    def chain(name):
        s = setups[name]
        return (chain(s["given"]) if "given" in s else []) + [("setup " + name, s)]

    for name, ex in (doc.get("examples") or {}).items():
        store = Store(kinds, enums)
        aliases = {OPERATOR: OPERATOR}
        stubs = {}
        try:
            parts = (chain(ex["given"]) if "given" in ex else []) + [("example " + name, ex)]
            for label, part in parts:
                stubs.update(part.get("evaluators") or {})
                for i, st in enumerate(part.get("steps") or []):
                    try:
                        step(runner, store, st, aliases, stubs)
                    except Refused as r:
                        raise ExampleError(f"{label}'s step {i + 1} does not apply: it is refused as {r}")
            outcome = step(runner, store, {"request": ex["request"]}, aliases, stubs, under_test=True)
            problem = compare(runner, store, ex["expect"], outcome, aliases)
            results.append((name, "failed" if problem else "passed", problem or ""))
        except NotRunnable as e:
            results.append((name, "not run", f"it reaches {e}, which the runner does not yet execute"))
        except ExampleError as e:
            results.append((name, "failed", str(e)))
    return results


def step(runner, store, st, aliases, stubs, under_test=False):
    """One step of a setup or the request under test; a setup's refusal propagates, the tested one is returned."""
    if "import" in st:
        im = st["import"]
        if im.get("after"):
            n, unit = im["after"].split()
            store.now += Fraction(int(n)) * SECONDS[unit]
        o = Obj(store.new_id(im["type"]), im["type"], im["state"], store.now)
        o.id_order = store.counter
        attrs = (runner.kinds.get(im["type"]) or {}).get("attributes") or {}
        for k, v in (im.get("values") or {}).items():
            o.attrs[k] = runner.value_of(store, attrs.get(k), v, aliases)
        store.objects[o.id] = o
        aliases[im["as"]] = o.id
        return None
    r = st["request"]
    if r.get("after"):
        n, unit = r["after"].split()
        store.now += Fraction(int(n)) * SECONDS[unit]
    actor = aliases.get(r.get("actor", OPERATOR), r.get("actor"))
    if "type" in r:
        tn, oid = r["type"], None
    else:
        oid = aliases[r["object"]]
        tn = store.type_of(oid)
    try:
        result = runner.attempt(store, tn, r["transition"], oid, actor, r.get("inputs"), aliases, stubs)
    except Refused as refusal:
        if under_test:
            return ("refused", refusal)
        raise
    if "as" in r:
        aliases[r["as"]] = result[1]
    return result


def compare(runner, store, want, outcome, aliases):
    """What differs between an expectation and what the request did, or None."""
    if outcome[0] == "refused":
        got = outcome[1]
        if want["verdict"] == "applied":
            return f"expected the request to apply, and it is refused as {got}"
        if got.verdict != want["verdict"]:
            return f"expected {want['verdict']}, and it is refused as {got}"
        for k in ("clause", "remedy"):
            if k in want and getattr(got, k) != want[k]:
                return f"expected the {k} {want[k]}, and it is refused as {got}"
        return None
    if want["verdict"] != "applied":
        return f"expected {want['verdict']}, and the request applies"
    o = store.objects[outcome[1]]
    if "state" in want and o.state != want["state"]:
        return f"expected the object in {want['state']}, and it is in {o.state}"
    attrs = (runner.kinds.get(o.type) or {}).get("attributes") or {}
    for k, v in (want.get("values") or {}).items():
        expected = runner.value_of(store, attrs.get(k), v, aliases)
        if o.attrs.get(k, ABSENT) != expected:
            named = {oid: alias for alias, oid in aliases.items()}
            got = o.attrs.get(k, ABSENT)
            return f"expected {k} to hold {v}, and it holds {named.get(got, got) if isinstance(got, str) else got}"
    if want.get("cascaded"):
        return "expected cascades, and this slice of the runner takes none"
    return None

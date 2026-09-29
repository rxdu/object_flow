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


def names_read(n):
    """The bare names an expression reads at the start of a path, a loop's own binder excepted."""
    if not isinstance(n, tuple) or not n:
        return set()
    if n[0] == "name":
        return {n[1]}
    if n[0] == "path":
        return names_read(n[1])
    if n[0] == "agg":
        inner = (names_read(n[5]) if n[5] else set()) | (names_read(n[6]) if n[6] else set())
        return (names_read(n[4]) if n[4] else set()) | (inner - {n[3]})
    parts = [p for p in n[1:] if isinstance(p, tuple)] + [q for p in n[1:] if isinstance(p, list) for q in p if isinstance(q, tuple)]
    return set().union(*(names_read(p) for p in parts)) if parts else set()


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
        self.admissions = set()                 # invariants whose violation an admission lets stand (ADR-0054)
        self.erased = False                     # its own erasure is recorded (DESIGN.md §8)


class Store:
    """The objects of one example, built from an empty store holding version 0 and its operator."""

    def __init__(self, kinds, enums):
        self.kinds, self.enums = kinds, enums
        self.now = timestamp(CLOCK_START)
        self.objects, self.counter = {}, 0
        self.sequences = {}                     # (sequence, scope value) -> the last number minted
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
        self.unanswered = False                 # an evaluator it asked did not answer (ADR-0138)
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
        if n == "referrers":
            return self.referrers()
        if n == "this_event":
            raise NotRunnable("`this_event`")
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
        if m == "type":
            return ("typeref", o.type)
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

    def referrers(self):
        """Every object holding a reference to this one, its parts excepted, since they cascade (§4.13)."""
        if self.this is None:
            return ()
        out = []
        for x in sorted(self.s.objects.values(), key=lambda x: x.id_order):
            if x.id == self.this:
                continue
            for a, spec in ((self.s.kinds.get(x.type) or {}).get("attributes") or {}).items():
                spec = spec or {}
                if "reference" not in spec or a not in x.attrs:
                    continue
                target = self.s.kinds.get(spec["reference"].rstrip("[]")) or {}
                owner = ((target.get("attributes") or {}).get(spec.get("opposite") or "") or {}).get("aggregation") == "composite"
                value = x.attrs[a]
                if not owner and (value == self.this or (isinstance(value, tuple) and self.this in value)):
                    out.append(x.id)
                    break
        return tuple(out)

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
            answer = self.stubs[f"{fn[1][1]}.{fn[2]}"]
            if answer == "unanswered":
                self.unanswered = True
                return ABSENT
            return answer == "satisfied"
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


# the generated guards' remedy classes, as DESIGN.md §5.5 states them (ADR-0138)
GENERATED_REMEDY = {"actor_known": "dependent", "occurred_within": "self_serviceable", "whole_open": "self_serviceable",
                    "not_erased": "unreachable_from_here", "subject_open": "unreachable_from_here",
                    "corrects_current": "self_serviceable", "subject_owned": "unreachable_from_here"}


class Ctx:
    """One request's working store, and what it has taken and written so far."""

    def __init__(self, work, who, stubs, root_type):
        self.work, self.who, self.stubs, self.root_type = work, who, stubs, root_type
        self.written = []           # object ids, in the order first written
        self.taken = []             # "Type.transition" of every transition taken, the requested one first
        self.flags = []             # the audit and warn guards that failed, at any depth
        self.asserted, self.admits = None, set()    # the object an assertion set, and what it admits
        self.erased = {}            # object id -> the personal attributes this request erased on it

    def wrote(self, oid):
        if oid not in self.written:
            self.written.append(oid)


def qualified(clause, tn):
    return clause if clause is None or "." in clause else f"{tn}.{clause}"


class Runner:
    """Requests against a store, as DESIGN.md §6 executes them, within the slices built so far."""

    def __init__(self, kinds, enums):
        self.kinds, self.enums = kinds, enums

    def value_of(self, store, spec, v, aliases):
        """An example's written value as a value of the type its spec declares (flow-format.md §11.3)."""
        spec = spec or {}
        if v is None:
            return ABSENT
        if "reference" in spec:
            if spec["reference"].endswith("[]"):
                return tuple(aliases[a] for a in v)
            return aliases[v]
        typ = spec.get("type", "")
        if typ.endswith("[]"):
            return tuple(self.value_of(store, dict(spec, type=typ[:-2]), x, aliases) for x in v)
        if typ in self.enums:
            e, m = v.split(".", 1)
            return ("enum", e, m)
        if typ.startswith("money") and isinstance(v, str):
            return Fraction(v.split()[-1])      # written as a money literal, SGD 12000.00 (§5)
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

    def owner_end(self, spec):
        """Whether a reference is a part's end back to its whole: its opposite is a composite end."""
        if not spec or "reference" not in spec:
            return False
        target = self.kinds.get(spec["reference"].rstrip("[]")) or {}
        back = ((target.get("attributes") or {}).get(spec.get("opposite") or "") or {})
        return back.get("aggregation") == "composite"

    # ── a request ─────────────────────────────────────────────────────────────
    def attempt(self, store, tn, xn, oid, actor, raw_inputs, aliases, stubs):
        """The request's result, ("applied", object id, flags, the transitions it caused), or a Refused.
        The store changes only if the request applies."""
        work = copy.deepcopy(store)
        # 1. the actor: an object of a type that marks an actor identity (DESIGN.md §6 step 1)
        if actor not in work.objects or work.actor_attr(work.type_of(actor)) is None:
            raise Refused("unsatisfied", "actor_known", "dependent")
        attr, akind = work.actor_attr(work.type_of(actor))
        ctx = Ctx(work, {"id": work.objects[actor].attrs.get(attr, ABSENT), "kind": akind}, stubs, tn)
        t = self.kinds.get(tn) or {}
        x = (t.get("transitions") or {}).get(xn)
        # 3. not found, and unknown transition; 4. not requestable, unavailable, invalid input, in that order
        if oid is not None and oid not in work.objects:
            raise Refused("not_found", remedy="unreachable_from_here")
        if x is None or (x["kind"] == "initial") != (oid is None) or (oid is not None and work.type_of(oid) != tn):
            raise Refused("unknown_transition", remedy="self_serviceable")
        if x.get("only_via"):
            raise Refused("not_requestable", remedy="unreachable_from_here")
        self.check_state(work, x, oid)
        inputs = {}
        attrs = t.get("attributes") or {}
        declared = x.get("inputs") or {}
        taken = set(x.get("required_inputs", [])) | set(x.get("optional_inputs", [])) | set(declared)
        for k, v in (raw_inputs or {}).items():
            if x["kind"] == "assertion" and k in ("to", "admits"):
                inputs[k] = tuple(v) if k == "admits" else v      # a state, and invariants, by name (§4.13)
                continue
            if k not in taken:
                raise Refused("invalid_input", remedy="self_serviceable", detail=f"no input {k}")
            value = self.value_of(work, declared.get(k) if k in declared else attrs.get(k), v, aliases)
            if value is not ABSENT:
                inputs[k] = value
        try:
            root = self.take(ctx, tn, xn, oid, inputs, depth=0)
            # 7. every invariant the writes could violate, in ascending id order of the object it fails on, except
            # a violation an admission lets stand, which is discharged once the invariant holds again (ADR-0054)
            for obj in sorted(work.objects.values(), key=lambda o: o.id_order):
                failing = self.failing_invariants(work, obj)
                obj.admissions &= set(failing)
                refused = []
                for n in failing:
                    if n in obj.admissions:
                        continue
                    if (obj.id == ctx.asserted and n in ctx.admits) or f"{obj.type}.{n}" in ctx.admits:
                        obj.admissions.add(n)           # an assertion's admission (§4.13)
                    elif obj.id in ctx.erased and self.reads(obj.type, n) & ctx.erased[obj.id]:
                        obj.admissions.add(n)           # an erasure admits what reads what it erased (§4.13)
                    else:
                        refused.append(n if obj.type == tn else f"{obj.type}.{n}")
                if refused:
                    raise Refused("invariant_violated", refused[0], detail=", ".join(refused[1:]) and "and " + ", ".join(refused[1:]))
        except Refused:
            store.sequences = work.sequences    # a number minted by a refused request stays used: the gap
            raise
        store.__dict__.update(work.__dict__)
        return ("applied", root, ctx.flags, ctx.taken[1:])

    def mint(self, ctx, o, attrs):
        """Each identifier of a new object, numbered from 1 per sequence and scope, as its format writes it
        (flow-format.md §4.12)."""
        work = ctx.work
        for a, spec in attrs.items():
            ident = (spec or {}).get("identifier")
            if not ident:
                continue
            scope = o.attrs.get(ident["scope"], ABSENT) if ident.get("scope") else None
            if scope is ABSENT:
                raise NotRunnable(f"minting {o.type}.{a} with its scope absent")
            key = (ident["sequence"], scope)
            n = work.sequences.get(key, 0) + 1
            work.sequences[key] = n

            def field(m):
                name, _, width = m.group(1).partition(":")
                if name == "n":
                    return str(n).zfill(int(width or 0))
                v = Evaluator(work, o.type, o.id).value(flowexpr.parse(name), {})
                if v is ABSENT:
                    raise NotRunnable(f"a format field {name} that is absent")
                return str(v)
            o.attrs[a] = re.sub(r"\{([^}]+)\}", field, ident.get("format", "{n}"))

    def check_state(self, work, x, oid):
        if oid is None or x["kind"] == "erasure":       # an erasure runs at any state, a final one included
            return
        if x["kind"] == "assertion":                    # an assertion runs from any state but a final one
            if work.final(work.objects[oid]):
                raise Refused("unavailable", remedy="unreachable_from_here", detail=f"it is in {work.objects[oid].state}")
            return
        froms = x.get("from")
        froms = [froms] if isinstance(froms, str) else list(froms or [])
        if work.objects[oid].state not in froms:
            raise Refused("unavailable", remedy="unreachable_from_here", detail=f"it is in {work.objects[oid].state}")

    def take(self, ctx, tn, xn, oid, inputs, depth):  # noqa: C901  one transition, as DESIGN.md §6 orders it
        """One transition, the requested one or one it causes: its checks, guards, outcome, effect and cascades.
        Returns the object's id."""
        work = ctx.work
        t = self.kinds.get(tn) or {}
        x = (t.get("transitions") or {}).get(xn)
        if x is None or (x["kind"] == "initial") != (oid is None):
            raise Refused("unknown_transition", remedy="self_serviceable")
        attrs = t.get("attributes") or {}
        declared = x.get("inputs") or {}
        taken = set(x.get("required_inputs", [])) | set(x.get("optional_inputs", [])) | set(declared)
        if depth:
            self.check_state(work, x, oid)
            for k in inputs:
                if k not in taken:
                    raise Refused("invalid_input", remedy="self_serviceable", detail=f"no input {k}")
        for k, v in inputs.items():
            spec = declared.get(k) if k in declared else attrs.get(k)
            if "reference" in (spec or {}):
                want = spec["reference"].rstrip("[]")
                for ref in (v if isinstance(v, tuple) else (v,)):
                    if ref not in work.objects or not work.is_a(work.type_of(ref), want):
                        raise Refused("invalid_input", remedy="self_serviceable", detail=f"{k} names no {want}")
        for k in x.get("required_inputs", []):
            spec = attrs.get(k) or {}
            if k not in inputs and not spec.get("optional") and "default" not in spec:
                raise Refused("invalid_input", remedy="self_serviceable", detail=f"{k} left out")
        for k, spec in declared.items():
            if k not in inputs and "default" in (spec or {}):
                inputs[k] = Evaluator(work, tn, oid, inputs, ctx.who, ctx.stubs).value(flowexpr.parse(spec["default"]), {})
            elif k not in inputs and not (spec or {}).get("optional"):
                raise Refused("invalid_input", remedy="self_serviceable", detail=f"{k} left out")
        if x["kind"] == "assertion":
            o = work.objects[oid]
            if inputs.get("to") not in (x.get("to") if isinstance(x.get("to"), list) else [x.get("to")]) or inputs.get("to") == o.state:
                raise Refused("invalid_input", remedy="self_serviceable", detail=f"{inputs.get('to')} is not a state it may put the object in")
            if not set(inputs.get("admits", ())) <= set(x.get("may_admit") or []):
                raise Refused("invalid_input", remedy="self_serviceable", detail="an admission it does not list in may_admit")
            ctx.asserted, ctx.admits = oid, set(inputs.get("admits", ()))
        # the generated guards, then the <attribute>_provided ones, then the transition's own (flow-format.md §4.8)
        personal = {a for a, sp in attrs.items() if (sp or {}).get("personal")}
        if oid is not None and x["kind"] != "erasure" and work.objects[oid].erased:
            writes = set(inputs) & set(x.get("required_inputs", []) + x.get("optional_inputs", []))
            writes |= {st["assign"]["location"] for st in x.get("effect") or [] if "assign" in st}
            if writes & personal:
                raise Refused("unsatisfied", "not_erased", GENERATED_REMEDY["not_erased"])
        if xn == "record" and "subject" in inputs:
            subject = work.objects[inputs["subject"]]
            if (self.kinds.get(subject.type) or {}).get("mirror"):
                raise Refused("unsatisfied", "subject_owned", GENERATED_REMEDY["subject_owned"])
            if "corrects" in inputs:
                corrected = work.objects[inputs["corrects"]]
                if corrected.attrs.get("subject") != subject.id:
                    raise NotRunnable("a correction naming a datapoint of another subject, whose refusal the design does not state")
                if any(other.attrs.get("corrects") == corrected.id for other in work.objects.values()):
                    raise Refused("unsatisfied", "corrects_current", GENERATED_REMEDY["corrects_current"])
            elif work.final(subject):       # a correction may be recorded after the subject is finished (DESIGN.md §5.11)
                raise Refused("unsatisfied", "subject_open", GENERATED_REMEDY["subject_open"])
        wholes = []
        for k in set(x.get("required_inputs", []) + x.get("optional_inputs", [])) & set(inputs):
            if self.owner_end(attrs.get(k)):
                if oid is not None and work.objects[oid].attrs.get(k) != inputs[k]:
                    if work.final(work.objects[inputs[k]]):
                        raise Refused("unsatisfied", "whole_open", GENERATED_REMEDY["whole_open"])
                    wholes.append(work.objects[oid].attrs.get(k))
                wholes.append(inputs[k])
        for k in x.get("required_inputs", []):
            spec = attrs.get(k) or {}
            if k not in inputs and (spec.get("optional") or "default" in spec):
                raise Refused("unsatisfied", f"{k}_provided", "self_serviceable")
        conditions = t.get("conditions") or {}
        for g, mode in (x.get("guards") or {}).items():
            asking = Evaluator(work, tn, oid, inputs, ctx.who, ctx.stubs)
            ok = truth(asking.value(flowexpr.parse(conditions[g]["expression"]), {}))
            if ok is not True:
                if mode == "deny":
                    # an evaluator that did not answer leaves the guard unknown, and the caller may try again (ADR-0138)
                    raise Refused("unsatisfied", g, "temporal" if asking.unanswered else conditions[g].get("remedy"))
                ctx.flags.append((f"{tn}.{g}", mode))
        # 5. the outcome: the new state and the attribute writes
        if oid is None:
            o = Obj(work.new_id(tn), tn, x["to"], work.now)
            o.id_order = work.counter
            work.objects[o.id] = o
            for a, spec in attrs.items():
                if (spec or {}).get("type") == "counter":
                    o.attrs[a] = 0
                elif "default" in (spec or {}):
                    o.attrs[a] = Evaluator(work, tn, o.id, inputs, ctx.who, ctx.stubs).value(flowexpr.parse(str(spec["default"])), {})
            if xn == "record" and "subject" in inputs:
                o.attrs["recorded_at"] = work.now
                o.attrs["occurred_at"] = work.now
        else:
            o = work.objects[oid]
            if x["kind"] == "external":
                o.state = x["to"]
                o.entered[o.state] = work.now
            elif x["kind"] == "assertion":
                o.state = inputs["to"]
                o.entered[o.state] = work.now
            elif x["kind"] == "erasure":
                self.erase(ctx, o)
        for k in x.get("required_inputs", []) + x.get("optional_inputs", []):
            if k in inputs:
                o.attrs[k] = inputs[k]
        if oid is None:
            self.mint(ctx, o, attrs)
        ctx.wrote(o.id)
        for w in wholes:
            if w is not None:
                ctx.wrote(w)
        ctx.taken.append(f"{tn}.{xn}")
        # 6. the effect, then the cascades to parts, depth-first (ADR-0038, ADR-0138)
        self.effects(ctx, tn, o, x.get("effect") or [], inputs, {}, depth)
        self.cascades(ctx, t, tn, xn, o, inputs, depth)
        if x["kind"] == "erasure":
            self.erase_chain(ctx, o, inputs, depth)
        o.version += 1
        return o.id

    def effects(self, ctx, tn, o, steps, inputs, env, depth):
        """An effect's steps in order; returns whether one reached another object by a call or a creation."""
        work, reached = ctx.work, False
        attrs = (self.kinds.get(tn) or {}).get("attributes") or {}

        def ev(tree, filtering=False):
            return Evaluator(work, tn, o.id, inputs, ctx.who, ctx.stubs, filter_mode=filtering).value(tree, env)

        def unsupplied(tree):
            # a write whose right-hand side is an unsupplied optional input is skipped (DESIGN.md §5.4)
            return tree[0] == "path" and tree[1] == ("name", "inputs") and tree[2] not in inputs

        def arguments(spec):
            out = {}
            for k, e in (spec.get("inputs") or {}).items():
                tree = flowexpr.parse(e)
                if unsupplied(tree):
                    continue
                v = ev(tree)
                if v is not ABSENT:
                    out[k] = v
            return out

        for st in steps:
            kind = next(iter(st))
            spec = st[kind]
            if kind == "assign":
                tree = flowexpr.parse(spec["expr"])
                if unsupplied(tree):
                    continue
                v = ev(tree)
                if v is ABSENT:
                    raise NotRunnable("an assignment of an absent value, which only an unsupplied optional input may be")
                if self.owner_end(attrs.get(spec["location"])) and o.attrs.get(spec["location"]) not in (None, v):
                    raise NotRunnable("re-parenting a part by an effect, whose whole_open guard this runner does not yet evaluate")
                o.attrs[spec["location"]] = v
                if self.owner_end(attrs.get(spec["location"])):
                    ctx.wrote(v)
            elif kind == "clear":
                for a in spec:
                    o.attrs.pop(a, None)
            elif kind in ("add", "remove"):
                tree = flowexpr.parse(spec["expr"])
                if unsupplied(tree):
                    continue
                v = ev(tree)
                if v is ABSENT:
                    raise NotRunnable(f"an `{kind}` of an absent value")
                current = tuple(o.attrs.get(spec["location"], ()))
                if kind == "add" and v not in current:
                    o.attrs[spec["location"]] = current + (v,)
                elif kind == "remove" and v in current:
                    o.attrs[spec["location"]] = tuple(c for c in current if c != v)
            elif kind == "create":
                values = arguments(spec)
                try:
                    new = self.take(ctx, spec["type"], spec["transition"], None, values, depth + 1)
                except Refused as r:
                    raise self.call_refused(r, f"create {spec['type']}.{spec['transition']}", spec["type"], None)
                if spec.get("result"):
                    env[spec["result"]] = new
                reached = True
            elif kind == "call":
                target = ev(flowexpr.parse(spec["target"]))
                if target is ABSENT:
                    continue            # a call through an absent optional end reaches nothing (DESIGN.md §5.4)
                if not isinstance(target, str) or target not in work.objects:
                    raise NotRunnable("a call whose target is not one object")
                values = arguments(spec)
                try:
                    self.take(ctx, work.type_of(target), spec["transition"], target, values, depth + 1)
                except Refused as r:
                    raise self.call_refused(r, f"call {spec['target']}.{spec['transition']}", work.type_of(target), target)
                reached = True
            elif kind == "foreach":
                if "range" in spec:
                    n = ev(flowexpr.parse(spec["range"]))
                    if n is ABSENT:
                        raise NotRunnable("a loop over an absent range")
                    items = list(range(1, n + 1))
                else:
                    coll = ev(flowexpr.parse(spec["array"]))
                    if coll is ABSENT or not isinstance(coll, tuple):
                        raise NotRunnable("a loop over what is not a collection")
                    items = sorted(coll, key=lambda i: work.objects[i].id_order if isinstance(i, str) and i in work.objects else 0)
                if spec.get("where"):
                    where = flowexpr.parse(spec["where"])
                    items = [i for i in items if truth(Evaluator(work, tn, o.id, inputs, ctx.who, ctx.stubs, filter_mode=True)
                                                       .value(where, dict(env, **{spec["item"]: i}))) is True]
                if len(items) > spec["limit"]:
                    raise Refused("over_limit", remedy="unreachable_from_here",
                                  detail=f"the loop over {spec.get('array', spec.get('range'))} selects {len(items)}, over its limit of {spec['limit']}")
                for item in items:
                    reached |= self.effects(ctx, tn, o, spec["steps"], inputs, dict(env, **{spec["item"]: item}), depth)
            elif kind == "supersede":
                successor = ev(flowexpr.parse(spec))
                if successor is ABSENT:
                    raise NotRunnable("a supersession naming no successor")
                o.attrs["_successor"] = successor
            else:
                raise NotRunnable(f"the effect step `{kind}`")
        return reached

    @staticmethod
    def call_refused(r, step, target_type, target):
        """A refused call or creation refuses the request, naming the step, the object it targeted and its own
        refusal (ADR-0122 decision 35); an expectation reads that refusal's clause and remedy."""
        inner_clause = r.clause if r.verdict == "call_refused" else qualified(r.clause, target_type)
        return Refused("call_refused", inner_clause, r.remedy, detail=f"{step} on {target or 'a new object'}: {r.verdict}")

    def cascades(self, ctx, t, tn, xn, o, inputs, depth):
        """The parts each composite end drives on this transition, in ascending id order, those already in a
        final state skipped (DESIGN.md §6 step 6)."""
        work = ctx.work
        ends = [(an, (spec or {}).get("cascade") or []) for an, spec in (t.get("attributes") or {}).items()
                if (spec or {}).get("aggregation") == "composite"]
        ends += [(an, (ip or {}).get("cascade") or []) for an, ip in (t.get("inherited_parts") or {}).items()]
        for an, clauses in ends:
            for clause in clauses:
                if xn not in clause.get("on", []):
                    continue
                parts = Evaluator(work, tn, o.id, inputs, ctx.who, ctx.stubs).member(o.id, an)
                parts = () if parts is ABSENT else (parts if isinstance(parts, tuple) else (parts,))
                driven = sorted((p for p in parts if not work.final(work.objects[p])), key=lambda p: work.objects[p].id_order)
                if not driven:
                    continue
                if len(driven) > clause.get("limit", len(driven)):
                    raise Refused("over_limit", remedy="unreachable_from_here",
                                  detail=f"the cascade on {an} drives {len(driven)}, over its limit of {clause['limit']}")
                for p in driven:
                    values = {}
                    for k, e in (clause.get("inputs") or {}).items():
                        v = Evaluator(work, tn, o.id, inputs, ctx.who, ctx.stubs).value(flowexpr.parse(e), {})
                        if v is not ABSENT:
                            values[k] = v
                    ptype = work.type_of(p)
                    try:
                        self.take(ctx, ptype, clause["transition"], p, values, depth + 1)
                    except Refused as r:
                        raise Refused(r.verdict, qualified(r.clause, ptype) if r.verdict != "call_refused" else r.clause,
                                      r.remedy, detail=f"cascading to {p}: {r.detail}")

    def erase(self, ctx, o):
        """Its personal attributes made absent, and its observations' personal fields, which the subject's erasure
        reaches through each one's generated `forget` without a declared step (DESIGN.md §8)."""
        work = ctx.work
        t = self.kinds.get(o.type) or {}
        erased = {a for a, sp in (t.get("attributes") or {}).items() if (sp or {}).get("personal") and a in o.attrs}
        for a in erased:
            del o.attrs[a]
        o.erased = True
        ctx.erased.setdefault(o.id, set()).update(erased)
        for ob in (t.get("observations") or {}).values():
            fields = {a for a, sp in (ob.get("attributes") or {}).items() if (sp or {}).get("personal")}
            for other in work.objects.values():
                if other.type == ob["kind"] and other.attrs.get("subject") == o.id:
                    gone = fields & set(other.attrs)
                    for a in gone:
                        del other.attrs[a]
                    if gone:
                        ctx.erased.setdefault(other.id, set()).update(gone)
                        ctx.wrote(other.id)

    def erase_chain(self, ctx, o, inputs, depth):
        """Every member of its supersession chain, predecessors and successors, each through its own type's erasure,
        in one request (ADR-0087)."""
        work = ctx.work
        chain, todo = set(), [o.id]
        while todo:
            cur = todo.pop()
            if cur in chain:
                continue
            chain.add(cur)
            succ = work.objects[cur].attrs.get("_successor")
            if succ:
                todo.append(succ)
            todo += [p.id for p in work.objects.values() if p.attrs.get("_successor") == cur]
        for member in sorted(chain - {o.id}, key=lambda i: work.objects[i].id_order):
            m = work.objects[member]
            if m.erased and member in ctx.erased:
                continue
            erasures = [n for n, xx in ((self.kinds.get(m.type) or {}).get("transitions") or {}).items() if xx["kind"] == "erasure"]
            if not erasures:
                raise NotRunnable(f"erasing {m.type}, a member of the chain, which declares no erasure")
            self.take(ctx, m.type, erasures[0], member, {k: v for k, v in inputs.items() if k == "reason"}, depth + 1)

    def reads(self, tn, n):
        """The attributes an invariant reads, generated ones included: an attribute's uniqueness, a state's requirement."""
        t = self.kinds.get(tn) or {}
        if n.endswith("_unique"):
            return {n[:-len("_unique")]}
        if n.endswith("_invariant"):
            state = {s.lower(): v for s, v in (t.get("states") or {}).items()}.get(n[:-len("_invariant")]) or {}
            return set(state.get("required_attributes") or [])
        inv = (t.get("invariants") or {}).get(n)
        return names_read(flowexpr.parse(inv["expression"])) if inv else set()

    def failing_invariants(self, store, o):
        """An object's failing invariants in the order its type declares them: an attribute's uniqueness, a
        state's required attributes, then the invariants (flow-format.md §3's order)."""
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
            # together with: the attribute and those listed; in scope: and the identifier's scope; where: among
            # the objects the condition holds for (flow-format.md §4.12). An absent part never collides.
            with_ = [a] + (unique.get("with", []) if isinstance(unique, dict) else [])
            if unique == "in_scope":
                with_.append(spec["identifier"]["scope"])
            where = flowexpr.parse(unique["where"]) if isinstance(unique, dict) and "where" in unique else None

            def key(obj):
                values = tuple(obj.attrs.get(w, ABSENT) for w in with_)
                if ABSENT in values:
                    return None
                if where is not None and truth(Evaluator(store, obj.type, obj.id, filter_mode=True).value(where, {})) is not True:
                    return None
                return values
            mine = key(o)
            if mine is not None and any(x.id != o.id and (store.is_a(x.type, o.type) or store.is_a(o.type, x.type)) and key(x) == mine
                                        for x in store.objects.values()):
                out.append(f"{a}_unique")
        st = (t.get("states") or {}).get(o.state) or {}
        if any(a not in o.attrs for a in st.get("required_attributes") or []):
            out.append(f"{o.state.lower()}_invariant")
        for n, inv in (t.get("invariants") or {}).items():
            if truth(Evaluator(store, o.type, o.id).value(flowexpr.parse(inv["expression"]), {})) is False:
                out.append(n)
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
        # the import writes through the built-in assertion, so what it brings in violating stands admitted (DESIGN.md §8)
        o.admissions = set(runner.failing_invariants(store, o))
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
    if "cascaded" in want and list(want["cascaded"]) != list(outcome[3]):
        return f"expected the request to cause {want['cascaded'] or 'nothing'}, and it causes {outcome[3] or 'nothing'}"
    return None


# ── the runner proven on a small order and its lines ───────────────────────────
def _order_kinds():
    line_ref = {"reference": "Line[]", "aggregation": "composite", "opposite": "order",
                "cascade": [{"on": ["cancel"], "transition": "void", "limit": 2}]}
    return {
        "Operator": {"attributes": {"identity": {"type": "identity", "actor_kind": "human"}}, "states": {"ACTIVE": {}}},
        "Order": {
            "attributes": {"lines": line_ref, "note": {"type": "string", "optional": True},
                           "tags": {"type": "string[]"}},
            "states": {"OPEN": {"category": "live"}, "DONE": {"category": "closed", "final": True},
                       "CANCELLED": {"category": "closed", "final": True}},
            "invariants": {"few_lines": {"expression": "count(l in lines) <= 3"}},
            "transitions": {
                "open": {"kind": "initial", "to": "OPEN"},
                "add_line": {"kind": "internal", "from": ["OPEN"], "inputs": {"qty": {"type": "int"}},
                             "effect": [{"create": {"type": "Line", "transition": "add", "inputs": {"order": "this", "qty": "inputs.qty"},
                                                    "result": "made"}},
                                        {"add": {"location": "tags", "expr": '"lined"'}}]},
                "ship": {"kind": "external", "from": ["OPEN"], "to": "DONE",
                         "effect": [{"foreach": {"item": "l", "array": "lines", "where": "l.qty > 0", "limit": 5,
                                                 "steps": [{"call": {"target": "l", "transition": "pack"}}]}}]},
                "cancel": {"kind": "external", "from": ["OPEN"], "to": "CANCELLED"},
                "tangle": {"kind": "internal", "from": ["OPEN"],
                           "effect": [{"create": {"type": "Line", "transition": "add", "inputs": {"order": "this", "qty": "1"}}}]},
            }},
        "Line": {
            "attributes": {"order": {"reference": "Order", "opposite": "lines"}, "qty": {"type": "int"}},
            "states": {"OPEN": {"category": "live"}, "PACKED": {"category": "closed"}, "VOID": {"category": "closed", "final": True}},
            "conditions": {"small": {"expression": "qty < 10", "remedy": "self_serviceable"}},
            "transitions": {
                "add": {"kind": "initial", "to": "OPEN", "only_via": ["Order.add_line", "Order.tangle"],
                        "required_inputs": ["order", "qty"]},
                "pack": {"kind": "external", "from": ["OPEN"], "to": "PACKED", "guards": {"small": "deny"}},
                "void": {"kind": "external", "from": ["OPEN", "PACKED"], "to": "VOID"},
            }},
    }


def self_test():
    """Each thing slice 2 executes, and each refusal it gives, shown on the order and its lines."""
    kinds = _order_kinds()
    ok = True

    def fresh(*qtys, cascade_too=False):
        k = copy.deepcopy(kinds)
        if cascade_too:
            k["Order"]["attributes"]["lines"]["cascade"].append({"on": ["tangle"], "transition": "void", "limit": 5})
        r, s = Runner(k, {}), Store(k, {})
        _a, order, *_ = r.attempt(s, "Order", "open", None, OPERATOR, {}, {}, {})
        lines = [r.attempt(s, "Order", "add_line", order, OPERATOR, {"qty": q}, {}, {}) for q in qtys]
        return r, s, order, lines

    def check(name, got, want):
        nonlocal ok
        if got != want:
            ok = False
            print(f"  {name}: {got!r}, not {want!r}")

    r, s, order, lines = fresh(1)
    check("a creation by an effect", lines[0][3], ["Line.add"])
    check("an add to a set", s.objects[order].attrs.get("tags"), ("lined",))
    r, s, order, _ = fresh(1, 0, 2)
    check("a loop's filter and its calls, in id order", r.attempt(s, "Order", "ship", order, OPERATOR, {}, {}, {})[3],
          ["Line.pack", "Line.pack"])
    r, s, order, _ = fresh(1, 20)
    try:
        r.attempt(s, "Order", "ship", order, OPERATOR, {}, {}, {})
        check("a call refused", "applied", "call_refused")
    except Refused as e:
        check("a call refused", (e.verdict, e.clause, e.remedy), ("call_refused", "Line.small", "self_serviceable"))
    check("a refused request changes nothing", s.objects[order].state, "OPEN")
    r, s, order, _ = fresh(1, 1, 1)
    try:
        r.attempt(s, "Order", "cancel", order, OPERATOR, {}, {}, {})
        check("a cascade over its limit", "applied", "over_limit")
    except Refused as e:
        check("a cascade over its limit", e.verdict, "over_limit")
    r, s, order, _ = fresh(1, 1)
    first = min((o for o in s.objects.values() if o.type == "Line"), key=lambda o: o.id_order).id
    r.attempt(s, "Line", "void", first, OPERATOR, {}, {}, {})
    try:
        r.attempt(s, "Line", "void", order, OPERATOR, {}, {}, {})
        check("a transition of another type", "applied", "unknown_transition")
    except Refused as e:
        check("a transition of another type", e.verdict, "unknown_transition")
    check("a cascade skips a part already final", r.attempt(s, "Order", "cancel", order, OPERATOR, {}, {}, {})[3], ["Line.void"])
    r, s, order, _ = fresh(1, 1, 1)
    try:
        r.attempt(s, "Order", "add_line", order, OPERATOR, {"qty": 1}, {}, {})
        check("a whole re-checked when a part is added", "applied", "invariant_violated")
    except Refused as e:
        check("a whole re-checked when a part is added", (e.verdict, e.clause), ("invariant_violated", "few_lines"))
    r, s, order, _ = fresh(1, cascade_too=True)
    check("an effect's creation before the cascade to parts", r.attempt(s, "Order", "tangle", order, OPERATOR, {}, {}, {})[3],
          ["Line.add", "Line.void", "Line.void"])
    k = copy.deepcopy(kinds)
    k["Order"]["conditions"] = {"paid": {"expression": "xero.invoice_paid(note)", "remedy": "dependent"}}
    k["Order"]["transitions"]["settle"] = {"kind": "internal", "from": ["OPEN"], "guards": {"paid": "deny"}}
    for answer, want in (("satisfied", "applied"), ("unsatisfied", ("unsatisfied", "dependent")),
                         ("unanswered", ("unsatisfied", "temporal"))):
        r, s = Runner(k, {}), Store(k, {})
        order = r.attempt(s, "Order", "open", None, OPERATOR, {}, {}, {})[1]
        try:
            got = r.attempt(s, "Order", "settle", order, OPERATOR, {}, {}, {"xero.invoice_paid": answer})[0]
        except Refused as e:
            got = (e.verdict, e.remedy)
        check(f"an evaluator that answers {answer}", got, want)
    print(f"runner: a small order and its lines, each thing slices 1 and 2 execute and each refusal they give: {'yes' if ok else 'NO'}")
    return ok

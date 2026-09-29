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
import functools
import math
import re
from fractions import Fraction

import flowexpr

class _Absent:
    """Absence, and unknown, one value (§8.2). A request works on a copy of the store, so the one value must stay
    itself when copied, or `is ABSENT` would fail on every copied row."""

    def __repr__(self):
        return "absent"

    def __copy__(self):
        return self

    def __deepcopy__(self, memo):
        return self

    def __reduce__(self):
        return "ABSENT"


ABSENT = _Absent()
SECONDS = {"s": 1, "min": 60, "h": 3600, "day": 86400, "days": 86400, "week": 604800, "weeks": 604800}
EPOCH = datetime.datetime(1970, 1, 1, tzinfo=datetime.timezone.utc)
CLOCK_START = "2026-01-01T00:00:00Z"            # flow-format.md §11.3
OPERATOR = "operator"                           # the alias, and the identity, of the store's operator (ADR-0126)
BUCKETS = {"day", "week", "month", "quarter", "year"}
ROW_AGGREGATES = {"count", "sum", "min", "max", "avg", "median", "percentile"}
AVERAGE_SCALE = 6                               # an average of integers is a decimal to six places (ADR-0139)
# each currency's minor unit, the scale its amounts are rounded to (ISO 4217; declaration-syntax.md §8.3)
MINOR_UNITS = {"USD": 2, "EUR": 2, "GBP": 2, "SGD": 2, "AUD": 2, "CAD": 2, "CHF": 2, "CNY": 2, "HKD": 2, "NZD": 2,
               "MYR": 2, "INR": 2, "THB": 2, "JPY": 0, "KRW": 0, "BHD": 3, "KWD": 3, "OMR": 3, "JOD": 3, "TND": 3}
# the tags of the values a tuple holds that are not sets: a state, an enumeration member, a category, a type named in
# an expression, the markers `inputs` and `actor`, and an event
TAGS = {"state", "enum", "category", "typeref", "marker", "event"}
parsed = functools.lru_cache(maxsize=None)(flowexpr.parse)


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


class Row(dict):
    """A row of an object's own flow data: a span of `intervals` or an event of `transitions` (§6.9)."""


class Shared:
    """Declarations every copy of a store reads and none writes, which a request's copy shares rather than copies."""

    def __init__(self, **kw):
        self.__dict__.update(kw)

    def __deepcopy__(self, memo):
        return self


def is_set(v):
    """Whether a value is a set: a tuple, but not one of the tagged tuples a state or an enumeration member is. A set
    of strings whose first is one of the tags would be misread, and no example writes one."""
    return isinstance(v, tuple) and not (len(v) in (2, 3) and v[0] in TAGS)


def half_even(v, places):
    """A value rounded half to even to so many decimal places (declaration-syntax.md §8.3)."""
    scaled = Fraction(v) * 10 ** places
    n = math.floor(scaled)
    rest = scaled - n
    if rest > Fraction(1, 2) or (rest == Fraction(1, 2) and n % 2):
        n += 1
    return Fraction(n, 10 ** places)


def rounded(v, kind):
    """A metric's value rounded half to even to the scale of its expression's type (ADR-0128)."""
    if v is ABSENT or isinstance(v, bool) or kind[0] == "bool":
        return v
    if kind[0] == "int":
        return v if isinstance(v, int) else int(half_even(v, 0))
    if kind[0] in ("decimal", "literal"):
        return half_even(v, kind[1])
    if kind[0] == "money":
        if kind[1] not in MINOR_UNITS:
            raise NotRunnable(f"an amount in {kind[1]}, whose minor unit the runner does not know")
        return half_even(v, MINOR_UNITS[kind[1]])
    if kind[0] == "duration":
        return half_even(v, 0)                  # a duration is held in whole seconds (storage-schema.md §3.1)
    if kind[0] == "timestamp":
        return v                                # no metric arithmetic makes one fractional: the clock moves by whole units
    if isinstance(v, (int, Fraction)) and Fraction(v).denominator != 1:
        raise NotRunnable("a metric's fractional value, whose type the runner cannot tell")
    return v


def spec_kind(spec):
    """An attribute's type, as far as a value's scale needs it (see Evaluator.kind)."""
    spec = spec or {}
    if "reference" in spec:
        ref = spec["reference"]
        return ("objects", ref[:-2]) if ref.endswith("[]") else ("object", ref)
    typ = str(spec.get("type", ""))
    m = re.fullmatch(r"decimal\((\d+),\s*(\d+)\)", typ)
    if m:
        return ("decimal", int(m.group(2)))
    m = re.fullmatch(r"money\((\w+)\)", typ)
    if m:
        return ("money", m.group(1))
    if typ in ("int", "counter"):
        return ("int",)
    return (typ,) if typ in ("duration", "timestamp", "bool") else ("other",)


def arith_kind(op, a, b):
    """The type of `a op b` as declaration-syntax.md §8.3 gives it, for the scale of a metric's value."""
    scalar = ("int", "decimal", "literal")
    if a[0] == "literal" and b[0] == "decimal":     # a decimal literal takes the scale of what it is combined with (§8.1)
        a = b
    if b[0] == "literal" and a[0] == "decimal":
        b = a
    if op in ("+", "-"):
        if op == "-" and a[0] == b[0] == "timestamp":
            return ("duration",)
        if (a[0], b[0]) == ("timestamp", "duration") or (op == "+" and (a[0], b[0]) == ("duration", "timestamp")):
            return ("timestamp",)
        if a[0] == b[0] == "literal":
            return ("literal", max(a[1], b[1]))
        return a if a == b else ("other",)
    if op == "*":
        if a[0] in scalar and b[0] in scalar:
            if a[0] == b[0] == "int":
                return ("int",)
            return ("decimal", (0 if a[0] == "int" else a[1]) + (0 if b[0] == "int" else b[1]))
        return b if a[0] in scalar else (a if b[0] in scalar else ("other",))
    if op == "/":
        if a == b and a[0] in ("duration", "money"):
            return ("decimal", 6)                   # a quotient of two like quantities
        if b[0] in scalar:
            return ("decimal", a[1]) if a[0] == "literal" else a
    return ("other",)


def bucket(unit, ts):
    """A timestamp's UTC calendar bucket, the week being the ISO 8601 week that begins on Monday (ADR-0106)."""
    t = EPOCH + datetime.timedelta(seconds=math.floor(ts))
    if unit == "week":
        year, week, _ = t.isocalendar()
        return f"{year}-W{week:02d}"
    return {"day": t.strftime("%Y-%m-%d"), "month": t.strftime("%Y-%m"), "quarter": f"{t.year}-Q{(t.month - 1) // 3 + 1}",
            "year": str(t.year)}[unit]


def tracked_members(kinds, enums, tn):
    """The members whose every value is timed: the state, each enumeration attribute and each singular stored
    reference (PRD D11, DESIGN.md §7)."""
    t = kinds.get(tn) or {}
    out = ["state"]
    for a, spec in (t.get("attributes") or {}).items():
        spec = spec or {}
        if spec.get("type") in enums:
            out.append(a)
        elif "reference" in spec and not spec["reference"].endswith("[]"):
            back = ((kinds.get(spec["reference"]) or {}).get("attributes") or {}).get(spec.get("opposite") or "") or {}
            if "opposite" not in spec or back.get("reference", "").endswith("[]") or spec.get("stored"):
                out.append(a)
    return out


# The standard metrics (declaration-syntax.md §6.11), each written as flow-format.md §4.9 writes a formula: the first
# table's over a type <T>, the second's over it and an assignee reference <r>. Those over attempt counts read a log the
# runner does not keep, and an example that reads one is reported as not run.
STANDARD = {
    "time_in_state": {"source": "intervals", "item": "i", "dimensions": {"state": "i.state", "month": "month(i.entered_at)"},
                      "expression": "median(i.duration)"},
    "time_in_state_p80": {"source": "intervals", "item": "i", "dimensions": {"state": "i.state", "month": "month(i.entered_at)"},
                          "expression": "percentile(0.8, i.duration)"},
    "throughput": {"source": "transitions", "item": "t", "filter": "t.completes and not t.imported and not t.migrated",
                   "dimensions": {"state": "t.to_state", "week": "week(t.occurred_at)", "actor_kind": "t.actor_kind"},
                   "expression": "count()"},
    "work_in_progress": {"source": "objects", "item": "o", "filter": "o.open", "dimensions": {"state": "o.state"},
                         "expression": "count()"},
    "oldest_open": {"source": "objects", "item": "o", "filter": "o.open", "dimensions": {"state": "o.state"},
                    "expression": "max(now - o.entered_at(state))"},
    "transition_counts": {"source": "transitions", "item": "t", "filter": "not t.imported and not t.migrated and not t.redacted",
                          "dimensions": {"transition": "t.transition", "actor_kind": "t.actor_kind", "week": "week(t.occurred_at)"},
                          "expression": "count()"},
    "refusals": {"source": "attempt_counts", "item": "a", "filter": "a.enforced",
                 "dimensions": {"transition": "a.transition", "clause": "a.clause", "remedy": "a.remedy",
                                "actor_kind": "a.actor_kind", "week": "week(a.day)"}, "expression": "sum(a.count)"},
    "flags_raised": {"source": "attempt_counts", "item": "a", "filter": "a.flagged",
                     "dimensions": {"transition": "a.transition", "clause": "a.clause", "remedy": "a.remedy",
                                    "actor_kind": "a.actor_kind", "week": "week(a.day)"}, "expression": "sum(a.count)"},
    "refusal_rate": {"input_metrics": {"refused": "<T>.refusals", "applied": "<T>.transition_counts"},
                     "group_by": ["transition", "clause", "actor_kind", "week"], "expression": "refused * 1.000 / (refused + applied)"},
    "override_counts": {"source": "transitions", "item": "t", "filter": "t.overrides and not t.imported",
                        "dimensions": {"state": "t.to_state", "reason": "t.reason", "week": "week(t.occurred_at)"},
                        "expression": "count()"},
    "rework": {"source": "transitions", "item": "t", "filter": "t.returns and not t.imported and not t.migrated",
               "dimensions": {"state": "t.to_state", "week": "week(t.occurred_at)"}, "expression": "count()"},
}
ASSIGNMENT = {
    "open_work": {"source": "objects", "item": "o", "filter": "o.open", "dimensions": {"assignee": "o.<r>"}, "expression": "count()"},
    "time_unassigned": {"source": "intervals(<r>)", "item": "i", "filter": "i.value is null",
                        "dimensions": {"month": "month(i.entered_at)"}, "expression": "median(i.duration)"},
    "time_to_first_assignment": {"source": "objects", "item": "o", "filter": "any(i in o.intervals(<r>) where i.value is not null)",
                                 "dimensions": {"month": "month(o.created_at)"},
                                 "expression": "median(min(i in o.intervals(<r>) where i.value is not null: i.entered_at) - o.created_at)"},
    "time_with_assignee": {"source": "intervals(<r>)", "item": "i", "filter": "i.value is not null",
                           "dimensions": {"assignee": "i.value"}, "expression": "sum(i.duration)"},
    "cycle_time_by_assignee": {"source": "transitions", "item": "t", "filter": "t.completes and not t.imported and not t.migrated",
                               "dimensions": {"assignee": "t.held(<r>)", "state": "t.to_state", "month": "month(t.occurred_at)"},
                               "expression": "median(t.occurred_at - t.object.created_at)"},
    "time_in_state_by_holder": {"source": "intervals", "item": "i", "dimensions": {"state": "i.state", "assignee": "i.held(<r>)"},
                                "expression": "median(i.duration)"},
    "handoffs": {"source": "objects", "item": "o", "filter": "any(i in o.intervals(<r>) where i.value is not null)",
                 "dimensions": {"month": "month(o.created_at)"},
                 "expression": "avg(count(i in o.intervals(<r>) where i.value is not null) - 1)"},
    "reassigned_back": {"source": "objects", "item": "o", "dimensions": {"month": "month(o.created_at)"},
                        "expression": "count(where count(distinct i in o.intervals(<r>) where i.value is not null: i.value) "
                                      "< count(i in o.intervals(<r>) where i.value is not null))"},
    "acted_by_non_assignee": {"source": "transitions", "item": "t",
                              "filter": "t.held(<r>) is not null and not t.imported and not t.migrated and not t.redacted",
                              "dimensions": {"transition": "t.transition"},
                              "expression": "count(where t.actor_id != t.held(<r>).actor_id) * 1.000 / count()"},
    "handoffs_by_object": {"source": "objects", "item": "o", "dimensions": {"object": "o.id"},
                           "expression": "sum(count(i in o.intervals(<r>) where i.value is not null) - 1)"},
    "returns_by_object": {"source": "objects", "item": "o", "dimensions": {"object": "o.id"},
                          "expression": "sum(count(i in o.intervals(<r>) where i.value is not null) "
                                        "- count(distinct i in o.intervals(<r>) where i.value is not null: i.value))"},
}


def instantiated(spec, owner, ref):
    """A standard metric over a type and, for an assignment metric, its assignee reference."""
    if isinstance(spec, str):
        return spec.replace("<T>", owner).replace("<r>", ref or "")
    if isinstance(spec, dict):
        return {k: instantiated(v, owner, ref) for k, v in spec.items()}
    if isinstance(spec, list):
        return [instantiated(v, owner, ref) for v in spec]
    return spec


def formula(kinds, enums, owner, spec):
    """A declared metric's rows and value as a formula (flow-format.md §4.9). A fixed measure is the formula §6 converts
    it to, with the dimensions and window of service's `time_working` (declaration-syntax.md §6.9, ADR-0139): a
    tracked attribute read as it was held, any other as it is now, and a month or week of the row's time, which it is
    windowed on."""
    measure = spec.get("measure")
    if measure is None:
        return spec
    if measure == "median_time_in_state":
        item, stamp = "i", "i.entered_at"
        out = {"source": "intervals", "item": "i", "filter": f"i.state == {owner}.{spec['state']}", "expression": "median(i.duration)"}
    elif measure == "transition_count":
        item, stamp = "t", "t.occurred_at"
        x = ((kinds.get(owner) or {}).get("transitions") or {}).get(spec["transition"]) or {}
        froms = x.get("from")
        froms = [froms] if isinstance(froms, str) else list(froms or [])
        if x.get("kind") == "initial":
            along = f"t.from_state is null and t.to_state == {owner}.{x['to']}"
        elif x.get("kind") in ("external", "internal") and froms:
            along = "t.from_state is not null" if froms == ["any"] else "t.from_state in {" + ", ".join(f"{owner}.{f}" for f in froms) + "}"
            along += f" and t.to_state == {owner}.{x['to']}" if x["kind"] == "external" else " and t.to_state == t.from_state"
        else:
            raise NotRunnable(f"a transition_count of {owner}.{spec['transition']}, a transition of the kind {x.get('kind')}")
        out = {"source": "transitions", "item": "t", "filter": along, "expression": "count()"}
    else:
        raise NotRunnable(f"the measure {measure}")
    tracked = tracked_members(kinds, enums, owner)
    dims = {}
    for g in spec.get("group_by") or []:
        if g in ("month", "week"):
            dims[g] = f"{g}({stamp})"
        elif g == "actor":
            dims[g] = "t.actor_id"
        else:
            dims[g] = f"{item}.held({g})" if g in tracked else f"{item}.object.{g}"
    return dict(out, dimensions=dims, time_dimension=stamp)


def dotted(ref):
    return ref[1] if ref[0] == "name" else f"{dotted(ref[1])}.{ref[2]}" if ref[0] == "path" else str(ref)


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
        self.spans = {}                         # tracked member -> its spans, each a Row (DESIGN.md §7)
        self.writes = {}                        # attribute or part relationship -> the event that last wrote it
        self.erased = False                     # its own erasure is recorded (DESIGN.md §8)
        self.created_by_kind = "human"          # the kind of actor that created it; the operator's, until a request sets it


class Store:
    """The objects of one example, built from an empty store holding version 0 and its operator."""

    def __init__(self, kinds, enums, metrics=None, homes=None):
        self.kinds, self.enums = kinds, enums
        # module -> each metric its types and it declare, as (type or None, declaration); type -> its module
        self.decl = Shared(metrics=metrics or {}, homes=homes or {})
        self.now = timestamp(CLOCK_START)
        self.objects, self.counter = {}, 0
        self.sequences = {}                     # (sequence, scope value) -> the last number minted
        self.events, self.position = [], 0      # the log, and the last position allocated (DESIGN.md §7)
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

    def find_metric(self, ref, tn=None, module=None):
        """What a metric reference names (declaration-syntax.md §6.9, §6.11): a bare name, a metric of its module, its
        types' or its own; `<Type>.<name>`, a metric declared under that type or a standard one; `<Type>.<name>.<r>`, a
        standard assignment metric over the assignee reference r. Returns (its type or None, the declaration, the
        module)."""
        declared, homes = self.decl.metrics, self.decl.homes
        if ref[0] == "name":
            mod = module if module is not None else homes.get(tn)
            if ref[1] in (declared.get(mod) or {}):
                owner, spec = declared[mod][ref[1]]
                return owner, spec, mod
        elif ref[0] == "path" and ref[1][0] == "name":
            owner, n = ref[1][1], ref[2]
            mod = homes.get(owner)
            hit = (declared.get(mod) or {}).get(n)
            if hit and hit[0] == owner:
                return owner, hit[1], mod
            if n in STANDARD:
                return owner, instantiated(STANDARD[n], owner, None), mod
        elif ref[0] == "path" and ref[1][0] == "path" and ref[1][1][0] == "name":
            owner, n, r = ref[1][1][1], ref[1][2], ref[2]
            if n in ASSIGNMENT and (((self.kinds.get(owner) or {}).get("attributes") or {}).get(r) or {}).get("assignee"):
                return owner, instantiated(ASSIGNMENT[n], owner, r), homes.get(owner)
        raise NotRunnable(f"the metric `{dotted(ref)}`, which names none the runner finds")

    def metric_dimensions(self, owner, spec):
        """A metric's dimensions: those it declares, and `version` and `actor_kind`, which a combined one has only when
        it names them (declaration-syntax.md §6.9)."""
        if "input_metrics" in spec:
            return set(spec.get("group_by") or [])
        return set(formula(self.kinds, self.enums, owner, spec).get("dimensions") or {}) | {"version", "actor_kind"}


class Evaluator:
    """An expression's value over the store, for one object and one request (§8, §9.2)."""

    def __init__(self, store, tn, this=None, inputs=None, actor=None, stubs=None, filter_mode=False, event=None, committed=None):
        self.s, self.tn, self.this, self.event = store, tn, this, event
        self.inputs, self.actor, self.stubs = inputs or {}, actor, stubs or {}
        self.filter_mode = filter_mode          # a filter leaves out an element it cannot decide (§8.2)
        self.committed = committed              # the store as the request found it, which a metric guard reads (ADR-0084)
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
            if self.event is None:
                raise NotRunnable("`this_event` outside an effect")
            return ("event", self.event)
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
        if isinstance(base, Row):
            if m in base:
                return base[m]
            raise NotRunnable(f"the member `{m}` of a row of an object's flow data")
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
        if m == "intervals":
            return self.intervals(o, "state")
        if m == "transitions":
            return tuple(Row(e) for e in self.s.events if e["object"] == o.id)
        if m == "actor_id":
            marked = self.s.actor_attr(o.type)
            return o.attrs.get(marked[0], ABSENT) if marked else ABSENT
        if m in ("attempts", "labels"):
            raise NotRunnable(f"an object's `{m}`")
        if m == "state":
            return ("state", o.type, o.state)
        if m == "created_at":
            return o.created_at
        if m in ("created_by_kind", "recorded_by_kind"):
            return o.created_by_kind
        if m == "recorded_from":
            return o.created_at             # an object the example created has its whole record
        if m == "declaration_version":
            return 1
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

    def intervals(self, o, member):
        """The object's spans in each state, or each value of a tracked member, as rows (§6.9)."""
        if member not in o.spans:
            raise NotRunnable(f"the intervals of {o.type}.{member}, which is not tracked")
        return tuple(Row(sp, duration=self.duration(o, sp)) for sp in o.spans[member])

    def duration(self, o, sp):
        """A span's exit less its entry; a current span runs to now while its object is open, and on a finished object
        to the entry of the state that finished it, a span that began after that entry having none (§6.9, ADR-0101)."""
        if sp["left_at"] is not ABSENT:
            return sp["left_at"] - sp["entered_at"]
        st = ((self.s.kinds.get(o.type) or {}).get("states") or {}).get(o.state) or {}
        if st.get("category") != "closed" and not st.get("final"):
            return self.s.now - sp["entered_at"]
        entry = o.entered.get(o.state, o.created_at)
        return entry - sp["entered_at"] if sp["entered_at"] < entry else ABSENT

    def time_in(self, o, state):
        spans = [sp for sp in o.spans.get("state", []) if sp["state"] == ("state", o.type, state)]
        return sum((d for d in (self.duration(o, sp) for sp in spans) if d is not ABSENT), Fraction(0))

    def entered_at(self, o, what):
        if what in ((self.s.kinds.get(o.type) or {}).get("states") or {}):
            return o.entered.get(what, ABSENT)
        if what in o.spans:
            return o.spans[what][-1]["entered_at"]
        raise NotRunnable(f"`entered_at({what})`, which names neither a state nor a tracked member")

    def changed_since(self, names, event):
        """Whether any of the attributes, or any part of the relationships, was written after the event (§8.3)."""
        if event is ABSENT or self.this is None:
            return ABSENT
        o = self.s.objects[self.this]
        return any(o.writes.get(n, 0) > event[1] for n in names)

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
        if k == "changed":
            return self.changed_since(e[1], self.value(e[2], env))
        if k == "metric":
            return self.metric(e, env)
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
        return self.combine(op, self.value(e[2], env), self.value(e[3], env))

    def combine(self, op, a, b):
        """A comparison or an arithmetic operator over two values (§8.2, §8.3)."""
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
        if fn[0] == "path" and fn[2] in ("held", "intervals", "entered_at", "time_in") and len(args) == 1 and args[0][0] == "name":
            base = self.value(fn[1], env)
            if isinstance(base, Row) and fn[2] == "held":
                return base["held"].get(args[0][1], ABSENT)
            if isinstance(base, str) and base in self.s.objects:
                o = self.s.objects[base]
                if fn[2] == "intervals":
                    return self.intervals(o, args[0][1])
                return self.entered_at(o, args[0][1]) if fn[2] == "entered_at" else self.time_in(o, args[0][1])
        if fn in (("name", "entered_at"), ("name", "time_in")) and len(args) == 1 and args[0][0] == "name":
            if self.this is None:
                return ABSENT
            o = self.s.objects[self.this]
            return self.entered_at(o, args[0][1]) if fn[1] == "entered_at" else self.time_in(o, args[0][1])
        if fn == ("name", "length") and len(args) == 1:
            v = self.value(args[0], env)
            return ABSENT if v is ABSENT else len(v)
        if fn[0] == "name" and fn[1] in BUCKETS and len(args) == 1:
            v = self.value(args[0], env)
            return ABSENT if v is ABSENT else bucket(fn[1], v)
        raise NotRunnable(f"the function `{fn[1] if fn[0] == 'name' else fn[2]}`")


    # ── a metric, as a guard reads it (declaration-syntax.md §6.9) ──────────────
    def metric(self, e, env):
        """A declared or standard metric's value: its rows narrowed to the dimensions the guard binds and to the window,
        aggregated over every dimension left unbound. `fresh` holds by construction, since the value is computed as the
        request is decided."""
        _, ref, binds, over, _fresh = e
        committed = self.committed or self.s
        owner, spec, module = committed.find_metric(ref, self.tn)
        bound = {}
        for d, x in binds:
            v = self.value(x, env)
            if committed is not self.s:
                # consulted before the transaction, so resolved against the store as the request found it, and again
                # inside it; they differ only where the request itself wrote what the argument reads (ADR-0084 §6)
                if self.this is not None and self.this not in committed.objects:
                    raise NotRunnable("a metric guard of an object the request itself creates, whose arguments no "
                                      "consultation before it can resolve")
                if Evaluator(committed, self.tn, self.this, self.inputs, self.actor, self.stubs).value(x, env) != v:
                    raise NotRunnable("a metric guard's argument the request itself writes, which check 61 refuses at publish")
            if v is ABSENT:
                return ABSENT               # a dimension bound to an unknown value leaves the metric unknown (ADR-0139)
            bound[d] = v
        window = None if over is None else self.value(over, env)
        # its rows are those of the store as the request found it, whichever transition of the request reads it
        return Evaluator(committed, self.tn, stubs=self.stubs).read_metric(owner, spec, module, bound, window)[0]

    def read_metric(self, owner, spec, module, bound, window):
        """A metric's value over the rows its filter, the bound dimensions and the window select, rounded half to even to
        its expression's scale (ADR-0128); and that expression's type."""
        s = self.s
        if "input_metrics" in spec:
            # a combined metric has no rows: each input is aggregated over the rows the read selects, those matching each
            # bound dimension it has and within the window by its own time, and the values are combined (ADR-0124, ADR-0128)
            for d in bound:
                if d not in (spec.get("group_by") or []):
                    raise NotRunnable(f"a read binding {d}, which the combined metric does not group by")
            values, kinds = {}, {}
            for n, ref in spec["input_metrics"].items():
                o2, s2, m2 = s.find_metric(parsed(ref), module=module)
                mine = s.metric_dimensions(o2, s2)
                values[n], kinds[n] = self.read_metric(o2, s2, m2, {d: v for d, v in bound.items() if d in mine}, window)
            tree = parsed(spec["expression"])
            kind = self.kind(tree, kinds)
            return rounded(self.value(tree, values), kind), kind
        f = formula(s.kinds, s.enums, owner, spec)
        rows, rowkind, by_kind = self.metric_rows(owner, f["source"])
        item = f["item"]
        row = Evaluator(s, owner, stubs=self.stubs, filter_mode=True)
        dims = {"version": f"{item}.declaration_version", "actor_kind": f"{item}.{by_kind}"}
        dims.update(f.get("dimensions") or {})
        for d in bound:
            if d not in dims:
                raise NotRunnable(f"a read binding {d}, which the metric does not have")
        if window is not None and not f.get("time_dimension"):
            raise NotRunnable("a window on a metric with no time dimension")
        where = parsed(f["filter"]) if f.get("filter") else None
        stamp = parsed(f["time_dimension"]) if window is not None else None
        picked = []
        for r in rows:
            env = {item: r}
            if where is not None and truth(row.value(where, env)) is not True:
                continue                    # a filter that is unknown selects nothing (§8.2)
            if stamp is not None:
                at = row.value(stamp, env)
                if at is ABSENT or not s.now - window <= at <= s.now:
                    continue                # the window holds its start, and a row with no time is in none (ADR-0139)
            if all(v in got if is_set(got) else got == v
                   for d, v in bound.items() for got in (row.reach(parsed(dims[d]), env),)):
                picked.append(r)
        tree = parsed(f["expression"])
        kind = row.kind(tree, {item: rowkind})
        return rounded(row.over_rows(tree, picked, item, rowkind), kind), kind

    def metric_rows(self, owner, source):
        """A metric's rows (declaration-syntax.md §6.9): each object of the type or its family, each span of its state or
        of a tracked member, each event of a transition, or each observation of one of its kinds that nothing has
        corrected. Returns them, what a row is, and the member naming the kind of actor behind one."""
        s = self.s
        family = [o for o in sorted(s.objects.values(), key=lambda o: o.id_order) if s.is_a(o.type, owner)]
        if source == "objects":
            return [o.id for o in family], ("object", owner), "created_by_kind"
        if source == "intervals" or source.startswith("intervals("):
            member = "state" if source == "intervals" else source[len("intervals("):-1].strip()
            return [r for o in family for r in self.intervals(o, member)], ("interval", owner), "entered_by_kind"
        if source == "transitions":
            ids = {o.id for o in family}
            return [Row(e) for e in s.events if e["object"] in ids], ("transition", owner), "actor_kind"
        observed = (s.kinds.get(owner) or {}).get("observations") or {}
        if source in observed:
            kind = observed[source]["kind"]
            ids = {o.id for o in family}
            corrected = {x.attrs.get("corrects") for x in s.objects.values() if x.type == kind}
            return ([x.id for x in sorted(s.objects.values(), key=lambda x: x.id_order)
                     if x.type == kind and x.attrs.get("subject") in ids and x.id not in corrected],
                    ("object", kind), "recorded_by_kind")
        if source in ("attempts", "attempt_counts"):
            raise NotRunnable(f"a metric over `{source}`, the attempt log the runner does not keep")
        raise NotRunnable(f"a metric over `{source}`")

    def reach(self, e, env):
        """A dimension's value for one row: a path through a set end reaches each distinct value, and the row is counted
        under each (ADR-0123)."""
        if e[0] != "path":
            return self.value(e, env)
        base = self.reach(e[1], env)
        if not is_set(base):
            return self.member(base, e[2])
        out = []
        for b in base:
            v = self.member(b, e[2])
            for x in (v if is_set(v) else (v,)):
                if x is not ABSENT and x not in out:
                    out.append(x)
        return tuple(out)

    def over_rows(self, e, rows, item, rowkind):
        """A metric's value over one group's rows: its own aggregates range over them, leaving out a row whose body is
        absent, and the arithmetic between them is the language's (§6.9, §8.3)."""
        k = e[0]
        if k == "agg" and e[3] is None:     # count(where f), count(distinct b)
            _, fn, distinct, _var, _coll, where, body = e
            if fn != "count":
                raise NotRunnable(f"`{fn}` over a metric's rows with a filter")
            picked = [r for r in rows if where is None or truth(self.value(where, {item: r})) is True]
            if not distinct:
                return len(picked)
            seen = []
            for r in picked:
                v = self.value(body, {item: r})
                if v is not ABSENT and v not in seen:
                    seen.append(v)
            return len(seen)
        if k == "call" and e[1][0] == "name" and e[1][1] in ROW_AGGREGATES:
            fn, args = e[1][1], e[2]
            if fn == "count":
                if args:
                    raise NotRunnable("`count` of an expression over a metric's rows")
                return len(rows)
            values = [v for v in (self.value(args[-1], {item: r}) for r in rows) if v is not ABSENT]
            if fn == "sum":
                return sum(values, 0)
            if not values:
                return ABSENT               # min, max, avg, median and a percentile over no rows are absent (§8.3)
            if fn in ("min", "max"):
                return (min if fn == "min" else max)(values)
            if fn == "avg":
                return rounded(Fraction(sum(values, 0)) / len(values), self.kind(e, {item: rowkind}))
            p = Fraction(1, 2) if fn == "median" else self.value(args[0], {})
            ordered = sorted(values)        # nearest rank: the smallest whose rank is at least p × n; a median the lower middle
            return ordered[max(1, math.ceil(p * len(ordered))) - 1]
        if k == "bin" and e[1] not in ("and", "or", "implies"):
            return self.combine(e[1], self.over_rows(e[2], rows, item, rowkind), self.over_rows(e[3], rows, item, rowkind))
        if k == "neg":
            v = self.over_rows(e[1], rows, item, rowkind)
            return ABSENT if v is ABSENT else -v
        if k in ("num", "dur", "money") or e == ("name", "now"):
            return self.value(e, {})
        raise NotRunnable("a metric's value reading its rows outside an aggregate")

    def kind(self, e, env):
        """An expression's type, as far as a metric's scale needs it (§8.1, §8.3): ("int",), ("decimal", scale), a
        decimal literal ("literal", scale), ("money", currency), ("duration",), ("timestamp",), ("bool",), an object
        ("object", type), a row ("interval" | "transition", type), a collection ("objects" | "intervals" |
        "transitions", type), or ("other",)."""
        k = e[0]
        if k == "num":
            return ("int",) if isinstance(e[1], int) else ("literal", len(str(e[1]).partition(".")[2]))
        if k == "dur":
            return ("duration",)
        if k == "money":
            return ("money", e[1])
        if k in ("bool", "not", "isnull", "in", "changed"):
            return ("bool",)
        if k == "name":
            if e[1] in env:
                return env[e[1]]
            if e[1] == "now":
                return ("timestamp",)
            return self.member_kind(env["this"], e[1]) if "this" in env else ("other",)
        if k == "path":
            return self.member_kind(self.kind(e[1], env), e[2])
        if k == "neg":
            return self.kind(e[1], env)
        if k == "if":
            return self.kind(e[2], env)
        if k == "bin":
            if e[1] in ("and", "or", "implies") or e[1] in flowexpr.COMPARISONS:
                return ("bool",)
            return arith_kind(e[1], self.kind(e[2], env), self.kind(e[3], env))
        if k == "agg":
            _, fn, _distinct, var, coll, _where, body = e
            if fn in ("count", "all", "any", "none"):
                return ("int",) if fn == "count" else ("bool",)
            if var is not None:
                c = self.kind(coll, env)
                element = {"objects": "object", "intervals": "interval", "transitions": "transition"}.get(c[0])
                env = dict(env, **{var: (element, c[1]) if element else ("other",)})
            return self.aggregate_kind(fn, body, env)
        if k == "call":
            fn, args = e[1], e[2]
            if fn[0] == "name":
                if fn[1] == "count" or fn[1] == "length":
                    return ("int",)
                if fn[1] in ROW_AGGREGATES:
                    return self.aggregate_kind(fn[1], args[-1], env)
                return {"entered_at": ("timestamp",), "time_in": ("duration",)}.get(fn[1], ("other",))
            base = self.kind(fn[1], env)
            if fn[2] in ("entered_at", "time_in"):
                return ("timestamp",) if fn[2] == "entered_at" else ("duration",)
            if fn[2] == "intervals" and base[0] == "object":
                return ("intervals", base[1])
            if fn[2] == "held" and base[0] in ("interval", "transition") and args and args[0][0] == "name":
                return self.member_kind(("object", base[1]), args[0][1])
        return ("other",)

    def aggregate_kind(self, fn, body, env):
        """An aggregate's type: its body's, except that an average of integers is a decimal (ADR-0106, ADR-0139)."""
        b = self.kind(body, env)
        if fn != "avg":
            return b
        if b[0] == "int":
            return ("decimal", AVERAGE_SCALE)
        if b[0] in ("decimal", "literal"):
            return ("decimal", b[1])
        if b[0] in ("duration", "money"):
            return b
        raise NotRunnable("an average of values that have no scale")

    def member_kind(self, base, m):
        if base[0] == "interval":
            return {"entered_at": ("timestamp",), "left_at": ("timestamp",), "duration": ("duration",),
                    "object": ("object", base[1])}.get(m, ("other",))
        if base[0] == "transition":
            return {"occurred_at": ("timestamp",), "recorded_at": ("timestamp",), "object": ("object", base[1])}.get(m, ("other",))
        if base[0] != "object":
            return ("other",)
        t = self.s.kinds.get(base[1]) or {}
        if m in ("created_at", "recorded_at", "occurred_at", "recorded_from", "imported_at"):
            return ("timestamp",)
        if m in ("intervals", "transitions"):
            return (m, base[1])
        if m in (t.get("observations") or {}):
            return ("objects", t["observations"][m]["kind"])
        if m in (t.get("attributes") or {}):
            return spec_kind(t["attributes"][m])
        if m in (t.get("derived_attributes") or {}):
            return self.kind(parsed(t["derived_attributes"][m]["expression"]), {"this": base})
        return ("other",)

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
        self.position_of = {}       # object id -> the position of the event its current transition records
        self.before = None          # the store as the request found it
        self.attr_writes = {}       # object id -> the attributes its current transition's effect wrote

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
        ctx.before = store
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
            requested = oid if oid is not None else root
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
                    first = refused[0].rpartition(".")[2]
                    raise Refused("invariant_violated", refused[0], self.invariant_remedy(work, x, obj, first, requested),
                                  detail=", ".join(refused[1:]) and "and " + ", ".join(refused[1:]))
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
            asking = Evaluator(work, tn, oid, inputs, ctx.who, ctx.stubs, committed=ctx.before)
            ok = truth(asking.value(flowexpr.parse(conditions[g]["expression"]), {}))
            if ok is not True:
                if mode == "deny":
                    # an evaluator that did not answer leaves the guard unknown, and the caller may try again (ADR-0138)
                    raise Refused("unsatisfied", g, "temporal" if asking.unanswered else conditions[g].get("remedy"))
                ctx.flags.append((f"{tn}.{g}", mode))
        # 5. the outcome: its event's position allocated as it begins applying, so this_event is known to it (§7)
        work.position += 1
        pos = work.position
        before = (work.objects[oid].state, dict(work.objects[oid].attrs), self.tracked_values(work.objects[oid])) if oid else (None, {}, {})
        if oid is None:
            o = Obj(work.new_id(tn), tn, x["to"], work.now)
            o.id_order = work.counter
            o.created_by_kind = ctx.who["kind"]
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
        ctx.position_of[o.id] = pos
        ctx.attr_writes[o.id] = set(inputs) & set(x.get("required_inputs", []) + x.get("optional_inputs", []))
        for w in wholes:
            if w is not None:
                ctx.wrote(w)
        ctx.taken.append(f"{tn}.{xn}")
        # 6. the effect, then the cascades to parts, depth-first (ADR-0038, ADR-0138)
        self.effects(ctx, tn, o, x.get("effect") or [], inputs, {}, depth)
        self.cascades(ctx, t, tn, xn, o, inputs, depth)
        if x["kind"] == "erasure":
            self.erase_chain(ctx, o, inputs, depth)
        self.record(ctx, o, x, xn, pos, before, inputs, created=oid is None)
        o.version += 1
        return o.id

    # ── the record a transition leaves: its event, its spans and what it wrote ─────
    def tracked(self, tn):
        return tracked_members(self.kinds, self.enums, tn)

    def tracked_values(self, o):
        return {m: (("state", o.type, o.state) if m == "state" else o.attrs.get(m, ABSENT)) for m in self.tracked(o.type)}

    def record(self, ctx, o, x, xn, pos, before, inputs, created):
        work = ctx.work
        before_state, before_attrs, held = before
        now_values = self.tracked_values(o)
        for m, v in now_values.items():
            spans = o.spans.setdefault(m, [])
            if created or not spans or held.get(m, ABSENT) != v:
                if spans and spans[-1]["left_at"] is ABSENT:
                    spans[-1]["left_at"] = work.now
                key = "state" if m == "state" else "value"
                spans.append(Row({"object": o.id, key: v, "entered_at": work.now, "left_at": ABSENT,
                                  "entered_by_kind": ctx.who["kind"], "declaration_version": 1, "legacy": False,
                                  "held": dict(now_values)}))
        written = ctx.attr_writes.get(o.id, set()) | ctx.erased.get(o.id, set())
        written |= {a for a in set(before_attrs) | set(o.attrs) if before_attrs.get(a, ABSENT) != o.attrs.get(a, ABSENT)}
        for a in written:
            o.writes[a] = pos
        # a part's transition is an event on its whole's part relationship, and a recording on its subject's collection
        for a, spec in ((self.kinds.get(o.type) or {}).get("attributes") or {}).items():
            if self.owner_end(spec) and o.attrs.get(a) in work.objects:
                work.objects[o.attrs[a]].writes[spec["opposite"]] = pos
        if xn == "record" and o.attrs.get("subject") in work.objects:
            subject = work.objects[o.attrs["subject"]]
            for name, ob in ((self.kinds.get(subject.type) or {}).get("observations") or {}).items():
                if ob["kind"] == o.type:
                    subject.writes[name] = pos
        states = (self.kinds.get(o.type) or {}).get("states") or {}

        def is_open(sn):
            st = states.get(sn) or {}
            return st.get("category") != "closed" and not st.get("final")
        visited = {sp["state"][2] for sp in o.spans.get("state", [])[:-1]}
        reason = inputs.get("reason") if x["kind"] == "assertion" else ("erasure" if x["kind"] == "erasure" else ABSENT)
        work.events.append({
            "position": pos, "object": o.id, "type": o.type, "transition": xn,
            "from_state": ("state", o.type, before_state) if before_state else ABSENT, "to_state": ("state", o.type, o.state),
            "completes": bool(before_state) and is_open(before_state) and not is_open(o.state),
            "returns": bool(before_state) and before_state != o.state and o.state in visited,
            "occurred_at": work.now, "recorded_at": work.now, "actor_id": ctx.who["id"], "actor_kind": ctx.who["kind"],
            "asserted": x["kind"] == "assertion", "overrides": x["kind"] == "assertion", "imported": False, "migrated": False,
            "redacted": False, "reason": reason, "declaration_version": 1, "held": held})

    def effects(self, ctx, tn, o, steps, inputs, env, depth):
        """An effect's steps in order; returns whether one reached another object by a call or a creation."""
        work, reached = ctx.work, False
        attrs = (self.kinds.get(tn) or {}).get("attributes") or {}

        def ev(tree, filtering=False):
            return Evaluator(work, tn, o.id, inputs, ctx.who, ctx.stubs, filter_mode=filtering,
                             event=ctx.position_of.get(o.id)).value(tree, env)

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
                ctx.attr_writes.setdefault(o.id, set()).add(spec["location"])
                if self.owner_end(attrs.get(spec["location"])):
                    ctx.wrote(v)
            elif kind == "clear":
                for a in spec:
                    o.attrs.pop(a, None)
                ctx.attr_writes.setdefault(o.id, set()).update(spec)
            elif kind in ("add", "remove"):
                tree = flowexpr.parse(spec["expr"])
                if unsupplied(tree):
                    continue
                v = ev(tree)
                if v is ABSENT:
                    raise NotRunnable(f"an `{kind}` of an absent value")
                ctx.attr_writes.setdefault(o.id, set()).add(spec["location"])
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

    def invariant_remedy(self, work, x, obj, n, requested):
        """The remedy DESIGN.md §5.5 infers for an invariant that fails, a property of the transition requested:
        `self_serviceable` where it takes an input it writes to a value the invariant reads, an attribute input writing
        its attribute and an assertion's `to` the state; else `dependent` where the invariant reads another object,
        through a relationship or a type scan; else `unreachable_from_here`. A uniqueness failing on another object of the
        family reads the requested one through its scan. None where the runner cannot tell: any other invariant on
        another object, which may read the requested one through a relationship, or one reading `this`."""
        mine = work.objects[requested].type
        if obj.id != requested and not (n.endswith("_unique") and (work.is_a(mine, obj.type) or work.is_a(obj.type, mine))):
            return None
        t = self.kinds.get(obj.type) or {}
        attrs = t.get("attributes") or {}
        if n.endswith("_unique"):
            unique = (attrs.get(n[:-len("_unique")]) or {}).get("unique")
            reads, other = {n[:-len("_unique")]} | set(unique.get("with", []) if isinstance(unique, dict) else []), True
        elif n.endswith("_invariant"):
            state = {st.lower(): v for st, v in (t.get("states") or {}).items()}.get(n[:-len("_invariant")]) or {}
            reads, other = set(state.get("required_attributes") or []) | {"state"}, False
        else:
            names, reads, other, seen = set(names_read(parsed((t.get("invariants") or {})[n]["expression"]))), set(), False, set()
            while names:
                m = names.pop()
                if m in seen:
                    continue
                seen.add(m)
                if m == "this":
                    return None
                if m in (t.get("derived_attributes") or {}):
                    names |= names_read(parsed(t["derived_attributes"][m]["expression"]))
                    continue
                if m == "state" or m in attrs or m in (t.get("observations") or {}):
                    reads.add(m)
                other |= m in (t.get("observations") or {}) or "reference" in (attrs.get(m) or {}) \
                    or m == "referrers" or (m in self.kinds and m not in (t.get("states") or {}))
        writes = set(x.get("required_inputs", [])) | set(x.get("optional_inputs", []))
        if x["kind"] == "assertion":
            writes.add("state")
        if writes & reads:
            return "self_serviceable"
        return "dependent" if other else "unreachable_from_here"

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


def declarations(modules):
    """Each module's metrics, its types' and its own, which share one namespace (flow-format.md §4.9), as
    (type or None, declaration) by name; and the module declaring each type."""
    declared, homes = {}, {}
    for mn, m in modules.items():
        for tn, t in (m.get("types") or {}).items():
            homes[tn] = mn
            for n, spec in (t.get("metrics") or {}).items():
                declared.setdefault(mn, {})[n] = (tn, spec)
        for n, spec in (m.get("metrics") or {}).items():
            declared.setdefault(mn, {})[n] = (None, spec)
    return declared, homes


def run_examples(doc, library, modules):
    """Each example's outcome: (name, "passed" | "failed" | "not run", message)."""
    enums = {n: list(v) for m in modules.values() for n, v in (m.get("enumerations") or {}).items()}
    kinds = dict(library)
    kinds.update(observation_kinds(modules))
    kinds["Operator"] = {"attributes": {"identity": {"type": "identity", "actor_kind": "human"}},
                         "states": {"ACTIVE": {"category": "live"}, "RETIRED": {"category": "closed", "final": True}}}
    runner = Runner(kinds, enums)
    declared, homes = declarations(modules)
    setups = doc.get("setups") or {}
    results = []

    def chain(name):
        s = setups[name]
        return (chain(s["given"]) if "given" in s else []) + [("setup " + name, s)]

    for name, ex in (doc.get("examples") or {}).items():
        store = Store(kinds, enums, declared, homes)
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
        store.position += 1
        values = runner.tracked_values(o)
        for m, v in values.items():
            o.spans[m] = [Row({"object": o.id, "state" if m == "state" else "value": v, "entered_at": store.now, "left_at": ABSENT,
                               "entered_by_kind": "human", "declaration_version": 1, "legacy": False, "held": dict(values)})]
        for a in o.attrs:
            o.writes[a] = store.position
        store.events.append({"position": store.position, "object": o.id, "type": o.type, "transition": "import", "from_state": ABSENT,
                             "to_state": ("state", o.type, o.state), "completes": False, "returns": False,
                             "occurred_at": store.now, "recorded_at": store.now, "actor_id": OPERATOR, "actor_kind": "human",
                             "asserted": True, "overrides": False, "imported": True, "migrated": False, "redacted": False,
                             "reason": ABSENT, "declaration_version": 1, "held": {}})
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
        if "remedy" in want and got.verdict == "invariant_violated" and got.remedy is None:
            raise NotRunnable("the remedy of an invariant that fails on an object other than the one requested, or reads `this`")
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
    if "warned" in want:
        # the flags an applied verdict lists (DESIGN.md §5.5): its own clauses by name, another type's qualified
        mine = f"{o.type}."
        warned = [c[len(mine):] if c.startswith(mine) else c for c, mode in outcome[2] if mode == "warn"]
        if list(want["warned"]) != warned:
            return f"expected the request to warn of {want['warned'] or 'nothing'}, and it warns of {warned or 'nothing'}"
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
        check("a whole re-checked when a part is added, the remedy dependent since it reads the parts",
              (e.verdict, e.clause, e.remedy), ("invariant_violated", "few_lines", "dependent"))
    r, s, order, _ = fresh(1, cascade_too=True)
    check("an effect's creation before the cascade to parts", r.attempt(s, "Order", "tangle", order, OPERATOR, {}, {}, {})[3],
          ["Line.add", "Line.void", "Line.void"])
    # a metric read by a transition the request causes counts the log as the request found it (ADR-0084 §6)
    k = copy.deepcopy(kinds)
    k["Line"]["conditions"]["few_packed"] = {"expression": 'metric(Line.transition_counts, transition := "pack") < 2', "remedy": "dependent"}
    k["Line"]["transitions"]["pack"]["guards"]["few_packed"] = "deny"
    r, s = Runner(k, {}), Store(k, {})
    order = r.attempt(s, "Order", "open", None, OPERATOR, {}, {}, {})[1]
    for _ in range(3):
        r.attempt(s, "Order", "add_line", order, OPERATOR, {"qty": 1}, {}, {})
    try:
        got = r.attempt(s, "Order", "ship", order, OPERATOR, {}, {}, {})[3]
    except Refused as e:
        got = (e.verdict, e.clause)
    check("a metric guard on caused transitions, read as the request found the store", got, ["Line.pack", "Line.pack", "Line.pack"])
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
    # slice 4a: a document signed off, edited, sent back, and timed, over its own history
    doc = {"Operator": kinds["Operator"], "Doc": {
        "attributes": {"note": {"type": "string", "optional": True}, "signed": {"type": "event", "optional": True}},
        "derived_attributes": {"rework": {"expression": "count(t in this.transitions where t.returns)"}},
        "states": {"DRAFT": {"category": "live"}, "REVIEW": {"category": "live"}, "DONE": {"category": "closed", "final": True}},
        "conditions": {"fresh": {"expression": "signed is not null and not changed_since([note], signed)", "remedy": "delegable"},
                       "waited": {"expression": "time_in(REVIEW) >= 1 h", "remedy": "temporal"}},
        "transitions": {
            "create": {"kind": "initial", "to": "DRAFT"},
            "edit": {"kind": "internal", "from": ["DRAFT", "REVIEW"], "required_inputs": ["note"]},
            "sign": {"kind": "internal", "from": ["DRAFT", "REVIEW"], "effect": [{"assign": {"location": "signed", "expr": "this_event"}}]},
            "submit": {"kind": "external", "from": ["DRAFT"], "to": "REVIEW"},
            "back": {"kind": "external", "from": ["REVIEW"], "to": "DRAFT"},
            "finish": {"kind": "external", "from": ["REVIEW"], "to": "DONE", "guards": {"fresh": "deny"}},
            "finish_slow": {"kind": "external", "from": ["REVIEW"], "to": "DONE", "guards": {"waited": "deny"}}}}}

    def history(*steps):
        r, s = Runner(doc, {}), Store(doc, {})
        d = r.attempt(s, "Doc", "create", None, OPERATOR, {}, {}, {})[1]
        for st in steps:
            if isinstance(st, int):
                s.now += st
                continue
            name, inputs = st if isinstance(st, tuple) else (st, {})
            try:
                r.attempt(s, "Doc", name, d, OPERATOR, inputs, {}, {})
            except Refused as e:
                return (e.verdict, e.clause), s, d
        return "applied", s, d
    check("a sign-off nothing changed since", history("sign", "submit", "finish")[0], "applied")
    check("a sign-off an edit made stale", history("sign", ("edit", {"note": "x"}), "submit", "finish")[0], ("unsatisfied", "fresh"))
    check("a stale sign-off given again", history("sign", ("edit", {"note": "x"}), "sign", "submit", "finish")[0], "applied")
    _v, s, d = history("submit", "back", "submit")
    check("rework, each return to a state held before", Evaluator(s, "Doc", d).member(d, "rework"), 2)
    check("time in a state over two spells", history("submit", 1800, "back", "submit", 1800, "finish_slow")[0], "applied")
    check("time in a state not yet long enough", history("submit", 1800, "back", "submit", 1200, "finish_slow")[0],
          ("unsatisfied", "waited"))
    _v, s, d = history("submit", 600, "back", 60, "submit")
    check("when it last entered a state", Evaluator(s, "Doc", d).value(flowexpr.parse("entered_at(REVIEW)"), {}) - timestamp(CLOCK_START), 660)
    ok &= metrics_self_test(kinds["Operator"])
    print(f"runner: a small order and its lines, a document over its history, and jobs read by their metrics, each thing "
          f"slices 1 to 4 execute: {'yes' if ok else 'NO'}")
    return ok


def metrics_self_test(operator):
    """Slice 4b: jobs and their inspections read by metrics, as a guard reads them (declaration-syntax.md §6.9)."""
    ok = True

    def check(name, got, want):
        nonlocal ok
        if got != want:
            ok = False
            print(f"  {name}: {got!r}, not {want!r}")
    rate = "count(where r.outcome == Outcome.PASS) * {} / count()"
    job = {
        "attributes": {"engineer": {"reference": "User", "optional": True, "assignee": True},
                       "tags": {"type": "string[]", "optional": True}},
        "observations": {"checks": {"kind": "Check", "attributes": {"outcome": {"type": "Outcome"}}}},
        "states": {"OPEN": {"category": "inbound"}, "WORKING": {"category": "live"}, "DONE": {"category": "closed", "final": True}},
        "conditions": {
            "good": {"expression": "metric(pass_rate, engineer := engineer, over last 30 days) >= 0.900", "remedy": "dependent"},
            "not_busy": {"expression": "metric(Job.open_work.engineer, assignee := inputs.engineer) < 2", "remedy": "dependent"}},
        "transitions": {
            "open": {"kind": "initial", "to": "OPEN", "optional_inputs": ["engineer", "tags"]},
            "assign": {"kind": "internal", "from": ["OPEN", "WORKING"], "required_inputs": ["engineer"], "guards": {"not_busy": "deny"}},
            "start": {"kind": "external", "from": ["OPEN"], "to": "WORKING"},
            "back": {"kind": "external", "from": ["WORKING"], "to": "OPEN"},
            "finish": {"kind": "external", "from": ["WORKING"], "to": "DONE", "guards": {"good": "deny"}}},
        "metrics": {
            "pass_rate": {"source": "checks", "item": "r", "filter": "r.outcome != Outcome.NA", "dimensions": {"engineer": "r.subject.engineer"},
                          "time_dimension": "r.occurred_at", "expression": rate.format("1.000")},
            "pass_share": {"source": "checks", "item": "r", "dimensions": {"engineer": "r.subject.engineer"}, "expression": rate.format("1.00")},
            "passes": {"source": "checks", "item": "r", "filter": "r.outcome == Outcome.PASS", "dimensions": {"engineer": "r.subject.engineer"},
                       "time_dimension": "r.occurred_at", "expression": "count()"},
            "jobs_opened": {"source": "objects", "item": "o", "dimensions": {"engineer": "o.engineer"}, "time_dimension": "o.created_at",
                            "expression": "count()"},
            "time_working": {"measure": "median_time_in_state", "state": "WORKING", "group_by": ["engineer", "month"]},
            "slow_working": {"source": "intervals", "item": "i", "filter": "i.state == WORKING", "expression": "percentile(0.8, i.duration)"},
            "starts": {"measure": "transition_count", "transition": "start", "group_by": ["actor", "month"]},
            "tagged": {"source": "objects", "item": "o", "dimensions": {"tag": "o.tags"}, "expression": "count()"},
            "tags_per_job": {"source": "objects", "item": "o", "expression": "avg(count(t in o.tags))"}}}
    module = {"enumerations": {"Outcome": ["PASS", "FAIL", "NA"]},
              "types": {"User": {"attributes": {"login": {"type": "identity", "actor_kind": "human"}},
                                 "states": {"ACTIVE": {"category": "live"}},
                                 "transitions": {"add": {"kind": "initial", "to": "ACTIVE", "required_inputs": ["login"]}}},
                        "Job": job},
              "metrics": {"passes_per_job": {"input_metrics": {"passes": "Job.passes", "jobs": "Job.jobs_opened"},
                                             "group_by": ["engineer"], "expression": "passes * 1.000 / jobs"}}}
    kinds = {"Operator": operator, **module["types"], **observation_kinds({"jobs": module})}
    enums = module["enumerations"]
    declared, homes = declarations({"jobs": module})

    def world():
        return Runner(kinds, enums), Store(kinds, enums, declared, homes)

    def req(r, s, tn, xn, oid=None, **inputs):
        try:
            return r.attempt(s, tn, xn, oid, OPERATOR, inputs, {o: o for o in s.objects}, {})[1]
        except Refused as e:
            return (e.verdict, e.clause)

    def read(s, oid, text, **env):
        return Evaluator(s, "Job", oid).value(flowexpr.parse(text), env)

    def record(r, s, j, *outcomes):
        for x in outcomes:
            req(r, s, "Check", "record", subject=j, outcome=f"Outcome.{x}")
    day = 86400
    # a rate over a window, bound to the job's engineer; the window holds its start
    r, s = world()
    ana, ben = req(r, s, "User", "add", login="ana"), req(r, s, "User", "add", login="ben")
    j1, j2, j0 = req(r, s, "Job", "open", engineer=ana), req(r, s, "Job", "open", engineer=ben), req(r, s, "Job", "open")
    record(r, s, j1, "FAIL")
    s.now += 30 * day
    record(r, s, j1, "PASS", "PASS", "NA")
    record(r, s, j0, "FAIL")              # a result of a job with no engineer, in a group of its own
    rate30 = "metric(pass_rate, engineer := engineer, over last 30 days)"
    check("a rate bound to the engineer, its window holding a result exactly thirty days old", read(s, j1, rate30), Fraction(667, 1000))
    check("a combined metric, each input windowed by its own time", read(s, j1, "metric(passes_per_job, engineer := engineer, over last 30 days)"), 2)
    req(r, s, "Job", "start", j1)
    check("a guard over a rate below its threshold", req(r, s, "Job", "finish", j1), ("unsatisfied", "good"))
    s.now += 1
    check("the failure a second later out of the window", read(s, j1, rate30), 1)
    check("a combined metric whose divisor has left its window is unknown",
          read(s, j1, "metric(passes_per_job, engineer := engineer, over last 30 days)"), ABSENT)
    check("a guard over a rate at its threshold", req(r, s, "Job", "finish", j1), j1)
    check("a rate over no rows is unknown", read(s, j2, rate30), ABSENT)
    check("a dimension bound to an absent value is unknown", read(s, j0, rate30), ABSENT)
    # rounding half to even, down to an even digit and up from an odd one
    r, s = world()
    cara, dan = req(r, s, "User", "add", login="cara"), req(r, s, "User", "add", login="dan")
    jc, jd = req(r, s, "Job", "open", engineer=cara), req(r, s, "Job", "open", engineer=dan)
    record(r, s, jc, "PASS", *["FAIL"] * 7)
    record(r, s, jd, *["PASS"] * 3, *["FAIL"] * 5)
    check("0.125 rounded half to even", read(s, jc, "metric(pass_share, engineer := engineer)"), Fraction(12, 100))
    check("0.375 rounded half to even", read(s, jd, "metric(pass_share, engineer := engineer)"), Fraction(38, 100))
    # time in a state held by whoever was assigned when the span began; nearest-rank medians; counts; set dimensions
    r, s = world()
    ana, ben = req(r, s, "User", "add", login="ana"), req(r, s, "User", "add", login="ben")
    ja = req(r, s, "Job", "open", engineer=ana, tags=["urgent"])
    for hours in (1, 2, 3, 4):
        req(r, s, "Job", "start", ja)
        s.now += hours * 3600
        req(r, s, "Job", "back", ja)
    jb = req(r, s, "Job", "open", engineer=ben, tags=["urgent", "vip"])
    req(r, s, "Job", "start", jb)
    s.now += 5 * 3600
    req(r, s, "Job", "assign", jb, engineer=ana)
    s.now += 3600
    req(r, s, "Job", "back", jb)
    check("a median, the lower middle of four spans", read(s, ja, "metric(time_working, engineer := engineer)"), 2 * 3600)
    check("a span attributed to whoever held the job when it began", read(s, jb, "metric(time_working, engineer := who)", who=ben), 6 * 3600)
    check("a nearest-rank percentile, the fourth of five", read(s, ja, "metric(slow_working)"), 4 * 3600)
    check("a transition count, its actor bound", read(s, ja, 'metric(starts, actor := "operator")'), 5)
    check("the standard rework, each return to a state held before", read(s, ja, "metric(Job.rework)"), 8)
    jc = req(r, s, "Job", "open", tags=["vip"])
    check("a guard over the standard open work of an assignee", req(r, s, "Job", "assign", jc, engineer=ana), ("unsatisfied", "not_busy"))
    check("the same guard for an assignee with none", req(r, s, "Job", "assign", jc, engineer=ben), jc)
    check("a set dimension, a job under each of its tags", (read(s, ja, 'metric(tagged, tag := "urgent")'), read(s, ja, 'metric(tagged, tag := "vip")')), (2, 2))
    check("an average of integers, to six places", read(s, ja, "metric(tags_per_job)"), Fraction(1333333, 1000000))
    print(f"  metrics: a rate windowed and bound, a combined one, rounding half to even, medians and percentiles, time held "
          f"by who held the job, counts, set dimensions, and the standard ones, as guards read them: {'yes' if ok else 'NO'}")
    return ok

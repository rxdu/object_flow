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
import hashlib
import json
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
# the calendar a time is written in, 0001-01-01 to 9999-12-31T23:59:59Z, in seconds since 1970 (ADR-0190 decision 1)
FIRST_SECOND, LAST_SECOND = -62_135_596_800, 253_402_300_799
CLOCK_START = "2026-01-01T00:00:00Z"            # flow-format.md §11.3
ACTOR_KINDS = ("human", "agent", "service")
OPERATOR = "operator"                           # the alias, and the identity, of the store's operator (ADR-0126)
BUCKETS = {"day", "week", "month", "quarter", "year"}
ROW_AGGREGATES = {"count", "sum", "min", "max", "avg", "median", "percentile"}
AVERAGE_SCALE = 6                               # an average of integers is a decimal to six places (ADR-0139)
# each currency's minor unit, the scale its amounts are rounded to (ISO 4217; declaration-syntax.md §8.3)
MINOR_UNITS = {"USD": 2, "EUR": 2, "GBP": 2, "SGD": 2, "AUD": 2, "CAD": 2, "CHF": 2, "CNY": 2, "HKD": 2, "NZD": 2,
               "MYR": 2, "INR": 2, "THB": 2, "JPY": 0, "KRW": 0, "BHD": 3, "KWD": 3, "OMR": 3, "JOD": 3, "TND": 3}
# the tags of the values a tuple holds that are not sets: a state, an enumeration member, a category, a type named in
# an expression, the markers `inputs` and `actor`, and an event
TAGS = {"state", "enum", "category", "typeref", "marker", "event", "file"}
parsed = functools.lru_cache(maxsize=None)(flowexpr.parse)


class NotRunnable(Exception):
    """A construct this slice of the runner does not execute; the example is reported, not passed."""


class ExampleError(Exception):
    """An example that cannot be what it claims, such as an evaluator with no stubbed answer."""


class Refused(Exception):
    """A refusal as an expectation reads it, its verdict, clause and remedy, and what a caller of the library is told
    beside them where the runner knows it: the object an invariant fails on, and a refused call's step, target and own
    refusal (library-api.md §4), which the Flow Studio's server reports (ADR-0150)."""

    def __init__(self, verdict, clause=None, remedy=None, detail="", object_id=None, object_type=None, step=None,
                 inner=None, unknown=False, inputs=None, parents=None, reasons=None, objects=None, failures=None):
        super().__init__(f"{verdict} {clause or ''} {remedy or ''} {detail}".strip())
        self.verdict, self.clause, self.remedy, self.detail = verdict, clause, remedy, detail
        self.object_id, self.object_type, self.step, self.inner = object_id, object_type, step, inner
        self.unknown = unknown      # its clause was unknown rather than false (§8.2), as the attempt log records
        # the inputs an `invalid_input` names, and the parents a `not_requestable` names, where the runner knows them
        self.inputs, self.parents = inputs, parents
        self.reasons = reasons                  # why each of `inputs` is at fault (ADR-0173 decision 9)
        # the objects the remedy points at, in ascending id order, and an invariant violation's every failure as
        # (object, type, invariant, remedy) (DESIGN.md §5.5, library-api.md §4)
        self.objects, self.failures = list(objects or ()), failures


class OutOfRange(Refused):
    """An arithmetic of present values past its type's bound, or a date moved past the calendar, on the object named:
    a request refuses it value_in_range and a read refuses it so, never answering it as unknown, as the core does
    (ADR-0205 decision 2). A division by zero is unknown, as §8.2 defines it."""

    def __init__(self, object_id=None):
        super().__init__("unsatisfied", "value_in_range", "unreachable_from_here",
                         objects=[object_id] if object_id else [])


def seconds(duration):
    """A duration written as a count and a unit, `2 days`, in seconds."""
    n, unit = str(duration).split()
    return Fraction(int(n)) * SECONDS[unit]


DATE_TEXT = re.compile(r"[0-9]{4}-[0-9]{2}-[0-9]{2}")
TIMESTAMP_TEXT = re.compile(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}(:[0-9]{2})?(Z|\+00:00)")


def timestamp(text):
    t = datetime.datetime.strptime(text.replace("Z", "+0000"), "%Y-%m-%dT%H:%M%z" if text.count(":") == 1 else "%Y-%m-%dT%H:%M:%S%z")
    return Fraction(int((t - EPOCH).total_seconds()))


class Row(dict):
    """A row of an object's own flow data: a span of `intervals` or an event of `transitions` (§6.9). A span also knows
    the position of the event that began it, which is no member of the row."""

    position = 0


def span_row(position, fields):
    """A span begun by the event at a position."""
    row = Row(fields)
    row.position = position
    return row


def attempt_row(store, tn, oid, xn, actor, r, proposal=None):
    """A refusal's row of the attempt log (DESIGN.md §6, library-api.md §5): who asked, as the request named them where
    the store holds no such actor, and their kind, `unknown` then; the type, none where the request named an unknown id;
    the object, none for a refused creation or an unknown id; the transition; the verdict, its clause, the first
    invariant for violated invariants, and its remedy, a refused call's being the call's own, so the causes of one
    clause count together under the transition requested (metric-scenarios.md §12); whether the clause was unknown; and
    the proposal an approval executed, which the row names."""
    marked = store.actor_attr(store.type_of(actor)) if actor in store.objects else None
    clause = r.clause.split(".")[-1] if r.verdict == "call_refused" and r.clause else r.clause
    return {"type": tn, "object": oid, "transition": xn, "verdict": r.verdict, "clause": clause, "remedy": r.remedy,
            "unknown": r.unknown, "enforced": True, "flagged": False,
            "actor_id": store.objects[actor].attrs.get(marked[0], actor) if marked else actor,
            "actor_kind": marked[1] if marked else "unknown", "at": store.now, "declaration_version": store.version,
            "applied_position": None, "proposal": proposal}


class Shared:
    """Declarations every copy of a store reads and none writes, which a request's copy shares rather than copies."""

    def __init__(self, **kw):
        self.__dict__.update(kw)

    def __deepcopy__(self, memo):
        return self


def is_set(v):
    """Whether a value is a set: a tuple, but not one of the tagged tuples a state, an enumeration member or a file
    is. A set of strings whose first is one of the tags would be misread, and no example writes one."""
    return isinstance(v, tuple) and not (len(v) in (2, 3, 4) and v[0] in TAGS)


def set_equal(a, b):
    """Two sets are equal when each element of either equals one of the other, in any order (the values audit's A8)."""
    def eq(x, y):
        return set_equal(x, y) if is_set(x) and is_set(y) else x == y
    return all(any(eq(x, y) for y in b) for x in a) and all(any(eq(x, y) for x in a) for y in b)


def is_set_attribute(spec):
    """A set the object stores: a set of values, or a set of references with no opposite end to compute it from."""
    spec = spec or {}
    return str(spec.get("type", "")).endswith("[]") or (str(spec.get("reference", "")).endswith("[]") and "opposite" not in spec)


def computed_end(kinds, spec):
    """An end of a relationship the other end stores, read from it and given by no write: a set end, or a single end
    whose single opposite is the one marked `stored` (flow-format.md §4.3)."""
    spec = spec or {}
    ref, opposite = str(spec.get("reference", "")), spec.get("opposite")
    if not ref or not opposite:
        return False
    if ref.endswith("[]"):
        return True
    other = ((kinds.get(ref) or {}).get("attributes") or {}).get(opposite) or {}
    return not str(other.get("reference", "")).endswith("[]") and bool(other.get("stored")) and not spec.get("stored")


def at_declared_scale(spec, v, oid=None):
    """A value written to an attribute, as its declared type holds it, as the core's `fit_value` writes it: a number to
    a decimal at its scale, a quotient rounded once half to even (declaration-syntax.md §8.3, F4); a number to an
    integer only where it is whole; text, a boolean or an enumeration's member only to its own type. A value it cannot
    hold refuses the request value_in_range, naming the object (the audit's values W1 to W3, ADR-0205 decision 1)."""
    typ = str((spec or {}).get("type") or "")
    if v is ABSENT or not typ or (spec or {}).get("reference") or typ.endswith("[]"):
        return v
    number = isinstance(v, (int, Fraction)) and not isinstance(v, bool)
    fits = True
    if typ.startswith("decimal("):
        if number:
            return half_even(v, int(typ.rstrip(")").rsplit(",", 1)[1]))
        fits = False
    elif typ in ("int", "counter"):
        fits = number and Fraction(v).denominator == 1
        v = int(v) if fits else v
    elif typ in ("string", "text", "identity", "actor_kind"):
        fits = isinstance(v, str)
    elif typ == "bool":
        fits = isinstance(v, bool)
    elif isinstance(v, tuple) and v[:1] == ("enum",):
        fits = v[1] == typ
    if not fits:
        raise Refused("unsatisfied", "value_in_range", "unreachable_from_here", objects=[oid] if oid else [])
    return v


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


def held_arith_kind(op, a, b):
    """The type the core's evaluator holds `a op b` at (`eval.rs` `combine`), which places a quotient's rounding: numbers
    aligned to the larger scale by a sum, to the sum of the scales by a product and to the dividend's by a quotient, a
    decimal literal widened to a decimal it is combined with that has more places (§8.1, the audit's values A1); an
    amount or a duration by a number keeps its kind, and a quotient of two like ones is a decimal of six places. Where
    `arith_kind` gives a metric's type (ADR-0128), which aligns no sum of two scales."""
    number = ("int", "decimal", "literal")

    def places(k, other):
        if k[0] == "literal":
            return max(k[1], other[1]) if other[0] == "decimal" else k[1]
        return k[1] if k[0] == "decimal" else 0
    if a[0] in number and b[0] in number:
        if a[0] == b[0] == "int":
            return ("int",)
        if op in ("+", "-"):
            return ("decimal", max(places(a, b), places(b, a)))
        if op == "*":
            return ("decimal", places(a, b) + places(b, a))
        return ("decimal", places(a, b)) if a[0] != "int" else ("other",)   # an int by a decimal is not run
    if op == "/" and a == b and a[0] in ("money", "duration"):
        return ("decimal", 6)
    if op in ("*", "/") and a[0] in ("money", "duration") and b[0] in number:
        return a
    if op == "*" and b[0] in ("money", "duration") and a[0] in number:
        return b
    return arith_kind(op, a, b)


def is_date(v):
    return isinstance(v, datetime.date) and not isinstance(v, datetime.datetime)


def instant(v):
    """A date placed at the start of its day, UTC, where a window or a bucket compares it with an instant (ADR-0145);
    any other value as it is."""
    return Fraction((v - EPOCH.date()).days * 86400) if is_date(v) else v


def day_of(ts):
    """The UTC calendar day a time falls on, as a date (ADR-0145)."""
    return (EPOCH + datetime.timedelta(seconds=math.floor(ts))).date()


def bucket(unit, ts):
    """A timestamp's UTC calendar bucket, the week being the ISO 8601 week that begins on Monday (ADR-0106); a date's
    own, since the start of its day, UTC, is in it (ADR-0145)."""
    t = EPOCH + datetime.timedelta(seconds=math.floor(instant(ts)))
    if unit == "week":
        year, week, _ = t.isocalendar()
        return f"{year}-W{week:02d}"
    return {"day": t.strftime("%Y-%m-%d"), "month": t.strftime("%Y-%m"), "quarter": f"{t.year}-Q{(t.month - 1) // 3 + 1}",
            "year": str(t.year)}[unit]


def next_boundary(unit, ts):
    """The start of the UTC calendar bucket after the one a timestamp is in, a week beginning on Monday (ADR-0106)."""
    t = EPOCH + datetime.timedelta(seconds=math.floor(ts))
    day = datetime.datetime(t.year, t.month, t.day, tzinfo=datetime.timezone.utc)
    if unit == "day":
        n = day + datetime.timedelta(days=1)
    elif unit == "week":
        n = day - datetime.timedelta(days=t.weekday()) + datetime.timedelta(days=7)
    elif unit == "month":
        n = datetime.datetime(t.year + (t.month == 12), t.month % 12 + 1, 1, tzinfo=datetime.timezone.utc)
    elif unit == "quarter":
        m = (t.month - 1) // 3 * 3 + 4
        n = datetime.datetime(t.year + (m > 12), (m - 1) % 12 + 1, 1, tzinfo=datetime.timezone.utc)
    else:
        n = datetime.datetime(t.year + 1, 1, 1, tzinfo=datetime.timezone.utc)
    return Fraction(int((n - EPOCH).total_seconds()))


def pieces(span, unit):
    """A span cut at every bucket boundary it crosses, each piece with its bucket, its bounds and its duration; a span
    with no duration is one piece, in the bucket it began, with none (ADR-0142)."""
    start, d = span["entered_at"], span["duration"]
    if d is ABSENT:
        return [Row(span, period=bucket(unit, start), piece_start=start, piece_end=ABSENT, duration=ABSENT)]
    # a member's span counts only the time its object was open, so its end is its own and a piece's duration the open
    # time within it (ADR-0148)
    end, closed = (span["_end"], span["_closed"]) if "_end" in span else (start + d, ())
    out, at = [], start
    while True:
        cut = min(next_boundary(unit, at), end)
        shut = sum(max(Fraction(0), min(cut, b) - max(at, a)) for a, b in closed)
        out.append(Row(span, period=bucket(unit, at), piece_start=at, piece_end=cut, duration=cut - at - shut))
        if cut >= end:
            return out
        at = cut


def clipped(piece, low, high):
    """The part of a piece between two times, both included, or None where it has none there (ADR-0142 decision 4)."""
    if piece["duration"] is ABSENT or piece["duration"] == 0:
        return piece if low <= piece["piece_start"] <= high else None
    a, b = max(piece["piece_start"], low), min(piece["piece_end"], high)
    return Row(piece, piece_start=a, piece_end=b, duration=b - a) if b > a else None


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
        # the transition's events by name, with the standard metrics' exclusions (ADR-0146)
        along = f't.transition == "{spec["transition"]}" and not t.imported and not t.migrated and not t.redacted'
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
        # each recorded change adds one, its creation's first, so a version is a point in the object's order of events
        # (ADR-0023 rule 4, storage-schema.md §2's object_seq)
        self.version = 0
        self.id_order = 0                       # creation order, which ascending id order is (§5.1)
        self.admissions = set()                 # invariants whose violation an admission lets stand (ADR-0054)
        self.spans = {}                         # tracked member -> its spans, each a Row (DESIGN.md §7)
        self.writes = {}                        # attribute or part relationship -> the event that last wrote it
        self.erased = False                     # its own erasure is recorded (DESIGN.md §8)
        self.created_by_kind = "human"          # the kind of actor that created it; the operator's, until a request sets it


# the built-in observation kind every type carries, and the reference end naming its subject, which may be of any type
LABEL = "label"
ANY_TYPE = "*"


def label_kind():
    """A label as the kind it is (declaration-syntax.md §6.8): a name, normalised to lower case and trimmed; an optional
    note, personal, which its `forget` redacts; and the subject's state when it was applied, which the recording writes.
    Its occurred time is its recorded time, so it has no recording bound, and a request supplying one is refused."""
    return {"attributes": {"name": {"type": "string"}, "note": {"type": "string", "optional": True, "personal": True},
                           "subject_state": {"type": "string"}, "subject": {"reference": ANY_TYPE},
                           "corrects": {"reference": LABEL, "optional": True},
                           "recorded_at": {"type": "timestamp"}, "occurred_at": {"type": "timestamp"}},
            "states": {"RECORDED": {"category": "closed", "final": True}},
            "transitions": {"record": {"kind": "initial", "to": "RECORDED", "required_inputs": ["subject", "name"],
                                       "optional_inputs": ["corrects", "note"]},
                            "forget": {"kind": "erasure", "inputs": {"reason": {"type": "string"}}}}}


# the built-in type a request filed for approval becomes (DESIGN.md §9)
PROPOSAL = "Proposal"


def proposal_kind():
    """A proposal as the built-in type it is (DESIGN.md §9, D521): the request it holds, who filed it and when, and its
    states, which only the built-in verbs move: `propose` files it, `approve`, `reject` and `withdraw` decide it, and
    an erasure of its target invalidates it."""
    def a(t, optional=False):
        return {"type": t, **({"optional": True} if optional else {})}
    return {"attributes": {"target": a("string", True), "target_type": a("string"), "transition": a("string"),
                           "inputs": a("string"), "proposer": a("string"), "context": a("string", True),
                           "occurred": a("timestamp", True), "filed_at": a("timestamp"),
                           "expires_at": a("timestamp", True)},
            "states": {"PENDING": {"category": "live"},
                       **{st: {"category": "closed", "final": True}
                          for st in ("EXECUTED", "REJECTED", "WITHDRAWN", "INVALIDATED")}}}


# the built-in type a change to the declaration is (DESIGN.md §9), whose requests flowpublish.py decides (ADR-0166)
DECLARATION_CHANGE = "DeclarationChange"


def change_kind():
    """A change to the declaration as the built-in type it is (DESIGN.md §9): its source, each file by the name it
    declares, held as JSON text; where the source came from and its digest; the version it was drafted against; its
    report and the evidence it cites, as JSON text; who drafted and approved it, with their kinds; and its states,
    which only its own requests and the publish move (ADR-0166)."""
    def a(t, optional=False):
        return {"type": t, **({"optional": True} if optional else {})}
    return {"attributes": {"source": a("string"), "source_ref": a("string"), "source_digest": a("string"),
                           "base_version": a("int"), "report": a("string", True), "evidence": a("string", True),
                           "drafted_by": a("string"), "drafted_by_kind": a("string"),
                           "approved_by": a("string", True), "approved_by_kind": a("string", True)},
            "states": {"DRAFTED": {"category": "live"}, "SUBMITTED": {"category": "live"},
                       **{st: {"category": "closed", "final": True}
                          for st in ("PUBLISHED", "REJECTED", "WITHDRAWN", "SUPERSEDED")}}}


def with_labels(kinds):
    """The types with the built-in label kind, the built-in proposal and the built-in change to the declaration, as
    every store is created with them (ADR-0126, DESIGN.md §9)."""
    kinds = kinds if LABEL in kinds else {**kinds, LABEL: label_kind()}
    kinds = kinds if PROPOSAL in kinds else {**kinds, PROPOSAL: proposal_kind()}
    return kinds if DECLARATION_CHANGE in kinds else {**kinds, DECLARATION_CHANGE: change_kind()}


def proposal_event(store, key, verb, before, after, actor, position=None):
    """A proposal's own event: filed, decided or invalidated, by the actor given, the next position or the one
    reserved for it; the object moves to its new state and version (DESIGN.md §9)."""
    if position is None:
        store.position += 1
        position = store.position
    marked = store.actor_attr(store.type_of(actor)) if actor in store.objects else None
    if marked is None:
        # named by its identity, in any case of its ASCII letters: the actor holding it, for its kind
        holder = next((o for o in store.objects.values()
                       if (store.actor_attr(o.type) or (None,))[0] is not None
                       and isinstance(o.attrs.get(store.actor_attr(o.type)[0]), str)
                       and ascii_fold(o.attrs[store.actor_attr(o.type)[0]]) == ascii_fold(actor)), None)
        marked = store.actor_attr(holder.type) if holder is not None else None
        if holder is not None:
            actor = holder.attrs[marked[0]]
    elif isinstance(store.objects[actor].attrs.get(marked[0]), str):
        actor = store.objects[actor].attrs[marked[0]]
    p = store.objects[key]
    p.state = after
    p.version += 1
    store.events.append({"position": position, "object": key, "type": PROPOSAL, "transition": verb,
                         "from_state": ("state", PROPOSAL, before) if before else ABSENT,
                         "to_state": ("state", PROPOSAL, after), "completes": bool(before), "returns": False,
                         "occurred_at": store.now, "recorded_at": store.now, "actor_id": actor,
                         "actor_kind": marked[1] if marked else "unknown", "asserted": False, "overrides": False,
                         "imported": False, "migrated": False, "redacted": False, "reason": ABSENT,
                         "declaration_version": store.version, "held": {}})
    store.events.sort(key=lambda e: e["position"])
    return position


class Store:
    """The objects of one example, built from an empty store holding version 0 and its operator."""

    def __init__(self, kinds, enums, metrics=None, homes=None):
        self.kinds, self.enums = with_labels(kinds), enums
        # the installed declaration version: an example's store, like a sandbox's load, starts at the version under
        # test, 1, over version 0's built-ins and operator (ADR-0155 decision 4); a publish moves it on (ADR-0166)
        self.version = 1
        # the installed version's source, each file by the name it declares, which a change is checked against
        self.source = {}
        # module -> each metric its types and it declare, as (type or None, declaration); type -> its module
        self.decl = Shared(metrics=metrics or {}, homes=homes or {})
        self.now = timestamp(CLOCK_START)
        self.objects, self.counter = {}, 0
        self.sequences = {}                     # (sequence, scope value) -> the last number minted
        self.events, self.position = [], 0      # the log, and the last position allocated (DESIGN.md §7)
        self.proposals = {}                     # proposal id -> the request it holds, its proposer and its state (§9)
        self.attempts = []                      # the attempt log: each request that did not apply, each audit's
                                                # would-be refusal and each raised flag (DESIGN.md §6, §7)
        # the operator, created with version 0 by its own act, the log's first event (storage-schema.md §6, ADR-0134)
        op = Obj(OPERATOR, "Operator", "ACTIVE", self.now)
        op.attrs["identity"] = OPERATOR
        self.objects[OPERATOR] = op
        self.position = 1
        op.version = 1
        op.writes["identity"] = self.position
        self.events.append({"position": self.position, "object": OPERATOR, "type": "Operator", "transition": "install",
                            "from_state": ABSENT, "to_state": ("state", "Operator", "ACTIVE"), "completes": False,
                            "returns": False, "occurred_at": self.now, "recorded_at": self.now, "actor_id": OPERATOR,
                            "actor_kind": "human", "asserted": False, "overrides": False, "imported": False,
                            "migrated": False, "redacted": False, "reason": ABSENT, "declaration_version": 0, "held": {}})

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

    def superseded(self, o):
        return bool(((self.kinds.get(o.type) or {}).get("states") or {}).get(o.state, {}).get("superseding"))

    def find_metric(self, ref, tn=None, module=None):
        """What a metric reference names (declaration-syntax.md §6.9, §6.11): a bare name, a metric of its module, its
        types' or its own; `<Type>.<name>`, a metric declared under that type or a standard one; `<Type>.<name>.<r>`, a
        standard assignment metric over the assignee reference r. Returns (its type or None, the declaration, the
        module)."""
        declared, homes = self.decl.metrics, self.decl.homes
        if ref[0] == "name":
            mod = module if module is not None else homes.get(tn)
            if mod is None and tn is None:
                # a reader's bare name: the one module that declares it (library-api.md §5)
                hits = [m for m, ms in declared.items() if ref[1] in ms]
                mod = hits[0] if len(hits) == 1 else None
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
        self.failed_arithmetic = False          # an arithmetic of present values with none, as the core's (ADR-0185)
        self.t = store.kinds.get(tn) or {}
        self.naming = None                      # where the objects a refusing clause names are noted (DESIGN.md §5.5)

    def actor_named(self, identity):
        """How a row names an actor its record names by identity: by the actor's object id where an actor type marks
        its identity personal, as a metric reads no personal value, by the identity where none does, as the core's
        World::actor_named (the audit's personal data R8)."""
        if not isinstance(identity, str):
            return identity
        if getattr(self, "_actors", None) is None:
            personal = any(isinstance(sp, dict) and sp.get("actor_kind") and sp.get("personal")
                           for k in self.s.kinds.values() for sp in ((k or {}).get("attributes") or {}).values())
            self._actors = {}
            if personal:
                for o in sorted(self.s.objects.values(), key=lambda o: o.id_order):
                    marked = self.s.actor_attr(o.type)
                    held = o.attrs.get(marked[0]) if marked else None
                    if isinstance(held, str):
                        self._actors[held.lower()] = o.id
        return self._actors.get(identity.lower(), identity)

    def noting(self, exclude):
        """Noting the objects the clause names, but the object it is evaluated on: every object a reference evaluates
        to, both sides of `and`, `or` and `implies` and the derived attributes it reads included; of a collection,
        the elements the outermost aggregate's `where` selects, a nested aggregate and a name it binds adding none."""
        self.naming = {"objects": set(), "depth": 0, "exclude": exclude}
        return self

    def note(self, v):
        n = self.naming
        if n is not None and n["depth"] == 0 and isinstance(v, str) and v in self.s.objects and v != n["exclude"]:
            n["objects"].add(v)
        return v

    def named(self):
        """The objects noted, in ascending id order, which is creation order (DESIGN.md §5.1)."""
        return sorted((self.naming or {}).get("objects", ()), key=lambda i: self.s.objects[i].id_order)

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
                or n in (self.t.get("observations") or {}) or n in ("id", "open", "created_at", "created_by_kind", "recorded_from", "declaration_version"):
            return self.member(self.this, n) if self.this is not None else ABSENT
        if n in (self.t.get("states") or {}):
            return ("state", self.tn, n)
        if any(n == v.get("category") for v in (self.t.get("states") or {}).values()) or n == "closed":
            return ("category", n)
        if n in ACTOR_KINDS:
            return n                            # a kind of actor, the value actor.kind and a row's .actor_kind hold (ADR-0145)
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
            if m == "actor_id" and m in base:
                return self.actor_named(base[m])
            if m in base:
                return base[m]
            if m == "corrected_by":
                return ABSENT                   # a span no correction relabelled (ADR-0146)
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
        if m == "attempts":
            return tuple(Row(a) for a in self.s.attempts if a["object"] == o.id)
        if m == "labels":
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
            # the version in force when the object was last written: its last event's (storage-schema.md §2)
            written = [e for e in self.s.events if e["object"] == o.id]
            return written[-1]["declaration_version"] if written else self.s.version
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
            derived = Evaluator(self.s, o.type, o.id, stubs=self.stubs)
            derived.naming = self.naming        # what it reads, the clause reads (DESIGN.md §5.5)
            return derived.value(flowexpr.parse(t["derived_attributes"][m]["expression"]), {})
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
        if member == "state":
            return tuple(Row(sp, duration=self.duration(o, sp)) for sp in o.spans[member])
        return tuple(Row(sp, duration=self.duration(o, sp), _end=sp["left_at"] if sp["left_at"] is not ABSENT else self.s.now,
                         _closed=self.closed_periods(o)) for sp in o.spans[member])

    def closed_periods(self, o):
        """The stretches, by occurred time, in which an object was in a closed or terminal state."""
        states = (self.s.kinds.get(o.type) or {}).get("states") or {}
        finished = lambda name: (states.get(name) or {}).get("category") == "closed" or (states.get(name) or {}).get("final")
        return tuple((sp["entered_at"], sp["left_at"] if sp["left_at"] is not ABSENT else self.s.now)
                     for sp in o.spans.get("state", []) if finished(sp["state"][2]))

    def duration(self, o, sp):
        """A span's exit less its entry, a current one running to now. A member's span counts only the time its object
        was open, and has none where it had no open time; a state's span keeps its exit less its entry, but the span of
        the closed or terminal state the object is in now has none (§6.9, ADR-0101, ADR-0146, ADR-0148)."""
        if "state" not in sp:
            end = sp["left_at"] if sp["left_at"] is not ABSENT else self.s.now
            length = end - sp["entered_at"]
            shut = sum(max(Fraction(0), min(end, b) - max(sp["entered_at"], a)) for a, b in self.closed_periods(o))
            return length - shut if length - shut > 0 or length == 0 else ABSENT
        if sp["left_at"] is not ABSENT:
            return sp["left_at"] - sp["entered_at"]
        states = (self.s.kinds.get(o.type) or {}).get("states") or {}
        finished = lambda name: (states.get(name) or {}).get("category") == "closed" or (states.get(name) or {}).get("final")
        if not finished(o.state):
            return self.s.now - sp["entered_at"]
        # the unbroken stretch of closed or terminal states the object is in now begins where its earliest span does
        entry = o.entered.get(o.state, o.created_at)
        for past in reversed(o.spans.get("state", [])):
            if not finished(past["state"][2]):
                break
            entry = past["entered_at"]
        return entry - sp["entered_at"] if sp["entered_at"] < entry else ABSENT

    def time_in(self, o, state):
        """Real elapsed time: every span in the state, a current one up to now, whatever state the object is in; the
        closed stretch is a rule of a span's .duration in a metric, not of time_in (declaration-syntax.md §8.3, ADR-0147)."""
        if state not in ((self.s.kinds.get(o.type) or {}).get("states") or {}):
            # of what is no state it is not run, as the core does not run it, where it was 0 s (the audit's values G2)
            raise NotRunnable(f"`time_in({state})`, which names no state of {o.type}")
        spans = [sp for sp in o.spans.get("state", []) if sp["state"] == ("state", o.type, state)]
        return sum(((sp["left_at"] if sp["left_at"] is not ABSENT else self.s.now) - sp["entered_at"] for sp in spans), Fraction(0))

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
            # a duration is whole seconds, and a literal of a fraction of one is not run, as the core does not run it
            # (the audit's values L2)
            v = Fraction(str(e[1])) * SECONDS[e[2]]
            if v.denominator != 1:
                raise NotRunnable(f"the duration {e[1]} {e[2]}, which is no whole number of seconds")
            return v
        if k == "money":
            # an amount with more places than its currency's minor unit is not run, as the core does not run it (L1)
            v, places = Fraction(e[2]), MINOR_UNITS.get(e[1])
            if places is None or (v * 10 ** places).denominator != 1:
                raise NotRunnable(f"an amount in {e[1]} with more places than its minor unit")
            return v
        if k == "name":
            v = self.name(e[1], env)
            return v if e[1] in env else self.note(v)       # a name an aggregate binds is no reference
        if k == "path":
            return self.note(self.member(self.value(e[1], env), e[2]))
        if k == "set":
            # a set: an element written twice is held once, and one that is unknown makes it unknown (§8.2), as the
            # core's (the values audit's A8)
            out = []
            for v in (self.value(i, env) for i in e[1]):
                if v is ABSENT:
                    return ABSENT
                if not any(set_equal(v, o) if is_set(v) and is_set(o) else v == o for o in out):
                    out.append(v)
            return tuple(out)
        if k == "not":
            v = truth(self.value(e[1], env))
            return ABSENT if v is ABSENT else not v
        if k == "neg":
            v = self.value(e[1], env)
            return ABSENT if v is ABSENT else self.bounded(v, 0, -v) if isinstance(v, int) else -v
        if k == "isnull":
            v = self.value(e[1], env)
            return (v is ABSENT) != e[2]
        if k == "in":
            left, right = self.value(e[1], env), e[2]
            if left is ABSENT:
                return ABSENT
            if right[0] == "name" and right[1] in self.s.enums and right[1] not in env:
                return isinstance(left, tuple) and left[0] == "enum" and left[1] == right[1]
            if right[0] == "set":
                # `x in {a, b}` is `x == a or x == b`: true where a known element equals it, unknown where none does
                # and one is unknown (§8.2), as the core's (the values audit's A8)
                items = [self.value(i, env) for i in right[1]]
                if any(i is not ABSENT and i == left for i in items):
                    return True
                return ABSENT if any(i is ABSENT for i in items) else False
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
                if self.naming is not None:
                    # the side that does not decide names its objects too, evaluated in full, as the core evaluates
                    # it for the record
                    try:
                        self.value(e[3], env)
                    except (NotRunnable, Refused):
                        pass
                return a if op != "implies" else True
            b = truth(self.value(e[3], env))
            if op == "and":
                return False if b is False else (ABSENT if ABSENT in (a, b) else True)
            return True if b is True else (ABSENT if ABSENT in (a, b) else False)
        r = self.combine(op, self.value(e[2], env), self.value(e[3], env))
        if isinstance(r, Fraction) and (op in ("*", "/") and r.denominator != 1 or abs(r) > 2 ** 63 - 1):
            r = self.as_held(e, env, r)
        return r

    def as_held(self, e, env, r):
        """A quotient, or a product of an amount or a duration, rounded half to even where the core's evaluator rounds
        it (declaration-syntax.md §8.3): a decimal quotient at its dividend's scale, an amount at its currency's minor
        unit, a duration at whole seconds; a product of numbers is exact. Where it rounded only a value written to an
        attribute, and a derived quotient that did not terminate failed its read (the audit's values R1). Then a value
        past what the core holds is out of range: a duration or a time past 64 bits of seconds, a decimal or an amount
        past 128 bits of units or 38 places, where the runner answered it (R5, ADR-0205 decision 2)."""
        try:
            kinds = self.kinds_here(env)
            k = held_arith_kind(e[1], self.held_kind(e[2], kinds), self.held_kind(e[3], kinds))
        except NotRunnable:
            return r
        places = None
        if k[0] == "money" and k[1] in MINOR_UNITS:
            places = MINOR_UNITS[k[1]]
            r = half_even(r, places)
        elif k[0] in ("duration", "timestamp", "int"):
            r = half_even(r, 0) if k[0] == "duration" else r
            if abs(r) > 2 ** 63 - 1:
                raise OutOfRange(self.this)
        elif k[0] == "decimal":
            places = k[1]
            r = half_even(r, places) if e[1] == "/" else r
        if places is not None and (places > 38 or abs(r) * 10 ** places > 2 ** 127 - 1):
            raise OutOfRange(self.this)
        return r

    def written(self, e, env):
        """A value written to a decimal attribute: a quotient divided once, exactly, for the write to round at its
        target's scale, as the core's `quotient_at` (§8.3, ADR-0205 decision 1); any other value as `value` gives it."""
        if e[0] == "bin" and e[1] == "/":
            return self.combine("/", self.value(e[2], env), self.value(e[3], env))
        return self.value(e, env)

    def kinds_here(self, env):
        """The types `kind` reads the names of an evaluation at: `this`, an object a name holds, a row of flow data."""
        kinds = {"this": ("object", self.tn)} if self.tn else {}
        for n, v in env.items():
            if isinstance(v, str) and v in self.s.objects:
                kinds[n] = ("object", self.s.objects[v].type)
            elif isinstance(v, Row):
                kinds[n] = ("interval" if "entered_at" in v else "transition", None)
            else:
                kinds[n] = ("other",)
        return kinds

    def held_kind(self, e, kinds):
        """The type the core's evaluator holds an expression's value at: `kind`, except that arithmetic aligns as it is
        evaluated (`held_arith_kind`)."""
        if e[0] == "bin" and e[1] in ("+", "-", "*", "/"):
            return held_arith_kind(e[1], self.held_kind(e[2], kinds), self.held_kind(e[3], kinds))
        if e[0] == "neg":
            return self.held_kind(e[1], kinds)
        if e[0] == "if":
            return self.held_kind(e[2], kinds)
        return self.kind(e, kinds)

    def combine(self, op, a, b):
        """A comparison or an arithmetic operator over two values (§8.2, §8.3)."""
        if a is ABSENT or b is ABSENT:
            return ABSENT
        if is_date(a) or is_date(b):
            try:
                return self.dates(op, a, b)
            except OverflowError:
                # a date moved past the calendar is out of range, as the core's is (ADR-0190 decision 1, ADR-0205)
                raise OutOfRange(self.this)
        if op in ("==", "!=") and is_set(a) and is_set(b):
            return set_equal(a, b) == (op == "==")
        if op == "==":
            return a == b
        if op == "!=":
            return a != b
        if op in ("<", "<=", ">", ">="):
            return {"<": a < b, "<=": a <= b, ">": a > b, ">=": a >= b}[op]
        if op in ("+", "-", "*"):
            r = a + b if op == "+" else a - b if op == "-" else a * b
            return self.bounded(a, b, r)
        if op == "/":
            if b == 0:
                self.failed_arithmetic = True
                return ABSENT
            if isinstance(a, int) and isinstance(b, int):
                return self.bounded(a, b, int(Fraction(a, b)))   # int / int discards the remainder (§8.3)
            return Fraction(a) / Fraction(b)
        raise NotRunnable(f"the operator {op}")

    def bounded(self, a, b, r):
        """An integer result within 64 bits, as the core's; past them it is out of range, which a request and a read
        refuse value_in_range, never unknown (ADR-0185, ADR-0205 decision 2)."""
        ints = all(isinstance(x, int) and not isinstance(x, bool) for x in (a, b))
        if ints and not -2 ** 63 <= r <= 2 ** 63 - 1:
            raise OutOfRange(self.this)
        return r

    @staticmethod
    def dates(op, a, b):
        """Dates compare as days; their difference is a duration of whole days, and a date plus or minus one is a date
        (ADR-0145)."""
        if is_date(a) and is_date(b):
            if op == "-":
                return Fraction((a - b).days * 86400)
            if op in ("==", "!=", "<", "<=", ">", ">="):
                return {"==": a == b, "!=": a != b, "<": a < b, "<=": a <= b, ">": a > b, ">=": a >= b}[op]
        if is_date(a) and op in ("+", "-") and isinstance(b, Fraction):
            if b % 86400:
                # a date moved by part of a day has no value, as one moved past the calendar has none, where it was not
                # run (fifteenth production review)
                return ABSENT
            return a + datetime.timedelta(days=int(b / 86400) * (1 if op == "+" else -1))
        raise NotRunnable(f"a date {op} a value that is not a date or a duration of days")

    def aggregate(self, e, env):
        _, kind, distinct, var, coll, where, body = e
        if var is None:
            raise NotRunnable(f"a metric's own `{kind}`")
        items = self.value(coll, env)
        if items is ABSENT:
            return ABSENT
        if isinstance(items, tuple) and items[:1] == ("typeref",) and items[1] in self.s.kinds:
            # a type scan: every object of the type and its family, in ascending id order (declaration-syntax.md §8)
            items = tuple(o.id for o in sorted(self.s.objects.values(), key=lambda o: o.id_order) if self.s.is_a(o.type, items[1]))
        if not isinstance(items, tuple):
            raise NotRunnable("an aggregate over a value that is not a collection")
        if self.naming is None:
            return self.aggregated(kind, distinct, var, items, where, body, env)
        # within it, what an element's where and body read names nothing; the outermost names what its where selects
        self.naming["depth"] += 1
        try:
            chosen = []
            v = self.aggregated(kind, distinct, var, items, where, body, env, chosen)
        finally:
            self.naming["depth"] -= 1
        for it in chosen:
            self.note(it)
        return v

    def aggregated(self, kind, distinct, var, items, where, body, env, chosen=None):
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
                if chosen is not None:
                    chosen.append(it)
        # the body of `any`, `none` and a count of elements is a condition each selected element meets too, as its
        # where is, as the core reads it (§8.3, the audit's values G1)
        if body is not None and (kind in ("any", "none") or (kind == "count" and not distinct)):
            met = []
            for inner in selected:
                b = truth(self.value(body, inner))
                if b is ABSENT:
                    if not self.filter_mode:
                        undecided = True
                elif b:
                    met.append(inner)
            if chosen is not None:
                kept = [i[var] for i in met]
                chosen[:] = [c for c in chosen if c in kept]
            selected = met
            if kind == "count":
                return ABSENT if undecided else len(selected)
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
        # an evaluator's function, named on a base that is no binder, no `this` and no attribute: only it is answered
        # by the stubs, so a flow function on a row or an object, `i.held(r)` or `this.intervals(r)`, is computed
        # even where every evaluator is left unanswered (D534)
        evaluator = fn[0] == "path" and fn[1][0] == "name" and fn[1][1] not in env and fn[1][1] != "this" \
            and fn[1][1] not in (self.t.get("attributes") or {})
        if evaluator:
            # an evaluator is asked nothing where an argument is absent: its guard is unknown, refused with the condition's
            # own remedy, not as an unanswered call (ADR-0146)
            if any(self.value(a, env) is ABSENT for a in args):
                return ABSENT
            if f"{fn[1][1]}.{fn[2]}" in self.stubs:
                answer = self.stubs[f"{fn[1][1]}.{fn[2]}"]
                if answer == "unanswered":
                    self.unanswered = True
                    return ABSENT
                return answer == "satisfied"
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
        if fn == ("name", "date") and len(args) == 1:
            v = self.value(args[0], env)            # the UTC calendar day of an instant (ADR-0145)
            return v if v is ABSENT or is_date(v) else (EPOCH + datetime.timedelta(seconds=math.floor(v))).date()
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
        if getattr(self.stubs, "metrics", True) is False:
            self.unanswered = True              # a dry run consults no metric, as it asks no evaluator (D537)
            return ABSENT
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

    def read_metric(self, owner, spec, module, bound, window, exact=False):
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
                values[n], kinds[n] = self.read_metric(o2, s2, m2, {d: v for d, v in bound.items() if d in mine}, window, exact=True)
            tree = parsed(spec["expression"])
            kind = self.kind(tree, kinds)
            v = self.value(tree, values)
            return (v if exact else rounded(v, kind)), kind
        picked, f, item, rowkind, _dims, row = self.selected(owner, spec, bound, window)
        tree = parsed(f["expression"])
        kind = row.kind(tree, {item: rowkind})
        v = row.over_rows(tree, picked, item, rowkind)
        return (v if exact else rounded(v, kind)), kind

    def selected(self, owner, spec, bound, window, filt=None):
        """The rows a read of a formula selects: its source's, cut by a split and clipped by the window, then those its
        filter, a reader's filter, the window and every bound dimension hold for (§6.9, ADR-0114, ADR-0139, ADR-0142).
        Returns them, the formula, its item, what a row is, the expression of each dimension, and the evaluator over
        its rows."""
        s = self.s
        f = formula(s.kinds, s.enums, owner, spec)
        rows, rowkind, by_kind = self.metric_rows(owner, f["source"])
        split = f.get("split_by")
        if split:
            if rowkind[0] != "interval":
                raise NotRunnable(f"a metric over {f['source']} split by {split}, which splits spans alone")
            # each span cut at the boundaries of the buckets it covers, and a window clips the pieces (ADR-0142)
            rows = [p for r in rows for p in pieces(r, split)]
            if window is not None:
                rows = [p for p in (clipped(r, s.now - window, s.now) for r in rows) if p is not None]
        item = f["item"]
        row = Evaluator(s, owner, stubs=self.stubs, filter_mode=True)
        dims = {"version": f"{item}.declaration_version", "actor_kind": f"{item}.{by_kind}"}
        dims.update(f.get("dimensions") or {})
        for d in bound:
            if d not in dims:
                raise NotRunnable(f"a read binding {d}, which the metric does not have")
        if window is not None and not f.get("time_dimension") and not split:
            raise NotRunnable("a window on a metric with no time dimension")
        where = parsed(f["filter"]) if f.get("filter") else None
        theirs = parsed(filt) if filt else None
        stamp = parsed(f["time_dimension"]) if window is not None and not split else None
        picked = []
        for r in rows:
            env = {item: r}
            if where is not None and truth(row.value(where, env)) is not True:
                continue                    # a filter that is unknown selects nothing (§8.2)
            if theirs is not None and truth(row.value(theirs, env)) is not True:
                continue                    # a reader's filter narrows the rows likewise (ADR-0114)
            if stamp is not None:
                at = instant(row.value(stamp, env))
                if at is ABSENT or not s.now - window <= at <= s.now:
                    continue                # the window holds its start, and a row with no time is in none (ADR-0139)
            if all(v in got if is_set(got) else got == v
                   for d, v in bound.items() for got in (row.reach(parsed(dims[d]), env),)):
                picked.append(r)
        return picked, f, item, rowkind, dims, row

    # ── a metric, as a reader reads it (library-api.md §5, declaration-syntax.md §6.9) ──────────────
    def read_page(self, name, bind=None, keep=None, over=None, filt=None):
        """A reader's read of a metric: its rows those a guard's read would select, narrowed by a filter over its binder,
        grouped by the dimensions kept, every declared one unless `keep` names them, the rest aggregated over, a row per
        group in their order; each declared flag on each declared group; and a refusal, a value, where the read cannot be
        answered (ADR-0098, ADR-0101, ADR-0114, ADR-0124, ADR-0127, ADR-0146, ADR-0147, ADR-0148)."""
        s, bind = self.s, dict(bind or {})

        def refused(why):
            return {"rows": [], "flags": [], "history_complete": True, "refused": why}
        try:
            owner, spec, module = s.find_metric(parsed(name))
        except NotRunnable:
            return refused("unknown_metric")
        combined = "input_metrics" in spec
        declared = list(spec.get("group_by") or []) if combined else list(formula(s.kinds, s.enums, owner, spec).get("dimensions") or {})
        keep = declared if keep is None else list(keep)
        dims = s.metric_dimensions(owner, spec)
        if any(d not in dims for d in keep + list(bind)):
            return refused("unknown_dimension")
        if filt is not None and combined:
            return refused("filter_on_combined")
        # a filter reads the rows through the metric's item: a name it does not bind is a filter the read cannot
        # apply, refused rather than answered with no rows (D569)
        if filt is not None:
            try:
                tree = parsed(filt)
            except flowexpr.ParseError:
                return refused("malformed_filter")
            item = formula(s.kinds, s.enums, owner, spec).get("item")
            if not reads_only_its_rows(tree, {item}, s.kinds, s.enums):
                return refused("malformed_filter")
        if over is not None and not self.windowed(owner, spec, module):
            return refused("no_window")
        if filt is not None and self.reads_personal(owner, spec, filt):
            return refused("personal")
        groups, kind = self.grouped(owner, spec, module, bind, keep, over, filt)
        rows = [{"dimensions": dict(zip(keep, key)), "value": rounded(v, kind), "gaps": list(gaps), "time_basis": "UTC calendar"}
                for key, (v, gaps) in sorted(groups.items(), key=lambda kv: self.page_order(owner, keep, kv[0]))]
        flags = []
        if spec.get("flag_when"):
            # a flag is raised on each declared group, whatever the read keeps, over the rows the read selects (§6.9)
            every, _ = self.grouped(owner, spec, module, bind, declared, over, filt)
            for key, (v, _gaps) in sorted(every.items(), key=lambda kv: self.page_order(owner, declared, kv[0])):
                env = dict(zip(declared, key), value=rounded(v, kind))
                for flag, text in spec["flag_when"].items():
                    t = truth(Evaluator(s, owner, stubs=self.stubs).value(parsed(text), env))
                    if t is not False:
                        flags.append({"flag": flag, "group": dict(zip(declared, key)), "unknown": t is ABSENT})
        return {"rows": rows, "flags": flags, "history_complete": not any(r["gaps"] for r in rows), "refused": None}

    def page_kinds(self, owner, spec, module, keep):
        """The type of a metric page's values and of each dimension named, as an encoder of the page reads them: a
        combined metric's dimension typed as its first input having it (declaration-syntax.md §6.9)."""
        s = self.s
        if "input_metrics" in spec:
            kinds, dims = {}, {}
            for n, ref in spec["input_metrics"].items():
                o2, s2, m2 = s.find_metric(parsed(ref), module=module)
                mine = s.metric_dimensions(o2, s2)
                kinds[n], got = self.page_kinds(o2, s2, m2, [d for d in keep if d in mine])
                for d, k in got.items():
                    dims.setdefault(d, k)
            return self.kind(parsed(spec["expression"]), kinds), dims
        f = formula(s.kinds, s.enums, owner, spec)
        _rows, rowkind, by_kind = self.metric_rows(owner, f["source"])
        item = f["item"]
        row = Evaluator(s, owner, stubs=self.stubs, filter_mode=True)
        exprs = {"version": f"{item}.declaration_version", "actor_kind": f"{item}.{by_kind}"}
        exprs.update(f.get("dimensions") or {})
        env = {item: rowkind}
        return row.kind(parsed(f["expression"]), env), {d: row.kind(parsed(exprs[d]), env) for d in keep}

    def windowed(self, owner, spec, module):
        """Whether a read may window a metric: one with a time dimension or a split, or a combined one whose every input
        may be (ADR-0147)."""
        if "input_metrics" in spec:
            return all(self.windowed(*self.s.find_metric(parsed(ref), module=module)) for ref in spec["input_metrics"].values())
        f = formula(self.s.kinds, self.s.enums, owner, spec)
        return bool(f.get("time_dimension") or f.get("split_by"))

    def reads_personal(self, owner, spec, filt):
        """Whether a reader's filter reads a personal attribute of an object it reaches from the row (§6.9, ADR-0148)."""
        s = self.s
        f = formula(s.kinds, s.enums, owner, spec)
        source = f["source"]
        observed = (s.kinds.get(owner) or {}).get("observations") or {}
        rowkind = (("object", owner) if source == "objects" else ("interval", owner) if source.startswith("intervals")
                   else ("transition", owner) if source == "transitions"
                   else ("object", observed[source]["kind"]) if source in observed
                   else ("object", LABEL) if source == "labels" else ("other",))
        env = {f["item"]: rowkind}

        def walk(e):
            if not isinstance(e, tuple):
                return False
            if e and e[0] == "path":
                base = self.kind(e[1], env)
                if base[0] == "object" and (((s.kinds.get(base[1]) or {}).get("attributes") or {}).get(e[2]) or {}).get("personal"):
                    return True
            return any(walk(p) for p in e[1:] if isinstance(p, tuple)) or \
                any(walk(q) for p in e[1:] if isinstance(p, list) for q in p)
        return walk(parsed(filt))

    def grouped(self, owner, spec, module, bind, keep, over, filt):
        """A metric's exact value on each group of the dimensions kept, with the objects each read from before what their
        record vouches for, and its expression's type. A combined metric reads each input to the dimensions it has of
        those kept, and joins over every group an input with all of them has, an input missing a group contributing its
        value over no rows (ADR-0122 decision 18, ADR-0124, ADR-0147)."""
        s = self.s
        if "input_metrics" in spec:
            inputs, kinds = [], {}
            for n, ref in spec["input_metrics"].items():
                o2, s2, m2 = s.find_metric(parsed(ref), module=module)
                mine = s.metric_dimensions(o2, s2)
                k2 = [d for d in keep if d in mine]
                g2, kinds[n] = self.grouped(o2, s2, m2, {d: v for d, v in bind.items() if d in mine}, k2, over, None)
                inputs.append((n, k2, g2, self.over_no_rows(o2, s2, m2)))
            keys = []
            for _n, k2, g2, _e in inputs:
                if len(k2) == len(keep):
                    keys += [k for k in g2 if k not in keys]
            tree = parsed(spec["expression"])
            out = {}
            for key in keys:
                values, gaps = {}, set()
                for n, k2, g2, empty in inputs:
                    sub = tuple(v for d, v in zip(keep, key) if d in k2)
                    values[n], got = g2.get(sub, (empty, ()))
                    gaps |= set(got)
                out[key] = (self.value(tree, values), tuple(sorted(gaps)))
            return out, self.kind(tree, kinds)
        picked, f, item, rowkind, dims, row = self.selected(owner, spec, bind, over, filt)
        tree = parsed(f["expression"])
        members = {}
        for r in picked:
            for key in self.combinations(row, [parsed(dims[d]) for d in keep], {item: r}):
                members.setdefault(key, []).append(r)
        if not keep:
            members.setdefault((), [])      # the whole, read with nothing kept, is one row even over no rows
        # every object the runner holds has its whole record, so no row reads from before what it vouches for
        return ({key: (row.over_rows(tree, rs, item, rowkind), ()) for key, rs in members.items()},
                row.kind(tree, {item: rowkind}))

    def over_no_rows(self, owner, spec, module):
        """The value a metric's expression has over no rows (ADR-0122 decision 18)."""
        if "input_metrics" in spec:
            values = {n: self.over_no_rows(*self.s.find_metric(parsed(ref), module=module)) for n, ref in spec["input_metrics"].items()}
            return self.value(parsed(spec["expression"]), values)
        f = formula(self.s.kinds, self.s.enums, owner, spec)
        _rows, rowkind, _by = self.metric_rows(owner, f["source"])
        return Evaluator(self.s, owner, stubs=self.stubs, filter_mode=True).over_rows(parsed(f["expression"]), [], f["item"], rowkind)

    def combinations(self, row, exprs, env):
        """Each combination of the kept dimensions' values a row is counted under: one value where a path reaches no set,
        each distinct value where it goes through one, and, where two paths go through one set, the values of each of
        its members, never crossed; an empty set, or a member whose path reaches no value, counts the row under the
        absent value (ADR-0123, ADR-0146)."""
        fixed, shared = {}, {}
        for i, e in enumerate(exprs):
            chain, node = [], e
            while node[0] == "path":
                chain.append(node[2])
                node = node[1]
            chain.reverse()
            v, prefix, j = row.value(node, env), node, 0
            while not is_set(v) and j < len(chain):
                v, prefix, j = row.member(v, chain[j]), ("path", prefix, chain[j]), j + 1
            if is_set(v):
                shared.setdefault(prefix, (v, []))[1].append((i, chain[j:]))
            else:
                fixed[i] = v
        keys = [dict(fixed)]
        for members, idx in shared.values():
            tuples = []
            for m in members:
                vals = []
                for _i, rest in idx:
                    x = m
                    for name in rest:
                        x = ABSENT if x is ABSENT else row.member(x, name)
                    vals.append(x)
                if tuple(vals) not in tuples:
                    tuples.append(tuple(vals))
            tuples = tuples or [tuple(ABSENT for _ in idx)]
            keys = [{**k, **{i: t[n] for n, (i, _rest) in enumerate(idx)}} for k in keys for t in tuples]
        return [tuple(k[i] for i in range(len(exprs))) for k in keys]

    def page_order(self, owner, keep, key):
        """A page's order: by each kept dimension ascending, an absent value last, a state, an enumeration member or a
        kind of actor in declaration order, an object by id, which is creation order, a time bucket in time order and
        a string by code point (ADR-0127, ADR-0147)."""
        s = self.s
        out = []
        for d, v in zip(keep, key):
            if v is ABSENT:
                out.append((1,))
            elif isinstance(v, tuple) and v[:1] in (("state",), ("enum",)):
                # in the installed version's order, and one it no longer declares, from before a publish, after
                # those it does, by name (D536)
                order = list(((s.kinds.get(v[1]) or {}).get("states") or {})) if v[0] == "state" \
                    else list(s.enums.get(v[1]) or [])
                out.append((0, order.index(v[2]), "") if v[2] in order else (0, len(order), v[2]))
            elif d == "actor_kind" and v in ACTOR_KINDS + ("unknown",):
                out.append((0, (ACTOR_KINDS + ("unknown",)).index(v)))
            elif isinstance(v, str) and v in s.objects:
                out.append((0, s.objects[v].id_order))
            else:
                out.append((0, v))
        return tuple(out)

    def metric_rows(self, owner, source):
        """A metric's rows (declaration-syntax.md §6.9): each object of the type or its family, each span of its state or
        of a tracked member, each event of a transition, or each observation of one of its kinds that nothing has
        corrected. Returns them, what a row is, and the member naming the kind of actor behind one."""
        s = self.s
        family = [o for o in sorted(s.objects.values(), key=lambda o: o.id_order) if s.is_a(o.type, owner)]
        if source == "objects":
            return [o.id for o in family], self.row_kind(owner, source), "created_by_kind"
        if source == "intervals" or source.startswith("intervals("):
            member = "state" if source == "intervals" else source[len("intervals("):-1].strip()
            return [r for o in family for r in self.intervals(o, member)], self.row_kind(owner, source), "entered_by_kind"
        if source == "transitions":
            ids = {o.id for o in family}
            return [Row(e) for e in s.events if e["object"] in ids], self.row_kind(owner, source), "actor_kind"
        observed = (s.kinds.get(owner) or {}).get("observations") or {}
        if source in observed:
            kind = observed[source]["kind"]
            ids = {o.id for o in family}
            corrected = {x.attrs.get("corrects") for x in s.objects.values() if x.type == kind}
            return ([x.id for x in sorted(s.objects.values(), key=lambda x: x.id_order)
                     if x.type == kind and x.attrs.get("subject") in ids and x.id not in corrected],
                    self.row_kind(owner, source), "recorded_by_kind")
        if source == "labels":
            # each label on an object of the family that nothing has corrected (declaration-syntax.md §6.9)
            ids = {o.id for o in family}
            corrected = {x.attrs.get("corrects") for x in s.objects.values() if x.type == LABEL}
            return ([x.id for x in sorted(s.objects.values(), key=lambda x: x.id_order)
                     if x.type == LABEL and x.attrs.get("subject") in ids and x.id not in corrected],
                    self.row_kind(owner, source), "recorded_by_kind")
        if source in ("attempts", "attempt_counts"):
            # the rows of the family's types; a row of no type, an unknown id's, is none of them (DESIGN.md §7)
            mine = [a for a in s.attempts if a["type"] and s.is_a(a["type"], owner)]
            if source == "attempts":
                return [Row(a) for a in mine], self.row_kind(owner, source), "actor_kind"
            # each UTC day, object and combination of the members, counted; nothing is pruned, so the rollup is empty
            counts = {}
            members = ("object", "transition", "verdict", "clause", "remedy", "actor_kind", "enforced", "flagged",
                       "declaration_version")
            for a in mine:
                key = (day_of(a["at"]),) + tuple(a[m] for m in members)
                counts[key] = counts.get(key, 0) + 1
            return ([Row(dict(zip(("day",) + members, key), count=n)) for key, n in counts.items()],
                    self.row_kind(owner, source), "actor_kind")
        raise NotRunnable(f"a metric over `{source}`")

    def row_kind(self, owner, source):
        """What a row of a metric's source is, read from the source alone: an object, a span, an event, an
        observation, a label, an attempt or a day's count of them (declaration-syntax.md §6.9)."""
        if source == "objects":
            return ("object", owner)
        if source == "intervals" or source.startswith("intervals("):
            return ("interval", owner)
        if source == "transitions":
            return ("transition", owner)
        observed = (self.s.kinds.get(owner) or {}).get("observations") or {}
        if source in observed:
            return ("object", observed[source]["kind"])
        if source == "labels":
            return ("label", owner)
        if source in ("attempts", "attempt_counts"):
            return ("attempt" if source == "attempts" else "attempt_count", owner)
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
                return Fraction(sum(values, 0)) / len(values)   # exact: the metric's value is rounded once (ADR-0146)
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
                    "piece_start": ("timestamp",), "piece_end": ("timestamp",), "object": ("object", base[1])}.get(m, ("other",))
        if base[0] == "transition":
            return {"occurred_at": ("timestamp",), "recorded_at": ("timestamp",), "object": ("object", base[1])}.get(m, ("other",))
        if base[0] == "attempt":
            return {"at": ("timestamp",), "object": ("object", base[1])}.get(m, ("other",))
        if base[0] == "attempt_count":
            return {"day": ("date",), "count": ("int",), "object": ("object", base[1])}.get(m, ("other",))
        if base[0] == "label":
            # a label of the metric's type is on an object of it
            return ("object", base[1]) if m == "subject" else self.member_kind(("object", LABEL), m)
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
class _NeverAsked(dict):
    """The answers an offer gives its evaluators: none, each call left unanswered, so nothing is asked (ADR-0048)."""

    def __contains__(self, key):
        return True

    def __getitem__(self, key):
        return "unanswered"

    def __bool__(self):
        return True                     # an empty table the evaluator would otherwise replace with its own


NEVER_ASKED = _NeverAsked()


class Probe(_NeverAsked):
    """An offer's simulation's answers: none, as an offer's, noting whether any evaluator was asked, so a simulation
    that would ask one leaves its offer as availability decided it (ADR-0176 decision 2)."""

    def __init__(self):
        super().__init__()
        self.asked = False

    def __getitem__(self, key):
        self.asked = True
        return "unanswered"


class _DryRun(_NeverAsked):
    """The answers a dry run gives: no evaluator asked, and no metric consulted, each read left unanswered
    (publish-and-import.md §1 step 3, D537)."""
    metrics = False


DRY_RUN = _DryRun()

def written_locations(steps):
    """The attributes an effect's steps assign or add to, at any depth, a loop's among them, as the core reads them
    for not_erased (the audit's write path R-02)."""
    out = set()
    for st in steps or []:
        for k in ("assign", "add"):
            if k in st:
                out.add(st[k]["location"])
        if "foreach" in st:
            out |= written_locations(st["foreach"].get("steps"))
    return out


GENERATED_REMEDY = {"actor_known": "dependent", "actor_active": "dependent", "occurred_within": "self_serviceable", "whole_open": "self_serviceable",
                    "not_erased": "unreachable_from_here", "subject_open": "unreachable_from_here",
                    "corrects_current": "self_serviceable", "subject_owned": "unreachable_from_here",
                    "successor_not_superseded": "self_serviceable"}


def input_personal(t, x, name):
    """Whether an input is personal (declaration-syntax.md §10, check 10): marked so, or written by the transition's
    effect into a personal attribute."""
    if ((x.get("inputs") or {}).get(name) or {}).get("personal"):
        return True
    attrs = t.get("attributes") or {}
    return any(((attrs.get(st[k]["location"]) or {}).get("personal")
                and name in inputs_read(st[k].get("value", st[k].get("expr"))))
               for st, k in effect_steps(x.get("effect") or []) if k in ("assign", "add"))


def effect_steps(steps):
    """Each step of an effect with its kind, a loop's steps after it."""
    for st in steps or ():
        k = next(iter(st))
        yield st, k
        if k == "foreach":
            yield from effect_steps(st[k].get("steps"))


def inputs_read(text):
    """The inputs an expression reads, `inputs.<name>`."""
    out = set()

    def walk(n):
        if isinstance(n, tuple):
            if len(n) == 3 and n[0] == "path" and n[1] == ("name", "inputs") and isinstance(n[2], str):
                out.add(n[2])
            for c in n[1:]:
                walk(c)
        elif isinstance(n, list):
            for c in n:
                walk(c)
    if isinstance(text, str):
        try:
            walk(flowexpr.parse(text))
        except Exception:           # a literal the format gives as YAML reads no input
            pass
    return out


def carries(text, tainted, holding):
    """Whether an expression carries a tainted value: it reads a tainted input, or an attribute holding one, bare or
    through `this` (ADR-0160)."""
    if not isinstance(text, str):
        return False
    if inputs_read(text) & tainted:
        return True
    found = []

    def walk(n):
        if isinstance(n, tuple):
            if n[:1] == ("name",) and len(n) == 2 and n[1] in holding:
                found.append(n)
            elif len(n) == 3 and n[0] == "path" and n[1] == ("name", "this") and n[2] in holding:
                found.append(n)
            for c in n[1:]:
                walk(c)
        elif isinstance(n, list):
            for c in n:
                walk(c)
    try:
        walk(flowexpr.parse(text))
    except Exception:
        return False
    return bool(found)


def steps_taking(store, steps, transition, tn):
    """For each step of an effect that takes the transition given, a call of it or a creation of the type, each input it
    fills and the expressions that fill it, a loop's array among them where the input reads the loop's item."""
    out = []
    for st in steps or ():
        k = next(iter(st))
        spec = st[k]
        if (k == "create" and spec.get("transition") == transition and store.is_a(tn, spec.get("type"))) \
                or (k == "call" and spec.get("transition") == transition):
            out += [(n, [e]) for n, e in (spec.get("inputs") or {}).items()]
        elif k == "foreach":
            for n, exprs in steps_taking(store, spec.get("steps"), transition, tn):
                reads = any(spec["item"] in str(e) for e in exprs)
                out.append((n, exprs + ([spec["array"]] if reads and "array" in spec else [])))
    return out


def attributes_written(t, x, name):
    """The attributes an input writes: itself where it is an attribute's input, and each location an effect step
    assigns or adds it to."""
    out = set()
    if name in x.get("required_inputs", []) + x.get("optional_inputs", []) and name in (t.get("attributes") or {}):
        out.add(name)
    for st, k in effect_steps(x.get("effect") or []):
        if k in ("assign", "add") and name in inputs_read(st[k].get("value", st[k].get("expr"))):
            out.add(st[k]["location"])
    return out


class Ctx:
    """One request's working store, and what it has taken and written so far."""

    def __init__(self, work, who, stubs, root_type):
        self.work, self.who, self.stubs, self.root_type = work, who, stubs, root_type
        self.written = []           # object ids, in the order first written
        self.taken = []             # "Type.transition" of every transition taken, the requested one first
        self.flags = []             # the audit and warn guards that failed, at any depth
        self.flag_told = {}         # a flag's clause -> (whether it was unknown, the objects it names)
        self.observed = []          # their attempt rows, each linked to the event of the transition whose clause failed
        self.asserted, self.admits = None, set()    # the object an assertion set, and what it admits
        self.erased = {}            # object id -> the personal attributes this request erased on it
        self.renamed = []           # each erased actor's identity and its object's id (ADR-0192 decision 4)
        self.position_of = {}       # object id -> the position of the event its current transition records
        self.taking = []            # (type, transition) of each transition being taken, the innermost last, which a
                                    # nested `only_via` reads its caller from (the audit's write path R-05)
        self.causes = []            # the positions of the transitions being taken, the innermost last: each event's
                                    # cause is the one taken when it began (storage-schema.md §2, cause_position)
        self.before = None          # the store as the request found it
        self.created = []           # the objects it creates other than its own, in the order it creates them
        self.attr_writes = {}       # object id -> the attributes its current transition's effect wrote
        self.occurred = None        # the time the request says the change happened, where it gives one
        self.filed_at = None        # for an approved proposal, when it was filed
        self.at = work.now          # the occurred time every transition the request takes records (DESIGN.md §6)

    def wrote(self, oid):
        if oid not in self.written:
            self.written.append(oid)


def qualified(clause, tn):
    return clause if clause is None or "." in clause else f"{tn}.{clause}"


class Runner:
    """Requests against a store, as DESIGN.md §6 executes them, within the slices built so far."""

    unasked = NEVER_ASKED                       # what an offer's evaluators are answered: nothing (ADR-0048)

    def __init__(self, kinds, enums):
        self.kinds, self.enums = with_labels(kinds), enums

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
            # written as a money literal, SGD 12000.00 (§5), whose currency, where written, is the attribute's (D548)
            code, _, amount = v.strip().rpartition(" ")
            if code.strip() and code.strip() != typ[len("money("):-1]:
                return ("another currency", code.strip(), amount)
            return Fraction(amount)
        if typ == "file":
            return file_value(v)
        if typ == "timestamp":
            m = re.fullmatch(r"now(?: ([+-]) ([0-9]+) (\w+))?", v)
            if m:
                shift = Fraction(int(m.group(2))) * SECONDS[m.group(3)] if m.group(1) else 0
                return store.now + (shift if m.group(1) != "-" else -shift)
            return timestamp(v)
        if typ == "date":
            return datetime.date.fromisoformat(str(v))
        if typ == "duration":
            n, unit = v.split()
            return Fraction(int(n)) * SECONDS[unit]
        if typ.startswith(("decimal", "money")):
            return Fraction(str(v))
        if typ in ("int", "counter") and isinstance(v, str):
            return int(v)   # an integer past 2^53, which the wire carries as a string (wire.md), held as the integer
        return v

    def owner_end(self, spec):
        """Whether a reference is a part's end back to its whole: its opposite is a composite end."""
        if not spec or "reference" not in spec:
            return False
        target = self.kinds.get(spec["reference"].rstrip("[]")) or {}
        back = ((target.get("attributes") or {}).get(spec.get("opposite") or "") or {})
        return back.get("aggregation") == "composite"

    # ── a request ─────────────────────────────────────────────────────────────
    def attempt(self, store, tn, xn, oid, actor, raw_inputs, aliases, stubs, occurred=None, filed_at=None, proposal=None,
                named=None):
        """The request's result, ("applied", object id, flags, the transitions it caused), or a Refused, which the
        attempt log records: one row of the requested transition, or for an approval of the proposed one on its
        object naming the proposal (DESIGN.md §6, §9). The store changes only if the request applies. `occurred` is the
        time the request says the change happened."""
        try:
            return self.attempted(store, tn, xn, oid, actor, raw_inputs, aliases, stubs, occurred, filed_at, named)
        except Refused as r:
            store.attempts.append(attempt_row(store, tn if oid is None or oid in store.objects else None,
                                              oid if oid in store.objects else None, xn, actor, r, proposal=proposal))
            raise

    def attempted(self, store, tn, xn, oid, actor, raw_inputs, aliases, stubs, occurred=None, filed_at=None, named=None):
        work = copy.deepcopy(store)
        # 1. the actor: an object of a type that marks an actor identity, in a state that is not final (DESIGN.md §6
        # step 1, ADR-0182 decision 6)
        if actor not in work.objects or work.actor_attr(work.type_of(actor)) is None:
            raise Refused("unsatisfied", "actor_known", "dependent")
        if work.final(work.objects[actor]):
            raise Refused("unsatisfied", "actor_active", "dependent")
        attr, akind = work.actor_attr(work.type_of(actor))
        ctx = Ctx(work, {"id": work.objects[actor].attrs.get(attr, ABSENT), "kind": akind}, stubs, tn)
        ctx.before = store
        if occurred is not None:
            ctx.occurred = ctx.at = occurred
        ctx.filed_at = filed_at     # an approved proposal's bound is measured from its filing (ADR-0147)
        t = self.kinds.get(tn) or {}
        x = (t.get("transitions") or {}).get(xn)
        # 3. not found, and unknown transition; 4. not requestable, unavailable, invalid input, in that order
        if oid is not None and oid not in work.objects:
            raise Refused("not_found", remedy="unreachable_from_here")
        # an object named beside a type is found only as one of that type's family, so a tool bound to a type takes
        # no other type's object of the id it is given (thirteenth production review B)
        if oid is not None and named is not None and not work.is_a(work.type_of(oid), named):
            raise Refused("not_found", remedy="unreachable_from_here")
        # the operator's creation is taken only by creating the store, and its retirement by nothing, which would leave
        # the store with no operator (ADR-0126, ADR-0184 decision 1)
        if tn == "Operator" and (xn, oid is None) in (("install", True), ("retire", False)):
            raise Refused("not_requestable", remedy="unreachable_from_here")
        if x is None or (x["kind"] == "initial") != (oid is None) or (oid is not None and work.type_of(oid) != tn):
            raise Refused("unknown_transition", remedy="self_serviceable")
        if x.get("only_via"):
            raise Refused("not_requestable", remedy="unreachable_from_here")
        self.check_state(work, x, oid)
        inputs = {}
        attrs = t.get("attributes") or {}
        declared = x.get("inputs") or {}
        taken = set(x.get("required_inputs", [])) | set(x.get("optional_inputs", [])) | set(declared)
        own = x["kind"] == "assertion" and {"to", "admits"} or set()
        # the inputs it does not take, every one, then every one not of its type, by name, as the core orders them
        unknown = [(k, "unknown_input") for k in sorted(raw_inputs or {}) if k not in taken and k not in own]
        if unknown:
            raise Refused("invalid_input", remedy="self_serviceable", inputs=[k for k, _ in unknown], reasons=[w for _, w in unknown])
        wrong = []
        for k in sorted(raw_inputs or {}):
            v = raw_inputs[k]
            if k in own:
                if k == "to" and isinstance(v, str) or k == "admits" and isinstance(v, (list, tuple)) and all(isinstance(a, str) for a in v):
                    inputs[k] = tuple(v) if k == "admits" else v      # a state, and invariants, by name (§4.13)
                else:
                    wrong.append((k, "wrong_type"))
                continue
            spec = declared.get(k) if k in declared else attrs.get(k)
            why = input_fault(spec, v, self.enums, self.kinds)
            if why:
                wrong.append((k, why))
                continue
            if "reference" in (spec or {}) and any(a not in aliases for a in (v if isinstance(v, (list, tuple)) else [v])):
                inputs[k] = v if not isinstance(v, (list, tuple)) else tuple(v)   # names nothing: refused below
                continue
            value = self.value_of(work, spec, v, aliases)
            if value is not ABSENT:
                inputs[k] = value
        if wrong:
            raise Refused("invalid_input", remedy="self_serviceable", inputs=[k for k, _ in wrong], reasons=[w for _, w in wrong])
        try:
            root = self.take(ctx, tn, xn, oid, inputs, depth=0)
            self.clear_intervals(ctx)
            self.rename_erased_actors(ctx)
            # 7. every invariant the writes could violate, in ascending id order of the object it fails on, except
            # a violation an admission lets stand, which is discharged once the invariant holds again (ADR-0054)
            requested = oid if oid is not None else root
            written = {e["object"] for e in work.events[len(store.events):]}
            # every object an invariant fails on, in ascending id order, every invariant that fails on it, and the
            # objects in conflict with them (DESIGN.md §5.5, library-api.md §4)
            failures, conflicting = [], set()
            for obj in sorted(work.objects.values(), key=lambda o: o.id_order):
                if (self.kinds.get(obj.type) or {}).get("_observation_on") and obj.id not in written:
                    continue                    # a kind's invariants hold of a recording as written, and bind none older (D539)
                failing = self.failing_invariants(work, obj)
                obj.admissions &= set(failing)
                for n in failing:
                    if n in obj.admissions:
                        continue
                    if (obj.id == ctx.asserted and n in ctx.admits) or f"{obj.type}.{n}" in ctx.admits:
                        obj.admissions.add(n)           # an assertion's admission (§4.13)
                    elif self.erasure_broke(ctx, obj, n):
                        obj.admissions.add(n)           # an erasure admits what it leaves violated, on that object (§4.13)
                    else:
                        failures.append((obj, n, self.invariant_remedy(work, x, obj, n, requested, xn)))
                        conflicting |= self.invariant_objects(work, obj, n)
            if failures:
                obj, first, remedy = failures[0]
                rest = [n if o.type == tn else f"{o.type}.{n}" for o, n, _ in failures[1:]]
                raise Refused("invariant_violated", first if obj.type == tn else f"{obj.type}.{first}", remedy,
                              detail=", ".join(rest) and "and " + ", ".join(rest), object_id=obj.id,
                              object_type=obj.type,
                              objects=sorted(conflicting, key=lambda i: work.objects[i].id_order),
                              failures=[(o.id, o.type, n, r) for o, n, r in failures])
        except Refused as r:
            store.sequences = work.sequences    # a number minted by a refused request stays used: the gap
            # a refusal names only objects the store holds: one the refused request was creating does not exist,
            # and a later object takes its id (D581)
            refused = r
            while refused is not None:
                refused.objects = [i for i in refused.objects if i in store.objects]
                # and a refused call names no object the request was creating (D581's residue)
                if refused.verdict == "call_refused" and refused.object_id not in store.objects:
                    refused.object_id = None
                if refused.failures:
                    refused.failures = [(o if o in store.objects else None, t, n, rem)
                                        for o, t, n, rem in refused.failures]
                refused = refused.inner
            raise
        work.attempts.extend(ctx.observed)
        store.__dict__.update(work.__dict__)
        return ("applied", root, ctx.flags, ctx.taken[1:], ctx.created, ctx.at, ctx.flag_told)

    def availability(self, store, actor, oid):
        """What each requestable transition of an object offers an actor now, and for a subject the recording of each
        observation kind on it (DESIGN.md §10, library-api.md §5): decided on the actor, the state, and the guards that
        read no input, as `available` sweeps, and never on the effect or the invariants, which `check` simulates
        (ADR-0152). An evaluator is never asked: a guard that would ask one is listed as unevaluated (ADR-0048,
        ADR-0049). A transition that takes a required input, or whose guard reads an input, is available with input
        once the rest passes. Each offer is (transition, the type a recording creates, availability, the refusal or
        None, the input schema, the guards unevaluated, the flags it would raise as (clause, remedy))."""
        if oid not in store.objects:
            raise Refused("not_found", remedy="unreachable_from_here")
        o = store.objects[oid]
        t = self.kinds.get(o.type) or {}
        who = None
        if actor in store.objects and store.actor_attr(store.type_of(actor)) is not None:
            attr, akind = store.actor_attr(store.type_of(actor))
            # an actor in a final state is refused every offer (ADR-0182 decision 6)
            who = {"id": store.objects[actor].attrs.get(attr, ABSENT), "kind": akind,
                   "ended": store.final(store.objects[actor])}
        offers = []
        for xn, x in (t.get("transitions") or {}).items():
            if x["kind"] == "initial" or x.get("only_via"):
                continue
            offers.append(self.offer(store, o, t, xn, x, who))
        if o.type == PROPOSAL:
            # a proposal's decisions, which its declared lifecycle names and a request takes on it by their own path: each
            # requestable out of PENDING and guarded by nothing of its own, the proposed request's guards being decided when
            # an approval executes it; `invalidate` is only via a publish or an erasure (DESIGN.md §9, D535)
            for xn in ("approve", "reject", "withdraw"):
                if who is None:
                    r = Refused("unsatisfied", "actor_known", GENERATED_REMEDY["actor_known"])
                elif who["ended"]:
                    r = Refused("unsatisfied", "actor_active", GENERATED_REMEDY["actor_active"])
                elif o.state != "PENDING":
                    r = Refused("unavailable", remedy="unreachable_from_here", detail=f"it is in {o.state}")
                else:
                    r = None
                offers.append((xn, None, "unavailable" if r else "available", r, {}, [], []))
        for kn, k in self.kinds.items():
            ref = ((k.get("attributes") or {}).get("subject") or {}).get("reference")
            if "record" in (k.get("transitions") or {}) and ref and (ref == ANY_TYPE or store.is_a(o.type, ref)):
                offers.append(self.record_offer(store, o, kn, k, who))
        return offers

    @staticmethod
    def input_schema(attrs, x, leave=()):
        """A transition's inputs by name, each with the type a caller supplies (library-api.md §5)."""
        def typed(spec):
            spec = spec or {}
            return spec.get("reference") or str(spec.get("type", ""))
        out = {k: typed(attrs.get(k)) for k in x.get("required_inputs", []) + x.get("optional_inputs", []) if k not in leave}
        out.update({k: typed(spec) for k, spec in (x.get("inputs") or {}).items()})
        if x["kind"] == "assertion":
            out.update({"to": "state", "admits": "invariant[]"})
        return out

    def decided_for(self, store, actor, oid, xn, aliases):
        """An offer that may be refused for who takes it, decided for who asks (ADR-0176 decision 2): the request
        with no inputs simulated whole on a copy of the store, as `check` simulates it, no evaluator asked. Its refusal,
        or None where it applies, or where the simulation would ask an evaluator or reaches what the runner does not
        run, which leaves the offer as availability decided it."""
        probe = Probe()
        try:
            self.attempted(copy.deepcopy(store), store.type_of(oid), xn, oid, actor, {}, aliases, probe)
        except Refused as r:
            return None if probe.asked else r
        except NotRunnable:
            return None
        return None

    def offer(self, store, o, t, xn, x, who):
        attrs = t.get("attributes") or {}
        schema = self.input_schema(attrs, x)
        needs = x["kind"] == "assertion" or bool(x.get("required_inputs")) or any(
            not (spec or {}).get("optional") and "default" not in (spec or {}) for spec in (x.get("inputs") or {}).values())
        unevaluated, flags = [], []
        try:
            if who is None:
                raise Refused("unsatisfied", "actor_known", GENERATED_REMEDY["actor_known"])
            if who["ended"]:
                raise Refused("unsatisfied", "actor_active", GENERATED_REMEDY["actor_active"])
            self.check_state(store, x, o.id)
            personal = {a for a, sp in attrs.items() if (sp or {}).get("personal")}
            if x["kind"] != "erasure" and o.erased:
                # decided on what its effect writes; what its inputs would write waits for them (ADR-0147)
                writes = written_locations(x.get("effect"))
                if writes & personal:
                    raise Refused("unsatisfied", "not_erased", GENERATED_REMEDY["not_erased"])
            conditions = t.get("conditions") or {}
            for g, mode in (x.get("guards") or {}).items():
                if g not in conditions:
                    # a version the checks refused, which a dry run reads on past them (D542)
                    raise NotRunnable(f"the guard {g}, which {o.type} does not declare")
                expr = flowexpr.parse(conditions[g]["expression"])
                if "inputs" in names_read(expr):
                    needs = True                # decided when the inputs are given
                    continue
                asking = Evaluator(store, o.type, o.id, {}, who, self.unasked, committed=store).noting(o.id)
                ok = truth(asking.value(expr, {}))
                if asking.unanswered:
                    unevaluated.append(g)
                    continue
                if ok is not True:
                    if mode == "deny":
                        raise Refused("unsatisfied", g, conditions[g].get("remedy"), unknown=ok is ABSENT,
                                      objects=asking.named())
                    if mode == "warn":
                        flags.append((g, conditions[g].get("remedy"), ok is ABSENT, asking.named()))
        except Refused as r:
            return (xn, None, "unavailable", r, schema, unevaluated, [])
        return (xn, None, "available_with_input" if needs else "available", None, schema, unevaluated, flags)

    def record_offer(self, store, o, kn, k, who):
        """Recording an observation of a kind on this subject: its generated guards are decided on the subject, and
        its fields wait for the caller (ADR-0099)."""
        x = k["transitions"]["record"]
        schema = self.input_schema(k.get("attributes") or {}, x, leave=("subject",))
        try:
            if who is None:
                raise Refused("unsatisfied", "actor_known", GENERATED_REMEDY["actor_known"])
            if who["ended"]:
                raise Refused("unsatisfied", "actor_active", GENERATED_REMEDY["actor_active"])
            if store.final(o):
                raise Refused("unsatisfied", "subject_open", GENERATED_REMEDY["subject_open"], objects=[o.id])
            if (self.kinds.get(o.type) or {}).get("mirror"):
                raise Refused("unsatisfied", "subject_owned", GENERATED_REMEDY["subject_owned"], objects=[o.id])
        except Refused as r:
            return ("record", kn, "unavailable", r, schema, [], [])
        needs = any(i != "subject" for i in x.get("required_inputs", []))
        return ("record", kn, "available_with_input" if needs else "available", None, schema, [], [])

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

            def value(name):
                return Evaluator(work, o.type, o.id).value(flowexpr.parse(name), {})

            def field(m):
                name, _, width = m.group(1).partition(":")
                if name == "n":
                    return str(n).zfill(int(width or 0))
                v = value(name)
                if v is ABSENT:
                    raise NotRunnable(f"a format field {name} that is absent outside `[ … ]`")
                return str(v)

            def segment(m):
                # `[ … ]`, a segment left out whole when a value inside it is absent (flow-format.md §4.12)
                names = [f.partition(":")[0] for f in re.findall(r"\{([^}]+)\}", m.group(1))]
                if any(nm != "n" and value(nm) is ABSENT for nm in names):
                    return ""
                return re.sub(r"\{([^}]+)\}", field, m.group(1))
            o.attrs[a] = re.sub(r"\{([^}]+)\}", field, re.sub(r"\[([^\]]*)\]", segment, ident.get("format", "{n}")))

    def check_state(self, work, x, oid):
        if oid is None or x["kind"] == "erasure":       # an erasure runs at any state, a final one included
            return
        if x["kind"] == "assertion":                    # an assertion runs from any state but a final one
            if work.final(work.objects[oid]):
                raise Refused("unavailable", remedy="unreachable_from_here", detail=f"it is in {work.objects[oid].state}")
            return
        froms = x.get("from")
        froms = [froms] if isinstance(froms, str) else list(froms or [])
        if froms == ["any"]:
            # every state that is not final, and for an external transition every one but its target, so no transition
            # leaves a state for itself (flow-format.md §4.8, ADR-0122)
            states = (self.kinds.get(work.objects[oid].type) or {}).get("states") or {}
            froms = [st for st, v in states.items() if not (v or {}).get("final") and st != x.get("to")]
        if work.objects[oid].state not in froms:
            raise Refused("unavailable", remedy="unreachable_from_here", detail=f"it is in {work.objects[oid].state}")

    def take(self, ctx, tn, xn, oid, inputs, depth):
        """One transition, the requested one or one it causes, as `taken` takes it: one a call, a creation or a cascade
        takes, where `only_via` names its callers, refused `not_requestable` unless the transition taking it is one it
        names, as a request for it is (the audit's write path R-05). Returns the object's id."""
        x = ((self.kinds.get(tn) or {}).get("transitions") or {}).get(xn)
        if depth and x is not None and x.get("only_via"):
            parent = ctx.taking[-1] if ctx.taking else None
            named = parent is not None and any(
                "." in v and v.split(".", 1)[1] == parent[1] and ctx.work.is_a(parent[0], v.split(".", 1)[0])
                for v in x["only_via"])
            if not named:
                raise Refused("not_requestable", remedy="unreachable_from_here", parents=list(x["only_via"]))
        ctx.taking.append((tn, xn))
        try:
            return self.taken(ctx, tn, xn, oid, inputs, depth)
        finally:
            ctx.taking.pop()

    def taken(self, ctx, tn, xn, oid, inputs, depth):  # noqa: C901  one transition, as DESIGN.md §6 orders it
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
            unknown = [(k, "unknown_input") for k in sorted(inputs) if k not in taken]
            if unknown:
                raise Refused("invalid_input", remedy="self_serviceable", inputs=[k for k, _ in unknown], reasons=[w for _, w in unknown])
        wrong = []
        for k in sorted(inputs):
            spec = declared.get(k) if k in declared else attrs.get(k)
            if "reference" in (spec or {}):
                want = spec["reference"].rstrip("[]")
                if not all(ref in work.objects and (want == ANY_TYPE or work.is_a(work.type_of(ref), want))
                           for ref in (inputs[k] if isinstance(inputs[k], tuple) else (inputs[k],))):
                    wrong.append((k, "no_such_object"))
        if wrong:
            raise Refused("invalid_input", remedy="self_serviceable", inputs=[k for k, _ in wrong], reasons=[w for _, w in wrong])
        left = [(k, "missing") for k in x.get("required_inputs", [])
                if k not in inputs and not (attrs.get(k) or {}).get("optional") and "default" not in (attrs.get(k) or {})]
        if left:
            raise Refused("invalid_input", remedy="self_serviceable", inputs=[k for k, _ in left], reasons=[w for _, w in left])
        missing = []
        for k, spec in declared.items():
            if k not in inputs and "default" in (spec or {}):
                inputs[k] = Evaluator(work, tn, oid, inputs, ctx.who, ctx.stubs).value(flowexpr.parse(spec["default"]), {})
            elif k not in inputs and not (spec or {}).get("optional"):
                missing.append((k, "missing"))
        if missing:
            raise Refused("invalid_input", remedy="self_serviceable", inputs=[k for k, _ in missing], reasons=[w for _, w in missing])
        if x["kind"] == "assertion":
            o = work.objects[oid]
            if inputs.get("to") not in (x.get("to") if isinstance(x.get("to"), list) else [x.get("to")]) or inputs.get("to") == o.state:
                raise Refused("invalid_input", remedy="self_serviceable", inputs=["to"], reasons=["not_allowed"])
            if not set(inputs.get("admits", ())) <= set(x.get("may_admit") or []):
                raise Refused("invalid_input", remedy="self_serviceable", inputs=["admits"], reasons=["not_allowed"])
            ctx.asserted, ctx.admits = oid, set(inputs.get("admits", ()))
        # the generated guards, in the order flow-format.md §4.8 states, then the <attribute>_provided ones, then the
        # transition's own. occurred_within: the requested transition's bound, its backdating_limit or its kind's
        # max_recording_delay, takes an occurred time no further back than the bound and not after the recorded time,
        # and one with no bound, or a correction, which takes the corrected's, takes none; and every transition the
        # request takes keeps the time from preceding the current interval of what it changes on its own object (§4.4)
        if depth == 0 and ctx.occurred is not None:
            limit, told = x.get("backdating_limit"), ctx.filed_at if ctx.filed_at is not None else work.now
            if "corrects" in inputs or limit is None or not told - seconds(limit) <= ctx.occurred <= told:
                raise Refused("unsatisfied", "occurred_within", GENERATED_REMEDY["occurred_within"])
        if oid is not None:
            changing = {m for m in set(self.tracked(tn)) & set(inputs) if work.objects[oid].attrs.get(m, ABSENT) != inputs[m]}
            if x["kind"] == "external" or (x["kind"] == "assertion" and inputs.get("to") != work.objects[oid].state):
                changing.add("state")
            self.check_interval(ctx, work.objects[oid], changing)
        wholes = []
        for k in set(x.get("required_inputs", []) + x.get("optional_inputs", [])) & set(inputs):
            if self.owner_end(attrs.get(k)):
                if oid is not None and work.objects[oid].attrs.get(k) != inputs[k]:
                    if work.final(work.objects[inputs[k]]):
                        raise Refused("unsatisfied", "whole_open", GENERATED_REMEDY["whole_open"], objects=[inputs[k]])
                    wholes.append(work.objects[oid].attrs.get(k))
                wholes.append(inputs[k])
        personal = {a for a, sp in attrs.items() if (sp or {}).get("personal")}
        if oid is not None and x["kind"] != "erasure" and work.objects[oid].erased:
            writes = set(inputs) & set(x.get("required_inputs", []) + x.get("optional_inputs", []))
            # decided here, from the inputs and the steps that assign or add to an attribute, whatever they compute; a
            # clear writes absence and passes (ADR-0147)
            writes |= written_locations(x.get("effect"))
            if writes & personal:
                raise Refused("unsatisfied", "not_erased", GENERATED_REMEDY["not_erased"])
        # a successor an input names is neither this object nor one already superseded, so no chain closes on itself;
        # a transition's new state is written before its effect runs, so this holds within one request too (ADR-0144)
        for st in x.get("effect") or []:
            m = re.fullmatch(r"inputs\.([a-z][a-z0-9_]*)", " ".join(str(st.get("supersede", "")).split()))
            successor = inputs.get(m.group(1)) if m else None
            if successor is not None and (successor == oid or work.superseded(work.objects[successor])):
                raise Refused("unsatisfied", "successor_not_superseded", GENERATED_REMEDY["successor_not_superseded"],
                              objects=[successor])
        if xn == "record" and "subject" in inputs:
            subject = work.objects[inputs["subject"]]
            if "corrects" in inputs:
                corrected = work.objects[inputs["corrects"]]
                if corrected.attrs.get("subject") != subject.id:
                    # a correction's subject is the corrected observation's (ADR-0146)
                    raise Refused("invalid_input", remedy="self_serviceable", inputs=["corrects"], reasons=["not_allowed"])
                recorded = [other.id for other in sorted(work.objects.values(), key=lambda o: o.id_order)
                            if other.attrs.get("corrects") == corrected.id]
                if recorded:
                    raise Refused("unsatisfied", "corrects_current", GENERATED_REMEDY["corrects_current"], objects=recorded[:1])
            elif work.final((ctx.before.objects.get(subject.id) if ctx.before else None) or subject) \
                    and (not ctx.before or subject.id in ctx.before.objects):
                # a correction may be recorded after the subject is finished (DESIGN.md §5.11); otherwise the subject is read
                # as the request found it, so an outcome may record on its own object as it enters a final state (ADR-0146)
                raise Refused("unsatisfied", "subject_open", GENERATED_REMEDY["subject_open"], objects=[subject.id])
            if (self.kinds.get(subject.type) or {}).get("mirror"):
                raise Refused("unsatisfied", "subject_owned", GENERATED_REMEDY["subject_owned"], objects=[subject.id])
        for k in x.get("required_inputs", []):
            spec = attrs.get(k) or {}
            if k not in inputs and (spec.get("optional") or "default" in spec):
                raise Refused("unsatisfied", f"{k}_provided", "self_serviceable")
        if tn == LABEL and xn == "record":
            # a label's name is vocabulary, normalised to lower case and trimmed (declaration-syntax.md §6.8)
            inputs = dict(inputs, name=inputs["name"].strip().lower())
            # and a name empty once normalised is no name (ADR-0161)
            if not inputs["name"]:
                raise Refused("invalid_input", remedy="self_serviceable", inputs=["name"], reasons=["malformed"])
        conditions = t.get("conditions") or {}
        failed = []
        for g, mode in (x.get("guards") or {}).items():
            asking = Evaluator(work, tn, oid, inputs, ctx.who, ctx.stubs, committed=ctx.before).noting(oid)
            ok = truth(asking.value(flowexpr.parse(conditions[g]["expression"]), {}))
            if ok is not True:
                remedy = "temporal" if asking.unanswered else conditions[g].get("remedy")
                if mode == "deny":
                    # an evaluator that did not answer leaves the guard unknown, and the caller may try again (ADR-0138)
                    raise Refused("unsatisfied", g, remedy, unknown=ok is ABSENT, objects=asking.named())
                ctx.flags.append((f"{tn}.{g}", mode))
                ctx.flag_told[f"{tn}.{g}"] = (ok is ABSENT, asking.named())
                failed.append((g, mode, remedy, ok is ABSENT))
        # 5. the outcome: its event's position allocated as it begins applying, so this_event is known to it (§7)
        work.position += 1
        pos = work.position
        cause = ctx.causes[-1] if ctx.causes else None
        ctx.causes.append(pos)
        before = (work.objects[oid].state, dict(work.objects[oid].attrs), self.tracked_values(work.objects[oid])) if oid else (None, {}, {})
        if oid is None:
            o = Obj(work.new_id(tn), tn, x["to"], ctx.at)
            o.id_order = work.counter
            o.created_by_kind = ctx.who["kind"]
            if depth:
                ctx.created.append(o.id)
            work.objects[o.id] = o
            for a, spec in attrs.items():
                if is_set_attribute(spec):
                    o.attrs[a] = ()             # a set is never absent: created empty unless given (flow-format.md §4.3)
                elif (spec or {}).get("type") == "counter":
                    o.attrs[a] = 0
                elif "default" in (spec or {}):
                    o.attrs[a] = at_declared_scale(spec, Evaluator(work, tn, o.id, inputs, ctx.who, ctx.stubs).value(
                        flowexpr.parse(str(spec["default"])), {}), o.id)
            if xn == "record" and "subject" in inputs:
                o.attrs["recorded_at"] = work.now
                # a correction replaces the fact it corrects, so it keeps that fact's occurred time (ADR-0146)
                corrected = work.objects.get(inputs["corrects"]) if "corrects" in inputs else None
                o.attrs["occurred_at"] = corrected.attrs.get("occurred_at", work.now) if corrected else ctx.at
                if tn == LABEL:
                    # the subject's state when it was applied (DESIGN.md §5.11); a label is requested, never created by an
                    # outcome, so nothing has changed the subject in the request before it
                    o.attrs["subject_state"] = work.objects[inputs["subject"]].state
        else:
            o = work.objects[oid]
            if x["kind"] == "external":
                o.state = x["to"]
                o.entered[o.state] = ctx.at
            elif x["kind"] == "assertion":
                o.state = inputs["to"]
                o.entered[o.state] = ctx.at
            elif x["kind"] == "erasure":
                self.erase(ctx, o)
        for k in x.get("required_inputs", []) + x.get("optional_inputs", []):
            if k in inputs:
                o.attrs[k] = at_declared_scale(attrs.get(k), inputs[k], o.id)
        if oid is None:
            self.mint(ctx, o, attrs)
        ctx.wrote(o.id)
        ctx.position_of[o.id] = pos
        # a flag raised, or an audit's would-be refusal, is a row of the type whose clause failed, linked to this event
        # and written with it (DESIGN.md §5.5, §7, ADR-0111)
        for g, mode, remedy, unknown in failed:
            ctx.observed.append({"type": tn, "object": o.id, "transition": xn, "verdict": "unsatisfied", "clause": g,
                                 "remedy": remedy, "unknown": unknown, "enforced": False, "flagged": mode == "warn",
                                 "actor_id": ctx.who["id"], "actor_kind": ctx.who["kind"], "at": work.now,
                                 "declaration_version": work.version, "applied_position": pos, "proposal": None})
        ctx.attr_writes[o.id] = set(inputs) & set(x.get("required_inputs", []) + x.get("optional_inputs", []))
        for w in wholes:
            if w is not None:
                ctx.wrote(w)
        ctx.taken.append(f"{tn}.{xn}")
        # 6. the effect, then the cascades to parts, depth-first (ADR-0038, ADR-0138)
        self.effects(ctx, tn, o, x.get("effect") or [], inputs, {}, depth)
        self.cascades(ctx, t, tn, xn, o, inputs, depth)
        # an actor's identity is written only at its creation (the audit's write path I-01, ADR-0205 decision 5): the
        # checker refuses a flow writing it later before the runner runs one, so the core's refusal at run time,
        # `identity_unchanged`, holds only flows an earlier release published, which the runner never runs
        if x["kind"] == "erasure":
            self.forget_observations(ctx, o, inputs, depth)
            self.erase_chain(ctx, o, inputs, depth)
        ctx.causes.pop()
        self.record(ctx, o, x, xn, pos, before, inputs, created=oid is None, cause=cause)
        o.version += 1
        return o.id

    # ── the record a transition leaves: its event, its spans and what it wrote ─────
    def tracked(self, tn):
        return tracked_members(self.kinds, self.enums, tn)

    def tracked_values(self, o):
        return {m: (("state", o.type, o.state) if m == "state" else o.attrs.get(m, ABSENT)) for m in self.tracked(o.type)}

    @staticmethod
    def check_interval(ctx, o, members):
        """A backdated change does not precede the current interval of a state or tracked value it changes (§4.8)."""
        for m in members:
            spans = o.spans.get(m) or []
            if spans and ctx.at < spans[-1]["entered_at"]:
                raise Refused("unsatisfied", "occurred_within", GENERATED_REMEDY["occurred_within"])

    def record(self, ctx, o, x, xn, pos, before, inputs, created, cause=None):
        work = ctx.work
        before_state, before_attrs, held = before
        now_values = self.tracked_values(o)
        for m, v in now_values.items():
            spans = o.spans.setdefault(m, [])
            if m in (x.get("corrects") or []) and spans and spans[-1]["left_at"] is ABSENT and held.get(m, ABSENT) != v:
                # a correction replaces the wrong value for the span it was held, explicitly: the span is relabelled and
                # carries the correction, and every value held then, by a span or an event of the object begun within it,
                # reads the corrected one; the original event is not changed (ADR-0146). Within it is by position, as
                # `.held` is read at the position of the transition that began a row (declaration-syntax.md §6.9)
                wrong, since = spans[-1]["value"], spans[-1].position
                spans[-1]["value"], spans[-1]["corrected_by"] = v, pos
                if len(spans) > 1 and spans[-2]["value"] == v:
                    # equal to the interval before it, it merges with it, so a correction never counts as a change of
                    # holder; the merged row carries the latest correction that reached it (ADR-0147)
                    spans[-2]["left_at"], spans[-2]["corrected_by"] = ABSENT, pos
                    spans.pop()
                for rows in o.spans.values():
                    for row in rows:
                        if row.position >= since and row["held"].get(m) == wrong:
                            row["held"][m], row["corrected_by"] = v, pos    # every row whose .held it changes (ADR-0147)
                for ev in work.events:
                    if ev["object"] == o.id and ev["position"] > since and ev["held"].get(m) == wrong:
                        ev["held"][m] = v
                held[m] = v     # its own row credits the corrected value, begun in the span it corrects (ADR-0147)
                continue
            if created or not spans or held.get(m, ABSENT) != v:
                if not created:
                    self.check_interval(ctx, o, {m})     # what an effect changed, which no guard could see
                if spans and spans[-1]["left_at"] is ABSENT:
                    spans[-1]["left_at"] = ctx.at
                key = "state" if m == "state" else "value"
                spans.append(span_row(pos, {"object": o.id, key: v, "entered_at": ctx.at, "left_at": ABSENT,
                                            "entered_by_kind": ctx.who["kind"], "declaration_version": work.version, "legacy": False,
                                            "migrated": False, "held": dict(now_values)}))
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
            if o.type == LABEL:
                subject.writes["labels"] = pos
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
            "occurred_at": ctx.at, "recorded_at": work.now, "actor_id": ctx.who["id"], "actor_kind": ctx.who["kind"],
            # an erasure is an override only where it leaves an invariant violated, which it admits (ADR-0146)
            "asserted": x["kind"] == "assertion",
            "overrides": x["kind"] == "assertion" or (x["kind"] == "erasure" and bool(self.failing_invariants(work, o))),
            "imported": False, "migrated": False,
            "redacted": False, "reason": reason, "declaration_version": work.version, "held": held,
            # what caused it, and the inputs it took by name, which an erasure's walk down follows (ADR-0160)
            "cause": cause, "inputs": sorted(inputs)})

    def loop_order(self, work, item):
        """A loop's order: objects by ascending id, which is creation order; values ascending, strings by code point,
        numbers by value and enumeration members in declaration order (DESIGN.md §6, ADR-0147)."""
        if isinstance(item, str) and item in work.objects:
            return (0, work.objects[item].id_order)
        if isinstance(item, tuple) and item and item[0] == "enum":
            members = list(self.enums.get(item[1]) or [])
            return (1, members.index(item[2]) if item[2] in members else len(members))
        return (2, item)

    def effects(self, ctx, tn, o, steps, inputs, env, depth):
        """An effect's steps in order; returns whether one reached another object by a call or a creation."""
        work, reached = ctx.work, False
        attrs = (self.kinds.get(tn) or {}).get("attributes") or {}

        def ev(tree, filtering=False, written=False):
            asking = Evaluator(work, tn, o.id, inputs, ctx.who, ctx.stubs, filter_mode=filtering,
                               event=ctx.position_of.get(o.id))
            v = asking.written(tree, env) if written else asking.value(tree, env)
            # an effect's value an arithmetic left with none, past 64 bits or a division by zero, refuses the request,
            # naming its object, as the core does (production review F5, ADR-0185)
            if v is ABSENT and asking.failed_arithmetic and not filtering:
                raise Refused("unsatisfied", "value_in_range", "unreachable_from_here", objects=[o.id])
            return v

        def unsupplied(tree):
            # a write whose right-hand side is an unsupplied optional input is skipped (DESIGN.md §5.4), and so is one of
            # the principal, which this runner never has, as the core skips it where a request names none (D687)
            return tree[0] == "path" and (tree[1] == ("name", "inputs") and tree[2] not in inputs
                                          or tree[1] == ("name", "actor") and tree[2] == "principal")

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
                v = ev(tree, written=str((attrs.get(spec["location"]) or {}).get("type") or "").startswith("decimal("))
                if v is ABSENT:
                    # arithmetic on an absent operand has no value, which refuses the request as one past its bound
                    # does, where it answered not run (fourteenth production review N5)
                    raise Refused("unsatisfied", "value_in_range", "unreachable_from_here", objects=[o.id])
                if self.owner_end(attrs.get(spec["location"])) and o.attrs.get(spec["location"]) not in (None, v):
                    raise NotRunnable("re-parenting a part by an effect, whose whole_open guard this runner does not yet evaluate")
                written = at_declared_scale(attrs.get(spec["location"]), v, o.id)
                # a write that would round a decimal to its target's scale loses what it holds, which §8.3 permits a
                # product or a quotient alone, as the core refuses it (the audit's values W5)
                if (str((attrs.get(spec["location"]) or {}).get("type") or "").startswith("decimal(")
                        and not (tree[0] == "bin" and tree[1] in ("*", "/"))
                        and isinstance(v, (int, Fraction)) and not isinstance(v, bool) and Fraction(written) != v):
                    raise Refused("unsatisfied", "value_in_range", "unreachable_from_here", objects=[o.id])
                # a decimal written past its p digits refuses the request, as an arithmetic past its bound does (ADR-0185)
                digits = precision_of((attrs.get(spec["location"]) or {}).get("type"))
                scale = int(str(attrs[spec["location"]]["type"]).rstrip(")").rsplit(",", 1)[1]) if digits else 0
                if digits and isinstance(written, (int, Fraction)) and abs(Fraction(written) * 10 ** scale) > 10 ** digits - 1:
                    raise Refused("unsatisfied", "value_in_range", "unreachable_from_here", objects=[o.id])
                # money past what an input holds, eighteen digits at its currency's minor unit, refuses the request, as
                # the core refuses it, so what a read answers a request can send (ADR-0191 decision 3)
                typ = str((attrs.get(spec["location"]) or {}).get("type") or "")
                if (typ.startswith("money(") and isinstance(written, (int, Fraction)) and not isinstance(written, bool)
                        and abs(half_even(written, MINOR_UNITS.get(typ[len("money("):-1], 2))
                                * 10 ** MINOR_UNITS.get(typ[len("money("):-1], 2)) > INPUT_UNITS):
                    raise Refused("unsatisfied", "value_in_range", "unreachable_from_here", objects=[o.id])
                # a time past the calendar refuses the request, as the core refuses it (ADR-0190 decision 1)
                if ((attrs.get(spec["location"]) or {}).get("type") == "timestamp"
                        and isinstance(written, (int, Fraction)) and not FIRST_SECOND <= written <= LAST_SECOND):
                    raise Refused("unsatisfied", "value_in_range", "unreachable_from_here", objects=[o.id])
                o.attrs[spec["location"]] = written
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
                    # an absent element refuses the request as an assignment of one does, as the core refuses it (the
                    # audit's values W6)
                    raise Refused("unsatisfied", "value_in_range", "unreachable_from_here", objects=[o.id])
                element = dict(attrs.get(spec["location"]) or {})
                element["type"] = str(element.get("type") or "").removesuffix("[]")
                v = at_declared_scale(element, v, o.id)
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
                    items = [] if n is ABSENT else list(range(1, int(n) + 1))   # absent or non-positive: no step (ADR-0146)
                else:
                    coll = ev(flowexpr.parse(spec["array"]))
                    if coll is ABSENT or not isinstance(coll, tuple):
                        raise NotRunnable("a loop over what is not a collection")
                    items = sorted(coll, key=lambda i: self.loop_order(work, i))
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
        return Refused("call_refused", inner_clause, r.remedy, detail=f"{step} on {target or 'a new object'}: {r.verdict}",
                       object_id=target, step=step, inner=r)

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
        # and a current value a later version no longer marks personal, judged by the event that last wrote it, its
        # name taken back through later renames (storage-schema.md §9 step 1, ADR-0146 decision 2)
        written_under = {e["position"]: e["declaration_version"] for e in work.events if e["object"] == o.id}
        erased |= {a for a in o.attrs if a not in erased and o.writes.get(a) in written_under
                   and self.personal_under(work, o.type, a, written_under[o.writes[a]])}
        # an actor's identity, which the store keeps beside the actor's object, taken before it is cleared and
        # replaced by the object's id once the request has written its own (ADR-0192 decision 4)
        marked = work.actor_attr(o.type)
        if marked and marked[0] in erased and isinstance(o.attrs.get(marked[0]), str):
            ctx.renamed.append((o.attrs[marked[0]], o.id))
        for a in erased:
            spec = (t.get("attributes") or {}).get(a)       # none for a dropped attribute's kept value
            if is_set_attribute(spec) if spec is not None else isinstance(o.attrs[a], tuple):
                o.attrs[a] = ()             # a personal set is emptied, a set being never absent (ADR-0148)
            else:
                del o.attrs[a]
        o.erased = True
        ctx.erased.setdefault(o.id, set()).update(erased)
        self.walk_down_from(ctx, o)
        # every pending proposal targeting the erased object is invalidated, its redaction and invalidation one event
        # (DESIGN.md §8, ADR-0147)
        for key, proposal in work.proposals.items():
            if proposal["state"] == "pending" and proposal["object"] == o.id:
                proposal["state"] = "invalidated"
                if key in work.objects:
                    held = work.objects[key].attrs.get("inputs")
                    names = set(json.loads(held)) if isinstance(held, str) and held.startswith("{") else set()
                    work.objects[key].attrs["inputs"] = "{}"
                    proposal_event(work, key, "invalidate", "PENDING", "INVALIDATED", ctx.who["id"])
                    ctx.erased.setdefault(key, set()).update(names)
        # and every one decided on it keeps its inputs but those personal to the transition it proposed, one redact
        # event each, which an erasure left in the proposal, its history and the log (thirteenth production review A)
        for key, proposal in work.proposals.items():
            if proposal["state"] == "pending" or proposal["object"] != o.id or key not in work.objects:
                continue
            p = work.objects[key]
            held = p.attrs.get("inputs")
            try:
                inputs = json.loads(held) if isinstance(held, str) else None
            except ValueError:
                inputs = None
            x = ((self.kinds.get(t_name := o.type) or {}).get("transitions") or {}).get(proposal["transition"])
            if not isinstance(inputs, dict) or x is None:
                continue
            personal = {k for k in inputs if input_personal(self.kinds.get(t_name) or {}, x, k)}
            if not personal:
                continue
            p.attrs["inputs"] = json.dumps({k: v for k, v in inputs.items() if k not in personal},
                                           sort_keys=True, separators=(",", ":"))
            proposal_event(work, key, "redact", p.state, p.state, ctx.who["id"])
            ctx.erased.setdefault(key, set()).update(personal)

    def personal_under(self, store, tn, a, version):
        """Whether attribute `a` of type `tn`, named as the installed version names it, was personal under a version:
        its name taken back through each later version's renames, and read in that version's declaration (ADR-0146
        decision 2). The installed version is the runner's own."""
        if version >= store.version:
            kinds = self.kinds
        else:
            kinds = (getattr(store, "history", None) or {}).get(version) or {}
            for u in range(store.version, version, -1):
                for base, frm, to in (getattr(store, "renames", None) or {}).get(u, ()):
                    if a == to and store.is_a(tn, base):
                        a = frm
        spec = ((kinds.get(tn) or {}).get("attributes") or {}).get(a)
        return bool((spec or {}).get("personal"))

    def identity_attrs(self, tn):
        """The attributes of type identity a type's objects hold, its bases' among them, where an erased actor's
        identity may be copied, however it got there, as the core's `Library::identity_held` (ADR-0203 decision 1)."""
        out, seen = set(), set()
        while tn and tn not in seen:
            seen.add(tn)
            k = self.kinds.get(tn) or {}
            out.update(a for a, spec in (k.get("attributes") or {}).items()
                       if isinstance(spec, dict) and spec.get("type") == "identity")
            tn = k.get("extends")
        return out

    def identity_inputs(self, tn):
        """The inputs of type identity a type's transitions take, their own or an identity attribute's, which its
        events record, as the core's `Library::identity_held`."""
        k = self.kinds.get(tn) or {}
        attrs = self.identity_attrs(tn)
        out = set()
        for x in (k.get("transitions") or {}).values():
            out.update(n for n, spec in (x.get("inputs") or {}).items()
                       if isinstance(spec, dict) and spec.get("type") == "identity")
            out.update(n for n in (x.get("required_inputs") or []) + (x.get("optional_inputs") or []) if n in attrs)
        return out

    def rename_erased_actors(self, ctx):
        """Each erased actor's identity, wherever the store keeps it, replaced by its object's id, which names no
        person: the events' actor, the attempts', and a proposal's or a change's recorded actors, this request's own
        among them, as the core's (ADR-0192 decision 4); and every copy of it of type identity, however it got there,
        an attribute, a set, an event's or a proposal's input, matched in any case of its ASCII letters as identities
        compare (ADR-0203 decision 1, ADR-0205 decision 6)."""
        work = ctx.work
        for identity, oid in getattr(ctx, "renamed", ()):
            def same(v):
                return isinstance(v, str) and v.lower() == identity.lower()

            def renamed_inputs(inputs, names):
                return {n: (oid if n in names and same(v) else v) for n, v in (inputs or {}).items()}
            for o in work.objects.values():
                spec = (self.kinds.get(o.type) or {}).get("attributes") or {}
                for a in self.identity_attrs(o.type):
                    if same(o.attrs.get(a)):
                        o.attrs[a] = oid
                for a, sp in spec.items():
                    if isinstance(sp, dict) and sp.get("type") == "identity[]" and isinstance(o.attrs.get(a), tuple):
                        kept = []
                        for v in o.attrs[a]:
                            v = oid if same(v) else v
                            if v not in kept:
                                kept.append(v)
                        o.attrs[a] = tuple(kept)
                if o.type == PROPOSAL and isinstance(o.attrs.get("inputs"), str):
                    try:
                        held = json.loads(o.attrs["inputs"])
                    except ValueError:
                        held = None
                    if isinstance(held, dict):
                        o.attrs["inputs"] = json.dumps(
                            renamed_inputs(held, self.identity_inputs(o.attrs.get("target_type"))))
            for e in work.events:
                if e.get("actor_id") == identity:
                    e["actor_id"] = oid
                o = work.objects.get(e.get("object"))
                if o is not None and isinstance(e.get("inputs"), dict):
                    tn = o.attrs.get("target_type") if o.type == PROPOSAL else o.type
                    e["inputs"] = renamed_inputs(e["inputs"], self.identity_inputs(tn))
            for row in getattr(work, "attempts", ()):
                if same(row.get("actor_id")):
                    row["actor_id"] = oid
            for o in work.objects.values():
                for a in ("proposer", "drafted_by", "approved_by"):
                    if o.type in (PROPOSAL, DECLARATION_CHANGE) and o.attrs.get(a) == identity:
                        o.attrs[a] = oid

    def clear_intervals(self, ctx):
        """Each erased object's intervals of an attribute personal now, or under the version that wrote the interval,
        holding no value, as the redacted events they are rebuilt from would give; run after the request has written
        its own (storage-schema.md §9 step 9, D540)."""
        work = ctx.work
        for oid in ctx.erased:
            o = work.objects.get(oid)
            if o is None or not o.erased:
                continue
            for d, rows in o.spans.items():
                if d == "state":
                    continue
                now = self.personal_under(work, o.type, d, work.version)
                for row in rows:
                    if row.get("value", ABSENT) is not ABSENT and (now or self.personal_under(
                            work, o.type, d, row.get("declaration_version", work.version))):
                        row["value"] = ABSENT

    # ── the erasure's walk down (storage-schema.md §9 step 12, ADR-0160 decision 2) ─────────────────────────────────
    def walk_down_from(self, ctx, o):
        """From each of the erased object's events, each event it caused on another object is followed: an input the
        causing step filled from a tainted input or a personal attribute is redacted there, marked personal or not, and
        a personal attribute it wrote is cleared while the object still holds what flowed in; the walk continues down.
        Every other object so redacted gets one `redact` event, caused by the erasure's event (ADR-0160)."""
        work, t = ctx.work, self.kinds.get(o.type) or {}
        holding = {a for a, sp in (t.get("attributes") or {}).items() if (sp or {}).get("personal")}
        redacted, cleared = {}, {}
        for e in [e for e in work.events if e["object"] == o.id]:
            x = (t.get("transitions") or {}).get(e["transition"]) or {}
            tainted = {n for n in e.get("inputs") or () if input_personal(t, x, n) or n in holding}
            self.walk_down(ctx, e, tainted, holding, redacted, cleared)
        for oid in sorted(set(redacted) | set(cleared), key=lambda i: work.objects[i].id_order):
            p = work.objects[oid]
            if oid == o.id or p.erased:
                continue                # an erased object holds nothing personal to redact
            for a in cleared.get(oid, ()):
                if is_set_attribute(((self.kinds.get(p.type) or {}).get("attributes") or {}).get(a) or {}):
                    p.attrs[a] = ()
                else:
                    p.attrs.pop(a, None)
            self.redact_event(ctx, p)

    @staticmethod
    def redact_event(ctx, p):
        """The event an erasure records on another object whose value it redacts, its version advanced, caused by the
        erasure (DESIGN.md §8)."""
        work = ctx.work
        work.position += 1
        p.version += 1
        state = ("state", p.type, p.state)
        work.events.append({
            "position": work.position, "object": p.id, "type": p.type, "transition": "redact",
            "from_state": state, "to_state": state, "completes": False, "returns": False,
            "occurred_at": ctx.at, "recorded_at": work.now, "actor_id": ctx.who["id"],
            "actor_kind": ctx.who["kind"], "asserted": False, "overrides": False, "imported": False,
            "migrated": False, "redacted": True, "reason": ABSENT, "declaration_version": work.version, "held": {},
            "cause": ctx.causes[-1] if ctx.causes else None, "inputs": []})

    def walk_down(self, ctx, e, tainted, holding, redacted, cleared):
        work = ctx.work
        fx = ((self.kinds.get(e["type"]) or {}).get("transitions") or {}).get(e["transition"]) or {}
        for f in [f for f in work.events if f.get("cause") == e["position"] and f["object"] != e["object"]]:
            p = work.objects.get(f["object"])
            if p is None:
                continue
            pt = self.kinds.get(p.type) or {}
            px = (pt.get("transitions") or {}).get(f["transition"]) or {}
            passed = {n for n, exprs in steps_taking(work, fx.get("effect") or [], f["transition"], p.type)
                      if any(carries(x, tainted, holding) for x in exprs)}
            next_holding = set()
            for n in passed:
                if n in (f.get("inputs") or ()):
                    redacted.setdefault(p.id, set()).add(n)
                for a in attributes_written(pt, px, n):
                    if not (((pt.get("attributes") or {}).get(a) or {}).get("personal")):
                        continue
                    next_holding.add(a)
                    if p.writes.get(a) == f["position"]:
                        cleared.setdefault(p.id, set()).add(a)
            if passed:
                self.walk_down(ctx, f, passed, next_holding, redacted, cleared)

    def forget_observations(self, ctx, o, inputs, depth):
        """An erasure reaches its object's observations through each one's generated `forget`, taken after its declared
        steps and cascades, in ascending id order, as transitions the request causes (DESIGN.md §8, ADR-0147)."""
        work = ctx.work
        for ob in ((self.kinds.get(o.type) or {}).get("observations") or {}).values():
            if "forget" not in ((self.kinds.get(ob["kind"]) or {}).get("transitions") or {}):
                continue                    # a kind with no personal field has nothing to forget
            for other in sorted((x for x in work.objects.values() if x.type == ob["kind"] and x.attrs.get("subject") == o.id),
                                key=lambda x: x.id_order):
                self.take(ctx, ob["kind"], "forget", other.id, {"reason": inputs.get("reason", "erasure")}, depth + 1)
        # and its labels' notes, corrected ones included, each with a redact event of the label's own, as the core
        # records it (declaration-syntax.md §6.8, the audit's write path A-01)
        for label in sorted(work.objects.values(), key=lambda x: x.id_order):
            if label.type == LABEL and label.attrs.get("subject") == o.id and "note" in label.attrs:
                del label.attrs["note"]
                ctx.erased.setdefault(label.id, set()).add("note")
                self.redact_event(ctx, label)

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
                continue                # a member holding nothing personal declares no erasure and is passed over (ADR-0147)
            # each member's erasure receives the requested erasure's inputs by name (ADR-0146), and a member's refusal
            # refuses the request as a caused refusal does, its step `group erasure` (ADR-0147)
            takes = set(((self.kinds.get(m.type) or {}).get("transitions") or {})[erasures[0]].get("inputs") or {})
            try:
                self.take(ctx, m.type, erasures[0], member, {k: v for k, v in inputs.items() if k in takes}, depth + 1)
            except Refused as r:
                raise self.call_refused(r, "group erasure", m.type, member)

    def invariant_remedy(self, work, x, obj, n, requested, xn=None):
        """The remedy DESIGN.md §5.5 infers for an invariant that fails, a property of the transition requested:
        `delegable` where it writes a value from the actor that the invariant reads, since another actor taking it can
        pass (ADR-0173); else `self_serviceable` where it takes an input it writes to a value the invariant reads, an
        attribute input writing its attribute and an assertion's `to` the state; else `dependent` where the invariant
        reads another object, through a relationship, a type scan or `this`; else `unreachable_from_here`. What it
        writes to the requested object is its inputs and its effect's; to an object it creates or calls, one hop away,
        that step's inputs and the effect of the transition the step takes (D547). A uniqueness failing on another
        object of the family reads the requested one through its scan."""
        mine = work.objects[requested].type
        unique_other = obj.id != requested and n.endswith("_unique") and \
            work.is_a(mine, self.unique_scope(obj.type, n[:-len("_unique")]))
        t = self.kinds.get(obj.type) or {}
        attrs = t.get("attributes") or {}
        if n == "identity_taken":
            # what another actor holds, as a uniqueness reads it (ADR-0190 decision 4)
            reads, other = self.reads(obj.type, n), True
        elif n.endswith("_unique"):
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
                    other = True                # a scan relative to the object reads others
                    continue
                if m in (t.get("derived_attributes") or {}):
                    names |= names_read(parsed(t["derived_attributes"][m]["expression"]))
                    continue
                if m == "state" or m in attrs or m in (t.get("observations") or {}):
                    reads.add(m)
                other |= m in (t.get("observations") or {}) or "reference" in (attrs.get(m) or {}) \
                    or m == "referrers" or (m in self.kinds and m not in (t.get("states") or {}))
        writes, by_actor = self.writes_of(x, obj.type, obj.id == requested or unique_other,
                                          (mine, xn) if xn else None)
        if by_actor & reads:
            return "delegable"
        if writes & reads:
            return "self_serviceable"
        return "dependent" if other else "unreachable_from_here"

    def actor_dependent(self, tn, xn):
        """Whether an offer of tn's transition xn may be refused for who takes it: xn writes a value from the actor
        that an invariant of tn reads, or of a type it creates, one hop away; availability runs no effects, so it
        cannot tell, and says so (ADR-0173 decision 1)."""
        def read_by(t):
            names = set()
            for spec in (t.get("invariants") or {}).values():
                names |= names_read(parsed(spec["expression"]))
            todo = list(names)
            while todo:                         # a derived attribute an invariant reads reads what it reads
                d = (t.get("derived_attributes") or {}).get(todo.pop())
                if d:
                    more = names_read(parsed(d["expression"])) - names
                    names |= more
                    todo += list(more)
            return names
        t = self.kinds.get(tn) or {}
        x = (t.get("transitions") or {}).get(xn)
        if x is None:
            return False
        if self.writes_of(x, tn, True)[1] & read_by(t):
            return True

        def created(steps):
            for st in steps or []:
                kind = next(iter(st))
                if kind == "create":
                    yield st["create"].get("type")
                elif kind == "foreach":
                    yield from created(st["foreach"].get("steps"))
        return any(self.writes_of(x, k, False)[1] & read_by(self.kinds.get(k) or {})
                   for k in set(created(x.get("effect"))) if k in self.kinds)

    def writes_of(self, x, tn, requested, cascading=None):
        """What transition x writes to an object of type tn, from its inputs and from its actor: to the requested
        object, its inputs and its effect's writes; to an object it creates or calls, one hop away, that step's inputs
        and the effect of the transition the step takes (DESIGN.md §5.5, D547, ADR-0173)."""
        writes, by_actor = set(), set()

        def effect(steps):
            # an effect's step that writes a value computed from an input writes that input to it (ADR-0122
            # decisions 33 and 36), and one computed from the actor writes the actor to it
            for st in steps or []:
                kind = next(iter(st))
                if kind in ("assign", "add", "remove"):
                    read = names_read(parsed(st[kind]["expr"]))
                    if "inputs" in read:
                        writes.add(st[kind]["location"])
                    if "actor" in read:
                        by_actor.add(st[kind]["location"])
                elif kind == "foreach":
                    effect(st["foreach"].get("steps"))

        def caused(steps):
            for st in steps or []:
                kind = next(iter(st))
                if kind == "foreach":
                    caused(st["foreach"].get("steps"))
                    continue
                if kind not in ("create", "call"):
                    continue
                body = st[kind]
                taken = ((self.kinds.get(tn) or {}).get("transitions") or {}).get(body.get("transition"))
                if taken is None or kind == "create" and body.get("type") != tn:
                    continue
                for name, e in (body.get("inputs") or {}).items():
                    read = names_read(parsed(str(e)))
                    if "inputs" in read:
                        writes.add(name)
                    if "actor" in read:
                        by_actor.add(name)
                for st2 in taken.get("effect") or []:
                    k2 = next(iter(st2))
                    if k2 in ("assign", "add", "remove") and "actor" in names_read(parsed(st2[k2]["expr"])):
                        by_actor.add(st2[k2]["location"])
        if requested:
            writes |= set(x.get("required_inputs", [])) | set(x.get("optional_inputs", []))
            if x["kind"] == "assertion":
                writes.add("state")
            effect(x.get("effect"))
        else:
            caused(x.get("effect"))
            # a part's transition its whole's cascade takes, one hop away as a call's is: the clause's inputs, and
            # what that transition's effect writes from the actor (the audit's write path A-04)
            if cascading:
                owner, xn = cascading

                def is_a(t, want):
                    seen = set()
                    while t and t not in seen:
                        if t == want:
                            return True
                        seen.add(t)
                        t = (self.kinds.get(t) or {}).get("extends")
                    return False
                for end in ((self.kinds.get(owner) or {}).get("attributes") or {}).values():
                    part = str(end.get("reference") or "").rstrip("[]")
                    if end.get("aggregation") != "composite" or not part or not is_a(tn, part):
                        continue
                    for clause in end.get("cascade") or []:
                        if xn not in (clause.get("on") or []):
                            continue
                        taken = ((self.kinds.get(tn) or {}).get("transitions") or {}).get(clause.get("transition"))
                        if taken is None:
                            continue
                        for name, e in (clause.get("inputs") or {}).items():
                            read = names_read(parsed(str(e)))
                            if "inputs" in read:
                                writes.add(name)
                            if "actor" in read:
                                by_actor.add(name)
                        for st2 in taken.get("effect") or []:
                            k2 = next(iter(st2))
                            if k2 in ("assign", "add", "remove") and "actor" in names_read(parsed(st2[k2]["expr"])):
                                by_actor.add(st2[k2]["location"])
        return writes, by_actor

    def erasure_broke(self, ctx, obj, n):
        """Whether invariant n fails on obj because this request erased what it reads: obj's own attribute, or, one hop
        away, an object a reference it reads names (§4.13, ADR-0147). The runner follows one hop."""
        read = self.reads(obj.type, n)
        if obj.id in ctx.erased and read & ctx.erased[obj.id]:
            return True
        return any(isinstance(obj.attrs.get(r), str) and obj.attrs[r] in ctx.erased for r in read)

    def reads(self, tn, n):
        """The attributes an invariant reads, generated ones included: an attribute's uniqueness, a state's requirement."""
        t = self.kinds.get(tn) or {}
        if n == "identity_taken":
            mine = self.actor_attr(tn)
            return {mine[0]} if mine else set()
        if n.endswith("_unique"):
            return {n[:-len("_unique")]}
        if n.endswith("_invariant"):
            state = {s.lower(): v for s, v in (t.get("states") or {}).items()}.get(n[:-len("_invariant")]) or {}
            return set(state.get("required_attributes") or [])
        inv = (t.get("invariants") or {}).get(n)
        return names_read(flowexpr.parse(inv["expression"])) if inv else set()

    def invariant_objects(self, store, o, n):
        """The objects in conflict where an invariant fails on an object, chosen as an unsatisfied clause's are, the
        object excluded: what a declared invariant's references evaluate to, and the other holders of a value a
        uniqueness requires be one's own (DESIGN.md §5.5, ADR-0122)."""
        t = self.kinds.get(o.type) or {}
        invariants = t.get("invariants") or {}
        if n in invariants:
            e = Evaluator(store, o.type, o.id).noting(o.id)
            e.value(flowexpr.parse(invariants[n]["expression"]), {})
            return set(e.naming["objects"])
        if n == "identity_taken":
            return {x.id for x in self.identity_holders(store, o)}
        if n.endswith("_unique"):
            return {x.id for x in self.unique_holders(store, o, n[:-len("_unique")])}
        return set()

    def unique_holders(self, store, o, a):
        """The other objects holding the value, or values, a uniqueness of the attribute requires be the object's own;
        none where it does not apply, or holds."""
        t = self.kinds.get(o.type) or {}
        spec = (t.get("attributes") or {}).get(a) or {}
        unique = spec.get("unique")
        if unique is None and "reference" in spec and "opposite" in spec and not spec["reference"].endswith("[]"):
            back = ((self.kinds.get(spec["reference"]) or {}).get("attributes") or {}).get(spec["opposite"]) or {}
            unique = True if back.get("reference") and not back["reference"].endswith("[]") else None
        if unique is None:
            return []
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
        scope = self.unique_scope(o.type, a)
        if mine is None:
            return []
        return [x for x in sorted(store.objects.values(), key=lambda x: x.id_order)
                if x.id != o.id and store.is_a(x.type, scope) and key(x) == mine]

    def failing_invariants(self, store, o):
        """An object's failing invariants in the order its type declares them: its own declared invariants as written,
        then inherited ones, then the generated, each state's requirement and then each attribute's uniqueness by
        attribute, inherited attributes first (flow-format.md §4.8, ADR-0147)."""
        t = self.kinds.get(o.type) or {}
        out, generated = [], []
        invariants = t.get("invariants") or {}
        origin = t.get("_from") or {}
        for n in [n for n in invariants if ("invariants", n) not in origin] + [n for n in invariants if ("invariants", n) in origin]:
            if truth(Evaluator(store, o.type, o.id).value(flowexpr.parse(invariants[n]["expression"]), {})) is False:
                out.append(n)
        st = (t.get("states") or {}).get(o.state) or {}
        if any(a not in o.attrs for a in st.get("required_attributes") or []):
            out.append(f"{o.state.lower()}_invariant")
        for a in (t.get("attributes") or {}):
            if self.unique_holders(store, o, a):
                out.append(f"{a}_unique")
        # an actor's identity names one actor, across every type whose objects are actors (ADR-0190 decision 4)
        mine = self.actor_attr(o.type)
        if mine and f"{mine[0]}_unique" not in out and self.identity_holders(store, o):
            out.append("identity_taken")
        return out

    def actor_attr(self, tn):
        """The attribute holding an actor's identity, and its kind, if the type's objects are actors, as the store reads
        it from the same kinds."""
        return Store.actor_attr(self, tn)

    def identity_holders(self, store, o):
        """The other actors, of any type whose objects are actors, holding an object's identity (ADR-0190 decision 4)."""
        mine = self.actor_attr(o.type)
        v = o.attrs.get(mine[0]) if mine else None
        if not isinstance(v, str):
            return []
        # compared without the case of its ASCII letters, as the core's (ADR-0192 decision 6, ADR-0194 decision 1)
        return [x for x in store.objects.values()
                if x.id != o.id and (theirs := self.actor_attr(x.type))
                and isinstance(x.attrs.get(theirs[0]), str) and ascii_fold(x.attrs[theirs[0]]) == ascii_fold(v)]

    def unique_scope(self, tn, a):
        """The type whose family a uniqueness spans: the one declaring the attribute, so a base's spans every subtype,
        siblings included (flow-format.md §4.14, D519)."""
        return ((self.kinds.get(tn) or {}).get("_from") or {}).get(("attributes", a), tn)


def person_dimensions(store, tn):
    """Each metric dimension of a type whose value names an actor, which `declaration` marks so an upper layer can
    keep a person's figures behind its own door (library-api.md §6, ADR-0114, D451), as `<Type>.<metric>.<dimension>`
    in ascending order: over the type's own metrics and the standard ones, an assignment metric once for each assignee
    reference. A value names an actor where it is an object of a type marking an `actor` identity, a tracked
    reference's value in an interval of it included, or an actor's id; a combined metric's dimensions are its inputs',
    marked there. Its rows are never read: a row's kind is its source's."""
    t = store.kinds.get(tn) or {}
    assignees = [a for a, sp in (t.get("attributes") or {}).items() if (sp or {}).get("assignee")]
    # the type's own metrics, declared under it in its module (flow-format.md §4.9)
    names = [n for ms in store.decl.metrics.values() for n, (owner, _spec) in ms.items() if owner == tn]
    names += [n for n, spec in STANDARD.items() if "input_metrics" not in spec]
    names += [f"{n}.{r}" for n, spec in ASSIGNMENT.items() if "input_metrics" not in spec for r in assignees]
    ev = Evaluator(store, tn)
    out = []
    for n in names:
        owner, spec, _module = store.find_metric(parsed(f"{tn}.{n}"), tn)
        if "input_metrics" in spec:
            continue
        f = formula(store.kinds, store.enums, owner, spec)
        row = ev.row_kind(owner, f["source"])
        for d, x in (f.get("dimensions") or {}).items():
            e = parsed(x)
            if e[0] == "path" and e[2] == "actor_id":
                out.append(f"{tn}.{n}.{d}")         # an actor's id, a transition's or an attempt's
                continue
            if e == ("path", ("name", f["item"]), "value") and f["source"].startswith("intervals("):
                member = f["source"][len("intervals("):-1].strip()
                spec_m = ((store.kinds.get(owner) or {}).get("attributes") or {}).get(member) or {}
                kind = ("object", str(spec_m["reference"]).removesuffix("[]")) if "reference" in spec_m else ("other",)
            else:
                kind = ev.kind(e, {f["item"]: row})
            if kind[0] == "object" and store.actor_attr(kind[1]) is not None:
                out.append(f"{tn}.{n}.{d}")
            elif (rows := row_type(store.kinds, owner, f["source"])) and \
                    identity_path(store.kinds, rows, f["item"], e):
                # an actor's identity, as `actor.id` records it (D565), on an object of the type or an observation
                # of one of its kinds (the audit's personal data R8)
                out.append(f"{tn}.{n}.{d}")
    return sorted(out)


def row_type(kinds, owner, source):
    """The type a metric's rows are objects of: the owner's for `objects`, an observation kind's for a source naming
    one of the owner's kinds; none for rows that are no objects."""
    if source == "objects":
        return owner
    return (((kinds.get(owner) or {}).get("observations") or {}).get(source) or {}).get("kind")


def reads_only_its_rows(e, bound, kinds, enums):
    """Whether every name a read's filter starts from is one it may: the metric's item, an aggregation's own, `now`,
    a type, an enumeration or a kind of actor (D569)."""
    if not isinstance(e, tuple) or not e:
        return True
    if e[0] == "name":
        return e[1] in bound or e[1] in ("now",) + ACTOR_KINDS or e[1] in kinds or e[1] in enums
    if e[0] == "agg" and isinstance(e[3], str):
        bound = bound | {e[3]}
    return all(reads_only_its_rows(p, bound, kinds, enums) for p in e[1:] if isinstance(p, tuple)) and \
        all(reads_only_its_rows(q, bound, kinds, enums) for p in e[1:] if isinstance(p, list) for q in p)


def identity_path(kinds, owner, item, e):
    """Whether a metric's dimension reads an `identity` attribute of its row's object, or of one its single
    references reach: an actor's identity, as `assign … actor.id` records who acted (D565)."""
    def type_of(x):
        if x == ("name", item):
            return owner
        if x[0] == "path":
            base = type_of(x[1])
            spec = (((kinds.get(base) or {}).get("attributes") or {}).get(x[2]) or {}) if base else {}
            ref = spec.get("reference", "")
            return ref if ref and not ref.endswith("[]") else None
        return None
    if e[0] != "path":
        return False
    base = type_of(e[1])
    spec = (((kinds.get(base) or {}).get("attributes") or {}).get(e[2]) or {}) if base else {}
    return spec.get("type") == "identity"


def observation_kinds(modules):
    """Each observation kind as the type it expands to: a record creation taking its fields and the subject."""
    kinds = {}
    for m in modules.values():
        for tn, t in (m.get("types") or {}).items():
            for ob in (t.get("observations") or {}).values():
                fields = ob.get("attributes") or {}
                kinds[ob["kind"]] = {
                    "_observation_on": tn,
                    "attributes": dict(fields, subject={"reference": tn}, corrects={"reference": ob["kind"], "optional": True},
                                       recorded_at={"type": "timestamp"}, occurred_at={"type": "timestamp"}),
                    "states": {"RECORDED": {"category": "closed", "final": True}},
                    "invariants": ob.get("invariants") or {},
                    "transitions": {"record": {"kind": "initial", "to": "RECORDED",
                                               "required_inputs": ["subject"] + [a for a, f in fields.items() if not f.get("optional")],
                                               "optional_inputs": ["corrects"] + [a for a, f in fields.items() if f.get("optional")]}}}
                if any((f or {}).get("personal") for f in fields.values()):
                    # a kind with a personal field has a generated erasure, `forget` (declaration-syntax.md §6.8)
                    kinds[ob["kind"]]["transitions"]["forget"] = {"kind": "erasure", "inputs": {"reason": {"type": "string"}}}
                if ob.get("max_recording_delay"):
                    # the model's `occurred within`, the bound a recording's occurred time is held to (flow-format.md §4.4)
                    kinds[ob["kind"]]["transitions"]["record"]["backdating_limit"] = ob["max_recording_delay"]
                if ob.get("description"):
                    # what the observation is, which its kind's declaration says as the core's does
                    kinds[ob["kind"]]["description"] = ob["description"]
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


def run_examples(doc, library, modules, file=None, read=None, flows=None, source=None):
    """Each example's outcome: (name, "passed" | "failed" | "not run", message). A `publish` step reads the files it
    names through `read`, each beside the examples file `file`, and runs its dry run's checks with `flows` over the
    version `source` holds; one given no reader is not run (ADR-0166 decision 3)."""
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

    publishing = (file, read, flows)
    for name, ex in (doc.get("examples") or {}).items():
        runner = Runner(kinds, enums)       # each example from the version checked, whatever one before it published
        store = Store(kinds, enums, declared, homes)
        store.source = dict(source or {})
        aliases = {OPERATOR: OPERATOR}
        stubs = {}
        try:
            parts = (chain(ex["given"]) if "given" in ex else []) + [("example " + name, ex)]
            for label, part in parts:
                stubs.update(part.get("evaluators") or {})
                for i, st in enumerate(part.get("steps") or []):
                    try:
                        step(runner, store, st, aliases, stubs, publishing=publishing)
                    except Refused as r:
                        raise ExampleError(f"{label}'s step {i + 1} does not apply: it is refused as {r}")
            tested = {"approve": ex["approve"]} if "approve" in ex else {"request": ex["request"]}
            outcome = step(runner, store, tested, aliases, stubs, under_test=True)
            problem = compare(runner, store, ex["expect"], outcome, aliases)
            for name_, want in (ex["expect"].get("metrics") or {}).items():
                problem = problem or metric_differs(runner, store, name_, want, aliases)
            if problem is None and "proposal" in ex["expect"]:
                got = store.proposals[aliases[ex["approve"]["proposal"]][1]]["state"]
                if got != ex["expect"]["proposal"]:
                    problem = f"expected the proposal {ex['expect']['proposal']}, and it is {got}"
            results.append((name, "failed" if problem else "passed", problem or ""))
        except NotRunnable as e:
            results.append((name, "not run", f"it reaches {e}, which the runner does not yet execute"))
        except ExampleError as e:
            results.append((name, "failed", str(e)))
    return results


def advance(store, st):
    """The clock moved by a step's `after`, before the step."""
    if st.get("after"):
        n, unit = st["after"].split()
        store.now += Fraction(int(n)) * SECONDS[unit]


class ImportRefused(ExampleError):
    """An object an import could not place, and why (ADR-0154)."""


def scope_spec(kinds, sequence):
    """The attribute a sequence is scoped by, where an identifier minted from it is scoped (flow-format.md §4.12)."""
    for t in kinds.values():
        for spec in (t.get("attributes") or {}).values():
            ident = (spec or {}).get("identifier") or {}
            if ident.get("sequence") == sequence and ident.get("scope"):
                return (t.get("attributes") or {}).get(ident["scope"])
    return None


def raise_sequence(store, sequence, last, scope):
    """An import raises a sequence past the last number a legacy system minted from it in a scope, never lowering it,
    so the next number minted follows it (publish-and-import.md §4, ADR-0103)."""
    key = (sequence, scope)
    store.sequences[key] = max(store.sequences.get(key, 0), last)


def import_object(runner, store, tn, state, given):
    """One new object of a mirror type, written through the built-in assertion as an import writes it, and returned
    by id (publish-and-import.md §4). An example's import admits nothing, so an object whose state its type does not
    declare, which leaves a required attribute out, or which violates an invariant is refused, the store left as it
    was (ADR-0154, D526). Its intervals open at the import, by a changer of kind `unknown` (ADR-0154, C28)."""
    kind = runner.kinds.get(tn) or {}
    if not kind.get("mirror"):
        raise ImportRefused(f"{tn} is owned by this store, not a mirror, and the import writes only a mirror")
    if state not in (kind.get("states") or {}):
        raise ImportRefused(f"{tn} declares no state {state}")
    attrs = kind.get("attributes") or {}
    for k in given:
        if k not in attrs:
            raise ImportRefused(f"{tn} declares no attribute {k}")
    counter = store.counter
    o = Obj(store.new_id(tn), tn, state, store.now)
    o.id_order = store.counter
    o.created_by_kind = "unknown"
    o.attrs.update({a: () for a, spec in attrs.items() if is_set_attribute(spec)})
    o.attrs.update({k: v for k, v in given.items() if v is not ABSENT})
    store.objects[o.id] = o
    left_out = [a for a, spec in attrs.items() if not (spec or {}).get("optional") and "default" not in (spec or {})
                and (spec or {}).get("type") != "counter" and not computed_end(runner.kinds, spec) and a not in o.attrs]
    failing = left_out and [f"{left_out[0]}_provided"] or runner.failing_invariants(store, o)
    if failing:
        del store.objects[o.id]
        store.counter = counter
        raise ImportRefused(f"the imported {tn} fails {failing[0]}, and an example's import admits nothing")
    store.position += 1
    values = runner.tracked_values(o)
    for m, v in values.items():
        o.spans[m] = [span_row(store.position, {"object": o.id, "state" if m == "state" else "value": v,
                                                "entered_at": store.now, "left_at": ABSENT, "entered_by_kind": "unknown",
                                                "declaration_version": store.version, "legacy": False, "migrated": False,
                                                "held": dict(values)})]
    for a in o.attrs:
        o.writes[a] = store.position
    store.events.append({"position": store.position, "object": o.id, "type": o.type, "transition": "import", "from_state": ABSENT,
                         "to_state": ("state", o.type, o.state), "completes": False, "returns": False,
                         "occurred_at": store.now, "recorded_at": store.now, "actor_id": OPERATOR, "actor_kind": "human",
                         "asserted": True, "overrides": False, "imported": True, "migrated": False, "redacted": False,
                         "reason": ABSENT, "declaration_version": store.version, "held": {}})
    o.version += 1
    return o.id


def refresh_object(runner, store, oid, tn, state, given):
    """An object an earlier import gave, refreshed as import_batch refreshes an object whose legacy key the store holds
    (publish-and-import.md §4, ADR-0176 decision 4): its state and the values given written through the built-in
    assertion, the rest, and a value given as null, left as they are; each tracked value it changes closes its interval
    and opens the next at the import, by a changer of kind `unknown`. One that changes nothing writes nothing, and one
    leaving an invariant failing is refused, an example's import admitting nothing (ADR-0154, D526)."""
    o = store.objects[oid]
    if tn != o.type:
        raise ImportRefused(f"its legacy key names a {o.type}")
    kind = runner.kinds.get(tn) or {}
    if state not in (kind.get("states") or {}):
        raise ImportRefused(f"{tn} declares no state {state}")
    attrs = kind.get("attributes") or {}
    for k in given:
        if k not in attrs:
            raise ImportRefused(f"{tn} declares no attribute {k}")
    changes = {k: v for k, v in given.items() if v is not ABSENT and o.attrs.get(k, ABSENT) != v}
    if state == o.state and not changes:
        return oid
    kept = copy.deepcopy(o)
    before = runner.tracked_values(o)
    from_state = o.state
    o.state = state
    o.attrs.update(changes)
    failing = runner.failing_invariants(store, o)
    if failing:
        store.objects[oid] = kept
        raise ImportRefused(f"the refreshed {tn} fails {failing[0]}, and an example's import admits nothing")
    store.position += 1
    values = runner.tracked_values(o)
    for m, v in values.items():
        spans = o.spans.setdefault(m, [])
        if before.get(m, ABSENT) != v:
            if spans and spans[-1]["left_at"] is ABSENT:
                spans[-1]["left_at"] = store.now
            spans.append(span_row(store.position, {"object": o.id, "state" if m == "state" else "value": v,
                                                   "entered_at": store.now, "left_at": ABSENT, "entered_by_kind": "unknown",
                                                   "declaration_version": store.version, "legacy": False,
                                                   "migrated": False, "held": dict(values)}))
    for a in changes:
        o.writes[a] = store.position
    store.events.append({"position": store.position, "object": o.id, "type": o.type, "transition": "import",
                         "from_state": ("state", o.type, from_state), "to_state": ("state", o.type, o.state),
                         "completes": False, "returns": False, "occurred_at": store.now, "recorded_at": store.now,
                         "actor_id": OPERATOR, "actor_kind": "human", "asserted": True, "overrides": False,
                         "imported": True, "migrated": False, "redacted": False, "reason": ABSENT,
                         "declaration_version": store.version, "held": {}})
    o.version += 1
    return oid


def step(runner, store, st, aliases, stubs, under_test=False, publishing=None):
    """One step of a setup or the request under test; a setup's refusal propagates, the tested one is returned."""
    if "publish" in st:
        publish_step(runner, store, st["publish"], aliases, *(publishing or (None, None, None)))
        return None
    if "import" in st:
        im = st["import"]
        if im.get("after"):
            n, unit = im["after"].split()
            store.now += Fraction(int(n)) * SECONDS[unit]
        attrs = (runner.kinds.get(im["type"]) or {}).get("attributes") or {}
        given = {k: runner.value_of(store, attrs.get(k), v, aliases) for k, v in (im.get("values") or {}).items()}
        if im["as"] in aliases:
            # an alias an earlier import gave: the other system's later record of the same object (ADR-0176 decision 4)
            refresh_object(runner, store, aliases[im["as"]], im["type"], im["state"], given)
        else:
            aliases[im["as"]] = import_object(runner, store, im["type"], im["state"], given)
        for rs in im.get("sequences") or []:
            raise_sequence(store, rs["sequence"], rs["last"],
                           runner.value_of(store, scope_spec(runner.kinds, rs["sequence"]), rs["scope"], aliases)
                           if "scope" in rs else None)
        return None
    if "propose" in st:
        r = st["propose"]
        oid = aliases[r["object"]] if "object" in r else None
        tn = store.type_of(oid) if oid else r["type"]
        try:
            return file_proposal(runner, store, st, aliases)
        except Refused as refusal:
            store.attempts.append(attempt_row(store, tn, oid, r["transition"], aliases.get(r.get("actor", OPERATOR), r.get("actor")), refusal))
            raise
    return decide_or_request(runner, store, st, aliases, stubs, under_test)


def publish_step(runner, store, p, aliases, file, read, flows):
    """An example's `publish` (flow-format.md §11.3, ADR-0166 decision 3): a change drafted with the files the step
    names, each read beside the examples file and keyed by the name it declares, its source reference the examples
    file's, then submitted, which runs the dry run, and approved as the step's actor, the operator by default; a refusal
    at any of the three fails the example, and the runner then decides every later step under the version installed."""
    import flowpublish
    advance(store, p)
    source = {}
    for f in p["source"]:
        text = read(f) if read is not None else None
        if text is None:
            raise NotRunnable(f"the publish step's file {f}, which no directory beside the examples file gives")
        source[flowpublish.declared_name(text) or f] = text
    actor = aliases.get(p.get("actor", OPERATOR), p.get("actor"))
    try:
        drafted = flowpublish.request(store, "draft", None, actor, {"source": source, "source_ref": f"example:{file}"},
                                      flows)
        change = drafted[1]
    except Refused as r:
        raise ExampleError(f"the publish's change is not drafted: it is refused as {r}")
    try:
        flowpublish.request(store, "submit", change, actor, {}, flows)
    except Refused as r:
        raise ExampleError(f"the publish is refused at submission as {r}")
    result, _report = flowpublish.approve(store, change, actor, store.objects[change].version, flows)
    if isinstance(result, Refused):
        raise ExampleError(f"the publish is refused at approval as {result}")
    runner.__init__(store.kinds, store.enums)


def file_proposal(runner, store, st, aliases):
    """A request filed for approval instead of executed, held as it was asked (DESIGN.md §9, ADR-0036)."""
    if True:
        r = st["propose"]
        advance(store, r)
        oid = aliases[r["object"]] if "object" in r else None
        tn = store.type_of(oid) if oid else r["type"]
        x = ((runner.kinds.get(tn) or {}).get("transitions") or {}).get(r["transition"])
        if x is None:
            raise Refused("unknown_transition", remedy="self_serviceable")
        if not x.get("proposable"):
            raise Refused("not_proposable", remedy="self_serviceable")    # send it as a request instead (ADR-0173)
        # filing checks the inputs as a request would, and leaves the object's state and the guards to approval (ADR-0146)
        attrs, declared = (runner.kinds.get(tn) or {}).get("attributes") or {}, x.get("inputs") or {}
        taken = set(x.get("required_inputs", [])) | set(x.get("optional_inputs", [])) | set(declared)
        given_inputs = r.get("inputs") or {}
        wrong = [(k, "unknown_input") for k in sorted(given_inputs) if k not in taken]
        for k in sorted(given_inputs):
            if k not in taken:
                continue
            v, spec = given_inputs[k], declared.get(k) if k in declared else attrs.get(k)
            why = input_fault(spec, v, runner.enums, runner.kinds)
            if why is None and "reference" in (spec or {}) and not all(
                    a in aliases and store.is_a(store.type_of(aliases[a]), spec["reference"].rstrip("[]"))
                    for a in (v if isinstance(v, (list, tuple)) else [v])):
                why = "no_such_object"
            if why:
                wrong.append((k, why))
        given = {k for k, v in given_inputs.items() if v is not None}
        wrong += [(k, "missing") for k in x.get("required_inputs", []) if k not in given
                  and not (attrs.get(k) or {}).get("optional") and "default" not in (attrs.get(k) or {})]
        wrong += [(k, "missing") for k, sp in declared.items() if k not in given and not (sp or {}).get("optional")
                  and "default" not in (sp or {})]
        if wrong:
            raise Refused("invalid_input", remedy="self_serviceable", inputs=[k for k, _ in wrong], reasons=[w for _, w in wrong])   # as a request would (ADR-0147)
        # a proposal keeps the occurred time the request gives, held to the bound when it is filed, since that is when
        # the store was told; the check against the current interval runs at approval (ADR-0147)
        occurred = runner.value_of(store, {"type": "timestamp"}, r["occurred_at"], aliases) if "occurred_at" in r else None
        if occurred is not None:
            limit = x.get("backdating_limit")
            if limit is None or not store.now - seconds(limit) <= occurred <= store.now:
                raise Refused("unsatisfied", "occurred_within", GENERATED_REMEDY["occurred_within"])
        key = store.new_id(PROPOSAL)
        proposer = aliases.get(r.get("actor", OPERATOR), r.get("actor"))
        p = Obj(key, PROPOSAL, "PENDING", store.now)
        p.id_order = store.counter
        p.created_by_kind = (store.actor_attr(store.type_of(proposer)) or (None, "unknown"))[1] \
            if proposer in store.objects else "unknown"
        # the proposer by its identity, as the core records it and an event names its actor (D611)
        marked = store.actor_attr(store.type_of(proposer)) if proposer in store.objects else None
        held = {"target": oid, "target_type": tn, "transition": r["transition"],
                "inputs": json.dumps(r.get("inputs") or {}, sort_keys=True, separators=(",", ":")),
                "proposer": store.objects[proposer].attrs.get(marked[0], proposer) if marked else proposer,
                "occurred": occurred, "filed_at": store.now}
        p.attrs.update({k: v for k, v in held.items() if v is not None})
        store.objects[key] = p
        proposal_event(store, key, "propose", None, "PENDING", proposer)
        store.proposals[key] = {"type": tn, "object": oid, "transition": r["transition"], "inputs": r.get("inputs"),
                                "aliases": dict(aliases), "proposer": aliases.get(r.get("actor", OPERATOR), r.get("actor")),
                                "state": "pending", "verdict": None, "occurred": occurred, "filed_at": store.now}
        aliases[r["proposal"]] = ("proposal", key)
        return None


def decide_or_request(runner, store, st, aliases, stubs, under_test):
    """A decision on a proposal, or a request (DESIGN.md §6, §9)."""
    for verb in ("approve", "reject", "withdraw"):
        if verb not in st:
            continue
        d = st[verb]
        advance(store, d)
        key = aliases[d["proposal"]][1]
        actor = aliases.get(d.get("actor", OPERATOR), d.get("actor"))
        try:
            try:
                if actor not in store.objects or store.actor_attr(store.type_of(actor)) is None:
                    raise Refused("unsatisfied", "actor_known", "dependent")
                if store.final(store.objects[actor]):
                    raise Refused("unsatisfied", "actor_active", "dependent")
                if store.proposals[key]["state"] != "pending":
                    decided = Refused("unavailable", remedy="unreachable_from_here",
                                      detail=f"the proposal is {store.proposals[key]['state']}")
                    decided.on_proposal = True      # the proposal's state refuses, not its target's
                    raise decided
                # a proposal is decided by those who may take its transition: a rejection, and a withdrawal by another
                # than its proposer, by a kind its transition's taken_by does not name is refused as approving it
                # would be, as the core refuses it (ADR-0191 decision 1)
                held = store.proposals[key]
                kinds = ((((runner.kinds.get(held["type"]) or {}).get("transitions") or {})
                          .get(held["transition"]) or {}).get("taken_by") or [])
                if (verb != "approve" and kinds and store.actor_attr(store.type_of(actor))[1] not in kinds
                        and not (verb == "withdraw" and actor == held["proposer"])):
                    clause = f"{held['transition']}_taken_by"
                    raise Refused("invariant_violated", clause, "delegable", object_id=held["object"],
                                  object_type=held["type"],
                                  failures=[(held["object"], held["type"], clause, "delegable")])
            except Refused as decision:
                store.attempts.append(attempt_row(store, "Proposal", key, verb, actor, decision))
                raise
            if verb != "approve":
                store.proposals[key]["state"] = "rejected" if verb == "reject" else "withdrawn"
                proposal_event(store, key, verb, "PENDING", "REJECTED" if verb == "reject" else "WITHDRAWN", actor)
                return None
            # approving executes the request as the approver, every guard evaluated now; a refusal leaves it pending.
            # The proposal's event comes first, its position reserved and given back on a refusal (DESIGN.md §9)
            held = store.proposals[key]
            store.position += 1
            reserved = store.position
            try:
                result = runner.attempt(store, held["type"], held["transition"], held["object"], actor, held["inputs"],
                                        held["aliases"], stubs, held.get("occurred"), held.get("filed_at"),
                                        proposal=key)
            except Refused:
                store.position -= 1
                raise
            proposal_event(store, key, "approve", "PENDING", "EXECUTED", actor, position=reserved)
        except Refused as refusal:
            if key in store.proposals and store.proposals[key]["state"] == "pending":
                store.proposals[key]["verdict"] = refusal
            if under_test:
                return ("refused", refusal)
            raise
        store.proposals[key]["state"] = "executed"      # the store's own, which the request's commit replaced
        return result
    r = st["request"]
    advance(store, r)
    actor = aliases.get(r.get("actor", OPERATOR), r.get("actor"))
    if "type" in r:
        tn, oid = r["type"], None
    else:
        oid = aliases[r["object"]]
        tn = store.type_of(oid)
    occurred = runner.value_of(store, {"type": "timestamp"}, r["occurred_at"], aliases) if "occurred_at" in r else None
    try:
        result = runner.attempt(store, tn, r["transition"], oid, actor, r.get("inputs"), aliases, stubs, occurred)
    except Refused as refusal:
        if under_test:
            return ("refused", refusal)
        raise
    if "as" in r:
        aliases[r["as"]] = result[1]
    # the objects its effect and cascades create, named in the order it creates them: a list, all of one type, or a
    # mapping from each type to its objects (flow-format.md §11.3, ADR-0141, ADR-0173 decision 6)
    named, made = r.get("creates") or [], result[4]
    if isinstance(named, dict):
        for ct, aliases_of in named.items():
            of = [m for m in made if store.type_of(m) == ct]
            if len(aliases_of) > len(of):
                raise ExampleError(f"a step names {len(aliases_of)} objects of {ct} its request creates, and it creates "
                                   f"{len(of)}")
            aliases.update(zip(aliases_of, of))
        return result
    if len(named) > len(made):
        raise ExampleError(f"a step names {len(named)} objects its request creates, and it creates {len(made)}")
    if len({store.type_of(m) for m in made[:len(named)]}) > 1:
        raise ExampleError("a step names objects its request creates, and they are of more than one type")
    for alias, oid in zip(named, made):
        aliases[alias] = oid
    return result


# the members every object has, by the type an expectation's value of one is written as (flow-format.md §5)
BUILT_IN_MEMBERS = {"id": {"type": "string"}, "open": {"type": "bool"}, "created_at": {"type": "timestamp"},
                    "created_by_kind": {"type": "actor_kind"}, "recorded_from": {"type": "timestamp"},
                    "declaration_version": {"type": "int"}}


def written(v, spec):
    """A value as an example writes it, for a message: a number in decimal digits, an amount with its currency, a file
    by its hash, its size and its media type."""
    if isinstance(v, tuple) and len(v) == 4 and v[0] == "file":
        return f"the file {v[1][:19]}…, {v[2]} bytes of {v[3]}"
    if not isinstance(v, Fraction) or isinstance(v, bool):
        return v
    places = 0
    while (v * 10 ** places).denominator != 1 and places < 18:
        places += 1
    n = v * 10 ** places
    if n.denominator != 1:
        text = str(v)
    else:
        digits = str(abs(n.numerator)).rjust(places + 1, "0")
        text = ("-" if n < 0 else "") + (digits[:-places] + "." + digits[-places:] if places else digits)
    typ = (spec or {}).get("type", "")
    return f"{typ[len('money('):-1]} {text}" if typ.startswith("money(") else text


class WrongKind:
    """A wire value of another JSON kind than its input's type takes, which the studio's server marks so the runner
    refuses it, `wrong_type`, at the step the core does (ADR-0173 decision 9)."""

    def __init__(self, value):
        self.value = value


TEXT_BYTES = 64 * 1024              # the most bytes a string value holds, in UTF-8 (ADR-0188 decision 1)
INPUT_UNITS = 999_999_999_999_999_999   # an amount or a decimal holds 18 digits at its scale, as the store does (ADR-0143)
NUMBER = re.compile(r"-?[0-9]+(?:\.[0-9]+)?")


def held_fault(amount, scale, digits=18):
    """Why a number does not fit its type: more places than the scale, or more digits than a decimal(p,s)'s p or an
    amount's 18; None where it fits (declaration-syntax.md §4)."""
    units = Fraction(amount) * 10 ** scale
    if units.denominator != 1:
        return "malformed"
    return "out_of_range" if abs(units) > min(INPUT_UNITS, 10 ** digits - 1) else None


def precision_of(typ):
    """A decimal(p,s)'s p, none for another type."""
    m = re.fullmatch(r"decimal\((\d+),\s*(\d+)\)", str(typ or ""))
    return int(m.group(1)) if m else None


FILE_HASH = re.compile(r"sha256:[0-9a-f]{64}")
MEDIA_TOKEN = r"[A-Za-z0-9!#$&^_.+-]+"


def file_value(v):
    """A file as the runner holds it, `("file", hash, size, media type)`: from its wire form, `{hash, size,
    media_type}`, or from the name an example writes it by, a file whose content is that name's UTF-8 bytes, of media
    type application/octet-stream (flow-format.md §11.3, ADR-0173 decision 11)."""
    if isinstance(v, dict):
        return ("file", v.get("hash"), v.get("size"), str(v.get("media_type", "")).lower())
    data = str(v).encode("utf-8")
    return ("file", "sha256:" + hashlib.sha256(data).hexdigest(), len(data), "application/octet-stream")


def file_fault(v):
    """Why a file's wire form is malformed, or None (ADR-0173 decision 11)."""
    if set(v) != {"hash", "size", "media_type"}:
        return "malformed"
    if not (isinstance(v["hash"], str) and FILE_HASH.fullmatch(v["hash"])):
        return "malformed"
    if isinstance(v["size"], bool) or not isinstance(v["size"], int) or not 0 <= v["size"] <= 2 ** 53 - 1:
        return "malformed"
    if not (isinstance(v["media_type"], str) and re.fullmatch(f"{MEDIA_TOKEN}/{MEDIA_TOKEN}", v["media_type"])):
        return "malformed"
    return None


def input_fault(spec, v, enums, kinds=None):
    """Why a request's input is not valid as its spec declares (ADR-0173 decision 9), as the core's decoder says it,
    or None: a member of no enumeration it names, a time, a date or a duration that does not parse, an amount without
    its currency or in another, more places than its scale, more than 18 digits, a string holding U+0000. A wire value
    of another JSON kind the studio's server marks as WrongKind; an example's YAML-native value is its own notation."""
    if v is None:
        return None
    if isinstance(v, WrongKind):
        return "wrong_type"
    spec = spec or {}
    if "reference" in spec:
        items = v if isinstance(v, (list, tuple)) else [v]
        # whether it names an object of its type is checked later; a set never repeats an element
        return "malformed" if len({repr(i) for i in items}) < len(items) else None
    typ = str(spec.get("type", ""))
    if typ.endswith("[]"):
        items = v if isinstance(v, (list, tuple)) else [v]
        for item in items:
            why = input_fault(dict(spec, type=typ[:-2]), item, enums)
            if why:
                return why
        return "malformed" if len({repr(i) for i in items}) < len(items) else None     # a set never repeats an element
    if typ in enums:
        member = str(v)[len(typ) + 1:] if str(v).startswith(typ + ".") else str(v)
        return None if member in enums[typ] else "wrong_type"
    if typ in ("int", "counter"):
        if isinstance(v, bool) or isinstance(v, float):
            return "wrong_type"
        if isinstance(v, str):
            if not re.fullmatch(r"-?[0-9]+", v):
                return "malformed"
            v = int(v)
        return None if -2 ** 63 <= v <= 2 ** 63 - 1 else "out_of_range"   # 64 bits, the least included, as the core's
    if typ == "timestamp":
        if isinstance(v, datetime.datetime):
            return None
        if re.fullmatch(r"now(?: [+-] [0-9]+ \w+)?", str(v)):
            return None                         # an example's clock (flow-format.md §11.3)
        if not TIMESTAMP_TEXT.fullmatch(str(v)):
            return "malformed"              # each field its digits exactly, at UTC, as the core reads one
        try:
            timestamp(str(v))
            return None
        except ValueError:
            return "malformed"
    if typ == "date":
        if isinstance(v, datetime.date):
            return None
        if not DATE_TEXT.fullmatch(str(v)):
            return "malformed"              # YYYY-MM-DD exactly, as the core reads one
        try:
            datetime.date.fromisoformat(str(v))
            return None
        except ValueError:
            return "malformed"
    if typ == "duration":
        m = re.fullmatch(r"([0-9]+) (\w+)", str(v))
        return None if m and m.group(2) in SECONDS else "malformed"
    if typ.startswith("money("):
        if not isinstance(v, str):
            return "wrong_type"
        # a currency, one space and an amount, nothing around them, as a date is read strictly (fourth production
        # review A-12)
        code, _, amount = v.rpartition(" ")
        if v != v.strip() or not code or code != code.strip() or not NUMBER.fullmatch(amount):
            return "malformed"
        if code.strip() != typ[len("money("):-1]:
            return "wrong_currency"
        return held_fault(amount, MINOR_UNITS.get(code.strip(), 2))
    if typ.startswith("decimal("):
        if isinstance(v, bool):
            return "wrong_type"
        text = str(v)
        if not NUMBER.fullmatch(text):
            return "malformed"
        return held_fault(text, int(typ.rstrip(")").rsplit(",", 1)[1]), precision_of(typ))
    if typ == "file":
        # its wire form, or the name an example writes it by (flow-format.md §11.3)
        return file_fault(v) if isinstance(v, dict) else (None if isinstance(v, str) else "wrong_type")
    if typ in ("string", "text", "identity", "actor_kind"):
        if not isinstance(v, str):
            return "wrong_type"
        # an identity holds no control, zero-width or bidirectional formatting character, as the core's (ADR-0190)
        if typ == "identity" and identity_fault(v, kinds):
            return "malformed"
        if "\0" in v:
            return "malformed"
        return "out_of_range" if len(v.encode("utf-8")) > TEXT_BYTES else None   # 64 KiB, as the core's (ADR-0188)
    return None


def formatting(c):
    """A control character, or one that shows nothing: a format character, a variation selector, a combining grapheme
    joiner, a Hangul filler, a tag or a separator, by their code points, as the core's (ADR-0192 decision 6)."""
    o = ord(c)
    return (o < 0x20 or 0x7F <= o <= 0x9F
            or o in (0x00AD, 0x034F, 0x061C, 0x06DD, 0x070F, 0x08E2, 0x180E, 0x3164, 0xFEFF, 0xFFA0, 0x110BD, 0x110CD)
            or any(lo <= o <= hi for lo, hi in (
                (0x0600, 0x0605), (0x0890, 0x0891), (0x115F, 0x1160), (0x200B, 0x200F), (0x2028, 0x202E),
                (0x2060, 0x2064), (0x2066, 0x206F), (0xFE00, 0xFE0F), (0xFFF9, 0xFFFB), (0x13430, 0x1343F),
                (0x1BCA0, 0x1BCA3), (0x1D173, 0x1D17A), (0xE0000, 0xE007F), (0xE0100, 0xE01EF))))


def ascii_fold(s):
    """A text with its ASCII letters in lower case and every other character as it is, as SQLite's `lower` and the
    core fold an identity (ADR-0194 decision 1)."""
    return s.translate(ASCII_LOWER)


ASCII_LOWER = str.maketrans("ABCDEFGHIJKLMNOPQRSTUVWXYZ", "abcdefghijklmnopqrstuvwxyz")


def identity_fault(v, kinds):
    """Why an identity names no actor as it shows, as the core's: empty, whitespace at an end or other than a space
    within, a character that shows nothing, or shaped as an object's id of a type the store has; None where it is
    fine (ADR-0192 decision 6)."""
    if v == "" or v != v.strip() or any(c.isspace() and c != " " for c in v) or any(formatting(c) for c in v):
        return "malformed"
    # a blank that shows nothing, a private-use character and a noncharacter, as the core's (ADR-0194 decision 6)
    if any(ord(c) in (0x2800, 0x17B4, 0x17B5) or 0xE000 <= ord(c) <= 0xF8FF or ord(c) >= 0xF0000
           or 0xFDD0 <= ord(c) <= 0xFDEF or ord(c) & 0xFFFE == 0xFFFE for c in v):
        return "malformed"
    prefix, _, n = v.rpartition("-")
    # in any case of its letters, where Staff-2 named a person and staff-2 an object (fifteenth production review)
    if prefix and n.isdigit() and n.isascii() and any(t.lower() == prefix.lower() for t in (kinds or {})):
        return "malformed"
    return None


def spec_of_kind(kind):
    """The spec an example writes a value of an expression's kind as: money with its currency, a decimal, an object
    or a set of them, or a scalar; None where the kind is not one a value is written as."""
    k = (kind or ("unknown",))[0]
    if k in ("decimal", "literal"):
        return {"type": "decimal"}
    if k == "money":
        return {"type": f"money({kind[1]})"}
    if k in ("int", "duration", "timestamp", "bool", "date", "string"):
        return {"type": k}
    if k == "object":
        return {"reference": kind[1]}
    if k == "objects":
        return {"reference": f"{kind[1]}[]"}
    return None


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
    derived = (runner.kinds.get(o.type) or {}).get("derived_attributes") or {}
    for k, v in (want.get("values") or {}).items():
        spec = attrs.get(k) or BUILT_IN_MEMBERS.get(k)
        if spec is None and k in derived:
            # a derived attribute's value is written as one of the kind its expression has, as a stored one is (D548)
            spec = spec_of_kind(Evaluator(store, o.type, o.id).kind(parsed(derived[k]["expression"]), {"this": ("object", o.type)}))
        expected = runner.value_of(store, spec, v, aliases)
        # a derived attribute and a member every object has are read as a reader reads them, the others as stored
        got = Evaluator(store, o.type, o.id).member(o.id, k) if k in derived or (k in BUILT_IN_MEMBERS and k not in attrs) \
            else o.attrs.get(k, ABSENT)
        if got != expected:
            named = {oid: alias for alias, oid in aliases.items()}
            return f"expected {k} to hold {v}, and it holds {named.get(got, got) if isinstance(got, str) else written(got, spec)}"
    if "occurred_at" in want:
        expected = runner.value_of(store, {"type": "timestamp"}, want["occurred_at"], aliases)
        if outcome[5] != expected:
            return f"expected the request to record the occurred time {want['occurred_at']}, and it records {outcome[5]}"
    if "cascaded" in want and list(want["cascaded"]) != list(outcome[3]):
        return f"expected the request to cause {want['cascaded'] or 'nothing'}, and it causes {outcome[3] or 'nothing'}"
    if "warned" in want:
        # the flags an applied verdict lists (DESIGN.md §5.5): its own clauses by name, another type's qualified
        mine = f"{o.type}."
        warned = [c[len(mine):] if c.startswith(mine) else c for c, mode in outcome[2] if mode == "warn"]
        if list(want["warned"]) != warned:
            return f"expected the request to warn of {want['warned'] or 'nothing'}, and it warns of {warned or 'nothing'}"
    return None


def metric_differs(runner, store, name, want, aliases):
    """What differs between a metric an example expects and what a reader reads after the request, or None: its value
    over every row, keeping no dimension, or its rows by every dimension it declares, each its dimensions' values and
    its `value`, all of them in any order (flow-format.md §11.4, ADR-0173 decision 6)."""
    ev = Evaluator(store, None, stubs=NEVER_ASKED)
    by_rows = isinstance(want, list)
    page = ev.read_page(name, keep=None if by_rows else [])
    if page["refused"]:
        return f"expected the metric {name} to read {want}, and the read is refused as {page['refused']}"
    owner, spec, module = store.find_metric(parsed(name))
    kind, _dims = ev.page_kinds(owner, spec, module, sorted(store.metric_dimensions(owner, spec)))
    vspec = spec_of_kind(kind)
    named = {oid: alias for alias, oid in aliases.items()}

    def value_is(expected, got):
        if expected is None or got is None or got is ABSENT:
            return expected is None and (got is None or got is ABSENT)
        return runner.value_of(store, vspec, expected, aliases) == got

    def dimension_is(expected, got):
        # a state or an enumeration member by its name, qualified or not; an object by its alias
        if isinstance(got, tuple) and got[:1] in (("state",), ("enum",)):
            return expected in (got[2], f"{got[1]}.{got[2]}")
        # an object by its alias, or a text that reads as one, such as an identity (D568)
        if isinstance(expected, str) and expected in aliases and aliases[expected] == got:
            return True
        return expected == got or str(expected) == str(got)

    def shown(row):
        # in the notation an example writes, a duration by its largest whole unit (D568)
        dims = {d: (v[2] if isinstance(v, tuple) else named.get(v, v)) for d, v in row["dimensions"].items()}
        v = row["value"]
        if (vspec or {}).get("type") == "duration" and isinstance(v, (int, Fraction)) and Fraction(v).denominator == 1:
            n = int(v)
            unit = next(u for u in ("week", "day", "h", "min", "s") if n % SECONDS[u] == 0 and (n or u == "s"))
            count = n // SECONDS[unit]
            return dict(dims, value=f"{count} {unit}{'s' if unit in ('week', 'day') and abs(count) != 1 else ''}")
        return dict(dims, value=written(v, vspec))

    rows = page["rows"]
    if not by_rows:
        got = rows[0]["value"] if rows else None
        return None if value_is(want, got) else \
            f"expected the metric {name} to read {want}, and it reads {written(got, vspec)}"
    left = list(rows)
    for w in want:
        dims = {d: v for d, v in w.items() if d != "value"}
        hit = next((r for r in left if set(r["dimensions"]) == set(dims)
                    and all(dimension_is(v, r["dimensions"][d]) for d, v in dims.items())
                    and value_is(w.get("value"), r["value"])), None)
        if hit is None:
            return f"expected the metric {name} to read a row {w}, and it reads {[shown(r) for r in rows]}"
        left.remove(hit)
    if left:
        return f"expected the metric {name} to read {len(want)} rows, and it reads {[shown(r) for r in rows]}"
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
    check("a refused call's attempt row carries the call's own clause, under the transition requested",
          (s.attempts[-1]["transition"], s.attempts[-1]["verdict"], s.attempts[-1]["clause"]), ("ship", "call_refused", "small"))
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
    k["Order"]["conditions"] = {"paid": {"expression": "xero.invoice_paid(id)", "remedy": "dependent"}}
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
    # an argument that is absent makes no call: the guard is unknown, refused with the condition's own remedy (ADR-0146)
    k["Order"]["conditions"]["paid"]["expression"] = "xero.invoice_paid(note)"
    r, s = Runner(k, {}), Store(k, {})
    order = r.attempt(s, "Order", "open", None, OPERATOR, {}, {}, {})[1]
    try:
        got = r.attempt(s, "Order", "settle", order, OPERATOR, {}, {}, {"xero.invoice_paid": "satisfied"})[0]
    except Refused as e:
        got = (e.verdict, e.remedy)
    check("an evaluator asked nothing where its argument is absent", got, ("unsatisfied", "dependent"))
    # an offer asks no evaluator: the guard that would ask one is listed as unevaluated, and refuses nothing (ADR-0152)
    k["Order"]["conditions"]["paid"]["expression"] = "xero.invoice_paid(id)"
    r, s = Runner(k, {}), Store(k, {})
    order = r.attempt(s, "Order", "open", None, OPERATOR, {}, {}, {})[1]
    settle = next(x for x in r.availability(s, OPERATOR, order) if x[0] == "settle")
    check("an offer lists a guard that would ask an evaluator as unevaluated, and asks nothing",
          (settle[2], settle[3], settle[5]), ("available", None, ["paid"]))
    # a flow function on the object is computed, never taken for an evaluator left unanswered (D534)
    k["Order"]["conditions"]["aged"] = {"expression": "this.time_in(OPEN) >= 1 h", "remedy": "temporal"}
    k["Order"]["transitions"]["settle"]["guards"] = {"aged": "deny"}
    r, s = Runner(k, {}), Store(k, {})
    order = r.attempt(s, "Order", "open", None, OPERATOR, {}, {}, {})[1]
    settle = next(x for x in r.availability(s, OPERATOR, order) if x[0] == "settle")
    check("an offer decides a guard reading a flow function, which asks no evaluator",
          (settle[2], settle[3].clause if settle[3] else None, settle[5]), ("unavailable", "aged", []))
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
    # a correction replaces the wrong value for the span it was held, explicitly (ADR-0146): the crew span is relabelled
    # and carries the correction, and the WORKING span and the start event begun within it credit the right crew
    jobs = {"Operator": kinds["Operator"], "Job": {
        "attributes": {"crew": {"type": "Crew"}},
        "states": {"OPEN": {"category": "live"}, "WORKING": {"category": "live"}, "DONE": {"category": "closed", "final": True}},
        "transitions": {"open": {"kind": "initial", "to": "OPEN", "required_inputs": ["crew"]},
                        "start": {"kind": "external", "from": ["OPEN"], "to": "WORKING"},
                        "move_crew": {"kind": "internal", "from": ["OPEN", "WORKING"], "required_inputs": ["crew"]},
                        "fix_crew": {"kind": "internal", "from": ["OPEN", "WORKING"], "corrects": ["crew"],
                                     "required_inputs": ["crew"], "inputs": {"reason": {"type": "string"}}}}}}
    r, s = Runner(jobs, {"Crew": ["NORTH", "SOUTH"]}), Store(jobs, {"Crew": ["NORTH", "SOUTH"]})
    j = r.attempt(s, "Job", "open", None, OPERATOR, {"crew": "Crew.NORTH"}, {}, {})[1]
    r.attempt(s, "Job", "start", j, OPERATOR, {}, {}, {})
    r.attempt(s, "Job", "fix_crew", j, OPERATOR, {"crew": "Crew.SOUTH", "reason": "mis-keyed"}, {}, {})
    o = s.objects[j]
    spans = o.spans["crew"]
    check("a correction relabels the span it corrects, marking it", (len(spans), spans[0]["value"], "corrected_by" in spans[0]),
          (1, ("enum", "Crew", "SOUTH"), True))
    check("a span begun within the corrected one credits the corrected value", o.spans["state"][-1]["held"]["crew"], ("enum", "Crew", "SOUTH"))
    start = next(ev for ev in s.events if ev["object"] == j and ev["transition"] == "start")
    check("an event taken within it does too, and the original write's event is not changed",
          (start["held"]["crew"], next(ev for ev in s.events if ev["transition"] == "open")["held"].get("crew", ABSENT)),
          (("enum", "Crew", "SOUTH"), ABSENT))
    check("a state span whose .held it changed carries the correction", "corrected_by" in o.spans["state"][-1], True)
    # a crew moved by mistake and corrected back: the relabelled interval merges with the one before it, and the
    # correction's own row credits the corrected crew (ADR-0147)
    r, s = Runner(jobs, {"Crew": ["NORTH", "SOUTH"]}), Store(jobs, {"Crew": ["NORTH", "SOUTH"]})
    j = r.attempt(s, "Job", "open", None, OPERATOR, {"crew": "Crew.NORTH"}, {}, {})[1]
    s.now += 3600
    r.attempt(s, "Job", "move_crew", j, OPERATOR, {"crew": "Crew.SOUTH"}, {}, {})
    s.now += 3600
    r.attempt(s, "Job", "fix_crew", j, OPERATOR, {"crew": "Crew.NORTH", "reason": "never moved"}, {}, {})
    spans = s.objects[j].spans["crew"]
    check("a corrected interval equal to the one before it merges with it",
          (len(spans), spans[0]["value"], spans[0]["left_at"], "corrected_by" in spans[0]), (1, ("enum", "Crew", "NORTH"), ABSENT, True))
    check("the correction's own row credits the corrected value", s.events[-1]["held"]["crew"], ("enum", "Crew", "NORTH"))
    # a row begun after the corrected interval's start, though dated before it, credits the corrected value, since `.held`
    # is read at the position of the transition that began a row (declaration-syntax.md §6.9)
    dated = copy.deepcopy(jobs)
    dated["Job"]["transitions"]["start"]["backdating_limit"] = "1 day"
    crews = {"Crew": ["NORTH", "SOUTH", "EAST"]}
    r, s = Runner(dated, crews), Store(dated, crews)
    j = r.attempt(s, "Job", "open", None, OPERATOR, {"crew": "Crew.NORTH"}, {}, {})[1]
    s.now += 7200
    r.attempt(s, "Job", "move_crew", j, OPERATOR, {"crew": "Crew.SOUTH"}, {}, {})
    r.attempt(s, "Job", "start", j, OPERATOR, {}, {}, {}, occurred=s.now - 3600)
    r.attempt(s, "Job", "fix_crew", j, OPERATOR, {"crew": "Crew.EAST", "reason": "mis-keyed"}, {}, {})
    o = s.objects[j]
    start = next(ev for ev in s.events if ev["transition"] == "start")
    check("a span and an event begun after the corrected interval's start, though dated before it, credit the corrected value",
          (o.spans["state"][-1]["held"]["crew"], start["held"]["crew"], [sp["value"][2] for sp in o.spans["crew"]]),
          (("enum", "Crew", "EAST"), ("enum", "Crew", "EAST"), ["NORTH", "EAST"]))
    check("a row's .corrected_by is read where a correction changed its value or its .held, and absent elsewhere",
          Evaluator(s, "Job", j).value(flowexpr.parse("count(i in this.intervals where i.corrected_by is not null)"), {}), 1)
    # a request failing two generated guards is refused naming the first in the order flow-format.md §4.8 states
    wallet = {"attributes": {"cards": {"reference": "Card[]", "aggregation": "composite", "opposite": "wallet"}},
              "states": {"OPEN": {"category": "live"}, "SHUT": {"category": "closed", "final": True}},
              "transitions": {"open": {"kind": "initial", "to": "OPEN"}, "shut": {"kind": "external", "from": ["OPEN"], "to": "SHUT"}}}
    card = {"attributes": {"wallet": {"reference": "Wallet", "opposite": "cards"},
                           "holder": {"type": "string", "optional": True, "personal": True}},
            "states": {"LIVE": {"category": "live"}, "MERGED": {"category": "closed", "final": True, "superseding": True}},
            "transitions": {"issue": {"kind": "initial", "to": "LIVE", "required_inputs": ["wallet", "holder"]},
                            "move": {"kind": "internal", "from": ["LIVE"], "required_inputs": ["wallet", "holder"]},
                            "merged_into": {"kind": "external", "from": ["LIVE"], "to": "MERGED", "required_inputs": ["holder"],
                                            "inputs": {"successor": {"reference": "Card"}}, "effect": [{"supersede": "inputs.successor"}]},
                            "forget": {"kind": "erasure", "inputs": {"reason": {"type": "string"}}}}}
    cards = {"Operator": kinds["Operator"], "Wallet": wallet, "Card": card}

    def first_refusal(xn, inputs):
        r, s = Runner(cards, {}), Store(cards, {})
        w = r.attempt(s, "Wallet", "open", None, OPERATOR, {}, {}, {})[1]
        shut = r.attempt(s, "Wallet", "open", None, OPERATOR, {}, {}, {})[1]
        c = r.attempt(s, "Card", "issue", None, OPERATOR, {"wallet": "w", "holder": "Dana"}, {"w": w}, {})[1]
        r.attempt(s, "Wallet", "shut", shut, OPERATOR, {}, {}, {})
        r.attempt(s, "Card", "forget", c, OPERATOR, {"reason": "asked"}, {}, {})
        aliases = {"w": w, "shut": shut, "c": c}
        try:
            r.attempt(s, "Card", xn, c, OPERATOR, dict(inputs, holder="Dee", **({"successor": "c"} if xn == "merged_into" else {"wallet": "shut"})), aliases, {})
            return "applied"
        except Refused as e:
            return e.clause
    check("whole_open before not_erased", first_refusal("move", {}), "whole_open")
    check("not_erased before successor_not_superseded", first_refusal("merged_into", {}), "not_erased")
    # a request dated when the change happened: the requested transition's bound, and each transition it takes kept
    # from preceding the current interval of what it changes, a call's included (flow-format.md §4.8, ADR-0146)
    k = copy.deepcopy(kinds)
    k["Order"]["transitions"]["ship"]["backdating_limit"] = "2 days"
    r, s = Runner(k, {}), Store(k, {})
    order = r.attempt(s, "Order", "open", None, OPERATOR, {}, {}, {})[1]
    s.now += 4 * 86400                  # the order open longer than the bound, so only the bound refuses a date past it
    r.attempt(s, "Order", "add_line", order, OPERATOR, {"qty": 1}, {}, {})
    s.now += 3600

    def dated(xn, back):
        w = copy.deepcopy(s)
        try:
            r.attempt(w, "Order", xn, order, OPERATOR, {}, {}, {}, occurred=w.now - back)
            return "applied", w.objects[order].entered.get("DONE"), w.events[-1]["occurred_at"], w.events[-1]["recorded_at"]
        except Refused as e:
            return e.verdict, e.clause
    check("a change dated within its bound, recorded when it was told", dated("ship", 1800),
          ("applied", s.now - 1800, s.now - 1800, s.now))
    check("a change dated past its bound", dated("ship", 3 * 86400), ("unsatisfied", "occurred_within"))
    check("a change dated after it was told", dated("ship", -3600), ("unsatisfied", "occurred_within"))
    check("a call dated before the current interval of what it changes", dated("ship", 2 * 3600),
          ("call_refused", "Line.occurred_within"))
    check("a transition with no bound, dated", dated("cancel", 60), ("unsatisfied", "occurred_within"))
    # an erasure takes each observation's generated forget after its own steps, a transition it causes and lists
    # (DESIGN.md §8, ADR-0147)
    module = {"types": {"Person": {
        "attributes": {"name": {"type": "string", "personal": True}},
        "observations": {"notes": {"kind": "Note", "attributes": {"body": {"type": "string", "personal": True}}}},
        "states": {"ACTIVE": {"category": "live"}},
        "transitions": {"add": {"kind": "initial", "to": "ACTIVE", "required_inputs": ["name"]},
                        "forget": {"kind": "erasure", "inputs": {"reason": {"type": "string"}}}}}}}
    k = {"Operator": kinds["Operator"], **module["types"], **observation_kinds({"m": module})}
    r, s = Runner(k, {}), Store(k, {})
    person = r.attempt(s, "Person", "add", None, OPERATOR, {"name": "Ana"}, {}, {})[1]
    note = r.attempt(s, "Note", "record", None, OPERATOR, {"subject": person, "body": "rang"}, {person: person}, {})[1]
    tag = r.attempt(s, LABEL, "record", None, OPERATOR, {"subject": person, "name": "VIP", "note": "rang"}, {person: person}, {})[1]
    erased = r.attempt(s, "Person", "forget", person, OPERATOR, {"reason": "asked"}, {}, {})
    other = r.attempt(s, "Person", "add", None, OPERATOR, {"name": "Ben"}, {}, {})[1]
    own = r.attempt(s, LABEL, "record", None, OPERATOR, {"subject": other, "name": "vip", "note": "rang"}, {other: other}, {})[1]
    r.attempt(s, LABEL, "forget", own, OPERATOR, {"reason": "asked"}, {}, {})
    # an example's import admits nothing: a violation refuses it, leaving the store as it was (ADR-0154, D526)
    hr = {"Operator": kinds["Operator"],
          "Employee": {"mirror": True, "attributes": {"grade": {"type": "int"}},
                       "states": {"EMPLOYED": {"category": "live"}},
                       "invariants": {"graded": {"expression": "grade >= 1"}}}}
    hs = Store(hr, {})
    held_before = (len(hs.objects), hs.counter)
    try:
        import_object(Runner(hr, {}), hs, "Employee", "EMPLOYED", {"grade": 0})
        imported = "applied"
    except ImportRefused as refusal:
        imported = str(refusal)
    check("an example's import that violates an invariant is refused, writing nothing",
          ("graded" in imported, (len(hs.objects), hs.counter)), (True, held_before))
    check("an example's import that complies is written",
          hs.objects[import_object(Runner(hr, {}), hs, "Employee", "EMPLOYED", {"grade": 2})].state, "EMPLOYED")
    # a base's uniqueness spans the whole family, siblings included (flow-format.md §4.14, D519)
    key = {"key": {"type": "string", "unique": True}, "name": {"type": "string"}}
    fam = {"Operator": kinds["Operator"], "Asset": {"abstract": True, "attributes": dict(key)}}
    for sub, st in (("Laptop", "IN_USE"), ("Server", "RACKED")):
        fam[sub] = {"extends": "Asset", "attributes": dict(key), "_from": {("attributes", "key"): "Asset", ("attributes", "name"): "Asset"},
                  "states": {st: {"category": "live"}},
                  "transitions": {"receive": {"kind": "initial", "to": st, "required_inputs": ["key", "name"]}}}
    fr, fs = Runner(fam, {}), Store(fam, {})
    fr.attempt(fs, "Laptop", "receive", None, OPERATOR, {"key": "IT-1", "name": "ThinkPad"}, {}, {})
    try:
        fr.attempt(fs, "Server", "receive", None, OPERATOR, {"key": "IT-1", "name": "db"}, {}, {})
        sibling = ("applied",)
    except Refused as refusal:
        sibling = (refusal.verdict, refusal.clause)
    # named on the laptop, the first object in ascending id order it fails on
    check("a sibling taking a base's unique key is refused", sibling, ("invariant_violated", "Laptop.key_unique"))
    check("an erasure takes each observation's forget, a transition it causes",
          (erased[3], s.objects[note].attrs.get("body", ABSENT)), (["Note.forget"], ABSENT))
    check("an erasure redacts its labels' notes as no transition, and a label's own forget its note, the names kept",
          [(s.objects[x].attrs.get("note", ABSENT), s.objects[x].attrs["name"], s.objects[x].erased) for x in (tag, own)],
          [(ABSENT, "vip", False), (ABSENT, "vip", True)])
    # an invariant failing on a part the request creates names a remedy, as one on the requested object does (D547):
    # delegable where the creation writes the actor to a value it reads, since another actor can pass it (ADR-0173)
    k2 = {"Operator": kinds["Operator"],
          "Ask": {"attributes": {"by": {"type": "string", "optional": True}},
                  "states": {"OPEN": {"category": "live"}},
                  "transitions": {"raise": {"kind": "initial", "to": "OPEN", "effect": [{"assign": {"location": "by", "expr": "actor.id"}}]},
                                  "approve": {"kind": "internal", "from": ["OPEN"], "effect": [
                                      {"create": {"type": "Nod", "transition": "give", "inputs": {"ask": "this", "who": "actor.id"}}}]}}},
          "Nod": {"attributes": {"ask": {"reference": "Ask"}, "who": {"type": "string"}},
                  "states": {"GIVEN": {"category": "live"}},
                  "invariants": {"not_own": {"expression": "who != ask.by"}},
                  "transitions": {"give": {"kind": "initial", "to": "GIVEN", "required_inputs": ["ask", "who"], "only_via": ["Ask.approve"]}}}}
    r2, s2 = Runner(k2, {}), Store(k2, {})
    ask = r2.attempt(s2, "Ask", "raise", None, OPERATOR, {}, {}, {})[1]
    try:
        r2.attempt(s2, "Ask", "approve", ask, OPERATOR, {}, {}, {})
        own = ("applied",)
    except Refused as refusal:
        own = (refusal.verdict, refusal.clause, refusal.remedy)
    check("an invariant failing on a part the request creates, over a value written from the actor, is delegable",
          own, ("invariant_violated", "Nod.not_own", "delegable"))
    # erasure (ADR-0147): its admission on the object the invariant fails on, one hop away; not_erased refusing an
    # assignment or an add to a personal attribute and passing a clear; a pending proposal on it invalidated
    k = {"Operator": kinds["Operator"],
         "Person": {"attributes": {"name": {"type": "string", "optional": True, "personal": True},
                                   "aliases": {"type": "string[]", "optional": True, "personal": True}},
                    "states": {"ACTIVE": {"category": "live"}},
                    "transitions": {"add": {"kind": "initial", "to": "ACTIVE", "required_inputs": ["name"]},
                                    "rename": {"kind": "internal", "from": ["ACTIVE"], "inputs": {"n": {"type": "string"}},
                                               "effect": [{"assign": {"location": "name", "expr": "inputs.n"}}]},
                                    "alias": {"kind": "internal", "from": ["ACTIVE"], "inputs": {"n": {"type": "string"}},
                                              "effect": [{"add": {"location": "aliases", "expr": "inputs.n"}}]},
                                    "wipe": {"kind": "internal", "from": ["ACTIVE"], "effect": [{"clear": ["name"]}]},
                                    "forget": {"kind": "erasure", "inputs": {"reason": {"type": "string"}}}}},
         "Order": {"attributes": {"customer": {"reference": "Person"}},
                   "states": {"OPEN": {"category": "live"}},
                   "invariants": {"named_customer": {"expression": "customer.name is not null"}},
                   "transitions": {"open": {"kind": "initial", "to": "OPEN", "required_inputs": ["customer"]}}}}
    r, s = Runner(k, {}), Store(k, {})
    person = r.attempt(s, "Person", "add", None, OPERATOR, {"name": "Ana"}, {}, {})[1]
    order = r.attempt(s, "Order", "open", None, OPERATOR, {"customer": person}, {person: person}, {})[1]
    s.proposals["p1"] = {"type": "Person", "object": person, "transition": "rename", "inputs": {"n": "Bo"}, "aliases": {},
                         "proposer": OPERATOR, "state": "pending", "verdict": None}
    r.attempt(s, "Person", "forget", person, OPERATOR, {"reason": "asked"}, {}, {})
    check("an erasure's admission is recorded on the object the invariant fails on", s.objects[order].admissions, {"named_customer"})
    check("an erasure empties a personal set", s.objects[person].attrs.get("aliases", ABSENT), ())
    check("a pending proposal on the erased object is invalidated", s.proposals["p1"]["state"], "invalidated")

    def after(xn, **inputs):
        try:
            return r.attempt(copy.deepcopy(s), "Person", xn, person, OPERATOR, inputs, {}, {})[0]
        except Refused as e:
            return e.clause
    check("not_erased refuses an assignment and an add to a personal attribute, and passes a clear",
          (after("rename", n="Bo"), after("alias", n="B"), after("wipe")), ("not_erased", "not_erased", "applied"))
    # a proposal of a backdatable transition keeps its occurred time, bounded at filing, so an approval two days later
    # dates the change when it happened; one dated past the bound is refused when filed (ADR-0147)
    k = {"Operator": kinds["Operator"], "Unit": {
        "attributes": {}, "states": {"ARRIVING": {"category": "inbound"}, "HERE": {"category": "live"}},
        "transitions": {"ship": {"kind": "initial", "to": "ARRIVING"},
                        "receive": {"kind": "external", "from": ["ARRIVING"], "to": "HERE", "proposable": True,
                                    "backdating_limit": "2 days"},
                        "remark": {"kind": "internal", "from": ["ARRIVING", "HERE"], "proposable": True,
                                   "inputs": {"text": {"type": "string"}}}}}}
    r, s, aliases = Runner(k, {}), Store(k, {}), {OPERATOR: OPERATOR}
    step(r, s, {"request": {"type": "Unit", "transition": "ship", "as": "u"}}, aliases, {})
    step(r, s, {"propose": {"object": "u", "transition": "receive", "after": "1 day", "occurred_at": "now - 1 day",
                            "proposal": "p"}}, aliases, {})
    filed = s.now
    # a pending proposal offers its decisions, guarded by nothing of their own, and one no longer pending refuses them
    # in its state (D535)
    decisions = lambda actor=OPERATOR: [(x[0], x[2], x[3].verdict if x[3] else None)
                                        for x in r.availability(s, actor, aliases["p"][1]) if x[1] is None]
    every = lambda how, why: [(x, how, why) for x in ("approve", "reject", "withdraw")]
    check("a pending proposal offers its decisions", decisions(), every("available", None))
    check("a pending proposal refuses an unknown actor its decisions", decisions("nobody"), every("unavailable", "unsatisfied"))
    applied = step(r, s, {"approve": {"proposal": "p", "after": "2 days"}}, aliases, {})
    check("an approved proposal records the occurred time it was filed with, bounded at filing",
          (applied[0], applied[5], s.proposals[aliases["p"][1]]["state"]), ("applied", filed - 86400, "executed"))
    check("a decided proposal offers no decision", decisions(), every("unavailable", "unavailable"))
    try:
        step(r, s, {"propose": {"object": "u", "transition": "receive", "occurred_at": "now - 3 days", "proposal": "q"}},
             aliases, {})
        check("a proposal dated past its bound when filed", "filed", "occurred_within")
    except Refused as e:
        check("a proposal dated past its bound when filed", e.clause, "occurred_within")
    try:
        step(r, s, {"propose": {"object": "u", "transition": "remark", "proposal": "m"}}, aliases, {})
        check("filing a proposal that leaves out a required input", "filed", "invalid_input")
    except Refused as e:
        check("filing a proposal that leaves out a required input", e.verdict, "invalid_input")
    # a type's invariants are named in their order, its own declared before a generated one; a loop over values runs in
    # ascending value order, whatever order they were given in (flow-format.md §4.8, ADR-0147)
    k = {"Operator": kinds["Operator"], "Tag": {
        "attributes": {"code": {"type": "string"}, "seen": {"type": "string[]", "optional": True},
                       "owner": {"type": "string", "optional": True}},
        "states": {"ON": {"category": "live", "required_attributes": ["owner"]}},
        "invariants": {"not_reserved": {"expression": 'code != "XX"'}},
        "transitions": {"add": {"kind": "initial", "to": "ON", "required_inputs": ["code"], "optional_inputs": ["owner"]},
                        "walk": {"kind": "internal", "from": ["ON"], "inputs": {"labels": {"type": "string[]"}},
                                 "effect": [{"foreach": {"item": "l", "array": "inputs.labels", "limit": 5,
                                                         "steps": [{"add": {"location": "seen", "expr": "l"}}]}}]}}}}
    r, s = Runner(k, {}), Store(k, {})
    try:
        r.attempt(s, "Tag", "add", None, OPERATOR, {"code": "XX"}, {}, {})
        check("a declared invariant named before a generated one", "applied", "not_reserved")
    except Refused as e:
        check("a declared invariant named before a generated one", e.clause, "not_reserved")
    tag = r.attempt(s, "Tag", "add", None, OPERATOR, {"code": "AB", "owner": "ana"}, {}, {})[1]
    r.attempt(s, "Tag", "walk", tag, OPERATOR, {"labels": ["b", "c", "a"]}, {}, {})
    check("a loop over values runs in ascending value order", s.objects[tag].attrs.get("seen"), ("a", "b", "c"))
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
                       "tags": {"type": "string[]", "optional": True},
                       "memo": {"type": "string", "optional": True, "personal": True}},
        "observations": {"checks": {"kind": "Check", "attributes": {"outcome": {"type": "Outcome"}}}},
        "states": {"OPEN": {"category": "inbound"}, "WORKING": {"category": "live"}, "SHELVED": {"category": "closed"},
                   "DONE": {"category": "closed", "final": True}},
        "conditions": {
            "good": {"expression": "metric(pass_rate, engineer := engineer, over last 30 days) >= 0.900", "remedy": "dependent"},
            "not_busy": {"expression": "metric(Job.open_work.engineer, assignee := inputs.engineer) < 2", "remedy": "dependent"},
            "checked": {"expression": "count(c in checks) > 0", "remedy": "self_serviceable"}},
        "transitions": {
            "open": {"kind": "initial", "to": "OPEN", "optional_inputs": ["engineer", "tags"]},
            "assign": {"kind": "internal", "from": ["OPEN", "WORKING", "SHELVED"], "required_inputs": ["engineer"], "guards": {"not_busy": "deny"}},
            "resume": {"kind": "external", "from": ["SHELVED"], "to": "WORKING"},
            "start": {"kind": "external", "from": ["OPEN"], "to": "WORKING"},
            "take": {"kind": "external", "from": ["OPEN"], "to": "WORKING", "required_inputs": ["engineer"]},
            "back": {"kind": "external", "from": ["WORKING"], "to": "OPEN"},
            "pause": {"kind": "external", "from": ["WORKING"], "to": "OPEN"},
            "shelve": {"kind": "external", "from": ["WORKING"], "to": "SHELVED"},
            "close": {"kind": "external", "from": ["SHELVED"], "to": "DONE"},
            "finish": {"kind": "external", "from": ["WORKING"], "to": "DONE", "guards": {"good": "deny"}, "proposable": True},
            "review": {"kind": "internal", "from": ["WORKING"], "guards": {"checked": "warn"}},
            "inspect": {"kind": "internal", "from": ["WORKING"], "guards": {"checked": "audit"}}},
        "metrics": {
            "pass_rate": {"source": "checks", "item": "r", "filter": "r.outcome != Outcome.NA", "dimensions": {"engineer": "r.subject.engineer"},
                          "time_dimension": "r.occurred_at", "expression": rate.format("1.000"), "flag_when": {"low": "value < 0.900"}},
            "pass_share": {"source": "checks", "item": "r", "dimensions": {"engineer": "r.subject.engineer"}, "expression": rate.format("1.00")},
            "passes": {"source": "checks", "item": "r", "filter": "r.outcome == Outcome.PASS", "dimensions": {"engineer": "r.subject.engineer"},
                       "time_dimension": "r.occurred_at", "expression": "count()"},
            "jobs_opened": {"source": "objects", "item": "o", "dimensions": {"engineer": "o.engineer"}, "time_dimension": "o.created_at",
                            "expression": "count()"},
            "time_working": {"measure": "median_time_in_state", "state": "WORKING", "group_by": ["engineer", "month"]},
            "slow_working": {"source": "intervals", "item": "i", "filter": "i.state == WORKING", "expression": "percentile(0.8, i.duration)"},
            "starts": {"measure": "transition_count", "transition": "start", "group_by": ["actor", "month"]},
            "tagged": {"source": "objects", "item": "o", "dimensions": {"tag": "o.tags"}, "expression": "count()"},
            "tags_per_job": {"source": "objects", "item": "o", "expression": "avg(count(t in o.tags))"},
            "tags_tripled": {"source": "objects", "item": "o", "expression": "avg(count(t in o.tags)) * 3"},
            "backs": {"measure": "transition_count", "transition": "back"},
            "time_held": {"source": "intervals(engineer)", "item": "i", "filter": "i.value is not null", "expression": "sum(i.duration)"},
            "held_by_month": {"source": "intervals(engineer)", "item": "i", "split_by": "month", "filter": "i.value is not null",
                              "dimensions": {"month": "i.period"}, "expression": "sum(i.duration)"},
            "working_by_month": {"source": "intervals", "item": "i", "split_by": "month", "filter": "i.state == WORKING",
                                 "dimensions": {"month": "i.period"}, "expression": "sum(i.duration)"},
            "jobs_working_by_month": {"source": "intervals", "item": "i", "split_by": "month", "filter": "i.state == WORKING",
                                      "dimensions": {"month": "i.period"}, "expression": "count()"},
            "takes": {"measure": "transition_count", "transition": "take", "group_by": ["engineer"]},
            "jobs_total": {"source": "objects", "item": "o", "expression": "count()"},
            "engineers_checked": {"source": "checks", "item": "r", "expression": "count(distinct r.subject.engineer)"},
            "anas_jobs": {"source": "objects", "item": "o", "filter": 'o.engineer.login == "ana"', "expression": "count()"},
            "third_of_checks": {"source": "checks", "item": "r", "expression": "count() / 3"},
            "outcome_pairs": {"source": "objects", "item": "o", "dimensions": {"outcome": "o.checks.outcome", "again": "o.checks.outcome"},
                              "expression": "count()"},
            "done_spans": {"source": "intervals", "item": "i", "filter": "i.state == DONE", "dimensions": {"engineer": "i.held(engineer)"},
                           "expression": "max(i.duration)", "flag_when": {"long": "value > 1 h"}},
            "labelled": {"source": "labels", "item": "l", "dimensions": {"name": "l.name", "state": "l.subject_state",
                                                                         "engineer": "l.subject.engineer"}, "expression": "count()"},
            "labels_total": {"source": "labels", "item": "l", "expression": "count()"}}}
    module = {"enumerations": {"Outcome": ["PASS", "FAIL", "NA"]},
              "types": {"User": {"attributes": {"login": {"type": "identity", "actor_kind": "human"}},
                                 "states": {"ACTIVE": {"category": "live"}},
                                 "transitions": {"add": {"kind": "initial", "to": "ACTIVE", "required_inputs": ["login"]}}},
                        "Job": job},
              "metrics": {"passes_of_all": {"input_metrics": {"passes": "Job.passes", "all": "Job.jobs_total"},
                                            "group_by": ["engineer"], "expression": "passes * 1.000 / all"},
                          "passes_per_job": {"input_metrics": {"passes": "Job.passes", "jobs": "Job.jobs_opened"},
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
    # an input without a dimension the read binds contributes its whole value (ADR-0124, ADR-0147)
    check("a combined metric whose input lacks the bound dimension", read(s, j1, "metric(passes_of_all, engineer := engineer)"),
          Fraction(667, 1000))
    check("a distinct count leaves out an absent value", read(s, j1, "metric(engineers_checked)"), 1)
    check("a filter unknown for a row selects it not", read(s, j1, "metric(anas_jobs)"), 1)
    check("int / int truncates in a metric's value, before the rounding", read(s, j1, "metric(third_of_checks)"), 1)
    # a reader's read: a row per group of the dimensions kept, in their order, an absent value last (library-api.md §5)
    r, s = world()
    ana, ben = req(r, s, "User", "add", login="ana"), req(r, s, "User", "add", login="ben")
    k1 = req(r, s, "Job", "open", engineer=ana, tags=["urgent"])
    k2 = req(r, s, "Job", "open", engineer=ben, tags=["urgent", "vip"])
    k0 = req(r, s, "Job", "open")
    record(r, s, k1, "PASS", "FAIL")
    record(r, s, k2, "PASS", "PASS")
    record(r, s, k0, "FAIL")
    page = lambda name, **kw: Evaluator(s, None).read_page(name, **kw)
    rows = lambda p: [(r_["dimensions"], r_["value"]) for r_ in p["rows"]]
    rate = page("Job.pass_rate")
    check("a row per engineer, the absent one last, as their objects were created",
          rows(rate), [({"engineer": ana}, Fraction(1, 2)), ({"engineer": ben}, 1), ({"engineer": ABSENT}, 0)])
    check("a flag on each declared group it holds for", [(f["flag"], f["group"], f["unknown"]) for f in rate["flags"]],
          [("low", {"engineer": ana}, False), ("low", {"engineer": ABSENT}, False)])
    check("nothing kept, one row over every row", rows(page("Job.pass_rate", keep=[])), [({}, Fraction(3, 5))])
    check("nothing kept, one row even over no rows", rows(page("Job.pass_rate", keep=[], filt="r.outcome == Outcome.NA")), [({}, ABSENT)])
    check("a dimension bound, its rows alone", rows(page("Job.pass_rate", bind={"engineer": ben}, keep=[])), [({}, 1)])
    check("a reader's filter narrows the rows before they are grouped",
          rows(page("Job.pass_rate", filt="r.outcome == Outcome.PASS")), [({"engineer": ana}, 1), ({"engineer": ben}, 1)])
    check("a set dimension, an object under each value and an absent one under none",
          rows(page("Job.tagged")), [({"tag": "urgent"}, 2), ({"tag": "vip"}, 1), ({"tag": ABSENT}, 1)])
    check("two dimensions through one set taken per member, never crossed, a value two members share once", rows(page("Job.outcome_pairs")),
          [({"outcome": ("enum", "Outcome", "PASS"), "again": ("enum", "Outcome", "PASS")}, 2),
           ({"outcome": ("enum", "Outcome", "FAIL"), "again": ("enum", "Outcome", "FAIL")}, 2)])
    check("a combined metric joined over every group, a missing input contributing its value over no rows",
          rows(page("passes_per_job")), [({"engineer": ana}, 1), ({"engineer": ben}, 2), ({"engineer": ABSENT}, 0)])
    k3 = req(r, s, "Job", "open", engineer=ana)
    for x in ("start", "shelve", "close"):
        req(r, s, "Job", x, k3)
    check("a flag unknown on a group whose value is", [(f["flag"], f["group"], f["unknown"]) for f in page("Job.done_spans")["flags"]],
          [("long", {"engineer": ana}, True)])
    # read leaving every evaluator unanswered, as an offer and the studio read it, a row's flow function is computed (D534)
    check("a page asking no evaluator reads a row's flow function", rows(Evaluator(s, None, stubs=NEVER_ASKED).read_page("Job.done_spans")),
          rows(page("Job.done_spans")))
    check("each refusal a value", [page(*a, **kw)["refused"] for a, kw in [
        (("nonesuch",), {}), (("Job.pass_rate",), {"keep": ["nobody"]}), (("passes_per_job",), {"filt": "true"}),
        (("Job.tagged",), {"over": 7 * day}), (("Job.jobs_opened",), {"filt": 'o.memo == "x"'})]],
          ["unknown_metric", "unknown_dimension", "filter_on_combined", "no_window", "personal"])
    # the attempt log: a refusal, a flag and an audit, each a row, read as its rows and its daily counts (DESIGN.md §6, §7)
    r, s = world()
    ana = req(r, s, "User", "add", login="ana")
    jr = req(r, s, "Job", "open", engineer=ana)
    req(r, s, "Job", "start", jr)
    req(r, s, "Job", "review", jr)                          # its warn guard fails: a flag
    req(r, s, "Job", "inspect", jr)                         # its audit guard fails: a would-be refusal
    req(r, s, "Job", "finish", jr)                          # no checks: the pass rate unknown, refused
    req(r, s, "Job", "nonesuch", jr)                        # no such transition
    try:
        r.attempt(s, "Job", "review", jr, "nobody", {}, {}, {})     # an actor the store does not hold
    except Refused:
        pass
    aliases, stubs = {"ana": ana, "jr": jr, OPERATOR: OPERATOR}, {}
    step(r, s, {"propose": {"object": "jr", "transition": "finish", "proposal": "p"}}, aliases, stubs)
    try:
        step(r, s, {"approve": {"proposal": "p"}}, aliases, stubs)
    except Refused:
        pass
    check("each refusal, flag and audit a row of the attempt log",
          [(a["type"], a["object"], a["transition"], a["verdict"], a["clause"], a["unknown"], a["enforced"], a["flagged"],
            a["actor_kind"], a["proposal"] is not None) for a in s.attempts],
          [("Job", jr, "review", "unsatisfied", "checked", False, False, True, "human", False),
           ("Job", jr, "inspect", "unsatisfied", "checked", False, False, False, "human", False),
           ("Job", jr, "finish", "unsatisfied", "good", True, True, False, "human", False),
           ("Job", jr, "nonesuch", "unknown_transition", None, False, True, False, "human", False),
           ("Job", jr, "review", "unsatisfied", "actor_known", False, True, False, "unknown", False),
           ("Job", jr, "finish", "unsatisfied", "good", True, True, False, "human", True)])
    check("the standard refusals, each enforced refusal of a transition",
          (read(s, jr, 'metric(Job.refusals, transition := "finish")'), read(s, jr, "metric(Job.refusals)")), (2, 4))
    check("the standard flags raised, and the audits counted by neither",
          read(s, jr, 'metric(Job.flags_raised, clause := "checked")'), 1)
    check("the standard refusal rate, a transition never applied refused every time",
          read(s, jr, 'metric(Job.refusal_rate, transition := "finish", clause := "good")'), 1)
    check("an object's own attempts", read(s, jr, "count(a in this.attempts where a.enforced)"), 4)
    # labels: the built-in kind every type carries, its name normalised, its subject's state stamped, dated when it is
    # recorded, and read only by metrics (declaration-syntax.md §6.8, §6.9)
    r, s = world()
    ana = req(r, s, "User", "add", login="ana")
    jl, jm = req(r, s, "Job", "open", engineer=ana), req(r, s, "Job", "open")
    l1 = req(r, s, LABEL, "record", subject=jl, name="  Late-Risk ")
    check("a label's name empty once normalised, refused as invalid input (ADR-0161)",
          req(r, s, LABEL, "record", subject=jl, name="   "), ("invalid_input", None))
    req(r, s, "Job", "start", jl)
    s.now += 60
    l2 = req(r, s, LABEL, "record", subject=jl, name="late-risk", note="rang twice")
    l3 = req(r, s, LABEL, "record", subject=jm, name="Odd")
    l4 = req(r, s, LABEL, "record", subject=jm, name="odd-one", corrects=l3)
    check("a label's name normalised, its subject's state stamped, and its occurred time its recorded time",
          [(s.objects[x].attrs["name"], s.objects[x].attrs["subject_state"], s.objects[x].attrs["occurred_at"],
            s.objects[x].created_by_kind) for x in (l1, l2, l4)],
          [("late-risk", "OPEN", s.now - 60, "human"), ("late-risk", "WORKING", s.now, "human"),
           ("odd-one", "OPEN", s.now, "human")])
    check("labelling stamps the subject's labels position", s.objects[jl].writes.get("labels"), s.objects[l2].writes.get("name"))
    try:
        r.attempt(s, LABEL, "record", None, OPERATOR, {"subject": jl, "name": "x"}, {jl: jl}, {}, occurred=s.now - 60)
        backdated = "applied"
    except Refused as e:
        backdated = (e.verdict, e.clause)
    for x in ("start", "shelve", "close"):
        req(r, s, "Job", x, jm)
    try:
        r.attempt(s, LABEL, "record", None, OPERATOR, {"subject": "nothing", "name": "x"}, {"nothing": "nothing"}, {})
        dangling = "applied"
    except Refused as e:
        dangling = (e.verdict, e.clause)
    check("each refusal of a label: a backdated one, an input it does not take, a subject naming nothing, a correction of a "
          "corrected one, one of another subject's, and a new fact on a finished subject",
          [backdated, req(r, s, LABEL, "record", subject=jl, name="x", subject_state="DONE"),
           dangling, req(r, s, LABEL, "record", subject=jm, name="x", corrects=l3),
           req(r, s, LABEL, "record", subject=jl, name="x", corrects=l4), req(r, s, LABEL, "record", subject=jm, name="x")],
          [("unsatisfied", "occurred_within"), ("invalid_input", None), ("invalid_input", None),
           ("unsatisfied", "corrects_current"), ("invalid_input", None), ("unsatisfied", "subject_open")])
    l5 = req(r, s, LABEL, "record", subject=jm, name="odd-two", corrects=l4)
    check("a correction of a label on a finished subject applies, keeping the corrected one's occurred time",
          (s.objects[l5].attrs["name"], s.objects[l5].attrs["occurred_at"]), ("odd-two", s.objects[l4].attrs["occurred_at"]))
    check("labelling offered on a finished subject is unavailable, refused as subject_open",
          [(x[2], x[3].clause) for x in r.availability(s, OPERATOR, jm) if x[:2] == ("record", LABEL)], [("unavailable", "subject_open")])
    check("a metric over labels counts each label nothing has corrected",
          (read(s, jl, "metric(Job.labels_total)"), read(s, jl, 'metric(Job.labelled, name := "late-risk")'),
           read(s, jl, 'metric(Job.labelled, name := "late-risk", state := "OPEN")'),
           read(s, jl, "metric(Job.labelled, engineer := who)", who=ana), read(s, jl, 'metric(Job.labelled, name := "odd-two")')),
          (3, 2, 1, 2, 1))
    check("a reader's filter on a label's note, which is personal, is refused",
          page("Job.labelled", filt='l.note == "x"')["refused"], "personal")
    # a span holds what the transition that began it wrote, and an event what was held before it (§6.9, ADR-0146)
    r, s = world()
    ana, ben = req(r, s, "User", "add", login="ana"), req(r, s, "User", "add", login="ben")
    jt = req(r, s, "Job", "open", engineer=ana)
    req(r, s, "Job", "take", jt, engineer=ben)
    s.now += 2 * 3600
    req(r, s, "Job", "back", jt)
    check("a span begun by a transition that assigns it holds the new assignee",
          (read(s, jt, "metric(time_working, engineer := who)", who=ben), read(s, jt, "metric(time_working, engineer := who)", who=ana)),
          (2 * 3600, ABSENT))
    check("an event holds the assignee before its own writes",
          (read(s, jt, "metric(takes, engineer := who)", who=ana), read(s, jt, "metric(takes, engineer := who)", who=ben)), (1, 0))
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
    check("an average rounded once, at the end, and not before it is tripled", read(s, ja, "metric(tags_tripled)"), 4)
    req(r, s, "Job", "start", jb)
    req(r, s, "Job", "pause", jb)
    check("a transition count by name, apart from another between the same states", read(s, ja, "metric(backs)"), 5)
    check("the states in their declared order, a state no span holds now left out",
          [(r_["dimensions"]["state"][2], r_["value"]) for r_ in Evaluator(s, None).read_page("Job.time_in_state", keep=["state"])["rows"]],
          [("OPEN", 0), ("WORKING", 2 * 3600)])     # WORKING's spans 0, 1, 2, 3, 4 and 6 h; the third
    # a span still current accrues nothing over an unbroken stretch of closed states (ADR-0146)
    r, s = world()
    eve = req(r, s, "User", "add", login="eve")
    je = req(r, s, "Job", "open", engineer=eve)
    req(r, s, "Job", "start", je)
    s.now += 3600
    req(r, s, "Job", "shelve", je)
    s.now += 5 * 3600
    req(r, s, "Job", "close", je)
    s.now += 7 * 3600
    check("an assignment still current stops at the start of the closed stretch", read(s, je, "metric(time_held)"), 3600)
    check("time in the closed state before the terminal one is still measured", read(s, je, "time_in(SHELVED)"), 5 * 3600)
    check("time in the closed state the object is in, real elapsed time", read(s, je, "time_in(DONE)"), 7 * 3600)
    # a member's span accrues only while its object is open: one ending after the close stops there, and one outlasting
    # a closed stretch the object reopened from leaves it out, a split piece likewise (ADR-0148, the author deciding)
    r, s = world()
    ann, bob = req(r, s, "User", "add", login="ann"), req(r, s, "User", "add", login="bob")
    jx = req(r, s, "Job", "open", engineer=ann)
    req(r, s, "Job", "start", jx)
    s.now += 2 * 3600
    req(r, s, "Job", "shelve", jx)
    s.now += 3 * 3600
    req(r, s, "Job", "assign", jx, engineer=bob)
    s.now += 4 * 3600
    check("a member's span ending after its object closed stops at the close", read(s, jx, "metric(time_held)"), 2 * 3600)
    r, s = world()
    cy = req(r, s, "User", "add", login="cy")
    jy = req(r, s, "Job", "open", engineer=cy)
    req(r, s, "Job", "start", jy)
    s.now += 3600
    req(r, s, "Job", "shelve", jy)
    s.now += 3 * 3600
    req(r, s, "Job", "resume", jy)
    s.now += 2 * 3600
    check("a member's span leaves out a closed stretch its object reopened from", read(s, jy, "metric(time_held)"), 3 * 3600)
    r, s = world()
    dee = req(r, s, "User", "add", login="dee")
    s.now = timestamp("2026-01-31T12:00Z")
    jz = req(r, s, "Job", "open", engineer=dee)
    req(r, s, "Job", "start", jz)
    s.now = timestamp("2026-01-31T18:00Z")
    req(r, s, "Job", "shelve", jz)
    s.now = timestamp("2026-02-01T06:00Z")
    req(r, s, "Job", "resume", jz)
    s.now = timestamp("2026-02-01T12:00Z")
    check("a member's split piece counts only its open time",
          (read(s, jz, 'metric(held_by_month, month := "2026-01")'), read(s, jz, 'metric(held_by_month, month := "2026-02")')), (6 * 3600, 6 * 3600))
    # a span split across the months it covers, a current one cut at now, and a window clipping the pieces (ADR-0142)
    r, s = world()
    one, two = req(r, s, "Job", "open"), req(r, s, "Job", "open")
    s.now = timestamp("2026-01-31T12:00Z")
    req(r, s, "Job", "start", one)
    s.now = timestamp("2026-02-02T00:00Z")
    req(r, s, "Job", "back", one)
    s.now = timestamp("2026-02-28T18:00Z")
    req(r, s, "Job", "start", two)
    s.now = timestamp("2026-03-01T06:00Z")
    hours = lambda text: read(s, one, text) / 3600
    check("a span's piece in the month it began", hours('metric(working_by_month, month := "2026-01")'), 12)
    check("its piece in the next month, with a current span's piece up to the month's end",
          hours('metric(working_by_month, month := "2026-02")'), 30)
    check("a current span's piece up to now", hours('metric(working_by_month, month := "2026-03")'), 6)
    check("a window clipping the pieces to its last ten hours", hours("metric(working_by_month, over last 10 h)"), 10)
    check("a split metric counting the spans active in a month", read(s, one, 'metric(jobs_working_by_month, month := "2026-02")'), 2)
    # the live offer: decided on the actor, the state and the guards that read no input (ADR-0152)
    r, s = world()
    ana = req(r, s, "User", "add", login="ana")
    jo = req(r, s, "Job", "open")

    def offered(oid, actor=OPERATOR):
        return {(x[0], x[1]): (x[2], (x[3].verdict, x[3].clause) if x[3] else None, x[5], x[6])
                for x in r.availability(s, actor, oid)}

    o = offered(jo)
    check("an offer for each requestable transition, the initial left out, and the recording of each kind on it",
          sorted(o), sorted([("assign", None), ("resume", None), ("start", None), ("back", None), ("pause", None),
                             ("shelve", None), ("close", None), ("finish", None), ("take", None),
                             ("review", None), ("inspect", None), ("record", "Check"), ("record", "label")]))
    check("labelling is offered on every subject, its name waiting for the caller (ADR-0099)",
          o[("record", "label")], ("available_with_input", None, [], []))
    check("a transition its state allows, taking no input, is available", o[("start", None)], ("available", None, [], []))
    check("one taking a required input is available with input, its guard on that input left for the input",
          o[("assign", None)], ("available_with_input", None, [], []))
    check("one its state does not allow is unavailable, refused as a request would be",
          o[("resume", None)], ("unavailable", ("unavailable", None), [], []))
    check("recording a kind on the subject waits for its fields", o[("record", "Check")][0], "available_with_input")
    check("an actor the store does not hold is refused every offer as actor_known",
          {v[1] for v in offered(jo, actor="nobody").values()}, {("unsatisfied", "actor_known")})
    req(r, s, "Job", "start", jo)
    check("a guard that reads no input is decided, its refusal the offer's verdict",
          offered(jo)[("finish", None)][:2], ("unavailable", ("unsatisfied", "good")))
    req(r, s, "Job", "shelve", jo)
    req(r, s, "Job", "close", jo)
    check("recording on a finished subject is refused as subject_open",
          offered(jo)[("record", "Check")][1], ("unsatisfied", "subject_open"))
    warned = copy.deepcopy(kinds)
    warned["Job"]["transitions"]["finish"]["guards"] = {"good": "warn"}
    r = Runner(warned, enums)
    s = Store(warned, enums, declared, homes)
    jw = r.attempt(s, "Job", "open", None, OPERATOR, {}, {}, {})[1]
    r.attempt(s, "Job", "start", jw, OPERATOR, {}, {}, {})
    check("a warning guard that would fail is a flag on an available offer, never a refusal",
          # its clause unknown, reading a value nobody has yet, and naming no object (ADR-0179)
          offered(jw)[("finish", None)], ("available", None, [], [("good", "dependent", True, [])]))
    print(f"  metrics: a rate windowed and bound, a combined one, rounding half to even, medians and percentiles, time held "
          f"by who held the job, counts, set dimensions, and the standard ones, as guards read them: {'yes' if ok else 'NO'}")
    return ok

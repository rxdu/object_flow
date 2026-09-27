#!/usr/bin/env python3
"""Recompute the values docs/design/metric-scenarios.md expects, from its history.

The scenarios fix a flow, the robot inventory of unit-journey.md with the agent
type the document declares, a history of requests over it, and the values the
standard and declared metrics give when read at the history's `now`. This:

  1. replays the history against the modules: every request names an actor the
     store holds, whose kind is its type's; every transition is one its type
     declares, requestable, of the request's kind and taken from a state it
     leaves; every refusal names a guard the transition declares, with its
     declared remedy, or a verdict with the remedy DESIGN.md §5.5 fixes; and
     every guard, effect and invariant the history reaches is evaluated from a
     transcription this script holds, each checked against the module's text,
     so an applied request passes its guards and a refused one fails first on
     the clause it names;
  2. publishes the lead's metric into the module and runs the flow checker;
  3. computes each read the document quotes, by declaration-syntax.md §6.9's
     rules, and compares it with the document's table.

It is not an engine: it evaluates only what it transcribes, and refuses a
history that reaches anything else. Its self-test, which runs after the check
unless `--no-self-test` is given, plants a wrong value, an illegal transition, a
refusal on the wrong clause, an unknown agent and two invalid published metrics,
and shows each caught.
"""
import importlib.util
import math
import pathlib
import re
import sys
from datetime import datetime, timedelta

import yaml

ROOT = pathlib.Path(__file__).resolve().parents[1]
DOC = ROOT / "docs/design/metric-scenarios.md"
JOURNEY = ROOT / "docs/design/unit-journey.md"
spec = importlib.util.spec_from_file_location("flows", ROOT / "scripts/check-flows.py")
flows = importlib.util.module_from_spec(spec)
spec.loader.exec_module(flows)


class Refused(Exception):
    pass


def when(s):
    return datetime.strptime(str(s), "%Y-%m-%dT%H:%MZ")


def fences(text):
    out, info, body, start = [], None, [], 0
    for i, line in enumerate(text.split("\n"), 1):
        if info is None and line.startswith("```"):
            info, body, start = line[3:].strip(), [], i + 1
        elif info is not None and line.strip() == "```":
            out.append((info, start, "\n".join(body) + "\n"))
            info = None
        elif info is not None:
            body.append(line)
    return out


def modules_of(text):
    """The modules a document writes, a `# <module>, continued` block appended to its module."""
    mods = {}
    for info, _start, body in fences(text):
        if info != "yaml":
            continue
        c = re.match(r"# (\w+), continued\n", body)
        if c:
            mods[c.group(1)] += body[c.end():]
        else:
            mods[re.search(r"^module: (\w+)", body, re.M).group(1)] = body
    return mods


def one(expr):
    return " ".join(str(expr).split())


# ── the transcription: what the history reaches, as the modules say it ─────────
# Each guard, invariant and effect below is the module's text and a Python
# reading of it; `transcribed()` refuses to run if the module's text differs.
GUARDS = {
    ("Delivery", "not_internal"): ("not internal", lambda s, o, i: not o.attrs.get("internal", False)),
    ("Delivery", "is_internal"): ("internal", lambda s, o, i: bool(o.attrs.get("internal", False))),
    ("Delivery", "filled"): ("count(u in units) >= 1 and none(u in pegged)",
                             lambda s, o, i: len(s.units(o.id)) >= 1 and not s.pegged(o.id)),
    ("Robot", "open"): ("inputs.slot.state == Delivery.PREPARATION",
                        lambda s, o, i: s.objects[i["slot"]].state == "PREPARATION"),
    ("Robot", "labelled"): ("label_printed_at is not null", lambda s, o, i: o.attrs.get("label_printed_at") is not None),
    ("Robot", "mfr_serial"): ("model.manufacturer_serial_required implies manufacturer_serial is not null",
                              lambda s, o, i: not s.objects[o.attrs["model"]].attrs.get("manufacturer_serial_required", False)
                              or o.attrs.get("manufacturer_serial") is not None),
    ("Robot", "photo"): ("model.label_photo_required implies count(p in photos) >= 1",
                         lambda s, o, i: not s.objects[o.attrs["model"]].attrs.get("label_photo_required", False)
                         or len(o.attrs.get("photos", [])) >= 1),
}
INVARIANTS = {
    ("Robot", "one_open_engagement"): ("count(l in engagement_lines where l.open) <= 1", lambda s, o: True),
    ("Robot", "one_claim"): ("binding is null or used_in is null",
                             lambda s, o: o.attrs.get("binding") is None or o.attrs.get("used_in") is None),
    ("Robot", "labelled"): ("state != AVAILABLE or label_printed_at is not null",
                            lambda s, o: o.state != "AVAILABLE" or o.attrs.get("label_printed_at") is not None),
    ("Robot", "mfr_serial"): ("state != AVAILABLE or not model.manufacturer_serial_required or manufacturer_serial is not null",
                              lambda s, o: o.state != "AVAILABLE"
                              or not s.objects[o.attrs["model"]].attrs.get("manufacturer_serial_required", False)
                              or o.attrs.get("manufacturer_serial") is not None),
    ("RobotModel", "reorder_point_nonneg"): ("reorder_point >= 0", lambda s, o: o.attrs.get("reorder_point", 0) >= 0),
}


def call_each(array, transition, inputs=None, where=None):
    """A foreach step calling `transition` on each element of a derived set end."""
    def run(s, o, i, actor, at):
        items = s.units(o.id) if array == "units" else s.pegged(o.id)
        for u in sorted(items):
            if where is None or s.objects[u].state == where:
                s.call(u, "Robot", transition, {k: v(s, o) for k, v in (inputs or {}).items()}, actor, at)
    return run


EFFECTS = {
    # (type, transition): (the module's effect, as loaded; what it does)
    ("Delivery", "peg_slot"): ([{"call": {"target": "inputs.robot", "transition": "peg_to", "inputs": {"slot": "this"}}}],
                               lambda s, o, i, a, t: s.call(i["robot"], "Robot", "peg_to", {"slot": o.id}, a, t)),
    ("Delivery", "bind_slot"): ([{"call": {"target": "inputs.robot", "transition": "reserve", "inputs": {"slot": "this"}}}],
                                lambda s, o, i, a, t: s.call(i["robot"], "Robot", "reserve", {"slot": o.id}, a, t)),
    ("Delivery", "complete_sale"): ([{"foreach": {"item": "u", "array": "units", "limit": 500, "steps": [
        {"call": {"target": "u", "transition": "sell", "inputs": {"buyer": "customer"}}}]}}],
        call_each("units", "sell", {"buyer": lambda s, o: o.attrs["customer"]})),
    ("Delivery", "complete_internal"): ([{"foreach": {"item": "u", "array": "units", "limit": 500, "steps": [
        {"call": {"target": "u", "transition": "deliver_internal"}}]}}], call_each("units", "deliver_internal")),
    ("Delivery", "revoke"): ([{"foreach": {"item": "u", "array": "units", "where": "u.state == Robot.SOLD", "limit": 500, "steps": [
        {"call": {"target": "u", "transition": "unsell"}}]}},
        {"foreach": {"item": "u", "array": "units", "where": "u.state == Robot.DEVELOPMENT", "limit": 500, "steps": [
            {"call": {"target": "u", "transition": "recall_internal"}}]}}],
        lambda s, o, i, a, t: (call_each("units", "unsell", where="SOLD")(s, o, i, a, t),
                               call_each("units", "recall_internal", where="DEVELOPMENT")(s, o, i, a, t))),
    ("Robot", "peg_to"): ([{"assign": {"location": "peg", "expr": "inputs.slot"}}], lambda s, o, i, a, t: o.set("peg", i["slot"])),
    ("Robot", "reserve"): ([{"assign": {"location": "binding", "expr": "inputs.slot"}}], lambda s, o, i, a, t: o.set("binding", i["slot"])),
    ("Robot", "sell"): ([{"assign": {"location": "sold_to", "expr": "inputs.buyer"}}], lambda s, o, i, a, t: o.set("sold_to", i["buyer"])),
    ("Robot", "unsell"): ([{"clear": ["binding", "sold_to"]}], lambda s, o, i, a, t: o.clear("binding", "sold_to")),
    ("Robot", "record_label_print"): ([{"assign": {"location": "label_printed_at", "expr": "now"}}],
                                      lambda s, o, i, a, t: o.set("label_printed_at", t)),
    ("Robot", "inventorize"): ([{"clear": ["peg"]}], lambda s, o, i, a, t: o.clear("peg")),
    ("Robot", "correct_state"): ([{"clear": ["peg", "binding", "used_in"]}], lambda s, o, i, a, t: o.clear("peg", "binding", "used_in")),
}
NO_EFFECT = {("Delivery", "open"), ("Robot", "add_opening_stock"), ("Robot", "add_to_intake"), ("Robot", "deliver_internal"),
             ("RobotModel", "add"), ("User", "add"), ("Agent", "issue")}
FIXED_REMEDY = {"unavailable": "unreachable_from_here", "stale": "self_serviceable", "not found": "unreachable_from_here",
                "not requestable": "unreachable_from_here", "invalid input": "self_serviceable",
                "unknown transition": "self_serviceable"}


def literal(v):
    """A loaded effect with its expressions as the loader leaves them, for comparison."""
    if isinstance(v, dict):
        return {k: literal(x) for k, x in v.items()}
    if isinstance(v, list):
        return [literal(x) for x in v]
    return one(v) if isinstance(v, str) else v


class Obj:
    def __init__(self, oid, tn, state, attrs, at, kind):
        self.id, self.type, self.state, self.attrs = oid, tn, state, dict(attrs)
        self.created_at, self.recorded_from, self.state_source = at, at, "observed"
        self.intervals = []                         # [state, entered, left, entered_by_kind]
        self.open_interval(state, at, kind)

    def open_interval(self, state, at, kind):
        if self.intervals:
            self.intervals[-1][2] = at
        self.intervals.append([state, at, None, kind])

    def set(self, k, v):
        self.attrs[k] = v

    def clear(self, *ks):
        for k in ks:
            self.attrs[k] = None


class Store:
    def __init__(self, mods, history):
        self.mods, self.h = mods, history
        self.now, self.port = when(history["now"]), when(history["port"])
        self.objects, self.events, self.attempts = {}, [], []
        self.types = {}
        for m in mods.values():
            for tn, t in (m.get("types") or {}).items():
                view = dict(t)
                if t.get("state_machine"):
                    mach = m["machines"][t["state_machine"]]
                    view = {**mach, **t, "conditions": {**(mach.get("conditions") or {}), **(t.get("conditions") or {})},
                            "transitions": {**(mach.get("transitions") or {}), **(t.get("transitions") or {})}}
                self.types[tn] = view
        self.transcribed()

    def transcribed(self):
        for (tn, c), (text, _f) in GUARDS.items():
            got = one(self.types[tn]["conditions"][c]["expression"])
            assert got == text, f"{tn}.{c} is now `{got}`; the transcription reads `{text}`"
        for (tn, c), (text, _f) in INVARIANTS.items():
            got = one(self.types[tn]["invariants"][c]["expression"])
            assert got == text, f"{tn}.{c} is now `{got}`; the transcription reads `{text}`"
        for tn, t in self.types.items():
            for c in (t.get("invariants") or {}):
                assert (tn, c) in INVARIANTS or tn not in ("Robot", "RobotModel"), f"{tn}.{c} is not transcribed"
        for (tn, x), (effect, _f) in EFFECTS.items():
            got = literal(self.types[tn]["transitions"][x].get("effect"))
            assert got == literal(effect), f"{tn}.{x}'s effect is now {got}; the transcription reads {literal(effect)}"
        for tn, x in NO_EFFECT:
            assert not self.types[tn]["transitions"][x].get("effect"), f"{tn}.{x} now has an effect"

    # derived ends, read from the stored ones
    def units(self, d):
        return {r for r, o in self.objects.items() if o.type == "Robot" and o.attrs.get("binding") == d}

    def pegged(self, d):
        return {r for r, o in self.objects.items() if o.type == "Robot" and o.attrs.get("peg") == d}

    def final(self, tn, state):
        return bool(self.types[tn]["states"][state].get("final"))

    def open(self, o):
        st = self.types[o.type]["states"][o.state]
        return st.get("category") != "closed" and not st.get("final")

    def kind_of(self, actor):
        o = self.objects.get(actor)
        if o is None:
            raise Refused(f"actor {actor} names no object the store holds")
        marks = [a["actor_kind"] for a in (self.types[o.type].get("attributes") or {}).values() if a.get("actor_kind")]
        if not marks:
            raise Refused(f"actor {actor} is a {o.type}, which marks no actor identity")
        return marks[0]

    def leaves(self, tn, x, state):
        x = flows.resolve_any(x, self.types[tn]["states"]) if x.get("from") == "any" else x
        f = x.get("from")
        return state in (f if isinstance(f, list) else [f])

    def guards(self, tn, xn, x, o, inputs):
        """The first declared guard that fails, or None; every guard reached must be transcribed."""
        for g in (x.get("guards") or {}):
            if (tn, g) not in GUARDS:
                raise Refused(f"{tn}.{xn}'s guard {g} is not transcribed, so this history cannot reach it")
            if not GUARDS[(tn, g)][1](self, o, inputs):
                return g
        return None

    def check_invariants(self, o):
        for (tn, c), (_t, f) in INVARIANTS.items():
            if tn == o.type and not f(self, o):
                raise Refused(f"{o.id} would break {tn}.{c}")

    def record(self, o, xn, frm, to, kind, at, **extra):
        self.events.append({"object": o.id, "type": o.type, "transition": xn, "from": frm, "to": to, "kind": kind,
                            "at": at, "overrides": False, "imported": False, "reason": None, **extra})

    def call(self, oid, tn, xn, inputs, kind, at):
        o = self.objects[oid]
        x = self.types[tn]["transitions"][xn]
        if not self.leaves(tn, x, o.state):
            raise Refused(f"the call {tn}.{xn} on {oid}, in {o.state}, is from a state it does not leave")
        failed = self.guards(tn, xn, x, o, inputs)
        if failed:
            raise Refused(f"the call {tn}.{xn} on {oid} fails {failed}")
        self.apply(o, tn, xn, x, inputs, kind, at)

    def apply(self, o, tn, xn, x, inputs, kind, at, to=None):
        frm = o.state
        for a in (x.get("required_inputs") or []) + (x.get("optional_inputs") or []):
            if a in inputs:
                o.set(a, inputs[a])
        if (tn, xn) in EFFECTS:
            EFFECTS[(tn, xn)][1](self, o, inputs, kind, at)
        elif (tn, xn) not in NO_EFFECT and x.get("effect"):
            raise Refused(f"{tn}.{xn}'s effect is not transcribed")
        target = to or x.get("to")
        if x["kind"] in ("external", "assertion") and target:
            o.state = target
            o.state_source = "asserted" if x["kind"] == "assertion" else "observed"
            o.open_interval(target, at, kind)
        self.check_invariants(o)
        self.record(o, xn, frm, o.state, kind, at, overrides=x["kind"] == "assertion",
                    reason=inputs.get("reason") if x["kind"] == "assertion" else None)

    def replay(self):
        for imp in self.h.get("imported") or []:
            at = when(imp["entered"]) if imp.get("entered") else self.port
            o = Obj(imp["object"], imp["type"], imp["state"], {k: (when(v) if k.endswith("_at") else v)
                                                               for k, v in (imp.get("attributes") or {}).items()},
                    at, imp.get("entered_by", "unknown"))
            o.created_at = when(imp["created"]) if imp.get("created") else self.port
            o.state_source = "imported"
            silent = imp.get("silent")
            o.recorded_from = (when(silent[1]) if silent else o.created_at if imp.get("created") else self.port)
            self.objects[o.id] = o
            self.events.append({"object": o.id, "type": o.type, "transition": "import", "from": None, "to": o.state,
                                "kind": "unknown", "at": self.port, "overrides": True, "imported": True, "reason": None})
        last = None
        for r in self.h["requests"]:
            at = when(r["at"])
            assert last is None or at >= last, f"request at {r['at']} is out of order"
            last = at
            kind = self.kind_of(r["actor"])
            inputs = dict(r.get("inputs") or {})
            for k, v in inputs.items():
                if k.endswith("_at"):
                    inputs[k] = when(v)
            tn = r.get("create") or self.objects[r["object"]].type
            x = self.types[tn]["transitions"].get(r["transition"])
            if x is None:
                raise Refused(f"{tn} declares no transition {r['transition']}")
            if x.get("only_via"):
                raise Refused(f"{tn}.{r['transition']} is only via {x['only_via']}")
            if r.get("refused"):
                self.refuse(r, tn, x, inputs, kind, at)
                continue
            if r.get("create"):
                if x["kind"] != "initial" or r["object"] in self.objects:
                    raise Refused(f"{r['object']}: {tn}.{r['transition']} is not a creation of a new object")
                o = Obj(r["object"], tn, x["to"], {}, at, kind)
                self.objects[o.id] = o
                o.state = x["to"]
                self.apply(o, tn, r["transition"], {**x, "kind": "created"}, inputs, kind, at)
                continue
            o = self.objects[r["object"]]
            if x["kind"] == "initial":
                raise Refused(f"{tn}.{r['transition']} is a creation, requested on {o.id}")
            if x["kind"] == "assertion":
                to = inputs.get("to")
                if to not in x["to"] or to == o.state or self.final(tn, o.state):
                    raise Refused(f"{o.id}: {tn}.{r['transition']} to {to} from {o.state} is not an assertion it may make")
                self.apply(o, tn, r["transition"], x, inputs, kind, at, to=to)
                continue
            if not self.leaves(tn, x, o.state):
                raise Refused(f"{o.id}: {tn}.{r['transition']} is not taken from {o.state}")
            failed = self.guards(tn, r["transition"], x, o, inputs)
            if failed:
                raise Refused(f"{o.id}: {tn}.{r['transition']} fails {failed}, and the history says it applied")
            self.apply(o, tn, r["transition"], x, inputs, kind, at)

    def refuse(self, r, tn, x, inputs, kind, at):
        v = r["refused"]
        o = self.objects.get(r["object"])
        if v["verdict"] == "unsatisfied":
            if x.get("guards", {}).get(v["clause"]) is None:
                raise Refused(f"{tn}.{r['transition']} declares no guard {v['clause']}")
            declared = self.types[tn]["conditions"][v["clause"]]["remedy"]
            if v["remedy"] != declared:
                raise Refused(f"{v['clause']}'s remedy is {declared}, not {v['remedy']}")
            if not self.leaves(tn, x, o.state):
                raise Refused(f"{o.id} is in {o.state}, so the refusal would be unavailable, not {v['clause']}")
            first = self.guards(tn, r["transition"], x, o, inputs)
            if first != v["clause"]:
                raise Refused(f"{o.id}: {tn}.{r['transition']} fails first on {first}, not {v['clause']}")
        else:
            if FIXED_REMEDY.get(v["verdict"]) != v["remedy"]:
                raise Refused(f"a {v['verdict']} refusal carries {FIXED_REMEDY.get(v['verdict'])}, not {v['remedy']}")
            if v["verdict"] == "unavailable" and self.leaves(tn, x, o.state):
                raise Refused(f"{o.id} is in {o.state}, which {tn}.{r['transition']} leaves")
        self.attempts.append({"object": r["object"], "type": tn, "transition": r["transition"], "verdict": v["verdict"],
                              "clause": v.get("clause"), "remedy": v["remedy"], "kind": kind, "at": at, "enforced": True})


# ── the reads, by declaration-syntax.md §6.9 ────────────────────────────────────
def duration(s, o, iv):
    """A span's exit less its entry; a current one to `now` while the object is open, and none on a finished object."""
    state, entered, left, _k = iv
    if left is not None:
        return left - entered
    return s.now - entered if s.open(o) else None


def nearest_rank(values, p):
    vs = sorted(values)
    return vs[max(1, math.ceil(p * len(vs))) - 1] if vs else None


def week(t):
    y, w, _d = t.isocalendar()
    return f"{y}-W{w:02d}"


def interval_rows(s, tn):
    for o in s.objects.values():
        if o.type == tn:
            for iv in o.intervals:
                yield o, iv


def group(rows, keep):
    """Rows of (dimensions, body, gap) grouped by the kept dimensions; a set-valued dimension's
    row is counted once under each distinct value."""
    out = {}
    for dims, body, gap in rows:
        keys = [()]
        for d in keep:
            vals = dims[d] if isinstance(dims[d], (set, frozenset)) else {dims[d]}
            keys = [k + (v,) for k in keys for v in (sorted(vals, key=str) or [None])]
        for k in keys:
            g = out.setdefault(k, {"bodies": [], "gaps": set()})
            g["bodies"].append(body)
            if gap:
                g["gaps"].add(gap)
    return out


def read(s, spec):
    m = re.fullmatch(r"metric\((\w+(?:\.\w+)?), keep: \[([\w, ]*)\]\)", spec)
    if m:
        name, keep = m.group(1), [k.strip() for k in m.group(2).split(",") if k.strip()]
        return metric(s, name, keep), keep + ["value", "gaps"]
    if spec == "exceptions(Robot)":
        return [{"object": o.id, "state": o.state} for o in s.objects.values()
                if o.type == "Robot" and o.state_source == "asserted"], ["object", "state"]
    if spec == 'query(Delivery, filter: "state.category != closed", order: "entered_at(state)")':
        rows = [(o.intervals[-1][1], o) for o in s.objects.values() if o.type == "Delivery" and s.open(o)]
        return [{"object": o.id, "state": o.state, "entered": t, "gaps": {o.id} if t < o.recorded_from else set()}
                for t, o in sorted(rows, key=lambda r: r[0])], ["object", "state", "entered", "gaps"]
    raise SystemExit(f"no read of that form: {spec}")


def metric(s, name, keep):
    tn, _, std = name.partition(".")
    if name == "Delivery.time_in_state" or name == "delivery_wait":
        tn = "Delivery"
        rows = []
        for o, iv in interval_rows(s, tn):
            d = duration(s, o, iv)
            dims = {"state": iv[0], "month": iv[1].strftime("%Y-%m"), "actor_kind": iv[3],
                    "customer": o.attrs.get("customer"),
                    "configuration": frozenset(s.objects[u].attrs["model"] for u in s.units(o.id))}
            rows.append((dims, d, o.id if iv[1] < o.recorded_from else None))
        return [{**dict(zip(keep, k)), "value": nearest_rank([b for b in g["bodies"] if b is not None], 0.5), "gaps": g["gaps"]}
                for k, g in group(rows, keep).items()]
    if std in ("oldest_open", "work_in_progress"):
        rows = []
        for o in s.objects.values():
            if o.type == tn and s.open(o):
                entered = o.intervals[-1][1]
                body = s.now - entered if std == "oldest_open" else 1
                # a gap is something read from before the record vouches: `oldest_open` reads the current
                # interval's entry, `work_in_progress` only the current state
                gap = o.id if std == "oldest_open" and entered < o.recorded_from else None
                rows.append(({"state": o.state}, body, gap))
        agg = max if std == "oldest_open" else len
        return [{**dict(zip(keep, k)), "value": agg(g["bodies"]) if std == "oldest_open" else len(g["bodies"]), "gaps": g["gaps"]}
                for k, g in group(rows, keep).items()]
    if std == "refusals":
        counts = {}
        for a in s.attempts:
            if a["type"] == tn and a["enforced"]:
                key = (a["at"].date(), a["object"], a["transition"], a["verdict"], a["clause"], a["remedy"], a["kind"])
                counts[key] = counts.get(key, 0) + 1
        rows = [({"transition": k[2], "clause": k[4], "remedy": k[5], "actor_kind": k[6],
                  "week": week(datetime(k[0].year, k[0].month, k[0].day))}, n, None) for k, n in counts.items()]
        return [{**dict(zip(keep, k)), "value": sum(g["bodies"]), "gaps": set()} for k, g in group(rows, keep).items()]
    if std == "override_counts":
        rows = [({"state": e["to"], "reason": e["reason"], "week": week(e["at"]), "actor_kind": e["kind"]}, 1, None)
                for e in s.events if e["type"] == tn and e["overrides"] and not e["imported"]]
        return [{**dict(zip(keep, k)), "value": len(g["bodies"]), "gaps": set()} for k, g in group(rows, keep).items()]
    raise SystemExit(f"no metric {name} in this checker")


def show(v):
    if v is None:
        return "absent"
    if isinstance(v, timedelta):
        d, rem = v.days, v.seconds
        parts = [f"{d}d"] if d else []
        if rem // 3600:
            parts.append(f"{rem // 3600}h")
        if rem % 3600 // 60:
            parts.append(f"{rem % 3600 // 60}m")
        return " ".join(parts) or "0"
    if isinstance(v, datetime):
        return v.strftime("%Y-%m-%dT%H:%MZ")
    if isinstance(v, (set, frozenset)):
        return ", ".join(sorted(v)) if v else ""
    return str(v)


def expected_tables(text):
    """Each `Read:` line and the table under it, as (line, spec, header, rows)."""
    lines = text.split("\n")
    out = []
    for i, line in enumerate(lines):
        m = re.match(r"^Read: `(.+)`", line)
        if not m:
            continue
        j = i + 1
        while j < len(lines) and not lines[j].startswith("|"):
            j += 1
        header = [c.strip() for c in lines[j].strip("|").split("|")]
        rows, k = [], j + 2
        while k < len(lines) and lines[k].startswith("|"):
            rows.append([c.strip().strip("`") for c in lines[k].strip("|").split("|")])
            k += 1
        out.append((i + 1, m.group(1), header, rows))
    return out


def check(text, journey_text, verbose=True):
    """Findings, as strings; empty when the document's history and every value it quotes hold."""
    found = []
    blocks = fences(text)
    mods_text = modules_of(journey_text)
    mods_text.update(modules_of(text))
    history = yaml.safe_load(next(b for i, _s, b in blocks if i == "yaml scenario"))
    publish = next((b for i, _s, b in blocks if i == "yaml publish"), None)
    # 2. the lead's metric, published into Delivery, passes every step
    if publish:
        jt = mods_text["inventory_journey"]
        cut = jt.index("\n  ServiceJob:")
        spliced = jt[:cut].rstrip("\n") + "\n\n    metrics:\n" + "".join("      " + ln + "\n" for ln in publish.rstrip("\n").split("\n")) + jt[cut:]
        agents = [n for n in mods_text if n not in ("operations_shared", "inventory_journey")]
        files = [("operations_shared.yaml", mods_text["operations_shared"]), ("inventory_journey.yaml", spliced)]
        files += [(f"{n}.yaml", mods_text[n]) for n in agents]
        errs, _notes = flows.check(files)
        found += [f"the published metric: {e[0]}:{e[1]} {e[3]} {e[4]}" for e in errs]
    mods = {n: flows.load(t) for n, t in mods_text.items()}
    s = Store(mods, history)
    try:
        s.replay()
    except Refused as e:
        return found + [f"the history: {e}"]
    for line, spec_, header, rows in expected_tables(text):
        got, cols = read(s, spec_)
        if header != cols:
            found.append(f"line {line}: `{spec_}` has columns {header}, and the read returns {cols}")
            continue
        want = [tuple(r) for r in rows]
        have = [tuple(show(g[c]) for c in cols) for g in got]
        ordered = spec_.startswith("query(")
        if (want != have) if ordered else (sorted(want) != sorted(have)):
            found.append(f"line {line}: `{spec_}` expects\n      {sorted(want) if not ordered else want}\n    and the history gives\n      {sorted(have) if not ordered else have}")
        elif verbose:
            print(f"  {spec_}: {len(want)} row(s) hold")
    return found


def self_test(text, journey_text):
    ok = True
    table_row = re.search(r"^\| PREPARATION \| 3d 12h \|.*$", text, re.M)
    plants = [
        ("a value the history does not give", text.replace(table_row.group(0), table_row.group(0).replace("3d 12h", "3d 11h"), 1)),
        ("a transition from a state it does not leave",
         text.replace("object: D03, transition: revoke }", "object: D03, transition: complete_sale }", 1)),
        ("a refusal on a clause that holds",
         text.replace("{ verdict: unsatisfied, clause: not_internal, remedy: unreachable_from_here }",
                      "{ verdict: unsatisfied, clause: filled, remedy: dependent }", 1)),
        ("an agent the store does not hold", text.replace("actor: A-SCOUT", "actor: A-NOBODY", 1)),
        ("a published dimension three hops long",
         text.replace("configuration: i.object.units.model", "configuration: i.object.units.model.units", 1)),
        ("a published metric reading a member its rows lack", text.replace("    state: i.state\n", "    state: i.stat\n", 1)),
    ]
    for name, planted in plants:
        assert planted != text, name
        caught = check(planted, journey_text, verbose=False)
        ok &= bool(caught)
        print(f"  planted {name}: {'caught, ' + caught[0].splitlines()[0][:110] if caught else 'MISSED'}")
    return ok


def main():
    text, journey = DOC.read_text(), JOURNEY.read_text()
    found = check(text, journey)
    for f in found:
        print(f"  {DOC.relative_to(ROOT)}: {f}")
    reads = len(expected_tables(text))
    print(f"metric scenarios: {reads} read(s) against one history: " + ("consistent" if not found else f"{len(found)} finding(s)"))
    ok = self_test(text, journey) if "--no-self-test" not in sys.argv else True
    return 1 if found or not ok else 0


if __name__ == "__main__":
    sys.exit(main())

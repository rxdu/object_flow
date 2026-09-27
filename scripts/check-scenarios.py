#!/usr/bin/env python3
"""Recompute the values docs/design/metric-scenarios.md expects, from its history.

The scenarios fix a flow, the robot inventory of unit-journey.md, a history of
requests over it, and the values its reads return. This:

  1. replays the history against the modules: every request names an actor the
     store holds, whose kind is its type's; every transition is one its type
     declares, requestable, of the request's kind and taken from a state it
     leaves; every recording names a kind its subject declares; every refusal
     names the first check that fails, a generated guard, a declared guard or a
     call's, with its remedy, or a verdict with the remedy DESIGN.md §5.5 fixes;
     and every guard, effect and invariant the history reaches is evaluated
     from a transcription this script holds, each checked against the module's
     text, so an applied request passes and a refused one fails as it says;
  2. applies each published version to the module, at its time, and runs the
     flow checker over it against the version before;
  3. computes each read the document quotes, after the history above it, by
     declaration-syntax.md §6.9's rules, and compares it with the table.

It is not an engine: it evaluates only what it transcribes, and refuses a
history that reaches anything else. Its self-test, which runs after the check
unless `--no-self-test` is given, plants mistakes and shows each caught.
"""
import copy
import importlib.util
import math
from decimal import Decimal
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
    """The history is not one the modules allow."""


class Fails(Exception):
    """A request fails: its verdict, clause, remedy and the object a refused call targeted."""
    def __init__(self, verdict, clause=None, remedy=None, call=None):
        super().__init__(verdict, clause)
        self.v = {"verdict": verdict, "clause": clause, "remedy": remedy, "call": call}


def when(s):
    return datetime.strptime(str(s), "%Y-%m-%dT%H:%MZ")


def value(v):
    """A request's or an import's value: a timestamp written as one is read as one, whatever its name."""
    return when(v) if isinstance(v, str) and re.fullmatch(r"\d{4}-\d\d-\d\dT\d\d:\d\dZ", v) else v


def span(s):
    n, unit = str(s).split()
    return timedelta(**{unit if unit.endswith("s") else unit + "s": int(n)})


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
# Each guard, invariant, effect and declared metric below is the module's text
# and a Python reading of it; `transcribed()` refuses to run if the module's text
# differs, in every version the history reaches.
def all_checked(s, o, i):
    ok = True
    results = s.current(o.id, "pdi_results")
    s.read(results)
    for u in sorted(s.units(o.id)):
        checks = [c for c in s.objects.values() if c.type == "PdiCheck" and c.attrs["model"] == s.objects[u].attrs["model"]
                  and c.state == "ACTIVE"]
        s.read(checks)
        for c in checks:
            if not any(r.fields["unit"] == u and r.fields["check"] == c.id for r in results):
                ok = False
    return ok


GUARDS = {
    ("Delivery", "not_internal"): ("not internal", lambda s, o, i: not o.attrs.get("internal", False)),
    ("Delivery", "is_internal"): ("internal", lambda s, o, i: bool(o.attrs.get("internal", False))),
    ("Delivery", "filled"): ("count(u in units) >= 1 and none(u in pegged)",
                             lambda s, o, i: len(s.units(o.id)) >= 1 and not s.pegged(o.id)),
    ("Delivery", "all_checked"): ("all(u in units: all(c in PdiCheck where c.model == u.model and c.state == PdiCheck.ACTIVE: "
                                  "any(r in pdi_results where r.unit == u and r.check == c)))", all_checked),
    ("Robot", "open"): ("inputs.slot.state == Delivery.PREPARATION",
                        lambda s, o, i: s.objects[i["slot"]].state == "PREPARATION"),
    ("Robot", "labelled"): ("label_printed_at is not null", lambda s, o, i: o.attrs.get("label_printed_at") is not None),
    ("Robot", "mfr_serial"): ("model.manufacturer_serial_required implies manufacturer_serial is not null",
                              lambda s, o, i: not s.objects[o.attrs["model"]].attrs.get("manufacturer_serial_required", False)
                              or o.attrs.get("manufacturer_serial") is not None),
    ("Robot", "photo"): ("model.label_photo_required implies count(p in photos) >= 1",
                         lambda s, o, i: not s.objects[o.attrs["model"]].attrs.get("label_photo_required", False)
                         or len(o.attrs.get("photos", [])) >= 1),
    ("Shipment", "ordered"): ("all(u in inputs.with_units: u.state == Robot.REQUESTED)",
                              lambda s, o, i: all(s.objects[u].state == "REQUESTED" for u in i["with_units"])),
    ("Shipment", "ours"): ("inputs.robot.shipment == this", lambda s, o, i: s.objects[i["robot"]].attrs.get("shipment") == o.id),
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
    """A foreach step calling `transition` on each element of a set end."""
    def run(s, o, i, occ):
        items = s.units(o.id) if array == "units" else s.pegged(o.id)
        for u in sorted(items):
            if where is None or s.objects[u].state == where:
                s.call(u, "Robot", transition, {k: v(s, o) for k, v in (inputs or {}).items()}, occ)
    return run


EFFECTS = {
    # (type, transition): (the module's effect, as loaded; what it does)
    ("Delivery", "peg_slot"): ([{"call": {"target": "inputs.robot", "transition": "peg_to", "inputs": {"slot": "this"}}}],
                               lambda s, o, i, t: s.call(i["robot"], "Robot", "peg_to", {"slot": o.id}, t)),
    ("Delivery", "bind_slot"): ([{"call": {"target": "inputs.robot", "transition": "reserve", "inputs": {"slot": "this"}}}],
                                lambda s, o, i, t: s.call(i["robot"], "Robot", "reserve", {"slot": o.id}, t)),
    ("Delivery", "complete_sale"): ([{"foreach": {"item": "u", "array": "units", "limit": 500, "steps": [
        {"call": {"target": "u", "transition": "sell", "inputs": {"buyer": "customer"}}}]}}],
        call_each("units", "sell", {"buyer": lambda s, o: o.attrs["customer"]})),
    ("Delivery", "complete_internal"): ([{"foreach": {"item": "u", "array": "units", "limit": 500, "steps": [
        {"call": {"target": "u", "transition": "deliver_internal"}}]}}], call_each("units", "deliver_internal")),
    ("Delivery", "revoke"): ([{"foreach": {"item": "u", "array": "units", "where": "u.state == Robot.SOLD", "limit": 500, "steps": [
        {"call": {"target": "u", "transition": "unsell"}}]}},
        {"foreach": {"item": "u", "array": "units", "where": "u.state == Robot.DEVELOPMENT", "limit": 500, "steps": [
            {"call": {"target": "u", "transition": "recall_internal"}}]}}],
        lambda s, o, i, t: (call_each("units", "unsell", where="SOLD")(s, o, i, t),
                            call_each("units", "recall_internal", where="DEVELOPMENT")(s, o, i, t))),
    ("Shipment", "dispatch"): ([{"foreach": {"item": "u", "array": "inputs.with_units", "limit": 200, "steps": [
        {"call": {"target": "u", "transition": "ship", "inputs": {"via_shipment": "this"}}}]}}],
        lambda s, o, i, t: [s.call(u, "Robot", "ship", {"via_shipment": o.id}, t) for u in sorted(i["with_units"])]),
    ("Shipment", "add_unit"): ([{"call": {"target": "inputs.robot", "transition": "ship", "inputs": {"via_shipment": "this"}}}],
                               lambda s, o, i, t: s.call(i["robot"], "Robot", "ship", {"via_shipment": o.id}, t)),
    ("Shipment", "receive_unit"): ([{"call": {"target": "inputs.robot", "transition": "receive"}}],
                                   lambda s, o, i, t: s.call(i["robot"], "Robot", "receive", {}, t)),
    ("Robot", "ship"): ([{"assign": {"location": "shipment", "expr": "inputs.via_shipment"}}],
                        lambda s, o, i, t: o.set("shipment", i["via_shipment"])),
    ("Robot", "peg_to"): ([{"assign": {"location": "peg", "expr": "inputs.slot"}}], lambda s, o, i, t: o.set("peg", i["slot"])),
    ("Robot", "reserve"): ([{"assign": {"location": "binding", "expr": "inputs.slot"}}], lambda s, o, i, t: o.set("binding", i["slot"])),
    ("Robot", "sell"): ([{"assign": {"location": "sold_to", "expr": "inputs.buyer"}}], lambda s, o, i, t: o.set("sold_to", i["buyer"])),
    ("Robot", "unsell"): ([{"clear": ["binding", "sold_to"]}], lambda s, o, i, t: o.clear("binding", "sold_to")),
    ("Robot", "record_label_print"): ([{"assign": {"location": "label_printed_at", "expr": "now"}}],
                                      lambda s, o, i, t: o.set("label_printed_at", t)),
    ("Robot", "inventorize"): ([{"clear": ["peg"]}], lambda s, o, i, t: o.clear("peg")),
    ("Robot", "correct_state"): ([{"clear": ["peg", "binding", "used_in"]}], lambda s, o, i, t: o.clear("peg", "binding", "used_in")),
}
NO_EFFECT = {("Service", "register"), ("Delivery", "open"), ("Robot", "add_opening_stock"), ("Robot", "add_to_intake"), ("Robot", "request"),
             ("Robot", "deliver_internal"), ("Robot", "receive"), ("Shipment", "arrive"), ("RobotModel", "add"),
             ("User", "add"), ("Agent", "issue"), ("PdiCheck", "add"), ("PdiCheck", "retire")}
FIXED_REMEDY = {"unavailable": "unreachable_from_here", "stale": "self_serviceable", "not found": "unreachable_from_here",
                "not requestable": "unreachable_from_here", "invalid input": "self_serviceable",
                "unknown transition": "self_serviceable"}
FIRST = "none(s in r.subject.pdi_results where s.unit == r.unit and s.check == r.check and s.recorded_at < r.recorded_at)"
DECLARED_METRICS = {
    ("Delivery", "delivery_wait"): {"source": "intervals", "item": "i", "dimensions": {
        "state": "i.state", "customer": "i.object.customer", "configuration": "i.object.units.model"},
        "expression": "median(i.duration)"},
    ("Delivery", "open_shortfall"): {"source": "availability_checks", "item": "r", "filter": "r.subject.open", "dimensions": {
        "model": "r.model", "week": "week(r.occurred_at)"}, "time_dimension": "r.occurred_at", "expression": "sum(r.shortfall)"},
    ("Robot", "supplier_lead_time"): {"source": "objects", "item": "o",
        "filter": "o.entered_at(REQUESTED) is not null and o.entered_at(INTAKE) is not null",
        "dimensions": {"supplier": "o.model.manufacturer", "month": "month(o.entered_at(INTAKE))"},
        "time_dimension": "o.entered_at(INTAKE)", "expression": "median(o.entered_at(INTAKE) - o.created_at)"},
    ("Robot", "supplier_lead_time_p80"): {"source": "objects", "item": "o",
        "filter": "o.entered_at(REQUESTED) is not null and o.entered_at(INTAKE) is not null",
        "dimensions": {"supplier": "o.model.manufacturer", "month": "month(o.entered_at(INTAKE))"},
        "time_dimension": "o.entered_at(INTAKE)", "expression": "percentile(0.8, o.entered_at(INTAKE) - o.created_at)"},
    ("Robot", "weak_battery_units"): {"source": "battery_days", "item": "b", "filter": "b.min_health < 80.0",
        "dimensions": {"model": "b.subject.model", "week": "week(b.occurred_at)"}, "expression": "count(distinct b.subject)"},
    ("Delivery", "first_results_inspected"): {"source": "pdi_results", "item": "r", "filter": FIRST,
        "dimensions": {"model": "r.unit.model", "month": "month(r.occurred_at)"}, "expression": "count(distinct r.unit)"},
    ("Delivery", "first_results_failed"): {"source": "pdi_results", "item": "r",
        "filter": "r.outcome == CheckOutcome.FAIL and " + FIRST,
        "dimensions": {"model": "r.unit.model", "month": "month(r.occurred_at)"}, "expression": "count(distinct r.unit)"},
    ("Delivery", "on_time_rate"): {"source": "transitions", "item": "t",
        "filter": "t.to_state == Delivery.DELIVERED and t.object.promised_date is not null",
        "dimensions": {"month": "month(t.occurred_at)", "customer": "t.object.customer"},
        "expression": "count(where t.occurred_at <= t.object.promised_date) * 1.000 / count()"},
    ("", "first_pass_yield"): {"input_metrics": {"inspected": "Delivery.first_results_inspected",
                                                 "failed": "Delivery.first_results_failed"},
                               "group_by": ["model", "month"], "expression": "(inspected - failed) * 1.000 / inspected"},
}


def literal(v):
    """A loaded structure with its expressions as the loader leaves them, for comparison."""
    if isinstance(v, dict):
        return {k: literal(x) for k, x in v.items()}
    if isinstance(v, list):
        return [literal(x) for x in v]
    return one(v) if isinstance(v, str) else v


class Obj:
    def __init__(self, oid, tn, state, attrs, at, kind):
        self.id, self.type, self.state, self.attrs = oid, tn, state, dict(attrs)
        self.created_at, self.recorded_from, self.state_source, self.undated = at, at, "observed", False
        self.intervals = []                         # [state, entered, left, entered_by_kind, legacy]
        self.open_interval(state, at, kind)

    def open_interval(self, state, at, kind):
        if self.intervals:
            self.intervals[-1][2] = at
        self.intervals.append([state, at, None, kind, False])

    def entered_at(self, state):
        """When the object last entered `state`, legacy intervals included, or None."""
        return max((iv[1] for iv in self.intervals if iv[0] == state), default=None)

    def set(self, k, v):
        self.attrs[k] = v

    def clear(self, *ks):
        for k in ks:
            self.attrs[k] = None


class Observation:
    def __init__(self, oid, kind, subject, coll, fields, occurred, recorded, corrects, actor_kind):
        self.id, self.type, self.subject, self.coll, self.fields = oid, kind, subject, coll, dict(fields)
        self.occurred, self.recorded, self.corrects, self.kind = occurred, recorded, corrects, actor_kind
        self.corrected_by = None


# ── publishing a version ────────────────────────────────────────────────────────
def insert(mapping, key, value, order):
    """`mapping` with `key` added in the declared order of its section, as step 3 requires."""
    if key in mapping or not order or key not in order:
        mapping[key] = value
        return
    items = list(mapping.items())
    pos = next((n for n, (k, _v) in enumerate(items) if k in order and order.index(k) > order.index(key)), len(items))
    items.insert(pos, (key, value))
    mapping.clear()
    mapping.update(items)


def patched(text, adds):
    """The module's text with each `<path>: <value>` of a publish added in its section's order."""
    doc = flows.load(text)
    for path, value in adds.items():
        keys, node = path.split("."), doc
        for depth, k in enumerate(keys):
            order = flows.MODULE_ORDER if depth == 0 else flows.TYPE_ORDER if depth == 2 and keys[0] == "types" else None
            if depth == len(keys) - 1:
                insert(node, k, copy.deepcopy(value), order)
            else:
                if k not in node:
                    insert(node, k, {}, order)
                node = node[k]
    return yaml.safe_dump(doc, sort_keys=False, allow_unicode=True, width=10_000)


class Store:
    def __init__(self, mods_text, publishes):
        self.text = dict(mods_text)
        self.publishes = sorted(publishes, key=lambda p: when(p["at"]))
        self.version, self.findings = 1, []
        self.objects, self.obs, self.events, self.attempts, self.reading = {}, {}, [], [], None
        self.now = self.port = self.at = self.kind = None
        self.load()

    def load(self):
        self.mods = {n: flows.load(t) for n, t in self.text.items()}
        self.types = {}
        for m in self.mods.values():
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
            cond = (self.types.get(tn, {}).get("conditions") or {}).get(c)
            if cond:
                assert one(cond["expression"]) == text, f"{tn}.{c} is now `{one(cond['expression'])}`; the transcription reads `{text}`"
        for (tn, c), (text, _f) in INVARIANTS.items():
            got = one(self.types[tn]["invariants"][c]["expression"])
            assert got == text, f"{tn}.{c} is now `{got}`; the transcription reads `{text}`"
        for tn in ("Robot", "RobotModel"):
            for c in (self.types[tn].get("invariants") or {}):
                assert (tn, c) in INVARIANTS, f"{tn}.{c} is not transcribed"
        for (tn, x), (effect, _f) in EFFECTS.items():
            got = literal(self.types[tn]["transitions"][x].get("effect"))
            assert got == literal(effect), f"{tn}.{x}'s effect is now {got}; the transcription reads {literal(effect)}"
        for tn, x in NO_EFFECT:
            if tn in self.types:
                assert not self.types[tn]["transitions"][x].get("effect"), f"{tn}.{x} now has an effect"
        for (tn, mn), want in DECLARED_METRICS.items():
            got = ((self.mods["inventory_journey"].get("metrics") or {}) if not tn
                   else (self.types[tn].get("metrics") or {})).get(mn)
            if got:
                got = {k: v for k, v in literal(got).items() if k != "description"}
                assert got == literal(want), f"{tn}.{mn} is now {got}; the transcription reads {literal(want)}"

    def publish(self, p):
        prev = self.text["inventory_journey"]
        new = patched(prev, p["add"])
        files = [("operations_shared.yaml", self.text["operations_shared"]), ("inventory_journey.yaml", new)]
        errs, _notes = flows.check(files, {"inventory_journey.yaml": prev})
        self.findings += [f"the version published at {p['at']}: {e[3]} {e[4]}" for e in errs if e[0] == "inventory_journey.yaml"]
        self.text["inventory_journey"] = new
        self.version += 1
        self.load()

    # derived ends and collections, read from the stored ones
    def units(self, d):
        return {r for r, o in self.objects.items() if o.type == "Robot" and o.attrs.get("binding") == d}

    def pegged(self, d):
        return {r for r, o in self.objects.items() if o.type == "Robot" and o.attrs.get("peg") == d}

    def current(self, subject, coll):
        return [o for o in self.obs.values() if o.subject == subject and o.coll == coll and o.corrected_by is None]

    def read(self, objs):
        if self.reading is not None:
            self.reading.update(o.id for o in objs)

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
        for g in (x.get("guards") or {}):
            if (tn, g) not in GUARDS:
                raise Refused(f"{tn}.{xn}'s guard {g} is not transcribed, so this history cannot reach it")
            if not GUARDS[(tn, g)][1](self, o, inputs):
                raise Fails("unsatisfied", g, self.types[tn]["conditions"][g]["remedy"])

    def check_invariants(self, o):
        for (tn, c), (_t, f) in INVARIANTS.items():
            if tn == o.type and not f(self, o):
                raise Refused(f"{o.id} would break {tn}.{c}")

    def call(self, oid, tn, xn, inputs, occ):
        o = self.objects[oid]
        x = self.types[tn]["transitions"][xn]
        try:
            if not self.leaves(tn, x, o.state):
                raise Fails("unavailable", None, "unreachable_from_here")
            # a called transition takes the request's occurred time, and checks only that it does not
            # precede the current interval of what it changes on its own object (flow-format.md §4.8)
            if x["kind"] == "external" and occ < o.intervals[-1][1]:
                raise Fails("unsatisfied", "occurred_within", "self_serviceable")
            self.guards(tn, xn, x, o, inputs)
        except Fails as f:
            f.v["call"] = oid
            raise
        self.apply(o, tn, xn, x, inputs, occ)

    def apply(self, o, tn, xn, x, inputs, occ, to=None):
        frm = o.state
        for a in (x.get("required_inputs") or []) + (x.get("optional_inputs") or []):
            if a in inputs:
                o.set(a, inputs[a])
        if (tn, xn) in EFFECTS:
            EFFECTS[(tn, xn)][1](self, o, inputs, occ)
        elif (tn, xn) not in NO_EFFECT and x.get("effect"):
            raise Refused(f"{tn}.{xn}'s effect is not transcribed")
        target = to or x.get("to")
        if x["kind"] in ("external", "assertion") and target:
            o.state = target
            o.state_source = "asserted" if x["kind"] == "assertion" else "observed"
            o.open_interval(target, occ, self.kind)
        self.check_invariants(o)
        self.events.append({"object": o.id, "type": tn, "transition": xn, "from": frm, "to": o.state, "kind": self.kind,
                            "occurred": occ, "recorded": self.at, "overrides": x["kind"] == "assertion",
                            "imported": False, "reason": inputs.get("reason") if x["kind"] == "assertion" else None,
                            "reads": set()})

    # ── one request ──
    def attempt(self, r):
        """Take the request on this store; raise Fails with its first failure."""
        at = self.at = when(r["at"])
        self.kind = self.kind_of(r["actor"])
        occ = when(r["occurred_at"]) if r.get("occurred_at") else at
        inputs = {k: value(v) for k, v in (r.get("inputs") or {}).items()}
        if r.get("record"):
            self.record(r, at, occ)
            return
        tn = r.get("create") or self.objects[r["object"]].type
        x = self.types[tn]["transitions"].get(r["transition"])
        if x is None:
            raise Refused(f"{tn} declares no transition {r['transition']}")
        if x.get("only_via"):
            raise Refused(f"{tn}.{r['transition']} is only via {x['only_via']}")
        if r.get("create"):
            if x["kind"] != "initial" or r["object"] in self.objects:
                raise Refused(f"{r['object']}: {tn}.{r['transition']} is not a creation of a new object")
            o = Obj(r["object"], tn, x["to"], {}, occ, self.kind)
            self.objects[o.id] = o
            self.guards(tn, r["transition"], x, o, inputs)
            self.apply(o, tn, r["transition"], {**x, "kind": "created"}, inputs, occ)
            return
        o = self.objects[r["object"]]
        if x["kind"] == "initial":
            raise Refused(f"{tn}.{r['transition']} is a creation, requested on {o.id}")
        if x["kind"] == "assertion":
            to = inputs.get("to")
            if to not in x["to"] or to == o.state or self.final(tn, o.state):
                raise Refused(f"{o.id}: {tn}.{r['transition']} to {to} from {o.state} is not an assertion it may make")
            self.apply(o, tn, r["transition"], x, inputs, occ, to=to)
            return
        if not self.leaves(tn, x, o.state):
            raise Fails("unavailable", None, "unreachable_from_here")
        if r.get("occurred_at"):
            limit = x.get("backdating_limit")
            changes = x["kind"] == "external"
            if limit is None or at - occ > span(limit) or occ > at or (changes and occ < o.intervals[-1][1]):
                raise Fails("unsatisfied", "occurred_within", "self_serviceable")
        self.reading = set()
        self.guards(tn, r["transition"], x, o, inputs)
        reads, self.reading = self.reading, None
        n = len(self.events)
        self.apply(o, tn, r["transition"], x, inputs, occ)
        for e in self.events[n:]:
            if e["object"] == o.id and e["transition"] == r["transition"]:
                e["reads"] = reads

    def record(self, r, at, occ):
        subj = self.objects[r["subject"]]
        spec_ = (self.types[subj.type].get("observations") or {}).get(r["record"])
        if spec_ is None:
            raise Refused(f"{subj.type} declares no observation {r['record']}")
        # the generated guards, in order: occurred_within, subject_open, corrects_current, subject_owned
        if r.get("occurred_at"):
            limit = spec_.get("max_recording_delay")
            if limit is None or at - occ > span(limit) or occ > at:
                raise Fails("unsatisfied", "occurred_within", "self_serviceable")
        fixed = r.get("corrects")
        if not fixed and self.final(subj.type, subj.state):
            raise Refused(f"{subj.id} is final; subject_open's remedy is not transcribed")
        if fixed:
            old = self.obs.get(fixed)
            if old is None or old.coll != r["record"] or old.subject != subj.id or old.corrected_by:
                raise Refused(f"{r['object']} corrects {fixed}, which corrects_current refuses")
        if self.types[subj.type].get("mirror"):
            raise Refused(f"{subj.id} is a mirror's; subject_owned refuses")
        fields = r.get("fields") or {}
        for a, s_ in (spec_.get("attributes") or {}).items():
            if not s_.get("optional") and a not in fields:
                raise Refused(f"{r['object']} leaves out {a}")
        unknown = set(fields) - set(spec_.get("attributes") or {})
        if unknown:
            raise Refused(f"{r['object']} has no field {sorted(unknown)}")
        o = Observation(r["object"], spec_["kind"], subj.id, r["record"], fields, occ, at, fixed, self.kind)
        self.obs[o.id] = o
        if fixed:
            self.obs[fixed].corrected_by = o.id

    def replay(self, block):
        self.now = when(block["now"])
        if block.get("port"):
            self.port = when(block["port"])
        for imp in block.get("imported") or []:
            at = when(imp["entered"]) if imp.get("entered") else self.port
            o = Obj(imp["object"], imp["type"], imp["state"], {k: value(v) for k, v in (imp.get("attributes") or {}).items()},
                    at, imp.get("entered_by", "unknown"))
            legacy = [[st, when(a), when(b), imp.get("entered_by", "unknown"), True] for st, a, b in imp.get("legacy") or []]
            o.intervals = legacy + o.intervals
            known = [iv[1] for iv in o.intervals] + ([self.port] if not imp.get("entered") else [])
            o.created_at = when(imp["created"]) if imp.get("created") else min(known)
            o.undated = not imp.get("created")
            o.state_source = "imported"
            silent = imp.get("silent")
            o.recorded_from = (when(silent[1]) if silent else o.created_at if imp.get("created") else self.port)
            self.objects[o.id] = o
            self.events.append({"object": o.id, "type": o.type, "transition": "import", "from": None, "to": o.state,
                                "kind": "unknown", "occurred": self.port, "recorded": self.port, "overrides": True,
                                "imported": True, "reason": None, "reads": set()})
        last = None
        for r in block.get("requests") or []:
            at = when(r["at"])
            assert last is None or at >= last, f"request at {r['at']} is out of order"
            last = at
            while self.publishes and when(self.publishes[0]["at"]) <= at:
                self.publish(self.publishes.pop(0))
            if r.get("refused"):
                self.refused(r, at)
                continue
            try:
                self.attempt(r)
            except Fails as f:
                raise Refused(f"the request at {r['at']} on {r.get('object') or r.get('subject')} fails with {f.v}, "
                              "and the history says it applies")
        while self.publishes and when(self.publishes[0]["at"]) <= self.now:
            self.publish(self.publishes.pop(0))

    def refused(self, r, at):
        want = {"verdict": r["refused"]["verdict"], "clause": r["refused"].get("clause"),
                "remedy": r["refused"]["remedy"], "call": r["refused"].get("call")}
        if want["verdict"] == "stale":
            # a moved version is the caller's to know, and the store's to detect; the verdict's remedy is fixed
            if FIXED_REMEDY["stale"] != want["remedy"]:
                raise Refused("a stale refusal carries self_serviceable")
        else:
            trial = copy.deepcopy(self)
            try:
                trial.attempt(r)
                raise Refused(f"the request at {r['at']} on {r.get('object') or r.get('subject')} applies, "
                              "and the history says it is refused")
            except Fails as f:
                if f.v != want:
                    raise Refused(f"the request at {r['at']} fails with {f.v}, and the history says {want}")
        if r.get("record"):
            tn = self.types[self.objects[r["subject"]].type]["observations"][r["record"]]["kind"]
        else:
            tn = r.get("create") or self.objects[r["object"]].type
        self.attempts.append({"object": None if r.get("record") or r.get("create") else r["object"], "type": tn,
                              "transition": "record" if r.get("record") else r["transition"],
                              "verdict": want["verdict"], "clause": want["clause"], "remedy": want["remedy"],
                              "kind": self.kind_of(r["actor"]), "at": at, "enforced": True})


# ── the reads, by declaration-syntax.md §6.9 ────────────────────────────────────
def duration(s, o, iv):
    """A span's exit less its entry; a current one to `now` while the object is open, and none on a finished object."""
    entered, left = iv[1], iv[2]
    if left is not None:
        return left - entered
    return s.now - entered if s.open(o) else None


def nearest_rank(values, p):
    vs = sorted(values)
    return vs[max(1, math.ceil(p * len(vs))) - 1] if vs else None


def week(t):
    y, w, _d = t.isocalendar()
    return f"{y}-W{w:02d}"


def group(rows, keep):
    """Rows of (dimensions, body, gap) grouped by the kept dimensions; a set-valued dimension's
    row is counted once under each distinct value (ADR-0123)."""
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


def row_filter(expr):
    """The one filter shape the document's reads use: `<item>[.<path>].<member> == <value>`."""
    if not expr:
        return lambda dims: True
    m = re.fullmatch(r'\w+(?:\.\w+)*\.(\w+) == (?:[A-Z]\w*\.(?=[A-Z_]+\b))?"?([\w-]+)"?', expr)
    member, value = m.group(1), m.group(2)
    return lambda dims: dims.get(member) == value


def rows_of(groups, keep, agg):
    return [{**dict(zip(keep, k)), "value": agg([b for b in g["bodies"] if b is not None]), "gaps": g["gaps"]}
            for k, g in groups.items()]


def first_results(s):
    """The pre-delivery results nothing has corrected and nothing recorded earlier for the same unit and check."""
    cur = [o for o in s.obs.values() if o.coll == "pdi_results" and o.corrected_by is None]
    return [r for r in cur if not any(x.subject == r.subject and x.fields["unit"] == r.fields["unit"]
                                      and x.fields["check"] == r.fields["check"] and x.recorded < r.recorded for x in cur)]


def distinct_units(s, results):
    out = {}
    for r in results:
        out.setdefault((s.objects[r.fields["unit"]].attrs["model"], r.occurred.strftime("%Y-%m")), set()).add(r.fields["unit"])
    return out


def rate(num, den):
    return (Decimal(num) * Decimal("1.000") / Decimal(den)).quantize(Decimal("0.001")) if den else None


def metric(s, name, keep, filt, over=None):
    tn, _, std = name.partition(".")
    keepf = row_filter(filt)
    if name in ("supplier_lead_time", "supplier_lead_time_p80"):
        rows = []
        for o in s.objects.values():
            got, ordered = o.entered_at("INTAKE"), o.entered_at("REQUESTED")
            if o.type != "Robot" or got is None or ordered is None or (over and got < s.now - over):
                continue
            gap = o.id if o.undated or got < o.recorded_from or ordered < o.recorded_from else None
            rows.append(({"supplier": s.objects[o.attrs["model"]].attrs.get("manufacturer"), "month": got.strftime("%Y-%m")},
                         got - o.created_at, gap))
        return rows_of(group(rows, keep), keep, lambda b: nearest_rank(b, 0.5 if name == "supplier_lead_time" else 0.8))
    if name == "weak_battery_units":
        rows = [({"model": s.objects[b.subject].attrs["model"], "week": week(b.occurred)}, b.subject, None)
                for b in s.obs.values() if b.coll == "battery_days" and b.corrected_by is None and b.fields["min_health"] < 80.0]
        return rows_of(group(rows, keep), keep, lambda b: len(set(b)))
    if name == "first_pass_yield":
        assert keep == ["model", "month"], "first_pass_yield is read by the dimensions it names"
        first = first_results(s)
        ins = distinct_units(s, first)
        fail = distinct_units(s, [r for r in first if r.fields["outcome"] == "FAIL"])
        return [{"model": k[0], "month": k[1], "value": rate(len(ins.get(k, ())) - len(fail.get(k, ())), len(ins.get(k, ()))),
                 "gaps": set()} for k in set(ins) | set(fail)]
    if name == "on_time_rate":
        rows = []
        for e in s.events:
            o = s.objects[e["object"]]
            if e["type"] == "Delivery" and e["to"] == "DELIVERED" and e["from"] != "DELIVERED" and o.attrs.get("promised_date"):
                dims = {"month": e["occurred"].strftime("%Y-%m"), "customer": o.attrs.get("customer")}
                if keepf(dims):
                    rows.append((dims, e["occurred"] <= o.attrs["promised_date"], None))
        return rows_of(group(rows, keep), keep, lambda b: rate(sum(b), len(b)))
    if std == "time_in_state" or name == "delivery_wait":
        tn = "Delivery" if name == "delivery_wait" else tn
        rows = []
        for o in s.objects.values():
            if o.type != tn:
                continue
            for iv in o.intervals:
                dims = {"state": iv[0], "month": iv[1].strftime("%Y-%m"), "actor_kind": iv[3]}
                if name == "delivery_wait":
                    dims.update(customer=o.attrs.get("customer"),
                                configuration=frozenset(s.objects[u].attrs["model"] for u in s.units(o.id)))
                if keepf(dims):
                    rows.append((dims, duration(s, o, iv), o.id if iv[1] < o.recorded_from else None))
        return rows_of(group(rows, keep), keep, lambda b: nearest_rank(b, 0.5))
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
        return rows_of(group(rows, keep), keep, (lambda b: max(b) if b else None) if std == "oldest_open" else len)
    if std == "refusals":
        counts = {}
        for a in s.attempts:
            if a["type"] == tn and a["enforced"]:
                key = (a["at"].date(), a["object"], a["transition"], a["verdict"], a["clause"], a["remedy"], a["kind"])
                counts[key] = counts.get(key, 0) + 1
        rows = []
        for k, n in counts.items():
            dims = {"transition": k[2], "clause": k[4], "remedy": k[5], "actor_kind": k[6],
                    "week": week(datetime(k[0].year, k[0].month, k[0].day))}
            if keepf(dims):
                rows.append((dims, n, None))
        return rows_of(group(rows, keep), keep, sum)
    if std == "override_counts":
        rows = [({"state": e["to"], "reason": e["reason"], "week": week(e["occurred"]), "actor_kind": e["kind"]}, 1, None)
                for e in s.events if e["type"] == tn and e["overrides"] and not e["imported"]]
        return rows_of(group(rows, keep), keep, len)
    if name == "open_shortfall":
        rows = [({"model": o.fields["model"], "week": week(o.occurred)}, o.fields["shortfall"], None)
                for o in s.obs.values() if o.coll == "availability_checks" and o.corrected_by is None
                and s.open(s.objects[o.subject])]
        return rows_of(group(rows, keep), keep, sum)
    raise SystemExit(f"no metric {name} in this checker")


def read(s, spec_):
    reader = re.search(r"\) as ([\w-]+)$", spec_)
    if reader:
        # no read is filtered by who is asking (PRD T5): the reader must be one the store holds, and changes nothing
        s.kind_of(reader.group(1))
        spec_ = spec_[:reader.start() + 1]
    m = re.fullmatch(r'metric\((\w+(?:\.\w+)?), keep: \[([\w, ]*)\](?:, filter: "(.+?)")?(?:, over: "(\d+ days)")?\)', spec_)
    if m:
        keep = [k.strip() for k in m.group(2).split(",") if k.strip()]
        return (metric(s, m.group(1), keep, (m.group(3) or "").replace('\\"', '"'), span(m.group(4)) if m.group(4) else None),
                keep + ["value", "gaps"])
    m = re.fullmatch(r"recorded\((\w+)\.(\w+)\)", spec_)
    if m:
        got = sorted((o for o in s.obs.values() if o.subject == m.group(1) and o.coll == m.group(2) and o.corrected_by is None),
                     key=lambda o: o.occurred)
        return [{"object": o.id, "occurred_at": o.occurred, "min_health": o.fields["min_health"], "samples": o.fields["samples"]}
                for o in got], ["object", "occurred_at", "min_health", "samples"]
    if spec_ == "exceptions(Robot)":
        return [{"object": o.id, "state": o.state} for o in s.objects.values()
                if o.type == "Robot" and o.state_source == "asserted"], ["object", "state"]
    if spec_ == 'query(Delivery, filter: "state.category != closed", order: "entered_at(state)")':
        rows = [(o.intervals[-1][1], o) for o in s.objects.values() if o.type == "Delivery" and s.open(o)]
        return [{"object": o.id, "state": o.state, "entered": t, "gaps": {o.id} if t < o.recorded_from else set()}
                for t, o in sorted(rows, key=lambda r: r[0])], ["object", "state", "entered", "gaps"]
    m = re.fullmatch(r"read set of (\w+)\.(\w+): (.+)", spec_)
    if m:
        kinds = {k.strip() for k in m.group(3).split(",")}
        ev = [e for e in s.events if e["object"] == m.group(1) and e["transition"] == m.group(2)][-1]
        out = []
        for oid in ev["reads"]:
            t = s.objects[oid].type if oid in s.objects else s.obs[oid].type
            if t in kinds:
                out.append({"object": oid, "kind": t})
        return out, ["object", "kind"]
    m = re.fullmatch(r"events\((\w+)\)", spec_)
    if m:
        return [{"transition": e["transition"], "occurred_at": e["occurred"], "recorded_at": e["recorded"]}
                for e in s.events if e["object"] == m.group(1)], ["transition", "occurred_at", "recorded_at"]
    raise SystemExit(f"no read of that form: {spec_}")


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
    if isinstance(v, bool):
        return str(v).lower()
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


ORDERED = ("query(", "events(", "recorded(")


def check(text, journey_text, verbose=True):
    """Findings, as strings; empty when the document's history and every value it quotes hold."""
    found = []
    blocks = fences(text)
    mods_text = modules_of(journey_text)
    mods_text.update(modules_of(text))
    publishes = [yaml.safe_load(b) for i, _s, b in blocks if i == "yaml publish"]
    histories = [(start, yaml.safe_load(b)) for i, start, b in blocks if i == "yaml scenario"]
    reads = expected_tables(text)
    try:
        s = Store(mods_text, publishes)
    except AssertionError as e:
        return [f"the transcription: {e}"]
    steps = sorted([(start, "history", h) for start, h in histories]
                   + [(line, "read", (line, sp, hd, rw)) for line, sp, hd, rw in reads], key=lambda x: x[0])
    for _line, what, item in steps:
        if what == "history":
            try:
                s.replay(item)
            except Refused as e:
                return found + s.findings + [f"the history: {e}"]
            except AssertionError as e:
                return found + s.findings + [f"the transcription: {e}"]
            continue
        line, spec_, header, rows = item
        got, cols = read(s, spec_)
        if header != cols:
            found.append(f"line {line}: `{spec_}` has columns {header}, and the read returns {cols}")
            continue
        want = [tuple(r) for r in rows]
        have = [tuple(show(g[c]) for c in cols) for g in got]
        ordered = spec_.startswith(ORDERED)
        if (want != have) if ordered else (sorted(want) != sorted(have)):
            found.append(f"line {line}: `{spec_}` expects\n      {want if ordered else sorted(want)}\n"
                         f"    and the history gives\n      {have if ordered else sorted(have)}")
        elif verbose:
            print(f"  {spec_}: {len(want)} row(s) hold")
    return found + s.findings


PLANTS = [
    ("a value the history does not give", r"^\| PREPARATION \| 3d 12h \|.*$", lambda m: m.group(0).replace("3d 12h", "3d 11h")),
    ("a transition from a state it does not leave", "object: D03, transition: revoke }", "object: D03, transition: complete_sale }"),
    ("a refusal on a clause that holds", "{ verdict: unsatisfied, clause: not_internal, remedy: unreachable_from_here }",
     "{ verdict: unsatisfied, clause: filled, remedy: dependent }"),
    ("an agent the store does not hold", "actor: A-SCOUT", "actor: A-NOBODY"),
    ("a published dimension three hops long", "configuration: i.object.units.model", "configuration: i.object.units.model.units"),
    ("a published metric reading a member its rows lack", "      state: i.state\n", "      state: i.stat\n"),
    ("a completion before its checks have results",
     "actor: A-SCOUT, object: D09, transition: complete_sale, refused: { verdict: unsatisfied, clause: all_checked, remedy: unreachable_from_here } }",
     "actor: A-SCOUT, object: D09, transition: complete_sale }"),
    ("a receipt backdated past its bound, said to apply",
     "inputs: { robot: R32 }, occurred_at: 2026-10-05T16:00Z, refused: { verdict: unsatisfied, clause: occurred_within, remedy: self_serviceable } }",
     "inputs: { robot: R32 }, occurred_at: 2026-10-05T16:00Z }"),
    ("a correction left out, so the gate reads the mistake", "outcome: PASS }, corrects: P2 }", "outcome: PASS } }"),
    ("a legacy unit given the order date its mapping lacked", "{ object: L05, type: Robot, state: AVAILABLE, entered:",
     "{ object: L05, type: Robot, state: AVAILABLE, created: 2026-04-06T00:00Z, entered:"),
    ("a summary past its bound, said to apply",
     "fields: { min_health: 90.0, mean_health: 93.0, samples: 86400 }, refused: { verdict: unsatisfied, clause: occurred_within, remedy: self_serviceable } }",
     "fields: { min_health: 90.0, mean_health: 93.0, samples: 86400 } }"),
    ("a first result recorded as a pass", "object: P3, fields: { unit: R02, check: K1, outcome: FAIL", "object: P3, fields: { unit: R02, check: K1, outcome: PASS"),
]


def self_test(text, journey_text):
    ok = True
    for name, old, new in PLANTS:
        if callable(new):
            planted = re.sub(old, new, text, count=1, flags=re.M)
        else:
            assert text.count(old) >= 1, name
            planted = text.replace(old, new, 1)
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
    print(f"metric scenarios: {len(expected_tables(text))} read(s) against the history: "
          + ("consistent" if not found else f"{len(found)} finding(s)"))
    ok = self_test(text, journey) if "--no-self-test" not in sys.argv else True
    return 1 if found or not ok else 0


if __name__ == "__main__":
    sys.exit(main())

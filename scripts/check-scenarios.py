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
    if isinstance(v, str) and re.fullmatch(r"\d{4}-\d\d-\d\dT\d\d:\d\dZ", v):
        return when(v)
    if isinstance(v, str) and re.fullmatch(r"\d+ (days?|hours?|minutes?)", v):
        return span(v)
    return v


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


def all_passed(s, o, i):
    """Every active check of every unit bound has a result that is not a failure."""
    results = s.current(o.id, "pdi_results")
    for u in sorted(s.units(o.id)):
        for c in s.objects.values():
            if c.type == "PdiCheck" and c.attrs["model"] == s.objects[u].attrs["model"] and c.state == "ACTIVE":
                if not any(r.fields["unit"] == u and r.fields["check"] == c.id and r.fields["outcome"] != "FAIL"
                           for r in results):
                    return False
    return True


def first_pass_rate(s, model, until, over):
    """Delivery.first_pass_rate bound to `model := <model>, over last <over>`, as consulted at `until`."""
    rows = [r for r in first_results(s) if s.objects[r.fields["unit"]].attrs["model"] == model
            and until - over <= r.occurred <= until]
    return rate(sum(r.fields["outcome"] != "FAIL" for r in rows), len(rows))


def yield_or_signed(s, o, i):
    """`count(s in second_sign_offs) >= 1 or all(u in units: u.model.yield_floor is null or
    metric(first_pass_rate, model := u.model, over last 30 days) >= u.model.yield_floor)`. Every metric
    reference is consulted before the transaction, whichever side decides (declaration-syntax.md §6.9)."""
    signed = len(s.current(o.id, "second_sign_offs")) >= 1
    ok = True
    for u in sorted(s.units(o.id)):
        model = s.objects[u].attrs["model"]
        value = first_pass_rate(s, model, s.at, timedelta(days=30))
        s.consulted[f"first_pass_rate(model: {model})"] = value
        floor = s.objects[model].attrs.get("yield_floor")
        s.read([s.objects[model]])
        if floor is not None:
            s.floors[f"{model}.yield_floor"] = Decimal(str(floor)).quantize(Decimal("0.001"))
        if floor is not None and (value is None or value < Decimal(str(floor))):
            ok = False
    return signed or ok


def returns_by_model(s, model, until, over):
    """Return.returns_by_model bound to `model := <model>, over last <over>`: the returns received in the window."""
    return sum(1 for o in s.objects.values() if o.type == "Return" and s.objects[o.attrs["unit"]].attrs["model"] == model
               and until - over <= o.created_at <= until)


def second_eye(s, o, i):
    model = s.objects[o.attrs["unit"]].attrs["model"]
    n = returns_by_model(s, model, s.at, timedelta(days=30))
    s.consulted[f"returns_by_model(model: {model})"] = n
    return i.get("outcome") != "REPLACED" or o.attrs.get("approved_by") is not None or n < 5


DERIVED = {
    ("RobotModel", "low_stock"): ("available_units < reorder_point", lambda s, o: sum(
        1 for u in s.objects.values() if u.type == "Robot" and u.attrs.get("model") == o.id and u.state == "AVAILABLE")
        < o.attrs.get("reorder_point", 0)),
    ("Shipment", "overdue"): ("state == IN_TRANSIT and eta < now",
                              lambda s, o: o.state == "IN_TRANSIT" and o.attrs.get("eta") is not None and o.attrs["eta"] < s.now),
    ("Engagement", "overdue"): ("state == OUT and expected_return < now",
                                lambda s, o: o.state == "OUT" and o.attrs.get("expected_return") is not None
                                and o.attrs["expected_return"] < s.now),
    ("Robot", "backorder_ageing"): ("peg is not null and (state == REQUESTED or state == PROCUREMENT) and "
                                    "model.usual_lead_time is not null and created_at + model.usual_lead_time < now",
                                    lambda s, o: o.attrs.get("peg") is not None and o.state in ("REQUESTED", "PROCUREMENT")
                                    and s.objects[o.attrs["model"]].attrs.get("usual_lead_time") is not None
                                    and o.created_at + s.objects[o.attrs["model"]].attrs["usual_lead_time"] < s.now),
    ("Delivery", "date_at_risk"): ("state == PREPARATION and promised_date is not null and promised_date < now + 2 days",
                                   lambda s, o: o.state == "PREPARATION" and o.attrs.get("promised_date") is not None
                                   and o.attrs["promised_date"] < s.now + timedelta(days=2)),
    ("Warranty", "expiring"): ("state == ACTIVE and ends_at < now + 30 days",
                               lambda s, o: o.state == "ACTIVE" and o.attrs.get("ends_at") is not None
                               and o.attrs["ends_at"] < s.now + timedelta(days=30)),
    ("Robot", "awaiting_assignment"): ("state == AVAILABLE and procured_for is not null",
                                       lambda s, o: o.state == "AVAILABLE" and o.attrs.get("procured_for") is not None),
    ("Robot", "missing_a_week"): ("state == MISSING and entered_at(MISSING) + 7 days <= now",
                                  lambda s, o: o.state == "MISSING" and o.entered_at("MISSING") + timedelta(days=7) <= s.now),
}

GUARDS = {
    ("Delivery", "not_internal"): ("not internal", lambda s, o, i: not o.attrs.get("internal", False)),
    ("Delivery", "is_internal"): ("internal", lambda s, o, i: bool(o.attrs.get("internal", False))),
    ("Delivery", "filled"): ("count(u in units) >= 1 and none(u in pegged)",
                             lambda s, o, i: len(s.units(o.id)) >= 1 and not s.pegged(o.id)),
    ("Delivery", "all_checked"): ("all(u in units: all(c in PdiCheck where c.model == u.model and c.state == PdiCheck.ACTIVE: "
                                  "any(r in pdi_results where r.unit == u and r.check == c)))", all_checked),
    ("Delivery", "all_passed"): ("all(u in units: all(c in PdiCheck where c.model == u.model and c.state == PdiCheck.ACTIVE: "
                                 "any(r in pdi_results where r.unit == u and r.check == c and r.outcome != CheckOutcome.FAIL)))",
                                 all_passed),
    ("Delivery", "ours"): ("inputs.robot.binding == this", lambda s, o, i: s.objects[i["robot"]].attrs.get("binding") == o.id),
    ("ExpenseClaim", "submitted"): ("submitted_at is not null", lambda s, o, i: o.attrs.get("submitted_at") is not None),
    ("ExpenseClaim", "within_policy"): ("inputs.amount <= inputs.category.policy_amount",
                                        lambda s, o, i: Decimal(str(i["amount"]))
                                        <= Decimal(str(s.objects[i["category"]].attrs["policy_amount"]))),
    ("Robot", "open"): ("inputs.slot.state == Delivery.PREPARATION",
                        lambda s, o, i: s.objects[i["slot"]].state == "PREPARATION"),
    ("Robot", "labelled"): ("label_printed_at is not null", lambda s, o, i: o.attrs.get("label_printed_at") is not None),
    ("Robot", "mfr_serial"): ("model.manufacturer_serial_required implies manufacturer_serial is not null",
                              lambda s, o, i: not s.objects[o.attrs["model"]].attrs.get("manufacturer_serial_required", False)
                              or o.attrs.get("manufacturer_serial") is not None),
    ("Robot", "photo"): ("model.label_photo_required implies count(p in photos) >= 1",
                         lambda s, o, i: not s.objects[o.attrs["model"]].attrs.get("label_photo_required", False)
                         or len(o.attrs.get("photos", [])) >= 1),
    ("Delivery", "yield_or_signed"): ("count(s in second_sign_offs) >= 1 or all(u in units: u.model.yield_floor is null or "
                                      "metric(first_pass_rate, model := u.model, over last 30 days) >= u.model.yield_floor)",
                                      yield_or_signed),
    ("Robot", "rover_photo"): ('model.name != "Rover 2" or count(p in photos) >= 1',
                               lambda s, o, i: s.objects[o.attrs["model"]].attrs.get("name") != "Rover 2"
                               or len(o.attrs.get("photos") or []) >= 1),
    ("Return", "second_eye"): ("inputs.outcome != ReturnOutcome.REPLACED or approved_by is not null or "
                               "metric(returns_by_model, model := unit.model, over last 30 days) < 5", second_eye),
    ("ServiceJob", "active"): ("inputs.engineer.state == User.ACTIVE", lambda s, o, i: s.objects[i["engineer"]].state == "ACTIVE"),
    ("ServiceJob", "delivered"): ("inputs.robot.state == Robot.SOLD or inputs.robot.state == Robot.DEVELOPMENT",
                                  lambda s, o, i: s.objects[i["robot"]].state in ("SOLD", "DEVELOPMENT")),
    ("ServiceJob", "theirs"): ("inputs.robot.state == Robot.DEVELOPMENT or inputs.robot.sold_to == inputs.customer",
                               lambda s, o, i: s.objects[i["robot"]].state == "DEVELOPMENT"
                               or s.objects[i["robot"]].attrs.get("sold_to") == i["customer"]),
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
    ("ServiceJob", "engineer_active"): ("not open or engineer.state == User.ACTIVE",
                                        lambda s, o: not s.open(o) or s.objects[o.attrs["engineer"]].state == "ACTIVE"),
}


def call_each(array, transition, inputs=None, where=None):
    """A foreach step calling `transition` on each element of a set end."""
    def run(s, o, i, occ):
        items = (s.units(o.id) if array == "units" else s.pegged(o.id) if array == "pegged"
                 else {r for r, x in s.objects.items() if x.type == "Robot" and x.attrs.get("used_in") == o.id})
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
    ("ServiceJob", "finish"): ([{"foreach": {"item": "p", "array": "parts_used", "limit": 50, "steps": [
        {"call": {"target": "p", "transition": "consume", "inputs": {"buyer": "customer"}}}]}}],
        call_each("parts_used", "consume", {"buyer": lambda s, o: o.attrs["customer"]})),
    ("Shipment", "dispatch"): ([{"foreach": {"item": "u", "array": "inputs.with_units", "limit": 200, "steps": [
        {"call": {"target": "u", "transition": "ship", "inputs": {"via_shipment": "this"}}}]}}],
        lambda s, o, i, t: [s.call(u, "Robot", "ship", {"via_shipment": o.id}, t) for u in sorted(i["with_units"])]),
    ("Shipment", "add_unit"): ([{"call": {"target": "inputs.robot", "transition": "ship", "inputs": {"via_shipment": "this"}}}],
                               lambda s, o, i, t: s.call(i["robot"], "Robot", "ship", {"via_shipment": o.id}, t)),
    ("Shipment", "receive_unit"): ([{"call": {"target": "inputs.robot", "transition": "receive"}}],
                                   lambda s, o, i, t: s.call(i["robot"], "Robot", "receive", {}, t)),
    ("Robot", "add_photo"): ([{"add": {"location": "photos", "expr": "inputs.photo"}}],
                             lambda s, o, i, t: o.set("photos", (o.attrs.get("photos") or []) + [i["photo"]])),
    ("Return", "receive"): ([{"assign": {"location": "handled_by", "expr": "actor.id"}}],
                            lambda s, o, i, t: o.set("handled_by", s.identity(s.actor))),
    ("Return", "resolve"): ([{"assign": {"location": "outcome", "expr": "inputs.outcome"}}],
                            lambda s, o, i, t: o.set("outcome", i["outcome"])),
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


def sell_and_warrant(s, o, i, t):
    """complete_sale from version 15: each unit bound is sold and gains its warranty, in ascending id order."""
    for u in sorted(s.units(o.id)):
        s.call(u, "Robot", "sell", {"buyer": o.attrs["customer"]}, t)
        s.create("Warranty", "start", {"unit": u, "months": s.objects[s.objects[u].attrs["model"]].attrs["warranty_months"]}, t)


# a later version may change an effect, so each transition holds every version's effect, and the one in force runs
EFFECTS = {k: [v] for k, v in EFFECTS.items()}
for _key, _alt in {
    ("Robot", "inventorize"): ([{"assign": {"location": "procured_for", "expr": "peg"}}, {"clear": ["peg"]}],
                               lambda s, o, i, t: (o.set("procured_for", o.attrs.get("peg")), o.clear("peg"))),
    ("Robot", "reserve"): ([{"assign": {"location": "binding", "expr": "inputs.slot"}}, {"clear": ["procured_for"]}],
                           lambda s, o, i, t: (o.set("binding", i["slot"]), o.clear("procured_for"))),
    ("Delivery", "complete_sale"): ([{"foreach": {"item": "u", "array": "units", "limit": 500, "steps": [
        {"call": {"target": "u", "transition": "sell", "inputs": {"buyer": "customer"}}},
        {"create": {"type": "Warranty", "transition": "start", "inputs": {"unit": "u", "months": "u.model.warranty_months"}}}]}}],
        sell_and_warrant),
    ("Delivery", "unbind_slot"): ([{"call": {"target": "inputs.robot", "transition": "release"}}],
                                  lambda s, o, i, t: s.call(i["robot"], "Robot", "release", {}, t)),
    ("Robot", "release"): ([{"clear": ["binding", "used_in"]}], lambda s, o, i, t: o.clear("binding", "used_in")),
    ("Shipment", "flag_missing"): ([{"call": {"target": "inputs.robot", "transition": "flag_missing"}}],
                                   lambda s, o, i, t: s.call(i["robot"], "Robot", "flag_missing", {}, t)),
    ("Robot", "flag_missing"): ([{"clear": ["peg"]}], lambda s, o, i, t: o.clear("peg")),
    ("ExpenseClaim", "submit"): ([{"assign": {"location": "submitted_at", "expr": "now"}}],
                                 lambda s, o, i, t: o.set("submitted_at", t)),
    ("ExpenseClaim", "add_line"): ([{"create": {"type": "ClaimLine", "transition": "add", "inputs": {
        "for_claim": "this", "category": "inputs.category", "amount": "inputs.amount"}}}],
        lambda s, o, i, t: s.create("ClaimLine", "add", {"for_claim": o.id, "category": i["category"], "amount": i["amount"]}, t)),
    ("ClaimLine", "add"): ([{"assign": {"location": "claim", "expr": "inputs.for_claim"}}],
                           lambda s, o, i, t: o.set("claim", i["for_claim"])),
}.items():
    EFFECTS.setdefault(_key, []).append(_alt)
NO_EFFECT = {("Delivery", "mark_ready"), ("Delivery", "back_to_preparation"), ("Robot", "record_manufacturer_serial"),
             ("RobotModel", "edit"), ("Warranty", "set_end"),
             ("Customer", "forget"), ("ServiceJob", "open"), ("ServiceJob", "reassign"), ("ServiceJob", "start"), ("User", "add"),
             ("Return", "inspect"), ("Return", "close"), ("Return", "await_parts"), ("Return", "parts_in"),
             ("RobotModel", "set_reorder_point"), ("RobotModel", "set_yield_floor"), ("RobotModel", "set_usual_lead_time"),
             ("Warranty", "start"), ("Service", "register"), ("Delivery", "open"), ("Robot", "add_opening_stock"), ("Robot", "add_to_intake"), ("Robot", "request"),
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
    ("Delivery", "first_pass_rate"): {"source": "pdi_results", "item": "r", "filter": FIRST,
        "dimensions": {"model": "r.unit.model"}, "time_dimension": "r.occurred_at",
        "expression": "count(where r.outcome != CheckOutcome.FAIL) * 1.000 / count()"},
    ("Return", "returns_by_model"): {"source": "objects", "item": "r", "dimensions": {"model": "r.unit.model"},
                                     "time_dimension": "r.created_at", "expression": "count()"},
    ("Return", "labelled_returns"): {"source": "labels", "item": "l", "filter": 'l.name == "missing-charger"',
                                     "dimensions": {"state": "l.subject_state"}, "expression": "count(distinct l.subject)"},
    ("Return", "labelled_wait"): {"source": "labels", "item": "l", "filter": 'l.name == "missing-charger"',
                                  "dimensions": {"state": "l.subject_state"}, "expression": "median(l.subject.time_in(INSPECTING))"},
    ("Return", "time_to_done"): {"source": "transitions", "item": "t", "filter": "t.to_state == Return.CLOSED and not t.migrated",
                                 "dimensions": {"month": "month(t.occurred_at)"},
                                 "expression": "median(t.occurred_at - t.object.created_at)"},
    ("Robot", "trial_refusals"): {"source": "attempts", "item": "a", "filter": "not a.enforced",
                                  "dimensions": {"clause": "a.clause", "actor": "a.actor_id"}, "expression": "count()"},
    ("Delivery", "delivery_trial_refusals"): {"source": "attempts", "item": "a", "filter": "not a.enforced",
                                     "dimensions": {"clause": "a.clause", "actor": "a.actor_id"}, "expression": "count()"},
    ("Delivery", "delivery_cycle_time"): {"source": "transitions", "item": "t",
        "filter": "t.to_state == Delivery.DELIVERED and t.from_state == Delivery.PREPARATION",
        "dimensions": {"creator": "t.object.created_by_kind"}, "expression": "median(t.occurred_at - t.object.created_at)"},
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
        self.created_by_kind, self.members = kind, {}     # members: {member: [[value, entered, left, kind, legacy]]}
        self.intervals = []                         # [state, entered, left, entered_by_kind, legacy]
        self.open_interval(state, at, kind)

    def open_interval(self, state, at, kind):
        if self.intervals:
            self.intervals[-1][2] = at
        self.intervals.append([state, at, None, kind, False])

    def hold(self, member, value, at, kind):
        """A tracked member takes a value: its current interval ends and a new one opens (DESIGN.md §7)."""
        spans = self.members.setdefault(member, [])
        if spans and spans[-1][2] is None:
            if spans[-1][0] == value:
                return
            spans[-1][2] = at
        spans.append([value, at, None, kind, False])

    def entered_at(self, state):
        """When the object last entered `state`, legacy intervals included, or None."""
        return max((iv[1] for iv in self.intervals if iv[0] == state), default=None)

    def set(self, k, v):
        self.attrs[k] = v

    def clear(self, *ks):
        for k in ks:
            self.attrs[k] = None


class Observation:
    def __init__(self, oid, kind, subject, coll, fields, occurred, recorded, corrects, actor_kind, actor=None):
        self.id, self.type, self.subject, self.coll, self.fields = oid, kind, subject, coll, dict(fields)
        self.occurred, self.recorded, self.corrects, self.kind, self.actor = occurred, recorded, corrects, actor_kind, actor
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


def module_text(ref):
    """`<document> <n>`: the n-th module a design document writes, as a version of it; or a module file, whole."""
    if ref.endswith(".yaml"):
        return (ROOT / "docs/design" / ref).read_text()
    name, n = ref.split()
    blocks = [b for info, _s, b in fences((ROOT / "docs/design" / name).read_text()) if info == "yaml" and b.startswith("module:")]
    return blocks[int(n) - 1]


def patched(text, adds, removes=()):
    """The module's text with each path of a publish removed, and each `<path>: <value>` added in its section's order."""
    doc = flows.load(text)
    for path in removes:
        keys, node = path.split("."), doc
        for k in keys[:-1]:
            node = node[k]
        del node[keys[-1]]
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
        self.publishes = sorted((p for p in publishes if not p.get("change")), key=lambda p: when(p["at"]))
        self.governed = {p["change"]: p for p in publishes if p.get("change")}
        self.changes, self.observed, self.actor = {}, [], None
        self.version, self.findings = 1, []
        self.objects, self.obs, self.events, self.attempts, self.reading = {}, {}, [], [], None
        self.subscriptions, self.consulted, self.floors, self.proposals = {}, {}, {}, {}
        self.causes, self.creating, self.checks, self.answers, self.notices = [], [], {}, {}, {}
        self.principal = None
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
        for (tn, d), (text, _f) in DERIVED.items():
            got = (self.types.get(tn, {}).get("derived_attributes") or {}).get(d)
            if got:
                assert one(got["expression"]) == text, f"{tn}.{d} is now `{one(got['expression'])}`; the transcription reads `{text}`"
        for (tn, c), (text, _f) in INVARIANTS.items():
            got = one(self.types[tn]["invariants"][c]["expression"])
            assert got == text, f"{tn}.{c} is now `{got}`; the transcription reads `{text}`"
        for tn in ("Robot", "RobotModel", "ServiceJob"):
            for c in (self.types[tn].get("invariants") or {}):
                assert (tn, c) in INVARIANTS, f"{tn}.{c} is not transcribed"
        for (tn, x), alts in EFFECTS.items():
            got = literal((self.types.get(tn, {}).get("transitions") or {}).get(x, {}).get("effect"))
            assert not got or got in [literal(e) for e, _f in alts], \
                f"{tn}.{x}'s effect is now {got}; the transcription reads {[literal(e) for e, _f in alts]}"
        for tn, x in NO_EFFECT:
            if x in (self.types.get(tn, {}).get("transitions") or {}):
                assert not self.types[tn]["transitions"][x].get("effect"), f"{tn}.{x} now has an effect"
        for (tn, mn), want in DECLARED_METRICS.items():
            got = ((self.mods["inventory_journey"].get("metrics") or {}) if not tn
                   else (self.types.get(tn, {}).get("metrics") or {})).get(mn)
            if got:
                got = {k: v for k, v in literal(got).items() if k != "description"}
                assert got == literal(want), f"{tn}.{mn} is now {got}; the transcription reads {literal(want)}"

    def publish(self, p, at=None, kind="human"):
        at = at or when(p["at"])
        mods = p.get("modules") or {"inventory_journey": {"add": p.get("add") or {}}}
        prev = dict(self.text)
        for name, change in mods.items():
            base = module_text(change["text"]) if change.get("text") else self.text[name]
            self.text[name] = patched(base, change.get("add") or {}, change.get("remove") or [])
        order = [n for n in ("operations_shared", "inventory_journey", "returns") if n in self.text]
        order += [n for n in self.text if n not in order]
        files = [(f"{n}.yaml", self.text[n]) for n in order]
        errs, notes = flows.check(files, {f"{n}.yaml": prev[n] for n in mods if n in prev})
        self.findings += [f"the version published at {at:%Y-%m-%dT%H:%MZ}: {e[0]} {e[3]} {e[4]}"
                          for e in errs if e[0][:-len(".yaml")] in mods]
        self.version += 1
        self.notices[self.version] = [{"module": n[0][:-len(".yaml")], "code": n[3], "notice": n[4]}
                                      for n in notes if n[0][:-len(".yaml")] in mods]
        self.load()
        # a publish changes a live object only by a recorded migration (flow-format.md §4.16)
        for name in mods:
            for tn, mapping in ((self.mods[name].get("migration") or {}).get("removed_states") or {}).items():
                for o in self.objects.values():
                    if o.type == tn and o.state in mapping:
                        frm, o.state = o.state, mapping[o.state]
                        o.open_interval(o.state, at, kind)
                        self.events.append({"object": o.id, "type": tn, "transition": "migrate", "from": frm, "to": o.state,
                                            "kind": kind, "occurred": at, "recorded": at, "overrides": False, "imported": False,
                                            "migrated": True, "reason": None, "reads": set(), "version": self.version})

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

    def tracked(self, tn):
        return [a for a, s_ in (self.types[tn].get("attributes") or {}).items() if s_.get("assignee")]

    def identity(self, actor):
        """The value the actor's object holds in its attribute marked `actor_kind` (ADR-0122 decision 21)."""
        o = self.objects[actor]
        return next(o.attrs[a] for a, s_ in (self.types[o.type].get("attributes") or {}).items() if s_.get("actor_kind"))

    def known(self, actor):
        """An actor the store holds: an object of a type that marks an actor identity."""
        o = self.objects.get(actor)
        return o is not None and any(a.get("actor_kind") for a in (self.types[o.type].get("attributes") or {}).values())

    def named(self, actor):
        """What an attempt records for its actor: the identity, or what the request named where no actor holds it."""
        return self.identity(actor) if self.known(actor) else actor

    def is_a(self, tn, want):
        while tn:
            if tn == want:
                return True
            tn = (self.types.get(tn) or {}).get("extends")
        return False

    def invalid_input(self, tn, x, inputs):
        """An input naming no object, or one of another type than the reference it fills (DESIGN.md §6 step 4)."""
        specs = dict(x.get("inputs") or {})
        for a in (x.get("required_inputs") or []) + (x.get("optional_inputs") or []):
            specs.setdefault(a, ((self.types[tn].get("attributes") or {}).get(a)) or {})
        for name, v in inputs.items():
            ref = (specs.get(name) or {}).get("reference")
            if ref and v is not None:
                for oid in (v if isinstance(v, list) else [v]):
                    if oid not in self.objects or not self.is_a(self.objects[oid].type, ref.rstrip("[]")):
                        return True
        return False

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
        for g, mode in (x.get("guards") or {}).items():
            if (tn, g) not in GUARDS:
                raise Refused(f"{tn}.{xn}'s guard {g} is not transcribed, so this history cannot reach it")
            if not GUARDS[(tn, g)][1](self, o, inputs):
                remedy = self.types[tn]["conditions"][g]["remedy"]
                if mode in ("audit", "warn"):
                    self.observed.append((g, remedy, mode))
                    continue
                raise Fails("unsatisfied", g, remedy)

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

    def create(self, tn, xn, inputs, occ):
        """A `create` step: the object named next by the request's `creates`, by an initial transition."""
        if not self.creating:
            raise Refused(f"a request creates a {tn}, and names no id for it in `creates`")
        x = self.types[tn]["transitions"][xn]
        o = Obj(self.creating.pop(0), tn, x["to"], {}, occ, self.kind)
        self.objects[o.id] = o
        try:
            self.guards(tn, xn, x, o, inputs)
        except Fails as f:
            f.v["call"] = None
            raise
        self.apply(o, tn, xn, {**x, "kind": "created"}, inputs, occ)

    def attr_ref(self, tn, name):
        spec = ((self.types.get(tn) or {}).get("attributes") or {}).get(name) or {}
        return spec["reference"].rstrip("[]") if spec.get("reference") else None

    def path_type(self, tn, x, env, path):
        """The type a step's path reaches: from an input, a loop's item, `this` or the object's own member."""
        first, *rest = path.split(".")
        if first == "inputs":
            name, *rest = rest
            spec = (x.get("inputs") or {}).get(name) or {}
            cur = spec["reference"].rstrip("[]") if spec.get("reference") else self.attr_ref(tn, name)
        else:
            cur = env[first] if first in env else tn if first == "this" else self.attr_ref(tn, first)
        for p in rest:
            cur = self.attr_ref(cur, p) if cur else None
        return cur

    def causes_of(self, tn, xn):
        """Every transition whose outcome can cause `tn.xn`, as the rule set prints it (renderers.md §2, ADR-0108)."""
        rows = []

        def walk(ctn, cxn, x, steps, env, loop):
            for st in steps or []:
                if "foreach" in st:
                    f = st["foreach"]
                    walk(ctn, cxn, x, f["steps"], {**env, f["item"]: self.path_type(ctn, x, env, f["array"])},
                         (f["array"], f.get("where"), f["limit"]))
                elif "call" in st or "create" in st:
                    c = st.get("call") or st["create"]
                    hit = (c["transition"] == xn and self.path_type(ctn, x, env, c["target"]) == tn) if "call" in st \
                        else (c["type"] == tn and c["transition"] == xn)
                    if hit:
                        rows.append({"cause": f"{ctn}.{cxn}", "through": loop[0] if loop else c.get("target"),
                                     "where": loop[1] if loop else None, "limit": loop[2] if loop else None})
        for ctn, t in self.types.items():
            for cxn, x in (t.get("transitions") or {}).items():
                walk(ctn, cxn, x, x.get("effect"), {}, None)
            for an, a in (t.get("attributes") or {}).items():
                for cl in a.get("cascade") or []:
                    if cl["transition"] == xn and self.attr_ref(ctn, an) == tn:
                        rows += [{"cause": f"{ctn}.{on}", "through": an, "where": None, "limit": cl["limit"]} for on in cl["on"]]
        printed = "caused by" if self.types[tn]["transitions"][xn].get("only_via") else "also caused by"
        return [{**r, "printed": printed} for r in rows]

    def apply(self, o, tn, xn, x, inputs, occ, to=None):
        frm = o.state
        held = {m: (None if x["kind"] == "created" else o.attrs.get(m)) for m in self.tracked(tn)}
        for a in (x.get("required_inputs") or []) + (x.get("optional_inputs") or []):
            if a in inputs:
                o.set(a, inputs[a])
        # a cascaded event records the event that caused it (DESIGN.md §6, ADR-0108)
        cause = self.causes[-1] if self.causes else None
        if x.get("effect"):
            fn = next((f for e, f in EFFECTS.get((tn, xn), []) if literal(e) == literal(x["effect"])), None)
            if fn is None:
                raise Refused(f"{tn}.{xn}'s effect is not transcribed")
            self.causes.append(f"{o.id}.{xn}")
            try:
                fn(self, o, inputs, occ)
            finally:
                self.causes.pop()
        target = to or x.get("to")
        if x["kind"] in ("external", "assertion") and target:
            o.state = target
            o.state_source = "asserted" if x["kind"] == "assertion" else "observed"
            o.open_interval(target, occ, self.kind)
        for m in self.tracked(tn):
            o.hold(m, o.attrs.get(m), occ, self.kind)
        self.check_invariants(o)
        # an invariant reading another object is checked when a request writes that object (DESIGN.md §6 step 7):
        # a unit's reads its model's flags, and a job's its engineer's state
        readers = {"RobotModel": ("Robot", "model"), "User": ("ServiceJob", "engineer")}
        if tn in readers:
            rt, ref = readers[tn]
            for u in sorted((u for u in self.objects.values() if u.type == rt and u.attrs.get(ref) == o.id), key=lambda u: u.id):
                self.check_invariants(u)
        self.events.append({"object": o.id, "type": tn, "transition": xn, "from": frm, "to": o.state, "kind": self.kind,
                            "cause": cause, "principal": self.identity(self.principal) if self.principal else None,
                            "actor": self.identity(self.actor) if self.actor else None, "held": held,
                            "payload": {a: inputs[a] for a in (x.get("required_inputs") or []) + (x.get("optional_inputs") or [])
                                        if a in inputs},
                            "occurred": occ, "recorded": self.at, "overrides": x["kind"] == "assertion",
                            "imported": False, "reason": inputs.get("reason") if x["kind"] == "assertion" else None,
                            "reads": set(), "migrated": False, "version": self.version})

    # ── one request ──
    def attempt(self, r):
        """Take the request on this store; raise Fails with its first failure."""
        at = self.at = when(r["at"])
        for who in (r["actor"], r.get("principal")):
            if who is not None and not self.known(who):
                self.kind = "unknown"
                raise Fails("unsatisfied", "actor_known", "dependent")
        self.kind = self.kind_of(r["actor"])
        self.principal = r.get("principal")
        occ = when(r["occurred_at"]) if r.get("occurred_at") else at
        inputs = {k: value(v) for k, v in (r.get("inputs") or {}).items()}
        self.consulted, self.floors, self.observed, self.actor = {}, {}, [], r["actor"]
        self.creating = list(r.get("creates") or [])
        # `check` simulates the request and changes nothing: advice, not a reservation (DESIGN.md §10)
        if r.get("check"):
            trial = copy.deepcopy(self)
            try:
                trial.attempt({k: v for k, v in r.items() if k != "check"})
                self.checks[r["check"]] = {"verdict": "satisfied", "clause": None, "remedy": None, "call": None}
            except Fails as f:
                self.checks[r["check"]] = dict(f.v)
            self.creating = []
            return
        # a flow changes only through a DeclarationChange: drafted, submitted with its impact, approved (publish-and-import.md §1)
        if r.get("draft"):
            self.changes[r["draft"]] = {"drafted_by": self.identity(r["actor"]), "drafted_kind": self.kind, "drafted_at": at,
                                        "evidence": [(e, read(self, e)[0]) for e in r.get("evidence") or []]}
            return
        if r.get("submit"):
            self.changes[r["submit"]]["impact"] = self.impact(self.governed[r["submit"]])
            return
        if r.get("approve"):
            ch = self.changes[r["approve"]]
            if "impact" not in ch:
                raise Refused(f"{r['approve']} is approved before it was submitted with its impact")
            if self.impact(self.governed[r["approve"]]) != ch["impact"]:
                raise Fails("impact_unchanged", None, "self_serviceable")
            self.publish(self.governed.pop(r["approve"]), at, self.kind)
            ch.update(approved_by=self.identity(r["actor"]), approved_kind=self.kind, approved_at=at, version=self.version)
            return
        if r.get("propose"):
            tn = self.objects[r["object"]].type
            if not self.types[tn]["transitions"][r["transition"]].get("proposable"):
                raise Refused(f"{tn}.{r['transition']} is not proposable")
            self.proposals[r["propose"]] = {"request": {k: v for k, v in r.items() if k in ("object", "transition", "inputs")},
                                            "proposed_by": self.identity(r["actor"]), "proposed_kind": self.kind, "state": "pending"}
            return
        if r.get("approve_proposal"):
            pr = self.proposals[r["approve_proposal"]]
            self.attempt({**pr["request"], "at": r["at"], "actor": r["actor"]})
            pr.update(approved_by=self.identity(r["actor"]), approved_kind=self.kind_of(r["actor"]), state="executed")
            return
        if r.get("forget"):
            ob = self.obs[r["forget"]]
            subj = self.objects[ob.subject]
            spec_ = next(v for v in (self.types[subj.type].get("observations") or {}).values() if v["kind"] == ob.type)
            for a, s_ in (spec_.get("attributes") or {}).items():
                if s_.get("personal"):
                    ob.fields[a] = None
            return
        if r.get("record"):
            self.record(r, at, occ)
            return
        if r.get("label"):
            subj = self.objects[r["subject"]]
            self.obs[r["object"]] = Observation(r["object"], "label", subj.id, "labels",
                                                {"name": r["label"], "subject_state": subj.state}, at, at, None, self.kind)
            return
        if r.get("subscribe"):
            self.subscriptions[r["subscribe"]] = {"filter": r["filter"], "from": len(self.events)}
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
            if self.invalid_input(tn, x, inputs):
                raise Fails("invalid input", None, "self_serviceable")
            o = Obj(r["object"], tn, x["to"], {}, occ, self.kind)
            self.objects[o.id] = o
            self.guards(tn, r["transition"], x, o, inputs)
            self.apply(o, tn, r["transition"], {**x, "kind": "created"}, inputs, occ)
            self.settle(r, o, tn, at)
            return
        o = self.objects[r["object"]]
        if x["kind"] == "initial":
            raise Refused(f"{tn}.{r['transition']} is a creation, requested on {o.id}")
        if x["kind"] == "erasure":
            personal = [a for a, s_ in (self.types[tn].get("attributes") or {}).items() if s_.get("personal")]
            for a in personal:
                o.attrs[a] = None
            for e in self.events:
                if e["object"] == o.id:
                    for a in personal:
                        if a in (e.get("payload") or {}):
                            e["payload"][a] = None
            self.events.append({"object": o.id, "type": tn, "transition": r["transition"], "from": o.state, "to": o.state,
                                "kind": self.kind, "actor": self.identity(r["actor"]), "held": {}, "payload": {},
                                "occurred": at, "recorded": at, "overrides": True, "imported": False, "migrated": False,
                                "reason": "erasure", "reads": set(), "version": self.version})
            return
        if x["kind"] == "assertion":
            to = inputs.get("to")
            if to not in x["to"] or to == o.state or self.final(tn, o.state):
                raise Refused(f"{o.id}: {tn}.{r['transition']} to {to} from {o.state} is not an assertion it may make")
            self.apply(o, tn, r["transition"], x, inputs, occ, to=to)
            return
        if not self.leaves(tn, x, o.state):
            raise Fails("unavailable", None, "unreachable_from_here")
        if self.invalid_input(tn, x, inputs):
            raise Fails("invalid input", None, "self_serviceable")
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
        self.settle(r, o, tn, at)
        for e in self.events[n:]:
            if e["object"] == o.id and e["transition"] == r["transition"]:
                e["reads"], e["consulted"] = reads, dict(self.consulted)

    def settle(self, r, o, tn, at):
        """A request that applied: each audited or flagged failure goes to the attempt log, linked to it, and a flag
        is returned with the verdict (DESIGN.md §6 step 4, ADR-0111)."""
        for g, remedy, mode in self.observed:
            self.attempts.append({"object": o.id, "type": tn, "transition": r["transition"], "verdict": "unsatisfied",
                                  "clause": g, "remedy": remedy, "kind": self.kind, "actor": self.identity(r["actor"]),
                                  "at": at, "enforced": False, "flagged": mode == "warn", "consulted": {}, "floors": {},
                                  "call": None})
        if r.get("answer"):
            flags = [(g, remedy) for g, remedy, mode in self.observed if mode == "warn"]
            self.answers[r["answer"]] = [{"verdict": "satisfied", "flag": g, "remedy": remedy} for g, remedy in flags] \
                or [{"verdict": "satisfied", "flag": None, "remedy": None}]

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
        o = Observation(r["object"], spec_["kind"], subj.id, r["record"], fields, occ, at, fixed, self.kind,
                        self.identity(r["actor"]))
        self.obs[o.id] = o
        if fixed:
            self.obs[fixed].corrected_by = o.id

    def replay(self, block):
        self.now = when(block["now"])
        if block.get("port"):
            self.port = when(block["port"])
        for imp in block.get("imported") or []:
            port = when(imp["at"]) if imp.get("at") else self.port
            at = when(imp["entered"]) if imp.get("entered") else port
            o = Obj(imp["object"], imp["type"], imp["state"], {k: value(v) for k, v in (imp.get("attributes") or {}).items()},
                    at, imp.get("entered_by", "unknown"))
            legacy = [[st, when(a), when(b), imp.get("entered_by", "unknown"), True] for st, a, b in imp.get("legacy") or []]
            o.intervals = legacy + o.intervals
            known = [iv[1] for iv in o.intervals] + ([port] if not imp.get("entered") else [])
            o.created_at = when(imp["created"]) if imp.get("created") else min(known)
            o.undated = not imp.get("created")
            o.created_by_kind = imp.get("created_by", "unknown")
            for m, spans in (imp.get("legacy_members") or {}).items():
                o.members[m] = [[v, when(a), when(b), imp.get("entered_by", "unknown"), True] for v, a, b in spans]
            for m in self.tracked(o.type):
                entered = (imp.get("member_entered") or {}).get(m)
                o.members.setdefault(m, []).append([o.attrs.get(m), when(entered) if entered else port, None,
                                                    imp.get("entered_by", "unknown"), False])
            o.state_source = "imported"
            silent = imp.get("silent")
            o.recorded_from = (when(silent[1]) if silent else o.created_at if imp.get("created") else port)
            self.objects[o.id] = o
            self.events.append({"object": o.id, "type": o.type, "transition": "import", "from": None, "to": o.state,
                                "kind": "unknown", "occurred": port, "recorded": port, "overrides": True,
                                "imported": True, "reason": None, "reads": set(), "migrated": False, "version": self.version,
                                "payload": dict(o.attrs)})
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
            if self.creating:
                raise Refused(f"the request at {r['at']} names {self.creating} in `creates`, and creates nothing by them")
        while self.publishes and when(self.publishes[0]["at"]) <= self.now:
            self.publish(self.publishes.pop(0))

    def impact(self, p):
        """A change's impact report, for the change this history drafts: each object a guard it enforces
        would now refuse, in a state its transition leaves (publish-and-import.md §1)."""
        rows = []
        for path, value in ((p.get("modules") or {}).get("inventory_journey", {}).get("add") or {}).items():
            k = path.split(".")
            if value != "deny" or k[-2] != "guards":
                continue
            kind_, owner, xn, g = k[0], k[1], k[3], k[-1]
            types = [tn for tn, t in self.types.items() if (t.get("state_machine") == owner if kind_ == "machines" else tn == owner)]
            for tn in types:
                x = self.types[tn]["transitions"][xn]
                for o in sorted(self.objects.values(), key=lambda o: o.id):
                    if o.type == tn and self.leaves(tn, x, o.state) and not GUARDS[(tn, g)][1](self, o, {}):
                        rows.append({"object": o.id, "transition": xn, "clause": g})
        return rows

    def refused(self, r, at):
        want = {"verdict": r["refused"]["verdict"], "clause": r["refused"].get("clause"),
                "remedy": r["refused"]["remedy"], "call": r["refused"].get("call")}
        consulted, objects = {}, {}
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
                consulted, objects = dict(trial.consulted), dict(trial.floors)
        if r.get("record"):
            tn = self.types[self.objects[r["subject"]].type]["observations"][r["record"]]["kind"]
        else:
            tn = r.get("create") or self.objects[r["object"]].type
        self.attempts.append({"object": None if r.get("record") or r.get("create") else r["object"], "type": tn,
                              "transition": "record" if r.get("record") else r["transition"],
                              "verdict": want["verdict"], "clause": want["clause"], "remedy": want["remedy"],
                              "kind": self.kind_of(r["actor"]) if self.known(r["actor"]) else "unknown",
                              "actor": self.named(r["actor"]), "at": at, "enforced": True, "flagged": False,
                              "consulted": consulted, "floors": objects, "call": want["call"]})


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


def time_in(s, o, state):
    """`o.time_in(<state>)`: the total of the object's spans in the state, each by §6.9's rule for a span."""
    return sum((d for iv in o.intervals if iv[0] == state for d in [duration(s, o, iv)] if d is not None), timedelta())


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


def metric(s, name, keep, filt, over=None, bind=None):
    tn, _, std = name.partition(".")
    keepf = row_filter(filt)
    if name in ("supplier_lead_time", "supplier_lead_time_p80"):
        rows = []
        for o in s.objects.values():
            got, ordered = o.entered_at("INTAKE"), o.entered_at("REQUESTED")
            if o.type != "Robot" or got is None or ordered is None or (over and got < s.now - over):
                continue
            gap = o.id if o.undated or got < o.recorded_from or ordered < o.recorded_from else None
            maker = s.objects[o.attrs["model"]].attrs.get("manufacturer")
            dims = {"supplier": maker, "manufacturer": maker, "month": got.strftime("%Y-%m")}
            if keepf(dims):
                rows.append((dims, got - o.created_at, gap))
        return rows_of(group(rows, keep), keep, lambda b: nearest_rank(b, 0.5 if name == "supplier_lead_time" else 0.8))
    if name == "weak_battery_units":
        rows = [({"model": s.objects[b.subject].attrs["model"], "week": week(b.occurred)}, b.subject, None)
                for b in s.obs.values() if b.coll == "battery_days" and b.corrected_by is None and b.fields["min_health"] < 80.0]
        return rows_of(group(rows, keep), keep, lambda b: len(set(b)))
    if name == "delivery_cycle_time":
        rows = [({"creator": s.objects[e["object"]].created_by_kind}, e["occurred"] - s.objects[e["object"]].created_at, None)
                for e in s.events if e["type"] == "Delivery" and e["from"] == "PREPARATION" and e["to"] == "DELIVERED"]
        return rows_of(group(rows, keep), keep, lambda b: nearest_rank(b, 0.5))
    if std in ("throughput", "rework"):
        rows = []
        for e in s.events:
            if e["type"] != tn or e["imported"] or e["migrated"] or e["from"] is None:
                continue
            o = s.objects[e["object"]]
            before = [iv[0] for iv in o.intervals if iv[1] < e["occurred"]]
            st = lambda x: s.types[tn]["states"][x]
            is_open = lambda x: st(x).get("category") != "closed" and not st(x).get("final")
            hit = (is_open(e["from"]) and not is_open(e["to"])) if std == "throughput" else \
                (e["to"] != e["from"] and e["to"] in before[:-1])
            if hit:
                rows.append(({"state": e["to"], "actor_kind": e["kind"], "week": week(e["occurred"])}, 1, None))
        return rows_of(group(rows, keep), keep, len)
    if std == "refusal_rate":
        bound = bind or {}
        refused, applied = {}, {}
        for a in s.attempts:
            if a["type"] == tn and a["enforced"] and all({"transition": a["transition"]}.get(k) == v for k, v in bound.items()):
                k = tuple({"transition": a["transition"], "actor_kind": a["kind"]}[d] for d in keep)
                refused[k] = refused.get(k, 0) + 1
        for e in s.events:
            if e["type"] == tn and not e["imported"] and not e["migrated"] and e["from"] is not None \
                    and all({"transition": e["transition"]}.get(k) == v for k, v in bound.items()):
                k = tuple({"transition": e["transition"], "actor_kind": e["kind"]}[d] for d in keep)
                applied[k] = applied.get(k, 0) + 1
        return [{**dict(zip(keep, k)), "value": rate(refused.get(k, 0), refused.get(k, 0) + applied.get(k, 0)), "gaps": set()}
                for k in set(refused) | set(applied)]
    if name.startswith("ServiceJob.") and name.endswith(".engineer"):
        m = name.split(".")[1]
        jobs = [o for o in s.objects.values() if o.type == "ServiceJob"]
        spans = lambda o: [iv for iv in o.members.get("engineer", []) if iv[0] is not None]
        if m == "open_work":
            rows = [({"assignee": o.attrs.get("engineer")}, 1, None) for o in jobs if s.open(o)]
            return rows_of(group(rows, keep), keep, len)
        if m == "time_to_first_assignment":
            rows = [({"month": o.created_at.strftime("%Y-%m")}, min(iv[1] for iv in spans(o)) - o.created_at, None)
                    for o in jobs if spans(o)]
            return rows_of(group(rows, keep), keep, lambda b: nearest_rank(b, 0.5))
        if m in ("handoffs_by_object", "returns_by_object"):
            rows = [({"object": o.id}, len(spans(o)) - (1 if m == "handoffs_by_object" else len({iv[0] for iv in spans(o)})), None)
                    for o in jobs]
            return rows_of(group(rows, keep), keep, sum)
        if m == "cycle_time_by_assignee":
            rows = []
            for e in s.events:
                st = s.types["ServiceJob"]["states"]
                if e["type"] == "ServiceJob" and e["from"] and st[e["from"]].get("category") != "closed" \
                        and st[e["to"]].get("category") == "closed" and not e["imported"]:
                    dims = {"assignee": e["held"]["engineer"], "state": e["to"], "month": e["occurred"].strftime("%Y-%m")}
                    if all(dims[k] == v for k, v in (bind or {}).items()):
                        rows.append((dims, e["occurred"] - s.objects[e["object"]].created_at, None))
            return rows_of(group(rows, keep), keep, lambda b: nearest_rank(b, 0.5))
        if m == "acted_by_non_assignee":
            rows = [({"transition": e["transition"]}, e["actor"] != s.identity(e["held"]["engineer"]), None)
                    for e in s.events if e["type"] == "ServiceJob" and not e["imported"] and e.get("held", {}).get("engineer")]
            return rows_of(group(rows, keep), keep, lambda b: rate(sum(b), len(b)))
    if name in ("labelled_returns", "labelled_wait"):
        rows = [({"state": l.fields["subject_state"]},
                 l.subject if name == "labelled_returns" else time_in(s, s.objects[l.subject], "INSPECTING"), None)
                for l in s.obs.values() if l.coll == "labels" and l.fields["name"] == "missing-charger"
                and s.objects[l.subject].type == "Return"]
        return rows_of(group(rows, keep), keep, (lambda b: len(set(b))) if name == "labelled_returns" else (lambda b: nearest_rank(b, 0.5)))
    if name == "time_to_done":
        rows = [({"version": e["version"], "month": e["occurred"].strftime("%Y-%m")},
                 e["occurred"] - s.objects[e["object"]].created_at, None)
                for e in s.events if e["type"] == "Return" and e["to"] == "CLOSED" and e["from"] != "CLOSED" and not e["migrated"]]
        return rows_of(group(rows, keep), keep, lambda b: nearest_rank(b, 0.5))
    if name in ("trial_refusals", "delivery_trial_refusals"):
        tn = "Robot" if name == "trial_refusals" else "Delivery"
        rows = [({"clause": a["clause"], "actor": a["actor"]}, 1, None)
                for a in s.attempts if a["type"] == tn and not a["enforced"]]
        return rows_of(group(rows, keep), keep, len)
    if std == "flags_raised":
        rows = [({"transition": a["transition"], "clause": a["clause"], "remedy": a["remedy"], "actor_kind": a["kind"],
                  "week": week(a["at"])}, 1, None) for a in s.attempts if a["type"] == tn and a.get("flagged")]
        return rows_of(group(rows, keep), keep, len)
    if std == "transition_counts":
        # a creation is a transition too; its `held` is absent (declaration-syntax.md §6.9)
        rows = []
        for e in s.events:
            if e["type"] == tn and not e["imported"] and not e["migrated"]:
                dims = {"transition": e["transition"], "actor_kind": e["kind"], "week": week(e["occurred"]),
                        "version": e["version"]}
                if keepf(dims):
                    rows.append((dims, 1, None))
        return rows_of(group(rows, keep), keep, len)
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
    m = re.fullmatch(r'metric\(([\w.]+)(?:, bind: \{(\w+): ([\w-]+)\})?, keep: \[([\w, ]*)\](?:, filter: "(.+?)")?(?:, over: "(\d+ days)")?\)', spec_)
    if m:
        keep = [k.strip() for k in m.group(4).split(",") if k.strip()]
        bind = {m.group(2): m.group(3)} if m.group(2) else None
        return (metric(s, m.group(1), keep, (m.group(5) or "").replace('\\"', '"'), span(m.group(6)) if m.group(6) else None, bind),
                keep + ["value", "gaps"])
    m = re.fullmatch(r"intervals\(([\w-]+)\.(\w+)\)", spec_)
    if m:
        return [{"value": v, "entered_at": a, "entered_by_kind": k, "legacy": str(leg).lower()}
                for v, a, _b, k, leg in s.objects[m.group(1)].members[m.group(2)]], ["value", "entered_at", "entered_by_kind", "legacy"]
    m = re.fullmatch(r"available\((\w+), (\w+)\)", spec_)
    if m:
        # the objects in a state the transition leaves whose every enforced guard holds (DESIGN.md §10, ADR-0048)
        tn, xn = m.groups()
        x = s.types[tn]["transitions"][xn]
        assert not x.get("only_via"), f"{tn}.{xn} is not requestable, so `available` does not list it"
        got = [o for o in s.objects.values() if o.type == tn and s.leaves(tn, x, o.state)
               and all(GUARDS[(tn, g)][1](s, o, {}) for g, mode in (x.get("guards") or {}).items() if mode == "deny")]
        return [{"object": o.id} for o in sorted(got, key=lambda o: o.id)], ["object"]
    m = re.fullmatch(r"answer ([\w-]+)", spec_)
    if m:
        return s.answers[m.group(1)], ["verdict", "flag", "remedy"]
    m = re.fullmatch(r"notices of version (\d+)", spec_)
    if m:
        return s.notices[int(m.group(1))], ["module", "code", "notice"]
    m = re.fullmatch(r"provenance\(([\w-]+)\)", spec_)
    if m:
        return [{"transition": e["transition"], "actor": e.get("actor"), "actor_kind": e["kind"], "principal": e.get("principal")}
                for e in s.events if e["object"] == m.group(1)], ["transition", "actor", "actor_kind", "principal"]
    if spec_ == "actor kinds":
        # each type that holds actors, the identity a request names them by, and the kind it declares (renderers.md §2)
        return [{"type": tn, "identity": a, "kind": spec["actor_kind"]} for tn in sorted(s.types)
                for a, spec in (s.types[tn].get("attributes") or {}).items() if spec.get("actor_kind")], \
            ["type", "identity", "kind"]
    m = re.fullmatch(r"marks\((\w+)\.(\w+)\.(\w+)\)", spec_)
    if m:
        # a standard assignment metric's dimensions (declaration-syntax.md §6.11), each marked where its value names
        # an actor: an object of a type that marks an actor identity, or an actor's id (DESIGN.md §5.12)
        tn, mn, r = m.groups()
        assert mn == "cycle_time_by_assignee", f"no marks for {mn} in this checker"
        target = s.attr_ref(tn, r)
        names = lambda ty: any(a.get("actor_kind") for a in (s.types[ty].get("attributes") or {}).values())
        dims = [("assignee", target), ("state", None), ("month", None), ("version", None), ("actor_kind", None)]
        return [{"dimension": d, "names_an_actor": "yes" if ty and names(ty) else "no"} for d, ty in dims], \
            ["dimension", "names_an_actor"]
    m = re.fullmatch(r"check ([\w-]+)", spec_)
    if m:
        return [dict(s.checks[m.group(1)])], ["verdict", "clause", "remedy", "call"]
    m = re.fullmatch(r"rules\((\w+)\.(\w+)\)", spec_)
    if m:
        # the rule set's `requires`, in the order the guards are evaluated (renderers.md §2)
        tn, xn = m.groups()
        x = s.types[tn]["transitions"][xn]
        marked = {"deny": None, "audit": "OBSERVING — NOT ENFORCED", "warn": "FLAG — NOT ENFORCED"}
        return [{"clause": g, "remedy": s.types[tn]["conditions"][g]["remedy"], "enforced": "yes" if mode == "deny" else "no",
                 "marked": marked[mode]} for g, mode in (x.get("guards") or {}).items()], ["clause", "remedy", "enforced", "marked"]
    m = re.fullmatch(r"causes\((\w+)\.(\w+)\)", spec_)
    if m:
        return s.causes_of(*m.groups()), ["cause", "through", "where", "limit", "printed"]
    m = re.fullmatch(r"history\(([\w-]+)\)", spec_)
    if m:
        oid = m.group(1)
        if oid in s.obs:
            ob = s.obs[oid]
            return [{"transition": "record", "occurred_at": ob.occurred, "actor": ob.actor, "actor_kind": ob.kind,
                     "cause": None}], ["transition", "occurred_at", "actor", "actor_kind", "cause"]
        return [{"transition": e["transition"], "occurred_at": e["occurred"], "actor": e.get("actor"), "actor_kind": e["kind"],
                 "cause": e.get("cause")} for e in s.events if e["object"] == oid], \
            ["transition", "occurred_at", "actor", "actor_kind", "cause"]
    m = re.fullmatch(r"attempts\(([\w-]+)\)", spec_)
    if m:
        return [{"transition": a["transition"], "actor_kind": a["kind"], "verdict": a["verdict"], "clause": a["clause"],
                 "remedy": a["remedy"], "call": a.get("call")} for a in s.attempts if a["object"] == m.group(1)], \
            ["transition", "actor_kind", "verdict", "clause", "remedy", "call"]
    m = re.fullmatch(r"caused by\(([\w-]+\.\w+)\)", spec_)
    if m:
        return [{"object": e["object"], "transition": e["transition"]} for e in s.events if e.get("cause") == m.group(1)], \
            ["object", "transition"]
    m = re.fullmatch(r"get\(([\w-]+)\)", spec_)
    if m:
        o = s.objects[m.group(1)]
        return [{"attribute": a, "value": v} for a, v in sorted(o.attrs.items())], ["attribute", "value"]
    m = re.fullmatch(r"payloads\(([\w-]+)\)", spec_)
    if m:
        return [{"transition": e["transition"], "attribute": a, "value": v}
                for e in s.events if e["object"] == m.group(1) for a, v in sorted((e.get("payload") or {}).items())], \
            ["transition", "attribute", "value"]
    m = re.fullmatch(r"results\(([\w-]+)\)", spec_)
    if m:
        got = sorted((o for o in s.obs.values() if o.subject == m.group(1) and o.coll == "pdi_results" and o.corrected_by is None),
                     key=lambda o: o.recorded)
        return [{"object": o.id, "unit": o.fields["unit"], "check": o.fields["check"], "outcome": o.fields["outcome"],
                 "remark": o.fields.get("remark")} for o in got], ["object", "unit", "check", "outcome", "remark"]
    m = re.fullmatch(r"proposal ([\w-]+)", spec_)
    if m:
        pr = s.proposals[m.group(1)]
        return [{k: pr.get(k) for k in ("proposed_by", "proposed_kind", "approved_by", "approved_kind", "state")}], \
            ["proposed_by", "proposed_kind", "approved_by", "approved_kind", "state"]
    m = re.fullmatch(r'query\((\w+), filter: "(\w+)"\)', spec_)
    if m:
        tn, d = m.groups()
        assert d in (s.types[tn].get("derived_attributes") or {}), f"{tn} declares no derived attribute {d}"
        return [{"object": o.id} for o in s.objects.values() if o.type == tn and DERIVED[(tn, d)][1](s, o)], ["object"]
    if spec_ == 'query(Robot, filter: "state == REQUESTED or state == PROCUREMENT", order: "created_at")':
        got = sorted((o for o in s.objects.values() if o.type == "Robot" and o.state in ("REQUESTED", "PROCUREMENT")),
                     key=lambda o: o.created_at)
        return [{"object": o.id, "state": o.state, "created_at": o.created_at} for o in got], ["object", "state", "created_at"]
    m = re.fullmatch(r"refusal of (\w+)\.(\w+)", spec_)
    if m:
        a = [a for a in s.attempts if a["object"] == m.group(1) and a["transition"] == m.group(2)][-1]
        return [{"clause": a["clause"], "remedy": a["remedy"],
                 "consulted": ", ".join(f"{k} = {v}" for k, v in sorted(a["consulted"].items())),
                 "threshold": ", ".join(f"{k} = {v}" for k, v in sorted(a["floors"].items()))}], \
            ["clause", "remedy", "consulted", "threshold"]
    m = re.fullmatch(r"consulted of (\w+)\.(\w+)", spec_)
    if m:
        e = [e for e in s.events if e["object"] == m.group(1) and e["transition"] == m.group(2)][-1]
        return [{"reference": k, "value": v} for k, v in sorted(e.get("consulted", {}).items())], ["reference", "value"]
    m = re.fullmatch(r"change ([\w-]+)", spec_)
    if m:
        ch = s.changes[m.group(1)]
        return [{k: ch.get(k) for k in ("drafted_by", "drafted_kind", "approved_by", "approved_kind", "version")}], \
            ["drafted_by", "drafted_kind", "approved_by", "approved_kind", "version"]
    m = re.fullmatch(r"impact of ([\w-]+)", spec_)
    if m:
        return s.changes[m.group(1)]["impact"], ["object", "transition", "clause"]
    m = re.fullmatch(r"evidence of ([\w-]+)", spec_)
    if m:
        rows = []
        for e, got in s.changes[m.group(1)]["evidence"]:
            for g in got:
                rows.append({"read": e, "group": ", ".join(f"{k} = {show(v)}" for k, v in g.items() if k not in ("value", "gaps")),
                             "value": g["value"]})
        return rows, ["read", "group", "value"]
    m = re.fullmatch(r"labels\(([\w-]+)\)", spec_)
    if m:
        return [{"object": o.id, "name": o.fields["name"], "recorded_by_kind": o.kind, "subject_state": o.fields["subject_state"]}
                for o in s.obs.values() if o.coll == "labels" and o.subject == m.group(1)], \
            ["object", "name", "recorded_by_kind", "subject_state"]
    m = re.fullmatch(r"pull\(([\w-]+)\)", spec_)
    if m:
        sub_ = s.subscriptions[m.group(1)]
        f = sub_["filter"]
        return [{"object": e["object"], "transition": e["transition"], "occurred_at": e["occurred"]}
                for e in s.events[sub_["from"]:] if e["type"] == f["type"] and e["transition"] in f["transitions"]], \
            ["object", "transition", "occurred_at"]
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
    m = re.fullmatch(r"events\(([\w-]+)\)", spec_)
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


ORDERED = ("events(", "recorded(", "pull(", "intervals(", "payloads(", "results(", "history(", "attempts(", "caused by(",
           "rules(", "provenance(", "marks(")


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
        ordered = spec_.startswith(ORDERED) or "order:" in spec_
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
    ("a guard windowing a combined metric", "metric(first_pass_rate, model := u.model, over last 30 days)",
     "metric(first_pass_yield, model := u.model, over last 30 days)"),
    ("a completion said to apply without its second sign-off",
     "  - { at: 2026-11-07T01:00Z, actor: U-ANA, record: second_sign_offs, subject: D14, object: SS1, fields: {} }\n", ""),
    ("a state dropped with no mapping", "        removed_states:\n          Return: { RESOLVED: CLOSED }\n", ""),
    ("a change published without its approval", "  - { at: 2026-12-07T15:00Z, actor: U-ANA, approve: DC1 }\n", ""),
    ("a proposal on a transition that is not proposable", "      types.ServiceJob.transitions.reassign.proposable: true\n", ""),
    ("a personal attribute declared required", "contact_email: { type: string, optional: true, personal: true }",
     "contact_email: { type: string, personal: true }"),
    ("an erasure never requested",
     "  - { at: 2027-02-02T09:00Z, actor: U-ANA, object: C-DANA, transition: forget, inputs: { reason: At the customer's request } }\n", ""),
    ("a rule on trial said to refuse", "actor: A-SCOUT, object: D17, transition: complete_sale }",
     "actor: A-SCOUT, object: D17, transition: complete_sale, refused: { verdict: unsatisfied, clause: all_passed, "
     "remedy: unreachable_from_here } }"),
    ("a sale without the serial its model requires",
     "  - { at: 2027-03-09T10:00Z, actor: U-BEN, object: R20, transition: record_manufacturer_serial, "
     "inputs: { manufacturer_serial: KD-20416 } }\n", ""),
    ("a completion that creates no warranty",
     "              - create: { type: Warranty, transition: start, inputs: { unit: u, months: u.model.warranty_months } }\n", ""),
    ("a user who leaves with open jobs", "actor: U-ANA, object: U-DIYA, transition: leave }",
     "actor: U-ANA, object: U-BEN, transition: leave }"),
    ("an agent named engineer-of-record", "inputs: { engineer: A-SCOUT }, refused: { verdict: invalid input, remedy: self_serviceable } }",
     "inputs: { engineer: A-SCOUT } }"),
    ("a departed user's request said to be refused", "actor: U-DIYA, object: J3, transition: start }",
     "actor: U-DIYA, object: J3, transition: start, refused: { verdict: unsatisfied, clause: actor_known, remedy: dependent } }"),
    ("a flag said to refuse", "inputs: { category: CAT-TRAVEL, amount: 420.00 }, creates: [CL1], answer: AN1 }",
     "inputs: { category: CAT-TRAVEL, amount: 420.00 }, creates: [CL1], refused: { verdict: unsatisfied, "
     "clause: within_policy, remedy: self_serviceable } }"),
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

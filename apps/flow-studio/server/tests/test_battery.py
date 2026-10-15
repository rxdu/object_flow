"""The differential battery (ADR-0204 §4's gate over expressions): every
expression a grammar makes over typed attributes, literals and the
language's functions is judged by both checkers, which must agree, the flow
checker through the runner's load and the core's checker through a change's
publish preview, and each one both accept is read on the core and the
reference runner over four objects, an ordinary one, one of negatives, ties
and a year below 1000, one of zeros and a leap day, and one with nothing
set, which must answer alike. The same expressions are judged and run as
conditions guarding transitions, as writes to each attribute, and as
metrics' values; a wider run, on request, takes each write on a fresh
object and fails on one refused on every object, a rule the checkers could
have applied. What each found is in `TODO.md` and `docs/LESSONS.md`.

(Its first form, 2026-10-15, judged both sides by the flow checker alone:
a core-backed Studio checks a load with the flow checker before forwarding
it, and the core's sandbox load runs none, so "both checkers agree" held
the flow checker to itself. Corrected the same day.)

It builds the core's service with the pinned toolchain, and fails, not
skips, without it.
"""

import itertools
import json
import os
import re

import pytest

from compare import raw
from test_publishing import OP, tee  # noqa: F401

HEAD = """module: bt
categories: [live, closed]
enumerations:
  Level: [HIGH, LOW]
types:
  B:
    description: A battery's object.
    tracking: record
    attributes:
      i: { type: int, optional: true }
      d2: { type: "decimal(10,2)", optional: true }
      d4: { type: "decimal(12,4)", optional: true }
      m: { type: money(USD), optional: true }
      du: { type: duration, optional: true }
      t: { type: timestamp, optional: true }
      dd: { type: date, optional: true }
      s: { type: string, optional: true }
      b: { type: bool, optional: true }
      lv: { type: Level, optional: true }
      tags: { type: "string[]", optional: true }
      who: { type: identity, optional: true }
    states: { OPEN: { category: live }, DONE: { category: closed, final: true } }
    derived_attributes:
"""
TAIL = """    transitions:
      open: { kind: initial, to: OPEN, optional_inputs: [i, d2, d4, m, du, t, dd, s, b, lv, tags, who] }
      close: { kind: external, from: [OPEN], to: DONE }
"""
ATOMS = ["i", "d2", "d4", "m", "du", "t", "dd", "s", "b", "lv", "tags", "who", "0",
         "2", "1.5", "2.255", '"a"', "true", "1 h", "USD 1.00", "Level.HIGH",
         "now", "length(s)", "date(t)", "time_in(OPEN)",
         "count(x in tags: true)", "state", "OPEN"]
BINARY = ["+", "-", "*", "/", "==", "!=", "<", "<=", ">", ">=", "and", "or",
          "in"]
OBJECTS = [
    {"i": 2, "d2": "10.00", "d4": "0.0050", "m": "USD 10.00", "du": "90 min",
     "t": "2026-01-01T00:00:00Z", "dd": "2026-01-01", "s": "ab", "b": True,
     "lv": "LOW", "tags": ["a", "b"], "who": "ann"},
    {"i": -3, "d2": "-0.05", "d4": "1.2345", "m": "USD 0.05", "du": "1 s",
     "t": "1999-12-31T23:59:59Z", "dd": "0999-06-15", "s": "", "b": False,
     "lv": "HIGH", "tags": [], "who": "x.y@z"},
    {"i": 0, "d2": "0.00", "d4": "0.0000", "m": "USD 0.00", "du": "0 s",
     "t": "2000-02-29T00:00:00Z", "dd": "2000-02-29"},
    {},
]


def expressions() -> list[str]:
    out = []
    for a in ATOMS:
        out += [a, f"-({a})" if a[0].isdigit() else f"-{a}", f"not {a}",
                f"{a} is null"]
    for a, b in itertools.product(ATOMS, ATOMS):
        out += [f"{a} {op} {b}" for op in BINARY]
        out.append(f"if b then {a} else {b}")
    return out


def flow(exprs: list[str]) -> str:
    return HEAD + "".join(
        f"      e{n}: {{ description: Probe., expression: {json.dumps(e)} }}\n"
        for n, e in enumerate(exprs)) + TAIL


def numbered(text: str, prefixes: str) -> dict[int, int]:
    """The line each item of a battery's module is declared on, as `e12`,
    `c12`, `g12`, `w12` or `m12`: its index, by the line's number."""
    out = {}
    for no, line in enumerate(text.split("\n"), start=1):
        name = line.strip().split(":", 1)[0]
        if name[:1] in prefixes and name[1:].isdigit():
            out[no] = int(name[1:])
    return out


def by_flow_checker(tee, text: str) -> set[int]:
    """The lines the flow checker refuses, through the runner's load."""
    r = raw(tee.runner, "POST", "/sandbox/load",
            {"files": [{"name": "bt.yaml", "text": text}]})[1]
    if r.get("loaded"):
        return set()
    return {f.get("line") for f in r.get("findings", [])}


def by_core_checker(tee, text: str) -> set[int]:
    """The lines the core's checker refuses, through the publish preview of
    a change whose source is the module: the path a version takes."""
    d = raw(tee.core, "POST", "/v1/request",
            {"actor": OP, "type": "DeclarationChange", "transition": "draft",
             "inputs": {"source": {"bt": text}, "source_ref": "battery"}})
    assert d[0] == 200 and d[1].get("object_id"), d
    p = raw(tee.core, "POST", "/v1/publish",
            {"actor": OP, "change": d[1]["object_id"], "dry_run": True})
    assert p[0] == 200, p
    lines = set()
    for f in (p[1].get("report") or {}).get("findings") or []:
        if f.get("severity") != "fatal" or not str(
                f.get("code", "")).startswith("check:"):
            continue
        m = re.match(r"bt:(\d+): ", f.get("message", ""))
        assert m, f"a fatal finding naming no line: {f}"
        lines.add(int(m.group(1)))
    return lines


def judge(tee, items: list, render, prefixes: str) -> tuple[list, list]:
    """The items both checkers accept, and those one alone refuses, judged
    to a fixed point, since a check that runs once the earlier pass (a
    guard that can never hold) finds more once others are gone."""
    raw(tee.core, "POST", "/sandbox/load",
        {"files": [{"name": "bt.yaml", "text": flow(["i"])}]})
    raw(tee.core, "POST", "/sandbox/reset", {})
    accepted, disagree, changed = list(items), [], True
    while changed:
        changed, kept = False, []
        for k in range(0, len(accepted), 300):
            batch = accepted[k:k + 300]
            text = render(batch)
            lines = numbered(text, prefixes)
            flow_lines = by_flow_checker(tee, text)
            core_lines = by_core_checker(tee, text)
            stray = (flow_lines | core_lines) - set(lines)
            assert not stray, f"a finding on no item's line: {sorted(stray)}"
            by_flow = {lines[n] for n in flow_lines}
            by_core = {lines[n] for n in core_lines}
            disagree += [(batch[n], "the core" if n in by_core
                          else "the flow checker")
                         for n in sorted(by_flow ^ by_core)]
            changed |= bool(by_flow | by_core)
            kept += [batch[n] for n in range(len(batch))
                     if n not in by_flow | by_core]
        accepted = kept
    return accepted, disagree


def reads(w, exprs: list[str]) -> list:
    raw(w, "POST", "/sandbox/load",
        {"files": [{"name": "bt.yaml", "text": flow(exprs)}]})
    raw(w, "POST", "/sandbox/reset", {})
    out = []
    for inputs in OBJECTS:
        o = raw(w, "POST", "/v1/request", {"actor": OP, "type": "B",
                                           "transition": "open",
                                           "inputs": inputs})
        oid = o[1].get("object_id")
        out.append(raw(w, "POST", "/v1/get", {"id": oid}) if oid else o)
    return out


def differing(tee, exprs: list[str], found: list) -> None:
    """Read a batch on both backends; where a read fails or they differ,
    split the batch down to the expressions that do."""
    core, runner = reads(tee.core, exprs), reads(tee.runner, exprs)
    bad = [k for k, (c, r) in enumerate(zip(core, runner))
           if c[0] != 200 or r[0] != 200 or c[1] != r[1] and
           c[1].get("attributes") != r[1].get("attributes")
           or c[1].get("refused") != r[1].get("refused")]
    if not bad:
        return
    if len(exprs) == 1:
        found.append((exprs[0], [(k, core[k], runner[k]) for k in bad]))
        return
    half = len(exprs) // 2
    differing(tee, exprs[:half], found)
    differing(tee, exprs[half:], found)


@pytest.fixture(scope="module")
def judged(tee):
    """The expressions both checkers accept as derived attributes, and those
    one alone refuses."""
    return judge(tee, expressions(), flow, "e")


def test_both_checkers_agree_and_every_accepted_expression_reads_alike(
        tee, judged):
    accepted, disagree = judged
    assert not disagree, f"refused by one checker alone: {disagree[:20]}"
    # the grammar reaches past the checkers' refusals: a battery that
    # accepted nothing would hold nothing
    assert len(accepted) > 1000, len(accepted)
    found = []
    for k in range(0, len(accepted), 40):
        differing(tee, accepted[k:k + 40], found)
    assert not found, "\n".join(
        f"{e}: object {k}: core {shown(c)} | runner {shown(r)}"
        for e, cases in found[:20] for k, c, r in cases[:1])


def shown(answer) -> str:
    """A read as a finding names it: the expression's value, or what the
    backend answered in its place."""
    status, body = answer
    if status == 200 and isinstance(body, dict) and "attributes" in body:
        return repr(body["attributes"].get("e0"))
    return f"{status} {str(body)[:140]}"



def guarded(exprs: list[str]) -> str:
    """Each expression a condition guarding a transition of its own."""
    base = HEAD[:HEAD.index("    derived_attributes:\n")]
    conditions = "".join(
        f"      c{n}: {{ description: Probe., expression: {json.dumps(e)}, "
        "remedy: dependent }\n" for n, e in enumerate(exprs))
    guards = "".join(
        f"      g{n}: {{ kind: internal, from: [OPEN], "
        f"guards: {{ c{n}: deny }} }}\n"
        for n in range(len(exprs)))
    return base + "    conditions:\n" + conditions + TAIL + guards


def offers(w, exprs: list[str]) -> list:
    raw(w, "POST", "/sandbox/load",
        {"files": [{"name": "bt.yaml", "text": guarded(exprs)}]})
    raw(w, "POST", "/sandbox/reset", {})
    out = []
    for inputs in OBJECTS:
        o = raw(w, "POST", "/v1/request", {"actor": OP, "type": "B",
                                           "transition": "open",
                                           "inputs": inputs})
        a = raw(w, "POST", "/v1/availability",
                {"actor": OP, "id": o[1].get("object_id")})
        out.append(a)
    return out


def test_every_accepted_condition_guards_alike(tee):
    """Each expression as a condition guarding a transition: both checkers
    judge it alike, and each one both accept gives each object's offer of
    its transition alike on both backends, unknown with its remedy (§8.2)."""
    accepted, disagree = judge(tee, expressions(), guarded, "cg")
    assert not disagree, f"refused by one checker alone: {disagree[:20]}"
    assert len(accepted) > 500, len(accepted)
    found = []
    for k in range(0, len(accepted), 40):
        batch = accepted[k:k + 40]
        core, runner = offers(tee.core, batch), offers(tee.runner, batch)
        for n_obj, (c, r) in enumerate(zip(core, runner)):
            if c[0] != 200 or r[0] != 200:
                found.append(f"{batch[0]}…: object {n_obj}: core {c[0]} "
                             f"{str(c[1])[:120]} | runner {r[0]} "
                             f"{str(r[1])[:120]}")
                continue
            by_core = {x["transition"]: x for x in c[1]}
            by_runner = {x["transition"]: x for x in r[1]}
            for n in range(len(batch)):
                a = comparable(by_core.get(f"g{n}"))
                b = comparable(by_runner.get(f"g{n}"))
                if a != b:
                    found.append(f"{batch[n]}: object {n_obj}: core {a} | "
                                 f"runner {b}")
    assert not found, f"{len(found)} guards differ:\n" + "\n".join(
        found[:20])


def comparable(offer):
    """An offer as both backends answer it, without what one leaves not
    computed."""
    if not isinstance(offer, dict):
        return offer
    out = {k: v for k, v in offer.items() if k != "not_computed"}
    if isinstance(out.get("verdict"), dict):
        out["verdict"] = {k: v for k, v in out["verdict"].items()
                          if k not in ("consulted", "not_computed")}
    return out



TARGETS = ["i", "d2", "d4", "m", "du", "t", "dd", "s", "b", "lv", "tags",
           "who"]


def writing(writes: list[tuple[str, str]]) -> str:
    """Each write a transition of its own, assigning its expression to its
    attribute."""
    base = HEAD[:HEAD.index("    derived_attributes:\n")]
    steps = "".join(
        f"      w{n}: {{ kind: internal, from: [OPEN], effect: [ {{ assign: "
        f"{{ location: {loc}, expr: {json.dumps(e)} }} }} ] }}\n"
        for n, (loc, e) in enumerate(writes))
    return base + TAIL + steps


def written(w, writes: list[tuple[str, str]]) -> list:
    """For each object, each write's verdict in turn, then the object."""
    raw(w, "POST", "/sandbox/load",
        {"files": [{"name": "bt.yaml", "text": writing(writes)}]})
    raw(w, "POST", "/sandbox/reset", {})
    out = []
    for inputs in OBJECTS:
        o = raw(w, "POST", "/v1/request", {"actor": OP, "type": "B",
                                           "transition": "open",
                                           "inputs": inputs})
        oid = o[1].get("object_id")
        verdicts = []
        for n in range(len(writes)):
            v = raw(w, "POST", "/v1/request", {"actor": OP, "object_id": oid,
                                               "transition": f"w{n}"})
            verdicts.append((v[0], {k: x for k, x in v[1].items()
                                    if k in ("kind", "clause", "remedy",
                                             "objects", "fault")}))
        g = raw(w, "POST", "/v1/get", {"id": oid})
        out.append((verdicts, g[0], g[1].get("attributes", g[1])))
    return out


def write_differing(tee, writes, found):
    core, runner = written(tee.core, writes), written(tee.runner, writes)
    if core == runner:
        return
    if len(writes) == 1:
        loc, e = writes[0]
        k = next(k for k, (c, r) in enumerate(zip(core, runner)) if c != r)
        found.append(f"{loc} := {e}: object {k}: core {core[k][0]} "
                     f"{str(core[k][2])[:160]} | runner {runner[k][0]} "
                     f"{str(runner[k][2])[:160]}")
        return
    half = len(writes) // 2
    write_differing(tee, writes[:half], found)
    write_differing(tee, writes[half:], found)


@pytest.fixture(scope="module")
def writes(tee, judged):
    """Each accepted expression assigned to each attribute, judged by both
    checkers to a fixed point: those both accept, and those one alone
    refuses."""
    accepted_exprs, _ = judged
    return judge(tee, [(loc, e) for e in accepted_exprs for loc in TARGETS],
                 writing, "w")


def test_every_accepted_write_writes_alike(tee, writes):
    """Each accepted expression assigned to each attribute: both checkers
    judge the write alike (check 17's fit, check 24), and each one both
    accept, taken in turn on each object, is answered and leaves the object
    alike on both backends: a value at its target's scale, a quotient
    divided once there, a write that would round or overflow refused
    `value_in_range` (§8.3, ADR-0205). Its first run found a set written a
    scalar and a state written an amount, which the checkers accepted, the
    core refused and the runner held."""
    accepted, disagree = writes
    assert not disagree, f"refused by one checker alone: {disagree[:20]}"
    assert len(accepted) > 500, len(accepted)
    found = []
    for k in range(0, len(accepted), 40):
        write_differing(tee, accepted[k:k + 40], found)
    assert not found, f"{len(found)} writes differ:\n" + "\n".join(found[:20])


@pytest.mark.skipif(not os.environ.get("OBJECTFLOW_BATTERY_WIDE"),
                    reason="about six minutes; set OBJECTFLOW_BATTERY_WIDE=1")
def test_no_accepted_write_is_refused_on_every_object(tee, writes):
    """Each accepted write on a fresh object of each kind, the clock moved a
    day on, so a time in a state is a whole day: alike on both backends, and none refused `value_in_range` on
    every object, which would be a rule the checkers could have applied at
    publish. Its first run found a zero divisor, a date moved by part of a
    day and a literal's places lost beside an int, each now refused at
    publish (the differential battery, 2026-10-15)."""
    accepted, _ = writes
    found, everywhere = [], []
    for k in range(0, len(accepted), 40):
        batch = accepted[k:k + 40]
        answers = {}
        for side, w in (("core", tee.core), ("runner", tee.runner)):
            raw(w, "POST", "/sandbox/load",
                {"files": [{"name": "bt.yaml", "text": writing(batch)}]})
            raw(w, "POST", "/sandbox/reset", {})
            made = [[raw(w, "POST", "/v1/request",
                         {"actor": OP, "type": "B", "transition": "open",
                          "inputs": inputs})[1].get("object_id")
                     for inputs in OBJECTS] for _ in batch]
            raw(w, "POST", "/sandbox/clock", {"advance": "1 day"})
            answers[side] = [
                [(raw(w, "POST", "/v1/request",
                      {"actor": OP, "object_id": oid,
                       "transition": f"w{n}"})[1].get("clause"),
                  raw(w, "POST", "/v1/get", {"id": oid})[1].get(
                      "attributes", {}).get(loc))
                 for oid in made[n]]
                for n, (loc, _e) in enumerate(batch)]
        for n, (loc, e) in enumerate(batch):
            core, runner = answers["core"][n], answers["runner"][n]
            if core != runner:
                found.append(f"{loc} := {e}: core {core} | runner {runner}")
            if all(clause == "value_in_range" for clause, _ in core):
                everywhere.append(f"{loc} := {e}")
    assert not found, f"{len(found)} writes differ:\n" + "\n".join(
        found[:20])
    assert not everywhere, (f"{len(everywhere)} refused on every object: "
                            f"{everywhere[:20]}")


def metric_values() -> list[str]:
    """A metric's values over the objects' rows: each aggregate of each
    attribute and the counts, each with a literal, and each pair of the
    numeric attributes' aggregates."""
    attrs = ["i", "d2", "d4", "m", "du", "t", "dd", "s", "b", "lv", "tags"]
    aggregates = [f"{fn}(o.{a})" for fn in ("sum", "avg", "min", "max",
                                            "median") for a in attrs]
    aggregates += ["count()", "count(where o.b)", "count(distinct o.s)",
                   "percentile(0.5, o.d2)"]
    numeric = [f"{fn}(o.{a})" for fn in ("sum", "avg", "min", "max",
                                         "median")
               for a in ("i", "d2", "d4", "m", "du")] + ["count()"]
    out = list(aggregates)
    for a, lit, op in itertools.product(aggregates, ["2", "1.5", "1.000"],
                                        ["+", "-", "*", "/"]):
        out.append(f"{a} {op} {lit}")
    for a, b, op in itertools.product(numeric, numeric, ["+", "-", "*", "/"]):
        out.append(f"{a} {op} {b}")
    # a date's and a time's difference, and each moved by a duration
    out += ["max(o.dd) - min(o.dd)", "min(o.dd) + 1 day",
            "max(o.t) - min(o.t)", "min(o.t) + 1 h"]
    return out


def measuring(values: list[str]) -> str:
    """Each value a metric of its own over the type's objects."""
    base = HEAD[:HEAD.index("    derived_attributes:\n")]
    metrics = "".join(
        f"      m{n}: {{ description: Probe., source: objects, item: o, "
        f"expression: {json.dumps(v)} }}\n" for n, v in enumerate(values))
    return base + TAIL + "    metrics:\n" + metrics


def test_every_accepted_metric_reads_alike(tee):
    """Each metric value both checkers accept (its form, its aggregates'
    bodies and its arithmetic, M2), read over the four objects on both
    backends: alike, rounded to its type's scale (ADR-0128)."""
    accepted, disagree = judge(tee, metric_values(), measuring, "m")
    assert not disagree, f"refused by one checker alone: {disagree[:20]}"
    assert len(accepted) > 300, len(accepted)
    found = []
    for k in range(0, len(accepted), 40):
        batch = accepted[k:k + 40]
        pages = {}
        for side, w in (("core", tee.core), ("runner", tee.runner)):
            raw(w, "POST", "/sandbox/load",
                {"files": [{"name": "bt.yaml", "text": measuring(batch)}]})
            raw(w, "POST", "/sandbox/reset", {})
            for inputs in OBJECTS:
                raw(w, "POST", "/v1/request", {"actor": OP, "type": "B",
                                               "transition": "open",
                                               "inputs": inputs})
            pages[side] = [raw(w, "POST", "/v1/metric",
                               {"actor": OP, "name": f"B.m{n}"})
                           for n in range(len(batch))]
        for n, v in enumerate(batch):
            c, r = pages["core"][n], pages["runner"][n]
            if c[0] != r[0] or c[1].get("rows") != r[1].get("rows") or \
                    c[1].get("refused") != r[1].get("refused"):
                found.append(f"{v}: core {c[0]} {str(c[1])[:160]} | "
                             f"runner {r[0]} {str(r[1])[:160]}")
    assert not found, f"{len(found)} metrics differ:\n" + "\n".join(
        found[:20])


INPUT_VALUES = [
    None, True, False, 0, 1, -1, 2**53, 2**63 - 1, 2**63, -2**63 - 1, 1.5,
    1e300, "", " ", "a", "10", "10.00", "-0.05", "1.555", "10.0000000000",
    "1e3", "USD 10.00", "USD 10.001", "EUR 1.00", "USD10.00", "90 min",
    "1.5 s", "1 h", "0 s", "2026-01-01", "0999-06-15", "2026-02-30",
    "2026-01-01T00:00:00Z", "2026-01-01T00:00:00.5Z", "10000-01-01T00:00:00Z",
    "2026-01-01T00:00:00+08:00", "LOW", "low", "HIGH", "NOPE", [], ["a"],
    ["a", "a"], [1], [None], ["a", None], [1, None], {}, {"a": 1},
    "x" * 300,
]


def test_every_input_value_is_taken_or_refused_alike(tee):
    """Each JSON value given as each attribute's input on a creation, on
    both backends: taken or refused alike, the object it makes read alike,
    a malformed value refused rather than held (the wire's boundary,
    `wire.md`'s inputs)."""
    base = HEAD[:HEAD.index("    derived_attributes:\n")]
    for w in (tee.core, tee.runner):
        raw(w, "POST", "/sandbox/load",
            {"files": [{"name": "bt.yaml", "text": base + TAIL}]})
        raw(w, "POST", "/sandbox/reset", {})
    found = []
    for attr, value in itertools.product(TARGETS, INPUT_VALUES):
        answers = []
        for w in (tee.core, tee.runner):
            v = raw(w, "POST", "/v1/request",
                    {"actor": OP, "type": "B", "transition": "open",
                     "inputs": {attr: value}})
            oid = v[1].get("object_id") if isinstance(v[1], dict) else None
            held = raw(w, "POST", "/v1/get", {"id": oid})[1].get(
                "attributes", {}).get(attr) if oid else None
            verdict = v[1].get("kind") if isinstance(v[1], dict) else None
            clause = v[1].get("clause") if isinstance(v[1], dict) else None
            answers.append((v[0], verdict, clause, held))
        if answers[0] != answers[1]:
            found.append(f"{attr} := {value!r}: core {answers[0]} | runner "
                         f"{answers[1]}")
    assert not found, f"{len(found)} inputs differ:\n" + "\n".join(found[:30])


CREATED = """  U:
    description: A thing a creation makes.
    tracking: record
    attributes:
      i: { type: int, optional: true }
      d2: { type: "decimal(10,2)", optional: true }
      d4: { type: "decimal(12,4)", optional: true }
      m: { type: money(USD), optional: true }
      du: { type: duration, optional: true }
      t: { type: timestamp, optional: true }
      dd: { type: date, optional: true }
      s: { type: string, optional: true }
      b: { type: bool, optional: true }
      lv: { type: Level, optional: true }
      tags: { type: "string[]", optional: true }
    states: { NEW: { category: live }, DONE: { category: closed, final: true } }
    transitions:
      make: { kind: initial, to: NEW, optional_inputs: [i, d2, d4, m, du, t, dd, s, b, lv, tags] }
      close: { kind: external, from: [NEW], to: DONE }
"""


def creating(items: list[tuple[str, str]]) -> str:
    """Each argument a creation of its own, passing its expression to the
    created type's input of the attribute's name."""
    base = HEAD[:HEAD.index("    derived_attributes:\n")]
    steps = "".join(
        f"      w{n}: {{ kind: internal, from: [OPEN], effect: [ {{ create: "
        f"{{ type: U, transition: make, inputs: {{ {loc}: {json.dumps(e)} }} "
        f"}} }} ] }}\n" for n, (loc, e) in enumerate(items))
    return base + TAIL + steps + CREATED


@pytest.mark.skipif(not os.environ.get("OBJECTFLOW_BATTERY_WIDE"),
                    reason="about eleven minutes; set OBJECTFLOW_BATTERY_WIDE=1")
def test_every_accepted_argument_creates_alike(tee, judged):
    """Each accepted expression passed as each input of a creation: both
    checkers judge the argument alike (check 2's fit), and each one both
    accept, taken on a fresh object of each kind, is answered alike on both
    backends. Its first run found an int passed to a time, an amount or a
    duration input, and an amount to a decimal or an int, which the checkers
    took, the core refused `call_refused` and the runner, holding them all
    as numbers, satisfied; the checkers now type an argument as they type an
    expression (the differential battery, 2026-10-16)."""
    exprs, _ = judged
    accepted, disagree = judge(
        tee, [(loc, e) for e in exprs for loc in TARGETS], creating, "w")
    assert not disagree, f"refused by one checker alone: {disagree[:20]}"
    assert len(accepted) > 500, len(accepted)
    found = []
    for k in range(0, len(accepted), 40):
        batch = accepted[k:k + 40]
        for w in (tee.core, tee.runner):
            raw(w, "POST", "/sandbox/load",
                {"files": [{"name": "bt.yaml", "text": creating(batch)}]})
            raw(w, "POST", "/sandbox/reset", {})
        for n, (loc, e) in enumerate(batch):
            for j, inputs in enumerate(OBJECTS):
                got = []
                for w in (tee.core, tee.runner):
                    oid = raw(w, "POST", "/v1/request", {
                        "actor": OP, "type": "B", "transition": "open",
                        "inputs": inputs})[1].get("object_id")
                    v = raw(w, "POST", "/v1/request", {
                        "actor": OP, "object_id": oid, "transition": f"w{n}"})
                    got.append((v[0], {x: y for x, y in v[1].items() if x in (
                        "kind", "clause", "remedy", "fault")}))
                if got[0] != got[1]:
                    found.append(f"{loc} <- {e}: object {j}: core {got[0]} | "
                                 f"runner {got[1]}")
    assert not found, f"{len(found)} creations differ:\n" + "\n".join(
        found[:20])

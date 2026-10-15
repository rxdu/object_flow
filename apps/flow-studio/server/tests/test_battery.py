"""The differential battery (ADR-0204 §4's gate over expressions): every
expression a grammar makes over typed attributes, literals and the
language's functions is checked by both checkers, which must agree, and each
one both accept is read on the core and the reference runner over four
objects, an ordinary one, one of negatives, ties and a year below 1000, one
of zeros and one with nothing set, which must answer alike. Its first run,
2026-10-15, found a set of values untyped, an order over states, an integer
0 read as the zero of a duration it multiplied, a date moved by part of a
day read as null, a date's difference written as a number and an `if`'s
literal 0 written as an integer, each fixed (`docs/LESSONS.md`). The same
expressions as conditions, each guarding a transition, are judged alike and
give each object's offer alike, unknown with its remedy; the sandbox serves
no `query`, so a filter's reading is the core's own tests'. Each accepted
expression assigned to each attribute is judged alike and writes alike; a
wider run, on request, takes each write on a fresh object and fails on one
refused on every object, a rule the checkers could have applied.

It builds the core's service with the pinned toolchain, and fails, not
skips, without it.
"""

import itertools
import json
import os

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
    states: { OPEN: { category: live }, DONE: { category: closed, final: true } }
    derived_attributes:
"""
TAIL = """    transitions:
      open: { kind: initial, to: OPEN, optional_inputs: [i, d2, d4, m, du, t, dd, s, b, lv, tags] }
      close: { kind: external, from: [OPEN], to: DONE }
"""
ATOMS = ["i", "d2", "d4", "m", "du", "t", "dd", "s", "b", "lv", "tags", "0",
         "2", "1.5", "2.255", '"a"', "true", "1 h", "USD 1.00", "Level.HIGH",
         "now", "length(s)", "date(t)", "time_in(OPEN)",
         "count(x in tags: true)", "state", "OPEN"]
BINARY = ["+", "-", "*", "/", "==", "!=", "<", "<=", ">", ">=", "and", "or",
          "in"]
OBJECTS = [
    {"i": 2, "d2": "10.00", "d4": "0.0050", "m": "USD 10.00", "du": "90 min",
     "t": "2026-01-01T00:00:00Z", "dd": "2026-01-01", "s": "ab", "b": True,
     "lv": "LOW", "tags": ["a", "b"]},
    {"i": -3, "d2": "-0.05", "d4": "1.2345", "m": "USD 0.05", "du": "1 s",
     "t": "1999-12-31T23:59:59Z", "dd": "0999-06-15", "s": "", "b": False,
     "lv": "HIGH", "tags": []},
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


def refused(w, exprs: list[str]) -> set[int]:
    """The expressions a backend's checker refuses, by the line each is
    declared on; a finding on any other line fails the battery itself."""
    first = HEAD.count("\n") + 1
    r = raw(w, "POST", "/sandbox/load",
            {"files": [{"name": "bt.yaml", "text": flow(exprs)}]})[1]
    if r.get("loaded"):
        return set()
    findings = r.get("findings", [])
    stray = [f for f in findings
             if not first <= f.get("line", 0) < first + len(exprs)]
    assert not stray, f"a finding on no expression's line: {stray[:3]}"
    lines = {f.get("line") for f in findings}
    return {n for n in range(len(exprs)) if first + n in lines}


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
    """The expressions both checkers accept, and those one alone refuses."""
    exprs = expressions()
    accepted, disagree = [], []
    for k in range(0, len(exprs), 300):
        batch = exprs[k:k + 300]
        by_core, by_runner = refused(tee.core, batch), refused(tee.runner,
                                                               batch)
        disagree += [(batch[n], "the core" if n in by_core else "the runner")
                     for n in sorted(by_core ^ by_runner)]
        accepted += [batch[n] for n in range(len(batch))
                     if n not in by_core | by_runner]
    return accepted, disagree


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


def guard_refused(w, exprs: list[str]) -> set[int]:
    """The expressions a backend's checker refuses as conditions, by the
    line of the condition or of the transition it guards, which a guard
    that can never hold is found on."""
    text = guarded(exprs)
    by_line = {}
    for no, line in enumerate(text.split("\n"), start=1):
        name = line.strip().split(":", 1)[0]
        if name[:1] in ("c", "g") and name[1:].isdigit():
            by_line[no] = int(name[1:])
    r = raw(w, "POST", "/sandbox/load",
            {"files": [{"name": "bt.yaml", "text": text}]})[1]
    if r.get("loaded"):
        return set()
    findings = r.get("findings", [])
    stray = [f for f in findings if f.get("line") not in by_line]
    assert not stray, f"a finding on no condition's line: {stray[:3]}"
    return {by_line[f["line"]] for f in findings}


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
    # judged to a fixed point: a check that runs once the earlier pass, a
    # guard that can never hold, finds more in a batch the earlier refused
    accepted, disagree, changed = expressions(), [], True
    while changed:
        changed, kept = False, []
        for k in range(0, len(accepted), 300):
            batch = accepted[k:k + 300]
            by_core = guard_refused(tee.core, batch)
            by_runner = guard_refused(tee.runner, batch)
            disagree += [(batch[n], "the core" if n in by_core
                          else "the runner")
                         for n in sorted(by_core ^ by_runner)]
            changed |= bool(by_core | by_runner)
            kept += [batch[n] for n in range(len(batch))
                     if n not in by_core | by_runner]
        accepted = kept
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



TARGETS = ["i", "d2", "d4", "m", "du", "t", "dd", "s", "b", "lv", "tags"]


def writing(writes: list[tuple[str, str]]) -> str:
    """Each write a transition of its own, assigning its expression to its
    attribute."""
    base = HEAD[:HEAD.index("    derived_attributes:\n")]
    steps = "".join(
        f"      w{n}: {{ kind: internal, from: [OPEN], effect: [ {{ assign: "
        f"{{ location: {loc}, expr: {json.dumps(e)} }} }} ] }}\n"
        for n, (loc, e) in enumerate(writes))
    return base + TAIL + steps


def write_refused(w, writes: list[tuple[str, str]]) -> set[int]:
    text = writing(writes)
    by_line = {no: int(line.strip().split(":", 1)[0][1:])
               for no, line in enumerate(text.split("\n"), start=1)
               if line.strip().split(":", 1)[0][:1] == "w"
               and line.strip().split(":", 1)[0][1:].isdigit()}
    r = raw(w, "POST", "/sandbox/load",
            {"files": [{"name": "bt.yaml", "text": text}]})[1]
    if r.get("loaded"):
        return set()
    findings = r.get("findings", [])
    stray = [f for f in findings if f.get("line") not in by_line]
    assert not stray, f"a finding on no write's line: {stray[:3]}"
    return {by_line[f["line"]] for f in findings}


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
    accepted = [(loc, e) for e in accepted_exprs for loc in TARGETS]
    disagree, changed = [], True
    while changed:
        changed, kept = False, []
        for k in range(0, len(accepted), 300):
            batch = accepted[k:k + 300]
            by_core = write_refused(tee.core, batch)
            by_runner = write_refused(tee.runner, batch)
            disagree += [(batch[n], "the core" if n in by_core
                          else "the runner")
                         for n in sorted(by_core ^ by_runner)]
            changed |= bool(by_core | by_runner)
            kept += [batch[n] for n in range(len(batch))
                     if n not in by_core | by_runner]
        accepted = kept
    return accepted, disagree


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

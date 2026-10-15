"""The differential battery (ADR-0204 §4's gate over expressions): every
expression a grammar makes over typed attributes, literals and the
language's functions is checked by both checkers, which must agree, and each
one both accept is read on the core and the reference runner over four
objects, an ordinary one, one of negatives, ties and a year below 1000, one
of zeros and one with nothing set, which must answer alike. Its first run,
2026-10-15, found a set of values untyped, an order over states, an integer
0 read as the zero of a duration it multiplied, a date moved by part of a
day read as null, a date's difference written as a number and an `if`'s
literal 0 written as an integer, each fixed (`docs/LESSONS.md`).

It builds the core's service with the pinned toolchain, and fails, not
skips, without it.
"""

import itertools
import json

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
    {"i": 0, "d2": "0.00", "d4": "0.0000", "m": "USD 0.00", "du": "0 s"},
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


def test_both_checkers_agree_and_every_accepted_expression_reads_alike(tee):
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

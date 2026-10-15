"""Values written and arithmetic past its bound, held alike on the core and
the reference runner (ADR-0205 decisions 1 and 2): every answer compared
whole by the tee, a difference failing the test as a finding against one of
them.

It builds the core's service with the pinned toolchain, and fails, not skips,
without it.
"""

import pytest

from compare import Tee
from flowstudio.core import Core
from test_backends_agree import started
from test_publishing import alike, load, made, took

OP = {"id": "operator"}

VALS = """module: vals
categories: [live, closed]
types:
  Item:
    description: Values held at their types.
    tracking: record
    attributes:
      d2: { type: "decimal(10,2)", default: 5 }
      d4: { type: "decimal(12,4)", optional: true }
      n: { type: int, optional: true }
      label: { type: string, optional: true }
      tags: { type: "string[]", optional: true }
      huge: { type: int, optional: true }
    states:
      OPEN: { category: live, description: Open. }
      DONE: { category: closed, final: true, description: Done. }
    derived_attributes:
      twice: { description: Twice huge., expression: huge * 2 }
      has_z: { description: Whether a tag is z., expression: 'any(t in tags: t == "z")' }
      not_z: { description: How many tags are not z., expression: 'count(t in tags: t != "z")' }
    conditions:
      decided: { description: Decided on its left., expression: label is null or huge * 2 > 0, remedy: unreachable_from_here }
    transitions:
      open: { kind: initial, to: OPEN, optional_inputs: [d2] }
      close: { kind: external, from: [OPEN], to: DONE }
      third:
        kind: internal
        from: OPEN
        effect:
          - assign: { location: d4, expr: d2 / 3 }
      count:
        kind: internal
        from: OPEN
        inputs: { k: { type: int } }
        effect:
          - assign: { location: d2, expr: inputs.k }
      tag:
        kind: internal
        from: OPEN
        effect:
          - add: { location: tags, expr: label }
      tag_as:
        kind: internal
        from: OPEN
        inputs: { t: { type: string } }
        effect:
          - add: { location: tags, expr: inputs.t }
      grow:
        kind: internal
        from: OPEN
        inputs: { h: { type: int } }
        effect:
          - assign: { location: huge, expr: inputs.h }
      poke: { kind: internal, from: OPEN, guards: { decided: deny } }
      sweep:
        kind: internal
        from: OPEN
        effect:
          - foreach:
              item: x
              array: "{1, 2, -3}"
              where: "x * 4611686018427387904 > 0"
              limit: 5
              steps:
                - assign: { location: n, expr: x }
"""


@pytest.fixture(scope="module")
def tee():
    core = Core()
    behind, core_url = started(core)
    runner, runner_url = started()
    tee = Tee(core_url, runner_url)
    tee.module = "arithmetic"
    yield tee
    for server in (behind, runner):
        server.shutdown()
        server.server_close()
    core.close()


def refused(tee, oid: str, xn: str, **inputs) -> None:
    v = tee.post("/v1/request", {"transition": xn, "actor": OP,
                                 "object_id": oid, "inputs": inputs})
    assert (v["kind"], v.get("clause")) == ("unsatisfied", "value_in_range"), \
        f"{xn}: {v}"
    assert v.get("objects") == [oid], f"{xn}: {v}"


def test_values_are_written_and_refused_alike(tee):
    """An integer written to a decimal at its scale and a quotient divided
    once at its target's; an absent element and an arithmetic past its bound
    refused `value_in_range`, and a read of a derived value past its bound
    refused, but not one on a side that decides nothing; on both backends
    (the audit's values W1, W4, W5, W6, U1 and U4), and the body of `any` and
    `count` a condition (G1). A value of another type is
    refused at publish, so the core's own tests hold its refusal at run
    time."""
    load(tee, "vals", VALS)
    item = made(tee, "Item", "open")
    assert tee.post("/v1/get", {"id": item})["attributes"]["d2"] == "5.00"
    took(tee, item, "third")
    assert tee.post("/v1/get", {"id": item})["attributes"]["d4"] == "1.6667"
    took(tee, item, "count", k=7)
    assert tee.post("/v1/get", {"id": item})["attributes"]["d2"] == "7.00"
    # a write that would round its decimal is refused at publish now that
    # the checkers type a literal's element (K-10), where `max(x in {d4}: x)`
    # into a decimal(10,2) passed them and was refused only at run time
    # (W5); the core's own test holds that refusal
    # (`request.rs`, a plain write that would round its decimal)
    refused(tee, item, "tag")
    took(tee, item, "tag_as", t="a")
    assert tee.post("/v1/get", {"id": item})["attributes"]["has_z"] is False
    took(tee, item, "tag_as", t="z")
    got = tee.post("/v1/get", {"id": item})["attributes"]
    assert (got["has_z"], got["not_z"]) == (True, 1), got
    took(tee, item, "tag_as", t="b")
    refused(tee, item, "sweep")
    took(tee, item, "grow", h=2**63 - 1)
    got = tee.post("/v1/get", {"id": item})
    assert (got.get("refused"), got.get("object_id")) == \
        ("value_in_range", item), got
    took(tee, item, "poke")
    alike(tee)


QUOTIENTS = """module: quotients
categories: [live, closed]
types:
  Share:
    description: Quotients held as the evaluator holds them.
    tracking: record
    attributes:
      d2: { type: "decimal(10,2)", optional: true }
      d4: { type: "decimal(12,4)", optional: true }
      m: { type: money(USD), optional: true }
      du: { type: duration, optional: true }
    states:
      OPEN: { category: live, description: Open. }
      DONE: { category: closed, final: true, description: Done. }
    derived_attributes:
      third: { description: A third., expression: d2 / 3 }
      half: { description: A half., expression: d2 / 2 }
      twice: { description: A third twice., expression: d2 / 3 * 2 }
      sum_third: { description: A sum's third., expression: (d2 + 1) / 3 }
      plus: { description: A sum., expression: d2 + 3 }
      wide: { description: A literal of more places., expression: 1.555 / d2 }
      m_third: { description: An amount's third., expression: m / 3 }
      m_times: { description: An amount scaled., expression: m * 1.005 }
      m_ratio: { description: Two amounts' ratio., expression: m / m }
      du_seventh: { description: A duration's seventh., expression: du / 7 }
    transitions:
      open: { kind: initial, to: OPEN, optional_inputs: [d2, m, du] }
      close: { kind: external, from: [OPEN], to: DONE }
      share:
        kind: internal
        from: OPEN
        effect:
          - assign: { location: d4, expr: d2 / 3 }
          - assign: { location: d2, expr: d2 / 3 + 1 }
"""


@pytest.mark.parametrize("d2, read, written", [
    ("10.00",
     {"third": "3.33", "half": "5.00", "twice": "6.66", "sum_third": "3.67",
      "plus": "13.00", "wide": "0.156"},
     {"d4": "3.3333", "d2": "4.33"}),
    # a tie rounds to the even neighbour, below zero as above it
    ("0.05", {"half": "0.02", "third": "0.02"}, {"d4": "0.0167", "d2": "1.02"}),
    ("-0.05", {"half": "-0.02", "third": "-0.02"},
     {"d4": "-0.0167", "d2": "0.98"}),
])
def test_a_quotient_is_rounded_where_the_evaluator_rounds_it_alike(
        tee, d2, read, written):
    """A quotient at its dividend's scale, an amount at its currency's minor
    unit and a duration at whole seconds, each half to even, a product of
    numbers exact and a sum at the larger scale, and a quotient written to a
    decimal divided once at its target's; where the reference runner held
    every quotient exact, so a derived one that did not terminate failed its
    read `500` and `d2 / 3 + 1` was refused as a write that rounds (the
    audit's values R1 and R7, declaration-syntax.md §8.3)."""
    load(tee, "quotients", QUOTIENTS)
    share = made(tee, "Share", "open", d2=d2, m="USD 10.00", du="90 min")
    got = tee.post("/v1/get", {"id": share})["attributes"]
    assert {k: got[k] for k in read} == read, got
    assert (got["m_third"], got["m_times"], got["m_ratio"], got["du_seventh"]) \
        == ("USD 3.33", "USD 10.05", "1.000000", "771 s"), got
    took(tee, share, "share")
    got = tee.post("/v1/get", {"id": share})["attributes"]
    assert {k: got[k] for k in written} == written, got
    alike(tee)


BOUNDED = """module: bounded
categories: [live, closed]
types:
  Span:
    description: A value past what its type holds.
    tracking: record
    attributes:
      d18: { type: "decimal(18,2)", optional: true }
      d9: { type: "decimal(18,9)", optional: true }
      tiny: { type: "decimal(18,9)", optional: true }
      m: { type: money(USD), optional: true }
      du: { type: duration, optional: true }
      t: { type: timestamp, optional: true }
    states:
      OPEN: { category: live, description: Open. }
      DONE: { category: closed, final: true, description: Done. }
    derived_attributes:
      past: { description: Past its bound., expression: '%s' }
    transitions:
      open: { kind: initial, to: OPEN, optional_inputs: [d18, d9, tiny, m, du, t] }
      close: { kind: external, from: [OPEN], to: DONE }
"""


@pytest.mark.parametrize("expr", [
    "du * 9223372036854775807",
    "du * 4611686018427387904 + du * 4611686018427387904",
    "t + du * 4611686018427387904",
    "d18 * d18 * d18",
    "d9 * d9 * d9 * d9 * d9",
    # 45 places and a single unit, past the places a decimal holds
    "tiny * tiny * tiny * tiny * tiny",
    "m * 9223372036854775807 * 9223372036854775807",
])
def test_a_value_past_what_its_type_holds_is_refused_alike(tee, expr):
    """A duration or a time past 64 bits of seconds, and a decimal or an
    amount past 128 bits of units or 38 places, refuse the read that derives
    it `value_in_range`, where the reference runner answered it (the audit's
    values R5, ADR-0205 decision 2)."""
    tee.found.clear()
    load(tee, "bounded", BOUNDED % expr)
    span = made(tee, "Span", "open", d18="9999999999999999.99",
                d9="123456789.123456789", tiny="0.000000001",
                m="USD 10.00", du="90 min",
                t="2026-01-01T00:00:00Z")
    got = tee.post("/v1/get", {"id": span})
    assert (got.get("refused"), got.get("object_id")) == \
        ("value_in_range", span), got
    alike(tee)

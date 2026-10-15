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

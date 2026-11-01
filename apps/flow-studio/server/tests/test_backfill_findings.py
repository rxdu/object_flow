"""A backfill writing a value its attribute cannot hold refuses the publish
whatever order its objects are met in, through the studio's server on the
reference runner, as the core's does: where it fitted the first object met,
the notice naming the objects it writes kept the finding a notice (TODO.md,
"A backfill that fits its first object hides a later object's value it cannot
hold").
"""

import pytest

OP = {"id": "operator"}

LEDGER = """module: ledger
categories: [live, closed]
types:
  Entry:
    description: An entry of the ledger.
    tracking: record
    attributes:
      n: { type: int, optional: true }
      d: { type: "decimal(4,2)", optional: true }
    states:
      OPEN: { category: live }
      SHUT: { category: closed, final: true }
    transitions:
      open: { kind: initial, to: OPEN, optional_inputs: [n] }
      shut: { kind: external, from: [OPEN], to: SHUT }
"""

BACKFILLED = LEDGER + """migration:
  description: Each entry's d from its n.
  backfill:
    Entry: { d: n }
"""


@pytest.mark.parametrize("ns", [(1, 1_000_000), (1_000_000, 1)])
def test_a_backfill_it_cannot_write_is_fatal_in_either_order(wire, ns):
    assert wire.post("/sandbox/load", {"files": [
        {"name": "ledger.yaml", "text": LEDGER}]})["loaded"]
    wire.post("/sandbox/reset", {})
    for n in ns:
        v = wire.post("/v1/request", {"type": "Entry", "transition": "open",
                                      "actor": OP, "inputs": {"n": n}})
        assert v["kind"] == "satisfied", v
    v = wire.post("/v1/request", {
        "type": "DeclarationChange", "transition": "draft", "actor": OP,
        "inputs": {"source": {"ledger": BACKFILLED},
                   "source_ref": "git:ledger@2"}})
    assert v["kind"] == "satisfied", v
    report = wire.post("/v1/publish", {"actor": OP, "change": v["object_id"],
                                       "dry_run": True})["report"]
    backfill = [f for f in report["findings"]
                if f["code"] == "mapping:backfill:Entry.d"]
    assert [f["severity"] for f in backfill] == ["fatal"], report["findings"]
    assert "cannot hold" in backfill[0]["message"], backfill

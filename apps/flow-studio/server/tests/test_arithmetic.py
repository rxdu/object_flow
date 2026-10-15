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


EARLY = """module: early
categories: [live, closed]
types:
  Visit:
    description: A visit before the year 1000.
    tracking: record
    attributes:
      at: { type: timestamp }
    states:
      OPEN: { category: live, description: Open. }
      DONE: { category: closed, final: true, description: Done. }
    transitions:
      open: { kind: initial, to: OPEN, inputs: { at: { type: timestamp } }, effect: [ { assign: { location: at, expr: inputs.at } } ] }
      close: { kind: external, from: [OPEN], to: DONE }
    metrics:
      by_time:
        description: Visits by every calendar unit.
        source: objects
        item: v
        dimensions:
          day: day(v.at)
          week: week(v.at)
          month: month(v.at)
          quarter: quarter(v.at)
          year: year(v.at)
        time_dimension: v.at
        expression: count()
"""


def test_a_year_before_1000_is_written_with_four_digits_alike(tee):
    """A time and each calendar bucket of a year below 1000 written with four
    digits, `0999-W24`, as the core writes them, where the reference runner
    wrote `999-W24` (the audit's values R8)."""
    tee.found.clear()
    load(tee, "early", EARLY)
    visit = made(tee, "Visit", "open", at="0999-06-15T08:30:00Z")
    assert tee.post("/v1/get", {"id": visit})["attributes"]["at"] == \
        "0999-06-15T08:30:00Z"
    page = tee.post("/v1/metric", {"actor": OP, "name": "Visit.by_time"})
    assert [r["dimensions"] for r in page["rows"]] == [
        {"day": "0999-06-15", "week": "0999-W24", "month": "0999-06",
         "quarter": "0999-Q2", "year": "0999"}], page
    alike(tee)


def test_a_publish_reports_a_clock_before_1000_with_four_digits():
    """The publish report's `now`, a sandbox clock set before the year 1000,
    written with four digits as the wire writes every time (R8)."""
    import datetime

    from flowstudio.scripts import flowpublish
    utc = datetime.timezone.utc
    at = datetime.datetime(999, 6, 15, 8, 30, tzinfo=utc)
    seconds = int((at - datetime.datetime(1970, 1, 1, tzinfo=utc))
                  .total_seconds())
    assert flowpublish.timestamp_text(seconds) == "0999-06-15T08:30:00Z"


HISTORY = """module: history
categories: [live, closed]
types:
  Person:
    description: Someone a ticket is given to.
    tracking: record
    attributes:
      who: { type: identity, actor_kind: human }
    states:
      ON: { category: live, description: On. }
      OFF: { category: closed, final: true, description: Off. }
    transitions:
      join: { kind: initial, to: ON, required_inputs: [who] }
      leave: { kind: external, from: [ON], to: OFF }
  Ticket:
    description: A ticket whose own history it derives.
    tracking: record
    attributes:
      n: { type: int, optional: true }
      d: { type: "decimal(10,2)", optional: true }
      owner: { reference: Person, assignee: true, optional: true }
    states:
      OPEN: { category: live, description: Open. }
      DONE: { category: closed, final: true, description: Done. }
    derived_attributes:
      spans: { description: Its spans., expression: this.intervals }
      events: { description: Its events., expression: this.transitions }
      owners: { description: Its owners' spans., expression: this.intervals(owner) }
      pick: { description: A choice., expression: 'if n > 1 then 1.5 else 2.25' }
      half: { description: A choice halved., expression: '(if n > 1 then d else 2.255) / 2' }
      kind_of: { description: Its state's category., expression: state.category }
    transitions:
      open: { kind: initial, to: OPEN, optional_inputs: [n, d] }
      give:
        kind: internal
        from: [OPEN]
        inputs: { to: { reference: Person } }
        effect: [ { assign: { location: owner, expr: inputs.to } } ]
      close: { kind: external, from: [OPEN], to: DONE }
"""


@pytest.mark.parametrize("n, pick, half", [(0, "2.25", "1.128"), (5, "1.5", "0.75")])
def test_a_derived_history_and_choice_are_read_alike(tee, n, pick, half):
    """An object's own spans and events read as the core's rows, a member's
    spans with its values, an `if` at the scale of the branch it takes and a
    category by its name; where the reference runner could write no row,
    took an `if`'s first branch's scale for either and wrote a category as a
    pair, and one value it could not write failed the whole read `500` (the
    audit's values R7 and R9)."""
    tee.found.clear()
    load(tee, "history", HISTORY)
    person = made(tee, "Person", "join", who="ann")
    ticket = made(tee, "Ticket", "open", n=n, d="1.50")
    took(tee, ticket, "give", to=person)
    took(tee, ticket, "close")
    got = tee.post("/v1/get", {"id": ticket})["attributes"]
    assert [(s["state"], s["left_at"] is None) for s in got["spans"]] == \
        [("OPEN", False), ("DONE", True)], got["spans"]
    assert [(e["transition"], e["from_state"], e["to_state"])
            for e in got["events"]] == [("open", None, "OPEN"),
                                        ("give", "OPEN", "OPEN"),
                                        ("close", "OPEN", "DONE")], got["events"]
    assert [s["value"] for s in got["owners"]] == [None, person], got["owners"]
    assert (got["pick"], got["half"]) == (pick, half), got
    assert got["kind_of"] == "closed", got
    alike(tee)

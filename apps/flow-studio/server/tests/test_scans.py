"""An invariant's scan of a type ranging over every other object, held alike
on the core and the reference runner (ADR-0061 decision 1, D696): a booking
overlaps itself, so both refused every booking that came under a rule that
no two live bookings of a room overlap, the first included; every answer
compared whole by the tee, a difference failing the test.

It builds the core's service with the pinned toolchain, and fails, not skips,
without it.
"""

import pytest

from compare import Tee
from flowstudio.core import Core
from test_backends_agree import started
from test_publishing import alike, load, made, took

OP = {"id": "operator"}

BOOKING = """module: booking
categories: [live, closed]
types:
  Room:
    description: A room that can be booked.
    tracking: record
    attributes:
      name: { type: string }
    states:
      OPEN: { category: live }
      SHUT: { category: closed, final: true }
    transitions:
      add: { kind: initial, to: OPEN, required_inputs: [name], description: Adds a room. }
      shut: { kind: external, from: [OPEN], to: SHUT, description: Shuts the room. }
  Booking:
    description: A room held for a span of time.
    tracking: record
    attributes:
      room: { reference: Room }
      start: { type: timestamp, indexed: true }
      end: { type: timestamp, indexed: true }
    states:
      BOOKED: { category: live }
      CANCELLED: { category: closed, final: true }
    invariants:
      no_overlap:
        description: No two live bookings of one room overlap.
        expression: >-
          none(b in Booking where b.room == room and b.start < end and b.end > start
               and b.state.category != closed and state.category != closed)
      ordered: { description: A booking ends after it starts., expression: start < end }
    transitions:
      book: { kind: initial, to: BOOKED, required_inputs: [room, start, end], description: Books the room. }
      cancel: { kind: external, from: [BOOKED], to: CANCELLED, description: Cancels the booking. }
"""


@pytest.fixture(scope="module")
def tee():
    core = Core()
    behind, core_url = started(core)
    runner, runner_url = started()
    tee = Tee(core_url, runner_url)
    tee.module = "scans"
    yield tee
    for server in (behind, runner):
        server.shutdown()
        server.server_close()
    core.close()


def test_a_booking_is_judged_against_the_others(tee):
    """The first booking of a room holds; one overlapping it, or around it,
    is refused naming it; touching ends, another room, and a slot a cancelled
    booking held are taken."""
    load(tee, "booking", BOOKING)
    a, b = made(tee, "Room", "add", name="A"), made(tee, "Room", "add", name="B")

    def book(room, start, end):
        return tee.post("/v1/request", {
            "transition": "book", "actor": OP, "type": "Booking",
            "inputs": {"room": room, "start": f"2026-11-02T{start}:00Z",
                       "end": f"2026-11-02T{end}:00Z"}})

    first = book(a, "10:00", "12:00")
    assert first["kind"] == "satisfied", first
    for start, end in (("11:00", "13:00"), ("09:00", "14:00")):
        v = book(a, start, end)
        assert v["kind"] == "invariant_violated", v
        assert first["object_id"] in v["objects"], v
        assert {f["invariant"] for f in v["failures"]} == {"no_overlap"}, v
    for room, start, end in ((a, "12:00", "14:00"), (b, "11:00", "13:00")):
        assert book(room, start, end)["kind"] == "satisfied"
    took(tee, first["object_id"], "cancel")
    assert book(a, "10:00", "11:00")["kind"] == "satisfied"
    alike(tee)

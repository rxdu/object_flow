"""A creation offered on the object one of its inputs references, through
the studio's server on the reference runner (ADR-0240): one offer for each
input marked `offered_on` that the object fills, a subtype's object among
them, decided with that input bound to the object by each guard reading
nothing else, and naming the input it binds; a recording binding `subject`
and the object's own transitions none.
"""

import pytest

OP = {"id": "operator"}

STAFFING = """module: staffing
categories: [live, closed]
enumerations:
  Level: [JUNIOR, SENIOR]
types:
  User:
    description: A person who may act.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states:
      ACTIVE: { category: live }
      LEFT: { category: closed, final: true }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [login] }
      leave: { kind: external, from: ACTIVE, to: LEFT }
  Agent:
    description: An agent that drafts.
    tracking: record
    attributes:
      key: { type: identity, actor_kind: agent, unique: true }
    states:
      ACTIVE: { category: live }
      RETIRED: { category: closed, final: true }
    transitions:
      issue: { kind: initial, to: ACTIVE, required_inputs: [key] }
      retire: { kind: external, from: ACTIVE, to: RETIRED }
  Place:
    description: A place something is booked in; it has no objects of its own.
    abstract: true
    tracking: record
    attributes:
      label: { type: string }
  Room:
    description: A room.
    extends: Place
    states:
      USED: { category: live }
      GONE: { category: closed, final: true }
    transitions:
      add: { kind: initial, to: USED, required_inputs: [label] }
      remove: { kind: external, from: [USED], to: GONE }
  Slot:
    description: A slot someone may be named to.
    tracking: record
    attributes:
      title: { type: string }
      seats: { type: int }
    observations:
      checks:
        kind: SlotCheck
        description: A check of the slot.
        attributes:
          fine: { type: bool }
    states:
      OPEN: { category: live }
      SHUT: { category: closed, final: true }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [title, seats] }
      shut: { kind: external, from: [OPEN], to: SHUT }
  Nomination:
    description: Someone named for a slot.
    tracking: record
    attributes:
      slot: { reference: Slot }
      level: { type: Level, optional: true }
      note: { type: string, optional: true }
    states:
      NAMED: { category: live }
      DROPPED: { category: closed, final: true }
    conditions:
      slot_open: { description: The slot is open., expression: "inputs.slot.state == Slot.OPEN", remedy: dependent }
      seat_left: { description: The slot has a seat left., expression: "inputs.slot.seats > 0", remedy: dependent }
      backup_open: { description: The backup is open., expression: "inputs.backup is null or inputs.backup.state == Slot.OPEN", remedy: self_serviceable }
      noted: { description: It is noted., expression: "note is not null", remedy: self_serviceable }
    transitions:
      name: { kind: initial, to: NAMED, required_inputs: [slot], offered_on: [slot], guards: { slot_open: deny, seat_left: warn } }
      pair: { kind: initial, to: NAMED, required_inputs: [slot], inputs: { backup: { reference: Slot } }, offered_on: [slot, backup], guards: { slot_open: deny } }
      spare: { kind: initial, to: NAMED, required_inputs: [slot], inputs: { backup: { reference: Slot, optional: true } }, offered_on: [slot], guards: { backup_open: deny } }
      remark: { kind: initial, to: NAMED, required_inputs: [slot], optional_inputs: [note], offered_on: [slot], guards: { noted: deny } }
      grade: { kind: initial, to: NAMED, required_inputs: [slot, level], offered_on: [slot], guards: { slot_open: deny } }
      draft: { kind: initial, to: NAMED, required_inputs: [slot] }
      drop: { kind: external, from: [NAMED], to: DROPPED }
  Booking:
    description: A booking of a place.
    tracking: record
    attributes:
      place: { reference: Place }
    states:
      BOOKED: { category: live }
      ENDED: { category: closed, final: true }
    transitions:
      book: { kind: initial, to: BOOKED, required_inputs: [place], offered_on: [place] }
      end: { kind: external, from: [BOOKED], to: ENDED }
  Approval:
    description: A slot's approval, which a person gives.
    tracking: record
    attributes:
      slot: { reference: Slot }
    states:
      GRANTED: { category: live }
      WITHDRAWN: { category: closed, final: true }
    transitions:
      grant: { kind: initial, to: GRANTED, required_inputs: [slot], offered_on: [slot], taken_by: [human] }
      withdraw: { kind: external, from: [GRANTED], to: WITHDRAWN }
"""

# each creation offered on a slot, in its order: by creating type, its
# transition, and the input's place in offered_on
CREATIONS = [("grant", "Approval", "slot"), ("grade", "Nomination", "slot"),
             ("name", "Nomination", "slot"), ("pair", "Nomination", "slot"),
             ("pair", "Nomination", "backup"),
             ("remark", "Nomination", "slot"),
             ("spare", "Nomination", "slot")]


@pytest.fixture()
def staffing(wire):
    assert wire.post("/sandbox/load", {"files": [
        {"name": "staffing.yaml", "text": STAFFING}]})["loaded"]
    wire.post("/sandbox/reset", {})
    return wire


def made(wire, tn: str, xn: str, **inputs) -> str:
    v = wire.post("/v1/request", {"type": tn, "transition": xn, "actor": OP,
                                  "inputs": inputs})
    assert v["kind"] == "satisfied", v
    return v["object_id"]


def offers(wire, oid: str, actor: dict = OP) -> list:
    return wire.post("/v1/availability", {"actor": actor, "id": oid})


def creation(offered: list, xn: str, binds: str) -> dict:
    return next(o for o in offered if o["transition"] == xn
                and o["binds"] == binds and o["creates"] not in (
                    None, "SlotCheck", "label"))


def test_each_marked_input_is_offered_in_its_order(staffing):
    """A slot is offered each creation marked on an input it fills, one
    offer for each such input, after its own transitions and before the
    recordings on it; its own bind nothing and a recording binds
    `subject`."""
    slot = made(staffing, "Slot", "open", title="Lead", seats=2)
    offered = offers(staffing, slot)
    assert [(o["transition"], o["creates"], o["binds"]) for o in offered] \
        == [("shut", None, None)] + CREATIONS + [
            ("record", "SlotCheck", "subject"), ("record", "label", "subject")], offered
    # a creation's offer lists its inputs but the one it binds
    assert creation(offered, "pair", "slot")["inputs"] == {"backup": "Slot"}
    assert creation(offered, "pair", "backup")["inputs"] == {"slot": "Slot"}
    assert creation(offered, "name", "slot")["inputs"] == {}
    # none of a type its marked input does not reference, and no creation
    # its author did not mark
    room = made(staffing, "Room", "add", label="Blue")
    assert [(o["transition"], o["creates"], o["binds"]) for o in offers(
        staffing, room) if o["binds"] != "subject"] == [
        ("remove", None, None), ("book", "Booking", "place")]


def test_a_creation_is_decided_by_the_guards_reading_its_bound_input(staffing):
    """A guard reading only the bound input is decided with it bound: an
    open slot is offered the naming, a flag raised where it warns, and a
    shut one is refused it by its clause; a guard reading another input or
    the would-be object waits, as another required input does."""
    slot = made(staffing, "Slot", "open", title="Lead", seats=0)
    offered = offers(staffing, slot)
    name = creation(offered, "name", "slot")
    assert (name["availability"], name["verdict"]) == ("available", None)
    assert [f["clause"] for f in name["flags"]] == ["seat_left"], name
    for xn, binds in (("pair", "slot"), ("pair", "backup"), ("spare", "slot"),
                      ("remark", "slot"), ("grade", "slot")):
        o = creation(offered, xn, binds)
        assert (o["availability"], o["verdict"]) == (
            "available_with_input", None), o
    assert staffing.post("/v1/request", {"object_id": slot, "actor": OP,
                                         "transition": "shut"})["kind"] \
        == "satisfied"
    offered = offers(staffing, slot)
    for xn, binds in (("name", "slot"), ("pair", "slot"), ("grade", "slot")):
        o = creation(offered, xn, binds)
        assert o["availability"] == "unavailable", o
        assert (o["verdict"]["kind"], o["verdict"]["clause"],
                o["verdict"]["remedy"]) == (
            "unsatisfied", "slot_open", "dependent"), o
    # what reads another input, or the object a request would create, waits
    for xn, binds in (("pair", "backup"), ("spare", "slot"), ("remark", "slot")):
        assert creation(offered, xn, binds)["availability"] \
            == "available_with_input"
    # the offer is what the request does: a shut slot's naming is refused
    v = staffing.post("/v1/request", {"type": "Nomination", "transition": "name",
                                      "actor": OP, "inputs": {"slot": slot}})
    assert (v["kind"], v["clause"]) == ("unsatisfied", "slot_open"), v


def test_a_subtypes_object_is_offered_a_creation_marked_on_its_base(staffing):
    room = made(staffing, "Room", "add", label="Blue")
    book = creation(offers(staffing, room), "book", "place")
    assert (book["creates"], book["availability"], book["inputs"]) == (
        "Booking", "available", {}), book


def test_an_unknown_actor_is_refused_each_creation(staffing):
    slot = made(staffing, "Slot", "open", title="Lead", seats=2)
    for o in offers(staffing, slot, {"id": "nobody"}):
        assert (o["availability"], o["verdict"]["clause"]) == (
            "unavailable", "actor_known"), o


def test_a_creation_refused_for_who_asks_is_decided_for_them(staffing):
    """A creation an invariant may refuse for who takes it, available
    otherwise, is simulated whole for who asks with its bound input given,
    as `check` would: an agent is refused a grant only a person takes, and
    a person is offered it (ADR-0176 decision 2)."""
    slot = made(staffing, "Slot", "open", title="Lead", seats=2)
    ana = made(staffing, "User", "add", login="ana")
    scout = made(staffing, "Agent", "issue", key="scout")
    person = creation(offers(staffing, slot, {"id": ana}), "grant", "slot")
    agent = creation(offers(staffing, slot, {"id": scout}), "grant", "slot")
    assert (person["availability"], person["actor_dependent"]) == (
        "available", True), person
    assert (agent["availability"], agent["actor_dependent"]) == (
        "unavailable", True), agent
    assert agent["verdict"]["kind"] == "invariant_violated", agent
    assert agent["verdict"]["failures"][0]["invariant"] == "grant_taken_by"
    # a creation no invariant reads who takes it is not simulated
    assert creation(offers(staffing, slot, {"id": scout}), "name", "slot")[
        "actor_dependent"] is False
    # and nothing was written: the grant a person then requests is the
    # store's fourth object, after the slot, the person and the agent
    v = staffing.post("/v1/request", {"type": "Approval", "transition": "grant",
                                      "actor": {"id": ana},
                                      "inputs": {"slot": slot}})
    assert (v["kind"], v["object_id"]) == ("satisfied", "approval-4"), v

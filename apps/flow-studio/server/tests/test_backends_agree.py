"""The two backends' answers compared whole (flow-studio.md §6, slice 4):
every example played through the core, behind the studio's server as the
page meets it, and through the reference runner, every answer compared but
for what either names as not computed. The replays judge each example's
outcome; this judges everything a person reads on the page.

A ratchet holds the differences: `backend_differences.txt` lists each known
one by where it is, with the `TODO.md` entry that accounts for it. One not
listed fails the test, as a finding against one backend; one listed that no
longer occurs is printed, to be removed from the list.

It builds the core's service with the pinned toolchain, and fails, not skips,
without it.
"""

import pathlib
import tempfile
import threading

import pytest

from compare import Tee, raw
from flowstudio.catalogue import Catalogue
from flowstudio.core import Core
from flowstudio.http import serve
from flowstudio.scripts import flows
from replay import Failed, NotRun, Wire, replay

KNOWN = pathlib.Path(__file__).with_name("backend_differences.txt")


def started(core=None):
    server = serve("127.0.0.1", 0, pathlib.Path(tempfile.mkdtemp()),
                   core=core)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, f"http://127.0.0.1:{server.server_address[1]}"


@pytest.fixture(scope="module")
def tee():
    core = Core()
    behind, core_url = started(core)
    runner, runner_url = started()
    yield Tee(core_url, runner_url)
    for server in (behind, runner):
        server.shutdown()
        server.server_close()
    core.close()


def read_metrics(wire: Wire, types: list) -> None:
    """Every metric each type declares, read whole, as the page's metrics
    strip reads it, and by its declared groups, as a reader does; each
    answer is compared by the tee, a read either backend refuses with it."""
    for t in types:
        decl = wire.post("/v1/declaration", {"type": t})
        for n in sorted(decl.get("metrics") or {}):
            for keep in ([], None):
                body = {"actor": {"id": "operator"}, "name": f"{t}.{n}"}
                if keep is not None:
                    body["keep"] = keep
                try:
                    wire.post("/v1/metric", body)
                except (Failed, NotRun):
                    pass


def test_the_backends_differ_only_where_known(tee):
    design = Catalogue(pathlib.Path(tempfile.mkdtemp()))
    for m in design.modules():
        if m["examples"] and m["source"] == "design":
            tee.module = m["file"]
            # a `publish` step's files read beside the design's examples, so
            # every example's publishes run through both (ADR-0166)
            replay(tee, m["file"], bundle=design.bundle(m["file"]),
                   after=read_metrics, read=flows.beside(flows.EXAMPLES))
    # the service's migration examples, an examples file of their own that
    # no change carries, its publishes run through both (ADR-0166)
    tee.module = "service-migration.examples.yaml"
    bundle = design.bundle("service.yaml")
    migration = flows.EXAMPLES / "service-migration.examples.yaml"
    bundle["examples"] = {"name": migration.name, "text": migration.read_text()}
    ran = replay(tee, "service.yaml", bundle=bundle, after=read_metrics,
                 read=flows.beside(flows.EXAMPLES))
    assert [o for o, _ in ran.values()] == ["passed"] * 4, ran
    found = tee.kinds()
    known = {line.strip() for line in KNOWN.read_text().splitlines()
             if line.strip() and not line.startswith("#")}
    for gone in sorted(known - found):
        print(f"  no longer differs, to remove from {KNOWN.name}: {gone}")
    new = sorted(found - known)
    for kind in new:
        first = next(f for f in tee.found if tee.kind(f) == kind)
        print(f"  new: {kind}\n       {first[3]}  <- {first[4]}")
    assert new == [], "the backends differ where no TODO entry accounts for it"


BOOKINGS = """module: bookings
categories: [live, closed]
types:
  Booking:
    description: A desk booked for a day.
    tracking: record
    attributes:
      day: { type: date }
    states:
      HELD: { category: live, description: Held. }
      DONE: { category: closed, final: true, description: Done. }
    transitions:
      book: { kind: initial, to: HELD, required_inputs: [day] }
      end: { kind: external, from: HELD, to: DONE }
    metrics:
      booked: { description: Bookings., source: objects, item: b, expression: count() }
"""


def test_a_metrics_parameters_are_read_alike(tee):
    """A metric's filter's parameters, bound by the fields of its rows, are
    answered alike by both backends, each refusal too (ADR-0221): a day, a
    state, an instant, an attempt count's day, one params does not give,
    one never named, params without a filter, and params that is no
    object."""
    tee.module = "bookings (parameters)"
    before = len(tee.found)
    op = {"id": "operator"}
    tee.post("/sandbox/load",
             {"files": [{"name": "bookings.yaml", "text": BOOKINGS}]})
    tee.post("/sandbox/clock", {"set": "2026-11-09T09:00:00Z"})
    ids = [tee.post("/v1/request",
                    {"actor": op, "type": "Booking", "transition": "book",
                     "inputs": {"day": d}})["object_id"]
           for d in ("2026-11-08", "2026-11-09", "2026-11-10")]
    end = {"actor": op, "object_id": ids[0], "transition": "end"}
    tee.post("/v1/request", end)
    # a refused request, an attempt the day's counts hold
    tee.post("/v1/request", end)
    days = {"from": "2026-11-09", "to": "2026-11-10"}
    reads = [
        ("Booking.booked", "b.day >= :from and b.day <= :to", days),
        ("Booking.booked", "b.state == :s", {"s": "DONE"}),
        ("Booking.booked", "b.created_at >= :at",
         {"at": "2026-11-09T00:00:00Z"}),
        ("Booking.transition_counts", "t.to_state == :s and t.object == :b",
         {"s": "DONE", "b": ids[0]}),
        ("Booking.refusals", "a.day == :d", {"d": "2026-11-09"}),
        ("Booking.booked", "b.day >= :from", {}),
        ("Booking.booked", "b.day >= :from", days),
        ("Booking.booked", None, {"from": "2026-11-09"}),
    ]
    answered = []
    for name, filt, params in reads:
        body = {"actor": op, "name": name, "keep": [], "params": params}
        if filt is not None:
            body["filter"] = filt
        answered.append(tee.post("/v1/metric", body)["refused"])
    assert answered == [None] * 5 + ["malformed_filter"] * 3, answered
    with pytest.raises(Failed):
        tee.post("/v1/metric", {"actor": op, "name": "Booking.booked",
                                "filter": "b.day >= :from",
                                "params": ["2026-11-09"]})
    assert tee.found[before:] == [], tee.found[before:]



NOTES = """module: notes
categories: [live, closed]
types:
  Note:
    description: A note written again and again.
    tracking: record
    attributes:
      text: { type: string, optional: true }
    states:
      OPEN: { category: live, description: Open. }
      DONE: { category: closed, final: true, description: Done. }
    transitions:
      jot: { kind: initial, to: OPEN }
      write: { kind: internal, from: OPEN, required_inputs: [text] }
      finish: { kind: external, from: OPEN, to: DONE }
"""


def test_a_history_newest_first_is_read_alike(tee):
    """A history read newest first, page by page at each limit, is
    answered alike by both backends, and is the oldest-first read reversed;
    a cursor given back goes on before it, and a `newest_first` that is not
    true or false is refused by both (ADR-0243 decision 1)."""
    tee.module = "notes (newest first)"
    before = len(tee.found)
    op = {"id": "operator"}
    tee.post("/sandbox/load", {"files": [{"name": "notes.yaml",
                                          "text": NOTES}]})
    a, b = (tee.post("/v1/request", {"actor": op, "type": "Note",
                                     "transition": "jot"})["object_id"]
            for _ in range(2))
    for n in (a, b, a, a, b, a):
        tee.post("/v1/request", {"actor": op, "object_id": n,
                                 "transition": "write",
                                 "inputs": {"text": "x"}})

    def read(oid: str, limit: int, newest_first: bool) -> list:
        out, cursor = [], None
        while True:
            body = {"actor": op, "id": oid, "limit": limit,
                    "newest_first": newest_first}
            if cursor is not None:
                body["cursor"] = cursor
            page = tee.post("/v1/history", body)
            assert len(page["events"]) <= limit, page
            out += [e["position"] for e in page["events"]]
            cursor = page["cursor"]
            if cursor is None:
                return out

    for oid, n in ((a, 5), (b, 3)):
        oldest = read(oid, 1000, False)
        assert len(oldest) == n and oldest == sorted(oldest), oldest
        for limit in (1, 2, 3):
            assert read(oid, limit, False) == oldest
            assert read(oid, limit, True) == oldest[::-1]
    with pytest.raises(Failed):
        tee.post("/v1/history", {"actor": op, "id": a, "newest_first": "yes"})
    assert tee.found[before:] == [], tee.found[before:]


MOVES = """module: moves
categories: [live, closed]
types:
  Thing:
    description: A thing whose hands change.
    tracking: record
    observations:
      custody:
        kind: Custody
        description: Who holds the thing.
        attributes:
          to_whom: { type: string, description: Who takes it. }
          note: { type: string, optional: true }
    states:
      ON: { category: live, description: On. }
      OFF: { category: closed, final: true, description: Off. }
    transitions:
      add: { kind: initial, to: ON }
      scrap: { kind: external, from: [ON], to: OFF }
  Mover:
    description: Moves a thing.
    tracking: record
    attributes:
      thing: { reference: Thing }
    states:
      ON: { category: live, description: On. }
      OFF: { category: closed, final: true, description: Off. }
    transitions:
      add: { kind: initial, to: ON, required_inputs: [thing] }
      scrap: { kind: external, from: [ON], to: OFF }
      move:
        kind: internal
        from: ON
        effect:
          - create: { type: Custody, transition: record, inputs: { subject: thing }, rest: inputs }
"""


def test_derived_inputs_are_served_and_taken_alike(tee):
    """A create taking the rest of its inputs (ADR-0271) is answered alike
    by both backends: the act's declaration serves each derived input as
    the record declares it, marked with the record it is derived from, and
    the create as written; its offer and its tool take them; given, they
    are recorded; and one the record requires, left out, is refused."""
    tee.module = "moves (derived inputs)"
    before = len(tee.found)
    op = {"id": "operator"}
    loaded = tee.post("/sandbox/load", {"files": [{"name": "moves.yaml",
                                                   "text": MOVES}]})
    assert loaded["loaded"], loaded
    move = tee.post("/v1/declaration", {"type": "Mover"})["transitions"]["move"]
    assert move["inputs"] == {
        "to_whom": {"type": "string", "description": "Who takes it.",
                    "derived_from": "Custody.record"},
        "note": {"type": "string", "optional": True,
                 "derived_from": "Custody.record"}}, move
    assert move["effect"][0]["create"]["rest"] == "inputs", move
    thing = tee.post("/v1/request", {"actor": op, "type": "Thing",
                                     "transition": "add"})["object_id"]
    mover = tee.post("/v1/request", {"actor": op, "type": "Mover",
                                     "transition": "add",
                                     "inputs": {"thing": thing}})["object_id"]
    tee.post("/v1/availability", {"actor": op, "id": mover})
    tee.post("/v1/tools", {"actor_kind": "human"})
    left_out = tee.post("/v1/request", {"actor": op, "object_id": mover,
                                        "transition": "move",
                                        "inputs": {"note": "At the gate"}})
    assert left_out["kind"] == "invalid_input", left_out
    assert left_out["reasons"] == {"to_whom": "missing"}, left_out
    given = tee.post("/v1/request", {"actor": op, "object_id": mover,
                                     "transition": "move",
                                     "inputs": {"to_whom": "Ana"}})
    assert given["kind"] == "satisfied" and given["object_id"] == mover, given
    [custody] = given["created"]
    recorded = tee.post("/v1/get", {"actor": op, "id": custody})
    assert recorded["attributes"]["to_whom"] == "Ana", recorded
    assert tee.found[before:] == [], tee.found[before:]


CARRIES = """module: carries
categories: [live, closed]
types:
  Thing:
    description: A thing whose hands change.
    tracking: record
    attributes:
      label: { type: string, optional: true }
    observations:
      custody:
        kind: Custody
        description: Who holds the thing.
        attributes:
          to_whom: { type: string, description: Who takes it. }
          tags: { type: "string[]", optional: true }
    states:
      ON: { category: live, description: On. }
      OFF: { category: closed, final: true, description: Off. }
    transitions:
      add: { kind: initial, to: ON }
      arrive:
        kind: initial
        to: ON
        required_inputs: [label]
        effect:
          - create: { type: Custody, transition: record, inputs: { subject: this }, rest: inputs }
      scrap: { kind: external, from: [ON], to: OFF }
  Mover:
    description: Moves a thing.
    tracking: record
    attributes:
      thing: { reference: Thing }
    states:
      ON: { category: live, description: On. }
      OFF: { category: closed, final: true, description: Off. }
    transitions:
      add: { kind: initial, to: ON, required_inputs: [thing] }
      scrap: { kind: external, from: [ON], to: OFF }
      move:
        kind: internal
        from: ON
        effect:
          - create: { type: Custody, transition: record, inputs: { subject: thing }, rest: inputs }
      make:
        kind: internal
        from: ON
        effect:
          - create: { type: Thing, transition: arrive, rest: inputs }
"""


def test_derived_sets_and_chains_are_served_and_taken_alike(tee):
    """A derived set the record need not be given, and a derivation through a
    create whose target takes the rest of its own (ADR-0275), are answered
    alike by both backends: the declaration serves the set defaulting to the
    empty set and the chained inputs marked with the step's own target; a
    set left out is recorded empty and one given as given; and a request
    through the chain records the custody, its receiver refused left out."""
    tee.module = "carries (derived sets and chains)"
    before = len(tee.found)
    op = {"id": "operator"}
    loaded = tee.post("/sandbox/load", {"files": [{"name": "carries.yaml",
                                                   "text": CARRIES}]})
    assert loaded["loaded"], loaded
    mover_type = tee.post("/v1/declaration", {"type": "Mover"})["transitions"]
    assert mover_type["move"]["inputs"]["tags"] == {
        "type": "string[]", "default": "{}",
        "derived_from": "Custody.record"}, mover_type["move"]
    assert mover_type["make"]["inputs"] == {
        "label": {"type": "string", "derived_from": "Thing.arrive"},
        "to_whom": {"type": "string", "description": "Who takes it.",
                    "derived_from": "Thing.arrive"},
        "tags": {"type": "string[]", "default": "{}",
                 "derived_from": "Thing.arrive"}}, mover_type["make"]
    thing = tee.post("/v1/request", {"actor": op, "type": "Thing",
                                     "transition": "add"})["object_id"]
    mover = tee.post("/v1/request", {"actor": op, "type": "Mover",
                                     "transition": "add",
                                     "inputs": {"thing": thing}})["object_id"]
    tee.post("/v1/availability", {"actor": op, "id": mover})
    for inputs, want in (({"to_whom": "Ana"}, []),
                         ({"to_whom": "Bo", "tags": ["fragile"]}, ["fragile"])):
        moved = tee.post("/v1/request", {"actor": op, "object_id": mover,
                                         "transition": "move",
                                         "inputs": inputs})
        assert moved["kind"] == "satisfied", moved
        [custody] = moved["created"]
        recorded = tee.post("/v1/get", {"actor": op, "id": custody})
        assert recorded["attributes"]["tags"] == want, recorded
    left_out = tee.post("/v1/request", {"actor": op, "object_id": mover,
                                        "transition": "make",
                                        "inputs": {"label": "Box"}})
    assert left_out["kind"] == "invalid_input", left_out
    assert left_out["reasons"] == {"to_whom": "missing"}, left_out
    made = tee.post("/v1/request", {"actor": op, "object_id": mover,
                                    "transition": "make",
                                    "inputs": {"label": "Box", "to_whom": "Cy"}})
    assert made["kind"] == "satisfied" and len(made["created"]) == 2, made
    custody = [c for c in made["created"]
               if tee.post("/v1/get", {"actor": op, "id": c})["type"] == "Custody"]
    assert len(custody) == 1, made
    recorded = tee.post("/v1/get", {"actor": op, "id": custody[0]})
    assert recorded["attributes"]["to_whom"] == "Cy", recorded
    assert recorded["attributes"]["tags"] == [], recorded
    assert tee.found[before:] == [], tee.found[before:]


LEDGER = """module: docs
categories: [live, closed]
evaluators:
  ledger:
    description: A ledger that clears references.
    functions:
      cleared:
        description: Whether the ledger has cleared this reference.
        arguments:
          ref: { type: string }
types:
  Doc:
    description: A document.
    tracking: record
    attributes:
      ref: { type: string }
    states:
      OPEN: { description: Open., category: live }
      DONE: { description: Done., category: closed, final: true }
    derived_attributes:
      clear: { description: Whether the ledger cleared it., expression: 'ledger.cleared(ref)' }
    conditions:
      is_clear: { description: The ledger cleared it., expression: 'clear', remedy: delegable }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [ref] }
      close: { kind: external, from: OPEN, to: DONE, guards: { is_clear: deny } }
"""


def test_an_evaluator_a_derivation_asks_is_answered_alike(tee):
    """A guard reading a derived attribute that asks an evaluator, loaded with
    the checks skipped since step 3 lets only a guard ask one, refuses as
    unknown with its own remedy where the evaluator does not answer, the
    derivation absent, and its offer lists nothing unevaluated, on both
    backends alike (ADR-0138, ADR-0146)."""
    tee.module = "docs (an evaluator a derivation asks)"
    before = len(tee.found)
    op = {"id": "operator"}
    loaded = tee.post("/sandbox/load", {"files": [{"name": "docs.yaml", "text": LEDGER}], "unchecked": True})
    assert loaded["loaded"], loaded
    tee.post("/sandbox/reset", {})
    tee.post("/sandbox/evaluators", {"answers": {"ledger.cleared": "unanswered"}})
    doc = tee.post("/v1/request", {"actor": op, "type": "Doc", "transition": "open",
                                   "inputs": {"ref": "R-1"}})["object_id"]
    [close] = [o for o in tee.post("/v1/availability", {"actor": op, "id": doc}) if o["transition"] == "close"]
    assert close["unevaluated"] == [], close
    v = tee.post("/v1/request", {"actor": op, "object_id": doc, "transition": "close"})
    assert (v["kind"], v.get("clause"), v.get("remedy"), v.get("unknown")) == ("unsatisfied", "is_clear", "delegable", True), v
    assert tee.found[before:] == [], tee.found[before:]


UNWRITTEN = """module: unwritten
categories: [live, closed]
types:
  Owner:
    description: An owner.
    tracking: record
    attributes:
      name: { type: string }
    states:
      ACTIVE: { description: Active., category: live }
      GONE: { description: Gone., category: closed, final: true }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [name] }
      drop: { kind: external, from: ACTIVE, to: GONE }
  Job:
    description: A job.
    tracking: record
    attributes:
      site: { type: string }
      owner: { reference: Owner }
    states:
      OPEN: { description: Open., category: live }
      DONE: { description: Done., category: closed, final: true }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [site] }
      close: { kind: external, from: OPEN, to: DONE }
"""


@pytest.mark.parametrize("case", ["reference", "string", "counter"])
def test_a_creation_leaving_a_required_attribute_absent_is_refused_alike(tee, case):
    """A creation that writes no value to a required attribute, a reference or
    a string, each of which check 8 refuses and so loaded with the checks
    skipped, refuses `value_in_range` naming no object on both backends,
    where the core's store faulted it 500 (ADR-0201 decision 7); a counter,
    which starts at 0, is no such attribute, and the creation applies."""
    tee.module = f"unwritten ({case})"
    before = len(tee.found)
    op = {"id": "operator"}
    text = {
        "reference": UNWRITTEN,
        "string": UNWRITTEN.replace("      owner: { reference: Owner }\n",
                                    "      owner: { reference: Owner, optional: true }\n      code: { type: string }\n"),
        # a counter is a quantity's (check 31)
        "counter": UNWRITTEN.replace("      owner: { reference: Owner }\n",
                                     "      owner: { reference: Owner, optional: true }\n      visits: { type: counter }\n"
                                     ).replace("  Job:\n    description: A job.\n    tracking: record\n",
                                               "  Job:\n    description: A job.\n    tracking: quantity\n"),
    }[case]
    loaded = tee.post("/sandbox/load", {"files": [{"name": "unwritten.yaml", "text": text}], "unchecked": case != "counter"})
    assert loaded["loaded"], loaded
    tee.post("/sandbox/reset", {})
    v = tee.post("/v1/request", {"actor": op, "type": "Job", "transition": "open", "inputs": {"site": "x"}})
    if case == "counter":
        assert v["kind"] == "satisfied", v
    else:
        assert (v["kind"], v.get("clause"), v.get("objects")) == ("unsatisfied", "value_in_range", []), v
    assert tee.found[before:] == [], tee.found[before:]


PLACING = """module: placing
categories: [live, closed]
enumerations:
  Size: [S, M, L]
types:
  Product:
    description: A product an order's line takes.
    tracking: record
    attributes:
      code: { type: string, description: Its code. }
    states:
      LISTED: { description: Listed., category: live }
      RETIRED: { description: Retired., category: closed, final: true }
    transitions:
      list: { kind: initial, to: LISTED, required_inputs: [code], description: Lists it. }
      retire: { kind: external, from: [LISTED], to: RETIRED, description: Retires it. }
  Order:
    description: An order placed with its lines.
    tracking: record
    attributes:
      title: { type: string, description: Its title. }
    states:
      PLACED: { description: Placed., category: live }
      CLOSED: { description: Closed., category: closed, final: true }
    transitions:
      place:
        kind: initial
        to: PLACED
        required_inputs: [title]
        description: Places it with its lines.
        proposable: true
        effect:
          - create: { type: Line, transition: add, inputs: { order: this }, each: lines, limit: 3 }
      close: { kind: external, from: [PLACED], to: CLOSED, description: Closes it. }
  Line:
    description: A line of an order.
    tracking: record
    attributes:
      order: { reference: Order, description: Its order. }
      product: { reference: Product, description: Its product. }
      qty: { type: int, description: How many units. }
      rush: { type: bool, optional: true, description: Whether it ships first. }
      size: { type: Size, optional: true, description: Its size. }
    states:
      OPEN: { description: Open., category: live }
      CLOSED: { description: Closed., category: closed, final: true }
    conditions:
      product_listed:
        description: The product is listed.
        expression: product.state == Product.LISTED
        remedy: self_serviceable
    transitions:
      add:
        kind: initial
        to: OPEN
        description: Adds it.
        required_inputs: [order, product, qty]
        optional_inputs: [rush, size]
        guards: { product_listed: deny }
      close: { kind: external, from: [OPEN], to: CLOSED, description: Closes it. }
"""


def test_entries_are_taken_and_refused_alike(tee):
    """A create taking each entry of a list (ADR-0290) is answered alike by
    both backends: each entry makes a line, in the order given, a repeated
    one its own, the list left out none; an entry at fault is refused
    `invalid_input` naming each fault by entry and field, the list named
    with the first fault's reason; one that is no list, or an entry that is
    no object, `wrong_type`; more entries than the limit `over_limit`, its
    loop `each lines`; and a creation refused for an entry `call_refused`,
    naming the entry."""
    tee.module = "placing (entries)"
    before = len(tee.found)
    op = {"id": "operator"}
    loaded = tee.post("/sandbox/load", {"files": [{"name": "placing.yaml",
                                                   "text": PLACING}]})
    assert loaded["loaded"], loaded
    tee.post("/sandbox/reset", {})

    def made(t, xn, inputs):
        return tee.post("/v1/request", {"actor": op, "type": t,
                                        "transition": xn, "inputs": inputs})

    a = made("Product", "list", {"code": "A"})["object_id"]
    b = made("Product", "list", {"code": "B"})["object_id"]
    tee.post("/v1/request", {"actor": op, "object_id": b,
                             "transition": "retire"})
    line = {"product": a, "qty": 2, "rush": True, "size": "M"}
    placed = made("Order", "place", {"title": "First", "lines": [line, line, dict(line, qty=5)]})
    assert placed["kind"] == "satisfied", placed
    assert len(placed["created"]) == 4, placed
    for lid, qty in zip(placed["created"][1:], (2, 2, 5)):
        got = tee.post("/v1/get", {"actor": op, "id": lid})
        assert got["attributes"]["qty"] == qty and got["attributes"]["size"] == "M", got
    bare = made("Order", "place", {"title": "Bare"})
    assert bare["kind"] == "satisfied" and bare["created"] == [bare["object_id"]], bare
    faulty = made("Order", "place", {"title": "Bad", "lines": [
        {"product": a, "qty": 1, "colour": "red"}, "a line",
        {"product": "product-99", "qty": "two"}]})
    assert faulty["kind"] == "invalid_input", faulty
    assert faulty["inputs"] == ["lines"], faulty
    assert faulty["reasons"] == {"lines": "unknown_input"}, faulty
    assert faulty["entries"] == [
        {"entry": 0, "input": "colour", "reason": "unknown_input"},
        {"entry": 1, "input": None, "reason": "wrong_type"},
        {"entry": 2, "input": "product", "reason": "no_such_object"},
        {"entry": 2, "input": "qty", "reason": "malformed"}], faulty
    missing = made("Order", "place", {"title": "Bad", "lines": [{"product": a}]})
    assert missing["entries"] == [{"entry": 0, "input": "qty", "reason": "missing"}], missing
    listless = made("Order", "place", {"title": "Bad", "lines": {"product": a}})
    assert (listless["reasons"], "entries" in listless) == ({"lines": "wrong_type"}, False), listless
    over = made("Order", "place", {"title": "Big", "lines": [line] * 4})
    assert (over["kind"], over["loop"], over["bound"]) == ("over_limit", "each lines", 3), over
    # the runner computes them too, which the tee would leave out were they
    # named not computed
    _, alone = raw(tee.runner, "POST", "/v1/request", {"actor": op, "type": "Order", "transition": "place",
                                                       "inputs": {"title": "Big", "lines": [line] * 4}})
    assert (alone["loop"], alone["bound"], "not_computed" in alone) == ("each lines", 3, False), alone
    # decided with the inputs' types, before a required input left out
    untitled = made("Order", "place", {"lines": [line] * 4})
    assert untitled["kind"] == "over_limit", untitled
    for k in range(3):
        lines = [dict(line, product=b if i == k else a) for i in range(3)]
        refused = made("Order", "place", {"title": "Retired", "lines": lines})
        assert refused["kind"] == "call_refused", refused
        assert (refused["step"], refused["entry"], refused["object_id"]) == ("create Line.add", k, None), refused
        assert refused["verdict"]["clause"] == "Line.product_listed", refused
    # a proposal's list filed as a request reads it
    proposed = tee.post("/v1/request", {"actor": op, "type": "Order", "transition": "place", "propose": True,
                                        "inputs": {"title": "Asked", "lines": [{"product": a}]}})
    assert proposed["entries"] == [{"entry": 0, "input": "qty", "reason": "missing"}], proposed
    proposed = tee.post("/v1/request", {"actor": op, "type": "Order", "transition": "place", "propose": True,
                                        "inputs": {"title": "Asked", "lines": [line] * 4}})
    assert (proposed["kind"], proposed["loop"]) == ("over_limit", "each lines"), proposed
    proposed = tee.post("/v1/request", {"actor": op, "type": "Order", "transition": "place", "propose": True,
                                        "inputs": {"title": "Asked", "lines": [line]}})
    assert proposed["kind"] == "satisfied", proposed
    assert tee.found[before:] == [], tee.found[before:]


def test_entries_are_declared_offered_and_published_alike(tee):
    """A list a create's `each` takes (ADR-0290) is served alike by both
    backends: `declaration` describes it by its entries' fields, its limit,
    its default and its target, and lists the enumerations its fields read;
    an offer types it `entries(<Type>.<transition>)` and takes it as no
    required input; an agent's tool takes it as an array of objects; and a
    publish invalidates a pending proposal whose entry holds a field the
    version no longer takes."""
    tee.module = "placing (declared)"
    before = len(tee.found)
    op = {"id": "operator"}
    extended = PLACING.replace(
        "      close: { kind: external, from: [PLACED], to: CLOSED, description: Closes it. }\n  Line:",
        "      close: { kind: external, from: [PLACED], to: CLOSED, description: Closes it. }\n"
        "      extend:\n        kind: internal\n        from: [PLACED]\n        description: Adds lines to it.\n"
        "        effect:\n"
        "          - create: { type: Line, transition: add, inputs: { order: this }, each: more, limit: 2 }\n"
        "  Line:")
    assert extended != PLACING
    loaded = tee.post("/sandbox/load", {"files": [{"name": "placing.yaml",
                                                   "text": extended}]})
    assert loaded["loaded"], loaded
    tee.post("/sandbox/reset", {})
    decl = tee.post("/v1/declaration", {"type": "Order"})
    assert decl["transitions"]["place"]["inputs"]["lines"] == {
        "entries": {
            "product": {"reference": "Product", "description": "Its product."},
            "qty": {"type": "int", "description": "How many units."},
            "rush": {"type": "bool", "description": "Whether it ships first.", "optional": True},
            "size": {"type": "Size", "description": "Its size.", "optional": True}},
        "limit": 3, "default": "[]", "derived_from": "Line.add"}, decl
    assert decl["enumerations"] == {"Size": ["S", "M", "L"]}, decl
    a = tee.post("/v1/request", {"actor": op, "type": "Product", "transition": "list",
                                 "inputs": {"code": "A"}})["object_id"]
    order = tee.post("/v1/request", {"actor": op, "type": "Order", "transition": "place",
                                     "inputs": {"title": "First"}})["object_id"]
    offers = {o["transition"]: o for o in tee.post("/v1/availability", {"actor": op, "id": order})}
    assert offers["extend"]["inputs"] == {"more": "entries(Line.add)"}, offers["extend"]
    assert offers["extend"]["availability"] == "available", offers["extend"]
    tools = {t["name"]: t for t in tee.post("/v1/tools", {"actor_kind": "human"})["tools"]}
    lines = tools["order_place"]["input_schema"]["properties"]["inputs"]["properties"]["lines"]
    assert (lines["type"], lines["maxItems"], lines["items"]["required"]) == ("array", 3, ["product", "qty"]), lines
    assert "lines" not in tools["order_place"]["input_schema"]["properties"]["inputs"].get("required", []), lines
    proposed = tee.post("/v1/request", {"actor": op, "type": "Order", "transition": "place", "propose": True,
                                        "inputs": {"title": "Asked",
                                                   "lines": [{"product": a, "qty": 1, "rush": True}]}})
    assert proposed["kind"] == "satisfied", proposed
    dropped = extended.replace(
        "      rush: { type: bool, optional: true, description: Whether it ships first. }\n", "").replace(
        "        optional_inputs: [rush, size]\n", "        optional_inputs: [size]\n")
    assert dropped.count("rush") == 0, dropped
    change = tee.post("/v1/request", {"actor": op, "type": "DeclarationChange", "transition": "draft",
                                      "inputs": {"source": {"placing": dropped},
                                                 "source_ref": "git:placing@2"}})["object_id"]
    report = tee.post("/v1/publish", {"actor": op, "change": change, "dry_run": True})["report"]
    assert report["affected"]["proposals_invalidated"] == [proposed["object_id"]], report
    assert tee.found[before:] == [], tee.found[before:]


def test_a_proposal_is_refused_in_the_order_a_request_is_alike(tee):
    """Both backends refuse a filed proposal in the order an executed
    request is refused (`DESIGN.md` §6): an input not of its type first,
    every other input at fault named with it when filed; then a list past
    its limit (ADR-0290 decision 4); then a reference naming nothing or an
    input left out. A list past its limit with a required input left out,
    or a reference naming nothing, is `over_limit`, executed or filed, and
    a proposal of a transition taking no list is refused as before."""
    tee.module = "placing (order of refusals)"
    before = len(tee.found)
    op = {"id": "operator"}
    priced = PLACING.replace(
        "      code: { type: string, description: Its code. }\n",
        "      code: { type: string, description: Its code. }\n"
        "      price: { type: int, description: Its price. }\n").replace(
        "      list: { kind: initial, to: LISTED, required_inputs: [code], description: Lists it. }",
        "      list: { kind: initial, to: LISTED, required_inputs: [code, price], proposable: true, "
        "description: Lists it. }").replace(
        "      title: { type: string, description: Its title. }\n",
        "      title: { type: string, description: Its title. }\n"
        "      rep: { reference: Product, optional: true, description: Its representative product. }\n").replace(
        "        required_inputs: [title]\n        description: Places it with its lines.\n",
        "        required_inputs: [title]\n        optional_inputs: [rep]\n"
        "        description: Places it with its lines.\n")
    assert priced.count("price") == 3 and priced.count("rep:") == priced.count("[rep]") == 1, priced
    loaded = tee.post("/sandbox/load", {"files": [{"name": "placing.yaml", "text": priced}]})
    assert loaded["loaded"], loaded
    tee.post("/sandbox/reset", {})

    def asked(t, xn, inputs, propose=False):
        body = {"actor": op, "type": t, "transition": xn, "inputs": inputs}
        if propose:
            body["propose"] = True
        return tee.post("/v1/request", body)

    a = asked("Product", "list", {"code": "A", "price": 1})["object_id"]
    four = [{"product": a, "qty": 1}] * 4
    for propose in (False, True):
        for given in ({"lines": four}, {"title": "T", "rep": "nothing", "lines": four}):
            v = asked("Order", "place", given, propose)
            assert (v["kind"], v["loop"], v["bound"]) == ("over_limit", "each lines", 3), (given, v)
        v = asked("Order", "place", {"title": 5, "lines": four}, propose)
        assert (v["kind"], v["inputs"]) == ("invalid_input", ["title"]), v
    v = asked("Order", "place", {"rep": "nothing", "lines": four[:1]}, True)
    assert (v["kind"], v["inputs"], v["reasons"]) == (
        "invalid_input", ["rep", "title"], {"rep": "no_such_object", "title": "missing"}), v
    v = asked("Product", "list", {"price": "high"}, True)
    assert (v["kind"], v["inputs"], v["reasons"]) == (
        "invalid_input", ["price", "code"], {"price": "malformed", "code": "missing"}), v
    assert "entries" not in v, v
    assert tee.found[before:] == [], tee.found[before:]


# a slot a hold refers to, in a module the holds' imports and the slot's may
# not (issue #103, ADR-0292): a hold follows its slot's cancel and rename, a
# lock its cancel and a tag its hold's release; the slot's shifts are parts
FOLLOW_HUB = """module: hub
categories: [live, closed]
types:
  Slot:
    description: A slot work is planned in.
    tracking: record
    attributes:
      label: { type: string, description: Its label. }
      cap: { type: int, default: 9, description: The most objects that may refer to it when it is cancelled. }
      shifts: { reference: "Shift[]", opposite: slot, aggregation: composite, cascade: [ { on: [cancel], transition: end, limit: 10 } ], description: Its shifts. }
    states:
      OPEN: { category: live, description: Open. }
      CANCELLED: { category: closed, final: true, description: Cancelled. }
    conditions:
      few_holds: { description: Few objects refer to it., expression: "count(r in referrers) <= cap", remedy: dependent }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [label], optional_inputs: [cap], description: Opens it. }
      rename: { kind: internal, from: [OPEN], required_inputs: [label], description: Renames it. }
      recount: { kind: internal, from: [OPEN], description: Counts its holds again. }
      staff: { kind: internal, from: [OPEN], effect: [ { create: { type: Shift, transition: add, inputs: { slot: this } } } ], description: Adds a shift. }
      cancel:
        kind: external
        from: [OPEN]
        to: CANCELLED
        proposable: true
        description: Cancels it.
        guards: { few_holds: deny }
        inputs:
          why: { type: string, optional: true, description: Why it is cancelled. }
  Shift:
    description: A shift of a slot.
    tracking: record
    attributes:
      slot: { reference: Slot, opposite: shifts, description: Its slot. }
    states:
      ON: { category: live, description: On. }
      OFF: { category: closed, final: true, description: Off. }
    transitions:
      add: { kind: initial, to: ON, required_inputs: [slot], only_via: [Slot.staff], description: Adds it. }
      end: { kind: external, from: [ON], to: OFF, description: Ends it. }
"""

FOLLOW_HOLDS = """module: holds
imports: { hub: [Slot] }
categories: [live, closed]
types:
  Lock:
    description: A lock fitted on a slot.
    tracking: record
    attributes:
      slot: { reference: Slot, follows: [ { on: [cancel], transition: open_up, limit: 5 } ], description: Its slot. }
    states:
      SHUT: { category: live, description: Shut. }
      OPENED: { category: closed, final: true, description: Opened. }
    transitions:
      fit: { kind: initial, to: SHUT, required_inputs: [slot], description: Fits it. }
      open_up: { kind: external, from: [SHUT], to: OPENED, description: Opens it. }
  Hold:
    description: A hold on a slot.
    tracking: record
    attributes:
      slot:
        reference: Slot
        description: The slot held.
        follows:
          - { on: [cancel], transition: release, inputs: { note: inputs.why, seen: slot.label, own: label }, limit: 2 }
          - { on: [rename], transition: relabel, inputs: { label: slot.label }, limit: 5 }
          - { on: [recount], transition: tally, inputs: { n: "1 / zero" }, limit: 5 }
      label: { type: string, optional: true, description: Its label. }
      note: { type: string, optional: true, description: Why it ended. }
      seen: { type: string, optional: true, description: Its slot's label when it ended. }
      own: { type: string, optional: true, description: Its own label when it ended. }
      blocked: { type: bool, default: false, description: Whether it may not end. }
      zero: { type: int, default: 0, description: Nothing. }
      n: { type: int, optional: true, description: A count. }
    states:
      HELD: { category: live, description: Held. }
      PAUSED: { category: live, description: Paused. }
      RELEASED: { category: closed, final: true, description: Released. }
    invariants:
      fair: { description: It ended for no bad reason., expression: 'note is null or note != "bad"' }
    conditions:
      not_blocked: { description: It is not blocked., expression: not blocked, remedy: dependent }
    transitions:
      take: { kind: initial, to: HELD, required_inputs: [slot], optional_inputs: [label, blocked], description: Takes it. }
      pause: { kind: external, from: [HELD], to: PAUSED, description: Pauses it. }
      relabel: { kind: internal, from: [HELD, PAUSED], required_inputs: [label], description: Relabels it. }
      tally: { kind: internal, from: [HELD, PAUSED], optional_inputs: [n], description: Counts it. }
      drop: { kind: external, from: [HELD, PAUSED], to: RELEASED, description: Drops it. }
      release: { kind: external, from: [HELD], to: RELEASED, optional_inputs: [note, seen, own], guards: { not_blocked: deny }, only_via: [Slot.cancel], description: Releases it. }
  Tag:
    description: A tag stuck on a hold.
    tracking: record
    attributes:
      hold: { reference: Hold, follows: [ { on: [release], transition: peel, limit: 5 } ], description: Its hold. }
    states:
      ON: { category: live, description: On. }
      OFF: { category: closed, final: true, description: Off. }
    transitions:
      stick: { kind: initial, to: ON, required_inputs: [hold], description: Sticks it. }
      peel: { kind: external, from: [ON], to: OFF, description: Peels it. }
"""


def test_followers_are_driven_and_refused_alike(tee):
    """An object following a transition of the object it refers to
    (ADR-0292) is driven alike by both backends, every answer compared:
    after the slot's effect and its cascade, a lock's clause before a hold's,
    each hold in creation order and a tag following its hold before the next
    hold, each read whole after; a clause's inputs read over the follower;
    no follower, nothing more; a follow past its limit `over_limit`, its loop
    `the follow on Hold.slot`, three final followers passed over and not
    counted; a paused follower `unavailable` in its own state, and a blocked
    one `unsatisfied` naming `Hold.not_blocked`, and one whose invariant the
    follow breaks with what the request gave `self_serviceable`; a clause's
    input a division by zero left with none `value_in_range`, naming the
    follower (ADR-0293 decision 1); a proposal
    driving them
    once approved; and a guard of the slot reading its referrers counting
    them."""
    tee.module = "hub and holds (follows)"
    before = len(tee.found)
    op = {"id": "operator"}
    loaded = tee.post("/sandbox/load", {"files": [{"name": "hub.yaml", "text": FOLLOW_HUB},
                                                   {"name": "holds.yaml", "text": FOLLOW_HOLDS}]})
    assert loaded["loaded"], loaded
    tee.post("/sandbox/reset", {})

    def made(t, xn, inputs):
        v = tee.post("/v1/request", {"actor": op, "type": t, "transition": xn, "inputs": inputs})
        assert v["kind"] == "satisfied", v
        return v["object_id"]

    def act(oid, xn, inputs=None, propose=False):
        body = {"actor": op, "object_id": oid, "transition": xn, "inputs": inputs or {}}
        if propose:
            body["propose"] = True
        return tee.post("/v1/request", body)

    def get(oid):
        return tee.post("/v1/get", {"actor": op, "id": oid})

    a, b, alone = (made("Slot", "open", {"label": n}) for n in ("A", "B", "C"))
    assert act(a, "staff")["kind"] == "satisfied"
    h1, h2 = (made("Hold", "take", {"slot": a, "label": n}) for n in ("h1", "h2"))
    hb = made("Hold", "take", {"slot": b, "label": "hb"})
    tag = made("Tag", "stick", {"hold": h1})
    lock = made("Lock", "fit", {"slot": a})
    renamed = act(a, "rename", {"label": "A2"})
    assert renamed["kind"] == "satisfied" and len(renamed["events"]) == 3, renamed
    assert act(h1, "relabel", {"label": "mine"})["kind"] == "satisfied"
    cancelled = act(a, "cancel", {"why": "gone"})
    assert cancelled["kind"] == "satisfied" and len(cancelled["events"]) == 6, cancelled
    for oid in (a, h1, h2, hb, tag, lock):
        tee.post("/v1/history", {"actor": op, "id": oid})
    got = get(h1)
    assert (got["state"], got["attributes"]["note"], got["attributes"]["seen"], got["attributes"]["own"]) == (
        "RELEASED", "gone", "A2", "mine"), got
    assert get(hb)["state"] == "HELD"
    assert len(act(alone, "cancel")["events"]) == 1
    assert act(hb, "release")["kind"] == "not_requestable"
    # past the limit, and then three final followers passed over
    c = made("Slot", "open", {"label": "D"})
    held = [made("Hold", "take", {"slot": c}) for _ in range(3)]
    over = act(c, "cancel")
    assert (over["kind"], over["loop"], over["bound"]) == ("over_limit", "the follow on Hold.slot", 2), over
    # the runner computes them too, which the tee would leave out were they named not computed
    _, alone_runner = raw(tee.runner, "POST", "/v1/request", {"actor": op, "object_id": c, "transition": "cancel"})
    assert (alone_runner["loop"], alone_runner["bound"], "not_computed" in alone_runner) == (
        "the follow on Hold.slot", 2, False), alone_runner
    assert act(held[2], "drop")["kind"] == "satisfied"
    for _ in range(2):
        assert act(made("Hold", "take", {"slot": c}), "drop")["kind"] == "satisfied"
    applied = act(c, "cancel")
    assert applied["kind"] == "satisfied" and len(applied["events"]) == 3, applied
    # a follower refusing refuses the request with its own verdict
    d = made("Slot", "open", {"label": "E"})
    made("Hold", "take", {"slot": d})
    paused = made("Hold", "take", {"slot": d})
    assert act(paused, "pause")["kind"] == "satisfied"
    v = act(d, "cancel")
    assert (v["kind"], v["state"]) == ("unavailable", "PAUSED"), v
    e = made("Slot", "open", {"label": "F"})
    made("Hold", "take", {"slot": e})
    made("Hold", "take", {"slot": e, "blocked": True})
    v = act(e, "cancel")
    assert (v["kind"], v["clause"], v["remedy"]) == ("unsatisfied", "Hold.not_blocked", "dependent"), v
    assert get(e)["state"] == "OPEN"
    # an invariant of the follower its follow breaks, with what the request gave
    g = made("Slot", "open", {"label": "I"})
    made("Hold", "take", {"slot": g})
    v = act(g, "cancel", {"why": "bad"})
    assert (v["kind"], v["remedy"], v["failures"][0]["invariant"]) == ("invariant_violated", "self_serviceable", "fair"), v
    # a clause's input a division by zero left with none refuses, unqualified, naming the follower (ADR-0293)
    r = made("Slot", "open", {"label": "J"})
    rh = made("Hold", "take", {"slot": r})
    made("Hold", "take", {"slot": r})
    v = act(r, "recount")
    assert (v["kind"], v["clause"], v["objects"]) == ("unsatisfied", "value_in_range", [rh]), v
    assert get(rh)["attributes"].get("n") is None
    # a guard reading the referrers counts the followers; a proposal drives
    # them once approved
    crowded = made("Slot", "open", {"label": "G", "cap": 1})
    made("Hold", "take", {"slot": crowded})
    made("Hold", "take", {"slot": crowded})
    v = act(crowded, "cancel")
    assert (v["kind"], v["clause"]) == ("unsatisfied", "few_holds"), v
    f = made("Slot", "open", {"label": "H", "cap": 1})
    fh = made("Hold", "take", {"slot": f})
    filed = act(f, "cancel", {"why": "asked"}, propose=True)
    assert filed["kind"] == "satisfied", filed
    assert get(fh)["state"] == "HELD"
    approved = tee.post("/v1/request", {"actor": op, "object_id": filed["object_id"], "transition": "approve"})
    assert approved["kind"] == "satisfied", approved
    got = get(fh)
    assert (got["state"], got["attributes"]["note"]) == ("RELEASED", "asked"), got
    assert tee.found[before:] == [], tee.found[before:]


# a slot whose contact is personal, its shifts parts its cancel cascades to
# and its holds following its cancel, each copying the contact and the
# cancel's reason into personal attributes of its own (the integrity
# review's finding 1)
ERASE_HUB = """module: hub
categories: [live, closed]
types:
  Slot:
    description: A slot work is planned in.
    tracking: record
    attributes:
      label: { type: string, description: Its label. }
      contact: { type: string, optional: true, personal: true, description: Who to call. }
      shifts:
        reference: "Shift[]"
        opposite: slot
        aggregation: composite
        description: Its shifts.
        cascade:
          - { on: [cancel], transition: end, inputs: { memo: contact, cause: inputs.why }, limit: 10 }
    states:
      OPEN: { category: live, description: Open. }
      CANCELLED: { category: closed, final: true, description: Cancelled. }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [label], optional_inputs: [contact], description: Opens it. }
      staff: { kind: internal, from: [OPEN], effect: [ { create: { type: Shift, transition: add, inputs: { slot: this } } } ], description: Adds a shift. }
      cancel:
        kind: external
        from: [OPEN]
        to: CANCELLED
        description: Cancels it.
        inputs:
          why: { type: string, optional: true, description: Why it is cancelled. }
      forget:
        kind: erasure
        description: Forgets it, and its shifts.
        inputs: { reason: { type: string, description: Why. } }
        effect:
          - foreach: { item: f, array: shifts, limit: 10, steps: [ { call: { target: f, transition: forget, inputs: { reason: inputs.reason } } } ] }
  Shift:
    description: A shift of a slot.
    tracking: record
    attributes:
      slot: { reference: Slot, opposite: shifts, description: Its slot. }
      memo: { type: string, optional: true, personal: true, description: Its slot's contact. }
      cause: { type: string, optional: true, personal: true, description: Why it ended. }
    states:
      ON: { category: live, description: On. }
      OFF: { category: closed, final: true, description: Off. }
    transitions:
      add: { kind: initial, to: ON, required_inputs: [slot], only_via: [Slot.staff], description: Adds it. }
      end: { kind: external, from: [ON], to: OFF, optional_inputs: [memo, cause], description: Ends it. }
      forget: { kind: erasure, inputs: { reason: { type: string, description: Why. } }, description: Forgets it. }
"""

ERASE_HOLDS = """module: holds
imports: { hub: [Slot] }
categories: [live, closed]
types:
  Hold:
    description: A hold on a slot.
    tracking: record
    attributes:
      slot: { reference: Slot, follows: [ { on: [cancel], transition: release, inputs: { note: slot.contact, cause: inputs.why }, limit: 5 } ], description: The slot held. }
      note: { type: string, optional: true, personal: true, description: Its slot's contact. }
      cause: { type: string, optional: true, personal: true, description: Why it ended. }
    states:
      HELD: { category: live, description: Held. }
      RELEASED: { category: closed, final: true, description: Released. }
    transitions:
      take: { kind: initial, to: HELD, required_inputs: [slot], description: Takes it. }
      release: { kind: external, from: [HELD], to: RELEASED, optional_inputs: [note, cause], description: Releases it. }
      forget: { kind: erasure, inputs: { reason: { type: string, description: Why. } }, description: Forgets it. }
"""


def test_an_erasure_reaches_across_a_cascade_and_a_follow_alike(tee):
    """An erasure follows a cascade and a follow as it follows a call, alike
    in both backends (`storage-schema.md` §9 steps 3 and 12): the slot's
    contact its hold's follow copied, read through the reference, is cleared
    from the hold, which records a `redact`; and the cancel's reason a
    follow or a cascade passed into the erased follower's or part's personal
    attribute leaves the slot's event, which records a `redact`. Every
    answer is compared, the histories with their `redact` events among
    them."""
    tee.module = "hub and holds (erasure)"
    before = len(tee.found)
    op = {"id": "operator"}
    loaded = tee.post("/sandbox/load", {"files": [{"name": "hub.yaml", "text": ERASE_HUB},
                                                   {"name": "holds.yaml", "text": ERASE_HOLDS}]})
    assert loaded["loaded"], loaded
    tee.post("/sandbox/reset", {})

    def made(t, xn, inputs):
        v = tee.post("/v1/request", {"actor": op, "type": t, "transition": xn, "inputs": inputs})
        assert v["kind"] == "satisfied", v
        return v

    def act(oid, xn, inputs=None):
        v = tee.post("/v1/request", {"actor": op, "object_id": oid, "transition": xn, "inputs": inputs or {}})
        assert v["kind"] == "satisfied", v
        return v

    def cancelled(shifted=True, held=True):
        slot = made("Slot", "open", {"label": "A", "contact": "a@b"})["object_id"]
        shift = act(slot, "staff")["created"][0] if shifted else None
        hold = made("Hold", "take", {"slot": slot})["object_id"] if held else None
        act(slot, "cancel", {"why": "moving"})
        return slot, shift, hold

    def histories(*ids):
        for oid in ids:
            if oid:
                tee.post("/v1/history", {"actor": op, "id": oid})

    # down: the slot erased with only a follower, then with only a part, so
    # one other object is redacted at a time (TODO.md: the two backends order
    # the `redact` events of several objects unlike)
    slot, _, hold = cancelled(shifted=False)
    act(slot, "forget", {"reason": "asked"})
    got = tee.post("/v1/get", {"actor": op, "id": hold})
    assert (got["attributes"].get("note"), got["attributes"].get("cause")) == (None, "moving"), got
    events = tee.post("/v1/history", {"actor": op, "id": hold})["events"]
    assert [e["transition"] for e in events] == ["take", "release", "redact"], events
    histories(slot)
    slot, shift, _ = cancelled(held=False)
    act(slot, "forget", {"reason": "asked"})
    events = tee.post("/v1/history", {"actor": op, "id": shift})["events"]
    assert [e["transition"] for e in events] == ["add", "end", "redact", "forget"], events
    histories(slot)
    # up: the follower erased, and the part, each redacting the slot's reason
    for erased in ("hold", "shift"):
        slot, shift, hold = cancelled()
        act(hold if erased == "hold" else shift, "forget", {"reason": "asked"})
        events = tee.post("/v1/history", {"actor": op, "id": slot})["events"]
        assert events[-1]["transition"] == "redact", (erased, events)
        histories(shift, hold)
    assert tee.found[before:] == [], tee.found[before:]

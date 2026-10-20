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

from compare import Tee
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
    assert [o for o, _ in ran.values()] == ["passed", "passed"], ran
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


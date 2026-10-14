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

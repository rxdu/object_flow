"""The Rust core as the studio's backend (flow-studio.md §2, §6 slice 4): the
server forwards the wire to the core's service unchanged and keeps its own
tooling. The replay and the front end's tests play every example and a free
session through it; these hold the forwarder at its edges.

It builds the core's service with the pinned toolchain, and fails, not skips,
without it.
"""

import re
import subprocess
import sys
import threading
import urllib.error
import urllib.request

import pytest

from flowstudio.core import Core
from flowstudio.http import serve
from replay import Failed, Wire

SERVICE = "service.yaml"


def running(workspace, core):
    server = serve("127.0.0.1", 0, workspace, core=core)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, Wire(f"http://127.0.0.1:{server.server_address[1]}")


@pytest.fixture(scope="module")
def studio(workspace):
    core = Core()
    server, wire = running(workspace, core)
    bundle = wire.get(f"/studio/module?file={SERVICE}")
    assert wire.post("/sandbox/load", {"files": bundle["files"]})["loaded"]
    yield wire, core
    server.shutdown()
    server.server_close()
    core.close()


def test_the_page_is_told_the_core_answers(studio):
    wire, core = studio
    assert wire.get("/studio/backend") == {"backend": "core",
                                           "service": core.url}


def test_every_operation_reaches_the_core_not_only_the_runners(studio):
    # query is the core's; the runner's server has no route for it
    wire, _ = studio
    page = wire.post("/v1/query", {"actor": {"id": "operator"},
                                   "type": "ServiceJob"})
    assert page["items"] == []


def test_a_fault_keeps_the_cores_status_and_words(studio):
    wire, _ = studio
    with pytest.raises(Failed, match="404: no type Nothing is declared"):
        wire.post("/v1/declaration", {"type": "Nothing"})
    # an operation the core does not have is its own JSON fault, its words
    # kept (ADR-0173 decision 12, D555), where its body was empty
    with pytest.raises(Failed, match="404: no operation /v1/nothing"):
        wire.post("/v1/nothing", {})


def test_a_load_of_another_shape_is_answered_not_dropped(studio, workspace, monkeypatch):
    # files named where each is an object of a name and a text: answered
    # 400 naming the shape, by either backend's server, where the core's
    # forwarder raised and closed the socket, a 502 through the front end's
    # proxy (found restarting the studio on 0.3.0)
    wire, _ = studio
    with pytest.raises(Failed, match="400: load takes `files`"):
        wire.post("/sandbox/load", {"files": ["service.yaml"]})
    server, runner = running(workspace, None)
    try:
        with pytest.raises(Failed, match="400: load takes `files`"):
            runner.post("/sandbox/load", {"files": ["service.yaml"]})
    finally:
        server.shutdown()
        server.server_close()
    # and a fault of the forwarder's own is answered 500, never a closed socket
    import flowstudio.http

    def broken(_body):
        raise RuntimeError("a fault of its own")
    monkeypatch.setattr(flowstudio.http, "checked", broken)
    with pytest.raises(Failed, match="500: RuntimeError: a fault of its own"):
        wire.post("/sandbox/load", {"files": []})
    assert wire.get("/studio/backend")["backend"] == "core"


def test_a_load_the_checker_refuses_never_reaches_the_core(studio):
    # the studio's load runs the flow checker first, for either backend
    # (ADR-0153 decision 5): the refusal is the checker's, and the core
    # keeps the module it held
    wire, _ = studio
    bundle = wire.get(f"/studio/module?file={SERVICE}")
    broken = [dict(f, text=f["text"].replace("        to: OPEN",
                                             "        to: NOWHERE", 1))
              if f["name"] == SERVICE else f for f in bundle["files"]]
    assert broken != bundle["files"]
    refused = wire.post("/sandbox/load", {"files": broken})
    assert refused["loaded"] is False
    assert any(f["file"] == SERVICE for f in refused["findings"])
    assert wire.post("/v1/declaration", {"type": "ServiceJob"})["states"]


def test_an_imported_objects_legacy_entries_come_before_its_events(workspace):
    # ADR-0163: an entry's own fields, the kind's kept fields with it, and
    # `history` returning the entries, by ordinal, before the events
    core = Core()
    server, wire = running(workspace, core)
    try:
        bundle = wire.get("/studio/module?file=servicedesk.yaml")
        assert wire.post("/sandbox/load", {"files": bundle["files"]})["loaded"]
        report = wire.post("/v1/import_batch", {
            "actor": {"id": "operator"},
            "batch": {
                "objects": [{
                    "type": "Employee", "legacy_key": "E-1",
                    "state": "EMPLOYED",
                    "attributes": {"employee_id": "1", "name": "Ana"},
                    "legacy_entries": [
                        {"kind": "audit", "occurred_at": "2025-06-01T00:00:00Z",
                         "payload": {"action": "hired", "by": "hr"}},
                        {"kind": "audit", "occurred_at": "2025-07-01T00:00:00Z",
                         "payload": {"action": "moved"}},
                    ],
                }],
                "legacy_kinds": {"audit": ["action"]},
            },
            "disposition": [], "dry_run": False,
        })
        assert report["written"] == 1, report
        oid = wire.post("/v1/lookup", {"actor": {"id": "operator"},
                                       "source": "hr", "value": "1"})["id"]
        items = wire.post("/v1/history", {"id": oid})["events"]
        assert [i.get("ordinal") for i in items[:2]] == [1, 2]
        assert items[0]["legacy"] and items[0]["payload"] == {
            "action": "hired", "by": "hr"}
        assert "position" in items[2] and items[2]["transition"] == "import"
    finally:
        server.shutdown()
        server.server_close()
        core.close()


def test_the_tooling_stays_the_servers(studio):
    wire, _ = studio
    bundle = wire.get(f"/studio/module?file={SERVICE}")
    checked = wire.post("/studio/check", {"files": bundle["files"]})
    assert checked["fatal"] == 0


def test_a_core_that_stopped_is_a_502_saying_so(workspace):
    core = Core()
    server, wire = running(workspace, core)
    try:
        core.close()
        with pytest.raises(Failed, match=r"502: the core's service did not "
                                         r"answer \(exited with"):
            wire.post("/sandbox/reset", {})
    finally:
        server.shutdown()
        server.server_close()


def answers(url):
    """Whether a service answers at all, whatever its status."""
    try:
        urllib.request.urlopen(urllib.request.Request(
            url + "/sandbox/reset", data=b"{}", method="POST",
            headers={"Content-Type": "application/json"}), timeout=5).close()
    except urllib.error.HTTPError:
        return True
    except urllib.error.URLError:
        return False
    return True


def test_a_terminated_server_stops_its_core(workspace):
    server = subprocess.Popen(
        [sys.executable, "-m", "flowstudio", "--port", "0", "--workspace",
         str(workspace), "--backend", "core"],
        stderr=subprocess.PIPE, text=True)
    try:
        line = server.stderr.readline()
        m = re.search(r"answered by the core at (http://127\.0\.0\.1:\d+)",
                      line)
        assert m, line
        assert answers(m.group(1))
        server.terminate()
        assert server.wait(timeout=10) == 0
        assert not answers(m.group(1))
    finally:
        server.kill()


def test_both_backends_report_the_same_load(studio):
    # what a load installed, for every design module: the page builds its
    # create list from the types, so a difference shows a person different
    # choices on each backend
    import tempfile
    import pathlib
    from flowstudio.catalogue import Catalogue
    from flowstudio.sandbox import Sandbox

    wire, _ = studio
    design = Catalogue(pathlib.Path(tempfile.mkdtemp()))
    try:
        for m in design.modules():
            if m["source"] != "design":
                continue
            files = design.bundle(m["file"])["files"]
            runner = Sandbox().load({"files": files})
            core = wire.post("/sandbox/load", {"files": files})
            for key in ("loaded", "types", "modules", "evaluators", "notices"):
                assert core[key] == runner[key], (m["file"], key)
    finally:
        bundle = wire.get(f"/studio/module?file={SERVICE}")
        wire.post("/sandbox/load", {"files": bundle["files"]})

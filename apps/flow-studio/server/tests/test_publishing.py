"""The reference runner's publishing held to the core's (ADR-0166): a change
to the declaration taken through both backends at once, every answer compared
whole by the tee, a difference failing the test as a finding against one of
them, as the backend comparison does for the examples.

It builds the core's service with the pinned toolchain, and fails, not skips,
without it.
"""

import pathlib
import tempfile

import pytest

from compare import Tee
from flowstudio.catalogue import Catalogue
from flowstudio.core import Core
from test_backends_agree import started

EXAMPLES = pathlib.Path(__file__).resolve().parents[4] / \
    "docs/design/flow-format/examples"
OP = {"id": "operator"}


@pytest.fixture(scope="module")
def tee():
    core = Core()
    behind, core_url = started(core)
    runner, runner_url = started()
    tee = Tee(core_url, runner_url)
    tee.module = "publishing"
    yield tee
    for server in (behind, runner):
        server.shutdown()
        server.server_close()
    core.close()


def loaded(tee: Tee, module: str) -> None:
    bundle = Catalogue(pathlib.Path(tempfile.mkdtemp())).bundle(module)
    assert tee.post("/sandbox/load", {"files": bundle["files"]})["loaded"]
    tee.post("/sandbox/reset", {})


def change(tee: Tee, xn: str, oid: str | None = None, **inputs) -> dict:
    body = {"transition": xn, "actor": OP, "inputs": inputs}
    if oid is None:
        body["type"] = "DeclarationChange"
    else:
        body["object_id"] = oid
    return tee.post("/v1/request", body)


def test_a_change_is_drafted_revised_and_withdrawn_alike(tee):
    loaded(tee, "service.yaml")
    tee.found.clear()
    v2 = (EXAMPLES / "service-v2.yaml").read_text()
    drafted = change(tee, "draft", source={"service": v2},
                     source_ref="git:service@2")
    assert drafted["kind"] == "satisfied", drafted
    c = drafted["object_id"]
    tee.post("/v1/get", {"id": c})
    revised = change(tee, "revise", c, source={"service": v2},
                     source_ref="git:service@3",
                     evidence=[{"metric": "ServiceJob.time_working",
                                "keep": ["engineer"]}])
    assert revised["kind"] == "satisfied", revised
    tee.post("/v1/get", {"id": c})
    # what a draft takes, refused as a request would be
    refused = [
        change(tee, "draft", source={"service": v2}, source_ref=""),
        change(tee, "draft", source={"elsewhere": v2}, source_ref="r"),
        change(tee, "draft", source={"service": v2}, source_ref="r", why=1),
        change(tee, "draft", source={}, source_ref="r"),
        change(tee, "draft", source={"service": v2}, source_ref="r",
               evidence=[{"metric": ""}]),
        change(tee, "supersede", c),
        change(tee, "reopen", c),
        change(tee, "reject", c),
    ]
    assert [r["kind"] for r in refused] == [
        "invalid_input", "invalid_input", "invalid_input", "invalid_input",
        "invalid_input", "not_requestable", "unavailable", "unavailable"]
    assert change(tee, "withdraw", c)["kind"] == "satisfied"
    assert change(tee, "revise", c, source={"service": v2},
                  source_ref="r")["kind"] == "unavailable"
    tee.post("/v1/get", {"id": c})
    tee.post("/v1/history", {"id": c})
    tee.post("/v1/availability", {"actor": OP, "id": c})
    differences = [f"{p} {w}: {what}" for _m, p, w, what, _b in tee.found]
    assert differences == [], "\n".join(differences)

"""The reference runner's import held to the core's (ADR-0154, ADR-0166
decision 2): a batch taken through both backends at once, every answer
compared whole by the tee, a difference failing the test as a finding
against one of them.

It builds the core's service with the pinned toolchain, and fails, not skips,
without it.
"""

import pytest

from compare import Tee
from flowstudio.core import Core
from test_backends_agree import started

OP = {"id": "operator"}

PORT = """module: port
categories: [live]
types:
  RobotModel:
    description: A model of robot, the legacy system's.
    mirror: true
    tracking: record
    attributes:
      code: { type: string, external: erp }
      units: { reference: "Robot[]", opposite: model }
    states:
      ACTIVE: { category: live }
  Robot:
    description: A robot, the legacy system's.
    mirror: true
    tracking: record
    attributes:
      serial: { type: string, external: erp }
      model: { reference: RobotModel, opposite: units, optional: true }
    states:
      READY: { category: live }
"""


@pytest.fixture(scope="module")
def tee():
    core = Core()
    behind, core_url = started(core)
    runner, runner_url = started()
    tee = Tee(core_url, runner_url)
    tee.module = "importing"
    yield tee
    for server in (behind, runner):
        server.shutdown()
        server.server_close()
    core.close()


def alike(tee) -> None:
    differences = sorted({f"{p} {w}: {what}"
                          for _m, p, w, what, _b in tee.found})
    assert differences == [], "\n".join(differences)


def test_an_end_the_other_side_stores_is_never_required(tee):
    """A model's `units`, the end of `Robot.model` the robots store, is read
    from them and given by no import, so a model imported without it is
    placed (flow-format.md §4.3, D541)."""
    assert tee.post("/sandbox/load", {"files": [
        {"name": "port.yaml", "text": PORT}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    report = tee.post("/v1/import_batch", {"actor": OP, "batch": {"objects": [
        {"type": "RobotModel", "legacy_key": "M-1", "state": "ACTIVE",
         "attributes": {"code": "M1"}}]}})
    assert not report.get("refusal"), report
    found = tee.post("/v1/lookup", {"source": "legacy",
                                    "value": "RobotModel:M-1"})
    tee.post("/v1/get", {"id": found["id"]})
    alike(tee)

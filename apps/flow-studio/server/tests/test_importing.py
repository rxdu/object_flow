"""The reference runner's import held to the core's (ADR-0154, ADR-0166
decision 2): a batch taken through both backends at once, every answer
compared whole by the tee, a difference failing the test as a finding
against one of them.

It builds the core's service with the pinned toolchain, and fails, not skips,
without it.
"""

import pytest

from compare import Tee, raw
from replay import Failed
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


NUMBERED = """module: numbered
categories: [live]
sequences:
  serial_no: { description: The legacy system's serial numbers. }
types:
  Part:
    description: A part, the legacy system's.
    mirror: true
    tracking: record
    attributes:
      code: { type: string, external: erp }
    states:
      READY: { category: live }
"""


def test_a_raise_naming_no_sequence_or_past_its_last_is_refused_alike(tee):
    """A batch's raise names a sequence a module declares, in a scope its
    identifiers take, with a last value that leaves a next: one naming none,
    past what a sequence draws, negative or scoped where none is, is
    refused alike through both backends, where i64::MAX was stored wrapped
    and stopped every write (fourth production review A-15)."""
    assert tee.post("/sandbox/load", {"files": [
        {"name": "numbered.yaml", "text": NUMBERED}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()

    def raised(sequence: str, last: int, scope: str = "") -> dict:
        return tee.post("/v1/import_batch", {"actor": OP, "batch": {
            "objects": [],
            "sequences": [{"sequence": sequence, "last": last, "scope": scope}]}})

    for sequence, last, scope, why in (
            ("junk", 7, "", "is no sequence the flow declares"),
            ("serial_no", 9223372036854775807, "", "leaves it no next number"),
            ("serial_no", -1, "", "leaves it no next number"),
            ("serial_no", 7, "north", "numbers in no scope")):
        report = raised(sequence, last, scope)
        assert why in ((report.get("refusal") or {}).get("reason") or ""), report
    assert raised("serial_no", 512).get("refusal") is None
    alike(tee)


def test_a_key_an_imported_object_does_not_take_is_refused_alike(tee):
    """An imported object holding a key it does not take, a misspelt
    `atributes`, is refused naming it through both backends, where each read
    it as absent and imported the object without its attributes (the second
    production review's minor); so is a batch's or a raise's."""
    assert tee.post("/sandbox/load", {"files": [
        {"name": "numbered.yaml", "text": NUMBERED}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    with pytest.raises(Failed, match="an imported object takes no key atributes"):
        tee.post("/v1/import_batch", {"actor": OP, "batch": {"objects": [
            {"type": "Part", "legacy_key": "P-1", "state": "READY",
             "atributes": {"code": "x"}}]}})
    # and a batch's or a raise's, where a misspelt `sequence` raised none,
    # so a go-live's numbers could take a legacy record's
    with pytest.raises(Failed, match="a batch takes no key sequence"):
        tee.post("/v1/import_batch", {"actor": OP, "batch": {
            "objects": [], "sequence": [{"sequence": "serial_no", "last": 7}]}})
    with pytest.raises(Failed, match="a sequence's raise takes no key scop"):
        tee.post("/v1/import_batch", {"actor": OP, "batch": {
            "objects": [], "sequences": [{"sequence": "serial_no", "last": 7, "scop": ""}]}})
    report = tee.post("/v1/import_batch", {"actor": OP, "batch": {"objects": [
        {"type": "Part", "legacy_key": "P-1", "state": "READY",
         "attributes": {"code": "x"}}]}})
    assert report.get("refusal") is None and report["written"] == 1, report
    alike(tee)


KITS = """module: kits
categories: [live]
types:
  Kit:
    description: A kit, the legacy system's.
    mirror: true
    tracking: record
    attributes:
      code: { type: string, external: erp }
      tags: { type: "string[]", optional: true }
    states:
      ACTIVE: { category: live }
"""


@pytest.mark.parametrize("tags", [[None], ["a", None], [1], 5])
def test_an_imported_value_not_of_its_type_is_never_held(tee, tags):
    """A value not of its attribute's type, a null among a set's elements
    too, is a class the core's import reports and writes nothing for until
    it is decided, where a null element faulted `500` at the store's
    constraint; the reference runner, which leaves classes to the core
    (ADR-0154 decision 8), answers `not_run` where it held the value (the
    differential battery)."""
    for w in (tee.core, tee.runner):
        assert raw(w, "POST", "/sandbox/load", {"files": [
            {"name": "kits.yaml", "text": KITS}]})[1]["loaded"]
        raw(w, "POST", "/sandbox/reset", {})
    batch = {"objects": [{"type": "Kit", "legacy_key": "k1",
                          "state": "ACTIVE",
                          "attributes": {"code": "C1", "tags": tags}}]}
    status, got = raw(tee.core, "POST", "/v1/import_batch",
                      {"actor": OP, "batch": batch})
    assert status == 200 and got["written"] == 0, got
    assert [(f["rule"], f["shape"]) for f in got["findings"]] == \
        [("tags", "is not of its type")], got
    status, got = raw(tee.runner, "POST", "/v1/import_batch",
                      {"actor": OP, "batch": batch})
    assert status == 501, got
    # a set that fits is imported by both
    batch["objects"][0]["attributes"]["tags"] = ["a", "b"]
    batch["objects"][0]["legacy_key"] = "k2"
    for w in (tee.core, tee.runner):
        assert raw(w, "POST", "/v1/import_batch",
                   {"actor": OP, "batch": batch})[1]["written"] == 1

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


def second_version(module: str) -> dict:
    """A module's second version as a change's source, which is the whole
    version (ADR-0161): the modules it imports as they stand, and the module
    and its examples file at version 2, each file under the name it declares
    (ADR-0155 decision 6)."""
    from flowstudio.scripts import flowpublish

    stem = module.removesuffix(".yaml")
    bundle = Catalogue(pathlib.Path(tempfile.mkdtemp())).bundle(module)
    out = {}
    for f in bundle["files"]:
        if f["name"] != module:
            out[flowpublish.declared_name(f["text"])] = f["text"]
    for f in (EXAMPLES / f"{stem}-v2.yaml",
              EXAMPLES / f"{stem}-v2.examples.yaml"):
        if f.exists():
            text = f.read_text()
            out[flowpublish.declared_name(text)] = text
    return out


@pytest.mark.parametrize("module", ["service.yaml", "inventory.yaml"])
def test_a_second_versions_dry_run_reports_alike(tee, module):
    """After each example of a module, its second version drafted, submitted
    and dry run through both backends, its report compared: what each
    mapping rewrites, what it leaves fatal or to decide, the offers it
    changes and what it discharges (publish-and-import.md §1, §2)."""
    from replay import replay

    source = second_version(module)
    design = Catalogue(pathlib.Path(tempfile.mkdtemp()))
    reports = []

    def dry_run(wire, _types):
        drafted = change(wire, "draft", source=source,
                         source_ref=f"git:{module}@2")
        assert drafted["kind"] == "satisfied", drafted
        c = drafted["object_id"]
        wire.post("/v1/request", {"transition": "submit", "actor": OP,
                                  "object_id": c})
        reports.append(wire.post("/v1/publish", {"actor": OP, "change": c,
                                                 "dry_run": True}))
        wire.post("/v1/get", {"id": c})
        change(wire, "withdraw", c)

    tee.found.clear()
    tee.module = module
    replay(tee, module, bundle=design.bundle(module), after=dry_run)
    assert reports, "no example ran"
    differences = sorted({f"{p} {w}: {what}"
                          for _m, p, w, what, _b in tee.found})
    assert differences == [], "\n".join(differences)


FLEET_V1 = """module: fleet
categories: [live, closed]
enumerations:
  Grade: [A, B, OLD]
  OverrideReason: [LEGACY]
types:
  Robot:
    description: A robot of the fleet.
    tracking: record
    attributes:
      serial: { type: string }
      grade: { type: Grade, optional: true }
    states:
      READY: { category: live }
      HOLD: { category: live }
      RETIRED: { category: closed, final: true }
    conditions:
      graded: { description: The robot is graded., expression: grade is not null, remedy: self_serviceable }
    transitions:
      add: { kind: initial, to: READY, required_inputs: [serial], optional_inputs: [grade] }
      hold: { kind: external, from: [READY], to: HOLD, proposable: true }
      release: { kind: external, from: [HOLD], to: READY }
      retire: { kind: external, from: [READY], to: RETIRED, guards: { graded: audit } }
"""


def fleet_v2(map_hold: bool, backfill: bool, admit: bool) -> str:
    """The fleet's second version, as the core's tests write it: HOLD and
    Grade.OLD removed, a required `site`, an invariant the robots meet
    differently, `graded` enforced, and no `hold`; each part of the
    migration switched on or off."""
    migration = ("migration:\n  description: The fleet without holds.\n"
                 "  removed_members:\n    Grade: { OLD: B }\n")
    if map_hold:
        migration += "  removed_states:\n    Robot: { HOLD: READY }\n"
    if backfill:
        migration += "  backfill:\n    Robot: { site: '\"north\"' }\n"
    if admit:
        migration += ("  admit:\n    - { invariant: Robot.graded_when_ready, "
                      "reason: OverrideReason.LEGACY }\n")
    return """module: fleet
categories: [live, closed]
enumerations:
  Grade: [A, B]
  OverrideReason: [LEGACY]
types:
  Robot:
    description: A robot of the fleet.
    tracking: record
    attributes:
      serial: { type: string }
      grade: { type: Grade, optional: true }
      site: { type: string }
    states:
      READY: { category: live }
      RETIRED: { category: closed, final: true }
    invariants:
      graded_when_ready: { description: A ready robot is graded., expression: "state != Robot.READY or grade is not null" }
    conditions:
      graded: { description: The robot is graded., expression: grade is not null, remedy: self_serviceable }
    transitions:
      add: { kind: initial, to: READY, required_inputs: [serial, site], optional_inputs: [grade] }
      retire: { kind: external, from: [READY], to: RETIRED, guards: { graded: deny } }
""" + migration


@pytest.mark.parametrize("map_hold", [False, True])
@pytest.mark.parametrize("backfill", [False, True])
@pytest.mark.parametrize("admit", [False, True])
def test_a_fleets_dry_run_reports_alike(tee, map_hold, backfill, admit):
    """The fleet's robots, ready graded and not, held on a removed grade and
    retired, a pending proposal on a transition the version removes and
    another open change, dry run under each variant of the second version:
    every category the report names compared, removed states and members,
    required values, decisions and admissions, newly enforced clauses, lost
    transitions, invalidated proposals and superseded changes."""
    assert tee.post("/sandbox/load", {"files": [
        {"name": "fleet.yaml", "text": FLEET_V1}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()

    def add(serial: str, grade: str | None = None) -> str:
        inputs = {"serial": serial, **({"grade": grade} if grade else {})}
        v = tee.post("/v1/request", {"transition": "add", "actor": OP,
                                     "type": "Robot", "inputs": inputs})
        assert v["kind"] == "satisfied", v
        return v["object_id"]

    def take(xn: str, oid: str, **kw) -> dict:
        return tee.post("/v1/request", {"transition": xn, "actor": OP,
                                        "object_id": oid, **kw})

    graded, ungraded, held, retired = (add("R1", "A"), add("R2"),
                                       add("R3", "OLD"), add("R4", "B"))
    assert take("hold", held)["kind"] == "satisfied"
    assert take("retire", retired)["kind"] == "satisfied"
    assert take("hold", graded, propose=True)["kind"] == "satisfied"
    assert change(tee, "draft", source={"fleet": FLEET_V1},
                  source_ref="git:fleet@1b")["kind"] == "satisfied"
    drafted = change(tee, "draft",
                     source={"fleet": fleet_v2(map_hold, backfill, admit)},
                     source_ref="git:fleet@2")
    c = drafted["object_id"]
    report = tee.post("/v1/publish", {"actor": OP, "change": c,
                                      "dry_run": True})["report"]
    codes = {f["code"] for f in report["findings"]}
    assert "proposals_invalidated" in codes and "superseded" in codes, codes
    tee.post("/v1/request", {"transition": "submit", "actor": OP,
                             "object_id": c})
    tee.post("/v1/get", {"id": c})
    differences = sorted({f"{p} {w}: {what}"
                          for _m, p, w, what, _b in tee.found})
    assert differences == [], "\n".join(differences)


def fleet(tee) -> dict:
    """The fleet loaded at its first version: four robots, ready graded and
    not, held on a removed grade and retired, a pending proposal to hold the
    graded one, and another open change; their ids by name."""
    assert tee.post("/sandbox/load", {"files": [
        {"name": "fleet.yaml", "text": FLEET_V1}]})["loaded"]
    tee.post("/sandbox/reset", {})

    def add(serial: str, grade: str | None = None) -> str:
        inputs = {"serial": serial, **({"grade": grade} if grade else {})}
        return tee.post("/v1/request", {"transition": "add", "actor": OP,
                                        "type": "Robot",
                                        "inputs": inputs})["object_id"]

    ids = {"graded": add("R1", "A"), "ungraded": add("R2"),
           "held": add("R3", "OLD"), "retired": add("R4", "B")}
    tee.post("/v1/request", {"transition": "hold", "actor": OP,
                             "object_id": ids["held"]})
    tee.post("/v1/request", {"transition": "retire", "actor": OP,
                             "object_id": ids["retired"]})
    ids["proposal"] = tee.post("/v1/request", {
        "transition": "hold", "actor": OP, "object_id": ids["graded"],
        "propose": True})["object_id"]
    ids["other"] = change(tee, "draft", source={"fleet": FLEET_V1},
                          source_ref="git:fleet@1b")["object_id"]
    return ids


def read_back(tee, ids: dict) -> None:
    """Everything a person reads of the fleet after a publish, through both
    backends, so the tee compares it: an hour on, a robot added under
    whatever version is installed, the time each spent in each state, and
    every object, its history and its offer."""
    tee.post("/sandbox/clock", {"advance": "1 h"})
    added = tee.post("/v1/request", {"transition": "add", "actor": OP,
                                     "type": "Robot",
                                     "inputs": {"serial": "R9", "grade": "A",
                                                "site": "south"}})
    if added["kind"] == "satisfied":
        # written under the installed version, which its record names
        ids = {**ids, "added": added["object_id"]}
    tee.post("/v1/metric", {"actor": OP, "name": "Robot.time_in_state",
                            "keep": ["state"]})
    for oid in ids.values():
        tee.post("/v1/get", {"id": oid})
        tee.post("/v1/history", {"id": oid})
        tee.post("/v1/availability", {"actor": OP, "id": oid})


@pytest.mark.parametrize("map_hold,backfill,admit",
                         [(True, True, True), (True, True, False),
                          (False, True, True)])
def test_a_fleets_approval_installs_alike(tee, map_hold, backfill, admit):
    """A second version approved, or refused, through both backends: a
    stale approval refused, then the approval, installing every migration,
    admission, interval, superseded change and invalidated proposal, or
    refused as a decision unresolved; what the fleet holds after compared
    whole (publish-and-import.md §1 step 5)."""
    ids = fleet(tee)
    tee.found.clear()
    c = change(tee, "draft",
               source={"fleet": fleet_v2(map_hold, backfill, admit)},
               source_ref="git:fleet@2")["object_id"]
    ids["change"] = c
    submitted = tee.post("/v1/request", {"transition": "submit", "actor": OP,
                                         "object_id": c})
    version = tee.post("/v1/get", {"id": c})["version"]
    stale = tee.post("/v1/publish", {"actor": OP, "change": c,
                                     "expected_version": version - 1})
    assert stale["verdict"]["kind"] in ("stale", "unavailable"), stale
    approved = tee.post("/v1/publish", {"actor": OP, "change": c,
                                        "expected_version": version})
    if submitted["kind"] == "satisfied" and admit:
        assert approved["verdict"]["kind"] == "satisfied", approved
        # a state the version no longer declares, held before it, follows
        # those it declares in a page's order (D536)
        page = tee.post("/v1/metric", {"actor": OP,
                                       "name": "Robot.time_in_state",
                                       "keep": ["state"]})
        assert [r["dimensions"]["state"] for r in page["rows"]] == [
            "READY", "RETIRED", "HOLD"], page["rows"]
    elif submitted["kind"] == "satisfied":
        assert approved["verdict"].get("clause") == "decision_unresolved", \
            approved
    read_back(tee, ids)
    differences = sorted({f"{p} {w}: {what}"
                          for _m, p, w, what, _b in tee.found})
    assert differences == [], "\n".join(differences)


def test_an_impact_the_approver_was_not_shown_refuses_the_approval(tee):
    """A robot added after the change was submitted, which its version would
    leave violating an invariant, is an impact the attached report did not
    name, so the approval is refused until a refresh shows it (ADR-0147)."""
    ids = fleet(tee)
    tee.found.clear()
    c = change(tee, "draft", source={"fleet": fleet_v2(True, True, True)},
               source_ref="git:fleet@2")["object_id"]
    assert tee.post("/v1/request", {"transition": "submit", "actor": OP,
                                    "object_id": c})["kind"] == "satisfied"
    tee.post("/v1/request", {"transition": "add", "actor": OP,
                             "type": "Robot", "inputs": {"serial": "R5"}})
    version = tee.post("/v1/get", {"id": c})["version"]
    refused = tee.post("/v1/publish", {"actor": OP, "change": c,
                                       "expected_version": version})
    assert refused["verdict"].get("clause") == "impact_unchanged", refused
    # a refresh shows it, and the approval of what it shows installs
    assert tee.post("/v1/request", {"transition": "refresh", "actor": OP,
                                    "object_id": c})["kind"] == "satisfied"
    version = tee.post("/v1/get", {"id": c})["version"]
    assert tee.post("/v1/publish", {"actor": OP, "change": c,
                                    "expected_version": version}
                    )["verdict"]["kind"] == "satisfied"
    ids["change"] = c
    read_back(tee, ids)
    differences = sorted({f"{p} {w}: {what}"
                          for _m, p, w, what, _b in tee.found})
    assert differences == [], "\n".join(differences)


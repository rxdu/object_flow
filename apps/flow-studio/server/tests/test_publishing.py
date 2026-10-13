"""The reference runner's publishing held to the core's (ADR-0166): a change
to the declaration taken through both backends at once, every answer compared
whole by the tee, a difference failing the test as a finding against one of
them, as the backend comparison does for the examples.

It builds the core's service with the pinned toolchain, and fails, not skips,
without it.
"""

import json
import pathlib
import tempfile
import urllib.error
import urllib.request

import pytest

from compare import Tee, raw
from replay import Failed
from flowstudio.catalogue import Catalogue
from flowstudio.core import Core
from test_backends_agree import started

EXAMPLES = pathlib.Path(__file__).resolve().parents[4] / \
    "docs/design/flow-format/examples"
OP = {"id": "operator"}
# a receipt as the wire carries a file, by its reference (ADR-0173 decision 11)
TAXI = {"hash": "sha256:" + "ab" * 32, "size": 48213,
        "media_type": "application/pdf"}


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
    # the type as the version installed declares it
    tee.post("/v1/declaration", {"type": "Robot"})
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



# What R2's and R3's fleet does not reach, each case a version that names a
# category of the report no other case does; the modules are the core's
# tests' own (`publish.rs`), so the two backends meet what the core's tests
# hold it to.

SHOP_V1 = """module: shop
categories: [live, closed]
enumerations:
  Tier: [GOLD, SILVER]
  Size: [S, L]
types:
  Customer:
    description: A customer.
    tracking: record
    attributes:
      name: { type: string, optional: true, personal: true }
      nick: { type: string, optional: true }
      tier: { type: Tier, optional: true }
      size: { type: Size, optional: true }
    states:
      ACTIVE: { category: live }
      GONE: { category: closed, final: true }
    transitions:
      join: { kind: initial, to: ACTIVE, optional_inputs: [name, nick, tier, size] }
      leave: { kind: external, from: [ACTIVE], to: GONE }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
  Coupon:
    description: A coupon.
    tracking: record
    attributes:
      code: { type: string }
    states:
      ISSUED: { category: live }
      USED: { category: closed, final: true }
    transitions:
      issue: { kind: initial, to: ISSUED, required_inputs: [code] }
      redeem: { kind: external, from: [ISSUED], to: USED }
"""


def shop_v2(rename: bool = True, size: str = "string") -> str:
    """`tier` renamed, or dropped where `level` of its type is added, `size`
    an enumeration no more, `nick` personal and required by an invariant,
    `band` newly tracked, and `Coupon` dropped. Retyped to an enumeration,
    `size` stays tracked, and a metric counts its intervals holding no
    value: the attribute of the new type opens its own at the publish, as a
    newly tracked member does (D528)."""
    metric = "" if size == "string" else (
        "    metrics:\n      unsized: { description: Spans of no size., "
        "source: intervals(size), item: i, filter: i.value is null, "
        "expression: count() }\n")
    return """module: shop
categories: [live, closed]
enumerations:
  Tier: [GOLD, SILVER]
  Band: [LOW, HIGH]
types:
  Customer:
    description: A customer.
    tracking: record
    attributes:
      name: { type: string, optional: true, personal: true }
      nick: { type: string, optional: true, personal: true }
      level: { type: Tier, optional: true }
      size: { type: """ + size + """, optional: true }
      band: { type: Band, optional: true }
    states:
      ACTIVE: { category: live }
      GONE: { category: closed, final: true }
    invariants:
      nicknamed: { description: A rule., expression: nick is not null }
    transitions:
      join: { kind: initial, to: ACTIVE, optional_inputs: [name, nick, level, size, band] }
      leave: { kind: external, from: [ACTIVE], to: GONE }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
""" + metric + """migration:
  description: What the new version does to the objects of the old.
""" + ("  renamed_attributes:\n    Customer: { tier: level }\n" if rename
       else "")


LAB = """module: lab
categories: [live, closed]
enumerations:
  Grade: [A, B, OLD]
types:
  Sample:
    description: A sample.
    tracking: record
    attributes:
      label: { type: string }
    observations:
      checks:
        description: A check of a sample.
        kind: Check
        attributes:
          grade: { type: Grade }
    states:
      OPEN: { category: live }
      DONE: { category: closed, final: true }
    transitions:
      take: { kind: initial, to: OPEN, required_inputs: [label] }
      finish: { kind: external, from: [OPEN], to: DONE }
"""


def lab_v2(members: str, migration: str = "") -> str:
    """The lab with a kind's invariant, every check grading A."""
    return LAB.replace("Grade: [A, B, OLD]", members).replace(
        "          grade: { type: Grade }\n",
        "          grade: { type: Grade }\n        invariants:\n"
        "          graded_a: { description: A check grades A., "
        "expression: grade == Grade.A }\n") + migration


RACKS = """module: racks
categories: [live, closed]
types:
  Rack:
    description: A rack of bays.
    tracking: record
    attributes:
      label: { type: string }
      bays:
        reference: "Bay[]"
        aggregation: composite
        opposite: rack
        cascade:
          - { on: [close], transition: retire, limit: 10 }
    states:
      OPEN: { category: live }
      PARKED: { category: live }
      CLOSED: { category: closed, final: true }
    transitions:
      build: { kind: initial, to: OPEN, required_inputs: [label] }
      add_bay:
        kind: internal
        from: [OPEN]
        effect:
          - create: { type: Bay, transition: add, inputs: { rack: this } }
      park: { kind: external, from: [OPEN], to: PARKED }
      unpark: { kind: external, from: [PARKED], to: OPEN }
      close: { kind: external, from: [OPEN], to: CLOSED }
  Bay:
    description: A bay of a rack.
    tracking: record
    attributes:
      rack: { reference: Rack, opposite: bays }
    states:
      LIVE: { category: live }
      GONE: { category: closed, final: true }
    transitions:
      add: { kind: initial, to: LIVE, only_via: [Rack.add_bay], required_inputs: [rack] }
      retire: { kind: external, from: [LIVE], to: GONE }
"""

# the racks without PARKED, a parked rack mapped into CLOSED with its bays
# live (`publish-and-import.md` §2, a mapping into a final state)
RACKS_V2 = RACKS.replace("      PARKED: { category: live }\n", "").replace(
    "      park: { kind: external, from: [OPEN], to: PARKED }\n"
    "      unpark: { kind: external, from: [PARKED], to: OPEN }\n", "") + (
    "migration:\n  description: The racks without parking.\n"
    "  removed_states:\n    Rack: { PARKED: CLOSED }\n")


def fleet_examples(expect: str) -> str:
    """The fleet's examples file, retiring a graded robot."""
    return ("examples_for: fleet\nexamples:\n  a_graded_robot_is_retired:\n"
            "    description: A graded robot is retired.\n    steps:\n"
            "      - request: { type: Robot, transition: add, as: r, inputs: "
            "{ serial: R1, site: north, grade: Grade.A } }\n"
            "    request: { object: r, transition: retire }\n"
            f"    expect: {expect}\n")


def metered(text: str, currency: str = "USD",
            guards: str = "graded: deny, settled: deny",
            settled: str = "metric(priced) < USD 3.00") -> str:
    """A version whose `retire` also guards on a metric, robots priced in a
    currency, which a dry run consults no more than an evaluator
    (`publish-and-import.md` §1 step 3). Neither backend knows a peso's minor
    unit, so a request reading the metric in pesos is not run; the core
    cannot hold a literal in pesos either, which the runner can, so the
    peso's case reads only whether the metric has a value."""
    return text.replace(
        "migration:\n",
        "    metrics:\n      priced: { description: Robots priced in pesos., "
        "source: objects, item: o, expression: count() * PHP 1.00 }\n"
        "migration:\n", 1).replace(
        "remedy: self_serviceable }\n",
        "remedy: self_serviceable }\n      settled: { description: Few "
        f"robots priced., expression: \"{settled}\", "
        "remedy: dependent }\n", 1).replace(
        "guards: { graded: deny }", f"guards: {{ {guards} }}").replace(
        "PHP", currency)


def load(tee, name: str, text: str) -> None:
    assert tee.post("/sandbox/load", {"files": [
        {"name": f"{name}.yaml", "text": text}]})["loaded"]
    tee.post("/sandbox/reset", {})


def made(tee, tn: str, xn: str, **inputs) -> str:
    v = tee.post("/v1/request", {"transition": xn, "actor": OP, "type": tn,
                                 "inputs": inputs})
    assert v["kind"] == "satisfied", v
    return v["object_id"]


def took(tee, oid: str, xn: str, **inputs) -> dict:
    v = tee.post("/v1/request", {"transition": xn, "actor": OP,
                                 "object_id": oid, "inputs": inputs})
    assert v["kind"] == "satisfied", v
    return v


def dry_run(tee, source: dict) -> tuple[str, set]:
    """A change drafted with the source and dry run; its id and the codes
    its report names."""
    c = change(tee, "draft", source=source,
               source_ref="git:case@2")["object_id"]
    report = tee.post("/v1/publish", {"actor": OP, "change": c,
                                      "dry_run": True})["report"]
    return c, {f["code"] for f in report["findings"]}


def approved(tee, c: str) -> dict:
    assert tee.post("/v1/request", {"transition": "submit", "actor": OP,
                                    "object_id": c})["kind"] == "satisfied"
    version = tee.post("/v1/get", {"id": c})["version"]
    v = tee.post("/v1/publish", {"actor": OP, "change": c,
                                 "expected_version": version})
    assert v["verdict"]["kind"] == "satisfied", v
    return v


def alike(tee) -> None:
    differences = sorted({f"{p} {w}: {what}"
                          for _m, p, w, what, _b in tee.found})
    assert differences == [], "\n".join(differences)


def fleet_case(tee, text: str, extra: dict | None = None) -> set:
    fleet(tee)
    tee.found.clear()
    return dry_run(tee, {"fleet": text, **(extra or {})})[1]


def shop_case(tee, text: str) -> set:
    load(tee, "shop", SHOP_V1)
    made(tee, "Customer", "join", nick="b", tier="SILVER", size="L")
    made(tee, "Coupon", "issue", code="C1")
    tee.found.clear()
    return dry_run(tee, {"shop": text})[1]


def racks_case(tee, text: str) -> set:
    load(tee, "racks", RACKS)
    parked = made(tee, "Rack", "build", label="K1")
    took(tee, parked, "add_bay")
    took(tee, parked, "park")
    made(tee, "Rack", "build", label="K2")
    tee.found.clear()
    return dry_run(tee, {"racks": text})[1]


def lab_case(tee, text: str) -> set:
    load(tee, "lab", LAB)
    sample = made(tee, "Sample", "take", label="S1")
    made(tee, "Check", "record", subject=sample, grade="OLD")
    tee.found.clear()
    return dry_run(tee, {"lab": text})[1]


# each case: how its store is made and the version dry run over it, and a
# code its report must name, so a case that stops reaching its category
# fails rather than passes alike
CASES = {
    "unreachable_state": (lambda tee: fleet_case(tee, fleet_v2(
        False, True, True).replace(
        "      RETIRED: { category: closed, final: true }\n",
        "      HOLD: { category: live }\n"
        "      RETIRED: { category: closed, final: true }\n").replace(
        "      retire:", "      release: { kind: external, from: [HOLD], "
        "to: READY }\n      retire:")), "unreachable_state:Robot.HOLD"),
    "unmapped_member": (lambda tee: fleet_case(tee, fleet_v2(
        True, True, True).replace(
        "  removed_members:\n    Grade: { OLD: B }\n", "")),
        "removed_member:Grade.OLD"),
    "example_passes": (lambda tee: fleet_case(
        tee, fleet_v2(True, True, True), {"fleet.examples": fleet_examples(
            "{ verdict: applied, state: RETIRED }")}), "superseded"),
    "example_fails": (lambda tee: fleet_case(
        tee, fleet_v2(True, True, True), {"fleet.examples": fleet_examples(
            "{ verdict: unsatisfied, clause: graded, "
            "remedy: self_serviceable }")}), "example_failed"),
    "metric_guard": (lambda tee: fleet_case(
        tee, metered(fleet_v2(True, True, True))), "lost:Robot.retire"),
    "metric_guard_first": (lambda tee: fleet_case(
        tee, metered(fleet_v2(True, True, True),
                     guards="settled: deny, graded: deny")),
        "lost:Robot.retire"),
    "metric_guard_audited": (lambda tee: fleet_case(
        tee, metered(fleet_v2(True, True, True),
                     guards="graded: deny, settled: audit")),
        "gained:Robot.retire"),
    "example_not_run": (lambda tee: fleet_case(
        tee, metered(fleet_v2(True, True, True), "PHP",
                     settled="metric(priced) is not null"),
        {"fleet.examples": fleet_examples(
            "{ verdict: applied, state: RETIRED }")}), "example_not_run"),
    "type_changed_and_dropped_type": (
        lambda tee: shop_case(tee, shop_v2()), "type_changed:Customer.size"),
    "possible_rename": (lambda tee: shop_case(tee, shop_v2(rename=False)),
                        "possible_rename:Customer.tier"),
    "stranded": (lambda tee: racks_case(tee, RACKS_V2), "stranded:Rack.bays"),
    "observation_member": (lambda tee: lab_case(tee, lab_v2(
        "Grade: [A, B]", "migration:\n  description: The lab without OLD.\n"
        "  removed_members:\n    Grade: { OLD: B }\n")),
        "removed_member:Grade.OLD"),
    "observation_invariant": (
        lambda tee: lab_case(tee, lab_v2("Grade: [A, B, OLD]")),
        "observation_invariant:Check.graded_a"),
}


@pytest.mark.parametrize("case", sorted(CASES))
def test_a_category_of_the_report_is_reported_alike(tee, case):
    """Each category of a dry run's report that the fleet's variants do not
    reach, reached by a version over a store made for it, through both
    backends: the report compared whole, and the category named."""
    made_and_run, code = CASES[case]
    codes = made_and_run(tee)
    alike(tee)
    assert code in codes, sorted(codes)


def test_a_discharge_is_installed_alike(tee):
    """A violation the second version admits, and a third version without
    the invariant, which discharges the admission: both approved through
    both backends and the fleet read back whole after each
    (`publish-and-import.md` §1 step 3, D531)."""
    ids = fleet(tee)
    tee.found.clear()
    v2 = fleet_v2(True, True, True)
    ids["second"] = c = change(tee, "draft", source={"fleet": v2},
                               source_ref="git:fleet@2")["object_id"]
    approved(tee, c)
    read_back(tee, ids)
    v3 = v2.split("migration:")[0].replace(
        "    invariants:\n      graded_when_ready: { description: A ready "
        "robot is graded., expression: \"state != Robot.READY or grade is "
        "not null\" }\n", "")
    assert "graded_when_ready" not in v3
    ids["third"], codes = dry_run(tee, {"fleet": v3})
    assert "discharged:Robot.graded_when_ready" in codes, sorted(codes)
    approved(tee, ids["third"])
    read_back(tee, ids)
    alike(tee)


def test_a_kinds_new_invariant_binds_later_recordings_alike(tee):
    """A kind's invariant the recorded checks violate, published, binds the
    recordings after it and leaves those before it as they are (ADR-0146,
    D532), through both backends."""
    load(tee, "lab", LAB)
    sample = made(tee, "Sample", "take", label="S1")
    check = made(tee, "Check", "record", subject=sample, grade="OLD")
    tee.found.clear()
    c, codes = dry_run(tee, {"lab": lab_v2("Grade: [A, B, OLD]")})
    assert "observation_invariant:Check.graded_a" in codes, sorted(codes)
    approved(tee, c)
    refused = tee.post("/v1/request", {"transition": "record", "actor": OP,
                                       "type": "Check", "inputs": {
                                           "subject": sample,
                                           "grade": "OLD"}})
    assert refused["kind"] == "invariant_violated", refused
    made(tee, "Check", "record", subject=sample, grade="A")
    for oid in (sample, check, c):
        tee.post("/v1/get", {"id": oid})
        tee.post("/v1/history", {"id": oid})
    alike(tee)


def test_a_version_redacting_retyping_and_dropping_installs_alike(tee):
    """The shop's second version approved over an erased customer, a kept
    one and a coupon: the rename carried, the retyped `size` set aside, the
    newly personal `nick` redacted on the erased customer and the invariant
    its absence breaks admitted, `band` newly tracked, and `Coupon` dropped,
    its object kept and readable; then the version's requests, through both
    backends (`publish-and-import.md` §1 step 5, ADR-0146)."""
    load(tee, "shop", SHOP_V1)
    erased = made(tee, "Customer", "join", name="N", nick="a", tier="GOLD",
                  size="S")
    kept = made(tee, "Customer", "join", nick="b", tier="SILVER", size="L")
    coupon = made(tee, "Coupon", "issue", code="C1")
    took(tee, erased, "forget", reason="asked")
    tee.found.clear()
    c, codes = dry_run(tee, {"shop": shop_v2(size="Band")})
    # the redaction's absence an erasure admits, as it does its own, so it is
    # no decision
    assert "type_changed:Customer.size" in codes and not any(
        k.startswith("invariant:") for k in codes), sorted(codes)
    approved(tee, c)
    tee.post("/sandbox/clock", {"advance": "1 h"})
    joined = made(tee, "Customer", "join", nick="c", level="GOLD",
                  size="HIGH", band="LOW")
    issued = tee.post("/v1/request", {"transition": "issue", "actor": OP,
                                      "type": "Coupon",
                                      "inputs": {"code": "C2"}})
    assert issued["kind"] != "satisfied", issued
    took(tee, kept, "leave")
    for oid in (erased, kept, coupon, joined, c):
        tee.post("/v1/get", {"id": oid})
        tee.post("/v1/history", {"id": oid})
        tee.post("/v1/availability", {"actor": OP, "id": oid})
    tee.post("/v1/metric", {"actor": OP, "name": "Customer.time_in_state",
                            "keep": ["state"]})
    tee.post("/v1/declaration", {"type": "Customer"})
    # the kept customer and the erased one hold `size` of the new type
    # absent from the publish on, an interval of their own, and the
    # intervals of the old type are set aside, read by nothing (D528)
    page = tee.post("/v1/metric", {"actor": OP, "name": "Customer.unsized",
                                   "keep": []})
    assert [r["value"] for r in page["rows"]] == [2], page
    # the redaction admitted the invariant its absence broke, an override
    # `override_counts` counts (declaration-syntax.md §6.9)
    page = tee.post("/v1/metric", {"actor": OP,
                                   "name": "Customer.override_counts",
                                   "keep": []})
    assert [r["value"] for r in page["rows"]] == [1], page
    alike(tee)


# An erasure after a publish reaches every value personal under the version
# that wrote it (ADR-0146 decision 2, `storage-schema.md` §9 steps 1 and 9),
# as the core's tests hold it (`publish.rs`).

FORGET = "      forget: { kind: erasure, inputs: { reason: { type: string } } }\n"
TIERS = ("    metrics:\n      tiers: { description: Spans by tier., source: "
         "intervals(tier), item: i, dimensions: { tier: i.value }, "
         "expression: count() }\n")


def personal_tier(text: str) -> str:
    """A shop whose `tier` is personal, counting its intervals by value."""
    return text.replace("tier: { type: Tier, optional: true }",
                        "tier: { type: Tier, optional: true, personal: true }"
                        ).replace(FORGET, FORGET + TIERS, 1)


def published(tee, text: str, ref: str = "git:shop@2") -> None:
    c = change(tee, "draft", source={"shop": text},
               source_ref=ref)["object_id"]
    approved(tee, c)


def tiers(tee) -> list:
    page = tee.post("/v1/metric", {"actor": OP, "name": "Customer.tiers",
                                   "keep": ["tier"]})
    return [(r["dimensions"]["tier"], r["value"]) for r in page["rows"]]


@pytest.mark.parametrize("what", ["unmarked", "dropped", "retyped"])
def test_an_erasure_reaches_a_name_personal_when_written(tee, what):
    """A customer who joined while `name` was personal, erased after a
    version unmarks, drops or retypes it, keeps no name; one who joined
    after the unmarking keeps a name that was never personal."""
    text = {
        "unmarked": SHOP_V1.replace(
            "name: { type: string, optional: true, personal: true }",
            "name: { type: string, optional: true }"),
        "dropped": SHOP_V1.replace(
            "      name: { type: string, optional: true, personal: true }\n",
            "").replace("optional_inputs: [name, nick, tier, size]",
                        "optional_inputs: [nick, tier, size]"),
        "retyped": SHOP_V1.replace(
            "name: { type: string, optional: true, personal: true }",
            "name: { type: int, optional: true }"),
    }[what]
    load(tee, "shop", SHOP_V1)
    ana = made(tee, "Customer", "join", name="Ana", nick="a")
    tee.found.clear()
    published(tee, text)
    took(tee, ana, "forget", reason="asked")
    assert tee.post("/v1/get", {"id": ana})["attributes"].get("name") is None
    tee.post("/v1/history", {"id": ana})
    if what == "unmarked":
        bo = made(tee, "Customer", "join", name="Bo")
        took(tee, bo, "forget", reason="asked")
        assert tee.post("/v1/get", {"id": bo})["attributes"]["name"] == "Bo"
    alike(tee)


def test_an_erasure_clears_a_personal_tiers_intervals(tee):
    """With no publish, an erased customer's intervals of a personal
    enumeration hold no value, the one before the erasure included
    (`storage-schema.md` §9 step 9, D540)."""
    load(tee, "shop", personal_tier(SHOP_V1))
    ana = made(tee, "Customer", "join", tier="GOLD")
    tee.found.clear()
    took(tee, ana, "forget", reason="asked")
    assert tiers(tee) == [(None, 2)]
    alike(tee)


def test_an_erasure_clears_intervals_by_the_version_that_wrote_them(tee):
    """After a version unmarks `tier`, an erasure clears the interval
    written while it was personal and keeps the one written after."""
    v1 = personal_tier(SHOP_V1)
    load(tee, "shop", v1)
    bo = made(tee, "Customer", "join", tier="GOLD")
    tee.found.clear()
    published(tee, v1.replace(
        "tier: { type: Tier, optional: true, personal: true }",
        "tier: { type: Tier, optional: true }").replace(
        "      leave: { kind: external, from: [ACTIVE], to: GONE }\n",
        "      leave: { kind: external, from: [ACTIVE], to: GONE }\n"
        "      retier: { kind: internal, from: [ACTIVE], "
        "required_inputs: [tier] }\n"))
    tee.post("/sandbox/clock", {"advance": "1 min"})
    took(tee, bo, "retier", tier="SILVER")
    took(tee, bo, "forget", reason="asked")
    assert tiers(tee) == [("SILVER", 1), (None, 1)]
    tee.post("/v1/get", {"id": bo})
    alike(tee)


def test_an_erasure_follows_a_personal_name_through_its_renames(tee):
    """`name` renamed `alias` by one version and unmarked by the next is
    still erased from a customer whose value was written before the
    rename."""
    load(tee, "shop", SHOP_V1)
    ana = made(tee, "Customer", "join", name="Ana")
    tee.found.clear()
    renamed = SHOP_V1.replace(
        "name: { type: string, optional: true, personal: true }",
        "alias: { type: string, optional: true, personal: true }").replace(
        "optional_inputs: [name, nick, tier, size]",
        "optional_inputs: [alias, nick, tier, size]")
    published(tee, renamed + "migration:\n  description: The name is an "
              "alias.\n  renamed_attributes:\n    Customer: { name: alias }\n")
    published(tee, renamed.replace(
        "alias: { type: string, optional: true, personal: true }",
        "alias: { type: string, optional: true }"), "git:shop@3")
    assert tee.post("/v1/get", {"id": ana})["attributes"]["alias"] == "Ana"
    took(tee, ana, "forget", reason="asked")
    assert tee.post("/v1/get", {"id": ana})["attributes"].get("alias") is None
    tee.post("/v1/history", {"id": ana})
    alike(tee)


@pytest.mark.parametrize("erased_first", [False, True])
def test_a_tier_newly_personal_is_cleared_erased_before_or_after(tee,
                                                                 erased_first):
    """A version marking `tier` personal: an erasure after it clears the
    interval written before it, personal now; and an erasure before it is
    completed by the publish's redaction, which clears the interval as the
    erasure would have (`publish-and-import.md` §1 step 5, D527)."""
    load(tee, "shop", SHOP_V1.replace(FORGET, FORGET + TIERS, 1))
    ana = made(tee, "Customer", "join", tier="GOLD")
    tee.found.clear()
    if erased_first:
        took(tee, ana, "forget", reason="asked")
    published(tee, personal_tier(SHOP_V1))
    if not erased_first:
        took(tee, ana, "forget", reason="asked")
    assert tiers(tee) == [(None, 1 if erased_first else 2)]
    assert tee.post("/v1/get", {"id": ana})["attributes"].get("tier") is None
    tee.post("/v1/history", {"id": ana})
    alike(tee)



def test_a_declaration_names_its_version_and_where_it_came_from(tee):
    """`declaration` returns the version read, its source reference and
    digest, an earlier version's when named, and refuses a version the store
    never held (`library-api.md` §6, ADR-0135)."""
    ids = fleet(tee)
    tee.found.clear()
    first = tee.post("/v1/declaration", {"type": "Robot"})
    assert (first["version"], first["source_ref"]) == (1, "sandbox:load")
    assert first["source_digest"].startswith("sha256:")
    c = change(tee, "draft", source={"fleet": fleet_v2(True, True, True)},
               source_ref="git:fleet@2")["object_id"]
    approved(tee, c)
    second = tee.post("/v1/declaration", {"type": "Robot"})
    assert (second["version"], second["source_ref"]) == (2, "git:fleet@2")
    assert second["source_digest"] == tee.post(
        "/v1/get", {"id": c})["attributes"]["source_digest"]
    assert "site" in second["attributes"]
    earlier = tee.post("/v1/declaration", {"type": "Robot", "version": 1})
    assert earlier["version"] == 1 and "site" not in earlier["attributes"]
    assert earlier["source_digest"] == first["source_digest"]
    # refused alike: a version never held, version 0, which holds no type of
    # the fleet, and a version that is not a number
    for refused in ({"version": 9}, {"version": 0}, {"version": "one"}):
        with pytest.raises(Failed):
            tee.post("/v1/declaration", {"type": "Robot", **refused})
    del ids
    alike(tee)


def test_a_declaration_marks_each_dimension_naming_an_actor(tee):
    """Every metric dimension of a job whose value names an actor, the
    engineer, an object of `User`, which marks an actor identity: its own
    metrics' and the standard assignment metrics' over its assignee, and
    none of the rest (`library-api.md` §6, D451)."""
    loaded(tee, "service.yaml")
    tee.found.clear()
    job = tee.post("/v1/declaration", {"type": "ServiceJob"})
    assert job["person_dimensions"] == [
        "ServiceJob.cycle_time_by_assignee.engineer.assignee",
        "ServiceJob.inspection_pass_rate.engineer",
        "ServiceJob.open_work.engineer.assignee",
        "ServiceJob.time_in_state_by_holder.engineer.assignee",
        "ServiceJob.time_with_assignee.engineer.assignee",
        "ServiceJob.time_working.engineer",
    ]
    assert tee.post("/v1/declaration", {"type": "User"})[
        "person_dimensions"] == []
    alike(tee)


DESK = """module: desk
categories: [live, closed]
types:
  Agent:
    description: A person who works tickets.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human }
    states:
      ACTIVE: { category: live }
      LEFT: { category: closed, final: true }
    transitions:
      join: { kind: initial, to: ACTIVE, required_inputs: [login] }
      leave: { kind: external, from: [ACTIVE], to: LEFT }
  Queue:
    description: A queue tickets are raised in.
    tracking: record
    attributes:
      name: { type: string }
    states:
      OPEN: { category: live }
      SHUT: { category: closed, final: true }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [name] }
      shut: { kind: external, from: [OPEN], to: SHUT }
  Ticket:
    description: A ticket raised in a queue, owned by an agent.
    tracking: record
    attributes:
      queue: { reference: Queue }
      owner: { reference: Agent, optional: true, assignee: true }
    states:
      NEW: { category: live }
      DONE: { category: closed, final: true }
    transitions:
      raise: { kind: initial, to: NEW, required_inputs: [queue], optional_inputs: [owner] }
      resolve: { kind: external, from: [NEW], to: DONE }
    metrics:
      by_actor: { description: Requests by who made them., source: transitions, item: t, dimensions: { actor: t.actor_id }, expression: count() }
      by_queue: { description: Tickets by queue., source: objects, item: o, dimensions: { queue: o.queue }, expression: count() }
"""


def test_an_actors_id_is_marked_and_another_object_is_not(tee):
    """An actor's id, `t.actor_id`, names an actor, and a reference to an
    object of a type marking no actor identity, a queue, does not
    (`library-api.md` §6, D451)."""
    load(tee, "desk", DESK)
    tee.found.clear()
    assert tee.post("/v1/declaration", {"type": "Ticket"})[
        "person_dimensions"] == [
        "Ticket.by_actor.actor",
        "Ticket.cycle_time_by_assignee.owner.assignee",
        "Ticket.open_work.owner.assignee",
        "Ticket.time_in_state_by_holder.owner.assignee",
        "Ticket.time_with_assignee.owner.assignee",
    ]
    alike(tee)



def test_a_reports_marks_are_the_versions_declarations(tee):
    """A dry run names every declared type's dimensions that name an actor
    in the version under publish, and they are the marks `declaration`
    gives each type once that version is installed (`publish-and-import.md`
    §2, `library-api.md` §6, ADR-0114)."""
    loaded(tee, "service.yaml")
    tee.found.clear()
    v2 = (EXAMPLES / "service-v2.yaml").read_text()
    people = (EXAMPLES / "people.yaml").read_text()
    c = change(tee, "draft", source={"people": people, "service": v2},
               source_ref="git:service@2")["object_id"]
    report = tee.post("/v1/publish", {"actor": OP, "change": c,
                                      "dry_run": True})["report"]
    assert "person_dimensions" not in report["not_computed"]
    assert "ServiceJob.cycle_time_by_assignee.engineer.assignee" in \
        report["person_dimensions"]
    approved(tee, c)
    declared = sorted(d for tn in ("User", "Agent", "Service",
                                   "InspectionCheck", "ServiceJob")
                      for d in tee.post("/v1/declaration", {"type": tn})[
                          "person_dimensions"])
    assert report["person_dimensions"] == declared
    alike(tee)


# people.yaml's actor types and their transitions, the creation first, each
# taken by a person or a service (`taken_by: [human, service]`)
PEOPLE_TRANSITIONS = {"User": ["add", "rename", "leave"],
                      "Agent": ["issue", "revoke"],
                      "Service": ["register", "retire"]}


def test_a_report_states_each_invariants_form(tee):
    """A dry run names the form of each invariant a type of the version
    under publish declares, as the checks classify it, alike through both
    backends (`publish-and-import.md` §2, `declaration-syntax.md` §3.4,
    ADR-0055)."""
    loaded(tee, "service.yaml")
    tee.found.clear()
    v2 = (EXAMPLES / "service-v2.yaml").read_text()
    people = (EXAMPLES / "people.yaml").read_text()
    c = change(tee, "draft", source={"people": people, "service": v2},
               source_ref="git:service@2")["object_id"]
    report = tee.post("/v1/publish", {"actor": OP, "change": c,
                                      "dry_run": True})["report"]
    # people's actor types are taken by a person or a service, each of their
    # transitions generating an invariant of who took it (ADR-0176)
    taken = {f"{t}.{x}_taken_by": "local" for t, xs in PEOPLE_TRANSITIONS.items()
             for x in xs}
    assert report["invariant_forms"] == {
        "ServiceJob.photo_when_done": "local", **taken}
    alike(tee)


def test_a_proposal_of_what_is_not_proposable_is_refused_alike(tee):
    """Filing a proposal of a transition not marked proposable is refused
    not_proposable through both backends, its remedy to send it as a
    request instead (ADR-0173 decision 3, D553): the runner raised a fault
    and the core answered not_requestable, the verdict of an only-via
    transition. A proposal of a transition the type does not have is still
    unknown_transition."""
    loaded(tee, "service.yaml")
    tee.found.clear()
    ana = tee.post("/v1/request", {"transition": "add", "actor": OP,
                                   "type": "User",
                                   "inputs": {"login": "ana", "name": "Ana"}})
    job = tee.post("/v1/request", {"transition": "open",
                                   "actor": {"id": ana["object_id"]},
                                   "type": "ServiceJob",
                                   "inputs": {"engineer": ana["object_id"]}})
    v = tee.post("/v1/request", {"transition": "start",
                                 "actor": {"id": ana["object_id"]},
                                 "object_id": job["object_id"],
                                 "propose": True})
    assert (v["kind"], v["remedy"]) == ("not_proposable",
                                        "self_serviceable"), v
    unknown = tee.post("/v1/request", {"transition": "fly",
                                       "actor": {"id": ana["object_id"]},
                                       "object_id": job["object_id"],
                                       "propose": True})
    assert unknown["kind"] == "unknown_transition", unknown
    alike(tee)


def test_an_invalid_input_says_why_alike(tee):
    """A refused input says why, each from a closed set, through both
    backends alike (ADR-0173 decision 9): another JSON kind, a value not in
    its notation, an amount in another currency, more places than its
    scale, more digits than the store holds (D554, a 500 before), a
    reference naming nothing (D560, a 500 on the runner before), an input
    not taken, and inputs left out."""
    loaded(tee, "reimbursement.yaml")
    tee.found.clear()
    emp = tee.post("/v1/request", {"type": "Employee", "transition": "onboard",
                                   "actor": OP, "inputs": {"user_id": "alice"}})
    cat = tee.post("/v1/request", {"type": "ExpenseCategory",
                                   "transition": "define", "actor": OP,
                                   "inputs": {"name": "Travel",
                                              "policy_amount": "USD 500.00"}})
    claim = tee.post("/v1/request", {"type": "Claim", "transition": "draft",
                                     "actor": OP,
                                     "inputs": {"claimant": emp["object_id"],
                                                "title": "Taxis"}})
    line = {"amount": "USD 20.00", "category": cat["object_id"],
            "expense_date": "2025-12-30", "receipt": TAXI}

    def why(**change):
        given = {k: v for k, v in {**line, **change}.items() if v is not None}
        v = tee.post("/v1/request", {"object_id": claim["object_id"],
                                     "transition": "add_line", "actor": OP,
                                     "inputs": given})
        assert v["kind"] == "invalid_input", v
        return v["reasons"]

    assert why(amount=1500) == {"amount": "wrong_type"}
    assert why(amount="1500.00") == {"amount": "malformed"}
    assert why(amount="SGD 20.00") == {"amount": "wrong_currency"}
    assert why(amount="USD 1.234") == {"amount": "malformed"}
    # nothing around an amount, as a date is read (fourth production review)
    for spaced in ("USD  20.00", "USD 20.00 ", " USD 20.00"):
        assert why(amount=spaced) == {"amount": "malformed"}, spaced
    assert why(amount="USD 99999999999999999999.00") == {
        "amount": "out_of_range"}
    assert why(expense_date="2025-02-30") == {"expense_date": "malformed"}
    assert why(category="alice") == {"category": "no_such_object"}
    assert why(colour="red") == {"colour": "unknown_input"}
    assert why(amount=None, receipt=None) == {"amount": "missing",
                                              "receipt": "missing"}
    big = tee.post("/v1/request", {"type": "ExpenseCategory",
                                   "transition": "define", "actor": OP,
                                   "inputs": {"name": "x", "policy_amount":
                                              "USD 99999999999999999999.00"}})
    assert big["reasons"] == {"policy_amount": "out_of_range"}, big
    alike(tee)


def test_a_fault_names_a_key_and_an_absent_object_reads_null_alike(tee):
    """A key an operation does not take is a fault, as is an operation that
    does not exist, and a read of an object that does not exist answers
    null, through both backends alike (ADR-0173 decision 12, D555): a
    misspelt key had been read as absent, a filter's widening a query to
    every object, and history
    and availability of an absent object answered an empty list and a
    404."""
    loaded(tee, "reimbursement.yaml")
    tee.found.clear()
    for path, body, said in (
            ("/v1/get", {"id": "claim-1", "folow": True},
             "takes no key folow"),
            ("/v1/request", {"actor": OP, "type": "Claim",
                             "transition": "draft", "inputz": {}},
             "takes no key inputz"),
            ("/v1/nope", {}, "no operation /v1/nope"),
            # an argument left out is a fault, never an absent object (D570)
            ("/v1/get", {}, "get names an object's id"),
            ("/v1/history", {}, "history names an object's id")):
        with pytest.raises(Exception) as e:
            tee.post(path, body)
        assert said in str(e.value), e.value
    for path, body in (("/v1/get", {"id": "claim-999"}),
                       ("/v1/history", {"id": "claim-999"}),
                       ("/v1/availability", {"actor": OP, "id": "claim-999"})):
        assert tee.post(path, body) is None, path
    alike(tee)


def test_a_set_reference_is_read_by_its_members_alike(tee):
    """get answers a set reference by its members' ids, an end the other
    side stores as a reader reads it, and an empty list where there are
    none, never null, through both backends alike (flow-format.md §4.3,
    D552): a claim's lines read null with a line present."""
    loaded(tee, "reimbursement.yaml")
    tee.found.clear()
    emp = tee.post("/v1/request", {"type": "Employee", "transition": "onboard",
                                   "actor": OP, "inputs": {"user_id": "alice"}})
    cat = tee.post("/v1/request", {"type": "ExpenseCategory",
                                   "transition": "define", "actor": OP,
                                   "inputs": {"name": "Travel",
                                              "policy_amount": "USD 500.00"}})
    claim = tee.post("/v1/request", {"type": "Claim", "transition": "draft",
                                     "actor": OP,
                                     "inputs": {"claimant": emp["object_id"],
                                                "title": "Taxis"}})["object_id"]
    assert tee.post("/v1/get", {"id": claim})["attributes"]["lines"] == []
    tee.post("/v1/request", {"object_id": claim, "transition": "add_line",
                             "actor": OP,
                             "inputs": {"amount": "USD 20.00",
                                        "category": cat["object_id"],
                                        "expense_date": "2025-12-30",
                                        "receipt": TAXI}})
    lines = tee.post("/v1/get", {"id": claim})["attributes"]["lines"]
    assert len(lines) == 1 and lines[0].startswith("claimline-"), lines
    alike(tee)


def test_an_offer_says_it_depends_on_who_takes_it_alike(tee):
    """An offer of a transition that writes who takes it into what an
    invariant reads says so, since availability runs no effects and so no
    invariants, through both backends alike (ADR-0173 decision 1): a line's
    confirm_nature writes the actor's kind that confirmed_by_a_person
    reads, suggest_nature the kind its taken_by: [agent] records (ADR-0189),
    and amend writes nothing of the actor."""
    loaded(tee, "reimbursement.yaml")
    tee.found.clear()
    emp = tee.post("/v1/request", {"type": "Employee", "transition": "onboard",
                                   "actor": OP, "inputs": {"user_id": "alice"}})
    cat = tee.post("/v1/request", {"type": "ExpenseCategory",
                                   "transition": "define", "actor": OP,
                                   "inputs": {"name": "Travel",
                                              "policy_amount": "USD 500.00"}})
    claim = tee.post("/v1/request", {"type": "Claim", "transition": "draft",
                                     "actor": OP,
                                     "inputs": {"claimant": emp["object_id"],
                                                "title": "Taxis"}})["object_id"]
    added = tee.post("/v1/request", {"object_id": claim,
                                     "transition": "add_line", "actor": OP,
                                     "inputs": {"amount": "USD 20.00",
                                                "category": cat["object_id"],
                                                "expense_date": "2025-12-30",
                                                "receipt": TAXI}})
    line = tee.post("/v1/get", {"id": claim})["attributes"]["lines"][0]
    assert added["kind"] == "satisfied", added
    offers = {o["transition"]: o for o in tee.post(
        "/v1/availability", {"actor": OP, "id": line}) if not o["creates"]}
    assert offers["confirm_nature"]["actor_dependent"] is True
    assert offers["suggest_nature"]["actor_dependent"] is True
    assert offers["amend"]["actor_dependent"] is False
    alike(tee)


# a person decides a request, which the agent may only propose to approve
TAKEN = """module: taken
categories: [live, closed]

types:
  User:
    description: A person who may act.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states:
      ACTIVE: { description: Working., category: live }
      LEFT:   { description: Gone., category: closed, final: true }
    transitions:
      add:   { kind: initial, to: ACTIVE, required_inputs: [login] }
      leave: { kind: external, from: ACTIVE, to: LEFT }

  Agent:
    description: An agent that drafts and proposes.
    tracking: record
    attributes:
      key: { type: identity, actor_kind: agent, unique: true }
    states:
      ACTIVE:  { description: Working., category: live }
      RETIRED: { description: Retired., category: closed, final: true }
    transitions:
      issue:  { kind: initial, to: ACTIVE, required_inputs: [key] }
      retire: { kind: external, from: ACTIVE, to: RETIRED }

  Request:
    description: A request a person decides.
    tracking: record
    attributes:
      title: { type: string }
    states:
      OPEN:     { description: Waiting., category: live }
      APPROVED: { description: Approved., category: closed, final: true }
      REJECTED: { description: Rejected., category: closed, final: true }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [title] }
      retitle: { kind: internal, from: OPEN, required_inputs: [title] }
      approve: { kind: external, from: OPEN, to: APPROVED, taken_by: [human], proposable: true }
      reject:  { kind: external, from: OPEN, to: REJECTED, taken_by: [human, service] }
"""


def test_an_offer_is_decided_for_who_asks_alike(tee):
    """An offer that may be refused for who takes it, takes no input and asks
    no evaluator is simulated for who asks, so the agent is offered no
    approval it would be refused, and a person is (ADR-0176 decision 2);
    one awaiting inputs is left to them, marked, through both backends."""
    tee.found.clear()
    assert tee.post("/sandbox/load",
                    {"files": [{"name": "taken.yaml", "text": TAKEN}]})["loaded"]
    ana = tee.post("/v1/request", {"type": "User", "transition": "add",
                                   "actor": OP, "inputs": {"login": "ana"}})
    scout = tee.post("/v1/request", {"type": "Agent", "transition": "issue",
                                     "actor": OP, "inputs": {"key": "scout"}})
    req = tee.post("/v1/request", {"type": "Request", "transition": "open",
                                   "actor": OP,
                                   "inputs": {"title": "Laptop"}})["object_id"]

    def offers(actor: str) -> dict:
        return {o["transition"]: o for o in tee.post(
            "/v1/availability", {"actor": {"id": actor}, "id": req})
            if not o["creates"]}

    agent, person = offers(scout["object_id"]), offers(ana["object_id"])
    alike(tee)
    for xn in ("approve", "reject"):
        assert agent[xn]["availability"] == "unavailable", agent[xn]
        assert agent[xn]["verdict"]["kind"] == "invariant_violated"
        assert agent[xn]["verdict"]["failures"][0]["invariant"] == f"{xn}_taken_by"
        assert agent[xn]["verdict"]["remedy"] == "delegable"
        assert person[xn]["availability"] == "available", person[xn]
        assert agent[xn]["actor_dependent"] and person[xn]["actor_dependent"]
    # the agent's request is refused for who takes it, and told whether the
    # transition takes a proposal, which approve does and reject does not
    # (D582)
    for xn, proposable in (("approve", True), ("reject", False)):
        v = tee.post("/v1/request", {"object_id": req, "transition": xn,
                                     "actor": {"id": scout["object_id"]}})
        assert (v["kind"], v["remedy"], v["proposable"]) == (
            "invariant_violated", "delegable", proposable), v
    # awaiting its title, a retitle is anyone's, and nothing was written
    assert agent["retitle"]["availability"] == "available_with_input"
    assert tee.post("/v1/get", {"id": req})["state"] == "OPEN"
    assert tee.post("/v1/history", {"id": req})["events"][-1]["transition"] == "open"
    alike(tee)
    # retired in the store, the agent is refused every offer and request,
    # as a person who left is (ADR-0182 decision 6)
    for gone, xn in ((scout, "retire"), (ana, "leave")):
        assert tee.post("/v1/request", {"object_id": gone["object_id"],
                                        "transition": xn, "actor": OP}
                        )["kind"] == "satisfied"
        offered = offers(gone["object_id"])
        assert {(o["verdict"] or {}).get("clause") for o in offered.values()} \
            == {"actor_active"}, offered
        v = tee.post("/v1/request", {"object_id": req, "transition": "retitle",
                                     "actor": {"id": gone["object_id"]},
                                     "inputs": {"title": "Desk"}})
        assert (v["kind"], v["clause"], v["remedy"]) == (
            "unsatisfied", "actor_active", "dependent"), v
    alike(tee)


# a refusal's clause, unknown or false, what it names and whether it takes a
# proposal (library-api.md §4)
TOLD = """module: told
categories: [live, closed]

types:
  Team:
    description: A team, its lead and its tasks.
    tracking: record
    attributes:
      name:  { type: string, unique: true }
      lead:  { reference: Person, optional: true }
      tasks: { reference: "Task[]", opposite: team }
    states:
      ACTIVE: { description: Working., category: live }
      GONE:   { description: Disbanded., category: closed, final: true }
    conditions:
      no_open_tasks:
        description: None of its tasks is open.
        expression: none(t in tasks where t.state == Task.OPEN)
        remedy: dependent
      led_by_a_senior:
        description: Its lead is senior.
        expression: lead.seniority >= 3
        remedy: dependent
    transitions:
      form:   { kind: initial, to: ACTIVE, required_inputs: [name], optional_inputs: [lead] }
      disband: { kind: external, from: ACTIVE, to: GONE, guards: { no_open_tasks: deny } }
      promote: { kind: internal, from: ACTIVE, guards: { led_by_a_senior: deny } }
      retire:
        kind: internal
        from: ACTIVE
        only_via: [Task.close_team]
        guards: { no_open_tasks: deny }

  Person:
    description: A person who may lead a team.
    tracking: record
    attributes:
      login:     { type: identity, actor_kind: human, unique: true }
      seniority: { type: int, optional: true }
    states:
      ACTIVE: { description: Working., category: live }
      LEFT:   { description: Gone., category: closed, final: true }
    transitions:
      join:  { kind: initial, to: ACTIVE, required_inputs: [login], optional_inputs: [seniority] }
      leave: { kind: external, from: ACTIVE, to: LEFT }

  Task:
    description: A task of a team.
    tracking: record
    attributes:
      title: { type: string }
      team:  { reference: Team, opposite: tasks }
    states:
      OPEN: { description: Open., category: live }
      DONE: { description: Done., category: closed, final: true }
    conditions:
      team_active:
        description: Its team is active.
        expression: team.state == Team.ACTIVE
        remedy: dependent
      team_led:
        description: Its team has a lead.
        expression: team.lead is not null
        remedy: dependent
      lead_senior:
        description: Its team's lead is senior.
        expression: team.lead.seniority >= 3
        remedy: dependent
    transitions:
      open:  { kind: initial, to: OPEN, required_inputs: [title, team] }
      finish:
        kind: external
        from: OPEN
        to: DONE
        proposable: true
        guards: { team_active: deny, team_led: warn }
      escalate: { kind: internal, from: OPEN, guards: { lead_senior: deny } }
      close_team:
        kind: internal
        from: OPEN
        effect:
          - call: { target: team, transition: retire, inputs: {} }
"""


def test_a_refusal_tells_what_it_names_alike(tee):
    """A refusal tells, beside its clause and remedy, whether the clause was
    unknown rather than false, the objects its remedy points at, and
    whether the transition would take a proposal; a flag names its objects;
    an invariant violation names the objects in conflict; and a refused
    call's verdict comes whole, through both backends alike (DESIGN.md
    §5.5, library-api.md §4)."""
    tee.found.clear()
    assert tee.post("/sandbox/load",
                    {"files": [{"name": "told.yaml", "text": TOLD}]})["loaded"]

    def made(tn: str, xn: str, **inputs) -> str:
        v = tee.post("/v1/request", {"type": tn, "transition": xn,
                                     "actor": OP, "inputs": inputs})
        assert v["kind"] == "satisfied", v
        return v["object_id"]

    def asked(oid: str, xn: str) -> dict:
        return tee.post("/v1/request", {"object_id": oid, "transition": xn,
                                        "actor": OP})

    made("Person", "join", login="ana", seniority=5)
    bo = made("Person", "join", login="bo")
    ops = made("Team", "form", name="ops", lead=bo)
    dev = made("Team", "form", name="dev")
    qa = made("Team", "form", name="qa")
    t1 = made("Task", "open", title="deploy", team=ops)
    t2 = made("Task", "open", title="review", team=dev)
    assert asked(qa, "disband")["kind"] == "satisfied"
    t3 = made("Task", "open", title="audit", team=qa)
    # a dependent guard names what it read through, false
    v = asked(t3, "finish")
    assert (v["kind"], v["clause"], v["remedy"]) == (
        "unsatisfied", "team_active", "dependent"), v
    assert (v["unknown"], v["objects"], v["proposable"]) == (
        False, [qa], True), v
    # an aggregate names the elements its where selects
    v = asked(ops, "disband")
    assert (v["clause"], v["objects"], v["proposable"]) == (
        "no_open_tasks", [t1], False), v
    # a clause reading an absent value is unknown, naming what it read
    v = asked(ops, "promote")
    assert (v["clause"], v["unknown"], v["objects"]) == (
        "led_by_a_senior", True, [bo]), v
    # a path names each object it passes through: the team and its lead
    v = asked(t1, "escalate")
    assert (v["clause"], v["unknown"], v["objects"]) == (
        "lead_senior", True, [bo, ops]), v
    # a flag names its objects, false
    v = asked(t2, "finish")
    assert v["kind"] == "satisfied", v
    assert v["flags"] == [{"clause": "team_led", "remedy": "dependent",
                           "unknown": False, "objects": [dev]}], v
    # an invariant violation names the objects in conflict, each failure's
    # but the object it fails on: the team holding the name; the team the
    # refused request was creating is named by no id, since it does not
    # exist and the next team takes its id (D581)
    v = tee.post("/v1/request", {"type": "Team", "transition": "form",
                                 "actor": OP, "inputs": {"name": "ops"}})
    assert v["kind"] == "invariant_violated", v
    assert [f["invariant"] for f in v["failures"]] == ["name_unique"] * 2, v
    assert [f["object_id"] for f in v["failures"]] == [ops, None], v
    assert v["objects"] == [ops] and v["proposable"] is False, v
    later = tee.post("/v1/request", {"type": "Team", "transition": "form",
                                     "actor": OP, "inputs": {"name": "qa2"}})
    assert later["kind"] == "satisfied", later
    # a refused call's verdict comes whole, taking no proposal
    v = asked(t1, "close_team")
    assert v["kind"] == "call_refused", v
    called = v["verdict"]
    assert (called["clause"], called["objects"], called["unknown"],
            called["proposable"]) == ("Team.no_open_tasks", [t1], False,
                                      False), v
    # an offer's refusal tells as much
    offers = {o["transition"]: o for o in tee.post(
        "/v1/availability", {"actor": OP, "id": ops}) if not o["creates"]}
    assert offers["disband"]["verdict"]["objects"] == [t1], offers["disband"]
    alike(tee)


def test_an_actor_is_named_by_identity_or_id_alike(tee):
    """A request names its actor by the object's id or by the actor
    identity it holds, the id resolved first, through both backends alike
    (ADR-0173 decision 13, D560): the runner's wire refused an identity
    as actor_known, which the core took."""
    loaded(tee, "reimbursement.yaml")
    tee.found.clear()
    emp = tee.post("/v1/request", {"type": "Employee", "transition": "onboard",
                                   "actor": OP, "inputs": {"user_id": "alice@corp"}})
    for who in ("alice@corp", emp["object_id"]):
        v = tee.post("/v1/request", {"type": "Claim", "transition": "draft",
                                     "actor": {"id": who},
                                     "inputs": {"claimant": emp["object_id"],
                                                "title": "Taxis"}})
        assert v["kind"] == "satisfied", (who, v)
    nobody = tee.post("/v1/request", {"type": "Claim", "transition": "draft",
                                      "actor": {"id": "bob@corp"},
                                      "inputs": {"claimant": emp["object_id"],
                                                 "title": "Taxis"}})
    assert (nobody["kind"], nobody.get("clause")) == ("unsatisfied",
                                                      "actor_known"), nobody
    alike(tee)


def test_a_derived_amount_summed_over_nothing_is_its_currencys_zero(tee):
    """A claim with no lines has a total summed over nothing, which is the
    zero of the amount its lines would sum, at the currency's minor unit,
    alike through both backends (`declaration-syntax.md` §8.3); the core
    gave the integer 0 until 2026-10-08, where the runner gave the zero."""
    loaded(tee, "reimbursement.yaml")
    tee.found.clear()
    alice = tee.post("/v1/request", {
        "type": "Employee", "transition": "onboard", "actor": OP,
        "inputs": {"user_id": "alice"}})["object_id"]
    claim = tee.post("/v1/request", {
        "type": "Claim", "transition": "draft", "actor": OP,
        "inputs": {"claimant": alice, "title": "Taxis in May"}})["object_id"]
    got = tee.post("/v1/get", {"id": claim})
    assert got["attributes"]["total"] == "USD 0.00"
    alike(tee)


TALLY = """module: tally
categories: [live, closed]
types:
  Batch:
    description: A batch of weighed lots.
    tracking: record
    attributes:
      lots: { reference: "Lot[]", opposite: batch, aggregation: composite, survives: true }
    states:
      OPEN: { category: live }
      DONE: { category: closed, final: true }
    derived_attributes:
      weight: { description: The lots' weight together., expression: "sum(l in lots: l.kg)" }
    transitions:
      open:   { kind: initial, to: OPEN }
      add:    { kind: internal, from: [OPEN], inputs: { kg: { type: "decimal(8,2)" } }, effect: [ { create: { type: Lot, transition: weigh, inputs: { batch: this, kg: inputs.kg } } } ] }
      finish: { kind: external, from: [OPEN], to: DONE }
  Lot:
    description: A lot of a batch, weighed.
    tracking: record
    attributes:
      batch: { reference: Batch, opposite: lots }
      kg:    { type: "decimal(8,2)" }
    states:
      IN:  { category: live }
      OUT: { category: closed, final: true }
    transitions:
      weigh: { kind: initial, to: IN, required_inputs: [batch, kg], only_via: [Batch.add] }
      drop:  { kind: external, from: [IN], to: OUT }
"""


def test_a_derived_decimal_summed_over_nothing_is_zero_at_its_scale(tee):
    """A batch with no lots weighs a sum over nothing, which is the decimal
    zero at the scale its lots' weights have, alike through both backends
    (`declaration-syntax.md` §8.3); the core gave the integer 0 until
    2026-10-08, where the runner gave the zero."""
    load(tee, "tally", TALLY)
    tee.found.clear()
    batch = made(tee, "Batch", "open")
    got = tee.post("/v1/get", {"id": batch})
    assert got["attributes"]["weight"] == "0.00"
    alike(tee)


def test_a_report_names_the_transitions_that_could_violate_each_invariant(tee):
    """A dry run names, for each invariant a type of the version under
    publish declares, the transitions that could violate it, alike through
    both backends (`DESIGN.md` §5.6, `publish-and-import.md` §2): a job's
    photo rule is reached by its creation and by the transitions moving
    its state, and not by a reassignment, which writes neither."""
    loaded(tee, "service.yaml")
    tee.found.clear()
    v2 = (EXAMPLES / "service-v2.yaml").read_text()
    people = (EXAMPLES / "people.yaml").read_text()
    c = change(tee, "draft", source={"people": people, "service": v2},
               source_ref="git:service@2")["object_id"]
    report = tee.post("/v1/publish", {"actor": OP, "change": c,
                                      "dry_run": True})["report"]
    # each of people's generated invariants of who took a transition, which
    # the transition and the creation, writing the record of who, reach
    # (ADR-0176)
    taken = {f"{t}.{x}_taken_by": sorted({f"{t}.{x}", f"{t}.{xs[0]}"})
             for t, xs in PEOPLE_TRANSITIONS.items() for x in xs}
    assert report["could_violate"] == {"ServiceJob.photo_when_done": [
        "ServiceJob.finish", "ServiceJob.open", "ServiceJob.start"], **taken}
    alike(tee)


def test_a_file_is_its_reference_alike(tee):
    """A file travels as its reference, `{hash, size, media_type}`, which
    `get` reads back as it was given; a name, a key it lacks or one it does
    not take, a hash not SHA-256's and a media type with no subtype are each
    refused, alike through both backends (ADR-0173 decision 11, D559)."""
    loaded(tee, "approvals.yaml")
    tee.found.clear()
    claim = tee.post("/v1/request", {"type": "ExpenseClaim",
                                     "transition": "file_claim", "actor": OP,
                                     "inputs": {"amount": "SGD 42.50"}})
    receipt = {"hash": "sha256:" + "ab" * 32, "size": 48213,
               "media_type": "application/pdf"}
    v = tee.post("/v1/request", {"object_id": claim["object_id"],
                                 "transition": "attach_receipt", "actor": OP,
                                 "inputs": {"receipt": receipt}})
    assert v["kind"] == "satisfied", v
    got = tee.post("/v1/get", {"id": claim["object_id"]})
    assert got["attributes"]["receipt"] == receipt
    for wrong, why in (("taxi.pdf", "wrong_type"),
                       ({"hash": receipt["hash"], "size": 1}, "malformed"),
                       (dict(receipt, name="taxi.pdf"), "malformed"),
                       (dict(receipt, hash="md5:abc"), "malformed"),
                       (dict(receipt, size=-1), "malformed"),
                       (dict(receipt, media_type="pdf"), "malformed")):
        v = tee.post("/v1/request", {"object_id": claim["object_id"],
                                     "transition": "attach_receipt",
                                     "actor": OP,
                                     "inputs": {"receipt": wrong}})
        assert (v["kind"], v.get("reasons")) == (
            "invalid_input", {"receipt": why}), (wrong, v)
    alike(tee)


def test_a_metric_by_a_recorded_identity_names_a_person_alike(tee):
    """A metric whose dimension reads the identity an approval recorded
    from `actor.id` names a person, and `declaration` marks it, alike
    through both backends (D565); the guide recommends recording who acted
    that way, and the marking had followed only references to actors."""
    from flowstudio.scripts import flows
    assert tee.post("/sandbox/load", {"files": [
        {"name": "approve.yaml", "text": flows.IDENTITY_FLOW}]})["loaded"]
    tee.found.clear()
    d = tee.post("/v1/declaration", {"type": "Request"})
    assert d["person_dimensions"] == ["Request.approvals_by_approver.approver"]
    alike(tee)


def test_a_metric_filter_reads_its_rows_through_the_item_alike(tee):
    """A metric read's filter reads its rows through the metric's item; one
    starting from a name the rows do not bind is refused malformed_filter,
    as is one that does not parse, through both backends alike (D569): the
    core answered no rows for a member's bare name and 501 for an unknown
    one, and the runner no rows for both."""
    loaded(tee, "reimbursement.yaml")
    tee.found.clear()
    tee.post("/v1/request", {"type": "Employee", "transition": "onboard",
                             "actor": OP, "inputs": {"user_id": "alice"}})
    read = lambda f: tee.post("/v1/metric", {  # noqa: E731
        "actor": OP, "name": "Employee.time_in_state", "filter": f})
    for wrong in ("state == Employee.ACTIVE", "nonsense == 3",
                  "i.state =="):
        assert read(wrong)["refused"] == "malformed_filter", wrong
    assert read("i.state == Employee.ACTIVE")["refused"] is None
    alike(tee)


def test_a_proposals_inputs_are_answered_as_json_alike(tee):
    """A built-in attribute holding JSON is answered as JSON, never as the
    text it is kept as: a proposal's inputs, as a request's are written
    (ADR-0182 decision 9), through both backends."""
    tee.found.clear()
    assert tee.post("/sandbox/load",
                    {"files": [{"name": "taken.yaml", "text": TAKEN}]})["loaded"]
    scout = tee.post("/v1/request", {"type": "Agent", "transition": "issue",
                                     "actor": OP, "inputs": {"key": "scout"}})
    req = tee.post("/v1/request", {"type": "Request", "transition": "open",
                                   "actor": OP,
                                   "inputs": {"title": "Laptop"}})["object_id"]
    filed = tee.post("/v1/request", {"object_id": req, "transition": "approve",
                                     "actor": {"id": scout["object_id"]},
                                     "propose": True})
    assert filed["kind"] == "satisfied", filed
    proposal = tee.post("/v1/get", {"id": filed["object_id"]})
    assert proposal["type"] == "Proposal", proposal
    assert proposal["attributes"]["inputs"] == {}, proposal["attributes"]
    alike(tee)



BOUNDED = """module: tally
categories: [live, closed]
types:
  Tally:
    description: A count.
    tracking: record
    attributes:
      n: { type: int }
    states:
      OPEN: { category: live, description: Open. }
      DONE: { category: closed, final: true, description: Done. }
    conditions:
      small: { description: Small., expression: n * 2 < 100, remedy: unreachable_from_here }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [n] }
      add:
        kind: internal
        from: OPEN
        inputs: { by: { type: int } }
        effect:
          - assign: { location: n, expr: n + inputs.by }
      halve:
        kind: internal
        from: OPEN
        inputs: { by: { type: int } }
        effect:
          - assign: { location: n, expr: n / inputs.by }
      negate:
        kind: internal
        from: OPEN
        effect:
          - assign: { location: n, expr: -n }
      check: { kind: internal, from: OPEN, guards: { small: deny } }
      close: { kind: external, from: OPEN, to: DONE }
"""


LEASES = """module: leases
categories: [live, closed]
types:
  Lease:
    description: A lease that runs to a day and an instant.
    tracking: record
    attributes:
      ends_at: { type: timestamp }
      ends_on: { type: date }
    states:
      ON: { category: live, description: Running. }
      ENDED: { category: closed, final: true, description: Ended. }
    conditions:
      room: { description: A month is left in the calendar., expression: ends_on + 30 days > ends_on, remedy: unreachable_from_here }
    transitions:
      start: { kind: initial, to: ON, required_inputs: [ends_at, ends_on] }
      renew: { kind: internal, from: ON, guards: { room: deny } }
      end: { kind: external, from: ON, to: ENDED }
      extend_at:
        kind: internal
        from: ON
        effect:
          - assign: { location: ends_at, expr: ends_at + 30 days }
      extend_on:
        kind: internal
        from: ON
        effect:
          - assign: { location: ends_on, expr: ends_on + 30 days }
"""


def test_a_time_moved_past_the_calendar_is_refused_alike(tee):
    """An effect moving a time or a date past year 9999, as one from the
    legacy "no end" 9999-12-31 does, refuses its request value_in_range,
    naming its object, alike through both backends, where the core wrote
    year 10000, which no read took back, so its object and its type answered
    500 for good (fifth production review BRICK, ADR-0190)."""
    assert tee.post("/sandbox/load", {"files": [
        {"name": "leases.yaml", "text": LEASES}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()

    def start(at: str, on: str) -> str:
        v = tee.post("/v1/request", {"transition": "start", "actor": OP, "type": "Lease",
                                     "inputs": {"ends_at": at, "ends_on": on}})
        assert v["kind"] == "satisfied", v
        return v["object_id"]

    usual = start("2026-12-31T00:00:00Z", "2026-12-31")
    endless = start("9999-12-31T00:00:00Z", "9999-12-31")
    for xn in ("extend_at", "extend_on"):
        v = tee.post("/v1/request", {"transition": xn, "actor": OP, "object_id": endless})
        assert (v["kind"], v.get("clause"), v.get("objects")) == (
            "unsatisfied", "value_in_range", [endless]), v
        v = tee.post("/v1/request", {"transition": xn, "actor": OP, "object_id": usual})
        assert v["kind"] == "satisfied", v
    # a guard over a date moved past it is unknown, and refuses
    v = tee.post("/v1/request", {"transition": "renew", "actor": OP, "object_id": endless})
    assert (v["kind"], v.get("clause"), v.get("unknown")) == ("unsatisfied", "room", True), v
    assert tee.post("/v1/request", {"transition": "renew", "actor": OP, "object_id": usual})["kind"] == "satisfied"
    # what was refused left the object readable, as it was
    assert tee.post("/v1/get", {"id": endless})["attributes"]["ends_on"] == "9999-12-31"
    alike(tee)


def test_an_effects_arithmetic_past_its_bound_is_refused_alike(tee):
    """An effect whose integer arithmetic passes 64 bits, or divides by
    zero, refuses its request as the generated clause value_in_range, naming
    its object, alike through both backends, where the core stopped 501 and
    the runner, its integers unbounded, stored the value; a guard over the
    same arithmetic stays unknown and refuses (production review F5,
    ADR-0185)."""
    loaded = tee.post("/sandbox/load", {"files": [
        {"name": "tally.yaml", "text": BOUNDED}]})
    assert loaded["loaded"], loaded
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    opened = tee.post("/v1/request", {"transition": "open", "actor": OP, "type": "Tally",
                                      "inputs": {"n": 9223372036854775000}})
    assert opened["kind"] == "satisfied", opened
    t = opened["object_id"]

    least = tee.post("/v1/request", {"transition": "open", "actor": OP, "type": "Tally",
                                     "inputs": {"n": "-9223372036854775808"}})
    assert least["kind"] == "satisfied", least

    def take(xn: str, oid: str = t, **inputs) -> dict:
        return tee.post("/v1/request", {"transition": xn, "actor": OP, "object_id": oid, "inputs": inputs})

    for xn, oid, inputs in (("add", t, {"by": 10000}), ("halve", t, {"by": 0}),
                            ("negate", least["object_id"], {})):
        v = take(xn, oid, **inputs)
        assert (v["kind"], v.get("clause"), v.get("remedy"), v.get("objects")) == (
            "unsatisfied", "value_in_range", "unreachable_from_here", [oid]), v
    guarded = take("check")
    assert (guarded["kind"], guarded.get("clause"), guarded.get("unknown")) == ("unsatisfied", "small", True), guarded
    assert take("add", by=7)["kind"] == "satisfied"
    alike(tee)


DATED = """module: dated
categories: [live, closed]
types:
  Visit:
    description: A visit.
    tracking: record
    attributes:
      on: { type: date }
      at: { type: timestamp }
    states:
      DUE: { category: live, description: Due. }
      DONE: { category: closed, final: true, description: Done. }
    transitions:
      book: { kind: initial, to: DUE, required_inputs: [on, at] }
      close: { kind: external, from: DUE, to: DONE }
"""


def test_a_date_and_a_timestamp_are_read_exactly_alike(tee):
    """A date and a timestamp an input gives are read as RFC 3339 writes
    them, each field its digits exactly, the year 0001 to 9999, at UTC,
    alike through both backends: the core took a space, a sign, a one-digit
    month and a year past 9999, and the runner a one-digit month and any
    offset (second production review, minor)."""
    loaded = tee.post("/sandbox/load", {"files": [
        {"name": "dated.yaml", "text": DATED}]})
    assert loaded["loaded"], loaded
    tee.post("/sandbox/reset", {})
    tee.found.clear()

    def book(on: str, at: str) -> dict:
        return tee.post("/v1/request", {"transition": "book", "actor": OP, "type": "Visit",
                                        "inputs": {"on": on, "at": at}})

    good_at = "2026-10-04T09:30:00Z"
    for on in ("0001-01-01", "9999-12-31", "2024-02-29"):
        assert book(on, good_at)["kind"] == "satisfied", on
    for at in ("2026-10-04T09:30Z", "2026-10-04T09:30:00+00:00"):
        assert book("2026-10-04", at)["kind"] == "satisfied", at
    for on in ("0000-01-01", "10000-01-01", "2026-2-3", " 2026-10-01", "+2026-10-01"):
        v = book(on, good_at)
        assert (v["kind"], v.get("inputs")) == ("invalid_input", ["on"]), (on, v)
    for at in ("2026-10-04T9:30:00Z", "2026-10-04T09:30:00+08:00", "2026-10-04T09:30:00+0000",
               "10000-01-01T00:00:00Z", "2026-2-4T09:30:00Z"):
        v = book("2026-10-04", at)
        assert (v["kind"], v.get("inputs")) == ("invalid_input", ["at"]), (at, v)
    alike(tee)


ROSTER = """module: roster
publishing: { taken_by: [human] }
categories: [live, closed]
types:
  User:
    description: A person who works the roster.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states:
      ACTIVE: { category: live, description: Active. }
      GONE: { category: closed, final: true, description: Gone. }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [login] }
      leave: { kind: external, from: ACTIVE, to: GONE }
  Bot:
    description: An agent of the roster.
    tracking: record
    attributes:
      handle: { type: identity, actor_kind: agent, unique: true }
    states:
      ACTIVE: { category: live, description: Active. }
      GONE: { category: closed, final: true, description: Gone. }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [handle] }
      retire: { kind: external, from: ACTIVE, to: GONE }
"""


def test_a_version_is_approved_only_by_the_declared_kinds_alike(tee):
    """A version the installed one says only a person approves is refused
    to an agent as publish_taken_by, delegable, naming the change, and
    approved by a person, alike through both backends (third production
    review AGENT-APPROVE, ADR-0187 decision 1)."""
    loaded = tee.post("/sandbox/load", {"files": [
        {"name": "roster.yaml", "text": ROSTER}]})
    assert loaded["loaded"], loaded
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    for tn, key, name in (("User", "login", "ana"), ("Bot", "handle", "bot")):
        v = tee.post("/v1/request", {"transition": "add", "actor": OP, "type": tn,
                                     "inputs": {key: name}})
        assert v["kind"] == "satisfied", v
    roomier = ROSTER.replace("      login: { type: identity, actor_kind: human, unique: true }\n",
                             "      login: { type: identity, actor_kind: human, unique: true }\n"
                             "      team: { type: string, optional: true }\n", 1)
    c = change(tee, "draft", source={"roster": roomier}, source_ref="git:roster@2")["object_id"]
    assert tee.post("/v1/request", {"transition": "submit", "actor": OP,
                                    "object_id": c})["kind"] == "satisfied"
    version = tee.post("/v1/get", {"id": c})["version"]
    refused = tee.post("/v1/publish", {"actor": {"id": "bot"}, "change": c,
                                       "expected_version": version})
    v = refused["verdict"]
    assert (v["kind"], v.get("remedy"), v.get("objects")) == ("invariant_violated", "delegable", [c]), refused
    assert [f["invariant"] for f in v["failures"]] == ["publish_taken_by"], refused
    # the version approved declares service, read only once it is installed
    services = roomier.replace("publishing: { taken_by: [human] }", "publishing: { taken_by: [service] }", 1)
    c = change(tee, "draft", source={"roster": services}, source_ref="git:roster@3")["object_id"]
    assert tee.post("/v1/request", {"transition": "submit", "actor": OP,
                                    "object_id": c})["kind"] == "satisfied"
    version = tee.post("/v1/get", {"id": c})["version"]
    approved = tee.post("/v1/publish", {"actor": {"id": "ana"}, "change": c,
                                        "expected_version": version})
    assert approved["verdict"]["kind"] == "satisfied", approved
    # under it a person is refused, and the store's operator approves
    c = change(tee, "draft", source={"roster": roomier}, source_ref="git:roster@4")["object_id"]
    assert tee.post("/v1/request", {"transition": "submit", "actor": OP,
                                    "object_id": c})["kind"] == "satisfied"
    version = tee.post("/v1/get", {"id": c})["version"]
    refused = tee.post("/v1/publish", {"actor": {"id": "ana"}, "change": c,
                                       "expected_version": version})
    assert refused["verdict"]["kind"] == "invariant_violated", refused
    approved = tee.post("/v1/publish", {"actor": OP, "change": c, "expected_version": version})
    assert approved["verdict"]["kind"] == "satisfied", approved
    alike(tee)


def test_with_no_publishing_rule_an_agent_does_not_approve_alike(tee):
    """A version that declares an agent kind of actor and no
    publishing.taken_by refuses the agent's approval of the next version as
    publish_taken_by, delegable, and takes a person's, alike through both
    backends, where the agent approved its own change (eighth production
    review, ADR-0195 decision 4)."""
    unruled = ROSTER.replace("publishing: { taken_by: [human] }\n", "", 1)
    loaded = tee.post("/sandbox/load", {"files": [{"name": "roster.yaml", "text": unruled}]})
    assert loaded["loaded"], loaded
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    for tn, key, name in (("User", "login", "ana"), ("Bot", "handle", "bot")):
        v = tee.post("/v1/request", {"transition": "add", "actor": OP, "type": tn,
                                     "inputs": {key: name}})
        assert v["kind"] == "satisfied", v
    roomier = unruled.replace("      login: { type: identity, actor_kind: human, unique: true }\n",
                              "      login: { type: identity, actor_kind: human, unique: true }\n"
                              "      team: { type: string, optional: true }\n", 1)
    # the agent drafts and submits it, and its approval is refused
    c = tee.post("/v1/request", {"transition": "draft", "actor": {"id": "bot"}, "type": "DeclarationChange",
                                 "inputs": {"source": {"roster": roomier}, "source_ref": "git:roster@2"}})["object_id"]
    assert tee.post("/v1/request", {"transition": "submit", "actor": {"id": "bot"},
                                    "object_id": c})["kind"] == "satisfied"
    version = tee.post("/v1/get", {"id": c})["version"]
    refused = tee.post("/v1/publish", {"actor": {"id": "bot"}, "change": c,
                                       "expected_version": version})
    v = refused["verdict"]
    assert (v["kind"], v.get("remedy"), v.get("objects")) == ("invariant_violated", "delegable", [c]), refused
    assert [f["invariant"] for f in v["failures"]] == ["publish_taken_by"], refused
    # a person approves it
    approved = tee.post("/v1/publish", {"actor": {"id": "ana"}, "change": c,
                                        "expected_version": version})
    assert approved["verdict"]["kind"] == "satisfied", approved
    alike(tee)


def test_an_agent_moves_on_only_its_own_change_alike(tee):
    """Where the version declares no publishing, an agent's reject, withdraw, reopen and refresh of a person's
    change are refused publish_taken_by, delegable, as its approval is, and its own change it moves on; a request
    of a change's publish is not_requestable, naming publish; alike through both backends, where an agent rejected,
    reopened and revised a person's change, and the request answered 501 (ninth production review N-1, N-2,
    ADR-0196 decisions 5 and 7)."""
    unruled = ROSTER.replace("publishing: { taken_by: [human] }\n", "", 1)
    assert tee.post("/sandbox/load", {"files": [{"name": "roster.yaml", "text": unruled}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    for tn, key, name in (("User", "login", "ana"), ("Bot", "handle", "bot")):
        assert tee.post("/v1/request", {"transition": "add", "actor": OP, "type": tn,
                                        "inputs": {key: name}})["kind"] == "satisfied"
    roomier = unruled.replace("      login: { type: identity, actor_kind: human, unique: true }\n",
                              "      login: { type: identity, actor_kind: human, unique: true }\n"
                              "      team: { type: string, optional: true }\n", 1)
    def drafted(actor):
        c = tee.post("/v1/request", {"transition": "draft", "actor": actor, "type": "DeclarationChange",
                                     "inputs": {"source": {"roster": roomier}, "source_ref": "git:roster@2"}})
        assert c["kind"] == "satisfied", c
        v = tee.post("/v1/request", {"transition": "submit", "actor": actor, "object_id": c["object_id"]})
        assert v["kind"] == "satisfied", v
        return c["object_id"]
    bot, ana = {"id": "bot"}, {"id": "ana"}
    theirs = drafted(ana)
    for xn in ("reject", "withdraw", "refresh", "reopen"):
        v = tee.post("/v1/request", {"transition": xn, "actor": bot, "object_id": theirs})
        assert (v["kind"], [f["invariant"] for f in v.get("failures", [])]) == \
            ("invariant_violated", ["publish_taken_by"]), (xn, v)
    # nor submit a person's draft (tenth production review)
    draft = tee.post("/v1/request", {"transition": "draft", "actor": ana, "type": "DeclarationChange",
                                     "inputs": {"source": {"roster": roomier}, "source_ref": "git:roster@3"}})
    v = tee.post("/v1/request", {"transition": "submit", "actor": bot, "object_id": draft["object_id"]})
    assert (v["kind"], [f["invariant"] for f in v.get("failures", [])]) == \
        ("invariant_violated", ["publish_taken_by"]), v
    # a person moves it on, and the request of its approval names its operation
    v = tee.post("/v1/request", {"transition": "publish", "actor": ana, "object_id": theirs})
    assert (v["kind"], v.get("parents")) == ("not_requestable", ["publish"]), v
    assert tee.post("/v1/request", {"transition": "reopen", "actor": ana,
                                    "object_id": theirs})["kind"] == "satisfied"
    # the agent's own change it moves on
    mine = drafted(bot)
    assert tee.post("/v1/request", {"transition": "withdraw", "actor": bot,
                                    "object_id": mine})["kind"] == "satisfied"
    alike(tee)


PAY = """module: pay
categories: [live, closed]
types:
  Payment:
    description: A payment to send.
    tracking: record
    attributes:
      amount: { type: int }
    states:
      DRAFT: { category: live, description: Drafted. }
      SENT: { category: closed, final: true, description: Sent. }
    transitions:
      make: { kind: initial, to: DRAFT, required_inputs: [amount] }
      grow: { kind: internal, from: [DRAFT], required_inputs: [amount] }
      send: { kind: external, from: [DRAFT], to: SENT, proposable: true }
"""


def test_an_approval_naming_what_its_approver_read_is_held_to_it_alike(tee):
    """An approval naming target_version, the version of the proposed object its approver read, is refused stale
    where the object moved since, and approved where it did not, alike through both backends; one naming none runs
    on the object as it stands, as the design says; target_version elsewhere is a 400 (tenth production review,
    ADR-0197)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "pay.yaml", "text": PAY}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    made = tee.post("/v1/request", {"transition": "make", "actor": OP, "type": "Payment", "inputs": {"amount": 5}})
    pay = made["object_id"]
    proposal = tee.post("/v1/request", {"transition": "send", "actor": OP, "object_id": pay, "propose": True})
    assert proposal["kind"] == "satisfied", proposal
    read = tee.post("/v1/get", {"id": pay})["version"]
    # the object grown after the proposal was filed and its approver read it
    assert tee.post("/v1/request", {"transition": "grow", "actor": OP, "object_id": pay,
                                    "inputs": {"amount": 5000000}})["kind"] == "satisfied"
    v = tee.post("/v1/request", {"transition": "approve", "actor": OP, "object_id": proposal["object_id"],
                                 "target_version": read})
    assert (v["kind"], v.get("expected"), v.get("actual")) == ("stale", read, read + 1), v
    # read again, it is approved
    v = tee.post("/v1/request", {"transition": "approve", "actor": OP, "object_id": proposal["object_id"],
                                 "target_version": read + 1})
    assert v["kind"] == "satisfied", v
    for body in ({"transition": "grow", "actor": OP, "object_id": pay, "target_version": 1, "inputs": {"amount": 1}},
                 {"transition": "send", "actor": OP, "object_id": pay, "propose": True, "target_version": 1}):
        with pytest.raises(Failed, match="400"):
            tee.post("/v1/request", body)
    alike(tee)


def test_a_decimal_written_past_its_digits_is_refused_alike(tee):
    """An effect writing a decimal past its 18 digits refuses its request value_in_range, alike through both
    backends, where the core answered 501 not run (eleventh production review)."""
    flow = PAY.replace("      amount: { type: int }\n", "      amount: { type: int }\n      ratio: { type: \"decimal(18,2)\", optional: true }\n", 1) \
        .replace("      send: { kind: external, from: [DRAFT], to: SENT, proposable: true }\n",
                 "      send: { kind: external, from: [DRAFT], to: SENT, proposable: true }\n"
                 "      rate: { kind: internal, from: [DRAFT], required_inputs: [ratio] }\n"
                 "      square: { kind: internal, from: [DRAFT], effect: [ { assign: { location: ratio, expr: \"ratio * ratio\" } } ] }\n", 1)
    assert tee.post("/sandbox/load", {"files": [{"name": "pay.yaml", "text": flow}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    pay = tee.post("/v1/request", {"transition": "make", "actor": OP, "type": "Payment", "inputs": {"amount": 5}})["object_id"]
    assert tee.post("/v1/request", {"transition": "rate", "actor": OP, "object_id": pay,
                                    "inputs": {"ratio": "9999999999.99"}})["kind"] == "satisfied"
    v = tee.post("/v1/request", {"transition": "square", "actor": OP, "object_id": pay})
    assert (v["kind"], v.get("clause")) == ("unsatisfied", "value_in_range"), v
    alike(tee)


TWO = """module: two
categories: [live, closed]
types:
  Customer:
    description: A customer.
    tracking: record
    states:
      ON: { category: live, description: On. }
      OFF: { category: closed, final: true, description: Off. }
    transitions:
      make: { kind: initial, to: ON }
      revoke: { kind: external, from: [ON], to: OFF }
  Pass:
    description: A pass.
    tracking: record
    states:
      ON: { category: live, description: On. }
      OFF: { category: closed, final: true, description: Off. }
    transitions:
      make: { kind: initial, to: ON }
      revoke: { kind: external, from: [ON], to: OFF }
"""


def test_an_object_named_beside_another_type_is_not_found_alike(tee):
    """An object named beside a type is found only as one of that type's family, alike through both backends, so a
    tool bound to a type takes no other type's object of the id it is given (thirteenth production review B)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "two.yaml", "text": TWO}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    tee.post("/v1/request", {"transition": "make", "actor": OP, "type": "Customer"})
    pass_ = tee.post("/v1/request", {"transition": "make", "actor": OP, "type": "Pass"})["object_id"]
    v = tee.post("/v1/request", {"transition": "revoke", "actor": OP, "object_id": pass_, "type": "Customer"})
    assert v["kind"] == "not_found", v
    v = tee.post("/v1/request", {"transition": "revoke", "actor": OP, "object_id": pass_, "type": "Pass"})
    assert v["kind"] == "satisfied", v
    alike(tee)


MEMO = """module: memo
categories: [live, closed]
types:
  Payment:
    description: A payment to send.
    tracking: record
    attributes:
      amount: { type: int }
    states:
      DRAFT: { category: live, description: Drafted. }
      SENT: { category: closed, final: true, description: Sent. }
    transitions:
      make: { kind: initial, to: DRAFT, required_inputs: [amount] }
      note: { kind: internal, from: [DRAFT], proposable: true, inputs: { memo: { type: string, personal: true } } }
      send: { kind: external, from: [DRAFT], to: SENT, proposable: true, inputs: { memo: { type: string, personal: true } } }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
"""


def test_an_erasure_redacts_every_proposal_on_its_object_alike(tee):
    """An erasure invalidates a pending proposal on its object and empties its inputs, and redacts the personal inputs
    of one already decided, alike through both backends, where a decided proposal kept a personal memo in its get, its
    history and the log (thirteenth production review A)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "memo.yaml", "text": MEMO}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    pay = tee.post("/v1/request", {"transition": "make", "actor": OP, "type": "Payment", "inputs": {"amount": 5}})["object_id"]
    decided = tee.post("/v1/request", {"transition": "note", "actor": OP, "object_id": pay, "propose": True,
                                       "inputs": {"memo": "zelda.secret@example.com"}})["object_id"]
    assert tee.post("/v1/request", {"transition": "approve", "actor": OP, "object_id": decided})["kind"] == "satisfied"
    pending = tee.post("/v1/request", {"transition": "send", "actor": OP, "object_id": pay, "propose": True,
                                       "inputs": {"memo": "zelda.other@example.com"}})["object_id"]
    v = tee.post("/v1/request", {"transition": "forget", "actor": OP, "object_id": pay, "inputs": {"reason": "asked"}})
    assert v["kind"] == "satisfied", v
    for p in (decided, pending):
        read = tee.post("/v1/get", {"id": p})
        assert "zelda" not in json.dumps(read), read
        history = tee.post("/v1/history", {"id": p})
        assert "zelda" not in json.dumps(history), history
    alike(tee)


CHAIN = """module: chain
categories: [live, closed]
types:
  Card:
    description: A card.
    tracking: record
    attributes:
      title: { type: string }
    states:
      LIVE: { category: live, description: Live. }
      MERGED: { category: closed, final: true, superseding: true, description: Merged into another. }
    transitions:
      open: { kind: initial, to: LIVE, required_inputs: [title] }
      merge:
        kind: external
        from: LIVE
        to: MERGED
        inputs:
          successor: { reference: Card }
        effect:
          - supersede: inputs.successor
"""


def test_a_superseded_object_is_followed_alike(tee):
    """`get` and `history` with `follow`, which wire.md and library-api.md
    promise and neither backend built, the key taken and ignored: get
    answers where the supersession chain ends, history the whole group's
    events in position order, alike through both backends; follow of
    another type is a 400 (third production review, minor)."""
    loaded = tee.post("/sandbox/load", {"files": [{"name": "chain.yaml", "text": CHAIN}]})
    assert loaded["loaded"], loaded
    tee.post("/sandbox/reset", {})
    tee.found.clear()

    def opened(title: str) -> str:
        v = tee.post("/v1/request", {"transition": "open", "actor": OP, "type": "Card",
                                     "inputs": {"title": title}})
        assert v["kind"] == "satisfied", v
        return v["object_id"]

    a, b, c = opened("a"), opened("b"), opened("c")
    for old, new in ((a, b), (b, c)):
        v = tee.post("/v1/request", {"transition": "merge", "actor": OP, "object_id": old,
                                     "inputs": {"successor": new}})
        assert v["kind"] == "satisfied", v
    assert tee.post("/v1/get", {"id": a})["id"] == a
    assert tee.post("/v1/get", {"id": a, "follow": True})["id"] == c
    assert tee.post("/v1/get", {"id": c, "follow": True})["id"] == c
    own = tee.post("/v1/history", {"id": b})["events"]
    group = tee.post("/v1/history", {"id": b, "follow": True})["events"]
    assert {e["object_id"] for e in own} == {b}, own
    assert [e["object_id"] for e in group] == [a, b, c, a, b], group
    assert [e["position"] for e in group] == sorted(e["position"] for e in group)
    with pytest.raises(Failed, match="follow takes true or false"):
        tee.post("/v1/get", {"id": a, "follow": "yes"})
    alike(tee)


PRECISE = """module: precise
categories: [live, closed]
types:
  Gauge:
    description: A gauge.
    tracking: record
    attributes:
      level: { type: "decimal(4,2)" }
    states:
      ON: { category: live, description: On. }
      OFF: { category: closed, final: true, description: Off. }
    transitions:
      fit: { kind: initial, to: ON, required_inputs: [level] }
      double:
        kind: internal
        from: ON
        effect:
          - assign: { location: level, expr: level * 2 }
      off: { kind: external, from: ON, to: OFF }
"""


def test_a_decimal_holds_its_precisions_digits_alike(tee):
    """A decimal(p,s) holds p digits, as declaration-syntax.md §4 says:
    an input past them is refused out_of_range and an effect writing past
    them refused value_in_range, alike through both backends, where both
    took 18 whatever p (third production review, minor)."""
    loaded = tee.post("/sandbox/load", {"files": [{"name": "precise.yaml", "text": PRECISE}]})
    assert loaded["loaded"], loaded
    tee.post("/sandbox/reset", {})
    tee.found.clear()

    def fit(level: str) -> dict:
        return tee.post("/v1/request", {"transition": "fit", "actor": OP, "type": "Gauge",
                                        "inputs": {"level": level}})

    past = fit("100.00")
    assert (past["kind"], past.get("inputs")) == ("invalid_input", ["level"]), past
    small, large = fit("49.99")["object_id"], fit("50.00")["object_id"]
    v = tee.post("/v1/request", {"transition": "double", "actor": OP, "object_id": small})
    assert v["kind"] == "satisfied", v
    v = tee.post("/v1/request", {"transition": "double", "actor": OP, "object_id": large})
    assert (v["kind"], v.get("clause"), v.get("objects")) == ("unsatisfied", "value_in_range", [large]), v
    alike(tee)


TEXTUAL = """module: textual
categories: [live, closed]
types:
  Memo:
    description: A memo.
    tracking: record
    attributes:
      note: { type: string }
    states:
      OPEN: { category: live, description: Open. }
      FILED: { category: closed, final: true, description: Filed. }
    transitions:
      write: { kind: initial, to: OPEN, required_inputs: [note] }
      file: { kind: external, from: OPEN, to: FILED }
"""


def test_a_string_holds_at_most_64_kib_alike(tee):
    """A string value holds at most 64 KiB in UTF-8, where a 1.9 MB title
    was stored: past it an input is refused out_of_range, alike through
    both backends, counted in bytes, not characters (ADR-0188 decision
    1)."""
    loaded = tee.post("/sandbox/load", {"files": [{"name": "textual.yaml", "text": TEXTUAL}]})
    assert loaded["loaded"], loaded
    tee.post("/sandbox/reset", {})
    tee.found.clear()

    def write(note: str) -> dict:
        return tee.post("/v1/request", {"transition": "write", "actor": OP, "type": "Memo",
                                        "inputs": {"note": note}})

    assert write("n" * 65536)["kind"] == "satisfied"
    assert write("é" * 32768)["kind"] == "satisfied"
    for note in ("n" * 65537, "é" * 32769):
        v = write(note)
        assert (v["kind"], v.get("inputs")) == ("invalid_input", ["note"]), v["kind"]
    alike(tee)


def test_a_key_and_a_position_are_each_taken_once_alike(tee):
    """A body naming a key twice, a second actor among them, is 400 in both
    backends, where the last one won and changed who was recorded; and events
    naming a position twice is 400, where one position named a thousand times
    answered 201 MB (fifth production review, ADR-0190 decisions 6 and 8)."""
    loaded(tee, "reimbursement.yaml")
    twice = (b'{"type": "Employee", "transition": "onboard", "actor": {"id": "operator"},'
             b' "actor": {"id": "someone"}, "inputs": {"user_id": "zed"}}')
    for base in (tee.core, tee.runner):
        req = urllib.request.Request(base + "/v1/request", data=twice, method="POST",
                                     headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req) as resp:
                status = resp.status
        except urllib.error.HTTPError as e:
            status = e.code
        assert status == 400, base
        assert raw(base, "POST", "/v1/events", {"actor": OP, "positions": [1, 1]})[0] == 400, base
        assert raw(base, "POST", "/v1/events", {"actor": OP, "positions": [1, 2]})[0] == 200, base


HELD = """module: held
categories: [live, closed]
types:
  Staff:
    description: A person who may act.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states:
      ACTIVE: { description: Working., category: live }
      LEFT: { description: Gone., category: closed, final: true }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [login] }
      leave: { kind: external, from: ACTIVE, to: LEFT }
  Bot:
    description: An agent.
    tracking: record
    attributes:
      key: { type: identity, actor_kind: agent, unique: true }
    states:
      ON: { description: On., category: live }
      OFF: { description: Off., category: closed, final: true }
    transitions:
      issue: { kind: initial, to: ON, required_inputs: [key] }
      retire: { kind: external, from: ON, to: OFF }
  Bill:
    description: A bill a person corrects.
    tracking: record
    taken_by: [human, service]
    attributes:
      amount: { type: int }
    states:
      OPEN: { description: Open., category: live }
      PAID: { description: Paid., category: closed, final: true }
    transitions:
      record: { kind: initial, to: OPEN, required_inputs: [amount] }
      fix: { kind: internal, from: OPEN, corrects: [amount], required_inputs: [amount], inputs: { reason: { type: string } } }
      pay: { kind: external, from: OPEN, to: PAID }
"""


def test_a_correction_held_to_people_refuses_an_agent_alike(tee):
    """A type's taken_by holds its correction to the kinds it names: an agent
    is refused, a person takes it, alike through both backends, where a
    correction could take no taken_by and the agent rewrote a recorded bill
    (fifth production review NEW-CORRECTION, ADR-0190 decision 5)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "held.yaml", "text": HELD}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    for t, x, inputs in (("Staff", "add", {"login": "ana"}), ("Bot", "issue", {"key": "scout"})):
        assert tee.post("/v1/request", {"type": t, "transition": x, "actor": OP, "inputs": inputs})["kind"] == "satisfied"
    bill = tee.post("/v1/request", {"type": "Bill", "transition": "record", "actor": {"id": "ana"},
                                    "inputs": {"amount": 5}})["object_id"]
    fix = {"object_id": bill, "transition": "fix", "inputs": {"amount": 7, "reason": "typo"}}
    v = tee.post("/v1/request", {**fix, "actor": {"id": "scout"}})
    assert (v["kind"], [f["invariant"] for f in v.get("failures", [])]) == (
        "invariant_violated", ["fix_taken_by"]), v
    assert tee.post("/v1/request", {**fix, "actor": {"id": "ana"}})["kind"] == "satisfied"
    alike(tee)


def test_an_identity_names_one_actor_alike(tee):
    """An identity another actor holds, of any type whose objects are
    actors, is refused identity_taken, and one holding a zero-width or
    bidirectional character is malformed, alike through both backends,
    where the first made of two answered for both, an agent passing as a
    person (fifth production review IDENTITY, ADR-0190 decision 4)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "held.yaml", "text": HELD}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    made = tee.post("/v1/request", {"type": "Staff", "transition": "add", "actor": OP, "inputs": {"login": "ana"}})
    assert made["kind"] == "satisfied", made
    # both now share it: the person who held it, and the agent asking for it
    v = tee.post("/v1/request", {"type": "Bot", "transition": "issue", "actor": OP, "inputs": {"key": "ana"}})
    assert (v["kind"], [(f["type"], f["invariant"]) for f in v.get("failures", [])]) == (
        "invariant_violated", [("Staff", "identity_taken"), ("Bot", "identity_taken")]), v
    v = tee.post("/v1/request", {"type": "Bot", "transition": "issue", "actor": OP, "inputs": {"key": "an\u200ba"}})
    assert (v["kind"], v.get("reasons")) == ("invalid_input", {"key": "malformed"}), v
    assert tee.post("/v1/request", {"type": "Bot", "transition": "issue", "actor": OP,
                                    "inputs": {"key": "scout"}})["kind"] == "satisfied"
    alike(tee)


TILLS = """module: tills
categories: [live, closed]
types:
  Till:
    description: A till that keeps a float and a day it is due back.
    tracking: record
    attributes:
      float: { type: money(USD) }
      opened_at: { type: timestamp }
      days: { type: int }
    states:
      OPEN: { category: live, description: Open. }
      CLOSED: { category: closed, final: true, description: Closed. }
    derived_attributes:
      due_back:
        description: When the till is due back.
        expression: opened_at + days * 1 day
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [float, opened_at, days] }
      grow:
        kind: internal
        from: OPEN
        effect:
          - assign: { location: float, expr: float * 2 }
      close: { kind: external, from: OPEN, to: CLOSED }
"""


def test_what_a_read_answers_a_request_can_send_alike(tee):
    """Money an effect writes past what an input holds is refused
    value_in_range, a derived time past the calendar is answered without a
    value, and a metric's window is positive, alike through both backends,
    where the core stored an amount no request could send, answered year
    2,739,933 and read "-5 days" as an empty page (fifth production review,
    ADR-0191 decision 3)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "tills.yaml", "text": TILLS}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()

    def till(amount: str, days: int) -> str:
        v = tee.post("/v1/request", {"transition": "open", "actor": OP, "type": "Till",
                                     "inputs": {"float": amount, "opened_at": "2026-10-13T00:00:00Z", "days": days}})
        assert v["kind"] == "satisfied", v
        return v["object_id"]

    # 16 digits before the point, an input's most; doubled, 17, which 64
    # bits hold, so the arithmetic has its value and the write is refused
    big = till("USD 9000000000000000.00", 30)
    v = tee.post("/v1/request", {"transition": "grow", "actor": OP, "object_id": big})
    assert (v["kind"], v.get("clause"), v.get("objects")) == ("unsatisfied", "value_in_range", [big]), v
    small = till("USD 4000000000000000.00", 30)
    assert tee.post("/v1/request", {"transition": "grow", "actor": OP, "object_id": small})["kind"] == "satisfied"
    assert tee.post("/v1/get", {"id": small})["attributes"]["float"] == "USD 8000000000000000.00"
    # a time past the calendar, derived, has no value; one within it has
    assert tee.post("/v1/get", {"id": big})["attributes"]["due_back"] == "2026-11-12T00:00:00Z"
    far = till("USD 1.00", 1_000_000_000)
    assert tee.post("/v1/get", {"id": far})["attributes"]["due_back"] is None
    # a window back from now is positive
    # a positive one is read, here refused as a value, since the metric takes none
    assert tee.post("/v1/metric", {"actor": OP, "name": "Till.refusals", "over": "5 days"})["refused"] == "no_window"
    alike(tee)
    for base in (tee.core, tee.runner):
        for over in ("-5 days", "0 s"):
            status, body = raw(base, "POST", "/v1/metric", {"actor": OP, "name": "Till.refusals", "over": over})
            assert (status, "positive" in str(body)) == (400, True), (base, body)


DECIDED = HELD.replace("""      pay: { kind: external, from: OPEN, to: PAID }
""", """      pay: { kind: external, from: OPEN, to: PAID, proposable: true }
  Memo:
    description: A memo anyone may post.
    tracking: record
    states:
      DRAFT: { description: Drafted., category: live }
      POSTED: { description: Posted., category: closed, final: true }
    transitions:
      write: { kind: initial, to: DRAFT }
      post: { kind: external, from: DRAFT, to: POSTED, proposable: true }
""")


def test_a_proposal_is_decided_by_those_who_may_take_it_alike(tee):
    """A proposal of a transition held to people and services is rejected by
    them alone, and withdrawn by them or its proposer, an agent refused as its
    approval would be, invariant_violated pay_taken_by, delegable, alike
    through both backends, where an agent could reject what a person
    proposed; a transition held to no kind is decided by anyone (fifth
    production review, ADR-0191 decision 1)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "held.yaml", "text": DECIDED}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    staff = tee.post("/v1/request", {"type": "Staff", "transition": "add", "actor": OP,
                                     "inputs": {"login": "ana"}})["object_id"]
    assert tee.post("/v1/request", {"type": "Bot", "transition": "issue", "actor": OP,
                                    "inputs": {"key": "scout"}})["kind"] == "satisfied"
    ana, scout = {"id": "ana"}, {"id": "scout"}

    def proposed(actor: dict, type_: str, made: str, xn: str) -> str:
        o = tee.post("/v1/request", {"type": type_, "transition": made, "actor": OP,
                                     **({"inputs": {"amount": 5}} if type_ == "Bill" else {})})["object_id"]
        v = tee.post("/v1/request", {"object_id": o, "transition": xn, "actor": actor, "propose": True})
        assert v["kind"] == "satisfied", v
        return v["object_id"]

    def decide(proposal: str, verb: str, actor: dict) -> dict:
        return tee.post("/v1/request", {"object_id": proposal, "transition": verb, "actor": actor})

    held = proposed(ana, "Bill", "record", "pay")
    for verb in ("reject", "withdraw"):
        v = decide(held, verb, scout)
        assert (v["kind"], [(f["type"], f["invariant"], f["remedy"]) for f in v.get("failures", [])]) == (
            "invariant_violated", [("Bill", "pay_taken_by", "delegable")]), v
    # an approval is refused by its execution as it was, which the proposal
    # keeps as its last verdict (DESIGN.md §9)
    v = decide(held, "approve", scout)
    assert (v["kind"], v.get("failures", [{}])[0].get("invariant")) == ("invariant_violated", "pay_taken_by"), v
    last = tee.post("/v1/get", {"id": held})["attributes"]["last_verdict"]
    assert (last["verdict"], last["clause"]) == ("invariant_violated", "pay_taken_by"), last
    assert decide(held, "reject", ana)["kind"] == "satisfied"
    # an agent takes back what it drafted
    drafted = proposed(scout, "Bill", "record", "pay")
    assert decide(drafted, "withdraw", scout)["kind"] == "satisfied"
    # a proposer named by its object's id is recorded by its identity, as
    # every event names its actor (D611)
    by_id = proposed({"id": staff}, "Bill", "record", "pay")
    assert tee.post("/v1/get", {"id": by_id})["attributes"]["proposer"] == "ana"
    assert decide(by_id, "withdraw", ana)["kind"] == "satisfied"
    # a transition held to no kind is decided by anyone
    open_ = proposed(ana, "Memo", "write", "post")
    assert decide(open_, "reject", scout)["kind"] == "satisfied"
    alike(tee)


TICKS = """module: ticks
categories: [live, closed]
types:
  Ticker:
    description: A ticker that ticks.
    tracking: record
    states:
      ON: { category: live, description: Ticking. }
      OFF: { category: closed, final: true, description: Stopped. }
    transitions:
      start: { kind: initial, to: ON }
      tick: { kind: internal, from: ON }
      stop: { kind: external, from: ON, to: OFF }
"""


def test_a_history_is_read_page_by_page_alike(tee):
    """An object's history answers at most a page of its events, 100 unless
    asked, with the cursor of the next, null on the last, alike through both
    backends, where it answered every event at once, 164.6 MB for one
    object (sixth production review HISTORY, ADR-0192 decision 5)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "ticks.yaml", "text": TICKS}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    t = tee.post("/v1/request", {"transition": "start", "actor": OP, "type": "Ticker"})["object_id"]
    for _ in range(250):
        assert tee.post("/v1/request", {"transition": "tick", "actor": OP, "object_id": t})["kind"] == "satisfied"
    pages, cursor = [], None
    while True:
        page = tee.post("/v1/history", {"actor": OP, "id": t, **({"cursor": cursor} if cursor else {})})
        pages.append(len(page["events"]))
        cursor = page["cursor"]
        if cursor is None:
            break
    assert pages == [100, 100, 51], pages
    assert len(tee.post("/v1/history", {"actor": OP, "id": t, "limit": 1000})["events"]) == 251
    alike(tee)
    for base in (tee.core, tee.runner):
        for wrong in ({"limit": 1001}, {"limit": 0}, {"cursor": "-1"}, {"cursor": "x"}):
            status, body = raw(base, "POST", "/v1/history", {"actor": OP, "id": t, **wrong})
            assert status == 400, (base, wrong, body)


FORGOTTEN = """module: forgotten
categories: [live, closed]
types:
  Person:
    description: A person, signing in by email.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human, unique: true, personal: true, optional: true }
    states:
      ON: { category: live, description: Here. }
      OFF: { category: closed, final: true, description: Gone. }
    transitions:
      add: { kind: initial, to: ON, required_inputs: [login] }
      leave: { kind: external, from: ON, to: OFF }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
  Note:
    description: A note.
    tracking: record
    states:
      OPEN: { category: live, description: Open. }
      DONE: { category: closed, final: true, description: Done. }
    transitions:
      write: { kind: initial, to: OPEN }
      close: { kind: external, from: OPEN, to: DONE, proposable: true }
"""


def test_an_erased_actor_is_named_by_its_object_alike(tee):
    """An erased person's identity, an email marked personal, leaves every
    event they made and the proposal they filed, which name their object's id
    instead, alike through both backends, where the email stayed in the log
    and history answered it (sixth production review ERASED-IDENTITY,
    ADR-0192 decision 4)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "forgotten.yaml", "text": FORGOTTEN}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    email = "ann.secret@corp.example"
    ann = tee.post("/v1/request", {"transition": "add", "actor": OP, "type": "Person",
                                   "inputs": {"login": email}})["object_id"]
    note = tee.post("/v1/request", {"transition": "write", "actor": {"id": email}, "type": "Note"})["object_id"]
    filed = tee.post("/v1/request", {"transition": "close", "actor": {"id": email}, "object_id": note,
                                     "propose": True})
    assert filed["kind"] == "satisfied", filed
    proposal = filed["object_id"]
    # before the erasure a proposal's own event names its actor by identity,
    # as every event does, in both backends (D613)
    before = tee.post("/v1/history", {"actor": OP, "id": proposal})["events"]
    assert [e["actor_id"] for e in before] == [email], before
    erased = tee.post("/v1/request", {"transition": "forget", "actor": OP, "object_id": ann,
                                      "inputs": {"reason": "asked"}})
    assert erased["kind"] == "satisfied", erased
    for oid in (note, proposal):
        events = tee.post("/v1/history", {"actor": OP, "id": oid})["events"]
        assert email not in str(events), events
        assert ann in [e["actor_id"] for e in events], events
    assert tee.post("/v1/get", {"id": proposal})["attributes"]["proposer"] == ann
    alike(tee)


def test_an_identity_shows_what_it_is_alike(tee):
    """An identity is not empty, holds no whitespace at its ends nor any but a
    space within, no character that shows nothing, and is not shaped as an
    object's id, else `malformed`; and one differing from another actor's by
    case alone is `identity_taken`, alike through both backends, where each
    made another actor than the one it showed (sixth production review
    IDENTITY, ADR-0192 decision 6)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "held.yaml", "text": HELD}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()

    def issue(key: str) -> dict:
        return tee.post("/v1/request", {"type": "Bot", "transition": "issue", "actor": OP, "inputs": {"key": key}})

    for key in ("", " scout", "scout ", "sc\u00a0out", "sc\u034fout", "sc\ufe0fout", "sc\u3164out",
                "sc\U000e0041out", "sc\u2028out", "sc\ufff9out", "sc\U0001d173out", "staff-7", "bill-2",
                # a blank pattern, a vowel that shows nothing, private use and noncharacters (ADR-0194 decision 6)
                "sc\u2800out", "sc\u17b4out", "sc\ue000out", "sc\U000f0000out", "sc\ufdd0out", "sc\U0010fffeout"):
        v = issue(key)
        assert (v["kind"], v.get("reasons")) == ("invalid_input", {"key": "malformed"}), (repr(key), v)
    assert issue("Scout One")["kind"] == "satisfied"
    assert issue("bills-2")["kind"] == "satisfied"
    assert tee.post("/v1/request", {"type": "Staff", "transition": "add", "actor": OP,
                                    "inputs": {"login": "Ana"}})["kind"] == "satisfied"
    v = issue("ana")
    assert (v["kind"], [(f["type"], f["invariant"]) for f in v.get("failures", [])]) == (
        "invariant_violated", [("Staff", "identity_taken"), ("Bot", "identity_taken")]), v
    alike(tee)


CRM = """module: crm
categories: [live, closed]
types:
  Person:
    description: A person.
    tracking: record
    attributes:
      name: { type: string }
    states:
      ACTIVE: { category: live, description: Here. }
      GONE: { category: closed, final: true, description: Gone. }
    conditions:
      unreferenced:
        description: Nothing refers to the person.
        expression: count(r in referrers) == 0
        remedy: dependent
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [name] }
      remove: { kind: external, from: ACTIVE, to: GONE, guards: { unreferenced: deny } }
  Team:
    description: A team a person leads.
    tracking: record
    attributes:
      lead: { reference: Person }
    states:
      OPEN: { category: live, description: Working. }
      DISBANDED: { category: closed, final: true, description: Ended. }
    transitions:
      form: { kind: initial, to: OPEN, required_inputs: [lead] }
      disband: { kind: external, from: OPEN, to: DISBANDED }
  Order:
    description: An order a person places.
    tracking: record
    attributes:
      buyer: { reference: Person }
    states:
      OPEN: { category: live, description: Placed. }
      CLOSED: { category: closed, final: true, description: Done. }
    transitions:
      place: { kind: initial, to: OPEN, required_inputs: [buyer] }
      close: { kind: external, from: OPEN, to: CLOSED }
"""


def test_referrers_through_two_references_alike(tee):
    """`referrers` finds what refers to an object through each reference that
    may name it, and nothing else, alike through both backends, the core
    reading each by its index (seventh production review REFERRERS, ADR-0194
    decision 3)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "crm.yaml", "text": CRM}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()

    def made(tn: str, xn: str, inputs: dict) -> str:
        v = tee.post("/v1/request", {"type": tn, "transition": xn, "actor": OP, "inputs": inputs})
        assert v["kind"] == "satisfied", v
        return v["object_id"]

    people = [made("Person", "add", {"name": f"P{i}"}) for i in range(4)]
    order = made("Order", "place", {"buyer": people[0]})
    team = made("Team", "form", {"lead": people[1]})
    for p in people:
        tee.post("/v1/request", {"object_id": p, "transition": "remove", "actor": OP})
    for p, holder in ((people[0], order), (people[1], team)):
        v = tee.post("/v1/request", {"object_id": p, "transition": "remove", "actor": OP})
        assert (v["kind"], v.get("clause"), v.get("objects")) == ("unsatisfied", "unreferenced", [holder]), v
    alike(tee)


def test_a_migration_may_admit_identity_taken_alike(tee):
    """A version's migration may admit an actor type's `identity_taken`, as a
    store an earlier release left with identities equal but for case needs,
    both checkers taking it, where neither counted it among a type's
    invariants and the store could not be decided (eighth production review
    CASE-PAIR, ADR-0195 decision 2)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "held.yaml", "text": HELD}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    nxt = (HELD.replace("categories: [live, closed]\n", "categories: [live, closed]\nenumerations:\n  Why: [LEGACY]\n", 1)
           + "migration:\n  description: Identities equal but for case admitted.\n  admit:\n"
           + "    - { invariant: Staff.identity_taken, reason: Why.LEGACY }\n")
    drafted = tee.post("/v1/request", {"actor": OP, "type": "DeclarationChange", "transition": "draft",
                                       "inputs": {"source": {"held": nxt}, "source_ref": "test:admit"}})
    submitted = tee.post("/v1/request", {"actor": OP, "object_id": drafted["object_id"], "transition": "submit"})
    assert submitted["kind"] == "satisfied", submitted
    published = tee.post("/v1/publish", {"actor": OP, "change": drafted["object_id"],
                                         "expected_version": submitted["version"]})
    assert published["verdict"]["kind"] == "satisfied", published
    alike(tee)


def test_a_dimension_kept_twice_is_refused_alike(tee):
    """A metric read keeping one dimension twice is a 400 from both backends,
    where one named 5,000 times ran to the read's deadline (seventh
    production review, ADR-0194 decision 6)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "held.yaml", "text": HELD}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    with pytest.raises(Failed):
        tee.post("/v1/metric", {"actor": OP, "name": "Bill.time_in_state", "keep": ["state", "state"]})
    alike(tee)


def test_an_actor_of_another_shape_is_refused_alike(tee):
    """A read's actor that is not an object of an id and a principal is a 400
    from both backends, on every read that takes one, where a read took an
    actor of any JSON type (eighth production review, ADR-0195 decision 7)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "held.yaml", "text": HELD}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    reads = [("/v1/get", {"id": "x"}), ("/v1/lookup", {"source": "s", "value": "v"}),
             ("/v1/declaration", {"type": "Bill"}), ("/v1/tools", {"actor_kind": "agent"}), ("/v1/history", {"id": "x"})]
    for path, body in reads:
        for actor in ([1], "operator", 7, {"id": 7}, {"id": "operator", "role": "x"}):
            with pytest.raises(Failed, match="400"):
                tee.post(path, {**body, "actor": actor})
        # an actor of the shape, or none, is read as before
        tee.post(path, {**body, "actor": OP})
        tee.post(path, body)
    alike(tee)


def test_an_actor_named_in_another_case_is_that_actor_alike(tee):
    """An actor named by its identity in another case of its ASCII letters is
    that actor, recorded under the identity it holds, as uniqueness compares
    them, and one differing beyond ASCII case is no actor, alike through both
    backends, where the core recorded an unknown actor under the text sent
    (seventh production review, ADR-0194 decision 5)."""
    assert tee.post("/sandbox/load", {"files": [{"name": "held.yaml", "text": HELD}]})["loaded"]
    tee.post("/sandbox/reset", {})
    tee.found.clear()
    assert tee.post("/v1/request", {"type": "Staff", "transition": "add", "actor": OP,
                                    "inputs": {"login": "Ana.Lee"}})["kind"] == "satisfied"
    v = tee.post("/v1/request", {"type": "Bill", "transition": "record", "actor": {"id": "ANA.LEE"},
                                 "inputs": {"amount": 5}})
    assert v["kind"] == "satisfied", v
    h = tee.post("/v1/history", {"id": v["object_id"]})
    assert [(e["actor_id"], e["actor_kind"]) for e in h["events"]] == [("Ana.Lee", "human")], h
    v = tee.post("/v1/request", {"type": "Bill", "transition": "record", "actor": {"id": "ANA.L\u00c9E"},
                                 "inputs": {"amount": 5}})
    assert (v["kind"], v.get("clause")) == ("unsatisfied", "actor_known"), v
    # a letter outside ASCII compares as written: two actors, not one taken
    assert tee.post("/v1/request", {"type": "Staff", "transition": "add", "actor": OP,
                                    "inputs": {"login": "\u00c9mile"}})["kind"] == "satisfied"
    v = tee.post("/v1/request", {"type": "Bot", "transition": "issue", "actor": OP, "inputs": {"key": "\u00e9mile"}})
    assert v["kind"] == "satisfied", v
    alike(tee)

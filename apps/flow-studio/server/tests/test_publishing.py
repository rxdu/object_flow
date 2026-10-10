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
from replay import Failed
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
    assert report["invariant_forms"] == {
        "ServiceJob.photo_when_done": "local"}
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
            "expense_date": "2025-12-30", "receipt": "taxi.pdf"}

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
            ("/v1/nope", {}, "no operation /v1/nope")):
        with pytest.raises(Exception) as e:
            tee.post(path, body)
        assert said in str(e.value), e.value
    for path, body in (("/v1/get", {"id": "claim-999"}),
                       ("/v1/history", {"id": "claim-999"}),
                       ("/v1/availability", {"actor": OP, "id": "claim-999"})):
        assert tee.post(path, body) is None, path
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
    assert report["could_violate"] == {"ServiceJob.photo_when_done": [
        "ServiceJob.finish", "ServiceJob.open", "ServiceJob.start"]}
    alike(tee)

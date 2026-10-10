"""The tools an agent is handed, rendered from the declaration
(`renderers.md` §3, ADR-0173 decision 10): compared whole between the core
and the reference runner over every module of the design and every actor
kind, each a valid JSON Schema that takes the calls the wire takes, and a
transition left out for an actor kind where an invariant over the kind it
records is certain to fail with it (decision 1).

It builds the core's service with the pinned toolchain, and fails, not skips,
without it.
"""

import pathlib
import tempfile

import jsonschema
import pytest

from compare import Tee
from flowstudio.catalogue import Catalogue
from flowstudio.core import Core
from test_backends_agree import started
from test_publishing import TAKEN, alike

DESIGN = Catalogue(pathlib.Path(tempfile.mkdtemp()))
FILES = [m["file"] for m in DESIGN.modules()
         if m["examples"] and m["source"] == "design"]


@pytest.fixture(scope="module")
def tee():
    core = Core()
    behind, core_url = started(core)
    runner, runner_url = started()
    tee = Tee(core_url, runner_url)
    tee.module = "tools"
    yield tee
    for server in (behind, runner):
        server.shutdown()
        server.server_close()
    core.close()


def tools(tee: Tee, module: str, kind: str) -> dict:
    bundle = DESIGN.bundle(module)
    assert tee.post("/sandbox/load", {"files": bundle["files"]})["loaded"]
    return {t["name"]: t for t in
            tee.post("/v1/tools", {"actor_kind": kind})["tools"]}


@pytest.mark.parametrize("module", FILES)
def test_the_backends_render_the_same_tools(tee, module):
    tee.found.clear()
    for kind in ("human", "agent", "service"):
        rendered = tools(tee, module, kind)
        assert "read_metric" in rendered
        for t in rendered.values():
            jsonschema.Draft7Validator.check_schema(t["input_schema"])
    alike(tee)


def test_an_agent_is_not_handed_a_persons_confirmation(tee):
    """`confirm_nature` records who confirmed, and the invariant
    `confirmed_by_a_person` refuses any but a person; the agent still
    suggests."""
    for_person = tools(tee, "reimbursement.yaml", "human")
    for_agent = tools(tee, "reimbursement.yaml", "agent")
    assert "claim_line_confirm_nature" in for_person
    assert "claim_line_confirm_nature" not in for_agent
    assert "claim_line_suggest_nature" in for_agent
    # a transition only via another is no tool, for anyone
    assert "claim_line_add" not in for_person


def test_an_agent_may_only_propose_what_a_person_takes(tee):
    """A transition `taken_by: [human]` is left out for an agent, unless it
    is proposable, when the agent is handed it for proposing only; a person
    takes it as before (ADR-0176 decision 1)."""
    tee.found.clear()
    assert tee.post("/sandbox/load",
                    {"files": [{"name": "taken.yaml", "text": TAKEN}]})["loaded"]
    kinds = {kind: {t["name"]: t for t in
                    tee.post("/v1/tools", {"actor_kind": kind})["tools"]}
             for kind in ("human", "agent", "service")}
    alike(tee)
    approve = kinds["agent"]["request_approve"]
    assert approve["input_schema"]["properties"]["propose"]["const"] is True
    assert "propose" in approve["input_schema"]["required"]
    assert "only propose" in approve["description"]
    call = {"object_id": "request-3", "inputs": {}}
    with pytest.raises(jsonschema.ValidationError):
        jsonschema.validate(call, approve["input_schema"])
    jsonschema.validate(dict(call, propose=True), approve["input_schema"])
    # not proposable, so not the agent's at all; the service's, as a person's
    assert "request_reject" not in kinds["agent"]
    assert "request_reject" in kinds["service"]
    assert "request_retitle" in kinds["agent"]
    mine = kinds["human"]["request_approve"]["input_schema"]
    assert mine["properties"]["propose"]["type"] == "boolean"
    assert "propose" not in mine["required"]


def test_a_tool_takes_the_calls_the_wire_takes(tee):
    t = tools(tee, "reimbursement.yaml", "human")
    schema = t["claim_add_line"]["input_schema"]
    good = {"object_id": "claim-4", "expected_version": 1,
            "idempotency_key": "k-1",
            "inputs": {"amount": "USD 120.00", "category": "expensecategory-3",
                       "expense_date": "2025-12-30",
                       "receipt": {"hash": "sha256:" + "0" * 64, "size": 8,
                                   "media_type": "application/pdf"},
                       "purpose": "Airport taxi", "merchant": "Comfort"}}
    jsonschema.validate(good, schema)
    for wrong in ({"amount": "SGD 120.00"}, {"amount": 120},
                  {"expense_date": "30/12/2025"}, {"nonsense": 1},
                  {"receipt": "taxi.pdf"}, {"amount": "USD 120.005"}):
        bad = dict(good, inputs=dict(good["inputs"], **wrong))
        with pytest.raises(jsonschema.ValidationError):
            jsonschema.validate(
                bad, schema,
                format_checker=jsonschema.draft7_format_checker)
    # a creation has no object to name
    assert "object_id" not in t["claim_draft"]["input_schema"]["properties"]


def test_an_agent_can_read_before_it_acts(tee):
    """Every transition's tool directs the agent to the object's
    availability, so the tools an agent is handed read too (D564)."""
    t = tools(tee, "reimbursement.yaml", "agent")
    assert {"get_object", "object_availability", "query_objects"} <= set(t)
    assert "object_availability" in t["claim_submit"]["description"]
    assert "Claim" in t["query_objects"]["input_schema"]["properties"][
        "type"]["enum"]


def test_an_unknown_actor_kind_is_a_fault_alike(tee):
    tools(tee, "people.yaml" if "people.yaml" in FILES else FILES[0], "human")
    tee.found.clear()
    for body in ({"actor_kind": "robot"}, {}):
        with pytest.raises(Exception):
            tee.post("/v1/tools", body)
    alike(tee)

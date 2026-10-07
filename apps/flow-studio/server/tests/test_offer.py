"""Slice 2: the live offer and a metric, read through the wire
(ADR-0152, `docs/design/flow-studio.md` §6)."""

import pytest


@pytest.fixture()
def service(wire):
    bundle = wire.get("/studio/module?file=service.yaml")
    loaded = wire.post("/sandbox/load", {"files": bundle["files"]})
    assert loaded["loaded"] and "ServiceJob" in loaded["types"]
    wire.post("/sandbox/reset", {})
    op = {"id": "operator"}
    ana = wire.post("/v1/request", {"transition": "add", "actor": op,
                                    "type": "User",
                                    "inputs": {"login": "ana",
                                               "name": "Ana"}})
    assert ana["kind"] == "satisfied", ana
    return wire, ana["object_id"]


def offers(wire, actor, oid):
    return {(o["transition"], o["creates"]): o
            for o in wire.post("/v1/availability",
                               {"actor": {"id": actor}, "id": oid})}


def test_an_offer_follows_the_state_and_the_guards(service):
    wire, ana = service
    job = wire.post("/v1/request", {"transition": "open",
                                    "actor": {"id": ana},
                                    "type": "ServiceJob",
                                    "inputs": {"engineer": ana}})
    assert job["kind"] == "satisfied", job
    o = offers(wire, ana, job["object_id"])
    assert ("open", None) not in o            # an initial transition
    start = o[("start", None)]
    assert start["availability"] in ("available", "available_with_input")
    assert start["verdict"] is None
    refused = [x for x in o.values() if x["availability"] == "unavailable"]
    assert refused and all(x["verdict"]["kind"] for x in refused)
    unknown = offers(wire, "nobody", job["object_id"])
    assert {(x["verdict"] or {}).get("clause") for x in unknown.values()} \
        == {"actor_known"}


def test_an_offer_of_an_unknown_object_is_a_fault(service):
    wire, ana = service
    with pytest.raises(Exception) as e:
        wire.post("/v1/availability", {"actor": {"id": ana}, "id": "x-1"})
    assert "404" in str(e.value)


def test_a_metric_is_read_whole_and_by_its_groups(service):
    wire, ana = service
    for _ in range(2):
        wire.post("/v1/request", {"transition": "open",
                                  "actor": {"id": ana},
                                  "type": "ServiceJob",
                                  "inputs": {"engineer": ana}})

    def read(**kw):
        return wire.post("/v1/metric", {"actor": {"id": ana},
                                        "name": "ServiceJob.work_in_progress",
                                        **kw})

    def rows(page):
        return [(r["dimensions"], r["value"]) for r in page["rows"]]
    # whole, nothing kept; by every declared dimension unless keep names some
    assert rows(read(keep=[])) == [({}, 2)]
    assert rows(read()) == [({"state": "OPEN"}, 2)]
    assert rows(read(keep=["state"])) == [({"state": "OPEN"}, 2)]
    # a dimension it does not have is refused, a value
    assert read(keep=["nobody"])["refused"] == "unknown_dimension"
    # a page holds every group, so a cursor has nothing to continue from
    with pytest.raises(Exception) as e:
        read(cursor="x")
    assert "400" in str(e.value)

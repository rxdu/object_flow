"""No request faults the core: every operation the wire records, each key
it takes given each of a battery of JSON values, in an otherwise valid
body, sent to the core's service itself, is answered or refused, never
`500` (`wire.md`, `wire-v1.json`; a fault is a defect, never an answer).

It builds the core's service with the pinned toolchain, and fails, not
skips, without it.
"""

import json
import pathlib
import urllib.error
import urllib.request

import pytest

from flowstudio.core import Core
from test_battery import HEAD, INPUT_VALUES, TAIL

OP = {"id": "operator"}
WIRE = json.loads((pathlib.Path(__file__).resolve().parents[4] / "docs" /
                   "guides" / "wire-v1.json").read_text())["operations"]
FLOW = HEAD[:HEAD.index("    derived_attributes:\n")].replace(
    "      tags: { type: \"string[]\", optional: true }\n",
    "      tags: { type: \"string[]\", optional: true }\n"
    "      code: { type: string, optional: true, external: erp }\n") + TAIL + \
    "    metrics:\n      opened: { description: Opened., source: objects, " \
    "item: o, expression: count() }\n"
ODD = INPUT_VALUES + [
    {"id": None}, {"id": 5}, {"id": ""}, {"id": "operator", "x": 1},
    [[]], [{}], {"a": {"b": {"c": {}}}}, "\ud800", "a\u0000b", "‮",
    "x" * 70000, -0.0, float(2**64), json.loads("[" * 30 + "]" * 30),
]


def call(url: str, path: str, body) -> tuple[int, object]:
    data = json.dumps(body).encode()
    req = urllib.request.Request(url + path, data=data, method="POST",
                                 headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return r.status, json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        raw = e.read()
        try:
            return e.code, json.loads(raw or b"null")
        except ValueError:
            return e.code, raw[:200]


def bases(oid: str, change: str) -> dict:
    """A valid body for each operation, each key fuzzed from it."""
    return {
        "/v1/request": {"actor": OP, "type": "B", "transition": "open",
                        "inputs": {}},
        "/v1/check": {"actor": OP, "type": "B", "transition": "open",
                      "inputs": {}},
        "/v1/get": {"actor": OP, "id": oid},
        "/v1/history": {"actor": OP, "id": oid},
        "/v1/availability": {"actor": OP, "id": oid},
        "/v1/available": {"actor": OP, "type": "B", "transition": "close"},
        "/v1/query": {"actor": OP, "type": "B"},
        "/v1/declaration": {"actor": OP, "type": "B"},
        "/v1/diagnostics": {"actor": OP, "type": "B"},
        "/v1/events": {"actor": OP, "positions": [1]},
        "/v1/referenced": {"actor": OP, "hashes": ["sha256:" + "a" * 64]},
        "/v1/exceptions": {"actor": OP, "type": "B"},
        "/v1/export": {"actor": OP, "source": "log"},
        "/v1/lookup": {"actor": OP, "source": "erp", "value": "x"},
        "/v1/maintain": {"actor": OP, "task": {"name": "prune_attempts",
                                               "before": "2020-01-01T00:00:00Z"}},
        "/v1/metric": {"actor": OP, "name": "B.opened"},
        "/v1/publish": {"actor": OP, "change": change, "dry_run": True},
        "/v1/pull": {"actor": OP, "subscription": "s1"},
        "/v1/acknowledge": {"actor": OP, "subscription": "s1", "cursor": "x"},
        "/v1/tools": {"actor": OP},
        "/v1/import_batch": {"actor": OP, "batch": {"objects": []}},
        "/v1/upgrade": {"actor": OP, "dry_run": True},
        "/sandbox/clock": {"advance": "1 h"},
        "/sandbox/evaluators": {"answers": {}},
        "/sandbox/reset": {},
    }


def test_the_studio_takes_the_keys_the_wire_lists():
    """The studio's server takes each operation's keys as the core's service
    holds them, `wire-v1.json`'s list, where a key the core added, `params`
    on a query and a metric, was refused by the runner's backend and no test
    said so (ADR-0218, ADR-0221)."""
    from flowstudio.http import TAKES
    for path, keys in TAKES.items():
        if path in WIRE:
            assert sorted(keys) == sorted(WIRE[path]), path


@pytest.fixture(scope="module")
def core():
    c = Core()
    yield c
    c.close()


def loaded(url: str) -> tuple[str, str]:
    assert call(url, "/sandbox/load", {"files": [
        {"name": "bt.yaml", "text": FLOW}]})[1]["loaded"]
    call(url, "/sandbox/reset", {})
    oid = call(url, "/v1/request", {"actor": OP, "type": "B",
                                    "transition": "open",
                                    "inputs": {}})[1]["object_id"]
    change = call(url, "/v1/request", {
        "actor": OP, "type": "DeclarationChange", "transition": "draft",
        "inputs": {"source": {"bt": FLOW}, "source_ref": "x"}})[1]["object_id"]
    return oid, change


def test_no_request_faults_the_core(core):
    oid, change = loaded(core.url)
    found = []
    for path, body in bases(oid, change).items():
        keys = WIRE.get(path, [])
        for key in keys or [None]:
            for value in ODD:
                sent = dict(body) if key is None else {**body, key: value}
                status, answer = call(core.url, path, sent)
                if status >= 500 and status not in (501, 503):
                    found.append(f"{path} {key}={str(value)[:40]!r}: "
                                 f"{status} {str(answer)[:160]}")
            if path == "/sandbox/reset":
                oid, change = loaded(core.url)
        # a whole body of another JSON kind
        for whole in (None, [], "x", 1, {"actor": None}):
            status, answer = call(core.url, path, whole)
            if status >= 500 and status not in (501, 503):
                found.append(f"{path} body {whole!r}: {status} "
                             f"{str(answer)[:160]}")
    # a load given each value, then the flow restored
    for key in WIRE["/sandbox/load"]:
        for value in ODD:
            status, answer = call(core.url, "/sandbox/load",
                                  {"files": [{"name": "bt.yaml",
                                              "text": FLOW}], key: value})
            if status >= 500 and status not in (501, 503):
                found.append(f"/sandbox/load {key}={str(value)[:40]!r}: "
                             f"{status} {str(answer)[:160]}")
    assert not found, f"{len(found)} faults:\n" + "\n".join(found[:40])


def nested(oid: str) -> list[tuple[str, object]]:
    """Each value placed inside a structured key: an actor's id, principal
    and kind, an input, a task, an import's objects and their keys, a
    metric's binding and filter, a load's files and an evaluator's answer."""
    out = []
    for v in ODD:
        out += [
            ("/v1/request", {"actor": {"id": v}, "type": "B",
                             "transition": "open", "inputs": {}}),
            ("/v1/request", {"actor": {"id": "operator", "principal": v},
                             "type": "B", "transition": "open"}),
            ("/v1/request", {"actor": {"id": "operator", "kind": v},
                             "type": "B", "transition": "open"}),
            ("/v1/request", {"actor": OP, "object_id": oid,
                             "transition": "close", "inputs": {"x": v}}),
            ("/v1/request", {"actor": OP, "type": "B", "transition": "open",
                             "inputs": {"lv": v, "tags": [v]}}),
            ("/v1/maintain", {"actor": OP, "task": {"name": v}}),
            ("/v1/maintain", {"actor": OP, "task": {"name": "prune_attempts",
                                                    "before": v}}),
            ("/v1/import_batch", {"actor": OP, "batch": {"objects": [v]}}),
            ("/v1/import_batch", {"actor": OP, "batch": {"objects": [
                {"type": "B", "legacy_key": v, "state": "OPEN"}]}}),
            ("/v1/import_batch", {"actor": OP, "batch": {"objects": [
                {"type": v, "legacy_key": "k", "state": "OPEN"}]}}),
            ("/v1/import_batch", {"actor": OP, "batch": {"sequences": [v]}}),
            ("/v1/import_batch", {"actor": OP, "batch": {"objects": [],
                                                         "admitted": v}}),
            ("/v1/metric", {"actor": OP, "name": "B.opened", "bind": {"x": v}}),
            ("/v1/metric", {"actor": OP, "name": "B.opened", "filter": v}),
            ("/v1/query", {"actor": OP, "type": "B", "order": [v]}),
            ("/v1/query", {"actor": OP, "type": "B", "fields": [v]}),
            ("/v1/events", {"actor": OP, "positions": [v]}),
            ("/sandbox/evaluators", {"answers": {"E.f": v}}),
        ]
    return out


def test_no_nested_value_faults_the_core(core):
    oid, _change = loaded(core.url)
    found = []
    for path, body in nested(oid):
        status, answer = call(core.url, path, body)
        if status >= 500 and status not in (501, 503):
            found.append(f"{path} {json.dumps(body)[:120]}: {status} "
                         f"{str(answer)[:160]}")
    for v in ODD:
        for files in ([v], [{"name": v, "text": "x"}],
                      [{"name": "a.yaml", "text": v}]):
            status, answer = call(core.url, "/sandbox/load", {"files": files})
            if status >= 500 and status not in (501, 503):
                found.append(f"/sandbox/load {json.dumps(files)[:100]}: "
                             f"{status} {str(answer)[:160]}")
    assert not found, f"{len(found)} faults:\n" + "\n".join(found[:40])


MIRROR = """module: mirrored
categories: [live]
enumerations:
  Level: [HIGH, LOW]
types:
  Kit:
    description: A kit, the legacy system's.
    mirror: true
    tracking: record
    attributes:
      code: { type: string, external: erp }
      i: { type: int, optional: true }
      d2: { type: "decimal(10,2)", optional: true }
      m: { type: money(USD), optional: true }
      du: { type: duration, optional: true }
      t: { type: timestamp, optional: true }
      dd: { type: date, optional: true }
      b: { type: bool, optional: true }
      lv: { type: Level, optional: true }
      tags: { type: "string[]", optional: true }
      parent: { reference: Kit, optional: true }
    states:
      ACTIVE: { category: live }
"""


def test_no_imported_value_faults_the_core(core):
    """Each value imported into each attribute of a mirror, an input of
    every type and a reference, is written, reported as a class or
    refused, never `500`: the import's decoder is the request's, and a null
    element once reached the store's constraint through both."""
    assert call(core.url, "/sandbox/load", {"files": [
        {"name": "mirrored.yaml", "text": MIRROR}]})[1]["loaded"]
    call(core.url, "/sandbox/reset", {})
    found = []
    attrs = ["i", "d2", "m", "du", "t", "dd", "b", "lv", "tags", "parent",
             "code"]
    for n, (attr, v) in enumerate(
            (a, v) for a in attrs for v in ODD):
        obj = {"type": "Kit", "legacy_key": f"k{n}", "state": "ACTIVE",
               "attributes": {"code": f"C{n}", attr: v}}
        for dry in (True, False):
            status, answer = call(core.url, "/v1/import_batch",
                                  {"actor": OP, "batch": {"objects": [obj]},
                                   "dry_run": dry})
            if status >= 500 and status not in (501, 503):
                found.append(f"{attr}={str(v)[:40]!r} dry_run={dry}: "
                             f"{status} {str(answer)[:160]}")
    assert not found, f"{len(found)} faults:\n" + "\n".join(found[:40])

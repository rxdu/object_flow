"""The worked go-live of `publish-and-import.md` §7 (ADR-0173 decision 4), run
as the section says, on a deployment's store (ADR-0174): the purchasing module
of `docs/guides/write-a-flow.md` §6, its go-live version written by
`scripts/mirror-version.py` and published; a legacy batch dry run, its class
cleaned upstream and imported with the order number raised past the legacy
system's last; the module published as the cutover; and the first requests
after it. Every figure the section quotes is asserted here.

It builds the core's service with the pinned toolchain, and fails, not skips,
without it.
"""

import collections
import pathlib
import re
import subprocess
import tempfile
import threading

import pytest

from replay import Wire

ROOT = pathlib.Path(__file__).resolve().parents[4]
CARGO = pathlib.Path.home() / ".cargo/bin/cargo"
GUIDE = ROOT / "docs/guides/write-a-flow.md"
OPERATOR = {"id": "operator"}


def guide_module() -> str:
    """The purchasing module, as the guide's §6 writes it."""
    blocks = re.findall(r"```yaml\n(.*?)```", GUIDE.read_text(), re.S)
    return next(b for b in blocks if b.startswith("module: purchasing"))


@pytest.fixture(scope="module")
def deployment():
    subprocess.run([str(CARGO), "build", "-q", "-p", "objectflow-service"],
                   cwd=ROOT / "core", check=True)
    store = pathlib.Path(tempfile.mkdtemp()) / "store.sqlite"
    service = subprocess.Popen(
        [str(ROOT / "core/target/debug/objectflow-service"),
         "--store", str(store), "--create", "--port", "0"],
        stderr=subprocess.PIPE, text=True)
    line = service.stderr.readline()
    m = re.search(r"listening on (http://127\.0\.0\.1:\d+)", line)
    assert m, f"the core's service did not start: {line}"
    # the service writes a line for each answer: read to its end, so a full
    # pipe never stops it
    threading.Thread(target=collections.deque, args=(service.stderr, 0),
                     daemon=True).start()
    yield Wire(m.group(1))
    service.terminate()
    service.wait()


def publish(wire: Wire, text: str) -> dict:
    """A version drafted, dry run, submitted and approved, as every version
    of a deployment's store is published; the approval's answer."""
    d = wire.post("/v1/request", {
        "actor": OPERATOR, "type": "DeclarationChange", "transition": "draft",
        "inputs": {"source": {"purchasing": text},
                   "source_ref": "test:golive"}})
    assert d["kind"] == "satisfied", d
    s = wire.post("/v1/request", {"actor": OPERATOR,
                                  "object_id": d["object_id"],
                                  "transition": "submit"})
    assert s["kind"] == "satisfied", s
    return wire.post("/v1/publish", {"actor": OPERATOR,
                                     "change": d["object_id"],
                                     "expected_version": s["version"]})


def batch(amount_of_the_last_line: str | None) -> dict:
    """The legacy system's two currencies, two orders and their lines, and
    its last order number."""
    last_line = {"order": "4300", "quantity": 1, "unit_price": "80.00",
                 "legacy_key": "4300-2"}
    if amount_of_the_last_line:
        last_line["amount_sgd"] = amount_of_the_last_line
    return {"objects": [
        {"type": "Currency", "legacy_key": "USD", "state": "ACTIVE",
         "attributes": {"code": "USD", "rate_to_sgd": "1.350000",
                        "legacy_key": "USD"}},
        {"type": "Currency", "legacy_key": "EUR", "state": "ACTIVE",
         "attributes": {"code": "EUR", "rate_to_sgd": "1.450000",
                        "legacy_key": "EUR"}},
        {"type": "Order", "legacy_key": "4299", "state": "PLACED",
         "attributes": {"number": "PO-04299", "currency": "EUR",
                        "legacy_key": "4299"}},
        {"type": "OrderLine", "legacy_key": "4299-1", "state": "LISTED",
         "attributes": {"order": "4299", "quantity": 2,
                        "unit_price": "250.00", "amount_sgd": "SGD 725.00",
                        "legacy_key": "4299-1"}},
        {"type": "Order", "legacy_key": "4300", "state": "OPEN",
         "attributes": {"number": "PO-04300", "currency": "USD",
                        "legacy_key": "4300"}},
        {"type": "OrderLine", "legacy_key": "4300-1", "state": "LISTED",
         "attributes": {"order": "4300", "quantity": 4,
                        "unit_price": "100.00", "amount_sgd": "SGD 540.00",
                        "legacy_key": "4300-1"}},
        {"type": "OrderLine", "legacy_key": "4300-2", "state": "LISTED",
         "attributes": last_line},
    ], "sequences": [{"sequence": "order_number", "last": 4300}]}


def test_the_worked_go_live(deployment):
    wire = deployment
    module = guide_module()
    made = subprocess.run(
        [str(ROOT / "scripts/.venv/bin/python"),
         str(ROOT / "scripts/mirror-version.py"), "-", "Order"],
        input=module, capture_output=True, text=True, check=True)
    # the types an order brings with it, said on stderr
    assert "Currency is a mirror too" in made.stderr
    assert "OrderLine is a mirror too" in made.stderr
    approved = publish(wire, made.stdout)
    assert approved["verdict"]["kind"] == "satisfied", approved
    assert wire.get("/health")["version"] == 1

    dry = wire.post("/v1/import_batch", {"actor": OPERATOR,
                                         "batch": batch(None),
                                         "disposition": [], "dry_run": True})
    assert [(c["id"], c["sample"]) for c in dry["findings"]] == [
        ("OrderLine.amount_sgd_provided: is absent", ["4300-2"])]
    cleaned = [dict(c, disposition="clean_upstream") for c in dry["findings"]]
    refused = wire.post("/v1/import_batch", {"actor": OPERATOR,
                                             "batch": batch(None),
                                             "disposition": cleaned})
    assert refused["written"] == 0
    assert refused["refusal"]["legacy_key"] == "4300-2"
    imported = wire.post("/v1/import_batch", {
        "actor": OPERATOR, "batch": batch("SGD 108.00"), "disposition": []})
    assert (imported["written"], imported["refusal"]) == (7, None)

    approved = publish(wire, module)
    assert approved["verdict"]["kind"] == "satisfied", approved
    assert wire.get("/health")["version"] == 2
    po = wire.post("/v1/lookup", {"actor": OPERATOR, "source": "legacy",
                                  "value": "Order:4300"})
    assert po["attributes"]["total_sgd"] == "SGD 648.00"
    added = wire.post("/v1/request", {
        "actor": OPERATOR, "object_id": po["id"], "transition": "add_line",
        "inputs": {"quantity": 10, "unit_price": "20.00"}})
    assert added["kind"] == "satisfied", added
    assert wire.post("/v1/get", {"id": po["id"]})["attributes"][
        "total_sgd"] == "SGD 918.00"
    usd = wire.post("/v1/lookup", {"actor": OPERATOR, "source": "legacy",
                                   "value": "Currency:USD"})
    opened = wire.post("/v1/request", {
        "actor": OPERATOR, "type": "Order", "transition": "open",
        "inputs": {"currency": usd["id"]}})
    assert wire.post("/v1/get", {"id": opened["object_id"]})["attributes"][
        "number"] == "PO-04301"
    late = wire.post("/v1/import_batch", {"actor": OPERATOR, "batch": {
        "objects": [{"type": "Order", "legacy_key": "4301", "state": "OPEN",
                     "attributes": {"number": "PO-04301",
                                    "currency": "USD"}}]},
        "disposition": []})
    assert late["refused_types"] == ["Order"]


POSTING = """module: posting
categories: [live, closed]
types:
  Opening:
    description: A position to fill.
    tracking: record
    attributes:
      title: { type: string }
    states:
      DRAFT: { description: Drafted., category: live }
      POSTED: { description: Posted., category: live }
      CLOSED: { description: Closed., category: closed, final: true }
    transitions:
      draft: { kind: initial, to: DRAFT, required_inputs: [title] }
      post: { kind: external, from: DRAFT, to: POSTED }
      close: { kind: external, from: POSTED, to: CLOSED }
    metrics:
      posted:
        description: Openings posted, by month.
        measure: transition_count
        transition: post
        group_by: [month]
metrics:
  posted_again:
    description: The openings posted, as a combined metric reads them.
    input_metrics: { p: Opening.posted }
    expression: p
"""


def test_a_go_live_version_leaves_out_metrics_over_dropped_transitions():
    """A mirror keeps no transition but its erasure, so a metric counting
    one of its transitions, and a combined metric reading that, would be
    refused by the checker: the go-live version leaves both out, saying so
    (D586), and checks clean."""
    made = subprocess.run(
        [str(ROOT / "scripts/.venv/bin/python"),
         str(ROOT / "scripts/mirror-version.py"), "-", "Opening"],
        input=POSTING, capture_output=True, text=True, check=True)
    assert "Opening.posted counts a transition" in made.stderr, made.stderr
    assert "posted_again reads one left out" in made.stderr, made.stderr
    with tempfile.TemporaryDirectory() as tmp:
        golive = pathlib.Path(tmp) / "posting-golive.yaml"
        golive.write_text(made.stdout)
        checked = subprocess.run(
            [str(ROOT / "scripts/.venv/bin/python"),
             str(ROOT / "scripts/check-flows.py"), str(golive)],
            capture_output=True, text=True)
        assert checked.returncode == 0, checked.stdout


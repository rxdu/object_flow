#!/usr/bin/env python3
"""Make a deployment's store with a release of the core, for the upgrade's tests (ADR-0177).

A store is made the way a team makes one: the service started on a new file, a design module published as its
first version, through the wire, and then every example of the module played through the same wire, each from its
setups, by the Flow Studio's replay player; an example the deployment cannot play, since it advances the sandbox's
clock or answers its evaluators, stops where it stops, what it wrote kept. Beside the store it writes what the
release answers of it that a later one must answer alike, the immutable record: each object's history and stored
attributes, and the exported log.

Run it with the release's own service, built at that release's commit:

    python3 make-release-store.py <objectflow-service> <module.yaml> <out-dir>

The stores under release-0.1.0/ were made at commit f6ee6d6, the last of release 0.1.0, those under release-0.2.0/
at commit 37e8825, the last of release 0.2.0, those under release-0.3.0/ by the service published as v0.3.0, built at
77bf468 and checked against its SHA-256, from the modules as 0.3.0 shipped them, those under release-0.4.0/ by the
service published as v0.4.0, built at 9c6cd26 and checked against its SHA-256, from the modules as 0.4.0 shipped them,
those under release-0.5.0/ by the service published as v0.5.0, built at ccc88de and checked against its SHA-256, from
the modules as 0.5.0 shipped them, those under release-0.5.1/ by the service published as v0.5.1, built at 4048a90
and checked against its SHA-256, from the modules as 0.5.1 shipped them, its ids UUIDv7s, those under
release-0.5.2/ by the service published as v0.5.2, built at a04acbd and checked against its SHA-256, from the modules
as 0.5.2 shipped them, those under release-0.5.3/ by the service published as v0.5.3, built at a200294 and checked
against its SHA-256, from the modules as 0.5.3 shipped them, and those under release-0.6.0/ by the service published
as v0.6.0, built at eff4a6d and checked against its SHA-256, from the modules as 0.6.0 shipped them.
"""
import json
import pathlib
import re
import shutil
import sqlite3
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[5]
sys.path[:0] = [str(ROOT / "apps/flow-studio/server"), str(ROOT / "apps/flow-studio/server/tests"),
                str(ROOT / "scripts")]
from replay import Failed, NotRun, Player, Waiting, Wire, chain  # noqa: E402
import importlib.util  # noqa: E402

spec = importlib.util.spec_from_file_location("flows", ROOT / "scripts/check-flows.py")
flows = importlib.util.module_from_spec(spec)
sys.argv, argv = [sys.argv[0]], sys.argv
spec.loader.exec_module(flows)
sys.argv = argv

OPERATOR = {"id": "operator"}


def publish(wire, module, text):
    d = wire.post("/v1/request", {"actor": OPERATOR, "type": "DeclarationChange", "transition": "draft",
                                  "inputs": {"source": {module: text}, "source_ref": f"fixture:{module}"}})
    assert d["kind"] == "satisfied", d
    s = wire.post("/v1/request", {"actor": OPERATOR, "object_id": d["object_id"], "transition": "submit"})
    assert s["kind"] == "satisfied", s
    p = wire.post("/v1/publish", {"actor": OPERATOR, "change": d["object_id"], "expected_version": s["version"]})
    assert p["verdict"]["kind"] == "satisfied", p


def played(wire, examples_text, declared):
    """Each example played on the store as far as a deployment plays it: its outcome. A store keeps what every
    example writes, so each chain of setups is played once, and the examples given it play on what it made; a
    chain that stops leaves its examples unplayed."""
    doc = flows.load(examples_text)
    setups = doc.get("setups") or {}
    out, players = {}, {}

    def steps(p, part):
        if part.get("evaluators"):
            raise NotRun("it answers evaluators")
        for st in part.get("steps") or []:
            v = p.play(st)
            if v is not None and v["kind"] != "satisfied":
                raise Failed(f"a step is refused as {v['kind']}")

    for name, ex in (doc.get("examples") or {}).items():
        given = ex.get("given")
        try:
            if given not in players:
                players[given] = None
                p = Player(wire, wire.get("/health")["now"])
                p.declared = declared
                for part in chain(setups, given) if given else []:
                    steps(p, part)
                players[given] = p
            p = players[given]
            if p is None:
                raise Failed("its setups stopped")
            steps(p, ex)
            p.play({"approve": ex["approve"]} if "approve" in ex else {"request": ex["request"]})
            out[name] = "played"
        except Exception as e:      # a deployment has no sandbox controls, and a store keeps what it wrote
            out[name] = f"stopped: {type(e).__name__}: {e}"[:200]
    return out


def built_ins(wire, types):
    """What every store may hold beside its flow: a label on an object of the first declared type that has one, and
    a subscription to that type."""
    for tn in types:
        items = wire.post("/v1/query", {"actor": OPERATOR, "type": tn}).get("items") or []
        if items:
            break
    else:
        return
    for body in ({"type": "label", "transition": "record", "inputs": {"subject": items[0]["id"], "name": "fixture"}},
                 {"type": "Subscription", "transition": "subscribe", "inputs": {"target_type": tn}}):
        v = wire.post("/v1/request", dict(body, actor=OPERATOR))
        assert v["kind"] == "satisfied", v


def record(wire, types):
    """What a later release must answer of the store alike: every object's history and stored attributes, and the
    exported log."""
    objects = {}
    for tn in types + ["Operator", "DeclarationChange", "Proposal", "label", "Subscription"]:
        page = wire.post("/v1/query", {"actor": OPERATOR, "type": tn})
        for item in page.get("items") or []:
            got = wire.post("/v1/get", {"actor": OPERATOR, "id": item["id"]})
            decl = wire.post("/v1/declaration", {"actor": OPERATOR, "type": got["type"]})
            derived = set((decl.get("derived_attributes") or {}))
            objects[item["id"]] = {
                "type": got["type"], "state": got["state"], "version": got["version"],
                "attributes": {k: v for k, v in got["attributes"].items() if k not in derived},
                "history": wire.post("/v1/history", {"actor": OPERATOR, "id": item["id"]})}
    log, cursor = [], None
    while True:
        page = wire.post("/v1/export", {"actor": OPERATOR, "source": "log", "cursor": cursor})
        log += page["lines"]
        if not page.get("cursor") or page.get("cursor") == cursor:
            break
        cursor = page["cursor"]
    return {"objects": objects, "log": log}


def main():
    service, module_file, out_dir = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
    out_dir.mkdir(parents=True, exist_ok=True)
    name = module_file.stem
    store = out_dir / f"{name}.sqlite"
    for p in out_dir.glob(f"{name}.sqlite*"):
        p.unlink()
    # a new store is made only when asked (since 0.2.0), and the start says where it listens among its other lines
    made = ["--create"] if "0.1.0" not in subprocess.run([str(service), "--version"], capture_output=True,
                                                         text=True).stdout else []
    proc = subprocess.Popen([str(service), "--store", str(store), "--port", "0", *made], stderr=subprocess.PIPE,
                            text=True)
    try:
        said = []
        while not (listening := re.search(r"(http://127\.0\.0\.1:\d+)", said[-1] if said else "")):
            line = proc.stderr.readline()
            if not line:
                raise SystemExit(f"the service stopped before it listened: {''.join(said)}")
            said.append(line)
        url = listening.group(1)
        wire = Wire(url)
        release = wire.get("/health")["release"]
        text = module_file.read_text()
        publish(wire, json.loads(json.dumps(flows.load(text)["module"])), text)
        declared = list((flows.load(text).get("types") or {}))
        outcomes = played(wire, (module_file.parent / f"{name}.examples.yaml").read_text(), declared)
        built_ins(wire, declared)
        reads = record(wire, declared)
    finally:
        proc.terminate()
        proc.wait()
    # one file, its log folded in, as a fixture is kept
    with sqlite3.connect(store) as db:
        db.execute("PRAGMA wal_checkpoint(TRUNCATE)")
        db.execute("PRAGMA journal_mode = DELETE")
        db.execute("VACUUM")
    db.close()
    for extra in (out_dir / f"{name}.sqlite-wal", out_dir / f"{name}.sqlite-shm", out_dir / f"{name}.sqlite.lock"):
        if extra.exists():
            extra.unlink()
    (out_dir / f"{name}.reads.json").write_text(json.dumps(
        {"release": release, "module": module_file.name, "examples": outcomes, **reads}, indent=1, sort_keys=True))
    played_n = sum(1 for v in outcomes.values() if v == "played")
    print(f"{store.name}: {release}, {played_n} of {len(outcomes)} examples played, {len(reads['objects'])} objects, "
          f"{len(reads['log'])} log rows")


if __name__ == "__main__":
    main()

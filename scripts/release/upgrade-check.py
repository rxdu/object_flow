#!/usr/bin/env python3
"""A PostgreSQL store made by one release's service, upgraded by another's, and read alike, object by object.

For each of the two example modules, reimbursement and servicedesk, the old service makes a store on the check
database, publishes the module, plays its examples as far as they play and records every object it can read. The new
service then answers the store as due an upgrade, runs a dry run that leaves it due, upgrades it, and serves it at the
new release. Each object is read again: its state and attributes as before, and each earlier event as the old release
answered it. A key the new release adds to an event is named, and held empty, since no event before the upgrade has a
value of it; `declaration_type` (ADR-0244) is the object's own type, since no version of these modules moves one.

The release gate runs it with the release before's published service and the release build; the release's own
verification runs it with the published service (`scripts/release/gate`, `scripts/release/verify`). SQLite's upgrade is
held by the deployment tests' fixtures (`core/crates/objectflow-service/tests/fixtures/`).

Usage:

    upgrade-check.py <old objectflow-service> <new objectflow-service> <new release, e.g. "objectflow-core 0.17.0">

Needs the check database `scripts/setup/postgres-check-db` starts, on 127.0.0.1:5432. It makes and drops a database
`of_upgrade_<module>` there. Exits 1 where a check fails for either module.
"""
import importlib.util
import json
import os
import pathlib
import re
import subprocess
import sys
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[2]
CONTAINER = "objectflow-check-db"

old, new, expected = sys.argv[1:4]
maker_path = ROOT / "core/crates/objectflow-service/tests/fixtures/make-release-store.py"
spec = importlib.util.spec_from_file_location("maker", maker_path)
maker = importlib.util.module_from_spec(spec)
argv = sys.argv
sys.argv = [str(maker_path)]
spec.loader.exec_module(maker)
sys.argv = argv
os.environ["OBJECTFLOW_STORE_PASSWORD"] = "objectflow"


def psql(sql):
    subprocess.run(["docker", "exec", CONTAINER, "psql", "-q", "-U", "objectflow", "-d", "objectflow", "-c", sql],
                   check=True)


def started(service, store, *extra):
    proc = subprocess.Popen([service, "--store", store, "--port", "0", *extra], stderr=subprocess.PIPE, text=True)
    said = []
    while not (m := re.search(r"(http://127\.0\.0\.1:\d+)", said[-1] if said else "")):
        line = proc.stderr.readline()
        if not line:
            raise SystemExit(f"the service stopped before it listened: {''.join(said)}")
        said.append(line)
    return proc, maker.Wire(m.group(1))


def health_of(wire):
    """The health answer, 503 included, which a store due an upgrade answers."""
    try:
        with urllib.request.urlopen(wire.base + "/health") as r:
            return r.status, json.loads(r.read())
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read() or b"{}")


failed = []
for name in ("reimbursement", "servicedesk"):
    module = ROOT / "docs/design/flow-format/examples" / f"{name}.yaml"
    db = f"of_upgrade_{name}"
    psql(f"DROP DATABASE IF EXISTS {db}")
    psql(f"CREATE DATABASE {db}")
    store = f"postgresql://objectflow@127.0.0.1:5432/{db}"
    proc, wire = started(old, store, "--create")
    try:
        made_by = wire.get("/health")["release"]
        text = module.read_text()
        maker.publish(wire, json.loads(json.dumps(maker.flows.load(text)["module"])), text)
        declared = list((maker.flows.load(text).get("types") or {}))
        outcomes = maker.played(wire, (module.parent / f"{name}.examples.yaml").read_text(), declared)
        maker.built_ins(wire, declared)
        before = maker.record(wire, declared)
    finally:
        proc.terminate()
        proc.wait()
    proc, wire = started(new, store)
    try:
        op = {"id": "operator"}
        status, health = health_of(wire)
        dry = wire.post("/v1/upgrade", {"actor": op, "dry_run": True})
        _, still = health_of(wire)
        done = wire.post("/v1/upgrade", {"actor": op})
        _, after_health = health_of(wire)
        checks = {
            "due before": (status, health.get("upgrade_due"), health.get("store_release")) == (503, True, made_by),
            "dry run": (dry.get("from"), dry.get("to"), dry.get("upgraded")) == (made_by, expected, False),
            "due after the dry run": still.get("upgrade_due") is True,
            "upgraded": done.get("upgraded") is True,
            "served at the new release": (after_health.get("store_release"), after_health.get("upgrade_due"))
            == (expected, False),
        }
        differ, added = [], set()
        for oid, was in before["objects"].items():
            got = wire.post("/v1/get", {"actor": op, "id": oid})
            hist = wire.post("/v1/history", {"actor": op, "id": oid})
            if got.get("state") != was["state"] or any(
                    (got.get("attributes") or {}).get(k) != v for k, v in was["attributes"].items()):
                differ.append(f"{oid}: state or attributes")
            was_events, now_events = was["history"]["events"], hist["events"]

            def alike(w, n):
                extra = set(n) - set(w)
                added.update(extra)
                return ({k: n.get(k) for k in w} == w
                        and all(n[k] == got.get("type") if k == "declaration_type" else n[k] is None for k in extra))
            if oid == "operator":
                # the operator's history gains the upgrade's own events, one a step
                if not all(alike(w, n) for w, n in zip(was_events, now_events)) or len(now_events) != len(
                        was_events) + len(dry.get("steps") or []):
                    differ.append(f"{oid}: history")
            elif len(now_events) != len(was_events) or not all(alike(w, n) for w, n in zip(was_events, now_events)):
                differ.append(f"{oid}: history")
        checks["read alike"] = not differ
    finally:
        proc.terminate()
        proc.wait()
    played = sum(1 for v in outcomes.values() if v == "played")
    print(f"{name}: made by {made_by}, {played} of {len(outcomes)} examples played, {len(before['objects'])} objects; "
          + ", ".join(f"{k}: {'yes' if v else 'NO'}" for k, v in checks.items())
          + (f"; differ: {differ[:5]}" if differ else "") + f"; keys added, empty on every earlier event: {sorted(added)}")
    if not all(checks.values()):
        failed.append(name)
    psql(f"DROP DATABASE IF EXISTS {db}")
sys.exit(1 if failed else 0)

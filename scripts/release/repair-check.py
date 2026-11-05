#!/usr/bin/env python3
"""ADR-0246's repair, on a store the published 0.12.0 made, taken through each step to the release under test.

0.12.0 left a stored set's write-index rows under its old name when a version renamed the set, so a guard reading
`changed_since` of the renamed set let `close` through (ADR-0246). The old service makes the store from
`repair-check.yaml`: a card signed off and then tagged, so `close` is refused; a version then renames `tags` to
`keywords`, and `close` is let through, the defect. The new service's dry run leaves the row where it is; its upgrade
carries the row to the new name, leaves none of the old, and `close` is refused again.

The release gate runs it with the published 0.12.0 and the release build, the release's own verification with the
published 0.12.0 and the published service, each on both backends (`scripts/release/gate`, `scripts/release/verify`).

Usage:

    repair-check.py <0.12.0 objectflow-service> <new objectflow-service> <work dir> [<postgresql conninfo>]

Without a connection string the store is a file in the work dir; with one, such as
`host=localhost port=5432 user=objectflow`, without its `dbname`, it is the database `repair_0120` on the check
database `scripts/setup/postgres-check-db` starts, made and dropped here. Exits 1 where a check fails.
"""
import json
import os
import pathlib
import re
import sqlite3
import subprocess
import sys
import time
import urllib.error
import urllib.request

FLOW = pathlib.Path(__file__).resolve().with_suffix(".yaml")
CONTAINER = "objectflow-check-db"
OP = {"id": "operator"}
DB = "repair_0120"


class Service:
    """The service on a store, its stderr in a log file so its line per answer never fills a pipe."""

    def __init__(self, binary, store, log, create=False, env=None):
        args = [binary, "--store", store, "--port", "0"] + (["--create"] if create else [])
        self.out = open(log, "w")
        self.proc = subprocess.Popen(args, stderr=self.out, stdout=subprocess.DEVNULL, text=True,
                                     env={**os.environ, **(env or {})})
        deadline = time.time() + 30
        while not (m := re.search(r"(http://127\.0\.0\.1:\d+)", open(log).read())):
            if self.proc.poll() is not None or time.time() > deadline:
                self.stop()
                raise SystemExit(f"the service did not start: {open(log).read()}")
            time.sleep(0.05)
        self.base = m.group(1)

    def post(self, path, body):
        req = urllib.request.Request(self.base + path, json.dumps(body).encode(), {"content-type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=120) as r:
                return r.status, json.loads(r.read())
        except urllib.error.HTTPError as e:
            return e.code, json.loads(e.read() or b"{}")

    def stop(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=20)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
        self.out.close()

    def request(self, type_or_id, transition, inputs=None, creating=False):
        body = {"actor": OP, "transition": transition, "inputs": inputs or {}}
        body["type" if creating else "object_id"] = type_or_id
        return self.post("/v1/request", body)

    def submit(self, source):
        """A DeclarationChange drafted from `source`, its module files by name, and submitted: its id and the
        version it proposes."""
        s, d = self.post("/v1/request", {"actor": OP, "type": "DeclarationChange", "transition": "draft",
                                          "inputs": {"source": source, "source_ref": "repair-check"}})
        assert s == 200 and d["kind"] == "satisfied", (s, d)
        s, sub = self.post("/v1/request", {"actor": OP, "object_id": d["object_id"], "transition": "submit"})
        assert s == 200, (s, sub)
        return d["object_id"], sub.get("version")

    def publish(self, change, version):
        return self.post("/v1/publish", {"actor": OP, "change": change, "expected_version": version})


def psql(db, *args):
    """psql in the check database's container, on the database `db`: its output."""
    return subprocess.run(["docker", "exec", CONTAINER, "psql", "-U", "objectflow", "-d", db, *args], check=True,
                          capture_output=True, text=True).stdout


old, new, work = sys.argv[1], sys.argv[2], pathlib.Path(sys.argv[3])
pg = sys.argv[4] if len(sys.argv) > 4 else None
work.mkdir(parents=True, exist_ok=True)
backend = "postgresql" if pg else "sqlite"
if pg:
    # a database comparing text by bytes, as a store needs (ADR-0213)
    psql("objectflow", "-qc", f"DROP DATABASE IF EXISTS {DB} WITH (FORCE)")
    psql("objectflow", "-qc", f"CREATE DATABASE {DB} TEMPLATE template0 LOCALE_PROVIDER libc "
         "LC_COLLATE 'C' LC_CTYPE 'C' ENCODING 'UTF8'")
    store = f"{pg} dbname={DB}"
else:
    store = str(work / f"{DB}.sqlite")
    for suffix in ("", "-wal", "-shm", ".lock"):
        pathlib.Path(store + suffix).unlink(missing_ok=True)
ENV = {"OBJECTFLOW_STORE_PASSWORD": "objectflow"} if pg else None
failed = []


def check(svc, oid):
    s, v = svc.post("/v1/check", {"actor": OP, "object_id": oid, "transition": "close", "inputs": {}})
    return (v.get("verdict", v)).get("kind")


def rows(oid):
    sql = f"SELECT attribute, position FROM of_attribute_write WHERE object_id = '{oid}' ORDER BY attribute"
    if pg:
        return [tuple(line.split("|")) for line in psql(DB, "-tAc", sql).splitlines() if line]
    with sqlite3.connect(store) as conn:
        return [(a, str(p)) for a, p in conn.execute(sql).fetchall()]


def expect(what, got, want):
    print(f"[{backend}] {what}: {got}" + ("" if got == want else f"   EXPECTED {want}"))
    if got != want:
        failed.append(what)


text = FLOW.read_text()
svc = Service(old, store, str(work / f"repair-{backend}-old.log"), create=True, env=ENV)
try:
    change, version = svc.submit({"cards": text})
    assert svc.publish(change, version)[0] == 200
    card = svc.request("Card", "open", {"title": "Plan"}, creating=True)[1]["object_id"]
    assert svc.request(card, "sign")[0] == 200
    assert svc.request(card, "tag", {"tags": ["urgent"]})[1]["kind"] == "satisfied"
    expect("0.12.0, close before the rename", check(svc, card), "unsatisfied")
    v2 = text
    for a, b in [('      tags: { type: "string[]", optional: true }\n',
                  '      keywords: { type: "string[]", optional: true }\n'),
                 (" tags], signed_at)", " keywords], signed_at)"),
                 ("required_inputs: [tags], description: Sets", "required_inputs: [keywords], description: Sets")]:
        assert v2.count(a) == 1, a
        v2 = v2.replace(a, b)
    v2 += "migration:\n  description: Renamed.\n  renamed_attributes:\n    Card: { tags: keywords }\n"
    change, version = svc.submit({"cards": v2})
    assert svc.publish(change, version)[0] == 200
    expect("0.12.0, close after the rename (the defect)", check(svc, card), "satisfied")
finally:
    svc.stop()
tagged = dict(rows(card))["tags"]
print(f"[{backend}] rows 0.12.0 left: {rows(card)}")
svc = Service(new, store, str(work / f"repair-{backend}-new.log"), env=ENV)
try:
    s, dry = svc.post("/v1/upgrade", {"actor": OP, "dry_run": True})
    print(f"[{backend}] upgrade dry run: {s} {[st.get('to') or st for st in dry.get('steps', [])]}")
    expect("the dry run left the row", dict(rows(card)).get("tags"), tagged)
    s, done = svc.post("/v1/upgrade", {"actor": OP})
    expect("upgrade", (s, done.get("upgraded")), (200, True))
    expect("the row carried to the new name", dict(rows(card)).get("keywords"), tagged)
    expect("no row of the old name", dict(rows(card)).get("tags"), None)
    expect("after the upgrade, close", check(svc, card), "unsatisfied")
finally:
    svc.stop()
if pg:
    psql("objectflow", "-qc", f"DROP DATABASE IF EXISTS {DB} WITH (FORCE)")
else:
    for suffix in ("", "-wal", "-shm", ".lock"):
        pathlib.Path(store + suffix).unlink(missing_ok=True)
print(f"[{backend}] " + ("all held" if not failed else f"FAILED: {failed}"))
sys.exit(1 if failed else 0)

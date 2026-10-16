#!/usr/bin/env python3
"""A write's and an import's speed on a fresh store, the figures `core/README.md` states.

The third hiring review measured writes at about 80 ms and an import of 1,620 objects at 37 s, both on debug builds
(`docs/design/hiring-third-review.md` E12), and asked for the release build's figures. This measures them, on a store
the service makes with `--create` in a temporary directory and a module of its own, so the figures are the engine's
and not a flow's:

1. writes: `--clients` clients at once, each creating a note in a loop for `--seconds`, its count a second and the
   p50 and p95 of each request's time as the client saw it, over HTTP on the loopback;
2. an import: a batch of `--objects` objects of a mirror type, its dry run and then its run, each timed whole.

Measurement conditions, said with the result: the machine and its load, and the store's disk, since each commit is
the disk's flush (ADR-0208).

Usage:

    throughput.py <objectflow-service> [--clients 1,8,32] [--seconds 20] [--objects 1620]
"""
import argparse
import json
import pathlib
import re
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request

OPERATOR = {"id": "operator"}
FLOW = """module: bench
categories: [live, closed]
types:
  Note:
    description: A note, the smallest write a flow makes.
    tracking: record
    attributes:
      text: { type: string }
    states:
      OPEN: { category: live }
      DONE: { category: closed, final: true }
    transitions:
      jot: { kind: initial, to: OPEN, required_inputs: [text], description: Jots a note. }
      done: { kind: external, from: [OPEN], to: DONE, description: Files the note away. }
  Legacy:
    description: A record another system holds, which only the import writes.
    mirror: true
    tracking: record
    attributes:
      code: { type: string, external: legacy, indexed: true }
      name: { type: string, optional: true }
    states:
      HELD: { category: live }
      GONE: { category: closed, final: true }
"""


class Wire:
    def __init__(self, url):
        self.url = url

    def post(self, path, body):
        req = urllib.request.Request(self.url + path, json.dumps(body).encode(), {"content-type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=600) as r:
                return json.loads(r.read())
        except urllib.error.HTTPError as e:
            raise RuntimeError(f"{path} answered {e.code}: {e.read().decode()[:400]}") from None


def publish(wire):
    drafted = wire.post("/v1/request", {"actor": OPERATOR, "type": "DeclarationChange", "transition": "draft",
                                        "inputs": {"source": {"bench": FLOW}, "source_ref": "profile:bench"}})
    if drafted.get("kind") != "satisfied":
        raise SystemExit(f"the flow was not drafted: {json.dumps(drafted)[:600]}")
    submitted = wire.post("/v1/request", {"actor": OPERATOR, "object_id": drafted["object_id"],
                                          "transition": "submit"})
    if submitted.get("kind") != "satisfied":
        raise SystemExit(f"the flow was not submitted: {json.dumps(submitted)[:1200]}")
    approved = wire.post("/v1/publish", {"actor": OPERATOR, "change": drafted["object_id"],
                                         "expected_version": submitted["version"]})
    if approved["verdict"]["kind"] != "satisfied":
        raise SystemExit(f"the flow was not published: {json.dumps(approved)[:600]}")


def percentile(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(p * len(xs)))]


def writes(wire, clients, seconds):
    """Notes created by `clients` clients for `seconds`: the count a second, and each request's p50 and p95."""
    times, errors, lock = [], [], threading.Lock()
    stop = time.monotonic() + seconds

    def client(k):
        n = 0
        while time.monotonic() < stop:
            began = time.monotonic()
            try:
                v = wire.post("/v1/request", {"actor": OPERATOR, "type": "Note", "transition": "jot",
                                              "inputs": {"text": f"note {k}.{n}"}})
                if v.get("kind") != "satisfied":
                    raise RuntimeError(json.dumps(v)[:200])
            except Exception as e:  # each client's fault kept, and the measurement failed for it
                with lock:
                    errors.append(f"client {k}: {e}")
                return
            with lock:
                times.append(time.monotonic() - began)
            n += 1

    threads = [threading.Thread(target=client, args=(k,)) for k in range(clients)]
    began = time.monotonic()
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    took = time.monotonic() - began
    if errors:
        raise SystemExit("the writes failed:\n" + "\n".join(errors[:5]))
    return len(times) / took, percentile(times, 0.5), percentile(times, 0.95)


def imports(wire, objects):
    """A batch of `objects` mirror objects, its dry run and its run, each timed whole."""
    batch = {"objects": [{"type": "Legacy", "legacy_key": f"L-{i}", "state": "HELD",
                          "attributes": {"code": f"C{i:06d}", "name": f"Record {i}"}} for i in range(objects)]}
    out = {}
    for dry in (True, False):
        began = time.monotonic()
        v = wire.post("/v1/import_batch", {"actor": OPERATOR, "batch": batch, "dry_run": dry})
        out["dry run" if dry else "run"] = time.monotonic() - began
        if v.get("refusal") is not None:
            raise SystemExit(f"the import was refused: {json.dumps(v)[:600]}")
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("service")
    ap.add_argument("--clients", default="1,8,32")
    ap.add_argument("--seconds", type=int, default=20)
    ap.add_argument("--objects", type=int, default=1620)
    a = ap.parse_args()
    with tempfile.TemporaryDirectory() as tmp:
        store = pathlib.Path(tmp) / "bench.sqlite"
        proc = subprocess.Popen([a.service, "--store", str(store), "--create", "--port", "0"],
                                stderr=subprocess.PIPE, text=True)
        said = []
        try:
            while not (m := re.search(r"(http://127\.0\.0\.1:\d+)", said[-1] if said else "")):
                line = proc.stderr.readline()
                if not line:
                    raise SystemExit("the service stopped before it listened:\n" + "".join(said))
                said.append(line)
            wire = Wire(m.group(1))
            threading.Thread(target=lambda: [None for _ in proc.stderr], daemon=True).start()
            publish(wire)
            print(f"store: {store} on {tmp}")
            for clients in [int(c) for c in a.clients.split(",")]:
                rate, p50, p95 = writes(wire, clients, a.seconds)
                print(f"writes, {clients} client(s) for {a.seconds} s: {rate:.0f} a second, "
                      f"p50 {p50 * 1000:.1f} ms, p95 {p95 * 1000:.1f} ms")
            took = imports(wire, a.objects)
            print(f"import of {a.objects} objects: dry run {took['dry run']:.1f} s, run {took['run']:.1f} s")
        finally:
            proc.terminate()
            proc.wait()


if __name__ == "__main__":
    sys.exit(main())

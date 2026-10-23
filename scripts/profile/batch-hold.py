#!/usr/bin/env python3
"""How long a batch holds the writer, by its size, and what a batch at the bound does to a request's p95 (PRD C5).

ADR-0228 decision 9 bounds a batch by measuring its hold on the writer against C5. This is that measurement, on a
copy of a store `build-store.py` made, in two parts:

1. the hold: `--repeat` batches of each of `--sizes` drafts, one at a time with nothing else running, each batch's
   answer time taken as its hold on the writer, which it holds from its first request to its commit;
2. a request's p95 at the profile's steady writes, `--rate` drafts a second sent one by one as they arrive, for
   `--seconds`: first alone, then with a batch of `--bound` drafts sent every `--every` seconds beside them.

Each draft is a claim drafted by its claimant, as `load.py`'s writes are. Measurement conditions, said with the
result: the machine and its load, the store, the binary. An answer's time holds its HTTP exchange beside the hold.

Usage:

    batch-hold.py <objectflow-service> <store.sqlite> [--sizes 1,10,25,50,100] [--repeat 20] [--bound 100]
                  [--every 1] [--rate 50] [--seconds 60]
"""
import argparse
import concurrent.futures
import os
import pathlib
import random
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from load import OPERATOR, Wire, p, pages  # noqa: E402


def draft(claimant, n):
    return {"type": "Claim", "transition": "draft", "inputs": {"claimant": claimant, "title": f"Batch {n}"}}


def steady(wire, staff, a, bound, rng):
    """Single drafts at `--rate` a second for `--seconds`, a batch of `bound` every `--every` seconds where
    `bound` is not 0: the singles' times, the batches' times, and the faults."""
    singles, batches, faults, lock = [], [], [], threading.Lock()
    stop = time.monotonic() + a.seconds

    def single(n):
        claimant, identity = staff[n % len(staff)]
        status, out, took = wire.post("/v1/request", {"actor": {"id": identity}, **draft(claimant, n)})
        with lock:
            (singles if status == 200 and out.get("kind") == "satisfied" else faults).append(
                took if status == 200 else f"{status} {out.get('code', '')}")

    def batcher():
        n = 0
        while time.monotonic() < stop:
            claimant, identity = staff[rng.randrange(len(staff))]
            began = time.monotonic()
            status, out, took = wire.post("/v1/batch", {
                "actor": {"id": identity}, "requests": [draft(claimant, n + i) for i in range(bound)]})
            n += bound
            with lock:
                (batches if status == 200 and out.get("applied") else faults).append(
                    took if status == 200 else f"batch {status} {out.get('code', '')}")
            time.sleep(max(0.0, a.every - (time.monotonic() - began)))

    with concurrent.futures.ThreadPoolExecutor(max_workers=64) as pool:
        if bound:
            pool.submit(batcher)
        n, at = 0, time.monotonic()
        while at < stop:
            pool.submit(single, n)
            n += 1
            at += 1 / a.rate
            time.sleep(max(0.0, at - time.monotonic()))
    return singles, batches, faults


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("service")
    ap.add_argument("store")
    ap.add_argument("--sizes", default="1,10,25,50,100")
    ap.add_argument("--repeat", type=int, default=20)
    ap.add_argument("--bound", type=int, default=100)
    ap.add_argument("--every", type=float, default=1.0)
    ap.add_argument("--rate", type=float, default=50.0)
    ap.add_argument("--seconds", type=int, default=60)
    ap.add_argument("--seed", type=int, default=13)
    a = ap.parse_args()
    work = pathlib.Path(tempfile.mkdtemp(prefix="objectflow-batch-hold-"))
    copy = work / "store.sqlite"
    shutil.copy(a.store, copy)
    os.chmod(copy, 0o600)
    proc = subprocess.Popen([a.service, "--store", str(copy), "--port", "0", "--attempt-retention", "90 days"],
                            stderr=subprocess.PIPE, text=True)
    said = []
    try:
        while not (m := re.search(r"listening on (http://127\.0\.0\.1:\d+)", said[-1] if said else "")):
            line = proc.stderr.readline()
            if not line:
                raise SystemExit("the service stopped before it listened:\n" + "".join(said))
            said.append(line)
        threading.Thread(target=lambda: [None for _ in proc.stderr], daemon=True).start()
        wire = Wire(m.group(1))
        staff = [(o["id"], o["fields"]["user_id"])
                 for o in pages(wire, {"actor": OPERATOR, "type": "Employee", "fields": ["user_id"],
                                       "limit": 1000, "reads_personal": True})]
        rng = random.Random(a.seed)
        print(f"store {a.store}, {len(staff)} employees; {os.cpu_count()} cores; the binary {a.service}")
        print("the hold, by the batch's size, nothing else running:")
        n = 0
        for size in [int(s) for s in a.sizes.split(",")]:
            times = []
            for _ in range(a.repeat):
                claimant, identity = staff[rng.randrange(len(staff))]
                status, out, took = wire.post("/v1/batch", {
                    "actor": {"id": identity}, "requests": [draft(claimant, n + i) for i in range(size)]})
                n += size
                if status != 200 or not out.get("applied"):
                    raise SystemExit(f"a batch of {size} was not applied: {status} {str(out)[:300]}")
                times.append(took)
            print(f"  {size:>4} drafts: p50 {p(times, 0.5) * 1000:6.1f} ms, p95 {p(times, 0.95) * 1000:6.1f} ms, "
                  f"max {max(times) * 1000:6.1f} ms, {p(times, 0.5) * 1000 / size:5.2f} ms a draft at p50")
        for bound in (0, a.bound):
            singles, batches, faults = steady(wire, staff, a, bound, rng)
            beside = f"a batch of {bound} every {a.every:g} s" if bound else "no batch"
            print(f"{a.rate:g} drafts a second for {a.seconds} s, {beside}: {len(singles)} answered, "
                  f"p50 {p(singles, 0.5) * 1000:.1f} ms, p95 {p(singles, 0.95) * 1000:.1f} ms, "
                  f"max {max(singles, default=0) * 1000:.1f} ms"
                  + (f"; {len(batches)} batches, p95 {p(batches, 0.95) * 1000:.1f} ms" if bound else "")
                  + (f"; faults {faults[:5]}" if faults else ""))
    finally:
        proc.terminate()
        proc.wait(timeout=30)
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()

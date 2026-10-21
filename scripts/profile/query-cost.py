#!/usr/bin/env python3
"""What a query's filter costs when no index serves it, beside one an index serves, on a copy of a store.

The indexed derivation's item in `TODO.md` waits on a measured query: a filter on a derived attribute, or on a time
compared with `now`, is computed for each object of its type, so a page that finds few matches reads the type whole.
This reads each filter's first page, 50 claims, `--runs` times, one at a time, then every page of 1,000 once, and
prints how many claims matched, the first page's median, p95 and slowest, and the whole read's seconds.

The default filters are the reimbursement flow's: a worklist by state, which an index serves; a derivation over a set
(`over_policy_lines`), once matching many and once none; the time a claim entered its state compared with `now`,
alone and narrowed by state; and a time the claim stores, compared with `now`, without an index.

Measurement conditions, said with the result: the machine's load at the start and the store. A store
`build-store.py` made records every event in the minutes it took to build, so no claim there has aged and a filter
comparing a time with `now` matches none: its first page reads the type whole, the worst case.

Usage:

    query-cost.py <objectflow-service> <store.sqlite | postgresql> [--runs 10] [--filter <expression>]...

A PostgreSQL store is read as it is, query-cost.py writing nothing (ADR-0213).
"""
import argparse
import os
import pathlib
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from idle import postgres  # noqa: E402
from load import OPERATOR, Wire, p  # noqa: E402

FILTERS = [
    "state == Claim.SUBMITTED",
    "over_policy_lines > 0",
    "over_policy_lines > 2",
    "entered_at(state) + 14 days <= now",
    "state == Claim.SUBMITTED and entered_at(state) + 14 days <= now",
    "first_submitted_at is not null and first_submitted_at + 30 days <= now",
]


def query(wire, body):
    """A page of claims and how long it took, or the script stopped saying why it was refused."""
    status, out, took = wire.post("/v1/query", body, timeout=600)
    if status != 200 or out.get("refused"):
        raise SystemExit(f"{body['filter']}: {status} {out}")
    return out, took


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("service")
    ap.add_argument("store")
    ap.add_argument("--runs", type=int, default=10)
    ap.add_argument("--filter", action="append", help="a filter to time in place of the defaults; repeatable")
    a = ap.parse_args()
    work = pathlib.Path(tempfile.mkdtemp(prefix="objectflow-query-cost-"))
    if postgres(a.store):
        copy = a.store
    else:
        copy = work / "store.sqlite"
        shutil.copy(a.store, copy)
        os.chmod(copy, 0o600)
    proc = subprocess.Popen([a.service, "--store", str(copy), "--port", "0"], stderr=subprocess.PIPE, text=True)
    said = []
    try:
        while not (m := re.search(r"listening on (http://127\.0\.0\.1:\d+)", said[-1] if said else "")):
            line = proc.stderr.readline()
            if not line:
                raise SystemExit("the service stopped before it listened:\n" + "".join(said))
            said.append(line)
        threading.Thread(target=lambda: [None for _ in proc.stderr], daemon=True).start()
        wire = Wire(m.group(1))
        print(f"{a.store}, load {os.getloadavg()[0]:.2f}; a first page of 50 read {a.runs} times, then every page")
        print()
        print("| filter | matched | first page, median ms | p95 ms | slowest ms | every page, s |")
        print("|---|---|---|---|---|---|")
        for flt in a.filter or FILTERS:
            body = {"actor": OPERATOR, "type": "Claim", "filter": flt, "limit": 50}
            first = [query(wire, body)[1] for _ in range(a.runs)]
            matched, whole, cursor = 0, 0.0, None
            while True:
                out, took = query(wire, {**body, "limit": 1000, **({"cursor": cursor} if cursor else {})})
                matched += len(out["items"])
                whole += took
                if not (cursor := out.get("cursor")):
                    break
            print(f"| `{flt}` | {matched} | {statistics.median(first) * 1e3:.0f} | {p(first, 0.95) * 1e3:.0f} | "
                  f"{max(first) * 1e3:.0f} | {whole:.1f} |", flush=True)
    finally:
        proc.terminate()
        proc.wait()
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()

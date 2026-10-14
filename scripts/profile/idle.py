#!/usr/bin/env python3
"""Gate I: each read of the profile timed alone, one client at a time, on a copy of a store.

The audit's Gate I (`docs/design/audit/cost.md` §3) ran each operation idle on both profile stores and failed where
`operate.md` gives a figure the operation passes by more than twice. This times the reads `load.py` plays, each
`--runs` times on a store `build-store.py` made, with the service's resident memory after each, and prints the median
and the slowest. A read whose median passes PRD C5's target, 1 s for a metric and 100 ms for one object, is named
FAILED, and the script exits 1.

Measurement conditions, said with the result: the machine's load at the start and the store.

Usage:

    idle.py <objectflow-service> <store.sqlite> [--runs 5]
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
from load import OPERATOR, REPORTS, Wire, pages  # noqa: E402


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("service")
    ap.add_argument("store")
    ap.add_argument("--runs", type=int, default=5)
    a = ap.parse_args()
    work = pathlib.Path(tempfile.mkdtemp(prefix="objectflow-idle-"))
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
        claims = pages(wire, {"actor": OPERATOR, "type": "Claim", "fields": ["claimant"], "limit": 1000})
        claim = claims[len(claims) // 2]["id"]

        def rss():
            status = pathlib.Path(f"/proc/{proc.pid}/status").read_text()
            return int(re.search(r"VmRSS:\s+(\d+) kB", status).group(1)) * 1024

        reads = [(f"metric {name}", "/v1/metric", {"actor": OPERATOR, "name": name, "keep": keep,
                                                   "reads_personal": True}, 1.0)
                 for name, keep in REPORTS]
        reads += [("get a claim", "/v1/get", {"actor": OPERATOR, "id": claim}, 0.1),
                  ("availability of a claim", "/v1/availability",
                   {"actor": OPERATOR, "id": claim, "reads_personal": True}, 0.1),
                  ("worklist of submitted claims", "/v1/query",
                   {"actor": OPERATOR, "type": "Claim", "filter": "state == Claim.SUBMITTED", "limit": 50}, None)]
        load = os.getloadavg()
        print(f"Gate I: {pathlib.Path(a.store).name}, {len(claims)} claims, {a.runs} runs each; load average at the "
              f"start {load[0]:.1f}, {os.cpu_count()} threads")
        print("| read | median ms | slowest ms | resident MiB after |")
        print("|---|---|---|---|")
        failed = []
        for name, path, body, bound in reads:
            times = []
            for _ in range(a.runs):
                status, out, took = wire.post(path, body, timeout=120)
                if status != 200 or (isinstance(out, dict) and out.get("refused")):
                    failed.append(f"{name} answered {status} {str(out)[:200]}")
                    break
                times.append(took)
            if not times:
                continue
            median = statistics.median(times)
            print(f"| {name} | {median * 1e3:.1f} | {max(times) * 1e3:.1f} | {rss() / (1 << 20):.0f} |")
            if bound is not None and median > bound:
                failed.append(f"{name}'s median is {median * 1e3:.0f} ms, past {bound * 1e3:.0f} ms")
        for f in failed:
            print(f"FAILED: {f}")
        print("Gate I: " + ("failed" if failed else "passed"))
        return 1 if failed else 0
    finally:
        proc.terminate()
        proc.wait()
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())

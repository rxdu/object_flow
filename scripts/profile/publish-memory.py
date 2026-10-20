#!/usr/bin/env python3
"""A publish's memory, phase by phase (TODO.md, MEMORY): the service started on a copy of a store `build-store.py`
made, its resident memory sampled every 10 ms from /proc, a change to the reimbursement module drafted, submitted, which
runs its dry run, and approved, which runs it again and installs it, and each phase's peak and what the process holds
after it printed. The change adds one optional attribute to Claim, so every claim is mapped through the new version.

With --valgrind the service runs under valgrind's massif instead, its heap written to the file named, which
`massif-sites.py` reads; nothing is sampled then, the service being some twenty times slower.

Measurement conditions, said with the result: the machine's load, the store, the build (an optimised one, so built
with `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only` for massif to name its sites).

Usage:

    publish-memory.py <objectflow-service> <store.sqlite> [--runs 3] [--valgrind <massif.out>]
"""
import argparse
import json
import pathlib
import re
import shutil
import subprocess
import tempfile
import threading
import time
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[2]
FLOW = ROOT / "docs/design/flow-format/examples/reimbursement.yaml"
OP = {"id": "operator"}


def post(url, path, body, timeout=3600):
    req = urllib.request.Request(url + path, data=json.dumps(body).encode(), method="POST",
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())


def rss_kib(pid):
    for line in pathlib.Path(f"/proc/{pid}/status").read_text().splitlines():
        if line.startswith("VmRSS:"):
            return int(line.split()[1])
    return 0


def changed(text):
    """The module with one optional attribute added to Claim, a change every object of its type is mapped through."""
    marker = "  Claim:\n"
    i = text.index(marker)
    j = text.index("    attributes:\n", i) + len("    attributes:\n")
    return text[:j] + "      reviewer_note: { type: string, optional: true, description: A note the reviewer leaves. }\n" + text[j:]


def run(service, store, massif=None):
    work = pathlib.Path(tempfile.mkdtemp(prefix="pubmem-"))
    copy = work / "store.sqlite"
    shutil.copy(store, copy)
    cmd = [str(service), "--store", str(copy), "--port", "0", "--dry-run-timeout", "60 min",
           "--read-timeout", "60 min"]
    if massif:
        cmd = ["valgrind", "--tool=massif", f"--massif-out-file={massif}", "--max-snapshots=400",
               "--detailed-freq=4", "--threshold=0.5"] + cmd
    proc = subprocess.Popen(cmd, stderr=subprocess.PIPE, text=True)
    said = []
    while not (m := re.search(r"(http://127\.0\.0\.1:\d+)", said[-1] if said else "")):
        line = proc.stderr.readline()
        if not line:
            raise SystemExit("the service stopped: " + "".join(said))
        said.append(line)
    threading.Thread(target=lambda: [None for _ in proc.stderr], daemon=True).start()
    url = m.group(1)
    pid = proc.pid
    samples, phase, stop = [], ["start"], [False]

    def sample():
        while not stop[0]:
            try:
                samples.append((time.monotonic(), phase[0], rss_kib(pid)))
            except (FileNotFoundError, ProcessLookupError):
                return
            time.sleep(0.01)
    if not massif:
        threading.Thread(target=sample, daemon=True).start()
    time.sleep(1.0)
    timed = {}

    def step(name, path, body):
        phase[0] = name
        began = time.monotonic()
        out = post(url, path, body)
        timed[name] = time.monotonic() - began
        phase[0] = name + " after"
        time.sleep(2.0)             # the trim after a heavy request, which runs once the answer is made
        return out
    text = changed(FLOW.read_text())
    drafted = step("draft", "/v1/request", {"actor": OP, "type": "DeclarationChange", "transition": "draft",
                                            "inputs": {"source": {"reimbursement": text},
                                                       "source_ref": "profile:reimbursement-changed"}})
    change = drafted["object_id"]
    submitted = step("submit", "/v1/request", {"actor": OP, "object_id": change, "transition": "submit"})
    assert submitted["kind"] == "satisfied", submitted
    approved = step("approve", "/v1/publish", {"actor": OP, "change": change,
                                               "expected_version": submitted["version"]})
    assert approved["verdict"]["kind"] == "satisfied", approved
    stop[0] = True
    proc.terminate()
    proc.wait()
    shutil.rmtree(work)
    if massif:
        return timed, None
    out = {}
    for name in ("start", "draft", "draft after", "submit", "submit after", "approve", "approve after"):
        rs = [r for _, p, r in samples if p == name]
        if rs:
            out[name] = (max(rs) / 1024, rs[-1] / 1024)
    return timed, out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("service")
    ap.add_argument("store")
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--valgrind")
    a = ap.parse_args()
    if a.valgrind:
        timed, _ = run(a.service, a.store, massif=a.valgrind)
        print("under massif:", {k: round(v, 1) for k, v in timed.items()})
        return
    load = pathlib.Path("/proc/loadavg").read_text().split()[0]
    print(f"{pathlib.Path(a.store).name}, load average at the start {load}")
    for i in range(a.runs):
        timed, out = run(a.service, a.store)
        print(f"run {i + 1}: " + ", ".join(f"{k} {v:.1f} s" for k, v in timed.items()))
        for name, (peak, last) in out.items():
            print(f"  {name:14} peak {peak:8.1f} MiB   last {last:8.1f} MiB")


if __name__ == "__main__":
    main()

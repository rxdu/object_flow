#!/usr/bin/env python3
"""Gate P: ADR-0204 §1's load profile against a store, held to PRD C5 and the audit's thresholds.

The audit of 2026-10-14 defined the gate and did not keep its scripts (`docs/design/audit/cost.md` §3, "Gate P"). This
is that gate: the service started on a copy of a store `build-store.py` made, and the profile's steady load played on
it for `--seconds`, at `--scale` 1 or 2, the month-end peak being twice the steady load:

- sixteen people's sessions at once, each a mix of one-object reads, worklists and writes, the writes of every
  session together held to 50 a second;
- four of them reading reports: per-person and standard metrics, opted into reading by a person as the team's
  application opts in for a manager (ADR-0206);
- two agents sweeping through their reads.

It fails, exiting 1, where any of these holds, as the audit set them:

- a write's p95 is past 100 ms, or a one-object read's;
- a metric's p95 is past 1 s;
- any answer is a 5xx, a 503 among them;
- the service's resident memory passes `--memory-max`, `operate.md`'s MemoryMax for the store.

Measurement conditions, said with the result: the machine and its load, the store, the duration. A shared machine
gives higher numbers than a quiet one. The profile's other parts, retries after timeouts, a restart, a publish, an
erasure batch, a backup and a prune, belong to the soak (ADR-0204 §7) and are not played here.

Usage:

    load.py <objectflow-service> <store.sqlite> [--seconds 150] [--scale 1] [--memory-max 8G]
            [--sessions N] [--reporters N] [--agents N]
"""
import argparse
import json
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
import urllib.error
import urllib.request

OPERATOR = {"id": "operator"}
WRITES_A_SECOND = 50
REPORTS = [
    ("Claim.claimant_amount_claimed", ["claimant"]),
    ("Claim.claimant_over_policy_lines", ["claimant"]),
    ("Claim.submission_to_payment", []),
    ("ClaimLine.amount_claimed_by_category", ["expense_category"]),
    ("Claim.time_in_state", ["state"]),
    ("Claim.throughput", []),
]


def size(text):
    m = re.fullmatch(r"(\d+)([KMG]?)", text)
    if not m:
        raise SystemExit(f"--memory-max {text}: a number of bytes, or one followed by K, M or G")
    return int(m.group(1)) * {"": 1, "K": 1 << 10, "M": 1 << 20, "G": 1 << 30}[m.group(2)]


class Wire:
    def __init__(self, url):
        self.url = url

    def post(self, path, body, timeout=60):
        """The answer's status, its body, and how long it took in seconds."""
        req = urllib.request.Request(self.url + path, json.dumps(body).encode(), {"content-type": "application/json"})
        began = time.monotonic()
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                status, out = r.status, r.read()
        except urllib.error.HTTPError as e:
            status, out = e.code, e.read()
        except (urllib.error.URLError, TimeoutError, ConnectionError) as e:
            return 599, {"fault": str(e)}, time.monotonic() - began
        took = time.monotonic() - began
        try:
            return status, json.loads(out), took
        except ValueError:
            return status, {"fault": out[:200].decode(errors="replace")}, took


class Record:
    """Each answer's class, status and duration."""

    def __init__(self):
        self.lock = threading.Lock()
        self.times = {}
        self.faults = {}

    def add(self, kind, status, took, body):
        with self.lock:
            self.times.setdefault(kind, []).append(took)
            # a read refused in a 200 is a fault of the profile, never a fast answer
            refused = body.get("refused") if isinstance(body, dict) else None
            if status >= 500 or refused:
                code = (body.get("code", "") if isinstance(body, dict) else "") or f"refused {refused}"
                self.faults.setdefault(f"{kind} {status} {code}", 0)
                self.faults[f"{kind} {status} {code}"] += 1


def p(values, q):
    if not values:
        return 0.0
    s = sorted(values)
    return s[min(len(s) - 1, int(q * len(s)))]


class Bucket:
    """Writes a second, shared by every session."""

    def __init__(self, rate):
        self.rate, self.tokens, self.at, self.lock = rate, rate, time.monotonic(), threading.Lock()

    def take(self):
        with self.lock:
            now = time.monotonic()
            self.tokens = min(self.rate, self.tokens + (now - self.at) * self.rate)
            self.at = now
            if self.tokens >= 1:
                self.tokens -= 1
                return True
            return False


def pages(wire, body):
    """Every item of a query, page by page."""
    items, cursor = [], None
    while True:
        status, page, _ = wire.post("/v1/query", {**body, **({"cursor": cursor} if cursor else {})})
        if status != 200:
            raise SystemExit(f"the store's directory was not read: {status} {json.dumps(page)[:300]}")
        items.extend(page["items"])
        cursor = page.get("cursor")
        if not cursor:
            return items


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("service")
    ap.add_argument("store")
    ap.add_argument("--seconds", type=int, default=150)
    ap.add_argument("--scale", type=int, default=1, choices=[1, 2])
    ap.add_argument("--memory-max", default="8G")
    ap.add_argument("--seed", type=int, default=11)
    # each part of the profile set apart, to tell what a result comes from: `--reporters 0 --agents 0` plays the
    # sessions and their writes alone
    ap.add_argument("--sessions", type=int, help="people's sessions, 16 at scale 1")
    ap.add_argument("--reporters", type=int, help="report readers, 4 at scale 1")
    ap.add_argument("--agents", type=int, help="agents, 2 at scale 1")
    a = ap.parse_args()
    sessions = 16 * a.scale if a.sessions is None else a.sessions
    reporters = 4 * a.scale if a.reporters is None else a.reporters
    agents = 2 * a.scale if a.agents is None else a.agents
    memory_max = size(a.memory_max)
    work = pathlib.Path(tempfile.mkdtemp(prefix="objectflow-profile-"))
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
        claims = [(o["id"], o["fields"]["claimant"])
                  for o in pages(wire, {"actor": OPERATOR, "type": "Claim", "fields": ["claimant"], "limit": 1000})]
        categories = [o["id"] for o in pages(wire, {"actor": OPERATOR, "type": "ExpenseCategory", "limit": 1000})]
        identity = dict(staff)
        record, bucket = Record(), Bucket(WRITES_A_SECOND * a.scale)
        stop = time.monotonic() + a.seconds
        peak = [0]

        def session(k):
            rng = random.Random(a.seed * 100 + k)
            while time.monotonic() < stop:
                claim, claimant = rng.choice(claims)
                me = {"id": identity.get(claimant, "operator")}
                if bucket.take():
                    status, out, took = wire.post("/v1/request", {
                        "actor": me, "type": "Claim", "transition": "draft",
                        "inputs": {"claimant": claimant, "title": "Profile"}})
                    record.add("write", status, took, out)
                    continue
                roll = rng.random()
                if roll < 0.6:
                    status, out, took = wire.post("/v1/get", {"actor": me, "id": claim})
                    record.add("one-object", status, took, out)
                elif roll < 0.8:
                    # an offer decided by a personal member, read as the team's application opts in for a
                    # person's own claim (ADR-0206)
                    status, out, took = wire.post("/v1/availability", {"actor": me, "id": claim,
                                                                        "reads_personal": True})
                    record.add("one-object", status, took, out)
                else:
                    status, out, took = wire.post("/v1/query", {
                        "actor": me, "type": "Claim", "filter": "state == Claim.SUBMITTED", "limit": 50})
                    record.add("worklist", status, took, out)

        def reports(k, actor, kind):
            rng = random.Random(a.seed * 1000 + k)
            while time.monotonic() < stop:
                name, keep = rng.choice(REPORTS)
                status, out, took = wire.post("/v1/metric", {"actor": actor, "name": name, "keep": keep,
                                                             "reads_personal": True})
                record.add(f"metric {name}", status, took, out)
                if kind == "agent":
                    status, out, took = wire.post("/v1/query", {
                        "actor": actor, "type": "ClaimLine", "filter": "state == ClaimLine.LISTED", "limit": 20})
                    record.add("agent query", status, took, out)

        def rss():
            while time.monotonic() < stop:
                try:
                    status = pathlib.Path(f"/proc/{proc.pid}/status").read_text()
                    kb = int(re.search(r"VmRSS:\s+(\d+) kB", status).group(1))
                    peak[0] = max(peak[0], kb * 1024)
                except (OSError, AttributeError):
                    pass
                time.sleep(0.5)

        threads = [threading.Thread(target=session, args=(k,)) for k in range(sessions)]
        threads += [threading.Thread(target=reports, args=(k, OPERATOR, "person")) for k in range(reporters)]
        threads += [threading.Thread(target=reports, args=(100 + k, {"id": "assistant"}, "agent"))
                    for k in range(agents)]
        threads.append(threading.Thread(target=rss))
        load = os.getloadavg()
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        failed = []
        print(f"Gate P: {pathlib.Path(a.store).name}, {len(claims)} claims, {len(staff)} employees, scale {a.scale}, "
              f"{a.seconds} s, {sessions} sessions, {reporters} report readers, {agents} agents; load average at the "
              f"start {load[0]:.1f}, {os.cpu_count()} threads")
        print("| class | answers | p50 ms | p95 ms | max ms |")
        print("|---|---|---|---|---|")
        for kind in sorted(record.times):
            t = record.times[kind]
            print(f"| {kind} | {len(t)} | {p(t, 0.5) * 1e3:.1f} | {p(t, 0.95) * 1e3:.1f} | {max(t) * 1e3:.1f} |")
            bound = 1.0 if kind.startswith("metric") else 0.1 if kind in ("write", "one-object") else None
            if bound is not None and p(t, 0.95) > bound:
                failed.append(f"{kind}'s p95 is {p(t, 0.95) * 1e3:.0f} ms, past {bound * 1e3:.0f} ms")
        for fault, n in sorted(record.faults.items()):
            failed.append(f"{n} answered {fault}")
        print(f"peak resident memory: {peak[0] / (1 << 20):.0f} MiB, against {memory_max / (1 << 20):.0f} MiB")
        if peak[0] > memory_max:
            failed.append(f"resident memory reached {peak[0] / (1 << 20):.0f} MiB")
        for f in failed:
            print(f"FAILED: {f}")
        print("Gate P: " + ("failed" if failed else "passed"))
        return 1 if failed else 0
    finally:
        proc.terminate()
        proc.wait()
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())

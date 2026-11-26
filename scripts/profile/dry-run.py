#!/usr/bin/env python3
"""A dry run's cost (issue #116, D706): the service started on a new SQLite store, a change drafted with the sample
modules and their examples, and submitted, which runs the dry run: the flows' check and every example. The submit's
wall time and the service's CPU seconds over it, in user space and in the kernel, are printed for each run, with the
machine's load at the start.

With `--parts K` the change also carries a generated module of K types, each with a composite part of its own that a
cascade drives, a peer it calls, a personal attribute and 26 transitions, and K part types: a library about the size
of a deployment's, which no example reads. It grows the library and not the examples, so what each request pays for
the library's size shows. With `--parts 15` the library holds 61 types; the published 0.30.0's dry run took 4.6 s of
CPU, 0.32.2's 10.0 s.

The samples are `docs/design/flow-format/examples`: every module but `servicedesk`, which declares names `issues`
and `reimbursement` declare too, with `inventory-v2` as `inventory`, which `delivery` imports. Four of `delivery`'s
examples fail against it, so the dry run refuses the change, after running every example.

Measurement conditions, said with the result: the machine's load, and the service's build.

Usage:

    dry-run.py <objectflow-service> [--runs 3] [--parts 15]
"""
import argparse
import json
import os
import pathlib
import re
import shutil
import subprocess
import tempfile
import threading
import time
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[2]
EXAMPLES = ROOT / "docs/design/flow-format/examples"
# each module of the change, by its name, and the file it is read from
MODULES = {"people": "people", "approvals": "approvals", "bookings": "bookings", "customers": "customers",
           "inventory": "inventory-v2", "delivery": "delivery", "issues": "issues",
           "reimbursement": "reimbursement", "service": "service"}
OP = {"id": "operator"}


def post(url, path, body, timeout=3600):
    req = urllib.request.Request(url + path, data=json.dumps(body).encode(), method="POST",
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())


def cpu(pid):
    """The process's CPU seconds over all its threads, in user space and in the kernel."""
    stat = pathlib.Path(f"/proc/{pid}/stat").read_text()
    fields = stat[stat.rindex(")") + 2:].split()
    tick = os.sysconf("SC_CLK_TCK")
    return int(fields[11]) / tick, int(fields[12]) / tick


def generated(k, edits=26):
    """A module of `k` types, each with a part type of its own that a cascade on `shut` drives, a peer it calls on
    each of its `edits` transitions, and a personal address an erasure clears."""
    out = ["module: bulk", "categories: [live, closed]", "", "types:"]
    for i in range(k):
        out += [f"  Bulk{i}:",
                f"    description: A generated record, number {i}.",
                "    tracking: record",
                "    attributes:",
                "      name: { type: string }",
                "      note: { type: string, optional: true }",
                "      email: { type: string, optional: true, personal: true }",
                f"      peer: {{ reference: Bulk{(i + 1) % k}, optional: true }}",
                "      parts:",
                f"        reference: \"BulkPart{i}[]\"",
                "        aggregation: composite",
                "        opposite: owner",
                "        description: Its parts, discarded as it shuts.",
                "        cascade:",
                "          - { on: [shut], transition: discard, limit: 50 }",
                "    states:",
                "      OPEN: { category: live }",
                "      SHUT: { category: closed, final: true }",
                "    transitions:",
                "      add:",
                "        kind: initial",
                "        to: OPEN",
                "        description: Adds one, with its first part.",
                "        required_inputs: [name]",
                "        optional_inputs: [note, email, peer]",
                "        effect:",
                f"          - create: {{ type: BulkPart{i}, transition: add, inputs: {{ owner: this, label: inputs.name }} }}",
                "      poke:",
                "        kind: internal",
                "        from: [OPEN]",
                "        description: Writes a note.",
                "        required_inputs: [note]"]
        for j in range(edits):
            out += [f"      edit{j}:",
                    "        kind: internal",
                    "        from: [OPEN]",
                    f"        description: Edits it, way {j}.",
                    "        required_inputs: [note]",
                    "        optional_inputs: [email]",
                    "        effect:",
                    "          - call: { target: peer, transition: poke, inputs: { note: inputs.note } }"]
        out += ["      forget:",
                "        kind: erasure",
                "        description: Erases its address.",
                "        inputs:",
                "          reason: { type: string }",
                "      shut:",
                "        kind: external",
                "        from: [OPEN]",
                "        to: SHUT",
                "        description: Shuts it.",
                f"  BulkPart{i}:",
                f"    description: A part of generated record {i}.",
                "    tracking: record",
                "    attributes:",
                f"      owner: {{ reference: Bulk{i}, opposite: parts }}",
                "      label: { type: string }",
                "    states:",
                "      OPEN: { category: live }",
                "      DISCARDED: { category: closed, final: true }",
                "    transitions:",
                "      add:",
                "        kind: initial",
                "        to: OPEN",
                "        description: Adds a part.",
                f"        only_via: [Bulk{i}.add]",
                "        required_inputs: [owner, label]",
                "      discard:",
                "        kind: external",
                "        from: [OPEN]",
                "        to: DISCARDED",
                "        description: Discards it."]
    return "\n".join(out) + "\n"


def source(parts):
    out = {"bulk": generated(parts)} if parts else {}
    for m, f in MODULES.items():
        out[m] = (EXAMPLES / f"{f}.yaml").read_text()
        examples = EXAMPLES / f"{f}.examples.yaml"
        if examples.exists():
            out[f"{m}.examples"] = examples.read_text()
    return out


def run(service, parts):
    work = pathlib.Path(tempfile.mkdtemp(prefix="dry-run-"))
    cmd = [str(service), "--store", str(work / "store.sqlite"), "--create", "--port", "0",
           "--dry-run-timeout", "60 min", "--read-timeout", "60 min"]
    proc = subprocess.Popen(cmd, stderr=subprocess.PIPE, text=True)
    # the service stopped whatever happens, so a refusal or a fault never leaves it running, holding the output it
    # inherited open
    try:
        said = []
        while not (m := re.search(r"(http://127\.0\.0\.1:\d+)", said[-1] if said else "")):
            line = proc.stderr.readline()
            if not line:
                raise SystemExit("the service stopped: " + "".join(said))
            said.append(line)
        threading.Thread(target=lambda: [None for _ in proc.stderr], daemon=True).start()
        url = m.group(1)
        drafted = post(url, "/v1/request", {"actor": OP, "type": "DeclarationChange", "transition": "draft",
                                            "inputs": {"source": source(parts), "source_ref": "profile:dry-run"}})
        assert drafted["kind"] == "satisfied", drafted
        began, before = time.monotonic(), cpu(proc.pid)
        submitted = post(url, "/v1/request", {"actor": OP, "object_id": drafted["object_id"],
                                              "transition": "submit"})
        wall, after = time.monotonic() - began, cpu(proc.pid)
        report = post(url, "/v1/publish", {"actor": OP, "change": drafted["object_id"], "dry_run": True})["report"]
        return wall, after[0] - before[0], after[1] - before[1], submitted["kind"], report["findings"]
    finally:
        proc.terminate()
        proc.wait()
        shutil.rmtree(work)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("service")
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--parts", type=int, default=0)
    a = ap.parse_args()
    load = pathlib.Path("/proc/loadavg").read_text().split()[0]
    print(f"{a.service}, generated types with parts: {a.parts}, load average at the start {load}")
    for i in range(a.runs):
        wall, user, system, kind, findings = run(a.service, a.parts)
        fatal = [f["message"] for f in findings if f["severity"] == "fatal"]
        print(f"run {i + 1}: submit {wall:.2f} s, CPU {user:.2f} s user {system:.2f} s system, {kind}, "
              f"{len(findings)} findings, {len(fatal)} fatal")
        for message in fatal:
            print(f"  {message}")


if __name__ == "__main__":
    main()

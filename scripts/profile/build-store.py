#!/usr/bin/env python3
"""Build a synthetic store at the production profile's sizes, for Gate P and Gate I.

ADR-0204 §1 names the stores a review tests against: the two-year store, about 36.7k objects, and the six-year
store, about 110k. The audit measured on stores it was given and did not keep (`docs/design/audit/cost.md`), so this
builds them from the shipped reimbursement flow (`docs/design/flow-format/examples/reimbursement.yaml`) through the
service, as a team's application would:

1. the service started on a new file with `--create`, and the flow published as its first version;
2. a company of `--employees` people, its business functions and its expense categories;
3. `--claims` claims, each drafted by its claimant with two lines, then taken as far through its life as a
   seeded draw says: most recorded in the accounts system, some approved, reviewed, submitted or still drafts, and a
   few returned and resubmitted, rejected or withdrawn.

Every value is made up here, by a seeded generator, so the store holds no one's personal data. Three claims an
object and its two lines make about 36.6k objects at `--claims 12000` and about 108.6k at `--claims 36000`.

Measurement condition: every event is recorded when the builder runs, since a deployment's clock is the wall clock,
so the stores have the profile's counts, not its spread over years.

Usage:

    build-store.py <objectflow-service> <out.sqlite | postgresql> [--claims 12000] [--employees 600] [--seed 7] [--writers 8]

A PostgreSQL store is built in an empty database the connection string or `postgresql://` URL names, its password in
`OBJECTFLOW_STORE_PASSWORD` (ADR-0213).
"""
import argparse
import hashlib
import json
import pathlib
import random
import re
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[2]
FLOW = ROOT / "docs/design/flow-format/examples/reimbursement.yaml"
OPERATOR = {"id": "operator"}


class Wire:
    def __init__(self, url):
        self.url = url

    def post(self, path, body):
        req = urllib.request.Request(self.url + path, json.dumps(body).encode(), {"content-type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=120) as r:
                return json.loads(r.read())
        except urllib.error.HTTPError as e:
            raise RuntimeError(f"{path} answered {e.code}: {e.read().decode()[:400]}") from None


def took(wire, body):
    """A request that must be satisfied, its object's id."""
    v = wire.post("/v1/request", body)
    if v.get("kind") != "satisfied":
        raise RuntimeError(f"{body['transition']} was answered {json.dumps(v)[:400]}")
    return v.get("object_id")


def publish(wire, text):
    change = took(wire, {"actor": OPERATOR, "type": "DeclarationChange", "transition": "draft",
                         "inputs": {"source": {"reimbursement": text}, "source_ref": "profile:reimbursement"}})
    submitted = wire.post("/v1/request", {"actor": OPERATOR, "object_id": change, "transition": "submit"})
    approved = wire.post("/v1/publish", {"actor": OPERATOR, "change": change,
                                         "expected_version": submitted["version"]})
    if approved["verdict"]["kind"] != "satisfied":
        raise SystemExit(f"the flow was not published: {json.dumps(approved)[:600]}")


def receipt(n):
    content = f"receipt {n}".encode()
    return {"hash": "sha256:" + hashlib.sha256(content).hexdigest(), "size": len(content),
            "media_type": "application/pdf"}


def claim_life(wire, rng, n, claimant, staff_id, categories, functions):
    """One claim drafted with two lines and taken as far as a draw says; its requests' count."""
    as_staff = {"id": staff_id}
    claim = took(wire, {"actor": as_staff, "type": "Claim", "transition": "draft",
                        "inputs": {"claimant": claimant, "title": f"Expenses {n}"}})
    for i in range(2):
        category, policy = rng.choice(categories)
        amount = round(rng.uniform(5, policy * 1.2), 2)
        took(wire, {"actor": as_staff, "object_id": claim, "transition": "add_line",
                    "inputs": {"amount": f"USD {amount:.2f}", "category": category,
                               "expense_date": time.strftime("%Y-%m-%d", time.gmtime(time.time() - 86_400 * (1 + i))),
                               "receipt": receipt(n * 2 + i), "merchant": f"Merchant {rng.randint(1, 300)}"}})
    count = 3
    stop = rng.random()
    if stop < 0.04:
        return count
    if stop < 0.07:
        took(wire, {"actor": as_staff, "object_id": claim, "transition": "withdraw"})
        return count + 1
    took(wire, {"actor": as_staff, "object_id": claim, "transition": "submit"})
    count += 1
    if stop < 0.12:
        return count
    lines = wire.post("/v1/get", {"actor": OPERATOR, "id": claim})["attributes"]["lines"]
    if stop < 0.15:
        took(wire, {"actor": OPERATOR, "object_id": claim, "transition": "reject",
                    "inputs": {"rejection_reason": "NOT_BUSINESS_SPEND"}})
        return count + 1
    if stop < 0.18:
        took(wire, {"actor": OPERATOR, "object_id": claim, "transition": "return_for_changes",
                    "inputs": {"return_reason": "MISSING_RECEIPT", "note": "The receipt for the second line."}})
        took(wire, {"actor": as_staff, "object_id": claim, "transition": "resubmit"})
        count += 2
    for line in lines:
        took(wire, {"actor": OPERATOR, "object_id": line, "transition": "confirm_nature",
                    "inputs": {"nature": "EXPENSE"}})
    took(wire, {"actor": OPERATOR, "object_id": claim, "transition": "complete_review"})
    count += len(lines) + 1
    if stop < 0.26:
        return count
    took(wire, {"actor": OPERATOR, "object_id": claim, "transition": "approve",
                "inputs": {"approving_function": rng.choice(functions)}})
    count += 1
    if stop < 0.36:
        return count
    took(wire, {"actor": OPERATOR, "object_id": claim, "transition": "record_in_xero",
                "inputs": {"xero_bill_id": f"BILL{n:07d}"}})
    return count + 1


def postgres(store):
    """Whether a store names a PostgreSQL database, a `postgresql://` URL or a connection string, which the service
    is given as it is, its password in OBJECTFLOW_STORE_PASSWORD (ADR-0213), where any other text is a file."""
    return store.startswith(("postgresql://", "postgres://")) or any(k in store for k in ("host=", "dbname="))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("service")
    ap.add_argument("out")
    ap.add_argument("--claims", type=int, default=12_000)
    ap.add_argument("--employees", type=int, default=600)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--writers", type=int, default=8)
    a = ap.parse_args()
    # a file made anew, or a PostgreSQL database holding no store, which --create makes one in
    if not postgres(a.out):
        out = pathlib.Path(a.out)
        for p in out.parent.glob(out.name + "*"):
            p.unlink()
    proc = subprocess.Popen([a.service, "--store", a.out, "--create", "--port", "0",
                             "--attempt-retention", "90 days"], stderr=subprocess.PIPE, text=True)
    said = []
    try:
        while not (m := re.search(r"(http://127\.0\.0\.1:\d+)", said[-1] if said else "")):
            line = proc.stderr.readline()
            if not line:
                raise SystemExit("the service stopped before it listened:\n" + "".join(said))
            said.append(line)
        wire = Wire(m.group(1))
        threading.Thread(target=lambda: [None for _ in proc.stderr], daemon=True).start()
        rng = random.Random(a.seed)
        text = FLOW.read_text()
        publish(wire, text)
        functions = [took(wire, {"actor": OPERATOR, "type": "BusinessFunction", "transition": "establish",
                                 "inputs": {"name": name}})
                     for name in ("Engineering", "Sales", "Operations", "Finance", "Support")]
        categories = []
        for name, policy in (("Travel", 800), ("Meals", 60), ("Lodging", 250), ("Software", 300), ("Supplies", 150),
                             ("Training", 900), ("Telephone", 80), ("Postage", 40)):
            categories.append((took(wire, {"actor": OPERATOR, "type": "ExpenseCategory", "transition": "define",
                                           "inputs": {"name": name, "policy_amount": f"USD {policy}.00"}}), policy))
        staff = []
        for i in range(a.employees):
            identity = f"staff{i:04d}@example.test"
            staff.append((took(wire, {"actor": OPERATOR, "type": "Employee", "transition": "onboard",
                                      "inputs": {"user_id": identity, "name": f"Staff Member {i:04d}"}}), identity))
        took(wire, {"actor": OPERATOR, "type": "AssistantAgent", "transition": "register",
                    "inputs": {"agent_id": "assistant", "name": "Expense assistant"}})
        began, done, requests, errors = time.monotonic(), [0], [0], []
        lock = threading.Lock()

        def writer(k):
            try:
                write(k)
            except Exception as e:  # each writer's fault kept, and the build failed for it
                with lock:
                    errors.append(f"writer {k}: {e}")

        def write(k):
            local = random.Random(a.seed * 1000 + k)
            for n in range(k, a.claims, a.writers):
                claimant, identity = staff[local.randrange(len(staff))]
                made = claim_life(wire, local, n, claimant, identity, categories, functions)
                with lock:
                    done[0] += 1
                    requests[0] += made
                    if done[0] % 500 == 0:
                        rate = requests[0] / (time.monotonic() - began)
                        print(f"{done[0]} of {a.claims} claims, {requests[0]} requests, {rate:.0f} a second",
                              flush=True)

        threads = [threading.Thread(target=writer, args=(k,)) for k in range(a.writers)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        if errors:
            raise SystemExit("the store was not built:\n" + "\n".join(errors))
        health = urllib.request.urlopen(wire.url + "/health", timeout=30).read().decode()
        print(f"built {a.out}: {a.claims} claims, {a.employees} employees, {requests[0]} requests in "
              f"{time.monotonic() - began:.0f} s; {health[:200]}")
    finally:
        proc.terminate()
        proc.wait()


if __name__ == "__main__":
    sys.exit(main())

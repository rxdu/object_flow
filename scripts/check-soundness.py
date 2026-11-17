#!/usr/bin/env python3
"""Hold the flow checker to the engine: what the checks accept, the core runs.

Each set of modules the checker accepts is loaded, checked, into a fresh sandbox
of the service: the design's modules, the format-1 corpus, and every planted
check that finds nothing fatal. Each type's objects are made by its creations,
each input given a value of its declared type; every transition each object
offers is requested with such values; each metric is read, and each object read
back. An answer `501 not_run`, or a fault `500`, names a construct the checks
passed that the engine does not run: a gap in the checks, which fails this check
unless KNOWN names it with its record in TODO.md. A refusal is the engine's to
give, and no finding here.

A value is generated, not chosen, so this finds a gap a value of the right type
reaches, and leaves to the planted checks a value only one input exposes.

Usage: check-soundness.py [--service <binary>] [--only <label substring>]
       The service defaults to the newer of the debug and release builds of
       objectflow-service under core/target, or under CARGO_TARGET_DIR.
"""
import http.client
import json
import os
import pathlib
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time

import yaml

ROOT = pathlib.Path(__file__).resolve().parents[1]
EXAMPLES = ROOT / "docs/design/flow-format/examples"
CORPUS = ROOT / "docs/design/flow-format/format-1"
PLANTS = ROOT / "core/crates/objectflow-core/tests/plants"
OP = {"id": "operator"}
ROUNDS = 2                  # transitions taken on each object, so a state one transition reaches is tried too
# a gap found and recorded: what its answer says, to the TODO.md entry recording it, kept while it answers so
KNOWN = {
    "501 not run: an amount in PHP": "a metric's amount in a currency the engine does not know passes check 24",
}
# each value is tried as itself and as its zero, which reaches a division and an empty text
FLAVOURS = ("one", "zero")


class Unix(http.client.HTTPConnection):
    def __init__(self, path):
        super().__init__("localhost", timeout=60)
        self.path_ = path

    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.settimeout(60)
        self.sock.connect(self.path_)


class Service:
    """The service on a socket of its own, a relative path, since an absolute one can pass AF_UNIX's 108 bytes."""

    def __init__(self, binary):
        self.dir = tempfile.mkdtemp(prefix="soundness-")
        self.log = pathlib.Path(self.dir, "service.log")
        self.proc = subprocess.Popen([str(binary), "--socket", "s.sock"], cwd=self.dir,
                                     stdout=self.log.open("w"), stderr=subprocess.STDOUT)
        sock = pathlib.Path(self.dir, "s.sock")
        for _ in range(200):
            if sock.exists():
                break
            time.sleep(0.05)
        self.sock = str(sock)

    def post(self, route, body):
        c = Unix(self.sock)
        try:
            c.request("POST", route, json.dumps(body), {"content-type": "application/json"})
            r = c.getresponse()
            raw = r.read()
            return r.status, json.loads(raw) if raw else None
        except (BrokenPipeError, ConnectionError) as e:
            # the service gone: its exit and its log's last lines say why, which the check reports as it fails
            code = self.proc.poll()
            tail = self.log.read_text(errors="replace").splitlines()[-15:]
            raise SystemExit(f"check-soundness: the service stopped answering {route} ({e}), its exit {code}:\n  "
                             + "\n  ".join(tail))
        finally:
            c.close()

    def stop(self):
        self.proc.terminate()
        self.proc.wait(timeout=30)
        shutil.rmtree(self.dir, ignore_errors=True)     # its socket and its log, kept no longer than its run


def module_sets():
    """Each set of modules to load, as (label, [(name, text)]), the files in import order and no examples file."""
    seen, out = set(), []

    def add(label, files):
        files = [(n, t) for n, t in files if not n.endswith(".examples.yaml")]
        key = tuple(t for _n, t in files)
        if files and key not in seen:
            seen.add(key)
            out.append((label, files))

    for folder in (EXAMPLES, CORPUS):
        texts = {p.name: p.read_text() for p in sorted(folder.glob("*.yaml")) if not p.name.endswith(".examples.yaml")}
        modules = {}
        for name, text in texts.items():
            doc = yaml.safe_load(text)
            if isinstance(doc, dict) and doc.get("module"):
                modules[doc["module"]] = (name, doc)
        for name, text in texts.items():
            doc = yaml.safe_load(text)
            if not isinstance(doc, dict) or not doc.get("module"):
                continue
            order, todo = [], [doc["module"]]
            while todo:                     # the modules it imports, each before its importer
                m = todo.pop()
                if m in order or m not in modules:
                    continue
                order.insert(0, m)
                todo += list((modules[m][1].get("imports") or {}).keys())
            add(f"{folder.name}/{name}", [(modules[m][0], texts[modules[m][0]]) for m in order])
    for case in json.loads((PLANTS / "cases.json").read_text()):
        if any(f[2] == "fatal" for f in case["findings"]):
            continue
        add(f"plant: {case['label']}", [(n, (PLANTS / "blobs" / f"{k}.yaml").read_text()) for n, k in case["files"]])
    return out


def generated(spec, enums, values, pool, flavour="one"):
    """A value of an attribute's or input's declared type, as the wire takes it; None where none can be made, a
    reference to a type no object of which is made yet."""
    spec = spec or {}
    if "reference" in spec:
        target = str(spec["reference"])
        many = target.endswith("[]")
        ids = pool.get(target.removesuffix("[]")) or []
        if not ids:
            return [] if many else None
        return [ids[0]] if many else ids[0]
    typ = str(spec.get("type", ""))
    many = typ.endswith("[]")
    one = typ.removesuffix("[]")
    v = scalar(one, enums, values, pool, flavour)
    if v is None:
        return None
    return [v] if many else v


def scalar(t, enums, values, pool, flavour="one"):
    zero = flavour == "zero"
    if t in ("string", "text"):
        return "" if zero else "x"
    if t == "identity":
        return "someone"
    if t == "actor_kind":
        return "human"
    if t in ("int", "counter"):
        return 0 if zero else 1
    if t == "bool":
        return not zero
    m = re.fullmatch(r"decimal\((\d+),\s*(\d+)\)", t)
    if m:
        return ("0" if zero else "1") + ("." + "0" * int(m.group(2)) if int(m.group(2)) else "")
    m = re.fullmatch(r"money\((\w+)\)", t)
    if m:
        return f"{m.group(1)} {0 if zero else 1}"
    if t == "date":
        return "2026-01-01"
    if t == "timestamp":
        return "2026-01-01T09:00:00Z"
    if t == "duration":
        return "1 h"
    if t == "file":
        return {"hash": "sha256:" + "0" * 64, "size": 1, "media_type": "application/pdf"}
    if t in enums:
        return enums[t][0]
    if t in values:
        out = {}
        for f, fspec in (values[t].get("attributes") or {}).items():
            v = generated(fspec, enums, values, pool, flavour)
            if v is None and not (fspec or {}).get("optional"):
                return None
            out[f] = v
        return out
    return None


def explore(svc, label, files):
    """The findings of one module set: each answer `501` or `500`, as (key, what was asked, the answer)."""
    st, loaded = svc.post("/sandbox/load", {"files": [{"name": n, "text": t} for n, t in files]})
    if st != 200 or not (loaded or {}).get("loaded"):
        return None                         # the checker refused it, which is the checker's to say
    found = []
    for flavour in FLAVOURS:
        svc.post("/sandbox/reset", {})
        found += explore_once(svc, f"{label} ({flavour})", files, loaded, flavour)
    return found


def explore_once(svc, label, files, loaded, flavour):
    # every evaluator answers, since one the sandbox has no answer for is not run by design
    evaluators = loaded.get("evaluators") or []
    if evaluators:
        svc.post("/sandbox/evaluators", {"answers": {f: "satisfied" for f in evaluators}})
    docs = [yaml.safe_load(t) or {} for _n, t in files]
    values = {k: v for d in docs for k, v in (d.get("values") or {}).items()}
    decls = {}
    for tn in loaded.get("types") or []:
        st, d = svc.post("/v1/declaration", {"type": tn})
        if st == 200 and isinstance(d, dict):
            decls[tn] = d
    enums = {k: v for d in decls.values() for k, v in (d.get("enumerations") or {}).items()}
    found, pool = [], {}
    gen = lambda spec: generated(spec, enums, values, pool, flavour)

    def ask(route, body, what):
        st, a = svc.post(route, body)
        if st in (500, 501):
            fault = a.get("fault", "") if isinstance(a, dict) else str(a)
            found.append((f"{label} | {what}", body, f"{st} {fault}"))
        return st, a

    def inputs_of(tn, names, declared):
        attrs = decls[tn].get("attributes") or {}
        out = {}
        for k in names:
            spec = declared.get(k) if k in declared else attrs.get(k)
            v = gen(spec)
            if v is not None:
                out[k] = v
        return out

    # creations, in passes, so a creation taking a reference finds an object made by an earlier one
    for _ in range(3):
        made = False
        for tn, d in decls.items():
            for xn, x in (d.get("transitions") or {}).items():
                if x.get("kind") != "initial" or pool.get(tn):
                    continue
                declared = x.get("inputs") or {}
                names = list(x.get("required_inputs") or []) + list(x.get("optional_inputs") or []) + list(declared)
                st, a = ask("/v1/request", {"actor": OP, "type": tn, "transition": xn,
                                            "inputs": inputs_of(tn, names, declared)}, f"{tn}.{xn}")
                if st == 200 and isinstance(a, dict) and a.get("object_id"):
                    pool.setdefault(tn, []).append(a["object_id"])
                    made = True
        if not made:
            break
    # each transition each object offers, its inputs given values of the types the offer names
    for _ in range(ROUNDS):
        for tn, ids in list(pool.items()):
            for oid in list(ids):
                st, offers = ask("/v1/availability", {"actor": OP, "id": oid}, f"{tn} offers")
                for o in offers if st == 200 and isinstance(offers, list) else []:
                    if o.get("creates"):
                        continue
                    # an input of a type the modules declare is a reference to one of its objects
                    given = {k: v for k, t in (o.get("inputs") or {}).items()
                             if (v := gen({"reference": t} if t.removesuffix("[]") in decls else {"type": t}))
                             is not None}
                    ask("/v1/request", {"actor": OP, "object_id": oid, "transition": o["transition"],
                                        "inputs": given}, f"{tn}.{o['transition']}")
    for tn, ids in pool.items():
        for oid in ids:
            ask("/v1/get", {"actor": OP, "id": oid}, f"{tn} read")
    for tn, d in decls.items():
        for mn in d.get("metrics") or {}:
            ask("/v1/metric", {"actor": OP, "name": f"{tn}.{mn}"}, f"{tn}.{mn} metric")
    return found


def service_binary(args):
    if args[:1] == ["--service"] and len(args) >= 2:
        return pathlib.Path(args[1]), args[2:]
    target = (ROOT / "core") / os.environ.get("CARGO_TARGET_DIR", "target")
    built = [p for p in (target / "debug/objectflow-service", target / "release/objectflow-service") if p.exists()]
    binary = max(built, key=lambda p: p.stat().st_mtime, default=None)
    if binary is None:
        sys.exit("check-soundness: build the service first (cargo build -p objectflow-service in core/)")
    return binary, args


def main():
    binary, args = service_binary(sys.argv[1:])
    only = args[1] if args[:1] == ["--only"] and len(args) == 2 else None
    sets = [s for s in module_sets() if only is None or only in s[0]]
    svc = Service(binary)
    findings, ran, refused = [], 0, 0
    try:
        for label, files in sets:
            got = explore(svc, label, files)
            if got is None:
                refused += 1
                continue
            ran += 1
            findings += got
    finally:
        svc.stop()
    known = lambda f: any(k in f[2] for k in KNOWN)
    new = [f for f in findings if not known(f)]
    for key, body, answer in findings:
        mark = "known" if known((key, body, answer)) else "GAP"
        print(f"  {mark}  {key}: {answer[:200]}")
        print(f"         {json.dumps(body)[:300]}")
    print(f"check-soundness: {ran} module set(s) run, {refused} refused by the checker; {len(findings)} answer(s) "
          f"501 or 500, {len(new)} not known; against {binary}")
    sys.exit(1 if new else 0)


if __name__ == "__main__":
    main()

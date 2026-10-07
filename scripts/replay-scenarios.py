#!/usr/bin/env python3
"""Replay docs/design/metric-scenarios.md's history against the Rust core, and compare what it quotes.

check-scenarios.py recomputes every value the scenarios quote from a transcription of the flow, and stays the
reference for those values. This holds the core to them (TODO, "the scenarios as the metric read's second oracle"):
it talks to the core's service over HTTP, as a caller would, and

  1. ports the store as the scenarios say it was ported (§1): the modules loaded with every type a mirror, the port's
     objects imported at the port's time by their legacy keys, and the journey published over them as the cutover
     (`publish-and-import.md` §7);
  2. replays the history in the document's order, each request with its actor, its occurred time and the outcome the
     history gives it, and each version published at its time through a DeclarationChange, drafted, submitted and
     approved (`publish-and-import.md` §1);
  3. reads each metric the document quotes, after the history above it, and compares the core's page with the table.

The ids are labels: each object the history names is the core's object its creation or its import made. The replay
stops at the first step the core cannot take as the history says, whether it does not run it or answers otherwise,
since every later value stands on it; each read after that is unreached. Two kinds of step change nothing a compared read
reads, and were stepped past while the core could not take them: a `check`, which is advice with no effect, and a
subscription, which feeds only `pull`; the core now takes both. A query is not run, since the core has no
`query`, and the other reads, of history, events and the rest, are not this driver's to compare.

It prints how far the replay reached and each read's outcome. It exits 1 where a read it reached disagrees with its
table, or a step the core takes answers otherwise than the history says, each a finding against the core or the
document; and where a read `scenarios_passing.txt` lists no longer matches, so a regression shows. A read newly
matching is printed, to be added to that list; one the list records as awaiting a decision, which TODO holds, has its
difference reported and is no finding.

With `--survey` it steps past each step it cannot take, recording why, so one run lists every gap the history meets;
a value read after a step it stepped past may stand on that step, so a survey's reads are reported and never held.

It builds the core's service with the pinned toolchain and starts it on a free port, stopping it when done.

With `--runner` it replays the same history through the reference runner's sandbox instead, served in this process by
the Flow Studio's server, so both backends are held to the values the document derives by hand (ADR-0166 decision 2).
An operation the runner's sandbox does not serve is not run there; its reads are held by `scenarios_passing_runner.txt`.
"""
import datetime
import importlib.util
import json
import pathlib
import re
import subprocess
import sys
import tempfile
import threading
import urllib.error
import urllib.request

import yaml

ROOT = pathlib.Path(__file__).resolve().parents[1]
CORE = ROOT / "core"
CARGO = pathlib.Path.home() / ".cargo/bin/cargo"
PASSING = pathlib.Path(__file__).with_name("scenarios_passing.txt")
spec = importlib.util.spec_from_file_location("scenarios", ROOT / "scripts/check-scenarios.py")
cs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cs)
spec = importlib.util.spec_from_file_location("flows", ROOT / "scripts/check-flows.py")
flows = importlib.util.module_from_spec(spec)
spec.loader.exec_module(flows)

OPERATOR = "operator"
ORDER = ("operations_shared", "inventory_journey", "returns")


class NotRun(Exception):
    """The core does not run what a step reaches."""


class Differs(Exception):
    """The core takes a step otherwise than the history says."""


# ── the wire ─────────────────────────────────────────────────────────────────────
class Service:
    """The core's service, built and started on a free loopback port."""

    def __init__(self):
        subprocess.run([str(CARGO), "build", "-q", "-p", "objectflow-service"], cwd=CORE, check=True)
        self.proc = subprocess.Popen([str(CORE / "target/debug/objectflow-service"), "--port", "0"],
                                     stderr=subprocess.PIPE, text=True)
        line = self.proc.stderr.readline()
        m = re.search(r"listening on (http://127\.0\.0\.1:\d+)", line)
        if not m:
            self.close()
            raise SystemExit(f"the core's service did not start: {line}")
        self.base = m.group(1)

    def close(self):
        self.proc.terminate()
        self.proc.wait()

    def call(self, path, body):
        req = urllib.request.Request(self.base + path, data=json.dumps(body).encode(), method="POST",
                                     headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req) as resp:
                return json.loads(resp.read())
        except urllib.error.HTTPError as e:
            fault = json.loads(e.read() or b"{}").get("fault", "")
            if e.code == 501:
                raise NotRun(fault.removeprefix("not run: "))     # the driver says it was not run
            raise Differs(f"{path} answered {e.code}: {fault}")


class RunnerSandbox(Service):
    """The reference runner's sandbox, served in this process by the Flow Studio's server, its wire the core's
    (ADR-0166 decision 2); an operation it does not serve is not run, never a difference."""

    def __init__(self):
        sys.path.insert(0, str(ROOT / "apps/flow-studio/server"))
        from flowstudio.catalogue import Catalogue
        from flowstudio.http import routes, serve
        from flowstudio.sandbox import Sandbox
        here = pathlib.Path(tempfile.mkdtemp())
        self.served = set(routes(Sandbox(), Catalogue(here)))
        self.box = Sandbox()
        self.server = serve("127.0.0.1", 0, here, box=self.box)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.base = f"http://127.0.0.1:{self.server.server_address[1]}"

    def close(self):
        self.server.shutdown()
        self.server.server_close()

    def call(self, path, body):
        if path not in self.served:
            raise NotRun(f"the runner's sandbox serves no {path}")
        if path == "/sandbox/load":
            # the port's mirrors loaded unchecked, as the core's service loads them, which the checker refuses
            # (TODO, the scenarios' port); the cutover's version is checked as every publish's is
            with self.box.lock:
                return self.box.load(body, check=False)
        return super().call(path, body)


# ── values ───────────────────────────────────────────────────────────────────────
TIME = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}Z$")
MINOR = {"SGD": 2, "USD": 2, "EUR": 2}


def stamp(t):
    """A time as the scenarios write it, `2026-09-01T02:00Z`, as the wire takes one."""
    return str(t)[:-1] + ":00Z"


UNITS = {"week": 604_800, "weeks": 604_800, "day": 86_400, "days": 86_400, "h": 3600, "min": 60, "s": 1}


def duration(text):
    """A duration as the wire writes one, a number in the largest unit it fills, `6 days` or `7630 min`, as a
    timedelta; none for text that is not one."""
    m = re.fullmatch(r"(-?\d+) (weeks?|days?|h|min|s)", text)
    return datetime.timedelta(seconds=int(m.group(1)) * UNITS[m.group(2)]) if m else None


def without_migration(text):
    """A module's text with no `migration`, as a version that does not introduce one carries it."""
    doc = yaml.safe_load(text)
    if "migration" not in (doc or {}):
        return text
    del doc["migration"]
    return yaml.safe_dump(doc, sort_keys=False, allow_unicode=True, width=10_000)


class Driver:
    def __init__(self, service, text, journey_text):
        self.core = service
        self.ids, self.aliases = {}, {}
        self.texts = cs.modules_of(journey_text)
        self.texts.update(cs.modules_of(text))
        publishes = [yaml.safe_load(b) for i, _s, b in cs.fences(text) if i == "yaml publish"]
        self.publishes = sorted((p for p in publishes if not p.get("change")), key=lambda p: cs.when(p["at"]))
        self.governed = {p["change"]: p for p in publishes if p.get("change")}
        self.changes, self.answers, self.checks, self.advice = {}, {}, {}, []
        self.versions = {}
        self.cited = {}
        self.now = None
        self.taken = 0
        self.last = 0                       # the last position a request's events reached

    # the ids: each alias the history names, the core's object
    def name(self, alias, oid):
        self.ids[alias], self.aliases[oid] = oid, alias

    def wire(self, v):
        """A value the history writes, as the wire takes it: an alias as the core's id, a time with its seconds."""
        if isinstance(v, list):
            return [self.wire(x) for x in v]
        if isinstance(v, dict):
            return {k: self.wire(x) for k, x in v.items()}
        if isinstance(v, float):
            return repr(v)                  # a decimal the wire takes as text, as YAML read it
        if isinstance(v, str):
            if v in self.ids:
                return self.ids[v]
            if TIME.match(v):
                return stamp(v)
        if isinstance(v, datetime.datetime):
            return v.strftime("%Y-%m-%dT%H:%M:%SZ")
        return v

    def typed(self, tn, xn, inputs):
        """The inputs with each amount of money written as the wire takes it, `SGD 420.00`, its currency the one its
        declaration names."""
        decl = self.core.call("/v1/declaration", {"type": tn})
        declared = ((decl.get("transitions") or {}).get(xn) or {}).get("inputs") or {}
        out = {}
        for k, v in inputs.items():
            spec_ = declared.get(k) or (decl.get("attributes") or {}).get(k) or {}
            m = re.fullmatch(r"money\((\w+)\)", str(spec_.get("type", "")))
            if m and isinstance(v, (int, float)):
                places = MINOR.get(m.group(1))
                if places is None:
                    raise NotRun(f"an amount in {m.group(1)}, whose minor unit this driver does not know")
                v = f"{m.group(1)} {v:.{places}f}"
            out[k] = v
        return out

    def actor(self, alias, principal=None):
        """An actor as the wire names it, with the principal it acts for, each the core's id where the history made
        it, and as named where it did not, as a request naming an unknown actor does."""
        out = {"id": self.ids.get(alias, alias)}
        if principal:
            out["principal"] = self.ids.get(principal, principal)
        return out

    def oid(self, alias):
        """The core's object an alias names; one no step the replay took made is a gap's consequence."""
        if alias not in self.ids:
            raise NotRun(f"{alias}, which no step the replay took made")
        return self.ids[alias]

    def clock(self, at):
        at = cs.when(at) if not isinstance(at, datetime.datetime) else at
        if self.now is not None and at < self.now:
            raise NotRun(f"the history moves the clock back, to {at:%Y-%m-%dT%H:%MZ}, which a sandbox's clock never does")
        self.now = at
        self.core.call("/sandbox/clock", {"set": at.strftime("%Y-%m-%dT%H:%M:%SZ")})

    # ── the port: the modules as mirrors, the objects imported, the journey cut over (§1) ─────────────
    def files(self, mirror=False):
        out = []
        for n in [n for n in ORDER if n in self.texts] + [n for n in self.texts if n not in ORDER]:
            text = self.texts[n]
            if mirror:
                # read as the checker and the core read it: `on` a key and not a truth value, a date the text it is
                doc = flows.load(text)
                for tn, t in list((doc.get("types") or {}).items()):
                    # in the place the format orders it, after the description (flow-format.md §4.1)
                    head = {k: v for k, v in t.items() if k == "description"}
                    doc["types"][tn] = {**head, "mirror": True, **{k: v for k, v in t.items() if k not in head}}
                text = yaml.safe_dump(doc, sort_keys=False, allow_unicode=True)
            out.append({"name": f"{n}.yaml", "text": text})
        return out

    def port(self, block):
        at = cs.when(block["port"])
        loaded = self.core.call("/sandbox/load", {"files": self.files(mirror=True)})
        if not loaded.get("loaded"):
            raise Differs(f"the modules as mirrors do not load: {loaded.get('findings')}")
        self.clock(at)
        self.imported(block.get("imported") or [])
        self.publish_text({n: t for n, t in self.texts.items()}, "the cutover")

    def imported(self, entries):
        objects = []
        for imp in entries:
            o = {"type": imp["type"], "legacy_key": imp["object"], "state": imp["state"],
                 "attributes": {k: (stamp(v) if isinstance(v, str) and TIME.match(v) else v)
                                for k, v in (imp.get("attributes") or {}).items()}}
            # a ported unit's serial, which the legacy record holds and the scenarios leave out, since no value they
            # quote depends on it, written so that no serial the store mints can equal it
            if imp["type"] == "Robot":
                o["attributes"].setdefault("serial", f"RBT-{imp['object']}")
            if imp.get("created"):
                o["created_at"] = stamp(imp["created"])
            if imp.get("created_by"):
                o["created_by_kind"] = imp["created_by"]
            entered = {}
            if imp.get("entered"):
                entered["state"] = {"at": stamp(imp["entered"]), "by_kind": imp.get("entered_by", "unknown")}
            for m, t in (imp.get("member_entered") or {}).items():
                entered[m] = {"at": stamp(t), "by_kind": (imp.get("member_entered_by") or {}).get(m, "unknown")}
            if entered:
                o["entered"] = entered
            legacy = [{"dimension": "state", "value": st, "entered_at": stamp(a), "left_at": stamp(b),
                       "entered_by_kind": imp.get("entered_by", "unknown")} for st, a, b in imp.get("legacy") or []]
            for m, spans in (imp.get("legacy_members") or {}).items():
                legacy += [{"dimension": m, "value": sp[0], "entered_at": stamp(sp[1]), "left_at": stamp(sp[2]),
                            "entered_by_kind": sp[3] if len(sp) > 3 else "unknown"} for sp in spans]
            if legacy:
                o["legacy_intervals"] = legacy
            if imp.get("silent"):
                o["silent_spans"] = [[stamp(imp["silent"][0]), stamp(imp["silent"][1])]]
            objects.append(o)
        report = self.core.call("/v1/import_batch", {"actor": {"id": OPERATOR}, "batch": {"objects": objects}})
        if report.get("refusal"):
            raise Differs(f"the port is refused: {report['refusal']}")
        for imp in entries:
            # a legacy key is kept under its type's family, its farthest base (ADR-0154)
            found = self.core.call("/v1/lookup", {"source": "legacy", "value": f"{self.root(imp['type'])}:{imp['object']}"})
            if not found:
                raise Differs(f"the port imported no object under the legacy key {imp['object']}")
            self.name(imp["object"], found["id"])

    def root(self, tn):
        types = {n: t for text in self.texts.values() for n, t in (yaml.safe_load(text).get("types") or {}).items()}
        seen = set()
        while (types.get(tn) or {}).get("extends") and tn not in seen:
            seen.add(tn)
            tn = types[tn]["extends"]
        return tn

    # ── publishing: a DeclarationChange drafted, submitted and approved (publish-and-import.md §1) ────
    def published(self, p):
        """The whole source of the version a publish gives, each module as check-scenarios.py patches it: a change's
        source is the version, and a module it leaves out is retired (declaration-syntax.md §6.6)."""
        mods = p.get("modules") or {"inventory_journey": {"add": p.get("add") or {}}}
        # a migration maps from the version before it, and so belongs to the version that introduced it: every module
        # of the next one starts without it (flow-format.md §4.16)
        out = {n: without_migration(t) for n, t in self.texts.items()}
        for name, change in mods.items():
            base = cs.module_text(change["text"]) if change.get("text") else out[name]
            out[name] = cs.patched(base, change.get("add") or {}, change.get("remove") or [])
        return out

    def draft(self, texts, actor=OPERATOR, evidence=None):
        inputs = {"source": texts, "source_ref": "metric-scenarios.md"}
        if evidence:
            # each reference the arguments of the read it names (ADR-0159), kept for the document's notation
            inputs["evidence"] = []
            for text in evidence:
                body, _keep = self.metric_body(text)
                ref = {"metric" if k == "name" else k: v for k, v in body.items() if k != "actor"}
                self.cited[json.dumps(ref, sort_keys=True)] = text
                inputs["evidence"].append(ref)
        v = self.core.call("/v1/request", {"type": "DeclarationChange", "transition": "draft", "actor": {"id": actor},
                                           "inputs": inputs})
        if v["kind"] != "satisfied":
            raise Differs(f"a change is not drafted: {v}")
        return v["object_id"]

    def submit(self, change, actor=OPERATOR):
        v = self.core.call("/v1/request", {"object_id": change, "transition": "submit", "actor": {"id": actor}})
        if v["kind"] != "satisfied":
            # the dry run's fatal findings, which refused it (ADR-0155)
            r = self.core.call("/v1/publish", {"actor": {"id": actor}, "change": change, "dry_run": True})
            fatal = [f"{f.get('code')}: {f.get('message', '')[:140]}" for f in (r.get("report") or {}).get("findings") or []
                     if f.get("severity") == "fatal"]
            raise Differs(f"the change is refused as {v.get('clause') or v['kind']}: {fatal[:3]}")
        return v["version"]

    def approve(self, change, version, texts, actor=OPERATOR):
        r = self.core.call("/v1/publish", {"actor": {"id": actor}, "change": change, "expected_version": version})
        if r.get("report"):
            # each version by the store's number, which the document's are since ADR-0161
            self.versions[r["report"]["version"]] = {"report": r["report"],
                                                     "changed": {n for n in texts if texts[n] != self.texts.get(n)}}
        if (r.get("verdict") or {}).get("kind") != "satisfied":
            findings = [(f.get("code"), f.get("message", "")[:120]) for f in ((r.get("report") or {}).get("findings") or [])
                        if f.get("severity") in ("fatal", "decision")]
            raise Differs(f"the publish is refused: {r.get('verdict')}; {findings[:4]}")
        self.texts.update(texts)

    def publish_text(self, texts, what):
        change = self.draft(texts)
        version = self.submit(change)
        try:
            self.approve(change, version, texts)
        except Differs as e:
            raise Differs(f"{what}: {e}")

    def publish_due(self, at):
        while self.publishes and cs.when(self.publishes[0]["at"]) <= at:
            p = self.publishes.pop(0)
            self.clock(p["at"])
            self.publish_text(self.published(p), f"the version published at {p['at']}")

    # ── the requests ────────────────────────────────────────────────────────────────────
    def request(self, r):
        if r.get("subscribe"):
            # a subscription, a request on the built-in type, its filter a type and transition names (DESIGN.md §7)
            f = r.get("filter") or {}
            inputs = {"target_type": f["type"]}
            if f.get("transitions"):
                inputs["transitions"] = f["transitions"]
            v = self.core.call("/v1/request", {"actor": self.actor(r["actor"]), "type": "Subscription",
                                               "transition": "subscribe", "inputs": inputs})
            if v["kind"] != "satisfied":
                raise Differs(f"the subscription is refused: {v}")
            self.name(r["subscribe"], v["object_id"])
            return
        if r.get("check"):
            # `check` is advice with no effect: the verdict the request would receive (library-api.md §6)
            self.checks[r["check"]] = self.core.call("/v1/check", self.body_of({k: v for k, v in r.items()
                                                                                 if k not in ("check", "creates")}))
            return
        if r.get("approve_proposal"):
            r = {**r, "object": r["approve_proposal"], "transition": "approve"}
        if r.get("forget"):
            # an observation's own erasure, its kind's generated `forget` (declaration-syntax.md §6.8)
            r = {**r, "object": r["forget"], "transition": "forget"}
        if r.get("draft"):
            self.changes[r["draft"]] = self.draft(self.published(self.governed[r["draft"]]), self.oid(r["actor"]),
                                                  r.get("evidence"))
            return
        if r.get("submit"):
            self.changes[r["submit"] + "@version"] = self.submit(self.changes[r["submit"]], self.oid(r["actor"]))
            return
        if r.get("approve"):
            p = self.governed[r["approve"]]
            self.approve(self.changes[r["approve"]], self.changes[r["approve"] + "@version"], self.published(p),
                         self.oid(r["actor"]))
            self.name(r["approve"], self.changes[r["approve"]])
            self.changes[r["approve"] + "@published"] = max(self.versions)
            return
        v = self.core.call("/v1/request", self.body_of(r))
        self.last = max([self.last] + list(v.get("events") or []))
        want = r.get("refused")
        if want:
            got = self.outcome(v)
            expect = {"verdict": want["verdict"], "clause": want.get("clause"), "remedy": want["remedy"],
                      "call": want.get("call")}
            if got != expect:
                raise Differs(f"it is answered {got}, and the history says {expect}")
            return
        if v["kind"] != "satisfied":
            raise Differs(f"it is answered {v}, and the history says it applies")
        if r.get("propose"):
            # a proposal filed, which the verdict names (ADR-0147)
            self.name(r["propose"], v["object_id"])
        elif r.get("record") or r.get("label") or r.get("create"):
            self.name(r["object"], v["object_id"])
        if r.get("answer"):
            self.answers[r["answer"]] = v
        made = list(r.get("creates") or [])
        if made:
            events = self.core.call("/v1/events", {"positions": v["events"]})["events"]
            new = [e["object_id"] for e in events if e and e["from_state"] is None and e["object_id"] != v["object_id"]]
            if len(new) != len(made):
                raise Differs(f"it creates {len(new)} object(s), and the history names {made}")
            for alias, oid in zip(made, new):
                self.name(alias, oid)

    def cause(self, position):
        """An event's cause as the history writes it, `D20.complete_sale`; absent where it has none."""
        if position is None:
            return "absent"
        e = self.core.call("/v1/events", {"positions": [position]})["events"][0]
        return f"{self.show(e['object_id'])}.{e['transition']}"

    def outcome(self, v):
        """A verdict as the history writes one: a call's refusal as the call's own verdict and the object it called."""
        if v["kind"] == "call_refused":
            inner = v.get("verdict") or {}
            return {"verdict": inner.get("kind"), "clause": (inner.get("clause") or "").split(".")[-1] or None,
                    "remedy": inner.get("remedy"), "call": self.aliases.get(v.get("object_id"), v.get("object_id"))}
        return {"verdict": v["kind"].replace("_", " ") if v["kind"] == "invalid_input" else v["kind"],
                "clause": v.get("clause"), "remedy": v.get("remedy"), "call": None}

    def body_of(self, r):
        """A request of the history as the wire carries it."""
        body = {"actor": self.actor(r["actor"], r.get("principal"))}
        if (r.get("refused") or {}).get("verdict") == "stale":
            # a request carrying an old version of its object, the one before the version it is at (ADR-0023)
            body["expected_version"] = self.core.call("/v1/get", {"id": self.oid(r["object"])})["version"] - 1
        if r.get("occurred_at"):
            body["occurred_at"] = stamp(r["occurred_at"])
        if r.get("propose"):
            body["propose"] = True
        if r.get("record"):
            subject = self.core.call("/v1/get", {"id": self.oid(r["subject"])})
            decl = self.core.call("/v1/declaration", {"type": subject["type"]})
            kind = ((decl.get("observations") or {}).get(r["record"]) or {}).get("kind")
            if kind is None:
                raise Differs(f"{subject['type']} declares no observation {r['record']}")
            inputs = {"subject": r["subject"], **(r.get("fields") or {})}
            if r.get("corrects"):
                inputs["corrects"] = r["corrects"]
            body.update(type=kind, transition="record", inputs=self.wire(inputs))
        elif r.get("label"):
            body.update(type="label", transition="record", inputs=self.wire({"subject": r["subject"], "name": r["label"]}))
        elif r.get("create"):
            body.update(type=r["create"], transition=r["transition"],
                        inputs=self.wire(self.typed(r["create"], r["transition"], r.get("inputs") or {})))
        else:
            tn = self.core.call("/v1/get", {"id": self.oid(r["object"])})["type"]
            body.update(object_id=self.oid(r["object"]), transition=r["transition"],
                        inputs=self.wire(self.typed(tn, r["transition"], r.get("inputs") or {})))
        return body

    def replay(self, block, survey=None):
        """The block's history; in a survey, each step it cannot take recorded in `survey` and stepped past."""
        if block.get("port"):
            self.port(block)
        # an import after the port, a refresh of a mirror, at its own time among the requests (publish-and-import.md §7)
        refreshes = [] if block.get("port") else sorted(block.get("imported") or [], key=lambda i: cs.when(i["at"]))
        for r in block.get("requests") or []:
            where = f"the request at {r['at']} on {r.get('object') or r.get('subject') or r.get('draft') or r.get('approve')}"
            try:
                while refreshes and cs.when(refreshes[0]["at"]) <= cs.when(r["at"]):
                    imp = refreshes.pop(0)
                    self.publish_due(cs.when(imp["at"]))
                    self.clock(imp["at"])
                    self.imported([imp])
                self.publish_due(cs.when(r["at"]))
                self.clock(r["at"])
                self.request(r)
            except (NotRun, Differs) as e:
                gap = type(e)(f"{where}: {e}")
                if survey is None:
                    raise gap
                survey.append(f"{'not run: ' if isinstance(e, NotRun) else ''}{gap}")
                continue
            self.taken += 1
        self.publish_due(cs.when(block["now"]))
        self.clock(block["now"])

    # ── a metric read, as its table writes it ──────────────────────────────────────────────
    def show(self, v):
        if v is None:
            return "absent"
        if isinstance(v, str):
            if v in self.aliases:
                return self.aliases[v]
            if re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z", v):
                return v[:-4] + "Z"
            d = duration(v)
            if d is not None:
                return cs.show(d)
            return v
        if isinstance(v, bool):
            return str(v).lower()
        if isinstance(v, list):
            return ", ".join(sorted(self.show(x) for x in v))
        return str(v)

    def exported(self, source):
        """Every row of an export source, page after page, until a page returns none."""
        rows, cursor = [], None
        while True:
            page = self.core.call("/v1/export", {"actor": {"id": OPERATOR}, "source": source, "cursor": cursor})
            if page.get("refused"):
                raise NotRun(f"export {source}: {page['refused']}")
            if not page["lines"]:
                return rows
            rows += [json.loads(line) for line in page["lines"]]
            cursor = page["cursor"]

    def other(self, spec_):
        """A read the core answers through its operations, as its table writes it: its rows, its columns, and the
        columns it quotes that no operation returns, which are named and left out."""
        m = re.fullmatch(r"check ([\w-]+)", spec_)
        if m:
            c = self.checks[m.group(1)]
            o = self.outcome(c["verdict"])
            return [tuple(self.show(o[k]) for k in ("verdict", "clause", "remedy", "call"))], \
                ["verdict", "clause", "remedy", "call"], []
        m = re.fullmatch(r"answer ([\w-]+)", spec_)
        if m:
            v = self.answers[m.group(1)]
            flags = v.get("flags") or [{"clause": None, "remedy": None}]
            return [(self.show(v["kind"]), self.show(f["clause"]), self.show(f["remedy"])) for f in flags], \
                ["verdict", "flag", "remedy"], []
        m = re.fullmatch(r"(events|history|provenance|payloads)\(([\w-]+)\)", spec_)
        if m:
            # an object's own events, in the order recorded (library-api.md §6)
            evs = self.core.call("/v1/history", {"id": self.oid(m.group(2))})["events"]
            if m.group(1) == "events":
                return [(self.show(e["transition"]), self.show(e["occurred_at"]), self.show(e["recorded_at"]))
                        for e in evs], ["transition", "occurred_at", "recorded_at"], []
            if m.group(1) == "history":
                # an event's cause as the history writes it, the causing event's object and transition
                return [(self.show(e["transition"]), self.show(e["occurred_at"]), self.show(e["actor_id"]),
                         self.show(e["actor_kind"]), self.cause(e["cause"])) for e in evs], \
                    ["transition", "occurred_at", "actor", "actor_kind", "cause"], []
            if m.group(1) == "provenance":
                return [(self.show(e["transition"]), self.show(e["actor_id"]), self.show(e["actor_kind"]),
                         self.show(e["principal"])) for e in evs], ["transition", "actor", "actor_kind", "principal"], []
            return [(self.show(e["transition"]), a, self.show(v)) for e in evs
                    for a, v in sorted(((e.get("payload") or {}).get("inputs") or {}).items())], \
                ["transition", "attribute", "value"], []
        m = re.fullmatch(r"notices of version (\d+)", spec_)
        if m:
            # the checker's notes on the modules the version changed, by the store's version number
            v = self.versions[int(m.group(1))]
            rows = []
            for f in v["report"]["findings"]:
                n = re.fullmatch(r"(\w+):\d+: (.*)", f["message"], re.S)
                if f["severity"] == "notice" and f["code"].startswith("check:") and n and n.group(1) in v["changed"]:
                    rows.append((n.group(1), f["code"][len("check:"):], n.group(2)))
            return rows, ["module", "code", "notice"], []
        m = re.fullmatch(r"evidence of ([\w-]+)", spec_)
        if m:
            # what each reference read at submission, as the change keeps it (ADR-0097 §5, ADR-0159)
            o = self.core.call("/v1/get", {"id": self.changes[m.group(1)]})
            evidence = json.loads(o["attributes"].get("evidence") or "{}")
            rows = []
            for read in evidence.get("read") or []:
                if read.get("refused"):
                    raise Differs(f"a cited read is refused as {read['refused']}")
                ref = read["reference"]
                text = self.cited.get(json.dumps(ref, sort_keys=True), json.dumps(ref))
                for r in read["rows"]:
                    group = ", ".join(f"{k} = {self.show(r['dimensions'].get(k))}" for k in ref.get("keep") or [])
                    rows.append((text, group, self.show(r["value"])))
            return rows, ["read", "group", "value"], []
        m = re.fullmatch(r"(change|impact of) ([\w-]+)", spec_)
        if m:
            version = self.changes[m.group(2) + "@published"]
            report = self.versions[version]["report"]
            if m.group(1) == "impact of":
                # each object a guard the change newly enforces would refuse (publish-and-import.md §1)
                rows = []
                for f in report["findings"]:
                    k = re.fullmatch(r"newly_enforced:\w+\.(\w+)\.(\w+)", f["code"])
                    if k:
                        rows += [(self.show(o), k.group(1), k.group(2)) for o in f["sample"]]
                return rows, ["object", "transition", "clause"], []
            evs = self.core.call("/v1/history", {"id": self.oid(m.group(2))})["events"]
            drafted = evs[0] if evs else {}
            published = next((e for e in evs if e["transition"] == "publish"), {})
            return [(self.show(drafted.get("actor_id")), self.show(drafted.get("actor_kind")),
                     self.show(published.get("actor_id")), self.show(published.get("actor_kind")), str(version))], \
                ["drafted_by", "drafted_kind", "approved_by", "approved_kind", "version"], []
        m = re.fullmatch(r"pull\(([\w-]+)\)", spec_)
        if m:
            # the events its filter selects after its acknowledged cursor (DESIGN.md §7)
            page = self.core.call("/v1/pull", {"actor": {"id": OPERATOR}, "subscription": self.oid(m.group(1))})
            if page.get("refused"):
                raise Differs(f"the pull is refused as {page['refused']}")
            return [(self.show(e["object_id"]), self.show(e["transition"]), self.show(e["occurred_at"]))
                    for e in page["events"]], ["object", "transition", "occurred_at"], []
        m = re.fullmatch(r"rules\((\w+)\.(\w+)\)", spec_)
        if m:
            # the installed declaration's guards on the transition, in the order evaluated, each with its condition's
            # remedy and whether it refuses; how a renderer marks one it does not enforce is the renderer's
            d = self.core.call("/v1/declaration", {"type": m.group(1)})
            guards = ((d.get("transitions") or {}).get(m.group(2)) or {}).get("guards") or {}
            return [(g, self.show((d.get("conditions") or {}).get(g, {}).get("remedy")), "yes" if mode == "deny" else "no")
                    for g, mode in guards.items()], ["clause", "remedy", "enforced"], ["marked"]
        m = re.fullmatch(r"exceptions\((\w+)\)|available\((\w+), (\w+)\)", spec_)
        if m:
            # a page of objects (DESIGN.md §10): exceptions with each one's state; available asking no evaluator
            if m.group(1):
                page = self.core.call("/v1/exceptions", {"actor": {"id": OPERATOR}, "type": m.group(1)})
            else:
                page = self.core.call("/v1/available", {"actor": {"id": OPERATOR}, "type": m.group(2),
                                                        "transition": m.group(3)})
            if page.get("refused"):
                raise NotRun(f"{spec_}: {page['refused']}")
            if m.group(1):
                return [(self.show(i["id"]), self.show(i["fields"]["state"])) for i in page["items"]], \
                    ["object", "state"], []
            return [(self.show(i["id"]),) for i in page["items"]], ["object"], []
        m = re.fullmatch(r"intervals\(([\w-]+)\.(\w+)\)", spec_)
        if m:
            # the type's interval export, a consumer keeping the latest row per object, dimension and entry (ADR-0100)
            oid = self.oid(m.group(1))
            latest = {}
            for line in self.exported(self.core.call("/v1/get", {"id": oid})["type"] + ".intervals"):
                if line["object_id"] == oid and line["dimension"] == m.group(2):
                    latest[line["entered_at"]] = line
            return [(self.show(r.get("value")), self.show(r["entered_at"]), self.show(r["entered_by_kind"]),
                     self.show(r["legacy"])) for _k, r in sorted(latest.items())], \
                ["value", "entered_at", "entered_by_kind", "legacy"], []
        m = re.fullmatch(r"(attempts\(([\w-]+)\)|refusal of ([\w-]+)\.(\w+))", spec_)
        if m:
            # the type's attempt export, in the order written (ADR-0094, ADR-0100)
            oid = self.oid(m.group(2) or m.group(3))
            rows = [a for a in self.exported(self.core.call("/v1/get", {"id": oid})["type"] + ".attempts")
                    if a["object_id"] == oid]
            if m.group(3):
                a = [a for a in rows if a["transition"] == m.group(4)][-1]
                consulted = ", ".join(f"{re.sub(r'(?<=: )([^,)]+)', lambda k: self.show(k.group(1)), ref)} = {self.show(v)}"
                                      for g in (a.get("consulted") or {}).values() for ref, v in sorted(g["values"].items()))
                # the threshold is a value the clause read, and a read set holds identities, not values
                return [(self.show(a["clause"]), self.show(a["remedy"]), consulted)], ["clause", "remedy", "consulted"], \
                    ["threshold"]
            # a verdict as the history writes it: a refused call's own, with the call's target (ADR-0159)
            return [(self.show(a["transition"]), self.show(a["actor_kind"]),
                     ((a.get("call") or {}).get("verdict") or a["verdict"]).replace("_", " "),
                     self.show(a["clause"]), self.show(a["remedy"]), self.show((a.get("call") or {}).get("object")))
                    for a in rows], ["transition", "actor_kind", "verdict", "clause", "remedy", "call"], []
        m = re.fullmatch(r"(read set|consulted) of ([\w-]+)\.(\w+)(?:: (.+))?", spec_)
        if m:
            # what the rules of the object's last event of the transition read and consulted (ADR-0088, ADR-0158)
            evs = self.core.call("/v1/history", {"id": self.oid(m.group(2))})["events"]
            e = [e for e in evs if e["transition"] == m.group(3)][-1]
            if m.group(1) == "read set":
                kinds = {k.strip() for k in (m.group(4) or "").split(",")}
                rows = []
                for oid in (e.get("reads") or {}).get("objects", {}):
                    t = self.core.call("/v1/get", {"id": oid})["type"]
                    if t in kinds:
                        rows.append((self.show(oid), t))
                return rows, ["object", "kind"], []
            # a reference binds an object by its id, which the document writes by its alias
            return [(re.sub(r"(?<=: )([^,)]+)", lambda k: self.show(k.group(1)), ref), self.show(v))
                    for g in ((e.get("payload") or {}).get("consulted") or {}).values()
                    for ref, v in sorted(g["values"].items())], ["reference", "value"], []
        m = re.fullmatch(r"caused by\(([\w-]+)\.(\w+)\)", spec_)
        if m:
            # every event the named one caused (DESIGN.md §6 step 8)
            evs = self.core.call("/v1/history", {"id": self.oid(m.group(1))})["events"]
            mine = [e["position"] for e in evs if e["transition"] == m.group(2)]
            every = self.core.call("/v1/events", {"positions": list(range(1, self.last + 1))})["events"]
            return [(self.show(e["object_id"]), self.show(e["transition"])) for e in every
                    if e and mine and e["cause"] == mine[-1]], ["object", "transition"], []
        m = re.fullmatch(r"get\(([\w-]+)\)", spec_)
        if m:
            # its stored attributes, an erased one absent; a derived value is no attribute it stores
            o = self.core.call("/v1/get", {"id": self.oid(m.group(1))})
            decl = self.core.call("/v1/declaration", {"type": o["type"]})
            stored = decl.get("attributes") or {}
            return [(a, self.show(v)) for a, v in sorted(o["attributes"].items()) if a in stored], \
                ["attribute", "value"], []
        m = re.fullmatch(r"(results|recorded|labels)\(([\w-]+)(?:\.(\w+))?\)", spec_)
        if m:
            what, alias, collection = m.groups()
            subject = self.oid(alias)
            if what == "labels":
                kind, fields, by = "label", ["name", "recorded_by_kind", "subject_state"], None
            else:
                kind_of = self.core.call("/v1/get", {"id": subject})["type"]
                decl = self.core.call("/v1/declaration", {"type": kind_of})
                kind = (decl.get("observations") or {})[collection or "pdi_results"]["kind"]
                fields = ["unit", "check", "outcome", "remark"] if what == "results" else \
                    ["occurred_at", "min_health", "samples"]
                by = "recorded_at" if what == "results" else "occurred_at"
            asked = fields + ["corrects"] + ([by] if by and by not in fields else [])
            page = self.core.call("/v1/query", {"actor": {"id": OPERATOR}, "type": kind,
                                                "filter": f'subject.id == "{subject}"', "fields": asked})
            if page.get("refused"):
                raise Differs(f"the query of its {kind} is refused as {page['refused']}")
            # the observations nothing has corrected, which every read of a kind sees (declaration-syntax.md §6.8)
            corrected = {i["fields"].get("corrects") for i in page["items"]}
            items = [i for i in page["items"] if i["id"] not in corrected]
            if by:
                items.sort(key=lambda i: i["fields"].get(by) or "")
            return [tuple([self.show(i["id"])] + [self.show(i["fields"].get(f)) for f in fields]) for i in items], \
                ["object"] + fields, []
        m = re.fullmatch(r"proposal ([\w-]+)", spec_)
        if m:
            # who filed it and who approved it, from its own events, and its state (DESIGN.md §9)
            pid = self.oid(m.group(1))
            p = self.core.call("/v1/get", {"id": pid})
            evs = self.core.call("/v1/history", {"id": pid})["events"]
            filed = evs[0] if evs else {}
            approved = next((e for e in evs if e["transition"] == "approve"), {})
            return [(self.show(filed.get("actor_id")), self.show(filed.get("actor_kind")),
                     self.show(approved.get("actor_id")), self.show(approved.get("actor_kind")),
                     self.show(p["state"].lower()))], \
                ["proposed_by", "proposed_kind", "approved_by", "approved_kind", "state"], []
        raise NotRun(f"a read the driver does not compare: {spec_}")

    def query(self, spec_, header):
        """A query as its table writes it: each object by its label, with the fields the table names, which a page
        returns as asked (ADR-0161)."""
        m = re.fullmatch(r'query\((\w+)(?:, filter: "(.+?)")?(?:, order: "(.+?)")?\)', spec_)
        if not m:
            raise NotRun(f"a read the driver does not parse: {spec_}")
        fields = [c for c in header if c != "object"]
        body = {"actor": {"id": OPERATOR}, "type": m.group(1), "fields": fields}
        if m.group(2):
            body["filter"] = self.labelled(m.group(2))
        if m.group(3):
            body["order"] = [k.strip() for k in m.group(3).split(",")]
        page = self.core.call("/v1/query", body)
        if page.get("refused"):
            raise Differs(f"the query is refused as {page['refused']}")
        rows = [tuple([self.show(i["id"])] + [self.show(i["fields"].get(f)) for f in fields]) for i in page["items"]]
        return rows, ["object"] + fields, []

    def labelled(self, text):
        """A filter's text with each object it names by its label compared by id, as the language names one
        (flow-format.md §5)."""
        return re.sub(r"([\w.]+) == ([A-Z][\w-]*)",
                      lambda g: f'{g.group(1)}.id == "{self.ids[g.group(2)]}"' if g.group(2) in self.ids else g.group(0),
                      text)

    def metric(self, spec_):
        body, keep = self.metric_body(spec_)
        page = self.core.call("/v1/metric", body)
        if page.get("refused"):
            raise Differs(f"the read is refused as {page['refused']}")
        rows = [tuple([self.show(r["dimensions"].get(k)) for k in keep] + [self.show(r["value"]), self.show(r["gaps"])])
                for r in page["rows"]]
        return rows, keep + ["value", "gaps"]

    def metric_body(self, spec_):
        """A metric read as the document writes it, as the `metric` operation's arguments, and the dimensions kept."""
        reader = re.search(r"\) as ([\w-]+)$", spec_)
        if reader:
            spec_ = spec_[:reader.start() + 1]
        m = re.fullmatch(r'metric\(([\w.]+)(?:, bind: \{(\w+): ([\w-]+)\})?, keep: \[([\w, ]*)\]'
                         r'(?:, filter: "(.+?)")?(?:, over: "(\d+ days)")?\)', spec_)
        if not m:
            raise NotRun(f"a read the driver does not parse: {spec_}")
        keep = [k.strip() for k in m.group(4).split(",") if k.strip()]
        body = {"actor": {"id": self.ids.get(reader.group(1), OPERATOR) if reader else OPERATOR}, "name": m.group(1),
                "keep": keep}
        if m.group(2):
            body["bind"] = {m.group(2): self.wire(m.group(3))}
        if m.group(5):
            body["filter"] = self.labelled(m.group(5).replace('\\"', '"'))
        if m.group(6):
            body["over"] = m.group(6)
        return body, keep


def main():
    text, journey = cs.DOC.read_text(), cs.JOURNEY.read_text()
    histories = [(start, yaml.safe_load(b)) for i, start, b in cs.fences(text) if i == "yaml scenario"]
    reads = cs.expected_tables(text)
    steps = sorted([(start, "history", h) for start, h in histories]
                   + [(line, "read", (line, sp, hd, rw)) for line, sp, hd, rw in reads], key=lambda x: x[0])
    total = sum(len(h.get("requests") or []) for _s, h in histories)
    survey = [] if "--survey" in sys.argv[1:] else None
    runner = "--runner" in sys.argv[1:]
    service = RunnerSandbox() if runner else Service()
    who = "the runner" if runner else "the core"
    passing = PASSING.with_name("scenarios_passing_runner.txt") if runner else PASSING
    findings, matched, outcomes, unrun, waiting, partial, stopped = [], set(), [], [], [], [], None
    awaiting = {}
    for line in (passing.read_text().splitlines() if passing.exists() else []):
        if line.startswith("awaiting: "):
            spec_, _, why = line[len("awaiting: "):].partition(" -- ")
            awaiting[spec_.strip()] = why.strip()
    seen = {}
    try:
        d = Driver(service, text, journey)
        for line, what, item in steps:
            if what == "history":
                if stopped:
                    continue
                try:
                    d.replay(item, survey)
                except NotRun as e:
                    stopped = f"line {line}: not run: {e}"
                except Differs as e:
                    stopped = f"line {line}: {e}"
                    findings.append(stopped)
                continue
            line, spec_, header, rows = item
            # a read named by its text, and by its place among reads of the same text
            seen[spec_] = seen.get(spec_, 0) + 1
            key = spec_ if seen[spec_] == 1 else f"{spec_} #{seen[spec_]}"
            if not spec_.startswith(("metric(", "query(", "check ", "answer ", "events(", "history(", "provenance(",
                                     "payloads(", "get(", "results(", "recorded(", "labels(", "proposal ",
                                     "caused by(", "pull(", "notices of", "change ", "impact of",
                                     "read set of", "consulted of", "intervals(", "attempts(", "refusal of",
                                     "exceptions(", "available(", "rules(", "evidence of")):
                outcomes.append("not compared")
                continue
            if stopped:
                outcomes.append("unreached")
                continue
            if survey:
                # a read after a step stepped past is reported, and never held
                try:
                    got, cols = d.metric(spec_)
                    same = sorted([tuple(r) for r in rows]) == sorted(got) and header == cols
                    outcomes.append("after a gap, " + ("matches" if same else "differs"))
                    if not same:
                        survey.append(f"read at line {line}, `{spec_}`, after a gap, expects\n      "
                                      f"{sorted(tuple(r) for r in rows)}\n      and {who} gives\n      {sorted(got)}")
                except (NotRun, Differs) as e:
                    outcomes.append(f"after a gap, {'not run' if isinstance(e, NotRun) else 'refused'}")
                    survey.append(f"read at line {line}, `{spec_}`, after a gap: {e}")
                continue
            beyond = []
            try:
                if spec_.startswith(("metric(",)):
                    got, cols = d.metric(spec_)
                else:
                    got, cols, beyond = d.query(spec_, header) if spec_.startswith("query(") else d.other(spec_)
                    # the columns no operation returns are left out of the comparison, and named
                    keep = [header.index(c) for c in cols if c in header]
                    rows = [[r[i] for i in keep] for r in rows]
                    header = [header[i] for i in keep]
            except NotRun as e:
                outcomes.append(f"not run: {e}")
                unrun.append(f"line {line}: `{spec_}`: {e}")
                continue
            except Differs as e:
                outcomes.append("differs")
                findings.append(f"line {line}: `{spec_}`: {e}")
                continue
            want = [tuple(r) for r in rows]
            # a query's page is ordered where it names an order; otherwise it ascends by id, which the document's
            # labels do not show
            ordered = "order:" in spec_ or spec_.startswith(cs.ORDERED)
            if header != cols:
                outcomes.append("differs")
                findings.append(f"line {line}: `{spec_}` has columns {header}, and the read returns {cols}")
            elif (want != got) if ordered else (sorted(want) != sorted(got)):
                outcomes.append("differs")
                text = (f"line {line}: `{spec_}` expects\n      {sorted(want)}\n    and {who} gives\n      "
                        f"{sorted(got)}")
                # a difference recorded as awaiting a decision is reported, and is no finding
                (waiting if key in awaiting else findings).append(text + (f"\n    awaiting: {awaiting[key]}"
                                                                          if key in awaiting else ""))
            elif beyond:
                outcomes.append("matches, but for columns no operation returns")
                partial.append(f"line {line}: `{spec_}`: {', '.join(beyond)}")
            else:
                outcomes.append("matches")
                matched.add(key)
    finally:
        service.close()
    tally = {}
    for o in outcomes:
        key = o.split(":")[0]
        tally[key] = tally.get(key, 0) + 1
    print(f"the replay took {d.taken} of the history's {total} requests"
          + (f", and stopped at {stopped}" if stopped else ", the whole history"))
    for o, n in sorted(tally.items()):
        print(f"  {n:3d} read(s) {o}")
    if survey is not None:
        print(f"  the survey stepped past {len(survey)} step(s):")
        for gap in survey:
            print(f"    {gap}")
        return 0
    for a in d.advice:
        print(f"  stepped past, changing nothing a compared read reads: {a}")
    for u in unrun:
        print(f"  not run: {u}")
    for p in partial:
        print(f"  matches on what {who} returns, and quotes what it does not: {p}")
    for w in waiting:
        print(f"  awaiting a decision: {w}")
    for f in findings:
        print(f"  finding: {f}")
    for spec_ in sorted(set(awaiting) & matched):
        print(f"  now matching, to move from awaiting to held: {spec_}")
    held = {line.strip() for line in passing.read_text().splitlines()
            if line.strip() and not line.startswith(("#", "awaiting: "))} if passing.exists() else set()
    lost = sorted(held - matched)
    for spec_ in lost:
        print(f"  no longer matches: {spec_}")
    for spec_ in sorted(matched - held - set(awaiting)):
        print(f"  newly matching, to add to {passing.name}: {spec_}")
    return 1 if findings or lost else 0


if __name__ == "__main__":
    sys.exit(main())

"""An examples file played through the studio's wire, as a caller would.

It talks HTTP only: every step is a library operation or a sandbox control,
and every expectation is checked against what `request`, `get` and `history`
report (`docs/design/flow-studio.md` §6, slice 1). Its outcomes are the flow
checker's, `passed`, `failed` and `not run`, and one more: `waiting`, for an
example that needs what the interface does not yet offer (ADR-0150,
Consequences).
"""

from __future__ import annotations

import json
import re
import urllib.error
import urllib.request
from fractions import Fraction
from typing import Any

from flowstudio.scripts import flowrun, flows
from flowstudio.values import timestamp_text

OPERATOR = flowrun.OPERATOR
VERBS = ("request", "propose", "approve", "reject", "withdraw")
BUILT_IN = {"id": "text", "open": "bool", "created_at": "timestamp",
            "created_by_kind": "text", "recorded_from": "timestamp",
            "declaration_version": "int"}


class Waiting(Exception):
    """The example needs what the interface does not yet offer."""


class NotRun(Exception):
    """The backend cannot run a construct the example reaches."""


class Failed(Exception):
    """The example does not do what it claims."""


class Wire:
    def __init__(self, base: str):
        self.base = base

    def call(self, method: str, path: str, body: Any = None) -> Any:
        data = json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(
            self.base + path, data=data, method=method,
            headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req) as resp:
                return json.loads(resp.read())
        except urllib.error.HTTPError as e:
            fault = json.loads(e.read() or b"{}").get("fault", "")
            if e.code == 501:
                raise NotRun(fault)
            raise Failed(f"the wire answered {e.code}: {fault}")

    def post(self, path: str, body: dict) -> Any:
        return self.call("POST", path, body)

    def get(self, path: str) -> Any:
        return self.call("GET", path)


def seconds_of(ts: str) -> Fraction:
    return flowrun.timestamp(ts)


class Player:
    """One example's run: the clock it knows, the aliases it has bound."""

    def __init__(self, wire: Wire, now: str):
        self.wire, self.now = wire, seconds_of(now)
        self.aliases: dict[str, str] = {OPERATOR: OPERATOR}
        self.types: dict[str, str] = {}
        self.decls: dict[str, dict] = {}

    # ── reading ─────────────────────────────────────────────────────────
    def decl(self, tn: str) -> dict:
        if tn not in self.decls:
            self.decls[tn] = self.wire.post("/v1/declaration", {"type": tn})
        return self.decls[tn]

    def type_of(self, oid: str) -> str:
        if oid not in self.types:
            obj = self.wire.post("/v1/get", {"id": oid})
            if obj is None:
                raise Failed(f"no object {oid}")
            self.types[oid] = obj["type"]
        return self.types[oid]

    def alias(self, a: str) -> str:
        if a not in self.aliases:
            raise Failed(f"no object is named {a}")
        return self.aliases[a]

    # ── writing an example's value as the wire carries it ────────────────
    def instant(self, v: str) -> str:
        m = re.fullmatch(r"now(?: ([+-]) ([0-9]+) (\w+))?", v)
        if not m:
            return timestamp_text(seconds_of(v))
        shift = (Fraction(int(m.group(2))) * flowrun.SECONDS[m.group(3)]
                 if m.group(1) else 0)
        return timestamp_text(self.now + (shift if m.group(1) != "-"
                                          else -shift))

    def wire_value(self, v: Any, spec: dict | None) -> Any:
        spec = spec or {}
        if v is None:
            return None
        if "reference" in spec:
            if spec["reference"].endswith("[]"):
                return [self.alias(a) for a in v]
            return self.alias(v)
        typ = str(spec.get("type", ""))
        if typ.endswith("[]"):
            return [self.wire_value(x, dict(spec, type=typ[:-2])) for x in v]
        if typ == "timestamp":
            return self.instant(v)
        if isinstance(v, str) and "." in v and typ and typ[0].isupper():
            return v.split(".", 1)[1]           # an enumeration member
        return v

    # ── a step ────────────────────────────────────────────────────────────
    def play(self, st: dict) -> dict | None:
        if "import" in st:
            raise Waiting("an import step, which reaches the port, "
                          "import_batch, a later slice")
        verb = next(v for v in VERBS if v in st)
        r = st[verb]
        if r.get("after"):
            self.now = seconds_of(self.wire.post(
                "/sandbox/clock", {"advance": r["after"]})["now"])
        actor = {"id": self.alias(r.get("actor", OPERATOR))}
        if verb in ("approve", "reject", "withdraw"):
            return self.wire.post("/v1/request", {
                "transition": verb, "actor": actor,
                "object_id": self.alias(r["proposal"])})
        if r.get("creates"):
            raise Waiting("names the objects a request created, which needs "
                          "an event read")
        body: dict[str, Any] = {"transition": r["transition"],
                                "actor": actor}
        if "type" in r:
            tn = body["type"] = r["type"]
        else:
            body["object_id"] = self.alias(r["object"])
            tn = self.type_of(body["object_id"])
        d = self.decl(tn)
        x = (d.get("transitions") or {}).get(r["transition"]) or {}
        specs = dict(d.get("attributes") or {})
        specs.update(x.get("inputs") or {})
        if r.get("inputs"):
            body["inputs"] = {k: self.wire_value(v, specs.get(k))
                              for k, v in r["inputs"].items()}
        if r.get("occurred_at"):
            body["occurred_at"] = self.instant(r["occurred_at"])
        if verb == "propose":
            body["propose"] = True
        v = self.wire.post("/v1/request", body)
        if v["kind"] == "satisfied":
            if verb == "propose":
                self.aliases[r["proposal"]] = v["object_id"]
            elif "as" in r:
                self.aliases[r["as"]] = v["object_id"]
                self.types[v["object_id"]] = r["type"]
        return v

    # ── an expectation ────────────────────────────────────────────────────
    def requested_type(self, tested: dict) -> str:
        verb = next(v for v in VERBS if v in tested)
        r = tested[verb]
        if verb == "approve":
            obj = self.wire.post("/v1/get", {"id": self.alias(r["proposal"])})
            target = obj["attributes"]["target"]
            return self.type_of(target) if target else None
        return r["type"] if "type" in r else self.type_of(
            self.alias(r["object"]))

    def compare(self, want: dict, v: dict, tested: dict) -> str | None:
        got = "applied" if v["kind"] == "satisfied" else v["kind"]
        if got != "applied":
            if want["verdict"] == "applied":
                return f"expected it to apply, and it is refused as {got}"
            if got != want["verdict"]:
                return f"expected {want['verdict']}, and it is {got}"
            if got == "unsatisfied":
                clause, remedy = v["clause"], v["remedy"]
            elif got == "invariant_violated":
                first = v["failures"][0]
                if self.wire.post("/v1/get", {"id": first["object_id"]}) \
                        is None:
                    raise Waiting("an invariant fails on an object the "
                                  "refused request would have created, "
                                  "which no read resolves, so the failure "
                                  "needs to carry its type")
                owner = self.type_of(first["object_id"])
                clause = (first["invariant"]
                          if owner == self.requested_type(tested)
                          else f"{owner}.{first['invariant']}")
                remedy = v["remedy"]
                if "remedy" in want and "remedy" in v.get("not_computed", []):
                    raise NotRun("the remedy of an invariant that fails on "
                                 "an object other than the one requested")
            elif got == "call_refused":
                clause, remedy = v["verdict"]["clause"], v["remedy"]
            else:
                clause, remedy = None, v.get("remedy")
            for k, have in (("clause", clause), ("remedy", remedy)):
                if k in want and have != want[k]:
                    return f"expected the {k} {want[k]}, and it is {have}"
            return None
        if want["verdict"] != "applied":
            return f"expected {want['verdict']}, and the request applies"
        obj = self.wire.post("/v1/get", {"id": v["object_id"]})
        if "state" in want and obj["state"] != want["state"]:
            return f"expected the state {want['state']}, and it is " \
                   f"{obj['state']}"
        for k, expected in (want.get("values") or {}).items():
            problem = self.value_differs(obj, k, expected)
            if problem:
                return problem
        if "occurred_at" in want:
            events = self.wire.post("/v1/history", {"id": v["object_id"]})
            mine = [e for e in events["events"]
                    if e["position"] == v["events"][0]]
            if not mine or mine[0]["occurred_at"] != self.instant(
                    want["occurred_at"]):
                return "the occurred time differs"
        if "cascaded" in want:
            raise Waiting("expects the transitions a request caused, which "
                          "needs an event read")
        if "warned" in want:
            warned = [f["clause"] for f in v["flags"]]
            if list(want["warned"]) != warned:
                return f"expected warnings {want['warned']}, and they are " \
                       f"{warned}"
        return None

    def value_differs(self, obj: dict, k: str, expected: Any) -> str | None:
        d = self.decl(obj["type"])
        spec = (d.get("attributes") or {}).get(k)
        if k in obj["attributes"]:
            got = obj["attributes"][k]
        elif k in BUILT_IN:
            got, spec = obj.get(k), {"type": BUILT_IN[k]}
        else:
            return f"{obj['type']} has no member {k} the wire reports"
        if spec is None:                        # a derived attribute
            same = loose_equal(expected, got)
        else:
            same = self.same(expected, got, spec)
        return None if same else f"expected {k} to hold {expected}, and " \
                                 f"it holds {got}"

    def same(self, expected: Any, got: Any, spec: dict) -> bool:
        if expected is None or got is None:
            return expected is None and got is None
        typ = str(spec.get("type", ""))
        if "reference" in spec or typ.endswith("[]"):
            return self.wire_value(expected, spec) == got
        if typ == "timestamp":
            return seconds_of(self.instant(expected)) == seconds_of(got)
        if typ == "duration":
            return flowrun.seconds(expected) == flowrun.seconds(got)
        if typ.startswith(("decimal", "money")):
            return Fraction(str(expected).split()[-1]) == Fraction(
                str(got).split()[-1])
        if typ == "date":
            return str(expected) == got
        return self.wire_value(expected, spec) == got


def loose_equal(expected: Any, got: Any) -> bool:
    """A derived attribute's value, which no declaration types: compared as
    the reference runner compares it, a number exactly."""
    if isinstance(expected, bool) or isinstance(got, bool):
        return expected == got
    if isinstance(expected, (int, float)) and isinstance(got, (int, str)):
        try:
            return Fraction(str(expected)) == Fraction(str(got))
        except ValueError:
            return False
    return expected == got


def chain(setups: dict, name: str) -> list[dict]:
    s = setups[name]
    return (chain(setups, s["given"]) if "given" in s else []) + [s]


def replay(wire: Wire, module_file: str) -> dict[str, tuple[str, str]]:
    """Every example of a module's examples file: its outcome and why."""
    bundle = wire.get(f"/studio/module?file={module_file}")
    loaded = wire.post("/sandbox/load", {"files": bundle["files"]})
    if not loaded["loaded"]:
        raise AssertionError(f"{module_file} does not load: {loaded}")
    doc = flows.load(bundle["examples"]["text"])
    setups = doc.get("setups") or {}
    out = {}
    for name, ex in (doc.get("examples") or {}).items():
        now = wire.post("/sandbox/reset", {})["now"]
        p = Player(wire, now)
        try:
            parts = (chain(setups, ex["given"]) if "given" in ex else []) \
                + [ex]
            for part in parts:
                if part.get("evaluators"):
                    wire.post("/sandbox/evaluators",
                              {"answers": part["evaluators"]})
                for i, st in enumerate(part.get("steps") or []):
                    v = p.play(st)
                    if v is not None and v["kind"] != "satisfied":
                        raise Failed(f"step {i + 1} is refused as {v['kind']}")
            tested = ({"approve": ex["approve"]} if "approve" in ex
                      else {"request": ex["request"]})
            v = p.play(tested)
            problem = p.compare(ex["expect"], v, tested)
            if problem is None and "proposal" in ex["expect"]:
                prop = wire.post("/v1/get", {
                    "id": p.alias(ex["approve"]["proposal"])})
                if prop["state"] != ex["expect"]["proposal"]:
                    problem = f"expected the proposal " \
                              f"{ex['expect']['proposal']}, and it is " \
                              f"{prop['state']}"
            out[name] = ("failed", problem) if problem else ("passed", "")
        except Waiting as w:
            out[name] = ("waiting", str(w))
        except NotRun as n:
            out[name] = ("not run", str(n))
        except Failed as f:
            out[name] = ("failed", str(f))
    return out

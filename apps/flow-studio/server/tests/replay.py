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
from typing import Any, Callable

from flowstudio.scripts import flowpublish, flowrun, flows
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

    def __init__(self, wire: Wire, now: str,
                 read: Callable[[str], str | None] | None = None,
                 examples: str = ""):
        self.wire, self.now = wire, seconds_of(now)
        # the types the modules declare, which the sandbox's load reports
        self.declared: list[str] = []
        self.aliases: dict[str, str] = {OPERATOR: OPERATOR}
        self.types: dict[str, str] = {}
        self.decls: dict[str, dict] = {}
        # a file beside the examples file, by name, which a `publish` step
        # names, and the examples file's own name (ADR-0166 decision 3)
        self.read, self.examples = read, examples

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
        if typ == "file" and isinstance(v, str):
            # the file an example names: its content the name (§11.3)
            f = flowrun.file_value(v)
            return {"hash": f[1], "size": f[2], "media_type": f[3]}
        if typ.startswith("decimal") and isinstance(v, (int, float)) \
                and not isinstance(v, bool):
            return str(v)               # a decimal is a string on the wire
        if isinstance(v, str) and "." in v and typ and typ[0].isupper():
            return v.split(".", 1)[1]           # an enumeration member
        return v

    # ── a step ────────────────────────────────────────────────────────────
    def scope_spec(self, sequence: str) -> dict | None:
        """The attribute a sequence is scoped by, where an identifier minted
        from it is scoped (`flow-format.md` §4.12)."""
        for tn in self.declared:
            attrs = self.decl(tn).get("attributes") or {}
            for spec in attrs.values():
                ident = (spec or {}).get("identifier") or {}
                if ident.get("sequence") == sequence and ident.get("scope"):
                    return attrs.get(ident["scope"])
        return None

    def root(self, tn: str) -> str:
        """The type a family's legacy keys are qualified by: its farthest
        base, or itself (ADR-0154, C23)."""
        seen = set()
        while tn not in seen and self.decl(tn).get("extends"):
            seen.add(tn)
            tn = self.decl(tn)["extends"]
        return tn

    def imported(self, im: dict) -> None:
        """An import step: a one-object batch, its legacy key the step's
        alias, with an empty disposition file, so it admits nothing; the
        object is then found by its legacy key (ADR-0154)."""
        if im.get("after"):
            self.now = seconds_of(self.wire.post(
                "/sandbox/clock", {"advance": im["after"]})["now"])
        tn, key = im["type"], im["as"]
        specs = self.decl(tn).get("attributes") or {}
        attributes = {
            # a reference names another imported object by its legacy key,
            # which is its alias
            k: v if "reference" in (specs.get(k) or {})
            else self.wire_value(v, specs.get(k))
            for k, v in (im.get("values") or {}).items()}
        # each sequence raised past the legacy system's last number, in the
        # scope written as its attribute's value is, a reference by its alias
        # (`flow-format.md` §11.3, ADR-0173 decision 6)
        sequences = []
        for rs in im.get("sequences") or []:
            raised = {"sequence": rs["sequence"], "last": rs["last"]}
            if "scope" in rs:
                scope = self.wire_value(rs["scope"],
                                        self.scope_spec(rs["sequence"]))
                raised["scope"] = scope if isinstance(scope, str) \
                    else str(scope)
            sequences.append(raised)
        r = self.wire.post("/v1/import_batch", {
            "actor": {"id": OPERATOR},
            "batch": {"objects": [{"type": tn, "legacy_key": key,
                                   "state": im["state"],
                                   "attributes": attributes}],
                      **({"sequences": sequences} if sequences else {})},
            "disposition": [], "dry_run": False})
        if r["refusal"]:
            raise Failed(f"the import of {key} is refused: "
                         f"{r['refusal']['reason']}")
        obj = self.wire.post("/v1/lookup", {
            "actor": {"id": OPERATOR}, "source": "legacy",
            "value": f"{self.root(tn)}:{key}"})
        if obj is None:
            raise Failed(f"the import of {key} wrote nothing lookup finds")
        self.aliases[key] = obj["id"]
        self.types[obj["id"]] = obj["type"]

    def published(self, p: dict) -> None:
        """A publish step: a change drafted with the files it names, each
        read beside the examples file and keyed by the name it declares, its
        source reference the examples file's, then submitted and approved as
        the step's actor; a refusal at any of the three fails the example,
        and the steps after it read the version installed (`flow-format.md`
        §11.3, ADR-0166 decision 3)."""
        if p.get("after"):
            self.now = seconds_of(self.wire.post(
                "/sandbox/clock", {"advance": p["after"]})["now"])
        source = {}
        for f in p["source"]:
            text = self.read(f) if self.read is not None else None
            if text is None:
                raise NotRun(f"the publish step's file {f}, which no "
                             "directory beside the examples file gives")
            source[flowpublish.declared_name(text) or f] = text
        actor = {"id": self.alias(p.get("actor", OPERATOR))}
        v = self.wire.post("/v1/request", {
            "type": "DeclarationChange", "transition": "draft",
            "actor": actor, "inputs": {
                "source": source, "source_ref": f"example:{self.examples}"}})
        if v["kind"] != "satisfied":
            raise Failed(f"the publish's change is not drafted: {v['kind']}")
        change = v["object_id"]
        v = self.wire.post("/v1/request", {
            "transition": "submit", "actor": actor, "object_id": change})
        if v["kind"] != "satisfied":
            raise Failed(f"the publish is refused at submission as "
                         f"{v['kind']}")
        version = self.wire.post("/v1/get", {"id": change})["version"]
        r = self.wire.post("/v1/publish", {
            "actor": actor, "change": change, "expected_version": version})
        if r["verdict"]["kind"] != "satisfied":
            raise Failed(f"the publish is refused at approval as "
                         f"{r['verdict']['kind']}")
        self.decls.clear()          # each type as the version installed has it

    def play(self, st: dict) -> dict | None:
        if "import" in st:
            self.imported(st["import"])
            return None
        if "publish" in st:
            self.published(st["publish"])
            return None
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
            if isinstance(r.get("creates"), dict):
                self.name_created_by_type(r["creates"], v)
            elif r.get("creates"):
                self.name_created(r["creates"], v)
        return v

    def appended(self, v: dict) -> list[dict]:
        """The events a satisfied request appended, read by position
        (ADR-0151)."""
        return self.wire.post("/v1/events",
                              {"positions": v["events"]})["events"]

    def name_created(self, names: list[str], v: dict) -> None:
        """Alias the objects the request created, other than its own, in the
        order it created them: its creation events after the first
        (`flow-format.md` §11.3)."""
        made = [e["object_id"] for e in self.appended(v)[1:]
                if e is not None and e["from_state"] is None]
        if len(names) > len(made):
            raise Failed(f"a step names {len(names)} objects its request "
                         f"creates, and it creates {len(made)}")
        if len({self.type_of(m) for m in made[:len(names)]}) > 1:
            raise Failed("a step names objects its request creates, and "
                         "they are of more than one type")
        for alias, oid in zip(names, made):
            self.aliases[alias] = oid

    def name_created_by_type(self, named: dict[str, list[str]],
                             v: dict) -> None:
        """Alias the objects of each type the request created, in the order
        it created them (`flow-format.md` §11.3, ADR-0173 decision 6)."""
        made = [e["object_id"] for e in self.appended(v)[1:]
                if e is not None and e["from_state"] is None]
        for ct, names in named.items():
            of = [m for m in made if self.type_of(m) == ct]
            if len(names) > len(of):
                raise Failed(f"a step names {len(names)} objects of {ct} its "
                             f"request creates, and it creates {len(of)}")
            self.aliases.update(zip(names, of))

    def caused(self, v: dict) -> list[str]:
        """The transitions a request caused, in the order each began: its
        events after the first, a redaction left out (`flow-format.md`
        §11.4)."""
        # a proposal's own events, an approval's and an invalidation's, are
        # not transitions the request took (flow-format.md §11.4)
        taken = [e for e in self.appended(v) if e is not None
                 and self.type_of(e["object_id"]) != "Proposal"]
        return [f"{self.type_of(e['object_id'])}.{e['transition']}"
                for e in taken[1:] if e["transition"] != "redact"]

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
                owner = first["type"]
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
        if "cascaded" in want and list(want["cascaded"]) != self.caused(v):
            return f"expected the request to cause {want['cascaded']}, and " \
                   f"it causes {self.caused(v)}"
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
            same = derived_equal(expected, got)
        else:
            same = self.same(expected, got, spec)
        return None if same else f"expected {k} to hold {expected}, and " \
                                 f"it holds {got}"

    def metric_differs(self, name: str, want: Any) -> str | None:
        """What differs between a metric an example expects and what a
        reader reads after the request, or None: its value over every row,
        keeping no dimension, or its rows by every dimension it declares, all
        of them in any order (`flow-format.md` §11.4, ADR-0173 decision 6)."""
        by_rows = isinstance(want, list)
        page = self.wire.post("/v1/metric", {
            "actor": {"id": OPERATOR}, "name": name,
            **({} if by_rows else {"keep": []})})
        if page["refused"]:
            return f"expected the metric {name} to read {want}, and the " \
                   f"read is refused as {page['refused']}"
        rows = page["rows"]
        if not by_rows:
            got = rows[0]["value"] if rows else None
            return None if self.metric_value_is(want, got) else \
                f"expected the metric {name} to read {want}, and it reads {got}"
        left = list(rows)
        for w in want:
            dims = {d: v for d, v in w.items() if d != "value"}
            hit = next((r for r in left
                        if set(r["dimensions"]) == set(dims)
                        and all(self.metric_value_is(v, r["dimensions"][d])
                                for d, v in dims.items())
                        and self.metric_value_is(w.get("value"), r["value"])),
                       None)
            if hit is None:
                return f"expected the metric {name} to read a row {w}, " \
                       f"and it reads {rows}"
            left.remove(hit)
        if left:
            return f"expected the metric {name} to read {len(want)} rows, " \
                   f"and it reads {rows}"
        return None

    def metric_value_is(self, expected: Any, got: Any) -> bool:
        """A metric's value or a dimension's, which no declaration types:
        typed by the wire's notation of it, an object by its alias and a
        state or an enumeration member by its name, qualified or not."""
        if expected is None or got is None:
            return expected is None and got is None
        if isinstance(expected, str) and expected in self.aliases:
            return self.aliases[expected] == got
        if isinstance(got, str) and isinstance(expected, str):
            if QUALIFIED.fullmatch(expected) and \
                    expected.split(".", 1)[1] == got:
                return True
            if DURATION.fullmatch(got) and DURATION.fullmatch(expected):
                return flowrun.seconds(expected) == flowrun.seconds(got)
        return derived_equal(expected, got)

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
            return amounts_equal(expected, got, typ[len("money("):-1]
                                 if typ.startswith("money") else None)
        if typ == "date":
            return str(expected) == got
        return self.wire_value(expected, spec) == got


MONEY = re.compile(r"([A-Z]{3}) (-?[0-9]+(?:\.[0-9]+)?)")
QUALIFIED = re.compile(r"[A-Z][A-Za-z0-9]*\.[A-Z][A-Z0-9_]*")
DURATION = re.compile(r"[0-9]+ (?:s|min|h|days?|weeks?)")
DECIMAL = re.compile(r"-?[0-9]+(?:\.[0-9]+)?")


def amounts_equal(expected: Any, got: Any, currency: str | None) -> bool:
    """An amount or a decimal written in an example, the same number as the
    value's at any scale, and of the attribute's currency where one is
    written (D548)."""
    written = str(expected).split()
    if not written or len(written) > 2 or (
            len(written) == 2 and written[0] != currency):
        return False
    try:
        return Fraction(written[-1]) == Fraction(str(got).split()[-1])
    except ValueError:
        return False


def derived_equal(expected: Any, got: Any) -> bool:
    """A derived attribute's value, typed by the wire's notation of it: an
    amount by its currency and number, a decimal by value at any scale, as a
    stored one is (D548)."""
    if isinstance(got, str) and not isinstance(expected, bool):
        m = MONEY.fullmatch(got)
        if m:
            return amounts_equal(expected, got, m.group(1))
        if DECIMAL.fullmatch(got):
            return amounts_equal(expected, got, None)
    return loose_equal(expected, got)


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


def catalogue_reader(wire: Wire) -> Callable[[str], str | None]:
    """A reader of the files beside an examples file, each from its module's
    bundle in the studio's catalogue, which a `publish` step names."""
    def read(name: str) -> str | None:
        try:
            files = wire.get(f"/studio/module?file={name}")["files"]
        except Exception:
            return None
        return next((f["text"] for f in files if f["name"] == name), None)
    return read


def replay(wire: Wire, module_file: str,
           bundle: dict | None = None,
           after: Callable[[Wire, list], None] | None = None,
           read: Callable[[str], str | None] | None = None,
           ) -> dict[str, tuple[str, str]]:
    """Every example of a module's examples file: its outcome and why. The
    module's files come from the studio's catalogue on the same wire, or are
    given, for an engine's service that serves no studio routes. `after`, if
    given, reads the sandbox after each example, with the types it holds.
    `read` reads a file beside the examples file, which a `publish` step
    names; with no bundle given, the catalogue's is read."""
    if bundle is None:
        bundle = wire.get(f"/studio/module?file={module_file}")
        if read is None:
            read = catalogue_reader(wire)
    loaded = wire.post("/sandbox/load", {"files": bundle["files"]})
    if not loaded["loaded"]:
        raise AssertionError(f"{module_file} does not load: {loaded}")
    doc = flows.load(bundle["examples"]["text"])
    setups = doc.get("setups") or {}
    out = {}
    for name, ex in (doc.get("examples") or {}).items():
        now = wire.post("/sandbox/reset", {})["now"]
        p = Player(wire, now, read, (bundle.get("examples") or {}).get(
            "name", module_file.replace(".yaml", ".examples.yaml")))
        p.declared = loaded["types"]
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
            for name_, want in (ex["expect"].get("metrics") or {}).items():
                problem = problem or p.metric_differs(name_, want)
            if problem is None and "proposal" in ex["expect"]:
                prop = wire.post("/v1/get", {
                    "id": p.alias(ex["approve"]["proposal"])})
                # the format writes the state in lower case (§11.4, D521)
                if prop["state"].lower() != ex["expect"]["proposal"]:
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
        if after is not None:
            after(wire, loaded.get("types") or [])
    return out

"""A sandbox store over the reference runner, reached by the library's operations.

The wire is the service's (ADR-0150): `request`, `get`, `history` and
`declaration` as `library-api.md` §6 shapes them, and the sandbox controls,
which a deployment's service never offers. The runner decides everything; this
module translates a library request into what the runner takes and the
runner's outcome into the library's shapes.

The runner is a reference, not the engine, and computes less than a verdict or
an event can carry. Every response names, as `not_computed`, the fields the
runner leaves out, so nothing here fills one in by guessing
(`docs/design/flow-studio.md` §2).
"""

from __future__ import annotations

import copy

import threading
from fractions import Fraction
from typing import Any

from . import values
from .scripts import flowexpr, flowrun, flows

PROPOSAL_VERBS = ("approve", "reject", "withdraw")
UNSUPPORTED = ("expected_version", "idempotency_key", "expires_at")
BUILT_IN = {
    "id": ("text",),
    "open": ("bool",),
    "created_at": ("timestamp",),
    "created_by_kind": ("text",),
    "recorded_from": ("timestamp",),
    "declaration_version": ("int",),
}
EVENT_NOT_COMPUTED = [
    "object_seq", "source", "principal", "context", "cause", "taint_version",
    "txn", "retries", "payload", "reads", "consulted", "as_of", "flags",
]


class Fault(Exception):
    """A request the sandbox cannot answer: the wire's 4xx or 5xx."""

    def __init__(self, status: int, message: str):
        super().__init__(message)
        self.status, self.message = status, message


def not_run(e: Exception) -> Fault:
    """A construct the runner does not yet execute, which is never guessed."""
    return Fault(501, f"not run: {e}")


def build(files: list[tuple[str, str]]) -> tuple[dict, dict]:
    """The library of types and the modules, as the flow checker builds them
    from module files given in import order."""
    library: dict = {}
    modules: dict = {}
    for _name, text in files:
        doc = flows.load(text)
        if isinstance(doc, dict) and "examples_for" in doc:
            continue
        flows.literals(doc)
        modules[doc["module"]] = doc
        view, _back = flows.view(doc, library)
        library.update(dict(flows.types(view)))
    return library, modules


def checked(body: dict) -> tuple[list[tuple[str, str]], dict | None, int]:
    """A load's module files and the flow checker's verdict on them, which the
    studio's load runs before either backend loads them (ADR-0153 decision
    5): the files, the answer refusing them where it finds them wrong, and
    the number of its notes."""
    files = [(f["name"], f["text"]) for f in body.get("files") or []]
    files = [(n, t) for n, t in files if not n.endswith(".examples.yaml")]
    if not files:
        raise Fault(400, "load takes the module files, in import order")
    found, notes = flows.check(files)
    if found:
        return files, {
            "loaded": False,
            "findings": [
                {"file": f, "line": line, "code": code, "message": msg}
                for f, line, _sev, code, msg in found
            ],
        }, len(notes)
    return files, None, len(notes)


def written(modules: dict) -> dict:
    """Each declared type as its module writes it, with what it binds and
    inherits merged in, each piece as written, which `declaration` returns as
    the core does: a binder's machine's states, conditions and transitions,
    the machine's initial ones giving way to the binder's own (flow-format.md
    §4.10), and a subtype's bases' attributes, derived attributes and
    invariants, the farthest base first and its own last
    (declaration-syntax.md §2)."""
    machines = {n: spec for m in modules.values()
                for n, spec in (m.get("machines") or {}).items()}
    types = {tn: spec for m in modules.values()
             for tn, spec in (m.get("types") or {}).items()}
    out = {}
    for tn, spec in types.items():
        t = copy.deepcopy(spec)
        machine = machines.get(t.get("state_machine"))
        if machine:
            own = t.get("transitions") or {}
            own_initial = any((x or {}).get("kind") == "initial"
                              for x in own.values())
            transitions = {
                n: x for n, x in (machine.get("transitions") or {}).items()
                if not (own_initial and (x or {}).get("kind") == "initial")}
            conditions = dict(machine.get("conditions") or {})
            conditions.update(t.get("conditions") or {})
            t["states"] = copy.deepcopy(machine.get("states") or {})
            t["transitions"] = copy.deepcopy({**transitions, **own})
            t["conditions"] = copy.deepcopy(conditions)
        chain, seen, base = [], {tn}, t.get("extends")
        while base in types and base not in seen:
            chain.append(base)
            seen.add(base)
            base = types[base].get("extends")
        for key in ("attributes", "derived_attributes", "invariants"):
            merged: dict = {}
            for b in reversed(chain):
                merged.update(types[b].get(key) or {})
            merged.update(spec.get(key) or {})
            if merged:
                t[key] = copy.deepcopy(merged)
        out[tn] = t
    return out


class Sandbox:
    """One store, its clock and its evaluators' answers, held by the caller."""

    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.runner = None
        self.store = None
        self.stubs: dict = {}
        self.parts: tuple | None = None
        # each declared type as its module writes it, which `declaration`
        # returns, where the runner's own entry is its loader's normal form
        self.written: dict = {}

    # ── sandbox control (ADR-0150 decision 3) ─────────────────────────────
    def load(self, body: dict) -> dict:
        files, refused, notices = checked(body)
        if refused:
            return refused
        library, modules = build(files)
        self.written = written(modules)
        enums = {
            n: list(v)
            for m in modules.values()
            for n, v in (m.get("enumerations") or {}).items()
        }
        kinds = dict(library)
        kinds.update(flowrun.observation_kinds(modules))
        kinds["Operator"] = {
            "attributes": {
                "identity": {"type": "identity", "actor_kind": "human"}
            },
            "states": {
                "ACTIVE": {"category": "live"},
                "RETIRED": {"category": "closed", "final": True},
            },
        }
        declared, homes = flowrun.declarations(modules)
        self.parts = (kinds, enums, declared, homes)
        self.runner = flowrun.Runner(kinds, enums)
        self.fresh()
        return {
            "loaded": True,
            "modules": list(modules),
            "notices": notices,
            "now": values.timestamp_text(self.store.now),
            # what a caller may create and answer in this sandbox: every
            # type the modules declare, and every evaluator function
            "types": [tn for m in modules.values()
                      for tn in (m.get("types") or {})],
            "evaluators": [f"{e}.{fn}" for m in modules.values()
                           for e, ev in (m.get("evaluators") or {}).items()
                           for fn in (ev.get("functions") or {})],
        }

    def fresh(self) -> None:
        kinds, enums, declared, homes = self.parts
        self.store = flowrun.Store(kinds, enums, declared, homes)
        self.stubs = {}
        # each imported object by its legacy key, qualified by its family's
        # root type (ADR-0154, C23)
        self.legacy: dict[tuple[str, str], str] = {}

    def loaded(self) -> None:
        if self.runner is None:
            raise Fault(409, "no modules are loaded; load them first")

    def reset(self, body: dict) -> dict:
        self.loaded()
        self.fresh()
        return {"now": values.timestamp_text(self.store.now)}

    def clock(self, body: dict) -> dict:
        self.loaded()
        if "advance" in body:
            try:
                step = flowrun.seconds(body["advance"])
            except (ValueError, KeyError) as e:
                raise Fault(400, f"advance takes a duration such as '1 h': {e}")
            target = self.store.now + step
        elif "set" in body:
            try:
                target = flowrun.timestamp(body["set"])
            except ValueError as e:
                raise Fault(400, f"set takes an RFC 3339 timestamp: {e}")
        else:
            return {"now": values.timestamp_text(self.store.now)}
        if target < self.store.now:
            raise Fault(400, "the clock never moves back, or an interval would")
        self.store.now = target
        return {"now": values.timestamp_text(self.store.now)}

    def evaluators(self, body: dict) -> dict:
        """The answers its evaluators give, written as an example stubs them
        (`flow-format.md` §11.3), added to those already given."""
        self.loaded()
        self.stubs.update(body.get("answers") or {})
        return {"answers": self.stubs}

    # ── the library's operations (`library-api.md` §6) ─────────────────────
    def request(self, body: dict) -> dict:
        self.loaded()
        for k in UNSUPPORTED:
            if body.get(k) is not None:
                raise not_run(f"a request's {k}")
        actor = (body.get("actor") or {}).get("id")
        xn = body.get("transition")
        if not actor or not xn:
            raise Fault(400, "a request names its transition and its actor")
        oid, tn = body.get("object_id"), body.get("type")
        if (oid is None) == (tn is None):
            raise Fault(400, "a request names an object_id or, to create, a type")
        s = self.store
        if oid in s.proposals or (body.get("propose") and xn):
            return self.proposal(body, actor)
        if oid is not None:
            tn = s.objects[oid].type if oid in s.objects else None
        x = ((self.runner.kinds.get(tn) or {}).get("transitions")
             or {}).get(xn) or {}
        inputs = self.inputs(tn, xn, body.get("inputs"))
        occurred = body.get("occurred_at")
        try:
            at = flowrun.timestamp(occurred) if occurred else None
        except ValueError as e:
            raise Fault(400, f"occurred_at takes an RFC 3339 timestamp: {e}")
        aliases = self.aliases()
        before = s.position
        try:
            result = self.runner.attempt(
                s, tn, xn, oid, actor, inputs, aliases, self.stubs, at
            )
        except flowrun.Refused as r:
            return self.refusal(r, tn, oid, x, xn)
        except flowrun.NotRunnable as e:
            raise not_run(e)
        except flowrun.ExampleError as e:
            raise Fault(400, str(e))
        return self.satisfied(result, tn, before)

    def inputs(self, tn: str | None, xn: str, given: dict | None) -> dict:
        """A request's inputs from the wire, as the runner reads an
        example's."""
        t = self.runner.kinds.get(tn) or {}
        x = (t.get("transitions") or {}).get(xn) or {}
        specs = dict(t.get("attributes") or {})
        specs.update(x.get("inputs") or {})
        return {k: values.decode_input(v, specs.get(k), self.runner.enums)
                for k, v in (given or {}).items()}

    def proposal(self, body: dict, actor: str) -> dict:
        """Filing a request as a proposal, and the built-in proposal's own
        transitions, which the runner holds beside its objects (`DESIGN.md`
        §9)."""
        s, xn, oid = self.store, body["transition"], body.get("object_id")
        aliases = self.aliases()
        if body.get("propose"):
            st = {"transition": xn, "actor": actor, "proposal": "__new__"}
            if oid is not None:
                if oid not in s.objects:
                    return {"kind": "not_found",
                            "remedy": "unreachable_from_here"}
                st["object"] = oid
                tn = s.objects[oid].type
            else:
                st["type"] = tn = body["type"]
            if body.get("inputs"):
                st["inputs"] = self.inputs(tn, xn, body["inputs"])
            if body.get("occurred_at"):
                st["occurred_at"] = body["occurred_at"]
            try:
                flowrun.step(self.runner, s, {"propose": st}, aliases,
                             self.stubs)
            except flowrun.Refused as r:
                return self.refusal(r, tn, oid, {}, xn)
            except flowrun.NotRunnable as e:
                raise not_run(e)
            except flowrun.ExampleError as e:
                raise Fault(400, str(e))
            key = aliases["__new__"][1]
            return {"kind": "satisfied", "object_id": key, "version": 1,
                    "events": [], "replayed": False, "flags": [],
                    "not_computed": ["events", "version"]}
        if xn not in PROPOSAL_VERBS:
            return {"kind": "unknown_transition", "transition": xn,
                    "remedy": "self_serviceable"}
        before = s.position
        st = {xn: {"proposal": oid, "actor": actor}}
        try:
            outcome = flowrun.step(self.runner, s, st, aliases, self.stubs,
                                   under_test=True)
        except flowrun.Refused as r:
            return self.refusal(r, None, oid, {}, xn)
        except flowrun.NotRunnable as e:
            raise not_run(e)
        if outcome is None:
            return {"kind": "satisfied", "object_id": oid, "version": 1,
                    "events": [], "replayed": False, "flags": [],
                    "not_computed": ["events", "version"]}
        if outcome[0] == "refused":
            held = s.proposals[oid]
            return self.refusal(outcome[1], held["type"], held["object"],
                                {}, held["transition"])
        return self.satisfied(outcome, s.proposals[oid]["type"], before)

    # ── the import (ADR-0154) ─────────────────────────────────────────────
    def root(self, tn: str) -> str:
        """The type a family's legacy keys are qualified by: its farthest
        base, or itself."""
        kinds, seen = self.runner.kinds, set()
        while tn not in seen and (kinds.get(tn) or {}).get("extends") in kinds:
            seen.add(tn)
            tn = kinds[tn]["extends"]
        return tn

    def import_batch(self, body: dict) -> dict:
        """New objects of mirror types with an empty disposition file, which
        is all an example's import is; the rest of the import, its classes,
        refreshes and legacy history, is the core's (ADR-0154 decision 8)."""
        self.loaded()
        batch = body.get("batch") or {}
        if not (body.get("actor") or {}).get("id"):
            raise Fault(400, "an import names its actor")
        if batch.get("admitted") is not None:
            raise Fault(400, "a batch admits through its disposition file, "
                             "not admitted (ADR-0154)")
        for k, what in (("dry_run", "a dry run"),
                        ("disposition", "a disposition file")):
            if body.get(k):
                raise not_run(what)
        if batch.get("sequences"):
            raise not_run("raising a sequence")
        kinds = self.runner.kinds
        objects = batch.get("objects") or []
        report: dict[str, Any] = {"written": 0, "findings": [],
                                  "refused_types": [], "refusal": None}
        for o in objects:
            tn = o.get("type")
            if tn in kinds and not kinds[tn].get("mirror") \
                    and tn not in report["refused_types"]:
                report["refused_types"].append(tn)
        backup, made = copy.deepcopy(self.store), {}

        def refused(o: dict, why: str) -> dict:
            self.store = backup
            report.update(written=0, refusal={
                "type": o.get("type"), "legacy_key": o.get("legacy_key"),
                "reason": why})
            return report

        for o in objects:
            tn, key = o.get("type"), o.get("legacy_key")
            for k in ("entered", "created_at", "created_by_kind",
                      "legacy_entries", "legacy_intervals", "silent_spans",
                      "occurred_at"):
                if o.get(k):
                    raise not_run(f"an import's {k}")
            qualified = ("legacy", f"{self.root(tn)}:{key}")
            if qualified in self.legacy:
                raise not_run("a refresh of an object already imported")
            if qualified in made:
                return refused(o, "its legacy key appears twice in the batch")
            specs = (kinds.get(tn) or {}).get("attributes") or {}
            given: dict[str, Any] = {}
            for k, v in (o.get("attributes") or {}).items():
                spec = specs.get(k)
                if spec and "reference" in spec and v is not None:
                    target = self.root(spec["reference"].rstrip("[]"))
                    keys = v if isinstance(v, list) else [v]
                    ids = [made.get(("legacy", f"{target}:{x}"))
                           or self.legacy.get(("legacy", f"{target}:{x}"))
                           for x in keys]
                    if None in ids:
                        return refused(o, f"{k} names a legacy key the "
                                          "batch and the store lack")
                    given[k] = tuple(ids) if isinstance(v, list) else ids[0]
                else:
                    given[k] = values.decode_input(v, spec, self.runner.enums) \
                        if spec else v
            try:
                made[qualified] = flowrun.import_object(
                    self.runner, self.store, tn, o.get("state"), given)
            except flowrun.ImportRefused as r:
                return refused(o, str(r))
            report["written"] += 1
        self.legacy.update(made)
        return report

    def lookup(self, body: dict) -> dict | None:
        """The object whose external identifier for a source has a value, a
        legacy key among them (`library-api.md` §6)."""
        self.loaded()
        source, value = body.get("source"), body.get("value")
        if source == "legacy":
            oid = self.legacy.get((source, value))
        else:
            oid = next((
                o.id for o in sorted(self.store.objects.values(),
                                     key=lambda o: o.id_order)
                if any((spec or {}).get("external") == source
                       and str(o.attrs.get(a)) == value
                       for a, spec in ((self.runner.kinds.get(o.type) or {})
                                       .get("attributes") or {}).items())),
                None)
        return self.get({"id": oid}) if oid else None

    def get(self, body: dict) -> dict | None:
        self.loaded()
        oid = body.get("id")
        s = self.store
        if oid in s.proposals:
            p = s.proposals[oid]
            # the state as the built-in type names it (DESIGN.md §9, D521)
            return {"id": oid, "type": "Proposal",
                    "state": p["state"].upper(),
                    "attributes": {"transition": p["transition"],
                                   "target": p["object"]},
                    "not_computed": ["version", "declaration_version",
                                     "created_at", "created_by_kind",
                                     "recorded_from", "created_dated"]}
        if oid not in s.objects:
            return None
        o = s.objects[oid]
        t = self.runner.kinds.get(o.type) or {}
        ev = flowrun.Evaluator(s, o.type, o.id)
        attrs = {}
        try:
            for k, spec in (t.get("attributes") or {}).items():
                typ = values.type_of_spec(spec, self.runner.enums)
                attrs[k] = values.encode(o.attrs.get(k, flowrun.ABSENT), typ)
            for k, spec in (t.get("derived_attributes") or {}).items():
                expr = flowexpr.parse(spec["expression"])
                # typed as the runner types it, a bare name read on `this`
                typ = values.type_of_kind(
                    ev.kind(expr, {"this": ("object", o.type)}))
                attrs[k] = values.encode(ev.member(o.id, k), typ)
            built = {k: values.encode(ev.member(o.id, k), typ)
                     for k, typ in BUILT_IN.items()}
        except flowrun.NotRunnable as e:
            raise not_run(e)
        except values.Unencodable as e:
            raise Fault(500, f"cannot encode a value of {oid}: {e}")
        return {"id": o.id, "type": o.type, "state": o.state,
                "version": o.version,
                "declaration_version": built["declaration_version"],
                "created_at": built["created_at"],
                "created_by_kind": built["created_by_kind"],
                "attributes": attrs, "recorded_from": built["recorded_from"],
                "open": built["open"], "not_computed": ["created_dated"]}

    def history(self, body: dict) -> dict:
        self.loaded()
        oid = body.get("id")
        events = [self.event(e) for e in self.store.events
                  if e["object"] == oid]
        return {"events": events, "not_computed": EVENT_NOT_COMPUTED}

    def availability(self, body: dict) -> list:
        """What each requestable transition of an object offers its actor
        now (`library-api.md` §5, ADR-0152)."""
        self.loaded()
        actor = (body.get("actor") or {}).get("id")
        oid = body.get("id")
        if not actor or not oid:
            raise Fault(400, "availability names its actor and an object's id")
        s = self.store
        try:
            offers = self.runner.availability(s, actor, oid)
        except flowrun.Refused as r:
            raise Fault(404, f"no object {oid}") from r
        except flowrun.NotRunnable as e:
            raise not_run(e)
        out = []
        for xn, creates, how, refusal, schema, unevaluated, flags in offers:
            tn = creates or s.objects[oid].type
            out.append({
                "transition": xn, "creates": creates, "availability": how,
                "verdict": self.refusal(refusal, tn, oid, {}, xn)
                if refusal else None,
                "inputs": schema, "unevaluated": unevaluated,
                "flags": [{"clause": c, "remedy": rem} for c, rem in flags],
            })
        return out

    def metric(self, body: dict) -> dict:
        """A metric read over every row it selects (`library-api.md` §5).
        The runner reads a metric as a guard does, one value with its
        dimensions bound, so a read that groups by a dimension, or filters,
        is not run."""
        self.loaded()
        name = body.get("name")
        if not name:
            raise Fault(400, "metric names the metric")
        if body.get("keep") or body.get("filter") or body.get("cursor"):
            raise not_run("a metric read grouped by a dimension, filtered or "
                          "paged, which the runner reads only bound")
        s = self.store
        try:
            owner, spec, module = s.find_metric(flowrun.parsed(name))
            window = flowrun.seconds(body["over"]) if body.get("over") \
                else None
            ev = flowrun.Evaluator(s, owner, stubs=flowrun.NEVER_ASKED)
            bound = dict(body.get("bind") or {})
            v, kind = ev.read_metric(owner, spec, module, bound, window)
            value = values.encode(v, values.type_of_kind(kind))
        except flowrun.NotRunnable as e:
            raise not_run(e)
        except values.Unencodable as e:
            raise Fault(500, f"cannot encode {name}'s value: {e}")
        return {"rows": [{"dimensions": bound, "value": value}],
                "cursor": None, "flags": [],
                "not_computed": ["rows[].gaps", "flags",
                                 "history_complete", "refused"]}

    def events(self, body: dict) -> dict:
        """The event at each position given, in the order given, and null
        where the log holds none (ADR-0151)."""
        self.loaded()
        positions = body.get("positions")
        if not isinstance(positions, list) or not all(
                isinstance(p, int) and not isinstance(p, bool)
                for p in positions):
            raise Fault(400, "events takes positions, a list of integers")
        log = {e["position"]: e for e in self.store.events}
        return {"events": [self.event(log[p]) if p in log else None
                           for p in positions],
                "not_computed": EVENT_NOT_COMPUTED}

    def declaration(self, body: dict) -> dict:
        self.loaded()
        tn = body.get("type")
        t = self.runner.kinds.get(tn)
        if t is None:
            raise Fault(404, f"no type {tn} is declared")
        out = jsonable(self.written.get(tn, t))
        # the members of each enumeration the type's attributes are of, which
        # its rules are read against
        used = {str((spec or {}).get("type", "")).removesuffix("[]")
                for spec in (t.get("attributes") or {}).values()}
        used |= {str((spec or {}).get("type", "")).removesuffix("[]")
                 for x in (t.get("transitions") or {}).values()
                 for spec in (x.get("inputs") or {}).values()}
        out["enumerations"] = {e: list(self.runner.enums[e])
                               for e in sorted(used)
                               if e in self.runner.enums}
        return out

    # ── translations ──────────────────────────────────────────────────────
    def aliases(self) -> dict:
        """Every name the runner may be given: each object's id, and each
        proposal's, as an example's aliases would name them."""
        s = self.store
        names: dict[str, Any] = {oid: oid for oid in s.objects}
        names[flowrun.OPERATOR] = flowrun.OPERATOR
        names.update({k: ("proposal", k) for k in s.proposals})
        return names

    def satisfied(self, result: tuple, tn: str, before: int) -> dict:
        s = self.store
        root = result[1]
        flags = []
        for clause, mode in result[2]:
            if mode != "warn":
                continue            # an audit is recorded, not told
            owner, _, name = clause.rpartition(".")
            cond = ((self.runner.kinds.get(owner) or {}).get("conditions")
                    or {}).get(name) or {}
            flags.append({"clause": name if owner == tn else clause,
                          "remedy": cond.get("remedy")})
        return {"kind": "satisfied", "object_id": root,
                "version": s.objects[root].version if root in s.objects
                else None,
                # by position, the order each began, which puts the parent's
                # first though the runner appends it after its effects'
                # (`library-api.md` §4)
                "events": sorted(e["position"] for e in s.events
                                 if e["position"] > before),
                "replayed": False, "flags": flags,
                "not_computed": ["flags[].unknown", "flags[].objects"]}

    def refusal(self, r, tn, oid, x, xn) -> dict:
        """A refusal as the library's verdict (`library-api.md` §4), its
        shape named by `kind` (ADR-0150)."""
        v = r.verdict
        out: dict[str, Any] = {"kind": v, "remedy": r.remedy}
        if v == "unsatisfied":
            out.update(clause=r.clause, not_computed=[
                "unknown", "objects", "proposable", "consulted"])
        elif v == "invariant_violated":
            out["failures"] = [{"object_id": r.object_id,
                                "type": r.object_type,
                                "invariant": (r.clause or "").rpartition(
                                    ".")[2],
                                "remedy": r.remedy}]
            out["not_computed"] = ["failures[1:]", "objects"]
            if r.remedy is None:
                out["not_computed"].append("remedy")
        elif v == "call_refused":
            inner = r.inner
            # the call's own refusal, its clause qualified by the type the
            # call targets, as an expectation names it (`flow-format.md`
            # §11.4)
            out.update(step=r.step, object_id=r.object_id, verdict={
                "kind": inner.verdict if inner else None,
                "clause": r.clause, "remedy": r.remedy})
            out["not_computed"] = ["the call's verdict beyond its clause"]
        elif v == "unavailable":
            out["state"] = (self.store.objects[oid].state
                            if oid in self.store.objects else None)
        elif v == "unknown_transition":
            out["transition"] = xn
        elif v == "invalid_input":
            out.update(inputs=[], not_computed=["inputs"])
        elif v == "not_requestable":
            parents = (x or {}).get("only_via") or []
            out["parents"] = list(parents) if isinstance(parents, list) \
                else [parents]
        elif v == "over_limit":
            out["not_computed"] = ["loop", "bound"]
        return out

    def event(self, e: dict) -> dict:
        def state(v):
            return None if v is flowrun.ABSENT else v[2]

        ts = values.timestamp_text
        return {"position": e["position"], "object_id": e["object"],
                "transition": e["transition"],
                "from_state": state(e["from_state"]),
                "to_state": state(e["to_state"]),
                "changes_state": state(e["from_state"]) != state(
                    e["to_state"]),
                "actor_id": e["actor_id"], "actor_kind": e["actor_kind"],
                "declaration_version": e["declaration_version"],
                "recorded_at": ts(e["recorded_at"]),
                "occurred_at": ts(e["occurred_at"]),
                "imported": e["imported"]}


def jsonable(v: Any) -> Any:
    """A declaration as JSON: its own YAML data, which holds no value of the
    runner's."""
    if isinstance(v, dict):
        return {str(k): jsonable(x) for k, x in v.items()}
    if isinstance(v, (list, tuple)):
        return [jsonable(x) for x in v]
    if isinstance(v, Fraction):
        return str(v)
    if v is None or isinstance(v, (bool, int, float, str)):
        return v
    return str(v)


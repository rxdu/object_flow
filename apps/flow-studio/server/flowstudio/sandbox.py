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
import json

import threading
from fractions import Fraction
from typing import Any

from . import values
from .scripts import flowexpr, flowpublish, flowrun, flows, flowtools

PROPOSAL_VERBS = ("approve", "reject", "withdraw")
UNSUPPORTED = ("expected_version", "idempotency_key", "expires_at")
# the built-in attributes that hold JSON, kept as text, which the wire
# answers as the JSON they hold (ADR-0182 decision 9)
HELD_AS_JSON = {("DeclarationChange", "source"), ("DeclarationChange", "report"),
                ("DeclarationChange", "evidence"), ("Proposal", "inputs")}

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
    "txn", "payload", "reads", "consulted", "as_of", "flags",
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


def version_parts(files: list[tuple[str, str]]) -> tuple:
    """A version's kinds, enumerations, metric declarations, the module of
    each type, and its modules, from its module files in import order, as a
    load builds them."""
    library, modules = build(files)
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
    return kinds, enums, declared, homes, modules


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
        self.written_at_load: dict = {}
        self.source: dict = {}

    # ── sandbox control (ADR-0150 decision 3) ─────────────────────────────
    def load(self, body: dict) -> dict:
        files, refused, notices = checked(body)
        if refused:
            return refused
        kinds, enums, declared, homes, modules = version_parts(files)
        # each type as the loaded version writes it, which a reset returns to
        self.written = self.written_at_load = written(modules)
        self.parts = (kinds, enums, declared, homes)
        # the installed version's source, each file by the name it declares,
        # which a change to the declaration is checked against (ADR-0166)
        self.source = {flowpublish.declared_name(t) or n: t for n, t in files}
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
        self.written = self.written_at_load
        self.runner = flowrun.Runner(kinds, enums)
        self.store = flowrun.Store(kinds, enums, declared, homes)
        self.store.source = dict(self.source)
        # each version the store holds, by number: version 1, the load, its
        # source reference `sandbox:load` and its digest computed, as the
        # core's load installs it (ADR-0155 decision 4); a publish adds one
        self.store.versions = {1: {
            "source": dict(self.source), "source_ref": "sandbox:load",
            "source_digest": flowpublish.source_digest(self.source)}}
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
        actor = self.who(body)
        xn = body.get("transition")
        if not actor or not xn:
            raise Fault(400, "a request names its transition and its actor")
        oid, tn = body.get("object_id"), body.get("type")
        if (oid is None) == (tn is None):
            raise Fault(400, "a request names an object_id or, to create, a type")
        s = self.store
        if oid in s.proposals or (body.get("propose") and xn):
            return self.proposal(body, actor)
        if tn == flowrun.DECLARATION_CHANGE or (
                oid in s.objects
                and s.objects[oid].type == flowrun.DECLARATION_CHANGE):
            return self.change(body, actor)
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

    def change(self, body: dict, actor: str) -> dict:
        """A request on a change to the declaration, which the runner's
        publishing decides (`DESIGN.md` §9, ADR-0166); its inputs are taken
        as given, a source being a map of files."""
        s = self.store
        xn, oid = body["transition"], body.get("object_id")
        before = s.position
        try:
            result = flowpublish.request(s, xn, oid, actor,
                                         body.get("inputs"), flows,
                                         self.evidence)
        except flowrun.Refused as r:
            return self.refusal(r, flowrun.DECLARATION_CHANGE, oid, None, xn)
        except flowrun.NotRunnable as e:
            raise not_run(e)
        return self.satisfied(result, flowrun.DECLARATION_CHANGE, before)

    def evidence(self, ref: dict) -> dict:
        """One reference a change cites, read as the store stands and kept
        as it answered, with its as-of time: a metric's page, or its
        refusal, a value (ADR-0159)."""
        if "metric" not in ref:
            raise flowrun.NotRunnable(
                "a cited diagnostics read, which the runner does not serve")
        body = {"name": ref["metric"], "bind": ref.get("bind"),
                "keep": ref.get("keep"), "over": ref.get("over"),
                "filter": ref.get("filter")}
        page = self.metric({k: v for k, v in body.items() if v is not None})
        out = {k: page[k] for k in ("rows", "flags", "history_complete",
                                    "refused") if k in page}
        out["reference"] = ref
        out["as_of"] = values.timestamp_text(self.store.now)
        return out

    def publish(self, body: dict) -> dict:
        """The library's `publish` (`library-api.md` §6): with `dry_run`, the
        change's report, which writes nothing; the approval, which installs
        the version, is the runner's next slice (ADR-0166 decision 4)."""
        self.loaded()
        change = body.get("change")
        if not (body.get("actor") or {}).get("id") or not change:
            raise Fault(400, "publish names its actor and the change")
        s = self.store
        if change not in s.objects or \
                s.objects[change].type != flowrun.DECLARATION_CHANGE:
            raise Fault(404, f"no change {change}")
        actor = self.who(body)
        if body.get("dry_run") is True:
            try:
                report = flowpublish.dry_run(s, change, flows, actor)
            except flowrun.NotRunnable as e:
                raise not_run(e)
            return {"verdict": None, "report": report}
        # the version of the change the approver read, required (ADR-0101)
        expected = body.get("expected_version")
        if not isinstance(expected, int) or isinstance(expected, bool):
            raise Fault(400, "a publish names the expected_version of the "
                             "change it approves")
        before = s.position
        try:
            result, report = flowpublish.approve(s, change, actor, expected,
                                                 flows)
        except flowrun.Refused as r:
            result, report = r, None
        except flowrun.NotRunnable as e:
            raise not_run(e)
        tn = flowrun.DECLARATION_CHANGE
        if isinstance(result, flowrun.Refused):
            verdict = self.refusal(result, tn, change, None, "publish")
        else:
            # the store now holds the new version, which every later request
            # is decided under, until a reset reloads the modules loaded; and
            # `declaration` returns its types as its modules write them
            self.runner = flowrun.Runner(s.kinds, s.enums)
            _library, modules = build(flowpublish.import_order(flows, s.source))
            self.written = written(modules)
            verdict = self.satisfied(("applied", change, [], []), tn, before)
        return {"verdict": verdict, "report": report}

    def who(self, body: dict) -> str | None:
        """The wire's actor as the runner names actors, by object id: an
        object's id taken as it is, else the object holding it as its actor
        identity, the earliest on a tie, as the core resolves one (ADR-0173
        decision 13, `DESIGN.md` §5.8); a name neither resolves is passed on,
        for the runner to refuse as actor_known."""
        named = (body.get("actor") or {}).get("id")
        s = self.store
        if not named or named in s.objects:
            return named
        held = [o for o in s.objects.values()
                if (s.actor_attr(o.type) or (None,))[0] is not None
                and o.attrs.get(s.actor_attr(o.type)[0]) == named]
        return min(held, key=lambda o: o.id_order).id if held else named

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
            # the request it holds as the wire gave it, as the core keeps it
            s.objects[key].attrs["inputs"] = json.dumps(
                body.get("inputs") or {}, sort_keys=True,
                separators=(",", ":"), ensure_ascii=False)
            return {"kind": "satisfied", "object_id": key, "version": 1,
                    "events": [s.position], "replayed": False, "flags": [],
                    "not_computed": []}
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
            p = s.objects[oid]
            return {"kind": "satisfied", "object_id": oid,
                    "version": p.version,
                    "events": [e["position"] for e in s.events
                               if e["position"] > before],
                    "replayed": False, "flags": [], "not_computed": []}
        if outcome[0] == "refused" and getattr(outcome[1], "on_proposal",
                                               False):
            # the proposal's own state refuses the decision (DESIGN.md §9)
            return self.refusal(outcome[1], flowrun.PROPOSAL, oid, {}, xn)
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
        """Objects of mirror types with an empty disposition file, new or
        refreshed by their legacy keys, which is all an example's import is
        (ADR-0176 decision 4); the rest of the import, its classes and legacy
        history, is the core's (ADR-0154 decision 8)."""
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
        raises = []
        for rs in batch.get("sequences") or []:
            seq, last = rs.get("sequence"), rs.get("last")
            if not isinstance(seq, str) or isinstance(last, bool) \
                    or not isinstance(last, int):
                raise Fault(400, "a sequence names its sequence and the last "
                                 "value minted")
            raises.append((seq, last, rs.get("scope") or ""))
        kinds = self.runner.kinds
        objects = batch.get("objects") or []
        report: dict[str, Any] = {"written": 0, "findings": [],
                                  "refused_types": [], "refusal": None}
        for o in objects:
            tn = o.get("type")
            if tn in kinds and not kinds[tn].get("mirror") \
                    and tn not in report["refused_types"]:
                report["refused_types"].append(tn)
        backup, made, seen = copy.deepcopy(self.store), {}, set()

        def refused(o: dict, why: str) -> dict:
            self.store = backup
            report.update(written=0, refusal={
                "type": o.get("type"), "legacy_key": o.get("legacy_key"),
                "reason": why})
            return report

        # a raise names a sequence a module declares, in a scope its
        # identifiers take, with a last value that leaves a next, as the core
        # refuses one (fourth production review A-15)
        declared, scoped = set(), set()
        for name, text in self.store.source.items():
            doc = flows.load(text) if not name.endswith(".examples") else None
            if isinstance(doc, dict):
                declared |= set(doc.get("sequences") or {})
        for spec in kinds.values():
            for a in (spec.get("attributes") or {}).values():
                ident = (a or {}).get("identifier") or {}
                if ident.get("scope"):
                    scoped.add(ident.get("sequence"))
        for seq, last, scope in raises:
            nobody = {"type": "", "legacy_key": ""}
            if seq not in declared:
                return refused(nobody, f"sequences: {seq} is no sequence the "
                                       f"flow declares")
            if last < 0 or last >= 2 ** 63 - 2:
                return refused(nobody, f"sequences: {seq}'s last value "
                                       f"{last} leaves it no next number")
            if scope and seq not in scoped:
                return refused(nobody, f"sequences: {seq} numbers in no "
                                       f"scope, not {scope}")

        for o in objects:
            tn, key = o.get("type"), o.get("legacy_key")
            for k in ("entered", "created_at", "created_by_kind",
                      "legacy_entries", "legacy_intervals", "silent_spans",
                      "occurred_at"):
                if o.get(k):
                    raise not_run(f"an import's {k}")
            qualified = ("legacy", f"{self.root(tn)}:{key}")
            if qualified in seen:
                return refused(o, "its legacy key appears twice in the batch")
            seen.add(qualified)
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
                        # a class of its own, which an empty disposition
                        # file leaves undecided (ADR-0176 decision 3)
                        return refused(o, f"its class {tn}.{k} (names "
                                          "nothing) is undecided")
                    given[k] = tuple(ids) if isinstance(v, list) else ids[0]
                else:
                    given[k] = values.decode_input(v, spec, self.runner.enums) \
                        if spec else v
            held = self.legacy.get(qualified)
            position = self.store.position
            try:
                if held is not None:
                    # an object the store holds by its legacy key is
                    # refreshed, and counted only where it changes (§4)
                    flowrun.refresh_object(self.runner, self.store, held, tn,
                                           o.get("state"), given)
                else:
                    made[qualified] = flowrun.import_object(
                        self.runner, self.store, tn, o.get("state"), given)
            except flowrun.ImportRefused as r:
                return refused(o, str(r))
            if self.store.position != position:
                report["written"] += 1
        self.legacy.update(made)
        # each sequence raised past the legacy system's last number, never
        # lowered, keyed as a mint keys it: by the scope's value, a reference
        # by its object's id (publish-and-import.md §4, ADR-0103)
        for seq, last, scope in raises:
            spec = flowrun.scope_spec(kinds, seq)
            value = None if not scope else scope if spec is None \
                or "reference" in spec \
                else values.decode_input(scope, spec, self.runner.enums)
            flowrun.raise_sequence(self.store, seq, last, value)
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

    def followed(self, oid: str) -> str:
        """Where an object's supersession chain ends: its successor's
        successor and on, the object itself where nothing succeeds it, as the
        core walks it (`DESIGN.md` §5, ADR-0028)."""
        seen = set()
        while oid not in seen:
            seen.add(oid)
            nxt = self.store.objects[oid].attrs.get("_successor") \
                if oid in self.store.objects else None
            if not isinstance(nxt, str):
                break
            oid = nxt
        return oid

    @staticmethod
    def follow(body: dict) -> bool:
        """`follow`, true or false, a fault otherwise, as the core types it."""
        follow = body.get("follow")
        if follow is not None and not isinstance(follow, bool):
            raise Fault(400, "follow takes true or false")
        return bool(follow)

    def get(self, body: dict) -> dict | None:
        self.loaded()
        oid = body.get("id")
        if not isinstance(oid, str):
            # the argument it needs, a fault and never an absent object (D570)
            raise Fault(400, "get names an object's id")
        follow = self.follow(body)
        s = self.store
        if oid not in s.objects:
            return None
        if follow:
            oid = self.followed(oid)
        o = s.objects[oid]
        t = self.runner.kinds.get(o.type) or {}
        ev = flowrun.Evaluator(s, o.type, o.id)
        attrs = {}
        try:
            for k, spec in (t.get("attributes") or {}).items():
                typ = values.type_of_spec(spec, self.runner.enums)
                v = o.attrs.get(k, flowrun.ABSENT)
                if v is flowrun.ABSENT and "opposite" in (spec or {}):
                    # an end the other side stores, by its members: a set
                    # is never null (`flow-format.md` §4.3, D552)
                    v = ev.member(o.id, k)
                attrs[k] = values.encode(v, typ)
                if (o.type, k) in HELD_AS_JSON and isinstance(attrs[k], str):
                    # a built-in attribute holding JSON, answered as the JSON
                    # it holds (ADR-0182 decision 9)
                    try:
                        attrs[k] = json.loads(attrs[k])
                    except ValueError:
                        pass
            for k, spec in (t.get("derived_attributes") or {}).items():
                expr = flowexpr.parse(spec["expression"])
                # typed as the runner types it, a bare name read on `this`
                typ = values.type_of_kind(
                    ev.kind(expr, {"this": ("object", o.type)}))
                attrs[k] = values.encode(ev.member(o.id, k), typ)
            built = {k: values.encode(ev.member(o.id, k), typ)
                     for k, typ in BUILT_IN.items()}
            if o.type == flowrun.PROPOSAL:
                attrs["last_verdict"] = self.last_verdict(oid)
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

    def last_verdict(self, oid: str) -> dict | None:
        """What a proposal's last refused approval was told, derived from
        its attempt row and never stored (ADR-0157); none where no approval
        was refused."""
        rows = [a for a in self.store.attempts if a.get("proposal") == oid]
        if not rows:
            return None
        a = rows[-1]
        return {"verdict": a["verdict"], "clause": a["clause"],
                "remedy": a["remedy"],
                "at": values.timestamp_text(a["at"])}

    def history(self, body: dict) -> dict | None:
        self.loaded()
        oid = body.get("id")
        if not isinstance(oid, str):
            raise Fault(400, "history names an object's id")
        follow = self.follow(body)
        if oid not in self.store.objects:
            return None             # no object has no history (ADR-0173)
        # with follow, every member whose chain ends where this one's does,
        # their events in position order (library-api.md §5)
        group = {oid}
        if follow:
            end = self.followed(oid)
            group = {m for m in self.store.objects if self.followed(m) == end}
        events = [self.event(e) for e in self.store.events
                  if e["object"] in group]
        return {"events": events, "not_computed": EVENT_NOT_COMPUTED}

    def availability(self, body: dict) -> list | None:
        """What each requestable transition of an object offers its actor
        now (`library-api.md` §5, ADR-0152)."""
        self.loaded()
        actor = self.who(body)
        oid = body.get("id")
        if not actor or not oid:
            raise Fault(400, "availability names its actor and an object's id")
        s = self.store
        if oid not in s.objects:
            return None             # no object offers nothing (ADR-0173)
        try:
            offers = self.runner.availability(s, actor, oid)
        except flowrun.Refused as r:
            raise Fault(404, f"no object {oid}") from r
        except flowrun.NotRunnable as e:
            raise not_run(e)
        out = []
        for xn, creates, how, refusal, schema, unevaluated, flags in offers:
            tn = creates or s.objects[oid].type
            # it may be refused for who takes it (ADR-0173 decision 1)
            dependent = creates is None and self.runner.actor_dependent(tn, xn)
            if dependent and how == "available" and not unevaluated:
                # so it is decided for who asks, where it takes no input and
                # asks no evaluator (ADR-0176 decision 2)
                refused = self.runner.decided_for(s, actor, oid, xn,
                                                  self.aliases())
                if refused is not None:
                    how, refusal, flags = "unavailable", refused, []
            out.append({
                "transition": xn, "creates": creates, "availability": how,
                "verdict": self.refusal(
                    refusal, tn, oid,
                    ((self.runner.kinds.get(tn) or {}).get("transitions")
                     or {}).get(xn) or {}, xn)
                if refusal else None,
                "inputs": schema, "unevaluated": unevaluated,
                "flags": [{"clause": c, "remedy": rem, "unknown": u,
                           "objects": list(objs)}
                          for c, rem, u, objs in flags],
                "actor_dependent": dependent,
            })
        return out

    def metric(self, body: dict) -> dict:
        """A reader's read of a metric (`library-api.md` §5), the runner's
        `read_page`: a row per group of the dimensions kept, every declared
        one unless `keep` names them, each declared flag on each declared
        group, and a refusal, a value. The page holds every group, so a
        cursor has nothing to continue from, as the core says."""
        self.loaded()
        name = body.get("name")
        if not name:
            raise Fault(400, "metric names the metric")
        if body.get("cursor") is not None:
            raise Fault(400, "a metric's page holds every group, so it has "
                             "no cursor to continue from")
        keep = body.get("keep")
        if keep is not None and not (isinstance(keep, list) and all(
                isinstance(d, str) for d in keep)):
            raise Fault(400, "keep takes dimension names")
        s = self.store
        ev = flowrun.Evaluator(s, None, stubs=flowrun.NEVER_ASKED)
        try:
            window = flowrun.seconds(body["over"]) if body.get("over") \
                else None
            page = ev.read_page(name, bind=dict(body.get("bind") or {}),
                                keep=keep, over=window,
                                filt=body.get("filter"))
            if page["refused"]:
                return {"rows": [], "cursor": None, "flags": [],
                        "history_complete": True,
                        "refused": page["refused"]}
            owner, spec, module = s.find_metric(flowrun.parsed(name))
            value_kind, dim_kinds = ev.page_kinds(
                owner, spec, module, sorted(s.metric_dimensions(owner, spec)))

            def group(dims: dict) -> dict:
                return {d: values.encode(v, values.type_of_kind(dim_kinds[d]))
                        for d, v in dims.items()}
            rows = [{"dimensions": group(r["dimensions"]),
                     "value": values.encode(
                         r["value"], values.type_of_kind(value_kind)),
                     "gaps": r["gaps"], "time_basis": r["time_basis"]}
                    for r in page["rows"]]
            flags = [{"flag": f["flag"], "group": group(f["group"]),
                      "unknown": f["unknown"]} for f in page["flags"]]
        except flowrun.NotRunnable as e:
            raise not_run(e)
        except values.Unencodable as e:
            raise Fault(500, f"cannot encode {name}'s page: {e}")
        # every object the runner holds has its whole record, so it reads no
        # gap, which the core alone computes (TODO, the metric read)
        return {"rows": rows, "cursor": None, "flags": flags,
                "history_complete": page["history_complete"],
                "refused": None,
                "not_computed": ["rows[].gaps", "history_complete"]}

    def events(self, body: dict) -> dict:
        """The event at each position given, in the order given, and null
        where the log holds none (ADR-0151)."""
        self.loaded()
        positions = body.get("positions")
        if not isinstance(positions, list) or not all(
                isinstance(p, int) and not isinstance(p, bool)
                for p in positions):
            raise Fault(400, "events takes positions, a list of integers")
        if len(set(positions)) != len(positions):
            # each position once, as the core takes it (ADR-0190 decision 6)
            raise Fault(400, "events takes each position once")
        log = {e["position"]: e for e in self.store.events}
        return {"events": [self.event(log[p]) if p in log else None
                           for p in positions],
                "not_computed": EVENT_NOT_COMPUTED}

    def tools(self, body: dict) -> dict:
        """The tool schemas an agent of a kind is handed, rendered from the
        installed version's declaration (`renderers.md` §3, ADR-0173
        decision 10), as the core's `tools` renders them."""
        self.loaded()
        kind = body.get("actor_kind")
        if kind not in flowtools.ACTOR_KINDS:
            raise Fault(400, "tools names an actor_kind: human, agent or "
                             "service")
        s = self.store
        return {"tools": flowtools.render(s.kinds, s.enums, list(self.written),
                                          s.decl.metrics, kind)}

    def declaration(self, body: dict) -> dict:
        """A type as a version declares it, the installed one where none is
        named, with the version's source reference and digest and the
        metric dimensions that name an actor (`library-api.md` §6,
        ADR-0135, D451)."""
        self.loaded()
        tn = body.get("type")
        s = self.store
        asked = body.get("version")
        if asked is not None and (not isinstance(asked, int)
                                  or isinstance(asked, bool)):
            raise Fault(400, "version takes a declaration version, an integer")
        version = s.version if asked is None else asked
        if version == s.version:
            runner, store, written_ = self.runner, s, self.written
        elif version in s.versions:
            # an earlier version, built from its source as a load builds one
            kinds, enums, declared, homes, modules = version_parts(
                flowpublish.import_order(flows, s.versions[version]["source"]))
            runner = flowrun.Runner(kinds, enums)
            store = flowrun.Store(kinds, enums, declared, homes)
            written_ = written(modules)
        else:
            raise Fault(404, f"no version {version} was published")
        record = s.versions.get(version) or {}
        t = runner.kinds.get(tn)
        if t is None:
            raise Fault(404, f"no type {tn} is declared")
        # a generated kind is the runner's own, less the keys it keeps for
        # itself, marked `_`
        out = jsonable({k: v for k, v in (
            flowpublish.change_declaration()
            if tn == flowrun.DECLARATION_CHANGE
            else written_.get(tn, t)).items() if not k.startswith("_")})
        # the members of each enumeration the type's attributes are of, which
        # its rules are read against
        used = {str((spec or {}).get("type", "")).removesuffix("[]")
                for spec in (t.get("attributes") or {}).values()}
        used |= {str((spec or {}).get("type", "")).removesuffix("[]")
                 for x in (t.get("transitions") or {}).values()
                 for spec in (x.get("inputs") or {}).values()}
        out["enumerations"] = {e: list(runner.enums[e])
                               for e in sorted(used)
                               if e in runner.enums}
        out["version"] = version
        out["source_ref"] = record.get("source_ref")
        out["source_digest"] = record.get("source_digest")
        out["person_dimensions"] = flowrun.person_dimensions(store, tn)
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
        told = result[6] if len(result) > 6 else {}
        for clause, mode in result[2]:
            if mode != "warn":
                continue            # an audit is recorded, not told
            owner, _, name = clause.rpartition(".")
            cond = ((self.runner.kinds.get(owner) or {}).get("conditions")
                    or {}).get(name) or {}
            unknown, objects = told.get(clause, (False, []))
            flags.append({"clause": name if owner == tn else clause,
                          "remedy": cond.get("remedy"), "unknown": unknown,
                          "objects": list(objects)})
        return {"kind": "satisfied", "object_id": root,
                "version": s.objects[root].version if root in s.objects
                else None,
                # by position, the order each began, which puts the parent's
                # first though the runner appends it after its effects'
                # (`library-api.md` §4)
                "events": sorted(e["position"] for e in s.events
                                 if e["position"] > before),
                "replayed": False, "flags": flags}

    def refusal(self, r, tn, oid, x, xn) -> dict:
        """A refusal as the library's verdict (`library-api.md` §4), its
        shape named by `kind` (ADR-0150)."""
        v = r.verdict
        out: dict[str, Any] = {"kind": v, "remedy": r.remedy}
        if v == "unsatisfied":
            # unknown or false, the objects its remedy points at, and
            # whether the transition takes a proposal; the runner records
            # nothing consulted (`library-api.md` §4)
            out.update(clause=r.clause, unknown=bool(r.unknown),
                       objects=list(r.objects),
                       proposable=bool((x or {}).get("proposable")),
                       not_computed=["consulted"])
        elif v == "invariant_violated":
            out["failures"] = [
                {"object_id": o, "type": t, "invariant": n, "remedy": rem}
                for o, t, n, rem in (r.failures or [(
                    r.object_id, r.object_type,
                    (r.clause or "").rpartition(".")[2], r.remedy)])]
            out["objects"] = list(r.objects)
            # whether the transition accepts a proposal, as a `taken_by`
            # refusal's does (DESIGN.md §5.5, D582)
            out["proposable"] = bool((x or {}).get("proposable"))
            if r.remedy is None:
                out["not_computed"] = ["remedy"]
        elif v == "call_refused":
            inner = r.inner
            # the call's own refusal whole, its clause qualified by the type
            # the call targets, as an expectation names it
            # (`flow-format.md` §11.4); a call is no request a caller files
            # a proposal of
            called = self.refusal(inner, tn, oid, {}, xn) if inner else {
                "kind": None, "remedy": r.remedy}
            called["clause"] = r.clause
            if "proposable" in called:
                called["proposable"] = False
            out.update(step=r.step, object_id=r.object_id, verdict=called)
            if "not_computed" in called:
                out["not_computed"] = list(called["not_computed"])
        elif v == "stale":
            # what was stale, the version the caller named (`library-api.md`
            # §4, StaleCause)
            out.update(cause="expected_version",
                       expected=getattr(r, "expected", None),
                       actual=getattr(r, "actual", None))
        elif v == "unavailable":
            out["state"] = (self.store.objects[oid].state
                            if oid in self.store.objects else None)
        elif v == "unknown_transition":
            out["transition"] = xn
        elif v == "invalid_input" and r.inputs is not None:
            out["inputs"] = list(r.inputs)
            # why each is at fault (ADR-0173 decision 9)
            out["reasons"] = dict(zip(r.inputs, r.reasons or []))
        elif v == "invalid_input":
            out.update(inputs=[], not_computed=["inputs"])
        elif v == "not_requestable":
            parents = r.parents or (x or {}).get("only_via") or []
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
                "imported": e["imported"],
                # one request at a time: none waits out another's lock
                # (ADR-0178)
                "retries": 0}


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


"""The reference runner's publishing (ADR-0166): a change to the declaration, its lifecycle, its dry run and its
approval, over the runner's Store.

Written from DESIGN.md §9 and publish-and-import.md §1 and §2, and not from the core's code, so that the core's
publishing and this one are each the other's oracle (ADR-0166 decision 1); where the design leaves a wire detail open,
such as how a change's source is held as text, it follows the core's, since that is an encoding and not a meaning.
The flow checker, a dry run's step 1, is given by the caller, the studio's sandbox or an example's run, since the
checker imports the runner.
"""

import copy
import hashlib
import json
import re

from flowrun import ABSENT, DECLARATION_CHANGE, NotRunnable, Obj, Refused, attempt_row

# each transition a request may take on a change: the states it leaves, none for the creation, and the one it enters
# (DESIGN.md §9); `supersede` is only via a publish, and `publish`, the approval, is the library's own operation
MOVES = {"draft": ((), "DRAFTED"), "revise": (("DRAFTED",), "DRAFTED"), "reopen": (("SUBMITTED",), "DRAFTED"),
         "submit": (("DRAFTED",), "SUBMITTED"), "refresh": (("SUBMITTED",), "SUBMITTED"),
         "reject": (("SUBMITTED",), "REJECTED"), "withdraw": (("DRAFTED", "SUBMITTED"), "WITHDRAWN")}
INPUTS = ("source", "source_ref", "evidence")
# a read's duration, as an evidence reference's `over` writes it (ADR-0159)
DURATION = re.compile(r"^\s*\d+\s+(s|min|h|day|days|week|weeks)\s*$")


def change_declaration():
    """The built-in change type as its rule set prints it (DESIGN.md §9): its attributes and states as the runner holds
    them, and its transitions, `supersede` only via a publish."""
    from flowrun import change_kind
    out = change_kind()
    out["description"] = "A change to the declaration, drafted, submitted and published (DESIGN.md §9)."
    out["transitions"] = {
        "draft": {"kind": "initial", "to": "DRAFTED"},
        "revise": {"kind": "internal", "from": ["DRAFTED"]},
        "reopen": {"kind": "external", "from": ["SUBMITTED"], "to": "DRAFTED"},
        "submit": {"kind": "external", "from": ["DRAFTED"], "to": "SUBMITTED"},
        "refresh": {"kind": "internal", "from": ["SUBMITTED"]},
        "publish": {"kind": "external", "from": ["SUBMITTED"], "to": "PUBLISHED"},
        "reject": {"kind": "external", "from": ["SUBMITTED"], "to": "REJECTED"},
        "withdraw": {"kind": "external", "from": ["DRAFTED", "SUBMITTED"], "to": "WITHDRAWN"},
        "supersede": {"kind": "external", "from": ["DRAFTED", "SUBMITTED"], "to": "SUPERSEDED",
                      "only_via": ["publish"]}}
    return out


def declared_name(text):
    """The name a file declares, which a change's source keys it by: its module's, or `<module>.examples` for an
    examples file; none where it declares neither, or does not parse, which the dry run reports (ADR-0155)."""
    module = re.search(r"^module:\s*([A-Za-z_][A-Za-z0-9_]*)\s*$", text, re.M)
    if module:
        return module.group(1)
    examples = re.search(r"^examples_for:\s*([A-Za-z_][A-Za-z0-9_]*)\s*$", text, re.M)
    return f"{examples.group(1)}.examples" if examples else None


def source_digest(files):
    """The digest a change records beside its source reference (ADR-0135): over its files in ascending order of
    their names' UTF-8 bytes, each as its name, a line feed, its length in bytes in decimal, a line feed and its
    bytes, as `check-flows.py --digest` computes it."""
    h = hashlib.sha256()
    for name, text in sorted(files.items(), key=lambda f: f[0].encode("utf-8")):
        body = text.encode("utf-8")
        h.update(name.encode("utf-8") + b"\n" + str(len(body)).encode("ascii") + b"\n" + body)
    return "sha256:" + h.hexdigest()


def evidence_formed(ref):
    """Whether an evidence reference is the arguments of a read it names (ADR-0159): `{metric, bind?, keep?, over?,
    filter?}`, or `{diagnostics, over}` with the window as `over`."""
    if not isinstance(ref, dict):
        return False
    if "metric" in ref:
        return (isinstance(ref["metric"], str) and bool(ref["metric"])
                and set(ref) <= {"metric", "bind", "keep", "over", "filter"}
                and isinstance(ref.get("bind", {}), dict)
                and isinstance(ref.get("keep", []), list)
                and all(isinstance(d, str) and d for d in ref.get("keep", []))
                and ("over" not in ref or (isinstance(ref["over"], str) and bool(DURATION.match(ref["over"]))))
                and ("filter" not in ref or isinstance(ref["filter"], str)))
    return (set(ref) == {"diagnostics", "over"} and isinstance(ref["diagnostics"], str) and bool(ref["diagnostics"])
            and isinstance(ref["over"], str) and bool(DURATION.match(ref["over"])))


def change_source(inputs):
    """A draft's or a revision's source, its reference, its digest and the evidence it cites, or the invalid input
    naming what is wrong (DESIGN.md §9, ADR-0135, ADR-0155 decisions 5 and 6, ADR-0159)."""
    unknown = sorted(k for k in inputs if k not in INPUTS)
    if unknown:
        raise Refused("invalid_input", remedy="self_serviceable", inputs=unknown)
    wrong = []
    files = inputs.get("source")
    # each file under the name it declares, which the digest is computed over; a file that does not parse is the
    # dry run's to report
    if not (isinstance(files, dict) and files and all(
            isinstance(t, str) and declared_name(t) in (None, n) for n, t in files.items())):
        wrong.append("source")
    reference = inputs.get("source_ref")
    if not (isinstance(reference, str) and 1 <= len(reference) <= 512
            and not any(ord(c) < 32 or 127 <= ord(c) < 160 for c in reference)):
        wrong.append("source_ref")
    evidence = inputs.get("evidence")
    if evidence is not None and not (isinstance(evidence, list) and all(evidence_formed(r) for r in evidence)):
        wrong.append("evidence")
    if wrong:
        raise Refused("invalid_input", remedy="self_serviceable", inputs=wrong)
    return files, reference, source_digest(files), evidence


def who(store, actor):
    """The actor's identity and kind, or the refusal of an actor the store does not hold (DESIGN.md §6 step 1)."""
    if actor not in store.objects or store.actor_attr(store.type_of(actor)) is None:
        raise Refused("unsatisfied", "actor_known", "dependent")
    attr, kind = store.actor_attr(store.type_of(actor))
    return store.objects[actor].attrs.get(attr, ABSENT), kind


def change_event(store, oid, xn, before, after, actor_id, actor_kind, payload):
    """A change's own event, at the next position, the change moving to its new state and version."""
    store.position += 1
    c = store.objects[oid]
    c.state = after
    c.version += 1
    store.events.append({"position": store.position, "object": oid, "type": DECLARATION_CHANGE, "transition": xn,
                         "from_state": ("state", DECLARATION_CHANGE, before) if before else ABSENT,
                         "to_state": ("state", DECLARATION_CHANGE, after), "completes": False, "returns": False,
                         "occurred_at": store.now, "recorded_at": store.now, "actor_id": actor_id,
                         "actor_kind": actor_kind, "asserted": False, "overrides": False, "imported": False,
                         "migrated": False, "redacted": False, "reason": ABSENT,
                         "declaration_version": store.version, "held": {}, "payload": payload})
    return store.position


def request(store, xn, oid, actor, inputs, flows=None, read_evidence=None):
    """A request on a change to the declaration (DESIGN.md §9): `draft` and `revise` take the source and its
    reference, and the evidence it cites; `submit` and `refresh` run the dry run with the checker given, attach its
    report and read the evidence with the reader given, refused while it finds anything fatal; `reopen`, `reject` and
    `withdraw` move it on; `supersede` is only via a publish. The result is the runner's, ("applied", the change's
    id, no flags, nothing caused), or a Refused, which the attempt log records as one row of the requested
    transition."""
    try:
        return requested(store, xn, oid, actor, inputs or {}, flows, read_evidence)
    except Refused as r:
        store.attempts.append(attempt_row(store, DECLARATION_CHANGE, oid if oid in store.objects else None, xn,
                                          actor, r))
        raise


def requested(store, xn, oid, actor, inputs, flows=None, read_evidence=None):
    identity, kind = who(store, actor)
    if oid is not None and (oid not in store.objects or store.type_of(oid) != DECLARATION_CHANGE):
        raise Refused("not_found", remedy="unreachable_from_here")
    if xn == "supersede" and oid is not None:
        raise Refused("not_requestable", remedy="unreachable_from_here", parents=["publish"])
    if xn not in MOVES or (xn == "draft") != (oid is None):
        raise Refused("unknown_transition", remedy="unreachable_from_here")
    froms, to = MOVES[xn]
    if oid is not None and store.objects[oid].state not in froms:
        raise Refused("unavailable", remedy="unreachable_from_here")
    if xn in ("submit", "refresh") and flows is None:
        raise NotRunnable("a change's dry run, with no checker given to run its step 1")
    payload = {}
    if xn in ("draft", "revise"):
        files, reference, digest, evidence = change_source(inputs)
    elif inputs:
        raise Refused("invalid_input", remedy="self_serviceable", inputs=sorted(inputs))
    if xn in ("submit", "refresh"):
        # the dry run, its report attached, refused while it finds anything fatal (DESIGN.md §9)
        report = dry_run(store, oid, flows, actor)
        if any(f["severity"] == "fatal" for f in report["findings"]):
            raise Refused("unsatisfied", "dry_run_clean", "unreachable_from_here")
        cites = json.loads(store.objects[oid].attrs.get("evidence") or "{}").get("cites")
        # each reference it cites, read as the store stands, kept as it answered (ADR-0097, ADR-0159)
        read = None
        if cites is not None:
            if read_evidence is None:
                raise NotRunnable("the evidence a change cites, with no reader given")
            read = [read_evidence(r) for r in cites]
    if oid is None:
        oid = store.new_id(DECLARATION_CHANGE)
        c = Obj(oid, DECLARATION_CHANGE, to, store.now)
        c.attrs.update({"drafted_by": identity, "drafted_by_kind": kind})
        c.created_by_kind = kind
        store.objects[oid] = c
        before = None
    else:
        before = store.objects[oid].state
    c = store.objects[oid]
    if xn in ("draft", "revise"):
        c.attrs.update({"source": json.dumps(files, sort_keys=True, separators=(",", ":"), ensure_ascii=False),
                        "source_ref": reference, "source_digest": digest, "base_version": store.version})
        # the references it cites, replaced where it gives them and kept where it does not (ADR-0159)
        if evidence is not None:
            c.attrs["evidence"] = json.dumps({"cites": evidence}, separators=(",", ":"), ensure_ascii=False)
        payload = {"source_ref": reference, "source_digest": digest}
        if evidence is not None:
            payload["evidence"] = evidence
    if xn in ("submit", "refresh"):
        c.attrs["report"] = json.dumps(report, separators=(",", ":"), ensure_ascii=False)
        if read is not None:
            c.attrs["evidence"] = json.dumps({"cites": cites, "read": read}, separators=(",", ":"), ensure_ascii=False)
    change_event(store, oid, xn, before, to, identity, kind, payload)
    for a in c.attrs:
        c.writes.setdefault(a, store.position)
    if xn in ("draft", "revise", "submit", "refresh"):
        for a in ("source", "source_ref", "source_digest", "base_version", "evidence", "report"):
            if a in c.attrs:
                c.writes[a] = store.position
    return ("applied", oid, [], [])


# ── the dry run (publish-and-import.md §1 steps 1 to 4, §2) ────────────────────────────────────────────────────────────

# what the report does not compute, as the core's names it (ADR-0155 decision 5)
NOT_COMPUTED = ["invariants", "sweepable", "unsweepable", "queryable_derives", "worst_case_fanout", "observed_fanout",
                "replaced_creations", "creations_past_first_state", "observing_clauses", "flags", "person_dimensions"]
# the store's own types, which no change declares and no mapping reaches
BUILT_IN = ("Operator", "Proposal", DECLARATION_CHANGE, "label")


class Version:
    """A version a source declares, as the runner reads it: its types, the observation kinds among them, its
    enumerations, its metrics and their modules, its modules, its migration and its source, each file by name."""

    def __init__(self, flows, files):
        import flowrun
        modules, library, observations = {}, {}, set()
        for _name, text in import_order(flows, files):
            doc = flows.load(text)
            if isinstance(doc, dict) and "examples_for" in doc:
                continue
            flows.literals(doc)
            modules[doc["module"]] = doc
            view, _back = flows.view(doc, library)
            library.update(dict(flows.types(view)))
        kinds = dict(library)
        obs = flowrun.observation_kinds(modules)
        kinds.update(obs)
        observations |= set(obs)
        kinds["Operator"] = {"attributes": {"identity": {"type": "identity", "actor_kind": "human"}},
                             "states": {"ACTIVE": {"category": "live"},
                                        "RETIRED": {"category": "closed", "final": True}}}
        self.kinds, self.observations, self.modules = kinds, observations, modules
        self.enums = {n: list(v) for m in modules.values() for n, v in (m.get("enumerations") or {}).items()}
        self.declared, self.homes = flowrun.declarations(modules)
        self.source = dict(files)
        # the mappings each module carries, from the version before it (flow-format.md §4.16)
        self.migration = {"renamed_attributes": {}, "removed_members": {}, "removed_states": {}, "backfill": {},
                          "admit": []}
        for m in modules.values():
            mig = m.get("migration") or {}
            for k in ("renamed_attributes", "removed_members", "removed_states", "backfill"):
                for owner, maps in (mig.get(k) or {}).items():
                    self.migration[k].setdefault(owner, {}).update(maps or {})
            self.migration["admit"] += list(mig.get("admit") or [])


def import_order(flows, files):
    """A source's files as the checker reads them: each module after those it imports, then the examples files."""
    docs, examples = {}, []
    for name, text in sorted(files.items()):
        try:
            doc = flows.load(text)
        except Exception:
            doc = None
        if isinstance(doc, dict) and "examples_for" in doc:
            examples.append((name, text))
        else:
            docs[name] = (text, doc)
    order, seen = [], set()

    def visit(name, path=()):
        if name in seen or name not in docs or name in path:
            return
        doc = docs[name][1]
        for mod in ((doc or {}).get("imports") or {}) if isinstance(doc, dict) else ():
            visit(mod, path + (name,))
        seen.add(name)
        order.append((name, docs[name][0]))
    for name in docs:
        visit(name)
    return order + examples


class Found:
    """Findings gathered by code, in the order first met, each with the ids it names (publish-and-import.md §2)."""

    def __init__(self):
        self.findings = []

    def add(self, severity, code, message, oid=None):
        f = next((f for f in self.findings if f["code"] == code), None)
        if f is None:
            f = {"severity": severity, "code": code, "message": message, "ids": []}
            self.findings.append(f)
        if oid is not None and oid not in f["ids"]:
            f["ids"].append(oid)

    def fatal(self):
        return any(f["severity"] == "fatal" for f in self.findings)


def dry_run(store, change_id, flows, actor):
    """A change's dry run over the store as it stands, at its `now`, and its report as the wire and a change's
    `report` carry it: step 1 by the checker given and the examples, then steps 2 and 3 over every object, writing
    nothing (publish-and-import.md §1, §2)."""
    return simulate(store, change_id, flows, actor)[0]


def simulate(store, change_id, flows, actor):
    """The dry run, and what an approval would install from it: the store as the mappings leave it, under the new
    version, and the version, none where the source does not load; the same code path, so what a dry run reports is
    what a publish does (publish-and-import.md §1)."""
    c = store.objects[change_id]
    files = json.loads(c.attrs.get("source") or "{}")
    found = Found()
    examples, analysed = {}, {}
    ordered = import_order(flows, files)
    errs, notes = flows.check(ordered, dict(store.source), analysed, examples)
    for f, line, _sev, code, message in errs:
        # an example's failure by the name the design gives it (ADR-0136, publish-and-import.md §1 step 1)
        found.add("fatal", "example_failed" if code == "example" else f"check:{code}", f"{f}:{line}: {message}")
    for f, line, _sev, code, message in notes:
        if code == "notrun":
            # an example not run has not passed, and nothing unchecked is installed (§1 step 1)
            found.add("fatal", "example_not_run", f"{f}:{line}: {message}")
        elif code == "imports":
            # a version's source is the whole version, so an import from a module it does not carry is refused
            # (ADR-0161)
            found.add("fatal", "check:imports", f"{f}:{line}: {message}; a version carries every module it imports")
        else:
            found.add("notice", f"check:{code}", f"{f}:{line}: {message}")
    refused = found.fatal()
    proposals_invalidated, installed = 0, None
    try:
        new = Version(flows, files)
    except Exception as e:                  # a source the checker refused, which the version cannot be read from
        found.add("fatal", "load", f"the source does not load as a version: {e}")
        new = None
    if new is not None:
        try:
            proposals_invalidated, installed = impact(store, new, found, actor, change_id)
        except NotRunnable as e:
            if not refused:
                raise
            # the dry run reports everything, except where the rules the checks refuse cannot run over the objects
            found.add("notice", "not_computed",
                      f"the impact past the checks' fatal findings, whose rules cannot be run over the objects: {e}")
    accepted = not found.fatal() and all(
        f["severity"] != "decision" or admitted(new, f["code"][len("invariant:"):])
        for f in found.findings)
    return {
        "version": store.version + 1, "accepted": accepted, "now": timestamp_text(store.now),
        "findings": [{"severity": f["severity"], "code": f["code"], "message": f["message"],
                      "affected": len(f["ids"]), "sample": f["ids"][:5]} for f in found.findings],
        "change_id": change_id, "source_ref": c.attrs.get("source_ref"),
        "source_digest": c.attrs.get("source_digest"), "proposals_invalidated": proposals_invalidated,
        "affected": {f["code"]: list(f["ids"]) for f in found.findings if f["ids"]},
        "examples": examples, "analysed": {t: list(n) for t, n in analysed.items()},
        "not_computed": NOT_COMPUTED,
    }, installed


def admitted(new, qualified):
    """Whether the change's `admit` covers an invariant, by its name on the type or a type it extends."""
    if new is None:
        return False
    tn, _, n = qualified.rpartition(".")
    for a in new.migration["admit"]:
        at, _, an = str(a.get("invariant", "")).rpartition(".")
        if an == n and is_a(new.kinds, tn, at):
            return True
    return False


def is_a(kinds, tn, want):
    seen = set()
    while tn and tn not in seen:
        if tn == want:
            return True
        seen.add(tn)
        tn = (kinds.get(tn) or {}).get("extends")
    return False


def declared_as(spec):
    """What an attribute holds, for comparing two versions of it: its type or its target, and whether it is a set."""
    spec = spec or {}
    t = spec.get("reference") or str(spec.get("type", ""))
    return t.removesuffix("[]"), t.endswith("[]")


def impact(store, new, found, actor, change_id):
    """Steps 2 and 3 (publish-and-import.md §1): every object as the mappings leave it, read under the new version,
    into the report's categories; returns how many pending proposals the version invalidates."""
    import copy

    import flowrun
    old_kinds = store.kinds
    kinds = dict(new.kinds)
    # a type the version drops is retired: its objects stay readable, and nothing creates or changes one
    retired = set()
    for tn, t in old_kinds.items():
        if tn not in kinds and tn not in BUILT_IN:
            kinds[tn] = {**copy.deepcopy(t), "transitions": {}}
            retired.add(tn)
    sim = copy.deepcopy(store)
    sim.kinds, sim.enums = flowrun.with_labels(kinds), new.enums
    sim.decl = flowrun.Shared(metrics=new.declared, homes=new.homes)
    sim.version = store.version + 1
    runner = flowrun.Runner(kinds, new.enums)
    m = new.migration
    old_enums = store.enums
    for oid, o in sim.objects.items():
        if o.type in BUILT_IN or o.type in retired or o.type not in kinds:
            continue
        before = store.objects[oid]
        nt, ot = kinds[o.type], old_kinds.get(o.type) or {}
        # renames first, which write nothing of their own and carry the attribute's history
        for tn, maps in m["renamed_attributes"].items():
            if is_a(kinds, o.type, tn):
                for frm, to in maps.items():
                    if frm in o.attrs:
                        o.attrs[to] = o.attrs.pop(frm)
                    if frm in o.spans:
                        o.spans[to] = o.spans.pop(frm)
                    if frm in o.writes:
                        o.writes[to] = o.writes.pop(frm)     # the write index follows, which an erasure reads
        # an attribute whose type changed starts absent, its values set aside with their history
        for a, spec in (ot.get("attributes") or {}).items():
            now = (nt.get("attributes") or {}).get(a)
            if now is not None and declared_as(now) != declared_as(spec):
                o.attrs.pop(a, None)
                o.spans.pop(a, None)
        # an observation is born final, and no migration reaches one
        if o.type in new.observations:
            continue
        # then removed enumeration members, a set keeping the target once
        for en, maps in m["removed_members"].items():
            for frm, to in maps.items():
                was, target = ("enum", en, frm), ("enum", en, to)
                changed = False
                for a, spec in (nt.get("attributes") or {}).items():
                    if declared_as(spec)[0] != en or a not in o.attrs:
                        continue
                    v = o.attrs[a]
                    if flowrun.is_set(v) and was in v:
                        out = []
                        for i in v:
                            i = target if i == was else i
                            if i not in out:
                                out.append(i)
                        o.attrs[a], changed = tuple(out), True
                    elif v == was:
                        o.attrs[a], changed = target, True
                if changed:
                    found.add("notice", f"mapping:removed_members:{en}.{frm}", f"objects holding {en}.{frm} take {en}.{to}",
                              oid)
        # then removed states
        if o.state in (ot.get("states") or {}) and o.state not in (nt.get("states") or {}):
            to = next((maps[o.state] for tn, maps in m["removed_states"].items()
                       if is_a(kinds, o.type, tn) and o.state in maps), None)
            if to is not None:
                found.add("notice", f"mapping:removed_states:{o.type}.{o.state}", f"objects in {o.type}.{o.state} move to {to}",
                          oid)
                o.state = to
            else:
                found.add("fatal", f"removed_state:{o.type}.{o.state}",
                          f"objects are in {o.type}.{o.state}, which the version removes and no mapping moves", oid)
    # then backfills, in the order written, each reading the object as the mappings before it left it, an erased
    # object's personal values skipped
    for tn, maps in m["backfill"].items():
        for attr, text in maps.items():
            for oid, o in sim.objects.items():
                if o.type in BUILT_IN or o.type in retired or o.type in new.observations or not is_a(kinds, o.type, tn):
                    continue
                spec = (kinds[o.type].get("attributes") or {}).get(attr) or {}
                if o.erased and spec.get("personal"):
                    continue
                v = flowrun.Evaluator(sim, o.type, oid).value(flowrun.parsed(str(text)), {})
                if o.attrs.get(attr, flowrun.ABSENT) != v:
                    found.add("notice", f"mapping:backfill:{tn}.{attr}", f"the backfill of {tn}.{attr} writes these objects",
                              oid)
                    if v is flowrun.ABSENT:
                        o.attrs.pop(attr, None)
                    else:
                        o.attrs[attr] = v
    removed = [(en, mem) for en, members in old_enums.items() for mem in members
               if mem not in (new.enums.get(en) or [])]
    for oid, o in sim.objects.items():
        if o.type in BUILT_IN or o.type in retired or o.type not in kinds:
            continue
        t = kinds[o.type]
        for a, spec in (t.get("attributes") or {}).items():
            v = o.attrs.get(a, flowrun.ABSENT)
            held = v if flowrun.is_set(v) else (v,)
            for en, mem in removed:
                if ("enum", en, mem) in held:
                    found.add("fatal", f"removed_member:{en}.{mem}",
                              f"objects hold {en}.{mem}, which the version removes and no mapping replaces", oid)
            # a required value, new or made required, left absent; a reference's derived end holds no value
            required = (not spec.get("optional") and "default" not in spec and not declared_as(spec)[1]
                        and spec.get("type") != "counter" and ("reference" not in spec or stored(kinds, spec)))
            if required and v is flowrun.ABSENT:
                found.add("fatal", f"required_absent:{o.type}.{a}",
                          f"{o.type}.{a} is required and these objects have none, nor a backfill to give it", oid)
        # a state no transition or assertion of the version reaches, a mirror's excepted, which the import writes
        if not t.get("mirror") and o.state in (t.get("states") or {}) and not reachable(t, o.state):
            found.add("fatal", f"unreachable_state:{o.type}.{o.state}",
                      f"objects are in {o.type}.{o.state}, which no transition or assertion of the version reaches", oid)
        # parts a mapping into a final state would strand: a mapping drives no cascade, and a `survives` list names
        # transitions, not migrations
        moved = store.objects[oid].state != o.state
        if moved and sim.final(o):
            for a, spec in (t.get("attributes") or {}).items():
                if (spec or {}).get("aggregation") != "composite" or (spec or {}).get("survives") is True:
                    continue
                parts = flowrun.Evaluator(sim, o.type, oid).member(oid, a)
                parts = () if parts is flowrun.ABSENT else (parts if isinstance(parts, tuple) else (parts,))
                if any(p in sim.objects and not sim.final(sim.objects[p]) for p in parts):
                    found.add("fatal", f"stranded:{o.type}.{a}",
                              f"a mapping moves these into a final state and would strand their live {a}", oid)
        # every invariant, once, after the mappings: a violation no standing admission covers is a decision, and a
        # standing admission whose invariant holds, or which the version no longer declares, is discharged (D531)
        failing = runner.failing_invariants(sim, o)
        for n in sorted(o.admissions):
            if n not in failing:
                found.add("notice", f"discharged:{o.type}.{n}",
                          f"these objects hold an admitted violation of {o.type}.{n} that the version cures", oid)
        for n in failing:
            if n in o.admissions:
                continue
            if o.type in new.observations:
                # a kind's new invariant applies to later recordings, and nothing can change these (ADR-0146)
                found.add("notice", f"observation_invariant:{o.type}.{n}",
                          f"observations already recorded violate {o.type}.{n}, which applies to later recordings", oid)
                continue
            q = f"{o.type}.{n}"
            covered = admitted(new, q)
            found.add("decision", f"invariant:{q}",
                      f"these objects would violate {q}, which the change admits" if covered
                      else f"these objects would violate {q}: fix them first, backfill, or admit it", oid)
            if covered and sim.final(o):
                found.add("notice", f"admit_final:{q}",
                          "an admission on objects in a final state, which no transition can discharge", oid)
    clauses_and_offers(store, sim, kinds, new.enums, actor, found)
    invalidated = proposals(store, kinds, found)
    # every other open change, which the publish supersedes
    for oid, o in store.objects.items():
        if o.type == DECLARATION_CHANGE and oid != change_id and o.state in ("DRAFTED", "SUBMITTED"):
            found.add("notice", "superseded", "open changes the publish supersedes", oid)
    attribute_notices(old_kinds, kinds, retired, m, found)
    return invalidated, (sim, new, retired)


def stored(kinds, spec):
    """Whether a singular reference is the end that holds the value: one naming no opposite, whose opposite is a set,
    or marked stored, as the runner's tracked members read it."""
    back = ((kinds.get(spec.get("reference")) or {}).get("attributes") or {}).get(spec.get("opposite") or "") or {}
    return "opposite" not in spec or back.get("reference", "").endswith("[]") or bool(spec.get("stored"))


def timestamp_text(seconds):
    """An instant as the wire writes it, in UTC to the second (ADR-0150 decision 2)."""
    import datetime
    t = datetime.datetime(1970, 1, 1, tzinfo=datetime.timezone.utc) + datetime.timedelta(seconds=int(seconds))
    return t.strftime("%Y-%m-%dT%H:%M:%SZ")


def reachable(t, state):
    """Whether some transition or assertion of the type enters a state."""
    for x in (t.get("transitions") or {}).values():
        to = (x or {}).get("to")
        if state == to or (isinstance(to, list) and state in to):
            return True
    return False


def offer_classes(runner, store, actor, oid, kinds):
    """Each transition an object is offered, classed as `availability` classes it, no evaluator asked and no metric
    consulted (§1 step 3): a refusal is one whatever it left unevaluated, and an offer otherwise waits on its inputs or
    on a `deny` clause it could not evaluate, which alone could refuse it, and is neither available nor not (D538)."""
    import flowrun
    runner.unasked = flowrun.DRY_RUN
    out = {}
    t = kinds.get(store.objects[oid].type) or {}
    for xn, creates, how, _r, _schema, unevaluated, _flags in runner.availability(store, actor, oid):
        if creates is not None:
            continue
        guards = ((t.get("transitions") or {}).get(xn) or {}).get("guards") or {}
        waits = how == "available_with_input" or any(guards.get(g) == "deny" for g in unevaluated)
        out[xn] = "waits" if how != "unavailable" and waits else how
    return out


def clauses_and_offers(store, sim, kinds, enums, actor, found):
    """The clauses the version newly enforces that would refuse an object, and the transitions each object gains or
    loses, judged as the dry run may: a clause or a verdict waiting on an input, an evaluator or a metric is neither
    (publish-and-import.md §1 step 3)."""
    import flowrun
    old_runner, new_runner = flowrun.Runner(store.kinds, store.enums), flowrun.Runner(kinds, enums)
    for oid, o in sim.objects.items():
        if o.type in BUILT_IN or oid not in store.objects or o.type not in kinds or not (kinds[o.type].get("transitions")):
            continue
        nt, ot = kinds[o.type], store.kinds.get(o.type) or {}
        for xn, x in (nt.get("transitions") or {}).items():
            if x.get("kind") == "initial" or not takes_from(nt, x, o.state):
                continue
            was = (ot.get("transitions") or {}).get(xn) or {}
            for g, mode in (x.get("guards") or {}).items():
                if mode != "deny" or (was.get("guards") or {}).get(g) == "deny":
                    continue
                c = (nt.get("conditions") or {}).get(g)
                if c is None:
                    continue
                expr = flowrun.parsed(c["expression"])
                if "inputs" in flowrun.names_read(expr):
                    continue
                asking = flowrun.Evaluator(sim, o.type, oid, {}, None, flowrun.DRY_RUN)
                ok = flowrun.truth(asking.value(expr, {}))
                if ok is not True and not getattr(asking, "unanswered", False):
                    found.add("notice", f"newly_enforced:{o.type}.{xn}.{g}",
                              f"{o.type}.{xn} newly denies on {g}, which these objects fail", oid)
        before = offer_classes(old_runner, store, actor, oid, store.kinds)
        after = offer_classes(new_runner, sim, actor, oid, kinds)
        for xn in sorted(set(before) | set(after)):
            b, a = before.get(xn), after.get(xn)
            if a == "available" and b in (None, "unavailable"):
                found.add("notice", f"gained:{o.type}.{xn}", f"these objects gain {o.type}.{xn}", oid)
            if b == "available" and a in (None, "unavailable"):
                found.add("notice", f"lost:{o.type}.{xn}", f"these objects lose {o.type}.{xn}", oid)


def takes_from(t, x, state):
    """Whether a transition leaves a state: its `from`, `any` being every state that is not final."""
    froms = x.get("from")
    froms = [froms] if isinstance(froms, str) else list(froms or [])
    if froms == ["any"]:
        st = (t.get("states") or {}).get(state) or {}
        return not st.get("final") and (x.get("kind") != "external" or x.get("to") != state)
    return state in froms


def proposals(store, kinds, found):
    """The pending proposals the version invalidates: its transition removed or no longer proposable, an input it no
    longer takes, or a value its type no longer holds (DESIGN.md §9); returns how many."""
    n = 0
    for pid, p in store.proposals.items():
        if p.get("state") != "pending":
            continue
        t = kinds.get(p.get("type")) or {}
        x = (t.get("transitions") or {}).get(p.get("transition"))
        breaks = x is None or not x.get("proposable")
        if not breaks:
            taken = set(x.get("required_inputs") or []) | set(x.get("optional_inputs") or []) | set(x.get("inputs") or {})
            breaks = any(k not in taken for k in (p.get("inputs") or {}))
        if breaks:
            n += 1
            found.add("notice", "proposals_invalidated", "pending proposals the version invalidates", pid)
    return n


def attribute_notices(old, new, retired, m, found):
    """Notices of the declaration alone: an attribute dropped where one of its type is added, which may be a rename,
    and an attribute whose type changed, a drop and an add of the same name."""
    for tn, nt in new.items():
        ot = old.get(tn)
        if ot is None or tn in BUILT_IN or tn in retired:
            continue
        renamed = set((m["renamed_attributes"].get(tn) or {}))
        nattrs, oattrs = nt.get("attributes") or {}, ot.get("attributes") or {}
        for a, spec in oattrs.items():
            now = nattrs.get(a)
            if now is not None and declared_as(now) != declared_as(spec):
                found.add("notice", f"type_changed:{tn}.{a}", f"{tn}.{a} changes its type, a drop and an add of the same name")
            elif now is None and a not in renamed and any(
                    n not in oattrs and declared_as(x) == declared_as(spec) for n, x in nattrs.items()):
                found.add("notice", f"possible_rename:{tn}.{a}",
                          f"{tn}.{a} is dropped where an attribute of its type is added, which may be a rename")


# ── the approval (publish-and-import.md §1 step 5) ───────────────────────────────────────────────────────────────────

def approve(store, change_id, actor, expected_version, flows):
    """The approval of a submitted change, which installs its version, or the refusal of it, with the report it
    recomputed at the attached report's `now`: refused `stale` where the approver read another version of the change,
    `current` where it was drafted against a version no longer installed, `fatal` where the recomputed report finds
    anything fatal, `impact_unchanged` where it names an object or proposal in a category the attached one did not,
    and `decision_unresolved` while a decision stands unadmitted (DESIGN.md §9, ADR-0101, ADR-0146, ADR-0147). Returns
    (the verdict, the report or none), the verdict ("applied", the change's id, the positions of its events) or a
    Refused."""
    import copy

    from flowrun import timestamp
    identity, kind = who(store, actor)
    c = store.objects[change_id]
    if c.state != "SUBMITTED":
        raise Refused("unavailable", remedy="unreachable_from_here")
    if expected_version != c.version:
        r = Refused("stale", remedy="self_serviceable")
        r.expected, r.actual = expected_version, c.version
        raise r
    if c.attrs.get("base_version") != store.version:
        return Refused("unsatisfied", "current", "unreachable_from_here"), None
    attached = json.loads(c.attrs.get("report") or "{}")
    at = copy.deepcopy(store)
    if attached.get("now"):
        at.now = timestamp(attached["now"])
    report, installed = simulate(at, change_id, flows, actor)
    if any(f["severity"] == "fatal" for f in report["findings"]) or installed is None:
        return Refused("unsatisfied", "fatal", "unreachable_from_here"), report
    shown = attached.get("affected") or {}
    for category, ids in report["affected"].items():
        # the changes it supersedes are reported and not compared (ADR-0147)
        if category != "superseded" and any(i not in (shown.get(category) or []) for i in ids):
            return Refused("unsatisfied", "impact_unchanged", "unreachable_from_here"), report
    if not report["accepted"]:
        return Refused("unsatisfied", "decision_unresolved", "unreachable_from_here"), report
    return ("applied", change_id, install(store, installed, report, change_id, identity, kind)), report


def install(store, installed, report, change_id, identity, kind):
    """The version installed, as §1 step 5 says, in the store's own order of events: the publish event on the change;
    one migration per object a mapping, an admit or a discharge reaches, in creation order, caused by the publish, the
    version before it its `declaration_version`, renames writing none; the intervals of a member newly tracked opened,
    at the migration where a backfill writes it and else at the publish event, and of one no longer tracked closed at
    the publish event; then each other open change superseded and each pending proposal the version breaks invalidated,
    each caused by the publish; an attribute whose type changed left with no value and no interval, the simulation
    having dropped both, so the attribute of the new type, where tracked, opens its own as a newly tracked member does
    (D528), the runner keeping none of the intervals the core sets aside, which no read reaches; and on
    each object already erased, an attribute the version newly marks personal redacted, its intervals included, by a
    `redact` event, which admits every invariant the absence leaves violated (D527). Returns the positions of its events, the publish's
    first."""
    import flowrun
    sim, new, retired = installed
    old_version, new_version, old_kinds = store.version, store.version + 1, store.kinds
    affected = report["affected"]
    positions = []

    def event(oid, tn, xn, before, after, version, payload, cause=None):
        store.position += 1
        store.events.append({"position": store.position, "object": oid, "type": tn, "transition": xn,
                             "from_state": ("state", tn, before) if before else ABSENT,
                             "to_state": ("state", tn, after), "completes": False, "returns": False,
                             "occurred_at": store.now, "recorded_at": store.now, "actor_id": identity,
                             "actor_kind": kind, "asserted": False, "overrides": False, "imported": False,
                             "migrated": xn == "migrate", "redacted": False, "reason": ABSENT,
                             "declaration_version": version, "held": {}, "payload": payload, "cause": cause})
        positions.append(store.position)
        return store.position

    c = store.objects[change_id]
    c.attrs.update({"approved_by": identity, "approved_by_kind": kind})
    c.writes.update({"approved_by": store.position + 1, "approved_by_kind": store.position + 1})
    publish = event(change_id, DECLARATION_CHANGE, "publish", c.state, "PUBLISHED", new_version,
                    {"version": new_version})
    c.state, c.version = "PUBLISHED", c.version + 1
    kinds = sim.kinds

    def of(prefix, oid):
        return [code for code, ids in affected.items() if code.startswith(prefix) and oid in ids]

    for oid, x in sim.objects.items():
        if oid not in store.objects or x.type in BUILT_IN or x.type in retired or x.type not in kinds:
            continue
        was = store.objects[oid]
        codes = of("mapping:", oid)
        admits = [code.rpartition(".")[2] for code in of("invariant:", oid)
                  if admitted(new, code[len("invariant:"):])]
        discharges = [code.rpartition(".")[2] for code in of("discharged:", oid)]
        migrates = bool(codes or admits or discharges)
        position = store.position + 1 if migrates else publish
        tracked_now = flowrun.tracked_members(kinds, sim.enums, x.type)
        # a member no longer tracked closes at the publish event
        for d, spans in x.spans.items():
            if d not in tracked_now and spans and spans[-1]["left_at"] is ABSENT:
                spans[-1]["left_at"] = store.now
        value = lambda d: ("state", x.type, x.state) if d == "state" else x.attrs.get(d, ABSENT)
        for d in tracked_now:
            spans = x.spans.setdefault(d, [])
            open_span = spans[-1] if spans and spans[-1]["left_at"] is ABSENT else None
            if open_span is None and not spans:
                # a member newly tracked opens where a backfill writes it, and else at the publish event
                backfilled = any(c_.startswith("mapping:backfill:") and c_.endswith(f".{d}") for c_ in codes)
                at = position if migrates and backfilled else publish
                spans.append(flowrun.span_row(at, opened(x, d, value(d), store.now, kind, new_version)))
            elif open_span is not None and migrates and open_span.get("state" if d == "state" else "value") != value(d):
                # a value the migration changes, its interval closed and the next opened by it
                open_span["left_at"] = store.now
                spans.append(flowrun.span_row(position, opened(x, d, value(d), store.now, kind, new_version)))
        if migrates:
            for n in admits:
                x.admissions.add(n)
            for n in discharges:
                x.admissions.discard(n)
            event(oid, x.type, "migrate", was.state, x.state, old_version,
                  {"mappings": codes, "admits": admits, "discharges": discharges}, publish)
            x.version += 1
        store.objects[oid] = x
    # on each object already erased, what the version newly marks personal redacted, the absence admitting every
    # invariant it leaves violated, as the erasure it completes would have (DESIGN.md §8, D527)
    judge = flowrun.Runner(kinds, sim.enums)
    renamed = {(tn, to): frm for tn, maps in new.migration["renamed_attributes"].items()
               for frm, to in (maps or {}).items()}
    for oid, x in sorted(sim.objects.items(), key=lambda kv: kv[1].id_order):
        if not x.erased or oid not in store.objects or x.type in retired or x.type not in kinds:
            continue
        old_attrs = (old_kinds.get(x.type) or {}).get("attributes") or {}
        redacted = []
        for a, spec in ((kinds[x.type].get("attributes") or {}).items()):
            before = old_attrs.get(renamed.get((x.type, a), a)) or {}
            if (spec or {}).get("personal") and not before.get("personal") and x.attrs.get(a, ABSENT) not in (ABSENT, ()):
                if flowrun.is_set_attribute(spec):
                    x.attrs[a] = ()     # a personal set is emptied, a set being never absent (ADR-0148)
                else:
                    del x.attrs[a]
                for row in x.spans.get(a, ()):
                    row["value"] = ABSENT   # its intervals too, as the erasure it completes clears them (§9 step 9)
                redacted.append(a)
        if not redacted:
            continue
        admitted_now = [n for n in judge.failing_invariants(sim, x) if n not in x.admissions]
        x.admissions.update(admitted_now)
        event(oid, x.type, "redact", x.state, x.state, new_version, {"redacted": redacted}, publish)
        store.events[-1].update({"redacted": True, "overrides": bool(admitted_now)})
        x.version += 1
    # the other open changes, superseded, and the pending proposals the version breaks, invalidated
    for oid in affected.get("superseded") or []:
        o = store.objects[oid]
        event(oid, DECLARATION_CHANGE, "supersede", o.state, "SUPERSEDED", new_version, {}, publish)
        o.state, o.version = "SUPERSEDED", o.version + 1
    for pid in affected.get("proposals_invalidated") or []:
        p = store.objects[pid]
        event(pid, p.type, "invalidate", p.state, "INVALIDATED", new_version, {}, publish)
        p.state, p.version = "INVALIDATED", p.version + 1
        store.proposals[pid]["state"] = "invalidated"
    store.kinds, store.enums = kinds, sim.enums
    store.decl, store.version, store.source = sim.decl, new_version, dict(new.source)
    # each version a store held before, by number, and the renames each version made, which an erasure reads what
    # was personal through (ADR-0146 decision 2)
    store.history = {**getattr(store, "history", {}), old_version: old_kinds}
    store.renames = {**getattr(store, "renames", {}), new_version: [
        (tn, frm, to) for tn, maps in new.migration["renamed_attributes"].items() for frm, to in (maps or {}).items()]}
    return positions


def opened(o, d, v, now, kind, version):
    """The fields of an interval a publish opens, marked as a migration's (ADR-0146)."""
    return {"object": o.id, ("state" if d == "state" else "value"): v, "entered_at": now, "left_at": ABSENT,
            "entered_by_kind": kind, "declaration_version": version, "legacy": False, "migrated": True, "held": {}}

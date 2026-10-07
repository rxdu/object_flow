"""The reference runner's publishing (ADR-0166): a change to the declaration, its lifecycle, its dry run and its
approval, over the runner's Store.

Written from DESIGN.md §9 and publish-and-import.md §1 and §2, and not from the core's code, so that the core's
publishing and this one are each the other's oracle (ADR-0166 decision 1); where the design leaves a wire detail open,
such as how a change's source is held as text, it follows the core's, since that is an encoding and not a meaning.
The flow checker, a dry run's step 1, is given by the caller, the studio's sandbox or an example's run, since the
checker imports the runner.
"""

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


def request(store, xn, oid, actor, inputs):
    """A request on a change to the declaration (DESIGN.md §9): `draft` and `revise` take the source and its
    reference, and the evidence it cites; `reopen`, `reject` and `withdraw` move it on; `supersede` is only via a
    publish. The result is the runner's, ("applied", the change's id, no flags, nothing caused), or a Refused,
    which the attempt log records as one row of the requested transition."""
    try:
        return requested(store, xn, oid, actor, inputs or {})
    except Refused as r:
        store.attempts.append(attempt_row(store, DECLARATION_CHANGE, oid if oid in store.objects else None, xn,
                                          actor, r))
        raise


def requested(store, xn, oid, actor, inputs):
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
    if xn in ("submit", "refresh"):
        raise NotRunnable("a change's dry run, which the runner's next slice runs (ADR-0166 decisions 4 and 5)")
    payload = {}
    if xn in ("draft", "revise"):
        files, reference, digest, evidence = change_source(inputs)
    elif inputs:
        raise Refused("invalid_input", remedy="self_serviceable", inputs=sorted(inputs))
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
    change_event(store, oid, xn, before, to, identity, kind, payload)
    for a in c.attrs:
        c.writes.setdefault(a, store.position)
    if xn in ("draft", "revise"):
        for a in ("source", "source_ref", "source_digest", "base_version", "evidence"):
            if a in c.attrs:
                c.writes[a] = store.position
    return ("applied", oid, [], [])

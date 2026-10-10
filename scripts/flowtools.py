"""The tool schemas an agent is handed, rendered from the declaration (`docs/design/renderers.md` §3, ADR-0173
decision 10): the reference for the core's `tools` operation, which the Flow Studio's server answers with it on the
runner's side, so the two are compared answer for answer.

`render(kinds, enums, declared, metrics, actor_kind)` gives every tool, sorted by name: one for each transition of a
declared type a request may name, one for each observation kind, `read_metric`, and the reads an agent plans with,
`get_object`, `object_availability` and `query_objects` (D564). A transition is left out for an
actor kind where an invariant of its type, over a value its effect assigns from `actor.kind`, is certain to fail with
that kind (decision 1).
"""

import re

import flowexpr
from flowrun import MINOR_UNITS

ACTOR_KINDS = ("human", "agent", "service")
# the standard metrics every type has, and those each assignee reference adds (declaration-syntax.md §6.11)
STANDARD = ("time_in_state", "time_in_state_p80", "throughput", "work_in_progress", "oldest_open", "transition_counts",
            "refusals", "flags_raised", "refusal_rate", "override_counts", "rework")
ASSIGNMENT = ("open_work", "time_unassigned", "time_to_first_assignment", "time_with_assignee",
              "cycle_time_by_assignee", "time_in_state_by_holder", "handoffs", "reassigned_back",
              "acted_by_non_assignee", "handoffs_by_object", "returns_by_object")
AVAILABILITY = ("Call object_availability first: it evaluates the real guards against the real data, and says "
                "whether this transition is available now, which rule refuses it if not, and what can be done about it.")


def snake(name):
    """`PurchaseRequest` as `purchase_request`."""
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def schema_of(spec, enums):
    """An input's JSON Schema, in the wire's notation (ADR-0150 decision 2)."""
    spec = spec or {}
    if "reference" in spec:
        target = spec["reference"]
        one = {"type": "string", "description": f"the {target.rstrip('[]')}, by its id"}
        if target.endswith("[]"):
            return {"type": "array", "items": {"type": "string"}, "uniqueItems": True,
                    "description": f"the {target[:-2]} objects, by their ids"}
        return one
    typ = str(spec.get("type", "string"))
    if typ.endswith("[]"):
        return {"type": "array", "items": schema_of(dict(spec, type=typ[:-2], unit=None), enums), "uniqueItems": True}
    if typ in enums:
        out = {"type": "string", "enum": list(enums[typ])}
    elif typ == "actor_kind":
        out = {"type": "string", "enum": list(ACTOR_KINDS)}
    elif typ == "int":
        out = {"type": "integer"}
    elif typ == "bool":
        out = {"type": "boolean"}
    elif typ.startswith("decimal"):
        scale = re.fullmatch(r"decimal\(\s*\d+\s*,\s*(\d+)\s*\)", typ)
        places = f"{{1,{scale.group(1)}}}" if scale and scale.group(1) != "0" else None
        out = {"type": "string", "pattern": "^-?[0-9]+" + (f"(\\.[0-9]{places})?" if places else "") + "$"}
    elif typ.startswith("money"):
        # no more places than the currency's minor unit (D571)
        currency = typ[len("money("):-1]
        minor = MINOR_UNITS.get(currency, 2)
        fraction = f"(\\.[0-9]{{1,{minor}}})?" if minor else ""
        out = {"type": "string", "pattern": f"^{currency} -?[0-9]+{fraction}$"}
    elif typ == "timestamp":
        out = {"type": "string", "format": "date-time"}
    elif typ == "date":
        out = {"type": "string", "format": "date"}
    elif typ == "duration":
        out = {"type": "string", "pattern": "^[0-9]+ (s|min|h|day|days|week|weeks)$"}
    elif typ == "file":
        # a file by its reference, never a name (ADR-0173 decision 11)
        out = {"type": "object", "properties": {
            "hash": {"type": "string", "pattern": "^sha256:[0-9a-f]{64}$"},
            "size": {"type": "integer", "minimum": 0},
            "media_type": {"type": "string", "pattern": "^[A-Za-z0-9!#$&^_.+-]+/[A-Za-z0-9!#$&^_.+-]+$"}},
            "required": ["hash", "media_type", "size"], "additionalProperties": False}
    else:
        out = {"type": "string"}
    if spec.get("unit"):
        out["description"] = f"in {spec['unit']}"
    return out


def inputs_schema(t, x, enums):
    """A transition's inputs, each an attribute it writes or an input it declares, and those a request must give."""
    attrs = t.get("attributes") or {}
    props, required = {}, []
    for name in list(x.get("required_inputs") or []) + list(x.get("optional_inputs") or []):
        props[name] = schema_of(attrs.get(name), enums)
    required += list(x.get("required_inputs") or [])
    for name, spec in (x.get("inputs") or {}).items():
        props[name] = schema_of(spec, enums)
        if not (spec or {}).get("optional") and "default" not in (spec or {}):
            required.append(name)
    out = {"type": "object", "properties": props, "additionalProperties": False}
    if required:
        out["required"] = sorted(set(required))
    return out


# ── for an actor kind: what an invariant over the recorded kind is certain of ──────────────────────────────────────
KNOWN, PRESENT, ABSENT = "known", "present", "absent"


def certain(e, env):
    """True, False or None, unknown: an invariant over what a request writes, each attribute (KNOWN, kind),
    PRESENT, ABSENT or not in env, unknown, in three-valued logic."""
    k = e[0]
    if k == "bin" and e[1] in ("and", "or"):
        a, b = certain(e[2], env), certain(e[3], env)
        if e[1] == "and":
            return False if False in (a, b) else (True if a is True and b is True else None)
        return True if True in (a, b) else (False if a is False and b is False else None)
    if k == "not":
        v = certain(e[1], env)
        return None if v is None else not v
    if k == "isnull" and e[1][0] == "name":
        held = env.get(e[1][1])
        if held is None:
            return None
        present = held[0] != ABSENT
        return present if e[2] else not present
    if k == "bin" and e[1] in ("==", "!="):
        sides = (e[2], e[3])
        for left, right in (sides, sides[::-1]):
            if left[0] == "name" and right[0] == "name" and right[1] in ACTOR_KINDS and right[1] not in env:
                held = env.get(left[1])
                if held is None or held[0] == PRESENT:
                    return None
                same = held[0] == KNOWN and held[1] == right[1]
                return same if e[1] == "==" else not same
    return None


def refuses(t, x, kind):
    """Whether an invariant of the type, reading a value the transition assigns from `actor.kind`, is certain to
    fail when an actor of that kind takes it (ADR-0173 decisions 1 and 10)."""
    env = {}
    for name in x.get("required_inputs") or []:
        env[name] = (PRESENT,)
    kinded = set()
    for st in x.get("effect") or []:
        if "assign" in st:
            location, expr = st["assign"]["location"], " ".join(str(st["assign"]["expr"]).split())
            if expr == "actor.kind":
                env[location] = (KNOWN, kind)
                kinded.add(location)
            elif expr == "now" or re.fullmatch(r'-?\d+(\.\d+)?|"[^"]*"|[A-Z][A-Za-z0-9]*\.[A-Z][A-Z0-9_]*', expr):
                env[location] = (PRESENT,)
            else:
                env.pop(location, None)
        elif "clear" in st:
            names = st["clear"] if isinstance(st["clear"], list) else [st["clear"]]
            for name in names:
                env[name] = (ABSENT,)
    if not kinded:
        return False
    for spec in (t.get("invariants") or {}).values():
        tree = flowexpr.parse((spec or {}).get("expression", ""))
        if flowexpr_names(tree) & kinded and certain(tree, env) is False:
            return True
    return False


def flowexpr_names(e):
    """The bare names an expression reads."""
    if not isinstance(e, tuple):
        return set()
    if e[0] == "name":
        return {e[1]}
    out = set()
    for part in e[1:]:
        if isinstance(part, tuple):
            out |= flowexpr_names(part)
        elif isinstance(part, list):
            for p in part:
                out |= flowexpr_names(p)
    return out


# ── the tools ───────────────────────────────────────────────────────────────────────────────────────────────────
def transition_tool(tn, xn, t, x, enums):
    creation = x.get("kind") == "initial"
    props = {} if creation else {"object_id": {"type": "string", "description": f"the {tn}, by its id"},
                                 "expected_version": {"type": "integer",
                                                      "description": "the version read, so a change since is refused as stale"}}
    props["idempotency_key"] = {"type": "string", "description": "the same key on a retry answers the first result"}
    if x.get("backdating_limit"):
        props["occurred_at"] = {"type": "string", "format": "date-time",
                                "description": "when it happened, if earlier than now; how far back is a rule the store checks"}
    if x.get("proposable"):
        props["propose"] = {"type": "boolean",
                            "description": "file the request as a proposal a person approves, instead of taking it"}
    props["inputs"] = inputs_schema(t, x, enums)
    lead = f"Creates a new {tn} by {xn}." if creation else f"Takes {tn}.{xn}."
    own = (x.get("description") or "").strip()
    text = " ".join(p for p in (own or lead, "" if creation else AVAILABILITY) if p)
    return {"name": f"{snake(tn)}_{xn}", "description": text,
            "input_schema": {"type": "object", "properties": props,
                             "required": ["inputs"] if creation else ["inputs", "object_id"],
                             "additionalProperties": False}}


def observation_tool(tn, ob, enums):
    fields = ob.get("attributes") or {}
    props = {"subject": {"type": "string", "description": f"the {tn}, by its id"}}
    props.update({f: schema_of(spec, enums) for f, spec in fields.items()})
    props["corrects"] = {"type": "string", "description": f"the {ob['kind']} on this {tn} that this one replaces"}
    required = sorted({"subject"} | {f for f, spec in fields.items() if not (spec or {}).get("optional")})
    own = (ob.get("description") or "").strip()
    return {"name": f"record_{snake(ob['kind'])}",
            "description": " ".join(p for p in (own or f"Records a new {ob['kind']} about its subject.",
                                                "availability(subject) offers recording it beside the subject's own "
                                                "transitions, and says whether it is available now.") if p),
            "input_schema": {"type": "object", "properties": {
                "idempotency_key": {"type": "string",
                                    "description": "the same key on a retry answers the first result"},
                "occurred_at": {"type": "string", "format": "date-time",
                                "description": "when it happened, if earlier than now; how far back is a rule the store checks"},
                "inputs": {"type": "object", "properties": props, "required": required,
                           "additionalProperties": False}},
                "required": ["inputs"], "additionalProperties": False}}


def metric_tool(names):
    return {"name": "read_metric",
            "description": "Read a declared or standard metric, as rows of its dimensions and value and the flags that "
                           "hold, in UTC calendar time. The definitions are in declaration(type); this tool does not "
                           "restate them.",
            "input_schema": {"type": "object", "properties": {
                "name": {"type": "string", "enum": names},
                "bind": {"type": "object", "description": "dimension values to fix"},
                "keep": {"type": "array", "items": {"type": "string"},
                         "description": "the dimensions to group by; all by default, and any left out is aggregated over"},
                "over": {"type": "string", "description": "a window back from now, such as 30 days"},
                "filter": {"type": "string"}},
                "required": ["name"], "additionalProperties": False}}


def read_tools(declared, kinds):
    """The reads an agent plans with before it acts (D564): an object, what it offers the agent now, and the objects
    of a type a filter selects."""
    types = [tn for tn in declared if not (kinds.get(tn) or {}).get("abstract")]
    return [
        {"name": "get_object",
         "description": "Read an object: its type, state, attributes, derived ones included, and version, which a "
                        "later request names as expected_version.",
         "input_schema": {"type": "object", "properties": {
             "id": {"type": "string", "description": "the object's id"}},
             "required": ["id"], "additionalProperties": False}},
        {"name": "object_availability",
         "description": "What an object offers you now: each transition available, available with input or "
                        "unavailable, with the rule that refuses it and what can be done, and its inputs' types.",
         "input_schema": {"type": "object", "properties": {
             "object_id": {"type": "string", "description": "the object's id"}},
             "required": ["object_id"], "additionalProperties": False}},
        {"name": "query_objects",
         "description": "Find objects of a type: those a filter in the expression language selects, such as "
                        "state == <Type>.<STATE> or a member compared with a value, in the order named.",
         "input_schema": {"type": "object", "properties": {
             "type": {"type": "string", "enum": types},
             "filter": {"type": "string"},
             "order": {"type": "string", "description": "a member to order by, then desc to descend"},
             "fields": {"type": "array", "items": {"type": "string"},
                        "description": "the members to answer, every one by default"}},
             "required": ["type"], "additionalProperties": False}},
    ]


def render(kinds, enums, declared, metrics, actor_kind):
    """Every tool for an actor of the kind, sorted by name (renderers.md §3, ADR-0173 decision 10). `declared` is
    the types the modules declare, in order; `metrics` each module's metrics by name, `(owner or None, spec)`."""
    tools = []
    for tn in declared:
        t = kinds.get(tn) or {}
        for xn, x in (t.get("transitions") or {}).items():
            if x.get("only_via") or refuses(t, x, actor_kind):
                continue
            tools.append(transition_tool(tn, xn, t, x, enums))
        for ob in (t.get("observations") or {}).values():
            tools.append(observation_tool(tn, ob, enums))
    names = set()
    for ms in metrics.values():
        for name, (owner, _spec) in ms.items():
            names.add(f"{owner}.{name}" if owner else name)
    for tn in declared:
        t = kinds.get(tn) or {}
        if t.get("abstract"):
            continue
        names |= {f"{tn}.{m}" for m in STANDARD}
        for r, spec in (t.get("attributes") or {}).items():
            if (spec or {}).get("assignee"):
                names |= {f"{tn}.{m}.{r}" for m in ASSIGNMENT}
    tools.append(metric_tool(sorted(names)))
    tools += read_tools(declared, kinds)
    return sorted(tools, key=lambda tool: tool["name"])

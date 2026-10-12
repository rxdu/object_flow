//! The tool schemas an agent is handed, rendered from the declaration
//! (`renderers.md` §3, ADR-0173 decision 10): one for each transition of a
//! declared type a request may name, one for each observation kind,
//! `read_metric`, and the reads an agent plans with, `get_object`,
//! `object_availability` and `query_objects` (D564), sorted by name. A transition is left out for an actor kind
//! where an invariant of its type, over a value its effect assigns from
//! `actor.kind`, is certain to fail with that kind (decision 1), and for an
//! agent wherever its `taken_by` does not name `agent` (ADR-0189 decision 1);
//! either is offered for proposing where it may be proposed.
//!
//! `scripts/flowtools.py` renders the same, the reference the Flow Studio's
//! server answers with on the runner's side, so the two are compared answer
//! for answer.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value as Json, json};

use crate::expr::{BinOp, Expr};
use crate::model::{Declared, Kind, Library, Step, Transition, Type};

/// The kinds of actor a tool is rendered for.
pub const ACTOR_KINDS: [&str; 3] = ["human", "agent", "service"];

/// The standard metrics every type has, and those each assignee reference
/// adds (`declaration-syntax.md` §6.11).
const STANDARD: [&str; 11] = [
    "time_in_state",
    "time_in_state_p80",
    "throughput",
    "work_in_progress",
    "oldest_open",
    "transition_counts",
    "refusals",
    "flags_raised",
    "refusal_rate",
    "override_counts",
    "rework",
];
const ASSIGNMENT: [&str; 11] = [
    "open_work",
    "time_unassigned",
    "time_to_first_assignment",
    "time_with_assignee",
    "cycle_time_by_assignee",
    "time_in_state_by_holder",
    "handoffs",
    "reassigned_back",
    "acted_by_non_assignee",
    "handoffs_by_object",
    "returns_by_object",
];
const AVAILABILITY: &str = "Call object_availability first: it evaluates the real guards against the real data, and says whether this transition is available now, which rule refuses it if not, and what can be done about it.";
/// A proposable transition a rule over who acted refuses this kind of actor:
/// it may only propose it (ADR-0176 decision 1).
const PROPOSING: &str =
    "You may only propose this: a person approves the proposal, which takes it as theirs.";
const RETRY: &str = "the same key on a retry answers the first result";
const BACKDATED: &str =
    "when it happened, if earlier than now; how far back is a rule the store checks";

/// `PurchaseRequest` as `purchase_request`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// A scalar type's JSON Schema, in the wire's notation (ADR-0150 decision 2).
fn scalar(typ: &str, enums: &BTreeMap<String, Vec<String>>) -> Map<String, Json> {
    let mut out = Map::new();
    out.insert("type".into(), json!("string"));
    if let Some(members) = enums.get(typ) {
        out.insert("enum".into(), json!(members));
    } else if typ == "actor_kind" {
        out.insert("enum".into(), json!(ACTOR_KINDS));
    } else if typ == "int" {
        out.insert("type".into(), json!("integer"));
    } else if typ == "bool" {
        out.insert("type".into(), json!("boolean"));
    } else if let Some(args) = typ.strip_prefix("decimal") {
        let scale = args
            .trim_start_matches('(')
            .trim_end_matches(')')
            .split(',')
            .nth(1)
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != "0");
        let fraction = scale.map_or(String::new(), |s| format!("(\\.[0-9]{{1,{s}}})?"));
        out.insert("pattern".into(), json!(format!("^-?[0-9]+{fraction}$")));
    } else if let Some(currency) = typ.strip_prefix("money(").and_then(|c| c.strip_suffix(')')) {
        // no more places than the currency's minor unit (D571)
        let minor = crate::value::minor_unit(currency).unwrap_or(2);
        let fraction = if minor == 0 {
            String::new()
        } else {
            format!("(\\.[0-9]{{1,{minor}}})?")
        };
        out.insert(
            "pattern".into(),
            json!(format!("^{currency} -?[0-9]+{fraction}$")),
        );
    } else if typ == "timestamp" {
        out.insert("format".into(), json!("date-time"));
    } else if typ == "date" {
        out.insert("format".into(), json!("date"));
    } else if typ == "duration" {
        out.insert(
            "pattern".into(),
            json!("^[0-9]+ (s|min|h|day|days|week|weeks)$"),
        );
    } else if typ == "file" {
        // a file by its reference, never a name (ADR-0173 decision 11)
        out.insert("type".into(), json!("object"));
        out.insert(
            "properties".into(),
            json!({
                "hash": {"type": "string", "pattern": "^sha256:[0-9a-f]{64}$"},
                "size": {"type": "integer", "minimum": 0},
                "media_type": {"type": "string", "pattern": "^[A-Za-z0-9!#$&^_.+-]+/[A-Za-z0-9!#$&^_.+-]+$"},
            }),
        );
        out.insert("required".into(), json!(["hash", "media_type", "size"]));
        out.insert("additionalProperties".into(), json!(false));
    }
    out
}

/// An input's schema from what it declares: a reference or a type, one or
/// a set, and the unit an observation's field is measured in.
fn schema_of(
    declared: &Declared,
    many: bool,
    unit: Option<&str>,
    enums: &BTreeMap<String, Vec<String>>,
) -> Json {
    match declared {
        Declared::Reference(target) if many => json!({
            "type": "array", "items": {"type": "string"}, "uniqueItems": true,
            "description": format!("the {target} objects, by their ids"),
        }),
        Declared::Reference(target) => {
            json!({"type": "string", "description": format!("the {target}, by its id")})
        }
        Declared::Scalar(typ) => {
            let one = Json::Object(scalar(typ, enums));
            let mut out = if many {
                json!({"type": "array", "items": one, "uniqueItems": true})
            } else {
                one
            };
            if let (Some(u), Some(o)) = (unit, out.as_object_mut())
                && !many
            {
                o.insert("description".into(), json!(format!("in {u}")));
            }
            out
        }
    }
}

/// An input written as the flow format writes it, an observation's field.
fn schema_of_source(spec: &Json, enums: &BTreeMap<String, Vec<String>>) -> Json {
    let (text, reference) = match (spec.get("reference"), spec.get("type")) {
        (Some(r), _) => (r.as_str().unwrap_or_default(), true),
        (None, Some(t)) => (t.as_str().unwrap_or_default(), false),
        (None, None) => ("string", false),
    };
    let many = text.ends_with("[]");
    let base = text.trim_end_matches("[]").to_string();
    let declared = if reference {
        Declared::Reference(base)
    } else {
        Declared::Scalar(base)
    };
    schema_of(
        &declared,
        many,
        spec.get("unit").and_then(Json::as_str),
        enums,
    )
}

/// A transition's inputs, each an attribute it writes or an input it
/// declares, and those a request must give.
fn inputs_schema(t: &Type, x: &Transition, enums: &BTreeMap<String, Vec<String>>) -> Json {
    let mut props = Map::new();
    let mut required: BTreeSet<String> = x.required_inputs.iter().cloned().collect();
    for name in x.required_inputs.iter().chain(&x.optional_inputs) {
        let schema = t.attributes.iter().find(|(a, _)| a == name).map_or_else(
            || json!({"type": "string"}),
            |(_, a)| schema_of(&a.declared, a.many, None, enums),
        );
        props.insert(name.clone(), schema);
    }
    for (name, i) in &x.inputs {
        props.insert(name.clone(), schema_of(&i.declared, i.many, None, enums));
        if !i.optional && i.default.is_none() {
            required.insert(name.clone());
        }
    }
    let mut out = json!({"type": "object", "properties": props, "additionalProperties": false});
    if !required.is_empty() {
        out["required"] = json!(required);
    }
    out
}

// ── for an actor kind: what an invariant over the recorded kind is certain of
#[derive(Clone, Debug, PartialEq)]
enum Held {
    Known(String),
    Present,
    Absent,
}

/// True, false or none, unknown: an invariant over what a request writes,
/// in three-valued logic.
fn certain(e: &Expr, env: &BTreeMap<String, Held>) -> Option<bool> {
    match e {
        Expr::Bin(op @ (BinOp::And | BinOp::Or), a, b) => {
            let (a, b) = (certain(a, env), certain(b, env));
            if *op == BinOp::And {
                if a == Some(false) || b == Some(false) {
                    Some(false)
                } else if a == Some(true) && b == Some(true) {
                    Some(true)
                } else {
                    None
                }
            } else if a == Some(true) || b == Some(true) {
                Some(true)
            } else if a == Some(false) && b == Some(false) {
                Some(false)
            } else {
                None
            }
        }
        Expr::Not(a) => certain(a, env).map(|v| !v),
        Expr::IsNull(inner, negated) => {
            let Expr::Name(n) = inner.as_ref() else {
                return None;
            };
            let present = *env.get(n)? != Held::Absent;
            Some(if *negated { present } else { !present })
        }
        Expr::Bin(op @ (BinOp::Eq | BinOp::Ne), a, b) => {
            for (left, right) in [(a, b), (b, a)] {
                if let (Expr::Name(l), Expr::Name(r)) = (left.as_ref(), right.as_ref())
                    && ACTOR_KINDS.contains(&r.as_str())
                    && !env.contains_key(r)
                {
                    let same = match env.get(l)? {
                        Held::Present => return None,
                        Held::Known(k) => k == r,
                        Held::Absent => false,
                    };
                    return Some(if *op == BinOp::Eq { same } else { !same });
                }
            }
            None
        }
        _ => None,
    }
}

/// The bare names an expression reads.
fn names(e: &Expr, out: &mut BTreeSet<String>) {
    if let Expr::Name(n) = e {
        out.insert(n.clone());
    }
    for c in e.children() {
        names(c, out);
    }
}

/// Whether an assigned expression is a value present whatever the request:
/// `now`, a number, a text or an enumeration's member.
fn constant(e: &Expr) -> bool {
    match e {
        Expr::Name(n) => n == "now",
        Expr::Num(_) | Expr::Str(_) => true,
        Expr::Path(head, member) => {
            matches!(head.as_ref(), Expr::Name(t)
            if t.starts_with(|c: char| c.is_ascii_uppercase()))
                && member.starts_with(|c: char| c.is_ascii_uppercase())
        }
        _ => false,
    }
}

/// Whether an invariant of the type, reading a value the transition assigns
/// from `actor.kind`, is certain to fail when an actor of that kind takes it
/// (ADR-0173 decisions 1 and 10).
fn refuses(t: &Type, x: &Transition, kind: &str) -> bool {
    let mut env: BTreeMap<String, Held> = BTreeMap::new();
    for name in &x.required_inputs {
        env.insert(name.clone(), Held::Present);
    }
    let mut kinded = BTreeSet::new();
    for st in &x.effect {
        match st {
            Step::Assign { location, expr } => {
                if matches!(expr, Expr::Path(head, m)
                    if m == "kind" && matches!(head.as_ref(), Expr::Name(a) if a == "actor"))
                {
                    env.insert(location.clone(), Held::Known(kind.to_string()));
                    kinded.insert(location.clone());
                } else if constant(expr) {
                    env.insert(location.clone(), Held::Present);
                } else {
                    env.remove(location);
                }
            }
            Step::Clear(cleared) => {
                for n in cleared {
                    env.insert(n.clone(), Held::Absent);
                }
            }
            _ => {}
        }
    }
    if kinded.is_empty() {
        return false;
    }
    t.invariants.iter().any(|(_, e)| {
        let mut read = BTreeSet::new();
        names(e, &mut read);
        !read.is_disjoint(&kinded) && certain(e, &env) == Some(false)
    })
}

// ── the tools ────────────────────────────────────────────────────────────
fn transition_tool(
    tn: &str,
    xn: &str,
    t: &Type,
    x: &Transition,
    enums: &BTreeMap<String, Vec<String>>,
    proposing: bool,
) -> Json {
    let creation = x.kind == Kind::Initial;
    let mut props = Map::new();
    if !creation {
        props.insert(
            "object_id".into(),
            json!({"type": "string", "description": format!("the {tn}, by its id")}),
        );
        props.insert(
            "expected_version".into(),
            json!({"type": "integer", "description": "the version read, so a change since is refused as stale"}),
        );
    }
    props.insert(
        "idempotency_key".into(),
        json!({"type": "string", "description": RETRY}),
    );
    if x.backdating_limit.is_some() {
        props.insert(
            "occurred_at".into(),
            json!({"type": "string", "format": "date-time", "description": BACKDATED}),
        );
    }
    if proposing {
        props.insert(
            "propose".into(),
            json!({"const": true, "description": "file the request as a proposal a person approves"}),
        );
    } else if x.proposable {
        props.insert(
            "propose".into(),
            json!({"type": "boolean", "description": "file the request as a proposal a person approves, instead of taking it"}),
        );
    }
    props.insert("inputs".into(), inputs_schema(t, x, enums));
    let own = x.description.as_deref().map(str::trim).unwrap_or_default();
    let lead = if !own.is_empty() {
        own.to_string()
    } else if creation {
        format!("Creates a new {tn} by {xn}.")
    } else {
        format!("Takes {tn}.{xn}.")
    };
    let mut description = lead;
    if proposing {
        description = format!("{description} {PROPOSING}");
    }
    if !creation {
        description = format!("{description} {AVAILABILITY}");
    }
    let mut required = if creation {
        vec!["inputs"]
    } else {
        vec!["inputs", "object_id"]
    };
    if proposing {
        required.push("propose");
    }
    json!({"name": format!("{}_{xn}", snake(tn)), "description": description,
           "input_schema": {"type": "object", "properties": props, "required": required,
                            "additionalProperties": false}})
}

fn observation_tool(tn: &str, ob: &Json, enums: &BTreeMap<String, Vec<String>>) -> Json {
    let kind = ob.get("kind").and_then(Json::as_str).unwrap_or_default();
    let mut props = Map::new();
    props.insert(
        "subject".into(),
        json!({"type": "string", "description": format!("the {tn}, by its id")}),
    );
    let mut required: BTreeSet<String> = BTreeSet::from(["subject".to_string()]);
    if let Some(fields) = ob.get("attributes").and_then(Json::as_object) {
        for (f, spec) in fields {
            props.insert(f.clone(), schema_of_source(spec, enums));
            if spec.get("optional").and_then(Json::as_bool) != Some(true) {
                required.insert(f.clone());
            }
        }
    }
    props.insert(
        "corrects".into(),
        json!({"type": "string", "description": format!("the {kind} on this {tn} that this one replaces")}),
    );
    let own = ob
        .get("description")
        .and_then(Json::as_str)
        .map(str::trim)
        .unwrap_or_default();
    let lead = if own.is_empty() {
        format!("Records a new {kind} about its subject.")
    } else {
        own.to_string()
    };
    json!({"name": format!("record_{}", snake(kind)),
           "description": format!("{lead} availability(subject) offers recording it beside the subject's own transitions, and says whether it is available now."),
           "input_schema": {"type": "object", "properties": {
               "idempotency_key": {"type": "string", "description": RETRY},
               "occurred_at": {"type": "string", "format": "date-time", "description": BACKDATED},
               "inputs": {"type": "object", "properties": props, "required": required,
                          "additionalProperties": false}},
               "required": ["inputs"], "additionalProperties": false}})
}

fn metric_tool(names: &BTreeSet<String>) -> Json {
    json!({"name": "read_metric",
           "description": "Read a declared or standard metric, as rows of its dimensions and value and the flags that hold, in UTC calendar time. The definitions are in declaration(type); this tool does not restate them.",
           "input_schema": {"type": "object", "properties": {
               "name": {"type": "string", "enum": names},
               "bind": {"type": "object", "description": "dimension values to fix"},
               "keep": {"type": "array", "items": {"type": "string"},
                        "description": "the dimensions to group by; all by default, and any left out is aggregated over"},
               "over": {"type": "string", "description": "a window back from now, such as 30 days"},
               "filter": {"type": "string"}},
               "required": ["name"], "additionalProperties": false}})
}

/// The reads an agent plans with before it acts (D564): an object, what it
/// offers the agent now, and the objects of a type a filter selects.
fn read_tools(lib: &Library) -> Vec<Json> {
    let types: Vec<&String> = lib
        .declared
        .iter()
        .filter(|tn| lib.types.get(*tn).is_some_and(|t| !t.abstract_))
        .collect();
    vec![
        json!({"name": "get_object",
               "description": "Read an object: its type, state, attributes, derived ones included, and version, which a later request names as expected_version.",
               "input_schema": {"type": "object", "properties": {
                   "id": {"type": "string", "description": "the object's id"}},
                   "required": ["id"], "additionalProperties": false}}),
        json!({"name": "object_availability",
               "description": "What an object offers you now: each transition available, available with input or unavailable, with the rule that refuses it and what can be done, and its inputs' types.",
               "input_schema": {"type": "object", "properties": {
                   "object_id": {"type": "string", "description": "the object's id"}},
                   "required": ["object_id"], "additionalProperties": false}}),
        json!({"name": "query_objects",
               "description": "Find objects of a type: those a filter in the expression language selects, such as state == <Type>.<STATE> or a member compared with a value, in the order named.",
               "input_schema": {"type": "object", "properties": {
                   "type": {"type": "string", "enum": types},
                   "filter": {"type": "string"},
                   "order": {"type": "string", "description": "a member to order by, then desc to descend"},
                   "fields": {"type": "array", "items": {"type": "string"},
                              "description": "the members to answer, every one by default"},
                   "cursor": {"type": "string",
                              "description": "the cursor a page answered, to read the page after it (D583)"},
                   "limit": {"type": "integer", "minimum": 1, "maximum": crate::reads::PAGE_LIMIT,
                             "description": format!("the most objects a page answers, {} by default", crate::reads::PAGE_DEFAULT)}},
                   "required": ["type"], "additionalProperties": false}}),
    ]
}

/// Every tool for an actor of the kind, sorted by name.
pub fn tools(lib: &Library, actor_kind: &str) -> Vec<Json> {
    let mut out = Vec::new();
    for tn in &lib.declared {
        let Some(t) = lib.types.get(tn) else { continue };
        for (xn, x) in &t.transitions {
            if !x.only_via.is_empty() {
                continue;
            }
            // an agent is offered what the flow names it for, a transition
            // whose taken_by names agent, and any other only for proposing,
            // where it may be proposed (ADR-0189 decision 1)
            let unnamed = actor_kind == "agent" && !x.taken_by.iter().any(|k| k == "agent");
            if unnamed || refuses(t, x, actor_kind) {
                // refused, it is offered for proposing where it may be
                // proposed (ADR-0176 decision 1)
                if x.proposable {
                    out.push(transition_tool(tn, xn, t, x, &lib.enumerations, true));
                }
                continue;
            }
            out.push(transition_tool(tn, xn, t, x, &lib.enumerations, false));
        }
        if let Some(obs) = t.source.get("observations").and_then(Json::as_object) {
            for ob in obs.values() {
                out.push(observation_tool(tn, ob, &lib.enumerations));
            }
        }
    }
    let mut names = BTreeSet::new();
    for metrics in lib.metrics.values() {
        for (name, (owner, _)) in metrics {
            names.insert(
                owner
                    .as_ref()
                    .map_or_else(|| name.clone(), |o| format!("{o}.{name}")),
            );
        }
    }
    for tn in &lib.declared {
        let Some(t) = lib.types.get(tn) else { continue };
        if t.abstract_ {
            continue;
        }
        names.extend(STANDARD.iter().map(|m| format!("{tn}.{m}")));
        for (r, a) in &t.attributes {
            if a.assignee {
                names.extend(ASSIGNMENT.iter().map(|m| format!("{tn}.{m}.{r}")));
            }
        }
    }
    out.push(metric_tool(&names));
    out.extend(read_tools(lib));
    out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    out
}

//! The names and relationships of a module (`check-flows.py`'s
//! `name_errors` and every check it calls), over the view of the module.
//! Each function keeps the name of the one it ports, so the two can be read
//! side by side.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::doc::{
    ACTOR_KINDS, Found, KEYWORDS, Obj, Path, as_obj, bare_upper, dotted_calls, ext, finding, flag,
    has, held_of, keys, kind_text, list, members_of, obj, optional, py_str, re, read_through,
    reads, segs, squash, str_or, strings, text,
};
use super::optional::{Env, OptionalReads};
use super::raw::{BUILT_IN_TYPES, imported};
use super::steps::{
    Step, assigns, body_text, cleared, input_reads, kind, required_input_names, sources,
    step_expressions, step_kind, taken, targets, walk,
};
use super::view::{TAKEN, bases, inherited, types};
use crate::expr::{BinOp, Expr, Number, parse};
use crate::yaml::Seg;

/// Every vocabulary has `closed` (`declaration-syntax.md` §1).
const BUILT_IN_CATEGORIES: &[&str] = &["closed"];
const NOT_A_CATEGORY: &[&str] = &["any", "terminal", "superseding"];
const NOT_A_TRANSITION: &[&str] = &["any", "migrate", "import", "redact"];
/// Names an expression resolves before an attribute (§9.2).
pub const NOT_AN_ATTRIBUTE: &[&str] = &[
    "state",
    "inputs",
    "actor",
    "this",
    "now",
    "referrers",
    "this_event",
];
/// The members every object has, which no type may declare: the
/// evaluator's own list, so the two cannot drift (issue #22, ADR-0216).
pub use crate::eval::OBJECT_MEMBERS;
/// A member withdrawn before 1.0.0, its name kept against shadowing so that
/// it may return with a use (ADR-0210 decision 3): a mirror's `imported_at`,
/// which no store kept.
const WITHDRAWN_MEMBERS: &[&str] = &["imported_at"];
const ONLY_ON_OBSERVATIONS: &[&str] = &["unit"];
const ONLY_ON_TYPES: &[&str] = &[
    "actor_kind",
    "assignee",
    "unique",
    "indexed",
    "identifier",
    "external",
    "default",
    "opposite",
    "stored",
    "aggregation",
    "cascade",
    "survives",
];
const GENERATED_GUARDS: &[&str] = &[
    "actor_known",
    "actor_active",
    "occurred_within",
    "whole_open",
    "not_erased",
    "subject_open",
    "corrects_current",
    "subject_owned",
    "successor_not_superseded",
    "value_in_range",
    "proposal_pending",
];
const SIMPLE_TYPES: &[&str] = &[
    "string",
    "int",
    "bool",
    "timestamp",
    "duration",
    "identity",
    "file",
    "date",
    "actor_kind",
];
const REQUEST_ONLY: &[&str] = &["inputs", "this_event"];
const OBSERVATION_MEMBERS: &[&str] = &[
    "now",
    "subject",
    "corrects",
    "occurred_at",
    "recorded_at",
    "recorded_by_kind",
    "declaration_version",
];
const BINDER_RESERVED: &[&str] = &[
    "state",
    "inputs",
    "actor",
    "this",
    "now",
    "referrers",
    "this_event",
    "id",
    "open",
    "created_at",
    "created_by_kind",
    "recorded_from",
    "declaration_version",
];
const OBJECT_ROW: &[&str] = &[
    "id",
    "state",
    "open",
    "created_at",
    "created_by_kind",
    "recorded_from",
    "declaration_version",
    "entered_at",
    "time_in",
    "intervals",
    "transitions",
    "attempts",
];
const KIND_ROW: &[&str] = &[
    "subject",
    "corrects",
    "occurred_at",
    "recorded_at",
    "recorded_by_kind",
    "declaration_version",
];
const PIECE_MEMBERS: &[&str] = &["period", "piece_start", "piece_end"];
const TRANSITION_ROWS: &[&str] = &["transitions", "attempts", "attempt_counts"];
const STANDARD_DIMENSIONS: &[&str] = &["version", "actor_kind"];

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| s.to_string()).collect()
}

fn dataset_rows(source: &str) -> Option<BTreeSet<String>> {
    let names: &[&str] = match source {
        "intervals" => &[
            "object",
            "state",
            "entered_at",
            "left_at",
            "duration",
            "entered_by_kind",
            "declaration_version",
            "legacy",
            "held",
            "migrated",
            "corrected_by",
        ],
        "transitions" => &[
            "object",
            "transition",
            "from_state",
            "to_state",
            "completes",
            "returns",
            "occurred_at",
            "recorded_at",
            "actor_id",
            "actor_kind",
            "asserted",
            "overrides",
            "imported",
            "migrated",
            "redacted",
            "reason",
            "declaration_version",
            "held",
        ],
        "attempts" => &[
            "object",
            "transition",
            "verdict",
            "clause",
            "remedy",
            "unknown",
            "enforced",
            "flagged",
            "actor_id",
            "actor_kind",
            "at",
            "declaration_version",
        ],
        "attempt_counts" => &[
            "day",
            "object",
            "transition",
            "verdict",
            "clause",
            "remedy",
            "actor_kind",
            "enforced",
            "flagged",
            "declaration_version",
            "count",
        ],
        "labels" => &[
            "subject",
            "name",
            "subject_state",
            "occurred_at",
            "recorded_at",
            "recorded_by_kind",
            "declaration_version",
        ],
        _ => return None,
    };
    Some(set(names))
}

static INTERVALS_OF: LazyLock<Regex> = LazyLock::new(|| re(r"^intervals\(([a-z][a-z0-9_]*)\)$"));
static QUALIFIED: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b([A-Z][A-Za-z0-9]*)\.([A-Z][A-Z0-9_]*)\b"));
/// A string literal, as the expression lexer reads one.
static STRING_LITERAL: LazyLock<Regex> = LazyLock::new(|| re(r#""(?:[^"\\]|\\.)*""#));

/// The categories a module's states may name.
fn categories(doc: &Value) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = list(doc, "categories").iter().map(py_str).collect();
    out.extend(BUILT_IN_CATEGORIES.iter().map(|s| s.to_string()));
    out
}

/// Every name and relationship of a module's view.
pub fn name_errors(doc: &Value, library: &Obj) -> Found {
    let mut library = library.clone();
    for (tn, t) in types(doc) {
        library.insert(tn, t.clone());
    }
    let mut out = reserved_name_errors(doc);
    out.extend(relationship_errors(doc, &library));
    out.extend(combined_errors(doc));
    out.extend(cascade_cycle_errors(doc, &library));
    out.extend(clock_errors(doc, &library));
    out.extend(personal_errors(doc, &library));
    out.extend(reparent_errors(doc, &library));
    out.extend(symmetry_errors(doc, &library));
    out.extend(argument_type_errors(doc, &library));
    out.extend(expression_type_errors(doc, &library));
    let cats = categories(doc);
    let imported = imported(doc);
    // the document's enumerations and its types' states, read once, where
    // each type read every type's again: a change of 8,000 types ran past
    // eleven minutes (ninth production review N-8)
    let values = Values::of(doc);
    let superseded = superseded_types(&library);
    for (tn, t) in types(doc) {
        let states = obj(t, "states");
        let attrs = obj(t, "attributes");
        let derived: BTreeSet<String> = keys(t, "derived_attributes").into_iter().collect();
        let conds: BTreeSet<String> = keys(t, "conditions").into_iter().collect();
        let required: BTreeMap<String, BTreeSet<String>> = states
            .iter()
            .map(|(s, v)| {
                (
                    s.clone(),
                    strings(v, "required_attributes").into_iter().collect(),
                )
            })
            .collect();
        let always: BTreeSet<String> = attrs
            .iter()
            .filter(|(_, s)| !optional(s))
            .map(|(a, _)| a.clone())
            .collect();
        let minted: BTreeSet<String> = attrs
            .iter()
            .filter(|(_, s)| {
                flag(s, "identifier") || has(s, "default") || text(s, "type") == Some("counter")
            })
            .map(|(a, _)| a.clone())
            .collect();
        let mut used = BTreeSet::new();
        for (s, v) in states {
            let category = v.get("category").map(py_str).unwrap_or_default();
            if !cats.contains(&category) {
                out.push(finding(
                    segs!["types", &tn, "states", s, "category"],
                    "names",
                    format!("state {s} has the category '{category}', which the module does not declare in categories"),
                ));
            }
            for a in strings(v, "required_attributes") {
                if derived.contains(&a) {
                    out.push(finding(
                        segs!["types", &tn, "states", s, "required_attributes"],
                        "names",
                        format!("state {s} requires the derived attribute '{a}'; a state requires stored attributes, and an invariant may read a derived one"),
                    ));
                } else if !attrs.contains_key(&a) {
                    out.push(finding(
                        segs!["types", &tn, "states", s, "required_attributes"],
                        "names",
                        format!(
                            "state {s} requires the attribute '{a}', which {tn} does not declare"
                        ),
                    ));
                }
            }
        }
        let conditions = obj(t, "conditions");
        for (xn, x) in obj(t, "transitions") {
            let base = segs!["types", &tn, "transitions", xn];
            let tg = targets(x);
            for s in sources(x).into_iter().chain(tg.iter().cloned()) {
                if !states.contains_key(&s) {
                    let key = if tg.contains(&s) { "to" } else { "from" };
                    out.push(finding(
                        ext(&base, segs![key]),
                        "names",
                        format!("{tn}.{xn} names the state {s}, which {tn} does not declare"),
                    ));
                }
            }
            if kind(x) == "external"
                && x.get("to")
                    .is_some_and(|to| sources(x).contains(&py_str(to)))
            {
                out.push(finding(
                    ext(&base, segs!["from"]),
                    "names",
                    format!(
                        "{tn}.{xn} leaves {} for itself; an external transition changes the state, and a change that keeps it is an internal transition",
                        x.get("to").map(py_str).unwrap_or_default()
                    ),
                ));
            }
            for g in keys(x, "guards") {
                used.insert(g.clone());
                let Some(c) = conditions.get(&g).filter(|_| conds.contains(&g)) else {
                    out.push(finding(
                        ext(&base, segs!["guards", &g]),
                        "names",
                        format!("{tn}.{xn} guards on '{g}', which is not a condition of {tn}"),
                    ));
                    continue;
                };
                let expr = c.get("expression").map(py_str).unwrap_or_default();
                for i in input_reads(&expr).difference(&taken(x)) {
                    out.push(finding(
                        ext(&base, segs!["guards", &g]),
                        "names",
                        format!("{tn}.{xn} guards on '{g}', which reads inputs.{i}, and {xn} takes no input '{i}'"),
                    ));
                }
            }
            for step in walk(x.get("effect")) {
                let where_ = ext(&base, ext(segs!["effect"], step.path.clone()));
                for e in step_expressions(&step) {
                    for i in input_reads(&e).difference(&taken(x)) {
                        out.push(finding(
                            where_.clone(),
                            "names",
                            format!(
                                "{tn}.{xn}'s effect reads inputs.{i}, and {xn} takes no input '{i}'"
                            ),
                        ));
                    }
                }
                let body = step.body();
                if matches!(step.kind, "assign" | "add" | "remove") {
                    let loc = body_text(body, "location");
                    if derived.contains(&loc) {
                        out.push(finding(
                            where_.clone(),
                            "names",
                            format!("{tn}.{xn} writes '{loc}', a derived attribute, whose value is its expression"),
                        ));
                    } else if !attrs.contains_key(&loc) {
                        out.push(finding(
                            where_.clone(),
                            "names",
                            format!("{tn}.{xn} writes '{loc}', which {tn} does not declare"),
                        ));
                    }
                    if matches!(step.kind, "add" | "remove")
                        && let Some(spec) = attrs.get(&loc)
                        && !str_or(spec, "type", "").ends_with("[]")
                    {
                        out.push(finding(
                            where_.clone(),
                            "names",
                            format!("{tn}.{xn} {}s to '{loc}', which is not a set", step.kind),
                        ));
                    }
                }
                if step.kind == "clear" && step.path.len() > 1 {
                    for a in list(step.st, "clear").iter().map(py_str) {
                        if !attrs.contains_key(&a) && !derived.contains(&a) {
                            out.push(finding(
                                where_.clone(),
                                "names",
                                format!("{tn}.{xn} clears '{a}', which {tn} does not declare"),
                            ));
                        }
                    }
                }
                if step.kind == "create" {
                    out.extend(create_errors(doc, &library, &where_, &tn, xn, body));
                }
                if step.kind == "call" {
                    let env = binders(&library, &tn, x, &step.path);
                    out.extend(call_errors(&library, &where_, t, &tn, xn, x, body, &env));
                }
            }
            if flag(x, "proposable") && flag(x, "only_via") {
                out.push(finding(
                    ext(&base, segs!["proposable"]),
                    "names",
                    format!("{tn}.{xn} is proposable and only via other transitions; no one may request it, so no proposal of it could be approved (check 14)"),
                ));
            }
            // `any`, every transition of any type, stands alone (ADR-0217)
            let via = strings(x, "only_via");
            if via.len() > 1 && via.iter().any(|v| v == "any") {
                out.push(finding(
                    ext(&base, segs!["only_via"]),
                    "names",
                    format!("{tn}.{xn} is only via any, every transition of any type, which takes no other transition beside it"),
                ));
            }
            for v in via.iter().filter(|v| *v != "any") {
                let (vt, vx) = v.split_once('.').unwrap_or((v, ""));
                if let Some(other) = library.get(vt) {
                    if !obj(other, "transitions").contains_key(vx) {
                        out.push(finding(
                            ext(&base, segs!["only_via"]),
                            "names",
                            format!("{tn}.{xn} is only via {v}, which is not a transition of {vt}"),
                        ));
                    }
                } else if !imported.contains(vt) {
                    out.push(finding(
                        ext(&base, segs!["only_via"]),
                        "names",
                        format!("{tn}.{xn} is only via {v}, and {vt} is neither declared in this module nor imported"),
                    ));
                }
            }
            let req_inputs = strings(x, "required_inputs");
            let opt_inputs = strings(x, "optional_inputs");
            for (i, spec) in obj(x, "inputs") {
                if req_inputs.contains(i) || opt_inputs.contains(i) {
                    out.push(finding(
                        ext(&base, segs!["inputs", i]),
                        "names",
                        format!("{tn}.{xn} declares the input '{i}' and also takes the attribute '{i}' as an input"),
                    ));
                }
                let k = spec
                    .get("type")
                    .filter(|t| super::doc::truthy(Some(t)))
                    .or_else(|| spec.get("reference"))
                    .map(py_str)
                    .unwrap_or_default();
                if k.ends_with("[]") && flag(spec, "optional") {
                    out.push(finding(
                        ext(&base, segs!["inputs", i]),
                        "names",
                        format!("{tn}.{xn}'s input '{i}' is a set, which is never optional: an empty set is supplied instead"),
                    ));
                }
            }
            let mut named: Vec<(&str, String)> = req_inputs
                .iter()
                .map(|a| ("required_inputs", a.clone()))
                .collect();
            named.extend(opt_inputs.iter().map(|a| ("optional_inputs", a.clone())));
            named.extend(cleared(x).into_iter().map(|a| ("effect", a)));
            for (k, a) in named {
                if derived.contains(&a) {
                    out.push(finding(
                        ext(&base, segs![k]),
                        "names",
                        format!("{tn}.{xn} names the derived attribute '{a}' in {k}; its value is its expression, and nothing writes it"),
                    ));
                } else if !attrs.contains_key(&a) {
                    out.push(finding(
                        ext(&base, segs![k]),
                        "names",
                        format!("{tn}.{xn} names the attribute '{a}' in {k}, which {tn} does not declare"),
                    ));
                }
            }
            for a in req_inputs
                .iter()
                .filter(|a| opt_inputs.contains(a))
                .collect::<BTreeSet<_>>()
            {
                out.push(finding(
                    ext(&base, segs!["optional_inputs"]),
                    "names",
                    format!("{tn}.{xn} lists '{a}' as both a required and an optional input"),
                ));
            }
            for a in &opt_inputs {
                if always.contains(a) && !attrs.get(a).is_some_and(|s| has(s, "default")) {
                    out.push(finding(
                        ext(&base, segs!["optional_inputs"]),
                        "names",
                        format!("{tn}.{xn} lists '{a}' as an optional input, but the attribute is neither optional nor defaulted, so the input is required"),
                    ));
                }
            }
            let mut written: BTreeSet<String> = req_inputs
                .iter()
                .chain(opt_inputs.iter())
                .cloned()
                .collect();
            written.extend(assigns(x).into_iter().map(|(_, loc)| loc));
            let cl: BTreeSet<String> = cleared(x).into_iter().collect();
            for a in written.intersection(&cl) {
                out.push(finding(
                    ext(&base, segs!["effect"]),
                    "names",
                    format!("{tn}.{xn} both writes and clears '{a}'"),
                ));
            }
            let mut supplied = always.clone();
            supplied.extend(minted.iter().cloned());
            out.extend(required_errors(&tn, xn, x, &required, &supplied, &library));
        }
        for c in conds.difference(&used) {
            out.push(finding(
                segs!["types", &tn, "conditions", c],
                "names",
                format!("condition '{c}' of {tn} is used by no transition"),
            ));
        }
        out.extend(value_errors(&values, &tn, t));
        out.extend(write_type_errors(doc, &tn, t, &library));
        out.extend(conditional_step_errors(&tn, t));
        let mut readable = cats.clone();
        readable.extend(evaluator_names(doc));
        out.extend(read_errors(&tn, t, &readable));
        out.extend(indexed_errors(&tn, t, &library));
        out.extend(scan_errors(&tn, t, &library));
        out.extend(observation_read_errors(&tn, t));
        out.extend(binder_errors(&tn, t));
        out.extend(identifier_errors(doc, &tn, t, &library));
        out.extend(evaluator_errors(doc, &tn, t));
        out.extend(supersession_errors(doc, &tn, t, &library, &superseded));
        out.extend(override_errors(doc, &tn, t, &library));
        for (mn, m) in obj(t, "metrics") {
            out.extend(metric_errors(&tn, t, mn, m));
        }
    }
    out
}

/// Names the format keeps for itself: the text language's and generated
/// ones.
fn reserved_name_errors(doc: &Value) -> Found {
    let mut out = Found::new();
    for (i, c) in list(doc, "categories").iter().enumerate() {
        let c = py_str(c);
        if NOT_A_CATEGORY.contains(&c.as_str()) {
            out.push(finding(
                segs!["categories", i],
                "names",
                format!(
                    "'{c}' cannot name a category: the text language reserves it in that position"
                ),
            ));
        }
    }
    for (tn, t) in types(doc) {
        let attrs = obj(t, "attributes");
        let states = obj(t, "states");
        let mut owned: Vec<(Path, String)> = attrs
            .keys()
            .map(|a| (segs!["types", &tn, "attributes", a], a.clone()))
            .collect();
        for (o, ob) in obj(t, "observations") {
            for a in keys(ob, "attributes") {
                owned.push((segs!["types", &tn, "observations", o, "attributes", &a], a));
            }
        }
        for d in keys(t, "derived_attributes") {
            owned.push((segs!["types", &tn, "derived_attributes", &d], d));
        }
        let mut members: BTreeSet<String> = attrs.keys().cloned().collect();
        members.extend(keys(t, "observations"));
        for d in keys(t, "derived_attributes") {
            if members.contains(&d) {
                out.push(finding(
                    segs!["types", &tn, "derived_attributes", &d],
                    "names",
                    format!("'{d}' names both a derived attribute and another member of {tn}; a member has one name"),
                ));
            }
        }
        for (p, a) in owned {
            if NOT_AN_ATTRIBUTE.contains(&a.as_str()) {
                out.push(finding(p, "names", format!("'{a}' cannot name an attribute: in an expression it means something else first")));
            } else if WITHDRAWN_MEMBERS.contains(&a.as_str()) {
                out.push(finding(
                    p,
                    "names",
                    format!("'{a}' cannot name a member: it is kept for a member withdrawn before 1.0.0, which may return with a use (ADR-0210 decision 3)"),
                ));
            } else if OBJECT_MEMBERS.contains(&a.as_str()) {
                out.push(finding(
                    p,
                    "names",
                    format!("'{a}' cannot name a member: every object has one of that name, which it would shadow (the model's check 33)"),
                ));
            }
        }
        for (a, spec) in attrs {
            for k in ONLY_ON_OBSERVATIONS.iter().filter(|k| has(spec, k)) {
                out.push(finding(
                    segs!["types", &tn, "attributes", a, *k],
                    "names",
                    format!("'{k}' applies only to an observation's attribute, not to {tn}.{a}"),
                ));
            }
        }
        for (o, ob) in obj(t, "observations") {
            for (a, spec) in obj(ob, "attributes") {
                if text(spec, "type") == Some("counter") {
                    out.push(finding(
                        segs!["types", &tn, "observations", o, "attributes", a],
                        "names",
                        format!("{o}.{a} is a counter, which an observation, born final, cannot have; a counter belongs to a type tracked by quantity"),
                    ));
                }
                let mut only: Vec<&&str> = ONLY_ON_TYPES.iter().filter(|k| has(spec, k)).collect();
                only.sort();
                for k in only {
                    out.push(finding(
                        segs!["types", &tn, "observations", o, "attributes", a, *k],
                        "names",
                        format!("'{k}' applies only to a type's attribute, not to the observation attribute {o}.{a}"),
                    ));
                }
            }
        }
        for xn in keys(t, "transitions") {
            if NOT_A_TRANSITION.contains(&xn.as_str()) {
                let why = if xn == "any" {
                    "the text language reserves it"
                } else {
                    "the engine's own events carry that name (ADR-0148)"
                };
                out.push(finding(
                    segs!["types", &tn, "transitions", &xn],
                    "names",
                    format!("'{xn}' cannot name a transition: {why}"),
                ));
            }
        }
        for c in keys(t, "conditions") {
            if c.strip_suffix("_provided")
                .is_some_and(|a| attrs.contains_key(a))
            {
                out.push(finding(
                    segs!["types", &tn, "conditions", &c],
                    "names",
                    format!("'{c}' is the name of the guard generated for a required input; choose another name"),
                ));
            }
            if GENERATED_GUARDS.contains(&c.as_str()) {
                out.push(finding(
                    segs!["types", &tn, "conditions", &c],
                    "names",
                    format!("'{c}' is the name of a guard the engine generates, so a refusal naming it would not say which it was; choose another name"),
                ));
            }
        }
        for i in keys(t, "invariants") {
            if i.strip_suffix("_invariant")
                .is_some_and(|s| states.contains_key(&s.to_uppercase()))
            {
                out.push(finding(
                    segs!["types", &tn, "invariants", &i],
                    "names",
                    format!("'{i}' is the name of the invariant generated for a state's required_attributes; choose another name"),
                ));
            }
            if i.strip_suffix("_unique")
                .is_some_and(|a| attrs.contains_key(a))
            {
                out.push(finding(
                    segs!["types", &tn, "invariants", &i],
                    "names",
                    format!("'{i}' is the name of the invariant a unique attribute or a one-to-one end generates; choose another name"),
                ));
            }
            if i == "identity_taken" {
                out.push(finding(
                    segs!["types", &tn, "invariants", &i],
                    "names",
                    format!("'{i}' is the name of the invariant an actor's identity generates (ADR-0190); choose another name"),
                ));
            }
        }
    }
    out
}

/// Every type a type's attributes, observations and inputs name.
fn references(t: &Value, tn: &str) -> Vec<(Path, String)> {
    let strip = |s: &Value| py_str(s).trim_end_matches(['[', ']']).to_string();
    let mut refs: Vec<(Path, String)> = obj(t, "attributes")
        .iter()
        .filter_map(|(a, s)| {
            s.get("reference")
                .map(|r| (segs!["types", tn, "attributes", a], strip(r)))
        })
        .collect();
    for (o, ob) in obj(t, "observations") {
        for (a, s) in obj(ob, "attributes") {
            if let Some(r) = s.get("reference") {
                refs.push((
                    segs!["types", tn, "observations", o, "attributes", a],
                    strip(r),
                ));
            }
        }
    }
    for (xn, x) in obj(t, "transitions") {
        for (i, s) in obj(x, "inputs") {
            if let Some(r) = s.get("reference") {
                refs.push((segs!["types", tn, "transitions", xn, "inputs", i], strip(r)));
            }
        }
    }
    refs
}

fn ref_of(spec: &Value) -> String {
    spec.get("reference").map(py_str).unwrap_or_default()
}

fn target_of(spec: &Value) -> String {
    ref_of(spec).trim_end_matches(['[', ']']).to_string()
}

/// The ends of a relationship name each other; a composite end's parts point
/// back with a single, required end; a cascade names a transition of the
/// part with its inputs; and every final transition of a whole is covered or
/// survived, never both (`declaration-syntax.md` §3.2, §3.3).
fn relationship_errors(doc: &Value, library: &Obj) -> Found {
    let mut out = Found::new();
    let module: Obj = types(doc)
        .into_iter()
        .map(|(n, t)| (n, t.clone()))
        .collect();
    let imported = imported(doc);
    let machines = obj(doc, "machines");
    // the library and the module's types together, built once: built for
    // each type, the copy made a module of 3,000 types take 83 s to check,
    // under the writer's lock as a submit's dry run (DRYRUN-LOCK)
    let mut kinds = library.clone();
    for (k, v) in &module {
        kinds.insert(k.clone(), v.clone());
    }
    for (tn, t) in types(doc) {
        for (p, r) in references(t, &tn) {
            if !module.contains_key(&r) && !imported.contains(&r) {
                out.push(finding(
                    p,
                    "names",
                    format!(
                        "{tn} references {r}, which is neither declared in this module nor imported"
                    ),
                ));
            }
        }
        let finals: BTreeSet<String> = obj(t, "states")
            .iter()
            .filter(|(_, v)| flag(v, "final"))
            .map(|(s, _)| s.clone())
            .collect();
        for (an, spec) in obj(t, "attributes") {
            let here = segs!["types", &tn, "attributes", an];
            let k = ref_of(spec);
            let (target, many) = (target_of(spec), k.ends_with("[]"));
            if has(spec, "opposite") {
                let opposite = spec.get("opposite").map(py_str).unwrap_or_default();
                let Some(other) = module.get(&target) else {
                    out.push(finding(
                        ext(&here, segs!["opposite"]),
                        "names",
                        format!("{tn}.{an} names an opposite end in {target}, which this module does not declare; both ends of a relationship are in one module"),
                    ));
                    continue;
                };
                let back = obj(other, "attributes")
                    .get(&opposite)
                    .filter(|b| !b.is_null());
                let names_back = back.is_some_and(|b| {
                    target_of(b) == tn
                        && b.get("opposite").map(py_str).as_deref() == Some(an.as_str())
                });
                if !names_back {
                    out.push(finding(
                        ext(&here, segs!["opposite"]),
                        "names",
                        format!(
                            "{tn}.{an}'s opposite {target}.{opposite} does not name {tn}.{an} back"
                        ),
                    ));
                    continue;
                }
                if flag(spec, "stored") && back.is_some_and(|b| flag(b, "stored")) {
                    out.push(finding(
                        ext(&here, segs!["stored"]),
                        "names",
                        format!("{tn}.{an} and {target}.{opposite} are both marked stored; one end holds the value"),
                    ));
                }
            } else if many && !machines.contains_key(&tn) {
                out.push(finding(
                    here.clone(),
                    "names",
                    format!("{tn}.{an} is a set of references with no opposite, which has nowhere to be stored; an end with no opposite is single (§4.3; the model's check 41)"),
                ));
            }
            let composite = text(spec, "aggregation") == Some("composite");
            if composite && flag(t, "abstract") && (has(spec, "cascade") || has(spec, "survives")) {
                out.push(finding(
                    here.clone(),
                    "names",
                    format!("{tn}.{an} is declared on the abstract {tn}, which has no transitions to cascade on or survive; each concrete subtype gives its cascade and survives in inherited_parts (§4.14)"),
                ));
                continue;
            }
            if composite {
                if !has(spec, "opposite") {
                    out.push(finding(
                        here.clone(),
                        "names",
                        format!(
                            "{tn}.{an} is composite and names no opposite: a part names its whole"
                        ),
                    ));
                    continue;
                }
                let opposite = spec.get("opposite").map(py_str).unwrap_or_default();
                let part = module
                    .get(&target)
                    .cloned()
                    .unwrap_or(Value::Object(Default::default()));
                let owner = obj(&part, "attributes")
                    .get(&opposite)
                    .cloned()
                    .unwrap_or(Value::Null);
                if ref_of(&owner).ends_with("[]") || flag(&owner, "optional") {
                    out.push(finding(
                        here.clone(),
                        "names",
                        format!("{target}.{opposite} MUST be single and required: a part of {tn}.{an} belongs to exactly one whole"),
                    ));
                }
                out.extend(part_coverage(
                    &tn,
                    t,
                    an,
                    &target,
                    &part,
                    list(spec, "cascade"),
                    spec.get("survives"),
                    &here,
                    &finals,
                ));
            } else if has(spec, "cascade") || has(spec, "survives") {
                out.push(finding(
                    here.clone(),
                    "names",
                    format!("{tn}.{an} has a cascade or survives, which only a composite end has"),
                ));
            }
        }
        let mut inherited_parts: Obj = Default::default();
        for b in bases(&tn, &kinds).iter().rev() {
            for (an, spec) in obj(&kinds[b.as_str()], "attributes") {
                if text(spec, "aggregation") == Some("composite") {
                    inherited_parts.insert(an.clone(), spec.clone());
                }
            }
        }
        let given = obj(t, "inherited_parts");
        for an in given.keys() {
            if !inherited_parts.contains_key(an) {
                out.push(finding(
                    segs!["types", &tn, "inherited_parts", an],
                    "names",
                    format!(
                        "{tn} gives '{an}' in inherited_parts, which is not a part it inherits"
                    ),
                ));
            }
        }
        if flag(t, "abstract") {
            if !given.is_empty() {
                out.push(finding(
                    segs!["types", &tn, "inherited_parts"],
                    "names",
                    format!("{tn} is abstract and has no transitions to cascade on; its concrete subtypes give its parts"),
                ));
            }
            continue;
        }
        for (an, spec) in &inherited_parts {
            let target = target_of(spec);
            let ip = given.get(an).cloned().unwrap_or(Value::Null);
            let part = kinds
                .get(&target)
                .cloned()
                .unwrap_or(Value::Object(Default::default()));
            out.extend(part_coverage(
                &tn,
                t,
                an,
                &target,
                &part,
                list(&ip, "cascade"),
                ip.get("survives"),
                &segs!["types", &tn, "inherited_parts", an],
                &finals,
            ));
        }
    }
    out
}

/// An expression with each child rebuilt by `f`, the rest as it was.
fn map_children(e: &Expr, f: &mut dyn FnMut(&Expr) -> Expr) -> Expr {
    let mut b = |x: &Expr| Box::new(f(x));
    match e {
        Expr::Num(_)
        | Expr::Str(_)
        | Expr::Bool(_)
        | Expr::Dur(..)
        | Expr::Money(..)
        | Expr::Name(_)
        | Expr::Param(_)
        | Expr::Lit(_) => e.clone(),
        Expr::Path(base, m) => Expr::Path(b(base), m.clone()),
        Expr::Call(func, args) => {
            let func = b(func);
            Expr::Call(func, args.iter().map(|a| *b(a)).collect())
        }
        Expr::Agg {
            kind,
            distinct,
            var,
            collection,
            filter,
            body,
        } => Expr::Agg {
            kind: kind.clone(),
            distinct: *distinct,
            var: var.clone(),
            collection: collection.as_deref().map(&mut b),
            filter: filter.as_deref().map(&mut b),
            body: body.as_deref().map(&mut b),
        },
        Expr::Metric {
            name,
            binds,
            over,
            fresh,
        } => {
            let name = b(name);
            let binds = binds.iter().map(|(d, x)| (d.clone(), *b(x))).collect();
            Expr::Metric {
                name,
                binds,
                over: over.as_deref().map(&mut b),
                fresh: fresh.as_deref().map(&mut b),
            }
        }
        Expr::Changed(names, ev) => Expr::Changed(names.clone(), b(ev)),
        Expr::Set(items) => Expr::Set(items.iter().map(|x| *b(x)).collect()),
        Expr::Not(x) => Expr::Not(b(x)),
        Expr::Neg(x) => Expr::Neg(b(x)),
        Expr::IsNull(x, neg) => Expr::IsNull(b(x), *neg),
        Expr::In(l, r) => {
            let l = b(l);
            Expr::In(l, b(r))
        }
        Expr::Bin(op, l, r) => {
            let l = b(l);
            Expr::Bin(*op, l, b(r))
        }
        Expr::If(c, t, e2) => {
            let c = b(c);
            let t = b(t);
            Expr::If(c, t, b(e2))
        }
    }
}

/// The members of a type the swap test exchanges: its attributes, derived
/// attributes and observations, and what every object has, `state` and
/// `referrers` among them (`check-flows.py`'s `swap_members`).
fn swap_members(t: &Value) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = keys(t, "attributes").into_iter().collect();
    out.extend(keys(t, "derived_attributes"));
    out.extend(keys(t, "observations"));
    out.extend(OBJECT_MEMBERS.iter().map(|s| s.to_string()));
    out.extend(["state".to_string(), "referrers".to_string()]);
    out
}

/// An expression with each derived attribute of its own object it reads, by
/// its name or through `this`, replaced by that attribute's expression,
/// however deep (`check-flows.py`'s `inline_derived`).
fn inline_derived(e: &Expr, t: &Value, seen: &BTreeSet<String>) -> Expr {
    let name = match e {
        Expr::Name(n) => Some(n),
        Expr::Path(base, n) if matches!(&**base, Expr::Name(b) if b == "this") => Some(n),
        _ => None,
    };
    if let Some(n) = name
        && !seen.contains(n)
        && !super::late()
        && let Some(spec) = obj(t, "derived_attributes").get(n)
        && let Ok(tree) = parse(&spec.get("expression").map(py_str).unwrap_or_default())
    {
        let mut deeper = seen.clone();
        deeper.insert(n.clone());
        return inline_derived(&tree, t, &deeper);
    }
    if let Expr::Agg {
        kind,
        distinct,
        var,
        collection,
        filter,
        body,
    } = e
    {
        // a bound name is not a member
        let mut inner = seen.clone();
        inner.extend(var.iter().cloned());
        return Expr::Agg {
            kind: kind.clone(),
            distinct: *distinct,
            var: var.clone(),
            collection: collection
                .as_deref()
                .map(|c| Box::new(inline_derived(c, t, seen))),
            filter: filter
                .as_deref()
                .map(|x| Box::new(inline_derived(x, t, &inner))),
            body: body
                .as_deref()
                .map(|x| Box::new(inline_derived(x, t, &inner))),
        };
    }
    map_children(e, &mut |c| inline_derived(c, t, seen))
}

/// An expression with the object it is evaluated on and the scan's bound
/// object exchanged: `this` and the bound name, and each member read on one,
/// read on the other (`declaration-syntax.md` §3.4; `check-flows.py`'s
/// `swapped`).
fn swapped(e: &Expr, var: &str, members: &BTreeSet<String>, bound: &BTreeSet<String>) -> Expr {
    match e {
        Expr::Name(n) if n == "this" => Expr::Name(var.to_string()),
        Expr::Name(n) if n == var && bound.contains(var) => Expr::Name("this".into()),
        Expr::Name(n) if members.contains(n) && !bound.contains(n) => {
            Expr::Path(Box::new(Expr::Name(var.to_string())), n.clone())
        }
        Expr::Agg {
            kind,
            distinct,
            var: v2,
            collection,
            filter,
            body,
        } => {
            let mut inner = bound.clone();
            inner.extend(v2.iter().cloned());
            Expr::Agg {
                kind: kind.clone(),
                distinct: *distinct,
                var: v2.clone(),
                collection: collection
                    .as_deref()
                    .map(|c| Box::new(swapped(c, var, members, bound))),
                filter: filter
                    .as_deref()
                    .map(|x| Box::new(swapped(x, var, members, &inner))),
                body: body
                    .as_deref()
                    .map(|x| Box::new(swapped(x, var, members, &inner))),
            }
        }
        _ => map_children(e, &mut |c| swapped(c, var, members, bound)),
    }
}

/// An expression under the swap test's four normalisations and no other,
/// comparable for equality and ordered so a conjunction is a set.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Canon {
    Leaf(String),
    Node(String, Vec<Canon>),
    Set(String, BTreeSet<Canon>),
}

/// The conjuncts of an `and`, however nested, in no order.
fn conjuncts(e: &Expr) -> Vec<&Expr> {
    match e {
        Expr::Bin(BinOp::And, l, r) => {
            let mut out = conjuncts(l);
            out.extend(conjuncts(r));
            out
        }
        _ => vec![e],
    }
}

/// An expression under the swap test's four normalisations (§3.4,
/// ADR-0063): an `and`'s conjuncts as a set, `==` and `!=` whatever their
/// operands' order, the comparisons whatever their direction, and a member
/// of `this` whatever its spelling (`check-flows.py`'s `normalised`).
fn normalised(e: &Expr) -> Canon {
    let opt = |x: &Option<Box<Expr>>| {
        x.as_deref()
            .map(normalised)
            .unwrap_or(Canon::Leaf("none".into()))
    };
    match e {
        Expr::Path(base, m) if matches!(&**base, Expr::Name(b) if b == "this") => {
            Canon::Leaf(format!("name:{m}"))
        }
        Expr::Name(n) => Canon::Leaf(format!("name:{n}")),
        Expr::Param(_) | Expr::Lit(_) => Canon::Leaf(format!("{e:?}")),
        Expr::Bin(BinOp::And, ..) => Canon::Set(
            "and".into(),
            conjuncts(e).into_iter().map(normalised).collect(),
        ),
        Expr::Bin(op @ (BinOp::Eq | BinOp::Ne), l, r) => Canon::Set(
            op.text().into(),
            [normalised(l), normalised(r)].into_iter().collect(),
        ),
        Expr::Bin(BinOp::Gt, l, r) => Canon::Node("<".into(), vec![normalised(r), normalised(l)]),
        Expr::Bin(BinOp::Ge, l, r) => Canon::Node("<=".into(), vec![normalised(r), normalised(l)]),
        Expr::Bin(op, l, r) => Canon::Node(op.text().into(), vec![normalised(l), normalised(r)]),
        Expr::Path(base, m) => Canon::Node(format!("path:{m}"), vec![normalised(base)]),
        Expr::Call(f, args) => Canon::Node(
            "call".into(),
            std::iter::once(normalised(f))
                .chain(args.iter().map(normalised))
                .collect(),
        ),
        Expr::Agg {
            kind,
            distinct,
            var,
            collection,
            filter,
            body,
        } => Canon::Node(
            format!("agg:{kind}:{distinct}:{var:?}"),
            vec![opt(collection), opt(filter), opt(body)],
        ),
        Expr::Metric {
            name,
            binds,
            over,
            fresh,
        } => Canon::Node(
            format!(
                "metric:{:?}",
                binds.iter().map(|(d, _)| d.as_str()).collect::<Vec<_>>()
            ),
            std::iter::once(normalised(name))
                .chain(binds.iter().map(|(_, x)| normalised(x)))
                .chain([opt(over), opt(fresh)])
                .collect(),
        ),
        Expr::Changed(names, ev) => Canon::Node(format!("changed:{names:?}"), vec![normalised(ev)]),
        Expr::Set(items) => Canon::Node("set".into(), items.iter().map(normalised).collect()),
        Expr::Not(x) => Canon::Node("not".into(), vec![normalised(x)]),
        Expr::Neg(x) => Canon::Node("neg".into(), vec![normalised(x)]),
        Expr::IsNull(x, neg) => Canon::Node(format!("isnull:{neg}"), vec![normalised(x)]),
        Expr::In(l, r) => Canon::Node("in".into(), vec![normalised(l), normalised(r)]),
        Expr::If(c, t, f) => Canon::Node(
            "if".into(),
            vec![normalised(c), normalised(t), normalised(f)],
        ),
        Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Dur(..) | Expr::Money(..) => {
            Canon::Leaf(format!("{e:?}"))
        }
    }
}

/// The members an expression reads on the scan's bound object and on its
/// own, as two sets (`check-flows.py`'s `sides`).
fn sides(
    e: &Expr,
    var: &str,
    members: &BTreeSet<String>,
    bound: &BTreeSet<String>,
    on: &mut (BTreeSet<String>, BTreeSet<String>),
) {
    match e {
        Expr::Path(base, m)
            if matches!(&**base, Expr::Name(b) if b == var) && bound.contains(var) =>
        {
            on.0.insert(m.clone());
        }
        Expr::Path(base, m) if matches!(&**base, Expr::Name(b) if b == "this") => {
            on.1.insert(m.clone());
        }
        Expr::Name(n) => {
            if members.contains(n) && !bound.contains(n) {
                on.1.insert(n.clone());
            }
        }
        Expr::Agg {
            var: v2,
            collection,
            filter,
            body,
            ..
        } => {
            if let Some(c) = collection {
                sides(c, var, members, bound, on);
            }
            let mut inner = bound.clone();
            inner.extend(v2.iter().cloned());
            for x in [filter, body].into_iter().flatten() {
                sides(x, var, members, &inner, on);
            }
        }
        _ => {
            for c in children(e) {
                sides(c, var, members, bound, on);
            }
        }
    }
}

/// Each type scan in an expression, as its bound name, the type and its
/// `where`, outermost first (`check-flows.py`'s `scans`).
fn scans(
    e: &Expr,
    lib: &Obj,
    bound: &BTreeSet<String>,
    out: &mut Vec<(String, String, Option<Expr>)>,
) {
    if let Expr::Agg {
        var,
        collection,
        filter,
        body,
        ..
    } = e
    {
        if let (Some(v), Some(c)) = (var, collection)
            && let Expr::Name(tn) = &**c
            && lib.contains_key(tn)
            && !bound.contains(tn)
        {
            out.push((v.clone(), tn.clone(), filter.as_deref().cloned()));
        }
        if let Some(c) = collection {
            scans(c, lib, bound, out);
        }
        let mut inner = bound.clone();
        inner.extend(var.iter().cloned());
        for x in [filter, body].into_iter().flatten() {
            scans(x, lib, &inner, out);
        }
        return;
    }
    for c in children(e) {
        scans(c, lib, bound, out);
    }
}

/// A type-scan invariant MUST be symmetric, its verdict the same whichever
/// of the objects it relates is written, decided by the swap test of
/// `declaration-syntax.md` §3.4 over the whole invariant, its own derived
/// attributes read as their expressions; a member of the object read on one
/// side only is named. A scan MUST NOT exclude the object itself, by its id
/// or by `this`, which it never ranges over (the model's check 6, ADR-0061,
/// ADR-0063; `check-flows.py`'s `symmetry_errors`).
fn symmetry_errors(doc: &Value, library: &Obj) -> Found {
    let mut kinds = library.clone();
    for (tn, t) in types(doc) {
        kinds.insert(tn, t.clone());
    }
    let none = BTreeSet::new();
    let mut out = Found::new();
    for (tn, t) in types(doc) {
        let members = swap_members(t);
        for (i, spec) in obj(t, "invariants") {
            let where_ = segs!["types", &tn, "invariants", i, "expression"];
            // step 2 refuses what does not parse
            let Ok(tree) = parse(&spec.get("expression").map(py_str).unwrap_or_default()) else {
                continue;
            };
            let tree = inline_derived(&tree, t, &none);
            let mut found = Vec::new();
            scans(&tree, &kinds, &none, &mut found);
            for (var, scanned, condition) in found {
                let conjunct_forms: Vec<Canon> = condition
                    .as_ref()
                    .map(|c| conjuncts(c).into_iter().map(normalised).collect())
                    .unwrap_or_default();
                let selves: [BTreeSet<Canon>; 2] = [
                    [
                        normalised(&Expr::Path(Box::new(Expr::Name(var.clone())), "id".into())),
                        normalised(&Expr::Name("id".into())),
                    ]
                    .into_iter()
                    .collect(),
                    [
                        normalised(&Expr::Name(var.clone())),
                        normalised(&Expr::Name("this".into())),
                    ]
                    .into_iter()
                    .collect(),
                ];
                let excludes = conjunct_forms.iter().any(
                    |c| matches!(c, Canon::Set(op, set) if op == "!=" && selves.contains(set)),
                );
                if excludes {
                    out.push(finding(
                        where_.clone(),
                        "names",
                        format!(
                            "{tn}'s invariant '{i}' excludes the object itself from its scan of {scanned}, which never ranges over it (the model's check 6)"
                        ),
                    ));
                    continue;
                }
                if normalised(&swapped(&tree, &var, &members, &none)) == normalised(&tree) {
                    continue;
                }
                let mut on = (BTreeSet::new(), BTreeSet::new());
                sides(&tree, &var, &members, &none, &mut on);
                let one_sided: Vec<String> = on.0.symmetric_difference(&on.1).cloned().collect();
                let why = if one_sided.is_empty() {
                    "with the two objects exchanged its predicate is not the one written, under the four normalisations of the swap test".to_string()
                } else {
                    format!(
                        "it reads {} of one of the two objects only, so its verdict depends on which is written",
                        one_sided.join(", ")
                    )
                };
                out.push(finding(
                    where_.clone(),
                    "names",
                    format!(
                        "{tn}'s invariant '{i}' scans {scanned} and is not symmetric: {why} (the model's check 6)"
                    ),
                ));
            }
        }
    }
    out
}

/// What a transition of a type writes, as (type, member): what its effect
/// assigns, adds, removes or clears and the attribute inputs it takes, each
/// stored end's far end and its target type's `referrers`; its type's
/// `state` where it moves or sets it; a new object, `*new`, where it creates
/// one; and every personal attribute where it is an erasure
/// (`check-flows.py`'s `transition_writes`).
fn transition_writes(t: &Value, tn: &str, x: &Value) -> BTreeSet<(String, String)> {
    let attrs = obj(t, "attributes");
    let mut out = BTreeSet::new();
    let wrote = |m: &str, out: &mut BTreeSet<(String, String)>| {
        out.insert((tn.to_string(), m.to_string()));
        let spec = attrs.get(m).cloned().unwrap_or(Value::Null);
        let target = target_of(&spec);
        if !target.is_empty() {
            out.insert((target.clone(), "referrers".to_string()));
            if let Some(o) = spec.get("opposite") {
                out.insert((target, py_str(o)));
            }
        }
    };
    for step in walk(x.get("effect")) {
        let body = &step.st[step.kind];
        match step.kind {
            "assign" | "add" | "remove" => {
                wrote(
                    &body.get("location").map(py_str).unwrap_or_default(),
                    &mut out,
                );
            }
            "clear" => {
                for a in list(step.st, "clear") {
                    wrote(&py_str(a), &mut out);
                }
            }
            _ => {}
        }
    }
    for m in strings(x, "required_inputs")
        .into_iter()
        .chain(strings(x, "optional_inputs"))
    {
        if attrs.contains_key(&m) {
            wrote(&m, &mut out);
        }
    }
    let k = kind(x);
    if matches!(k, "initial" | "external" | "assertion") {
        out.insert((tn.to_string(), "state".to_string()));
    }
    if k == "initial" {
        out.insert((tn.to_string(), "*new".to_string()));
    }
    if k == "erasure" {
        for (m, spec) in attrs {
            if flag(spec, "personal") {
                wrote(m, &mut out);
            }
        }
    }
    out
}

/// What an invariant reads, as (type, member), as far as step 3's walker
/// types it (`check-flows.py`'s `invariant_reads`).
fn invariant_reads(
    kinds: &Obj,
    tn: &str,
    text: &str,
    seen: &BTreeSet<(String, String)>,
) -> BTreeSet<(String, String)> {
    // step 2 refuses what does not parse
    let Ok(tree) = parse(text) else {
        return BTreeSet::new();
    };
    let mut walker = OptionalReads::new(kinds, tn, None);
    walker.through_tests = true;
    walker.walk(&tree, &Env::new());
    let mut out = walker.members.clone();
    out.extend(
        walker
            .scanned
            .iter()
            .map(|u| (u.clone(), "*new".to_string())),
    );
    out.extend(
        walker
            .members
            .iter()
            .filter(|(_, m)| m == "open")
            .map(|(u, _)| (u.clone(), "state".to_string())),
    );
    for pair in walker.derived.iter().filter(|p| !seen.contains(*p)) {
        let (u, d) = pair;
        let expr = kinds
            .get(u)
            .and_then(|t| obj(t, "derived_attributes").get(d))
            .and_then(|s| s.get("expression"))
            .map(py_str)
            .unwrap_or_default();
        let mut deeper = seen.clone();
        deeper.insert(pair.clone());
        out.extend(invariant_reads(kinds, u, &expr, &deeper));
    }
    out
}

/// The transitions, `<Type>.<transition>`, that could violate an invariant
/// of a type, for a publish report to name (`DESIGN.md` §5.6): each whose
/// writes meet what the invariant reads, and the type's own creations
/// (`check-flows.py`'s `could_violate`).
pub fn could_violate(
    kinds: &Obj,
    tn: &str,
    text: &str,
    machines: &BTreeSet<String>,
) -> Vec<String> {
    let mut reads = invariant_reads(kinds, tn, text, &BTreeSet::new());
    reads.insert((tn.to_string(), "*new".to_string()));
    let mut out: Vec<String> = Vec::new();
    // a machine's transitions are named by the type binding it
    for (u, t) in kinds.iter().filter(|(u, _)| !machines.contains(*u)) {
        for (xn, x) in obj(t, "transitions") {
            if !transition_writes(t, u, x).is_disjoint(&reads) {
                out.push(format!("{u}.{xn}"));
            }
        }
    }
    out.sort();
    out
}

/// An invariant's form, which a publish report states
/// (`declaration-syntax.md` §3.4, ADR-0055, ADR-0171): `type_scan` where it
/// ranges over a type by name, else `traversal` where it reads through a
/// reference or over `referrers`, reading other objects, else `local`; a
/// derived attribute it reads counts as its own expression's form, however
/// deep. No invariant reads an observation, which step 3 refuses
/// (`check-flows.py`'s `invariant_form`).
pub fn invariant_form(
    kinds: &Obj,
    tn: &str,
    text: &str,
    seen: &BTreeSet<(String, String)>,
) -> &'static str {
    // step 2 refuses what does not parse
    let Ok(tree) = parse(text) else {
        return "local";
    };
    let mut walker = OptionalReads::new(kinds, tn, None);
    walker.through_tests = true;
    walker.walk(&tree, &Env::new());
    if !walker.scanned.is_empty() {
        return "type_scan";
    }
    let mut form =
        if !walker.traversed.is_empty() || walker.members.iter().any(|(_, m)| m == "referrers") {
            "traversal"
        } else {
            "local"
        };
    for pair in walker.derived.iter().filter(|p| !seen.contains(*p)) {
        let (u, d) = pair;
        let expr = kinds
            .get(u)
            .and_then(|t| obj(t, "derived_attributes").get(d))
            .and_then(|s| s.get("expression"))
            .map(py_str)
            .unwrap_or_default();
        let mut deeper = seen.clone();
        deeper.insert(pair.clone());
        match invariant_form(kinds, u, &expr, &deeper) {
            "type_scan" => return "type_scan",
            "traversal" => form = "traversal",
            _ => {}
        }
    }
    form
}

/// A re-parent, a `do` or an `act` writing a part's end back to its whole on
/// an existing object, MUST NOT write an end whose type is abstract, since
/// which source whole's invariants are re-checked would then not be known
/// from the text (the model's check 36, ADR-0058). It writes the end by an
/// `assign` step or by naming it among its inputs; a creation writing it is
/// not a re-parent. A part whose end back names another type than the whole
/// is refused as ends that do not name each other (`relationship_errors`)
/// (`check-flows.py`'s `reparent_errors`).
fn reparent_errors(doc: &Value, library: &Obj) -> Found {
    let mut kinds = library.clone();
    for (tn, t) in types(doc) {
        kinds.insert(tn, t.clone());
    }
    let mut out = Found::new();
    for (tn, t) in types(doc) {
        // each end back to a whole of an abstract type, by the type
        let mut owners: BTreeMap<String, String> = BTreeMap::new();
        for (a, spec) in obj(t, "attributes") {
            let w = ref_of(spec);
            let Some(whole) = kinds.get(&w) else { continue };
            let opposite = spec.get("opposite").map(py_str).unwrap_or_default();
            let composite = obj(whole, "attributes")
                .get(&opposite)
                .is_some_and(|end| text(end, "aggregation") == Some("composite"));
            if composite && flag(whole, "abstract") {
                owners.insert(a.clone(), w);
            }
        }
        for (xn, x) in obj(t, "transitions") {
            if !matches!(kind(x), "external" | "internal") {
                continue;
            }
            let base = segs!["types", &tn, "transitions", xn];
            let mut written: Vec<(Path, String)> = Vec::new();
            for key in ["required_inputs", "optional_inputs"] {
                for a in strings(x, key) {
                    written.push((ext(&base, segs![key]), a));
                }
            }
            for step in walk(x.get("effect")) {
                if step.kind == "assign" {
                    let loc = step.st["assign"]
                        .get("location")
                        .map(py_str)
                        .unwrap_or_default();
                    written.push((ext(&base, ext(segs!["effect"], step.path.clone())), loc));
                }
            }
            for (where_, a) in written {
                if let Some(w) = owners.get(&a) {
                    out.push(finding(
                        where_,
                        "names",
                        format!(
                            "{tn}.{xn} re-parents a part, writing '{a}', whose type {w} is abstract, so which whole's invariants it re-checks is not known from the text (the model's check 36, ADR-0058)"
                        ),
                    ));
                }
            }
        }
    }
    out
}

/// The inputs of a transition that are personal values (the model's check
/// 10, ADR-0078): marked `personal`, an attribute input of a personal
/// attribute, or read by a write of its effect into a personal attribute, as
/// the reference runner's `input_personal` has it (`check-flows.py`'s
/// `personal_inputs`).
fn personal_inputs(kinds: &Obj, tn: &str, x: &Value) -> BTreeSet<String> {
    let t = kinds.get(tn).unwrap_or(&super::doc::NULL);
    let attrs = obj(t, "attributes");
    let personal = |n: &str| attrs.get(n).is_some_and(|s| flag(s, "personal"));
    let mut out: BTreeSet<String> = obj(x, "inputs")
        .iter()
        .filter(|(_, s)| flag(s, "personal"))
        .map(|(n, _)| n.clone())
        .collect();
    for n in strings(x, "required_inputs")
        .into_iter()
        .chain(strings(x, "optional_inputs"))
    {
        if personal(&n) {
            out.insert(n);
        }
    }
    let took = taken(x);
    for step in walk(x.get("effect")) {
        if matches!(step.kind, "assign" | "add") {
            let body = &step.st[step.kind];
            if personal(&body.get("location").map(py_str).unwrap_or_default()) {
                let read = input_reads(&body.get("expr").map(py_str).unwrap_or_default());
                out.extend(read.intersection(&took).cloned());
            }
        }
    }
    out
}

/// An argument a transition or a cascade clause passes: its expression, its
/// loops' items, the type and transition it reaches, the input, and where it
/// is given.
type Passed = (String, Env, Option<String>, String, String, Path);

/// The module's types, those checked before it, and the observation kinds
/// they declare, as check 10 reads them.
struct Personal<'a> {
    kinds: &'a Obj,
    fields: BTreeMap<String, Value>,
}

impl Personal<'_> {
    fn personal(&self, u: &str, m: &str) -> bool {
        let t = self.kinds.get(u).or_else(|| self.fields.get(u));
        t.and_then(|t| obj(t, "attributes").get(m))
            .is_some_and(|s| flag(s, "personal"))
    }

    /// What personal value an expression carries, or none.
    fn why(
        &self,
        tn: &str,
        x: Option<&Value>,
        text: &str,
        env: &Env,
        inputs: &BTreeSet<String>,
        seen: &BTreeSet<(String, String)>,
    ) -> Option<String> {
        if let Some(n) = input_reads(text).intersection(inputs).next() {
            return Some(format!("the personal input '{n}'"));
        }
        let read = reads(text);
        if let Some(n) = read.iter().find(|n| self.personal(tn, n)) {
            return Some(format!("the personal {tn}.{n}"));
        }
        // step 2 refuses what does not parse
        let tree = parse(text).ok()?;
        let mut walker = OptionalReads::new(self.kinds, tn, x);
        walker.through_tests = true;
        walker.walk(&tree, env);
        if let Some((u, m)) = walker.members.iter().find(|(u, m)| self.personal(u, m)) {
            return Some(format!("the personal {u}.{m}"));
        }
        let own = self.kinds.get(tn).unwrap_or(&super::doc::NULL);
        let mut derived = walker.derived.clone();
        for n in &read {
            if obj(own, "derived_attributes").contains_key(n) {
                derived.insert((tn.to_string(), n.clone()));
            }
        }
        derived.into_iter().find_map(|(u, d)| {
            let pair = (u.clone(), d.clone());
            if seen.contains(&pair) {
                return None;
            }
            let mut deeper = seen.clone();
            deeper.insert(pair);
            let expr = obj(&self.kinds[u.as_str()], "derived_attributes")
                .get(&d)
                .and_then(|s| s.get("expression"))
                .map(py_str)
                .unwrap_or_default();
            self.why(&u, None, &expr, &Env::new(), &BTreeSet::new(), &deeper)
                .map(|_| format!("the personal value in the derived {u}.{d}"))
        })
    }

    /// Each composite end of a type, its own or inherited, with its cascade
    /// clauses and where they are given.
    fn parts(&self, tn: &str) -> Vec<(Value, Vec<Value>, Path)> {
        let t = self.kinds.get(tn).unwrap_or(&super::doc::NULL);
        let mut out: Vec<(Value, Vec<Value>, Path)> = obj(t, "attributes")
            .iter()
            .filter(|(_, spec)| text(spec, "aggregation") == Some("composite"))
            .map(|(an, spec)| {
                (
                    spec.clone(),
                    list(spec, "cascade").to_vec(),
                    segs!["types", tn, "attributes", an],
                )
            })
            .collect();
        let mut inherited: Obj = Default::default();
        for b in bases(tn, self.kinds).iter().rev() {
            for (an, spec) in obj(&self.kinds[b.as_str()], "attributes") {
                if text(spec, "aggregation") == Some("composite") {
                    inherited.insert(an.clone(), spec.clone());
                }
            }
        }
        for (an, ip) in obj(t, "inherited_parts") {
            if let Some(spec) = inherited.get(an) {
                out.push((
                    spec.clone(),
                    list(ip, "cascade").to_vec(),
                    segs!["types", tn, "inherited_parts", an],
                ));
            }
        }
        out
    }

    /// Each argument a transition passes by a `call` or a `create`.
    fn passes(&self, tn: &str, x: &Value) -> Vec<Passed> {
        let t = self.kinds.get(tn).unwrap_or(&super::doc::NULL);
        let mut out = Vec::new();
        for step in walk(x.get("effect")) {
            if !matches!(step.kind, "call" | "create") {
                continue;
            }
            let body = &step.st[step.kind];
            let env = binders(self.kinds, tn, x, &step.path);
            let k = if step.kind == "call" {
                call_target(self.kinds, t, tn, x, body, &env)
            } else {
                body.get("type").map(py_str)
            };
            let ct = body.get("transition").map(py_str).unwrap_or_default();
            for (j, e) in obj(body, "inputs") {
                out.push((
                    py_str(e),
                    env.clone(),
                    k.clone(),
                    ct.clone(),
                    j.clone(),
                    ext(segs!["effect"], step.path.clone()),
                ));
            }
        }
        out
    }

    /// Each argument a cascade clause on a transition passes, where the clause
    /// is.
    fn cascades(&self, tn: &str, xn: &str) -> Vec<Passed> {
        let mut out = Vec::new();
        for (spec, clauses, here) in self.parts(tn) {
            for (i, c) in clauses.iter().enumerate() {
                if !list(c, "on").iter().any(|w| py_str(w) == xn) {
                    continue;
                }
                let ct = c.get("transition").map(py_str).unwrap_or_default();
                for (j, e) in obj(c, "inputs") {
                    out.push((
                        py_str(e),
                        Env::new(),
                        Some(target_of(&spec)),
                        ct.clone(),
                        j.clone(),
                        ext(&here, segs!["cascade", i]),
                    ));
                }
            }
        }
        out
    }

    /// Where input `j` of `k.ct` reaches a non-personal attribute, or none.
    fn leak(
        &self,
        k: &str,
        ct: &str,
        j: &str,
        seen: &BTreeSet<(String, String, String)>,
    ) -> Option<String> {
        let t = self.kinds.get(k)?;
        let x = obj(t, "transitions").get(ct).filter(|x| !x.is_null())?;
        let key = (k.to_string(), ct.to_string(), j.to_string());
        if seen.contains(&key) || personal_inputs(self.kinds, k, x).contains(j) {
            return None;
        }
        let attrs = obj(t, "attributes");
        let attribute_input = strings(x, "required_inputs")
            .iter()
            .chain(&strings(x, "optional_inputs"))
            .any(|n| n == j);
        if attribute_input && attrs.contains_key(j) {
            return Some(format!("writes it to {k}.{j}, which is not personal"));
        }
        for step in walk(x.get("effect")) {
            if !matches!(step.kind, "assign" | "add") {
                continue;
            }
            let body = &step.st[step.kind];
            let loc = body.get("location").map(py_str).unwrap_or_default();
            if input_reads(&body.get("expr").map(py_str).unwrap_or_default()).contains(j)
                && attrs.contains_key(&loc)
                && !self.personal(k, &loc)
            {
                return Some(format!("writes it to {k}.{loc}, which is not personal"));
            }
        }
        let mut deeper = seen.clone();
        deeper.insert(key);
        self.passes(k, x)
            .into_iter()
            .chain(self.cascades(k, ct))
            .find_map(|(e, _env, k2, ct2, j2, _where)| {
                let k2 = k2?;
                if !input_reads(&e).contains(j) {
                    return None;
                }
                self.leak(&k2, &ct2, &j2, &deeper)
                    .map(|inner| format!("passes it to {k2}.{ct2}, which {inner}"))
            })
    }
}

/// A personal value MUST NOT reach a non-personal attribute (the model's
/// check 10, ADR-0078): by a write of the effect, by an argument to a `call`
/// or a `create`, or a cascade clause's, that the transition it reaches
/// writes to one, however many calls on, or by a migration's backfill
/// (`check-flows.py`'s `personal_errors`).
fn personal_errors(doc: &Value, library: &Obj) -> Found {
    let mut kinds = library.clone();
    for (tn, t) in types(doc) {
        kinds.insert(tn, t.clone());
    }
    let mut fields: BTreeMap<String, Value> = BTreeMap::new();
    for t in kinds.values() {
        for o in obj(t, "observations").values() {
            if let Some(k) = o.get("kind") {
                fields.insert(py_str(k), o.clone());
            }
        }
    }
    let check = Personal {
        kinds: &kinds,
        fields,
    };
    let none = BTreeSet::new();
    let leaks = |what: String| {
        format!("{what}; a personal value reaches only a personal attribute (the model's check 10)")
    };
    let mut out = Found::new();
    for (tn, t) in types(doc) {
        let attrs = obj(t, "attributes");
        for (xn, x) in obj(t, "transitions") {
            let base = segs!["types", &tn, "transitions", xn];
            let inputs = personal_inputs(&kinds, &tn, x);
            for step in walk(x.get("effect")) {
                if !matches!(step.kind, "assign" | "add") {
                    continue;
                }
                let body = &step.st[step.kind];
                let loc = body.get("location").map(py_str).unwrap_or_default();
                if !attrs.contains_key(&loc) || check.personal(&tn, &loc) {
                    continue;
                }
                let env = binders(&kinds, &tn, x, &step.path);
                let expr = body.get("expr").map(py_str).unwrap_or_default();
                if let Some(carried) = check.why(&tn, Some(x), &expr, &env, &inputs, &none) {
                    out.push(finding(
                        ext(&base, ext(segs!["effect"], step.path.clone())),
                        "names",
                        leaks(format!(
                            "{tn}.{xn} writes {carried} to '{loc}', which is not personal"
                        )),
                    ));
                }
            }
            for (e, env, k, ct, j, where_) in check.passes(&tn, x) {
                let Some(k) = k else { continue };
                let Some(carried) = check.why(&tn, Some(x), &e, &env, &inputs, &none) else {
                    continue;
                };
                if let Some(reached) = check.leak(&k, &ct, &j, &BTreeSet::new()) {
                    out.push(finding(
                        ext(&base, where_),
                        "names",
                        leaks(format!(
                            "{tn}.{xn} passes {carried} to {k}.{ct}'s input '{j}', which {reached}"
                        )),
                    ));
                }
            }
            for (e, _env, k, ct, j, where_) in check.cascades(&tn, xn) {
                let Some(k) = k else { continue };
                let Some(carried) = check.why(&tn, Some(x), &e, &Env::new(), &inputs, &none) else {
                    continue;
                };
                if let Some(reached) = check.leak(&k, &ct, &j, &BTreeSet::new()) {
                    out.push(finding(
                        where_,
                        "names",
                        leaks(format!(
                            "{tn}.{xn} cascades {carried} to {k}.{ct}'s input '{j}', which {reached}"
                        )),
                    ));
                }
            }
        }
    }
    let migration = doc.get("migration").cloned().unwrap_or(Value::Null);
    for (tn, given) in obj(&migration, "backfill") {
        for (a, e) in as_obj(given) {
            if check.personal(tn, a) {
                continue;
            }
            if let Some(carried) =
                check.why(tn, None, &py_str(e), &Env::new(), &BTreeSet::new(), &none)
            {
                out.push(finding(
                    segs!["migration", "backfill", tn, a],
                    "names",
                    leaks(format!(
                        "the backfill of {tn}.{a} reads {carried}, and {a} is not personal; a backfill is a write"
                    )),
                ));
            }
        }
    }
    // a metric over objects reads no personal value through a reference or
    // a derived attribute either, where check 56, reading its item's own
    // members, passed `d.owner.name`, `max(d.owner.salary)` and
    // `d.owner_name`, and an agent's metric read answered each person's name
    // and salary (sixteenth production review N6, ADR-0203 decision 2)
    fn chains(e: &Expr, item: &str, out: &mut Vec<Vec<String>>) {
        if let Expr::Path(..) = e {
            let mut members = Vec::new();
            let mut base = e;
            while let Expr::Path(b, m) = base {
                members.insert(0, m.clone());
                base = b;
            }
            if matches!(base, Expr::Name(n) if n == item) {
                out.push(members);
            }
        }
        for c in super::children_of(e) {
            chains(c, item, out);
        }
    }
    // a copy of an identity is the identity, personal where an actor type
    // marks its own so (ADR-0203 decision 1), where a metric grouped by an
    // approver set from actor.id answered each person's address (the
    // audit's personal data R8)
    let identities_personal = check.kinds.values().any(|k| {
        obj(k, "attributes")
            .values()
            .any(|spec| spec.get("actor_kind").is_some() && flag(spec, "personal"))
    });
    let reaches = |tn: &str, members: &[String]| -> Option<String> {
        let mut u = tn.to_string();
        for (i, m) in members.iter().enumerate() {
            let k = check.kinds.get(&u).cloned().unwrap_or(Value::Null);
            if let Some(spec) = obj(&k, "attributes").get(m).filter(|s| s.is_object()) {
                if identities_personal
                    && str_or(spec, "type", "").trim_end_matches("[]") == "identity"
                {
                    return Some(format!("the identity {u}.{m}, personal as an actor's is"));
                }
                if flag(spec, "personal") {
                    return (i > 0).then(|| format!("the personal {u}.{m}"));
                }
                if let Some(r) = spec
                    .get("reference")
                    .filter(|r| super::doc::truthy(Some(r)))
                {
                    u = py_str(r).trim_end_matches(['[', ']']).to_string();
                    continue;
                }
                return None;
            }
            if let Some(d) = obj(&k, "derived_attributes").get(m) {
                let text = d.get("expression").map(py_str).unwrap_or_default();
                return check
                    .why(&u, None, &text, &Env::new(), &BTreeSet::new(), &none)
                    .map(|w| format!("the derived {u}.{m}, which reads {w}"));
            }
            return None;
        }
        None
    };
    for (tn, t) in types(doc) {
        for (mn, m) in obj(t, "metrics") {
            let item = m.get("item").map(py_str).unwrap_or_default();
            let source = m
                .get("source")
                .map_or_else(|| "objects".to_string(), py_str);
            if !m.is_object() || source != "objects" || item.is_empty() {
                continue;
            }
            let mut parts: Vec<(Vec<String>, Option<&Value>)> = vec![
                (vec!["expression".to_string()], m.get("expression")),
                (vec!["filter".to_string()], m.get("filter")),
            ];
            for (d, e) in obj(m, "dimensions") {
                parts.push((vec!["dimensions".to_string(), d.clone()], Some(e)));
            }
            for (key, text) in parts {
                let Some(text) = text.filter(|t| !t.is_null()) else {
                    continue;
                };
                let squashed = py_str(text)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                // step 2 refuses what does not parse
                let Ok(tree) = parse(&squashed) else {
                    continue;
                };
                let mut found = Vec::new();
                chains(&tree, &item, &mut found);
                let said: BTreeSet<String> = found
                    .iter()
                    .filter_map(|members| {
                        reaches(&tn, members).map(|reached| {
                            format!(
                                "metric {mn} reads {item}.{}, which reaches {reached}; a metric reads no personal value, through a reference or a derived attribute either (ADR-0203)",
                                members.join(".")
                            )
                        })
                    })
                    .collect();
                for w in said {
                    let mut at = segs!["types", tn.as_str(), "metrics", mn.as_str()];
                    at.extend(key.iter().map(|k| Seg::Key(k.clone())));
                    out.push(finding(at, "names", w));
                }
            }
        }
    }
    out
}

/// What an expression reads of the clock itself: `now`; `time_in`, which
/// counts a current span up to now; or a span's `duration`, which does the
/// same, in an aggregate over this object's intervals (`declaration-syntax.md`
/// §6.9, §8.3). `entered_at` reads when a value was entered, not the clock
/// (`check-flows.py`'s `clock_read`).
fn clock_read(e: &Expr, spans: &BTreeSet<String>) -> Option<&'static str> {
    match e {
        Expr::Name(n) => (n == "now").then_some("now"),
        Expr::Call(f, _) if matches!(&**f, Expr::Name(n) if n == "time_in") => Some("time_in"),
        Expr::Path(b, m)
            if m == "duration" && matches!(&**b, Expr::Name(n) if spans.contains(n)) =>
        {
            Some("a span's duration")
        }
        Expr::Agg {
            var: Some(var),
            collection: Some(coll),
            filter,
            body,
            ..
        } => {
            let head = match &**coll {
                Expr::Call(f, _) => &**f,
                other => other,
            };
            let over_spans = matches!(head, Expr::Path(b, m)
                if m == "intervals" && matches!(&**b, Expr::Name(n) if n == "this"));
            let mut inner = spans.clone();
            if over_spans {
                inner.insert(var.clone());
            } else {
                inner.remove(var);
            }
            clock_read(coll, spans)
                .or_else(|| filter.as_deref().and_then(|w| clock_read(w, &inner)))
                .or_else(|| body.as_deref().and_then(|b| clock_read(b, &inner)))
        }
        _ => children(e).into_iter().find_map(|c| clock_read(c, spans)),
    }
}

/// An invariant MUST NOT read the clock, since the clock moves without a
/// write and no check after one can enforce it: `now`, `time_in` or a span's
/// duration, or a derived attribute that does, of its own type or of another
/// reached through a path, however many derivations deep (the model's check
/// 32, `declaration-syntax.md` §8.3). An observation kind's invariants are
/// check 54's (`check-flows.py`'s `clock_errors`).
fn clock_errors(doc: &Value, library: &Obj) -> Found {
    let mut kinds = library.clone();
    for (tn, t) in types(doc) {
        kinds.insert(tn, t.clone());
    }
    fn why(
        kinds: &Obj,
        tn: &str,
        source: &str,
        seen: &BTreeSet<(String, String)>,
    ) -> Option<String> {
        // step 2 refuses what does not parse
        let tree = parse(source).ok()?;
        if let Some(direct) = clock_read(&tree, &BTreeSet::new()) {
            return Some(
                match direct {
                    "now" => "reads now, the clock",
                    "time_in" => "reads time_in, which counts to now",
                    _ => "reads a span's duration, which a current span counts to now",
                }
                .to_string(),
            );
        }
        let mut walker = OptionalReads::new(kinds, tn, None);
        walker.walk(&tree, &Env::new());
        walker.derived.iter().find_map(|(u, d)| {
            let pair = (u.clone(), d.clone());
            if seen.contains(&pair) {
                return None;
            }
            let mut deeper = seen.clone();
            deeper.insert(pair);
            let expr = obj(&kinds[u.as_str()], "derived_attributes")
                .get(d)
                .and_then(|s| text(s, "expression"))
                .unwrap_or_default()
                .to_string();
            why(kinds, u, &expr, &deeper)
                .map(|_| format!("reads {u}.{d}, a derived attribute that reads the clock"))
        })
    }
    let mut out = Found::new();
    for (tn, t) in types(doc) {
        for (i, spec) in obj(t, "invariants") {
            let expr = text(spec, "expression").unwrap_or_default().to_string();
            if let Some(reason) = why(&kinds, &tn, &expr, &BTreeSet::new()) {
                out.push(finding(
                    segs!["types", &tn, "invariants", i, "expression"],
                    "names",
                    format!(
                        "{tn}'s invariant '{i}' {reason}; the clock moves without a write, so no check after one can enforce it, and a rule about time is a guard (the model's check 32)"
                    ),
                ));
            }
        }
    }
    out
}

/// A transition of a module's type, by its type and name.
type Node = (String, String);

/// What a transition causes: each transition, with where its step or clause is.
type Causes<'a> = &'a dyn Fn(&str, &str) -> Vec<(Node, Path)>;

/// A transition causes others: by a `call` or a `create` among its effect's
/// steps, a loop's included, and by a cascade clause on it of a part its type
/// gives, its own or one it inherits. Transitions MUST NOT cause each other in
/// a cycle, nor one cause itself, on its own object or another of its type
/// (the model's check 12, ADR-0019). A transition no type of the module
/// declares is left to the checks that name it, and an imported module cannot
/// cause one of this module's, so no cycle passes through it. A cycle is
/// reported at the step or the clause that closes it, once for each set of
/// transitions a cycle passes through (`check-flows.py`'s
/// `cascade_cycle_errors`).
fn cascade_cycle_errors(doc: &Value, library: &Obj) -> Found {
    let module: Obj = types(doc)
        .into_iter()
        .map(|(tn, t)| (tn, t.clone()))
        .collect();
    let mut kinds = library.clone();
    for (k, v) in &module {
        kinds.insert(k.clone(), v.clone());
    }
    let declares = |k: &str, xn: &str| {
        module
            .get(k)
            .is_some_and(|t| obj(t, "transitions").contains_key(xn))
    };
    let causes = |tn: &str, xn: &str| -> Vec<(Node, Path)> {
        let t = &module[tn];
        let x = &obj(t, "transitions")[xn];
        let mut out = Vec::new();
        for step in walk(x.get("effect")) {
            let where_ = ext(
                segs!["types", tn, "transitions", xn, "effect"],
                step.path.clone(),
            );
            let body = &step.st[step.kind];
            let ct = body.get("transition").map(py_str).unwrap_or_default();
            let k = match step.kind {
                "call" => call_target(&kinds, t, tn, x, body, &binders(&kinds, tn, x, &step.path)),
                "create" => body.get("type").map(py_str),
                _ => None,
            };
            if let Some(k) = k.filter(|k| declares(k, &ct)) {
                out.push(((k, ct), where_));
            }
        }
        let mut parts: Vec<(Value, Vec<Value>, Path)> = obj(t, "attributes")
            .iter()
            .filter(|(_, spec)| text(spec, "aggregation") == Some("composite"))
            .map(|(an, spec)| {
                (
                    spec.clone(),
                    list(spec, "cascade").to_vec(),
                    segs!["types", tn, "attributes", an],
                )
            })
            .collect();
        let mut inherited: Obj = Default::default();
        for b in bases(tn, &kinds).iter().rev() {
            for (an, spec) in obj(&kinds[b.as_str()], "attributes") {
                if text(spec, "aggregation") == Some("composite") {
                    inherited.insert(an.clone(), spec.clone());
                }
            }
        }
        for (an, ip) in obj(t, "inherited_parts") {
            if let Some(spec) = inherited.get(an) {
                parts.push((
                    spec.clone(),
                    list(ip, "cascade").to_vec(),
                    segs!["types", tn, "inherited_parts", an],
                ));
            }
        }
        for (spec, clauses, here) in parts {
            let part = target_of(&spec);
            for (i, c) in clauses.iter().enumerate() {
                let ct = c.get("transition").map(py_str).unwrap_or_default();
                if list(c, "on").iter().any(|w| py_str(w) == xn) && declares(&part, &ct) {
                    out.push(((part.clone(), ct), ext(&here, segs!["cascade", i])));
                }
            }
        }
        out
    };
    struct Search<'a> {
        causes: Causes<'a>,
        open: BTreeSet<Node>,
        closed: BTreeSet<Node>,
        reported: BTreeSet<BTreeSet<Node>>,
        out: Found,
    }
    impl Search<'_> {
        fn visit(&mut self, node: &Node, trail: &mut Vec<Node>) {
            self.open.insert(node.clone());
            trail.push(node.clone());
            for (next, where_) in (self.causes)(&node.0, &node.1) {
                if self.open.contains(&next) {
                    let at = trail.iter().position(|n| *n == next).unwrap_or(0);
                    let mut ring = trail[at..].to_vec();
                    ring.push(next);
                    if self.reported.insert(ring.iter().cloned().collect()) {
                        let names: Vec<String> =
                            ring.iter().map(|(u, e)| format!("{u}.{e}")).collect();
                        self.out.push(finding(
                            where_,
                            "names",
                            format!(
                                "a transition causes itself, by a call, a create or a cascade: {}; no transition may cause itself, on its own object or another (the model's check 12)",
                                names.join(" → ")
                            ),
                        ));
                    }
                } else if !self.closed.contains(&next) {
                    self.visit(&next, trail);
                }
            }
            trail.pop();
            self.open.remove(node);
            self.closed.insert(node.clone());
        }
    }
    let mut search = Search {
        causes: &causes,
        open: BTreeSet::new(),
        closed: BTreeSet::new(),
        reported: BTreeSet::new(),
        out: Found::new(),
    };
    for (tn, t) in &module {
        for xn in obj(t, "transitions").keys() {
            let node = (tn.clone(), xn.clone());
            if !search.open.contains(&node) && !search.closed.contains(&node) {
                search.visit(&node, &mut Vec::new());
            }
        }
    }
    search.out
}

/// A composite end's cascade clauses name transitions of the whole and of
/// the part, with the part's inputs, and every final transition of the whole
/// is covered by a clause or survived, never both (check 37).
#[allow(clippy::too_many_arguments)]
fn part_coverage(
    tn: &str,
    t: &Value,
    an: &str,
    target: &str,
    part: &Value,
    clauses: &[Value],
    survives: Option<&Value>,
    here: &Path,
    finals: &BTreeSet<String>,
) -> Found {
    let mut out = Found::new();
    let mut covered: BTreeSet<String> = BTreeSet::new();
    let transitions = obj(t, "transitions");
    for (i, c) in clauses.iter().enumerate() {
        let where_ = ext(here, segs!["cascade", i]);
        let on: Vec<String> = list(c, "on").iter().map(py_str).collect();
        for w in &on {
            if !transitions.contains_key(w) {
                out.push(finding(
                    where_.clone(),
                    "names",
                    format!("{tn}.{an} cascades on '{w}', which is not a transition of {tn}"),
                ));
            }
        }
        covered.extend(on);
        let ct = c.get("transition").map(py_str).unwrap_or_default();
        let called = obj(part, "transitions").get(&ct).filter(|x| !x.is_null());
        let Some(called) = called.filter(|x| kind(x) != "initial") else {
            out.push(finding(
                where_.clone(),
                "names",
                format!("{tn}.{an} cascades to {target}.{ct}, which is not a transition of {target} that acts on a part"),
            ));
            continue;
        };
        if kind(called) == "assertion" {
            out.push(finding(
                where_.clone(),
                "names",
                format!("{tn}.{an} cascades to {target}.{ct}, an assertion, which only a request takes; an override is requested, never caused (ADR-0163)"),
            ));
            continue;
        }
        let given: BTreeSet<String> = keys(c, "inputs").into_iter().collect();
        for i in given.difference(&taken(called)) {
            out.push(finding(
                where_.clone(),
                "names",
                format!("{tn}.{an} passes '{i}' to {target}.{ct}, which takes no input '{i}'"),
            ));
        }
        for i in required_input_names(called).difference(&given) {
            if !obj(called, "inputs")
                .get(i)
                .is_some_and(|s| has(s, "default"))
            {
                out.push(finding(
                    where_.clone(),
                    "names",
                    format!("{tn}.{an} cascades to {target}.{ct} without the input '{i}', which it requires"),
                ));
            }
        }
    }
    let kept: BTreeSet<String> = match survives {
        Some(Value::Bool(true)) => transitions.keys().cloned().collect(),
        Some(Value::Array(a)) => a.iter().map(py_str).collect(),
        _ => BTreeSet::new(),
    };
    for w in covered.intersection(&kept) {
        out.push(finding(
            here.clone(),
            "names",
            format!("{tn}.{an} both cascades on and survives '{w}'"),
        ));
    }
    for (xn, x) in transitions {
        let into: Vec<String> = match kind(x) {
            "external" => vec![x.get("to").map(py_str).unwrap_or_default()],
            "assertion" => match x.get("to") {
                Some(Value::Array(a)) => a.iter().map(py_str).filter(|s| !s.is_empty()).collect(),
                Some(Value::Null) | None => Vec::new(),
                Some(v) => vec![py_str(v)]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect(),
            },
            _ => Vec::new(),
        };
        if let Some(f) = into.iter().find(|s| finals.contains(*s))
            && !covered.contains(xn)
            && !kept.contains(xn)
        {
            out.push(finding(
                here.clone(),
                "names",
                format!("{tn}.{xn} enters the final state {f}, and {tn}.{an} neither cascades on it nor survives it"),
            ));
        }
    }
    out
}

/// The inputs of a transition that take an event.
fn event_inputs(target: &Value, x: &Value) -> BTreeSet<String> {
    let attrs = obj(target, "attributes");
    let mut named: BTreeSet<String> = obj(x, "inputs")
        .iter()
        .filter(|(_, s)| text(s, "type") == Some("event"))
        .map(|(i, _)| i.clone())
        .collect();
    for a in strings(x, "required_inputs")
        .into_iter()
        .chain(strings(x, "optional_inputs"))
    {
        if attrs
            .get(&a)
            .is_some_and(|s| text(s, "type") == Some("event"))
        {
            named.insert(a);
        }
    }
    named
}

/// An event-typed input accepts only `this_event` (check 25).
fn event_errors(
    where_: &Path,
    tn: &str,
    xn: &str,
    target_name: &str,
    x: &Value,
    given: &Value,
    target: &Value,
) -> Found {
    let given_obj = as_obj(given);
    event_inputs(target, x)
        .into_iter()
        .filter(|i| given_obj.contains_key(i))
        .filter_map(|i| {
            let value = squash(&py_str(&given_obj[&i]));
            (value != "this_event").then(|| {
                finding(
                    where_.clone(),
                    "names",
                    format!("{tn}.{xn} passes {value} to {target_name}'s event input '{i}', which accepts only this_event (check 25)"),
                )
            })
        })
        .collect()
}

/// A create names a type of the module, or an imported one, and an initial
/// transition of it, and supplies that transition's required inputs.
fn create_errors(
    doc: &Value,
    library: &Obj,
    where_: &Path,
    tn: &str,
    xn: &str,
    c: &Value,
) -> Found {
    let ty = c.get("type").map(py_str).unwrap_or_default();
    if !obj(doc, "types").contains_key(&ty) && !imported(doc).contains(&ty) {
        return vec![finding(
            where_.clone(),
            "names",
            format!(
                "{tn}.{xn} creates a {ty}, which is neither declared in this module nor imported"
            ),
        )];
    }
    let Some(target) = library.get(&ty) else {
        return Found::new();
    };
    let xt = c.get("transition").map(py_str).unwrap_or_default();
    let Some(t) = obj(target, "transitions")
        .get(&xt)
        .filter(|x| !x.is_null() && kind(x) == "initial")
    else {
        return vec![finding(
            where_.clone(),
            "names",
            format!(
                "{tn}.{xn} creates a {ty} by '{xt}', which is not an initial transition of {ty}"
            ),
        )];
    };
    let given: BTreeSet<String> = keys(c, "inputs").into_iter().collect();
    let mut out: Found = given
        .difference(&taken(t))
        .map(|i| {
            finding(
                where_.clone(),
                "names",
                format!("{tn}.{xn} passes '{i}' to {ty}.{xt}, which takes no input '{i}'"),
            )
        })
        .collect();
    out.extend(event_errors(
        where_,
        tn,
        xn,
        &format!("{ty}.{xt}"),
        t,
        c.get("inputs").unwrap_or(&Value::Null),
        target,
    ));
    for i in required_input_names(t).difference(&given) {
        if !obj(t, "inputs").get(i).is_some_and(|s| has(s, "default")) {
            out.push(finding(
                where_.clone(),
                "names",
                format!(
                    "{tn}.{xn} creates a {ty} without the input '{i}', which {ty}.{xt} requires"
                ),
            ));
        }
    }
    out
}

/// The type each loop enclosing an effect step binds its item to, where the
/// loop ranges over objects of a known type.
pub fn binders(library: &Obj, tn: &str, x: &Value, step_path: &[Seg]) -> Env {
    let mut env = Env::new();
    let mut steps: Vec<Value> = list(x, "effect").to_vec();
    let mut depth = 0;
    while depth + 1 < step_path.len() {
        let Seg::Index(i) = step_path[depth] else {
            break;
        };
        let Some(spec) = steps.get(i).and_then(|s| s.get("foreach")).cloned() else {
            break;
        };
        let mut at = None;
        if let Some(array) = spec.get("array") {
            let text = py_str(array);
            if let Ok(tree) = parse(&text) {
                at = OptionalReads::new(library, tn, Some(x)).walk(&tree, &env);
            }
            if at.is_none() && library.contains_key(&text) {
                at = Some(format!("{text}[]"));
            }
        }
        let item = spec.get("item").map(py_str).unwrap_or_default();
        env.insert(
            item,
            at.and_then(|t| t.strip_suffix("[]").map(str::to_string)),
        );
        steps = list(&spec, "steps").to_vec();
        depth += 3;
    }
    env
}

static CALL_ATTRIBUTE: LazyLock<Regex> = LazyLock::new(|| re(r"^(?:this\.)?([a-z][a-z0-9_]*)$"));
static CALL_INPUT: LazyLock<Regex> = LazyLock::new(|| re(r"^inputs\.([a-z][a-z0-9_]*)$"));

/// The type of a call's target, read from an attribute, an input, or a path
/// from either or from a loop's item, the loops' items typed by what they
/// range over; none where it cannot be known, or is a set or no type known.
pub(crate) fn call_target(
    library: &Obj,
    t: &Value,
    tn: &str,
    x: &Value,
    c: &Value,
    env: &Env,
) -> Option<String> {
    let target = squash(&c.get("target").map(py_str).unwrap_or_default());
    let mut spec: Option<Value> = None;
    if let Some(m) = CALL_ATTRIBUTE.captures(&target)
        && let Some(s) = obj(t, "attributes").get(&m[1])
    {
        spec = Some(s.clone());
    }
    if let Some(m) = CALL_INPUT.captures(&target) {
        spec = obj(x, "inputs")
            .get(&m[1])
            .filter(|s| super::doc::truthy(Some(s)))
            .or_else(|| obj(t, "attributes").get(&m[1]))
            .cloned();
    }
    let mut k = spec
        .as_ref()
        .and_then(|s| s.get("reference"))
        .map(py_str)
        .filter(|r| !r.is_empty());
    if k.is_none() {
        k = parse(&target)
            .ok()
            .and_then(|tree| OptionalReads::new(library, tn, Some(x)).walk(&tree, env));
    }
    k.filter(|k| !k.ends_with("[]") && library.contains_key(k))
}

/// A call names a transition of its target's type that acts on an existing
/// object, and passes its inputs; a target whose type cannot be known is
/// left to the publish checks.
#[allow(clippy::too_many_arguments)]
fn call_errors(
    library: &Obj,
    where_: &Path,
    t: &Value,
    tn: &str,
    xn: &str,
    x: &Value,
    c: &Value,
    env: &Env,
) -> Found {
    let Some(k) = call_target(library, t, tn, x, c, env) else {
        return Found::new();
    };
    let other = &library[&k];
    let ct = c.get("transition").map(py_str).unwrap_or_default();
    let Some(called) = obj(other, "transitions").get(&ct).filter(|x| !x.is_null()) else {
        return vec![finding(
            where_.clone(),
            "names",
            format!("{tn}.{xn} calls {k}.{ct}, which is not a transition of {k}"),
        )];
    };
    if kind(called) == "initial" {
        return vec![finding(
            where_.clone(),
            "names",
            format!(
                "{tn}.{xn} calls {k}.{ct}, an initial transition; create an object with create"
            ),
        )];
    }
    if kind(called) == "assertion" {
        return vec![finding(
            where_.clone(),
            "names",
            format!(
                "{tn}.{xn} calls {k}.{ct}, an assertion, which only a request takes; an override is requested, never caused (ADR-0163)"
            ),
        )];
    }
    let given: BTreeSet<String> = keys(c, "inputs").into_iter().collect();
    let mut out: Found = given
        .difference(&taken(called))
        .map(|i| {
            finding(
                where_.clone(),
                "names",
                format!("{tn}.{xn} passes '{i}' to {k}.{ct}, which takes no input '{i}'"),
            )
        })
        .collect();
    out.extend(event_errors(
        where_,
        tn,
        xn,
        &format!("{k}.{ct}"),
        called,
        c.get("inputs").unwrap_or(&Value::Null),
        other,
    ));
    for i in required_input_names(called).difference(&given) {
        if !obj(called, "inputs")
            .get(i)
            .is_some_and(|s| has(s, "default"))
        {
            out.push(finding(
                where_.clone(),
                "names",
                format!("{tn}.{xn} calls {k}.{ct} without the input '{i}', which it requires"),
            ));
        }
    }
    out
}

static PRESENT_LITERAL: LazyLock<Regex> =
    LazyLock::new(|| re(r#"^(?:-?\d+(\.\d+)?|"[^"]*"|[A-Z][A-Za-z0-9]*\.[A-Z][A-Z0-9_]*)$"#));
static NAME: LazyLock<Regex> = LazyLock::new(|| re(r"^[a-z][a-z0-9_]*$"));

/// Whether an assigned expression provably has a value: who acts is one,
/// since every request names its actor.
fn present(expr: &str, known: &BTreeSet<String>, inputs: &BTreeSet<String>) -> bool {
    if ["now", "actor.id", "actor.kind"].contains(&expr) || PRESENT_LITERAL.is_match(expr) {
        return true;
    }
    if NAME.is_match(expr) && !NOT_AN_ATTRIBUTE.contains(&expr) {
        return known.contains(expr);
    }
    CALL_INPUT
        .captures(expr)
        .is_some_and(|m| inputs.contains(&m[1]))
}

/// Whether an assigned path provably has a value: from an attribute known to
/// have one, or a required input, through references none of which is
/// optional, to an attribute that is not optional (`flow-format.md` §4.5), as
/// `cost_centre.fiscal_year` does; an attribute the path's types do not
/// declare themselves is not counted (`check-flows.py`'s `present_path`).
fn present_path(
    expr: &str,
    tn: &str,
    x: &Value,
    library: &Obj,
    known: &BTreeSet<String>,
    inputs: &BTreeSet<String>,
) -> bool {
    let parts: Vec<&str> = expr.split('.').collect();
    let start = usize::from(parts[0] == "inputs");
    if parts.len() < start + 2 || !parts[start..].iter().all(|p| NAME.is_match(p)) {
        return false;
    }
    let Some(t) = library.get(tn) else {
        return false;
    };
    let name = parts[start];
    let truthy = |s: &&Value| super::doc::truthy(Some(*s));
    let mut spec: Option<&Value> = if start == 1 {
        if !inputs.contains(name) {
            return false;
        }
        obj(x, "inputs")
            .get(name)
            .filter(truthy)
            .or_else(|| obj(t, "attributes").get(name))
    } else {
        if !known.contains(name) || NOT_AN_ATTRIBUTE.contains(&name) {
            return false;
        }
        obj(t, "attributes").get(name)
    };
    for seg in &parts[start + 1..] {
        let kind = spec.map(|s| str_or(s, "reference", "")).unwrap_or_default();
        if kind.is_empty() || kind.ends_with("[]") {
            return false;
        }
        let Some(other) = library.get(&kind) else {
            return false;
        };
        spec = obj(other, "attributes").get(*seg);
        if !spec.is_some_and(|s| !optional(s)) {
            return false;
        }
    }
    true
}

/// Every transition into a state sets each attribute the state requires.
fn required_errors(
    tn: &str,
    xn: &str,
    x: &Value,
    required: &BTreeMap<String, BTreeSet<String>>,
    always: &BTreeSet<String>,
    library: &Obj,
) -> Found {
    let base = segs!["types", tn, "transitions", xn];
    let mut out = Found::new();
    if matches!(kind(x), "assertion" | "erasure") {
        return out;
    }
    let src = sources(x);
    let to = x
        .get("to")
        .filter(|t| super::doc::truthy(Some(t)))
        .map(py_str);
    let into: Vec<String> = match &to {
        Some(t) => vec![t.clone()],
        None => src.clone(),
    };
    let cl: BTreeSet<String> = cleared(x).into_iter().collect();
    let none = BTreeSet::new();
    for s in &into {
        for a in required.get(s).unwrap_or(&none).intersection(&cl) {
            out.push(finding(
                ext(&base, segs!["effect"]),
                "required",
                format!("{tn}.{xn} clears '{a}', which state {s} requires"),
            ));
        }
    }
    if kind(x) == "internal" {
        return out;
    }
    let mut supplied: BTreeSet<String> = strings(x, "required_inputs").into_iter().collect();
    supplied.extend(always.iter().cloned());
    let kept: BTreeSet<String> = match src.split_first() {
        Some((first, rest)) => rest.iter().fold(
            required.get(first).cloned().unwrap_or_default(),
            |acc, s| {
                acc.intersection(required.get(s).unwrap_or(&none))
                    .cloned()
                    .collect()
            },
        ),
        None => BTreeSet::new(),
    };
    let assigned: BTreeMap<String, String> =
        assigns(x).into_iter().map(|(e, loc)| (loc, e)).collect();
    let to = to.unwrap_or_default();
    let mut known = supplied.clone();
    known.extend(kept.iter().cloned());
    for a in required.get(&to).unwrap_or(&none) {
        if supplied.contains(a) || (kept.contains(a) && !cl.contains(a)) {
            continue;
        }
        if assigned
            .get(a)
            .is_some_and(|e| present(e, &known, &required_input_names(x)))
        {
            continue;
        }
        if assigned
            .get(a)
            .is_some_and(|e| present_path(e, tn, x, library, &known, &required_input_names(x)))
        {
            continue;
        }
        let how = if src.is_empty() {
            "since an initial transition starts from no state".to_string()
        } else {
            format!("or require it in state {}", src.join(", "))
        };
        out.push(finding(
            ext(&base, segs!["to"]),
            "required",
            format!(
                "{tn}.{xn} enters {to} without setting '{a}', which {to} requires: make it a required input, assign it a value that is provably present, {how}"
            ),
        ));
    }
    out
}

/// What a write to an attribute must be, as far as a publish can tell.
fn written_kind(spec: Option<&Value>, enums: &BTreeSet<String>) -> Option<Vec<String>> {
    let spec = spec.filter(|s| super::doc::truthy(Some(s)))?;
    if let Some(r) = spec.get("reference") {
        let r = py_str(r);
        return if r.ends_with("[]") {
            None
        } else {
            Some(vec!["reference".into(), r])
        };
    }
    let typ = str_or(spec, "type", "");
    if enums.contains(&typ) {
        return Some(vec!["enumeration".into(), typ]);
    }
    if let Some(m) = MONEY_TYPE.captures(&typ) {
        return Some(vec!["money".into(), m[1].to_string()]);
    }
    if DECIMAL_TYPE.is_match(&typ) {
        return Some(vec!["decimal".into()]);
    }
    if typ == "counter" {
        return Some(vec!["int".into()]);
    }
    SIMPLE_TYPES.contains(&typ.as_str()).then(|| vec![typ])
}

/// The kind of an assigned value where a publish can tell it.
fn value_kind(
    e: &Expr,
    tn: &str,
    t: &Value,
    x: &Value,
    enums: &BTreeSet<String>,
) -> Option<Vec<String>> {
    let one = |s: &str| Some(vec![s.to_string()]);
    let attrs = obj(t, "attributes");
    match e {
        Expr::Str(_) => one("string"),
        Expr::Num(_) => one("number"),
        Expr::Call(f, _) if matches!(&**f, Expr::Name(n) if n == "date") => one("date"),
        Expr::Money(c, _) => Some(vec!["money".into(), c.clone()]),
        Expr::Bool(_) => one("bool"),
        Expr::Name(n) => match n.as_str() {
            "now" => one("timestamp"),
            "this" => Some(vec!["reference".into(), tn.to_string()]),
            k if ACTOR_KINDS.contains(&k) && !attrs.contains_key(k) => one("actor_kind"),
            k => written_kind(attrs.get(k), enums),
        },
        Expr::Path(base, m) => {
            let Expr::Name(b) = &**base else { return None };
            if enums.contains(b) {
                return Some(vec!["enumeration".into(), b.clone()]);
            }
            match b.as_str() {
                "this" => written_kind(attrs.get(m), enums),
                "actor" if m == "id" => one("identity"),
                "actor" if m == "kind" => one("actor_kind"),
                "inputs" => {
                    let mut spec = obj(x, "inputs").get(m).filter(|s| !s.is_null()).cloned();
                    if spec.is_none()
                        && (strings(x, "required_inputs").contains(m)
                            || strings(x, "optional_inputs").contains(m))
                    {
                        spec = attrs.get(m).cloned();
                    }
                    written_kind(spec.as_ref(), enums)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// What a loop's range counts to, where a publish can tell (ADR-0146).
fn range_kind(
    e: &Expr,
    tn: &str,
    t: &Value,
    x: &Value,
    enums: &BTreeSet<String>,
) -> Option<Vec<String>> {
    if let Expr::Num(n) = e {
        return Some(vec![
            if matches!(n, Number::Int(_)) {
                "int"
            } else {
                "decimal"
            }
            .to_string(),
        ]);
    }
    let attrs = obj(t, "attributes");
    let spec = match e {
        Expr::Name(m) => attrs.get(m).cloned(),
        Expr::Path(b, m) => match &**b {
            Expr::Name(n) if n == "this" => attrs.get(m).cloned(),
            Expr::Name(n) if n == "inputs" => obj(x, "inputs")
                .get(m)
                .filter(|s| super::doc::truthy(Some(s)))
                .or_else(|| attrs.get(m))
                .cloned(),
            _ => None,
        },
        _ => None,
    };
    if spec
        .as_ref()
        .is_some_and(|s| super::doc::truthy(Some(s)) && kind_text(s).ends_with("[]"))
    {
        return Some(vec!["set".into()]);
    }
    value_kind(e, tn, t, x, enums)
}

/// Whether a value of one kind may be written to a target of another (the
/// model's check 17).
fn fits(target: &[String], value: &[String], library: &Obj) -> bool {
    if target == value {
        return true;
    }
    if value == ["number"] && (target == ["int"] || target == ["decimal"]) {
        return true;
    }
    if target == ["decimal"] && value == ["int"] {
        return true;
    }
    if target.first().map(String::as_str) == Some("reference")
        && value.first().map(String::as_str) == Some("reference")
    {
        let mut seen = BTreeSet::new();
        let mut k = value.get(1).cloned();
        while let Some(kk) = k {
            if kk.is_empty() || !seen.insert(kk.clone()) {
                break;
            }
            if Some(&kk) == target.get(1) {
                return true;
            }
            k = library
                .get(&kk)
                .and_then(|t| t.get("extends"))
                .filter(|e| !e.is_null())
                .map(py_str);
        }
    }
    false
}

/// A type as check 24 infers it (`declaration-syntax.md` §8.1, §8.3).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Ty {
    /// A number literal, with its written scale, which a product adds
    /// (§8.3); in a sum or a comparison it takes the other operand's type.
    Number(u32),
    Int,
    /// A decimal, with its scale where it is known.
    Decimal(Option<u32>),
    Money(String),
    Duration,
    Timestamp,
    Date,
    Bool,
    Str,
    Identity,
    ActorKind,
    File,
    Category,
    Verdict,
    Reference(String),
    Objects(String),
    Enumeration(String),
    State(String),
    /// A set of values, of its element's type (the differential battery).
    Values(Box<Ty>),
}

impl Ty {
    /// The type's name as `check-flows.py`'s tuples spell it.
    fn tag(&self) -> &'static str {
        match self {
            Ty::Number(_) => "number",
            Ty::Int => "int",
            Ty::Decimal(_) => "decimal",
            Ty::Money(_) => "money",
            Ty::Duration => "duration",
            Ty::Timestamp => "timestamp",
            Ty::Date => "date",
            Ty::Bool => "bool",
            Ty::Str => "string",
            Ty::Identity => "identity",
            Ty::ActorKind => "actor_kind",
            Ty::File => "file",
            Ty::Category => "category",
            Ty::Verdict => "verdict",
            Ty::Reference(_) => "reference",
            Ty::Objects(_) => "objects",
            Ty::Enumeration(_) => "enumeration",
            Ty::State(_) => "state",
            Ty::Values(_) => "values",
        }
    }

    fn scalar(&self) -> bool {
        matches!(self, Ty::Int | Ty::Decimal(_) | Ty::Number(_))
    }

    /// A type as a finding names it (`check-flows.py`'s `type_text`).
    fn text(&self) -> String {
        match self {
            Ty::Decimal(Some(s)) => format!("a decimal of scale {s}"),
            Ty::Decimal(None) => "a decimal".into(),
            Ty::Money(c) => format!("money({c})"),
            Ty::Reference(t) => format!("an object of {t}"),
            Ty::Objects(t) => format!("a set of {t}"),
            Ty::Values(e) => match &**e {
                Ty::Enumeration(n) | Ty::Reference(n) => format!("a set of {n}"),
                other => format!("a set of {}", other.tag()),
            },
            Ty::Enumeration(e) => format!("a member of {e}"),
            Ty::State(t) => format!("a state of {t}"),
            Ty::Int => "an int".into(),
            Ty::Identity => "an identity".into(),
            Ty::ActorKind => "an actor kind".into(),
            other => format!("a {}", other.tag()),
        }
    }
}

// patterns compiled once, where each call compiled its own (ninth
// production review N-8)
static MONEY_TYPE: LazyLock<Regex> = LazyLock::new(|| re(r"^money\((\w+)\)$"));
static DECIMAL_TYPE: LazyLock<Regex> = LazyLock::new(|| re(r"^decimal\(\d+,\s*\d+\)$"));
static PLACE_N: LazyLock<Regex> = LazyLock::new(|| re(r"^n(:.*)?$"));
static PLACE_N_DIGITS: LazyLock<Regex> = LazyLock::new(|| re(r"^n(:\d+)?$"));
static DECIMAL_SCALE: LazyLock<Regex> = LazyLock::new(|| re(r"^decimal\(\s*\d+\s*,\s*(\d+)\s*\)$"));

/// An attribute's or an input's type as an expression reads it, a decimal
/// with its scale, a set of references as the objects it holds; none where
/// it cannot be told (`check-flows.py`'s `attribute_type`).
fn attribute_type(spec: Option<&Value>, enums: &BTreeSet<String>) -> Option<Ty> {
    let spec = spec.filter(|s| super::doc::truthy(Some(s)))?;
    let reference = ref_of(spec);
    if !reference.is_empty() {
        return Some(match reference.strip_suffix("[]") {
            Some(t) => Ty::Objects(t.to_string()),
            None => Ty::Reference(reference),
        });
    }
    let typ = spec.get("type").map(py_str).unwrap_or_default();
    // a set of values, of its element's type, where it typed as nothing, so
    // a set added, multiplied, ordered or negated published and was not run
    // (the differential battery, 2026-10-15)
    if let Some(base) = typ.strip_suffix("[]") {
        let element = serde_json::json!({ "type": base });
        return attribute_type(Some(&element), enums).map(|e| Ty::Values(Box::new(e)));
    }
    if let Some(m) = DECIMAL_SCALE.captures(&typ) {
        return Some(Ty::Decimal(m[1].parse().ok()));
    }
    let k = written_kind(Some(spec), enums)?;
    Some(match (k[0].as_str(), k.get(1)) {
        ("reference", Some(t)) => Ty::Reference(t.clone()),
        ("enumeration", Some(e)) => Ty::Enumeration(e.clone()),
        ("money", Some(c)) => Ty::Money(c.clone()),
        ("decimal", _) => Ty::Decimal(None),
        ("int", _) => Ty::Int,
        ("string", _) => Ty::Str,
        ("bool", _) => Ty::Bool,
        ("timestamp", _) => Ty::Timestamp,
        ("duration", _) => Ty::Duration,
        ("identity", _) => Ty::Identity,
        ("file", _) => Ty::File,
        ("date", _) => Ty::Date,
        ("actor_kind", _) => Ty::ActorKind,
        _ => return None,
    })
}

/// What every object has, by its type (§8.1).
fn object_member_type(m: &str) -> Option<Ty> {
    Some(match m {
        "open" => Ty::Bool,
        "id" => Ty::Identity,
        "created_at" | "recorded_from" => Ty::Timestamp,
        "created_by_kind" => Ty::ActorKind,
        "declaration_version" => Ty::Int,
        _ => return None,
    })
}

/// The types of a module's expressions as far as a publish can tell them,
/// and each rule of `declaration-syntax.md` §8.3 an expression breaks (the
/// model's check 24; `check-flows.py`'s `Typing`).
/// The most derived attributes one typing goes through, as an evaluation
/// goes through at most `eval::DERIVED_HOPS` (tenth production review).
const TYPING_DEPTH: usize = 64;

/// `tracked_attributes` of `check-flows.py`: an attribute whose every value
/// is timed, an enumeration's, or a single reference that stores its value,
/// one with no opposite, one marked stored, or one whose opposite end is a
/// set (ADR-0083).
fn tracked_attribute(t: &Value, a: &str, kinds: &Obj) -> bool {
    let Some(spec) = obj(t, "attributes").get(a) else {
        return false;
    };
    let typ = str_or(spec, "type", "");
    let reference = str_or(spec, "reference", "");
    if typ.chars().next().is_some_and(char::is_uppercase) && !typ.ends_with("[]") {
        return true;
    }
    if reference.is_empty() || reference.ends_with("[]") {
        return false;
    }
    let opposite = str_or(spec, "opposite", "");
    let far = kinds
        .get(&reference)
        .map(|k| obj(k, "attributes"))
        .and_then(|attrs| attrs.get(&opposite).cloned());
    spec.get("opposite").is_none()
        || flag(spec, "stored")
        || far.is_some_and(|f| str_or(&f, "reference", "").ends_with("[]"))
}

struct Typing<'a> {
    kinds: &'a Obj,
    enums: &'a BTreeSet<String>,
    categories: &'a BTreeSet<String>,
    evaluators: &'a BTreeSet<String>,
    broken: Vec<String>,
    /// Whether a flow function's argument is checked here: in the places
    /// check 58 does not reach through a transition's body, a derived
    /// attribute, an invariant, a uniqueness condition and a backfill.
    flow_names: bool,
    /// Each derived attribute's type, by its type and name, once typed: a
    /// chain of attributes each reading the one before twice typed each
    /// 2^n times, forty holding the dry run's slot past six minutes
    /// (eleventh production review).
    known: &'a std::cell::RefCell<BTreeMap<(String, String), Option<Ty>>>,
}

type TyEnv = BTreeMap<String, Option<Ty>>;

impl Typing<'_> {
    /// Why a member read through a reference is none its type has: an
    /// attribute, a derived attribute, an observation kind, what every object
    /// has, its flow data among it, its state or its referrers; none where it
    /// is one, or where nothing declares the type (D695, `check-flows.py`'s
    /// `undeclared`).
    fn undeclared(&self, tn: &str, m: &str) -> Option<String> {
        let t = self.kinds.get(tn)?;
        if m == "state"
            || m == "referrers"
            || object_member_type(m).is_some()
            || super::optional::BUILT_IN.contains(&m)
        {
            return None;
        }
        if obj(t, "attributes").contains_key(m)
            || obj(t, "derived_attributes").contains_key(m)
            || obj(t, "observations").contains_key(m)
        {
            return None;
        }
        if WITHDRAWN_MEMBERS.contains(&m) {
            return Some(format!(
                "it reads {m} of {tn}, which is withdrawn before 1.0.0, no store keeping it, and may return with a use (ADR-0210 decision 3)"
            ));
        }
        Some(format!("it reads {m} of {tn}, which {tn} does not declare"))
    }

    fn member(&self, tn: &str, m: &str, seen: &BTreeSet<(String, String)>) -> Option<Ty> {
        // the type read where it is, where each member cloned it whole
        static NONE: Value = Value::Null;
        let t = self.kinds.get(tn).unwrap_or(&NONE);
        if m == "state" {
            return Some(Ty::State(tn.to_string()));
        }
        if let Some(k) = object_member_type(m) {
            return Some(k);
        }
        if let Some(spec) = obj(t, "attributes").get(m) {
            return attribute_type(Some(spec), self.enums);
        }
        let pair = (tn.to_string(), m.to_string());
        if let Some(known) = self.known.borrow().get(&pair) {
            return known.clone();
        }
        // a derived attribute typed through at most TYPING_DEPTH others,
        // past which its type is unknown: each typed the whole chain below
        // it again, a chain of 1,000 taking 171 s and one of 3,000 deeper
        // than a stack (tenth production review)
        if let Some(derived) = obj(t, "derived_attributes").get(m)
            && !seen.contains(&pair)
            && seen.len() < TYPING_DEPTH
            && !super::late()
        {
            let text = squash(&derived.get("expression").map(py_str).unwrap_or_default());
            let tree = parse(&text).ok()?;
            let mut quiet = Typing {
                kinds: self.kinds,
                enums: self.enums,
                categories: self.categories,
                evaluators: self.evaluators,
                broken: Vec::new(),
                flow_names: false,
                known: self.known,
            };
            let mut deeper = seen.clone();
            deeper.insert(pair.clone());
            let ty = quiet.of(&tree, tn, None, &TyEnv::new(), &deeper);
            self.known.borrow_mut().insert(pair, ty.clone());
            return ty;
        }
        None
    }

    /// A flow function names what its object's type has: `time_in` a
    /// state, `entered_at` a state or a tracked member, wherever an
    /// expression is typed, as the model's check 58 has it for a
    /// transition's body (the audit's values G2, where `time_in` of a member
    /// answered 0 s).
    fn flow_function_names(
        &mut self,
        f: &Expr,
        args: &[Expr],
        tn: &str,
        x: Option<&Value>,
        env: &TyEnv,
        seen: &BTreeSet<(String, String)>,
    ) {
        if !self.flow_names {
            return;
        }
        let (on, fname) = match f {
            Expr::Name(n) => (Some(tn.to_string()), n.as_str()),
            Expr::Path(b, n) => match self.of(b, tn, x, env, seen) {
                Some(Ty::Reference(t)) => (Some(t), n.as_str()),
                _ => (None, n.as_str()),
            },
            _ => return,
        };
        let ([Expr::Name(arg)], Some(on)) = (args, on) else {
            return;
        };
        if !matches!(fname, "time_in" | "entered_at") {
            return;
        }
        let Some(ot) = self.kinds.get(&on) else {
            return;
        };
        let state = obj(ot, "states").contains_key(arg);
        if fname == "time_in" && !state {
            self.fail(format!("time_in({arg}) names no state of {on}"));
        } else if fname == "entered_at" && !state && !tracked_attribute(ot, arg, self.kinds) {
            self.fail(format!(
                "entered_at({arg}) names neither a state of {on} nor a member it tracks"
            ));
        }
    }

    fn fail(&mut self, detail: String) {
        self.broken.push(detail);
    }

    /// The type of an element of a set of values the type declares, `tags`
    /// or `this.tags`, or none (`check-flows.py`'s `value_set_element`).
    fn value_set_element(
        &self,
        tn: &str,
        coll: &Expr,
        env: &BTreeMap<String, Option<Ty>>,
    ) -> Option<Ty> {
        let name = match coll {
            Expr::Name(n) if !env.contains_key(n) => n.as_str(),
            Expr::Path(b, m) if matches!(&**b, Expr::Name(t) if t == "this") => m.as_str(),
            _ => return None,
        };
        let spec = self
            .kinds
            .get(tn)
            .and_then(|t| obj(t, "attributes").get(name))?;
        let typ = spec.get("type").map(py_str).unwrap_or_default();
        let base = typ.strip_suffix("[]")?;
        attribute_type(Some(&serde_json::json!({ "type": base })), self.enums)
    }

    fn of(
        &mut self,
        e: &Expr,
        tn: &str,
        x: Option<&Value>,
        env: &TyEnv,
        seen: &BTreeSet<(String, String)>,
    ) -> Option<Ty> {
        match e {
            // an int literal past 64 bits, which has no value (§8.1, the
            // values audit's L3)
            Expr::Num(Number::Decimal(t)) if !t.contains('.') => {
                self.fail(format!("the literal {t} is past an int's 64 bits"));
                Some(Ty::Int)
            }
            Expr::Num(n) => Some(Ty::Number(match n {
                Number::Int(_) => 0,
                Number::Decimal(t) => t.split_once('.').map_or(0, |(_, f)| f.len() as u32),
            })),
            Expr::Str(_) => Some(Ty::Str),
            Expr::Bool(_) => Some(Ty::Bool),
            // a read's filter's, never a flow's
            Expr::Param(_) | Expr::Lit(_) => None,
            // an amount in a currency the engine has no minor unit for, or
            // with more places than its minor unit, is one it cannot hold,
            // where it was rounded (the audit's values L1 and L6)
            Expr::Money(c, amount) => {
                match crate::value::minor_unit(c) {
                    None => self.fail(format!(
                        "{c} is a currency whose minor unit the engine does not know"
                    )),
                    Some(m) => {
                        let exact = crate::value::Decimal::parse(amount)
                            .is_some_and(|d| d.rescale(m).is_some_and(|r| r == d));
                        if !exact {
                            self.fail(format!(
                                "{c} {amount} has more places than {c}'s minor unit"
                            ));
                        }
                    }
                }
                Some(Ty::Money(c.clone()))
            }
            // a duration is whole seconds, where `1.5 s` was rounded to 2 s
            // (the audit's values L2)
            // and holds 64 bits of them, where one past was not run (ADR-0202
            // decision 8)
            Expr::Dur(n, unit) => {
                if let Some(size) = crate::value::unit_seconds(unit) {
                    let t = match n {
                        Number::Int(i) => i.to_string(),
                        Number::Decimal(t) => t.clone(),
                    };
                    let seconds = crate::value::Decimal::parse(&t)
                        .and_then(|d| d.checked_mul(&crate::value::Decimal::from_int(size)));
                    match seconds {
                        Some(d) if d.rescale(0).is_some_and(|w| w == d) => {
                            if d.to_int().is_none() {
                                self.fail(format!(
                                    "{t} {unit} is past a duration's 64 bits of seconds"
                                ));
                            }
                        }
                        _ => self.fail(format!("{t} {unit} is no whole number of seconds")),
                    }
                }
                Some(Ty::Duration)
            }
            Expr::Name(n) => {
                if let Some(t) = env.get(n) {
                    return t.clone();
                }
                if n == "now" {
                    return Some(Ty::Timestamp);
                }
                if n == "this" {
                    return Some(Ty::Reference(tn.to_string()));
                }
                if self.categories.contains(n) {
                    return Some(Ty::Category);
                }
                let states = self
                    .kinds
                    .get(tn)
                    .map(|t| obj(t, "states").contains_key(n))
                    .unwrap_or(false);
                if states {
                    return Some(Ty::State(tn.to_string()));
                }
                // an enumeration's name is no value, and a name naming
                // nothing is none, where each typed as nothing and was not
                // run (the values audit's K-9); an all-capital name is a
                // state's, another rule's
                if self.enums.contains(n) {
                    self.fail(format!("{n} is an enumeration, which is no value: name one of its members, as {n}.<MEMBER>"));
                    return None;
                }
                let got = self.member(tn, n, seen);
                if got.is_none() && ACTOR_KINDS.contains(&n.as_str()) {
                    return Some(Ty::ActorKind);
                }
                let capital = n.chars().next().is_some_and(char::is_uppercase);
                if got.is_none()
                    && capital
                    && n.chars().any(char::is_lowercase)
                    && !self.kinds.contains_key(n)
                {
                    self.fail(format!(
                        "{n} names nothing: no type, enumeration or state of {tn} is called {n}"
                    ));
                    return None;
                }
                got
            }
            Expr::Path(base, m) => {
                if matches!(&**base, Expr::Name(b) if b == "inputs") {
                    let x = x.cloned().unwrap_or(Value::Null);
                    let mut spec = obj(&x, "inputs").get(m).cloned();
                    let attribute_input = strings(&x, "required_inputs")
                        .iter()
                        .chain(&strings(&x, "optional_inputs"))
                        .any(|n| n == m);
                    if spec.is_none() && attribute_input {
                        spec = self
                            .kinds
                            .get(tn)
                            .and_then(|t| obj(t, "attributes").get(m))
                            .cloned();
                    }
                    return attribute_type(spec.as_ref(), self.enums);
                }
                // who acted, as an outcome value reads it (§8.1), where its
                // members typed as nothing, so `"by " + actor.id` passed and
                // was not run (D687)
                if matches!(&**base, Expr::Name(b) if b == "actor") && !env.contains_key("actor") {
                    return match m.as_str() {
                        "id" => Some(Ty::Identity),
                        "kind" => Some(Ty::ActorKind),
                        "principal" => Some(Ty::Str),
                        _ => None,
                    };
                }
                if let Expr::Name(b) = &**base
                    && !env.contains_key(b)
                {
                    if self.enums.contains(b) {
                        return Some(Ty::Enumeration(b.clone()));
                    }
                    let upper =
                        m.chars().any(char::is_uppercase) && !m.chars().any(char::is_lowercase);
                    if self.kinds.contains_key(b) && upper {
                        return Some(Ty::State(b.clone()));
                    }
                    // an owner nothing declares, the qualified names' rule's
                    if b.chars().next().is_some_and(char::is_uppercase)
                        && !self.kinds.contains_key(b)
                    {
                        return None;
                    }
                    // a type's member is one of its states, where it typed as
                    // nothing and was not run (the values audit's K-9)
                    if self.kinds.contains_key(b) {
                        self.fail(format!("{b}.{m} names no state of {b}"));
                        return None;
                    }
                }
                // a value with no members has none read (K-9)
                match self.of(base, tn, x, env, seen) {
                    Some(Ty::Reference(t)) => match self.undeclared(&t, m) {
                        Some(why) => {
                            self.fail(why);
                            None
                        }
                        None => self.member(&t, m, seen),
                    },
                    Some(Ty::State(_)) if m == "category" => Some(Ty::Category),
                    Some(Ty::State(_)) => {
                        self.fail(format!("it reads {m} of a state, which has only category"));
                        None
                    }
                    Some(t)
                        if matches!(
                            t,
                            Ty::Number(_)
                                | Ty::Int
                                | Ty::Decimal(_)
                                | Ty::Money(_)
                                | Ty::Duration
                                | Ty::Timestamp
                                | Ty::Date
                                | Ty::Bool
                                | Ty::Str
                                | Ty::Identity
                                | Ty::ActorKind
                                | Ty::Category
                                | Ty::Enumeration(_)
                        ) =>
                    {
                        let t = t.text();
                        self.fail(format!("it reads {m} of {t}, which has no members"));
                        None
                    }
                    _ => None,
                }
            }
            Expr::Call(f, args) => {
                for a in args {
                    if !matches!(a, Expr::Name(_)) {
                        self.of(a, tn, x, env, seen);
                    }
                }
                if let Expr::Path(b, _) = &**f
                    && matches!(&**b, Expr::Name(ev) if self.evaluators.contains(ev))
                {
                    return Some(Ty::Verdict);
                }
                // a function the language has, given one argument, where any
                // other call typed as nothing and the core did not run it
                // (the values audit's K-7, K-8)
                match &**f {
                    Expr::Name(name)
                        if !FUNCTIONS.contains(&name.as_str())
                            && !METRIC_ONLY_FUNCTIONS.contains(&name.as_str()) =>
                    {
                        self.fail(format!("{name} is no function the language has: it has length, date, entered_at and time_in, and in a metric avg, median, percentile, day, week, month, quarter and year"));
                        return None;
                    }
                    Expr::Name(name) if FUNCTIONS.contains(&name.as_str()) && args.len() != 1 => {
                        let n = args.len();
                        self.fail(format!("{name} takes one argument, and is given {n}"));
                        return None;
                    }
                    Expr::Path(_, name) if !MEMBER_FUNCTIONS.contains(&name.as_str()) => {
                        self.fail(format!("{name} is no member function the language has: it has intervals, entered_at, time_in and a row's held"));
                        return None;
                    }
                    _ => {}
                }
                // a member function on what has it, `held` on a row of
                // intervals or transitions, the others on an object,
                // `intervals(m)` of a member its type tracks; and a flow
                // function names a state or a member, never a literal; where
                // each published and was not run (the values audit's K-8)
                if let Expr::Path(base, name) = &**f
                    && let Some(bt) = self.of(base, tn, x, env, seen)
                {
                    if name == "held" {
                        self.fail(format!(
                            "held is a row's, of intervals or transitions, and it is called on {}",
                            bt.text()
                        ));
                        return None;
                    }
                    let Ty::Reference(on) = &bt else {
                        self.fail(format!(
                            "{name} is an object's, and it is called on {}",
                            bt.text()
                        ));
                        return None;
                    };
                    if name == "intervals"
                        && let [Expr::Name(m)] = args.as_slice()
                        && m != "state"
                        && let Some(ot) = self.kinds.get(on)
                        && !tracked_attribute(ot, m, self.kinds)
                    {
                        self.fail(format!(
                            "intervals({m}) names neither the state nor a member {on} tracks"
                        ));
                        return None;
                    }
                }
                let called = match &**f {
                    Expr::Name(n) | Expr::Path(_, n) => Some(n.as_str()),
                    _ => None,
                };
                if matches!(called, Some("time_in" | "entered_at"))
                    && let [
                        arg @ (Expr::Str(_)
                        | Expr::Num(_)
                        | Expr::Bool(_)
                        | Expr::Money(..)
                        | Expr::Dur(..)),
                    ] = args.as_slice()
                    && let Some(at) = self.of(arg, tn, x, env, seen)
                {
                    self.fail(format!(
                        "{} names a state or a tracked member, and is given {}",
                        called.unwrap_or_default(),
                        at.text()
                    ));
                    return None;
                }
                self.flow_function_names(f, args, tn, x, env, seen);
                let Expr::Name(name) = &**f else {
                    return None;
                };
                match name.as_str() {
                    "length" => {
                        let arg = if args.len() == 1 {
                            self.of(&args[0], tn, x, env, seen)
                        } else {
                            None
                        };
                        if let Some(arg) = arg
                            && !matches!(arg, Ty::Str | Ty::Values(_))
                        {
                            self.fail(format!(
                                "length takes a string or a set of values, and is given {}",
                                arg.text()
                            ));
                        }
                        Some(Ty::Int)
                    }
                    "entered_at" => Some(Ty::Timestamp),
                    "time_in" => Some(Ty::Duration),
                    "date" => Some(Ty::Date),
                    _ => None,
                }
            }
            // the event it reads is one no type here names, so a value of a
            // type it can tell is not one, where an int was read as a log
            // position (the values audit's G7)
            Expr::Changed(_, ev) => {
                if let Some(given) = self.of(ev, tn, x, env, seen) {
                    self.fail(format!(
                        "changed_since reads an event, and is given {}",
                        given.text()
                    ));
                }
                Some(Ty::Bool)
            }
            Expr::IsNull(inner, _) => {
                self.of(inner, tn, x, env, seen);
                Some(Ty::Bool)
            }
            Expr::Not(inner) => {
                let v = self.of(inner, tn, x, env, seen);
                if let Some(v) = v
                    && !matches!(v, Ty::Bool | Ty::Verdict)
                {
                    self.fail(format!("not takes a bool, and is given {}", v.text()));
                }
                Some(Ty::Bool)
            }
            // a negative of a number, an amount or a duration, where any other
            // typed as its operand and was not run (the values audit's K-5)
            Expr::Neg(inner) => {
                let v = self.of(inner, tn, x, env, seen);
                if let Some(t) = &v
                    && !(t.scalar() || matches!(t, Ty::Money(_) | Ty::Duration))
                {
                    self.fail(format!("it negates {}, which has no negative", t.text()));
                    return None;
                }
                v
            }
            Expr::Agg {
                kind,
                distinct,
                var,
                collection,
                filter,
                body,
                ..
            } => {
                // a metric's own count, over its rows, where it typed as an
                // int and was not run (the values audit's K-9)
                if var.is_none() {
                    self.fail("count(where …) and count(distinct …) count a metric's rows, which no expression outside a metric has".to_string());
                    return None;
                }
                let mut inner = env.clone();
                if let (Some(var), Some(coll)) = (var, collection)
                    && let Expr::Set(items) = &**coll
                {
                    // a literal's elements share one type, its item's, where a
                    // mixed one typed as nothing and was not run (the values
                    // audit's K-10)
                    let known: Vec<Ty> = items
                        .iter()
                        .filter_map(|item| self.of(item, tn, x, env, seen))
                        .collect();
                    let clash = known.iter().find_map(|a| {
                        known
                            .iter()
                            .find(|b| !self.alike(a, b))
                            .map(|b| (a.clone(), b.clone()))
                    });
                    if let Some((a, b)) = clash {
                        let (a, b) = (a.text(), b.text());
                        self.fail(format!("{fn} ranges over a set of {a} and {b}, which share no type", fn = kind));
                        return None;
                    }
                    let element = known
                        .iter()
                        .find(|t| !matches!(t, Ty::Number(_)))
                        .or(known.first())
                        .cloned();
                    inner.insert(var.clone(), element);
                } else if let (Some(var), Some(coll)) = (var, collection) {
                    let scan = matches!(&**coll, Expr::Name(n)
                        if !env.contains_key(n) && self.kinds.contains_key(n));
                    let ct = if scan {
                        let Expr::Name(n) = &**coll else {
                            unreachable!()
                        };
                        Some(Ty::Objects(n.clone()))
                    } else {
                        self.of(coll, tn, x, env, seen)
                    };
                    let element = match ct {
                        Some(Ty::Objects(t)) => Some(Ty::Reference(t)),
                        // a set of values a path or an input names
                        Some(Ty::Values(e)) => {
                            Some(self.value_set_element(tn, coll, env).unwrap_or(*e))
                        }
                        Some(t) => match self.value_set_element(tn, coll, env) {
                            // a set of values, its item of their type (K-10)
                            Some(e) => Some(e),
                            None => {
                                // an aggregate ranges over a collection, where
                                // one over a single value was not run (K-10)
                                let t = t.text();
                                self.fail(format!("{fn} ranges over {t}, which is no collection", fn = kind));
                                return None;
                            }
                        },
                        None => self.value_set_element(tn, coll, env),
                    };
                    inner.insert(var.clone(), element);
                }
                if let Some(w) = filter {
                    let w = self.of(w, tn, x, &inner, seen);
                    if let Some(w) = w
                        && w != Ty::Bool
                    {
                        self.fail(format!(
                            "the condition of a {kind} is {}, not a bool",
                            w.text()
                        ));
                    }
                }
                let b = match body {
                    Some(b) => self.of(b, tn, x, &inner, seen),
                    None => None,
                };
                match kind.as_str() {
                    // a count of elements counts those its body holds for,
                    // a condition, as `any`'s is (§8.3, the audit's values
                    // G1); a distinct count counts its body's values
                    "count" => {
                        if !*distinct
                            && let Some(b) = b
                            && b != Ty::Bool
                        {
                            self.fail(format!("the body of count is {}, not a bool", b.text()));
                        }
                        Some(Ty::Int)
                    }
                    "all" | "any" | "none" => {
                        if let Some(b) = b
                            && b != Ty::Bool
                        {
                            self.fail(format!("the body of {kind} is {}, not a bool", b.text()));
                        }
                        Some(Ty::Bool)
                    }
                    "sum" | "min" | "max" if b.is_some() => {
                        let b = b.unwrap_or(Ty::Number(0));
                        if kind == "sum"
                            && !(b.scalar() || matches!(b, Ty::Money(_) | Ty::Duration))
                        {
                            self.fail(format!("sum adds {}, which does not add", b.text()));
                        } else if matches!(b, Ty::Enumeration(_)) {
                            // the least or the greatest member, of what has no
                            // order, which the core did not run and the
                            // reference runner answered by name (the values
                            // audit's R4, as K-4)
                            self.fail(format!(
                                "{kind} orders {}, and an enumeration has no order",
                                b.text()
                            ));
                        }
                        Some(b)
                    }
                    _ => None,
                }
            }
            Expr::Metric { binds, .. } => {
                for (_, v) in binds {
                    self.of(v, tn, x, env, seen);
                }
                None
            }
            Expr::In(l, r) => {
                let left = self.of(l, tn, x, env, seen);
                match &**r {
                    Expr::Name(n) if self.enums.contains(n) && !env.contains_key(n) => {
                        if let Some(left) = left
                            && left != Ty::Enumeration(n.clone())
                        {
                            self.fail(format!("it asks whether {} is in {n}", left.text()));
                        }
                    }
                    Expr::Set(items) => {
                        for item in items {
                            let it = self.of(item, tn, x, env, seen);
                            if let (Some(left), Some(it)) = (&left, &it)
                                && !self.alike(left, it)
                            {
                                self.fail(format!(
                                    "it asks whether {} is among {}",
                                    left.text(),
                                    it.text()
                                ));
                                break;
                            }
                        }
                    }
                    // membership in a collection, where one in a single value
                    // was not run (the values audit's K-6)
                    _ => {
                        if let Some(rt) = self.of(r, tn, x, env, seen)
                            && !matches!(rt, Ty::Objects(_) | Ty::Values(_))
                        {
                            self.fail(format!(
                                "it asks for membership in {}, which is no collection",
                                rt.text()
                            ));
                        }
                    }
                }
                Some(Ty::Bool)
            }
            Expr::If(c, a, b) => {
                let c = self.of(c, tn, x, env, seen);
                if let Some(c) = c
                    && c != Ty::Bool
                {
                    self.fail(format!("if takes a bool, and is given {}", c.text()));
                }
                let a = self.of(a, tn, x, env, seen);
                let b = self.of(b, tn, x, env, seen);
                if let (Some(av), Some(bv)) = (&a, &b)
                    && !self.alike(av, bv)
                {
                    self.fail(format!(
                        "if yields {} or {}, which share no type",
                        av.text(),
                        bv.text()
                    ));
                    return None;
                }
                match a {
                    Some(a) if !matches!(a, Ty::Number(_)) => Some(a),
                    _ => b,
                }
            }
            Expr::Bin(op, le, re) => {
                let l = self.of(le, tn, x, env, seen);
                let r = self.of(re, tn, x, env, seen);
                match op {
                    BinOp::And | BinOp::Or | BinOp::Implies => {
                        // a verdict inside a larger expression is check 24's
                        // evaluator rule's, refused there
                        for v in [&l, &r].into_iter().flatten() {
                            if !matches!(v, Ty::Bool | Ty::Verdict) {
                                self.fail(format!(
                                    "{} takes bools, and is given {}",
                                    op.text(),
                                    v.text()
                                ));
                                break;
                            }
                        }
                        Some(Ty::Bool)
                    }
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        if let (Some(lv), Some(rv)) = (&l, &r)
                            && !self.alike(lv, rv)
                        {
                            self.fail(format!("it compares {} with {}", lv.text(), rv.text()));
                        } else if !matches!(op, BinOp::Eq | BinOp::Ne)
                            && let Some(e) = [&l, &r]
                                .into_iter()
                                .flatten()
                                .find(|v| matches!(v, Ty::Enumeration(_)))
                        {
                            // an enumeration has no order, which was not run
                            // (the values audit's K-4)
                            self.fail(format!(
                                "it orders {}, and an enumeration has no order",
                                e.text()
                            ));
                        } else if !matches!(op, BinOp::Eq | BinOp::Ne)
                            && let Some(v) = [&l, &r]
                                .into_iter()
                                .flatten()
                                .find(|v| matches!(v, Ty::State(_) | Ty::Values(_)))
                        {
                            // nor has a state or a set, which was not run and
                            // the runner ordered by name or crashed on (the
                            // differential battery, 2026-10-15)
                            let what = if matches!(v, Ty::State(_)) {
                                "a state"
                            } else {
                                "a set"
                            };
                            self.fail(format!("it orders {}, and {what} has no order", v.text()));
                        }
                        Some(Ty::Bool)
                    }
                    _ => {
                        // a literal zero divisor, which no value has, and a
                        // date moved by a literal part of a day, which no
                        // date is, each refused at run time on every object
                        // (the differential battery)
                        if *op == BinOp::Div && literal_zero(re) {
                            self.fail("it divides by zero, which has no value".to_string());
                            return None;
                        }
                        if matches!(op, BinOp::Add | BinOp::Sub)
                            && matches!(l, Some(Ty::Date))
                            && let Expr::Dur(n, unit) = &**re
                            && let Some(t) = part_of_a_day(n, unit)
                        {
                            self.fail(format!(
                                "it moves a date by {t} {unit}, which is no whole number of days"
                            ));
                            return None;
                        }
                        match (l, r) {
                            (Some(l), Some(r)) => self.arithmetic(*op, l, r),
                            _ => None,
                        }
                    }
                }
            }
            Expr::Set(items) => {
                for item in items {
                    self.of(item, tn, x, env, seen);
                }
                None
            }
        }
    }

    /// Whether two types are one for a comparison or a conditional's
    /// branches (`check-flows.py`'s `alike`).
    fn alike(&self, a: &Ty, b: &Ty) -> bool {
        let numeric = |t: &Ty| matches!(t, Ty::Number(_) | Ty::Int | Ty::Decimal(_));
        let literal = |t: &Ty| matches!(t, Ty::Number(_));
        if a == b || (numeric(a) && numeric(b) && (literal(a) || literal(b))) {
            return true;
        }
        match (a, b) {
            (Ty::Decimal(_), Ty::Decimal(_)) | (Ty::State(_), Ty::State(_)) => true,
            (Ty::Identity, Ty::Str) | (Ty::Str, Ty::Identity) => true,
            (Ty::Reference(x), Ty::Reference(y)) => {
                let rx = ["reference".to_string(), x.clone()];
                let ry = ["reference".to_string(), y.clone()];
                fits(&rx, &ry, self.kinds) || fits(&ry, &rx, self.kinds)
            }
            _ => false,
        }
    }

    /// The type of `l op r` by §8.3, or none, noting a rule it breaks
    /// (`check-flows.py`'s `arithmetic`).
    fn arithmetic(&mut self, op: BinOp, l: Ty, r: Ty) -> Option<Ty> {
        let verb = if op == BinOp::Add {
            "adds"
        } else {
            "subtracts"
        };
        if matches!(op, BinOp::Add | BinOp::Sub) {
            if l.scalar() && r.scalar() {
                // a literal takes its partner's scale (§8.1), and one of more
                // places than a decimal holds would lose them, where `rate +
                // 0.005` was rounded on its write (the audit's values W5)
                for (a, b) in [(&l, &r), (&r, &l)] {
                    if let (Ty::Number(p), Ty::Decimal(Some(s))) = (a, b)
                        && p > s
                    {
                        self.fail(format!(
                            "it {verb} a number of {p} places and a decimal of scale {s}, which holds fewer"
                        ));
                        return None;
                    }
                }
                // two literals keep the more places, and a literal beside an
                // int its own, as the core aligns them, where `i + 1.5` was
                // an int and written to one (the differential battery)
                if let (Ty::Number(a), Ty::Number(b)) = (&l, &r) {
                    return Some(Ty::Number(*a.max(b)));
                }
                for (a, b) in [(&l, &r), (&r, &l)] {
                    if let (Ty::Number(p), Ty::Int) = (a, b) {
                        return Some(if *p == 0 {
                            Ty::Int
                        } else {
                            Ty::Decimal(Some(*p))
                        });
                    }
                }
                if matches!(l, Ty::Number(_)) {
                    return Some(r);
                }
                if matches!(r, Ty::Number(_)) {
                    return Some(l);
                }
                if l.tag() != r.tag() {
                    self.fail(format!(
                        "it {verb} {} and {}, two types",
                        l.text(),
                        r.text()
                    ));
                    return None;
                }
                if let (Ty::Decimal(Some(a)), Ty::Decimal(Some(b))) = (&l, &r)
                    && a != b
                {
                    self.fail(format!("it {verb} decimals of two scales, {a} and {b}"));
                    return None;
                }
                return Some(l);
            }
            if let (Ty::Money(a), Ty::Money(b)) = (&l, &r) {
                if a != b {
                    self.fail(format!("it {verb} money of two currencies, {a} and {b}"));
                    return None;
                }
                return Some(l);
            }
            if l == Ty::Duration && r == Ty::Duration {
                return Some(l);
            }
            if (l == Ty::Timestamp && r == Ty::Duration)
                || (op == BinOp::Add && l == Ty::Duration && r == Ty::Timestamp)
            {
                return Some(Ty::Timestamp);
            }
            if op == BinOp::Sub && l == Ty::Timestamp && r == Ty::Timestamp {
                return Some(Ty::Duration);
            }
            // a date less a date is a duration, and a date plus or minus a
            // duration a date, as §8.3's Dates says and the core runs; every
            // other with a date is an error, where it typed as nothing (the
            // values audit's K-1)
            if op == BinOp::Sub && l == Ty::Date && r == Ty::Date {
                return Some(Ty::Duration);
            }
            if l == Ty::Date && r == Ty::Duration {
                return Some(Ty::Date);
            }
            self.fail(format!("it {verb} {} and {}", l.text(), r.text()));
            return None;
        }
        // * and /
        if l.scalar() && r.scalar() {
            if op == BinOp::Div {
                // an int divided by a decimal is not run, its rounding to an
                // int being unsaid (ADR-0202 decision 8)
                let whole = |t: &Ty| matches!(t, Ty::Int | Ty::Number(0));
                let fraction =
                    |t: &Ty| matches!(t, Ty::Decimal(_)) || matches!(t, Ty::Number(s) if *s > 0);
                if whole(&l) && fraction(&r) {
                    self.fail(format!(
                        "it divides {} by {}, which the engine does not run: divide a decimal, such as n * 1.0, for the fraction",
                        l.text(),
                        r.text()
                    ));
                    return None;
                }
                // / yields its left operand's type
                return Some(l);
            }
            if let (Ty::Number(a), Ty::Number(b)) = (&l, &r) {
                return Some(Ty::Number(a + b));
            }
            let whole = |t: &Ty| matches!(t, Ty::Int | Ty::Number(0));
            if whole(&l) && whole(&r) {
                return Some(Ty::Int);
            }
            // decimal(p1,s1) * decimal(p2,s2) is of scale s1+s2, and an
            // int's scale is 0
            let scale = |t: &Ty| match t {
                Ty::Int => Some(0),
                Ty::Number(s) => Some(*s),
                Ty::Decimal(s) => *s,
                _ => None,
            };
            return Some(Ty::Decimal(scale(&l).zip(scale(&r)).map(|(a, b)| a + b)));
        }
        let quantity = |t: &Ty| matches!(t, Ty::Money(_) | Ty::Duration);
        if op == BinOp::Div && l.tag() == r.tag() && quantity(&l) {
            if let (Ty::Money(a), Ty::Money(b)) = (&l, &r)
                && a != b
            {
                self.fail(format!("it divides money of two currencies, {a} and {b}"));
                return None;
            }
            // a quotient of two like quantities
            return Some(Ty::Decimal(Some(6)));
        }
        if quantity(&l) && r.scalar() {
            return Some(l);
        }
        if op == BinOp::Mul && quantity(&r) && l.scalar() {
            return Some(r);
        }
        if op == BinOp::Div && l.scalar() && quantity(&r) {
            self.fail(format!(
                "it divides {} by {}, and / takes its scalar on the right",
                l.text(),
                r.text()
            ));
            return None;
        }
        if matches!(l, Ty::Timestamp | Ty::Date) || matches!(r, Ty::Timestamp | Ty::Date) {
            // neither is multiplied nor divided, where it typed as nothing
            // and was not run (the values audit's K-1, K-2)
            let verb = if op == BinOp::Mul {
                "multiplies"
            } else {
                "divides"
            };
            self.fail(format!(
                "it {verb} {} by {}, and a time or a date is neither multiplied nor divided",
                l.text(),
                r.text()
            ));
            return None;
        }
        let verb = if op == BinOp::Mul {
            "multiplies"
        } else {
            "divides"
        };
        self.fail(format!(
            "it {verb} {} by {}, and takes a scalar on at least one side",
            l.text(),
            r.text()
        ));
        None
    }
}

/// Each expression of a module that does not type by
/// `declaration-syntax.md` §8.3, where the types it combines can be told,
/// and a condition that is no bool or verdict and an invariant or a
/// uniqueness condition that is no bool (the model's check 24;
/// `check-flows.py`'s `expression_type_errors`).
/// A type the typing infers as check 17's kinds spell it, none where they
/// spell none: a number literal is a number where it is whole and a decimal
/// where it has places.
fn typed_kind(ty: &Ty) -> Option<Vec<String>> {
    let one = |k: &str| Some(vec![k.to_string()]);
    match ty {
        Ty::Number(0) => one("number"),
        Ty::Number(_) | Ty::Decimal(_) => one("decimal"),
        Ty::Money(c) => Some(vec!["money".into(), c.clone()]),
        Ty::Reference(t) => Some(vec!["reference".into(), t.clone()]),
        Ty::Enumeration(e) => Some(vec!["enumeration".into(), e.clone()]),
        // held by no attribute, so written to none (the differential battery)
        Ty::State(_) | Ty::Category => one(ty.tag()),
        Ty::Int
        | Ty::Str
        | Ty::Bool
        | Ty::Timestamp
        | Ty::Duration
        | Ty::Identity
        | Ty::File
        | Ty::Date
        | Ty::ActorKind => one(ty.tag()),
        _ => None,
    }
}

/// Whether an expression is a literal zero, `0` or `0.00`.
fn literal_zero(e: &Expr) -> bool {
    match e {
        Expr::Num(Number::Int(n)) => *n == 0,
        Expr::Num(Number::Decimal(d)) => d.contains('.') && d.chars().all(|c| c == '0' || c == '.'),
        _ => false,
    }
}

/// A duration literal's text where it is a whole number of seconds and no
/// whole number of days.
fn part_of_a_day(n: &Number, unit: &str) -> Option<String> {
    let size = crate::value::unit_seconds(unit)?;
    let t = match n {
        Number::Int(i) => i.to_string(),
        Number::Decimal(t) => t.clone(),
    };
    let seconds =
        crate::value::Decimal::parse(&t)?.checked_mul(&crate::value::Decimal::from_int(size))?;
    let whole = seconds.rescale(0).filter(|w| *w == seconds)?.to_int()?;
    (whole % 86_400 != 0).then_some(t)
}

/// The type a member of a row of a source other than objects holds, for a
/// metric's aggregate over `item.member`: its times, its duration, its
/// counts, its texts and its flags (`check-flows.py`'s `ROW_MEMBER_TYPES`).
fn row_member_ty(member: &str) -> Option<Ty> {
    Some(match member {
        "entered_at" | "left_at" | "occurred_at" | "recorded_at" | "at" | "piece_start"
        | "piece_end" => Ty::Timestamp,
        "duration" => Ty::Duration,
        "day" => Ty::Date,
        "count" | "declaration_version" | "corrected_by" => Ty::Int,
        "actor_id" => Ty::Identity,
        "actor_kind" | "entered_by_kind" | "recorded_by_kind" => Ty::ActorKind,
        "transition" | "verdict" | "clause" | "remedy" | "name" => Ty::Str,
        "legacy" | "migrated" | "completes" | "returns" | "asserted" | "overrides" | "imported"
        | "redacted" | "unknown" | "enforced" | "flagged" => Ty::Bool,
        _ => return None,
    })
}

/// The type `item.member` holds on a row of a source other than objects:
/// a row's state and object its type's own, a span's value its member's
/// (`check-flows.py`'s `row_member_type`).
fn row_member_type(
    tn: &str,
    t: &Value,
    source: &str,
    member: &str,
    enums: &BTreeSet<String>,
) -> Option<Ty> {
    if matches!(
        member,
        "state" | "from_state" | "to_state" | "subject_state"
    ) {
        return Some(Ty::State(tn.to_string()));
    }
    if matches!(member, "object" | "subject") {
        return Some(Ty::Reference(tn.to_string()));
    }
    let tracked = source
        .strip_prefix("intervals(")
        .and_then(|r| r.strip_suffix(')'))
        .filter(|m| {
            m.starts_with(|c: char| c.is_ascii_lowercase())
                && m.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        });
    if member == "value"
        && let Some(m) = tracked
    {
        return if m == "state" {
            Some(Ty::State(tn.to_string()))
        } else {
            attribute_type(obj(t, "attributes").get(m), enums)
        };
    }
    row_member_ty(member)
}

/// The type of `a op b` in a metric's value, as `metric.rs` `arith_kind`
/// gives it, which places its rounding (ADR-0128); a sum or a difference of
/// two types is refused, saying so, which the core does not compute where
/// its value is fractional (`check-flows.py`'s `metric_arith`, values M2).
fn metric_arith(op: BinOp, a: Ty, b: Ty) -> Result<Option<Ty>, String> {
    let scalar = |k: &Ty| matches!(k, Ty::Int | Ty::Decimal(_) | Ty::Number(_));
    // a decimal literal takes the scale of what it is combined with (§8.1)
    let (a, b) = match (&a, &b) {
        (Ty::Number(_), Ty::Decimal(_)) => (b.clone(), b),
        (Ty::Decimal(_), Ty::Number(_)) => (a.clone(), a),
        _ => (a, b),
    };
    if matches!(op, BinOp::Mul | BinOp::Div)
        && let Some(t) = [&a, &b]
            .into_iter()
            .find(|k| matches!(k, Ty::Timestamp | Ty::Date))
    {
        // neither is multiplied nor divided, as K-1 refuses elsewhere, where
        // a metric's was not run or answered a date's seconds (the
        // differential battery)
        let verb = if op == BinOp::Mul {
            "multiplies"
        } else {
            "divides"
        };
        return Err(format!(
            "it {verb} {}, and a time or a date is neither multiplied nor divided",
            t.text()
        ));
    }
    match op {
        BinOp::Add | BinOp::Sub => {
            if op == BinOp::Sub
                && matches!(
                    (&a, &b),
                    (Ty::Timestamp, Ty::Timestamp) | (Ty::Date, Ty::Date)
                )
            {
                return Ok(Some(Ty::Duration));
            }
            if a == Ty::Date && b == Ty::Duration {
                return Ok(Some(Ty::Date));
            }
            if (a == Ty::Timestamp && b == Ty::Duration)
                || (op == BinOp::Add && a == Ty::Duration && b == Ty::Timestamp)
            {
                return Ok(Some(Ty::Timestamp));
            }
            if let (Ty::Number(x), Ty::Number(y)) = (&a, &b) {
                return Ok(Some(Ty::Number(*x.max(y))));
            }
            if a == b {
                return Ok(Some(a));
            }
            let verb = if op == BinOp::Add {
                format!("adds {} to", b.text())
            } else {
                format!("subtracts {} from", b.text())
            };
            Err(format!(
                "it {verb} {}, and a metric's value adds values of one type, a decimal of one scale (§8.3)",
                a.text()
            ))
        }
        BinOp::Mul => {
            if scalar(&a) && scalar(&b) {
                if a == Ty::Int && b == Ty::Int {
                    return Ok(Some(Ty::Int));
                }
                let places = |k: &Ty| match k {
                    Ty::Decimal(s) => *s,
                    Ty::Number(s) => Some(*s),
                    _ => Some(0),
                };
                return Ok(Some(Ty::Decimal(
                    places(&a).zip(places(&b)).map(|(x, y)| x + y),
                )));
            }
            Ok(if scalar(&a) {
                Some(b)
            } else if scalar(&b) {
                Some(a)
            } else {
                None
            })
        }
        _ => {
            // a quotient of two like quantities
            if a == b && matches!(a, Ty::Duration | Ty::Money(_)) {
                return Ok(Some(Ty::Decimal(Some(6))));
            }
            if scalar(&b) {
                return Ok(Some(match a {
                    Ty::Number(s) => Ty::Decimal(Some(s)),
                    other => other,
                }));
            }
            Ok(None)
        }
    }
}

/// A metric's rows and where it reads them, for typing its value.
struct MetricRows<'a> {
    tn: &'a str,
    t: &'a Value,
    enums: &'a BTreeSet<String>,
    source: String,
    item: String,
}

impl MetricRows<'_> {
    /// The type of an aggregate's body over a row, None where it cannot be
    /// told: an object's by the typing, another row's `item.member` and
    /// `item.held(m)` by their members.
    fn body(&self, typing: &mut Typing<'_>, b: &Expr) -> Option<Ty> {
        if self.source == "objects" {
            typing.broken.clear();
            typing.flow_names = true;
            let mut env = TyEnv::new();
            env.insert(self.item.clone(), Some(Ty::Reference(self.tn.to_string())));
            let got = typing.of(b, self.tn, None, &env, &BTreeSet::new());
            return if typing.broken.is_empty() { got } else { None };
        }
        let is_item = |e: &Expr| matches!(e, Expr::Name(n) if *n == self.item);
        match b {
            Expr::Path(base, m) if is_item(base) => {
                row_member_type(self.tn, self.t, &self.source, m, self.enums)
            }
            Expr::Call(f, args)
                if args.len() == 1
                    && matches!(&**f, Expr::Path(base, h) if h == "held" && is_item(base)) =>
            {
                match &args[0] {
                    Expr::Name(n) if n == "state" => Some(Ty::State(self.tn.to_string())),
                    Expr::Name(n) => attribute_type(obj(self.t, "attributes").get(n), self.enums),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The type the core's metric evaluator gives a metric's value, each
    /// rule it breaks noted in `problems`, in evaluation order.
    fn kind(&self, typing: &mut Typing<'_>, e: &Expr, problems: &mut Vec<String>) -> Option<Ty> {
        let aggregate = match e {
            Expr::Call(f, args) => match &**f {
                // the body is the last argument, after a percentile's p
                Expr::Name(n) if ROW_AGGREGATES.contains(&n.as_str()) => {
                    Some((n.as_str(), args.last()))
                }
                _ => None,
            },
            Expr::Agg {
                kind,
                var: None,
                body,
                ..
            } => Some((kind.as_str(), body.as_deref())),
            _ => None,
        };
        if let Some((fn_, over)) = aggregate {
            if fn_ == "count" {
                return Some(Ty::Int);
            }
            let b = self.body(typing, over?)?;
            let order = !matches!(fn_, "sum" | "avg");
            let reads = ["int", "decimal", "number", "money", "duration"].contains(&b.tag())
                || order && matches!(b, Ty::Timestamp | Ty::Date);
            if !reads {
                let what = if order {
                    "a number, an amount or a duration, a time or a date"
                } else {
                    "a number, an amount or a duration"
                };
                problems.push(format!(
                    "{fn_} reads {}, and {fn_} over a metric's rows reads {what}",
                    b.text()
                ));
                return None;
            }
            return Some(if fn_ == "avg" && b == Ty::Int {
                Ty::Decimal(Some(6))
            } else {
                b
            });
        }
        match e {
            Expr::Num(Number::Int(_)) => Some(Ty::Int),
            Expr::Num(Number::Decimal(d)) => Some(match d.split_once('.') {
                Some((_, f)) => Ty::Number(u32::try_from(f.len()).unwrap_or(u32::MAX)),
                None => Ty::Int,
            }),
            Expr::Dur(..) => Some(Ty::Duration),
            Expr::Money(c, _) => Some(Ty::Money(c.clone())),
            Expr::Name(n) if n == "now" => Some(Ty::Timestamp),
            Expr::Neg(x) => self.kind(typing, x, problems),
            Expr::Bin(op @ (BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div), l, r) => {
                let a = self.kind(typing, l, problems);
                let b = self.kind(typing, r, problems);
                let (Some(a), Some(b)) = (a, b) else {
                    return None;
                };
                match metric_arith(*op, a, b) {
                    Ok(got) => got,
                    Err(p) => {
                        problems.push(p);
                        None
                    }
                }
            }
            _ => None,
        }
    }
}

/// What a metric's value breaks that the core's metric evaluator does not
/// compute, where the types it combines can be told: an aggregate over its
/// rows of what it does not compute over, a text, a bool, a member, an
/// identity or a state, and a sum of times; and a sum or a difference of
/// two types, a decimal and an int or two scales, whose fractional value
/// the core cannot place (the values audit's M2, the model's check 24;
/// `check-flows.py`'s `metric_type_errors`).
fn metric_type_errors(
    typing: &mut Typing<'_>,
    tn: &str,
    t: &Value,
    enums: &BTreeSet<String>,
) -> Found {
    let mut out = Found::new();
    for (mn, m) in obj(t, "metrics") {
        if inherited(t, "metrics", mn) {
            continue;
        }
        // step 2 refuses what does not parse
        let Ok(tree) = parse(&squash(&body_text(m, "expression"))) else {
            continue;
        };
        let rows = MetricRows {
            tn,
            t,
            enums,
            source: body_text(m, "source"),
            item: body_text(m, "item"),
        };
        let mut problems = Vec::new();
        rows.kind(typing, &tree, &mut problems);
        if let Some(p) = problems.first() {
            out.push(finding(
                segs!["types", tn, "metrics", mn, "expression"],
                "names",
                format!("metric {mn}'s value does not type: {p} (the model's check 24)"),
            ));
        }
    }
    out
}

/// What check 17 refuses of an add or a remove: a value of another kind
/// than its set's elements (the values audit's K-10).
#[allow(clippy::too_many_arguments)]
fn typed_element_error(
    typing: &mut Typing<'_>,
    t: &Value,
    body: &Value,
    tn: &str,
    x: &Value,
    env: &TyEnv,
    kinds: &Obj,
    enums: &BTreeSet<String>,
) -> Option<String> {
    let loc = body_text(body, "location");
    let spec = obj(t, "attributes").get(&loc)?;
    let typ = spec.get("type").map(py_str).unwrap_or_default();
    let reference = spec.get("reference").map(py_str).unwrap_or_default();
    let element = match typ.strip_suffix("[]") {
        Some(b) => serde_json::json!({ "type": b }),
        None => serde_json::json!({ "reference": reference.strip_suffix("[]")? }),
    };
    let tree = parse(&squash(&body_text(body, "expr"))).ok()?;
    typing.broken.clear();
    let got = typing.of(&tree, tn, Some(x), env, &BTreeSet::new())?;
    let want = written_kind(Some(&element), enums)?;
    let kind = typed_kind(&got)?;
    (!fits(&want, &kind, kinds)).then(|| {
        format!(
            "{}, to {loc}, whose elements are {}: an element must fit its set",
            got.text(),
            want.join(" ")
        )
    })
}

/// A loop's array of a type the typing tells that is no collection, a set
/// of objects or of values (the values audit's K-10).
fn typed_loop_error(
    typing: &mut Typing<'_>,
    body: &Value,
    tn: &str,
    x: &Value,
    env: &TyEnv,
) -> Option<String> {
    let tree = parse(&squash(&body_text(body, "array"))).ok()?;
    typing.broken.clear();
    let got = typing.of(&tree, tn, Some(x), env, &BTreeSet::new())?;
    if matches!(got, Ty::Objects(_) | Ty::Values(_))
        || typing.value_set_element(tn, &tree, env).is_some()
    {
        return None;
    }
    Some(format!("{}, which is no collection", got.text()))
}

/// What check 17 refuses of an assign by the type the typing infers, where
/// `value_kind` tells none: a value of another kind than its target's, as a
/// decimal computed into an int (the audit's values W2 and W3); and a
/// decimal of more places than its target's that is no product or quotient,
/// which §8.3 alone rounds (W5). `check-flows.py`'s `typed_write_error`.
#[allow(clippy::too_many_arguments)]
fn typed_write_error(
    typing: &mut Typing<'_>,
    t: &Value,
    body: &Value,
    tn: &str,
    x: &Value,
    env: &TyEnv,
    kinds: &Obj,
    enums: &BTreeSet<String>,
) -> Option<String> {
    let loc = body_text(body, "location");
    let attrs = obj(t, "attributes");
    let spec = attrs.get(&loc);
    let tree = parse(&squash(&body_text(body, "expr"))).ok()?;
    let target = attribute_type(spec, enums);
    typing.broken.clear();
    let got = typing.of(&tree, tn, Some(x), env, &BTreeSet::new());
    let (Some(target), Some(got)) = (target, got) else {
        return None;
    };
    // a set holds a set and nothing else does, where either was written and
    // refused at run time (the differential battery)
    if matches!(target, Ty::Values(_)) != matches!(got, Ty::Values(_)) {
        return Some(format!(
            "{}, to {loc}, which holds {}: a write's value must fit its target",
            got.text(),
            target.text()
        ));
    }
    let product = matches!(tree, Expr::Bin(BinOp::Mul | BinOp::Div, ..));
    let scale = match got {
        Ty::Decimal(Some(s)) | Ty::Number(s) => Some(s),
        _ => None,
    };
    if let (Ty::Decimal(Some(ts)), Some(s)) = (&target, scale)
        && s > *ts
        && !product
    {
        return Some(format!(
            "a decimal of scale {s}, to {loc}, which holds a decimal of scale {ts}: only a product or a quotient is rounded to its target"
        ));
    }
    let want = written_kind(spec, enums);
    let kind = typed_kind(&got);
    if let Some(told) = value_kind(&tree, tn, t, x, enums)
        && !(told == ["number"] && kind.as_deref() == Some(&["decimal".to_string()][..]))
    {
        return None;
    }
    if let (Some(want), Some(kind)) = (want, kind)
        && !fits(&want, &kind, kinds)
    {
        return Some(format!(
            "{}, to {loc}, which holds {}: a write's value must fit its target",
            got.text(),
            want.join(" ")
        ));
    }
    None
}

fn expression_type_errors(doc: &Value, library: &Obj) -> Found {
    let mut kinds = library.clone();
    for (tn, t) in types(doc) {
        kinds.insert(tn, t.clone());
    }
    let mut enums: BTreeSet<String> = keys(doc, "enumerations").into_iter().collect();
    for list_ in obj(doc, "imports").values() {
        if let Value::Array(names) = list_ {
            for n in names {
                let n = py_str(n);
                if !kinds.contains_key(&n) {
                    enums.insert(n);
                }
            }
        }
    }
    let cats = categories(doc);
    let evaluators = evaluator_names(doc);
    let known = std::cell::RefCell::new(BTreeMap::new());
    let mut typing = Typing {
        kinds: &kinds,
        enums: &enums,
        categories: &cats,
        evaluators: &evaluators,
        broken: Vec::new(),
        flow_names: false,
        known: &known,
    };
    let mut out = Found::new();
    #[allow(clippy::too_many_arguments)]
    fn check(
        typing: &mut Typing<'_>,
        out: &mut Found,
        where_: Path,
        name: &str,
        text: &str,
        tn: &str,
        x: Option<&Value>,
        env: &TyEnv,
        want: &[&str],
    ) {
        // step 2 refuses what does not parse
        let Ok(tree) = parse(&squash(text)) else {
            return;
        };
        typing.broken.clear();
        // a transition's guards and effects are check 58's, through its
        // body; any other place is the typing's
        typing.flow_names = !matches!(
            (where_.first(), where_.get(2)),
            (Some(Seg::Key(a)), Some(Seg::Key(b)))
                if a == "types" && (b == "conditions" || b == "transitions")
        );
        let got = typing.of(&tree, tn, x, env, &BTreeSet::new());
        if typing.broken.is_empty()
            && !want.is_empty()
            && let Some(got) = got
            && !want.contains(&got.tag())
        {
            typing
                .broken
                .push(format!("it is {}, not a bool", got.text()));
        }
        if let Some(first) = typing.broken.first() {
            out.push(finding(
                where_,
                "names",
                format!("{name} does not type: {first} (the model's check 24)"),
            ));
        }
    }
    for (tn, t) in types(doc) {
        for (cn, c) in obj(t, "conditions") {
            let text = c.get("expression").map(py_str).unwrap_or_default();
            check(
                &mut typing,
                &mut out,
                segs!["types", &tn, "conditions", cn, "expression"],
                &format!("condition {cn}"),
                &text,
                &tn,
                None,
                &TyEnv::new(),
                &["bool", "verdict"],
            );
        }
        for (i, spec) in obj(t, "invariants") {
            if !inherited(t, "invariants", i) {
                let text = spec.get("expression").map(py_str).unwrap_or_default();
                check(
                    &mut typing,
                    &mut out,
                    segs!["types", &tn, "invariants", i, "expression"],
                    &format!("invariant {i}"),
                    &text,
                    &tn,
                    None,
                    &TyEnv::new(),
                    &["bool"],
                );
            }
        }
        for (d, spec) in obj(t, "derived_attributes") {
            if !inherited(t, "derived_attributes", d) {
                let text = spec.get("expression").map(py_str).unwrap_or_default();
                check(
                    &mut typing,
                    &mut out,
                    segs!["types", &tn, "derived_attributes", d, "expression"],
                    &format!("derived attribute {d}"),
                    &text,
                    &tn,
                    None,
                    &TyEnv::new(),
                    &[],
                );
            }
        }
        for (a, spec) in obj(t, "attributes") {
            if let Some(w) = spec.get("unique").and_then(|u| u.get("where"))
                && !inherited(t, "attributes", a)
            {
                check(
                    &mut typing,
                    &mut out,
                    segs!["types", &tn, "attributes", a, "unique", "where"],
                    &format!("{tn}.{a}'s uniqueness condition"),
                    &py_str(w),
                    &tn,
                    None,
                    &TyEnv::new(),
                    &["bool"],
                );
            }
        }
        for (xn, x) in obj(t, "transitions") {
            for step in walk(x.get("effect")) {
                let where_ = ext(
                    segs!["types", &tn, "transitions", xn, "effect"],
                    step.path.clone(),
                );
                let env: TyEnv = binders(&kinds, &tn, x, &step.path)
                    .into_iter()
                    .map(|(n, k)| (n, k.map(Ty::Reference)))
                    .collect();
                for e in step_expressions(&step) {
                    check(
                        &mut typing,
                        &mut out,
                        where_.clone(),
                        &format!("{tn}.{xn}'s effect"),
                        &e,
                        &tn,
                        Some(x),
                        &env,
                        &[],
                    );
                }
                if step.kind == "assign"
                    && let Some(misfit) =
                        typed_write_error(&mut typing, t, step.body(), &tn, x, &env, &kinds, &enums)
                {
                    out.push(finding(
                        where_.clone(),
                        "names",
                        format!(
                            "{tn}.{xn} writes {}, {misfit} (the model's check 17)",
                            body_text(step.body(), "expr")
                        ),
                    ));
                }
                // an element added or removed fits its set, and a loop ranges
                // over a collection, where each published and the core
                // refused or did not run it (the values audit's K-10)
                if matches!(step.kind, "add" | "remove")
                    && let Some(misfit) = typed_element_error(
                        &mut typing,
                        t,
                        step.body(),
                        &tn,
                        x,
                        &env,
                        &kinds,
                        &enums,
                    )
                {
                    let verb = if step.kind == "add" {
                        "adds"
                    } else {
                        "removes"
                    };
                    out.push(finding(
                        where_.clone(),
                        "names",
                        format!(
                            "{tn}.{xn} {verb} {}, {misfit} (the model's check 17)",
                            body_text(step.body(), "expr")
                        ),
                    ));
                }
                if step.kind == "foreach"
                    && step.body().get("array").is_some_and(|a| !a.is_null())
                    && let Some(loose) = typed_loop_error(&mut typing, step.body(), &tn, x, &env)
                {
                    out.push(finding(
                        where_.clone(),
                        "names",
                        format!(
                            "{tn}.{xn} loops over {}, {loose} (the model's check 24)",
                            body_text(step.body(), "array")
                        ),
                    ));
                }
            }
        }
        out.extend(metric_type_errors(&mut typing, &tn, t, &enums));
    }
    let migration = doc.get("migration").cloned().unwrap_or(Value::Null);
    for (tn, given) in obj(&migration, "backfill") {
        for (a, e) in as_obj(given) {
            check(
                &mut typing,
                &mut out,
                segs!["migration", "backfill", tn, a],
                &format!("the backfill of {tn}.{a}"),
                &py_str(e),
                tn,
                None,
                &TyEnv::new(),
                &[],
            );
        }
    }
    out
}

/// Each argument a `call` or a `create` passes whose value does not fit the
/// input it is given to, where both kinds can be told, as a write's value
/// must fit its target: an input the transition reached declares, or an
/// attribute input, which writes the attribute of its name (the model's
/// check 2, its types; `check-flows.py`'s `argument_type_errors`).
fn argument_type_errors(doc: &Value, library: &Obj) -> Found {
    let mut kinds = library.clone();
    for (tn, t) in types(doc) {
        kinds.insert(tn, t.clone());
    }
    let enums: BTreeSet<String> = keys(doc, "enumerations").into_iter().collect();
    // a value no literal or name tells is typed as an expression is, where an
    // int passed to a time or an amount to an int was taken and the core
    // refused the call (the differential battery, 2026-10-16)
    let mut typing_enums = enums.clone();
    for list_ in obj(doc, "imports").values() {
        if let Value::Array(names) = list_ {
            for n in names {
                let n = py_str(n);
                if !kinds.contains_key(&n) {
                    typing_enums.insert(n);
                }
            }
        }
    }
    let cats = categories(doc);
    let evaluators = evaluator_names(doc);
    let known = std::cell::RefCell::new(BTreeMap::new());
    let mut typing = Typing {
        kinds: &kinds,
        enums: &typing_enums,
        categories: &cats,
        evaluators: &evaluators,
        broken: Vec::new(),
        flow_names: false,
        known: &known,
    };
    let mut out = Found::new();
    for (tn, t) in types(doc) {
        for (xn, x) in obj(t, "transitions") {
            for step in walk(x.get("effect")) {
                if !matches!(step.kind, "call" | "create") {
                    continue;
                }
                let body = &step.st[step.kind];
                let k = if step.kind == "call" {
                    call_target(
                        &kinds,
                        t,
                        &tn,
                        x,
                        body,
                        &binders(&kinds, &tn, x, &step.path),
                    )
                } else {
                    body.get("type").map(py_str)
                };
                let Some(k) = k else { continue };
                let ct = body.get("transition").map(py_str).unwrap_or_default();
                let Some(callee) = kinds.get(&k) else {
                    continue;
                };
                // a transition no type declares is the names checks'
                let Some(reached) = obj(callee, "transitions").get(&ct).filter(|r| !r.is_null())
                else {
                    continue;
                };
                for (j, e) in obj(body, "inputs") {
                    let mut spec = obj(reached, "inputs").get(j).filter(|s| !s.is_null());
                    let attribute_input = strings(reached, "required_inputs")
                        .iter()
                        .chain(&strings(reached, "optional_inputs"))
                        .any(|n| n == j);
                    if spec.is_none() && attribute_input {
                        spec = obj(callee, "attributes").get(j);
                    }
                    let target = written_kind(spec, &enums);
                    let text = py_str(e);
                    // step 2 refuses what does not parse
                    let Ok(tree) = parse(&squash(&text)) else {
                        continue;
                    };
                    let value = value_kind(&tree, &tn, t, x, &enums).or_else(|| {
                        typing.broken.clear();
                        typing.flow_names = false;
                        let env: TyEnv = binders(&kinds, &tn, x, &step.path)
                            .into_iter()
                            .map(|(n, c)| (n, c.map(Ty::Reference)))
                            .collect();
                        let got = typing.of(&tree, &tn, Some(x), &env, &BTreeSet::new())?;
                        if !typing.broken.is_empty() {
                            return None;
                        }
                        // an identity and a string the typing gives the
                        // benefit of the doubt, as it does in a comparison,
                        // where `actor.principal` passed to an identity input
                        // was refused (`alike`)
                        let kind = typed_kind(&got)?;
                        let doubt = target.as_ref().is_some_and(|t| {
                            matches!(
                                (t[0].as_str(), kind[0].as_str()),
                                ("identity", "string") | ("string", "identity")
                            )
                        });
                        (!doubt).then_some(kind)
                    });
                    if let (Some(target), Some(value)) = (target, value)
                        && !fits(&target, &value, &kinds)
                    {
                        out.push(finding(
                            ext(segs!["types", &tn, "transitions", xn, "effect"], step.path.clone()),
                            "names",
                            format!(
                                "{tn}.{xn} passes {text}, {}, to {k}.{ct}'s input '{j}', which takes {}: an argument must fit its input (the model's check 2)",
                                value.join(" "),
                                target.join(" ")
                            ),
                        ));
                    }
                }
            }
        }
    }
    out
}

/// Each assign whose value does not fit its target, and each range that is
/// not an int (the model's check 17, ADR-0146).
fn write_type_errors(doc: &Value, tn: &str, t: &Value, library: &Obj) -> Found {
    let enums: BTreeSet<String> = keys(doc, "enumerations").into_iter().collect();
    let mut out = Found::new();
    // an actor's identity is written only at its creation, which the record
    // names actors by (ADR-0205 decision 5)
    let identity = obj(t, "attributes")
        .iter()
        .find(|(_, spec)| spec.get("actor_kind").is_some())
        .map(|(a, _)| a.clone());
    for (xn, x) in obj(t, "transitions") {
        if let Some(identity) = &identity
            && !matches!(str_or(x, "kind", "").as_str(), "initial" | "erasure")
        {
            let by_input = strings(x, "required_inputs")
                .iter()
                .chain(strings(x, "optional_inputs").iter())
                .any(|n| n == identity);
            let by_step = walk(x.get("effect")).iter().any(|step| match step.kind {
                "assign" => body_text(step.body(), "location") == *identity,
                "clear" => step
                    .body()
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(identity.as_str()))),
                _ => false,
            });
            if by_input || by_step {
                out.push(finding(
                    segs!["types", tn, "transitions", xn],
                    "names",
                    format!(
                        "{tn}.{xn} writes {identity}, the identity {tn}'s actors are named by, which is written only at their creation (the model's check 17)"
                    ),
                ));
            }
        }
        for step in walk(x.get("effect")) {
            let body = step.body();
            if step.kind == "foreach"
                && let Some(range) = body.get("range")
            {
                let got = parse(&squash(&py_str(range)))
                    .ok()
                    .and_then(|e| range_kind(&e, tn, t, x, &enums));
                if let Some(got) = got.filter(|g| g != &["int"]) {
                    out.push(finding(
                        ext(
                            segs!["types", tn, "transitions", xn, "effect"],
                            ext(&step.path, segs!["foreach", "range"]),
                        ),
                        "names",
                        format!(
                            "{tn}.{xn} loops over a range of {}, and a range is an int",
                            got.join(" ")
                        ),
                    ));
                }
            }
            if step.kind != "assign" {
                continue;
            }
            let loc = body_text(body, "location");
            let target = written_kind(obj(t, "attributes").get(&loc), &enums);
            let expr = body_text(body, "expr");
            let Ok(tree) = parse(&squash(&expr)) else {
                continue;
            };
            let value = value_kind(&tree, tn, t, x, &enums);
            if let (Some(target), Some(value)) = (target, value)
                && !fits(&target, &value, library)
            {
                out.push(finding(
                    ext(segs!["types", tn, "transitions", xn, "effect"], step.path.clone()),
                    "names",
                    format!(
                        "{tn}.{xn} writes {expr}, {}, to {loc}, which holds {}: a write's value must fit its target (the model's check 17)",
                        value.join(" "),
                        target.join(" ")
                    ),
                ));
            }
        }
    }
    out
}

/// Every value an expression names is a state or an enumeration's member.
/// What a module's expressions may name qualified: its enumerations' members
/// and its types' states, and the names it imports, read once for every
/// type's expressions.
struct Values {
    enums: BTreeMap<String, BTreeSet<String>>,
    own: BTreeMap<String, BTreeSet<String>>,
    imported: BTreeSet<String>,
}

impl Values {
    fn of(doc: &Value) -> Values {
        Values {
            enums: obj(doc, "enumerations")
                .iter()
                .map(|(n, m)| {
                    (
                        n.clone(),
                        match m {
                            Value::Array(a) => a.iter().map(py_str).collect(),
                            _ => BTreeSet::new(),
                        },
                    )
                })
                .collect(),
            own: types(doc)
                .into_iter()
                .map(|(k, v)| (k, keys(v, "states").into_iter().collect()))
                .collect(),
            imported: imported(doc).into_iter().collect(),
        }
    }
}

fn value_errors(values: &Values, tn: &str, t: &Value) -> Found {
    let states: BTreeSet<String> = keys(t, "states").into_iter().collect();
    let Values {
        enums,
        own,
        imported,
    } = values;
    let mut exprs: Vec<(Path, String, String)> = Vec::new();
    for sec in ["conditions", "invariants", "derived_attributes"] {
        for (n, c) in obj(t, sec) {
            exprs.push((
                segs!["types", tn, sec, n, "expression"],
                n.clone(),
                c.get("expression").map(py_str).unwrap_or_default(),
            ));
        }
    }
    for (a, spec) in obj(t, "attributes") {
        if let Some(d) = spec.get("default") {
            exprs.push((
                segs!["types", tn, "attributes", a, "default"],
                format!("{tn}.{a}'s default"),
                py_str(d),
            ));
        }
    }
    for (a, spec) in obj(t, "attributes") {
        if let Some(Value::Object(u)) = spec.get("unique")
            && let Some(w) = u.get("where")
        {
            exprs.push((
                segs!["types", tn, "attributes", a, "unique", "where"],
                a.clone(),
                py_str(w),
            ));
        }
    }
    for (xn, x) in obj(t, "transitions") {
        for step in walk(x.get("effect")) {
            for e in step_expressions(&step) {
                exprs.push((
                    ext(
                        segs!["types", tn, "transitions", xn, "effect"],
                        step.path.clone(),
                    ),
                    xn.clone(),
                    e,
                ));
            }
        }
    }
    for (o, ob) in obj(t, "observations") {
        for (n, i) in obj(ob, "invariants") {
            exprs.push((
                segs![
                    "types",
                    tn,
                    "observations",
                    o,
                    "invariants",
                    n,
                    "expression"
                ],
                n.clone(),
                i.get("expression").map(py_str).unwrap_or_default(),
            ));
        }
    }
    for (mn, m) in obj(t, "metrics") {
        if !has(m, "source") {
            continue;
        }
        let base = segs!["types", tn, "metrics", mn];
        let name = format!("metric {mn}");
        for k in ["filter", "time_dimension", "expression"] {
            if let Some(e) = m.get(k).filter(|e| super::doc::truthy(Some(e))) {
                exprs.push((ext(&base, segs![k]), name.clone(), py_str(e)));
            }
        }
        for (d, e) in obj(m, "dimensions") {
            exprs.push((ext(&base, segs!["dimensions", d]), name.clone(), py_str(e)));
        }
        for (f, c) in obj(m, "flag_when") {
            exprs.push((ext(&base, segs!["flag_when", f]), name.clone(), py_str(c)));
        }
    }
    let mut out = Found::new();
    for (p, name, text) in exprs {
        // a string literal's text names nothing, where "EU" read as a state
        // (twelfth production review)
        let text = STRING_LITERAL.replace_all(&text, |m: &regex::Captures<'_>| {
            format!("\"{}\"", " ".repeat(m[0].chars().count() - 2))
        });
        for m in QUALIFIED.captures_iter(&text) {
            let (owner, value) = (&m[1], &m[2]);
            let known = enums.get(owner).or_else(|| own.get(owner));
            match known {
                None if !imported.contains(owner) => out.push(finding(
                    p.clone(),
                    "names",
                    format!("{name} names {owner}.{value}, and no enumeration or type {owner} is declared in this module or imported"),
                )),
                Some(k) if !k.contains(value) => {
                    let what = if enums.contains_key(owner) { "enumeration" } else { "type" };
                    out.push(finding(p.clone(), "names", format!("{name} names {owner}.{value}, and the {what} {owner} has no {value}")));
                }
                _ => {}
            }
        }
        let chars: Vec<char> = text.chars().collect();
        for (word, _, end) in bare_upper(&text) {
            if word.len() == 3 && word.chars().all(|c| c.is_ascii_uppercase()) {
                // the text after the word, read where it is, where it was
                // collected again for each word, quadratic in a long one
                let mut at = end;
                while chars.get(at).is_some_and(|c| c.is_whitespace()) {
                    at += 1;
                }
                if at > end {
                    if chars.get(at) == Some(&'-') {
                        at += 1;
                    }
                    if chars.get(at).is_some_and(|c| c.is_numeric()) {
                        continue; // a money literal's currency, as in USD 19.99
                    }
                }
            }
            if !states.contains(&word) {
                out.push(finding(
                    p.clone(),
                    "names",
                    format!("{name} names {word}, which is not a state of {tn}; write an enumeration value qualified, as <Enumeration>.<VALUE>"),
                ));
            }
        }
    }
    out
}

/// Every name an expression reads is a member of the object it is evaluated
/// on.
fn read_errors(tn: &str, t: &Value, categories: &BTreeSet<String>) -> Found {
    let derived = keys(t, "derived_attributes");
    let mut members: BTreeSet<String> = keys(t, "attributes").into_iter().collect();
    members.extend(keys(t, "observations"));
    members.extend(derived.iter().cloned());
    members.extend(NOT_AN_ATTRIBUTE.iter().map(|s| s.to_string()));
    members.extend(OBJECT_MEMBERS.iter().map(|s| s.to_string()));
    members.extend(categories.iter().cloned());
    let request_only = set(REQUEST_ONLY);
    let without_request: BTreeSet<String> = members.difference(&request_only).cloned().collect();
    // each derived attribute's place, so one reads those after it without a
    // set of them made for each (tenth production review)
    let placed: BTreeMap<&str, usize> = derived
        .iter()
        .enumerate()
        .map(|(i, d)| (d.as_str(), i))
        .collect();
    let mut exprs: Vec<(Path, String, String, Scope<'_>)> = Vec::new();
    for sec in ["conditions", "invariants"] {
        // an invariant is checked after a write, where no request's inputs
        // or event are (the model's check 5)
        let readable = if sec == "invariants" {
            &without_request
        } else {
            &members
        };
        for (n, c) in obj(t, sec) {
            exprs.push((
                segs!["types", tn, sec, n, "expression"],
                n.clone(),
                c.get("expression").map(py_str).unwrap_or_default(),
                Scope::Of(readable, None),
            ));
        }
    }
    for (a, spec) in obj(t, "attributes") {
        if let Some(Value::Object(u)) = spec.get("unique")
            && let Some(w) = u.get("where")
        {
            exprs.push((
                segs!["types", tn, "attributes", a, "unique", "where"],
                format!("{tn}.{a}'s uniqueness condition"),
                py_str(w),
                Scope::Of(&without_request, None),
            ));
        }
    }
    let mut out = Found::new();
    for (i, d) in derived.iter().enumerate() {
        let p = segs!["types", tn, "derived_attributes", d, "expression"];
        let expr = obj(t, "derived_attributes")
            .get(d)
            .and_then(|s| s.get("expression"))
            .map(py_str)
            .unwrap_or_default();
        let read = reads(&expr);
        for r in read
            .iter()
            .filter(|r| placed.get(r.as_str()).is_some_and(|at| *at >= i))
        {
            let message = if r == d {
                format!("derived attribute '{d}' reads itself")
            } else {
                format!(
                    "derived attribute '{d}' reads '{r}', which is declared after it; a derived attribute reads only those above it"
                )
            };
            out.push(finding(p.clone(), "order", message));
        }
        exprs.push((
            p,
            format!("derived attribute '{d}'"),
            expr,
            Scope::Later(&without_request, &placed, i),
        ));
    }
    for (xn, x) in obj(t, "transitions") {
        let walked = walk(x.get("effect"));
        let mut binds: BTreeSet<String> = BTreeSet::new();
        for s in &walked {
            if s.kind == "foreach" {
                binds.insert(body_text(s.body(), "item"));
            }
            if s.kind == "create"
                && let Some(r) = s
                    .body()
                    .get("result")
                    .filter(|r| super::doc::truthy(Some(r)))
            {
                binds.insert(py_str(r));
            }
        }
        let binds = std::rc::Rc::new(binds);
        for s in &walked {
            for e in step_expressions(s) {
                exprs.push((
                    ext(
                        segs!["types", tn, "transitions", xn, "effect"],
                        s.path.clone(),
                    ),
                    xn.clone(),
                    e,
                    Scope::Of(&members, Some(binds.clone())),
                ));
            }
        }
    }
    for (o, ob) in obj(t, "observations") {
        let mut fields: BTreeSet<String> = keys(ob, "attributes").into_iter().collect();
        fields.extend(OBSERVATION_MEMBERS.iter().map(|s| s.to_string()));
        for (n, i) in obj(ob, "invariants") {
            exprs.push((
                segs![
                    "types",
                    tn,
                    "observations",
                    o,
                    "invariants",
                    n,
                    "expression"
                ],
                n.clone(),
                i.get("expression").map(py_str).unwrap_or_default(),
                Scope::Own(std::rc::Rc::new(fields.clone())),
            ));
        }
    }
    for (p, name, text, scope) in exprs {
        let in_observation = p.contains(&Seg::Key("observations".into()));
        for r in reads(&text).iter().filter(|r| !scope.holds(r)) {
            let message = if request_only.contains(r) {
                format!("{name} reads '{r}', which only a transition has")
            } else if r == "labels" {
                format!(
                    "{name} reads labels, which are read only as a metric's source, source: labels (check 55)"
                )
            } else if in_observation {
                let o = match p.get(3) {
                    Some(Seg::Key(o)) => o.clone(),
                    _ => String::new(),
                };
                format!("{name} reads '{r}', which is not a field of the observation {o}")
            } else if WITHDRAWN_MEMBERS.contains(&r.as_str()) {
                format!(
                    "{name} reads '{r}', which is withdrawn before 1.0.0, no store keeping it, and may return with a use (ADR-0210 decision 3)"
                )
            } else {
                format!("{name} reads '{r}', which {tn} does not declare")
            };
            out.push(finding(p.clone(), "names", message));
        }
    }
    out
}

/// The names an expression of a type may read, described rather than
/// copied for each expression, where a set of every member made for each
/// derived attribute made a type's check quadratic in them (tenth production
/// review).
enum Scope<'s> {
    /// A set, and the names its expression binds beside it.
    Of(&'s BTreeSet<String>, Option<std::rc::Rc<BTreeSet<String>>>),
    /// A set, and the derived attributes from a place on.
    Later(&'s BTreeSet<String>, &'s BTreeMap<&'s str, usize>, usize),
    /// A set of its own.
    Own(std::rc::Rc<BTreeSet<String>>),
}

impl Scope<'_> {
    fn holds(&self, name: &str) -> bool {
        match self {
            Scope::Of(set, bound) => {
                set.contains(name) || bound.as_ref().is_some_and(|b| b.contains(name))
            }
            Scope::Later(set, placed, from) => {
                set.contains(name) || placed.get(name).is_some_and(|at| at >= from)
            }
            Scope::Own(set) => set.contains(name),
        }
    }
}

/// Whether a single reference end holds the value (§3.3).
fn stores(spec: &Value, library: &Obj) -> bool {
    let target = ref_of(spec);
    let opposite = spec.get("opposite").map(py_str).unwrap_or_default();
    let other = library
        .get(&target)
        .and_then(|t| obj(t, "attributes").get(&opposite))
        .filter(|o| !o.is_null() && has(spec, "opposite"));
    other.is_none_or(|o| ref_of(o).ends_with("[]")) || flag(spec, "stored")
}

/// No invariant reads an observation collection, directly or through a
/// derived attribute (ADR-0099, ADR-0146).
fn observation_read_errors(tn: &str, t: &Value) -> Found {
    let kinds: BTreeSet<String> = keys(t, "observations").into_iter().collect();
    if kinds.is_empty() {
        return Found::new();
    }
    let derived: BTreeMap<String, BTreeSet<String>> = obj(t, "derived_attributes")
        .iter()
        .map(|(d, spec)| {
            let read = reads(&spec.get("expression").map(py_str).unwrap_or_default());
            (d.clone(), read.intersection(&kinds).cloned().collect())
        })
        .collect();
    let mut out = Found::new();
    for (n, inv) in obj(t, "invariants") {
        let names = reads(&inv.get("expression").map(py_str).unwrap_or_default());
        let mut hit: BTreeSet<String> = names.intersection(&kinds).cloned().collect();
        for d in names.iter().filter(|d| derived.contains_key(*d)) {
            hit.extend(derived[d].iter().cloned());
        }
        if !hit.is_empty() {
            out.push(finding(
                segs!["types", tn, "invariants", n],
                "names",
                format!(
                    "invariant {n} reads the observations {}; a recorded fact matters to the flow only through a guard, so no invariant reads one",
                    hit.into_iter().collect::<Vec<_>>().join(", ")
                ),
            ));
        }
    }
    out
}

/// A name an effect binds shares no name with a member, a reserved word or
/// a name an enclosing loop binds (ADR-0146, ADR-0147).
fn binder_errors(tn: &str, t: &Value) -> Found {
    let mut members: BTreeSet<String> = keys(t, "attributes").into_iter().collect();
    members.extend(keys(t, "derived_attributes"));
    members.extend(keys(t, "observations"));
    let mut out = Found::new();
    fn scan(
        steps: &[Value],
        at: Path,
        bound: &BTreeSet<String>,
        xn: &str,
        tn: &str,
        members: &BTreeSet<String>,
        out: &mut Found,
    ) {
        for (i, st) in steps.iter().enumerate() {
            let Some(k) = step_kind(st) else { continue };
            let body = st.get(k).unwrap_or(&Value::Null);
            let name = match k {
                "create" => body.get("result").filter(|r| !r.is_null()).map(py_str),
                "foreach" => body.get("item").filter(|r| !r.is_null()).map(py_str),
                _ => None,
            };
            let where_ = ext(
                segs!["types", tn, "transitions", xn, "effect"],
                ext(&at, segs![i]),
            );
            if let Some(name) = &name {
                if members.contains(name) {
                    out.push(finding(where_.clone(), "names", format!("{tn}.{xn} binds '{name}', which is a member of {tn}; a bound name shares no name with a member")));
                } else if BINDER_RESERVED.contains(&name.as_str()) {
                    out.push(finding(where_.clone(), "names", format!("{tn}.{xn} binds '{name}', a word an expression resolves first or every object has (ADR-0147)")));
                } else if k == "foreach" && bound.contains(name) {
                    out.push(finding(where_.clone(), "names", format!("{tn}.{xn} binds '{name}' in a loop inside one that binds it already (ADR-0147)")));
                }
            }
            if k == "foreach" {
                let mut inner = bound.clone();
                if let Some(n) = name {
                    inner.insert(n);
                }
                let steps = list(body, "steps");
                scan(
                    steps,
                    ext(&at, segs![i, "foreach", "steps"]),
                    &inner,
                    xn,
                    tn,
                    members,
                    out,
                );
            }
        }
    }
    for (xn, x) in obj(t, "transitions") {
        scan(
            list(x, "effect"),
            Path::new(),
            &BTreeSet::new(),
            xn,
            tn,
            &members,
            &mut out,
        );
    }
    out
}

/// A type scan reads only what the store holds indexed on the scanned type
/// (the model's check 7).
fn scan_errors(tn: &str, t: &Value, library: &Obj) -> Found {
    fn readable(tt: &Value, m: &str) -> bool {
        let spec = obj(tt, "attributes").get(m).filter(|s| !s.is_null());
        if let Some(spec) = spec {
            return flag(spec, "indexed") || flag(spec, "unique") || has(spec, "reference");
        }
        let derived = obj(tt, "derived_attributes")
            .get(m)
            .filter(|d| !d.is_null());
        m == "state"
            || m == "id"
            || derived.is_some_and(|d| flag(d, "indexed"))
            || derived.is_none()
    }
    fn reads_of(
        n: &Expr,
        var: &str,
        scanned: &Value,
        scanned_name: &str,
        found: &mut BTreeSet<(String, String)>,
    ) {
        if let Expr::Path(b, m) = n
            && matches!(&**b, Expr::Name(v) if v == var)
            && !readable(scanned, m)
        {
            found.insert((scanned_name.to_string(), format!("{var}.{m}")));
        }
        for c in children(n) {
            reads_of(c, var, scanned, scanned_name, found);
        }
    }
    fn walk_tree(e: &Expr, t: &Value, library: &Obj, found: &mut BTreeSet<(String, String)>) {
        if let Expr::Agg {
            var: Some(var),
            collection: Some(c),
            filter,
            body,
            ..
        } = e
            && let Expr::Name(cn) = &**c
            && library.contains_key(cn)
            && !obj(t, "attributes").contains_key(cn)
        {
            let scanned = &library[cn];
            for part in [filter, body].into_iter().flatten() {
                reads_of(part, var, scanned, cn, found);
            }
        }
        for c in children(e) {
            walk_tree(c, t, library, found);
        }
    }
    let mut rules: Vec<(&str, String, String, Path)> = obj(t, "invariants")
        .iter()
        .map(|(n, i)| {
            (
                "invariant",
                n.clone(),
                i.get("expression").map(py_str).unwrap_or_default(),
                segs!["types", tn, "invariants", n],
            )
        })
        .collect();
    rules.extend(obj(t, "conditions").iter().map(|(n, c)| {
        (
            "condition",
            n.clone(),
            c.get("expression").map(py_str).unwrap_or_default(),
            segs!["types", tn, "conditions", n],
        )
    }));
    let mut out = Found::new();
    for (k, name, text, p) in rules {
        let Ok(tree) = parse(&squash(&text)) else {
            continue;
        };
        let mut found = BTreeSet::new();
        walk_tree(&tree, t, library, &mut found);
        for (scanned, read) in found {
            out.push(finding(
                p.clone(),
                "names",
                format!("{k} {name} scans every {scanned} and reads {read}, which is not indexed: a type scan reads only indexed or unique attributes, references, the state and indexed derived attributes (the model's check 7)"),
            ));
        }
    }
    out
}

/// An expression's direct subexpressions, as `flowexpr`'s tuples hold them.
pub fn children(e: &Expr) -> Vec<&Expr> {
    match e {
        Expr::Param(_) | Expr::Lit(_) => Vec::new(),
        Expr::Num(_)
        | Expr::Str(_)
        | Expr::Bool(_)
        | Expr::Dur(..)
        | Expr::Money(..)
        | Expr::Name(_) => Vec::new(),
        Expr::Path(b, _) => vec![b],
        Expr::Call(f, args) => {
            let mut v: Vec<&Expr> = vec![f];
            v.extend(args.iter());
            v
        }
        Expr::Agg {
            collection,
            filter,
            body,
            ..
        } => [collection, filter, body]
            .into_iter()
            .flatten()
            .map(|b| &**b)
            .collect(),
        Expr::Metric {
            name,
            binds,
            over,
            fresh,
        } => {
            let mut v: Vec<&Expr> = vec![name];
            v.extend(binds.iter().map(|(_, x)| x));
            v.extend([over, fresh].into_iter().flatten().map(|b| &**b));
            v
        }
        Expr::Changed(_, ev) => vec![ev],
        Expr::Set(items) => items.iter().collect(),
        Expr::Not(x) | Expr::Neg(x) | Expr::IsNull(x, _) => vec![x],
        Expr::In(l, r) | Expr::Bin(_, l, r) => vec![l, r],
        Expr::If(c, a, b) => vec![c, a, b],
    }
}

static FLOW_DATA_READ: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b(?:time_in|entered_at)\(|\bthis\.(?:intervals|transitions)\b"));

/// An indexed derived attribute reads only this object's stored, indexed
/// values (the model's check 46).
fn indexed_errors(tn: &str, t: &Value, library: &Obj) -> Found {
    let attrs = obj(t, "attributes");
    let mut out = Found::new();
    for (d, spec) in obj(t, "derived_attributes") {
        if !flag(spec, "indexed") {
            continue;
        }
        let text = squash(&spec.get("expression").map(py_str).unwrap_or_default());
        let mut why = Vec::new();
        let mut names = reads(&text);
        names.remove("state");
        names.remove("this");
        for r in names {
            let a = attrs.get(&r).filter(|a| !a.is_null());
            if r == "now" {
                why.push("reads now, the clock".to_string());
            } else if let Some(a) = a {
                if ref_of(a).ends_with("[]") || str_or(a, "type", "").ends_with("[]") {
                    why.push(format!("reads '{r}', a set"));
                } else if has(a, "reference") && read_through(&text, &r) {
                    why.push(format!("reads through '{r}' into another object"));
                } else if has(a, "reference") && !stores(a, library) {
                    why.push(format!(
                        "reads '{r}', whose value the other end of the relationship stores"
                    ));
                } else if !has(a, "reference") && !flag(a, "indexed") {
                    why.push(format!("reads '{r}', which is not indexed"));
                }
            } else {
                why.push(format!("reads '{r}', which is not a stored attribute"));
            }
        }
        if FLOW_DATA_READ.is_match(&text) {
            why.push("reads the object's flow data".into());
        }
        for w in why {
            out.push(finding(
                segs!["types", tn, "derived_attributes", d, "indexed"],
                "names",
                format!("derived attribute '{d}' is indexed and {w}"),
            ));
        }
    }
    out
}

/// A string, or an enumeration, which a format fills with its member's name.
fn is_text(spec: &Value) -> bool {
    let typ = str_or(spec, "type", "");
    let head: String = typ
        .chars()
        .take_while(|c| c.is_ascii_alphabetic() || *c == '_')
        .collect();
    let k = if flag(spec, "type") {
        head
    } else {
        String::new()
    };
    !typ.ends_with("[]")
        && (k == "string"
            || (k.chars().next().is_some_and(char::is_uppercase)
                && !BUILT_IN_TYPES.contains(&k.as_str())))
}

/// What every creation of the type writes (the model's check 8).
fn creation_writes(t: &Value) -> BTreeSet<String> {
    let given: BTreeSet<String> = obj(t, "attributes")
        .iter()
        .filter(|(_, s)| has(s, "default") || text(s, "type") == Some("counter"))
        .map(|(a, _)| a.clone())
        .collect();
    let mut sets: Vec<BTreeSet<String>> = Vec::new();
    for x in obj(t, "transitions").values() {
        if kind(x) != "initial" {
            continue;
        }
        let req: BTreeSet<String> = strings(x, "required_inputs").into_iter().collect();
        let mut s = given.clone();
        s.extend(req.iter().cloned());
        for (e, loc) in assigns(x) {
            if present(&e, &req, &required_input_names(x)) {
                s.insert(loc);
            }
        }
        sets.push(s);
    }
    match sets.split_first() {
        Some((first, rest)) => rest.iter().fold(first.clone(), |acc, s| {
            acc.intersection(s).cloned().collect()
        }),
        None => BTreeSet::new(),
    }
}

static FORMAT_PART: LazyLock<Regex> = LazyLock::new(|| re(r"\[|\]|\{([^{}]*)\}"));

/// A minted identifier and the uniqueness forms (the model's check 44).
fn identifier_errors(doc: &Value, tn: &str, t: &Value, library: &Obj) -> Found {
    if flag(t, "abstract") {
        return Found::new();
    }
    let attrs = obj(t, "attributes");
    let imported = imported(doc);
    let written = creation_writes(t);
    // what "any one of the type's creations does not write" asks of a type
    // with none, a mirror, nothing (ADR-0169)
    let creates = obj(t, "transitions").values().any(|x| kind(x) == "initial");
    let mut out = Found::new();
    for (a, spec) in attrs {
        let here = if inherited(t, "attributes", a) {
            segs!["types", tn]
        } else {
            segs!["types", tn, "attributes", a]
        };
        let u = spec.get("unique");
        let ident = spec
            .get("identifier")
            .filter(|i| super::doc::truthy(Some(i)));
        if let Some(Value::Object(uo)) = u {
            for w in uo
                .get("with")
                .map(|w| match w {
                    Value::Array(a) => a.iter().map(py_str).collect::<Vec<_>>(),
                    _ => Vec::new(),
                })
                .unwrap_or_default()
            {
                if !attrs.contains_key(&w) {
                    out.push(finding(
                        ext(&here, segs!["unique"]),
                        "names",
                        format!("{tn}.{a} is unique with '{w}', which {tn} does not declare"),
                    ));
                }
            }
        }
        if u.and_then(Value::as_str) == Some("in_scope") && !ident.is_some_and(|i| flag(i, "scope"))
        {
            out.push(finding(
                ext(&here, segs!["unique"]),
                "names",
                format!("{tn}.{a} is unique in scope, and has no identifier with a scope"),
            ));
        }
        if text(spec, "type") == Some("counter") {
            if optional(spec) {
                out.push(finding(
                    here.clone(),
                    "names",
                    format!("{tn}.{a} is a counter, which is never absent, so it is not optional"),
                ));
            }
            if has(spec, "default") {
                out.push(finding(
                    here.clone(),
                    "names",
                    format!("{tn}.{a} is a counter, which starts at zero, so it has no default"),
                ));
            }
        }
        if let Some(d) = spec.get("default") {
            let mut r = reads(&py_str(d));
            r.remove("now");
            for r in r {
                out.push(finding(
                    ext(&here, segs!["default"]),
                    "names",
                    format!("{tn}.{a}'s default reads '{r}'; a default is written before anything else, so it is a literal, a qualified value or now"),
                ));
            }
        }
        let Some(ident) = ident else { continue };
        let where_ = ext(&here, segs!["identifier"]);
        if text(spec, "type") != Some("string") {
            out.push(finding(
                where_.clone(),
                "names",
                format!(
                    "{tn}.{a} has an identifier, which is minted as text, so its type is string"
                ),
            ));
        }
        let sequence = ident.get("sequence").map(py_str).unwrap_or_default();
        if !obj(doc, "sequences").contains_key(&sequence) && !imported.contains(&sequence) {
            out.push(finding(
                ext(&where_, segs!["sequence"]),
                "names",
                format!("{tn}.{a} is minted from {sequence}, which is neither a sequence of this module nor imported"),
            ));
        }
        if let Some(sc) = ident.get("scope").filter(|s| !s.is_null()).map(py_str) {
            match attrs.get(&sc).filter(|s| !s.is_null()) {
                None => out.push(finding(
                    ext(&where_, segs!["scope"]),
                    "names",
                    format!("{tn}.{a} is scoped by '{sc}', which {tn} does not declare"),
                )),
                Some(ss) if kind_text_any(ss).ends_with("[]") => out.push(finding(
                    ext(&where_, segs!["scope"]),
                    "names",
                    format!("{tn}.{a} is scoped by '{sc}', a set; a scope is one value"),
                )),
                Some(ss) if !has(ss, "reference") && !flag(ss, "indexed") => out.push(finding(
                    ext(&where_, segs!["scope"]),
                    "names",
                    format!("{tn}.{a} is scoped by '{sc}', which is not indexed"),
                )),
                Some(_) if creates && !written.contains(&sc) => out.push(finding(
                    ext(&where_, segs!["scope"]),
                    "names",
                    format!(
                        "{tn}.{a} is scoped by '{sc}', which not every creation of {tn} writes"
                    ),
                )),
                Some(_) => {}
            }
            // a scope is kept with its sequence, which no erasure reaches
            // (the audit's personal data S27)
            if attrs.get(&sc).is_some_and(|ss| flag(ss, "personal")) {
                out.push(finding(
                    ext(&where_, segs!["scope"]),
                    "names",
                    format!("{tn}.{a} is scoped by '{sc}', which is personal; a sequence keeps its scope's value, which no erasure reaches"),
                ));
            }
        }
        let fmt = ident.get("format").map(py_str).unwrap_or_default();
        let fw = ext(&where_, segs!["format"]);
        let (mut depth, mut numbered) = (0usize, false);
        for m in FORMAT_PART.captures_iter(&fmt) {
            let whole = &m[0];
            if whole == "[" {
                depth += 1;
                continue;
            }
            if whole == "]" {
                if depth == 0 {
                    out.push(finding(
                        fw.clone(),
                        "names",
                        format!("{tn}.{a}'s format closes a ] it did not open"),
                    ));
                }
                depth = depth.saturating_sub(1);
                continue;
            }
            let ph = &m[1];
            if PLACE_N.is_match(ph) {
                numbered = true;
                if !PLACE_N_DIGITS.is_match(ph) {
                    out.push(finding(
                        fw.clone(),
                        "names",
                        format!(
                            "{tn}.{a}'s format has {{{ph}}}, whose width is not a number of digits"
                        ),
                    ));
                } else if ph
                    .strip_prefix("n:")
                    .and_then(|w| w.parse::<u64>().ok().or(Some(u64::MAX)))
                    .is_some_and(|w| w > crate::model::IDENTIFIER_WIDTH as u64)
                {
                    // a width a number can fill, where one of a million
                    // panicked as each object was made (tenth production
                    // review)
                    out.push(finding(
                        fw.clone(),
                        "names",
                        format!(
                            "{tn}.{a}'s format has {{{ph}}}, wider than the {} digits a number is padded to at most",
                            crate::model::IDENTIFIER_WIDTH
                        ),
                    ));
                }
                continue;
            }
            let hops: Vec<&str> = ph.split('.').collect();
            if !hops.iter().all(|h| NAME.is_match(h)) {
                out.push(finding(
                    fw.clone(),
                    "names",
                    format!("{tn}.{a}'s format has {{{ph}}}, which is not {{n}}, {{n:<width>}}, {{<attribute>}} or {{<reference>.<attribute>}}"),
                ));
                continue;
            }
            if hops.len() > 2 {
                out.push(finding(fw.clone(), "names", format!("{tn}.{a}'s format reads {ph}, more than one reference away; a placeholder reaches one hop")));
                continue;
            }
            let Some(first) = attrs.get(hops[0]).filter(|f| !f.is_null()) else {
                out.push(finding(
                    fw.clone(),
                    "names",
                    format!(
                        "{tn}.{a}'s format reads '{}', which {tn} does not declare",
                        hops[0]
                    ),
                ));
                continue;
            };
            if hops.len() == 2 && (!has(first, "reference") || ref_of(first).ends_with("[]")) {
                out.push(finding(
                    fw.clone(),
                    "names",
                    format!(
                        "{tn}.{a}'s format reads through '{}', which is not a single reference",
                        hops[0]
                    ),
                ));
                continue;
            }
            if creates && !written.contains(hops[0]) {
                out.push(finding(
                    fw.clone(),
                    "names",
                    format!("{tn}.{a}'s format reads '{}', which not every creation of {tn} writes before the mint", hops[0]),
                ));
                continue;
            }
            let mut target = first.clone();
            if hops.len() == 2 {
                let Some(other) = library.get(&ref_of(first)) else {
                    continue;
                };
                let Some(tg) = obj(other, "attributes")
                    .get(hops[1])
                    .filter(|t| !t.is_null())
                else {
                    out.push(finding(
                        fw.clone(),
                        "names",
                        format!(
                            "{tn}.{a}'s format reads {ph}, and {} has no attribute '{}'",
                            ref_of(first),
                            hops[1]
                        ),
                    ));
                    continue;
                };
                target = tg.clone();
            }
            if !is_text(&target) {
                out.push(finding(
                    fw.clone(),
                    "names",
                    format!(
                        "{tn}.{a}'s format reads {ph}, which is not a string or an enumeration"
                    ),
                ));
            } else if optional(&target) && depth == 0 {
                out.push(finding(
                    fw.clone(),
                    "names",
                    format!("{tn}.{a}'s format reads {ph}, which is optional, outside [ … ]; a segment that may be absent goes in brackets"),
                ));
            }
            // an identifier is kept past an erasure, so it holds no personal
            // value (the audit's personal data S28)
            if flag(&target, "personal") {
                out.push(finding(
                    fw.clone(),
                    "names",
                    format!("{tn}.{a}'s format reads {ph}, which is personal; an identifier is kept past an erasure"),
                ));
            }
        }
        if depth > 0 {
            out.push(finding(
                fw.clone(),
                "names",
                format!("{tn}.{a}'s format leaves a [ open"),
            ));
        }
        if !numbered {
            out.push(finding(fw, "names", format!("{tn}.{a}'s format has no {{n}} or {{n:<width>}}, so it would mint one value only")));
        }
    }
    out
}

/// `str(spec.get("reference", spec.get("type", "")))`.
fn kind_text_any(spec: &Value) -> String {
    match spec.get("reference") {
        Some(r) => py_str(r),
        None => str_or(spec, "type", ""),
    }
}

/// The members a row of the source has, or none if the type has no such
/// source.
fn row_members(t: &Value, source: &str) -> Option<BTreeSet<String>> {
    if INTERVALS_OF.is_match(source) {
        let mut rows = dataset_rows("intervals").unwrap_or_default();
        rows.remove("state");
        rows.insert("value".into());
        return Some(rows);
    }
    if let Some(rows) = dataset_rows(source) {
        return Some(rows);
    }
    if source == "objects" {
        let mut out: BTreeSet<String> = keys(t, "attributes").into_iter().collect();
        out.extend(keys(t, "observations"));
        out.extend(keys(t, "derived_attributes"));
        out.extend(OBJECT_ROW.iter().map(|s| s.to_string()));
        return Some(out);
    }
    let ob = obj(t, "observations")
        .get(source)
        .filter(|o| !o.is_null())?;
    let mut out: BTreeSet<String> = keys(ob, "attributes").into_iter().collect();
    out.extend(KIND_ROW.iter().map(|s| s.to_string()));
    Some(out)
}

/// The strings a row's `.transition` is compared with by `==`, `!=` or `in`.
fn transition_literals(e: &Expr, item: &str) -> BTreeSet<String> {
    let is_row = |n: &Expr| matches!(n, Expr::Path(b, m) if m == "transition" && matches!(&**b, Expr::Name(v) if v == item));
    let mut found = BTreeSet::new();
    fn go(n: &Expr, is_row: &dyn Fn(&Expr) -> bool, found: &mut BTreeSet<String>) {
        if let Expr::Bin(op, l, r) = n
            && matches!(op, BinOp::Eq | BinOp::Ne)
            && (is_row(l) || is_row(r))
        {
            let other = if is_row(l) { r } else { l };
            if let Expr::Str(s) = &**other {
                found.insert(s.clone());
            }
        }
        if let Expr::In(l, r) = n
            && is_row(l)
            && let Expr::Set(items) = &**r
        {
            for m in items {
                if let Expr::Str(s) = m {
                    found.insert(s.clone());
                }
            }
        }
        for c in children(n) {
            go(c, is_row, found);
        }
    }
    go(e, &is_row, &mut found);
    found
}

static PERCENTILE_LITERAL: LazyLock<Regex> =
    LazyLock::new(|| re(r"\bpercentile\(\s*([-+]?[0-9.]+)\s*,"));
static PERCENTILE_ANY: LazyLock<Regex> = LazyLock::new(|| re(r"\bpercentile\(\s*([^,()]+?)\s*,"));
static SPLITTABLE: LazyLock<Regex> = LazyLock::new(|| re(r"^intervals(\([a-z][a-z0-9_]*\))?$"));
static PERCENTILE_NUMBER: LazyLock<Regex> = LazyLock::new(|| re(r"^[-+]?[0-9.]+$"));

/// Whether a percentile's p, as written, lies in (0, 1].
fn percentile_in_range(p: &str) -> bool {
    p.parse::<f64>().is_ok_and(|v| v > 0.0 && v <= 1.0)
}

/// A metric in the metric language reads its rows through its item, and its
/// flags read its value and its dimensions.
fn formula_errors(tn: &str, t: &Value, mn: &str, m: &Value) -> Found {
    let base = segs!["types", tn, "metrics", mn];
    let source = m.get("source").map(py_str).unwrap_or_default();
    let item = m.get("item").map(py_str).unwrap_or_default();
    let mut members = row_members(t, &source);
    let mut out = Found::new();
    if let Some(split) = m.get("split_by").filter(|s| super::doc::truthy(Some(s))) {
        if !SPLITTABLE.is_match(&source) {
            out.push(finding(
                ext(&base, segs!["split_by"]),
                "names",
                format!("metric {mn} splits {source} by {}, and only a type's spans, intervals or intervals(<member>), are split", py_str(split)),
            ));
        } else if let Some(mm) = members.as_mut() {
            mm.extend(PIECE_MEMBERS.iter().map(|s| s.to_string()));
        }
        if flag(m, "time_dimension") {
            out.push(finding(
                ext(&base, segs!["time_dimension"]),
                "names",
                format!("metric {mn} splits its spans, so its time is its pieces', and it declares no time_dimension; a window clips the pieces"),
            ));
        }
    }
    if members.is_none() {
        out.push(finding(
            ext(&base, segs!["source"]),
            "names",
            format!("metric {mn} reads '{source}', which is not objects, intervals, intervals(<member>), transitions, attempts, attempt_counts, labels or an observation kind of {tn}"),
        ));
    }
    if let Some(tr) = INTERVALS_OF.captures(&source)
        && !obj(t, "attributes").contains_key(&tr[1])
        && &tr[1] != "state"
    {
        out.push(finding(
            ext(&base, segs!["source"]),
            "names",
            format!(
                "metric {mn} reads the intervals of '{}', which {tn} does not declare",
                &tr[1]
            ),
        ));
    }
    if NOT_AN_ATTRIBUTE.contains(&item.as_str())
        || KEYWORDS.contains(&item.as_str())
        || item == "value"
    {
        out.push(finding(
            ext(&base, segs!["item"]),
            "names",
            format!(
                "metric {mn} names its rows '{item}', which an expression reads as something else"
            ),
        ));
    }
    for d in keys(m, "dimensions") {
        if d == "value" {
            out.push(finding(
                ext(&base, segs!["dimensions", &d]),
                "names",
                format!("metric {mn} has a dimension named 'value', which its flags read as the metric's value"),
            ));
        }
    }
    let mut parts: Vec<(Path, Option<String>)> = vec![
        (
            segs!["filter"],
            m.get("filter").filter(|v| !v.is_null()).map(py_str),
        ),
        (
            segs!["time_dimension"],
            m.get("time_dimension").filter(|v| !v.is_null()).map(py_str),
        ),
        (segs!["expression"], m.get("expression").map(py_str)),
    ];
    for (d, e) in obj(m, "dimensions") {
        parts.push((segs!["dimensions", d], Some(py_str(e))));
    }
    let expression = squash(&m.get("expression").map(py_str).unwrap_or_default());
    for c in PERCENTILE_LITERAL.captures_iter(&expression) {
        if !percentile_in_range(&c[1]) {
            out.push(finding(
                ext(&base, segs!["expression"]),
                "names",
                format!("metric {mn} takes the percentile {}, and a percentile's p is in (0, 1] (ADR-0146)", &c[1]),
            ));
        }
    }
    // count(<expression>) over the rows answers nothing, which the core did
    // not run (D687)
    if COUNT_CALL.find_iter(&expression).any(|at| {
        let rest = expression[at.end()..].trim_start();
        !(rest.starts_with(')') || COUNTED.is_match(rest))
    }) {
        out.push(finding(
            ext(&base, segs!["expression"]),
            "names",
            format!("metric {mn} counts an expression over its rows, which none answers: count() counts the rows, count(where …) those a condition selects, count(distinct …) the values they hold"),
        ));
    }
    // a metric's value combines aggregates over its rows, literals and now
    // by + - * / and a negative, which is all the core computes over them
    // (declaration-syntax.md §6.9); any other form published and was not
    // run (the values audit's M2)
    if let Some(held) = crate::expr::parse(&expression)
        .ok()
        .and_then(|e| metric_value_form(&e))
    {
        out.push(finding(
            ext(&base, segs!["expression"]),
            "names",
            format!("metric {mn}'s value holds {held}, which a metric does not compute: it combines aggregates over its rows, literals and now by + - * / and a negative"),
        ));
    }
    for (key, text) in &parts {
        let Some(text) = text else { continue };
        // .held(<member>) names a member of the type whose spans or
        // transitions the rows are, not a name of the metric's own (§6.9, D550)
        let held: BTreeSet<String> = held_of(&squash(text), &item).into_iter().collect();
        let mut r = reads(text);
        r.remove(&item);
        r.remove("now");
        for r in r.difference(&held) {
            out.push(finding(
                ext(&base, key.clone()),
                "names",
                format!("metric {mn} reads '{r}'; a metric reads its rows through '{item}'"),
            ));
        }
        let declared: BTreeSet<String> = keys(t, "attributes").into_iter().collect();
        for h in &held {
            if h != "state" && !declared.contains(h) {
                out.push(finding(
                    ext(&base, key.clone()),
                    "names",
                    format!("metric {mn} reads {item}.held({h}), and {tn} has no member '{h}'"),
                ));
            }
        }
        let read: BTreeSet<String> = members_of(&squash(text), &item).into_iter().collect();
        for r in read {
            if let Some(mm) = &members
                && !mm.contains(&r)
            {
                let message = if source == "labels" && r == "note" {
                    format!(
                        "metric {mn} reads {item}.{r}, a label's note, which is personal and not readable"
                    )
                } else {
                    format!(
                        "metric {mn} reads {item}.{r}, and a row of {source} has no member '{r}'"
                    )
                };
                out.push(finding(ext(&base, key.clone()), "names", message));
            }
        }
    }
    if TRANSITION_ROWS.contains(&source.as_str()) {
        let names: BTreeSet<String> = keys(t, "transitions").into_iter().collect();
        for (key, text) in &parts {
            let Some(text) = text else { continue };
            let Ok(tree) = parse(&squash(text)) else {
                continue;
            };
            for lit in transition_literals(&tree, &item).difference(&names) {
                out.push(finding(
                    ext(&base, key.clone()),
                    "names",
                    format!("metric {mn} compares {item}.transition with \"{lit}\", which is not a transition of {tn}"),
                ));
            }
        }
    }
    let dims: BTreeSet<String> = keys(m, "dimensions").into_iter().collect();
    for (f, c) in obj(m, "flag_when") {
        let mut r = reads(&py_str(c));
        r.retain(|n| !dims.contains(n) && n != "value" && n != "now");
        for r in r {
            out.push(finding(
                ext(&base, segs!["flag_when", f]),
                "names",
                format!("metric {mn}'s flag {f} reads '{r}', which is neither value nor a dimension the metric declares"),
            ));
        }
    }
    for c in PERCENTILE_ANY.captures_iter(&expression) {
        if !PERCENTILE_NUMBER.is_match(&c[1]) {
            out.push(finding(
                ext(&base, segs!["expression"]),
                "names",
                format!("metric {mn} takes the percentile {}, and a percentile's p is a literal (ADR-0147)", &c[1]),
            ));
        }
    }
    out
}

/// The invariants of a type: declared, generated for a state's required
/// attributes, and, for a machine, those it requires of its binders.
pub fn invariant_names(doc: &Value, tn: &str, t: &Value) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = keys(t, "invariants").into_iter().collect();
    for (s, v) in obj(t, "states") {
        if flag(v, "required_attributes") {
            names.insert(format!("{}_invariant", s.to_lowercase()));
        }
    }
    // an actor's identity's, which a migration may admit for actors an
    // earlier release took (ADR-0195 decision 2)
    if obj(t, "attributes")
        .values()
        .any(|a| str_or(a, "type", "") == "identity" && a.get("actor_kind").is_some())
    {
        names.insert("identity_taken".to_string());
    }
    if let Some(machine) = obj(doc, "machines").get(tn) {
        names.extend(strings(
            machine.get("requires").unwrap_or(&Value::Null),
            "invariants",
        ));
    }
    names
}

/// The types reachable from a type by relationships whose ends name each
/// other.
fn related(tn: &str, library: &Obj) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut todo = vec![tn.to_string()];
    while let Some(next) = todo.pop() {
        for spec in library
            .get(&next)
            .map(|t| obj(t, "attributes").values().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
        {
            let other = target_of(&spec);
            if flag(&spec, "opposite") && !other.is_empty() && !seen.contains(&other) && other != tn
            {
                seen.insert(other.clone());
                todo.push(other);
            }
        }
    }
    seen
}

/// (part attribute, transition) for each call an effect makes on a part.
fn erasure_calls(x: &Value, attrs: &Obj) -> BTreeSet<(String, String)> {
    let mut items: BTreeMap<String, String> = BTreeMap::new();
    let mut out = BTreeSet::new();
    for step in walk(x.get("effect")) {
        let body = step.body();
        if step.kind == "foreach"
            && let Some(array) = body.get("array")
        {
            items.insert(body_text(body, "item"), squash(&py_str(array)));
        }
        if step.kind == "call" {
            let target = squash(&body_text(body, "target"));
            let target = items
                .get(&target)
                .cloned()
                .unwrap_or_else(|| target.strip_prefix("this.").unwrap_or(&target).to_string());
            if attrs.contains_key(&target) {
                out.insert((target, body_text(body, "transition")));
            }
        }
    }
    out
}

/// Assertions, erasures and corrections (the model's checks 9, 27, 28, 29
/// and 39).
fn override_errors(doc: &Value, tn: &str, t: &Value, library: &Obj) -> Found {
    let attrs = obj(t, "attributes");
    let mut enums: BTreeSet<String> = keys(doc, "enumerations").into_iter().collect();
    enums.extend(imported(doc));
    let mut out = Found::new();
    for (a, spec) in attrs {
        if flag(spec, "personal") && !optional(spec) && !kind_text(spec).ends_with("[]") {
            out.push(finding(
                segs!["types", tn, "attributes", a],
                "names",
                format!("{tn}.{a} is personal and required; erasure writes absence, so a personal attribute is optional"),
            ));
        }
    }
    for (o, ob) in obj(t, "observations") {
        for (a, spec) in obj(ob, "attributes") {
            if flag(spec, "personal") && !optional(spec) {
                out.push(finding(
                    segs!["types", tn, "observations", o, "attributes", a],
                    "names",
                    format!("{o}.{a} is personal and required; erasure writes absence, so a personal field is optional"),
                ));
            }
        }
    }
    let own = invariant_names(doc, tn, t);
    let reach = related(tn, library);
    for (xn, x) in obj(t, "transitions") {
        let base = segs!["types", tn, "transitions", xn];
        let k = kind(x);
        let inputs = obj(x, "inputs");
        let reason = inputs.get("reason").filter(|r| !r.is_null());
        let needs = match k {
            "assertion" => Some("an assertion"),
            "erasure" => Some("an erasure"),
            _ if flag(x, "corrects") => Some("a correction"),
            _ => None,
        };
        if let Some(needs) = needs
            && reason.is_none()
        {
            out.push(finding(
                base.clone(),
                "names",
                format!("{tn}.{xn} is {needs}, and declares no input 'reason'"),
            ));
        }
        if k == "assertion" {
            if let Some(r) = reason
                && !r
                    .get("type")
                    .map(py_str)
                    .is_some_and(|t| enums.contains(&t))
            {
                out.push(finding(
                    ext(&base, segs!["inputs", "reason"]),
                    "names",
                    format!("{tn}.{xn}'s reason is not an enumeration; an assertion's reasons are counted, so they are declared values"),
                ));
            }
            for i in ["admits", "to"]
                .into_iter()
                .filter(|i| inputs.contains_key(*i))
            {
                out.push(finding(
                    ext(&base, segs!["inputs", i]),
                    "names",
                    format!("{tn}.{xn} declares the input '{i}', which an assertion has already: the state it asks for, or the invariants it admits"),
                ));
            }
        }
        if flag(x, "may_admit") && k != "assertion" {
            out.push(finding(
                ext(&base, segs!["may_admit"]),
                "names",
                format!("{tn}.{xn} may admit invariants, which only an assertion may"),
            ));
        }
        for inv in strings(x, "may_admit") {
            // who may take a transition is never admitted (D579)
            let (on, name) = inv.rsplit_once('.').unwrap_or(("", inv.as_str()));
            let declaring = if on.is_empty() {
                Some(t)
            } else {
                types(doc)
                    .into_iter()
                    .find(|(n, _)| n == on)
                    .map(|(_, d)| d)
                    .or_else(|| library.get(on))
            };
            if declaring
                .and_then(|d| obj(d, "invariants").get(name))
                .is_some_and(|spec| spec.get(TAKEN).is_some())
            {
                out.push(finding(
                    ext(&base, segs!["may_admit"]),
                    "names",
                    format!("{tn}.{xn} may admit {inv}, which taken_by generates: who may take a transition is never admitted (D579)"),
                ));
                continue;
            }
            if let Some((on, name)) = inv.split_once('.') {
                if !reach.contains(on) {
                    out.push(finding(
                        ext(&base, segs!["may_admit"]),
                        "names",
                        format!("{tn}.{xn} may admit {inv}, and {on} is not reachable from {tn} by a relationship whose ends name each other"),
                    ));
                } else if !invariant_names(doc, on, library.get(on).unwrap_or(&Value::Null))
                    .contains(name)
                {
                    out.push(finding(
                        ext(&base, segs!["may_admit"]),
                        "names",
                        format!("{tn}.{xn} may admit {inv}, which is not an invariant of {on}"),
                    ));
                }
            } else if !own.contains(&inv) {
                out.push(finding(
                    ext(&base, segs!["may_admit"]),
                    "names",
                    format!("{tn}.{xn} may admit '{inv}', which is not an invariant of {tn}"),
                ));
            }
        }
        if flag(x, "corrects") {
            if !matches!(k, "internal" | "external") {
                out.push(finding(
                    ext(&base, segs!["corrects"]),
                    "names",
                    format!("{tn}.{xn} corrects attributes, which only an internal or external transition may"),
                ));
            }
            let mut written: BTreeSet<String> = strings(x, "required_inputs").into_iter().collect();
            written.extend(strings(x, "optional_inputs"));
            for s in walk(x.get("effect")) {
                if matches!(s.kind, "assign" | "add" | "remove") {
                    written.insert(body_text(s.body(), "location"));
                }
                if s.kind == "clear" {
                    written.extend(list(s.st, "clear").iter().map(py_str));
                }
            }
            // the record of who took it, which taken_by generates, is no
            // value it corrects, as for an assertion or an erasure (ADR-0181
            // decision 1, ADR-0190 decision 5)
            if !strings(x, "taken_by").is_empty() {
                written.remove(&crate::model::taken_by_names(xn).0);
            }
            let corrects: BTreeSet<String> = strings(x, "corrects").into_iter().collect();
            for a in corrects.difference(&written) {
                out.push(finding(
                    ext(&base, segs!["corrects"]),
                    "names",
                    format!("{tn}.{xn} corrects '{a}' and does not write it"),
                ));
            }
            for a in written.difference(&corrects) {
                out.push(finding(
                    ext(&base, segs!["corrects"]),
                    "names",
                    format!("{tn}.{xn} writes '{a}', which it does not list in corrects; a correction writes exactly what it corrects"),
                ));
            }
        }
        if k == "erasure" {
            let calls = erasure_calls(x, attrs);
            for (a, spec) in attrs {
                let Some(part) = library.get(&target_of(spec)) else {
                    continue;
                };
                if text(spec, "aggregation") != Some("composite") {
                    continue;
                }
                if !obj(part, "attributes")
                    .values()
                    .any(|s| flag(s, "personal"))
                {
                    continue;
                }
                let erasures: BTreeSet<String> = obj(part, "transitions")
                    .iter()
                    .filter(|(_, px)| kind(px) == "erasure")
                    .map(|(n, _)| n.clone())
                    .collect();
                if !calls.iter().any(|(c, n)| c == a && erasures.contains(n)) {
                    out.push(finding(
                        base.clone(),
                        "names",
                        format!("{tn}.{xn} erases {tn} and does not call an erasure of its parts in '{a}', which hold personal attributes"),
                    ));
                }
            }
        }
    }
    out
}

/// A superseding transition names one successor, at the top level, and only
/// it does; a group's erasures agree; a type with personal data has an
/// erasure (`declaration-syntax.md` §6.1, §6.4; checks 26, 49 and 60).
/// The types a supersede step of any type's transition names as its
/// successor's, by the input's reference or by what a create binds, read
/// once for every type, where each type walked every type's transitions:
/// quadratic in the types (ninth production review N-8).
fn superseded_types(library: &Obj) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for other in library.values() {
        for x in obj(other, "transitions").values() {
            let walked = walk(x.get("effect"));
            for s in &walked {
                if s.kind != "supersede" {
                    continue;
                }
                let who = squash(&py_str(s.body()));
                if let Some(r) = CALL_INPUT
                    .captures(&who)
                    .and_then(|m| obj(x, "inputs").get(&m[1]).cloned())
                    .and_then(|sp| sp.get("reference").map(py_str))
                {
                    out.insert(r);
                }
                out.extend(
                    walked
                        .iter()
                        .filter(|c| {
                            c.kind == "create"
                                && c.body().get("result").map(py_str).as_deref()
                                    == Some(who.as_str())
                        })
                        .map(|c| body_text(c.body(), "type")),
                );
            }
        }
    }
    out
}

fn supersession_errors(
    doc: &Value,
    tn: &str,
    t: &Value,
    library: &Obj,
    superseded: &BTreeSet<String>,
) -> Found {
    let states = obj(t, "states");
    let mut out = Found::new();
    for (xn, x) in obj(t, "transitions") {
        let where_ = segs!["types", tn, "transitions", xn];
        let into = x
            .get("to")
            .and_then(Value::as_str)
            .and_then(|to| states.get(to));
        let steps: Vec<Step<'_>> = walk(x.get("effect"))
            .into_iter()
            .filter(|s| s.kind == "supersede")
            .collect();
        let superseding = into.is_some_and(|s| flag(s, "superseding"));
        if !steps.is_empty() && !superseding {
            out.push(finding(
                ext(&where_, segs!["effect"]),
                "names",
                format!("{tn}.{xn} supersedes, and does not enter a superseding state"),
            ));
        }
        if superseding && steps.is_empty() {
            out.push(finding(
                ext(&where_, segs!["to"]),
                "names",
                format!("{tn}.{xn} enters the superseding state {} and names no successor with supersede", x.get("to").map(py_str).unwrap_or_default()),
            ));
        }
        if steps.len() > 1 {
            out.push(finding(
                ext(&where_, ext(segs!["effect"], steps[1].path.clone())),
                "names",
                format!("{tn}.{xn} names more than one successor, and an object has one"),
            ));
        }
        let mut bound: Vec<String> = Vec::new();
        for s in walk(x.get("effect")) {
            if s.kind == "create"
                && s.depth == 0
                && let Some(r) = s
                    .body()
                    .get("result")
                    .filter(|r| super::doc::truthy(Some(r)))
            {
                bound.push(py_str(r));
            }
            if s.kind != "supersede" {
                continue;
            }
            let at = ext(&where_, ext(segs!["effect"], s.path.clone()));
            if s.depth > 0 {
                out.push(finding(at.clone(), "names", format!("{tn}.{xn} supersedes inside a foreach, which may name no successor or several")));
            }
            let who = squash(&py_str(s.body()));
            if who == "this" {
                out.push(finding(
                    at,
                    "names",
                    format!("{tn}.{xn} supersedes the object with itself"),
                ));
            } else if let Some(m) = CALL_INPUT.captures(&who) {
                let spec = obj(x, "inputs").get(&m[1]).filter(|s| !s.is_null());
                if let Some(spec) = spec {
                    if !has(spec, "reference") {
                        out.push(finding(
                            at,
                            "names",
                            format!(
                                "{tn}.{xn} supersedes with inputs.{}, which is not an object",
                                &m[1]
                            ),
                        ));
                    } else if flag(spec, "optional") {
                        out.push(finding(
                            at,
                            "names",
                            format!("{tn}.{xn} supersedes with inputs.{}, which is optional, so a request leaving it out would name no successor", &m[1]),
                        ));
                    }
                }
            } else if !bound.contains(&who) {
                out.push(finding(
                    at,
                    "names",
                    format!("{tn}.{xn} supersedes with {who}, which is neither an input nor a name a create at the top level of the effect binds (check 49)"),
                ));
            }
        }
    }
    let erasure_needs = |tt: Option<&Value>| -> Option<BTreeMap<String, (String, bool)>> {
        let tt = tt?;
        let ex = obj(tt, "transitions")
            .values()
            .find(|x| kind(x) == "erasure")?;
        Some(
            obj(ex, "inputs")
                .iter()
                .map(|(i, sp)| (i.clone(), (kind_text_ref_or_type(sp), flag(sp, "optional"))))
                .collect(),
        )
    };
    for (xn, x) in obj(t, "transitions") {
        let walked = walk(x.get("effect"));
        for s in &walked {
            if s.kind != "supersede" {
                continue;
            }
            let who = squash(&py_str(s.body()));
            let reference = CALL_INPUT
                .captures(&who)
                .and_then(|m| obj(x, "inputs").get(&m[1]).cloned())
                .map(|sp| sp.get("reference").map(py_str).unwrap_or_default())
                .unwrap_or_default();
            let made: Vec<String> = walked
                .iter()
                .filter(|c| {
                    c.kind == "create"
                        && c.body().get("result").map(py_str).as_deref() == Some(who.as_str())
                })
                .map(|c| body_text(c.body(), "type"))
                .collect();
            let other = {
                let r = reference.trim_end_matches(['[', ']']).to_string();
                if r.is_empty() {
                    made.first().cloned().unwrap_or_default()
                } else {
                    r
                }
            };
            let (mine, theirs) = (erasure_needs(Some(t)), erasure_needs(library.get(&other)));
            if !other.is_empty()
                && other != tn
                && let (Some(mine), Some(theirs)) = (mine, theirs)
                && mine != theirs
            {
                let names = |m: &BTreeMap<String, (String, bool)>| {
                    if m.is_empty() {
                        "no input".to_string()
                    } else {
                        m.keys().cloned().collect::<Vec<_>>().join(", ")
                    }
                };
                out.push(finding(
                    ext(segs!["types", tn, "transitions", xn, "effect"], s.path.clone()),
                    "names",
                    format!(
                        "{tn}.{xn} supersedes with a {other}, whose erasure takes {} where {tn}'s takes {}; a group's erasures receive one request's inputs, so they require the same, by name, type and optionality",
                        names(&theirs),
                        names(&mine)
                    ),
                ));
            }
        }
    }
    let takes_part = states.values().any(|v| flag(v, "superseding")) || superseded.contains(tn);
    let erasures: Vec<String> = obj(t, "transitions")
        .iter()
        .filter(|(_, x)| kind(x) == "erasure")
        .map(|(n, _)| n.clone())
        .collect();
    if erasures.len() > 1 {
        out.push(finding(
            segs!["types", tn, "transitions", &erasures[1]],
            "names",
            format!("{tn} declares the erasures {}; a type declares at most one, which a group's erasure takes (ADR-0147)", erasures.join(", ")),
        ));
    }
    for (xn, x) in obj(t, "transitions") {
        let to: Vec<String> = match x.get("to") {
            Some(Value::Array(a)) => a.iter().map(py_str).collect(),
            Some(v) => vec![py_str(v)],
            None => vec!["None".into()],
        };
        if kind(x) == "assertion"
            && to
                .iter()
                .any(|s| states.get(s).is_some_and(|v| flag(v, "superseding")))
        {
            out.push(finding(
                segs!["types", tn, "transitions", xn, "to"],
                "names",
                format!("{tn}.{xn} asserts a superseding state, which would name no successor; a merge is a declared transition (the model's check 26, ADR-0147)"),
            ));
        }
    }
    let personal = obj(t, "attributes").values().any(|sp| flag(sp, "personal"));
    let mut taken_personal: Vec<String> = obj(t, "transitions")
        .iter()
        .flat_map(|(xn, x)| {
            obj(x, "inputs")
                .iter()
                .filter(|(_, sp)| flag(sp, "personal"))
                .map(move |(i, _)| format!("{xn}.{i}"))
        })
        .collect();
    taken_personal.sort();
    let erasable = obj(t, "transitions").values().any(|x| kind(x) == "erasure");
    if (personal || !taken_personal.is_empty())
        && !erasable
        && !flag(t, "abstract")
        && !obj(doc, "machines").contains_key(tn)
    {
        let why = if takes_part {
            ", and takes part in supersession, through which a superseded chain is erased"
        } else {
            ""
        };
        let what = if personal {
            "personal data".to_string()
        } else {
            format!("the personal input {}", taken_personal[0])
        };
        out.push(finding(
            segs!["types", tn],
            "names",
            format!("{tn} holds {what} and declares no erasure{why} (check 60)"),
        ));
    }
    out
}

/// `str(sp.get("type", sp.get("reference")))`.
fn kind_text_ref_or_type(sp: &Value) -> String {
    match sp.get("type") {
        Some(t) => py_str(t),
        None => sp
            .get("reference")
            .map_or_else(|| "None".to_string(), py_str),
    }
}

/// The evaluators a module declares or imports.
pub fn evaluator_names(doc: &Value) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = keys(doc, "evaluators").into_iter().collect();
    out.extend(
        imported(doc)
            .into_iter()
            .filter(|n| n.chars().next().is_some_and(|c| c.is_lowercase())),
    );
    out
}

/// (evaluator, function, number of arguments) for each call an expression
/// makes to an evaluator.
fn evaluator_calls(text: &str, names: &BTreeSet<String>) -> Vec<(String, String, usize)> {
    let text = squash(text);
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    for (ev, f, open) in dotted_calls(&text) {
        if !names.contains(&ev) {
            continue;
        }
        let (mut depth, mut i, mut commas) = (1i32, open, 0usize);
        while i < chars.len() && depth > 0 {
            match chars[i] {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            if chars[i] == ',' && depth == 1 {
                commas += 1;
            }
            i += 1;
        }
        let body: String = chars[open..i.saturating_sub(1).max(open)].iter().collect();
        out.push((
            ev,
            f,
            if body.trim().is_empty() {
                0
            } else {
                commas + 1
            },
        ));
    }
    out
}

static WHOLE_VERDICT: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(not\s+)?[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*\(.*\)$"));

/// Each argument of a condition's evaluator call whose value does not fit
/// the argument the function declares, in order, where both kinds can be
/// told (the model's check 2, its types); the call is the condition or its
/// negation, check 24 refusing it elsewhere (`check-flows.py`'s
/// `evaluator_argument_errors`).
#[allow(clippy::too_many_arguments)]
fn evaluator_argument_errors(
    where_: &Path,
    cn: &str,
    text: &str,
    ev: &str,
    f: &str,
    fun: &Value,
    tn: &str,
    t: &Value,
    doc: &Value,
) -> Found {
    // step 2 refuses what does not parse
    let Ok(mut tree) = parse(text) else {
        return Found::new();
    };
    if let Expr::Not(inner) = tree {
        tree = *inner;
    }
    let Expr::Call(callee, args) = &tree else {
        return Found::new();
    };
    let named = matches!(&**callee, Expr::Path(base, m)
        if m == f && matches!(&**base, Expr::Name(b) if b == ev));
    if !named {
        return Found::new();
    }
    let enums: BTreeSet<String> = keys(doc, "enumerations").into_iter().collect();
    let no_transition = Value::Object(Default::default());
    let mut out = Found::new();
    for ((a, spec), arg) in obj(fun, "arguments").iter().zip(args) {
        let target = written_kind(Some(spec), &enums);
        let value = value_kind(arg, tn, t, &no_transition, &enums);
        if let (Some(target), Some(value)) = (target, value)
            && !fits(&target, &value, &Obj::new())
        {
            out.push(finding(
                where_.clone(),
                "names",
                format!(
                    "condition {cn} passes {} to {ev}.{f}'s argument '{a}', which takes {}: an argument must fit its input (the model's check 2)",
                    value.join(" "),
                    target.join(" ")
                ),
            ));
        }
    }
    out
}

/// A guard, and only a guard, asks an evaluator, with its arguments, as the
/// whole condition or its negation (checks 19 and 24).
fn evaluator_errors(doc: &Value, tn: &str, t: &Value) -> Found {
    let evaluators = obj(doc, "evaluators");
    let names = evaluator_names(doc);
    let mut out = Found::new();
    for (cn, c) in obj(t, "conditions") {
        let where_ = segs!["types", tn, "conditions", cn, "expression"];
        let text = squash(&c.get("expression").map(py_str).unwrap_or_default());
        let calls = evaluator_calls(&text, &names);
        for (ev, f, n) in &calls {
            if let Some(e) = evaluators.get(ev) {
                match obj(e, "functions").get(f).filter(|x| !x.is_null()) {
                    None => out.push(finding(
                        where_.clone(),
                        "names",
                        format!("condition {cn} calls {ev}.{f}, which {ev} does not declare"),
                    )),
                    Some(fun) => {
                        let want = obj(fun, "arguments").len();
                        if *n != want {
                            out.push(finding(where_.clone(), "names", format!("condition {cn} passes {n} argument(s) to {ev}.{f}, which takes {want}")));
                        } else {
                            out.extend(evaluator_argument_errors(
                                &where_, cn, &text, ev, f, fun, tn, t, doc,
                            ));
                        }
                    }
                }
            }
        }
        if !calls.is_empty() && !WHOLE_VERDICT.is_match(&text) {
            out.push(finding(
                where_.clone(),
                "names",
                format!("condition {cn} calls an evaluator inside a larger expression; a verdict is the whole condition or its negation (check 24)"),
            ));
        }
    }
    let mut others: Vec<(Path, String, String)> = Vec::new();
    for sec in ["invariants", "derived_attributes"] {
        for (n, x) in obj(t, sec) {
            others.push((
                segs!["types", tn, sec, n, "expression"],
                n.clone(),
                x.get("expression").map(py_str).unwrap_or_default(),
            ));
        }
    }
    for (xn, x) in obj(t, "transitions") {
        for s in walk(x.get("effect")) {
            for e in step_expressions(&s) {
                others.push((
                    ext(
                        segs!["types", tn, "transitions", xn, "effect"],
                        s.path.clone(),
                    ),
                    xn.clone(),
                    e,
                ));
            }
        }
    }
    for (p, name, text) in others {
        for (ev, f, _) in evaluator_calls(&text, &names) {
            out.push(finding(
                p.clone(),
                "names",
                format!("{name} calls {ev}.{f}; only a guard asks an evaluator"),
            ));
        }
    }
    out
}

/// The functions an expression may call, and the member functions
/// (`declaration-syntax.md` §8.3).
const FUNCTIONS: [&str; 4] = ["length", "date", "entered_at", "time_in"];
/// The functions a metric alone calls, which check 56 refuses elsewhere.
const METRIC_ONLY_FUNCTIONS: [&str; 8] = [
    "avg",
    "median",
    "percentile",
    "day",
    "week",
    "month",
    "quarter",
    "year",
];
const MEMBER_FUNCTIONS: [&str; 4] = ["intervals", "entered_at", "time_in", "held"];

/// The aggregates over a metric's rows (`metric.rs`'s `ROW_AGGREGATES`).
const ROW_AGGREGATES: [&str; 7] = ["count", "sum", "min", "max", "avg", "median", "percentile"];

/// Whether an expression is an aggregate over a metric's rows: a count of
/// them, or one called on its body (`check-flows.py`'s `row_aggregate`).
fn row_aggregate(e: &Expr) -> bool {
    match e {
        Expr::Agg { var: None, .. } => true,
        Expr::Call(f, _) => matches!(&**f, Expr::Name(n) if ROW_AGGREGATES.contains(&n.as_str())),
        _ => false,
    }
}

/// Whether an expression holds an aggregate over a metric's rows anywhere.
fn holds_row_aggregate(e: &Expr) -> bool {
    row_aggregate(e) || e.children().into_iter().any(holds_row_aggregate)
}

/// The first form a metric's value holds that the core does not compute
/// over its rows, or none (the values audit's M2).
fn metric_value_form(e: &Expr) -> Option<&'static str> {
    if row_aggregate(e) {
        let parts: Vec<&Expr> = match e {
            Expr::Agg {
                collection,
                filter,
                body,
                ..
            } => [collection, filter, body]
                .into_iter()
                .flatten()
                .map(AsRef::as_ref)
                .collect(),
            Expr::Call(_, args) => args.iter().collect(),
            _ => Vec::new(),
        };
        return parts
            .into_iter()
            .any(holds_row_aggregate)
            .then_some("an aggregate over its rows within another");
    }
    match e {
        Expr::Agg { .. } | Expr::Num(_) | Expr::Dur(..) | Expr::Money(..) | Expr::Metric { .. } => {
            None
        }
        Expr::Name(n) if n == "now" => None,
        Expr::Bin(BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div, l, r) => {
            metric_value_form(l).or_else(|| metric_value_form(r))
        }
        Expr::Bin(BinOp::And | BinOp::Or | BinOp::Implies, ..) => Some("a connective"),
        Expr::Bin(..) => Some("a comparison"),
        Expr::Neg(x) => metric_value_form(x),
        Expr::Not(_) => Some("a negated condition"),
        Expr::If(..) => Some("a choice"),
        Expr::In(..) => Some("a membership"),
        Expr::IsNull(..) => Some("a presence test"),
        Expr::Changed(..) => Some("a change test"),
        Expr::Name(_) | Expr::Path(..) => Some("a row's member outside an aggregate"),
        Expr::Call(..) => Some("a call of what is no aggregate"),
        _ => Some("a literal that is no number"),
    }
}

/// A metric's `count(`, and what it may begin with besides `)`: a condition,
/// distinct values, or a binder over a row's collection (D687).
static COUNT_CALL: LazyLock<Regex> = LazyLock::new(|| re(r"\bcount\("));
static COUNTED: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(where\b|distinct\b|[a-z_][a-z0-9_]*\s+in\b)"));
static AGGREGATE_CALL: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b(count|sum|min|max|avg|median|percentile)\s*\("));

/// The dimensions a metric has.
fn dimensions_of(m: &Value) -> BTreeSet<String> {
    let mut out = if has(m, "source") {
        keys(m, "dimensions").into_iter().collect::<BTreeSet<_>>()
    } else {
        strings(m, "group_by").into_iter().collect()
    };
    out.extend(STANDARD_DIMENSIONS.iter().map(|s| s.to_string()));
    out
}

/// A module's metrics share one namespace, and a combined metric's inputs,
/// groups and value are its inputs' (checks 33 and 56).
fn combined_errors(doc: &Value) -> Found {
    let mut out = Found::new();
    let mut owner: BTreeMap<String, String> = BTreeMap::new();
    for (tn, t) in types(doc) {
        for mn in keys(t, "metrics") {
            if let Some(o) = owner.get(&mn) {
                out.push(finding(
                    segs!["types", &tn, "metrics", &mn],
                    "names",
                    format!("the metric {mn} is declared by both {o} and {tn}; a module's metrics share one namespace"),
                ));
            }
            owner.entry(mn).or_insert(tn.clone());
        }
    }
    let kinds: Obj = types(doc)
        .into_iter()
        .map(|(n, t)| (n, t.clone()))
        .collect();
    let combined = obj(doc, "metrics");
    let names: Vec<String> = combined.keys().cloned().collect();
    for (i, (mn, m)) in combined.iter().enumerate() {
        let base = segs!["metrics", mn];
        if let Some(o) = owner.get(mn) {
            out.push(finding(base.clone(), "names", format!("the metric {mn} is also declared by {o}; a module's metrics share one namespace")));
        }
        let mut dims: BTreeSet<String> = set(STANDARD_DIMENSIONS);
        let mut held: Vec<BTreeSet<String>> = Vec::new();
        for (alias, r) in obj(m, "input_metrics") {
            let r = py_str(r);
            let where_ = ext(&base, segs!["input_metrics", alias]);
            if let Some((tn, name)) = r.split_once('.') {
                let target = kinds
                    .get(tn)
                    .and_then(|t| obj(t, "metrics").get(name))
                    .filter(|x| !x.is_null());
                if !kinds.contains_key(tn) {
                    out.push(finding(
                        where_,
                        "names",
                        format!("metric {mn} combines {r}, and the module declares no type {tn}"),
                    ));
                } else if let Some(target) = target {
                    dims.extend(dimensions_of(target));
                    held.push(dimensions_of(target));
                } else {
                    out.push(finding(
                        where_,
                        "names",
                        format!("metric {mn} combines {r}, which is not a metric of {tn}"),
                    ));
                }
            } else if names[i..].contains(&r) {
                out.push(finding(where_, "order", format!("metric {mn} combines {r}, which is itself or declared after it; a combined metric combines those above it")));
            } else if let Some(c) = combined.get(&r) {
                dims.extend(dimensions_of(c));
                held.push(dimensions_of(c));
            } else {
                out.push(finding(
                    where_,
                    "names",
                    format!("metric {mn} combines {r}, which is neither a combined metric of this module nor written as <Type>.<metric>"),
                ));
            }
        }
        let group_by = strings(m, "group_by");
        for d in &group_by {
            if !dims.contains(d) {
                out.push(finding(
                    ext(&base, segs!["group_by"]),
                    "names",
                    format!("metric {mn} groups by '{d}', which none of the metrics it combines has as a dimension"),
                ));
            }
        }
        let wanted: BTreeSet<String> = group_by.iter().cloned().collect();
        if !held.is_empty() && wanted.is_subset(&dims) && !held.iter().any(|h| wanted.is_subset(h))
        {
            out.push(finding(
                ext(&base, segs!["group_by"]),
                "names",
                format!(
                    "metric {mn} groups by {}, which no one of the metrics it combines has all of; one input has every dimension the combination names (the model's check 56, ADR-0147)",
                    wanted.iter().cloned().collect::<Vec<_>>().join(", ")
                ),
            ));
        }
        let expression = m.get("expression").map(py_str).unwrap_or_default();
        let inputs: BTreeSet<String> = keys(m, "input_metrics").into_iter().collect();
        for r in reads(&expression).difference(&inputs) {
            out.push(finding(
                ext(&base, segs!["expression"]),
                "names",
                format!("metric {mn} reads '{r}', which is not one of the metrics it combines"),
            ));
        }
        if AGGREGATE_CALL.is_match(&expression) {
            out.push(finding(
                ext(&base, segs!["expression"]),
                "names",
                format!("metric {mn} aggregates, and a combined metric has no rows: its value combines the values of the metrics it names"),
            ));
        }
        for (f, c) in obj(m, "flag_when") {
            let mut r = reads(&py_str(c));
            r.retain(|n| !wanted.contains(n) && n != "value" && n != "now");
            for r in r {
                out.push(finding(
                    ext(&base, segs!["flag_when", f]),
                    "names",
                    format!("metric {mn}'s flag {f} reads '{r}', which is neither value nor a dimension the metric names"),
                ));
            }
        }
    }
    out
}

/// A metric's measure names a transition and a state of its type, and its
/// groups are its type's.
fn metric_errors(tn: &str, t: &Value, mn: &str, m: &Value) -> Found {
    if has(m, "source") {
        return formula_errors(tn, t, mn, m);
    }
    let base = segs!["types", tn, "metrics", mn];
    let measure = m.get("measure").map(py_str).unwrap_or_default();
    let mut out = Found::new();
    if measure == "transition_count" {
        let xn = m.get("transition").map(py_str).unwrap_or_default();
        if !obj(t, "transitions").contains_key(&xn) {
            return vec![finding(
                ext(&base, segs!["transition"]),
                "names",
                format!("metric {mn} counts '{xn}', which is not a transition of {tn}"),
            )];
        }
    }
    if measure == "median_time_in_state" {
        let st = m.get("state").map(py_str).unwrap_or_default();
        if !obj(t, "states").contains_key(&st) {
            out.push(finding(
                ext(&base, segs!["state"]),
                "names",
                format!("metric {mn} measures the state {st}, which {tn} does not declare"),
            ));
        }
    }
    for d in strings(m, "group_by") {
        if d == "actor" && measure != "transition_count" {
            out.push(finding(
                ext(&base, segs!["group_by"]),
                "names",
                format!("metric {mn} groups by actor, which only a transition_count has"),
            ));
        } else if !["actor", "month", "week"].contains(&d.as_str())
            && !obj(t, "attributes").contains_key(&d)
        {
            out.push(finding(
                ext(&base, segs!["group_by"]),
                "names",
                format!("metric {mn} groups by '{d}', which is not month, week, actor or an attribute of {tn}"),
            ));
        }
    }
    out
}

/// An optional input is read in a step only as the whole of an expression
/// (`DESIGN.md` §5.4; the model's check 48).
fn conditional_step_errors(tn: &str, t: &Value) -> Found {
    let mut out = Found::new();
    for (xn, x) in obj(t, "transitions") {
        let mut optional_inputs: BTreeSet<String> = obj(x, "inputs")
            .iter()
            .filter(|(_, s)| flag(s, "optional") && !has(s, "default"))
            .map(|(i, _)| i.clone())
            .collect();
        optional_inputs.extend(strings(x, "optional_inputs"));
        for s in walk(x.get("effect")) {
            for e in step_expressions(&s) {
                let text = squash(&e);
                for n in &optional_inputs {
                    if text != format!("inputs.{n}")
                        && re(&format!(r"\binputs\.{}\b", regex::escape(n))).is_match(&text)
                    {
                        out.push(finding(
                            ext(segs!["types", tn, "transitions", xn, "effect"], s.path.clone()),
                            "names",
                            format!(
                                "{tn}.{xn} reads the optional input '{n}' inside a larger expression of a step, which makes the step conditional; declare one transition per case, each taking the input it needs (DESIGN.md §5.4; the model's check 48)"
                            ),
                        ));
                    }
                }
            }
        }
    }
    out
}

//! The notices step 3 gives beside its findings: guards that refuse
//! nothing, guards that read a value before the transition writes it, and
//! guards and filters that can be unknown (`check-flows.py`'s `notices`,
//! `befores` and `unknowns`).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::doc::{
    Found, Obj, Path, ext, finding, flag, keys, list, obj, py_str, segs, str_or, strings, text,
};
use super::names::children;
use super::optional::{Env, OptionalReads};
use super::steps::{kind, walk};
use super::view::{inherited, types};
use crate::expr::{Expr, parse};

/// An invariant that can only refuse other objects' changes, as
/// `check-flows.py`'s `vetoes`: one a type declares whose every state is
/// final, so no transition of its own can break it once it is created,
/// reading through a reference or its referrers, so it is checked again
/// whenever what it reads changes, and refuses that change (`flow-format.md`
/// §4.6, ADR-0176 decision 5).
/// The kinds of actor a `taken_by` names, its texts in the order given.
pub fn kinds_named(given: Option<&Value>) -> Vec<String> {
    given
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The kinds of actor that may take a transition, as the view holds it: its
/// own `taken_by`, or its type's where it names none (ADR-0182 decision 5),
/// sorted; none where neither names one.
pub fn taken_kinds(x: &Value, t: &Value) -> Vec<String> {
    let mut kinds = kinds_named(x.get("taken_by"));
    if kinds.is_empty() {
        kinds = kinds_named(t.get("taken_by"));
    }
    kinds.sort();
    kinds
}

/// The identity attributes of a module's concrete types that declare an
/// agent kind of actor, as `Type.attribute`.
pub fn agent_kinds(doc: &Value, v: &Value) -> Vec<String> {
    let machines = keys(doc, "machines");
    let mut out = Vec::new();
    for (tn, t) in types(v) {
        if machines.contains(&tn) || flag(t, "abstract") {
            continue;
        }
        for (an, a) in obj(t, "attributes") {
            if a.get("actor_kind").and_then(Value::as_str) == Some("agent") {
                out.push(format!("{tn}.{an}"));
            }
        }
    }
    out
}

/// Each concrete type's count of the transitions a request may name that
/// name no `taken_by`, its type's included: an agent may take each on the
/// wire wherever the version declares an agent kind of actor (ADR-0189
/// decision 1).
pub fn agent_open(doc: &Value, v: &Value) -> BTreeMap<String, usize> {
    let machines = keys(doc, "machines");
    let mut out = BTreeMap::new();
    for (tn, t) in types(v) {
        if machines.contains(&tn) || flag(t, "abstract") {
            continue;
        }
        let n = obj(t, "transitions")
            .values()
            .filter(|x| {
                x.is_object()
                    && !super::doc::truthy(x.get("only_via"))
                    && taken_kinds(x, t).is_empty()
            })
            .count();
        if n > 0 {
            out.insert(tn, n);
        }
    }
    out
}

/// The most of a type's set its invariants hold it to, `count(x in a) <= n`
/// or `count(x in a) < n + 1`, the least where several do; none where none
/// does.
fn reach_bound(t: &Value, a: &str) -> Option<i64> {
    let form = super::doc::re(&format!(
        r"^count\(\w+ in (?:this\.)?{}\) (<=|<) (\d+)$",
        regex::escape(a)
    ));
    let mut most: Option<i64> = None;
    for spec in obj(t, "invariants").values() {
        let text = spec
            .get("expression")
            .map(py_str)
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if let Some(c) = form.captures(&text)
            && let Ok(n) = c[2].parse::<i64>()
        {
            let n = if &c[1] == "<" { n - 1 } else { n };
            most = Some(most.map_or(n, |m| m.min(n)));
        }
    }
    most
}

/// An erasure's reach a limit may stop: a foreach of an erasure, or a
/// cascade an erasure sets off, declaring a limit over a set of its type that
/// no invariant of the type holds at or below it, so past the limit the
/// erasure is refused `over_limit` and cannot be carried out; a limit, which
/// every loop declares, on any other transition is the author's rule (fifth
/// production review ERASE-LIMIT, ADR-0190 decision 7).
/// A step's target, an attribute of its object, compiled once, where each
/// type compiled its own (ninth production review N-8).
static WORD: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| super::doc::re(r"^(?:this\.)?(\w+)$"));

pub fn reaches(doc: &Value) -> Found {
    let mut out = Found::new();
    let add = |out: &mut Found, found: (Path, String, String)| {
        if !out.contains(&found) {
            out.push(found);
        }
    };
    let limit_of = |v: &Value| -> Option<i64> {
        v.get("limit")
            .filter(|l| !l.is_boolean())
            .and_then(Value::as_i64)
    };
    for (tn, t) in types(doc) {
        if flag(t, "abstract") {
            continue;
        }
        let attrs = obj(t, "attributes");
        let transitions = obj(t, "transitions");
        let erasures: Vec<&String> = transitions
            .iter()
            .filter(|(_, x)| x.is_object() && text(x, "kind") == Some("erasure"))
            .map(|(xn, _)| xn)
            .collect();
        for (a, spec) in attrs {
            for c in list(spec, "cascade") {
                let (erasing, others): (Vec<String>, Vec<String>) = list(c, "on")
                    .iter()
                    .map(py_str)
                    .partition(|e| erasures.contains(&e));
                let Some(n) = limit_of(c) else {
                    continue;
                };
                if reach_bound(t, a).is_some_and(|most| most <= n) {
                    continue;
                }
                if !erasing.is_empty() {
                    add(
                        &mut out,
                        finding(
                            segs!["types", tn.as_str(), "attributes", a.as_str(), "cascade"],
                            "unbounded_reach",
                            format!(
                                "{tn}.{a}'s cascade on {} takes at most {n} of it, and no invariant of {tn} holds it to {n}: past {n}, the erasure is refused over_limit and cannot be carried out; declare one such as count(x in {a}) <= {n} (ADR-0190)",
                                erasing.join(", ")
                            ),
                        ),
                    );
                }
                // and past it any other transition it cascades on, which an
                // object holding more can never take, a claim of 201 lines
                // never withdrawn or paid (fourteenth production review N8)
                if !others.is_empty() {
                    add(
                        &mut out,
                        finding(
                            segs!["types", tn.as_str(), "attributes", a.as_str(), "cascade"],
                            "unbounded_reach",
                            format!(
                                "{tn}.{a}'s cascade on {} takes at most {n} of it, and no invariant of {tn} holds it to {n}: past {n}, each is refused over_limit, and an object holding more can never take it; declare one such as count(x in {a}) <= {n} (ADR-0201)",
                                others.join(", ")
                            ),
                        ),
                    );
                }
            }
        }
        let word = &*WORD;
        for (xn, x) in transitions {
            if !x.is_object() {
                continue;
            }
            let erasure = text(x, "kind") == Some("erasure");
            for s in super::steps::walk(x.get("effect")) {
                if s.kind != "foreach" {
                    continue;
                }
                let f = s.body();
                // a loop with `where` bounds what it keeps, an author's rule
                // on another transition, where one picking a robot's open
                // line was noticed as if it bounded every line (ADR-0201
                // decision 9)
                if !erasure && f.get("where").is_some_and(|w| !w.is_null()) {
                    continue;
                }
                let array = f.get("array").map(py_str).unwrap_or_default();
                let Some(m) = word.captures(array.trim()) else {
                    continue;
                };
                let a = m[1].to_string();
                let Some(n) = limit_of(f) else {
                    continue;
                };
                if attrs.contains_key(&a) && reach_bound(t, &a).is_none_or(|most| most > n) {
                    add(
                        &mut out,
                        finding(
                            segs!["types", tn.as_str(), "transitions", xn.as_str()],
                            "unbounded_reach",
                            format!(
                                "{tn}.{xn} takes at most {n} of {a}, and no invariant of {tn} holds it to {n}: past {n}, {}; declare one such as count(x in {a}) <= {n} ({})",
                                if erasure {
                                    "the erasure is refused over_limit and cannot be carried out"
                                } else {
                                    "it is refused over_limit, and an object holding more can never take it"
                                },
                                if erasure { "ADR-0190" } else { "ADR-0201" }
                            ),
                        ),
                    );
                }
            }
        }
    }
    out
}

pub fn vetoes(doc: &Value, library: &Obj) -> Found {
    let mut kinds = library.clone();
    for (tn, t) in types(doc) {
        kinds.insert(tn, t.clone());
    }
    let mut out = Found::new();
    for (tn, t) in types(doc) {
        let states = obj(t, "states");
        if states.is_empty() || !states.values().all(|s| flag(s, "final")) {
            continue;
        }
        for (i, spec) in obj(t, "invariants") {
            if super::view::inherited(t, "invariants", i) {
                continue;
            }
            let text = spec.get("expression").map(py_str).unwrap_or_default();
            if super::names::invariant_form(&kinds, &tn, &text, &BTreeSet::new()) != "traversal" {
                continue;
            }
            let Ok(tree) = parse(&text) else {
                continue;
            };
            let mut walker = OptionalReads::new(&kinds, &tn, None);
            walker.through_tests = true;
            walker.walk(&tree, &Env::new());
            let through: Vec<&str> = walker
                .traversed
                .iter()
                .filter(|(u, _)| *u == tn)
                .map(|(_, n)| n.as_str())
                .collect();
            let what = if !through.is_empty() {
                through.join(", ")
            } else if walker.members.iter().any(|(_, m)| m == "referrers") {
                "its referrers".to_string()
            } else {
                "the derived attributes it reads".to_string()
            };
            out.push((
                segs!["types", &tn, "invariants", i],
                "veto".into(),
                format!(
                    "{tn}.{i} reads through {what}, and every state of {tn} is final, so no transition of its own can break it: it can only refuse a change to what it reads there. To hold a fact as it stood when the {tn} was made, copy it onto the {tn} then (write-a-flow.md, Patterns, ADR-0176)"
                ),
            ));
        }
    }
    out
}

/// An optional attribute of an owned type that nothing writes, so it is
/// always absent: no transition of the type, a subtype or its machine takes
/// it as an input or sets it in an effect, and no migration backfills it;
/// one with a default, a counter, an identifier, an external identifier or
/// a relationship's end, which its far end writes, is written (D557,
/// `check-flows.py`'s `unwritten`).
pub fn unwritten(view: &Value, doc: &Value) -> Found {
    let machines: BTreeSet<String> = keys(view, "machines").into_iter().collect();
    let tps: Vec<(String, &Value)> = types(view);
    let migration = doc.get("migration").cloned().unwrap_or(Value::Null);
    let mut backfilled = BTreeSet::new();
    for (tn, given) in obj(&migration, "backfill") {
        if let Some(m) = given.as_object() {
            for a in m.keys() {
                backfilled.insert((tn.clone(), a.clone()));
            }
        }
    }
    let family = |tn: &str| -> BTreeSet<String> {
        let mut out: BTreeSet<String> = BTreeSet::from([tn.to_string()]);
        loop {
            let more: Vec<String> = tps
                .iter()
                .filter(|(n, t)| !out.contains(n) && out.contains(&str_or(t, "extends", "")))
                .map(|(n, _)| n.clone())
                .collect();
            if more.is_empty() {
                return out;
            }
            out.extend(more);
        }
    };
    let mut out = Found::new();
    for (tn, t) in &tps {
        if machines.contains(tn) || flag(t, "mirror") {
            continue;
        }
        let mut written: BTreeSet<String> = BTreeSet::new();
        for f in family(tn) {
            let Some((_, ft)) = tps.iter().find(|(n, _)| *n == f) else {
                continue;
            };
            for x in obj(ft, "transitions").values() {
                written.extend(strings(x, "required_inputs"));
                written.extend(strings(x, "optional_inputs"));
                for step in walk(x.get("effect")) {
                    if matches!(step.kind, "assign" | "add" | "remove") {
                        written.insert(str_or(step.body(), "location", ""));
                    }
                }
            }
        }
        for (a, spec) in obj(t, "attributes") {
            if inherited(t, "attributes", a)
                || !flag(spec, "optional")
                || written.contains(a)
                || backfilled.contains(&(tn.clone(), a.clone()))
            {
                continue;
            }
            if ["default", "identifier", "external", "opposite"]
                .iter()
                .any(|k| spec.get(*k).is_some())
                || str_or(spec, "type", "") == "counter"
            {
                continue;
            }
            out.push(finding(
                segs!["types", tn, "attributes", a],
                "unwritten",
                format!(
                    "{tn}.{a} is optional and nothing writes it: no transition takes it as an input or sets it, and no migration backfills it, so it is always absent"
                ),
            ));
        }
    }
    out
}

/// A guard marked `audit` or `warn`, a `self_serviceable` guard on a
/// transition that takes no input, and a correction's free-text reason on
/// a type an erasure reaches.
pub fn notices(doc: &Value) -> Found {
    let mut out = Found::new();
    for (tn, t) in types(doc) {
        let conds = obj(t, "conditions");
        let transitions = obj(t, "transitions");
        let erasable = transitions
            .values()
            .any(|x| str_or(x, "kind", "") == "erasure");
        for (xn, x) in transitions {
            let takes =
                flag(x, "required_inputs") || flag(x, "optional_inputs") || flag(x, "inputs");
            for (g, mode) in obj(x, "guards") {
                let at = segs!["types", &tn, "transitions", xn, "guards", g];
                match py_str(mode).as_str() {
                    "audit" => out.push(finding(
                        at.clone(),
                        "audit",
                        format!("{g} is audited: its failures are recorded and it refuses nothing"),
                    )),
                    "warn" => out.push(finding(
                        at.clone(),
                        "warn",
                        format!(
                            "{g} warns: a failure is reported with the result and refuses nothing"
                        ),
                    )),
                    _ => {}
                }
                let remedy = conds
                    .get(g)
                    .map(|c| str_or(c, "remedy", ""))
                    .unwrap_or_default();
                if !takes && remedy == "self_serviceable" {
                    out.push(finding(
                        at,
                        "self_serviceable",
                        format!("{g}'s remedy is self_serviceable, 'supply it', and {xn} takes no input a caller could supply"),
                    ));
                }
            }
            // a correction's reason is kept with its event, which an erasure
            // of the object does not reach unless the reason is personal:
            // free text there keeps what a clerk typed (production review F7)
            let reason = obj(x, "inputs").get("reason");
            if erasable
                && flag(x, "corrects")
                && reason.is_some_and(|r| str_or(r, "type", "") == "string" && !flag(r, "personal"))
            {
                out.push(finding(
                    segs!["types", &tn, "transitions", xn, "inputs", "reason"],
                    "reason_kept",
                    format!("{tn}.{xn}'s reason is free text, which its event keeps through an erasure of the {tn}: mark it personal, which keeps it off the record, or make it an enumeration, as an override's is"),
                ));
            }
        }
    }
    out
}

/// The attributes an expression reads of its own object, bare or through
/// `this`, and the inputs it reads.
fn supplied_reads(tree: &Expr) -> (BTreeSet<String>, BTreeSet<String>) {
    fn go(
        e: &Expr,
        bound: &BTreeSet<String>,
        attrs: &mut BTreeSet<String>,
        inputs: &mut BTreeSet<String>,
    ) {
        match e {
            Expr::Name(n) => {
                if !bound.contains(n) {
                    attrs.insert(n.clone());
                }
            }
            Expr::Path(b, m) if matches!(&**b, Expr::Name(n) if n == "inputs") => {
                inputs.insert(m.clone());
            }
            Expr::Path(b, m) if matches!(&**b, Expr::Name(n) if n == "this") => {
                attrs.insert(m.clone());
            }
            Expr::Agg {
                var,
                collection,
                filter,
                body,
                ..
            } => {
                if let Some(c) = collection {
                    go(c, bound, attrs, inputs);
                }
                let mut inner = bound.clone();
                if let Some(v) = var {
                    inner.insert(v.clone());
                }
                for part in [filter, body].into_iter().flatten() {
                    go(part, &inner, attrs, inputs);
                }
            }
            other => {
                for c in children(other) {
                    go(c, bound, attrs, inputs);
                }
            }
        }
    }
    let (mut attrs, mut inputs) = (BTreeSet::new(), BTreeSet::new());
    go(tree, &BTreeSet::new(), &mut attrs, &mut inputs);
    (attrs, inputs)
}

/// Each guard that reads an attribute its own transition takes as an input
/// without also reading that input: a guard runs before the transition's
/// writes (`DESIGN.md` §6).
pub fn befores(v: &Value) -> Found {
    let mut out = Found::new();
    for (tn, t) in types(v) {
        let conds = obj(t, "conditions");
        for (xn, x) in obj(t, "transitions") {
            let supplied: BTreeSet<String> = strings(x, "required_inputs")
                .into_iter()
                .chain(strings(x, "optional_inputs"))
                .collect();
            for g in keys(x, "guards") {
                let Some(expr) = conds.get(&g).and_then(|c| text(c, "expression")) else {
                    continue;
                };
                let Ok(tree) = parse(expr) else { continue };
                let (attrs, inputs) = supplied_reads(&tree);
                for a in attrs
                    .intersection(&supplied)
                    .filter(|a| !inputs.contains(*a))
                {
                    let had = if kind(x) == "initial" {
                        "the object it creates has none yet".to_string()
                    } else {
                        format!("it reads the value {a} had before the request")
                    };
                    out.push(finding(
                        segs!["types", &tn, "transitions", xn, "guards", &g],
                        "before",
                        format!("{g} reads {a}, which {xn} takes as an input: a guard runs before the transition writes it, so {had}; inputs.{a} is the value supplied"),
                    ));
                }
            }
        }
    }
    out
}

fn said(why: &BTreeMap<String, String>) -> String {
    why.iter()
        .map(|(k, w)| format!("{k} ({w})"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Each guard, and each filter of a loop or a metric, that can be unknown
/// through an optional it never tests (`declaration-syntax.md` §8.2,
/// ADR-0103 §6).
pub fn unknowns(v: &Value, library: &Obj) -> Found {
    let mut out = Found::new();
    for (tn, t) in types(v) {
        let conds = obj(t, "conditions");
        for (xn, x) in obj(t, "transitions") {
            for (g, mode) in obj(x, "guards") {
                let Some(expr) = conds.get(g).and_then(|c| text(c, "expression")) else {
                    continue;
                };
                let Ok(tree) = parse(expr) else { continue };
                let why = OptionalReads::new(library, &tn, Some(x)).untested(&tree);
                if why.is_empty() {
                    continue;
                }
                let does = match py_str(mode).as_str() {
                    "deny" => "a deny guard that is unknown refuses",
                    "audit" => {
                        "an audit guard that is unknown is recorded as failing, and refuses nothing"
                    }
                    "warn" => "a warn guard that is unknown is raised, and refuses nothing",
                    _ => "",
                };
                out.push(finding(
                    segs!["types", &tn, "transitions", xn, "guards", g],
                    "unknown",
                    format!("{g} can be unknown when {xn} is requested, through {}, which it never tests with `is null`; {does}", said(&why)),
                ));
            }
            loops(
                library,
                &tn,
                xn,
                x,
                list(x, "effect"),
                segs!["types", &tn, "transitions", xn, "effect"],
                &Env::new(),
                &mut out,
            );
        }
        for (mn, m) in obj(t, "metrics") {
            let Some(filter) = m
                .get("filter")
                .filter(|f| super::doc::truthy(Some(f)))
                .map(py_str)
            else {
                continue;
            };
            let src = str_or(m, "source", "None");
            let item = if src == "objects" {
                Some(tn.clone())
            } else {
                obj(t, "observations")
                    .get(&src)
                    .and_then(|o| o.get("kind"))
                    .filter(|k| !k.is_null())
                    .map(py_str)
            };
            let Some(item) = item else { continue };
            let Ok(tree) = parse(&filter) else { continue };
            let mut r = OptionalReads::new(library, &tn, None);
            let mut env = Env::new();
            env.insert(str_or(m, "item", ""), Some(item));
            r.walk(&tree, &env);
            let why = r.left();
            if !why.is_empty() {
                out.push(finding(
                    segs!["types", &tn, "metrics", mn, "filter"],
                    "unknown",
                    format!("{mn}'s filter can be unknown through {}, which it never tests; a row it cannot decide is left out", said(&why)),
                ));
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn loops(
    library: &Obj,
    tn: &str,
    xn: &str,
    x: &Value,
    steps: &[Value],
    at: Path,
    env: &Env,
    out: &mut Found,
) {
    for (i, st) in steps.iter().enumerate() {
        let first = super::doc::as_obj(st)
            .keys()
            .next()
            .cloned()
            .unwrap_or_default();
        if first != "foreach" {
            continue;
        }
        let spec = st.get("foreach").cloned().unwrap_or(Value::Null);
        let mut reader = OptionalReads::new(library, tn, Some(x));
        let ty = spec
            .get("array")
            .and_then(|a| parse(&py_str(a)).ok())
            .and_then(|tree| reader.walk(&tree, env));
        let mut inner = env.clone();
        inner.insert(
            str_or(&spec, "item", ""),
            ty.and_then(|t| t.strip_suffix("[]").map(str::to_string)),
        );
        if let Some(w) = spec.get("where").filter(|w| super::doc::truthy(Some(w))) {
            let mut r = OptionalReads::new(library, tn, Some(x));
            if let Ok(tree) = parse(&py_str(w)) {
                r.walk(&tree, &inner);
            }
            let why = r.left();
            if !why.is_empty() {
                let over = spec
                    .get("array")
                    .or_else(|| spec.get("range"))
                    .map_or_else(|| "None".to_string(), py_str);
                out.push(finding(
                    ext(&at, segs![i, "foreach", "where"]),
                    "unknown",
                    format!("the loop over {over} in {xn} filters by what can be unknown, through {}, which it never tests; an element it cannot decide is left out", said(&why)),
                ));
            }
        }
        loops(
            library,
            tn,
            xn,
            x,
            list(&spec, "steps"),
            ext(&at, segs![i, "foreach", "steps"]),
            &inner,
            out,
        );
    }
}

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
use super::steps::kind;
use super::view::types;
use crate::expr::{Expr, parse};

/// An invariant that can only refuse other objects' changes, as
/// `check-flows.py`'s `vetoes`: one a type declares whose every state is
/// final, so no transition of its own can break it once it is created,
/// reading through a reference or its referrers, so it is checked again
/// whenever what it reads changes, and refuses that change (`flow-format.md`
/// §4.6, ADR-0176 decision 5).
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
                    "{tn}.{i} reads through {what}, and every state of {tn} is final, so no transition of its own can break it: it can only refuse a change to what it reads there. To hold a fact as it stood when the {tn} was made, copy it onto the {tn} then (write-a-flow.md §5, ADR-0176)"
                ),
            ));
        }
    }
    out
}

/// A guard marked `audit` or `warn`, and a `self_serviceable` guard on a
/// transition that takes no input.
pub fn notices(doc: &Value) -> Found {
    let mut out = Found::new();
    for (tn, t) in types(doc) {
        let conds = obj(t, "conditions");
        for (xn, x) in obj(t, "transitions") {
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

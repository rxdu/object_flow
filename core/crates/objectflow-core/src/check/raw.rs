//! The checks of step 3 that read a module as it is written, before the
//! view merges machines and bases into types: the order of its sections,
//! cycles among derived attributes, its families, its machines, its
//! decimals and the names of its types (`check-flows.py`).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::doc::{
    Found, Obj, Path, ext, finding, flag, has, keys, obj, optional, py_str, re, segs, squash,
    str_or, strings, text, truthy,
};
use super::view::{INHERITED, bases, types};
use crate::yaml::Seg;

const MODULE_ORDER: &[&str] = &[
    "module",
    "imports",
    "publishing",
    "categories",
    "enumerations",
    "sequences",
    "evaluators",
    "machines",
    "types",
    "metrics",
    "migration",
];
const COMBINED_ORDER: &[&str] = &[
    "description",
    "input_metrics",
    "group_by",
    "expression",
    "flag_when",
];
const MACHINE_ORDER: &[&str] = &[
    "description",
    "requires",
    "states",
    "conditions",
    "transitions",
];
const METRIC_ORDER: &[&str] = &[
    "description",
    "measure",
    "state",
    "transition",
    "source",
    "item",
    "split_by",
    "filter",
    "dimensions",
    "group_by",
    "time_dimension",
    "expression",
    "flag_when",
];
const TYPE_ORDER: &[&str] = &[
    "description",
    "abstract",
    "extends",
    "mirror",
    "tracking",
    "state_machine",
    "taken_by",
    "attributes",
    "inherited_parts",
    "observations",
    "states",
    "derived_attributes",
    "summary",
    "invariants",
    "conditions",
    "transitions",
    "metrics",
];
pub const BUILT_IN_TYPES: &[&str] = &[
    "string",
    "bool",
    "int",
    "decimal",
    "money",
    "timestamp",
    "duration",
    "identity",
    "file",
    "event",
    "date",
    "actor_kind",
];

/// Every name is defined before it is used: sections in their order, and
/// derived attributes reading each other in no cycle.
pub fn order_errors(doc: &Value) -> Found {
    let mut out = Found::new();
    let mut in_order = |keys: Vec<String>, order: &[&str], at: Path, place: String| {
        let index = |k: &str| order.iter().position(|o| *o == k);
        let mut top: Option<String> = None;
        for k in &keys {
            let Some(i) = index(k) else { continue };
            let t = top.as_deref().and_then(index);
            if t.is_some_and(|t| i < t) {
                let named: Vec<&str> = order
                    .iter()
                    .copied()
                    .filter(|o| keys.iter().any(|k| k == o))
                    .collect();
                out.push(finding(
                    ext(&at, segs![k]),
                    "order",
                    format!(
                        "{place}'{k}' comes after '{}'; declare {} in that order",
                        top.as_deref().unwrap_or_default(),
                        named.join(", ")
                    ),
                ));
            } else if t.is_none_or(|t| i > t) {
                top = Some(k.clone());
            }
        }
    };
    in_order(
        super::doc::as_obj(doc).keys().cloned().collect(),
        MODULE_ORDER,
        Path::new(),
        String::new(),
    );
    for (mn, m) in obj(doc, "machines") {
        in_order(
            super::doc::as_obj(m).keys().cloned().collect(),
            MACHINE_ORDER,
            segs!["machines", mn],
            format!("in {mn}, "),
        );
    }
    for (tn, t) in types(doc) {
        in_order(
            super::doc::as_obj(t).keys().cloned().collect(),
            TYPE_ORDER,
            segs!["types", &tn],
            format!("in {tn}, "),
        );
        for (mn, m) in obj(t, "metrics") {
            in_order(
                super::doc::as_obj(m).keys().cloned().collect(),
                METRIC_ORDER,
                segs!["types", &tn, "metrics", mn],
                format!("in the metric {mn}, "),
            );
        }
    }
    for (mn, m) in obj(doc, "metrics") {
        in_order(
            super::doc::as_obj(m).keys().cloned().collect(),
            COMBINED_ORDER,
            segs!["metrics", mn],
            format!("in the metric {mn}, "),
        );
    }
    out.extend(derived_cycle_errors(doc));
    out
}

/// Derived attributes MUST NOT read each other in a cycle (the model's check
/// 3): `.<name>` of a derived attribute the module declares counts as a read
/// of it, another type's or its own type's through a reference, where
/// `depth: parent.depth + 1` on a Node whose parent is a Node, a cycle of
/// parents or a chain of thousands, overflowed the stack on a read (tenth
/// production review). The reads are found by each expression's names after
/// a dot, and the search visits each attribute once, iteratively, so neither
/// a long chain nor many paths makes it long.
fn derived_cycle_errors(doc: &Value) -> Found {
    let order: Vec<(String, String)> = types(doc)
        .into_iter()
        .flat_map(|(tn, t)| {
            keys(t, "derived_attributes")
                .into_iter()
                .map(move |d| (tn.clone(), d))
        })
        .collect();
    let index: BTreeMap<&(String, String), usize> =
        order.iter().enumerate().map(|(i, n)| (n, i)).collect();
    let mut by_name: BTreeMap<&str, Vec<&(String, String)>> = BTreeMap::new();
    for node in &order {
        by_name.entry(node.1.as_str()).or_default().push(node);
    }
    let typed: BTreeMap<String, &Value> = types(doc).into_iter().collect();
    let mut edges: BTreeMap<&(String, String), Vec<&(String, String)>> = BTreeMap::new();
    for node in &order {
        let spec = typed
            .get(&node.0)
            .and_then(|t| obj(t, "derived_attributes").get(&node.1).cloned());
        let text = squash(
            &spec
                .as_ref()
                .and_then(|s| s.get("expression"))
                .map(py_str)
                .unwrap_or_default(),
        );
        let named: BTreeSet<&str> = AFTER_DOT
            .captures_iter(&text)
            .filter_map(|m| m.get(1).map(|g| g.as_str()))
            .collect();
        let mut to: Vec<&(String, String)> = named
            .iter()
            .flat_map(|nm| by_name.get(nm).into_iter().flatten().copied())
            .collect();
        to.sort_by_key(|n| index.get(n).copied().unwrap_or(usize::MAX));
        edges.insert(node, to);
    }
    let mut out = Found::new();
    let mut reported: BTreeSet<&(String, String)> = BTreeSet::new();
    // 1 on the search's path, 2 done
    let mut state: BTreeMap<&(String, String), u8> = BTreeMap::new();
    for start in &order {
        if state.contains_key(start) {
            continue;
        }
        let mut trail: Vec<&(String, String)> = vec![start];
        let mut stack: Vec<(&(String, String), usize)> = vec![(start, 0)];
        state.insert(start, 1);
        while let Some((node, k)) = stack.last().copied() {
            let nexts = edges.get(node).map_or(&[][..], Vec::as_slice);
            if k == nexts.len() {
                stack.pop();
                trail.pop();
                state.insert(node, 2);
                continue;
            }
            if let Some(top) = stack.last_mut() {
                top.1 = k + 1;
            }
            let next = nexts[k];
            match state.get(next) {
                Some(1) => {
                    let at = trail.iter().position(|n| *n == next).unwrap_or(0);
                    let mut ring: Vec<&(String, String)> = trail[at..].to_vec();
                    ring.push(next);
                    if !ring.iter().all(|r| reported.contains(r)) {
                        reported.extend(ring.iter().copied());
                        let names: Vec<String> =
                            ring.iter().map(|(u, e)| format!("{u}.{e}")).collect();
                        out.push(finding(
                            segs![
                                "types",
                                &next.0,
                                "derived_attributes",
                                &next.1,
                                "expression"
                            ],
                            "order",
                            format!(
                                "derived attributes read each other in a cycle: {}",
                                names.join(" → ")
                            ),
                        ));
                    }
                }
                Some(_) => {}
                None => {
                    state.insert(next, 1);
                    trail.push(next);
                    stack.push((next, 0));
                }
            }
        }
    }
    out
}

/// The most reads of derived attributes a derived attribute may unfold to,
/// each counted as often as it is made.
pub const UNFOLD_LIMIT: usize = 10_000;

/// No expression MAY unfold to more than [`UNFOLD_LIMIT`] reads of derived
/// attributes, each counted as often as it is made: one reading another
/// twice reads everything that one reads twice, so `d1: d0 + d0` on to `d40`
/// unfolds to 2^41 reads, and every check that reads a derived attribute
/// through those it reads walked each again, forty holding a dry run's slot
/// past six minutes; an invariant reading one a thousand times is inlined a
/// thousand times (eleventh production review). A name after a dot reads
/// every derived attribute of that name the module declares, as
/// [`derived_cycle_errors`] reads it, and a bare name the one its own type
/// declares; a read back into a cycle counts one, the cycle refused by its
/// own check. Each derived attribute's count is found once, iteratively, and
/// capped past the limit. The first derived attribute past it on each path
/// is refused, and any other expression past it through none that is; a
/// module holding one is refused before any later step walks it, as one
/// that does not parse is.
pub fn fan_out_errors(doc: &Value, expressions: &[(Path, String)]) -> Found {
    let order: Vec<(String, String)> = types(doc)
        .into_iter()
        .flat_map(|(tn, t)| {
            keys(t, "derived_attributes")
                .into_iter()
                .map(move |d| (tn.clone(), d))
        })
        .collect();
    let index: BTreeMap<&(String, String), usize> =
        order.iter().enumerate().map(|(i, n)| (n, i)).collect();
    let mut by_name: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, node) in order.iter().enumerate() {
        by_name.entry(node.1.as_str()).or_default().push(i);
    }
    let typed: BTreeMap<String, &Value> = types(doc).into_iter().collect();
    // an expression's reads, in its text's order, a repeated read repeated,
    // a bare name read of the type given
    let reads_of = |own: Option<&str>, text: &str| {
        let mut to = Vec::new();
        for m in NAMED.captures_iter(&squash(text)) {
            let Some(name) = m.get(2).map(|g| g.as_str()) else {
                continue;
            };
            if m.get(1).is_some() {
                to.extend(by_name.get(name).into_iter().flatten().copied());
            } else if let Some(i) =
                own.and_then(|tn| index.get(&(tn.to_string(), name.to_string())))
            {
                to.push(*i);
            }
        }
        to
    };
    let reads: Vec<Vec<usize>> = order
        .iter()
        .map(|(tn, d)| {
            let text = typed
                .get(tn)
                .and_then(|t| obj(t, "derived_attributes").get(d))
                .and_then(|s| s.get("expression"))
                .map(py_str)
                .unwrap_or_default();
            reads_of(Some(tn), &text)
        })
        .collect();
    let cap = UNFOLD_LIMIT + 1;
    let mut size: Vec<Option<usize>> = vec![None; order.len()];
    let mut on = vec![false; order.len()];
    for start in 0..order.len() {
        if size[start].is_some() {
            continue;
        }
        // each attribute on the search's path, the read it is at, and what
        // it unfolds to so far
        let mut stack: Vec<(usize, usize, usize)> = vec![(start, 0, 1)];
        on[start] = true;
        while let Some(&(node, k, total)) = stack.last() {
            if k == reads[node].len() {
                stack.pop();
                on[node] = false;
                size[node] = Some(total);
                if let Some(parent) = stack.last_mut() {
                    parent.2 = (parent.2 + total).min(cap);
                }
                continue;
            }
            let next = reads[node][k];
            let add = match size[next] {
                Some(s) => Some(s),
                None if on[next] => Some(1),
                None => None,
            };
            if let Some(top) = stack.last_mut() {
                top.1 = k + 1;
                if let Some(s) = add {
                    top.2 = (total + s).min(cap);
                }
            }
            if add.is_none() {
                on[next] = true;
                stack.push((next, 0, 1));
            }
        }
    }
    let past = |i: usize| size[i].is_some_and(|s| s > UNFOLD_LIMIT);
    let unfolds = format!(
        "unfolds to more than {UNFOLD_LIMIT} reads of derived attributes, each counted as often as it is made: one reading another twice reads all that one reads twice"
    );
    let mut out = Found::new();
    for (i, (tn, d)) in order.iter().enumerate() {
        if past(i) && !reads[i].iter().any(|r| past(*r)) {
            out.push(finding(
                segs!["types", tn, "derived_attributes", d, "expression"],
                "order",
                format!("{tn}.{d} {unfolds}"),
            ));
        }
    }
    // every other expression, its reads unfolded as a derived attribute's
    for (path, text) in expressions {
        let own = match (path.first(), path.get(1)) {
            (Some(Seg::Key(k)), Some(Seg::Key(tn))) if k == "types" => Some(tn.as_str()),
            _ => None,
        };
        if own.is_some() && matches!(path.get(2), Some(Seg::Key(k)) if k == "derived_attributes") {
            continue;
        }
        let read = reads_of(own, text);
        let total = read
            .iter()
            .fold(1, |n, r| (n + size[*r].unwrap_or(1)).min(cap));
        if total > UNFOLD_LIMIT && !read.iter().any(|r| past(*r)) {
            let at: Vec<String> = path
                .iter()
                .map(|s| match s {
                    Seg::Key(k) => k.clone(),
                    Seg::Index(i) => i.to_string(),
                })
                .collect();
            out.push(finding(
                path.clone(),
                "order",
                format!("{} {unfolds}", at.join(".")),
            ));
        }
    }
    out
}

/// A name, after a dot or bare, as [`fan_out_errors`] reads it.
static NAMED: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| re(r"(\.)?\b([A-Za-z_]\w*)\b"));

/// A name after a dot, as an expression reads a member.
static AFTER_DOT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| re(r"\.(\w+)"));

/// A base is declared and abstract, extending never comes back to itself, a
/// subtype does not redeclare what it inherits, a type with objects has a
/// tracking mode, and a mirror's family and references are a mirror's (the
/// model's checks 33, 42 and 43; ADR-0075, ADR-0148).
pub fn family_errors(doc: &Value, library: &Obj) -> Found {
    let mut kinds = library.clone();
    for (tn, t) in types(doc) {
        kinds.insert(tn, t.clone());
    }
    let mut out = Found::new();
    for (tn, t) in types(doc) {
        let here = segs!["types", &tn];
        if let Some(base) = t.get("extends").filter(|b| super::doc::truthy(Some(b))) {
            let base = py_str(base);
            match kinds.get(&base) {
                None => out.push(finding(
                    ext(&here, segs!["extends"]),
                    "names",
                    format!("{tn} extends {base}, which is neither declared in this module nor imported"),
                )),
                Some(b) if !flag(b, "abstract") => out.push(finding(
                    ext(&here, segs!["extends"]),
                    "names",
                    format!("{tn} extends {base}, which is not abstract; a base has no objects of its own"),
                )),
                Some(_) => {}
            }
            let mut step = Some(base.clone());
            let mut seen = BTreeSet::new();
            while let Some(s) = step {
                if !kinds.contains_key(&s) || !seen.insert(s.clone()) {
                    break;
                }
                if s == tn {
                    out.push(finding(
                        ext(&here, segs!["extends"]),
                        "names",
                        format!("{tn} extends {base}, and following its bases comes back to {tn}"),
                    ));
                    break;
                }
                step = kinds
                    .get(&s)
                    .and_then(|k| k.get("extends"))
                    .filter(|e| !e.is_null())
                    .map(py_str);
            }
        }
        let chain = bases(&tn, &kinds);
        for key in INHERITED {
            for n in keys(t, key) {
                if let Some(owner) = chain
                    .iter()
                    .find(|c| obj(&kinds[c.as_str()], key).contains_key(&n))
                {
                    out.push(finding(
                        ext(&here, segs![*key, &n]),
                        "names",
                        format!("{tn} declares '{n}', which it inherits from {owner}; a member has one declaration"),
                    ));
                }
            }
        }
        let mut members_all: BTreeSet<String> = keys(t, "attributes").into_iter().collect();
        members_all.extend(keys(t, "derived_attributes"));
        members_all.insert("state".into());
        for c in &chain {
            members_all.extend(keys(&kinds[c.as_str()], "attributes"));
            members_all.extend(keys(&kinds[c.as_str()], "derived_attributes"));
        }
        for m in strings(t, "summary") {
            if !members_all.contains(&m) {
                out.push(finding(
                    ext(&here, segs!["summary"]),
                    "names",
                    format!(
                        "{tn}'s summary names '{m}', which is neither state nor a member of {tn}"
                    ),
                ));
            }
        }
        if !flag(t, "abstract")
            && !has(t, "tracking")
            && !chain.iter().any(|c| has(&kinds[c.as_str()], "tracking"))
        {
            out.push(finding(
                here.clone(),
                "names",
                format!("{tn} has no tracking, declared or inherited"),
            ));
        }
        let mut members: Obj = obj(t, "attributes").clone();
        for c in &chain {
            for (k, v) in obj(&kinds[c.as_str()], "attributes") {
                members.insert(k.clone(), v.clone());
            }
        }
        let mirror = flag(t, "mirror");
        for c in &chain {
            if flag(&kinds[c.as_str()], "mirror") != mirror {
                let (is, other) = if mirror {
                    ("is", "is not")
                } else {
                    ("is not", "is")
                };
                out.push(finding(
                    here.clone(),
                    "names",
                    format!(
                        "{tn} {is} a mirror and extends {c}, which {other}; a family is ported together, every type of it a mirror or none (ADR-0148)"
                    ),
                ));
                break;
            }
        }
        if mirror && !flag(t, "abstract") && !members.values().any(|a| flag(a, "external")) {
            out.push(finding(
                here.clone(),
                "names",
                format!("{tn} is a mirror and declares no external identifier, by which the import matches its objects"),
            ));
        }
        if mirror {
            for (a, spec) in obj(t, "attributes") {
                let reference = spec.get("reference").map(py_str).unwrap_or_default();
                if reference.is_empty() {
                    continue;
                }
                let name = reference.trim_end_matches("[]");
                let Some(target) = kinds.get(name) else {
                    continue;
                };
                if flag(target, "mirror") {
                    continue;
                }
                let opposite = text(spec, "opposite").unwrap_or_default();
                let far = obj(target, "attributes")
                    .get(opposite)
                    .cloned()
                    .unwrap_or(Value::Null);
                let stored = !has(spec, "opposite")
                    || flag(spec, "stored")
                    || (!reference.ends_with("[]") && kind_text_ref(&far).ends_with("[]"));
                if stored {
                    out.push(finding(
                        ext(&here, segs!["attributes", a]),
                        "names",
                        format!(
                            "{tn}.{a} is a mirror's reference to {name}, which this store owns: a type is cut over only after every type that references it (ADR-0075), so a mirror references only mirrors"
                        ),
                    ));
                }
            }
        }
        for (a, spec) in obj(t, "attributes") {
            if flag(spec, "external") && has(spec, "reference") {
                out.push(finding(
                    ext(&here, segs!["attributes", a, "external"]),
                    "names",
                    format!("{tn}.{a} is a reference; an external identifier is a value the other system gives"),
                ));
            }
        }
    }
    out
}

/// `str(spec.get("reference", ""))`.
fn kind_text_ref(spec: &Value) -> String {
    spec.get("reference").map(py_str).unwrap_or_default()
}

/// A binder names a declared or imported machine and declares what it
/// requires, of the same type (check 16); a machine's states require no
/// attributes.
pub fn machine_errors(doc: &Value, kinds: &Obj) -> Found {
    let mut out = Found::new();
    let machines = obj(doc, "machines");
    let imported = imported(doc);
    for (mn, m) in machines {
        if obj(doc, "types").contains_key(mn) {
            out.push(finding(
                segs!["machines", mn],
                "names",
                format!("{mn} names both a machine and a type"),
            ));
        }
        for (sn, st) in obj(m, "states") {
            if flag(st, "required_attributes") {
                out.push(finding(
                    segs!["machines", mn, "states", sn, "required_attributes"],
                    "names",
                    format!(
                        "state {sn} of the machine {mn} requires attributes; declare them as an invariant of each type that binds {mn}"
                    ),
                ));
            }
        }
    }
    for (tn, t) in types(doc) {
        let Some(mn) = t
            .get("state_machine")
            .filter(|m| super::doc::truthy(Some(m)))
            .map(py_str)
        else {
            continue;
        };
        let Some(m) = machines.get(&mn) else {
            if !imported.contains(&mn) {
                out.push(finding(
                    segs!["types", &tn, "state_machine"],
                    "names",
                    format!(
                        "{tn} binds {mn}, which is neither a machine of this module nor imported"
                    ),
                ));
            }
            continue;
        };
        let req = m.get("requires").cloned().unwrap_or(Value::Null);
        let full = kinds.get(&tn).cloned().unwrap_or_else(|| t.clone());
        for (a, want) in obj(&req, "attributes") {
            let Some(have) = obj(&full, "attributes").get(a).filter(|h| !h.is_null()) else {
                out.push(finding(
                    segs!["types", &tn, "state_machine"],
                    "names",
                    format!("{tn} binds {mn}, which requires the attribute '{a}', and {tn} does not declare it"),
                ));
                continue;
            };
            let sig = |s: &Value| {
                (
                    s.get("type").cloned(),
                    s.get("reference").cloned(),
                    flag(s, "optional"),
                )
            };
            if sig(have) != sig(want) {
                out.push(finding(
                    segs!["types", &tn, "attributes", a],
                    "names",
                    format!(
                        "{tn} declares '{a}' as {}, and its machine {mn} requires {}",
                        declared_as(a, have),
                        declared_as(a, want)
                    ),
                ));
            }
        }
        for i in strings(&req, "invariants") {
            if !obj(&full, "invariants").contains_key(&i) {
                out.push(finding(
                    segs!["types", &tn, "state_machine"],
                    "names",
                    format!("{tn} binds {mn}, which requires the invariant '{i}', and {tn} does not declare it"),
                ));
            }
        }
        for c in keys(t, "conditions") {
            if obj(m, "conditions").contains_key(&c) {
                out.push(finding(
                    segs!["types", &tn, "conditions", &c],
                    "names",
                    format!(
                        "{tn} declares the condition '{c}', which its machine {mn} also declares"
                    ),
                ));
            }
        }
        for x in keys(t, "transitions") {
            if let Some(mx) = obj(m, "transitions").get(&x)
                && text(mx, "kind") != Some("initial")
            {
                out.push(finding(
                    segs!["types", &tn, "transitions", &x],
                    "names",
                    format!(
                        "{tn} declares the transition '{x}', which its machine {mn} also declares"
                    ),
                ));
            }
        }
    }
    out
}

/// The names a module imports.
pub fn imported(doc: &Value) -> BTreeSet<String> {
    obj(doc, "imports")
        .values()
        .flat_map(|names| match names {
            Value::Array(a) => a.iter().map(py_str).collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect()
}

static DECIMAL: LazyLock<Regex> =
    LazyLock::new(|| re(r"^decimal\(\s*(\d+)\s*,\s*(\d+)\s*\)(\[\])?$"));
static TYPE_HEAD: LazyLock<Regex> = LazyLock::new(|| re(r"^[A-Za-z_][A-Za-z0-9_]*"));

/// Every decimal a module declares holds 1 to 18 digits, its scale within
/// them (ADR-0143).
pub fn decimal_errors(doc: &Value) -> Found {
    let mut out = Found::new();
    fn walk(node: &Value, at: &mut Path, out: &mut Found) {
        match node {
            Value::Object(o) => {
                if let Some(Value::String(t)) = o.get("type")
                    && let Some(m) = DECIMAL.captures(t)
                {
                    let p: u64 = m[1].parse().unwrap_or(0);
                    let s: u64 = m[2].parse().unwrap_or(u64::MAX);
                    if !((1..=18).contains(&p) && s <= p) {
                        let last = match at.last() {
                            Some(Seg::Key(k)) => k.clone(),
                            Some(Seg::Index(i)) => i.to_string(),
                            None => String::new(),
                        };
                        out.push(finding(
                            ext(&*at, segs!["type"]),
                            "names",
                            format!(
                                "{last} is a decimal({},{}); a decimal holds 1 to 18 digits, and its scale is within them",
                                &m[1], &m[2]
                            ),
                        ));
                    }
                }
                for (k, v) in o {
                    at.push(Seg::Key(k.clone()));
                    walk(v, at, out);
                    at.pop();
                }
            }
            Value::Array(a) => {
                for (i, v) in a.iter().enumerate() {
                    at.push(Seg::Index(i));
                    walk(v, at, out);
                    at.pop();
                }
            }
            _ => {}
        }
    }
    walk(doc, &mut Path::new(), &mut out);
    out
}

/// Every type a module names is built in, an enumeration it declares, or a
/// name it imports (`flow-format.md` §4.3, D493).
pub fn type_name_errors(doc: &Value) -> Found {
    let mut known: BTreeSet<String> = BUILT_IN_TYPES.iter().map(|s| s.to_string()).collect();
    known.insert("counter".into());
    known.extend(keys(doc, "enumerations"));
    known.extend(imported(doc));
    let mut out = Found::new();
    let mut one = |spec: &Value, at: Path| {
        if let Some(Value::String(typ)) = spec.get("type")
            && let Some(head) = TYPE_HEAD.find(typ)
            && !known.contains(head.as_str())
        {
            let last = match at.last() {
                Some(Seg::Key(k)) => k.clone(),
                _ => String::new(),
            };
            out.push(finding(
                ext(&at, segs!["type"]),
                "names",
                format!(
                    "{last} is of the type {typ}, which is neither built in, nor an enumeration this module declares, nor a name it imports"
                ),
            ));
        }
    };
    for kind in ["types", "machines"] {
        for (tn, t) in obj(doc, kind) {
            let base = segs![kind, tn];
            for (n, spec) in obj(t, "attributes") {
                one(spec, ext(&base, segs!["attributes", n]));
            }
            if let Some(Value::Object(req)) = t.get("requires") {
                for (n, spec) in req {
                    one(spec, ext(&base, segs!["requires", n]));
                }
            }
            for (o, ob) in obj(t, "observations") {
                for (n, spec) in obj(ob, "attributes") {
                    one(spec, ext(&base, segs!["observations", o, "attributes", n]));
                }
            }
            for (xn, x) in obj(t, "transitions") {
                for (n, spec) in obj(x, "inputs") {
                    one(spec, ext(&base, segs!["transitions", xn, "inputs", n]));
                }
            }
        }
    }
    for (e, ev) in obj(doc, "evaluators") {
        for (f, fun) in obj(ev, "functions") {
            for (n, spec) in obj(fun, "arguments") {
                one(spec, segs!["evaluators", e, "functions", f, "arguments", n]);
            }
        }
    }
    out
}

/// The markings an attribute or a reference carries after its type, in the
/// model's words (`declaration-syntax.md` §3.1, §3.2; `check-flows.py`'s
/// `markings`).
fn markings(spec: &Value) -> String {
    let mut marks: Vec<String> = Vec::new();
    if flag(spec, "external") {
        marks.push(format!("external \"{}\"", str_or(spec, "external", "")));
    }
    if let Some(d) = spec.get("default") {
        marks.push(format!("default {}", squash(&py_str(d))));
    }
    marks.extend(
        ["indexed", "personal"]
            .into_iter()
            .filter(|m| flag(spec, m))
            .map(String::from),
    );
    match spec.get("unique") {
        Some(Value::Bool(true)) => marks.push("unique".into()),
        Some(Value::String(s)) if s == "in_scope" => marks.push("unique in scope".into()),
        Some(u @ Value::Object(o)) if o.contains_key("with") => {
            marks.push(format!("unique with {}", strings(u, "with").join(", ")))
        }
        Some(u @ Value::Object(o)) if o.contains_key("where") => {
            marks.push(format!("unique where {}", squash(&str_or(u, "where", ""))))
        }
        _ => {}
    }
    marks.iter().map(|m| format!(" {m}")).collect()
}

/// The first line an attribute is declared by in the internal form
/// (`check-flows.py`'s `attribute_line`, outside an observation and with no
/// module to tell an owner by).
fn declared_as(name: &str, spec: &Value) -> String {
    let q = if optional(spec) { "?" } else { "" };
    if let Some(r) = spec.get("reference") {
        let r = py_str(r);
        let inverse = spec
            .get("opposite")
            .filter(|o| truthy(Some(o)))
            .map(|o| format!(" inverse {}", py_str(o)))
            .unwrap_or_default();
        if str_or(spec, "aggregation", "") == "composite" {
            return format!("part {name} : {r}{q}{inverse}");
        }
        let stored = if flag(spec, "stored") { " stored" } else { "" };
        let assignee = if flag(spec, "assignee") {
            " assignee"
        } else {
            ""
        };
        return format!(
            "ref {name} : {r}{q}{inverse}{stored}{assignee}{}",
            markings(spec)
        );
    }
    let typ = str_or(spec, "type", "");
    if typ == "counter" {
        return format!("counter {name}{}", markings(spec));
    }
    let mut marks: Vec<String> = Vec::new();
    if flag(spec, "actor_kind") {
        marks.push(format!("actor {}", str_or(spec, "actor_kind", "")));
    }
    if let Some(ident) = spec.get("identifier").filter(|i| truthy(Some(i))) {
        let scope = if flag(ident, "scope") {
            format!(" scoped by {}", str_or(ident, "scope", ""))
        } else {
            String::new()
        };
        marks.push(format!(
            "identifier from {}{scope} format \"{}\"",
            str_or(ident, "sequence", ""),
            str_or(ident, "format", "")
        ));
    }
    let marks: String = marks.iter().map(|m| format!(" {m}")).collect();
    format!("attr {name} {typ}{q}{marks}{}", markings(spec))
}

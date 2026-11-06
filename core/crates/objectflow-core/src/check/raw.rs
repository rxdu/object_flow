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
    "values",
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

/// The most derived attributes, each reading the next, one derived
/// attribute's reads go through: an evaluation goes through at most
/// `eval::DERIVED_HOPS`, past which a read is not run, where a chain of
/// 9,500 that both checkers accepted answered all past the 65th as null
/// (fourteenth production review N1).
pub const DEPTH_LIMIT: usize = crate::eval::DERIVED_HOPS;

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
///
/// A name after a dot also reads the derived attributes of that name the
/// modules checked before this one declare, `outer` holding the most each
/// unfolds to: a chain split across a module and one importing it passed
/// each module's bound and multiplied past it, a dry run's checks holding
/// the slot to its deadline and the reference checker without end (twelfth
/// production review). Beside the findings, what each of the module's
/// derived attributes unfolds to, by name, which the caller adds to `outer`
/// for the modules after it.
pub fn fan_out_errors(
    doc: &Value,
    expressions: &[(Path, String)],
    outer: &BTreeMap<String, (usize, usize)>,
) -> (Found, Vec<(String, usize, usize)>) {
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
    let cap = UNFOLD_LIMIT + 1;
    // an expression's reads, in its text's order, a repeated read repeated,
    // a bare name read of the type given; and what its reads of the modules
    // checked before unfold to, together, and the deepest of them
    let reads_of = |own: Option<&str>, text: &str| {
        let mut to = Vec::new();
        let (mut before, mut deepest) = (0, 0);
        for m in NAMED.captures_iter(&squash(text)) {
            let Some(name) = m.get(2).map(|g| g.as_str()) else {
                continue;
            };
            if m.get(1).is_some() {
                to.extend(by_name.get(name).into_iter().flatten().copied());
                let (n, deep) = outer.get(name).copied().unwrap_or((0, 0));
                before = (before + n).min(cap);
                deepest = deepest.max(deep);
            } else if let Some(i) =
                own.and_then(|tn| index.get(&(tn.to_string(), name.to_string())))
            {
                to.push(*i);
            }
        }
        (to, before, deepest)
    };
    let mut reads: Vec<Vec<usize>> = Vec::new();
    let mut before: Vec<usize> = Vec::new();
    let mut below: Vec<usize> = Vec::new();
    for (tn, d) in &order {
        let text = typed
            .get(tn)
            .and_then(|t| obj(t, "derived_attributes").get(d))
            .and_then(|s| s.get("expression"))
            .map(py_str)
            .unwrap_or_default();
        let (to, n, deep) = reads_of(Some(tn), &text);
        reads.push(to);
        before.push(n);
        below.push(deep);
    }
    let mut size: Vec<Option<usize>> = vec![None; order.len()];
    let mut depth: Vec<usize> = vec![1; order.len()];
    let mut on = vec![false; order.len()];
    for start in 0..order.len() {
        if size[start].is_some() {
            continue;
        }
        // each attribute on the search's path, the read it is at, what it
        // unfolds to so far, and how deep its reads go so far
        let mut stack: Vec<(usize, usize, usize, usize)> =
            vec![(start, 0, (1 + before[start]).min(cap), 1 + below[start])];
        on[start] = true;
        while let Some(&(node, k, total, deep)) = stack.last() {
            if k == reads[node].len() {
                stack.pop();
                on[node] = false;
                size[node] = Some(total);
                depth[node] = deep;
                if let Some(parent) = stack.last_mut() {
                    parent.2 = (parent.2 + total).min(cap);
                    parent.3 = parent.3.max(1 + deep);
                }
                continue;
            }
            let next = reads[node][k];
            let add = match size[next] {
                Some(s) => Some((s, depth[next])),
                None if on[next] => Some((1, 1)),
                None => None,
            };
            if let Some(top) = stack.last_mut() {
                top.1 = k + 1;
                if let Some((s, d)) = add {
                    top.2 = (total + s).min(cap);
                    top.3 = deep.max(1 + d);
                }
            }
            if add.is_none() {
                on[next] = true;
                stack.push((next, 0, (1 + before[next]).min(cap), 1 + below[next]));
            }
        }
    }
    let past = |i: usize| size[i].is_some_and(|s| s > UNFOLD_LIMIT);
    let unfolds = format!(
        "unfolds to more than {UNFOLD_LIMIT} reads of derived attributes, each counted as often as it is made: one reading another twice reads all that one reads twice"
    );
    let deep = |i: usize| depth[i] > DEPTH_LIMIT;
    let mut out = Found::new();
    for (i, (tn, d)) in order.iter().enumerate() {
        if past(i) && !reads[i].iter().any(|r| past(*r)) {
            out.push(finding(
                segs!["types", tn, "derived_attributes", d, "expression"],
                "order",
                format!("{tn}.{d} {unfolds}"),
            ));
        }
        if deep(i) && !reads[i].iter().any(|r| deep(*r)) {
            out.push(finding(
                segs!["types", tn, "derived_attributes", d, "expression"],
                "order",
                format!(
                    "{tn}.{d} reads derived attributes more than {DEPTH_LIMIT} deep, each reading the next, past the most one evaluation goes through"
                ),
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
        let (read, earlier, _) = reads_of(own, text);
        let total = read.iter().fold((1 + earlier).min(cap), |n, r| {
            (n + size[*r].unwrap_or(1)).min(cap)
        });
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
    let sizes = order
        .iter()
        .enumerate()
        .map(|(i, (_, d))| (d.clone(), size[i].unwrap_or(1), depth[i]))
        .collect();
    (out, sizes)
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

/// `acts` on a state that is not final, of a machine or of a type that
/// marks an actor identity, its own or inherited (ADR-0241 decision 1): a
/// final state acts never, and a type with no actor's objects has no actor
/// for it to refuse. A machine's state may say it for the binders that hold
/// actors.
pub fn acts_errors(doc: &Value, kinds: &Obj) -> Found {
    let mut out = Found::new();
    let lifecycles = obj(doc, "machines")
        .iter()
        .map(|(n, m)| ("machines", n, m))
        .chain(obj(doc, "types").iter().map(|(n, t)| ("types", n, t)));
    for (section, name, decl) in lifecycles {
        for (sn, st) in obj(decl, "states") {
            if st.get("acts").is_none() {
                continue;
            }
            let at = segs![section, name, "states", sn, "acts"];
            if flag(st, "final") {
                out.push(finding(
                    at,
                    "names",
                    format!("'acts' applies only to a state that is not final, not to {name}.{sn}"),
                ));
            } else if section == "types" && !super::examples::marks_actor(name, kinds) {
                out.push(finding(
                    at,
                    "names",
                    format!(
                        "'acts' applies only to a state of a type that marks an actor identity, not to {name}.{sn}"
                    ),
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
    known.extend(keys(doc, "values"));
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
                    "{last} is of the type {typ}, which is neither built in, nor an enumeration or a value this module declares, nor a name it imports"
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

static VALUE_STRING: LazyLock<Regex> = LazyLock::new(|| re(r#""(?:[^"\\]|\\.)*""#));
static VALUE_CALL: LazyLock<Regex> = LazyLock::new(|| re(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\("));

/// What a value's field takes in this release (ADR-0254 decision 1).
const FIELD_TYPES: &[&str] = &[
    "string",
    "bool",
    "int",
    "decimal",
    "money",
    "timestamp",
    "date",
    "duration",
];

/// A text marked `not_blank` (ADR-0257): one string, an attribute of a type
/// or a transition's declared input, a machine's or a type's; no observation's
/// field and no machine's required attribute; and no invariant of the type
/// named as the one it generates, `<attribute>_not_blank`.
pub fn not_blank_errors(doc: &Value) -> Found {
    let mut out = Found::new();
    let marked = |spec: &Value| spec.get("not_blank").and_then(Value::as_bool) == Some(true);
    let typ = |spec: &Value| {
        spec.get("type")
            .map(py_str)
            .unwrap_or_else(|| "a reference".into())
    };
    let one_string = |spec: &Value| spec.get("type").map(py_str).as_deref() == Some("string");
    for kind in ["types", "machines"] {
        for (tn, t) in obj(doc, kind) {
            let base = segs![kind, tn];
            for (a, spec) in obj(t, "attributes") {
                if !marked(spec) {
                    continue;
                }
                let at = ext(&base, segs!["attributes", a, "not_blank"]);
                if kind == "machines" {
                    out.push(finding(at, "names", format!("the machine {tn} requires {a} marked not_blank; a machine requires no not_blank in this release, which the type binding it declares (ADR-0257)")));
                } else if !one_string(spec) {
                    out.push(finding(at, "names", format!("{tn}.{a} is marked not_blank and is of the type {}; not_blank marks one string in this release (ADR-0257)", typ(spec))));
                } else if obj(t, "invariants").contains_key(&format!("{a}_not_blank")) {
                    out.push(finding(
                        ext(&base, segs!["invariants", &format!("{a}_not_blank")]),
                        "names",
                        format!("{tn}.{a}_not_blank is the invariant {a}'s not_blank generates; name the type's invariant otherwise (ADR-0257)"),
                    ));
                }
            }
            for (o, ob) in obj(t, "observations") {
                for (f, spec) in obj(ob, "attributes") {
                    if marked(spec) {
                        out.push(finding(ext(&base, segs!["observations", o, "attributes", f, "not_blank"]), "names", format!("the observation {o}'s field {f} is marked not_blank; an observation's field takes no not_blank in this release (ADR-0257)")));
                    }
                }
            }
            for (xn, x) in obj(t, "transitions") {
                for (i, spec) in obj(x, "inputs") {
                    if marked(spec) && !one_string(spec) {
                        out.push(finding(ext(&base, segs!["transitions", xn, "inputs", i, "not_blank"]), "names", format!("{xn}'s input {i} is marked not_blank and is of the type {}; not_blank marks one string in this release (ADR-0257)", typ(spec))));
                    }
                }
            }
        }
    }
    // a subtype's own invariant named as the one a base's marked text
    // generates, which takes its place in the subtype; a text the subtype
    // declares again is its own (ADR-0257 decision 3)
    let kinds = obj(doc, "types");
    for (tn, t) in kinds {
        for b in super::view::bases(tn, kinds) {
            for (a, spec) in obj(&kinds[&b], "attributes") {
                let name = format!("{a}_not_blank");
                if marked(spec)
                    && one_string(spec)
                    && !obj(t, "attributes").contains_key(a)
                    && obj(t, "invariants").contains_key(&name)
                {
                    out.push(finding(
                        segs!["types", tn, "invariants", &name],
                        "names",
                        format!("{tn}.{name} is the invariant {b}.{a}'s not_blank generates, which {tn} inherits; name the type's invariant otherwise (ADR-0257)"),
                    ));
                }
            }
        }
    }
    out
}

/// What an attribute typed by a value takes beside its type in this
/// release (ADR-0254 decision 9).
const VALUE_MARKINGS: &[&str] = &["type", "optional", "description"];

/// A value's fields, and where a value may stand, in this release
/// (ADR-0254 decisions 1, 2, 6 and 9): a field is one value of a built-in
/// type or an enumeration; an attribute of a type holds one value, marked
/// only optional; a declared input takes one, with no default; and no
/// machine's requirement, observation's field or evaluator's argument is
/// one. No member is named for the column a field is kept in, as no
/// attribute is named for a reference's (D578). `check-flows.py`'s
/// `declared_value_errors`.
pub fn declared_value_errors(doc: &Value) -> Found {
    let values = obj(doc, "values");
    let mut out = Found::new();
    let enums: BTreeSet<String> = keys(doc, "enumerations").into_iter().collect();
    for (vn, v) in values {
        // a value's invariant reads its own fields, calling length or trim
        // alone, since each attribute holding it reads it through itself
        // (ADR-0255)
        let fields: BTreeSet<String> = keys(v, "attributes").into_iter().collect();
        for (i, inv) in obj(v, "invariants") {
            let text = inv.get("expression").map(py_str).unwrap_or_default();
            let at = segs!["values", vn, "invariants", i, "expression"];
            for r in super::doc::reads(&text).difference(&fields) {
                out.push(finding(at.clone(), "names", format!("the invariant {vn}.{i} reads {r}, which is no field of {vn}; a value's invariant reads its own fields (ADR-0255)")));
            }
            let bare = VALUE_STRING.replace_all(&text, "\"\"");
            let mut called: BTreeSet<String> = VALUE_CALL
                .captures_iter(&bare)
                .map(|c| c[1].to_string())
                .collect();
            // a keyword before a bracket, as `not (…)`, calls nothing
            called.retain(|c| {
                ![
                    "length", "trim", "and", "or", "not", "implies", "in", "is", "null", "if",
                    "then", "else", "true", "false",
                ]
                .contains(&c.as_str())
            });
            for c in called {
                out.push(finding(at.clone(), "names", format!("the invariant {vn}.{i} calls {c}; a value's invariant reads its own fields, calling length or trim alone (ADR-0255)")));
            }
        }
        for (f, spec) in obj(v, "attributes") {
            let typ = spec.get("type").map(py_str).unwrap_or_default();
            let head = TYPE_HEAD
                .find(&typ)
                .map(|h| h.as_str().to_string())
                .unwrap_or_default();
            if typ.ends_with("[]")
                || !(FIELD_TYPES.contains(&head.as_str()) || enums.contains(&head))
            {
                out.push(finding(
                    segs!["values", vn, "attributes", f, "type"],
                    "names",
                    format!("the field {vn}.{f} is of the type {typ}, which a value's field does not take in this release: one string, bool, int, decimal, money, timestamp, date or duration, or a member of an enumeration this module declares (ADR-0254)"),
                ));
            }
        }
    }
    if values.is_empty() {
        return out;
    }
    let value_of = |spec: &Value| -> Option<(String, bool)> {
        let typ = spec.get("type").map(py_str)?;
        let base = typ.trim_end_matches("[]");
        values
            .contains_key(base)
            .then(|| (base.to_string(), typ.ends_with("[]")))
    };
    for kind in ["types", "machines"] {
        for (tn, t) in obj(doc, kind) {
            let base = segs![kind, tn];
            let members: BTreeSet<String> = keys(t, "attributes")
                .into_iter()
                .chain(keys(t, "derived_attributes"))
                .collect();
            for (a, spec) in obj(t, "attributes") {
                let Some((vn, many)) = value_of(spec) else {
                    continue;
                };
                let at = ext(&base, segs!["attributes", a]);
                if kind == "machines" {
                    out.push(finding(at, "names", format!("the machine {tn} requires {a}, a {vn}; a machine requires no value in this release (ADR-0254)")));
                    continue;
                }
                if many {
                    out.push(finding(ext(&at, segs!["type"]), "names", format!("{tn}.{a} is a set of {vn}; an attribute holds one value of several fields in this release (ADR-0254)")));
                }
                for k in super::doc::as_obj(spec)
                    .keys()
                    .filter(|k| !VALUE_MARKINGS.contains(&k.as_str()))
                {
                    out.push(finding(
                        ext(&at, segs![k]),
                        "names",
                        format!(
                            "{tn}.{a} is a {vn}, which takes no {k} in this release (ADR-0254)"
                        ),
                    ));
                }
                for i in keys(&values[&vn], "invariants") {
                    if obj(t, "invariants").contains_key(&format!("{a}_{i}")) {
                        out.push(finding(
                            ext(&base, segs!["invariants", &format!("{a}_{i}")]),
                            "names",
                            format!("{tn}.{a}_{i} is the invariant {a} holds of its value {vn}'s {i}; name the type's invariant otherwise (ADR-0255)"),
                        ));
                    }
                }
                for f in keys(&values[&vn], "attributes") {
                    let col = format!("{a}__{f}");
                    if members.contains(&col) {
                        out.push(finding(
                            ext(&base, segs!["attributes", &col]),
                            "names",
                            format!("{tn}.{col} is the column the store keeps the field {f} of {a} in; name the member otherwise (ADR-0254, as D578)"),
                        ));
                    }
                }
            }
            for (o, ob) in obj(t, "observations") {
                for (f, spec) in obj(ob, "attributes") {
                    if let Some((vn, _)) = value_of(spec) {
                        out.push(finding(ext(&base, segs!["observations", o, "attributes", f, "type"]), "names", format!("the observation {o}'s field {f} is a {vn}; an observation records no value of several fields in this release (ADR-0254)")));
                    }
                }
            }
            for (xn, x) in obj(t, "transitions") {
                for (i, spec) in obj(x, "inputs") {
                    let Some((vn, many)) = value_of(spec) else {
                        continue;
                    };
                    let at = ext(&base, segs!["transitions", xn, "inputs", i]);
                    if many {
                        out.push(finding(ext(&at, segs!["type"]), "names", format!("{xn}'s input {i} is a set of {vn}; an input takes one value of several fields in this release (ADR-0254)")));
                    }
                    for k in ["default", "personal", "media_types"] {
                        if spec.get(k).is_some() {
                            out.push(finding(ext(&at, segs![k]), "names", format!("{xn}'s input {i} is a {vn}, which takes no {k} in this release (ADR-0254)")));
                        }
                    }
                }
            }
        }
    }
    for (e, ev) in obj(doc, "evaluators") {
        for (f, fun) in obj(ev, "functions") {
            for (n, spec) in obj(fun, "arguments") {
                if let Some((vn, _)) = value_of(spec) {
                    out.push(finding(segs!["evaluators", e, "functions", f, "arguments", n, "type"], "names", format!("the argument {n} of {e}.{f} is a {vn}; an evaluator takes no value of several fields in this release (ADR-0254)")));
                }
            }
        }
    }
    // a subtype's own invariant named as one a base's value generates, which
    // takes its place in the subtype; an attribute the subtype declares again
    // is its own (ADR-0255)
    let kinds = obj(doc, "types");
    for (tn, t) in kinds {
        for b in super::view::bases(tn, kinds) {
            for (a, spec) in obj(&kinds[&b], "attributes") {
                let Some((vn, false)) = value_of(spec) else {
                    continue;
                };
                if obj(t, "attributes").contains_key(a) {
                    continue;
                }
                for i in keys(&values[&vn], "invariants") {
                    let name = format!("{a}_{i}");
                    if obj(t, "invariants").contains_key(&name) {
                        out.push(finding(
                            segs!["types", tn, "invariants", &name],
                            "names",
                            format!("{tn}.{name} is the invariant {b}.{a} holds of its value {vn}'s {i}, which {tn} inherits; name the type's invariant otherwise (ADR-0255)"),
                        ));
                    }
                }
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

//! A module's migration, checked against this version and the one before
//! it (`check-flows.py`'s `migration_errors`, `final_admission_notes` and
//! `migration_pair_errors`; `declaration-syntax.md` §6.6).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::doc::{
    Found, Obj, Path, ext, finding, flag, keys, list, obj, optional, py_str, re, reads, segs,
    str_or,
};
use super::names::invariant_names;
use super::notices::{kinds_named, taken_kinds};
use super::raw::imported;
use super::steps::sources;
use super::view::{View, types};

/// A backfill reads the object's own members and literals (§6.6).
const BACKFILL_READS: &[&str] = &["state", "this"];

/// Whether an attribute's change of type is a reference following a moved
/// type, which keeps its values (`check-flows.py`'s `follows_move`,
/// ADR-0229 decision 5).
fn follows_move(m: &Value, was: &Value, now: &Value) -> bool {
    let (before, after) = (str_or(was, "reference", ""), str_or(now, "reference", ""));
    !before.is_empty()
        && !after.is_empty()
        && before.ends_with("[]") == after.ends_with("[]")
        && obj(m, "moved_types").contains_key(before.trim_end_matches(['[', ']']))
}

/// Why a type does not move, or take moved objects, yet, or none
/// (`check-flows.py`'s `unmovable`, ADR-0229 decision 6).
fn unmovable(tn: &str, t: &Value, kinds: &Obj) -> Option<&'static str> {
    let attrs = obj(t, "attributes");
    let composite = |sp: &Value| super::doc::text(sp, "aggregation") == Some("composite");
    if flag(t, "mirror") {
        return Some("a mirror, which only the import writes");
    }
    if flag(t, "abstract") {
        return Some("an abstract type");
    }
    if attrs.values().any(|sp| flag(sp, "actor_kind")) {
        return Some("an actor's type, whose objects every event names as its actor");
    }
    if flag(t, "inherited_parts") || attrs.values().any(composite) {
        return Some("a whole in a composition, whose parts follow it");
    }
    if kinds.values().any(|k| {
        obj(k, "attributes").values().any(|sp| {
            composite(sp) && str_or(sp, "reference", "").trim_end_matches(['[', ']']) == tn
        })
    }) {
        return Some("a part in a composition, which follows its whole");
    }
    if obj(t, "states").values().any(|st| flag(st, "superseding")) {
        return Some(
            "a type with a superseding state, whose objects a supersession chain joins for erasure",
        );
    }
    if flag(t, "observations") {
        return Some("a type observations are recorded against, whose kinds it declares");
    }
    None
}

static QUALIFIED: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b([A-Z][A-Za-z0-9]*)\.([A-Z][A-Z0-9_]*)\b"));
static FAILS_ONLY_IN: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\s*state\s*!=\s*([A-Z][A-Z0-9_]*)\s+or\b"));
static GENERATED: LazyLock<Regex> = LazyLock::new(|| re(r"^([a-z0-9_]+)_invariant$"));
static ROUTED: LazyLock<Regex> =
    LazyLock::new(|| re(r"\.(from_state|to_state)\s*==\s*(\w+)\.(\w+)"));

fn mapping(m: &Value, k: &str) -> Vec<(String, Vec<(String, String)>)> {
    obj(m, k)
        .iter()
        .map(|(n, maps)| {
            (
                n.clone(),
                super::doc::as_obj(maps)
                    .iter()
                    .map(|(a, b)| (a.clone(), py_str(b)))
                    .collect(),
            )
        })
        .collect()
}

fn enums_of(doc: &Value) -> BTreeMap<String, BTreeSet<String>> {
    obj(doc, "enumerations")
        .iter()
        .map(|(n, x)| {
            (
                n.clone(),
                match x {
                    Value::Array(a) => a.iter().map(py_str).collect(),
                    _ => BTreeSet::new(),
                },
            )
        })
        .collect()
}

/// What a migration maps is checked against this version (§6.6).
pub fn migration_errors(doc: &Value, v: &Value) -> Found {
    let Some(m) = doc.get("migration").filter(|m| super::doc::truthy(Some(m))) else {
        return Found::new();
    };
    let machines = obj(doc, "machines");
    let kinds: Obj = types(v)
        .into_iter()
        .filter(|(tn, _)| !machines.contains_key(tn))
        .map(|(n, t)| (n, t.clone()))
        .collect();
    let enums = enums_of(doc);
    let imported = imported(doc);
    let mut out = Found::new();
    for (tn, maps) in mapping(m, "removed_states") {
        let at = segs!["migration", "removed_states", &tn];
        let Some(t) = kinds.get(&tn) else {
            out.push(finding(
                at,
                "names",
                format!("the migration moves objects of {tn}, which this module does not declare"),
            ));
            continue;
        };
        let states = obj(t, "states");
        for (old, new) in maps {
            let here = ext(&at, segs![&old]);
            if states.contains_key(&old) {
                out.push(finding(here.clone(), "names", format!("the migration maps {tn}.{old}, which this version still has; a mapping is for a state it removes")));
            }
            match states.get(&new) {
                None => out.push(finding(here, "names", format!("the migration moves {tn}.{old} to {new}, which is not a state of {tn}"))),
                Some(s) if flag(s, "superseding") => out.push(finding(
                    here,
                    "names",
                    format!("the migration moves {tn}.{old} to {new}, a superseding state, which would leave its objects superseded by nothing (ADR-0146)"),
                )),
                Some(_) => {}
            }
        }
    }
    for (en, maps) in mapping(m, "removed_members") {
        let at = segs!["migration", "removed_members", &en];
        let Some(members) = enums.get(&en) else {
            out.push(finding(
                at,
                "names",
                format!("the migration maps members of {en}, which this module does not declare"),
            ));
            continue;
        };
        for (old, new) in maps {
            let here = ext(&at, segs![&old]);
            if members.contains(&old) {
                out.push(finding(
                    here.clone(),
                    "names",
                    format!("the migration maps {en}.{old}, which this version still has"),
                ));
            }
            if !members.contains(&new) {
                out.push(finding(
                    here,
                    "names",
                    format!(
                        "the migration maps {en}.{old} to {new}, which is not a member of {en}"
                    ),
                ));
            }
        }
    }
    for (tn, maps) in mapping(m, "renamed_attributes") {
        let at = segs!["migration", "renamed_attributes", &tn];
        let Some(t) = kinds.get(&tn) else {
            out.push(finding(
                at,
                "names",
                format!(
                    "the migration renames attributes of {tn}, which this module does not declare"
                ),
            ));
            continue;
        };
        let attrs = obj(t, "attributes");
        for (old, new) in maps {
            let here = ext(&at, segs![&old]);
            if !attrs.contains_key(&new) {
                out.push(finding(
                    here.clone(),
                    "names",
                    format!(
                        "the migration renames {tn}.{old} to {new}, which {tn} does not declare"
                    ),
                ));
            }
            if attrs.contains_key(&old) {
                out.push(finding(
                    here,
                    "names",
                    format!("the migration renames {tn}.{old}, which this version still has"),
                ));
            }
        }
    }
    for (tn, maps) in mapping(m, "backfill") {
        let at = segs!["migration", "backfill", &tn];
        let Some(t) = kinds.get(&tn) else {
            out.push(finding(
                at,
                "names",
                format!("the migration backfills {tn}, which this module does not declare"),
            ));
            continue;
        };
        let mut members: BTreeSet<String> = keys(t, "attributes").into_iter().collect();
        members.extend(keys(t, "derived_attributes"));
        members.extend(BACKFILL_READS.iter().map(|s| s.to_string()));
        for (a, expr) in maps {
            let here = ext(&at, segs![&a]);
            if !obj(t, "attributes").contains_key(&a) {
                out.push(finding(
                    here.clone(),
                    "names",
                    format!(
                        "the migration backfills {tn}.{a}, which is not a stored attribute of {tn}"
                    ),
                ));
            }
            for r in reads(&expr).difference(&members) {
                out.push(finding(
                    here.clone(),
                    "names",
                    format!("the backfill of {tn}.{a} reads '{r}'; a backfill reads the object's own members and literals"),
                ));
            }
            for c in QUALIFIED.captures_iter(&expr) {
                let (owner, value) = (&c[1], &c[2]);
                if enums.get(owner).is_some_and(|e| !e.contains(value)) {
                    out.push(finding(here.clone(), "names", format!("the backfill of {tn}.{a} names {owner}.{value}, and {owner} has no {value}")));
                }
            }
        }
    }
    let moved = obj(m, "moved_types");
    let retired = obj(m, "retired_types");
    let mut both: Vec<&String> = moved
        .keys()
        .filter(|tn| retired.contains_key(*tn))
        .collect();
    both.sort();
    for tn in both {
        out.push(finding(
            segs!["migration", "retired_types", tn],
            "names",
            format!("the migration both moves and retires {tn}; its objects are moved or retired, not both"),
        ));
    }
    for (section, verb, names) in [
        ("moved_types", "moves", moved),
        ("retired_types", "retires", retired),
    ] {
        for tn in names.keys() {
            if kinds.contains_key(tn) {
                out.push(finding(
                    segs!["migration", section, tn],
                    "names",
                    format!("the migration {verb} {tn}, which this version still has; a type it removes is moved or retired (ADR-0229)"),
                ));
            }
        }
    }
    for (tn, entries) in moved {
        let entries = match entries {
            Value::Array(a) => a.as_slice(),
            _ => &[],
        };
        for (i, e) in entries.iter().enumerate() {
            let here = segs!["migration", "moved_types", tn, i];
            let to = str_or(e, "to", "");
            if entries.len() > 1 && !super::doc::has(e, "where") {
                out.push(finding(
                    here.clone(),
                    "names",
                    format!("an entry of {tn}'s moves without a where takes every object, so it is its type's only entry (ADR-0229)"),
                ));
            }
            let Some(t) = kinds.get(&to) else {
                out.push(finding(
                    ext(&here, segs!["to"]),
                    "names",
                    format!("the migration moves {tn} to {to}, which this module does not declare"),
                ));
                continue;
            };
            if let Some(why) = unmovable(&to, t, &kinds) {
                out.push(finding(
                    ext(&here, segs!["to"]),
                    "names",
                    format!("the migration moves {tn} to {to}, {why}; such a type takes no moved objects yet (ADR-0229)"),
                ));
            }
            for a in obj(e, "attributes").keys() {
                if !obj(t, "attributes").contains_key(a) {
                    out.push(finding(
                        ext(&here, segs!["attributes", a]),
                        "names",
                        format!("the move of {tn} to {to} gives {to}.{a}, which is not a stored attribute of {to}"),
                    ));
                }
            }
            let states = obj(t, "states");
            for (old, new) in obj(e, "states") {
                let new = py_str(new);
                match states.get(&new) {
                    None => out.push(finding(
                        ext(&here, segs!["states", old]),
                        "names",
                        format!("the move of {tn} to {to} maps {old} to {new}, which is not a state of {to}"),
                    )),
                    Some(st) if flag(st, "superseding") => out.push(finding(
                        ext(&here, segs!["states", old]),
                        "names",
                        format!("the move of {tn} to {to} maps {old} to {new}, a superseding state, which would leave its objects superseded by nothing (ADR-0146)"),
                    )),
                    Some(_) => {}
                }
            }
            for expr in e
                .get("where")
                .into_iter()
                .chain(obj(e, "attributes").values())
            {
                for c in QUALIFIED.captures_iter(&py_str(expr)) {
                    let (owner, value) = (&c[1], &c[2]);
                    if enums.get(owner).is_some_and(|en| !en.contains(value)) {
                        out.push(finding(
                            here.clone(),
                            "names",
                            format!("the move of {tn} to {to} names {owner}.{value}, and {owner} has no {value}"),
                        ));
                    }
                }
            }
        }
    }
    let observed: BTreeSet<String> = types(v)
        .into_iter()
        .flat_map(|(_, t)| {
            obj(t, "observations")
                .values()
                .map(|ob| str_or(ob, "kind", ""))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut admitted = BTreeSet::new();
    for (i, adm) in list(m, "admit").iter().enumerate() {
        let at = segs!["migration", "admit", i];
        let invariant = str_or(adm, "invariant", "");
        let (tn, inv) = invariant.split_once('.').unwrap_or((&invariant, ""));
        if admitted.contains(&invariant) {
            out.push(finding(at.clone(), "names", format!("the migration admits {invariant} twice; one change admits an invariant once (ADR-0147)")));
        }
        admitted.insert(invariant.clone());
        if observed.contains(tn) {
            out.push(finding(
                at.clone(),
                "names",
                format!("the migration admits {invariant}, an observation kind's invariant, which no migration reaches; its recorded observations are reported as a notice (ADR-0147)"),
            ));
        } else if let Some(t) = kinds.get(tn) {
            if !invariant_names(doc, tn, t).contains(inv) {
                out.push(finding(
                    at.clone(),
                    "names",
                    format!("the migration admits {invariant}, which is not an invariant of {tn}"),
                ));
            }
        } else {
            out.push(finding(
                at.clone(),
                "names",
                format!("the migration admits {invariant}, and this module declares no type {tn}"),
            ));
        }
        let reason = str_or(adm, "reason", "");
        let (en, member) = reason.split_once('.').unwrap_or((&reason, ""));
        match enums.get(en) {
            Some(members) if !members.contains(member) => out.push(finding(
                at,
                "names",
                format!("the migration admits {invariant} because {reason}, and {en} has no {member}"),
            )),
            Some(_) => {}
            None if !imported.contains(en) => out.push(finding(
                at,
                "names",
                format!("the migration's reason {reason} names {en}, which is not an enumeration of this module"),
            )),
            None => {}
        }
    }
    out
}

/// An admission of an invariant that can fail only in a final state can
/// never be discharged (ADR-0146).
pub fn final_admission_notes(doc: &Value, v: &Value) -> Found {
    let kinds = obj(v, "types");
    let mut out = Found::new();
    for (i, adm) in list(doc.get("migration").unwrap_or(&Value::Null), "admit")
        .iter()
        .enumerate()
    {
        let invariant = str_or(adm, "invariant", "");
        let (tn, inv) = invariant.split_once('.').unwrap_or((&invariant, ""));
        let t = kinds.get(tn).unwrap_or(&super::doc::NULL);
        let states = obj(t, "states");
        let expr = obj(t, "invariants")
            .get(inv)
            .map(|i| str_or(i, "expression", ""))
            .unwrap_or_default();
        let only = match FAILS_ONLY_IN.captures(&expr) {
            Some(c) => Some(c[1].to_string()),
            None => GENERATED
                .captures(inv)
                .map(|g| g[1].to_uppercase())
                .filter(|s| states.contains_key(s)),
        };
        if let Some(only) = only
            && states.get(&only).is_some_and(|s| flag(s, "final"))
        {
            out.push(finding(
                segs!["migration", "admit", i],
                "migration",
                format!("the migration admits {invariant}, which fails only in {only}, a final state no transition leaves: nothing can ever discharge the admission"),
            ));
        }
    }
    out
}

/// The transitions a filter comparing a field to a state can match.
fn route(kinds: &Obj, tn: &str, field: &str, st: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(t) = kinds.get(tn) else { return out };
    for (xn, x) in obj(t, "transitions") {
        let frm = sources(x);
        let to = x
            .get("to")
            .filter(|t| super::doc::truthy(Some(t)))
            .map(py_str);
        let into = field == "to_state" && to.as_deref() == Some(st);
        let leaving = field == "from_state"
            && to.as_deref().is_some_and(|to| to != st)
            && (frm.iter().any(|f| f == st) || frm.iter().any(|f| f == "any"));
        if into || leaving {
            out.insert(xn.clone());
        }
    }
    out
}

/// This version against the one before it (the model's checks 22 and 23),
/// as fatal findings and notices.
pub fn migration_pair_errors(prev: &Value, doc: &Value, library: &Obj) -> (Found, Found) {
    let pv = View::new(prev, library);
    let v = View::new(doc, library);
    let machines: BTreeSet<String> = keys(doc, "machines")
        .into_iter()
        .chain(keys(prev, "machines"))
        .collect();
    let old = obj(&pv.doc, "types").clone();
    let new = obj(&v.doc, "types").clone();
    let m = doc.get("migration").cloned().unwrap_or(Value::Null);
    let (mut fatal, mut notes) = (Found::new(), Found::new());
    // `(m.get(section) or {}).get(name, {})`, its values as text
    let as_map = |section: &str, name: &str| -> BTreeMap<String, String> {
        super::doc::as_obj(obj(&m, section).get(name).unwrap_or(&Value::Null))
            .iter()
            .map(|(a, b)| (a.clone(), py_str(b)))
            .collect()
    };
    for (tn, t) in &new {
        if machines.contains(tn) || flag(t, "abstract") {
            continue;
        }
        let Some(ot) = old.get(tn) else { continue };
        let here = segs!["types", tn];
        let mapped = as_map("removed_states", tn);
        let old_states: BTreeSet<String> = keys(ot, "states").into_iter().collect();
        let new_states: BTreeSet<String> = keys(t, "states").into_iter().collect();
        for st in &old_states {
            if !new_states.contains(st) && !mapped.contains_key(st) {
                fatal.push(finding(
                    here.clone(),
                    "names",
                    format!("{tn} no longer has the state {st}, and the migration maps it nowhere; its objects need a state to move to"),
                ));
            }
        }
        for st in mapped.keys() {
            if !old_states.contains(st) {
                fatal.push(finding(
                    segs!["migration", "removed_states", tn, st],
                    "names",
                    format!(
                        "the migration maps {tn}.{st}, which the previous version did not have"
                    ),
                ));
            }
        }
        let renamed = as_map("renamed_attributes", tn);
        let old_attrs = obj(ot, "attributes");
        let new_attrs = obj(t, "attributes");
        for a in renamed.keys() {
            if !old_attrs.contains_key(a) {
                fatal.push(finding(
                    segs!["migration", "renamed_attributes", tn, a],
                    "names",
                    format!(
                        "the migration renames {tn}.{a}, which the previous version did not have"
                    ),
                ));
            }
        }
        let sig = |s: &Value| {
            let k = match s.get("type") {
                Some(t) => py_str(t),
                None => s
                    .get("reference")
                    .map_or_else(|| "None".to_string(), py_str),
            };
            (k, optional(s))
        };
        for (a, b) in &renamed {
            let (Some(before), Some(after)) = (
                old_attrs.get(a).filter(|x| super::doc::truthy(Some(x))),
                new_attrs.get(b).filter(|x| super::doc::truthy(Some(x))),
            ) else {
                continue;
            };
            if sig(before) != sig(after) {
                fatal.push(finding(
                    segs!["migration", "renamed_attributes", tn, a],
                    "names",
                    format!("the migration renames {tn}.{a} to {b}, which changes its type or whether it is optional; a rename carries the values and their history unchanged (ADR-0146)"),
                ));
            }
        }
        let renamed_to: BTreeSet<String> = renamed.values().cloned().collect();
        let added: BTreeSet<String> = new_attrs
            .keys()
            .filter(|a| !old_attrs.contains_key(*a) && !renamed_to.contains(*a))
            .cloned()
            .collect();
        let dropped: BTreeSet<String> = old_attrs
            .keys()
            .filter(|a| !new_attrs.contains_key(*a) && !renamed.contains_key(*a))
            .cloned()
            .collect();
        let filled = as_map("backfill", tn);
        for a in &added {
            let spec = &new_attrs[a];
            let k = match spec.get("reference") {
                Some(r) => py_str(r),
                None => str_or(spec, "type", ""),
            };
            if optional(spec) || flag(spec, "identifier") || k.ends_with("[]") {
                continue;
            }
            if super::doc::text(spec, "aggregation") == Some("composite") {
                notes.push(finding(
                    ext(&here, segs!["attributes", a]),
                    "migration",
                    format!("{tn}.{a} is a new required part: a publish over live objects of {tn} is refused, so declare it optional or a set"),
                ));
            } else if !filled.contains_key(a) {
                notes.push(finding(
                    ext(&here, segs!["attributes", a]),
                    "migration",
                    format!("{tn}.{a} is new and required: a publish over live objects of {tn} needs a backfill for it"),
                ));
            }
        }
        let type_of = |s: &Value| match s.get("type") {
            Some(t) => py_str(t),
            None => s
                .get("reference")
                .map_or_else(|| "None".to_string(), py_str),
        };
        for a in new_attrs
            .keys()
            .filter(|a| old_attrs.contains_key(*a))
            .collect::<BTreeSet<_>>()
        {
            let (now_spec, was_spec) = (&new_attrs[a], &old_attrs[a]);
            if type_of(now_spec) != type_of(was_spec) && !follows_move(&m, was_spec, now_spec) {
                notes.push(finding(
                    ext(&here, segs!["attributes", a]),
                    "migration",
                    format!("{tn}.{a} changed its type: that is a drop and an add of the same name, every object's value starting absent (ADR-0147)"),
                ));
            } else if optional(was_spec)
                && !optional(now_spec)
                && !filled.contains_key(a)
                && !now_spec
                    .get("reference")
                    .map(py_str)
                    .unwrap_or_default()
                    .ends_with("[]")
            {
                notes.push(finding(
                    ext(&here, segs!["attributes", a]),
                    "migration",
                    format!("{tn}.{a} is now required, which counts as a new required attribute: a publish over objects lacking it needs a backfill (ADR-0147)"),
                ));
            }
        }
        for d in &dropped {
            for a in &added {
                let pair = |s: &Value| (s.get("type").cloned(), s.get("reference").cloned());
                if pair(&old_attrs[d]) == pair(&new_attrs[a]) {
                    notes.push(finding(
                        ext(&here, segs!["attributes", a]),
                        "migration",
                        format!("{tn}.{d} is gone and {tn}.{a} is new, of the same type: if it is a rename, map it, or its values stay behind"),
                    ));
                }
            }
        }
        let held_personal = old_attrs.values().any(|sp| flag(sp, "personal"))
            || obj(ot, "transitions")
                .values()
                .any(|x| obj(x, "inputs").values().any(|sp| flag(sp, "personal")));
        if held_personal
            && !obj(t, "transitions")
                .values()
                .any(|x| super::steps::kind(x) == "erasure")
        {
            fatal.push(finding(
                here.clone(),
                "names",
                format!("{tn} held personal data in the version before and now declares no erasure; a type keeps its erasure once it has needed one, so the values written then can be erased (the model's check 60, ADR-0147)"),
            ));
        }
        for (o, ob) in obj(t, "observations") {
            let Some(before) = obj(ot, "observations")
                .get(o)
                .and_then(|b| b.get("attributes"))
                .filter(|b| !b.is_null())
            else {
                continue;
            };
            for (fld, spec) in obj(ob, "attributes") {
                if !super::doc::as_obj(before).contains_key(fld) && !optional(spec) {
                    fatal.push(finding(
                        ext(&here, segs!["observations", o, "attributes", fld]),
                        "names",
                        format!("{o}.{fld} is added to an observation kind and is required; an observation is born final, so nothing could fill it"),
                    ));
                }
            }
        }
    }
    // a type the version removes: moved, retired, or noticed (ADR-0229)
    let moved = obj(&m, "moved_types").clone();
    let retired = obj(&m, "retired_types").clone();
    let mut gone: Vec<&String> = old
        .keys()
        .filter(|tn| !new.contains_key(*tn) && !machines.contains(*tn))
        .collect();
    gone.sort();
    for tn in gone {
        if !moved.contains_key(tn) && !retired.contains_key(tn) && !flag(&old[tn], "abstract") {
            notes.push(finding(
                segs!["types"],
                "migration",
                format!("{tn} is gone from this module: unless another module declares it, a publish over its objects not in a final state needs moved_types or retired_types for it (ADR-0229)"),
            ));
        }
    }
    for (section, names) in [("moved_types", &moved), ("retired_types", &retired)] {
        for tn in names.keys() {
            if !old.contains_key(tn) {
                notes.push(finding(
                    segs!["migration", section, tn],
                    "migration",
                    format!("{tn} is not a type of this module's previous version, so the publish checks it against every module of the version before (ADR-0229)"),
                ));
            }
        }
    }
    for (tn, entries) in &moved {
        let Some(ot) = old.get(tn) else { continue };
        if let Some(why) = unmovable(tn, ot, &old) {
            fatal.push(finding(
                segs!["migration", "moved_types", tn],
                "names",
                format!(
                    "the migration moves {tn}, {why}; such a type does not move yet (ADR-0229)"
                ),
            ));
        }
        let oattrs = obj(ot, "attributes");
        let mut members: BTreeSet<String> = oattrs.keys().cloned().collect();
        members.extend(keys(ot, "derived_attributes"));
        members.extend(BACKFILL_READS.iter().map(|r| r.to_string()));
        let personal: BTreeSet<String> = oattrs
            .iter()
            .filter(|(_, sp)| flag(sp, "personal"))
            .map(|(a, _)| a.clone())
            .collect();
        let entries = match entries {
            Value::Array(a) => a.as_slice(),
            _ => &[],
        };
        for (i, e) in entries.iter().enumerate() {
            let here = segs!["migration", "moved_types", tn, i];
            let to = str_or(e, "to", "");
            let given = obj(e, "attributes");
            for expr in e.get("where").into_iter().chain(given.values()) {
                for r in super::doc::reads(&py_str(expr)).difference(&members) {
                    fatal.push(finding(
                        here.clone(),
                        "names",
                        format!("the move of {tn} to {to} reads '{r}'; a move reads the old object's own members and literals"),
                    ));
                }
            }
            let ostates = obj(ot, "states");
            let mut unknown: Vec<String> = keys(e, "states")
                .into_iter()
                .filter(|st| !ostates.contains_key(st))
                .collect();
            unknown.sort();
            for st in unknown {
                fatal.push(finding(
                    ext(&here, segs!["states", &st]),
                    "names",
                    format!("the move of {tn} to {to} maps {tn}.{st}, which the previous version did not have"),
                ));
            }
            let Some(t) = new.get(&to) else { continue };
            let (was, now) = (
                str_or(ot, "tracking", "None"),
                str_or(t, "tracking", "None"),
            );
            if was != now {
                fatal.push(finding(
                    ext(&here, segs!["to"]),
                    "names",
                    format!("the migration moves {tn}, tracked by {was}, to {to}, tracked by {now}; a move keeps its tracking (ADR-0229)"),
                ));
            }
            let tattrs = obj(t, "attributes");
            let mut gives: Vec<(&String, &Value)> = given.iter().collect();
            gives.sort_by(|x, y| x.0.cmp(y.0));
            for (a, x) in gives {
                let leaked: Vec<String> = super::doc::reads(&py_str(x))
                    .intersection(&personal)
                    .cloned()
                    .collect();
                if !leaked.is_empty() && !tattrs.get(a).is_some_and(|sp| flag(sp, "personal")) {
                    fatal.push(finding(
                        ext(&here, segs!["attributes", a]),
                        "names",
                        format!("the move of {tn} to {to} gives {to}.{a}, which is not personal, from {}, personal in {tn}; a personal value reaches no attribute that is not (the model's check 10)", leaked.join(", ")),
                    ));
                }
            }
            let mut targets: Vec<(&String, &Value)> = tattrs.iter().collect();
            targets.sort_by(|x, y| x.0.cmp(y.0));
            let pair = |sp: &Value| (sp.get("type").cloned(), sp.get("reference").cloned());
            for (a, spec) in targets {
                let Some(was) = oattrs.get(a).filter(|w| super::doc::truthy(Some(w))) else {
                    continue;
                };
                if given.contains_key(a) {
                    continue;
                }
                if pair(was) == pair(spec) && flag(was, "personal") && !flag(spec, "personal") {
                    fatal.push(finding(
                        here.clone(),
                        "names",
                        format!("the move of {tn} to {to} carries {a}, personal in {tn}, into {to}.{a}, which is not; a personal value reaches no attribute that is not (the model's check 10)"),
                    ));
                }
            }
            let (tstates, mapped) = (obj(t, "states"), obj(e, "states"));
            let mut left: Vec<String> = ostates
                .keys()
                .filter(|st| !mapped.contains_key(*st) && !tstates.contains_key(*st))
                .cloned()
                .collect();
            left.sort();
            for st in left {
                notes.push(finding(
                    here.clone(),
                    "migration",
                    format!("{tn}.{st} has no state of its name in {to}, and the entry maps it nowhere: a publish over objects of {tn} in it is refused"),
                ));
            }
        }
    }
    let mut declared: Vec<(Path, Value)> = Vec::new();
    for (tn, t) in &new {
        for (mn, mm) in obj(t, "metrics") {
            declared.push((segs!["types", tn, "metrics", mn], mm.clone()));
        }
    }
    for (mn, mm) in obj(doc, "metrics") {
        declared.push((segs!["metrics", mn], mm.clone()));
    }
    for (at, mm) in declared {
        if str_or(&mm, "source", "") != "transitions" {
            continue;
        }
        let filter = mm
            .get("filter")
            .filter(|f| super::doc::truthy(Some(f)))
            .map(py_str)
            .unwrap_or_default();
        let found: BTreeSet<(String, String, String)> = ROUTED
            .captures_iter(&filter)
            .map(|c| (c[1].to_string(), c[2].to_string(), c[3].to_string()))
            .collect();
        for (field, owner, st) in found {
            if old.contains_key(&owner)
                && new.contains_key(&owner)
                && route(&old, &owner, &field, &st) != route(&new, &owner, &field, &st)
            {
                let way = if field == "to_state" {
                    "into"
                } else {
                    "out of"
                };
                let name = match at.last() {
                    Some(crate::yaml::Seg::Key(k)) => k.clone(),
                    _ => String::new(),
                };
                notes.push(finding(
                    at.clone(),
                    "metric",
                    format!("{name}'s filter counts transitions {way} {owner}.{st}, and this version changes which transitions those are, so what the metric counts changes from this version on"),
                ));
            }
        }
    }
    let old_enums = enums_of(prev);
    let new_enums = enums_of(doc);
    for (en, members) in &new_enums {
        let Some(was) = old_enums.get(en) else {
            continue;
        };
        let mapped = as_map("removed_members", en);
        for x in was {
            if !members.contains(x) && !mapped.contains_key(x) {
                notes.push(finding(
                    segs!["enumerations", en],
                    "migration",
                    format!("{en}.{x} is gone: a publish needs a mapping for it if live objects hold it, and is refused if an observation does"),
                ));
            }
        }
        for x in mapped.keys() {
            if !was.contains(x) {
                fatal.push(finding(
                    segs!["migration", "removed_members", en, x],
                    "names",
                    format!("the migration maps {en}.{x}, which the previous version did not have"),
                ));
            }
        }
    }
    // who may take a transition, or approve a version, named no longer or
    // widened (ADR-0189 decision 1)
    for (tn, t) in &new {
        if machines.contains(tn) || flag(t, "abstract") {
            continue;
        }
        let Some(ot) = old.get(tn) else { continue };
        for (xn, ox) in obj(ot, "transitions") {
            let Some(x) = obj(t, "transitions").get(xn) else {
                continue;
            };
            if !x.is_object() || !ox.is_object() {
                continue;
            }
            let (was, now) = (taken_kinds(ox, ot), taken_kinds(x, t));
            if let Some(message) = narrowed_no_longer(&format!("{tn}.{xn}"), &was, &now, true) {
                notes.push(finding(
                    segs!["types", tn.as_str(), "transitions", xn.as_str()],
                    "taken_by_dropped",
                    message,
                ));
            }
        }
    }
    // an attribute no longer personal, whose values an erasure then no longer
    // reaches (ninth production review N-11, ADR-0196 decision 6)
    for (tn, t) in &new {
        if machines.contains(tn) || flag(t, "abstract") {
            continue;
        }
        let Some(ot) = old.get(tn) else { continue };
        for (an, oa) in obj(ot, "attributes") {
            let Some(a) = obj(t, "attributes").get(an) else {
                continue;
            };
            let personal = |v: &Value| v.get("personal") == Some(&Value::Bool(true));
            if oa.is_object() && a.is_object() && personal(oa) && !personal(a) {
                notes.push(finding(
                    segs!["types", tn.as_str(), "attributes", an.as_str()],
                    "personal_dropped",
                    format!(
                        "{tn}.{an} was personal, and this version drops it: an erasure no longer reaches its values, which stay where they are (ADR-0196)"
                    ),
                ));
            }
        }
    }
    // the rules a version removes or rewrites, named to whoever approves it,
    // so a draft making a guard always true is seen (fifth production review
    // REVIEW-GAP, ADR-0191 decision 2)
    for (tn, t) in &new {
        if machines.contains(tn) || flag(t, "abstract") {
            continue;
        }
        let Some(ot) = old.get(tn) else { continue };
        let said = rules_changed(ot, t);
        if !said.is_empty() {
            notes.push(finding(
                segs!["types", tn.as_str()],
                "rule_changed",
                format!(
                    "{tn}'s rules change against the previous version: {}; read each before approving (ADR-0191)",
                    said.join("; ")
                ),
            ));
        }
    }
    let approving = |d: &Value| {
        let mut kinds = kinds_named(d.get("publishing").and_then(|p| p.get("taken_by")));
        kinds.sort();
        kinds
    };
    let (was, now) = (approving(prev), approving(doc));
    if let Some(message) = narrowed_no_longer("publishing.taken_by", &was, &now, false) {
        notes.push(finding(segs!["publishing"], "taken_by_dropped", message));
    } else if was.is_empty() && now.iter().any(|k| k == "agent") {
        // named where none was, the default refusing agents (ninth production
        // review N-1, ADR-0196 decision 5)
        notes.push(finding(
            segs!["publishing"],
            "taken_by_dropped",
            format!(
                "publishing.taken_by is [{}], where the previous version declared none, under which no agent approves: once it is installed an agent may approve the next (ADR-0196)",
                now.join(", ")
            ),
        ));
    }
    (fatal, notes)
}

/// What a type's rules lose against the previous version, each a phrase:
/// its conditions and invariants removed or rewritten, and its transitions'
/// guards that refused and no longer do; none where nothing changes. What
/// `taken_by` generates is `taken_by_dropped`'s to say.
fn rules_changed(ot: &Value, t: &Value) -> Vec<String> {
    let text = |spec: &Value| -> String {
        if !spec.is_object() {
            return String::new();
        }
        spec.get("expression")
            .filter(|e| !e.is_null())
            .map(py_str)
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let own = |d: &Value, key: &str| -> BTreeMap<String, Value> {
        obj(d, key)
            .iter()
            .filter(|(_, x)| x.get(super::view::TAKEN).is_none())
            .map(|(n, x)| (n.clone(), x.clone()))
            .collect()
    };
    let mut out = Vec::new();
    for key in ["conditions", "invariants"] {
        let (before, after) = (own(ot, key), own(t, key));
        let gone: Vec<&str> = before
            .keys()
            .filter(|n| !after.contains_key(*n))
            .map(String::as_str)
            .collect();
        let rewritten: Vec<&str> = before
            .iter()
            .filter(|(n, x)| after.get(*n).is_some_and(|y| text(x) != text(y)))
            .map(|(n, _)| n.as_str())
            .collect();
        if !gone.is_empty() {
            out.push(format!("{key} removed: {}", gone.join(", ")));
        }
        if !rewritten.is_empty() {
            out.push(format!("{key} rewritten: {}", rewritten.join(", ")));
        }
    }
    let mut lowered = Vec::new();
    let transitions: BTreeMap<&String, &Value> = obj(ot, "transitions").iter().collect();
    for (xn, ox) in transitions {
        let Some(x) = obj(t, "transitions").get(xn.as_str()) else {
            continue;
        };
        if !ox.is_object() || !x.is_object() {
            continue;
        }
        let guards: BTreeMap<&String, &Value> = obj(ox, "guards").iter().collect();
        for (g, mode) in guards {
            if mode.as_str() == Some("deny")
                && obj(x, "guards").get(g.as_str()).and_then(Value::as_str) != Some("deny")
            {
                lowered.push(format!("{xn}.{g}"));
            }
        }
    }
    if !lowered.is_empty() {
        out.push(format!("guards no longer refusing: {}", lowered.join(", ")));
    }
    out
}

/// What a version says of who may act, where the previous one named kinds
/// and this one names none or adds one: of a transition, who may take it, or
/// of `publishing`, who may approve the next version (ADR-0189 decision 1).
fn narrowed_no_longer(what: &str, was: &[String], now: &[String], taking: bool) -> Option<String> {
    if was.is_empty() {
        return None;
    }
    if now.is_empty() {
        let after = if taking {
            "every kind of actor may take it"
        } else {
            "once it is installed any kind but agent may approve the next"
        };
        let verb = if taking { " was taken by" } else { " was" };
        return Some(format!(
            "{what}{verb} [{}], and this version names no kind, so {after} (ADR-0189)",
            was.join(", ")
        ));
    }
    if now.iter().any(|k| !was.contains(k)) {
        let verb = if taking { " is taken by" } else { " is" };
        return Some(format!(
            "{what}{verb} [{}], where the previous version allowed [{}] (ADR-0189)",
            now.join(", "),
            was.join(", ")
        ));
    }
    None
}

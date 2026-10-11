//! A module as step 3 of the flow checker sees it (`check-flows.py`'s
//! `view`): each machine as a type, each binder merged with its machine,
//! `from: any` resolved, and each subtype with what its bases give it; and
//! the way back from a path of the view to the YAML it stands for.

use serde_json::{Map, Value, json};

use super::doc::{Found, Obj, Path, as_obj, flag, obj, py_str, text};
use crate::yaml::Seg;

/// What a subtype inherits: these members of its bases.
pub const INHERITED: &[&str] = &["attributes", "derived_attributes", "invariants"];

/// The key under which a view's type records where each inherited member
/// is declared, as `_from` does.
const FROM: &str = "_from";

/// The types of a module, in the order written.
pub fn types(doc: &Value) -> Vec<(String, &Value)> {
    obj(doc, "types")
        .iter()
        .map(|(n, t)| (n.clone(), t))
        .collect()
}

/// A machine, checked as a type whose attributes are those it requires.
fn machine_as_type(m: &Value) -> Value {
    json!({
        "description": m.get("description").cloned().unwrap_or(Value::Null),
        "tracking": "record",
        "attributes": Value::Object(obj(m.get("requires").unwrap_or(&Value::Null), "attributes").clone()),
        "states": m.get("states").cloned().unwrap_or(Value::Null),
        "conditions": Value::Object(obj(m, "conditions").clone()),
        "transitions": m.get("transitions").cloned().unwrap_or(Value::Null),
    })
}

/// A type with the machine it binds merged in: the machine's states, its
/// conditions, and its transitions, of which the initial ones give way to
/// a binder's own (`declaration-syntax.md` §2.1).
fn bound(t: &Value, machines: &Obj) -> Value {
    let Some(m) = text(t, "state_machine").and_then(|n| machines.get(n)) else {
        return t.clone();
    };
    let own = obj(t, "transitions");
    let own_initial = own.values().any(|x| text(x, "kind") == Some("initial"));
    let mut merged: Obj = obj(m, "transitions")
        .iter()
        .filter(|(_, v)| !(own_initial && text(v, "kind") == Some("initial")))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (k, v) in own {
        merged.insert(k.clone(), v.clone());
    }
    let mut conditions = obj(m, "conditions").clone();
    for (k, v) in obj(t, "conditions") {
        conditions.insert(k.clone(), v.clone());
    }
    let mut out = as_obj(t).clone();
    out.insert(
        "states".into(),
        m.get("states").cloned().unwrap_or(Value::Null),
    );
    out.insert("transitions".into(), Value::Object(merged));
    out.insert("conditions".into(), Value::Object(conditions));
    Value::Object(out)
}

/// `from: any` is every state that is not final, and for an external
/// transition every one but its target (`declaration-syntax.md` §4.2).
fn resolve_any(x: &Value, states: Option<&Value>) -> Value {
    if text(x, "from") != Some("any") {
        return x.clone();
    }
    let to = x.get("to");
    let from: Vec<Value> = states
        .map(as_obj)
        .into_iter()
        .flatten()
        .filter(|(s, v)| !flag(v, "final") && to.and_then(Value::as_str) != Some(s.as_str()))
        .map(|(s, _)| Value::String(s.clone()))
        .collect();
    let mut out = as_obj(x).clone();
    out.insert("from".into(), Value::Array(from));
    Value::Object(out)
}

/// On a member a transition's `taken_by` generates: the transition
/// (ADR-0176).
pub(super) const TAKEN: &str = "_taken_by";

/// A transition's `taken_by`, its kinds of actor, where it has one.
fn taken_kinds(x: &Value) -> Option<Vec<String>> {
    let kinds: Vec<String> = x.get("taken_by")?.as_array()?.iter().map(py_str).collect();
    (!kinds.is_empty()).then_some(kinds)
}

/// A type with what its transitions' `taken_by` generate (ADR-0176 decision
/// 1), as `check-flows.py`'s `with_taken_by`: for each, an optional
/// attribute of type `actor_kind` that its effect first assigns
/// `actor.kind`, and an invariant holding that attribute absent or one of
/// the kinds named, each after the type's own. A name the type declares
/// itself is left as it is declared, which `taken_by_errors` reports.
fn with_taken_by(t: Value) -> Value {
    let marked: Vec<(String, Vec<String>)> = obj(&t, "transitions")
        .iter()
        .filter_map(|(xn, x)| taken_kinds(x).map(|k| (xn.clone(), k)))
        .collect();
    if marked.is_empty() {
        return t;
    }
    let mut attributes = obj(&t, "attributes").clone();
    let mut invariants = obj(&t, "invariants").clone();
    let mut transitions = obj(&t, "transitions").clone();
    for (xn, kinds) in marked {
        let (a, i) = crate::model::taken_by_names(&xn);
        if !attributes.contains_key(&a) {
            let mut spec = Map::new();
            spec.insert("type".into(), json!("actor_kind"));
            spec.insert("optional".into(), json!(true));
            spec.insert(TAKEN.into(), json!(xn));
            spec.insert(
                "description".into(),
                json!(format!(
                    "The kind of actor that took {xn}, recorded as it was taken."
                )),
            );
            attributes.insert(a.clone(), Value::Object(spec));
        }
        if !invariants.contains_key(&i) {
            let expression = std::iter::once(format!("{a} is null"))
                .chain(kinds.iter().map(|k| format!("{a} == {k}")))
                .collect::<Vec<_>>()
                .join(" or ");
            let mut spec = Map::new();
            spec.insert("expression".into(), json!(expression));
            spec.insert(TAKEN.into(), json!(xn));
            spec.insert(
                "description".into(),
                json!(format!("{xn} is taken by {} only.", kinds.join(" or "))),
            );
            invariants.insert(i, Value::Object(spec));
        }
        if let Some(x) = transitions.get_mut(&xn).and_then(Value::as_object_mut) {
            let mut effect = vec![json!({"assign": {"location": a, "expr": "actor.kind"}})];
            effect.extend(
                x.get("effect")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            );
            x.insert("effect".into(), Value::Array(effect));
        }
    }
    let mut out = as_obj(&t).clone();
    out.insert("attributes".into(), Value::Object(attributes));
    out.insert("invariants".into(), Value::Object(invariants));
    out.insert("transitions".into(), Value::Object(transitions));
    Value::Object(out)
}

/// A transition marked `taken_by` is one a request takes, with an effect:
/// an initial, external or internal one; and what it generates is not a
/// name its type declares (ADR-0176 decision 1). Over the view.
/// No attribute of a type is named `<reference>_id` beside a single
/// reference of that name, its own or inherited: the store keeps the
/// reference in a column of that name, so it could not store the type
/// (`storage-schema.md` §3.2, D578; `check-flows.py`'s `column_errors`).
pub fn column_errors(doc: &Value) -> Found {
    let mut out = Found::new();
    for (tn, t) in types(doc) {
        let attrs = obj(t, "attributes");
        for a in attrs.keys() {
            let Some(base) = a.strip_suffix("_id") else {
                continue;
            };
            let single = attrs
                .get(base)
                .and_then(|r| r.get("reference"))
                .and_then(Value::as_str)
                .is_some_and(|r| !r.ends_with("[]"));
            if single {
                out.push((
                    ["types", tn.as_str(), "attributes", a.as_str()]
                        .iter()
                        .map(|k| Seg::Key((*k).to_string()))
                        .collect(),
                    "names".into(),
                    format!(
                        "{tn}.{a} is the column the store keeps the reference {base} in; name the attribute otherwise (storage-schema.md §3.2, D578)"
                    ),
                ));
            }
        }
    }
    out
}

pub fn taken_by_errors(doc: &Value) -> Found {
    let mut out = Found::new();
    for (tn, t) in types(doc) {
        for (xn, x) in obj(t, "transitions") {
            if taken_kinds(x).is_none() {
                continue;
            }
            let at: Path = ["types", tn.as_str(), "transitions", xn.as_str(), "taken_by"]
                .iter()
                .map(|k| Seg::Key((*k).to_string()))
                .collect();
            let kind = text(x, "kind").unwrap_or("None");
            if !["initial", "external", "internal", "assertion", "erasure"].contains(&kind) {
                out.push((
                    at.clone(),
                    "names".into(),
                    format!(
                        "{tn}.{xn} is an {kind} transition, which taken_by does not mark; it marks an initial, external, internal, assertion or erasure one (ADR-0176, D579)"
                    ),
                ));
            }
            let (a, i) = crate::model::taken_by_names(xn);
            for (section, n, what) in [
                ("attributes", a, "attribute"),
                ("invariants", i, "invariant"),
            ] {
                if let Some(spec) = obj(t, section).get(&n)
                    && spec.is_object()
                    && spec.get(TAKEN).and_then(Value::as_str) != Some(xn.as_str())
                {
                    out.push((
                        at.clone(),
                        "names".into(),
                        format!(
                            "{tn}.{xn}'s taken_by generates the {what} {n}, which {tn} declares itself (ADR-0176)"
                        ),
                    ));
                }
            }
        }
    }
    out
}

/// The bases a type extends, nearest first; the walk stops at an unknown
/// name or where it would come back to a type it has passed.
pub fn bases(tn: &str, kinds: &Obj) -> Vec<String> {
    let mut chain = Vec::new();
    let mut seen = vec![tn.to_string()];
    let mut base = kinds
        .get(tn)
        .and_then(|t| text(t, "extends"))
        .map(str::to_string);
    while let Some(b) = base {
        if b.is_empty() || seen.contains(&b) || !kinds.contains_key(&b) {
            break;
        }
        chain.push(b.clone());
        seen.push(b.clone());
        base = kinds
            .get(&b)
            .and_then(|t| text(t, "extends"))
            .map(str::to_string);
    }
    chain
}

/// A type with what its bases give it: their attributes, derived attributes
/// and invariants, and their tracking, each inherited member's base noted.
fn inherit(tn: &str, t: &Value, kinds: &Obj) -> Value {
    let chain = bases(tn, kinds);
    if chain.is_empty() {
        return t.clone();
    }
    let mut merged = as_obj(t).clone();
    let mut origin: Obj = Map::new();
    for key in INHERITED {
        let mut members: Obj = Map::new();
        for b in chain.iter().rev() {
            for (n, spec) in obj(&kinds[b], key) {
                members.insert(n.clone(), spec.clone());
                origin.insert(from_key(key, n), Value::String(b.clone()));
            }
        }
        for (n, spec) in obj(t, key) {
            origin.shift_remove(&from_key(key, n));
            members.insert(n.clone(), spec.clone());
        }
        merged.insert((*key).to_string(), Value::Object(members));
    }
    if !merged.contains_key("tracking")
        && let Some(tracking) = chain.iter().find_map(|b| kinds[b].get("tracking"))
        && super::doc::truthy(Some(tracking))
    {
        merged.insert("tracking".into(), tracking.clone());
    }
    merged.insert(FROM.into(), Value::Object(origin));
    Value::Object(merged)
}

fn from_key(key: &str, name: &str) -> String {
    format!("{key}\u{1f}{name}")
}

/// Whether a view's type inherits a member rather than declaring it.
pub fn inherited(t: &Value, key: &str, name: &str) -> bool {
    obj(t, FROM).contains_key(&from_key(key, name))
}

/// A module as the checks see it, and the YAML each view path stands for.
pub struct View {
    pub doc: Value,
    machines: Vec<String>,
    raw_types: Obj,
}

impl View {
    pub fn new(doc: &Value, library: &Obj) -> View {
        let machines = obj(doc, "machines");
        let mut kinds: Obj = machines
            .iter()
            .map(|(n, m)| (n.clone(), machine_as_type(m)))
            .collect();
        for (tn, t) in obj(doc, "types") {
            kinds.insert(tn.clone(), bound(t, machines));
        }
        let kinds: Obj = kinds
            .into_iter()
            .map(|(tn, t)| {
                let transitions: Obj = obj(&t, "transitions")
                    .iter()
                    .map(|(xn, x)| (xn.clone(), resolve_any(x, t.get("states"))))
                    .collect();
                let mut t = as_obj(&t).clone();
                t.insert("transitions".into(), Value::Object(transitions));
                (tn, with_taken_by(Value::Object(t)))
            })
            .collect();
        let mut everything = library.clone();
        for (k, v) in &kinds {
            everything.insert(k.clone(), v.clone());
        }
        let kinds: Obj = kinds
            .iter()
            .map(|(tn, t)| (tn.clone(), inherit(tn, t, &everything)))
            .collect();
        let mut v = as_obj(doc).clone();
        v.insert("types".into(), Value::Object(kinds));
        View {
            doc: Value::Object(v),
            machines: machines.keys().cloned().collect(),
            raw_types: obj(doc, "types").clone(),
        }
    }

    /// The YAML path a view path stands for, or none where it is a
    /// machine's part of a binder, reported at the machine, or a member a
    /// subtype inherits, reported at the base that declares it.
    pub fn back(&self, path: &[Seg]) -> Option<Path> {
        let (Some(Seg::Key(first)), Some(Seg::Key(name))) = (path.first(), path.get(1)) else {
            return Some(path.to_vec());
        };
        if first != "types" {
            return Some(path.to_vec());
        }
        let kind = obj(&self.doc, "types")
            .get(name)
            .cloned()
            .unwrap_or(Value::Null);
        if let (Some(Seg::Key(k)), Some(Seg::Key(n))) = (path.get(2), path.get(3))
            && inherited(&kind, k, n)
        {
            return None;
        }
        if self.machines.contains(name) {
            let mut out: Path = vec![Seg::Key("machines".into()), Seg::Key(name.clone())];
            if matches!(path.get(2), Some(Seg::Key(k)) if k == "attributes") {
                out.push(Seg::Key("requires".into()));
            }
            out.extend(path[2..].iter().cloned());
            return Some(out);
        }
        let raw = self.raw_types.get(name).cloned().unwrap_or(Value::Null);
        if flag(&raw, "state_machine")
            && let (Some(Seg::Key(section)), Some(Seg::Key(member))) = (path.get(2), path.get(3))
            && ["states", "transitions", "conditions"].contains(&section.as_str())
            && !obj(&raw, section).contains_key(member)
        {
            return None;
        }
        Some(path.to_vec())
    }
}

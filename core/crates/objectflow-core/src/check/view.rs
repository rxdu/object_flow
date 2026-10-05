//! A module as step 3 of the flow checker sees it (`check-flows.py`'s
//! `view`): each machine as a type, each binder merged with its machine,
//! `from: any` resolved, and each subtype with what its bases give it; and
//! the way back from a path of the view to the YAML it stands for.

use serde_json::{Map, Value, json};

use super::doc::{Obj, Path, as_obj, flag, obj, text};
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
                (tn, Value::Object(t))
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

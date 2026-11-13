//! A module as step 3 of the flow checker sees it (`check-flows.py`'s
//! `view`): each machine as a type, each binder merged with its machine,
//! `from: any` resolved, and each subtype with what its bases give it; and
//! the way back from a path of the view to the YAML it stands for.

use serde_json::{Map, Value, json};

use std::collections::{BTreeMap, BTreeSet};

use super::doc::{Found, Obj, Path, as_obj, flag, list, obj, py_str, text};
use super::steps::{required_input_names, step_kind, taken, walk};
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
/// Beside an attribute or an input typed by a value, the value's fields, as
/// its module declares them, so a module importing the type reads them too
/// (ADR-0254).
pub(super) const FIELDS: &str = "_fields";

/// On an invariant a value's invariant gives an attribute holding it: the
/// value and its invariant, `[<Value>, <invariant>]`, where a finding on it
/// is reported (ADR-0255).
pub(super) const VALUE_OF: &str = "_value";

/// On an invariant a text's `not_blank` generates: the attribute marked,
/// where a finding on it is reported (ADR-0257).
pub(super) const NOT_BLANK: &str = "_not_blank";
/// On an input a create's `rest: inputs` derives: its target,
/// `<Type>.<transition>`, and the step's path (ADR-0271).
pub(super) const DERIVED: &str = "_derived_from";
/// On a transition: each name its `rest: inputs` could not derive, and why
/// (ADR-0271).
pub(super) const RESTED: &str = "_rest_clashes";
/// The keys an input's declaration takes (`flow.schema.json`'s `input`).
const INPUT_KEYS: [&str; 8] = [
    "type",
    "reference",
    "optional",
    "personal",
    "not_blank",
    "media_types",
    "default",
    "description",
];

/// A type with the invariant each string attribute it marks `not_blank`
/// generates, `<attribute>_not_blank`, after the type's own; one the type
/// declares itself is left as declared, which `not_blank_errors` reports
/// (ADR-0257 decision 3), as `check-flows.py`'s `with_not_blank`.
fn with_not_blank(t: Value) -> Value {
    let marked: Vec<String> = obj(&t, "attributes")
        .iter()
        .filter(|(_, spec)| {
            spec.get("not_blank").and_then(Value::as_bool) == Some(true)
                && spec.get("type").map(py_str).as_deref() == Some("string")
        })
        .map(|(a, _)| a.clone())
        .collect();
    if marked.is_empty() {
        return t;
    }
    let mut t = as_obj(&t).clone();
    let mut invariants = obj(&Value::Object(t.clone()), "invariants").clone();
    for a in marked {
        let name = format!("{a}_not_blank");
        if !invariants.contains_key(&name) {
            invariants.insert(
                name,
                json!({
                    "description": format!("{a} is not blank, as its not_blank marks it (ADR-0257)."),
                    "expression": format!("{a} is null or trim({a}) != \"\""),
                    NOT_BLANK: a,
                }),
            );
        }
    }
    t.insert("invariants".into(), Value::Object(invariants));
    Value::Object(t)
}

/// A type's attributes and its transitions' declared inputs typed by a value
/// `values` holds, each given the value's fields beside it (ADR-0254).
fn with_fields(t: Value, values: &Obj) -> Value {
    if values.is_empty() {
        return t;
    }
    let give = |spec: &Value| -> Value {
        let typ = spec.get("type").and_then(Value::as_str).unwrap_or_default();
        match values.get(typ).and_then(|v| v.get("attributes")) {
            Some(fields) => {
                let mut s = as_obj(spec).clone();
                s.insert(FIELDS.into(), fields.clone());
                Value::Object(s)
            }
            None => spec.clone(),
        }
    };
    let mut t = as_obj(&t).clone();
    let attrs: Obj = obj(&Value::Object(t.clone()), "attributes")
        .iter()
        .map(|(n, spec)| (n.clone(), give(spec)))
        .collect();
    // each value's invariant, held by each attribute holding the value as
    // the type's invariant `<attribute>_<invariant>`, after the type's own;
    // one the type declares itself is left as declared, which
    // `declared_value_errors` reports (ADR-0255)
    let mut invariants = obj(&Value::Object(t.clone()), "invariants").clone();
    for (a, spec) in &attrs {
        let typ = spec.get("type").and_then(Value::as_str).unwrap_or_default();
        let Some(v) = values.get(typ) else { continue };
        let fields: std::collections::BTreeSet<String> =
            obj(v, "attributes").keys().cloned().collect();
        for (i, inv) in obj(v, "invariants") {
            let name = format!("{a}_{i}");
            let text = inv.get("expression").map(py_str).unwrap_or_default();
            let Ok(expression) = crate::expr::held_through(&text, &fields, a) else {
                continue;
            };
            if !invariants.contains_key(&name) {
                invariants.insert(
                    name,
                    json!({
                        "description": inv.get("description").cloned().unwrap_or(Value::Null),
                        "expression": expression,
                        VALUE_OF: [typ, i],
                    }),
                );
            }
        }
    }
    if !invariants.is_empty() {
        t.insert("invariants".into(), Value::Object(invariants));
    }
    if !attrs.is_empty() {
        t.insert(
            "attributes".into(),
            Value::Object(attrs.into_iter().collect()),
        );
    }
    let transitions: Obj = obj(&Value::Object(t.clone()), "transitions")
        .iter()
        .map(|(xn, x)| {
            let inputs: Obj = obj(x, "inputs")
                .iter()
                .map(|(n, s)| (n.clone(), give(s)))
                .collect();
            let mut x = as_obj(x).clone();
            if !inputs.is_empty() {
                x.insert("inputs".into(), Value::Object(inputs.into_iter().collect()));
            }
            (xn.clone(), Value::Object(x))
        })
        .collect();
    if !transitions.is_empty() {
        t.insert(
            "transitions".into(),
            Value::Object(transitions.into_iter().collect()),
        );
    }
    Value::Object(t)
}

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
    // a transition declaring none takes its type's, a bound machine's among
    // them (ADR-0182 decision 5)
    let own = taken_kinds(&t);
    let marked: Vec<(String, Vec<String>)> = obj(&t, "transitions")
        .iter()
        .filter_map(|(xn, x)| {
            taken_kinds(x)
                .or_else(|| own.clone())
                .map(|k| (xn.clone(), k))
        })
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
                .chain(kinds.iter().map(|k| format!("{a} == ActorKind.{k}")))
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
            x.insert("taken_by".into(), json!(kinds));
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

/// The columns every row of a type's table has beside `id`, `state` and
/// `declaration_version`, which no member is named (`flow-format.md` §3),
/// and those an observation kind's has beside `occurred_at`, which check 54
/// keeps (`storage-schema.md` §3, ADR-0265).
const ROW_COLUMNS: &[&str] = &["version", "last_event", "state_source", "superseded_by"];
const OBSERVATION_COLUMNS: &[&str] = &["subject_id", "corrects_id"];

/// No attribute of a type is named `<reference>_id` beside a single
/// reference of that name, its own or inherited: the store keeps the
/// reference in a column of that name, so it could not store the type
/// (`storage-schema.md` §3.2, D578; `check-flows.py`'s `column_errors`).
/// Nor is one the store keeps under its own name, a single value that is no
/// reference, named as a column every row of its table has, an observation
/// kind's included (ADR-0265, issue #72). Nor is one the store indexes by
/// its own name, marked `indexed` or `external`, named so that its index
/// takes a name the store gives another (ADR-0267). Over the view.
pub fn column_errors(doc: &Value) -> Found {
    let at = |segs: &[&str]| -> Path { segs.iter().map(|k| Seg::Key((*k).to_string())).collect() };
    // a single value kept under its own name: neither a reference, kept as
    // `<name>_id`, nor a set, kept in a table of its own
    let own_column = |spec: &Value| {
        spec.get("type")
            .and_then(Value::as_str)
            .is_some_and(|t| !t.ends_with("[]"))
    };
    let indexed =
        |spec: &Value| own_column(spec) && (flag(spec, "indexed") || flag(spec, "external"));
    // what already holds the index name `t_<type>_<a>_ix`
    // (`storage-schema.md` §5): the store's index on the state's source, a
    // uniqueness's key index, `t_<type>_<b>_key_ix`, or the actor identity's
    // folded one, `t_<type>_<b>_fold_ix`
    let index_taken = |tn: &str, a: &str, attrs: &Obj| -> Option<String> {
        if a == "asserted" {
            return Some(format!("the store's index on {tn}.state_source"));
        }
        let (base, suffix) = a.rsplit_once('_')?;
        let spec = attrs.get(base)?;
        match suffix {
            "key" if flag(spec, "unique") => {
                Some(format!("the key index of {tn}.{base}'s uniqueness"))
            }
            "fold" if flag(spec, "actor_kind") => Some(format!(
                "the folded index of {tn}.{base}, its actor identity"
            )),
            _ => None,
        }
    };
    let index_error = |path: Path, tn: &str, a: &str, taken: &str| {
        (
            path,
            "names".to_string(),
            format!(
                "{tn}.{a} is indexed, and its index would take the name of {taken}; name the attribute otherwise (storage-schema.md §5, ADR-0267)"
            ),
        )
    };
    let mut out = Found::new();
    for (tn, t) in types(doc) {
        let attrs = obj(t, "attributes");
        for (a, spec) in attrs {
            let single = a.strip_suffix("_id").is_some_and(|base| {
                attrs
                    .get(base)
                    .and_then(|r| r.get("reference"))
                    .and_then(Value::as_str)
                    .is_some_and(|r| !r.ends_with("[]"))
            });
            if single {
                let base = &a[..a.len() - "_id".len()];
                out.push((
                    at(&["types", tn.as_str(), "attributes", a.as_str()]),
                    "names".into(),
                    format!(
                        "{tn}.{a} is the column the store keeps the reference {base} in; name the attribute otherwise (storage-schema.md §3.2, D578)"
                    ),
                ));
            } else if ROW_COLUMNS.contains(&a.as_str()) && own_column(spec) {
                out.push((
                    at(&["types", tn.as_str(), "attributes", a.as_str()]),
                    "names".into(),
                    format!(
                        "{tn}.{a} is a column every row of {tn}'s table has; name the attribute otherwise (storage-schema.md §3.2, ADR-0265)"
                    ),
                ));
            } else if indexed(spec)
                && let Some(taken) = index_taken(tn.as_str(), a, attrs)
            {
                out.push(index_error(
                    at(&["types", tn.as_str(), "attributes", a.as_str()]),
                    tn.as_str(),
                    a,
                    &taken,
                ));
            }
        }
        for (on, o) in obj(t, "observations") {
            let kind = o.get("kind").and_then(Value::as_str).unwrap_or_default();
            for (a, spec) in obj(o, "attributes") {
                let column =
                    ROW_COLUMNS.contains(&a.as_str()) || OBSERVATION_COLUMNS.contains(&a.as_str());
                if column && own_column(spec) {
                    out.push((
                        at(&["types", tn.as_str(), "observations", on.as_str(), "attributes", a.as_str()]),
                        "names".into(),
                        format!(
                            "{kind}.{a} is a column every row of {kind}'s table has; name the attribute otherwise (storage-schema.md §3, ADR-0265)"
                        ),
                    ));
                }
            }
        }
    }
    out
}

/// A transition marked `taken_by` is one a request takes, with an effect:
/// an initial, external or internal one; and what it generates is not a
/// name its type declares (ADR-0176 decision 1). Over the view.
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

/// Whether a transition holds a create taking the rest of its inputs from
/// its own.
fn has_rest(x: &Value) -> bool {
    walk(x.get("effect")).iter().any(|s| {
        s.kind == "create" && py_str(s.body().get("rest").unwrap_or(&Value::Null)) == "inputs"
    })
}

fn path_json(path: &[Seg]) -> Value {
    Value::Array(
        path.iter()
            .map(|s| match s {
                Seg::Key(k) => json!(k),
                Seg::Index(i) => json!(i),
            })
            .collect(),
    )
}

fn json_path(v: &Value) -> Path {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|s| match s {
                    Value::Number(n) => Seg::Index(n.as_u64().unwrap_or(0) as usize),
                    other => Seg::Key(py_str(other)),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a default is the empty set as written, `{}`, spaces aside.
fn empty_set(v: &Value) -> bool {
    py_str(v)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        == "{}"
}

/// Each type of `kinds` with each `rest: inputs` of its creates expanded, as
/// `check-flows.py`'s `with_rests` (ADR-0271, ADR-0275): each input of the
/// created transition the step does not name becomes an input of the
/// transition making it, declared as the target declares it, required where
/// the target requires it and gives no default, marked with its target; and
/// the step passes it as `inputs.<name>`. One the target does not require
/// is optional, and a set defaults to the empty set, `{}`, where the target
/// gives it no default of its own. An observation kind's `subject` and
/// `corrects` are never derived. A target taking the rest of its own inputs
/// is expanded first, so a derivation carries through it; one reached again
/// while it is expanded, a cycle the model's check 12 refuses, is read as
/// written. A name the transition takes itself, two creates deriving one
/// name unlike, or a set the target defaults to a value of its own, is left
/// underived and recorded on the transition, which `rest_errors` reports.
fn with_rests(kinds: Obj, everything: &Obj) -> Obj {
    let mut rests = Rests {
        everything,
        recorded: observation_kinds(everything),
        done: BTreeMap::new(),
        doing: BTreeSet::new(),
    };
    kinds
        .into_iter()
        .map(|(tn, t)| {
            let transitions = obj(&t, "transitions").clone();
            if !transitions.values().any(has_rest) {
                return (tn, t);
            }
            let mut out = Map::new();
            for (xn, x) in &transitions {
                let expanded = rests.transition(&tn, xn).unwrap_or_else(|| x.clone());
                out.insert(xn.clone(), expanded);
            }
            let mut t = as_obj(&t).clone();
            t.insert("transitions".into(), Value::Object(out));
            (tn, Value::Object(t))
        })
        .collect()
}

/// `with_rests`'s expansion, each transition expanded once, its targets
/// first.
struct Rests<'a> {
    everything: &'a Obj,
    recorded: Obj,
    done: BTreeMap<(String, String), Value>,
    doing: BTreeSet<(String, String)>,
}

impl Rests<'_> {
    /// `tn`'s transition `xn` with its rests expanded; as written where it
    /// holds none, or is being expanded.
    fn transition(&mut self, tn: &str, xn: &str) -> Option<Value> {
        let x = self
            .everything
            .get(tn)
            .and_then(|t| obj(t, "transitions").get(xn))
            .cloned()?;
        let key = (tn.to_string(), xn.to_string());
        if !x.is_object() || !has_rest(&x) || self.doing.contains(&key) {
            return Some(x);
        }
        if let Some(done) = self.done.get(&key) {
            return Some(done.clone());
        }
        self.doing.insert(key.clone());
        let expanded = self.expanded(tn, xn, &x);
        self.doing.remove(&key);
        self.done.insert(key, expanded.clone());
        Some(expanded)
    }

    fn expanded(&mut self, tn: &str, xn: &str, x: &Value) -> Value {
        let own = taken(x);
        let mut derived: Map<String, Value> = Map::new();
        let mut clashes: Vec<Value> = Vec::new();
        let steps = self.expand_rest(
            tn,
            xn,
            list(x, "effect"),
            &[],
            &own,
            &mut derived,
            &mut clashes,
        );
        let mut x = as_obj(x).clone();
        x.insert("effect".into(), Value::Array(steps));
        let mut inputs = obj(&Value::Object(x.clone()), "inputs").clone();
        for (j, spec) in derived {
            inputs.insert(j, spec);
        }
        x.insert("inputs".into(), Value::Object(inputs));
        if !clashes.is_empty() {
            x.insert(RESTED.into(), Value::Array(clashes));
        }
        Value::Object(x)
    }

    #[allow(clippy::too_many_arguments)]
    fn expand_rest(
        &mut self,
        tn: &str,
        xn: &str,
        steps: &[Value],
        path: &[Seg],
        own: &BTreeSet<String>,
        derived: &mut Map<String, Value>,
        clashes: &mut Vec<Value>,
    ) -> Vec<Value> {
        let mut result = Vec::new();
        for (i, st) in steps.iter().enumerate() {
            let mut here = path.to_vec();
            here.push(Seg::Index(i));
            let kind = step_kind(st).unwrap_or("");
            if kind == "foreach" {
                let mut inner = here.clone();
                inner.push(Seg::Key("foreach".into()));
                inner.push(Seg::Key("steps".into()));
                let f = st.get("foreach").cloned().unwrap_or(Value::Null);
                let body =
                    self.expand_rest(tn, xn, list(&f, "steps"), &inner, own, derived, clashes);
                let mut f = as_obj(&f).clone();
                f.insert("steps".into(), Value::Array(body));
                let mut st = as_obj(st).clone();
                st.insert("foreach".into(), Value::Object(f));
                result.push(Value::Object(st));
                continue;
            }
            let c = st.get("create").cloned().unwrap_or(Value::Null);
            if kind != "create" || py_str(c.get("rest").unwrap_or(&Value::Null)) != "inputs" {
                result.push(st.clone());
                continue;
            }
            let mut c = as_obj(&c).clone();
            c.remove("rest");
            let (k, tx) = (
                py_str(c.get("type").unwrap_or(&Value::Null)),
                py_str(c.get("transition").unwrap_or(&Value::Null)),
            );
            let target = self
                .everything
                .get(&k)
                .or_else(|| self.recorded.get(&k))
                .cloned()
                .unwrap_or(Value::Null);
            let y = if self.everything.contains_key(&k) {
                self.transition(&k, &tx)
            } else {
                obj(&target, "transitions").get(&tx).cloned()
            };
            let mut given = obj(&Value::Object(c.clone()), "inputs").clone();
            if let Some(y) = y.filter(|y| y.is_object()) {
                let required = required_input_names(&y);
                let left: BTreeSet<String> = if self.recorded.contains_key(&k) {
                    ["subject", "corrects"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect()
                } else {
                    BTreeSet::new()
                };
                let names: BTreeSet<String> = taken(&y)
                    .into_iter()
                    .filter(|j| !given.contains_key(j) && !left.contains(j))
                    .collect();
                for j in names {
                    let declared = obj(&y, "inputs").get(&j).filter(|s| !s.is_null()).cloned();
                    let defaulted = declared
                        .as_ref()
                        .is_some_and(|s| s.get("default").is_some());
                    let source = declared.unwrap_or_else(|| {
                        obj(&target, "attributes")
                            .get(&j)
                            .cloned()
                            .unwrap_or(Value::Null)
                    });
                    let mut spec: Map<String, Value> = Map::new();
                    for key in INPUT_KEYS {
                        if key == "optional" || key == "default" {
                            continue;
                        }
                        if let Some(v) = source.get(key) {
                            spec.insert(key.into(), v.clone());
                        }
                    }
                    let mark = format!("{k}.{tx}");
                    if own.contains(&j) {
                        clashes.push(json!([path_json(&here), format!(
                            "{tn}.{xn}'s create of {mark} takes the rest of its inputs, and derives '{j}', which the transition takes itself: name it in the step's inputs (ADR-0271)"
                        )]));
                        continue;
                    }
                    if !required.contains(&j) || defaulted {
                        let of = source
                            .get("type")
                            .filter(|t| super::doc::truthy(Some(t)))
                            .or_else(|| source.get("reference"))
                            .map(py_str)
                            .unwrap_or_default();
                        if of.ends_with("[]") {
                            if source
                                .get("default")
                                .is_some_and(|d| !d.is_null() && !empty_set(d))
                            {
                                clashes.push(json!([path_json(&here), format!(
                                    "{tn}.{xn}'s create of {mark} takes the rest of its inputs, and derives the set '{j}', which {mark} gives a default of its own: name it in the step's inputs (ADR-0275)"
                                )]));
                                continue;
                            }
                            spec.insert("default".into(), json!("{}"));
                        } else {
                            spec.insert("optional".into(), json!(true));
                        }
                    }
                    if let Some(was) = derived.get(&j) {
                        let mut plain = as_obj(was).clone();
                        let first = plain
                            .remove(DERIVED)
                            .and_then(|d| d.get(0).map(py_str))
                            .unwrap_or_default();
                        if plain != spec {
                            clashes.push(json!([path_json(&here), format!(
                                "two creates derive '{j}' unlike, {first} and {mark}: name it in the step's inputs (ADR-0271)"
                            )]));
                            continue;
                        }
                    } else {
                        let mut effect = vec![Seg::Key("effect".into())];
                        effect.extend(here.iter().cloned());
                        let mut marked = spec.clone();
                        marked.insert(DERIVED.into(), json!([mark, path_json(&effect)]));
                        derived.insert(j.clone(), Value::Object(marked));
                    }
                    given.insert(j.clone(), json!(format!("inputs.{j}")));
                }
            }
            c.insert("inputs".into(), Value::Object(given));
            let mut st = as_obj(st).clone();
            st.insert("create".into(), Value::Object(c));
            result.push(Value::Object(st));
        }
        result
    }
}

/// A `rest: inputs` that derives a name its transition takes itself, or one
/// two creates derive unlike (ADR-0271).
pub fn rest_errors(doc: &Value) -> Found {
    let mut out = Found::new();
    for (tn, t) in obj(doc, "types") {
        for (xn, x) in obj(t, "transitions") {
            for clash in list(x, RESTED) {
                let mut path: Path = vec![
                    Seg::Key("types".into()),
                    Seg::Key(tn.clone()),
                    Seg::Key("transitions".into()),
                    Seg::Key(xn.clone()),
                    Seg::Key("effect".into()),
                ];
                path.extend(json_path(clash.get(0).unwrap_or(&Value::Null)));
                out.push(super::doc::finding(
                    path,
                    "names",
                    py_str(clash.get(1).unwrap_or(&Value::Null)),
                ));
            }
        }
    }
    out
}

/// Each observation kind the types given declare, as the checks see one
/// (`declaration-syntax.md` §6.8; `check-flows.py`'s `observation_kinds`): a
/// type born final, recorded by its generated creation, `record`, by a
/// caller or an effect's `create` (ADR-0270), taking the subject and each
/// field, and optionally the observation it corrects; a kind with a personal
/// field has a generated erasure, `forget`, too (ADR-0147). Its occurred and
/// recorded times are read, and never given as inputs.
pub(crate) fn observation_kinds(kinds: &Obj) -> Obj {
    let mut out = Obj::new();
    for (tn, t) in kinds {
        for ob in obj(t, "observations").values() {
            let Some(kind) = ob.get("kind").and_then(Value::as_str) else {
                continue;
            };
            let fields = obj(ob, "attributes").clone();
            let mut attrs = fields.clone();
            attrs.insert("subject".into(), json!({"reference": tn}));
            attrs.insert(
                "corrects".into(),
                json!({"reference": kind, "optional": true}),
            );
            attrs.insert("occurred_at".into(), json!({"type": "timestamp"}));
            attrs.insert("recorded_at".into(), json!({"type": "timestamp"}));
            let required: Vec<String> = std::iter::once("subject".to_string())
                .chain(
                    fields
                        .iter()
                        .filter(|(_, f)| !super::doc::optional(f))
                        .map(|(a, _)| a.clone()),
                )
                .collect();
            let optional_inputs: Vec<String> = std::iter::once("corrects".to_string())
                .chain(
                    fields
                        .iter()
                        .filter(|(_, f)| super::doc::optional(f))
                        .map(|(a, _)| a.clone()),
                )
                .collect();
            let mut transitions = Map::new();
            transitions.insert(
                "record".into(),
                json!({"kind": "initial", "to": "RECORDED", "required_inputs": required, "optional_inputs": optional_inputs}),
            );
            if fields.values().any(|f| flag(f, "personal")) {
                transitions.insert(
                    "forget".into(),
                    json!({"kind": "erasure", "inputs": {"reason": {"type": "string"}}}),
                );
            }
            out.insert(
                kind.to_string(),
                json!({"attributes": attrs, "states": {"RECORDED": {"final": true}},
                       "invariants": ob.get("invariants").cloned().unwrap_or(json!({})), "transitions": transitions}),
            );
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
                (
                    tn,
                    with_not_blank(with_fields(
                        with_taken_by(Value::Object(t)),
                        obj(doc, "values"),
                    )),
                )
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
        // each create's `rest: inputs` expanded over every type, after
        // inheritance (ADR-0271)
        let mut everything = library.clone();
        for (k, v) in &kinds {
            everything.insert(k.clone(), v.clone());
        }
        let kinds = with_rests(kinds, &everything);
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
        // read where they are, where each path mapped back cloned its type
        // whole, twice (tenth production review)
        static NONE: Value = Value::Null;
        let kind = obj(&self.doc, "types").get(name).unwrap_or(&NONE);
        if let (Some(Seg::Key(k)), Some(Seg::Key(n))) = (path.get(2), path.get(3))
            && inherited(kind, k, n)
        {
            return None;
        }
        // an input a create's `rest: inputs` derives is reported at that
        // step (ADR-0271)
        if let (Some(Seg::Key(k)), Some(Seg::Key(xn)), Some(Seg::Key(i)), Some(Seg::Key(j))) =
            (path.get(2), path.get(3), path.get(4), path.get(5))
            && k == "transitions"
            && i == "inputs"
            && let Some(d) = obj(kind, "transitions")
                .get(xn)
                .map(|x| obj(x, "inputs"))
                .and_then(|ins| ins.get(j))
                .and_then(|s| s.get(DERIVED))
        {
            let mut out: Path = vec![
                Seg::Key("types".into()),
                Seg::Key(name.clone()),
                Seg::Key("transitions".into()),
                Seg::Key(xn.clone()),
            ];
            out.extend(json_path(d.get(1).unwrap_or(&Value::Null)));
            return Some(out);
        }
        // an invariant a value gives its holder is reported at the value's
        // own (ADR-0255)
        if let (Some(Seg::Key(k)), Some(Seg::Key(n))) = (path.get(2), path.get(3))
            && k == "invariants"
            && let Some(Value::Array(of)) =
                obj(kind, "invariants").get(n).and_then(|i| i.get(VALUE_OF))
            && let [Value::String(vn), Value::String(i)] = of.as_slice()
        {
            let mut out: Path = vec![
                Seg::Key("values".into()),
                Seg::Key(vn.clone()),
                Seg::Key("invariants".into()),
                Seg::Key(i.clone()),
            ];
            out.extend(path[4..].iter().cloned());
            return Some(out);
        }
        // an invariant a text's not_blank generates is reported at the
        // marking (ADR-0257)
        if let (Some(Seg::Key(k)), Some(Seg::Key(n))) = (path.get(2), path.get(3))
            && k == "invariants"
            && let Some(Value::String(a)) = obj(kind, "invariants")
                .get(n)
                .and_then(|i| i.get(NOT_BLANK))
        {
            return Some(vec![
                Seg::Key("types".into()),
                Seg::Key(name.clone()),
                Seg::Key("attributes".into()),
                Seg::Key(a.clone()),
                Seg::Key("not_blank".into()),
            ]);
        }
        if self.machines.contains(name) {
            let mut out: Path = vec![Seg::Key("machines".into()), Seg::Key(name.clone())];
            if matches!(path.get(2), Some(Seg::Key(k)) if k == "attributes") {
                out.push(Seg::Key("requires".into()));
            }
            out.extend(path[2..].iter().cloned());
            return Some(out);
        }
        let raw = self.raw_types.get(name).unwrap_or(&NONE);
        if flag(raw, "state_machine")
            && let (Some(Seg::Key(section)), Some(Seg::Key(member))) = (path.get(2), path.get(3))
            && ["states", "transitions", "conditions"].contains(&section.as_str())
            && !obj(raw, section).contains_key(member)
        {
            return None;
        }
        Some(path.to_vec())
    }
}

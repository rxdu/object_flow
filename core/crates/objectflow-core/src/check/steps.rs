//! A transition's parts as the flow checker reads them: its states, its
//! inputs and the steps of its effect (`check-flows.py`, "reading a module").

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::doc::{Path, obj, optional, py_str, re, squash, strings, text};
use crate::yaml::Seg;

/// The states a transition leaves, as a list.
pub fn sources(x: &Value) -> Vec<String> {
    match x.get("from") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a.iter().map(py_str).collect(),
        Some(other) => vec![py_str(other)],
    }
}

/// The states a transition enters, as a list.
pub fn targets(x: &Value) -> Vec<String> {
    match x.get("to") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a.iter().map(py_str).collect(),
        Some(other) => vec![py_str(other)],
    }
}

pub fn kind(x: &Value) -> &str {
    text(x, "kind").unwrap_or_default()
}

/// Every input a transition takes: attribute inputs and declared inputs.
pub fn taken(x: &Value) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = strings(x, "required_inputs").into_iter().collect();
    out.extend(strings(x, "optional_inputs"));
    out.extend(obj(x, "inputs").keys().cloned());
    out
}

/// The inputs a request always has a value for: required attribute inputs,
/// and declared inputs that are not optional or that carry a default.
pub fn required_input_names(x: &Value) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = obj(x, "inputs")
        .iter()
        .filter(|(_, spec)| !optional(spec) || spec.get("default").is_some())
        .map(|(i, _)| i.clone())
        .collect();
    out.extend(strings(x, "required_inputs"));
    out
}

/// The kinds of step an effect has.
pub const STEP_KINDS: &[&str] = &[
    "assign",
    "clear",
    "add",
    "remove",
    "call",
    "create",
    "foreach",
    "supersede",
];

/// One step of an effect at any depth: the step, its kind, its path from
/// the effect, and its depth.
pub struct Step<'a> {
    pub st: &'a Value,
    pub kind: &'static str,
    pub path: Path,
    pub depth: usize,
}

impl Step<'_> {
    /// The step's own body, under its kind.
    pub fn body(&self) -> &Value {
        self.st.get(self.kind).unwrap_or(&Value::Null)
    }
}

/// The kind of one step: the first of the kinds it has.
pub fn step_kind(st: &Value) -> Option<&'static str> {
    STEP_KINDS.iter().copied().find(|k| st.get(*k).is_some())
}

/// Every effect step at any depth, with its path from the effect and its
/// depth (`check-flows.py`'s `walk`).
pub fn walk(steps: Option<&Value>) -> Vec<Step<'_>> {
    fn go<'a>(steps: &'a [Value], prefix: &Path, out: &mut Vec<Step<'a>>) {
        for (i, st) in steps.iter().enumerate() {
            let Some(kind) = step_kind(st) else { continue };
            let mut path = prefix.clone();
            path.push(Seg::Index(i));
            out.push(Step {
                st,
                kind,
                path: path.clone(),
                depth: prefix.len() / 3,
            });
            if kind == "foreach" {
                let mut inner = path;
                inner.push(Seg::Key("foreach".into()));
                inner.push(Seg::Key("steps".into()));
                let nested = st.get("foreach").and_then(|f| f.get("steps"));
                if let Some(Value::Array(a)) = nested {
                    go(a, &inner, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    if let Some(Value::Array(a)) = steps {
        go(a, &Vec::new(), &mut out);
    }
    out
}

/// The expressions a step reads, as text.
pub fn step_expressions(step: &Step<'_>) -> Vec<String> {
    let body = step.body();
    let mut out = Vec::new();
    let mut push = |v: Option<&Value>| {
        if let Some(v) = v {
            out.push(py_str(v));
        }
    };
    match step.kind {
        "assign" | "add" | "remove" => push(body.get("expr")),
        "call" => {
            push(body.get("target"));
            for v in obj(body, "inputs").values() {
                push(Some(v));
            }
        }
        "create" => {
            for v in obj(body, "inputs").values() {
                push(Some(v));
            }
        }
        "foreach" => {
            for k in ["array", "range", "where"] {
                push(body.get(k));
            }
        }
        "supersede" => push(Some(body)),
        _ => {}
    }
    out
}

/// The top-level steps of one kind's bodies.
pub fn steps<'a>(x: &'a Value, verb: &str) -> Vec<&'a Value> {
    match x.get("effect") {
        Some(Value::Array(a)) => a.iter().filter_map(|s| s.get(verb)).collect(),
        _ => Vec::new(),
    }
}

/// (expression, location) for each top-level assign step.
pub fn assigns(x: &Value) -> Vec<(String, String)> {
    steps(x, "assign")
        .into_iter()
        .map(|a| {
            (
                squash(&a.get("expr").map(py_str).unwrap_or_default()),
                a.get("location").map(py_str).unwrap_or_default(),
            )
        })
        .collect()
}

/// The attributes a transition's top-level clear steps clear.
pub fn cleared(x: &Value) -> Vec<String> {
    steps(x, "clear")
        .into_iter()
        .flat_map(|g| match g {
            Value::Array(a) => a.iter().map(py_str).collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect()
}

static INPUT_READ: LazyLock<Regex> = LazyLock::new(|| re(r"\binputs\.([a-z][a-z0-9_]*)"));

/// The inputs an expression reads, `inputs.<name>`.
pub fn input_reads(text: &str) -> BTreeSet<String> {
    INPUT_READ
        .captures_iter(text)
        .map(|c| c[1].to_string())
        .collect()
}

/// A step body's member as text.
pub fn body_text(v: &Value, k: &str) -> String {
    v.get(k).map(py_str).unwrap_or_default()
}

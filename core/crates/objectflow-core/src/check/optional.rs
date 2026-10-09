//! What an expression reads that may be absent, what it tests with `is null`
//! or `is not null`, and the type its value names where it names objects,
//! over the types of the modules checked (`check-flows.py`'s
//! `OptionalReads`; `declaration-syntax.md` §8.2, ADR-0103 §6).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::doc::{Obj, flag, obj, py_str, text};
use super::steps::{kind, sources};
use crate::expr::{Expr, parse};

/// The members every object or row has, which no declaration makes optional.
const BUILT_IN: &[&str] = &[
    "id",
    "state",
    "open",
    "created_at",
    "created_by_kind",
    "category",
    "type",
    "recorded_at",
    "occurred_at",
    "subject",
    "intervals",
    "transitions",
];

/// A name an effect or an aggregate binds, to the type of object it holds.
pub type Env = BTreeMap<String, Option<String>>;

pub struct OptionalReads<'l> {
    lib: &'l Obj,
    tn: String,
    x: Value,
    t: Value,
    derived_seen: Vec<(String, String)>,
    present: BTreeSet<String>,
    /// Each optional read, by the text it is matched on, with why.
    pub reads: BTreeMap<String, String>,
    pub tested: BTreeSet<String>,
    /// Each derived attribute read, as (type, name), through any path whose
    /// type is known.
    pub derived: BTreeSet<(String, String)>,
    /// Each member read, as (type, name), through any path whose type is
    /// known.
    pub members: BTreeSet<(String, String)>,
    /// Whether `is null` walks what it tests, which reads it as any
    /// expression does.
    pub through_tests: bool,
    /// Each reference read through, to a member or as a collection, as
    /// (type, name).
    pub traversed: BTreeSet<(String, String)>,
    /// The reference the walk last read, if its last read was one.
    last: Option<(String, String)>,
    /// Each type ranged over by name, a type scan.
    pub scanned: BTreeSet<String>,
}

impl<'l> OptionalReads<'l> {
    pub fn new(lib: &'l Obj, tn: &str, x: Option<&Value>) -> OptionalReads<'l> {
        OptionalReads::seen(lib, tn, x, Vec::new())
    }

    fn seen(
        lib: &'l Obj,
        tn: &str,
        x: Option<&Value>,
        derived_seen: Vec<(String, String)>,
    ) -> OptionalReads<'l> {
        let x = x.cloned().unwrap_or(Value::Object(Default::default()));
        let t = lib.get(tn).cloned().unwrap_or(Value::Null);
        let froms = if matches!(kind(&x), "external" | "internal") {
            sources(&x)
        } else {
            Vec::new()
        };
        let required: Vec<BTreeSet<String>> = froms
            .iter()
            .map(|st| {
                let state = obj(&t, "states").get(st).cloned().unwrap_or(Value::Null);
                super::doc::strings(&state, "required_attributes")
                    .into_iter()
                    .collect()
            })
            .collect();
        let present = match required.split_first() {
            Some((first, rest)) => rest.iter().fold(first.clone(), |acc, s| {
                acc.intersection(s).cloned().collect()
            }),
            None => BTreeSet::new(),
        };
        OptionalReads {
            lib,
            tn: tn.to_string(),
            x,
            t,
            derived_seen,
            present,
            reads: BTreeMap::new(),
            tested: BTreeSet::new(),
            derived: BTreeSet::new(),
            members: BTreeSet::new(),
            through_tests: false,
            traversed: BTreeSet::new(),
            last: None,
            scanned: BTreeSet::new(),
        }
    }

    /// Each optional read the expression never tests, with why it may be
    /// absent.
    pub fn untested(mut self, tree: &Expr) -> BTreeMap<String, String> {
        self.walk(tree, &Env::new());
        let tested = self.tested;
        self.reads
            .into_iter()
            .filter(|(k, _)| !tested.contains(k))
            .collect()
    }

    /// The reads it made that it never tested.
    pub fn left(&self) -> BTreeMap<String, String> {
        self.reads
            .iter()
            .filter(|(k, _)| !self.tested.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// The text a read and a test are matched by; `this.x` and `x` are one.
    fn key(e: &Expr) -> Option<String> {
        match e {
            Expr::Name(n) => Some(n.clone()),
            Expr::Path(base, m) => {
                let k = OptionalReads::key(base)?;
                Some(if k == "this" {
                    m.clone()
                } else {
                    format!("{k}.{m}")
                })
            }
            _ => None,
        }
    }

    fn note(&mut self, key: &str, why: String) {
        self.reads.entry(key.to_string()).or_insert(why);
    }

    /// The type of a member read, noting it if it may be absent.
    fn attribute(&mut self, tn: &str, m: &str, key: &str, own: bool) -> Option<String> {
        self.members.insert((tn.to_string(), m.to_string()));
        let t = self.lib.get(tn).cloned().unwrap_or(Value::Null);
        if let Some(spec) = obj(&t, "attributes").get(m).filter(|s| !s.is_null()) {
            let reference = spec.get("reference").map(py_str).unwrap_or_default();
            if flag(spec, "optional")
                && spec.get("default").is_none()
                && !(own && self.present.contains(m))
            {
                self.note(key, format!("{tn}.{m} is optional"));
            }
            self.last = (!reference.is_empty()).then(|| (tn.to_string(), m.to_string()));
            return (!reference.is_empty()).then_some(reference);
        }
        let seen = (tn.to_string(), m.to_string());
        if obj(&t, "derived_attributes").contains_key(m) {
            self.derived.insert(seen.clone());
        }
        if let Some(d) = obj(&t, "derived_attributes").get(m)
            && !self.derived_seen.contains(&seen)
        {
            let mut chain = self.derived_seen.clone();
            chain.push(seen);
            let inner = OptionalReads::seen(self.lib, tn, None, chain);
            if let Some(tree) = text(d, "expression").and_then(|e| parse(e).ok()) {
                let why = inner.untested(&tree);
                if !why.is_empty() {
                    let names: Vec<&str> = why.keys().map(String::as_str).collect();
                    self.note(
                        key,
                        format!("{tn}.{m} can be unknown through {}", names.join(", ")),
                    );
                }
            }
        }
        None
    }

    fn observed(&self, base: &str, member: &str) -> Option<String> {
        let t = self.lib.get(base)?;
        let ob = obj(t, "observations").get(member)?;
        Some(format!(
            "{}[]",
            ob.get("kind").map(py_str).unwrap_or_default()
        ))
    }

    /// The type an expression's value has where it names objects, as `T` or
    /// `T[]`; noting its optional reads.
    pub fn walk(&mut self, e: &Expr, env: &Env) -> Option<String> {
        match e {
            Expr::IsNull(inner, _) => {
                if let Some(k) = OptionalReads::key(inner) {
                    self.tested.insert(k);
                }
                if self.through_tests {
                    self.walk(inner, env);
                }
                None
            }
            Expr::Name(n) => {
                if let Some(t) = env.get(n) {
                    return t.clone();
                }
                match n.as_str() {
                    "this" => Some(self.tn.clone()),
                    "inputs" => Some("<inputs>".into()),
                    _ => {
                        let tn = self.tn.clone();
                        // a collection by its bare name, as through `this`
                        if let Some(k) = self.observed(&tn, n) {
                            return Some(k);
                        }
                        self.attribute(&tn, n, n, true)
                    }
                }
            }
            Expr::Path(base, m) => {
                if matches!(&**base, Expr::Name(n) if n == "inputs") {
                    let declared = obj(&self.x, "inputs")
                        .get(m)
                        .filter(|d| !d.is_null())
                        .cloned();
                    if let Some(declared) = declared {
                        if flag(&declared, "optional") && declared.get("default").is_none() {
                            self.note(&format!("inputs.{m}"), format!("the input {m} is optional"));
                        }
                        let r = declared.get("reference").map(py_str).unwrap_or_default();
                        return (!r.is_empty()).then_some(r);
                    }
                    let spec = obj(&self.t, "attributes")
                        .get(m)
                        .cloned()
                        .unwrap_or(Value::Null);
                    let optional_input =
                        super::doc::strings(&self.x, "optional_inputs").contains(m);
                    if optional_input && spec.get("default").is_none() {
                        self.note(&format!("inputs.{m}"), format!("the input {m} is optional"));
                    }
                    let r = spec.get("reference").map(py_str).unwrap_or_default();
                    return (!r.is_empty()).then_some(r);
                }
                self.last = None;
                let b = self.walk(base, env);
                if matches!(&**base, Expr::Name(_) | Expr::Path(..))
                    && let Some(l) = self.last.take()
                {
                    self.traversed.insert(l);
                }
                // what every object has, read through a path
                if let Some(bb) = &b
                    && !bb.ends_with("[]")
                    && bb != "<inputs>"
                    && BUILT_IN.contains(&m.as_str())
                {
                    self.members.insert((bb.clone(), m.clone()));
                }
                match b {
                    None => self.observed("", m),
                    Some(b)
                        if b.ends_with("[]")
                            || b == "<inputs>"
                            || BUILT_IN.contains(&m.as_str()) =>
                    {
                        self.observed(&b, m)
                    }
                    Some(b) => {
                        if let Some(o) = self.observed(&b, m) {
                            return Some(o);
                        }
                        let own = matches!(&**base, Expr::Name(n) if n == "this");
                        match OptionalReads::key(e) {
                            Some(k) => self.attribute(&b, m, &k, own),
                            None => None,
                        }
                    }
                }
            }
            Expr::Agg {
                var,
                collection,
                filter,
                body,
                ..
            } => {
                let Some(var) = var else {
                    for part in [filter, body].into_iter().flatten() {
                        self.walk(part, env);
                    }
                    return None;
                };
                self.last = None;
                let mut ct = collection.as_ref().and_then(|c| self.walk(c, env));
                if collection
                    .as_deref()
                    .is_some_and(|c| matches!(c, Expr::Name(_) | Expr::Path(..)))
                    && let Some(l) = self.last.take()
                {
                    self.traversed.insert(l);
                }
                if ct.is_none()
                    && let Some(c) = collection
                    && let Expr::Name(n) = &**c
                    && self.lib.contains_key(n)
                    && !env.contains_key(n)
                {
                    ct = Some(format!("{n}[]")); // a type scan: every object of the type
                    self.scanned.insert(n.clone());
                }
                let mut inner = env.clone();
                inner.insert(
                    var.clone(),
                    ct.and_then(|t| t.strip_suffix("[]").map(str::to_string)),
                );
                for part in [filter, body].into_iter().flatten() {
                    self.walk(part, &inner);
                }
                None
            }
            Expr::In(l, r) => {
                self.walk(l, env);
                if !matches!(&**r, Expr::Name(n) if !env.contains_key(n)) {
                    self.walk(r, env);
                }
                None
            }
            Expr::Metric { binds, .. } => {
                for (_, x) in binds {
                    self.walk(x, env);
                }
                None
            }
            Expr::Changed(_, ev) => {
                self.walk(ev, env);
                None
            }
            Expr::Call(f, args) => {
                for a in args {
                    if !matches!(a, Expr::Name(_)) {
                        self.walk(a, env);
                    }
                }
                if let Expr::Path(b, _) = &**f {
                    self.walk(b, env);
                }
                None
            }
            Expr::Not(x) | Expr::Neg(x) => {
                self.walk(x, env);
                None
            }
            Expr::Bin(_, l, r) => {
                self.walk(l, env);
                self.walk(r, env);
                None
            }
            Expr::If(c, a, b) => {
                self.walk(c, env);
                self.walk(a, env);
                self.walk(b, env);
                None
            }
            Expr::Set(items) => {
                for i in items {
                    self.walk(i, env);
                }
                None
            }
            Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Dur(..) | Expr::Money(..) => None,
        }
    }
}

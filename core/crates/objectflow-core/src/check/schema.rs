//! Step 2 of the flow checker: a description against `flow.schema.json`
//! (ADR-0116 decision 6, ADR-0156 decision 2).
//!
//! The validator implements exactly the draft-07 keywords the schema uses,
//! with the semantics of the reference checker's `jsonschema` 3.2.0, and
//! refuses a schema that uses any other, so a change to the schema cannot
//! quietly stop being checked. Errors are placed as `check-flows.py`'s
//! `schema_errors` places them.

use std::collections::BTreeMap;

use regex::Regex;
use serde_json::{Map, Value};

use crate::yaml::{Node, Seg};

/// The schema, as the repository holds it.
pub const SOURCE: &str = include_str!("../../../../../docs/design/flow-format/flow.schema.json");
/// An examples file's schema (`flow-format.md` §11.5).
pub const EXAMPLES: &str =
    include_str!("../../../../../docs/design/flow-format/examples.schema.json");

/// The keywords the validator implements; annotations are read and ignored.
const KEYWORDS: &[&str] = &[
    "$ref",
    "type",
    "required",
    "additionalProperties",
    "properties",
    "propertyNames",
    "const",
    "pattern",
    "items",
    "oneOf",
    "anyOf",
    "not",
    "minItems",
    "minProperties",
    "maxProperties",
    "enum",
    "minLength",
    "minimum",
    "uniqueItems",
    "if",
    "then",
];
const ANNOTATIONS: &[&str] = &["description", "title", "$schema", "$id", "definitions"];

/// A schema compiled for validation.
#[derive(Debug)]
pub struct Schema {
    root: Value,
    patterns: BTreeMap<String, Regex>,
}

/// One validation error, as `jsonschema` 3.2.0 yields it.
#[derive(Clone, Debug)]
pub struct Error {
    /// The keyword that failed.
    pub validator: &'static str,
    /// The path from the document's root to the instance.
    pub path: Vec<Seg>,
    /// The length of the path of the instance its parent validated, for an
    /// error in a `oneOf` or `anyOf`'s context, so its relative path is the
    /// rest (`ValidationError.path`).
    parent_len: usize,
    /// The index of the alternative it came from, for a context error.
    pub branch: Option<usize>,
    pub message: String,
    pub context: Vec<Error>,
}

impl Error {
    fn relevance(&self) -> (isize, bool) {
        let rel = self.path.len().saturating_sub(self.parent_len);
        (
            -(isize::try_from(rel).unwrap_or(isize::MAX)),
            !matches!(self.validator, "anyOf" | "oneOf"),
        )
    }
}

/// Why a schema cannot be used.
#[derive(Debug)]
pub struct SchemaError(pub String);

impl Schema {
    /// The repository's schema, compiled.
    pub fn flow() -> Result<Schema, SchemaError> {
        Schema::parse("flow.schema.json", SOURCE)
    }

    /// The schema of an examples file, compiled.
    pub fn examples() -> Result<Schema, SchemaError> {
        Schema::parse("examples.schema.json", EXAMPLES)
    }

    fn parse(name: &str, text: &str) -> Result<Schema, SchemaError> {
        let root: Value =
            serde_json::from_str(text).map_err(|e| SchemaError(format!("{name}: {e}")))?;
        Schema::new(root).map_err(|e| SchemaError(format!("{name}: {}", e.0)))
    }

    pub fn new(root: Value) -> Result<Schema, SchemaError> {
        let mut patterns = BTreeMap::new();
        compile(&root, &mut patterns, "#")?;
        Ok(Schema { root, patterns })
    }

    pub fn root(&self) -> &Value {
        &self.root
    }

    /// A definition by its reference, `#/definitions/<name>` or `#`.
    pub fn resolve<'s>(&'s self, reference: &str) -> Option<&'s Value> {
        let pointer = reference.strip_prefix('#')?;
        if pointer.is_empty() {
            return Some(&self.root);
        }
        self.root.pointer(pointer)
    }

    /// Every error in a document, in the order `check-flows.py` reports
    /// them: by path, each path's in the order the schema's keywords yield.
    pub fn errors(&self, doc: &Value) -> Vec<Error> {
        let mut out = Vec::new();
        self.descend(&self.root, doc, &mut Vec::new(), &mut out);
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    pub fn is_valid(&self, schema: &Value, instance: &Value) -> bool {
        let mut out = Vec::new();
        self.descend(schema, instance, &mut Vec::new(), &mut out);
        out.is_empty()
    }

    fn descend(&self, schema: &Value, inst: &Value, path: &mut Vec<Seg>, out: &mut Vec<Error>) {
        let error = |validator: &'static str, path: &Vec<Seg>, message: String| Error {
            validator,
            path: path.clone(),
            parent_len: 0,
            branch: None,
            message,
            context: Vec::new(),
        };
        let s = match schema {
            Value::Bool(true) => return,
            Value::Bool(false) => {
                out.push(error(
                    "false",
                    path,
                    format!("False schema does not allow {}", repr(inst)),
                ));
                return;
            }
            Value::Object(s) => s,
            _ => return,
        };
        // draft-07: a $ref stands alone, its siblings ignored
        if let Some(Value::String(r)) = s.get("$ref") {
            if let Some(target) = self.resolve(r) {
                self.descend(target, inst, path, out);
            }
            return;
        }
        for (k, v) in s {
            match k.as_str() {
                "type" => {
                    let types: Vec<&str> = match v {
                        Value::String(t) => vec![t.as_str()],
                        Value::Array(ts) => ts.iter().filter_map(Value::as_str).collect(),
                        _ => Vec::new(),
                    };
                    if !types.iter().any(|t| is_type(inst, t)) {
                        let named = match v {
                            Value::String(t) => format!("'{t}'"),
                            other => repr(other),
                        };
                        out.push(error(
                            "type",
                            path,
                            format!("{} is not of type {named}", repr(inst)),
                        ));
                    }
                }
                "required" => {
                    if let (Value::Object(o), Value::Array(req)) = (inst, v) {
                        for r in req.iter().filter_map(Value::as_str) {
                            if !o.contains_key(r) {
                                out.push(error(
                                    "required",
                                    path,
                                    format!("'{r}' is a required property"),
                                ));
                            }
                        }
                    }
                }
                "additionalProperties" => {
                    let Value::Object(o) = inst else { continue };
                    let declared = s.get("properties").and_then(Value::as_object);
                    let extras: Vec<&String> = o
                        .keys()
                        .filter(|key| !declared.is_some_and(|d| d.contains_key(*key)))
                        .collect();
                    match v {
                        Value::Object(_) => {
                            for extra in extras {
                                path.push(Seg::Key(extra.clone()));
                                self.descend(v, &o[extra.as_str()], path, out);
                                path.pop();
                            }
                        }
                        Value::Bool(false) if !extras.is_empty() => {
                            let names: Vec<String> =
                                extras.iter().map(|e| format!("'{e}'")).collect();
                            let verb = if extras.len() == 1 { "was" } else { "were" };
                            out.push(error(
                                "additionalProperties",
                                path,
                                format!(
                                    "Additional properties are not allowed ({} {verb} unexpected)",
                                    names.join(", ")
                                ),
                            ));
                        }
                        _ => {}
                    }
                }
                "properties" => {
                    if let (Value::Object(o), Value::Object(props)) = (inst, v) {
                        for (name, sub) in props {
                            if let Some(child) = o.get(name) {
                                path.push(Seg::Key(name.clone()));
                                self.descend(sub, child, path, out);
                                path.pop();
                            }
                        }
                    }
                }
                "propertyNames" => {
                    if let Value::Object(o) = inst {
                        for key in o.keys() {
                            self.descend(v, &Value::String(key.clone()), path, out);
                        }
                    }
                }
                "const" => {
                    if inst != v {
                        out.push(error("const", path, format!("{} was expected", repr(v))));
                    }
                }
                "pattern" => {
                    if let (Value::String(text), Value::String(p)) = (inst, v)
                        && let Some(re) = self.patterns.get(p)
                        && !search(re, text)
                    {
                        out.push(error(
                            "pattern",
                            path,
                            format!("{} does not match {}", repr(inst), repr(v)),
                        ));
                    }
                }
                "items" => {
                    if let Value::Array(items) = inst {
                        for (i, item) in items.iter().enumerate() {
                            path.push(Seg::Index(i));
                            self.descend(v, item, path, out);
                            path.pop();
                        }
                    }
                }
                "oneOf" | "anyOf" => {
                    let alternatives = v.as_array().map(Vec::as_slice).unwrap_or_default();
                    let mut context = Vec::new();
                    let mut first_valid = None;
                    for (i, alt) in alternatives.iter().enumerate() {
                        let mut errs = Vec::new();
                        self.descend(alt, inst, &mut path.clone(), &mut errs);
                        if errs.is_empty() {
                            first_valid = Some(i);
                            break;
                        }
                        for mut e in errs {
                            mark(&mut e, path.len(), i);
                            context.push(e);
                        }
                    }
                    let validator = if k == "oneOf" { "oneOf" } else { "anyOf" };
                    match first_valid {
                        None => out.push(Error {
                            context,
                            ..error(
                                validator,
                                path,
                                format!(
                                    "{} is not valid under any of the given schemas",
                                    repr(inst)
                                ),
                            )
                        }),
                        Some(i) if validator == "oneOf" => {
                            let more = alternatives[i + 1..]
                                .iter()
                                .any(|alt| self.is_valid(alt, inst));
                            if more {
                                out.push(error(
                                    "oneOf",
                                    path,
                                    format!(
                                        "{} is valid under more than one of the given schemas",
                                        repr(inst)
                                    ),
                                ));
                            }
                        }
                        Some(_) => {}
                    }
                }
                "not" => {
                    if self.is_valid(v, inst) {
                        out.push(error(
                            "not",
                            path,
                            format!("{} is not allowed for {}", repr(v), repr(inst)),
                        ));
                    }
                }
                "minItems" => {
                    if let (Value::Array(items), Some(n)) = (inst, v.as_u64())
                        && (items.len() as u64) < n
                    {
                        out.push(error(
                            "minItems",
                            path,
                            format!("{} is too short", repr(inst)),
                        ));
                    }
                }
                "minProperties" => {
                    if let (Value::Object(o), Some(n)) = (inst, v.as_u64())
                        && (o.len() as u64) < n
                    {
                        out.push(error(
                            "minProperties",
                            path,
                            format!("{} does not have enough properties", repr(inst)),
                        ));
                    }
                }
                "maxProperties" => {
                    if let (Value::Object(o), Some(n)) = (inst, v.as_u64())
                        && (o.len() as u64) > n
                    {
                        out.push(error(
                            "maxProperties",
                            path,
                            format!("{} has too many properties", repr(inst)),
                        ));
                    }
                }
                "enum" => {
                    if let Value::Array(options) = v
                        && !options.contains(inst)
                    {
                        out.push(error(
                            "enum",
                            path,
                            format!("{} is not one of {}", repr(inst), repr(v)),
                        ));
                    }
                }
                "minLength" => {
                    if let (Value::String(text), Some(n)) = (inst, v.as_u64())
                        && (text.chars().count() as u64) < n
                    {
                        out.push(error(
                            "minLength",
                            path,
                            format!("{} is too short", repr(inst)),
                        ));
                    }
                }
                "minimum" => {
                    if let (Some(x), Some(m)) = (number(inst), v.as_f64())
                        && x < m
                    {
                        out.push(error(
                            "minimum",
                            path,
                            format!("{} is less than the minimum of {}", repr(inst), repr(v)),
                        ));
                    }
                }
                "uniqueItems" => {
                    if let (Value::Array(items), Value::Bool(true)) = (inst, v)
                        && items
                            .iter()
                            .enumerate()
                            .any(|(i, a)| items[i + 1..].contains(a))
                    {
                        out.push(error(
                            "uniqueItems",
                            path,
                            format!("{} has non-unique elements", repr(inst)),
                        ));
                    }
                }
                "if" => {
                    if self.is_valid(v, inst)
                        && let Some(then) = s.get("then")
                    {
                        self.descend(then, inst, path, out);
                    }
                }
                _ => {}
            }
        }
    }
}

/// A context error's parent and alternative, recorded on it and on what it
/// carries, as `jsonschema` sets `parent` and `schema_path`.
fn mark(e: &mut Error, parent_len: usize, branch: usize) {
    e.parent_len = parent_len;
    e.branch = Some(branch);
}

/// Every keyword known, every pattern compiled, every reference resolvable.
fn compile(
    schema: &Value,
    patterns: &mut BTreeMap<String, Regex>,
    at: &str,
) -> Result<(), SchemaError> {
    let Value::Object(s) = schema else {
        return match schema {
            Value::Bool(_) => Ok(()),
            _ => Err(SchemaError(format!(
                "{at}: a schema is an object or a boolean"
            ))),
        };
    };
    for (k, v) in s {
        if ANNOTATIONS.contains(&k.as_str()) {
            if k == "definitions"
                && let Value::Object(defs) = v
            {
                for (n, d) in defs {
                    compile(d, patterns, &format!("{at}/definitions/{n}"))?;
                }
            }
            continue;
        }
        if !KEYWORDS.contains(&k.as_str()) {
            return Err(SchemaError(format!(
                "{at}: the keyword {k}, which the core's validator does not implement (ADR-0156 decision 2)"
            )));
        }
        match (k.as_str(), v) {
            ("pattern", Value::String(p)) => {
                let re = Regex::new(p)
                    .map_err(|e| SchemaError(format!("{at}: the pattern {p}: {e}")))?;
                patterns.insert(p.clone(), re);
            }
            ("properties", Value::Object(props)) => {
                for (n, sub) in props {
                    compile(sub, patterns, &format!("{at}/properties/{n}"))?;
                }
            }
            ("oneOf" | "anyOf", Value::Array(alts)) => {
                for (i, alt) in alts.iter().enumerate() {
                    compile(alt, patterns, &format!("{at}/{k}/{i}"))?;
                }
            }
            ("additionalProperties" | "propertyNames" | "items" | "not" | "if" | "then", sub) => {
                compile(sub, patterns, &format!("{at}/{k}"))?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// `re.search` of an anchored pattern: Python's `$` also matches before a
/// final newline.
fn search(re: &Regex, text: &str) -> bool {
    re.is_match(text) || text.strip_suffix('\n').is_some_and(|t| re.is_match(t))
}

fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

/// `jsonschema`'s draft-07 types: a boolean is not a number, and a float
/// with no fraction is an integer.
fn is_type(v: &Value, t: &str) -> bool {
    match t {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        "number" => v.is_number(),
        "integer" => match v {
            Value::Number(n) => {
                n.is_i64() || n.is_u64() || n.as_f64().is_some_and(|f| f.fract() == 0.0)
            }
            _ => false,
        },
        _ => false,
    }
}

/// A value as Python's `repr` writes it, near enough for a message.
pub fn repr(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("'{s}'"),
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(repr).collect::<Vec<_>>().join(", ")
        ),
        Value::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| format!("'{k}': {}", repr(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// `jsonschema.exceptions.best_match` of 3.2.0: the most relevant error,
/// the shallowest and not a `oneOf` or `anyOf`, the first of equals; then,
/// while it has a context, the least relevant of that, the deepest.
pub fn best_match(errors: &[Error]) -> Option<&Error> {
    let mut best = errors.iter().fold(None::<&Error>, |acc, e| match acc {
        Some(b) if b.relevance() >= e.relevance() => Some(b),
        _ => Some(e),
    })?;
    while !best.context.is_empty() {
        best = best
            .context
            .iter()
            .fold(None::<&Error>, |acc, e| match acc {
                Some(b) if b.relevance() <= e.relevance() => Some(b),
                _ => Some(e),
            })?;
    }
    Some(best)
}

/// The schema's findings over a document: a path, the code `schema`, and a
/// message, each placed and worded as `check-flows.py`'s `schema_errors`
/// places it.
pub fn findings(schema: &Schema, doc: &Value) -> Vec<(Vec<Seg>, String)> {
    let mut out = Vec::new();
    for e in schema.errors(doc) {
        let instance = at(doc, &e.path);
        let kind = instance.and_then(|i| i.get("kind")).and_then(Value::as_str);
        let last = e.path.last();
        let key_is = |k: &str| matches!(last, Some(Seg::Key(x)) if x == k);
        let under_types =
            e.path.len() == 2 && matches!(e.path.first(), Some(Seg::Key(x)) if x == "types");
        if e.validator == "oneOf" && instance.is_some_and(Value::is_object) && kind.is_some() {
            let rule = match kind {
                Some("initial") => "an initial transition declares `to` and no `from`".to_string(),
                Some("external") => "an external transition declares `from` and `to`".to_string(),
                Some("internal") => "an internal transition declares `from` and no `to`".to_string(),
                Some("assertion") => "an assertion declares as `to` the states it may put an object in, and no `from`: it runs from any state".to_string(),
                Some("erasure") => "an erasure declares neither `from` nor `to`: it runs at any state".to_string(),
                _ => e.message.clone(),
            };
            out.push((e.path.clone(), rule));
        } else if e.validator == "oneOf" && under_types {
            out.push((
                e.path.clone(),
                "a type declares its own states and transitions, binds a state_machine, is abstract, or is a mirror with states, \
                 and only one of them; an abstract type has no lifecycle, conditions, observations or metrics"
                    .to_string(),
            ));
        } else if e.validator == "anyOf" && under_types {
            out.push((
                e.path.clone(),
                "a type declares its tracking, extends a base that does, or is abstract"
                    .to_string(),
            ));
        } else if e.validator == "oneOf" && key_is("from") {
            out.push((
                e.path.clone(),
                "`from` is a state, a list of states, or any, alone".to_string(),
            ));
        } else if e.validator == "oneOf" && key_is("unique") {
            out.push((
                e.path.clone(),
                "unique is true, in_scope, { with: [<attribute>, …] } or { where: <expression> }, one of them"
                    .to_string(),
            ));
        } else if e.validator == "oneOf"
            && e.path.len() == 4
            && matches!(e.path.get(2), Some(Seg::Key(x)) if x == "metrics")
        {
            // a metric is a fixed measure or a formula: the error of the form it chose
            let branch = usize::from(!instance.is_some_and(|i| i.get("measure").is_some()));
            let within: Vec<Error> = e
                .context
                .iter()
                .filter(|c| c.branch == Some(branch))
                .cloned()
                .collect();
            let plain: Vec<Error> = within
                .iter()
                .filter(|c| c.validator != "oneOf")
                .cloned()
                .collect();
            if !within.is_empty() && plain.is_empty() {
                out.push((
                    e.path.clone(),
                    "a median_time_in_state metric names a `state`, and a transition_count names a `transition`"
                        .to_string(),
                ));
            } else if let Some(best) =
                best_match(if plain.is_empty() { &e.context } else { &plain })
            {
                out.push((best.path.clone(), best.message.clone()));
            }
        } else if e.validator == "oneOf" && !e.context.is_empty() {
            // for a step, the error of the alternative its one key names
            let mut branch = None;
            if let Some(Value::Object(o)) = instance
                && o.len() == 1
                && let Some(alts) = schema_at(schema, &e)
                    .and_then(|s| s.get("oneOf"))
                    .and_then(Value::as_array)
            {
                for (n, alt) in alts.iter().enumerate() {
                    let req: Option<Vec<&str>> = alt
                        .get("required")
                        .and_then(Value::as_array)
                        .map(|r| r.iter().filter_map(Value::as_str).collect());
                    if req.is_some_and(|r| r == o.keys().map(String::as_str).collect::<Vec<_>>()) {
                        branch = Some(n);
                    }
                }
            }
            let within: Vec<Error> = e
                .context
                .iter()
                .filter(|c| branch.is_none() || c.branch == branch)
                .cloned()
                .collect();
            if let Some(best) = best_match(if within.is_empty() {
                &e.context
            } else {
                &within
            }) {
                out.push((best.path.clone(), best.message.clone()));
            }
        } else {
            out.push((e.path.clone(), e.message.clone()));
        }
    }
    out.into_iter().map(|(p, m)| (p, cut_value(&m))).collect()
}

/// An examples file's findings against its schema, as `check-flows.py`'s
/// `examples_schema_errors` places them.
pub fn examples_findings(schema: &Schema, doc: &Value) -> Vec<(Vec<Seg>, String)> {
    schema
        .errors(doc)
        .into_iter()
        .map(|e| {
            let request = at(doc, &e.path).is_some_and(|i| i.get("transition").is_some() && i.is_object());
            if e.validator == "oneOf" && request {
                (
                    e.path,
                    "a request names the `type` of a creation, with `as` for the alias it gives the object, \
                     or the `object` it acts on by its alias, not both"
                        .to_string(),
                )
            } else {
                (e.path, e.message)
            }
        })
        .collect()
}

/// The value at a path.
fn at<'v>(doc: &'v Value, path: &[Seg]) -> Option<&'v Value> {
    let mut v = doc;
    for seg in path {
        v = match (v, seg) {
            (Value::Object(o), Seg::Key(k)) => o.get(k)?,
            (Value::Array(a), Seg::Index(i)) => a.get(*i)?,
            _ => return None,
        };
    }
    Some(v)
}

/// The schema whose `oneOf` an error came from: found again by walking the
/// schema down the error's path, through references, properties, items and
/// alternatives, to a subschema with a `oneOf` the instance fails.
fn schema_at<'s>(schema: &'s Schema, e: &Error) -> Option<&'s Map<String, Value>> {
    let doc_path = &e.path;
    let mut frontier: Vec<&Value> = vec![schema.root()];
    for seg in doc_path {
        let mut next = Vec::new();
        for s in expand(schema, &frontier) {
            let Value::Object(o) = s else { continue };
            match seg {
                Seg::Key(k) => {
                    if let Some(sub) = o.get("properties").and_then(|p| p.get(k)) {
                        next.push(sub);
                    } else if let Some(sub @ Value::Object(_)) = o.get("additionalProperties") {
                        next.push(sub);
                    }
                }
                Seg::Index(_) => {
                    if let Some(sub) = o.get("items") {
                        next.push(sub);
                    }
                }
            }
        }
        frontier = next;
    }
    expand(schema, &frontier)
        .into_iter()
        .filter_map(Value::as_object)
        .find(|o| o.contains_key("oneOf"))
}

/// The schemas a set stands for once references, alternatives and a
/// conditional's `then` are followed.
fn expand<'s>(schema: &'s Schema, set: &[&'s Value]) -> Vec<&'s Value> {
    let mut out = Vec::new();
    let mut todo: Vec<&Value> = set.to_vec();
    while let Some(s) = todo.pop() {
        if let Some(r) = s.get("$ref").and_then(Value::as_str) {
            if let Some(t) = schema.resolve(r) {
                todo.push(t);
            }
            continue;
        }
        out.push(s);
        for k in ["oneOf", "anyOf"] {
            if let Some(alts) = s.get(k).and_then(Value::as_array) {
                todo.extend(alts.iter());
            }
        }
        if let Some(t) = s.get("then") {
            todo.push(t);
        }
    }
    out
}

/// A key that could not be a name, having a space or punctuation in it, is
/// the rest of a value an inline mapping cut at a comma (`flow-format.md` §2
/// rule 4).
pub fn cut_value(message: &str) -> String {
    let Some(rest) = message.strip_prefix("Additional properties are not allowed ('") else {
        return message.to_string();
    };
    let Some(key) = rest.strip_suffix("' was unexpected)") else {
        return message.to_string();
    };
    let name = key
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if name || key.contains('\'') {
        return message.to_string();
    }
    format!(
        "'{key}' is not a key but the rest of a value cut at a comma: inside {{ … }}, \
         a value containing a comma is quoted, as in {{ description: \"One, two.\" }} (§2 rule 4)"
    )
}

/// A document as the reference checker's strict loader types it: YAML 1.1's
/// implicit resolvers, PyYAML 5.4.1's, but that only a plain `true` or
/// `false` is a boolean and a timestamp stays text (`check-flows.py`'s
/// `StrictLoader`).
pub fn typed(node: &Node) -> Value {
    match node {
        Node::Scalar {
            text, plain: true, ..
        } => plain_scalar(text),
        Node::Scalar { text, .. } => Value::String(text.clone()),
        Node::Seq { items, .. } => Value::Array(items.iter().map(typed).collect()),
        Node::Map { entries, .. } => {
            Value::Object(entries.iter().map(|(k, v)| (k.clone(), typed(v))).collect())
        }
    }
}

fn plain_scalar(text: &str) -> Value {
    if matches!(text, "" | "~" | "null" | "Null" | "NULL") {
        return Value::Null;
    }
    match text {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }
    if let Some(n) = yaml11_int(text) {
        return Value::from(n);
    }
    if let Some(f) = yaml11_float(text)
        && let Some(n) = serde_json::Number::from_f64(f)
    {
        return Value::Number(n);
    }
    Value::String(text.to_string())
}

/// PyYAML's `int` resolver and constructor: binary, octal with a leading
/// zero, decimal, hexadecimal and base 60, underscores ignored.
fn yaml11_int(text: &str) -> Option<i64> {
    let (sign, body) = match text.as_bytes().first()? {
        b'-' => (-1, &text[1..]),
        b'+' => (1, &text[1..]),
        _ => (1, text),
    };
    let digits = |s: &str, radix: u32| -> Option<i64> {
        let clean: String = s.chars().filter(|c| *c != '_').collect();
        if clean.is_empty() {
            return None;
        }
        i64::from_str_radix(&clean, radix).ok()
    };
    let value = if let Some(b) = body.strip_prefix("0b") {
        if !b.chars().all(|c| matches!(c, '0' | '1' | '_')) {
            return None;
        }
        digits(b, 2)?
    } else if let Some(h) = body.strip_prefix("0x") {
        if !h.chars().all(|c| c.is_ascii_hexdigit() || c == '_') {
            return None;
        }
        digits(h, 16)?
    } else if body.len() > 1 && body.starts_with('0') {
        if !body.chars().all(|c| matches!(c, '0'..='7' | '_')) {
            return None;
        }
        digits(body, 8)?
    } else if body.contains(':') {
        let mut parts = body.split(':');
        let head = parts.next()?;
        if !head.starts_with(|c: char| matches!(c, '1'..='9'))
            || !head.chars().all(|c| c.is_ascii_digit() || c == '_')
        {
            return None;
        }
        let mut v = digits(head, 10)?;
        for p in parts {
            if p.is_empty() || p.len() > 2 || !p.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            if p.len() == 2 && !matches!(p.as_bytes()[0], b'0'..=b'5') {
                return None;
            }
            v = v.checked_mul(60)?.checked_add(p.parse::<i64>().ok()?)?;
        }
        v
    } else {
        let ok = body == "0"
            || (body.starts_with(|c: char| matches!(c, '1'..='9'))
                && body.chars().all(|c| c.is_ascii_digit() || c == '_'));
        if !ok {
            return None;
        }
        digits(body, 10)?
    };
    value.checked_mul(sign)
}

/// PyYAML's `float` resolver and constructor.
fn yaml11_float(text: &str) -> Option<f64> {
    let (sign, body) = match text.as_bytes().first()? {
        b'-' => (-1.0, &text[1..]),
        b'+' => (1.0, &text[1..]),
        _ => (1.0, text),
    };
    match body {
        ".inf" | ".Inf" | ".INF" => return Some(sign * f64::INFINITY),
        ".nan" | ".NaN" | ".NAN" if sign > 0.0 && text == body => return Some(f64::NAN),
        _ => {}
    }
    let clean: String = body.chars().filter(|c| *c != '_').collect();
    if body.contains(':') {
        // base 60: [0-9][0-9_]*(:[0-5]?[0-9])+\.[0-9_]*
        let (whole, frac) = body.split_once('.')?;
        let mut parts = whole.split(':');
        let head = parts.next()?;
        if !head.starts_with(|c: char| c.is_ascii_digit())
            || !head.chars().all(|c| c.is_ascii_digit() || c == '_')
            || !frac.chars().all(|c| c.is_ascii_digit() || c == '_')
        {
            return None;
        }
        let mut v: f64 = head.replace('_', "").parse().ok()?;
        let mut seen = false;
        for p in parts {
            if p.is_empty() || p.len() > 2 || !p.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            if p.len() == 2 && !matches!(p.as_bytes()[0], b'0'..=b'5') {
                return None;
            }
            v = v * 60.0 + p.parse::<f64>().ok()?;
            seen = true;
        }
        if !seen {
            return None;
        }
        let f: f64 = format!("0.{}", frac.replace('_', ""))
            .parse()
            .unwrap_or(0.0);
        return Some(sign * (v * 60.0_f64.powi(0) + f));
    }
    // [0-9][0-9_]*\.[0-9_]*([eE][-+][0-9]+)?  or  \.[0-9_]+([eE][-+][0-9]+)?
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let (int, frac) = mantissa.split_once('.')?;
    let int_ok = if int.is_empty() {
        !frac.is_empty()
            && frac.starts_with(|c: char| c.is_ascii_digit() || c == '_')
            && text == body
    } else {
        int.starts_with(|c: char| c.is_ascii_digit())
            && int.chars().all(|c| c.is_ascii_digit() || c == '_')
    };
    if !int_ok || !frac.chars().all(|c| c.is_ascii_digit() || c == '_') {
        return None;
    }
    if let Some(x) = exponent {
        let digits = x.strip_prefix(['-', '+'])?;
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
    }
    let v: f64 = clean
        .parse()
        .ok()
        .or_else(|| format!("0{clean}").parse().ok())?;
    Some(sign * v)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_repositorys_schemas_compile() {
        Schema::flow().unwrap();
        Schema::examples().unwrap();
    }

    #[test]
    fn a_keyword_the_validator_lacks_refuses_the_schema() {
        let err = Schema::new(json!({"type": "object", "patternProperties": {}})).unwrap_err();
        assert!(err.0.contains("patternProperties"), "{}", err.0);
    }

    #[test]
    fn scalars_are_typed_as_pyyaml_types_them() {
        let doc = crate::yaml::load(
            "a: 0x1F\nb: 010\nc: 1:30\nd: 1_000\ne: 12.50\nf: .5\ng: 1e3\nh: 1.0e+3\ni: Null\nj: '3'\nk: NO\nl: -0\nm: 0o7\n",
        )
        .unwrap();
        let v = typed(&doc);
        assert_eq!(v["a"], json!(31));
        assert_eq!(v["b"], json!(8));
        assert_eq!(v["c"], json!(90));
        assert_eq!(v["d"], json!(1000));
        assert_eq!(v["e"], json!(12.5));
        assert_eq!(v["f"], json!(0.5));
        assert_eq!(v["g"], json!("1e3"));
        assert_eq!(v["h"], json!(1000.0));
        assert_eq!(v["i"], Value::Null);
        assert_eq!(v["j"], json!("3"));
        assert_eq!(v["k"], json!("NO"));
        assert_eq!(v["l"], json!(0));
        assert_eq!(v["m"], json!("0o7"));
    }

    #[test]
    fn best_match_prefers_the_shallow_then_descends_to_the_deep() {
        let schema = Schema::new(json!({"type": "object", "properties": {"steps": {"type": "array", "items": {"oneOf": [
            {"type": "object", "required": ["assign"], "properties": {"assign": {"type": "object", "required": ["location"]}}},
            {"type": "object", "required": ["clear"]}]}}}}))
        .unwrap();
        let doc = json!({"steps": [{"assign": {"expr": "1"}}]});
        let errors = schema.errors(&doc);
        assert_eq!(errors.len(), 1);
        let best = best_match(&errors[0].context).unwrap();
        assert_eq!(best.message, "'clear' is a required property");
        // the step's own alternative, as schema_errors chooses it
        let found = findings(&schema, &doc);
        assert_eq!(
            found[0].0,
            vec![
                Seg::Key("steps".into()),
                Seg::Index(0),
                Seg::Key("assign".into())
            ]
        );
    }
}

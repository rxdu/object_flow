//! The core's checker (ADR-0156): what `scripts/check-flows.py` checks of a
//! flow description, run by the engine, held to the Python checker's
//! findings over every design module and every plant of its self-test.
//!
//! It runs the checker's steps 1 to 4: each file loaded strictly, checked
//! against the schema, its YAML literals given their expression text, every
//! expression parsed, its imports and the names its closure declares, the
//! names and relationships of the module, with its migration's rules and its
//! checks against the previous version, and the sixty-four checks of
//! `declaration-syntax.md` §10 on the core's reading of the modules. The
//! contradictions within a type and the examples follow (`TODO.md`,
//! `docs/design/core-checker.md`).

mod data;
mod doc;
mod lines;
mod migration;
mod model;
mod names;
mod notices;
mod optional;
mod raw;
pub mod schema;
mod steps;
mod view;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::yaml::Seg;
use schema::Schema;

/// Fatal, refusing a publish, or a notice, reported beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Fatal,
    Notice,
}

impl Severity {
    pub fn text(self) -> &'static str {
        match self {
            Severity::Fatal => "fatal",
            Severity::Notice => "notice",
        }
    }
}

/// One finding: the file, the line, its severity, its code and what it says.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub file: String,
    pub line: usize,
    pub severity: Severity,
    pub code: String,
    pub message: String,
}

/// The kinds of declaration a module names, by the key that holds them.
const DECLARES: &[&str] = &[
    "enumerations",
    "sequences",
    "evaluators",
    "machines",
    "types",
];

/// The modules and examples files given, in import order, each with its
/// name; `previous` maps a file to the text of the version before it.
pub fn check(files: &[(String, String)], previous: &BTreeMap<String, String>) -> Vec<Finding> {
    let mut found = Vec::new();
    let schema = match Schema::flow() {
        Ok(s) => s,
        Err(e) => {
            found.push(Finding {
                file: "flow.schema.json".into(),
                line: 1,
                severity: Severity::Fatal,
                code: "schema".into(),
                message: e.0,
            });
            return found;
        }
    };
    let examples_schema = match Schema::examples() {
        Ok(s) => s,
        Err(e) => {
            found.push(Finding {
                file: "examples.schema.json".into(),
                line: 1,
                severity: Severity::Fatal,
                code: "schema".into(),
                message: e.0,
            });
            return found;
        }
    };
    let mut examples_files = Vec::new();
    // each module that reached the end of step 3, for step 4: its file's
    // index, its tree and its document
    let mut modules: Vec<(usize, crate::yaml::Node, Value)> = Vec::new();
    let mut parsed = 0usize;
    let mut declared: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // every type of the modules checked so far, as the view holds it
    let mut library = doc::Obj::new();
    let mut uses: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (index, (file, text)) in files.iter().enumerate() {
        let fatal = |found: &mut Vec<Finding>, line: usize, code: &str, message: String| {
            found.push(Finding {
                file: file.clone(),
                line,
                severity: Severity::Fatal,
                code: code.into(),
                message,
            });
        };
        let node = match crate::yaml::load(text) {
            Ok(n) => n,
            Err(e) => {
                fatal(&mut found, e.line, "yaml", e.message);
                continue;
            }
        };
        parsed += 1;
        let mut doc = schema::typed(&node);
        if doc.get("examples_for").is_some() {
            // an examples file is checked once every module is (flow-format.md §11)
            examples_files.push((file.clone(), node, doc));
            continue;
        }
        let errors = schema::findings(&schema, &doc);
        if !errors.is_empty() {
            for (path, message) in errors {
                fatal(&mut found, node.line_at(&path), "schema", message);
            }
            continue;
        }
        literals(&schema, &mut doc);
        let mut bad = false;
        for (path, text) in expressions(&schema, &doc) {
            if let Err(e) = crate::expr::parse(&text) {
                bad = true;
                fatal(
                    &mut found,
                    node.line_at(&path),
                    "syntax",
                    format!("{e}: not an expression of declaration-syntax.md §8"),
                );
            }
        }
        if bad {
            continue;
        }
        let module = doc
            .get("module")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let imports: Vec<(String, Vec<String>)> = doc
            .get("imports")
            .and_then(Value::as_object)
            .map(|o| {
                o.iter()
                    .map(|(m, names)| {
                        let names = names
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_string)
                                    .collect()
                            })
                            .unwrap_or_default();
                        (m.clone(), names)
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (m, names) in &imports {
            let at = node.line_at(&[Seg::Key("imports".into()), Seg::Key(m.clone())]);
            let Some(theirs) = declared.get(m) else {
                found.push(Finding {
                    file: file.clone(),
                    line: at,
                    severity: Severity::Notice,
                    code: "imports".into(),
                    message: format!(
                        "{m} is not among the files checked, so steps 2 and 3 cannot check what is imported from it"
                    ),
                });
                continue;
            };
            for n in names {
                if !theirs.contains(n) {
                    fatal(
                        &mut found,
                        at,
                        "names",
                        format!("imports '{n}' from {m}, which does not declare it"),
                    );
                }
            }
        }
        // a name is declared once across the closure: the modules this one
        // imports, and theirs (the model's check 33)
        let mut mine: BTreeMap<String, &str> = BTreeMap::new();
        for kind in DECLARES {
            if let Some(o) = doc.get(*kind).and_then(Value::as_object) {
                for n in o.keys() {
                    mine.insert(n.clone(), kind);
                }
            }
        }
        let mut closure = BTreeSet::new();
        let mut todo: Vec<String> = imports
            .iter()
            .map(|(m, _)| m.clone())
            .filter(|m| declared.contains_key(m))
            .collect();
        while let Some(m) = todo.pop() {
            if closure.insert(m.clone()) {
                todo.extend(
                    uses.get(&m)
                        .into_iter()
                        .flatten()
                        .filter(|n| declared.contains_key(*n))
                        .cloned(),
                );
            }
        }
        for m in &closure {
            let theirs = &declared[m];
            for (n, kind) in &mine {
                if theirs.contains(n) {
                    fatal(
                        &mut found,
                        node.line_at(&[Seg::Key((*kind).to_string()), Seg::Key(n.clone())]),
                        "names",
                        format!(
                            "{n} is also declared by {m}, which is in this module's closure; a name is declared once across the closure (the model's check 33)"
                        ),
                    );
                }
            }
        }
        declared.insert(module.clone(), mine.keys().cloned().collect());
        uses.insert(module, imports.into_iter().map(|(m, _)| m).collect());
        // step 3: the module's names and relationships, over its view
        let v = view::View::new(&doc, &library);
        let notice = |found: &mut Vec<Finding>, line: usize, code: String, message: String| {
            found.push(Finding {
                file: file.clone(),
                line,
                severity: Severity::Notice,
                code,
                message,
            });
        };
        let (pair, pair_notes) = match previous.get(file).and_then(|t| crate::yaml::load(t).ok()) {
            Some(prev) => migration::migration_pair_errors(&schema::typed(&prev), &doc, &library),
            None => (Vec::new(), Vec::new()),
        };
        for (p, code, message) in pair_notes
            .into_iter()
            .chain(migration::final_admission_notes(&doc, &v.doc))
        {
            notice(&mut found, node.line_at(&p), code, message);
        }
        let view_types = doc::obj(&v.doc, "types").clone();
        let fatal_found = raw::order_errors(&doc)
            .into_iter()
            .chain(raw::family_errors(&doc, &library))
            .chain(raw::machine_errors(&doc, &view_types))
            .chain(raw::decimal_errors(&doc))
            .chain(raw::type_name_errors(&doc))
            .chain(names::name_errors(&v.doc, &library))
            .chain(migration::migration_errors(&doc, &v.doc))
            .chain(pair);
        let mut seen: BTreeSet<(Vec<Seg>, String)> = BTreeSet::new();
        for (p, code, mut message) in fatal_found {
            let Some(p) = v.back(&p) else { continue };
            if let (Some(Seg::Key(first)), Some(Seg::Key(name))) = (p.first(), p.get(1))
                && first == "machines"
                && message.contains('\'')
            {
                // a machine's attributes are the ones it requires of its binders
                message = message.replace(
                    &format!(", which {name} does not declare"),
                    &format!(", which {name} does not require"),
                );
            }
            if seen.insert((p.clone(), message.clone())) {
                fatal(&mut found, node.line_at(&p), &code, message);
            }
        }
        for (tn, t) in view_types {
            library.insert(tn, t);
        }
        for (p, code, message) in notices::notices(&v.doc)
            .into_iter()
            .chain(notices::unknowns(&v.doc, &library))
            .chain(notices::befores(&v.doc))
        {
            if let Some(p) = v.back(&p) {
                notice(&mut found, node.line_at(&p), code, message);
            }
        }
        modules.push((index, node, doc));
    }
    let all_parsed = parsed == files.len();
    // the shape of each examples file (publish-and-import.md §1 step 1)
    for (file, node, doc) in &examples_files {
        for (path, message) in schema::examples_findings(&examples_schema, doc) {
            found.push(Finding {
                file: file.clone(),
                line: node.line_at(&path),
                severity: Severity::Fatal,
                code: "schema".into(),
                message,
            });
        }
    }
    // step 4, the sixty-four checks, once no file has a fatal finding and
    // every file loaded, over every module together (ADR-0156 decision 1)
    if all_parsed && !found.iter().any(|f| f.severity == Severity::Fatal) {
        let docs: Vec<Value> = modules.iter().map(|(_, _, d)| d.clone()).collect();
        let mut fours: Vec<(usize, usize, u32, String)> = model::checks(&docs)
            .into_iter()
            .map(|f| {
                let (index, node, _) = &modules[f.module];
                (*index, node.line_at(&f.path), f.check, f.message)
            })
            .collect();
        // the text checker reports a check's words once, at the first line
        fours.sort();
        let mut said = BTreeSet::new();
        for (index, line, check, message) in fours {
            if said.insert((check, message.clone())) {
                found.push(Finding {
                    file: files[index].0.clone(),
                    line,
                    severity: Severity::Fatal,
                    code: format!("check {check}"),
                    message,
                });
            }
        }
    }
    found
}

/// The schemas a schema stands for: itself, or the expression it refers to,
/// with its references followed and its alternatives and conditional
/// branches added, as `check-flows.py`'s `branches` follows them.
fn branches<'s>(schema: &'s Schema, sch: &'s Value) -> Vec<&'s Value> {
    if let Some(r) = sch.get("$ref").and_then(Value::as_str) {
        if r == EXPRESSION {
            return vec![sch];
        }
        return schema
            .resolve(r)
            .map(|t| branches(schema, t))
            .unwrap_or_default();
    }
    let mut out = vec![sch];
    for k in ["anyOf", "oneOf", "allOf"] {
        for b in sch.get(k).and_then(Value::as_array).into_iter().flatten() {
            out.extend(branches(schema, b));
        }
    }
    for k in ["then", "else"] {
        if let Some(b) = sch.get(k) {
            out.extend(branches(schema, b));
        }
    }
    out
}

const EXPRESSION: &str = "#/definitions/expression";

/// The schemas for a mapping's key or a sequence's items, under a set.
fn inner<'s>(schema: &'s Schema, schemas: &[&'s Value], key: Option<&str>) -> Vec<&'s Value> {
    let mut out = Vec::new();
    for sch in schemas {
        match key {
            Some(k) => {
                if let Some(sub) = sch.get("properties").and_then(|p| p.get(k)) {
                    out.extend(branches(schema, sub));
                } else if let Some(sub @ Value::Object(_)) = sch.get("additionalProperties") {
                    out.extend(branches(schema, sub));
                }
            }
            None => {
                if let Some(sub @ Value::Object(_)) = sch.get("items") {
                    out.extend(branches(schema, sub));
                }
            }
        }
    }
    out
}

fn is_expression(schemas: &[&Value]) -> bool {
    schemas
        .iter()
        .any(|s| s.get("$ref").and_then(Value::as_str) == Some(EXPRESSION))
}

fn yaml_literal(v: &Value) -> bool {
    v.is_boolean() || v.is_number()
}

/// Each expression written as YAML's own boolean or number given its
/// literal (`flow-format.md` §5), where the schema expects an expression and
/// no alternative takes the value as YAML's own, as `final: true` and
/// `limit: 50` are (`check-flows.py`'s `literals`).
fn literals(schema: &Schema, doc: &mut Value) {
    fn native(sch: &Value) -> bool {
        let typed = match sch.get("type") {
            Some(Value::String(t)) => matches!(t.as_str(), "boolean" | "integer" | "number"),
            Some(Value::Array(ts)) => ts
                .iter()
                .any(|t| matches!(t.as_str(), Some("boolean" | "integer" | "number"))),
            _ => false,
        };
        typed
            || sch.get("const").is_some_and(yaml_literal)
            || sch
                .get("enum")
                .and_then(Value::as_array)
                .is_some_and(|e| e.iter().any(yaml_literal))
    }
    fn walk(schema: &Schema, node: &mut Value, schemas: &[&Value]) {
        let convert = |v: &Value, inner: &[&Value]| {
            yaml_literal(v) && is_expression(inner) && !inner.iter().any(|s| native(s))
        };
        match node {
            Value::Object(o) => {
                for (k, v) in o.iter_mut() {
                    let sub = inner(schema, schemas, Some(k));
                    if convert(v, &sub) {
                        *v = Value::String(literal(v));
                    } else {
                        walk(schema, v, &sub);
                    }
                }
            }
            Value::Array(items) => {
                let sub = inner(schema, schemas, None);
                for v in items.iter_mut() {
                    if convert(v, &sub) {
                        *v = Value::String(literal(v));
                    } else {
                        walk(schema, v, &sub);
                    }
                }
            }
            _ => {}
        }
    }
    let top = branches(schema, schema.root());
    walk(schema, doc, &top);
}

/// A boolean or a number as Python's `str` writes it, which is the literal
/// the reference checker gives it.
fn literal(v: &Value) -> String {
    match v {
        Value::Bool(true) => "true".into(),
        Value::Bool(false) => "false".into(),
        Value::Number(n) if n.is_i64() || n.is_u64() => n.to_string(),
        Value::Number(n) => n.as_f64().map_or_else(|| n.to_string(), py_float),
        other => other.to_string(),
    }
}

/// Python's `repr` of a float: the shortest digits that round-trip, with
/// `.0` on a whole number and an exponent below 1e-4 or from 1e16.
fn py_float(f: f64) -> String {
    let a = f.abs();
    if a != 0.0 && !(1e-4..1e16).contains(&a) {
        let s = format!("{f:e}");
        let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
        let e: i32 = e.parse().unwrap_or(0);
        return format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs());
    }
    let s = format!("{f}");
    if s.contains('.') { s } else { format!("{s}.0") }
}

/// Every expression in a description, with its path: the positions the
/// schema marks as expressions (`flowexpr.expressions`).
fn expressions(schema: &Schema, doc: &Value) -> Vec<(Vec<Seg>, String)> {
    fn walk(
        schema: &Schema,
        node: &Value,
        schemas: &[&Value],
        path: &mut Vec<Seg>,
        out: &mut Vec<(Vec<Seg>, String)>,
    ) {
        if is_expression(schemas) && (node.is_string() || yaml_literal(node)) {
            let text = node.as_str().map_or_else(|| literal(node), str::to_string);
            out.push((path.clone(), text));
            return;
        }
        match node {
            Value::Object(o) => {
                for (k, v) in o {
                    let sub = inner(schema, schemas, Some(k));
                    path.push(Seg::Key(k.clone()));
                    walk(schema, v, &sub, path, out);
                    path.pop();
                }
            }
            Value::Array(items) => {
                let sub = inner(schema, schemas, None);
                for (i, v) in items.iter().enumerate() {
                    path.push(Seg::Index(i));
                    walk(schema, v, &sub, path, out);
                    path.pop();
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    let top = branches(schema, schema.root());
    walk(schema, doc, &top, &mut Vec::new(), &mut out);
    out
}

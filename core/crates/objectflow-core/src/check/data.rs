//! Step 4's data checks, 54 to 62 and 17's decimals (`check-syntax-doc.py`'s
//! `data_checks`): observation kinds, metrics, what a transition's clauses
//! read, assignees and actor identities, and erasure.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::doc::{Path, ext, flag, is_word, obj, py_str, re, segs, squash, str_or, strings};
use super::model::{Decl, DeclKind, Found4, Head, RelKind, add, stores};
use super::steps::{step_expressions, walk};
use super::view::types;

/// The members every observation has, which no field may name.
const GENERATED: &[&str] = &[
    "subject",
    "corrects",
    "occurred_at",
    "recorded_at",
    "recorded_by_kind",
    "created_at",
    "created_by_kind",
];
/// The sources of a type's flow data.
const SOURCES: &[&str] = &[
    "labels",
    "intervals",
    "transitions",
    "attempts",
    "attempt_counts",
];

/// An observation kind, as the text's `observation <K> on <T> as <c>` holds it.
pub struct ObsD {
    pub name: String,
    pub module: usize,
    pub path: Path,
    pub fields: Vec<(String, Value)>,
    pub invariants: Vec<(String, String)>,
    pub delay: Option<String>,
}

/// One clause of a metric: its text and the YAML path its line stands for.
type Clause = (String, Path);

/// A metric, as the text's `metric <m>` block holds its clauses.
pub struct MetricD {
    pub name: String,
    pub module: usize,
    pub path: Path,
    pub from: Option<Clause>,
    pub by: Option<Clause>,
    pub window: bool,
    pub value: Option<Clause>,
    pub flags: Vec<Clause>,
    pub combine: Vec<String>,
}

/// A type's attribute as the text's `attr` line writes its type: the type,
/// `?` where optional.
fn first_token(spec: &Value) -> String {
    format!(
        "{}{}",
        str_or(spec, "type", ""),
        if flag(spec, "optional") { "?" } else { "" }
    )
}

/// Whether a member is tracked, as the text checker decides it: an `attr`
/// whose type is a name in capitals and not a set, or a stored single end.
pub fn tracked(d: &Decl, member: &str, world: &BTreeMap<&str, &Decl>) -> bool {
    if let Some((_, spec)) = d.attrs.iter().find(|(a, _)| a == member) {
        let t = first_token(spec);
        return t.chars().next().is_some_and(char::is_uppercase) && !t.contains("[]");
    }
    if let Some(r) = d.rels.iter().find(|r| r.name == member) {
        return matches!(r.kind, RelKind::Ref | RelKind::Owner)
            && (r.kind == RelKind::Owner || stores(r, world));
    }
    false
}

/// The observation kinds and metrics of the modules.
pub fn kinds_and_metrics(docs: &[Value]) -> (Vec<ObsD>, Vec<MetricD>) {
    let (mut kinds, mut metrics) = (Vec::new(), Vec::new());
    for (module, doc) in docs.iter().enumerate() {
        for (tn, t) in types(doc) {
            for (coll, ob) in obj(t, "observations") {
                let at = segs!["types", &tn, "observations", coll];
                kinds.push(ObsD {
                    name: str_or(ob, "kind", ""),
                    module,
                    path: at,
                    fields: obj(ob, "attributes")
                        .iter()
                        .map(|(a, s)| (a.clone(), s.clone()))
                        .collect(),
                    invariants: obj(ob, "invariants")
                        .iter()
                        .map(|(n, i)| (n.clone(), squash(&str_or(i, "expression", ""))))
                        .collect(),
                    delay: ob
                        .get("max_recording_delay")
                        .filter(|d| super::doc::truthy(Some(d)))
                        .map(py_str),
                });
            }
            for (mn, m) in obj(t, "metrics") {
                metrics.push(type_metric(&tn, t, mn, m, module));
            }
        }
        for (mn, m) in obj(doc, "metrics") {
            let at = segs!["metrics", mn];
            let combine: Vec<String> = obj(m, "input_metrics")
                .values()
                .map(|r| py_str(r).rsplit('.').next().unwrap_or_default().to_string())
                .collect();
            let group_by = strings(m, "group_by");
            metrics.push(MetricD {
                name: mn.clone(),
                module,
                path: at.clone(),
                from: None,
                by: (!group_by.is_empty())
                    .then(|| (group_by.join(", "), ext(&at, segs!["group_by"]))),
                window: false,
                value: Some((
                    squash(&str_or(m, "expression", "")),
                    ext(&at, segs!["expression"]),
                )),
                flags: obj(m, "flag_when")
                    .iter()
                    .map(|(f, c)| {
                        (
                            format!("{f} when {}", squash(&py_str(c))),
                            ext(&at, segs!["flag_when", f]),
                        )
                    })
                    .collect(),
                combine,
            });
        }
    }
    (kinds, metrics)
}

/// A type's metric, its clauses as `metric_text` writes them.
fn type_metric(tn: &str, t: &Value, mn: &str, m: &Value, module: usize) -> MetricD {
    let at = segs!["types", tn, "metrics", mn];
    let flags: Vec<Clause> = obj(m, "flag_when")
        .iter()
        .map(|(f, c)| {
            (
                format!("{f} when {}", squash(&py_str(c))),
                ext(&at, segs!["flag_when", f]),
            )
        })
        .collect();
    if m.get("source").is_some() {
        let src = str_or(m, "source", "");
        let rows = if src == "objects" {
            tn.to_string()
        } else if let Some(ob) = obj(t, "observations").get(&src) {
            str_or(ob, "kind", "")
        } else {
            format!("{tn}.{src}")
        };
        let mut from = format!("{} in {rows}", str_or(m, "item", ""));
        if let Some(s) = m.get("split_by").filter(|s| super::doc::truthy(Some(s))) {
            from.push_str(&format!(" split by {}", py_str(s)));
        }
        if let Some(f) = m.get("filter").filter(|f| super::doc::truthy(Some(f))) {
            from.push_str(&format!(" where {}", squash(&py_str(f))));
        }
        let dims = obj(m, "dimensions");
        return MetricD {
            name: mn.to_string(),
            module,
            path: at.clone(),
            from: Some((from, ext(&at, segs!["source"]))),
            by: (!dims.is_empty()).then(|| {
                let text: Vec<String> = dims
                    .iter()
                    .map(|(d, e)| format!("{d} = {}", squash(&py_str(e))))
                    .collect();
                (text.join(", "), ext(&at, segs!["dimensions"]))
            }),
            window: m
                .get("time_dimension")
                .is_some_and(|w| super::doc::truthy(Some(w))),
            value: Some((
                squash(&str_or(m, "expression", "")),
                ext(&at, segs!["expression"]),
            )),
            flags,
            combine: Vec::new(),
        };
    }
    // a fixed measure, as the text writes it out
    let (v, time, from, from_path) = if str_or(m, "measure", "") == "median_time_in_state" {
        (
            "i",
            "i.entered_at",
            format!(
                "i in {tn}.intervals where i.state == {tn}.{}",
                str_or(m, "state", "")
            ),
            segs!["state"],
        )
    } else {
        let cond = format!(
            "t.transition == \"{}\" and not t.imported and not t.migrated and not t.redacted",
            str_or(m, "transition", "")
        );
        (
            "t",
            "t.occurred_at",
            format!("t in {tn}.transitions where {cond}"),
            segs!["transition"],
        )
    };
    let dims: Vec<String> = strings(m, "group_by")
        .iter()
        .map(|d| {
            let e = match d.as_str() {
                "month" => format!("month({time})"),
                "week" => format!("week({time})"),
                "actor" => "t.actor_id".to_string(),
                _ if tracked_yaml(t, d) => format!("{v}.held({d})"),
                _ => format!("{v}.object.{d}"),
            };
            format!("{d} = {e}")
        })
        .collect();
    MetricD {
        name: mn.to_string(),
        module,
        path: at.clone(),
        from: Some((from, ext(&at, from_path))),
        by: (!dims.is_empty()).then(|| (dims.join(", "), ext(&at, segs!["group_by"]))),
        window: true,
        value: Some((
            if v == "i" {
                "median(i.duration)"
            } else {
                "count()"
            }
            .to_string(),
            at.clone(),
        )),
        flags,
        combine: Vec::new(),
    }
}

/// `tracked_attributes` of `check-flows.py`: an enumeration attribute, or a
/// single reference that stores its value, read from the type's YAML.
fn tracked_yaml(t: &Value, a: &str) -> bool {
    let Some(spec) = obj(t, "attributes").get(a) else {
        return false;
    };
    let typ = str_or(spec, "type", "");
    let reference = str_or(spec, "reference", "");
    if typ.chars().next().is_some_and(char::is_uppercase) && !typ.ends_with("[]") {
        return true;
    }
    !reference.is_empty()
        && !reference.ends_with("[]")
        && (spec.get("opposite").is_none() || flag(spec, "stored"))
}

static UNIT_DURATION: LazyLock<Regex> = LazyLock::new(|| re(r"^\d+\s*(?:s|min|h|days|weeks)$"));
static BACKDATABLE: LazyLock<Regex> =
    LazyLock::new(|| re(r"\bbackdatable\s+within\s+\d+\s*(?:s|min|h|days|weeks)\b"));
static FROM: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(\w+)\s+in\s+(\w+)(?:\.(\w+)(?:\((\w+)\))?)?"));
/// Whether a kind's invariant, parsed, reads more than its own fields: an
/// aggregate, a type it scans, a metric, or a call but `length` (check 54,
/// ADR-0164). One that does not parse is left to the checks that say so.
fn beyond_own_fields(expr: &str) -> bool {
    fn reaches(e: &crate::expr::Expr) -> bool {
        use crate::expr::Expr;
        match e {
            Expr::Agg { .. } | Expr::Metric { .. } => true,
            Expr::Call(f, _) if !matches!(f.as_ref(), Expr::Name(n) if n == "length") => true,
            _ => e.children().into_iter().any(reaches),
        }
    }
    crate::expr::parse(expr.trim()).is_ok_and(|e| reaches(&e))
}

static OUTSIDE: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b(subject|actor|inputs|now|this|referrers)\b|\b[a-z_]\w*\.\w"));
static METRIC_REF: LazyLock<Regex> = LazyLock::new(|| re(r"\bmetric\(\s*(\w+)\s*((?:,[^()]*)?)\)"));
static TIME_IN: LazyLock<Regex> = LazyLock::new(|| re(r"\btime_in\(\s*(\w+)\s*\)"));
static ENTERED_AT: LazyLock<Regex> = LazyLock::new(|| re(r"\bentered_at\(\s*(\w+)\s*\)"));
static BOUND: LazyLock<Regex> = LazyLock::new(|| re(r"(\w+)\s*:="));
static EVALUATOR_ARGS: LazyLock<Regex> = LazyLock::new(|| re(r"\b(\w+)\.\w+\(([^)]*)\)"));
static METRIC_ARGS: LazyLock<Regex> = LazyLock::new(|| re(r"\bmetric\(([^)]*)\)"));
static OBSERVE: LazyLock<Regex> = LazyLock::new(|| re(r"\bobserve\b"));
static FLAG: LazyLock<Regex> = LazyLock::new(|| re(r"\bflag\b"));

/// Whether a text has `(?<![\w.])(?:this\.)?<name>\b`: the name read as a
/// member, bare or through `this`.
fn names_word(text: &str, name: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let want: Vec<char> = name.chars().collect();
    (0..chars.len()).any(|i| {
        let mut start = i;
        let this: Vec<char> = "this.".chars().collect();
        if chars[i..].starts_with(&this) {
            start = i + this.len();
        }
        let before_ok = i == 0 || !(is_word(chars[i - 1]) || chars[i - 1] == '.');
        before_ok
            && chars[start..].starts_with(&want)
            && !chars.get(start + want.len()).is_some_and(|c| is_word(*c))
    })
}

/// A transition's guards as the text's `require` clauses write them, with
/// the conditions of its type, its machine's merged in.
pub fn require_clauses(
    x: &Value,
    conditions: &serde_json::Map<String, Value>,
    attrs: &serde_json::Map<String, Value>,
) -> Vec<String> {
    let mut out: Vec<String> = strings(x, "required_inputs")
        .into_iter()
        .filter(|a| {
            attrs
                .get(a)
                .is_some_and(|s| flag(s, "optional") || s.get("default").is_some())
        })
        .map(|a| format!("{a}_provided: inputs.{a} is not null because self_serviceable"))
        .collect();
    for (g, mode) in obj(x, "guards") {
        if let Some(c) = conditions.get(g) {
            let mark = match py_str(mode).as_str() {
                "audit" => " observe",
                "warn" => " flag",
                _ => "",
            };
            out.push(format!(
                "{g}: {}{mark} because {}",
                squash(&str_or(c, "expression", "")),
                str_or(c, "remedy", "None")
            ));
        }
    }
    out
}

/// A type's conditions, its machine's merged under its own.
fn conditions_of(d: &Decl, world: &BTreeMap<&str, &Decl>) -> serde_json::Map<String, Value> {
    let mut out = d
        .machine
        .as_deref()
        .and_then(|m| world.get(m))
        .map(|m| obj(&m.yaml, "conditions").clone())
        .unwrap_or_default();
    for (k, v) in obj(&d.yaml, "conditions") {
        out.insert(k.clone(), v.clone());
    }
    out
}

/// What a transition's body says beyond its clauses: its inputs' defaults
/// and its steps' expressions.
fn body_texts(x: &Value) -> Vec<String> {
    let mut out: Vec<String> = obj(x, "inputs")
        .values()
        .filter_map(|s| s.get("default").map(|d| squash(&py_str(d))))
        .collect();
    for s in walk(x.get("effect")) {
        out.extend(step_expressions(&s).into_iter().map(|e| squash(&e)));
    }
    out
}

/// The data checks over every declaration.
#[allow(clippy::too_many_lines)]
pub fn checks(docs: &[Value], all: &[Decl], world: &BTreeMap<&str, &Decl>, out: &mut Vec<Found4>) {
    let (kinds, metrics) = kinds_and_metrics(docs);
    let evaluators: BTreeSet<String> = docs
        .iter()
        .flat_map(|d| obj(d, "evaluators").keys().cloned().collect::<Vec<_>>())
        .collect();
    let kind_names: BTreeSet<&str> = kinds.iter().map(|k| k.name.as_str()).collect();
    // 17: a declared attribute's or field's decimal holds 1 to 18 digits
    for d in all {
        for (an, spec) in &d.attrs {
            decimal(
                &d.name,
                an,
                &str_or(spec, "type", ""),
                d.module,
                &ext(&d.path, segs!["attributes", an]),
                out,
            );
        }
    }
    for k in &kinds {
        for (an, spec) in &k.fields {
            decimal(
                &k.name,
                an,
                &str_or(spec, "type", ""),
                k.module,
                &ext(&k.path, segs!["attributes", an]),
                out,
            );
        }
    }
    // 54: observation kinds
    for k in &kinds {
        for (fname, spec) in &k.fields {
            let at = ext(&k.path, segs!["attributes", fname]);
            if GENERATED.contains(&fname.as_str()) {
                add(
                    out,
                    54,
                    format!(
                        "{}.{fname} is named as a member every observation already has",
                        k.name
                    ),
                    k.module,
                    &at,
                );
            }
            let kind = str_or(spec, "type", "");
            let numeric = kind.starts_with("int")
                && !kind[3..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
                || kind.starts_with("decimal")
                    && !kind[7..].starts_with(|c: char| c.is_alphanumeric() || c == '_');
            if spec
                .get("unit")
                .is_some_and(|u| super::doc::truthy(Some(u)))
                && !numeric
            {
                add(
                    out,
                    54,
                    format!(
                        "{}.{fname} declares a unit and is not int or decimal",
                        k.name
                    ),
                    k.module,
                    &at,
                );
            }
        }
        for (n, expr) in &k.invariants {
            if OUTSIDE.is_match(expr) || beyond_own_fields(expr) {
                let clause = format!("{n}: {expr}");
                let cut: String = clause.chars().take(48).collect();
                add(
                    out,
                    54,
                    format!("{}: invariant reads beyond its own fields: {cut}", k.name),
                    k.module,
                    &ext(&k.path, segs!["invariants", n]),
                );
            }
        }
    }
    for d in all {
        for r in d.rels.iter().filter(|r| r.kind == RelKind::Part) {
            if kind_names.contains(r.target().as_str()) {
                add(
                    out,
                    54,
                    format!(
                        "{}.{} declares a part of observation {}, which the kind's 'as' adds; the subject does not declare it",
                        d.name,
                        r.name,
                        r.target()
                    ),
                    r.module,
                    &r.path,
                );
            }
        }
    }
    // 56 and 58: metrics and the members they read
    let by_metric: BTreeMap<&str, &MetricD> =
        metrics.iter().map(|m| (m.name.as_str(), m)).collect();
    for m in &metrics {
        let (mut src_decl, mut binder, mut dataset): (
            Option<SourceDecl<'_>>,
            Option<String>,
            bool,
        ) = (None, None, false);
        if let Some((fr, fp)) = &m.from {
            let Some(c) = FROM.captures(fr) else {
                add(
                    out,
                    56,
                    format!("metric {}: 'from' is not '<binder> in <source>'", m.name),
                    m.module,
                    fp,
                );
                continue;
            };
            binder = Some(c[1].to_string());
            let base = c[2].to_string();
            let suffix = c.get(3).map(|s| s.as_str().to_string());
            let member = c.get(4).map(|s| s.as_str().to_string());
            src_decl = source_decl(&base, world, &kinds);
            if let Some(sfx) = &suffix
                && !SOURCES.contains(&sfx.as_str())
            {
                add(
                    out,
                    56,
                    format!(
                        "metric {} reads {base}.{sfx}, which is not a source",
                        m.name
                    ),
                    m.module,
                    fp,
                );
            }
            if src_decl.is_none() {
                add(
                    out,
                    19,
                    format!("metric {} reads undeclared {base}", m.name),
                    m.module,
                    fp,
                );
            }
            if let (Some(mem), Some(SourceDecl::Type(sd))) = (&member, &src_decl)
                && !tracked(sd, mem, world)
            {
                add(
                    out,
                    58,
                    format!(
                        "metric {} reads intervals of {base}.{mem}, which is not tracked",
                        m.name
                    ),
                    m.module,
                    fp,
                );
            }
            if let (Some(mem), Some(SourceDecl::Kind(k))) = (&member, &src_decl) {
                // a kind's fields are its `attrs`: tracked where the type is a name in capitals
                let tracked_field = k.fields.iter().any(|(f, sp)| {
                    let t = first_token(sp);
                    f == mem
                        && t.chars().next().is_some_and(char::is_uppercase)
                        && !t.contains("[]")
                });
                if !tracked_field {
                    add(
                        out,
                        58,
                        format!(
                            "metric {} reads intervals of {base}.{mem}, which is not tracked",
                            m.name
                        ),
                        m.module,
                        fp,
                    );
                }
            }
            if fr.contains(" split by ") || fr.starts_with("split by") {
                if suffix.as_deref() != Some("intervals") {
                    let shown = suffix.as_ref().map(|s| format!(".{s}")).unwrap_or_default();
                    add(
                        out,
                        56,
                        format!(
                            "metric {} splits {base}{shown}, and only a type's spans are split",
                            m.name
                        ),
                        m.module,
                        fp,
                    );
                }
                if m.window {
                    add(
                        out,
                        56,
                        format!(
                            "metric {} splits its spans and declares 'window on'; a window clips its pieces",
                            m.name
                        ),
                        m.module,
                        fp,
                    );
                }
            }
            if suffix.is_some() {
                src_decl = None;
                dataset = true;
            }
        }
        if let (Some((dims, bp)), Some(b)) = (&m.by, &binder) {
            let paths = re(&format!(r"\b{}((?:\.\w+)+)", regex::escape(b)));
            for dim in dims.split(',') {
                let expr = dim.split_once('=').map_or(dim, |(_, e)| e);
                for c in paths.captures_iter(expr) {
                    let path = &c[1];
                    let hops = path.matches('.').count()
                        - usize::from(dataset && path.starts_with(".object"));
                    if hops > 2 {
                        add(
                            out,
                            56,
                            format!(
                                "metric {}: dimension '{}' is {hops} hops",
                                m.name,
                                dim.trim()
                            ),
                            m.module,
                            bp,
                        );
                    }
                    let first = path.split('.').nth(1).unwrap_or_default();
                    if src_decl.as_ref().is_some_and(|s| s.personal(first)) {
                        add(
                            out,
                            56,
                            format!(
                                "metric {}: personal member '{first}' is a dimension",
                                m.name
                            ),
                            m.module,
                            bp,
                        );
                    }
                }
            }
        }
        if let (Some(sd), Some(b)) = (&src_decl, &binder) {
            let read = re(&format!(r"\b{}\.(\w+)", regex::escape(b)));
            let clauses = m
                .from
                .iter()
                .map(|c| ("from", c))
                .chain(m.value.iter().map(|c| ("value", c)))
                .chain(m.flags.iter().map(|c| ("flag", c)));
            for (key, (txt, p)) in clauses {
                for c in read.captures_iter(txt) {
                    if sd.personal(&c[1]) {
                        add(
                            out,
                            56,
                            format!(
                                "metric {}: reads personal member '{}' in its {key}",
                                m.name, &c[1]
                            ),
                            m.module,
                            p,
                        );
                    }
                }
            }
        }
    }
    // 57, 58, 61 and a metric reference's window and dimensions (56), per transition
    for d in all {
        let machine = d.machine.as_deref().and_then(|m| world.get(m)).copied();
        let states: Vec<String> = if d.states.is_empty() {
            machine
                .map(|m| m.states.iter().map(|s| s.name.clone()).collect())
                .unwrap_or_default()
        } else {
            d.states.iter().map(|s| s.name.clone()).collect()
        };
        let conditions = if d.kind == DeclKind::Type {
            conditions_of(d, world)
        } else {
            obj(&d.yaml, "conditions").clone()
        };
        let attrs = if d.kind == DeclKind::Machine {
            obj(d.yaml.get("requires").unwrap_or(&Value::Null), "attributes").clone()
        } else {
            obj(&d.yaml, "attributes").clone()
        };
        for t in &d.trans {
            let (dn, tn) = (&d.name, &t.name);
            let reqs = require_clauses(&t.x, &conditions, &attrs);
            let word = |r: &str, w: &str| {
                if w == "observe" {
                    OBSERVE.is_match(r)
                } else {
                    FLAG.is_match(r)
                }
            };
            if matches!(t.head, Head::Assert | Head::Erase)
                && reqs.iter().any(|r| word(r, "observe") || word(r, "flag"))
            {
                let k = if t.head == Head::Assert {
                    "assert"
                } else {
                    "erase"
                };
                add(
                    out,
                    57,
                    format!("{dn}.{tn} is an {k} with an observing or flag clause"),
                    t.module,
                    &t.path,
                );
            }
            if reqs.iter().any(|r| word(r, "observe") && word(r, "flag")) {
                add(
                    out,
                    57,
                    format!("{dn}.{tn} has a clause marked both observe and flag (ADR-0111)"),
                    t.module,
                    &t.path,
                );
            }
            if let Some(limit) =
                t.x.get("backdating_limit")
                    .filter(|l| super::doc::truthy(Some(l)))
            {
                let head = format!("backdatable within {}", py_str(limit));
                if !BACKDATABLE.is_match(&head) {
                    add(
                        out,
                        58,
                        format!(
                            "{dn}.{tn} is backdatable without a duration in s, min, h, days or weeks"
                        ),
                        t.module,
                        &t.path,
                    );
                }
                let via = !strings(&t.x, "only_via").is_empty();
                if matches!(t.head, Head::Assert | Head::Erase) || via {
                    let what = if via {
                        "only via".to_string()
                    } else {
                        format!(
                            "an {}",
                            if t.head == Head::Assert {
                                "assert"
                            } else {
                                "erase"
                            }
                        )
                    };
                    add(
                        out,
                        58,
                        format!("{dn}.{tn} is backdatable and is {what}"),
                        t.module,
                        &t.path,
                    );
                }
            }
            let mut texts = reqs.clone();
            texts.extend(body_texts(&t.x));
            for txt in &texts {
                for c in TIME_IN.captures_iter(txt) {
                    if !states.contains(&c[1].to_string()) {
                        add(
                            out,
                            58,
                            format!("{dn}.{tn}: time_in({}) names no state", &c[1]),
                            t.module,
                            &t.path,
                        );
                    }
                }
                for c in ENTERED_AT.captures_iter(txt) {
                    if !states.contains(&c[1].to_string()) && !tracked(d, &c[1], world) {
                        add(
                            out,
                            58,
                            format!(
                                "{dn}.{tn}: entered_at({}) names neither a state nor a tracked member",
                                &c[1]
                            ),
                            t.module,
                            &t.path,
                        );
                    }
                }
            }
            for r in &reqs {
                for c in METRIC_REF.captures_iter(r) {
                    let (mn, margs) = (&c[1], &c[2]);
                    let Some(md) = by_metric.get(mn) else {
                        add(
                            out,
                            19,
                            format!("{dn}.{tn} reads undeclared metric {mn}"),
                            t.module,
                            &t.path,
                        );
                        continue;
                    };
                    if margs.contains("over last") && !windowed(md, &by_metric, &mut Vec::new()) {
                        add(
                            out,
                            56,
                            format!(
                                "{dn}.{tn}: 'over last' on metric {mn}, which declares no 'window on', nor combines inputs that each do"
                            ),
                            t.module,
                            &t.path,
                        );
                    }
                    let mut dims: BTreeSet<String> = md
                        .by
                        .iter()
                        .flat_map(|(b, _)| {
                            b.split(',')
                                .map(|x| x.split('=').next().unwrap_or_default().trim().to_string())
                                .collect::<Vec<_>>()
                        })
                        .collect();
                    dims.insert("version".into());
                    dims.insert("actor_kind".into());
                    for b in BOUND.captures_iter(margs) {
                        if !dims.contains(&b[1]) {
                            add(
                                out,
                                56,
                                format!(
                                    "{dn}.{tn} binds '{}', not a dimension of metric {mn}",
                                    &b[1]
                                ),
                                t.module,
                                &t.path,
                            );
                        }
                    }
                }
            }
            argument_from_written(d, t, world, &evaluators, out);
        }
    }
    // 58: an observation kind's bound
    for k in &kinds {
        if let Some(dur) = &k.delay
            && !UNIT_DURATION.is_match(dur.trim())
        {
            add(
                out,
                58,
                format!(
                    "{}: occurred within '{}' is not in s, min, h, days or weeks",
                    k.name,
                    dur.trim()
                ),
                k.module,
                &ext(&k.path, segs!["max_recording_delay"]),
            );
        }
    }
    // 59 and 62: an assignee and the actor identity
    for d in all {
        let mut marked = Vec::new();
        for (an, spec) in &d.attrs {
            let actor = spec
                .get("actor_kind")
                .is_some_and(|k| super::doc::truthy(Some(k)));
            let identity = str_or(spec, "type", "") == "identity";
            let at = ext(&d.path, segs!["attributes", an]);
            if actor && !identity {
                add(
                    out,
                    59,
                    format!("{}.{an} is marked actor and is not an identity", d.name),
                    d.module,
                    &at,
                );
            }
            if actor && identity {
                marked.push(an.clone());
                let k = str_or(spec, "actor_kind", "");
                if !["human", "agent", "service"].contains(&k.as_str()) {
                    add(
                        out,
                        62,
                        format!(
                            "{}.{an} is an actor identity with no kind of human, agent or service",
                            d.name
                        ),
                        d.module,
                        &at,
                    );
                }
            }
        }
        if marked.len() > 1 {
            add(
                out,
                62,
                format!(
                    "{} marks {} actor identities; a type holds one kind of actor, named once",
                    d.name,
                    marked.len()
                ),
                d.module,
                &d.path,
            );
        }
        for r in d
            .rels
            .iter()
            .filter(|r| r.kind == RelKind::Ref && flag(&r.spec, "assignee"))
        {
            if !stores(r, world) {
                add(
                    out,
                    59,
                    format!(
                        "{}.{} is an assignee and is not a singular stored ref",
                        d.name, r.name
                    ),
                    r.module,
                    &r.path,
                );
            }
            if let Some(tgt) = world.get(r.target().as_str()) {
                let ids = tgt
                    .attrs
                    .iter()
                    .filter(|(_, s)| {
                        str_or(s, "type", "") == "identity"
                            && s.get("actor_kind")
                                .is_some_and(|k| super::doc::truthy(Some(k)))
                    })
                    .count();
                if ids != 1 {
                    add(
                        out,
                        59,
                        format!(
                            "{}.{} is an assignee; {} marks {ids} actor identities, not one",
                            d.name, r.name, tgt.name
                        ),
                        r.module,
                        &r.path,
                    );
                }
            }
        }
    }
    erasure(all, world, out);
}

fn decimal(owner: &str, an: &str, kind: &str, module: usize, at: &Path, out: &mut Vec<Found4>) {
    let Some(inner) = kind.split("decimal(").nth(1) else {
        return;
    };
    let Some((p, rest)) = inner.split_once(',') else {
        return;
    };
    let Some(s) = rest.split(')').next() else {
        return;
    };
    let (Ok(p), Ok(s)) = (p.trim().parse::<u64>(), s.trim().parse::<u64>()) else {
        return;
    };
    if !((1..=18).contains(&p) && s <= p) {
        add(
            out,
            17,
            format!(
                "{owner}.{an} is a decimal({p},{s}); a decimal holds 1 to 18 digits, its scale within them"
            ),
            module,
            at,
        );
    }
}

/// The declaration a metric's `from` names: a type, or an observation kind.
enum SourceDecl<'a> {
    Type(&'a Decl),
    Kind(&'a ObsD),
}

impl SourceDecl<'_> {
    /// Whether a member is personal.
    fn personal(&self, member: &str) -> bool {
        match self {
            SourceDecl::Type(d) => d
                .attrs
                .iter()
                .any(|(a, s)| a == member && flag(s, "personal")),
            SourceDecl::Kind(k) => k
                .fields
                .iter()
                .any(|(a, s)| a == member && flag(s, "personal")),
        }
    }
}

fn source_decl<'a>(
    base: &str,
    world: &BTreeMap<&str, &'a Decl>,
    kinds: &'a [ObsD],
) -> Option<SourceDecl<'a>> {
    world
        .get(base)
        .map(|d| SourceDecl::Type(d))
        .or_else(|| kinds.iter().find(|k| k.name == base).map(SourceDecl::Kind))
}

/// A metric a guard may window: one that declares `window on`, splits its
/// spans, or combines inputs that each are windowed (ADR-0128).
fn windowed(m: &MetricD, by: &BTreeMap<&str, &MetricD>, seen: &mut Vec<String>) -> bool {
    if m.window
        || m.from
            .as_ref()
            .is_some_and(|(f, _)| f.contains(" split by "))
    {
        return true;
    }
    if m.combine.is_empty() {
        return false;
    }
    seen.push(m.name.clone());
    let all = m
        .combine
        .iter()
        .all(|n| !seen.contains(n) && by.get(n.as_str()).is_some_and(|x| windowed(x, by, seen)));
    seen.pop();
    all
}

/// Check 61: a call or create binding an argument from what its own outcome
/// writes, which an evaluator or metric guard of the callee reads.
fn argument_from_written(
    d: &Decl,
    t: &super::model::TransD,
    world: &BTreeMap<&str, &Decl>,
    evaluators: &BTreeSet<String>,
    out: &mut Vec<Found4>,
) {
    let steps = walk(t.x.get("effect"));
    let written: Vec<String> = steps
        .iter()
        .filter(|s| s.kind == "assign")
        .map(|s| str_or(s.body(), "location", ""))
        .collect();
    if written.is_empty() {
        return;
    }
    for s in &steps {
        let body = s.body();
        let target = match s.kind {
            "call" => squash(&str_or(body, "target", "")),
            "create"
                if body
                    .get("result")
                    .filter(|r| super::doc::truthy(Some(r)))
                    .is_none() =>
            {
                str_or(body, "type", "")
            }
            _ => continue,
        };
        if target.is_empty()
            || !target
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '$')
        {
            continue;
        }
        let last = target.rsplit('.').next().unwrap_or(&target).to_string();
        let Some(callee) = world
            .get(target.as_str())
            .or_else(|| world.get(last.as_str()))
        else {
            continue;
        };
        let tn2 = str_or(body, "transition", "");
        let Some(ct) = callee.trans.iter().find(|c| c.name == tn2) else {
            continue;
        };
        let conditions = conditions_of(callee, world);
        let reqs = require_clauses(&ct.x, &conditions, obj(&callee.yaml, "attributes"));
        for (an, aexpr) in obj(body, "inputs") {
            let aexpr = squash(&py_str(aexpr));
            let aexpr = aexpr.split(',').next().unwrap_or_default();
            let reads_written = written.iter().any(|w| names_word(aexpr, w));
            if !reads_written {
                continue;
            }
            let read_input = re(&format!(r"\binputs\.{}\b", regex::escape(an)));
            let hit = reqs.iter().any(|r| {
                let mut args: Vec<String> = EVALUATOR_ARGS
                    .captures_iter(r)
                    .filter(|c| evaluators.contains(&c[1]))
                    .map(|c| c[2].to_string())
                    .collect();
                args.extend(METRIC_ARGS.captures_iter(r).map(|c| c[1].to_string()));
                args.iter().any(|a| read_input.is_match(a))
            });
            if hit {
                add(
                    out,
                    61,
                    format!(
                        "{}.{} binds {target}.{tn2}'s '{an}' from a value its outcome writes, which an evaluator or metric guard reads",
                        d.name, t.name
                    ),
                    t.module,
                    &t.path,
                );
            }
        }
    }
}

/// Check 60: a type holding a personal value declares its erasure.
fn erasure(all: &[Decl], world: &BTreeMap<&str, &Decl>, out: &mut Vec<Found4>) {
    let mut superseded = BTreeSet::new();
    for x in all {
        for t in &x.trans {
            for s in walk(t.x.get("effect"))
                .iter()
                .filter(|s| s.kind == "supersede")
            {
                let who = squash(&py_str(s.body()));
                if let Some(op) = who.strip_prefix("inputs.")
                    && let Some(sp) = obj(&t.x, "inputs").get(op)
                {
                    let k = sp
                        .get("type")
                        .filter(|k| super::doc::truthy(Some(k)))
                        .or_else(|| sp.get("reference"))
                        .map(py_str)
                        .unwrap_or_default();
                    superseded.insert(
                        k.chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '_')
                            .collect::<String>(),
                    );
                }
            }
        }
    }
    for d in all.iter().filter(|d| d.kind == DeclKind::Type) {
        let machine = d.machine.as_deref().and_then(|m| world.get(m)).copied();
        let states_superseding = if d.states.is_empty() {
            machine.is_some_and(|m| m.states.iter().any(|s| s.superseding))
        } else {
            d.states.iter().any(|s| s.superseding)
        };
        let mut attrs = d.attrs.clone();
        let mut seen = BTreeSet::new();
        let mut anc = d.base.as_deref().and_then(|b| world.get(b)).copied();
        while let Some(a) = anc {
            if !seen.insert(a.name.clone()) {
                break;
            }
            for (n, s) in &a.attrs {
                if !attrs.iter().any(|(m, _)| m == n) {
                    attrs.push((n.clone(), s.clone()));
                }
            }
            anc = a.base.as_deref().and_then(|b| world.get(b)).copied();
        }
        let mut personal: Vec<String> = attrs
            .iter()
            .filter(|(_, s)| flag(s, "personal"))
            .map(|(a, _)| format!("'{a}'"))
            .collect();
        let trans = d
            .trans
            .iter()
            .chain(machine.map(|m| m.trans.iter()).into_iter().flatten());
        for t in trans.clone() {
            for (i, sp) in obj(&t.x, "inputs") {
                if flag(sp, "personal") {
                    personal.push(format!("'{}.{i}'", t.name));
                }
            }
        }
        let chained = superseded.contains(&d.name) || states_superseding;
        let erases = trans.into_iter().any(|t| t.head == Head::Erase);
        if let Some(first) = personal.first()
            && !erases
            && !d.abstract_
        {
            let why = if chained {
                ", and takes part in supersession"
            } else {
                ""
            };
            add(
                out,
                60,
                format!(
                    "{} holds personal {first} and declares no erase{why}",
                    d.name
                ),
                d.module,
                &d.path,
            );
        }
    }
}

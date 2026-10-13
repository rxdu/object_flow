//! Step 4's checks of single lines (`check-syntax-doc.py`'s `line_checks`,
//! `actor_reads` and `state_literals`): 21, 55, 56, 64, and 19 for a state
//! literal. The text checker scans the converted text line by line; these
//! scan the same words, built for each part of the YAML that writes a line
//! holding an expression, at the YAML path that line stands for. The words
//! are a message's quotation and the scan's input; no internal form is read.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::data::{kinds_and_metrics, require_clauses};
use super::doc::{Path, ext, flag, is_word, list, obj, py_str, re, segs, squash, str_or, strings};
use super::model::{Decl, DeclKind, Found4, add};
use super::view::types;

/// One line of the text: its words, stripped; whether it continues the
/// clause above it; the type or machine whose block it is in; and where.
struct Line {
    text: String,
    continues: Option<String>,
    block: Option<String>,
    module: usize,
    path: Path,
}

fn one(v: &Value) -> String {
    squash(&py_str(v))
}

fn args(inputs: &Value) -> String {
    super::doc::as_obj(inputs)
        .iter()
        .map(|(k, v)| format!("{k} := {}", one(v)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The markings an attribute or a reference carries after its type.
fn markings(spec: &Value) -> String {
    let mut marks = Vec::new();
    if let Some(e) = spec.get("external").filter(|e| super::doc::truthy(Some(e))) {
        marks.push(format!("external \"{}\"", py_str(e)));
    }
    if let Some(d) = spec.get("default") {
        marks.push(format!("default {}", one(d)));
    }
    for m in ["indexed", "personal"] {
        if flag(spec, m) {
            marks.push(m.to_string());
        }
    }
    match spec.get("unique") {
        Some(Value::Bool(true)) => marks.push("unique".into()),
        Some(Value::String(s)) if s == "in_scope" => marks.push("unique in scope".into()),
        Some(Value::Object(u)) if u.contains_key("with") => {
            marks.push(format!(
                "unique with {}",
                list(&Value::Object(u.clone()), "with")
                    .iter()
                    .map(py_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        Some(Value::Object(u)) if u.contains_key("where") => {
            marks.push(format!("unique where {}", one(&u["where"])))
        }
        _ => {}
    }
    marks.iter().map(|m| format!(" {m}")).collect()
}

/// The lines an effect writes, as `effect_lines` writes them.
fn effect_lines(
    steps: Option<&Value>,
    at: &Path,
    block: &Option<String>,
    module: usize,
    out: &mut Vec<Line>,
) {
    let Some(Value::Array(steps)) = steps else {
        return;
    };
    for (i, st) in steps.iter().enumerate() {
        let path = ext(at, segs![i]);
        let mut push = |text: String| {
            out.push(Line {
                text,
                continues: None,
                block: block.clone(),
                module,
                path: path.clone(),
            });
        };
        if let Some(a) = st.get("assign") {
            push(format!(
                "set {} := {}",
                str_or(a, "location", ""),
                one(a.get("expr").unwrap_or(&Value::Null))
            ));
        } else if let Some(Value::Array(c)) = st.get("clear") {
            for a in c {
                push(format!("clear {}", py_str(a)));
            }
        } else if let Some((k, a)) = st
            .get("add")
            .map(|a| ("add", a))
            .or_else(|| st.get("remove").map(|a| ("remove", a)))
        {
            push(format!(
                "{k} {} := {}",
                str_or(a, "location", ""),
                one(a.get("expr").unwrap_or(&Value::Null))
            ));
        } else if let Some(c) = st.get("call") {
            push(format!(
                "call {}.{}({})",
                one(c.get("target").unwrap_or(&Value::Null)),
                str_or(c, "transition", ""),
                args(c.get("inputs").unwrap_or(&Value::Null))
            ));
        } else if let Some(c) = st.get("create") {
            let bound = c
                .get("result")
                .filter(|r| super::doc::truthy(Some(r)))
                .map(|r| format!("{} = ", py_str(r)))
                .unwrap_or_default();
            push(format!(
                "create {bound}{}.{}({})",
                str_or(c, "type", ""),
                str_or(c, "transition", ""),
                args(c.get("inputs").unwrap_or(&Value::Null))
            ));
        } else if let Some(s) = st.get("supersede") {
            push(format!("supersede {}", one(s)));
        } else if let Some(f) = st.get("foreach") {
            let over = match f.get("array") {
                Some(a) => one(a),
                None => format!("1..{}", one(f.get("range").unwrap_or(&Value::Null))),
            };
            let filter = f
                .get("where")
                .map(|w| format!(" where {}", one(w)))
                .unwrap_or_default();
            push(format!(
                "for {} in {over}{filter} limit {} {{",
                str_or(f, "item", ""),
                py_str(f.get("limit").unwrap_or(&Value::Null))
            ));
            effect_lines(
                f.get("steps"),
                &ext(&path, segs!["foreach", "steps"]),
                block,
                module,
                out,
            );
        }
    }
}

fn transition_lines(
    x: &Value,
    conditions: &serde_json::Map<String, Value>,
    attrs: &serde_json::Map<String, Value>,
    at: &Path,
    block: &Option<String>,
    module: usize,
    out: &mut Vec<Line>,
) {
    let mut push = |text: String, path: Path| {
        out.push(Line {
            text,
            continues: None,
            block: block.clone(),
            module,
            path,
        })
    };
    for (i, spec) in obj(x, "inputs") {
        let k = spec
            .get("type")
            .filter(|k| super::doc::truthy(Some(k)))
            .or_else(|| spec.get("reference"))
            .map(py_str)
            .unwrap_or_default();
        let mut line = format!(
            "input {i} : {k}{}",
            if flag(spec, "optional") { "?" } else { "" }
        );
        if let Some(d) = spec.get("default") {
            line.push_str(&format!(" default {}", one(d)));
        }
        if flag(spec, "personal") {
            line.push_str(" personal");
        }
        push(line, ext(at, segs!["inputs", i]));
    }
    for r in require_clauses(x, conditions, attrs) {
        let path = match r.split_once(':').map(|(g, _)| g.to_string()) {
            Some(g) if obj(x, "guards").contains_key(&g) => ext(at, segs!["guards", &g]),
            _ => ext(at, segs!["required_inputs"]),
        };
        push(format!("require {r}"), path);
    }
    if flag(x, "may_admit") {
        push(
            format!("may admit {}", strings(x, "may_admit").join(", ")),
            ext(at, segs!["may_admit"]),
        );
    }
    if flag(x, "corrects") {
        push(
            format!("corrects {}", strings(x, "corrects").join(", ")),
            ext(at, segs!["corrects"]),
        );
    }
    effect_lines(
        x.get("effect"),
        &ext(at, segs!["effect"]),
        block,
        module,
        out,
    );
}

/// Every line of the modules that a line check reads.
fn lines(docs: &[Value], all: &[Decl], world: &BTreeMap<&str, &Decl>) -> Vec<Line> {
    let (kinds, metrics) = kinds_and_metrics(docs);
    let mut out = Vec::new();
    for (module, doc) in docs.iter().enumerate() {
        let mut last: Option<String> = None;
        for (mn, m) in obj(doc, "machines") {
            let block = Some(mn.clone());
            let at = segs!["machines", mn];
            let req = m.get("requires").cloned().unwrap_or(Value::Null);
            for (xn, x) in obj(m, "transitions") {
                transition_lines(
                    x,
                    obj(m, "conditions"),
                    obj(&req, "attributes"),
                    &ext(&at, segs!["transitions", xn]),
                    &block,
                    module,
                    &mut out,
                );
            }
            last = block;
        }
        for (tn, t) in types(doc) {
            let block = Some(tn.clone());
            let at = segs!["types", &tn];
            let mut push = |text: String, continues: Option<String>, path: Path| {
                out.push(Line {
                    text,
                    continues,
                    block: block.clone(),
                    module,
                    path,
                });
            };
            let summary: Vec<String> = list(t, "summary").iter().map(py_str).collect();
            if !summary.is_empty() {
                push(
                    format!("summary  {}", summary.join(", ")),
                    None,
                    ext(&at, segs!["summary"]),
                );
            }
            for (an, spec) in obj(t, "attributes") {
                let path = ext(&at, segs!["attributes", an]);
                let Some(r) = spec.get("reference").map(py_str) else {
                    let typ = str_or(spec, "type", "");
                    if typ == "counter" {
                        push(format!("counter {an}{}", markings(spec)), None, path);
                    } else {
                        let opt = if flag(spec, "optional") { "?" } else { "" };
                        let actor = spec
                            .get("actor_kind")
                            .filter(|k| super::doc::truthy(Some(k)))
                            .map(|k| format!(" actor {}", py_str(k)))
                            .unwrap_or_default();
                        let ident = spec
                            .get("identifier")
                            .filter(|i| super::doc::truthy(Some(i)))
                            .map(|i| {
                                let scope = i
                                    .get("scope")
                                    .filter(|s| super::doc::truthy(Some(s)))
                                    .map(|s| format!(" scoped by {}", py_str(s)))
                                    .unwrap_or_default();
                                format!(
                                    " identifier from {}{scope} format \"{}\"",
                                    str_or(i, "sequence", ""),
                                    str_or(i, "format", "")
                                )
                            })
                            .unwrap_or_default();
                        push(
                            format!("attr {an} {typ}{opt}{actor}{ident}{}", markings(spec)),
                            None,
                            path,
                        );
                    }
                    continue;
                };
                let opt = if flag(spec, "optional") { "?" } else { "" };
                let inverse = spec
                    .get("opposite")
                    .filter(|o| super::doc::truthy(Some(o)))
                    .map(|o| format!(" inverse {}", py_str(o)))
                    .unwrap_or_default();
                if str_or(spec, "aggregation", "") == "composite" {
                    let head = format!("part {an} : {r}{opt}{inverse}");
                    push(head.clone(), None, path.clone());
                    for c in list(spec, "cascade") {
                        let on: Vec<String> = list(c, "on").iter().map(py_str).collect();
                        let on = if on.len() == 1 {
                            on[0].clone()
                        } else {
                            format!("{{ {} }}", on.join(", "))
                        };
                        let a = args(c.get("inputs").unwrap_or(&Value::Null));
                        let a = if a.is_empty() {
                            String::new()
                        } else {
                            format!("({a})")
                        };
                        push(
                            format!(
                                "cascade on {on} to {}.{}{a} limit {}",
                                r.trim_end_matches(['[', ']']),
                                str_or(c, "transition", ""),
                                py_str(c.get("limit").unwrap_or(&Value::Null))
                            ),
                            Some(head.clone()),
                            path.clone(),
                        );
                    }
                    continue;
                }
                let far = obj(doc, "types")
                    .get(r.trim_end_matches(['[', ']']))
                    .cloned()
                    .unwrap_or(Value::Null);
                let owner = spec
                    .get("opposite")
                    .and_then(|o| obj(&far, "attributes").get(&py_str(o)))
                    .is_some_and(|e| str_or(e, "aggregation", "") == "composite");
                if owner {
                    push(format!("owner {an} : {r}{inverse}"), None, path);
                } else {
                    let stored = if flag(spec, "stored") { " stored" } else { "" };
                    let assignee = if flag(spec, "assignee") {
                        " assignee"
                    } else {
                        ""
                    };
                    push(
                        format!(
                            "ref {an} : {r}{opt}{inverse}{stored}{assignee}{}",
                            markings(spec)
                        ),
                        None,
                        path,
                    );
                }
            }
            for (an, ip) in obj(t, "inherited_parts") {
                let target = all
                    .iter()
                    .find(|d| d.name == tn)
                    .map(|d| super::model::inherited_part_target(d, an, world))
                    .unwrap_or_default();
                for (i, c) in list(ip, "cascade").iter().enumerate() {
                    let on: Vec<String> = list(c, "on").iter().map(py_str).collect();
                    let on = if on.len() == 1 {
                        on[0].clone()
                    } else {
                        format!("{{ {} }}", on.join(", "))
                    };
                    let a = args(c.get("inputs").unwrap_or(&Value::Null));
                    let a = if a.is_empty() {
                        String::new()
                    } else {
                        format!("({a})")
                    };
                    push(
                        format!(
                            "cascade {an} on {on} to {target}.{}{a} limit {}",
                            str_or(c, "transition", ""),
                            py_str(c.get("limit").unwrap_or(&Value::Null))
                        ),
                        None,
                        ext(&at, segs!["inherited_parts", an, "cascade", i]),
                    );
                }
            }
            for (dn, d) in obj(t, "derived_attributes") {
                push(
                    format!(
                        "derive {dn} = {}{}",
                        one(d.get("expression").unwrap_or(&Value::Null)),
                        if flag(d, "indexed") { " indexed" } else { "" }
                    ),
                    None,
                    ext(&at, segs!["derived_attributes", dn]),
                );
            }
            for (sn, v) in obj(t, "states") {
                let req = strings(v, "required_attributes");
                if req.is_empty() {
                    continue;
                }
                let cond = req
                    .iter()
                    .map(|a| format!("{a} is not null"))
                    .collect::<Vec<_>>()
                    .join(" and ");
                let cond = if req.len() > 1 {
                    format!("({cond})")
                } else {
                    cond
                };
                push(
                    format!(
                        "invariant {}_invariant: state != {sn} or {cond}",
                        sn.to_lowercase()
                    ),
                    None,
                    ext(&at, segs!["states", sn, "required_attributes"]),
                );
            }
            for (n, inv) in obj(t, "invariants") {
                push(
                    format!(
                        "invariant {n}: {}",
                        one(inv.get("expression").unwrap_or(&Value::Null))
                    ),
                    None,
                    ext(&at, segs!["invariants", n]),
                );
            }
            let decl = all
                .iter()
                .find(|d| d.name == tn && d.kind == DeclKind::Type);
            let mut conditions = decl
                .and_then(|d| d.machine.as_deref())
                .and_then(|m| world.get(m))
                .map(|m| obj(&m.yaml, "conditions").clone())
                .unwrap_or_default();
            for (k, v) in obj(t, "conditions") {
                conditions.insert(k.clone(), v.clone());
            }
            for (xn, x) in obj(t, "transitions") {
                transition_lines(
                    x,
                    &conditions,
                    obj(t, "attributes"),
                    &ext(&at, segs!["transitions", xn]),
                    &block,
                    module,
                    &mut out,
                );
            }
            last = block;
        }
        // the tail: each observation kind's and metric's lines, then the
        // module's metrics, none of them in a type's block
        for k in kinds.iter().filter(|k| k.module == module) {
            for (an, spec) in &k.fields {
                let opt = if flag(spec, "optional") { "?" } else { "" };
                let kind = spec
                    .get("type")
                    .or_else(|| spec.get("reference"))
                    .map(py_str)
                    .unwrap_or_default();
                let unit = spec
                    .get("unit")
                    .filter(|u| super::doc::truthy(Some(u)))
                    .map(|u| format!(" unit \"{}\"", py_str(u)))
                    .unwrap_or_default();
                let personal = if flag(spec, "personal") {
                    " personal"
                } else {
                    ""
                };
                out.push(Line {
                    text: format!("field {an} : {kind}{opt}{unit}{personal}"),
                    continues: None,
                    block: None,
                    module,
                    path: ext(&k.path, segs!["attributes", an]),
                });
            }
            if let Some(d) = &k.delay {
                out.push(Line {
                    text: format!("occurred within {d}"),
                    continues: None,
                    block: None,
                    module,
                    path: ext(&k.path, segs!["max_recording_delay"]),
                });
            }
            for (n, e) in &k.invariants {
                out.push(Line {
                    text: format!("invariant {n}: {e}"),
                    continues: None,
                    block: None,
                    module,
                    path: ext(&k.path, segs!["invariants", n]),
                });
            }
        }
        let mut tail = false;
        for m in metrics.iter().filter(|m| m.module == module) {
            tail = true;
            if let Some((f, p)) = &m.from {
                out.push(Line {
                    text: format!("from      {f}"),
                    continues: None,
                    block: None,
                    module,
                    path: p.clone(),
                });
            }
            if !m.combine.is_empty() {
                out.push(Line {
                    text: format!("combine   {}", m.combine.join(", ")),
                    continues: None,
                    block: None,
                    module,
                    path: ext(&m.path, segs!["input_metrics"]),
                });
            }
            if let Some((b, p)) = &m.by {
                out.push(Line {
                    text: format!("by        {b}"),
                    continues: None,
                    block: None,
                    module,
                    path: p.clone(),
                });
            }
            if let Some((v, p)) = &m.value {
                out.push(Line {
                    text: format!("value     {v}"),
                    continues: None,
                    block: None,
                    module,
                    path: p.clone(),
                });
            }
            for (f, p) in &m.flags {
                out.push(Line {
                    text: format!("flag      {f}"),
                    continues: None,
                    block: None,
                    module,
                    path: p.clone(),
                });
            }
        }
        tail |= kinds.iter().any(|k| k.module == module);
        // the migration's lines follow, in the last block where no tail ends it
        let block = if tail { None } else { last };
        let mig = doc.get("migration").cloned().unwrap_or(Value::Null);
        for (tn, maps) in obj(&mig, "backfill") {
            for (a, e) in super::doc::as_obj(maps) {
                out.push(Line {
                    text: format!("backfill       {tn}.{a} := {}", one(e)),
                    continues: None,
                    block: block.clone(),
                    module,
                    path: segs!["migration", "backfill", tn, a],
                });
            }
        }
    }
    out
}

static NULL_COMPARE: LazyLock<Regex> = LazyLock::new(|| re(r"(==|!=)\s*null|null\s*(==|!=)"));
static AGGREGATE_OPEN: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b(?:count|sum|all|any|none|min|max)\(\s*"));
static BINDER: LazyLock<Regex> = LazyLock::new(|| re(r"^\w+\s+in\b"));
static UNBOUND: LazyLock<Regex> = LazyLock::new(|| re(r"^[a-z_]+\s+where"));

/// `\b(count|…|max)\(\s*(?!\w+\s+in\b)[a-z_]+\s+where`: an aggregate whose
/// collection is filtered with no name bound to its element.
fn unbound_aggregate(text: &str) -> bool {
    AGGREGATE_OPEN.find_iter(text).any(|m| {
        let rest = &text[m.end()..];
        !BINDER.is_match(rest) && UNBOUND.is_match(rest)
    })
}
static METRIC_ONLY: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b(avg|median|percentile|day|week|month|quarter|year)\("));
static RULE: LazyLock<Regex> = LazyLock::new(|| {
    re(r"^(require|invariant|derive|visible|set|add|remove|value|flag|by|from|window)\b")
});
static OUTCOME_STEP: LazyLock<Regex> =
    LazyLock::new(|| re(r"^(set|add|remove|clear|call|supersede|for)\b|^create\s+[\w.$]+\.\w+\("));
static ACTOR: LazyLock<Regex> = LazyLock::new(|| re(r"\bactor\."));
static QUALIFIED: LazyLock<Regex> = LazyLock::new(|| re(r"\b([A-Z]\w*)\.([A-Z][A-Z0-9_]*)\b"));
static LABELS: LazyLock<Regex> = LazyLock::new(|| re(r"\blabels\b"));
static TYPE_LABELS: LazyLock<Regex> = LazyLock::new(|| re(r"\b[A-Z]\w*\.labels\b"));
static LABELS_SOURCE: LazyLock<Regex> = LazyLock::new(|| re(r"^from\s+\w+\s+in\s+\w+\.labels\b"));

/// Each `(?<![\w.])state\s*(?:==|!=)\s*([A-Z][A-Z0-9_]*)\b(?!\.)`.
fn compared_states(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    // each `state` found where it is, the text walked once, where each match
    // collected the rest of the text again: a literal holding it 128k times
    // held the dry run's slot 229 s (ninth production review N-8)
    let word = ['s', 't', 'a', 't', 'e'];
    let mut i = 0;
    while let Some(off) = chars
        .get(i..)
        .and_then(|rest| rest.windows(5).position(|w| w == word))
    {
        let at = i + off;
        i = at + 5;
        if at > 0 && (is_word(chars[at - 1]) || chars[at - 1] == '.') {
            continue;
        }
        let mut j = at + 5;
        while chars.get(j).is_some_and(|c| c.is_whitespace()) {
            j += 1;
        }
        if !(chars.get(j) == Some(&'=') || chars.get(j) == Some(&'!'))
            || chars.get(j + 1) != Some(&'=')
        {
            continue;
        }
        j += 2;
        while chars.get(j).is_some_and(|c| c.is_whitespace()) {
            j += 1;
        }
        if !chars.get(j).is_some_and(char::is_ascii_uppercase) {
            continue;
        }
        let start = j;
        while chars
            .get(j)
            .is_some_and(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
        {
            j += 1;
        }
        // \b, and not followed by `.`
        if chars.get(j).is_some_and(|c| is_word(*c)) || chars.get(j) == Some(&'.') {
            continue;
        }
        out.push(chars[start..j].iter().collect());
    }
    out
}

/// The line checks over every module.
pub fn checks(docs: &[Value], all: &[Decl], world: &BTreeMap<&str, &Decl>, out: &mut Vec<Found4>) {
    let lines = lines(docs, all, world);
    // a reserved word may name a member, which a bare name then reads first (check 55)
    let member_labels = docs.iter().any(|d| {
        types(d).iter().any(|(_, t)| {
            obj(t, "attributes").contains_key("labels")
                || obj(t, "derived_attributes").contains_key("labels")
        })
    });
    let states_of = |name: &str| -> BTreeSet<String> {
        let Some(d) = world.get(name) else {
            return BTreeSet::new();
        };
        let own: BTreeSet<String> = d.states.iter().map(|s| s.name.clone()).collect();
        if !own.is_empty() {
            return own;
        }
        d.machine
            .as_deref()
            .and_then(|m| world.get(m))
            .map(|m| m.states.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default()
    };
    for l in &lines {
        // a line is read up to a `#`, as the text checker reads it
        let st = l.text.split('#').next().unwrap_or_default().trim();
        let cut48: String = st.chars().take(48).collect();
        if NULL_COMPARE.is_match(st) {
            add(
                out,
                21,
                "comparison against a bare null".into(),
                l.module,
                &l.path,
            );
        }
        if unbound_aggregate(st) {
            add(
                out,
                21,
                format!("aggregate without a binder: {cut48}"),
                l.module,
                &l.path,
            );
        }
        if st.contains('$') {
            add(
                out,
                21,
                format!("'$' input prefix: {cut48}"),
                l.module,
                &l.path,
            );
        }
        let labelled = if member_labels {
            TYPE_LABELS.is_match(st)
        } else {
            LABELS.is_match(st)
        };
        let labels_by = st.starts_with("labels by") && !LABELS.is_match(&st[9..]);
        if labelled && !labels_by && !LABELS_SOURCE.is_match(st) {
            add(
                out,
                55,
                format!("a label read outside a metric's source: {cut48}"),
                l.module,
                &l.path,
            );
        }
        if let Some(rule) = RULE.captures(st).map(|c| c[1].to_string()) {
            if st.contains("metric(") && rule != "require" {
                add(
                    out,
                    56,
                    format!("metric(…) outside a guard: {cut48}"),
                    l.module,
                    &l.path,
                );
            }
            if [
                "require",
                "invariant",
                "derive",
                "visible",
                "set",
                "add",
                "remove",
            ]
            .contains(&rule.as_str())
                && METRIC_ONLY.is_match(st)
            {
                add(
                    out,
                    56,
                    format!("a metric-only function outside a metric: {cut48}"),
                    l.module,
                    &l.path,
                );
            }
        }
        if ACTOR.is_match(st) {
            let gov = l.continues.as_deref().unwrap_or(st);
            if !OUTCOME_STEP.is_match(gov) {
                let cut = if st.chars().count() <= 64 {
                    st.to_string()
                } else {
                    let head: String = st.chars().take(64).collect();
                    format!(
                        "{} \u{2026}",
                        head.rsplit_once(' ').map_or(head.as_str(), |(a, _)| a)
                    )
                };
                add(
                    out,
                    64,
                    format!("reads the actor outside an outcome value: {cut}"),
                    l.module,
                    &l.path,
                );
            }
        }
        if let Some(cur) = &l.block {
            let own = states_of(cur);
            for s in compared_states(st) {
                if !own.is_empty() && !own.contains(&s) {
                    add(
                        out,
                        19,
                        format!("{cur}: state {s} names none of its own states"),
                        l.module,
                        &l.path,
                    );
                }
            }
        }
        for c in QUALIFIED.captures_iter(st) {
            let (t, s) = (&c[1], &c[2]);
            let Some(d) = world.get(t) else { continue };
            if !matches!(d.kind, DeclKind::Type | DeclKind::Machine) {
                continue;
            }
            let states = states_of(t);
            if !states.is_empty() && !states.contains(s) {
                add(
                    out,
                    19,
                    format!("{t}.{s} names no state of {t}"),
                    l.module,
                    &l.path,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The states a text compares `state` with, each found where it is, the
    /// text walked once, where each was found by collecting the rest of the
    /// text again: a literal holding `state` 128k times held the dry run's
    /// slot 229 s (ninth production review N-8).
    #[test]
    fn compared_states_walks_its_text_once() {
        assert_eq!(
            compared_states("state == OPEN or x.state == SHUT or state != DONE"),
            vec!["OPEN".to_string(), "DONE".to_string()]
        );
        // 5,000, so a quadratic walk fails in seconds, where 20,000 held the
        // suite 277 s before it failed
        let long = vec!["state == OPEN"; 5_000].join(" or ");
        let t = std::time::Instant::now();
        assert_eq!(compared_states(&long).len(), 5_000);
        let took = t.elapsed().as_secs_f64();
        assert!(took < 2.0, "{took:.2} s for 5,000 comparisons");
    }
}

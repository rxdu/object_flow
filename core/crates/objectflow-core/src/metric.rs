//! A metric read, as a guard reads it (`declaration-syntax.md` §6.9, §6.11;
//! `flow-format.md` §4.9): the rows of its source, narrowed by its filter,
//! the dimensions the guard binds and the window, and its value over them,
//! computed exactly and rounded once, half to even, to its expression's scale
//! (ADR-0128, ADR-0146).
//!
//! It is a port of the reference runner's (`scripts/flowrun.py`:
//! `Evaluator.metric`, `read_metric`, `metric_rows`, `over_rows`, `kind`),
//! which the examples and the runner's own metrics self-test hold it to. The
//! attempt log and labels are not kept, and a read of either is not run.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value as Json, json};

use crate::eval::{
    Evaluator, NotRun, attempt_row, closed_periods, interval_rows, transition_row, truth,
};
use crate::expr::{BinOp, Expr, Number, parse};
use crate::model::{Declared, Library, Type};
use crate::rat::Rat;
use crate::value::{Decimal, Value, minor_unit};
use crate::world::{Object, World};

fn not_run<T>(what: impl Into<String>) -> Result<T, NotRun> {
    Err(NotRun::Unsupported(what.into()))
}

/// An average of integers is a decimal to six places (ADR-0139).
const AVERAGE_SCALE: u8 = 6;

/// The aggregates a metric's value ranges over its rows with.
const ROW_AGGREGATES: &[&str] = &["count", "sum", "min", "max", "avg", "median", "percentile"];

/// A metric as a reference names it: the type declaring it, none for a
/// combined one, its declaration, and the module whose namespace holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub owner: Option<String>,
    pub spec: Json,
    pub module: String,
}

/// A type's family's attempt counts (`declaration-syntax.md` §6.9): each
/// UTC day, object and combination of the members, counted, the rows
/// `maintain` pruned, from the rollup, with those that remain; each a row of
/// the members with its `count`.
pub(crate) fn attempt_count_rows(world: &World<'_>, owner: &str) -> Result<Vec<Value>, NotRun> {
    const MEMBERS: [&str; 9] = [
        "object",
        "transition",
        "verdict",
        "clause",
        "remedy",
        "actor_kind",
        "enforced",
        "flagged",
        "declaration_version",
    ];
    let key_of = |day: Value, f: &BTreeMap<String, Value>| -> Vec<Value> {
        std::iter::once(day)
            .chain(
                MEMBERS
                    .iter()
                    .map(|m| f.get(*m).cloned().unwrap_or(Value::Absent)),
            )
            .collect()
    };
    // each key's count found by the key, where each attempt searched every
    // count before it, quadratic in the keys; the family's attempts read by
    // their type's index, never the whole attempt log; and the read's
    // deadline looked at as it goes (the audit's cost B5, ADR-0205 decision
    // 15). The keys' members are text, flags, numbers and a day, whose
    // printed forms are equal exactly where they are.
    let mut counts: Vec<(Vec<Value>, i64)> = Vec::new();
    let mut found: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut add = |key: Vec<Value>, n: i64| {
        let printed = format!("{key:?}");
        match found.get(&printed) {
            Some(&i) => counts[i].1 += n,
            None => {
                found.insert(printed, counts.len());
                counts.push((key, n));
            }
        }
    };
    for (f, n) in world.rollup()? {
        if matches!(f.get("type"), Some(Value::Text(t)) if world.lib.is_a(t, owner)) {
            add(
                key_of(f.get("day").cloned().unwrap_or(Value::Absent), f),
                *n,
            );
        }
    }
    for t in world.family(owner).iter() {
        for (i, a) in world.attempts_of_type(t)?.iter().enumerate() {
            if i % 1024 == 0 {
                world.due()?;
            }
            let Value::Row(f) = attempt_row(world, a)? else {
                continue;
            };
            add(key_of(Value::Date(a.at.div_euclid(86_400)), &f), 1);
        }
    }
    Ok(counts
        .into_iter()
        .map(|(key, n)| {
            let mut f: BTreeMap<String, Value> = std::iter::once("day")
                .chain(MEMBERS)
                .map(str::to_string)
                .zip(key)
                .collect();
            f.insert("count".into(), Value::Int(n));
            Value::row(f)
        })
        .collect())
}

/// How many rows a metric reads before it reads them as a scan, the types
/// they reach read whole once each (ADR-0205 decision 13).
const BATCHED_ROWS: usize = 256;

/// Whether an expression reads a member of the name given, at any depth.
fn reads_member(e: &Expr, name: &str) -> bool {
    matches!(e, Expr::Path(_, m) if m == name)
        || e.children().into_iter().any(|c| reads_member(c, name))
}

/// Whether a filter holds only of a transition that completes its object:
/// a conjunct of it, at its top, is `<item>.completes`.
fn completing(e: &Expr, item: &str) -> bool {
    match e {
        Expr::Path(b, member) => {
            member == "completes" && matches!(b.as_ref(), Expr::Name(n) if n == item)
        }
        Expr::Bin(BinOp::And, a, b) => completing(a, item) || completing(b, item),
        _ => false,
    }
}

/// The states a type's family finishes in, each once: those a transition
/// that completes its object enters.
fn finished_states(world: &World<'_>, owner: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tn in world.family(owner).iter() {
        if let Some(t) = world.lib.type_(tn) {
            for (s, _) in &t.states {
                if t.finished(s) && !out.contains(s) {
                    out.push(s.clone());
                }
            }
        }
    }
    out
}

/// The standard metrics every type has (`declaration-syntax.md` §6.11), as
/// `flow-format.md` §4.9 writes a formula, over a type `<T>`.
fn standard(name: &str) -> Option<Json> {
    Some(match name {
        "time_in_state" => json!({"source": "intervals", "item": "i",
            "dimensions": {"state": "i.state", "month": "month(i.entered_at)"}, "expression": "median(i.duration)"}),
        "time_in_state_p80" => json!({"source": "intervals", "item": "i",
            "dimensions": {"state": "i.state", "month": "month(i.entered_at)"}, "expression": "percentile(0.8, i.duration)"}),
        "throughput" => {
            json!({"source": "transitions", "item": "t", "filter": "t.completes and not t.imported and not t.migrated",
            "dimensions": {"state": "t.to_state", "week": "week(t.occurred_at)", "actor_kind": "t.actor_kind"}, "expression": "count()"})
        }
        "work_in_progress" => json!({"source": "objects", "item": "o", "filter": "o.open",
            "dimensions": {"state": "o.state"}, "expression": "count()"}),
        "oldest_open" => json!({"source": "objects", "item": "o", "filter": "o.open",
            "dimensions": {"state": "o.state"}, "expression": "max(now - o.entered_at(state))"}),
        "transition_counts" => json!({"source": "transitions", "item": "t",
            "filter": "not t.imported and not t.migrated and not t.redacted",
            "dimensions": {"transition": "t.transition", "actor_kind": "t.actor_kind", "week": "week(t.occurred_at)"},
            "expression": "count()"}),
        "refusals" => json!({"source": "attempt_counts", "item": "a", "filter": "a.enforced",
            "dimensions": {"transition": "a.transition", "clause": "a.clause", "remedy": "a.remedy",
                           "actor_kind": "a.actor_kind", "week": "week(a.day)"}, "expression": "sum(a.count)"}),
        "flags_raised" => json!({"source": "attempt_counts", "item": "a", "filter": "a.flagged",
            "dimensions": {"transition": "a.transition", "clause": "a.clause", "remedy": "a.remedy",
                           "actor_kind": "a.actor_kind", "week": "week(a.day)"}, "expression": "sum(a.count)"}),
        "refusal_rate" => {
            json!({"input_metrics": {"refused": "<T>.refusals", "applied": "<T>.transition_counts"},
            "group_by": ["transition", "clause", "actor_kind", "week"], "expression": "refused * 1.000 / (refused + applied)"})
        }
        "override_counts" => {
            json!({"source": "transitions", "item": "t", "filter": "t.overrides and not t.imported",
            "dimensions": {"state": "t.to_state", "reason": "t.reason", "week": "week(t.occurred_at)"}, "expression": "count()"})
        }
        "rework" => {
            json!({"source": "transitions", "item": "t", "filter": "t.returns and not t.imported and not t.migrated",
            "dimensions": {"state": "t.to_state", "week": "week(t.occurred_at)"}, "expression": "count()"})
        }
        _ => return None,
    })
}

/// The standard assignment metrics, over a type `<T>` and its assignee
/// reference `<r>` (`declaration-syntax.md` §6.11).
fn assignment(name: &str) -> Option<Json> {
    Some(match name {
        "open_work" => json!({"source": "objects", "item": "o", "filter": "o.open",
            "dimensions": {"assignee": "o.<r>"}, "expression": "count()"}),
        "time_unassigned" => {
            json!({"source": "intervals(<r>)", "item": "i", "filter": "i.value is null",
            "dimensions": {"month": "month(i.entered_at)"}, "expression": "median(i.duration)"})
        }
        "time_to_first_assignment" => json!({"source": "objects", "item": "o",
            "filter": "any(i in o.intervals(<r>) where i.value is not null)",
            "dimensions": {"month": "month(o.created_at)"},
            "expression": "median(min(i in o.intervals(<r>) where i.value is not null: i.entered_at) - o.created_at)"}),
        "time_with_assignee" => {
            json!({"source": "intervals(<r>)", "item": "i", "filter": "i.value is not null",
            "dimensions": {"assignee": "i.value"}, "expression": "sum(i.duration)"})
        }
        "cycle_time_by_assignee" => json!({"source": "transitions", "item": "t",
            "filter": "t.completes and not t.imported and not t.migrated",
            "dimensions": {"assignee": "t.held(<r>)", "state": "t.to_state", "month": "month(t.occurred_at)"},
            "expression": "median(t.occurred_at - t.object.created_at)"}),
        "time_in_state_by_holder" => json!({"source": "intervals", "item": "i",
            "dimensions": {"state": "i.state", "assignee": "i.held(<r>)"}, "expression": "median(i.duration)"}),
        "handoffs" => {
            json!({"source": "objects", "item": "o", "filter": "any(i in o.intervals(<r>) where i.value is not null)",
            "dimensions": {"month": "month(o.created_at)"},
            "expression": "avg(count(i in o.intervals(<r>) where i.value is not null) - 1)"})
        }
        "reassigned_back" => {
            json!({"source": "objects", "item": "o", "dimensions": {"month": "month(o.created_at)"},
            "expression": "count(where count(distinct i in o.intervals(<r>) where i.value is not null: i.value) < count(i in o.intervals(<r>) where i.value is not null))"})
        }
        "acted_by_non_assignee" => json!({"source": "transitions", "item": "t",
            "filter": "t.held(<r>) is not null and not t.imported and not t.migrated and not t.redacted",
            "dimensions": {"transition": "t.transition"},
            "expression": "count(where t.actor_id != t.held(<r>).actor_id) * 1.000 / count()"}),
        "handoffs_by_object" => {
            json!({"source": "objects", "item": "o", "dimensions": {"object": "o.id"},
            "expression": "sum(count(i in o.intervals(<r>) where i.value is not null) - 1)"})
        }
        "returns_by_object" => {
            json!({"source": "objects", "item": "o", "dimensions": {"object": "o.id"},
            "expression": "sum(count(i in o.intervals(<r>) where i.value is not null) - count(distinct i in o.intervals(<r>) where i.value is not null: i.value))"})
        }
        _ => return None,
    })
}

/// A standard metric over a type and, for an assignment metric, its assignee
/// reference.
fn instantiated(spec: &Json, owner: &str, reference: &str) -> Json {
    match spec {
        Json::String(s) => Json::String(s.replace("<T>", owner).replace("<r>", reference)),
        Json::Object(o) => Json::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), instantiated(v, owner, reference)))
                .collect(),
        ),
        Json::Array(a) => Json::Array(
            a.iter()
                .map(|v| instantiated(v, owner, reference))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn dotted(e: &Expr) -> String {
    match e {
        Expr::Name(n) => n.clone(),
        Expr::Path(b, m) => format!("{}.{m}", dotted(b)),
        other => format!("{other:?}"),
    }
}

/// What a metric reference names (`declaration-syntax.md` §6.9, §6.11): a
/// bare name, a metric of the module of the type reading it, or of `module`;
/// `<Type>.<name>`, a metric declared under that type or a standard one; and
/// `<Type>.<name>.<r>`, a standard assignment metric over its assignee
/// reference `r`.
pub fn find(
    lib: &Library,
    name: &Expr,
    tn: Option<&str>,
    module: Option<&str>,
) -> Result<Found, NotRun> {
    let home = |t: &str| lib.type_(t).map(|t| t.module.clone());
    match name {
        Expr::Name(n) => {
            let module = module
                .map(str::to_string)
                .or_else(|| tn.and_then(home))
                .or_else(|| {
                    // a reader's bare name: the one module that declares it
                    // (library-api.md §5)
                    let hits: Vec<&String> = lib
                        .metrics
                        .iter()
                        .filter(|(_, ms)| ms.contains_key(n))
                        .map(|(m, _)| m)
                        .collect();
                    (tn.is_none() && hits.len() == 1).then(|| hits[0].clone())
                });
            if let Some(m) = module
                && let Some((owner, spec)) = lib.metrics.get(&m).and_then(|ms| ms.get(n))
            {
                return Ok(Found {
                    owner: owner.clone(),
                    spec: spec.clone(),
                    module: m,
                });
            }
        }
        Expr::Path(base, n) => match &**base {
            Expr::Name(owner) => {
                let module = home(owner).unwrap_or_default();
                if let Some((Some(declared_by), spec)) =
                    lib.metrics.get(&module).and_then(|ms| ms.get(n))
                    && declared_by == owner
                {
                    return Ok(Found {
                        owner: Some(owner.clone()),
                        spec: spec.clone(),
                        module,
                    });
                }
                if let Some(spec) = standard(n) {
                    return Ok(Found {
                        owner: Some(owner.clone()),
                        spec: instantiated(&spec, owner, ""),
                        module,
                    });
                }
            }
            Expr::Path(inner, metric) => {
                if let Expr::Name(owner) = &**inner
                    && let Some(spec) = assignment(metric)
                    && lib
                        .type_(owner)
                        .and_then(|t| t.attribute(n))
                        .is_some_and(|a| a.assignee)
                {
                    return Ok(Found {
                        owner: Some(owner.clone()),
                        spec: instantiated(&spec, owner, n),
                        module: home(owner).unwrap_or_default(),
                    });
                }
            }
            _ => {}
        },
        _ => {}
    }
    not_run(format!(
        "the metric `{}`, which names none the core finds",
        dotted(name)
    ))
}

/// A declared metric's rows and value as a formula (`flow-format.md` §4.9):
/// a fixed measure is the formula it converts to, grouped and windowed as
/// service's `time_working` is (`declaration-syntax.md` §6.9, ADR-0139): a
/// tracked attribute read as it was held, any other as it is now, and a month
/// or week of the row's time, which it is windowed on.
fn formula(lib: &Library, owner: &str, spec: &Json) -> Result<Json, NotRun> {
    let Some(measure) = spec.get("measure").and_then(Json::as_str) else {
        return Ok(spec.clone());
    };
    let text = |k: &str| {
        spec.get(k)
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (item, stamp, mut out) = match measure {
        "median_time_in_state" => (
            "i",
            "i.entered_at",
            json!({"source": "intervals", "item": "i", "filter": format!("i.state == {owner}.{}", text("state")),
                   "expression": "median(i.duration)"}),
        ),
        "transition_count" => (
            "t",
            "t.occurred_at",
            // the transition's events by name, with the standard metrics'
            // exclusions (ADR-0146)
            json!({"source": "transitions", "item": "t",
                   "filter": format!("t.transition == \"{}\" and not t.imported and not t.migrated and not t.redacted", text("transition")),
                   "expression": "count()"}),
        ),
        other => return not_run(format!("the measure {other}")),
    };
    let tracked = lib.type_(owner).map(|t| lib.tracked(t)).unwrap_or_default();
    let mut dims = serde_json::Map::new();
    for g in spec
        .get("group_by")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let g = g.as_str().unwrap_or_default();
        let d = match g {
            "month" | "week" => format!("{g}({stamp})"),
            "actor" => "t.actor_id".to_string(),
            _ if tracked.iter().any(|t| t == g) => format!("{item}.held({g})"),
            _ => format!("{item}.object.{g}"),
        };
        dims.insert(g.to_string(), json!(d));
    }
    if let Some(o) = out.as_object_mut() {
        o.insert("dimensions".into(), Json::Object(dims));
        o.insert("time_dimension".into(), json!(stamp));
    }
    Ok(out)
}

/// A metric's dimensions: those it declares, and `version` and
/// `actor_kind`, which a combined one has only when it names them
/// (`declaration-syntax.md` §6.9).
fn dimensions(lib: &Library, owner: Option<&str>, spec: &Json) -> Result<Vec<String>, NotRun> {
    let names = |j: Option<&Json>| -> Vec<String> {
        j.and_then(Json::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Json::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    if spec.get("input_metrics").is_some() {
        return Ok(names(spec.get("group_by")));
    }
    let f = formula(lib, owner.unwrap_or_default(), spec)?;
    let mut out: Vec<String> = f
        .get("dimensions")
        .and_then(Json::as_object)
        .map(|d| d.keys().cloned().collect())
        .unwrap_or_default();
    out.extend(["version".to_string(), "actor_kind".to_string()]);
    Ok(out)
}

fn parsed(text: &str) -> Result<Expr, NotRun> {
    parse(text).map_err(|e| NotRun::Unsupported(format!("the metric expression {text:?}: {e}")))
}

/// An expression's type, as far as a metric's scale needs it
/// (`declaration-syntax.md` §8.1, §8.3).
#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Int,
    Decimal(u8),
    /// A decimal literal, which takes the scale of what it is combined with.
    Literal(u8),
    Money(String),
    Duration,
    Timestamp,
    /// A date, computed as the seconds of its day's start (ADR-0145).
    Date,
    Bool,
    Object(String),
    Objects(String),
    Interval(String),
    Intervals(String),
    Transition(String),
    Transitions(String),
    Attempt(String),
    AttemptCount(String),
    /// A label on an object of the type.
    Label(String),
    Other,
}

/// A value as the metric computes it, exactly: an integer, which `int /
/// int` truncates, or a fraction, or absent (`declaration-syntax.md` §8.3).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Num {
    Absent,
    Int(i128),
    Frac(Rat),
}

impl Num {
    fn of(v: &Value) -> Result<Num, NotRun> {
        let frac = |r: Option<Rat>| {
            r.map(Num::Frac)
                .ok_or_else(|| NotRun::OutOfRange(String::new()))
        };
        match v {
            Value::Absent => Ok(Num::Absent),
            Value::Int(n) => Ok(Num::Int(i128::from(*n))),
            Value::Decimal(d) => frac(Rat::of_decimal(d)),
            Value::Money { amount, .. } => frac(Rat::of_decimal(amount)),
            Value::Duration(s) | Value::Timestamp(s) => Ok(Num::Frac(Rat::int(i128::from(*s)))),
            // a date placed at the start of its day, UTC (ADR-0145)
            Value::Date(d) => Ok(Num::Frac(Rat::int(i128::from(*d) * 86_400))),
            other => not_run(format!("a metric's arithmetic over {}", other.kind())),
        }
    }

    fn rat(self) -> Option<Rat> {
        match self {
            Num::Absent => None,
            Num::Int(n) => Some(Rat::int(n)),
            Num::Frac(r) => Some(r),
        }
    }

    /// An operator between two values, as the language's arithmetic gives
    /// it, exactly: `int / int` discards the remainder (§8.3).
    fn combine(op: BinOp, a: Num, b: Num) -> Result<Num, NotRun> {
        let past = || NotRun::OutOfRange(String::new());
        let (Some(x), Some(y)) = (a.rat(), b.rat()) else {
            return Ok(Num::Absent);
        };
        if let (Num::Int(p), Num::Int(q)) = (a, b) {
            return Ok(match op {
                BinOp::Add => Num::Int(p.checked_add(q).ok_or_else(past)?),
                BinOp::Sub => Num::Int(p.checked_sub(q).ok_or_else(past)?),
                BinOp::Mul => Num::Int(p.checked_mul(q).ok_or_else(past)?),
                BinOp::Div if q == 0 => Num::Absent,
                BinOp::Div => Num::Int(p / q),
                other => {
                    return not_run(format!("the operator {} in a metric's value", other.text()));
                }
            });
        }
        let r = match op {
            BinOp::Add => x.checked_add(y),
            BinOp::Sub => x.checked_sub(y),
            BinOp::Mul => x.checked_mul(y),
            BinOp::Div if y == Rat::int(0) => return Ok(Num::Absent),
            BinOp::Div => x.checked_div(y),
            other => return not_run(format!("the operator {} in a metric's value", other.text())),
        };
        r.map(Num::Frac).ok_or_else(past)
    }
}

/// The reach of a dimension's path for one row: a path through a set end
/// reaches each distinct value, and the row is counted under each
/// (ADR-0123). `None` where the value is not a set.
enum Reach {
    One(Value),
    Many(Vec<Value>),
}

/// A value a read binds a dimension to: a guard's, as the language holds
/// it, or a reader's, as the wire carries it (ADR-0150), matched against a
/// row's value encoded the same way.
#[derive(Clone, Debug, PartialEq)]
enum Want {
    Value(Value),
    Wire(Json),
}

impl Want {
    fn matches(&self, got: &Value) -> bool {
        match self {
            Want::Value(v) => got.equals(v),
            Want::Wire(j) => crate::wire::encode(got) == *j,
        }
    }
}

type Bound = BTreeMap<String, Want>;

/// The rows a read of a formula selects, and how to read them.
struct Selected<'w> {
    picked: Vec<Value>,
    item: String,
    rowkind: Kind,
    dims: BTreeMap<String, Expr>,
    evaluator: Evaluator<'w>,
    tree: Expr,
    /// What the formula reads that a gap concerns: an object's creation
    /// time, and an object's own flow data (§6.9).
    reads_created: bool,
    reads_flow: bool,
    /// The scan its rows are read under, where they are many, held as long
    /// as the rows are read (ADR-0205 decision 13).
    _scan: Option<crate::world::Scan<'w>>,
}

/// A derived attribute's value as its expression's type has it: a sum over
/// no elements is the integer 0 (`eval.rs`), which is a duration's zero
/// where the expression yields a duration, an amount's in its currency's
/// minor unit where it yields money, and a decimal's at its scale where it
/// yields a decimal (§8.3), as the reference runner's service encodes it by
/// the expression's kind.
pub fn typed_derived(world: &World<'_>, t: &Type, e: &Expr, v: Value) -> Value {
    if v != Value::Int(0) {
        return v;
    }
    let reader = Reader {
        world,
        answers: None,
        typing: Default::default(),
    };
    let this = BTreeMap::from([("this".to_string(), Kind::Object(t.name.clone()))]);
    match reader.kind(e, &this, Some(t)) {
        Ok(Kind::Duration) => Value::Duration(0),
        Ok(Kind::Money(currency)) => minor_unit(&currency)
            .and_then(|scale| Decimal::new(0, scale))
            .map_or(v, |amount| Value::Money { currency, amount }),
        Ok(Kind::Decimal(scale)) => Decimal::new(0, scale).map_or(v, Value::Decimal),
        _ => v,
    }
}

/// The read of one metric over a store as the request found it.
struct Reader<'w> {
    world: &'w World<'w>,
    answers: Option<&'w BTreeMap<String, String>>,
    /// Each derived member's kind, by its type and name, once found, and
    /// `None` while it is being found: a derived attribute reading itself
    /// through a set, `count(c in children) + sum(c in children: c.size)`,
    /// recursed until the stack overflowed and aborted the service on a read
    /// of a leaf, and a chain of attributes each reading the one before twice
    /// was typed 2^n times (eleventh production review).
    typing: std::cell::RefCell<BTreeMap<(String, String), Option<Kind>>>,
}

/// The most derived members one kind is found through, as the checkers type
/// through at most as many (`check::names`'s `TYPING_DEPTH`).
const TYPING_DEPTH: usize = 64;

/// The observations given that are on an object of the family and that
/// none corrects, as rows, in the order given.
fn uncorrected_on(
    family: &[&Object],
    observations: &[&Object],
    corrected: &BTreeSet<String>,
) -> Vec<Value> {
    let subjects: BTreeSet<&str> = family.iter().map(|o| o.id.as_str()).collect();
    observations
        .iter()
        .filter(|x| matches!(x.attr("subject"), Value::Object(s) if subjects.contains(s.as_str())))
        .filter(|x| !corrected.contains(&x.id))
        .map(|x| Value::Object(x.id.clone()))
        .collect()
}

/// What a metric's rows are: each an object, a span or an event of a type,
/// and the member that names the kind of actor behind one.
struct Rows {
    rows: Vec<Value>,
    kind: Kind,
    by_kind: &'static str,
}

impl<'w> Reader<'w> {
    /// What a row of a metric's source is, read from the source alone, as
    /// its rows give it (§6.9): an object, a span, an event, an observation,
    /// a label, an attempt or a day's count of them.
    fn row_kind(&self, owner: &str, source: &str) -> Result<Kind, NotRun> {
        let observed = self
            .world
            .lib
            .type_(owner)
            .and_then(|t| t.observations.iter().find(|(c, _)| c == source))
            .map(|(_, kind)| kind.clone());
        Ok(match source {
            "objects" => Kind::Object(owner.to_string()),
            "transitions" => Kind::Transition(owner.to_string()),
            "labels" => Kind::Label(owner.to_string()),
            "attempts" => Kind::Attempt(owner.to_string()),
            "attempt_counts" => Kind::AttemptCount(owner.to_string()),
            s if s == "intervals" || s.starts_with("intervals(") => {
                Kind::Interval(owner.to_string())
            }
            _ => match observed {
                Some(kind) => Kind::Object(kind),
                None => return not_run(format!("a metric over `{source}`")),
            },
        })
    }

    /// An evaluator over the rows of a metric of a type: a filter leaves
    /// out a row it cannot decide (§8.2).
    fn row_eval(&self, owner: Option<&'w Type>) -> Evaluator<'w> {
        let e = Evaluator::new(self.world, owner, None).filtering();
        match self.answers {
            Some(a) => e.with_answers(a),
            None => e,
        }
    }

    fn type_of(&self, name: &str) -> Result<&'w Type, NotRun> {
        self.world
            .lib
            .type_(name)
            .ok_or_else(|| NotRun::Unsupported(format!("the type {name}")))
    }

    /// A metric's value over the rows its filter, the bound dimensions and
    /// the window select, exact, and its expression's type.
    fn read(
        &self,
        owner: Option<&str>,
        spec: &Json,
        module: &str,
        bound: &Bound,
        window: Option<i64>,
    ) -> Result<(Num, Kind), NotRun> {
        let lib = self.world.lib;
        if let Some(inputs) = spec.get("input_metrics").and_then(Json::as_object) {
            // a combined metric has no rows: each input is read over the rows
            // the read selects, those matching each bound dimension it has
            // and within the window by its own time, and the values are
            // combined (ADR-0124, ADR-0128)
            let group: Vec<String> = dimensions(lib, owner, spec)?;
            for d in bound.keys() {
                if !group.contains(d) {
                    return not_run(format!(
                        "a read binding {d}, which the combined metric does not group by"
                    ));
                }
            }
            let mut values = BTreeMap::new();
            let mut kinds = BTreeMap::new();
            for (n, reference) in inputs {
                let reference = parsed(reference.as_str().unwrap_or_default())?;
                let input = find(lib, &reference, None, Some(module))?;
                let mine = dimensions(lib, input.owner.as_deref(), &input.spec)?;
                let narrowed: Bound = bound
                    .iter()
                    .filter(|(d, _)| mine.contains(d))
                    .map(|(d, v)| (d.clone(), v.clone()))
                    .collect();
                let (v, k) = self.read(
                    input.owner.as_deref(),
                    &input.spec,
                    &input.module,
                    &narrowed,
                    window,
                )?;
                values.insert(n.clone(), v);
                kinds.insert(n.clone(), k);
            }
            let tree = parsed(
                spec.get("expression")
                    .and_then(Json::as_str)
                    .unwrap_or_default(),
            )?;
            let kind = self.kind(&tree, &kinds, None)?;
            return Ok((self.combined(&tree, &values)?, kind));
        }
        let sel = self.selected(owner.unwrap_or_default(), spec, bound, window, None)?;
        let value = self.over_rows(&sel.evaluator, &sel.tree, &sel.picked, &sel.item)?;
        let env = BTreeMap::from([(sel.item.clone(), sel.rowkind.clone())]);
        let kind = self.kind(
            &sel.tree,
            &env,
            self.world.lib.type_(owner.unwrap_or_default()),
        )?;
        Ok((value, kind))
    }

    /// The rows a read of a formula selects: its source's, cut by a split
    /// and clipped by the window, then those its filter, a reader's filter,
    /// the window and every bound dimension hold for (§6.9, ADR-0114,
    /// ADR-0139, ADR-0142).
    fn selected(
        &self,
        owner: &str,
        spec: &Json,
        bound: &Bound,
        window: Option<i64>,
        theirs: Option<&Expr>,
    ) -> Result<Selected<'w>, NotRun> {
        let lib = self.world.lib;
        let owner_t = self.type_of(owner)?;
        let f = formula(lib, owner, spec)?;
        let text = |k: &str| f.get(k).and_then(Json::as_str).map(str::to_string);
        let source = text("source").unwrap_or_default();
        let item = text("item").unwrap_or_default();
        let filter = text("filter").map(|t| parsed(&t)).transpose()?;
        // what the read may narrow its rows to before it judges them: the
        // formula's filter and a reader's, each selecting nothing it leaves
        // unknown
        let narrow = match (&filter, theirs) {
            (Some(a), Some(b)) => Some(Expr::Bin(
                BinOp::And,
                Box::new(a.clone()),
                Box::new(b.clone()),
            )),
            (Some(a), None) => Some(a.clone()),
            (None, Some(b)) => Some(b.clone()),
            (None, None) => None,
        };
        // a bound dimension that is the item's own stored attribute narrows
        // the objects read to those holding its value, by its column's index,
        // where a guard bound to one claimant read every claim of the store
        // for each request (fourteenth production review)
        let mut columns: Vec<(String, Value)> = Vec::new();
        if source == "objects"
            && let Some(dims) = f.get("dimensions").and_then(Json::as_object)
        {
            for (d, want) in bound {
                let Some(Ok(Expr::Path(base, attr))) =
                    dims.get(d).and_then(Json::as_str).map(parsed)
                else {
                    continue;
                };
                let own = matches!(&*base, Expr::Name(n) if *n == item);
                let Some(a) = owner_t.attribute(&attr).filter(|a| own && !a.many) else {
                    continue;
                };
                // a read's binding, the wire's, decoded as the attribute
                // holds it, where a metric read bound to one claimant read
                // every claim (fifteenth production review); one that does
                // not decode reads the type, the binding then deciding
                let v = match want {
                    Want::Value(v) => v.clone(),
                    Want::Wire(j) => {
                        match crate::wire::decode(j, &a.declared, false, self.world.lib) {
                            Ok(v) if !v.is_absent() => v,
                            _ => continue,
                        }
                    }
                };
                columns.push((attr, v));
            }
        }
        // whether a row's `held` is read, by the formula or the reader's
        // filter, which a read over intervals otherwise does not build
        let reads_held = ["filter", "time_dimension", "expression", "split_by"]
            .iter()
            .filter_map(|k| text(k))
            .chain(
                f.get("dimensions")
                    .and_then(Json::as_object)
                    .into_iter()
                    .flatten()
                    .filter_map(|(_, e)| e.as_str().map(str::to_string)),
            )
            .any(|t| t.contains("held"))
            || theirs.is_some_and(|e| reads_member(e, "held"));
        // a window over the item's own time or date read by its column, the
        // rows at or after the window's start and no other, where a report
        // over a window read every object of the type (ADR-0210 decision 2);
        // the window still decides each row below, so this narrows what is
        // read and not what is kept
        let since = match (window, text("time_dimension"), text("split_by")) {
            (Some(w), Some(t), None) if source == "objects" => match parsed(&t)? {
                Expr::Path(base, attr)
                    if matches!(&*base, Expr::Name(n) if *n == item)
                        && owner_t.attribute(&attr).is_some_and(|a| {
                            !a.many
                                && matches!(&a.declared, Declared::Scalar(s) if s == "timestamp" || s == "date")
                        }) =>
                {
                    let from = self.world.now.saturating_sub(w);
                    let at = match owner_t.attribute(&attr).map(|a| &a.declared) {
                        Some(Declared::Scalar(s)) if s == "date" => Value::Date(from.div_euclid(86_400)),
                        _ => Value::Timestamp(from),
                    };
                    Some((attr, at))
                }
                _ => None,
            },
            _ => None,
        };
        let Rows {
            mut rows,
            kind: rowkind,
            by_kind,
        } = self.rows(
            owner,
            owner_t,
            &source,
            narrow.as_ref().map(|e| (item.as_str(), e)),
            &columns,
            since.as_ref(),
            reads_held,
        )?;
        // many rows read as a scan: the types they reach read whole once
        // each, as they reach them (the audit's cost B2, ADR-0205 decision 13)
        let scan = (rows.len() > BATCHED_ROWS).then(|| self.world.scan());
        let split = text("split_by");
        if let Some(unit) = &split {
            if !matches!(rowkind, Kind::Interval(_)) {
                return not_run(format!(
                    "a metric over {source} split by {unit}, which splits spans alone"
                ));
            }
            // each span cut at the boundaries of the buckets it covers, and a
            // window clips the pieces (ADR-0142)
            let member = source
                .strip_prefix("intervals(")
                .and_then(|m| m.strip_suffix(')'))
                .map_or("state", str::trim);
            rows = self.pieces(owner, member, unit)?;
            if let Some(w) = window {
                let now = self.world.now;
                rows = rows
                    .into_iter()
                    .filter_map(|r| clipped(r, now - w, now))
                    .collect();
            }
        }
        let evaluator = self.row_eval(Some(owner_t));
        let mut dims: BTreeMap<String, Expr> = BTreeMap::new();
        dims.insert(
            "version".into(),
            parsed(&format!("{item}.declaration_version"))?,
        );
        dims.insert("actor_kind".into(), parsed(&format!("{item}.{by_kind}"))?);
        for (d, e) in f
            .get("dimensions")
            .and_then(Json::as_object)
            .into_iter()
            .flatten()
        {
            dims.insert(d.clone(), parsed(e.as_str().unwrap_or_default())?);
        }
        for d in bound.keys() {
            if !dims.contains_key(d) {
                return not_run(format!(
                    "a read binding {d}, which the metric does not have"
                ));
            }
        }
        let stamp = match (window, text("time_dimension"), &split) {
            (Some(_), _, Some(_)) | (None, _, _) => None,
            (Some(_), Some(t), None) => Some(parsed(&t)?),
            (Some(_), None, None) => return not_run("a window on a metric with no time dimension"),
        };
        let now = self.world.now;
        let mut picked = Vec::new();
        for r in rows {
            let env = BTreeMap::from([(item.clone(), r.clone())]);
            // a filter that is unknown selects nothing (§8.2)
            if let Some(w) = &filter
                && truth(&evaluator.eval_in(w, &env)?)? != Some(true)
            {
                continue;
            }
            // a reader's filter narrows the rows likewise (ADR-0114); one a
            // row cannot answer is the reader's malformed filter, never a
            // 501 whose fault carries what it met (ninth production review
            // N-3, ADR-0196 decision 7)
            if let Some(w) = theirs {
                match evaluator.eval_in(w, &env).and_then(|v| truth(&v)) {
                    Ok(Some(true)) => {}
                    Ok(_) => continue,
                    Err(NotRun::Unsupported(why)) => {
                        return Err(NotRun::Unsupported(format!("{THEIR_FILTER}{why}")));
                    }
                    Err(e) => return Err(e),
                }
            }
            if let (Some(s), Some(w)) = (&stamp, window) {
                // the window holds its start, and a row with no time is in
                // none (ADR-0139)
                let at = match evaluator.eval_in(s, &env)? {
                    Value::Timestamp(t) => Some(t),
                    Value::Date(d) => Some(d * 86_400),
                    _ => None,
                };
                if !at.is_some_and(|at| now - w <= at && at <= now) {
                    continue;
                }
            }
            let mut keep = true;
            for (d, v) in bound {
                let reached = self.reach(&evaluator, &dims[d], &env)?;
                let matches = match reached {
                    Reach::One(got) => v.matches(&got),
                    Reach::Many(got) => got.iter().any(|g| v.matches(g)),
                };
                if !matches {
                    keep = false;
                    break;
                }
            }
            if keep {
                picked.push(r);
            }
        }
        let tree = parsed(&text("expression").unwrap_or_default())?;
        let read = ["filter", "time_dimension", "expression"]
            .iter()
            .filter_map(|k| text(k))
            .chain(
                f.get("dimensions")
                    .and_then(Json::as_object)
                    .into_iter()
                    .flatten()
                    .filter_map(|(_, e)| e.as_str().map(str::to_string)),
            )
            .collect::<Vec<_>>()
            .join(" ");
        Ok(Selected {
            picked,
            item,
            rowkind,
            dims,
            evaluator,
            tree,
            reads_created: read.contains("created_at"),
            reads_flow: ["entered_at(", "time_in(", "intervals"]
                .iter()
                .any(|w| read.contains(w)),
            _scan: scan,
        })
    }

    /// A metric's rows (`declaration-syntax.md` §6.9): each object of the
    /// type or its family, each span of its state or of a tracked member,
    /// each event of a transition, or each observation of one of its kinds
    /// that nothing has corrected.
    #[allow(clippy::too_many_arguments)]
    fn rows(
        &self,
        owner: &str,
        t: &Type,
        source: &str,
        narrow: Option<(&str, &Expr)>,
        columns: &[(String, Value)],
        since: Option<&(String, Value)>,
        reads_held: bool,
    ) -> Result<Rows, NotRun> {
        let world = self.world;
        if source == "objects" {
            let objects = if columns.is_empty() && since.is_none() {
                world.of_type(owner)?
            } else {
                world.of_type_where_since(
                    owner,
                    None,
                    columns,
                    since.map(|(a, v)| (a.as_str(), v)),
                )?
            };
            return Ok(Rows {
                rows: objects
                    .iter()
                    .map(|o| Value::Object(o.id.clone()))
                    .collect(),
                kind: Kind::Object(owner.to_string()),
                by_kind: "created_by_kind",
            });
        }
        if source == "attempts" || source == "attempt_counts" {
            // the rows of the family's types, read by the index on their
            // type, and never the owner's objects, which they do not read,
            // where each read the type whole and the whole attempt log (the
            // audit's cost B4, B5); a row of no type, an unknown id's, is
            // none of them (DESIGN.md §7)
            if source == "attempt_counts" {
                return Ok(Rows {
                    rows: attempt_count_rows(world, owner)?,
                    kind: Kind::AttemptCount(owner.to_string()),
                    by_kind: "actor_kind",
                });
            }
            let mut mine = Vec::new();
            for tn in world.family(owner).iter() {
                for a in world.attempts_of_type(tn)? {
                    mine.push(attempt_row(world, &a)?);
                }
            }
            return Ok(Rows {
                rows: mine,
                kind: Kind::Attempt(owner.to_string()),
                by_kind: "actor_kind",
            });
        }
        let family = world.of_type(owner)?;
        if source == "intervals" || source.starts_with("intervals(") {
            let member = source
                .strip_prefix("intervals(")
                .and_then(|m| m.strip_suffix(')'))
                .map_or("state", str::trim);
            let mut rows = Vec::new();
            // a read that never reads `held` reads the state's spans and the
            // member's by type, where it read each object's record whole,
            // its attribute writes among them, and each row's `held` (the
            // audit's cost B3, ADR-0205 decision 13); an object whose record
            // the read holds, as an operation may have changed it, from it
            let by_type: BTreeMap<String, crate::eval::Spans> = if reads_held {
                BTreeMap::new()
            } else {
                let mut read = BTreeMap::new();
                for tn in world.family(owner).iter() {
                    if let Some(spans) = world.type_spans(tn, &["state", member])? {
                        read.extend(spans);
                    }
                }
                read
            };
            let none = crate::eval::Spans::new();
            for o in &family {
                let ot = self.type_of(&o.type_)?;
                match o.parts.get() {
                    _ if reads_held || world.reads_no_store() => {
                        rows.extend(interval_rows(world, o, ot, member)?);
                    }
                    Some(p) => rows.extend(crate::eval::interval_rows_from(
                        world, o, ot, member, &p.spans, false,
                    )?),
                    None => rows.extend(crate::eval::interval_rows_from(
                        world,
                        o,
                        ot,
                        member,
                        by_type.get(&o.id).unwrap_or(&none),
                        false,
                    )?),
                }
            }
            return Ok(Rows {
                rows,
                kind: Kind::Interval(owner.to_string()),
                by_kind: "entered_by_kind",
            });
        }
        if source == "transitions" {
            let by_id: BTreeMap<&str, &Object> =
                family.iter().map(|o| (o.id.as_str(), *o)).collect();
            // where the filter holds only of a transition that completes its
            // object, the events entering a finished state alone, since one
            // entering an open state completes nothing: throughput read every
            // event of the type to keep the few that close (the audit's cost
            // B3, ADR-0205 decision 13)
            let entering: Option<Vec<String>> = narrow
                .filter(|(item, e)| completing(e, item))
                .map(|_| finished_states(world, owner));
            // the family's events, a page at a time by their types' index and
            // the read's deadline between pages, where it read the whole log
            // (the audit's cost B3, ADR-0205 decision 13)
            const PAGE: usize = 4_096;
            let types: Vec<String> = world.family(owner).to_vec();
            let mut rows = Vec::new();
            let mut after = 0;
            loop {
                world.due()?;
                let page = world.log_after(after, Some(&types), entering.as_deref(), PAGE)?;
                // what each transition held, which its row reads from the
                // flow data, read together for the page's objects alone,
                // where every object's was read before any event
                let mut theirs: Vec<&Object> = Vec::new();
                let mut seen: BTreeSet<&str> = BTreeSet::new();
                for e in &page {
                    if let Some(o) = by_id.get(e.object_id.as_str())
                        && seen.insert(o.id.as_str())
                    {
                        theirs.push(o);
                    }
                }
                world.with_parts(&theirs)?;
                for e in &page {
                    if let Some(o) = by_id.get(e.object_id.as_str()) {
                        let p = world.parts(o)?;
                        rows.push(transition_row(world, o, p, self.type_of(&o.type_)?, e)?);
                    }
                }
                match page.last() {
                    Some(e) if page.len() == PAGE => after = e.position,
                    _ => break,
                }
            }
            return Ok(Rows {
                rows,
                kind: Kind::Transition(owner.to_string()),
                by_kind: "actor_kind",
            });
        }
        if let Some((_, kind)) = t.observations.iter().find(|(c, _)| c == source) {
            return Ok(Rows {
                rows: uncorrected_on(&family, &world.of_type(kind)?, &world.corrected(kind)?),
                kind: Kind::Object(kind.clone()),
                by_kind: "recorded_by_kind",
            });
        }
        if source == "labels" {
            // each label on an object of the family that nothing has
            // corrected (declaration-syntax.md §6.9): where the filters
            // require values of a label's stored attributes, only the labels
            // holding them are read, by the label table's indexes, any other
            // failing a conjunct outright, and the corrections by their own
            // index, since a correction may carry another name (ADR-0162
            // step 3, storage-schema.md §6)
            let label = crate::model::LABEL;
            let narrowed = narrow.and_then(|(item, e)| {
                self.row_eval(None)
                    .narrowing(label, item, e, &BTreeMap::new())
            });
            let labels = match narrowed {
                Some((states, columns)) => world.of_type_where(label, states.as_ref(), &columns)?,
                None => world.of_type(label)?,
            };
            return Ok(Rows {
                rows: uncorrected_on(&family, &labels, &world.corrected(label)?),
                kind: Kind::Label(owner.to_string()),
                by_kind: "recorded_by_kind",
            });
        }
        not_run(format!("a metric over `{source}`"))
    }

    /// Each span of a type's family cut at the bucket boundaries it covers,
    /// each piece with its period, its bounds and its duration (ADR-0142).
    fn pieces(&self, owner: &str, member: &str, unit: &str) -> Result<Vec<Value>, NotRun> {
        let mut out = Vec::new();
        let family = self.world.of_type(owner)?;
        self.world.with_parts(&family)?;
        for o in family {
            let t = self.type_of(&o.type_)?;
            let p = self.world.parts(o)?;
            let spans = p.spans.get(member).map_or(&[][..], Vec::as_slice);
            let rows = interval_rows(self.world, o, t, member)?;
            for (sp, row) in spans.iter().zip(rows) {
                out.extend(pieces(self.world, p, t, member, sp, row, unit));
            }
        }
        Ok(out)
    }

    /// A dimension's value for one row: a path through a set end reaches each
    /// distinct value (ADR-0123).
    fn reach(
        &self,
        ev: &Evaluator<'w>,
        e: &Expr,
        env: &BTreeMap<String, Value>,
    ) -> Result<Reach, NotRun> {
        let Expr::Path(base, member) = e else {
            return Ok(Reach::One(ev.eval_in(e, env)?));
        };
        let of = |b: &Value| -> Result<Value, NotRun> {
            let mut inner = env.clone();
            inner.insert("\u{0}base".into(), b.clone());
            ev.eval_in(
                &Expr::Path(Box::new(Expr::Name("\u{0}base".into())), member.clone()),
                &inner,
            )
        };
        let spread = |v: Value, out: &mut Vec<Value>| match v {
            Value::Set(items) => {
                for x in items {
                    if !x.is_absent() && !out.iter().any(|o: &Value| o.equals(&x)) {
                        out.push(x);
                    }
                }
            }
            x => {
                if !x.is_absent() && !out.iter().any(|o| o.equals(&x)) {
                    out.push(x);
                }
            }
        };
        match self.reach(ev, base, env)? {
            Reach::One(Value::Set(items)) | Reach::Many(items) => {
                let mut out = Vec::new();
                for b in &items {
                    spread(of(b)?, &mut out);
                }
                Ok(Reach::Many(out))
            }
            Reach::One(b) => Ok(match of(&b)? {
                Value::Set(items) => Reach::Many(items),
                v => Reach::One(v),
            }),
        }
    }

    /// A metric's value over one group's rows: its own aggregates range over
    /// them, leaving out a row whose body is absent, and the arithmetic
    /// between them is the language's (§6.9, §8.3).
    fn over_rows(
        &self,
        ev: &Evaluator<'w>,
        e: &Expr,
        rows: &[Value],
        item: &str,
    ) -> Result<Num, NotRun> {
        let env_of = |r: &Value| BTreeMap::from([(item.to_string(), r.clone())]);
        let count = |n: usize| Num::Int(i128::try_from(n).unwrap_or(i128::MAX));
        match e {
            // count(where f), count(distinct b)
            Expr::Agg {
                kind,
                distinct,
                var: None,
                filter,
                body,
                ..
            } => {
                if kind != "count" {
                    return not_run(format!("`{kind}` over a metric's rows with a filter"));
                }
                let mut picked = Vec::new();
                for r in rows {
                    let keep = match filter {
                        None => true,
                        Some(f) => truth(&ev.eval_in(f, &env_of(r))?)? == Some(true),
                    };
                    if keep {
                        picked.push(r);
                    }
                }
                if !distinct {
                    return Ok(count(picked.len()));
                }
                let mut seen: Vec<Value> = Vec::new();
                for r in picked {
                    let v = match body {
                        Some(b) => ev.eval_in(b, &env_of(r))?,
                        None => Value::Absent,
                    };
                    if !v.is_absent() && !seen.iter().any(|s| s.equals(&v)) {
                        seen.push(v);
                    }
                }
                Ok(count(seen.len()))
            }
            Expr::Call(function, args) if matches!(&**function, Expr::Name(n) if ROW_AGGREGATES.contains(&n.as_str())) =>
            {
                let Expr::Name(f) = &**function else {
                    return not_run("an aggregate");
                };
                if f == "count" {
                    if !args.is_empty() {
                        return not_run("`count` of an expression over a metric's rows");
                    }
                    return Ok(count(rows.len()));
                }
                let Some(body) = args.last() else {
                    return not_run(format!("`{f}` with no body"));
                };
                let mut values = Vec::new();
                for r in rows {
                    let v = Num::of(&ev.eval_in(body, &env_of(r))?)?;
                    if v != Num::Absent {
                        values.push(v);
                    }
                }
                if f == "sum" {
                    let mut total = Num::Int(0);
                    for v in values {
                        total = Num::combine(BinOp::Add, total, v)?;
                    }
                    return Ok(total);
                }
                if values.is_empty() {
                    // min, max, avg, median and a percentile over no rows
                    // are absent (§8.3)
                    return Ok(Num::Absent);
                }
                let order = |a: &Num, b: &Num| a.rat().cmp(&b.rat());
                match f.as_str() {
                    "min" => Ok(values.into_iter().min_by(order).unwrap_or(Num::Absent)),
                    "max" => Ok(values.into_iter().max_by(order).unwrap_or(Num::Absent)),
                    "avg" => {
                        // exact: the metric's value is rounded once (ADR-0146)
                        let n = i128::try_from(values.len()).unwrap_or(i128::MAX);
                        let mut total = Rat::int(0);
                        for v in &values {
                            total = total
                                .checked_add(v.rat().unwrap_or(Rat::int(0)))
                                .ok_or_else(|| NotRun::OutOfRange(String::new()))?;
                        }
                        Ok(total
                            .checked_div(Rat::int(n))
                            .map_or(Num::Absent, Num::Frac))
                    }
                    _ => {
                        // nearest rank: the smallest whose rank is at least
                        // p × n, a median the lower middle (ADR-0147)
                        let p = if f == "median" {
                            Rat::new(1, 2).unwrap_or(Rat::int(0))
                        } else {
                            let Some(p) = args.first() else {
                                return not_run("a percentile with no p");
                            };
                            match Num::of(&ev.eval_in(p, &BTreeMap::new())?)?.rat() {
                                Some(p) => p,
                                None => return Ok(Num::Absent),
                            }
                        };
                        let mut ordered = values;
                        ordered.sort_by(order);
                        let n = i128::try_from(ordered.len()).unwrap_or(i128::MAX);
                        let rank = p
                            .checked_mul(Rat::int(n))
                            .ok_or_else(|| NotRun::OutOfRange(String::new()))?
                            .ceil()
                            .max(1);
                        let at = usize::try_from(rank - 1)
                            .unwrap_or(usize::MAX)
                            .min(ordered.len() - 1);
                        Ok(ordered[at])
                    }
                }
            }
            Expr::Bin(op, a, b) if !matches!(op, BinOp::And | BinOp::Or | BinOp::Implies) => {
                Num::combine(
                    *op,
                    self.over_rows(ev, a, rows, item)?,
                    self.over_rows(ev, b, rows, item)?,
                )
            }
            Expr::Neg(x) => {
                Num::combine(BinOp::Sub, Num::Int(0), self.over_rows(ev, x, rows, item)?)
            }
            Expr::Num(_) | Expr::Dur(..) | Expr::Money(..) => {
                Num::of(&ev.eval_in(e, &BTreeMap::new())?)
            }
            Expr::Name(n) if n == "now" => Num::of(&ev.eval_in(e, &BTreeMap::new())?),
            _ => not_run("a metric's value reading its rows outside an aggregate"),
        }
    }

    /// A combined metric's value from its inputs' exact values (ADR-0124).
    fn combined(&self, e: &Expr, values: &BTreeMap<String, Num>) -> Result<Num, NotRun> {
        match e {
            Expr::Name(n) => values
                .get(n)
                .copied()
                .ok_or_else(|| NotRun::Unsupported(format!("the input metric {n}"))),
            Expr::Bin(op, a, b) => {
                Num::combine(*op, self.combined(a, values)?, self.combined(b, values)?)
            }
            Expr::Neg(x) => Num::combine(BinOp::Sub, Num::Int(0), self.combined(x, values)?),
            Expr::Num(_) | Expr::Dur(..) | Expr::Money(..) => {
                Num::of(&Evaluator::new(self.world, None, None).eval(e)?)
            }
            other => not_run(format!("a combined metric's expression {other:?}")),
        }
    }

    /// An expression's type, as far as a metric's scale needs it (§8.1, §8.3).
    fn kind(
        &self,
        e: &Expr,
        env: &BTreeMap<String, Kind>,
        this: Option<&Type>,
    ) -> Result<Kind, NotRun> {
        Ok(match e {
            Expr::Num(Number::Int(_)) => Kind::Int,
            Expr::Num(Number::Decimal(t)) => Kind::Literal(scale_of(t)),
            Expr::Dur(..) => Kind::Duration,
            Expr::Money(c, _) => Kind::Money(c.clone()),
            Expr::Bool(_) | Expr::Not(_) | Expr::IsNull(..) | Expr::In(..) | Expr::Changed(..) => {
                Kind::Bool
            }
            Expr::Name(n) => match env.get(n) {
                Some(k) => k.clone(),
                None if n == "now" => Kind::Timestamp,
                None => match this {
                    Some(t) => self.member_kind(&Kind::Object(t.name.clone()), n)?,
                    None => Kind::Other,
                },
            },
            Expr::Path(base, m) => self.member_kind(&self.kind(base, env, this)?, m)?,
            Expr::Neg(x) => self.kind(x, env, this)?,
            Expr::If(_, a, _) => self.kind(a, env, this)?,
            Expr::Bin(op, a, b) => {
                if matches!(
                    op,
                    BinOp::And
                        | BinOp::Or
                        | BinOp::Implies
                        | BinOp::Eq
                        | BinOp::Ne
                        | BinOp::Lt
                        | BinOp::Le
                        | BinOp::Gt
                        | BinOp::Ge
                ) {
                    Kind::Bool
                } else {
                    arith_kind(*op, self.kind(a, env, this)?, self.kind(b, env, this)?)
                }
            }
            Expr::Agg {
                kind,
                var,
                collection,
                body,
                ..
            } => {
                if matches!(kind.as_str(), "count") {
                    return Ok(Kind::Int);
                }
                if matches!(kind.as_str(), "all" | "any" | "none") {
                    return Ok(Kind::Bool);
                }
                let mut inner = env.clone();
                if let (Some(v), Some(c)) = (var, collection) {
                    let element = match self.kind(c, env, this)? {
                        Kind::Objects(t) => Kind::Object(t),
                        Kind::Intervals(t) => Kind::Interval(t),
                        Kind::Transitions(t) => Kind::Transition(t),
                        _ => Kind::Other,
                    };
                    inner.insert(v.clone(), element);
                }
                match body {
                    Some(b) => self.aggregate_kind(kind, b, &inner, this)?,
                    None => Kind::Other,
                }
            }
            Expr::Call(function, args) => match &**function {
                Expr::Name(f) if f == "count" || f == "length" => Kind::Int,
                Expr::Name(f) if ROW_AGGREGATES.contains(&f.as_str()) => match args.last() {
                    Some(b) => self.aggregate_kind(f, b, env, this)?,
                    None => Kind::Other,
                },
                Expr::Name(f) if f == "entered_at" => Kind::Timestamp,
                Expr::Name(f) if f == "time_in" => Kind::Duration,
                Expr::Name(_) => Kind::Other,
                Expr::Path(base, m) => {
                    let base = self.kind(base, env, this)?;
                    match (m.as_str(), &base, args.as_slice()) {
                        ("entered_at", _, _) => Kind::Timestamp,
                        ("time_in", _, _) => Kind::Duration,
                        ("intervals", Kind::Object(t), _) => Kind::Intervals(t.clone()),
                        ("held", Kind::Interval(t) | Kind::Transition(t), [Expr::Name(r)]) => {
                            self.member_kind(&Kind::Object(t.clone()), r)?
                        }
                        _ => Kind::Other,
                    }
                }
                _ => Kind::Other,
            },
            _ => Kind::Other,
        })
    }

    /// An aggregate's type: its body's, except that an average of integers
    /// is a decimal (ADR-0106, ADR-0139).
    fn aggregate_kind(
        &self,
        f: &str,
        body: &Expr,
        env: &BTreeMap<String, Kind>,
        this: Option<&Type>,
    ) -> Result<Kind, NotRun> {
        let b = self.kind(body, env, this)?;
        if f != "avg" {
            return Ok(b);
        }
        Ok(match b {
            Kind::Int => Kind::Decimal(AVERAGE_SCALE),
            Kind::Decimal(s) | Kind::Literal(s) => Kind::Decimal(s),
            Kind::Duration | Kind::Money(_) => b,
            _ => return not_run("an average of values that have no scale"),
        })
    }

    fn member_kind(&self, base: &Kind, m: &str) -> Result<Kind, NotRun> {
        Ok(match base {
            Kind::Interval(t) => match m {
                "entered_at" | "left_at" | "piece_start" | "piece_end" => Kind::Timestamp,
                "duration" => Kind::Duration,
                "object" => Kind::Object(t.clone()),
                _ => Kind::Other,
            },
            Kind::Transition(t) => match m {
                "occurred_at" | "recorded_at" => Kind::Timestamp,
                "object" => Kind::Object(t.clone()),
                _ => Kind::Other,
            },
            Kind::Attempt(t) => match m {
                "at" => Kind::Timestamp,
                "object" => Kind::Object(t.clone()),
                _ => Kind::Other,
            },
            Kind::AttemptCount(t) => match m {
                "count" => Kind::Int,
                "object" => Kind::Object(t.clone()),
                _ => Kind::Other,
            },
            // a label of the metric's type is on an object of it
            Kind::Label(t) => match m {
                "subject" => Kind::Object(t.clone()),
                _ => return self.member_kind(&Kind::Object(crate::model::LABEL.into()), m),
            },
            Kind::Object(tn) => {
                let Some(t) = self.world.lib.type_(tn) else {
                    return Ok(Kind::Other);
                };
                if matches!(
                    m,
                    "created_at" | "recorded_at" | "occurred_at" | "recorded_from" | "imported_at"
                ) {
                    return Ok(Kind::Timestamp);
                }
                if m == "intervals" {
                    return Ok(Kind::Intervals(tn.clone()));
                }
                if m == "transitions" {
                    return Ok(Kind::Transitions(tn.clone()));
                }
                if let Some((_, kind)) = t.observations.iter().find(|(c, _)| c == m) {
                    return Ok(Kind::Objects(kind.clone()));
                }
                if let Some(spec) = t.source.get("attributes").and_then(|a| a.get(m)) {
                    return Ok(spec_kind(spec));
                }
                if let Some(a) = t.attribute(m) {
                    // an attribute the declaration's source does not spell,
                    // such as an observation's subject
                    return Ok(match &a.declared {
                        Declared::Reference(r) if a.many => Kind::Objects(r.clone()),
                        Declared::Reference(r) => Kind::Object(r.clone()),
                        Declared::Scalar(s) => spec_kind(&json!({ "type": s })),
                    });
                }
                if let Some((_, e)) = t.derived.iter().find(|(d, _)| d == m) {
                    let key = (tn.clone(), m.to_string());
                    {
                        let typing = self.typing.borrow();
                        match typing.get(&key) {
                            Some(Some(known)) => return Ok(known.clone()),
                            // reading itself, or past the depth the
                            // checkers type to: a kind not found
                            Some(None) => return Ok(Kind::Other),
                            None if typing.values().filter(|k| k.is_none()).count()
                                >= TYPING_DEPTH =>
                            {
                                return Ok(Kind::Other);
                            }
                            None => {}
                        }
                    }
                    self.typing.borrow_mut().insert(key.clone(), None);
                    let found = self.kind(e, &BTreeMap::new(), Some(t));
                    let known = found.as_ref().map_or(Kind::Other, Clone::clone);
                    self.typing.borrow_mut().insert(key, Some(known));
                    return found;
                }
                Kind::Other
            }
            _ => Kind::Other,
        })
    }
}

/// The scale a decimal literal is written to.
fn scale_of(text: &str) -> u8 {
    u8::try_from(text.split_once('.').map_or(0, |(_, f)| f.len())).unwrap_or(u8::MAX)
}

/// An attribute's type, as far as a value's scale needs it.
fn spec_kind(spec: &Json) -> Kind {
    if let Some(r) = spec.get("reference").and_then(Json::as_str) {
        return match r.strip_suffix("[]") {
            Some(t) => Kind::Objects(t.to_string()),
            None => Kind::Object(r.to_string()),
        };
    }
    let typ = spec.get("type").and_then(Json::as_str).unwrap_or_default();
    if let Some(args) = typ
        .strip_prefix("decimal(")
        .and_then(|a| a.strip_suffix(')'))
    {
        let scale = args
            .split(',')
            .nth(1)
            .and_then(|s| s.trim().parse::<u8>().ok());
        return scale.map_or(Kind::Other, Kind::Decimal);
    }
    if let Some(c) = typ.strip_prefix("money(").and_then(|a| a.strip_suffix(')')) {
        return Kind::Money(c.trim().to_string());
    }
    match typ {
        "int" | "counter" => Kind::Int,
        "duration" => Kind::Duration,
        "timestamp" => Kind::Timestamp,
        "date" => Kind::Date,
        "bool" => Kind::Bool,
        _ => Kind::Other,
    }
}

/// The type of `a op b` as `declaration-syntax.md` §8.3 gives it, for the
/// scale of a metric's value.
fn arith_kind(op: BinOp, a: Kind, b: Kind) -> Kind {
    let scalar = |k: &Kind| matches!(k, Kind::Int | Kind::Decimal(_) | Kind::Literal(_));
    let places = |k: &Kind| match k {
        Kind::Decimal(s) | Kind::Literal(s) => *s,
        _ => 0,
    };
    // a decimal literal takes the scale of what it is combined with (§8.1)
    let (a, b) = match (&a, &b) {
        (Kind::Literal(_), Kind::Decimal(_)) => (b.clone(), b),
        (Kind::Decimal(_), Kind::Literal(_)) => (a.clone(), a),
        _ => (a, b),
    };
    match op {
        BinOp::Add | BinOp::Sub => {
            if op == BinOp::Sub
                && matches!(
                    (&a, &b),
                    (Kind::Timestamp, Kind::Timestamp) | (Kind::Date, Kind::Date)
                )
            {
                return Kind::Duration;
            }
            // a date moved by a duration is a date (§8.3, ADR-0145)
            if a == Kind::Date && b == Kind::Duration {
                return Kind::Date;
            }
            if (a == Kind::Timestamp && b == Kind::Duration)
                || (op == BinOp::Add && a == Kind::Duration && b == Kind::Timestamp)
            {
                return Kind::Timestamp;
            }
            if let (Kind::Literal(x), Kind::Literal(y)) = (&a, &b) {
                return Kind::Literal(*x.max(y));
            }
            if a == b { a } else { Kind::Other }
        }
        BinOp::Mul => {
            if scalar(&a) && scalar(&b) {
                if a == Kind::Int && b == Kind::Int {
                    return Kind::Int;
                }
                return Kind::Decimal(places(&a).saturating_add(places(&b)));
            }
            if scalar(&a) {
                b
            } else if scalar(&b) {
                a
            } else {
                Kind::Other
            }
        }
        BinOp::Div => {
            if a == b && matches!(a, Kind::Duration | Kind::Money(_)) {
                return Kind::Decimal(6); // a quotient of two like quantities
            }
            if scalar(&b) {
                if let Kind::Literal(s) = a {
                    return Kind::Decimal(s);
                }
                return a;
            }
            Kind::Other
        }
        _ => Kind::Other,
    }
}

/// A metric's value rounded half to even to the scale of its expression's
/// type (ADR-0128), as a value a guard compares.
fn rounded(v: Num, kind: &Kind) -> Result<Value, NotRun> {
    let Some(r) = v.rat() else {
        return Ok(Value::Absent);
    };
    let past = || NotRun::OutOfRange(String::new());
    let whole = |places: u8| r.half_even(places).ok_or_else(past);
    let int64 = |n: i128| i64::try_from(n).map_err(|_| past());
    Ok(match kind {
        Kind::Int => match v {
            Num::Int(n) => Value::Int(int64(n)?),
            _ => Value::Int(int64(whole(0)?)?),
        },
        Kind::Decimal(s) | Kind::Literal(s) => {
            Value::Decimal(Decimal::new(whole(*s)?, *s).ok_or_else(past)?)
        }
        Kind::Money(c) => {
            let Some(places) = minor_unit(c) else {
                return not_run(format!(
                    "an amount in {c}, whose minor unit the core does not know"
                ));
            };
            Value::Money {
                currency: c.clone(),
                amount: Decimal::new(whole(places)?, places).ok_or_else(past)?,
            }
        }
        // a duration is held in whole seconds (storage-schema.md §3.1)
        Kind::Duration => Value::Duration(int64(whole(0)?)?),
        Kind::Timestamp if r.is_integer() => Value::Timestamp(int64(r.trunc())?),
        // a date by its day, where its seconds were answered as an integer
        // (the differential battery, 2026-10-15)
        Kind::Date if r.is_integer() && r.trunc().rem_euclid(86_400) == 0 => {
            Value::Date(int64(r.trunc().div_euclid(86_400))?)
        }
        // part of a day has no date, which is out of range, never unknown
        // (ADR-0205 decision 2)
        Kind::Date => return Err(past()),
        _ if r.is_integer() => Value::Int(int64(r.trunc())?),
        _ => return not_run("a metric's fractional value, whose type the core cannot tell"),
    })
}

/// A span cut at every bucket boundary it crosses, each piece with its
/// bucket, its bounds and its duration; a span with no duration is one
/// piece, in the bucket it began, with none (ADR-0142, ADR-0147). A
/// member's span counts only the time its object was open (ADR-0148).
fn pieces(
    world: &World<'_>,
    p: &crate::world::Parts,
    t: &Type,
    member: &str,
    sp: &crate::world::Span,
    row: Value,
    unit: &str,
) -> Vec<Value> {
    let Value::Row(fields) = row else {
        return Vec::new();
    };
    let start = sp.entered_at;
    let with = |period: String, from: Value, to: Value, duration: Value| {
        let mut f = (*fields).clone();
        f.insert("period".into(), Value::Text(period));
        f.insert("piece_start".into(), from);
        f.insert("piece_end".into(), to);
        f.insert("duration".into(), duration);
        Value::row(f)
    };
    let Some(Value::Duration(d)) = fields.get("duration").cloned() else {
        return vec![with(
            crate::eval::bucket(unit, start),
            Value::Timestamp(start),
            Value::Absent,
            Value::Absent,
        )];
    };
    let (end, closed) = if member == "state" {
        (start + d, Vec::new())
    } else {
        (
            sp.left_at.unwrap_or(world.now),
            closed_periods(p, t, world.now),
        )
    };
    let mut out = Vec::new();
    let mut at = start;
    loop {
        let cut = next_boundary(unit, at).min(end);
        let shut: i64 = closed
            .iter()
            .map(|(a, b)| (cut.min(*b) - at.max(*a)).max(0))
            .sum();
        out.push(with(
            crate::eval::bucket(unit, at),
            Value::Timestamp(at),
            Value::Timestamp(cut),
            Value::Duration(cut - at - shut),
        ));
        if cut >= end {
            return out;
        }
        at = cut;
    }
}

/// The start of the UTC calendar bucket after the one a time is in, a week
/// beginning on Monday (ADR-0106).
fn next_boundary(unit: &str, seconds: i64) -> i64 {
    use crate::value::{civil_from_days, days_from_civil};
    let days = seconds.div_euclid(86_400);
    let (y, m, _) = civil_from_days(days);
    let next = match unit {
        "day" => days + 1,
        "week" => days - (days + 3).rem_euclid(7) + 7,
        "month" => {
            if m == 12 {
                days_from_civil(y + 1, 1, 1)
            } else {
                days_from_civil(y, m + 1, 1)
            }
        }
        "quarter" => {
            let first = (m - 1) / 3 * 3 + 4;
            if first > 12 {
                days_from_civil(y + 1, first - 12, 1)
            } else {
                days_from_civil(y, first, 1)
            }
        }
        _ => days_from_civil(y + 1, 1, 1),
    };
    next * 86_400
}

/// The part of a piece between two times, both included, or none where it
/// has none there (ADR-0142 decision 4).
fn clipped(piece: Value, low: i64, high: i64) -> Option<Value> {
    let Value::Row(mut shared) = piece else {
        return None;
    };
    let time = |v: Option<&Value>| match v {
        Some(Value::Timestamp(t)) => Some(*t),
        _ => None,
    };
    let start = time(shared.get("piece_start"))?;
    match shared.get("duration") {
        Some(Value::Duration(d)) if *d != 0 => {}
        _ => return (low <= start && start <= high).then_some(Value::Row(shared)),
    }
    let end = time(shared.get("piece_end"))?;
    let (a, b) = (start.max(low), end.min(high));
    if b <= a {
        return None;
    }
    let f = std::sync::Arc::make_mut(&mut shared);
    f.insert("piece_start".into(), Value::Timestamp(a));
    f.insert("piece_end".into(), Value::Timestamp(b));
    f.insert("duration".into(), Value::Duration(b - a));
    Some(Value::Row(shared))
}

/// Whether every name a read's filter starts from is one it may: the
/// metric's item, `now`, a type, an enumeration or a kind of actor (D569).
fn reads_only_its_rows(lib: &Library, item: &str, e: &Expr) -> bool {
    fn within(lib: &Library, bound: &[&str], e: &Expr) -> bool {
        match e {
            Expr::Name(n) => {
                bound.contains(&n.as_str())
                    || ["now", "human", "agent", "service"].contains(&n.as_str())
                    || lib.types.contains_key(n)
                    || lib.enumerations.contains_key(n)
            }
            // an aggregation binds its own item over what it ranges
            Expr::Agg { var: Some(v), .. } => {
                let mut inner = bound.to_vec();
                inner.push(v.as_str());
                e.children().into_iter().all(|c| within(lib, &inner, c))
            }
            _ => e.children().into_iter().all(|c| within(lib, bound, c)),
        }
    }
    within(lib, &[item], e)
}

/// A metric's value, as a guard reads it: its rows those of the store as
/// the request found it, narrowed to the dimensions bound and the window,
/// and rounded once (`declaration-syntax.md` §6.9, ADR-0128).
pub fn read(
    world: &World<'_>,
    answers: Option<&BTreeMap<String, String>>,
    found: &Found,
    bound: &BTreeMap<String, Value>,
    window: Option<i64>,
) -> Result<Value, NotRun> {
    let reader = Reader {
        world,
        answers,
        typing: Default::default(),
    };
    let bound: Bound = bound
        .iter()
        .map(|(d, v)| (d.clone(), Want::Value(v.clone())))
        .collect();
    let (v, kind) = reader.read(
        found.owner.as_deref(),
        &found.spec,
        &found.module,
        &bound,
        window,
    )?;
    rounded(v, &kind)
}

/// One row of a reader's read: a group of the dimensions kept, its value,
/// and the objects it read from before what their record vouches for
/// (`library-api.md` §5, ADR-0106).
#[derive(Clone, Debug, PartialEq)]
pub struct PageRow {
    pub dimensions: Vec<(String, Value)>,
    pub value: Value,
    pub gaps: Vec<String>,
}

/// A declared flag on one of the declared groups, raised or unknown there
/// (ADR-0146).
#[derive(Clone, Debug, PartialEq)]
pub struct RaisedFlag {
    pub flag: String,
    pub group: Vec<(String, Value)>,
    pub unknown: bool,
}

/// A page as the wire gives it, the `metric` read's and a draft's evidence
/// snapshot's alike (`library-api.md` §5, ADR-0159): its rows, its flags,
/// whether the history it read is complete, and its refusal, a value.
pub fn page_json(page: &Page) -> serde_json::Value {
    let group = |pairs: &[(String, Value)]| {
        serde_json::Value::Object(
            pairs
                .iter()
                .map(|(d, v)| (d.clone(), crate::wire::encode(v)))
                .collect(),
        )
    };
    let mut out = serde_json::json!({
        "rows": page.rows.iter().map(|r| serde_json::json!({
            "dimensions": group(&r.dimensions), "value": crate::wire::encode(&r.value), "gaps": r.gaps,
            "time_basis": "UTC calendar",
        })).collect::<Vec<_>>(),
        "flags": page.flags.iter().map(|f| serde_json::json!({
            "flag": f.flag, "group": group(&f.group), "unknown": f.unknown,
        })).collect::<Vec<_>>(),
        "history_complete": page.history_complete,
        "refused": page.refused,
    });
    if let (Some(reason), Some(o)) = (&page.reason, out.as_object_mut()) {
        o.insert(
            "reason".to_string(),
            serde_json::Value::String(reason.clone()),
        );
    }
    out
}

/// What a reader's filter that a row cannot answer is told by, so the read
/// refuses it as malformed rather than not run (ADR-0196 decision 7).
const THEIR_FILTER: &str = "a metric's filter: ";

/// A metric read refused for a filter a row cannot answer, its reason at
/// most a short sentence, never what the row held.
fn unanswerable_filter(why: &str) -> Page {
    let why: String = why
        .trim_start_matches(THEIR_FILTER)
        .chars()
        .take(160)
        .collect();
    Page {
        refused: Some("malformed_filter"),
        reason: Some(format!("a row of the metric cannot answer it: {why}")),
        history_complete: true,
        ..Page::default()
    }
}

/// A reader's read of a metric (`library-api.md` §5, `MetricPage`): every
/// group on one page, whose cursor is therefore none.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Page {
    pub rows: Vec<PageRow>,
    pub flags: Vec<RaisedFlag>,
    pub history_complete: bool,
    /// Why the read cannot be answered, with no rows: `unknown_metric`,
    /// `unknown_dimension`, `filter_on_combined`, `no_window` or `personal`
    /// (ADR-0148).
    pub refused: Option<&'static str>,
    /// What a `malformed_filter` refusal found, a sentence (ADR-0191
    /// decision 4).
    pub reason: Option<String>,
}

/// A set some dimension's path goes through, by the prefix reaching it: its
/// members, and each dimension going through it with the rest of its path.
struct Through {
    prefix: Expr,
    members: Vec<Value>,
    via: Vec<(usize, Vec<String>)>,
}

/// One group's exact value and the objects whose gaps it carries.
type Groups = Vec<(Vec<Value>, Num, BTreeSet<String>)>;

impl<'w> Reader<'w> {
    /// A metric's exact value on each group of the dimensions kept, in the
    /// order the groups first appear, with each group's gaps, and its
    /// expression's type. A combined metric reads each input to the
    /// dimensions it has of those kept and joins over every group an input
    /// with all of them has, an input missing a group contributing its value
    /// over no rows (ADR-0122 decision 18, ADR-0124, ADR-0147).
    #[allow(clippy::too_many_arguments)]
    fn grouped(
        &self,
        owner: Option<&str>,
        spec: &Json,
        module: &str,
        bound: &Bound,
        keep: &[String],
        window: Option<i64>,
        theirs: Option<&Expr>,
    ) -> Result<(Groups, Kind), NotRun> {
        let lib = self.world.lib;
        if let Some(inputs) = spec.get("input_metrics").and_then(Json::as_object) {
            let mut read = Vec::new();
            let mut kinds = BTreeMap::new();
            for (n, reference) in inputs {
                let input = find(
                    lib,
                    &parsed(reference.as_str().unwrap_or_default())?,
                    None,
                    Some(module),
                )?;
                let mine = dimensions(lib, input.owner.as_deref(), &input.spec)?;
                let kept: Vec<String> = keep.iter().filter(|d| mine.contains(d)).cloned().collect();
                let narrowed: Bound = bound
                    .iter()
                    .filter(|(d, _)| mine.contains(d))
                    .map(|(d, v)| (d.clone(), v.clone()))
                    .collect();
                let (groups, kind) = self.grouped(
                    input.owner.as_deref(),
                    &input.spec,
                    &input.module,
                    &narrowed,
                    &kept,
                    window,
                    None,
                )?;
                let empty =
                    self.over_no_rows(input.owner.as_deref(), &input.spec, &input.module)?;
                kinds.insert(n.clone(), kind);
                read.push((n.clone(), kept, groups, empty));
            }
            let mut keys: Vec<Vec<Value>> = Vec::new();
            for (_, kept, groups, _) in &read {
                if kept.len() == keep.len() {
                    for (k, _, _) in groups {
                        if !keys.contains(k) {
                            keys.push(k.clone());
                        }
                    }
                }
            }
            let tree = parsed(
                spec.get("expression")
                    .and_then(Json::as_str)
                    .unwrap_or_default(),
            )?;
            let mut out = Vec::new();
            for key in keys {
                let mut values = BTreeMap::new();
                let mut gaps = BTreeSet::new();
                for (n, kept, groups, empty) in &read {
                    let sub: Vec<Value> = keep
                        .iter()
                        .zip(&key)
                        .filter(|(d, _)| kept.contains(d))
                        .map(|(_, v)| v.clone())
                        .collect();
                    match groups.iter().find(|(k, _, _)| *k == sub) {
                        Some((_, v, g)) => {
                            values.insert(n.clone(), *v);
                            gaps.extend(g.iter().cloned());
                        }
                        None => {
                            values.insert(n.clone(), *empty);
                        }
                    }
                }
                out.push((key, self.combined(&tree, &values)?, gaps));
            }
            return Ok((out, self.kind(&tree, &kinds, None)?));
        }
        let owner = owner.unwrap_or_default();
        let sel = self.selected(owner, spec, bound, window, theirs)?;
        let exprs: Vec<&Expr> = keep.iter().map(|d| &sel.dims[d]).collect();
        let mut members: Vec<(Vec<Value>, Vec<Value>)> = Vec::new();
        for r in &sel.picked {
            let env = BTreeMap::from([(sel.item.clone(), r.clone())]);
            for key in self.combinations(&sel.evaluator, &exprs, &env)? {
                match members.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, rows)) => rows.push(r.clone()),
                    None => members.push((key, vec![r.clone()])),
                }
            }
        }
        // the whole, read with nothing kept, is one row even over no rows
        if keep.is_empty() && members.is_empty() {
            members.push((Vec::new(), Vec::new()));
        }
        let window_start = window.map(|w| self.world.now - w);
        let mut out = Vec::new();
        for (key, rows) in members {
            let value = self.over_rows(&sel.evaluator, &sel.tree, &rows, &sel.item)?;
            let gaps = rows
                .iter()
                .map(|r| self.gap(r, &sel, window_start))
                .collect::<Result<Vec<_>, NotRun>>()?
                .into_iter()
                .flatten()
                .collect();
            out.push((key, value, gaps));
        }
        let env = BTreeMap::from([(sel.item.clone(), sel.rowkind.clone())]);
        Ok((
            out,
            self.kind(&sel.tree, &env, self.world.lib.type_(owner))?,
        ))
    }

    /// The object a row read from before what its record vouches for, if
    /// any: a span or an event earlier than its object's `.recorded_from`, a
    /// window beginning before it on an object that existed then, an undated
    /// creation the formula reads, or an object's own flow data that began
    /// before it (`declaration-syntax.md` §6.9, ADR-0106, ADR-0146).
    fn gap(
        &self,
        r: &Value,
        sel: &Selected<'w>,
        window_start: Option<i64>,
    ) -> Result<Option<String>, NotRun> {
        let (id, since) = match r {
            Value::Object(id) => (id.clone(), None),
            Value::Row(f) => match (
                f.get("object"),
                f.get("entered_at").or_else(|| f.get("occurred_at")),
            ) {
                (Some(Value::Object(id)), Some(Value::Timestamp(t))) => (id.clone(), Some(*t)),
                (Some(Value::Object(id)), _) => (id.clone(), None),
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        let Some(o) = self.world.get(&id)? else {
            return Ok(None);
        };
        let before = |t: i64| t < o.recorded_from;
        let flow_before = matches!(r, Value::Object(_))
            && sel.reads_flow
            && self
                .world
                .parts(o)?
                .spans
                .values()
                .flatten()
                .any(|sp| before(sp.entered_at));
        let gapped = since.is_some_and(before)
            || window_start.is_some_and(|w| before(w) && o.created_at < w)
            || (sel.reads_created && !o.created_dated)
            || flow_before;
        Ok(gapped.then_some(id))
    }

    /// Each combination of the kept dimensions' values a row is counted
    /// under: one value where a path reaches no set, each distinct value
    /// where it goes through one, and, where two paths go through one set,
    /// the values of each of its members, never crossed; an empty set, or a
    /// member whose path reaches no value, counts the row under the absent
    /// value (ADR-0123, ADR-0146).
    fn combinations(
        &self,
        ev: &Evaluator<'w>,
        exprs: &[&Expr],
        env: &BTreeMap<String, Value>,
    ) -> Result<Vec<Vec<Value>>, NotRun> {
        let of = |b: &Value, member: &str| -> Result<Value, NotRun> {
            match b {
                Value::Absent => return Ok(Value::Absent),
                // a row's member read as the evaluator reads it, without
                // binding the row in a copy of the environment
                Value::Row(fields) => return crate::eval::row_member(fields, member),
                _ => {}
            }
            let mut inner = env.clone();
            inner.insert("\u{0}base".into(), b.clone());
            ev.eval_in(
                &Expr::Path(Box::new(Expr::Name("\u{0}base".into())), member.to_string()),
                &inner,
            )
        };
        let mut fixed: BTreeMap<usize, Value> = BTreeMap::new();
        let mut shared: Vec<Through> = Vec::new();
        for (i, e) in exprs.iter().enumerate() {
            let mut chain = Vec::new();
            let mut node = *e;
            while let Expr::Path(base, member) = node {
                chain.push(member.clone());
                node = base;
            }
            chain.reverse();
            let mut v = ev.eval_in(node, env)?;
            let mut prefix = node.clone();
            let mut j = 0;
            while !matches!(v, Value::Set(_)) && j < chain.len() {
                v = of(&v, &chain[j])?;
                prefix = Expr::Path(Box::new(prefix), chain[j].clone());
                j += 1;
            }
            match v {
                Value::Set(items) => match shared.iter_mut().find(|t| t.prefix == prefix) {
                    Some(t) => t.via.push((i, chain[j..].to_vec())),
                    None => shared.push(Through {
                        prefix,
                        members: items,
                        via: vec![(i, chain[j..].to_vec())],
                    }),
                },
                v => {
                    fixed.insert(i, v);
                }
            }
        }
        let mut keys: Vec<BTreeMap<usize, Value>> = vec![fixed];
        for Through { members, via, .. } in shared {
            let mut tuples: Vec<Vec<Value>> = Vec::new();
            for m in &members {
                let mut vals = Vec::new();
                for (_, rest) in &via {
                    let mut x = m.clone();
                    for name in rest {
                        x = of(&x, name)?;
                    }
                    vals.push(x);
                }
                if !tuples.contains(&vals) {
                    tuples.push(vals);
                }
            }
            if tuples.is_empty() {
                tuples.push(vec![Value::Absent; via.len()]);
            }
            let mut next = Vec::new();
            for k in &keys {
                for t in &tuples {
                    let mut k = k.clone();
                    for (n, (i, _)) in via.iter().enumerate() {
                        k.insert(*i, t[n].clone());
                    }
                    next.push(k);
                }
            }
            keys = next;
        }
        Ok(keys
            .into_iter()
            .map(|k| {
                (0..exprs.len())
                    .map(|i| k.get(&i).cloned().unwrap_or(Value::Absent))
                    .collect()
            })
            .collect())
    }

    /// The value a metric's expression has over no rows (ADR-0122 decision
    /// 18).
    fn over_no_rows(&self, owner: Option<&str>, spec: &Json, module: &str) -> Result<Num, NotRun> {
        let lib = self.world.lib;
        if let Some(inputs) = spec.get("input_metrics").and_then(Json::as_object) {
            let mut values = BTreeMap::new();
            for (n, reference) in inputs {
                let input = find(
                    lib,
                    &parsed(reference.as_str().unwrap_or_default())?,
                    None,
                    Some(module),
                )?;
                values.insert(
                    n.clone(),
                    self.over_no_rows(input.owner.as_deref(), &input.spec, &input.module)?,
                );
            }
            let tree = parsed(
                spec.get("expression")
                    .and_then(Json::as_str)
                    .unwrap_or_default(),
            )?;
            return self.combined(&tree, &values);
        }
        let owner = owner.unwrap_or_default();
        let f = formula(lib, owner, spec)?;
        let tree = parsed(
            f.get("expression")
                .and_then(Json::as_str)
                .unwrap_or_default(),
        )?;
        let item = f.get("item").and_then(Json::as_str).unwrap_or_default();
        self.over_rows(&self.row_eval(lib.type_(owner)), &tree, &[], item)
    }

    /// Whether a read may window a metric: one with a time dimension or a
    /// split, or a combined one whose every input may be (ADR-0147).
    fn windowed(&self, owner: Option<&str>, spec: &Json, module: &str) -> Result<bool, NotRun> {
        let lib = self.world.lib;
        if let Some(inputs) = spec.get("input_metrics").and_then(Json::as_object) {
            for reference in inputs.values() {
                let input = find(
                    lib,
                    &parsed(reference.as_str().unwrap_or_default())?,
                    None,
                    Some(module),
                )?;
                if !self.windowed(input.owner.as_deref(), &input.spec, &input.module)? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        let f = formula(lib, owner.unwrap_or_default(), spec)?;
        Ok(f.get("time_dimension").is_some() || f.get("split_by").is_some())
    }

    /// Whether a reader's filter reads a personal attribute of an object it
    /// reaches from the row (§6.9, ADR-0148).
    fn reads_personal(&self, owner: &str, spec: &Json, filter: &Expr) -> Result<bool, NotRun> {
        let lib = self.world.lib;
        let f = formula(lib, owner, spec)?;
        let source = f.get("source").and_then(Json::as_str).unwrap_or_default();
        let item = f
            .get("item")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        let t = self.type_of(owner)?;
        let rowkind = if source == "objects" {
            Kind::Object(owner.to_string())
        } else if source.starts_with("intervals") {
            Kind::Interval(owner.to_string())
        } else if source == "transitions" {
            Kind::Transition(owner.to_string())
        } else if let Some((_, kind)) = t.observations.iter().find(|(c, _)| c == source) {
            Kind::Object(kind.clone())
        } else if source == "labels" {
            Kind::Object(crate::model::LABEL.into())
        } else {
            Kind::Other
        };
        let env = BTreeMap::from([(item, rowkind)]);
        // a member of the path's type marked personal, or a derived one
        // reading what is, as a consulted reference is masked, where a filter
        // on `d.owner_name >= "B"` was answered (sixteenth production review
        // N6, ADR-0203 decision 2)
        let personal = crate::eval::personal_names(lib);
        // and a member of type identity, where an actor type marks its
        // identity personal (ADR-0206 decision 1)
        let identities = crate::eval::identities_personal(lib);
        fn walk(
            r: &Reader<'_>,
            e: &Expr,
            env: &BTreeMap<String, Kind>,
            personal: &BTreeSet<String>,
            identities: bool,
        ) -> Result<bool, NotRun> {
            if let Expr::Path(base, m) = e
                && let Kind::Object(tn) = r.kind(base, env, None)?
                && let Some(t) = r.world.lib.type_(&tn)
                && (t.attribute(m).is_some_and(|a| a.personal)
                    || (t.derived.iter().any(|(d, _)| d == m) && personal.contains(m))
                    || (identities
                        && t.attribute(m).is_some_and(
                            |a| matches!(&a.declared, Declared::Scalar(s) if s == "identity"),
                        )))
            {
                return Ok(true);
            }
            for c in crate::check::children_of(e) {
                if walk(r, c, env, personal, identities)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        walk(self, filter, &env, &personal, identities)
    }

    /// A page's order between two keys: by each kept dimension ascending, an
    /// absent value last, a state, an enumeration member or a kind of actor
    /// in declaration order, an object by id, which is creation order, a time
    /// bucket in time order and a string by code point (ADR-0127, ADR-0147).
    /// An object's place is read before the sort, in `places`, since a
    /// comparison cannot stop the read.
    fn order(
        &self,
        places: &BTreeMap<String, i64>,
        keep: &[String],
        a: &[Value],
        b: &[Value],
    ) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        const KINDS: [&str; 4] = ["human", "agent", "service", "unknown"];
        let lib = self.world.lib;
        let rank = |d: &str, v: &Value| -> (u8, Option<i128>, Option<Rat>, String) {
            match v {
                Value::Absent => (1, None, None, String::new()),
                // in the installed version's order, and one it no longer
                // declares, from before a publish, after those it does, by
                // name (D536)
                Value::State { type_, name } => {
                    let states = lib
                        .type_(type_)
                        .map(|t| t.states.as_slice())
                        .unwrap_or_default();
                    let at = states.iter().position(|(s, _)| s == name);
                    declared_rank(at, states.len(), name)
                }
                Value::Member {
                    enumeration,
                    member,
                } => {
                    let members = lib
                        .enumerations
                        .get(enumeration)
                        .map(Vec::as_slice)
                        .unwrap_or_default();
                    let at = members.iter().position(|m| m == member);
                    declared_rank(at, members.len(), member)
                }
                Value::Text(t) if d == "actor_kind" && KINDS.contains(&t.as_str()) => (
                    0,
                    KINDS
                        .iter()
                        .position(|k| k == t)
                        .and_then(|i| i128::try_from(i).ok()),
                    None,
                    String::new(),
                ),
                Value::Object(id) => (0, places.get(id).map(|n| i128::from(*n)), None, id.clone()),
                Value::Text(t) => (0, None, None, t.clone()),
                Value::Bool(b) => (0, Some(i128::from(*b)), None, String::new()),
                other => (
                    0,
                    None,
                    Num::of(other).ok().and_then(Num::rat),
                    String::new(),
                ),
            }
        };
        for ((d, x), y) in keep.iter().zip(a).zip(b) {
            let o = rank(d, x).cmp(&rank(d, y));
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    }

    /// The place in creation order of each object the keys given name, which
    /// [`Reader::order`] ranks objects by.
    fn places<'k>(
        &self,
        keys: impl Iterator<Item = &'k [Value]>,
    ) -> Result<BTreeMap<String, i64>, NotRun> {
        let mut places = BTreeMap::new();
        for key in keys {
            for v in key {
                if let Value::Object(id) = v
                    && !places.contains_key(id)
                    && let Some(o) = self.world.get(id)?
                {
                    places.insert(id.clone(), o.order);
                }
            }
        }
        Ok(places)
    }
}

/// A reader's read of a metric (`library-api.md` §5, `declaration-syntax.md`
/// §6.9): its rows those a guard's read would select, narrowed by a filter
/// over its binder, grouped by the dimensions kept, every declared one unless
/// `keep` names them, the rest aggregated over, a row per group in their
/// order; each declared flag on each declared group; and a refusal, a value,
/// where the read cannot be answered (ADR-0098, ADR-0101, ADR-0114, ADR-0124,
/// ADR-0127, ADR-0146, ADR-0147, ADR-0148). A bound value is the wire's
/// (ADR-0150), and so is a filter's text.
pub fn page(
    world: &World<'_>,
    answers: Option<&BTreeMap<String, String>>,
    name: &str,
    bind: &BTreeMap<String, Json>,
    keep: Option<&[String]>,
    over: Option<i64>,
    filter: Option<&str>,
) -> Result<Page, NotRun> {
    page_as(world, answers, name, bind, keep, over, filter, false)
}

/// A page, its request opting into reading by a personal value or not: a
/// filter reading one, or a binding of a dimension that names a person, is
/// refused `personal` without the opt-in, whoever asks (ADR-0206 decision
/// 2), where a binding of an author's address was answered to an agent
/// (the audit's personal data R7).
#[allow(clippy::too_many_arguments)]
pub fn page_as(
    world: &World<'_>,
    answers: Option<&BTreeMap<String, String>>,
    name: &str,
    bind: &BTreeMap<String, Json>,
    keep: Option<&[String]>,
    over: Option<i64>,
    filter: Option<&str>,
    reads_personal: bool,
) -> Result<Page, NotRun> {
    let refused = |why: &'static str| {
        Ok(Page {
            refused: Some(why),
            history_complete: true,
            ..Page::default()
        })
    };
    let reader = Reader {
        world,
        answers,
        typing: Default::default(),
    };
    let lib = world.lib;
    let Ok(reference) = parse(name) else {
        return refused("unknown_metric");
    };
    let Ok(found) = find(lib, &reference, None, None) else {
        return refused("unknown_metric");
    };
    let owner = found.owner.as_deref();
    let combined = found.spec.get("input_metrics").is_some();
    let declared: Vec<String> = if combined {
        found
            .spec
            .get("group_by")
            .and_then(Json::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Json::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    } else {
        formula(lib, owner.unwrap_or_default(), &found.spec)?
            .get("dimensions")
            .and_then(Json::as_object)
            .map(|d| d.keys().cloned().collect())
            .unwrap_or_default()
    };
    let keep: Vec<String> = keep.map_or_else(|| declared.clone(), <[String]>::to_vec);
    let dims = dimensions(lib, owner, &found.spec)?;
    if keep.iter().chain(bind.keys()).any(|d| !dims.contains(d)) {
        return refused("unknown_dimension");
    }
    if filter.is_some() && combined {
        return refused("filter_on_combined");
    }
    if over.is_some() && !reader.windowed(owner, &found.spec, &found.module)? {
        return refused("no_window");
    }
    // a filter reads the rows through the metric's item: a name it does not
    // bind is a filter the read cannot apply, refused rather than answered
    // with no rows (D569)
    let theirs = match filter.map(parse).transpose() {
        Ok(t) => t,
        Err(e) => {
            return Ok(Page {
                refused: Some("malformed_filter"),
                reason: Some(format!("it does not parse: {e}")),
                history_complete: true,
                ..Page::default()
            });
        }
    };
    // and takes a query filter's bounds, where a set of 5,000 members ran to
    // the read's deadline, holding a pool's slot (the audit's cost B6,
    // ADR-0205 decision 15)
    if let Some(why) = theirs.as_ref().and_then(crate::reads::filter_cost) {
        return Ok(Page {
            refused: Some("malformed_filter"),
            reason: Some(why),
            history_complete: true,
            ..Page::default()
        });
    }
    // and a literal past what its type holds, as a query's is, where an int
    // or a duration past 64 bits was refused `value_in_range` once read
    if let Some(f) = &theirs
        && crate::reads::literal_out_of_range(world, None, f)?
    {
        return Ok(Page {
            refused: Some("malformed_filter"),
            reason: Some("it holds a literal past what its type holds".to_string()),
            history_complete: true,
            ..Page::default()
        });
    }
    if let Some(f) = &theirs {
        let item = formula(lib, owner.unwrap_or_default(), &found.spec)?
            .get("item")
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_string();
        if !reads_only_its_rows(lib, &item, f) {
            return Ok(Page {
                refused: Some("malformed_filter"),
                reason: Some(format!(
                    "a metric's filter reads its rows through the metric's item, {item}, and this one names something else"
                )),
                history_complete: true,
                ..Page::default()
            });
        }
    }
    if let Some(f) = &theirs
        && !reads_personal
        && reader.reads_personal(owner.unwrap_or_default(), &found.spec, f)?
    {
        return refused("personal");
    }
    if !reads_personal
        && !bind.is_empty()
        && let Some(o) = owner
    {
        let marked = person_dimensions(lib, o)?;
        if bind.keys().any(|d| marked.contains(&format!("{name}.{d}"))) {
            return refused("personal");
        }
    }
    let bound: Bound = bind
        .iter()
        .map(|(d, v)| (d.clone(), Want::Wire(v.clone())))
        .collect();
    let (mut groups, kind) = match reader.grouped(
        owner,
        &found.spec,
        &found.module,
        &bound,
        &keep,
        over,
        theirs.as_ref(),
    ) {
        Err(NotRun::Unsupported(why)) if why.starts_with(THEIR_FILTER) => {
            return Ok(unanswerable_filter(&why));
        }
        other => other?,
    };
    let places = reader.places(groups.iter().map(|g| g.0.as_slice()))?;
    groups.sort_by(|a, b| reader.order(&places, &keep, &a.0, &b.0));
    let mut rows = Vec::new();
    for (key, value, gaps) in groups {
        rows.push(PageRow {
            dimensions: keep.iter().cloned().zip(key).collect(),
            value: rounded(value, &kind)?,
            gaps: gaps.into_iter().collect(),
        });
    }
    let mut flags = Vec::new();
    if let Some(when) = found.spec.get("flag_when").and_then(Json::as_object) {
        // a flag is raised on each declared group, whatever the read keeps,
        // over the rows the read selects (§6.9)
        let (mut every, _) = match reader.grouped(
            owner,
            &found.spec,
            &found.module,
            &bound,
            &declared,
            over,
            theirs.as_ref(),
        ) {
            Err(NotRun::Unsupported(why)) if why.starts_with(THEIR_FILTER) => {
                return Ok(unanswerable_filter(&why));
            }
            other => other?,
        };
        let places = reader.places(every.iter().map(|g| g.0.as_slice()))?;
        every.sort_by(|a, b| reader.order(&places, &declared, &a.0, &b.0));
        let t = owner.and_then(|o| lib.type_(o));
        for (key, value, _) in every {
            let mut env: BTreeMap<String, Value> =
                declared.iter().cloned().zip(key.iter().cloned()).collect();
            env.insert("value".into(), rounded(value, &kind)?);
            for (flag, text) in when {
                let condition = parsed(text.as_str().unwrap_or_default())?;
                let holds = truth(&reader.row_eval(t).eval_in(&condition, &env)?)?;
                if holds != Some(false) {
                    flags.push(RaisedFlag {
                        flag: flag.clone(),
                        group: declared.iter().cloned().zip(key.iter().cloned()).collect(),
                        unknown: holds.is_none(),
                    });
                }
            }
        }
    }
    Ok(Page {
        history_complete: rows.iter().all(|r| r.gaps.is_empty()),
        rows,
        flags,
        refused: None,
        reason: None,
    })
}

/// The standard metrics over a type, as `declaration-syntax.md` §6.11's
/// first table gives them.
pub(crate) const STANDARD_NAMES: [&str; 11] = [
    "time_in_state",
    "time_in_state_p80",
    "throughput",
    "work_in_progress",
    "oldest_open",
    "transition_counts",
    "refusals",
    "flags_raised",
    "refusal_rate",
    "override_counts",
    "rework",
];

/// The standard assignment metrics, each over a type and one of its
/// assignee references, as §6.11's second table gives them.
const ASSIGNMENT_NAMES: [&str; 11] = [
    "open_work",
    "time_unassigned",
    "time_to_first_assignment",
    "time_with_assignee",
    "cycle_time_by_assignee",
    "time_in_state_by_holder",
    "handoffs",
    "reassigned_back",
    "acted_by_non_assignee",
    "handoffs_by_object",
    "returns_by_object",
];

/// Each metric dimension of a type whose value names an actor, which
/// `declaration` marks so an upper layer can keep a person's figures behind
/// its own door (`library-api.md` §6, ADR-0114, D451), as
/// `<Type>.<metric>.<dimension>` in ascending order: over the type's own
/// metrics and the standard ones, an assignment metric once for each
/// assignee reference. A value names an actor where it is an object of a
/// type marking an `actor` identity, a tracked reference's value in an
/// interval of it included, or an actor's id; a combined metric's
/// dimensions are its inputs', marked there. Its rows are never read: a
/// row's kind is its source's.
pub fn person_dimensions(lib: &Library, tn: &str) -> Result<Vec<String>, NotRun> {
    let Some(t) = lib.type_(tn) else {
        return Ok(Vec::new());
    };
    let world = World::new(lib, 0);
    let r = Reader {
        world: &world,
        answers: None,
        typing: Default::default(),
    };
    // the type's own metrics, declared under it in its module (flow-format.md §4.9)
    let mut names: Vec<String> = lib
        .metrics
        .values()
        .flat_map(|ms| ms.iter())
        .filter(|(_, (owner, _))| owner.as_deref() == Some(tn))
        .map(|(n, _)| n.clone())
        .collect();
    names.extend(STANDARD_NAMES.iter().map(|n| (*n).to_string()));
    for (a, _) in t.attributes.iter().filter(|(_, a)| a.assignee) {
        names.extend(ASSIGNMENT_NAMES.iter().map(|n| format!("{n}.{a}")));
    }
    let mut out = Vec::new();
    for n in names {
        let found = find(lib, &parsed(&format!("{tn}.{n}"))?, Some(tn), None)?;
        if found.spec.get("input_metrics").is_some() {
            continue;
        }
        let owner = found.owner.clone().unwrap_or_else(|| tn.to_string());
        let f = formula(lib, &owner, &found.spec)?;
        let text = |k: &str| {
            f.get(k)
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let (source, item) = (text("source"), text("item"));
        let env = BTreeMap::from([(item.clone(), r.row_kind(&owner, &source)?)]);
        let Some(dims) = f.get("dimensions").and_then(Json::as_object) else {
            continue;
        };
        for (d, x) in dims {
            let e = parsed(x.as_str().unwrap_or_default())?;
            // an actor's id, a transition's or an attempt's
            if matches!(&e, Expr::Path(_, m) if m == "actor_id") {
                out.push(format!("{tn}.{n}.{d}"));
                continue;
            }
            let member = source
                .strip_prefix("intervals(")
                .and_then(|m| m.strip_suffix(')'))
                .map(str::trim);
            let kind = match (member, &e) {
                // a tracked reference's value in an interval of it
                (Some(m), Expr::Path(base, v))
                    if v == "value" && matches!(&**base, Expr::Name(b) if *b == item) =>
                {
                    match lib
                        .type_(&owner)
                        .and_then(|t| t.attribute(m))
                        .map(|a| &a.declared)
                    {
                        Some(Declared::Reference(target)) => Kind::Object(target.clone()),
                        _ => Kind::Other,
                    }
                }
                _ => r.kind(&e, &env, lib.type_(&owner))?,
            };
            if let Kind::Object(k) = kind
                && lib.type_(&k).and_then(Type::actor_identity).is_some()
            {
                out.push(format!("{tn}.{n}.{d}"));
            } else if let Some(row) = row_type(lib, &owner, &source)
                && identity_path(lib, &row, &item, &e)
            {
                // an actor's identity, as `actor.id` records it (D565), on
                // an object of the type or an observation of one of its
                // kinds, where an observation's was left unmarked (the
                // audit's personal data R8)
                out.push(format!("{tn}.{n}.{d}"));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Whether a metric's dimension reads an `identity` attribute of its row's
/// object, or of one its single references reach: an actor's identity, as
/// `assign … actor.id` records who acted (D565).
/// The type a metric's rows are objects of: the owner's for `objects`, an
/// observation kind's for a source naming one of the owner's kinds; none
/// for rows that are no objects, spans, events or attempts.
fn row_type(lib: &Library, owner: &str, source: &str) -> Option<String> {
    if source == "objects" {
        return Some(owner.to_string());
    }
    lib.type_(owner)?
        .observations
        .iter()
        .find(|(c, _)| c == source)
        .map(|(_, kind)| kind.clone())
}

fn identity_path(lib: &Library, owner: &str, item: &str, e: &Expr) -> bool {
    fn type_of(lib: &Library, owner: &str, item: &str, x: &Expr) -> Option<String> {
        match x {
            Expr::Name(n) if n == item => Some(owner.to_string()),
            Expr::Path(base, m) => {
                let t = type_of(lib, owner, item, base)?;
                let a = lib.type_(&t)?.attribute(m)?;
                match &a.declared {
                    Declared::Reference(target) if !a.many => Some(target.clone()),
                    _ => None,
                }
            }
            _ => None,
        }
    }
    let Expr::Path(base, m) = e else { return false };
    type_of(lib, owner, item, base)
        .and_then(|t| {
            lib.type_(&t)
                .and_then(|t| t.attribute(m))
                .map(|a| a.declared.clone())
        })
        .is_some_and(|d| d == Declared::Scalar("identity".to_string()))
}

/// A declared value's place in a page's order: its position in the installed
/// version's declaration, and one the version no longer declares after them
/// all, by name (D536).
fn declared_rank(
    at: Option<usize>,
    declared: usize,
    name: &str,
) -> (u8, Option<i128>, Option<Rat>, String) {
    match at {
        Some(i) => (0, i128::try_from(i).ok(), None, String::new()),
        None => (0, i128::try_from(declared).ok(), None, name.to_string()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Agents, queues and tickets: a ticket's metrics by the actor who made a
    /// request and by its queue, and its owner an assignee.
    const DESK: &str = r#"module: desk
categories: [live, closed]
types:
  Agent:
    description: A person who works tickets.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human }
    states:
      ACTIVE: { category: live }
      LEFT: { category: closed, final: true }
    transitions:
      join: { kind: initial, to: ACTIVE, required_inputs: [login] }
      leave: { kind: external, from: [ACTIVE], to: LEFT }
  Queue:
    description: A queue tickets are raised in.
    tracking: record
    attributes:
      name: { type: string }
    states:
      OPEN: { category: live }
      SHUT: { category: closed, final: true }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [name] }
      shut: { kind: external, from: [OPEN], to: SHUT }
  Ticket:
    description: A ticket raised in a queue, owned by an agent.
    tracking: record
    attributes:
      queue: { reference: Queue }
      owner: { reference: Agent, optional: true, assignee: true }
    states:
      NEW: { category: live }
      DONE: { category: closed, final: true }
    transitions:
      raise: { kind: initial, to: NEW, required_inputs: [queue], optional_inputs: [owner] }
      resolve: { kind: external, from: [NEW], to: DONE }
    metrics:
      by_actor: { description: Requests by who made them., source: transitions, item: t, dimensions: { actor: t.actor_id }, expression: count() }
      by_queue: { description: Tickets by queue., source: objects, item: o, dimensions: { queue: o.queue }, expression: count() }
"#;

    /// An identity copied into an observation, as `actor.id` records who
    /// signed off, is marked when a metric over the observations groups by
    /// it, as one on the type's own objects is, where it was left unmarked
    /// for the gateway (the audit's personal data R8).
    #[test]
    fn an_identity_an_observation_holds_is_marked() {
        let memos = r#"
module: memos
categories: [live, closed]
types:
  User:
    description: A person.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states:
      ACTIVE: { category: live }
      GONE: { category: closed, final: true }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [login] }
      leave: { kind: external, from: [ACTIVE], to: GONE }
  Memo:
    description: A memo.
    tracking: record
    attributes:
      title: { type: string }
    observations:
      signoffs:
        kind: Signoff
        description: Who signed the memo off.
        attributes:
          who: { type: identity }
    states:
      OPEN: { category: live }
      FILED: { category: closed, final: true }
    transitions:
      write: { kind: initial, to: OPEN, required_inputs: [title] }
      file: { kind: external, from: [OPEN], to: FILED }
    metrics:
      signoffs_by_who: { description: Sign-offs by who gave them., source: signoffs, item: s, dimensions: { who: s.who }, expression: count() }
"#;
        let lib = Library::load(&[("memos.yaml".into(), memos.into())]).unwrap();
        assert!(
            person_dimensions(&lib, "Memo")
                .unwrap()
                .contains(&"Memo.signoffs_by_who.who".to_string()),
            "{:?}",
            person_dimensions(&lib, "Memo").unwrap()
        );
    }

    /// `declaration`'s marks (`library-api.md` §6, D451): an actor's id and
    /// every standard assignment metric's assignee, an object of a type that
    /// marks an actor identity, and not a queue, which marks none.
    #[test]
    fn a_dimension_naming_an_actor_is_marked_and_no_other() {
        let lib = Library::load(&[("desk.yaml".into(), DESK.into())]).unwrap();
        assert_eq!(
            person_dimensions(&lib, "Ticket").unwrap(),
            [
                "Ticket.by_actor.actor",
                "Ticket.cycle_time_by_assignee.owner.assignee",
                "Ticket.open_work.owner.assignee",
                "Ticket.time_in_state_by_holder.owner.assignee",
                "Ticket.time_with_assignee.owner.assignee",
            ]
        );
        assert!(person_dimensions(&lib, "Queue").unwrap().is_empty());
        // each name is one its table answers
        for n in STANDARD_NAMES {
            assert!(standard(n).is_some(), "{n}");
        }
        for n in ASSIGNMENT_NAMES {
            assert!(assignment(n).is_some(), "{n}");
        }
    }
    use crate::model::LABEL;
    use crate::model::OPERATOR;
    use crate::request::{Request, request};
    use crate::store::Store;
    use crate::value::parse_timestamp;
    use crate::verdict::Verdict;

    /// The reference runner's metrics self-test's module (`flowrun.py`,
    /// `metrics_self_test`): jobs, their engineers and their checks.
    const JOBS: &str = r#"
module: jobs
categories: [inbound, live, closed]
enumerations:
  Outcome: [PASS, FAIL, NA]
  Why: [MISKEYED, SUPPLIER]
types:
  User:
    description: A user.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human }
    states:
      ACTIVE: { category: live }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [login] }
  Job:
    description: A job.
    tracking: record
    attributes:
      engineer: { reference: User, optional: true, assignee: true }
      tags: { type: "string[]", optional: true }
      memo: { type: string, optional: true, personal: true }
    observations:
      checks:
        kind: Check
        description: A check of the job.
        attributes:
          outcome: { type: Outcome }
    states:
      OPEN: { category: inbound }
      WORKING: { category: live }
      SHELVED: { category: closed }
      DONE: { category: closed, final: true }
    conditions:
      good: { description: Good work., expression: "metric(pass_rate, engineer := engineer, over last 30 days) >= 0.900", remedy: dependent }
      not_busy: { description: Not busy., expression: "metric(Job.open_work.engineer, assignee := inputs.engineer) < 2", remedy: dependent }
      checked: { description: Checked., expression: count(c in checks) > 0, remedy: self_serviceable }
    transitions:
      open: { kind: initial, to: OPEN, optional_inputs: [engineer, tags] }
      assign: { kind: internal, from: [OPEN, WORKING, SHELVED], required_inputs: [engineer], guards: { not_busy: deny } }
      resume: { kind: external, from: [SHELVED], to: WORKING }
      start: { kind: external, from: [OPEN], to: WORKING }
      take: { kind: external, from: [OPEN], to: WORKING, required_inputs: [engineer] }
      back: { kind: external, from: [WORKING], to: OPEN }
      pause: { kind: external, from: [WORKING], to: OPEN }
      shelve: { kind: external, from: [WORKING], to: SHELVED }
      close: { kind: external, from: [SHELVED], to: DONE }
      finish: { kind: external, from: [WORKING], to: DONE, guards: { good: deny }, proposable: true }
      review: { kind: internal, from: [WORKING], guards: { checked: warn } }
      inspect: { kind: internal, from: [WORKING], guards: { checked: audit } }
      override: { kind: assertion, to: [OPEN, WORKING], inputs: { reason: { type: Why } } }
    metrics:
      pass_rate: { description: x, source: checks, item: r, filter: r.outcome != Outcome.NA, dimensions: { engineer: r.subject.engineer }, time_dimension: r.occurred_at, expression: count(where r.outcome == Outcome.PASS) * 1.000 / count(), flag_when: { low: "value < 0.900" } }
      pass_share: { description: x, source: checks, item: r, dimensions: { engineer: r.subject.engineer }, expression: count(where r.outcome == Outcome.PASS) * 1.00 / count() }
      passes: { description: x, source: checks, item: r, filter: r.outcome == Outcome.PASS, dimensions: { engineer: r.subject.engineer }, time_dimension: r.occurred_at, expression: count() }
      jobs_opened: { description: x, source: objects, item: o, dimensions: { engineer: o.engineer }, time_dimension: o.created_at, expression: count() }
      time_working: { description: x, measure: median_time_in_state, state: WORKING, group_by: [engineer, month] }
      slow_working: { description: x, source: intervals, item: i, filter: i.state == WORKING, expression: "percentile(0.8, i.duration)" }
      starts: { description: x, measure: transition_count, transition: start, group_by: [actor, month] }
      tagged: { description: x, source: objects, item: o, dimensions: { tag: o.tags }, expression: count() }
      tags_per_job: { description: x, source: objects, item: o, expression: avg(count(t in o.tags)) }
      tags_tripled: { description: x, source: objects, item: o, expression: avg(count(t in o.tags)) * 3 }
      backs: { description: x, measure: transition_count, transition: back }
      time_held: { description: x, source: intervals(engineer), item: i, filter: i.value is not null, expression: sum(i.duration) }
      held_by_month: { description: x, source: intervals(engineer), item: i, split_by: month, filter: i.value is not null, dimensions: { month: i.period }, expression: sum(i.duration) }
      working_by_month: { description: x, source: intervals, item: i, split_by: month, filter: i.state == WORKING, dimensions: { month: i.period }, expression: sum(i.duration) }
      jobs_working_by_month: { description: x, source: intervals, item: i, split_by: month, filter: i.state == WORKING, dimensions: { month: i.period }, expression: count() }
      takes: { description: x, measure: transition_count, transition: take, group_by: [engineer] }
      jobs_total: { description: x, source: objects, item: o, expression: count() }
      engineers_checked: { description: x, source: checks, item: r, expression: count(distinct r.subject.engineer) }
      anas_jobs: { description: x, source: objects, item: o, filter: 'o.engineer.login == "ana"', expression: count() }
      third_of_checks: { description: x, source: checks, item: r, expression: count() / 3 }
      outcome_pairs: { description: x, source: objects, item: o, dimensions: { outcome: o.checks.outcome, again: o.checks.outcome }, expression: count() }
      done_spans: { description: x, source: intervals, item: i, filter: i.state == DONE, dimensions: { engineer: i.held(engineer) }, expression: max(i.duration), flag_when: { long: "value > 1 h" } }
      labelled: { description: x, source: labels, item: l, dimensions: { name: l.name, state: l.subject_state, engineer: l.subject.engineer }, expression: count() }
      labels_total: { description: x, source: labels, item: l, expression: count() }
      risky: { description: x, source: labels, item: l, filter: 'l.name == "late-risk"', expression: count() }
      odd_named: { description: x, source: labels, item: l, filter: 'l.name == "odd"', expression: count() }
metrics:
  passes_of_all: { description: x, input_metrics: { passes: Job.passes, all: Job.jobs_total }, group_by: [engineer], expression: passes * 1.000 / all }
  passes_per_job: { description: x, input_metrics: { passes: Job.passes, jobs: Job.jobs_opened }, group_by: [engineer], expression: passes * 1.000 / jobs }
"#;

    const DAY: i64 = 86_400;

    fn world() -> Store {
        let lib = Library::load(&[("jobs.yaml".into(), JOBS.into())]).unwrap();
        Store::sandbox(lib, parse_timestamp("2026-01-01T00:00:00Z").unwrap()).unwrap()
    }

    /// A request as the operator: the object's id where it applies, its
    /// verdict and clause where it is refused.
    fn req(
        s: &mut Store,
        tn: &str,
        xn: &str,
        oid: Option<&str>,
        inputs: Json,
    ) -> Result<String, (String, String)> {
        let r = Request {
            transition: xn.into(),
            actor: OPERATOR.into(),
            object_id: oid.map(str::to_string),
            type_: oid.is_none().then(|| tn.to_string()),
            inputs: inputs
                .as_object()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
        };
        match request(s, &r).unwrap().unwrap() {
            Verdict::Satisfied { object_id, .. } => Ok(object_id),
            Verdict::Unsatisfied { clause, .. } => Err(("unsatisfied".into(), clause)),
            other => Err((other.kind().into(), String::new())),
        }
    }

    /// A type's attempt counts read its family's attempts by their type's
    /// index, never the whole attempt log, merged by key, and a read past
    /// its deadline stops among them (the audit's cost B5, ADR-0205
    /// decision 15).
    #[test]
    fn attempt_counts_read_by_type_and_stop_at_the_deadline() {
        let text = "\nmodule: gate\ncategories: [live]\ntypes:\n  Gate:\n    attributes:\n      n: { type: int }\n    states:\n      ON: { category: live }\n    conditions:\n      small: { expression: n < 3, remedy: self_serviceable }\n    transitions:\n      make: { kind: initial, to: ON, required_inputs: [n] }\n      pass: { kind: internal, from: [ON], guards: { small: deny } }\n  Other:\n    states:\n      ON: { category: live }\n    conditions:\n      never: { expression: false, remedy: dependent }\n    transitions:\n      make: { kind: initial, to: ON }\n      try: { kind: internal, from: [ON], guards: { never: deny } }\n";
        let lib = Library::load(&[("gate.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let big = req(&mut s, "Gate", "make", None, json!({"n": 5})).unwrap();
        let other = req(&mut s, "Other", "make", None, json!({})).unwrap();
        for _ in 0..30 {
            assert!(req(&mut s, "Gate", "pass", Some(&big), json!({})).is_err());
            assert!(req(&mut s, "Other", "try", Some(&other), json!({})).is_err());
        }
        let before = crate::world::whole_attempt_reads();
        let w = s.snapshot().unwrap();
        let rows = attempt_count_rows(&w, "Gate").unwrap();
        assert_eq!(crate::world::whole_attempt_reads(), before);
        let Some(Value::Row(f)) = rows.first() else {
            panic!("{rows:?}")
        };
        assert_eq!((rows.len(), f.get("count")), (1, Some(&Value::Int(30))));
        let mut w = s.snapshot().unwrap();
        w.deadline = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(1));
        assert!(matches!(
            attempt_count_rows(&w, "Gate"),
            Err(NotRun::TimedOut(_))
        ));
        drop(w);
        // a standard metric over attempt counts reads no object of its owner,
        // and one over transitions the family's events by their index,
        // never the whole log (the audit's cost B3, B4)
        let before = (
            crate::world::whole_type_reads("Gate"),
            crate::world::whole_attempt_reads(),
            crate::world::whole_log_reads(),
        );
        let refusals = page_of(&s, "Gate.refusals", &[], Some(&[]), None, None);
        assert_eq!(refusals.rows.len(), 1, "{:?}", refusals.refused);
        assert_eq!(
            (
                crate::world::whole_type_reads("Gate"),
                crate::world::whole_attempt_reads()
            ),
            (before.0, before.1)
        );
        let throughput = page_of(&s, "Gate.throughput", &[], Some(&[]), None, None);
        assert!(throughput.refused.is_none(), "{:?}", throughput.refused);
        assert_eq!(crate::world::whole_log_reads(), before.2);
    }

    /// A metric's filter takes a query filter's bounds: a set of more than
    /// 1,000 members, aggregates nested three deep, more than 256 terms and
    /// a metric called within are each refused `malformed_filter`, where a
    /// set of 5,000 ran to the read's deadline (the audit's cost B6,
    /// ADR-0205 decision 15), and so is a literal past its type's range.
    #[test]
    fn a_metric_filter_takes_a_query_filters_bounds() {
        let lib = Library::load(&[("jobs.yaml".into(), JOBS.into())]).unwrap();
        let s = Store::sandbox(lib, 0).unwrap();
        let refused = |filter: &str| {
            page_of(&s, "Job.jobs_opened", &[], Some(&[]), None, Some(filter)).refused
        };
        let set = |n: usize| {
            let members: Vec<String> = (0..n).map(|i| format!("\"s{i}\"")).collect();
            format!("o.state in {{{}}}", members.join(", "))
        };
        let terms = |n: usize| {
            let alternatives: Vec<String> = (0..n).map(|i| format!("o.id == \"j{i}\"")).collect();
            alternatives.join(" or ")
        };
        for (filter, malformed) in [
            (set(1_000), false),
            (set(1_001), true),
            (terms(20), false),
            (terms(200), true),
            ("metric(Job.jobs_opened) > 0".to_string(), true),
            // a literal past what its type holds, as a query's, where it was
            // read once a row met it (the values audit's L3, ADR-0202
            // decision 8)
            (
                "o.created_at < now + 9223372036854775808 s".to_string(),
                true,
            ),
            ("o.id != \"j\" or 1 < 9223372036854775808".to_string(), true),
            (
                "o.id != \"j\" or 1 < 9223372036854775807".to_string(),
                false,
            ),
        ] {
            assert_eq!(
                refused(&filter) == Some("malformed_filter"),
                malformed,
                "{}",
                &filter[..filter.len().min(60)]
            );
        }
    }

    fn user(s: &mut Store, login: &str) -> String {
        req(s, "User", "add", None, json!({ "login": login })).unwrap()
    }

    fn job(s: &mut Store, inputs: Json) -> String {
        req(s, "Job", "open", None, inputs).unwrap()
    }

    fn take(s: &mut Store, xn: &str, j: &str) {
        req(s, "Job", xn, Some(j), json!({})).unwrap();
    }

    fn record(s: &mut Store, j: &str, outcomes: &[&str]) {
        for x in outcomes {
            req(
                s,
                "Check",
                "record",
                None,
                json!({ "subject": j, "outcome": x }),
            )
            .unwrap();
        }
    }

    /// An expression's value for a job, as a guard on it reads it, with
    /// names bound.
    fn read(s: &Store, j: &str, text: &str, env: &[(&str, Value)]) -> Value {
        let w = s.snapshot().unwrap();
        let env: BTreeMap<String, Value> = env
            .iter()
            .map(|(n, v)| (n.to_string(), v.clone()))
            .collect();
        Evaluator::new(&w, w.lib.type_("Job"), Some(j))
            .eval_in(&parse(text).unwrap(), &env)
            .unwrap()
    }

    /// A value's number, exact, whatever its type.
    fn num(v: &Value) -> Option<Rat> {
        Num::of(v).ok()?.rat()
    }

    fn rat(n: i128, d: i128) -> Option<Rat> {
        Rat::new(n, d)
    }

    /// A reader's filter on a metric reading a personal value through a
    /// derived attribute is refused `personal`, as one through a reference
    /// is, where `d.owner_name >= "B"` ranged over people's names; one on a
    /// derived attribute reading nothing personal is answered (sixteenth
    /// production review N6, ADR-0203 decision 2).
    #[test]
    fn a_filter_reaching_a_personal_value_through_a_derived_one_is_refused() {
        let text = "\nmodule: desks\ncategories: [live]\ntypes:\n  Person:\n    attributes:\n      name: { type: string, personal: true, optional: true }\n    states:\n      ON: { category: live }\n    transitions:\n      join: { kind: initial, to: ON, optional_inputs: [name] }\n  Desk:\n    attributes:\n      owner: { reference: Person, optional: true }\n    derived_attributes:\n      owner_name: { expression: owner.name }\n      placed: { expression: owner is not null }\n    states:\n      ON: { category: live }\n    transitions:\n      place: { kind: initial, to: ON, optional_inputs: [owner] }\n    metrics:\n      desks:\n        source: objects\n        item: d\n        expression: count()\n";
        let lib = Library::load(&[("desks.yaml".into(), text.into())]).unwrap();
        let s = Store::sandbox(lib, 0).unwrap();
        let w = s.snapshot().unwrap();
        let read = |filter: &str| {
            page(
                &w,
                None,
                "Desk.desks",
                &BTreeMap::new(),
                None,
                None,
                Some(filter),
            )
            .unwrap()
            .refused
        };
        assert_eq!(read(r#"d.owner_name >= "B""#), Some("personal"));
        assert_eq!(read(r#"d.owner.name == "Ann""#), Some("personal"));
        assert_eq!(read("d.placed"), None);
    }

    /// A metric's rows name an actor by its object where an actor type
    /// marks its identity personal, as a metric reads no personal value,
    /// and by its identity where none does, where `requests by who made
    /// them` answered each person's address (the audit's personal data R8).
    #[test]
    fn a_metric_names_an_actor_by_its_object_where_identities_are_personal() {
        for personal in [true, false] {
            let text = format!(
                "\nmodule: desk\ncategories: [live]\ntypes:\n  Person:\n    attributes:\n      login: {{ type: identity, actor_kind: human, personal: {personal}, optional: true }}\n    states:\n      ON: {{ category: live }}\n    transitions:\n      join: {{ kind: initial, to: ON, required_inputs: [login] }}\n  Ticket:\n    attributes:\n      title: {{ type: string }}\n    states:\n      NEW: {{ category: live }}\n    transitions:\n      raise: {{ kind: initial, to: NEW, required_inputs: [title] }}\n    metrics:\n      by_actor: {{ source: transitions, item: t, dimensions: {{ actor: t.actor_id }}, expression: count() }}\n"
            );
            let lib = Library::load(&[("desk.yaml".into(), text)]).unwrap();
            let mut s = Store::sandbox(lib, 0).unwrap();
            let ask = |s: &mut Store, actor: &str, tn: &str, xn: &str, inputs: Json| {
                let r = Request {
                    transition: xn.into(),
                    actor: actor.into(),
                    object_id: None,
                    type_: Some(tn.into()),
                    inputs: inputs
                        .as_object()
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .collect(),
                    occurred_at: None,
                    propose: false,
                    expires_at: None,
                    expected_version: None,
                    target_version: None,
                    principal: None,
                    idempotency_key: None,
                    context: None,
                };
                match request(s, &r).unwrap().unwrap() {
                    Verdict::Satisfied { object_id, .. } => object_id,
                    other => panic!("{other:?}"),
                }
            };
            let ana = ask(
                &mut s,
                OPERATOR,
                "Person",
                "join",
                json!({"login": "ana@corp.example"}),
            );
            ask(
                &mut s,
                "ana@corp.example",
                "Ticket",
                "raise",
                json!({"title": "Broken"}),
            );
            let w = s.snapshot().unwrap();
            let p = page(
                &w,
                None,
                "Ticket.by_actor",
                &BTreeMap::new(),
                None,
                None,
                None,
            )
            .unwrap();
            let named: Vec<String> = p
                .rows
                .iter()
                .filter_map(|r| {
                    r.dimensions.iter().find_map(|(d, v)| match v {
                        Value::Text(t) if d == "actor" => Some(t.clone()),
                        _ => None,
                    })
                })
                .collect();
            let want = if personal {
                ana.clone()
            } else {
                "ana@corp.example".to_string()
            };
            assert_eq!(named, vec![want], "personal: {personal}");
        }
    }

    /// A metric read whose filter or value passes its type's bound refuses
    /// the read as out of range, where the filter dropped the row and the
    /// value was answered not run (the audit's values U5 and U8, ADR-0205
    /// decision 2).
    #[test]
    fn a_metric_past_its_bound_refuses_the_read() {
        let text = "\nmodule: tally\ncategories: [live]\ntypes:\n  Claim:\n    attributes:\n      total: { type: int }\n    states:\n      OPEN: { category: live }\n    transitions:\n      open: { kind: initial, to: OPEN, required_inputs: [total] }\n    metrics:\n      claimed:\n        source: objects\n        item: c\n        expression: sum(c.total)\n      vast:\n        source: objects\n        item: c\n        expression: sum(c.total) * 100000000000000000000000\n";
        let lib = Library::load(&[("tally.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        for total in [i64::MAX, 7] {
            let r = Request {
                transition: "open".into(),
                actor: OPERATOR.into(),
                object_id: None,
                type_: Some("Claim".into()),
                inputs: BTreeMap::from([("total".to_string(), json!(total))]),
                occurred_at: None,
                propose: false,
                expires_at: None,
                expected_version: None,
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            };
            assert!(matches!(
                request(&mut s, &r).unwrap().unwrap(),
                Verdict::Satisfied { .. }
            ));
        }
        let w = s.snapshot().unwrap();
        let read = |filter: Option<&str>| {
            page(
                &w,
                None,
                "Claim.claimed",
                &BTreeMap::new(),
                None,
                None,
                filter,
            )
            .map(|p| p.rows.len())
        };
        assert!(matches!(
            read(Some("c.total * 2 > 0")),
            Err(NotRun::OutOfRange(_))
        ));
        assert!(matches!(
            read(Some("c.total + 1 > 0")),
            Err(NotRun::OutOfRange(_))
        ));
        assert!(matches!(read(None), Err(NotRun::OutOfRange(_))));
        assert_eq!(read(Some("c.total < 10")), Ok(1));
        // past 128 bits within the metric's own arithmetic
        let vast = page(&w, None, "Claim.vast", &BTreeMap::new(), None, None, None);
        assert!(
            matches!(vast, Err(NotRun::OutOfRange(_))),
            "{:?}",
            vast.map(|p| p.rows.len())
        );
    }

    /// A metric over many rows reads as a scan: a type its rows reach, an
    /// order's lines summed for its total, read whole once, a type they never
    /// reach never, where every type a row's type refers to was read and made
    /// a metric reading none three times slower; the scan ends with the
    /// metric, so a later read of a few objects reads them alone. Throughput
    /// reads only the events entering a finished state, where it read every
    /// event of the type (the audit's cost B2 and B3, ADR-0205 decision 13).
    #[test]
    fn a_metric_over_many_rows_reads_what_they_reach_and_no_more() {
        let text = "\nmodule: shop\ncategories: [live, closed]\ntypes:\n  Note:\n    attributes:\n      text: { type: string }\n    states:\n      ON: { category: live }\n    transitions:\n      write: { kind: initial, to: ON, required_inputs: [text] }\n  Order:\n    attributes:\n      note: { reference: Note, optional: true }\n      lines: { reference: \"Line[]\", opposite: order }\n    derived_attributes:\n      total:\n        expression: \"sum(l in lines: l.amount)\"\n    states:\n      OPEN: { category: live }\n      DONE: { category: closed, final: true }\n    transitions:\n      open: { kind: initial, to: OPEN, optional_inputs: [note] }\n      close: { kind: external, from: OPEN, to: DONE }\n    metrics:\n      totals:\n        source: objects\n        item: o\n        expression: sum(o.total)\n      counted:\n        source: objects\n        item: o\n        expression: count()\n  Line:\n    attributes:\n      order: { reference: Order, opposite: lines }\n      amount: { type: int }\n    states:\n      ON: { category: live }\n    transitions:\n      add: { kind: initial, to: ON, required_inputs: [order, amount] }\n    metrics:\n      noted:\n        source: objects\n        item: l\n        filter: l.order.note is not null\n        expression: count()\n";
        let lib = Library::load(&[("shop.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let ask = |s: &mut Store, object: Option<&str>, t: Option<&str>, xn: &str, inputs: Json| {
            let r = Request {
                transition: xn.into(),
                actor: OPERATOR.into(),
                object_id: object.map(str::to_string),
                type_: t.map(str::to_string),
                inputs: serde_json::from_value(inputs).unwrap(),
                occurred_at: None,
                propose: false,
                expires_at: None,
                expected_version: None,
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            };
            match request(s, &r).unwrap().unwrap() {
                Verdict::Satisfied { object_id, .. } => object_id,
                other => panic!("{other:?}"),
            }
        };
        let mut notes = Vec::new();
        for i in 0..300 {
            let note = ask(&mut s, None, Some("Note"), "write", json!({"text": "n"}));
            let order = ask(&mut s, None, Some("Order"), "open", json!({"note": note}));
            notes.push(note);
            for amount in [i, 1] {
                ask(
                    &mut s,
                    None,
                    Some("Line"),
                    "add",
                    json!({"order": order, "amount": amount}),
                );
            }
            if i < 20 {
                ask(&mut s, Some(&order), None, "close", json!({}));
            }
        }
        let w = s.snapshot().unwrap();
        let (lines, of_notes) = (
            crate::world::whole_type_reads("Line"),
            crate::world::whole_type_reads("Note"),
        );
        let p = page(
            &w,
            None,
            "Order.totals",
            &BTreeMap::new(),
            Some(&[]),
            None,
            None,
        )
        .unwrap();
        // 0 + 1 + ... + 299, and one each
        assert_eq!(p.rows[0].value, Value::Int(44_850 + 300));
        assert_eq!(crate::world::whole_type_reads("Line"), lines + 1);
        let p = page(
            &w,
            None,
            "Order.counted",
            &BTreeMap::new(),
            Some(&[]),
            None,
            None,
        )
        .unwrap();
        assert_eq!(p.rows[0].value, Value::Int(300));
        assert_eq!(crate::world::whole_type_reads("Note"), of_notes);
        // a line's order reached by its reference, by id, read whole once
        let orders = crate::world::whole_type_reads("Order");
        let w = s.snapshot().unwrap();
        let p = page(
            &w,
            None,
            "Line.noted",
            &BTreeMap::new(),
            Some(&[]),
            None,
            None,
        )
        .unwrap();
        assert_eq!(p.rows[0].value, Value::Int(600));
        assert_eq!(crate::world::whole_type_reads("Order"), orders + 1);
        // the scan over, a few notes are read alone
        for n in &notes[..20] {
            assert!(w.get(n).unwrap().is_some());
        }
        assert_eq!(crate::world::whole_type_reads("Note"), of_notes);
        // throughput: the twenty closings, read alone of the type's 320 events
        let w = s.snapshot().unwrap();
        let events = crate::world::events_read();
        let p = page(
            &w,
            None,
            "Order.throughput",
            &BTreeMap::new(),
            Some(&[]),
            None,
            None,
        )
        .unwrap();
        assert_eq!(p.rows[0].value, Value::Int(20));
        assert_eq!(crate::world::events_read() - events, 20);
    }

    /// A metric over objects bound to one of the item's own stored
    /// attributes reads only the objects holding the value, by the column's
    /// index, never the type whole, where a guard bound to one claimant read
    /// every claim for each request (fourteenth production review); it reads
    /// the same total.
    #[test]
    fn an_objects_metric_bound_to_a_column_reads_by_it() {
        let text = "\nmodule: pay\ncategories: [live]\ntypes:\n  Person:\n    attributes:\n      login: { type: identity, actor_kind: human }\n    states:\n      ON: { category: live }\n    transitions:\n      join: { kind: initial, to: ON, required_inputs: [login] }\n  Claim:\n    attributes:\n      claimant: { reference: Person, optional: true }\n      total: { type: int }\n    states:\n      OPEN: { category: live }\n    transitions:\n      open: { kind: initial, to: OPEN, required_inputs: [total], optional_inputs: [claimant] }\n    metrics:\n      claimed:\n        source: objects\n        item: c\n        dimensions:\n          claimant: c.claimant\n        expression: sum(c.total)\n";
        let lib = Library::load(&[("pay.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let person = |s: &mut Store, login: &str| {
            let r = Request {
                transition: "join".into(),
                actor: OPERATOR.into(),
                object_id: None,
                type_: Some("Person".into()),
                inputs: BTreeMap::from([("login".to_string(), json!(login))]),
                occurred_at: None,
                propose: false,
                expires_at: None,
                expected_version: None,
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            };
            match request(s, &r).unwrap().unwrap() {
                Verdict::Satisfied { object_id, .. } => object_id,
                other => panic!("{other:?}"),
            }
        };
        let (ana, ben) = (person(&mut s, "ana"), person(&mut s, "ben"));
        let (mut first, mut unclaimed) = (String::new(), String::new());
        for i in 0..23 {
            let who = if i % 4 == 0 { &ana } else { &ben };
            let mut inputs = BTreeMap::from([("total".to_string(), json!(i))]);
            if i < 20 {
                inputs.insert("claimant".to_string(), json!(who));
            }
            let r = Request {
                transition: "open".into(),
                actor: OPERATOR.into(),
                object_id: None,
                type_: Some("Claim".into()),
                inputs,
                occurred_at: None,
                propose: false,
                expires_at: None,
                expected_version: None,
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            };
            if let Verdict::Satisfied { object_id, .. } = request(&mut s, &r).unwrap().unwrap() {
                if i == 0 {
                    first = object_id;
                } else if i == 20 {
                    unclaimed = object_id;
                }
            }
        }
        let w = s.snapshot().unwrap();
        let before = crate::world::whole_type_reads("Claim");
        let v = Evaluator::new(&w, w.lib.type_("Claim"), Some(&first))
            .eval(&parse("metric(Claim.claimed, claimant := claimant)").unwrap())
            .unwrap();
        // ana's claims are 0, 4, 8, 12 and 16
        assert_eq!(v, Value::Int(40));
        assert_eq!(crate::world::whole_type_reads("Claim"), before);
        // bound to no claimant, its value unknown, as it was read whole
        let v = Evaluator::new(&w, w.lib.type_("Claim"), Some(&unclaimed))
            .eval(&parse("metric(Claim.claimed, claimant := claimant)").unwrap())
            .unwrap();
        assert_eq!(v, Value::Absent);
        // and a read of it bound to one claimant on the wire, as the service
        // reads it, the same, without the type (fifteenth production review);
        // a claimant is a person, which the request opts into (ADR-0206)
        let before = crate::world::whole_type_reads("Claim");
        let bind = BTreeMap::from([("claimant".to_string(), json!(ana))]);
        let p = page_as(
            &w,
            None,
            "Claim.claimed",
            &bind,
            Some(&[]),
            None,
            None,
            true,
        )
        .unwrap();
        assert_eq!(p.rows.len(), 1, "{:?}", p.refused);
        assert_eq!(p.rows[0].value, Value::Int(40));
        assert_eq!(crate::world::whole_type_reads("Claim"), before);
    }

    #[test]
    fn a_rate_over_a_window_bound_to_the_engineer() {
        let mut s = world();
        let (ana, ben) = (user(&mut s, "ana"), user(&mut s, "ben"));
        let j1 = job(&mut s, json!({ "engineer": ana }));
        let j2 = job(&mut s, json!({ "engineer": ben }));
        let j0 = job(&mut s, json!({}));
        record(&mut s, &j1, &["FAIL"]);
        s.now += 30 * DAY;
        record(&mut s, &j1, &["PASS", "PASS", "NA"]);
        // a result of a job with no engineer, in a group of its own
        record(&mut s, &j0, &["FAIL"]);
        let rate30 = "metric(pass_rate, engineer := engineer, over last 30 days)";
        let per_job = "metric(passes_per_job, engineer := engineer, over last 30 days)";
        // its window holding a result exactly thirty days old
        assert_eq!(
            read(&s, &j1, rate30, &[]),
            Value::Decimal(Decimal::new(667, 3).unwrap())
        );
        // a combined metric, each input windowed by its own time
        assert_eq!(num(&read(&s, &j1, per_job, &[])), rat(2, 1));
        take(&mut s, "start", &j1);
        assert_eq!(
            req(&mut s, "Job", "finish", Some(&j1), json!({})),
            Err(("unsatisfied".into(), "good".into()))
        );
        s.now += 1;
        // the failure a second later out of the window
        assert_eq!(num(&read(&s, &j1, rate30, &[])), rat(1, 1));
        // a combined metric whose divisor has left its window is unknown
        assert_eq!(read(&s, &j1, per_job, &[]), Value::Absent);
        assert_eq!(
            req(&mut s, "Job", "finish", Some(&j1), json!({})),
            Ok(j1.clone())
        );
        // a rate over no rows, and a dimension bound to an absent value
        assert_eq!(read(&s, &j2, rate30, &[]), Value::Absent);
        assert_eq!(read(&s, &j0, rate30, &[]), Value::Absent);
        // an input without a dimension the read binds contributes its whole
        // value (ADR-0124, ADR-0147)
        assert_eq!(
            read(&s, &j1, "metric(passes_of_all, engineer := engineer)", &[]),
            Value::Decimal(Decimal::new(667, 3).unwrap())
        );
        // a distinct count leaves out an absent value, and a filter unknown
        // for a row selects it not
        assert_eq!(
            read(&s, &j1, "metric(engineers_checked)", &[]),
            Value::Int(1)
        );
        assert_eq!(read(&s, &j1, "metric(anas_jobs)", &[]), Value::Int(1));
        // int / int truncates in a metric's value, before the rounding
        assert_eq!(read(&s, &j1, "metric(third_of_checks)", &[]), Value::Int(1));
    }

    #[test]
    fn a_span_holds_what_began_it_and_an_event_what_was_held_before() {
        let mut s = world();
        let (ana, ben) = (user(&mut s, "ana"), user(&mut s, "ben"));
        let jt = job(&mut s, json!({ "engineer": ana }));
        req(&mut s, "Job", "take", Some(&jt), json!({ "engineer": ben })).unwrap();
        s.now += 2 * 3600;
        take(&mut s, "back", &jt);
        let who = |id: &str| [("who", Value::Object(id.to_string()))];
        let working = "metric(time_working, engineer := who)";
        assert_eq!(
            read(&s, &jt, working, &who(&ben)),
            Value::Duration(2 * 3600)
        );
        assert_eq!(read(&s, &jt, working, &who(&ana)), Value::Absent);
        let takes = "metric(takes, engineer := who)";
        assert_eq!(read(&s, &jt, takes, &who(&ana)), Value::Int(1));
        assert_eq!(read(&s, &jt, takes, &who(&ben)), Value::Int(0));
    }

    #[test]
    fn a_value_is_rounded_half_to_even_once() {
        let mut s = world();
        let (cara, dan) = (user(&mut s, "cara"), user(&mut s, "dan"));
        let jc = job(&mut s, json!({ "engineer": cara }));
        let jd = job(&mut s, json!({ "engineer": dan }));
        record(
            &mut s,
            &jc,
            &[
                "PASS", "FAIL", "FAIL", "FAIL", "FAIL", "FAIL", "FAIL", "FAIL",
            ],
        );
        record(
            &mut s,
            &jd,
            &[
                "PASS", "PASS", "PASS", "FAIL", "FAIL", "FAIL", "FAIL", "FAIL",
            ],
        );
        let share = "metric(pass_share, engineer := engineer)";
        assert_eq!(
            read(&s, &jc, share, &[]),
            Value::Decimal(Decimal::new(12, 2).unwrap())
        );
        assert_eq!(
            read(&s, &jd, share, &[]),
            Value::Decimal(Decimal::new(38, 2).unwrap())
        );
    }

    #[test]
    fn spans_holders_ranks_counts_and_set_dimensions() {
        let mut s = world();
        let (ana, ben) = (user(&mut s, "ana"), user(&mut s, "ben"));
        let ja = job(&mut s, json!({ "engineer": ana, "tags": ["urgent"] }));
        for hours in 1..=4 {
            take(&mut s, "start", &ja);
            s.now += hours * 3600;
            take(&mut s, "back", &ja);
        }
        let jb = job(
            &mut s,
            json!({ "engineer": ben, "tags": ["urgent", "vip"] }),
        );
        take(&mut s, "start", &jb);
        s.now += 5 * 3600;
        req(
            &mut s,
            "Job",
            "assign",
            Some(&jb),
            json!({ "engineer": ana }),
        )
        .unwrap();
        s.now += 3600;
        take(&mut s, "back", &jb);
        // a median, the lower middle of four spans
        assert_eq!(
            read(&s, &ja, "metric(time_working, engineer := engineer)", &[]),
            Value::Duration(2 * 3600)
        );
        // a span attributed to whoever held the job when it began
        assert_eq!(
            read(
                &s,
                &jb,
                "metric(time_working, engineer := who)",
                &[("who", Value::Object(ben.clone()))]
            ),
            Value::Duration(6 * 3600)
        );
        // a nearest-rank percentile, the fourth of five
        assert_eq!(
            read(&s, &ja, "metric(slow_working)", &[]),
            Value::Duration(4 * 3600)
        );
        // a transition count, its actor bound
        assert_eq!(
            read(&s, &ja, r#"metric(starts, actor := "operator")"#, &[]),
            Value::Int(5)
        );
        // the standard rework, each return to a state held before
        assert_eq!(read(&s, &ja, "metric(Job.rework)", &[]), Value::Int(8));
        let jc = job(&mut s, json!({ "tags": ["vip"] }));
        // a guard over the standard open work of an assignee, and of one with none
        assert_eq!(
            req(
                &mut s,
                "Job",
                "assign",
                Some(&jc),
                json!({ "engineer": ana })
            ),
            Err(("unsatisfied".into(), "not_busy".into()))
        );
        assert_eq!(
            req(
                &mut s,
                "Job",
                "assign",
                Some(&jc),
                json!({ "engineer": ben })
            ),
            Ok(jc.clone())
        );
        // a set dimension, a job under each of its tags
        assert_eq!(
            read(&s, &ja, r#"metric(tagged, tag := "urgent")"#, &[]),
            Value::Int(2)
        );
        assert_eq!(
            read(&s, &ja, r#"metric(tagged, tag := "vip")"#, &[]),
            Value::Int(2)
        );
        // an average of integers, to six places, rounded once, at the end
        assert_eq!(
            read(&s, &ja, "metric(tags_per_job)", &[]),
            Value::Decimal(Decimal::new(1_333_333, 6).unwrap())
        );
        assert_eq!(num(&read(&s, &ja, "metric(tags_tripled)", &[])), rat(4, 1));
        take(&mut s, "start", &jb);
        take(&mut s, "pause", &jb);
        // a transition count by name, apart from another between the same states
        assert_eq!(read(&s, &ja, "metric(backs)", &[]), Value::Int(5));
    }

    #[test]
    fn a_member_accrues_only_while_its_object_is_open() {
        // an assignment still current stops at the start of the closed stretch
        let mut s = world();
        let eve = user(&mut s, "eve");
        let je = job(&mut s, json!({ "engineer": eve }));
        take(&mut s, "start", &je);
        s.now += 3600;
        take(&mut s, "shelve", &je);
        s.now += 5 * 3600;
        take(&mut s, "close", &je);
        s.now += 7 * 3600;
        assert_eq!(
            read(&s, &je, "metric(time_held)", &[]),
            Value::Duration(3600)
        );
        // time in a closed state, and in the one the object is in, are real
        assert_eq!(
            read(&s, &je, "time_in(SHELVED)", &[]),
            Value::Duration(5 * 3600)
        );
        assert_eq!(
            read(&s, &je, "time_in(DONE)", &[]),
            Value::Duration(7 * 3600)
        );
        // a member's span ending after its object closed stops at the close
        let mut s = world();
        let (ann, bob) = (user(&mut s, "ann"), user(&mut s, "bob"));
        let jx = job(&mut s, json!({ "engineer": ann }));
        take(&mut s, "start", &jx);
        s.now += 2 * 3600;
        take(&mut s, "shelve", &jx);
        s.now += 3 * 3600;
        req(
            &mut s,
            "Job",
            "assign",
            Some(&jx),
            json!({ "engineer": bob }),
        )
        .unwrap();
        s.now += 4 * 3600;
        assert_eq!(
            read(&s, &jx, "metric(time_held)", &[]),
            Value::Duration(2 * 3600)
        );
        // and leaves out a closed stretch its object reopened from
        let mut s = world();
        let cy = user(&mut s, "cy");
        let jy = job(&mut s, json!({ "engineer": cy }));
        take(&mut s, "start", &jy);
        s.now += 3600;
        take(&mut s, "shelve", &jy);
        s.now += 3 * 3600;
        take(&mut s, "resume", &jy);
        s.now += 2 * 3600;
        assert_eq!(
            read(&s, &jy, "metric(time_held)", &[]),
            Value::Duration(3 * 3600)
        );
        // a member's split piece counts only its open time
        let mut s = world();
        let dee = user(&mut s, "dee");
        s.now = parse_timestamp("2026-01-31T12:00Z").unwrap();
        let jz = job(&mut s, json!({ "engineer": dee }));
        take(&mut s, "start", &jz);
        s.now = parse_timestamp("2026-01-31T18:00Z").unwrap();
        take(&mut s, "shelve", &jz);
        s.now = parse_timestamp("2026-02-01T06:00Z").unwrap();
        take(&mut s, "resume", &jz);
        s.now = parse_timestamp("2026-02-01T12:00Z").unwrap();
        assert_eq!(
            read(&s, &jz, r#"metric(held_by_month, month := "2026-01")"#, &[]),
            Value::Duration(6 * 3600)
        );
        assert_eq!(
            read(&s, &jz, r#"metric(held_by_month, month := "2026-02")"#, &[]),
            Value::Duration(6 * 3600)
        );
    }

    #[test]
    fn a_span_split_across_the_months_it_covers() {
        let mut s = world();
        let one = job(&mut s, json!({}));
        let two = job(&mut s, json!({}));
        s.now = parse_timestamp("2026-01-31T12:00Z").unwrap();
        take(&mut s, "start", &one);
        s.now = parse_timestamp("2026-02-02T00:00Z").unwrap();
        take(&mut s, "back", &one);
        s.now = parse_timestamp("2026-02-28T18:00Z").unwrap();
        take(&mut s, "start", &two);
        s.now = parse_timestamp("2026-03-01T06:00Z").unwrap();
        let hours = |text: &str| match read(&s, &one, text, &[]) {
            Value::Duration(d) => d / 3600,
            other => panic!("{other:?}"),
        };
        // its piece in the month it began, the next, and a current one's up to now
        assert_eq!(hours(r#"metric(working_by_month, month := "2026-01")"#), 12);
        assert_eq!(hours(r#"metric(working_by_month, month := "2026-02")"#), 30);
        assert_eq!(hours(r#"metric(working_by_month, month := "2026-03")"#), 6);
        // a window clipping the pieces to its last ten hours
        assert_eq!(hours("metric(working_by_month, over last 10 h)"), 10);
        // a split metric counting the spans active in a month
        assert_eq!(
            read(
                &s,
                &one,
                r#"metric(jobs_working_by_month, month := "2026-02")"#,
                &[]
            ),
            Value::Int(2)
        );
    }

    /// A page's rows as each group's dimensions and its value's number.
    type Group = Vec<(String, Value)>;

    fn rows_of(p: &Page) -> Vec<(Group, Option<Rat>)> {
        p.rows
            .iter()
            .map(|r| (r.dimensions.clone(), num(&r.value)))
            .collect()
    }

    fn page_of(
        s: &Store,
        name: &str,
        bind: &[(&str, Value)],
        keep: Option<&[&str]>,
        over: Option<i64>,
        filter: Option<&str>,
    ) -> Page {
        page_read(s, name, bind, keep, over, filter, false)
    }

    /// A page as [`page_of`] reads it, the request opting into reading by
    /// a personal value or not (ADR-0206).
    fn page_read(
        s: &Store,
        name: &str,
        bind: &[(&str, Value)],
        keep: Option<&[&str]>,
        over: Option<i64>,
        filter: Option<&str>,
        reads_personal: bool,
    ) -> Page {
        let w = s.snapshot().unwrap();
        let bind: BTreeMap<String, Json> = bind
            .iter()
            .map(|(d, v)| (d.to_string(), crate::wire::encode(v)))
            .collect();
        let keep: Option<Vec<String>> = keep.map(|k| k.iter().map(|d| d.to_string()).collect());
        page_as(
            &w,
            None,
            name,
            &bind,
            keep.as_deref(),
            over,
            filter,
            reads_personal,
        )
        .unwrap()
    }

    fn dims(pairs: &[(&str, Value)]) -> Vec<(String, Value)> {
        pairs
            .iter()
            .map(|(d, v)| (d.to_string(), v.clone()))
            .collect()
    }

    /// A report over a window reads the objects whose time or date falls in
    /// it by that column, and no other, where it read every object of the
    /// type, and answers as a read of every object does (ADR-0210 decision
    /// 2).
    #[test]
    fn a_window_over_an_objects_time_reads_by_its_column() {
        let text = "\nmodule: v\ncategories: [live]\ntypes:\n  Visit:\n    attributes:\n      at: { type: timestamp, indexed: true }\n      on: { type: date, indexed: true }\n    states:\n      ON: { category: live }\n    transitions:\n      log: { kind: initial, to: ON, required_inputs: [at, on] }\n    metrics:\n      recent:\n        source: objects\n        item: v\n        time_dimension: v.at\n        expression: count()\n      recent_days:\n        source: objects\n        item: v\n        time_dimension: v.on\n        expression: count()\n";
        let lib = Library::load(&[("v.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        // a visit a day for 400 days, each at its day's start
        for d in 0..400 {
            s.now = d * 86_400;
            let r = crate::request::Request {
                transition: "log".into(),
                actor: crate::model::OPERATOR.into(),
                object_id: None,
                type_: Some("Visit".into()),
                inputs: [
                    ("at".to_string(), json!(crate::value::timestamp_text(s.now))),
                    ("on".to_string(), json!(crate::value::date_text(d))),
                ]
                .into_iter()
                .collect(),
                occurred_at: None,
                propose: false,
                expires_at: None,
                expected_version: None,
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            };
            assert!(matches!(
                crate::request::request(&mut s, &r).unwrap().unwrap(),
                Verdict::Satisfied { .. }
            ));
        }
        for name in ["Visit.recent", "Visit.recent_days"] {
            // thirty days back from day 399 holds days 369 to 399
            let w = s.snapshot().unwrap();
            let windowed = page_as(
                &w,
                None,
                name,
                &BTreeMap::new(),
                None,
                Some(30 * 86_400),
                None,
                false,
            )
            .unwrap();
            assert!(w.held_count() <= 32, "{name} held {}", w.held_count());
            assert_eq!(rows_of(&windowed), vec![(Vec::new(), Some(Rat::int(31)))]);
            // as a read of every object, held first
            let all = s.snapshot().unwrap();
            all.of_type("Visit").unwrap();
            let whole = page_as(
                &all,
                None,
                name,
                &BTreeMap::new(),
                None,
                Some(30 * 86_400),
                None,
                false,
            )
            .unwrap();
            assert_eq!(windowed.rows, whole.rows, "{name}");
        }
    }

    /// The reference runner's reader cases (`flowrun.py`,
    /// `metrics_self_test`, "a reader's read").
    #[test]
    fn a_reader_reads_a_row_per_group() {
        let mut s = world();
        let (ana, ben) = (user(&mut s, "ana"), user(&mut s, "ben"));
        let k1 = job(&mut s, json!({ "engineer": ana, "tags": ["urgent"] }));
        let k2 = job(
            &mut s,
            json!({ "engineer": ben, "tags": ["urgent", "vip"] }),
        );
        let k0 = job(&mut s, json!({}));
        record(&mut s, &k1, &["PASS", "FAIL"]);
        record(&mut s, &k2, &["PASS", "PASS"]);
        record(&mut s, &k0, &["FAIL"]);
        let (a, b) = (Value::Object(ana.clone()), Value::Object(ben.clone()));
        let rate = page_of(&s, "Job.pass_rate", &[], None, None, None);
        // a row per engineer, the absent one last, as their objects were created
        assert_eq!(
            rows_of(&rate),
            vec![
                (dims(&[("engineer", a.clone())]), rat(1, 2)),
                (dims(&[("engineer", b.clone())]), rat(1, 1)),
                (dims(&[("engineer", Value::Absent)]), rat(0, 1)),
            ]
        );
        // a flag on each declared group it holds for
        assert_eq!(
            rate.flags
                .iter()
                .map(|f| (f.flag.as_str(), f.group.clone(), f.unknown))
                .collect::<Vec<_>>(),
            vec![
                ("low", dims(&[("engineer", a.clone())]), false),
                ("low", dims(&[("engineer", Value::Absent)]), false),
            ]
        );
        assert!(rate.history_complete && rate.refused.is_none());
        // nothing kept, and a dimension bound
        assert_eq!(
            rows_of(&page_of(&s, "Job.pass_rate", &[], Some(&[]), None, None)),
            vec![(vec![], rat(3, 5))]
        );
        // nothing kept, one row even over no rows
        assert_eq!(
            rows_of(&page_of(
                &s,
                "Job.pass_rate",
                &[],
                Some(&[]),
                None,
                Some("r.outcome == Outcome.NA")
            )),
            vec![(vec![], None)]
        );
        // bound to an engineer, a person, which the request opts into
        assert_eq!(
            rows_of(&page_read(
                &s,
                "Job.pass_rate",
                &[("engineer", b.clone())],
                Some(&[]),
                None,
                None,
                true
            )),
            vec![(vec![], rat(1, 1))]
        );
        // a reader's filter narrows the rows before they are grouped
        assert_eq!(
            rows_of(&page_of(
                &s,
                "Job.pass_rate",
                &[],
                None,
                None,
                Some("r.outcome == Outcome.PASS")
            )),
            vec![
                (dims(&[("engineer", a.clone())]), rat(1, 1)),
                (dims(&[("engineer", b.clone())]), rat(1, 1))
            ]
        );
        // a set dimension, an object under each value and an absent one under none
        let text = |t: &str| Value::Text(t.into());
        assert_eq!(
            rows_of(&page_of(&s, "Job.tagged", &[], None, None, None)),
            vec![
                (dims(&[("tag", text("urgent"))]), rat(2, 1)),
                (dims(&[("tag", text("vip"))]), rat(1, 1)),
                (dims(&[("tag", Value::Absent)]), rat(1, 1)),
            ]
        );
        // two dimensions through one set taken per member, never crossed
        let outcome = |m: &str| Value::Member {
            enumeration: "Outcome".into(),
            member: m.into(),
        };
        assert_eq!(
            rows_of(&page_of(&s, "Job.outcome_pairs", &[], None, None, None)),
            vec![
                (
                    dims(&[("outcome", outcome("PASS")), ("again", outcome("PASS"))]),
                    rat(2, 1)
                ),
                (
                    dims(&[("outcome", outcome("FAIL")), ("again", outcome("FAIL"))]),
                    rat(2, 1)
                ),
            ]
        );
        // a combined metric joined over every group, a missing input
        // contributing its value over no rows
        assert_eq!(
            rows_of(&page_of(&s, "passes_per_job", &[], None, None, None)),
            vec![
                (dims(&[("engineer", a.clone())]), rat(1, 1)),
                (dims(&[("engineer", b.clone())]), rat(2, 1)),
                (dims(&[("engineer", Value::Absent)]), rat(0, 1)),
            ]
        );
        // a flag unknown on a group whose value is
        let k3 = job(&mut s, json!({ "engineer": ana }));
        for x in ["start", "shelve", "close"] {
            take(&mut s, x, &k3);
        }
        assert_eq!(
            page_of(&s, "Job.done_spans", &[], None, None, None)
                .flags
                .iter()
                .map(|f| (f.flag.as_str(), f.group.clone(), f.unknown))
                .collect::<Vec<_>>(),
            vec![("long", dims(&[("engineer", a.clone())]), true)]
        );
        // each refusal a value
        let refused =
            |name: &str, keep: Option<&[&str]>, over: Option<i64>, filter: Option<&str>| {
                page_of(&s, name, &[], keep, over, filter).refused
            };
        assert_eq!(
            [
                refused("nonesuch", None, None, None),
                refused("Job.pass_rate", Some(&["nobody"]), None, None),
                refused("passes_per_job", None, None, Some("true")),
                refused("Job.tagged", None, Some(7 * DAY), None),
                refused("Job.jobs_opened", None, None, Some(r#"o.memo == "x""#)),
            ],
            [
                Some("unknown_metric"),
                Some("unknown_dimension"),
                Some("filter_on_combined"),
                Some("no_window"),
                Some("personal"),
            ]
        );
        // every row a guard could ask for is the guard's value
        let w = s.snapshot().unwrap();
        for name in [
            "Job.pass_rate",
            "Job.tagged",
            "Job.outcome_pairs",
            "passes_per_job",
            "passes_of_all",
            "Job.jobs_opened",
        ] {
            let found = find(w.lib, &parse(name).unwrap(), None, None).unwrap();
            for row in page_of(&s, name, &[], None, None, None).rows {
                if row.dimensions.iter().any(|(_, v)| v.is_absent()) {
                    continue;
                }
                let bound: BTreeMap<String, Value> = row.dimensions.iter().cloned().collect();
                assert_eq!(
                    super::read(&w, None, &found, &bound, None).unwrap(),
                    row.value,
                    "{name} {:?}",
                    row.dimensions
                );
            }
        }
    }

    /// A metric over intervals that never reads `held` reads the state's
    /// spans by type and no object's record, and answers as a read of every
    /// record does; a reader's filter reading `held` reads the records, as
    /// before (the audit's cost B3, ADR-0205 decision 13).
    #[test]
    fn a_read_over_intervals_reads_the_spans_it_needs() {
        let mut s = world();
        let ana = user(&mut s, "ana");
        for hours in 1..=3 {
            let j = job(&mut s, json!({ "engineer": ana }));
            take(&mut s, "start", &j);
            s.now += hours * 3600;
            take(&mut s, "back", &j);
        }
        let before = crate::world::records_read();
        let narrow = page_of(&s, "Job.time_in_state", &[], Some(&["state"]), None, None);
        assert_eq!(crate::world::records_read(), before, "a record read");
        assert!(!narrow.rows.is_empty());
        // the same as a read of every record, held first
        let w = s.snapshot().unwrap();
        w.with_parts(&w.of_type("Job").unwrap()).unwrap();
        let whole = page(
            &w,
            None,
            "Job.time_in_state",
            &BTreeMap::new(),
            Some(&["state".to_string()]),
            None,
            None,
        )
        .unwrap();
        assert_eq!(narrow.rows, whole.rows);
        // an object whose record the read holds, as an operation may have
        // changed it, read from what it holds: two spans ten hours long
        // added, WORKING's median from 2 h to 3 h
        let mut w = s.snapshot().unwrap();
        let first = w.of_type("Job").unwrap()[0].id.clone();
        {
            let o = w.get_mut(&first).unwrap().unwrap();
            let spans = o.parts_mut().unwrap().spans.get_mut("state").unwrap();
            let mut longer = spans
                .iter()
                .find(|sp| matches!(&sp.value, Value::State { name, .. } if name == "WORKING"))
                .cloned()
                .unwrap();
            longer.left_at = Some(longer.entered_at + 10 * 3600);
            spans.push(longer.clone());
            spans.push(longer);
        }
        let changed = page(
            &w,
            None,
            "Job.time_in_state",
            &BTreeMap::new(),
            Some(&["state".to_string()]),
            None,
            None,
        )
        .unwrap();
        assert_eq!(changed.rows[1].value, Value::Duration(3 * 3600));
        // a filter reading `held` reads the records
        let before = crate::world::records_read();
        let held = page_of(
            &s,
            "Job.time_in_state",
            &[],
            Some(&["state"]),
            None,
            Some("i.held.engineer is not null"),
        );
        assert_eq!(held.refused, None, "{:?}", held.reason);
        assert!(crate::world::records_read() > before);
        // over a tracked member, the time in a closed state taken out, read
        // from the state's spans beside the member's: two hours working,
        // then five shelved
        let mut s = world();
        let ana = user(&mut s, "ana");
        let j = job(&mut s, json!({ "engineer": ana }));
        take(&mut s, "start", &j);
        s.now += 2 * 3600;
        take(&mut s, "shelve", &j);
        s.now += 5 * 3600;
        let before = crate::world::records_read();
        let held = page_of(&s, "Job.time_held", &[], Some(&[]), None, None);
        assert_eq!(crate::world::records_read(), before, "a record read");
        assert_eq!(held.rows[0].value, Value::Duration(2 * 3600));
    }

    #[test]
    fn a_page_orders_states_as_declared() {
        let mut s = world();
        let ana = user(&mut s, "ana");
        let ja = job(&mut s, json!({ "engineer": ana }));
        for hours in 1..=4 {
            take(&mut s, "start", &ja);
            s.now += hours * 3600;
            take(&mut s, "back", &ja);
        }
        let jb = job(&mut s, json!({ "engineer": ana }));
        take(&mut s, "start", &jb);
        s.now += 6 * 3600;
        take(&mut s, "back", &jb);
        take(&mut s, "start", &jb);
        take(&mut s, "pause", &jb);
        let state = |n: &str| Value::State {
            type_: "Job".into(),
            name: n.into(),
        };
        // WORKING's spans 0, 1, 2, 3, 4 and 6 h; the third
        assert_eq!(
            page_of(&s, "Job.time_in_state", &[], Some(&["state"]), None, None)
                .rows
                .iter()
                .map(|r| (r.dimensions.clone(), r.value.clone()))
                .collect::<Vec<_>>(),
            vec![
                (dims(&[("state", state("OPEN"))]), Value::Duration(0)),
                (
                    dims(&[("state", state("WORKING"))]),
                    Value::Duration(2 * 3600)
                ),
            ]
        );
    }

    /// Gaps, which the reference runner cannot show, its objects having
    /// whole records: a creation the import could not date, read by a
    /// metric, and a window beginning before an object's record while it
    /// existed (`declaration-syntax.md` §6.9, ADR-0106).
    #[test]
    fn a_row_names_the_objects_read_from_before_their_record() {
        use crate::import::{EnteredAt, ImportBatch, ImportedObject, import_batch};
        const FLEET: &str = r#"
module: fleet
categories: [active, closed]
types:
  Unit:
    mirror: true
    attributes:
      code: { type: string, external: legacy }
    states:
      ACTIVE: { category: active }
    metrics:
      by_month: { description: x, source: objects, item: o, dimensions: { month: month(o.created_at) }, expression: count() }
      units: { description: x, source: objects, item: o, time_dimension: o.created_at, expression: count() }
      in_state: { description: x, source: intervals, item: i, dimensions: { state: i.state }, expression: count() }
      entries: { description: x, source: intervals, item: i, time_dimension: i.entered_at, expression: count() }
"#;
        let lib = Library::load(&[("fleet.yaml".into(), FLEET.into())]).unwrap();
        let port = parse_timestamp("2026-09-01T00:00:00Z").unwrap();
        let mut s = Store::sandbox(lib, port).unwrap();
        let t = |text: &str| parse_timestamp(text).unwrap();
        let unit = |key: &str, created_at: Option<i64>, entered: Option<i64>| ImportedObject {
            type_: "Unit".into(),
            legacy_key: key.into(),
            state: "ACTIVE".into(),
            attributes: BTreeMap::from([("code".to_string(), json!(key))]),
            entered: entered
                .map(|at| {
                    BTreeMap::from([(
                        "state".to_string(),
                        EnteredAt {
                            at,
                            by: None,
                            by_kind: Some("human".into()),
                        },
                    )])
                })
                .unwrap_or_default(),
            created_at,
            ..ImportedObject::default()
        };
        let batch = ImportBatch {
            objects: vec![
                // created on 1 August, its record vouched for from 15 August
                unit(
                    "U-1",
                    Some(t("2026-08-01T00:00:00Z")),
                    Some(t("2026-08-15T00:00:00Z")),
                ),
                // ported with no history: undated, its record from the port
                unit("U-2", None, None),
            ],
            sequences: Vec::new(),
            legacy_kinds: BTreeMap::new(),
        };
        let report = import_batch(&mut s, OPERATOR, &batch, &[], false)
            .unwrap()
            .unwrap();
        assert!(report.refusal.is_none(), "{report:?}");
        let w = s.snapshot().unwrap();
        let id = |key: &str| {
            w.external_all().unwrap()[&("legacy".to_string(), format!("Unit:{key}"))].clone()
        };
        let (u1, u2) = (id("U-1"), id("U-2"));
        // a creation the import could not date, read by the metric
        let months = page_of(&s, "Unit.by_month", &[], None, None, None);
        assert_eq!(
            months
                .rows
                .iter()
                .map(|r| r.gaps.clone())
                .collect::<Vec<_>>(),
            vec![vec![], vec![u2.clone()]]
        );
        assert!(!months.history_complete);
        // a window over creation times holds U-2 alone, whose undated
        // creation it reads; U-1 was created before it
        let recent = page_of(&s, "Unit.units", &[], None, Some(20 * DAY), None);
        assert_eq!(recent.rows[0].gaps, vec![u2.clone()]);
        // a window beginning on 12 August, before U-1's record, while U-1
        // existed, over the span U-1 entered on 15 August; U-2 did not exist
        // before the port
        let entries = page_of(&s, "Unit.entries", &[], None, Some(20 * DAY), None);
        assert_eq!(entries.rows[0].value, Value::Int(2));
        assert_eq!(entries.rows[0].gaps, vec![u1.clone()]);
        // spans that begin where their record does carry none
        let spans = page_of(&s, "Unit.in_state", &[], None, None, None);
        assert!(spans.history_complete, "{spans:?}");
    }

    /// The reference runner's attempt-log cases (`flowrun.py`,
    /// `metrics_self_test`, "the attempt log"): a refusal, a flag and an
    /// audit each a row, read as rows and as daily counts.
    #[test]
    fn the_attempt_log_holds_each_refusal_flag_and_audit() {
        let mut s = world();
        let ana = user(&mut s, "ana");
        let jr = job(&mut s, json!({ "engineer": ana }));
        take(&mut s, "start", &jr);
        take(&mut s, "review", &jr); // its warn guard fails: a flag
        take(&mut s, "inspect", &jr); // its audit guard fails: a would-be refusal
        let _ = req(&mut s, "Job", "finish", Some(&jr), json!({})); // the pass rate unknown
        let _ = req(&mut s, "Job", "nonesuch", Some(&jr), json!({}));
        let ask = |transition: &str, actor: &str, object: &str, propose: bool| Request {
            transition: transition.into(),
            actor: actor.into(),
            object_id: Some(object.to_string()),
            type_: None,
            inputs: BTreeMap::new(),
            occurred_at: None,
            propose,
            expires_at: None,
            expected_version: None,
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
        };
        // an actor the store does not hold
        request(&mut s, &ask("review", "nobody", &jr, false))
            .unwrap()
            .unwrap();
        // a proposal of the finish, its approval refused
        let Verdict::Satisfied { object_id: p, .. } =
            request(&mut s, &ask("finish", OPERATOR, &jr, true))
                .unwrap()
                .unwrap()
        else {
            panic!("the proposal is filed")
        };
        request(&mut s, &ask("approve", OPERATOR, &p, false))
            .unwrap()
            .unwrap();
        let w = s.snapshot().unwrap();
        let rows: Vec<_> = w
            .attempts()
            .unwrap()
            .iter()
            .map(|a| {
                (
                    a.type_.clone().unwrap_or_default(),
                    a.object_id.clone().unwrap_or_default(),
                    a.transition.clone().unwrap_or_default(),
                    a.verdict.clone(),
                    a.clause.clone(),
                    a.unknown,
                    a.enforced,
                    a.flagged,
                    a.actor_kind.clone(),
                    a.proposal.is_some(),
                )
            })
            .collect();
        let row = |x: &str,
                   verdict: &str,
                   clause: Option<&str>,
                   unknown,
                   enforced,
                   flagged,
                   kind: &str,
                   named| {
            (
                "Job".to_string(),
                jr.clone(),
                x.to_string(),
                verdict.to_string(),
                clause.map(str::to_string),
                unknown,
                enforced,
                flagged,
                kind.to_string(),
                named,
            )
        };
        assert_eq!(
            rows,
            vec![
                row(
                    "review",
                    "unsatisfied",
                    Some("checked"),
                    false,
                    false,
                    true,
                    "human",
                    false
                ),
                row(
                    "inspect",
                    "unsatisfied",
                    Some("checked"),
                    false,
                    false,
                    false,
                    "human",
                    false
                ),
                row(
                    "finish",
                    "unsatisfied",
                    Some("good"),
                    true,
                    true,
                    false,
                    "human",
                    false
                ),
                row(
                    "nonesuch",
                    "unknown_transition",
                    None,
                    false,
                    true,
                    false,
                    "human",
                    false
                ),
                row(
                    "review",
                    "unsatisfied",
                    Some("actor_known"),
                    false,
                    true,
                    false,
                    "unknown",
                    false
                ),
                row(
                    "finish",
                    "unsatisfied",
                    Some("good"),
                    true,
                    true,
                    false,
                    "human",
                    true
                ),
            ]
        );
        // a flag and an audit name the event they were raised on
        assert!(
            w.attempts().unwrap()[..2]
                .iter()
                .all(|a| a.applied_position.is_some())
        );
        // the refused approval's row names the proposal in a column of its
        // own, not among the request's values (storage-schema.md §6)
        let approval = w
            .attempts()
            .unwrap()
            .iter()
            .find(|a| a.proposal.is_some())
            .unwrap();
        assert_eq!(approval.proposal.as_deref(), Some(p.as_str()));
        assert!(approval.request_values.get("proposal").is_none());
        // the proposal's read carries the verdict its refused approval left
        // (ADR-0157)
        let view = crate::reads::get(&w, &p).unwrap().unwrap();
        let last = view
            .derived
            .iter()
            .find(|(n, _)| n == "last_verdict")
            .map(|(_, v)| v.clone());
        let Some(Value::Row(last)) = last else {
            panic!("a last verdict: {:?}", view.derived)
        };
        assert_eq!(
            (
                last["verdict"].clone(),
                last["clause"].clone(),
                last["remedy"].clone()
            ),
            (
                Value::Text("unsatisfied".into()),
                Value::Text("good".into()),
                Value::Text("dependent".into())
            )
        );
        drop(w);
        // the standard refusals, flags raised and refusal rate, and an
        // object's own attempts
        assert_eq!(
            read(
                &s,
                &jr,
                r#"metric(Job.refusals, transition := "finish")"#,
                &[]
            ),
            Value::Int(2)
        );
        assert_eq!(read(&s, &jr, "metric(Job.refusals)", &[]), Value::Int(4));
        assert_eq!(
            read(
                &s,
                &jr,
                r#"metric(Job.flags_raised, clause := "checked")"#,
                &[]
            ),
            Value::Int(1)
        );
        assert_eq!(
            num(&read(
                &s,
                &jr,
                r#"metric(Job.refusal_rate, transition := "finish", clause := "good")"#,
                &[]
            )),
            rat(1, 1)
        );
        assert_eq!(
            read(&s, &jr, "count(a in this.attempts where a.enforced)", &[]),
            Value::Int(4)
        );
    }

    /// A metric over an observation kind reads each observation nothing has
    /// corrected, its correction among them (`declaration-syntax.md` §6.9).
    #[test]
    fn a_metric_over_a_kind_reads_what_nothing_corrects() {
        let mut s = world();
        let j = job(&mut s, json!({}));
        let first = req(
            &mut s,
            "Check",
            "record",
            None,
            json!({ "subject": j, "outcome": "FAIL" }),
        )
        .unwrap();
        req(
            &mut s,
            "Check",
            "record",
            None,
            json!({ "subject": j, "outcome": "PASS", "corrects": first }),
        )
        .unwrap();
        assert_eq!(num(&read(&s, &j, "metric(Job.pass_share)", &[])), rat(1, 1));
    }

    /// The reference runner's label cases (`flowrun.py`,
    /// `metrics_self_test`, "labels"): the built-in kind every type carries,
    /// its name normalised, its subject's state stamped, dated when it is
    /// recorded, and read only by metrics.
    #[test]
    fn labels_are_recorded_on_any_subject_and_read_by_metrics() {
        let mut s = world();
        let ana = user(&mut s, "ana");
        let jl = job(&mut s, json!({ "engineer": ana }));
        let jm = job(&mut s, json!({}));
        let label = |s: &mut Store, inputs: Json| req(s, LABEL, "record", None, inputs);
        let l1 = label(&mut s, json!({ "subject": jl, "name": "  Late-Risk " })).unwrap();
        // a name empty once normalised is no name (ADR-0161)
        assert_eq!(
            label(&mut s, json!({ "subject": jl, "name": "   " })),
            Err(("invalid_input".into(), String::new()))
        );
        take(&mut s, "start", &jl);
        s.now += 60;
        let l2 = label(
            &mut s,
            json!({ "subject": jl, "name": "late-risk", "note": "rang twice" }),
        )
        .unwrap();
        let l3 = label(&mut s, json!({ "subject": jm, "name": "Odd" })).unwrap();
        let l4 = label(
            &mut s,
            json!({ "subject": jm, "name": "odd-one", "corrects": l3 }),
        )
        .unwrap();
        let w = s.snapshot().unwrap();
        let held = |id: &str| {
            let o = w.whole(id).unwrap().unwrap();
            (
                o.attr("name"),
                o.attr("subject_state"),
                o.attr("occurred_at"),
                o.created_by_kind.clone(),
            )
        };
        let (now, text) = (s.now, |t: &str| Value::Text(t.into()));
        assert_eq!(
            [held(&l1), held(&l2), held(&l4)],
            [
                (
                    text("late-risk"),
                    text("OPEN"),
                    Value::Timestamp(now - 60),
                    "human".to_string()
                ),
                (
                    text("late-risk"),
                    text("WORKING"),
                    Value::Timestamp(now),
                    "human".to_string()
                ),
                (
                    text("odd-one"),
                    text("OPEN"),
                    Value::Timestamp(now),
                    "human".to_string()
                ),
            ]
        );
        // labelling stamps the subject's labels position
        assert_eq!(
            w.whole(&jl)
                .unwrap()
                .unwrap()
                .held_parts()
                .unwrap()
                .writes
                .get("labels"),
            w.whole(&l2)
                .unwrap()
                .unwrap()
                .held_parts()
                .unwrap()
                .writes
                .get("name")
        );
        drop(w);
        let backdated = {
            let r = Request {
                transition: "record".into(),
                actor: OPERATOR.into(),
                object_id: None,
                type_: Some(LABEL.into()),
                inputs: [
                    ("subject".to_string(), json!(jl)),
                    ("name".to_string(), json!("x")),
                ]
                .into(),
                occurred_at: Some(s.now - 60),
                propose: false,
                expires_at: None,
                expected_version: None,
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            };
            match request(&mut s, &r).unwrap().unwrap() {
                Verdict::Unsatisfied { clause, .. } => Err(("unsatisfied".to_string(), clause)),
                other => Err((other.kind().to_string(), String::new())),
            }
        };
        for x in ["start", "shelve", "close"] {
            take(&mut s, x, &jm);
        }
        let refusal = |verdict: &str, clause: &str| {
            Err::<String, _>((verdict.to_string(), clause.to_string()))
        };
        assert_eq!(
            [
                backdated,
                label(
                    &mut s,
                    json!({ "subject": jl, "name": "x", "subject_state": "DONE" })
                ),
                label(&mut s, json!({ "subject": "nothing", "name": "x" })),
                label(
                    &mut s,
                    json!({ "subject": jm, "name": "x", "corrects": l3 })
                ),
                label(
                    &mut s,
                    json!({ "subject": jl, "name": "x", "corrects": l4 })
                ),
                label(&mut s, json!({ "subject": jm, "name": "x" })),
            ],
            [
                refusal("unsatisfied", "occurred_within"),
                refusal("invalid_input", ""),
                refusal("invalid_input", ""),
                refusal("unsatisfied", "corrects_current"),
                refusal("invalid_input", ""),
                refusal("unsatisfied", "subject_open"),
            ]
        );
        // a correction on a finished subject applies, keeping the corrected
        // one's occurred time
        let l5 = label(
            &mut s,
            json!({ "subject": jm, "name": "odd-two", "corrects": l4 }),
        )
        .unwrap();
        let w = s.snapshot().unwrap();
        assert_eq!(
            (
                w.whole(&l5).unwrap().unwrap().attr("name"),
                w.whole(&l5).unwrap().unwrap().attr("occurred_at")
            ),
            (
                text("odd-two"),
                w.whole(&l4).unwrap().unwrap().attr("occurred_at")
            )
        );
        // labelling offered on a finished subject is unavailable
        let offered: Vec<_> = crate::reads::availability(&w, OPERATOR, &jm)
            .unwrap()
            .unwrap()
            .into_iter()
            .filter(|o| o.transition == "record" && o.creates.as_deref() == Some(LABEL))
            .map(|o| (o.availability, o.verdict.and_then(|v| v.clause())))
            .collect();
        assert_eq!(
            offered,
            [(
                crate::reads::Availability::Unavailable,
                Some("subject_open".to_string())
            )]
        );
        drop(w);
        // a label on an object of another type is none of the family's
        label(&mut s, json!({ "subject": ana, "name": "late-risk" })).unwrap();
        // a metric over labels counts each label nothing has corrected
        assert_eq!(
            [
                read(&s, &jl, "metric(Job.labels_total)", &[]),
                read(&s, &jl, r#"metric(Job.labelled, name := "late-risk")"#, &[]),
                read(
                    &s,
                    &jl,
                    r#"metric(Job.labelled, name := "late-risk", state := "OPEN")"#,
                    &[]
                ),
                read(
                    &s,
                    &jl,
                    "metric(Job.labelled, engineer := who)",
                    &[("who", Value::Object(ana.clone()))]
                ),
                read(&s, &jl, r#"metric(Job.labelled, name := "odd-two")"#, &[]),
            ],
            [
                Value::Int(3),
                Value::Int(2),
                Value::Int(1),
                Value::Int(2),
                Value::Int(1)
            ]
        );
        // a filter on a label's name reads the labels holding it alone, and a
        // correction carrying another name still corrects one
        assert_eq!(
            [
                read(&s, &jl, "metric(Job.risky)", &[]),
                read(&s, &jl, "metric(Job.odd_named)", &[]),
            ],
            [Value::Int(2), Value::Int(0)]
        );
        let names = |w: &World| -> Vec<Value> {
            w.held()
                .filter(|o| o.type_ == LABEL)
                .map(|o| o.attr("name"))
                .collect()
        };
        let w = s.snapshot().unwrap();
        page(&w, None, "Job.risky", &BTreeMap::new(), None, None, None).unwrap();
        assert_eq!(names(&w), vec![text("late-risk"); 3]);
        // and a reader's filter likewise
        let w = s.snapshot().unwrap();
        let read_by = page(
            &w,
            None,
            "Job.labels_total",
            &BTreeMap::new(),
            None,
            None,
            Some(r#"l.name == "odd-two""#),
        )
        .unwrap();
        assert_eq!(read_by.rows[0].value, Value::Int(1));
        assert_eq!(names(&w), [text("odd-two")]);
        // a correction the operation holds, and has not stored, corrects
        let mut w = s.snapshot().unwrap();
        let mut made = w.whole(&l2).unwrap().unwrap().clone();
        made.id = "label-made".into();
        made.order = w.next_order().unwrap();
        made.attrs.insert("name".into(), text("on-time"));
        made.attrs
            .insert("corrects".into(), Value::Object(l1.clone()));
        w.insert(made).unwrap();
        let held_correction =
            page(&w, None, "Job.risky", &BTreeMap::new(), None, None, None).unwrap();
        assert_eq!(held_correction.rows[0].value, Value::Int(1));
        // a reader's filter on a label's note, which is personal, is refused
        assert_eq!(
            page_of(
                &s,
                "Job.labelled",
                &[],
                None,
                None,
                Some(r#"l.note == "x""#)
            )
            .refused,
            Some("personal")
        );
    }

    /// An override's reason, an input of its own, is recorded on its event,
    /// so the standard `override_counts` counts overrides by reason
    /// (`flow-format.md` §4.8, `metric-scenarios.md` UC-3).
    #[test]
    fn an_override_is_counted_by_its_reason() {
        let mut s = world();
        let j = job(&mut s, json!({}));
        req(
            &mut s,
            "Job",
            "override",
            Some(&j),
            json!({"to": "WORKING", "reason": "MISKEYED"}),
        )
        .unwrap();
        let p = page_of(
            &s,
            "Job.override_counts",
            &[],
            Some(&["state", "reason"]),
            None,
            None,
        );
        let rows: Vec<_> = p
            .rows
            .iter()
            .map(|r| (r.dimensions.clone(), r.value.clone()))
            .collect();
        let working = Value::State {
            type_: "Job".into(),
            name: "WORKING".into(),
        };
        let miskeyed = Value::Member {
            enumeration: "Why".into(),
            member: "MISKEYED".into(),
        };
        assert_eq!(
            rows,
            vec![(
                dims(&[("state", working), ("reason", miskeyed)]),
                Value::Int(1)
            )]
        );
    }

    /// `DESIGN.md` §6 and §7, ADR-0088, ADR-0158: an event records the read
    /// set of the rules evaluated for it and the values they consulted. A
    /// scan records, by guard and type, what satisfied its `where`, an empty
    /// match empty, and not what it examined and rejected; a collection's
    /// elements are read; the event's own object is left to its order; and a
    /// metric is consulted, by guard and reference, though the `or`'s other
    /// side decides. An invariant's reads go on its object's event. A
    /// refusal's row holds everything read and what its clause consulted, an
    /// audit's what its clause read.
    #[test]
    fn an_event_records_what_its_rules_read_and_consulted() {
        let text = JOBS
            .replace(
                "      checked: { description: Checked., expression: count(c in checks) > 0, remedy: self_serviceable }",
                "      checked: { description: Checked., expression: count(c in checks) > 0, remedy: self_serviceable }
      tagged_or_good: { description: x, expression: \"count(t in tags) >= 1 or metric(pass_rate, engineer := engineer, over last 30 days) >= 0.900\", remedy: dependent }
      ana_known: { description: x, expression: 'any(u in User where u.login == \"ana\")', remedy: dependent }
      no_carl: { description: x, expression: 'none(u in User where u.login == \"carl\")', remedy: dependent }
      login_unknown: { description: x, expression: engineer_login is null, remedy: dependent }",
            )
            .replace(
                "      review: { kind: internal, from: [WORKING], guards: { checked: warn } }",
                "      review: { kind: internal, from: [WORKING], guards: { checked: warn } }
      wrap: { kind: internal, from: [WORKING], guards: { checked: deny, tagged_or_good: deny, ana_known: deny, no_carl: deny } }
      vet: { kind: internal, from: [WORKING], guards: { login_unknown: audit } }",
            )
            .replace(
                "    transitions:\n      open: { kind: initial, to: OPEN, optional_inputs: [engineer, tags] }",
                "    derived_attributes:\n      engineer_login: { expression: engineer.login }\n    invariants:\n      engineer_named: { expression: engineer is null or engineer.login is not null }\n    transitions:\n      open: { kind: initial, to: OPEN, optional_inputs: [engineer, tags] }",
            );
        assert_eq!(text.matches("wrap:").count(), 1);
        assert_eq!(text.matches("engineer_named").count(), 1);
        assert_eq!(text.matches("no_carl").count(), 2);
        let lib = Library::load(&[("jobs.yaml".into(), text)]).unwrap();
        let mut s = Store::sandbox(lib, parse_timestamp("2026-01-01T00:00:00Z").unwrap()).unwrap();
        let (ana, ben) = (user(&mut s, "ana"), user(&mut s, "ben"));
        let tagged = job(&mut s, json!({ "engineer": ana, "tags": ["urgent"] }));
        let plain = job(&mut s, json!({ "engineer": ana }));
        record(&mut s, &tagged, &["PASS", "FAIL"]);
        for j in [&tagged, &plain] {
            take(&mut s, "start", j);
        }
        let checks: Vec<String> = s
            .snapshot()
            .unwrap()
            .of_type("Check")
            .unwrap()
            .iter()
            .map(|c| c.id.clone())
            .collect();
        assert_eq!(checks.len(), 2);
        let last = |s: &Store, j: &str, xn: &str| {
            s.history(j)
                .unwrap()
                .into_iter()
                .rfind(|e| e.transition == xn)
                .unwrap()
        };
        // the tag decides, and the rate is consulted all the same: ana's
        // checks, one pass in two
        assert_eq!(
            req(&mut s, "Job", "wrap", Some(&tagged), json!({})),
            Ok(tagged.clone())
        );
        let e = last(&s, &tagged, "wrap");
        let reference = format!("pass_rate(engineer: {ana})");
        assert_eq!(
            e.payload["consulted"]["tagged_or_good"]["values"],
            json!({ reference.clone(): "0.500" })
        );
        assert_eq!(
            e.payload["consulted"]["tagged_or_good"]["as_of"],
            json!("2026-01-01T00:00:00Z")
        );
        // ana matched, ben examined and rejected, carl matched by none; the
        // checks read as the collection's elements; the job's own entry left
        // to its order
        let mut read: Vec<&String> = e.reads["objects"].as_object().unwrap().keys().collect();
        read.sort();
        let mut want = vec![&ana, &checks[0], &checks[1]];
        want.sort();
        assert_eq!(read, want);
        assert!(!read.contains(&&ben) && !read.contains(&&tagged));
        assert_eq!(e.reads["scans"]["ana_known"], json!({ "User": [ana] }));
        assert_eq!(e.reads["scans"]["no_carl"], json!({ "User": [] }));
        assert_eq!(e.reads["now"], json!("2026-01-01T00:00:00Z"));
        // each read with the version it read
        let w = s.snapshot().unwrap();
        assert_eq!(
            e.reads["objects"][&ana],
            json!(w.whole(&ana).unwrap().unwrap().version)
        );
        drop(w);
        // an untagged job is refused on the rate; its row holds what the
        // request read, its own job included, and what its clause consulted
        record(&mut s, &plain, &["PASS"]);
        assert_eq!(
            req(&mut s, "Job", "wrap", Some(&plain), json!({})),
            Err(("unsatisfied".into(), "tagged_or_good".into()))
        );
        let w = s.snapshot().unwrap();
        let row = w.attempts().unwrap().last().unwrap();
        assert_eq!(row.clause.as_deref(), Some("tagged_or_good"));
        assert_eq!(
            row.consulted["tagged_or_good"]["values"],
            json!({ reference.clone(): "0.667" })
        );
        assert!(row.reads["objects"].get(&plain).is_some());
        drop(w);
        // a clause reading a derived attribute reads what its expression
        // reads: the audit's row holds the engineer
        assert_eq!(
            req(&mut s, "Job", "vet", Some(&tagged), json!({})),
            Ok(tagged.clone())
        );
        let w = s.snapshot().unwrap();
        let row = w.attempts().unwrap().last().unwrap();
        assert_eq!(row.clause.as_deref(), Some("login_unknown"));
        assert!(row.reads["objects"].get(&ana).is_some());
        drop(w);
        // an audit's row holds what its clause read
        let bare = job(&mut s, json!({}));
        take(&mut s, "start", &bare);
        assert_eq!(
            req(&mut s, "Job", "inspect", Some(&bare), json!({})),
            Ok(bare.clone())
        );
        let w = s.snapshot().unwrap();
        let row = w.attempts().unwrap().last().unwrap();
        assert_eq!(row.clause.as_deref(), Some("checked"));
        assert_eq!(
            row.reads["objects"],
            json!({ bare.clone(): w.whole(&bare).unwrap().unwrap().version - 1 })
        );
        assert_eq!(row.consulted, Json::Null);
        drop(w);
        // the invariant's read of the engineer goes on the job's event
        let free = job(&mut s, json!({}));
        assert_eq!(
            req(
                &mut s,
                "Job",
                "take",
                Some(&free),
                json!({ "engineer": ben })
            ),
            Ok(free.clone())
        );
        let taken = s
            .snapshot()
            .unwrap()
            .log()
            .unwrap()
            .rfind(|e| e.transition == "take")
            .unwrap()
            .clone();
        assert!(taken.reads["objects"].get(&ben).is_some());
        assert!(taken.reads["objects"].get(&taken.object_id).is_none());
    }

    /// `maintain` (`DESIGN.md` §10, ADR-0101, ADR-0103): a prune inside the
    /// store's retention, or of a store built with none, is refused; pruned
    /// attempt rows leave the log and keep counting through the rollup, a
    /// second prune of the same day adding to its count; a pruned key's
    /// retry is a new request; and a task it does not know is refused.
    #[test]
    fn maintain_prunes_outside_the_retention_and_the_counts_survive() {
        use crate::store::MaintenanceTask;
        let mut s = world();
        let j = job(&mut s, json!({}));
        let refuse = |s: &mut Store| {
            assert_eq!(
                req(s, "Job", "close", Some(&j), json!({})),
                Err(("unavailable".into(), String::new()))
            );
        };
        // by transition and week, so a pruned row keeps its day
        let refusals = |s: &Store| -> Vec<(Value, Value, Value)> {
            let w = s.snapshot().unwrap();
            let keep = ["transition".to_string(), "week".to_string()];
            page(
                &w,
                None,
                "Job.refusals",
                &BTreeMap::new(),
                Some(&keep),
                None,
                None,
            )
            .unwrap()
            .rows
            .into_iter()
            .map(|r| {
                (
                    r.dimensions[0].1.clone(),
                    r.dimensions[1].1.clone(),
                    r.value,
                )
            })
            .collect()
        };
        let task = |name: &str, before: i64| MaintenanceTask {
            name: name.into(),
            before,
        };
        // two refusals two hours apart, and another type's, which the job's
        // count never includes
        refuse(&mut s);
        assert_eq!(
            req(&mut s, "User", "add", None, json!({})).map(|_| ()),
            Err(("invalid_input".into(), String::new()))
        );
        let first = s.now;
        s.now += 2 * 3600;
        refuse(&mut s);
        let counted = refusals(&s);
        assert_eq!(
            counted,
            [(
                Value::Text("close".into()),
                Value::Text("2026-W01".into()),
                Value::Int(2)
            )]
        );
        // a store built with no retention prunes nothing, and says so
        assert_eq!(
            s.maintain(crate::model::OPERATOR, &task("prune_attempts", first))
                .unwrap()
                .unwrap(),
            Err("no_retention")
        );
        s.attempt_retention = Some(30 * DAY);
        s.now += 40 * DAY;
        // inside the retention, refused
        assert_eq!(
            s.maintain(
                crate::model::OPERATOR,
                &task("prune_attempts", s.now - 10 * DAY)
            )
            .unwrap()
            .unwrap(),
            Err("inside_retention")
        );
        // the first row pruned, then the second, into the same day's count
        assert_eq!(
            s.maintain(crate::model::OPERATOR, &task("prune_attempts", first + 1))
                .unwrap()
                .unwrap(),
            Ok(2)
        );
        assert_eq!(s.snapshot().unwrap().attempts().unwrap().len(), 1);
        assert_eq!(refusals(&s), counted);
        assert_eq!(
            s.maintain(
                crate::model::OPERATOR,
                &task("prune_attempts", first + 3 * 3600)
            )
            .unwrap()
            .unwrap(),
            Ok(1)
        );
        assert!(s.snapshot().unwrap().attempts().unwrap().is_empty());
        assert_eq!(refusals(&s), counted);
        // a pruned key's retry is a new request
        let keyed = Request {
            transition: "open".into(),
            actor: OPERATOR.into(),
            object_id: None,
            type_: Some("Job".into()),
            inputs: BTreeMap::new(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            target_version: None,
            principal: None,
            idempotency_key: Some("k".into()),
            context: None,
        };
        let opened = |s: &mut Store| match request(s, &keyed).unwrap().unwrap() {
            Verdict::Satisfied {
                object_id,
                replayed,
                ..
            } => (object_id, replayed),
            other => panic!("{other:?}"),
        };
        let (once, _) = opened(&mut s);
        assert_eq!(opened(&mut s), (once.clone(), true));
        s.idempotency_retention = Some(DAY);
        s.now += 2 * DAY;
        assert_eq!(
            s.maintain(
                crate::model::OPERATOR,
                &task("prune_idempotency", s.now - DAY)
            )
            .unwrap()
            .unwrap(),
            Ok(1)
        );
        let (again, replayed) = opened(&mut s);
        assert!(!replayed && again != once);
        // a task it does not know, and one it does not run
        assert_eq!(
            s.maintain(crate::model::OPERATOR, &task("prune_everything", 0))
                .unwrap()
                .unwrap(),
            Err("unknown_task")
        );
        assert!(
            s.maintain(crate::model::OPERATOR, &task("archive_events", 0))
                .unwrap()
                .is_err()
        );
        // an actor the store does not know prunes nothing, past the
        // retention as the task is (production review MAINTAIN)
        assert_eq!(
            s.maintain("alice", &task("prune_idempotency", s.now - 10 * DAY))
                .unwrap()
                .unwrap(),
            Err("actor_known")
        );
    }
}

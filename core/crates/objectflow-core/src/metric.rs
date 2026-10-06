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
    Err(NotRun(what.into()))
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
    parse(text).map_err(|e| NotRun(format!("the metric expression {text:?}: {e}")))
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
                .ok_or_else(|| NotRun("a metric's value past 128 bits".into()))
        };
        match v {
            Value::Absent => Ok(Num::Absent),
            Value::Int(n) => Ok(Num::Int(i128::from(*n))),
            Value::Decimal(d) => frac(Rat::of_decimal(d)),
            Value::Money { amount, .. } => frac(Rat::of_decimal(amount)),
            Value::Duration(s) | Value::Timestamp(s) => Ok(Num::Frac(Rat::int(i128::from(*s)))),
            // a date placed at the start of its day, UTC (ADR-0145)
            Value::Date(d) => Ok(Num::Frac(Rat::int(i128::from(*d) * 86_400))),
            other => not_run(format!("a metric's arithmetic over {other:?}")),
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
        let past = || NotRun("a metric's value past 128 bits".into());
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
}

/// The read of one metric over a store as the request found it.
struct Reader<'w> {
    world: &'w World<'w>,
    answers: Option<&'w BTreeMap<String, String>>,
}

/// What a metric's rows are: each an object, a span or an event of a type,
/// and the member that names the kind of actor behind one.
struct Rows {
    rows: Vec<Value>,
    kind: Kind,
    by_kind: &'static str,
}

impl<'w> Reader<'w> {
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
            .ok_or_else(|| NotRun(format!("the type {name}")))
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
        let Rows {
            mut rows,
            kind: rowkind,
            by_kind,
        } = self.rows(owner, owner_t, &source)?;
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
        let filter = text("filter").map(|t| parsed(&t)).transpose()?;
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
            // a reader's filter narrows the rows likewise (ADR-0114)
            if let Some(w) = theirs
                && truth(&evaluator.eval_in(w, &env)?)? != Some(true)
            {
                continue;
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
        })
    }

    /// A metric's rows (`declaration-syntax.md` §6.9): each object of the
    /// type or its family, each span of its state or of a tracked member,
    /// each event of a transition, or each observation of one of its kinds
    /// that nothing has corrected.
    fn rows(&self, owner: &str, t: &Type, source: &str) -> Result<Rows, NotRun> {
        let world = self.world;
        let family = world.of_type(owner);
        if source == "objects" {
            return Ok(Rows {
                rows: family.iter().map(|o| Value::Object(o.id.clone())).collect(),
                kind: Kind::Object(owner.to_string()),
                by_kind: "created_by_kind",
            });
        }
        if source == "intervals" || source.starts_with("intervals(") {
            let member = source
                .strip_prefix("intervals(")
                .and_then(|m| m.strip_suffix(')'))
                .map_or("state", str::trim);
            let mut rows = Vec::new();
            for o in &family {
                let ot = self.type_of(&o.type_)?;
                rows.extend(interval_rows(world, o, ot, member)?);
            }
            return Ok(Rows {
                rows,
                kind: Kind::Interval(owner.to_string()),
                by_kind: "entered_by_kind",
            });
        }
        if source == "transitions" {
            let mut rows = Vec::new();
            for e in &world.events {
                if let Some(o) = family.iter().find(|o| o.id == e.object_id) {
                    rows.push(transition_row(world, o, self.type_of(&o.type_)?, e));
                }
            }
            return Ok(Rows {
                rows,
                kind: Kind::Transition(owner.to_string()),
                by_kind: "actor_kind",
            });
        }
        if let Some((_, kind)) = t.observations.iter().find(|(c, _)| c == source) {
            let ids: Vec<Value> = family.iter().map(|o| Value::Object(o.id.clone())).collect();
            let of_kind = world.of_type(kind);
            let corrected: Vec<Value> = of_kind.iter().map(|x| x.attr("corrects")).collect();
            return Ok(Rows {
                rows: of_kind
                    .iter()
                    .filter(|x| ids.contains(&x.attr("subject")))
                    .filter(|x| !corrected.contains(&Value::Object(x.id.clone())))
                    .map(|x| Value::Object(x.id.clone()))
                    .collect(),
                kind: Kind::Object(kind.clone()),
                by_kind: "recorded_by_kind",
            });
        }
        if source == "labels" {
            // each label on an object of the family that nothing has
            // corrected (declaration-syntax.md §6.9)
            let ids: Vec<Value> = family.iter().map(|o| Value::Object(o.id.clone())).collect();
            let labels = world.of_type(crate::model::LABEL);
            let corrected: Vec<Value> = labels.iter().map(|x| x.attr("corrects")).collect();
            return Ok(Rows {
                rows: labels
                    .iter()
                    .filter(|x| ids.contains(&x.attr("subject")))
                    .filter(|x| !corrected.contains(&Value::Object(x.id.clone())))
                    .map(|x| Value::Object(x.id.clone()))
                    .collect(),
                kind: Kind::Label(owner.to_string()),
                by_kind: "recorded_by_kind",
            });
        }
        if source == "attempts" || source == "attempt_counts" {
            // the rows of the family's types; a row of no type, an unknown
            // id's, is none of them (DESIGN.md §7)
            let mine: Vec<Value> = world
                .attempts
                .iter()
                .filter(|a| a.type_.as_deref().is_some_and(|t| world.lib.is_a(t, owner)))
                .map(attempt_row)
                .collect();
            if source == "attempts" {
                return Ok(Rows {
                    rows: mine,
                    kind: Kind::Attempt(owner.to_string()),
                    by_kind: "actor_kind",
                });
            }
            // each UTC day, object and combination of the members, counted;
            // nothing is pruned, so the rollup holds nothing
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
            let mut counts: Vec<(Vec<Value>, i64)> = Vec::new();
            for row in &mine {
                let Value::Row(f) = row else { continue };
                let day = match f.get("at") {
                    Some(Value::Timestamp(t)) => Value::Date(t.div_euclid(86_400)),
                    _ => Value::Absent,
                };
                let key: Vec<Value> = std::iter::once(day)
                    .chain(
                        MEMBERS
                            .iter()
                            .map(|m| f.get(*m).cloned().unwrap_or(Value::Absent)),
                    )
                    .collect();
                match counts.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, n)) => *n += 1,
                    None => counts.push((key, 1)),
                }
            }
            return Ok(Rows {
                rows: counts
                    .into_iter()
                    .map(|(key, n)| {
                        let mut f: BTreeMap<String, Value> = std::iter::once("day")
                            .chain(MEMBERS)
                            .map(str::to_string)
                            .zip(key)
                            .collect();
                        f.insert("count".into(), Value::Int(n));
                        Value::Row(f)
                    })
                    .collect(),
                kind: Kind::AttemptCount(owner.to_string()),
                by_kind: "actor_kind",
            });
        }
        not_run(format!("a metric over `{source}`"))
    }

    /// Each span of a type's family cut at the bucket boundaries it covers,
    /// each piece with its period, its bounds and its duration (ADR-0142).
    fn pieces(&self, owner: &str, member: &str, unit: &str) -> Result<Vec<Value>, NotRun> {
        let mut out = Vec::new();
        for o in self.world.of_type(owner) {
            let t = self.type_of(&o.type_)?;
            let spans = o.spans.get(member).map_or(&[][..], Vec::as_slice);
            let rows = interval_rows(self.world, o, t, member)?;
            for (sp, row) in spans.iter().zip(rows) {
                out.extend(pieces(self.world, o, t, member, sp, row, unit));
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
                                .ok_or_else(|| NotRun("a metric's value past 128 bits".into()))?;
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
                            .ok_or_else(|| NotRun("a metric's value past 128 bits".into()))?
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
                .ok_or_else(|| NotRun(format!("the input metric {n}"))),
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
                    return self.kind(e, &BTreeMap::new(), Some(t));
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
            if op == BinOp::Sub && a == Kind::Timestamp && b == Kind::Timestamp {
                return Kind::Duration;
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
    let past = || NotRun("a metric's value past its type's range".into());
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
    o: &Object,
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
        let mut f = fields.clone();
        f.insert("period".into(), Value::Text(period));
        f.insert("piece_start".into(), from);
        f.insert("piece_end".into(), to);
        f.insert("duration".into(), duration);
        Value::Row(f)
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
            closed_periods(o, t, world.now),
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
    let Value::Row(mut f) = piece else {
        return None;
    };
    let time = |v: Option<&Value>| match v {
        Some(Value::Timestamp(t)) => Some(*t),
        _ => None,
    };
    let start = time(f.get("piece_start"))?;
    match f.get("duration") {
        Some(Value::Duration(d)) if *d != 0 => {}
        _ => return (low <= start && start <= high).then_some(Value::Row(f)),
    }
    let end = time(f.get("piece_end"))?;
    let (a, b) = (start.max(low), end.min(high));
    if b <= a {
        return None;
    }
    f.insert("piece_start".into(), Value::Timestamp(a));
    f.insert("piece_end".into(), Value::Timestamp(b));
    f.insert("duration".into(), Value::Duration(b - a));
    Some(Value::Row(f))
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
    let reader = Reader { world, answers };
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
                .filter_map(|r| self.gap(r, &sel, window_start))
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
    fn gap(&self, r: &Value, sel: &Selected<'w>, window_start: Option<i64>) -> Option<String> {
        let (id, since) = match r {
            Value::Object(id) => (id.clone(), None),
            Value::Row(f) => match (
                f.get("object"),
                f.get("entered_at").or_else(|| f.get("occurred_at")),
            ) {
                (Some(Value::Object(id)), Some(Value::Timestamp(t))) => (id.clone(), Some(*t)),
                (Some(Value::Object(id)), _) => (id.clone(), None),
                _ => return None,
            },
            _ => return None,
        };
        let o = self.world.get(&id)?;
        let before = |t: i64| t < o.recorded_from;
        let gapped = since.is_some_and(before)
            || window_start.is_some_and(|w| before(w) && o.created_at < w)
            || (sel.reads_created && !o.created_dated)
            || (matches!(r, Value::Object(_))
                && sel.reads_flow
                && o.spans.values().flatten().any(|sp| before(sp.entered_at)));
        gapped.then_some(id)
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
            if b.is_absent() {
                return Ok(Value::Absent);
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
        fn walk(r: &Reader<'_>, e: &Expr, env: &BTreeMap<String, Kind>) -> Result<bool, NotRun> {
            if let Expr::Path(base, m) = e
                && let Kind::Object(tn) = r.kind(base, env, None)?
                && r.world
                    .lib
                    .type_(&tn)
                    .and_then(|t| t.attribute(m))
                    .is_some_and(|a| a.personal)
            {
                return Ok(true);
            }
            for c in crate::check::children_of(e) {
                if walk(r, c, env)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        walk(self, filter, &env)
    }

    /// A page's order between two keys: by each kept dimension ascending, an
    /// absent value last, a state, an enumeration member or a kind of actor
    /// in declaration order, an object by id, which is creation order, a time
    /// bucket in time order and a string by code point (ADR-0127, ADR-0147).
    fn order(&self, keep: &[String], a: &[Value], b: &[Value]) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        const KINDS: [&str; 4] = ["human", "agent", "service", "unknown"];
        let lib = self.world.lib;
        let rank = |d: &str, v: &Value| -> (u8, Option<i128>, Option<Rat>, String) {
            match v {
                Value::Absent => (1, None, None, String::new()),
                Value::State { type_, name } => {
                    let at = lib
                        .type_(type_)
                        .and_then(|t| t.states.iter().position(|(s, _)| s == name));
                    (
                        0,
                        at.and_then(|i| i128::try_from(i).ok()),
                        None,
                        String::new(),
                    )
                }
                Value::Member {
                    enumeration,
                    member,
                } => {
                    let at = lib
                        .enumerations
                        .get(enumeration)
                        .and_then(|ms| ms.iter().position(|m| m == member));
                    (
                        0,
                        at.and_then(|i| i128::try_from(i).ok()),
                        None,
                        String::new(),
                    )
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
                Value::Object(id) => (
                    0,
                    self.world.get(id).map(|o| i128::from(o.order)),
                    None,
                    id.clone(),
                ),
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
    let refused = |why: &'static str| {
        Ok(Page {
            refused: Some(why),
            history_complete: true,
            ..Page::default()
        })
    };
    let reader = Reader { world, answers };
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
    let theirs = filter.map(parsed).transpose()?;
    if let Some(f) = &theirs
        && reader.reads_personal(owner.unwrap_or_default(), &found.spec, f)?
    {
        return refused("personal");
    }
    let bound: Bound = bind
        .iter()
        .map(|(d, v)| (d.clone(), Want::Wire(v.clone())))
        .collect();
    let (mut groups, kind) = reader.grouped(
        owner,
        &found.spec,
        &found.module,
        &bound,
        &keep,
        over,
        theirs.as_ref(),
    )?;
    groups.sort_by(|a, b| reader.order(&keep, &a.0, &b.0));
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
        let (mut every, _) = reader.grouped(
            owner,
            &found.spec,
            &found.module,
            &bound,
            &declared,
            over,
            theirs.as_ref(),
        )?;
        every.sort_by(|a, b| reader.order(&declared, &a.0, &b.0));
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
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
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
            principal: None,
        };
        match request(s, &r).unwrap().unwrap() {
            Verdict::Satisfied { object_id, .. } => Ok(object_id),
            Verdict::Unsatisfied { clause, .. } => Err(("unsatisfied".into(), clause)),
            other => Err((other.kind().into(), String::new())),
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
        let w = s.snapshot().unwrap();
        let bind: BTreeMap<String, Json> = bind
            .iter()
            .map(|(d, v)| (d.to_string(), crate::wire::encode(v)))
            .collect();
        let keep: Option<Vec<String>> = keep.map(|k| k.iter().map(|d| d.to_string()).collect());
        page(&w, None, name, &bind, keep.as_deref(), over, filter).unwrap()
    }

    fn dims(pairs: &[(&str, Value)]) -> Vec<(String, Value)> {
        pairs
            .iter()
            .map(|(d, v)| (d.to_string(), v.clone()))
            .collect()
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
        assert_eq!(
            rows_of(&page_of(
                &s,
                "Job.pass_rate",
                &[("engineer", b.clone())],
                Some(&[]),
                None,
                None
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
        };
        let report = import_batch(&mut s, OPERATOR, &batch, &[], false)
            .unwrap()
            .unwrap();
        assert!(report.refusal.is_none(), "{report:?}");
        let w = s.snapshot().unwrap();
        let id = |key: &str| w.external[&("legacy".to_string(), format!("Unit:{key}"))].clone();
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
            principal: None,
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
            .attempts
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
                    a.request_values.get("proposal").is_some(),
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
        assert!(w.attempts[..2].iter().all(|a| a.applied_position.is_some()));
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
            let o = w.get(id).unwrap();
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
            w.get(&jl).unwrap().writes.get("labels"),
            w.get(&l2).unwrap().writes.get("name")
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
                principal: None,
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
                w.get(&l5).unwrap().attr("name"),
                w.get(&l5).unwrap().attr("occurred_at")
            ),
            (text("odd-two"), w.get(&l4).unwrap().attr("occurred_at"))
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
}

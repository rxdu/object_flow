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

use std::collections::BTreeMap;

use serde_json::{Value as Json, json};

use crate::eval::{Evaluator, NotRun, closed_periods, interval_rows, transition_row, truth};
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
            let module = module.map(str::to_string).or_else(|| tn.and_then(home));
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
        bound: &BTreeMap<String, Value>,
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
                let narrowed: BTreeMap<String, Value> = bound
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
        let owner = owner.unwrap_or_default();
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
                    Reach::One(got) => got.equals(v),
                    Reach::Many(got) => got.iter().any(|g| g.equals(v)),
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
        let env = BTreeMap::from([(item.clone(), rowkind)]);
        let kind = self.kind(&tree, &env, Some(owner_t))?;
        let value = self.over_rows(&evaluator, &tree, &picked, &item)?;
        Ok((value, kind))
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
        if source == "attempts" || source == "attempt_counts" {
            return not_run(format!(
                "a metric over `{source}`, the attempt log the core does not keep"
            ));
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
    let (v, kind) = reader.read(
        found.owner.as_deref(),
        &found.spec,
        &found.module,
        bound,
        window,
    )?;
    rounded(v, &kind)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
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
      finish: { kind: external, from: [WORKING], to: DONE, guards: { good: deny } }
    metrics:
      pass_rate: { description: x, source: checks, item: r, filter: r.outcome != Outcome.NA, dimensions: { engineer: r.subject.engineer }, time_dimension: r.occurred_at, expression: count(where r.outcome == Outcome.PASS) * 1.000 / count() }
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
}

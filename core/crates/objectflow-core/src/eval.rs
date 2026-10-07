//! The expression language's meaning (`declaration-syntax.md` §8, §9.2), over
//! a [`World`], three-valued: a value, or unknown, which is absent (§8.2).
//!
//! It follows the reference runner's evaluator case for case
//! (`scripts/flowrun.py`, `Evaluator`), as the replay of the design's examples
//! holds it to (ADR-0153 decision 6). A construct the first slice does not run
//! is answered as [`NotRun`], naming it, and never guessed.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use crate::expr::{BinOp, Expr, Number};
use crate::model::{Declared, Type};
use crate::value::{Decimal, Value, minor_unit, unit_seconds};
use crate::world::{Object, World};

/// Why the core did not run what it was asked: a construct the first slice
/// does not run (ADR-0153 decision 3), or a read of the store that failed,
/// which is unavailable storage, never a construct not run (`library-api.md`
/// §7, ADR-0164).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotRun {
    /// A construct the core does not run, named.
    Unsupported(String),
    /// A read of the store that failed, naming what it read and why.
    Unreadable(String),
}

impl NotRun {
    /// What was not run, or what could not be read.
    pub fn what(&self) -> &str {
        match self {
            NotRun::Unsupported(w) | NotRun::Unreadable(w) => w,
        }
    }
}

impl fmt::Display for NotRun {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.what())
    }
}

impl std::error::Error for NotRun {}

fn not_run<T>(what: impl Into<String>) -> Result<T, NotRun> {
    Err(NotRun::Unsupported(what.into()))
}

/// What an expression's subterm evaluates to: a value, or one of the names a
/// path continues from.
#[derive(Clone, Debug, PartialEq)]
enum Term {
    Value(Value),
    /// `inputs`, whose members are the request's inputs.
    Inputs,
    /// `actor`, whose members are who acts.
    Actor,
    /// A type or an enumeration named in an expression.
    TypeRef(String),
    /// A state category.
    Category(String),
}

impl Term {
    fn absent() -> Term {
        Term::Value(Value::Absent)
    }
}

type Evaluated = Result<Term, NotRun>;

/// Who acts, as a rule may read it in an outcome only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Who {
    pub id: String,
    pub kind: String,
    /// The principal it acts for, by its identity, where the request names
    /// one (`DESIGN.md` §5.8).
    pub principal: Option<String>,
}

/// An evaluation of expressions for one object, or none, and one request.
#[derive(Debug)]
pub struct Evaluator<'a> {
    world: &'a World<'a>,
    /// The object `this` names, if any.
    this: Option<&'a str>,
    /// The type whose names a bare name resolves against.
    type_: Option<&'a Type>,
    inputs: &'a BTreeMap<String, Value>,
    actor: Option<&'a Who>,
    /// In a filter, an element whose filter is unknown is left out (§8.2).
    filter_mode: bool,
    /// The position of the event an effect belongs to, `this_event`.
    event: Option<i64>,
    /// The answers the store's evaluators give, `<evaluator>.<function>` to
    /// `satisfied`, `unsatisfied` or `unanswered`: a sandbox's, which its
    /// caller sets (ADR-0150). None for an offer, which asks no evaluator,
    /// every call left unanswered (ADR-0048).
    answers: Option<&'a BTreeMap<String, String>>,
    /// Whether an evaluator it asked did not answer, which makes a refusing
    /// guard's remedy temporal (ADR-0138).
    pub unanswered: std::cell::Cell<bool>,
    /// The store as the request found it, which a metric guard reads
    /// (ADR-0084 §6); none where it is the world itself.
    committed: Option<&'a World<'a>>,
    /// Where a rule's evaluation records what it read and consulted, and
    /// the rule's name, which keys its scans and consulted values (ADR-0088,
    /// ADR-0158); none where nothing records.
    reading: Option<(&'a std::cell::RefCell<Reading>, &'a str)>,
    /// How deep inside a type-scan's `where` it is, where an object it
    /// examines is not yet read: only what the scan matches is (ADR-0158).
    quiet: std::cell::Cell<usize>,
    /// How deep inside a part evaluated for the record alone it is, where an
    /// evaluator the sandbox gives no answer for is set aside rather than
    /// stopping a request its verdict does not decide.
    aside: std::cell::Cell<usize>,
}

/// What the rules evaluated for an event read and consulted (`DESIGN.md`
/// §6, ADR-0088, ADR-0158): each object read, with the version read; each
/// type-scan's matches, by rule and type, an empty match recorded empty;
/// and each value consulted, by rule and the reference with its bindings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    pub objects: BTreeMap<String, i64>,
    pub scans: BTreeMap<String, BTreeMap<String, std::collections::BTreeSet<String>>>,
    pub consulted: BTreeMap<String, BTreeMap<String, Value>>,
}

impl Reading {
    /// The read set as an event or an attempt row records it
    /// (`storage-schema.md` §4): each object read with its version, but the
    /// one given, the event's own, whose version read its order records;
    /// each scan's matches by rule and type; and the `now` the request fixed.
    pub fn record(&self, now: i64, own: Option<&str>) -> serde_json::Value {
        let objects: serde_json::Map<String, serde_json::Value> = self
            .objects
            .iter()
            .filter(|(id, _)| Some(id.as_str()) != own)
            .map(|(id, v)| (id.clone(), serde_json::json!(v)))
            .collect();
        serde_json::json!({
            "objects": objects,
            "scans": self.scans,
            "now": crate::value::timestamp_text(now),
        })
    }

    /// The values it consulted, by rule and the reference with its bindings,
    /// with the as-of time of each rule's (ADR-0158); none where it consulted
    /// nothing.
    pub fn consulted_record(&self, as_of: i64) -> Option<serde_json::Value> {
        if self.consulted.is_empty() {
            return None;
        }
        let mut out = serde_json::Map::new();
        for (rule, refs) in &self.consulted {
            let values: serde_json::Map<String, serde_json::Value> = refs
                .iter()
                .map(|(r, v)| (r.clone(), crate::wire::encode(v)))
                .collect();
            out.insert(
                rule.clone(),
                serde_json::json!({
                    "as_of": crate::value::timestamp_text(as_of),
                    "values": values,
                }),
            );
        }
        Some(serde_json::Value::Object(out))
    }

    /// Another's reads added to these.
    pub fn merge(&mut self, other: Reading) {
        self.objects.extend(other.objects);
        for (rule, types) in other.scans {
            let mine = self.scans.entry(rule).or_default();
            for (t, ids) in types {
                mine.entry(t).or_default().extend(ids);
            }
        }
        for (rule, refs) in other.consulted {
            self.consulted.entry(rule).or_default().extend(refs);
        }
    }
}

/// The truth of a value, three-valued: `None` for unknown.
pub fn truth(v: &Value) -> Result<Option<bool>, NotRun> {
    match v {
        Value::Bool(b) => Ok(Some(*b)),
        Value::Absent => Ok(None),
        other => not_run(format!(
            "a condition whose value is not a boolean ({other:?})"
        )),
    }
}

static NO_INPUTS: BTreeMap<String, Value> = BTreeMap::new();

impl<'a> Evaluator<'a> {
    /// An evaluator for an object of a type, or for a type with no object yet.
    pub fn new(
        world: &'a World<'a>,
        type_: Option<&'a Type>,
        this: Option<&'a str>,
    ) -> Evaluator<'a> {
        Evaluator {
            world,
            this,
            type_,
            inputs: &NO_INPUTS,
            actor: None,
            filter_mode: false,
            event: None,
            answers: None,
            unanswered: std::cell::Cell::new(false),
            committed: None,
            reading: None,
            quiet: std::cell::Cell::new(0),
            aside: std::cell::Cell::new(0),
        }
    }

    /// Recording what a rule's evaluation reads and consults, under the
    /// rule's name (ADR-0088, ADR-0158).
    pub fn with_reading(
        mut self,
        reading: &'a std::cell::RefCell<Reading>,
        rule: &'a str,
    ) -> Evaluator<'a> {
        self.reading = Some((reading, rule));
        self
    }

    /// An object read, with the version it is at, unless a scan is only
    /// examining it.
    fn note(&self, id: &str) -> Result<(), NotRun> {
        if let Some((reading, _)) = self.reading
            && self.quiet.get() == 0
            && let Some(o) = self.world.get(id)?
        {
            reading
                .borrow_mut()
                .objects
                .insert(id.to_string(), o.version);
        }
        Ok(())
    }

    /// A part that does not decide, evaluated for the record alone where a
    /// rule records what it reads: every metric and evaluator a rule names is
    /// consulted whichever side decides (`DESIGN.md` §6, ADR-0158), and what
    /// it read is read. Its verdict, and any evaluator it leaves unanswered,
    /// are set aside, so the rule decides as it would stopping early.
    fn aside(&self, e: &Expr, env: &BTreeMap<String, Value>) -> Result<(), NotRun> {
        if self.reading.is_some() {
            let asked = self.unanswered.get();
            self.aside.set(self.aside.get() + 1);
            let read = self.value(e, env);
            self.aside.set(self.aside.get() - 1);
            read?;
            self.unanswered.set(asked);
        }
        Ok(())
    }

    /// A value a rule consulted, by the reference with its bindings.
    fn consult(&self, reference: String, value: &Value) {
        if let Some((reading, rule)) = self.reading {
            reading
                .borrow_mut()
                .consulted
                .entry(rule.to_string())
                .or_default()
                .insert(reference, value.clone());
        }
    }

    /// The store as the request found it, which a metric guard reads while
    /// the request's working copy changes (ADR-0084 §6).
    pub fn with_committed(mut self, committed: &'a World<'a>) -> Evaluator<'a> {
        self.committed = Some(committed);
        self
    }

    pub fn with_answers(mut self, answers: &'a BTreeMap<String, String>) -> Evaluator<'a> {
        self.answers = Some(answers);
        self
    }

    pub fn with_event(mut self, position: i64) -> Evaluator<'a> {
        self.event = Some(position);
        self
    }

    /// An expression's value with names bound: a loop's item, a creation's
    /// result.
    pub fn eval_in(&self, e: &Expr, env: &BTreeMap<String, Value>) -> Result<Value, NotRun> {
        self.value(e, env)
    }

    pub fn with_inputs(mut self, inputs: &'a BTreeMap<String, Value>) -> Evaluator<'a> {
        self.inputs = inputs;
        self
    }

    pub fn with_actor(mut self, actor: &'a Who) -> Evaluator<'a> {
        self.actor = Some(actor);
        self
    }

    pub fn filtering(mut self) -> Evaluator<'a> {
        self.filter_mode = true;
        self
    }

    /// An expression's value.
    pub fn eval(&self, e: &Expr) -> Result<Value, NotRun> {
        match self.term(e, &BTreeMap::new())? {
            Term::Value(v) => Ok(v),
            Term::Category(c) => Ok(Value::Text(c)),
            other => not_run(format!("an expression whose value is {other:?}")),
        }
    }

    fn value(&self, e: &Expr, env: &BTreeMap<String, Value>) -> Result<Value, NotRun> {
        match self.term(e, env)? {
            Term::Value(v) => Ok(v),
            Term::Category(c) => Ok(Value::Text(format!("category:{c}"))),
            other => not_run(format!("an expression whose value is {other:?}")),
        }
    }

    fn type_named(&self, name: &str) -> Option<&'a Type> {
        self.world.lib.type_(name)
    }

    fn term(&self, e: &Expr, env: &BTreeMap<String, Value>) -> Evaluated {
        Ok(match e {
            Expr::Num(Number::Int(n)) => Term::Value(Value::Int(*n)),
            Expr::Num(Number::Decimal(t)) => match Decimal::parse(t) {
                Some(d) => Term::Value(Value::Decimal(d)),
                None => return not_run(format!("the number {t}")),
            },
            Expr::Str(s) => Term::Value(Value::Text(s.clone())),
            Expr::Bool(b) => Term::Value(Value::Bool(*b)),
            Expr::Dur(n, unit) => {
                let Some(size) = unit_seconds(unit) else {
                    return not_run(format!("the unit {unit}"));
                };
                match n {
                    Number::Int(n) => match n.checked_mul(size) {
                        Some(s) => Term::Value(Value::Duration(s)),
                        None => return not_run("a duration past 64 bits"),
                    },
                    Number::Decimal(t) => {
                        let seconds = Decimal::parse(t)
                            .and_then(|d| d.checked_mul(&Decimal::from_int(size)))
                            .and_then(|d| d.rescale(0))
                            .and_then(|d| d.to_int());
                        match seconds {
                            Some(s) => Term::Value(Value::Duration(s)),
                            None => return not_run(format!("the duration {t} {unit}")),
                        }
                    }
                }
            }
            Expr::Money(currency, amount) => {
                let amount = minor_unit(currency)
                    .and_then(|m| Decimal::parse(amount).and_then(|d| d.rescale(m)));
                match amount {
                    Some(amount) => Term::Value(Value::Money {
                        currency: currency.clone(),
                        amount,
                    }),
                    None => return not_run(format!("an amount in {currency}")),
                }
            }
            Expr::Name(n) => self.name(n, env)?,
            Expr::Path(base, member) => {
                let base = self.term(base, env)?;
                self.member(&base, member)?
            }
            Expr::Set(items) => {
                let mut out = Vec::new();
                for i in items {
                    out.push(self.value(i, env)?);
                }
                Term::Value(Value::Set(out))
            }
            Expr::Not(inner) => {
                let v = truth(&self.value(inner, env)?)?;
                Term::Value(v.map_or(Value::Absent, |b| Value::Bool(!b)))
            }
            Expr::Neg(inner) => Term::Value(match self.value(inner, env)? {
                Value::Absent => Value::Absent,
                Value::Int(n) => n.checked_neg().map_or(Value::Absent, Value::Int),
                Value::Decimal(d) => d.checked_neg().map_or(Value::Absent, Value::Decimal),
                Value::Duration(s) => s.checked_neg().map_or(Value::Absent, Value::Duration),
                Value::Money { currency, amount } => match amount.checked_neg() {
                    Some(amount) => Value::Money { currency, amount },
                    None => Value::Absent,
                },
                other => return not_run(format!("negating {other:?}")),
            }),
            Expr::IsNull(inner, negated) => {
                let v = self.value(inner, env)?;
                Term::Value(Value::Bool(v.is_absent() != *negated))
            }
            Expr::In(left, right) => self.membership(left, right, env)?,
            Expr::Bin(op, l, r) => Term::Value(self.binary(*op, l, r, env)?),
            Expr::If(c, a, b) => match truth(&self.value(c, env)?)? {
                None => Term::absent(),
                Some(true) => self.term(a, env)?,
                Some(false) => self.term(b, env)?,
            },
            Expr::Agg {
                kind,
                distinct,
                var,
                collection,
                filter,
                body,
            } => Term::Value(self.aggregate(
                kind,
                *distinct,
                var.as_deref(),
                collection.as_deref(),
                filter.as_deref(),
                body.as_deref(),
                env,
            )?),
            Expr::Call(function, args) => Term::Value(self.call(function, args, env)?),
            // whether any of them was written after the event (§8.3)
            Expr::Changed(names, event) => {
                let since = self.value(event, env)?;
                let o = match self.this {
                    Some(id) => self.world.get(id)?,
                    None => None,
                };
                Term::Value(match (o, since) {
                    (Some(o), Value::Int(p)) => Value::Bool(
                        names
                            .iter()
                            .any(|n| o.writes.get(n).is_some_and(|w| *w > p)),
                    ),
                    (_, Value::Absent) | (None, _) => Value::Absent,
                    (_, other) => {
                        return not_run(format!("changed_since an event that is {other:?}"));
                    }
                })
            }
            Expr::Metric {
                name,
                binds,
                over,
                fresh: _,
            } => Term::Value(self.metric(name, binds, over.as_deref(), env)?),
        })
    }

    /// A bare name (§9.2), as the runner resolves it.
    fn name(&self, n: &str, env: &BTreeMap<String, Value>) -> Evaluated {
        if let Some(v) = env.get(n) {
            return Ok(Term::Value(v.clone()));
        }
        match n {
            "now" => return Ok(Term::Value(Value::Timestamp(self.world.now))),
            "this" => {
                return Ok(Term::Value(
                    self.this
                        .map_or(Value::Absent, |id| Value::Object(id.to_string())),
                ));
            }
            "inputs" => return Ok(Term::Inputs),
            "actor" => return Ok(Term::Actor),
            "referrers" => return Ok(Term::Value(self.referrers()?)),
            "this_event" => {
                return match self.event {
                    Some(p) => Ok(Term::Value(Value::Int(p))),
                    None => not_run("`this_event` outside an effect"),
                };
            }
            _ => {}
        }
        if let Some(t) = self.type_ {
            let member = n == "state"
                || t.attribute(n).is_some()
                || t.derived.iter().any(|(d, _)| d == n)
                || t.observations.iter().any(|(c, _)| c == n)
                || ["id", "open", "created_at", "created_by_kind"].contains(&n);
            if member {
                return match self.this {
                    Some(id) => self.member(&Term::Value(Value::Object(id.to_string())), n),
                    None => Ok(Term::absent()),
                };
            }
            if t.state(n).is_some() {
                return Ok(Term::Value(Value::State {
                    type_: t.name.clone(),
                    name: n.to_string(),
                }));
            }
            if n == "closed" || t.states.iter().any(|(_, s)| s.category == n) {
                return Ok(Term::Category(n.to_string()));
            }
        } else if n == "closed" {
            return Ok(Term::Category(n.to_string()));
        }
        if ["human", "agent", "service"].contains(&n) {
            return Ok(Term::Value(Value::Text(n.to_string())));
        }
        Ok(Term::TypeRef(n.to_string()))
    }

    /// A member of what a path reaches.
    fn member(&self, base: &Term, m: &str) -> Evaluated {
        let id = match base {
            Term::Value(Value::Absent) => return Ok(Term::absent()),
            Term::Inputs => {
                return Ok(Term::Value(
                    self.inputs.get(m).cloned().unwrap_or(Value::Absent),
                ));
            }
            Term::Actor => {
                return Ok(Term::Value(match (self.actor, m) {
                    (Some(a), "id") => Value::Text(a.id.clone()),
                    (Some(a), "kind") => Value::Text(a.kind.clone()),
                    (Some(a), "principal") => {
                        a.principal.clone().map_or(Value::Absent, Value::Text)
                    }
                    _ => Value::Absent,
                }));
            }
            Term::TypeRef(name) => {
                if self
                    .world
                    .lib
                    .enumerations
                    .get(name)
                    .is_some_and(|ms| ms.iter().any(|x| x == m))
                {
                    return Ok(Term::Value(Value::Member {
                        enumeration: name.clone(),
                        member: m.to_string(),
                    }));
                }
                if self.type_named(name).is_some_and(|t| t.state(m).is_some()) {
                    return Ok(Term::Value(Value::State {
                        type_: name.clone(),
                        name: m.to_string(),
                    }));
                }
                return not_run(format!("`{name}.{m}`"));
            }
            Term::Value(Value::State { type_, name }) if m == "category" => {
                let category = self
                    .type_named(type_)
                    .and_then(|t| t.state(name))
                    .map(|s| s.category.clone())
                    .unwrap_or_default();
                return Ok(Term::Category(category));
            }
            Term::Value(Value::Row(fields)) => {
                return match fields.get(m) {
                    Some(v) => Ok(Term::Value(v.clone())),
                    // a span no correction relabelled (ADR-0146)
                    None if m == "corrected_by" => Ok(Term::absent()),
                    None => not_run(format!(
                        "the member `{m}` of a row of an object's flow data"
                    )),
                };
            }
            Term::Value(Value::Object(id)) => id,
            other => return not_run(format!("a member `{m}` of {other:?}")),
        };
        self.note(id)?;
        let Some(o) = self.world.get(id)? else {
            return not_run(format!(
                "a member `{m}` of an object the store does not hold"
            ));
        };
        let Some(t) = self.type_named(&o.type_) else {
            return not_run(format!("the type {}", o.type_));
        };
        Ok(Term::Value(match m {
            "id" => Value::Text(o.id.clone()),
            "type" => return Ok(Term::TypeRef(o.type_.clone())),
            "state" => Value::State {
                type_: o.type_.clone(),
                name: o.state.clone(),
            },
            "created_at" => Value::Timestamp(o.created_at),
            "recorded_from" => Value::Timestamp(o.recorded_from),
            "created_by_kind" | "recorded_by_kind" => Value::Text(o.created_by_kind.clone()),
            "declaration_version" => Value::Int(self.world.version_of(&o.id)?),
            "open" => Value::Bool(!t.finished(&o.state)),
            "actor_id" => match t.actor_identity() {
                Some((attr, _)) => o.attr(attr),
                None => Value::Absent,
            },
            "intervals" => self.intervals(o, t, "state")?,
            "transitions" => Value::Set(
                self.world
                    .log()?
                    .filter(|e| e.object_id == o.id)
                    .map(|e| transition_row(self.world, o, t, e))
                    .collect(),
            ),
            "attempts" => Value::Set(
                self.world
                    .attempts()?
                    .iter()
                    .filter(|a| a.object_id.as_deref() == Some(o.id.as_str()))
                    .map(attempt_row)
                    .collect(),
            ),
            "labels" => {
                return not_run(format!("an object's `{m}`, its flow data"));
            }
            _ => {
                if let Some(a) = t.attribute(m) {
                    if let Some(v) = o.attrs.get(m) {
                        v.clone()
                    } else if let (Declared::Reference(target), Some(opposite)) =
                        (&a.declared, &a.opposite)
                    {
                        self.opposite_end(o, target, opposite, a.many)?
                    } else {
                        Value::Absent
                    }
                } else if let Some((_, kind)) = t.observations.iter().find(|(c, _)| c == m) {
                    self.collection(o, kind)?
                } else if let Some((_, e)) = t.derived.iter().find(|(d, _)| d == m) {
                    // asked with the same answers, as the runner asks it
                    let mut derived = Evaluator::new(self.world, Some(t), Some(&o.id));
                    derived.answers = self.answers;
                    derived.reading = self.reading;
                    crate::metric::typed_derived(self.world, t, e, derived.eval(e)?)
                } else {
                    return not_run(format!("the member `{}.{m}`", o.type_));
                }
            }
        }))
    }

    /// An end the other side stores: the objects of the target type whose
    /// opposite end names this one.
    fn opposite_end(
        &self,
        o: &Object,
        target: &str,
        opposite: &str,
        many: bool,
    ) -> Result<Value, NotRun> {
        let found: Vec<Value> = self
            .world
            .of_type(target)?
            .into_iter()
            .filter(|x| x.attrs.get(opposite) == Some(&Value::Object(o.id.clone())))
            .map(|x| Value::Object(x.id.clone()))
            .collect();
        Ok(if many {
            Value::Set(found)
        } else {
            found.into_iter().next().unwrap_or(Value::Absent)
        })
    }

    fn membership(&self, left: &Expr, right: &Expr, env: &BTreeMap<String, Value>) -> Evaluated {
        let l = self.value(left, env)?;
        if l.is_absent() {
            return Ok(Term::absent());
        }
        if let Expr::Name(n) = right
            && !env.contains_key(n)
            && self.world.lib.enumerations.contains_key(n)
        {
            let is = matches!(&l, Value::Member { enumeration, .. } if enumeration == n);
            return Ok(Term::Value(Value::Bool(is)));
        }
        match self.value(right, env)? {
            Value::Absent => Ok(Term::absent()),
            Value::Set(items) => Ok(Term::Value(Value::Bool(items.iter().any(|i| i.equals(&l))))),
            other => not_run(format!("membership in {other:?}")),
        }
    }

    fn binary(
        &self,
        op: BinOp,
        l: &Expr,
        r: &Expr,
        env: &BTreeMap<String, Value>,
    ) -> Result<Value, NotRun> {
        if matches!(op, BinOp::And | BinOp::Or | BinOp::Implies) {
            let mut a = truth(&self.value(l, env)?)?;
            if op == BinOp::Implies {
                a = a.map(|b| !b);
            }
            let short =
                (op == BinOp::And && a == Some(false)) || (op != BinOp::And && a == Some(true));
            if short {
                // the side that does not decide is evaluated for the record
                self.aside(r, env)?;
                return Ok(Value::Bool(op != BinOp::And));
            }
            let b = truth(&self.value(r, env)?)?;
            return Ok(match op {
                BinOp::And => match (a, b) {
                    (_, Some(false)) => Value::Bool(false),
                    (Some(true), Some(true)) => Value::Bool(true),
                    _ => Value::Absent,
                },
                _ => match (a, b) {
                    (_, Some(true)) => Value::Bool(true),
                    (Some(false), Some(false)) => Value::Bool(false),
                    _ => Value::Absent,
                },
            });
        }
        let a = self.value(l, env)?;
        let b = self.value(r, env)?;
        combine(op, &a, &b)
    }

    #[allow(clippy::too_many_arguments)]
    fn aggregate(
        &self,
        kind: &str,
        distinct: bool,
        var: Option<&str>,
        collection: Option<&Expr>,
        filter: Option<&Expr>,
        body: Option<&Expr>,
        env: &BTreeMap<String, Value>,
    ) -> Result<Value, NotRun> {
        let (Some(var), Some(collection)) = (var, collection) else {
            return not_run(format!("a metric's own `{kind}`"));
        };
        let mut scanned = None;
        let items: Vec<Value> = match self.term(collection, env)? {
            Term::Value(Value::Absent) => return Ok(Value::Absent),
            Term::Value(Value::Set(items)) => {
                // each element of a collection a rule aggregates is read
                for i in &items {
                    if let Value::Object(id) = i {
                        self.note(id)?;
                    }
                }
                items
            }
            Term::TypeRef(t) if self.type_named(&t).is_some() => {
                let all = self
                    .world
                    .of_type(&t)?
                    .into_iter()
                    .map(|o| Value::Object(o.id.clone()))
                    .collect();
                scanned = Some(t);
                all
            }
            other => return not_run(format!("an aggregate over {other:?}")),
        };
        let mut selected = Vec::new();
        let mut undecided = false;
        for item in items {
            let mut inner = env.clone();
            inner.insert(var.to_string(), item);
            // a type-scan's where examines an object, which is read only if
            // the scan matches it (ADR-0158)
            if scanned.is_some() {
                self.quiet.set(self.quiet.get() + 1);
            }
            let keep = match filter {
                None => Ok(Some(true)),
                Some(f) => self.value(f, &inner).and_then(|v| truth(&v)),
            };
            if scanned.is_some() {
                self.quiet.set(self.quiet.get() - 1);
            }
            let keep = keep?;
            match keep {
                None if self.filter_mode => {}
                None => undecided = true,
                Some(true) => selected.push(inner),
                Some(false) => {}
            }
        }
        if let (Some(t), Some((reading, rule))) = (&scanned, self.reading)
            && self.quiet.get() == 0
        {
            let matched: std::collections::BTreeSet<String> = selected
                .iter()
                .filter_map(|inner| match inner.get(var) {
                    Some(Value::Object(id)) => Some(id.clone()),
                    _ => None,
                })
                .collect();
            for id in &matched {
                self.note(id)?;
            }
            reading
                .borrow_mut()
                .scans
                .entry(rule.to_string())
                .or_default()
                .entry(t.clone())
                .or_default()
                .extend(matched);
        }
        match kind {
            "any" => Ok(if !selected.is_empty() {
                Value::Bool(true)
            } else if undecided {
                Value::Absent
            } else {
                Value::Bool(false)
            }),
            "none" => Ok(if !selected.is_empty() {
                Value::Bool(false)
            } else if undecided {
                Value::Absent
            } else {
                Value::Bool(true)
            }),
            "all" => {
                let mut unknown = undecided;
                let mut failed = false;
                for inner in &selected {
                    if let Some(b) = body {
                        // once one fails, the rest are evaluated for the
                        // record alone
                        if failed {
                            self.aside(b, inner)?;
                            continue;
                        }
                        match truth(&self.value(b, inner)?)? {
                            Some(false) if self.reading.is_none() => {
                                return Ok(Value::Bool(false));
                            }
                            Some(false) => failed = true,
                            None => unknown = true,
                            Some(true) => {}
                        }
                    }
                }
                Ok(if failed {
                    Value::Bool(false)
                } else if unknown {
                    Value::Absent
                } else {
                    Value::Bool(true)
                })
            }
            _ if undecided => Ok(Value::Absent),
            "count" => {
                if distinct {
                    let mut seen: Vec<Value> = Vec::new();
                    for inner in &selected {
                        let v = match body {
                            Some(b) => self.value(b, inner)?,
                            None => Value::Absent,
                        };
                        if !seen.iter().any(|s| s.equals(&v)) {
                            seen.push(v);
                        }
                    }
                    Ok(Value::Int(i64::try_from(seen.len()).unwrap_or(i64::MAX)))
                } else {
                    Ok(Value::Int(
                        i64::try_from(selected.len()).unwrap_or(i64::MAX),
                    ))
                }
            }
            "sum" | "min" | "max" => {
                let Some(b) = body else {
                    return not_run(format!("`{kind}` with no body"));
                };
                let mut values = Vec::new();
                for inner in &selected {
                    let v = self.value(b, inner)?;
                    if v.is_absent() {
                        return Ok(Value::Absent);
                    }
                    values.push(v);
                }
                if kind == "sum" {
                    // over no elements, 0; over some, their total, of their
                    // type, so a sum of amounts is an amount (§8.3)
                    let mut iter = values.into_iter();
                    let Some(mut total) = iter.next() else {
                        return Ok(Value::Int(0));
                    };
                    for v in iter {
                        total = combine(BinOp::Add, &total, &v)?;
                    }
                    return Ok(total);
                }
                let mut best: Option<Value> = None;
                for v in values {
                    best = Some(match best {
                        None => v,
                        Some(b) => {
                            let Some(order) = v.compare(&b) else {
                                return not_run("`min` or `max` over values of different types");
                            };
                            let better = if kind == "min" {
                                order == Ordering::Less
                            } else {
                                order == Ordering::Greater
                            };
                            if better { v } else { b }
                        }
                    });
                }
                Ok(best.unwrap_or(Value::Absent))
            }
            other => not_run(format!("`{other}`")),
        }
    }

    fn call(
        &self,
        function: &Expr,
        args: &[Expr],
        env: &BTreeMap<String, Value>,
    ) -> Result<Value, NotRun> {
        let name = match function {
            Expr::Name(n) => n.as_str(),
            Expr::Path(base, member) => {
                if let Expr::Name(b) = &**base
                    && !env.contains_key(b)
                    && b != "this"
                    && !self.attribute_of_this(b)
                {
                    return self.ask(b, member, args, env);
                }
                return match (member.as_str(), args) {
                    ("intervals" | "entered_at" | "time_in", [Expr::Name(arg)]) => {
                        match self.value(base, env)? {
                            Value::Absent => Ok(Value::Absent),
                            Value::Object(id) => self.flow_function(&id, member, arg),
                            other => not_run(format!("`{member}` of {other:?}")),
                        }
                    }
                    // the value a tracked member held as a row began
                    // (declaration-syntax.md §6.9)
                    ("held", [Expr::Name(m)]) => match self.value(base, env)? {
                        Value::Absent => Ok(Value::Absent),
                        Value::Row(fields) => Ok(match fields.get("held") {
                            Some(Value::Row(held)) => {
                                held.get(m.as_str()).cloned().unwrap_or(Value::Absent)
                            }
                            _ => Value::Absent,
                        }),
                        other => not_run(format!("`held` of {other:?}")),
                    },
                    _ => not_run(format!("the member function `{member}`")),
                };
            }
            other => return not_run(format!("a call of {other:?}")),
        };
        match (name, args) {
            ("length", [a]) => Ok(match self.value(a, env)? {
                Value::Absent => Value::Absent,
                Value::Text(s) => Value::Int(i64::try_from(s.chars().count()).unwrap_or(i64::MAX)),
                Value::Set(items) => Value::Int(i64::try_from(items.len()).unwrap_or(i64::MAX)),
                other => return not_run(format!("the length of {other:?}")),
            }),
            ("date", [a]) => Ok(match self.value(a, env)? {
                Value::Absent => Value::Absent,
                Value::Date(d) => Value::Date(d),
                Value::Timestamp(t) => Value::Date(t.div_euclid(86_400)),
                other => return not_run(format!("the date of {other:?}")),
            }),
            ("entered_at" | "time_in", [Expr::Name(arg)]) => match self.this {
                Some(id) => self.flow_function(id, name, arg),
                None => Ok(Value::Absent),
            },
            // a time's UTC calendar bucket, which a metric groups by (§6.9)
            ("day" | "week" | "month" | "quarter" | "year", [a]) => {
                Ok(match self.value(a, env)? {
                    Value::Absent => Value::Absent,
                    Value::Timestamp(t) => Value::Text(bucket(name, t)),
                    Value::Date(d) => Value::Text(bucket(name, d.saturating_mul(86_400))),
                    other => return not_run(format!("the {name} of {other:?}")),
                })
            }
            _ => not_run(format!("the function `{name}`")),
        }
    }

    /// Every object holding a reference to this one, in ascending id order,
    /// its parts excepted, since they cascade (`declaration-syntax.md` §6.5).
    fn referrers(&self) -> Result<Value, NotRun> {
        let Some(this) = self.this else {
            return Ok(Value::Set(Vec::new()));
        };
        let target = Value::Object(this.to_string());
        let lib = self.world.lib;
        let holds = |x: &Object| {
            let Some(xt) = lib.type_(&x.type_) else {
                return false;
            };
            xt.attributes.iter().any(|(a, spec)| {
                let Declared::Reference(to) = &spec.declared else {
                    return false;
                };
                // a part's end back to its whole cascades, and is no referrer
                let part = spec
                    .opposite
                    .as_ref()
                    .and_then(|back| lib.type_(to).and_then(|tt| tt.attribute(back)))
                    .is_some_and(|b| b.composite);
                !part
                    && match x.attrs.get(a) {
                        Some(Value::Set(items)) => items.contains(&target),
                        Some(v) => *v == target,
                        None => false,
                    }
            })
        };
        Ok(Value::Set(
            self.world
                .in_order()?
                .into_iter()
                .filter(|x| x.id != this && holds(x))
                .map(|x| Value::Object(x.id.clone()))
                .collect(),
        ))
    }

    /// A subject's collection of a kind: its observations nothing has
    /// corrected, in ascending id order (`declaration-syntax.md` §6.8).
    fn collection(&self, o: &Object, kind: &str) -> Result<Value, NotRun> {
        let of_kind = self.world.of_type(kind)?;
        let corrected: Vec<Value> = of_kind.iter().map(|x| x.attr("corrects")).collect();
        let subject = Value::Object(o.id.clone());
        Ok(Value::Set(
            of_kind
                .iter()
                .filter(|x| x.attr("subject") == subject)
                .filter(|x| !corrected.contains(&Value::Object(x.id.clone())))
                .map(|x| Value::Object(x.id.clone()))
                .collect(),
        ))
    }

    /// An object's `intervals(<member>)`, `entered_at(<STATE or member>)` or
    /// `time_in(<STATE>)`, read from its flow data (`declaration-syntax.md`
    /// §8.3).
    fn flow_function(&self, id: &str, function: &str, arg: &str) -> Result<Value, NotRun> {
        self.note(id)?;
        let Some(o) = self.world.get(id)? else {
            return not_run(format!("`{function}` of an object the store does not hold"));
        };
        let Some(t) = self.type_named(&o.type_) else {
            return not_run(format!("the type {}", o.type_));
        };
        let state_spans = || {
            o.spans
                .get("state")
                .into_iter()
                .flatten()
                .filter(move |sp| matches!(&sp.value, Value::State { name, .. } if name == arg))
        };
        match function {
            "intervals" => self.intervals(o, t, arg),
            // when it most recently entered the state, or took the member's
            // current value; unknown if it never has
            "entered_at" if t.state(arg).is_some() => Ok(state_spans()
                .next_back()
                .map_or(Value::Absent, |sp| Value::Timestamp(sp.entered_at))),
            "entered_at" => match o.spans.get(arg).and_then(|spans| spans.last()) {
                Some(sp) => Ok(Value::Timestamp(sp.entered_at)),
                None => not_run(format!(
                    "`entered_at({arg})`, which names neither a state nor a tracked member"
                )),
            },
            // real elapsed time in the state, a current span up to now,
            // whatever state the object is in (ADR-0147)
            "time_in" => Ok(Value::Duration(
                state_spans()
                    .map(|sp| sp.left_at.unwrap_or(self.world.now) - sp.entered_at)
                    .sum(),
            )),
            _ => not_run(format!("the member function `{function}`")),
        }
    }

    /// An object's spans in each state, or each value of a tracked member, as
    /// rows (§6.9).
    fn intervals(&self, o: &Object, t: &Type, member: &str) -> Result<Value, NotRun> {
        interval_rows(self.world, o, t, member).map(Value::Set)
    }

    /// A metric read, as a guard reads it: the dimensions it binds, each
    /// resolved against the store as the request found it and again in the
    /// request, which differ only where the request wrote what the argument
    /// reads, and the metric read from the store as the request found it
    /// (`declaration-syntax.md` §6.9, ADR-0084 §6, ADR-0139). `fresh` holds by
    /// construction, since the value is computed as the request is decided.
    fn metric(
        &self,
        name: &Expr,
        binds: &[(String, Expr)],
        over: Option<&Expr>,
        env: &BTreeMap<String, Value>,
    ) -> Result<Value, NotRun> {
        let committed = self.committed.unwrap_or(self.world);
        let found = crate::metric::find(
            committed.lib,
            name,
            self.type_.map(|t| t.name.as_str()),
            None,
        )?;
        let separate = !std::ptr::eq(committed, self.world);
        let mut bound = BTreeMap::new();
        for (d, x) in binds {
            let v = self.value(x, env)?;
            if separate {
                if let Some(id) = self.this
                    && committed.get(id)?.is_none()
                {
                    return not_run(
                        "a metric guard of an object the request itself creates, whose arguments no consultation before it can resolve",
                    );
                }
                let mut before = Evaluator::new(committed, self.type_, self.this);
                before.inputs = self.inputs;
                before.actor = self.actor;
                before.answers = self.answers;
                if before.value(x, env)? != v {
                    return not_run(
                        "a metric guard's argument the request itself writes, which check 61 refuses at publish",
                    );
                }
            }
            // a dimension bound to an unknown value leaves the metric
            // unknown (ADR-0139)
            if v.is_absent() {
                return Ok(Value::Absent);
            }
            bound.insert(d.clone(), v);
        }
        let window = match over {
            None => None,
            Some(o) => match self.value(o, env)? {
                Value::Duration(s) => Some(s),
                other => return not_run(format!("a window of {other:?}")),
            },
        };
        let value = crate::metric::read(committed, self.answers, &found, &bound, window)?;
        // consulted, by the metric and its bindings (ADR-0158)
        let written = match name {
            Expr::Name(n) => n.clone(),
            Expr::Path(b, n) => match b.as_ref() {
                Expr::Name(t) => format!("{t}.{n}"),
                _ => n.clone(),
            },
            _ => String::new(),
        };
        let bindings: Vec<String> = bound
            .iter()
            .map(|(d, v)| format!("{d}: {}", text_of_value(v)))
            .collect();
        self.consult(format!("{written}({})", bindings.join(", ")), &value);
        Ok(value)
    }
}

/// A bound value as a consulted reference writes it: as the wire does,
/// text bare.
fn text_of_value(v: &Value) -> String {
    match crate::wire::encode(v) {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    }
}

/// An object's spans in each state, or each value of a tracked member, as
/// rows (§6.9).
pub(crate) fn interval_rows(
    world: &World<'_>,
    o: &Object,
    t: &Type,
    member: &str,
) -> Result<Vec<Value>, NotRun> {
    {
        let Some(spans) = o.spans.get(member) else {
            return not_run(format!(
                "the intervals of {}.{member}, which is not tracked",
                o.type_
            ));
        };
        let now = world.now;
        let closed = closed_periods(o, t, now);
        let imported = world.imported()?;
        let rows = spans
            .iter()
            .map(|sp| {
                let mut row = BTreeMap::new();
                row.insert("object".to_string(), Value::Object(o.id.clone()));
                row.insert(
                    if member == "state" { "state" } else { "value" }.to_string(),
                    sp.value.clone(),
                );
                row.insert("entered_at".to_string(), Value::Timestamp(sp.entered_at));
                row.insert(
                    "left_at".to_string(),
                    sp.left_at.map_or(Value::Absent, Value::Timestamp),
                );
                row.insert(
                    "entered_by_kind".to_string(),
                    Value::Text(sp.entered_by_kind.clone()),
                );
                row.insert(
                    "declaration_version".to_string(),
                    Value::Int(sp.declaration_version),
                );
                row.insert("legacy".to_string(), Value::Bool(sp.legacy));
                row.insert("migrated".to_string(), Value::Bool(sp.migrated));
                // the event of the latest correction that changed its value
                // or its .held (§6.4, ADR-0147)
                row.insert(
                    "corrected_by".to_string(),
                    sp.corrected_by.map_or(Value::Absent, Value::Int),
                );
                row.insert(
                    "duration".to_string(),
                    span_duration(t, o, member, sp, &closed, now),
                );
                // the values held when it began, after the writes of the
                // transition that began it; on a span whose entry precedes
                // the event that opened it, a port's, the values the index
                // holds at its entry, legacy intervals included (§6.9,
                // ADR-0146, ADR-0147)
                let by_time = sp.legacy || imported.contains(&sp.entered_position);
                row.insert(
                    "held".to_string(),
                    if by_time {
                        held_at(world.lib, t, o, sp.entered_at)
                    } else {
                        held(world.lib, t, o, sp.entered_position, true)
                    },
                );
                Value::Row(row)
            })
            .collect();
        Ok(rows)
    }
}

impl Evaluator<'_> {
    /// An evaluator's verdict, as the store's evaluators answer it: asked
    /// nothing where an argument is absent, the guard then unknown with its
    /// own remedy (ADR-0146); `unanswered` leaves it unknown and temporal
    /// (ADR-0138).
    fn ask(
        &self,
        evaluator: &str,
        function: &str,
        args: &[Expr],
        env: &BTreeMap<String, Value>,
    ) -> Result<Value, NotRun> {
        let key = format!("{evaluator}.{function}");
        let mut written = Vec::new();
        for a in args {
            let v = self.value(a, env)?;
            if v.is_absent() {
                return Ok(Value::Absent);
            }
            written.push(text_of_value(&v));
        }
        // an offer asks no evaluator: the call is left unanswered (ADR-0048)
        let Some(answers) = self.answers else {
            self.unanswered.set(true);
            return Ok(Value::Absent);
        };
        let answer = answers.get(&key).map(String::as_str);
        let verdict = match answer {
            Some("satisfied") => Value::Bool(true),
            Some("unsatisfied") => Value::Bool(false),
            Some("unanswered") => {
                self.unanswered.set(true);
                Value::Absent
            }
            // asked for the record alone, a call the sandbox has no answer
            // for consults nothing, as stopping early would never have asked
            None if self.aside.get() > 0 => return Ok(Value::Absent),
            _ => {
                return not_run(format!(
                    "the evaluator function {key}, which the sandbox gives no answer for"
                ));
            }
        };
        // consulted, by the call and its arguments: the verdict, never a
        // value (ADR-0069, ADR-0158)
        if let Some(a) = answer {
            self.consult(
                format!("{key}({})", written.join(", ")),
                &Value::Text(a.to_string()),
            );
        }
        Ok(verdict)
    }

    fn attribute_of_this(&self, name: &str) -> bool {
        self.type_.is_some_and(|t| t.attribute(name).is_some())
    }
}

/// A comparison or an arithmetic operator over two values (§8.2, §8.3).
/// The integer 0 a sum over no elements yields, read as the zero of the
/// value it meets. Such a sum is 0 of its element type (§8.3), which an empty
/// set cannot tell, and a module the flow checker passed sets an int against a
/// duration or an amount nowhere else (ADR-0153 decision 5).
fn zero_as(v: &Value, other: &Value) -> Value {
    match (v, other) {
        (Value::Int(0), Value::Duration(_)) => Value::Duration(0),
        (Value::Int(0), Value::Money { currency, amount }) => Decimal::new(0, amount.scale())
            .map_or(Value::Int(0), |zero| Value::Money {
                currency: currency.clone(),
                amount: zero,
            }),
        _ => v.clone(),
    }
}

pub fn combine(op: BinOp, a: &Value, b: &Value) -> Result<Value, NotRun> {
    use Value as V;
    let (za, zb) = (zero_as(a, b), zero_as(b, a));
    let (a, b) = (&za, &zb);
    if a.is_absent() || b.is_absent() {
        return Ok(V::Absent);
    }
    let unknown = |v: Option<Value>| v.unwrap_or(V::Absent);
    let compare = |want: fn(Ordering) -> bool| -> Result<Value, NotRun> {
        match a.compare(b) {
            Some(o) => Ok(V::Bool(want(o))),
            None => not_run(format!("comparing {a:?} with {b:?}")),
        }
    };
    match op {
        BinOp::Eq => return Ok(V::Bool(a.equals(b))),
        BinOp::Ne => return Ok(V::Bool(!a.equals(b))),
        BinOp::Lt => return compare(|o| o == Ordering::Less),
        BinOp::Le => return compare(|o| o != Ordering::Greater),
        BinOp::Gt => return compare(|o| o == Ordering::Greater),
        BinOp::Ge => return compare(|o| o != Ordering::Less),
        _ => {}
    }
    let number = |v: &Value| match v {
        V::Int(n) => Some(Decimal::from_int(*n)),
        V::Decimal(d) => Some(*d),
        _ => None,
    };
    Ok(match (op, a, b) {
        (BinOp::Add, V::Int(x), V::Int(y)) => unknown(x.checked_add(*y).map(V::Int)),
        (BinOp::Sub, V::Int(x), V::Int(y)) => unknown(x.checked_sub(*y).map(V::Int)),
        (BinOp::Mul, V::Int(x), V::Int(y)) => unknown(x.checked_mul(*y).map(V::Int)),
        (BinOp::Div, V::Int(_), V::Int(0)) => V::Absent,
        // int / int discards the remainder, toward zero (§8.3, ADR-0147)
        (BinOp::Div, V::Int(x), V::Int(y)) => unknown(x.checked_div(*y).map(V::Int)),
        (
            BinOp::Add | BinOp::Sub,
            V::Money {
                currency: c,
                amount: x,
            },
            V::Money {
                currency: d,
                amount: y,
            },
        ) if c == d => {
            let r = if op == BinOp::Add {
                x.checked_add(y)
            } else {
                x.checked_sub(y)
            };
            unknown(r.map(|amount| V::Money {
                currency: c.clone(),
                amount,
            }))
        }
        (
            BinOp::Div,
            V::Money {
                currency: c,
                amount: x,
            },
            V::Money {
                currency: d,
                amount: y,
            },
        ) if c == d => unknown(x.checked_div(y, 6).map(V::Decimal)),
        (BinOp::Mul | BinOp::Div, V::Money { currency, amount }, n)
        | (BinOp::Mul, n, V::Money { currency, amount })
            if number(n).is_some() =>
        {
            let k = number(n).unwrap_or_else(|| Decimal::from_int(0));
            let minor = minor_unit(currency).unwrap_or(2);
            let r = if op == BinOp::Mul {
                amount.checked_mul(&k).and_then(|p| p.rescale(minor))
            } else {
                amount.checked_div(&k, minor)
            };
            unknown(r.map(|amount| V::Money {
                currency: currency.clone(),
                amount,
            }))
        }
        (BinOp::Add, V::Duration(x), V::Duration(y)) => unknown(x.checked_add(*y).map(V::Duration)),
        (BinOp::Sub, V::Duration(x), V::Duration(y)) => unknown(x.checked_sub(*y).map(V::Duration)),
        (BinOp::Div, V::Duration(x), V::Duration(y)) => unknown(
            Decimal::from_int(*x)
                .checked_div(&Decimal::from_int(*y), 6)
                .map(V::Decimal),
        ),
        (BinOp::Mul | BinOp::Div, V::Duration(s), n) | (BinOp::Mul, n, V::Duration(s))
            if number(n).is_some() =>
        {
            let k = number(n).unwrap_or_else(|| Decimal::from_int(0));
            let r = if op == BinOp::Mul {
                Decimal::from_int(*s)
                    .checked_mul(&k)
                    .and_then(|p| p.rescale(0))
            } else {
                Decimal::from_int(*s).checked_div(&k, 0)
            };
            unknown(r.and_then(|d| d.to_int()).map(V::Duration))
        }
        (BinOp::Add, V::Timestamp(t), V::Duration(d))
        | (BinOp::Add, V::Duration(d), V::Timestamp(t)) => {
            unknown(t.checked_add(*d).map(V::Timestamp))
        }
        (BinOp::Sub, V::Timestamp(t), V::Duration(d)) => {
            unknown(t.checked_sub(*d).map(V::Timestamp))
        }
        (BinOp::Sub, V::Timestamp(x), V::Timestamp(y)) => {
            unknown(x.checked_sub(*y).map(V::Duration))
        }
        (BinOp::Sub, V::Date(x), V::Date(y)) => unknown(
            x.checked_sub(*y)
                .and_then(|d| d.checked_mul(86_400))
                .map(V::Duration),
        ),
        (BinOp::Add | BinOp::Sub, V::Date(day), V::Duration(s)) => {
            if s % 86_400 != 0 {
                return not_run("a date moved by part of a day");
            }
            let days = s / 86_400;
            unknown(
                if op == BinOp::Add {
                    day.checked_add(days)
                } else {
                    day.checked_sub(days)
                }
                .map(V::Date),
            )
        }
        (BinOp::Div, _, n) if number(n).is_some_and(|d| d.is_zero()) => V::Absent,
        (BinOp::Div, V::Int(_), V::Decimal(_)) => return not_run("an int divided by a decimal"),
        (_, x, y) if number(x).is_some() && number(y).is_some() => {
            let (p, q) = (
                number(x).unwrap_or_else(|| Decimal::from_int(0)),
                number(y).unwrap_or_else(|| Decimal::from_int(0)),
            );
            unknown(
                match op {
                    BinOp::Add => p.checked_add(&q),
                    BinOp::Sub => p.checked_sub(&q),
                    BinOp::Mul => p.checked_mul(&q),
                    // a quotient keeps the left operand's scale (§8.3)
                    _ => p.checked_div(&q, p.scale()),
                }
                .map(V::Decimal),
            )
        }
        _ => return not_run(format!("{a:?} {} {b:?}", op.text())),
    })
}

/// The stretches, by occurred time, an object spent in a closed or terminal
/// state.
pub(crate) fn closed_periods(o: &Object, t: &Type, now: i64) -> Vec<(i64, i64)> {
    o.spans
        .get("state")
        .into_iter()
        .flatten()
        .filter(|sp| matches!(&sp.value, Value::State { name, .. } if t.finished(name)))
        .map(|sp| (sp.entered_at, sp.left_at.unwrap_or(now)))
        .collect()
}

/// The value each tracked member held at an event's position: after its
/// writes on a span it began, before them on the event's own row, read from
/// the interval index (`declaration-syntax.md` §6.9, ADR-0146). A member no
/// span covers there is absent, as on a creation's row.
pub(crate) fn held(
    lib: &crate::model::Library,
    t: &Type,
    o: &Object,
    position: i64,
    after: bool,
) -> Value {
    let mut out = BTreeMap::new();
    for m in lib.tracked(t) {
        let covering = o.spans.get(&m).into_iter().flatten().find(|sp| {
            if after {
                sp.entered_position <= position && sp.left_position.is_none_or(|l| l > position)
            } else {
                sp.entered_position < position && sp.left_position.is_none_or(|l| l >= position)
            }
        });
        if let Some(sp) = covering {
            out.insert(m, sp.value.clone());
        }
    }
    Value::Row(out)
}

/// The value each tracked member held at a time, by the interval index,
/// legacy intervals included: absent where no span covers it (§6.9,
/// ADR-0147).
pub(crate) fn held_at(lib: &crate::model::Library, t: &Type, o: &Object, at: i64) -> Value {
    let mut out = BTreeMap::new();
    for m in lib.tracked(t) {
        let covering = o
            .spans
            .get(&m)
            .into_iter()
            .flatten()
            .filter(|sp| sp.entered_at <= at && sp.left_at.is_none_or(|l| l > at))
            .max_by_key(|sp| sp.entered_at);
        if let Some(sp) = covering {
            out.insert(m, sp.value.clone());
        }
    }
    Value::Row(out)
}

/// An event as a row of `transitions` (`declaration-syntax.md` §6.9): what
/// it records, and what the declaration and the index say of it.
pub(crate) fn transition_row(
    world: &World<'_>,
    o: &Object,
    t: &Type,
    e: &crate::store::Event,
) -> Value {
    let state = |name: &str| Value::State {
        type_: o.type_.clone(),
        name: name.to_string(),
    };
    let open = |name: &str| !t.finished(name);
    // a transition into a state the object held before, read over every
    // span the index held when it was recorded (§6.9, ADR-0147)
    let visited = o
        .spans
        .get("state")
        .into_iter()
        .flatten()
        .filter(|sp| sp.entered_position < e.position)
        .any(|sp| matches!(&sp.value, Value::State { name, .. } if *name == e.to_state));
    let from = e.from_state.as_deref();
    let asserted = e.source == "asserted";
    let erased = e.source == "erased";
    // an assertion's reason input, its declared enumeration's member; an
    // erasure's the fixed value `erasure` (§6.9)
    let reason = if erased {
        Value::Text("erasure".into())
    } else if asserted && !e.imported {
        // the reason its event records apart, withheld where it is personal:
        // the inputs the event also records may hold a personal one
        match e.payload["reason"].as_str() {
            Some(member) => match t
                .transition(&e.transition)
                .and_then(|x| x.inputs.iter().find(|(n, _)| n == "reason"))
                .map(|(_, i)| &i.declared)
            {
                Some(Declared::Scalar(enumeration))
                    if world
                        .lib
                        .enumerations
                        .get(enumeration)
                        .is_some_and(|ms| ms.iter().any(|m| m == member)) =>
                {
                    Value::Member {
                        enumeration: enumeration.clone(),
                        member: member.to_string(),
                    }
                }
                _ => Value::Text(member.to_string()),
            },
            None => Value::Absent,
        }
    } else {
        Value::Absent
    };
    let mut row = BTreeMap::new();
    row.insert("object".to_string(), Value::Object(o.id.clone()));
    row.insert("transition".to_string(), Value::Text(e.transition.clone()));
    row.insert("from_state".to_string(), from.map_or(Value::Absent, state));
    row.insert("to_state".to_string(), state(&e.to_state));
    row.insert(
        "completes".to_string(),
        Value::Bool(from.is_some_and(|f| open(f) && !open(&e.to_state))),
    );
    row.insert(
        "returns".to_string(),
        Value::Bool(from.is_some_and(|f| f != e.to_state) && visited),
    );
    row.insert(
        "occurred_at".to_string(),
        Value::Timestamp(e.occurred_at.unwrap_or(e.recorded_at)),
    );
    row.insert("recorded_at".to_string(), Value::Timestamp(e.recorded_at));
    row.insert("actor_id".to_string(), Value::Text(e.actor_id.clone()));
    row.insert("actor_kind".to_string(), Value::Text(e.actor_kind.clone()));
    row.insert("asserted".to_string(), Value::Bool(asserted));
    // an assertion's event, and any that recorded an admission (§6.9)
    row.insert(
        "overrides".to_string(),
        Value::Bool((asserted && !e.imported) || world.admitting.contains(&e.position)),
    );
    row.insert("imported".to_string(), Value::Bool(e.imported));
    row.insert("migrated".to_string(), Value::Bool(e.source == "migrated"));
    row.insert(
        "redacted".to_string(),
        Value::Bool(e.transition == "redact"),
    );
    row.insert("reason".to_string(), reason);
    row.insert(
        "declaration_version".to_string(),
        Value::Int(e.declaration_version),
    );
    row.insert("held".to_string(), held(world.lib, t, o, e.position, false));
    Value::Row(row)
}

/// A row of the attempt log as a row of `attempts` (`declaration-syntax.md`
/// §6.9): what the log holds of it, a remedy the core could not tell absent.
pub(crate) fn attempt_row(a: &crate::store::Attempt) -> Value {
    let text = |t: &Option<String>| t.clone().map_or(Value::Absent, Value::Text);
    let mut row = BTreeMap::new();
    row.insert(
        "object".to_string(),
        a.object_id.clone().map_or(Value::Absent, Value::Object),
    );
    row.insert("transition".to_string(), text(&a.transition));
    row.insert("verdict".to_string(), Value::Text(a.verdict.clone()));
    row.insert("clause".to_string(), text(&a.clause));
    row.insert(
        "remedy".to_string(),
        if a.remedy.is_empty() {
            Value::Absent
        } else {
            Value::Text(a.remedy.clone())
        },
    );
    row.insert("unknown".to_string(), Value::Bool(a.unknown));
    row.insert("enforced".to_string(), Value::Bool(a.enforced));
    row.insert("flagged".to_string(), Value::Bool(a.flagged));
    row.insert("actor_id".to_string(), Value::Text(a.actor_id.clone()));
    row.insert("actor_kind".to_string(), Value::Text(a.actor_kind.clone()));
    row.insert("at".to_string(), Value::Timestamp(a.at));
    row.insert(
        "declaration_version".to_string(),
        Value::Int(a.declaration_version),
    );
    Value::Row(row)
}

/// A time's UTC calendar bucket, as a metric's dimension writes it:
/// `YYYY-MM-DD`, the ISO 8601 week `YYYY-Www` that begins on Monday,
/// `YYYY-MM`, `YYYY-Qn` or `YYYY` (`declaration-syntax.md` §6.9, ADR-0106).
pub(crate) fn bucket(unit: &str, seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let (y, m, d) = crate::value::civil_from_days(days);
    match unit {
        "day" => format!("{y:04}-{m:02}-{d:02}"),
        "week" => {
            // the week's Thursday names its ISO year
            let weekday = (days + 3).rem_euclid(7);
            let thursday = days - weekday + 3;
            let (year, _, _) = crate::value::civil_from_days(thursday);
            let week = (thursday - crate::value::days_from_civil(year, 1, 1)) / 7 + 1;
            format!("{year:04}-W{week:02}")
        }
        "month" => format!("{y:04}-{m:02}"),
        "quarter" => format!("{y:04}-Q{}", (m - 1) / 3 + 1),
        _ => format!("{y:04}"),
    }
}

/// A span's duration. A state's span is its exit less its entry, a current
/// one running to now, but a span within the unbroken stretch of closed or
/// terminal states the object is in now has none. A member's span counts
/// only the time its object was open, and has none where it had no open time
/// (§6.9, ADR-0101, ADR-0146, ADR-0148).
fn span_duration(
    t: &Type,
    o: &Object,
    member: &str,
    sp: &crate::world::Span,
    closed: &[(i64, i64)],
    now: i64,
) -> Value {
    let end = sp.left_at.unwrap_or(now);
    if member != "state" {
        let length = end - sp.entered_at;
        let shut: i64 = closed
            .iter()
            .map(|(a, b)| (end.min(*b) - sp.entered_at.max(*a)).max(0))
            .sum();
        return if length - shut > 0 || length == 0 {
            Value::Duration(length - shut)
        } else {
            Value::Absent
        };
    }
    if sp.left_at.is_some() || !t.finished(&o.state) {
        return Value::Duration(end - sp.entered_at);
    }
    // the stretch begins where it entered its current state, which is now
    // while the transition into it is being taken and its span not yet
    // recorded, and runs back over the closed spans before
    let spans = o.spans.get("state").map_or(&[][..], Vec::as_slice);
    let name = |sp: &crate::world::Span| match &sp.value {
        Value::State { name, .. } => name.clone(),
        _ => String::new(),
    };
    let mut entry = match spans.last() {
        Some(last) if name(last) == o.state => last.entered_at,
        _ => now,
    };
    for past in spans.iter().rev() {
        if !t.finished(&name(past)) {
            break;
        }
        entry = past.entered_at;
    }
    if sp.entered_at < entry {
        Value::Duration(entry - sp.entered_at)
    } else {
        Value::Absent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::parse;
    use crate::model::Library;

    fn lib() -> Library {
        Library::load(&[(
            "jobs.yaml".into(),
            r#"
module: jobs
categories: [inbound, live, closed]
enumerations: { Priority: [LOW, HIGH] }
types:
  Job:
    attributes:
      title: { type: string }
      priority: { type: Priority, optional: true }
      hours: { type: int, optional: true }
      tags: { type: "string[]", optional: true }
    derived_attributes:
      urgent: { expression: priority == Priority.HIGH }
    states:
      OPEN: { category: inbound }
      DONE: { category: closed, final: true }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [title] }
"#
            .into(),
        )])
        .unwrap()
    }

    fn job(id: &str, order: i64, state: &str, attrs: &[(&str, Value)]) -> Object {
        Object {
            id: id.into(),
            type_: "Job".into(),
            state: state.into(),
            version: 1,
            attrs: attrs
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect(),
            created_at: 0,
            created_by_kind: "human".into(),
            order,
            superseded_by: None,
            spans: BTreeMap::new(),
            writes: BTreeMap::new(),
            admissions: std::collections::BTreeSet::new(),
            erased: false,
            created_dated: true,
            recorded_from: 0,
            state_source: None,
        }
    }

    fn eval(world: &World, this: &str, text: &str) -> Result<Value, NotRun> {
        let t = world.lib.type_("Job");
        Evaluator::new(world, t, Some(this)).eval(&parse(text).unwrap())
    }

    #[test]
    fn names_resolve_as_the_runner_resolves_them() {
        let lib = lib();
        let mut w = World::new(&lib, 100);
        w.insert(job(
            "job-1",
            1,
            "OPEN",
            &[
                ("title", Value::Text("Fix".into())),
                (
                    "priority",
                    Value::Member {
                        enumeration: "Priority".into(),
                        member: "HIGH".into(),
                    },
                ),
            ],
        ))
        .unwrap();
        assert_eq!(eval(&w, "job-1", "title == \"Fix\""), Ok(Value::Bool(true)));
        assert_eq!(eval(&w, "job-1", "urgent"), Ok(Value::Bool(true)));
        assert_eq!(eval(&w, "job-1", "state == OPEN"), Ok(Value::Bool(true)));
        assert_eq!(
            eval(&w, "job-1", "state.category == inbound"),
            Ok(Value::Bool(true))
        );
        assert_eq!(eval(&w, "job-1", "open"), Ok(Value::Bool(true)));
        assert_eq!(eval(&w, "job-1", "now"), Ok(Value::Timestamp(100)));
        assert_eq!(
            eval(&w, "job-1", "priority in Priority"),
            Ok(Value::Bool(true))
        );
    }

    #[test]
    fn unknown_propagates_but_through_is_null_and_or() {
        let lib = lib();
        let mut w = World::new(&lib, 0);
        w.insert(job("job-1", 1, "OPEN", &[])).unwrap();
        assert_eq!(eval(&w, "job-1", "hours > 2"), Ok(Value::Absent));
        assert_eq!(eval(&w, "job-1", "hours is null"), Ok(Value::Bool(true)));
        assert_eq!(
            eval(&w, "job-1", "hours > 2 and false"),
            Ok(Value::Bool(false))
        );
        assert_eq!(
            eval(&w, "job-1", "hours > 2 or true"),
            Ok(Value::Bool(true))
        );
        assert_eq!(eval(&w, "job-1", "hours > 2 or false"), Ok(Value::Absent));
        assert_eq!(eval(&w, "job-1", "1 / 0"), Ok(Value::Absent));
        assert_eq!(eval(&w, "job-1", "7 / 2"), Ok(Value::Int(3)));
        assert_eq!(eval(&w, "job-1", "-7 / 2"), Ok(Value::Int(-3)));
    }

    #[test]
    fn a_type_scan_and_its_aggregates() {
        let lib = lib();
        let mut w = World::new(&lib, 0);
        w.insert(job("job-1", 1, "OPEN", &[("hours", Value::Int(3))]))
            .unwrap();
        w.insert(job("job-2", 2, "DONE", &[("hours", Value::Int(5))]))
            .unwrap();
        w.insert(job("job-3", 3, "OPEN", &[])).unwrap();
        assert_eq!(
            eval(&w, "job-1", "count(j in Job where j.open)"),
            Ok(Value::Int(2))
        );
        assert_eq!(
            eval(
                &w,
                "job-1",
                "sum(j in Job where j.hours is not null: j.hours)"
            ),
            Ok(Value::Int(8))
        );
        assert_eq!(
            eval(
                &w,
                "job-1",
                "max(j in Job where j.hours is not null: j.hours)"
            ),
            Ok(Value::Int(5))
        );
        // a filter that is unknown for one element makes count unknown, unless filtering
        assert_eq!(
            eval(&w, "job-1", "count(j in Job where j.hours > 1)"),
            Ok(Value::Absent)
        );
        assert_eq!(
            eval(&w, "job-1", "any(j in Job where j.hours > 4)"),
            Ok(Value::Bool(true))
        );
        assert_eq!(
            eval(&w, "job-1", "none(j in Job where j.hours > 9)"),
            Ok(Value::Absent)
        );
        assert_eq!(
            eval(&w, "job-1", "sum(j in Job where false: j.hours)"),
            Ok(Value::Int(0))
        );
    }

    #[test]
    fn arithmetic_keeps_its_types() {
        let usd = |a: &str| Value::Money {
            currency: "USD".into(),
            amount: Decimal::parse(a).unwrap(),
        };
        assert_eq!(
            combine(BinOp::Add, &usd("1.10"), &usd("2.05")),
            Ok(usd("3.15"))
        );
        assert_eq!(
            combine(BinOp::Div, &usd("10.00"), &Value::Int(3)),
            Ok(usd("3.33"))
        );
        assert_eq!(
            combine(
                BinOp::Mul,
                &usd("0.05"),
                &Value::Decimal(Decimal::parse("0.5").unwrap())
            ),
            Ok(usd("0.02"))
        );
        assert_eq!(
            combine(BinOp::Sub, &Value::Timestamp(7200), &Value::Timestamp(3600)),
            Ok(Value::Duration(3600))
        );
        assert_eq!(
            combine(BinOp::Add, &Value::Date(10), &Value::Duration(86_400)),
            Ok(Value::Date(11))
        );
        assert!(combine(BinOp::Add, &Value::Date(10), &Value::Duration(3600)).is_err());
        // a sum over nothing is 0 of what it meets (§8.3)
        assert_eq!(
            combine(BinOp::Lt, &Value::Int(0), &Value::Duration(60)),
            Ok(Value::Bool(true))
        );
        assert_eq!(
            combine(BinOp::Add, &Value::Int(0), &usd("1.00")),
            Ok(usd("1.00"))
        );
        assert!(combine(BinOp::Lt, &Value::Int(1), &Value::Duration(60)).is_err());
        assert!(combine(BinOp::Lt, &usd("1.00"), &Value::Text("x".into())).is_err());
    }

    #[test]
    fn an_evaluator_answers_as_the_sandbox_says() {
        let lib = lib();
        let mut w = World::new(&lib, 0);
        w.insert(job(
            "job-1",
            1,
            "OPEN",
            &[("title", Value::Text("Fix".into()))],
        ))
        .unwrap();
        let t = w.lib.type_("Job");
        let call = parse("xero.bill_paid(title)").unwrap();
        let answers: BTreeMap<String, String> =
            [("xero.bill_paid".to_string(), "satisfied".to_string())].into();
        let asking = Evaluator::new(&w, t, Some("job-1")).with_answers(&answers);
        assert_eq!(asking.eval(&call), Ok(Value::Bool(true)));
        assert!(!asking.unanswered.get());
        // unanswered leaves it unknown, and says so
        let answers: BTreeMap<String, String> =
            [("xero.bill_paid".to_string(), "unanswered".to_string())].into();
        let asking = Evaluator::new(&w, t, Some("job-1")).with_answers(&answers);
        assert_eq!(asking.eval(&call), Ok(Value::Absent));
        assert!(asking.unanswered.get());
        // an absent argument asks nothing: unknown, but not unanswered (ADR-0146)
        let absent = parse("xero.bill_paid(hours)").unwrap();
        let asking = Evaluator::new(&w, t, Some("job-1")).with_answers(&answers);
        assert_eq!(asking.eval(&absent), Ok(Value::Absent));
        assert!(!asking.unanswered.get());
        // an offer asks no evaluator (ADR-0048)
        let offering = Evaluator::new(&w, t, Some("job-1"));
        assert_eq!(offering.eval(&call), Ok(Value::Absent));
        assert!(offering.unanswered.get());
        // and a sandbox that gives no answer is not guessed
        let none = BTreeMap::new();
        let asking = Evaluator::new(&w, t, Some("job-1")).with_answers(&none);
        assert!(
            asking
                .eval(&call)
                .unwrap_err()
                .what()
                .contains("xero.bill_paid")
        );
    }

    #[test]
    fn what_the_slice_does_not_run_is_named() {
        let lib = lib();
        let mut w = World::new(&lib, 0);
        w.insert(job("job-1", 1, "OPEN", &[])).unwrap();
        assert!(
            eval(&w, "job-1", "metric(x) > 1")
                .unwrap_err()
                .what()
                .contains("metric")
        );
    }
}

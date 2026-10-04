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

/// A construct the first slice does not run (ADR-0153 decision 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotRun(pub String);

impl fmt::Display for NotRun {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for NotRun {}

fn not_run<T>(what: impl Into<String>) -> Result<T, NotRun> {
    Err(NotRun(what.into()))
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
        }
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
            Expr::Changed(..) => {
                return not_run("changed_since, which reads the attribute write index");
            }
            Expr::Metric { .. } => return not_run("a metric read in a guard"),
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
            "referrers" => return not_run("referrers"),
            "this_event" => return not_run("`this_event` outside an effect"),
            _ => {}
        }
        if let Some(t) = self.type_ {
            let member = n == "state"
                || t.attribute(n).is_some()
                || t.derived.iter().any(|(d, _)| d == n)
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
            Term::Value(Value::Object(id)) => id,
            other => return not_run(format!("a member `{m}` of {other:?}")),
        };
        let Some(o) = self.world.get(id) else {
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
            "created_at" | "recorded_from" => Value::Timestamp(o.created_at),
            "created_by_kind" => Value::Text(o.created_by_kind.clone()),
            "declaration_version" => Value::Int(1),
            "open" => Value::Bool(!t.finished(&o.state)),
            "actor_id" => match t.actor_identity() {
                Some((attr, _)) => o.attr(attr),
                None => Value::Absent,
            },
            "intervals" | "transitions" | "attempts" | "labels" => {
                return not_run(format!("an object's `{m}`, its flow data"));
            }
            _ => {
                if let Some(a) = t.attribute(m) {
                    if let Some(v) = o.attrs.get(m) {
                        v.clone()
                    } else if let (Declared::Reference(target), Some(opposite)) =
                        (&a.declared, &a.opposite)
                    {
                        self.opposite_end(o, target, opposite, a.many)
                    } else {
                        Value::Absent
                    }
                } else if let Some((_, e)) = t.derived.iter().find(|(d, _)| d == m) {
                    Evaluator::new(self.world, Some(t), Some(&o.id)).eval(e)?
                } else {
                    return not_run(format!("the member `{}.{m}`", o.type_));
                }
            }
        }))
    }

    /// An end the other side stores: the objects of the target type whose
    /// opposite end names this one.
    fn opposite_end(&self, o: &Object, target: &str, opposite: &str, many: bool) -> Value {
        let found: Vec<Value> = self
            .world
            .of_type(target)
            .into_iter()
            .filter(|x| x.attrs.get(opposite) == Some(&Value::Object(o.id.clone())))
            .map(|x| Value::Object(x.id.clone()))
            .collect();
        if many {
            Value::Set(found)
        } else {
            found.into_iter().next().unwrap_or(Value::Absent)
        }
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
        let items: Vec<Value> = match self.term(collection, env)? {
            Term::Value(Value::Absent) => return Ok(Value::Absent),
            Term::Value(Value::Set(items)) => items,
            Term::TypeRef(t) if self.type_named(&t).is_some() => self
                .world
                .of_type(&t)
                .into_iter()
                .map(|o| Value::Object(o.id.clone()))
                .collect(),
            other => return not_run(format!("an aggregate over {other:?}")),
        };
        let mut selected = Vec::new();
        let mut undecided = false;
        for item in items {
            let mut inner = env.clone();
            inner.insert(var.to_string(), item);
            let keep = match filter {
                None => Some(true),
                Some(f) => truth(&self.value(f, &inner)?)?,
            };
            match keep {
                None if self.filter_mode => {}
                None => undecided = true,
                Some(true) => selected.push(inner),
                Some(false) => {}
            }
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
                for inner in &selected {
                    if let Some(b) = body {
                        match truth(&self.value(b, inner)?)? {
                            Some(false) => return Ok(Value::Bool(false)),
                            None => unknown = true,
                            Some(true) => {}
                        }
                    }
                }
                Ok(if unknown {
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
                    let mut total = Value::Int(0);
                    for v in &values {
                        total = combine(BinOp::Add, &total, v)?;
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
                return match &**base {
                    Expr::Name(b)
                        if !env.contains_key(b) && b != "this" && !self.attribute_of_this(b) =>
                    {
                        not_run(format!("the evaluator function {b}.{member}"))
                    }
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
            ("entered_at" | "time_in", _) => {
                not_run(format!("`{name}`, which reads an object's intervals"))
            }
            _ => not_run(format!("the function `{name}`")),
        }
    }

    fn attribute_of_this(&self, name: &str) -> bool {
        self.type_.is_some_and(|t| t.attribute(name).is_some())
    }
}

/// A comparison or an arithmetic operator over two values (§8.2, §8.3).
pub fn combine(op: BinOp, a: &Value, b: &Value) -> Result<Value, NotRun> {
    use Value as V;
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
        w.objects.insert(
            "job-1".into(),
            job(
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
            ),
        );
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
        w.objects
            .insert("job-1".into(), job("job-1", 1, "OPEN", &[]));
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
        w.objects.insert(
            "job-1".into(),
            job("job-1", 1, "OPEN", &[("hours", Value::Int(3))]),
        );
        w.objects.insert(
            "job-2".into(),
            job("job-2", 2, "DONE", &[("hours", Value::Int(5))]),
        );
        w.objects
            .insert("job-3".into(), job("job-3", 3, "OPEN", &[]));
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
        assert!(combine(BinOp::Lt, &usd("1.00"), &Value::Text("x".into())).is_err());
    }

    #[test]
    fn what_the_slice_does_not_run_is_named() {
        let lib = lib();
        let mut w = World::new(&lib, 0);
        w.objects
            .insert("job-1".into(), job("job-1", 1, "OPEN", &[]));
        assert!(
            eval(&w, "job-1", "xero.bill_paid(title)")
                .unwrap_err()
                .0
                .contains("evaluator")
        );
        assert!(
            eval(&w, "job-1", "time_in(OPEN) > 1 h")
                .unwrap_err()
                .0
                .contains("intervals")
        );
        assert!(
            eval(&w, "job-1", "metric(x) > 1")
                .unwrap_err()
                .0
                .contains("metric")
        );
    }
}

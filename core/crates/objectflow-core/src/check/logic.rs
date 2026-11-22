//! Step 5 of the flow checker: contradictions within one type, a port of
//! `scripts/flowlogic.py` (ADR-0131 decisions 7 to 9, ADR-0137).
//!
//! Three shapes are decided, each over one type's rules: a transition no
//! request can take, a state no object can be in, and a guard that audits
//! or warns and can never pass when its transition is taken. A condition is
//! expanded into cases, each a conjunction of atoms, under the three-valued
//! logic of `declaration-syntax.md` §8.2; atoms are a value's presence, a
//! finite domain, equality with a string, and a difference bound between two
//! ordered values, which a Floyd–Warshall pass decides. Anything else is an
//! opaque atom, which can only make conditions look satisfiable, so the
//! analysis says "cannot hold" only where it has proved it.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::doc::{Obj, Path, flag, list, obj, py_str, segs, str_or, strings};
use super::steps::kind;
use crate::expr::{BinOp, Expr, Number, parse};

/// The cases a group of conditions sharing a value may expand to before it
/// is reported rather than decided.
const LIMIT: usize = 4096;

/// Too many cases to decide.
struct TooLarge;

type Decided<T> = Result<T, TooLarge>;

/// An exact rational, as `fractions.Fraction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Rat {
    n: i128,
    d: i128,
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

impl Rat {
    fn new(n: i128, d: i128) -> Rat {
        let (n, d) = if d < 0 { (-n, -d) } else { (n, d) };
        let g = gcd(n, d);
        Rat { n: n / g, d: d / g }
    }

    fn int(n: i128) -> Rat {
        Rat::new(n, 1)
    }

    /// A decimal's text, as `Fraction(str)` reads it.
    pub(super) fn parse(text: &str) -> Option<Rat> {
        let t = text.trim();
        let (mantissa, exp) = match t.split_once(['e', 'E']) {
            Some((m, e)) => (m, e.parse::<i32>().ok()?),
            None => (t, 0),
        };
        let (neg, digits) = match mantissa.strip_prefix('-') {
            Some(d) => (true, d),
            None => (false, mantissa.strip_prefix('+').unwrap_or(mantissa)),
        };
        let (whole, frac) = digits.split_once('.').unwrap_or((digits, ""));
        let all: String = format!("{whole}{frac}");
        if all.is_empty() || !all.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let mut n: i128 = all.parse().ok()?;
        let mut d: i128 = 10i128.checked_pow(u32::try_from(frac.len()).ok()?)?;
        if exp > 0 {
            n = n.checked_mul(10i128.checked_pow(exp.unsigned_abs())?)?;
        } else if exp < 0 {
            d = d.checked_mul(10i128.checked_pow(exp.unsigned_abs())?)?;
        }
        Some(Rat::new(if neg { -n } else { n }, d))
    }

    fn add(self, o: Rat) -> Rat {
        Rat::new(self.n * o.d + o.n * self.d, self.d * o.d)
    }

    fn sub(self, o: Rat) -> Rat {
        self.add(Rat { n: -o.n, d: o.d })
    }

    fn neg(self) -> Rat {
        Rat {
            n: -self.n,
            d: self.d,
        }
    }

    fn mul(self, o: Rat) -> Rat {
        Rat::new(self.n * o.n, self.d * o.d)
    }

    fn floor(self) -> i128 {
        self.n.div_euclid(self.d)
    }

    fn ceil(self) -> i128 {
        -(-self.n).div_euclid(self.d)
    }
}

impl Ord for Rat {
    fn cmp(&self, o: &Rat) -> Ordering {
        (self.n * o.d).cmp(&(o.n * self.d))
    }
}

impl PartialOrd for Rat {
    fn partial_cmp(&self, o: &Rat) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// A value a condition reads.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Var {
    Zero,
    State,
    Now,
    Entered(String),
    Count(String),
    Attr(String),
    In(String),
}

/// A member of a finite domain: a state's or an enumeration value's name,
/// or a boolean.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Val {
    S(String),
    B(bool),
}

/// One atom of a case.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Atom {
    Null(Var),
    Present(Var),
    Dom(Var, BTreeSet<Val>),
    Eqv(Var, String),
    Nev(Var, String),
    /// u - v <= c, strict where marked.
    Le(Var, Var, Rat, bool),
    Op(String, char),
}

type Case = BTreeSet<Atom>;

fn product(case_lists: &[Vec<Case>]) -> Decided<Vec<Case>> {
    let mut out: Vec<Case> = vec![BTreeSet::new()];
    for cases in case_lists {
        let mut next = Vec::new();
        for a in &out {
            for b in cases {
                next.push(a.union(b).cloned().collect());
            }
        }
        if next.len() > LIMIT {
            return Err(TooLarge);
        }
        if next.is_empty() {
            return Ok(Vec::new());
        }
        out = next;
    }
    Ok(out)
}

fn union(lists: Vec<Vec<Case>>) -> Decided<Vec<Case>> {
    let out: Vec<Case> = lists.into_iter().flatten().collect();
    if out.len() > LIMIT {
        return Err(TooLarge);
    }
    Ok(out)
}

fn one(case: Vec<Atom>) -> Vec<Case> {
    vec![case.into_iter().collect()]
}

fn always() -> Vec<Case> {
    vec![BTreeSet::new()]
}

/// The kind of value a read has: finite (a domain), int, rat or eq, and
/// whether it may be absent.
#[derive(Clone)]
enum Info {
    Finite(BTreeSet<Val>, bool),
    Int(bool),
    Rat(&'static str, bool),
    Eq(bool),
}

impl Info {
    fn nullable(&self) -> bool {
        match self {
            Info::Finite(_, n) | Info::Int(n) | Info::Rat(_, n) | Info::Eq(n) => *n,
        }
    }

    /// The ordered kind, where it is one.
    fn order(&self) -> Option<&'static str> {
        match self {
            Info::Int(_) => Some("int"),
            Info::Rat(k, _) => Some(k),
            _ => None,
        }
    }
}

/// What the analysis knows of one type: the domain of each value a
/// condition may read.
struct TypeModel<'a> {
    tn: String,
    t: &'a Value,
    enums: &'a BTreeMap<String, Vec<String>>,
    attrs: Obj,
    derived: Obj,
    states: Obj,
    categories: BTreeSet<String>,
    open_states: BTreeSet<String>,
    inputs: BTreeMap<String, Option<Value>>,
    present: BTreeSet<String>,
    created: bool,
}

impl<'a> TypeModel<'a> {
    fn new(tn: &str, t: &'a Value, enums: &'a BTreeMap<String, Vec<String>>) -> TypeModel<'a> {
        let states = obj(t, "states").clone();
        let mut categories: BTreeSet<String> = states
            .values()
            .map(|v| str_or(v, "category", "None"))
            .collect();
        categories.insert("closed".into());
        let open_states = states
            .iter()
            .filter(|(_, v)| str_or(v, "category", "") != "closed" && !flag(v, "final"))
            .map(|(s, _)| s.clone())
            .collect();
        TypeModel {
            tn: tn.to_string(),
            t,
            enums,
            attrs: obj(t, "attributes").clone(),
            derived: obj(t, "derived_attributes").clone(),
            states,
            categories,
            open_states,
            inputs: BTreeMap::new(),
            present: BTreeSet::new(),
            created: false,
        }
    }

    fn kind(&self, spec: Option<&Value>, nullable: bool) -> Option<Info> {
        let spec = spec.filter(|s| super::doc::truthy(Some(s)))?;
        if let Some(r) = spec.get("reference") {
            return if py_str(r).ends_with("[]") {
                None
            } else {
                Some(Info::Eq(nullable))
            };
        }
        let typ = str_or(spec, "type", "");
        if typ.ends_with("[]") {
            return None;
        }
        if let Some(members) = self.enums.get(&typ) {
            return Some(Info::Finite(
                members.iter().map(|m| Val::S(m.clone())).collect(),
                nullable,
            ));
        }
        match typ.as_str() {
            "bool" => {
                return Some(Info::Finite(
                    BTreeSet::from([Val::B(true), Val::B(false)]),
                    nullable,
                ));
            }
            "counter" => return Some(Info::Int(false)),
            "int" => return Some(Info::Int(nullable)),
            "string" | "identity" | "file" => return Some(Info::Eq(nullable)),
            _ => {}
        }
        match typ.split('(').next().unwrap_or_default() {
            "decimal" => Some(Info::Rat("decimal", nullable)),
            "money" => Some(Info::Rat("money", nullable)),
            "timestamp" => Some(Info::Rat("timestamp", nullable)),
            "duration" => Some(Info::Rat("duration", nullable)),
            _ => None,
        }
    }

    fn var_info(&self, var: &Var) -> Option<Info> {
        match var {
            Var::State => Some(Info::Finite(
                self.states.keys().map(|s| Val::S(s.clone())).collect(),
                false,
            )),
            Var::Now => Some(Info::Rat("timestamp", false)),
            Var::Entered(_) => Some(Info::Rat("timestamp", true)),
            Var::Count(_) => Some(Info::Int(true)),
            Var::Attr(n) => {
                let spec = self.attrs.get(n);
                let optional = spec.is_some_and(|s| flag(s, "optional"));
                self.kind(spec, self.created || optional)
            }
            Var::In(n) => self.kind(
                self.inputs.get(n).and_then(Option::as_ref),
                !self.present.contains(n),
            ),
            Var::Zero => None,
        }
    }
}

/// A literal a condition names.
#[derive(Clone, Debug, PartialEq)]
enum Lit {
    State(String),
    Category(String),
    Enum(String),
    Number(Rat),
    Duration(Rat),
    Money(Rat),
    Str(String),
    Bool(bool),
}

impl Lit {
    fn kind(&self) -> &'static str {
        match self {
            Lit::State(_) => "state",
            Lit::Category(_) => "category",
            Lit::Enum(_) => "enum",
            Lit::Number(_) => "number",
            Lit::Duration(_) => "duration",
            Lit::Money(_) => "money",
            Lit::Str(_) => "string",
            Lit::Bool(_) => "bool",
        }
    }
}

/// A value node resolved.
#[derive(Clone, Debug, PartialEq)]
enum Res {
    Var(Var),
    Lit(Lit),
    Null,
}

/// What a transition leaves in an attribute: an expression, absence, or what
/// the analysis cannot follow.
#[derive(Clone)]
enum Written {
    Expr(Expr),
    Null,
    Unknown,
}

/// Names no expression parses, standing in a part's key for absence and for
/// a value not exact after a transition's writes (ADR-0279).
const NULL: &str = "\u{0}null";
const AFTER: &str = "\u{0}after";

/// A part read after a transition's writes where its value is not exact: a
/// part of its own, `flowlogic.py`'s `AFTER`.
fn after(node: Expr) -> Expr {
    Expr::Call(Box::new(Expr::Name(AFTER.into())), vec![node])
}

const SECONDS: &[(&str, i128)] = &[
    ("s", 1),
    ("min", 60),
    ("h", 3600),
    ("day", 86400),
    ("days", 86400),
    ("week", 604_800),
    ("weeks", 604_800),
];

fn num_rat(n: &Number) -> Option<Rat> {
    match n {
        Number::Int(i) => Some(Rat::int(i128::from(*i))),
        Number::Decimal(t) => Rat::parse(t),
    }
}

struct Analysis<'m, 'a> {
    m: &'m TypeModel<'a>,
    written: BTreeMap<String, Written>,
    fixed_state: Option<String>,
    /// The transition may change another object: a call, a create or a
    /// cascade.
    others: bool,
    opaque_seen: bool,
    inlining: BTreeSet<String>,
}

fn comparison(op: BinOp) -> Option<&'static str> {
    Some(match op {
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        _ => return None,
    })
}

fn not(want: char) -> char {
    match want {
        'T' => 'F',
        'F' => 'T',
        _ => 'U',
    }
}

impl<'m, 'a> Analysis<'m, 'a> {
    fn new(
        m: &'m TypeModel<'a>,
        written: BTreeMap<String, Written>,
        fixed_state: Option<String>,
        others: bool,
    ) -> Analysis<'m, 'a> {
        Analysis {
            m,
            written,
            fixed_state,
            others,
            opaque_seen: false,
            inlining: BTreeSet::new(),
        }
    }

    /// `e` with each name read as the value it stands for, so two parts are
    /// one where they read one value: a derived attribute as its expression,
    /// `this.x` as x, on a creation an attribute it takes as the input, and
    /// after a transition's writes each written attribute as the value
    /// written and `state` as the state entered. Where that value is not
    /// exact after the writes, the node is marked [`after`]: an attribute
    /// whose write is not followed, a `changed_since` naming a written
    /// attribute, `entered_at` once a state is entered, and a path through
    /// another object where the transition may change one. `bound` holds
    /// the loop names in scope (`flowlogic.py`'s `part`, ADR-0279).
    fn part(&self, e: &Expr, bound: &BTreeSet<String>, unfolding: &mut BTreeSet<String>) -> Expr {
        let m = self.m;
        let each =
            |x: &Expr, unfolding: &mut BTreeSet<String>| Box::new(self.part(x, bound, unfolding));
        match e {
            Expr::Name(n) if bound.contains(n) => e.clone(),
            Expr::Name(n) if m.attrs.contains_key(n) => match self.written.get(n) {
                Some(Written::Unknown) => after(e.clone()),
                Some(Written::Null) => Expr::Name(NULL.into()),
                Some(Written::Expr(w)) => self.part(w, &BTreeSet::new(), unfolding),
                None if m.created && m.inputs.contains_key(n) => {
                    Expr::Path(Box::new(Expr::Name("inputs".into())), n.clone())
                }
                None => e.clone(),
            },
            Expr::Name(n)
                if m.derived.contains_key(n) && !unfolding.contains(n) && !super::late() =>
            {
                let Ok(tree) = parse(&str_or(&m.derived[n.as_str()], "expression", "")) else {
                    return e.clone();
                };
                unfolding.insert(n.clone());
                let out = self.part(&tree, &BTreeSet::new(), unfolding);
                unfolding.remove(n);
                out
            }
            Expr::Name(n) if n == "state" => self
                .fixed_state
                .as_ref()
                .map_or_else(|| e.clone(), |s| Expr::Name(s.clone())),
            Expr::Path(base, member) if matches!(&**base, Expr::Name(b) if b == "this") => {
                self.part(&Expr::Name(member.clone()), bound, unfolding)
            }
            Expr::Path(base, member) => {
                let node = Expr::Path(each(base, unfolding), member.clone());
                if self.others && !self.own(base, bound) {
                    after(node)
                } else {
                    node
                }
            }
            Expr::Agg {
                kind,
                distinct,
                var,
                collection,
                filter,
                body,
            } => {
                let mut inner = bound.clone();
                inner.extend(var.iter().cloned());
                Expr::Agg {
                    kind: kind.clone(),
                    distinct: *distinct,
                    var: var.clone(),
                    collection: collection.as_ref().map(|c| each(c, unfolding)),
                    filter: filter
                        .as_ref()
                        .map(|f| Box::new(self.part(f, &inner, unfolding))),
                    body: body
                        .as_ref()
                        .map(|b| Box::new(self.part(b, &inner, unfolding))),
                }
            }
            Expr::Changed(names, ev) => {
                let node = Expr::Changed(names.clone(), each(ev, unfolding));
                if names.iter().any(|n| self.written.contains_key(n)) {
                    after(node)
                } else {
                    node
                }
            }
            Expr::Call(f, args) => {
                let fun = if matches!(&**f, Expr::Name(_)) {
                    f.clone()
                } else {
                    each(f, unfolding)
                };
                let node = Expr::Call(fun, args.iter().map(|a| *each(a, unfolding)).collect());
                if self.fixed_state.is_some() && matches!(&**f, Expr::Name(n) if n == "entered_at")
                {
                    after(node)
                } else {
                    node
                }
            }
            Expr::Set(items) => Expr::Set(items.iter().map(|i| *each(i, unfolding)).collect()),
            Expr::Not(x) => Expr::Not(each(x, unfolding)),
            Expr::Neg(x) => Expr::Neg(each(x, unfolding)),
            Expr::IsNull(x, negated) => Expr::IsNull(each(x, unfolding), *negated),
            Expr::In(a, b) => Expr::In(each(a, unfolding), each(b, unfolding)),
            Expr::Bin(op, a, b) => Expr::Bin(*op, each(a, unfolding), each(b, unfolding)),
            Expr::If(c, a, b) => {
                Expr::If(each(c, unfolding), each(a, unfolding), each(b, unfolding))
            }
            // a metric is a guard's alone: a part read before the request
            _ => e.clone(),
        }
    }

    /// Whether a path from `base` reads this object, an input or a literal,
    /// rather than another object.
    fn own(&self, base: &Expr, bound: &BTreeSet<String>) -> bool {
        matches!(base, Expr::Name(b) if !bound.contains(b)
            && (b == "inputs" || b == "state" || self.m.enums.contains_key(b) || *b == self.m.tn))
    }

    fn key(&self, e: &Expr) -> String {
        format!("{:?}", self.part(e, &BTreeSet::new(), &mut BTreeSet::new()))
    }

    /// A count over one of the object's own collections, keyed as the
    /// collection and filter it reads.
    fn count_key(&self, c: &Expr, var: &str, filter: Option<&Expr>) -> String {
        let inner = BTreeSet::from([var.to_string()]);
        let f = filter.map(|f| self.part(f, &inner, &mut BTreeSet::new()));
        format!(
            "{:?}|{f:?}",
            self.part(c, &BTreeSet::new(), &mut BTreeSet::new())
        )
    }

    fn resolve_written(&self, w: &Written) -> Option<Res> {
        match w {
            Written::Expr(e) => self.resolve(e),
            Written::Null => Some(Res::Null),
            Written::Unknown => None,
        }
    }

    fn resolve(&self, e: &Expr) -> Option<Res> {
        let m = self.m;
        match e {
            Expr::Name(n) => {
                if n == "now" {
                    return Some(Res::Var(Var::Now));
                }
                if n == "state" {
                    return Some(match &self.fixed_state {
                        Some(s) => Res::Lit(Lit::State(s.clone())),
                        None => Res::Var(Var::State),
                    });
                }
                if m.attrs.contains_key(n) {
                    return match self.written.get(n) {
                        Some(w) => self.resolve_written(w),
                        // a creation's object has no earlier value: a member
                        // it takes as an input is the input (ADR-0251)
                        None if m.created && m.inputs.contains_key(n) => {
                            Some(Res::Var(Var::In(n.clone())))
                        }
                        None => Some(Res::Var(Var::Attr(n.clone()))),
                    };
                }
                if m.states.contains_key(n) {
                    return Some(Res::Lit(Lit::State(n.clone())));
                }
                if m.categories.contains(n) {
                    return Some(Res::Lit(Lit::Category(n.clone())));
                }
                None
            }
            Expr::Path(base, member) => match &**base {
                Expr::Name(b) if b == "inputs" => m
                    .inputs
                    .contains_key(member)
                    .then(|| Res::Var(Var::In(member.clone()))),
                Expr::Name(b) if b == "this" => self.resolve(&Expr::Name(member.clone())),
                Expr::Name(b) if m.enums.get(b).is_some_and(|ms| ms.contains(member)) => {
                    Some(Res::Lit(Lit::Enum(member.clone())))
                }
                Expr::Name(b) if *b == m.tn && m.states.contains_key(member) => {
                    Some(Res::Lit(Lit::State(member.clone())))
                }
                Expr::Name(b) if b == "state" && member == "category" => {
                    self.fixed_state.as_ref().map(|s| {
                        Res::Lit(Lit::Category(str_or(
                            m.states.get(s).unwrap_or(&Value::Null),
                            "category",
                            "None",
                        )))
                    })
                }
                _ => None,
            },
            Expr::Call(f, args)
                if matches!(&**f, Expr::Name(n) if n == "entered_at")
                    && args.len() == 1
                    && matches!(&args[0], Expr::Name(s) if m.states.contains_key(s))
                    && self.fixed_state.is_none() =>
            {
                let Expr::Name(s) = &args[0] else { return None };
                Some(Res::Var(Var::Entered(s.clone())))
            }
            Expr::Num(n) => num_rat(n).map(|r| Res::Lit(Lit::Number(r))),
            Expr::Dur(n, unit) => {
                let secs = SECONDS.iter().find(|(u, _)| u == unit).map(|(_, s)| *s)?;
                num_rat(n).map(|r| Res::Lit(Lit::Duration(r.mul(Rat::int(secs)))))
            }
            Expr::Money(_, amount) => Rat::parse(amount).map(|r| Res::Lit(Lit::Money(r))),
            Expr::Str(s) => Some(Res::Lit(Lit::Str(s.clone()))),
            Expr::Bool(b) => Some(Res::Lit(Lit::Bool(*b))),
            _ => None,
        }
    }

    fn order_kind(&self, var: &Var) -> Option<&'static str> {
        self.m.var_info(var).and_then(|i| i.order())
    }

    /// An ordered value as (variable, constant, kind): x, a literal, or x ±
    /// a constant.
    fn linear(&self, e: &Expr) -> Option<(Option<Var>, Rat, &'static str)> {
        if let Expr::Bin(op @ (BinOp::Add | BinOp::Sub), l, r) = e {
            let (a, b) = (self.linear(l)?, self.linear(r)?);
            if b.0.is_some() {
                return None;
            }
            let kind = sum_kind(a.2, b.2)?;
            let c = if *op == BinOp::Add {
                a.1.add(b.1)
            } else {
                a.1.sub(b.1)
            };
            return Some((a.0, c, kind));
        }
        if let Expr::Agg {
            kind,
            var: Some(v),
            collection: Some(c),
            filter,
            ..
        } = e
            && kind == "count"
            && self.own_collection(c)
        {
            return Some((
                Some(Var::Count(self.count_key(c, v, filter.as_deref()))),
                Rat::int(0),
                "int",
            ));
        }
        match self.resolve(e)? {
            Res::Null => None,
            Res::Var(v) => self.order_kind(&v).map(|k| (Some(v), Rat::int(0), k)),
            Res::Lit(Lit::Number(r)) => Some((None, r, "number")),
            Res::Lit(Lit::Duration(r)) => Some((None, r, "duration")),
            Res::Lit(Lit::Money(r)) => Some((None, r, "money")),
            Res::Lit(_) => None,
        }
    }

    fn own_collection(&self, e: &Expr) -> bool {
        matches!(e, Expr::Name(n) if self.m.attrs.contains_key(n) || obj(self.m.t, "observations").contains_key(n))
    }

    fn finite_value(info: Option<&Info>, lit: &Res) -> Option<Val> {
        let (Some(Info::Finite(domain, _)), Res::Lit(l)) = (info, lit) else {
            return None;
        };
        let v = match l {
            Lit::State(s) | Lit::Enum(s) => Val::S(s.clone()),
            Lit::Bool(b) => Val::B(*b),
            _ => return None,
        };
        domain.contains(&v).then_some(v)
    }

    fn opaque(&mut self, e: &Expr, want: char) -> Vec<Case> {
        self.opaque_seen = true;
        one(vec![Atom::Op(self.key(e), want)])
    }

    /// The conjunctions of atoms under which e has the value want: T, F or U.
    fn cases(&mut self, e: &Expr, want: char) -> Decided<Vec<Case>> {
        match e {
            // `this.x` is x, a derived attribute's expression among them
            Expr::Path(base, member) if matches!(&**base, Expr::Name(b) if b == "this") => {
                return self.cases(&Expr::Name(member.clone()), want);
            }
            Expr::Bin(BinOp::Implies, a, b) => {
                let rewritten = Expr::Bin(BinOp::Or, Box::new(Expr::Not(a.clone())), b.clone());
                return self.cases(&rewritten, want);
            }
            Expr::Bin(op @ (BinOp::And | BinOp::Or), a, b) => {
                let (short, other) = if *op == BinOp::And {
                    ('F', 'T')
                } else {
                    ('T', 'F')
                };
                if want == short {
                    return union(vec![self.cases(a, short)?, self.cases(b, short)?]);
                }
                if want == other {
                    return product(&[self.cases(a, other)?, self.cases(b, other)?]);
                }
                let first = product(&[self.cases(a, 'U')?, self.cases(b, other)?])?;
                let second = product(&[self.cases(a, other)?, self.cases(b, 'U')?])?;
                let third = product(&[self.cases(a, 'U')?, self.cases(b, 'U')?])?;
                return union(vec![first, second, third]);
            }
            Expr::Not(x) => return self.cases(x, not(want)),
            Expr::Bool(b) => {
                return Ok(if want == if *b { 'T' } else { 'F' } {
                    always()
                } else {
                    Vec::new()
                });
            }
            Expr::IsNull(x, negated) => {
                let r = self.resolve(x);
                if r == Some(Res::Null) {
                    return Ok(if want == if *negated { 'F' } else { 'T' } {
                        always()
                    } else {
                        Vec::new()
                    });
                }
                if let Some(Res::Var(v)) = &r
                    && self.m.var_info(v).is_some()
                {
                    if want == 'U' {
                        return Ok(Vec::new());
                    }
                    let absent = (want == 'T') != *negated;
                    return Ok(one(vec![if absent {
                        Atom::Null(v.clone())
                    } else {
                        Atom::Present(v.clone())
                    }]));
                }
                return Ok(self.opaque(e, want));
            }
            Expr::Bin(op, l, r) if comparison(*op).is_some() => {
                return self.compare(e, *op, l, r, want);
            }
            Expr::In(l, r) => return self.membership(e, l, r, want),
            Expr::Agg {
                kind,
                var: Some(v),
                collection: Some(c),
                filter,
                body: None,
                ..
            } if (kind == "none" || kind == "any") && self.own_collection(c) => {
                let count = Var::Count(self.count_key(c, v, filter.as_deref()));
                if want == 'U' {
                    return Ok(one(vec![Atom::Null(count)]));
                }
                let empty = (kind == "none") == (want == 'T');
                let bound = if empty {
                    Atom::Le(count.clone(), Var::Zero, Rat::int(0), false)
                } else {
                    Atom::Le(Var::Zero, count.clone(), Rat::int(-1), false)
                };
                return Ok(one(vec![Atom::Present(count), bound]));
            }
            Expr::Name(n) if n == "open" && !self.m.attrs.contains_key("open") => {
                if want == 'U' {
                    return Ok(Vec::new());
                }
                if let Some(s) = &self.fixed_state {
                    return Ok(if self.m.open_states.contains(s) == (want == 'T') {
                        always()
                    } else {
                        Vec::new()
                    });
                }
                let allowed: BTreeSet<Val> = if want == 'T' {
                    self.m
                        .open_states
                        .iter()
                        .map(|s| Val::S(s.clone()))
                        .collect()
                } else {
                    self.m
                        .states
                        .keys()
                        .filter(|s| !self.m.open_states.contains(*s))
                        .map(|s| Val::S(s.clone()))
                        .collect()
                };
                return Ok(one(vec![Atom::Dom(Var::State, allowed)]));
            }
            Expr::Name(n)
                if self.m.derived.contains_key(n)
                    && !self.inlining.contains(n)
                    && !super::late() =>
            {
                let text = str_or(&self.m.derived[n.as_str()], "expression", "");
                let Ok(tree) = parse(&text) else {
                    return Ok(self.opaque(e, want));
                };
                self.inlining.insert(n.clone());
                let out = self.cases(&tree, want);
                self.inlining.remove(n);
                return out;
            }
            _ => {}
        }
        let r = self.resolve(e);
        if let Some(Res::Var(v)) = &r
            && let Some(Info::Finite(d, _)) = self.m.var_info(v)
            && d == BTreeSet::from([Val::B(true), Val::B(false)])
        {
            if want == 'U' {
                return Ok(one(vec![Atom::Null(v.clone())]));
            }
            return Ok(one(vec![
                Atom::Present(v.clone()),
                Atom::Dom(v.clone(), BTreeSet::from([Val::B(want == 'T')])),
            ]));
        }
        if let Some(Res::Lit(Lit::Bool(b))) = r {
            return Ok(if want == if b { 'T' } else { 'F' } {
                always()
            } else {
                Vec::new()
            });
        }
        Ok(self.opaque(e, want))
    }

    fn compare(
        &mut self,
        e: &Expr,
        op: BinOp,
        left: &Expr,
        right: &Expr,
        want: char,
    ) -> Decided<Vec<Case>> {
        let (a, b) = (self.resolve(left), self.resolve(right));
        let eq_ne = matches!(op, BinOp::Eq | BinOp::Ne);
        if a == Some(Res::Null) || b == Some(Res::Null) {
            return Ok(if want == 'U' { always() } else { Vec::new() });
        }
        if let (Some(Res::Lit(x)), Some(Res::Lit(y))) = (&a, &b)
            && eq_ne
            && x.kind() == y.kind()
        {
            let truth = (x == y) == (op == BinOp::Eq);
            return Ok(if want == if truth { 'T' } else { 'F' } {
                always()
            } else {
                Vec::new()
            });
        }
        let var_lit = match (&a, &b) {
            (Some(Res::Var(v)), Some(l @ Res::Lit(_)))
            | (Some(l @ Res::Lit(_)), Some(Res::Var(v))) => Some((v.clone(), l.clone())),
            _ => None,
        };
        if let Some((v, lit)) = var_lit
            && eq_ne
        {
            let info = self.m.var_info(&v);
            if let Some(value) = Analysis::finite_value(info.as_ref(), &lit) {
                if want == 'U' {
                    return Ok(one(vec![Atom::Null(v)]));
                }
                let keep = (op == BinOp::Eq) == (want == 'T');
                let Some(Info::Finite(domain, _)) = info else {
                    return Ok(Vec::new());
                };
                let allowed: BTreeSet<Val> = if keep {
                    BTreeSet::from([value])
                } else {
                    domain.into_iter().filter(|x| *x != value).collect()
                };
                return Ok(one(vec![Atom::Present(v.clone()), Atom::Dom(v, allowed)]));
            }
            if let (Some(Info::Eq(_)), Res::Lit(Lit::Str(s))) = (&info, &lit) {
                if want == 'U' {
                    return Ok(one(vec![Atom::Null(v)]));
                }
                let keep = (op == BinOp::Eq) == (want == 'T');
                let atom = if keep {
                    Atom::Eqv(v.clone(), s.clone())
                } else {
                    Atom::Nev(v.clone(), s.clone())
                };
                return Ok(one(vec![Atom::Present(v), atom]));
            }
        }
        if let Expr::Path(base, member) = left
            && matches!(&**base, Expr::Name(n) if n == "state")
            && member == "category"
            && eq_ne
            && let Some(Res::Lit(Lit::Category(c))) = &b
            && self.fixed_state.is_none()
        {
            if want == 'U' {
                return Ok(Vec::new());
            }
            let with_it: BTreeSet<String> = self
                .m
                .states
                .iter()
                .filter(|(_, v)| str_or(v, "category", "None") == *c)
                .map(|(s, _)| s.clone())
                .collect();
            let allowed: BTreeSet<Val> = if (op == BinOp::Eq) == (want == 'T') {
                with_it.iter().map(|s| Val::S(s.clone())).collect()
            } else {
                self.m
                    .states
                    .keys()
                    .filter(|s| !with_it.contains(*s))
                    .map(|s| Val::S(s.clone()))
                    .collect()
            };
            return Ok(one(vec![Atom::Dom(Var::State, allowed)]));
        }
        if let (Some(la), Some(lb)) = (self.linear(left), self.linear(right))
            && (la.0.is_some() || lb.0.is_some())
            && comparable(la.2, lb.2)
        {
            return Ok(ordered(op, la, lb, want));
        }
        Ok(self.opaque(e, want))
    }

    fn membership(
        &mut self,
        e: &Expr,
        left: &Expr,
        right: &Expr,
        want: char,
    ) -> Decided<Vec<Case>> {
        let a = self.resolve(left);
        let info = match &a {
            Some(Res::Var(v)) => self.m.var_info(v),
            _ => None,
        };
        if let (Some(Res::Lit(x)), Expr::Set(items)) = (&a, right) {
            let values: Vec<Option<Res>> = items.iter().map(|i| self.resolve(i)).collect();
            if values
                .iter()
                .all(|v| matches!(v, Some(Res::Lit(l)) if l.kind() == x.kind()))
            {
                let truth = values
                    .iter()
                    .any(|v| matches!(v, Some(Res::Lit(l)) if l == x));
                return Ok(if want == if truth { 'T' } else { 'F' } {
                    always()
                } else {
                    Vec::new()
                });
            }
            return Ok(self.opaque(e, want));
        }
        let Some(Info::Finite(domain, _)) = &info else {
            return Ok(self.opaque(e, want));
        };
        let Some(Res::Var(v)) = a else {
            return Ok(self.opaque(e, want));
        };
        let members: BTreeSet<Val> = match right {
            Expr::Set(items) => {
                let values: Vec<Option<Val>> = items
                    .iter()
                    .map(|i| {
                        Analysis::finite_value(info.as_ref(), &self.resolve(i).unwrap_or(Res::Null))
                    })
                    .collect();
                if values.iter().any(Option::is_none) {
                    return Ok(self.opaque(e, want));
                }
                values.into_iter().flatten().collect()
            }
            Expr::Name(n)
                if self.m.enums.get(n).is_some_and(|ms| {
                    ms.iter()
                        .map(|m| Val::S(m.clone()))
                        .collect::<BTreeSet<_>>()
                        == *domain
                }) =>
            {
                domain.clone()
            }
            _ => return Ok(self.opaque(e, want)),
        };
        if want == 'U' {
            return Ok(one(vec![Atom::Null(v)]));
        }
        let allowed: BTreeSet<Val> = if want == 'T' {
            members
        } else {
            domain.difference(&members).cloned().collect()
        };
        Ok(one(vec![Atom::Present(v.clone()), Atom::Dom(v, allowed)]))
    }
}

/// u + cu op v + cv, as difference bounds on u - v.
fn ordered(
    op: BinOp,
    la: (Option<Var>, Rat, &'static str),
    lb: (Option<Var>, Rat, &'static str),
    want: char,
) -> Vec<Case> {
    let (u, cu, _) = la;
    let (v, cv, _) = lb;
    if want == 'U' {
        return [u, v]
            .into_iter()
            .flatten()
            .map(|x| [Atom::Null(x)].into_iter().collect())
            .collect();
    }
    let present: Vec<Atom> = [u.clone(), v.clone()]
        .into_iter()
        .flatten()
        .map(Atom::Present)
        .collect();
    let (uu, vv, c) = (u.unwrap_or(Var::Zero), v.unwrap_or(Var::Zero), cv.sub(cu));
    let le = |strict: bool| Atom::Le(uu.clone(), vv.clone(), c, strict);
    let ge = |strict: bool| Atom::Le(vv.clone(), uu.clone(), c.neg(), strict);
    let op = if want == 'T' {
        op
    } else {
        match op {
            BinOp::Le => BinOp::Gt,
            BinOp::Lt => BinOp::Ge,
            BinOp::Ge => BinOp::Lt,
            BinOp::Gt => BinOp::Le,
            BinOp::Eq => BinOp::Ne,
            _ => BinOp::Eq,
        }
    };
    let relation: Vec<Vec<Atom>> = match op {
        BinOp::Le => vec![vec![le(false)]],
        BinOp::Lt => vec![vec![le(true)]],
        BinOp::Ge => vec![vec![ge(false)]],
        BinOp::Gt => vec![vec![ge(true)]],
        BinOp::Eq => vec![vec![le(false), ge(false)]],
        _ => vec![vec![le(true)], vec![ge(true)]],
    };
    relation
        .into_iter()
        .map(|atoms| present.iter().cloned().chain(atoms).collect())
        .collect()
}

const NUMERIC: &[&str] = &["int", "decimal"];

/// The kind of a ± b, b a constant (`declaration-syntax.md` §8.3).
fn sum_kind(a: &'static str, b: &'static str) -> Option<&'static str> {
    if a == "timestamp" && b == "duration" {
        return Some("timestamp");
    }
    if a == b && ["duration", "money", "number"].contains(&a) {
        return Some(a);
    }
    if b == "number" && NUMERIC.contains(&a) {
        return Some(a);
    }
    None
}

/// Whether two ordered kinds compare.
fn comparable(a: &str, b: &str) -> bool {
    if a == b {
        return a != "number";
    }
    if a == "number" || b == "number" {
        let other = if a == "number" { b } else { a };
        return NUMERIC.contains(&other) || other == "money";
    }
    false
}

/// Whether a conjunction of atoms can hold.
fn satisfiable(case: &Case, model: &TypeModel<'_>) -> bool {
    let mut presence: BTreeMap<&Var, bool> = BTreeMap::new();
    let mut domains: BTreeMap<&Var, BTreeSet<Val>> = BTreeMap::new();
    let mut equal: BTreeMap<&Var, &String> = BTreeMap::new();
    let mut unequal: BTreeMap<&Var, BTreeSet<&String>> = BTreeMap::new();
    let mut bounds: Vec<(Var, Var, Rat, bool)> = Vec::new();
    let mut opaque: BTreeMap<&String, char> = BTreeMap::new();
    for atom in case {
        match atom {
            Atom::Null(v) | Atom::Present(v) => {
                let p = matches!(atom, Atom::Present(_));
                if *presence.entry(v).or_insert(p) != p {
                    return false;
                }
            }
            Atom::Dom(v, d) => {
                let cur = domains.entry(v).or_insert_with(|| d.clone());
                *cur = cur.intersection(d).cloned().collect();
            }
            Atom::Eqv(v, s) => {
                if *equal.entry(v).or_insert(s) != s {
                    return false;
                }
            }
            Atom::Nev(v, s) => {
                unequal.entry(v).or_default().insert(s);
            }
            Atom::Le(u, v, c, s) => bounds.push((u.clone(), v.clone(), *c, *s)),
            Atom::Op(k, w) => {
                if *opaque.entry(k).or_insert(*w) != *w {
                    return false;
                }
            }
        }
    }
    for (v, present) in &presence {
        if !present && model.var_info(v).is_some_and(|i| !i.nullable()) {
            return false;
        }
    }
    if domains.values().any(BTreeSet::is_empty) {
        return false;
    }
    if equal
        .iter()
        .any(|(v, s)| unequal.get(v).is_some_and(|u| u.contains(s)))
    {
        return false;
    }
    let counts: BTreeSet<Var> = bounds
        .iter()
        .flat_map(|b| [b.0.clone(), b.1.clone()])
        .filter(|x| matches!(x, Var::Count(_)))
        .collect();
    bounds.extend(
        counts
            .into_iter()
            .map(|c| (Var::Zero, c, Rat::int(0), false)),
    );
    consistent(&bounds, model)
}

/// Difference bounds u - v <= c have a solution, decided for each part the
/// bounds join apart from the constant zero: a negative cycle through zero
/// that visits two parts is two cycles, each within one part, one of them
/// negative, so this is exact, and each part tests alone whether its values
/// are all integers (ADR-0280).
fn consistent(bounds: &[(Var, Var, Rat, bool)], model: &TypeModel<'_>) -> bool {
    fn find<'v>(parent: &mut BTreeMap<&'v Var, &'v Var>, x: &'v Var) -> &'v Var {
        let mut x = x;
        loop {
            let p = *parent.entry(x).or_insert(x);
            if p == x {
                return x;
            }
            let up = *parent.entry(p).or_insert(p);
            parent.insert(x, up);
            x = up;
        }
    }
    let mut parent: BTreeMap<&Var, &Var> = BTreeMap::new();
    for (u, v, _, _) in bounds {
        if *u != Var::Zero && *v != Var::Zero {
            let (a, b) = (find(&mut parent, u), find(&mut parent, v));
            parent.insert(a, b);
        }
    }
    let mut parts: BTreeMap<&Var, Vec<(Var, Var, Rat, bool)>> = BTreeMap::new();
    for b in bounds {
        let root = find(&mut parent, if b.0 == Var::Zero { &b.1 } else { &b.0 });
        parts.entry(root).or_default().push(b.clone());
    }
    parts.values().all(|part| cycle_free(part, model))
}

/// Difference bounds have no negative cycle (Floyd–Warshall); over integers
/// a strict bound tightens to the next integer.
fn cycle_free(bounds: &[(Var, Var, Rat, bool)], model: &TypeModel<'_>) -> bool {
    if bounds.is_empty() {
        return true;
    }
    let nodes: Vec<Var> = bounds
        .iter()
        .flat_map(|b| [b.0.clone(), b.1.clone()])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let index: BTreeMap<&Var, usize> = nodes.iter().enumerate().map(|(i, n)| (n, i)).collect();
    let integer = nodes
        .iter()
        .all(|n| *n == Var::Zero || matches!(model.var_info(n), Some(Info::Int(_))));
    let size = nodes.len();
    let mut dist: Vec<Vec<Option<(Rat, bool)>>> = vec![vec![None; size]; size];
    for (i, row) in dist.iter_mut().enumerate() {
        row[i] = Some((Rat::int(0), false));
    }
    let better = |a: (Rat, bool), b: Option<(Rat, bool)>| match b {
        None => true,
        Some(b) => a.0 < b.0 || (a.0 == b.0 && a.1 && !b.1),
    };
    for (u, v, c, strict) in bounds {
        let w = if integer {
            (
                Rat::int(if *strict { c.ceil() - 1 } else { c.floor() }),
                false,
            )
        } else {
            (*c, *strict)
        };
        let (i, j) = (index[v], index[u]);
        if better(w, dist[i][j]) {
            dist[i][j] = Some(w);
        }
    }
    for k in 0..size {
        let row_k = dist[k].clone();
        for row in &mut dist {
            let Some(ik) = row[k] else { continue };
            for (cell, kj) in row.iter_mut().zip(&row_k) {
                let Some(kj) = kj else { continue };
                let w = (ik.0.add(kj.0), ik.1 || kj.1);
                if better(w, *cell) {
                    *cell = Some(w);
                }
            }
        }
    }
    (0..size).all(|i| dist[i][i].is_some_and(|(d, s)| d > Rat::int(0) || (d == Rat::int(0) && !s)))
}

type Requirement = (String, Vec<Case>);

/// What an atom reads: the values it constrains, or an opaque part's key.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Read<'a> {
    Value(&'a Var),
    Part(&'a str),
}

fn reads_of(atom: &Atom) -> Vec<Read<'_>> {
    match atom {
        Atom::Le(u, v, _, _) => [u, v]
            .into_iter()
            .filter(|x| **x != Var::Zero)
            .map(Read::Value)
            .collect(),
        Atom::Op(k, _) => vec![Read::Part(k)],
        Atom::Null(v) | Atom::Present(v) | Atom::Dom(v, _) | Atom::Eqv(v, _) | Atom::Nev(v, _) => {
            vec![Read::Value(v)]
        }
    }
}

/// The requirements, by index, as groups that share nothing, no value and
/// no opaque part, each in the order of its first: KLEE's constraint
/// independence. Requirements that share nothing cannot constrain each
/// other, so a set can hold exactly when each group can (ADR-0280).
fn groups(reqs: &[Requirement]) -> Vec<Vec<usize>> {
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut parent: Vec<usize> = (0..reqs.len()).collect();
    let mut owner: BTreeMap<Read<'_>, usize> = BTreeMap::new();
    for (i, (_, cases)) in reqs.iter().enumerate() {
        for key in cases.iter().flatten().flat_map(reads_of) {
            let j = *owner.entry(key).or_insert(i);
            let (a, b) = (find(&mut parent, i), find(&mut parent, j));
            parent[a] = b;
        }
    }
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut at: BTreeMap<usize, usize> = BTreeMap::new();
    for i in 0..reqs.len() {
        let root = find(&mut parent, i);
        let g = *at.entry(root).or_insert_with(|| {
            out.push(Vec::new());
            out.len() - 1
        });
        out[g].push(i);
    }
    out
}

/// Whether requirements can all hold at once, each group of them that shares
/// nothing decided alone: a group that cannot hold decides the set, and
/// otherwise one past the bound leaves it undecided.
fn any_satisfiable(reqs: &[Requirement], model: &TypeModel<'_>) -> Decided<bool> {
    let mut too_large = false;
    for group in groups(reqs) {
        let lists: Vec<Vec<Case>> = group.iter().map(|&i| reqs[i].1.clone()).collect();
        match product(&lists) {
            Ok(cases) if !cases.iter().any(|c| satisfiable(c, model)) => return Ok(false),
            Ok(_) => {}
            Err(TooLarge) => too_large = true,
        }
    }
    if too_large { Err(TooLarge) } else { Ok(true) }
}

/// Whether every atom of a case is given, or a domain within a given one.
fn implied(case: &Case, given: &Case) -> bool {
    let domains: BTreeMap<&Var, &BTreeSet<Val>> = given
        .iter()
        .filter_map(|a| {
            if let Atom::Dom(v, d) = a {
                Some((v, d))
            } else {
                None
            }
        })
        .collect();
    case.iter().all(|a| {
        given.contains(a)
            || matches!(a, Atom::Dom(v, d) if domains.get(v).is_some_and(|g| g.is_subset(d)))
    })
}

/// The requirements with what `given` already settles taken out.
fn fixed(reqs: Vec<Requirement>, given: &Case, model: &TypeModel<'_>) -> Vec<Requirement> {
    reqs.into_iter()
        .filter(|(_, cases)| !cases.iter().any(|c| implied(c, given)))
        .map(|(n, cases)| {
            let kept = cases
                .into_iter()
                .filter(|c| satisfiable(&c.union(given).cloned().collect(), model))
                .collect();
            (n, kept)
        })
        .collect()
}

/// A smallest set of the requirements, by deletion, that cannot hold,
/// sought within the first group of them that cannot hold alone: sought over
/// the whole set, a trial would decide a group past the bound, and leave
/// what was proved undecided (ADR-0280). A subset of a group within the
/// bound is within it too.
fn core(reqs: &[Requirement], model: &TypeModel<'_>) -> Decided<Vec<Requirement>> {
    for group in groups(reqs) {
        let members: Vec<Requirement> = group.iter().map(|&i| reqs[i].clone()).collect();
        if !matches!(any_satisfiable(&members, model), Ok(false)) {
            continue;
        }
        let mut kept = members.clone();
        for r in &members {
            let Some(at) = kept.iter().position(|k| k.0 == r.0 && k.1 == r.1) else {
                continue;
            };
            let mut trial = kept.clone();
            trial.remove(at);
            if !trial.is_empty() && !any_satisfiable(&trial, model)? {
                kept = trial;
            }
        }
        return Ok(kept);
    }
    Ok(reqs.to_vec())
}

/// A type's invariants by name, a state's required attributes among them.
fn state_invariants(t: &Value) -> Vec<(String, Expr)> {
    let mut out: Vec<(String, Expr)> = obj(t, "invariants")
        .iter()
        .filter_map(|(n, i)| {
            parse(&str_or(i, "expression", ""))
                .ok()
                .map(|e| (n.clone(), e))
        })
        .collect();
    for (s, v) in obj(t, "states") {
        let req = strings(v, "required_attributes");
        if req.is_empty() {
            continue;
        }
        let text = format!(
            "state != {s} or ({})",
            req.iter()
                .map(|a| format!("{a} is not null"))
                .collect::<Vec<_>>()
                .join(" and ")
        );
        if let Ok(e) = parse(&text) {
            let name = format!("{}_invariant", s.to_lowercase());
            match out.iter_mut().find(|(n, _)| *n == name) {
                Some(slot) => slot.1 = e,
                None => out.push((name, e)),
            }
        }
    }
    // an invariant `taken_by` generates, left out where no other invariant
    // reads its attribute: absence satisfies it and only its own transition
    // writes the attribute, so it holds together with any other, and
    // expanding it only multiplied the cases past what step 5 decides
    // (ADR-0176); `flowlogic.py`'s `state_invariants`
    // and one a text's `not_blank` generates, which reads its attribute
    // alone and holds where the text is absent or not blank (ADR-0274)
    let (transitions, attributes) = (obj(t, "transitions"), obj(t, "attributes"));
    let invariants = obj(t, "invariants");
    let generated = |n: &str| -> Option<String> {
        if let Some(x) = n.strip_suffix("_taken_by")
            && transitions.contains_key(x)
            && attributes.contains_key(&format!("{x}_by_kind"))
        {
            return Some(format!("{x}_by_kind"));
        }
        let a = invariants
            .get(n)
            .and_then(|i| i.get(super::view::NOT_BLANK))
            .map(py_str)?;
        attributes
            .get(&a)
            .filter(|s| s.get("not_blank").and_then(Value::as_bool) == Some(true))
            .map(|_| a)
    };
    let read: BTreeSet<String> = out
        .iter()
        .filter(|(n, _)| generated(n).is_none())
        .flat_map(|(_, e)| names_in(e))
        .collect();
    out.retain(|(n, _)| generated(n).is_none_or(|a| read.contains(&a)));
    out
}

fn together(names: &[String]) -> &'static str {
    if names.len() == 2 { "both" } else { "all" }
}

fn holds(a: &mut Analysis<'_, '_>, tree: &Expr) -> Decided<Vec<Case>> {
    union(vec![a.cases(tree, 'T')?, a.cases(tree, 'U')?])
}

/// The bare names an expression reads at the start of a path.
fn names_in(e: &Expr) -> BTreeSet<String> {
    match e {
        Expr::Name(n) => BTreeSet::from([n.clone()]),
        // `this.x` reads x
        Expr::Path(b, m) if matches!(&**b, Expr::Name(t) if t == "this") => {
            BTreeSet::from([m.clone()])
        }
        Expr::Path(b, _) => names_in(b),
        Expr::Agg {
            var,
            collection,
            filter,
            body,
            ..
        } => {
            let mut inner: BTreeSet<String> = [filter, body]
                .into_iter()
                .flatten()
                .flat_map(|x| names_in(x))
                .collect();
            if let Some(v) = var {
                inner.remove(v);
            }
            let mut out: BTreeSet<String> = collection.iter().flat_map(|c| names_in(c)).collect();
            out.extend(inner);
            out
        }
        Expr::Metric { binds, .. } => binds.iter().flat_map(|(_, x)| names_in(x)).collect(),
        Expr::Changed(names, ev) => names.iter().cloned().chain(names_in(ev)).collect(),
        Expr::Call(f, args) => {
            let mut out = if matches!(&**f, Expr::Name(_)) {
                BTreeSet::new()
            } else {
                names_in(f)
            };
            out.extend(args.iter().flat_map(names_in));
            out
        }
        other => super::names::children(other)
            .into_iter()
            .flat_map(names_in)
            .collect(),
    }
}

/// The bare names an expression reads, a derived attribute's own among them.
fn reads(e: &Expr, derived: &Obj) -> BTreeSet<String> {
    let mut out = names_in(e);
    let mut todo: Vec<String> = out
        .iter()
        .filter(|n| derived.contains_key(*n))
        .cloned()
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(d) = todo.pop() {
        if !seen.insert(d.clone()) {
            continue;
        }
        let Ok(tree) = parse(&str_or(&derived[d.as_str()], "expression", "")) else {
            continue;
        };
        for n in names_in(&tree) {
            if derived.contains_key(&n) {
                todo.push(n.clone());
            }
            out.insert(n);
        }
    }
    out
}

/// The attributes effect steps write, within a loop's steps too.
fn writes(steps: &[Value]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for st in steps {
        if let Some(a) = st.get("assign") {
            out.insert(str_or(a, "location", ""));
        } else if let Some(Value::Array(cleared)) = st.get("clear") {
            out.extend(cleared.iter().map(py_str));
        } else if let Some(a) = st.get("add").or_else(|| st.get("remove")) {
            out.insert(str_or(a, "location", ""));
        } else if let Some(Value::Array(inner)) = st.get("foreach").and_then(|f| f.get("steps")) {
            out.extend(writes(inner));
        }
    }
    out
}

/// Whether taking a transition may change another object: a call or a
/// create in its effect, within a loop too, a cascade on it from one of its
/// type's parts, or a follow of it, `followed` holding the type's
/// transitions a follows clause names (`flow-format.md` §4.3, §4.8;
/// ADR-0292).
fn changes_others(xn: &str, x: &Value, t: &Value, followed: &BTreeSet<String>) -> bool {
    fn calls(steps: &[Value]) -> bool {
        steps.iter().any(|st| {
            st.get("call").is_some()
                || st.get("create").is_some()
                || matches!(st.get("foreach").and_then(|f| f.get("steps")), Some(Value::Array(inner)) if calls(inner))
        })
    }
    let effect = match x.get("effect") {
        Some(Value::Array(steps)) => calls(steps),
        _ => false,
    };
    effect
        || obj(t, "attributes")
            .values()
            .chain(obj(t, "inherited_parts").values())
            .flat_map(|p| list(p, "cascade"))
            .any(|c| list(c, "on").iter().any(|o| o.as_str() == Some(xn)))
        || followed.contains(xn)
}

/// A finding of step 5: where, fatal or a notice, its code and its words.
pub type Logic = (Path, bool, &'static str, String);

/// The findings for one type of the view, and how many of the guard
/// conditions it read it decided in full, reading nothing outside the type,
/// of all it read (ADR-0137 decision 3); `followed` holds its transitions a
/// follows clause names, each of which may change another object (ADR-0292).
pub fn analyse_type(
    tn: &str,
    t: &Value,
    enums: &BTreeMap<String, Vec<String>>,
    followed: &BTreeSet<String>,
) -> (Vec<Logic>, usize, usize) {
    let mut out = Vec::new();
    let (mut whole, mut total) = (0, 0);
    let states = obj(t, "states");
    let conditions = obj(t, "conditions");
    let invariants = state_invariants(t);
    let transitions = obj(t, "transitions");
    for s in states.keys() {
        let model = TypeModel::new(tn, t, enums);
        let here = segs!["types", tn, "states", s];
        let decided: Decided<()> = (|| {
            let mut a = Analysis::new(&model, BTreeMap::new(), None, false);
            let given: Case = [
                Atom::Dom(Var::State, BTreeSet::from([Val::S(s.clone())])),
                Atom::Present(Var::State),
            ]
            .into_iter()
            .collect();
            let mut reqs: Vec<Requirement> = vec![(format!("in {s}"), vec![given.clone()])];
            let mut inv_reqs = Vec::new();
            for (n, tree) in &invariants {
                inv_reqs.push((format!("invariant {n}"), holds(&mut a, tree)?));
            }
            reqs.extend(fixed(inv_reqs, &given, &model));
            if reqs.len() > 1 && !any_satisfiable(&reqs, &model)? {
                let mut entering: Vec<String> = transitions
                    .iter()
                    .filter(|(_, x)| {
                        matches!(kind(x), "initial" | "external")
                            && x.get("to").and_then(Value::as_str) == Some(s.as_str())
                    })
                    .map(|(n, _)| n.clone())
                    .collect();
                entering.sort();
                if !entering.is_empty() {
                    let names: Vec<String> = core(&reqs, &model)?
                        .into_iter()
                        .filter(|(n, _)| n.starts_with("invariant"))
                        .map(|(n, _)| n.split_once(' ').map_or(n.clone(), |(_, r)| r.to_string()))
                        .collect();
                    out.push((
                        here.clone(),
                        true,
                        "contradiction",
                        format!(
                            "no object of {tn} can be in {s}: {} cannot {} hold there, and {} enter{} it",
                            names.join(" and "),
                            together(&names),
                            entering.join(", "),
                            if entering.len() == 1 { "s" } else { "" }
                        ),
                    ));
                }
            }
            Ok(())
        })();
        if decided.is_err() {
            out.push((
                here,
                false,
                "undecided",
                format!("the invariants of {tn} in {s} expand to more than {LIMIT} cases, so whether they can hold together is not decided; they are still enforced when a request is taken. Each `and`, `or` and `implies` they join, and each value they read that may be absent, multiplies the cases: an attribute the state requires in place of an optional one, or fewer clauses in one invariant, brings them within it"),
            ));
        }
    }
    for (xn, x) in transitions {
        if matches!(kind(x), "assertion" | "erasure") {
            continue;
        }
        let mut model = TypeModel::new(tn, t, enums);
        model.created = kind(x) == "initial";
        let declared = obj(x, "inputs").clone();
        for i in strings(x, "required_inputs")
            .into_iter()
            .chain(strings(x, "optional_inputs"))
        {
            let spec = model.attrs.get(&i).cloned();
            model.inputs.insert(i, spec);
        }
        for (i, sp) in &declared {
            model.inputs.insert(i.clone(), Some(sp.clone()));
        }
        model.present = strings(x, "required_inputs").into_iter().collect();
        for (i, sp) in &declared {
            if !flag(sp, "optional") || sp.get("default").is_some() {
                model.present.insert(i.clone());
            }
        }
        let here = segs!["types", tn, "transitions", xn];
        let froms: Vec<String> = match x.get("from") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::String(f)) => vec![f.clone()],
            Some(Value::Array(a)) => a.iter().map(py_str).collect(),
            Some(other) => vec![py_str(other)],
        };
        let decided: Decided<()> = (|| {
            let mut a = Analysis::new(&model, BTreeMap::new(), None, false);
            let mut reqs: Vec<Requirement> = if kind(x) == "initial" {
                Vec::new()
            } else {
                vec![(
                    format!("from {}", froms.join(", ")),
                    vec![
                        [Atom::Dom(
                            Var::State,
                            froms.iter().map(|f| Val::S(f.clone())).collect(),
                        )]
                        .into_iter()
                        .collect(),
                    ],
                )]
            };
            let (mut denied, mut flagged) = (Vec::new(), Vec::new());
            for (g, mode) in obj(x, "guards") {
                let Some(c) = conditions.get(g) else { continue };
                a.opaque_seen = false;
                let Ok(tree) = parse(&str_or(c, "expression", "")) else {
                    continue;
                };
                let r = (format!("guard {g}"), a.cases(&tree, 'T')?);
                total += 1;
                whole += usize::from(!a.opaque_seen);
                let mode = py_str(mode);
                if mode == "deny" {
                    denied.push(r)
                } else {
                    flagged.push((g.clone(), mode, r))
                }
            }
            reqs.extend(denied);
            if !any_satisfiable(&reqs, &model)? {
                let names: Vec<String> = core(&reqs, &model)?.into_iter().map(|(n, _)| n).collect();
                let message = if names.len() > 1 {
                    format!(
                        "{tn}.{xn} can never be taken: {} cannot {} hold",
                        names.join(" and "),
                        together(&names)
                    )
                } else {
                    format!(
                        "{tn}.{xn} can never be taken: its {} can never hold",
                        names.first().cloned().unwrap_or_default()
                    )
                };
                out.push((here.clone(), true, "contradiction", message));
                return Ok(());
            }
            for (g, mode, r) in &flagged {
                let mut trial = reqs.clone();
                trial.push(r.clone());
                if !any_satisfiable(&trial, &model)? {
                    let what = if mode == "audit" {
                        "records a would-be refusal"
                    } else {
                        "is raised"
                    };
                    out.push((
                        super::doc::ext(&here, segs!["guards", g]),
                        false,
                        "neverpasses",
                        format!("{tn}.{xn}'s {mode} guard {g} can never pass when the transition is taken, so it {what} on every request that applies"),
                    ));
                }
            }
            // what the transition writes, against the invariants that read
            // nothing but it and the state it enters
            let mut written: BTreeMap<String, Written> = strings(x, "required_inputs")
                .into_iter()
                .map(|i| {
                    (
                        i.clone(),
                        Written::Expr(Expr::Path(Box::new(Expr::Name("inputs".into())), i)),
                    )
                })
                .collect();
            let mut assigned: Vec<(String, Expr)> = Vec::new();
            if let Some(Value::Array(steps)) = x.get("effect") {
                for st in steps {
                    if let Some(asg) = st.get("assign") {
                        let loc = str_or(asg, "location", "");
                        if let Ok(tree) = parse(&py_str(asg.get("expr").unwrap_or(&Value::Null))) {
                            assigned.retain(|(l, _)| *l != loc);
                            assigned.push((loc.clone(), tree.clone()));
                            written.insert(loc, Written::Expr(tree));
                        }
                    } else if let Some(Value::Array(cleared)) = st.get("clear") {
                        for c in cleared {
                            written.insert(py_str(c), Written::Null);
                        }
                    } else {
                        // within a loop, or a set's member added or removed:
                        // not followed
                        for loc in writes(std::slice::from_ref(st)) {
                            assigned.retain(|(l, _)| *l != loc);
                            written.insert(loc, Written::Unknown);
                        }
                    }
                }
            }
            let written_names: BTreeSet<String> = written.keys().cloned().collect();
            for (loc, tree) in &assigned {
                // read in the effect's own order, which is not followed
                if !reads(tree, &model.derived).is_disjoint(&written_names) {
                    written.insert(loc.clone(), Written::Unknown);
                }
            }
            let post = matches!(kind(x), "initial" | "external")
                .then(|| x.get("to").map(py_str))
                .flatten();
            let mut after = Analysis::new(
                &model,
                written.clone(),
                post.clone(),
                changes_others(xn, x, t, followed),
            );
            let mut post_reqs = reqs.clone();
            for (n, tree) in &invariants {
                let mut read = names_in(tree);
                for skip in ["inputs", "now", "this"] {
                    read.remove(skip);
                }
                let allowed: BTreeSet<String> = written_names
                    .iter()
                    .cloned()
                    .chain(post.iter().map(|_| "state".to_string()))
                    .chain(model.states.keys().cloned())
                    .collect();
                if !read.is_empty() && read.is_subset(&allowed) {
                    post_reqs.push((format!("invariant {n}"), holds(&mut after, tree)?));
                }
            }
            if post_reqs.len() > reqs.len() && !any_satisfiable(&post_reqs, &model)? {
                let names: Vec<String> = core(&post_reqs, &model)?
                    .into_iter()
                    .map(|(n, _)| n)
                    .collect();
                let broken: Vec<String> = names
                    .iter()
                    .filter(|n| n.starts_with("invariant"))
                    .map(|n| n.split_once(' ').map_or(n.clone(), |(_, r)| r.to_string()))
                    .collect();
                let others: Vec<String> = names
                    .iter()
                    .filter(|n| !n.starts_with("invariant"))
                    .cloned()
                    .collect();
                let leaves = post
                    .as_ref()
                    .map_or(",".to_string(), |p| format!(" leaves {p}, where"));
                let with = if others.is_empty() {
                    String::new()
                } else {
                    format!(", with {}", others.join(" and "))
                };
                out.push((
                    here.clone(),
                    true,
                    "contradiction",
                    format!("{tn}.{xn} can never be taken: what it writes{leaves} {} cannot hold{with}, whatever the request", broken.join(" and ")),
                ));
            }
            Ok(())
        })();
        if decided.is_err() {
            out.push((
                here,
                false,
                "undecided",
                format!("the conditions of {tn}.{xn} expand to more than {LIMIT} cases, so whether they can hold together is not decided; they are still evaluated when the transition is requested. Each `and`, `or` and `implies` they join, and each value they read that may be absent, multiplies the cases: an attribute its states require in place of an optional one, or fewer clauses in one condition, brings them within it"),
            ));
        }
    }
    (out, whole, total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rationals_read_and_order_as_fractions_do() {
        assert_eq!(Rat::parse("12.50"), Some(Rat::new(25, 2)));
        assert_eq!(Rat::parse("-3"), Some(Rat::int(-3)));
        assert!(Rat::new(7, 2) > Rat::int(3));
        assert_eq!(Rat::new(7, 2).floor(), 3);
        assert_eq!(Rat::new(7, 2).ceil(), 4);
        assert_eq!(Rat::new(-7, 2).floor(), -4);
    }

    /// `flowlogic.py`'s `_type`: a small type of the self-test.
    fn small(
        conditions: &[(&str, &str)],
        transitions: Value,
        invariants: &[(&str, &str)],
        derived: &[(&str, &str)],
    ) -> Value {
        let attrs = json!({
            "amount": {"type": "decimal(10,2)"}, "n": {"type": "int"}, "note": {"type": "string", "optional": true},
            "grade": {"type": "Grade"}, "x": {"type": "int", "optional": true}, "eta": {"type": "timestamp"},
            "flag": {"type": "bool", "optional": true}, "a": {"type": "bool"}, "b": {"type": "bool"},
            "items": {"reference": "Item[]"}, "reason": {"type": "string", "optional": true},
            "item": {"reference": "Item"}, "approvers": {"type": "identity[]"},
            "k1": {"type": "int", "optional": true}, "k2": {"type": "int", "optional": true},
            "k3": {"type": "int", "optional": true}, "k4": {"type": "int", "optional": true},
            "k5": {"type": "int", "optional": true}, "k6": {"type": "int", "optional": true}
        });
        let states = json!({"OPEN": {"category": "live"}, "CLOSED": {"category": "closed"}, "DONE": {"category": "closed", "final": true}});
        let cond: serde_json::Map<String, Value> = conditions
            .iter()
            .map(|(n, e)| {
                (
                    n.to_string(),
                    json!({"expression": e, "remedy": "dependent"}),
                )
            })
            .collect();
        let inv: serde_json::Map<String, Value> = invariants
            .iter()
            .map(|(n, e)| (n.to_string(), json!({"expression": e})))
            .collect();
        let der: serde_json::Map<String, Value> = derived
            .iter()
            .map(|(n, e)| (n.to_string(), json!({"expression": e})))
            .collect();
        json!({"attributes": attrs, "states": states, "conditions": cond, "transitions": transitions,
               "invariants": inv, "derived_attributes": der})
    }

    /// `flowlogic.py`'s `_taking`: one transition `go`, its guards deny and
    /// its flags warn.
    fn taking(guards: &[&str], flags: &[&str], from: &str, extra: Value) -> Value {
        let mut g: serde_json::Map<String, Value> = guards
            .iter()
            .map(|x| (x.to_string(), json!("deny")))
            .collect();
        g.extend(flags.iter().map(|x| (x.to_string(), json!("warn"))));
        let mut x = json!({"kind": "external", "from": from, "to": "DONE", "guards": g});
        if let (Some(o), Value::Object(e)) = (x.as_object_mut(), extra) {
            o.extend(e);
        }
        json!({"go": x})
    }

    /// One creation `go` into DONE, its guards deny.
    fn creating(guards: &[&str], extra: Value) -> Value {
        let g: serde_json::Map<String, Value> = guards
            .iter()
            .map(|x| (x.to_string(), json!("deny")))
            .collect();
        let mut x = json!({"kind": "initial", "to": "DONE", "guards": g});
        if let (Some(o), Value::Object(e)) = (x.as_object_mut(), extra) {
            o.extend(e);
        }
        json!({"go": x})
    }

    /// One case of the self-test: its name, conditions, transitions,
    /// invariants, derived attributes and the code it expects, if any.
    type SelfTest = (
        &'static str,
        Vec<(&'static str, &'static str)>,
        Value,
        Vec<(&'static str, &'static str)>,
        Vec<(&'static str, &'static str)>,
        Option<&'static str>,
    );

    /// The self-test of `flowlogic.py`, case for case: each shape decided
    /// on a small type, and each case where refusing would be wrong left
    /// alone.
    #[test]
    fn the_reference_self_test_decides_the_same() {
        let enums = BTreeMap::from([(
            "Grade".to_string(),
            vec!["A".into(), "B".into(), "C".into()],
        )]);
        let none = json!({});
        let user = json!({"inputs": {"e": {"reference": "User"}}});
        let mark = json!([{"call": {"target": "inputs.item", "transition": "mark", "inputs": {}}}]);
        // issue #98's six, five cases each; six in one group, past the bound
        // alone; six each bounding a number through zero alone
        let apart: Vec<(&str, &str)> = vec![
            ("rule_1", "is_whole(q1) or item.f1"),
            ("rule_2", "is_whole(q2) or item.f2"),
            ("rule_3", "is_whole(q3) or item.f3"),
            ("rule_4", "is_whole(q4) or item.f4"),
            ("rule_5", "is_whole(q5) or item.f5"),
            ("rule_6", "is_whole(q6) or item.f6"),
        ];
        let entangled: Vec<(&str, &str)> = vec![
            ("rule_1", "is_whole(q1) or item.f0"),
            ("rule_2", "is_whole(q2) or item.f0"),
            ("rule_3", "is_whole(q3) or item.f0"),
            ("rule_4", "is_whole(q4) or item.f0"),
            ("rule_5", "is_whole(q5) or item.f0"),
            ("rule_6", "is_whole(q6) or item.f0"),
        ];
        let bounded: Vec<(&str, &str)> = vec![
            ("rule_1", "k1 > 0 or item.f1"),
            ("rule_2", "k2 > 0 or item.f2"),
            ("rule_3", "k3 > 0 or item.f3"),
            ("rule_4", "k4 > 0 or item.f4"),
            ("rule_5", "k5 > 0 or item.f5"),
            ("rule_6", "k6 > 0 or item.f6"),
        ];
        let contra: Vec<(&str, &str)> = vec![
            ("pos", "state != DONE or n > 0"),
            ("neg", "state != DONE or n < 0"),
        ];
        let once = json!([{"foreach": {"item": "k", "range": 1, "limit": 1, "steps": [{"assign": {"location": "n", "expr": "1"}}]}}]);
        let cases: Vec<SelfTest> = vec![
            (
                "two bounds with no number between",
                vec![("c1", "amount > 100"), ("c2", "amount < 50")],
                taking(&["c1", "c2"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                Some("contradiction"),
            ),
            (
                "two bounds with numbers between",
                vec![("c1", "amount > 100"), ("c2", "amount < 150")],
                taking(&["c1", "c2"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                None,
            ),
            (
                "an int strictly between 3 and 4",
                vec![("c1", "n > 3"), ("c2", "n < 4")],
                taking(&["c1", "c2"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                Some("contradiction"),
            ),
            (
                "a decimal strictly between 3 and 4",
                vec![("c1", "amount > 3"), ("c2", "amount < 4")],
                taking(&["c1", "c2"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                None,
            ),
            (
                "a guard on a state the transition does not leave",
                vec![("c1", "state == DONE")],
                taking(&["c1"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                Some("contradiction"),
            ),
            (
                "an enumeration value and its negation",
                vec![("c1", "grade == Grade.A"), ("c2", "grade != Grade.A")],
                taking(&["c1", "c2"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                Some("contradiction"),
            ),
            (
                "an enumeration value among others",
                vec![
                    ("c1", "grade == Grade.A or grade == Grade.B"),
                    ("c2", "grade != Grade.A"),
                ],
                taking(&["c1", "c2"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                None,
            ),
            (
                "a value both absent and equal to something",
                vec![("c1", "note is null"), ("c2", "note == \"x\"")],
                taking(&["c1", "c2"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                Some("contradiction"),
            ),
            (
                "an optional value compared, and tested absent",
                vec![("c1", "x > 3"), ("c2", "x is null")],
                taking(&["c1", "c2"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                Some("contradiction"),
            ),
            (
                "an opaque condition and its negation",
                vec![
                    ("c1", "inputs.e.state == User.ACTIVE"),
                    ("c2", "not (inputs.e.state == User.ACTIVE)"),
                ],
                taking(&["c1", "c2"], &[], "OPEN", user.clone()),
                vec![],
                vec![],
                Some("contradiction"),
            ),
            (
                "an opaque condition alone",
                vec![("c1", "inputs.e.state == User.ACTIVE")],
                taking(&["c1"], &[], "OPEN", user.clone()),
                vec![],
                vec![],
                None,
            ),
            (
                "a date both past and two days ahead",
                vec![("c1", "eta < now"), ("c2", "eta > now + 2 days")],
                taking(&["c1", "c2"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                Some("contradiction"),
            ),
            (
                "none of a collection, and at least one of it",
                vec![
                    ("c1", "none(i in items where i.bad)"),
                    ("c2", "count(i in items where i.bad) >= 1"),
                ],
                taking(&["c1", "c2"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                Some("contradiction"),
            ),
            (
                "a derived attribute replaced by its expression",
                vec![("c1", "big"), ("c2", "amount < 50")],
                taking(&["c1", "c2"], &[], "OPEN", none.clone()),
                vec![],
                vec![("big", "amount > 100")],
                Some("contradiction"),
            ),
            (
                "open, on a transition from a closed state",
                vec![("c1", "open")],
                taking(&["c1"], &[], "CLOSED", none.clone()),
                vec![],
                vec![],
                Some("contradiction"),
            ),
            (
                "an implication, its premise and not its conclusion",
                vec![("c1", "a implies b"), ("c2", "a"), ("c3", "not b")],
                taking(&["c1", "c2", "c3"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                Some("contradiction"),
            ),
            (
                "not of an absent value is unknown, never true",
                vec![("c1", "not flag"), ("c2", "flag is null")],
                taking(&["c1", "c2"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                Some("contradiction"),
            ),
            (
                "a state whose invariants cannot hold together",
                vec![],
                taking(&[], &[], "OPEN", none.clone()),
                vec![
                    ("pos", "state != DONE or n > 0"),
                    ("neg", "state != DONE or n < 0"),
                ],
                vec![],
                Some("contradiction"),
            ),
            (
                "an invariant left unknown by an absent value holds",
                vec![],
                taking(&[], &[], "OPEN", none.clone()),
                vec![
                    ("pos", "state != DONE or x > 0"),
                    ("neg", "state != DONE or x < 0"),
                ],
                vec![],
                None,
            ),
            (
                "a write that breaks the state entered",
                vec![],
                taking(&[], &[], "OPEN", json!({"effect": [{"clear": ["reason"]}]})),
                vec![("reasoned", "state != DONE or reason is not null")],
                vec![],
                Some("contradiction"),
            ),
            (
                "a write the state entered does not read",
                vec![],
                taking(&[], &[], "OPEN", json!({"effect": [{"clear": ["note"]}]})),
                vec![("reasoned", "state != DONE or reason is not null")],
                vec![],
                None,
            ),
            (
                "a flag that fails whenever the blocking guards pass",
                vec![("c1", "n > 5"), ("c2", "n < 3")],
                taking(&["c1"], &["c2"], "OPEN", none.clone()),
                vec![],
                vec![],
                Some("neverpasses"),
            ),
            // a part read after the write is the part of the value written
            // (ADR-0279)
            (
                "an opaque part read before the write and after it",
                vec![("c1", "not is_whole(amount)")],
                taking(&["c1"], &[], "OPEN", json!({"required_inputs": ["amount"]})),
                vec![("whole", "is_whole(amount)")],
                vec![],
                None,
            ),
            (
                "an opaque part of the input, read after the write",
                vec![("c1", "not is_whole(inputs.amount)")],
                taking(&["c1"], &[], "OPEN", json!({"required_inputs": ["amount"]})),
                vec![("whole", "is_whole(amount)")],
                vec![],
                Some("contradiction"),
            ),
            (
                "a count read before the write and after it",
                vec![("c1", "none(a in approvers)")],
                taking(
                    &["c1"],
                    &[],
                    "OPEN",
                    json!({"required_inputs": ["approvers"]}),
                ),
                vec![("some", "any(a in approvers)")],
                vec![],
                None,
            ),
            (
                "a path through a reference read before the write and after it",
                vec![("c1", "not item.f1")],
                taking(&["c1"], &[], "OPEN", json!({"required_inputs": ["item"]})),
                vec![("marked", "item.f1")],
                vec![],
                None,
            ),
            (
                "a path through the input, read after the write",
                vec![("c1", "not inputs.item.f1")],
                taking(&["c1"], &[], "OPEN", json!({"required_inputs": ["item"]})),
                vec![("marked", "item.f1")],
                vec![],
                Some("contradiction"),
            ),
            (
                "a path through an object a call may change, read after it",
                vec![("c1", "not inputs.item.f1")],
                taking(
                    &["c1"],
                    &[],
                    "OPEN",
                    json!({"required_inputs": ["item"], "effect": mark}),
                ),
                vec![("marked", "item.f1")],
                vec![],
                None,
            ),
            (
                "a name read through this, not written",
                vec![("c1", "n <= 5")],
                taking(&["c1"], &[], "OPEN", json!({"required_inputs": ["amount"]})),
                vec![("both", "amount > 0 and this.n > 5")],
                vec![],
                None,
            ),
            (
                "a name read through this, written",
                vec![("c1", "inputs.amount <= 0")],
                taking(&["c1"], &[], "OPEN", json!({"required_inputs": ["amount"]})),
                vec![("positive", "this.amount > 0")],
                vec![],
                Some("contradiction"),
            ),
            (
                "a derived attribute read through this",
                vec![("c1", "this.big"), ("c2", "amount < 50")],
                taking(&["c1", "c2"], &[], "OPEN", none.clone()),
                vec![],
                vec![("big", "amount > 100")],
                Some("contradiction"),
            ),
            (
                "a write within a loop, after the input",
                vec![("c1", "inputs.n <= 0")],
                taking(
                    &["c1"],
                    &[],
                    "OPEN",
                    json!({"required_inputs": ["n"], "effect": once}),
                ),
                vec![("positive", "n > 0")],
                vec![],
                None,
            ),
            (
                "the input written, and no loop after it",
                vec![("c1", "inputs.n <= 0")],
                taking(&["c1"], &[], "OPEN", json!({"required_inputs": ["n"]})),
                vec![("positive", "n > 0")],
                vec![],
                Some("contradiction"),
            ),
            (
                "a derived attribute within an opaque part, read as its expression",
                vec![("c1", "not (wq == b)"), ("c2", "is_whole(amount) == b")],
                taking(&["c1", "c2"], &[], "OPEN", none.clone()),
                vec![],
                vec![("wq", "is_whole(amount)")],
                Some("contradiction"),
            ),
            (
                "the state within an opaque part, read before and after it changes",
                vec![("c1", "not (inputs.item.home == state)")],
                taking(&["c1"], &[], "OPEN", json!({"required_inputs": ["item"]})),
                vec![("home", "item.home == state")],
                vec![],
                None,
            ),
            (
                "an opaque part of a write the analysis cannot follow",
                vec![("c1", "not is_whole(amount)")],
                taking(
                    &["c1"],
                    &[],
                    "OPEN",
                    json!({"inputs": {"d": {"type": "decimal(10,2)"}},
                    "effect": [{"assign": {"location": "amount", "expr": "amount + inputs.d"}}]}),
                ),
                vec![("whole", "is_whole(amount)")],
                vec![],
                None,
            ),
            (
                "a change test naming what the transition writes",
                vec![("c1", "not changed_since([amount], inputs.ev)")],
                taking(&["c1"], &[], "OPEN", json!({"required_inputs": ["amount"]})),
                vec![("changed", "changed_since([amount], inputs.ev)")],
                vec![],
                None,
            ),
            (
                "when the state entered was entered, read before and after",
                vec![("c1", "not (inputs.item.at == entered_at(DONE))")],
                taking(&["c1"], &[], "OPEN", json!({"required_inputs": ["item"]})),
                vec![("at", "item.at == entered_at(DONE)")],
                vec![],
                None,
            ),
            (
                "a set given, then a member removed from it",
                vec![("c1", "not all(i in inputs.items: i.ok)")],
                taking(
                    &["c1"],
                    &[],
                    "OPEN",
                    json!({"required_inputs": ["items"], "inputs": {"bad": {"reference": "Item"}},
                    "effect": [{"remove": {"location": "items", "expr": "inputs.bad"}}]}),
                ),
                vec![("ok", "all(i in items: i.ok)")],
                vec![],
                None,
            ),
            (
                "a derived attribute written before what it reads is",
                vec![("c1", "not is_whole(inputs.v)")],
                taking(
                    &["c1"],
                    &[],
                    "OPEN",
                    json!({"inputs": {"v": {"type": "decimal(10,2)"}},
                    "effect": [{"assign": {"location": "a", "expr": "wq"}}, {"assign": {"location": "amount", "expr": "inputs.v"}}]}),
                ),
                vec![("held", "a")],
                vec![("wq", "is_whole(amount)")],
                None,
            ),
            (
                "a creation's guard reads the value it supplies, as the invariant after it",
                vec![("c1", "not is_whole(amount)")],
                creating(&["c1"], json!({"required_inputs": ["amount"]})),
                vec![("whole", "is_whole(amount)")],
                vec![],
                Some("contradiction"),
            ),
            // each group of requirements that share nothing decided alone
            // (ADR-0280)
            (
                "six two-part rules sharing nothing, 5^6 cases together",
                vec![],
                taking(&[], &[], "OPEN", none.clone()),
                apart.clone(),
                vec![],
                None,
            ),
            (
                "six rules each bounding a number from zero, sharing nothing",
                vec![],
                taking(&[], &[], "OPEN", none.clone()),
                bounded.clone(),
                vec![],
                None,
            ),
            (
                "two rules contradicting after one group past the bound",
                vec![],
                taking(&[], &[], "OPEN", none.clone()),
                entangled.iter().chain(&contra).copied().collect(),
                vec![],
                Some("contradiction"),
            ),
            (
                "two rules contradicting, met first, beside one group past the bound",
                vec![],
                taking(&[], &[], "OPEN", none.clone()),
                contra.iter().chain(&entangled).copied().collect(),
                vec![],
                Some("contradiction"),
            ),
            (
                "an int strictly between 3 and 4 beside a decimal bound",
                vec![("c1", "n > 3"), ("c2", "n < 4"), ("c3", "amount > 0")],
                taking(&["c1", "c2", "c3"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                Some("contradiction"),
            ),
            (
                "an int strictly between 3 and 4 and a decimal bound, in one guard",
                vec![("c1", "n > 3 and n < 4 and amount > 0")],
                taking(&["c1"], &[], "OPEN", none.clone()),
                vec![],
                vec![],
                Some("contradiction"),
            ),
        ];
        let mut wrong = Vec::new();
        for (name, conds, transitions, invariants, derived, want) in &cases {
            let t = small(conds, transitions.clone(), invariants, derived);
            let codes: BTreeSet<&str> = analyse_type("T", &t, &enums, &BTreeSet::new())
                .0
                .iter()
                .map(|f| f.2)
                .collect();
            let right = match want {
                Some(w) => codes.contains(w),
                None => codes.is_empty(),
            };
            if !right {
                wrong.push(format!("{name}: expected {want:?}, found {codes:?}"));
            }
        }
        // a contradiction found after a group past the bound names the rules
        // that cannot hold, sought within their group
        let t = small(
            &[],
            taking(&[], &[], "OPEN", none.clone()),
            &entangled.iter().chain(&contra).copied().collect::<Vec<_>>(),
            &[],
        );
        let named =
            "no object of T can be in DONE: pos and neg cannot both hold there, and go enters it";
        let messages: Vec<String> = analyse_type("T", &t, &enums, &BTreeSet::new())
            .0
            .into_iter()
            .map(|f| f.3)
            .collect();
        if !messages.iter().any(|m| m == named) {
            wrong.push(format!("a contradiction after a group past the bound, named: expected {named:?}, found {messages:?}"));
        }
        // a part's cascade on the transition may change another object, so a
        // path through one read after it is its own part
        let mut parted = small(
            &[("c1", "not inputs.item.f1")],
            taking(&["c1"], &[], "OPEN", json!({"required_inputs": ["item"]})),
            &[("marked", "item.f1")],
            &[],
        );
        parted["attributes"]["parts"] = json!({"reference": "Part[]", "cascade": [{"on": ["go"], "transition": "close", "limit": 10}]});
        let found = analyse_type("T", &parted, &enums, &BTreeSet::new()).0;
        if !found.is_empty() {
            wrong.push(format!("a path through another object read after a cascade: expected nothing, found {found:?}"));
        }
        // so does a follow of the transition (ADR-0292): read after it, the
        // same path is its own part, and without a follow or a cascade it is
        // the transition's to decide
        let plain = small(
            &[("c1", "not inputs.item.f1")],
            taking(&["c1"], &[], "OPEN", json!({"required_inputs": ["item"]})),
            &[("marked", "item.f1")],
            &[],
        );
        for (followed, want) in [
            (BTreeSet::from(["go".to_string()]), false),
            (BTreeSet::new(), true),
        ] {
            let found = analyse_type("T", &plain, &enums, &followed).0;
            if found.is_empty() == want {
                wrong.push(format!("a path through another object read with {followed:?} followed: expected a finding {want}, found {found:?}"));
            }
        }
        // a lifecycle of thirteen states, each requiring what the one before
        // set, decided in every state rather than expanded past the limit
        let mut chain = serde_json::Map::new();
        for i in 1..=13 {
            let req: Vec<String> = (1..=i).map(|j| format!("t{j}")).collect();
            chain.insert(
                format!("S{i}"),
                json!({"category": "live", "required_attributes": req, "final": i == 13}),
            );
        }
        let mut moves = serde_json::Map::new();
        moves.insert(
            "start".into(),
            json!({"kind": "initial", "to": "S1", "required_inputs": ["t1"]}),
        );
        for i in 2..=13 {
            moves.insert(format!("to{i}"), json!({"kind": "external", "from": format!("S{}", i - 1), "to": format!("S{i}"), "required_inputs": [format!("t{i}")]}));
        }
        let stamps: serde_json::Map<String, Value> = (1..=13)
            .map(|i| {
                (
                    format!("t{i}"),
                    json!({"type": "timestamp", "optional": true}),
                )
            })
            .collect();
        for (name, invariants, want) in [
            (
                "a long lifecycle, its last state contradicted",
                json!({"never": {"expression": "state != S13 or t13 is null"}}),
                Some("contradiction"),
            ),
            ("a long lifecycle that holds together", json!({}), None),
        ] {
            let t = json!({"attributes": stamps, "states": chain, "conditions": {}, "transitions": moves, "invariants": invariants, "derived_attributes": {}});
            let codes: BTreeSet<&str> = analyse_type("T", &t, &enums, &BTreeSet::new())
                .0
                .iter()
                .map(|f| f.2)
                .collect();
            let right = match want {
                Some(w) => codes.contains(w) && !codes.contains("undecided"),
                None => codes.is_empty(),
            };
            if !right {
                wrong.push(format!("{name}: expected {want:?}, found {codes:?}"));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn two_bounds_with_no_number_between_contradict() {
        let t = json!({
            "attributes": { "amount": { "type": "decimal(10,2)" } },
            "states": { "OPEN": { "category": "live" }, "DONE": { "category": "closed", "final": true } },
            "conditions": {
                "c1": { "expression": "amount > 100", "remedy": "dependent" },
                "c2": { "expression": "amount < 50", "remedy": "dependent" }
            },
            "transitions": { "go": { "kind": "external", "from": ["OPEN"], "to": "DONE", "guards": { "c1": "deny", "c2": "deny" } } }
        });
        let (found, _, _) = analyse_type("T", &t, &BTreeMap::new(), &BTreeSet::new());
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].2, "contradiction");
        let mut wide = t.clone();
        wide["conditions"]["c2"]["expression"] = json!("amount < 150");
        assert!(
            analyse_type("T", &wide, &BTreeMap::new(), &BTreeSet::new())
                .0
                .is_empty()
        );
    }
}

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

use super::doc::{Obj, Path, flag, obj, py_str, segs, str_or, strings};
use super::steps::kind;
use crate::expr::{BinOp, Expr, Number, parse};

/// The cases a set of conditions may expand to before it is reported
/// rather than decided.
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
    ) -> Analysis<'m, 'a> {
        Analysis {
            m,
            written,
            fixed_state,
            opaque_seen: false,
            inlining: BTreeSet::new(),
        }
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
            var: Some(_),
            collection: Some(c),
            filter,
            ..
        } = e
            && kind == "count"
            && self.own_collection(c)
        {
            return Some((
                Some(Var::Count(format!("{c:?}|{filter:?}"))),
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
        one(vec![Atom::Op(format!("{e:?}"), want)])
    }

    /// The conjunctions of atoms under which e has the value want: T, F or U.
    fn cases(&mut self, e: &Expr, want: char) -> Decided<Vec<Case>> {
        match e {
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
                var: Some(_),
                collection: Some(c),
                filter,
                body: None,
                ..
            } if (kind == "none" || kind == "any") && self.own_collection(c) => {
                let count = Var::Count(format!("{c:?}|{filter:?}"));
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
            Expr::Name(n) if self.m.derived.contains_key(n) && !self.inlining.contains(n) => {
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

/// Difference bounds u - v <= c have a solution: no negative cycle.
fn consistent(bounds: &[(Var, Var, Rat, bool)], model: &TypeModel<'_>) -> bool {
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

fn any_satisfiable(reqs: &[Requirement], model: &TypeModel<'_>) -> Decided<bool> {
    let lists: Vec<Vec<Case>> = reqs.iter().map(|(_, c)| c.clone()).collect();
    Ok(product(&lists)?.iter().any(|c| satisfiable(c, model)))
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

/// A smallest set of the requirements, by deletion, that cannot hold.
fn core(reqs: &[Requirement], model: &TypeModel<'_>) -> Decided<Vec<Requirement>> {
    let mut kept: Vec<Requirement> = reqs.to_vec();
    for r in reqs {
        let Some(at) = kept.iter().position(|k| k.0 == r.0 && k.1 == r.1) else {
            continue;
        };
        let mut trial = kept.clone();
        trial.remove(at);
        if !trial.is_empty() && !any_satisfiable(&trial, model)? {
            kept = trial;
        }
    }
    Ok(kept)
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

/// A finding of step 5: where, fatal or a notice, its code and its words.
pub type Logic = (Path, bool, &'static str, String);

/// The findings for one type of the view, and how many of the guard
/// conditions it read it decided in full, reading nothing outside the type,
/// of all it read (ADR-0137 decision 3).
#[allow(clippy::too_many_lines)]
pub fn analyse_type(
    tn: &str,
    t: &Value,
    enums: &BTreeMap<String, Vec<String>>,
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
            let mut a = Analysis::new(&model, BTreeMap::new(), None);
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
                format!("the invariants of {tn} in {s} expand to more than {LIMIT} cases, so whether they can hold together is not decided"),
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
            let mut a = Analysis::new(&model, BTreeMap::new(), None);
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
                    }
                }
            }
            let written_names: BTreeSet<String> = written.keys().cloned().collect();
            for (loc, tree) in &assigned {
                if !names_in(tree).is_disjoint(&written_names) {
                    written.insert(loc.clone(), Written::Unknown);
                }
            }
            let post = matches!(kind(x), "initial" | "external")
                .then(|| x.get("to").map(py_str))
                .flatten();
            let mut after = Analysis::new(&model, written.clone(), post.clone());
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
                format!("the conditions of {tn}.{xn} expand to more than {LIMIT} cases, so whether they can hold together is not decided"),
            ));
        }
    }
    (out, whole, total)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
            "items": {"reference": "Item[]"}, "reason": {"type": "string", "optional": true}
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
        ];
        let mut wrong = Vec::new();
        for (name, conds, transitions, invariants, derived, want) in &cases {
            let t = small(conds, transitions.clone(), invariants, derived);
            let codes: BTreeSet<&str> = analyse_type("T", &t, &enums)
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
            let codes: BTreeSet<&str> = analyse_type("T", &t, &enums)
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
        let (found, _, _) = analyse_type("T", &t, &BTreeMap::new());
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].2, "contradiction");
        let mut wide = t.clone();
        wide["conditions"]["c2"]["expression"] = json!("amount < 150");
        assert!(analyse_type("T", &wide, &BTreeMap::new()).0.is_empty());
    }
}

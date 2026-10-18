//! The expression language, `declaration-syntax.md` §8 and §9, parsed into a
//! tree.
//!
//! The parser follows `scripts/flowexpr.py` rule for rule, so the two read
//! every expression alike; a test holds this one to that one over every
//! expression in the design's modules (ADR-0132 decision 2). Precedence,
//! lowest first: `if`; `implies` (right-associative); `or`; `and`; one
//! comparison, `in` or `is null`, which does not chain; `+ -`; `* /`; unary
//! `not` and `-`; paths and calls.

use std::fmt;

/// A number as written: an integer, or a decimal kept as its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Number {
    Int(i64),
    Decimal(String),
}

/// A binary operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Implies,
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
}

impl BinOp {
    pub fn text(self) -> &'static str {
        match self {
            BinOp::Implies => "implies",
            BinOp::Or => "or",
            BinOp::And => "and",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
        }
    }

    fn comparison(text: &str) -> Option<BinOp> {
        Some(match text {
            "==" => BinOp::Eq,
            "!=" => BinOp::Ne,
            "<" => BinOp::Lt,
            "<=" => BinOp::Le,
            ">" => BinOp::Gt,
            ">=" => BinOp::Ge,
            _ => return None,
        })
    }
}

/// An expression's tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    Num(Number),
    Str(String),
    Bool(bool),
    /// A duration literal: its count, as written, and its unit.
    Dur(Number, String),
    /// A money literal: its currency and its amount, as written.
    Money(String, String),
    /// A bare name, resolved where it is used (§9.2).
    Name(String),
    Path(Box<Expr>, String),
    Call(Box<Expr>, Vec<Expr>),
    /// `count(x in c where f: b)`, or a metric's own `count(where f)`.
    Agg {
        kind: String,
        distinct: bool,
        var: Option<String>,
        collection: Option<Box<Expr>>,
        filter: Option<Box<Expr>>,
        body: Option<Box<Expr>>,
    },
    /// `metric(n, d := e, over last 30 days, fresh 1 h)`.
    Metric {
        name: Box<Expr>,
        binds: Vec<(String, Expr)>,
        over: Option<Box<Expr>>,
        fresh: Option<Box<Expr>>,
    },
    /// `changed_since([a, b], e)`.
    Changed(Vec<String>, Box<Expr>),
    Set(Vec<Expr>),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    /// `e is null`, or `e is not null` when negated.
    IsNull(Box<Expr>, bool),
    In(Box<Expr>, Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `:name`, a read's filter's parameter, which only [`parse_filter`]
    /// reads, bound to its value before the filter is used (ADR-0218).
    Param(String),
    /// A value written into the tree, a parameter bound: what no literal
    /// writes, a date or an instant among them (ADR-0218).
    Lit(Box<crate::value::Value>),
}

impl Expr {
    /// The expressions directly within this one.
    pub fn children(&self) -> Vec<&Expr> {
        match self {
            Expr::Num(_)
            | Expr::Str(_)
            | Expr::Bool(_)
            | Expr::Dur(..)
            | Expr::Money(..)
            | Expr::Name(_)
            | Expr::Param(_)
            | Expr::Lit(_) => Vec::new(),
            Expr::Path(b, _) => vec![b.as_ref()],
            Expr::Call(f, args) => std::iter::once(f.as_ref()).chain(args).collect(),
            Expr::Agg {
                collection,
                filter,
                body,
                ..
            } => [collection, filter, body]
                .into_iter()
                .flatten()
                .map(AsRef::as_ref)
                .collect(),
            Expr::Metric {
                name,
                binds,
                over,
                fresh,
            } => std::iter::once(name.as_ref())
                .chain(binds.iter().map(|(_, e)| e))
                .chain([over, fresh].into_iter().flatten().map(AsRef::as_ref))
                .collect(),
            Expr::Changed(_, e) | Expr::Not(e) | Expr::Neg(e) | Expr::IsNull(e, _) => {
                vec![e.as_ref()]
            }
            Expr::Set(items) => items.iter().collect(),
            Expr::In(l, r) | Expr::Bin(_, l, r) => vec![l.as_ref(), r.as_ref()],
            Expr::If(c, a, b) => vec![c.as_ref(), a.as_ref(), b.as_ref()],
        }
    }

    /// Each input it reads, as `inputs.<name>`.
    pub fn inputs_read(&self, out: &mut std::collections::BTreeSet<String>) {
        if let Expr::Path(b, m) = self
            && matches!(b.as_ref(), Expr::Name(n) if n == "inputs")
        {
            out.insert(m.clone());
        }
        for c in self.children() {
            c.inputs_read(out);
        }
    }
}

/// Why an expression does not parse, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub text: String,
    /// The character offset where parsing stopped.
    pub pos: usize,
    pub message: String,
}

/// A string quoted as Python's `repr` quotes it, which the reference
/// parser's messages use: single quotes, unless the text holds one and no
/// double quote.
pub fn py_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}, at {} of {}",
            self.message,
            self.pos + 1,
            py_repr(&self.text)
        )
    }
}

impl std::error::Error for ParseError {}

const UNITS: [&str; 7] = ["s", "min", "h", "day", "days", "week", "weeks"];
const AGGREGATES: [&str; 10] = [
    "count",
    "sum",
    "min",
    "max",
    "all",
    "any",
    "none",
    "avg",
    "median",
    "percentile",
];

/// The aggregates whose value is their body, which therefore has one (§8,
/// §6.9; the model's check 47).
const BODIED: [&str; 5] = ["sum", "min", "max", "avg", "median"];
const KEYWORDS: [&str; 12] = [
    "and", "or", "not", "implies", "in", "is", "null", "if", "then", "else", "true", "false",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Str,
    Num,
    Name,
    Op,
    End,
}

#[derive(Clone, Debug)]
struct Token {
    kind: Kind,
    text: String,
    pos: usize,
}

fn tokenize(text: &str) -> Result<Vec<Token>, ParseError> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let fail = |pos: usize, message: String| ParseError {
        text: text.to_string(),
        pos,
        message,
    };
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            if i >= chars.len() {
                return Err(fail(
                    start,
                    format!("unexpected character {}", py_repr(&c.to_string())),
                ));
            }
            i += 1;
            out.push(Token {
                kind: Kind::Str,
                text: chars[start..i].iter().collect(),
                pos: start,
            });
            continue;
        }
        if c.is_ascii_digit() {
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            out.push(Token {
                kind: Kind::Num,
                text: chars[start..i].iter().collect(),
                pos: start,
            });
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(Token {
                kind: Kind::Name,
                text: chars[start..i].iter().collect(),
                pos: start,
            });
            continue;
        }
        let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
        if ["==", "!=", "<=", ">=", ":="].contains(&two.as_str()) {
            i += 2;
            out.push(Token {
                kind: Kind::Op,
                text: two,
                pos: start,
            });
            continue;
        }
        if "<>+-*/(){}[],.:…?".contains(c) {
            i += 1;
            out.push(Token {
                kind: Kind::Op,
                text: c.to_string(),
                pos: start,
            });
            continue;
        }
        return Err(fail(
            start,
            format!("unexpected character {}", py_repr(&c.to_string())),
        ));
    }
    out.push(Token {
        kind: Kind::End,
        text: String::new(),
        pos: chars.len(),
    });
    Ok(out)
}

struct Parser {
    text: String,
    toks: Vec<Token>,
    i: usize,
    /// How deeply the parse has nested, bounded by `MAX_NESTING`.
    depth: usize,
    /// Whether `:name` is a parameter, a read's filter's alone (ADR-0218).
    params: bool,
}

/// The most an expression's parse may nest, parentheses, `not`, `-`,
/// `implies` and `if` each a level, so no request's filter or module's
/// expression exhausts the stack that parses it (F1).
pub const MAX_NESTING: usize = 64;

/// The deepest an expression's tree may be, every node a level, which its
/// evaluation and its drop recurse through: a chain of 300 `or`s is 300
/// levels, where a list of alternatives is one, `x in {a, b, …}` (F1).
pub const MAX_DEPTH: usize = 128;

/// The depth of a tree, counted without recursion, since the tree is what
/// recursion would exhaust.
fn depth(e: &Expr) -> usize {
    let mut deepest = 0;
    let mut stack: Vec<(&Expr, usize)> = vec![(e, 1)];
    while let Some((e, d)) = stack.pop() {
        deepest = deepest.max(d);
        let mut children: Vec<&Expr> = Vec::new();
        match e {
            Expr::Num(_)
            | Expr::Str(_)
            | Expr::Bool(_)
            | Expr::Dur(..)
            | Expr::Money(..)
            | Expr::Name(_)
            | Expr::Param(_)
            | Expr::Lit(_) => {}
            Expr::Path(b, _)
            | Expr::Not(b)
            | Expr::Neg(b)
            | Expr::IsNull(b, _)
            | Expr::Changed(_, b) => children.push(b),
            Expr::Call(f, args) => {
                children.push(f);
                children.extend(args.iter());
            }
            Expr::Agg {
                collection,
                filter,
                body,
                ..
            } => children.extend(
                [collection.as_deref(), filter.as_deref(), body.as_deref()]
                    .into_iter()
                    .flatten(),
            ),
            Expr::Metric {
                name,
                binds,
                over,
                fresh,
            } => {
                children.push(name);
                children.extend(binds.iter().map(|(_, b)| b));
                children.extend([over.as_deref(), fresh.as_deref()].into_iter().flatten());
            }
            Expr::Set(items) => children.extend(items.iter()),
            Expr::In(a, b) | Expr::Bin(_, a, b) => {
                children.push(a);
                children.push(b);
            }
            Expr::If(c, a, b) => children.extend([&**c, &**a, &**b]),
        }
        stack.extend(children.into_iter().map(|c| (c, d + 1)));
    }
    deepest
}

type Parsed<T> = Result<T, ParseError>;

fn boxed(e: Expr) -> Box<Expr> {
    Box::new(e)
}

impl Parser {
    fn peek(&self, k: usize) -> &Token {
        let last = self.toks.len() - 1;
        &self.toks[(self.i + k).min(last)]
    }

    fn at(&self, value: &str, k: usize) -> bool {
        let t = self.peek(k);
        t.text == value && matches!(t.kind, Kind::Op | Kind::Name)
    }

    fn error(&self, message: String) -> ParseError {
        let t = self.peek(0);
        ParseError {
            text: self.text.clone(),
            pos: t.pos,
            message,
        }
    }

    fn found(&self) -> String {
        let t = self.peek(0);
        if t.text.is_empty() {
            "the end".to_string()
        } else {
            t.text.clone()
        }
    }

    fn fail<T>(&self, what: &str) -> Parsed<T> {
        Err(self.error(format!("{what}, found {}", py_repr(&self.found()))))
    }

    fn take(&mut self) -> Token {
        let t = self.peek(0).clone();
        self.i += 1;
        t
    }

    fn expect(&mut self, value: &str) -> Parsed<Token> {
        if !self.at(value, 0) {
            return Err(self.error(format!(
                "expected {}, found {}",
                py_repr(value),
                py_repr(&self.found())
            )));
        }
        Ok(self.take())
    }

    fn expr(&mut self) -> Parsed<Expr> {
        self.depth += 1;
        let e = self.nested_expr();
        self.depth -= 1;
        e
    }

    /// An expression at one more level of nesting, refused past
    /// `MAX_NESTING` (F1).
    fn nested_expr(&mut self) -> Parsed<Expr> {
        if self.depth > MAX_NESTING {
            return self.fail(&format!(
                "the expression nests deeper than {MAX_NESTING} levels"
            ));
        }
        if self.at("if", 0) {
            self.take();
            let cond = self.expr()?;
            self.expect("then")?;
            let a = self.expr()?;
            self.expect("else")?;
            let b = self.expr()?;
            return Ok(Expr::If(boxed(cond), boxed(a), boxed(b)));
        }
        self.implies()
    }

    fn implies(&mut self) -> Parsed<Expr> {
        let left = self.or()?;
        if self.at("implies", 0) {
            self.take();
            let right = self.implies()?;
            return Ok(Expr::Bin(BinOp::Implies, boxed(left), boxed(right)));
        }
        Ok(left)
    }

    fn or(&mut self) -> Parsed<Expr> {
        let mut left = self.and()?;
        while self.at("or", 0) {
            self.take();
            let right = self.and()?;
            left = Expr::Bin(BinOp::Or, boxed(left), boxed(right));
        }
        Ok(left)
    }

    fn and(&mut self) -> Parsed<Expr> {
        let mut left = self.comparison()?;
        while self.at("and", 0) {
            self.take();
            let right = self.comparison()?;
            left = Expr::Bin(BinOp::And, boxed(left), boxed(right));
        }
        Ok(left)
    }

    fn is_comparison(&self) -> bool {
        let t = self.peek(0);
        (t.kind == Kind::Op && BinOp::comparison(&t.text).is_some())
            || self.at("in", 0)
            || self.at("is", 0)
    }

    fn comparison(&mut self) -> Parsed<Expr> {
        let left = self.additive()?;
        let t = self.peek(0).clone();
        let node = if let (Kind::Op, Some(op)) = (t.kind, BinOp::comparison(&t.text)) {
            self.take();
            Expr::Bin(op, boxed(left), boxed(self.additive()?))
        } else if self.at("in", 0) {
            self.take();
            Expr::In(boxed(left), boxed(self.additive()?))
        } else if self.at("is", 0) {
            self.take();
            let negated = self.at("not", 0);
            if negated {
                self.take();
            }
            self.expect("null")?;
            Expr::IsNull(boxed(left), negated)
        } else {
            return Ok(left);
        };
        if self.is_comparison() {
            return self.fail("a comparison does not chain");
        }
        Ok(node)
    }

    fn additive(&mut self) -> Parsed<Expr> {
        let mut left = self.multiplicative()?;
        loop {
            let t = self.peek(0);
            let op = match (t.kind, t.text.as_str()) {
                (Kind::Op, "+") => BinOp::Add,
                (Kind::Op, "-") => BinOp::Sub,
                _ => return Ok(left),
            };
            self.take();
            let right = self.multiplicative()?;
            left = Expr::Bin(op, boxed(left), boxed(right));
        }
    }

    fn multiplicative(&mut self) -> Parsed<Expr> {
        let mut left = self.unary()?;
        loop {
            let t = self.peek(0);
            let op = match (t.kind, t.text.as_str()) {
                (Kind::Op, "*") => BinOp::Mul,
                (Kind::Op, "/") => BinOp::Div,
                _ => return Ok(left),
            };
            self.take();
            let right = self.unary()?;
            left = Expr::Bin(op, boxed(left), boxed(right));
        }
    }

    fn unary(&mut self) -> Parsed<Expr> {
        if !(self.at("not", 0) || self.at("-", 0)) {
            return self.postfix();
        }
        self.depth += 1;
        let e = if self.depth > MAX_NESTING {
            self.fail(&format!(
                "the expression nests deeper than {MAX_NESTING} levels"
            ))
        } else if self.at("not", 0) {
            self.take();
            self.unary().map(|x| Expr::Not(boxed(x)))
        } else {
            self.take();
            self.unary().map(|x| Expr::Neg(boxed(x)))
        };
        self.depth -= 1;
        e
    }

    fn postfix(&mut self) -> Parsed<Expr> {
        let mut node = self.primary()?;
        loop {
            if self.at(".", 0) {
                self.take();
                if self.peek(0).kind != Kind::Name {
                    return self.fail("a member name follows `.`");
                }
                let member = self.take().text;
                node = Expr::Path(boxed(node), member);
            } else if self.at("(", 0) && matches!(node, Expr::Path(..)) {
                let args = self.arguments()?;
                node = Expr::Call(boxed(node), args);
            } else {
                return Ok(node);
            }
        }
    }

    fn arguments(&mut self) -> Parsed<Vec<Expr>> {
        self.expect("(")?;
        let mut args = Vec::new();
        while !self.at(")", 0) {
            args.push(self.expr()?);
            if !self.at(")", 0) {
                self.expect(",")?;
            }
        }
        self.expect(")")?;
        Ok(args)
    }

    fn number(text: &str) -> Parsed<Number> {
        if text.contains('.') {
            return Ok(Number::Decimal(text.to_string()));
        }
        // a literal past i64 is kept as written, as Python keeps it whole
        Ok(text
            .parse::<i64>()
            .map(Number::Int)
            .unwrap_or_else(|_| Number::Decimal(text.to_string())))
    }

    fn primary(&mut self) -> Parsed<Expr> {
        let t = self.peek(0).clone();
        if self.params && t.kind == Kind::Op && t.text == ":" && self.peek(1).kind == Kind::Name {
            self.take();
            return Ok(Expr::Param(self.take().text));
        }
        match t.kind {
            Kind::Num => {
                self.take();
                let next = self.peek(0);
                if next.kind == Kind::Name && UNITS.contains(&next.text.as_str()) {
                    let unit = self.take().text;
                    return Ok(Expr::Dur(Self::number(&t.text)?, unit));
                }
                Ok(Expr::Num(Self::number(&t.text)?))
            }
            Kind::Str => {
                self.take();
                let inner = &t.text[1..t.text.len() - 1];
                Ok(Expr::Str(inner.to_string()))
            }
            Kind::Name => {
                if t.text == "true" || t.text == "false" {
                    self.take();
                    return Ok(Expr::Bool(t.text == "true"));
                }
                if t.text.len() == 3
                    && t.text.bytes().all(|b| b.is_ascii_uppercase())
                    && self.peek(1).kind == Kind::Num
                {
                    self.take();
                    let amount = self.take().text;
                    return Ok(Expr::Money(t.text, amount));
                }
                if KEYWORDS.contains(&t.text.as_str()) {
                    return self.fail("an operand");
                }
                self.take();
                if self.at("(", 0) {
                    return self.call(&t.text);
                }
                Ok(Expr::Name(t.text))
            }
            Kind::Op if t.text == "(" => {
                self.take();
                let node = self.expr()?;
                self.expect(")")?;
                Ok(node)
            }
            Kind::Op if t.text == "{" => {
                self.take();
                let mut items = Vec::new();
                while !self.at("}", 0) {
                    items.push(self.expr()?);
                    if !self.at("}", 0) {
                        self.expect(",")?;
                    }
                }
                self.expect("}")?;
                Ok(Expr::Set(items))
            }
            _ => self.fail("an operand"),
        }
    }

    fn call(&mut self, function: &str) -> Parsed<Expr> {
        let comprehension =
            (self.at("distinct", 1) && self.peek(2).kind == Kind::Name && self.at("in", 3))
                || (self.peek(1).kind == Kind::Name && self.at("in", 2));
        let aggregate = AGGREGATES.contains(&function);
        if aggregate && !comprehension && (self.at("where", 1) || self.at("distinct", 1)) {
            // a metric's aggregate over its own rows: count(where f),
            // count(distinct e [where f]) (§6.9)
            self.expect("(")?;
            let distinct = self.at("distinct", 0);
            if distinct {
                self.take();
            }
            let body = if self.at("where", 0) {
                None
            } else {
                Some(boxed(self.expr()?))
            };
            let mut filter = None;
            if self.at("where", 0) {
                self.take();
                filter = Some(boxed(self.expr()?));
            }
            self.expect(")")?;
            return Ok(Expr::Agg {
                kind: function.to_string(),
                distinct,
                var: None,
                collection: None,
                filter,
                body,
            });
        }
        if aggregate && comprehension {
            self.expect("(")?;
            let distinct = self.at("distinct", 0);
            if distinct {
                self.take();
            }
            let var = self.take().text;
            self.expect("in")?;
            let collection = self.additive()?;
            let mut filter = None;
            let mut body = None;
            if self.at("where", 0) {
                self.take();
                filter = Some(boxed(self.expr()?));
            }
            if self.at(":", 0) {
                self.take();
                body = Some(boxed(self.expr()?));
            }
            // the aggregates whose value is their body have one (§8, §6.9;
            // the model's check 47)
            if body.is_none() && (BODIED.contains(&function) || distinct) {
                let which = if distinct {
                    "a distinct count"
                } else {
                    function
                };
                return self.fail(&format!("a body after ':', which {which} requires"));
            }
            self.expect(")")?;
            return Ok(Expr::Agg {
                kind: function.to_string(),
                distinct,
                var: Some(var),
                collection: Some(boxed(collection)),
                filter,
                body,
            });
        }
        if function == "metric" {
            self.expect("(")?;
            let name = self.additive()?;
            let mut binds = Vec::new();
            let mut over = None;
            let mut fresh = None;
            while self.at(",", 0) {
                self.take();
                if self.at("over", 0) {
                    self.take();
                    self.expect("last")?;
                    over = Some(boxed(self.primary()?));
                } else if self.at("fresh", 0) {
                    self.take();
                    fresh = Some(boxed(self.primary()?));
                } else {
                    let dimension = self.take().text;
                    self.expect(":=")?;
                    binds.push((dimension, self.expr()?));
                }
            }
            self.expect(")")?;
            return Ok(Expr::Metric {
                name: boxed(name),
                binds,
                over,
                fresh,
            });
        }
        if function == "changed_since" {
            self.expect("(")?;
            self.expect("[")?;
            let mut names = Vec::new();
            while !self.at("]", 0) {
                names.push(self.take().text);
                if !self.at("]", 0) {
                    self.expect(",")?;
                }
            }
            self.expect("]")?;
            self.expect(",")?;
            let event = self.expr()?;
            self.expect(")")?;
            return Ok(Expr::Changed(names, boxed(event)));
        }
        let args = self.arguments()?;
        Ok(Expr::Call(boxed(Expr::Name(function.to_string())), args))
    }
}

impl Number {
    fn json(&self) -> serde_json::Value {
        match self {
            Number::Int(n) => serde_json::Value::from(*n),
            Number::Decimal(t) => serde_json::Value::from(t.as_str()),
        }
    }
}

impl Expr {
    /// The tree as `scripts/flowexpr.py` writes it, each node an array whose
    /// first element names it: the form the parsers are compared in.
    pub fn tree(&self) -> serde_json::Value {
        use serde_json::{Value as J, json};
        let opt = |e: &Option<Box<Expr>>| e.as_ref().map_or(J::Null, |e| e.tree());
        match self {
            Expr::Num(n) => json!(["num", n.json()]),
            Expr::Str(s) => json!(["str", s]),
            Expr::Bool(b) => json!(["bool", b]),
            Expr::Dur(n, u) => json!(["dur", n.json(), u]),
            Expr::Money(c, a) => json!(["money", c, a]),
            Expr::Name(n) => json!(["name", n]),
            Expr::Param(n) => json!(["param", n]),
            Expr::Lit(v) => json!(["lit", crate::wire::encode(v)]),
            Expr::Path(b, m) => json!(["path", b.tree(), m]),
            Expr::Call(f, args) => {
                json!([
                    "call",
                    f.tree(),
                    args.iter().map(Expr::tree).collect::<Vec<_>>()
                ])
            }
            Expr::Agg {
                kind,
                distinct,
                var,
                collection,
                filter,
                body,
            } => json!([
                "agg",
                kind,
                distinct,
                var,
                opt(collection),
                opt(filter),
                opt(body)
            ]),
            Expr::Metric {
                name,
                binds,
                over,
                fresh,
            } => json!([
                "metric",
                name.tree(),
                binds
                    .iter()
                    .map(|(d, e)| json!([d, e.tree()]))
                    .collect::<Vec<_>>(),
                opt(over),
                opt(fresh)
            ]),
            Expr::Changed(names, e) => json!(["changed", names, e.tree()]),
            Expr::Set(items) => json!(["set", items.iter().map(Expr::tree).collect::<Vec<_>>()]),
            Expr::Not(e) => json!(["not", e.tree()]),
            Expr::Neg(e) => json!(["neg", e.tree()]),
            Expr::IsNull(e, n) => json!(["isnull", e.tree(), n]),
            Expr::In(l, r) => json!(["in", l.tree(), r.tree()]),
            Expr::Bin(op, l, r) => json!(["bin", op.text(), l.tree(), r.tree()]),
            Expr::If(c, a, b) => json!(["if", c.tree(), a.tree(), b.tree()]),
        }
    }
}

/// The tree of an expression written as text.
pub fn parse(text: &str) -> Result<Expr, ParseError> {
    parsed(text, false)
}

/// The tree of a read's filter, which may name parameters, `:name`, each
/// bound to a value the request gives beside it (ADR-0218).
pub fn parse_filter(text: &str) -> Result<Expr, ParseError> {
    parsed(text, true)
}

fn parsed(text: &str, params: bool) -> Result<Expr, ParseError> {
    let mut p = Parser {
        text: text.to_string(),
        toks: tokenize(text)?,
        i: 0,
        depth: 0,
        params,
    };
    let node = p.expr()?;
    if p.peek(0).kind != Kind::End {
        return p.fail("the end of the expression");
    }
    if depth(&node) > MAX_DEPTH {
        return Err(p.error(format!(
            "the expression is deeper than {MAX_DEPTH} levels; a long list of alternatives is written x in {{a, b, …}}"
        )));
    }
    Ok(node)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_and_associativity_are_the_grammars() {
        let e = parse("a or b and c").unwrap();
        assert!(matches!(e, Expr::Bin(BinOp::Or, _, _)));
        let e = parse("a implies b implies c").unwrap();
        match e {
            Expr::Bin(BinOp::Implies, l, r) => {
                assert_eq!(*l, Expr::Name("a".into()));
                assert!(matches!(*r, Expr::Bin(BinOp::Implies, _, _)));
            }
            other => panic!("{other:?}"),
        }
        let e = parse("a - b - c").unwrap();
        match e {
            Expr::Bin(BinOp::Sub, l, _) => assert!(matches!(*l, Expr::Bin(BinOp::Sub, _, _))),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_aggregate_whose_value_is_its_body_has_one() {
        // the reference parser's own words (`flowexpr.py`; the model's
        // check 47)
        let refused = |t: &str| parse(t).expect_err(t).to_string();
        assert_eq!(
            refused("sum(x in lines where x.qty > 0)"),
            "a body after ':', which sum requires, found ')', at 31 of 'sum(x in lines where x.qty > 0)'"
        );
        assert_eq!(
            refused("count(distinct x in lines)"),
            "a body after ':', which a distinct count requires, found ')', at 26 of 'count(distinct x in lines)'"
        );
        for bodiless in [
            "min(x in lines)",
            "max(x in lines)",
            "avg(x in lines)",
            "median(x in lines where x.ok)",
        ] {
            assert!(
                refused(bodiless).starts_with("a body after ':'"),
                "{bodiless}"
            );
        }
        for kept in [
            "count(x in lines)",
            "sum(x in lines: x.qty)",
            "all(x in lines where x.ok)",
        ] {
            assert!(parse(kept).is_ok(), "{kept}");
        }
    }

    #[test]
    fn nesting_and_depth_are_bounded() {
        // a long chain is refused past the tree's bound, a list of
        // alternatives is one level, and nesting is refused past its bound,
        // so no expression exhausts the stack that parses or evaluates it
        // (F1)
        let chain = |n: usize| {
            (0..n)
                .map(|i| format!("x == {i}"))
                .collect::<Vec<_>>()
                .join(" or ")
        };
        assert!(parse(&chain(120)).is_ok());
        let long = parse(&chain(200)).expect_err("a chain past the bound");
        assert!(long.message.contains("deeper than 128"), "{}", long.message);
        let set = format!(
            "x in {{{}}}",
            (0..3000)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
        assert!(parse(&set).is_ok());
        let parens = |n: usize| format!("{}x{}", "(".repeat(n), ")".repeat(n));
        assert!(parse(&parens(60)).is_ok());
        assert!(
            parse(&parens(70))
                .expect_err("nesting past the bound")
                .message
                .contains("nests deeper than 64")
        );
        assert!(parse(&format!("{}x", "not ".repeat(70))).is_err());
        // past both bounds by far, refused rather than overflowing
        assert!(parse(&chain(20_000)).is_err());
        assert!(parse(&parens(5_000)).is_err());
    }

    #[test]
    fn a_comparison_does_not_chain() {
        let err = parse("a < b < c").unwrap_err();
        assert!(err.message.starts_with("a comparison does not chain"));
    }

    #[test]
    fn literals_are_told_apart() {
        assert_eq!(
            parse("30 min").unwrap(),
            Expr::Dur(Number::Int(30), "min".into())
        );
        assert_eq!(
            parse("USD 19.99").unwrap(),
            Expr::Money("USD".into(), "19.99".into())
        );
        assert_eq!(
            parse("0.900").unwrap(),
            Expr::Num(Number::Decimal("0.900".into()))
        );
        assert_eq!(parse(r#""HQ""#).unwrap(), Expr::Str("HQ".into()));
    }

    #[test]
    fn a_keyword_is_no_operand() {
        assert!(parse("and").is_err());
        assert!(parse("a b").is_err());
        assert!(parse("\"open").is_err());
    }
}

//! The reads the first slice offers (`DESIGN.md` §10): an object as `get`
//! returns it, and what it offers its actor, `availability`, decided as
//! ADR-0152 states: the actor, the state and each guard that reads no input,
//! in the order of refusals; what reads an input waits for it; no evaluator
//! asked; neither the effect nor the invariants run.

use std::collections::{BTreeMap, BTreeSet};

use crate::eval::{Evaluator, NotRun, truth};
use crate::expr::{BinOp, Expr};
use crate::model::{Declared, Enforce, Kind, Library, Remedy, Transition, Type};
use crate::request::{names_read, resolve_actor, takes_from};
use crate::value::Value;
use crate::verdict::{Flag, Told, Verdict};
use crate::world::{Object, World};

/// An object as `get` returns it: its row, its derived attributes evaluated,
/// and whether it is open.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub object: Object,
    pub derived: Vec<(String, Value)>,
    pub open: bool,
}

/// Where an object's supersession chain ends: its successor's successor
/// and on, the object itself where nothing succeeds it, which `get` and
/// `history` answer with `follow` (`DESIGN.md` §5, ADR-0028). A chain that
/// would close on itself, which `successor_not_superseded` refuses to make,
/// ends where it would repeat.
pub fn followed(world: &World<'_>, id: &str) -> Result<String, NotRun> {
    let mut seen = BTreeSet::new();
    let mut at = id.to_string();
    while seen.insert(at.clone()) {
        match world.get(&at)?.and_then(|o| o.superseded_by.clone()) {
            Some(next) => at = next,
            None => break,
        }
    }
    Ok(at)
}

/// Every member of an object's supersession group, the objects whose chains
/// end where its does, in creation order, which `history` reads with
/// `follow` (`library-api.md` §5, ADR-0147).
pub fn superseded_group(world: &World<'_>, id: &str) -> Result<Vec<String>, NotRun> {
    let end = followed(world, id)?;
    // the types whose states supersede, the only ones holding a successor
    let superseding: Vec<&String> = world
        .lib
        .types
        .iter()
        .filter(|(_, t)| t.states.iter().any(|(_, st)| st.superseding))
        .map(|(n, _)| n)
        .collect();
    let mut members = BTreeSet::from([end.clone()]);
    let mut todo = vec![end];
    while let Some(at) = todo.pop() {
        // each member's predecessors by the index on the successor's column,
        // as the write path reads them, where every superseding type was read
        // whole for each member (the audit's cost B9, ADR-0191 decision 5)
        for tn in &superseding {
            for p in world.superseded_into(tn, &at)? {
                if members.insert(p.id.clone()) {
                    todo.push(p.id.clone());
                }
            }
        }
    }
    let mut placed = Vec::new();
    for m in members {
        if let Some(o) = world.get(&m)? {
            placed.push((o.order, m));
        }
    }
    placed.sort();
    Ok(placed.into_iter().map(|(_, m)| m).collect())
}

/// The object an id names, with what its type derives from it.
pub fn get(world: &World<'_>, id: &str) -> Result<Option<View>, NotRun> {
    let Some(o) = world.get(id)? else {
        return Ok(None);
    };
    let Some(t) = world.lib.type_(&o.type_) else {
        return Err(NotRun::Unsupported(format!("the type {}", o.type_)));
    };
    let mut derived = Vec::new();
    for (n, e) in &t.derived {
        let v = Evaluator::new(world, Some(t), Some(id)).eval(e)?;
        // a derived time past the calendar refuses the read, as an
        // arithmetic past its bound does, where it was answered null; a
        // guard compares it as the number it is (fifth production review,
        // ADR-0191 decision 3, ADR-0205 decision 2)
        let v = match crate::metric::typed_derived(world, t, e, v) {
            Value::Timestamp(s)
                if !(crate::value::FIRST_SECOND..=crate::value::LAST_SECOND).contains(&s) =>
            {
                return Err(NotRun::OutOfRange(id.to_string()));
            }
            v => v,
        };
        derived.push((n.clone(), v));
    }
    // a subscription carries its lag, and whether it is lagging or dead,
    // derived from its acknowledged cursor (DESIGN.md §7)
    if o.type_ == crate::model::SUBSCRIPTION {
        let (lag, lagging, dead) = crate::subscription::lag(world, o)?;
        derived.push(("lag".into(), Value::Duration(lag)));
        derived.push(("lagging".into(), Value::Bool(lagging)));
        derived.push(("dead".into(), Value::Bool(dead)));
    }
    // a proposal carries the verdict its latest refused approval left it
    // with, read from that approval's attempt row and never stored, since
    // the refusal records no event on it (DESIGN.md §9, ADR-0157)
    if o.type_ == crate::model::PROPOSAL {
        // its approvals' attempts by their index, where it read the whole
        // attempt log (the audit's cost B14)
        let proposing = world.attempts_proposing(id)?;
        let refused = proposing.last();
        derived.push((
            "last_verdict".into(),
            refused.map_or(Value::Absent, |a| {
                let text = |t: &Option<String>| t.clone().map_or(Value::Absent, Value::Text);
                Value::row(std::collections::BTreeMap::from([
                    ("verdict".to_string(), Value::Text(a.verdict.clone())),
                    ("clause".to_string(), text(&a.clause)),
                    (
                        "remedy".to_string(),
                        if a.remedy.is_empty() {
                            Value::Absent
                        } else {
                            Value::Text(a.remedy.clone())
                        },
                    ),
                    ("at".to_string(), Value::Timestamp(a.at)),
                ]))
            }),
        ));
    }
    Ok(Some(View {
        object: o.clone(),
        derived,
        open: !t.finished(&o.state),
    }))
}

/// One object of a query's page: its id, its type, and the fields asked
/// for, each as a read gives it.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: String,
    pub type_: String,
    pub fields: Vec<(String, Value)>,
}

/// A query's page (`library-api.md` §6): every object of the type or its
/// family that the filter holds for, in the order asked, or a refusal, a
/// value as every read's is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QueryPage {
    pub items: Vec<Item>,
    pub refused: Option<String>,
    /// What a `malformed_filter` refusal found, a sentence, where the code
    /// alone left its caller guessing (fifth production review, ADR-0191
    /// decision 4).
    pub reason: Option<String>,
    /// Where the next page continues, none where this is the last
    /// (ADR-0179).
    pub cursor: Option<String>,
}

/// The largest page a read answers, and the one it answers unless asked
/// for another (ADR-0179).
pub const PAGE_LIMIT: usize = 1000;
pub const PAGE_DEFAULT: usize = 100;

/// The most a page of `query`, `pull` or `export` holds, as a page of
/// `history` does, one item at least, a cursor going on from its last:
/// 1,000 objects of three 64 KiB strings answered 197 MB, and eight such
/// pages at once 4.78 GB (ninth production review N-9).
pub const PAGE_BYTES: usize = crate::store::EVENT_BYTES;

/// A read, digested, which its cursors carry, so a cursor continues only
/// the read it came from.
fn read_digest(read: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(read.as_bytes());
    h.finalize()
        .iter()
        .take(6)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A page's cursor: the read it continues and the last object it gave,
/// opaque to its caller (`library-api.md` §5, ADR-0179).
fn cursor_of(read: &str, last: &str) -> String {
    format!("{}.{last}", read_digest(read))
}

/// The object a cursor continues after, where it is one of this read's;
/// none for a cursor of another read or naming no object.
fn continued<'w>(
    world: &'w World<'w>,
    read: &str,
    type_: &str,
    cursor: &str,
) -> Result<Option<&'w Object>, NotRun> {
    let Some((digest, id)) = cursor.split_once('.') else {
        return Ok(None);
    };
    if digest != read_digest(read) {
        return Ok(None);
    }
    Ok(world.get(id)?.filter(|o| world.lib.is_a(&o.type_, type_)))
}

/// `exceptions(type)` (`DESIGN.md` §10): the objects of the type or its
/// family whose state an assertion last set, the import's excepted, and
/// those holding an admitted invariant violation, in id order, each with
/// its state; a type the store does not declare is refused, a value.
pub fn exceptions(world: &World<'_>, type_: &str) -> Result<QueryPage, NotRun> {
    exceptions_from(world, type_, None)
}

/// `exceptions` from a cursor: a page of at most [`PAGE_DEFAULT`] of the
/// objects, in creation order, and a cursor going on from its last where
/// more follow, where every one was answered in one page and the cursor
/// taken and ignored (the audit's cost A13); a cursor of another read is
/// refused, as a query's is.
pub fn exceptions_from(
    world: &World<'_>,
    type_: &str,
    after: Option<&str>,
) -> Result<QueryPage, NotRun> {
    let refuse = |why: &str| -> Result<QueryPage, NotRun> {
        Ok(QueryPage {
            items: Vec::new(),
            refused: Some(why.to_string()),
            reason: None,
            cursor: None,
        })
    };
    if world.lib.type_(type_).is_none() {
        return refuse("unknown_type");
    }
    let read = format!("exceptions {type_}");
    let from = match after {
        None => None,
        Some(c) => match continued(world, &read, type_, c)? {
            Some(o) => Some(o.order),
            None => return refuse("malformed_cursor"),
        },
    };
    let mut rest = world
        .excepted(type_)?
        .into_iter()
        .filter(|o| from.is_none_or(|f| o.order > f));
    let page: Vec<&Object> = rest.by_ref().take(PAGE_DEFAULT).collect();
    let cursor = match (page.last(), rest.next()) {
        (Some(last), Some(_)) => Some(cursor_of(&read, &last.id)),
        _ => None,
    };
    Ok(QueryPage {
        items: page.into_iter().map(state_item).collect(),
        refused: None,
        reason: None,
        cursor,
    })
}

/// An object as a page of objects gives it, with its state.
fn state_item(o: &Object) -> Item {
    Item {
        id: o.id.clone(),
        type_: o.type_.clone(),
        fields: vec![(
            "state".into(),
            Value::State {
                type_: o.type_.clone(),
                name: o.state.clone(),
            },
        )],
    }
}

/// `available(type, transition)` (`DESIGN.md` §10): the objects of the
/// type or its family the transition is offered to, as `availability`
/// offers it to each, available or available given its inputs, in id
/// order, each with its state and the guards it did not evaluate, since it
/// asks no evaluator (ADR-0048, ADR-0049). A type or a transition the store
/// does not declare, and a transition only a cascade takes, are refused, a
/// value.
pub fn available(
    world: &World<'_>,
    actor: &str,
    type_: &str,
    transition: &str,
) -> Result<QueryPage, NotRun> {
    available_page(world, actor, type_, transition, None, usize::MAX)
}

/// A page of `available` (ADR-0179): at most `limit` objects in id order,
/// after the one the cursor names, and the cursor of a further page.
pub fn available_page(
    world: &World<'_>,
    actor: &str,
    type_: &str,
    transition: &str,
    after: Option<&str>,
    limit: usize,
) -> Result<QueryPage, NotRun> {
    let read = format!("available\u{1f}{actor}\u{1f}{type_}\u{1f}{transition}");
    let refuse = |why: &str| {
        Ok(QueryPage {
            items: Vec::new(),
            refused: Some(why.to_string()),
            reason: None,
            cursor: None,
        })
    };
    let Some(t) = world.lib.type_(type_) else {
        return refuse("unknown_type");
    };
    let Some(x) = t.transition(transition).filter(|x| x.kind != Kind::Initial) else {
        return refuse("unknown_transition");
    };
    if !x.only_via.is_empty() || t.name == crate::model::OPERATOR_TYPE {
        return refuse("not_requestable");
    }
    let from = match after {
        None => None,
        Some(c) => match continued(world, &read, type_, c)? {
            Some(o) => Some(o.order),
            None => return refuse("malformed_cursor"),
        },
    };
    let mut items = Vec::new();
    let mut more = false;
    // the objects in a state the transition is taken from, by the state's
    // index, where no subtype may declare states of its own; never every
    // object of the type, which made a worklist with nothing to find read
    // a year's claims (READ-COST)
    let family = world
        .lib
        .types
        .keys()
        .any(|n| n != type_ && world.lib.is_a(n, type_));
    let states: Option<BTreeSet<String>> = (!family).then(|| {
        t.states
            .iter()
            .filter(|(s, st)| match x.kind {
                Kind::Erasure => true,
                Kind::Assertion => !st.is_final,
                _ => takes_from(t, x, s),
            })
            .map(|(s, _)| s.clone())
            .collect()
    });
    // read and judged from the cursor a chunk at a time, to the page and one
    // more, where each page read every object in those states, 0.75 s a page
    // on six years whatever its limit, in a person's worklist slot (twelfth
    // production review)
    let chunk = limit.saturating_add(1).max(CHUNK);
    let mut at = from.unwrap_or(i64::MIN);
    'reading: loop {
        let (next, follows) = world.of_type_from(type_, states.as_ref(), &[], at, chunk)?;
        for o in &next {
            if items.len() == limit {
                more = true;
                break 'reading;
            }
            let offers =
                offers_of(world, actor, &o.id, true, Some(transition))?.unwrap_or_default();
            let Some(offer) = offers.iter().find(|f| f.transition == transition) else {
                continue;
            };
            if offer.availability == Availability::Unavailable {
                continue;
            }
            let mut item = state_item(o);
            if !offer.unevaluated.is_empty() {
                item.fields.push((
                    "unevaluated".into(),
                    Value::Set(offer.unevaluated.iter().cloned().map(Value::Text).collect()),
                ));
            }
            items.push(item);
        }
        match next.last() {
            Some(last) if follows => at = last.order,
            _ => break,
        }
    }
    let cursor = more
        .then(|| items.last().map(|i: &Item| cursor_of(&read, &i.id)))
        .flatten();
    Ok(QueryPage {
        items,
        refused: None,
        reason: None,
        cursor,
    })
}

/// One key of a query's order: what it reads, and whether it descends.
enum Key {
    CreatedAt,
    EnteredAt(String),
    Attribute(String),
}

/// A query over a type or a family (`DESIGN.md` §10, ADR-0048, ADR-0127):
/// the objects its filter holds for, a filter unknown for one selecting it
/// not; ordered by each key, ascending unless marked `desc`, an absent value
/// last and the object's id, creation order, breaking every tie; each with
/// the fields asked for, or its type's summary set, or its state where the
/// type declares none. A type it does not know, a filter or a key it cannot
/// read, and a field no object of the type has, are refused, as values.
///
/// The read surface answers a filter by an index (ADR-0048): this one is
/// answered over every object of the type, so a filter no index could
/// answer is not refused, and a time-dependent derived attribute is
/// evaluated rather than rewritten to its operand.
/// Whether a filter reads a reference as something else (D556): compares
/// it with a string, which never equals an object, so `claim == "claim-3"`
/// held for nothing, or computes with it, as `claim == claim-3` does, an
/// object less a number; a reference is compared with another or by its
/// `.id`.
fn misread_reference(lib: &Library, t: &Type, e: &Expr) -> bool {
    // the reference a path names, where it names one
    fn reference<'a>(lib: &'a Library, t: &'a Type, e: &Expr) -> Option<&'a Type> {
        match e {
            Expr::Name(m) => match t.attribute(m).map(|a| &a.declared) {
                Some(Declared::Reference(target)) => lib.type_(target),
                _ => None,
            },
            Expr::Path(base, m) => {
                let owner = reference(lib, t, base)?;
                match owner.attribute(m).map(|a| &a.declared) {
                    Some(Declared::Reference(target)) => lib.type_(target),
                    _ => None,
                }
            }
            _ => None,
        }
    }
    let is_ref = |x: &Expr| reference(lib, t, x).is_some();
    match e {
        Expr::Bin(op, a, b) => {
            let compared = matches!(
                op,
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
            );
            let computed = matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div);
            (compared
                && ((is_ref(a) && matches!(**b, Expr::Str(_)))
                    || (is_ref(b) && matches!(**a, Expr::Str(_)))))
                || (computed && (is_ref(a) || is_ref(b)))
                || misread_reference(lib, t, a)
                || misread_reference(lib, t, b)
        }
        Expr::Not(x) | Expr::Neg(x) | Expr::IsNull(x, _) => misread_reference(lib, t, x),
        Expr::In(a, b) => misread_reference(lib, t, a) || misread_reference(lib, t, b),
        Expr::If(c, a, b) => {
            misread_reference(lib, t, c)
                || misread_reference(lib, t, a)
                || misread_reference(lib, t, b)
        }
        _ => false,
    }
}

/// Whether a filter compares a member with a literal its type cannot hold
/// (D580): a timestamp, a date, a state or an enumeration member with any
/// literal, since none is written as one (a time is read against `now`, a
/// state and a member by name); text with a number; a number with text; a
/// truth with anything but a truth. Such a comparison is never true, or
/// cannot be made, so the filter is refused rather than answered empty.
/// Whether a filter reads what no stored object answers, so that it is
/// unknown for every object and the page answered empty: an evaluator, which
/// a read never asks; the request's `actor`, `inputs` or `this_event`, which
/// a read has none of; or a division by a literal zero (D580, fourth
/// production review A-10).
fn unanswerable(lib: &Library, t: &Type, e: &Expr) -> bool {
    let here = match e {
        Expr::Call(f, _) => matches!(&**f, Expr::Path(base, m)
            if matches!(&**base, Expr::Name(ns)
                if lib.evaluators.iter().any(|x| x.split_once('.') == Some((ns.as_str(), m.as_str()))))),
        Expr::Name(n) => {
            matches!(n.as_str(), "actor" | "inputs" | "this_event") && t.attribute(n).is_none()
        }
        // a literal zero, a number's or money's (ADR-0192 decision 7)
        Expr::Bin(BinOp::Div, _, d) => {
            matches!(&**d, Expr::Num(n) if zero(n))
                || matches!(&**d, Expr::Money(_, amount)
                    if amount.bytes().all(|b| matches!(b, b'0' | b'.' | b'-')))
        }
        _ => false,
    };
    here || e.children().into_iter().any(|c| unanswerable(lib, t, c))
}

/// The most aggregates a query filter nests, and the most members a set
/// it writes holds: past either it is refused, where aggregates nested six
/// deep over a claim's lines ran past 30 s each and a set of 10,000 values
/// took 7.8 s, each holding a read's slot while the worklists waited
/// (fifth production review FILTER-COST, ADR-0190 decision 3).
pub const FILTER_AGGREGATES: usize = 2;

pub const FILTER_MEMBERS: usize = 1_000;

/// The most terms a query filter holds, past which it is refused, where an
/// `or` of 10,000 comparisons, balanced under the depth bound, ran to the
/// read's deadline (sixth production review, ADR-0192 decision 3).
pub const FILTER_NODES: usize = 256;

/// The terms of an expression, each node of its tree, a set of literals
/// one term, which `FILTER_MEMBERS` bounds.
fn nodes(e: &Expr) -> usize {
    let literal = |x: &&Expr| {
        matches!(
            x,
            Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Dur(..) | Expr::Money(..)
        )
    };
    match e {
        Expr::Set(items) => {
            1 + items
                .iter()
                .filter(|x| !literal(x))
                .map(nodes)
                .sum::<usize>()
        }
        _ => 1 + e.children().into_iter().map(nodes).sum::<usize>(),
    }
}

/// Whether an expression writes a set holding a set.
fn sets_within(e: &Expr) -> bool {
    match e {
        Expr::Set(items) => {
            items.iter().any(|x| matches!(x, Expr::Set(_))) || items.iter().any(sets_within)
        }
        _ => e.children().into_iter().any(sets_within),
    }
}

/// Whether an expression calls a metric anywhere within it.
fn calls_metric(e: &Expr) -> bool {
    matches!(e, Expr::Metric { .. }) || e.children().into_iter().any(calls_metric)
}

/// Why a reader's filter costs more than a read takes, the reason a
/// `malformed_filter` refusal gives, or none: it calls a metric, holds more
/// than [`FILTER_NODES`] terms, nests aggregates deeper than
/// [`FILTER_AGGREGATES`] or writes a set of more than [`FILTER_MEMBERS`]. A
/// query's filter and a metric's take the same bounds (ADR-0190, ADR-0192,
/// ADR-0205 decision 15).
pub(crate) fn filter_cost(e: &Expr) -> Option<String> {
    if calls_metric(e) {
        return Some(
            "it calls a metric, which a filter does not: a metric read per row ran to the read's deadline; read the metric itself (ADR-0192)".to_string(),
        );
    }
    if nodes(e) > FILTER_NODES {
        return Some(format!(
            "it holds more than {FILTER_NODES} terms; a long list of alternatives is written as a set, `number in {{\"C-1\", \"C-2\"}}` (ADR-0192)"
        ));
    }
    if costly(e) {
        return Some(format!(
            "it nests aggregates more than {FILTER_AGGREGATES} deep or writes a set of more than {FILTER_MEMBERS} members (ADR-0190)"
        ));
    }
    None
}

/// Whether a filter nests aggregates deeper, or writes a larger set, than a
/// query takes.
fn costly(e: &Expr) -> bool {
    fn depth(e: &Expr) -> usize {
        let inner = e.children().into_iter().map(depth).max().unwrap_or(0);
        inner + usize::from(matches!(e, Expr::Agg { .. }))
    }
    fn widest(e: &Expr) -> usize {
        let inner = e.children().into_iter().map(widest).max().unwrap_or(0);
        match e {
            Expr::Set(items) => inner.max(items.len()),
            _ => inner,
        }
    }
    depth(e) > FILTER_AGGREGATES || widest(e) > FILTER_MEMBERS
}

/// Whether a number written in an expression is zero.
fn zero(n: &crate::expr::Number) -> bool {
    match n {
        crate::expr::Number::Int(i) => *i == 0,
        crate::expr::Number::Decimal(d) => d.bytes().all(|b| matches!(b, b'0' | b'.' | b'-')),
    }
}

/// Whether a literal of a filter is past what its type holds, a money of
/// 5,000 digits or a duration of 10^15 weeks, which every object's
/// evaluation answered not run, or a type with no object answered empty
/// (fourth production review A-10).
pub(crate) fn literal_out_of_range(
    world: &World<'_>,
    t: Option<&Type>,
    e: &Expr,
) -> Result<bool, NotRun> {
    if matches!(e, Expr::Num(_) | Expr::Money(..) | Expr::Dur(..)) {
        // past its type's range, a literal has no value whatever object
        // reads it (§8.2), where an int or a duration past 64 bits escaped
        // as an error once its evaluation answered it out of range
        return match Evaluator::new(world, t, None).eval(e) {
            Err(NotRun::Unsupported(_) | NotRun::OutOfRange(_)) => Ok(true),
            Err(other) => Err(other),
            Ok(_) => Ok(false),
        };
    }
    for c in e.children() {
        if literal_out_of_range(world, t, c)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether an operand of `and`, `or`, `implies` or `not` in a filter is
/// plainly no boolean: a literal of another kind, a stored member of
/// another type, or arithmetic, where `title == "x" and title` answered an
/// empty page, the operand never read once the first decided (seventh
/// production review, ADR-0194 decision 6).
fn mistyped_logic(t: &Type, e: &Expr) -> bool {
    let not_boolean = |x: &Expr| -> bool {
        match x {
            Expr::Str(_) | Expr::Num(_) | Expr::Dur(..) | Expr::Money(..) => true,
            Expr::Bin(op, ..) => matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div),
            Expr::Name(m) => match m.as_str() {
                "state" | "created_at" => true,
                _ => t.attribute(m).is_some_and(|a| {
                    a.many || !matches!(&a.declared, Declared::Scalar(ty) if ty == "bool")
                }),
            },
            _ => false,
        }
    };
    match e {
        Expr::Bin(BinOp::And | BinOp::Or | BinOp::Implies, a, b) => {
            not_boolean(a) || not_boolean(b) || mistyped_logic(t, a) || mistyped_logic(t, b)
        }
        Expr::Bin(_, a, b) => mistyped_logic(t, a) || mistyped_logic(t, b),
        Expr::Not(x) => not_boolean(x) || mistyped_logic(t, x),
        Expr::Neg(x) | Expr::IsNull(x, _) => mistyped_logic(t, x),
        Expr::If(c, a, b) => {
            not_boolean(c) || mistyped_logic(t, c) || mistyped_logic(t, a) || mistyped_logic(t, b)
        }
        _ => false,
    }
}

fn mistyped_literal(lib: &Library, t: &Type, e: &Expr) -> bool {
    // the declared type of the member a name or a path reads, where it is
    // a stored scalar or the state
    fn member<'a>(lib: &'a Library, t: &'a Type, e: &Expr) -> Option<String> {
        let (owner, m) = match e {
            Expr::Name(m) => (t, m),
            Expr::Path(base, m) => {
                let Expr::Name(r) = &**base else {
                    return None;
                };
                match t.attribute(r).map(|a| &a.declared) {
                    Some(Declared::Reference(target)) => (lib.type_(target)?, m),
                    _ => return None,
                }
            }
            _ => return None,
        };
        match m.as_str() {
            "state" => return Some("state".to_string()),
            "created_at" => return Some("timestamp".to_string()),
            _ => {}
        }
        match owner.attribute(m) {
            Some(a) if !a.many => match &a.declared {
                Declared::Scalar(ty) => Some(ty.clone()),
                Declared::Reference(_) => None,
            },
            _ => None,
        }
    }
    let wrong = |ty: &str, literal: &Expr| -> bool {
        let base = ty.split('(').next().unwrap_or(ty);
        match literal {
            Expr::Str(_) => !matches!(base, "string" | "identity"),
            Expr::Num(_) => !matches!(base, "int" | "decimal" | "money"),
            Expr::Bool(_) => base != "bool",
            Expr::Dur(..) => base != "duration",
            Expr::Money(..) => base != "money",
            _ => false,
        }
    };
    match e {
        Expr::Bin(op, a, b) => {
            let compared = matches!(
                op,
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
            );
            (compared
                && (member(lib, t, a).is_some_and(|ty| wrong(&ty, b))
                    || member(lib, t, b).is_some_and(|ty| wrong(&ty, a))))
                || mistyped_literal(lib, t, a)
                || mistyped_literal(lib, t, b)
        }
        Expr::Not(x) | Expr::Neg(x) | Expr::IsNull(x, _) => mistyped_literal(lib, t, x),
        Expr::If(c, a, b) => {
            mistyped_literal(lib, t, c)
                || mistyped_literal(lib, t, a)
                || mistyped_literal(lib, t, b)
        }
        _ => false,
    }
}

pub fn query(
    world: &World<'_>,
    type_: &str,
    filter: Option<&str>,
    order: &[String],
    fields: Option<&[String]>,
) -> Result<QueryPage, NotRun> {
    query_page(world, type_, filter, None, order, fields, None, usize::MAX)
}

/// A read's parameters, by name, as its request gives them (ADR-0218).
pub type Params = serde_json::Map<String, serde_json::Value>;

/// What a parameter is read as: a member's declared type, or the queried
/// type's state.
enum Reads {
    Declared(Declared),
    State(String),
}

/// What a filter's operand reads, where it says how to read a value
/// compared with it: a stored member of the type, by its bare name or
/// through `this`, or one reference away, an object's `id` and
/// `created_at`, and the state; and whether it holds a set.
fn reads_as(lib: &Library, t: &Type, e: &Expr) -> Option<(Reads, bool)> {
    fn of(t: &Type, m: &str) -> Option<(Reads, bool)> {
        match m {
            "state" => Some((Reads::State(t.name.clone()), false)),
            "id" => Some((Reads::Declared(Declared::Scalar("string".into())), false)),
            "created_at" => Some((Reads::Declared(Declared::Scalar("timestamp".into())), false)),
            _ => t
                .attribute(m)
                .map(|a| (Reads::Declared(a.declared.clone()), a.many)),
        }
    }
    match e {
        Expr::Name(m) => of(t, m),
        Expr::Path(base, m) => match &**base {
            Expr::Name(b) if b == "this" => of(t, m),
            Expr::Name(r) => match t.attribute(r).map(|a| &a.declared) {
                Some(Declared::Reference(target)) => of(lib.type_(target)?, m),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

/// A read's filter with its parameters bound: each `:name` replaced by the
/// value `params` gives it, read as the member it is compared with is
/// declared, as a request's input of that type is read, so a date, an
/// instant, an amount or an object's id is stated as data, never written
/// into the filter's text, and no caller escapes a value or states a day
/// as an offset from the engine's clock (issue #24, ADR-0218). A parameter
/// compared with nothing that says how to read it, one `params` does not
/// give, a value that is not of its member's type, and one given and never
/// named are each refused, naming the parameter and never its value.
pub fn bound(lib: &Library, t: &Type, e: Expr, params: &Params) -> Result<Expr, String> {
    fn contains_param(e: &Expr) -> bool {
        matches!(e, Expr::Param(_)) || e.children().into_iter().any(contains_param)
    }
    fn value(
        lib: &Library,
        n: &str,
        reads: &Reads,
        params: &Params,
        used: &mut BTreeSet<String>,
    ) -> Result<Expr, String> {
        let Some(given) = params.get(n) else {
            return Err(format!(
                "it names the parameter :{n}, which params does not give"
            ));
        };
        used.insert(n.to_string());
        let v = match reads {
            // its member's type named, never its value, which may be personal
            Reads::Declared(declared) => {
                crate::wire::decode(given, declared, false, lib).map_err(|_| {
                    let ty = match declared {
                        Declared::Scalar(ty) => format!("a {ty}"),
                        Declared::Reference(target) => format!("the id of a {target}"),
                    };
                    format!("the parameter :{n} is not a value of its member's type, {ty}")
                })?
            }
            Reads::State(tn) => {
                let named = given.as_str().unwrap_or_default();
                let name = named.strip_prefix(&format!("{tn}.")).unwrap_or(named);
                if lib.type_(tn).and_then(|t| t.state(name)).is_none() {
                    return Err(format!("the parameter :{n} names no state of {tn}"));
                }
                Value::State {
                    type_: tn.clone(),
                    name: name.to_string(),
                }
            }
        };
        Ok(Expr::Lit(Box::new(v)))
    }
    fn go(
        lib: &Library,
        t: &Type,
        e: Expr,
        params: &Params,
        used: &mut BTreeSet<String>,
    ) -> Result<Expr, String> {
        let side = |other: &Expr, this: Expr, used: &mut BTreeSet<String>| match (
            &this,
            reads_as(lib, t, other),
        ) {
            (Expr::Param(n), Some((reads, _))) => value(lib, n, &reads, params, used),
            _ => go(lib, t, this, params, used),
        };
        match e {
            Expr::Bin(op, a, b)
                if matches!(
                    op,
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
                ) =>
            {
                let a2 = side(&b, *a.clone(), used)?;
                let b2 = side(&a, *b, used)?;
                Ok(Expr::Bin(op, Box::new(a2), Box::new(b2)))
            }
            Expr::Bin(op, a, b) => Ok(Expr::Bin(
                op,
                Box::new(go(lib, t, *a, params, used)?),
                Box::new(go(lib, t, *b, params, used)?),
            )),
            // a member among parameters, each read as the member is, or a
            // parameter among a set's elements, read as an element
            Expr::In(x, set) => match (*x, *set) {
                (member, Expr::Set(items)) if reads_as(lib, t, &member).is_some() => {
                    let reads = reads_as(lib, t, &member).map(|(r, _)| r);
                    let mut out = Vec::new();
                    for item in items {
                        out.push(match (&item, &reads) {
                            (Expr::Param(n), Some(r)) => value(lib, n, r, params, used)?,
                            _ => go(lib, t, item, params, used)?,
                        });
                    }
                    Ok(Expr::In(Box::new(member), Box::new(Expr::Set(out))))
                }
                (Expr::Param(n), member) if reads_as(lib, t, &member).is_some_and(|(_, many)| many) => {
                    let (reads, _) = reads_as(lib, t, &member).ok_or_else(String::new)?;
                    Ok(Expr::In(
                        Box::new(value(lib, &n, &reads, params, used)?),
                        Box::new(member),
                    ))
                }
                (x, set) => Ok(Expr::In(
                    Box::new(go(lib, t, x, params, used)?),
                    Box::new(go(lib, t, set, params, used)?),
                )),
            },
            Expr::Not(x) => Ok(Expr::Not(Box::new(go(lib, t, *x, params, used)?))),
            Expr::Neg(x) => Ok(Expr::Neg(Box::new(go(lib, t, *x, params, used)?))),
            Expr::IsNull(x, negated) => Ok(Expr::IsNull(Box::new(go(lib, t, *x, params, used)?), negated)),
            Expr::If(c, a, b) => Ok(Expr::If(
                Box::new(go(lib, t, *c, params, used)?),
                Box::new(go(lib, t, *a, params, used)?),
                Box::new(go(lib, t, *b, params, used)?),
            )),
            Expr::Param(n) => Err(format!(
                "the parameter :{n} is compared with no member that says how to read it; compare it with one, as day >= :{n}"
            )),
            other if contains_param(&other) => Err(
                "a parameter is read only compared with a member of the type, or one reference away, or among a set's elements".to_string(),
            ),
            other => Ok(other),
        }
    }
    let mut used = BTreeSet::new();
    let out = go(lib, t, e, params, &mut used)?;
    if let Some(unused) = params.keys().find(|k| !used.contains(*k)) {
        return Err(format!(
            "params gives :{unused}, which the filter never names"
        ));
    }
    Ok(out)
}

/// How many objects of a type a page in creation order reads at a time, at
/// least, from its cursor (READ-PAGES).
const CHUNK: usize = 256;

/// Whether a query reads its type whole: ordered or filtered, with a
/// filter that neither requires the states it holds nor holds a column equal
/// to a value, the indexes a page is read through. Such a query waits among
/// the analytics, never in a person's worklist slot, where ten of them left a
/// plain worklist query timing out (ninth production review, ADR-0196
/// decision 4). So does one whose filter aggregates over a collection for
/// each object, itself or through a derived attribute it reads, however its
/// page is narrowed: eight such reads of 8 s each on six years made a plain
/// worklist wait 16 s (twelfth production review). So does one computing a
/// field, asked for or in its type's summary, that is a derived attribute
/// growing with the store (`grows_with_store`), where four such queries of
/// a type-scanning count, 17 ms a row, made a worklist wait 5.6 s
/// (fourteenth production review N7). A filter that does not parse is no
/// scan: it is refused.
pub fn query_scans(
    lib: &Library,
    type_: &str,
    filter: Option<&str>,
    order: &[String],
    fields: Option<&[String]>,
) -> bool {
    if let Some(t) = lib.type_(type_) {
        let summary: Vec<String> = t
            .source
            .get("summary")
            .and_then(|s| s.as_array())
            .map(|s| {
                s.iter()
                    .filter_map(|f| f.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let growing = growing_derived(lib);
        let computes = |n: &String| growing.contains(n) && t.derived.iter().any(|(d, _)| d == n);
        if fields
            .map_or(summary.as_slice(), |f| f)
            .iter()
            .any(computes)
        {
            return true;
        }
    }
    // read as a query reads it, its parameters among it (ADR-0218)
    let parsed = match filter.map(crate::expr::parse_filter) {
        Some(Ok(e)) => Some(e),
        Some(Err(_)) => return false,
        None => None,
    };
    if parsed.is_none() && order.is_empty() {
        return false;
    }
    parsed.as_ref().is_none_or(|f| {
        (required_states(lib, type_, f).is_none() && required_columns(lib, type_, f).is_empty())
            || aggregates(lib, type_, f, &mut BTreeSet::new())
    })
}

/// What a read of one object computes of its type's rules: a `get` its
/// derived attributes, an `availability` its conditions too, and a `check`
/// its invariants as well.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectRead {
    Get,
    Availability,
    Check,
}

/// Whether a read of an object of a type computes what grows with the
/// store: a rule it computes, a derived attribute, a condition or an
/// invariant as `what` reads them, reading a metric, a type scan or the
/// objects referring to it, itself or through a derived attribute it reads
/// by name. Such a read waits in a pool, where it ran in none: 64
/// `availability`s of a type guarded by a monthly limit's metric peaked at
/// 4.49 GB (thirteenth production review C); and a `get` waits only where
/// its derived attributes grow, where every `get` of a type with a guarded
/// transition waited, 25.5 s behind per-person metrics (fourteenth
/// production review).
pub fn grows_with_store(lib: &Library, type_: &str, what: ObjectRead) -> bool {
    // a proposal's read carries the verdict its approvals' attempts left it
    // with (B14)
    if type_ == crate::model::PROPOSAL {
        return true;
    }
    let growing = growing_derived(lib);
    let Some(t) = lib.type_(type_) else {
        return false;
    };
    let conditions = t
        .conditions
        .iter()
        .map(|(_, c)| &c.expression)
        .filter(|_| what != ObjectRead::Get);
    let invariants = t
        .invariants
        .iter()
        .map(|(_, e)| e)
        .filter(|_| what == ObjectRead::Check);
    t.derived
        .iter()
        .map(|(_, e)| e)
        .chain(conditions)
        .chain(invariants)
        .any(|e| wide(lib, e) || reads(e, &growing))
}

/// Whether an expression reads what grows with the store itself: a metric,
/// a type scan by a type's name, or the objects referring to one; or what
/// grows with one object's life, read by an index: its transitions, its
/// attempts, its declaration version and an observation collection, which
/// a rule of one object's read the whole log, attempt log or kind for (the
/// audit's cost B1, ADR-0205 decision 12).
fn wide(lib: &Library, e: &Expr) -> bool {
    let growing_member = |m: &str| {
        matches!(m, "transitions" | "attempts" | "declaration_version")
            || lib
                .types
                .values()
                .any(|t| t.observations.iter().any(|(c, _)| c == m))
    };
    match e {
        Expr::Metric { .. } => true,
        Expr::Name(n) | Expr::Path(_, n) if growing_member(n) => true,
        // a type scan: an aggregate over a type's objects by its name
        Expr::Agg {
            collection: Some(c),
            ..
        } if matches!(&**c, Expr::Name(n) if lib.type_(n).is_some()) => true,
        Expr::Name(n) if n == "referrers" => true,
        Expr::Path(_, m) if m == "referrers" => true,
        _ => e.children().into_iter().any(|c| wide(lib, c)),
    }
}

/// Whether an expression reads a member of one of the names given.
fn reads(e: &Expr, names: &BTreeSet<String>) -> bool {
    match e {
        Expr::Name(n) if names.contains(n) => true,
        Expr::Path(_, m) if names.contains(m) => true,
        _ => e.children().into_iter().any(|c| reads(c, names)),
    }
}

/// The derived attributes whose values grow with the store, by name in any
/// type, to a fixpoint: each reading what grows, itself or through another
/// it reads by name.
fn growing_derived(lib: &Library) -> BTreeSet<String> {
    let mut growing: BTreeSet<String> = BTreeSet::new();
    loop {
        let mut grew = false;
        for t in lib.types.values() {
            for (d, e) in &t.derived {
                if !growing.contains(d) && (wide(lib, e) || reads(e, &growing)) {
                    growing.insert(d.clone());
                    grew = true;
                }
            }
        }
        if !grew {
            return growing;
        }
    }
}

/// The members a read may not select by without its request's opt-in:
/// those marked personal, by name in any type, an input written into one
/// and a derived attribute reading one, and, where an actor type marks its
/// identity personal, every member of type `identity`, a copy of an
/// identity being the identity (ADR-0206 decision 1).
pub fn personal_members(lib: &Library) -> BTreeSet<String> {
    let mut out = crate::eval::personal_names(lib);
    if crate::eval::identities_personal(lib) {
        out.extend(crate::eval::identity_names(lib));
    }
    out
}

/// What a library holds personal, read by type (ADR-0219): each type's
/// members personal by its own declaration, a derived attribute reading
/// one of the members its expression reaches, and, where an actor type
/// marks its identity personal, every member of type `identity`; and the
/// names personal in any type, for a member whose type an expression does
/// not tell, judged by its name as every member was.
struct Personal<'l> {
    lib: &'l Library,
    derived: BTreeSet<(String, String)>,
    identities: bool,
    anywhere: BTreeSet<String>,
    /// The transition whose guards are judged, and its type, whose inputs
    /// `inputs.<name>` reads: none for a query, whose filter has none.
    inputs: Option<(&'l Type, &'l Transition)>,
}

/// The names an expression binds, each to the type its objects are of,
/// where the expression tells it.
type Bound = BTreeMap<String, Option<String>>;

impl<'l> Personal<'l> {
    fn of(lib: &'l Library) -> Personal<'l> {
        let mut p = Personal {
            lib,
            derived: BTreeSet::new(),
            identities: crate::eval::identities_personal(lib),
            anywhere: personal_members(lib),
            inputs: None,
        };
        loop {
            let mut grew = false;
            for t in lib.types.values() {
                for (d, e) in &t.derived {
                    let key = (t.name.clone(), d.clone());
                    if !p.derived.contains(&key) && p.reads(t, e, &Bound::new()) {
                        p.derived.insert(key);
                        grew = true;
                    }
                }
            }
            if !grew {
                return p;
            }
        }
    }

    /// Whether a type's own member is personal.
    fn member(&self, t: &Type, m: &str) -> bool {
        match t.attribute(m) {
            Some(a) => {
                a.personal
                    || (self.identities
                        && matches!(&a.declared, Declared::Scalar(ty) if ty == "identity"))
            }
            None => self.derived.contains(&(t.name.clone(), m.to_string())),
        }
    }

    /// Whether an input of the transition judged is personal: marked so,
    /// written or passed into a personal attribute, or of type `identity`
    /// where an actor type marks its identity personal (ADR-0206 decision
    /// 1), judged by the transition's own declaration (ADR-0220).
    fn input(&self, m: &str) -> bool {
        let Some((t, x)) = self.inputs else {
            return self.anywhere.contains(m);
        };
        crate::request::input_personal(self.lib, t, x, m)
            || (self.identities
                && x.inputs.iter().any(|(n, i)| {
                    n == m && matches!(&i.declared, Declared::Scalar(ty) if ty == "identity")
                }))
    }

    /// The type of the objects an expression names, where it tells: `this`,
    /// a name bound to a type, and a single reference, by itself or at the
    /// end of a path of them.
    fn type_of(&self, t: &'l Type, e: &Expr, bound: &Bound) -> Option<&'l Type> {
        let one = |owner: &Type, m: &str| match owner.attribute(m) {
            Some(a) if !a.many => match &a.declared {
                Declared::Reference(target) => self.lib.type_(target),
                Declared::Scalar(_) => None,
            },
            _ => None,
        };
        match e {
            Expr::Name(n) if n == "this" => Some(t),
            Expr::Name(n) => match bound.get(n) {
                Some(Some(tn)) => self.lib.type_(tn),
                Some(None) => None,
                None => one(t, n),
            },
            Expr::Path(b, m) => {
                let owner = self.type_of(t, b, bound)?;
                one(owner, m)
            }
            _ => None,
        }
    }

    /// The type of the elements of a collection an aggregate ranges over,
    /// where it tells: a set of references, or a type scanned whole.
    fn element_type(&self, t: &'l Type, c: &Expr, bound: &Bound) -> Option<&'l Type> {
        let set = |owner: &Type, m: &str| match owner.attribute(m) {
            Some(a) if a.many => match &a.declared {
                Declared::Reference(target) => self.lib.type_(target),
                Declared::Scalar(_) => None,
            },
            _ => None,
        };
        match c {
            Expr::Name(n) if !bound.contains_key(n) && t.attribute(n).is_none() => {
                self.lib.type_(n)
            }
            Expr::Name(n) if !bound.contains_key(n) => set(t, n),
            Expr::Path(b, m) => set(self.type_of(t, b, bound)?, m),
            _ => None,
        }
    }

    /// Whether an expression, read on an object of `t`, reads a personal
    /// member: a member of `t` by its bare name, one a path through
    /// references or an aggregate's binding reaches on its own type, and
    /// one whose type it does not tell, by its name in any type.
    fn reads(&self, t: &'l Type, e: &Expr, bound: &Bound) -> bool {
        match e {
            Expr::Name(n) => !bound.contains_key(n) && self.member(t, n),
            // a transition's input, as its own declaration holds it
            Expr::Path(b, m)
                if self.inputs.is_some()
                    && matches!(&**b, Expr::Name(n) if n == "inputs" && !bound.contains_key(n)) =>
            {
                self.input(m)
            }
            Expr::Path(b, m) => {
                self.reads(t, b, bound)
                    || match self.type_of(t, b, bound) {
                        Some(owner) => self.member(owner, m),
                        None => self.anywhere.contains(m),
                    }
            }
            Expr::Agg {
                var,
                collection,
                filter,
                body,
                ..
            } => {
                let mut inner = bound.clone();
                if let Some(v) = var {
                    let of = collection
                        .as_deref()
                        .and_then(|c| self.element_type(t, c, bound))
                        .map(|et| et.name.clone());
                    inner.insert(v.clone(), of);
                }
                collection
                    .as_deref()
                    .is_some_and(|c| self.reads(t, c, bound))
                    || filter.as_deref().is_some_and(|f| self.reads(t, f, &inner))
                    || body.as_deref().is_some_and(|b| self.reads(t, b, &inner))
            }
            other => other
                .children()
                .into_iter()
                .any(|c| self.reads(t, c, bound)),
        }
    }
}

/// Whether a query's filter or order reads a personal member, which its
/// request then opts into or is refused `personal` (ADR-0206 decision 2),
/// whoever asks, where an agent's alone was refused, by the actor's kind
/// (ADR-0201 decision 4, ADR-0202 decision 3, superseded). A member is
/// judged as the queried type and the types its paths reach declare it, so
/// a room's own `name` is not personal because a contact's is, which made a
/// personal name of one module reserved in every other (issue #23,
/// ADR-0219); one whose type the filter does not tell is judged by its
/// name, as every member was.
pub fn query_reads_personal(
    lib: &Library,
    type_: &str,
    filter: Option<&str>,
    order: &[String],
) -> bool {
    let Some(t) = lib.type_(type_) else {
        return false;
    };
    let personal = Personal::of(lib);
    // parsed as a query parses it, its parameters among it, so a filter
    // naming one is judged by the members it reads, where one that did not
    // parse read as reading nothing personal (ADR-0218)
    let filtered = filter
        .and_then(|f| crate::expr::parse_filter(f).ok())
        .is_some_and(|e| personal.reads(t, &e, &Bound::new()));
    let ordered = order.iter().any(|k| {
        let key = k.split_whitespace().next().unwrap_or_default();
        let member = key
            .strip_prefix("entered_at(")
            .and_then(|m| m.strip_suffix(')'))
            .unwrap_or(key);
        personal.member(t, member)
    });
    filtered || ordered
}

/// Whether a transition's answer is decided by a personal member: a guard
/// it enforces, or an invariant of its type, reading one, which `check`,
/// `availability` and `available` then opt into or are refused
/// `personal`, where an agent's `available` listed the people a salary
/// guard passed (the audit's personal data R5, R15, R17; ADR-0206
/// decision 2). Each member is judged as a query's is, by the type it
/// belongs to, and an input by the transition's own declaration, where a
/// name personal in any type or transition made a guard on a room's own
/// `name`, or on an input named like another transition's personal one,
/// refused to every caller without the opt-in (ADR-0220).
pub fn transition_reads_personal(lib: &Library, type_: &str, transition: &str) -> bool {
    let Some(t) = lib.type_(type_) else {
        return false;
    };
    let x = t.transition(transition);
    let mut personal = Personal::of(lib);
    personal.inputs = x.map(|x| (t, x));
    let guards = x
        .map(|x| x.guards.iter().map(|(g, _)| g.clone()).collect::<Vec<_>>())
        .unwrap_or_default();
    let by_guard = t
        .conditions
        .iter()
        .filter(|(n, _)| guards.contains(n))
        .any(|(_, c)| personal.reads(t, &c.expression, &Bound::new()));
    let by_invariant = t
        .invariants
        .iter()
        .any(|(_, i)| personal.reads(t, i, &Bound::new()));
    by_guard || by_invariant
}

/// Whether a refusal's clause decides by the holder of a personal value: a
/// uniqueness over a personal member, or `identity_taken` where an actor
/// type marks its identity personal, whose answer then names no object
/// without its request's opt-in (ADR-0206 decision 3).
pub fn clause_names_personal_holder(lib: &Library, clause: &str) -> bool {
    if clause == crate::request::IDENTITY_TAKEN {
        return crate::eval::identities_personal(lib);
    }
    clause
        .strip_suffix("_unique")
        .is_some_and(|a| personal_members(lib).contains(a))
}

/// Whether a `lookup` by an external identifier of this source matches a
/// personal attribute's value, which its request then opts into or is
/// refused `personal` (the audit's personal data R6, ADR-0206 decision 2).
pub fn lookup_reads_personal(lib: &Library, source: &str) -> bool {
    let personal = personal_members(lib);
    lib.types.values().any(|t| {
        t.attributes.iter().any(|(n, a)| {
            a.external.as_deref() == Some(source) && (a.personal || personal.contains(n))
        })
    })
}

/// Whether an expression aggregates over a collection, itself or through a
/// derived attribute of the type it reads by name, each read once.
fn aggregates(lib: &Library, type_: &str, e: &Expr, seen: &mut BTreeSet<String>) -> bool {
    if matches!(e, Expr::Agg { .. }) {
        return true;
    }
    let named = match e {
        Expr::Name(n) => Some(n),
        Expr::Path(base, n) if matches!(&**base, Expr::Name(b) if b == "this") => Some(n),
        _ => None,
    };
    if let Some(n) = named
        && let Some((_, derived)) = lib
            .type_(type_)
            .and_then(|t| t.derived.iter().find(|(d, _)| d == n))
        && seen.insert(n.clone())
        && aggregates(lib, type_, derived, seen)
    {
        return true;
    }
    e.children()
        .into_iter()
        .any(|c| aggregates(lib, type_, c, seen))
}

/// A page of a query (ADR-0179): at most `limit` objects, after the one
/// the cursor names in the query's order, which is the next page's place
/// whatever was written since; the cursor of a further page, none where
/// this is the last; a cursor of another read refused `malformed_cursor`.
#[allow(clippy::too_many_arguments)]
pub fn query_page(
    world: &World<'_>,
    type_: &str,
    filter: Option<&str>,
    params: Option<&Params>,
    order: &[String],
    fields: Option<&[String]>,
    after: Option<&str>,
    limit: usize,
) -> Result<QueryPage, NotRun> {
    // the parameters too, so a cursor continues only a read given the same
    let read = format!(
        "query\u{1f}{type_}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        filter.unwrap_or_default(),
        params
            .map(|p| serde_json::Value::Object(p.clone()).to_string())
            .unwrap_or_default(),
        order.join(","),
        fields.map(|f| f.join(",")).unwrap_or_default()
    );
    let refuse = |why: &str| {
        Ok(QueryPage {
            items: Vec::new(),
            refused: Some(why.to_string()),
            reason: None,
            cursor: None,
        })
    };
    let malformed = |why: String| {
        Ok(QueryPage {
            items: Vec::new(),
            refused: Some("malformed_filter".to_string()),
            reason: Some(why),
            cursor: None,
        })
    };
    let Some(t) = world.lib.type_(type_) else {
        return refuse("unknown_type");
    };
    let from = match after {
        None => None,
        Some(c) => match continued(world, &read, type_, c)? {
            Some(o) => Some(o),
            None => return refuse("malformed_cursor"),
        },
    };
    if filter.is_none() && params.is_some_and(|p| !p.is_empty()) {
        return malformed(
            "params gives parameters, and there is no filter to name them".to_string(),
        );
    }
    let empty = Params::new();
    let parsed = filter.map(|f| {
        crate::expr::parse_filter(f)
            .map_err(|e| format!("it does not parse: {e}"))
            .and_then(|e| bound(world.lib, t, e, params.unwrap_or(&empty)))
    });
    let filter = match parsed {
        Some(Err(why)) => return malformed(why),
        Some(Ok(e)) => Some(Ok::<Expr, String>(e)),
        None => None,
    };
    let filter = match filter {
        None => None,
        Some(Ok(e)) if misread_reference(world.lib, t, &e) => {
            return malformed(
                "it compares a reference with text or computes with one; a reference is compared by its id, as claim.id == \"claim-4\"".to_string(),
            );
        }
        Some(Ok(e)) if mistyped_literal(world.lib, t, &e) => {
            return malformed(
                "it compares a member with a literal its type cannot hold".to_string(),
            );
        }
        Some(Ok(e)) if mistyped_logic(t, &e) => {
            return malformed(
                "an operand of and, or, implies or not is no boolean: a literal, a member of another type or arithmetic".to_string(),
            );
        }
        Some(Ok(e)) if unanswerable(world.lib, t, &e) => {
            return malformed(
                "no stored object can answer it: it calls an evaluator, reads the request's actor, inputs or this_event, or divides by a literal zero".to_string(),
            );
        }
        Some(Ok(e)) if sets_within(&e) => {
            return malformed(
                "it writes a set within a set, which no member holds (ADR-0192)".to_string(),
            );
        }
        Some(Ok(e)) if filter_cost(&e).is_some() => {
            return malformed(filter_cost(&e).unwrap_or_default());
        }
        Some(Ok(e)) if literal_out_of_range(world, Some(t), &e)? => {
            return malformed("it holds a literal past what its type holds".to_string());
        }
        Some(Ok(e)) => Some(e),
        Some(Err(e)) => return malformed(e),
    };
    let mut keys = Vec::new();
    for k in order {
        let (name, desc) = match k.trim().strip_suffix(" desc") {
            Some(n) => (n.trim(), true),
            None => (k.trim(), false),
        };
        let key = if name == "created_at" {
            Key::CreatedAt
        } else if let Some(m) = name
            .strip_prefix("entered_at(")
            .and_then(|r| r.strip_suffix(')'))
        {
            if !world.lib.tracked(t).iter().any(|x| x == m.trim()) {
                return refuse("unknown_order_key");
            }
            Key::EnteredAt(m.trim().to_string())
        } else if t.attribute(name).is_some_and(|a| !a.many) {
            Key::Attribute(name.to_string())
        } else {
            return refuse("unknown_order_key");
        };
        keys.push((key, desc));
    }
    let summary: Vec<String> = t
        .source
        .get("summary")
        .and_then(|s| s.as_array())
        .map(|s| {
            s.iter()
                .filter_map(|f| f.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_else(|| vec!["state".to_string()]);
    let asked: Vec<String> = fields.map_or(summary, <[String]>::to_vec);
    // where the filter requires its states, only the objects in them are
    // read, through each type's state index, and the filter then decides
    // among them as it would among all (ADR-0162 step 3)
    let states = filter
        .as_ref()
        .and_then(|f| required_states(world.lib, type_, f));
    // and through the columns its conjuncts hold equal to a value, a
    // claim's lines by `claim.id == "…"` among them, never the type whole
    // (PERF-SCAN)
    let columns = filter
        .as_ref()
        .map(|f| required_columns(world.lib, type_, f))
        .unwrap_or_default();
    // whether the filter selects an object; none where it compares what its
    // values cannot, such as a derived time with text, or a time computed
    // past what one holds, which refuses the filter as malformed rather than
    // the read as not run (D580, fourth production review A-10)
    // a filter an object of the type cannot answer, an ill-typed one among
    // them, is refused with why, where it answered 501 and wrote the value
    // it met into the log (sixth production review, ADR-0192 decision 7)
    let selects = |o: &Object| -> Result<Result<bool, String>, NotRun> {
        let Some(ot) = world.lib.type_(&o.type_) else {
            return Err(NotRun::Unsupported(format!("the type {}", o.type_)));
        };
        Ok(match &filter {
            None => Ok(true),
            Some(e) => match Evaluator::new(world, Some(ot), Some(&o.id))
                .eval(e)
                .and_then(|v| truth(&v))
            {
                Ok(t) => Ok(t == Some(true)),
                Err(NotRun::Unsupported(why)) => Err(why),
                Err(e) => return Err(e),
            },
        })
    };
    let mut chosen = Vec::new();
    if keys.is_empty() {
        // in creation order, the page is the first `limit` the filter
        // selects after the cursor, and one more says another follows: read
        // and judged from the cursor a chunk at a time, never the type
        // whole, which each page did, so reading a result page by page was
        // quadratic (READ-PAGES)
        let chunk = limit.saturating_add(1).max(CHUNK);
        let mut after = from.map_or(i64::MIN, |f| f.order);
        'reading: loop {
            let (next, more) =
                world.of_type_from(type_, states.as_ref(), &columns, after, chunk)?;
            for o in &next {
                match selects(o)? {
                    Err(why) => {
                        return malformed(format!("an object of the type cannot answer it: {why}"));
                    }
                    Ok(true) => chosen.push(*o),
                    Ok(false) => {}
                }
                if chosen.len() > limit {
                    break 'reading;
                }
            }
            match next.last() {
                Some(last) if more => after = last.order,
                _ => break,
            }
        }
    } else {
        let candidates = if states.is_none() && columns.is_empty() {
            world.of_type(type_)?
        } else {
            world.of_type_where(type_, states.as_ref(), &columns)?
        };
        for o in candidates {
            match selects(o)? {
                Err(why) => {
                    return malformed(format!("an object of the type cannot answer it: {why}"));
                }
                Ok(true) => chosen.push(o),
                Ok(false) => {}
            }
        }
    }
    let lib = world.lib;
    // the entry time of each chosen object's current interval an order key
    // or an asked field names, read before sorting, since a comparison
    // cannot stop the read
    let mut entered: BTreeMap<(String, String), Value> = BTreeMap::new();
    let members: BTreeSet<String> = keys
        .iter()
        .filter_map(|(key, _)| match key {
            Key::EnteredAt(m) => Some(m.clone()),
            _ => None,
        })
        .chain(asked.iter().filter_map(|f| {
            f.strip_prefix("entered_at(")
                .and_then(|r| r.strip_suffix(')'))
                .map(|m| m.trim().to_string())
        }))
        .collect();
    // the cursor's object is ordered as the chosen are, held or not
    let mut keyed = chosen.clone();
    keyed.extend(from);
    if !members.is_empty() {
        world.with_parts(&keyed)?;
    }
    for m in &members {
        for o in &keyed {
            let v = world
                .parts(o)?
                .spans
                .get(m)
                .and_then(|s| s.last())
                .filter(|sp| sp.left_position.is_none())
                .map_or(Value::Absent, |sp| Value::Timestamp(sp.entered_at));
            entered.insert((o.id.clone(), m.clone()), v);
        }
    }
    // a key's value for an object, and how two values of one key order
    let value = |o: &Object, key: &Key| -> Value {
        match key {
            Key::CreatedAt => Value::Timestamp(o.created_at),
            Key::EnteredAt(m) => entered
                .get(&(o.id.clone(), m.clone()))
                .cloned()
                .unwrap_or(Value::Absent),
            Key::Attribute(a) => o.attr(a),
        }
    };
    // the place in creation order of each object a key's value names, read
    // before sorting, since a comparison cannot stop the read
    let mut placed: BTreeMap<String, i64> = BTreeMap::new();
    for o in &keyed {
        for (key, _) in &keys {
            if let Value::Object(id) = value(o, key)
                && !placed.contains_key(&id)
                && let Some(r) = world.get(&id)?
            {
                placed.insert(id, r.order);
            }
        }
    }
    let rank = |v: &Value, w: &Value| -> std::cmp::Ordering {
        use std::cmp::Ordering;
        let position = |v: &Value| -> Option<i64> {
            match v {
                Value::Member {
                    enumeration,
                    member,
                } => lib
                    .enumerations
                    .get(enumeration)
                    .and_then(|ms| ms.iter().position(|m| m == member))
                    .and_then(|i| i64::try_from(i).ok()),
                Value::State { type_, name } => lib
                    .type_(type_)
                    .and_then(|t| t.states.iter().position(|(s, _)| s == name))
                    .and_then(|i| i64::try_from(i).ok()),
                Value::Object(id) => placed.get(id).copied(),
                _ => None,
            }
        };
        match (position(v), position(w)) {
            (Some(a), Some(b)) => a.cmp(&b),
            _ => v.compare(w).unwrap_or(Ordering::Equal),
        }
    };
    let ordered = |a: &Object, b: &Object| -> std::cmp::Ordering {
        for (key, desc) in &keys {
            let (x, y) = (value(a, key), value(b, key));
            // an absent value last, whichever way the key runs
            let o = match (x.is_absent(), y.is_absent()) {
                (true, true) => std::cmp::Ordering::Equal,
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                (false, false) if *desc => rank(&y, &x),
                (false, false) => rank(&x, &y),
            };
            if o != std::cmp::Ordering::Equal {
                return o;
            }
        }
        a.order.cmp(&b.order)
    };
    chosen.sort_by(|a, b| ordered(a, b));
    // after the cursor's object, as many as a page holds
    if let Some(f) = from {
        chosen.retain(|o| ordered(o, f) == std::cmp::Ordering::Greater);
    }
    let more = chosen.len() > limit;
    chosen.truncate(limit);
    let mut cursor = more
        .then(|| chosen.last().map(|o| cursor_of(&read, &o.id)))
        .flatten();
    let mut items = Vec::new();
    let mut bytes = 0;
    for o in chosen {
        // a page ends at its bytes as at its limit, one object at least,
        // its cursor going on from the last it holds
        if bytes >= PAGE_BYTES {
            cursor = items.last().map(|it: &Item| cursor_of(&read, &it.id));
            break;
        }
        let Some(ot) = lib.type_(&o.type_) else {
            return Err(NotRun::Unsupported(format!("the type {}", o.type_)));
        };
        let reader = Evaluator::new(world, Some(ot), Some(&o.id));
        let mut out = Vec::new();
        for f in &asked {
            // the entry time of a tracked member's current interval, as an
            // order key reads it (ADR-0127, ADR-0161)
            if let Some(m) = f
                .strip_prefix("entered_at(")
                .and_then(|r| r.strip_suffix(')'))
            {
                if !lib.tracked(ot).iter().any(|x| x == m.trim()) {
                    return refuse("unknown_field");
                }
                out.push((f.clone(), value(o, &Key::EnteredAt(m.trim().to_string()))));
                continue;
            }
            // an attribute, a derived value, or a member every object has
            // (flow-format.md §5)
            let known = ot.attribute(f).is_some()
                || ot.derived.iter().any(|(d, _)| d == f)
                || [
                    "state",
                    "open",
                    "created_at",
                    "created_by_kind",
                    "recorded_by_kind",
                    "recorded_from",
                ]
                .contains(&f.as_str());
            if !known {
                return refuse("unknown_field");
            }
            let member = Expr::Path(Box::new(Expr::Name("this".into())), f.clone());
            // a derived time past the calendar refuses the read, as `get`
            // refuses it (ADR-0191 decision 3, ADR-0205 decision 2)
            let v = match reader.eval(&member)? {
                Value::Timestamp(s)
                    if ot.derived.iter().any(|(d, _)| d == f)
                        && !(crate::value::FIRST_SECOND..=crate::value::LAST_SECOND)
                            .contains(&s) =>
                {
                    return Err(NotRun::OutOfRange(o.id.clone()));
                }
                v => v,
            };
            out.push((f.clone(), v));
        }
        bytes += o.id.len()
            + o.type_.len()
            + out
                .iter()
                .map(|(f, v)| f.len() + v.wire_bytes() + 4)
                .sum::<usize>()
            + 32;
        items.push(Item {
            id: o.id.clone(),
            type_: o.type_.clone(),
            fields: out,
        });
    }
    Ok(QueryPage {
        items,
        refused: None,
        reason: None,
        cursor,
    })
}

/// How available a transition is (`library-api.md` §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Available,
    AvailableWithInput,
    Unavailable,
}

impl Availability {
    pub fn text(self) -> &'static str {
        match self {
            Availability::Available => "available",
            Availability::AvailableWithInput => "available_with_input",
            Availability::Unavailable => "unavailable",
        }
    }
}

/// What one transition offers (`library-api.md` §5).
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub transition: String,
    /// The observation kind a recording creates; otherwise none.
    pub creates: Option<String>,
    pub availability: Availability,
    pub verdict: Option<Verdict>,
    /// Each input by name, with the type a caller supplies.
    pub inputs: Vec<(String, String)>,
    pub unevaluated: Vec<String>,
    pub flags: Vec<Flag>,
    /// What its refusal tells beside it: the objects its clause names,
    /// whether it was unknown, and whether the transition takes a
    /// proposal; an offer consults no evaluator, so nothing consulted.
    pub told: Told,
}

/// An offer's refusal told: the objects named, its clause unknown or
/// false, and whether the transition takes a proposal.
fn offer_told(objects: Vec<String>, unknown: bool, proposable: bool) -> Told {
    Told {
        unknown,
        objects,
        proposable,
        consulted: None,
    }
}

fn written(declared: &Declared, many: bool) -> String {
    let base = match declared {
        Declared::Scalar(t) | Declared::Reference(t) => t.clone(),
    };
    if many { format!("{base}[]") } else { base }
}

/// A transition's inputs by name, each with the type a caller supplies.
fn input_schema(t: &Type, x: &Transition) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = x
        .required_inputs
        .iter()
        .chain(&x.optional_inputs)
        .map(|k| {
            let typ = t
                .attribute(k)
                .map(|a| written(&a.declared, a.many))
                .unwrap_or_default();
            (k.clone(), typ)
        })
        .collect();
    out.extend(
        x.inputs
            .iter()
            .map(|(k, spec)| (k.clone(), written(&spec.declared, spec.many))),
    );
    if x.kind == Kind::Assertion {
        out.push(("to".into(), "state".into()));
        out.push(("admits".into(), "invariant[]".into()));
    }
    out
}

/// The states a query's filter requires of an object for it to hold: those
/// a conjunct `state == S` names, or `state == T.S`, intersected across the
/// conjuncts of an `and`; none where it requires none this can tell. A state
/// written bare is taken only where every type of the family resolves the
/// name to its state of that name, as the filter's own evaluation would.
/// The columns a filter's conjuncts hold equal to a value, which a query
/// reads its candidates by: `<reference>.id == "<id>"` of a single
/// reference, and `<attribute> == <literal>` of a stored text, identity or
/// integer attribute, each a conjunct of the filter's top-level `and`s, so
/// every object the filter holds for holds them (PERF-SCAN). The filter
/// still decides among the candidates.
fn required_columns(lib: &crate::model::Library, type_: &str, e: &Expr) -> Vec<(String, Value)> {
    use crate::expr::BinOp;
    let Some(t) = lib.type_(type_) else {
        return Vec::new();
    };
    let mut conjuncts = vec![e];
    let mut out = Vec::new();
    while let Some(c) = conjuncts.pop() {
        let Expr::Bin(op, a, b) = c else {
            continue;
        };
        match op {
            BinOp::And => {
                conjuncts.push(a);
                conjuncts.push(b);
            }
            BinOp::Eq => {
                for (side, other) in [(a, b), (b, a)] {
                    let held = match (side.as_ref(), other.as_ref()) {
                        (Expr::Path(base, id), Expr::Str(v)) if id == "id" => match base.as_ref() {
                            Expr::Name(r) => t
                                .attribute(r)
                                .filter(|x| !x.many && matches!(x.declared, Declared::Reference(_)))
                                .map(|_| (r.clone(), Value::Object(v.clone()))),
                            _ => None,
                        },
                        (Expr::Name(n), Expr::Str(v)) => t
                            .attribute(n)
                            .filter(|x| {
                                !x.many
                                    && matches!(&x.declared, Declared::Scalar(ty) if ty == "string" || ty == "identity")
                            })
                            .map(|_| (n.clone(), Value::Text(v.clone()))),
                        (Expr::Name(n), Expr::Num(crate::expr::Number::Int(k))) => t
                            .attribute(n)
                            .filter(|x| !x.many && matches!(&x.declared, Declared::Scalar(ty) if ty == "int"))
                            .map(|_| (n.clone(), Value::Int(*k))),
                        _ => None,
                    };
                    if let Some(h) = held {
                        out.push(h);
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn required_states(lib: &crate::model::Library, type_: &str, e: &Expr) -> Option<BTreeSet<String>> {
    use crate::expr::BinOp;
    let family: Vec<&crate::model::Type> = lib
        .types
        .iter()
        .filter(|(n, _)| lib.is_a(n, type_))
        .map(|(_, t)| t)
        .collect();
    // a name every type of the family reads as its member, not a constant
    let member = |n: &str| {
        family.iter().any(|t| {
            ["state", "id", "open", "created_at", "created_by_kind"].contains(&n)
                || t.attribute(n).is_some()
                || t.derived.iter().any(|(d, _)| d == n)
                || t.observations.iter().any(|(c, _)| c == n)
        })
    };
    let state_named = |side: &Expr, other: &Expr| -> Option<String> {
        if !matches!(side, Expr::Name(n) if n == "state") {
            return None;
        }
        match other {
            Expr::Name(s) if !member(s) && family.iter().all(|t| t.state(s).is_some()) => {
                Some(s.clone())
            }
            Expr::Path(base, s) => match base.as_ref() {
                Expr::Name(x)
                    if !member(x)
                        && family.iter().all(|t| t.state(x).is_none())
                        && lib.type_(x).is_some_and(|t| t.state(s).is_some()) =>
                {
                    Some(s.clone())
                }
                _ => None,
            },
            _ => None,
        }
    };
    match e {
        Expr::Bin(BinOp::And, l, r) => {
            match (
                required_states(lib, type_, l),
                required_states(lib, type_, r),
            ) {
                (Some(a), Some(b)) => Some(a.intersection(&b).cloned().collect()),
                (a, b) => a.or(b),
            }
        }
        Expr::Bin(BinOp::Eq, l, r) => state_named(l, r)
            .or_else(|| state_named(r, l))
            .map(|s| BTreeSet::from([s])),
        _ => None,
    }
}

/// What each requestable transition of an object offers its actor now.
/// `None` where no object has the id.
pub fn availability(
    world: &World<'_>,
    actor: &str,
    id: &str,
) -> Result<Option<Vec<Offer>>, NotRun> {
    offers(world, actor, id, true)
}

/// What [`availability`] offers, a guard reading a metric left unevaluated
/// where `metrics` is false, as a dry run reads one (D537).
pub(crate) fn offers(
    world: &World<'_>,
    actor: &str,
    id: &str,
    metrics: bool,
) -> Result<Option<Vec<Offer>>, NotRun> {
    offers_of(world, actor, id, metrics, None)
}

/// What [`offers`] gives, or its offer of one transition alone where one is
/// named, as `available` reads it, never every other transition's guards
/// (READ-COST).
fn offers_of(
    world: &World<'_>,
    actor: &str,
    id: &str,
    metrics: bool,
    only: Option<&str>,
) -> Result<Option<Vec<Offer>>, NotRun> {
    let Some(o) = world.get(id)? else {
        return Ok(None);
    };
    let Some(t) = world.lib.type_(&o.type_) else {
        return Err(NotRun::Unsupported(format!("the type {}", o.type_)));
    };
    if let Some(what) = t.beyond.first() {
        return Err(NotRun::Unsupported(what.clone()));
    }
    // the actor, or what refuses every offer to it (ADR-0182 decision 6)
    let who = crate::request::active(resolve_actor(world, actor)?);
    let no_inputs = BTreeMap::new();
    let mut offers = Vec::new();
    for (xn, x) in &t.transitions {
        // the operator's transitions are requested by nothing (ADR-0184)
        if x.kind == Kind::Initial
            || !x.only_via.is_empty()
            || t.name == crate::model::OPERATOR_TYPE
            || only.is_some_and(|n| n != xn)
        {
            continue;
        }
        let inputs = input_schema(t, x);
        let mut needs = x.kind == Kind::Assertion
            || !x.required_inputs.is_empty()
            || x.inputs
                .iter()
                .any(|(_, s)| !s.optional && s.default.is_none());
        let mut unevaluated = Vec::new();
        let mut flags = Vec::new();
        let refused = |v: Verdict,
                       inputs: Vec<(String, String)>,
                       unevaluated: Vec<String>,
                       told: Told| Offer {
            transition: xn.clone(),
            creates: None,
            availability: Availability::Unavailable,
            verdict: Some(v),
            inputs,
            unevaluated,
            flags: Vec::new(),
            told,
        };
        let who = match &who {
            Ok(who) => who,
            Err(v) => {
                offers.push(refused(
                    v.clone(),
                    inputs,
                    unevaluated,
                    offer_told(Vec::new(), false, x.proposable),
                ));
                continue;
            }
        };
        let state_allows = match x.kind {
            Kind::Erasure => true,
            Kind::Assertion => !t.state(&o.state).is_some_and(|s| s.is_final),
            _ => takes_from(t, x, &o.state),
        };
        if !state_allows {
            offers.push(refused(
                Verdict::Unavailable {
                    state: o.state.clone(),
                },
                inputs,
                unevaluated,
                Told::default(),
            ));
            continue;
        }
        // decided on what its effect writes; what its inputs would write waits
        // for them (ADR-0147)
        if x.kind != Kind::Erasure && world.parts(o)?.erased {
            let writes_personal = x.effect.iter().any(|st| match st {
                crate::model::Step::Assign { location, .. }
                | crate::model::Step::Add { location, .. } => {
                    t.attribute(location).is_some_and(|a| a.personal)
                }
                _ => false,
            });
            if writes_personal {
                offers.push(refused(
                    Verdict::Unsatisfied {
                        clause: "not_erased".into(),
                        remedy: Some(Remedy::UnreachableFromHere),
                    },
                    inputs,
                    unevaluated,
                    offer_told(Vec::new(), false, x.proposable),
                ));
                continue;
            }
        }
        let mut denied = None;
        for (g, mode) in &x.guards {
            let Some(c) = t.condition(g) else {
                return Err(NotRun::Unsupported(format!(
                    "the guard {g}, which {} does not declare",
                    t.name
                )));
            };
            let mut read = BTreeSet::new();
            names_read(&c.expression, &mut read);
            if read.contains("inputs") {
                needs = true; // decided when the inputs are given
                continue;
            }
            let named = crate::eval::Named::on(Some(id));
            let mut asking = Evaluator::new(world, Some(t), Some(id))
                .with_inputs(&no_inputs)
                .with_actor(who)
                .with_naming(&named);
            if !metrics {
                asking = asking.consulting_no_metric();
            }
            let ok = truth(&asking.eval(&c.expression)?)?;
            let objects = crate::request::in_id_order(world, named.borrow().objects.clone())?;
            // an evaluator is never asked by an offer (ADR-0048)
            if asking.unanswered.get() {
                unevaluated.push(g.clone());
                continue;
            }
            if ok != Some(true) {
                match mode {
                    Enforce::Deny => {
                        denied = Some((
                            Verdict::Unsatisfied {
                                clause: g.clone(),
                                remedy: c.remedy,
                            },
                            offer_told(objects, ok.is_none(), x.proposable),
                        ));
                        break;
                    }
                    Enforce::Warn => flags.push(Flag {
                        clause: g.clone(),
                        remedy: c.remedy,
                        unknown: ok.is_none(),
                        objects,
                    }),
                    Enforce::Audit => {}
                }
            }
        }
        if let Some(v) = denied {
            let (v, told) = v;
            offers.push(refused(v, inputs, unevaluated, told));
            continue;
        }
        offers.push(Offer {
            transition: xn.clone(),
            creates: None,
            availability: if needs {
                Availability::AvailableWithInput
            } else {
                Availability::Available
            },
            verdict: None,
            inputs,
            unevaluated,
            flags,
            told: Told::default(),
        });
    }
    // a proposal's decisions, which its declared lifecycle names and a
    // request takes on it by their own path: each requestable out of
    // PENDING and guarded by nothing of its own, the proposed request's
    // guards being decided when an approval executes it; `invalidate` is
    // only via a publish or an erasure (DESIGN.md §9, D535)
    if o.type_ == crate::model::PROPOSAL {
        for xn in ["approve", "reject", "withdraw"] {
            let verdict = if let Err(v) = &who {
                Some(v.clone())
            } else if o.state != "PENDING" {
                Some(Verdict::Unavailable {
                    state: o.state.clone(),
                })
            } else {
                None
            };
            offers.push(Offer {
                transition: xn.into(),
                creates: None,
                availability: if verdict.is_some() {
                    Availability::Unavailable
                } else {
                    Availability::Available
                },
                verdict,
                inputs: Vec::new(),
                unevaluated: Vec::new(),
                flags: Vec::new(),
                told: Told::default(),
            });
        }
    }
    // and recording each kind on it, and labelling, which every type
    // carries, its generated guards decided on the subject and its fields
    // waiting for the caller (ADR-0099)
    let label = crate::model::LABEL.to_string();
    let recording = only.is_none_or(|n| n == "record");
    for kind in t
        .observations
        .iter()
        .map(|(_, k)| k)
        .chain([&label])
        .filter(|_| recording)
    {
        let Some(k) = world.lib.type_(kind) else {
            return Err(NotRun::Unsupported(format!("the observation kind {kind}")));
        };
        let Some(x) = k.transition("record") else {
            return Err(NotRun::Unsupported(format!("{kind}, which has no record")));
        };
        let inputs: Vec<(String, String)> = input_schema(k, x)
            .into_iter()
            .filter(|(n, _)| n != "subject")
            .collect();
        let refusal = if let Err(v) = &who {
            Some(v.clone())
        } else if t.state(&o.state).is_some_and(|s| s.is_final) {
            Some(Verdict::Unsatisfied {
                clause: "subject_open".into(),
                remedy: Some(Remedy::UnreachableFromHere),
            })
        } else if t.mirror {
            Some(Verdict::Unsatisfied {
                clause: "subject_owned".into(),
                remedy: Some(Remedy::UnreachableFromHere),
            })
        } else {
            None
        };
        let needs = x.required_inputs.iter().any(|n| n != "subject");
        offers.push(Offer {
            transition: "record".into(),
            creates: Some(kind.clone()),
            availability: match (&refusal, needs) {
                (Some(_), _) => Availability::Unavailable,
                (None, true) => Availability::AvailableWithInput,
                (None, false) => Availability::Available,
            },
            // the subject a generated guard states (DESIGN.md §5.5)
            told: offer_told(
                match &refusal {
                    Some(Verdict::Unsatisfied { clause, .. })
                        if clause == "subject_open" || clause == "subject_owned" =>
                    {
                        vec![o.id.clone()]
                    }
                    _ => Vec::new(),
                },
                false,
                false,
            ),
            verdict: refusal,
            inputs,
            unevaluated: Vec::new(),
            flags: Vec::new(),
        });
    }
    Ok(Some(offers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Library, OPERATOR};
    use crate::request::{Request, request};
    use crate::store::Store;
    use serde_json::json;

    /// A query's members are judged personal as the types they belong to
    /// declare them: a room's own `name` is not personal because a contact's
    /// is, filtered or ordered, nor a derived attribute reading it, where
    /// marking a contact's name personal refused the room list (issue #23,
    /// ADR-0219); a contact's name is personal still, read on the contact,
    /// through a reference, through an aggregate's binding and through a
    /// derived attribute that reads it.
    #[test]
    fn a_querys_members_are_personal_as_their_own_types_declare() {
        const OFFICE: &str = r#"
module: office
categories: [live, closed]
types:
  Contact:
    attributes:
      name: { type: string, personal: true, optional: true }
    states:
      KNOWN: { category: live }
    transitions:
      add: { kind: initial, to: KNOWN, optional_inputs: [name] }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
  Room:
    attributes:
      name: { type: string, indexed: true }
      floor: { type: int, indexed: true }
      host: { reference: Contact, optional: true }
      guests: { reference: "Contact[]", optional: true }
    derived_attributes:
      label: { type: string, expression: name }
      hosted_by: { type: string, expression: host.name }
    states:
      OPEN: { category: live }
    transitions:
      add: { kind: initial, to: OPEN, required_inputs: [name, floor], optional_inputs: [host, guests] }
"#;
        let lib = Library::load(&[("office.yaml".into(), OFFICE.into())]).unwrap();
        let personal = |ty: &str, filter: Option<&str>, order: &[&str]| {
            let order: Vec<String> = order.iter().map(|o| o.to_string()).collect();
            query_reads_personal(&lib, ty, filter, &order)
        };
        // the room's own, which no type marks personal on the room
        assert!(!personal("Room", None, &["name"]));
        assert!(!personal("Room", Some("name == \"Pantry\""), &[]));
        assert!(!personal(
            "Room",
            Some("this.name == \"Pantry\" and floor > 1"),
            &["floor"]
        ));
        assert!(!personal("Room", Some("label == \"Pantry\""), &[]));
        // the contact's, wherever the filter reaches it
        assert!(personal("Contact", None, &["name"]));
        assert!(personal("Contact", Some("name == \"Ana\""), &[]));
        assert!(personal("Room", Some("host.name == \"Ana\""), &[]));
        assert!(personal(
            "Room",
            Some("count(g in guests where g.name == \"Ana\") > 0"),
            &[]
        ));
        assert!(personal(
            "Room",
            Some("count(c in Contact where c.name == \"Ana\") > 0"),
            &[]
        ));
        assert!(personal("Room", Some("hosted_by == \"Ana\""), &[]));
        assert!(personal("Room", None, &["hosted_by"]));
    }

    /// A transition's answer is decided by a personal member as its guards
    /// and its type's invariants read it, each member judged as the type it
    /// belongs to declares it and an input as its own transition declares
    /// it (ADR-0220): a guard on a room's own `name`, or on the room's own
    /// input `name`, is not personal because a contact's `name` and the
    /// input that writes it are, where both were refused to every caller
    /// without the opt-in; a contact's name read through a reference, a
    /// derived attribute or an aggregate's binding is personal still, and
    /// so is an input marked personal or of type `identity` where an
    /// actor's identity is personal, and an invariant reading what is.
    #[test]
    fn a_transitions_guards_are_personal_as_their_own_types_declare() {
        const OFFICE: &str = r#"
module: office
categories: [live, closed]
types:
  Contact:
    attributes:
      name: { type: string, personal: true, optional: true }
    states:
      KNOWN: { category: live }
    transitions:
      add: { kind: initial, to: KNOWN, optional_inputs: [name] }
  Room:
    attributes:
      name: { type: string }
      host: { reference: Contact, optional: true }
    derived_attributes:
      hosted_by: { type: string, expression: host.name }
    conditions:
      named: { expression: name != "" }
      given_a_name: { expression: inputs.name != "" }
      hosted_by_ana: { expression: host.name == "Ana" }
      host_named: { expression: hosted_by == "Ana" }
      noted: { expression: inputs.note != "" }
      no_namesake: { expression: "count(c in Contact where c.name == inputs.name) == 0" }
      admitted: { expression: inputs.who != "" }
    states:
      OPEN: { category: live }
    transitions:
      add: { kind: initial, to: OPEN, required_inputs: [name], optional_inputs: [host], guards: { given_a_name: deny } }
      tidy: { kind: internal, from: OPEN, guards: { named: deny } }
      greet: { kind: internal, from: OPEN, guards: { hosted_by_ana: deny } }
      welcome: { kind: internal, from: OPEN, guards: { host_named: deny } }
      note: { kind: internal, from: OPEN, inputs: { note: { type: string, personal: true } }, guards: { noted: deny } }
      dedupe: { kind: internal, from: OPEN, inputs: { name: { type: string } }, guards: { no_namesake: deny } }
      admit: { kind: internal, from: OPEN, inputs: { who: { type: identity } }, guards: { admitted: deny } }
  Person:
    attributes:
      login: { type: identity, actor_kind: human, personal: true }
    states:
      ON: { category: live }
    transitions:
      join: { kind: initial, to: ON, required_inputs: [login] }
  Desk:
    attributes:
      label: { type: string }
    invariants:
      labelled: { expression: label != "" }
    states:
      FREE: { category: live }
    transitions:
      add: { kind: initial, to: FREE, required_inputs: [label] }
  Locker:
    attributes:
      owner: { reference: Contact }
    invariants:
      owned_by_someone_named: { expression: owner.name != "" }
    states:
      FREE: { category: live }
    transitions:
      add: { kind: initial, to: FREE, required_inputs: [owner] }
"#;
        let lib = Library::load(&[("office.yaml".into(), OFFICE.into())]).unwrap();
        let personal = |ty: &str, x: &str| transition_reads_personal(&lib, ty, x);
        // the room's own, by its member and by its input
        assert!(!personal("Room", "tidy"));
        assert!(!personal("Room", "add"));
        // the contact's, wherever a guard reaches it
        assert!(personal("Room", "greet"));
        assert!(personal("Room", "welcome"));
        assert!(personal("Room", "dedupe"));
        assert!(!personal("Contact", "add"));
        // an input its own transition marks personal, and one of type
        // identity, which a person's personal login makes personal
        assert!(personal("Room", "note"));
        assert!(personal("Room", "admit"));
        // an invariant of the type, by what it reads
        assert!(!personal("Desk", "add"));
        assert!(personal("Locker", "add"));
    }

    /// A filter's parameters are stated as data, each read as the member it
    /// meets is declared: a day, an instant, an object's id, a text holding
    /// quotes and a state, alone and among a set's elements, so a window of
    /// days answers the same whatever the engine's clock says and no caller
    /// writes a value into the filter's text, where the only way to state a
    /// day was an offset from the engine's clock (issue #24, ADR-0218); a
    /// parameter missing, unused, compared with nothing that says how to
    /// read it, or not of its member's type is refused, and one on a
    /// personal member is personal still.
    #[test]
    fn a_filters_parameters_are_read_as_their_members() {
        const DESK: &str = r#"
module: desk
categories: [live, closed]
types:
  Site:
    attributes:
      code: { type: string }
    states:
      OPEN: { category: live }
    transitions:
      add: { kind: initial, to: OPEN, required_inputs: [code] }
  Booking:
    attributes:
      day: { type: date, indexed: true }
      at: { type: timestamp, indexed: true }
      site: { reference: Site }
      note: { type: string }
      guest: { type: string, optional: true, personal: true }
    states:
      BOOKED: { category: live }
      RELEASED: { category: closed, final: true }
    transitions:
      book: { kind: initial, to: BOOKED, required_inputs: [day, at, site, note], optional_inputs: [guest] }
      release: { kind: external, from: BOOKED, to: RELEASED }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
"#;
        let lib = Library::load(&[("desk.yaml".into(), DESK.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let ask =
            |s: &mut Store, tn: &str, xn: &str, id: Option<&str>, inputs: serde_json::Value| {
                let r = Request {
                    transition: xn.into(),
                    actor: OPERATOR.into(),
                    object_id: id.map(str::to_string),
                    type_: id.is_none().then(|| tn.to_string()),
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
                    crate::verdict::Verdict::Satisfied { object_id, .. } => object_id,
                    other => panic!("{other:?}"),
                }
            };
        let north = ask(&mut s, "Site", "add", None, json!({"code": "N"}));
        let south = ask(&mut s, "Site", "add", None, json!({"code": "S"}));
        let mut made = Vec::new();
        for (day, site, note) in [
            ("2026-11-02", &north, "plain"),
            (
                "2026-11-09",
                &north,
                "a \"quoted\" note \\ with a backslash",
            ),
            ("2026-11-16", &south, "plain"),
        ] {
            made.push(ask(
                &mut s,
                "Booking",
                "book",
                None,
                json!({"day": day, "at": format!("{day}T09:00:00Z"), "site": site, "note": note, "guest": "Ana"}),
            ));
        }
        let ask_page = |s: &Store, filter: &str, params: serde_json::Value| {
            let w = s.snapshot().unwrap();
            let p = params.as_object().cloned();
            query_page(
                &w,
                "Booking",
                Some(filter),
                p.as_ref(),
                &[],
                None,
                None,
                100,
            )
            .unwrap()
        };
        let ids = |page: QueryPage| -> Vec<String> {
            assert!(
                page.refused.is_none(),
                "{:?} {:?}",
                page.refused,
                page.reason
            );
            page.items.into_iter().map(|i| i.id).collect()
        };
        let window = json!({"from": "2026-11-09", "to": "2026-11-22"});
        assert_eq!(
            ids(ask_page(&s, "day >= :from and day <= :to", window.clone())),
            made[1..].to_vec()
        );
        // the same answer whatever the engine's clock says
        s.now += 400 * 86_400;
        assert_eq!(
            ids(ask_page(&s, "day >= :from and day <= :to", window)),
            made[1..].to_vec()
        );
        assert_eq!(
            ids(ask_page(
                &s,
                "at <= :t",
                json!({"t": "2026-11-09T12:00:00Z"})
            )),
            made[..2].to_vec()
        );
        assert_eq!(
            ids(ask_page(&s, "site == :s", json!({"s": south}))),
            made[2..].to_vec()
        );
        assert_eq!(
            ids(ask_page(
                &s,
                "note == :n",
                json!({"n": "a \"quoted\" note \\ with a backslash"})
            )),
            made[1..2].to_vec()
        );
        assert_eq!(
            ids(ask_page(&s, "state == :st", json!({"st": "BOOKED"}))),
            made.clone()
        );
        assert_eq!(
            ids(ask_page(
                &s,
                "day in {:a, :b}",
                json!({"a": "2026-11-02", "b": "2026-11-16"})
            )),
            vec![made[0].clone(), made[2].clone()]
        );
        let refused = |filter: &str, params: serde_json::Value, why: &str| {
            let page = ask_page(&s, filter, params);
            assert_eq!(
                page.refused.as_deref(),
                Some("malformed_filter"),
                "{filter}"
            );
            let reason = page.reason.unwrap_or_default();
            assert!(reason.contains(why), "{filter}: {reason}");
            assert!(!reason.contains("2026"), "a value said: {reason}");
        };
        refused("day >= :from", json!({}), "params does not give");
        refused(
            "day >= :from",
            json!({"from": "2026-11-09", "to": "2026-11-22"}),
            ":to, which the filter never names",
        );
        refused(
            ":a == :b",
            json!({"a": 1, "b": 1}),
            "compared with no member",
        );
        refused(
            "day >= :from",
            json!({"from": "2026-13-45"}),
            "not a value of its member's type",
        );
        refused(
            "count(x in {1} where x > :n) > 0",
            json!({"n": 1}),
            "a parameter is read only",
        );
        let w = s.snapshot().unwrap();
        let page = query_page(
            &w,
            "Booking",
            None,
            json!({"x": 1}).as_object(),
            &[],
            None,
            None,
            10,
        )
        .unwrap();
        assert_eq!(page.refused.as_deref(), Some("malformed_filter"));
        drop(w);
        // a parameter on a personal member is personal still
        assert!(query_reads_personal(
            &s.lib,
            "Booking",
            Some("guest == :g"),
            &[]
        ));
        assert!(!query_reads_personal(
            &s.lib,
            "Booking",
            Some("note == :n"),
            &[]
        ));
    }

    /// One object's rules reading its transitions, its attempts, its
    /// declaration version or an observation collection read each by an
    /// index in a `get`, an `availability` and a `check`, never the whole
    /// log, the whole attempt log or the kind, and give what the whole read
    /// gave; each is classed as growing, so those reads wait in a pool (the
    /// audit's cost B1 and B14, ADR-0205 decision 12).
    #[test]
    fn an_objects_rules_read_its_own_by_index_and_wait_in_a_pool() {
        let text = "\nmodule: grown\ncategories: [live]\ntypes:\n  Job:\n    attributes:\n      title: { type: string, optional: true }\n    derived_attributes:\n      moves: { expression: count(t in this.transitions) }\n      tries: { expression: count(a in this.attempts) }\n      version: { expression: this.declaration_version }\n      read: { expression: count(r in this.readings) }\n    states:\n      ON: { category: live }\n    conditions:\n      few: { expression: count(t in this.transitions) < 2, remedy: dependent }\n    transitions:\n      open: { kind: initial, to: ON, optional_inputs: [title] }\n      note: { kind: internal, from: [ON], guards: { few: deny }, optional_inputs: [title] }\n    observations:\n      readings:\n        kind: Reading\n        description: A reading.\n        attributes:\n          level: { type: int }\n  Plain:\n    states:\n      ON: { category: live }\n    transitions:\n      make: { kind: initial, to: ON }\n";
        let lib = Library::load(&[("grown.yaml".into(), text.into())]).unwrap();
        for what in [ObjectRead::Get, ObjectRead::Availability, ObjectRead::Check] {
            assert!(grows_with_store(&lib, "Job", what));
            assert!(!grows_with_store(&lib, "Plain", what));
            assert!(grows_with_store(&lib, crate::model::PROPOSAL, what));
        }
        let mut s = Store::sandbox(lib, 0).unwrap();
        let ask =
            |s: &mut Store, tn: &str, xn: &str, id: Option<&str>, inputs: serde_json::Value| {
                let r = Request {
                    transition: xn.into(),
                    actor: OPERATOR.into(),
                    object_id: id.map(str::to_string),
                    type_: id.is_none().then(|| tn.to_string()),
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
                request(s, &r).unwrap().unwrap()
            };
        let mut jobs = Vec::new();
        for _ in 0..3 {
            let crate::verdict::Verdict::Satisfied { object_id, .. } =
                ask(&mut s, "Job", "open", None, json!({}))
            else {
                panic!("not opened")
            };
            ask(&mut s, "", "note", Some(&object_id), json!({"title": "a"}));
            // refused, its transitions now two: an attempt on it
            ask(&mut s, "", "note", Some(&object_id), json!({"title": "b"}));
            ask(
                &mut s,
                "Reading",
                "record",
                None,
                json!({"subject": object_id, "level": 3}),
            );
            jobs.push(object_id);
        }
        let job = &jobs[1];
        let before = (
            crate::world::whole_log_reads(),
            crate::world::whole_attempt_reads(),
            crate::world::whole_type_reads("Reading"),
        );
        let w = s.snapshot().unwrap();
        let view = get(&w, job).unwrap().unwrap();
        let derived: BTreeMap<String, Value> = view.derived.iter().cloned().collect();
        assert_eq!(
            (
                &derived["moves"],
                &derived["tries"],
                &derived["version"],
                &derived["read"]
            ),
            (
                &Value::Int(2),
                &Value::Int(1),
                &Value::Int(1),
                &Value::Int(1)
            )
        );
        assert!(availability(&w, OPERATOR, job).unwrap().is_some());
        drop(w);
        let checked = crate::request::check(
            &mut s,
            &Request {
                transition: "note".into(),
                actor: OPERATOR.into(),
                object_id: Some(job.clone()),
                type_: None,
                inputs: BTreeMap::new(),
                occurred_at: None,
                propose: false,
                expires_at: None,
                expected_version: None,
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            },
        )
        .unwrap();
        assert!(checked.is_ok());
        assert_eq!(
            (
                crate::world::whole_log_reads(),
                crate::world::whole_attempt_reads(),
                crate::world::whole_type_reads("Reading"),
            ),
            before
        );
    }

    /// A proposal's read carries the verdict its last refused approval left
    /// it with, read from its approvals' attempts by their index, never the
    /// whole attempt log (the audit's cost B14).
    #[test]
    fn a_proposals_last_verdict_reads_its_attempts_by_index() {
        let text = "\nmodule: memo\ncategories: [live]\ntypes:\n  Doc:\n    attributes:\n      n: { type: int, optional: true }\n    states:\n      ON: { category: live }\n    conditions:\n      small: { expression: inputs.n < 3, remedy: self_serviceable }\n    transitions:\n      make: { kind: initial, to: ON }\n      grow: { kind: internal, from: [ON], proposable: true, required_inputs: [n], guards: { small: deny } }\n";
        let lib = Library::load(&[("memo.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let ask = |s: &mut Store,
                   xn: &str,
                   tn: Option<&str>,
                   id: Option<&str>,
                   inputs: serde_json::Value,
                   propose: bool| {
            let r = Request {
                transition: xn.into(),
                actor: OPERATOR.into(),
                object_id: id.map(str::to_string),
                type_: tn.map(str::to_string),
                inputs: inputs
                    .as_object()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
                occurred_at: None,
                propose,
                expires_at: None,
                expected_version: None,
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            };
            request(s, &r).unwrap().unwrap()
        };
        let crate::verdict::Verdict::Satisfied { object_id: doc, .. } =
            ask(&mut s, "make", Some("Doc"), None, json!({}), false)
        else {
            panic!("no doc")
        };
        let crate::verdict::Verdict::Satisfied {
            object_id: proposal,
            ..
        } = ask(&mut s, "grow", None, Some(&doc), json!({"n": 5}), true)
        else {
            panic!("not proposed")
        };
        let refused = ask(&mut s, "approve", None, Some(&proposal), json!({}), false);
        assert!(
            !matches!(refused, crate::verdict::Verdict::Satisfied { .. }),
            "{refused:?}"
        );
        let before = crate::world::whole_attempt_reads();
        let w = s.snapshot().unwrap();
        let view = get(&w, &proposal).unwrap().unwrap();
        let derived: BTreeMap<String, Value> = view.derived.iter().cloned().collect();
        assert!(!derived["last_verdict"].is_absent(), "{derived:?}");
        assert_eq!(crate::world::whole_attempt_reads(), before);
    }

    /// A page of `available` reads from its cursor a chunk at a time, to the
    /// page and one more, where each page read every object in the
    /// transition's states (twelfth production review); paged, it gives
    /// what it gives whole, in order.
    #[test]
    fn a_page_of_available_reads_to_its_page() {
        let text = "\nmodule: work\ncategories: [live, closed]\ntypes:\n  Job:\n    attributes:\n      n: { type: int }\n    states:\n      OPEN: { category: live }\n      DONE: { category: closed, final: true }\n    transitions:\n      open: { kind: initial, to: OPEN, required_inputs: [n] }\n      finish: { kind: external, from: [OPEN], to: DONE }\n";
        let lib = Library::load(&[("work.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        for n in 0..600 {
            let r = Request {
                transition: "open".into(),
                actor: OPERATOR.into(),
                object_id: None,
                type_: Some("Job".into()),
                inputs: BTreeMap::from([("n".to_string(), json!(n))]),
                occurred_at: None,
                propose: false,
                expires_at: None,
                expected_version: None,
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            };
            assert!(request(&mut s, &r).unwrap().is_ok());
        }
        let w = s.snapshot().unwrap();
        let page = available_page(&w, OPERATOR, "Job", "finish", None, 2).unwrap();
        assert_eq!(page.items.len(), 2);
        assert!(page.cursor.is_some());
        assert!(
            w.held_count() < 600,
            "a page of two held {}",
            w.held_count()
        );
        drop(w);
        let w = s.snapshot().unwrap();
        let whole: Vec<String> = available(&w, OPERATOR, "Job", "finish")
            .unwrap()
            .items
            .into_iter()
            .map(|i| i.id)
            .collect();
        drop(w);
        assert_eq!(whole.len(), 600);
        let mut paged = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let w = s.snapshot().unwrap();
            let page =
                available_page(&w, OPERATOR, "Job", "finish", cursor.as_deref(), 50).unwrap();
            paged.extend(page.items.into_iter().map(|i| i.id));
            match page.cursor {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        assert_eq!(paged, whole);
    }

    /// A page that reads its objects' records, ordered by when each entered
    /// its state or offering a transition, reads the records of the objects
    /// it holds, never every object of the type: a worklist of 50 read 110k
    /// records, 0.8 s a page on six years whatever its limit (thirteenth
    /// production review N3).
    #[test]
    fn a_page_reads_the_records_of_its_objects_alone() {
        let text = "\nmodule: work\ncategories: [live, closed]\ntypes:\n  Job:\n    attributes:\n      n: { type: int }\n    states:\n      OPEN: { category: live }\n      DONE: { category: closed, final: true }\n    transitions:\n      open: { kind: initial, to: OPEN, required_inputs: [n] }\n      finish: { kind: external, from: [OPEN], to: DONE }\n";
        let lib = Library::load(&[("work.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let ask = |s: &mut Store, xn: &str, id: Option<String>, inputs: serde_json::Value| {
            let r = Request {
                transition: xn.into(),
                actor: OPERATOR.into(),
                type_: id.is_none().then(|| "Job".to_string()),
                object_id: id,
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
                crate::verdict::Verdict::Satisfied { object_id, .. } => object_id,
                other => panic!("{other:?}"),
            }
        };
        for n in 0..1_000 {
            let id = ask(&mut s, "open", None, json!({"n": n}));
            if n >= 10 {
                ask(&mut s, "finish", Some(id), json!({}));
            }
        }
        let w = s.snapshot().unwrap();
        let before = crate::world::records_read();
        let order = ["entered_at(state)".to_string()];
        let page = query_page(
            &w,
            "Job",
            Some("state == OPEN"),
            None,
            &order,
            None,
            None,
            5,
        )
        .unwrap();
        assert_eq!(page.items.len(), 5);
        let read = crate::world::records_read() - before;
        assert!(read <= 20, "a page of five read {read} records");
        drop(w);
        let w = s.snapshot().unwrap();
        let before = crate::world::records_read();
        let open = available_page(&w, OPERATOR, "Job", "finish", None, 100).unwrap();
        assert_eq!(open.items.len(), 10);
        let read = crate::world::records_read() - before;
        assert!(read <= 20, "a page of available read {read} records");
    }

    /// A type whose rules read a metric, a type scan or its referrers, itself
    /// or through a derived attribute read by name from another type, grows
    /// with the store; one comparing its state does not (thirteenth
    /// production review C).
    #[test]
    fn a_types_reads_grow_where_its_rules_read_the_store() {
        let text = "\nmodule: g\ncategories: [live]\ntypes:\n  Scan:\n    summary: [state, peers]\n    derived_attributes:\n      peers: { expression: count(s in Scan) }\n    states:\n      ON: { category: live }\n    transitions:\n      make: { kind: initial, to: ON }\n  Refs:\n    derived_attributes:\n      pointed: { expression: count(r in referrers) }\n    states:\n      ON: { category: live }\n    transitions:\n      make: { kind: initial, to: ON }\n  Metered:\n    states:\n      ON: { category: live }\n    conditions:\n      calm: { expression: metric(Metered.open_work) < 5, remedy: dependent }\n    transitions:\n      make: { kind: initial, to: ON }\n  Reader:\n    attributes:\n      scan: { reference: Scan, optional: true }\n    states:\n      ON: { category: live }\n    invariants:\n      few: { expression: scan is null or scan.peers < 9 }\n    transitions:\n      make: { kind: initial, to: ON }\n  Plain:\n    states:\n      ON: { category: live }\n    conditions:\n      on: { expression: state == Plain.ON, remedy: dependent }\n    transitions:\n      make: { kind: initial, to: ON }\n";
        let lib = Library::load(&[("g.yaml".into(), text.into())]).unwrap();
        for tn in ["Scan", "Refs", "Metered", "Reader"] {
            assert!(grows_with_store(&lib, tn, ObjectRead::Check), "{tn}");
        }
        assert!(!grows_with_store(&lib, "Plain", ObjectRead::Check));
        // a get computes derived attributes alone, and an availability its
        // conditions too, never an invariant (fourteenth production review)
        assert!(grows_with_store(&lib, "Scan", ObjectRead::Get));
        assert!(!grows_with_store(&lib, "Metered", ObjectRead::Get));
        assert!(grows_with_store(&lib, "Metered", ObjectRead::Availability));
        assert!(!grows_with_store(&lib, "Reader", ObjectRead::Availability));
        assert!(grows_with_store(&lib, "Reader", ObjectRead::Check));
        // a query computing a field that grows waits among the analytics,
        // asked for or in its type's summary, and one asking for none does
        // not (fourteenth production review N7)
        let (peers, state) = (["peers".to_string()], ["state".to_string()]);
        assert!(query_scans(&lib, "Scan", None, &[], Some(&peers)));
        assert!(query_scans(&lib, "Scan", None, &[], None));
        assert!(!query_scans(&lib, "Scan", None, &[], Some(&state)));
        assert!(!query_scans(&lib, "Plain", None, &[], None));
    }

    /// A query whose filter aggregates over a collection for each object,
    /// itself or through a derived attribute, waits among the analytics
    /// however an index narrows its page, where eight such left a plain
    /// worklist waiting 16 s (twelfth production review); one the index
    /// narrows and that aggregates nothing waits among the worklists.
    #[test]
    fn a_query_aggregating_for_each_object_waits_among_the_analytics() {
        let text = "\nmodule: crews\ncategories: [live, closed]\ntypes:\n  Crew:\n    attributes:\n      jobs: { reference: \"Job[]\", opposite: crew }\n    derived_attributes:\n      busy: { expression: count(j in jobs) > 3 }\n    states:\n      ON: { category: live }\n    transitions:\n      form: { kind: initial, to: ON }\n  Job:\n    attributes:\n      crew: { reference: Crew, optional: true, opposite: jobs }\n      hours: { type: int, optional: true }\n    states:\n      OPEN: { category: live }\n      DONE: { category: closed, final: true }\n    transitions:\n      open: { kind: initial, to: OPEN, optional_inputs: [crew] }\n";
        let lib = Library::load(&[("crews.yaml".into(), text.into())]).unwrap();
        let scans = |filter: &str| query_scans(&lib, "Crew", Some(filter), &[], None);
        assert!(!scans("state == ON"));
        assert!(scans(
            "state == ON and count(j in jobs where j.hours > 2) > 1"
        ));
        assert!(scans("state == ON and busy"));
        assert!(scans("busy"));
    }

    const JOBS: &str = r#"
module: jobs
categories: [inbound, live, closed]
types:
  Job:
    attributes:
      title: { type: string }
      hours: { type: int, optional: true }
    derived_attributes:
      long: { expression: hours is not null and hours > 8 }
    states:
      OPEN: { category: inbound }
      DONE: { category: closed, final: true }
    conditions:
      estimated: { expression: hours is not null, remedy: self_serviceable }
      small: { expression: inputs.hours is null or inputs.hours < 10, remedy: self_serviceable }
      short: { expression: not long, remedy: dependent }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [title] }
      estimate: { kind: internal, from: [OPEN], required_inputs: [hours], guards: { small: deny } }
      finish: { kind: external, from: [OPEN], to: DONE, guards: { estimated: deny, short: warn } }
      reopen: { kind: external, from: [DONE], to: OPEN }
"#;

    /// A filter reading a reference as a string, or computing with one, is
    /// refused malformed, where it held for nothing or faulted (D556); one
    /// reading the reference's `.id`, a member through it, or comparing it
    /// with a reference is not.
    #[test]
    fn a_filter_misreading_a_reference_is_malformed() {
        const SHOP: &str = r#"
module: shop
categories: [live, closed]
types:
  Customer:
    attributes:
      name: { type: string }
    states: { ACTIVE: { category: live } }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [name] }
  Order:
    attributes:
      customer: { reference: Customer }
    states: { OPEN: { category: live } }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [customer] }
"#;
        let lib = Library::load(&[("shop.yaml".into(), SHOP.into())]).unwrap();
        let s = Store::sandbox(lib, 0).unwrap();
        let w = s.snapshot().unwrap();
        let refused = |f: &str| query(&w, "Order", Some(f), &[], None).unwrap().refused;
        let malformed = Some("malformed_filter".to_string());
        assert_eq!(refused(r#"customer == "customer-1""#), malformed);
        assert_eq!(refused(r#""customer-1" != customer"#), malformed);
        assert_eq!(refused("customer == customer-1"), malformed);
        assert_eq!(refused(r#"customer.id == "customer-1""#), None);
        assert_eq!(refused(r#"customer.name == "Ana""#), None);
        assert_eq!(refused("customer == customer"), None);
    }

    /// A filter no stored object can answer, an evaluator's, the request's
    /// `actor`, `inputs` or `this_event`, or a division by a literal zero, is
    /// refused malformed where it was answered empty, and one with a literal
    /// past its type's range where it was not run (D580, fourth production
    /// review A-10).
    /// A personal member is one marked so, and, where an actor type marks
    /// its identity personal, every member of type `identity`, which a
    /// read selecting by one then opts into (ADR-0206 decision 1).
    #[test]
    fn an_identity_is_personal_where_an_actors_is() {
        for (personal, refused) in [(true, true), (false, false)] {
            let text = format!(
                "\nmodule: crm\ncategories: [live]\ntypes:\n  Person:\n    attributes:\n      login: {{ type: identity, actor_kind: human, personal: {personal}, optional: true }}\n    states:\n      ON: {{ category: live }}\n    transitions:\n      join: {{ kind: initial, to: ON, required_inputs: [login] }}\n  Memo:\n    attributes:\n      author: {{ type: identity, optional: true }}\n    states:\n      ON: {{ category: live }}\n    transitions:\n      write: {{ kind: initial, to: ON }}\n"
            );
            let lib = Library::load(&[("crm.yaml".into(), text)]).unwrap();
            assert_eq!(
                query_reads_personal(&lib, "Memo", Some("author >= \"a\""), &[]),
                refused,
                "personal: {personal}"
            );
        }
    }

    /// A filter refused for a value of the wrong kind says the kind and
    /// never the value an object holds, where `month(name)` quoted the name
    /// (the audit's personal data R14, `wire.md`).
    #[test]
    fn a_refused_filter_names_no_stored_value() {
        let text = "\nmodule: crm\ncategories: [live]\ntypes:\n  Person:\n    attributes:\n      name: { type: string }\n    states:\n      ON: { category: live }\n    transitions:\n      join: { kind: initial, to: ON, required_inputs: [name] }\n";
        let lib = Library::load(&[("crm.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let r = crate::request::Request {
            transition: "join".into(),
            actor: crate::model::OPERATOR.into(),
            object_id: None,
            type_: Some("Person".into()),
            inputs: [("name".to_string(), serde_json::json!("Alice Liddell"))]
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
        crate::request::request(&mut s, &r).unwrap().unwrap();
        let w = s.snapshot().unwrap();
        let p = query(&w, "Person", Some(r#"month(name) == "x""#), &[], None).unwrap();
        assert_eq!(p.refused.as_deref(), Some("malformed_filter"));
        let reason = p.reason.unwrap_or_default();
        assert!(!reason.contains("Alice"), "{reason}");
    }

    #[test]
    fn a_filter_no_object_can_answer_is_malformed() {
        const LEDGER: &str = r#"
module: ledger
categories: [live]
evaluators:
  bank:
    description: The bank.
    functions:
      known:
        description: Whether the bank knows the payee.
        arguments:
          payee: { type: string }
types:
  Payment:
    attributes:
      payee: { type: string }
      amount: { type: money(USD) }
      count: { type: int }
    states: { DUE: { category: live } }
    transitions:
      add: { kind: initial, to: DUE, required_inputs: [payee, amount, count] }
  Refund:
    attributes:
      amount: { type: money(USD) }
    states: { DUE: { category: live } }
    transitions:
      add: { kind: initial, to: DUE, required_inputs: [amount] }
"#;
        let lib = Library::load(&[("ledger.yaml".into(), LEDGER.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let r = Request {
            transition: "add".into(),
            actor: OPERATOR.into(),
            object_id: None,
            type_: Some("Payment".into()),
            inputs: [
                ("payee".to_string(), json!("Ana")),
                ("amount".to_string(), json!("USD 5.00")),
                ("count".to_string(), json!(2)),
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
        request(&mut s, &r).unwrap().unwrap();
        let w = s.snapshot().unwrap();
        let refused = |f: &str| query(&w, "Payment", Some(f), &[], None).unwrap().refused;
        let malformed = Some("malformed_filter".to_string());
        let huge = format!("amount > USD {}", "9".repeat(5000));
        for f in [
            "bank.known(payee)",
            r#"actor.kind == "human""#,
            "inputs.count > 1",
            "count / 0 > 1",
            "count / 0.00 > 1",
            huge.as_str(),
            "created_at < now + 999999999999999 weeks",
            "count < 9223372036854775808",
            "created_at < now + 9223372036854775808 s",
        ] {
            assert_eq!(refused(f), malformed, "{}", &f[..f.len().min(60)]);
        }
        // a value past its range only once computed on an object, and a
        // literal past it on a type with no object, which no object's
        // evaluation reaches
        assert_eq!(
            refused("created_at + 9223372036854775807 seconds > now"),
            malformed
        );
        let empty = query(&w, "Refund", Some(huge.as_str()), &[], None).unwrap();
        assert_eq!(empty.refused, malformed);
        // aggregates nested two deep and a set of 1,000 members are taken,
        // three deep and 1,001 refused (fifth production review FILTER-COST)
        let nested = |depth: usize| {
            let mut f = "p0.count > 0".to_string();
            for i in (1..=depth).rev() {
                f = format!("count(p{i} in Payment where {f}) > 0")
                    .replace("p0.", &format!("p{i}."));
            }
            f
        };
        assert_eq!(refused(&nested(2)), None, "{}", nested(2));
        assert_eq!(refused(&nested(3)), malformed, "{}", nested(3));
        let set = |n: usize| {
            format!(
                "count in {{{}}}",
                (0..n).map(|i| i.to_string()).collect::<Vec<_>>().join(", ")
            )
        };
        assert_eq!(refused(&set(FILTER_MEMBERS)), None);
        assert_eq!(refused(&set(FILTER_MEMBERS + 1)), malformed);
        // a metric called per object, and past 256 terms, refused; an `or`
        // of 64 comparisons, 255 terms, taken (ADR-0192 decision 3)
        assert_eq!(refused("metric(Payment.work_in_progress) > 0"), malformed);
        let chain = |n: usize| {
            (0..n)
                .map(|i| format!("count == {i}"))
                .collect::<Vec<_>>()
                .join(" or ")
        };
        assert_eq!(refused(&chain(64)), None);
        assert_eq!(refused(&chain(65)), malformed);
        // a division by a money zero, and a set within a set, refused,
        // where each answered an empty page (ADR-0192 decision 7)
        assert_eq!(refused("amount / USD 0.00 > 0"), malformed);
        assert_eq!(refused("count in {{1}}"), malformed);
        assert_eq!(refused("count / 2 > 0"), None);
        assert_eq!(refused(r#"payee == "Ana""#), None);
        assert_eq!(
            query(&w, "Payment", Some("count / 2 > 0"), &[], None)
                .unwrap()
                .items
                .len(),
            1
        );
    }

    fn setup() -> (Store, String) {
        let lib = Library::load(&[("jobs.yaml".into(), JOBS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let r = Request {
            transition: "open".into(),
            actor: OPERATOR.into(),
            object_id: None,
            type_: Some("Job".into()),
            inputs: [("title".to_string(), json!("Fix"))].into_iter().collect(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
        };
        let job = match request(&mut s, &r).unwrap().unwrap() {
            Verdict::Satisfied { object_id, .. } => object_id,
            other => panic!("{other:?}"),
        };
        (s, job)
    }

    fn offered(s: &Store, actor: &str, id: &str) -> BTreeMap<String, Offer> {
        let w = s.snapshot().unwrap();
        availability(&w, actor, id)
            .unwrap()
            .unwrap()
            .into_iter()
            .map(|o| (o.transition.clone(), o))
            .collect()
    }

    /// A query (`DESIGN.md` §10, ADR-0127): the objects its filter holds
    /// for, one unknown for an object selecting it not; ordered by each key,
    /// `desc` reversing one, an absent value last, creation order breaking
    /// ties; each with the fields asked for, or its state; and a refusal a
    /// value.
    #[test]
    fn a_query_filters_orders_and_refuses_as_a_value() {
        let (mut s, first) = setup();
        let ask = |s: &mut Store, xn: &str, object: Option<&str>, inputs: serde_json::Value| {
            let r = Request {
                transition: xn.into(),
                actor: OPERATOR.into(),
                object_id: object.map(str::to_string),
                type_: object.is_none().then(|| "Job".to_string()),
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
        // first: no hours; then three more, an hour apart
        let mut jobs = vec![first];
        for hours in [9, 3, 9] {
            s.now += 3600;
            let j = ask(&mut s, "open", None, json!({"title": "Job"}));
            ask(&mut s, "estimate", Some(&j), json!({"hours": hours}));
            jobs.push(j);
        }
        // the first estimated is finished and reopened, entering OPEN after
        // the one created after it
        s.now += 3600;
        ask(&mut s, "finish", Some(jobs[1].as_str()), json!({}));
        ask(&mut s, "reopen", Some(jobs[1].as_str()), json!({}));
        s.now += 3600;
        ask(&mut s, "finish", Some(jobs[3].as_str()), json!({}));
        let w = s.snapshot().unwrap();
        let ids = |p: QueryPage| -> Vec<String> {
            assert_eq!(p.refused, None);
            p.items.into_iter().map(|i| i.id).collect()
        };
        let order = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect::<Vec<_>>();
        let q = |filter: Option<&str>, keys: &[&str]| {
            query(&w, "Job", filter, &order(keys), None).unwrap()
        };
        // by state's category, newest first
        assert_eq!(
            ids(q(Some("state.category != closed"), &["created_at desc"])),
            [jobs[2].as_str(), jobs[1].as_str(), jobs[0].as_str()]
        );
        // by a derived value; one whose hours are unknown is not selected
        assert_eq!(
            ids(q(Some("long"), &[])),
            [jobs[1].as_str(), jobs[3].as_str()]
        );
        assert_eq!(
            ids(q(Some("hours > 5"), &[])),
            [jobs[1].as_str(), jobs[3].as_str()]
        );
        // by an attribute, an absent value last whichever way, ties by creation
        assert_eq!(
            ids(q(None, &["hours"])),
            [
                jobs[2].as_str(),
                jobs[1].as_str(),
                jobs[3].as_str(),
                jobs[0].as_str()
            ]
        );
        assert_eq!(
            ids(q(None, &["hours desc"])),
            [
                jobs[1].as_str(),
                jobs[3].as_str(),
                jobs[2].as_str(),
                jobs[0].as_str()
            ]
        );
        // by when the current state was entered, not when it was created:
        // the reopened job last
        assert_eq!(
            ids(q(Some("state.category != closed"), &["entered_at(state)"])),
            [jobs[0].as_str(), jobs[2].as_str(), jobs[1].as_str()]
        );
        // a filter requiring a state reads the objects in it alone, and
        // answers as the same filter that requires none (ADR-0162 step 3)
        for (pushed, whole) in [
            ("state == OPEN", "state == OPEN or false"),
            (
                "state == Job.OPEN and hours > 5",
                "(state == Job.OPEN and hours > 5) or false",
            ),
            (
                "hours < 5 and OPEN == state",
                "(hours < 5 and OPEN == state) or false",
            ),
            (
                "state == OPEN and state == DONE",
                "(state == OPEN and state == DONE) or false",
            ),
        ] {
            assert_eq!(
                ids(q(Some(pushed), &["created_at desc"])),
                ids(q(Some(whole), &["created_at desc"])),
                "{pushed}"
            );
        }
        assert_eq!(
            ids(q(Some("state == OPEN"), &["created_at desc"])),
            [jobs[2].as_str(), jobs[1].as_str(), jobs[0].as_str()]
        );
        assert_eq!(
            ids(q(Some("state == Job.OPEN and hours > 5"), &[])),
            [jobs[1].as_str()]
        );
        // and reads only the jobs in the state, where the whole filter reads all
        let fresh = s.snapshot().unwrap();
        query(&fresh, "Job", Some("state == OPEN"), &[], None).unwrap();
        assert_eq!(fresh.held().filter(|o| o.type_ == "Job").count(), 3);
        let fresh = s.snapshot().unwrap();
        query(&fresh, "Job", Some("state == OPEN or false"), &[], None).unwrap();
        assert_eq!(fresh.held().filter(|o| o.type_ == "Job").count(), 4);
        // a rule's scan by state reads as a query does, and counts as the
        // same scan that requires no state
        let counted = |filter: &str| {
            let fresh = s.snapshot().unwrap();
            let t = fresh.lib.type_("Job");
            let n = crate::eval::Evaluator::new(&fresh, t, None)
                .eval(&crate::expr::parse(&format!("count(j in Job where {filter})")).unwrap())
                .unwrap();
            (n, fresh.held().filter(|o| o.type_ == "Job").count())
        };
        // an open job with no hours leaves the count unknown, as the scan of
        // every job leaves it, the jobs in other states failing outright
        let (pushed, read) = counted("j.state == OPEN and j.hours > 5");
        let (whole, all) = counted("(j.state == OPEN and j.hours > 5) or false");
        assert_eq!((pushed, read, all), (whole, 3, 4));
        assert_eq!(counted("j.state == Job.DONE"), (Value::Int(1), 1));
        // and by a stored attribute's value, an object without one read as
        // well, since it compares as unknown and leaves the count undecided
        let narrowed = |filter: &str, reads: usize| {
            let (pushed, read) = counted(filter);
            let (whole, all) = counted(&format!("({filter}) or false"));
            assert_eq!((&pushed, read, all), (&whole, reads, 4), "{filter}");
        };
        narrowed("j.title == \"Job\"", 3);
        narrowed("j.hours == 9", 3);
        // a decimal compared with an integer column is not compared by the
        // store, which holds the two in different units
        narrowed("j.hours == 9.0", 4);
        // a type that declares no summary gives each object's state, and the
        // fields asked for are given
        let p = query(&w, "Job", Some("hours < 5"), &[], None).unwrap();
        assert_eq!(
            p.items[0].fields,
            vec![(
                "state".to_string(),
                Value::State {
                    type_: "Job".into(),
                    name: "OPEN".into()
                }
            )]
        );
        let p = query(
            &w,
            "Job",
            Some("hours < 5"),
            &[],
            Some(&order(&["title", "hours", "long", "created_by_kind"])),
        )
        .unwrap();
        assert_eq!(
            p.items[0].fields,
            vec![
                ("title".to_string(), Value::Text("Job".into())),
                ("hours".to_string(), Value::Int(3)),
                ("long".to_string(), Value::Bool(false)),
                ("created_by_kind".to_string(), Value::Text("human".into()))
            ]
        );
        // the entry time of a tracked member's current interval, as a field
        // as an order key (ADR-0161): each item's own
        let p = query(
            &w,
            "Job",
            Some("state.category != closed"),
            &order(&["entered_at(state)"]),
            Some(&order(&["entered_at(state)"])),
        )
        .unwrap();
        assert_eq!((p.refused.as_deref(), p.items.len()), (None, 3));
        for item in &p.items {
            let entered = w
                .whole(&item.id)
                .unwrap()
                .unwrap()
                .held_parts()
                .unwrap()
                .spans["state"]
                .last()
                .unwrap()
                .entered_at;
            assert_eq!(
                item.fields,
                vec![("entered_at(state)".to_string(), Value::Timestamp(entered))]
            );
        }
        assert_eq!(
            query(&w, "Job", None, &[], Some(&order(&["entered_at(title)"])))
                .unwrap()
                .refused
                .as_deref(),
            Some("unknown_field")
        );
        // each refusal a value
        let refused = |p: QueryPage| p.refused;
        assert_eq!(
            refused(query(&w, "Nonesuch", None, &[], None).unwrap()).as_deref(),
            Some("unknown_type")
        );
        assert_eq!(
            refused(q(Some("hours >"), &[])).as_deref(),
            Some("malformed_filter")
        );
        assert_eq!(
            refused(q(None, &["nonesuch"])).as_deref(),
            Some("unknown_order_key")
        );
        assert_eq!(
            refused(q(None, &["entered_at(title)"])).as_deref(),
            Some("unknown_order_key")
        );
        assert_eq!(
            refused(query(&w, "Job", None, &[], Some(&order(&["nonesuch"]))).unwrap()).as_deref(),
            Some("unknown_field")
        );
    }

    #[test]
    fn an_offer_decides_what_reads_no_input() {
        let (s, job) = setup();
        let o = offered(&s, OPERATOR, &job);
        assert!(!o.contains_key("open"));
        // a guard on an input waits for it
        assert_eq!(o["estimate"].availability, Availability::AvailableWithInput);
        // a guard reading the object is decided, its refusal the verdict
        assert_eq!(o["finish"].availability, Availability::Unavailable);
        assert_eq!(
            o["finish"].verdict,
            Some(Verdict::Unsatisfied {
                clause: "estimated".into(),
                remedy: Some(Remedy::SelfServiceable)
            })
        );
        // a state it is not taken from
        assert_eq!(
            o["reopen"].verdict,
            Some(Verdict::Unavailable {
                state: "OPEN".into()
            })
        );
        // an actor the store does not hold is refused every offer
        let nobody = offered(&s, "nobody", &job);
        assert!(nobody.values().all(|x| matches!(&x.verdict, Some(Verdict::Unsatisfied { clause, .. }) if clause == "actor_known")));
    }

    /// A derived sum over no elements is the zero of its expression's type,
    /// a duration here, read by `get` and as a query's field alike, as the
    /// reference runner's service encodes it (`declaration-syntax.md` §8.3).
    #[test]
    fn a_derived_sum_over_nothing_is_its_types_zero() {
        let text = JOBS.replace(
            "      long: { expression: hours is not null and hours > 8 }",
            "      long: { expression: hours is not null and hours > 8 }\n      \
             done_for: { expression: \"sum(i in this.intervals where i.state == DONE: i.duration)\" }",
        );
        let lib = Library::load(&[("jobs.yaml".into(), text)]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let r = Request {
            transition: "open".into(),
            actor: OPERATOR.into(),
            object_id: None,
            type_: Some("Job".into()),
            inputs: [("title".to_string(), json!("Fix"))].into_iter().collect(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
        };
        let Verdict::Satisfied { object_id: job, .. } = request(&mut s, &r).unwrap().unwrap()
        else {
            panic!("the job is not opened")
        };
        let w = s.snapshot().unwrap();
        let v = get(&w, &job).unwrap().unwrap();
        let done_for = v
            .derived
            .iter()
            .find(|(n, _)| n == "done_for")
            .map(|(_, x)| x);
        assert_eq!(done_for, Some(&Value::Duration(0)));
        let page = query(&w, "Job", None, &[], Some(&["done_for".to_string()])).unwrap();
        assert_eq!(
            page.items[0].fields,
            vec![("done_for".to_string(), Value::Duration(0))]
        );
    }

    #[test]
    fn a_get_evaluates_what_the_type_derives() {
        let (s, job) = setup();
        let w = s.snapshot().unwrap();
        let v = get(&w, &job).unwrap().unwrap();
        assert_eq!(v.derived, vec![("long".to_string(), Value::Bool(false))]);
        assert!(v.open);
        assert!(get(&w, "job-99").unwrap().is_none());
    }

    /// `exceptions` and `available` (`DESIGN.md` §10): an object an
    /// assertion put in its state is an exception until an ordinary
    /// transition moves it, as is one holding an admitted violation, over a
    /// family; `available` lists the objects a transition is offered to, a
    /// warning blocking none and an evaluator's guard left unevaluated and
    /// named; and what neither can read is refused, a value.
    #[test]
    fn exceptions_and_available_page_objects_and_refuse_as_values() {
        const SHOP: &str = r#"
module: shop
categories: [live, closed]
evaluators:
  vault:
    functions:
      cleared: { arguments: { n: { type: int } } }
types:
  Task:
    abstract: true
    attributes:
      n: { type: int }
  Job:
    extends: Task
    states:
      OPEN: { category: live }
      DONE: { category: closed, final: false }
    conditions:
      small: { expression: n < 10, remedy: self_serviceable }
      positive: { expression: n > 0, remedy: self_serviceable }
      cleared: { expression: vault.cleared(n), remedy: dependent }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [n] }
      finish: { kind: external, from: [OPEN], to: DONE, guards: { small: deny, positive: warn } }
      clear: { kind: external, from: [OPEN], to: DONE, guards: { cleared: deny } }
      reopen: { kind: external, from: [DONE], to: OPEN }
      archive: { kind: internal, from: [DONE], only_via: [finish] }
      force: { kind: assertion, to: [OPEN, DONE], inputs: { reason: { type: string } } }
"#;
        let lib = Library::load(&[("shop.yaml".into(), SHOP.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let ask = |s: &mut Store, xn: &str, object: Option<&str>, inputs: serde_json::Value| {
            let r = Request {
                transition: xn.into(),
                actor: OPERATOR.into(),
                object_id: object.map(str::to_string),
                type_: object.is_none().then(|| "Job".to_string()),
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
        let one = ask(&mut s, "open", None, json!({"n": 1}));
        let big = ask(&mut s, "open", None, json!({"n": 50}));
        let below = ask(&mut s, "open", None, json!({"n": -1}));
        let ids = |p: &QueryPage| -> Vec<String> {
            assert_eq!(p.refused, None);
            p.items.iter().map(|i| i.id.clone()).collect()
        };
        let avail = |s: &Store, xn: &str| {
            let w = s.snapshot().unwrap();
            available(&w, OPERATOR, "Job", xn).unwrap()
        };
        let excepted = |s: &Store, t: &str| exceptions(&s.snapshot().unwrap(), t).unwrap();
        // the deny guard blocks fifty; the warning blocks none
        assert_eq!(ids(&avail(&s, "finish")), [one.clone(), below.clone()]);
        assert_eq!(ids(&excepted(&s, "Job")), Vec::<String>::new());
        // an assertion's state is an exception, over the family, and the
        // object is no longer offered a transition from OPEN
        ask(
            &mut s,
            "force",
            Some(&one),
            json!({"to": "DONE", "reason": "boxed"}),
        );
        let page = excepted(&s, "Task");
        assert_eq!(ids(&page), [one.as_str()]);
        assert_eq!(
            page.items[0].fields,
            [(
                "state".to_string(),
                Value::State {
                    type_: "Job".into(),
                    name: "DONE".into()
                }
            )]
        );
        assert_eq!(ids(&avail(&s, "finish")), [below.as_str()]);
        // an ordinary transition returns it to the machine
        ask(&mut s, "reopen", Some(&one), json!({}));
        assert_eq!(ids(&excepted(&s, "Job")), Vec::<String>::new());
        assert_eq!(ids(&avail(&s, "finish")), [one.clone(), below.clone()]);
        // an admitted violation is an exception
        let mut w = s.snapshot().unwrap();
        w.get_mut(&big)
            .unwrap()
            .unwrap()
            .parts_mut()
            .unwrap()
            .admissions
            .insert("bounded".into());
        assert_eq!(ids(&exceptions(&w, "Job").unwrap()), [big.as_str()]);
        drop(w);
        // a standing admission the store holds is one, a discharged one not;
        // the read holds only what it lists, read by the store's indexes and
        // never the type whole
        let conn = s.connection();
        let at: i64 = conn
            .query_row("SELECT MAX(position) FROM of_event", [], |r| r.get(0))
            .unwrap();
        conn.execute(
            "INSERT INTO of_admission (object_id, invariant, position, reason) \
             VALUES (?1, 'bounded', ?2, 'test')",
            crate::db::params![big, at],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO of_admission (object_id, invariant, position, reason, discharged_position) \
             VALUES (?1, 'bounded', ?2, 'test', ?2)",
            crate::db::params![below, at],
        )
        .unwrap();
        if !crate::db::sqlite_only(s.backend(), "its index plans") {
            let plan: String = conn
                .query_row(
                    "EXPLAIN QUERY PLAN SELECT id FROM t_job WHERE state_source = 'asserted'",
                    [],
                    |r| r.get(3),
                )
                .unwrap();
            assert!(plan.contains("t_job_asserted_ix"), "{plan}");
        }
        let w = s.snapshot().unwrap();
        assert_eq!(ids(&exceptions(&w, "Task").unwrap()), [big.as_str()]);
        assert_eq!(w.held().count(), 1);
        // and one the operation discharged is not
        let mut w = s.snapshot().unwrap();
        w.get_mut(&big)
            .unwrap()
            .unwrap()
            .parts_mut()
            .unwrap()
            .admissions
            .clear();
        assert_eq!(ids(&exceptions(&w, "Job").unwrap()), Vec::<String>::new());
        drop(w);
        // an evaluator's guard is not asked, and is named on every object
        let page = avail(&s, "clear");
        assert_eq!(ids(&page), [one.clone(), big.clone(), below.clone()]);
        assert!(page.items.iter().all(|i| i.fields.contains(&(
            "unevaluated".to_string(),
            Value::Set(vec![Value::Text("cleared".into())])
        ))));
        // what it cannot read is refused, a value
        let w = s.snapshot().unwrap();
        for (t, xn, why) in [
            ("Nope", "finish", "unknown_type"),
            ("Job", "nope", "unknown_transition"),
            ("Job", "open", "unknown_transition"),
            ("Job", "archive", "not_requestable"),
        ] {
            assert_eq!(
                available(&w, OPERATOR, t, xn).unwrap().refused.as_deref(),
                Some(why)
            );
        }
        assert_eq!(
            exceptions(&w, "Nope").unwrap().refused.as_deref(),
            Some("unknown_type")
        );
    }

    /// `exceptions` answers a page at a time, at most a hundred, in creation
    /// order, a cursor going on from its last where more follow, every
    /// excepted object once, where it answered all in one page and took its
    /// cursor and ignored it (the audit's cost A13); a cursor of another read
    /// is refused, as a query's is.
    #[test]
    fn exceptions_answer_a_page_at_a_time() {
        let text = "\nmodule: shop\ncategories: [live]\ntypes:\n  Job:\n    attributes:\n      n: { type: int }\n    states:\n      OPEN: { category: live }\n    transitions:\n      open: { kind: initial, to: OPEN, required_inputs: [n] }\n";
        let lib = Library::load(&[("shop.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let mut made = Vec::new();
        for n in 0..250 {
            let r = Request {
                transition: "open".into(),
                actor: OPERATOR.into(),
                object_id: None,
                type_: Some("Job".into()),
                inputs: BTreeMap::from([("n".to_string(), json!(n))]),
                occurred_at: None,
                propose: false,
                expires_at: None,
                expected_version: None,
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            };
            match request(&mut s, &r).unwrap().unwrap() {
                Verdict::Satisfied { object_id, .. } => made.push(object_id),
                other => panic!("{other:?}"),
            }
        }
        let conn = s.connection();
        let at: i64 = conn
            .query_row("SELECT MAX(position) FROM of_event", [], |r| r.get(0))
            .unwrap();
        for id in &made {
            conn.execute(
                "INSERT INTO of_admission (object_id, invariant, position, reason) \
                 VALUES (?1, 'bounded', ?2, 'test')",
                crate::db::params![id, at],
            )
            .unwrap();
        }
        let w = s.snapshot().unwrap();
        let (mut seen, mut pages, mut cursor) = (Vec::new(), Vec::new(), None::<String>);
        // a bounded walk, so a cursor that never moves fails rather than hangs
        for _ in 0..10 {
            let p = exceptions_from(&w, "Job", cursor.as_deref()).unwrap();
            assert_eq!(p.refused, None);
            pages.push(p.items.len());
            seen.extend(p.items.iter().map(|i| i.id.clone()));
            match p.cursor {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        assert_eq!(pages, [100, 100, 50]);
        assert_eq!(seen, made);
        let another = format!("{}.{}", "0".repeat(12), made[0]);
        assert_eq!(
            exceptions_from(&w, "Job", Some(&another))
                .unwrap()
                .refused
                .as_deref(),
            Some("malformed_cursor")
        );
    }
}

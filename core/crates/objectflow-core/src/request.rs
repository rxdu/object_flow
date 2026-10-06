//! A request on one object, in the order of refusals (`DESIGN.md` §6), as the
//! reference runner takes it (`scripts/flowrun.py`, `Runner.attempt`):
//!
//! 1. the actor, by the generated guard `actor_known`;
//! 2. the object, `not found`; the transition, `unknown transition`;
//! 3. `not requestable`, a transition only via others; `unavailable`, a
//!    state the transition is not taken from;
//! 4. `invalid input`: a name it does not take, a value not of its type, an
//!    object of another type, a required input left out;
//! 5. the generated `<attribute>_provided` guards, then its own guards in
//!    order, a `deny` refusing, a `warn` told as a flag, an `audit` recorded;
//! 6. the outcome: the state, and the attributes its inputs write;
//! 7. every invariant, on every object in ascending id order: the declared,
//!    each state's required attributes, and uniqueness.
//!
//! What the first slice does not run is answered as [`NotRun`] where the
//! request reaches it, so every refusal decided before it is still decided
//! (ADR-0153 decision 3).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value as Json, json};

use crate::eval::{Evaluator, NotRun, Reading, Who, truth};
use crate::expr::Expr;
use crate::model::{
    DECLARATION_CHANGE, Declared, Enforce, From, Kind, Library, PROPOSAL, Remedy, Step, Transition,
    Type, Unique,
};
use crate::store::{Allocator, Change, Event};
use crate::value::{Value, parse_duration};
use crate::verdict::{Flag, InvariantFailure, Verdict};
use crate::wire;
use crate::world::{Object, Span, World};

/// A request, as the library takes it (`library-api.md` §3).
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub transition: String,
    /// The actor: an object's id, or the identity an actor's object holds.
    pub actor: String,
    pub object_id: Option<String>,
    /// The type to create, for an initial transition.
    pub type_: Option<String>,
    /// The inputs as the wire carries them (ADR-0150).
    pub inputs: BTreeMap<String, Json>,
    pub occurred_at: Option<i64>,
    /// File it as a proposal instead of executing it (`DESIGN.md` §9).
    pub propose: bool,
    /// A proposal's expiry, with `propose` only.
    pub expires_at: Option<i64>,
    /// The version of the object its caller read, the optimistic check of
    /// ADR-0023: a request finding it moved is refused as `stale`.
    pub expected_version: Option<i64>,
    /// The principal its actor acts for, an object's id or the identity it
    /// holds, recorded and never read by a rule (`DESIGN.md` §5.8).
    pub principal: Option<String>,
}

fn not_run<T>(what: impl Into<String>) -> Result<T, NotRun> {
    Err(NotRun(what.into()))
}

/// Who acts, resolved from a request's actor: an object of a type that marks
/// an actor identity (`DESIGN.md` §6 step 1).
pub(crate) fn resolve_actor(world: &World<'_>, actor: &str) -> Option<Who> {
    let identity = |o: &Object| -> Option<Who> {
        let t = world.lib.type_(&o.type_)?;
        let (attr, kind) = t.actor_identity()?;
        let id = match o.attr(attr) {
            Value::Text(s) => s,
            _ => return None,
        };
        Some(Who {
            id,
            kind: kind.to_string(),
            principal: None,
        })
    };
    if let Some(o) = world.get(actor) {
        return identity(o);
    }
    world
        .in_order()
        .into_iter()
        .filter_map(identity)
        .find(|w| w.id == actor)
}

/// The states a transition may be taken from (`flow-format.md` §4.8).
pub(crate) fn takes_from(t: &Type, x: &Transition, state: &str) -> bool {
    match &x.from {
        None => false,
        Some(From::States(states)) => states.iter().any(|s| s == state),
        // every state that is not final, and for an external transition
        // every one but its target, so no transition leaves a state for
        // itself (ADR-0122)
        Some(From::Any) => {
            t.state(state).is_some_and(|s| !s.is_final)
                && !(x.kind == Kind::External && x.to.first().is_some_and(|to| to == state))
        }
    }
}

/// What a refusal tells the attempt log beyond its verdict (`DESIGN.md`
/// §6, §9).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Refusal {
    /// Its clause was unknown rather than false (§8.2).
    pub unknown: bool,
    /// For an approval its request refused, the proposal and the request it
    /// held, whose transition the row is of.
    pub approving: Option<Box<(String, Request)>>,
    /// Everything the request had read when refused, the objects its
    /// outcome and cascades reached included, and what the failing clause
    /// consulted (`DESIGN.md` §7, ADR-0088): null where it read nothing.
    pub reads: Json,
    pub consulted: Json,
}

/// Why a request stops before it applies: a refusal, or a construct the
/// slice does not run.
enum Stop {
    /// Boxed, since what a refusal read makes it large beside the rest.
    Refused(Verdict, Box<Refusal>),
    NotRun(NotRun),
}

impl std::convert::From<NotRun> for Stop {
    fn from(e: NotRun) -> Stop {
        Stop::NotRun(e)
    }
}

fn refused<T>(v: Verdict) -> Result<T, Stop> {
    Err(Stop::Refused(v, Box::default()))
}

/// Who a request acts as: its actor, and the principal it names, each an
/// object of a type that marks an actor identity, or the request is refused
/// as `actor_known` (`DESIGN.md` §6 step 1). The kind is the actor's.
fn request_who(world: &World<'_>, req: &Request) -> Result<Who, Stop> {
    let unknown = || {
        refused(Verdict::Unsatisfied {
            clause: "actor_known".into(),
            remedy: Some(Remedy::Dependent),
        })
    };
    let Some(mut who) = resolve_actor(world, &req.actor) else {
        return unknown();
    };
    if let Some(p) = &req.principal {
        let Some(principal) = resolve_actor(world, p) else {
            return unknown();
        };
        who.principal = Some(principal.id);
    }
    Ok(who)
}

/// The optimistic check of ADR-0023: a request naming the version of its
/// object that its caller read is refused as `stale` where the object has
/// moved since, after its transition is found and before anything else of it
/// is checked (`DESIGN.md` §6 step 3). A creation names no object, and so has
/// no version to find moved.
fn stale(world: &World<'_>, req: &Request) -> Result<(), Stop> {
    if let (Some(expected), Some(o)) = (
        req.expected_version,
        req.object_id.as_deref().and_then(|id| world.get(id)),
    ) && o.version != expected
    {
        return refused(Verdict::Stale {
            expected,
            actual: o.version,
        });
    }
    Ok(())
}

fn stop_not_run<T>(what: impl Into<String>) -> Result<T, Stop> {
    Err(Stop::NotRun(NotRun(what.into())))
}

/// A clause qualified by the type it is of, unless it names one already.
fn qualified(clause: &str, type_: &str) -> String {
    if clause.contains('.') {
        clause.to_string()
    } else {
        format!("{type_}.{clause}")
    }
}

/// A refused call or creation refuses the request, naming the step, the
/// object it targeted and its own refusal, whose clause is qualified by the
/// type it targeted (ADR-0122 decision 35).
fn call_refused(stop: Stop, step: String, type_: &str, target: Option<String>) -> Stop {
    match stop {
        Stop::Refused(v, r) => {
            let clause = match &v {
                Verdict::CallRefused { clause, .. } => clause.clone(),
                other => other.clause().map(|c| qualified(&c, type_)),
            };
            Stop::Refused(
                Verdict::CallRefused {
                    step,
                    object_id: target,
                    verdict: Box::new(v),
                    clause,
                },
                r,
            )
        }
        other => other,
    }
}

/// A refusal of a part a cascade drives, its clause qualified by the part's
/// type (`flow-format.md` §11.4).
fn as_part(stop: Stop, type_: &str) -> Stop {
    match stop {
        Stop::Refused(Verdict::Unsatisfied { clause, remedy }, r) => Stop::Refused(
            Verdict::Unsatisfied {
                clause: qualified(&clause, type_),
                remedy,
            },
            r,
        ),
        other => other,
    }
}

/// One request's working copy of the store, and what it has done so far.
struct Ctx<'w> {
    work: World<'w>,
    /// The store as the request found it.
    before: World<'w>,
    who: Who,
    /// The requested transition's type, whose own clauses go unqualified.
    requested: String,
    occurred: Option<i64>,
    /// For an approved proposal, when it was filed, from which its occurred
    /// time's bound is measured (ADR-0147).
    filed_at: Option<i64>,
    /// The occurred time every transition the request takes records: the
    /// request's own, or now (`DESIGN.md` §6).
    at: i64,
    answers: BTreeMap<String, String>,
    flags: Vec<Flag>,
    /// The `warn` and `audit` guards of the transition being taken that
    /// failed: each clause, whether it raises a flag, its remedy and whether
    /// it was unknown, linked to its event once its position is allocated.
    failed: Vec<(String, bool, Option<Remedy>, bool, Reading)>,
    /// Their rows of the attempt log, written with the events (§7).
    observed: Vec<crate::store::Attempt>,
    /// For a check, where it names each guard it did not evaluate, one that
    /// asks an evaluator, which a check never asks (ADR-0054).
    checking: Checking,
    /// The positions of the transitions taking place, the innermost last:
    /// each event's cause is the one below its own (`DESIGN.md` §6 step 8).
    causes: Vec<i64>,
    /// The spans a correction merged into the one before them: an object,
    /// a dimension, and the span's entered position and time.
    merged: Vec<(String, String, i64, i64)>,
    /// For each object, what its current transition wrote by an input or an
    /// effect, whatever the value.
    wrote: BTreeMap<String, BTreeSet<String>>,
    /// The write index's rows the request sets, in the order set.
    index: Vec<(String, String, i64)>,
    /// The object an assertion set, and the invariants it admits (§4.13).
    asserted: Option<String>,
    admits: BTreeSet<String>,
    /// For each object this request erased, the personal attributes it erased.
    erased: BTreeMap<String, BTreeSet<String>>,
    /// Every event, in the order recorded.
    events: Vec<Event>,
    created: BTreeSet<String>,
    /// Everything the request's rules have read, for a refusal's row.
    read: Reading,
    /// What the rules evaluated for each event read, by its position: its
    /// transition's guards, and the invariants of its object if it is the
    /// object's last (ADR-0088).
    readings: BTreeMap<i64, Reading>,
}

/// What the request answers: its verdict, and what it writes if it applies.
pub fn decide(
    world: &World<'_>,
    alloc: &mut Allocator,
    req: &Request,
    answers: &BTreeMap<String, String>,
) -> Result<(Verdict, Option<Change>, Refusal), NotRun> {
    decide_as(world, alloc, req, answers, &None)
}

/// A check's collector of the guards it leaves unevaluated; none for a
/// request.
type Checking = Option<std::rc::Rc<std::cell::RefCell<Vec<String>>>>;

/// What a request decides, or, with a collector, what a check of it would
/// answer, its evaluator guards named and not asked.
fn decide_as(
    world: &World<'_>,
    alloc: &mut Allocator,
    req: &Request,
    answers: &BTreeMap<String, String>,
    checking: &Checking,
) -> Result<(Verdict, Option<Change>, Refusal), NotRun> {
    // a request filed for approval, and one deciding a proposal, are the
    // store's own (DESIGN.md §9)
    let target = req.object_id.as_deref().and_then(|id| world.get(id));
    let deciding = target.is_some_and(|o| o.type_ == PROPOSAL);
    let changing = target.is_some_and(|o| o.type_ == DECLARATION_CHANGE)
        || (req.object_id.is_none() && req.type_.as_deref() == Some(DECLARATION_CHANGE));
    let answer = if changing {
        decide_change(world, alloc, req)
    } else if req.propose {
        file(world, alloc, req)
    } else if deciding {
        decide_proposal(world, alloc, req, answers, checking)
    } else {
        decide_or_stop(world, alloc, req, answers, None, checking, None)
    };
    match answer {
        Ok((v, change)) => Ok((v, change, Refusal::default())),
        Err(Stop::Refused(v, r)) => Ok((v, None, *r)),
        Err(Stop::NotRun(e)) => Err(e),
    }
}

fn decide_or_stop(
    world: &World<'_>,
    alloc: &mut Allocator,
    req: &Request,
    answers: &BTreeMap<String, String>,
    filed_at: Option<i64>,
    checking: &Checking,
    cause: Option<i64>,
) -> Result<(Verdict, Option<Change>), Stop> {
    let lib = world.lib;
    // 1. the actor
    let who = request_who(world, req)?;
    // 2. the object and the transition
    let (tn, oid) = match &req.object_id {
        Some(id) => match world.get(id) {
            None => return refused(Verdict::NotFound),
            Some(o) => (o.type_.clone(), Some(id.clone())),
        },
        None => match &req.type_ {
            Some(t) => (t.clone(), None),
            None => {
                return refused(Verdict::UnknownTransition {
                    transition: req.transition.clone(),
                });
            }
        },
    };
    let Some(t) = lib.type_(&tn) else {
        return refused(Verdict::UnknownTransition {
            transition: req.transition.clone(),
        });
    };
    if let Some(what) = t.beyond.first() {
        return stop_not_run(what.clone());
    }
    let x = match t.transition(&req.transition) {
        Some(x) if (x.kind == Kind::Initial) == oid.is_none() => x,
        _ => {
            return refused(Verdict::UnknownTransition {
                transition: req.transition.clone(),
            });
        }
    };
    stale(world, req)?;
    // 3. not requestable, and unavailable
    if !x.only_via.is_empty() {
        return refused(Verdict::NotRequestable {
            parents: x.only_via.clone(),
        });
    }
    // an assertion runs from any state but a final one, an erasure at any
    if let Some(o) = oid.as_deref().and_then(|id| world.get(id))
        && !(match x.kind {
            Kind::Assertion => t.state(&o.state).is_some_and(|s| !s.is_final),
            Kind::Erasure => true,
            _ => takes_from(t, x, &o.state),
        })
    {
        return refused(Verdict::Unavailable {
            state: o.state.clone(),
        });
    }
    // 4. the inputs it does not take, and those not of their type
    let unknown: Vec<String> = req
        .inputs
        .keys()
        .filter(|k| !takes(t, x, k))
        .cloned()
        .collect();
    if !unknown.is_empty() {
        return refused(Verdict::InvalidInput { inputs: unknown });
    }
    let mut inputs: BTreeMap<String, Value> = BTreeMap::new();
    let mut wrong = Vec::new();
    for (k, raw) in &req.inputs {
        // an assertion's own: a state, and invariants, by name (§4.13)
        if x.kind == Kind::Assertion && (k == "to" || k == "admits") {
            match (k.as_str(), raw) {
                ("to", Json::String(s)) => {
                    inputs.insert(k.clone(), Value::Text(s.clone()));
                }
                ("admits", Json::Array(items)) if items.iter().all(Json::is_string) => {
                    let names = items
                        .iter()
                        .filter_map(|i| i.as_str().map(|s| Value::Text(s.to_string())))
                        .collect();
                    inputs.insert(k.clone(), Value::Set(names));
                }
                _ => wrong.push(k.clone()),
            }
            continue;
        }
        let Some((declared, many)) = input_type(t, x, k) else {
            return stop_not_run(format!("the input {k}, which names no attribute of {tn}"));
        };
        match wire::decode(raw, declared, many, lib) {
            Ok(Value::Absent) => {}
            Ok(v) => {
                inputs.insert(k.clone(), v);
            }
            Err(_) => wrong.push(k.clone()),
        }
    }
    if !wrong.is_empty() {
        return refused(Verdict::InvalidInput { inputs: wrong });
    }
    let mut ctx = Ctx {
        work: world.clone(),
        before: world.clone(),
        who,
        requested: tn.clone(),
        occurred: req.occurred_at,
        filed_at,
        at: req.occurred_at.unwrap_or(world.now),
        answers: answers.clone(),
        flags: Vec::new(),
        failed: Vec::new(),
        observed: Vec::new(),
        checking: checking.clone(),
        causes: cause.into_iter().collect(),
        merged: Vec::new(),
        wrote: BTreeMap::new(),
        index: Vec::new(),
        asserted: None,
        admits: BTreeSet::new(),
        erased: BTreeMap::new(),
        events: Vec::new(),
        created: BTreeSet::new(),
        read: Reading::default(),
        readings: BTreeMap::new(),
    };
    // an admission records the request's reason (ADR-0054)
    let reason = inputs.get("reason").map(text_of).unwrap_or_default();
    let root = match take(&mut ctx, alloc, &tn, &req.transition, oid, inputs, 0) {
        Ok(root) => root,
        Err(Stop::Refused(v, mut r)) => {
            r.reads = refused_reads(&ctx);
            return Err(Stop::Refused(v, r));
        }
        Err(stop) => return Err(stop),
    };
    // 7. the invariants, on every object, in ascending id order, except a
    // violation an admission lets stand, which is discharged once the
    // invariant holds again (ADR-0054); an assertion admits what it names
    let first = ctx.events.iter().map(|e| e.position).min().unwrap_or(0);
    let mut admitted = Vec::new();
    let mut discharged = Vec::new();
    let ids: Vec<String> = ctx.work.in_order().iter().map(|o| o.id.clone()).collect();
    for id in ids {
        let Some(obj) = ctx.work.get(&id) else {
            continue;
        };
        // what an object's invariants read is recorded on its last event,
        // for an object the request wrote
        let last = ctx
            .events
            .iter()
            .filter(|e| e.object_id == id)
            .map(|e| e.position)
            .max();
        let reading = std::cell::RefCell::new(Reading::default());
        let failing = failing_invariants_reading(&ctx.work, obj, last.map(|_| &reading))?;
        let read = reading.into_inner();
        ctx.read.merge(read.clone());
        if let Some(position) = last {
            ctx.readings.entry(position).or_default().merge(read);
        }
        let holding: Vec<String> = obj
            .admissions
            .iter()
            .filter(|n| !failing.contains(n))
            .cloned()
            .collect();
        let mut admit = Vec::new();
        let mut refusing = Vec::new();
        for n in failing {
            if obj.admissions.contains(&n) {
                continue;
            }
            // an assertion admits what it names; an erasure what it leaves
            // violated, on that object (§4.13)
            let named = (ctx.asserted.as_deref() == Some(id.as_str()) && ctx.admits.contains(&n))
                || ctx.admits.contains(&format!("{}.{n}", obj.type_))
                || erasure_broke(&ctx, obj, &n);
            if named { &mut admit } else { &mut refusing }.push(n);
        }
        if !refusing.is_empty() {
            let failures: Vec<InvariantFailure> = refusing
                .iter()
                .map(|n| InvariantFailure {
                    object_id: obj.id.clone(),
                    type_: obj.type_.clone(),
                    invariant: n.clone(),
                    remedy: invariant_remedy(&ctx.work, x, obj, n, &root),
                })
                .collect();
            let remedy = failures.first().and_then(|f| f.remedy);
            return Err(Stop::Refused(
                Verdict::InvariantViolated { failures, remedy },
                Box::new(Refusal {
                    reads: refused_reads(&ctx),
                    ..Refusal::default()
                }),
            ));
        }
        if let Some(o) = ctx.work.objects.get_mut(&id) {
            for n in holding {
                o.admissions.remove(&n);
                discharged.push((id.clone(), n, first));
            }
            for n in admit {
                o.admissions.insert(n.clone());
                admitted.push((id.clone(), n, first, reason.clone()));
            }
        }
    }
    // each event's read set, its own object's entry left to its order
    // (DESIGN.md §6)
    for e in &mut ctx.events {
        if let Some(r) = ctx.readings.get(&e.position) {
            e.reads = r.record(ctx.work.now, Some(&e.object_id));
        }
    }
    // the events in position order, the order they were allocated in: a
    // cause before what it caused, which names it, and an object's creation
    // before its other events (storage-schema.md §2)
    ctx.events.sort_by_key(|e| e.position);
    let mut seen = BTreeSet::new();
    let mut writes = Vec::new();
    for e in &ctx.events {
        let Some(o) = ctx.work.get(&e.object_id) else {
            return stop_not_run(format!(
                "an event on {}, which the request did not keep",
                e.object_id
            ));
        };
        let first = seen.insert(e.object_id.clone());
        writes.push((
            o.clone(),
            first && ctx.created.contains(&e.object_id),
            e.clone(),
        ));
    }
    let mut positions: Vec<i64> = ctx.events.iter().map(|e| e.position).collect();
    positions.sort_unstable();
    let version = ctx.work.get(&root).map_or(0, |o| o.version);
    // a row of a creation names the object its event made
    let mut observed = std::mem::take(&mut ctx.observed);
    for a in &mut observed {
        if a.object_id.is_none() {
            a.object_id = ctx
                .events
                .iter()
                .find(|e| Some(e.position) == a.applied_position)
                .map(|e| e.object_id.clone());
        }
    }
    Ok((
        Verdict::Satisfied {
            object_id: root,
            version,
            events: positions,
            flags: ctx.flags,
        },
        Some(Change {
            writes,
            index: ctx.index,
            admitted,
            discharged,
            redacted: ctx
                .erased
                .iter()
                .map(|(id, attrs)| (id.clone(), attrs.iter().cloned().collect()))
                .collect(),
            attempts: observed,
            merged: ctx.merged,
            ..Change::default()
        }),
    ))
}

/// Whether a transition takes an input of a name.
fn takes(t: &Type, x: &Transition, k: &str) -> bool {
    let _ = t;
    (x.kind == Kind::Assertion && (k == "to" || k == "admits"))
        || x.required_inputs
            .iter()
            .chain(&x.optional_inputs)
            .any(|n| n == k)
        || x.inputs.iter().any(|(n, _)| n == k)
}

/// The type an input is declared of: the transition's own input, or the
/// attribute it writes.
fn input_type<'t>(t: &'t Type, x: &'t Transition, k: &str) -> Option<(&'t Declared, bool)> {
    match x.inputs.iter().find(|(n, _)| n == k) {
        Some((_, spec)) => Some((&spec.declared, spec.many)),
        None => t.attribute(k).map(|a| (&a.declared, a.many)),
    }
}

/// One transition, the requested one or one it causes: its checks, its
/// guards, its outcome, its effect and its cascades, depth-first, and its
/// event (`DESIGN.md` §6). Returns the object's id.
fn take(
    ctx: &mut Ctx<'_>,
    alloc: &mut Allocator,
    tn: &str,
    xn: &str,
    oid: Option<String>,
    mut inputs: BTreeMap<String, Value>,
    depth: usize,
) -> Result<String, Stop> {
    let lib = ctx.work.lib;
    let Some(t) = lib.type_(tn) else {
        return stop_not_run(format!("the type {tn}"));
    };
    if let Some(what) = t.beyond.first() {
        return stop_not_run(what.clone());
    }
    let x = match t.transition(xn) {
        Some(x) if (x.kind == Kind::Initial) == oid.is_none() => x,
        _ => {
            return refused(Verdict::UnknownTransition {
                transition: xn.to_string(),
            });
        }
    };
    if x.kind == Kind::Assertion && depth > 0 {
        return stop_not_run(format!("a caused assertion ({tn}.{xn})"));
    }
    if depth > 0 {
        if let Some(o) = oid.as_deref().and_then(|id| ctx.work.get(id))
            && x.kind != Kind::Erasure
            && !takes_from(t, x, &o.state)
        {
            return refused(Verdict::Unavailable {
                state: o.state.clone(),
            });
        }
        let unknown: Vec<String> = inputs.keys().filter(|k| !takes(t, x, k)).cloned().collect();
        if !unknown.is_empty() {
            return refused(Verdict::InvalidInput { inputs: unknown });
        }
    }
    // a reference names an object of its type
    let mut wrong = Vec::new();
    for (k, v) in &inputs {
        if let Some((Declared::Reference(target), _)) = input_type(t, x, k) {
            let ids: Vec<&Value> = match v {
                Value::Set(items) => items.iter().collect(),
                one => vec![one],
            };
            let fits = ids.iter().all(|i| match i {
                // a label's subject may be of any type (declaration-syntax.md §6.8)
                Value::Object(id) => ctx.work.get(id).is_some_and(|o| {
                    target == crate::model::ANY_TYPE || lib.is_a(&o.type_, target)
                }),
                _ => false,
            });
            if !fits {
                wrong.push(k.clone());
            }
        }
    }
    if !wrong.is_empty() {
        return refused(Verdict::InvalidInput { inputs: wrong });
    }
    let left_out: Vec<String> = x
        .required_inputs
        .iter()
        .filter(|k| !inputs.contains_key(*k))
        .filter(|k| {
            t.attribute(k)
                .is_none_or(|a| !a.optional && a.default.is_none())
        })
        .cloned()
        .collect();
    if !left_out.is_empty() {
        return refused(Verdict::InvalidInput { inputs: left_out });
    }
    let mut missing = Vec::new();
    for (k, spec) in &x.inputs {
        if inputs.contains_key(k) {
            continue;
        }
        if let Some(d) = &spec.default {
            let v = Evaluator::new(&ctx.work, Some(t), oid.as_deref())
                .with_committed(&ctx.before)
                .with_inputs(&inputs)
                .with_actor(&ctx.who)
                .eval(d)?;
            inputs.insert(k.clone(), v);
        } else if !spec.optional {
            missing.push(k.clone());
        }
    }
    if !missing.is_empty() {
        return refused(Verdict::InvalidInput { inputs: missing });
    }
    // an assertion's `to` is a state it lists, other than the one the object
    // is in, and its `admits` only invariants it may admit (§4.13)
    if x.kind == Kind::Assertion {
        let current = oid
            .as_deref()
            .and_then(|id| ctx.work.get(id))
            .map(|o| o.state.clone());
        let to = match inputs.get("to") {
            Some(Value::Text(s)) if x.to.contains(s) && current.as_ref() != Some(s) => s.clone(),
            _ => {
                return refused(Verdict::InvalidInput {
                    inputs: vec!["to".into()],
                });
            }
        };
        let admits: BTreeSet<String> = match inputs.get("admits") {
            Some(Value::Set(items)) => items.iter().map(text_of).collect(),
            _ => BTreeSet::new(),
        };
        if !admits.iter().all(|a| x.may_admit.contains(a)) {
            return refused(Verdict::InvalidInput {
                inputs: vec!["admits".into()],
            });
        }
        let _ = to;
        ctx.asserted = oid.clone();
        ctx.admits = admits;
    }
    // 5. the generated guards, in the order flow-format.md §4.8 states,
    // then the transition's own. occurred_within: the requested transition's
    // bound, its backdating_limit or its kind's max_recording_delay, takes an
    // occurred time no further back than the bound and not after now; one
    // with no bound, or a correction, which keeps the corrected fact's, takes
    // none (§4.4)
    let occurred_within = || Verdict::Unsatisfied {
        clause: "occurred_within".into(),
        remedy: Some(Remedy::SelfServiceable),
    };
    if depth == 0
        && let Some(occurred) = ctx.occurred
    {
        let now = ctx.filed_at.unwrap_or(ctx.work.now);
        let within = x
            .backdating_limit
            .as_deref()
            .and_then(parse_duration)
            .is_some_and(|limit| now - limit <= occurred && occurred <= now);
        if inputs.contains_key("corrects") || !within {
            return refused(occurred_within());
        }
    }
    // and no transition the request takes precedes the current interval of
    // what it changes on its object
    if let Some(o) = oid.as_deref().and_then(|id| ctx.work.get(id)) {
        let mut changing: Vec<String> = lib
            .tracked(t)
            .into_iter()
            .filter(|m| inputs.get(m).is_some_and(|v| *v != o.attr(m)))
            .collect();
        let asserted_elsewhere = x.kind == Kind::Assertion
            && !matches!(inputs.get("to"), Some(Value::Text(s)) if *s == o.state);
        if x.kind == Kind::External || asserted_elsewhere {
            changing.push("state".into());
        }
        if changing.iter().any(|m| {
            o.spans
                .get(m)
                .and_then(|s| s.last())
                .is_some_and(|last| ctx.at < last.entered_at)
        }) {
            return refused(occurred_within());
        }
    }
    // whole_open: a part is not moved into a whole that is finished
    if let Some(o) = oid.as_deref().and_then(|id| ctx.work.get(id)) {
        for k in x.required_inputs.iter().chain(&x.optional_inputs) {
            let (Some(Value::Object(whole)), Some(a)) = (inputs.get(k), t.attribute(k)) else {
                continue;
            };
            let owner = match (&a.declared, &a.opposite) {
                (Declared::Reference(target), Some(back)) => lib
                    .type_(target)
                    .and_then(|tt| tt.attribute(back))
                    .is_some_and(|b| b.composite),
                _ => false,
            };
            let moved = o.attr(k) != Value::Object(whole.clone());
            let finished = ctx.work.get(whole).is_some_and(|w| {
                lib.type_(&w.type_)
                    .and_then(|wt| wt.state(&w.state))
                    .is_some_and(|s| s.is_final)
            });
            if owner && moved && finished {
                return refused(Verdict::Unsatisfied {
                    clause: "whole_open".into(),
                    remedy: Some(Remedy::SelfServiceable),
                });
            }
        }
    }
    // not_erased: an erased object's personal attributes are not written
    // again, by an input or by a step that assigns or adds; a clear writes
    // absence and passes (ADR-0147)
    if x.kind != Kind::Erasure
        && let Some(o) = oid.as_deref().and_then(|id| ctx.work.get(id))
        && o.erased
    {
        let by_input = x
            .required_inputs
            .iter()
            .chain(&x.optional_inputs)
            .filter(|k| inputs.contains_key(*k));
        let by_step = x.effect.iter().filter_map(|st| match st {
            Step::Assign { location, .. } | Step::Add { location, .. } => Some(location),
            _ => None,
        });
        if by_input
            .chain(by_step)
            .any(|a| t.attribute(a).is_some_and(|spec| spec.personal))
        {
            return refused(Verdict::Unsatisfied {
                clause: "not_erased".into(),
                remedy: Some(Remedy::UnreachableFromHere),
            });
        }
    }
    // a successor an input names is neither this object nor one already
    // superseded, so no supersession chain closes on itself (ADR-0144)
    for step in &x.effect {
        if let Step::Supersede(Expr::Path(base, k)) = step
            && matches!(&**base, Expr::Name(n) if n == "inputs")
            && let Some(Value::Object(successor)) = inputs.get(k)
        {
            let superseded = ctx.work.get(successor).is_some_and(|s| {
                lib.type_(&s.type_)
                    .and_then(|st| st.state(&s.state))
                    .is_some_and(|state| state.superseding)
            });
            if Some(successor.as_str()) == oid.as_deref() || superseded {
                return refused(Verdict::Unsatisfied {
                    clause: "successor_not_superseded".into(),
                    remedy: Some(Remedy::SelfServiceable),
                });
            }
        }
    }
    // a recording's: a correction names an observation of its subject that
    // nothing has corrected yet; a new fact is not added to a finished
    // subject, read as the request found it (declaration-syntax.md §6.8)
    if t.observation_on.is_some()
        && xn == "record"
        && let Some(Value::Object(subject)) = inputs.get("subject")
    {
        if let Some(Value::Object(corrected)) = inputs.get("corrects") {
            let named = Value::Object(corrected.clone());
            if ctx.work.get(corrected).map(|c| c.attr("subject"))
                != Some(Value::Object(subject.clone()))
            {
                return refused(Verdict::InvalidInput {
                    inputs: vec!["corrects".into()],
                });
            }
            if ctx
                .work
                .objects
                .values()
                .any(|other| other.attr("corrects") == named)
            {
                return refused(Verdict::Unsatisfied {
                    clause: "corrects_current".into(),
                    remedy: Some(Remedy::SelfServiceable),
                });
            }
        } else if let Some(s) = ctx.before.get(subject)
            && lib
                .type_(&s.type_)
                .and_then(|st| st.state(&s.state))
                .is_some_and(|state| state.is_final)
        {
            return refused(Verdict::Unsatisfied {
                clause: "subject_open".into(),
                remedy: Some(Remedy::UnreachableFromHere),
            });
        }
        // a mirror's observations are the import's to write (ADR-0101)
        if ctx
            .work
            .get(subject)
            .and_then(|s| lib.type_(&s.type_))
            .is_some_and(|st| st.mirror)
        {
            return refused(Verdict::Unsatisfied {
                clause: "subject_owned".into(),
                remedy: Some(Remedy::UnreachableFromHere),
            });
        }
    }
    for k in &x.required_inputs {
        if !inputs.contains_key(k) {
            return refused(Verdict::Unsatisfied {
                clause: format!("{k}_provided"),
                remedy: Some(Remedy::SelfServiceable),
            });
        }
    }
    // a label's name is vocabulary, normalised to lower case and trimmed
    // (declaration-syntax.md §6.8)
    if tn == crate::model::LABEL
        && xn == "record"
        && let Some(Value::Text(name)) = inputs.get_mut("name")
    {
        *name = name.trim().to_lowercase();
    }
    // what the transition's guards read, each guard's under its name
    let mut read = Reading::default();
    for (g, mode) in &x.guards {
        let Some(c) = t.condition(g) else {
            return stop_not_run(format!("the guard {g}, which {tn} does not declare"));
        };
        let reading = std::cell::RefCell::new(Reading::default());
        let (ok, unanswered) = {
            let asking = Evaluator::new(&ctx.work, Some(t), oid.as_deref())
                .with_committed(&ctx.before)
                .with_inputs(&inputs)
                .with_actor(&ctx.who)
                .with_reading(&reading, g);
            // a check asks no evaluator: a guard left unanswered is named,
            // and decides nothing (ADR-0054)
            let asking = if ctx.checking.is_some() {
                asking
            } else {
                asking.with_answers(&ctx.answers)
            };
            (
                truth(&asking.eval(&c.expression)?)?,
                asking.unanswered.get(),
            )
        };
        let guard_read = reading.into_inner();
        ctx.read.merge(guard_read.clone());
        read.merge(guard_read.clone());
        if let Some(named) = &ctx.checking
            && unanswered
        {
            named.borrow_mut().push(if tn == ctx.requested {
                g.clone()
            } else {
                qualified(g, tn)
            });
            continue;
        }
        if ok != Some(true) {
            match mode {
                Enforce::Deny => {
                    // an evaluator that did not answer leaves the guard
                    // unknown, and the caller may try again (ADR-0138)
                    return Err(Stop::Refused(
                        Verdict::Unsatisfied {
                            clause: g.clone(),
                            remedy: if unanswered {
                                Some(Remedy::Temporal)
                            } else {
                                c.remedy
                            },
                        },
                        Box::new(Refusal {
                            unknown: ok.is_none(),
                            consulted: guard_read
                                .consulted_record(ctx.work.now)
                                .unwrap_or(Json::Null),
                            ..Refusal::default()
                        }),
                    ));
                }
                Enforce::Warn => {
                    let clause = if tn == ctx.requested {
                        g.clone()
                    } else {
                        qualified(g, tn)
                    };
                    ctx.flags.push(Flag {
                        clause,
                        remedy: c.remedy,
                    });
                    ctx.failed
                        .push((g.clone(), true, c.remedy, ok.is_none(), guard_read));
                }
                // an observing clause's would-be refusal is recorded, and
                // refuses nothing (DESIGN.md §5.5)
                Enforce::Audit => {
                    ctx.failed
                        .push((g.clone(), false, c.remedy, ok.is_none(), guard_read));
                }
            }
        }
    }
    if let Some(what) = t.at_outcome.first() {
        return stop_not_run(what.clone());
    }
    // the attributes as the transition finds them, what it changed measured
    // against them
    let before_attrs = oid
        .as_deref()
        .and_then(|id| ctx.work.get(id))
        .map(|o| o.attrs.clone())
        .unwrap_or_default();
    // the tracked values as the transition finds them, its spans' baseline
    let before = oid
        .as_deref()
        .and_then(|id| ctx.work.get(id))
        .map(|o| tracked_values(lib, t, o));
    // 6. the outcome, its position allocated as it begins applying; its
    // cause is the transition taking place below it, and it is the cause of
    // every one it takes (DESIGN.md §6 step 8)
    let position = alloc.position();
    let cause = ctx.causes.last().copied();
    ctx.causes.push(position);
    let consulted = read.consulted_record(ctx.work.now);
    ctx.readings.insert(position, read);
    // each flag raised, and each audit's would-be refusal, a row of the type
    // whose clause failed, linked to this event (DESIGN.md §5.5, §7)
    for (clause, flagged, remedy, unknown, clause_read) in std::mem::take(&mut ctx.failed) {
        ctx.observed.push(crate::store::Attempt {
            at: ctx.work.now,
            actor_id: ctx.who.id.clone(),
            actor_kind: ctx.who.kind.clone(),
            principal: ctx.who.principal.clone(),
            type_: Some(tn.to_string()),
            object_id: oid.clone(),
            transition: Some(xn.to_string()),
            verdict: "unsatisfied".into(),
            clause: Some(clause),
            remedy: remedy.map_or_else(String::new, |r| r.text().to_string()),
            unknown,
            enforced: false,
            flagged,
            applied_position: Some(position),
            request_values: Json::Null,
            withheld: Vec::new(),
            declaration_version: lib.version,
            // an observing clause's would-be refusal carries what it read
            // and consulted (ADR-0088 §3)
            reads: clause_read.record(ctx.work.now, None),
            consulted: clause_read
                .consulted_record(ctx.work.now)
                .unwrap_or(Json::Null),
        });
    }
    alloc.txn();
    let (mut o, from_state) = match oid.as_deref().and_then(|id| ctx.work.get(id)) {
        Some(o) => (o.clone(), Some(o.state.clone())),
        None => {
            let Some(to) = x.to.first() else {
                return stop_not_run(format!("an initial transition with no target ({tn}.{xn})"));
            };
            let id = alloc.object_id(tn);
            let mut o = Object {
                id: id.clone(),
                type_: tn.to_string(),
                state: to.clone(),
                version: 0,
                attrs: BTreeMap::new(),
                created_at: ctx.at,
                created_by_kind: ctx.who.kind.clone(),
                order: ctx.work.next_order(),
                superseded_by: None,
                spans: BTreeMap::new(),
                writes: BTreeMap::new(),
                admissions: BTreeSet::new(),
                erased: false,
                created_dated: true,
                recorded_from: ctx.at,
            };
            for (a, spec) in &t.attributes {
                // a set is never absent: created empty unless given, a derived
                // end excepted, which its opposite computes (flow-format.md §4.3)
                if spec.stored_set() {
                    o.attrs.insert(a.clone(), Value::Set(Vec::new()));
                } else if matches!(&spec.declared, Declared::Scalar(s) if s == "counter") {
                    o.attrs.insert(a.clone(), Value::Int(0));
                }
            }
            ctx.work.objects.insert(id.clone(), o.clone());
            for (a, spec) in &t.attributes {
                if let Some(d) = &spec.default {
                    let v = Evaluator::new(&ctx.work, Some(t), Some(&id))
                        .with_committed(&ctx.before)
                        .with_inputs(&inputs)
                        .with_actor(&ctx.who)
                        .eval(d)?;
                    if !v.is_absent() {
                        o.attrs.insert(a.clone(), v);
                    }
                }
            }
            ctx.created.insert(id);
            (o, None)
        }
    };
    if x.kind == Kind::External
        && let Some(to) = x.to.first()
    {
        o.state = to.clone();
    }
    // an erasure makes every personal attribute absent, a personal set
    // empty, and its values leave the interval index (DESIGN.md §8)
    if x.kind == Kind::Erasure {
        let mut erased = BTreeSet::new();
        for (a, spec) in &t.attributes {
            if !spec.personal || !o.attrs.contains_key(a) {
                continue;
            }
            if spec.stored_set() {
                o.attrs.insert(a.clone(), Value::Set(Vec::new()));
            } else {
                o.attrs.remove(a);
            }
            erased.insert(a.clone());
        }
        for (a, spec) in &t.attributes {
            if spec.personal
                && let Some(spans) = o.spans.get_mut(a)
            {
                for sp in spans {
                    sp.value = Value::Absent;
                }
            }
        }
        o.erased = true;
        ctx.erased.entry(o.id.clone()).or_default().extend(erased);
        invalidate_proposals(ctx, alloc, &o.id)?;
    }
    // an assertion puts the object into the state the request names
    if x.kind == Kind::Assertion
        && let Some(Value::Text(to)) = inputs.get("to")
    {
        o.state = to.clone();
    }
    let mut written = serde_json::Map::new();
    for k in x.required_inputs.iter().chain(&x.optional_inputs) {
        if let Some(v) = inputs.get(k) {
            o.attrs.insert(k.clone(), v.clone());
            written.insert(k.clone(), wire::encode(v));
        }
    }
    let id = o.id.clone();
    ctx.wrote.insert(
        id.clone(),
        x.required_inputs
            .iter()
            .chain(&x.optional_inputs)
            .filter(|k| inputs.contains_key(*k))
            .cloned()
            .collect(),
    );
    if from_state.is_none() {
        mint(&mut ctx.work, alloc, t, &mut o)?;
    }
    // a recording is dated as it is recorded; a correction keeps the
    // occurred time of the fact it replaces (ADR-0146)
    if t.observation_on.is_some() && xn == "record" {
        let now = ctx.work.now;
        o.attrs.insert("recorded_at".into(), Value::Timestamp(now));
        let occurred = match inputs.get("corrects") {
            Some(Value::Object(c)) => ctx
                .work
                .get(c)
                .map_or(Value::Timestamp(now), |c| c.attr("occurred_at")),
            _ => Value::Timestamp(ctx.at),
        };
        o.attrs.insert("occurred_at".into(), occurred);
        // a label holds its subject's state when it was applied (DESIGN.md
        // §5.11); a label is requested, never created by an outcome, so
        // nothing has changed the subject in the request before it
        if tn == crate::model::LABEL
            && let Some(Value::Object(subject)) = inputs.get("subject")
            && let Some(s) = ctx.work.get(subject)
        {
            o.attrs
                .insert("subject_state".into(), Value::Text(s.state.clone()));
        }
    }
    ctx.work.objects.insert(id.clone(), o);
    // the effect, then the cascades to the parts, depth-first (ADR-0038)
    effects(
        ctx,
        alloc,
        t,
        &id,
        &x.effect,
        &inputs,
        &BTreeMap::new(),
        depth,
        position,
    )?;
    cascades(ctx, alloc, t, xn, &id, &inputs, depth)?;
    // an erasure reaches its observations through each one's forget, then
    // every member of its supersession chain, after its own steps and
    // cascades (DESIGN.md §8, ADR-0087, ADR-0147)
    if x.kind == Kind::Erasure {
        forget_observations(ctx, alloc, t, &id, &inputs, depth)?;
        erase_chain(ctx, alloc, &id, &inputs, depth)?;
    }
    // the event, recorded as the transition ends, with the spans it opens
    let imported = if x.corrects.is_empty() {
        BTreeSet::new()
    } else {
        ctx.work.imported.clone()
    };
    let Some(o) = ctx.work.objects.get_mut(&id) else {
        return stop_not_run(format!("the object {id}, which its own effect removed"));
    };
    // a correction replaces the wrong value for the span it was held,
    // explicitly: a tracked member's current interval is relabelled, and
    // opens no interval of its own, so its spans see no change; its event
    // names each attribute, the value it replaces, the new value and the
    // span it corrects (declaration-syntax.md §6.4)
    let mut before = before;
    let mut corrections = serde_json::Map::new();
    let tracked = lib.tracked(t);
    for m in &x.corrects {
        let value = o.attr(m);
        let was = before_attrs.get(m).cloned().unwrap_or(Value::Absent);
        let mut span = Json::Null;
        if was != value
            && tracked.contains(m)
            && let Some((since, gone)) = correct_span(lib, t, o, m, &value, position, &imported)
        {
            span = json!(since);
            if let Some((p, at)) = gone {
                ctx.merged.push((id.clone(), m.clone(), p, at));
            }
            if let Some(b) = before.as_mut() {
                for (_, v) in b.iter_mut().filter(|(n, _)| n == m) {
                    *v = value.clone();
                }
            }
        }
        corrections.insert(
            m.clone(),
            json!({ "replaced": wire::encode(&was), "value": wire::encode(&value), "span": span }),
        );
    }
    if !record_spans(lib, t, o, before.as_ref(), position, ctx.at, &ctx.who) {
        // what an effect changed, which no guard could see, precedes its
        // current interval
        return refused(occurred_within());
    }
    o.version += 1;
    // what it wrote, by an input or an effect or by changing a value, in the
    // write index; a part's transition writes its whole's part relationship,
    // and a recording its subject's collection (storage-schema.md §2)
    let mut touched = ctx.wrote.remove(&id).unwrap_or_default();
    for (a, v) in &o.attrs {
        if before_attrs.get(a) != Some(v) {
            touched.insert(a.clone());
        }
    }
    touched.extend(
        before_attrs
            .keys()
            .filter(|a| !o.attrs.contains_key(*a))
            .cloned(),
    );
    let mut index: Vec<(String, String)> = touched.into_iter().map(|a| (id.clone(), a)).collect();
    for (a, spec) in &t.attributes {
        if let (Declared::Reference(target), Some(back), Value::Object(whole)) =
            (&spec.declared, &spec.opposite, o.attr(a))
            && lib
                .type_(target)
                .and_then(|tt| tt.attribute(back))
                .is_some_and(|b| b.composite)
        {
            index.push((whole, back.clone()));
        }
    }
    if t.observation_on.is_some()
        && xn == "record"
        && let Value::Object(subject) = o.attr("subject")
        && let Some(st) = ctx.work.get(&subject).and_then(|s| lib.type_(&s.type_))
    {
        for (collection, kind) in &st.observations {
            if *kind == t.name {
                index.push((subject.clone(), collection.clone()));
            }
        }
        if t.name == crate::model::LABEL {
            index.push((subject.clone(), "labels".into()));
        }
    }
    for (object, attribute) in index {
        if let Some(target) = ctx.work.objects.get_mut(&object) {
            target.writes.insert(attribute.clone(), position);
        }
        ctx.index.push((object, attribute, position));
    }
    let Some(o) = ctx.work.objects.get(&id) else {
        return stop_not_run(format!("the object {id}, which its own effect removed"));
    };
    let event = Event {
        position,
        object_id: id.clone(),
        object_seq: o.version,
        transition: xn.to_string(),
        from_state: from_state.clone(),
        to_state: o.state.clone(),
        changes_state: from_state.as_deref() != Some(o.state.as_str()),
        source: match x.kind {
            Kind::Assertion => "asserted",
            Kind::Erasure => "erased",
            _ if !x.corrects.is_empty() => "corrected",
            _ => "observed",
        }
        .into(),
        actor_id: ctx.who.id.clone(),
        actor_kind: ctx.who.kind.clone(),
        principal: ctx.who.principal.clone(),
        cause,
        declaration_version: ctx.work.lib.version,
        recorded_at: ctx.work.now,
        occurred_at: Some(ctx.at),
        imported: false,
        // an erasure names what it erased, and never the values
        payload: if x.kind == Kind::Erasure {
            json!({ "erased": ctx.erased.get(&id).cloned().unwrap_or_default() })
        } else {
            let mut p = json!({ "inputs": written });
            if !corrections.is_empty() {
                p["corrected"] = Json::Object(corrections);
            }
            // an override's reason, by which overrides are counted, and a
            // correction's, unless the reason is personal (flow-format.md
            // §4.8, declaration-syntax.md §6.4)
            if (x.kind == Kind::Assertion || !x.corrects.is_empty())
                && !input_personal(t, x, "reason")
                && let Some(r) = inputs.get("reason")
            {
                p["reason"] = wire::encode(r);
            }
            // what its guards consulted, by guard and reference (ADR-0158)
            if let Some(c) = consulted {
                p["consulted"] = c;
            }
            p
        },
        reads: json!({}),
    };
    ctx.work.events.push(event.clone());
    ctx.events.push(event);
    ctx.causes.pop();
    Ok(id)
}

/// Whether an input is personal (`declaration-syntax.md` §10, check 10):
/// marked so, or written by the transition's effect into a personal
/// attribute, which makes it personal without the marking.
fn input_personal(t: &Type, x: &Transition, name: &str) -> bool {
    fn writes(steps: &[Step], t: &Type, name: &str) -> bool {
        steps.iter().any(|st| match st {
            Step::Assign { location, expr } | Step::Add { location, expr } => {
                let mut read = BTreeSet::new();
                expr.inputs_read(&mut read);
                read.contains(name) && t.attribute(location).is_some_and(|a| a.personal)
            }
            Step::Foreach { steps, .. } => writes(steps, t, name),
            _ => false,
        })
    }
    x.inputs.iter().any(|(n, i)| n == name && i.personal) || writes(&x.effect, t, name)
}

/// A correction of a tracked member: its current interval relabelled with
/// the corrected value and opening none of its own, a relabelled interval
/// equal in value to the one before it merging with it, and every interval
/// row whose value or `.held` it changes, read as a read derives it,
/// carrying the correction (`declaration-syntax.md` §6.4, §6.9, ADR-0146,
/// ADR-0147). Returns the corrected span's entered position and the entry of
/// a span it merged away; none where the member has no current interval.
fn correct_span(
    lib: &Library,
    t: &Type,
    o: &mut Object,
    m: &str,
    value: &Value,
    position: i64,
    imported: &BTreeSet<i64>,
) -> Option<(i64, Option<(i64, i64)>)> {
    type Rows = BTreeMap<(String, i64, i64), Value>;
    let held_of = |o: &Object| -> Rows {
        let mut out = Rows::new();
        for (d, spans) in &o.spans {
            for sp in spans {
                let row = if sp.legacy || imported.contains(&sp.entered_position) {
                    crate::eval::held_at(lib, t, o, sp.entered_at)
                } else {
                    crate::eval::held(lib, t, o, sp.entered_position, true)
                };
                let v = match row {
                    Value::Row(f) => f.get(m).cloned().unwrap_or(Value::Absent),
                    _ => Value::Absent,
                };
                out.insert((d.clone(), sp.entered_position, sp.entered_at), v);
            }
        }
        out
    };
    let was = held_of(o);
    let spans = o.spans.get_mut(m)?;
    let last = spans.last_mut().filter(|l| l.left_position.is_none())?;
    let since = last.entered_position;
    last.value = value.clone();
    last.corrected_by = Some(position);
    let mut merged = None;
    if spans.len() > 1
        && spans
            .get(spans.len() - 2)
            .is_some_and(|p| p.value == *value)
    {
        merged = spans
            .pop()
            .map(|gone| (gone.entered_position, gone.entered_at));
        if let Some(prev) = spans.last_mut() {
            prev.left_position = None;
            prev.left_at = None;
            prev.corrected_by = Some(position);
        }
    }
    let now = held_of(o);
    for (d, spans) in &mut o.spans {
        for sp in spans {
            let key = (d.clone(), sp.entered_position, sp.entered_at);
            if was.get(&key) != now.get(&key) {
                sp.corrected_by = Some(position);
            }
        }
    }
    Some((since, merged))
}

/// The value of the state and of each tracked member (`DESIGN.md` §7).
fn tracked_values(lib: &Library, t: &Type, o: &Object) -> Vec<(String, Value)> {
    lib.tracked(t)
        .into_iter()
        .map(|m| {
            let v = if m == "state" {
                Value::State {
                    type_: o.type_.clone(),
                    name: o.state.clone(),
                }
            } else {
                o.attr(&m)
            };
            (m, v)
        })
        .collect()
}

/// The spans a transition opens at its occurred time: on a creation, one for
/// the state and for every tracked member, absent ones included; otherwise
/// one for each whose value it changed, ending the current one. A write that
/// leaves a value as it was opens nothing (`storage-schema.md`, ADR-0101,
/// ADR-0106). False, and nothing opened, where a change would precede the
/// current interval it ends (§4.4).
fn record_spans(
    lib: &Library,
    t: &Type,
    o: &mut Object,
    before: Option<&Vec<(String, Value)>>,
    position: i64,
    now: i64,
    who: &Who,
) -> bool {
    let changed: Vec<(String, Value)> = tracked_values(lib, t, o)
        .into_iter()
        .filter(|(m, v)| {
            let was = before.and_then(|b| b.iter().find(|(n, _)| n == m).map(|(_, w)| w));
            let spans = o.spans.get(m).map_or(0, Vec::len);
            before.is_none() || spans == 0 || was != Some(v)
        })
        .collect();
    if before.is_some()
        && changed.iter().any(|(m, _)| {
            o.spans
                .get(m)
                .and_then(|s| s.last())
                .is_some_and(|last| now < last.entered_at)
        })
    {
        return false;
    }
    for (m, v) in changed {
        let spans = o.spans.entry(m).or_default();
        if let Some(last) = spans.last_mut()
            && last.left_position.is_none()
        {
            last.left_position = Some(position);
            last.left_at = Some(now);
        }
        spans.push(Span {
            value: v,
            entered_position: position,
            left_position: None,
            entered_at: now,
            left_at: None,
            entered_by: Some(who.id.clone()),
            entered_by_kind: who.kind.clone(),
            migrated: false,
            declaration_version: lib.version,
            legacy: false,
            corrected_by: None,
        });
    }
    true
}

/// Every proposal still pending on an erased object is invalidated, its
/// inputs redacted, the two one event, which is no transition the request
/// took (`DESIGN.md` §8, `storage-schema.md` §9 steps 7 and 11).
fn invalidate_proposals(
    ctx: &mut Ctx<'_>,
    alloc: &mut Allocator,
    erased: &str,
) -> Result<(), Stop> {
    let lib = ctx.work.lib;
    let Some(pt) = lib.type_(PROPOSAL) else {
        return Ok(());
    };
    let target = Value::Text(erased.to_string());
    let pending: Vec<String> = ctx
        .work
        .of_type(PROPOSAL)
        .iter()
        .filter(|p| p.state == "PENDING" && p.attr("target") == target)
        .map(|p| p.id.clone())
        .collect();
    for id in pending {
        let Some(mut p) = ctx.work.get(&id).cloned() else {
            continue;
        };
        let names: BTreeSet<String> = match p.attr("inputs") {
            Value::Text(t) => serde_json::from_str::<serde_json::Map<String, Json>>(&t)
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default(),
            _ => BTreeSet::new(),
        };
        let position = alloc.position();
        let before = tracked_values(lib, pt, &p);
        p.state = "INVALIDATED".into();
        p.attrs.insert("inputs".into(), Value::Text("{}".into()));
        record_spans(lib, pt, &mut p, Some(&before), position, ctx.at, &ctx.who);
        p.version += 1;
        let invalidated = Event {
            position,
            object_id: id.clone(),
            object_seq: p.version,
            transition: "invalidate".into(),
            from_state: Some("PENDING".into()),
            to_state: "INVALIDATED".into(),
            changes_state: true,
            source: "erased".into(),
            actor_id: ctx.who.id.clone(),
            actor_kind: ctx.who.kind.clone(),
            principal: ctx.who.principal.clone(),
            // an invalidation is caused by the erasure that made it
            cause: ctx.causes.last().copied(),
            declaration_version: ctx.work.lib.version,
            recorded_at: ctx.work.now,
            occurred_at: Some(ctx.at),
            imported: false,
            payload: json!({ "redacted": ["inputs"] }),
            reads: json!({}),
        };
        ctx.work.events.push(invalidated.clone());
        ctx.events.push(invalidated);
        // the inputs leave the proposal's own creation event too
        ctx.erased.entry(id.clone()).or_default().extend(names);
        ctx.work.objects.insert(id, p);
    }
    Ok(())
}

/// A request filed for approval instead of executed (`DESIGN.md` §9): it is
/// checked as a request is short of the object's state and the guards, its
/// occurred time held to the transition's bound now, when the store is
/// told; it creates a proposal, `PENDING`, which `Satisfied` names.
fn file(
    world: &World<'_>,
    alloc: &mut Allocator,
    req: &Request,
) -> Result<(Verdict, Option<Change>), Stop> {
    let lib = world.lib;
    let who = request_who(world, req)?;
    let (tn, oid) = match &req.object_id {
        Some(id) => match world.get(id) {
            None => return refused(Verdict::NotFound),
            Some(o) => (o.type_.clone(), Some(id.clone())),
        },
        None => match &req.type_ {
            Some(t) => (t.clone(), None),
            None => {
                return refused(Verdict::UnknownTransition {
                    transition: req.transition.clone(),
                });
            }
        },
    };
    let unknown = || Verdict::UnknownTransition {
        transition: req.transition.clone(),
    };
    let Some(t) = lib.type_(&tn) else {
        return refused(unknown());
    };
    if let Some(what) = t.beyond.first() {
        return stop_not_run(what.clone());
    }
    let x = match t.transition(&req.transition) {
        Some(x) if (x.kind == Kind::Initial) == oid.is_none() => x,
        _ => return refused(unknown()),
    };
    stale(world, req)?;
    if !x.proposable {
        return refused(Verdict::NotRequestable {
            parents: x.only_via.clone(),
        });
    }
    // its inputs as a request checks them: taken, of their type, naming
    // what exists, and none left out
    let mut wrong: Vec<String> = req
        .inputs
        .keys()
        .filter(|k| !takes(t, x, k))
        .cloned()
        .collect();
    for (k, raw) in &req.inputs {
        if wrong.contains(k) {
            continue;
        }
        let Some((declared, many)) = input_type(t, x, k) else {
            continue;
        };
        match wire::decode(raw, declared, many, lib) {
            Ok(v) => {
                if let Declared::Reference(target) = declared {
                    let ids: Vec<&Value> = match &v {
                        Value::Set(items) => items.iter().collect(),
                        Value::Absent => Vec::new(),
                        one => vec![one],
                    };
                    let fits = ids.iter().all(|i| matches!(i, Value::Object(id) if world.get(id).is_some_and(|o| lib.is_a(&o.type_, target))));
                    if !fits {
                        wrong.push(k.clone());
                    }
                }
            }
            Err(_) => wrong.push(k.clone()),
        }
    }
    let given = |k: &String| req.inputs.get(k).is_some_and(|v| !v.is_null());
    wrong.extend(
        x.required_inputs
            .iter()
            .filter(|k| !given(k))
            .filter(|k| {
                t.attribute(k)
                    .is_none_or(|a| !a.optional && a.default.is_none())
            })
            .cloned(),
    );
    wrong.extend(
        x.inputs
            .iter()
            .filter(|(k, spec)| !given(k) && !spec.optional && spec.default.is_none())
            .map(|(k, _)| k.clone()),
    );
    if !wrong.is_empty() {
        return refused(Verdict::InvalidInput { inputs: wrong });
    }
    if let Some(occurred) = req.occurred_at {
        let within = x
            .backdating_limit
            .as_deref()
            .and_then(parse_duration)
            .is_some_and(|limit| world.now - limit <= occurred && occurred <= world.now);
        if !within {
            return refused(Verdict::Unsatisfied {
                clause: "occurred_within".into(),
                remedy: Some(Remedy::SelfServiceable),
            });
        }
    }
    let Some(pt) = lib.type_(PROPOSAL) else {
        return stop_not_run("a store without the built-in Proposal");
    };
    let position = alloc.position();
    alloc.txn();
    let id = alloc.object_id(PROPOSAL);
    let mut attrs = BTreeMap::new();
    if let Some(target) = &oid {
        attrs.insert("target".to_string(), Value::Text(target.clone()));
    }
    attrs.insert("target_type".into(), Value::Text(tn.clone()));
    attrs.insert("transition".into(), Value::Text(req.transition.clone()));
    attrs.insert(
        "inputs".into(),
        Value::Text(Json::Object(req.inputs.clone().into_iter().collect()).to_string()),
    );
    attrs.insert("proposer".into(), Value::Text(req.actor.clone()));
    if let Some(occurred) = req.occurred_at {
        attrs.insert("occurred".into(), Value::Timestamp(occurred));
    }
    attrs.insert("filed_at".into(), Value::Timestamp(world.now));
    if let Some(expiry) = req.expires_at {
        attrs.insert("expires_at".into(), Value::Timestamp(expiry));
    }
    let mut o = Object {
        id: id.clone(),
        type_: PROPOSAL.into(),
        state: "PENDING".into(),
        version: 0,
        attrs,
        created_at: world.now,
        created_by_kind: who.kind.clone(),
        order: world.next_order(),
        superseded_by: None,
        spans: BTreeMap::new(),
        writes: BTreeMap::new(),
        admissions: BTreeSet::new(),
        erased: false,
        created_dated: true,
        recorded_from: world.now,
    };
    record_spans(lib, pt, &mut o, None, position, world.now, &who);
    o.version = 1;
    let event = Event {
        position,
        object_id: id.clone(),
        object_seq: 1,
        transition: "propose".into(),
        from_state: None,
        to_state: "PENDING".into(),
        changes_state: true,
        source: "observed".into(),
        actor_id: who.id.clone(),
        actor_kind: who.kind.clone(),
        principal: who.principal.clone(),
        cause: None,
        declaration_version: world.lib.version,
        recorded_at: world.now,
        occurred_at: Some(world.now),
        imported: false,
        payload: json!({ "inputs": req.inputs }),
        reads: json!({}),
    };
    Ok((
        Verdict::Satisfied {
            object_id: id,
            version: 1,
            events: vec![position],
            flags: Vec::new(),
        },
        Some(Change {
            writes: vec![(o, true, event)],
            index: Vec::new(),
            admitted: Vec::new(),
            discharged: Vec::new(),
            redacted: Vec::new(),
            ..Change::default()
        }),
    ))
}

/// A built-in object's event as the store records it: its spans, its
/// version, and the event itself, by the actor asking now.
#[allow(clippy::too_many_arguments)]
fn store_event(
    lib: &Library,
    t: &Type,
    o: &mut Object,
    before: Option<&Vec<(String, Value)>>,
    from_state: Option<String>,
    transition: &str,
    position: i64,
    now: i64,
    who: &Who,
    payload: Json,
) -> Event {
    record_spans(lib, t, o, before, position, now, who);
    o.version += 1;
    Event {
        position,
        object_id: o.id.clone(),
        object_seq: o.version,
        transition: transition.to_string(),
        changes_state: from_state.as_deref() != Some(o.state.as_str()),
        from_state,
        to_state: o.state.clone(),
        source: "observed".into(),
        actor_id: who.id.clone(),
        actor_kind: who.kind.clone(),
        principal: who.principal.clone(),
        cause: None,
        declaration_version: lib.version,
        recorded_at: now,
        occurred_at: Some(now),
        imported: false,
        payload,
        reads: json!({}),
    }
}

/// A change's source from the wire: each file's name to its text, and its
/// reference, refused where empty, longer than 512 characters, or holding a
/// control character (ADR-0135).
/// A change's source, its reference and its digest.
struct ChangeSource {
    files: Vec<(String, String)>,
    reference: String,
    digest: String,
}

fn change_source(req: &Request) -> Result<ChangeSource, Stop> {
    let mut wrong = Vec::new();
    let source: Vec<(String, String)> = match req.inputs.get("source") {
        // each file under the name it declares, a module's or its examples',
        // which the digest is computed over (ADR-0135, ADR-0155); a file that
        // does not parse is the dry run's to report
        Some(Json::Object(files))
            if !files.is_empty()
                && files.iter().all(|(n, t)| {
                    t.as_str().is_some_and(|t| {
                        crate::source::name_of(t).is_none_or(|declared| declared == *n)
                    })
                }) =>
        {
            files
                .iter()
                .map(|(n, t)| (n.clone(), t.as_str().unwrap_or_default().to_string()))
                .collect()
        }
        _ => {
            wrong.push("source".to_string());
            Vec::new()
        }
    };
    let reference = match req.inputs.get("source_ref") {
        Some(Json::String(r))
            if (1..=512).contains(&r.chars().count()) && !r.chars().any(char::is_control) =>
        {
            r.clone()
        }
        _ => {
            wrong.push("source_ref".to_string());
            String::new()
        }
    };
    let unknown: Vec<String> = req
        .inputs
        .keys()
        .filter(|k| !["source", "source_ref"].contains(&k.as_str()))
        .cloned()
        .collect();
    if !unknown.is_empty() {
        return refused(Verdict::InvalidInput { inputs: unknown });
    }
    if !wrong.is_empty() {
        return refused(Verdict::InvalidInput { inputs: wrong });
    }
    let Some(digest) = crate::source::digest(&source) else {
        return refused(Verdict::InvalidInput {
            inputs: vec!["source".into()],
        });
    };
    Ok(ChangeSource {
        files: source,
        reference,
        digest,
    })
}

/// A request on a change to the declaration (`DESIGN.md` §9, ADR-0155):
/// `draft` and `revise` take the source and its reference; `submit` and
/// `refresh` run the dry run and attach its report, refused while it finds
/// anything fatal; `reopen`, `reject` and `withdraw` move it on; `supersede`
/// is a publish's; and `publish`, the approval, is the next slice's.
fn decide_change(
    world: &World<'_>,
    alloc: &mut Allocator,
    req: &Request,
) -> Result<(Verdict, Option<Change>), Stop> {
    let lib = world.lib;
    let who = request_who(world, req)?;
    let Some(t) = lib.type_(DECLARATION_CHANGE) else {
        return stop_not_run("a store without the built-in DeclarationChange");
    };
    let xn = req.transition.as_str();
    let held = req.object_id.as_deref().and_then(|id| world.get(id));
    let unknown = || Verdict::UnknownTransition {
        transition: req.transition.clone(),
    };
    let (from, to): (&[&str], &str) = match (xn, held.is_some()) {
        ("draft", false) => (&[], "DRAFTED"),
        ("revise", true) => (&["DRAFTED"], "DRAFTED"),
        ("reopen", true) => (&["SUBMITTED"], "DRAFTED"),
        ("submit", true) => (&["DRAFTED"], "SUBMITTED"),
        ("refresh", true) => (&["SUBMITTED"], "SUBMITTED"),
        ("reject", true) => (&["SUBMITTED"], "REJECTED"),
        ("withdraw", true) => (&["DRAFTED", "SUBMITTED"], "WITHDRAWN"),
        ("supersede", true) => {
            stale(world, req)?;
            return refused(Verdict::NotRequestable {
                parents: vec!["publish".into()],
            });
        }
        ("publish", true) => {
            return stop_not_run(
                "the approval, which installs a version, the next slice (ADR-0155)",
            );
        }
        _ => return refused(unknown()),
    };
    stale(world, req)?;
    if let Some(h) = held
        && !from.contains(&h.state.as_str())
    {
        return refused(Verdict::Unavailable {
            state: h.state.clone(),
        });
    }
    let now = world.now;
    let mut o = match held {
        Some(h) => h.clone(),
        None => Object {
            id: String::new(),
            type_: DECLARATION_CHANGE.into(),
            state: to.into(),
            version: 0,
            attrs: BTreeMap::new(),
            created_at: now,
            created_by_kind: who.kind.clone(),
            order: world.next_order(),
            superseded_by: None,
            spans: BTreeMap::new(),
            writes: BTreeMap::new(),
            admissions: BTreeSet::new(),
            erased: false,
            created_dated: true,
            recorded_from: now,
        },
    };
    let before = held.map(|h| tracked_values(lib, t, h));
    let from_state = held.map(|h| h.state.clone());
    let mut payload = json!({});
    match xn {
        "draft" | "revise" => {
            let ChangeSource {
                files: source,
                reference,
                digest,
            } = change_source(req)?;
            let files: serde_json::Map<String, Json> = source
                .into_iter()
                .map(|(n, t)| (n, Json::String(t)))
                .collect();
            o.attrs.insert(
                "source".into(),
                Value::Text(Json::Object(files).to_string()),
            );
            o.attrs
                .insert("source_ref".into(), Value::Text(reference.clone()));
            o.attrs
                .insert("source_digest".into(), Value::Text(digest.clone()));
            o.attrs
                .insert("base_version".into(), Value::Int(lib.version));
            if xn == "draft" {
                o.attrs
                    .insert("drafted_by".into(), Value::Text(who.id.clone()));
                o.attrs
                    .insert("drafted_by_kind".into(), Value::Text(who.kind.clone()));
            }
            payload = json!({ "source_ref": reference, "source_digest": digest });
        }
        "submit" | "refresh" => {
            let text = |a: &str| match o.attr(a) {
                Value::Text(t) => t,
                _ => String::new(),
            };
            let source: Vec<(String, String)> =
                serde_json::from_str::<serde_json::Map<String, Json>>(&text("source"))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(n, t)| (n, t.as_str().unwrap_or_default().to_string()))
                    .collect();
            let report = crate::publish::dry_run(
                world,
                &crate::publish::Draft {
                    change_id: Some(&o.id),
                    source: &source,
                    source_ref: Some(&text("source_ref")),
                    source_digest: Some(&text("source_digest")),
                    actor: &req.actor,
                },
            )?;
            if report.fatal() {
                return refused(Verdict::Unsatisfied {
                    clause: "dry_run_clean".into(),
                    remedy: Some(Remedy::UnreachableFromHere),
                });
            }
            o.attrs.insert(
                "report".into(),
                Value::Text(crate::publish::report_json(&report).to_string()),
            );
        }
        _ => {}
    }
    let position = alloc.position();
    alloc.txn();
    if held.is_none() {
        o.id = alloc.object_id(DECLARATION_CHANGE);
    }
    o.state = to.into();
    let event = store_event(
        lib,
        t,
        &mut o,
        before.as_ref(),
        from_state,
        xn,
        position,
        now,
        &who,
        payload,
    );
    let id = o.id.clone();
    let version = o.version;
    Ok((
        Verdict::Satisfied {
            object_id: id,
            version,
            events: vec![position],
            flags: Vec::new(),
        },
        Some(Change {
            writes: vec![(o, held.is_none(), event)],
            ..Change::default()
        }),
    ))
}

/// A decision on a proposal (`DESIGN.md` §9): `reject` and `withdraw` end
/// it; `approve` executes the request it holds as the approver, every guard
/// evaluated now under the rules in force, its occurred time's bound
/// measured from the filing. A refused approval returns the request's
/// verdict and leaves the proposal pending; an applied one records the
/// proposal's `approve` first, then the target's event and what it causes,
/// and `Satisfied` names the target. A decision on a proposal no longer
/// pending is refused as `unavailable`, and so is an approval from its
/// expiry on, naming the state `expired`.
fn decide_proposal(
    world: &World<'_>,
    alloc: &mut Allocator,
    req: &Request,
    answers: &BTreeMap<String, String>,
    checking: &Checking,
) -> Result<(Verdict, Option<Change>), Stop> {
    let lib = world.lib;
    let who = request_who(world, req)?;
    let Some(p) = req.object_id.as_deref().and_then(|id| world.get(id)) else {
        return refused(Verdict::NotFound);
    };
    let Some(pt) = lib.type_(PROPOSAL) else {
        return stop_not_run("a store without the built-in Proposal");
    };
    let to = match req.transition.as_str() {
        "approve" => "EXECUTED",
        "reject" => "REJECTED",
        "withdraw" => "WITHDRAWN",
        // only a publish or an erasure invalidates a proposal
        "invalidate" => {
            stale(world, req)?;
            return refused(Verdict::NotRequestable {
                parents: Vec::new(),
            });
        }
        _ => {
            return refused(Verdict::UnknownTransition {
                transition: req.transition.clone(),
            });
        }
    };
    stale(world, req)?;
    if p.state != "PENDING" {
        return refused(Verdict::Unavailable {
            state: p.state.clone(),
        });
    }
    if to == "EXECUTED"
        && let Value::Timestamp(expiry) = p.attr("expires_at")
        && world.now >= expiry
    {
        return refused(Verdict::Unavailable {
            state: "expired".into(),
        });
    }
    // the proposal's own event, first
    let position = alloc.position();
    alloc.txn();
    let mut decided = p.clone();
    let before = tracked_values(lib, pt, &decided);
    decided.state = to.into();
    record_spans(
        lib,
        pt,
        &mut decided,
        Some(&before),
        position,
        world.now,
        &who,
    );
    decided.version += 1;
    let event = Event {
        position,
        object_id: p.id.clone(),
        object_seq: decided.version,
        transition: req.transition.clone(),
        from_state: Some("PENDING".into()),
        to_state: to.into(),
        changes_state: true,
        source: "observed".into(),
        actor_id: who.id.clone(),
        actor_kind: who.kind.clone(),
        principal: who.principal.clone(),
        cause: None,
        declaration_version: world.lib.version,
        recorded_at: world.now,
        occurred_at: Some(world.now),
        imported: false,
        payload: json!({}),
        reads: json!({}),
    };
    if to != "EXECUTED" {
        return Ok((
            Verdict::Satisfied {
                object_id: p.id.clone(),
                version: decided.version,
                events: vec![position],
                flags: Vec::new(),
            },
            Some(Change {
                writes: vec![(decided, false, event)],
                index: Vec::new(),
                admitted: Vec::new(),
                discharged: Vec::new(),
                redacted: Vec::new(),
                ..Change::default()
            }),
        ));
    }
    let text = |a: &str| match p.attr(a) {
        Value::Text(t) => Some(t),
        _ => None,
    };
    let held = Request {
        transition: text("transition").unwrap_or_default(),
        actor: req.actor.clone(),
        object_id: text("target"),
        type_: if text("target").is_some() {
            None
        } else {
            text("target_type")
        },
        inputs: text("inputs")
            .and_then(|i| serde_json::from_str::<BTreeMap<String, Json>>(&i).ok())
            .unwrap_or_default(),
        occurred_at: match p.attr("occurred") {
            Value::Timestamp(t) => Some(t),
            _ => None,
        },
        propose: false,
        expires_at: None,
        expected_version: None,
        principal: req.principal.clone(),
    };
    let filed_at = match p.attr("filed_at") {
        Value::Timestamp(t) => Some(t),
        _ => None,
    };
    let (verdict, change) = decide_or_stop(
        world,
        alloc,
        &held,
        answers,
        filed_at,
        checking,
        Some(position),
    )
    .map_err(|stop| match stop {
        Stop::Refused(v, mut r) => {
            r.approving = Some(Box::new((p.id.clone(), held.clone())));
            Stop::Refused(v, r)
        }
        other => other,
    })?;
    let Some(mut change) = change else {
        return Ok((verdict, None));
    };
    change.writes.insert(0, (decided, false, event));
    Ok((
        match verdict {
            Verdict::Satisfied {
                object_id,
                version,
                mut events,
                flags,
            } => {
                events.push(position);
                events.sort_unstable();
                Verdict::Satisfied {
                    object_id,
                    version,
                    events,
                    flags,
                }
            }
            other => other,
        },
        Some(change),
    ))
}

/// Each observation in the subject's collections whose kind has a personal
/// field, corrected ones included, forgotten in ascending id order, passing
/// the erasure's reason (`declaration-syntax.md` §6.8).
fn forget_observations(
    ctx: &mut Ctx<'_>,
    alloc: &mut Allocator,
    t: &Type,
    id: &str,
    inputs: &BTreeMap<String, Value>,
    depth: usize,
) -> Result<(), Stop> {
    for (_, kind) in &t.observations {
        if ctx
            .work
            .lib
            .type_(kind)
            .and_then(|k| k.transition("forget"))
            .is_none()
        {
            continue; // a kind with no personal field has nothing to forget
        }
        let subject = Value::Object(id.to_string());
        let ids: Vec<String> = ctx
            .work
            .of_type(kind)
            .iter()
            .filter(|x| x.attr("subject") == subject)
            .map(|x| x.id.clone())
            .collect();
        let reason = inputs
            .get("reason")
            .cloned()
            .unwrap_or_else(|| Value::Text("erasure".into()));
        for x in ids {
            take(
                ctx,
                alloc,
                kind,
                "forget",
                Some(x),
                [("reason".to_string(), reason.clone())].into(),
                depth + 1,
            )?;
        }
    }
    // and its labels' notes, corrected ones included, on its own event as
    // no transition (declaration-syntax.md §6.8)
    let subject = Value::Object(id.to_string());
    let labels: Vec<String> = ctx
        .work
        .of_type(crate::model::LABEL)
        .iter()
        .filter(|l| l.attr("subject") == subject && l.attr("note") != Value::Absent)
        .map(|l| l.id.clone())
        .collect();
    for l in labels {
        if let Some(label) = ctx.work.objects.get_mut(&l) {
            label.attrs.remove("note");
        }
        ctx.erased.entry(l).or_default().insert("note".into());
    }
    Ok(())
}

/// Every other member of the object's supersession chain, predecessors and
/// successors, each through its own type's erasure, given the inputs it
/// takes of the requested erasure's; a member with nothing personal declares
/// no erasure and is passed over, and a member's refusal refuses the request
/// as a group erasure (ADR-0087, ADR-0146, ADR-0147).
fn erase_chain(
    ctx: &mut Ctx<'_>,
    alloc: &mut Allocator,
    id: &str,
    inputs: &BTreeMap<String, Value>,
    depth: usize,
) -> Result<(), Stop> {
    let mut chain = BTreeSet::new();
    let mut todo = vec![id.to_string()];
    while let Some(cur) = todo.pop() {
        if !chain.insert(cur.clone()) {
            continue;
        }
        if let Some(next) = ctx.work.get(&cur).and_then(|o| o.superseded_by.clone()) {
            todo.push(next);
        }
        todo.extend(
            ctx.work
                .objects
                .values()
                .filter(|p| p.superseded_by.as_deref() == Some(cur.as_str()))
                .map(|p| p.id.clone()),
        );
    }
    let mut members: Vec<(i64, String, String)> = chain
        .iter()
        .filter(|m| m.as_str() != id)
        .filter_map(|m| {
            ctx.work
                .get(m)
                .map(|o| (o.order, o.id.clone(), o.type_.clone()))
        })
        .collect();
    members.sort();
    for (_, member, type_) in members {
        if ctx.erased.contains_key(&member) && ctx.work.get(&member).is_some_and(|o| o.erased) {
            continue;
        }
        let Some((xn, x)) = ctx
            .work
            .lib
            .type_(&type_)
            .and_then(|mt| mt.transitions.iter().find(|(_, x)| x.kind == Kind::Erasure))
        else {
            continue;
        };
        let (xn, given) = (
            xn.clone(),
            inputs
                .iter()
                .filter(|(k, _)| x.inputs.iter().any(|(n, _)| n == *k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );
        take(
            ctx,
            alloc,
            &type_,
            &xn,
            Some(member.clone()),
            given,
            depth + 1,
        )
        .map_err(|s| call_refused(s, "group erasure".into(), &type_, Some(member.clone())))?;
    }
    Ok(())
}

/// Whether an invariant fails on an object because this request erased what
/// it reads: the object's own attribute, or, one hop away, an object a
/// reference it reads names (§4.13, ADR-0147).
fn erasure_broke(ctx: &Ctx<'_>, obj: &Object, n: &str) -> bool {
    let Some(t) = ctx.work.lib.type_(&obj.type_) else {
        return false;
    };
    let read = invariant_reads(t, n);
    if ctx
        .erased
        .get(&obj.id)
        .is_some_and(|e| e.iter().any(|a| read.contains(a)))
    {
        return true;
    }
    read.iter()
        .any(|r| matches!(obj.attr(r), Value::Object(other) if ctx.erased.contains_key(&other)))
}

/// The attributes an invariant reads, generated ones included: an
/// attribute's uniqueness, a state's requirement.
pub(crate) fn invariant_reads(t: &Type, n: &str) -> BTreeSet<String> {
    if let Some(a) = n.strip_suffix("_unique") {
        return [a.to_string()].into();
    }
    if let Some(s) = n.strip_suffix("_invariant") {
        return t
            .states
            .iter()
            .find(|(name, _)| name.to_lowercase() == s)
            .map(|(_, st)| st.required_attributes.iter().cloned().collect())
            .unwrap_or_default();
    }
    let mut read = BTreeSet::new();
    if let Some((_, e)) = t.invariants.iter().find(|(name, _)| name == n) {
        names_read(e, &mut read);
    }
    read
}

/// A write whose right-hand side is an unsupplied optional input is skipped
/// (`DESIGN.md` §5.4).
fn unsupplied(e: &Expr, inputs: &BTreeMap<String, Value>) -> bool {
    matches!(e, Expr::Path(base, k) if matches!(&**base, Expr::Name(n) if n == "inputs") && !inputs.contains_key(k))
}

/// An expression as a step writes it, for naming a call.
fn dotted(e: &Expr) -> String {
    match e {
        Expr::Name(n) => n.clone(),
        Expr::Path(b, m) => format!("{}.{m}", dotted(b)),
        _ => "…".to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn effect_value(
    ctx: &Ctx<'_>,
    t: &Type,
    id: &str,
    inputs: &BTreeMap<String, Value>,
    env: &BTreeMap<String, Value>,
    position: i64,
    e: &Expr,
    filtering: bool,
) -> Result<Value, NotRun> {
    let ev = Evaluator::new(&ctx.work, Some(t), Some(id))
        .with_committed(&ctx.before)
        .with_inputs(inputs)
        .with_actor(&ctx.who)
        .with_answers(&ctx.answers)
        .with_event(position);
    if filtering {
        ev.filtering().eval_in(e, env)
    } else {
        ev.eval_in(e, env)
    }
}

/// The order a loop takes its items in: objects by ascending id, enumeration
/// members in declaration order, other values ascending (`DESIGN.md` §6,
/// ADR-0147).
fn loop_order(world: &World<'_>, a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let rank = |v: &Value| -> (u8, i64) {
        match v {
            Value::Object(id) => (0, world.get(id).map_or(i64::MAX, |o| o.order)),
            Value::Member {
                enumeration,
                member,
            } => (
                1,
                world
                    .lib
                    .enumerations
                    .get(enumeration)
                    .and_then(|ms| ms.iter().position(|m| m == member))
                    .and_then(|p| i64::try_from(p).ok())
                    .unwrap_or(i64::MAX),
            ),
            _ => (2, 0),
        }
    };
    let (ra, rb) = (rank(a), rank(b));
    if ra.0 != 2 || rb.0 != 2 {
        return ra.cmp(&rb);
    }
    a.compare(b).unwrap_or(Ordering::Equal)
}

/// An effect's steps, in order (`DESIGN.md` §6 step 6).
#[allow(clippy::too_many_arguments)]
fn effects(
    ctx: &mut Ctx<'_>,
    alloc: &mut Allocator,
    t: &Type,
    id: &str,
    steps: &[Step],
    inputs: &BTreeMap<String, Value>,
    env: &BTreeMap<String, Value>,
    depth: usize,
    position: i64,
) -> Result<(), Stop> {
    let mut env = env.clone();
    for step in steps {
        match step {
            Step::Assign { location, expr } => {
                if unsupplied(expr, inputs) {
                    continue;
                }
                let v = effect_value(ctx, t, id, inputs, &env, position, expr, false)?;
                if v.is_absent() {
                    return stop_not_run(
                        "an assignment of an absent value, which only an unsupplied optional input may be",
                    );
                }
                if let Some(o) = ctx.work.objects.get_mut(id) {
                    o.attrs.insert(location.clone(), v);
                }
                ctx.wrote
                    .entry(id.to_string())
                    .or_default()
                    .insert(location.clone());
            }
            Step::Clear(names) => {
                if let Some(o) = ctx.work.objects.get_mut(id) {
                    for n in names {
                        o.attrs.remove(n);
                    }
                }
                ctx.wrote
                    .entry(id.to_string())
                    .or_default()
                    .extend(names.iter().cloned());
            }
            Step::Add { location, expr } | Step::Remove { location, expr } => {
                if unsupplied(expr, inputs) {
                    continue;
                }
                let v = effect_value(ctx, t, id, inputs, &env, position, expr, false)?;
                if v.is_absent() {
                    return stop_not_run("adding or removing an absent value");
                }
                ctx.wrote
                    .entry(id.to_string())
                    .or_default()
                    .insert(location.clone());
                if let Some(o) = ctx.work.objects.get_mut(id) {
                    let mut items = match o.attr(location) {
                        Value::Set(items) => items,
                        _ => Vec::new(),
                    };
                    let present = items.iter().any(|i| i.equals(&v));
                    match step {
                        Step::Add { .. } if !present => items.push(v),
                        Step::Remove { .. } if present => items.retain(|i| !i.equals(&v)),
                        _ => {}
                    }
                    o.attrs.insert(location.clone(), Value::Set(items));
                }
            }
            Step::Create {
                type_,
                transition,
                inputs: binds,
                result,
            } => {
                let values = arguments(ctx, t, id, inputs, &env, position, binds)?;
                let new =
                    take(ctx, alloc, type_, transition, None, values, depth + 1).map_err(|s| {
                        call_refused(s, format!("create {type_}.{transition}"), type_, None)
                    })?;
                if let Some(r) = result {
                    env.insert(r.clone(), Value::Object(new));
                }
            }
            Step::Call {
                target,
                transition,
                inputs: binds,
            } => {
                let target_id =
                    match effect_value(ctx, t, id, inputs, &env, position, target, false)? {
                        // a call through an absent optional end reaches nothing (DESIGN.md §5.4)
                        Value::Absent => continue,
                        Value::Object(tid) if ctx.work.get(&tid).is_some() => tid,
                        _ => return stop_not_run("a call whose target is not one object"),
                    };
                let values = arguments(ctx, t, id, inputs, &env, position, binds)?;
                let ttype = ctx
                    .work
                    .get(&target_id)
                    .map(|o| o.type_.clone())
                    .unwrap_or_default();
                take(
                    ctx,
                    alloc,
                    &ttype,
                    transition,
                    Some(target_id.clone()),
                    values,
                    depth + 1,
                )
                .map_err(|s| {
                    call_refused(
                        s,
                        format!("call {}.{transition}", dotted(target)),
                        &ttype,
                        Some(target_id.clone()),
                    )
                })?;
            }
            Step::Foreach {
                item,
                array,
                range,
                filter,
                limit,
                steps,
            } => {
                let mut items: Vec<Value> = if let Some(r) = range {
                    match effect_value(ctx, t, id, inputs, &env, position, r, false)? {
                        Value::Int(n) => (1..=n).map(Value::Int).collect(),
                        // absent or not positive: no step (ADR-0146)
                        _ => Vec::new(),
                    }
                } else if let Some(a) = array {
                    match effect_value(ctx, t, id, inputs, &env, position, a, false)? {
                        Value::Set(items) => items,
                        _ => return stop_not_run("a loop over what is not a collection"),
                    }
                } else {
                    Vec::new()
                };
                items.sort_by(|a, b| loop_order(&ctx.work, a, b));
                if let Some(f) = filter {
                    let mut kept = Vec::new();
                    for i in items {
                        let mut inner = env.clone();
                        inner.insert(item.clone(), i.clone());
                        if truth(&effect_value(
                            ctx, t, id, inputs, &inner, position, f, true,
                        )?)? == Some(true)
                        {
                            kept.push(i);
                        }
                    }
                    items = kept;
                }
                if i64::try_from(items.len()).unwrap_or(i64::MAX) > *limit {
                    return refused(Verdict::OverLimit {
                        loop_: format!(
                            "the loop over {}",
                            array
                                .as_ref()
                                .or(range.as_ref())
                                .map_or_else(String::new, dotted)
                        ),
                        bound: *limit,
                    });
                }
                for i in items {
                    let mut inner = env.clone();
                    inner.insert(item.clone(), i);
                    effects(ctx, alloc, t, id, steps, inputs, &inner, depth, position)?;
                }
            }
            Step::Supersede(expr) => {
                match effect_value(ctx, t, id, inputs, &env, position, expr, false)? {
                    Value::Object(successor) => {
                        if let Some(o) = ctx.work.objects.get_mut(id) {
                            o.superseded_by = Some(successor);
                        }
                    }
                    _ => return stop_not_run("a supersession naming no successor"),
                }
            }
        }
    }
    Ok(())
}

/// A call's or a creation's inputs, each evaluated in the calling object's
/// context; an unsupplied optional input, or an absent value, is not passed.
fn arguments(
    ctx: &Ctx<'_>,
    t: &Type,
    id: &str,
    inputs: &BTreeMap<String, Value>,
    env: &BTreeMap<String, Value>,
    position: i64,
    binds: &[(String, Expr)],
) -> Result<BTreeMap<String, Value>, NotRun> {
    let mut out = BTreeMap::new();
    for (k, e) in binds {
        if unsupplied(e, inputs) {
            continue;
        }
        let v = effect_value(ctx, t, id, inputs, env, position, e, false)?;
        if !v.is_absent() {
            out.insert(k.clone(), v);
        }
    }
    Ok(out)
}

/// The parts each composite end drives on this transition, in ascending id
/// order, those already in a final state skipped (`DESIGN.md` §6 step 6).
fn cascades(
    ctx: &mut Ctx<'_>,
    alloc: &mut Allocator,
    t: &Type,
    xn: &str,
    id: &str,
    inputs: &BTreeMap<String, Value>,
    depth: usize,
) -> Result<(), Stop> {
    for (an, a) in &t.attributes {
        if !a.composite {
            continue;
        }
        for clause in &a.cascade {
            if !clause.on.iter().any(|o| o == xn) {
                continue;
            }
            let parts = Evaluator::new(&ctx.work, Some(t), Some(id))
                .with_committed(&ctx.before)
                .with_inputs(inputs)
                .with_actor(&ctx.who)
                .eval(&Expr::Name(an.clone()))?;
            let ids: Vec<String> = match parts {
                Value::Set(items) => items
                    .into_iter()
                    .filter_map(|v| {
                        if let Value::Object(p) = v {
                            Some(p)
                        } else {
                            None
                        }
                    })
                    .collect(),
                Value::Object(p) => vec![p],
                _ => Vec::new(),
            };
            let mut driven: Vec<&Object> = ids
                .iter()
                .filter_map(|p| ctx.work.get(p))
                .filter(|p| {
                    ctx.work
                        .lib
                        .type_(&p.type_)
                        .and_then(|pt| pt.state(&p.state))
                        .is_none_or(|s| !s.is_final)
                })
                .collect();
            driven.sort_by_key(|p| p.order);
            let driven: Vec<(String, String)> = driven
                .iter()
                .map(|p| (p.id.clone(), p.type_.clone()))
                .collect();
            if driven.is_empty() {
                continue;
            }
            if let Some(limit) = clause.limit
                && i64::try_from(driven.len()).unwrap_or(i64::MAX) > limit
            {
                return refused(Verdict::OverLimit {
                    loop_: format!("the cascade on {an}"),
                    bound: limit,
                });
            }
            for (p, ptype) in driven {
                let mut values = BTreeMap::new();
                for (k, e) in &clause.inputs {
                    let v = Evaluator::new(&ctx.work, Some(t), Some(id))
                        .with_committed(&ctx.before)
                        .with_inputs(inputs)
                        .with_actor(&ctx.who)
                        .eval(e)?;
                    if !v.is_absent() {
                        values.insert(k.clone(), v);
                    }
                }
                take(
                    ctx,
                    alloc,
                    &ptype,
                    &clause.transition,
                    Some(p),
                    values,
                    depth + 1,
                )
                .map_err(|s| as_part(s, &ptype))?;
            }
        }
    }
    Ok(())
}

/// A value as an identifier's format writes it, as text.
pub(crate) fn text_of(v: &Value) -> String {
    match v {
        Value::Text(s) | Value::Object(s) => s.clone(),
        Value::State { name, .. } => name.clone(),
        Value::Member { member, .. } => member.clone(),
        other => match wire::encode(other) {
            Json::String(s) => s,
            j => j.to_string(),
        },
    }
}

/// Each identifier of a new object, numbered from 1 per sequence and scope,
/// as its format writes it (`flow-format.md` §4.12): `{n}` the number, `{n:6}`
/// zero-padded, `{a.b}` a value of the object, and `[ … ]` a segment left out
/// whole when a value in it is absent.
fn mint(
    after: &mut World<'_>,
    alloc: &mut Allocator,
    t: &Type,
    o: &mut Object,
) -> Result<(), NotRun> {
    for (a, spec) in &t.attributes {
        let Some(ident) = &spec.identifier else {
            continue;
        };
        let scope = match &ident.scope {
            None => String::new(),
            Some(attr) => match o.attr(attr) {
                Value::Absent => {
                    return not_run(format!("minting {}.{a} with its scope absent", t.name));
                }
                v => text_of(&v),
            },
        };
        let n = alloc.draw(&ident.sequence, &scope);
        after.objects.insert(o.id.clone(), o.clone());
        let field = |name: &str| -> Result<Value, NotRun> {
            let e = crate::expr::parse(name)
                .map_err(|e| NotRun(format!("a format field {name}: {e}")))?;
            Evaluator::new(after, Some(t), Some(&o.id)).eval(&e)
        };
        // a field's text: the number, padded, or a value of the object
        let render = |body: &str| -> Result<Option<String>, NotRun> {
            let mut out = String::new();
            let mut rest = body;
            while let Some(start) = rest.find('{') {
                out.push_str(&rest[..start]);
                let Some(len) = rest[start..].find('}') else {
                    return not_run(format!("the format {}", ident.format));
                };
                let inner = &rest[start + 1..start + len];
                let (name, width) = inner.split_once(':').unwrap_or((inner, ""));
                if name == "n" {
                    let w: usize = width.parse().unwrap_or(0);
                    out.push_str(&format!("{n:0w$}"));
                } else {
                    match field(name)? {
                        Value::Absent => return Ok(None),
                        v => out.push_str(&text_of(&v)),
                    }
                }
                rest = &rest[start + len + 1..];
            }
            out.push_str(rest);
            Ok(Some(out))
        };
        let mut text = String::new();
        let mut rest = ident.format.as_str();
        while let Some(open) = rest.find('[') {
            let Some(close) = rest[open..].find(']') else {
                return not_run(format!("the format {}", ident.format));
            };
            match render(&rest[..open])? {
                Some(t) => text.push_str(&t),
                None => {
                    return not_run(format!(
                        "a format field of {}.{a} that is absent outside `[ … ]`",
                        t.name
                    ));
                }
            }
            // a segment whose value is absent is left out whole
            if let Some(seg) = render(&rest[open + 1..open + close])? {
                text.push_str(&seg);
            }
            rest = &rest[open + close + 1..];
        }
        match render(rest)? {
            Some(t) => text.push_str(&t),
            None => {
                return not_run(format!(
                    "a format field of {}.{a} that is absent outside `[ … ]`",
                    t.name
                ));
            }
        }
        o.attrs.insert(a.clone(), Value::Text(text));
    }
    after.objects.insert(o.id.clone(), o.clone());
    Ok(())
}

/// An object's failing invariants in the order its type declares them: its
/// declared invariants as written, then its state's required attributes, then
/// each attribute's uniqueness by attribute (`flow-format.md` §4.8,
/// ADR-0147). An invariant that is unknown holds (`declaration-syntax.md`
/// §8.2).
pub fn failing_invariants(world: &World<'_>, o: &Object) -> Result<Vec<String>, NotRun> {
    failing_invariants_reading(world, o, None)
}

/// The invariants an object fails, and, given a collector, what each
/// declared invariant read, under its name (ADR-0088).
fn failing_invariants_reading(
    world: &World<'_>,
    o: &Object,
    reading: Option<&std::cell::RefCell<Reading>>,
) -> Result<Vec<String>, NotRun> {
    let Some(t) = world.lib.type_(&o.type_) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (n, e) in &t.invariants {
        let mut evaluating = Evaluator::new(world, Some(t), Some(&o.id));
        if let Some(r) = reading {
            evaluating = evaluating.with_reading(r, n);
        }
        if truth(&evaluating.eval(e)?)? == Some(false) {
            out.push(n.clone());
        }
    }
    if let Some(s) = t.state(&o.state)
        && s.required_attributes.iter().any(|a| o.attr(a).is_absent())
    {
        out.push(format!("{}_invariant", o.state.to_lowercase()));
    }
    for (a, spec) in &t.attributes {
        let mut unique = spec.unique.clone();
        // a singular stored end whose opposite is singular too is one to one
        if unique == Unique::No
            && let (Declared::Reference(target), Some(opposite), false) =
                (&spec.declared, &spec.opposite, spec.many)
        {
            let back = world
                .lib
                .type_(target)
                .and_then(|bt| bt.attribute(opposite));
            if back.is_some_and(|b| matches!(b.declared, Declared::Reference(_)) && !b.many) {
                unique = Unique::Yes;
            }
        }
        let (with, filter): (Vec<String>, Option<&Expr>) = match &unique {
            Unique::No => continue,
            Unique::Yes => (vec![a.clone()], None),
            Unique::With(others) => (
                std::iter::once(a.clone())
                    .chain(others.iter().cloned())
                    .collect(),
                None,
            ),
            Unique::Where(e) => (vec![a.clone()], Some(e)),
            // unique within the scope its identifier numbers by
            Unique::InScope => (
                std::iter::once(a.clone())
                    .chain(spec.identifier.as_ref().and_then(|i| i.scope.clone()))
                    .collect(),
                None,
            ),
        };
        let key = |obj: &Object| -> Result<Option<Vec<Value>>, NotRun> {
            let values: Vec<Value> = with.iter().map(|w| obj.attr(w)).collect();
            if values.iter().any(Value::is_absent) {
                return Ok(None);
            }
            if let Some(f) = filter {
                let ot = world.lib.type_(&obj.type_);
                if truth(
                    &Evaluator::new(world, ot, Some(&obj.id))
                        .filtering()
                        .eval(f)?,
                )? != Some(true)
                {
                    return Ok(None);
                }
            }
            Ok(Some(values))
        };
        let Some(mine) = key(o)? else { continue };
        // unique across the family of the type declaring it
        let scope = world.lib.unique_scope(t, a);
        for other in world.of_type(scope) {
            if other.id != o.id
                && key(other)?.is_some_and(|k| k.iter().zip(&mine).all(|(x, y)| x.equals(y)))
            {
                out.push(format!("{a}_unique"));
                break;
            }
        }
    }
    Ok(out)
}

/// The bare names an expression reads at the start of a path, a loop's own
/// binder excepted.
pub(crate) fn names_read(e: &Expr, out: &mut BTreeSet<String>) {
    match e {
        Expr::Name(n) => {
            out.insert(n.clone());
        }
        Expr::Path(b, _) => names_read(b, out),
        Expr::Agg {
            var,
            collection,
            filter,
            body,
            ..
        } => {
            if let Some(c) = collection {
                names_read(c, out);
            }
            let mut inner = BTreeSet::new();
            for x in [filter, body].into_iter().flatten() {
                names_read(x, &mut inner);
            }
            if let Some(v) = var {
                inner.remove(v);
            }
            out.extend(inner);
        }
        Expr::Call(f, args) => {
            names_read(f, out);
            for a in args {
                names_read(a, out);
            }
        }
        Expr::Metric { name, binds, .. } => {
            names_read(name, out);
            for (_, b) in binds {
                names_read(b, out);
            }
        }
        Expr::Changed(_, e) | Expr::Not(e) | Expr::Neg(e) | Expr::IsNull(e, _) => {
            names_read(e, out)
        }
        Expr::Set(items) => items.iter().for_each(|i| names_read(i, out)),
        Expr::In(l, r) | Expr::Bin(_, l, r) => {
            names_read(l, out);
            names_read(r, out);
        }
        Expr::If(c, a, b) => {
            names_read(c, out);
            names_read(a, out);
            names_read(b, out);
        }
        Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Dur(..) | Expr::Money(..) => {}
    }
}

/// The remedy `DESIGN.md` §5.5 infers for an invariant that fails, as the
/// runner infers it: `self_serviceable` where the transition takes an input
/// it writes to a value the invariant reads; else `dependent` where the
/// invariant reads another object; else `unreachable_from_here`. `None` where
/// it cannot be told: an invariant on another object, or one reading `this`.
fn invariant_remedy(
    world: &World<'_>,
    x: &Transition,
    obj: &Object,
    n: &str,
    requested: &str,
) -> Option<Remedy> {
    let lib: &Library = world.lib;
    let mine = world
        .get(requested)
        .map(|o| o.type_.clone())
        .unwrap_or_default();
    if obj.id != requested && !(n.ends_with("_unique") && mine == obj.type_) {
        return None;
    }
    let t = lib.type_(&obj.type_)?;
    let (reads, other): (BTreeSet<String>, bool) = if let Some(a) = n.strip_suffix("_unique") {
        let mut reads = BTreeSet::from([a.to_string()]);
        if let Some(Unique::With(others)) = t.attribute(a).map(|s| &s.unique) {
            reads.extend(others.iter().cloned());
        }
        (reads, true)
    } else if let Some(s) = n.strip_suffix("_invariant") {
        let state = t
            .states
            .iter()
            .find(|(name, _)| name.to_lowercase() == s)
            .map(|(_, st)| st);
        let mut reads: BTreeSet<String> = state
            .map(|st| st.required_attributes.iter().cloned().collect())
            .unwrap_or_default();
        reads.insert("state".into());
        (reads, false)
    } else {
        let e = t
            .invariants
            .iter()
            .find(|(name, _)| name == n)
            .map(|(_, e)| e)?;
        let mut names = BTreeSet::new();
        names_read(e, &mut names);
        let (mut reads, mut other, mut seen) = (BTreeSet::new(), false, BTreeSet::new());
        let mut todo: Vec<String> = names.into_iter().collect();
        while let Some(m) = todo.pop() {
            if !seen.insert(m.clone()) {
                continue;
            }
            if m == "this" {
                return None;
            }
            if let Some((_, d)) = t.derived.iter().find(|(name, _)| *name == m) {
                let mut more = BTreeSet::new();
                names_read(d, &mut more);
                todo.extend(more);
                continue;
            }
            let attr = t.attribute(&m);
            if m == "state" || attr.is_some() {
                reads.insert(m.clone());
            }
            other |= attr.is_some_and(|a| matches!(a.declared, Declared::Reference(_)))
                || m == "referrers"
                || (lib.types.contains_key(&m) && t.state(&m).is_none());
        }
        (reads, other)
    };
    let mut writes: BTreeSet<String> = x
        .required_inputs
        .iter()
        .chain(&x.optional_inputs)
        .cloned()
        .collect();
    // an effect's step that writes a value computed from an input writes
    // that input to it (ADR-0122 decisions 33 and 36, DESIGN.md §5.5)
    fn from_inputs(steps: &[Step], writes: &mut BTreeSet<String>) {
        for st in steps {
            match st {
                Step::Assign { location, expr }
                | Step::Add { location, expr }
                | Step::Remove { location, expr } => {
                    let mut read = BTreeSet::new();
                    names_read(expr, &mut read);
                    if read.contains("inputs") {
                        writes.insert(location.clone());
                    }
                }
                Step::Foreach { steps, .. } => from_inputs(steps, writes),
                _ => {}
            }
        }
    }
    from_inputs(&x.effect, &mut writes);
    if reads.iter().any(|r| writes.contains(r)) {
        Some(Remedy::SelfServiceable)
    } else if other {
        Some(Remedy::Dependent)
    } else {
        Some(Remedy::UnreachableFromHere)
    }
}

/// A request run against a store, in one transaction: its verdict, or that
/// the first slice does not run what it reaches.
pub fn request(
    store: &mut crate::store::Store,
    req: &Request,
) -> Result<Result<Verdict, NotRun>, crate::store::StoreError> {
    let answers = store.answers.clone();
    let decided = store.transact(|world, alloc| {
        decide(world, alloc, req, &answers).map(|(v, change, r)| ((v, r), change))
    })?;
    let (verdict, refusal) = match decided {
        Ok(decided) => decided,
        Err(e) => return Ok(Err(e)),
    };
    // a request that did not apply is a row of the attempt log, written in
    // its own short transaction after the rollback (DESIGN.md §6)
    if !matches!(verdict, Verdict::Satisfied { .. }) {
        let attempt = {
            let world = store.snapshot()?;
            refused_attempt(&world, req, &verdict, &refusal)
        };
        store.record_attempt(&attempt)?;
    }
    Ok(Ok(verdict))
}

/// What `check` answers (`library-api.md` §4): the verdict the request
/// would receive, and the guards it did not evaluate, so the verdict is
/// explicitly partial (ADR-0054).
#[derive(Clone, Debug, PartialEq)]
pub struct Checked {
    pub verdict: Verdict,
    pub unevaluated: Vec<String>,
}

/// The verdict a request would receive (`DESIGN.md` §10, ADR-0054),
/// simulating the same sequence on the store as it stands: lock-free and
/// side-effect-free, it writes nothing, not a row of the attempt log nor the
/// numbers it drew, and it asks no evaluator, naming each guard that would
/// have; a creation is checked given its type (ADR-0099).
pub fn check(
    store: &mut crate::store::Store,
    req: &Request,
) -> Result<Result<Checked, NotRun>, crate::store::StoreError> {
    let named = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let checking = Some(named.clone());
    let decided =
        store.simulate(|world, alloc| decide_as(world, alloc, req, &BTreeMap::new(), &checking))?;
    Ok(decided.map(|(verdict, _, _)| Checked {
        verdict,
        unevaluated: named.borrow().clone(),
    }))
}

/// Everything a request had read when refused, the objects its outcome and
/// cascades reached included, at the versions it left them (`DESIGN.md` §7).
fn refused_reads(ctx: &Ctx<'_>) -> Json {
    let mut read = ctx.read.clone();
    for e in &ctx.events {
        if let Some(o) = ctx.work.get(&e.object_id) {
            read.objects.insert(o.id.clone(), o.version);
        }
    }
    read.record(ctx.work.now, None)
}

/// A refused request's row of the attempt log (`DESIGN.md` §6, §9): who
/// asked, as the request named them where the store holds no such actor,
/// and their kind, `unknown` then; the type, none for an unknown id; the
/// object, none for a refused creation or an unknown id; the transition; the
/// verdict, its clause, the first invariant for violated invariants, and its
/// remedy; whether the clause was unknown; and the request's values, a
/// personal input named and withheld (ADR-0105). An approval its request
/// refused is a row of the proposed transition, on its object, naming the
/// proposal.
fn refused_attempt(
    world: &World<'_>,
    req: &Request,
    verdict: &Verdict,
    refusal: &Refusal,
) -> crate::store::Attempt {
    let lib = world.lib;
    let (asked, proposal) = match refusal.approving.as_deref() {
        Some((p, held)) => (held, Some(p.clone())),
        None => (req, None),
    };
    let (type_, object_id) = match &asked.object_id {
        Some(id) => match world.get(id) {
            Some(o) => (Some(o.type_.clone()), Some(id.clone())),
            None => (None, None),
        },
        None => (asked.type_.clone(), None),
    };
    let clause = match verdict {
        Verdict::Unsatisfied { clause, .. } => Some(clause.clone()),
        Verdict::InvariantViolated { failures, .. } => failures.first().map(|f| {
            if type_.as_deref() == Some(f.type_.as_str()) {
                f.invariant.clone()
            } else {
                format!("{}.{}", f.type_, f.invariant)
            }
        }),
        // a refused call's own clause, so the causes of one clause count
        // together under the transition requested (metric-scenarios.md §12)
        Verdict::CallRefused { clause, .. } => clause
            .as_deref()
            .map(|c| c.rsplit('.').next().unwrap_or(c).to_string()),
        _ => None,
    };
    let t = type_.as_deref().and_then(|n| lib.type_(n));
    let x = t.and_then(|t| t.transition(&asked.transition));
    let personal = |k: &str| {
        x.and_then(|x| x.inputs.iter().find(|(n, _)| n == k))
            .map(|(_, i)| i.personal)
            .or_else(|| t.and_then(|t| t.attribute(k)).map(|a| a.personal))
            .unwrap_or(false)
    };
    let mut values = serde_json::Map::new();
    let mut withheld = Vec::new();
    for (k, v) in &asked.inputs {
        if personal(k) {
            withheld.push(k.clone());
        } else {
            values.insert(k.clone(), v.clone());
        }
    }
    let mut request_values = json!({ "inputs": values });
    if let Some(at) = asked.occurred_at {
        request_values["occurred_at"] = json!(crate::value::timestamp_text(at));
    }
    if let Some(p) = proposal {
        request_values["proposal"] = json!(p);
    }
    let who = resolve_actor(world, &req.actor);
    crate::store::Attempt {
        at: world.now,
        actor_id: who
            .as_ref()
            .map_or_else(|| req.actor.clone(), |w| w.id.clone()),
        actor_kind: who.map_or_else(|| "unknown".to_string(), |w| w.kind),
        // the principal as resolved, or as named where the store holds none
        principal: req
            .principal
            .as_ref()
            .map(|p| resolve_actor(world, p).map_or_else(|| p.clone(), |w| w.id)),
        type_,
        object_id,
        transition: Some(asked.transition.clone()),
        verdict: verdict.kind().to_string(),
        clause,
        remedy: verdict
            .remedy()
            .map_or_else(String::new, |r| r.text().to_string()),
        unknown: refusal.unknown,
        enforced: true,
        flagged: false,
        applied_position: None,
        request_values,
        withheld,
        declaration_version: lib.version,
        reads: refusal.reads.clone(),
        consulted: refusal.consulted.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OPERATOR;
    use crate::store::Store;

    const JOBS: &str = r#"
module: jobs
categories: [inbound, live, closed]
enumerations: { Priority: [LOW, HIGH] }
types:
  User:
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states: { ACTIVE: { category: live } }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [login] }
  Job:
    attributes:
      title: { type: string }
      priority: { type: Priority, optional: true }
      hours: { type: int, optional: true }
      note: { type: string, optional: true }
      engineer: { reference: User, optional: true }
    states:
      OPEN: { category: inbound }
      WORKING: { category: live, required_attributes: [engineer] }
      DONE: { category: closed, final: true }
    conditions:
      small: { expression: inputs.hours is null or inputs.hours < 10, remedy: self_serviceable }
      prioritised: { expression: priority is not null, remedy: self_serviceable }
    invariants:
      bounded: { expression: hours is null or hours >= 0 }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [title], optional_inputs: [priority, hours, note] }
      start: { kind: external, from: [OPEN], to: WORKING, optional_inputs: [engineer], guards: { small: deny, prioritised: warn } }
      estimate: { kind: internal, from: any, required_inputs: [hours], guards: { small: deny } }
      annotate: { kind: internal, from: [OPEN], required_inputs: [note] }
      finish: { kind: external, from: [WORKING], to: DONE }
      archive: { kind: internal, from: [DONE], only_via: [finish] }
"#;

    fn store() -> Store {
        let lib = Library::load(&[("jobs.yaml".into(), JOBS.into())]).unwrap();
        Store::sandbox(lib, 1_767_225_600).unwrap()
    }

    fn req(s: &mut Store, xn: &str, actor: &str, target: (&str, bool), inputs: Json) -> Verdict {
        let (name, is_type) = target;
        let r = Request {
            transition: xn.into(),
            actor: actor.into(),
            object_id: (!is_type).then(|| name.to_string()),
            type_: is_type.then(|| name.to_string()),
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
        request(s, &r).unwrap().unwrap()
    }

    fn created(v: &Verdict) -> String {
        match v {
            Verdict::Satisfied { object_id, .. } => object_id.clone(),
            other => panic!("{other:?}"),
        }
    }

    /// The optimistic check of ADR-0023 (`DESIGN.md` §6 step 3): a request
    /// naming the version of its object its caller read is refused as
    /// `stale` where the object has moved, after its transition is found and
    /// before it is found not requestable or unavailable, in each kind of
    /// request; and a refusal is a row of the attempt log.
    #[test]
    fn a_request_naming_a_moved_version_is_stale() {
        let text = JOBS.replace(
            "      finish: { kind: external, from: [WORKING], to: DONE }",
            "      finish: { kind: external, from: [WORKING], to: DONE, proposable: true }",
        );
        let lib = Library::load(&[("jobs.yaml".into(), text)]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let ask = |s: &mut Store, xn: &str, object: &str, version: Option<i64>, propose: bool| {
            let r = Request {
                transition: xn.into(),
                actor: OPERATOR.into(),
                object_id: Some(object.to_string()),
                type_: None,
                inputs: if xn == "estimate" {
                    [("hours".to_string(), json!(3))].into()
                } else {
                    BTreeMap::new()
                },
                occurred_at: None,
                propose,
                expires_at: None,
                expected_version: version,
                principal: None,
            };
            request(s, &r).unwrap().unwrap()
        };
        // a creation names no object, and so no version to find moved
        let creation = Request {
            transition: "open".into(),
            actor: OPERATOR.into(),
            object_id: None,
            type_: Some("Job".into()),
            inputs: [("title".to_string(), json!("Fix"))].into(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: Some(7),
            principal: None,
        };
        let job = created(&request(&mut s, &creation).unwrap().unwrap());
        // the version read applies, and moves the object on
        let Verdict::Satisfied { version, .. } = ask(&mut s, "estimate", &job, Some(1), false)
        else {
            panic!("the version read applies")
        };
        assert_eq!(version, 2);
        let moved = Verdict::Stale {
            expected: 1,
            actual: 2,
        };
        assert_eq!(ask(&mut s, "estimate", &job, Some(1), false), moved);
        // after an unknown transition, and before not requestable and
        // unavailable
        assert_eq!(
            ask(&mut s, "nonesuch", &job, Some(1), false),
            Verdict::UnknownTransition {
                transition: "nonesuch".into()
            }
        );
        assert_eq!(ask(&mut s, "archive", &job, Some(1), false), moved);
        assert_eq!(ask(&mut s, "finish", &job, Some(1), false), moved);
        // a proposal filed against a moved version, and a decision on a
        // proposal its caller read before it moved
        assert_eq!(ask(&mut s, "finish", &job, Some(1), true), moved);
        let p = created(&ask(&mut s, "finish", &job, Some(2), true));
        assert_eq!(
            ask(&mut s, "reject", &p, Some(0), false),
            Verdict::Stale {
                expected: 0,
                actual: 1
            }
        );
        assert_eq!(
            ask(&mut s, "invalidate", &p, Some(0), false),
            Verdict::Stale {
                expected: 0,
                actual: 1
            }
        );
        // and a change to the declaration, its transitions and a publish's
        let draft = Request {
            transition: "draft".into(),
            actor: OPERATOR.into(),
            object_id: None,
            type_: Some(crate::model::DECLARATION_CHANGE.into()),
            inputs: [
                ("source".to_string(), json!({ "jobs": JOBS })),
                ("source_ref".to_string(), json!("git:jobs@2")),
            ]
            .into(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            principal: None,
        };
        let change = created(&request(&mut s, &draft).unwrap().unwrap());
        let read_before = Verdict::Stale {
            expected: 0,
            actual: 1,
        };
        assert_eq!(
            ask(&mut s, "withdraw", &change, Some(0), false),
            read_before
        );
        assert_eq!(
            ask(&mut s, "supersede", &change, Some(0), false),
            read_before
        );
        // each refusal is a row of the attempt log, its remedy re-reading
        let w = s.snapshot().unwrap();
        let stale: Vec<_> = w
            .attempts
            .iter()
            .filter(|a| a.verdict == "stale")
            .map(|a| (a.clause.clone(), a.remedy.clone()))
            .collect();
        assert_eq!(stale.len(), 8);
        assert!(
            stale
                .iter()
                .all(|(c, r)| c.is_none() && r == "self_serviceable")
        );
    }

    /// A refused call's attempt row is a row of the transition requested,
    /// carrying the call's own clause, unqualified, so a call's
    /// `occurred_within` and the request's own count together
    /// (`declaration-syntax.md` §6.9, `metric-scenarios.md` §12).
    #[test]
    fn a_refused_call_is_a_row_of_the_request_with_the_call_s_clause() {
        const SHOP: &str = r#"
module: shop
categories: [live, closed]
types:
  Line:
    attributes:
      qty: { type: int }
    states:
      OPEN: { category: live }
      PACKED: { category: closed, final: true }
    conditions:
      small: { expression: qty < 10, remedy: self_serviceable }
    transitions:
      add: { kind: initial, to: OPEN, required_inputs: [qty] }
      pack: { kind: external, from: [OPEN], to: PACKED, guards: { small: deny } }
  Order:
    attributes:
      line: { reference: Line }
    states:
      OPEN: { category: live }
      DONE: { category: closed, final: true }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [line] }
      ship: { kind: external, from: [OPEN], to: DONE, effect: [ { call: { target: line, transition: pack } } ] }
"#;
        let lib = Library::load(&[("shop.yaml".into(), SHOP.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let line = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("Line", true),
            json!({"qty": 20}),
        ));
        let order = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Order", true),
            json!({"line": line}),
        ));
        let v = req(&mut s, "ship", OPERATOR, (&order, false), json!({}));
        let Verdict::CallRefused { clause, .. } = &v else {
            panic!("{v:?}")
        };
        // the verdict names the call's clause qualified, as an expectation does
        assert_eq!(clause.as_deref(), Some("Line.small"));
        // a call that applies records its caller's event as its cause
        let fine = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("Line", true),
            json!({"qty": 3}),
        ));
        let shipped = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Order", true),
            json!({"line": fine}),
        ));
        let Verdict::Satisfied { events, .. } =
            req(&mut s, "ship", OPERATOR, (&shipped, false), json!({}))
        else {
            panic!("a small line ships")
        };
        let evs: Vec<_> = s.events(&events).unwrap().into_iter().flatten().collect();
        assert_eq!(
            evs.iter()
                .map(|e| (e.transition.as_str(), e.cause))
                .collect::<Vec<_>>(),
            [("ship", None), ("pack", Some(evs[0].position))]
        );
        let w = s.snapshot().unwrap();
        let a = w.attempts.last().unwrap();
        assert_eq!(
            (
                a.type_.as_deref(),
                a.transition.as_deref(),
                a.verdict.as_str(),
                a.clause.as_deref(),
                a.remedy.as_str()
            ),
            (
                Some("Order"),
                Some("ship"),
                "call_refused",
                Some("small"),
                "self_serviceable"
            )
        );
    }

    /// A request's principal (`DESIGN.md` §5.8, §6 step 1): resolved as
    /// the actor is, refused as `actor_known` where it names no actor,
    /// recorded on the event and the attempt row, and readable in an outcome
    /// as `actor.principal`, which no rule reads.
    #[test]
    fn a_principal_is_resolved_recorded_and_written_by_an_outcome() {
        let text = JOBS
            .replace(
                "      note: { type: string, optional: true }",
                "      note: { type: string, optional: true }\n      for_whom: { type: string, optional: true }",
            )
            .replace(
                "      finish: { kind: external, from: [WORKING], to: DONE }",
                "      finish: { kind: external, from: [WORKING], to: DONE }\n      \
                 sign: { kind: internal, from: any, effect: [ { assign: { location: for_whom, expr: actor.principal } } ] }",
            );
        let lib = Library::load(&[("jobs.yaml".into(), text)]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let ana = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ));
        let job = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"title": "Fix"}),
        ));
        let sign = |s: &mut Store, principal: Option<&str>| {
            let r = Request {
                transition: "sign".into(),
                actor: OPERATOR.into(),
                object_id: Some(job.clone()),
                type_: None,
                inputs: BTreeMap::new(),
                occurred_at: None,
                propose: false,
                expires_at: None,
                expected_version: None,
                principal: principal.map(str::to_string),
            };
            request(s, &r).unwrap().unwrap()
        };
        // named by its object's id or by the identity it holds, recorded by
        // the identity, and written by the outcome
        for named in [ana.as_str(), "ana"] {
            created(&sign(&mut s, Some(named)));
            let w = s.snapshot().unwrap();
            assert_eq!(
                w.get(&job).unwrap().attr("for_whom"),
                Value::Text("ana".into())
            );
            let last = s.history(&job).unwrap().last().cloned().unwrap();
            assert_eq!(last.principal.as_deref(), Some("ana"));
        }
        // none named, none recorded; an outcome assigning `actor.principal`
        // then would assign an absent value, which neither runner runs
        created(&req(
            &mut s,
            "estimate",
            OPERATOR,
            (&job, false),
            json!({"hours": 2}),
        ));
        assert_eq!(s.history(&job).unwrap().last().unwrap().principal, None);
        // a principal the store holds no actor for is refused as actor_known,
        // its attempt row keeping it as named
        assert_eq!(
            sign(&mut s, Some("nobody")),
            Verdict::Unsatisfied {
                clause: "actor_known".into(),
                remedy: Some(Remedy::Dependent),
            }
        );
        let w = s.snapshot().unwrap();
        let a = w.attempts.last().unwrap();
        assert_eq!(
            (
                a.clause.as_deref(),
                a.actor_kind.as_str(),
                a.principal.as_deref()
            ),
            (Some("actor_known"), "human", Some("nobody"))
        );
    }

    /// `check` (`DESIGN.md` §10, ADR-0054): the verdict a request would
    /// receive, its effects and calls simulated, written nowhere, not even
    /// the numbers it drew, and asking no evaluator, naming each guard that
    /// would have.
    #[test]
    fn a_check_simulates_writes_nothing_and_names_what_it_did_not_ask() {
        const SHOP: &str = r#"
module: shop
categories: [live, closed]
evaluators:
  vault:
    functions:
      cleared: { arguments: { n: { type: int } } }
types:
  Line:
    attributes:
      qty: { type: int }
    states:
      OPEN: { category: live }
      PACKED: { category: closed, final: true }
    conditions:
      small: { expression: qty < 10, remedy: self_serviceable }
    transitions:
      add: { kind: initial, to: OPEN, required_inputs: [qty] }
      pack: { kind: external, from: [OPEN], to: PACKED, guards: { small: deny } }
  Order:
    attributes:
      line: { reference: Line }
      n: { type: int }
    states:
      OPEN: { category: live }
      DONE: { category: closed, final: true }
    conditions:
      cleared: { expression: vault.cleared(n), remedy: dependent }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [line, n] }
      ship: { kind: external, from: [OPEN], to: DONE, guards: { cleared: deny }, effect: [ { call: { target: line, transition: pack } } ] }
"#;
        let lib = Library::load(&[("shop.yaml".into(), SHOP.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        // the evaluator would refuse, if it were asked
        s.answers
            .insert("vault.cleared".into(), "unsatisfied".into());
        let big = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("Line", true),
            json!({"qty": 20}),
        ));
        let small = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("Line", true),
            json!({"qty": 3}),
        ));
        let order = |s: &mut Store, line: &str| {
            created(&req(
                s,
                "open",
                OPERATOR,
                ("Order", true),
                json!({"line": line, "n": 1}),
            ))
        };
        let refused = order(&mut s, &big);
        let fine = order(&mut s, &small);
        let ship = |object: &str| Request {
            transition: "ship".into(),
            actor: OPERATOR.into(),
            object_id: Some(object.to_string()),
            type_: None,
            inputs: BTreeMap::new(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            principal: None,
        };
        let before = |s: &Store| {
            let w = s.snapshot().unwrap();
            (
                w.events.len(),
                w.attempts.len(),
                w.get(&fine).unwrap().version,
            )
        };
        let was = before(&s);
        // the call its effect makes is refused, and the evaluator guard is
        // named, not asked
        let c = check(&mut s, &ship(&refused)).unwrap().unwrap();
        assert!(
            matches!(&c.verdict, Verdict::CallRefused { clause, .. } if clause.as_deref() == Some("Line.small")),
            "{c:?}"
        );
        assert_eq!(c.unevaluated, ["cleared"]);
        // one that would apply, its verdict as the request's would be
        let c = check(&mut s, &ship(&fine)).unwrap().unwrap();
        assert!(matches!(c.verdict, Verdict::Satisfied { .. }), "{c:?}");
        assert_eq!(c.unevaluated, ["cleared"]);
        // nothing written: no event, no attempt row, no version moved
        assert_eq!(before(&s), was);
        // the request itself asks the evaluator, which refuses
        assert_eq!(
            request(&mut s, &ship(&fine)).unwrap().unwrap(),
            Verdict::Unsatisfied {
                clause: "cleared".into(),
                remedy: Some(Remedy::Dependent)
            }
        );
        // a creation is checked given its type, and the id it would draw is
        // drawn again by the creation itself
        let add = Request {
            transition: "add".into(),
            actor: OPERATOR.into(),
            object_id: None,
            type_: Some("Line".into()),
            inputs: [("qty".to_string(), json!(4))].into(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            principal: None,
        };
        let Verdict::Satisfied {
            object_id: would, ..
        } = check(&mut s, &add).unwrap().unwrap().verdict
        else {
            panic!("a creation is checked")
        };
        assert_eq!(created(&request(&mut s, &add).unwrap().unwrap()), would);
    }

    #[test]
    fn the_order_of_refusals_is_designs() {
        let mut s = store();
        let ana = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ));
        let job = created(&req(
            &mut s,
            "open",
            "ana",
            ("Job", true),
            json!({"title": "Fix"}),
        ));
        // 1. an actor the store does not hold
        assert_eq!(
            req(&mut s, "start", "nobody", (&job, false), json!({})),
            Verdict::Unsatisfied {
                clause: "actor_known".into(),
                remedy: Some(Remedy::Dependent)
            }
        );
        // 2. an object, a transition
        assert_eq!(
            req(&mut s, "start", OPERATOR, ("job-99", false), json!({})),
            Verdict::NotFound
        );
        assert!(matches!(
            req(&mut s, "fly", OPERATOR, (&job, false), json!({})),
            Verdict::UnknownTransition { .. }
        ));
        assert!(matches!(
            req(&mut s, "open", OPERATOR, (&job, false), json!({})),
            Verdict::UnknownTransition { .. }
        ));
        // 3. only via others; a state it is not taken from
        assert_eq!(
            req(&mut s, "archive", OPERATOR, (&job, false), json!({})),
            Verdict::NotRequestable {
                parents: vec!["finish".into()]
            }
        );
        assert_eq!(
            req(&mut s, "finish", OPERATOR, (&job, false), json!({})),
            Verdict::Unavailable {
                state: "OPEN".into()
            }
        );
        // 4. inputs: a name it does not take, a value not of its type, an
        // object of another type, a required input left out
        assert_eq!(
            req(
                &mut s,
                "start",
                OPERATOR,
                (&job, false),
                json!({"colour": "red"})
            ),
            Verdict::InvalidInput {
                inputs: vec!["colour".into()]
            }
        );
        assert_eq!(
            req(
                &mut s,
                "estimate",
                OPERATOR,
                (&job, false),
                json!({"hours": "many"})
            ),
            Verdict::InvalidInput {
                inputs: vec!["hours".into()]
            }
        );
        assert_eq!(
            req(
                &mut s,
                "start",
                OPERATOR,
                (&job, false),
                json!({"engineer": &job})
            ),
            Verdict::InvalidInput {
                inputs: vec!["engineer".into()]
            }
        );
        assert_eq!(
            req(&mut s, "open", OPERATOR, ("Job", true), json!({})),
            Verdict::InvalidInput {
                inputs: vec!["title".into()]
            }
        );
        // a required input of an optional attribute, left out, is the
        // generated guard's (flow-format.md §4.8)
        assert_eq!(
            req(&mut s, "estimate", OPERATOR, (&job, false), json!({})),
            Verdict::Unsatisfied {
                clause: "hours_provided".into(),
                remedy: Some(Remedy::SelfServiceable)
            }
        );
        // 5. a guard that denies, with its declared remedy
        assert_eq!(
            req(
                &mut s,
                "estimate",
                OPERATOR,
                (&job, false),
                json!({"hours": 12})
            ),
            Verdict::Unsatisfied {
                clause: "small".into(),
                remedy: Some(Remedy::SelfServiceable)
            }
        );
        // 7. an invariant, the remedy self-serviceable since the input writes
        // what it reads
        match req(
            &mut s,
            "estimate",
            OPERATOR,
            (&job, false),
            json!({"hours": -1}),
        ) {
            Verdict::InvariantViolated { failures, remedy } => {
                assert_eq!(failures[0].invariant, "bounded");
                assert_eq!(failures[0].type_, "Job");
                assert_eq!(remedy, Some(Remedy::SelfServiceable));
            }
            other => panic!("{other:?}"),
        }
        // a state's required attribute, left out
        match req(&mut s, "start", OPERATOR, (&job, false), json!({})) {
            Verdict::InvariantViolated { failures, .. } => {
                assert_eq!(failures[0].invariant, "working_invariant")
            }
            other => panic!("{other:?}"),
        }
        // and the request that applies: a warning told, the state and the
        // input written, one event
        match req(
            &mut s,
            "start",
            "ana",
            (&job, false),
            json!({"engineer": &ana}),
        ) {
            Verdict::Satisfied {
                object_id,
                version,
                events,
                flags,
            } => {
                assert_eq!(object_id, job);
                assert_eq!(version, 2);
                assert_eq!(events.len(), 1);
                assert_eq!(
                    flags,
                    vec![Flag {
                        clause: "prioritised".into(),
                        remedy: Some(Remedy::SelfServiceable)
                    }]
                );
            }
            other => panic!("{other:?}"),
        }
        let world = s.snapshot().unwrap();
        let j = world.get(&job).unwrap();
        assert_eq!(j.state, "WORKING");
        assert_eq!(j.attr("engineer"), Value::Object(ana.clone()));
        let h = s.history(&job).unwrap();
        assert_eq!(
            h.iter().map(|e| e.transition.as_str()).collect::<Vec<_>>(),
            ["open", "start"]
        );
        assert_eq!(h[1].from_state.as_deref(), Some("OPEN"));
        assert_eq!(h[1].actor_id, "ana");
    }

    #[test]
    fn a_refused_request_writes_nothing_and_allocates_nothing() {
        let mut s = store();
        created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ));
        let before = s.snapshot().unwrap().objects.len();
        assert!(matches!(
            req(
                &mut s,
                "add",
                OPERATOR,
                ("User", true),
                json!({"login": "ana"})
            ),
            Verdict::InvariantViolated { .. }
        ));
        assert_eq!(s.snapshot().unwrap().objects.len(), before);
        let next = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ben"}),
        ));
        assert_eq!(next, "user-2");
    }

    const ORDERS: &str = r#"
module: orders
categories: [live, closed]
sequences:
  order_number: { description: Orders. }
  line_number: { description: Lines, per order. }
types:
  Order:
    attributes:
      number: { type: string, identifier: { sequence: order_number, format: "ORD-{n:4}" }, unique: true }
      region: { type: string, optional: true }
      code: { type: string, identifier: { sequence: line_number, format: "{number}[-{region}]" } }
      cap: { type: int, optional: true }
    states: { OPEN: { category: live } }
    invariants:
      capped: { expression: cap is null or cap < 5 }
    transitions:
      open: { kind: initial, to: OPEN, optional_inputs: [region, cap] }
  Line:
    attributes:
      order: { reference: Order }
      position: { type: string, identifier: { sequence: line_number, scope: order, format: "{order.number}-{n:2}" }, unique: in_scope }
    states: { LIVE: { category: live } }
    transitions:
      add: { kind: initial, to: LIVE, required_inputs: [order] }
"#;

    #[test]
    fn an_identifier_is_minted_as_its_format_writes_it() {
        let lib = Library::load(&[("orders.yaml".into(), ORDERS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let a = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Order", true),
            json!({"region": "EU"}),
        ));
        let b = created(&req(&mut s, "open", OPERATOR, ("Order", true), json!({})));
        let w = s.snapshot().unwrap();
        assert_eq!(
            w.get(&a).unwrap().attr("number"),
            Value::Text("ORD-0001".into())
        );
        // a field read from the object, and a segment left out whole where
        // its value is absent
        assert_eq!(
            w.get(&a).unwrap().attr("code"),
            Value::Text("ORD-0001-EU".into())
        );
        assert_eq!(
            w.get(&b).unwrap().attr("code"),
            Value::Text("ORD-0002".into())
        );
        // a sequence numbered per scope
        let l1 = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("Line", true),
            json!({"order": &a}),
        ));
        let l2 = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("Line", true),
            json!({"order": &a}),
        ));
        let l3 = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("Line", true),
            json!({"order": &b}),
        ));
        let w = s.snapshot().unwrap();
        let pos = |id: &str| w.get(id).unwrap().attr("position");
        assert_eq!(pos(&l1), Value::Text("ORD-0001-01".into()));
        assert_eq!(pos(&l2), Value::Text("ORD-0001-02".into()));
        assert_eq!(pos(&l3), Value::Text("ORD-0002-01".into()));
    }

    #[test]
    fn a_number_drawn_by_a_refused_request_is_not_reused() {
        let lib = Library::load(&[("orders.yaml".into(), ORDERS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        created(&req(&mut s, "open", OPERATOR, ("Order", true), json!({})));
        assert!(matches!(
            req(&mut s, "open", OPERATOR, ("Order", true), json!({"cap": 9})),
            Verdict::InvariantViolated { .. }
        ));
        let c = created(&req(&mut s, "open", OPERATOR, ("Order", true), json!({})));
        let w = s.snapshot().unwrap();
        assert_eq!(
            w.get(&c).unwrap().attr("number"),
            Value::Text("ORD-0003".into())
        );
    }

    #[test]
    fn a_uniqueness_names_its_failure() {
        let mut s = store();
        created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ));
        match req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ) {
            Verdict::InvariantViolated { failures, remedy } => {
                assert_eq!(failures[0].invariant, "login_unique");
                assert_eq!(remedy, Some(Remedy::SelfServiceable));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_effect_writes_and_a_supersession_records_its_successor() {
        let text = JOBS.replace(
            "      finish: { kind: external, from: [WORKING], to: DONE }",
            "      finish: { kind: external, from: [WORKING], to: DONE, effect: [ { assign: { location: note, expr: '\"done\"' } }, { clear: [hours] } ] }\n      \
             retire: { kind: external, from: [DONE], to: RETIRED, effect: [ { supersede: engineer } ] }",
        );
        let text = text.replace(
            "      DONE: { category: closed, final: true }",
            "      DONE: { category: closed }\n      RETIRED: { category: closed, final: true, superseding: true }",
        );
        let lib = Library::load(&[("jobs.yaml".into(), text)]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let job = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"title": "Fix", "hours": 3}),
        ));
        // a refusal before the effect is still decided
        assert_eq!(
            req(&mut s, "finish", OPERATOR, (&job, false), json!({})),
            Verdict::Unavailable {
                state: "OPEN".into()
            }
        );
        let ana = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ));
        created(&req(
            &mut s,
            "start",
            OPERATOR,
            (&job, false),
            json!({"engineer": &ana}),
        ));
        // an effect's writes, in order
        created(&req(&mut s, "finish", OPERATOR, (&job, false), json!({})));
        let w = s.snapshot().unwrap();
        let j = w.get(&job).unwrap();
        assert_eq!(j.state, "DONE");
        assert_eq!(j.attr("note"), Value::Text("done".into()));
        assert_eq!(j.attr("hours"), Value::Absent);
        // a supersession records its successor
        created(&req(&mut s, "retire", OPERATOR, (&job, false), json!({})));
        let w = s.snapshot().unwrap();
        assert_eq!(
            w.get(&job).unwrap().superseded_by.as_deref(),
            Some(ana.as_str())
        );
    }

    #[test]
    fn intervals_are_opened_ended_and_read_back() {
        let mut s = store();
        let t0 = s.now;
        let job = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"title": "Fix", "priority": "LOW"}),
        ));
        s.now += 3600;
        let ana = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ));
        created(&req(
            &mut s,
            "start",
            OPERATOR,
            (&job, false),
            json!({"engineer": &ana}),
        ));
        // a write that changes no tracked value opens nothing
        created(&req(
            &mut s,
            "estimate",
            OPERATOR,
            (&job, false),
            json!({"hours": 3}),
        ));
        s.now += 7200;
        created(&req(&mut s, "finish", OPERATOR, (&job, false), json!({})));
        s.now += 600;
        // read back from the store, as the next request reads them
        let w = s.snapshot().unwrap();
        let o = w.get(&job).unwrap();
        let counts: Vec<(&str, usize)> =
            o.spans.iter().map(|(m, v)| (m.as_str(), v.len())).collect();
        assert_eq!(counts, vec![("engineer", 2), ("priority", 1), ("state", 3)]);
        let t = w.lib.type_("Job");
        let at = |e: &str| {
            Evaluator::new(&w, t, Some(&job))
                .eval(&crate::expr::parse(e).unwrap())
                .unwrap()
        };
        assert_eq!(
            at("count(i in this.intervals where i.state == Job.WORKING)"),
            Value::Int(1)
        );
        assert_eq!(at("time_in(WORKING)"), Value::Duration(7200));
        assert_eq!(at("time_in(OPEN)"), Value::Duration(3600));
        assert_eq!(at("entered_at(DONE)"), Value::Timestamp(t0 + 10_800));
        assert_eq!(at("entered_at(priority)"), Value::Timestamp(t0));
        assert_eq!(at("entered_at(WORKING) is not null"), Value::Bool(true));
        // the current span of a closed state has no duration (ADR-0146)
        assert_eq!(
            at("count(i in this.intervals where i.duration is null)"),
            Value::Int(1)
        );
        // a member's span counts only its object's open time (ADR-0148):
        // unassigned an hour, then assigned two hours and ten minutes, the
        // last ten closed
        assert_eq!(
            at("sum(i in this.intervals(engineer) where i.value is not null: i.duration)"),
            Value::Duration(7200)
        );
        assert_eq!(
            at("sum(i in this.intervals(engineer) where i.value is null: i.duration)"),
            Value::Duration(3600)
        );
        assert!(
            Evaluator::new(&w, t, Some(&job))
                .eval(&crate::expr::parse("count(i in this.intervals(title): i)").unwrap())
                .unwrap_err()
                .0
                .contains("not tracked")
        );
    }

    #[test]
    fn a_recording_is_dated_corrected_once_and_kept_off_a_finished_subject() {
        let text = JOBS.replace(
            "    states:\n      OPEN: { category: inbound }",
            "    observations:\n      checks:\n        kind: Check\n        attributes:\n          outcome: { type: Priority }\n          \
             note: { type: string, optional: true, personal: true }\n    states:\n      OPEN: { category: inbound }",
        );
        let lib = Library::load(&[("jobs.yaml".into(), text)]).unwrap();
        let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
        let t0 = s.now;
        let job = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"title": "Fix"}),
        ));
        let other = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"title": "Other"}),
        ));
        let first = created(&req(
            &mut s,
            "record",
            OPERATOR,
            ("Check", true),
            json!({"subject": &job, "outcome": "LOW"}),
        ));
        let theirs = created(&req(
            &mut s,
            "record",
            OPERATOR,
            ("Check", true),
            json!({"subject": &other, "outcome": "LOW"}),
        ));
        s.now += 3600;
        // a correction keeps the occurred time of the fact it replaces
        let fixed = created(&req(
            &mut s,
            "record",
            OPERATOR,
            ("Check", true),
            json!({"subject": &job, "outcome": "HIGH", "corrects": &first}),
        ));
        let w = s.snapshot().unwrap();
        let f = w.get(&fixed).unwrap();
        assert_eq!(f.attr("occurred_at"), Value::Timestamp(t0));
        assert_eq!(f.attr("recorded_at"), Value::Timestamp(t0 + 3600));
        // the subject reads only what nothing has corrected
        let t = w.lib.type_("Job");
        let checks = Evaluator::new(&w, t, Some(&job))
            .eval(&crate::expr::parse("checks").unwrap())
            .unwrap();
        assert_eq!(checks, Value::Set(vec![Value::Object(fixed.clone())]));
        // a fact is corrected once, and only by its own subject's
        assert_eq!(
            req(
                &mut s,
                "record",
                OPERATOR,
                ("Check", true),
                json!({"subject": &job, "outcome": "LOW", "corrects": &first})
            ),
            Verdict::Unsatisfied {
                clause: "corrects_current".into(),
                remedy: Some(Remedy::SelfServiceable)
            }
        );
        assert_eq!(
            req(
                &mut s,
                "record",
                OPERATOR,
                ("Check", true),
                json!({"subject": &job, "outcome": "LOW", "corrects": &theirs})
            ),
            Verdict::InvalidInput {
                inputs: vec!["corrects".into()]
            }
        );
        // a finished subject takes no new fact, and still takes a correction
        let ana = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ));
        created(&req(
            &mut s,
            "start",
            OPERATOR,
            (&job, false),
            json!({"engineer": &ana}),
        ));
        created(&req(&mut s, "finish", OPERATOR, (&job, false), json!({})));
        assert_eq!(
            req(
                &mut s,
                "record",
                OPERATOR,
                ("Check", true),
                json!({"subject": &job, "outcome": "LOW"})
            ),
            Verdict::Unsatisfied {
                clause: "subject_open".into(),
                remedy: Some(Remedy::UnreachableFromHere)
            }
        );
        created(&req(
            &mut s,
            "record",
            OPERATOR,
            ("Check", true),
            json!({"subject": &job, "outcome": "LOW", "corrects": &fixed}),
        ));
        // and its offer says so
        let w = s.snapshot().unwrap();
        let offers = crate::reads::availability(&w, OPERATOR, &job)
            .unwrap()
            .unwrap();
        let record = offers
            .iter()
            .find(|o| o.creates.as_deref() == Some("Check"))
            .unwrap();
        assert_eq!(
            record.verdict,
            Some(Verdict::Unsatisfied {
                clause: "subject_open".into(),
                remedy: Some(Remedy::UnreachableFromHere)
            })
        );
        assert!(record.inputs.iter().all(|(n, _)| n != "subject"));
    }

    #[test]
    fn a_backdated_request_is_held_to_its_bound_and_its_object_s_intervals() {
        let text = JOBS.replace(
            "to: WORKING, optional_inputs: [engineer]",
            "to: WORKING, backdating_limit: 2 days, optional_inputs: [engineer]",
        );
        let lib = Library::load(&[("jobs.yaml".into(), text)]).unwrap();
        let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
        let day = 86_400;
        let job = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"title": "Fix", "priority": "LOW"}),
        ));
        s.now += 3 * day;
        let fresh = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"title": "New", "priority": "LOW"}),
        ));
        let ana = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ));
        let at = |s: &mut Store, xn: &str, id: &str, when: i64| {
            let mut inputs = BTreeMap::new();
            if xn == "start" {
                inputs.insert("engineer".to_string(), json!(ana));
            }
            let r = Request {
                transition: xn.into(),
                actor: OPERATOR.into(),
                object_id: Some(id.into()),
                type_: None,
                inputs,
                occurred_at: Some(when),
                propose: false,
                expires_at: None,
                expected_version: None,
                principal: None,
            };
            request(s, &r).unwrap().unwrap()
        };
        let refused = Verdict::Unsatisfied {
            clause: "occurred_within".into(),
            remedy: Some(Remedy::SelfServiceable),
        };
        let now = s.now;
        // past its bound, ahead of now, or on a transition with none
        assert_eq!(at(&mut s, "start", &job, now - 2 * day - 1), refused);
        assert_eq!(at(&mut s, "start", &job, now + 1), refused);
        // before the interval it would end, though within its bound
        assert_eq!(at(&mut s, "start", &fresh, now - day), refused);
        // within it, dated by when it happened
        let v = at(&mut s, "start", &job, now - day);
        assert!(matches!(v, Verdict::Satisfied { .. }), "{v:?}");
        let w = s.snapshot().unwrap();
        let o = w.get(&job).unwrap();
        let working = o.spans["state"].last().unwrap();
        assert_eq!(working.entered_at, now - day);
        assert_eq!(o.spans["state"][0].left_at, Some(now - day));
        let events = s.history(&job).unwrap();
        assert_eq!(events.last().unwrap().occurred_at, Some(now - day));
        assert_eq!(events.last().unwrap().recorded_at, now);
        assert_eq!(at(&mut s, "finish", &job, now), refused);
    }

    #[test]
    fn a_part_is_not_moved_into_a_finished_whole() {
        let text = r#"
module: orders
categories: [open, closed]
types:
  Order:
    attributes:
      ref: { type: string }
      lines: { reference: "Line[]", opposite: order, aggregation: composite }
    states:
      OPEN: { category: open }
      CLOSED: { category: closed, final: true }
    transitions:
      place: { kind: initial, to: OPEN, required_inputs: [ref] }
      close: { kind: external, from: [OPEN], to: CLOSED }
  Line:
    attributes:
      order: { reference: Order, opposite: lines }
    states:
      ACTIVE: { category: open }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [order] }
      move: { kind: internal, from: [ACTIVE], required_inputs: [order] }
"#;
        let lib = Library::load(&[("orders.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let a = created(&req(
            &mut s,
            "place",
            OPERATOR,
            ("Order", true),
            json!({"ref": "A"}),
        ));
        let b = created(&req(
            &mut s,
            "place",
            OPERATOR,
            ("Order", true),
            json!({"ref": "B"}),
        ));
        let c = created(&req(
            &mut s,
            "place",
            OPERATOR,
            ("Order", true),
            json!({"ref": "C"}),
        ));
        created(&req(&mut s, "close", OPERATOR, (&b, false), json!({})));
        let line = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("Line", true),
            json!({"order": &a}),
        ));
        assert_eq!(
            req(
                &mut s,
                "move",
                OPERATOR,
                (&line, false),
                json!({"order": &b})
            ),
            Verdict::Unsatisfied {
                clause: "whole_open".into(),
                remedy: Some(Remedy::SelfServiceable)
            }
        );
        created(&req(
            &mut s,
            "move",
            OPERATOR,
            (&line, false),
            json!({"order": &c}),
        ));
        // a part left where it is moves nowhere, whatever its whole's state
        created(&req(&mut s, "close", OPERATOR, (&c, false), json!({})));
        assert_ne!(
            req(
                &mut s,
                "move",
                OPERATOR,
                (&line, false),
                json!({"order": &c})
            ),
            Verdict::Unsatisfied {
                clause: "whole_open".into(),
                remedy: Some(Remedy::SelfServiceable)
            }
        );
    }

    #[test]
    fn a_sign_off_goes_stale_when_what_it_read_is_written_after_it() {
        let text = JOBS
            .replace(
                "      note: { type: string, optional: true }",
                "      note: { type: string, optional: true }\n      signed: { type: event, optional: true }",
            )
            .replace(
                "      prioritised: { expression: priority is not null, remedy: self_serviceable }",
                "      prioritised: { expression: priority is not null, remedy: self_serviceable }\n      \
                 fresh: { expression: \"signed is not null and not changed_since([note], signed)\", remedy: delegable }",
            )
            .replace(
                "      finish: { kind: external, from: [WORKING], to: DONE }",
                "      finish: { kind: external, from: [WORKING], to: DONE, guards: { fresh: deny } }\n      \
                 sign: { kind: internal, from: any, effect: [ { assign: { location: signed, expr: this_event } } ] }\n      \
                 renote: { kind: internal, from: any, required_inputs: [note] }",
            );
        let lib = Library::load(&[("jobs.yaml".into(), text)]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let job = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"title": "Fix", "note": "a"}),
        ));
        let ana = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ));
        created(&req(
            &mut s,
            "start",
            OPERATOR,
            (&job, false),
            json!({"engineer": &ana}),
        ));
        let stale = Verdict::Unsatisfied {
            clause: "fresh".into(),
            remedy: Some(Remedy::Delegable),
        };
        created(&req(&mut s, "sign", OPERATOR, (&job, false), json!({})));
        created(&req(
            &mut s,
            "renote",
            OPERATOR,
            (&job, false),
            json!({"note": "b"}),
        ));
        assert_eq!(
            req(&mut s, "finish", OPERATOR, (&job, false), json!({})),
            stale
        );
        // the index is the store's: it holds across requests
        let w = s.snapshot().unwrap();
        let o = w.get(&job).unwrap();
        assert!(o.writes["note"] > o.writes["signed"]);
        created(&req(&mut s, "sign", OPERATOR, (&job, false), json!({})));
        created(&req(&mut s, "finish", OPERATOR, (&job, false), json!({})));
    }

    #[test]
    fn an_assertion_admits_what_it_names_until_it_holds_again() {
        let text = JOBS.replace(
            "      finish: { kind: external, from: [WORKING], to: DONE }",
            "      finish: { kind: external, from: [WORKING], to: DONE }\n      \
             correct: { kind: assertion, to: [OPEN, WORKING], inputs: { reason: { type: Priority } }, may_admit: [working_invariant] }\n      \
             assign: { kind: internal, from: any, required_inputs: [engineer] }",
        );
        let lib = Library::load(&[("jobs.yaml".into(), text)]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let job = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"title": "Fix"}),
        ));
        let to =
            |state: &str, admits: &[&str]| json!({"to": state, "reason": "LOW", "admits": admits});
        // without its admission, the state it asserts breaks an invariant
        assert!(matches!(
            req(
                &mut s,
                "correct",
                OPERATOR,
                (&job, false),
                to("WORKING", &[])
            ),
            Verdict::InvariantViolated { .. }
        ));
        // no assertion leaves a state for itself, or admits what it may not
        assert_eq!(
            req(&mut s, "correct", OPERATOR, (&job, false), to("OPEN", &[])),
            Verdict::InvalidInput {
                inputs: vec!["to".into()]
            }
        );
        assert_eq!(
            req(
                &mut s,
                "correct",
                OPERATOR,
                (&job, false),
                to("WORKING", &["bounded"])
            ),
            Verdict::InvalidInput {
                inputs: vec!["admits".into()]
            }
        );
        created(&req(
            &mut s,
            "correct",
            OPERATOR,
            (&job, false),
            to("WORKING", &["working_invariant"]),
        ));
        let w = s.snapshot().unwrap();
        assert_eq!(w.get(&job).unwrap().state, "WORKING");
        assert!(
            w.get(&job)
                .unwrap()
                .admissions
                .contains("working_invariant")
        );
        assert_eq!(s.history(&job).unwrap().last().unwrap().source, "asserted");
        // the admitted violation stands through another request
        created(&req(
            &mut s,
            "estimate",
            OPERATOR,
            (&job, false),
            json!({"hours": 3}),
        ));
        // and is discharged once the invariant holds again
        let ana = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ));
        created(&req(
            &mut s,
            "assign",
            OPERATOR,
            (&job, false),
            json!({"engineer": &ana}),
        ));
        assert!(
            s.snapshot()
                .unwrap()
                .get(&job)
                .unwrap()
                .admissions
                .is_empty()
        );
        // and none is taken at a final state
        created(&req(&mut s, "finish", OPERATOR, (&job, false), json!({})));
        assert_eq!(
            req(&mut s, "correct", OPERATOR, (&job, false), to("OPEN", &[])),
            Verdict::Unavailable {
                state: "DONE".into()
            }
        );
    }

    #[test]
    fn an_erasure_redacts_its_history_reaches_its_observations_and_stays() {
        let text = JOBS
            .replace(
                "      note: { type: string, optional: true }",
                "      note: { type: string, optional: true, personal: true }",
            )
            .replace(
                "      bounded: { expression: hours is null or hours >= 0 }",
                "      bounded: { expression: hours is null or hours >= 0 }\n      \
                 noted: { expression: \"state != Job.WORKING or note is not null\" }",
            )
            .replace(
                "      finish: { kind: external, from: [WORKING], to: DONE }",
                "      finish: { kind: external, from: [WORKING], to: DONE }\n      \
                 forget: { kind: erasure, inputs: { reason: { type: string } } }\n      \
                 renote: { kind: internal, from: any, required_inputs: [note] }\n      \
                 fix_note: { kind: internal, from: any, required_inputs: [note], corrects: [note], inputs: { reason: { type: string } } }\n      \
                 fix_by_reason: { kind: internal, from: any, corrects: [note], inputs: { reason: { type: string } }, effect: [ { assign: { location: note, expr: inputs.reason } } ] }\n      \
                 stamp: { kind: internal, from: any, effect: [ { assign: { location: note, expr: '\"x\"' } } ] }",
            )
            .replace(
                "    states:\n      OPEN: { category: inbound }",
                "    observations:\n      checks:\n        kind: Check\n        attributes:\n          \
                 remark: { type: string, optional: true, personal: true }\n    states:\n      OPEN: { category: inbound }",
            );
        let lib = Library::load(&[("jobs.yaml".into(), text)]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let job = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"title": "Fix", "note": "secret"}),
        ));
        let ana = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ));
        created(&req(
            &mut s,
            "start",
            OPERATOR,
            (&job, false),
            json!({"engineer": &ana}),
        ));
        let check = created(&req(
            &mut s,
            "record",
            OPERATOR,
            ("Check", true),
            json!({"subject": &job, "remark": "ana said"}),
        ));
        let tag = created(&req(
            &mut s,
            "record",
            OPERATOR,
            (crate::model::LABEL, true),
            json!({"subject": &job, "name": "VIP", "note": "rang"}),
        ));
        created(&req(
            &mut s,
            "fix_note",
            OPERATOR,
            (&job, false),
            json!({"note": "secret!", "reason": "typo"}),
        ));
        created(&req(
            &mut s,
            "fix_by_reason",
            OPERATOR,
            (&job, false),
            json!({"reason": "ana's number"}),
        ));
        // the erasure admits the invariant it breaks, and forgets the check
        let v = req(
            &mut s,
            "forget",
            OPERATOR,
            (&job, false),
            json!({"reason": "asked"}),
        );
        let Verdict::Satisfied { events, .. } = &v else {
            panic!("{v:?}");
        };
        assert_eq!(events.len(), 2);
        let w = s.snapshot().unwrap();
        let o = w.get(&job).unwrap();
        assert_eq!(o.attr("note"), Value::Absent);
        assert!(o.erased && o.admissions.contains("noted"));
        assert_eq!(w.get(&check).unwrap().attr("remark"), Value::Absent);
        // the past event keeps its place and shape, and loses the value
        let history = s.history(&job).unwrap();
        assert_eq!(history[0].payload["inputs"]["note"], Json::Null);
        assert_eq!(history[0].payload["inputs"]["title"], json!("Fix"));
        assert_eq!(history.last().unwrap().source, "erased");
        assert_eq!(history.last().unwrap().payload, json!({"erased": ["note"]}));
        // a reason the effect writes into a personal attribute is personal
        // without the marking, and stays off the event (check 10)
        assert!(
            history
                .iter()
                .filter(|e| e.transition == "fix_by_reason")
                .all(|e| e.payload.get("reason").is_none())
        );
        assert!(history.iter().any(|e| e.transition == "fix_by_reason"));
        // a correction's record of the value it replaced and wrote goes too,
        // its reason kept
        let fixed = history.iter().find(|e| e.source == "corrected").unwrap();
        assert_eq!(
            (
                fixed.payload["corrected"]["note"].clone(),
                fixed.payload["reason"].clone()
            ),
            (
                json!({"replaced": null, "value": null, "span": null}),
                json!("typo")
            )
        );
        // its label's note leaves the label's row and its creation event, as
        // no transition, the name kept (declaration-syntax.md §6.8)
        let l = w.get(&tag).unwrap();
        assert_eq!(
            (l.attr("note"), l.attr("name"), l.erased),
            (Value::Absent, Value::Text("vip".into()), false)
        );
        let labelled = s.history(&tag).unwrap();
        assert_eq!(labelled.len(), 1);
        assert_eq!(labelled[0].payload["inputs"]["note"], Json::Null);
        assert_eq!(labelled[0].payload["inputs"]["name"], json!("vip"));
        // and a label's own forget redacts its note
        let own = created(&req(
            &mut s,
            "record",
            OPERATOR,
            (crate::model::LABEL, true),
            json!({"subject": &ana, "name": "vip", "note": "rang"}),
        ));
        created(&req(
            &mut s,
            "forget",
            OPERATOR,
            (&own, false),
            json!({"reason": "asked"}),
        ));
        let w = s.snapshot().unwrap();
        let l = w.get(&own).unwrap();
        assert_eq!((l.attr("note"), l.erased), (Value::Absent, true));
        // what was erased stays erased, and the rest is still written
        let not_erased = Verdict::Unsatisfied {
            clause: "not_erased".into(),
            remedy: Some(Remedy::UnreachableFromHere),
        };
        assert_eq!(
            req(
                &mut s,
                "renote",
                OPERATOR,
                (&job, false),
                json!({"note": "again"})
            ),
            not_erased
        );
        created(&req(
            &mut s,
            "estimate",
            OPERATOR,
            (&job, false),
            json!({"hours": 3}),
        ));
        let w = s.snapshot().unwrap();
        let offers = crate::reads::availability(&w, OPERATOR, &job)
            .unwrap()
            .unwrap();
        let stamp = offers.iter().find(|o| o.transition == "stamp").unwrap();
        assert_eq!(stamp.verdict, Some(not_erased));
    }

    #[test]
    fn referrers_are_what_refers_to_an_object_its_parts_excepted() {
        let text = r#"
module: orders
categories: [open, closed]
types:
  Order:
    attributes:
      ref: { type: string }
      lines: { reference: "Line[]", opposite: order, aggregation: composite }
    states:
      OPEN: { category: open }
    transitions:
      place: { kind: initial, to: OPEN, required_inputs: [ref] }
  Line:
    attributes:
      order: { reference: Order, opposite: lines }
    states:
      ACTIVE: { category: open }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [order] }
  Note:
    attributes:
      about: { reference: Order }
    states:
      KEPT: { category: open }
    transitions:
      write: { kind: initial, to: KEPT, required_inputs: [about] }
"#;
        let lib = Library::load(&[("orders.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let order = created(&req(
            &mut s,
            "place",
            OPERATOR,
            ("Order", true),
            json!({"ref": "A"}),
        ));
        let other = created(&req(
            &mut s,
            "place",
            OPERATOR,
            ("Order", true),
            json!({"ref": "B"}),
        ));
        created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("Line", true),
            json!({"order": &order}),
        ));
        let note = created(&req(
            &mut s,
            "write",
            OPERATOR,
            ("Note", true),
            json!({"about": &order}),
        ));
        created(&req(
            &mut s,
            "write",
            OPERATOR,
            ("Note", true),
            json!({"about": &other}),
        ));
        let w = s.snapshot().unwrap();
        let referrers = Evaluator::new(&w, w.lib.type_("Order"), Some(&order))
            .eval(&crate::expr::parse("referrers").unwrap())
            .unwrap();
        assert_eq!(referrers, Value::Set(vec![Value::Object(note)]));
    }

    #[test]
    fn a_family_shares_its_base_s_members_scans_and_uniqueness() {
        let text = r#"
module: assets
categories: [live]
types:
  Asset:
    abstract: true
    attributes:
      key: { type: string, unique: true }
      name: { type: string }
    invariants:
      named: { expression: "length(name) > 0" }
  Laptop:
    extends: Asset
    attributes:
      owner: { type: string, optional: true }
    states:
      IN_USE: { category: live }
    transitions:
      receive: { kind: initial, to: IN_USE, required_inputs: [key, name] }
  Server:
    extends: Asset
    states:
      RACKED: { category: live }
    transitions:
      receive: { kind: initial, to: RACKED, required_inputs: [key, name] }
  Ticket:
    attributes:
      about: { reference: Asset }
    states:
      OPEN: { category: live }
    transitions:
      raise: { kind: initial, to: OPEN, required_inputs: [about] }
"#;
        let lib = Library::load(&[("assets.yaml".into(), text.into())]).unwrap();
        assert!(lib.is_a("Laptop", "Asset") && !lib.is_a("Asset", "Laptop"));
        let names: Vec<&str> = lib
            .type_("Laptop")
            .unwrap()
            .attributes
            .iter()
            .map(|(n, _)| n.as_str())
            .collect();
        assert_eq!(names, vec!["key", "name", "owner"]);
        let mut s = Store::sandbox(lib, 0).unwrap();
        let laptop = created(&req(
            &mut s,
            "receive",
            OPERATOR,
            ("Laptop", true),
            json!({"key": "IT-1", "name": "ThinkPad"}),
        ));
        // a sibling shares the base's uniqueness, and its invariant
        assert!(matches!(
            req(
                &mut s,
                "receive",
                OPERATOR,
                ("Server", true),
                json!({"key": "IT-1", "name": "db"})
            ),
            Verdict::InvariantViolated { .. }
        ));
        assert!(matches!(
            req(
                &mut s,
                "receive",
                OPERATOR,
                ("Server", true),
                json!({"key": "IT-2", "name": ""})
            ),
            Verdict::InvariantViolated { .. }
        ));
        created(&req(
            &mut s,
            "receive",
            OPERATOR,
            ("Server", true),
            json!({"key": "IT-2", "name": "db"}),
        ));
        // a reference to the base takes any of the family
        created(&req(
            &mut s,
            "raise",
            OPERATOR,
            ("Ticket", true),
            json!({"about": &laptop}),
        ));
        // and a scan of the base reads the family
        let w = s.snapshot().unwrap();
        let n = Evaluator::new(&w, None, None)
            .eval(&crate::expr::parse("count(a in Asset: a)").unwrap())
            .unwrap();
        assert_eq!(n, Value::Int(2));
    }

    #[test]
    fn a_proposal_is_filed_decided_and_invalidated_by_an_erasure() {
        let text = JOBS
            .replace(
                "      finish: { kind: external, from: [WORKING], to: DONE }",
                "      finish: { kind: external, from: [WORKING], to: DONE, proposable: true }\n      \
                 forget: { kind: erasure, inputs: { reason: { type: string } } }",
            )
            .replace(
                "      annotate: { kind: internal, from: [OPEN], required_inputs: [note] }",
                "      annotate: { kind: internal, from: [OPEN], required_inputs: [note], proposable: true }",
            )
            .replace(
                "      note: { type: string, optional: true }",
                "      note: { type: string, optional: true, personal: true }",
            );
        let lib = Library::load(&[("jobs.yaml".into(), text)]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let job = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"title": "Fix"}),
        ));
        let propose = |s: &mut Store, xn: &str, inputs: Json, expires_at: Option<i64>| {
            let r = Request {
                transition: xn.into(),
                actor: OPERATOR.into(),
                object_id: Some(job.clone()),
                type_: None,
                inputs: inputs
                    .as_object()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
                occurred_at: None,
                propose: true,
                expires_at,
                expected_version: None,
                principal: None,
            };
            request(s, &r).unwrap().unwrap()
        };
        // filing checks the transition and the inputs, not the state
        assert_eq!(
            propose(&mut s, "start", json!({}), None),
            Verdict::NotRequestable {
                parents: Vec::new()
            }
        );
        assert_eq!(
            propose(&mut s, "finish", json!({"bogus": 1}), None),
            Verdict::InvalidInput {
                inputs: vec!["bogus".into()]
            }
        );
        // an input left out that a generated guard refuses waits for approval
        assert!(matches!(
            propose(&mut s, "annotate", json!({}), None),
            Verdict::Satisfied { .. }
        ));
        let handover = created(&propose(&mut s, "finish", json!({}), None));
        // approved too soon, it is refused as the request would be, and waits
        assert_eq!(
            req(&mut s, "approve", OPERATOR, (&handover, false), json!({})),
            Verdict::Unavailable {
                state: "OPEN".into()
            }
        );
        assert_eq!(
            s.snapshot().unwrap().get(&handover).unwrap().state,
            "PENDING"
        );
        let ana = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ));
        created(&req(
            &mut s,
            "start",
            OPERATOR,
            (&job, false),
            json!({"engineer": &ana}),
        ));
        // approved, it is executed as the approver, its own event first
        let v = req(&mut s, "approve", OPERATOR, (&handover, false), json!({}));
        let Verdict::Satisfied {
            object_id, events, ..
        } = &v
        else {
            panic!("{v:?}");
        };
        assert_eq!(object_id, &job);
        let first = s.events(&events[..1]).unwrap().remove(0).unwrap();
        assert_eq!(
            (first.object_id.as_str(), first.transition.as_str()),
            (handover.as_str(), "approve")
        );
        // the approval is the cause of the request it runs (DESIGN.md §9)
        let ran = s.events(&events[1..2]).unwrap().remove(0).unwrap();
        assert_eq!((first.cause, ran.cause), (None, Some(first.position)));
        let w = s.snapshot().unwrap();
        assert_eq!(
            (
                w.get(&job).unwrap().state.as_str(),
                w.get(&handover).unwrap().state.as_str()
            ),
            ("DONE", "EXECUTED")
        );
        // a decided proposal is decided once
        assert_eq!(
            req(&mut s, "reject", OPERATOR, (&handover, false), json!({})),
            Verdict::Unavailable {
                state: "EXECUTED".into()
            }
        );
        // an expired one is not approved
        let other = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"title": "Other"}),
        ));
        let late = {
            let r = Request {
                transition: "annotate".into(),
                actor: OPERATOR.into(),
                object_id: Some(other.clone()),
                type_: None,
                inputs: [("note".to_string(), json!("ana's"))].into(),
                occurred_at: None,
                propose: true,
                expires_at: Some(s.now + 60),
                expected_version: None,
                principal: None,
            };
            created(&request(&mut s, &r).unwrap().unwrap())
        };
        s.now += 60;
        assert_eq!(
            req(&mut s, "approve", OPERATOR, (&late, false), json!({})),
            Verdict::Unavailable {
                state: "expired".into()
            }
        );
        // an erasure of its object invalidates it, and redacts its inputs
        created(&req(
            &mut s,
            "forget",
            OPERATOR,
            (&other, false),
            json!({"reason": "asked"}),
        ));
        let w = s.snapshot().unwrap();
        let p = w.get(&late).unwrap();
        assert_eq!(p.state, "INVALIDATED");
        let erasure = s.history(&other).unwrap().last().cloned().unwrap();
        let invalidated = s.history(&late).unwrap().last().cloned().unwrap();
        assert_eq!(invalidated.cause, Some(erasure.position));
        assert_eq!(p.attr("inputs"), Value::Text("{}".into()));
        assert_eq!(
            s.history(&late).unwrap()[0].payload["inputs"]["note"],
            Json::Null
        );
    }

    /// The reference runner's correction cases (`flowrun.py`, `self_test`,
    /// "a correction replaces the wrong value"): a crew corrected relabels
    /// its interval, corrected back merges it, and a row begun after the
    /// corrected interval's start credits the corrected value however it is
    /// dated (`declaration-syntax.md` §6.4, §6.9).
    #[test]
    fn a_correction_relabels_the_interval_it_corrects() {
        const CREWS: &str = r#"
module: crews
categories: [live, closed]
enumerations: { Crew: [NORTH, SOUTH, EAST] }
types:
  Job:
    attributes:
      crew: { type: Crew }
      title: { type: string, optional: true }
    states:
      OPEN: { category: live }
      WORKING: { category: live }
      DONE: { category: closed, final: true }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [crew] }
      start: { kind: external, from: [OPEN], to: WORKING, backdating_limit: 1 day }
      move_crew: { kind: internal, from: [OPEN, WORKING], required_inputs: [crew] }
      fix_crew: { kind: internal, from: [OPEN, WORKING], corrects: [crew], required_inputs: [crew], inputs: { reason: { type: string } } }
      fix_title: { kind: internal, from: any, corrects: [title], required_inputs: [title], inputs: { reason: { type: string } } }
"#;
        let crews = || {
            let lib = Library::load(&[("crews.yaml".into(), CREWS.into())]).unwrap();
            Store::sandbox(lib, 1_767_225_600).unwrap()
        };
        let crew = |c: &str| Value::Member {
            enumeration: "Crew".into(),
            member: c.into(),
        };
        let held_crew = |row: &Value| match row {
            Value::Row(f) => match f.get("held") {
                Some(Value::Row(h)) => h.get("crew").cloned().unwrap_or(Value::Absent),
                _ => Value::Absent,
            },
            _ => Value::Absent,
        };
        // a correction relabels the interval it corrects, marking it, and a
        // span or an event begun within it credits the corrected value
        let mut s = crews();
        let j = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"crew": "NORTH"}),
        ));
        created(&req(&mut s, "start", OPERATOR, (&j, false), json!({})));
        let fix = req(
            &mut s,
            "fix_crew",
            OPERATOR,
            (&j, false),
            json!({"crew": "SOUTH", "reason": "mis-keyed"}),
        );
        let Verdict::Satisfied { events, .. } = &fix else {
            panic!("{fix:?}");
        };
        let at = events[0];
        let w = s.snapshot().unwrap();
        let o = w.get(&j).unwrap();
        let t = w.lib.type_("Job").unwrap();
        let spans = &o.spans["crew"];
        assert_eq!(
            (spans.len(), spans[0].value.clone(), spans[0].corrected_by),
            (1, crew("SOUTH"), Some(at))
        );
        let states = crate::eval::interval_rows(&w, o, t, "state").unwrap();
        let working = states.last().unwrap();
        assert_eq!(held_crew(working), crew("SOUTH"));
        let Value::Row(f) = working else { panic!() };
        assert_eq!(f["corrected_by"], Value::Int(at));
        let rows: Vec<Value> = w
            .events
            .iter()
            .filter(|e| e.object_id == j)
            .map(|e| crate::eval::transition_row(&w, o, t, e))
            .collect();
        // the original write's event is not changed, and the start credits
        // the corrected crew
        assert_eq!(
            rows.iter().map(held_crew).collect::<Vec<_>>(),
            [Value::Absent, crew("SOUTH"), crew("SOUTH")]
        );
        // the correction's event names the attribute, the value it replaces,
        // the new value, the span it corrects and the reason
        let e = w.events.iter().find(|e| e.position == at).unwrap();
        assert_eq!(e.source, "corrected");
        assert_eq!(
            (
                e.payload["corrected"]["crew"].clone(),
                e.payload["reason"].clone()
            ),
            (
                json!({"replaced": "NORTH", "value": "SOUTH", "span": spans[0].entered_position}),
                json!("mis-keyed")
            )
        );
        drop(w);
        // a crew moved by mistake and corrected back: the relabelled interval
        // merges with the one before it, which leaves the index, and the
        // correction's own row credits the corrected crew
        let mut s = crews();
        let j = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"crew": "NORTH"}),
        ));
        s.now += 3600;
        created(&req(
            &mut s,
            "move_crew",
            OPERATOR,
            (&j, false),
            json!({"crew": "SOUTH"}),
        ));
        s.now += 3600;
        created(&req(
            &mut s,
            "fix_crew",
            OPERATOR,
            (&j, false),
            json!({"crew": "NORTH", "reason": "never moved"}),
        ));
        let w = s.snapshot().unwrap();
        let o = w.get(&j).unwrap();
        let t = w.lib.type_("Job").unwrap();
        let spans = &o.spans["crew"];
        assert_eq!(
            (
                spans.len(),
                spans[0].value.clone(),
                spans[0].left_at,
                spans[0].corrected_by.is_some()
            ),
            (1, crew("NORTH"), None, true)
        );
        let last = w.events.iter().rfind(|e| e.object_id == j).unwrap();
        assert_eq!(
            held_crew(&crate::eval::transition_row(&w, o, t, last)),
            crew("NORTH")
        );
        drop(w);
        // a span and an event begun after the corrected interval's start,
        // though dated before it, credit the corrected value: `.held` is read
        // at the position of the transition that began a row (§6.9)
        let mut s = crews();
        let j = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"crew": "NORTH"}),
        ));
        s.now += 7200;
        created(&req(
            &mut s,
            "move_crew",
            OPERATOR,
            (&j, false),
            json!({"crew": "SOUTH"}),
        ));
        let dated = Request {
            transition: "start".into(),
            actor: OPERATOR.into(),
            object_id: Some(j.clone()),
            type_: None,
            inputs: BTreeMap::new(),
            occurred_at: Some(s.now - 3600),
            propose: false,
            expires_at: None,
            expected_version: None,
            principal: None,
        };
        created(&request(&mut s, &dated).unwrap().unwrap());
        created(&req(
            &mut s,
            "fix_crew",
            OPERATOR,
            (&j, false),
            json!({"crew": "EAST", "reason": "mis-keyed"}),
        ));
        let w = s.snapshot().unwrap();
        let o = w.get(&j).unwrap();
        let t = w.lib.type_("Job").unwrap();
        let states = crate::eval::interval_rows(&w, o, t, "state").unwrap();
        let start = w.events.iter().find(|e| e.transition == "start").unwrap();
        assert_eq!(
            (
                held_crew(states.last().unwrap()),
                held_crew(&crate::eval::transition_row(&w, o, t, start)),
                o.spans["crew"]
                    .iter()
                    .map(|sp| sp.value.clone())
                    .collect::<Vec<_>>()
            ),
            (
                crew("EAST"),
                crew("EAST"),
                vec![crew("NORTH"), crew("EAST")]
            )
        );
        // a row's .corrected_by is read where a correction changed its value
        // or its .held, and absent elsewhere
        let read = Evaluator::new(&w, Some(t), Some(&j))
            .eval(
                &crate::expr::parse("count(i in this.intervals where i.corrected_by is not null)")
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(read, Value::Int(1));
        // an untracked attribute has no interval: its correction changes its
        // value and its event, naming no span
        created(&req(
            &mut s,
            "fix_title",
            OPERATOR,
            (&j, false),
            json!({"title": "Fix", "reason": "typo"}),
        ));
        let last = s.history(&j).unwrap().last().cloned().unwrap();
        assert_eq!(
            last.payload["corrected"]["title"],
            json!({"replaced": null, "value": "Fix", "span": null})
        );
    }

    /// A rule recording what it reads is evaluated whole (`DESIGN.md` §6,
    /// ADR-0158): the side of an `and` or an `or` that does not decide, and
    /// an `all`'s elements after one fails, are evaluated for the record,
    /// each evaluator's answer consulted by the call and its argument; and
    /// the verdict is the one stopping early would give, an evaluator the
    /// deciding side never needed leaving its remedy its own (ADR-0138).
    #[test]
    fn a_rule_is_evaluated_whole_for_its_record_and_decides_as_before() {
        const SHOP: &str = r#"
module: shop
categories: [live, closed]
evaluators:
  vault:
    functions:
      cleared: { arguments: { n: { type: int } } }
types:
  Order:
    attributes:
      n: { type: int }
    states:
      OPEN: { category: live }
      DONE: { category: closed, final: true }
    conditions:
      small_and_cleared: { expression: n < 10 and vault.cleared(n), remedy: self_serviceable }
      big_or_cleared: { expression: n >= 10 or vault.cleared(n), remedy: dependent }
      every_cleared: { expression: "all(o in Order: vault.cleared(o.n))", remedy: dependent }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [n] }
      ship: { kind: external, from: [OPEN], to: DONE, guards: { small_and_cleared: deny } }
      send: { kind: external, from: [OPEN], to: DONE, guards: { big_or_cleared: deny } }
      note: { kind: internal, from: [OPEN], guards: { every_cleared: audit } }
"#;
        let lib = Library::load(&[("shop.yaml".into(), SHOP.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let open = |s: &mut Store, n: i64| {
            created(&req(
                s,
                "open",
                OPERATOR,
                ("Order", true),
                json!({ "n": n }),
            ))
        };
        let (one, two, big) = (open(&mut s, 1), open(&mut s, 2), open(&mut s, 20));
        // the size decides; the vault, asked for the record, did not answer,
        // and the refusal keeps the guard's own remedy
        s.answers
            .insert("vault.cleared".into(), "unanswered".into());
        assert_eq!(
            req(&mut s, "ship", OPERATOR, (&big, false), json!({})),
            Verdict::Unsatisfied {
                clause: "small_and_cleared".into(),
                remedy: Some(Remedy::SelfServiceable),
            }
        );
        let w = s.snapshot().unwrap();
        let row = w.attempts.last().unwrap();
        assert!(!row.unknown);
        assert_eq!(
            row.consulted["small_and_cleared"]["values"],
            json!({ "vault.cleared(20)": "unanswered" })
        );
        drop(w);
        // the size decides again, and the vault's answer is on the event
        s.answers
            .insert("vault.cleared".into(), "unsatisfied".into());
        assert_eq!(
            created(&req(&mut s, "send", OPERATOR, (&big, false), json!({}))),
            big
        );
        let sent = s.history(&big).unwrap().pop().unwrap();
        assert_eq!(
            sent.payload["consulted"]["big_or_cleared"]["values"],
            json!({ "vault.cleared(20)": "unsatisfied" })
        );
        // the first order fails, and every order is asked for all the same
        assert_eq!(
            created(&req(&mut s, "note", OPERATOR, (&one, false), json!({}))),
            one
        );
        let noted = s.history(&one).unwrap().pop().unwrap();
        assert_eq!(
            noted.payload["consulted"]["every_cleared"]["values"],
            json!({ "vault.cleared(1)": "unsatisfied", "vault.cleared(2)": "unsatisfied", "vault.cleared(20)": "unsatisfied" })
        );
        assert_eq!(
            noted.reads["scans"]["every_cleared"]["Order"],
            json!(
                [big.clone(), one.clone(), two.clone()]
                    .iter()
                    .collect::<BTreeSet<_>>()
            )
        );
        let w = s.snapshot().unwrap();
        assert_eq!(
            w.attempts.last().unwrap().clause.as_deref(),
            Some("every_cleared")
        );
    }
}

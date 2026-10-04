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

use crate::eval::{Evaluator, NotRun, Who, truth};
use crate::expr::Expr;
use crate::model::{
    Declared, Enforce, From, Kind, Library, Remedy, Step, Transition, Type, Unique,
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

/// Why a request stops before it applies: a refusal, or a construct the
/// slice does not run.
enum Stop {
    Refused(Verdict),
    NotRun(NotRun),
}

impl std::convert::From<NotRun> for Stop {
    fn from(e: NotRun) -> Stop {
        Stop::NotRun(e)
    }
}

fn refused<T>(v: Verdict) -> Result<T, Stop> {
    Err(Stop::Refused(v))
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
        Stop::Refused(v) => {
            let clause = match &v {
                Verdict::CallRefused { clause, .. } => clause.clone(),
                other => other.clause().map(|c| qualified(&c, type_)),
            };
            Stop::Refused(Verdict::CallRefused {
                step,
                object_id: target,
                verdict: Box::new(v),
                clause,
            })
        }
        other => other,
    }
}

/// A refusal of a part a cascade drives, its clause qualified by the part's
/// type (`flow-format.md` §11.4).
fn as_part(stop: Stop, type_: &str) -> Stop {
    match stop {
        Stop::Refused(Verdict::Unsatisfied { clause, remedy }) => {
            Stop::Refused(Verdict::Unsatisfied {
                clause: qualified(&clause, type_),
                remedy,
            })
        }
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
    /// The occurred time every transition the request takes records: the
    /// request's own, or now (`DESIGN.md` §6).
    at: i64,
    answers: BTreeMap<String, String>,
    flags: Vec<Flag>,
    /// Every event, in the order recorded.
    events: Vec<Event>,
    created: BTreeSet<String>,
}

/// What the request answers: its verdict, and what it writes if it applies.
pub fn decide(
    world: &World<'_>,
    alloc: &mut Allocator,
    req: &Request,
    answers: &BTreeMap<String, String>,
) -> Result<(Verdict, Option<Change>), NotRun> {
    match decide_or_stop(world, alloc, req, answers) {
        Ok(answer) => Ok(answer),
        Err(Stop::Refused(v)) => Ok((v, None)),
        Err(Stop::NotRun(e)) => Err(e),
    }
}

fn decide_or_stop(
    world: &World<'_>,
    alloc: &mut Allocator,
    req: &Request,
    answers: &BTreeMap<String, String>,
) -> Result<(Verdict, Option<Change>), Stop> {
    let lib = world.lib;
    // 1. the actor
    let Some(who) = resolve_actor(world, &req.actor) else {
        return refused(Verdict::Unsatisfied {
            clause: "actor_known".into(),
            remedy: Some(Remedy::Dependent),
        });
    };
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
    if matches!(x.kind, Kind::Assertion | Kind::Erasure) {
        let what = if x.kind == Kind::Assertion {
            "assertion"
        } else {
            "erasure"
        };
        return stop_not_run(format!("an {what} ({tn}.{})", req.transition));
    }
    // 3. not requestable, and unavailable
    if !x.only_via.is_empty() {
        return refused(Verdict::NotRequestable {
            parents: x.only_via.clone(),
        });
    }
    if let Some(o) = oid.as_deref().and_then(|id| world.get(id))
        && !takes_from(t, x, &o.state)
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
        at: req.occurred_at.unwrap_or(world.now),
        answers: answers.clone(),
        flags: Vec::new(),
        events: Vec::new(),
        created: BTreeSet::new(),
    };
    let root = take(&mut ctx, alloc, &tn, &req.transition, oid, inputs, 0)?;
    // 7. the invariants, on every object, in ascending id order
    for obj in ctx.work.in_order() {
        let failing = failing_invariants(&ctx.work, obj)?;
        if failing.is_empty() {
            continue;
        }
        let failures: Vec<InvariantFailure> = failing
            .iter()
            .map(|n| InvariantFailure {
                object_id: obj.id.clone(),
                type_: obj.type_.clone(),
                invariant: n.clone(),
                remedy: invariant_remedy(&ctx.work, x, obj, n, &root),
            })
            .collect();
        let remedy = failures.first().and_then(|f| f.remedy);
        return refused(Verdict::InvariantViolated { failures, remedy });
    }
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
    Ok((
        Verdict::Satisfied {
            object_id: root,
            version,
            events: positions,
            flags: ctx.flags,
        },
        Some(Change { writes }),
    ))
}

/// Whether a transition takes an input of a name.
fn takes(t: &Type, x: &Transition, k: &str) -> bool {
    let _ = t;
    x.required_inputs
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
    if matches!(x.kind, Kind::Assertion | Kind::Erasure) {
        return stop_not_run(format!("a caused assertion or erasure ({tn}.{xn})"));
    }
    if depth > 0 {
        if let Some(o) = oid.as_deref().and_then(|id| ctx.work.get(id))
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
                Value::Object(id) => ctx.work.get(id).is_some_and(|o| &o.type_ == target),
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
        let now = ctx.work.now;
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
        if x.kind == Kind::External {
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
    }
    for k in &x.required_inputs {
        if !inputs.contains_key(k) {
            return refused(Verdict::Unsatisfied {
                clause: format!("{k}_provided"),
                remedy: Some(Remedy::SelfServiceable),
            });
        }
    }
    for (g, mode) in &x.guards {
        let Some(c) = t.condition(g) else {
            return stop_not_run(format!("the guard {g}, which {tn} does not declare"));
        };
        let asking = Evaluator::new(&ctx.work, Some(t), oid.as_deref())
            .with_inputs(&inputs)
            .with_actor(&ctx.who)
            .with_answers(&ctx.answers);
        let ok = truth(&asking.eval(&c.expression)?)?;
        if ok != Some(true) {
            match mode {
                Enforce::Deny => {
                    // an evaluator that did not answer leaves the guard
                    // unknown, and the caller may try again (ADR-0138)
                    return refused(Verdict::Unsatisfied {
                        clause: g.clone(),
                        remedy: if asking.unanswered.get() {
                            Some(Remedy::Temporal)
                        } else {
                            c.remedy
                        },
                    });
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
                }
                Enforce::Audit => {}
            }
        }
    }
    if let Some(what) = t.at_outcome.first() {
        return stop_not_run(what.clone());
    }
    // the tracked values as the transition finds them, its spans' baseline
    let before = oid
        .as_deref()
        .and_then(|id| ctx.work.get(id))
        .map(|o| tracked_values(lib, t, o));
    // 6. the outcome, its position allocated as it begins applying
    let position = alloc.position();
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
    let mut written = serde_json::Map::new();
    for k in x.required_inputs.iter().chain(&x.optional_inputs) {
        if let Some(v) = inputs.get(k) {
            o.attrs.insert(k.clone(), v.clone());
            written.insert(k.clone(), wire::encode(v));
        }
    }
    let id = o.id.clone();
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
    // the event, recorded as the transition ends, with the spans it opens
    let Some(o) = ctx.work.objects.get_mut(&id) else {
        return stop_not_run(format!("the object {id}, which its own effect removed"));
    };
    if !record_spans(lib, t, o, before.as_ref(), position, ctx.at, &ctx.who) {
        // what an effect changed, which no guard could see, precedes its
        // current interval
        return refused(occurred_within());
    }
    o.version += 1;
    let event = Event {
        position,
        object_id: id.clone(),
        object_seq: o.version,
        transition: xn.to_string(),
        from_state: from_state.clone(),
        to_state: o.state.clone(),
        changes_state: from_state.as_deref() != Some(o.state.as_str()),
        source: "observed".into(),
        actor_id: ctx.who.id.clone(),
        actor_kind: ctx.who.kind.clone(),
        declaration_version: 1,
        recorded_at: ctx.work.now,
        occurred_at: Some(ctx.at),
        imported: false,
        payload: json!({ "inputs": written }),
    };
    ctx.events.push(event);
    Ok(id)
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
        });
    }
    true
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
            }
            Step::Clear(names) => {
                if let Some(o) = ctx.work.objects.get_mut(id) {
                    for n in names {
                        o.attrs.remove(n);
                    }
                }
            }
            Step::Add { location, expr } | Step::Remove { location, expr } => {
                if unsupplied(expr, inputs) {
                    continue;
                }
                let v = effect_value(ctx, t, id, inputs, &env, position, expr, false)?;
                if v.is_absent() {
                    return stop_not_run("adding or removing an absent value");
                }
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
fn text_of(v: &Value) -> String {
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
    let Some(t) = world.lib.type_(&o.type_) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for (n, e) in &t.invariants {
        if truth(&Evaluator::new(world, Some(t), Some(&o.id)).eval(e)?)? == Some(false) {
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
        for other in world.of_type(&o.type_) {
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
    store.transact(|world, alloc| decide(world, alloc, req, &answers))
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
        };
        request(s, &r).unwrap().unwrap()
    }

    fn created(v: &Verdict) -> String {
        match v {
            Verdict::Satisfied { object_id, .. } => object_id.clone(),
            other => panic!("{other:?}"),
        }
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
}

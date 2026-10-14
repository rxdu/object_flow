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
use crate::expr::{BinOp, Expr};
use crate::model::{
    DECLARATION_CHANGE, Declared, Enforce, From, Kind, Library, PROPOSAL, Remedy, Step, Transition,
    Type, Unique,
};
use crate::store::{Allocator, Change, Event};
use crate::value::{Decimal, Value, parse_duration};
use crate::verdict::{Flag, InputReason, InvariantFailure, Told, Verdict};
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
    /// For a proposal's approval, the version of the proposed object its
    /// approver read: an approval finding it moved is refused as `stale`,
    /// where an agent grew a payment it had proposed to send and a person's
    /// approval sent the grown amount (tenth production review). Read by no
    /// other request.
    pub target_version: Option<i64>,
    /// The principal its actor acts for, an object's id or the identity it
    /// holds, recorded and never read by a rule (`DESIGN.md` §5.8).
    pub principal: Option<String>,
    /// The key that makes a retry safe: a request its actor applied before
    /// under it is replayed, not applied again (`DESIGN.md` §6 step 2,
    /// ADR-0041, ADR-0077).
    pub idempotency_key: Option<String>,
    /// Why the request was sent and by which route, the caller's reason,
    /// recorded on every event and attempt row it makes and read by no rule
    /// (`DESIGN.md` §6, ADR-0105 §8).
    pub context: Option<String>,
}

/// Why a request gets no verdict: a construct the slice does not run, or a
/// key its actor applied before to a different request, a fault in the
/// caller's key generation (`library-api.md` §7, ADR-0077).
#[derive(Clone, Debug, PartialEq)]
pub enum NoVerdict {
    NotRun(NotRun),
    KeyReused { key: String },
}

impl std::convert::From<NotRun> for NoVerdict {
    fn from(e: NotRun) -> NoVerdict {
        NoVerdict::NotRun(e)
    }
}

/// A request's digest, over everything it carries but its key and the
/// version it names: a retry is the original request re-sent, its occurred
/// time among what the key identifies, and it replays whatever version it
/// carries, a body's difference being the digest's to find and not the
/// version's (`DESIGN.md` §6 step 2, ADR-0076). An input marked personal,
/// by name in any type as a consulted reference masks it, counts by its
/// name alone: the store keeps no digest of personal data, which an erasure
/// then left nothing to compare and replayed a different request's verdict
/// under its key (fourteenth production review N2, ADR-0201).
fn request_digest(lib: &Library, req: &Request, actor: Option<&str>) -> String {
    use sha2::{Digest, Sha256};
    let personal = crate::eval::personal_names(lib);
    let inputs: BTreeMap<&String, Json> = req
        .inputs
        .iter()
        .map(|(n, v)| {
            let v = if personal.contains(n) {
                json!(crate::eval::PERSONAL)
            } else {
                v.clone()
            };
            (n, v)
        })
        .collect();
    // the actor as resolved, so a key sent by an actor named by its object
    // and then by its identity is one request (ADR-0192 decision 7)
    let body = json!({
        "transition": req.transition,
        "actor": actor.unwrap_or(&req.actor),
        "object_id": req.object_id,
        "type": req.type_,
        "inputs": inputs,
        "occurred_at": req.occurred_at,
        "propose": req.propose,
        "expires_at": req.expires_at,
        "principal": req.principal,
        "context": req.context,
    });
    let hex: String = Sha256::digest(body.to_string().as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{}{hex}", crate::store::MASKED_DIGEST)
}

/// A satisfied verdict as `of_idempotency` records it, and back.
fn recorded_verdict(v: &Verdict) -> Json {
    match v {
        Verdict::Satisfied {
            object_id,
            version,
            events,
            flags,
            ..
        } => json!({
            "object_id": object_id,
            "version": version,
            "events": events,
            "flags": flags.iter().map(|f| json!({
                "clause": f.clause,
                "remedy": f.remedy.map(|r| r.text()),
                "unknown": f.unknown,
                "objects": f.objects,
            })).collect::<Vec<_>>(),
        }),
        _ => Json::Null,
    }
}

fn replayed_verdict(recorded: &Json) -> Result<Verdict, NotRun> {
    let text = |k: &str| recorded[k].as_str().map(str::to_string);
    let (Some(object_id), Some(version), Some(events), Some(flags)) = (
        text("object_id"),
        recorded["version"].as_i64(),
        recorded["events"].as_array(),
        recorded["flags"].as_array(),
    ) else {
        return not_run(format!(
            "a recorded verdict the core cannot read: {recorded}"
        ));
    };
    Ok(Verdict::Satisfied {
        object_id,
        version,
        events: events.iter().filter_map(Json::as_i64).collect(),
        flags: flags
            .iter()
            .map(|f| Flag {
                clause: f["clause"].as_str().unwrap_or_default().to_string(),
                remedy: f["remedy"].as_str().and_then(Remedy::parse),
                unknown: f["unknown"].as_bool().unwrap_or(false),
                objects: f["objects"]
                    .as_array()
                    .map(|o| {
                        o.iter()
                            .filter_map(|i| i.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect(),
        replayed: true,
    })
}

fn not_run<T>(what: impl Into<String>) -> Result<T, NotRun> {
    Err(NotRun::Unsupported(what.into()))
}

/// Who acts, resolved from a request's actor: an object of a type that marks
/// an actor identity (`DESIGN.md` §6 step 1), named by its id or by its
/// identity, the earliest created where several hold it. Only the types that
/// mark an identity are read.
/// The kind of actor a name resolves to, none where it names none: what a
/// deployment admits a read by (ADR-0192 decision 3).
pub fn actor_kind(world: &World<'_>, actor: &str) -> Result<Option<String>, NotRun> {
    Ok(resolve_actor(world, actor)?.map(|w| w.kind))
}

pub(crate) fn resolve_actor(world: &World<'_>, actor: &str) -> Result<Option<Who>, NotRun> {
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
            ended: t.state(&o.state).is_some_and(|s| s.is_final),
        })
    };
    if let Some(o) = world.get(actor)? {
        return Ok(identity(o));
    }
    // by the identity's column and its index, never every object of each
    // actor type: a dry run resolving its submitter for each object read a
    // store's actors whole each time, and a person's submit on 37k objects
    // ran past 20 minutes where the operator's, named by id, took 9 s
    // (DRYRUN-ACTOR)
    let mut found: Option<(i64, Who)> = None;
    for (tn, t) in &world.lib.types {
        let Some((attr, _)) = t.actor_identity() else {
            continue;
        };
        let named = Value::Text(actor.to_string());
        for o in world.with_key(
            tn,
            std::slice::from_ref(&attr.to_string()),
            std::slice::from_ref(&named),
        )? {
            if o.type_ == *tn
                && let Some(w) = identity(o)
                && w.id == actor
                && found.as_ref().is_none_or(|(n, _)| o.order < *n)
            {
                found = Some((o.order, w));
            }
        }
    }
    // an identity named in another case than it is held in names that actor,
    // as uniqueness compares them, recorded under the identity it holds, so
    // an erasure's renaming reaches what it sent (seventh production review,
    // ADR-0194 decision 5)
    if found.is_none() {
        for (tn, t) in &world.lib.types {
            let Some((attr, _)) = t.actor_identity() else {
                continue;
            };
            for o in world.with_folded(tn, attr, actor)? {
                if o.type_ == *tn
                    && let Some(w) = identity(o)
                    && found.as_ref().is_none_or(|(n, _)| o.order < *n)
                {
                    found = Some((o.order, w));
                }
            }
        }
    }
    Ok(found.map(|(_, w)| w))
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
    /// The objects its remedy points at: those its failing clause names, a
    /// generated guard's stated ones, or an invariant violation's
    /// conflicting objects, in ascending id order (`DESIGN.md` §5.5).
    pub objects: Vec<String>,
}

/// Why a request stops before it applies: a refusal, or a construct the
/// slice does not run.
enum Stop {
    /// Boxed, since what a refusal read makes it large beside the rest.
    Refused(Verdict, Box<Refusal>),
    NotRun(NotRun),
}

impl std::convert::From<NotRun> for Stop {
    /// A value past its type's range, in a guard, an invariant, a loop's
    /// filter or an effect, refuses the request as the generated guard
    /// `value_in_range`, naming the object (ADR-0205 decision 2).
    fn from(e: NotRun) -> Stop {
        match e {
            NotRun::OutOfRange(id) => Stop::Refused(
                Verdict::Unsatisfied {
                    clause: "value_in_range".into(),
                    remedy: Some(Remedy::UnreachableFromHere),
                },
                Box::new(Refusal {
                    objects: if id.is_empty() { Vec::new() } else { vec![id] },
                    ..Refusal::default()
                }),
            ),
            e => Stop::NotRun(e),
        }
    }
}

fn refused<T>(v: Verdict) -> Result<T, Stop> {
    Err(Stop::Refused(v, Box::default()))
}

/// A generated guard's refusal, naming the objects it states (`DESIGN.md`
/// §5.5): its subject, the correction already recorded, its successor or
/// the destination whole.
fn refused_naming<T>(v: Verdict, object: &str) -> Result<T, Stop> {
    Err(Stop::Refused(
        v,
        Box::new(Refusal {
            objects: vec![object.to_string()],
            ..Refusal::default()
        }),
    ))
}

/// Objects by id, in ascending id order, which is creation order
/// (`DESIGN.md` §5.1).
pub(crate) fn in_id_order(
    world: &World<'_>,
    ids: impl IntoIterator<Item = String>,
) -> Result<Vec<String>, NotRun> {
    let mut out = Vec::new();
    for id in ids {
        let order = world.get(&id)?.map_or(i64::MAX, |o| o.order);
        out.push((order, id));
    }
    out.sort();
    Ok(out.into_iter().map(|(_, id)| id).collect())
}

/// The objects in conflict where an invariant fails on an object, as an
/// unsatisfied clause's are chosen, the object excluded: what a declared
/// invariant's references evaluate to, and the other holders of a value a
/// uniqueness requires be one's own (`DESIGN.md` §5.5, ADR-0122).
fn invariant_objects(world: &World<'_>, o: &Object, n: &str) -> Result<BTreeSet<String>, NotRun> {
    let Some(t) = world.lib.type_(&o.type_) else {
        return Ok(BTreeSet::new());
    };
    if let Some((_, e)) = t.invariants.iter().find(|(i, _)| i == n) {
        let named = crate::eval::Named::on(Some(&o.id));
        Evaluator::new(world, Some(t), Some(&o.id))
            .with_naming(&named)
            .eval(e)?;
        return Ok(named.into_inner().objects);
    }
    if n == IDENTITY_TAKEN {
        return Ok(identity_holders(world, o)?.into_iter().collect());
    }
    if n.ends_with("_unique") {
        let reading = std::cell::RefCell::new(Reading::default());
        failing_invariants_reading(world, o, Some(&reading))?;
        return Ok(reading
            .into_inner()
            .scans
            .get(n)
            .map(|by_type| {
                by_type
                    .values()
                    .flatten()
                    .filter(|id| **id != o.id)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default());
    }
    Ok(BTreeSet::new())
}

/// A value written to an attribute, as its declared type holds it, or why
/// it cannot be:
/// - an integer is written to a decimal at its scale, and a decimal to a
///   decimal or an integer rounded half to even to its scale, as §8.3 says a
///   product or quotient is (F4);
/// - an amount keeps its currency at its minor unit;
/// - text stays within what a request can send;
/// - every other kind is written only to an attribute of its own kind.
///
/// An integer written to a decimal was stored in its units, `5` read as
/// `0.05`, and a decimal to an integer likewise, `10.50` read as `1050`; a
/// date written to a timestamp left its object unreadable (the audit's
/// values, ADR-0205 decision 1). Past the store's 18 digits nothing is
/// written.
pub(crate) fn fit_value(declared: &Declared, many: bool, v: Value) -> Result<Value, String> {
    if v.is_absent() {
        return Ok(v);
    }
    if many {
        let Value::Set(items) = v else {
            return Err(format!("a {} where a set is held", v.kind()));
        };
        return items
            .into_iter()
            .map(|i| fit_value(declared, false, i))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Set);
    }
    let kind = v.kind();
    let wrong = || Err(format!("a {kind} where {declared:?} is held"));
    let digits = |d: Decimal| -> Result<Decimal, String> {
        if d.units().abs() <= 999_999_999_999_999_999 {
            Ok(d)
        } else {
            Err(format!("{d} is past the 18 digits a value holds"))
        }
    };
    let ty = match declared {
        Declared::Reference(_) => {
            return if matches!(v, Value::Object(_)) {
                Ok(v)
            } else {
                wrong()
            };
        }
        Declared::Scalar(ty) => ty.as_str(),
    };
    if let Some(rest) = ty.strip_prefix("decimal(") {
        let scale = rest
            .trim_end_matches(')')
            .rsplit(',')
            .next()
            .and_then(|s| s.trim().parse::<u8>().ok())
            .unwrap_or(0);
        let p = rest
            .split(',')
            .next()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(18);
        let d = match &v {
            Value::Int(n) => Decimal::from_int(*n),
            Value::Decimal(d) => *d,
            _ => return wrong(),
        };
        let held = d
            .rescale(scale)
            .ok_or_else(|| format!("{d} has no value at {scale} places"))?;
        if held.units().abs() > 10i128.saturating_pow(p).saturating_sub(1) {
            return Err(format!("{held} has more than {p} digits"));
        }
        return digits(held).map(Value::Decimal);
    }
    if let Some(ccy) = ty.strip_prefix("money(").map(|c| c.trim_end_matches(')')) {
        let Value::Money { currency, amount } = &v else {
            return wrong();
        };
        if currency != ccy {
            return Err(format!("an amount in {currency} where {ccy} is held"));
        }
        let minor = crate::value::minor_unit(ccy).unwrap_or(2);
        let held = amount
            .rescale(minor)
            .ok_or_else(|| format!("{amount} has no value at {minor} places"))?;
        return digits(held).map(|amount| Value::Money {
            currency: currency.clone(),
            amount,
        });
    }
    match (ty, &v) {
        ("int" | "counter" | "event", Value::Int(_)) => Ok(v),
        // a decimal to an integer only where it is whole, never rounded
        ("int" | "counter", Value::Decimal(d)) => d
            .rescale(0)
            .filter(|w| w == d)
            .and_then(|w| w.to_int())
            .map(Value::Int)
            .ok_or_else(|| format!("{d} has no integer value")),
        ("string" | "text" | "identity" | "actor_kind", Value::Text(s)) => {
            if s.len() > crate::wire::TEXT_BYTES {
                Err(format!(
                    "a string of {} bytes, past the {} a value holds",
                    s.len(),
                    crate::wire::TEXT_BYTES
                ))
            } else {
                Ok(v)
            }
        }
        // a date or a time past the calendar, which no read takes back
        // (fifth production review BRICK, ADR-0190 decision 1)
        ("date", Value::Date(d)) if !crate::value::in_calendar(*d) => {
            Err("a date past the calendar".to_string())
        }
        ("timestamp", Value::Timestamp(s))
            if !(crate::value::FIRST_SECOND..=crate::value::LAST_SECOND).contains(s) =>
        {
            Err("a time past the calendar".to_string())
        }
        ("bool", Value::Bool(_))
        | ("date", Value::Date(_))
        | ("timestamp", Value::Timestamp(_))
        | ("duration", Value::Duration(_))
        | ("file", Value::File { .. }) => Ok(v),
        // an enumeration's member, of that enumeration and no other
        (e, Value::Member { enumeration, .. }) if enumeration == e => Ok(v),
        _ => wrong(),
    }
}

/// The scale of a declared `decimal(p,s)`, none for any other type.
fn decimal_scale(declared: &Declared) -> Option<u8> {
    let Declared::Scalar(ty) = declared else {
        return None;
    };
    ty.strip_prefix("decimal(")
        .and_then(|r| r.trim_end_matches(')').rsplit(',').next())
        .and_then(|s| s.trim().parse::<u8>().ok())
}

/// A value written to an attribute, fitted to its declared type
/// (`fit_value`); one it cannot hold refuses the request `value_in_range`,
/// as arithmetic past its bound does.
fn at_declared_scale(t: &Type, name: &str, v: Value, id: &str) -> Result<Value, Stop> {
    let Some(a) = t.attribute(name) else {
        return Ok(v);
    };
    match fit_value(&a.declared, a.many, v) {
        Ok(v) => Ok(v),
        Err(_) => refused_naming(
            Verdict::Unsatisfied {
                clause: "value_in_range".into(),
                remedy: Some(Remedy::UnreachableFromHere),
            },
            id,
        ),
    }
}

/// What refuses an actor, resolved or not: one the store holds no actor
/// for, as `actor_known`, and one whose object is in a final state, a
/// person who left or an agent retired, as `actor_active`, each `dependent`
/// (`DESIGN.md` §5.8, §6 step 1, ADR-0182 decision 6).
pub(crate) fn active(who: Option<Who>) -> Result<Who, Verdict> {
    let clause = match who {
        None => "actor_known",
        Some(w) if w.ended => "actor_active",
        Some(w) => return Ok(w),
    };
    Err(Verdict::Unsatisfied {
        clause: clause.into(),
        remedy: Some(Remedy::Dependent),
    })
}

/// Who a request acts as: its actor, and the principal it names, each an
/// object of a type that marks an actor identity, or the request is refused
/// as `actor_known`, and each in a state that is not final, or it is
/// refused as `actor_active` (`DESIGN.md` §6 step 1). The kind is the
/// actor's.
fn request_who(world: &World<'_>, req: &Request) -> Result<Who, Stop> {
    let mut who = match active(resolve_actor(world, &req.actor)?) {
        Ok(who) => who,
        Err(v) => return refused(v),
    };
    if let Some(p) = &req.principal {
        match active(resolve_actor(world, p)?) {
            Ok(principal) => who.principal = Some(principal.id),
            Err(v) => return refused(v),
        }
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
        world.get_if(req.object_id.as_deref())?,
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
    Err(Stop::NotRun(NotRun::Unsupported(what.into())))
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
    /// Each erased actor's identity, with its object's id, which replaces
    /// it wherever the store keeps it (ADR-0192 decision 4).
    renamed: Vec<(String, String)>,
    /// Whether a group erasure is walking its group, which a member's own
    /// erasure then does not walk again (ADR-0192 decision 7).
    in_group: bool,
    /// Each past event's personal input an erasure redacts, by the event's
    /// position and the input's name.
    redacted_inputs: Vec<(i64, String)>,
    /// What an erasure's walk down clears on another object: the object,
    /// the attribute, and the event that carried the erased value in; and
    /// the interval spans so redacted, by object, dimension and entry.
    cleared: Vec<(String, String, i64)>,
    redacted_spans: Vec<(String, String, i64)>,
    /// The external identifiers it writes, and those it no longer holds:
    /// source, value and object, with the attribute of a written one.
    external: Vec<(String, String, String, String)>,
    external_removed: Vec<(String, String, String)>,
    /// Every event, in the order recorded.
    events: Vec<Event>,
    created: BTreeSet<String>,
    /// Why the request was sent, which every event and row it makes records.
    context: Option<String>,
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
    decide_as(world, alloc, req, answers, &None, None)
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
    prepared: Option<&crate::publish::Prepared>,
) -> Result<(Verdict, Option<Change>, Refusal), NotRun> {
    // a request filed for approval, and one deciding a proposal, are the
    // store's own (DESIGN.md §9)
    let target = world.get_if(req.object_id.as_deref())?;
    let deciding = target.is_some_and(|o| o.type_ == PROPOSAL);
    let changing = target.is_some_and(|o| o.type_ == DECLARATION_CHANGE)
        || (req.object_id.is_none() && req.type_.as_deref() == Some(DECLARATION_CHANGE));
    let answer = if changing {
        decide_change(world, alloc, req, answers, prepared)
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
        Some(id) => match world.get(id)? {
            None => return refused(Verdict::NotFound),
            // an object named beside a type is found only as one of that
            // type's family, so a tool bound to a type takes no other type's
            // object of the id it is given: an agent's `customer_revoke`
            // revoked a person's subscription (thirteenth production review B)
            Some(o) if req.type_.as_deref().is_some_and(|t| !lib.is_a(&o.type_, t)) => {
                return refused(Verdict::NotFound);
            }
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
    // 3. not requestable, and unavailable: the operator's creation is taken
    // only by creating the store (ADR-0126), and its retirement by nothing,
    // since it is irreversible and would leave the store with no operator,
    // reading no one's kind (ADR-0184 decision 1)
    if t.name == crate::model::OPERATOR_TYPE {
        return refused(Verdict::NotRequestable {
            parents: Vec::new(),
        });
    }
    if !x.only_via.is_empty() {
        return refused(Verdict::NotRequestable {
            parents: x.only_via.clone(),
        });
    }
    // an assertion runs from any state but a final one, an erasure at any
    if let Some(o) = world.get_if(oid.as_deref())?
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
        return refused(Verdict::invalid_input(
            unknown.into_iter().map(|k| (k, InputReason::UnknownInput)),
        ));
    }
    let mut inputs: BTreeMap<String, Value> = BTreeMap::new();
    let mut wrong: Vec<(String, InputReason)> = Vec::new();
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
                _ => wrong.push((k.clone(), InputReason::WrongType)),
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
            Err(e) => wrong.push((k.clone(), e.1)),
        }
    }
    if !wrong.is_empty() {
        return refused(Verdict::invalid_input(wrong));
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
        renamed: Vec::new(),
        in_group: false,
        redacted_inputs: Vec::new(),
        external: Vec::new(),
        external_removed: Vec::new(),
        cleared: Vec::new(),
        redacted_spans: Vec::new(),
        context: req.context.clone(),
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
            r.reads = refused_reads(&ctx)?;
            return Err(Stop::Refused(v, r));
        }
        Err(stop) => return Err(stop),
    };
    // 7. the invariants the writes could violate, in ascending id order of
    // the object each is on (DESIGN.md §5.6, ADR-0045), except a violation an
    // admission lets stand, which is discharged once the invariant holds
    // again (ADR-0054); an assertion admits what it names
    let first = ctx.events.iter().map(|e| e.position).min().unwrap_or(0);
    let mut admitted = Vec::new();
    // every failing invariant on every object, by object id, then in the
    // order the type declares them (library-api.md §4, InvariantViolated)
    let mut violated: Vec<InvariantFailure> = Vec::new();
    let mut discharged = Vec::new();
    // the requested object's type, which a uniqueness's remedy reads
    let requested_type = ctx
        .work
        .get(&root)?
        .map(|o| o.type_.clone())
        .unwrap_or_default();
    let ids = affected_by_writes(&ctx.work, &ctx.before)?;
    // the objects this request is creating, which a refusal names by no id,
    // since none exists and a later object may take it (D581)
    let mut creating = BTreeSet::new();
    // each failure's object, which its conflicting objects are read from
    let mut failing_on: Vec<(String, String)> = Vec::new();
    for id in ids {
        let Some(obj) = ctx.work.get(&id)? else {
            continue;
        };
        let existed = ctx.before.get(&id)?.is_some();
        if !existed {
            creating.insert(id.clone());
        }
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
        let standing = &ctx.work.parts(obj)?.admissions;
        let holding: Vec<String> = standing
            .iter()
            .filter(|n| !failing.contains(n))
            .cloned()
            .collect();
        let mut admit = Vec::new();
        let mut refusing = Vec::new();
        for n in failing {
            if standing.contains(&n) {
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
            failing_on.extend(refusing.iter().map(|n| (obj.id.clone(), n.clone())));
            violated.extend(refusing.iter().map(|n| InvariantFailure {
                object_id: existed.then(|| obj.id.clone()),
                type_: obj.type_.clone(),
                invariant: n.clone(),
                remedy: invariant_remedy(&ctx.work, x, obj, n, &root, &requested_type),
            }));
            continue;
        }
        if let Some(o) = ctx.work.get_mut(&id)? {
            let o = o.parts_mut()?;
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
    if !violated.is_empty() {
        let remedy = violated.first().and_then(|f| f.remedy);
        // each failure's conflicting objects, the object it fails on
        // excluded (DESIGN.md §5.5)
        let mut conflicting = BTreeSet::new();
        for (id, n) in &failing_on {
            if let Some(o) = ctx.work.get(id)? {
                conflicting.extend(invariant_objects(&ctx.work, o, n)?);
            }
        }
        conflicting.retain(|id| !creating.contains(id));
        return Err(Stop::Refused(
            Verdict::InvariantViolated {
                failures: violated,
                remedy,
            },
            Box::new(Refusal {
                reads: refused_reads(&ctx)?,
                objects: in_id_order(&ctx.work, conflicting)?,
                ..Refusal::default()
            }),
        ));
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
        let Some(o) = ctx.work.get(&e.object_id)? else {
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
    let version = ctx.work.get(&root)?.map_or(0, |o| o.version);
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
            replayed: false,
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
            redacted_inputs: ctx.redacted_inputs,
            external: ctx.external,
            external_removed: ctx.external_removed,
            redacted_spans: ctx.redacted_spans,
            renamed_actors: ctx.renamed,
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

/// The most transitions one request takes each caused by the one before:
/// a transition calling itself down a reference chain, which both checkers
/// refuse and an unchecked load may hold, recursed once a link and aborted
/// the process past a few thousand (twelfth production review). A
/// supersession group's members are each one deeper than the erasure that
/// walks it, never one another.
pub const CALL_DEPTH: usize = 64;

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
    if depth > CALL_DEPTH {
        return stop_not_run(format!(
            "a chain of transitions each caused by the one before, deeper than {CALL_DEPTH} ({tn}.{xn})"
        ));
    }
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
        if let Some(o) = ctx.work.get_if(oid.as_deref())?
            && x.kind != Kind::Erasure
            && !takes_from(t, x, &o.state)
        {
            return refused(Verdict::Unavailable {
                state: o.state.clone(),
            });
        }
        let unknown: Vec<String> = inputs.keys().filter(|k| !takes(t, x, k)).cloned().collect();
        if !unknown.is_empty() {
            return refused(Verdict::invalid_input(
                unknown.into_iter().map(|k| (k, InputReason::UnknownInput)),
            ));
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
            let mut fits = true;
            for i in &ids {
                fits &= match i {
                    // a label's subject may be of any type (declaration-syntax.md §6.8)
                    Value::Object(id) => ctx.work.get(id)?.is_some_and(|o| {
                        target == crate::model::ANY_TYPE || lib.is_a(&o.type_, target)
                    }),
                    _ => false,
                };
            }
            if !fits {
                wrong.push((k.clone(), InputReason::NoSuchObject));
            }
        }
    }
    if !wrong.is_empty() {
        return refused(Verdict::invalid_input(wrong));
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
        return refused(Verdict::invalid_input(
            left_out.into_iter().map(|k| (k, InputReason::Missing)),
        ));
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
        return refused(Verdict::invalid_input(
            missing.into_iter().map(|k| (k, InputReason::Missing)),
        ));
    }
    // an assertion's `to` is a state it lists, other than the one the object
    // is in, and its `admits` only invariants it may admit (§4.13)
    if x.kind == Kind::Assertion {
        let current = ctx.work.get_if(oid.as_deref())?.map(|o| o.state.clone());
        let to = match inputs.get("to") {
            Some(Value::Text(s)) if x.to.contains(s) && current.as_ref() != Some(s) => s.clone(),
            _ => {
                return refused(Verdict::invalid_input([(
                    "to".to_string(),
                    InputReason::NotAllowed,
                )]));
            }
        };
        let admits: BTreeSet<String> = match inputs.get("admits") {
            Some(Value::Set(items)) => items.iter().map(text_of).collect(),
            _ => BTreeSet::new(),
        };
        if !admits.iter().all(|a| x.may_admit.contains(a)) {
            return refused(Verdict::invalid_input([(
                "admits".to_string(),
                InputReason::NotAllowed,
            )]));
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
    if let Some(o) = ctx.work.get_if(oid.as_deref())? {
        let spans = &ctx.work.parts(o)?.spans;
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
            spans
                .get(m)
                .and_then(|s| s.last())
                .is_some_and(|last| ctx.at < last.entered_at)
        }) {
            return refused(occurred_within());
        }
    }
    // whole_open: a part is not moved into a whole that is finished
    if let Some(o) = ctx.work.get_if(oid.as_deref())? {
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
            let finished = ctx.work.get(whole)?.is_some_and(|w| {
                lib.type_(&w.type_)
                    .and_then(|wt| wt.state(&w.state))
                    .is_some_and(|s| s.is_final)
            });
            if owner && moved && finished {
                return refused_naming(
                    Verdict::Unsatisfied {
                        clause: "whole_open".into(),
                        remedy: Some(Remedy::SelfServiceable),
                    },
                    whole,
                );
            }
        }
    }
    // not_erased: an erased object's personal attributes are not written
    // again, by an input or by a step that assigns or adds; a clear writes
    // absence and passes (ADR-0147)
    if x.kind != Kind::Erasure
        && let Some(o) = ctx.work.get_if(oid.as_deref())?
        && ctx.work.parts(o)?.erased
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
            let superseded = ctx.work.get(successor)?.is_some_and(|s| {
                lib.type_(&s.type_)
                    .and_then(|st| st.state(&s.state))
                    .is_some_and(|state| state.superseding)
            });
            if Some(successor.as_str()) == oid.as_deref() || superseded {
                return refused_naming(
                    Verdict::Unsatisfied {
                        clause: "successor_not_superseded".into(),
                        remedy: Some(Remedy::SelfServiceable),
                    },
                    successor,
                );
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
            if ctx.work.get(corrected)?.map(|c| c.attr("subject"))
                != Some(Value::Object(subject.clone()))
            {
                return refused(Verdict::invalid_input([(
                    "corrects".to_string(),
                    InputReason::NotAllowed,
                )]));
            }
            // the observations that correct another, of the types that can:
            // the correction already recorded
            let mut current: Option<String> = None;
            for (kn, k) in &lib.types {
                if current.is_none() && k.attribute("corrects").is_some() {
                    current = ctx
                        .work
                        .of_type(kn)?
                        .iter()
                        .find(|other| other.attr("corrects") == named)
                        .map(|other| other.id.clone());
                }
            }
            if let Some(recorded) = current {
                return refused_naming(
                    Verdict::Unsatisfied {
                        clause: "corrects_current".into(),
                        remedy: Some(Remedy::SelfServiceable),
                    },
                    &recorded,
                );
            }
        } else if let Some(s) = ctx.before.get(subject)?
            && lib
                .type_(&s.type_)
                .and_then(|st| st.state(&s.state))
                .is_some_and(|state| state.is_final)
        {
            return refused_naming(
                Verdict::Unsatisfied {
                    clause: "subject_open".into(),
                    remedy: Some(Remedy::UnreachableFromHere),
                },
                subject,
            );
        }
        // a mirror's observations are the import's to write (ADR-0101)
        if ctx
            .work
            .get(subject)?
            .and_then(|s| lib.type_(&s.type_))
            .is_some_and(|st| st.mirror)
        {
            return refused_naming(
                Verdict::Unsatisfied {
                    clause: "subject_owned".into(),
                    remedy: Some(Remedy::UnreachableFromHere),
                },
                subject,
            );
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
    // a label's name is vocabulary, normalised to lower case and trimmed, and
    // a name empty once normalised is no name (declaration-syntax.md §6.8,
    // ADR-0161)
    if tn == crate::model::LABEL
        && xn == "record"
        && let Some(Value::Text(name)) = inputs.get_mut("name")
    {
        *name = name.trim().to_lowercase();
        if name.is_empty() {
            return refused(Verdict::invalid_input([(
                "name".to_string(),
                InputReason::Malformed,
            )]));
        }
    }
    // what the transition's guards read, each guard's under its name
    let mut read = Reading::default();
    for (g, mode) in &x.guards {
        let Some(c) = t.condition(g) else {
            return stop_not_run(format!("the guard {g}, which {tn} does not declare"));
        };
        let reading = std::cell::RefCell::new(Reading::default());
        let named = crate::eval::Named::on(oid.as_deref());
        let (ok, unanswered) = {
            let asking = Evaluator::new(&ctx.work, Some(t), oid.as_deref())
                .with_committed(&ctx.before)
                .with_inputs(&inputs)
                .with_actor(&ctx.who)
                .with_reading(&reading, g)
                .with_naming(&named);
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
                            objects: in_id_order(&ctx.work, named.into_inner().objects)?,
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
                        unknown: ok.is_none(),
                        objects: in_id_order(&ctx.work, named.into_inner().objects)?,
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
    let before_attrs = ctx
        .work
        .get_if(oid.as_deref())?
        .map(|o| o.attrs.clone())
        .unwrap_or_default();
    // the tracked values as the transition finds them, its spans' baseline
    let before = ctx
        .work
        .get_if(oid.as_deref())?
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
            context: ctx.context.clone(),
            request_values: Json::Null,
            withheld: Vec::new(),
            declaration_version: lib.version,
            // an observing clause's would-be refusal carries what it read
            // and consulted (ADR-0088 §3)
            reads: clause_read.record(ctx.work.now, None),
            consulted: clause_read
                .consulted_record(ctx.work.now)
                .unwrap_or(Json::Null),
            txn: 0,
            call: Json::Null,
            proposal: None,
        });
    }
    alloc.txn();
    let (mut o, from_state) = match ctx.work.get_if(oid.as_deref())? {
        // the object as it stands, the rest of its record read, which the
        // transition extends
        Some(o) => {
            ctx.work.parts(o)?;
            (o.clone(), Some(o.state.clone()))
        }
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
                order: ctx.work.next_order()?,
                superseded_by: None,
                parts: crate::world::Parts::default().held(),
                created_dated: true,
                recorded_from: ctx.at,
                state_source: None,
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
            ctx.work.insert(o.clone())?;
            for (a, spec) in &t.attributes {
                if let Some(d) = &spec.default {
                    let v = Evaluator::new(&ctx.work, Some(t), Some(&id))
                        .with_committed(&ctx.before)
                        .with_inputs(&inputs)
                        .with_actor(&ctx.who)
                        .eval(d)?;
                    if !v.is_absent() {
                        o.attrs.insert(a.clone(), at_declared_scale(t, a, v, &id)?);
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
        // an actor's identity, which the store keeps beside the actor's
        // object, its events' actor among it, taken before it is cleared
        // (sixth production review ERASED-IDENTITY, ADR-0192 decision 4)
        let identity = t.actor_identity().and_then(|(a, _)| match o.attrs.get(a) {
            Some(Value::Text(v)) => Some((a.to_string(), v.clone())),
            _ => None,
        });
        // personal under the installed version, or under the version of
        // the event that last wrote it, an attribute a later version no
        // longer marks keeping what it was written as (ADR-0146 decision 2)
        // the events the erasure and its walks read, each by the log's
        // indexes, this request's own among them, where it read the whole
        // log under the writer (ADR-0162 step 1, fourth production review)
        let writes = o.held_parts()?.writes.clone();
        for (a, spec) in &t.attributes {
            if !o.attrs.contains_key(a) {
                continue;
            }
            let personal = spec.personal
                || match writes.get(a) {
                    Some(position) => ctx.work.event_at(*position)?.is_some_and(|e| {
                        ctx.work.personal_under(e.declaration_version, &o.type_, a)
                    }),
                    None => false,
                };
            if !personal {
                continue;
            }
            if spec.stored_set() {
                o.attrs.insert(a.clone(), Value::Set(Vec::new()));
            } else {
                o.attrs.remove(a);
            }
            erased.insert(a.clone());
        }
        // each span personal under the installed version or the version it
        // was written under
        let (id, type_) = (o.id.clone(), o.type_.clone());
        for (a, spans) in o.parts_mut()?.spans.iter_mut() {
            let now_personal = t.attribute(a).is_some_and(|spec| spec.personal);
            for sp in spans {
                if now_personal || ctx.work.personal_under(sp.declaration_version, &type_, a) {
                    sp.value = Value::Absent;
                    // written by the store, which writes only the spans an
                    // event opens or closes, as the walk down's are
                    ctx.redacted_spans
                        .push((id.clone(), a.clone(), sp.entered_position));
                }
            }
        }
        o.parts_mut()?.erased = true;
        let redacted_since = ctx.redacted_inputs.len();
        // what the erased object holds that is personal, the source of
        // every value the walks follow
        let own: Vec<Event> = ctx.work.events_of(&o.id)?;
        let own_versions: BTreeSet<i64> = own.iter().map(|e| e.declaration_version).collect();
        let holding: BTreeSet<String> = t
            .attributes
            .iter()
            .filter(|(a, spec)| {
                spec.personal
                    || own_versions
                        .iter()
                        .any(|v| ctx.work.personal_under(*v, &o.type_, a))
            })
            .map(|(a, _)| a.clone())
            .collect();
        // each input personal under the transition its event recorded,
        // redacted there alone, since an input personal in one transition
        // may not be in another (storage-schema.md §9 step 8)
        for e in &own {
            // the transition as the version its event was written under
            // declared it (storage-schema.md §9 step 2, per version)
            let tv = ctx
                .work
                .type_at(e.declaration_version, &o.type_)
                .unwrap_or(t);
            let Some(ex) = tv
                .transition(&e.transition)
                .or_else(|| t.transition(&e.transition))
            else {
                continue;
            };
            let mut tainted = BTreeSet::new();
            if let Some(recorded) = e.payload["inputs"].as_object() {
                for name in recorded.keys() {
                    if input_personal(tv, ex, name)
                        || tv.attribute(name).is_some_and(|a| a.personal)
                        || input_personal(t, ex, name)
                        || t.attribute(name).is_some_and(|a| a.personal)
                    {
                        ctx.redacted_inputs.push((e.position, name.clone()));
                        tainted.insert(name.clone());
                    }
                }
            }
            // and what flowed down from it into other objects (§9 step 12)
            walk_down(ctx, e, &tainted, &holding)?;
            // and the callers that passed one on (§9 step 3)
            caller_taint(ctx, e, tainted)?;
        }
        // every other object whose value it redacted gets an event
        // (DESIGN.md §8)
        redact_others(ctx, alloc, &o.id, redacted_since)?;
        if let Some((a, v)) = identity
            && erased.contains(&a)
        {
            ctx.renamed.push((v, o.id.clone()));
        }
        ctx.erased.entry(o.id.clone()).or_default().extend(erased);
        invalidate_proposals(ctx, alloc, &o.id)?;
    }
    // an assertion puts the object into the state the request names
    if x.kind == Kind::Assertion
        && let Some(Value::Text(to)) = inputs.get("to")
    {
        o.state = to.clone();
    }
    for k in x.required_inputs.iter().chain(&x.optional_inputs) {
        if let Some(v) = inputs.get(k) {
            o.attrs
                .insert(k.clone(), at_declared_scale(t, k, v.clone(), &o.id)?);
        }
    }
    // the event records every input the transition took, those that write
    // an attribute and its declared ones alike, personal ones until an
    // erasure redacts them (storage-schema.md §4, §9 step 8)
    let written: serde_json::Map<String, Json> = inputs
        .iter()
        .map(|(k, v)| (k.clone(), wire::encode(v)))
        .collect();
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
                .get(c)?
                .map_or(Value::Timestamp(now), |c| c.attr("occurred_at")),
            _ => Value::Timestamp(ctx.at),
        };
        o.attrs.insert("occurred_at".into(), occurred);
        // a label holds its subject's state when it was applied (DESIGN.md
        // §5.11); a label is requested, never created by an outcome, so
        // nothing has changed the subject in the request before it
        if tn == crate::model::LABEL
            && let Some(Value::Object(subject)) = inputs.get("subject")
            && let Some(s) = ctx.work.get(subject)?
        {
            o.attrs
                .insert("subject_state".into(), Value::Text(s.state.clone()));
        }
    }
    ctx.work.insert(o)?;
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
        ctx.work.imported()?.clone()
    };
    let Some(o) = ctx.work.get_mut(&id)? else {
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
            && let Some((since, gone)) =
                correct_span(lib, t, o.parts_mut()?, m, &value, position, &imported)
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
    if !record_spans(lib, t, o, before.as_ref(), position, ctx.at, &ctx.who)? {
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
        && let Some(st) = ctx.work.get(&subject)?.and_then(|s| lib.type_(&s.type_))
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
        if let Some(target) = ctx.work.get_mut(&object)? {
            target
                .parts_mut()?
                .writes
                .insert(attribute.clone(), position);
        }
        ctx.index.push((object, attribute, position));
    }
    let Some(o) = ctx.work.get(&id)? else {
        return stop_not_run(format!("the object {id}, which its own effect removed"));
    };
    // an external identifier it wrote: `lookup` finds the object by the new
    // value, and no longer by the old (declaration-syntax.md §4, DESIGN.md §10)
    for (a, spec) in &t.attributes {
        let Some(source) = &spec.external else {
            continue;
        };
        let was = before_attrs.get(a).cloned().unwrap_or(Value::Absent);
        let now = o.attr(a);
        if was == now {
            continue;
        }
        if let Some(v) = external_text(&was) {
            ctx.external_removed.push((source.clone(), v, id.clone()));
        }
        if let Some(v) = external_text(&now) {
            ctx.external
                .push((source.clone(), v, id.clone(), a.clone()));
        }
    }
    let event = Event {
        position,
        retries: 0,
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
        context: ctx.context.clone(),
        imported: false,
        // an erasure names what it erased, and never the values
        payload: if x.kind == Kind::Erasure {
            {
                let mut p = json!({ "erased": ctx.erased.get(&id).cloned().unwrap_or_default() });
                // and how many legacy entries it redacted (§9 step 6, ADR-0078)
                let entries = ctx.work.legacy_of(&id)?.len();
                if entries > 0 {
                    p["legacy_entries"] = json!(entries);
                }
                p
            }
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
    ctx.work.appended.push(event.clone());
    ctx.events.push(event);
    ctx.causes.pop();
    Ok(id)
}

/// An external identifier's value as `of_external_id` keeps it: text, or a
/// number written out; none where it is absent.
pub(crate) fn external_text(v: &Value) -> Option<String> {
    match v {
        Value::Absent => None,
        Value::Text(t) => Some(t.clone()),
        other => Some(match wire::encode(other) {
            Json::String(s) => s,
            j => j.to_string(),
        }),
    }
}

/// The callers that passed a personal value on (`storage-schema.md` §9 step
/// 3): up an event's cause chain, each caller's inputs that the step taking
/// the event's transition mapped into one of its tainted inputs, redacted in
/// the caller's event and tainted in turn, until a level passes none on. A
/// call's step is matched by the transition it takes, its target's type not
/// inferred, so two calls of one transition name on different types taint
/// both, which redacts more and never less; the installed version's steps are
/// read, as for every erasure here (TODO: an earlier version's).
fn caller_taint(
    ctx: &mut Ctx<'_>,
    from: &Event,
    mut tainted: BTreeSet<String>,
) -> Result<(), NotRun> {
    let lib = ctx.work.lib;
    let mut callee = from.clone();
    while !tainted.is_empty() {
        let Some(caller) = (match callee.cause {
            Some(c) => ctx.work.event_at(c)?,
            None => None,
        }) else {
            break;
        };
        let Some(cx) = ctx
            .work
            .get(&caller.object_id)?
            .and_then(|o| lib.type_(&o.type_))
            .and_then(|t| t.transition(&caller.transition))
        else {
            break;
        };
        let callee_type = ctx.work.get(&callee.object_id)?.map(|o| o.type_.clone());
        let mut passed = BTreeSet::new();
        for (input, exprs) in
            steps_taking(lib, &cx.effect, &callee.transition, callee_type.as_deref())
        {
            if tainted.contains(input) {
                for e in exprs {
                    e.inputs_read(&mut passed);
                }
            }
        }
        let recorded = caller.payload["inputs"].as_object();
        for n in &passed {
            if recorded.is_some_and(|r| r.contains_key(n)) {
                ctx.redacted_inputs.push((caller.position, n.clone()));
            }
        }
        tainted = passed;
        callee = caller;
    }
    Ok(())
}

/// What flowed down from an erased value (`storage-schema.md` §9 step 12,
/// ADR-0160): each event the given one caused on another object, the inputs
/// its step filled from what carries the erased value, the event's tainted
/// inputs or an attribute holding it, redacted in the caused event, marked
/// personal or not, since a copy there is none of the limits `DESIGN.md` §8
/// lists; each personal attribute such an input writes cleared on the object
/// while the object still holds what flowed in, its last write being that
/// event; and so on down.
fn walk_down(
    ctx: &mut Ctx<'_>,
    from: &Event,
    tainted: &BTreeSet<String>,
    holding: &BTreeSet<String>,
) -> Result<(), NotRun> {
    let lib = ctx.work.lib;
    let Some(fx) = ctx
        .work
        .get(&from.object_id)?
        .and_then(|o| lib.type_(&o.type_))
        .and_then(|t| t.transition(&from.transition))
    else {
        return Ok(());
    };
    let caused: Vec<Event> = ctx
        .work
        .caused_by(from.position)?
        .into_iter()
        .filter(|f| f.object_id != from.object_id)
        .collect();
    for f in caused {
        let Some(p) = ctx.work.get(&f.object_id)? else {
            continue;
        };
        ctx.work.parts(p)?;
        let p = p.clone();
        let Some(pt) = lib.type_(&p.type_) else {
            continue;
        };
        let Some(px) = pt.transition(&f.transition) else {
            continue;
        };
        let mut passed = BTreeSet::new();
        for (input, exprs) in steps_taking(lib, &fx.effect, &f.transition, Some(&p.type_)) {
            if exprs.iter().any(|e| carries(e, tainted, holding)) {
                passed.insert(input.clone());
            }
        }
        let recorded = f.payload["inputs"].as_object();
        let mut next_inputs = BTreeSet::new();
        let mut next_holding = BTreeSet::new();
        for n in &passed {
            if recorded.is_some_and(|r| r.contains_key(n)) {
                ctx.redacted_inputs.push((f.position, n.clone()));
            }
            next_inputs.insert(n.clone());
            for a in attributes_written(pt, px, n) {
                if !pt.attribute(&a).is_some_and(|s| s.personal) {
                    continue;
                }
                next_holding.insert(a.clone());
                if p.held_parts()?.writes.get(&a) == Some(&f.position) {
                    ctx.cleared.push((p.id.clone(), a, f.position));
                }
            }
        }
        if !next_inputs.is_empty() {
            walk_down(ctx, &f, &next_inputs, &next_holding)?;
        }
    }
    Ok(())
}

/// Whether an expression carries a tainted value: it reads a tainted input,
/// or an attribute holding one, bare or through `this`.
fn carries(e: &Expr, inputs: &BTreeSet<String>, attributes: &BTreeSet<String>) -> bool {
    let mut read = BTreeSet::new();
    e.inputs_read(&mut read);
    read.iter().any(|n| inputs.contains(n)) || reads_attribute(e, attributes)
}

fn reads_attribute(e: &Expr, attributes: &BTreeSet<String>) -> bool {
    match e {
        Expr::Name(n) => attributes.contains(n),
        Expr::Path(b, m) if matches!(b.as_ref(), Expr::Name(t) if t == "this") => {
            attributes.contains(m)
        }
        _ => e
            .children()
            .into_iter()
            .any(|c| reads_attribute(c, attributes)),
    }
}

/// The attributes an input writes: itself where it is an attribute's input,
/// and each location an effect step assigns or adds it to.
fn attributes_written(t: &Type, x: &Transition, input: &str) -> BTreeSet<String> {
    fn walk(steps: &[Step], input: &str, out: &mut BTreeSet<String>) {
        for st in steps {
            match st {
                Step::Assign { location, expr } | Step::Add { location, expr } => {
                    let mut read = BTreeSet::new();
                    expr.inputs_read(&mut read);
                    if read.contains(input) {
                        out.insert(location.clone());
                    }
                }
                Step::Foreach { steps, .. } => walk(steps, input, out),
                _ => {}
            }
        }
    }
    let mut out = BTreeSet::new();
    if x.required_inputs
        .iter()
        .chain(&x.optional_inputs)
        .any(|k| k == input)
        && t.attribute(input).is_some()
    {
        out.insert(input.to_string());
    }
    walk(&x.effect, input, &mut out);
    out
}

/// A `redact` event on every object other than the erased one whose value
/// the erasure redacted, an attribute it held or a value in a past event
/// (`DESIGN.md` §7, §8, ADR-0147): source `erased`, caused by the erasure's
/// event, naming what it redacted and never the values; the object's version
/// advances, and the object is not itself erased. One already erased holds
/// nothing personal to redact, and records none.
fn redact_others(
    ctx: &mut Ctx<'_>,
    alloc: &mut Allocator,
    erased: &str,
    since: usize,
) -> Result<(), NotRun> {
    let mut names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (position, n) in &ctx.redacted_inputs[since..] {
        if let Some(e) = ctx.work.event_at(*position)?
            && e.object_id != erased
        {
            names
                .entry(e.object_id.clone())
                .or_default()
                .insert(n.clone());
        }
    }
    let cleared = std::mem::take(&mut ctx.cleared);
    for (id, a, _) in &cleared {
        if id != erased {
            names.entry(id.clone()).or_default().insert(a.clone());
        }
    }
    for (id, redacted) in names {
        let Some(held) = ctx.work.get(&id)? else {
            continue;
        };
        ctx.work.parts(held)?;
        let mut p = held.clone();
        if p.held_parts()?.erased {
            continue;
        }
        let stored_set = |a: &str| {
            ctx.work
                .lib
                .type_(&p.type_)
                .and_then(|t| t.attribute(a))
                .is_some_and(|s| s.stored_set())
        };
        for (_, a, flowed) in cleared.iter().filter(|(o, _, _)| *o == id) {
            if stored_set(a) {
                p.attrs.insert(a.clone(), Value::Set(Vec::new()));
            } else {
                p.attrs.remove(a);
            }
            // the parts alone, beside the type the closure above reads
            let Some(parts) = p.parts.get_mut() else {
                return Err(NotRun::Unsupported(format!(
                    "the record of {id}, which was not read"
                )));
            };
            if let Some(spans) = parts.spans.get_mut(a) {
                for sp in spans.iter_mut().filter(|sp| sp.entered_position == *flowed) {
                    sp.value = Value::Absent;
                    ctx.redacted_spans.push((id.clone(), a.clone(), *flowed));
                }
            }
        }
        let position = alloc.position();
        p.version += 1;
        let event = Event {
            position,
            retries: 0,
            object_id: id.clone(),
            object_seq: p.version,
            transition: "redact".into(),
            from_state: Some(p.state.clone()),
            to_state: p.state.clone(),
            changes_state: false,
            source: "erased".into(),
            actor_id: ctx.who.id.clone(),
            actor_kind: ctx.who.kind.clone(),
            principal: ctx.who.principal.clone(),
            cause: ctx.causes.last().copied(),
            declaration_version: ctx.work.lib.version,
            recorded_at: ctx.work.now,
            occurred_at: Some(ctx.at),
            context: ctx.context.clone(),
            imported: false,
            payload: json!({ "redacted": redacted }),
            reads: json!({}),
        };
        ctx.work.appended.push(event.clone());
        ctx.events.push(event);
        ctx.work.insert(p)?;
    }
    Ok(())
}

/// The inputs each step that takes a transition maps, a loop's steps
/// included: a creation of the type, or a call of the transition; each with
/// the expressions its value comes from, its own and, where it reads a
/// loop's item, the loop's array, whose element it is.
fn steps_taking<'s>(
    lib: &Library,
    steps: &'s [Step],
    transition: &str,
    type_: Option<&str>,
) -> Vec<(&'s String, Vec<&'s Expr>)> {
    let mut out = Vec::new();
    for st in steps {
        match st {
            Step::Create {
                type_: created,
                transition: tr,
                inputs,
                ..
            } if tr == transition && type_.is_none_or(|t| lib.is_a(t, created)) => {
                out.extend(inputs.iter().map(|(n, e)| (n, vec![e])));
            }
            Step::Call {
                transition: tr,
                inputs,
                ..
            } if tr == transition => {
                out.extend(inputs.iter().map(|(n, e)| (n, vec![e])));
            }
            Step::Foreach {
                item, array, steps, ..
            } => {
                for (n, mut exprs) in steps_taking(lib, steps, transition, type_) {
                    let mut read = BTreeSet::new();
                    for e in &exprs {
                        names_read(e, &mut read);
                    }
                    if read.contains(item)
                        && let Some(a) = array
                    {
                        exprs.push(a);
                    }
                    out.push((n, exprs));
                }
            }
            _ => {}
        }
    }
    out
}

/// Whether an input is personal (`declaration-syntax.md` §10, check 10):
/// marked so, or written by the transition's effect into a personal
/// attribute, which makes it personal without the marking.
pub(crate) fn input_personal(t: &Type, x: &Transition, name: &str) -> bool {
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
    o: &mut crate::world::Parts,
    m: &str,
    value: &Value,
    position: i64,
    imported: &BTreeSet<i64>,
) -> Option<(i64, Option<(i64, i64)>)> {
    type Rows = BTreeMap<(String, i64, i64), Value>;
    let held_of = |o: &crate::world::Parts| -> Rows {
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
) -> Result<bool, NotRun> {
    let tracked = tracked_values(lib, t, o);
    let parts = o.parts_mut()?;
    let changed: Vec<(String, Value)> = tracked
        .into_iter()
        .filter(|(m, v)| {
            let was = before.and_then(|b| b.iter().find(|(n, _)| n == m).map(|(_, w)| w));
            let spans = parts.spans.get(m).map_or(0, Vec::len);
            before.is_none() || spans == 0 || was != Some(v)
        })
        .collect();
    if before.is_some()
        && changed.iter().any(|(m, _)| {
            parts
                .spans
                .get(m)
                .and_then(|s| s.last())
                .is_some_and(|last| now < last.entered_at)
        })
    {
        return Ok(false);
    }
    for (m, v) in changed {
        let spans = parts.spans.entry(m).or_default();
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
    Ok(true)
}

/// Every proposal still pending on an erased object is invalidated, its
/// inputs redacted, the two one event, which is no transition the request
/// took (`DESIGN.md` §8, `storage-schema.md` §9 steps 7 and 11); and every
/// one decided on it loses the inputs personal to its transition, one
/// `redact` event each.
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
        .of_type(PROPOSAL)?
        .iter()
        .filter(|p| p.state == "PENDING" && p.attr("target") == target)
        .map(|p| p.id.clone())
        .collect();
    for id in pending {
        let Some(mut p) = ctx.work.whole(&id)?.cloned() else {
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
        record_spans(lib, pt, &mut p, Some(&before), position, ctx.at, &ctx.who)?;
        p.version += 1;
        let invalidated = Event {
            position,
            retries: 0,
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
            context: ctx.context.clone(),
            imported: false,
            payload: json!({ "redacted": ["inputs"] }),
            reads: json!({}),
        };
        ctx.work.appended.push(invalidated.clone());
        ctx.events.push(invalidated);
        // the inputs leave the proposal's own creation event too
        ctx.erased.entry(id.clone()).or_default().extend(names);
        ctx.work.insert(p)?;
    }
    // and every one decided on it, approved, rejected or withdrawn, keeps its
    // inputs but those personal to the transition it proposed, which an
    // erasure of its object left in the proposal, its history and the log
    // (thirteenth production review A)
    let decided: Vec<String> = ctx
        .work
        .of_type(PROPOSAL)?
        .iter()
        .filter(|p| p.state != "PENDING" && p.attr("target") == target)
        .map(|p| p.id.clone())
        .collect();
    for id in decided {
        let Some(mut p) = ctx.work.whole(&id)?.cloned() else {
            continue;
        };
        let (Value::Text(tt), Value::Text(xn), Value::Text(held)) = (
            p.attr("target_type"),
            p.attr("transition"),
            p.attr("inputs"),
        ) else {
            continue;
        };
        let (Some(t), Ok(mut inputs)) = (
            lib.type_(&tt),
            serde_json::from_str::<serde_json::Map<String, Json>>(&held),
        ) else {
            continue;
        };
        let Some(x) = t.transition(&xn) else {
            continue;
        };
        let personal: BTreeSet<String> = inputs
            .keys()
            .filter(|k| input_personal(t, x, k))
            .cloned()
            .collect();
        if personal.is_empty() {
            continue;
        }
        inputs.retain(|k, _| !personal.contains(k));
        let position = alloc.position();
        let before = tracked_values(lib, pt, &p);
        p.attrs.insert(
            "inputs".into(),
            Value::Text(Json::Object(inputs).to_string()),
        );
        record_spans(lib, pt, &mut p, Some(&before), position, ctx.at, &ctx.who)?;
        p.version += 1;
        let redacted = Event {
            position,
            retries: 0,
            object_id: id.clone(),
            object_seq: p.version,
            transition: "redact".into(),
            from_state: Some(p.state.clone()),
            to_state: p.state.clone(),
            changes_state: false,
            source: "erased".into(),
            actor_id: ctx.who.id.clone(),
            actor_kind: ctx.who.kind.clone(),
            principal: ctx.who.principal.clone(),
            cause: ctx.causes.last().copied(),
            declaration_version: ctx.work.lib.version,
            recorded_at: ctx.work.now,
            occurred_at: Some(ctx.at),
            context: ctx.context.clone(),
            imported: false,
            payload: json!({ "redacted": personal.iter().collect::<Vec<_>>() }),
            reads: json!({}),
        };
        ctx.work.appended.push(redacted.clone());
        ctx.events.push(redacted);
        ctx.erased.entry(id.clone()).or_default().extend(personal);
        ctx.work.insert(p)?;
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
        Some(id) => match world.get(id)? {
            None => return refused(Verdict::NotFound),
            // an object named beside a type is found only as one of that
            // type's family, so a tool bound to a type takes no other type's
            // object of the id it is given: an agent's `customer_revoke`
            // revoked a person's subscription (thirteenth production review B)
            Some(o) if req.type_.as_deref().is_some_and(|t| !lib.is_a(&o.type_, t)) => {
                return refused(Verdict::NotFound);
            }
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
        return refused(Verdict::NotProposable);
    }
    // its inputs as a request checks them: taken, of their type, naming
    // what exists, and none left out
    let mut wrong: Vec<(String, InputReason)> = req
        .inputs
        .keys()
        .filter(|k| !takes(t, x, k))
        .map(|k| (k.clone(), InputReason::UnknownInput))
        .collect();
    for (k, raw) in &req.inputs {
        if wrong.iter().any(|(w, _)| w == k) {
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
                    let mut fits = true;
                    for i in &ids {
                        fits &= match i {
                            Value::Object(id) => {
                                world.get(id)?.is_some_and(|o| lib.is_a(&o.type_, target))
                            }
                            _ => false,
                        };
                    }
                    if !fits {
                        wrong.push((k.clone(), InputReason::NoSuchObject));
                    }
                }
            }
            Err(e) => wrong.push((k.clone(), e.1)),
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
            .map(|k| (k.clone(), InputReason::Missing)),
    );
    wrong.extend(
        x.inputs
            .iter()
            .filter(|(k, spec)| !given(k) && !spec.optional && spec.default.is_none())
            .map(|(k, _)| (k.clone(), InputReason::Missing)),
    );
    if !wrong.is_empty() {
        return refused(Verdict::invalid_input(wrong));
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
    // the proposer by its identity, as every event names its actor, however
    // the request named it (D611)
    attrs.insert("proposer".into(), Value::Text(who.id.clone()));
    if let Some(c) = &req.context {
        attrs.insert("context".into(), Value::Text(c.clone()));
    }
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
        order: world.next_order()?,
        superseded_by: None,
        parts: crate::world::Parts::default().held(),
        state_source: None,
        created_dated: true,
        recorded_from: world.now,
    };
    record_spans(lib, pt, &mut o, None, position, world.now, &who)?;
    o.version = 1;
    let event = Event {
        position,
        retries: 0,
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
        context: req.context.clone(),
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
            replayed: false,
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
) -> Result<Event, NotRun> {
    record_spans(lib, t, o, before, position, now, who)?;
    o.version += 1;
    Ok(Event {
        position,
        retries: 0,
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
        context: None,
        imported: false,
        payload,
        reads: json!({}),
    })
}

/// A change's source from the wire: each file's name to its text, and its
/// reference, refused where empty, longer than 512 characters, or holding a
/// control character (ADR-0135).
/// A change's source, its reference and its digest.
struct ChangeSource {
    files: Vec<(String, String)>,
    reference: String,
    digest: String,
    /// The references it cites, where it gives them (ADR-0159).
    evidence: Option<Vec<Json>>,
}

/// Whether a cited reference is of its form (`library-api.md` §5,
/// `EvidenceRef`, ADR-0159): a metric read's arguments, `{metric, bind?,
/// keep?, over?, filter?}`, or a diagnostics read's, `{diagnostics, over}`,
/// the window as `over`, each duration one the reads take.
fn reference_formed(r: &Json) -> bool {
    let Some(r) = r.as_object() else {
        return false;
    };
    let text = |k: &str| r.get(k).and_then(Json::as_str);
    let named = |k: &str| text(k).is_some_and(|n| !n.trim().is_empty());
    let duration = |k: &str| text(k).is_some_and(|d| crate::value::parse_duration(d).is_some());
    if r.contains_key("metric") {
        named("metric")
            && r.keys()
                .all(|k| ["metric", "bind", "keep", "over", "filter"].contains(&k.as_str()))
            && r.get("bind").is_none_or(Json::is_object)
            && r.get("keep").is_none_or(|k| {
                k.as_array()
                    .is_some_and(|ds| ds.iter().all(|d| d.as_str().is_some_and(|d| !d.is_empty())))
            })
            && (!r.contains_key("over") || duration("over"))
            && (!r.contains_key("filter") || text("filter").is_some())
    } else {
        named("diagnostics")
            && duration("over")
            && r.keys()
                .all(|k| ["diagnostics", "over"].contains(&k.as_str()))
    }
}

fn change_source(req: &Request) -> Result<ChangeSource, Stop> {
    let mut wrong: Vec<String> = Vec::new();
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
    let evidence = match req.inputs.get("evidence") {
        None => None,
        Some(Json::Array(refs)) if refs.iter().all(reference_formed) => Some(refs.clone()),
        Some(_) => {
            wrong.push("evidence".to_string());
            None
        }
    };
    let unknown: Vec<String> = req
        .inputs
        .keys()
        .filter(|k| !["source", "source_ref", "evidence"].contains(&k.as_str()))
        .cloned()
        .collect();
    if !unknown.is_empty() {
        return refused(Verdict::invalid_input(
            unknown.into_iter().map(|k| (k, InputReason::UnknownInput)),
        ));
    }
    if !wrong.is_empty() {
        return refused(Verdict::invalid_input(
            wrong.into_iter().map(|k| (k, InputReason::Malformed)),
        ));
    }
    let Some(digest) = crate::source::digest(&source) else {
        return refused(Verdict::invalid_input([(
            "source".to_string(),
            InputReason::Malformed,
        )]));
    };
    Ok(ChangeSource {
        files: source,
        reference,
        digest,
        evidence,
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
    answers: &BTreeMap<String, String>,
    prepared: Option<&crate::publish::Prepared>,
) -> Result<(Verdict, Option<Change>), Stop> {
    let lib = world.lib;
    let who = request_who(world, req)?;
    let Some(t) = lib.type_(DECLARATION_CHANGE) else {
        return stop_not_run("a store without the built-in DeclarationChange");
    };
    let xn = req.transition.as_str();
    let held = world.get_if(req.object_id.as_deref())?;
    // the change's own record, which its event extends
    if let Some(h) = held {
        world.parts(h)?;
    }
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
        // the approval is its own operation, `publish`, as a supersession
        // is a publish's, where a request of it answered 501 (ninth
        // production review N-2, ADR-0196 decision 7)
        ("publish", true) => {
            stale(world, req)?;
            return refused(Verdict::NotRequestable {
                parents: vec!["publish".into()],
            });
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
    // a change another actor drafted is moved on only by one that may approve
    // the next version, as approving it is refused, where an agent rejected,
    // reopened and revised a person's (ninth production review N-1, ADR-0196
    // decision 5)
    if let Some(h) = held
        && matches!(
            xn,
            "revise" | "submit" | "reopen" | "refresh" | "withdraw" | "reject"
        )
        && h.attr("drafted_by") != Value::Text(who.id.clone())
        && !crate::store::may_approve(lib, &who.kind)
        && !crate::store::is_the_operator(world, &req.actor)?
    {
        return refused_naming(
            Verdict::InvariantViolated {
                failures: vec![crate::verdict::InvariantFailure {
                    object_id: Some(h.id.clone()),
                    type_: DECLARATION_CHANGE.to_string(),
                    invariant: "publish_taken_by".to_string(),
                    remedy: Some(Remedy::Delegable),
                }],
                remedy: Some(Remedy::Delegable),
            },
            &h.id,
        );
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
            order: world.next_order()?,
            superseded_by: None,
            parts: crate::world::Parts::default().held(),
            state_source: None,
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
                evidence,
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
            // the references it cites, replaced where it gives them and kept
            // where it does not (DESIGN.md §5.4); what a submission read
            // goes with the submission (ADR-0159)
            let cites = match &evidence {
                Some(refs) => Some(refs.clone()),
                None => match o.attr("evidence") {
                    Value::Text(t) => serde_json::from_str::<Json>(&t)
                        .ok()
                        .and_then(|e| e["cites"].as_array().cloned()),
                    _ => None,
                },
            };
            match cites {
                Some(refs) => {
                    o.attrs.insert(
                        "evidence".into(),
                        Value::Text(json!({ "cites": refs }).to_string()),
                    );
                }
                None => {
                    o.attrs.remove("evidence");
                }
            }
            if let Some(refs) = evidence {
                payload["evidence"] = Json::Array(refs);
            }
        }
        "submit" | "refresh" => {
            // the dry run prepared on a read beside the writes, where the
            // change and the store's version are what that read saw, and
            // computed here otherwise (DRYRUN-LOCK)
            let report = match prepared.filter(|p| {
                p.change == o.id
                    && Some(p.change_version) == held.map(|h| h.version)
                    && p.at == world.lib.version
            }) {
                Some(p) => p.report.clone(),
                None => {
                    let (source, source_ref, source_digest) = crate::publish::draft_of(&o);
                    crate::publish::dry_run(
                        world,
                        &crate::publish::Draft {
                            change_id: Some(&o.id),
                            source: &source,
                            source_ref: Some(&source_ref),
                            source_digest: Some(&source_digest),
                            actor: &req.actor,
                        },
                    )?
                }
            };
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
            // each reference it cites, read as the store stands, kept as it
            // answered, a refusal as the value it is (ADR-0097 §5, ADR-0159)
            if let Value::Text(t) = o.attr("evidence")
                && let Some(cites) = serde_json::from_str::<Json>(&t)
                    .ok()
                    .and_then(|e| e["cites"].as_array().cloned())
            {
                let mut read = Vec::new();
                for r in &cites {
                    read.push(evidence_read(world, answers, r)?);
                }
                o.attrs.insert(
                    "evidence".into(),
                    Value::Text(json!({ "cites": cites, "read": read }).to_string()),
                );
            }
        }
        _ => {}
    }
    let position = alloc.position();
    alloc.txn();
    if held.is_none() {
        o.id = alloc.object_id(DECLARATION_CHANGE);
    }
    o.state = to.into();
    let mut event = store_event(
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
    )?;
    event.context = req.context.clone();
    let id = o.id.clone();
    let version = o.version;
    Ok((
        Verdict::Satisfied {
            object_id: id,
            version,
            events: vec![position],
            flags: Vec::new(),
            replayed: false,
        },
        Some(Change {
            writes: vec![(o, held.is_none(), event)],
            ..Change::default()
        }),
    ))
}

/// One cited reference read as the store stands, with its as-of time: a
/// metric's page or a type's diagnostics, or the refusal of either, a value
/// (ADR-0159, ADR-0160).
fn evidence_read(
    world: &World<'_>,
    answers: &BTreeMap<String, String>,
    reference: &Json,
) -> Result<Json, Stop> {
    // a diagnostics read, its window as `over` (ADR-0159, ADR-0160)
    if let Some(type_) = reference["diagnostics"].as_str() {
        let window = reference["over"]
            .as_str()
            .and_then(crate::value::parse_duration)
            .unwrap_or(0);
        let mut out = match crate::diagnostics::diagnostics(world, type_, window)
            .map_err(Stop::NotRun)?
        {
            Ok(found) => json!({
                "diagnostics": found.iter().map(crate::diagnostics::Diagnostic::json).collect::<Vec<_>>(),
                "refused": Json::Null,
            }),
            Err(why) => json!({ "diagnostics": [], "refused": why }),
        };
        out["reference"] = reference.clone();
        out["as_of"] = json!(crate::value::timestamp_text(world.now));
        return Ok(out);
    }
    let Some(name) = reference["metric"].as_str() else {
        return stop_not_run("a cited reference that names neither a metric nor diagnostics");
    };
    let bind: BTreeMap<String, Json> = reference["bind"]
        .as_object()
        .map(|b| b.clone().into_iter().collect())
        .unwrap_or_default();
    let keep: Option<Vec<String>> = reference["keep"].as_array().map(|ds| {
        ds.iter()
            .filter_map(|d| d.as_str().map(str::to_string))
            .collect()
    });
    let over = reference["over"]
        .as_str()
        .and_then(crate::value::parse_duration);
    let page = crate::metric::page(
        world,
        Some(answers),
        name,
        &bind,
        keep.as_deref(),
        over,
        reference["filter"].as_str(),
    )
    .map_err(Stop::NotRun)?;
    let mut out = crate::metric::page_json(&page);
    out["reference"] = reference.clone();
    out["as_of"] = json!(crate::value::timestamp_text(world.now));
    Ok(out)
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
    let Some(p) = world.get_if(req.object_id.as_deref())? else {
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
    // a proposal is decided by those who may take its transition: a
    // rejection, and a withdrawal by another than its proposer, by a kind its
    // transition's `taken_by` does not name is refused as approving it would
    // be, where an agent could reject what a person proposed (fifth
    // production review, ADR-0191 decision 1)
    if to != "EXECUTED" {
        let text = |a: &str| match p.attr(a) {
            Value::Text(t) => Some(t),
            _ => None,
        };
        let (tn, xn) = (
            text("target_type").unwrap_or_default(),
            text("transition").unwrap_or_default(),
        );
        let own = to == "WITHDRAWN"
            && text("proposer")
                .map(|by| resolve_actor(world, &by))
                .transpose()?
                .flatten()
                .is_some_and(|by| by.id == who.id);
        if !own
            && let Some(x) = lib.type_(&tn).and_then(|t| t.transition(&xn))
            && !x.taken_by.is_empty()
            && !x.taken_by.contains(&who.kind)
        {
            let remedy = Some(Remedy::Delegable);
            return refused(Verdict::InvariantViolated {
                failures: vec![InvariantFailure {
                    object_id: text("target"),
                    type_: tn,
                    invariant: crate::model::taken_by_names(&xn).1,
                    remedy,
                }],
                remedy,
            });
        }
    }
    if to == "EXECUTED"
        && let Value::Timestamp(expiry) = p.attr("expires_at")
        && world.now >= expiry
    {
        return refused(Verdict::Unavailable {
            state: "expired".into(),
        });
    }
    // the proposed object as its approver read it: one moved since is
    // refused `stale`, so what a person approves is what runs (tenth
    // production review)
    if to == "EXECUTED"
        && let Some(read) = req.target_version
        && let Value::Text(target) = p.attr("target")
        && let Some(now) = world.get(&target)?.map(|o| o.version)
        && now != read
    {
        return refused(Verdict::Stale {
            expected: read,
            actual: now,
        });
    }
    // the proposal's own event, first
    let position = alloc.position();
    alloc.txn();
    // the proposal's own record, which its decision extends
    world.parts(p)?;
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
    )?;
    decided.version += 1;
    let event = Event {
        position,
        retries: 0,
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
        context: req.context.clone(),
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
                replayed: false,
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
        target_version: None,
        principal: req.principal.clone(),
        idempotency_key: None,
        // the proposer's reason, which its execution records
        context: text("context"),
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
                ..
            } => {
                events.push(position);
                events.sort_unstable();
                Verdict::Satisfied {
                    object_id,
                    version,
                    events,
                    flags,
                    replayed: false,
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
            .of_type(kind)?
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
        .of_type(crate::model::LABEL)?
        .iter()
        .filter(|l| l.attr("subject") == subject && l.attr("note") != Value::Absent)
        .map(|l| l.id.clone())
        .collect();
    for l in labels {
        if let Some(label) = ctx.work.get_mut(&l)? {
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
    // the group walked once, by the erasure that began it: each member's
    // erasure walked it again, a chain of 1,201 holding the writer 66.5 s
    // (sixth production review, ADR-0192 decision 7)
    if ctx.in_group {
        return Ok(());
    }
    ctx.in_group = true;
    GROUP_WALKS.with(|n| n.set(n.get() + 1));
    let mut chain = BTreeSet::new();
    let mut todo = vec![id.to_string()];
    while let Some(cur) = todo.pop() {
        if !chain.insert(cur.clone()) {
            continue;
        }
        if let Some(next) = ctx.work.get(&cur)?.and_then(|o| o.superseded_by.clone()) {
            todo.push(next);
        }
        // its predecessors, of the types whose states supersede, the only
        // ones that hold a successor
        // by the index on the successor's column, never each type whole,
        // where a 41-member group held the writer 1.43 s (fifth production
        // review, ADR-0191 decision 5)
        for (tn, t) in &ctx.work.lib.types {
            if t.states.iter().any(|(_, s)| s.superseding) {
                todo.extend(
                    ctx.work
                        .superseded_into(tn, &cur)?
                        .iter()
                        .map(|p| p.id.clone()),
                );
            }
        }
    }
    let mut members: Vec<(i64, String, String)> = Vec::new();
    for m in chain.iter().filter(|m| m.as_str() != id) {
        if let Some(o) = ctx.work.get(m)? {
            members.push((o.order, o.id.clone(), o.type_.clone()));
        }
    }
    members.sort();
    for (_, member, type_) in members {
        let member_erased = match ctx.work.get(&member)? {
            Some(o) => ctx.work.parts(o)?.erased,
            None => false,
        };
        if ctx.erased.contains_key(&member) && member_erased {
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
    ctx.in_group = false;
    Ok(())
}

thread_local! {
    /// How many groups this thread's erasures walked (ADR-0192 decision 7).
    static GROUP_WALKS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// How many supersession groups this thread's erasures walked.
pub fn group_walks() -> u64 {
    GROUP_WALKS.with(std::cell::Cell::get)
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
    if n == IDENTITY_TAKEN {
        return t
            .actor_identity()
            .map(|(a, _)| BTreeSet::from([a.to_string()]))
            .unwrap_or_default();
    }
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

/// How an effect's expression is evaluated: as a value, as a loop's filter,
/// or as a value written to a decimal of the scale given, whose quotient is
/// computed exactly and rounded once at that scale, half to even, where it
/// was rounded at its dividend's scale first (§8.3, the audit's values W4,
/// ADR-0205 decision 1).
#[derive(Clone, Copy)]
enum Evaluated {
    Plain,
    Filtering,
    WrittenAt(u8),
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
    how: Evaluated,
) -> Result<Value, Stop> {
    let ev = Evaluator::new(&ctx.work, Some(t), Some(id))
        .with_committed(&ctx.before)
        .with_inputs(inputs)
        .with_actor(&ctx.who)
        .with_answers(&ctx.answers)
        .with_event(position);
    let v = match (how, e) {
        (Evaluated::Filtering, _) => return Ok(ev.filtering().eval_in(e, env)?),
        // a quotient written to a decimal is divided once at its scale
        (Evaluated::WrittenAt(scale), Expr::Bin(BinOp::Div, l, r)) => {
            ev.quotient_at(l, r, env, scale)?
        }
        _ => ev.eval_in(e, env)?,
    };
    // an effect's value an arithmetic left with none, past its bound or a
    // division by zero, refuses the request, naming its object, where it
    // stopped as not run (production review F5, ADR-0185)
    if v.is_absent() && ev.failed_arithmetic.get() {
        return refused_naming(
            Verdict::Unsatisfied {
                clause: "value_in_range".into(),
                remedy: Some(Remedy::UnreachableFromHere),
            },
            id,
        );
    }
    Ok(v)
}

/// The place in creation order of each object among a loop's items, read
/// before they are sorted, since a comparison cannot stop the read.
fn places_of(world: &World<'_>, items: &[Value]) -> Result<BTreeMap<String, i64>, NotRun> {
    let mut places = BTreeMap::new();
    for i in items {
        if let Value::Object(id) = i
            && !places.contains_key(id)
            && let Some(o) = world.get(id)?
        {
            places.insert(id.clone(), o.order);
        }
    }
    Ok(places)
}

/// The order a loop takes its items in: objects by ascending id, enumeration
/// members in declaration order, other values ascending (`DESIGN.md` §6,
/// ADR-0147), each object's place read before, in `places`.
fn loop_order(
    world: &World<'_>,
    places: &BTreeMap<String, i64>,
    a: &Value,
    b: &Value,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let rank = |v: &Value| -> (u8, i64) {
        match v {
            Value::Object(id) => (0, places.get(id).copied().unwrap_or(i64::MAX)),
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
                let how = t
                    .attribute(location)
                    .and_then(|a| decimal_scale(&a.declared))
                    .map_or(Evaluated::Plain, Evaluated::WrittenAt);
                let v = effect_value(ctx, t, id, inputs, &env, position, expr, how)?;
                // a write that would round a decimal to its target's scale
                // loses what it holds, which §8.3 permits a product or a
                // quotient alone, where `set d2 := d4` rounded 2.5555 to
                // 2.56 (the audit's values W5)
                if let (Evaluated::WrittenAt(scale), Value::Decimal(d)) = (how, &v)
                    && !matches!(expr, Expr::Bin(BinOp::Mul | BinOp::Div, ..))
                    && d.rescale(scale).is_none_or(|r| r != *d)
                {
                    return refused_naming(
                        Verdict::Unsatisfied {
                            clause: "value_in_range".into(),
                            remedy: Some(Remedy::UnreachableFromHere),
                        },
                        id,
                    );
                }
                // an assignment's arithmetic on an absent operand has no
                // value, which refuses the request as one past its bound
                // does, where it answered not run (fourteenth production
                // review N5)
                if v.is_absent() {
                    return refused_naming(
                        Verdict::Unsatisfied {
                            clause: "value_in_range".into(),
                            remedy: Some(Remedy::UnreachableFromHere),
                        },
                        id,
                    );
                }
                // a date or a time past the calendar refuses the request, as
                // an arithmetic past its bound does (`fit_value`)
                let v = at_declared_scale(t, location, v, id)?;
                // a decimal written past its `p` digits refuses the request,
                // as an arithmetic past its bound does (ADR-0185, third
                // production review, minor)
                if let Value::Decimal(d) = &v
                    && let Some(Declared::Scalar(ty)) = t.attribute(location).map(|a| &a.declared)
                    && let Some((digits, _)) = crate::wire::precision_and_scale(ty)
                    && d.units().abs() > 10i128.pow(digits) - 1
                {
                    return refused_naming(
                        Verdict::Unsatisfied {
                            clause: "value_in_range".into(),
                            remedy: Some(Remedy::UnreachableFromHere),
                        },
                        id,
                    );
                }
                // money written past what an input holds, eighteen digits at
                // its currency's minor unit, refuses the request, so what a
                // read answers a request can send (fifth production review,
                // ADR-0191 decision 3)
                if let Value::Money { currency, amount } = &v
                    && amount
                        .rescale(crate::value::minor_unit(currency).unwrap_or(2))
                        .is_none_or(|held| held.units().abs() > crate::wire::INPUT_UNITS)
                {
                    return refused_naming(
                        Verdict::Unsatisfied {
                            clause: "value_in_range".into(),
                            remedy: Some(Remedy::UnreachableFromHere),
                        },
                        id,
                    );
                }
                if let Some(o) = ctx.work.get_mut(id)? {
                    o.attrs.insert(location.clone(), v);
                }
                ctx.wrote
                    .entry(id.to_string())
                    .or_default()
                    .insert(location.clone());
            }
            Step::Clear(names) => {
                if let Some(o) = ctx.work.get_mut(id)? {
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
                let v = effect_value(ctx, t, id, inputs, &env, position, expr, Evaluated::Plain)?;
                // an absent element, or one of another kind than the set
                // holds, refuses the request as an assignment of one does,
                // where it answered not run (the audit's values W6, ADR-0205
                // decision 1)
                let fitted = t
                    .attribute(location)
                    .map_or(Ok(v.clone()), |a| fit_value(&a.declared, false, v));
                let v = match fitted {
                    Ok(v) if !v.is_absent() => v,
                    _ => {
                        return refused_naming(
                            Verdict::Unsatisfied {
                                clause: "value_in_range".into(),
                                remedy: Some(Remedy::UnreachableFromHere),
                            },
                            id,
                        );
                    }
                };
                ctx.wrote
                    .entry(id.to_string())
                    .or_default()
                    .insert(location.clone());
                if let Some(o) = ctx.work.get_mut(id)? {
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
                let target_id = match effect_value(
                    ctx,
                    t,
                    id,
                    inputs,
                    &env,
                    position,
                    target,
                    Evaluated::Plain,
                )? {
                    // a call through an absent optional end reaches nothing (DESIGN.md §5.4)
                    Value::Absent => continue,
                    Value::Object(tid) if ctx.work.get(&tid)?.is_some() => tid,
                    _ => return stop_not_run("a call whose target is not one object"),
                };
                let values = arguments(ctx, t, id, inputs, &env, position, binds)?;
                let ttype = ctx
                    .work
                    .get(&target_id)?
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
                    match effect_value(ctx, t, id, inputs, &env, position, r, Evaluated::Plain)? {
                        Value::Int(n) => (1..=n).map(Value::Int).collect(),
                        // absent or not positive: no step (ADR-0146)
                        _ => Vec::new(),
                    }
                } else if let Some(a) = array {
                    match effect_value(ctx, t, id, inputs, &env, position, a, Evaluated::Plain)? {
                        Value::Set(items) => items,
                        _ => return stop_not_run("a loop over what is not a collection"),
                    }
                } else {
                    Vec::new()
                };
                let places = places_of(&ctx.work, &items)?;
                items.sort_by(|a, b| loop_order(&ctx.work, &places, a, b));
                if let Some(f) = filter {
                    let mut kept = Vec::new();
                    for i in items {
                        let mut inner = env.clone();
                        inner.insert(item.clone(), i.clone());
                        if truth(&effect_value(
                            ctx,
                            t,
                            id,
                            inputs,
                            &inner,
                            position,
                            f,
                            Evaluated::Filtering,
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
                match effect_value(ctx, t, id, inputs, &env, position, expr, Evaluated::Plain)? {
                    Value::Object(successor) => {
                        if let Some(o) = ctx.work.get_mut(id)? {
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
) -> Result<BTreeMap<String, Value>, Stop> {
    let mut out = BTreeMap::new();
    for (k, e) in binds {
        if unsupplied(e, inputs) {
            continue;
        }
        let v = effect_value(ctx, t, id, inputs, env, position, e, Evaluated::Plain)?;
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
            let mut driven: Vec<&Object> = Vec::new();
            for p in &ids {
                if let Some(p) = ctx.work.get(p)?
                    && ctx
                        .work
                        .lib
                        .type_(&p.type_)
                        .and_then(|pt| pt.state(&p.state))
                        .is_none_or(|s| !s.is_final)
                {
                    driven.push(p);
                }
            }
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
) -> Result<(), Stop> {
    for (a, spec) in &t.attributes {
        let Some(ident) = &spec.identifier else {
            continue;
        };
        let scope = match &ident.scope {
            None => String::new(),
            Some(attr) => match o.attr(attr) {
                Value::Absent => {
                    return stop_not_run(format!("minting {}.{a} with its scope absent", t.name));
                }
                v => text_of(&v),
            },
        };
        // a sequence that has drawn its last number refuses the creation,
        // a value past its bound, which names no object, the one created
        // not existing (ADR-0185, D581)
        let Some(n) = alloc.draw(&ident.sequence, &scope) else {
            return refused(Verdict::Unsatisfied {
                clause: "value_in_range".into(),
                remedy: Some(Remedy::UnreachableFromHere),
            });
        };
        after.insert(o.clone())?;
        let field = |name: &str| -> Result<Value, NotRun> {
            let e = crate::expr::parse(name)
                .map_err(|e| NotRun::Unsupported(format!("a format field {name}: {e}")))?;
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
                    // a width a number can fill, where one of a million
                    // panicked in the formatter (tenth production review)
                    if w > crate::model::IDENTIFIER_WIDTH {
                        return not_run(format!(
                            "the format {}, whose width passes {}",
                            ident.format,
                            crate::model::IDENTIFIER_WIDTH
                        ));
                    }
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
                return stop_not_run(format!("the format {}", ident.format));
            };
            match render(&rest[..open])? {
                Some(t) => text.push_str(&t),
                None => {
                    return stop_not_run(format!(
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
                return stop_not_run(format!(
                    "a format field of {}.{a} that is absent outside `[ … ]`",
                    t.name
                ));
            }
        }
        o.attrs.insert(a.clone(), Value::Text(text));
    }
    after.insert(o.clone())?;
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

/// The generated invariant an actor's identity another actor holds fails,
/// named apart from an attribute's `<name>_unique` (ADR-0190 decision 4).
pub(crate) const IDENTITY_TAKEN: &str = "identity_taken";

/// The source a reach's key names an actor's identity by, which no external
/// identifier's source can be, a source being a word.
const IDENTITY_SOURCE: &str = "actor identity";

/// The other actors, of any type whose objects are actors, holding an
/// object's identity, by each type's indexed identity; none where it holds
/// none (ADR-0190 decision 4).
fn identity_holders(world: &World<'_>, o: &Object) -> Result<Vec<String>, NotRun> {
    let Some((a, _)) = world.lib.type_(&o.type_).and_then(|t| t.actor_identity()) else {
        return Ok(Vec::new());
    };
    let Value::Text(mine) = o.attr(a) else {
        return Ok(Vec::new());
    };
    // compared without the case of its ASCII letters, so `ADMIN` and `admin`
    // name one actor (ADR-0192 decision 6), by each actor type's folded
    // column, never its objects whole (ADR-0194 decision 1)
    let mut out = Vec::new();
    for (tn, ot) in &world.lib.types {
        let Some((b, _)) = ot.actor_identity() else {
            continue;
        };
        for other in world.with_folded(tn, b, &mine)? {
            if other.type_ == *tn && other.id != o.id {
                out.push(other.id.clone());
            }
        }
    }
    Ok(out)
}

/// An attribute's uniqueness as the invariants check it: its own, or, for
/// a singular stored end whose opposite is singular too, one to one.
pub(crate) fn effective_unique(lib: &Library, spec: &crate::model::Attribute) -> Unique {
    if spec.unique == Unique::No
        && let (Declared::Reference(target), Some(opposite), false) =
            (&spec.declared, &spec.opposite, spec.many)
    {
        let back = lib.type_(target).and_then(|bt| bt.attribute(opposite));
        if back.is_some_and(|b| matches!(b.declared, Declared::Reference(_)) && !b.many) {
            return Unique::Yes;
        }
    }
    spec.unique.clone()
}

/// The attributes a uniqueness compares, and the filter it holds among,
/// for an attribute whose uniqueness is not `No`.
pub(crate) fn unique_key<'u>(
    a: &str,
    spec: &crate::model::Attribute,
    unique: &'u Unique,
) -> Option<(Vec<String>, Option<&'u Expr>)> {
    Some(match unique {
        Unique::No => return None,
        Unique::Yes => (vec![a.to_string()], None),
        Unique::With(others) => (
            std::iter::once(a.to_string())
                .chain(others.iter().cloned())
                .collect(),
            None,
        ),
        Unique::Where(e) => (vec![a.to_string()], Some(e)),
        // unique within the scope its identifier numbers by
        Unique::InScope => (
            std::iter::once(a.to_string())
                .chain(spec.identifier.as_ref().and_then(|i| i.scope.clone()))
                .collect(),
            None,
        ),
    })
}

/// What a type's invariants read beyond the object they are on, which
/// decides whose invariants a write could break (`DESIGN.md` §5.6,
/// ADR-0045, ADR-0055).
#[derive(Clone, Debug, PartialEq)]
enum Reach {
    /// The object alone.
    Itself,
    /// The objects sharing a key with the one written, which a uniqueness
    /// or an external identifier compares, and those a written object is
    /// reached from along a chain of relationship ends an invariant reads
    /// (`DESIGN.md` §5.6, the traversal form).
    Related {
        keys: Vec<Keyed>,
        chains: BTreeSet<Vec<Hop>>,
    },
    /// What no form here tells, a type scan, a metric or an observation,
    /// taken as any object.
    Anything,
}

/// A hop of a chain an invariant reads, from objects of one type to the
/// objects one of its relationship ends reaches, or to those referring to
/// them.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Hop {
    /// `from.attr`, an end of a relationship of `from`, reaching `to`.
    End {
        from: String,
        attr: String,
        to: String,
    },
    /// `referrers`: any object holding a reference to one of `of`.
    Referrers { of: String },
}

/// A key a type's invariants compare across objects: the attributes of a
/// family a uniqueness compares, or one a family holds an external
/// identifier of a source in, which a written object's own attributes of
/// that source are compared with.
#[derive(Clone, Debug, PartialEq)]
struct Keyed {
    family: String,
    attrs: Vec<String>,
    source: Option<String>,
}

/// A type's invariants' reach: its declared invariants each reading only
/// its own object, or else anything, and the keys its uniquenesses and
/// external identifiers compare. Its state invariants read the object alone.
fn invariant_reach(lib: &Library, t: &Type) -> Reach {
    let own = |e: &Expr| reads_only_this(lib, t, e, &mut BTreeSet::new());
    // an invariant reading related objects is checked from them, along
    // each chain of relationship ends it reads, never by its type whole,
    // which every write of the store swept under the writer, 2.8 s a draft
    // on a go-live's claims (sixth production review P8)
    let mut chains = BTreeSet::new();
    for (_, e) in &t.invariants {
        if own(e) {
            continue;
        }
        match traversals(lib, t, e) {
            Some(found) => chains.extend(found),
            None => return Reach::Anything,
        }
    }
    let mut keys = Vec::new();
    for (a, spec) in &t.attributes {
        let unique = effective_unique(lib, spec);
        if let Some((attrs, filter)) = unique_key(a, spec, &unique) {
            if filter.is_some_and(|e| !own(e)) {
                return Reach::Anything;
            }
            keys.push(Keyed {
                family: lib.unique_scope(t, a).to_string(),
                attrs,
                source: None,
            });
        }
        if let Some(source) = &spec.external {
            for (tn, ot) in &lib.types {
                for (b, bs) in &ot.attributes {
                    if bs.external.as_deref() == Some(source.as_str()) {
                        keys.push(Keyed {
                            family: tn.clone(),
                            attrs: vec![b.clone()],
                            source: Some(source.clone()),
                        });
                    }
                }
            }
        }
    }
    // an actor's identity, which a write of any actor holding the same
    // identity breaks (ADR-0190 decision 4)
    if let Some((a, _)) = t.actor_identity() {
        keys.push(Keyed {
            family: t.name.clone(),
            attrs: vec![a.to_string()],
            source: Some(IDENTITY_SOURCE.to_string()),
        });
    }
    if keys.is_empty() && chains.is_empty() {
        Reach::Itself
    } else {
        Reach::Related { keys, chains }
    }
}

/// What an expression denotes, as far as the chains it reads: no object,
/// the objects at the end of a chain from the invariant's object, of a
/// type, none known for a referrer's, an interval's rows, or a row's value,
/// which may be any object.
#[derive(Clone)]
enum Denotes {
    Nothing,
    Objects(String, Vec<Hop>),
    Rows,
    Opaque,
}

/// Each chain of relationship ends an invariant of a type reads from its
/// object, every prefix of each; none where it reads what no chain tells, a
/// type scan, a metric, an observation or a call this does not know, which
/// is then checked on its type whole.
fn traversals(lib: &Library, t: &Type, e: &Expr) -> Option<BTreeSet<Vec<Hop>>> {
    let mut walk = Walk {
        lib,
        chains: BTreeSet::new(),
        deriving: Vec::new(),
    };
    walk.objects(e, &BTreeMap::new(), &(t.name.clone(), Vec::new()))
        .ok()?;
    Some(walk.chains)
}

struct Walk<'l> {
    lib: &'l Library,
    chains: BTreeSet<Vec<Hop>>,
    /// The derived attributes being walked, so one reading itself is
    /// taken whole.
    deriving: Vec<(String, String)>,
}

/// The members every object has, which read the object alone.
const BUILT_IN_MEMBERS: &[&str] = &[
    "state",
    "id",
    "open",
    "created_at",
    "created_by_kind",
    "closed",
    "category",
    "human",
    "agent",
    "service",
];

impl Walk<'_> {
    fn objects(
        &mut self,
        e: &Expr,
        env: &BTreeMap<String, Denotes>,
        here: &(String, Vec<Hop>),
    ) -> Result<Denotes, ()> {
        Ok(match e {
            Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Dur(..) | Expr::Money(..) => {
                Denotes::Nothing
            }
            Expr::Name(n) => match env.get(n) {
                Some(d) => d.clone(),
                None if n == "this" => Denotes::Objects(here.0.clone(), here.1.clone()),
                None => self.member(&here.0, &here.1, n)?,
            },
            Expr::Path(base, m) => {
                // a state of a type, or a member of an enumeration, by name
                if let Expr::Name(b) = base.as_ref()
                    && !env.contains_key(b)
                    && b != "this"
                    && (b == "state"
                        || (self.lib.type_(&here.0).is_none_or(|ht| {
                            ht.attribute(b).is_none() && !ht.derived.iter().any(|(d, _)| d == b)
                        }) && (self.lib.type_(b).is_some()
                            || self.lib.enumerations.contains_key(b))))
                {
                    return Ok(Denotes::Nothing);
                }
                match self.objects(base, env, here)? {
                    Denotes::Objects(ty, path) => self.member(&ty, &path, m)?,
                    Denotes::Rows => {
                        if ["value", "object", "subject"].contains(&m.as_str()) {
                            Denotes::Opaque
                        } else {
                            Denotes::Nothing
                        }
                    }
                    Denotes::Opaque => return Err(()),
                    Denotes::Nothing => Denotes::Nothing,
                }
            }
            Expr::Call(f, args) => {
                let Expr::Name(n) = f.as_ref() else {
                    return Err(());
                };
                if !["length", "entered_at", "intervals"].contains(&n.as_str()) {
                    return Err(());
                }
                for a in args {
                    self.objects(a, env, here)?;
                }
                if n == "intervals" {
                    Denotes::Rows
                } else {
                    Denotes::Nothing
                }
            }
            Expr::Agg {
                var,
                collection,
                filter,
                body,
                ..
            } => {
                let (Some(var), Some(c)) = (var, collection) else {
                    return Err(());
                };
                let over = match self.objects(c, env, here)? {
                    Denotes::Opaque => return Err(()),
                    d => d,
                };
                let mut inner = env.clone();
                inner.insert(var.clone(), over);
                for x in [filter, body].into_iter().flatten() {
                    self.objects(x, &inner, here)?;
                }
                Denotes::Nothing
            }
            Expr::Metric { .. } => return Err(()),
            Expr::Changed(_, x) => {
                self.objects(x, env, here)?;
                Denotes::Nothing
            }
            Expr::If(c, a, b) => {
                self.objects(c, env, here)?;
                for x in [a, b] {
                    if !matches!(self.objects(x, env, here)?, Denotes::Nothing) {
                        return Err(());
                    }
                }
                Denotes::Nothing
            }
            Expr::Set(_)
            | Expr::Not(_)
            | Expr::Neg(_)
            | Expr::IsNull(..)
            | Expr::In(..)
            | Expr::Bin(..) => {
                for c in e.children() {
                    self.objects(c, env, here)?;
                }
                Denotes::Nothing
            }
        })
    }

    /// A member of the objects at the end of a chain, of a type: a
    /// relationship end extends the chain, a derived attribute is walked
    /// on those objects, and a value or a built-in member ends it.
    fn member(&mut self, ty: &str, path: &[Hop], m: &str) -> Result<Denotes, ()> {
        // a member the type declares first, so one named as a built-in,
        // `category` among them, is never taken for it
        let declared = self
            .lib
            .type_(ty)
            .is_some_and(|t| t.attribute(m).is_some() || t.derived.iter().any(|(d, _)| d == m));
        if !declared && BUILT_IN_MEMBERS.contains(&m) {
            return Ok(Denotes::Nothing);
        }
        if m == "referrers" {
            let mut chain = path.to_vec();
            chain.push(Hop::Referrers { of: ty.to_string() });
            self.chains.insert(chain.clone());
            return Ok(Denotes::Objects(String::new(), chain));
        }
        // a referrer's type is not known, so only its built-ins are read
        let Some(t) = self.lib.type_(ty) else {
            return Err(());
        };
        if let Some(spec) = t.attribute(m) {
            if let Declared::Reference(to) = &spec.declared {
                let mut chain = path.to_vec();
                chain.push(Hop::End {
                    from: ty.to_string(),
                    attr: m.to_string(),
                    to: to.clone(),
                });
                self.chains.insert(chain.clone());
                return Ok(Denotes::Objects(to.clone(), chain));
            }
            return Ok(Denotes::Nothing);
        }
        if let Some((_, d)) = t.derived.iter().find(|(n, _)| n == m) {
            let key = (ty.to_string(), m.to_string());
            if self.deriving.contains(&key) {
                return Err(());
            }
            self.deriving.push(key);
            let found = self.objects(d, &BTreeMap::new(), &(ty.to_string(), path.to_vec()));
            self.deriving.pop();
            return found;
        }
        if t.state(m).is_some() || t.states.iter().any(|(_, s)| s.category == m) {
            return Ok(Denotes::Nothing);
        }
        Err(())
    }
}

/// The objects of a step's `from` that reach an object through the step:
/// through the end the object stores, its opposite, and through the end
/// `from` stores, by its column and its index; for `referrers`, the
/// objects it refers to.
fn back_from(work: &World<'_>, step: &Hop, w: &Object) -> Result<BTreeSet<String>, NotRun> {
    let lib = work.lib;
    let ids = |v: &Value| -> Vec<String> {
        match v {
            Value::Object(id) => vec![id.clone()],
            Value::Set(items) => items
                .iter()
                .filter_map(|x| match x {
                    Value::Object(id) => Some(id.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    let mut out = BTreeSet::new();
    match step {
        Hop::End { from, attr, to } => {
            if !lib.is_a(&w.type_, to) {
                return Ok(out);
            }
            let Some(spec) = lib.type_(from).and_then(|ft| ft.attribute(attr)) else {
                return Ok(out);
            };
            if let Some(back) = &spec.opposite {
                out.extend(ids(&w.attr(back)));
            }
            if !spec.derived_end() {
                let named = Value::Object(w.id.clone());
                let by: Vec<(String, Value)> = if spec.many {
                    Vec::new()
                } else {
                    vec![(attr.clone(), named.clone())]
                };
                for x in work.of_type_where(from, None, &by)? {
                    if ids(&x.attr(attr)).contains(&w.id) {
                        out.insert(x.id.clone());
                    }
                }
            }
        }
        Hop::Referrers { of } => {
            if let Some(wt) = lib.type_(&w.type_) {
                for (a, spec) in &wt.attributes {
                    if matches!(spec.declared, Declared::Reference(_)) {
                        for id in ids(&w.attr(a)) {
                            if work.get(&id)?.is_some_and(|o| lib.is_a(&o.type_, of)) {
                                out.insert(id);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Whether an expression on an object of a type reads that object alone:
/// its stored values, state and flow data, constants, and derived
/// attributes that read it alone. A reference end with an opposite, which
/// may be read from the other side, a path through another object, an
/// aggregate, a metric, and any call or name this does not know are taken
/// as reading more.
fn reads_only_this(lib: &Library, t: &Type, e: &Expr, seen: &mut BTreeSet<String>) -> bool {
    let name = |n: &str, seen: &mut BTreeSet<String>| -> bool {
        if n == "this" || ["state", "id", "open", "created_at", "created_by_kind"].contains(&n) {
            return true;
        }
        if let Some(spec) = t.attribute(n) {
            return !(matches!(spec.declared, Declared::Reference(_)) && spec.opposite.is_some());
        }
        if let Some((_, d)) = t.derived.iter().find(|(d, _)| d == n) {
            // a derived attribute read twice in one walk is a cycle, taken whole
            return seen.insert(n.to_string()) && reads_only_this(lib, t, d, seen);
        }
        if t.observations.iter().any(|(c, _)| c == n) {
            return false;
        }
        t.state(n).is_some()
            || n == "closed"
            || t.states.iter().any(|(_, s)| s.category == n)
            || ["human", "agent", "service"].contains(&n)
    };
    match e {
        Expr::Num(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Dur(..) | Expr::Money(..) => true,
        Expr::Name(n) => name(n, seen),
        Expr::Path(base, m) => match base.as_ref() {
            Expr::Name(b) if b == "this" => name(m, seen),
            Expr::Name(b) if b == "state" => m == "category",
            // a state of a type, or a member of an enumeration, by name
            Expr::Name(b) if t.attribute(b).is_none() && !t.derived.iter().any(|(d, _)| d == b) => {
                lib.type_(b).is_some_and(|bt| bt.state(m).is_some())
                    || lib
                        .enumerations
                        .get(b)
                        .is_some_and(|ms| ms.iter().any(|x| x == m))
            }
            _ => false,
        },
        Expr::Call(f, args) => {
            matches!(f.as_ref(), Expr::Name(n) if ["length", "entered_at", "intervals"].contains(&n.as_str()))
                && args.iter().all(|a| reads_only_this(lib, t, a, seen))
        }
        Expr::Agg { .. } | Expr::Metric { .. } => false,
        Expr::Changed(_, e) => reads_only_this(lib, t, e, seen),
        Expr::Set(_)
        | Expr::Not(_)
        | Expr::Neg(_)
        | Expr::IsNull(..)
        | Expr::In(..)
        | Expr::Bin(..)
        | Expr::If(..) => e
            .children()
            .into_iter()
            .all(|c| reads_only_this(lib, t, c, seen)),
    }
}

/// The objects whose invariants a request's writes could break, in
/// ascending id order: each object it wrote; for a type whose invariants
/// compare a key, every object sharing one with what it wrote, before the
/// write or after it, read by the key; and every object of a type whose
/// reach is not told (`DESIGN.md` §5.6, ADR-0045, ADR-0162 step 3). A kind's
/// invariants are checked on the recording alone (§5.11, ADR-0146), so an
/// observation is never reached from another object.
fn affected_by_writes(work: &World<'_>, before: &World<'_>) -> Result<Vec<String>, NotRun> {
    let lib = work.lib;
    let mut affected: BTreeMap<String, i64> = BTreeMap::new();
    // each object written, as it is now and as the store held it
    let mut versions: Vec<&Object> = Vec::new();
    let written: Vec<String> = work.written().cloned().collect();
    for id in &written {
        if let Some(o) = work.get(id)? {
            affected.insert(o.id.clone(), o.order);
            versions.push(o);
        }
        if let Some(o) = before.get(id)? {
            versions.push(o);
        }
    }
    if versions.is_empty() {
        return Ok(Vec::new());
    }
    for (tn, t) in &lib.types {
        if t.observation_on.is_some() {
            continue;
        }
        match invariant_reach(lib, t) {
            Reach::Itself => {}
            Reach::Anything => {
                for o in work.of_type(tn)? {
                    affected.insert(o.id.clone(), o.order);
                }
            }
            Reach::Related { keys, chains } => {
                // back along each chain from each object written, as it is
                // and as it was, to the objects of this type it is reached
                // from (`DESIGN.md` §5.6)
                for chain in &chains {
                    let Some((last, rest)) = chain.split_last() else {
                        continue;
                    };
                    let mut reached = BTreeSet::new();
                    for w in &versions {
                        reached.extend(back_from(work, last, w)?);
                    }
                    for step in rest.iter().rev() {
                        let mut next = BTreeSet::new();
                        for id in &reached {
                            if let Some(o) = work.get(id)? {
                                next.extend(back_from(work, step, o)?);
                            }
                        }
                        reached = next;
                    }
                    for id in reached {
                        if let Some(o) = work.get(&id)?
                            && lib.is_a(&o.type_, tn)
                        {
                            affected.insert(o.id.clone(), o.order);
                        }
                    }
                }
                for k in &keys {
                    // an actor's identity, compared without the case of its
                    // ASCII letters, so every actor holding it in any case is
                    // checked again (ADR-0192 decision 6), each found by the
                    // folded column (ADR-0194 decision 1)
                    if k.source.as_deref() == Some(IDENTITY_SOURCE) {
                        let wanted: BTreeSet<String> = versions
                            .iter()
                            .filter_map(|w| {
                                let wt = lib.type_(&w.type_)?;
                                let (a, _) = wt.actor_identity()?;
                                match w.attr(a) {
                                    Value::Text(t) => Some(t.to_ascii_lowercase()),
                                    _ => None,
                                }
                            })
                            .collect();
                        for v in &wanted {
                            for o in work.with_folded(&k.family, &k.attrs[0], v)? {
                                affected.insert(o.id.clone(), o.order);
                            }
                        }
                        continue;
                    }
                    for w in &versions {
                        let values: Vec<Vec<Value>> = match &k.source {
                            None if lib.is_a(&w.type_, &k.family) => {
                                vec![k.attrs.iter().map(|a| w.attr(a)).collect()]
                            }
                            None => Vec::new(),
                            // an actor's identity, read from the written
                            // object's own identity attribute (ADR-0190
                            // decision 4)
                            Some(source) if source == IDENTITY_SOURCE => lib
                                .type_(&w.type_)
                                .and_then(|wt| wt.actor_identity())
                                .map(|(a, _)| vec![vec![w.attr(a)]])
                                .unwrap_or_default(),
                            Some(source) => lib
                                .type_(&w.type_)
                                .map(|wt| {
                                    wt.attributes
                                        .iter()
                                        .filter(|(_, s)| {
                                            s.external.as_deref() == Some(source.as_str())
                                        })
                                        .map(|(a, _)| vec![w.attr(a)])
                                        .collect()
                                })
                                .unwrap_or_default(),
                        };
                        for key in values {
                            if key.iter().any(Value::is_absent) {
                                continue;
                            }
                            for o in work.with_key(&k.family, &k.attrs, &key)? {
                                affected.insert(o.id.clone(), o.order);
                            }
                        }
                    }
                }
            }
        }
    }
    let mut ids: Vec<(i64, String)> = affected.into_iter().map(|(id, n)| (n, id)).collect();
    ids.sort();
    Ok(ids.into_iter().map(|(_, id)| id).collect())
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
        let unique = effective_unique(world.lib, spec);
        let Some((with, filter)) = unique_key(a, spec, &unique) else {
            continue;
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
        // unique across the family of the type declaring it; what it read is
        // a scan of that family, its matches the objects holding the same
        // key, none where it holds (ADR-0088, ADR-0158)
        let scope = world.lib.unique_scope(t, a);
        let mut matched = Vec::new();
        // the objects whose columns hold the key, read by it, each then
        // judged as the scan judged it (ADR-0162 step 3)
        for other in world.with_key(scope, &with, &mine)? {
            if other.id != o.id
                && key(other)?.is_some_and(|k| k.iter().zip(&mine).all(|(x, y)| x.equals(y)))
            {
                matched.push(other);
            }
        }
        let rule = format!("{a}_unique");
        record_scan(reading, &rule, scope, &matched);
        if !matched.is_empty() {
            out.push(rule);
        }
    }
    // an external identifier is unique per source, across every type that
    // holds one of that source, an absent value excepted
    // (declaration-syntax.md §4)
    for (a, spec) in &t.attributes {
        let Some(source) = &spec.external else {
            continue;
        };
        let mine = o.attr(a);
        let name = format!("{a}_unique");
        if mine.is_absent() || out.contains(&name) {
            continue;
        }
        // the objects holding the same identifier, read from the types that
        // hold one of the source, each object from its own type's read
        let mut holders: Vec<&Object> = Vec::new();
        for (tn, ot) in &world.lib.types {
            let of_source: Vec<&String> = ot
                .attributes
                .iter()
                .filter(|(_, bs)| bs.external.as_deref() == Some(source.as_str()))
                .map(|(b, _)| b)
                .collect();
            if of_source.is_empty() {
                continue;
            }
            for b in &of_source {
                for other in
                    world.with_key(tn, std::slice::from_ref(*b), std::slice::from_ref(&mine))?
                {
                    if other.type_ == *tn
                        && other.id != o.id
                        && !holders.iter().any(|h| h.id == other.id)
                    {
                        holders.push(other);
                    }
                }
            }
        }
        // a scan of every type holding an identifier of the source, by type
        for (tn, ot) in &world.lib.types {
            if ot
                .attributes
                .iter()
                .any(|(_, bs)| bs.external.as_deref() == Some(source.as_str()))
            {
                record_scan(reading, &name, tn, &[]);
            }
        }
        for h in &holders {
            record_scan(reading, &name, &h.type_, &[h]);
        }
        if !holders.is_empty() {
            out.push(name);
        }
    }
    // an actor's identity names one actor, across every type whose objects
    // are actors, where the first made of two answered for both, an agent
    // passing as a person (fifth production review IDENTITY, ADR-0190
    // decision 4)
    if let Some((a, _)) = t.actor_identity()
        && !out.contains(&format!("{a}_unique"))
        && !identity_holders(world, o)?.is_empty()
    {
        out.push(IDENTITY_TAKEN.to_string());
    }
    Ok(out)
}

/// A uniqueness check's scan, recorded as a type scan's is: under its rule
/// and the type scanned, the objects it matched, read at their versions,
/// and an empty match recorded empty (ADR-0088, ADR-0158).
fn record_scan(
    reading: Option<&std::cell::RefCell<Reading>>,
    rule: &str,
    type_: &str,
    matched: &[&Object],
) {
    let Some(r) = reading else { return };
    let mut r = r.borrow_mut();
    for m in matched {
        r.objects.insert(m.id.clone(), m.version);
    }
    r.scans
        .entry(rule.to_string())
        .or_default()
        .entry(type_.to_string())
        .or_default()
        .extend(matched.iter().map(|m| m.id.clone()));
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
/// runner infers it: `delegable` where the transition writes a value from
/// the actor that the invariant reads (ADR-0173); else `self_serviceable`
/// where it takes an input it writes to a value the invariant reads, an
/// attribute input writing its attribute and an assertion's `to` the state;
/// else `dependent` where the invariant reads another object, or `this`; else
/// `unreachable_from_here`. What it writes to an object it creates or calls is
/// that step's inputs and the taken transition's effect, one hop away (D547).
fn invariant_remedy(
    world: &World<'_>,
    x: &Transition,
    obj: &Object,
    n: &str,
    requested: &str,
    mine: &str,
) -> Option<Remedy> {
    let lib: &Library = world.lib;
    let unique_other = obj.id != requested && n.ends_with("_unique") && mine == obj.type_;
    let t = lib.type_(&obj.type_)?;
    let (reads, other): (BTreeSet<String>, bool) = if n == IDENTITY_TAKEN {
        // what another actor holds, as a uniqueness reads it
        (invariant_reads(t, n), true)
    } else if let Some(a) = n.strip_suffix("_unique") {
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
                // a scan relative to the object reads others
                other = true;
                continue;
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
    let (writes, by_actor) = writes_of(lib, x, &obj.type_, obj.id == requested || unique_other);
    if reads.iter().any(|r| by_actor.contains(r)) {
        Some(Remedy::Delegable)
    } else if reads.iter().any(|r| writes.contains(r)) {
        Some(Remedy::SelfServiceable)
    } else if other {
        Some(Remedy::Dependent)
    } else {
        Some(Remedy::UnreachableFromHere)
    }
}

/// Whether an offer of transition x on type t may be refused for who takes
/// it: x writes a value from the actor that an invariant of t reads, or of a
/// type it creates, one hop away; `availability` runs no effects, so it
/// cannot tell, and says so (ADR-0173 decision 1).
pub fn actor_dependent(lib: &Library, t: &Type, x: &Transition) -> bool {
    let read_by = |t: &Type| {
        let mut names = BTreeSet::new();
        for (_, e) in &t.invariants {
            names_read(e, &mut names);
        }
        // a derived attribute an invariant reads reads what it reads
        let mut todo: Vec<String> = names.iter().cloned().collect();
        while let Some(m) = todo.pop() {
            if let Some((_, d)) = t.derived.iter().find(|(n, _)| *n == m) {
                let mut more = BTreeSet::new();
                names_read(d, &mut more);
                for n in more {
                    if names.insert(n.clone()) {
                        todo.push(n);
                    }
                }
            }
        }
        names
    };
    let (_, own) = writes_of(lib, x, &t.name, true);
    if own.iter().any(|w| read_by(t).contains(w)) {
        return true;
    }
    fn created(steps: &[Step], out: &mut BTreeSet<String>) {
        for st in steps {
            match st {
                Step::Create { type_, .. } => {
                    out.insert(type_.clone());
                }
                Step::Foreach { steps, .. } => created(steps, out),
                _ => {}
            }
        }
    }
    let mut kinds = BTreeSet::new();
    created(&x.effect, &mut kinds);
    kinds.iter().filter_map(|k| lib.type_(k)).any(|t2| {
        let (_, by) = writes_of(lib, x, &t2.name, false);
        by.iter().any(|w| read_by(t2).contains(w))
    })
}

/// What transition x writes to an object of type tn, from its inputs and
/// from its actor: to the requested object, its inputs and its effect's
/// writes; to an object it creates or calls, one hop away, that step's
/// inputs and the effect of the transition the step takes (`DESIGN.md` §5.5,
/// D547, ADR-0173; `flowrun.py`'s `writes_of`).
fn writes_of(
    lib: &Library,
    x: &Transition,
    tn: &str,
    requested: bool,
) -> (BTreeSet<String>, BTreeSet<String>) {
    // an effect's step that writes a value computed from an input writes
    // that input to it (ADR-0122 decisions 33 and 36), and one computed from
    // the actor writes the actor to it
    fn effect(steps: &[Step], writes: &mut BTreeSet<String>, by_actor: &mut BTreeSet<String>) {
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
                    if read.contains("actor") {
                        by_actor.insert(location.clone());
                    }
                }
                Step::Foreach { steps, .. } => effect(steps, writes, by_actor),
                _ => {}
            }
        }
    }
    fn caused(
        lib: &Library,
        steps: &[Step],
        tn: &str,
        writes: &mut BTreeSet<String>,
        by_actor: &mut BTreeSet<String>,
    ) {
        for st in steps {
            let (created, transition, inputs) = match st {
                Step::Foreach { steps, .. } => {
                    caused(lib, steps, tn, writes, by_actor);
                    continue;
                }
                Step::Create {
                    type_,
                    transition,
                    inputs,
                    ..
                } => (Some(type_.as_str()), transition, inputs),
                Step::Call {
                    transition, inputs, ..
                } => (None, transition, inputs),
                _ => continue,
            };
            let Some(taken) = lib.type_(tn).and_then(|t| t.transition(transition)) else {
                continue;
            };
            if created.is_some_and(|c| c != tn) {
                continue;
            }
            for (name, e) in inputs {
                let mut read = BTreeSet::new();
                names_read(e, &mut read);
                if read.contains("inputs") {
                    writes.insert(name.clone());
                }
                if read.contains("actor") {
                    by_actor.insert(name.clone());
                }
            }
            for st2 in &taken.effect {
                if let Step::Assign { location, expr }
                | Step::Add { location, expr }
                | Step::Remove { location, expr } = st2
                {
                    let mut read = BTreeSet::new();
                    names_read(expr, &mut read);
                    if read.contains("actor") {
                        by_actor.insert(location.clone());
                    }
                }
            }
        }
    }
    let (mut writes, mut by_actor) = (BTreeSet::new(), BTreeSet::new());
    if requested {
        writes.extend(x.required_inputs.iter().chain(&x.optional_inputs).cloned());
        if x.kind == Kind::Assertion {
            writes.insert("state".into());
        }
        effect(&x.effect, &mut writes, &mut by_actor);
    } else {
        caused(lib, &x.effect, tn, &mut writes, &mut by_actor);
    }
    (writes, by_actor)
}

/// A request run against a store, in one transaction: its verdict, or that
/// the first slice does not run what it reaches.
pub fn request(
    store: &mut crate::store::Store,
    req: &Request,
) -> Result<Result<Verdict, NoVerdict>, crate::store::StoreError> {
    Ok(request_told(store, req)?.map(|(v, _)| v))
}

/// What a refusal tells its caller beside its verdict (`library-api.md`
/// §4): its clause unknown or false, the objects its remedy points at,
/// whether the transition requested would take a proposal, which a
/// refusal of a guard of its own, a generated one included, may, and what
/// the clause consulted.
fn told(lib: &Library, world: &World<'_>, req: &Request, v: &Verdict, r: &Refusal) -> Told {
    let requested = match (&req.object_id, &req.type_) {
        (Some(id), _) => world.get(id).ok().flatten().map(|o| o.type_.clone()),
        (None, Some(tn)) => Some(tn.clone()),
        _ => None,
    };
    // whether the transition accepts a proposal, told where a guard or an
    // invariant refuses it, as `taken_by`'s does (DESIGN.md §5.5, D582)
    let proposable = matches!(
        v,
        Verdict::Unsatisfied { .. } | Verdict::InvariantViolated { .. }
    ) && requested
        .and_then(|tn| lib.type_(&tn).and_then(|t| t.transition(&req.transition)))
        .is_some_and(|x| x.proposable);
    // only objects the store holds: one the refused request was creating
    // does not exist, and a later object may take its id (D581)
    let objects = r
        .objects
        .iter()
        .filter(|id| world.get(id).ok().flatten().is_some())
        .cloned()
        .collect();
    Told {
        unknown: r.unknown,
        objects,
        proposable,
        consulted: Some(match &r.consulted {
            Json::Null => Json::Object(serde_json::Map::new()),
            c => c.clone(),
        }),
    }
}

/// A request, its verdict and what its caller is told beside it.
pub fn request_told(
    store: &mut crate::store::Store,
    req: &Request,
) -> Result<Result<(Verdict, Told), NoVerdict>, crate::store::StoreError> {
    let answers = store.answers.clone();
    let prepared = store.prepared.take();
    // what an erasure reads was personal under each earlier version
    store.load_earlier()?;
    let decided = store.transact(|world, alloc| {
        // 2. a key its actor applied before replays that request's verdict,
        // whatever version the retry names, and a different request under it
        // is a fault (DESIGN.md §6, ADR-0041, ADR-0076, ADR-0077); an actor
        // the store does not know has applied nothing, and is refused next
        let keyed = match &req.idempotency_key {
            Some(key) => resolve_actor(world, &req.actor)?.map(|who| (who.id, key.clone())),
            None => None,
        };
        let digest = request_digest(world.lib, req, keyed.as_ref().map(|(a, _)| a.as_str()));
        let held = match &keyed {
            Some((actor, key)) => world.idempotency_of(actor, key)?,
            None => None,
        };
        if let (Some((_, key)), Some((applied, recorded))) = (&keyed, &held) {
            // a request whose inputs an erasure redacted before its digest
            // masked them is matched by its key alone, so the key confirms
            // no guess (fourth production review A-5)
            if *applied != digest && applied != crate::store::ERASED_DIGEST {
                return Err(NoVerdict::KeyReused { key: key.clone() });
            }
            let v = replayed_verdict(recorded)?;
            return Ok(((v, Refusal::default(), false), None));
        }
        let (v, change, r) = decide_as(world, alloc, req, &answers, &None, prepared.as_deref())?;
        // what a key applied is recorded with it, so only what commits replays
        let change = match (change, &keyed) {
            (Some(mut c), Some((actor, key))) => {
                let first = c.writes.iter().map(|(_, _, e)| e.position).min();
                c.idempotency = Some((
                    actor.clone(),
                    key.clone(),
                    digest,
                    first,
                    recorded_verdict(&v),
                ));
                Some(c)
            }
            (change, _) => change,
        };
        // whether it erases, which the store's log is truncated after (D584)
        let erasing = change.as_ref().is_some_and(|c| {
            c.writes.iter().any(|(o, _, e)| {
                world
                    .lib
                    .type_(&o.type_)
                    .and_then(|t| t.transition(&e.transition))
                    .is_some_and(|x| x.kind == crate::model::Kind::Erasure)
            })
        });
        Ok(((v, r, erasing), change))
    });
    // the store's write lock held past the busy timeout and every retry: a
    // race, answered as a verdict and logged as one (ADR-0090, ADR-0178)
    let decided = match decided {
        Err(e) if e.is_busy() => Ok((Verdict::RetriesExhausted, Refusal::default(), false)),
        other => other?,
    };
    let (verdict, refusal, erasing) = match decided {
        Ok(decided) => decided,
        Err(NoVerdict::KeyReused { key }) => {
            // a fault, logged with the remedy a fresh key corrects
            // (ADR-0105 §6)
            let attempt = {
                let world = store.snapshot()?;
                let mut a = refused_attempt(
                    &world,
                    req,
                    &Verdict::Unsatisfied {
                        clause: String::new(),
                        remedy: Some(Remedy::SelfServiceable),
                    },
                    &Refusal::default(),
                )?;
                a.verdict = "key_reused".into();
                a.clause = None;
                a
            };
            store.record_attempt(&attempt)?;
            return Ok(Err(NoVerdict::KeyReused { key }));
        }
        Err(e) => return Ok(Err(e)),
    };
    // a request that did not apply is a row of the attempt log, written in
    // its own short transaction after the rollback (DESIGN.md §6)
    if !matches!(verdict, Verdict::Satisfied { .. }) {
        let attempt = {
            let world = store.snapshot()?;
            refused_attempt(&world, req, &verdict, &refusal)?
        };
        match store.record_attempt(&attempt) {
            // a lock still held keeps the race's own row from the log: the
            // caller is answered all the same
            Err(e) if e.is_busy() && verdict == Verdict::RetriesExhausted => {}
            other => other?,
        }
    }
    // an erasure's values leave the write-ahead log too (D584)
    store.after_erasure(erasing)?;
    let told = {
        let world = store.snapshot()?;
        told(world.lib, &world, req, &verdict, &refusal)
    };
    Ok(Ok((verdict, told)))
}

/// What `check` answers (`library-api.md` §4): the verdict the request
/// would receive, and the guards it did not evaluate, so the verdict is
/// explicitly partial (ADR-0054).
#[derive(Clone, Debug, PartialEq)]
pub struct Checked {
    pub verdict: Verdict,
    pub unevaluated: Vec<String>,
    /// What its refusal would tell beside it (`library-api.md` §4).
    pub told: Told,
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
    let prepared = store.prepared.take();
    let decided = store.simulate(|world, alloc| {
        decide_as(
            world,
            alloc,
            req,
            &BTreeMap::new(),
            &checking,
            prepared.as_deref(),
        )
        .map(|(v, _, r)| {
            let t = told(world.lib, world, req, &v, &r);
            (v, t)
        })
    })?;
    Ok(decided.map(|(verdict, told)| Checked {
        verdict,
        unevaluated: named.borrow().clone(),
        told,
    }))
}

/// Everything a request had read when refused, the objects its outcome and
/// cascades reached included, at the versions it left them (`DESIGN.md` §7).
fn refused_reads(ctx: &Ctx<'_>) -> Result<Json, NotRun> {
    let mut read = ctx.read.clone();
    for e in &ctx.events {
        if let Some(o) = ctx.work.get(&e.object_id)? {
            read.objects.insert(o.id.clone(), o.version);
        }
    }
    Ok(read.record(ctx.work.now, None))
}

/// A refused request's row of the attempt log (`DESIGN.md` §6, §9): who
/// asked, as the request named them where the store holds no such actor,
/// and their kind, `unknown` then; the type, none for an unknown id; the
/// object, none for a refused creation or an unknown id; the transition; the
/// verdict, its clause, the first invariant for violated invariants, and its
/// remedy; whether the clause was unknown; and the request's values, a
/// personal input, and one the transition does not declare, named and
/// withheld (ADR-0105): what was sent under a name no declaration marks, or
/// to a transition there is none of, is no value an erasure could reach
/// (ATTEMPT-INPUTS). An approval its request
/// refused is a row of the proposed transition, on its object, naming the
/// proposal.
fn refused_attempt(
    world: &World<'_>,
    req: &Request,
    verdict: &Verdict,
    refusal: &Refusal,
) -> Result<crate::store::Attempt, NotRun> {
    let lib = world.lib;
    let (asked, proposal) = match refusal.approving.as_deref() {
        Some((p, held)) => (held, Some(p.clone())),
        None => (req, None),
    };
    let (type_, object_id) = match &asked.object_id {
        Some(id) => match world.get(id)? {
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
    let declared = |k: &str| {
        x.is_some_and(|x| {
            x.inputs.iter().any(|(n, _)| n == k)
                || x.required_inputs.iter().any(|n| n == k)
                || x.optional_inputs.iter().any(|n| n == k)
        })
    };
    let mut values = serde_json::Map::new();
    let mut withheld = Vec::new();
    for (k, v) in &asked.inputs {
        if personal(k) || !declared(k) {
            withheld.push(k.clone());
        } else {
            values.insert(k.clone(), v.clone());
        }
    }
    let mut request_values = json!({ "inputs": values });
    if let Some(at) = asked.occurred_at {
        request_values["occurred_at"] = json!(crate::value::timestamp_text(at));
    }
    let who = resolve_actor(world, &req.actor)?;
    let principal = match &req.principal {
        Some(p) => Some(resolve_actor(world, p)?.map_or_else(|| p.clone(), |w| w.id)),
        None => None,
    };
    Ok(crate::store::Attempt {
        at: world.now,
        actor_id: who
            .as_ref()
            .map_or_else(|| req.actor.clone(), |w| w.id.clone()),
        actor_kind: who.map_or_else(|| "unknown".to_string(), |w| w.kind),
        // the principal as resolved, or as named where the store holds none
        principal,
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
        context: req.context.clone(),
        proposal,
        request_values,
        withheld,
        declaration_version: lib.version,
        reads: refusal.reads.clone(),
        consulted: refusal.consulted.clone(),
        txn: 0,
        // a refused call's target, and the verdict it received (ADR-0159)
        call: match verdict {
            Verdict::CallRefused {
                object_id, verdict, ..
            } => json!({ "object": object_id, "verdict": verdict.kind() }),
            _ => Json::Null,
        },
    })
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
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
        };
        request(s, &r).unwrap().unwrap()
    }

    fn created(v: &Verdict) -> String {
        match v {
            Verdict::Satisfied { object_id, .. } => object_id.clone(),
            other => panic!("{other:?}"),
        }
    }

    /// An invariant failing on a part the request creates names its remedy as
    /// one on the requested object does (D547): `delegable` where the
    /// creation writes the actor to a value the invariant reads, since another
    /// actor taking it can pass (ADR-0173).
    #[test]
    fn an_invariant_on_a_created_part_names_a_remedy() {
        const ASKS: &str = r#"
module: asks
categories: [live, closed]
types:
  User:
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states: { ACTIVE: { category: live } }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [login] }
  Ask:
    attributes:
      by: { type: string, optional: true }
    states: { OPEN: { category: live } }
    transitions:
      raise: { kind: initial, to: OPEN, effect: [ { assign: { location: by, expr: actor.id } } ] }
      approve: { kind: internal, from: [OPEN], effect: [ { create: { type: Nod, transition: give, inputs: { ask: this, who: actor.id } } } ] }
  Nod:
    attributes:
      ask: { reference: Ask }
      who: { type: string }
    states: { GIVEN: { category: live } }
    invariants:
      not_own: { expression: who != ask.by }
    transitions:
      give: { kind: initial, to: GIVEN, required_inputs: [ask, who], only_via: [Ask.approve] }
"#;
        let lib = Library::load(&[("asks.yaml".into(), ASKS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
        req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        );
        let ask = created(&req(&mut s, "raise", "ana", ("Ask", true), json!({})));
        match req(&mut s, "approve", "ana", (&ask, false), json!({})) {
            Verdict::InvariantViolated { failures, remedy } => {
                assert_eq!(remedy, Some(Remedy::Delegable));
                assert_eq!(failures[0].invariant, "not_own");
                assert_eq!(failures[0].remedy, Some(Remedy::Delegable));
            }
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
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
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
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
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
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
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
            .attempts()
            .unwrap()
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
        let a = w.attempts().unwrap().last().unwrap();
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
        // the row names the call's target and the verdict it received
        // (ADR-0159)
        assert_eq!(a.call, json!({ "object": line, "verdict": "unsatisfied" }));
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
                target_version: None,
                principal: principal.map(str::to_string),
                idempotency_key: None,
                context: None,
            };
            request(s, &r).unwrap().unwrap()
        };
        // named by its object's id or by the identity it holds, recorded by
        // the identity, and written by the outcome
        for named in [ana.as_str(), "ana"] {
            created(&sign(&mut s, Some(named)));
            let w = s.snapshot().unwrap();
            assert_eq!(
                w.whole(&job).unwrap().unwrap().attr("for_whom"),
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
        let a = w.attempts().unwrap().last().unwrap();
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
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
        };
        let before = |s: &Store| {
            let w = s.snapshot().unwrap();
            (
                w.log().unwrap().count(),
                w.attempts().unwrap().len(),
                w.whole(&fine).unwrap().unwrap().version,
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
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
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
            Verdict::invalid_input([("colour".to_string(), InputReason::UnknownInput)])
        );
        assert_eq!(
            req(
                &mut s,
                "estimate",
                OPERATOR,
                (&job, false),
                json!({"hours": "many"})
            ),
            Verdict::invalid_input([("hours".to_string(), InputReason::Malformed)])
        );
        assert_eq!(
            req(
                &mut s,
                "start",
                OPERATOR,
                (&job, false),
                json!({"engineer": &job})
            ),
            Verdict::invalid_input([("engineer".to_string(), InputReason::NoSuchObject)])
        );
        assert_eq!(
            req(&mut s, "open", OPERATOR, ("Job", true), json!({})),
            Verdict::invalid_input([("title".to_string(), InputReason::Missing)])
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
                replayed,
            } => {
                assert!(!replayed);
                assert_eq!(object_id, job);
                assert_eq!(version, 2);
                assert_eq!(events.len(), 1);
                assert_eq!(
                    flags,
                    vec![Flag {
                        clause: "prioritised".into(),
                        remedy: Some(Remedy::SelfServiceable),
                        // `is not null` is false of an absent value, never
                        // unknown (§8.2)
                        unknown: false,
                        objects: Vec::new(),
                    }]
                );
            }
            other => panic!("{other:?}"),
        }
        let world = s.snapshot().unwrap();
        let j = world.whole(&job).unwrap().unwrap();
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
        let before = s.snapshot().unwrap().in_order().unwrap().len();
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
        assert_eq!(s.snapshot().unwrap().in_order().unwrap().len(), before);
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
            w.whole(&a).unwrap().unwrap().attr("number"),
            Value::Text("ORD-0001".into())
        );
        // a field read from the object, and a segment left out whole where
        // its value is absent
        assert_eq!(
            w.whole(&a).unwrap().unwrap().attr("code"),
            Value::Text("ORD-0001-EU".into())
        );
        assert_eq!(
            w.whole(&b).unwrap().unwrap().attr("code"),
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
        let pos = |id: &str| w.whole(id).unwrap().unwrap().attr("position");
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
            w.whole(&c).unwrap().unwrap().attr("number"),
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
        let j = w.whole(&job).unwrap().unwrap();
        assert_eq!(j.state, "DONE");
        assert_eq!(j.attr("note"), Value::Text("done".into()));
        assert_eq!(j.attr("hours"), Value::Absent);
        // a supersession records its successor
        created(&req(&mut s, "retire", OPERATOR, (&job, false), json!({})));
        let w = s.snapshot().unwrap();
        assert_eq!(
            w.whole(&job).unwrap().unwrap().superseded_by.as_deref(),
            Some(ana.as_str())
        );
    }

    /// An object's record beyond its row, its intervals, write index,
    /// admissions and erasure, is read where an operation first reaches it:
    /// for the one object, or, once a few of a type have been read alone,
    /// for every object of the type the operation holds, as each would be
    /// read alone; and an object without it is refused, never read as one
    /// with no history (ADR-0165).
    #[test]
    fn an_objects_record_is_read_on_first_use_and_a_types_together_past_a_few() {
        use crate::world::BATCH;
        let mut s = store();
        let n = 3 * BATCH;
        let jobs: Vec<String> = (0..n)
            .map(|k| {
                let job = created(&req(
                    &mut s,
                    "open",
                    OPERATOR,
                    ("Job", true),
                    json!({"title": format!("J{k}")}),
                ));
                // records that differ, so one read in place of another shows
                if k % 2 == 0 {
                    s.now += 60;
                    created(&req(
                        &mut s,
                        "estimate",
                        OPERATOR,
                        (&job, false),
                        json!({"hours": 1}),
                    ));
                }
                job
            })
            .collect();
        let alone = |id: &str| {
            let w = s.snapshot().unwrap();
            let o = w.get(id).unwrap().unwrap();
            w.parts(o).unwrap().clone()
        };
        // a read by id and a scan read the rows alone
        let w = s.snapshot().unwrap();
        assert!(w.get(&jobs[0]).unwrap().unwrap().parts.get().is_none());
        let scanned = w.of_type("Job").unwrap();
        assert_eq!(scanned.len(), n);
        let read = || scanned.iter().filter(|o| o.parts.get().is_some()).count();
        assert_eq!(read(), 0);
        // the first few reached are read one by one, as the store holds them
        for (k, o) in scanned.iter().take(BATCH).enumerate() {
            assert_eq!(w.parts(o).unwrap(), &alone(&o.id));
            assert_eq!(read(), k + 1);
        }
        assert!(w.parts(scanned[0]).unwrap().writes.contains_key("hours"));
        assert!(!w.parts(scanned[1]).unwrap().writes.contains_key("hours"));
        // the next reads the rest of the type together, each as it is
        // alone
        w.parts(scanned[BATCH]).unwrap();
        assert_eq!(read(), n);
        for o in &scanned {
            assert_eq!(o.parts.get(), Some(&alone(&o.id)), "{}", o.id);
        }
        // an object without its record is refused, never taken as one
        // with no history
        let fresh = s.snapshot().unwrap();
        let mut unread = fresh.get(&jobs[0]).unwrap().unwrap().clone();
        assert!(unread.parts.get().is_none());
        assert!(matches!(unread.held_parts(), Err(NotRun::Unsupported(_))));
        assert!(matches!(unread.parts_mut(), Err(NotRun::Unsupported(_))));
        let mut bare = World::new(fresh.lib, fresh.now);
        assert!(matches!(bare.parts(&unread), Err(NotRun::Unsupported(_))));
        assert!(matches!(
            bare.insert(unread.clone()),
            Err(NotRun::Unsupported(_))
        ));
        // a change reads it first
        let mut w = s.snapshot().unwrap();
        let changing = w.get_mut(&jobs[0]).unwrap().unwrap();
        assert_eq!(changing.parts_mut().unwrap(), &mut alone(&jobs[0]));
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
        let o = w.whole(&job).unwrap().unwrap();
        let counts: Vec<(&str, usize)> = o
            .held_parts()
            .unwrap()
            .spans
            .iter()
            .map(|(m, v)| (m.as_str(), v.len()))
            .collect();
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
                .what()
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
        let f = w.whole(&fixed).unwrap().unwrap();
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
            Verdict::invalid_input([("corrects".to_string(), InputReason::NotAllowed)])
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
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
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
        let o = w.whole(&job).unwrap().unwrap();
        let working = o.held_parts().unwrap().spans["state"].last().unwrap();
        assert_eq!(working.entered_at, now - day);
        assert_eq!(
            o.held_parts().unwrap().spans["state"][0].left_at,
            Some(now - day)
        );
        let events = s.history(&job).unwrap();
        assert_eq!(events.last().unwrap().occurred_at, Some(now - day));
        assert_eq!(events.last().unwrap().recorded_at, now);
        assert_eq!(at(&mut s, "finish", &job, now), refused);
    }

    #[test]
    fn the_invariants_a_write_could_break_are_checked_and_no_others() {
        let text = r#"
module: caps
categories: [open, closed]
types:
  Order:
    attributes:
      ref: { type: string, unique: true }
      cap: { type: int }
      lines: { reference: "Line[]", opposite: order }
    states:
      OPEN: { category: open }
    transitions:
      place: { kind: initial, to: OPEN, required_inputs: [ref, cap] }
      resize: { kind: internal, from: [OPEN], required_inputs: [cap] }
      rename: { kind: internal, from: [OPEN], required_inputs: [ref] }
  Line:
    attributes:
      order: { reference: Order, opposite: lines }
    states:
      ACTIVE: { category: open }
    invariants:
      order_takes_lines: { description: A line's order takes lines., expression: order.cap > 0 }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [order] }
"#;
        let lib = Library::load(&[("caps.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let place = |s: &mut Store, r: &str, cap: i64| {
            created(&req(
                s,
                "place",
                OPERATOR,
                ("Order", true),
                json!({"ref": r, "cap": cap}),
            ))
        };
        let (a, b) = (place(&mut s, "X", 2), place(&mut s, "Y", 1));
        let line = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("Line", true),
            json!({"order": &a}),
        ));
        let failure = |id: &str, type_: &str, invariant: &str| InvariantFailure {
            object_id: Some(id.into()),
            type_: type_.into(),
            invariant: invariant.into(),
            remedy: None,
        };
        let failures = |v: Verdict| match v {
            Verdict::InvariantViolated { failures, .. } => failures
                .into_iter()
                .map(|f| InvariantFailure { remedy: None, ..f })
                .collect::<Vec<_>>(),
            other => panic!("{other:?}"),
        };
        // a write to the order breaks the invariant of the line reading it
        assert_eq!(
            failures(req(
                &mut s,
                "resize",
                OPERATOR,
                (&a, false),
                json!({"cap": 0})
            )),
            vec![failure(&line, "Line", "order_takes_lines")]
        );
        // a key taken breaks the uniqueness of both orders holding it
        assert_eq!(
            failures(req(
                &mut s,
                "rename",
                OPERATOR,
                (&b, false),
                json!({"ref": "X"})
            )),
            vec![
                failure(&a, "Order", "ref_unique"),
                failure(&b, "Order", "ref_unique")
            ]
        );
        // a creation taking a key breaks the uniqueness of the order holding
        // it and its own, which is named by no id, since the object does not
        // exist and a later one takes its id (D581)
        assert_eq!(
            failures(req(
                &mut s,
                "place",
                OPERATOR,
                ("Order", true),
                json!({"ref": "X", "cap": 1})
            )),
            vec![
                failure(&a, "Order", "ref_unique"),
                InvariantFailure {
                    object_id: None,
                    ..failure("", "Order", "ref_unique")
                }
            ]
        );
        // what reads nothing written is not read
        created(&req(
            &mut s,
            "resize",
            OPERATOR,
            (&b, false),
            json!({"cap": 3}),
        ));
        // a key one order leaves cures the conflict admitted on the other,
        // which is reached by the key the written order held before
        let c = place(&mut s, "Z", 1);
        let conn = s.connection();
        let at: i64 = conn
            .query_row("SELECT MAX(position) FROM of_event", [], |r| r.get(0))
            .unwrap();
        conn.execute("UPDATE t_order SET ref = 'X' WHERE id = ?1", [&c])
            .unwrap();
        for id in [&a, &c] {
            conn.execute(
                "INSERT INTO of_admission (object_id, invariant, position, reason) VALUES (?1, 'ref_unique', ?2, '')",
                rusqlite::params![id, at],
            )
            .unwrap();
        }
        created(&req(
            &mut s,
            "rename",
            OPERATOR,
            (&c, false),
            json!({"ref": "W"}),
        ));
        let w = s.snapshot().unwrap();
        assert!(
            w.whole(&a)
                .unwrap()
                .unwrap()
                .held_parts()
                .unwrap()
                .admissions
                .is_empty()
        );
        assert!(
            w.whole(&c)
                .unwrap()
                .unwrap()
                .held_parts()
                .unwrap()
                .admissions
                .is_empty()
        );
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
        let o = w.whole(&job).unwrap().unwrap();
        assert!(o.held_parts().unwrap().writes["note"] > o.held_parts().unwrap().writes["signed"]);
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
        // without its admission, the state it asserts breaks an invariant,
        // which the caller can remedy, since the assertion's `to` writes the
        // state the invariant reads (library-api.md §4, InvariantFailure)
        assert!(matches!(
            req(
                &mut s,
                "correct",
                OPERATOR,
                (&job, false),
                to("WORKING", &[])
            ),
            Verdict::InvariantViolated {
                remedy: Some(Remedy::SelfServiceable),
                ..
            }
        ));
        // no assertion leaves a state for itself, or admits what it may not
        assert_eq!(
            req(&mut s, "correct", OPERATOR, (&job, false), to("OPEN", &[])),
            Verdict::invalid_input([("to".to_string(), InputReason::NotAllowed)])
        );
        assert_eq!(
            req(
                &mut s,
                "correct",
                OPERATOR,
                (&job, false),
                to("WORKING", &["bounded"])
            ),
            Verdict::invalid_input([("admits".to_string(), InputReason::NotAllowed)])
        );
        created(&req(
            &mut s,
            "correct",
            OPERATOR,
            (&job, false),
            to("WORKING", &["working_invariant"]),
        ));
        let w = s.snapshot().unwrap();
        assert_eq!(w.whole(&job).unwrap().unwrap().state, "WORKING");
        assert!(
            w.whole(&job)
                .unwrap()
                .unwrap()
                .held_parts()
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
                .whole(&job)
                .unwrap()
                .unwrap()
                .held_parts()
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
        let o = w.whole(&job).unwrap().unwrap();
        assert_eq!(o.attr("note"), Value::Absent);
        assert!(
            o.held_parts().unwrap().erased && o.held_parts().unwrap().admissions.contains("noted")
        );
        assert_eq!(
            w.whole(&check).unwrap().unwrap().attr("remark"),
            Value::Absent
        );
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
        let l = w.whole(&tag).unwrap().unwrap();
        assert_eq!(
            (
                l.attr("note"),
                l.attr("name"),
                l.held_parts().unwrap().erased
            ),
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
        let l = w.whole(&own).unwrap().unwrap();
        assert_eq!(
            (l.attr("note"), l.held_parts().unwrap().erased),
            (Value::Absent, true)
        );
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
            .eval(&crate::expr::parse("count(a in Asset)").unwrap())
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
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            };
            request(s, &r).unwrap().unwrap()
        };
        // filing checks the transition and the inputs, not the state; a
        // transition not proposable is refused as such (ADR-0173)
        assert_eq!(
            propose(&mut s, "start", json!({}), None),
            Verdict::NotProposable
        );
        assert_eq!(
            propose(&mut s, "finish", json!({"bogus": 1}), None),
            Verdict::invalid_input([("bogus".to_string(), InputReason::UnknownInput)])
        );
        // an input left out that a generated guard refuses waits for approval
        assert!(matches!(
            propose(&mut s, "annotate", json!({}), None),
            Verdict::Satisfied { .. }
        ));
        let handover = created(&propose(&mut s, "finish", json!({}), None));
        // a pending proposal offers its decisions, guarded by nothing of their
        // own, and one no longer pending refuses them in its state (D535)
        let decisions = |s: &Store, actor: &str| {
            let w = s.snapshot().unwrap();
            crate::reads::availability(&w, actor, &handover)
                .unwrap()
                .unwrap()
                .into_iter()
                .filter(|o| o.creates.is_none())
                .map(|o| (o.transition, o.availability, o.verdict))
                .collect::<Vec<_>>()
        };
        let all = |a: crate::reads::Availability, v: Option<Verdict>| {
            ["approve", "reject", "withdraw"]
                .map(|x| (x.to_string(), a, v.clone()))
                .to_vec()
        };
        assert_eq!(
            decisions(&s, OPERATOR),
            all(crate::reads::Availability::Available, None)
        );
        assert_eq!(
            decisions(&s, "nobody"),
            all(
                crate::reads::Availability::Unavailable,
                Some(Verdict::Unsatisfied {
                    clause: "actor_known".into(),
                    remedy: Some(Remedy::Dependent),
                })
            )
        );
        // approved too soon, it is refused as the request would be, and waits
        assert_eq!(
            req(&mut s, "approve", OPERATOR, (&handover, false), json!({})),
            Verdict::Unavailable {
                state: "OPEN".into()
            }
        );
        assert_eq!(
            s.snapshot()
                .unwrap()
                .whole(&handover)
                .unwrap()
                .unwrap()
                .state,
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
                w.whole(&job).unwrap().unwrap().state.as_str(),
                w.whole(&handover).unwrap().unwrap().state.as_str()
            ),
            ("DONE", "EXECUTED")
        );
        // a decided proposal is decided once, and offers no decision
        assert_eq!(
            req(&mut s, "reject", OPERATOR, (&handover, false), json!({})),
            Verdict::Unavailable {
                state: "EXECUTED".into()
            }
        );
        assert_eq!(
            decisions(&s, OPERATOR),
            all(
                crate::reads::Availability::Unavailable,
                Some(Verdict::Unavailable {
                    state: "EXECUTED".into()
                })
            )
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
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
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
        let p = w.whole(&late).unwrap().unwrap();
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
        let o = w.whole(&j).unwrap().unwrap();
        let t = w.lib.type_("Job").unwrap();
        let spans = &o.held_parts().unwrap().spans["crew"];
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
            .log()
            .unwrap()
            .filter(|e| e.object_id == j)
            .map(|e| crate::eval::transition_row(&w, o, w.parts(o).unwrap(), t, e))
            .collect();
        // the original write's event is not changed, and the start credits
        // the corrected crew
        assert_eq!(
            rows.iter().map(held_crew).collect::<Vec<_>>(),
            [Value::Absent, crew("SOUTH"), crew("SOUTH")]
        );
        // the correction's event names the attribute, the value it replaces,
        // the new value, the span it corrects and the reason
        let e = w.log().unwrap().find(|e| e.position == at).unwrap();
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
        let o = w.whole(&j).unwrap().unwrap();
        let t = w.lib.type_("Job").unwrap();
        let spans = &o.held_parts().unwrap().spans["crew"];
        assert_eq!(
            (
                spans.len(),
                spans[0].value.clone(),
                spans[0].left_at,
                spans[0].corrected_by.is_some()
            ),
            (1, crew("NORTH"), None, true)
        );
        let last = w.log().unwrap().rfind(|e| e.object_id == j).unwrap();
        assert_eq!(
            held_crew(&crate::eval::transition_row(
                &w,
                o,
                w.parts(o).unwrap(),
                t,
                last
            )),
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
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
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
        let o = w.whole(&j).unwrap().unwrap();
        let t = w.lib.type_("Job").unwrap();
        let states = crate::eval::interval_rows(&w, o, t, "state").unwrap();
        let start = w.log().unwrap().find(|e| e.transition == "start").unwrap();
        assert_eq!(
            (
                held_crew(states.last().unwrap()),
                held_crew(&crate::eval::transition_row(
                    &w,
                    o,
                    w.parts(o).unwrap(),
                    t,
                    start
                )),
                o.held_parts().unwrap().spans["crew"]
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
    /// A uniqueness check reads its family as a type scan does (ADR-0088,
    /// ADR-0158): an event records the scan under the invariant's name, empty
    /// where the value is unique, and a refused duplicate's attempt row
    /// records the object it collided with, at the version read.
    #[test]
    fn a_uniqueness_check_records_the_scan_it_made() {
        let mut s = store();
        let ana = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ));
        let bob = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "bob"}),
        ));
        let added = s.history(&bob).unwrap().pop().unwrap();
        assert_eq!(added.reads["scans"]["login_unique"], json!({"User": []}));
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
        let w = s.snapshot().unwrap();
        let refused = &w.attempts().unwrap().last().unwrap().reads;
        assert_eq!(
            refused["scans"]["login_unique"],
            json!({ "User": [ana.clone()] })
        );
        assert_eq!(
            refused["objects"][&ana],
            json!(w.whole(&ana).unwrap().unwrap().version)
        );
    }

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
        // the size decides, and the sandbox has no answer for the vault, which
        // only the record would ask: it consults nothing rather than stopping
        // the request, as stopping early never asks it
        assert_eq!(
            req(&mut s, "ship", OPERATOR, (&big, false), json!({})),
            Verdict::Unsatisfied {
                clause: "small_and_cleared".into(),
                remedy: Some(Remedy::SelfServiceable),
            }
        );
        assert!(
            s.snapshot()
                .unwrap()
                .attempts()
                .unwrap()
                .last()
                .unwrap()
                .consulted["small_and_cleared"]
                .get("values")
                .is_none_or(|v| v.as_object().is_none_or(|m| m.is_empty()))
        );
        // where it decides, a call with no answer still stops the request
        assert!(
            request(
                &mut s,
                &Request {
                    transition: "ship".into(),
                    actor: OPERATOR.into(),
                    object_id: Some(one.clone()),
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
                }
            )
            .unwrap()
            .is_err()
        );
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
        let row = w.attempts().unwrap().last().unwrap();
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
            w.attempts().unwrap().last().unwrap().clause.as_deref(),
            Some("every_cleared")
        );
    }

    /// Idempotency (`DESIGN.md` §6 step 2, ADR-0041, ADR-0076, ADR-0077): a
    /// key its actor applied replays that request's verdict, marked so and
    /// writing nothing, whatever version the retry names; a different
    /// request under it is a fault, logged as `self_serviceable`; a key is
    /// its actor's alone; a refused request records nothing, so its key
    /// stays free; and an actor the store does not know replays nothing.
    #[test]
    fn a_key_replays_what_it_applied_and_refuses_another_request() {
        let mut s = store();
        let ana = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("User", true),
            json!({"login": "ana"}),
        ));
        let keyed = |actor: &str,
                     xn: &str,
                     object: Option<&str>,
                     inputs: Json,
                     version: Option<i64>,
                     key: &str| Request {
            transition: xn.into(),
            actor: actor.into(),
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
            expected_version: version,
            target_version: None,
            principal: None,
            idempotency_key: Some(key.to_string()),
            context: None,
        };
        let counts = |s: &Store| {
            let w = s.snapshot().unwrap();
            (
                w.in_order().unwrap().len(),
                w.log().unwrap().count(),
                w.attempts().unwrap().len(),
            )
        };
        // applied, then replayed: the same verdict, marked, nothing written
        let open = keyed(OPERATOR, "open", None, json!({"title": "Fix"}), None, "k1");
        let first = request(&mut s, &open).unwrap().unwrap();
        let Verdict::Satisfied {
            object_id: job,
            version,
            events,
            replayed: false,
            ..
        } = first.clone()
        else {
            panic!("{first:?}")
        };
        let before = counts(&s);
        assert_eq!(
            request(&mut s, &open).unwrap().unwrap(),
            Verdict::Satisfied {
                object_id: job.clone(),
                version,
                events,
                flags: Vec::new(),
                replayed: true,
            }
        );
        assert_eq!(counts(&s), before);
        // a retry names the version the original was checked against, now
        // moved: it replays, and is not stale
        let start = keyed(
            OPERATOR,
            "start",
            Some(&job),
            json!({"engineer": ana}),
            Some(1),
            "k2",
        );
        let started = request(&mut s, &start).unwrap().unwrap();
        let Verdict::Satisfied { flags, .. } = &started else {
            panic!("{started:?}")
        };
        // the warning it raised is replayed with it
        assert_eq!(flags.len(), 1);
        let again = request(&mut s, &start).unwrap().unwrap();
        assert!(
            matches!(&again, Verdict::Satisfied { replayed: true, flags: f, .. } if f == flags)
        );
        // whatever version the retry names
        let mut renamed = start.clone();
        renamed.expected_version = Some(7);
        assert!(matches!(
            request(&mut s, &renamed).unwrap().unwrap(),
            Verdict::Satisfied { replayed: true, .. }
        ));
        // but a different occurred time is another request
        let mut dated = start.clone();
        dated.occurred_at = Some(1_767_225_000);
        assert_eq!(
            request(&mut s, &dated).unwrap(),
            Err(NoVerdict::KeyReused { key: "k2".into() })
        );
        // a different request under the key is a fault, logged, and writes
        // nothing
        let before = counts(&s);
        let other = keyed(
            OPERATOR,
            "estimate",
            Some(&job),
            json!({"hours": 3}),
            None,
            "k2",
        );
        assert_eq!(
            request(&mut s, &other).unwrap(),
            Err(NoVerdict::KeyReused { key: "k2".into() })
        );
        assert_eq!(counts(&s), (before.0, before.1, before.2 + 1));
        let w = s.snapshot().unwrap();
        let row = w.attempts().unwrap().last().unwrap();
        assert_eq!(
            (
                row.verdict.as_str(),
                row.remedy.as_str(),
                row.clause.as_deref()
            ),
            ("key_reused", "self_serviceable", None)
        );
        drop(w);
        // a key is its actor's: another actor's identical key applies anew
        let anas = keyed("ana", "open", None, json!({"title": "Fix"}), None, "k1");
        let Verdict::Satisfied {
            object_id: second,
            replayed: false,
            ..
        } = request(&mut s, &anas).unwrap().unwrap()
        else {
            panic!("ana's own key applies")
        };
        assert_ne!(second, job);
        // a refused request records nothing: its retry is refused afresh,
        // and the key then applies another request
        let big = keyed(
            OPERATOR,
            "estimate",
            Some(&job),
            json!({"hours": 20}),
            None,
            "k3",
        );
        for _ in 0..2 {
            assert!(matches!(
                request(&mut s, &big).unwrap().unwrap(),
                Verdict::Unsatisfied { .. }
            ));
        }
        let small = keyed(
            OPERATOR,
            "estimate",
            Some(&job),
            json!({"hours": 4}),
            None,
            "k3",
        );
        assert!(matches!(
            request(&mut s, &small).unwrap().unwrap(),
            Verdict::Satisfied {
                replayed: false,
                ..
            }
        ));
        // once applied, a body differing in its inputs alone is another request
        assert_eq!(
            request(&mut s, &big).unwrap(),
            Err(NoVerdict::KeyReused { key: "k3".into() })
        );
        // an actor the store does not know has applied nothing
        let stranger = keyed("nobody", "open", None, json!({"title": "Fix"}), None, "k1");
        assert!(matches!(
            request(&mut s, &stranger).unwrap().unwrap(),
            Verdict::Unsatisfied { ref clause, .. } if clause == "actor_known"
        ));
    }

    /// An event records every input its transition took, declared ones
    /// with those that write an attribute (`storage-schema.md` §4); an
    /// erasure redacts each one personal under the transition its event
    /// recorded, marked so or flowing into a personal attribute, and keeps a
    /// same-named input another transition does not mark (§9 step 8).
    #[test]
    fn an_event_records_every_input_and_an_erasure_redacts_the_personal_ones() {
        const NOTES: &str = r#"
module: notes
categories: [live, closed]
types:
  Job:
    attributes:
      title: { type: string }
      owner_name: { type: string, optional: true, personal: true }
    states:
      OPEN: { category: live }
      HELD: { category: live }
    conditions:
      explained: { expression: inputs.why is not null, remedy: self_serviceable }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [title], optional_inputs: [owner_name] }
      remark: { kind: internal, from: [OPEN], inputs: { memo: { type: string, personal: true }, why: { type: string } }, guards: { explained: deny } }
      log: { kind: internal, from: [OPEN], inputs: { memo: { type: string } } }
      sign: { kind: internal, from: [OPEN], inputs: { signer: { type: string } }, effect: [ { assign: { location: owner_name, expr: inputs.signer } } ] }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
      fix: { kind: assertion, to: [HELD], inputs: { reason: { type: string, personal: true } } }
"#;
        let lib = Library::load(&[("notes.yaml".into(), NOTES.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let job = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Job", true),
            json!({"title": "Fix", "owner_name": "Dana"}),
        ));
        for (xn, inputs) in [
            (
                "remark",
                json!({"memo": "Dana's private note", "why": "audit"}),
            ),
            ("log", json!({"memo": "a shared note"})),
            ("sign", json!({"signer": "Dana D."})),
        ] {
            created(&req(&mut s, xn, OPERATOR, (&job, false), inputs));
        }
        let inputs = |s: &Store| -> Vec<(String, Json)> {
            s.history(&job)
                .unwrap()
                .into_iter()
                .map(|e| (e.transition, e.payload["inputs"].clone()))
                .collect()
        };
        // every input recorded, declared ones and personal ones included
        assert_eq!(
            inputs(&s),
            [
                (
                    "open".to_string(),
                    json!({"title": "Fix", "owner_name": "Dana"})
                ),
                (
                    "remark".to_string(),
                    json!({"memo": "Dana's private note", "why": "audit"})
                ),
                ("log".to_string(), json!({"memo": "a shared note"})),
                ("sign".to_string(), json!({"signer": "Dana D."})),
            ]
        );
        // an override's personal reason is recorded with its inputs, and no
        // read of its transition takes it from there
        created(&req(
            &mut s,
            "fix",
            OPERATOR,
            (&job, false),
            json!({"to": "HELD", "reason": "Dana asked"}),
        ));
        {
            let w = s.snapshot().unwrap();
            let o = w.whole(&job).unwrap().unwrap();
            let t = w.lib.type_("Job").unwrap();
            let e = w.log().unwrap().rfind(|e| e.transition == "fix").unwrap();
            assert_eq!(e.payload["inputs"]["reason"], json!("Dana asked"));
            let Value::Row(row) = crate::eval::transition_row(&w, o, w.parts(o).unwrap(), t, e)
            else {
                panic!("a transition's row")
            };
            assert_eq!(row.get("reason"), Some(&Value::Absent));
        }
        created(&req(
            &mut s,
            "forget",
            OPERATOR,
            (&job, false),
            json!({"reason": "request"}),
        ));
        // the personal ones redacted where they were personal, the rest kept
        let after = inputs(&s);
        assert_eq!(
            &after[..4],
            [
                (
                    "open".to_string(),
                    json!({"title": "Fix", "owner_name": null})
                ),
                ("remark".to_string(), json!({"memo": null, "why": "audit"})),
                ("log".to_string(), json!({"memo": "a shared note"})),
                ("sign".to_string(), json!({"signer": null})),
            ]
        );
    }

    /// A request's `context`, the caller's reason (`DESIGN.md` §6, ADR-0105
    /// §8): every event it makes records it, a cascade's included, as does
    /// each attempt row, an audit's and a refusal's; a proposal keeps its
    /// proposer's, which its execution records, while the approval's own
    /// event records the approver's; and it is part of what a key identifies.
    #[test]
    fn a_requests_context_is_recorded_on_everything_it_makes() {
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
      counted: { expression: qty > 0, remedy: self_serviceable }
    transitions:
      add: { kind: initial, to: OPEN, required_inputs: [qty] }
      pack: { kind: external, from: [OPEN], to: PACKED, guards: { small: deny, counted: audit }, proposable: true }
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
        let ask = |s: &mut Store,
                   xn: &str,
                   target: (&str, bool),
                   inputs: Json,
                   context: &str,
                   propose: bool| {
            let (name, is_type) = target;
            let r = Request {
                transition: xn.into(),
                actor: OPERATOR.into(),
                object_id: (!is_type).then(|| name.to_string()),
                type_: is_type.then(|| name.to_string()),
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
                context: Some(context.to_string()),
            };
            request(s, &r).unwrap().unwrap()
        };
        let line = |s: &mut Store, qty: i64| {
            created(&ask(
                s,
                "add",
                ("Line", true),
                json!({"qty": qty}),
                "stock",
                false,
            ))
        };
        let order = |s: &mut Store, l: &str| {
            created(&ask(
                s,
                "open",
                ("Order", true),
                json!({"line": l}),
                "sales",
                false,
            ))
        };
        let contexts = |s: &Store, positions: &[i64]| -> Vec<(String, Option<String>)> {
            s.events(positions)
                .unwrap()
                .into_iter()
                .flatten()
                .map(|e| (e.transition, e.context))
                .collect()
        };
        // a cascade's event records the request's reason
        let (l1, l0, big) = (line(&mut s, 3), line(&mut s, 0), line(&mut s, 20));
        let o1 = order(&mut s, &l1);
        let Verdict::Satisfied { events, .. } =
            ask(&mut s, "ship", (&o1, false), json!({}), "job:ship", false)
        else {
            panic!("a small line ships")
        };
        let why = Some("job:ship".to_string());
        assert_eq!(
            contexts(&s, &events),
            [
                ("ship".to_string(), why.clone()),
                ("pack".to_string(), why.clone())
            ]
        );
        // an audit's row, and a refusal's
        let o0 = order(&mut s, &l0);
        ask(
            &mut s,
            "ship",
            (&o0, false),
            json!({}),
            "job:audited",
            false,
        );
        let w = s.snapshot().unwrap();
        let audit = w.attempts().unwrap().last().unwrap();
        assert_eq!(
            (audit.clause.as_deref(), audit.context.as_deref()),
            (Some("counted"), Some("job:audited"))
        );
        drop(w);
        let ob = order(&mut s, &big);
        assert!(matches!(
            ask(
                &mut s,
                "ship",
                (&ob, false),
                json!({}),
                "job:refused",
                false
            ),
            Verdict::CallRefused { .. }
        ));
        let w = s.snapshot().unwrap();
        assert_eq!(
            w.attempts().unwrap().last().unwrap().context.as_deref(),
            Some("job:refused")
        );
        drop(w);
        // a proposal keeps its proposer's reason for the request it executes
        let l2 = line(&mut s, 2);
        let proposal = created(&ask(
            &mut s,
            "pack",
            (&l2, false),
            json!({}),
            "proposer:why",
            true,
        ));
        let Verdict::Satisfied { events, .. } = ask(
            &mut s,
            "approve",
            (&proposal, false),
            json!({}),
            "approver:why",
            false,
        ) else {
            panic!("the approval applies")
        };
        assert_eq!(
            contexts(&s, &events),
            [
                ("approve".to_string(), Some("approver:why".to_string())),
                ("pack".to_string(), Some("proposer:why".to_string()))
            ]
        );
        // and a different reason under a used key is another request
        let mut keyed = Request {
            transition: "add".into(),
            actor: OPERATOR.into(),
            object_id: None,
            type_: Some("Line".into()),
            inputs: [("qty".to_string(), json!(1))].into_iter().collect(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            target_version: None,
            principal: None,
            idempotency_key: Some("k".into()),
            context: Some("first".into()),
        };
        created(&request(&mut s, &keyed).unwrap().unwrap());
        keyed.context = Some("second".into());
        assert_eq!(
            request(&mut s, &keyed).unwrap(),
            Err(NoVerdict::KeyReused { key: "k".into() })
        );
    }

    /// A keyed request's digest masks its personal values, so the key
    /// confirms no guess of one, before an erasure or after it (fourth
    /// production review A-5); and another request under the key is refused,
    /// before an erasure and after it, where after it its verdict was
    /// replayed for a request that never ran (fourteenth production review
    /// N2).
    #[test]
    fn an_erased_requests_key_confirms_no_guess() {
        const CRM: &str = r#"
module: crm
categories: [live]
types:
  Contact:
    attributes:
      title: { type: string }
      name: { type: string, optional: true, personal: true }
    states:
      ON: { category: live }
    transitions:
      add: { kind: initial, to: ON, required_inputs: [title], optional_inputs: [name] }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
"#;
        let lib = Library::load(&[("crm.yaml".into(), CRM.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let keyed = |title: &str, name: &str| Request {
            transition: "add".into(),
            actor: OPERATOR.into(),
            object_id: None,
            type_: Some("Contact".into()),
            inputs: [
                ("title".to_string(), json!(title)),
                ("name".to_string(), json!(name)),
            ]
            .into_iter()
            .collect(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            target_version: None,
            principal: None,
            idempotency_key: Some("k".into()),
            context: None,
        };
        let id = created(&request(&mut s, &keyed("Lead", "Dana")).unwrap().unwrap());
        let guesses = |s: &mut Store| {
            for guess in ["Dana", "Ana"] {
                let again = request(s, &keyed("Lead", guess)).unwrap();
                assert!(
                    matches!(&again, Ok(Verdict::Satisfied { object_id, .. }) if *object_id == id),
                    "{guess}: {again:?}"
                );
            }
            assert_eq!(
                request(s, &keyed("Cold", "Dana")).unwrap(),
                Err(NoVerdict::KeyReused { key: "k".into() })
            );
        };
        guesses(&mut s);
        created(&req(
            &mut s,
            "forget",
            OPERATOR,
            (&id, false),
            json!({"reason": "asked"}),
        ));
        guesses(&mut s);
    }

    /// An assignment whose arithmetic reads an absent optional attribute has
    /// no value, which refuses the request `value_in_range`, as arithmetic
    /// past its bound does, where it answered not run; one from an
    /// unsupplied optional input still changes nothing (fourteenth production
    /// review N5).
    #[test]
    fn an_assignment_of_arithmetic_on_an_absent_value_is_refused() {
        let text = "\nmodule: c\ncategories: [live]\ntypes:\n  Counter:\n    attributes:\n      count: { type: int, optional: true }\n      note: { type: string, optional: true }\n    states:\n      ON: { category: live }\n    transitions:\n      make: { kind: initial, to: ON }\n      bump:\n        kind: internal\n        from: [ON]\n        inputs: { by: { type: int } }\n        effect:\n          - assign: { location: count, expr: count + inputs.by }\n      jot:\n        kind: internal\n        from: [ON]\n        inputs: { text: { type: string, optional: true } }\n        effect:\n          - assign: { location: note, expr: inputs.text }\n";
        let lib = Library::load(&[("c.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let id = created(&req(&mut s, "make", OPERATOR, ("Counter", true), json!({})));
        let v = req(&mut s, "bump", OPERATOR, (&id, false), json!({"by": 1}));
        assert!(
            matches!(&v, Verdict::Unsatisfied { clause, .. } if clause == "value_in_range"),
            "{v:?}"
        );
        created(&req(&mut s, "jot", OPERATOR, (&id, false), json!({})));
    }

    /// A date moved by part of a day has no value, so an assignment of it
    /// refuses its request `value_in_range`, where it answered not run; a
    /// date moved by whole days is moved (fifteenth production review).
    #[test]
    fn a_date_moved_by_part_of_a_day_has_no_value() {
        let text = "\nmodule: d\ncategories: [live]\ntypes:\n  Task:\n    attributes:\n      due: { type: date }\n    states:\n      ON: { category: live }\n    transitions:\n      make: { kind: initial, to: ON, required_inputs: [due] }\n      defer:\n        kind: internal\n        from: [ON]\n        inputs: { by: { type: duration } }\n        effect:\n          - assign: { location: due, expr: due + inputs.by }\n";
        let lib = Library::load(&[("d.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let id = created(&req(
            &mut s,
            "make",
            OPERATOR,
            ("Task", true),
            json!({"due": "2027-01-04"}),
        ));
        let v = req(
            &mut s,
            "defer",
            OPERATOR,
            (&id, false),
            json!({"by": "12 h"}),
        );
        assert!(
            matches!(&v, Verdict::Unsatisfied { clause, .. } if clause == "value_in_range"),
            "{v:?}"
        );
        created(&req(
            &mut s,
            "defer",
            OPERATOR,
            (&id, false),
            json!({"by": "2 days"}),
        ));
    }

    /// An erasure reads the events it walks, its object's own, those they
    /// caused and those that called them, by the log's indexes, where it
    /// read the whole log under the writer; what it redacts is the same
    /// (fourth production review).
    #[test]
    fn an_erasure_reads_only_the_events_it_walks() {
        const CRM: &str = r#"
module: crm
categories: [live]
types:
  Contact:
    attributes:
      title: { type: string }
      name: { type: string, optional: true, personal: true }
    states:
      ON: { category: live }
    transitions:
      add: { kind: initial, to: ON, required_inputs: [title], optional_inputs: [name] }
      rename: { kind: internal, from: [ON], required_inputs: [name] }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
"#;
        let lib = Library::load(&[("crm.yaml".into(), CRM.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let mut ids = Vec::new();
        for i in 0..50 {
            ids.push(created(&req(
                &mut s,
                "add",
                OPERATOR,
                ("Contact", true),
                json!({"title": format!("Lead {i}"), "name": format!("Person {i}")}),
            )));
        }
        created(&req(
            &mut s,
            "rename",
            OPERATOR,
            (&ids[7], false),
            json!({"name": "Dana"}),
        ));
        let before = crate::world::whole_log_reads();
        created(&req(
            &mut s,
            "forget",
            OPERATOR,
            (&ids[7], false),
            json!({"reason": "asked"}),
        ));
        assert_eq!(
            crate::world::whole_log_reads(),
            before,
            "the erasure read the whole log"
        );
        let inputs: Vec<Json> = s
            .history(&ids[7])
            .unwrap()
            .into_iter()
            .filter(|e| e.transition != "forget")
            .map(|e| e.payload["inputs"]["name"].clone())
            .collect();
        assert_eq!(inputs, [Json::Null, Json::Null]);
        let other = s.history(&ids[8]).unwrap();
        assert_eq!(other[0].payload["inputs"]["name"], json!("Person 8"));
    }

    /// An invariant reading related objects is checked from them: a write
    /// reads none of its type whole, where every write swept it under the
    /// writer (sixth production review P8), and every way a write can break
    /// one is still found: a bound over a part set, a sum over its parts, a
    /// path through another type, a part moved between wholes and a chain
    /// of two (`DESIGN.md` §5.6).
    #[test]
    fn a_traversal_invariant_is_checked_from_what_was_written() {
        const CLAIMS: &str = r#"
module: claims
categories: [live]
types:
  Unit:
    attributes:
      code: { type: string }
    states:
      ON: { category: live }
    invariants:
      shared: { expression: "count(r in referrers) <= 2" }
    transitions:
      make: { kind: initial, to: ON, required_inputs: [code] }
      recode: { kind: internal, from: [ON], required_inputs: [code] }
  Category:
    attributes:
      name: { type: string }
      unit: { reference: Unit }
    states:
      ON: { category: live }
    transitions:
      define: { kind: initial, to: ON, required_inputs: [name, unit] }
      rename: { kind: internal, from: [ON], required_inputs: [name] }
  Claim:
    attributes:
      cap: { type: int }
      lines: { reference: "Line[]", aggregation: composite, opposite: claim }
    states:
      ON: { category: live }
    invariants:
      few: { expression: "count(l in lines) <= 3" }
      under: { expression: "sum(l in lines: l.amount) <= cap" }
      kept: { expression: "cap != 50 or count(l in lines) >= 1" }
    transitions:
      open: { kind: initial, to: ON, required_inputs: [cap] }
      recap: { kind: internal, from: [ON], required_inputs: [cap] }
  Line:
    attributes:
      claim: { reference: Claim, opposite: lines }
      category: { reference: Category }
      amount: { type: int }
    states:
      ON: { category: live }
    invariants:
      named: { expression: "category.name != \"\"" }
      coded: { expression: "category.unit.code != \"X\"" }
    transitions:
      add: { kind: initial, to: ON, required_inputs: [claim, category, amount] }
      move: { kind: internal, from: [ON], required_inputs: [claim] }
      reprice: { kind: internal, from: [ON], required_inputs: [amount] }
"#;
        let lib = Library::load(&[("claims.yaml".into(), CLAIMS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let unit = created(&req(
            &mut s,
            "make",
            OPERATOR,
            ("Unit", true),
            json!({"code": "EA"}),
        ));
        let cat = created(&req(
            &mut s,
            "define",
            OPERATOR,
            ("Category", true),
            json!({"name": "Travel", "unit": unit}),
        ));
        let mut claims = Vec::new();
        for _ in 0..40 {
            claims.push(created(&req(
                &mut s,
                "open",
                OPERATOR,
                ("Claim", true),
                json!({"cap": 100}),
            )));
        }
        let line = |s: &mut Store, claim: &str, amount: i64| {
            req(
                s,
                "add",
                OPERATOR,
                ("Line", true),
                json!({"claim": claim, "category": cat, "amount": amount}),
            )
        };
        let mut lines = Vec::new();
        for _ in 0..3 {
            lines.push(created(&line(&mut s, &claims[0], 10)));
        }
        created(&line(&mut s, &claims[1], 10));
        let violated = |v: &Verdict, n: &str| matches!(v, Verdict::InvariantViolated { failures, .. } if failures.iter().any(|f| f.invariant == n));
        // no write reads the claims, or the lines, whole
        let (claims_read, lines_read) = (
            crate::world::whole_type_reads("Claim"),
            crate::world::whole_type_reads("Line"),
        );
        created(&req(
            &mut s,
            "recap",
            OPERATOR,
            (&claims[5], false),
            json!({"cap": 90}),
        ));
        created(&req(
            &mut s,
            "rename",
            OPERATOR,
            (&cat, false),
            json!({"name": "Trips"}),
        ));
        created(&line(&mut s, &claims[2], 5));
        assert_eq!(
            (
                crate::world::whole_type_reads("Claim"),
                crate::world::whole_type_reads("Line")
            ),
            (claims_read, lines_read),
            "a write read a type its invariants reach whole"
        );
        // a fourth line breaks its claim's bound, from the line
        assert!(violated(&line(&mut s, &claims[0], 1), "few"));
        // a line repriced past its claim's cap, from the line
        assert!(violated(
            &req(
                &mut s,
                "reprice",
                OPERATOR,
                (&lines[0], false),
                json!({"amount": 95})
            ),
            "under"
        ));
        // a line moved into a full claim breaks it, from the line as it is
        let spare = created(&line(&mut s, &claims[3], 10));
        assert!(violated(
            &req(
                &mut s,
                "move",
                OPERATOR,
                (&spare, false),
                json!({"claim": claims[0]})
            ),
            "few"
        ));
        // a line moved out of a claim that needs one breaks the claim it
        // left, from the line as it was
        created(&req(
            &mut s,
            "recap",
            OPERATOR,
            (&claims[4], false),
            json!({"cap": 60}),
        ));
        let only = created(&line(&mut s, &claims[4], 10));
        created(&req(
            &mut s,
            "recap",
            OPERATOR,
            (&claims[4], false),
            json!({"cap": 50}),
        ));
        assert!(violated(
            &req(
                &mut s,
                "move",
                OPERATOR,
                (&only, false),
                json!({"claim": claims[6]})
            ),
            "kept"
        ));
        // a third category of one unit breaks the unit, from the category
        // that refers to it
        created(&req(
            &mut s,
            "define",
            OPERATOR,
            ("Category", true),
            json!({"name": "Meals", "unit": unit}),
        ));
        assert!(violated(
            &req(
                &mut s,
                "define",
                OPERATOR,
                ("Category", true),
                json!({"name": "Hotels", "unit": unit}),
            ),
            "shared"
        ));
        // a category renamed to nothing breaks every line naming it, along
        // the path from the category
        assert!(violated(
            &req(
                &mut s,
                "rename",
                OPERATOR,
                (&cat, false),
                json!({"name": ""})
            ),
            "named"
        ));
        // and a unit two hops away, along the chain from the unit
        assert!(violated(
            &req(
                &mut s,
                "recode",
                OPERATOR,
                (&unit, false),
                json!({"code": "X"})
            ),
            "coded"
        ));
        // a claim's cap lowered under its lines, from the claim itself
        assert!(violated(
            &req(
                &mut s,
                "recap",
                OPERATOR,
                (&claims[0], false),
                json!({"cap": 20})
            ),
            "under"
        ));
    }

    /// A supersession group's erasure reads its members' predecessors by the
    /// index on their successor's column, never a superseding type whole,
    /// where a 41-member group held the writer 1.43 s (fifth production
    /// review, ADR-0191 decision 5); what it erases is the same.
    #[test]
    fn a_groups_erasure_reads_no_type_whole() {
        const ACCOUNTS: &str = r#"
module: accounts
categories: [live, closed]
types:
  Account:
    attributes:
      name: { type: string, optional: true, personal: true }
    states:
      ON: { category: live }
      MERGED: { category: closed, final: true, superseding: true }
    transitions:
      open: { kind: initial, to: ON, optional_inputs: [name] }
      absorb:
        kind: internal
        from: [ON]
        inputs: { other: { reference: Account } }
        effect:
          - call: { target: inputs.other, transition: merged_into, inputs: { successor: this } }
      merged_into:
        kind: external
        from: [ON]
        to: MERGED
        only_via: [Account.absorb]
        inputs: { successor: { reference: Account } }
        effect:
          - supersede: inputs.successor
      forget: { kind: erasure, inputs: { reason: { type: string } } }
"#;
        let lib = Library::load(&[("accounts.yaml".into(), ACCOUNTS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let mut ids = Vec::new();
        for i in 0..50 {
            ids.push(created(&req(
                &mut s,
                "open",
                OPERATOR,
                ("Account", true),
                json!({"name": format!("Person {i}")}),
            )));
        }
        for other in &ids[1..6] {
            created(&req(
                &mut s,
                "absorb",
                OPERATOR,
                (&ids[0], false),
                json!({"other": other}),
            ));
        }
        let before = crate::world::whole_type_reads("Account");
        let walks = group_walks();
        created(&req(
            &mut s,
            "forget",
            OPERATOR,
            (&ids[0], false),
            json!({"reason": "asked"}),
        ));
        assert_eq!(
            crate::world::whole_type_reads("Account"),
            before,
            "the group's erasure read its type whole"
        );
        // and walked the group once, not once for each member (ADR-0192
        // decision 7)
        assert_eq!(group_walks() - walks, 1, "the group walked by each member");
        let snapshot = s.snapshot().unwrap();
        for id in &ids[..6] {
            let o = snapshot.whole(id).unwrap().unwrap();
            assert!(o.attr("name").is_absent(), "{id} kept its name");
        }
        assert_eq!(
            snapshot.whole(&ids[6]).unwrap().unwrap().attr("name"),
            Value::Text("Person 6".into())
        );
    }

    /// The store's operator (ADR-0126, ADR-0134): its creation, `install`,
    /// is the store's own first event, the operator's act with the context
    /// naming the release; no request takes it; and `retire` is requested
    /// like any transition.
    #[test]
    fn the_operator_is_installed_with_the_store_and_retires_like_any_object() {
        let mut s = store();
        let installed = s.history(OPERATOR).unwrap();
        assert_eq!(installed.len(), 1);
        let e = &installed[0];
        assert_eq!(
            (
                e.transition.as_str(),
                e.actor_id.as_str(),
                e.actor_kind.as_str(),
                e.source.as_str()
            ),
            ("install", OPERATOR, "human", "observed")
        );
        assert_eq!(
            e.context.as_deref(),
            Some(format!("store-creation:{}", crate::store::BUILTINS).as_str())
        );
        assert_eq!(
            req(
                &mut s,
                "install",
                OPERATOR,
                (crate::model::OPERATOR_TYPE, true),
                json!({"identity": "ops:ben"})
            ),
            Verdict::NotRequestable {
                parents: Vec::new()
            }
        );
        // and its retirement is taken by nothing (ADR-0184 decision 1)
        assert!(matches!(
            req(&mut s, "retire", OPERATOR, (OPERATOR, false), json!({})),
            Verdict::NotRequestable { .. }
        ));
        assert_eq!(
            s.snapshot()
                .unwrap()
                .whole(OPERATOR)
                .unwrap()
                .unwrap()
                .state,
            "ACTIVE"
        );
    }

    /// An erasure walks up its events' cause chains (`storage-schema.md` §9
    /// step 3): a caller's input passed into the erased personal value,
    /// directly or through another caller, is redacted in the caller's
    /// event; an input it kept, or passed elsewhere, stays.
    #[test]
    fn an_erasure_redacts_what_callers_passed_into_the_erased_value() {
        const DESKS: &str = r#"
module: desks
categories: [live, closed]
enumerations: { Tier: [GOLD, SILVER] }
types:
  Person:
    attributes:
      name: { type: string, optional: true, personal: true }
      level: { type: Tier, optional: true, personal: true }
      desk: { reference: Desk }
    states:
      ACTIVE: { category: live }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [name, desk] }
      rename: { kind: internal, from: [ACTIVE], required_inputs: [name] }
      grade: { kind: internal, from: [ACTIVE], required_inputs: [level] }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
      issue_badge:
        kind: internal
        from: [ACTIVE]
        effect:
          - create: { type: Badge, transition: issue, inputs: { holder_name: name, tier: level, echo: name, person: this } }
  Badge:
    attributes:
      holder_name: { type: string, optional: true, personal: true }
      tier: { type: Tier, optional: true, personal: true }
      person: { reference: Person }
    states:
      ACTIVE: { category: live }
    transitions:
      issue:
        kind: initial
        to: ACTIVE
        required_inputs: [holder_name, person]
        optional_inputs: [tier]
        inputs: { echo: { type: string } }
        effect:
          - create: { type: Card, transition: print, inputs: { text: inputs.holder_name, badge: this } }
      rewrite: { kind: internal, from: [ACTIVE], required_inputs: [holder_name] }
  Card:
    attributes:
      text: { type: string, optional: true, personal: true }
      badge: { reference: Badge }
    states:
      ACTIVE: { category: live }
    transitions:
      print: { kind: initial, to: ACTIVE, required_inputs: [text, badge] }
  Desk:
    attributes:
      person: { reference: Person, optional: true }
      label: { type: string, optional: true }
    states:
      OPEN: { category: live }
    transitions:
      open: { kind: initial, to: OPEN }
      register:
        kind: internal
        from: [OPEN]
        inputs: { full_name: { type: string }, note: { type: string } }
        effect:
          - create: { type: Person, transition: add, inputs: { name: inputs.full_name, desk: this }, result: p }
          - assign: { location: person, expr: p }
          - assign: { location: label, expr: inputs.note }
      relabel_person:
        kind: internal
        from: [OPEN]
        inputs: { new_name: { type: string }, memo: { type: string } }
        effect:
          - call: { target: person, transition: rename, inputs: { name: inputs.new_name } }
  Office:
    attributes:
      desk: { reference: Desk }
    states:
      OPEN: { category: live }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [desk] }
      rename_at_desk:
        kind: internal
        from: [OPEN]
        inputs: { who: { type: string }, why: { type: string } }
        effect:
          - call: { target: desk, transition: relabel_person, inputs: { new_name: inputs.who, memo: inputs.why } }
      rename_in_turn:
        kind: internal
        from: [OPEN]
        inputs: { aliases: { type: "string[]" }, why: { type: string } }
        effect:
          - foreach:
              item: n
              array: inputs.aliases
              limit: 5
              steps:
                - call: { target: desk, transition: relabel_person, inputs: { new_name: n, memo: inputs.why } }
"#;
        let lib = Library::load(&[("desks.yaml".into(), DESKS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let desk = created(&req(&mut s, "open", OPERATOR, ("Desk", true), json!({})));
        created(&req(
            &mut s,
            "register",
            OPERATOR,
            (&desk, false),
            json!({"full_name": "Dana Doe", "note": "corner desk"}),
        ));
        let office = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Office", true),
            json!({"desk": desk}),
        ));
        created(&req(
            &mut s,
            "rename_at_desk",
            OPERATOR,
            (&office, false),
            json!({"who": "Dana D.", "why": "a typo"}),
        ));
        // a loop's item, from its array: the array is what passed the name
        created(&req(
            &mut s,
            "rename_in_turn",
            OPERATOR,
            (&office, false),
            json!({"aliases": ["Dee", "D. Doe"], "why": "nicknames"}),
        ));
        let person = match s
            .snapshot()
            .unwrap()
            .whole(&desk)
            .unwrap()
            .unwrap()
            .attr("person")
        {
            Value::Object(p) => p,
            other => panic!("{other:?}"),
        };
        // two badges copy the name and the grade down, each name on to a
        // card, and into an input no attribute keeps; the first's name is
        // since replaced by another value
        created(&req(
            &mut s,
            "grade",
            OPERATOR,
            (&person, false),
            json!({"level": "GOLD"}),
        ));
        for _ in 0..2 {
            created(&req(
                &mut s,
                "issue_badge",
                OPERATOR,
                (&person, false),
                json!({}),
            ));
        }
        let of_type = |s: &Store, t: &str| -> Vec<String> {
            s.snapshot()
                .unwrap()
                .of_type(t)
                .unwrap()
                .iter()
                .map(|o| o.id.clone())
                .collect()
        };
        let (badges, cards) = (of_type(&s, "Badge"), of_type(&s, "Card"));
        assert_eq!((badges.len(), cards.len()), (2, 2));
        created(&req(
            &mut s,
            "rewrite",
            OPERATOR,
            (&badges[0], false),
            json!({"holder_name": "Visitor"}),
        ));
        let versions = |s: &Store| -> Vec<i64> {
            let w = s.snapshot().unwrap();
            [&desk, &office, &badges[0], &badges[1], &cards[0], &cards[1]]
                .iter()
                .map(|id| w.whole(id).unwrap().unwrap().version)
                .collect()
        };
        let before = versions(&s);
        created(&req(
            &mut s,
            "forget",
            OPERATOR,
            (&person, false),
            json!({"reason": "request"}),
        ));
        let inputs = |s: &Store, id: &str, xn: &str| -> Json {
            s.history(id)
                .unwrap()
                .into_iter()
                .find(|e| e.transition == xn)
                .unwrap()
                .payload["inputs"]
                .clone()
        };
        // the creation's caller: the name it passed redacted, its note kept
        assert_eq!(
            inputs(&s, &desk, "register"),
            json!({"full_name": null, "note": "corner desk"})
        );
        // a call's caller, and its caller in turn
        assert_eq!(
            inputs(&s, &desk, "relabel_person"),
            json!({"new_name": null, "memo": "a typo"})
        );
        assert_eq!(
            inputs(&s, &office, "rename_at_desk"),
            json!({"who": null, "why": "a typo"})
        );
        assert_eq!(
            inputs(&s, &office, "rename_in_turn"),
            json!({"aliases": null, "why": "nicknames"})
        );
        // down: each copy redacted in the event that carried it, two levels
        // deep, and cleared where the object still holds it
        for b in &badges {
            let issued = inputs(&s, b, "issue");
            assert_eq!(
                (&issued["holder_name"], &issued["tier"], &issued["echo"]),
                (&Json::Null, &Json::Null, &Json::Null)
            );
        }
        for c in &cards {
            assert_eq!(inputs(&s, c, "print")["text"], Json::Null);
        }
        let w = s.snapshot().unwrap();
        assert_eq!(
            w.whole(&badges[0]).unwrap().unwrap().attr("holder_name"),
            Value::Text("Visitor".into())
        );
        assert_eq!(
            w.whole(&badges[1]).unwrap().unwrap().attr("holder_name"),
            Value::Absent
        );
        // the grade it copied leaves the attribute and its interval
        for b in &badges {
            let o = w.whole(b).unwrap().unwrap();
            assert_eq!(o.attr("tier"), Value::Absent);
            assert!(
                o.held_parts().unwrap().spans["tier"]
                    .iter()
                    .all(|sp| sp.value == Value::Absent)
            );
        }
        for c in &cards {
            assert_eq!(w.whole(c).unwrap().unwrap().attr("text"), Value::Absent);
        }
        // every other object redacted gets one redact event, caused by the
        // erasure, naming what it redacted, its version advanced
        let forget = w
            .log()
            .unwrap()
            .rfind(|e| e.transition == "forget")
            .unwrap()
            .position;
        drop(w);
        assert_eq!(
            versions(&s),
            before.iter().map(|v| v + 1).collect::<Vec<_>>()
        );
        let redacted = |s: &Store, id: &str| -> Json {
            let last = s.history(id).unwrap().pop().unwrap();
            assert_eq!(
                (last.transition.as_str(), last.source.as_str(), last.cause),
                ("redact", "erased", Some(forget))
            );
            last.payload["redacted"].clone()
        };
        assert_eq!(redacted(&s, &desk), json!(["full_name", "new_name"]));
        assert_eq!(redacted(&s, &office), json!(["aliases", "who"]));
        assert_eq!(
            redacted(&s, &badges[0]),
            json!(["echo", "holder_name", "tier"])
        );
        assert_eq!(
            redacted(&s, &badges[1]),
            json!(["echo", "holder_name", "tier"])
        );
        assert_eq!(redacted(&s, &cards[0]), json!(["text"]));
        // the erased object records its own erasure, and no redact
        assert!(
            s.history(&person)
                .unwrap()
                .iter()
                .all(|e| e.transition != "redact")
        );
    }

    /// An owned type's external identifiers (`declaration-syntax.md` §4,
    /// `DESIGN.md` §10): a transition that writes one makes `lookup` find the
    /// object by it, and no longer by the value it replaced; it is unique per
    /// source across every type holding one, a value given up taken freely;
    /// and an erased personal one is no longer found.
    #[test]
    fn an_external_identifier_is_found_by_its_source_and_unique_per_source() {
        const BOOKS: &str = r#"
module: books
categories: [live, closed]
types:
  Customer:
    attributes:
      xero_id: { type: string, external: xero, optional: true }
      nick: { type: string, external: crm, optional: true, personal: true }
    states:
      ACTIVE: { category: live }
    transitions:
      add: { kind: initial, to: ACTIVE, optional_inputs: [xero_id, nick] }
      relink: { kind: internal, from: [ACTIVE], required_inputs: [xero_id] }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
  Vendor:
    attributes:
      xero_ref: { type: string, external: xero, optional: true }
    states:
      ACTIVE: { category: live }
    transitions:
      add: { kind: initial, to: ACTIVE, optional_inputs: [xero_ref] }
"#;
        let lib = Library::load(&[("books.yaml".into(), BOOKS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let found = |s: &Store, source: &str, value: &str| {
            s.snapshot()
                .unwrap()
                .external_all()
                .unwrap()
                .get(&(source.to_string(), value.to_string()))
                .cloned()
        };
        // each object's failure, the new one's and that of whichever it
        // collides with
        let refused_as = |v: Verdict| match v {
            Verdict::InvariantViolated { failures, .. } => failures
                .iter()
                .map(|f| format!("{}.{}", f.type_, f.invariant))
                .collect::<Vec<_>>(),
            other => panic!("{other:?}"),
        };
        let c1 = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("Customer", true),
            json!({"xero_id": "X1", "nick": "dd"}),
        ));
        assert_eq!(found(&s, "xero", "X1"), Some(c1.clone()));
        assert_eq!(found(&s, "crm", "dd"), Some(c1.clone()));
        // relinked, it is found by the new value and not the old
        created(&req(
            &mut s,
            "relink",
            OPERATOR,
            (&c1, false),
            json!({"xero_id": "X2"}),
        ));
        assert_eq!(found(&s, "xero", "X1"), None);
        assert_eq!(found(&s, "xero", "X2"), Some(c1.clone()));
        // unique per source, within the type and across types
        assert!(
            refused_as(req(
                &mut s,
                "add",
                OPERATOR,
                ("Customer", true),
                json!({"xero_id": "X2"})
            ))
            .contains(&"Customer.xero_id_unique".to_string())
        );
        // each side of the collision named, the existing one first by id
        assert_eq!(
            refused_as(req(
                &mut s,
                "add",
                OPERATOR,
                ("Vendor", true),
                json!({"xero_ref": "X2"})
            )),
            ["Customer.xero_id_unique", "Vendor.xero_ref_unique"]
        );
        // the value given up is free
        let vendor = created(&req(
            &mut s,
            "add",
            OPERATOR,
            ("Vendor", true),
            json!({"xero_ref": "X1"}),
        ));
        assert_eq!(found(&s, "xero", "X1"), Some(vendor));
        // erased, the personal one is no longer found, the other still is
        created(&req(
            &mut s,
            "forget",
            OPERATOR,
            (&c1, false),
            json!({"reason": "request"}),
        ));
        assert_eq!(found(&s, "crm", "dd"), None);
        assert_eq!(found(&s, "xero", "X2"), Some(c1));
    }

    /// A read of an object's one-to-many end holds that object's parts,
    /// read through the stored end's index, never every object of the
    /// parts' type, which made every read of a claim read every claim line
    /// in the store (PERF-SCAN); and a query by a reference's id reads the
    /// objects naming it, never the type whole.
    #[test]
    fn a_one_to_many_end_is_read_by_its_index_not_its_type_whole() {
        let text = r#"
module: lines
categories: [open, closed]
types:
  Order:
    attributes:
      lines: { reference: "Line[]", opposite: order, aggregation: composite, survives: true }
    derived_attributes:
      size: { description: Its lines., expression: count(l in lines) }
    states:
      OPEN: { category: open }
    transitions:
      place: { kind: initial, to: OPEN }
  Line:
    attributes:
      order: { reference: Order, opposite: lines }
      qty: { type: int }
    states:
      ACTIVE: { category: open }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [order, qty] }
"#;
        let lib = Library::load(&[("lines.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let mut orders = Vec::new();
        for _ in 0..20 {
            let o = created(&req(&mut s, "place", OPERATOR, ("Order", true), json!({})));
            for q in 0..10 {
                created(&req(
                    &mut s,
                    "add",
                    OPERATOR,
                    ("Line", true),
                    json!({"order": &o, "qty": q}),
                ));
            }
            orders.push(o);
        }
        let w = s.snapshot().unwrap();
        let t = w.lib.type_("Order").unwrap();
        let size = Evaluator::new(&w, Some(t), Some(&orders[7]))
            .eval(&crate::expr::parse("size").unwrap())
            .unwrap();
        assert_eq!(size, Value::Int(10));
        // the order and its ten lines, and nothing like the 200 lines held
        assert!(w.held_count() <= 12, "{} held", w.held_count());
        let page = crate::reads::query(
            &w,
            "Line",
            Some(&format!("order.id == \"{}\"", orders[3])),
            &[],
            None,
        )
        .unwrap();
        assert_eq!(page.items.len(), 10);
        assert!(w.held_count() <= 24, "{} held", w.held_count());
    }

    /// A sweep reading each object's parts, as a filter on a derived count
    /// of them does, keys each object it holds into the column indexes a
    /// bounded number of times: a column rebuilt whenever an object was
    /// held keyed every held object again for each one read, and a metric
    /// or filter over 12,000 claims took 77 to 84 s (READ-COST).
    #[test]
    fn a_sweep_keys_what_it_holds_a_bounded_number_of_times() {
        let text = r#"
module: lines
categories: [open, closed]
types:
  Order:
    attributes:
      lines: { reference: "Line[]", opposite: order, aggregation: composite, survives: true }
    derived_attributes:
      size: { description: Its lines., expression: count(l in lines) }
    states:
      OPEN: { category: open }
    transitions:
      place: { kind: initial, to: OPEN }
  Line:
    attributes:
      order: { reference: Order, opposite: lines }
      qty: { type: int }
    states:
      ACTIVE: { category: open }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [order, qty] }
"#;
        let lib = Library::load(&[("lines.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        for i in 0..150 {
            let o = created(&req(&mut s, "place", OPERATOR, ("Order", true), json!({})));
            for q in 0..(i % 6) {
                created(&req(
                    &mut s,
                    "add",
                    OPERATOR,
                    ("Line", true),
                    json!({"order": &o, "qty": q}),
                ));
            }
        }
        let w = s.snapshot().unwrap();
        let page = crate::reads::query(&w, "Order", Some("size > 3"), &[], None).unwrap();
        assert_eq!(page.items.len(), 50);
        let (held, keyed) = (w.held_count(), w.keyed_count());
        assert!(held > 500, "{held} held");
        assert!(keyed <= 3 * held, "{keyed} keyings for {held} held");
    }

    /// A query read page by page in creation order, over a family of two
    /// interleaved types, gives what one read of it gives, each page read
    /// and judged from its cursor a chunk at a time: a page deep in the
    /// result holds the chunks its span needs, where each page read the
    /// family whole, and reading a result page by page was quadratic
    /// (READ-PAGES).
    #[test]
    fn a_query_read_by_pages_reads_each_from_its_cursor() {
        let text = r#"
module: assets
categories: [live]
types:
  Asset:
    abstract: true
    attributes:
      n: { type: int }
  Laptop:
    extends: Asset
    states:
      IN_USE: { category: live }
    transitions:
      receive: { kind: initial, to: IN_USE, required_inputs: [n] }
  Server:
    extends: Asset
    states:
      RACKED: { category: live }
    transitions:
      receive: { kind: initial, to: RACKED, required_inputs: [n] }
"#;
        let lib = Library::load(&[("assets.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        for i in 0..3000 {
            // the two types interleaved unevenly, so a table's chunk ends
            // before the other's
            let tn = if i % 3 == 0 { "Server" } else { "Laptop" };
            created(&req(
                &mut s,
                "receive",
                OPERATOR,
                (tn, true),
                json!({"n": i}),
            ));
        }
        let filter = Some("n - (n / 100) * 100 == 7");
        let whole: Vec<String> = {
            let w = s.snapshot().unwrap();
            crate::reads::query_page(&w, "Asset", filter, &[], None, None, 1000)
                .unwrap()
                .items
                .into_iter()
                .map(|i| i.id)
                .collect()
        };
        assert_eq!(whole.len(), 30);
        let mut paged = Vec::new();
        let mut cursor: Option<String> = None;
        let mut deepest = 0;
        loop {
            let w = s.snapshot().unwrap();
            let page =
                crate::reads::query_page(&w, "Asset", filter, &[], None, cursor.as_deref(), 7)
                    .unwrap();
            assert!(page.refused.is_none(), "{:?}", page.refused);
            paged.extend(page.items.into_iter().map(|i| i.id));
            deepest = deepest.max(w.held_count());
            match page.cursor {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        assert_eq!(paged, whole);
        // a page of 7 spans about 700 objects, past the first chunk of 256
        // from each type's table: never the family's 3,000
        assert!(deepest < 2000, "a page held {deepest} objects");
    }

    /// A store an earlier release left with a sequence's next value wrapped
    /// past 64 bits refuses only a creation that mints from it, as a value
    /// past its bound, and takes every other write, where the allocator's
    /// SQL overflowed to a REAL and every write answered 500 (fourth
    /// production review A-15).
    #[test]
    fn a_wrapped_sequence_refuses_only_what_mints_from_it() {
        let text = r#"
module: desk
categories: [open, closed]
sequences:
  ticket_number: { description: Tickets. }
types:
  Ticket:
    attributes:
      number: { type: string, identifier: { sequence: ticket_number, format: "T-{n}" }, unique: true }
    states:
      OPEN: { category: open }
      DONE: { category: closed, final: true }
    transitions:
      raise: { kind: initial, to: OPEN }
      close: { kind: external, from: OPEN, to: DONE }
  Note:
    attributes:
      text: { type: string }
    states:
      KEPT: { category: open }
    transitions:
      write: { kind: initial, to: KEPT, required_inputs: [text] }
"#;
        let lib = Library::load(&[("desk.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let first = created(&req(&mut s, "raise", OPERATOR, ("Ticket", true), json!({})));
        s.connection()
            .execute(
                "UPDATE of_sequence SET next_value = -9223372036854775808 WHERE name = 'ticket_number'",
                [],
            )
            .unwrap();
        let v = req(&mut s, "raise", OPERATOR, ("Ticket", true), json!({}));
        assert!(
            matches!(&v, Verdict::Unsatisfied { clause, .. } if clause == "value_in_range"),
            "{v:?}"
        );
        let v = req(&mut s, "close", OPERATOR, (&first, false), json!({}));
        assert!(matches!(v, Verdict::Satisfied { .. }), "{v:?}");
        let v = req(
            &mut s,
            "write",
            OPERATOR,
            ("Note", true),
            json!({"text": "kept"}),
        );
        assert!(matches!(v, Verdict::Satisfied { .. }), "{v:?}");
    }

    /// An external identifier's uniqueness, checked on each object of a
    /// store held whole, as a dry run checks it, is looked up among the
    /// objects holding the same identifier only: the objects holding none
    /// were gathered for every check, and a publish on 110k objects held
    /// every write 44 s, 36 s of it there (PUBLISH-SCALE).
    #[test]
    fn an_identifiers_check_gathers_only_its_matches() {
        let text = r#"
module: bills
categories: [open, closed]
types:
  Bill:
    attributes:
      ref: { type: string, external: ledger, optional: true }
    states:
      OPEN: { category: open }
    transitions:
      raise: { kind: initial, to: OPEN, optional_inputs: [ref] }
"#;
        let lib = Library::load(&[("bills.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        for i in 0..400 {
            let inputs = if i % 20 == 0 {
                json!({"ref": format!("L{i}")})
            } else {
                json!({})
            };
            created(&req(&mut s, "raise", OPERATOR, ("Bill", true), inputs));
        }
        let w = s.snapshot().unwrap();
        let all = w.in_order().unwrap();
        let mut checked = 0;
        for o in all.iter().filter(|o| o.type_ == "Bill") {
            assert!(failing_invariants(&w, o).unwrap().is_empty());
            checked += usize::from(!o.attr("ref").is_absent());
        }
        assert_eq!(checked, 20);
        let gathered = w.gathered_count();
        assert!(
            gathered <= 2 * checked,
            "{gathered} gathered for {checked} checks"
        );
    }

    /// An identity's check on each actor of a store held whole, as a
    /// dry run checks every object, looks the identity's folded value up,
    /// sweeping no actors: a sweep of every actor for each actor checked
    /// made a publish grow with the square of the actors, 70 s of approval
    /// with 3,600 (seventh production review PUBLISH-SLOW, ADR-0194
    /// decision 1).
    #[test]
    fn an_identitys_check_sweeps_no_actors() {
        let text = r#"
module: people
categories: [live, closed]
types:
  Person:
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states:
      ACTIVE: { category: live }
    transitions:
      join: { kind: initial, to: ACTIVE, required_inputs: [login] }
"#;
        let lib = Library::load(&[("people.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        for i in 0..300 {
            created(&req(
                &mut s,
                "join",
                OPERATOR,
                ("Person", true),
                json!({"login": format!("P{i}")}),
            ));
        }
        let w = s.snapshot().unwrap();
        let all = w.in_order().unwrap();
        let before = w.enumerated_count();
        let mut checked = 0;
        for o in all.iter().filter(|o| o.type_ == "Person") {
            assert!(failing_invariants(&w, o).unwrap().is_empty());
            checked += 1;
        }
        assert_eq!(checked, 300);
        let swept = w.enumerated_count() - before;
        assert!(
            swept <= 4 * checked,
            "{swept} objects swept for {checked} checks"
        );
    }

    /// An actor named by its identity in another case of its ASCII letters
    /// is that actor, recorded under the identity it holds, as uniqueness
    /// compares them; one differing beyond ASCII case is no actor (ADR-0194
    /// decision 5).
    #[test]
    fn an_actor_named_in_another_case_is_that_actor() {
        let text = r#"
module: notes
categories: [live, closed]
types:
  Person:
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states:
      ACTIVE: { category: live }
    transitions:
      join: { kind: initial, to: ACTIVE, required_inputs: [login] }
  Note:
    attributes:
      text: { type: string, optional: true }
    states:
      OPEN: { category: live }
    transitions:
      jot: { kind: initial, to: OPEN }
"#;
        let lib = Library::load(&[("notes.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        created(&req(
            &mut s,
            "join",
            OPERATOR,
            ("Person", true),
            json!({"login": "Ana.Lee"}),
        ));
        let note = created(&req(&mut s, "jot", "ANA.LEE", ("Note", true), json!({})));
        let w = s.snapshot().unwrap();
        let e = w.log().unwrap().find(|e| e.object_id == note).unwrap();
        assert_eq!(
            (e.actor_id.as_str(), e.actor_kind.as_str()),
            ("Ana.Lee", "human")
        );
        drop(w);
        assert!(matches!(
            req(&mut s, "jot", "ANA.LÉE", ("Note", true), json!({})),
            Verdict::Unsatisfied { ref clause, .. } if clause == "actor_known"
        ));
    }

    /// `referrers` reads each stored end that may name the object by its
    /// column's index, never a type whole: it read the store whole for each
    /// object, a filter over a type running to its deadline at 1 GB (seventh
    /// production review REFERRERS, ADR-0194 decision 3).
    #[test]
    fn referrers_are_read_by_index_never_a_type_whole() {
        let text = r#"
module: crm
categories: [live, closed]
types:
  Person:
    description: A person.
    tracking: record
    attributes:
      name: { type: string }
    states:
      ACTIVE: { category: live, description: Here. }
      GONE: { category: closed, final: true, description: Gone. }
    conditions:
      unreferenced:
        description: Nothing refers to the person.
        expression: count(r in referrers) == 0
        remedy: dependent
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [name] }
      remove: { kind: external, from: ACTIVE, to: GONE, guards: { unreferenced: deny } }
  Team:
    description: A team a person leads.
    tracking: record
    attributes:
      lead: { reference: Person }
    states:
      OPEN: { category: live, description: Working. }
      DISBANDED: { category: closed, final: true, description: Ended. }
    transitions:
      form: { kind: initial, to: OPEN, required_inputs: [lead] }
      disband: { kind: external, from: OPEN, to: DISBANDED }
  Order:
    description: An order a person places.
    tracking: record
    attributes:
      buyer: { reference: Person }
    states:
      OPEN: { category: live, description: Placed. }
      CLOSED: { category: closed, final: true, description: Done. }
    transitions:
      place: { kind: initial, to: OPEN, required_inputs: [buyer] }
      close: { kind: external, from: OPEN, to: CLOSED }
"#;
        let lib = Library::load(&[("crm.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let people: Vec<String> = (0..120)
            .map(|i| {
                created(&req(
                    &mut s,
                    "add",
                    OPERATOR,
                    ("Person", true),
                    json!({"name": format!("P{i}")}),
                ))
            })
            .collect();
        for p in &people[..60] {
            created(&req(
                &mut s,
                "place",
                OPERATOR,
                ("Order", true),
                json!({"buyer": p}),
            ));
        }
        created(&req(
            &mut s,
            "form",
            OPERATOR,
            ("Team", true),
            json!({"lead": people[100]}),
        ));
        let whole = || {
            (
                crate::world::whole_type_reads("Order"),
                crate::world::whole_type_reads("Team"),
            )
        };
        let before = whole();
        let refused = |v: &Verdict| matches!(v, Verdict::Unsatisfied { clause, .. } if clause == "unreferenced");
        assert!(refused(&req(
            &mut s,
            "remove",
            OPERATOR,
            (&people[3], false),
            json!({})
        )));
        assert!(refused(&req(
            &mut s,
            "remove",
            OPERATOR,
            (&people[100], false),
            json!({})
        )));
        assert!(matches!(
            req(&mut s, "remove", OPERATOR, (&people[110], false), json!({})),
            Verdict::Satisfied { .. }
        ));
        assert_eq!(whole(), before);
    }

    /// A page of history ends at [`EVENT_BYTES`] of events as at its limit,
    /// with a cursor going on, and an answer of events past them is refused
    /// with how many positions fit, where a page of 1,000 events of large
    /// inputs answered 123 MB (seventh production review HISTORY, ADR-0194
    /// decision 6).
    #[test]
    fn a_page_of_events_ends_at_its_bytes() {
        let text = r#"
module: notes
categories: [live]
types:
  Note:
    attributes:
      text: { type: string, optional: true }
    states:
      OPEN: { category: live }
    transitions:
      jot: { kind: initial, to: OPEN }
      write: { kind: internal, from: OPEN, required_inputs: [text] }
"#;
        let lib = Library::load(&[("notes.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let note = created(&req(&mut s, "jot", OPERATOR, ("Note", true), json!({})));
        let big = "x".repeat(60_000);
        for _ in 0..200 {
            created(&req(
                &mut s,
                "write",
                OPERATOR,
                (&note, false),
                json!({"text": big}),
            ));
        }
        let (page, next) = s
            .history_page(std::slice::from_ref(&note), 0, 1000)
            .unwrap();
        assert!(page.len() > 50 && page.len() < 200, "{} events", page.len());
        assert_eq!(next, page.last().map(|e| e.position));
        let bytes: usize = page.iter().map(crate::store::event_bytes).sum();
        assert!(bytes < crate::store::EVENT_BYTES + 100_000, "{bytes}");
        let (rest, _) = s
            .history_page(std::slice::from_ref(&note), next.unwrap_or(0), 1000)
            .unwrap();
        assert_eq!(page.len() + rest.len(), 201);
        let positions: Vec<i64> = page.iter().chain(&rest).map(|e| e.position).collect();
        assert!(matches!(s.events_within(&positions).unwrap(), Err(fit) if fit < positions.len()));
        assert!(s.events_within(&positions[..10]).unwrap().is_ok());
    }

    /// A subscription's lag reads no event whole, and a page of `pull` or of
    /// the log's `export` reads whole only the events it holds and one chunk
    /// more, where a `get` of a lagging subscription read 256 events whole
    /// and a page read its limit before cutting at its bytes (tenth
    /// production review).
    #[test]
    fn a_subscriptions_reads_hold_only_the_events_they_answer() {
        let text = r#"
module: notes
categories: [live]
types:
  Note:
    attributes:
      text: { type: string, optional: true }
    states:
      OPEN: { category: live }
    transitions:
      jot: { kind: initial, to: OPEN, optional_inputs: [text] }
"#;
        let lib = Library::load(&[("notes.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let sub = created(&req(
            &mut s,
            "subscribe",
            OPERATOR,
            (crate::model::SUBSCRIPTION, true),
            json!({"target_type": "Note", "target_family": false}),
        ));
        let big = "x".repeat(60_000);
        for _ in 0..300 {
            created(&req(
                &mut s,
                "jot",
                OPERATOR,
                ("Note", true),
                json!({"text": big}),
            ));
        }
        s.now += 3600;
        let w = s.snapshot().unwrap();
        let o = w.get(&sub).unwrap().unwrap().clone();
        let before = crate::world::events_read();
        let (lag, _, _) = crate::subscription::lag(&w, &o).unwrap();
        assert!(lag >= 3600, "{lag}");
        assert_eq!(
            crate::world::events_read() - before,
            0,
            "the lag read events whole"
        );
        let before = crate::world::events_read();
        let page = crate::subscription::pull_page(&w, &sub, None, 30).unwrap();
        assert_eq!(page.events.len(), 30);
        assert!(
            crate::world::events_read() - before <= 30,
            "a pull of 30 read {}",
            crate::world::events_read() - before
        );
        let before = crate::world::events_read();
        let out = crate::export::export_page(&w, "log", Some("0"), 1000).unwrap();
        let read = crate::world::events_read() - before;
        assert!(
            read <= out.lines.len() as u64 + 16,
            "an export of {} read {read}",
            out.lines.len()
        );
    }

    /// A derived value computed once in one evaluation, where a chain of
    /// attributes each reading the one before twice computed each 2^n times
    /// and a read of twenty took a second of a core (eleventh production
    /// review).
    #[test]
    fn a_derived_chain_is_computed_once_per_read() {
        let mut text = String::from(
            "\nmodule: chain\ncategories: [live]\ntypes:\n  T:\n    attributes:\n      x: { type: int, optional: true }\n    derived_attributes:\n      d0: { expression: \"x + x\" }\n",
        );
        for i in 1..22 {
            text.push_str(&format!(
                "      d{i}: {{ expression: \"d{} + d{}\" }}\n",
                i - 1,
                i - 1
            ));
        }
        text.push_str("    states:\n      ON: { category: live }\n    transitions:\n      make: { kind: initial, to: ON, optional_inputs: [x] }\n");
        let lib = Library::load(&[("chain.yaml".into(), text)]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let o = created(&req(&mut s, "make", OPERATOR, ("T", true), json!({"x": 1})));
        let w = s.snapshot().unwrap();
        let t = std::time::Instant::now();
        let read = crate::reads::get(&w, &o).unwrap().unwrap();
        let took = t.elapsed().as_secs_f64();
        let d21 = read
            .derived
            .iter()
            .find(|(d, _)| d == "d21")
            .map(|(_, v)| v.clone());
        assert_eq!(d21, Some(Value::Int(1 << 22)), "{read:?}");
        assert!(took < 2.0, "a read of a chain of 22 took {took:.2} s");
        // at zero each value is given its type by its expression's, typed
        // once each too
        let zero = created(&req(&mut s, "make", OPERATOR, ("T", true), json!({"x": 0})));
        let w = s.snapshot().unwrap();
        let t = std::time::Instant::now();
        let read = crate::reads::get(&w, &zero).unwrap().unwrap();
        let took = t.elapsed().as_secs_f64();
        assert!(
            read.derived
                .iter()
                .any(|(d, v)| d == "d21" && *v == Value::Int(0)),
            "{read:?}"
        );
        assert!(
            took < 2.0,
            "a read of a chain of 22 at zero took {took:.2} s"
        );
    }

    /// A personal value passed to an evaluator is recorded as `(personal)`
    /// in what a guard consulted, on the event, a refusal's attempt and the
    /// log's export, where the address was written and outlived an erasure
    /// (thirteenth production review A).
    #[test]
    fn a_personal_argument_is_consulted_as_personal() {
        let text = "\nmodule: crm\ncategories: [live]\nevaluators:\n  crm:\n    functions:\n      known: { arguments: { email: { type: string } } }\ntypes:\n  Customer:\n    attributes:\n      email: { type: string, personal: true }\n    states:\n      ON: { category: live }\n    conditions:\n      known: { expression: crm.known(email), remedy: dependent }\n    transitions:\n      make: { kind: initial, to: ON, required_inputs: [email] }\n      check: { kind: internal, from: [ON], guards: { known: deny } }\n";
        let lib = Library::load(&[("crm.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let c = created(&req(
            &mut s,
            "make",
            OPERATOR,
            ("Customer", true),
            json!({"email": "zelda.secret@example.com"}),
        ));
        s.answers.insert("crm.known".into(), "satisfied".into());
        assert_eq!(
            created(&req(&mut s, "check", OPERATOR, (&c, false), json!({}))),
            c
        );
        let checked = s.history(&c).unwrap().pop().unwrap();
        assert_eq!(
            checked.payload["consulted"]["known"]["values"],
            json!({ "crm.known((personal))": "satisfied" })
        );
        s.answers.insert("crm.known".into(), "unsatisfied".into());
        assert!(matches!(
            req(&mut s, "check", OPERATOR, (&c, false), json!({})),
            Verdict::Unsatisfied { .. }
        ));
        let w = s.snapshot().unwrap();
        let row = w.attempts().unwrap().last().unwrap();
        assert_eq!(
            row.consulted["known"]["values"],
            json!({ "crm.known((personal))": "unsatisfied" })
        );
        let page = crate::export::export_page(&w, "log", None, 100).unwrap();
        assert!(
            !page.lines.iter().any(|l| l.contains("zelda")),
            "{:?}",
            page.lines
        );
    }

    /// An object named beside a type is found only as one of that type's
    /// family, so a tool bound to a type takes no other type's object of the
    /// id it is given: an agent's `customer_revoke` revoked a person's
    /// subscription (thirteenth production review B).
    #[test]
    fn an_object_named_beside_another_type_is_not_found() {
        let text = "\nmodule: two\ncategories: [live, closed]\ntypes:\n  Base:\n    abstract: true\n    attributes:\n      n: { type: int, optional: true }\n  Customer:\n    extends: Base\n    states:\n      ON: { category: live }\n      OFF: { category: closed, final: true }\n    transitions:\n      make: { kind: initial, to: ON }\n      revoke: { kind: external, from: [ON], to: OFF }\n  Pass:\n    states:\n      ON: { category: live }\n      OFF: { category: closed, final: true }\n    transitions:\n      make: { kind: initial, to: ON }\n      revoke: { kind: external, from: [ON], to: OFF }\n";
        let lib = Library::load(&[("two.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let customer = created(&req(
            &mut s,
            "make",
            OPERATOR,
            ("Customer", true),
            json!({}),
        ));
        let pass = created(&req(&mut s, "make", OPERATOR, ("Pass", true), json!({})));
        let revoke = |s: &mut Store, id: &str, type_: Option<&str>| {
            request(
                s,
                &Request {
                    transition: "revoke".into(),
                    actor: OPERATOR.into(),
                    object_id: Some(id.to_string()),
                    type_: type_.map(str::to_string),
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
            .unwrap()
            .unwrap()
        };
        assert_eq!(revoke(&mut s, &pass, Some("Customer")), Verdict::NotFound);
        assert!(matches!(
            revoke(&mut s, &customer, Some("Base")),
            Verdict::Satisfied { .. }
        ));
        assert!(matches!(
            revoke(&mut s, &pass, Some("Pass")),
            Verdict::Satisfied { .. }
        ));
    }

    /// A transition calling itself down a reference chain, which both
    /// checkers refuse and an unchecked load may hold, stops past
    /// [`CALL_DEPTH`] links as not run, where 3,000 aborted the process on
    /// a stack overflow (twelfth production review).
    #[test]
    fn a_chain_of_calls_stops_without_a_crash() {
        // on a stack as large as the service's request threads have
        // (`objectflow-service`'s `WORKER_STACK`), as a request runs
        let ran = std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(chain_of_calls)
            .unwrap()
            .join();
        assert!(ran.is_ok());
    }

    fn chain_of_calls() {
        let text = "\nmodule: chain\ncategories: [live]\ntypes:\n  Node:\n    attributes:\n      parent: { reference: Node, optional: true }\n    states:\n      ON: { category: live }\n    transitions:\n      make: { kind: initial, to: ON, optional_inputs: [parent] }\n      poke: { kind: internal, from: [ON], effect: [ { call: { target: parent, transition: poke } } ] }\n";
        let lib = Library::load(&[("chain.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let mut last = created(&req(&mut s, "make", OPERATOR, ("Node", true), json!({})));
        for _ in 0..3_000 {
            last = created(&req(
                &mut s,
                "make",
                OPERATOR,
                ("Node", true),
                json!({"parent": last}),
            ));
        }
        let stopped = request(
            &mut s,
            &Request {
                transition: "poke".into(),
                actor: OPERATOR.into(),
                object_id: Some(last),
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
        );
        assert!(
            matches!(&stopped, Ok(Err(NoVerdict::NotRun(NotRun::Unsupported(why)))) if why.contains("deeper than 64")),
            "{stopped:?}"
        );
    }

    /// A derived attribute reading itself through a set, which a version
    /// installed before the checkers refused it may hold, answers a read of
    /// a leaf, where typing its sum over nothing recursed until the stack
    /// overflowed and aborted the service (eleventh production review).
    #[test]
    fn a_derived_value_reading_itself_through_a_set_answers_a_leaf() {
        let text = "\nmodule: tree\ncategories: [live]\ntypes:\n  Node:\n    attributes:\n      parent: { reference: Node, optional: true, opposite: children }\n      children: { reference: \"Node[]\", opposite: parent }\n    derived_attributes:\n      size: { expression: \"count(c in children) + sum(c in children: c.size)\" }\n    states:\n      LIVE: { category: live }\n    transitions:\n      make: { kind: initial, to: LIVE, optional_inputs: [parent] }\n";
        let lib = Library::load(&[("tree.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let root = created(&req(&mut s, "make", OPERATOR, ("Node", true), json!({})));
        let leaf = created(&req(
            &mut s,
            "make",
            OPERATOR,
            ("Node", true),
            json!({"parent": root}),
        ));
        let w = s.snapshot().unwrap();
        let size = |id: &str| {
            crate::reads::get(&w, id)
                .unwrap()
                .unwrap()
                .derived
                .into_iter()
                .find(|(d, _)| d == "size")
                .map(|(_, v)| v)
        };
        assert_eq!(size(&leaf), Some(Value::Int(0)));
        assert_eq!(size(&root), Some(Value::Int(1)));
    }

    /// A derived attribute reading itself through a reference, which a
    /// version installed before the checkers refused it may hold, is not run
    /// past [`crate::eval::DERIVED_HOPS`] references, where a cycle of two
    /// nodes, or a chain of thousands, overflowed the stack and aborted the
    /// service on a `get` (tenth production review), and where it was then
    /// answered as none (fourteenth production review N1); a chain within
    /// the bound is read; and an identifier's width past what a number fills
    /// refuses its creation as not run, where it panicked (tenth production
    /// review).
    #[test]
    fn a_recursive_derived_value_and_a_wide_identifier_stop_without_a_crash() {
        let text = r#"
module: tree
categories: [live]
sequences:
  node_number:
    description: Numbers nodes.
types:
  Node:
    attributes:
      code: { type: string, identifier: { sequence: node_number, format: "N-{n:1000000}" } }
      parent: { reference: Node, optional: true }
    derived_attributes:
      depth: { expression: "parent.depth + 1" }
    states:
      ON: { category: live }
    transitions:
      make: { kind: initial, to: ON, optional_inputs: [parent] }
      hang: { kind: internal, from: ON, required_inputs: [parent] }
"#;
        let lib = Library::load(&[("tree.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        // the identifier's width: refused as not run, never a panic
        let r = Request {
            transition: "make".into(),
            actor: OPERATOR.into(),
            object_id: None,
            type_: Some("Node".into()),
            inputs: BTreeMap::new(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
        };
        let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| request(&mut s, &r)));
        assert!(matches!(made, Ok(Err(_)) | Ok(Ok(Err(_)))), "{made:?}");
        // the same flow without the identifier, so nodes can be made
        let lib = Library::load(&[(
            "tree.yaml".into(),
            text.replace(
                "      code: { type: string, identifier: { sequence: node_number, format: \"N-{n:1000000}\" } }\n",
                "",
            ),
        )])
        .unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let a = created(&req(&mut s, "make", OPERATOR, ("Node", true), json!({})));
        let b = created(&req(
            &mut s,
            "make",
            OPERATOR,
            ("Node", true),
            json!({"parent": a}),
        ));
        // a cycle of two
        created(&req(
            &mut s,
            "hang",
            OPERATOR,
            (&a, false),
            json!({"parent": b}),
        ));
        let w = s.snapshot().unwrap();
        let read = crate::reads::get(&w, &a);
        assert!(
            matches!(&read, Err(NotRun::Unsupported(m)) if m.contains("more than 64 deep")),
            "{read:?}"
        );
        drop(w);
        // a chain within the bound read, its depth counted
        let root = created(&req(&mut s, "make", OPERATOR, ("Node", true), json!({})));
        let mut last = root.clone();
        for _ in 0..40 {
            last = created(&req(
                &mut s,
                "make",
                OPERATOR,
                ("Node", true),
                json!({"parent": last}),
            ));
        }
        let w = s.snapshot().unwrap();
        let read = crate::reads::get(&w, &last).unwrap().unwrap();
        let depth = read
            .derived
            .iter()
            .find(|(d, _)| d == "depth")
            .map(|(_, v)| v.clone());
        // the root's parent absent, so its depth is, and each below unknown
        // past it: what matters is that it is read, not refused
        assert!(depth.is_some(), "{read:?}");
        drop(w);
        // and a chain of 200, its deep end not run, never a crash
        for _ in 0..200 {
            last = created(&req(
                &mut s,
                "make",
                OPERATOR,
                ("Node", true),
                json!({"parent": last}),
            ));
        }
        let w = s.snapshot().unwrap();
        assert!(crate::reads::get(&w, &last).is_err());
    }

    /// A page of `query`, `pull` and `export` ends at [`PAGE_BYTES`] as at
    /// its limit, one item at least, its cursor going on, and the next page
    /// takes up where it ended: nothing passed over, nothing twice; `pull`
    /// and `export` say `more` where a page ended so, where 1,000 objects
    /// of three 64 KiB strings answered 197 MB a page (ninth production
    /// review N-9).
    ///
    /// [`PAGE_BYTES`]: crate::reads::PAGE_BYTES
    #[test]
    fn a_page_of_a_query_a_pull_or_an_export_ends_at_its_bytes() {
        let text = r#"
module: notes
categories: [live]
types:
  Note:
    attributes:
      text: { type: string, optional: true }
    states:
      OPEN: { category: live }
    transitions:
      jot: { kind: initial, to: OPEN, optional_inputs: [text] }
"#;
        let lib = Library::load(&[("notes.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let sub = created(&req(
            &mut s,
            "subscribe",
            OPERATOR,
            (crate::model::SUBSCRIPTION, true),
            json!({"target_type": "Note", "target_family": false}),
        ));
        let big = "x".repeat(60_000);
        let mut made = Vec::new();
        for _ in 0..200 {
            made.push(created(&req(
                &mut s,
                "jot",
                OPERATOR,
                ("Note", true),
                json!({"text": big}),
            )));
        }
        let w = s.snapshot().unwrap();
        let bound = crate::reads::PAGE_BYTES + 200_000;
        // a query asking for the text
        let fields = ["text".to_string()];
        let first =
            crate::reads::query_page(&w, "Note", None, &[], Some(&fields), None, 1000).unwrap();
        assert!(
            first.items.len() > 50 && first.items.len() < 200,
            "{} items",
            first.items.len()
        );
        let bytes: usize = first.items.len() * 60_000;
        assert!(bytes < bound, "{bytes}");
        let rest = crate::reads::query_page(
            &w,
            "Note",
            None,
            &[],
            Some(&fields),
            first.cursor.as_deref(),
            1000,
        )
        .unwrap();
        let ids: Vec<&String> = first
            .items
            .iter()
            .chain(&rest.items)
            .map(|i| &i.id)
            .collect();
        assert_eq!(ids, made.iter().collect::<Vec<_>>());
        assert_eq!(rest.cursor, None);
        // a subscription's pull
        let page = crate::subscription::pull_page(&w, &sub, None, 1000).unwrap();
        assert!(
            page.more && page.events.len() > 50 && page.events.len() < 200,
            "{}",
            page.events.len()
        );
        let pulled: usize = page.events.iter().map(crate::store::event_bytes).sum();
        assert!(pulled < bound, "{pulled}");
        let next = crate::subscription::pull_page(&w, &sub, Some(page.settled), 1000).unwrap();
        assert!(!next.more);
        assert_eq!(page.events.len() + next.events.len(), 200);
        // the log's export, its rows one per event
        let out = crate::export::export_page(&w, "log", Some("0"), 1000).unwrap();
        assert!(out.more && out.lines.len() < 200, "{}", out.lines.len());
        let exported: usize = out.lines.iter().map(String::len).sum();
        assert!(exported < bound, "{exported}");
        let after = crate::export::export_page(&w, "log", Some(&out.cursor), 1000).unwrap();
        assert!(!after.more);
        let positions = |p: &crate::export::ExportPage| -> Vec<i64> {
            p.lines
                .iter()
                .map(|l| {
                    serde_json::from_str::<Json>(l).unwrap()["position"]
                        .as_i64()
                        .unwrap()
                })
                .collect()
        };
        let all: Vec<i64> = positions(&out)
            .into_iter()
            .chain(positions(&after))
            .collect();
        assert!(all.windows(2).all(|p| p[0] < p[1]), "out of order or twice");
        assert_eq!(all.len(), w.last_position().unwrap() as usize);
    }

    /// `available` reads the objects in a state its transition is taken
    /// from, by the state's index, and each one's offer of that transition
    /// alone: a worklist with nothing to find read every claim of the type,
    /// 41 s at 12,000 claims (READ-COST).
    #[test]
    fn available_reads_only_the_objects_its_transition_is_taken_from() {
        let text = r#"
module: desk
categories: [open, closed]
types:
  Ticket:
    attributes:
      note: { type: string, optional: true }
    states:
      OPEN: { category: open }
      DONE: { category: closed, final: true }
    transitions:
      open: { kind: initial, to: OPEN }
      close: { kind: external, from: OPEN, to: DONE }
"#;
        let lib = Library::load(&[("desk.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        for i in 0..120 {
            let t = created(&req(&mut s, "open", OPERATOR, ("Ticket", true), json!({})));
            if i >= 20 {
                req(&mut s, "close", OPERATOR, (&t, false), json!({}));
            }
        }
        let w = s.snapshot().unwrap();
        let page = crate::reads::available_page(&w, OPERATOR, "Ticket", "close", None, 50).unwrap();
        assert_eq!(page.items.len(), 20);
        assert!(w.held_count() <= 25, "{} held", w.held_count());
    }

    /// An actor named by its identity is found by the identity's column,
    /// holding the actor alone: reading every object of each actor type for
    /// each lookup made a person's submit on 37k objects run past 20 minutes,
    /// the dry run resolving its submitter for each object (DRYRUN-ACTOR).
    #[test]
    fn an_actor_named_by_identity_is_found_by_its_column() {
        let text = r#"
module: people
categories: [live, closed]
types:
  User:
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states:
      ACTIVE: { category: live }
      LEFT: { category: closed, final: true }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [login] }
      leave: { kind: external, from: ACTIVE, to: LEFT }
"#;
        let lib = Library::load(&[("people.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        for i in 0..200 {
            created(&req(
                &mut s,
                "add",
                OPERATOR,
                ("User", true),
                json!({"login": format!("u{i}@corp.example")}),
            ));
        }
        let w = s.snapshot().unwrap();
        let who = resolve_actor(&w, "u150@corp.example").unwrap().unwrap();
        assert_eq!(
            (who.id.as_str(), who.kind.as_str()),
            ("u150@corp.example", "human")
        );
        assert!(resolve_actor(&w, "nobody@corp.example").unwrap().is_none());
        assert!(w.held_count() <= 3, "{} held", w.held_count());
    }

    /// A submit takes the dry run prepared on a read beside the writes
    /// where the change and the store's version are what the read saw, and
    /// computes its own where either moved, so no write waits for a dry run
    /// and none records one of another state (DRYRUN-LOCK).
    #[test]
    fn a_submit_takes_a_prepared_dry_run_only_where_nothing_moved() {
        let module = "module: post\ncategories: [open, closed]\ntypes:\n  Parcel:\n    description: A parcel.\n    tracking: record\n    attributes:\n      code: { type: string }\n    states:\n      OPEN: { category: open }\n      DONE: { category: closed, final: true }\n    transitions:\n      send: { kind: initial, to: OPEN, required_inputs: [code] }\n      deliver: { kind: external, from: OPEN, to: DONE }\n";
        let mut s = Store::sandbox(Library::load(&[]).unwrap(), 0).unwrap();
        let draft = |s: &mut Store| {
            created(&req(
                s,
                "draft",
                OPERATOR,
                ("DeclarationChange", true),
                json!({"source": {"post": module}, "source_ref": "test"}),
            ))
        };
        let marked = |s: &Store, c: &str| {
            let mut p = crate::publish::prepare(&s.snapshot().unwrap(), c, OPERATOR)
                .unwrap()
                .unwrap();
            p.report.person_dimensions.push("prepared.marker".into());
            p
        };
        let report = |s: &Store, c: &str| match s
            .snapshot()
            .unwrap()
            .get(c)
            .unwrap()
            .unwrap()
            .attr("report")
        {
            Value::Text(t) => t,
            other => panic!("{other:?}"),
        };
        // nothing moved: the prepared report is the one recorded
        let first = draft(&mut s);
        s.prepared = Some(Box::new(marked(&s, &first)));
        let v = req(&mut s, "submit", OPERATOR, (&first, false), json!({}));
        assert!(matches!(v, Verdict::Satisfied { .. }), "{v:?}");
        assert!(report(&s, &first).contains("prepared.marker"));
        // the change revised since the read: computed again
        let second = draft(&mut s);
        let stale = marked(&s, &second);
        let revised = req(
            &mut s,
            "revise",
            OPERATOR,
            (&second, false),
            json!({"source": {"post": module.replace("A parcel.", "A parcel, revised.")}, "source_ref": "test"}),
        );
        assert!(matches!(revised, Verdict::Satisfied { .. }), "{revised:?}");
        s.prepared = Some(Box::new(stale));
        let v = req(&mut s, "submit", OPERATOR, (&second, false), json!({}));
        assert!(matches!(v, Verdict::Satisfied { .. }), "{v:?}");
        assert!(!report(&s, &second).contains("prepared.marker"));
    }

    /// A refused request's row of the attempt log keeps the values of the
    /// inputs its transition declares, and names the rest withheld: what was
    /// sent under a name no declaration marks, or to a transition there is
    /// none of, is no value an erasure could reach (ATTEMPT-INPUTS).
    #[test]
    fn a_refused_attempt_keeps_no_input_its_transition_does_not_declare() {
        let text = r#"
module: desk
categories: [open, closed]
types:
  Ticket:
    attributes:
      code: { type: string }
    states:
      OPEN: { category: open }
      DONE: { category: closed, final: true }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [code] }
      close: { kind: external, from: OPEN, to: DONE }
"#;
        let lib = Library::load(&[("desk.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        // refused, an input it does not take among the declared one
        req(
            &mut s,
            "open",
            OPERATOR,
            ("Ticket", true),
            json!({"code": "T-1", "note": "Ysolde Quillfeather's number"}),
        );
        // refused, a transition the type has none of
        req(
            &mut s,
            "clsoe",
            OPERATOR,
            ("Ticket", true),
            json!({"why": "Ysolde again"}),
        );
        let w = s.snapshot().unwrap();
        let rows = w.attempts().unwrap();
        assert_eq!(rows.len(), 2, "{rows:?}");
        let held = format!("{rows:?}");
        assert!(!held.contains("Ysolde"), "{held}");
        assert!(
            held.contains("T-1"),
            "the declared input's value is kept: {held}"
        );
        assert!(rows[0].withheld.contains(&"note".to_string()), "{rows:?}");
        assert!(rows[1].withheld.contains(&"why".to_string()), "{rows:?}");
    }

    /// The operator's retirement is requested by nothing, as its creation
    /// is: it is irreversible, and an agent that retired the operator left
    /// the store with no one answerable for it (ADR-0184 decision 1). It is
    /// refused as not requestable, offered to no one, and `available` for
    /// it refuses; the operator stays active.
    #[test]
    fn the_operators_retirement_is_requested_by_nothing() {
        let mut s = Store::sandbox(Library::load(&[]).unwrap(), 0).unwrap();
        let v = req(&mut s, "retire", OPERATOR, (OPERATOR, false), json!({}));
        assert!(matches!(v, Verdict::NotRequestable { .. }), "{v:?}");
        let w = s.snapshot().unwrap();
        assert_eq!(w.get(OPERATOR).unwrap().unwrap().state, "ACTIVE");
        let offers = crate::reads::availability(&w, OPERATOR, OPERATOR)
            .unwrap()
            .unwrap();
        assert!(
            offers.iter().all(|o| o.transition != "retire"),
            "{offers:?}"
        );
        let page =
            crate::reads::available_page(&w, OPERATOR, "Operator", "retire", None, 10).unwrap();
        assert_eq!(page.refused.as_deref(), Some("not_requestable"));
    }

    const TALLY: &str = "module: tally
categories: [live, closed]
types:
  Tally:
    description: A count.
    tracking: record
    attributes:
      n: { type: int }
    states:
      OPEN: { category: live, description: Open. }
      DONE: { category: closed, final: true, description: Done. }
    conditions:
      small: { description: Small., expression: n * 2 < 100, remedy: unreachable_from_here }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [n] }
      add:
        kind: internal
        from: OPEN
        inputs: { by: { type: int } }
        effect:
          - assign: { location: n, expr: n + inputs.by }
      negate:
        kind: internal
        from: OPEN
        effect:
          - assign: { location: n, expr: -n }
      halve:
        kind: internal
        from: OPEN
        inputs: { by: { type: int } }
        effect:
          - assign: { location: n, expr: n / inputs.by }
      check: { kind: internal, from: OPEN, guards: { small: deny } }
";

    /// An effect whose arithmetic of present values has none, an integer
    /// past 64 bits, a negation of the least or a division by zero, refuses
    /// its request as `value_in_range`, naming its object and storing
    /// nothing, where it stopped not run; a guard over the same arithmetic
    /// refuses it so too, where it was unknown (production review F5,
    /// ADR-0185, ADR-0205 decision 2).
    const VALS: &str = r#"module: vals
categories: [live, closed]
types:
  Item:
    description: Values held at their types.
    tracking: record
    attributes:
      d2: { type: "decimal(10,2)", default: 5 }
      d4: { type: "decimal(12,4)", optional: true }
      n: { type: int, optional: true }
      label: { type: string, optional: true }
      tags: { type: "string[]", optional: true }
      big: { type: int, optional: true }
      huge: { type: int, optional: true }
    derived_attributes:
      twice: { expression: huge * 2 }
    invariants:
      doubled: { expression: big is null or big * 2 <= 100 }
    conditions:
      decided: { description: Decided on its left., expression: label is null or huge * 2 > 0, remedy: unreachable_from_here }
    states:
      OPEN: { category: live, description: Open. }
    transitions:
      open: { kind: initial, to: OPEN, optional_inputs: [big, d2] }
      third:
        kind: internal
        from: OPEN
        effect:
          - assign: { location: d4, expr: d2 / 3 }
      count:
        kind: internal
        from: OPEN
        inputs: { k: { type: int } }
        effect:
          - assign: { location: d2, expr: inputs.k }
      price:
        kind: internal
        from: OPEN
        inputs: { p: { type: "decimal(10,2)" } }
        effect:
          - assign: { location: d2, expr: inputs.p }
      times:
        kind: internal
        from: OPEN
        inputs: { k: { type: int } }
        effect:
          - assign: { location: n, expr: inputs.k * d2 }
      text:
        kind: internal
        from: OPEN
        inputs: { s: { type: string } }
        effect:
          - assign: { location: n, expr: inputs.s }
      tag:
        kind: internal
        from: OPEN
        effect:
          - add: { location: tags, expr: label }
      narrow:
        kind: internal
        from: OPEN
        effect:
          - assign: { location: d2, expr: d4 }
      nudge:
        kind: internal
        from: OPEN
        effect:
          - assign: { location: d2, expr: d2 + 0.005 }
      spawn:
        kind: internal
        from: OPEN
        effect:
          - create: { type: Item, transition: open, inputs: { d2: 7 } }
      grow:
        kind: internal
        from: OPEN
        inputs: { h: { type: int } }
        effect:
          - assign: { location: huge, expr: inputs.h }
      poke: { kind: internal, from: OPEN, guards: { decided: deny } }
      sweep:
        kind: internal
        from: OPEN
        effect:
          - foreach:
              item: x
              array: "{1, 2, -3}"
              where: "x * 4611686018427387904 > 0"
              limit: 5
              steps:
                - assign: { location: n, expr: x }
"#;

    /// A value is written as its attribute's type holds it, by a default, an
    /// effect and a creation's argument alike: an integer to a decimal at its
    /// scale, where `5` was stored as `0.05`; a decimal to an integer only
    /// where it is whole, where `10.50` was stored as `1050`; a value of
    /// another kind, or an absent element added to a set, refused
    /// `value_in_range`; and a quotient written to a wider decimal divided
    /// once at its scale, where it was rounded at its dividend's first (the
    /// audit's values W1 to W4 and W6, ADR-0205 decision 1).
    #[test]
    fn a_value_is_written_as_its_type_holds_it() {
        use crate::value::Decimal;
        let lib = Library::load(&[("vals.yaml".into(), VALS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let dec = |t: &str| Value::Decimal(Decimal::parse(t).unwrap());
        let held =
            |s: &Store, id: &str, a: &str| s.snapshot().unwrap().get(id).unwrap().unwrap().attr(a);
        let out = |v: &Verdict| matches!(v, Verdict::Unsatisfied { clause, remedy: Some(Remedy::UnreachableFromHere) } if clause == "value_in_range");
        let item = created(&req(&mut s, "open", OPERATOR, ("Item", true), json!({})));
        assert_eq!(held(&s, &item, "d2"), dec("5.00"));
        created(&req(&mut s, "third", OPERATOR, (&item, false), json!({})));
        assert_eq!(held(&s, &item, "d4"), dec("1.6667"));
        created(&req(
            &mut s,
            "count",
            OPERATOR,
            (&item, false),
            json!({"k": 7}),
        ));
        assert_eq!(held(&s, &item, "d2"), dec("7.00"));
        created(&req(
            &mut s,
            "times",
            OPERATOR,
            (&item, false),
            json!({"k": 3}),
        ));
        assert_eq!(held(&s, &item, "n"), Value::Int(21));
        created(&req(
            &mut s,
            "price",
            OPERATOR,
            (&item, false),
            json!({"p": "1.50"}),
        ));
        assert!(out(&req(
            &mut s,
            "times",
            OPERATOR,
            (&item, false),
            json!({"k": 7})
        )));
        assert!(out(&req(
            &mut s,
            "text",
            OPERATOR,
            (&item, false),
            json!({"s": "3"})
        )));
        assert_eq!(held(&s, &item, "n"), Value::Int(21));
        assert!(out(&req(
            &mut s,
            "tag",
            OPERATOR,
            (&item, false),
            json!({})
        )));
        // a plain write that would round its decimal is refused, as §8.3
        // permits rounding a product or a quotient alone (W5)
        assert!(out(&req(
            &mut s,
            "narrow",
            OPERATOR,
            (&item, false),
            json!({})
        )));
        assert!(out(&req(
            &mut s,
            "nudge",
            OPERATOR,
            (&item, false),
            json!({})
        )));
        assert_eq!(held(&s, &item, "d2"), dec("1.50"));
        created(&req(&mut s, "spawn", OPERATOR, (&item, false), json!({})));
        let w = s.snapshot().unwrap();
        let spawned: Vec<Value> = w
            .of_type("Item")
            .unwrap()
            .iter()
            .filter(|o| o.id != item)
            .map(|o| o.attr("d2"))
            .collect();
        assert_eq!(spawned, vec![dec("7.00")]);
    }

    /// Arithmetic past its bound is refused wherever it is read, never
    /// unknown: an invariant refuses its request, where it held and admitted
    /// the write; a loop's filter refuses it, where it dropped the element;
    /// and a read of a derived value or by a filter refuses the read, where
    /// it answered null and left the object out (the audit's values U1 to
    /// U4, ADR-0205 decision 2).
    #[test]
    fn arithmetic_past_its_bound_is_refused_where_it_is_read() {
        let lib = Library::load(&[("vals.yaml".into(), VALS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let out = |v: &Verdict| matches!(v, Verdict::Unsatisfied { clause, .. } if clause == "value_in_range");
        assert!(out(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Item", true),
            json!({"big": 1i64 << 62})
        )));
        assert!(matches!(
            req(&mut s, "open", OPERATOR, ("Item", true), json!({"big": 60})),
            Verdict::InvariantViolated { .. }
        ));
        let item = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Item", true),
            json!({"big": 10}),
        ));
        assert!(out(&req(
            &mut s,
            "sweep",
            OPERATOR,
            (&item, false),
            json!({})
        )));
        let w = s.snapshot().unwrap();
        assert_eq!(w.get(&item).unwrap().unwrap().attr("n"), Value::Absent);
        assert!(crate::reads::get(&w, &item).is_ok());
        created(&req(
            &mut s,
            "grow",
            OPERATOR,
            (&item, false),
            json!({"h": i64::MAX}),
        ));
        let w = s.snapshot().unwrap();
        let refused = Err(NotRun::OutOfRange(item.clone()));
        assert_eq!(crate::reads::get(&w, &item).map(|_| ()), refused);
        let q = |f: &str| crate::reads::query(&w, "Item", Some(f), &[], None).map(|_| ());
        assert_eq!(q("huge + 1 > 0"), refused);
        assert_eq!(q("not (huge + 1 > 0)"), refused);
        assert_eq!(q("huge is not null"), Ok(()));
        // the side of an `or` that does not decide refuses nothing, though a
        // guard reads it for the record
        created(&req(&mut s, "poke", OPERATOR, (&item, false), json!({})));
    }

    #[test]
    fn an_effects_arithmetic_past_its_bound_is_refused() {
        let lib = Library::load(&[("tally.yaml".into(), TALLY.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let told = |s: &mut Store, xn: &str, id: &str, inputs: Json| {
            let mut r = Request {
                transition: xn.into(),
                actor: OPERATOR.into(),
                object_id: Some(id.into()),
                type_: None,
                inputs: Default::default(),
                occurred_at: None,
                propose: false,
                expires_at: None,
                expected_version: None,
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            };
            r.inputs = inputs
                .as_object()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            request_told(s, &r).unwrap().unwrap()
        };
        let big = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Tally", true),
            json!({"n": i64::MAX - 5}),
        ));
        let least = created(&req(
            &mut s,
            "open",
            OPERATOR,
            ("Tally", true),
            json!({"n": i64::MIN}),
        ));
        for (xn, id, inputs) in [
            ("add", &big, json!({"by": 6})),
            ("negate", &least, json!({})),
            ("halve", &big, json!({"by": 0})),
        ] {
            let (v, t) = told(&mut s, xn, id, inputs);
            assert!(
                matches!(&v, Verdict::Unsatisfied { clause, remedy: Some(Remedy::UnreachableFromHere) } if clause == "value_in_range"),
                "{xn}: {v:?}"
            );
            assert_eq!(t.objects, vec![id.clone()], "{xn}");
        }
        let w = s.snapshot().unwrap();
        assert_eq!(
            w.get(&big).unwrap().unwrap().attrs.get("n"),
            Some(&crate::value::Value::Int(i64::MAX - 5))
        );
        // a guard whose arithmetic passes its bound refuses its request
        // so too, where it was unknown (ADR-0205 decision 2)
        let (v, t) = told(&mut s, "check", &big, json!({}));
        assert!(
            matches!(&v, Verdict::Unsatisfied { clause, .. } if clause == "value_in_range"),
            "{v:?}"
        );
        assert_eq!(t.objects, vec![big.clone()]);
        let (v, _) = told(&mut s, "add", &big, json!({"by": 5}));
        assert!(matches!(v, Verdict::Satisfied { .. }), "{v:?}");
    }
}

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
use crate::model::{Declared, Enforce, From, Kind, Library, Remedy, Transition, Type, Unique};
use crate::store::{Allocator, Change, Event};
use crate::value::Value;
use crate::verdict::{Flag, InvariantFailure, Verdict};
use crate::wire;
use crate::world::{Object, World};

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

/// What the request answers: its verdict, and what it writes if it applies.
pub fn decide(
    world: &World<'_>,
    alloc: &mut Allocator,
    req: &Request,
) -> Result<(Verdict, Option<Change>), NotRun> {
    let lib = world.lib;
    let refuse = |v: Verdict| Ok((v, None));
    // 1. the actor
    let Some(who) = resolve_actor(world, &req.actor) else {
        return refuse(Verdict::Unsatisfied {
            clause: "actor_known".into(),
            remedy: Some(Remedy::Dependent),
        });
    };
    // 2. the object and the transition
    let (tn, oid) = match &req.object_id {
        Some(id) => match world.get(id) {
            None => return refuse(Verdict::NotFound),
            Some(o) => (o.type_.clone(), Some(id.as_str())),
        },
        None => match &req.type_ {
            Some(t) => (t.clone(), None),
            None => {
                return refuse(Verdict::UnknownTransition {
                    transition: req.transition.clone(),
                });
            }
        },
    };
    if lib.observation_kinds.contains(&tn) {
        return not_run(format!("recording an observation of {tn}"));
    }
    let Some(t) = lib.type_(&tn) else {
        return refuse(Verdict::UnknownTransition {
            transition: req.transition.clone(),
        });
    };
    if let Some(what) = t.beyond.first() {
        return not_run(what.clone());
    }
    let x = match t.transition(&req.transition) {
        Some(x) if (x.kind == Kind::Initial) == oid.is_none() => x,
        _ => {
            return refuse(Verdict::UnknownTransition {
                transition: req.transition.clone(),
            });
        }
    };
    if matches!(x.kind, Kind::Assertion | Kind::Erasure) {
        return not_run(format!(
            "an {} ({tn}.{})",
            if x.kind == Kind::Assertion {
                "assertion"
            } else {
                "erasure"
            },
            req.transition
        ));
    }
    // 3. not requestable, and unavailable
    if !x.only_via.is_empty() {
        return refuse(Verdict::NotRequestable {
            parents: x.only_via.clone(),
        });
    }
    if let Some(o) = oid.and_then(|id| world.get(id))
        && !takes_from(t, x, &o.state)
    {
        return refuse(Verdict::Unavailable {
            state: o.state.clone(),
        });
    }
    // 4. the inputs
    let taken: BTreeSet<&str> = x
        .required_inputs
        .iter()
        .chain(&x.optional_inputs)
        .map(String::as_str)
        .chain(x.inputs.iter().map(|(n, _)| n.as_str()))
        .collect();
    let invalid = |names: Vec<String>| Ok((Verdict::InvalidInput { inputs: names }, None));
    let unknown_names: Vec<String> = req
        .inputs
        .keys()
        .filter(|k| !taken.contains(k.as_str()))
        .cloned()
        .collect();
    if !unknown_names.is_empty() {
        return invalid(unknown_names);
    }
    let mut inputs: BTreeMap<String, Value> = BTreeMap::new();
    let mut wrong = Vec::new();
    for (k, raw) in &req.inputs {
        let (declared, many) = match x.inputs.iter().find(|(n, _)| n == k) {
            Some((_, spec)) => (&spec.declared, spec.many),
            None => match t.attribute(k) {
                Some(a) => (&a.declared, a.many),
                None => return not_run(format!("the input {k}, which names no attribute of {tn}")),
            },
        };
        let Ok(v) = wire::decode(raw, declared, many, lib) else {
            wrong.push(k.clone());
            continue;
        };
        if let Declared::Reference(target) = declared {
            let ids: Vec<&Value> = match &v {
                Value::Set(items) => items.iter().collect(),
                Value::Absent => Vec::new(),
                one => vec![one],
            };
            let fits = ids.iter().all(|i| match i {
                Value::Object(id) => world.get(id).is_some_and(|o| &o.type_ == target),
                _ => false,
            });
            if !fits {
                wrong.push(k.clone());
                continue;
            }
        }
        if !v.is_absent() {
            inputs.insert(k.clone(), v);
        }
    }
    if !wrong.is_empty() {
        return invalid(wrong);
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
        return invalid(left_out);
    }
    let mut missing = Vec::new();
    for (k, spec) in &x.inputs {
        if inputs.contains_key(k) {
            continue;
        }
        if let Some(d) = &spec.default {
            let v = Evaluator::new(world, Some(t), oid)
                .with_inputs(&inputs)
                .with_actor(&who)
                .eval(d)?;
            inputs.insert(k.clone(), v);
        } else if !spec.optional {
            missing.push(k.clone());
        }
    }
    if !missing.is_empty() {
        return invalid(missing);
    }
    // 5. the generated guards, then the transition's own
    if req.occurred_at.is_some() {
        return not_run("a request's occurred time, which the first slice does not date");
    }
    for k in &x.required_inputs {
        if !inputs.contains_key(k) {
            return refuse(Verdict::Unsatisfied {
                clause: format!("{k}_provided"),
                remedy: Some(Remedy::SelfServiceable),
            });
        }
    }
    let mut flags = Vec::new();
    for (g, mode) in &x.guards {
        let Some(c) = t.condition(g) else {
            return not_run(format!("the guard {g}, which {tn} does not declare"));
        };
        let ok = truth(
            &Evaluator::new(world, Some(t), oid)
                .with_inputs(&inputs)
                .with_actor(&who)
                .eval(&c.expression)?,
        )?;
        if ok != Some(true) {
            match mode {
                Enforce::Deny => {
                    return refuse(Verdict::Unsatisfied {
                        clause: g.clone(),
                        remedy: c.remedy,
                    });
                }
                Enforce::Warn => flags.push(Flag {
                    clause: g.clone(),
                    remedy: c.remedy,
                }),
                Enforce::Audit => {}
            }
        }
    }
    // 6. the outcome
    if x.has_effect {
        return not_run(format!("an effect ({tn}.{})", req.transition));
    }
    if let Some(what) = t.at_outcome.first() {
        return not_run(what.clone());
    }
    let mut after = world.clone();
    let position = alloc.position();
    alloc.txn();
    let (mut o, created, from_state) = match oid.and_then(|id| world.get(id)) {
        Some(o) => (o.clone(), false, Some(o.state.clone())),
        None => {
            let Some(to) = x.to.first() else {
                return not_run(format!(
                    "an initial transition with no target ({tn}.{})",
                    req.transition
                ));
            };
            let id = alloc.object_id(&tn);
            let mut o = Object {
                id: id.clone(),
                type_: tn.clone(),
                state: to.clone(),
                version: 0,
                attrs: BTreeMap::new(),
                created_at: world.now,
                created_by_kind: who.kind.clone(),
                order: world.next_order(),
            };
            for (a, spec) in &t.attributes {
                if spec.many {
                    o.attrs.insert(a.clone(), Value::Set(Vec::new()));
                } else if matches!(&spec.declared, Declared::Scalar(s) if s == "counter") {
                    o.attrs.insert(a.clone(), Value::Int(0));
                }
            }
            after.objects.insert(id.clone(), o.clone());
            for (a, spec) in &t.attributes {
                if let Some(d) = &spec.default {
                    let v = Evaluator::new(&after, Some(t), Some(&id))
                        .with_inputs(&inputs)
                        .with_actor(&who)
                        .eval(d)?;
                    if !v.is_absent() {
                        o.attrs.insert(a.clone(), v);
                    }
                }
            }
            (o, true, None)
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
    if created {
        mint(&mut after, alloc, t, &mut o)?;
    }
    o.version += 1;
    after.objects.insert(o.id.clone(), o.clone());
    // 7. the invariants, on every object, in ascending id order
    let requested = o.id.clone();
    for obj in after.in_order() {
        let failing = failing_invariants(&after, obj)?;
        if failing.is_empty() {
            continue;
        }
        let failures: Vec<InvariantFailure> = failing
            .iter()
            .map(|n| InvariantFailure {
                object_id: obj.id.clone(),
                type_: obj.type_.clone(),
                invariant: n.clone(),
                remedy: invariant_remedy(&after, x, obj, n, &requested),
            })
            .collect();
        let remedy = failures.first().and_then(|f| f.remedy);
        return refuse(Verdict::InvariantViolated { failures, remedy });
    }
    let event = Event {
        position,
        object_id: o.id.clone(),
        object_seq: o.version,
        transition: req.transition.clone(),
        from_state: from_state.clone(),
        to_state: o.state.clone(),
        changes_state: from_state.as_deref() != Some(o.state.as_str()),
        source: "observed".into(),
        actor_id: who.id.clone(),
        actor_kind: who.kind.clone(),
        declaration_version: 1,
        recorded_at: world.now,
        occurred_at: Some(world.now),
        imported: false,
        payload: json!({ "inputs": written }),
    };
    let verdict = Verdict::Satisfied {
        object_id: o.id.clone(),
        version: o.version,
        events: vec![position],
        flags,
    };
    Ok((
        verdict,
        Some(Change {
            writes: vec![(o, created, event)],
        }),
    ))
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
    let writes: BTreeSet<&String> = x.required_inputs.iter().chain(&x.optional_inputs).collect();
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
    store.transact(|world, alloc| decide(world, alloc, req))
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
      archive: { kind: external, from: [DONE], to: DONE, only_via: [finish] }
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
    fn what_the_slice_does_not_run_is_reached_late() {
        let text = JOBS.replace(
            "      finish: { kind: external, from: [WORKING], to: DONE }",
            "      finish: { kind: external, from: [WORKING], to: DONE, effect: [ { assign: { location: note, expr: '\"done\"' } } ] }",
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
        // a refusal before the effect is still decided
        assert_eq!(
            req(&mut s, "finish", OPERATOR, (&job, false), json!({})),
            Verdict::Unavailable {
                state: "OPEN".into()
            }
        );
        // and an applying request reaches the effect, which is not run
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
        let r = Request {
            transition: "finish".into(),
            actor: OPERATOR.into(),
            object_id: Some(job.clone()),
            type_: None,
            inputs: BTreeMap::new(),
            occurred_at: None,
        };
        let not = request(&mut s, &r).unwrap().unwrap_err();
        assert!(not.0.contains("an effect (Job.finish)"), "{not}");
        assert_eq!(s.snapshot().unwrap().get(&job).unwrap().state, "WORKING");
    }
}

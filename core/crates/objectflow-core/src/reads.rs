//! The reads the first slice offers (`DESIGN.md` §10): an object as `get`
//! returns it, and what it offers its actor, `availability`, decided as
//! ADR-0152 states: the actor, the state and each guard that reads no input,
//! in the order of refusals; what reads an input waits for it; no evaluator
//! asked; neither the effect nor the invariants run.

use std::collections::{BTreeMap, BTreeSet};

use crate::eval::{Evaluator, NotRun, truth};
use crate::expr::Expr;
use crate::model::{Declared, Enforce, Kind, Remedy, Transition, Type};
use crate::request::{names_read, resolve_actor, takes_from};
use crate::value::Value;
use crate::verdict::{Flag, Verdict};
use crate::world::{Object, World};

/// An object as `get` returns it: its row, its derived attributes evaluated,
/// and whether it is open.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub object: Object,
    pub derived: Vec<(String, Value)>,
    pub open: bool,
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
        derived.push((n.clone(), crate::metric::typed_derived(world, t, e, v)));
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
        let refused = world
            .attempts()?
            .iter()
            .rev()
            .find(|a| a.proposal.as_deref() == Some(id));
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
}

/// `exceptions(type)` (`DESIGN.md` §10): the objects of the type or its
/// family whose state an assertion last set, the import's excepted, and
/// those holding an admitted invariant violation, in id order, each with
/// its state; a type the store does not declare is refused, a value.
pub fn exceptions(world: &World<'_>, type_: &str) -> Result<QueryPage, NotRun> {
    if world.lib.type_(type_).is_none() {
        return Ok(QueryPage {
            items: Vec::new(),
            refused: Some("unknown_type".into()),
        });
    }
    let items = world.excepted(type_)?.into_iter().map(state_item).collect();
    Ok(QueryPage {
        items,
        refused: None,
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
    let refuse = |why: &str| {
        Ok(QueryPage {
            items: Vec::new(),
            refused: Some(why.to_string()),
        })
    };
    let Some(t) = world.lib.type_(type_) else {
        return refuse("unknown_type");
    };
    let Some(x) = t.transition(transition).filter(|x| x.kind != Kind::Initial) else {
        return refuse("unknown_transition");
    };
    if !x.only_via.is_empty() {
        return refuse("not_requestable");
    }
    let mut items = Vec::new();
    for o in world.of_type(type_)? {
        let offers = availability(world, actor, &o.id)?.unwrap_or_default();
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
    Ok(QueryPage {
        items,
        refused: None,
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
pub fn query(
    world: &World<'_>,
    type_: &str,
    filter: Option<&str>,
    order: &[String],
    fields: Option<&[String]>,
) -> Result<QueryPage, NotRun> {
    let refuse = |why: &str| {
        Ok(QueryPage {
            items: Vec::new(),
            refused: Some(why.to_string()),
        })
    };
    let Some(t) = world.lib.type_(type_) else {
        return refuse("unknown_type");
    };
    let filter = match filter.map(crate::expr::parse) {
        None => None,
        Some(Ok(e)) => Some(e),
        Some(Err(_)) => return refuse("malformed_filter"),
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
    let candidates = match filter
        .as_ref()
        .and_then(|f| required_states(world.lib, type_, f))
    {
        Some(states) => world.of_type_in(type_, &states)?,
        None => world.of_type(type_)?,
    };
    let mut chosen = Vec::new();
    for o in candidates {
        let Some(ot) = world.lib.type_(&o.type_) else {
            return Err(NotRun::Unsupported(format!("the type {}", o.type_)));
        };
        let held = match &filter {
            None => true,
            Some(e) => truth(&Evaluator::new(world, Some(ot), Some(&o.id)).eval(e)?)? == Some(true),
        };
        if held {
            chosen.push(o);
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
    if !members.is_empty() {
        world.with_parts(&chosen)?;
    }
    for m in &members {
        for o in &chosen {
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
    for o in &chosen {
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
    chosen.sort_by(|a, b| {
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
    });
    let mut items = Vec::new();
    for o in chosen {
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
            out.push((f.clone(), reader.eval(&member)?));
        }
        items.push(Item {
            id: o.id.clone(),
            type_: o.type_.clone(),
            fields: out,
        });
    }
    Ok(QueryPage {
        items,
        refused: None,
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
    let Some(o) = world.get(id)? else {
        return Ok(None);
    };
    let Some(t) = world.lib.type_(&o.type_) else {
        return Err(NotRun::Unsupported(format!("the type {}", o.type_)));
    };
    if let Some(what) = t.beyond.first() {
        return Err(NotRun::Unsupported(what.clone()));
    }
    let who = resolve_actor(world, actor)?;
    let no_inputs = BTreeMap::new();
    let mut offers = Vec::new();
    for (xn, x) in &t.transitions {
        if x.kind == Kind::Initial || !x.only_via.is_empty() {
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
        let refused = |v: Verdict, inputs: Vec<(String, String)>, unevaluated: Vec<String>| Offer {
            transition: xn.clone(),
            creates: None,
            availability: Availability::Unavailable,
            verdict: Some(v),
            inputs,
            unevaluated,
            flags: Vec::new(),
        };
        let Some(who) = &who else {
            offers.push(refused(
                Verdict::Unsatisfied {
                    clause: "actor_known".into(),
                    remedy: Some(Remedy::Dependent),
                },
                inputs,
                unevaluated,
            ));
            continue;
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
            let asking = Evaluator::new(world, Some(t), Some(id))
                .with_inputs(&no_inputs)
                .with_actor(who);
            let ok = truth(&asking.eval(&c.expression)?)?;
            // an evaluator is never asked by an offer (ADR-0048)
            if asking.unanswered.get() {
                unevaluated.push(g.clone());
                continue;
            }
            if ok != Some(true) {
                match mode {
                    Enforce::Deny => {
                        denied = Some(Verdict::Unsatisfied {
                            clause: g.clone(),
                            remedy: c.remedy,
                        });
                        break;
                    }
                    Enforce::Warn => flags.push(Flag {
                        clause: g.clone(),
                        remedy: c.remedy,
                    }),
                    Enforce::Audit => {}
                }
            }
        }
        if let Some(v) = denied {
            offers.push(refused(v, inputs, unevaluated));
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
        });
    }
    // and recording each kind on it, and labelling, which every type
    // carries, its generated guards decided on the subject and its fields
    // waiting for the caller (ADR-0099)
    let label = crate::model::LABEL.to_string();
    for kind in t.observations.iter().map(|(_, k)| k).chain([&label]) {
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
        let refusal = if who.is_none() {
            Some(Verdict::Unsatisfied {
                clause: "actor_known".into(),
                remedy: Some(Remedy::Dependent),
            })
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
            rusqlite::params![big, at],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO of_admission (object_id, invariant, position, reason, discharged_position) \
             VALUES (?1, 'bounded', ?2, 'test', ?2)",
            rusqlite::params![below, at],
        )
        .unwrap();
        let plan: String = conn
            .query_row(
                "EXPLAIN QUERY PLAN SELECT id FROM t_job WHERE state_source = 'asserted'",
                [],
                |r| r.get(3),
            )
            .unwrap();
        assert!(plan.contains("t_job_asserted_ix"), "{plan}");
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
}

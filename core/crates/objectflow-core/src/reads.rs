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
    let Some(o) = world.get(id) else {
        return Ok(None);
    };
    let Some(t) = world.lib.type_(&o.type_) else {
        return Err(NotRun(format!("the type {}", o.type_)));
    };
    let mut derived = Vec::new();
    for (n, e) in &t.derived {
        derived.push((n.clone(), Evaluator::new(world, Some(t), Some(id)).eval(e)?));
    }
    // a subscription carries its lag, and whether it is lagging or dead,
    // derived from its acknowledged cursor (DESIGN.md §7)
    if o.type_ == crate::model::SUBSCRIPTION {
        let (lag, lagging, dead) = crate::subscription::lag(world, o);
        derived.push(("lag".into(), Value::Duration(lag)));
        derived.push(("lagging".into(), Value::Bool(lagging)));
        derived.push(("dead".into(), Value::Bool(dead)));
    }
    // a proposal carries the verdict its latest refused approval left it
    // with, read from that approval's attempt row and never stored, since
    // the refusal records no event on it (DESIGN.md §9, ADR-0157)
    if o.type_ == crate::model::PROPOSAL {
        let refused = world
            .attempts
            .iter()
            .rev()
            .find(|a| a.request_values["proposal"].as_str() == Some(id));
        derived.push((
            "last_verdict".into(),
            refused.map_or(Value::Absent, |a| {
                let text = |t: &Option<String>| t.clone().map_or(Value::Absent, Value::Text);
                Value::Row(std::collections::BTreeMap::from([
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
    let mut chosen = Vec::new();
    for o in world.of_type(type_) {
        let Some(ot) = world.lib.type_(&o.type_) else {
            return Err(NotRun(format!("the type {}", o.type_)));
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
    // a key's value for an object, and how two values of one key order
    let value = |o: &Object, key: &Key| -> Value {
        match key {
            Key::CreatedAt => Value::Timestamp(o.created_at),
            Key::EnteredAt(m) => o
                .spans
                .get(m)
                .and_then(|s| s.last())
                .filter(|sp| sp.left_position.is_none())
                .map_or(Value::Absent, |sp| Value::Timestamp(sp.entered_at)),
            Key::Attribute(a) => o.attr(a),
        }
    };
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
                Value::Object(id) => world.get(id).map(|o| o.order),
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
            return Err(NotRun(format!("the type {}", o.type_)));
        };
        let reader = Evaluator::new(world, Some(ot), Some(&o.id));
        let mut out = Vec::new();
        for f in &asked {
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

/// What each requestable transition of an object offers its actor now.
/// `None` where no object has the id.
pub fn availability(
    world: &World<'_>,
    actor: &str,
    id: &str,
) -> Result<Option<Vec<Offer>>, NotRun> {
    let Some(o) = world.get(id) else {
        return Ok(None);
    };
    let Some(t) = world.lib.type_(&o.type_) else {
        return Err(NotRun(format!("the type {}", o.type_)));
    };
    if let Some(what) = t.beyond.first() {
        return Err(NotRun(what.clone()));
    }
    let who = resolve_actor(world, actor);
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
        if x.kind != Kind::Erasure && o.erased {
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
                return Err(NotRun(format!(
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
            return Err(NotRun(format!("the observation kind {kind}")));
        };
        let Some(x) = k.transition("record") else {
            return Err(NotRun(format!("{kind}, which has no record")));
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

    #[test]
    fn a_get_evaluates_what_the_type_derives() {
        let (s, job) = setup();
        let w = s.snapshot().unwrap();
        let v = get(&w, &job).unwrap().unwrap();
        assert_eq!(v.derived, vec![("long".to_string(), Value::Bool(false))]);
        assert!(v.open);
        assert!(get(&w, "job-99").unwrap().is_none());
    }
}

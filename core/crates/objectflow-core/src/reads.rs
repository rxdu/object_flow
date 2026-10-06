//! The reads the first slice offers (`DESIGN.md` §10): an object as `get`
//! returns it, and what it offers its actor, `availability`, decided as
//! ADR-0152 states: the actor, the state and each guard that reads no input,
//! in the order of refusals; what reads an input waits for it; no evaluator
//! asked; neither the effect nor the invariants run.

use std::collections::{BTreeMap, BTreeSet};

use crate::eval::{Evaluator, NotRun, truth};
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

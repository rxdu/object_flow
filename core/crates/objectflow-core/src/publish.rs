//! A publish's dry run: a change's source loaded as the next version,
//! compared with the installed one, and every object read into the impact
//! report a person approves (`publish-and-import.md` §1 steps 2 to 4, §2,
//! ADR-0155). Step 1, the checks and the examples, is a sandbox caller's
//! (ADR-0155 decision 1).

use std::collections::BTreeMap;

use crate::eval::{Evaluator, NotRun, truth};
use crate::model::{Declared, Enforce, Kind, Library, PROPOSAL, Type};
use crate::reads::{self, Availability};
use crate::request::{failing_invariants, names_read, takes_from};
use crate::value::Value;
use crate::world::{Object, World};

/// How much a finding matters (`publish-and-import.md` §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The publish is refused.
    Fatal,
    /// A person must choose before it can proceed.
    Decision,
    /// Reported, and it proceeds.
    Notice,
}

impl Severity {
    pub fn text(self) -> &'static str {
        match self {
            Severity::Fatal => "fatal",
            Severity::Decision => "decision",
            Severity::Notice => "notice",
        }
    }
}

/// One thing a publish has to say, with the objects it names.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub severity: Severity,
    /// The impact's category, which names what it is about.
    pub code: String,
    pub message: String,
    pub affected: Vec<String>,
}

/// What a dry run found (`publish-and-import.md` §2's `PublishReport`, the
/// parts the first slice computes).
#[derive(Clone, Debug, PartialEq)]
pub struct PublishReport {
    pub version: i64,
    pub accepted: bool,
    pub now: i64,
    pub findings: Vec<Finding>,
    pub change_id: Option<String>,
    pub source_ref: Option<String>,
    pub source_digest: Option<String>,
    pub proposals_invalidated: usize,
}

impl PublishReport {
    /// The ids each impact names, per category (ADR-0096).
    pub fn affected(&self) -> BTreeMap<String, Vec<String>> {
        self.findings
            .iter()
            .filter(|f| !f.affected.is_empty())
            .map(|f| (f.code.clone(), f.affected.clone()))
            .collect()
    }

    pub fn fatal(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Fatal)
    }
}

/// What a dry run reads its change from.
#[derive(Debug)]
pub struct Draft<'a> {
    pub change_id: Option<&'a str>,
    pub source: &'a [(String, String)],
    pub source_ref: Option<&'a str>,
    pub source_digest: Option<&'a str>,
    /// Whose offers are compared, the actor asking.
    pub actor: &'a str,
}

/// Findings gathered by category, in the order first met.
#[derive(Debug, Default)]
struct Found(Vec<Finding>);

impl Found {
    fn add(
        &mut self,
        severity: Severity,
        code: String,
        message: impl Into<String>,
        id: Option<&str>,
    ) {
        let message = message.into();
        let f = match self.0.iter_mut().find(|f| f.code == code) {
            Some(f) => f,
            None => {
                self.0.push(Finding {
                    severity,
                    code,
                    message,
                    affected: Vec::new(),
                });
                let last = self.0.len() - 1;
                &mut self.0[last]
            }
        };
        if let Some(id) = id
            && !f.affected.iter().any(|a| a == id)
        {
            f.affected.push(id.to_string());
        }
    }
}

/// A report as the wire and a change's `report` carry it: the parts the
/// first slice computes, and the rest named as not computed (ADR-0155).
pub fn report_json(r: &PublishReport) -> serde_json::Value {
    serde_json::json!({
        "version": r.version,
        "accepted": r.accepted,
        "now": crate::value::timestamp_text(r.now),
        "findings": r.findings.iter().map(|f| serde_json::json!({
            "severity": f.severity.text(), "code": f.code, "message": f.message,
            "affected": f.affected.len(), "sample": f.affected.iter().take(5).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "change_id": r.change_id,
        "source_ref": r.source_ref,
        "source_digest": r.source_digest,
        "proposals_invalidated": r.proposals_invalidated,
        "affected": r.affected(),
        "not_computed": [
            "invariants", "sweepable", "unsweepable", "queryable_derives", "worst_case_fanout",
            "observed_fanout", "replaced_creations", "creations_past_first_state", "observing_clauses",
            "flags", "person_dimensions", "examples", "analysed",
        ],
    })
}

/// The dry run over a sandbox's objects.
pub fn dry_run(world: &World<'_>, draft: &Draft<'_>) -> Result<PublishReport, NotRun> {
    let old = world.lib;
    let mut found = Found::default();
    let mut report = PublishReport {
        version: old.version + 1,
        accepted: false,
        now: world.now,
        findings: Vec::new(),
        change_id: draft.change_id.map(str::to_string),
        source_ref: draft.source_ref.map(str::to_string),
        source_digest: draft.source_digest.map(str::to_string),
        proposals_invalidated: 0,
    };
    let mut new = match Library::load(draft.source) {
        Ok(lib) => lib,
        Err(e) => {
            found.add(
                Severity::Fatal,
                "load".into(),
                format!("{}:{}: {}", e.file, e.line, e.message),
                None,
            );
            report.findings = found.0;
            return Ok(report);
        }
    };
    new.version = old.version + 1;
    let m = new.migration.clone();
    // the objects as the mappings leave them, read under the new version
    let mut mapped: BTreeMap<String, Object> = BTreeMap::new();
    for o in world.in_order() {
        if builtin(&o.type_) {
            // no change touches a built-in type, whose objects, the actors
            // among them, stand as they are
            mapped.insert(o.id.clone(), o.clone());
            continue;
        }
        let Some(nt) = new.type_(&o.type_) else {
            continue; // a retired type's objects are read for nothing
        };
        let mut x = o.clone();
        // renames first, which write nothing of their own
        for (tn, from, to) in &m.renamed_attributes {
            if new.is_a(&o.type_, tn)
                && let Some(v) = x.attrs.remove(from)
            {
                x.attrs.insert(to.clone(), v);
            }
        }
        // then removed enumeration members, a set keeping the target once
        for (en, from, to) in &m.removed_members {
            let mut changed = false;
            for (a, spec) in &nt.attributes {
                if !matches!(&spec.declared, Declared::Scalar(s) if s == en) {
                    continue;
                }
                let is = |v: &Value| matches!(v, Value::Member { enumeration, member } if enumeration == en && member == from);
                let target = Value::Member {
                    enumeration: en.clone(),
                    member: to.clone(),
                };
                match x.attrs.get(a).cloned() {
                    Some(Value::Set(items)) if items.iter().any(is) => {
                        let mut out: Vec<Value> = Vec::new();
                        for i in items {
                            let i = if is(&i) { target.clone() } else { i };
                            if !out.contains(&i) {
                                out.push(i);
                            }
                        }
                        x.attrs.insert(a.clone(), Value::Set(out));
                        changed = true;
                    }
                    Some(v) if is(&v) => {
                        x.attrs.insert(a.clone(), target.clone());
                        changed = true;
                    }
                    _ => {}
                }
            }
            if changed {
                found.add(
                    Severity::Notice,
                    format!("mapping:removed_members:{en}.{from}"),
                    format!("objects holding {en}.{from} take {en}.{to}"),
                    Some(&o.id),
                );
            }
        }
        // then removed states
        let old_type = old.type_(&o.type_);
        if old_type.is_some_and(|ot| ot.state(&o.state).is_some()) && nt.state(&o.state).is_none() {
            match m
                .removed_states
                .iter()
                .find(|(tn, s, _)| new.is_a(&o.type_, tn) && *s == o.state)
            {
                Some((_, _, to)) => {
                    found.add(
                        Severity::Notice,
                        format!("mapping:removed_states:{}.{}", o.type_, o.state),
                        format!("objects in {}.{} move to {to}", o.type_, o.state),
                        Some(&o.id),
                    );
                    x.state = to.clone();
                }
                None => found.add(
                    Severity::Fatal,
                    format!("removed_state:{}.{}", o.type_, o.state),
                    format!(
                        "objects are in {}.{}, which the version removes and no mapping moves",
                        o.type_, o.state
                    ),
                    Some(&o.id),
                ),
            }
        }
        mapped.insert(x.id.clone(), x);
    }
    let mut sim = World::new(&new, world.now);
    sim.external = world.external.clone();
    sim.objects = mapped;
    // then backfills, in the order written, each reading the object as the
    // mappings before it left it, an erased object's personal values skipped
    for (tn, attr, e) in &m.backfill {
        let ids: Vec<String> = sim.of_type(tn).iter().map(|o| o.id.clone()).collect();
        for id in ids {
            let Some(o) = sim.get(&id) else { continue };
            let Some(t) = new.type_(&o.type_) else {
                continue;
            };
            if o.erased && t.attribute(attr).is_some_and(|a| a.personal) {
                continue;
            }
            let v = Evaluator::new(&sim, Some(t), Some(&id)).eval(e)?;
            if o.attr(attr) != v {
                found.add(
                    Severity::Notice,
                    format!("mapping:backfill:{tn}.{attr}"),
                    format!("the backfill of {tn}.{attr} writes these objects"),
                    Some(&id),
                );
                if let Some(o) = sim.objects.get_mut(&id) {
                    if v.is_absent() {
                        o.attrs.remove(attr);
                    } else {
                        o.attrs.insert(attr.clone(), v);
                    }
                }
            }
        }
    }
    // what the mapped objects hold that the version cannot
    let removed_members: Vec<(String, String)> = old
        .enumerations
        .iter()
        .flat_map(|(en, members)| {
            members
                .iter()
                .filter(|mem| new.enumerations.get(en).is_none_or(|ms| !ms.contains(mem)))
                .map(|mem| (en.clone(), mem.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    for o in sim.in_order() {
        if builtin(&o.type_) {
            continue;
        }
        let Some(t) = new.type_(&o.type_) else {
            continue;
        };
        for (a, spec) in &t.attributes {
            let v = o.attr(a);
            let holds = |en: &str, mem: &str| {
                match &v {
                Value::Member { enumeration, member } => enumeration == en && member == mem,
                Value::Set(items) => items
                    .iter()
                    .any(|i| matches!(i, Value::Member { enumeration, member } if enumeration == en && member == mem)),
                _ => false,
            }
            };
            for (en, mem) in &removed_members {
                if holds(en, mem) {
                    found.add(
                        Severity::Fatal,
                        format!("removed_member:{en}.{mem}"),
                        format!("objects hold {en}.{mem}, which the version removes and no mapping replaces"),
                        Some(&o.id),
                    );
                }
            }
            // a required value, new or made required, left absent
            let counter = matches!(&spec.declared, Declared::Scalar(s) if s == "counter");
            if !spec.optional && spec.default.is_none() && !spec.many && !counter && v.is_absent() {
                found.add(
                    Severity::Fatal,
                    format!("required_absent:{}.{a}", o.type_),
                    format!(
                        "{}.{a} is required and these objects have none, nor a backfill to give it",
                        o.type_
                    ),
                    Some(&o.id),
                );
            }
        }
        // a state no transition or assertion of the version reaches, a
        // mirror's excepted, which the import writes
        if !t.mirror && t.state(&o.state).is_some() && !reachable(t, &o.state) {
            found.add(
                Severity::Fatal,
                format!("unreachable_state:{}.{}", o.type_, o.state),
                format!(
                    "objects are in {}.{}, which no transition or assertion of the version reaches",
                    o.type_, o.state
                ),
                Some(&o.id),
            );
        }
        // parts a mapping into a final state would strand
        let moved = world
            .get(&o.id)
            .is_some_and(|before| before.state != o.state);
        if moved && t.state(&o.state).is_some_and(|s| s.is_final) {
            for (a, spec) in &t.attributes {
                if !spec.composite || spec.survives_always {
                    continue;
                }
                let parts: Vec<String> = match o.attr(a) {
                    Value::Set(items) => items
                        .into_iter()
                        .filter_map(|i| {
                            if let Value::Object(p) = i {
                                Some(p)
                            } else {
                                None
                            }
                        })
                        .collect(),
                    Value::Object(p) => vec![p],
                    _ => opposite_parts(&sim, o, spec),
                };
                for p in parts {
                    let live = sim.get(&p).is_some_and(|po| {
                        new.type_(&po.type_)
                            .and_then(|pt| pt.state(&po.state))
                            .is_some_and(|s| !s.is_final)
                    });
                    if live {
                        found.add(
                            Severity::Fatal,
                            format!("stranded:{}.{a}", o.type_),
                            format!("a mapping moves these into a final state and would strand their live {a}"),
                            Some(&o.id),
                        );
                    }
                }
            }
        }
        // every invariant, once, after the mappings: a violation no standing
        // admission covers is a decision, which an admit may resolve
        for n in failing_invariants(&sim, o)? {
            if o.admissions.contains(&n) {
                continue;
            }
            let qualified = format!("{}.{n}", o.type_);
            let admitted = m.admit.iter().any(|(inv, _)| {
                inv.rsplit_once('.')
                    .is_some_and(|(tn, name)| name == n && new.is_a(&o.type_, tn))
            });
            found.add(
                Severity::Decision,
                format!("invariant:{qualified}"),
                if admitted {
                    format!("these objects would violate {qualified}, which the change admits")
                } else {
                    format!("these objects would violate {qualified}: fix them first, backfill, or admit it")
                },
                Some(&o.id),
            );
            if admitted && t.state(&o.state).is_some_and(|s| s.is_final) {
                found.add(
                    Severity::Notice,
                    format!("admit_final:{qualified}"),
                    "an admission on objects in a final state, which no transition can discharge",
                    Some(&o.id),
                );
            }
        }
    }
    clauses_and_offers(world, &sim, draft.actor, &mut found)?;
    report.proposals_invalidated = proposals(world, &new, &mut found);
    // every other open change, which the publish supersedes
    for c in world.of_type(crate::model::DECLARATION_CHANGE) {
        if Some(c.id.as_str()) != draft.change_id
            && (c.state == "DRAFTED" || c.state == "SUBMITTED")
        {
            found.add(
                Severity::Notice,
                "superseded".into(),
                "open changes the publish supersedes",
                Some(&c.id),
            );
        }
    }
    attribute_notices(old, &new, &mut found);
    report.findings = found.0;
    report.accepted = !report.fatal()
        && report.findings.iter().all(|f| {
            f.severity != Severity::Decision || {
                let inv = f.code.trim_start_matches("invariant:");
                m.admit.iter().any(|(a, _)| {
                    a == inv
                        || inv
                            .rsplit_once('.')
                            .zip(a.rsplit_once('.'))
                            .is_some_and(|((t, n), (at, an))| n == an && new.is_a(t, at))
                })
            }
        });
    Ok(report)
}

/// Whether a type is one of the store's own, which no change declares.
fn builtin(type_: &str) -> bool {
    [
        crate::model::OPERATOR_TYPE,
        PROPOSAL,
        crate::model::DECLARATION_CHANGE,
    ]
    .contains(&type_)
}

/// Whether a state is one some transition or assertion of the type enters.
fn reachable(t: &Type, state: &str) -> bool {
    t.transitions
        .iter()
        .any(|(_, x)| x.to.iter().any(|s| s == state))
}

/// A composite end its parts store, read from their side.
fn opposite_parts(world: &World<'_>, o: &Object, spec: &crate::model::Attribute) -> Vec<String> {
    let (Declared::Reference(target), Some(back)) = (&spec.declared, &spec.opposite) else {
        return Vec::new();
    };
    world
        .of_type(target)
        .iter()
        .filter(|p| p.attr(back) == Value::Object(o.id.clone()))
        .map(|p| p.id.clone())
        .collect()
}

/// The clauses the version newly enforces that would refuse an object, and
/// the transitions each object gains or loses, a transition whose verdict
/// waits on an input or an evaluator being neither (`publish-and-import.md`
/// §1 step 3).
fn clauses_and_offers(
    old: &World<'_>,
    new: &World<'_>,
    actor: &str,
    found: &mut Found,
) -> Result<(), NotRun> {
    for o in new.in_order() {
        if builtin(&o.type_) {
            continue;
        }
        let (Some(nt), Some(before)) = (new.lib.type_(&o.type_), old.get(&o.id)) else {
            continue;
        };
        let ot = old.lib.type_(&o.type_);
        for (xn, x) in &nt.transitions {
            if x.kind == Kind::Initial || !takes_from(nt, x, &o.state) {
                continue;
            }
            let was = ot.and_then(|t| t.transition(xn));
            for (g, mode) in &x.guards {
                if *mode != Enforce::Deny {
                    continue;
                }
                let enforced_before = was.is_some_and(|w| {
                    w.guards
                        .iter()
                        .any(|(wg, wm)| wg == g && *wm == Enforce::Deny)
                });
                if enforced_before {
                    continue;
                }
                let Some(c) = nt.condition(g) else { continue };
                let mut read = std::collections::BTreeSet::new();
                names_read(&c.expression, &mut read);
                if read.contains("inputs") {
                    continue; // decided when the inputs are given
                }
                let asking = Evaluator::new(new, Some(nt), Some(&o.id));
                let ok = truth(&asking.eval(&c.expression)?)?;
                if ok != Some(true) && !asking.unanswered.get() {
                    found.add(
                        Severity::Notice,
                        format!("newly_enforced:{}.{xn}.{g}", o.type_),
                        format!(
                            "{}.{xn} newly denies on {g}, which these objects fail",
                            o.type_
                        ),
                        Some(&o.id),
                    );
                }
            }
        }
        let class = |offers: &[reads::Offer], xn: &str| -> Option<Availability> {
            offers
                .iter()
                .find(|f| f.transition == xn && f.creates.is_none())
                .map(|f| {
                    if !f.unevaluated.is_empty() {
                        Availability::AvailableWithInput
                    } else {
                        f.availability
                    }
                })
        };
        let before_offers = reads::availability(old, actor, &before.id)?.unwrap_or_default();
        let after_offers = reads::availability(new, actor, &o.id)?.unwrap_or_default();
        let names: std::collections::BTreeSet<String> = before_offers
            .iter()
            .chain(&after_offers)
            .filter(|f| f.creates.is_none())
            .map(|f| f.transition.clone())
            .collect();
        for xn in names {
            let (b, a) = (class(&before_offers, &xn), class(&after_offers, &xn));
            let gained = a == Some(Availability::Available)
                && matches!(b, None | Some(Availability::Unavailable));
            let lost = b == Some(Availability::Available)
                && matches!(a, None | Some(Availability::Unavailable));
            if gained {
                found.add(
                    Severity::Notice,
                    format!("gained:{}.{xn}", o.type_),
                    format!("these objects gain {}.{xn}", o.type_),
                    Some(&o.id),
                );
            }
            if lost {
                found.add(
                    Severity::Notice,
                    format!("lost:{}.{xn}", o.type_),
                    format!("these objects lose {}.{xn}", o.type_),
                    Some(&o.id),
                );
            }
        }
    }
    Ok(())
}

/// The pending proposals the version invalidates: its transition removed or
/// no longer proposable, an input it no longer takes, or a value its type no
/// longer holds (`DESIGN.md` §9). Returns how many.
fn proposals(world: &World<'_>, new: &Library, found: &mut Found) -> usize {
    let mut n = 0;
    for p in world.of_type(PROPOSAL) {
        if p.state != "PENDING" {
            continue;
        }
        let text = |a: &str| match p.attr(a) {
            Value::Text(t) => t,
            _ => String::new(),
        };
        let x = new
            .type_(&text("target_type"))
            .and_then(|t| t.transition(&text("transition")).map(|x| (t, x)));
        let inputs: BTreeMap<String, serde_json::Value> =
            serde_json::from_str(&text("inputs")).unwrap_or_default();
        let breaks = match x {
            None => true,
            Some((t, x)) => {
                !x.proposable
                    || inputs.iter().any(|(k, v)| {
                        let declared = x
                            .inputs
                            .iter()
                            .find(|(n, _)| n == k)
                            .map(|(_, i)| (&i.declared, i.many))
                            .or_else(|| {
                                x.required_inputs
                                    .iter()
                                    .chain(&x.optional_inputs)
                                    .any(|n| n == k)
                                    .then(|| t.attribute(k).map(|a| (&a.declared, a.many)))
                                    .flatten()
                            });
                        match declared {
                            None => true,
                            Some((d, many)) => crate::wire::decode(v, d, many, new).is_err(),
                        }
                    })
            }
        };
        if breaks {
            n += 1;
            found.add(
                Severity::Notice,
                "proposals_invalidated".into(),
                "pending proposals the version invalidates",
                Some(&p.id),
            );
        }
    }
    n
}

/// Notices of the declaration alone: an attribute dropped where one of its
/// type is added, which may be a rename, and an attribute whose type
/// changed, which is a drop and an add of the same name.
fn attribute_notices(old: &Library, new: &Library, found: &mut Found) {
    for (tn, nt) in &new.types {
        let Some(ot) = old.type_(tn) else { continue };
        let renamed = |a: &str| {
            new.migration
                .renamed_attributes
                .iter()
                .any(|(t, from, _)| t == tn && from == a)
        };
        for (a, spec) in &ot.attributes {
            match nt.attribute(a) {
                Some(now) if now.declared != spec.declared || now.many != spec.many => found.add(
                    Severity::Notice,
                    format!("type_changed:{tn}.{a}"),
                    format!("{tn}.{a} changes its type, a drop and an add of the same name"),
                    None,
                ),
                None if !renamed(a)
                    && nt.attributes.iter().any(|(n, x)| {
                        ot.attribute(n).is_none()
                            && x.declared == spec.declared
                            && x.many == spec.many
                    }) =>
                {
                    found.add(
                        Severity::Notice,
                        format!("possible_rename:{tn}.{a}"),
                        format!("{tn}.{a} is dropped where an attribute of its type is added, which may be a rename"),
                        None,
                    );
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::model::{OPERATOR, Remedy};
    use crate::request::{Request, request};
    use crate::store::Store;
    use crate::verdict::Verdict;
    use serde_json::{Value as Json, json};

    const V1: &str = r#"
module: fleet
categories: [live, closed]
enumerations:
  Grade: [A, B, OLD]
  OverrideReason: [LEGACY]
types:
  Robot:
    attributes:
      serial: { type: string }
      grade: { type: Grade, optional: true }
    states:
      READY: { category: live }
      HOLD: { category: live }
      RETIRED: { category: closed, final: true }
    conditions:
      graded: { expression: grade is not null, remedy: self_serviceable }
    transitions:
      add: { kind: initial, to: READY, required_inputs: [serial], optional_inputs: [grade] }
      hold: { kind: external, from: [READY], to: HOLD, proposable: true }
      release: { kind: external, from: [HOLD], to: READY }
      retire: { kind: external, from: [READY], to: RETIRED, guards: { graded: audit } }
"#;

    /// The second version: HOLD and Grade.OLD removed, a required `site`, an
    /// invariant three robots meet differently, `graded` enforced, and no
    /// `hold`; each part of the migration switched on or off.
    fn v2(map_hold: bool, backfill: bool, admit: bool) -> String {
        let mut migration = String::from("migration:\n  removed_members:\n    Grade: { OLD: B }\n");
        if map_hold {
            migration.push_str("  removed_states:\n    Robot: { HOLD: READY }\n");
        }
        if backfill {
            migration.push_str("  backfill:\n    Robot: { site: '\"north\"' }\n");
        }
        if admit {
            migration.push_str("  admit:\n    - { invariant: Robot.graded_when_ready, reason: OverrideReason.LEGACY }\n");
        }
        format!(
            r#"
module: fleet
categories: [live, closed]
enumerations:
  Grade: [A, B]
  OverrideReason: [LEGACY]
types:
  Robot:
    attributes:
      serial: {{ type: string }}
      grade: {{ type: Grade, optional: true }}
      site: {{ type: string }}
    states:
      READY: {{ category: live }}
      RETIRED: {{ category: closed, final: true }}
    invariants:
      graded_when_ready: {{ expression: "state != Robot.READY or grade is not null" }}
    conditions:
      graded: {{ expression: grade is not null, remedy: self_serviceable }}
    transitions:
      add: {{ kind: initial, to: READY, required_inputs: [serial, site], optional_inputs: [grade] }}
      retire: {{ kind: external, from: [READY], to: RETIRED, guards: {{ graded: deny }} }}
{migration}"#
        )
    }

    fn req(s: &mut Store, xn: &str, target: (&str, bool), inputs: Json) -> Verdict {
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
            propose: false,
            expires_at: None,
        };
        request(s, &r).unwrap().unwrap()
    }

    fn created(v: &Verdict) -> String {
        match v {
            Verdict::Satisfied { object_id, .. } => object_id.clone(),
            other => panic!("{other:?}"),
        }
    }

    /// A sandbox at version 1 holding three robots, a pending proposal of
    /// `hold`, and another open change.
    fn fleet() -> (Store, [String; 3], String, String) {
        let lib = Library::load(&[("fleet.yaml".into(), V1.into())]).unwrap();
        let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
        let ready = created(&req(
            &mut s,
            "add",
            ("Robot", true),
            json!({"serial": "R1", "grade": "A"}),
        ));
        let held = created(&req(
            &mut s,
            "add",
            ("Robot", true),
            json!({"serial": "R2", "grade": "OLD"}),
        ));
        created(&req(&mut s, "hold", (&held, false), json!({})));
        let bare = created(&req(
            &mut s,
            "add",
            ("Robot", true),
            json!({"serial": "R3"}),
        ));
        let proposal = Request {
            transition: "hold".into(),
            actor: OPERATOR.into(),
            object_id: Some(ready.clone()),
            type_: None,
            inputs: Default::default(),
            occurred_at: None,
            propose: true,
            expires_at: None,
        };
        let proposal = created(&request(&mut s, &proposal).unwrap().unwrap());
        let other = created(&req(
            &mut s,
            "draft",
            (crate::model::DECLARATION_CHANGE, true),
            json!({"source": {"fleet": V1}, "source_ref": "git:fleet@0"}),
        ));
        (s, [ready, held, bare], proposal, other)
    }

    fn draft(s: &mut Store, text: &str) -> String {
        created(&req(
            s,
            "draft",
            (crate::model::DECLARATION_CHANGE, true),
            json!({"source": {"fleet": text}, "source_ref": "git:fleet@1"}),
        ))
    }

    fn run(s: &Store, change: &str, text: &str) -> PublishReport {
        let w = s.snapshot().unwrap();
        dry_run(
            &w,
            &Draft {
                change_id: Some(change),
                source: &[("fleet".to_string(), text.to_string())],
                source_ref: Some("git:fleet@1"),
                source_digest: None,
                actor: OPERATOR,
            },
        )
        .unwrap()
    }

    fn ids(r: &PublishReport, code: &str) -> Vec<String> {
        r.affected().get(code).cloned().unwrap_or_default()
    }

    #[test]
    fn a_dry_run_names_every_object_each_impact_reaches() {
        let (mut s, [ready, held, bare], proposal, other) = fleet();
        let text = v2(true, true, true);
        let change = draft(&mut s, &text);
        let r = run(&s, &change, &text);
        assert_eq!(r.version, 2);
        assert!(r.accepted, "{:#?}", r.findings);
        assert_eq!(
            ids(&r, "mapping:removed_states:Robot.HOLD"),
            vec![held.clone()]
        );
        assert_eq!(
            ids(&r, "mapping:removed_members:Grade.OLD"),
            vec![held.clone()]
        );
        assert_eq!(ids(&r, "mapping:backfill:Robot.site").len(), 3);
        assert_eq!(
            ids(&r, "invariant:Robot.graded_when_ready"),
            vec![bare.clone()]
        );
        assert_eq!(
            ids(&r, "newly_enforced:Robot.retire.graded"),
            vec![bare.clone()]
        );
        // every robot that could hold loses it; the one held moves to READY
        assert_eq!(
            ids(&r, "lost:Robot.hold"),
            vec![ready.clone(), bare.clone()]
        );
        assert_eq!(ids(&r, "gained:Robot.retire"), vec![held.clone()]);
        assert_eq!(ids(&r, "lost:Robot.retire"), vec![bare.clone()]);
        assert_eq!(ids(&r, "proposals_invalidated"), vec![proposal]);
        assert_eq!(r.proposals_invalidated, 1);
        assert_eq!(ids(&r, "superseded"), vec![other]);
        let decision = r
            .findings
            .iter()
            .find(|f| f.severity == Severity::Decision)
            .unwrap();
        assert!(decision.message.contains("admits"));
    }

    #[test]
    fn what_no_mapping_covers_is_fatal_and_an_unadmitted_violation_a_decision() {
        let (mut s, [_, held, bare], _, _) = fleet();
        let text = v2(false, false, false);
        let change = draft(&mut s, &text);
        let r = run(&s, &change, &text);
        assert!(!r.accepted);
        assert_eq!(ids(&r, "removed_state:Robot.HOLD"), vec![held.clone()]);
        assert_eq!(ids(&r, "required_absent:Robot.site").len(), 3);
        assert_eq!(ids(&r, "invariant:Robot.graded_when_ready"), vec![bare]);
        // a submit is refused while the dry run finds anything fatal
        assert_eq!(
            req(&mut s, "submit", (&change, false), json!({})),
            Verdict::Unsatisfied {
                clause: "dry_run_clean".into(),
                remedy: Some(Remedy::UnreachableFromHere)
            }
        );
        // with every mapping but no admit, nothing is fatal, and one decision
        // stays unresolved, so it is not accepted
        let partial = v2(true, true, false);
        let r = run(&s, &change, &partial);
        assert!(!r.fatal() && !r.accepted);
    }

    #[test]
    fn a_change_is_drafted_submitted_and_ended_by_its_own_transitions() {
        let (mut s, _, _, _) = fleet();
        assert_eq!(
            req(
                &mut s,
                "draft",
                (crate::model::DECLARATION_CHANGE, true),
                json!({"source": {"fleet": V1}, "source_ref": ""})
            ),
            Verdict::InvalidInput {
                inputs: vec!["source_ref".into()]
            }
        );
        let text = v2(true, true, true);
        let change = draft(&mut s, &text);
        let w = s.snapshot().unwrap();
        let c = w.get(&change).unwrap();
        let want = crate::source::digest(&[("fleet".to_string(), text.clone())]).unwrap();
        assert_eq!(c.attr("source_digest"), Value::Text(want));
        assert_eq!(c.attr("base_version"), Value::Int(1));
        created(&req(&mut s, "submit", (&change, false), json!({})));
        let w = s.snapshot().unwrap();
        let c = w.get(&change).unwrap();
        assert_eq!(c.state, "SUBMITTED");
        let Value::Text(report) = c.attr("report") else {
            panic!("no report attached")
        };
        assert_eq!(
            serde_json::from_str::<Json>(&report).unwrap()["accepted"],
            json!(true)
        );
        created(&req(&mut s, "refresh", (&change, false), json!({})));
        assert!(matches!(
            req(&mut s, "supersede", (&change, false), json!({})),
            Verdict::NotRequestable { .. }
        ));
        created(&req(&mut s, "reject", (&change, false), json!({})));
        assert_eq!(
            req(&mut s, "withdraw", (&change, false), json!({})),
            Verdict::Unavailable {
                state: "REJECTED".into()
            }
        );
        // outside a sandbox, the dry run waits for the core's own checks
        let other = draft(&mut s, &text);
        s.sandbox = false;
        let r = Request {
            transition: "submit".into(),
            actor: OPERATOR.into(),
            object_id: Some(other),
            type_: None,
            inputs: Default::default(),
            occurred_at: None,
            propose: false,
            expires_at: None,
        };
        assert!(
            request(&mut s, &r)
                .unwrap()
                .unwrap_err()
                .0
                .contains("publish checks")
        );
    }
}

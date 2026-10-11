//! A publish's dry run: a change's source checked and its examples run
//! (step 1, by the core's own checker since ADR-0156), loaded as the next
//! version, compared with the installed one, and every object read into the
//! impact report a person approves (`publish-and-import.md` §1 steps 1 to 4,
//! §2, ADR-0155).

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
    /// Per transition, `<Type>.<transition>`, the examples that request it,
    /// applied and refused; reported, never required (ADR-0136).
    pub examples: BTreeMap<String, Vec<String>>,
    /// Per type, the guard conditions the contradiction analysis decided in
    /// full, of all it read (ADR-0137).
    pub analysed: BTreeMap<String, (usize, usize)>,
    /// Every declared type's metric dimensions that name an actor in the
    /// version under publish, as `declaration` marks them, for an upper
    /// layer's door (`publish-and-import.md` §2, ADR-0114); none where the
    /// version does not load.
    pub person_dimensions: Vec<String>,
    /// Each invariant a type of the version declares, `<Type>.<name>`, by
    /// its form, `local`, `traversal` or `type_scan` (§3.4, ADR-0055).
    pub invariant_forms: BTreeMap<String, String>,
    /// Each invariant a type of the version declares with the transitions
    /// that could violate it (`DESIGN.md` §5.6).
    pub could_violate: BTreeMap<String, Vec<String>>,
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

/// A change's dry run computed on a read of the store beside its writes,
/// for a `submit` or `refresh` to record without computing it under the
/// writer's lock (DRYRUN-LOCK): taken only where the change and the store's
/// version are what the read saw, and computed again otherwise. The report
/// is the reviewer's to read, and an approval computes its own again.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub change: String,
    /// The change's version and the store's declaration version it was
    /// computed at.
    pub change_version: i64,
    pub at: i64,
    pub report: PublishReport,
}

/// A change's source, reference and digest, as its record holds them.
pub(crate) fn draft_of(o: &Object) -> (Vec<(String, String)>, String, String) {
    let text = |a: &str| match o.attr(a) {
        Value::Text(t) => t,
        _ => String::new(),
    };
    let source: Vec<(String, String)> =
        serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&text("source"))
            .unwrap_or_default()
            .into_iter()
            .map(|(n, t)| (n, t.as_str().unwrap_or_default().to_string()))
            .collect();
    (source, text("source_ref"), text("source_digest"))
}

/// The dry run a `submit` or `refresh` of a change would record, computed on
/// a read; none where the id names no change in a state either is taken
/// from.
pub fn prepare(world: &World<'_>, change: &str, actor: &str) -> Result<Option<Prepared>, NotRun> {
    let Some(o) = world
        .get(change)?
        .filter(|o| o.type_ == crate::model::DECLARATION_CHANGE)
        .filter(|o| o.state == "DRAFTED" || o.state == "SUBMITTED")
    else {
        return Ok(None);
    };
    let (source, source_ref, source_digest) = draft_of(o);
    let report = dry_run(
        world,
        &Draft {
            change_id: Some(&o.id),
            source: &source,
            source_ref: Some(&source_ref),
            source_digest: Some(&source_digest),
            actor,
        },
    )?;
    Ok(Some(Prepared {
        change: o.id.clone(),
        change_version: o.version,
        at: world.lib.version,
        report,
    }))
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
        "examples": r.examples,
        "analysed": r.analysed,
        "person_dimensions": r.person_dimensions,
        "invariant_forms": r.invariant_forms,
        "could_violate": r.could_violate,
        "not_computed": [
            "invariants", "sweepable", "unsweepable", "queryable_derives", "worst_case_fanout",
            "observed_fanout", "replaced_creations", "creations_past_first_state", "observing_clauses",
            "flags",
        ],
    })
}

/// The dry run over a sandbox's objects.
pub fn dry_run(world: &World<'_>, draft: &Draft<'_>) -> Result<PublishReport, NotRun> {
    simulate(world, draft).map(|s| s.report)
}

/// A dry run, and what an approval would install from it: the next
/// version's library and every object as its mappings leave it, so the
/// publish does what its dry run reported (`publish-and-import.md` §1).
#[derive(Debug)]
pub struct Simulated {
    pub report: PublishReport,
    pub install: Option<(Library, BTreeMap<String, Object>)>,
}

pub fn simulate(world: &World<'_>, draft: &Draft<'_>) -> Result<Simulated, NotRun> {
    let old = world.lib;
    let mut found = Found::default();
    let report = PublishReport {
        version: old.version + 1,
        accepted: false,
        now: world.now,
        findings: Vec::new(),
        change_id: draft.change_id.map(str::to_string),
        source_ref: draft.source_ref.map(str::to_string),
        source_digest: draft.source_digest.map(str::to_string),
        proposals_invalidated: 0,
        examples: BTreeMap::new(),
        analysed: BTreeMap::new(),
        person_dimensions: Vec::new(),
        invariant_forms: BTreeMap::new(),
        could_violate: BTreeMap::new(),
    };
    // step 1, the checks and the examples (ADR-0156), each module against
    // the installed version of it
    let previous: BTreeMap<String, String> = old.source.iter().cloned().collect();
    let checks = crate::check::checked(draft.source, &previous);
    // every declared type's marks in the version under publish, read from
    // it whether or not its impact can be (§2, ADR-0114)
    let person_dimensions = match Library::load(draft.source) {
        Ok(lib) => {
            let mut out = Vec::new();
            for (tn, t) in &lib.types {
                if !builtin(tn) && t.observation_on.is_none() {
                    out.extend(crate::metric::person_dimensions(&lib, tn)?);
                }
            }
            out.sort();
            out
        }
        Err(_) => Vec::new(),
    };
    let report = PublishReport {
        examples: checks.examples,
        analysed: checks.analysed,
        invariant_forms: checks.forms,
        could_violate: checks.violators,
        person_dimensions,
        ..report
    };
    for f in checks.findings {
        let severity = match f.severity {
            crate::check::Severity::Fatal => Severity::Fatal,
            crate::check::Severity::Notice => Severity::Notice,
        };
        let (severity, code) = match f.code.as_str() {
            // an example's failure by the name the design gives it (ADR-0136
            // decision 5, publish-and-import.md §1 step 1)
            "example" => (Severity::Fatal, "example_failed".to_string()),
            // an example the core does not yet run has not passed, and
            // nothing unchecked is installed (ADR-0155 decision 1, ADR-0156
            // decision 3)
            "notrun" => (Severity::Fatal, "example_not_run".to_string()),
            // a version's source is the whole version, so an import from a
            // module it does not carry resolves to nothing (ADR-0161)
            "imports" => (Severity::Fatal, "check:imports".to_string()),
            other => (severity, format!("check:{other}")),
        };
        let message = if f.code == "imports" {
            format!(
                "{}:{}: {}; a version carries every module it imports from, so it is refused",
                f.file, f.line, f.message
            )
        } else {
            format!("{}:{}: {}", f.file, f.line, f.message)
        };
        found.add(severity, code, message, None);
    }
    // the dry run goes on past what the checks refuse, reporting everything
    // (publish-and-import.md §1 step 4), except where the rules they refuse
    // cannot be run over the objects, which it says
    let checked = found.0.clone();
    let refused = checked.iter().any(|f| f.severity == Severity::Fatal);
    match impact(world, draft, found, report.clone()) {
        // a read of the store that failed is not a rule that cannot run
        Err(NotRun::Unsupported(e)) if refused => {
            let mut found = Found(checked);
            found.add(
                Severity::Notice,
                "not_computed".into(),
                format!("the impact past the checks' fatal findings, whose rules cannot be run over the objects: {e}"),
                None,
            );
            let mut report = report;
            report.findings = found.0;
            Ok(Simulated {
                report,
                install: None,
            })
        }
        other => other,
    }
}

/// Steps 2 and 3 over the objects, after the checks: the source loaded,
/// compared with the installed version, and every object it touches read
/// into the report.
fn impact(
    world: &World<'_>,
    draft: &Draft<'_>,
    mut found: Found,
    mut report: PublishReport,
) -> Result<Simulated, NotRun> {
    let old = world.lib;
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
            return Ok(Simulated {
                report,
                install: None,
            });
        }
    };
    new.version = old.version + 1;
    // a type the version drops is retired: its objects and history stay
    // readable, and nothing creates or changes one (storage-schema.md §10)
    for (tn, t) in &old.types {
        if !new.types.contains_key(tn) {
            let mut retired = t.clone();
            retired.transitions.clear();
            retired.retired = true;
            new.types.insert(tn.clone(), retired);
        }
    }
    let m = new.migration.clone();
    // the objects as the mappings leave them, read under the new version
    let mut mapped: BTreeMap<String, Object> = BTreeMap::new();
    // every object whole, the version carrying each record over
    let all = world.in_order()?;
    world.with_parts(&all)?;
    for o in all {
        if builtin(&o.type_) {
            // no change touches a built-in type, whose objects, the actors
            // among them, stand as they are
            mapped.insert(o.id.clone(), o.clone());
            continue;
        }
        let Some(nt) = new.type_(&o.type_).filter(|t| !t.retired) else {
            // a retired type's objects are read for nothing
            mapped.insert(o.id.clone(), o.clone());
            continue;
        };
        let mut x = o.clone();
        // renames first, which write nothing of their own
        for (tn, from, to) in &m.renamed_attributes {
            if new.is_a(&o.type_, tn) {
                if let Some(v) = x.attrs.remove(from) {
                    x.attrs.insert(to.clone(), v);
                }
                // a rename carries the attribute's history, its intervals
                // included (publish-and-import.md §3)
                let parts = x.parts_mut()?;
                if let Some(spans) = parts.spans.remove(from) {
                    parts.spans.insert(to.clone(), spans);
                }
            }
        }
        // an attribute whose type changed starts absent, its values set aside
        // with their history (storage-schema.md §10)
        if let Some(ot) = old.type_(&o.type_) {
            for (a, spec) in &ot.attributes {
                if nt
                    .attribute(a)
                    .is_some_and(|na| na.declared != spec.declared || na.many != spec.many)
                {
                    x.attrs.remove(a);
                    x.parts_mut()?.spans.remove(a);
                }
            }
        }
        // an observation is born final, and no migration reaches one: a
        // removed member it holds is fatal below (storage-schema.md §10)
        if nt.observation_on.is_some() {
            mapped.insert(x.id.clone(), x);
            continue;
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
    for o in mapped.into_values() {
        sim.insert(o)?;
    }
    // then backfills, in the order written, each reading the object as the
    // mappings before it left it, an erased object's personal values skipped
    for (tn, attr, e) in &m.backfill {
        let ids: Vec<String> = sim.of_type(tn)?.iter().map(|o| o.id.clone()).collect();
        for id in ids {
            let Some(o) = sim.get(&id)? else { continue };
            let Some(t) = new.type_(&o.type_).filter(|t| t.observation_on.is_none()) else {
                continue;
            };
            if sim.parts(o)?.erased && t.attribute(attr).is_some_and(|a| a.personal) {
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
                if let Some(o) = sim.get_mut(&id)? {
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
    for o in sim.in_order()? {
        if builtin(&o.type_) {
            continue;
        }
        let Some(t) = new.type_(&o.type_).filter(|t| !t.retired) else {
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
            .get(&o.id)?
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
                    _ => opposite_parts(&sim, o, spec)?,
                };
                for p in parts {
                    let live = sim.get(&p)?.is_some_and(|po| {
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
        // admission covers is a decision, which an admit may resolve, and a
        // standing admission whose invariant now holds, or which the version
        // no longer declares, is discharged (D531)
        let failing = failing_invariants(&sim, o)?;
        let admissions = &sim.parts(o)?.admissions;
        for n in admissions.iter().filter(|n| !failing.contains(n)) {
            found.add(
                Severity::Notice,
                format!("discharged:{}.{n}", o.type_),
                format!(
                    "these objects hold an admitted violation of {}.{n} that the version cures, which the publish discharges",
                    o.type_
                ),
                Some(&o.id),
            );
        }
        for n in failing {
            if admissions.contains(&n) {
                continue;
            }
            // a kind's new invariant applies to recordings after the publish,
            // and nothing can change the observations that violate it
            // (ADR-0146)
            if t.observation_on.is_some() {
                found.add(
                    Severity::Notice,
                    format!("observation_invariant:{}.{n}", o.type_),
                    format!("observations already recorded violate {}.{n}, which applies to later recordings", o.type_),
                    Some(&o.id),
                );
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
    report.proposals_invalidated = proposals(world, &new, &mut found)?;
    // every other open change, which the publish supersedes
    for c in world.of_type(crate::model::DECLARATION_CHANGE)? {
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
    let objects: BTreeMap<String, Object> = sim
        .in_order()?
        .into_iter()
        .map(|o| (o.id.clone(), o.clone()))
        .collect();
    Ok(Simulated {
        report,
        install: Some((new, objects)),
    })
}

/// Whether a type is one of the store's own, which no change declares.
pub(crate) fn builtin(type_: &str) -> bool {
    [
        crate::model::OPERATOR_TYPE,
        PROPOSAL,
        crate::model::DECLARATION_CHANGE,
        crate::model::SUBSCRIPTION,
        crate::model::LABEL,
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
fn opposite_parts(
    world: &World<'_>,
    o: &Object,
    spec: &crate::model::Attribute,
) -> Result<Vec<String>, NotRun> {
    let (Declared::Reference(target), Some(back)) = (&spec.declared, &spec.opposite) else {
        return Ok(Vec::new());
    };
    Ok(world
        .of_type(target)?
        .iter()
        .filter(|p| p.attr(back) == Value::Object(o.id.clone()))
        .map(|p| p.id.clone())
        .collect())
}

/// The clauses the version newly enforces that would refuse an object, and
/// the transitions each object gains or loses, a transition whose verdict
/// waits on an input, an evaluator or a metric being neither, and none
/// consulted (`publish-and-import.md` §1 step 3, D537, D538).
fn clauses_and_offers(
    old: &World<'_>,
    new: &World<'_>,
    actor: &str,
    found: &mut Found,
) -> Result<(), NotRun> {
    for o in new.in_order()? {
        if builtin(&o.type_) {
            continue;
        }
        let (Some(nt), Some(before)) = (
            new.lib.type_(&o.type_).filter(|t| !t.retired),
            old.get(&o.id)?,
        ) else {
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
                let asking = Evaluator::new(new, Some(nt), Some(&o.id)).consulting_no_metric();
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
        // as availability classes it: a refusal is one whatever it left
        // unevaluated, and an offer otherwise waits only on a `deny` clause
        // it could not evaluate, which alone could refuse it (D538)
        let class = |offers: &[reads::Offer], t: Option<&crate::model::Type>, xn: &str| {
            offers
                .iter()
                .find(|f| f.transition == xn && f.creates.is_none())
                .map(|f| {
                    let deny = |g: &String| {
                        t.and_then(|t| t.transition(xn)).is_some_and(|x| {
                            x.guards.iter().any(|(n, m)| n == g && *m == Enforce::Deny)
                        })
                    };
                    if f.availability != Availability::Unavailable && f.unevaluated.iter().any(deny)
                    {
                        Availability::AvailableWithInput
                    } else {
                        f.availability
                    }
                })
        };
        let before_offers = reads::offers(old, actor, &before.id, false)?.unwrap_or_default();
        let after_offers = reads::offers(new, actor, &o.id, false)?.unwrap_or_default();
        let names: std::collections::BTreeSet<String> = before_offers
            .iter()
            .chain(&after_offers)
            .filter(|f| f.creates.is_none())
            .map(|f| f.transition.clone())
            .collect();
        for xn in names {
            let (b, a) = (
                class(&before_offers, ot, &xn),
                class(&after_offers, Some(nt), &xn),
            );
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
fn proposals(world: &World<'_>, new: &Library, found: &mut Found) -> Result<usize, NotRun> {
    let mut n = 0;
    for p in world.of_type(PROPOSAL)? {
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
    Ok(n)
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
    use crate::verdict::{InputReason, Verdict};
    use serde_json::{Value as Json, json};

    const V1: &str = r#"
module: fleet
categories: [live, closed]
enumerations:
  Grade: [A, B, OLD]
  OverrideReason: [LEGACY]
types:
  Robot:
    description: A robot of the fleet.
    tracking: record
    attributes:
      serial: { type: string }
      grade: { type: Grade, optional: true }
    states:
      READY: { category: live }
      HOLD: { category: live }
      RETIRED: { category: closed, final: true }
    conditions:
      graded: { description: The robot is graded., expression: grade is not null, remedy: self_serviceable }
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
        let mut migration = String::from(
            "migration:\n  description: The fleet without holds.\n  removed_members:\n    Grade: { OLD: B }\n",
        );
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
    description: A robot of the fleet.
    tracking: record
    attributes:
      serial: {{ type: string }}
      grade: {{ type: Grade, optional: true }}
      site: {{ type: string }}
    states:
      READY: {{ category: live }}
      RETIRED: {{ category: closed, final: true }}
    invariants:
      graded_when_ready: {{ description: A ready robot is graded., expression: "state != Robot.READY or grade is not null" }}
    conditions:
      graded: {{ description: The robot is graded., expression: grade is not null, remedy: self_serviceable }}
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
            expected_version: None,
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
            expected_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
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

    fn run_on(s: &Store, change: &str, module: &str, text: &str) -> PublishReport {
        let w = s.snapshot().unwrap();
        dry_run(
            &w,
            &Draft {
                change_id: Some(change),
                source: &[(module.to_string(), text.to_string())],
                source_ref: Some("git:lab@1"),
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

    /// A guard reading a metric is not consulted by a dry run, as an
    /// evaluator is not asked (D537), and an offer it leaves unevaluated is
    /// classed as availability classes it, whatever the guards' order: a
    /// refusal on another clause is a loss, an offer passing every other
    /// clause waits on it, and an audited clause, which refuses nothing,
    /// leaves nothing waiting (D538).
    #[test]
    fn a_dry_run_consults_no_metric_and_classes_an_offer_as_availability_does() {
        let (mut s, [_, held, bare], _, _) = fleet();
        // four robots priced at USD 4.00, over the ceiling, were it read
        let metered = |guards: &str| {
            let text = v2(true, true, true)
                .replace(
                    "migration:\n",
                    "    metrics:\n      priced: { description: Robots priced., source: objects, item: o, expression: count() * USD 1.00 }\nmigration:\n",
                )
                .replace(
                    "remedy: self_serviceable }\n",
                    "remedy: self_serviceable }\n      settled: { description: Few robots priced., expression: \"metric(priced) < USD 3.00\", remedy: dependent }\n",
                )
                .replace("guards: { graded: deny }", guards);
            assert!(text.contains(guards) && text.contains("metric(priced)"));
            text
        };
        for guards in [
            "guards: { graded: deny, settled: deny }",
            "guards: { settled: deny, graded: deny }",
        ] {
            let text = metered(guards);
            let change = draft(&mut s, &text);
            let r = run(&s, &change, &text);
            assert!(
                ids(&r, "newly_enforced:Robot.retire.settled").is_empty(),
                "{guards}"
            );
            assert_eq!(
                ids(&r, "newly_enforced:Robot.retire.graded"),
                vec![bare.clone()]
            );
            // refused on `graded` whatever `settled` would say; the held
            // robot, graded once mapped, waits on it
            assert_eq!(ids(&r, "lost:Robot.retire"), vec![bare.clone()], "{guards}");
            assert!(ids(&r, "gained:Robot.retire").is_empty(), "{guards}");
        }
        let text = metered("guards: { graded: deny, settled: audit }");
        let change = draft(&mut s, &text);
        let r = run(&s, &change, &text);
        assert_eq!(ids(&r, "gained:Robot.retire"), vec![held]);
        assert_eq!(ids(&r, "lost:Robot.retire"), vec![bare]);
    }

    #[test]
    fn a_dry_run_reports_what_the_checks_find() {
        let (mut s, _, _, _) = fleet();
        // a guard on no declared condition, at the line of the guard
        let text = v2(true, true, true)
            .replace("guards: {{ graded: deny }}", "guards: {{ grade: deny }}")
            .replace("guards: { graded: deny }", "guards: { grade: deny }");
        let change = draft(&mut s, &text);
        let r = run(&s, &change, &text);
        let refused = r
            .findings
            .iter()
            .find(|f| f.code == "check:names" && f.severity == Severity::Fatal)
            .expect("the checks' finding");
        assert!(
            refused
                .message
                .starts_with("fleet:24: Robot.retire guards on 'grade'"),
            "{}",
            refused.message
        );
        assert!(r.fatal() && !r.accepted);
        // a clean module's notices are reported, and refuse nothing
        let clean = v2(true, true, true);
        let r = run(&s, &change, &clean);
        assert!(
            r.findings
                .iter()
                .any(|f| f.code == "check:self_serviceable" && f.severity == Severity::Notice)
        );
        assert!(r.accepted, "{:#?}", r.findings);
    }

    #[test]
    fn a_dry_run_runs_the_examples_and_refuses_one_that_fails() {
        let (mut s, _, _, _) = fleet();
        let text = v2(true, true, true);
        let change = draft(&mut s, &text);
        let examples = |verdict: &str| {
            format!(
                "examples_for: fleet\nexamples:\n  a_graded_robot_is_retired:\n    description: A graded robot is retired.\n    \
                 steps:\n      - request: {{ type: Robot, transition: add, as: r, inputs: {{ serial: R1, site: north, grade: Grade.A }} }}\n    \
                 request: {{ object: r, transition: retire }}\n    expect: {verdict}\n"
            )
        };
        let with = |ex: &str| {
            let w = s.snapshot().unwrap();
            dry_run(
                &w,
                &Draft {
                    change_id: Some(&change),
                    source: &[
                        ("fleet".to_string(), text.clone()),
                        ("fleet.examples".to_string(), ex.to_string()),
                    ],
                    source_ref: Some("git:fleet@1"),
                    source_digest: None,
                    actor: OPERATOR,
                },
            )
            .unwrap()
        };
        let r = with(&examples("{ verdict: applied, state: RETIRED }"));
        // which example requests each transition, and what step 5 decided
        assert_eq!(
            r.examples.get("Robot.retire"),
            Some(&vec!["a_graded_robot_is_retired".to_string()])
        );
        assert_eq!(r.analysed.get("Robot"), Some(&(1, 1)));
        assert!(
            r.findings.iter().all(|f| f.code != "example_failed"),
            "{:#?}",
            r.findings
        );
        assert!(r.accepted, "{:#?}", r.findings);
        let r = with(&examples(
            "{ verdict: unsatisfied, clause: graded, remedy: self_serviceable }",
        ));
        let failed = r
            .findings
            .iter()
            .find(|f| f.code == "example_failed")
            .expect("the example's failure");
        assert_eq!(failed.severity, Severity::Fatal);
        assert!(
            failed.message.contains("a_graded_robot_is_retired: "),
            "{}",
            failed.message
        );
        assert!(r.fatal() && !r.accepted);
        // an example reaching what the core does not yet run refuses the
        // publish, which cannot say it passed
        // a guard reading a metric whose value is an amount in a currency
        // whose minor unit the core does not know, so it cannot round it
        let metered = text
            .replace(
                "migration:\n",
                "    metrics:\n      priced: { description: Robots priced in pesos., source: objects, item: o, expression: count() * PHP 1.00 }\nmigration:\n",
            )
            .replace(
                "remedy: self_serviceable }\n",
                "remedy: self_serviceable }\n      settled: { description: Few robots priced., expression: \"metric(priced) < PHP 3.00\", remedy: dependent }\n",
            )
            .replace("guards: { graded: deny }", "guards: { graded: deny, settled: deny }");
        assert!(metered.contains("metric(priced)") && metered.contains("settled: deny"));
        let w = s.snapshot().unwrap();
        let r = dry_run(
            &w,
            &Draft {
                change_id: Some(&change),
                source: &[
                    ("fleet".to_string(), metered),
                    (
                        "fleet.examples".to_string(),
                        examples("{ verdict: applied, state: RETIRED }"),
                    ),
                ],
                source_ref: Some("git:fleet@1"),
                source_digest: None,
                actor: OPERATOR,
            },
        )
        .unwrap();
        let unrun = r
            .findings
            .iter()
            .find(|f| f.code == "example_not_run")
            .unwrap_or_else(|| panic!("{:#?}", r.findings));
        assert_eq!(unrun.severity, Severity::Fatal);
        assert!(r.fatal() && !r.accepted);
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
            Verdict::invalid_input([("source_ref".to_string(), InputReason::Malformed)])
        );
        // a file under a name it does not declare
        assert_eq!(
            req(
                &mut s,
                "draft",
                (crate::model::DECLARATION_CHANGE, true),
                json!({"source": {"lab": V1}, "source_ref": "git:fleet@1"})
            ),
            Verdict::invalid_input([("source".to_string(), InputReason::Malformed)])
        );
        let text = v2(true, true, true);
        let change = draft(&mut s, &text);
        let w = s.snapshot().unwrap();
        let c = w.whole(&change).unwrap().unwrap();
        let want = crate::source::digest(&[("fleet".to_string(), text.clone())]).unwrap();
        assert_eq!(c.attr("source_digest"), Value::Text(want));
        assert_eq!(c.attr("base_version"), Value::Int(1));
        created(&req(&mut s, "submit", (&change, false), json!({})));
        let w = s.snapshot().unwrap();
        let c = w.whole(&change).unwrap().unwrap();
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
        // a submit is decided by the core's own checks, in any store
        // (ADR-0156 decision 5)
        let other = draft(&mut s, &text);
        let r = Request {
            transition: "submit".into(),
            actor: OPERATOR.into(),
            object_id: Some(other),
            type_: None,
            inputs: Default::default(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
        };
        assert!(matches!(
            request(&mut s, &r).unwrap().unwrap(),
            Verdict::Satisfied { .. }
        ));
    }

    fn approve(s: &mut Store, change: &str, version: i64) -> (Verdict, Option<PublishReport>) {
        s.publish(OPERATOR, change, version).unwrap().unwrap()
    }

    fn submitted(s: &mut Store, text: &str) -> (String, i64) {
        let change = draft(s, text);
        created(&req(s, "submit", (&change, false), json!({})));
        let version = s
            .snapshot()
            .unwrap()
            .whole(&change)
            .unwrap()
            .unwrap()
            .version;
        (change, version)
    }

    fn refused(clause: &str) -> Verdict {
        Verdict::Unsatisfied {
            clause: clause.into(),
            remedy: Some(Remedy::UnreachableFromHere),
        }
    }

    #[test]
    fn a_publish_discharges_the_admissions_its_version_cures() {
        const MIGRATION: &str = "migration:\n  description: The fleet without holds.\n  removed_members:\n    Grade: { OLD: B }\n";
        const INVARIANT: &str = "    invariants:\n      graded_when_ready: { description: A ready robot is graded., expression: \"state != Robot.READY or grade is not null\" }\n";
        // the version curing it: a backfill grading every robot, or the
        // invariant dropped with no mapping touching the robot
        let cures = [
            v2(false, false, false).replace(
                MIGRATION,
                "migration:\n  description: Every robot graded.\n  backfill:\n    Robot: { grade: 'Grade.A' }\n",
            ),
            v2(false, false, false).replace(MIGRATION, "").replace(INVARIANT, ""),
        ];
        for (i, v3) in cures.iter().enumerate() {
            let (mut s, [ready, _, bare], _, _) = fleet();
            let (change, v) = submitted(&mut s, &v2(true, true, true));
            assert!(matches!(
                approve(&mut s, &change, v).0,
                Verdict::Satisfied { .. }
            ));
            let admissions = |s: &Store| {
                s.snapshot()
                    .unwrap()
                    .whole(&bare)
                    .unwrap()
                    .unwrap()
                    .held_parts()
                    .unwrap()
                    .admissions
                    .clone()
            };
            assert!(admissions(&s).contains("graded_when_ready"));
            let (change, v) = submitted(&mut s, v3);
            let r = run(&s, &change, v3);
            assert!(r.accepted, "cure {i}: {:#?}", r.findings);
            assert_eq!(
                ids(&r, "discharged:Robot.graded_when_ready"),
                vec![bare.clone()],
                "cure {i}"
            );
            assert!(matches!(
                approve(&mut s, &change, v).0,
                Verdict::Satisfied { .. }
            ));
            // discharged at the publish, on the robot's own migration
            assert!(admissions(&s).is_empty(), "cure {i}");
            let migration = s.history(&bare).unwrap().pop().unwrap();
            assert_eq!(migration.source, "migrated", "cure {i}");
            assert_eq!(
                migration.payload["discharges"],
                json!(["graded_when_ready"]),
                "cure {i}"
            );
            let at: Option<i64> = s
                .connection()
                .query_row(
                    "SELECT discharged_position FROM of_admission WHERE object_id = ?1",
                    [&bare],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(at, Some(migration.position), "cure {i}");
            // a later request on another robot discharges nothing of it
            req(&mut s, "retire", (&ready, false), json!({}));
            assert_eq!(s.history(&bare).unwrap().len(), {
                let w = s.snapshot().unwrap();
                usize::try_from(w.whole(&bare).unwrap().unwrap().version).unwrap()
            });
        }
    }

    #[test]
    fn a_publish_indexes_the_identifiers_its_version_marks() {
        let (mut s, [ready, _, _], _, _) = fleet();
        let lookup = |s: &Store| s.snapshot().unwrap().external_id("crm", "R1").unwrap();
        // marked external, a robot is found by the serial it held already
        let marked = V1.replace(
            "      serial: { type: string }",
            "      serial: { type: string, external: crm }",
        );
        let (change, v) = submitted(&mut s, &marked);
        assert!(matches!(
            approve(&mut s, &change, v).0,
            Verdict::Satisfied { .. }
        ));
        assert_eq!(lookup(&s), Some(ready.clone()));
        // and no longer once unmarked (D533)
        let (change, v) = submitted(&mut s, V1);
        assert!(matches!(
            approve(&mut s, &change, v).0,
            Verdict::Satisfied { .. }
        ));
        assert_eq!(lookup(&s), None);
    }

    /// A publish approved by an actor of a declared type, which the
    /// approval reads by id before the install reads every object, installs
    /// as any other: the install reads each object's record, the actor's
    /// among them, for the identifiers it held (ADR-0165; the scenarios'
    /// replay found its publish answered 500 without it).
    #[test]
    fn a_publish_approved_by_a_declared_actor_reads_every_record_it_installs() {
        const USER: &str = "  User:\n    description: A person who works the fleet.\n    tracking: record\n    attributes:\n      login: { type: identity, actor_kind: human, unique: true }\n    states: { ACTIVE: { category: live }, GONE: { category: closed, final: true } }\n    transitions:\n      add: { kind: initial, to: ACTIVE, required_inputs: [login] }\n      leave: { kind: external, from: [ACTIVE], to: GONE }\n  Robot:\n";
        let v1 = V1.replacen("  Robot:\n", USER, 1);
        let lib = Library::load(&[("fleet.yaml".into(), v1.clone())]).unwrap();
        let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
        let ana = created(&req(&mut s, "add", ("User", true), json!({"login": "ana"})));
        let robot = created(&req(
            &mut s,
            "add",
            ("Robot", true),
            json!({"serial": "R1"}),
        ));
        let marked = v1.replace(
            "      serial: { type: string }",
            "      serial: { type: string, external: crm }",
        );
        let (change, v) = submitted(&mut s, &marked);
        let (verdict, _) = s.publish(&ana, &change, v).unwrap().unwrap();
        assert!(matches!(verdict, Verdict::Satisfied { .. }), "{verdict:?}");
        let w = s.snapshot().unwrap();
        assert_eq!(w.external_id("crm", "R1").unwrap(), Some(robot));
        assert_eq!(
            w.whole(&ana).unwrap().unwrap().attr("login"),
            Value::Text("ana".into())
        );
    }

    #[test]
    fn an_erased_value_a_version_marks_is_never_indexed() {
        let lib = Library::load(&[("shop.yaml".into(), SHOP_V1.into())]).unwrap();
        let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
        let ana = created(&req(
            &mut s,
            "join",
            ("Customer", true),
            json!({"nick": "ana"}),
        ));
        created(&req(
            &mut s,
            "forget",
            (&ana, false),
            json!({"reason": "asked"}),
        ));
        // the nick, not personal, outlives the erasure, and the version
        // makes it personal and an identifier at once: it is redacted, and
        // lookup never finds it (D533)
        let text = SHOP_V1.replace(
            "      nick: { type: string, optional: true }",
            "      nick: { type: string, optional: true, personal: true, external: crm }",
        );
        let change = created(&req(
            &mut s,
            "draft",
            (crate::model::DECLARATION_CHANGE, true),
            json!({"source": {"shop": &text}, "source_ref": "git:shop@1"}),
        ));
        created(&req(&mut s, "submit", (&change, false), json!({})));
        let v = s
            .snapshot()
            .unwrap()
            .whole(&change)
            .unwrap()
            .unwrap()
            .version;
        assert!(matches!(
            approve(&mut s, &change, v).0,
            Verdict::Satisfied { .. }
        ));
        let w = s.snapshot().unwrap();
        assert_eq!(w.whole(&ana).unwrap().unwrap().attr("nick"), Value::Absent);
        assert_eq!(w.external_id("crm", "ana").unwrap(), None);
    }

    #[test]
    fn a_publish_indexes_a_uniqueness_it_adds_and_drops_one_it_removes() {
        let (mut s, _, _, _) = fleet();
        let indexed = |s: &Store| -> bool {
            s.connection()
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = 't_robot_serial_key_ix'",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap()
                == 1
        };
        assert!(!indexed(&s));
        let unique = V1.replace(
            "      serial: { type: string }",
            "      serial: { type: string, unique: true }",
        );
        let (change, v) = submitted(&mut s, &unique);
        assert!(matches!(
            approve(&mut s, &change, v).0,
            Verdict::Satisfied { .. }
        ));
        assert!(indexed(&s));
        let (change, v) = submitted(&mut s, V1);
        assert!(matches!(
            approve(&mut s, &change, v).0,
            Verdict::Satisfied { .. }
        ));
        assert!(!indexed(&s));
    }

    /// A state the installed version no longer declares, which a robot's
    /// history holds from before the publish removed it, follows those it
    /// declares in a page's order (D536).
    #[test]
    fn a_removed_state_follows_the_declared_ones_in_a_page() {
        let (mut s, _, _, _) = fleet();
        let (change, v) = submitted(&mut s, &v2(true, true, true));
        assert!(matches!(
            approve(&mut s, &change, v).0,
            Verdict::Satisfied { .. }
        ));
        let w = s.snapshot().unwrap();
        let keep = vec!["state".to_string()];
        let page = crate::metric::page(
            &w,
            None,
            "Robot.time_in_state",
            &BTreeMap::new(),
            Some(&keep),
            None,
            None,
        )
        .unwrap();
        let states: Vec<Value> = page
            .rows
            .iter()
            .map(|r| r.dimensions[0].1.clone())
            .collect();
        let state = |n: &str| Value::State {
            type_: "Robot".into(),
            name: n.into(),
        };
        assert_eq!(states, [state("READY"), state("HOLD")]);
    }

    #[test]
    fn an_approval_installs_what_its_dry_run_reported() {
        let (mut s, [ready, held, bare], proposal, other) = fleet();
        let (change, v) = submitted(&mut s, &v2(true, true, true));
        // the approver read an earlier version of the change
        assert_eq!(
            approve(&mut s, &change, v - 1).0,
            Verdict::Stale {
                expected: v - 1,
                actual: v
            }
        );
        let (verdict, report) = approve(&mut s, &change, v);
        let Verdict::Satisfied { events, .. } = verdict else {
            panic!("{verdict:?}")
        };
        // the publish event, three migrations, a supersession, an invalidation
        assert_eq!(events.len(), 6);
        assert!(report.unwrap().accepted);
        assert_eq!(s.lib.version, 2);
        // every object as the mappings left it, read back under version 2
        let w = s.snapshot().unwrap();
        let h = w.whole(&held).unwrap().unwrap();
        assert_eq!(h.state, "READY");
        let member = |m: &str| Value::Member {
            enumeration: "Grade".into(),
            member: m.into(),
        };
        assert_eq!(h.attr("grade"), member("B"));
        for id in [&ready, &held, &bare] {
            assert_eq!(
                w.whole(id).unwrap().unwrap().attr("site"),
                Value::Text("north".into())
            );
        }
        assert!(
            w.whole(&bare)
                .unwrap()
                .unwrap()
                .held_parts()
                .unwrap()
                .admissions
                .contains("graded_when_ready")
        );
        assert_eq!(w.whole(&other).unwrap().unwrap().state, "SUPERSEDED");
        assert_eq!(w.whole(&proposal).unwrap().unwrap().state, "INVALIDATED");
        let c = w.whole(&change).unwrap().unwrap();
        assert_eq!(c.state, "PUBLISHED");
        assert_eq!(c.attr("approved_by"), Value::Text(OPERATOR.into()));
        // the held robot's intervals: HOLD and OLD closed at its migration,
        // READY and B opened there, marked migrated
        for (d, was, now) in [
            (
                "state",
                Value::State {
                    type_: "Robot".into(),
                    name: "HOLD".into(),
                },
                Value::State {
                    type_: "Robot".into(),
                    name: "READY".into(),
                },
            ),
            ("grade", member("OLD"), member("B")),
        ] {
            let spans = &h.held_parts().unwrap().spans[d];
            let (before, last) = (&spans[spans.len() - 2], &spans[spans.len() - 1]);
            assert_eq!((&before.value, &last.value), (&was, &now), "{d}");
            assert_eq!(before.left_position, Some(last.entered_position));
            assert!(last.migrated && !before.migrated, "{d}");
        }
        // each migration recorded under the version it migrated from, its
        // state's source left as it was
        let history = s.history(&held).unwrap();
        let migrate = history.last().unwrap();
        assert_eq!(
            (migrate.transition.as_str(), migrate.source.as_str()),
            ("migrate", "migrated")
        );
        assert_eq!(migrate.declaration_version, 1);
        let published = s.history(&change).unwrap();
        assert_eq!(published.last().unwrap().declaration_version, 2);
        // a migration is caused by the publish's event
        assert_eq!(migrate.cause, Some(published.last().unwrap().position));
        let (source, versions): (String, i64) = s
            .conn()
            .query_row(
                "SELECT (SELECT state_source FROM t_robot WHERE id = ?1), \
                 (SELECT count(*) FROM of_declaration)",
                [&held],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((source.as_str(), versions), ("observed", 3));
        // the table holds the version's shape: HOLD refused, site required
        for broken in [
            "UPDATE t_robot SET state = 'HOLD'",
            "UPDATE t_robot SET site = NULL",
            "UPDATE t_robot SET grade = 'OLD'",
        ] {
            assert!(s.conn().execute(broken, []).is_err(), "{broken}");
        }
        // and the store runs version 2's requests
        assert!(matches!(
            req(&mut s, "add", ("Robot", true), json!({"serial": "R4"})),
            Verdict::InvalidInput { .. }
        ));
        created(&req(
            &mut s,
            "add",
            ("Robot", true),
            json!({"serial": "R4", "grade": "A", "site": "south"}),
        ));
        created(&req(&mut s, "retire", (&held, false), json!({})));
        assert_eq!(
            req(&mut s, "retire", (&bare, false), json!({})),
            Verdict::Unsatisfied {
                clause: "graded".into(),
                remedy: Some(Remedy::SelfServiceable)
            }
        );
    }

    #[test]
    fn an_approval_is_refused_on_anything_its_approver_did_not_see() {
        let (mut s, _, _, _) = fleet();
        // a decision no admit resolves
        let (change, v) = submitted(&mut s, &v2(true, true, false));
        let (verdict, report) = approve(&mut s, &change, v);
        assert_eq!(verdict, refused("decision_unresolved"));
        assert!(!report.unwrap().accepted);
        created(&req(&mut s, "withdraw", (&change, false), json!({})));
        // an object the attached report did not name
        let (change, v) = submitted(&mut s, &v2(true, true, true));
        created(&req(
            &mut s,
            "add",
            ("Robot", true),
            json!({"serial": "R4", "grade": "A"}),
        ));
        assert_eq!(approve(&mut s, &change, v).0, refused("impact_unchanged"));
        assert_eq!(s.lib.version, 1);
        // a refresh attaches the report again, and the approval of it installs
        created(&req(&mut s, "refresh", (&change, false), json!({})));
        let v = s
            .snapshot()
            .unwrap()
            .whole(&change)
            .unwrap()
            .unwrap()
            .version;
        assert!(matches!(
            approve(&mut s, &change, v).0,
            Verdict::Satisfied { .. }
        ));
        assert_eq!(s.lib.version, 2);
        // a change drafted on version 2 that changes nothing installs version
        // 3, with no migration: version 2's text without its migration, which
        // maps what version 2 no longer has and the checks refuse
        let same = v2(true, true, true);
        let same = same.split("migration:").next().unwrap().to_string();
        let (change, v) = submitted(&mut s, &same);
        let (verdict, _) = approve(&mut s, &change, v);
        let Verdict::Satisfied { events, .. } = verdict else {
            panic!("{verdict:?}")
        };
        assert_eq!((events.len(), s.lib.version), (1, 3));
    }

    const SHOP_V1: &str = r#"
module: shop
categories: [live, closed]
enumerations:
  Tier: [GOLD, SILVER]
  Size: [S, L]
types:
  Customer:
    description: A customer.
    tracking: record
    attributes:
      name: { type: string, optional: true, personal: true }
      nick: { type: string, optional: true }
      tier: { type: Tier, optional: true }
      size: { type: Size, optional: true }
    states:
      ACTIVE: { category: live }
      GONE: { category: closed, final: true }
    transitions:
      join: { kind: initial, to: ACTIVE, optional_inputs: [name, nick, tier, size] }
      leave: { kind: external, from: [ACTIVE], to: GONE }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
  Coupon:
    description: A coupon.
    tracking: record
    attributes:
      code: { type: string }
    states:
      ISSUED: { category: live }
      USED: { category: closed, final: true }
    transitions:
      issue: { kind: initial, to: ISSUED, required_inputs: [code] }
      redeem: { kind: external, from: [ISSUED], to: USED }
"#;

    /// `tier` renamed, `size` an enumeration no more, `nick` personal and
    /// required by an invariant, `band` newly tracked, and `Coupon` dropped.
    const SHOP_V2: &str = r#"
module: shop
categories: [live, closed]
enumerations:
  Tier: [GOLD, SILVER]
  Band: [LOW, HIGH]
types:
  Customer:
    description: A customer.
    tracking: record
    attributes:
      name: { type: string, optional: true, personal: true }
      nick: { type: string, optional: true, personal: true }
      level: { type: Tier, optional: true }
      size: { type: string, optional: true }
      band: { type: Band, optional: true }
    states:
      ACTIVE: { category: live }
      GONE: { category: closed, final: true }
    invariants:
      nicknamed: { description: A rule., expression: nick is not null }
    transitions:
      join: { kind: initial, to: ACTIVE, optional_inputs: [name, nick, level, size, band] }
      leave: { kind: external, from: [ACTIVE], to: GONE }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
migration:
  description: What the new version does to the objects of the old.
  renamed_attributes:
    Customer: { tier: level }
"#;

    const LAB: &str = r#"
module: lab
categories: [live, closed]
enumerations:
  Grade: [A, B, OLD]
types:
  Sample:
    description: A sample.
    tracking: record
    attributes:
      label: { type: string }
    observations:
      checks:
        description: A check of a sample.
        kind: Check
        attributes:
          grade: { type: Grade }
    states:
      OPEN: { category: live }
      DONE: { category: closed, final: true }
    transitions:
      take: { kind: initial, to: OPEN, required_inputs: [label] }
      finish: { kind: external, from: [OPEN], to: DONE }
"#;

    /// Check 54 refuses a kind's invariant reading its subject, and this
    /// loads one without the checks, as `Library::load` does, to hold step
    /// 7's rule alone: a kind's invariants are checked on the recording.
    #[test]
    fn a_kinds_invariant_is_checked_on_the_recording_alone() {
        let text = LAB
            .replace(
                "          grade: { type: Grade }\n",
                "          grade: { type: Grade }\n        invariants:\n          labelled: { description: Its sample is labelled., expression: subject.label != \"\" }\n",
            )
            .replace(
                "      finish: { kind: external, from: [OPEN], to: DONE }\n",
                "      finish: { kind: external, from: [OPEN], to: DONE }\n      relabel: { kind: internal, from: [OPEN], required_inputs: [label] }\n",
            );
        let lib = Library::load(&[("lab.yaml".into(), text)]).unwrap();
        let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
        let sample = created(&req(
            &mut s,
            "take",
            ("Sample", true),
            json!({"label": "S1"}),
        ));
        let record = |s: &mut Store| {
            req(
                s,
                "record",
                ("Check", true),
                json!({"subject": &sample, "grade": "A"}),
            )
        };
        created(&record(&mut s));
        // a write the recorded check reads does not re-check it (DESIGN.md
        // §5.11, ADR-0146); a recording after it is checked
        created(&req(
            &mut s,
            "relabel",
            (&sample, false),
            json!({"label": ""}),
        ));
        assert!(matches!(
            record(&mut s),
            Verdict::InvariantViolated { failures, .. }
                if failures.len() == 1 && failures[0].invariant == "labelled"
        ));
    }

    #[test]
    /// Erasure reaches every value personal under any version an object's
    /// history was written under (ADR-0146 decision 2): after a version
    /// unmarks, drops or retypes a personal attribute, erasing a customer
    /// who joined before leaves its name in no row, kept or set-aside column
    /// or past event; and one who joined after the unmarking keeps a name
    /// that was never personal.
    fn a_version_unmarking_dropping_or_retyping_a_personal_value_leaves_it_erasable() {
        let unmarked = SHOP_V1.replace(
            "name: { type: string, optional: true, personal: true }",
            "name: { type: string, optional: true }",
        );
        let dropped = SHOP_V1
            .replace(
                "      name: { type: string, optional: true, personal: true }\n",
                "",
            )
            .replace(
                "optional_inputs: [name, nick, tier, size]",
                "optional_inputs: [nick, tier, size]",
            );
        let retyped = SHOP_V1.replace(
            "name: { type: string, optional: true, personal: true }",
            "name: { type: int, optional: true }",
        );
        for (what, text, column) in [
            ("unmarked", unmarked, "name"),
            ("dropped", dropped, "name"),
            ("retyped", retyped, "name__v1"),
        ] {
            let lib = Library::load(&[("shop.yaml".into(), SHOP_V1.into())]).unwrap();
            let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
            let ana = created(&req(
                &mut s,
                "join",
                ("Customer", true),
                json!({"name": "Ana", "nick": "a"}),
            ));
            let change = created(&req(
                &mut s,
                "draft",
                (crate::model::DECLARATION_CHANGE, true),
                json!({"source": {"shop": text}, "source_ref": "git:shop@1"}),
            ));
            created(&req(&mut s, "submit", (&change, false), json!({})));
            let v = s
                .snapshot()
                .unwrap()
                .whole(&change)
                .unwrap()
                .unwrap()
                .version;
            let (verdict, report) = approve(&mut s, &change, v);
            assert!(
                matches!(verdict, Verdict::Satisfied { .. }),
                "{what}: {verdict:?} {report:#?}"
            );
            assert_eq!(s.lib.version, 2, "{what}");
            created(&req(
                &mut s,
                "forget",
                (&ana, false),
                json!({"reason": "asked"}),
            ));
            let held: Option<String> = s
                .conn()
                .query_row(
                    &format!("SELECT {column} FROM t_customer WHERE id = ?1"),
                    [&ana],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(held, None, "{what}: the {column} column");
            let joined = s.history(&ana).unwrap().into_iter().next().unwrap();
            assert_eq!(
                joined.payload["inputs"]["name"],
                json!(null),
                "{what}: the join event"
            );
            assert_eq!(joined.payload["inputs"]["nick"], json!("a"), "{what}");
            if what == "unmarked" {
                // a name given under the version that unmarked it was never
                // personal, and an erasure leaves it
                let bo = created(&req(
                    &mut s,
                    "join",
                    ("Customer", true),
                    json!({"name": "Bo"}),
                ));
                created(&req(
                    &mut s,
                    "forget",
                    (&bo, false),
                    json!({"reason": "asked"}),
                ));
                assert_eq!(
                    s.snapshot()
                        .unwrap()
                        .whole(&bo)
                        .unwrap()
                        .unwrap()
                        .attr("name"),
                    Value::Text("Bo".into())
                );
            }
        }
    }

    /// The interval index too: a tracked personal enumeration's intervals
    /// are cleared by the version they were written under, whether a later
    /// version unmarked it, keeping its intervals, or retyped it, setting
    /// them aside as `tier__v1` (ADR-0146 decision 2, `storage-schema.md` §9
    /// step 9).
    #[test]
    fn an_erasure_clears_intervals_personal_under_the_version_they_were_written_under() {
        let v1 = SHOP_V1.replace(
            "tier: { type: Tier, optional: true }",
            "tier: { type: Tier, optional: true, personal: true }",
        );
        let unmarked = SHOP_V1.to_string();
        let retyped = SHOP_V1.replace(
            "tier: { type: Tier, optional: true }",
            "tier: { type: string, optional: true }",
        );
        for (what, text, dimension) in [
            ("unmarked", unmarked, "tier"),
            ("retyped", retyped, "tier__v1"),
        ] {
            let lib = Library::load(&[("shop.yaml".into(), v1.clone())]).unwrap();
            let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
            let ana = created(&req(
                &mut s,
                "join",
                ("Customer", true),
                json!({"tier": "GOLD"}),
            ));
            let change = created(&req(
                &mut s,
                "draft",
                (crate::model::DECLARATION_CHANGE, true),
                json!({"source": {"shop": text}, "source_ref": "git:shop@1"}),
            ));
            created(&req(&mut s, "submit", (&change, false), json!({})));
            let v = s
                .snapshot()
                .unwrap()
                .whole(&change)
                .unwrap()
                .unwrap()
                .version;
            assert!(
                matches!(approve(&mut s, &change, v).0, Verdict::Satisfied { .. }),
                "{what}"
            );
            let held = |s: &Store| -> Vec<Option<String>> {
                let mut q = s
                    .conn()
                    .prepare(
                        "SELECT value FROM of_interval WHERE object_id = ?1 AND dimension = ?2",
                    )
                    .unwrap();
                q.query_map(rusqlite::params![ana, dimension], |r| r.get(0))
                    .unwrap()
                    .map(Result::unwrap)
                    .collect()
            };
            assert!(
                held(&s).iter().any(Option::is_some),
                "{what}: a value before the erasure"
            );
            created(&req(
                &mut s,
                "forget",
                (&ana, false),
                json!({"reason": "asked"}),
            ));
            assert!(
                !held(&s).is_empty() && held(&s).iter().all(Option::is_none),
                "{what}: {:?}",
                held(&s)
            );
        }
        // a value given after the unmarking stays, and the one before it,
        // personal then, is cleared, though the current value is not erased
        let lib = Library::load(&[("shop.yaml".into(), v1)]).unwrap();
        let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
        let bo = created(&req(
            &mut s,
            "join",
            ("Customer", true),
            json!({"tier": "GOLD"}),
        ));
        let v2 = SHOP_V1.replace(
            "      leave: { kind: external, from: [ACTIVE], to: GONE }",
            "      leave: { kind: external, from: [ACTIVE], to: GONE }\n      retier: { kind: internal, from: [ACTIVE], required_inputs: [tier] }",
        );
        let change = created(&req(
            &mut s,
            "draft",
            (crate::model::DECLARATION_CHANGE, true),
            json!({"source": {"shop": v2}, "source_ref": "git:shop@2"}),
        ));
        created(&req(&mut s, "submit", (&change, false), json!({})));
        let v = s
            .snapshot()
            .unwrap()
            .whole(&change)
            .unwrap()
            .unwrap()
            .version;
        assert!(matches!(
            approve(&mut s, &change, v).0,
            Verdict::Satisfied { .. }
        ));
        s.now += 60;
        created(&req(
            &mut s,
            "retier",
            (&bo, false),
            json!({"tier": "SILVER"}),
        ));
        created(&req(
            &mut s,
            "forget",
            (&bo, false),
            json!({"reason": "asked"}),
        ));
        let mut q = s
            .conn()
            .prepare("SELECT value FROM of_interval WHERE object_id = ?1 AND dimension = 'tier' ORDER BY entered_at")
            .unwrap();
        let values: Vec<Option<String>> = q
            .query_map([&bo], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(values, vec![None, Some("SILVER".to_string())]);
    }

    /// A personal attribute renamed by one version and unmarked by the
    /// next is still erased from objects whose value was written before the
    /// rename, its name taken back through the rename (ADR-0146 decision 2).
    #[test]
    fn an_erasure_follows_a_personal_attribute_through_its_renames() {
        let lib = Library::load(&[("shop.yaml".into(), SHOP_V1.into())]).unwrap();
        let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
        let ana = created(&req(
            &mut s,
            "join",
            ("Customer", true),
            json!({"name": "Ana"}),
        ));
        let renamed = SHOP_V1
            .replace(
                "name: { type: string, optional: true, personal: true }",
                "alias: { type: string, optional: true, personal: true }",
            )
            .replace(
                "optional_inputs: [name, nick, tier, size]",
                "optional_inputs: [alias, nick, tier, size]",
            )
            + "migration:\n  description: The name is an alias.\n  renamed_attributes:\n    Customer: { name: alias }\n";
        let unmarked = renamed
            .replace(
                "alias: { type: string, optional: true, personal: true }",
                "alias: { type: string, optional: true }",
            )
            .replace(
                "migration:\n  description: The name is an alias.\n  renamed_attributes:\n    Customer: { name: alias }\n",
                "",
            );
        for (n, text) in [(1, renamed), (2, unmarked)] {
            let change = created(&req(
                &mut s,
                "draft",
                (crate::model::DECLARATION_CHANGE, true),
                json!({"source": {"shop": text}, "source_ref": format!("git:shop@{n}")}),
            ));
            created(&req(&mut s, "submit", (&change, false), json!({})));
            let v = s
                .snapshot()
                .unwrap()
                .whole(&change)
                .unwrap()
                .unwrap()
                .version;
            let (verdict, report) = approve(&mut s, &change, v);
            assert!(
                matches!(verdict, Verdict::Satisfied { .. }),
                "{verdict:?} {report:#?}"
            );
        }
        assert_eq!(s.lib.version, 3);
        let w = s.snapshot().unwrap();
        let held = w.whole(&ana).unwrap().unwrap();
        assert_eq!(held.attr("alias"), Value::Text("Ana".into()));
        // the write index follows the rename, so `changed_since` reads it
        assert!(
            held.held_parts().unwrap().writes.contains_key("alias")
                && !held.held_parts().unwrap().writes.contains_key("name")
        );
        drop(w);
        created(&req(
            &mut s,
            "forget",
            (&ana, false),
            json!({"reason": "asked"}),
        ));
        assert_eq!(
            s.snapshot()
                .unwrap()
                .whole(&ana)
                .unwrap()
                .unwrap()
                .attr("alias"),
            Value::Absent
        );
    }

    #[test]
    fn no_migration_reaches_an_observation() {
        let lib = Library::load(&[("lab.yaml".into(), LAB.into())]).unwrap();
        let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
        let sample = created(&req(
            &mut s,
            "take",
            ("Sample", true),
            json!({"label": "S1"}),
        ));
        let check = created(&req(
            &mut s,
            "record",
            ("Check", true),
            json!({"subject": &sample, "grade": "OLD"}),
        ));
        let lab = |members: &str, migration: &str| {
            LAB.replace("Grade: [A, B, OLD]", members).replace(
                "          grade: { type: Grade }\n",
                "          grade: { type: Grade }\n        invariants:\n          graded_a: { description: A check grades A., expression: grade == Grade.A }\n",
            ) + migration
        };
        // a removed member an observation holds is fatal, a mapping or not
        let text = lab(
            "Grade: [A, B]",
            "migration:\n  description: The lab without OLD.\n  removed_members:\n    Grade: { OLD: B }\n",
        );
        let change = created(&req(
            &mut s,
            "draft",
            (crate::model::DECLARATION_CHANGE, true),
            json!({"source": {"lab": &text}, "source_ref": "git:lab@1"}),
        ));
        let r = run_on(&s, &change, "lab", &text);
        assert_eq!(ids(&r, "removed_member:Grade.OLD"), vec![check.clone()]);
        assert!(ids(&r, "mapping:removed_members:Grade.OLD").is_empty());
        // a kind's new invariant the observation violates is a notice
        let text = lab("Grade: [A, B, OLD]", "");
        let r = run_on(&s, &change, "lab", &text);
        assert!(r.accepted, "{:#?}", r.findings);
        assert_eq!(
            ids(&r, "observation_invariant:Check.graded_a"),
            vec![check.clone()]
        );
        // published, it binds later recordings and no request reads the
        // observation that violates it (D532)
        let change = created(&req(
            &mut s,
            "draft",
            (crate::model::DECLARATION_CHANGE, true),
            json!({"source": {"lab": &text}, "source_ref": "git:lab@2"}),
        ));
        created(&req(&mut s, "submit", (&change, false), json!({})));
        let v = s
            .snapshot()
            .unwrap()
            .whole(&change)
            .unwrap()
            .unwrap()
            .version;
        assert!(matches!(
            approve(&mut s, &change, v).0,
            Verdict::Satisfied { .. }
        ));
        created(&req(
            &mut s,
            "take",
            ("Sample", true),
            json!({"label": "S2"}),
        ));
        assert!(matches!(
            req(
                &mut s,
                "record",
                ("Check", true),
                json!({"subject": &sample, "grade": "OLD"})
            ),
            Verdict::InvariantViolated { failures, .. }
                if failures.len() == 1 && failures[0].invariant == "graded_a" && failures[0].object_id.as_deref() != Some(check.as_str())
        ));
    }

    #[test]
    fn an_approval_keeps_what_a_version_drops_as_history() {
        let lib = Library::load(&[("shop.yaml".into(), SHOP_V1.into())]).unwrap();
        let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
        let join =
            |s: &mut Store, inputs: Json| created(&req(s, "join", ("Customer", true), inputs));
        let erased = join(
            &mut s,
            json!({"name": "N", "nick": "a", "tier": "GOLD", "size": "S"}),
        );
        let kept = join(&mut s, json!({"nick": "b", "tier": "SILVER", "size": "L"}));
        let coupon = created(&req(
            &mut s,
            "issue",
            ("Coupon", true),
            json!({"code": "C1"}),
        ));
        created(&req(
            &mut s,
            "forget",
            (&erased, false),
            json!({"reason": "asked"}),
        ));
        let change = created(&req(
            &mut s,
            "draft",
            (crate::model::DECLARATION_CHANGE, true),
            json!({"source": {"shop": SHOP_V2}, "source_ref": "git:shop@1"}),
        ));
        created(&req(&mut s, "submit", (&change, false), json!({})));
        let v = s
            .snapshot()
            .unwrap()
            .whole(&change)
            .unwrap()
            .unwrap()
            .version;
        let (verdict, report) = approve(&mut s, &change, v);
        let Verdict::Satisfied { events, .. } = verdict else {
            panic!("{verdict:?} {report:#?}")
        };
        // the publish, and the erased customer's redaction; a rename and a
        // type change write no event
        assert_eq!(events.len(), 2);
        let w = s.snapshot().unwrap();
        let k = w.whole(&kept).unwrap().unwrap();
        let tier = |m: &str| Value::Member {
            enumeration: "Tier".into(),
            member: m.into(),
        };
        assert_eq!(k.attr("level"), tier("SILVER"));
        assert_eq!(k.attr("size"), Value::Absent);
        assert_eq!(k.attr("nick"), Value::Text("b".into()));
        // the renamed attribute's intervals carried, the newly tracked one
        // opened at the publish event, marked migrated
        assert_eq!(
            k.held_parts().unwrap().spans["level"][0].value,
            tier("SILVER")
        );
        assert!(
            !k.held_parts().unwrap().spans.contains_key("tier")
                && !k.held_parts().unwrap().spans.contains_key("size")
        );
        let band = &k.held_parts().unwrap().spans["band"][0];
        assert!(band.migrated && band.value == Value::Absent);
        assert_eq!(band.entered_position, events[0]);
        // the newly personal nick redacted on the erased customer, its
        // history included, and the invariant its absence breaks admitted
        let e = w.whole(&erased).unwrap().unwrap();
        assert_eq!(e.attr("nick"), Value::Absent);
        assert!(e.held_parts().unwrap().admissions.contains("nicknamed"));
        let history = s.history(&erased).unwrap();
        assert_eq!(history[0].payload["inputs"]["nick"], Json::Null);
        let redact = history.last().unwrap();
        assert_eq!(
            (redact.transition.as_str(), &redact.payload),
            ("redact", &json!({"redacted": ["nick"]}))
        );
        // the dropped type's object stays readable, and nothing creates one
        assert_eq!(w.whole(&coupon).unwrap().unwrap().state, "ISSUED");
        drop(w);
        assert!(!matches!(
            req(&mut s, "issue", ("Coupon", true), json!({"code": "C2"})),
            Verdict::Satisfied { .. }
        ));
        // the type-changed column and its intervals set aside, closed
        let (aside, open): (String, i64) = s
            .conn()
            .query_row(
                "SELECT (SELECT size__v1 FROM t_customer WHERE id = ?1), \
                 (SELECT count(*) FROM of_interval WHERE dimension = 'size__v1' AND left_position IS NULL)",
                [&kept],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((aside.as_str(), open), ("L", 0));
        // and the version's requests run over it
        created(&req(
            &mut s,
            "join",
            ("Customer", true),
            json!({"nick": "c", "level": "GOLD", "size": "XL", "band": "LOW"}),
        ));
    }

    /// A draft's evidence (ADR-0097 §5, ADR-0159): `draft` takes the
    /// references it cites, each a read's arguments, refusing one of another
    /// form; `submit` and `refresh` keep each read as it answered, a refusal
    /// as a value; a `revise` giving none keeps the citations and drops what a
    /// submission read; and a cited diagnostics read is not run.
    #[test]
    fn a_drafts_evidence_is_cited_then_read_at_submission() {
        let (mut s, _, _, _) = fleet();
        let refuse = |s: &mut Store| {
            assert!(!matches!(
                req(s, "add", ("Robot", true), json!({})),
                Verdict::Satisfied { .. }
            ));
        };
        refuse(&mut s);
        let text = v2(true, true, true);
        let draft = |s: &mut Store, evidence: Json| {
            req(
                s,
                "draft",
                (crate::model::DECLARATION_CHANGE, true),
                json!({"source": {"fleet": text}, "source_ref": "git:fleet@1", "evidence": evidence}),
            )
        };
        // a reference not of its form is refused, the draft with it
        for wrong in [
            json!("metric(Robot.refusals)"),
            json!([{"metric": ""}]),
            json!([{"metric": "Robot.refusals", "bogus": 1}]),
            json!([{"metric": "Robot.refusals", "over": "forever"}]),
            json!([{"metric": "Robot.refusals", "keep": "transition"}]),
            json!([{"diagnostics": "Robot"}]),
        ] {
            assert_eq!(
                draft(&mut s, wrong.clone()),
                Verdict::invalid_input([("evidence".to_string(), InputReason::Malformed)]),
                "{wrong}"
            );
        }
        // a window the metric cannot take, a read it can, and a metric it
        // does not declare
        let cites = json!([
            {"metric": "Robot.refusals", "keep": ["transition"], "over": "30 days"},
            {"metric": "Robot.refusals", "keep": ["transition"]},
            {"metric": "nope"}
        ]);
        let change = created(&draft(&mut s, cites.clone()));
        let evidence = |s: &Store| -> Json {
            match s
                .snapshot()
                .unwrap()
                .whole(&change)
                .unwrap()
                .unwrap()
                .attr("evidence")
            {
                Value::Text(t) => serde_json::from_str(&t).unwrap(),
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(evidence(&s), json!({ "cites": cites }));
        assert_eq!(s.history(&change).unwrap()[0].payload["evidence"], cites);
        // submitted, each reference is read as it then stands
        created(&req(&mut s, "submit", (&change, false), json!({})));
        let read = evidence(&s)["read"].clone();
        assert_eq!(read[0]["reference"], cites[0]);
        assert_eq!(read[0]["refused"], json!("no_window"));
        assert_eq!(
            read[1]["rows"][0]["dimensions"],
            json!({"transition": "add"})
        );
        assert_eq!(read[1]["rows"][0]["value"], json!(1));
        assert_eq!(read[1]["as_of"], json!("2026-01-01T00:00:00Z"));
        assert_eq!(read[2]["refused"], json!("unknown_metric"));
        // a refresh reads again
        refuse(&mut s);
        created(&req(&mut s, "refresh", (&change, false), json!({})));
        assert_eq!(evidence(&s)["read"][1]["rows"][0]["value"], json!(2));
        // a revise giving none keeps the citations, and not the reads
        created(&req(&mut s, "reopen", (&change, false), json!({})));
        created(&req(
            &mut s,
            "revise",
            (&change, false),
            json!({"source": {"fleet": text}, "source_ref": "git:fleet@2"}),
        ));
        assert_eq!(evidence(&s), json!({ "cites": cites }));
        // one giving a diagnostics read reads it at submission (ADR-0160)
        let diagnostics = json!([{"diagnostics": "Robot", "over": "7 days"}]);
        created(&req(
            &mut s,
            "revise",
            (&change, false),
            json!({"source": {"fleet": text}, "source_ref": "git:fleet@3", "evidence": diagnostics}),
        ));
        assert_eq!(evidence(&s), json!({ "cites": diagnostics }));
        created(&req(&mut s, "submit", (&change, false), json!({})));
        let read = evidence(&s)["read"][0].clone();
        assert_eq!(
            (&read["reference"], &read["refused"]),
            (&diagnostics[0], &Json::Null)
        );
        assert!(
            read["diagnostics"]
                .as_array()
                .is_some_and(|d| !d.is_empty())
        );
    }

    /// A version's source is the whole version (ADR-0161): a module
    /// importing from a module the version does not carry is refused as a
    /// fatal finding, where the checker on a module alone only notices it.
    #[test]
    fn a_version_importing_from_a_module_it_does_not_carry_is_refused() {
        let (mut s, _, _, _) = fleet();
        let text = v2(true, true, true).replacen(
            "module: fleet\n",
            "module: fleet\nimports: { gone: [Thing] }\n",
            1,
        );
        assert!(text.contains("imports: { gone: [Thing] }"));
        let change = draft(&mut s, &text);
        let r = run(&s, &change, &text);
        let unresolved = r
            .findings
            .iter()
            .find(|f| f.code == "check:imports")
            .unwrap_or_else(|| panic!("{:#?}", r.findings));
        assert_eq!(unresolved.severity, Severity::Fatal);
        assert!(
            unresolved.message.contains("gone"),
            "{}",
            unresolved.message
        );
        assert!(r.fatal() && !r.accepted);
        // without the import the same version is accepted
        let clean = v2(true, true, true);
        let change = draft(&mut s, &clean);
        assert!(run(&s, &change, &clean).accepted);
    }

    /// `declaration` at an earlier version (`library-api.md` §6): each
    /// published version's library loads again from the source it recorded,
    /// the built-ins alone at version 0, and a version never published has
    /// none.
    #[test]
    fn an_earlier_versions_library_loads_from_its_source() {
        let (mut s, _, _, _) = fleet();
        let (change, v) = submitted(&mut s, &v2(true, true, true));
        assert!(matches!(
            approve(&mut s, &change, v).0,
            Verdict::Satisfied { .. }
        ));
        let first = s.library_at(1).unwrap().unwrap();
        let robot = first.type_("Robot").unwrap();
        assert_eq!(first.version, 1);
        assert!(robot.state("HOLD").is_some() && robot.transition("hold").is_some());
        assert!(robot.attribute("site").is_none());
        let second = s.library_at(2).unwrap().unwrap();
        let robot = second.type_("Robot").unwrap();
        assert_eq!(second.version, 2);
        assert!(robot.state("HOLD").is_none() && robot.attribute("site").is_some());
        let builtins = s.library_at(0).unwrap().unwrap();
        assert!(builtins.type_("Robot").is_none() && builtins.type_("Operator").is_some());
        assert!(s.library_at(9).unwrap().is_none());
    }
}

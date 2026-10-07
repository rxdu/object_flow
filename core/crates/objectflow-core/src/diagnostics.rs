//! `diagnostics(type, window)`: where a flow and reality disagree
//! (`DESIGN.md` §10, ADR-0085), each kind ADR-0160 decided, in its order, a
//! kind's rows in descending measure and then by subject. It is computed
//! like the standard metrics it rests on, over the window ending now: events
//! and intervals by occurred time, attempt rows by recorded time.

use std::collections::{BTreeMap, BTreeSet};

use crate::eval::{NotRun, transition_row};
use crate::model::LABEL;
use crate::value::{Decimal, Value};
use crate::world::World;

/// One place a flow and reality disagree (`library-api.md` §5): its kind,
/// what it concerns, the count, duration or share that put it on the list,
/// and whether its subject names an actor, as a marked dimension does.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub kind: String,
    pub subject: String,
    pub measure: Value,
    pub names_actor: bool,
}

impl Diagnostic {
    /// As the wire and a draft's evidence give it.
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind,
            "subject": self.subject,
            "measure": crate::wire::encode(&self.measure),
            "names_actor": self.names_actor,
        })
    }
}

/// A row being gathered: its subject, its measure, and the key it sorts by,
/// thousandths of the measure.
type Row = (String, Value, i128);

/// The diagnostics of a type's family over a window, in seconds; a type
/// the store does not declare is refused, a value.
pub fn diagnostics(
    world: &World<'_>,
    type_: &str,
    window: i64,
) -> Result<Result<Vec<Diagnostic>, &'static str>, NotRun> {
    let lib = world.lib;
    let Some(t) = lib.type_(type_) else {
        return Ok(Err("unknown_type"));
    };
    let since = world.now - window;
    let within = |at: i64| at >= since && at <= world.now;
    let objects = world.of_type(type_)?;
    // their flow data, which the waits, the entries and the assignees read
    world.with_parts(&objects)?;
    let family: BTreeSet<&str> = objects.iter().map(|o| o.id.as_str()).collect();
    // each of the family's events in the window, as the transitions source
    // reads it
    let mut rows: Vec<BTreeMap<String, Value>> = Vec::new();
    for e in world.log()? {
        if !family.contains(e.object_id.as_str()) || !within(e.occurred_at.unwrap_or(e.recorded_at))
        {
            continue;
        }
        let Some(o) = world.get(&e.object_id)? else {
            continue;
        };
        let Some(ot) = lib.type_(&o.type_) else {
            continue;
        };
        if let Value::Row(f) = transition_row(world, o, world.parts(o)?, ot, e) {
            rows.push(std::sync::Arc::unwrap_or_clone(f));
        }
    }
    let flag = |f: &BTreeMap<String, Value>, m: &str| f.get(m) == Some(&Value::Bool(true));
    let text = |v: Option<&Value>| match v {
        Some(Value::Text(s)) => s.clone(),
        Some(Value::State { name, .. }) => name.clone(),
        Some(Value::Object(id)) => id.clone(),
        _ => String::new(),
    };
    // a request's own events, not a port's, a migration's or a redaction's
    let recorded: Vec<&BTreeMap<String, Value>> = rows
        .iter()
        .filter(|f| !flag(f, "imported") && !flag(f, "migrated") && !flag(f, "redacted"))
        .collect();
    let mut out = Vec::new();
    let mut emit = |kind: &str, mut rows: Vec<Row>, names_actor: bool| {
        rows.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
        for (subject, measure, _) in rows {
            out.push(Diagnostic {
                kind: kind.to_string(),
                subject,
                measure,
                names_actor,
            });
        }
    };
    let count = |n: i64| (Value::Int(n), i128::from(n) * 1000);
    let share = |part: i64, whole: i64| {
        let milli = (i128::from(part) * 1000 + i128::from(whole) / 2) / i128::from(whole.max(1));
        (
            Decimal::new(milli, 3).map_or(Value::Absent, Value::Decimal),
            milli,
        )
    };
    // transitions and states never used
    let taken: BTreeSet<String> = rows.iter().map(|f| text(f.get("transition"))).collect();
    emit(
        "unused_transition",
        t.transitions
            .iter()
            .filter(|(xn, _)| !taken.contains(xn.as_str()))
            .map(|(xn, _)| (xn.clone(), Value::Int(0), 0))
            .collect(),
        false,
    );
    let mut entered: BTreeSet<String> = BTreeSet::new();
    for o in &objects {
        for sp in world.parts(o)?.spans.get("state").into_iter().flatten() {
            if within(sp.entered_at) {
                entered.insert(text(Some(&sp.value)));
            }
        }
    }
    let held_now: BTreeSet<&str> = objects.iter().map(|o| o.state.as_str()).collect();
    emit(
        "unused_state",
        t.states
            .iter()
            .filter(|(s, _)| !entered.contains(s.as_str()) && !held_now.contains(s.as_str()))
            .map(|(s, _)| (s.clone(), Value::Int(0), 0))
            .collect(),
        false,
    );
    // the clauses that refuse most, by remedy and kind of actor
    let attempts: Vec<&crate::store::Attempt> = world
        .attempts()?
        .iter()
        .filter(|a| a.type_.as_deref().is_some_and(|at| lib.is_a(at, type_)) && within(a.at))
        .collect();
    let mut refusals: BTreeMap<String, i64> = BTreeMap::new();
    for a in attempts.iter().filter(|a| a.enforced) {
        let clause = a.clause.clone().unwrap_or_else(|| a.verdict.clone());
        *refusals
            .entry(format!("{clause}, {}, {}", a.remedy, a.actor_kind))
            .or_default() += 1;
    }
    emit(
        "top_refusal",
        refusals
            .into_iter()
            .map(|(s, n)| {
                let (m, k) = count(n);
                (s, m, k)
            })
            .collect(),
        false,
    );
    // each state's entries by route, and those entered mostly by override
    let mut routes: BTreeMap<String, [i64; 3]> = BTreeMap::new();
    for f in &recorded {
        let creation = f.get("from_state").is_none_or(Value::is_absent);
        let changes = text(f.get("from_state")) != text(f.get("to_state"));
        if !creation && !changes {
            continue;
        }
        let route = if flag(f, "overrides") {
            1
        } else if creation {
            2
        } else {
            0
        };
        routes.entry(text(f.get("to_state"))).or_default()[route] += 1;
    }
    emit(
        "state_entries",
        routes
            .iter()
            .map(|(s, [by_transition, by_override, by_creation])| {
                let mut m = BTreeMap::new();
                m.insert("transition".to_string(), Value::Int(*by_transition));
                m.insert("override".to_string(), Value::Int(*by_override));
                m.insert("creation".to_string(), Value::Int(*by_creation));
                (s.clone(), Value::row(m), i128::from(*by_override) * 1000)
            })
            .collect(),
        false,
    );
    emit(
        "mostly_override",
        routes
            .iter()
            .filter(|(_, r)| r[1] * 2 > r.iter().sum::<i64>())
            .map(|(s, r)| {
                let (m, k) = share(r[1], r.iter().sum());
                (s.clone(), m, k)
            })
            .collect(),
        false,
    );
    // labels by frequency, and by the state their subject was in
    let labels = world.of_type(LABEL)?;
    let corrected: BTreeSet<String> = labels
        .iter()
        .filter_map(|l| match l.attr("corrects") {
            Value::Object(id) => Some(id),
            _ => None,
        })
        .collect();
    let mut by_name: BTreeMap<String, i64> = BTreeMap::new();
    let mut by_state: BTreeMap<String, i64> = BTreeMap::new();
    for l in labels {
        let on_family =
            matches!(l.attr("subject"), Value::Object(s) if family.contains(s.as_str()));
        let at = match l.attr("recorded_at") {
            Value::Timestamp(t) => t,
            _ => l.created_at,
        };
        if !on_family || !within(at) || corrected.contains(&l.id) {
            continue;
        }
        let name = text(Some(&l.attr("name")));
        *by_state
            .entry(format!("{name}, {}", text(Some(&l.attr("subject_state")))))
            .or_default() += 1;
        *by_name.entry(name).or_default() += 1;
    }
    for (kind, counted) in [("label_frequency", by_name), ("label_by_state", by_state)] {
        emit(
            kind,
            counted
                .into_iter()
                .map(|(s, n)| {
                    let (m, k) = count(n);
                    (s, m, k)
                })
                .collect(),
            false,
        );
    }
    // the states where work waits longest: each one's median interval
    // entered in the window, an open one counting to now, the lower middle
    // (ADR-0147)
    let mut waits: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for o in &objects {
        for sp in world.parts(o)?.spans.get("state").into_iter().flatten() {
            if within(sp.entered_at) {
                waits
                    .entry(text(Some(&sp.value)))
                    .or_default()
                    .push(sp.left_at.unwrap_or(world.now) - sp.entered_at);
            }
        }
    }
    emit(
        "longest_wait",
        waits
            .into_iter()
            .map(|(s, mut d)| {
                d.sort_unstable();
                let median = d[d.len().div_ceil(2) - 1];
                (s, Value::Duration(median), i128::from(median) * 1000)
            })
            .collect(),
        false,
    );
    // rework: re-entries of a state left before
    let mut rework: BTreeMap<String, i64> = BTreeMap::new();
    for f in recorded.iter().filter(|f| flag(f, "returns")) {
        *rework.entry(text(f.get("to_state"))).or_default() += 1;
    }
    emit(
        "rework",
        rework
            .into_iter()
            .map(|(s, n)| {
                let (m, k) = count(n);
                (s, m, k)
            })
            .collect(),
        false,
    );
    // reassignment back, and work done by someone other than the assignee
    let assignee = t
        .attributes
        .iter()
        .find(|(_, spec)| spec.assignee)
        .map(|(a, _)| a.clone());
    let mut back: BTreeMap<String, i64> = BTreeMap::new();
    let mut acted: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    if let Some(a) = &assignee {
        for o in &objects {
            let spans = world.parts(o)?.spans.get(a).map_or(&[][..], Vec::as_slice);
            for (i, sp) in spans.iter().enumerate() {
                if within(sp.entered_at)
                    && !sp.value.is_absent()
                    && spans[..i.saturating_sub(1)]
                        .iter()
                        .any(|before| before.value == sp.value)
                {
                    *back.entry(text(Some(&sp.value))).or_default() += 1;
                }
            }
        }
        for f in &recorded {
            let held = match f.get("held") {
                Some(Value::Row(h)) => h.get(a).cloned(),
                _ => None,
            };
            let Some(Value::Object(holder)) = held else {
                continue;
            };
            // an event names its actor by identity, as the assignee's own
            // `actor_id` does (declaration-syntax.md §6.11)
            let identity = world.get(&holder)?.and_then(|h| {
                let (attr, _) = lib.type_(&h.type_)?.actor_identity()?;
                match h.attr(attr) {
                    Value::Text(i) => Some(i),
                    _ => None,
                }
            });
            let entry = acted.entry(text(f.get("transition"))).or_default();
            entry.1 += 1;
            if identity.as_deref() != Some(text(f.get("actor_id")).as_str()) {
                entry.0 += 1;
            }
        }
    }
    emit(
        "reassigned_back",
        back.into_iter()
            .map(|(s, n)| {
                let (m, k) = count(n);
                (s, m, k)
            })
            .collect(),
        true,
    );
    emit(
        "acted_by_non_assignee",
        acted
            .into_iter()
            .filter(|(_, (by_other, _))| *by_other > 0)
            .map(|(s, (by_other, all))| {
                let (m, k) = share(by_other, all);
                (s, m, k)
            })
            .collect(),
        false,
    );
    // how often each transition is backdated
    let mut dated: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for f in &recorded {
        let (Some(Value::Timestamp(occurred)), Some(Value::Timestamp(recorded_at))) =
            (f.get("occurred_at"), f.get("recorded_at"))
        else {
            continue;
        };
        let entry = dated.entry(text(f.get("transition"))).or_default();
        entry.1 += 1;
        if occurred < recorded_at {
            entry.0 += 1;
        }
    }
    emit(
        "backdating_rate",
        dated
            .into_iter()
            .map(|(s, (late, all))| {
                let (m, k) = share(late, all);
                (s, m, k)
            })
            .collect(),
        false,
    );
    // each observing clause's would-be refusals, and the flags raised most
    let mut observed: BTreeMap<String, i64> = BTreeMap::new();
    let mut flags: BTreeMap<String, i64> = BTreeMap::new();
    for a in attempts.iter().filter(|a| !a.enforced) {
        let clause = a.clause.clone().unwrap_or_default();
        *if a.flagged { &mut flags } else { &mut observed }
            .entry(clause)
            .or_default() += 1;
    }
    for (kind, counted) in [("would_be_refusal", observed), ("top_flag", flags)] {
        emit(
            kind,
            counted
                .into_iter()
                .map(|(s, n)| {
                    let (m, k) = count(n);
                    (s, m, k)
                })
                .collect(),
            false,
        );
    }
    // a file deletion still pending: the core holds no files yet
    Ok(Ok(out))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::model::{Library, OPERATOR};
    use crate::request::{Request, request};
    use crate::store::Store;
    use crate::verdict::Verdict;
    use serde_json::{Value as Json, json};

    const DESK: &str = r#"
module: desk
categories: [live, closed]
enumerations: { Why: [MISKEYED] }
types:
  User:
    attributes:
      login: { type: identity, actor_kind: human }
    states:
      ACTIVE: { category: live }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [login] }
  Ticket:
    attributes:
      engineer: { reference: User, optional: true, assignee: true }
      size: { type: int, optional: true }
    states:
      NEW: { category: live }
      WORKING: { category: live }
      PARKED: { category: live }
      LIMBO: { category: live }
      ONHOLD: { category: live }
      DONE: { category: closed, final: true }
    conditions:
      small: { expression: size is null or size < 10, remedy: self_serviceable }
      tidy: { expression: size is null or size < 5, remedy: self_serviceable }
      sized: { expression: size is not null, remedy: self_serviceable }
    transitions:
      open: { kind: initial, to: NEW, optional_inputs: [engineer, size] }
      start: { kind: external, from: [NEW, PARKED], to: WORKING, guards: { small: deny, tidy: audit, sized: warn }, backdating_limit: 1 day }
      park: { kind: external, from: [WORKING], to: PARKED }
      assign: { kind: internal, from: [NEW, WORKING, PARKED], required_inputs: [engineer] }
      finish: { kind: external, from: [WORKING], to: DONE }
      archive: { kind: internal, from: [DONE] }
      force: { kind: assertion, to: [WORKING, DONE, ONHOLD], inputs: { reason: { type: Why } } }
"#;

    fn ask(
        s: &mut Store,
        actor: &str,
        xn: &str,
        target: (&str, bool),
        inputs: Json,
        occurred: Option<i64>,
    ) -> Verdict {
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
            occurred_at: occurred,
            propose: false,
            expires_at: None,
            expected_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
        };
        request(s, &r).unwrap().unwrap()
    }

    fn made(v: Verdict) -> String {
        match v {
            Verdict::Satisfied { object_id, .. } => object_id,
            other => panic!("{other:?}"),
        }
    }

    /// `diagnostics` (`DESIGN.md` §10, ADR-0085, ADR-0160): each kind, in
    /// its order, its rows by descending measure then subject, over the
    /// window; a type it does not declare refused, a value.
    #[test]
    fn diagnostics_report_where_the_flow_and_reality_disagree() {
        let lib = Library::load(&[("desk.yaml".into(), DESK.into())]).unwrap();
        let mut s = Store::sandbox(lib, 10 * 86_400).unwrap();
        let ana = made(ask(
            &mut s,
            OPERATOR,
            "add",
            ("User", true),
            json!({"login": "ana"}),
            None,
        ));
        let ben = made(ask(
            &mut s,
            OPERATOR,
            "add",
            ("User", true),
            json!({"login": "ben"}),
            None,
        ));
        let open = |s: &mut Store, inputs: Json| {
            made(ask(s, OPERATOR, "open", ("Ticket", true), inputs, None))
        };
        // t1: started by its engineer, parked by another, started again,
        // handed to ben and back to ana, finished
        let t1 = open(&mut s, json!({"engineer": ana, "size": 3}));
        made(ask(&mut s, "ana", "start", (&t1, false), json!({}), None));
        made(ask(&mut s, OPERATOR, "park", (&t1, false), json!({}), None));
        made(ask(&mut s, "ana", "start", (&t1, false), json!({}), None));
        made(ask(
            &mut s,
            OPERATOR,
            "assign",
            (&t1, false),
            json!({"engineer": ben}),
            None,
        ));
        made(ask(
            &mut s,
            OPERATOR,
            "assign",
            (&t1, false),
            json!({"engineer": ana}),
            None,
        ));
        made(ask(&mut s, "ana", "finish", (&t1, false), json!({}), None));
        // t2 refused twice; t3 audited and backdated an hour; t4 flagged
        let t2 = open(&mut s, json!({"size": 20}));
        for _ in 0..2 {
            assert!(matches!(
                ask(&mut s, OPERATOR, "start", (&t2, false), json!({}), None),
                Verdict::Unsatisfied { .. }
            ));
        }
        let t3 = open(&mut s, json!({"size": 7}));
        s.now += 2 * 3600;
        let an_hour_ago = s.now - 3600;
        made(ask(
            &mut s,
            OPERATOR,
            "start",
            (&t3, false),
            json!({}),
            Some(an_hour_ago),
        ));
        let t4 = open(&mut s, json!({}));
        made(ask(
            &mut s,
            OPERATOR,
            "start",
            (&t4, false),
            json!({}),
            None,
        ));
        // DONE entered as often by override as by transition, which is not
        // mostly; ONHOLD by override alone, which is
        made(ask(
            &mut s,
            OPERATOR,
            "finish",
            (&t4, false),
            json!({}),
            None,
        ));
        for to in ["DONE", "DONE", "ONHOLD"] {
            let t = open(&mut s, json!({}));
            made(ask(
                &mut s,
                OPERATOR,
                "force",
                (&t, false),
                json!({"to": to, "reason": "MISKEYED"}),
                None,
            ));
        }
        // labels
        for (subject, name) in [(&t2, "vip"), (&t2, "urgent")] {
            made(ask(
                &mut s,
                OPERATOR,
                "record",
                (crate::model::LABEL, true),
                json!({"subject": subject, "name": name}),
                None,
            ));
        }
        // a misspelt label, corrected: only the correction counts
        let typo = made(ask(
            &mut s,
            OPERATOR,
            "record",
            (crate::model::LABEL, true),
            json!({"subject": t3, "name": "VPI"}),
            None,
        ));
        made(ask(
            &mut s,
            OPERATOR,
            "record",
            (crate::model::LABEL, true),
            json!({"subject": t3, "name": "VIP", "corrects": typo}),
            None,
        ));
        let w = s.snapshot().unwrap();
        let found = diagnostics(&w, "Ticket", 86_400).unwrap().unwrap();
        let rows = |kind: &str| -> Vec<(String, Json)> {
            found
                .iter()
                .filter(|d| d.kind == kind)
                .map(|d| (d.subject.clone(), crate::wire::encode(&d.measure)))
                .collect()
        };
        let subjects =
            |kind: &str| -> Vec<String> { rows(kind).into_iter().map(|(s, _)| s).collect() };
        assert_eq!(subjects("unused_transition"), ["archive"]);
        assert_eq!(subjects("unused_state"), ["LIMBO"]);
        assert_eq!(
            rows("top_refusal"),
            [("small, self_serviceable, human".to_string(), json!(2))]
        );
        assert_eq!(
            rows("state_entries")[0],
            (
                "DONE".to_string(),
                json!({"creation": 0, "override": 2, "transition": 2})
            )
        );
        assert_eq!(
            rows("mostly_override"),
            [("ONHOLD".to_string(), json!("1.000"))]
        );
        assert_eq!(
            rows("label_frequency"),
            [
                ("vip".to_string(), json!(2)),
                ("urgent".to_string(), json!(1))
            ]
        );
        assert_eq!(
            subjects("label_by_state"),
            ["urgent, NEW", "vip, NEW", "vip, WORKING"]
        );
        // each state's median wait, the lower middle: NEW waited 0, 0, 0,
        // 0, 0, an hour and two, and its median is no wait at all
        let none = crate::wire::encode(&Value::Duration(0));
        assert!(rows("longest_wait").iter().all(|(_, m)| *m == none));
        assert!(subjects("longest_wait").contains(&"NEW".to_string()));
        assert_eq!(rows("rework"), [("WORKING".to_string(), json!(1))]);
        // handed back to ana: a subject that names an actor
        assert_eq!(rows("reassigned_back"), [(ana.clone(), json!(1))]);
        assert!(
            found
                .iter()
                .filter(|d| d.kind == "reassigned_back")
                .all(|d| d.names_actor)
        );
        // t1 was parked and reassigned by someone other than its engineer,
        // and started and finished by her
        assert_eq!(
            rows("acted_by_non_assignee"),
            [
                ("assign".to_string(), json!("1.000")),
                ("park".to_string(), json!("1.000"))
            ]
        );
        // one of the four starts that applied
        assert_eq!(
            rows("backdating_rate")[0],
            ("start".to_string(), json!("0.250"))
        );
        assert!(
            rows("backdating_rate")[1..]
                .iter()
                .all(|(_, m)| *m == json!("0.000"))
        );
        assert_eq!(rows("would_be_refusal"), [("tidy".to_string(), json!(1))]);
        assert_eq!(rows("top_flag"), [("sized".to_string(), json!(1))]);
        // the kinds in ADR-0160's order
        let order = [
            "unused_transition",
            "unused_state",
            "top_refusal",
            "state_entries",
            "mostly_override",
            "label_frequency",
            "label_by_state",
            "longest_wait",
            "rework",
            "reassigned_back",
            "acted_by_non_assignee",
            "backdating_rate",
            "would_be_refusal",
            "top_flag",
        ];
        let kinds: Vec<usize> = found
            .iter()
            .map(|d| order.iter().position(|k| *k == d.kind).unwrap())
            .collect();
        assert!(kinds.windows(2).all(|w| w[0] <= w[1]));
        // outside the window nothing was used, and nothing is counted
        drop(w);
        s.now += 3 * 86_400;
        let w = s.snapshot().unwrap();
        let later = diagnostics(&w, "Ticket", 86_400).unwrap().unwrap();
        assert_eq!(
            later
                .iter()
                .filter(|d| d.kind == "unused_transition")
                .count(),
            7
        );
        assert!(
            later
                .iter()
                .all(|d| d.kind != "top_refusal" && d.kind != "label_frequency")
        );
        assert_eq!(
            diagnostics(&w, "Nope", 86_400).unwrap(),
            Err("unknown_type")
        );
    }
}

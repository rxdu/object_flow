//! Subscriptions: a filter over the log that a reader pulls and
//! acknowledges, the core's only delivery (`DESIGN.md` §7). The log is read
//! by a settled cursor, which on SQLite, whose writers are serial, is the
//! position (ADR-0089).

use std::collections::BTreeSet;

use crate::eval::NotRun;
use crate::model::SUBSCRIPTION;
use crate::store::Event;
use crate::value::Value;
use crate::world::{Object, World};

/// A page of events (`library-api.md` §6): those the subscription's filter
/// selects after the cursor, in position order, which is each object's own,
/// and the settled cursor, the furthest a reader may acknowledge; or a
/// refusal, a value, with no events.
#[derive(Clone, Debug, PartialEq)]
pub struct EventPage {
    pub events: Vec<Event>,
    pub settled: i64,
    pub refused: Option<String>,
}

/// The events a subscription selects after a cursor, exclusive, or after
/// its acknowledged cursor where none is given; a subscription the store
/// does not hold is refused as `unknown` and a revoked one as `revoked`.
pub fn pull(
    world: &World<'_>,
    subscription: &str,
    cursor: Option<i64>,
) -> Result<EventPage, NotRun> {
    pull_page(world, subscription, cursor, usize::MAX)
}

/// A page of `pull` (ADR-0179): at most `limit` events, and, where more
/// remain, the last one's position as the settled cursor to continue from
/// and acknowledge, which on SQLite, whose writers are serial, is settled.
pub fn pull_page(
    world: &World<'_>,
    subscription: &str,
    cursor: Option<i64>,
    limit: usize,
) -> Result<EventPage, NotRun> {
    let settled = world.log()?.map(|e| e.position).max().unwrap_or(0);
    let refuse = |why: &str| EventPage {
        events: Vec::new(),
        settled,
        refused: Some(why.to_string()),
    };
    let Some(s) = world.get(subscription)?.filter(|s| s.type_ == SUBSCRIPTION) else {
        return Ok(refuse("unknown"));
    };
    if s.state == "REVOKED" {
        return Ok(refuse("revoked"));
    }
    let from = match cursor {
        Some(c) => c,
        None => acknowledged(world, s)?,
    };
    let targets = targets(world, s)?;
    let mut events: Vec<Event> = world
        .log()?
        .filter(|e| e.position > from && selects(s, &targets, e))
        .cloned()
        .collect();
    events.sort_by_key(|e| e.position);
    let settled = if events.len() > limit {
        events.truncate(limit);
        events.last().map_or(settled, |e| e.position)
    } else {
        settled
    };
    Ok(EventPage {
        events,
        settled,
        refused: None,
    })
}

/// Its acknowledged cursor: the one last acknowledged, or the log's settled
/// cursor at its creation, its own creation's position, so it starts with
/// nothing old (`DESIGN.md` §7).
pub fn acknowledged(world: &World<'_>, s: &Object) -> Result<i64, NotRun> {
    if let Some(at) = world.acknowledged.get(&s.id) {
        return Ok(*at);
    }
    Ok(world
        .log()?
        .find(|e| e.object_id == s.id)
        .map_or(0, |e| e.position))
}

/// The objects its filter's type selects, by id: its target type's, or its
/// family's where it says so, each type read in one fetch rather than an
/// object an event.
fn targets<'w>(world: &'w World<'_>, s: &Object) -> Result<BTreeSet<&'w str>, NotRun> {
    let Value::Text(target) = s.attr("target_type") else {
        return Ok(BTreeSet::new());
    };
    let family = s.attr("target_family") == Value::Bool(true);
    Ok(world
        .of_type(&target)?
        .into_iter()
        .filter(|o| family || o.type_ == target)
        .map(|o| o.id.as_str())
        .collect())
}

/// Whether its filter selects an event: one of an object its type selects;
/// of a transition it names, or of any where it names none; and one
/// changing the state where it asks only for those.
fn selects(s: &Object, targets: &BTreeSet<&str>, e: &Event) -> bool {
    let typed = targets.contains(e.object_id.as_str());
    let named = match s.attr("transitions") {
        Value::Set(names) if !names.is_empty() => names
            .iter()
            .any(|n| *n == Value::Text(e.transition.clone())),
        _ => true,
    };
    let changing = s.attr("changes_state_only") != Value::Bool(true) || e.changes_state;
    typed && named && changing
}

/// Its lag, the age by recorded time of the oldest event its filter selects
/// after its acknowledged cursor, zero where none is pending; and whether it
/// is lagging, or dead, from the moment the lag reaches `lagging_after`, or
/// `dead_after`, derived and never moved into (`DESIGN.md` §7, ADR-0100).
pub fn lag(world: &World<'_>, s: &Object) -> Result<(i64, bool, bool), NotRun> {
    let from = acknowledged(world, s)?;
    let targets = targets(world, s)?;
    let oldest = world
        .log()?
        .filter(|e| e.position > from && selects(s, &targets, e))
        .map(|e| e.recorded_at)
        .min();
    let lag = oldest.map_or(0, |at| (world.now - at).max(0));
    let reached = |threshold: &str| matches!(s.attr(threshold), Value::Duration(d) if lag >= d);
    Ok((lag, reached("lagging_after"), reached("dead_after")))
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

    const JOBS: &str = r#"
module: jobs
categories: [live, closed]
types:
  Job:
    abstract: true
    attributes:
      title: { type: string }
  Repair:
    extends: Job
    tracking: record
    states:
      OPEN: { category: live }
      WORKING: { category: live }
      DONE: { category: closed, final: true }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [title] }
      start: { kind: external, from: [OPEN], to: WORKING }
      note: { kind: internal, from: [OPEN, WORKING], required_inputs: [title] }
      finish: { kind: external, from: [WORKING], to: DONE }
"#;

    fn req(s: &mut Store, tn: &str, xn: &str, object: Option<&str>, inputs: Json) -> String {
        let r = Request {
            transition: xn.into(),
            actor: OPERATOR.into(),
            object_id: object.map(str::to_string),
            type_: object.is_none().then(|| tn.to_string()),
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
    }

    fn pulled(s: &Store, sub: &str, cursor: Option<i64>) -> Vec<(String, String)> {
        let w = s.snapshot().unwrap();
        let page = pull(&w, sub, cursor).unwrap();
        assert_eq!(page.refused, None);
        page.events
            .into_iter()
            .map(|e| (e.object_id, e.transition))
            .collect()
    }

    /// `DESIGN.md` §7: a subscription starts with nothing old; it selects
    /// by type or family, transition and change of state; it is pulled from
    /// its acknowledged cursor, or one given; acknowledging moves it either
    /// way and records nothing in the log; revoked or unknown, it is refused
    /// as a value; and its lag is derived.
    #[test]
    fn a_subscription_is_pulled_acknowledged_and_lags() {
        let lib = Library::load(&[("jobs.yaml".into(), JOBS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let old = req(&mut s, "Repair", "open", None, json!({"title": "old"}));
        req(&mut s, "", "start", Some(&old), json!({}));
        // every Job's start or finish, by family; and only changes of state
        let sub = req(
            &mut s,
            SUBSCRIPTION,
            "subscribe",
            None,
            json!({"target_type": "Job", "target_family": true, "transitions": ["start", "note"],
                   "changes_state_only": true, "lagging_after": "1 h", "dead_after": "3 h"}),
        );
        // nothing old
        assert_eq!(pulled(&s, &sub, None), []);
        let j = req(&mut s, "Repair", "open", None, json!({"title": "new"}));
        req(&mut s, "", "note", Some(&j), json!({"title": "renamed"}));
        req(&mut s, "", "start", Some(&j), json!({}));
        req(&mut s, "", "finish", Some(&j), json!({}));
        // a note changes no state, a finish is no transition it names
        assert_eq!(pulled(&s, &sub, None), [(j.clone(), "start".to_string())]);
        // from the log's start, a cursor given, it reads the old start too
        assert_eq!(
            pulled(&s, &sub, Some(0)),
            [
                (old.clone(), "start".to_string()),
                (j.clone(), "start".to_string())
            ]
        );
        // its lag: the pending start recorded two hours ago
        s.now += 2 * 3600;
        let lag = |s: &Store| {
            let w = s.snapshot().unwrap();
            let v = crate::reads::get(&w, &sub).unwrap().unwrap();
            ["lag", "lagging", "dead"]
                .map(|n| v.derived.iter().find(|(d, _)| d == n).unwrap().1.clone())
        };
        assert_eq!(
            lag(&s),
            [Value::Duration(7200), Value::Bool(true), Value::Bool(false)]
        );
        // acknowledging records nothing in the log, and moves it forward
        let (events, version) = {
            let w = s.snapshot().unwrap();
            (
                w.log().unwrap().count(),
                w.get(&sub).unwrap().unwrap().version,
            )
        };
        let settled = pull(&s.snapshot().unwrap(), &sub, None).unwrap().settled;
        assert_eq!(s.acknowledge(&sub, settled).unwrap(), None);
        {
            let w = s.snapshot().unwrap();
            assert_eq!(
                (
                    w.log().unwrap().count(),
                    w.get(&sub).unwrap().unwrap().version
                ),
                (events, version)
            );
        }
        assert_eq!(pulled(&s, &sub, None), []);
        assert_eq!(
            lag(&s),
            [Value::Duration(0), Value::Bool(false), Value::Bool(false)]
        );
        // and back, so the next pull reads from there again
        assert_eq!(s.acknowledge(&sub, 0).unwrap(), None);
        assert_eq!(pulled(&s, &sub, None).len(), 2);
        // revoked, or unknown, it is refused, a value
        req(&mut s, "", "revoke", Some(&sub), json!({}));
        let w = s.snapshot().unwrap();
        assert_eq!(
            pull(&w, &sub, None).unwrap().refused.as_deref(),
            Some("revoked")
        );
        assert_eq!(
            pull(&w, &j, None).unwrap().refused.as_deref(),
            Some("unknown")
        );
        drop(w);
        assert_eq!(s.acknowledge(&sub, 0).unwrap().as_deref(), Some("revoked"));
        assert_eq!(
            s.acknowledge("nonesuch", 0).unwrap().as_deref(),
            Some("unknown")
        );
    }
}

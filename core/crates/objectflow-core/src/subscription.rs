//! Subscriptions: a filter over the log that a reader pulls and
//! acknowledges, the core's only delivery (`DESIGN.md` §7). The log is read
//! by a settled cursor, which on SQLite, whose writers are serial, is the
//! position (ADR-0089).

use crate::eval::NotRun;
use crate::model::SUBSCRIPTION;
use crate::store::{Event, Head};
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
    /// Whether the page ended at its limit or its bytes, so the next may
    /// hold more now; where it did not, it holds every event there is now
    /// (ninth production review N-9).
    pub more: bool,
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
    let settled = world.last_position()?;
    let refuse = |why: &str| EventPage {
        events: Vec::new(),
        settled,
        refused: Some(why.to_string()),
        more: false,
    };
    let Some(s) = world.get(subscription)?.filter(|s| s.type_ == SUBSCRIPTION) else {
        return Ok(refuse("unknown"));
    };
    if s.state == "REVOKED" {
        return Ok(refuse("revoked"));
    }
    // a cursor past what the store has handed out is none of its own, as
    // `acknowledge` and `export` refuse it, where one a subscriber kept from
    // before a restore answered an empty page and a settled cursor behind
    // it, and the events written since never came (sixteenth production
    // review)
    // and one in a gap a restore skipped, which the copy never saw, where it
    // was honoured once the log passed it and the events at the positions
    // it reached past were never delivered (the audit's durability B5,
    // ADR-0207 decision 5)
    let from = match cursor {
        Some(c) if c < 0 || c > settled || world.skipped_position(c)? => {
            return Ok(refuse("invalid_cursor"));
        }
        Some(c) => c,
        None => acknowledged(world, s)?,
    };
    // one past the page says another follows
    let mut heads = selected(world, s, from, limit.saturating_add(1))?;
    let mut more = heads.len() > limit;
    heads.truncate(limit);
    // the page's events read whole one at a time, ending at its bytes as at
    // its limit, one event at least, where 1,000 events of large inputs
    // answered 197 MB (ninth production review N-9) and every one was read
    // before the page was cut (tenth production review)
    let mut events = Vec::new();
    let mut bytes = 0;
    for (i, h) in heads.iter().enumerate() {
        let Some(e) = world.event_at(h.position)? else {
            continue;
        };
        bytes += crate::store::event_bytes(&e);
        events.push(e);
        if bytes >= crate::reads::PAGE_BYTES && i + 1 < heads.len() {
            more = true;
            break;
        }
    }
    let settled = if more {
        events.last().map_or(settled, |e| e.position)
    } else {
        settled
    };
    Ok(EventPage {
        events,
        settled,
        refused: None,
        more,
    })
}

/// Its acknowledged cursor: the one last acknowledged, or the log's settled
/// cursor at its creation, its own creation's position, so it starts with
/// nothing old (`DESIGN.md` §7).
pub fn acknowledged(world: &World<'_>, s: &Object) -> Result<i64, NotRun> {
    if let Some(at) = world.acknowledged()?.get(&s.id) {
        return Ok(*at);
    }
    Ok(world.first_position(&s.id)?.unwrap_or(0))
}

/// The types whose objects' events its filter selects: its target type, or
/// its family where it says so; none where it names no type.
fn types_of(world: &World<'_>, s: &Object) -> Vec<String> {
    let Value::Text(target) = s.attr("target_type") else {
        return Vec::new();
    };
    let family = s.attr("target_family") == Value::Bool(true);
    world
        .lib
        .types
        .keys()
        .filter(|t| {
            if family {
                world.lib.is_a(t, &target)
            } else {
                **t == target
            }
        })
        .cloned()
        .collect()
}

/// The heads of the first `limit` events after a position its filter
/// selects, read from the log by their objects' types a chunk at a time from
/// the position, where each pull read the whole log and every object of the
/// type (fourth production review FEED); their heads alone, so choosing them
/// reads no event whole, where a `get` of a lagging subscription read 256
/// events of 2 MB (tenth production review).
fn selected(world: &World<'_>, s: &Object, from: i64, limit: usize) -> Result<Vec<Head>, NotRun> {
    let types = types_of(world, s);
    let mut out = Vec::new();
    if types.is_empty() || limit == 0 {
        return Ok(out);
    }
    let chunk = limit.saturating_add(1).max(256);
    let mut at = from;
    loop {
        let page = world.log_heads_after(at, Some(&types), chunk)?;
        let done = page.len() < chunk;
        for h in page {
            at = h.position;
            if selects(s, &h) {
                out.push(h);
                if out.len() >= limit {
                    return Ok(out);
                }
            }
        }
        if done {
            return Ok(out);
        }
    }
}

fn selects(s: &Object, h: &Head) -> bool {
    let named = match s.attr("transitions") {
        Value::Set(names) if !names.is_empty() => names
            .iter()
            .any(|n| *n == Value::Text(h.transition.clone())),
        _ => true,
    };
    let changing = s.attr("changes_state_only") != Value::Bool(true) || h.changes_state;
    named && changing
}

/// Its lag, the age by recorded time of the oldest event its filter selects
/// after its acknowledged cursor, zero where none is pending; and whether it
/// is lagging, or dead, from the moment the lag reaches `lagging_after`, or
/// `dead_after`, derived and never moved into (`DESIGN.md` §7, ADR-0100).
pub fn lag(world: &World<'_>, s: &Object) -> Result<(i64, bool, bool), NotRun> {
    let from = acknowledged(world, s)?;
    // the oldest pending is the first, the store's clock never moving back,
    // its head read and never the event whole (tenth production review)
    let oldest = selected(world, s, from, 1)?.first().map(|h| h.recorded_at);
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

    /// An acknowledgement of a cursor the log never had, past its last
    /// position or below 0, is refused, where one past the log skipped
    /// every later event, and 0 still replays it from the start; and an
    /// export's cursor past the log or the attempts is
    /// refused, where a forged one answered an empty page for ever (fourth
    /// production review A-12).
    #[test]
    fn a_cursor_the_store_never_handed_out_is_refused() {
        let lib = Library::load(&[("jobs.yaml".into(), JOBS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        req(&mut s, "Repair", "open", None, json!({"title": "before"}));
        let sub = req(
            &mut s,
            "Subscription",
            "subscribe",
            None,
            json!({"target_type": "Repair"}),
        );
        req(&mut s, "Repair", "open", None, json!({"title": "after"}));
        let w = s.snapshot().unwrap();
        let (first, last) = (
            w.first_position(&sub).unwrap().unwrap(),
            w.last_position().unwrap(),
        );
        for source in ["log", "Repair.transitions", "Repair.attempts"] {
            let far = (last + 1_000_000).to_string();
            let page = crate::export::export_page(&w, source, Some(&far), 10).unwrap();
            assert_eq!(page.refused.as_deref(), Some("invalid_cursor"), "{source}");
        }
        let page = crate::export::export_page(&w, "log", Some(&first.to_string()), 10).unwrap();
        assert_eq!(page.refused, None);
        drop(w);
        let invalid = Some("invalid_cursor".to_string());
        // and a pull's, as a subscriber kept it from before a restore
        // (sixteenth production review)
        let w = s.snapshot().unwrap();
        assert_eq!(pull(&w, &sub, Some(last + 1)).unwrap().refused, invalid);
        assert_eq!(pull(&w, &sub, Some(-1)).unwrap().refused, invalid);
        assert_eq!(pull(&w, &sub, Some(last)).unwrap().refused, None);
        drop(w);
        assert_eq!(s.acknowledge(&sub, last + 1).unwrap(), invalid);
        assert_eq!(s.acknowledge(&sub, -1).unwrap(), invalid);
        assert_eq!(s.acknowledge(&sub, last).unwrap(), None);
        assert_eq!(s.acknowledge(&sub, first).unwrap(), None);
        assert_eq!(s.acknowledge(&sub, 0).unwrap(), None);
    }

    /// A page of the feed reads from its cursor, never the whole log nor
    /// every object: a pull with nothing new took 1.3 s and 640 MB on six
    /// years, an export page about 5 s whatever its size (fourth production
    /// review FEED).
    #[test]
    fn a_page_of_the_feed_reads_from_its_cursor() {
        let lib = Library::load(&[("jobs.yaml".into(), JOBS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let object_id = req(
            &mut s,
            "Subscription",
            "subscribe",
            None,
            json!({"target_type": "Repair"}),
        );
        for i in 0..300 {
            req(
                &mut s,
                "Repair",
                "open",
                None,
                json!({"title": format!("r{i}")}),
            );
        }
        let w = s.snapshot().unwrap();
        let page = pull_page(&w, &object_id, None, 10).unwrap();
        assert_eq!(page.events.len(), 10);
        assert!(!w.log_read_whole(), "a pull read the whole log");
        let w = s.snapshot().unwrap();
        let page = crate::export::export_page(&w, "log", Some("100"), 10).unwrap();
        assert_eq!(page.lines.len(), 10);
        assert!(!w.log_read_whole(), "an export read the whole log");
        assert!(
            w.held_count() < 100,
            "an export page held {} objects",
            w.held_count()
        );
        let w = s.snapshot().unwrap();
        let page = crate::export::export_page(&w, "Repair.transitions", Some("100"), 10).unwrap();
        assert_eq!(page.lines.len(), 10);
        assert!(!w.log_read_whole(), "a type's export read the whole log");
    }

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
            target_version: None,
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
    /// A subscription is refused where its filter would select nothing of
    /// the installed version or its durations make no sense, each input
    /// named, where it was taken and delivered nothing for good (D689,
    /// ADR-0205 decision 10); a family's subscription may name a transition
    /// only a kind of it declares.
    #[test]
    fn a_subscription_selecting_nothing_is_refused() {
        let lib = Library::load(&[("jobs.yaml".into(), JOBS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let ask = |s: &mut Store, inputs: Json| {
            let r = Request {
                transition: "subscribe".into(),
                actor: OPERATOR.into(),
                object_id: None,
                type_: Some(SUBSCRIPTION.into()),
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
        };
        let refused = |field: &str| {
            Verdict::invalid_input([(field.to_string(), crate::verdict::InputReason::NotAllowed)])
        };
        for (inputs, field) in [
            (json!({"target_type": "Jbo"}), "target_type"),
            (json!({"target_type": "job"}), "target_type"),
            (
                json!({"target_type": "Job", "transitions": ["strat"]}),
                "transitions",
            ),
            (
                json!({"target_type": "Job", "lagging_after": "0 s"}),
                "lagging_after",
            ),
            (
                json!({"target_type": "Job", "dead_after": "0 s"}),
                "dead_after",
            ),
            (
                json!({"target_type": "Job", "lagging_after": "3 h", "dead_after": "1 h"}),
                "dead_after",
            ),
        ] {
            assert_eq!(ask(&mut s, inputs.clone()), refused(field), "{inputs}");
        }
        // a transition every kind of Job declares, and a duration in order
        assert!(matches!(
            ask(
                &mut s,
                json!({"target_type": "Job", "target_family": true, "transitions": ["start"],
                       "lagging_after": "1 h", "dead_after": "1 h"})
            ),
            Verdict::Satisfied { .. }
        ));
    }

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

    /// A request, a `get` and a pull from a cursor given read no
    /// subscription's cursor and no admission, where every operation read
    /// both (`research/write-throughput.md` §6); one reaching either reads
    /// it once and finds what the store holds, and a cursor the store cannot
    /// read stops the operation reading it alone, named.
    #[test]
    fn an_operation_reads_the_cursors_and_the_admissions_where_it_reaches_them() {
        let lib = Library::load(&[("jobs.yaml".into(), JOBS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let sub = req(
            &mut s,
            SUBSCRIPTION,
            "subscribe",
            None,
            json!({"target_type": "Repair"}),
        );
        let j = req(&mut s, "Repair", "open", None, json!({"title": "one"}));
        let settled = pull(&s.snapshot().unwrap(), &sub, None).unwrap().settled;
        assert_eq!(s.acknowledge(&sub, settled).unwrap(), None);
        let reads = || {
            (
                crate::world::whole_cursor_reads(),
                crate::world::whole_admission_reads(),
            )
        };
        let before = reads();
        req(&mut s, "", "start", Some(&j), json!({}));
        {
            let w = s.snapshot().unwrap();
            crate::reads::get(&w, &j).unwrap().unwrap();
            assert_eq!(pull(&w, &sub, Some(0)).unwrap().refused, None);
        }
        assert_eq!(reads(), before);
        // the start's event recorded an admission, since discharged, which its
        // row reads as an override, every row of the object from one read
        let at: i64 = s
            .connection()
            .query_row("SELECT MAX(position) FROM of_event", [], |r| r.get(0))
            .unwrap();
        s.connection()
            .execute(
                "INSERT INTO of_admission (object_id, invariant, position, reason, \
                 discharged_position) VALUES (?1, 'bounded', ?2, 'test', ?2)",
                crate::db::params![j, at],
            )
            .unwrap();
        {
            let w = s.snapshot().unwrap();
            let o = w.whole(&j).unwrap().unwrap();
            let t = w.lib.type_("Repair").unwrap();
            let overrides: Vec<(i64, Value)> = w
                .log()
                .unwrap()
                .filter(|e| e.object_id == j)
                .map(|e| {
                    let Value::Row(f) =
                        crate::eval::transition_row(&w, o, w.parts(o).unwrap(), t, e).unwrap()
                    else {
                        panic!("no row")
                    };
                    (e.position, f["overrides"].clone())
                })
                .collect();
            assert_eq!(
                overrides,
                [(at - 1, Value::Bool(false)), (at, Value::Bool(true))]
            );
            // and its pull from its acknowledged cursor reads the cursors
            // once, finding the one acknowledged
            assert_eq!(pull(&w, &sub, None).unwrap().events.len(), 1);
            assert_eq!(pull(&w, &sub, None).unwrap().events.len(), 1);
        }
        assert_eq!(reads(), (before.0 + 1, before.1 + 1));
        // a cursor damaged stops the pull reading it, never a request
        s.connection()
            .execute("UPDATE of_subscription_position SET acknowledged = 'x'", [])
            .unwrap();
        req(&mut s, "", "finish", Some(&j), json!({}));
        let w = s.snapshot().unwrap();
        crate::reads::get(&w, &j).unwrap().unwrap();
        match pull(&w, &sub, None) {
            Err(NotRun::Unreadable(why)) => {
                assert!(
                    why.contains(&format!("{sub}'s acknowledged cursor x")),
                    "{why}"
                );
            }
            other => panic!("{other:?}"),
        }
    }
}

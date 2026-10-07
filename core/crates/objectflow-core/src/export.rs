//! `export`: rows of one source as JSON lines, after a cursor, every personal
//! value omitted (`library-api.md` §6, ADR-0094). The log pages by the
//! settled cursor, which on SQLite, whose writers are serial, is the
//! position (ADR-0089); an attempt source by each row's writing
//! transaction; and an interval source by the event that last wrote each
//! row, so a row closed or relabelled since comes again and a consumer keeps
//! the latest per object, dimension and entry (ADR-0100 §6).

use std::collections::BTreeMap;

use serde_json::{Value as Json, json};

use crate::eval::NotRun;
use crate::model::Library;
use crate::store::{Attempt, Event};
use crate::value::timestamp_text;
use crate::wire;
use crate::world::{Object, Span, World};

/// A page of an export (`library-api.md` §6): its rows, one JSON object per
/// line, and the cursor to continue from; or a refusal, a value, with no
/// rows.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportPage {
    pub lines: Vec<String>,
    pub cursor: String,
    pub refused: Option<String>,
}

/// The rows of a source after a cursor, exclusive, in cursor order: `log`,
/// `<Type>.intervals` or `<Type>.attempts`. A source naming no type the
/// store declares, or a cursor that is not one, is refused as a value; a
/// source the slice does not serve yet is not run.
pub fn export(world: &World<'_>, source: &str, cursor: Option<&str>) -> Result<ExportPage, NotRun> {
    let refuse = |why: &str| {
        Ok(ExportPage {
            lines: Vec::new(),
            cursor: cursor.unwrap_or("0").to_string(),
            refused: Some(why.to_string()),
        })
    };
    // a type's attempt counts page by settled day, their cursor a date
    if let Some(type_) = source.strip_suffix(".attempt_counts")
        && world.lib.type_(type_).is_some()
    {
        return counts_page(world, type_, cursor);
    }
    let after = match cursor {
        None => 0,
        Some(c) => match c.parse::<i64>() {
            Ok(n) if n >= 0 => n,
            _ => return refuse("invalid_cursor"),
        },
    };
    let mut rows: Vec<(i64, Json)> = if source == "log" {
        // every object held first, a type at a time, rather than fetched
        // one at a time as each event names it
        world.in_order()?;
        world
            .log()?
            .filter(|e| e.position > after)
            .map(|e| Ok((e.position, event_row(world, e)?)))
            .collect::<Result<_, NotRun>>()?
    } else if world
        .lib
        .type_(source)
        .is_some_and(|t| t.observation_on.is_some())
    {
        // an observation kind's records, each by its latest event
        object_rows(world, world.of_type(source)?, after)?
    } else {
        let Some((type_, kind)) = source.split_once('.') else {
            return refuse("unknown_source");
        };
        if world.lib.type_(type_).is_none() {
            return refuse("unknown_source");
        }
        match kind {
            "intervals" => interval_rows(world, type_, after)?,
            "attempts" => world
                .attempts()?
                .iter()
                .filter(|a| a.txn > after)
                .filter(|a| a.type_.as_deref().is_some_and(|t| world.lib.is_a(t, type_)))
                .map(|a| (a.txn, attempt_row(a)))
                .collect(),
            // the log of the type's family
            "transitions" => {
                let family = family_of(world, type_)?;
                world
                    .log()?
                    .filter(|e| e.position > after && family.contains(e.object_id.as_str()))
                    .map(|e| Ok((e.position, event_row(world, e)?)))
                    .collect::<Result<_, NotRun>>()?
            }
            // the labels on the type's family's objects
            "labels" => {
                let family = family_of(world, type_)?;
                let on: Vec<&Object> = world
                    .of_type(crate::model::LABEL)?
                    .into_iter()
                    .filter(|l| {
                        matches!(l.attr("subject"), crate::value::Value::Object(s)
                            if family.contains(s.as_str()))
                    })
                    .collect();
                object_rows(world, on, after)?
            }
            _ => return refuse("unknown_source"),
        }
    };
    // in cursor order; rows sharing a cursor in the order written
    rows.sort_by_key(|(c, _)| *c);
    let next = rows.last().map_or(after, |(c, _)| *c);
    Ok(ExportPage {
        lines: rows.iter().map(|(_, r)| r.to_string()).collect(),
        cursor: next.to_string(),
        refused: None,
    })
}

/// A type's attempt counts after the cursor's day, a day's once the day is
/// over (ADR-0160): `declaration-syntax.md` §6.9's rows, the rollup's counts
/// and the remaining rows' together, by day and then by key; the cursor is
/// the last day exported, `YYYY-MM-DD`, an empty one the start. A day's
/// counts never change once it is over, attempts being dated by when they
/// were recorded, and a prune moves a count without changing it.
fn counts_page(world: &World<'_>, type_: &str, cursor: Option<&str>) -> Result<ExportPage, NotRun> {
    let day_of = |text: &str| {
        crate::value::parse_timestamp(&format!("{text}T00:00:00Z")).map(|t| t.div_euclid(86_400))
    };
    let after = match cursor {
        None | Some("") => None,
        Some(c) => match day_of(c) {
            Some(d) => Some(d),
            None => {
                return Ok(ExportPage {
                    lines: Vec::new(),
                    cursor: c.to_string(),
                    refused: Some("invalid_cursor".into()),
                });
            }
        },
    };
    let today = world.now.div_euclid(86_400);
    let mut rows: Vec<(i64, String)> = crate::metric::attempt_count_rows(world, type_)?
        .into_iter()
        .filter_map(|r| {
            let crate::value::Value::Row(f) = r else {
                return None;
            };
            let crate::value::Value::Date(day) = f.get("day")? else {
                return None;
            };
            let day = *day;
            if day >= today || after.is_some_and(|a| day <= a) {
                return None;
            }
            let row: serde_json::Map<String, Json> = f
                .iter()
                .map(|(k, v)| (k.clone(), wire::encode(v)))
                .collect();
            Some((day, Json::Object(row).to_string()))
        })
        .collect();
    rows.sort();
    let last = rows.last().map(|(d, _)| *d);
    Ok(ExportPage {
        cursor: match (last, cursor) {
            (Some(d), _) => match wire::encode(&crate::value::Value::Date(d)) {
                Json::String(t) => t,
                other => other.to_string(),
            },
            (None, c) => c.unwrap_or_default().to_string(),
        },
        lines: rows.into_iter().map(|(_, r)| r).collect(),
        refused: None,
    })
}

/// Each object after the cursor by its latest event, which a correction or a
/// forget moves, so the row comes again (ADR-0100 §6).
fn object_rows(
    world: &World<'_>,
    objects: Vec<&Object>,
    after: i64,
) -> Result<Vec<(i64, Json)>, NotRun> {
    // each object's latest event, read from the log once
    let mut last: BTreeMap<&str, i64> = BTreeMap::new();
    for e in world.log()? {
        last.insert(e.object_id.as_str(), e.position);
    }
    let mut rows = Vec::new();
    for o in objects {
        if let Some(at) = last.get(o.id.as_str()).copied()
            && at > after
        {
            rows.push((at, object_row(world, o)?));
        }
    }
    Ok(rows)
}

/// An object in `Object`'s shape (`library-api.md` §5), its personal
/// attributes omitted (ADR-0094 §3).
fn object_row(world: &World<'_>, o: &Object) -> Result<Json, NotRun> {
    let t = world.lib.type_(&o.type_);
    let attributes: serde_json::Map<String, Json> = o
        .attrs
        .iter()
        .filter(|(a, _)| !t.and_then(|t| t.attribute(a)).is_some_and(|s| s.personal))
        .map(|(a, v)| (a.clone(), wire::encode(v)))
        .collect();
    Ok(json!({
        "id": o.id,
        "type": o.type_,
        "state": o.state,
        "version": o.version,
        "declaration_version": world.version_of(&o.id)?,
        "created_at": timestamp_text(o.created_at),
        "created_by_kind": o.created_by_kind,
        "attributes": attributes,
        "recorded_from": timestamp_text(o.recorded_from),
        "created_dated": o.created_dated,
    }))
}

/// The objects of a type's family, by id, the family read a type at a time.
fn family_of<'w>(
    world: &'w World<'_>,
    type_: &str,
) -> Result<std::collections::BTreeSet<&'w str>, NotRun> {
    Ok(world
        .of_type(type_)?
        .into_iter()
        .map(|o| o.id.as_str())
        .collect())
}

/// An event as the log exports it, its personal inputs and corrected
/// values omitted (ADR-0094 §3).
fn event_row(world: &World<'_>, e: &Event) -> Result<Json, NotRun> {
    let type_ = world.get(&e.object_id)?.map(|o| o.type_.as_str());
    let mut payload = e.payload.clone();
    for part in ["inputs", "corrected"] {
        if let Some(values) = payload.get_mut(part).and_then(Json::as_object_mut) {
            values.retain(|k, _| !personal(world.lib, type_, &e.transition, k));
        }
    }
    Ok(json!({
        "position": e.position,
        "object_id": e.object_id,
        "object_seq": e.object_seq,
        "transition": e.transition,
        "from_state": e.from_state,
        "to_state": e.to_state,
        "changes_state": e.changes_state,
        "source": e.source,
        "actor_id": e.actor_id,
        "actor_kind": e.actor_kind,
        "principal": e.principal,
        "cause": e.cause,
        "declaration_version": e.declaration_version,
        "recorded_at": timestamp_text(e.recorded_at),
        "occurred_at": e.occurred_at.map(timestamp_text),
        "imported": e.imported,
        "payload": payload,
        "reads": e.reads,
        "context": e.context,
    }))
}

/// Whether an input an event's payload holds is personal under the
/// transition it recorded: an attribute marked so, an input marked so, or
/// one its effect writes into a personal attribute (check 10).
fn personal(lib: &Library, type_: Option<&str>, transition: &str, name: &str) -> bool {
    let Some(t) = type_.and_then(|t| lib.type_(t)) else {
        return false;
    };
    t.attribute(name).is_some_and(|a| a.personal)
        || t.transition(transition)
            .is_some_and(|x| crate::request::input_personal(t, x, name))
}

/// Each span of each object of the type, after the cursor, by the event that
/// last wrote it: the one that opened it, closed it, or relabelled it in a
/// correction; a legacy span, which no event opened, by the import event
/// that wrote its object (`library-api.md` §6).
fn interval_rows(world: &World<'_>, type_: &str, after: i64) -> Result<Vec<(i64, Json)>, NotRun> {
    let log: Vec<&Event> = world.log()?.collect();
    let transition_at = |position: i64| {
        log.binary_search_by_key(&position, |e| e.position)
            .ok()
            .map(|i| log[i].transition.clone())
    };
    let mut rows = Vec::new();
    let objects = world.of_type(type_)?;
    world.with_parts(&objects)?;
    for o in objects {
        let imported = log
            .iter()
            .find(|e| e.object_id == o.id && e.imported)
            .map_or(0, |e| e.position);
        let Some(t) = world.lib.type_(&o.type_) else {
            continue;
        };
        for (dimension, spans) in &world.parts(o)?.spans {
            let personal =
                dimension != "state" && t.attribute(dimension).is_some_and(|a| a.personal);
            for sp in spans {
                let cursor = if sp.legacy {
                    imported
                } else {
                    [Some(sp.entered_position), sp.left_position, sp.corrected_by]
                        .into_iter()
                        .flatten()
                        .max()
                        .unwrap_or(0)
                };
                if cursor <= after {
                    continue;
                }
                rows.push((cursor, span_row(o, dimension, sp, personal, &transition_at)));
            }
        }
    }
    Ok(rows)
}

/// A span as the export writes it, in `Interval`'s shape
/// (`library-api.md` §4), its value omitted where the member is personal.
fn span_row(
    o: &Object,
    dimension: &str,
    sp: &Span,
    personal: bool,
    transition_at: &dyn Fn(i64) -> Option<String>,
) -> Json {
    let mut row = json!({
        "object_id": o.id,
        "dimension": dimension,
        "entered_at": timestamp_text(sp.entered_at),
        "left_at": sp.left_at.map(timestamp_text),
        "entered_by": sp.entered_by,
        "entered_by_kind": sp.entered_by_kind,
        "transition": if sp.legacy { None } else { transition_at(sp.entered_position) },
        "declaration_version": sp.declaration_version,
        "legacy": sp.legacy,
    });
    if !personal {
        row["value"] = match &sp.value {
            crate::value::Value::State { name, .. } => json!(name),
            v => wire::encode(v),
        };
    }
    row
}

/// An attempt as the export writes it, in `Attempt`'s shape
/// (`library-api.md` §4), which holds no personal value: its personal inputs
/// are named and withheld when it is written (ADR-0105).
fn attempt_row(a: &Attempt) -> Json {
    json!({
        "at": timestamp_text(a.at),
        "actor_id": a.actor_id,
        "actor_kind": a.actor_kind,
        "principal": a.principal,
        "context": a.context,
        "type": a.type_,
        "object_id": a.object_id,
        "transition": a.transition,
        "verdict": a.verdict,
        "clause": a.clause,
        "remedy": a.remedy,
        "unknown": a.unknown,
        "enforced": a.enforced,
        "flagged": a.flagged,
        "reads": a.reads,
        "applied_position": a.applied_position,
        "call": a.call,
        "proposal": a.proposal,
        "consulted": a.consulted,
        "request_values": a.request_values,
        "withheld_inputs": a.withheld,
        "declaration_version": a.declaration_version,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::model::{Library, OPERATOR};
    use crate::request::{Request, request};
    use crate::store::Store;
    use crate::verdict::Verdict;
    use serde_json::json;

    const CREWS: &str = r#"
module: crews
categories: [live, closed]
enumerations: { Crew: [NORTH, SOUTH], Mood: [CALM, CROSS] }
types:
  Job:
    attributes:
      crew: { type: Crew }
      mood: { type: Mood, optional: true, personal: true }
      note: { type: string, optional: true, personal: true }
    states:
      OPEN: { category: live }
      WORKING: { category: live }
    conditions:
      calm: { expression: mood != Mood.CROSS, remedy: self_serviceable }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [crew], optional_inputs: [mood, note] }
      start: { kind: external, from: [OPEN], to: WORKING, guards: { calm: deny } }
      fix_crew: { kind: internal, from: [OPEN, WORKING], corrects: [crew], required_inputs: [crew], inputs: { reason: { type: string } } }
      note: { kind: internal, from: [OPEN, WORKING], inputs: { whisper: { type: string, personal: true }, said: { type: string } } }
    observations:
      readings:
        kind: Reading
        description: A reading taken on the job.
        attributes:
          level: { type: int }
          remark: { type: string, optional: true, personal: true }
  Tool:
    attributes:
      size: { type: int }
    states:
      ON: { category: live }
    transitions:
      add: { kind: initial, to: ON, required_inputs: [size] }
"#;

    fn req(s: &mut Store, xn: &str, object: Option<&str>, inputs: Json) -> Verdict {
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
        request(s, &r).unwrap().unwrap()
    }

    fn page(s: &Store, source: &str, cursor: Option<&str>) -> (Vec<Json>, String) {
        let w = s.snapshot().unwrap();
        let p = export(&w, source, cursor).unwrap();
        assert_eq!(p.refused, None);
        let rows = p
            .lines
            .iter()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        (rows, p.cursor)
    }

    /// `library-api.md` §6, ADR-0094, ADR-0100 §6: the log pages by
    /// position and an attempt source by writing transaction; an interval
    /// source by the event that last wrote each row, so a closing, and a
    /// correction's relabelling, emits the row again; every personal value is
    /// omitted; and a source or a cursor it cannot read is refused as a value.
    #[test]
    fn an_export_pages_by_what_last_wrote_each_row_and_omits_personal_values() {
        let lib = Library::load(&[("crews.yaml".into(), CREWS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let open = |s: &mut Store, mood: &str| match req(
            s,
            "open",
            None,
            json!({"crew": "NORTH", "mood": mood, "note": "private"}),
        ) {
            Verdict::Satisfied { object_id, .. } => object_id,
            other => panic!("{other:?}"),
        };
        let (calm, cross) = (open(&mut s, "CALM"), open(&mut s, "CROSS"));
        // the log, its personal inputs omitted
        let (log, at) = page(&s, "log", None);
        let opened = log.iter().find(|e| e["object_id"] == json!(calm)).unwrap();
        assert_eq!(opened["payload"]["inputs"], json!({"crew": "NORTH"}));
        assert_eq!(at, log.last().unwrap()["position"].to_string());
        assert_eq!(page(&s, "log", Some(&at)).0, Vec::<Json>::new());
        // a declared input marked personal is left out too
        req(
            &mut s,
            "note",
            Some(&cross),
            json!({"whisper": "private", "said": "public"}),
        );
        let (log, _) = page(&s, "log", Some(&at));
        assert_eq!(log[0]["payload"]["inputs"], json!({"said": "public"}));
        // each job's state, crew and mood, the mood's value omitted
        let (rows, c1) = page(&s, "Job.intervals", None);
        assert_eq!(rows.len(), 6);
        let mood = rows
            .iter()
            .find(|r| r["object_id"] == json!(calm) && r["dimension"] == json!("mood"))
            .unwrap();
        assert!(mood.get("value").is_none());
        assert_eq!(mood["transition"], json!("open"));
        // a start closes OPEN, which comes again with its end, and opens WORKING
        req(&mut s, "start", Some(&calm), json!({}));
        let (rows, c2) = page(&s, "Job.intervals", Some(&c1));
        let seen: Vec<(Json, Json)> = rows
            .iter()
            .map(|r| (r["value"].clone(), r["left_at"].clone()))
            .collect();
        assert_eq!(
            seen,
            [
                (json!("OPEN"), json!("1970-01-01T00:00:00Z")),
                (json!("WORKING"), Json::Null)
            ]
        );
        // a correction relabels the crew's row, and marks each row whose
        // `.held` crew it changed, every row of the job: each comes again
        req(
            &mut s,
            "fix_crew",
            Some(&calm),
            json!({"crew": "SOUTH", "reason": "mis-keyed"}),
        );
        let (rows, c3) = page(&s, "Job.intervals", Some(&c2));
        let mut seen: Vec<(Json, Json, Json)> = rows
            .iter()
            .map(|r| {
                let value = r.get("value").cloned().unwrap_or(Json::Null);
                (r["object_id"].clone(), r["dimension"].clone(), value)
            })
            .collect();
        seen.sort_by_key(|r| format!("{r:?}"));
        assert_eq!(
            seen,
            [
                (json!(calm), json!("crew"), json!("SOUTH")),
                (json!(calm), json!("mood"), Json::Null),
                (json!(calm), json!("state"), json!("OPEN")),
                (json!(calm), json!("state"), json!("WORKING")),
            ]
        );
        assert_eq!(page(&s, "Job.intervals", Some(&c3)), (Vec::new(), c3));
        // a refusal is a row of its type's attempt source, by its
        // transaction, and of no other type's
        assert!(matches!(
            req(&mut s, "start", Some(&cross), json!({})),
            Verdict::Unsatisfied { .. }
        ));
        let tool = Request {
            transition: "add".into(),
            actor: OPERATOR.into(),
            object_id: None,
            type_: Some("Tool".into()),
            inputs: std::collections::BTreeMap::new(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
        };
        assert!(matches!(
            request(&mut s, &tool).unwrap().unwrap(),
            Verdict::InvalidInput { .. }
        ));
        assert_eq!(page(&s, "Tool.attempts", None).0.len(), 1);
        let (rows, a1) = page(&s, "Job.attempts", None);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (&rows[0]["object_id"], &rows[0]["clause"], &rows[0]["call"]),
            (&json!(cross), &json!("calm"), &Json::Null)
        );
        assert_eq!(page(&s, "Job.attempts", Some(&a1)), (Vec::new(), a1));
        // what it cannot read is refused, a value; what it does not serve
        // yet is not run
        let w = s.snapshot().unwrap();
        for (source, cursor, why) in [
            ("Nope.intervals", None, "unknown_source"),
            ("Job.nothing", None, "unknown_source"),
            ("everything", None, "unknown_source"),
            ("log", Some("-1"), "invalid_cursor"),
            ("log", Some("x"), "invalid_cursor"),
        ] {
            assert_eq!(
                export(&w, source, cursor).unwrap().refused.as_deref(),
                Some(why)
            );
        }
        drop(w);
        // a type's attempt counts: today's wait until the day is over, then
        // come once, a prune changing nothing a consumer read (ADR-0160)
        assert_eq!(
            page(&s, "Job.attempt_counts", None),
            (Vec::new(), String::new())
        );
        s.now += 86_400;
        let (counts, day) = page(&s, "Job.attempt_counts", None);
        assert_eq!(day, "1970-01-01");
        assert_eq!(counts.len(), 1);
        assert_eq!(
            (
                &counts[0]["day"],
                &counts[0]["clause"],
                &counts[0]["count"],
                &counts[0]["object"]
            ),
            (
                &json!("1970-01-01"),
                &json!("calm"),
                &json!(1),
                &json!(cross)
            )
        );
        assert_eq!(
            page(&s, "Job.attempt_counts", Some(&day)),
            (Vec::new(), day.clone())
        );
        s.attempt_retention = Some(1);
        assert!(matches!(
            s.maintain(&crate::store::MaintenanceTask {
                name: "prune_attempts".into(),
                before: s.now - 1
            }),
            Ok(Ok(Ok(_)))
        ));
        assert_eq!(page(&s, "Job.attempt_counts", None).0, counts);
        let w = s.snapshot().unwrap();
        assert_eq!(
            export(&w, "Job.attempt_counts", Some("yesterday"))
                .unwrap()
                .refused
                .as_deref(),
            Some("invalid_cursor")
        );
        drop(w);
        // a type's transitions: its family's log, and no other type's
        let (rows, _) = page(&s, "Job.transitions", None);
        assert!(
            !rows.is_empty()
                && rows
                    .iter()
                    .all(|e| e["object_id"] == json!(calm) || e["object_id"] == json!(cross))
        );
        // an observation kind's records, and a type's labels, each without
        // its personal field
        let record = |s: &mut Store, type_: &str, inputs: Json| {
            let r = Request {
                transition: "record".into(),
                actor: OPERATOR.into(),
                object_id: None,
                type_: Some(type_.into()),
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
        let reading = record(
            &mut s,
            "Reading",
            json!({"subject": calm, "level": 3, "remark": "Dana's"}),
        );
        let label = record(
            &mut s,
            crate::model::LABEL,
            json!({"subject": calm, "name": "VIP", "note": "private"}),
        );
        let (rows, r1) = page(&s, "Reading", None);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (&rows[0]["id"], &rows[0]["attributes"]["level"]),
            (&json!(reading), &json!(3))
        );
        assert!(rows[0]["attributes"].get("remark").is_none());
        assert_eq!(page(&s, "Reading", Some(&r1)), (Vec::new(), r1));
        let (rows, _) = page(&s, "Job.labels", None);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (&rows[0]["id"], &rows[0]["attributes"]["name"]),
            (&json!(label), &json!("vip"))
        );
        assert!(rows[0]["attributes"].get("note").is_none());
        assert_eq!(page(&s, "Tool.labels", None).0, Vec::<Json>::new());
    }
}

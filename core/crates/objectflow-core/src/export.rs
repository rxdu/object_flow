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
use crate::store::{Attempt, CursoredSpan, Event};
use crate::value::timestamp_text;
use crate::wire;
use crate::world::{Object, World};

/// A page of an export (`library-api.md` §6): its rows, one JSON object per
/// line, and the cursor to continue from; or a refusal, a value, with no
/// rows.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportPage {
    pub lines: Vec<String>,
    pub cursor: String,
    pub refused: Option<String>,
    /// Whether the page ended at its limit or its bytes, so the next may
    /// hold more now; where it did not, it holds every row there is now
    /// (ninth production review N-9).
    pub more: bool,
}

/// The rows of a source after a cursor, exclusive, in cursor order: `log`,
/// `<Type>.intervals` or `<Type>.attempts`. A source naming no type the
/// store declares, or a cursor that is not one, is refused as a value; a
/// source the slice does not serve yet is not run.
pub fn export(world: &World<'_>, source: &str, cursor: Option<&str>) -> Result<ExportPage, NotRun> {
    export_page(world, source, cursor, usize::MAX)
}

/// A page of `export` (ADR-0179): rows after the cursor up to `limit`, and
/// every further row sharing the last one's cursor, so a page never ends
/// inside a cursor's rows, which the next page would pass over.
pub fn export_page(
    world: &World<'_>,
    source: &str,
    cursor: Option<&str>,
    limit: usize,
) -> Result<ExportPage, NotRun> {
    let refuse = |why: &str| {
        Ok(ExportPage {
            lines: Vec::new(),
            cursor: cursor.unwrap_or("0").to_string(),
            refused: Some(why.to_string()),
            more: false,
        })
    };
    // a type's attempt counts page by settled day, their cursor a date
    if let Some(type_) = source.strip_suffix(".attempt_counts")
        && world.lib.type_(type_).is_some()
    {
        return counts_page(world, type_, cursor, limit);
    }
    let after = match cursor {
        None => 0,
        Some(c) => match c.parse::<i64>() {
            Ok(n) if n >= 0 => n,
            _ => return refuse("invalid_cursor"),
        },
    };
    // a cursor past what the store has handed out is none of its own, where
    // a forged one answered an empty page and was kept, so a consumer waited
    // for ever unsaid (fourth production review A-12)
    // and one in a gap a restore skipped (ADR-0207 decision 5)
    let (last, skipped) = if source.ends_with(".attempts") {
        (world.last_txn()?, world.skipped_txn(after)?)
    } else {
        (world.last_position()?, world.skipped_position(after)?)
    };
    if after > last || skipped {
        return refuse("invalid_cursor");
    }
    let mut rows: Vec<(i64, Json)> = if source == "log" {
        // a page of the log read from the cursor, each event's object
        // fetched as its row is written, where every page read the whole log
        // and every object first (fourth production review FEED); a few
        // events at a time, to the page's bytes, where a page read its limit
        // of events whole first (tenth production review)
        log_rows(world, after, None, limit)?
    } else if world
        .lib
        .type_(source)
        .is_some_and(|t| t.observation_on.is_some())
    {
        // an observation kind's records, each by its latest event, a page
        // read by the index on it (the audit's cost B12)
        match world.ids_after(source, None, after, limit.saturating_add(1))? {
            Some(ids) => paged_rows(world, &ids)?,
            None => object_rows(world, world.of_type(source)?, after)?,
        }
    } else {
        let Some((type_, kind)) = source.split_once('.') else {
            return refuse("unknown_source");
        };
        if world.lib.type_(type_).is_none() {
            return refuse("unknown_source");
        }
        match kind {
            "intervals" => interval_rows(world, type_, after, limit)?,
            "attempts" => attempt_rows(world, type_, after, limit)?,
            // the log of the type's family, a page read from the cursor
            "transitions" => {
                let family: Vec<String> = world
                    .lib
                    .types
                    .keys()
                    .filter(|t| world.lib.is_a(t, type_))
                    .cloned()
                    .collect();
                log_rows(world, after, Some(&family), limit)?
            }
            // the labels on the type's family's objects
            "labels" => {
                let types: Vec<String> = world.family(type_).to_vec();
                if let Some(ids) = world.ids_after(
                    crate::model::LABEL,
                    Some(&types),
                    after,
                    limit.saturating_add(1),
                )? {
                    paged_rows(world, &ids)?
                } else {
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
            }
            _ => return refuse("unknown_source"),
        }
    };
    // in cursor order; rows sharing a cursor in the order written
    rows.sort_by_key(|(c, _)| *c);
    let mut more = rows.len() >= limit && limit > 0;
    if rows.len() > limit && limit > 0 {
        let cut = rows[limit - 1].0;
        rows.retain(|(c, _)| *c <= cut);
    }
    let mut lines: Vec<(i64, String)> = rows.iter().map(|(c, r)| (*c, r.to_string())).collect();
    // a page ends at its bytes as at its limit, never among the rows of one
    // cursor, which the next page would pass over: 1,000 rows of large
    // inputs answered 197 MB (ninth production review N-9)
    let mut bytes = 0;
    if let Some(fit) = lines.iter().position(|(_, l)| {
        bytes += l.len() + 1;
        bytes >= crate::reads::PAGE_BYTES
    }) {
        let cut = lines[fit].0;
        if lines.iter().any(|(c, _)| *c > cut) {
            lines.retain(|(c, _)| *c <= cut);
        }
        // reaching its bytes, the page says more may follow, where its rows
        // were read only up to them (tenth production review)
        more = true;
    }
    let next = lines.last().map_or(after, |(c, _)| *c);
    Ok(ExportPage {
        lines: lines.into_iter().map(|(_, l)| l).collect(),
        cursor: next.to_string(),
        refused: None,
        more,
    })
}

/// The log's rows after a position, of the types given or of any: at most
/// `limit`, read a few events at a time and ending once they reach a page's
/// bytes, so no page reads more events whole than it holds and one more
/// chunk (tenth production review).
fn log_rows(
    world: &World<'_>,
    after: i64,
    types: Option<&[String]>,
    limit: usize,
) -> Result<Vec<(i64, Json)>, NotRun> {
    const CHUNK: usize = 16;
    let mut rows = Vec::new();
    let mut bytes = 0;
    let mut at = after;
    while rows.len() < limit && bytes < crate::reads::PAGE_BYTES {
        let page = world.log_after(at, types, None, CHUNK.min(limit - rows.len()))?;
        let done = page.len() < CHUNK.min(limit - rows.len());
        for e in &page {
            at = e.position;
            let row = event_row(world, e)?;
            bytes += row.to_string().len() + 1;
            rows.push((e.position, row));
            if bytes >= crate::reads::PAGE_BYTES {
                break;
            }
        }
        if done {
            break;
        }
    }
    Ok(rows)
}

/// A type's attempt counts after the cursor's day, a day's once the day is
/// over (ADR-0160): `declaration-syntax.md` §6.9's rows, the rollup's counts
/// and the remaining rows' together, by day and then by key; the cursor is
/// the last day exported, `YYYY-MM-DD`, an empty one the start. A day's
/// counts never change once it is over, attempts being dated by when they
/// were recorded, and a prune moves a count without changing it.
fn counts_page(
    world: &World<'_>,
    type_: &str,
    cursor: Option<&str>,
    limit: usize,
) -> Result<ExportPage, NotRun> {
    let day_of = |text: &str| {
        crate::value::parse_timestamp(&format!("{text}T00:00:00Z")).map(|t| t.div_euclid(86_400))
    };
    let today = world.now.div_euclid(86_400);
    // a day not over yet, which no page has given, where one ahead of the
    // store's today answered nothing until then and passed over every day
    // before it (the audit's durability V5); and a day a restore recounts,
    // from the copy's last to the restore's (ADR-0207 decision 5)
    let restores = world.restores()?;
    let recounted = |d: i64| {
        restores.iter().any(|r| {
            (r.copy_last_at.div_euclid(86_400)..=r.restored_at.div_euclid(86_400)).contains(&d)
        })
    };
    let after = match cursor {
        None | Some("") => None,
        Some(c) => match day_of(c) {
            Some(d) if d < today && !recounted(d) => Some(d),
            _ => {
                return Ok(ExportPage {
                    lines: Vec::new(),
                    cursor: c.to_string(),
                    refused: Some("invalid_cursor".into()),
                    more: false,
                });
            }
        },
    };
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
    // a page ends at its limit or its bytes, never among one day's rows,
    // since the day is its cursor, where every settled day came in one page
    // (the audit's cost B13)
    let mut bytes = 0;
    let end = rows.iter().enumerate().find_map(|(i, (_, r))| {
        bytes += r.len() + 1;
        (i + 1 >= limit || bytes >= crate::reads::PAGE_BYTES).then_some(i)
    });
    let more = match end {
        Some(i) if i + 1 < rows.len() => {
            let day = rows[i].0;
            rows.retain(|(d, _)| *d <= day);
            true
        }
        _ => false,
    };
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
        more,
        refused: None,
    })
}

/// The rows of the objects given by id, each at its last event.
fn paged_rows(world: &World<'_>, ids: &[(String, i64)]) -> Result<Vec<(i64, Json)>, NotRun> {
    let mut rows = Vec::new();
    for (id, at) in ids {
        if let Some(o) = world.get(id)? {
            rows.push((*at, object_row(world, o)?));
        }
    }
    Ok(rows)
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
    // the object's type alone, where its whole object was held for the page,
    // a change's source of 1.9 MB with each of its events, 550 MB for a page
    // of 178 KB (eleventh production review)
    let mut type_ = world.type_of(&e.object_id)?;
    let mut transition = e.transition.clone();
    // a proposal's event holds the inputs of the transition it proposed,
    // judged under that transition, where they were judged under the
    // proposal's, which declares none, and its personal inputs were
    // exported (the audit's personal data R12)
    if type_.as_deref() == Some(crate::model::PROPOSAL)
        && let Some(p) = world.get(&e.object_id)?
        && let (crate::value::Value::Text(target), crate::value::Value::Text(proposed)) =
            (p.attr("target_type"), p.attr("transition"))
    {
        type_ = Some(target);
        transition = proposed;
    }
    let mut payload = e.payload.clone();
    for part in ["inputs", "corrected"] {
        if let Some(values) = payload.get_mut(part).and_then(Json::as_object_mut) {
            values.retain(|k, _| !personal(world.lib, type_.as_deref(), &transition, k));
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
            .is_some_and(|x| crate::request::input_personal(lib, t, x, name))
}

/// Each span of each object of the type, after the cursor, by the event that
/// last wrote it: the one that opened it, closed it, or relabelled it in a
/// correction; a legacy span, which no event opened, by the import event
/// that wrote its object (`library-api.md` §6). It reads the store's
/// intervals a window of positions at a time, by their index, to the page's
/// limit and bytes, the read's deadline and stop read between windows,
/// where a page read every event into memory and walked them once for each
/// of the type's objects, past its deadline and an erasure's cut (eleventh
/// production review N1). A window that finds nothing doubles, so a type
/// written seldom crosses the log in few reads.
fn interval_rows(
    world: &World<'_>,
    type_: &str,
    after: i64,
    limit: usize,
) -> Result<Vec<(i64, Json)>, NotRun> {
    const FIRST: i64 = 4_096;
    const WIDEST: i64 = 65_536;
    let family = world.family(type_);
    let last = world.last_position()?;
    let mut rows = Vec::new();
    let mut bytes = 0;
    let mut at = after;
    let mut window = FIRST;
    while at < last && rows.len() < limit && bytes < crate::reads::PAGE_BYTES {
        world.due()?;
        let upto = at.saturating_add(window).min(last);
        let spans = world.intervals_between(&family, at, upto)?;
        // each row a later commit deleted or moved, retracted at that
        // commit's cursor and before the rows it wrote there, so a consumer
        // folding the rows by their key holds what the store holds, where it
        // kept a merged span open and a renamed member's rows under the old
        // name (the audit's durability V7, ADR-0205 decision 9)
        let retracted = world.retractions_between(&family, at, upto)?;
        window = if spans.is_empty() && retracted.is_empty() {
            window.saturating_mul(2).min(WIDEST)
        } else {
            FIRST
        };
        let mut at_cursor: Vec<(i64, bool, Json)> = retracted
            .iter()
            .map(|r| {
                (
                    r.position,
                    false,
                    json!({
                        "retracted": true,
                        "object_id": r.object_id,
                        "dimension": r.dimension,
                        "entered_position": r.entered_position,
                        "entered_at": timestamp_text(r.entered_at),
                    }),
                )
            })
            .collect();
        for s in &spans {
            let personal = s.dimension != "state"
                && world
                    .lib
                    .type_(&s.type_)
                    .and_then(|t| t.attribute(&s.dimension))
                    .is_some_and(|a| a.personal);
            at_cursor.push((s.cursor, true, span_row(s, personal)));
        }
        // stable: each cursor's retractions first, then its rows as read
        at_cursor.sort_by_key(|(cursor, written, _)| (*cursor, *written));
        for (cursor, _, row) in at_cursor {
            bytes += row.to_string().len() + 1;
            rows.push((cursor, row));
        }
        at = upto;
    }
    Ok(rows)
}

/// The rows a page of a type's intervals held as it read them before the
/// eleventh production review, every event into memory and each object's
/// parts walked: the oracle the indexed read is compared with.
#[cfg(test)]
pub(crate) fn intervals_as_walked(world: &World<'_>, type_: &str) -> Vec<String> {
    let log: Vec<&Event> = world.log().unwrap().collect();
    let mut rows = Vec::new();
    let objects = world.of_type(type_).unwrap();
    for o in objects {
        let imported = log
            .iter()
            .find(|e| e.object_id == o.id && e.imported)
            .map_or(0, |e| e.position);
        let t = world.lib.type_(&o.type_).unwrap();
        for (dimension, spans) in &world.parts(o).unwrap().spans {
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
                if cursor == 0 {
                    continue;
                }
                let transition = log
                    .iter()
                    .find(|e| e.position == sp.entered_position)
                    .map(|e| e.transition.clone());
                let s = CursoredSpan {
                    cursor,
                    object_id: o.id.clone(),
                    type_: o.type_.clone(),
                    dimension: dimension.clone(),
                    span: sp.clone(),
                    transition,
                    order: o.order,
                };
                rows.push((cursor, span_row(&s, personal).to_string()));
            }
        }
    }
    rows.sort_by_key(|(c, _)| *c);
    rows.into_iter().map(|(_, r)| r).collect()
}

/// Whether a type's attempts, paged at each of several limits, give the
/// rows the whole attempt log filtered gives, as a page read them before the
/// eleventh production review, in its order.
#[cfg(test)]
pub(crate) fn assert_attempts_as_listed(s: &crate::store::Store, type_: &str) {
    let w = s.snapshot().unwrap();
    let listed: Vec<String> = w
        .attempts()
        .unwrap()
        .iter()
        .filter(|a| a.type_.as_deref().is_some_and(|t| w.lib.is_a(t, type_)))
        .map(|a| attempt_row(a).to_string())
        .collect();
    drop(w);
    assert!(listed.len() > 3, "{type_} has too few attempts to compare");
    let source = format!("{type_}.attempts");
    for limit in [1, 2, 3, usize::MAX] {
        let mut read = Vec::new();
        let mut cursor = "0".to_string();
        loop {
            let w = s.snapshot().unwrap();
            let page = export_page(&w, &source, Some(&cursor), limit).unwrap();
            assert_eq!(page.refused, None);
            if page.lines.is_empty() {
                break;
            }
            read.extend(page.lines);
            cursor = page.cursor;
        }
        assert_eq!(read, listed, "{source} paged by {limit}");
    }
}

/// Whether a type's intervals, paged at each of several limits, give the
/// rows the walk of [`intervals_as_walked`] gives, in its order, no page
/// ending among one cursor's rows, and reading neither the log nor a type
/// whole (eleventh production review N1).
#[cfg(test)]
pub(crate) fn assert_intervals_as_walked(s: &crate::store::Store, type_: &str) {
    let walked = intervals_as_walked(&s.snapshot().unwrap(), type_);
    assert!(!walked.is_empty(), "{type_} has no intervals to compare");
    let source = format!("{type_}.intervals");
    for limit in [1, 2, 3, usize::MAX] {
        let mut read = Vec::new();
        let mut cursor = "0".to_string();
        loop {
            let w = s.snapshot().unwrap();
            let page = export_page(&w, &source, Some(&cursor), limit).unwrap();
            assert_eq!(page.refused, None);
            assert!(!w.log_read_whole(), "a page of {source} read the log whole");
            if page.lines.is_empty() {
                break;
            }
            read.extend(page.lines);
            cursor = page.cursor;
        }
        assert_eq!(read, walked, "{source} paged by {limit}");
    }
}

/// A span as the export writes it, in `Interval`'s shape
/// (`library-api.md` §4), its value omitted where the member is personal.
fn span_row(s: &CursoredSpan, personal: bool) -> Json {
    let sp = &s.span;
    let mut row = json!({
        "object_id": s.object_id,
        "dimension": s.dimension,
        // with `entered_at`, the row's key, which a retraction names
        // (the audit's durability V8)
        "entered_position": sp.entered_position,
        "entered_at": timestamp_text(sp.entered_at),
        "left_at": sp.left_at.map(timestamp_text),
        "entered_by": sp.entered_by,
        "entered_by_kind": sp.entered_by_kind,
        "transition": if sp.legacy { None } else { s.transition.clone() },
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

/// A type's attempts after the cursor, by the transaction that wrote each: a
/// window of transactions at a time, read by their index a few attempts at a
/// time, to the page's limit and bytes and then to the end of the last
/// transaction read, so a page never ends among one transaction's attempts;
/// the read's deadline and stop read between windows. A page read the whole
/// attempt log, which one actor's refusals in a loop grow by millions a day
/// (eleventh production review). A window that finds nothing doubles.
fn attempt_rows(
    world: &World<'_>,
    type_: &str,
    after: i64,
    limit: usize,
) -> Result<Vec<(i64, Json)>, NotRun> {
    const CHUNK: usize = 16;
    const FIRST: i64 = 4_096;
    const WIDEST: i64 = 1 << 20;
    let family = world.family(type_);
    let last = world.last_txn()?;
    let mut rows = Vec::new();
    let mut bytes = 0;
    // the place read to, in the order written: past every attempt of `after`
    let mut from = (after, i64::MAX);
    let mut window = FIRST;
    let full = |rows: &Vec<(i64, Json)>, bytes: usize| {
        rows.len() >= limit || bytes >= crate::reads::PAGE_BYTES
    };
    while from.0 < last && !full(&rows, bytes) {
        world.due()?;
        let upto = from.0.saturating_add(window).min(last);
        let mut found = 0;
        loop {
            let chunk = world.attempts_after(&family, from, upto, CHUNK)?;
            for (id, a) in &chunk {
                from = (a.txn, *id);
                let row = attempt_row(a);
                bytes += row.to_string().len() + 1;
                rows.push((a.txn, row));
                found += 1;
            }
            if chunk.len() < CHUNK || full(&rows, bytes) {
                break;
            }
        }
        if full(&rows, bytes) {
            // the rest of the last transaction read, which the next page,
            // reading after it, would pass over
            for (_, a) in world.attempts_after(&family, from, from.0, usize::MAX)? {
                let row = attempt_row(&a);
                rows.push((a.txn, row));
            }
            break;
        }
        from = (upto, i64::MAX);
        window = if found == 0 {
            window.saturating_mul(2).min(WIDEST)
        } else {
            FIRST
        };
    }
    Ok(rows)
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
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
        };
        request(s, &r).unwrap().unwrap()
    }

    /// An export of an observation kind, or of a type's labels, reads a
    /// page by the index on each record's last event, never the whole log
    /// or every record, and gives, page by page, what reading them whole
    /// gave (the audit's cost B12).
    #[test]
    fn an_export_of_records_reads_a_page_by_its_index() {
        let lib = Library::load(&[("crews.yaml".into(), CREWS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let Verdict::Satisfied { object_id: job, .. } =
            req(&mut s, "open", None, json!({"crew": "NORTH"}))
        else {
            panic!("no job")
        };
        let mut ask = |tn: &str, inputs: Json| {
            let r = Request {
                transition: "record".into(),
                actor: OPERATOR.into(),
                object_id: None,
                type_: Some(tn.into()),
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
            assert!(matches!(
                request(&mut s, &r).unwrap().unwrap(),
                Verdict::Satisfied { .. }
            ));
        };
        for i in 0..12 {
            ask("Reading", json!({"subject": job, "level": i}));
            ask(
                crate::model::LABEL,
                json!({"subject": job, "name": format!("L{i}")}),
            );
        }
        for (source, kind) in [("Reading", "Reading"), ("Job.labels", crate::model::LABEL)] {
            // as the log, read whole, gave each record's last event
            let w = s.snapshot().unwrap();
            let mut walked = object_rows(&w, w.of_type(kind).unwrap(), 0).unwrap();
            walked.sort_by_key(|(c, _)| *c);
            let whole: Vec<Json> = walked.into_iter().map(|(_, r)| r).collect();
            drop(w);
            assert_eq!(whole.len(), 12, "{source}");
            let before = (
                crate::world::whole_log_reads(),
                crate::world::whole_type_reads(kind),
            );
            let (mut paged, mut cursor) = (Vec::new(), None::<String>);
            loop {
                let w = s.snapshot().unwrap();
                let p = export_page(&w, source, cursor.as_deref(), 5).unwrap();
                assert_eq!(p.refused, None);
                paged.extend(
                    p.lines
                        .iter()
                        .map(|l| serde_json::from_str::<Json>(l).unwrap()),
                );
                cursor = Some(p.cursor);
                if !p.more {
                    break;
                }
            }
            assert_eq!(paged, whole, "{source}");
            assert_eq!(
                (
                    crate::world::whole_log_reads(),
                    crate::world::whole_type_reads(kind)
                ),
                before,
                "{source}"
            );
        }
    }

    /// A type's attempt counts come in pages of `limit` rows, never ending
    /// among one day's, their cursor the last day, where every settled day
    /// came in one page (the audit's cost B13).
    #[test]
    fn attempt_counts_come_a_day_at_a_time() {
        let lib = Library::load(&[("crews.yaml".into(), CREWS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let Verdict::Satisfied {
            object_id: cross, ..
        } = req(
            &mut s,
            "open",
            None,
            json!({"crew": "NORTH", "mood": "CROSS"}),
        )
        else {
            panic!("no job")
        };
        for day in 0..3 {
            s.now = day * 86_400 + 100;
            assert!(matches!(
                req(&mut s, "start", Some(&cross), json!({})),
                Verdict::Unsatisfied { .. }
            ));
        }
        s.now = 3 * 86_400 + 100;
        let whole = page(&s, "Job.attempt_counts", None).0;
        assert_eq!(whole.len(), 3);
        let (mut paged, mut cursor) = (Vec::new(), None::<String>);
        for expected in [true, true, false] {
            let w = s.snapshot().unwrap();
            let p = export_page(&w, "Job.attempt_counts", cursor.as_deref(), 1).unwrap();
            assert_eq!((p.lines.len(), p.more), (1, expected), "{p:?}");
            paged.extend(
                p.lines
                    .iter()
                    .map(|l| serde_json::from_str::<Json>(l).unwrap()),
            );
            cursor = Some(p.cursor);
        }
        assert_eq!(paged, whole);
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

    /// A proposal's events leave out the personal inputs of the transition
    /// it proposed, as that transition's own events do, where they were
    /// judged under the proposal's type, which declares none, and exported
    /// (the audit's personal data R12).
    #[test]
    fn a_proposals_events_leave_its_transitions_personal_inputs_out() {
        let text = "\nmodule: crm\ncategories: [live]\ntypes:\n  Person:\n    attributes:\n      remark: { type: string, optional: true, personal: true }\n    states:\n      ON: { category: live }\n    transitions:\n      join: { kind: initial, to: ON }\n      note: { kind: internal, from: [ON], required_inputs: [remark], proposable: true }\n";
        let lib = Library::load(&[("crm.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let ask = |s: &mut Store, xn: &str, id: Option<&str>, propose: bool, inputs: Json| {
            let r = Request {
                transition: xn.into(),
                actor: OPERATOR.into(),
                object_id: id.map(str::to_string),
                type_: id.is_none().then(|| "Person".to_string()),
                inputs: inputs
                    .as_object()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
                occurred_at: None,
                propose,
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
        };
        let ana = ask(&mut s, "join", None, false, json!({}));
        ask(
            &mut s,
            "note",
            Some(&ana),
            true,
            json!({"remark": "Secret remark about her"}),
        );
        let (log, _) = page(&s, "log", None);
        let said = serde_json::to_string(&log).unwrap();
        assert!(!said.contains("Secret remark"), "{said}");
        let (proposed, _) = page(&s, "Proposal.transitions", None);
        let said = serde_json::to_string(&proposed).unwrap();
        assert!(!said.contains("Secret remark"), "{said}");
    }

    /// A transaction's attempts are never split between pages, however many
    /// there are: a request audited by twenty conditions records twenty in
    /// its transaction, more than a page reads at once (eleventh production
    /// review).
    #[test]
    fn a_transactions_attempts_are_read_whole_on_one_page() {
        let conditions: String = (0..20)
            .map(|i| format!("      c{i}:\n        description: Large.\n        expression: n > {}\n        remedy: self_serviceable\n", 100 + i))
            .collect();
        let guards: Vec<String> = (0..20).map(|i| format!("c{i}: audit")).collect();
        let text = format!(
            "module: gates\ncategories: [live]\ntypes:\n  Gate:\n    description: A gate.\n    tracking: record\n    attributes:\n      n: {{ type: int }}\n    states:\n      ON: {{ category: live, description: On. }}\n    conditions:\n{conditions}    transitions:\n      make: {{ kind: initial, to: ON, required_inputs: [n] }}\n      poke: {{ kind: internal, from: [ON], guards: {{ {} }} }}\n",
            guards.join(", ")
        );
        let lib = Library::load(&[("gates.yaml".into(), text)]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let ask = |s: &mut Store, xn: &str, object: Option<&str>, inputs: Json| {
            let r = Request {
                transition: xn.into(),
                actor: OPERATOR.into(),
                object_id: object.map(str::to_string),
                type_: object.is_none().then(|| "Gate".to_string()),
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
        let gate = match ask(&mut s, "make", None, json!({"n": 1})) {
            Verdict::Satisfied { object_id, .. } => object_id,
            other => panic!("{other:?}"),
        };
        for _ in 0..2 {
            assert!(matches!(
                ask(&mut s, "poke", Some(&gate), json!({})),
                Verdict::Satisfied { .. }
            ));
        }
        let w = s.snapshot().unwrap();
        let mut txns: BTreeMap<i64, usize> = BTreeMap::new();
        for a in w.attempts().unwrap() {
            *txns.entry(a.txn).or_default() += 1;
        }
        drop(w);
        assert!(txns.values().any(|n| *n > 16), "{txns:?}");
        assert_attempts_as_listed(&s, "Gate");
        // and a page of them stops at its read's deadline
        let mut w = s.snapshot().unwrap();
        w.deadline = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(1));
        assert!(matches!(
            export_page(&w, "Gate.attempts", None, 10),
            Err(NotRun::TimedOut(_))
        ));
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
        // a page of the log reads each event's object's type, never the
        // object, which a change's source made 550 MB for a page of 178 KB
        // (eleventh production review)
        let w = s.snapshot().unwrap();
        let p = export_page(&w, "log", None, 100).unwrap();
        assert!(!p.lines.is_empty());
        assert_eq!(w.held_count(), 0);
        drop(w);
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
        // read by the store's indexes, a page at a time, as the walk of every
        // event read them (eleventh production review N1)
        assert_intervals_as_walked(&s, "Job");
        // a page of intervals stops at its read's deadline and at an
        // erasure's cut, where a page read on past both, 45 s against a
        // deadline of 5 and an erased name left in the store's files
        let mut w = s.snapshot().unwrap();
        w.deadline = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(1));
        assert!(matches!(
            export_page(&w, "Job.intervals", None, 10),
            Err(NotRun::TimedOut(_))
        ));
        w.deadline = None;
        w.stop = Some((std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1)), 0));
        assert!(matches!(
            export_page(&w, "Job.intervals", None, 10),
            Err(NotRun::TimedOut(_))
        ));
        drop(w);
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
            target_version: None,
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
            s.maintain(
                crate::model::OPERATOR,
                &crate::store::MaintenanceTask {
                    name: "prune_attempts".into(),
                    before: s.now - 1
                }
            ),
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
        // a day not over, today's or one ahead, is no cursor a page gave,
        // where one ahead answered nothing until then and passed over every
        // day before it (the audit's durability V5)
        let today = crate::value::timestamp_text(w.now)[..10].to_string();
        for ahead in [today.as_str(), "2999-01-01"] {
            assert_eq!(
                export(&w, "Job.attempt_counts", Some(ahead))
                    .unwrap()
                    .refused
                    .as_deref(),
                Some("invalid_cursor"),
                "{ahead}"
            );
        }
        drop(w);
        // a type's transitions: its family's log, and no other type's
        let (rows, _) = page(&s, "Job.transitions", None);
        assert!(
            !rows.is_empty()
                && rows
                    .iter()
                    .all(|e| e["object_id"] == json!(calm) || e["object_id"] == json!(cross))
        );
        // read by the transaction's index, a page at a time, as the whole
        // attempt log filtered read them (eleventh production review)
        for _ in 0..4 {
            assert!(matches!(
                req(&mut s, "start", Some(&cross), json!({})),
                Verdict::Unsatisfied { .. }
            ));
        }
        assert_attempts_as_listed(&s, "Job");
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
                target_version: None,
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

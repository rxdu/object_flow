//! The store, on SQLite, laid out as `docs/design/storage-schema.md` §2 and §3
//! say: the directory `of_object`, the log `of_event`, the counters, one
//! table of typed columns per declared type, and a side table per
//! set-valued attribute (ADR-0153 decision 4).
//!
//! A request reads a snapshot of every object inside its transaction, decides
//! on it, and writes what changed in the same transaction (`DESIGN.md` §6).
//! The schema is the second line of defence: the request has already refused
//! what a constraint would, and a constraint that fires is a fault in the
//! core, reported, never a verdict.

use std::collections::BTreeMap;
use std::fmt;

use rusqlite::types::Value as Sql;
use rusqlite::{Connection, Transaction, params};

use crate::eval::NotRun;
use crate::model::{Attribute, DECLARATION_CHANGE, Declared, Library, OPERATOR, Remedy, Type};
use crate::publish::PublishReport;
use crate::request::{failing_invariants, invariant_reads, resolve_actor};
use crate::value::{
    Decimal, Value, date_text, minor_unit, parse_date, parse_timestamp, timestamp_text,
};
use crate::verdict::Verdict;
use crate::world::{Object, Span, World};

/// A fault of the store: the database, or a row that does not read back.
#[derive(Debug)]
pub struct StoreError(pub String);

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the store: {}", self.0)
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> StoreError {
        StoreError(e.to_string())
    }
}

type Stored<T> = Result<T, StoreError>;

/// An event, as the log holds it (`library-api.md` §5).
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub position: i64,
    pub object_id: String,
    pub object_seq: i64,
    pub transition: String,
    pub from_state: Option<String>,
    pub to_state: String,
    pub changes_state: bool,
    pub source: String,
    pub actor_id: String,
    pub actor_kind: String,
    /// The principal the actor acted for, where the request named one.
    pub principal: Option<String>,
    pub declaration_version: i64,
    pub recorded_at: i64,
    pub occurred_at: Option<i64>,
    pub imported: bool,
    pub payload: serde_json::Value,
}

/// What a request writes: the objects it created or changed, each with its
/// event, in the order the events were allocated.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Change {
    pub writes: Vec<(Object, bool, Event)>,
    /// The write index's rows the request sets: an object, what it wrote,
    /// and the event's position, written once every event is.
    pub index: Vec<(String, String, i64)>,
    /// The admissions it records: an object, the invariant, the event and
    /// the reason (ADR-0054).
    pub admitted: Vec<(String, String, i64, String)>,
    /// The admissions it discharges, the invariant holding again: an
    /// object, the invariant, and the event that discharged it.
    pub discharged: Vec<(String, String, i64)>,
    /// What an erasure redacts beyond the object's row: for an object, its
    /// erased attributes in every past event's payload, and its personal
    /// attributes' values in the interval index (§9, steps 2 and 9).
    pub redacted: Vec<(String, Vec<String>)>,
    /// The spans a correction merged into the one before them, which leave
    /// the index: an object, a dimension, and the span's entered position
    /// and time (`declaration-syntax.md` §6.4).
    pub merged: Vec<(String, String, i64, i64)>,
    /// Spans written beside any event: an object, a dimension and the span,
    /// such as a publish opens on a member it newly tracks (ADR-0155).
    pub spans: Vec<(String, String, Span)>,
    /// External identifiers it keeps: source, value, object and attribute,
    /// a legacy key's attribute empty (ADR-0154).
    pub external: Vec<(String, String, String, String)>,
    /// Sequences it raises past a value, never lowering one: sequence, scope
    /// and value (ADR-0154).
    pub raised: Vec<(String, String, i64)>,
    /// The attempt log's rows written with the events they are linked to: a
    /// raised flag and an observing clause's would-be refusal (`DESIGN.md`
    /// §7, `storage-schema.md` §6).
    pub attempts: Vec<Attempt>,
}

/// A row of the attempt log (`library-api.md` §5, `of_attempt`): a request
/// that did not apply, an observing clause's would-be refusal, or a raised
/// flag. It holds no personal value: a personal input is named in
/// `withheld` and its value left out (ADR-0105).
#[derive(Clone, Debug, PartialEq)]
pub struct Attempt {
    pub at: i64,
    /// As the request named it where the store holds no such actor.
    pub actor_id: String,
    pub actor_kind: String,
    /// The principal the actor acted for: resolved, or as the request named
    /// it where the store holds none.
    pub principal: Option<String>,
    /// None where the request named an unknown id.
    pub type_: Option<String>,
    /// None for a refused creation, and for an unknown id.
    pub object_id: Option<String>,
    pub transition: Option<String>,
    pub verdict: String,
    pub clause: Option<String>,
    /// Empty where the core cannot tell it, as for an invariant's whose
    /// object is another than the one requested (ADR-0151).
    pub remedy: String,
    pub unknown: bool,
    pub enforced: bool,
    pub flagged: bool,
    pub applied_position: Option<i64>,
    /// The request's non-personal inputs and its occurred time, and for an
    /// approval its request refused, the proposal it executed.
    pub request_values: serde_json::Value,
    pub withheld: Vec<String>,
    pub declaration_version: i64,
}

fn insert_attempt(tx: &Connection, a: &Attempt, txn: &str) -> Stored<()> {
    let values = serde_json::json!({ "values": a.request_values, "withheld": a.withheld });
    tx.execute(
        &format!(
            "INSERT INTO of_attempt (at, actor_id, actor_kind, txn, type, object_id, transition, verdict, clause, remedy, \
             unknown, enforced, flagged, applied_position, request_values, declaration_version, actor_principal) \
             VALUES (?1, ?2, ?3, {txn}, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)"
        ),
        params![
            timestamp_text(a.at),
            a.actor_id,
            a.actor_kind,
            a.type_,
            a.object_id,
            a.transition,
            a.verdict,
            a.clause,
            a.remedy,
            i64::from(a.unknown),
            i64::from(a.enforced),
            i64::from(a.flagged),
            a.applied_position,
            values.to_string(),
            a.declaration_version,
            a.principal,
        ],
    )?;
    Ok(())
}

fn attempt_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Attempt> {
    let at: String = r.get(0)?;
    let values: Option<String> = r.get(13)?;
    let values: serde_json::Value = values
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or(serde_json::Value::Null);
    Ok(Attempt {
        at: parse_timestamp(&at).unwrap_or(0),
        actor_id: r.get(1)?,
        actor_kind: r.get(2)?,
        principal: r.get(15)?,
        type_: r.get(3)?,
        object_id: r.get(4)?,
        transition: r.get(5)?,
        verdict: r.get(6)?,
        clause: r.get(7)?,
        remedy: r.get(8)?,
        unknown: r.get::<_, i64>(9)? != 0,
        enforced: r.get::<_, i64>(10)? != 0,
        flagged: r.get::<_, i64>(11)? != 0,
        applied_position: r.get(12)?,
        request_values: values["values"].clone(),
        withheld: values["withheld"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|w| w.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        declaration_version: r.get(14)?,
    })
}

fn table(type_: &str) -> String {
    format!("t_{}", type_.to_lowercase())
}

/// The table a reference's foreign key names: its target's, or, for a
/// target with subtypes, whose objects live in their own types' tables, the
/// directory, which holds every object (§3.1, D520).
fn referenced(lib: &Library, target: &str) -> String {
    let family = lib.types.keys().any(|n| n != target && lib.is_a(n, target));
    // a label's subject may be of any type
    if family || target == crate::model::ANY_TYPE {
        "of_object".to_string()
    } else {
        table(target)
    }
}

/// A type's table as a version declares it: its columns, its constraints,
/// its set tables and its indexes (§3). Relaxed, it has no `NOT NULL` on an
/// attribute and no `CHECK` but `state_source`'s, the shape a publish's
/// mappings write through between two versions (ADR-0155).
struct TableParts {
    name: String,
    columns: Vec<(String, String)>,
    constraints: Vec<String>,
    sets: Vec<(String, String)>,
    indexes: Vec<String>,
}

impl TableParts {
    /// The DDL creating it under a name, with extra columns beside its own.
    fn create(&self, under: &str, extra: &[(String, String)]) -> String {
        let mut cols: Vec<String> = self
            .columns
            .iter()
            .chain(extra)
            .map(|(_, def)| def.clone())
            .collect();
        cols.extend(self.constraints.iter().cloned());
        let mut ddl = format!("CREATE TABLE {under} (\n  {}\n);\n", cols.join(",\n  "));
        if under == self.name {
            for i in &self.indexes {
                ddl.push_str(i);
                ddl.push('\n');
            }
            for (_, set) in &self.sets {
                ddl.push_str(set);
                ddl.push('\n');
            }
        }
        ddl
    }
}

fn table_parts(lib: &Library, t: &Type, relaxed: bool) -> TableParts {
    let name = table(&t.name);
    let states = t
        .states
        .iter()
        .map(|(s, _)| literal(s))
        .collect::<Vec<_>>()
        .join(",");
    let mut columns: Vec<(String, String)> = [
        ("id", "id TEXT PRIMARY KEY REFERENCES of_object(id)"),
        ("state", "state TEXT NOT NULL"),
        ("version", "version INTEGER NOT NULL"),
        (
            "declaration_version",
            "declaration_version INTEGER NOT NULL",
        ),
        (
            "last_event",
            "last_event INTEGER NOT NULL REFERENCES of_event(position)",
        ),
        (
            "state_source",
            "state_source TEXT NOT NULL DEFAULT 'observed'",
        ),
    ]
    .iter()
    .map(|(n, d)| (n.to_string(), d.to_string()))
    .collect();
    // present only on a type with a superseding state (§3)
    if t.states.iter().any(|(_, s)| s.superseding) {
        columns.push((
            "superseded_by".into(),
            "superseded_by TEXT REFERENCES of_object(id) DEFERRABLE INITIALLY DEFERRED".into(),
        ));
    }
    let mut constraints = vec![format!(
        "CONSTRAINT {} CHECK (state_source IN ('observed','asserted','imported'))",
        quote(&format!("{name}_state_source"))
    )];
    if !relaxed {
        constraints.insert(
            0,
            format!(
                "CONSTRAINT {} CHECK (state IN ({states}))",
                quote(&format!("{name}_state"))
            ),
        );
    }
    let mut sets = Vec::new();
    let mut indexes = vec![format!(
        "CREATE INDEX {} ON {name} (state);",
        quote(&format!("{name}_state_ix"))
    )];
    for (n, a) in &t.attributes {
        if a.derived_end() {
            continue;
        }
        if a.many {
            let target = match &a.declared {
                Declared::Reference(target) if lib.types.contains_key(target) => format!(
                    " REFERENCES {}(id) DEFERRABLE INITIALLY DEFERRED",
                    referenced(lib, target)
                ),
                _ => String::new(),
            };
            let set = format!("{name}__{n}");
            sets.push((
                set.clone(),
                format!(
                    "CREATE TABLE IF NOT EXISTS {} (object_id TEXT NOT NULL REFERENCES {name}(id), ordinal INTEGER NOT NULL, \
                     value {} NOT NULL{target}, PRIMARY KEY (object_id, ordinal));",
                    quote(&set),
                    sql_type(&a.declared)
                ),
            ));
            continue;
        }
        let col = column(n, a);
        let required = !a.optional && a.default.is_none() && base(&a.declared) != "counter";
        let mut def = format!("{} {}", quote(&col), sql_type(&a.declared));
        if base(&a.declared) == "counter" {
            def.push_str(" NOT NULL DEFAULT 0");
        } else if required && !relaxed {
            def.push_str(" NOT NULL");
        }
        match &a.declared {
            Declared::Reference(target)
                if lib.types.contains_key(target) || target == crate::model::ANY_TYPE =>
            {
                def.push_str(&format!(
                    " REFERENCES {}(id) DEFERRABLE INITIALLY DEFERRED",
                    referenced(lib, target)
                ));
                indexes.push(format!(
                    "CREATE INDEX {} ON {name} ({});",
                    quote(&format!("{name}_{col}_ix")),
                    quote(&col)
                ));
            }
            Declared::Scalar(e) => {
                if let Some(members) = lib.enumerations.get(e)
                    && !relaxed
                {
                    let ms = members
                        .iter()
                        .map(|m| literal(m))
                        .collect::<Vec<_>>()
                        .join(",");
                    constraints.push(format!(
                        "CONSTRAINT {} CHECK ({} IS NULL OR {} IN ({ms}))",
                        quote(&format!("{name}_{n}")),
                        quote(&col),
                        quote(&col)
                    ));
                }
            }
            Declared::Reference(_) => {}
        }
        columns.push((col, def));
    }
    TableParts {
        name,
        columns,
        constraints,
        sets,
        indexes,
    }
}

/// A span a publish opens on a member it newly tracks, holding its current
/// value, marked migrated (ADR-0146).
fn opened(
    o: &Object,
    dimension: &str,
    position: i64,
    now: i64,
    who: &crate::eval::Who,
    version: i64,
) -> Span {
    Span {
        value: if dimension == "state" {
            Value::State {
                type_: o.type_.clone(),
                name: o.state.clone(),
            }
        } else {
            o.attr(dimension)
        },
        entered_position: position,
        left_position: None,
        entered_at: now,
        left_at: None,
        entered_by: Some(who.id.clone()),
        entered_by_kind: who.kind.clone(),
        migrated: true,
        declaration_version: version,
        legacy: false,
        corrected_by: None,
    }
}

/// An event a publish records on an object: its spans for what changed,
/// marked migrated, its version, and the event (`publish-and-import.md` §3).
#[allow(clippy::too_many_arguments)]
fn event_of(
    o: &mut Object,
    before: Option<&Vec<(String, Value)>>,
    from_state: Option<String>,
    transition: &str,
    position: i64,
    now: i64,
    who: &crate::eval::Who,
    source: &str,
    declaration_version: i64,
    opened_under: i64,
    payload: serde_json::Value,
) -> Event {
    if let Some(before) = before {
        for (d, was) in before {
            let v = if d == "state" {
                Value::State {
                    type_: o.type_.clone(),
                    name: o.state.clone(),
                }
            } else {
                o.attr(d)
            };
            if *was == v {
                continue;
            }
            let spans = o.spans.entry(d.clone()).or_default();
            if let Some(last) = spans.last_mut()
                && last.left_position.is_none()
            {
                last.left_position = Some(position);
                last.left_at = Some(now);
            }
            spans.push(Span {
                value: v,
                entered_position: position,
                left_position: None,
                entered_at: now,
                left_at: None,
                entered_by: Some(who.id.clone()),
                entered_by_kind: who.kind.clone(),
                migrated: source == "migrated",
                declaration_version: opened_under,
                legacy: false,
                corrected_by: None,
            });
        }
    }
    o.version += 1;
    Event {
        position,
        object_id: o.id.clone(),
        object_seq: o.version,
        transition: transition.to_string(),
        changes_state: from_state.as_deref() != Some(o.state.as_str()),
        from_state,
        to_state: o.state.clone(),
        source: source.to_string(),
        actor_id: who.id.clone(),
        actor_kind: who.kind.clone(),
        principal: who.principal.clone(),
        declaration_version,
        recorded_at: now,
        occurred_at: Some(now),
        imported: false,
        payload,
    }
}

/// One span of the interval index, inserted or brought up to date.
fn upsert_span(tx: &Transaction<'_>, id: &str, dimension: &str, sp: &Span) -> Stored<()> {
    tx.execute(
        "INSERT INTO of_interval (object_id, dimension, value, entered_position, left_position, \
         entered_at, left_at, entered_by, entered_by_kind, declaration_version, migrated, legacy, corrected_by) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?11, ?10, ?12, ?13) \
         ON CONFLICT (object_id, dimension, entered_position, entered_at) DO UPDATE SET \
         value = excluded.value, left_position = excluded.left_position, left_at = excluded.left_at, \
         corrected_by = excluded.corrected_by",
        params![
            id,
            dimension,
            to_sql(&sp.value)?,
            sp.entered_position,
            sp.left_position,
            timestamp_text(sp.entered_at),
            sp.left_at.map(timestamp_text),
            sp.entered_by,
            sp.entered_by_kind,
            i64::from(sp.migrated),
            sp.declaration_version,
            i64::from(sp.legacy),
            sp.corrected_by,
        ],
    )?;
    Ok(())
}

/// A table brought to a version's shape (§10): a new copy of its columns,
/// relaxed or exact, every column the old one held that the version does
/// not, a dropped attribute's and a set-aside's, kept for history, the rows
/// copied by column name, the old table dropped and the copy renamed. The
/// caller holds foreign keys off, and checks them before committing.
fn rebuild(tx: &Transaction<'_>, lib: &Library, t: &Type, relaxed: bool) -> Stored<()> {
    let parts = table_parts(lib, t, relaxed);
    let name = parts.name.clone();
    let mut existing: Vec<(String, String)> = Vec::new();
    {
        let mut info = tx.prepare(&format!("PRAGMA table_info({})", quote(&name)))?;
        let rows = info.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        for row in rows {
            existing.push(row?);
        }
    }
    let own: Vec<&String> = parts.columns.iter().map(|(c, _)| c).collect();
    let kept: Vec<(String, String)> = existing
        .iter()
        .filter(|(c, _)| !own.contains(&c))
        .map(|(c, ty)| (c.clone(), format!("{} {ty}", quote(c))))
        .collect();
    let copied: Vec<String> = existing
        .iter()
        .map(|(c, _)| c.clone())
        .filter(|c| own.contains(&c) || kept.iter().any(|(k, _)| k == c))
        .map(|c| quote(&c))
        .collect();
    let copy = format!("{name}__rebuild");
    tx.execute_batch(&parts.create(&copy, &kept))?;
    tx.execute_batch(&format!(
        "INSERT INTO {copy} ({cols}) SELECT {cols} FROM {name}; DROP TABLE {name}; ALTER TABLE {copy} RENAME TO {name};",
        cols = copied.join(", ")
    ))?;
    for i in &parts.indexes {
        tx.execute_batch(i)?;
    }
    for (_, set) in &parts.sets {
        tx.execute_batch(set)?;
    }
    Ok(())
}

fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

fn base(declared: &Declared) -> &str {
    match declared {
        Declared::Scalar(t) | Declared::Reference(t) => t,
    }
}

/// The column an attribute is stored in: its own name, or `<name>_id` for a
/// reference (§3.2).
fn column(name: &str, a: &Attribute) -> String {
    match a.declared {
        Declared::Reference(_) => format!("{name}_id"),
        Declared::Scalar(_) => name.to_string(),
    }
}

/// A value as its column holds it (§3.1): a decimal scaled to an integer,
/// money in minor units, a timestamp and a date as ISO-8601 text.
fn to_sql(v: &Value) -> Stored<Sql> {
    Ok(match v {
        Value::Absent => Sql::Null,
        Value::Bool(b) => Sql::Integer(i64::from(*b)),
        Value::Int(n) | Value::Duration(n) => Sql::Integer(*n),
        Value::Decimal(d) | Value::Money { amount: d, .. } => Sql::Integer(
            i64::try_from(d.units())
                .map_err(|_| StoreError("a decimal past 64 bits".to_string()))?,
        ),
        Value::Timestamp(t) => Sql::Text(timestamp_text(*t)),
        Value::Date(d) => Sql::Text(date_text(*d)),
        Value::Text(s) | Value::Object(s) => Sql::Text(s.clone()),
        Value::State { name, .. } => Sql::Text(name.clone()),
        Value::Member { member, .. } => Sql::Text(member.clone()),
        Value::Set(_) => return Err(StoreError("a set in a column".to_string())),
        Value::Row(_) => return Err(StoreError("a row of flow data in a column".to_string())),
    })
}

/// A column's content read back as a value of its declared type.
fn from_sql(v: Sql, declared: &Declared, lib: &Library) -> Stored<Value> {
    let bad = || StoreError(format!("a column of {declared:?} holds {v:?}"));
    if matches!(v, Sql::Null) {
        return Ok(Value::Absent);
    }
    match declared {
        Declared::Reference(_) => match &v {
            Sql::Text(s) => Ok(Value::Object(s.clone())),
            _ => Err(bad()),
        },
        Declared::Scalar(t) => {
            if lib.enumerations.contains_key(t.as_str()) {
                return match &v {
                    Sql::Text(s) => Ok(Value::Member {
                        enumeration: t.clone(),
                        member: s.clone(),
                    }),
                    _ => Err(bad()),
                };
            }
            match (t.as_str(), &v) {
                ("bool", Sql::Integer(n)) => Ok(Value::Bool(*n != 0)),
                ("int" | "counter" | "event", Sql::Integer(n)) => Ok(Value::Int(*n)),
                ("duration", Sql::Integer(n)) => Ok(Value::Duration(*n)),
                ("timestamp", Sql::Text(s)) => {
                    parse_timestamp(s).map(Value::Timestamp).ok_or_else(bad)
                }
                ("date", Sql::Text(s)) => parse_date(s).map(Value::Date).ok_or_else(bad),
                (t, Sql::Integer(n)) if t.starts_with("decimal(") => {
                    let scale = t
                        .trim_end_matches(')')
                        .rsplit(',')
                        .next()
                        .and_then(|s| s.trim().parse::<u8>().ok())
                        .ok_or_else(bad)?;
                    Decimal::new(i128::from(*n), scale)
                        .map(Value::Decimal)
                        .ok_or_else(bad)
                }
                (t, Sql::Integer(n)) if t.starts_with("money(") => {
                    let currency = t.trim_start_matches("money(").trim_end_matches(')');
                    let minor = minor_unit(currency).ok_or_else(bad)?;
                    Decimal::new(i128::from(*n), minor)
                        .map(|amount| Value::Money {
                            currency: currency.to_string(),
                            amount,
                        })
                        .ok_or_else(bad)
                }
                (_, Sql::Text(s)) => Ok(Value::Text(s.clone())),
                _ => Err(bad()),
            }
        }
    }
}

/// The SQL type a declared type's column takes (§3.1).
fn sql_type(declared: &Declared) -> &'static str {
    match declared {
        Declared::Reference(_) => "TEXT",
        Declared::Scalar(t) => match t.as_str() {
            "bool" | "int" | "counter" | "duration" | "event" => "INTEGER",
            t if t.starts_with("decimal(") || t.starts_with("money(") => "INTEGER",
            _ => "TEXT",
        },
    }
}

const SCHEMA: &str = "
CREATE TABLE of_object (
  id              TEXT    PRIMARY KEY,
  type            TEXT    NOT NULL,
  created_at      TEXT    NOT NULL,
  created_by_kind TEXT    NOT NULL,
  created_dated   INTEGER NOT NULL DEFAULT 1,
  recorded_from   TEXT    NOT NULL
);
CREATE TABLE of_event (
  position            INTEGER PRIMARY KEY,
  txn                 INTEGER NOT NULL,
  object_id           TEXT    NOT NULL REFERENCES of_object(id),
  object_seq          INTEGER NOT NULL,
  transition          TEXT    NOT NULL,
  from_state          TEXT,
  to_state            TEXT    NOT NULL,
  changes_state       INTEGER NOT NULL,
  source              TEXT    NOT NULL,
  actor_id            TEXT    NOT NULL,
  actor_kind          TEXT    NOT NULL,
  actor_principal     TEXT,
  context             TEXT,
  cause_position      INTEGER REFERENCES of_event(position),
  declaration_version INTEGER NOT NULL,
  taint_version       INTEGER NOT NULL,
  recorded_at         TEXT    NOT NULL,
  occurred_at         TEXT,
  imported            INTEGER NOT NULL DEFAULT 0,
  retries             INTEGER NOT NULL DEFAULT 0,
  payload             TEXT    NOT NULL,
  reads               TEXT    NOT NULL,
  CONSTRAINT of_event_per_object UNIQUE (object_id, object_seq),
  CONSTRAINT of_event_source CHECK (source IN
    ('observed','asserted','migrated','corrected','erased'))
);
CREATE INDEX of_event_by_object ON of_event (object_id, object_seq);
CREATE INDEX of_event_by_cause  ON of_event (cause_position);
CREATE INDEX of_event_by_cursor ON of_event (txn, position);
CREATE TABLE of_counter (
  name   TEXT    PRIMARY KEY,
  value  INTEGER NOT NULL
);
INSERT INTO of_counter (name, value) VALUES ('position', 0), ('txn', 0), ('object', 0);
-- The attempt log: every request that did not apply, every observing
-- clause's would-be refusal, and every raised flag. No personal value
-- (storage-schema.md §6; the rollup a prune fills is not kept, since
-- nothing prunes).
CREATE TABLE of_attempt (
  id                  INTEGER PRIMARY KEY,
  at                  TEXT    NOT NULL,
  actor_id            TEXT    NOT NULL,
  actor_kind          TEXT    NOT NULL,
  actor_principal     TEXT,
  context             TEXT,
  txn                 INTEGER NOT NULL,
  type                TEXT,
  object_id           TEXT,
  transition          TEXT,
  verdict             TEXT    NOT NULL,
  clause              TEXT,
  remedy              TEXT    NOT NULL,
  unknown             INTEGER NOT NULL DEFAULT 0,
  enforced            INTEGER NOT NULL DEFAULT 1,
  flagged             INTEGER NOT NULL DEFAULT 0,
  applied_position    INTEGER,
  reads               TEXT,
  consulted           TEXT,
  request_values      TEXT,
  declaration_version INTEGER NOT NULL
);
CREATE INDEX of_attempt_by_rule ON of_attempt (type, transition, clause, at);
CREATE INDEX of_attempt_by_actor ON of_attempt (actor_id, at);
CREATE TABLE of_declaration (
  version       INTEGER PRIMARY KEY,
  module        TEXT    NOT NULL,
  source        TEXT    NOT NULL,
  source_ref    TEXT,
  source_digest TEXT,
  builtins      TEXT    NOT NULL,
  published_at  TEXT    NOT NULL,
  published_by  TEXT    NOT NULL,
  change_id     TEXT,
  release       TEXT,
  report        TEXT    NOT NULL
);
CREATE TABLE of_external_id (
  source     TEXT NOT NULL,
  value      TEXT NOT NULL,
  object_id  TEXT NOT NULL REFERENCES of_object(id),
  attribute  TEXT NOT NULL,
  PRIMARY KEY (source, value)
);
CREATE TABLE of_admission (
  object_id           TEXT    NOT NULL REFERENCES of_object(id),
  invariant           TEXT    NOT NULL,
  position            INTEGER NOT NULL REFERENCES of_event(position),
  reason              TEXT    NOT NULL,
  discharged_position INTEGER REFERENCES of_event(position),
  PRIMARY KEY (object_id, invariant, position)
);
CREATE INDEX of_admission_by_object ON of_admission (object_id);
CREATE TABLE of_attribute_write (
  object_id  TEXT    NOT NULL REFERENCES of_object(id),
  attribute  TEXT    NOT NULL,
  position   INTEGER NOT NULL REFERENCES of_event(position),
  PRIMARY KEY (object_id, attribute)
);
CREATE TABLE of_interval (
  object_id           TEXT    NOT NULL REFERENCES of_object(id),
  dimension           TEXT    NOT NULL,
  value               TEXT,
  entered_position    INTEGER NOT NULL,
  left_position       INTEGER,
  entered_at          TEXT    NOT NULL,
  left_at             TEXT,
  entered_by          TEXT,
  entered_by_kind     TEXT    NOT NULL,
  declaration_version INTEGER NOT NULL,
  legacy              INTEGER NOT NULL DEFAULT 0,
  migrated            INTEGER NOT NULL DEFAULT 0,
  corrected_by        INTEGER REFERENCES of_event(position),
  PRIMARY KEY (object_id, dimension, entered_position, entered_at)
);
CREATE INDEX of_interval_current ON of_interval (dimension, value) WHERE left_position IS NULL;
CREATE INDEX of_interval_entered ON of_interval (dimension, entered_at) WHERE left_position IS NULL;
CREATE INDEX of_interval_by_object ON of_interval (object_id, dimension, left_position);
CREATE TABLE of_sequence (
  sequence TEXT    NOT NULL,
  scope    TEXT    NOT NULL,                        -- '' for a sequence with no scope
  value    INTEGER NOT NULL,
  PRIMARY KEY (sequence, scope)
);
";

/// A store and the library its rules come from.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
    pub lib: Library,
    /// The store's clock, in seconds since 1970: a sandbox's caller holds it
    /// (PRD F13).
    pub now: i64,
    /// What its evaluators answer, `<evaluator>.<function>` to `satisfied`,
    /// `unsatisfied` or `unanswered`: a sandbox's caller sets them (PRD F13).
    pub answers: BTreeMap<String, String>,
}

/// The built-in module a version was published with: this engine release.
pub const BUILTINS: &str = concat!("objectflow-core ", env!("CARGO_PKG_VERSION"));

impl Store {
    /// An in-memory store for a sandbox, holding the library's types and its
    /// operator, created at `now`.
    pub fn sandbox(lib: Library, now: i64) -> Stored<Store> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        conn.execute_batch(SCHEMA)?;
        let mut store = Store {
            conn,
            lib,
            now,
            answers: BTreeMap::new(),
        };
        for t in store.lib.types.values() {
            store.conn.execute_batch(&store.type_ddl(t))?;
        }
        store.create_operator()?;
        // version 0, the built-ins the store is created with, and version 1,
        // the modules a sandbox's load installs directly (ADR-0155 decision 4)
        let at = timestamp_text(now);
        store.conn.execute(
            "INSERT INTO of_declaration (version, module, source, builtins, published_at, published_by, report) \
             VALUES (0, '', '{}', ?1, ?2, ?1, '{}')",
            params![BUILTINS, at],
        )?;
        let source: serde_json::Map<String, serde_json::Value> = store
            .lib
            .source
            .iter()
            .map(|(n, t)| (n.clone(), serde_json::Value::String(t.clone())))
            .collect();
        store.conn.execute(
            "INSERT INTO of_declaration (version, module, source, source_ref, source_digest, builtins, \
             published_at, published_by, report) VALUES (1, ?1, ?2, 'sandbox:load', ?3, ?4, ?5, ?6, '{}')",
            params![
                store.lib.modules.last().cloned().unwrap_or_default(),
                serde_json::Value::Object(source).to_string(),
                crate::source::digest(&store.lib.source),
                BUILTINS,
                at,
                OPERATOR
            ],
        )?;
        store.lib.version = 1;
        Ok(store)
    }

    /// The DDL of a type's table and its set tables (§3, §3.3).
    fn type_ddl(&self, t: &Type) -> String {
        let parts = table_parts(&self.lib, t, false);
        parts.create(&table(&t.name), &[])
    }

    /// The store's first actor, the operator (ADR-0126).
    fn create_operator(&self) -> Stored<()> {
        let at = timestamp_text(self.now);
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO of_object (id, type, created_at, created_by_kind, recorded_from) VALUES (?1, 'Operator', ?2, 'human', ?2)",
            params![OPERATOR, at],
        )?;
        let position = next(&tx, "position")?;
        let txn = next(&tx, "txn")?;
        tx.execute(
            "INSERT INTO of_event (position, txn, object_id, object_seq, transition, from_state, to_state, changes_state, \
             source, actor_id, actor_kind, declaration_version, taint_version, recorded_at, occurred_at, payload, reads) \
             VALUES (?1, ?2, ?3, 1, 'create', NULL, 'ACTIVE', 1, 'observed', ?3, 'human', 1, 0, ?4, ?4, '{}', '{}')",
            params![position, txn, OPERATOR, at],
        )?;
        tx.execute(
            "INSERT INTO t_operator (id, state, version, declaration_version, last_event, identity) VALUES (?1, 'ACTIVE', 1, 1, ?2, ?1)",
            params![OPERATOR, position],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// A refused request's row of the attempt log, in a short transaction of
    /// its own after the request's rolled back (`DESIGN.md` §6,
    /// `storage-schema.md` §6).
    pub fn record_attempt(&mut self, a: &Attempt) -> Stored<()> {
        let tx = self.conn.transaction()?;
        next(&tx, "txn")?;
        insert_attempt(&tx, a, "(SELECT value FROM of_counter WHERE name = 'txn')")?;
        tx.commit()?;
        Ok(())
    }

    /// Every object, as a request reads them.
    pub fn snapshot(&self) -> Stored<World<'_>> {
        snapshot(&self.conn, &self.lib, self.now)
    }

    /// Decide on a snapshot and write what the decision changed, in one
    /// transaction: the request path's frame (`DESIGN.md` §6).
    /// What a decision would be on the store as it stands, never written,
    /// not even the numbers it drew: a check's simulation (`library-api.md`
    /// §6, ADR-0054).
    pub fn simulate<T>(
        &mut self,
        decide: impl FnOnce(&World<'_>, &mut Allocator) -> T,
    ) -> Result<T, StoreError> {
        let tx = self.conn.transaction()?;
        let world = snapshot(&tx, &self.lib, self.now)?;
        let mut alloc = Allocator::read(&tx)?;
        let out = decide(&world, &mut alloc);
        tx.rollback()?;
        Ok(out)
    }

    pub fn transact<T, E>(
        &mut self,
        decide: impl FnOnce(&World<'_>, &mut Allocator) -> Result<(T, Option<Change>), E>,
    ) -> Result<Result<T, E>, StoreError> {
        let tx = self.conn.transaction()?;
        let world = snapshot(&tx, &self.lib, self.now)?;
        let mut alloc = Allocator::read(&tx)?;
        let decided = decide(&world, &mut alloc);
        match decided {
            Ok((answer, Some(change))) => {
                alloc.write(&tx)?;
                alloc.write_sequences(&tx)?;
                write(&tx, &self.lib, &change)?;
                tx.commit()?;
                Ok(Ok(answer))
            }
            Ok((answer, None)) => {
                // a refused request writes nothing but the numbers it drew,
                // which are not reused (flow-format.md §4.12)
                tx.rollback()?;
                if !alloc.drawn.is_empty() {
                    let tx = self.conn.transaction()?;
                    alloc.write_sequences(&tx)?;
                    tx.commit()?;
                }
                Ok(Ok(answer))
            }
            Err(e) => {
                tx.rollback()?;
                Ok(Err(e))
            }
        }
    }

    /// A change's approval (`publish-and-import.md` §1 step 5, ADR-0155):
    /// refused as `stale` where the change moved since its approver read it,
    /// and as `current`, `fatal`, `impact_unchanged` or `decision_unresolved`
    /// where its dry run, recomputed at the attached report's `now`, says so;
    /// otherwise the version installed in one transaction, with the report it
    /// was decided on.
    pub fn publish(
        &mut self,
        actor: &str,
        change: &str,
        expected_version: i64,
    ) -> Stored<Result<(Verdict, Option<PublishReport>), NotRun>> {
        let world = self.snapshot()?;
        let refusal = |v: Verdict| Ok(Ok((v, None)));
        let unreachable = |clause: &str| Verdict::Unsatisfied {
            clause: clause.into(),
            remedy: Some(Remedy::UnreachableFromHere),
        };
        let Some(who) = resolve_actor(&world, actor) else {
            return refusal(Verdict::Unsatisfied {
                clause: "actor_known".into(),
                remedy: Some(Remedy::Dependent),
            });
        };
        let Some(c) = world.get(change).filter(|c| c.type_ == DECLARATION_CHANGE) else {
            return refusal(Verdict::NotFound);
        };
        if c.state != "SUBMITTED" {
            return refusal(Verdict::Unavailable {
                state: c.state.clone(),
            });
        }
        if c.version != expected_version {
            return refusal(Verdict::Stale {
                expected: expected_version,
                actual: c.version,
            });
        }
        if c.attr("base_version") != Value::Int(self.lib.version) {
            return refusal(unreachable("current"));
        }
        let text = |a: &str| match c.attr(a) {
            Value::Text(t) => t,
            _ => String::new(),
        };
        let attached: serde_json::Value = serde_json::from_str(&text("report")).unwrap_or_default();
        let mut at = world.clone();
        if let Some(now) = attached["now"].as_str().and_then(parse_timestamp) {
            at.now = now;
        }
        let source: Vec<(String, String)> =
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&text("source"))
                .unwrap_or_default()
                .into_iter()
                .map(|(n, t)| (n, t.as_str().unwrap_or_default().to_string()))
                .collect();
        let (source_ref, digest) = (text("source_ref"), text("source_digest"));
        let draft = crate::publish::Draft {
            change_id: Some(change),
            source: &source,
            source_ref: Some(&source_ref),
            source_digest: Some(&digest),
            actor,
        };
        let simulated = match crate::publish::simulate(&at, &draft) {
            Ok(s) => s,
            Err(e) => return Ok(Err(e)),
        };
        let report = simulated.report.clone();
        let refused_with = |clause: &str| Ok(Ok((unreachable(clause), Some(report.clone()))));
        if report.fatal() {
            return refused_with("fatal");
        }
        // nothing named in an impact the approver was not shown, the changes
        // it supersedes reported and not compared (ADR-0147)
        let shown = &attached["affected"];
        for (category, ids) in report.affected() {
            if category == "superseded" {
                continue;
            }
            let was: Vec<&str> = shown[&category]
                .as_array()
                .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
                .unwrap_or_default();
            if ids.iter().any(|id| !was.contains(&id.as_str())) {
                return refused_with("impact_unchanged");
            }
        }
        if !report.accepted {
            return refused_with("decision_unresolved");
        }
        let Some((mut new, objects)) = simulated.install else {
            return refused_with("fatal");
        };
        new.version = self.lib.version + 1;
        let change_obj = c.clone();
        let old_objects = world.objects.clone();
        drop(world);
        self.conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
        let installed = self.install(
            &new,
            &objects,
            &old_objects,
            &report,
            &change_obj,
            &who,
            &source,
        );
        self.conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let events = installed?;
        let version = change_obj.version + 1;
        self.lib = new;
        Ok(Ok((
            Verdict::Satisfied {
                object_id: change.to_string(),
                version,
                events,
                flags: Vec::new(),
            },
            Some(report),
        )))
    }

    /// The version installed, in one transaction, foreign keys checked
    /// before it commits (`publish-and-import.md` §1 step 5,
    /// `storage-schema.md` §10). Returns the positions of its events, the
    /// publish's first.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn install(
        &mut self,
        new: &Library,
        objects: &BTreeMap<String, Object>,
        old_objects: &BTreeMap<String, Object>,
        report: &PublishReport,
        change: &Object,
        who: &crate::eval::Who,
        source: &[(String, String)],
    ) -> Stored<Vec<i64>> {
        let old = self.lib.clone();
        let now = self.now;
        let tx = self.conn.transaction()?;
        let mut alloc = Allocator::read(&tx)?;
        let publish_position = alloc.position();
        alloc.txn();
        // before the mappings: new tables, and each table the version
        // changes relaxed, its renamed columns and set tables and its
        // set-asides first; a table it leaves as it was is not rebuilt
        let m = &new.migration;
        let mut changed = Vec::new();
        for (tn, t) in &new.types {
            if t.retired || crate::publish::builtin(tn) {
                continue;
            }
            let Some(ot) = old.type_(tn) else {
                tx.execute_batch(&table_parts(new, t, false).create(&table(tn), &[]))?;
                continue;
            };
            let name = table(tn);
            if table_parts(&old, ot, false).create(&name, &[])
                == table_parts(new, t, false).create(&name, &[])
            {
                continue;
            }
            changed.push(t);
            for (owner, from, to) in &m.renamed_attributes {
                if owner != tn {
                    continue;
                }
                match ot.attribute(from) {
                    Some(a) if a.stored_set() => tx.execute_batch(&format!(
                        "ALTER TABLE {} RENAME TO {};",
                        quote(&format!("{name}__{from}")),
                        quote(&format!("{name}__{to}"))
                    ))?,
                    Some(a) => {
                        let to_col = t
                            .attribute(to)
                            .map_or_else(|| to.clone(), |na| column(to, na));
                        tx.execute_batch(&format!(
                            "ALTER TABLE {name} RENAME COLUMN {} TO {};",
                            quote(&column(from, a)),
                            quote(&to_col)
                        ))?;
                        // its intervals carried under its new name (§3)
                        tx.execute(
                            &format!(
                                "UPDATE of_interval SET dimension = ?2 WHERE dimension = ?1 \
                                 AND object_id IN (SELECT id FROM {name})"
                            ),
                            params![from, to],
                        )?;
                    }
                    None => {}
                }
            }
            for (n, a) in &ot.attributes {
                let Some(na) = t.attribute(n) else { continue };
                if na.declared == a.declared && na.many == a.many {
                    continue;
                }
                // its intervals set aside under the name its column takes,
                // the open one closed at the publish event
                tx.execute(
                    &format!(
                        "UPDATE of_interval SET dimension = ?2, \
                         left_position = coalesce(left_position, ?3), left_at = coalesce(left_at, ?4) \
                         WHERE dimension = ?1 AND object_id IN (SELECT id FROM {name})"
                    ),
                    params![n, format!("{n}__v{}", old.version), publish_position, timestamp_text(now)],
                )?;
                if a.stored_set() {
                    tx.execute_batch(&format!(
                        "ALTER TABLE {} RENAME TO {};",
                        quote(&format!("{name}__{n}")),
                        quote(&format!("{name}__{n}__v{}", old.version))
                    ))?;
                }
                if !a.many && !a.derived_end() {
                    tx.execute_batch(&format!(
                        "ALTER TABLE {name} RENAME COLUMN {} TO {};",
                        quote(&column(n, a)),
                        quote(&format!("{}__v{}", column(n, a), old.version))
                    ))?;
                }
            }
            rebuild(&tx, new, t, true)?;
        }
        // the publish event, then one migration per object whose row the
        // mappings change or an admission reaches, in ascending id order
        let mut change_obj = change.clone();
        let before = Some(vec![(
            "state".to_string(),
            Value::State {
                type_: DECLARATION_CHANGE.into(),
                name: change.state.clone(),
            },
        )]);
        change_obj.state = "PUBLISHED".into();
        change_obj
            .attrs
            .insert("approved_by".into(), Value::Text(who.id.clone()));
        change_obj
            .attrs
            .insert("approved_by_kind".into(), Value::Text(who.kind.clone()));
        let publish_event = event_of(
            &mut change_obj,
            before.as_ref(),
            Some(change.state.clone()),
            "publish",
            publish_position,
            now,
            who,
            "observed",
            new.version,
            new.version,
            serde_json::json!({ "version": new.version }),
        );
        let mut change_set = Change::default();
        let mut positions = vec![publish_position];
        change_set.writes.push((change_obj, false, publish_event));
        let affected = report.affected();
        let codes_of = |id: &str| -> Vec<String> {
            affected
                .iter()
                .filter(|(code, ids)| code.starts_with("mapping:") && ids.iter().any(|i| i == id))
                .map(|(code, _)| code.clone())
                .collect()
        };
        let admitted_of = |id: &str| -> Vec<(String, String)> {
            affected
                .iter()
                .filter(|(code, ids)| code.starts_with("invariant:") && ids.iter().any(|i| i == id))
                .filter_map(|(code, _)| {
                    let inv = code.trim_start_matches("invariant:");
                    let (tn, name) = inv.rsplit_once('.')?;
                    m.admit
                        .iter()
                        .find(|(a, _)| {
                            a == inv
                                || a.rsplit_once('.')
                                    .is_some_and(|(at, an)| an == name && new.is_a(tn, at))
                        })
                        .map(|(_, reason)| (name.to_string(), reason.clone()))
                })
                .collect()
        };
        let mut in_order: Vec<&Object> = objects.values().collect();
        in_order.sort_by_key(|o| o.order);
        for o in in_order {
            let Some(t) = new.type_(&o.type_) else {
                continue;
            };
            if t.retired || crate::publish::builtin(&o.type_) {
                continue;
            }
            let Some(was) = old_objects.get(&o.id) else {
                continue;
            };
            let codes = codes_of(&o.id);
            let admits = admitted_of(&o.id);
            let mut x = o.clone();
            // a member newly tracked opens at the migration where a backfill
            // writes it, and else at the publish event; one no longer tracked
            // closes at the publish event (ADR-0146, ADR-0147). A rename has
            // carried its spans under the new name already.
            let old_t = old.type_(&o.type_);
            let tracked_now = new.tracked(t);
            let backfilled = |d: &str| {
                codes
                    .iter()
                    .any(|c| c.starts_with("mapping:backfill:") && c.ends_with(&format!(".{d}")))
            };
            let migrates = !codes.is_empty() || !admits.is_empty();
            let position = if migrates {
                alloc.position()
            } else {
                publish_position
            };
            let mut extra = Vec::new();
            for (d, spans) in &mut x.spans {
                if !tracked_now.contains(d)
                    && let Some(last) = spans.last_mut()
                    && last.left_position.is_none()
                {
                    last.left_position = Some(publish_position);
                    last.left_at = Some(now);
                    extra.push((d.clone(), last.clone()));
                }
            }
            // what each tracked member held before the migration: its open
            // span's value
            let before: Vec<(String, Value)> = tracked_now
                .iter()
                .filter_map(|d| {
                    x.spans
                        .get(d)
                        .and_then(|s| s.last())
                        .filter(|sp| sp.left_position.is_none())
                        .map(|sp| (d.clone(), sp.value.clone()))
                })
                .collect();
            for d in &tracked_now {
                if x.spans.get(d).is_none_or(Vec::is_empty) {
                    let at = if migrates && backfilled(d) {
                        position
                    } else {
                        publish_position
                    };
                    let sp = opened(&x, d, at, now, who, new.version);
                    x.spans.entry(d.clone()).or_default().push(sp.clone());
                    if !migrates || at == publish_position {
                        extra.push((d.clone(), sp));
                    }
                }
            }
            for (d, sp) in extra {
                change_set.spans.push((x.id.clone(), d, sp));
            }
            if migrates {
                positions.push(position);
                for (n, reason) in &admits {
                    x.admissions.insert(n.clone());
                    change_set
                        .admitted
                        .push((x.id.clone(), n.clone(), position, reason.clone()));
                }
                let event = event_of(
                    &mut x,
                    Some(&before),
                    Some(was.state.clone()),
                    "migrate",
                    position,
                    now,
                    who,
                    "migrated",
                    old.version,
                    new.version,
                    serde_json::json!({ "mappings": codes, "admits": admits.iter().map(|(n, _)| n).collect::<Vec<_>>() }),
                );
                change_set.writes.push((x.clone(), false, event));
            }
            // an attribute the version newly marks personal, redacted on an
            // object already erased, by a redact event (publish-and-import.md
            // §1 step 5), which admits what the absence breaks, as the
            // erasure it completes does (declaration-syntax.md §6.4)
            let newly: Vec<String> = t
                .attributes
                .iter()
                .filter(|(n, a)| {
                    x.erased
                        && a.personal
                        && old_t
                            .and_then(|ot| ot.attribute(n))
                            .is_some_and(|oa| !oa.personal)
                })
                .map(|(n, _)| n.clone())
                .collect();
            if !newly.is_empty() {
                let position = alloc.position();
                positions.push(position);
                for n in &newly {
                    x.attrs.remove(n);
                    for sp in x.spans.get_mut(n).into_iter().flatten() {
                        sp.value = Value::Absent;
                    }
                }
                let mut after = World::new(new, now);
                after.objects = objects.clone();
                after.objects.insert(x.id.clone(), x.clone());
                for n in failing_invariants(&after, &x).map_err(|e| StoreError(e.0))? {
                    if !x.admissions.contains(&n)
                        && invariant_reads(t, &n).iter().any(|r| newly.contains(r))
                    {
                        x.admissions.insert(n.clone());
                        change_set
                            .admitted
                            .push((x.id.clone(), n, position, String::new()));
                    }
                }
                let state = x.state.clone();
                let event = event_of(
                    &mut x,
                    None,
                    Some(state),
                    "redact",
                    position,
                    now,
                    who,
                    "erased",
                    new.version,
                    new.version,
                    serde_json::json!({ "redacted": newly }),
                );
                change_set.redacted.push((x.id.clone(), newly));
                change_set.writes.push((x, false, event));
            }
        }
        // the other open changes, which the publish supersedes, and the
        // pending proposals it invalidates
        for (code, state, transition) in [
            ("superseded", "SUPERSEDED", "supersede"),
            ("proposals_invalidated", "INVALIDATED", "invalidate"),
        ] {
            for id in affected.get(code).into_iter().flatten() {
                let Some(o) = old_objects.get(id) else {
                    continue;
                };
                let mut x = o.clone();
                let before = Some(vec![(
                    "state".to_string(),
                    Value::State {
                        type_: o.type_.clone(),
                        name: o.state.clone(),
                    },
                )]);
                x.state = state.into();
                let position = alloc.position();
                positions.push(position);
                let event = event_of(
                    &mut x,
                    before.as_ref(),
                    Some(o.state.clone()),
                    transition,
                    position,
                    now,
                    who,
                    "observed",
                    new.version,
                    new.version,
                    serde_json::json!({ "by": change.id }),
                );
                change_set.writes.push((x, false, event));
            }
        }
        alloc.write(&tx)?;
        write(&tx, new, &change_set)?;
        // after the mappings: every changed table brought to the version's
        // exact shape, then every foreign key checked
        for t in changed {
            rebuild(&tx, new, t, false)?;
        }
        let dangling: Vec<String> = {
            let mut check = tx.prepare("PRAGMA foreign_key_check")?;
            let rows = check.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        if let Some(table) = dangling.first() {
            return Err(StoreError(format!(
                "the publish left a reference in {table} that names nothing"
            )));
        }
        let files: serde_json::Map<String, serde_json::Value> = source
            .iter()
            .map(|(n, t)| (n.clone(), serde_json::Value::String(t.clone())))
            .collect();
        tx.execute(
            "INSERT INTO of_declaration (version, module, source, source_ref, source_digest, builtins, \
             published_at, published_by, change_id, report) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                new.version,
                new.modules.last().cloned().unwrap_or_default(),
                serde_json::Value::Object(files).to_string(),
                report.source_ref,
                report.source_digest,
                BUILTINS,
                timestamp_text(now),
                who.id,
                change.id,
                crate::publish::report_json(report).to_string(),
            ],
        )?;
        tx.commit()?;
        positions.sort_unstable();
        Ok(positions)
    }

    /// The store's connection, for a test to read its schema.
    #[cfg(test)]
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// An object's events, in its order.
    pub fn history(&self, id: &str) -> Stored<Vec<Event>> {
        let mut stmt = self.conn.prepare(&format!(
            "{EVENT_SELECT} WHERE object_id = ?1 ORDER BY object_seq"
        ))?;
        let rows = stmt.query_map(params![id], event_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// The event at each position, in the order given (ADR-0151).
    pub fn events(&self, positions: &[i64]) -> Stored<Vec<Option<Event>>> {
        let mut stmt = self
            .conn
            .prepare(&format!("{EVENT_SELECT} WHERE position = ?1"))?;
        let mut out = Vec::new();
        for p in positions {
            let mut rows = stmt.query_map(params![p], event_row)?;
            out.push(rows.next().transpose()?);
        }
        Ok(out)
    }
}

/// The counters a request allocates from, read at its start and written at
/// its end, so a refused request allocates nothing (§2).
#[derive(Debug)]
pub struct Allocator {
    position: i64,
    txn: i64,
    object: i64,
    txn_taken: bool,
    /// Each sequence's last number, per scope, and the ones this request drew,
    /// which stay drawn though it is refused (`flow-format.md` §4.12).
    sequences: BTreeMap<(String, String), i64>,
    drawn: std::collections::BTreeSet<(String, String)>,
}

impl Allocator {
    fn read(tx: &Transaction<'_>) -> Stored<Allocator> {
        let get = |n: &str| -> Stored<i64> {
            Ok(tx.query_row(
                "SELECT value FROM of_counter WHERE name = ?1",
                params![n],
                |r| r.get(0),
            )?)
        };
        let mut sequences = BTreeMap::new();
        let mut stmt = tx.prepare("SELECT sequence, scope, value FROM of_sequence")?;
        let rows = stmt.query_map([], |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?)))?;
        for row in rows {
            let (key, value): ((String, String), i64) = row?;
            sequences.insert(key, value);
        }
        Ok(Allocator {
            position: get("position")?,
            txn: get("txn")?,
            object: get("object")?,
            txn_taken: false,
            sequences,
            drawn: std::collections::BTreeSet::new(),
        })
    }

    /// The next number of a sequence in a scope, numbered from 1.
    pub fn draw(&mut self, sequence: &str, scope: &str) -> i64 {
        let key = (sequence.to_string(), scope.to_string());
        let n = self.sequences.get(&key).copied().unwrap_or(0) + 1;
        self.sequences.insert(key.clone(), n);
        self.drawn.insert(key);
        n
    }

    /// The numbers drawn, written whatever became of the request.
    fn write_sequences(&self, tx: &Transaction<'_>) -> Stored<()> {
        for key in &self.drawn {
            let value = self.sequences.get(key).copied().unwrap_or(0);
            tx.execute(
                "INSERT INTO of_sequence (sequence, scope, value) VALUES (?1, ?2, ?3) \
                 ON CONFLICT (sequence, scope) DO UPDATE SET value = excluded.value",
                params![key.0, key.1, value],
            )?;
        }
        Ok(())
    }

    /// The next event's position, allocated as its transition begins.
    pub fn position(&mut self) -> i64 {
        self.position += 1;
        self.position
    }

    /// The writing transaction's number, the same for every event it writes.
    pub fn txn(&mut self) -> i64 {
        if !self.txn_taken {
            self.txn += 1;
            self.txn_taken = true;
        }
        self.txn
    }

    /// A new object's id: its type and a number, readable in a sandbox.
    pub fn object_id(&mut self, type_: &str) -> String {
        self.object += 1;
        format!("{}-{}", type_.to_lowercase(), self.object)
    }

    fn write(&self, tx: &Transaction<'_>) -> Stored<()> {
        for (n, v) in [
            ("position", self.position),
            ("txn", self.txn),
            ("object", self.object),
        ] {
            tx.execute(
                "UPDATE of_counter SET value = ?2 WHERE name = ?1",
                params![n, v],
            )?;
        }
        Ok(())
    }
}

fn next(tx: &Connection, name: &str) -> Stored<i64> {
    tx.execute(
        "UPDATE of_counter SET value = value + 1 WHERE name = ?1",
        params![name],
    )?;
    Ok(tx.query_row(
        "SELECT value FROM of_counter WHERE name = ?1",
        params![name],
        |r| r.get(0),
    )?)
}

const EVENT_SELECT: &str = "SELECT position, object_id, object_seq, transition, from_state, to_state, changes_state, \
    source, actor_id, actor_kind, declaration_version, recorded_at, occurred_at, imported, payload, actor_principal FROM of_event";

fn event_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    let recorded: String = r.get(11)?;
    let occurred: Option<String> = r.get(12)?;
    let payload: String = r.get(14)?;
    Ok(Event {
        position: r.get(0)?,
        object_id: r.get(1)?,
        object_seq: r.get(2)?,
        transition: r.get(3)?,
        from_state: r.get(4)?,
        to_state: r.get(5)?,
        changes_state: r.get::<_, i64>(6)? != 0,
        source: r.get(7)?,
        actor_id: r.get(8)?,
        actor_kind: r.get(9)?,
        principal: r.get(15)?,
        declaration_version: r.get(10)?,
        recorded_at: parse_timestamp(&recorded).unwrap_or(0),
        occurred_at: occurred.as_deref().and_then(parse_timestamp),
        imported: r.get::<_, i64>(13)? != 0,
        payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
    })
}

fn snapshot<'l>(conn: &Connection, lib: &'l Library, now: i64) -> Stored<World<'l>> {
    let mut world = World::new(lib, now);
    let mut directory =
        conn.prepare("SELECT rowid, id, type, created_at, created_by_kind FROM of_object")?;
    let rows = directory.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mut by_type: BTreeMap<String, Vec<(i64, String, i64, String)>> = BTreeMap::new();
    for row in rows {
        let (order, id, type_, created_at, kind) = row?;
        let at = parse_timestamp(&created_at)
            .ok_or_else(|| StoreError(format!("{id}'s creation time")))?;
        by_type
            .entry(type_)
            .or_default()
            .push((order, id, at, kind));
    }
    for (type_, objects) in by_type {
        let Some(t) = lib.type_(&type_) else {
            return Err(StoreError(format!(
                "the directory names a type {type_} the library lacks"
            )));
        };
        let name = table(&type_);
        let scalars: Vec<&(String, Attribute)> =
            t.attributes.iter().filter(|(_, a)| !a.many).collect();
        let cols: Vec<String> = scalars.iter().map(|(n, a)| quote(&column(n, a))).collect();
        let select = format!(
            "SELECT state, version{}{} FROM {name} WHERE id = ?1",
            if cols.is_empty() { "" } else { ", " },
            cols.join(", ")
        );
        let mut stmt = conn.prepare(&select)?;
        for (order, id, created_at, kind) in objects {
            let (state, version, values) = stmt.query_row(params![id], |r| {
                let mut values = Vec::new();
                for i in 0..scalars.len() {
                    values.push(r.get::<_, Sql>(i + 2)?);
                }
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, values))
            })?;
            let mut attrs = BTreeMap::new();
            for ((n, a), v) in scalars.iter().zip(values) {
                let v = from_sql(v, &a.declared, lib)?;
                if !v.is_absent() {
                    attrs.insert(n.clone(), v);
                }
            }
            for (n, a) in t.attributes.iter().filter(|(_, a)| a.stored_set()) {
                let mut set = conn.prepare(&format!(
                    "SELECT value FROM {} WHERE object_id = ?1 ORDER BY ordinal",
                    quote(&format!("{name}__{n}"))
                ))?;
                let items = set.query_map(params![id], |r| r.get::<_, Sql>(0))?;
                let mut values = Vec::new();
                for item in items {
                    values.push(from_sql(item?, &a.declared, lib)?);
                }
                attrs.insert(n.clone(), Value::Set(values));
            }
            world.objects.insert(
                id.clone(),
                Object {
                    superseded_by: if t.states.iter().any(|(_, s)| s.superseding) {
                        conn.query_row(
                            &format!("SELECT superseded_by FROM {name} WHERE id = ?1"),
                            params![id],
                            |r| r.get::<_, Option<String>>(0),
                        )?
                    } else {
                        None
                    },
                    id,
                    type_: type_.clone(),
                    state,
                    version,
                    attrs,
                    created_at,
                    created_by_kind: kind,
                    order,
                    spans: BTreeMap::new(),
                    writes: BTreeMap::new(),
                    admissions: std::collections::BTreeSet::new(),
                    erased: false,
                    created_dated: true,
                    recorded_from: created_at,
                },
            );
        }
    }
    spans(conn, lib, &mut world)?;
    // whether each creation is dated, and its record's start
    let mut dated = conn.prepare("SELECT id, created_dated, recorded_from FROM of_object")?;
    let rows = dated.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (id, is_dated, from) = row?;
        if let Some(o) = world.objects.get_mut(&id) {
            o.created_dated = is_dated != 0;
            o.recorded_from = parse_timestamp(&from)
                .ok_or_else(|| StoreError(format!("{id}'s recorded_from")))?;
        }
    }
    let mut ids = conn.prepare("SELECT source, value, object_id FROM of_external_id")?;
    let rows = ids.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (source, value, id) = row?;
        world.external.insert((source, value), id);
    }
    let mut index =
        conn.prepare("SELECT object_id, attribute, position FROM of_attribute_write")?;
    let rows = index.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (id, attribute, position) = row?;
        if let Some(o) = world.objects.get_mut(&id) {
            o.writes.insert(attribute, position);
        }
    }
    // an object whose own erasure is recorded
    let mut erasures =
        conn.prepare("SELECT DISTINCT object_id FROM of_event WHERE source = 'erased'")?;
    let rows = erasures.query_map([], |r| r.get::<_, String>(0))?;
    for row in rows {
        if let Some(o) = world.objects.get_mut(&row?) {
            o.erased = true;
        }
    }
    // the attempt log, in the order written
    let mut attempts = conn.prepare(
        "SELECT at, actor_id, actor_kind, type, object_id, transition, verdict, clause, remedy, unknown, enforced, \
         flagged, applied_position, request_values, declaration_version, actor_principal FROM of_attempt ORDER BY id",
    )?;
    let rows = attempts.query_map([], attempt_row)?;
    world.attempts = rows.collect::<Result<Vec<_>, _>>()?;
    // the log, and the events that recorded an admission
    let mut log = conn.prepare(&format!("{EVENT_SELECT} ORDER BY position"))?;
    let rows = log.query_map([], event_row)?;
    world.events = rows.collect::<Result<Vec<_>, _>>()?;
    world.imported = world
        .events
        .iter()
        .filter(|e| e.imported)
        .map(|e| e.position)
        .collect();
    let mut admitting = conn.prepare("SELECT DISTINCT position FROM of_admission")?;
    let rows = admitting.query_map([], |r| r.get::<_, i64>(0))?;
    for row in rows {
        world.admitting.insert(row?);
    }
    // the admissions that stand
    let mut standing = conn.prepare(
        "SELECT object_id, invariant FROM of_admission WHERE discharged_position IS NULL",
    )?;
    let rows = standing.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (id, invariant) = row?;
        if let Some(o) = world.objects.get_mut(&id) {
            o.admissions.insert(invariant);
        }
    }
    Ok(world)
}

/// Every object's flow data, from the interval index, in each dimension's
/// order.
fn spans(conn: &Connection, lib: &Library, world: &mut World<'_>) -> Stored<()> {
    let mut stmt = conn.prepare(
        "SELECT object_id, dimension, value, entered_position, left_position, entered_at, left_at, \
         entered_by, entered_by_kind, migrated, declaration_version, legacy, corrected_by FROM of_interval \
         ORDER BY object_id, dimension, entered_position, entered_at",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Sql>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, Option<i64>>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, Option<String>>(7)?,
            r.get::<_, String>(8)?,
            r.get::<_, i64>(9)? != 0,
            r.get::<_, i64>(10)?,
            r.get::<_, i64>(11)? != 0,
            r.get::<_, Option<i64>>(12)?,
        ))
    })?;
    for row in rows {
        let (
            id,
            dimension,
            value,
            entered_position,
            left_position,
            entered_at,
            left_at,
            by,
            kind,
            migrated,
            declaration_version,
            legacy,
            corrected_by,
        ) = row?;
        let Some(o) = world.objects.get_mut(&id) else {
            return Err(StoreError(format!(
                "an interval of {id}, which the directory lacks"
            )));
        };
        let value = if dimension == "state" {
            match value {
                Sql::Text(name) => Value::State {
                    type_: o.type_.clone(),
                    name,
                },
                other => return Err(StoreError(format!("{id}'s state interval holds {other:?}"))),
            }
        } else {
            // a dropped attribute's intervals, and a set-aside's, stay as
            // history, and nothing the version declares reads them (§10)
            let Some(a) = lib.type_(&o.type_).and_then(|t| t.attribute(&dimension)) else {
                continue;
            };
            from_sql(value, &a.declared, lib)?
        };
        let at = |text: &str| {
            parse_timestamp(text).ok_or_else(|| StoreError(format!("an interval time {text}")))
        };
        let span = Span {
            value,
            entered_position,
            left_position,
            entered_at: at(&entered_at)?,
            left_at: left_at.as_deref().map(at).transpose()?,
            entered_by: by,
            entered_by_kind: kind,
            migrated,
            declaration_version,
            legacy,
            corrected_by,
        };
        o.spans.entry(dimension).or_default().push(span);
    }
    Ok(())
}

fn write(tx: &Transaction<'_>, lib: &Library, change: &Change) -> Stored<()> {
    for (o, created, event) in &change.writes {
        let Some(t) = lib.type_(&o.type_) else {
            return Err(StoreError(format!(
                "a write to a type {} the library lacks",
                o.type_
            )));
        };
        if *created {
            let at = timestamp_text(o.created_at);
            tx.execute(
                "INSERT INTO of_object (id, type, created_at, created_by_kind, created_dated, recorded_from) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    o.id,
                    o.type_,
                    at,
                    o.created_by_kind,
                    i64::from(o.created_dated),
                    timestamp_text(o.recorded_from)
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO of_event (position, txn, object_id, object_seq, transition, from_state, to_state, changes_state, \
             source, actor_id, actor_kind, declaration_version, taint_version, recorded_at, occurred_at, payload, reads, imported, \
             actor_principal) \
             VALUES (?1, (SELECT value FROM of_counter WHERE name = 'txn'), ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, ?12, ?13, ?14, '{}', ?15, ?16)",
            params![
                event.position,
                event.object_id,
                event.object_seq,
                event.transition,
                event.from_state,
                event.to_state,
                i64::from(event.changes_state),
                event.source,
                event.actor_id,
                event.actor_kind,
                event.declaration_version,
                timestamp_text(event.recorded_at),
                event.occurred_at.map(timestamp_text),
                event.payload.to_string(),
                i64::from(event.imported),
                event.principal,
            ],
        )?;
        let name = table(&o.type_);
        let scalars: Vec<&(String, Attribute)> =
            t.attributes.iter().filter(|(_, a)| !a.many).collect();
        let mut cols = vec![
            "id".to_string(),
            "state".to_string(),
            "version".to_string(),
            "declaration_version".to_string(),
            "last_event".to_string(),
        ];
        let mut vals: Vec<Sql> = vec![
            Sql::Text(o.id.clone()),
            Sql::Text(o.state.clone()),
            Sql::Integer(o.version),
            Sql::Integer(event.declaration_version),
            Sql::Integer(event.position),
        ];
        if t.states.iter().any(|(_, s)| s.superseding) {
            cols.push("superseded_by".to_string());
            vals.push(o.superseded_by.clone().map_or(Sql::Null, Sql::Text));
        }
        // the source of the event that last changed the state (§3)
        // a migration leaves it as it was (§3)
        if (*created || event.changes_state) && event.source != "migrated" {
            cols.push("state_source".to_string());
            vals.push(Sql::Text(
                match (event.source.as_str(), event.imported) {
                    (_, true) => "imported",
                    ("asserted", false) => "asserted",
                    _ => "observed",
                }
                .to_string(),
            ));
        }
        for (n, a) in &scalars {
            cols.push(quote(&column(n, a)));
            vals.push(to_sql(&o.attr(n))?);
        }
        if *created {
            let marks: Vec<String> = (1..=vals.len()).map(|i| format!("?{i}")).collect();
            tx.execute(
                &format!(
                    "INSERT INTO {name} ({}) VALUES ({})",
                    cols.join(", "),
                    marks.join(", ")
                ),
                rusqlite::params_from_iter(vals.iter()),
            )?;
        } else {
            // an existing row is updated in place, since its set tables'
            // rows reference it
            let sets: Vec<String> = cols
                .iter()
                .enumerate()
                .skip(1)
                .map(|(i, c)| format!("{c} = ?{}", i + 1))
                .collect();
            tx.execute(
                &format!("UPDATE {name} SET {} WHERE id = ?1", sets.join(", ")),
                rusqlite::params_from_iter(vals.iter()),
            )?;
        }
        // the spans this event opened or ended, written with it, in the same
        // transaction (storage-schema.md, of_interval)
        for (dimension, spans) in &o.spans {
            // and those a correction relabelled or reached (§6.4)
            for sp in spans.iter().filter(|sp| {
                sp.entered_position == event.position
                    || sp.left_position == Some(event.position)
                    || sp.corrected_by == Some(event.position)
            }) {
                upsert_span(tx, &o.id, dimension, sp)?;
            }
        }
        for (n, _) in t.attributes.iter().filter(|(_, a)| a.stored_set()) {
            let side = quote(&format!("{name}__{n}"));
            tx.execute(
                &format!("DELETE FROM {side} WHERE object_id = ?1"),
                params![o.id],
            )?;
            if let Value::Set(items) = o.attr(n) {
                for (i, item) in items.iter().enumerate() {
                    let ordinal = i64::try_from(i)
                        .map_err(|_| StoreError("a set past 64 bits".to_string()))?;
                    tx.execute(
                        &format!(
                            "INSERT INTO {side} (object_id, ordinal, value) VALUES (?1, ?2, ?3)"
                        ),
                        params![o.id, ordinal, to_sql(item)?],
                    )?;
                }
            }
        }
    }
    for (id, dimension, position, at) in &change.merged {
        tx.execute(
            "DELETE FROM of_interval WHERE object_id = ?1 AND dimension = ?2 \
             AND entered_position = ?3 AND entered_at = ?4",
            params![id, dimension, position, timestamp_text(*at)],
        )?;
    }
    // the write index, once every event it names is in the log
    for (id, attribute, position) in &change.index {
        tx.execute(
            "INSERT INTO of_attribute_write (object_id, attribute, position) VALUES (?1, ?2, ?3) \
             ON CONFLICT (object_id, attribute) DO UPDATE SET position = excluded.position",
            params![id, attribute, position],
        )?;
    }
    for (id, invariant, position, reason) in &change.admitted {
        tx.execute(
            "INSERT INTO of_admission (object_id, invariant, position, reason) VALUES (?1, ?2, ?3, ?4)",
            params![id, invariant, position, reason],
        )?;
    }
    for (id, dimension, sp) in &change.spans {
        upsert_span(tx, id, dimension, sp)?;
    }
    // a raised flag and an observing clause's would-be refusal, written with
    // the event each is linked to (storage-schema.md §6)
    for a in &change.attempts {
        insert_attempt(tx, a, "(SELECT value FROM of_counter WHERE name = 'txn')")?;
    }
    // personal values leave the log and the interval index, the events and
    // their shape kept (§9, steps 2 and 9)
    for (id, attributes) in &change.redacted {
        for a in attributes {
            // and a correction's record of the value it replaced and wrote
            let corrected = format!("$.corrected.\"{}\"", a.replace('"', ""));
            tx.execute(
                "UPDATE of_event SET payload = json_set(payload, ?2 || '.replaced', json('null'), \
                 ?2 || '.value', json('null')) WHERE object_id = ?1 AND json_type(payload, ?2) IS NOT NULL",
                params![id, corrected],
            )?;
            let path = format!("$.inputs.\"{}\"", a.replace('"', ""));
            tx.execute(
                "UPDATE of_event SET payload = json_set(payload, ?2, json('null')) \
                 WHERE object_id = ?1 AND json_type(payload, ?2) IS NOT NULL",
                params![id, path],
            )?;
            tx.execute(
                "UPDATE of_interval SET value = NULL WHERE object_id = ?1 AND dimension = ?2",
                params![id, a],
            )?;
            // an erased personal external identifier no longer resolves (§9 step 10)
            tx.execute(
                "DELETE FROM of_external_id WHERE object_id = ?1 AND attribute = ?2",
                params![id, a],
            )?;
        }
        // a row no event of the change writes, a label whose note its
        // subject's erasure redacted as no transition, loses the value too
        // (§9 step 14, declaration-syntax.md §6.8)
        if !change.writes.iter().any(|(o, _, _)| &o.id == id) {
            let type_: Option<String> = tx
                .query_row("SELECT type FROM of_object WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })?;
            if let Some(t) = type_.as_deref().and_then(|n| lib.type_(n)) {
                for a in attributes {
                    if let Some(spec) = t.attribute(a).filter(|s| !s.many && s.personal) {
                        tx.execute(
                            &format!(
                                "UPDATE {} SET {} = NULL WHERE id = ?1",
                                table(&t.name),
                                quote(&column(a, spec))
                            ),
                            [id],
                        )?;
                    }
                }
            }
        }
    }
    for (source, value, id, attribute) in &change.external {
        tx.execute(
            "INSERT INTO of_external_id (source, value, object_id, attribute) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (source, value) DO UPDATE SET object_id = excluded.object_id, attribute = excluded.attribute",
            params![source, value, id, attribute],
        )?;
    }
    for (sequence, scope, value) in &change.raised {
        tx.execute(
            "INSERT INTO of_sequence (sequence, scope, value) VALUES (?1, ?2, ?3) \
             ON CONFLICT (sequence, scope) DO UPDATE SET value = max(value, excluded.value)",
            params![sequence, scope, value],
        )?;
    }
    for (id, invariant, position) in &change.discharged {
        tx.execute(
            "UPDATE of_admission SET discharged_position = ?3 \
             WHERE object_id = ?1 AND invariant = ?2 AND discharged_position IS NULL",
            params![id, invariant, position],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sandbox_holds_its_operator() {
        let lib = Library::load(&[]).unwrap();
        let store = Store::sandbox(lib, 1_767_225_600).unwrap();
        let world = store.snapshot().unwrap();
        let op = world.get(OPERATOR).unwrap();
        assert_eq!(op.type_, "Operator");
        assert_eq!(op.state, "ACTIVE");
        assert_eq!(op.attr("identity"), Value::Text(OPERATOR.into()));
        let h = store.history(OPERATOR).unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].to_state, "ACTIVE");
    }
}

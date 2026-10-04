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

use crate::model::{Attribute, Declared, Library, OPERATOR, Type};
use crate::value::{
    Decimal, Value, date_text, minor_unit, parse_date, parse_timestamp, timestamp_text,
};
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
    pub declaration_version: i64,
    pub recorded_at: i64,
    pub occurred_at: Option<i64>,
    pub imported: bool,
    pub payload: serde_json::Value,
}

/// What a request writes: the objects it created or changed, each with its
/// event, in the order the events were allocated.
#[derive(Clone, Debug, PartialEq)]
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
}

fn table(type_: &str) -> String {
    format!("t_{}", type_.to_lowercase())
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

impl Store {
    /// An in-memory store for a sandbox, holding the library's types and its
    /// operator, created at `now`.
    pub fn sandbox(lib: Library, now: i64) -> Stored<Store> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        conn.execute_batch(SCHEMA)?;
        let store = Store {
            conn,
            lib,
            now,
            answers: BTreeMap::new(),
        };
        for t in store.lib.types.values() {
            store.conn.execute_batch(&store.type_ddl(t))?;
        }
        store.create_operator()?;
        Ok(store)
    }

    /// The DDL of a type's table and its set tables (§3, §3.3).
    fn type_ddl(&self, t: &Type) -> String {
        let name = table(&t.name);
        let states = t
            .states
            .iter()
            .map(|(s, _)| literal(s))
            .collect::<Vec<_>>()
            .join(",");
        let mut cols = vec![
            "id TEXT PRIMARY KEY REFERENCES of_object(id)".to_string(),
            "state TEXT NOT NULL".to_string(),
            "version INTEGER NOT NULL".to_string(),
            "declaration_version INTEGER NOT NULL".to_string(),
            "last_event INTEGER NOT NULL REFERENCES of_event(position)".to_string(),
            "state_source TEXT NOT NULL DEFAULT 'observed'".to_string(),
        ];
        // present only on a type with a superseding state (§3)
        if t.states.iter().any(|(_, s)| s.superseding) {
            cols.push(
                "superseded_by TEXT REFERENCES of_object(id) DEFERRABLE INITIALLY DEFERRED"
                    .to_string(),
            );
        }
        let mut constraints = vec![
            format!(
                "CONSTRAINT {} CHECK (state IN ({states}))",
                quote(&format!("{name}_state"))
            ),
            format!(
                "CONSTRAINT {} CHECK (state_source IN ('observed','asserted','imported'))",
                quote(&format!("{name}_state_source"))
            ),
        ];
        let mut after = Vec::new();
        for (n, a) in &t.attributes {
            if a.derived_end() {
                continue;
            }
            if a.many {
                let target = match &a.declared {
                    Declared::Reference(target) if self.lib.types.contains_key(target) => {
                        format!(
                            " REFERENCES {}(id) DEFERRABLE INITIALLY DEFERRED",
                            table(target)
                        )
                    }
                    _ => String::new(),
                };
                after.push(format!(
                    "CREATE TABLE {} (object_id TEXT NOT NULL REFERENCES {name}(id), ordinal INTEGER NOT NULL, \
                     value {} NOT NULL{target}, PRIMARY KEY (object_id, ordinal));",
                    quote(&format!("{name}__{n}")),
                    sql_type(&a.declared)
                ));
                continue;
            }
            let col = column(n, a);
            let required = !a.optional && a.default.is_none() && base(&a.declared) != "counter";
            let mut def = format!("{} {}", quote(&col), sql_type(&a.declared));
            if base(&a.declared) == "counter" {
                def.push_str(" NOT NULL DEFAULT 0");
            } else if required {
                def.push_str(" NOT NULL");
            }
            match &a.declared {
                Declared::Reference(target) if self.lib.types.contains_key(target) => {
                    def.push_str(&format!(
                        " REFERENCES {}(id) DEFERRABLE INITIALLY DEFERRED",
                        table(target)
                    ));
                    after.push(format!(
                        "CREATE INDEX {} ON {name} ({});",
                        quote(&format!("{name}_{col}_ix")),
                        quote(&col)
                    ));
                }
                Declared::Scalar(e) => {
                    if let Some(members) = self.lib.enumerations.get(e) {
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
            cols.push(def);
        }
        cols.extend(constraints);
        let mut ddl = format!("CREATE TABLE {name} (\n  {}\n);\n", cols.join(",\n  "));
        ddl.push_str(&format!(
            "CREATE INDEX {} ON {name} (state);\n",
            quote(&format!("{name}_state_ix"))
        ));
        for a in after {
            ddl.push_str(&a);
            ddl.push('\n');
        }
        ddl
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

    /// Every object, as a request reads them.
    pub fn snapshot(&self) -> Stored<World<'_>> {
        snapshot(&self.conn, &self.lib, self.now)
    }

    /// Decide on a snapshot and write what the decision changed, in one
    /// transaction: the request path's frame (`DESIGN.md` §6).
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
    source, actor_id, actor_kind, declaration_version, recorded_at, occurred_at, imported, payload FROM of_event";

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
                },
            );
        }
    }
    spans(conn, lib, &mut world)?;
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
         entered_by, entered_by_kind FROM of_interval ORDER BY object_id, dimension, entered_position, entered_at",
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
        ))
    })?;
    for row in rows {
        let (id, dimension, value, entered_position, left_position, entered_at, left_at, by, kind) =
            row?;
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
            let Some(a) = lib.type_(&o.type_).and_then(|t| t.attribute(&dimension)) else {
                return Err(StoreError(format!(
                    "an interval of {id}.{dimension}, which is not an attribute"
                )));
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
                "INSERT INTO of_object (id, type, created_at, created_by_kind, recorded_from) VALUES (?1, ?2, ?3, ?4, ?3)",
                params![o.id, o.type_, at, o.created_by_kind],
            )?;
        }
        tx.execute(
            "INSERT INTO of_event (position, txn, object_id, object_seq, transition, from_state, to_state, changes_state, \
             source, actor_id, actor_kind, declaration_version, taint_version, recorded_at, occurred_at, payload, reads) \
             VALUES (?1, (SELECT value FROM of_counter WHERE name = 'txn'), ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, ?12, ?13, ?14, '{}')",
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
            Sql::Integer(1),
            Sql::Integer(event.position),
        ];
        if t.states.iter().any(|(_, s)| s.superseding) {
            cols.push("superseded_by".to_string());
            vals.push(o.superseded_by.clone().map_or(Sql::Null, Sql::Text));
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
            for sp in spans.iter().filter(|sp| {
                sp.entered_position == event.position || sp.left_position == Some(event.position)
            }) {
                tx.execute(
                    "INSERT INTO of_interval (object_id, dimension, value, entered_position, left_position, \
                     entered_at, left_at, entered_by, entered_by_kind, declaration_version) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1) \
                     ON CONFLICT (object_id, dimension, entered_position, entered_at) DO UPDATE SET \
                     value = excluded.value, left_position = excluded.left_position, left_at = excluded.left_at",
                    params![
                        o.id,
                        dimension,
                        to_sql(&sp.value)?,
                        sp.entered_position,
                        sp.left_position,
                        timestamp_text(sp.entered_at),
                        sp.left_at.map(timestamp_text),
                        sp.entered_by,
                        sp.entered_by_kind,
                    ],
                )?;
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
    // personal values leave the log and the interval index, the events and
    // their shape kept (§9, steps 2 and 9)
    for (id, attributes) in &change.redacted {
        for a in attributes {
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
        }
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

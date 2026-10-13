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

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

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

/// What a store at another engine release is refused with, before its
/// reason (`library-api.md` §7, `EngineMismatch`).
pub const ENGINE_MISMATCH: &str = "engine_mismatch";

impl StoreError {
    /// Whether it is `EngineMismatch`: the store is at another release.
    pub fn is_mismatch(&self) -> bool {
        self.0.starts_with(ENGINE_MISMATCH)
    }

    /// Whether the store's volume was full.
    pub fn is_disk_full(&self) -> bool {
        self.0.starts_with(DISK_FULL)
    }

    /// Whether a write found the store's write lock held past the busy
    /// timeout and its retries (ADR-0178 decision 2), which a request
    /// answers `stale`, cause `retries_exhausted`.
    pub fn is_busy(&self) -> bool {
        self.0.starts_with(BUSY)
    }
}

/// What a write the store's full volume stopped is refused with, before its
/// reason.
pub const DISK_FULL: &str = "disk_full";

/// What a write that met the store's lock past its busy timeout and every
/// retry is refused with, before its reason.
pub const BUSY: &str = "busy";

/// How many times a write transaction whose lock is held past the busy
/// timeout is begun again before it is given up (ADR-0178 decision 2).
pub const BUSY_RETRIES: i64 = 3;

/// The busy timeout a store's connections wait out unless told another.
pub const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

fn is_busy(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(f, _)
        if f.code == rusqlite::ErrorCode::DatabaseBusy || f.code == rusqlite::ErrorCode::DatabaseLocked)
}

/// A write transaction, begun `IMMEDIATE` so its reads and its writes are
/// serialised with every other writer's, a second process's on the file
/// included, its lock waited for up to the busy timeout and begun again up
/// to `BUSY_RETRIES` times: the transaction, and the retries it took
/// (`storage-schema.md` §7, ADR-0090, ADR-0178 decision 2). A macro, so
/// the transaction borrows the connection only where it is begun.
macro_rules! begin_write {
    ($conn:expr) => {{
        let mut retries = 0;
        loop {
            match $conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate) {
                Ok(tx) => break (tx, retries),
                Err(e) if is_busy(&e) && retries < BUSY_RETRIES => retries += 1,
                Err(e) if is_busy(&e) => {
                    return Err(StoreError(format!(
                        "{BUSY}: the store's write lock was held past the busy timeout {} times",
                        retries + 1
                    ))
                    .into());
                }
                Err(e) => return Err(StoreError::from(e).into()),
            }
        }
    }};
}

/// The declaration version a store is at.
fn declared_version(conn: &Connection) -> Stored<i64> {
    Ok(conn.query_row("SELECT max(version) FROM of_declaration", [], |r| r.get(0))?)
}

/// Every operation reads the store's release in its transaction, and one at
/// another release refuses, so no copy of an earlier release serves a
/// store once it is upgraded (`storage-schema.md` §12, ADR-0177 decision 3).
fn release_held(conn: &Connection) -> Stored<()> {
    let at: Option<String> = conn
        .query_row(
            "SELECT release FROM of_engine_release ORDER BY seq DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            e => Err(e),
        })?;
    match at {
        Some(r) if r == BUILTINS => Ok(()),
        other => Err(StoreError(format!(
            "{ENGINE_MISMATCH}: the store is at {}, and this core is {BUILTINS}",
            other.as_deref().unwrap_or("no release")
        ))),
    }
}

/// The store's own schema and the built-ins, as this release creates them:
/// the tables of §6, each built-in type's table, states, transitions and
/// invariants, and the standard metrics, digested; its release records it
/// (ADR-0177 decision 1).
pub fn release_fingerprint() -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(SCHEMA.as_bytes());
    if let Ok(lib) = Library::load(&[]) {
        for (name, t) in &lib.types {
            h.update(format!("\ntype {name}\n").as_bytes());
            // an identity's folded index, made at a store's open whatever
            // release made the store, as the actor indexes are, is no part
            // of what a release holds (ADR-0194 decision 1)
            let kept: String = table_parts(&lib, t, false)
                .create(&table(name), &[])
                .lines()
                .filter(|l| !l.contains("_fold_ix"))
                .map(|l| format!("{l}\n"))
                .collect();
            h.update(kept.as_bytes());
            for (s, st) in &t.states {
                h.update(format!("state {s} {} {}\n", st.category, st.is_final).as_bytes());
            }
            for (x, tr) in &t.transitions {
                h.update(format!("transition {x} {:?}\n", tr.kind).as_bytes());
            }
            for (i, _) in &t.invariants {
                h.update(format!("invariant {i}\n").as_bytes());
            }
        }
    }
    for m in crate::metric::STANDARD_NAMES {
        h.update(format!("metric {m}\n").as_bytes());
    }
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> StoreError {
        // a full volume, which a client may retry once space is freed,
        // told apart from the store's own faults (fourth production review)
        if let rusqlite::Error::SqliteFailure(f, _) = &e
            && f.code == rusqlite::ErrorCode::DiskFull
        {
            return StoreError(format!("{DISK_FULL}: {e}"));
        }
        StoreError(e.to_string())
    }
}

/// An operation of the store's own stopped, its read of the store among the
/// reasons it can stop.
impl From<NotRun> for StoreError {
    fn from(e: NotRun) -> StoreError {
        StoreError(e.what().to_string())
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
    /// The event whose transition caused this one: a call's, a creation's,
    /// a cascade's or an erasure's, an approval's for the request it ran,
    /// and a publish's for its migrations (`DESIGN.md` §6 step 8, §9).
    pub cause: Option<i64>,
    pub declaration_version: i64,
    pub recorded_at: i64,
    pub occurred_at: Option<i64>,
    pub imported: bool,
    pub payload: serde_json::Value,
    /// The read set of the rules evaluated for it, as JSON (ADR-0088,
    /// ADR-0158): empty where it evaluated none.
    pub reads: serde_json::Value,
    /// Why the request it came from was sent, the caller's reason
    /// (`DESIGN.md` §6); an approval's executed events record the proposer's.
    pub context: Option<String>,
    /// The serialisation retries its request took (`DESIGN.md` §6 step 8,
    /// ADR-0178 decision 2); none but a write's that met a busy lock.
    pub retries: i64,
}

/// What a request writes: the objects it created or changed, each with its
/// event, in the order the events were allocated.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Change {
    pub writes: Vec<(Object, bool, Event)>,
    /// The serialisation retries its transaction took, which each event it
    /// writes records (`DESIGN.md` §6 step 8, ADR-0178 decision 2).
    pub retries: i64,
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
    /// Each past event's personal input an erasure redacts, by the event's
    /// position and the input's name (§9 step 8).
    pub redacted_inputs: Vec<(i64, String)>,
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
    /// External identifiers an object no longer holds: source, value and
    /// object, removed before any is kept.
    pub external_removed: Vec<(String, String, String)>,
    /// Interval spans an erasure's walk down redacted on another object: the
    /// object, the dimension and the span's entry (§9 step 12).
    pub redacted_spans: Vec<(String, String, i64)>,
    /// The legacy entries an import attaches, each to its object, with the
    /// ordinal the import gave it (ADR-0163).
    pub legacy: Vec<(String, crate::world::LegacyEntry)>,
    /// Sequences it raises past a value, never lowering one: sequence, scope
    /// and value (ADR-0154).
    pub raised: Vec<(String, String, i64)>,
    /// The attempt log's rows written with the events they are linked to: a
    /// raised flag and an observing clause's would-be refusal (`DESIGN.md`
    /// §7, `storage-schema.md` §6).
    pub attempts: Vec<Attempt>,
    /// The record of a request its actor sent with a key: the actor's id,
    /// the key, the request's digest, its first event and its verdict,
    /// written in its transaction so a retry replays only what committed
    /// (`storage-schema.md` §6).
    pub idempotency: Option<(String, String, String, Option<i64>, serde_json::Value)>,
    /// Each erased actor's identity, with its object's id, which replaces it
    /// in every event, attempt, interval, idempotency record and recorded
    /// actor of a proposal or a change (ADR-0192 decision 4).
    pub renamed_actors: Vec<(String, String)>,
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
    /// The request's non-personal inputs and its occurred time.
    pub request_values: serde_json::Value,
    pub withheld: Vec<String>,
    pub declaration_version: i64,
    /// Everything the request had read when refused, or, for an observing
    /// clause's row, what the clause read (ADR-0088 §3); and the values the
    /// failing clause consulted (ADR-0096, ADR-0158). Null where none.
    pub reads: serde_json::Value,
    pub consulted: serde_json::Value,
    /// Its writing transaction, by which the attempt export pages (ADR-0100
    /// §6): 0 until it is written.
    pub txn: i64,
    /// For a refused call or creation, the object it targeted and the kind
    /// of the verdict it received (ADR-0159); null otherwise.
    pub call: serde_json::Value,
    /// Why the request was sent, the caller's reason (`DESIGN.md` §7).
    pub context: Option<String>,
    /// For an approval its held request refused, the proposal it would have
    /// executed, whose `last_verdict` the row is (ADR-0157); none otherwise.
    pub proposal: Option<String>,
}

fn insert_attempt(tx: &Connection, a: &Attempt, txn: &str) -> Stored<()> {
    let values = serde_json::json!({ "values": a.request_values, "withheld": a.withheld });
    tx.execute(
        &format!(
            "INSERT INTO of_attempt (at, actor_id, actor_kind, txn, type, object_id, transition, verdict, clause, remedy, \
             unknown, enforced, flagged, applied_position, request_values, declaration_version, actor_principal, \
             reads, consulted, call, context, proposal) \
             VALUES (?1, ?2, ?3, {txn}, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)"
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
            (!a.reads.is_null()).then(|| a.reads.to_string()),
            (!a.consulted.is_null()).then(|| a.consulted.to_string()),
            (!a.call.is_null()).then(|| a.call.to_string()),
            a.context,
            a.proposal,
        ],
    )?;
    Ok(())
}

fn attempt_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Attempt> {
    let at: String = r.get(0)?;
    let values = json_column(r, 13, "an attempt's request_values")?;
    Ok(Attempt {
        at: time_column(0, &at, "an attempt's at")?,
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
        reads: json_column(r, 16, "an attempt's reads")?,
        consulted: json_column(r, 17, "an attempt's consulted")?,
        txn: r.get(18)?,
        call: json_column(r, 19, "an attempt's call")?,
        context: r.get(20)?,
        proposal: r.get(21)?,
    })
}

/// A JSON column as written, null where empty.
/// A column of a row read that does not decode, which fails the row: a
/// damaged row is refused, never read as a default. The error names the
/// column and what it holds, not its text, which may be personal.
fn undecodable(column: usize, what: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        Box::new(StoreError(format!("{what}, which does not decode"))),
    )
}

/// A time column's text, decoded.
fn time_column(column: usize, text: &str, what: &str) -> rusqlite::Result<i64> {
    parse_timestamp(text).ok_or_else(|| undecodable(column, what))
}

/// A JSON column, decoded; `null` where the column holds none.
fn json_column(
    r: &rusqlite::Row<'_>,
    column: usize,
    what: &str,
) -> rusqlite::Result<serde_json::Value> {
    match r.get::<_, Option<String>>(column)? {
        None => Ok(serde_json::Value::Null),
        Some(text) => serde_json::from_str(&text).map_err(|_| undecodable(column, what)),
    }
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
    let mut indexes = vec![
        format!(
            "CREATE INDEX {} ON {name} (state);",
            quote(&format!("{name}_state_ix"))
        ),
        // the objects whose state an assertion last set, which
        // `exceptions(type)` reads by (§3.2)
        format!(
            "CREATE INDEX {} ON {name} (state_source);",
            quote(&format!("{name}_asserted_ix"))
        ),
    ];
    // the built-in label kind, read by its name and the state its subject
    // was in (§6)
    if t.name == crate::model::LABEL {
        indexes.push(format!(
            "CREATE INDEX {} ON {name} (name, subject_state);",
            quote(&format!("{name}_by_name"))
        ));
    }
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
    indexes.extend(attribute_indexes(lib, t));
    // each uniqueness's key, which the uniqueness reads by: an index and
    // not a constraint, which an admitted conflict would refuse (ADR-0165);
    // a reference's key is its column, indexed above as a stored end
    for (n, a) in &t.attributes {
        if a.many {
            continue;
        }
        let unique = crate::request::effective_unique(lib, a);
        let Some((attrs, _)) = crate::request::unique_key(n, a, &unique) else {
            continue;
        };
        let Some(cols) = attrs
            .iter()
            .map(|k| {
                t.attribute(k)
                    .filter(|s| !s.many)
                    .map(|s| quote(&column(k, s)))
            })
            .collect::<Option<Vec<String>>>()
        else {
            continue;
        };
        if cols.len() == 1 && matches!(a.declared, Declared::Reference(_)) {
            continue;
        }
        indexes.push(format!(
            "CREATE INDEX {} ON {name} ({});",
            quote(&format!("{name}_{n}_key_ix")),
            cols.join(", ")
        ));
    }
    TableParts {
        name,
        columns,
        constraints,
        sets,
        indexes,
    }
}

/// The index on each attribute a query may filter on, `indexed`, and on each
/// external identifier, which every import, publish and `lookup` reads by
/// value, where a reference's or a single-attribute key's index does not
/// already serve it (`storage-schema.md` §10). The core made none before
/// D593, so every read by such an attribute, an import's legacy key among
/// them, read its whole table, and none on an external identifier not marked
/// `indexed` before D606, so each import batch's checks of them grew with
/// the store; [`Store::open`] makes one a store lacks.
fn attribute_indexes(lib: &Library, t: &Type) -> Vec<String> {
    let name = table(&t.name);
    t.attributes
        .iter()
        .filter(|(n, a)| {
            (a.indexed || a.external.is_some())
                && !a.many
                && !a.derived_end()
                && !matches!(a.declared, Declared::Reference(_))
                && !crate::request::unique_key(n, a, &crate::request::effective_unique(lib, a))
                    .is_some_and(|(key, _)| key == [n.to_string()])
        })
        .map(|(n, a)| {
            let col = column(n, a);
            format!(
                "CREATE INDEX IF NOT EXISTS {} ON {name} ({});",
                quote(&format!("{name}_{col}_ix")),
                quote(&col)
            )
        })
        .chain(
            // an actor's identity folded, which every check of an identity
            // and every request naming one by another case reads by, where
            // each scanned every actor (seventh production review
            // PUBLISH-SLOW, ADR-0194 decision 1)
            t.actor_identity()
                .and_then(|(n, _)| t.attribute(n).map(|a| (n, a)))
                .filter(|(_, a)| !a.many && !a.derived_end())
                .map(|(n, a)| {
                    let col = column(n, a);
                    format!(
                        "CREATE INDEX IF NOT EXISTS {} ON {name} (lower({}));",
                        quote(&format!("{name}_{col}_fold_ix")),
                        quote(&col)
                    )
                }),
        )
        .chain(
            // the successor's column, which a group erasure reads each
            // member's predecessors by, where it read every superseding
            // type whole for each member (fifth production review,
            // ADR-0191 decision 5)
            t.states.iter().any(|(_, s)| s.superseding).then(|| {
                format!(
                    "CREATE INDEX IF NOT EXISTS {} ON {name} (superseded_by);",
                    quote(&format!("{name}_superseded_by_ix"))
                )
            }),
        )
        .collect()
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
) -> Stored<Event> {
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
            let spans = o.parts_mut()?.spans.entry(d.clone()).or_default();
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
    Ok(Event {
        position,
        retries: 0,
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
        cause: None,
        declaration_version,
        recorded_at: now,
        occurred_at: Some(now),
        imported: false,
        payload,
        reads: serde_json::json!({}),
        context: None,
    })
}

/// What a version kept for history on an erased object, cleared where it
/// was personal under the version it belongs to (ADR-0146 decision 2,
/// `storage-schema.md` §10): a column a later version dropped, kept under
/// its own name, and one a type change set aside as `<column>__v<n>`; the
/// intervals set aside with it as `<attribute>__v<n>`; and a personal set's
/// side table, dropped or set aside. Personal under a version means as its
/// library declares the attribute; a dropped column was personal where any
/// earlier version declaring it marked it so.
fn clear_kept(
    tx: &Transaction<'_>,
    lib: &Library,
    earlier: &BTreeMap<i64, Library>,
    o: &Object,
) -> Stored<()> {
    let Some(t) = lib.type_(&o.type_) else {
        return Ok(());
    };
    let name = table(&t.name);
    // the attribute a column held under a version, and whether it was
    // personal there; under any earlier version where none is named
    let personal = |col: &str, version: Option<i64>| -> bool {
        let under = |l: &Library| {
            l.type_(&t.name).is_some_and(|lt| {
                lt.attributes
                    .iter()
                    .any(|(n, a)| a.personal && (column(n, a) == col || n == col))
            })
        };
        match version {
            Some(v) if v == lib.version => under(lib),
            Some(v) => earlier.get(&v).is_some_and(under),
            None => earlier.values().any(under),
        }
    };
    let split = |c: &str| match c.rsplit_once("__v") {
        Some((base, v)) => match v.parse::<i64>() {
            Ok(v) => (base.to_string(), Some(v)),
            Err(_) => (c.to_string(), None),
        },
        None => (c.to_string(), None),
    };
    let own: Vec<String> = table_parts(lib, t, false)
        .columns
        .iter()
        .map(|(c, _)| c.clone())
        .collect();
    let mut kept = Vec::new();
    {
        let mut info = tx.prepare(&format!("PRAGMA table_info({})", quote(&name)))?;
        let rows = info.query_map([], |r| r.get::<_, String>(1))?;
        for row in rows {
            let c = row?;
            if !own.contains(&c) {
                kept.push(c);
            }
        }
    }
    for c in kept {
        let (base, version) = split(&c);
        if personal(&base, version) {
            tx.execute(
                &format!("UPDATE {name} SET {} = NULL WHERE id = ?1", quote(&c)),
                [&o.id],
            )?;
        }
    }
    // the intervals set aside with a type change
    let mut dims = Vec::new();
    {
        let mut q = tx.prepare(
            "SELECT DISTINCT dimension FROM of_interval WHERE object_id = ?1 AND dimension LIKE '%\\_\\_v%' ESCAPE '\\'",
        )?;
        let rows = q.query_map([&o.id], |r| r.get::<_, String>(0))?;
        for row in rows {
            dims.push(row?);
        }
    }
    for d in dims {
        let (base, version) = split(&d);
        if version.is_some() && personal(&base, version) {
            tx.execute(
                "UPDATE of_interval SET value = NULL WHERE object_id = ?1 AND dimension = ?2",
                params![o.id, d],
            )?;
        }
    }
    // a personal set's side table, dropped or set aside
    let mut sides = Vec::new();
    {
        let mut q = tx.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE ?1 ESCAPE '\\'",
        )?;
        let pattern = format!("{}\\_\\_%", name.replace('_', "\\_"));
        let rows = q.query_map([pattern], |r| r.get::<_, String>(0))?;
        for row in rows {
            sides.push(row?);
        }
    }
    let prefix = format!("{name}__");
    for side in sides {
        let Some(rest) = side.strip_prefix(&prefix) else {
            continue;
        };
        let (attribute, version) = split(rest);
        let current = version.is_none() && t.attribute(&attribute).is_some();
        if !current && personal(&attribute, version) {
            tx.execute(
                &format!("DELETE FROM {} WHERE object_id = ?1", quote(&side)),
                [&o.id],
            )?;
        }
    }
    Ok(())
}

/// An erased object's legacy entries, each left with the fields its kind
/// keeps, every other replaced by absence (`storage-schema.md` §9 step 6).
fn redact_legacy(tx: &Transaction<'_>, id: &str) -> Stored<()> {
    let rows: Vec<(i64, String, String)> = {
        let mut q =
            tx.prepare("SELECT ordinal, payload, kept FROM of_legacy_entry WHERE object_id = ?1")?;
        let rows = q.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for (ordinal, payload, kept) in rows {
        // a damaged entry stops the erasure, rather than being overwritten
        // with what a default would keep
        let damaged = |column: &str| {
            StoreError(format!(
                "{id}'s legacy entry {ordinal}'s {column}, which does not decode"
            ))
        };
        let entry = crate::world::LegacyEntry {
            ordinal,
            kind: String::new(),
            occurred_at: 0,
            payload: serde_json::from_str(&payload).map_err(|_| damaged("payload"))?,
            kept: serde_json::from_str(&kept).map_err(|_| damaged("kept"))?,
        };
        tx.execute(
            "UPDATE of_legacy_entry SET payload = ?3 WHERE object_id = ?1 AND ordinal = ?2",
            params![
                id,
                ordinal,
                serde_json::Value::Object(entry.redacted()).to_string()
            ],
        )?;
    }
    Ok(())
}

/// The attempt log, in the order written, which a world reads where an
/// operation reads it (ADR-0162 step 1).
pub(crate) fn read_attempts(conn: &Connection) -> rusqlite::Result<Vec<Attempt>> {
    let mut attempts = conn.prepare(
        "SELECT at, actor_id, actor_kind, type, object_id, transition, verdict, clause, remedy, unknown, enforced, \
         flagged, applied_position, request_values, declaration_version, actor_principal, reads, consulted, txn, call, context, \
         proposal FROM of_attempt ORDER BY id",
    )?;
    let rows = attempts.query_map([], attempt_row)?;
    rows.collect()
}

/// The pruned attempt rows' daily counts, as rows of the `attempt_counts`
/// source, which a world reads where an operation reads them.
pub(crate) fn read_rollup(
    conn: &Connection,
) -> rusqlite::Result<Vec<(BTreeMap<String, Value>, i64)>> {
    let mut rolled = conn.prepare(
        "SELECT day, object_id, verdict, type, transition, clause, remedy, actor_kind, enforced, flagged, \
         declaration_version, count FROM of_attempt_rollup",
    )?;
    let rows = rolled.query_map([], |r| {
        let text = |i: usize| -> rusqlite::Result<Value> {
            let t: String = r.get(i)?;
            Ok(if t.is_empty() {
                Value::Absent
            } else {
                Value::Text(t)
            })
        };
        let day: String = r.get(0)?;
        let object: String = r.get(1)?;
        let mut f = BTreeMap::new();
        f.insert(
            "day".to_string(),
            crate::value::parse_timestamp(&format!("{day}T00:00:00Z"))
                .map_or(Value::Absent, |t| Value::Date(t.div_euclid(86_400))),
        );
        f.insert(
            "object".to_string(),
            if object.is_empty() {
                Value::Absent
            } else {
                Value::Object(object)
            },
        );
        f.insert("verdict".to_string(), text(2)?);
        f.insert("type".to_string(), text(3)?);
        f.insert("transition".to_string(), text(4)?);
        f.insert("clause".to_string(), text(5)?);
        f.insert("remedy".to_string(), text(6)?);
        f.insert("actor_kind".to_string(), text(7)?);
        f.insert(
            "enforced".to_string(),
            Value::Bool(r.get::<_, i64>(8)? != 0),
        );
        f.insert("flagged".to_string(), Value::Bool(r.get::<_, i64>(9)? != 0));
        f.insert("declaration_version".to_string(), Value::Int(r.get(10)?));
        Ok((f, r.get::<_, i64>(11)?))
    })?;
    rows.collect()
}

/// What a key an actor applied recorded: the digest of the request it
/// applied and the verdict it was given, which a retry replays; none for a
/// key it has not applied (`storage-schema.md` §6, `of_idempotency`).
pub(crate) fn read_idempotency(
    conn: &Connection,
    actor: &str,
    key: &str,
) -> Result<Option<(String, serde_json::Value)>, StoreError> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT request_digest, verdict FROM of_idempotency WHERE actor_id = ?1 AND key = ?2",
            params![actor, key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    let Some((digest, verdict)) = row else {
        return Ok(None);
    };
    let verdict = serde_json::from_str(&verdict)
        .map_err(|e| StoreError(format!("{actor}'s key {key}'s verdict: {e}")))?;
    Ok(Some((digest, verdict)))
}

/// An object's row as its type's table holds it: its state, version, state
/// source, successor where its type supersedes, and its scalar columns.
type RowFields = (String, i64, String, Option<String>, Vec<Sql>);

/// The log the store holds, in position order, which a world reads where an
/// operation reads it (ADR-0162 step 1).
/// The events after a position, in position order, at most `limit`, of the
/// objects of the types given where some are: a page of the log read from a
/// cursor by its key, where every pull and export read the log whole
/// (fourth production review FEED).
pub(crate) fn read_log_after(
    conn: &Connection,
    after: i64,
    types: Option<&[String]>,
    limit: usize,
) -> rusqlite::Result<Vec<Event>> {
    let mut binds: Vec<Sql> = vec![Sql::Integer(after)];
    let mut sql = EVENT_SELECT.to_string();
    match types {
        Some(types) => {
            let marks: Vec<String> = types
                .iter()
                .map(|t| {
                    binds.push(Sql::Text(t.clone()));
                    format!("?{}", binds.len())
                })
                .collect();
            sql.push_str(&format!(
                " JOIN of_object o ON o.id = of_event.object_id WHERE of_event.position > ?1 AND o.type IN ({})",
                marks.join(", ")
            ));
        }
        None => sql.push_str(" WHERE of_event.position > ?1"),
    }
    binds.push(Sql::Integer(i64::try_from(limit).unwrap_or(i64::MAX)));
    sql.push_str(&format!(
        " ORDER BY of_event.position LIMIT ?{}",
        binds.len()
    ));
    let mut page = conn.prepare(&sql)?;
    let rows = page.query_map(rusqlite::params_from_iter(binds), event_row)?;
    rows.collect()
}

/// The digest an idempotency record holds once an erasure redacted what its
/// request sent: the key replays its verdict whatever is sent under it
/// (fourth production review A-5).
pub const ERASED_DIGEST: &str = "erased";

/// A counter's value: the last position, transaction or object number
/// the store has handed out.
pub(crate) fn read_counter(conn: &Connection, name: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT value FROM of_counter WHERE name = ?1",
        params![name],
        |r| r.get(0),
    )
}

/// The log's last position, 0 for an empty log.
pub(crate) fn read_last_position(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("SELECT COALESCE(MAX(position), 0) FROM of_event", [], |r| {
        r.get(0)
    })
}

/// A connection that only looks at a store's file, carrying no write-ahead
/// log into the file as it closes, so a start that refuses a store leaves
/// the file and its log as they were: the release a start reads first, on a
/// plain connection, wrote another state's log into a restored copy as it
/// closed, which the check after it did not (fourth production review A-16,
/// fifth production review).
pub(crate) fn looking(path: &std::path::Path) -> Stored<Connection> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.set_db_config(
        rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
        true,
    )?;
    Ok(conn)
}

/// The events a clause over one value selects, in position order: an
/// event by its position, an object's own, or those one caused, each by
/// the log's indexes (fourth production review).
pub(crate) fn read_events(
    conn: &Connection,
    clause: &str,
    value: &dyn rusqlite::ToSql,
) -> rusqlite::Result<Vec<Event>> {
    let mut read = conn.prepare(&format!("{EVENT_SELECT} WHERE {clause} ORDER BY position"))?;
    let rows = read.query_map([value], event_row)?;
    rows.collect()
}

/// An object's first event's position, by the index on its events.
pub(crate) fn read_first_position(conn: &Connection, id: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT MIN(position) FROM of_event WHERE object_id = ?1",
        params![id],
        |r| r.get(0),
    )
}

pub(crate) fn read_log(conn: &Connection) -> rusqlite::Result<Vec<Event>> {
    let mut log = conn.prepare(&format!("{EVENT_SELECT} ORDER BY position"))?;
    let rows = log.query_map([], event_row)?;
    rows.collect()
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
        // a file as its wire form, which a column of `file` reads back
        file @ Value::File { .. } => Sql::Text(crate::wire::encode(file).to_string()),
        Value::Set(_) => return Err(StoreError("a set in a column".to_string())),
        Value::Row(_) => return Err(StoreError("a row of flow data in a column".to_string())),
    })
}

/// Whether a value compared in SQL with a column of the declared type
/// finds every row the language's equality would: an integer only in an
/// integer column, a decimal only in a decimal column of its own scale, as
/// the language compares the two numerically and the columns hold them in
/// different units; any other value is held as its column holds it, or not
/// equal to what the column holds at all.
fn comparable(v: &Value, declared: &Declared) -> bool {
    let Declared::Scalar(t) = declared else {
        return !matches!(v, Value::Int(_) | Value::Decimal(_));
    };
    match v {
        Value::Int(_) => matches!(t.as_str(), "int" | "counter" | "event"),
        Value::Decimal(d) => t
            .strip_prefix("decimal(")
            .and_then(|r| r.trim_end_matches(')').rsplit(',').next())
            .and_then(|s| s.trim().parse::<u8>().ok())
            .is_some_and(|s| s == d.scale()),
        _ => true,
    }
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
                ("file", Sql::Text(s)) => serde_json::from_str(s)
                    .ok()
                    .and_then(|j| crate::wire::file_of(&j).ok())
                    .ok_or_else(bad),
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
CREATE TABLE of_subscription_position (
  subscription_id  TEXT    PRIMARY KEY REFERENCES of_object(id),
  acknowledged     TEXT    NOT NULL,
  acknowledged_at  TEXT    NOT NULL
);
-- The daily counts of the attempt rows maintain pruned (storage-schema.md §6).
CREATE TABLE of_attempt_rollup (
  day          TEXT    NOT NULL,
  object_id    TEXT    NOT NULL DEFAULT '',
  verdict      TEXT    NOT NULL,
  type         TEXT    NOT NULL DEFAULT '',
  transition   TEXT    NOT NULL DEFAULT '',
  clause       TEXT    NOT NULL DEFAULT '',
  remedy       TEXT    NOT NULL DEFAULT '',
  actor_kind   TEXT    NOT NULL,
  enforced     INTEGER NOT NULL,
  flagged      INTEGER NOT NULL DEFAULT 0,
  declaration_version INTEGER NOT NULL,
  count        INTEGER NOT NULL,
  PRIMARY KEY (day, type, object_id, verdict, transition, clause, remedy, actor_kind, enforced,
               flagged, declaration_version)
);
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
  call                TEXT,
  proposal            TEXT,
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
CREATE TABLE of_legacy_entry (
  object_id   TEXT    NOT NULL REFERENCES of_object(id),
  ordinal     INTEGER NOT NULL,
  kind        TEXT    NOT NULL,
  occurred_at TEXT    NOT NULL,
  payload     TEXT    NOT NULL,
  kept        TEXT    NOT NULL,
  PRIMARY KEY (object_id, ordinal)
);
CREATE TABLE of_external_id (
  source     TEXT NOT NULL,
  value      TEXT NOT NULL,
  object_id  TEXT NOT NULL REFERENCES of_object(id),
  attribute  TEXT NOT NULL,
  PRIMARY KEY (source, value)
);
-- Idempotency: the verdict of each request an actor's key applied, replayed
-- verbatim to a retry; scoped to the actor's id (storage-schema.md §6,
-- ADR-0077). Nothing prunes it yet.
CREATE TABLE of_idempotency (
  actor_id       TEXT    NOT NULL,
  key            TEXT    NOT NULL,
  request_digest TEXT    NOT NULL,
  first_position INTEGER REFERENCES of_event(position),
  verdict        TEXT    NOT NULL,
  applied_at     TEXT    NOT NULL,
  PRIMARY KEY (actor_id, key)
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
  name       TEXT    NOT NULL,
  scope_key  TEXT    NOT NULL,                      -- '' for a sequence with no scope
  next_value INTEGER NOT NULL,                      -- the next number it draws
  PRIMARY KEY (name, scope_key)
);
-- The engine release the store is at, and every upgrade (storage-schema.md
-- §6, §12, ADR-0177): every operation reads the last row.
CREATE TABLE of_engine_release (
  seq           INTEGER PRIMARY KEY,
  release       TEXT    NOT NULL,
  installed_at  TEXT    NOT NULL,
  installed_by  TEXT    NOT NULL
);
";

/// A store and the library its rules come from.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
    /// The library its rules come from, shared with the reads that run
    /// beside its writes (ADR-0178).
    pub lib: Arc<Library>,
    /// The store's clock, in seconds since 1970: a sandbox's caller holds it
    /// (PRD F13).
    pub now: i64,
    /// What its evaluators answer, `<evaluator>.<function>` to `satisfied`,
    /// `unsatisfied` or `unanswered`: a sandbox's caller sets them (PRD F13).
    pub answers: BTreeMap<String, String>,
    /// How long attempt rows, and idempotency records, are kept before
    /// `maintain` may prune them, in seconds: the deployment's, set when the
    /// store is built, with no default, since they are numbers to measure
    /// (`DESIGN.md` §8, ADR-0101, ADR-0103); a store built with none prunes
    /// nothing.
    pub attempt_retention: Option<i64>,
    pub idempotency_retention: Option<i64>,
    /// Each earlier version's library, loaded once from the source it
    /// recorded, since a published version never changes: what an erasure
    /// reads was personal under (ADR-0146 decision 2).
    earlier: Arc<BTreeMap<i64, Library>>,
    /// Whether an erasure committed since the write-ahead log was last
    /// truncated, so the log's earlier frames still hold what it erased
    /// (D584).
    checkpoint_due: bool,
    /// A change's dry run prepared on a read for the next request, which
    /// takes it (DRYRUN-LOCK).
    pub prepared: Option<Box<crate::publish::Prepared>>,
    /// When a read on this store must have answered by, which each world it
    /// snapshots carries (ADR-0184 decision 3); none for the writer.
    pub deadline: Option<std::time::Instant>,
    /// How long its connection waits out another's lock, restored after
    /// the truncation an erasure is due, which waits for nothing.
    busy: std::cell::Cell<std::time::Duration>,
}

/// The largest number a sequence draws, so the next value the store keeps
/// for it always fits (fourth production review A-15).
pub const SEQUENCE_MOST: i64 = i64::MAX - 1;

/// What a checkpoint that empties the write-ahead log cuts it back to, in
/// bytes (third production review N6).
pub const WAL_LIMIT: i64 = 64 * 1024 * 1024;

/// Whether an actor names the store's operator, by its object's id or its
/// identity (ADR-0126).
fn is_the_operator(world: &World<'_>, actor: &str) -> Result<bool, NotRun> {
    if world
        .get(actor)?
        .is_some_and(|o| o.type_ == crate::model::OPERATOR_TYPE)
    {
        return Ok(true);
    }
    Ok(world
        .of_type(crate::model::OPERATOR_TYPE)?
        .iter()
        .any(|o| matches!(o.attr("identity"), Value::Text(t) if t == actor)))
}

/// How many read connections a deployment keeps idle for the next reads;
/// more are opened while more reads run at once (ADR-0178 decision 6).
pub const IDLE_READERS: usize = 8;

/// A deployment's read connections to its store's file (ADR-0178): a read
/// takes one, idle or new, in a read transaction of its own, and gives it
/// back when it ends.
#[derive(Debug)]
pub struct Readers {
    path: std::path::PathBuf,
    busy_timeout: std::time::Duration,
    idle: std::sync::Mutex<Vec<Connection>>,
}

impl Readers {
    pub fn new(path: &std::path::Path, busy_timeout: std::time::Duration) -> Readers {
        Readers {
            path: path.to_path_buf(),
            busy_timeout,
            idle: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// A read of the store as it stands, with the library, the earlier
    /// versions', the clock and the evaluator answers the writer holds.
    pub fn read(
        &self,
        lib: Arc<Library>,
        earlier: Arc<BTreeMap<i64, Library>>,
        now: i64,
        answers: BTreeMap<String, String>,
    ) -> Stored<Store> {
        let idle = self.idle.lock().ok().and_then(|mut idle| idle.pop());
        let conn = match idle {
            Some(conn) => conn,
            None => {
                let conn = Store::read_connection(&self.path)?;
                conn.busy_timeout(self.busy_timeout)?;
                conn
            }
        };
        Store::reading(conn, lib, earlier, now, answers)
    }

    /// A read ended: its connection kept for the next, up to `IDLE_READERS`.
    pub fn give_back(&self, read: Store) {
        let conn = read.into_connection();
        if let Ok(mut idle) = self.idle.lock()
            && idle.len() < IDLE_READERS
        {
            idle.push(conn);
        }
    }
}

/// What `maintain` was asked to do (`library-api.md` §5, ADR-0100,
/// ADR-0103): prune attempt rows, or idempotency records, older than
/// `before`, or archive events.
#[derive(Clone, Debug, PartialEq)]
pub struct MaintenanceTask {
    pub name: String,
    pub before: i64,
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
        // the actor's columns an erasure rewrites, indexed beside the
        // schema, whose fingerprint they leave as it was (ADR-0192)
        conn.execute_batch(ACTOR_INDEXES)?;
        let mut store = Store {
            conn,
            lib: Arc::new(lib),
            now,
            answers: BTreeMap::new(),
            attempt_retention: None,
            checkpoint_due: false,
            deadline: None,
            prepared: None,
            busy: std::cell::Cell::new(BUSY_TIMEOUT),
            idempotency_retention: None,
            earlier: Arc::new(BTreeMap::new()),
        };
        for t in store.lib.types.values() {
            store.conn.execute_batch(&store.type_ddl(t))?;
        }
        store.create_operator()?;
        store.record_release()?;
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
        Arc::make_mut(&mut store.lib).version = 1;
        Ok(store)
    }

    /// SQLite's quick check of a store's file read with the write-ahead log
    /// beside it, before anything opens it to write: the first fault it
    /// finds, none where it holds. Its connection carries no log into the
    /// file as it closes, so a check that fails leaves the file and its log
    /// as they were, where a refused start wrote another state's log into a
    /// restored copy and deleted it, so the copy failed again once the log
    /// was removed (fourth production review A-16).
    pub fn checked_file(path: &std::path::Path) -> Stored<Option<String>> {
        let conn = looking(path)?;
        let first: String = conn.query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
        Ok((first != "ok").then_some(first))
    }

    /// SQLite's quick check of the store's file read with its write-ahead
    /// log: the first fault it finds, none where it holds. A copy restored
    /// beside another state's `-wal` opened and answered health while its
    /// reads failed (third production review N5).
    pub fn quick_check(&self) -> Stored<Option<String>> {
        let first: String = self
            .conn
            .query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
        Ok((first != "ok").then_some(first))
    }

    /// A deployment's store, one SQLite file (ADR-0174 decision 2): created
    /// where the file holds none, at version 0, with the built-ins and the
    /// operator, its flow to arrive by a published `DeclarationChange`; and
    /// otherwise reopened at its latest version, its library loaded from the
    /// source that version recorded, its clock no earlier than the last time
    /// it recorded. A store another engine release created is refused
    /// (decision 3), since only an upgrade moves a store between releases.
    pub fn open(path: &std::path::Path, now: i64) -> Stored<Store> {
        let conn = Connection::open(path)?;
        // a write answered for survives a crash of the process or the machine
        conn.execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;",
        )?;
        // what a write deletes or overwrites, an erasure's personal values
        // among it, is zeroed in the file rather than left in free space
        // (D584)
        conn.execute_batch("PRAGMA secure_delete = ON;")?;
        // the write-ahead log cut back to 64 MiB whenever a checkpoint
        // empties it, where it grew to six times the store's file and stayed
        // (third production review N6)
        conn.execute_batch(&format!("PRAGMA journal_size_limit = {WAL_LIMIT};"))?;
        // another connection's write lock is waited for, not met at once
        // (storage-schema.md §7, ADR-0178 decision 2)
        conn.busy_timeout(BUSY_TIMEOUT)?;
        let created: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'of_declaration'",
            [],
            |r| r.get(0),
        )?;
        let builtins = Library::load(&[]).map_err(|e| StoreError(format!("the built-ins: {e}")))?;
        let mut store = Store {
            conn,
            lib: Arc::new(builtins),
            now,
            answers: BTreeMap::new(),
            attempt_retention: None,
            checkpoint_due: false,
            deadline: None,
            prepared: None,
            busy: std::cell::Cell::new(BUSY_TIMEOUT),
            idempotency_retention: None,
            earlier: Arc::new(BTreeMap::new()),
        };
        if created == 0 {
            store.conn.execute_batch(SCHEMA)?;
            store.conn.execute_batch(ACTOR_INDEXES)?;
            for t in store.lib.types.values() {
                store.conn.execute_batch(&store.type_ddl(t))?;
            }
            store.create_operator()?;
            store.record_release()?;
            store.conn.execute(
                "INSERT INTO of_declaration (version, module, source, builtins, published_at, published_by, report) \
                 VALUES (0, '', '{}', ?1, ?2, ?1, '{}')",
                params![BUILTINS, timestamp_text(now)],
            )?;
            Arc::make_mut(&mut store.lib).version = 0;
            return Ok(store);
        }
        // a store at another release is upgraded first, by a core of this
        // one (storage-schema.md §12, ADR-0177)
        let release = crate::release::store_release(&store.conn)?;
        if release.as_deref() != Some(BUILTINS) {
            return Err(StoreError(format!(
                "{ENGINE_MISMATCH}: {} is at {}, and this core is {BUILTINS}",
                path.display(),
                release.as_deref().unwrap_or("no release")
            )));
        }
        let latest: i64 =
            store
                .conn
                .query_row("SELECT max(version) FROM of_declaration", [], |r| r.get(0))?;
        store.lib = store
            .library_at(latest)?
            .map(Arc::new)
            .ok_or_else(|| StoreError(format!("version {latest} recorded no source")))?;
        // an index the version declares and the store lacks, which a core
        // before D593 never made: made now, since an index changes no answer,
        // only how much of the store a read by it reads
        for t in store.lib.types.values() {
            for ddl in attribute_indexes(&store.lib, t) {
                store.conn.execute_batch(&ddl)?;
            }
        }
        // the actor's columns an erasure rewrites, read by index, which a
        // core before ADR-0192 never made
        store.conn.execute_batch(ACTOR_INDEXES)?;
        // never earlier than the last time the store recorded (decision 4)
        let last: Option<String> =
            store
                .conn
                .query_row("SELECT max(recorded_at) FROM of_event", [], |r| r.get(0))?;
        if let Some(at) = last.as_deref().and_then(parse_timestamp) {
            store.now = store.now.max(at);
        }
        Ok(store)
    }

    /// How long its connection waits out another's write lock before a
    /// write meets it as busy (ADR-0178 decision 2).
    pub fn set_busy_timeout(&self, wait: std::time::Duration) -> Stored<()> {
        self.busy.set(wait);
        Ok(self.conn.busy_timeout(wait)?)
    }

    /// A read of a deployment's store beside its writes (ADR-0178): a
    /// connection of its own to the store's file, in one read transaction,
    /// so every statement of the read sees one committed state; the library
    /// of the declaration version that state is at, the one given where they
    /// agree, so a read racing a publish reads each row under the rules it
    /// was written under; and the clock and evaluator answers given.
    pub fn reading(
        conn: Connection,
        lib: Arc<Library>,
        earlier: Arc<BTreeMap<i64, Library>>,
        now: i64,
        answers: BTreeMap<String, String>,
    ) -> Stored<Store> {
        conn.execute_batch("BEGIN")?;
        let mut store = Store {
            conn,
            lib,
            now,
            answers,
            attempt_retention: None,
            checkpoint_due: false,
            deadline: None,
            prepared: None,
            busy: std::cell::Cell::new(BUSY_TIMEOUT),
            idempotency_retention: None,
            earlier,
        };
        // its first read takes the snapshot the rest of the read sees
        let at = declared_version(&store.conn)?;
        if at != store.lib.version {
            store.lib = Arc::new(
                store
                    .library_at(at)?
                    .ok_or_else(|| StoreError(format!("version {at} recorded no source")))?,
            );
        }
        Ok(store)
    }

    /// A connection for reads of the store's file beside this store's
    /// writes, waiting out a write lock as this one does (ADR-0178).
    /// After an erasure commits, the write-ahead log truncated into the
    /// store's file, so the log's earlier frames, which held what it erased,
    /// leave the files as the pages `secure_delete` zeroed do (D584). A
    /// reader holding the log keeps it: the truncation is due again after
    /// the next write, until it completes. Whether it completed.
    pub fn after_erasure(&mut self, erased: bool) -> Stored<bool> {
        self.checkpoint_due |= erased;
        if !self.checkpoint_due {
            return Ok(true);
        }
        // tried once, waiting for no reader: a reader holding the log keeps
        // it until it ends, and the truncation stays due after the next
        // write; waiting here, under the writer's lock, made every write
        // take the busy timeout while any read older than the erasure was
        // open (ERASE-CHECKPOINT)
        self.conn.busy_timeout(std::time::Duration::ZERO)?;
        let tried: Result<i64, rusqlite::Error> =
            self.conn
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0));
        self.conn.busy_timeout(self.busy.get())?;
        let busy = match tried {
            Ok(busy) => busy,
            Err(e) => {
                let e = StoreError::from(e);
                if !e.is_busy() {
                    return Err(e);
                }
                1
            }
        };
        self.checkpoint_due = busy != 0;
        Ok(busy == 0)
    }

    /// The write-ahead log carried into the store's file and truncated,
    /// waiting `wait` for the reads still open on it, which a service holding
    /// back its long reads leaves short: whether it completed, an erasure's
    /// truncation, due till then, done with it. Reads that always overlapped
    /// kept the log growing, 3.2 GB in 150 s, and an erasure's frames in it
    /// (fourth production review A-5, A-14).
    pub fn truncate_log(&mut self, wait: std::time::Duration) -> Stored<bool> {
        self.conn.busy_timeout(wait)?;
        let tried: Result<i64, rusqlite::Error> =
            self.conn
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0));
        self.conn.busy_timeout(self.busy.get())?;
        let busy = match tried {
            Ok(busy) => busy,
            Err(e) => {
                let e = StoreError::from(e);
                if !e.is_busy() {
                    return Err(e);
                }
                1
            }
        };
        if busy == 0 {
            self.checkpoint_due = false;
        }
        Ok(busy == 0)
    }

    /// Whether an erasure's truncation of the log is still due.
    pub fn checkpoint_due(&self) -> bool {
        self.checkpoint_due
    }

    /// The write-ahead log carried into the store's file and truncated, as a
    /// service stopping cleanly leaves it: whether it completed, which a
    /// reader still open prevents.
    pub fn checkpoint(&self) -> Stored<bool> {
        let busy: i64 = self
            .conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))?;
        Ok(busy == 0)
    }

    pub fn read_connection(path: &std::path::Path) -> Stored<Connection> {
        let conn = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        Ok(conn)
    }

    /// A read's connection back, its transaction ended, for the next read.
    pub fn into_connection(self) -> Connection {
        let _ = self.conn.execute_batch("ROLLBACK");
        self.conn
    }

    /// What a read beside it shares of it: the library and the earlier
    /// versions' (ADR-0178).
    pub fn shared(&self) -> (Arc<Library>, Arc<BTreeMap<i64, Library>>) {
        (Arc::clone(&self.lib), Arc::clone(&self.earlier))
    }

    /// The DDL of a type's table and its set tables (§3, §3.3).
    fn type_ddl(&self, t: &Type) -> String {
        let parts = table_parts(&self.lib, t, false);
        parts.create(&table(&t.name), &[])
    }

    /// The release that creates the store, its first row of the release
    /// table, installed by its operator (`storage-schema.md` §6).
    fn record_release(&self) -> Stored<()> {
        self.conn.execute(
            "INSERT INTO of_engine_release (seq, release, installed_at, installed_by) VALUES (1, ?1, ?2, ?3)",
            params![BUILTINS, timestamp_text(self.now), OPERATOR],
        )?;
        Ok(())
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
             source, actor_id, actor_kind, declaration_version, taint_version, recorded_at, occurred_at, payload, reads, \
             context) \
             VALUES (?1, ?2, ?3, 1, 'install', NULL, 'ACTIVE', 1, 'observed', ?3, 'human', 0, 0, ?4, ?4, '{}', '{}', ?5)",
            // its own act, the context naming the release that created the
            // store, under version 0, which the same step installs
            // (storage-schema.md §6, ADR-0134)
            params![position, txn, OPERATOR, at, format!("store-creation:{BUILTINS}")],
        )?;
        tx.execute(
            "INSERT INTO t_operator (id, state, version, declaration_version, last_event, identity) VALUES (?1, 'ACTIVE', 1, 0, ?2, ?1)",
            params![OPERATOR, position],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// `maintain` (`DESIGN.md` §10): prunes attempt rows recorded before
    /// `before`, adding each to its day's count in the rollup, which the
    /// `attempt_counts` source reads with the rows that remain; or deletes the
    /// idempotency records applied before it, after which a retry under a
    /// pruned key is a new request. It refuses, as a value, a task it does not
    /// know and a `before` inside the retention the store was built with,
    /// which a store built with none puts everywhere; it changes no governed
    /// state or history, and returns the rows it moved or removed. Archiving
    /// events, a tiering of the log a single SQLite file has none of, is not
    /// run.
    pub fn maintain(
        &mut self,
        actor: &str,
        task: &MaintenanceTask,
    ) -> Stored<Result<Result<i64, &'static str>, NotRun>> {
        // an actor the store knows, its object in no final state, as a
        // request's is (DESIGN.md §6 step 1); who may maintain is the upper
        // layer's to decide, never a capability the store holds (DESIGN.md
        // §10, ADR-0114, ADR-0183 decision 5)
        {
            let world = self.snapshot()?;
            match crate::request::active(resolve_actor(&world, actor)?) {
                Ok(_) => {}
                Err(Verdict::Unsatisfied { clause, .. }) if clause == "actor_active" => {
                    return Ok(Ok(Err("actor_active")));
                }
                Err(_) => return Ok(Ok(Err("actor_known"))),
            }
        }
        let retention = match task.name.as_str() {
            "prune_attempts" => self.attempt_retention,
            "prune_idempotency" => self.idempotency_retention,
            "archive_events" => {
                return Ok(Err(NotRun::Unsupported(
                    "archiving events, which a single SQLite file has no tier for".into(),
                )));
            }
            _ => return Ok(Ok(Err("unknown_task"))),
        };
        // a store given no retention for the task prunes nothing, which a
        // refusal says rather than one inside a retention it does not have
        // (second production review, minor)
        match retention {
            None => return Ok(Ok(Err("no_retention"))),
            Some(kept) if task.before <= self.now - kept => {}
            Some(_) => return Ok(Ok(Err("inside_retention"))),
        }
        let before = timestamp_text(task.before);
        let (tx, _) = begin_write!(self.conn);
        release_held(&tx)?;
        let moved = if task.name == "prune_attempts" {
            // each row's day, as the rollup keys it: its recorded time's UTC
            // calendar day
            tx.execute(
                "INSERT INTO of_attempt_rollup (day, object_id, verdict, type, transition, clause, remedy, \
                 actor_kind, enforced, flagged, declaration_version, count) \
                 SELECT substr(at, 1, 10), COALESCE(object_id, ''), verdict, COALESCE(type, ''), \
                 COALESCE(transition, ''), COALESCE(clause, ''), remedy, actor_kind, enforced, flagged, \
                 declaration_version, COUNT(*) FROM of_attempt WHERE at < ?1 \
                 GROUP BY 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11 \
                 ON CONFLICT (day, type, object_id, verdict, transition, clause, remedy, actor_kind, enforced, \
                 flagged, declaration_version) DO UPDATE SET count = count + excluded.count",
                params![before],
            )?;
            tx.execute("DELETE FROM of_attempt WHERE at < ?1", params![before])?
        } else {
            tx.execute(
                "DELETE FROM of_idempotency WHERE applied_at < ?1",
                params![before],
            )?
        };
        tx.commit()?;
        Ok(Ok(Ok(i64::try_from(moved).unwrap_or(i64::MAX))))
    }

    /// A refused request's row of the attempt log, in a short transaction of
    /// its own after the request's rolled back (`DESIGN.md` §6,
    /// `storage-schema.md` §6).
    pub fn record_attempt(&mut self, a: &Attempt) -> Stored<()> {
        let (tx, _) = begin_write!(self.conn);
        release_held(&tx)?;
        next(&tx, "txn")?;
        insert_attempt(&tx, a, "(SELECT value FROM of_counter WHERE name = 'txn')")?;
        tx.commit()?;
        Ok(())
    }

    /// The store's connection, for a test reading a table no operation
    /// returns.
    #[cfg(test)]
    pub(crate) fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Every object, as a request reads them.
    pub fn snapshot(&self) -> Stored<World<'_>> {
        let mut world = snapshot(&self.conn, &self.lib, &self.earlier, self.now)?;
        world.deadline = self.deadline;
        Ok(world)
    }

    /// Every earlier version's library loaded, those already loaded kept:
    /// a request's world reads them where an erasure decides what was
    /// personal (ADR-0146 decision 2). Version 0 holds the built-ins, which
    /// every version holds as well.
    pub fn load_earlier(&mut self) -> Stored<()> {
        for v in 1..self.lib.version {
            if self.earlier.contains_key(&v) {
                continue;
            }
            if let Some(lib) = self.library_at(v)? {
                Arc::make_mut(&mut self.earlier).insert(v, lib);
            }
        }
        Ok(())
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
        // a savepoint, so it runs as well inside a read's transaction,
        // where a read connection holds one (ADR-0178), and is undone
        let sp = self.conn.savepoint()?;
        let world = snapshot(&sp, &self.lib, &self.earlier, self.now)?;
        let mut alloc = Allocator::read(&sp)?;
        let out = decide(&world, &mut alloc);
        drop(world);
        sp.finish()?;
        Ok(out)
    }

    /// A subscription's progress moved to a cursor, back as well as forward:
    /// runtime state, written beside the log and never in it, so it is not
    /// versioned and emits no event (`DESIGN.md` §7). A subscription the
    /// store does not hold is refused as `unknown` and a revoked one as
    /// `revoked`, each a value (`library-api.md` §6).
    pub fn acknowledge(&mut self, subscription: &str, cursor: i64) -> Stored<Option<String>> {
        let refusal = {
            let world = self.snapshot()?;
            match world.get(subscription)? {
                Some(s) if s.type_ == crate::model::SUBSCRIPTION && s.state == "REVOKED" => {
                    Some("revoked".to_string())
                }
                // a cursor of the log's, from 0, which replays it from the
                // start, to its last position, where any integer was taken
                // and one past the log skipped every later event (fourth
                // production review A-12)
                Some(s) if s.type_ == crate::model::SUBSCRIPTION => (cursor < 0
                    || cursor > world.last_position()?)
                .then(|| "invalid_cursor".to_string()),
                _ => Some("unknown".to_string()),
            }
        };
        if refusal.is_some() {
            return Ok(refusal);
        }
        self.conn.execute(
            "INSERT INTO of_subscription_position (subscription_id, acknowledged, acknowledged_at) \
             VALUES (?1, ?2, ?3) ON CONFLICT (subscription_id) DO UPDATE SET \
             acknowledged = excluded.acknowledged, acknowledged_at = excluded.acknowledged_at",
            params![subscription, cursor.to_string(), timestamp_text(self.now)],
        )?;
        Ok(None)
    }

    pub fn transact<T, E>(
        &mut self,
        decide: impl FnOnce(&World<'_>, &mut Allocator) -> Result<(T, Option<Change>), E>,
    ) -> Result<Result<T, E>, StoreError> {
        // the library of the version the store is at, inside the write's
        // transaction, which another process may have published since this
        // one read it (ADR-0178 decision 3)
        let mut reloaded = false;
        let (tx, retries) = loop {
            let (tx, retries) = begin_write!(self.conn);
            let at = declared_version(&tx)?;
            if at == self.lib.version || reloaded {
                break (tx, retries);
            }
            tx.rollback()?;
            self.lib = Arc::new(
                self.library_at(at)?
                    .ok_or_else(|| StoreError(format!("version {at} recorded no source")))?,
            );
            reloaded = true;
        };
        let world = snapshot(&tx, &self.lib, &self.earlier, self.now)?;
        let mut alloc = Allocator::read(&tx)?;
        let decided = decide(&world, &mut alloc);
        drop(world);
        match decided {
            Ok((answer, Some(mut change))) => {
                change.retries = retries;
                alloc.write(&tx)?;
                alloc.write_sequences(&tx)?;
                write(&tx, &self.lib, &self.earlier, &change)?;
                tx.commit()?;
                Ok(Ok(answer))
            }
            Ok((answer, None)) => {
                // a refused request writes nothing but the numbers it drew,
                // which are not reused (flow-format.md §4.12)
                tx.rollback()?;
                if !alloc.drawn.is_empty() {
                    let (tx, _) = begin_write!(self.conn);
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
        let who = match crate::request::active(resolve_actor(&world, actor)?) {
            Ok(who) => who,
            Err(v) => return refusal(v),
        };
        let Some(c) = world.get(change)?.filter(|c| c.type_ == DECLARATION_CHANGE) else {
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
        // the approval taken only by the kinds the installed version
        // declares, read from it and never from the version under approval,
        // which could drop its own rule; the store's operator, answerable for
        // it, always may, so no declaration shuts every approver out
        // (ADR-0187 decision 1). Refused before the dry run is computed again.
        if let Some(kinds) = &self.lib.publishing
            && !kinds.contains(&who.kind)
            && !is_the_operator(&world, actor)?
        {
            return refusal(Verdict::InvariantViolated {
                failures: vec![crate::verdict::InvariantFailure {
                    object_id: Some(c.id.clone()),
                    type_: DECLARATION_CHANGE.to_string(),
                    invariant: "publish_taken_by".to_string(),
                    remedy: Some(Remedy::Delegable),
                }],
                remedy: Some(Remedy::Delegable),
            });
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
        // the change's own record, which its publish event extends
        world.parts(c)?;
        let change_obj = c.clone();
        // every object whole, the identifiers each held before the version
        // read from its record, the actor's among them: the objects the
        // simulation read, taken out of it, which reading the store whole a
        // second time and copying it made an approval's largest step in
        // memory (MEMORY)
        drop(world);
        {
            let all = at.in_order()?;
            at.with_parts(&all)?;
        }
        let old_objects: BTreeMap<String, Object> = at
            .into_objects()
            .into_iter()
            .map(|o| (o.id.clone(), o))
            .collect();
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
        self.lib = Arc::new(new);
        Ok(Ok((
            Verdict::Satisfied {
                object_id: change.to_string(),
                version,
                events,
                flags: Vec::new(),
                replayed: false,
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
        let (tx, _) = begin_write!(self.conn);
        release_held(&tx)?;
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
                        // and its write index, so `changed_since` and an
                        // erasure read who last wrote it under its new name
                        tx.execute(
                            &format!(
                                "UPDATE of_attribute_write SET attribute = ?2 WHERE attribute = ?1 \
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
        )?;
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
        // the standing admissions the version cures, each discharged on its
        // object's migration (D531)
        let discharged_of = |id: &str| -> Vec<String> {
            affected
                .iter()
                .filter(|(code, ids)| {
                    code.starts_with("discharged:") && ids.iter().any(|i| i == id)
                })
                .filter_map(|(code, _)| code.rsplit_once('.').map(|(_, n)| n.to_string()))
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
            // the external identifiers the version marks, which `lookup`
            // finds the object by: each it holds now, one the version newly
            // marks or a mapping wrote among them, and none it no longer
            // marks or holds, nor a personal one of an erased object, which
            // is absent or redacted below (DESIGN.md §10, D533)
            let identifiers =
                |ty: Option<&Type>, obj: &Object| -> Stored<Vec<(String, String, String)>> {
                    let erased = obj.parts.get().map(|p| p.erased).ok_or_else(|| {
                        StoreError(format!(
                            "{}'s record, which the publish did not read",
                            obj.id
                        ))
                    })?;
                    Ok(ty
                        .map(|ty| {
                            ty.attributes
                                .iter()
                                .filter(|(_, spec)| !(erased && spec.personal))
                                .filter_map(|(a, spec)| {
                                    let source = spec.external.as_ref()?;
                                    let v = crate::request::external_text(&obj.attr(a))?;
                                    Some((source.clone(), v, a.clone()))
                                })
                                .collect()
                        })
                        .unwrap_or_default())
                };
            if let Some(was) = old_objects.get(&o.id) {
                let held = identifiers(Some(t), o)?;
                for (source, value, _) in identifiers(old.type_(&was.type_), was)? {
                    if !held.iter().any(|(s, v, _)| *s == source && *v == value) {
                        change_set
                            .external_removed
                            .push((source, value, o.id.clone()));
                    }
                }
                for (source, value, attribute) in held {
                    change_set
                        .external
                        .push((source, value, o.id.clone(), attribute));
                }
            }
            let codes = codes_of(&o.id);
            let admits = admitted_of(&o.id);
            let discharges = discharged_of(&o.id);
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
            let migrates = !codes.is_empty() || !admits.is_empty() || !discharges.is_empty();
            let position = if migrates {
                alloc.position()
            } else {
                publish_position
            };
            let mut extra = Vec::new();
            for (d, spans) in &mut x.parts_mut()?.spans {
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
            let spans_now = &x.parts_mut()?.spans;
            let before: Vec<(String, Value)> = tracked_now
                .iter()
                .filter_map(|d| {
                    spans_now
                        .get(d)
                        .and_then(|s| s.last())
                        .filter(|sp| sp.left_position.is_none())
                        .map(|sp| (d.clone(), sp.value.clone()))
                })
                .collect();
            for d in &tracked_now {
                if x.parts_mut()?.spans.get(d).is_none_or(Vec::is_empty) {
                    let at = if migrates && backfilled(d) {
                        position
                    } else {
                        publish_position
                    };
                    let sp = opened(&x, d, at, now, who, new.version);
                    x.parts_mut()?
                        .spans
                        .entry(d.clone())
                        .or_default()
                        .push(sp.clone());
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
                    x.parts_mut()?.admissions.insert(n.clone());
                    change_set
                        .admitted
                        .push((x.id.clone(), n.clone(), position, reason.clone()));
                }
                for n in &discharges {
                    x.parts_mut()?.admissions.remove(n);
                    change_set
                        .discharged
                        .push((x.id.clone(), n.clone(), position));
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
                    serde_json::json!({
                        "mappings": codes,
                        "admits": admits.iter().map(|(n, _)| n).collect::<Vec<_>>(),
                        "discharges": discharges,
                    }),
                )?;
                // a migration is caused by the publish's event (DESIGN.md §5.9)
                let event = Event {
                    cause: Some(publish_position),
                    ..event
                };
                change_set.writes.push((x.clone(), false, event));
            }
            // an attribute the version newly marks personal, redacted on an
            // object already erased, by a redact event (publish-and-import.md
            // §1 step 5), which admits what the absence breaks, as the
            // erasure it completes does (declaration-syntax.md §6.4)
            let erased = x.parts_mut()?.erased;
            let newly: Vec<String> = t
                .attributes
                .iter()
                .filter(|(n, a)| {
                    erased
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
                    for sp in x.parts_mut()?.spans.get_mut(n).into_iter().flatten() {
                        sp.value = Value::Absent;
                    }
                }
                let mut after = World::new(new, now);
                for o in objects.values() {
                    after.insert(o.clone())?;
                }
                after.insert(x.clone())?;
                for n in failing_invariants(&after, &x).map_err(StoreError::from)? {
                    if !x.parts_mut()?.admissions.contains(&n)
                        && invariant_reads(t, &n).iter().any(|r| newly.contains(r))
                    {
                        x.parts_mut()?.admissions.insert(n.clone());
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
                )?;
                // a migration is caused by the publish's event (DESIGN.md §5.9)
                let event = Event {
                    cause: Some(publish_position),
                    ..event
                };
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
                )?;
                // a migration is caused by the publish's event (DESIGN.md §5.9)
                let event = Event {
                    cause: Some(publish_position),
                    ..event
                };
                change_set.writes.push((x, false, event));
            }
        }
        alloc.write(&tx)?;
        write(&tx, new, &self.earlier, &change_set)?;
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

    /// The library a version installed, loaded again from the source it
    /// recorded (`library-api.md` §6, `declaration` at an earlier version):
    /// none for a version the store never published; a source that no
    /// longer loads is the store's failure, since it loaded when published.
    /// A version's source reference and digest, as its publish, or a
    /// sandbox's load, recorded them; none for a version the store never
    /// held (ADR-0135).
    pub fn declaration_source(
        &self,
        version: i64,
    ) -> Stored<Option<(Option<String>, Option<String>)>> {
        self.conn
            .query_row(
                "SELECT source_ref, source_digest FROM of_declaration WHERE version = ?1 LIMIT 1",
                params![version],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .map_err(Into::into)
    }

    pub fn library_at(&self, version: i64) -> Stored<Option<Library>> {
        let source: Option<String> = self
            .conn
            .query_row(
                "SELECT source FROM of_declaration WHERE version = ?1",
                params![version],
                |r| r.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        let Some(source) = source else {
            return Ok(None);
        };
        let files: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&source)
            .map_err(|e| StoreError(format!("version {version}'s source: {e}")))?;
        let files: Vec<(String, String)> = files
            .into_iter()
            .map(|(n, t)| (n, t.as_str().unwrap_or_default().to_string()))
            .collect();
        let mut lib = Library::load(&files)
            .map_err(|e| StoreError(format!("version {version}'s source no longer loads: {e}")))?;
        lib.version = version;
        Ok(Some(lib))
    }

    /// The store's connection, for a test to read its schema.
    #[cfg(test)]
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    /// An object's events, in its order.
    /// An object's legacy entries, by ordinal, which `history` returns
    /// before its events (`library-api.md` §6, ADR-0015).
    pub fn legacy_entries(&self, id: &str) -> Stored<Vec<crate::world::LegacyEntry>> {
        read_legacy(&self.conn, id)
    }

    pub fn history(&self, id: &str) -> Stored<Vec<Event>> {
        let mut stmt = self.conn.prepare(&format!(
            "{EVENT_SELECT} WHERE object_id = ?1 ORDER BY object_seq"
        ))?;
        let rows = stmt.query_map(params![id], event_row)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// A page of the events of objects, after a position, in position
    /// order, at most `limit` of them, read by the object's index, never the
    /// object's history whole, which a `history` of 10,000 events of 16 KB
    /// each answered at 164.6 MB (sixth production review HISTORY, ADR-0192
    /// decision 5).
    pub fn history_page(&self, ids: &[String], after: i64, limit: usize) -> Stored<Vec<Event>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let marks: Vec<String> = (0..ids.len()).map(|i| format!("?{}", i + 3)).collect();
        let mut stmt = self.conn.prepare(&format!(
            "{EVENT_SELECT} WHERE object_id IN ({}) AND position > ?1 ORDER BY position LIMIT ?2",
            marks.join(", ")
        ))?;
        let mut values: Vec<rusqlite::types::Value> = vec![
            rusqlite::types::Value::Integer(after),
            rusqlite::types::Value::Integer(i64::try_from(limit).unwrap_or(i64::MAX)),
        ];
        values.extend(
            ids.iter()
                .map(|id| rusqlite::types::Value::Text(id.clone())),
        );
        let rows = stmt.query_map(rusqlite::params_from_iter(values.iter()), event_row)?;
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
    fn read(tx: &Connection) -> Stored<Allocator> {
        let get = |n: &str| -> Stored<i64> {
            Ok(tx.query_row(
                "SELECT value FROM of_counter WHERE name = ?1",
                params![n],
                |r| r.get(0),
            )?)
        };
        let mut sequences = BTreeMap::new();
        // each sequence's last number drawn, the one before its next
        // read whole and counted here, never in SQL, where a next value an
        // earlier release wrapped overflowed to a REAL and failed every
        // write; such a sequence reads as exhausted, so only a request
        // minting from it is refused (fourth production review A-15)
        let mut stmt = tx.prepare("SELECT name, scope_key, next_value FROM of_sequence")?;
        let rows = stmt.query_map([], |r| Ok(((r.get(0)?, r.get(1)?), r.get(2)?)))?;
        for row in rows {
            let (key, next): ((String, String), i64) = row?;
            let last = next
                .checked_sub(1)
                .filter(|l| *l >= 0)
                .unwrap_or(SEQUENCE_MOST);
            sequences.insert(key, last);
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

    /// The next number of a sequence in a scope, numbered from 1; none once
    /// it has drawn [`SEQUENCE_MOST`], when it is exhausted.
    pub fn draw(&mut self, sequence: &str, scope: &str) -> Option<i64> {
        let key = (sequence.to_string(), scope.to_string());
        let n = self
            .sequences
            .get(&key)
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .filter(|n| *n <= SEQUENCE_MOST)?;
        self.sequences.insert(key.clone(), n);
        self.drawn.insert(key);
        Some(n)
    }

    /// The numbers drawn, written whatever became of the request.
    fn write_sequences(&self, tx: &Transaction<'_>) -> Stored<()> {
        for key in &self.drawn {
            let value = self.sequences.get(key).copied().unwrap_or(0);
            tx.execute(
                "INSERT INTO of_sequence (name, scope_key, next_value) VALUES (?1, ?2, ?3) \
                 ON CONFLICT (name, scope_key) DO UPDATE SET next_value = excluded.next_value",
                params![key.0, key.1, value + 1],
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

/// A counter's next value, for the upgrade's record (`crate::release`).
pub(crate) fn next_counter(tx: &Connection, name: &str) -> Stored<i64> {
    next(tx, name)
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
    source, actor_id, actor_kind, declaration_version, recorded_at, occurred_at, imported, payload, actor_principal, \
    cause_position, reads, context, retries FROM of_event";

fn event_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    let position: i64 = r.get(0)?;
    let recorded: String = r.get(11)?;
    let occurred: Option<String> = r.get(12)?;
    let payload: String = r.get(14)?;
    let what = |column: &str| format!("event {position}'s {column}");
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
        cause: r.get(16)?,
        declaration_version: r.get(10)?,
        recorded_at: time_column(11, &recorded, &what("recorded_at"))?,
        occurred_at: occurred
            .as_deref()
            .map(|t| time_column(12, t, &what("occurred_at")))
            .transpose()?,
        imported: r.get::<_, i64>(13)? != 0,
        payload: serde_json::from_str(&payload).map_err(|_| undecodable(14, &what("payload")))?,
        reads: json_column(r, 17, &what("reads"))?.as_object().map_or_else(
            || serde_json::json!({}),
            |o| serde_json::Value::Object(o.clone()),
        ),
        context: r.get(18)?,
        retries: r.get(19)?,
    })
}

fn snapshot<'l>(
    conn: &'l Connection,
    lib: &'l Library,
    earlier: &'l BTreeMap<i64, Library>,
    now: i64,
) -> Stored<World<'l>> {
    release_held(conn)?;
    // the objects, and the log, are read where an operation reads them
    // (ADR-0162 steps 1 and 2)
    let mut world = World::new(lib, now).reading_from(conn);
    world.earlier = earlier;
    // each subscription's acknowledged cursor, a position on SQLite
    let mut positions =
        conn.prepare("SELECT subscription_id, acknowledged FROM of_subscription_position")?;
    let rows = positions.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (id, cursor) = row?;
        let Ok(at) = cursor.parse::<i64>() else {
            return Err(StoreError(format!("{id}'s acknowledged cursor {cursor}")));
        };
        world.acknowledged.insert(id, at);
    }
    // the events that recorded an admission; the log, the imported events,
    // the external identifiers and the legacy entries are read where an
    // operation reads them (ADR-0162 step 1)
    let mut admitting = conn.prepare("SELECT DISTINCT position FROM of_admission")?;
    let rows = admitting.query_map([], |r| r.get::<_, i64>(0))?;
    for row in rows {
        world.admitting.insert(row?);
    }
    Ok(world)
}

/// Every external identifier the store holds, by source and value, and
/// the object that holds it.
pub(crate) fn read_external_all(conn: &Connection) -> Stored<BTreeMap<(String, String), String>> {
    let mut ids = conn.prepare("SELECT source, value, object_id FROM of_external_id")?;
    let rows = ids.query_map([], |r| {
        Ok((
            (r.get::<_, String>(0)?, r.get::<_, String>(1)?),
            r.get::<_, String>(2)?,
        ))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The object an external identifier names, by its source and value.
pub(crate) fn read_external(
    conn: &Connection,
    source: &str,
    value: &str,
) -> Stored<Option<String>> {
    let mut stmt = conn
        .prepare_cached("SELECT object_id FROM of_external_id WHERE source = ?1 AND value = ?2")?;
    let mut rows = stmt.query_map(params![source, value], |r| r.get::<_, String>(0))?;
    Ok(rows.next().transpose()?)
}

/// An object's legacy entries, by ordinal, a damaged one refused.
pub(crate) fn read_legacy(conn: &Connection, id: &str) -> Stored<Vec<crate::world::LegacyEntry>> {
    let mut entries = conn.prepare_cached(
        "SELECT ordinal, kind, occurred_at, payload, kept FROM of_legacy_entry WHERE object_id = ?1 ORDER BY ordinal",
    )?;
    let rows = entries.query_map(params![id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (ordinal, kind, at, payload, kept) = row?;
        let damaged = |column: &str| {
            StoreError(format!(
                "{id}'s legacy entry {ordinal}'s {column}, which does not decode"
            ))
        };
        out.push(crate::world::LegacyEntry {
            ordinal,
            kind,
            occurred_at: parse_timestamp(&at).ok_or_else(|| damaged("occurred_at"))?,
            payload: serde_json::from_str(&payload).map_err(|_| damaged("payload"))?,
            kept: serde_json::from_str(&kept).map_err(|_| damaged("kept"))?,
        });
    }
    Ok(out)
}

/// The position of each event the import recorded.
pub(crate) fn read_imported(conn: &Connection) -> Stored<std::collections::BTreeSet<i64>> {
    let mut imported = conn.prepare("SELECT position FROM of_event WHERE imported = 1")?;
    let rows = imported.query_map([], |r| r.get::<_, i64>(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// An object the store holds, with every part of its record; none where
/// the directory holds no such id (ADR-0162 step 2).
pub(crate) fn read_object(conn: &Connection, lib: &Library, id: &str) -> Stored<Option<Object>> {
    let mut directory = conn.prepare_cached("SELECT type FROM of_object WHERE id = ?1")?;
    let mut types = directory.query_map(params![id], |r| r.get::<_, String>(0))?;
    let Some(type_) = types.next().transpose()? else {
        return Ok(None);
    };
    let mut found = read_objects(conn, lib, &type_, Scope::One(id))?;
    match (found.pop(), found.is_empty()) {
        (Some(o), true) => Ok(Some(o)),
        _ => Err(StoreError(format!(
            "the directory names {id}, which {} does not hold once",
            table(&type_)
        ))),
    }
}

/// Every object of a concrete type the store holds, in creation order, in
/// one read of its table and one of each other part of their records.
pub(crate) fn read_type(conn: &Connection, lib: &Library, type_: &str) -> Stored<Vec<Object>> {
    read_objects(conn, lib, type_, Scope::Type(table(type_)))
}

/// The objects of a concrete type the store holds in one of the states
/// given, where any are, and whose columns named each hold the value given
/// or none, in creation order, read through the type's state index and its
/// columns (ADR-0162 step 3); none where a value has no column to compare
/// in, a set's or an absent one.
pub(crate) fn read_type_where(
    conn: &Connection,
    lib: &Library,
    type_: &str,
    states: Option<&[String]>,
    columns: &[(&str, &Value)],
) -> Stored<Option<Vec<Object>>> {
    if states.is_some_and(<[String]>::is_empty) {
        return Ok(Some(Vec::new()));
    }
    let Some(narrowed) = narrowing(conn, lib, type_, states, columns)? else {
        return Ok(None);
    };
    Ok(Some(read_objects(
        conn,
        lib,
        type_,
        Scope::Where(table(type_), narrowed),
    )?))
}

/// The objects of a concrete type after a place in creation order, the
/// first so many of those in the states and holding the values given, each
/// as [`read_type_where`] reads it: the next of a read in order from a
/// cursor, never the type whole, which each page of a query read again
/// (READ-PAGES); none where a value has no column to compare in.
pub(crate) fn read_type_from(
    conn: &Connection,
    lib: &Library,
    type_: &str,
    states: Option<&[String]>,
    columns: &[(&str, &Value)],
    after: i64,
    first: usize,
) -> Stored<Option<Vec<Object>>> {
    if states.is_some_and(<[String]>::is_empty) {
        return Ok(Some(Vec::new()));
    }
    let Some(mut narrowed) = narrowing(conn, lib, type_, states, columns)? else {
        return Ok(None);
    };
    narrowed.bound = Some((table(type_), after, first));
    Ok(Some(read_objects(
        conn,
        lib,
        type_,
        Scope::Where(table(type_), narrowed),
    )?))
}

/// What [`read_type_where`] narrows a type's rows to, or none where a value
/// has no column to compare in; a column the table holds no null in is
/// compared alone, which its index serves (`storage-schema.md` §5).
fn narrowing(
    conn: &Connection,
    lib: &Library,
    type_: &str,
    states: Option<&[String]>,
    columns: &[(&str, &Value)],
) -> Stored<Option<Narrowed>> {
    let Some(t) = lib.type_(type_) else {
        return Err(StoreError(format!(
            "the directory names a type {type_} the library lacks"
        )));
    };
    let required = if columns.is_empty() {
        BTreeSet::new()
    } else {
        not_null(conn, &table(type_))?
    };
    let mut narrowed = Narrowed {
        states: states.map(<[String]>::to_vec),
        columns: Vec::new(),
        bound: None,
    };
    for (n, v) in columns {
        let Some(a) = t
            .attribute(n)
            .filter(|a| !a.many && comparable(v, &a.declared))
        else {
            return Ok(None);
        };
        let col = column(n, a);
        match to_sql(v) {
            Ok(Sql::Null) | Err(_) => return Ok(None),
            Ok(sql) => {
                let nullable = !required.contains(&col);
                narrowed.columns.push((quote(&col), sql, nullable));
            }
        }
    }
    Ok(Some(narrowed))
}

/// The columns of a table that hold no null, as the table declares them:
/// a required attribute's, unless the table was built relaxed.
fn not_null(conn: &Connection, name: &str) -> Stored<BTreeSet<String>> {
    let mut info = conn.prepare(&format!("PRAGMA table_info({})", quote(name)))?;
    let rows = info.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(3)?)))?;
    let mut out = BTreeSet::new();
    for row in rows {
        let (col, notnull) = row?;
        if notnull != 0 {
            out.insert(col);
        }
    }
    Ok(out)
}

/// What a read of a type's objects narrows them to: a state among those
/// given, where any are, and each column holding its value or none, since an
/// absent value compares as unknown and not as unequal; a column marked as
/// holding no null is compared alone.
struct Narrowed {
    states: Option<Vec<String>>,
    columns: Vec<(String, Sql, bool)>,
    /// Where it reads from a place in creation order: its table, the place,
    /// and how many rows at most (READ-PAGES).
    bound: Option<(String, i64, usize)>,
}

impl Narrowed {
    /// The conditions on a row of the type's table, its columns prefixed as
    /// given, and the parameters they bind: where it reads from a place, the
    /// rows after it in creation order that meet the rest, the first so
    /// many, which a read of each object's sets narrows to alike.
    fn conditions(&self, prefix: &str) -> (String, Vec<Sql>) {
        let Some((table, after, first)) = &self.bound else {
            return self.unbounded(prefix);
        };
        let (inner, mut bound) = self.unbounded("t2.");
        bound.push(Sql::Integer(*after));
        let place = bound.len();
        bound.push(Sql::Integer(i64::try_from(*first).unwrap_or(i64::MAX)));
        let most = bound.len();
        let meets = if inner.is_empty() {
            String::new()
        } else {
            format!("{inner} AND ")
        };
        (
            format!(
                "{prefix}id IN (SELECT t2.id FROM {table} t2 JOIN of_object o2 ON o2.id = t2.id WHERE {meets}o2.rowid > ?{place} ORDER BY o2.rowid LIMIT ?{most})"
            ),
            bound,
        )
    }

    /// The conditions on its states and columns alone.
    fn unbounded(&self, prefix: &str) -> (String, Vec<Sql>) {
        let mut parts = Vec::new();
        let mut bound = Vec::new();
        if let Some(states) = &self.states {
            let marks: Vec<String> = states
                .iter()
                .map(|st| {
                    bound.push(Sql::Text(st.clone()));
                    format!("?{}", bound.len())
                })
                .collect();
            parts.push(format!("{prefix}state IN ({})", marks.join(", ")));
        }
        for (c, v, nullable) in &self.columns {
            bound.push(v.clone());
            let n = bound.len();
            parts.push(if *nullable {
                format!("({prefix}{c} = ?{n} OR {prefix}{c} IS NULL)")
            } else {
                format!("{prefix}{c} = ?{n}")
            });
        }
        (parts.join(" AND "), bound)
    }
}

/// Every object the store holds, in creation order: each type's rows in one
/// read of its table, and each other part of their records in one read of
/// its table, as a load of the whole store is cheapest. A type the directory
/// names that the library lacks is refused, never passed over.
pub(crate) fn read_all(conn: &Connection, lib: &Library) -> Stored<Vec<Object>> {
    let mut directory = conn.prepare("SELECT DISTINCT type FROM of_object")?;
    let types = directory
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<String>, _>>()?;
    let mut objects = BTreeMap::new();
    for type_ in &types {
        objects.append(&mut read_rows(conn, lib, type_, &Scope::All)?);
    }
    let of: BTreeMap<String, String> = objects
        .iter()
        .map(|(id, o)| (id.clone(), o.type_.clone()))
        .collect();
    let mut parts = read_parts(conn, lib, &Scope::All, &of)?;
    for (id, o) in &mut objects {
        o.parts = parts.remove(id).unwrap_or_default().held();
    }
    let mut all: Vec<Object> = objects.into_values().collect();
    all.sort_by_key(|o| o.order);
    Ok(all)
}

/// Which objects a read of the store fetches: one by id, every one a type's
/// table holds, those of a type's table it narrows to, or every one.
enum Scope<'a> {
    One(&'a str),
    Type(String),
    Where(String, Narrowed),
    All,
}

impl Scope<'_> {
    /// The condition on a type's row: its id, or its state among those given.
    fn on_row(&self) -> String {
        match self {
            Scope::One(_) => "WHERE t.id = ?1".into(),
            Scope::Where(_, narrowed) => format!("WHERE {}", narrowed.conditions("t.").0),
            Scope::Type(_) | Scope::All => String::new(),
        }
    }

    /// The condition a read of a part of the records takes on its
    /// `object_id`, joined to what precedes it by the keyword given; none
    /// for every object.
    fn on_object(&self, keyword: &str) -> String {
        match self {
            Scope::One(_) => format!(" {keyword} object_id = ?1"),
            Scope::Type(table) => format!(" {keyword} object_id IN (SELECT id FROM {table})"),
            Scope::Where(table, narrowed) => format!(
                " {keyword} object_id IN (SELECT id FROM {table} WHERE {})",
                narrowed.conditions("").0
            ),
            Scope::All => String::new(),
        }
    }

    /// What a read binds: the one id, or what it narrows by.
    fn bound(&self) -> Vec<Sql> {
        match self {
            Scope::One(id) => vec![Sql::Text((*id).to_string())],
            Scope::Where(_, narrowed) => narrowed.conditions("").1,
            Scope::Type(_) | Scope::All => Vec::new(),
        }
    }
}

/// The ids of the objects of a concrete type whose columns hold the values
/// given, compared as each column encodes its declared type, so equal as
/// the expression language's equality would find them; none where a value
/// has no column to compare in, a set's or an absent one (ADR-0162 step 3).
/// The ids of a type's objects superseded by an object, by the index on
/// their successor's column (ADR-0191 decision 5).
pub(crate) fn read_superseded_into(
    conn: &Connection,
    type_: &str,
    id: &str,
) -> Stored<Vec<String>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT id FROM {} WHERE superseded_by = ?1",
        table(type_)
    ))?;
    let ids = stmt
        .query_map([id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

/// The ids of a type's objects whose text attribute, its ASCII letters
/// folded to lower case, is the text given folded: by the index on the
/// folded column an actor's identity has (ADR-0194 decision 1).
pub(crate) fn read_ids_folded(
    conn: &Connection,
    lib: &Library,
    type_: &str,
    attr: &str,
    text: &str,
) -> Stored<Vec<String>> {
    let Some(a) = lib
        .type_(type_)
        .and_then(|t| t.attribute(attr))
        .filter(|a| !a.many && !a.derived_end())
    else {
        return Ok(Vec::new());
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT id FROM {} WHERE lower({}) = ?1",
        table(type_),
        quote(&column(attr, a))
    ))?;
    let ids = stmt
        .query_map([text.to_ascii_lowercase()], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

pub(crate) fn read_ids_with(
    conn: &Connection,
    lib: &Library,
    type_: &str,
    key: &[(&str, &Value)],
) -> Stored<Option<Vec<String>>> {
    let Some(t) = lib.type_(type_) else {
        return Err(StoreError(format!(
            "the directory names a type {type_} the library lacks"
        )));
    };
    let mut conditions = Vec::new();
    let mut values = Vec::new();
    for (n, v) in key {
        let Some(a) = t
            .attribute(n)
            .filter(|a| !a.many && comparable(v, &a.declared))
        else {
            return Ok(None);
        };
        let sql = match to_sql(v) {
            Ok(Sql::Null) | Err(_) => return Ok(None),
            Ok(sql) => sql,
        };
        values.push(sql);
        conditions.push(format!("{} = ?{}", quote(&column(n, a)), values.len()));
    }
    if conditions.is_empty() {
        return Ok(None);
    }
    let mut stmt = conn.prepare(&format!(
        "SELECT id FROM {} WHERE {}",
        table(type_),
        conditions.join(" AND ")
    ))?;
    let ids = stmt
        .query_map(rusqlite::params_from_iter(values.iter()), |r| {
            r.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(ids))
}

/// The ids of the objects of a concrete type that one of it corrects, read
/// by the index on its stored `corrects` end; none for a type without one.
pub(crate) fn read_corrected(
    conn: &Connection,
    lib: &Library,
    type_: &str,
) -> Stored<BTreeSet<String>> {
    let Some(t) = lib.type_(type_) else {
        return Err(StoreError(format!(
            "the directory names a type {type_} the library lacks"
        )));
    };
    let mut out = BTreeSet::new();
    let Some(a) = t
        .attribute("corrects")
        .filter(|a| !a.many && matches!(a.declared, Declared::Reference(_)))
    else {
        return Ok(out);
    };
    let col = quote(&column("corrects", a));
    let mut q = conn.prepare(&format!(
        "SELECT {col} FROM {} WHERE {col} IS NOT NULL",
        table(type_)
    ))?;
    for id in q.query_map([], |r| r.get::<_, String>(0))? {
        out.insert(id?);
    }
    Ok(out)
}

/// What `exceptions(type)` lists of the concrete types given, as the store
/// holds it: the ids of the objects whose state an assertion last set, read
/// by the asserted-state index, and of those holding a standing admission,
/// by the admissions (`storage-schema.md` §3.2, §5).
pub(crate) fn read_excepted(
    conn: &Connection,
    types: &[&str],
) -> Stored<(BTreeSet<String>, BTreeSet<String>)> {
    let mut asserted = BTreeSet::new();
    for t in types {
        let mut q = conn.prepare(&format!(
            "SELECT id FROM {} WHERE state_source = 'asserted'",
            table(t)
        ))?;
        for id in q.query_map([], |r| r.get::<_, String>(0))? {
            asserted.insert(id?);
        }
    }
    let mut admitted = BTreeSet::new();
    if !types.is_empty() {
        let marks = vec!["?"; types.len()].join(", ");
        let mut q = conn.prepare(&format!(
            "SELECT DISTINCT a.object_id FROM of_admission a JOIN of_object o ON o.id = a.object_id \
             WHERE a.discharged_position IS NULL AND o.type IN ({marks})"
        ))?;
        for id in q.query_map(rusqlite::params_from_iter(types), |r| r.get::<_, String>(0))? {
            admitted.insert(id?);
        }
    }
    Ok((asserted, admitted))
}

/// The parts of one object's record beyond its row (ADR-0165).
pub(crate) fn read_parts_of(
    conn: &Connection,
    lib: &Library,
    id: &str,
    type_: &str,
) -> Stored<crate::world::Parts> {
    let types = BTreeMap::from([(id.to_string(), type_.to_string())]);
    Ok(read_parts(conn, lib, &Scope::One(id), &types)?
        .remove(id)
        .unwrap_or_default())
}

/// The parts of the records of every object of a concrete type, by id, in
/// one read of each part's table (ADR-0165).
pub(crate) fn read_type_parts(
    conn: &Connection,
    lib: &Library,
    type_: &str,
) -> Stored<BTreeMap<String, crate::world::Parts>> {
    let name = table(type_);
    let mut ids = conn.prepare(&format!("SELECT id FROM {name}"))?;
    let types: BTreeMap<String, String> = ids
        .query_map([], |r| r.get::<_, String>(0))?
        .map(|id| id.map(|id| (id, type_.to_string())))
        .collect::<Result<_, _>>()?;
    read_parts(conn, lib, &Scope::Type(name), &types)
}

/// The highest place in creation order the store holds, 0 where it holds
/// no object.
pub(crate) fn read_max_order(conn: &Connection) -> Stored<i64> {
    Ok(
        conn.query_row("SELECT COALESCE(MAX(rowid), 0) FROM of_object", [], |r| {
            r.get(0)
        })?,
    )
}

/// The objects of a concrete type in the scope given, in creation order,
/// each with its directory row, its type's row and sets, its flow data, its
/// write index, its standing admissions and whether its own erasure is
/// recorded.
fn read_objects(
    conn: &Connection,
    lib: &Library,
    type_: &str,
    scope: Scope<'_>,
) -> Stored<Vec<Object>> {
    let objects = read_rows(conn, lib, type_, &scope)?;
    let mut all: Vec<Object> = objects.into_values().collect();
    all.sort_by_key(|o| o.order);
    Ok(all)
}

/// The objects of a concrete type in the scope given, by id, each with its
/// directory row and its type's row and sets, the other parts of its record
/// empty.
fn read_rows(
    conn: &Connection,
    lib: &Library,
    type_: &str,
    scope: &Scope<'_>,
) -> Stored<BTreeMap<String, Object>> {
    let Some(t) = lib.type_(type_) else {
        return Err(StoreError(format!(
            "the directory names a type {type_} the library lacks"
        )));
    };
    let name = table(type_);
    let only = scope.bound();
    let of_row = scope.on_row();
    // a type's sets are read whole with its rows, an object's alone, and
    // those of a type's objects in some states by the same condition
    let of_set = match scope {
        Scope::One(_) | Scope::Where(..) => scope.on_object("WHERE"),
        Scope::Type(_) | Scope::All => String::new(),
    };
    let scalars: Vec<&(String, Attribute)> = t.attributes.iter().filter(|(_, a)| !a.many).collect();
    let superseding = t.states.iter().any(|(_, s)| s.superseding);
    let mut cols = vec![
        "o.rowid".to_string(),
        "t.id".into(),
        "o.created_at".into(),
        "o.created_by_kind".into(),
        "o.created_dated".into(),
        "o.recorded_from".into(),
        "t.state".into(),
        "t.version".into(),
        "t.state_source".into(),
    ];
    if superseding {
        cols.push("t.superseded_by".into());
    }
    let first = cols.len();
    cols.extend(
        scalars
            .iter()
            .map(|(n, a)| format!("t.{}", quote(&column(n, a)))),
    );
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM {name} t JOIN of_object o ON o.id = t.id {of_row}",
        cols.join(", ")
    ))?;
    let read = stmt.query_map(rusqlite::params_from_iter(&only), |r| {
        let mut values = Vec::new();
        for i in 0..scalars.len() {
            values.push(r.get::<_, Sql>(i + first)?);
        }
        Ok((
            (
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)? != 0,
                r.get::<_, String>(5)?,
            ),
            (
                r.get::<_, String>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, String>(8)?,
                if superseding {
                    r.get::<_, Option<String>>(9)?
                } else {
                    None
                },
                values,
            ) as RowFields,
        ))
    })?;
    let mut objects: BTreeMap<String, Object> = BTreeMap::new();
    for row in read {
        let ((order, id, created_at, kind, created_dated, from), fields) = row?;
        let (state, version, source, superseded_by, values) = fields;
        let at = |text: &str, what: &str| {
            parse_timestamp(text).ok_or_else(|| StoreError(format!("{id}'s {what} {text}")))
        };
        let created_at = at(&created_at, "creation time")?;
        let recorded_from = at(&from, "recorded_from")?;
        let mut attrs = BTreeMap::new();
        for ((n, a), v) in scalars.iter().zip(values) {
            let v = from_sql(v, &a.declared, lib)?;
            if !v.is_absent() {
                attrs.insert(n.clone(), v);
            }
        }
        for (n, _) in t.attributes.iter().filter(|(_, a)| a.stored_set()) {
            attrs.insert(n.clone(), Value::Set(Vec::new()));
        }
        objects.insert(
            id.clone(),
            Object {
                superseded_by,
                id,
                type_: type_.to_string(),
                state,
                version,
                attrs,
                created_at,
                created_by_kind: kind,
                order,
                // the rest of its record is read on first use (ADR-0165)
                parts: std::cell::OnceCell::new(),
                created_dated,
                recorded_from,
                state_source: Some(source),
            },
        );
    }
    // each set's items, in order
    for (n, a) in t.attributes.iter().filter(|(_, a)| a.stored_set()) {
        let mut set = conn.prepare(&format!(
            "SELECT object_id, value FROM {}{of_set} ORDER BY object_id, ordinal",
            quote(&format!("{name}__{n}"))
        ))?;
        let items = set.query_map(rusqlite::params_from_iter(&only), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Sql>(1)?))
        })?;
        for item in items {
            let (id, v) = item?;
            let v = from_sql(v, &a.declared, lib)?;
            if let Some(Value::Set(items)) = objects.get_mut(&id).and_then(|o| o.attrs.get_mut(n)) {
                items.push(v);
            }
        }
    }
    Ok(objects)
}

/// The parts of the records of the objects given, by id with their types,
/// beyond their rows, for the scope given: their flow data, write index,
/// standing admissions and whether their own erasure is recorded, an object
/// with none of a part holding it empty.
fn read_parts(
    conn: &Connection,
    lib: &Library,
    scope: &Scope<'_>,
    types: &BTreeMap<String, String>,
) -> Stored<BTreeMap<String, crate::world::Parts>> {
    let only = scope.bound();
    let mut parts: BTreeMap<String, crate::world::Parts> = types
        .keys()
        .map(|id| (id.clone(), crate::world::Parts::default()))
        .collect();
    spans(conn, lib, scope, types, &mut parts)?;
    let mut index = conn.prepare(&format!(
        "SELECT object_id, attribute, position FROM of_attribute_write{}",
        scope.on_object("WHERE")
    ))?;
    let rows = index.query_map(rusqlite::params_from_iter(&only), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (id, attribute, position) = row?;
        if let Some(p) = parts.get_mut(&id) {
            p.writes.insert(attribute, position);
        }
    }
    // an object whose own erasure is recorded
    let mut erasures = conn.prepare(&format!(
        "SELECT DISTINCT object_id FROM of_event WHERE source = 'erased'{}",
        scope.on_object("AND")
    ))?;
    let rows = erasures.query_map(rusqlite::params_from_iter(&only), |r| r.get::<_, String>(0))?;
    for row in rows {
        if let Some(p) = parts.get_mut(&row?) {
            p.erased = true;
        }
    }
    // the admissions that stand
    let mut standing = conn.prepare(&format!(
        "SELECT object_id, invariant FROM of_admission WHERE discharged_position IS NULL{}",
        scope.on_object("AND")
    ))?;
    let rows = standing.query_map(rusqlite::params_from_iter(&only), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (id, invariant) = row?;
        if let Some(p) = parts.get_mut(&id) {
            p.admissions.insert(invariant);
        }
    }
    Ok(parts)
}

/// The flow data of the objects given, from the interval index, in each
/// dimension's order.
fn spans(
    conn: &Connection,
    lib: &Library,
    scope: &Scope<'_>,
    types: &BTreeMap<String, String>,
    parts: &mut BTreeMap<String, crate::world::Parts>,
) -> Stored<()> {
    let only = scope.bound();
    let mut stmt = conn.prepare(&format!(
        "SELECT object_id, dimension, value, entered_position, left_position, entered_at, left_at, \
         entered_by, entered_by_kind, migrated, declaration_version, legacy, corrected_by FROM of_interval\
         {} ORDER BY object_id, dimension, entered_position, entered_at",
        scope.on_object("WHERE")
    ))?;
    let rows = stmt.query_map(rusqlite::params_from_iter(&only), |r| {
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
        let (Some(type_), Some(p)) = (types.get(&id), parts.get_mut(&id)) else {
            return Err(StoreError(format!(
                "an interval of {id}, which the directory lacks"
            )));
        };
        let value = if dimension == "state" {
            match value {
                Sql::Text(name) => Value::State {
                    type_: type_.clone(),
                    name,
                },
                other => return Err(StoreError(format!("{id}'s state interval holds {other:?}"))),
            }
        } else {
            // a dropped attribute's intervals, and a set-aside's, stay as
            // history, and nothing the version declares reads them (§10)
            let Some(a) = lib.type_(type_).and_then(|t| t.attribute(&dimension)) else {
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
        p.spans.entry(dimension).or_default().push(span);
    }
    Ok(())
}

/// The indexes on the actor's columns an erasure rewrites, which a store
/// opened without them is given (ADR-0192 decision 4).
const ACTOR_INDEXES: &str = "\
CREATE INDEX IF NOT EXISTS of_event_by_actor ON of_event (actor_id);
CREATE INDEX IF NOT EXISTS of_event_by_principal ON of_event (actor_principal) WHERE actor_principal IS NOT NULL;
CREATE INDEX IF NOT EXISTS of_attempt_by_principal ON of_attempt (actor_principal) WHERE actor_principal IS NOT NULL;
CREATE INDEX IF NOT EXISTS of_interval_by_entered ON of_interval (entered_by) WHERE entered_by IS NOT NULL;
";

fn write(
    tx: &Transaction<'_>,
    lib: &Library,
    earlier: &BTreeMap<i64, Library>,
    change: &Change,
) -> Stored<()> {
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
             actor_principal, cause_position, context, retries) \
             VALUES (?1, (SELECT value FROM of_counter WHERE name = 'txn'), ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, ?12, ?13, ?14, ?18, ?15, ?16, ?17, ?19, ?20)",
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
                event.cause,
                event.reads.to_string(),
                event.context,
                change.retries,
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
        // a written object's record was read before it was changed
        let parts = o
            .parts
            .get()
            .ok_or_else(|| StoreError(format!("{}'s record, written without being read", o.id)))?;
        for (dimension, spans) in &parts.spans {
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
    if let Some((actor, key, digest, first, verdict)) = &change.idempotency {
        let applied = change.writes.first().map_or(0, |(_, _, e)| e.recorded_at);
        tx.execute(
            "INSERT INTO of_idempotency (actor_id, key, request_digest, first_position, verdict, applied_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![actor, key, digest, first, verdict.to_string(), timestamp_text(applied)],
        )?;
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
    // an erased actor's identity, wherever the store keeps it, replaced by
    // its object's id, which names no person: the events' actor and
    // principal, the attempts', the intervals' entries, the idempotency
    // records and a proposal's or a change's recorded actors, this request's
    // own among them, each by its index (ADR-0192 decision 4)
    for (identity, id) in &change.renamed_actors {
        for sql in [
            "UPDATE of_event SET actor_id = ?2 WHERE actor_id = ?1",
            "UPDATE of_event SET actor_principal = ?2 WHERE actor_principal = ?1",
            "UPDATE of_attempt SET actor_id = ?2 WHERE actor_id = ?1",
            "UPDATE of_attempt SET actor_principal = ?2 WHERE actor_principal = ?1",
            "UPDATE of_interval SET entered_by = ?2 WHERE entered_by = ?1",
            "UPDATE OR REPLACE of_idempotency SET actor_id = ?2 WHERE actor_id = ?1",
        ] {
            tx.execute(sql, params![identity, id])?;
        }
        for (tn, column) in [
            (crate::model::PROPOSAL, "proposer"),
            (crate::model::DECLARATION_CHANGE, "drafted_by"),
            (crate::model::DECLARATION_CHANGE, "approved_by"),
        ] {
            if lib.type_(tn).is_some_and(|t| t.attribute(column).is_some()) {
                tx.execute(
                    &format!(
                        "UPDATE {} SET {} = ?2 WHERE {} = ?1",
                        table(tn),
                        quote(column),
                        quote(column)
                    ),
                    params![identity, id],
                )?;
            }
        }
    }
    // the legacy entries an import attaches (ADR-0163)
    for (id, e) in &change.legacy {
        tx.execute(
            "INSERT INTO of_legacy_entry (object_id, ordinal, kind, occurred_at, payload, kept) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                e.ordinal,
                e.kind,
                timestamp_text(e.occurred_at),
                serde_json::Value::Object(e.payload.clone()).to_string(),
                serde_json::json!(e.kept).to_string()
            ],
        )?;
    }
    // what a version kept for history, personal under the version it
    // belongs to, leaves an erased object too (ADR-0146 decision 2), and so
    // does every field of its legacy entries their kind does not keep (§9
    // step 6, ADR-0078)
    for (o, _, _) in &change.writes {
        if o.parts.get().is_some_and(|p| p.erased) {
            clear_kept(tx, lib, earlier, o)?;
            redact_legacy(tx, &o.id)?;
        }
    }
    for (position, name) in &change.redacted_inputs {
        let path = format!("$.inputs.\"{}\"", name.replace('"', ""));
        tx.execute(
            "UPDATE of_event SET payload = json_set(payload, ?2, json('null')) \
             WHERE position = ?1 AND json_type(payload, ?2) IS NOT NULL",
            params![position, path],
        )?;
        // the key of the request that sent it replays its verdict whatever
        // is sent under it, so its digest confirms no guess of what was
        // erased (fourth production review A-5)
        tx.execute(
            "UPDATE of_idempotency SET request_digest = ?2 WHERE first_position = \
             (SELECT MIN(position) FROM of_event WHERE txn = (SELECT txn FROM of_event WHERE position = ?1))",
            params![position, ERASED_DIGEST],
        )?;
    }
    for (id, dimension, entered) in &change.redacted_spans {
        tx.execute(
            "UPDATE of_interval SET value = NULL WHERE object_id = ?1 AND dimension = ?2 AND entered_position = ?3",
            params![id, dimension, entered],
        )?;
    }
    for (source, value, id) in &change.external_removed {
        tx.execute(
            "DELETE FROM of_external_id WHERE source = ?1 AND value = ?2 AND object_id = ?3",
            params![source, value, id],
        )?;
    }
    for (source, value, id, attribute) in &change.external {
        tx.execute(
            "INSERT INTO of_external_id (source, value, object_id, attribute) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT (source, value) DO UPDATE SET object_id = excluded.object_id, attribute = excluded.attribute",
            params![source, value, id, attribute],
        )?;
    }
    for (sequence, scope, value) in &change.raised {
        // raised past the legacy system's last value, never lowered
        // (storage-schema.md §6, ADR-0103); its next checked, which the
        // import bounds already (A-15)
        let next = value.checked_add(1).ok_or_else(|| {
            StoreError(format!(
                "the sequence {sequence} raised past its last number"
            ))
        })?;
        tx.execute(
            "INSERT INTO of_sequence (name, scope_key, next_value) VALUES (?1, ?2, ?3) \
             ON CONFLICT (name, scope_key) DO UPDATE SET next_value = max(next_value, excluded.next_value)",
            params![sequence, scope, next],
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

    /// A deployment's store cuts its write-ahead log back to its limit when
    /// a checkpoint empties it, where the log grew to 792 MB beside a 133 MB
    /// store and stayed (third production review N6).
    /// A store's file checked with the log beside it keeps both as they
    /// were, where closing a connection carried the log into the file and
    /// deleted it, as a plain connection still does (fourth production
    /// review A-16).
    #[test]
    fn a_checked_store_keeps_its_file_and_log_as_they_were() {
        let dir = std::env::temp_dir().join(format!("objectflow-checked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("store.sqlite");
        drop(Store::open(&path, 0).unwrap());
        // a write the log holds, which no close carries into the file
        {
            let w = Connection::open(&path).unwrap();
            w.set_db_config(
                rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
                true,
            )
            .unwrap();
            w.execute_batch("PRAGMA wal_autocheckpoint = 0; CREATE TABLE later (x); INSERT INTO later VALUES (1);")
                .unwrap();
        }
        let wal = dir.join("store.sqlite-wal");
        let file = std::fs::read(&path).unwrap();
        let log = std::fs::read(&wal).unwrap();
        assert!(!log.is_empty(), "a log to keep");
        assert_eq!(Store::checked_file(&path).unwrap(), None);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            file,
            "the check changed the file"
        );
        assert_eq!(
            std::fs::read(&wal).unwrap(),
            log,
            "the check changed the log"
        );
        // as a plain connection's close does not
        drop(
            Connection::open(&path)
                .unwrap()
                .query_row("PRAGMA quick_check(1)", [], |r| r.get::<_, String>(0))
                .unwrap(),
        );
        drop(Connection::open(&path).unwrap());
        assert!(
            std::fs::read(&path).unwrap() != file || !wal.exists(),
            "a plain close carried nothing, so the test shows nothing"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_deployments_write_ahead_log_is_cut_back() {
        let dir = std::env::temp_dir().join(format!("of-wal-limit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let s = Store::open(&dir.join("store.sqlite"), 0).unwrap();
        let limit: i64 = s
            .conn
            .query_row("PRAGMA journal_size_limit", [], |r| r.get(0))
            .unwrap();
        assert_eq!(limit, WAL_LIMIT);
        assert_eq!(s.quick_check().unwrap(), None);
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An actor type's identity is read without the case of its ASCII
    /// letters by an index on its folded column, which a store's tables are
    /// made with, never a scan of the table (ADR-0194 decision 1).
    #[test]
    fn an_identity_is_read_by_its_folded_index() {
        let text = "module: people\ncategories: [live]\ntypes:\n  Person:\n    attributes:\n      login: { type: identity, actor_kind: human, unique: true }\n    states:\n      ACTIVE: { category: live }\n    transitions:\n      join: { kind: initial, to: ACTIVE, required_inputs: [login] }\n";
        let lib = Library::load(&[("people.yaml".into(), text.into())]).unwrap();
        let s = Store::sandbox(lib, 0).unwrap();
        let t = s.lib.type_("Person").unwrap();
        let a = t.attribute("login").unwrap();
        let plan: Vec<String> = s
            .connection()
            .prepare(&format!(
                "EXPLAIN QUERY PLAN SELECT id FROM {} WHERE lower({}) = 'ana'",
                table("Person"),
                quote(&column("login", a))
            ))
            .unwrap()
            .query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(plan.iter().any(|p| p.contains("_fold_ix")), "{plan:?}");
    }

    #[test]
    fn a_sandbox_holds_its_operator() {
        let lib = Library::load(&[]).unwrap();
        let store = Store::sandbox(lib, 1_767_225_600).unwrap();
        let world = store.snapshot().unwrap();
        let op = world.whole(OPERATOR).unwrap().unwrap();
        assert_eq!(op.type_, "Operator");
        assert_eq!(op.state, "ACTIVE");
        assert_eq!(op.attr("identity"), Value::Text(OPERATOR.into()));
        let h = store.history(OPERATOR).unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].to_state, "ACTIVE");
    }

    #[test]
    fn an_object_is_read_on_first_use_and_held_as_changed() {
        let lib = Library::load(&[]).unwrap();
        let store = Store::sandbox(lib, 1_767_225_600).unwrap();
        let mut world = store.snapshot().unwrap();
        assert_eq!(world.held().count(), 0);
        // a change the operation makes outlasts a scan that reads the type
        // from the store again
        world.get_mut(OPERATOR).unwrap().unwrap().state = "CHANGED".into();
        let scanned = world.of_type("Operator").unwrap();
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].state, "CHANGED");
        assert_eq!(world.in_order().unwrap()[0].state, "CHANGED");
        // an id the store does not hold is none, and is held once made
        assert_eq!(world.get("nobody").unwrap(), None);
        let order = world.next_order().unwrap();
        assert!(order > world.whole(OPERATOR).unwrap().unwrap().order);
        let mut made = world.whole(OPERATOR).unwrap().unwrap().clone();
        made.id = "nobody".into();
        made.order = order;
        made.state_source = None;
        world.insert(made).unwrap();
        assert_eq!(world.whole("nobody").unwrap().unwrap().order, order);
        assert_eq!(world.next_order().unwrap(), order + 1);
        assert_eq!(world.of_type("Operator").unwrap().len(), 2);
    }

    /// A narrowed read compares a column the table holds no null in alone,
    /// which its index serves; one that may hold none keeps its nulls, which
    /// compare as unknown, and its index serves both.
    #[test]
    fn a_narrowed_read_compares_a_column_holding_no_null_by_its_index() {
        let text = r#"
module: desk
categories: [open, closed]
types:
  Desk:
    states:
      OPEN: { category: open }
    transitions:
      add: { kind: initial, to: OPEN }
  Ticket:
    attributes:
      desk: { reference: Desk }
      seat: { reference: Desk, optional: true }
    states:
      OPEN: { category: open }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [desk], optional_inputs: [seat] }
"#;
        let lib = Library::load(&[("desk.yaml".into(), text.into())]).unwrap();
        let store = Store::sandbox(lib, 1_767_225_600).unwrap();
        let plan = |columns: &[(&str, &Value)]| -> (String, String) {
            let n = narrowing(&store.conn, &store.lib, "Ticket", None, columns)
                .unwrap()
                .unwrap();
            let (cond, bound) = n.conditions("t.");
            let mut st = store
                .conn
                .prepare(&format!(
                    "EXPLAIN QUERY PLAN SELECT t.id FROM t_ticket t WHERE {cond}"
                ))
                .unwrap();
            let plan: Vec<String> = st
                .query_map(rusqlite::params_from_iter(bound), |r| r.get(3))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            (cond, plan.join("; "))
        };
        let desk = Value::Object("desk-1".into());
        let (cond, p) = plan(&[("desk", &desk)]);
        assert!(!cond.contains("IS NULL"), "{cond}");
        assert!(
            p.contains("USING INDEX t_ticket_desk_id_ix") && !p.contains("SCAN"),
            "{p}"
        );
        let (cond, p) = plan(&[("seat", &desk)]);
        assert!(cond.contains("IS NULL"), "{cond}");
        assert!(
            p.contains("t_ticket_seat_id_ix") && !p.contains("SCAN"),
            "{p}"
        );
    }

    /// The built-in label table is read by a label's name through its own
    /// index (`storage-schema.md` §6).
    #[test]
    fn a_label_is_read_by_its_name_through_its_index() {
        let store = Store::sandbox(Library::load(&[]).unwrap(), 1_767_225_600).unwrap();
        let late = Value::Text("late".into());
        let n = narrowing(
            &store.conn,
            &store.lib,
            crate::model::LABEL,
            None,
            &[("name", &late)],
        )
        .unwrap()
        .unwrap();
        let (cond, bound) = n.conditions("t.");
        let mut st = store
            .conn
            .prepare(&format!(
                "EXPLAIN QUERY PLAN SELECT t.id FROM t_label t WHERE {cond}"
            ))
            .unwrap();
        let plan: Vec<String> = st
            .query_map(rusqlite::params_from_iter(bound), |r| r.get(3))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(
            plan.iter()
                .any(|p| p.contains("USING INDEX t_label_by_name")),
            "{plan:?}"
        );
    }

    /// An attribute marked `indexed` has an index on its column, which a
    /// read by it goes through, and so has an external identifier, which every
    /// import and publish reads by value (D606); one whose reference or
    /// single-attribute key is indexed already gets no second (D593).
    #[test]
    fn an_indexed_attribute_is_read_by_its_index() {
        let text = r#"
module: post
categories: [open, closed]
types:
  Depot:
    attributes:
      name: { type: string }
    states:
      OPEN: { category: open }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [name] }
  Parcel:
    attributes:
      code: { type: string, indexed: true }
      note: { type: string, optional: true }
      serial: { type: string, unique: true, indexed: true }
      depot: { reference: Depot, indexed: true }
      bill: { type: string, external: ledger, optional: true }
    states:
      OPEN: { category: open }
    transitions:
      send: { kind: initial, to: OPEN, required_inputs: [code, serial, depot], optional_inputs: [note, bill] }
"#;
        let lib = Library::load(&[("post.yaml".into(), text.into())]).unwrap();
        let store = Store::sandbox(lib, 1_767_225_600).unwrap();
        let indexes: Vec<String> = store
            .conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND tbl_name = 't_parcel'",
            )
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .filter(|n| !n.starts_with("sqlite_"))
            .collect();
        let mut indexes = indexes;
        indexes.sort();
        assert_eq!(
            indexes,
            [
                "t_parcel_asserted_ix",
                "t_parcel_bill_ix",
                "t_parcel_code_ix",
                "t_parcel_depot_id_ix",
                "t_parcel_serial_key_ix",
                "t_parcel_state_ix",
            ]
        );
        let plan: Vec<String> = store
            .conn
            .prepare("EXPLAIN QUERY PLAN SELECT id FROM t_parcel WHERE \"code\" = ?1")
            .unwrap()
            .query_map(["P-1"], |r| r.get::<_, String>(3))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(
            plan.iter().any(|p| p.contains("t_parcel_code_ix")),
            "{plan:?}"
        );
    }

    #[test]
    fn a_uniqueness_key_is_indexed_and_read_by_its_index() {
        let text = r#"
module: desk
categories: [open, closed]
types:
  Ticket:
    attributes:
      ref: { type: string, unique: true }
      team: { type: string, optional: true }
      seat: { type: string, optional: true, unique: { with: [team] } }
    states:
      OPEN: { category: open }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [ref], optional_inputs: [team, seat] }
"#;
        let lib = Library::load(&[("desk.yaml".into(), text.into())]).unwrap();
        let store = Store::sandbox(lib, 1_767_225_600).unwrap();
        let indexes: Vec<String> = store
            .conn
            .prepare("SELECT sql FROM sqlite_master WHERE type = 'index' AND tbl_name = 't_ticket'")
            .unwrap()
            .query_map([], |r| r.get::<_, Option<String>>(0))
            .unwrap()
            .filter_map(|r| r.unwrap())
            .collect();
        assert!(
            indexes
                .iter()
                .any(|i| i.contains("t_ticket_ref_key_ix") && i.contains("(\"ref\")")),
            "{indexes:?}"
        );
        assert!(
            indexes
                .iter()
                .any(|i| i.contains("t_ticket_seat_key_ix") && i.contains("(\"seat\", \"team\")")),
            "{indexes:?}"
        );
        // and the read by key goes through it, not the table
        let plan: Vec<String> = store
            .conn
            .prepare("EXPLAIN QUERY PLAN SELECT id FROM t_ticket WHERE \"ref\" = ?1")
            .unwrap()
            .query_map(["T-1"], |r| r.get::<_, String>(3))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(
            plan.iter().any(|p| p.contains("t_ticket_ref_key_ix")),
            "{plan:?}"
        );
    }

    #[test]
    fn a_read_by_state_keeps_what_the_operation_holds() {
        let lib = Library::load(&[]).unwrap();
        let store = Store::sandbox(lib, 1_767_225_600).unwrap();
        let mut world = store.snapshot().unwrap();
        world.get_mut(OPERATOR).unwrap().unwrap().state = "CHANGED".into();
        let states = |s: &[&str]| s.iter().map(|x| x.to_string()).collect();
        // the store holds the operator ACTIVE, the operation CHANGED
        assert!(
            world
                .of_type_in("Operator", &states(&["ACTIVE"]))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            world
                .of_type_in("Operator", &states(&["CHANGED"]))
                .unwrap()
                .len(),
            1
        );
        let fresh = store.snapshot().unwrap();
        assert_eq!(
            fresh
                .of_type_in("Operator", &states(&["ACTIVE"]))
                .unwrap()
                .len(),
            1
        );
        assert!(
            fresh
                .of_type_in("Operator", &states(&[]))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_store_upgraded_past_this_release_is_served_no_more() {
        let lib = Library::load(&[]).unwrap();
        let mut store = Store::sandbox(lib, 1_767_225_600).unwrap();
        // created at this release, its first row
        let first: (i64, String) = store
            .conn
            .query_row("SELECT seq, release FROM of_engine_release", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(first, (1, BUILTINS.to_string()));
        assert!(store.snapshot().is_ok());
        // a later release's upgrade commits from another copy
        store
            .conn
            .execute(
                "INSERT INTO of_engine_release (seq, release, installed_at, installed_by) \
                 VALUES (2, 'objectflow-core 9.0.0', '2026-01-01T00:00:00Z', 'operator')",
                [],
            )
            .unwrap();
        // a read, a request, an attempt's row and a maintenance task each
        // refuse with EngineMismatch
        assert!(store.snapshot().err().unwrap().is_mismatch());
        let tried = store.transact(|_, _| Ok::<_, ()>(((), None)));
        assert!(tried.err().unwrap().is_mismatch());
        assert!(store.simulate(|_, _| ()).err().unwrap().is_mismatch());
        store.attempt_retention = Some(0);
        let pruned = store.maintain(
            crate::model::OPERATOR,
            &MaintenanceTask {
                name: "prune_attempts".into(),
                before: 1_767_225_600,
            },
        );
        assert!(pruned.err().unwrap().is_mismatch());
    }

    #[test]
    fn an_object_that_cannot_be_read_stops_the_operation() {
        let lib = Library::load(&[]).unwrap();
        let store = Store::sandbox(lib, 1_767_225_600).unwrap();
        store.conn.execute_batch("DROP TABLE t_operator;").unwrap();
        let world = store.snapshot().unwrap();
        // never read as an object the store does not hold
        let by_id = world.get(OPERATOR).unwrap_err();
        // unavailable storage, never a construct the core does not run
        assert!(matches!(by_id, NotRun::Unreadable(_)), "{by_id:?}");
        assert!(by_id.what().contains(OPERATOR), "{by_id}");
        assert!(world.of_type("Operator").is_err());
        assert!(world.in_order().is_err());
        // nor an identifier as one the store does not hold
        store
            .conn
            .execute_batch("DROP TABLE of_external_id;")
            .unwrap();
        let world = store.snapshot().unwrap();
        assert!(world.external_id("legacy", "Robot:R1").is_err());
        assert!(world.external_all().is_err());
    }

    #[test]
    fn a_type_the_library_lacks_is_refused_not_passed_over() {
        let lib = Library::load(&[]).unwrap();
        let store = Store::sandbox(lib, 1_767_225_600).unwrap();
        store
            .conn
            .execute_batch(
                "INSERT INTO of_object (id, type, created_at, created_by_kind, recorded_from) \
                 VALUES ('ghost-1', 'Ghost', '2026-01-01T00:00:00Z', 'human', '2026-01-01T00:00:00Z');",
            )
            .unwrap();
        let world = store.snapshot().unwrap();
        let whole = world.in_order().unwrap_err();
        assert!(matches!(whole, NotRun::Unreadable(_)), "{whole:?}");
        assert!(whole.what().contains("Ghost"), "{whole}");
        assert!(world.get("ghost-1").is_err());
        // a scan of another type is not affected
        assert_eq!(world.of_type("Operator").unwrap().len(), 1);
    }

    #[test]
    fn a_damaged_row_is_refused_not_read_as_a_default() {
        let sandbox = || Store::sandbox(Library::load(&[]).unwrap(), 1_767_225_600).unwrap();
        // an event's time and its payload
        for (column, text) in [
            ("recorded_at", "later"),
            ("occurred_at", "soon"),
            ("payload", "{oops"),
        ] {
            let store = sandbox();
            store
                .conn
                .execute(&format!("UPDATE of_event SET {column} = ?1"), [text])
                .unwrap();
            let e = store.history(OPERATOR).unwrap_err();
            assert!(e.0.contains(column), "{column}: {}", e.0);
            assert!(!e.0.contains(text), "the text is not named: {}", e.0);
            assert!(store.snapshot().unwrap().log().is_err());
        }
        // an attempt's recorded values: an unknown actor's request is one
        let mut store = sandbox();
        let refused = crate::request::request(
            &mut store,
            &crate::request::Request {
                transition: "anything".into(),
                actor: "nobody".into(),
                object_id: None,
                type_: Some("Operator".into()),
                inputs: BTreeMap::new(),
                occurred_at: None,
                propose: false,
                expires_at: None,
                expected_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            },
        );
        assert!(refused.unwrap().is_ok());
        assert_eq!(store.snapshot().unwrap().attempts().unwrap().len(), 1);
        store
            .conn
            .execute_batch("UPDATE of_attempt SET request_values = '{oops';")
            .unwrap();
        let e = store.snapshot().unwrap().attempts().unwrap_err();
        assert!(matches!(e, NotRun::Unreadable(_)), "{e:?}");
        assert!(e.what().contains("request_values"), "{e}");
        // a legacy entry
        let store = sandbox();
        store
            .conn
            .execute(
                "INSERT INTO of_legacy_entry (object_id, ordinal, kind, occurred_at, payload, kept) \
                 VALUES (?1, 1, 'note', '2026-01-01T00:00:00Z', '{oops', '[]')",
                [OPERATOR],
            )
            .unwrap();
        let e = store.legacy_entries(OPERATOR).unwrap_err();
        assert!(e.0.contains("payload"), "{}", e.0);
    }
}

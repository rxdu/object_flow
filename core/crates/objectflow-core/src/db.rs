//! The database the store reaches, behind one interface (ADR-0213 decision
//! 2): the part of `rusqlite`'s the core uses, a connection, its statements
//! and rows, its transactions and savepoints, written in SQL so each backend
//! takes them, and the values bound and read. SQLite is its one backend
//! until PostgreSQL's lands behind it; nothing outside this module names
//! `rusqlite`, which `a_store_names_no_database_but_through_db` holds.

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;
use std::time::Duration;

/// What a statement binds and a row holds, as the store keeps it: a
/// database's five kinds of value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

/// A value borrowed from a row or from what binds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ValueRef<'a> {
    Null,
    Integer(i64),
    Real(f64),
    Text(&'a str),
    Blob(&'a [u8]),
}

impl Value {
    fn as_ref(&self) -> ValueRef<'_> {
        match self {
            Value::Null => ValueRef::Null,
            Value::Integer(i) => ValueRef::Integer(*i),
            Value::Real(r) => ValueRef::Real(*r),
            Value::Text(t) => ValueRef::Text(t),
            Value::Blob(b) => ValueRef::Blob(b),
        }
    }
}

impl ValueRef<'_> {
    fn to_owned_value(self) -> Value {
        match self {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(i) => Value::Integer(i),
            ValueRef::Real(r) => Value::Real(r),
            ValueRef::Text(t) => Value::Text(t.to_string()),
            ValueRef::Blob(b) => Value::Blob(b.to_vec()),
        }
    }

    fn kind(self) -> &'static str {
        match self {
            ValueRef::Null => "null",
            ValueRef::Integer(_) => "an integer",
            ValueRef::Real(_) => "a real",
            ValueRef::Text(_) => "text",
            ValueRef::Blob(_) => "a blob",
        }
    }
}

/// A database's fault, as its backend gave it.
#[derive(Debug)]
pub struct Error(Fault);

#[derive(Debug)]
enum Fault {
    Sqlite(rusqlite::Error),
    Postgres(tokio_postgres::Error),
    /// What a backend cannot do, or the store asked of it wrongly.
    Unsupported(String),
    /// A fault with the statement that met it, PostgreSQL's SQL as sent.
    InStatement(Box<Fault>, String),
    /// A query expected a row and found none.
    NoRows,
    /// A value read as a type it does not hold.
    Conversion {
        index: usize,
        found: &'static str,
        wanted: &'static str,
    },
    /// A value the store's own decoding refused.
    Undecodable {
        index: usize,
        why: String,
    },
    /// A name the store made past PostgreSQL's 63 bytes, which it would cut
    /// to the first 63 and so merge with another name cut to the same.
    NameTooLong(String),
    /// A commit refused, the transaction rolled back by a statement of it
    /// that failed, which PostgreSQL answers a `COMMIT` of with `ROLLBACK`
    /// and no error.
    RolledBack(String),
    /// A statement on PostgreSQL stopped at the deadline set on its
    /// connection, which is closed (ADR-0215), naming what it was doing.
    DeadlinePassed(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    /// A query that found no row, where one was expected.
    pub fn no_rows() -> Error {
        Error(Fault::NoRows)
    }

    /// A column's value the store could not decode, naming the column and
    /// what it holds, never its text, which may be personal.
    pub fn undecodable(index: usize, why: String) -> Error {
        Error(Fault::Undecodable { index, why })
    }

    pub fn is_no_rows(&self) -> bool {
        matches!(
            self.fault(),
            Fault::NoRows | Fault::Sqlite(rusqlite::Error::QueryReturnedNoRows)
        )
    }

    fn fault(&self) -> &Fault {
        match &self.0 {
            Fault::InStatement(f, _) => f,
            f => f,
        }
    }

    /// The fault with the statement that met it.
    fn with_sql(self, sql: &str) -> Error {
        Error(Fault::InStatement(
            Box::new(self.0),
            sql.chars().take(400).collect(),
        ))
    }

    /// PostgreSQL's code for the fault, where it gave one.
    fn sqlstate(&self) -> Option<&tokio_postgres::error::SqlState> {
        match self.fault() {
            Fault::Postgres(e) => e.code(),
            _ => None,
        }
    }

    /// Another connection held the lock past the busy timeout; on
    /// PostgreSQL a lock not had in its timeout, or a serialisable
    /// transaction the server rolled back, each retried as SQLite's busy is.
    pub fn is_busy(&self) -> bool {
        use tokio_postgres::error::SqlState;
        if let Some(code) = self.sqlstate() {
            return [
                SqlState::LOCK_NOT_AVAILABLE,
                SqlState::T_R_SERIALIZATION_FAILURE,
                SqlState::T_R_DEADLOCK_DETECTED,
            ]
            .contains(code);
        }
        matches!(self.fault(), Fault::Sqlite(rusqlite::Error::SqliteFailure(f, _))
            if matches!(f.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked))
    }

    /// The connection to the database was lost: closed by the server, its
    /// restart or an idle timeout among the reasons, or its socket failed.
    /// The store is opened again for the next write, its lock taken again
    /// (durability on PostgreSQL, finding 1); SQLite has no connection to
    /// lose.
    /// A statement stopped at its connection's deadline (ADR-0215).
    pub fn is_deadline_passed(&self) -> bool {
        matches!(self.fault(), Fault::DeadlinePassed(_))
    }

    pub fn is_connection_lost(&self) -> bool {
        let Fault::Postgres(e) = self.fault() else {
            return false;
        };
        if e.is_closed() || std::error::Error::source(e).is_some_and(|s| s.is::<std::io::Error>()) {
            return true;
        }
        e.code().is_some_and(|c| {
            let c = c.code();
            c.starts_with("08") || matches!(c, "57P01" | "57P02" | "57P03" | "57P05")
        })
    }

    /// A value past what PostgreSQL holds in an index's entry, 2,704 bytes,
    /// which SQLite has no bound on (records on PostgreSQL, F4).
    pub fn is_value_too_long(&self) -> bool {
        self.sqlstate()
            .is_some_and(|c| *c == tokio_postgres::error::SqlState::PROGRAM_LIMIT_EXCEEDED)
    }

    /// A name the store made would be cut by PostgreSQL.
    pub fn is_name_too_long(&self) -> bool {
        matches!(self.fault(), Fault::NameTooLong(_))
    }

    /// The role may not do what was asked, a checkpoint without
    /// `pg_checkpoint`.
    pub fn is_insufficient_privilege(&self) -> bool {
        self.sqlstate()
            .is_some_and(|c| *c == tokio_postgres::error::SqlState::INSUFFICIENT_PRIVILEGE)
    }

    /// The volume the database writes to is full.
    pub fn is_disk_full(&self) -> bool {
        if let Some(code) = self.sqlstate() {
            return *code == tokio_postgres::error::SqlState::DISK_FULL;
        }
        matches!(self.fault(), Fault::Sqlite(rusqlite::Error::SqliteFailure(f, _))
            if f.code == rusqlite::ErrorCode::DiskFull)
    }

    /// The file opened is not a database.
    pub fn is_not_a_database(&self) -> bool {
        matches!(self.fault(), Fault::Sqlite(rusqlite::Error::SqliteFailure(f, _))
            if f.code == rusqlite::ErrorCode::NotADatabase)
    }

    /// A full volume's fault, as a test raises one.
    pub fn disk_full() -> Error {
        Error(Fault::Sqlite(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_FULL),
            Some("database or disk is full".into()),
        )))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        show(&self.0, f)
    }
}

fn show(fault: &Fault, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    {
        match fault {
            Fault::Sqlite(e) => fmt::Display::fmt(e, f),
            Fault::Postgres(e) => match e.as_db_error() {
                Some(db) => write!(f, "{}: {}", db.code().code(), db.message()),
                None => fmt::Display::fmt(e, f),
            },
            Fault::Unsupported(what) => write!(f, "not supported: {what}"),
            Fault::InStatement(inner, sql) => {
                show(inner, f)?;
                write!(f, " (in `{sql}`)")
            }
            Fault::NoRows => f.write_str("Query returned no rows"),
            Fault::Conversion {
                index,
                found,
                wanted,
            } => {
                write!(f, "the value at column {index} is {found}, not {wanted}")
            }
            Fault::Undecodable { index, why } => write!(f, "the value at column {index} is {why}"),
            Fault::RolledBack(why) => write!(f, "rolled back: {why}"),
            Fault::DeadlinePassed(what) => write!(
                f,
                "{what} passed its deadline, the database not answering in time; its connection was closed"
            ),
            Fault::NameTooLong(name) => write!(
                f,
                "the name {name}, {} bytes, past the 63 PostgreSQL keeps of a name: it would cut it, and two names cut alike would be one table; a shorter type or attribute name makes it fit",
                name.len()
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Error {
        match e {
            rusqlite::Error::QueryReturnedNoRows => Error(Fault::NoRows),
            e => Error(Fault::Sqlite(e)),
        }
    }
}

/// A value a statement binds.
pub trait ToSql {
    fn to_sql(&self) -> ValueRef<'_>;
}

impl<T: ToSql + ?Sized> ToSql for &T {
    fn to_sql(&self) -> ValueRef<'_> {
        (**self).to_sql()
    }
}

impl<T: ToSql> ToSql for Option<T> {
    fn to_sql(&self) -> ValueRef<'_> {
        self.as_ref().map_or(ValueRef::Null, ToSql::to_sql)
    }
}

impl ToSql for Value {
    fn to_sql(&self) -> ValueRef<'_> {
        self.as_ref()
    }
}

impl ToSql for str {
    fn to_sql(&self) -> ValueRef<'_> {
        ValueRef::Text(self)
    }
}

impl ToSql for String {
    fn to_sql(&self) -> ValueRef<'_> {
        ValueRef::Text(self)
    }
}

impl ToSql for [u8] {
    fn to_sql(&self) -> ValueRef<'_> {
        ValueRef::Blob(self)
    }
}

impl ToSql for Vec<u8> {
    fn to_sql(&self) -> ValueRef<'_> {
        ValueRef::Blob(self)
    }
}

impl ToSql for bool {
    fn to_sql(&self) -> ValueRef<'_> {
        ValueRef::Integer(i64::from(*self))
    }
}

impl ToSql for f64 {
    fn to_sql(&self) -> ValueRef<'_> {
        ValueRef::Real(*self)
    }
}

macro_rules! integers {
    ($($t:ty),*) => {$(
        impl ToSql for $t {
            fn to_sql(&self) -> ValueRef<'_> {
                ValueRef::Integer(i64::from(*self))
            }
        }
    )*};
}
integers!(i64, i32, i16, i8, u32, u16, u8);

impl ToSql for usize {
    // a count the store binds is far below i64's bound; one past it binds
    // its saturation, which no row holds, rather than wrapping
    fn to_sql(&self) -> ValueRef<'_> {
        ValueRef::Integer(i64::try_from(*self).unwrap_or(i64::MAX))
    }
}

impl ToSql for u64 {
    fn to_sql(&self) -> ValueRef<'_> {
        ValueRef::Integer(i64::try_from(*self).unwrap_or(i64::MAX))
    }
}

/// A value a row holds, read as a type.
pub trait FromSql: Sized {
    fn from_sql(v: ValueRef<'_>) -> std::result::Result<Self, &'static str>;
}

impl FromSql for Value {
    fn from_sql(v: ValueRef<'_>) -> std::result::Result<Self, &'static str> {
        Ok(v.to_owned_value())
    }
}

impl<T: FromSql> FromSql for Option<T> {
    fn from_sql(v: ValueRef<'_>) -> std::result::Result<Self, &'static str> {
        match v {
            ValueRef::Null => Ok(None),
            v => T::from_sql(v).map(Some),
        }
    }
}

impl FromSql for String {
    fn from_sql(v: ValueRef<'_>) -> std::result::Result<Self, &'static str> {
        match v {
            ValueRef::Text(t) => Ok(t.to_string()),
            _ => Err("text"),
        }
    }
}

impl FromSql for Vec<u8> {
    fn from_sql(v: ValueRef<'_>) -> std::result::Result<Self, &'static str> {
        match v {
            ValueRef::Blob(b) => Ok(b.to_vec()),
            ValueRef::Text(t) => Ok(t.as_bytes().to_vec()),
            _ => Err("a blob"),
        }
    }
}

impl FromSql for f64 {
    fn from_sql(v: ValueRef<'_>) -> std::result::Result<Self, &'static str> {
        match v {
            ValueRef::Real(r) => Ok(r),
            // as SQLite reads an integer as a real
            #[allow(clippy::cast_precision_loss)]
            ValueRef::Integer(i) => Ok(i as f64),
            _ => Err("a real"),
        }
    }
}

impl FromSql for bool {
    fn from_sql(v: ValueRef<'_>) -> std::result::Result<Self, &'static str> {
        match v {
            ValueRef::Integer(i) => Ok(i != 0),
            _ => Err("a boolean"),
        }
    }
}

macro_rules! from_integers {
    ($($t:ty),*) => {$(
        impl FromSql for $t {
            fn from_sql(v: ValueRef<'_>) -> std::result::Result<Self, &'static str> {
                match v {
                    ValueRef::Integer(i) => <$t>::try_from(i).map_err(|_| "an integer in range"),
                    _ => Err("an integer"),
                }
            }
        }
    )*};
}
from_integers!(i64, i32, u32, u64, usize, u16, u8);

/// What a statement binds: none, a list, or an iterator's values.
pub trait Params {
    fn values(&self) -> Vec<ValueRef<'_>>;
}

impl Params for [&dyn ToSql] {
    fn values(&self) -> Vec<ValueRef<'_>> {
        self.iter().map(|v| v.to_sql()).collect()
    }
}

impl Params for &[&dyn ToSql] {
    fn values(&self) -> Vec<ValueRef<'_>> {
        self.iter().map(|v| v.to_sql()).collect()
    }
}

impl Params for [&dyn ToSql; 0] {
    fn values(&self) -> Vec<ValueRef<'_>> {
        Vec::new()
    }
}

macro_rules! arrays {
    ($($n:literal),*) => {$(
        impl<T: ToSql> Params for [T; $n] {
            fn values(&self) -> Vec<ValueRef<'_>> {
                self.iter().map(ToSql::to_sql).collect()
            }
        }
    )*};
}
arrays!(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16);

/// An iterator's values bound in order, as `rusqlite::params_from_iter`.
pub struct ParamsFromIter<T>(Vec<T>);

pub fn params_from_iter<I>(iter: I) -> ParamsFromIter<I::Item>
where
    I: IntoIterator,
    I::Item: ToSql,
{
    ParamsFromIter(iter.into_iter().collect())
}

impl<T: ToSql> Params for ParamsFromIter<T> {
    fn values(&self) -> Vec<ValueRef<'_>> {
        self.0.iter().map(ToSql::to_sql).collect()
    }
}

/// A list of values to bind, as `rusqlite::params!`.
#[macro_export]
macro_rules! db_params {
    () => { &[] as &[&dyn $crate::db::ToSql] };
    ($($p:expr),+ $(,)?) => { &[$(&$p as &dyn $crate::db::ToSql),+] as &[&dyn $crate::db::ToSql] };
}
pub use crate::db_params as params;

/// A value bound to SQLite as borrowed.
struct Bound<'a>(ValueRef<'a>);

impl rusqlite::ToSql for Bound<'_> {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        use rusqlite::types::{ToSqlOutput, ValueRef as R};
        Ok(ToSqlOutput::Borrowed(match self.0 {
            ValueRef::Null => R::Null,
            ValueRef::Integer(i) => R::Integer(i),
            ValueRef::Real(r) => R::Real(r),
            ValueRef::Text(t) => R::Text(t.as_bytes()),
            ValueRef::Blob(b) => R::Blob(b),
        }))
    }
}

fn bound<'a>(values: &'a [ValueRef<'a>]) -> Vec<Bound<'a>> {
    values.iter().map(|v| Bound(*v)).collect()
}

/// The values bound as SQLite takes them, which it copies as it binds, so
/// they need not outlive the call.
fn bind_refs<'a>(binds: &'a [Bound<'a>]) -> Vec<&'a dyn rusqlite::ToSql> {
    binds.iter().map(|b| b as &dyn rusqlite::ToSql).collect()
}

/// A value bound to PostgreSQL, fitted to the type the server inferred for
/// its place: SQLite takes any value anywhere, and the store's SQL was
/// written for it, so an integer compared with text binds as its text, and
/// a flag stored as an integer binds as the boolean a comparison wants.
#[derive(Debug)]
struct PgBound<'a>(ValueRef<'a>);

fn texty(ty: &tokio_postgres::types::Type) -> bool {
    use tokio_postgres::types::Type;
    [
        Type::TEXT,
        Type::VARCHAR,
        Type::BPCHAR,
        Type::NAME,
        Type::UNKNOWN,
    ]
    .contains(ty)
}

impl tokio_postgres::types::ToSql for PgBound<'_> {
    fn to_sql(
        &self,
        ty: &tokio_postgres::types::Type,
        out: &mut bytes::BytesMut,
    ) -> std::result::Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
    {
        use tokio_postgres::types::{IsNull, Type};
        match self.0 {
            ValueRef::Null => Ok(IsNull::Yes),
            ValueRef::Integer(i) if *ty == Type::INT8 => {
                tokio_postgres::types::ToSql::to_sql(&i, ty, out)
            }
            ValueRef::Integer(i) if *ty == Type::INT4 => {
                tokio_postgres::types::ToSql::to_sql(&i32::try_from(i)?, ty, out)
            }
            ValueRef::Integer(i) if *ty == Type::INT2 => {
                tokio_postgres::types::ToSql::to_sql(&i16::try_from(i)?, ty, out)
            }
            ValueRef::Integer(i) if *ty == Type::BOOL => {
                tokio_postgres::types::ToSql::to_sql(&(i != 0), ty, out)
            }
            #[allow(clippy::cast_precision_loss)]
            ValueRef::Integer(i) if *ty == Type::FLOAT8 => {
                tokio_postgres::types::ToSql::to_sql(&(i as f64), ty, out)
            }
            ValueRef::Integer(i) if texty(ty) => {
                tokio_postgres::types::ToSql::to_sql(&i.to_string(), ty, out)
            }
            ValueRef::Real(r) if *ty == Type::FLOAT8 => {
                tokio_postgres::types::ToSql::to_sql(&r, ty, out)
            }
            #[allow(clippy::cast_possible_truncation)]
            ValueRef::Real(r) if *ty == Type::FLOAT4 => {
                tokio_postgres::types::ToSql::to_sql(&(r as f32), ty, out)
            }
            ValueRef::Real(r) if texty(ty) => {
                tokio_postgres::types::ToSql::to_sql(&r.to_string(), ty, out)
            }
            ValueRef::Text(t) if texty(ty) => tokio_postgres::types::ToSql::to_sql(&t, ty, out),
            ValueRef::Text(t) if *ty == Type::INT8 => {
                tokio_postgres::types::ToSql::to_sql(&t.parse::<i64>()?, ty, out)
            }
            ValueRef::Text(t) if *ty == Type::JSON || *ty == Type::JSONB => {
                serde_json::from_str::<serde_json::Value>(t)?;
                if *ty == Type::JSONB {
                    out.extend_from_slice(&[1]);
                }
                out.extend_from_slice(t.as_bytes());
                Ok(IsNull::No)
            }
            ValueRef::Blob(b) if *ty == Type::BYTEA => {
                tokio_postgres::types::ToSql::to_sql(&b, ty, out)
            }
            v => Err(format!("{} bound where PostgreSQL wants {ty}", v.kind()).into()),
        }
    }

    fn accepts(_: &tokio_postgres::types::Type) -> bool {
        true
    }

    tokio_postgres::types::to_sql_checked!();
}

/// A row a query read.
pub struct Row<'r> {
    inner: RowInner<'r>,
}

enum RowInner<'r> {
    Sqlite(&'r rusqlite::Row<'r>),
    Postgres(&'r tokio_postgres::Row),
}

/// A column of a row, by its place, as `rusqlite::RowIndex`.
pub trait RowIndex {
    fn index(&self) -> usize;
}

impl RowIndex for usize {
    fn index(&self) -> usize {
        *self
    }
}

impl Row<'_> {
    /// The value at a column, read as a type.
    pub fn get<I: RowIndex, T: FromSql>(&self, index: I) -> Result<T> {
        let index = index.index();
        let v = self.get_ref(index)?;
        T::from_sql(v).map_err(|wanted| {
            Error(Fault::Conversion {
                index,
                found: v.kind(),
                wanted,
            })
        })
    }

    /// The value at a column, borrowed.
    pub fn get_ref<I: RowIndex>(&self, index: I) -> Result<ValueRef<'_>> {
        let index = index.index();
        match &self.inner {
            RowInner::Sqlite(r) => {
                use rusqlite::types::ValueRef as R;
                Ok(match r.get_ref(index)? {
                    R::Null => ValueRef::Null,
                    R::Integer(i) => ValueRef::Integer(i),
                    R::Real(r) => ValueRef::Real(r),
                    R::Text(t) => ValueRef::Text(std::str::from_utf8(t).map_err(|_| {
                        Error(Fault::Conversion {
                            index,
                            found: "text not in UTF-8",
                            wanted: "text",
                        })
                    })?),
                    R::Blob(b) => ValueRef::Blob(b),
                })
            }
            RowInner::Postgres(r) => pg_value(r, index),
        }
    }
}

/// A PostgreSQL column read as one of the five kinds SQLite holds: an
/// integer of any width or a boolean as an integer, a real, text, bytes.
fn pg_value<'r>(r: &'r tokio_postgres::Row, index: usize) -> Result<ValueRef<'r>> {
    use tokio_postgres::types::Type;
    let Some(column) = r.columns().get(index) else {
        return Err(Error(Fault::Conversion {
            index,
            found: "no column",
            wanted: "a column",
        }));
    };
    let ty = column.type_();
    let read = |e: tokio_postgres::Error| Error(Fault::Postgres(e));
    Ok(if *ty == Type::INT8 {
        r.try_get::<_, Option<i64>>(index)
            .map_err(read)?
            .map_or(ValueRef::Null, ValueRef::Integer)
    } else if *ty == Type::INT4 {
        r.try_get::<_, Option<i32>>(index)
            .map_err(read)?
            .map_or(ValueRef::Null, |i| ValueRef::Integer(i64::from(i)))
    } else if *ty == Type::INT2 {
        r.try_get::<_, Option<i16>>(index)
            .map_err(read)?
            .map_or(ValueRef::Null, |i| ValueRef::Integer(i64::from(i)))
    } else if *ty == Type::BOOL {
        r.try_get::<_, Option<bool>>(index)
            .map_err(read)?
            .map_or(ValueRef::Null, |b| ValueRef::Integer(i64::from(b)))
    } else if *ty == Type::FLOAT8 {
        r.try_get::<_, Option<f64>>(index)
            .map_err(read)?
            .map_or(ValueRef::Null, ValueRef::Real)
    } else if *ty == Type::FLOAT4 {
        r.try_get::<_, Option<f32>>(index)
            .map_err(read)?
            .map_or(ValueRef::Null, |f| ValueRef::Real(f64::from(f)))
    } else if texty(ty) || *ty == Type::JSON {
        r.try_get::<_, Option<&str>>(index)
            .map_err(read)?
            .map_or(ValueRef::Null, ValueRef::Text)
    } else if *ty == Type::BYTEA {
        r.try_get::<_, Option<&[u8]>>(index)
            .map_err(read)?
            .map_or(ValueRef::Null, ValueRef::Blob)
    } else {
        return Err(Error(Fault::Unsupported(format!(
            "a column of type {ty}, which the store does not read: cast it in the query"
        ))));
    })
}

/// The rows of a query, each mapped as it is read, as
/// `rusqlite::MappedRows`: on SQLite a scan is never held whole; on
/// PostgreSQL the client has read it whole before the first.
pub struct MappedRows<'s, F> {
    rows: MappedInner<'s>,
    f: F,
}

enum MappedInner<'s> {
    Sqlite(rusqlite::Rows<'s>),
    Postgres(std::vec::IntoIter<tokio_postgres::Row>),
}

impl<T, F> Iterator for MappedRows<'_, F>
where
    F: FnMut(&Row<'_>) -> Result<T>,
{
    type Item = Result<T>;

    fn next(&mut self) -> Option<Result<T>> {
        match &mut self.rows {
            MappedInner::Sqlite(rows) => match rows.next() {
                Ok(Some(r)) => Some((self.f)(&Row {
                    inner: RowInner::Sqlite(r),
                })),
                Ok(None) => None,
                Err(e) => Some(Err(e.into())),
            },
            MappedInner::Postgres(rows) => rows.next().map(|r| {
                (self.f)(&Row {
                    inner: RowInner::Postgres(&r),
                })
            }),
        }
    }
}

/// The rows of a query, read one at a time.
pub struct Rows<'s> {
    inner: RowsInner<'s>,
}

enum RowsInner<'s> {
    Sqlite(rusqlite::Rows<'s>),
    Postgres {
        rows: Vec<tokio_postgres::Row>,
        at: usize,
    },
}

impl Rows<'_> {
    // each row borrows the rows it came from, which `Iterator` cannot
    // say, so it is named as `rusqlite::Rows::next` is
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Row<'_>>> {
        match &mut self.inner {
            RowsInner::Sqlite(rows) => Ok(rows.next()?.map(|r| Row {
                inner: RowInner::Sqlite(r),
            })),
            RowsInner::Postgres { rows, at } => {
                *at += 1;
                Ok(rows.get(*at - 1).map(|r| Row {
                    inner: RowInner::Postgres(r),
                }))
            }
        }
    }
}

/// A statement, prepared once and run as often as asked.
pub struct Statement<'c> {
    inner: StatementInner<'c>,
}

enum StatementInner<'c> {
    Sqlite(rusqlite::Statement<'c>),
    SqliteCached(rusqlite::CachedStatement<'c>),
    Postgres(&'c Pg, tokio_postgres::Statement),
}

/// A statement the connection keeps prepared.
pub type CachedStatement<'c> = Statement<'c>;

impl<'c> Statement<'c> {
    fn sqlite(&mut self) -> Option<&mut rusqlite::Statement<'c>> {
        match &mut self.inner {
            StatementInner::Sqlite(s) => Some(s),
            StatementInner::SqliteCached(s) => Some(&mut *s),
            StatementInner::Postgres(..) => None,
        }
    }

    /// Each row the query reads, mapped as it is read.
    pub fn query_map<T, P, F>(&mut self, params: P, f: F) -> Result<MappedRows<'_, F>>
    where
        P: Params,
        F: FnMut(&Row<'_>) -> Result<T>,
    {
        let values = params.values();
        if let StatementInner::Postgres(pg, s) = &self.inner {
            let rows = pg.query(s, &values)?;
            return Ok(MappedRows {
                rows: MappedInner::Postgres(rows.into_iter()),
                f,
            });
        }
        let binds = bound(&values);
        let Some(s) = self.sqlite() else {
            unreachable!("a PostgreSQL statement answered above")
        };
        let rows = s.query(bind_refs(&binds).as_slice())?;
        Ok(MappedRows {
            rows: MappedInner::Sqlite(rows),
            f,
        })
    }

    /// The first row the query reads, mapped; `no_rows` where it reads none.
    pub fn query_row<T, P, F>(&mut self, params: P, f: F) -> Result<T>
    where
        P: Params,
        F: FnOnce(&Row<'_>) -> Result<T>,
    {
        let mut rows = self.query(params)?;
        match rows.next()? {
            Some(r) => f(&r),
            None => Err(Error::no_rows()),
        }
    }

    /// The rows of the query, read one at a time.
    pub fn query<P: Params>(&mut self, params: P) -> Result<Rows<'_>> {
        let values = params.values();
        if let StatementInner::Postgres(pg, s) = &self.inner {
            let rows = pg.query(s, &values)?;
            return Ok(Rows {
                inner: RowsInner::Postgres { rows, at: 0 },
            });
        }
        let binds = bound(&values);
        let Some(s) = self.sqlite() else {
            unreachable!("a PostgreSQL statement answered above")
        };
        Ok(Rows {
            inner: RowsInner::Sqlite(s.query(bind_refs(&binds).as_slice())?),
        })
    }

    /// The rows the statement changed.
    pub fn execute<P: Params>(&mut self, params: P) -> Result<usize> {
        let values = params.values();
        if let StatementInner::Postgres(pg, s) = &self.inner {
            return pg.execute(s, &values);
        }
        let binds = bound(&values);
        let Some(s) = self.sqlite() else {
            unreachable!("a PostgreSQL statement answered above")
        };
        Ok(s.execute(bind_refs(&binds).as_slice())?)
    }

    pub fn column_count(&self) -> usize {
        match &self.inner {
            StatementInner::Sqlite(s) => s.column_count(),
            StatementInner::SqliteCached(s) => s.column_count(),
            StatementInner::Postgres(_, s) => s.columns().len(),
        }
    }
}

/// How a connection is opened: read-only, or reading and writing, and
/// whether its handle is used from one thread alone.
#[derive(Clone, Copy, Debug)]
pub struct OpenFlags {
    write: bool,
    one_thread: bool,
}

impl OpenFlags {
    pub const READ_ONLY: OpenFlags = OpenFlags {
        write: false,
        one_thread: false,
    };
    pub const READ_WRITE: OpenFlags = OpenFlags {
        write: true,
        one_thread: false,
    };

    /// The handle used from one thread at a time, SQLite's no-mutex mode.
    #[must_use]
    pub fn one_thread(self) -> OpenFlags {
        OpenFlags {
            one_thread: true,
            ..self
        }
    }
}

/// How a write transaction begins: taking the write lock at once, or when
/// it first writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behavior {
    Deferred,
    Immediate,
}

/// Which database a connection reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Sqlite,
    Postgres,
}

/// A connection to PostgreSQL: its client, the statements it keeps
/// prepared, whether a transaction is open, and the schema a sandbox made
/// for itself, dropped with it.
pub struct Pg {
    client: std::cell::RefCell<Option<PgClient>>,
    /// What it was made from, which a reader of the same store connects with.
    config: PgConfig,
    prepared: std::cell::RefCell<std::collections::HashMap<String, tokio_postgres::Statement>>,
    open: std::cell::Cell<bool>,
    schema: Option<String>,
    /// The columns holding no null of each table read, by the store's key:
    /// see [`Connection::not_null_held`].
    shapes: std::cell::RefCell<std::collections::HashMap<String, BTreeSet<String>>>,
    /// Whether a schema was changed since the last transaction ended, so
    /// what is held is let go again as the one that changed it ends.
    reshaped: std::cell::Cell<bool>,
    /// Whether a statement of the open transaction failed, which aborts it
    /// on PostgreSQL until a rollback, to a savepoint or whole: its commit
    /// is then refused, where PostgreSQL answers it `ROLLBACK` without an
    /// error and the store took it as committed (records on PostgreSQL,
    /// F7).
    aborted: std::cell::Cell<bool>,
    /// The database reached, as [`Connection::origin`] reads it, held so it
    /// is known after the connection is lost.
    origin: std::cell::RefCell<Option<String>>,
}

impl Pg {
    fn client(&self) -> Result<std::cell::RefMut<'_, PgClient>> {
        let busy = || {
            Error(Fault::Unsupported(
                "a statement run on a PostgreSQL connection while another's rows are being read"
                    .into(),
            ))
        };
        std::cell::RefMut::filter_map(
            self.client.try_borrow_mut().map_err(|_| busy())?,
            Option::as_mut,
        )
        .map_err(|_| busy())
    }

    /// A statement's outcome, a failure inside a transaction marking it
    /// aborted, as the server holds it.
    fn met<T>(&self, outcome: Result<T>) -> Result<T> {
        if outcome.is_err() && self.open.get() {
            self.aborted.set(true);
        }
        outcome
    }

    fn prepare(&self, sql: &str) -> Result<tokio_postgres::Statement> {
        // kept by the SQL as the store writes it, so a statement prepared
        // before is found without translating it again: a request runs
        // tens of them
        if let Some(s) = self.prepared.borrow().get(sql) {
            return Ok(s.clone());
        }
        let translated = to_postgres(sql);
        let s = self.met(
            blocking(|| self.client()?.prepare(&translated)).map_err(|e| e.with_sql(&translated)),
        )?;
        self.prepared
            .borrow_mut()
            .insert(sql.to_string(), s.clone());
        Ok(s)
    }

    fn bind<'v>(values: &'v [ValueRef<'v>]) -> Vec<PgBound<'v>> {
        values.iter().map(|v| PgBound(*v)).collect()
    }

    fn query(
        &self,
        s: &tokio_postgres::Statement,
        values: &[ValueRef<'_>],
    ) -> Result<Vec<tokio_postgres::Row>> {
        let binds = Pg::bind(values);
        let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = binds
            .iter()
            .map(|b| b as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();
        self.met(blocking(|| self.client()?.query(s, &refs)))
    }

    fn execute(&self, s: &tokio_postgres::Statement, values: &[ValueRef<'_>]) -> Result<usize> {
        let binds = Pg::bind(values);
        let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = binds
            .iter()
            .map(|b| b as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();
        let n = self.met(blocking(|| self.client()?.execute(s, &refs)))?;
        Ok(usize::try_from(n).unwrap_or(usize::MAX))
    }

    fn batch(&self, sql: &str) -> Result<()> {
        let translated = to_postgres(sql);
        if translated.trim().is_empty() {
            return Ok(());
        }
        let head = translated.trim_start().to_ascii_uppercase();
        // a schema changed drops what was prepared against it, which the
        // server would refuse as a plan of another shape
        let ddl = ["CREATE", "ALTER", "DROP"].iter().any(|w| head.contains(w));
        if ddl {
            self.prepared.borrow_mut().clear();
            self.shapes.borrow_mut().clear();
            self.reshaped.set(true);
        }
        // a table changed inside a transaction has the keys checked that
        // were deferred to its commit, which PostgreSQL holds as pending
        // events no change of the table may pass, and defers them again
        // after; SQLite turns its keys off around the same rebuild
        // a name PostgreSQL would cut to 63 bytes refused before it is sent:
        // two set attributes of a long type shared one table, made `IF NOT
        // EXISTS`, a document lost and the log recording a write never made
        // (records on PostgreSQL, F1)
        if ddl && let Some(name) = long_identifier(&translated) {
            return self.met(Err(Error(Fault::NameTooLong(name))));
        }
        let ends = head.starts_with("COMMIT") || head.starts_with("END");
        if ends && self.aborted.get() {
            let rolled = blocking(|| self.client()?.batch_execute("ROLLBACK"));
            self.open.set(false);
            self.aborted.set(false);
            if self.reshaped.replace(false) {
                self.shapes.borrow_mut().clear();
            }
            rolled?;
            return Err(Error(Fault::RolledBack(
                "a statement of the transaction failed, so nothing of it was committed".into(),
            )));
        }
        let sent = if ddl && self.open.get() {
            format!("SET CONSTRAINTS ALL IMMEDIATE; {translated}; SET CONSTRAINTS ALL DEFERRED")
        } else {
            translated.clone()
        };
        let ran =
            blocking(|| self.client()?.batch_execute(&sent)).map_err(|e| e.with_sql(&translated));
        if ran.is_err() && ends {
            // a commit that fails ends the transaction, rolled back
            self.open.set(false);
            self.aborted.set(false);
        }
        self.met(ran)?;
        // whether a transaction is open, which the server does not say: a
        // begin opens one, a commit or a rollback but to a savepoint ends it
        if head.starts_with("ROLLBACK") {
            self.aborted.set(false);
        }
        if head.starts_with("BEGIN") || head.starts_with("START TRANSACTION") {
            self.open.set(true);
            self.aborted.set(false);
        } else if head.starts_with("COMMIT")
            || head.starts_with("END")
            || (head.starts_with("ROLLBACK") && !head.starts_with("ROLLBACK TO"))
        {
            self.open.set(false);
            // what was read after a schema changed let go once more as its
            // transaction ends, so a change rolled back leaves no shape of it
            if self.reshaped.replace(false) {
                self.shapes.borrow_mut().clear();
            }
        } else if head.starts_with("ROLLBACK TO") && self.reshaped.get() {
            self.shapes.borrow_mut().clear();
        }
        Ok(())
    }
}

impl Drop for Pg {
    fn drop(&mut self) {
        // a sandbox's schema dropped, and the client closed, each blocking
        // as the client does, beside a caller's runtime too
        let client = self.client.get_mut().take();
        let schema = self.schema.take();
        blocking(move || {
            if let (Some(schema), Some(c)) = (schema, client) {
                let _ =
                    c.batch_execute(&format!("ROLLBACK; DROP SCHEMA IF EXISTS {schema} CASCADE"));
                drop(c);
            }
        });
    }
}

/// A call into the PostgreSQL client, which blocks on a runtime of its own:
/// made through `block_in_place` where the caller is a worker of a
/// multi-threaded tokio runtime, the service's, on which the client's
/// runtime cannot block, and as it is anywhere else (ADR-0213).
fn blocking<T>(f: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current() {
        Ok(h) if h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(f)
        }
        _ => f(),
    }
}

/// How long the making of a PostgreSQL connection is waited for, its
/// startup and the store's checks of the database included: the socket's
/// connect alone was bounded, by `connect_timeout`, so a server that stops
/// answering with its socket open held a new connection without end
/// (ADR-0215).
const PG_SETUP_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a closing connection is given to tell the server it goes.
const PG_GOODBYE: Duration = Duration::from_secs(1);

/// A client of PostgreSQL: `tokio-postgres`'s, blocked on a runtime of its
/// own, as the `postgres` crate wrapped it, its connection's I/O a task of
/// that runtime, run while a call waits; and each call bounded by the
/// deadline set on it, past which the connection is closed, so a read is
/// answered at its deadline by a server that stops answering with its
/// connection open, frozen or stalled on its disk, where it waited with the
/// server (ADR-0215). A write's connection has none, since a commit cut
/// short has an outcome no one knows.
struct PgClient {
    runtime: tokio::runtime::Runtime,
    client: Option<tokio_postgres::Client>,
    connection: std::cell::RefCell<Option<tokio::task::JoinHandle<()>>>,
    deadline: std::cell::Cell<Option<std::time::Instant>>,
}

impl fmt::Debug for PgClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PgClient")
            .field("deadline", &self.deadline.get())
            .finish_non_exhaustive()
    }
}

impl PgClient {
    /// A connection made as `config` says, by `until` or not at all, its
    /// deadline `until` until it is set otherwise.
    fn connect(config: &tokio_postgres::Config, until: std::time::Instant) -> Result<PgClient> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                Error(Fault::Unsupported(format!(
                    "a runtime for PostgreSQL's client: {e}"
                )))
            })?;
        let tls = tls()?;
        // the timer made within the runtime, whose clock it reads
        let made = runtime.block_on(async {
            tokio::time::timeout_at(tokio::time::Instant::from_std(until), config.connect(tls))
                .await
        });
        let (client, connection) = match made {
            Ok(made) => made.map_err(|e| Error(Fault::Postgres(e)))?,
            Err(_) => {
                return Err(Error(Fault::DeadlinePassed(
                    "the connection's making".into(),
                )));
            }
        };
        let connection = runtime.spawn(async move {
            let _ = connection.await;
        });
        Ok(PgClient {
            runtime,
            client: Some(client),
            connection: std::cell::RefCell::new(Some(connection)),
            deadline: std::cell::Cell::new(Some(until)),
        })
    }

    fn client(&self) -> Result<&tokio_postgres::Client> {
        self.client
            .as_ref()
            .filter(|_| self.connection.borrow().is_some())
            .ok_or_else(|| {
                Error(Fault::Unsupported(
                    "a statement on a PostgreSQL connection closed at its deadline".into(),
                ))
            })
    }

    /// `work` waited for, by the deadline where one is set; past it the
    /// connection closed and the call answered as stopped there.
    fn run<T>(
        &self,
        what: &str,
        work: impl std::future::Future<Output = std::result::Result<T, tokio_postgres::Error>>,
    ) -> Result<T> {
        let done = match self.deadline.get() {
            None => self.runtime.block_on(work),
            Some(at) => match self.runtime.block_on(async {
                tokio::time::timeout_at(tokio::time::Instant::from_std(at), work).await
            }) {
                Ok(done) => done,
                Err(_) => {
                    self.close();
                    return Err(Error(Fault::DeadlinePassed(what.to_string())));
                }
            },
        };
        // a call that met the connection's loss closes it, so the loss is
        // known at once, where its task, run only while a call waits, had
        // not yet read the server's goodbye and the connection was taken
        // for open, the next write sent on it and refused (durability on
        // PostgreSQL 1)
        done.map_err(|e| {
            let e = Error(Fault::Postgres(e));
            if e.is_connection_lost() {
                self.close();
            }
            e
        })
    }

    /// The connection closed now: its task stopped, its socket dropped
    /// with it, so nothing of the stopped call is read on it after.
    fn close(&self) {
        let task = self.connection.borrow_mut().take();
        if let Some(task) = task {
            task.abort();
            let _ = self.runtime.block_on(task);
        }
    }

    fn is_closed(&self) -> bool {
        self.client
            .as_ref()
            .is_none_or(tokio_postgres::Client::is_closed)
            || self
                .connection
                .borrow()
                .as_ref()
                .is_none_or(tokio::task::JoinHandle::is_finished)
    }

    fn prepare(&self, sql: &str) -> Result<tokio_postgres::Statement> {
        let c = self.client()?;
        self.run("a statement's preparing", c.prepare(sql))
    }

    fn query<S: ?Sized + tokio_postgres::ToStatement + Sync>(
        &self,
        s: &S,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>> {
        let c = self.client()?;
        self.run("a read", c.query(s, params))
    }

    fn query_one(&self, sql: &str) -> Result<tokio_postgres::Row> {
        let c = self.client()?;
        self.run("a read", c.query_one(sql, &[]))
    }

    fn execute<S: ?Sized + tokio_postgres::ToStatement + Sync>(
        &self,
        s: &S,
        params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
    ) -> Result<u64> {
        let c = self.client()?;
        self.run("a statement", c.execute(s, params))
    }

    fn batch_execute(&self, sql: &str) -> Result<()> {
        let c = self.client()?;
        self.run("a statement", c.batch_execute(sql))
    }

    fn cancel_token(&self) -> Option<tokio_postgres::CancelToken> {
        self.client
            .as_ref()
            .map(tokio_postgres::Client::cancel_token)
    }
}

impl Drop for PgClient {
    fn drop(&mut self) {
        // the client let go, so its connection tells the server it goes and
        // ends, given a moment; one closed at a deadline ended already.
        // Dropped where the thread may block, as `Pg`'s drop does.
        drop(self.client.take());
        if let Some(task) = self.connection.get_mut().take() {
            let _ = self
                .runtime
                .block_on(async { tokio::time::timeout(PG_GOODBYE, task).await });
        }
    }
}

/// The catalog the store reads as SQLite keeps it, made in the schema a
/// PostgreSQL connection writes to: its tables and indexes as
/// `sqlite_master` lists them, an index that backs a constraint left out as
/// SQLite's automatic ones are, and a table's columns as `PRAGMA
/// table_info` gives them, each type named as SQLite names it.
/// The catalog a store on PostgreSQL is read through, which the release's
/// fingerprint digests.
pub(crate) const PG_CATALOG: &str = r#"
CREATE OR REPLACE VIEW objectflow_catalog AS
  SELECT 'table'::text AS type, c.relname::text AS name, c.relname::text AS tbl_name, NULL::text AS sql
    FROM pg_class c
   WHERE c.relnamespace = current_schema()::regnamespace AND c.relkind = 'r'
  UNION ALL
  SELECT 'index'::text, i.relname::text, r.relname::text, pg_get_indexdef(i.oid)
    FROM pg_index x
    JOIN pg_class i ON i.oid = x.indexrelid
    JOIN pg_class r ON r.oid = x.indrelid
   WHERE i.relnamespace = current_schema()::regnamespace
     AND NOT EXISTS (SELECT 1 FROM pg_constraint k WHERE k.conindid = i.oid);
CREATE OR REPLACE FUNCTION objectflow_table_info(t text)
  RETURNS TABLE (cid bigint, name text, type text, "notnull" bigint, dflt_value text, pk bigint)
  LANGUAGE sql STABLE AS $$
  SELECT (a.attnum - 1)::bigint, a.attname::text,
         CASE format_type(a.atttypid, NULL)
           WHEN 'bigint' THEN 'INTEGER' WHEN 'integer' THEN 'INTEGER' WHEN 'text' THEN 'TEXT'
           WHEN 'double precision' THEN 'REAL' WHEN 'bytea' THEN 'BLOB'
           ELSE upper(format_type(a.atttypid, NULL)) END,
         a.attnotnull::int::bigint,
         pg_get_expr(d.adbin, d.adrelid),
         COALESCE((SELECT array_position(k.conkey, a.attnum)
                     FROM pg_constraint k WHERE k.conrelid = a.attrelid AND k.contype = 'p'), 0)::bigint
    FROM pg_attribute a
    LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
   WHERE a.attrelid = to_regclass(quote_ident(current_schema()) || '.' || quote_ident(t))
     AND a.attnum > 0 AND NOT a.attisdropped
   ORDER BY a.attnum
$$;
CREATE OR REPLACE FUNCTION objectflow_json_path(p text) RETURNS text[]
  LANGUAGE sql IMMUTABLE AS $$
  SELECT COALESCE(array_agg(COALESCE(m[1], m[2], m[3]) ORDER BY n), '{}')
    FROM regexp_matches(p, '\."([^"]*)"|\.([^.\[]+)|\[(\d+)\]', 'g') WITH ORDINALITY AS r(m, n)
$$;
CREATE OR REPLACE FUNCTION objectflow_json(t text) RETURNS jsonb
  LANGUAGE sql IMMUTABLE AS $$ SELECT t::jsonb $$;
CREATE OR REPLACE FUNCTION objectflow_json_valid(t text) RETURNS boolean
  LANGUAGE sql IMMUTABLE AS $$ SELECT pg_input_is_valid(t, 'jsonb') $$;
CREATE OR REPLACE FUNCTION objectflow_json_extract(t text, p text) RETURNS text
  LANGUAGE sql IMMUTABLE AS $$ SELECT t::jsonb #>> objectflow_json_path(p) $$;
CREATE OR REPLACE FUNCTION objectflow_json_type(t text, p text) RETURNS text
  LANGUAGE sql IMMUTABLE AS $$
  SELECT CASE jsonb_typeof(v)
           WHEN 'string' THEN 'text'
           WHEN 'boolean' THEN v::text
           WHEN 'number' THEN CASE WHEN v::text ~ '^-?[0-9]+$' THEN 'integer' ELSE 'real' END
           ELSE jsonb_typeof(v) END
    FROM (SELECT t::jsonb #> objectflow_json_path(p) AS v) x
$$;
CREATE OR REPLACE FUNCTION objectflow_json_set(t text, p text, v jsonb) RETURNS text
  LANGUAGE sql IMMUTABLE AS $$
  SELECT CASE WHEN p = '$' THEN v::text
              ELSE jsonb_set(t::jsonb, objectflow_json_path(p), v, true)::text END
$$;
CREATE OR REPLACE FUNCTION objectflow_json_set(t text, p text, v text) RETURNS text
  LANGUAGE sql IMMUTABLE AS $$ SELECT objectflow_json_set(t, p, to_jsonb(v)) $$;
CREATE OR REPLACE FUNCTION objectflow_json_set(t text, p1 text, v1 jsonb, p2 text, v2 jsonb) RETURNS text
  LANGUAGE sql IMMUTABLE AS $$ SELECT objectflow_json_set(objectflow_json_set(t, p1, v1), p2, v2) $$;
"#;

/// A pragma, which tunes or checks SQLite's own file, as PostgreSQL takes
/// it: a setting of the file's none, a check of it a sound file's answer,
/// and a table's columns read from the catalog above. None where it is no
/// pragma.
fn pragma(sql: &str) -> Option<String> {
    let trimmed = sql.trim().trim_end_matches(';').trim();
    let upper = trimmed.to_ascii_uppercase();
    let rest = upper.strip_prefix("PRAGMA")?.trim_start();
    let body = &trimmed[trimmed.len() - rest.len()..];
    if let Some(args) = rest.strip_prefix("TABLE_INFO") {
        let name = body[body.len() - args.len()..]
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim()
            .trim_matches('"')
            .replace('\'', "''");
        return Some(format!(
            "SELECT cid, name, type, \"notnull\", dflt_value, pk FROM objectflow_table_info('{name}')"
        ));
    }
    Some(
        if rest.starts_with("QUICK_CHECK") || rest.starts_with("INTEGRITY_CHECK") {
            "SELECT 'ok'"
        } else if rest.starts_with("WAL_CHECKPOINT") {
            // the log PostgreSQL keeps is its own: nothing is left to wait on
            "SELECT 0, 0, 0"
        } else if rest.starts_with("FOREIGN_KEY_CHECK") {
            // PostgreSQL holds every key as it writes, so none is broken
            "SELECT NULL::text, NULL::bigint, NULL::text, NULL::bigint WHERE false"
        } else if rest.contains('=') {
            ""
        } else {
            "SELECT 0"
        }
        .to_string(),
    )
}

/// The store's SQL, written for SQLite, as PostgreSQL takes it: its
/// placeholders numbered as `$n`, its integers, reals and blobs as
/// PostgreSQL names them, `instr` as `strpos`, and an insert that ignores a
/// conflict as one that does nothing on it. Text in quotes is left as it
/// is.
pub fn to_postgres(sql: &str) -> String {
    // pragmas, one to a statement, each as PostgreSQL takes it
    if sql
        .trim_start()
        .get(..6)
        .is_some_and(|h| h.eq_ignore_ascii_case("PRAGMA"))
    {
        return sql
            .split(';')
            .filter(|s| !s.trim().is_empty())
            .map(|s| pragma(s).unwrap_or_else(|| to_postgres(s)))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("; ");
    }
    // an insert that ignores a conflict, which the store writes as one
    // statement alone
    let trimmed = sql.trim();
    if let Some(rest) = trimmed
        .get(..16)
        .filter(|h| h.eq_ignore_ascii_case("INSERT OR IGNORE"))
        .map(|_| &trimmed[16..])
    {
        let body = rest.trim_end().trim_end_matches(';');
        return format!(
            "{} ON CONFLICT DO NOTHING",
            to_postgres(&format!("INSERT{body}"))
        );
    }
    let mut out = String::with_capacity(sql.len() + 16);
    let mut next = 0;
    let mut chars = sql.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            // a comment, copied to its line's end: an apostrophe in it opens
            // no string
            '-' if chars.peek().is_some_and(|(_, d)| *d == '-') => {
                out.push(c);
                for (_, d) in chars.by_ref() {
                    out.push(d);
                    if d == '\n' {
                        break;
                    }
                }
            }
            '\'' | '"' => {
                // a quoted string or name, copied whole
                out.push(c);
                for (_, d) in chars.by_ref() {
                    out.push(d);
                    if d == c {
                        break;
                    }
                }
            }
            '?' => {
                let mut digits = String::new();
                while let Some((_, d)) = chars.peek().copied() {
                    if d.is_ascii_digit() {
                        digits.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if digits.is_empty() {
                    next += 1;
                    out.push_str(&format!("${next}"));
                } else {
                    out.push('$');
                    out.push_str(&digits);
                }
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut word = String::from(c);
                while let Some((_, d)) = chars.peek().copied() {
                    if d.is_ascii_alphanumeric() || d == '_' {
                        word.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let upper = word.to_ascii_uppercase();
                match upper.as_str() {
                    // a key SQLite fills where an insert gives none, which
                    // PostgreSQL fills from an identity
                    "INTEGER"
                        if sql[i + word.len()..]
                            .trim_start()
                            .get(..11)
                            .is_some_and(|w| w.eq_ignore_ascii_case("PRIMARY KEY")) =>
                    {
                        out.push_str("BIGINT GENERATED BY DEFAULT AS IDENTITY");
                    }
                    "INTEGER" => out.push_str("BIGINT"),
                    "SQLITE_MASTER" => out.push_str("objectflow_catalog"),
                    "PRAGMA_TABLE_INFO" if sql[i + word.len()..].trim_start().starts_with('(') => {
                        out.push_str("objectflow_table_info");
                    }
                    // SQLite's JSON functions, as the catalog above makes
                    // them, named so no built-in of a later release meets them
                    "JSON" | "JSON_SET" | "JSON_EXTRACT" | "JSON_TYPE" | "JSON_VALID"
                        if sql[i + word.len()..].trim_start().starts_with('(') =>
                    {
                        out.push_str("objectflow_");
                        out.push_str(&word.to_ascii_lowercase());
                    }
                    // a reference's key names the directory, which is made
                    // first, where PostgreSQL refuses one to a table not yet
                    // made and a flow's references may form a cycle; the
                    // engine checks the reference's type, as it does where
                    // the target has subtypes (`storage-schema.md` §3)
                    "REFERENCES" if references_type_table(&sql[i + word.len()..]) => {
                        out.push_str("REFERENCES of_object");
                        skip_reference_target(&mut chars);
                    }
                    // a list built empty, which SQLite reads as matching
                    // nothing and PostgreSQL refuses: the same answer, true
                    // for every row where it is negated (records on
                    // PostgreSQL, outside its area)
                    "IN" if empty_list(&sql[i + word.len()..]) => {
                        let kept = out.trim_end();
                        let negated = kept.len() >= 3
                            && kept[kept.len() - 3..].eq_ignore_ascii_case("NOT")
                            && !kept[..kept.len() - 3]
                                .ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_');
                        if negated {
                            let at = kept.len() - 3;
                            out.truncate(at);
                        }
                        out.push_str(if negated {
                            "IN (NULL) IS NOT TRUE"
                        } else {
                            "IN (NULL) IS TRUE"
                        });
                        for (_, d) in chars.by_ref() {
                            if d == ')' {
                                break;
                            }
                        }
                    }
                    "REAL" => out.push_str("DOUBLE PRECISION"),
                    "BLOB" => out.push_str("BYTEA"),
                    "INSTR" if sql[i + word.len()..].trim_start().starts_with('(') => {
                        out.push_str("strpos")
                    }
                    // a scalar max or min of several values, which
                    // PostgreSQL names greatest and least; one value is the
                    // aggregate, the same on both
                    "MAX" | "MIN" if arguments(&sql[i + word.len()..]) > 1 => {
                        out.push_str(if upper == "MAX" { "GREATEST" } else { "LEAST" });
                    }
                    _ => out.push_str(&word),
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// Whether a `REFERENCES` names a type's table, `t_…`, quoted or not.
/// Whether what follows a word is an empty list, `()` with nothing but
/// space within.
fn empty_list(rest: &str) -> bool {
    rest.trim_start()
        .strip_prefix('(')
        .is_some_and(|r| r.trim_start().starts_with(')'))
}

fn references_type_table(rest: &str) -> bool {
    let r = rest.trim_start().trim_start_matches('"');
    r.starts_with("t_")
}

/// The table name after a `REFERENCES`, skipped, up to its `(`.
fn skip_reference_target(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) {
    while let Some((_, d)) = chars.peek().copied() {
        if d == '(' {
            break;
        }
        chars.next();
    }
}

/// How many arguments the call a text opens with takes, counting commas
/// at its own depth, quotes skipped; none where it opens no call.
fn arguments(rest: &str) -> usize {
    let rest = rest.trim_start();
    if !rest.starts_with('(') {
        return 0;
    }
    let (mut depth, mut count, mut quote) = (0_usize, 1_usize, None::<char>);
    for c in rest.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') => {
                depth -= 1;
                if depth == 0 {
                    return count;
                }
            }
            (None, ',') if depth == 1 => count += 1,
            _ => {}
        }
    }
    count
}

/// The schema a sandbox on PostgreSQL is made in, named for this machine,
/// the process and the name given, so only this machine takes it for
/// orphaned once its process is gone, where a sandbox another machine made
/// on a shared server was dropped as this machine's (the personal-data
/// review on PostgreSQL, outside its area).
pub fn sandbox_schema(name: &str) -> String {
    format!("sandbox_{}_{}_{name}", machine_mark(), std::process::id())
}

/// This machine, as a sandbox's schema names it: a digest of its machine
/// id, or of its host's name where it has none.
fn machine_mark() -> String {
    let seen = std::fs::read_to_string("/etc/machine-id")
        .or_else(|_| std::fs::read_to_string("/proc/sys/kernel/hostname"))
        .unwrap_or_default();
    // FNV-1a, 32 bits: a mark, not a secret
    let mut h: u32 = 0x811c_9dc5;
    for b in seen.trim().bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    format!("m{h:08x}")
}

/// The schemas of this machine's sandboxes whose process no longer runs,
/// dropped: a process killed, a service's test ending it, drops none of its
/// own (ADR-0213). One another machine made, or a process still running, is
/// left; one that cannot be dropped is left to the next.
fn drop_orphaned_sandboxes(client: &PgClient) {
    let Ok(rows) = client.query(
        "SELECT nspname::text FROM pg_namespace WHERE nspname LIKE 'sandbox\\_%' ESCAPE '\\'",
        &[],
    ) else {
        return;
    };
    let mine = format!("sandbox_{}_", machine_mark());
    for row in rows {
        let name: String = row.get(0);
        let pid = name
            .strip_prefix(&mine)
            .and_then(|rest| rest.split('_').next())
            .and_then(|p| p.parse::<u32>().ok());
        if let Some(pid) = pid
            && cfg!(target_os = "linux")
            && !Path::new(&format!("/proc/{pid}")).exists()
        {
            let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {name} CASCADE"));
        }
    }
}

/// How a PostgreSQL store is reached: its connection string or URL, and the
/// password set apart from it, so it is never written where a process list
/// shows it (ADR-0213).
#[derive(Clone)]
pub struct PgConfig(tokio_postgres::Config);

impl PgConfig {
    /// A connection string or a `postgresql://` URL, with the password given
    /// apart where one is; one written in the string itself is refused, as
    /// a command line shows it to every user of the machine.
    pub fn parse(config: &str, password: Option<&str>) -> Result<PgConfig> {
        let mut c: tokio_postgres::Config =
            config.parse().map_err(|e| Error(Fault::Postgres(e)))?;
        if c.get_password().is_some() && password.is_some() {
            return Err(Error(Fault::Unsupported(
                "a password both in the store's connection string and given apart".into(),
            )));
        }
        if let Some(p) = password {
            c.password(p);
        }
        Ok(PgConfig(c))
    }

    /// Whether the string itself carried a password.
    pub fn has_password(config: &str) -> bool {
        config
            .parse::<tokio_postgres::Config>()
            .is_ok_and(|c| c.get_password().is_some())
    }

    /// Whether the connection may run without TLS to a host beyond the
    /// machine: an `sslmode` short of `require`, which falls back to a
    /// plain connection where TLS fails, to a host not the loopback's or a
    /// socket's.
    pub fn plain_to_another_host(&self) -> bool {
        let remote = self.0.get_hosts().iter().any(|h| match h {
            tokio_postgres::config::Host::Tcp(h) => {
                !["localhost", "127.0.0.1", "::1"].contains(&h.as_str())
            }
            tokio_postgres::config::Host::Unix(_) => false,
        });
        remote && self.0.get_ssl_mode() != tokio_postgres::config::SslMode::Require
    }

    /// The database it names, and its host, for what a start says.
    pub fn describe(&self) -> String {
        let host = self
            .0
            .get_hosts()
            .first()
            .map(|h| match h {
                tokio_postgres::config::Host::Tcp(h) => h.clone(),
                tokio_postgres::config::Host::Unix(p) => p.display().to_string(),
            })
            .unwrap_or_default();
        format!(
            "PostgreSQL database {} on {host}",
            self.0.get_dbname().unwrap_or("(the user's)")
        )
    }
}

impl fmt::Debug for PgConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // its password never printed
        f.write_str(&self.describe())
    }
}

/// The first name in a statement past the 63 bytes PostgreSQL keeps of
/// one, bare or quoted, outside its text's literals and comments; none
/// where every name fits.
fn long_identifier(sql: &str) -> Option<String> {
    const KEPT: usize = 63;
    let b = sql.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\'' => {
                i += 1;
                while i < b.len() {
                    if b[i] == b'\'' {
                        if b.get(i + 1) == Some(&b'\'') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' => {
                let start = i + 1;
                i += 1;
                while i < b.len() {
                    if b[i] == b'"' {
                        if b.get(i + 1) == Some(&b'"') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                let name = sql.get(start..i.min(b.len())).unwrap_or_default();
                if name.len() > KEPT {
                    return Some(name.to_string());
                }
                i += 1;
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$')
                {
                    i += 1;
                }
                if i - start > KEPT {
                    return sql.get(start..i).map(str::to_string);
                }
            }
            c if c.is_ascii_digit() => {
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// A connection's settings beside what its configuration names, its own
/// options kept and these after them, so they hold: a connect, and a
/// connection's silence, bounded, where none was and a stalled or vanished
/// database hung every request (durability on PostgreSQL, finding 3); the
/// server's keepalives on the session, so the store's advisory lock goes
/// with a vanished host's connection in about half a minute, where the
/// server's defaults kept it two hours (finding 4); and every commit
/// flushed before it is answered, whatever the database's default
/// (finding 8).
fn settled(config: &tokio_postgres::Config) -> tokio_postgres::Config {
    let mut c = config.clone();
    let mine = c.get_options().unwrap_or_default().to_string();
    c.options(format!(
        "{mine} -c synchronous_commit=on -c tcp_keepalives_idle=10 -c tcp_keepalives_interval=5 \
         -c tcp_keepalives_count=3 -c tcp_user_timeout=30000"
    ))
    .connect_timeout(PG_CONNECT_TIMEOUT)
    .keepalives(true)
    .keepalives_idle(Duration::from_secs(10))
    .keepalives_interval(Duration::from_secs(5))
    .keepalives_retries(3)
    .tcp_user_timeout(Duration::from_secs(30));
    c
}

/// How long a connection to PostgreSQL is waited for.
const PG_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The catalog the store's SQL reads PostgreSQL through, in the
/// connection's schema: made where it is not, or not as this core makes it,
/// under a lock one connection at a time, its mark a comment on its view;
/// left as it is otherwise, where every connection rewrote it before the
/// store's lock was tried, so a start refused changed the database, one
/// waited behind any transaction that had read the view, and connections
/// made at once could fail on each other's rewrite (durability on
/// PostgreSQL, finding 9).
fn catalogued(client: &PgClient) -> Result<()> {
    let mark = catalog_mark();
    let found: Option<String> = client
        .query_one(
            "SELECT CASE WHEN to_regclass('objectflow_catalog') IS NULL THEN NULL \
             ELSE obj_description(to_regclass('objectflow_catalog'), 'pg_class') END",
        )?
        .get(0);
    if found.as_deref() == Some(mark.as_str()) {
        return Ok(());
    }
    client.batch_execute(&format!(
        "BEGIN; SELECT pg_advisory_xact_lock(hashtext('objectflow catalog ' || current_schema())); \
             {PG_CATALOG}; COMMENT ON VIEW objectflow_catalog IS '{mark}'; COMMIT"
    ))
}

/// The catalog's mark: a digest of its text, so a core that makes it
/// otherwise makes it again.
fn catalog_mark() -> String {
    // FNV-1a, 64 bits: a mark, not a secret
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in PG_CATALOG.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("objectflow catalog {h:016x}")
}

/// TLS for a PostgreSQL connection, through rustls on ring with the
/// platform's trust store, as the evaluator client has it; used where the
/// connection string's `sslmode` asks for it, `prefer` by default
/// (ADR-0213 decision 3). A certificate is always verified, host and chain.
fn tls() -> Result<tokio_postgres_rustls::MakeRustlsConnect> {
    use rustls_platform_verifier::BuilderVerifierExt;
    let fault = |e: rustls::Error| Error(Fault::Unsupported(format!("TLS: {e}")));
    let config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(fault)?
    .with_platform_verifier()
    .map_err(fault)?
    .with_no_client_auth();
    Ok(tokio_postgres_rustls::MakeRustlsConnect::new(config))
}

/// A connection to the database.
pub struct Connection {
    inner: Inner,
}

enum Inner {
    Sqlite(rusqlite::Connection),
    Postgres(Box<Pg>),
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.inner {
            Inner::Sqlite(_) => "Connection(SQLite)",
            Inner::Postgres(_) => "Connection(PostgreSQL)",
        })
    }
}

impl Connection {
    /// A SQLite database in memory, a sandbox's.
    pub fn open_in_memory() -> Result<Connection> {
        Ok(Connection {
            inner: Inner::Sqlite(rusqlite::Connection::open_in_memory()?),
        })
    }

    /// A SQLite database file, made where none is.
    pub fn open(path: impl AsRef<Path>) -> Result<Connection> {
        Ok(Connection {
            inner: Inner::Sqlite(rusqlite::Connection::open(path)?),
        })
    }

    /// A SQLite database file opened as asked, never made.
    pub fn open_with_flags(path: impl AsRef<Path>, flags: OpenFlags) -> Result<Connection> {
        use rusqlite::OpenFlags as F;
        let mut f = if flags.write {
            F::SQLITE_OPEN_READ_WRITE
        } else {
            F::SQLITE_OPEN_READ_ONLY
        };
        if flags.one_thread {
            f |= F::SQLITE_OPEN_NO_MUTEX;
        }
        f |= F::SQLITE_OPEN_URI;
        Ok(Connection {
            inner: Inner::Sqlite(rusqlite::Connection::open_with_flags(path, f)?),
        })
    }

    /// A PostgreSQL database, given as a connection string (`host=… dbname=…`
    /// or a `postgresql://` URL); refused where its text is not compared
    /// by bytes, as SQLite compares it and the store's ascending id order
    /// needs (ADR-0213).
    pub fn postgres(config: &str) -> Result<Connection> {
        Connection::postgres_with(&PgConfig::parse(config, None)?)
    }

    /// A PostgreSQL database as its configuration says.
    pub fn postgres_with(config: &PgConfig) -> Result<Connection> {
        Connection::postgres_in(config, None, None)
    }

    /// A PostgreSQL database, every table of it in a schema of its own,
    /// made for it and dropped with it: a sandbox's or a test's.
    pub fn postgres_sandbox(config: &str, schema: &str) -> Result<Connection> {
        Connection::postgres_in(
            &PgConfig::parse(config, None)?,
            Some(schema.to_string()),
            None,
        )
    }

    /// A copy of a PostgreSQL store, every table of its schema with its
    /// rows, in a schema of its own dropped with it, each identity carried
    /// on from where it stood: the adversarial harness's fork, which a file
    /// makes with `VACUUM INTO` (ADR-0213). Its keys to other tables are not
    /// copied, as a file's copy turns its keys off.
    pub fn copied(&self) -> Result<Connection> {
        let Inner::Postgres(pg) = &self.inner else {
            return Err(Error(Fault::Unsupported(
                "a copy of a SQLite store, which VACUUM INTO makes".into(),
            )));
        };
        static MADE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let source: String = self.query_row("SELECT current_schema()::text", [], |r| r.get(0))?;
        let tables: Vec<String> = self
            .prepare("SELECT name FROM objectflow_catalog WHERE type = 'table' ORDER BY name")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_>>()?;
        let n = MADE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let fork =
            Connection::postgres_in(&pg.config, Some(sandbox_schema(&format!("fork{n}"))), None)?;
        // every table made and filled, and each identity carried on from
        // the largest value copied, as the store's own would give the next,
        // in one batch: a fork for every request the harness takes
        let identities: Vec<(String, String)> = self
            .prepare(
                "SELECT table_name::text, column_name::text FROM information_schema.columns \
                 WHERE table_schema = current_schema() AND is_identity = 'YES'",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_>>()?;
        let named = |t: &str| format!("\"{}\"", t.replace('"', "\"\""));
        let mut batch = String::new();
        for t in &tables {
            let quoted = named(t);
            batch.push_str(&format!(
                "CREATE TABLE {quoted} (LIKE \"{source}\".{quoted} INCLUDING ALL); \
                 INSERT INTO {quoted} OVERRIDING SYSTEM VALUE SELECT * FROM \"{source}\".{quoted}; "
            ));
        }
        for (t, c) in &identities {
            batch.push_str(&format!(
                "SELECT setval(pg_get_serial_sequence('{}', '{c}'), \
                 GREATEST(COALESCE(max(\"{c}\"), 0), 1), max(\"{c}\") IS NOT NULL) FROM {}; ",
                named(t).replace('\'', "''"),
                named(t)
            ));
        }
        fork.execute_batch(&batch)?;
        Ok(fork)
    }

    /// A connection that reads the same store, a deployment's read
    /// connection (ADR-0178): a SQLite file opened read-only, or the same
    /// PostgreSQL database, read-only by default.
    pub fn reader(&self) -> Result<Connection> {
        match &self.inner {
            Inner::Sqlite(c) => {
                let path = c.path().ok_or_else(|| {
                    Error(Fault::Unsupported(
                        "a reader of a database in memory".into(),
                    ))
                })?;
                Connection::open_with_flags(path, OpenFlags::READ_ONLY.one_thread())
            }
            Inner::Postgres(pg) => {
                let r = Connection::postgres_in(&pg.config, None, None)?;
                r.execute_batch("SET default_transaction_read_only = on")?;
                Ok(r)
            }
        }
    }

    /// A read connection to a PostgreSQL store, read-only, its every
    /// statement, and its making, bounded by `until`, the read's deadline,
    /// past which it is closed (ADR-0215).
    pub fn postgres_reading(config: &PgConfig, until: std::time::Instant) -> Result<Connection> {
        let conn = Connection::postgres_in(config, None, Some(until))?;
        conn.execute_batch("SET default_transaction_read_only = on")?;
        Ok(conn)
    }

    /// The deadline every statement on a read connection is stopped at, the
    /// connection closed (ADR-0215), or none: on PostgreSQL, where a server
    /// that stops answering would hold a statement past it; a write's
    /// connection is given none. SQLite's reads are bounded by the store's
    /// own checks between their steps.
    pub fn set_deadline(&self, at: Option<std::time::Instant>) {
        if let Inner::Postgres(pg) = &self.inner
            && let Ok(client) = pg.client.try_borrow()
            && let Some(client) = client.as_ref()
        {
            client.deadline.set(at);
        }
    }

    fn postgres_in(
        config: &PgConfig,
        schema: Option<String>,
        keep: Option<std::time::Instant>,
    ) -> Result<Connection> {
        blocking(|| Connection::postgres_blocking(config, schema, keep))
    }

    /// A connection made by [`PG_SETUP_TIMEOUT`], or by `keep` where that
    /// comes first, and bounded by `keep` after.
    fn postgres_blocking(
        config: &PgConfig,
        schema: Option<String>,
        keep: Option<std::time::Instant>,
    ) -> Result<Connection> {
        let setup = std::time::Instant::now() + PG_SETUP_TIMEOUT;
        let client = PgClient::connect(&settled(&config.0), keep.map_or(setup, |k| k.min(setup)))?;
        // text sorted as its bytes, and `lower()` folding ASCII letters
        // alone, as the engine and SQLite do: under another character type
        // or ICU's, `lower()` folds `Ä` to `ä`, two identities the engine
        // holds apart match, and an erasure of one renamed the other
        // (records and personal-data reviews on PostgreSQL, ADR-0213)
        let locale = client.query_one(
            "SELECT datcollate::text, datctype::text, datlocprovider::text FROM pg_database \
             WHERE datname = current_database()",
        )?;
        let (collation, ctype, provider): (String, String, String) =
            (locale.get(0), locale.get(1), locale.get(2));
        let bytes = |l: &str| l == "C" || l == "POSIX";
        if !bytes(&collation) || !bytes(&ctype) || provider != "c" {
            return Err(Error(Fault::Unsupported(format!(
                "a PostgreSQL database whose text is compared by the collation {collation} and folded by the character type {ctype}, of the {} provider: the store needs one made with `TEMPLATE template0 LOCALE_PROVIDER libc LC_COLLATE 'C' LC_CTYPE 'C'`, so text sorts as its bytes do and folds its ASCII letters alone, as the engine does",
                if provider == "c" {
                    "libc"
                } else if provider == "i" {
                    "ICU"
                } else {
                    provider.as_str()
                }
            ))));
        }
        if let Some(s) = &schema {
            drop_orphaned_sandboxes(&client);
            client.batch_execute(&format!("CREATE SCHEMA {s}; SET search_path TO {s}"))?;
        }
        catalogued(&client)?;
        client.deadline.set(keep);
        Ok(Connection {
            inner: Inner::Postgres(Box::new(Pg {
                client: std::cell::RefCell::new(Some(client)),
                config: config.clone(),
                prepared: std::cell::RefCell::new(std::collections::HashMap::new()),
                open: std::cell::Cell::new(false),
                schema,
                shapes: std::cell::RefCell::new(std::collections::HashMap::new()),
                reshaped: std::cell::Cell::new(false),
                aborted: std::cell::Cell::new(false),
                origin: std::cell::RefCell::new(None),
            })),
        })
    }

    /// The columns of a table that hold no null, as `read` reads them from
    /// the database: on PostgreSQL read once for each key and held, where
    /// the catalog's answer was a round trip before every narrowed read of
    /// an object, a quarter of the database's time under the profile's load
    /// (Gate P on PostgreSQL, ADR-0213); on SQLite read each time, its
    /// pragma a call in the process. The key names the table and the
    /// library version read by, as a type's table changes shape only with
    /// a publish, a new version, and a reader's library follows the
    /// store's. What is held is let go when the connection changes a
    /// schema, and again as that transaction ends or rolls back to a
    /// savepoint, so neither a publish's relaxed tables nor one rolled back
    /// outlive it under a version a later publish takes.
    pub fn not_null_held(
        &self,
        key: &str,
        read: impl FnOnce() -> Result<BTreeSet<String>>,
    ) -> Result<BTreeSet<String>> {
        let Inner::Postgres(pg) = &self.inner else {
            return read();
        };
        if let Some(held) = pg.shapes.borrow().get(key).cloned() {
            return Ok(held);
        }
        let found = read()?;
        pg.shapes
            .borrow_mut()
            .insert(key.to_string(), found.clone());
        Ok(found)
    }

    /// Which database the connection reaches.
    pub fn backend(&self) -> Backend {
        match self.inner {
            Inner::Sqlite(_) => Backend::Sqlite,
            Inner::Postgres(_) => Backend::Postgres,
        }
    }

    pub fn execute<P: Params>(&self, sql: &str, params: P) -> Result<usize> {
        self.prepare(sql)?.execute(params)
    }

    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        match &self.inner {
            Inner::Sqlite(c) => Ok(c.execute_batch(sql)?),
            Inner::Postgres(pg) => pg.batch(sql),
        }
    }

    pub fn prepare(&self, sql: &str) -> Result<Statement<'_>> {
        Ok(Statement {
            inner: match &self.inner {
                Inner::Sqlite(c) => StatementInner::Sqlite(c.prepare(sql)?),
                Inner::Postgres(pg) => StatementInner::Postgres(pg, pg.prepare(sql)?),
            },
        })
    }

    /// A statement kept prepared for the connection's next use of it.
    pub fn prepare_cached(&self, sql: &str) -> Result<CachedStatement<'_>> {
        Ok(Statement {
            inner: match &self.inner {
                Inner::Sqlite(c) => StatementInner::SqliteCached(c.prepare_cached(sql)?),
                Inner::Postgres(pg) => StatementInner::Postgres(pg, pg.prepare(sql)?),
            },
        })
    }

    pub fn query_row<T, P, F>(&self, sql: &str, params: P, f: F) -> Result<T>
    where
        P: Params,
        F: FnOnce(&Row<'_>) -> Result<T>,
    {
        self.prepare(sql)?.query_row(params, f)
    }

    /// A transaction, begun as asked, rolled back unless it commits.
    pub fn transaction_with_behavior(&mut self, behavior: Behavior) -> Result<Transaction<'_>> {
        self.begin(behavior)
    }

    /// A transaction on a connection shared, rolled back unless it commits.
    pub fn unchecked_transaction(&self) -> Result<Transaction<'_>> {
        self.begin(Behavior::Deferred)
    }

    /// The statement a transaction begins with on the connection's
    /// database: on PostgreSQL a write's is serialisable, the database's
    /// safety net beneath the one writer, and a read's one snapshot
    /// (ADR-0213 decision 4).
    pub fn begin_sql(&self, behavior: Behavior) -> &'static str {
        match (&self.inner, behavior) {
            (Inner::Sqlite(_), Behavior::Deferred) => "BEGIN DEFERRED",
            (Inner::Sqlite(_), Behavior::Immediate) => "BEGIN IMMEDIATE",
            (Inner::Postgres(_), Behavior::Deferred) => "BEGIN ISOLATION LEVEL REPEATABLE READ",
            (Inner::Postgres(_), Behavior::Immediate) => "BEGIN ISOLATION LEVEL SERIALIZABLE",
        }
    }

    fn begin(&self, behavior: Behavior) -> Result<Transaction<'_>> {
        self.execute_batch(self.begin_sql(behavior))?;
        Ok(Transaction {
            conn: self,
            done: false,
        })
    }

    /// A savepoint inside the transaction open, or a transaction of its
    /// own, released unless it rolls back.
    pub fn savepoint(&mut self) -> Result<Savepoint<'_>> {
        let own = self.is_autocommit();
        if own {
            self.execute_batch(self.begin_sql(Behavior::Deferred))?;
        }
        self.execute_batch("SAVEPOINT objectflow_savepoint")?;
        Ok(Savepoint {
            conn: self,
            done: false,
            own,
        })
    }

    /// Whether no transaction is open.
    /// The database a connection to PostgreSQL reached, as the server names
    /// it: its cluster's system identifier, the timeline it writes on, read
    /// from its current log file's name so a promotion shows at once, and
    /// the database's oid; read once and held. A database restarted keeps
    /// all three; a standby promoted by a failover writes on a new
    /// timeline, and a restore into another cluster or database has another
    /// identifier or oid, either a copy that may lack the last writes the
    /// store answered (durability on PostgreSQL, finding 2). None on
    /// SQLite.
    pub fn origin(&self) -> Result<Option<String>> {
        let Inner::Postgres(pg) = &self.inner else {
            return Ok(None);
        };
        if let Some(held) = pg.origin.borrow().clone() {
            return Ok(Some(held));
        }
        let found: String = self.query_row(
            "SELECT (SELECT system_identifier::text FROM pg_control_system()) || '/' || \
             (('x' || substr(pg_walfile_name(pg_current_wal_lsn()), 1, 8))::bit(32)::int)::text || '/' || \
             (SELECT oid::text FROM pg_database WHERE datname = current_database())",
            [],
            |r| r.get(0),
        )?;
        *pg.origin.borrow_mut() = Some(found.clone());
        Ok(Some(found))
    }

    /// The origin [`Connection::origin`] read, without asking the
    /// database, which a lost connection cannot.
    pub fn held_origin(&self) -> Option<String> {
        match &self.inner {
            Inner::Sqlite(_) => None,
            Inner::Postgres(pg) => pg.origin.borrow().clone(),
        }
    }

    /// Whether the connection is lost, which a statement that met its loss
    /// tells: a store's connection to PostgreSQL closed by the server or its
    /// socket; never on SQLite.
    pub fn is_closed(&self) -> bool {
        match &self.inner {
            Inner::Sqlite(_) => false,
            Inner::Postgres(pg) => pg
                .client
                .try_borrow()
                .map(|c| c.as_ref().is_none_or(PgClient::is_closed))
                .unwrap_or(false),
        }
    }

    pub fn is_autocommit(&self) -> bool {
        match &self.inner {
            Inner::Sqlite(c) => c.is_autocommit(),
            Inner::Postgres(pg) => !pg.open.get(),
        }
    }

    /// How long a statement waits for another connection's lock: on
    /// PostgreSQL its lock timeout, none being a millisecond, since zero
    /// there waits for ever.
    pub fn busy_timeout(&self, timeout: Duration) -> Result<()> {
        match &self.inner {
            Inner::Sqlite(c) => Ok(c.busy_timeout(timeout)?),
            Inner::Postgres(pg) => pg.batch(&format!(
                "SET lock_timeout = '{}ms'",
                timeout.as_millis().max(1)
            )),
        }
    }

    /// That closing the connection carries no log into the file, a
    /// checked copy keeping both as they were: SQLite's alone.
    pub fn keep_log_on_close(&self) -> Result<()> {
        match &self.inner {
            Inner::Sqlite(c) => {
                c.set_db_config(
                    rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
                    true,
                )?;
                Ok(())
            }
            Inner::Postgres(_) => Ok(()),
        }
    }

    /// The file the connection opened, none for a database in memory or a
    /// server's.
    pub fn path(&self) -> Option<&str> {
        match &self.inner {
            Inner::Sqlite(c) => c.path(),
            Inner::Postgres(_) => None,
        }
    }

    /// A handle that stops the statement running on the connection from
    /// another thread.
    pub fn interrupt_handle(&self) -> InterruptHandle {
        InterruptHandle {
            inner: match &self.inner {
                Inner::Sqlite(c) => InterruptInner::Sqlite(c.get_interrupt_handle()),
                Inner::Postgres(pg) => InterruptInner::Postgres(
                    pg.client.borrow().as_ref().and_then(PgClient::cancel_token),
                ),
            },
        }
    }
}

/// Stops the statement running on a connection, from another thread.
pub struct InterruptHandle {
    inner: InterruptInner,
}

enum InterruptInner {
    Sqlite(rusqlite::InterruptHandle),
    Postgres(Option<tokio_postgres::CancelToken>),
}

impl InterruptHandle {
    pub fn interrupt(&self) {
        match &self.inner {
            InterruptInner::Sqlite(h) => h.interrupt(),
            // a cancel that fails leaves the statement to its own deadline;
            // sent over TLS where the server asks it, as it refuses one
            // without
            InterruptInner::Postgres(Some(t)) => {
                if let Ok(tls) = tls() {
                    let _ = blocking(|| {
                        tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .map(|r| r.block_on(t.cancel_query(tls)))
                    });
                }
            }
            InterruptInner::Postgres(None) => {}
        }
    }
}

/// A transaction: it commits when asked, and rolls back otherwise.
pub struct Transaction<'c> {
    conn: &'c Connection,
    done: bool,
}

impl<'c> Transaction<'c> {
    /// A transaction begun on a connection shared, as asked.
    pub fn new_unchecked(conn: &'c Connection, behavior: Behavior) -> Result<Transaction<'c>> {
        conn.begin(behavior)
    }

    pub fn commit(mut self) -> Result<()> {
        self.done = true;
        self.conn.execute_batch("COMMIT")
    }

    pub fn rollback(mut self) -> Result<()> {
        self.done = true;
        self.conn.execute_batch("ROLLBACK")
    }

    /// Ended as it would be dropped: rolled back.
    pub fn finish(self) -> Result<()> {
        self.rollback()
    }
}

impl std::ops::Deref for Transaction<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if !self.done && !self.conn.is_autocommit() {
            // a rollback that fails leaves the connection's next begin to
            // fail, which says so
            let _ = self.conn.execute_batch("ROLLBACK");
        }
    }
}

/// A savepoint: released when it commits, rolled back to otherwise; on a
/// connection with no transaction open, a transaction of its own, as
/// SQLite makes one.
pub struct Savepoint<'c> {
    conn: &'c Connection,
    done: bool,
    own: bool,
}

impl Savepoint<'_> {
    pub fn commit(mut self) -> Result<()> {
        self.done = true;
        self.conn.execute_batch("RELEASE objectflow_savepoint")?;
        if self.own {
            self.conn.execute_batch("COMMIT")?;
        }
        Ok(())
    }

    pub fn rollback(&mut self) -> Result<()> {
        self.conn.execute_batch("ROLLBACK TO objectflow_savepoint")
    }

    /// Ended as it would be dropped: what it wrote undone, and released.
    pub fn finish(mut self) -> Result<()> {
        self.done = true;
        self.conn
            .execute_batch("ROLLBACK TO objectflow_savepoint; RELEASE objectflow_savepoint")?;
        if self.own {
            self.conn.execute_batch("ROLLBACK")?;
        }
        Ok(())
    }
}

impl std::ops::Deref for Savepoint<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn
    }
}

impl Drop for Savepoint<'_> {
    fn drop(&mut self) {
        if !self.done {
            let _ = self
                .conn
                .execute_batch("ROLLBACK TO objectflow_savepoint; RELEASE objectflow_savepoint");
            if self.own {
                let _ = self.conn.execute_batch("ROLLBACK");
            }
        }
    }
}

/// Whether a test of SQLite's own layout stops on PostgreSQL, where its
/// planner and its catalog are its own: said where it does, so a run on
/// PostgreSQL shows what it did not check (ADR-0213).
pub fn sqlite_only(backend: Backend, what: &str) -> bool {
    if backend == Backend::Postgres {
        eprintln!("skipped on PostgreSQL: {what}, which is SQLite's");
        return true;
    }
    false
}

/// A query's one row, or none where it reads none, as
/// `rusqlite::OptionalExtension`.
pub trait OptionalExtension<T> {
    fn optional(self) -> Result<Option<T>>;
}

impl<T> OptionalExtension<T> for Result<T> {
    fn optional(self) -> Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(e) if e.is_no_rows() => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// The wrappers each print their name alone: what they hold is the
/// backend's, and a statement's or a row's values may be personal.
macro_rules! named {
    ($($t:ident $(<$($l:tt),*>)?),*) => {$(
        impl$(<$($l),*>)? fmt::Debug for $t$(<$($l),*>)? {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(stringify!($t))
            }
        }
    )*};
}
named!(
    Row<'r>,
    Rows<'s>,
    Statement<'c>,
    InterruptHandle,
    Transaction<'c>,
    Savepoint<'c>,
    Pg
);

impl<T> fmt::Debug for ParamsFromIter<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ParamsFromIter")
    }
}

impl<F> fmt::Debug for MappedRows<'_, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MappedRows")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing outside this module names the database's crate, so a second
    /// backend arrives behind it without the store's code changing
    /// (ADR-0213 decision 2).
    #[test]
    fn a_store_names_no_database_but_through_db() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = Vec::new();
        let mut todo = vec![src];
        while let Some(dir) = todo.pop() {
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    todo.push(path);
                } else if path.extension().is_some_and(|e| e == "rs")
                    && path.file_name().is_some_and(|n| n != "db.rs")
                    && std::fs::read_to_string(&path).unwrap().contains("rusqlite")
                {
                    found.push(path.display().to_string());
                }
            }
        }
        assert!(found.is_empty(), "rusqlite named outside db.rs: {found:?}");
    }

    /// A list built empty matches no row, and none negated matches every
    /// row, on either backend: SQLite reads `IN ()` so, and PostgreSQL,
    /// which refuses it, is given the same (records on PostgreSQL).
    #[test]
    fn an_empty_list_matches_as_sqlite_reads_it() {
        let conn = match std::env::var(crate::store::SANDBOX_POSTGRES) {
            Ok(c) if !c.trim().is_empty() => {
                Connection::postgres_sandbox(&c, &sandbox_schema("emptylist")).unwrap()
            }
            _ => Connection::open_in_memory().unwrap(),
        };
        conn.execute_batch(
            "CREATE TABLE t (a INTEGER, b TEXT); INSERT INTO t VALUES (1, 'x'), (2, NULL)",
        )
        .unwrap();
        let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
        assert_eq!(count("SELECT count(*) FROM t WHERE b IN ()"), 0);
        assert_eq!(count("SELECT count(*) FROM t WHERE a = 1 AND b IN ()"), 0);
        assert_eq!(count("SELECT count(*) FROM t WHERE b NOT IN ()"), 2);
        assert_eq!(
            count("SELECT count(*) FROM t WHERE a = 2 AND b NOT IN ()"),
            1
        );
    }

    /// A sandbox's schema left by a process that is gone is dropped by the
    /// next sandbox made on this machine, and one another machine made is
    /// left, its process unknown here (the personal-data review on
    /// PostgreSQL, outside its area).
    /// A connection to a server that takes the connection and never
    /// answers, as a frozen one does, its kernel completing the handshake,
    /// is given up at the read's deadline, where only the socket's connect
    /// was bounded and its startup waited without end (ADR-0215). Needs no
    /// database: a listener that never answers is the server.
    #[test]
    fn a_connection_never_answered_is_given_up_at_its_deadline() {
        let silent = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = silent.local_addr().unwrap().port();
        let config =
            PgConfig::parse(&format!("host=127.0.0.1 port={port} user=x dbname=x"), None).unwrap();
        let began = std::time::Instant::now();
        let made =
            Connection::postgres_reading(&config, began + std::time::Duration::from_millis(300));
        let Err(e) = made else {
            panic!("a silent server connected to")
        };
        assert!(e.is_deadline_passed(), "{e}");
        assert!(
            began.elapsed() < std::time::Duration::from_secs(3),
            "{:?}",
            began.elapsed()
        );
        drop(silent);
    }

    /// A read connection's statement is stopped at its deadline and the
    /// connection closed, so a database that does not answer in time holds
    /// no read past it, where a cancel request, which a frozen server never
    /// acts on, was all there was (ADR-0215); a connection given no
    /// deadline, a write's, waits as it did.
    #[test]
    fn a_read_connection_stops_at_its_deadline() {
        let base = match std::env::var(crate::store::SANDBOX_POSTGRES) {
            Ok(c) if !c.trim().is_empty() => c,
            _ => {
                eprintln!("skipped on SQLite: a statement's deadline on PostgreSQL");
                return;
            }
        };
        let config = PgConfig::parse(&base, None).unwrap();
        let began = std::time::Instant::now();
        let read =
            Connection::postgres_reading(&config, began + std::time::Duration::from_millis(500))
                .unwrap();
        let slept = read.query_row("SELECT pg_sleep(5)::text", [], |r| r.get::<_, String>(0));
        let Err(e) = slept else {
            panic!("a read slept past its deadline")
        };
        assert!(e.is_deadline_passed(), "{e}");
        assert!(
            began.elapsed() < std::time::Duration::from_secs(3),
            "{:?}",
            began.elapsed()
        );
        assert!(
            read.is_closed(),
            "a connection stopped at its deadline is closed"
        );
        assert!(
            read.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
                .is_err()
        );
        // a deadline let go, a kept connection's, bounds nothing
        let kept = Connection::postgres_reading(
            &config,
            std::time::Instant::now() + std::time::Duration::from_secs(5),
        )
        .unwrap();
        kept.set_deadline(None);
        let write = Connection::postgres(&base).unwrap();
        for c in [&kept, &write] {
            let began = std::time::Instant::now();
            let slept: String = c
                .query_row("SELECT pg_sleep(0.7)::text", [], |r| r.get(0))
                .unwrap();
            assert_eq!(slept, "");
            assert!(began.elapsed() >= std::time::Duration::from_millis(700));
            assert!(!c.is_closed());
        }
    }

    #[test]
    fn a_sandbox_another_machine_made_is_left() {
        let base = match std::env::var(crate::store::SANDBOX_POSTGRES) {
            Ok(c) if !c.trim().is_empty() => c,
            _ => {
                eprintln!("skipped on SQLite: sandboxes on a shared PostgreSQL");
                return;
            }
        };
        let gone = u32::MAX - 1;
        let mine = format!("sandbox_{}_{gone}_left", machine_mark());
        let theirs = format!("sandbox_m00000000_{gone}_left");
        let admin = Connection::postgres(&base).unwrap();
        for s in [&mine, &theirs] {
            admin
                .execute_batch(&format!(
                    "DROP SCHEMA IF EXISTS {s} CASCADE; CREATE SCHEMA {s}"
                ))
                .unwrap();
        }
        drop(Connection::postgres_sandbox(&base, &sandbox_schema("orphans")).unwrap());
        let left = |s: &str| -> i64 {
            admin
                .query_row(
                    "SELECT count(*) FROM pg_namespace WHERE nspname = ?1",
                    [s],
                    |r| r.get(0),
                )
                .unwrap()
        };
        assert_eq!((left(&mine), left(&theirs)), (0, 1));
        admin
            .execute_batch(&format!("DROP SCHEMA {theirs} CASCADE"))
            .unwrap();
    }

    /// A transaction a failed statement aborted is not committed: PostgreSQL
    /// answers its `COMMIT` with `ROLLBACK` and no error, which the store
    /// took as committed, so the commit is refused; a rollback to a
    /// savepoint takes the transaction back, as the server does (records on
    /// PostgreSQL, F7). SQLite rolls back the failed statement alone, and
    /// commits the rest.
    #[test]
    fn a_transaction_a_failed_statement_aborted_is_not_committed() {
        let conn = match std::env::var(crate::store::SANDBOX_POSTGRES) {
            Ok(c) if !c.trim().is_empty() => {
                Connection::postgres_sandbox(&c, &sandbox_schema("aborted")).unwrap()
            }
            _ => Connection::open_in_memory().unwrap(),
        };
        conn.execute_batch("CREATE TABLE t (a INTEGER NOT NULL)")
            .unwrap();
        let count = |a: i64| -> i64 {
            conn.query_row("SELECT count(*) FROM t WHERE a = ?1", [a], |r| r.get(0))
                .unwrap()
        };
        conn.execute_batch("BEGIN").unwrap();
        conn.execute("INSERT INTO t (a) VALUES (1)", []).unwrap();
        assert!(conn.execute("INSERT INTO t (a) VALUES (NULL)", []).is_err());
        let committed = conn.execute_batch("COMMIT");
        if conn.backend() == Backend::Postgres {
            let refused = committed.unwrap_err().to_string();
            assert!(refused.contains("rolled back"), "{refused}");
            assert_eq!(count(1), 0, "nothing of it committed");
        } else {
            committed.unwrap();
            assert_eq!(count(1), 1);
        }
        assert!(conn.is_autocommit(), "no transaction left open");
        conn.execute_batch("BEGIN; SAVEPOINT s").unwrap();
        conn.execute("INSERT INTO t (a) VALUES (2)", []).unwrap();
        assert!(conn.execute("INSERT INTO t (a) VALUES (NULL)", []).is_err());
        conn.execute_batch("ROLLBACK TO s").unwrap();
        conn.execute("INSERT INTO t (a) VALUES (3)", []).unwrap();
        conn.execute_batch("COMMIT").unwrap();
        assert_eq!(
            (count(2), count(3)),
            (0, 1),
            "the savepoint's work undone, the rest kept"
        );
    }

    /// A connection to PostgreSQL holds what the store's durability needs
    /// whatever its database's defaults: each commit flushed before it is
    /// answered, and the server's keepalives short, so the store's lock goes
    /// with a vanished host's connection (durability on PostgreSQL 4, 8); the
    /// catalog, made once, is left as it is by the next connection (9); and
    /// a database that takes no writes is refused to the store's writer at
    /// its start (S6).
    #[test]
    fn a_connection_to_postgresql_holds_its_settings_and_leaves_the_catalog() {
        let base = match std::env::var(crate::store::SANDBOX_POSTGRES) {
            Ok(c) if !c.trim().is_empty() => c,
            _ => {
                eprintln!("skipped on SQLite: a PostgreSQL connection's settings");
                return;
            }
        };
        let admin = Connection::postgres(&base).unwrap();
        let name = format!("objectflow_settings_{}", std::process::id());
        admin
            .execute_batch(&format!("DROP DATABASE IF EXISTS {name}"))
            .unwrap();
        admin
            .execute_batch(&format!(
                "CREATE DATABASE {name} TEMPLATE template0 ENCODING 'UTF8' LOCALE_PROVIDER libc LC_COLLATE 'C' LC_CTYPE 'C'"
            ))
            .unwrap();
        admin
            .execute_batch(&format!(
                "ALTER DATABASE {name} SET synchronous_commit = off"
            ))
            .unwrap();
        let at = format!("{base} dbname={name}");
        let setting = |c: &Connection, s: &str| -> String {
            c.query_row(&format!("SELECT current_setting('{s}')"), [], |r| r.get(0))
                .unwrap()
        };
        let made = |c: &Connection| -> String {
            c.query_row(
                "SELECT xmin::text FROM pg_class WHERE oid = to_regclass('objectflow_catalog')",
                [],
                |r| r.get(0),
            )
            .unwrap()
        };
        let first = Connection::postgres(&at).unwrap();
        assert_eq!(setting(&first, "synchronous_commit"), "on");
        assert_eq!(setting(&first, "tcp_keepalives_idle"), "10");
        let second = Connection::postgres(&at).unwrap();
        assert_eq!(made(&first), made(&second), "the catalog made again");
        drop((first, second));
        admin
            .execute_batch(&format!(
                "ALTER DATABASE {name} SET default_transaction_read_only = on"
            ))
            .unwrap();
        let refused =
            crate::store::Store::open_postgres(&PgConfig::parse(&at, None).unwrap(), 0, true)
                .map(|_| ())
                .unwrap_err()
                .to_string();
        assert!(refused.contains("takes no writes"), "{refused}");
        admin
            .execute_batch(&format!("DROP DATABASE {name} WITH (FORCE)"))
            .unwrap();
    }

    /// A database that sorts text otherwise than by its bytes, or folds it
    /// beyond ASCII, is refused, each setting alone: the collation, the
    /// character type, which `lower()` follows, and an ICU provider, whose
    /// collation and folding are its own whatever the other two say
    /// (ADR-0213). On SQLite there is nothing to choose.
    #[test]
    fn a_database_sorting_or_folding_text_beyond_bytes_is_refused() {
        let base = match std::env::var(crate::store::SANDBOX_POSTGRES) {
            Ok(c) if !c.trim().is_empty() => c,
            _ => {
                eprintln!(
                    "skipped on SQLite: a PostgreSQL database's locale, which SQLite has none of"
                );
                return;
            }
        };
        let admin = Connection::postgres(&base).unwrap();
        for (what, made) in [
            ("collation", "LC_COLLATE 'en_US.utf8' LC_CTYPE 'C'"),
            ("ctype", "LC_COLLATE 'C' LC_CTYPE 'en_US.utf8'"),
            (
                "icu",
                "LOCALE_PROVIDER icu ICU_LOCALE 'en-US' LC_COLLATE 'C' LC_CTYPE 'C'",
            ),
        ] {
            let name = format!("objectflow_locale_{}_{what}", std::process::id());
            admin
                .execute_batch(&format!("DROP DATABASE IF EXISTS {name}"))
                .unwrap();
            admin
                .execute_batch(&format!(
                    "CREATE DATABASE {name} TEMPLATE template0 ENCODING 'UTF8' {made}"
                ))
                .unwrap();
            let refused = Connection::postgres(&format!("{base} dbname={name}"))
                .map(|_| ())
                .unwrap_err()
                .to_string();
            admin
                .execute_batch(&format!("DROP DATABASE {name} WITH (FORCE)"))
                .unwrap();
            assert!(
                refused.contains("LC_COLLATE 'C' LC_CTYPE 'C'")
                    && refused.contains("LOCALE_PROVIDER libc"),
                "{what}: {refused}"
            );
        }
    }

    /// A name past the 63 bytes PostgreSQL keeps is found, bare or quoted,
    /// and one of 63 is not; nor is text in a literal or a comment
    /// (records on PostgreSQL, F1).
    #[test]
    fn a_name_past_postgresqls_63_bytes_is_found() {
        let fits = "a".repeat(63);
        let past = "a".repeat(64);
        assert_eq!(
            long_identifier(&format!("CREATE TABLE {fits} (x TEXT)")),
            None
        );
        assert_eq!(
            long_identifier(&format!("CREATE TABLE {past} (x TEXT)")),
            Some(past.clone())
        );
        assert_eq!(
            long_identifier(&format!("CREATE INDEX \"{past}\" ON t (x)")),
            Some(past.clone())
        );
        assert_eq!(
            long_identifier(&format!(
                "CREATE TABLE t (x TEXT CHECK (x IN ('{past}', 'it''s {past}'))) -- {past}\n"
            )),
            None
        );
        assert_eq!(
            long_identifier(&format!("CREATE TABLE \"{fits}\" (x TEXT)")),
            None
        );
    }

    /// A store whose names PostgreSQL would cut is refused there, where two
    /// set attributes of a long type shared one table; SQLite keeps whole
    /// names and takes it (records on PostgreSQL, F1).
    #[test]
    fn a_store_whose_names_postgresql_would_cut_is_refused_there() {
        let text = "\nmodule: onboarding\ncategories: [live]\ntypes:\n  CustomerOnboardingComplianceReviewCase:\n    attributes:\n      supporting_documents_received: { type: \"string[]\", optional: true }\n      supporting_documents_requested: { type: \"string[]\", optional: true }\n    states:\n      OPEN: { category: live }\n    transitions:\n      open: { kind: initial, to: OPEN }\n";
        let lib = crate::model::Library::load(&[("onboarding.yaml".into(), text.into())]).unwrap();
        let made = crate::store::Store::sandbox(lib, 0);
        let postgres =
            std::env::var(crate::store::SANDBOX_POSTGRES).is_ok_and(|c| !c.trim().is_empty());
        if postgres {
            let refused = made.map(|_| ()).unwrap_err();
            assert!(refused.is_name_too_long(), "{refused}");
            assert!(
                refused
                    .to_string()
                    .contains("t_customeronboardingcompliancereviewcase__supporting_documents_re"),
                "{refused}"
            );
        } else {
            made.unwrap();
        }
    }

    /// The columns a table holds no null in, held on PostgreSQL by their
    /// key and read again once a schema changes, both inside the
    /// transaction that changed it and after it ends, by a commit, a
    /// rollback or one to a savepoint, so a NOT NULL a rollback undid is
    /// never served: narrowing would compare its column alone and miss the
    /// rows holding none (ADR-0213). On SQLite read each time.
    #[test]
    fn a_tables_shape_is_held_until_a_schema_changes() {
        let conn = match std::env::var(crate::store::SANDBOX_POSTGRES) {
            Ok(c) if !c.trim().is_empty() => {
                Connection::postgres_sandbox(&c, &sandbox_schema("shape")).unwrap()
            }
            _ => Connection::open_in_memory().unwrap(),
        };
        conn.execute_batch("CREATE TABLE t (a TEXT NOT NULL, b TEXT)")
            .unwrap();
        let reads = std::cell::Cell::new(0);
        let shape = || {
            conn.not_null_held("t 1", || {
                reads.set(reads.get() + 1);
                let mut info = conn.prepare("PRAGMA table_info(t)")?;
                let rows =
                    info.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(3)?)))?;
                let mut out = BTreeSet::new();
                for row in rows {
                    let (c, n) = row?;
                    if n != 0 {
                        out.insert(c);
                    }
                }
                Ok(out)
            })
            .unwrap()
        };
        let a = BTreeSet::from(["a".to_string()]);
        let ab = BTreeSet::from(["a".to_string(), "b".to_string()]);
        assert_eq!(shape(), a);
        assert_eq!(shape(), a);
        if conn.backend() == Backend::Sqlite {
            assert_eq!(reads.get(), 2, "SQLite's pragma is read each time");
            return;
        }
        assert_eq!(reads.get(), 1, "held on PostgreSQL");
        conn.execute_batch("BEGIN").unwrap();
        conn.execute_batch("ALTER TABLE t ALTER COLUMN b SET NOT NULL")
            .unwrap();
        assert_eq!(
            shape(),
            ab,
            "read again inside the transaction that changed it"
        );
        conn.execute_batch("ROLLBACK").unwrap();
        assert_eq!(shape(), a, "read again once a rollback undid it");
        conn.execute_batch("BEGIN; SAVEPOINT s").unwrap();
        conn.execute_batch("ALTER TABLE t ALTER COLUMN b SET NOT NULL")
            .unwrap();
        assert_eq!(shape(), ab);
        conn.execute_batch("ROLLBACK TO s").unwrap();
        assert_eq!(
            shape(),
            a,
            "read again once a savepoint's rollback undid it"
        );
        conn.execute_batch("COMMIT").unwrap();
        conn.execute_batch("ALTER TABLE t ALTER COLUMN b SET NOT NULL")
            .unwrap();
        assert_eq!(
            shape(),
            ab,
            "read again after a change outside a transaction"
        );
    }

    /// The store's SQL as PostgreSQL takes it, each rule at its case
    /// (ADR-0213).
    #[test]
    fn sqlite_sql_is_translated_for_postgresql() {
        let cases = [
            (
                "SELECT a FROM t WHERE b = ?1 AND c = ?2",
                "SELECT a FROM t WHERE b = $1 AND c = $2",
            ),
            (
                "SELECT a FROM t WHERE b = ? AND c = ?",
                "SELECT a FROM t WHERE b = $1 AND c = $2",
            ),
            ("SELECT '?1', \"?\" FROM t", "SELECT '?1', \"?\" FROM t"),
            (
                "CREATE TABLE t (n INTEGER, r REAL, b BLOB)",
                "CREATE TABLE t (n BIGINT, r DOUBLE PRECISION, b BYTEA)",
            ),
            (
                "CREATE TABLE t (\n  id                  INTEGER PRIMARY KEY,\n  n INTEGER)",
                "CREATE TABLE t (\n  id                  BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,\n  n BIGINT)",
            ),
            (
                "SELECT instr(a, 'x') FROM t",
                "SELECT strpos(a, 'x') FROM t",
            ),
            (
                "SELECT max(a, b), max(c), min(d, (e)) FROM t",
                "SELECT GREATEST(a, b), max(c), LEAST(d, (e)) FROM t",
            ),
            (
                "SELECT a FROM t WHERE b = 1 AND c IN () ORDER BY a",
                "SELECT a FROM t WHERE b = 1 AND c IN (NULL) IS TRUE ORDER BY a",
            ),
            (
                "SELECT a FROM t WHERE c NOT IN ( )",
                "SELECT a FROM t WHERE c IN (NULL) IS NOT TRUE",
            ),
            (
                "SELECT a FROM t WHERE c IN (?1)",
                "SELECT a FROM t WHERE c IN ($1)",
            ),
            (
                "INSERT OR IGNORE INTO t (a) VALUES (?1)",
                "INSERT INTO t (a) VALUES ($1) ON CONFLICT DO NOTHING",
            ),
            (
                "SELECT name FROM sqlite_master WHERE type = 'table'",
                "SELECT name FROM objectflow_catalog WHERE type = 'table'",
            ),
            (
                "x TEXT REFERENCES \"t_job\"(id) DEFERRABLE",
                "x TEXT REFERENCES of_object(id) DEFERRABLE",
            ),
            (
                "x TEXT REFERENCES of_event(position)",
                "x TEXT REFERENCES of_event(position)",
            ),
            (
                "PRAGMA table_info(\"t_job\")",
                "SELECT cid, name, type, \"notnull\", dflt_value, pk FROM objectflow_table_info('t_job')",
            ),
            ("PRAGMA foreign_keys = ON; PRAGMA synchronous = FULL;", ""),
            ("PRAGMA quick_check(1)", "SELECT 'ok'"),
            (
                "UPDATE e SET p = json_set(p, ?2, json('null')) WHERE json_type(p, ?2) IS NOT NULL",
                "UPDATE e SET p = objectflow_json_set(p, $2, objectflow_json('null')) WHERE objectflow_json_type(p, $2) IS NOT NULL",
            ),
            (
                "-- an object's id\nCREATE TABLE t (id INTEGER PRIMARY KEY)",
                "-- an object's id\nCREATE TABLE t (id BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY)",
            ),
        ];
        for (sqlite, postgres) in cases {
            assert_eq!(to_postgres(sqlite), postgres, "{sqlite}");
        }
    }

    #[test]
    fn values_bind_and_read_back_as_they_were() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE t (i INTEGER, r REAL, s TEXT, b BLOB, n TEXT)")
            .unwrap();
        c.execute(
            "INSERT INTO t VALUES (?1, ?2, ?3, ?4, ?5)",
            params![7_i64, 1.5_f64, "x", vec![1_u8, 2], None::<String>],
        )
        .unwrap();
        let got = c
            .query_row("SELECT i, r, s, b, n FROM t", [], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Vec<u8>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })
            .unwrap();
        assert_eq!(got, (7, 1.5, "x".to_string(), vec![1, 2], None));
        // a value read as a type it does not hold is a fault naming both
        let e = c
            .query_row("SELECT s FROM t", [], |r| r.get::<_, i64>(0))
            .unwrap_err();
        assert!(e.to_string().contains("text, not an integer"), "{e}");
        assert!(
            c.query_row("SELECT i FROM t WHERE i = 9", [], |r| r.get::<_, i64>(0))
                .optional()
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_transaction_commits_when_asked_and_rolls_back_otherwise() {
        let mut c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE t (i INTEGER)").unwrap();
        {
            let tx = c.transaction_with_behavior(Behavior::Immediate).unwrap();
            tx.execute("INSERT INTO t VALUES (1)", []).unwrap();
        }
        {
            let tx = c.transaction_with_behavior(Behavior::Immediate).unwrap();
            tx.execute("INSERT INTO t VALUES (2)", []).unwrap();
            tx.commit().unwrap();
        }
        let n: i64 = c
            .query_row("SELECT count(*) FROM t", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        assert!(c.is_autocommit());
    }
}

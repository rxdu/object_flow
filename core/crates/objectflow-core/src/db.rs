//! The database the store reaches, behind one interface (ADR-0213 decision
//! 2): the part of `rusqlite`'s the core uses, a connection, its statements
//! and rows, its transactions and savepoints, written in SQL so each backend
//! takes them, and the values bound and read. SQLite is its one backend
//! until PostgreSQL's lands behind it; nothing outside this module names
//! `rusqlite`, which `a_store_names_no_database_but_through_db` holds.

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
    Postgres(postgres::Error),
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
    fn sqlstate(&self) -> Option<&postgres::error::SqlState> {
        match self.fault() {
            Fault::Postgres(e) => e.code(),
            _ => None,
        }
    }

    /// Another connection held the lock past the busy timeout; on
    /// PostgreSQL a lock not had in its timeout, or a serialisable
    /// transaction the server rolled back, each retried as SQLite's busy is.
    pub fn is_busy(&self) -> bool {
        use postgres::error::SqlState;
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

    /// The role may not do what was asked, a checkpoint without
    /// `pg_checkpoint`.
    pub fn is_insufficient_privilege(&self) -> bool {
        self.sqlstate()
            .is_some_and(|c| *c == postgres::error::SqlState::INSUFFICIENT_PRIVILEGE)
    }

    /// The volume the database writes to is full.
    pub fn is_disk_full(&self) -> bool {
        if let Some(code) = self.sqlstate() {
            return *code == postgres::error::SqlState::DISK_FULL;
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

fn texty(ty: &postgres::types::Type) -> bool {
    use postgres::types::Type;
    [
        Type::TEXT,
        Type::VARCHAR,
        Type::BPCHAR,
        Type::NAME,
        Type::UNKNOWN,
    ]
    .contains(ty)
}

impl postgres::types::ToSql for PgBound<'_> {
    fn to_sql(
        &self,
        ty: &postgres::types::Type,
        out: &mut bytes::BytesMut,
    ) -> std::result::Result<postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
    {
        use postgres::types::{IsNull, Type};
        match self.0 {
            ValueRef::Null => Ok(IsNull::Yes),
            ValueRef::Integer(i) if *ty == Type::INT8 => {
                postgres::types::ToSql::to_sql(&i, ty, out)
            }
            ValueRef::Integer(i) if *ty == Type::INT4 => {
                postgres::types::ToSql::to_sql(&i32::try_from(i)?, ty, out)
            }
            ValueRef::Integer(i) if *ty == Type::INT2 => {
                postgres::types::ToSql::to_sql(&i16::try_from(i)?, ty, out)
            }
            ValueRef::Integer(i) if *ty == Type::BOOL => {
                postgres::types::ToSql::to_sql(&(i != 0), ty, out)
            }
            #[allow(clippy::cast_precision_loss)]
            ValueRef::Integer(i) if *ty == Type::FLOAT8 => {
                postgres::types::ToSql::to_sql(&(i as f64), ty, out)
            }
            ValueRef::Integer(i) if texty(ty) => {
                postgres::types::ToSql::to_sql(&i.to_string(), ty, out)
            }
            ValueRef::Real(r) if *ty == Type::FLOAT8 => postgres::types::ToSql::to_sql(&r, ty, out),
            #[allow(clippy::cast_possible_truncation)]
            ValueRef::Real(r) if *ty == Type::FLOAT4 => {
                postgres::types::ToSql::to_sql(&(r as f32), ty, out)
            }
            ValueRef::Real(r) if texty(ty) => {
                postgres::types::ToSql::to_sql(&r.to_string(), ty, out)
            }
            ValueRef::Text(t) if texty(ty) => postgres::types::ToSql::to_sql(&t, ty, out),
            ValueRef::Text(t) if *ty == Type::INT8 => {
                postgres::types::ToSql::to_sql(&t.parse::<i64>()?, ty, out)
            }
            ValueRef::Text(t) if *ty == Type::JSON || *ty == Type::JSONB => {
                serde_json::from_str::<serde_json::Value>(t)?;
                if *ty == Type::JSONB {
                    out.extend_from_slice(&[1]);
                }
                out.extend_from_slice(t.as_bytes());
                Ok(IsNull::No)
            }
            ValueRef::Blob(b) if *ty == Type::BYTEA => postgres::types::ToSql::to_sql(&b, ty, out),
            v => Err(format!("{} bound where PostgreSQL wants {ty}", v.kind()).into()),
        }
    }

    fn accepts(_: &postgres::types::Type) -> bool {
        true
    }

    postgres::types::to_sql_checked!();
}

/// A row a query read.
pub struct Row<'r> {
    inner: RowInner<'r>,
}

enum RowInner<'r> {
    Sqlite(&'r rusqlite::Row<'r>),
    Postgres(&'r postgres::Row),
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
fn pg_value<'r>(r: &'r postgres::Row, index: usize) -> Result<ValueRef<'r>> {
    use postgres::types::Type;
    let Some(column) = r.columns().get(index) else {
        return Err(Error(Fault::Conversion {
            index,
            found: "no column",
            wanted: "a column",
        }));
    };
    let ty = column.type_();
    let read = |e: postgres::Error| Error(Fault::Postgres(e));
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
    Postgres(std::vec::IntoIter<postgres::Row>),
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
    Postgres { rows: Vec<postgres::Row>, at: usize },
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
    Postgres(&'c Pg, postgres::Statement),
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
    client: std::cell::RefCell<postgres::Client>,
    prepared: std::cell::RefCell<std::collections::HashMap<String, postgres::Statement>>,
    open: std::cell::Cell<bool>,
    schema: Option<String>,
}

impl Pg {
    fn client(&self) -> Result<std::cell::RefMut<'_, postgres::Client>> {
        self.client.try_borrow_mut().map_err(|_| {
            Error(Fault::Unsupported(
                "a statement run on a PostgreSQL connection while another's rows are being read"
                    .into(),
            ))
        })
    }

    fn prepare(&self, sql: &str) -> Result<postgres::Statement> {
        let translated = to_postgres(sql);
        if let Some(s) = self.prepared.borrow().get(&translated) {
            return Ok(s.clone());
        }
        let s = self
            .client()?
            .prepare(&translated)
            .map_err(|e| Error(Fault::Postgres(e)).with_sql(&translated))?;
        self.prepared.borrow_mut().insert(translated, s.clone());
        Ok(s)
    }

    fn bind<'v>(values: &'v [ValueRef<'v>]) -> Vec<PgBound<'v>> {
        values.iter().map(|v| PgBound(*v)).collect()
    }

    fn query(
        &self,
        s: &postgres::Statement,
        values: &[ValueRef<'_>],
    ) -> Result<Vec<postgres::Row>> {
        let binds = Pg::bind(values);
        let refs: Vec<&(dyn postgres::types::ToSql + Sync)> = binds
            .iter()
            .map(|b| b as &(dyn postgres::types::ToSql + Sync))
            .collect();
        self.client()?
            .query(s, &refs)
            .map_err(|e| Error(Fault::Postgres(e)))
    }

    fn execute(&self, s: &postgres::Statement, values: &[ValueRef<'_>]) -> Result<usize> {
        let binds = Pg::bind(values);
        let refs: Vec<&(dyn postgres::types::ToSql + Sync)> = binds
            .iter()
            .map(|b| b as &(dyn postgres::types::ToSql + Sync))
            .collect();
        let n = self
            .client()?
            .execute(s, &refs)
            .map_err(|e| Error(Fault::Postgres(e)))?;
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
        }
        // a table changed inside a transaction has the keys checked that
        // were deferred to its commit, which PostgreSQL holds as pending
        // events no change of the table may pass, and defers them again
        // after; SQLite turns its keys off around the same rebuild
        let sent = if ddl && self.open.get() {
            format!("SET CONSTRAINTS ALL IMMEDIATE; {translated}; SET CONSTRAINTS ALL DEFERRED")
        } else {
            translated.clone()
        };
        self.client()?
            .batch_execute(&sent)
            .map_err(|e| Error(Fault::Postgres(e)).with_sql(&translated))?;
        // whether a transaction is open, which the server does not say: a
        // begin opens one, a commit or a rollback but to a savepoint ends it
        if head.starts_with("BEGIN") || head.starts_with("START TRANSACTION") {
            self.open.set(true);
        } else if head.starts_with("COMMIT")
            || head.starts_with("END")
            || (head.starts_with("ROLLBACK") && !head.starts_with("ROLLBACK TO"))
        {
            self.open.set(false);
        }
        Ok(())
    }
}

impl Drop for Pg {
    fn drop(&mut self) {
        if let Some(schema) = &self.schema
            && let Ok(mut c) = self.client.try_borrow_mut()
        {
            let _ = c.batch_execute(&format!("ROLLBACK; DROP SCHEMA IF EXISTS {schema} CASCADE"));
        }
    }
}

/// The catalog the store reads as SQLite keeps it, made in the schema a
/// PostgreSQL connection writes to: its tables and indexes as
/// `sqlite_master` lists them, an index that backs a constraint left out as
/// SQLite's automatic ones are, and a table's columns as `PRAGMA
/// table_info` gives them, each type named as SQLite names it.
const PG_CATALOG: &str = r#"
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

/// The schemas of sandboxes whose process no longer runs on this machine,
/// dropped: a process killed, a service's test ending it, drops none of its
/// own (ADR-0213). One made elsewhere, or by a process still running, is
/// left; one that cannot be dropped is left to the next.
fn drop_orphaned_sandboxes(client: &mut postgres::Client) {
    let Ok(rows) = client.query(
        "SELECT nspname::text FROM pg_namespace WHERE nspname LIKE 'sandbox\\_%' ESCAPE '\\'",
        &[],
    ) else {
        return;
    };
    for row in rows {
        let name: String = row.get(0);
        let pid = name.split('_').nth(1).and_then(|p| p.parse::<u32>().ok());
        if let Some(pid) = pid
            && cfg!(target_os = "linux")
            && !Path::new(&format!("/proc/{pid}")).exists()
        {
            let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {name} CASCADE"));
        }
    }
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
        Connection::postgres_in(config, None)
    }

    /// A PostgreSQL database, every table of it in a schema of its own,
    /// made for it and dropped with it: a sandbox's or a test's.
    pub fn postgres_sandbox(config: &str, schema: &str) -> Result<Connection> {
        Connection::postgres_in(config, Some(schema.to_string()))
    }

    fn postgres_in(config: &str, schema: Option<String>) -> Result<Connection> {
        let mut client = postgres::Client::connect(config, postgres::NoTls)
            .map_err(|e| Error(Fault::Postgres(e)))?;
        let collation: String = client
            .query_one(
                "SELECT datcollate FROM pg_database WHERE datname = current_database()",
                &[],
            )
            .map_err(|e| Error(Fault::Postgres(e)))?
            .get(0);
        if collation != "C" && collation != "POSIX" {
            return Err(Error(Fault::Unsupported(format!(
                "a PostgreSQL database whose text is compared by its collation {collation}: the store needs one made with `LC_COLLATE 'C'`, so text sorts as its bytes do"
            ))));
        }
        if let Some(s) = &schema {
            drop_orphaned_sandboxes(&mut client);
            client
                .batch_execute(&format!("CREATE SCHEMA {s}; SET search_path TO {s}"))
                .map_err(|e| Error(Fault::Postgres(e)))?;
        }
        client
            .batch_execute(PG_CATALOG)
            .map_err(|e| Error(Fault::Postgres(e)))?;
        Ok(Connection {
            inner: Inner::Postgres(Box::new(Pg {
                client: std::cell::RefCell::new(client),
                prepared: std::cell::RefCell::new(std::collections::HashMap::new()),
                open: std::cell::Cell::new(false),
                schema,
            })),
        })
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
                Inner::Postgres(pg) => InterruptInner::Postgres(pg.client.borrow().cancel_token()),
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
    Postgres(postgres::CancelToken),
}

impl InterruptHandle {
    pub fn interrupt(&self) {
        match &self.inner {
            InterruptInner::Sqlite(h) => h.interrupt(),
            // a cancel that fails leaves the statement to its own deadline
            InterruptInner::Postgres(t) => {
                let _ = t.cancel_query(postgres::NoTls);
            }
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

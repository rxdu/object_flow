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
            self.0,
            Fault::NoRows | Fault::Sqlite(rusqlite::Error::QueryReturnedNoRows)
        )
    }

    /// Another connection held the lock past the busy timeout.
    pub fn is_busy(&self) -> bool {
        matches!(&self.0, Fault::Sqlite(rusqlite::Error::SqliteFailure(f, _))
            if matches!(f.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked))
    }

    /// The volume the database writes to is full.
    pub fn is_disk_full(&self) -> bool {
        matches!(&self.0, Fault::Sqlite(rusqlite::Error::SqliteFailure(f, _))
            if f.code == rusqlite::ErrorCode::DiskFull)
    }

    /// The file opened is not a database.
    pub fn is_not_a_database(&self) -> bool {
        matches!(&self.0, Fault::Sqlite(rusqlite::Error::SqliteFailure(f, _))
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
        match &self.0 {
            Fault::Sqlite(e) => e.fmt(f),
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

/// A row a query read.
pub struct Row<'r> {
    sqlite: &'r rusqlite::Row<'r>,
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
        use rusqlite::types::ValueRef as R;
        let index = index.index();
        Ok(match self.sqlite.get_ref(index)? {
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
}

/// The rows of a query, read one at a time.
pub struct Rows<'s> {
    sqlite: rusqlite::Rows<'s>,
}

impl Rows<'_> {
    // each row borrows the rows it came from, which `Iterator` cannot
    // say, so it is named as `rusqlite::Rows::next` is
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Row<'_>>> {
        Ok(self.sqlite.next()?.map(|r| Row { sqlite: r }))
    }
}

/// The rows of a query, each mapped as it is read, as
/// `rusqlite::MappedRows`: a scan is never held whole.
pub struct MappedRows<'s, F> {
    rows: rusqlite::Rows<'s>,
    f: F,
}

impl<T, F> Iterator for MappedRows<'_, F>
where
    F: FnMut(&Row<'_>) -> Result<T>,
{
    type Item = Result<T>;

    fn next(&mut self) -> Option<Result<T>> {
        match self.rows.next() {
            Ok(Some(r)) => Some((self.f)(&Row { sqlite: r })),
            Ok(None) => None,
            Err(e) => Some(Err(e.into())),
        }
    }
}

/// The values bound as SQLite takes them, which it copies as it binds, so
/// they need not outlive the call.
fn bind_refs<'a>(binds: &'a [Bound<'a>]) -> Vec<&'a dyn rusqlite::ToSql> {
    binds.iter().map(|b| b as &dyn rusqlite::ToSql).collect()
}

/// A statement, prepared once and run as often as asked.
pub struct Statement<'c> {
    sqlite: rusqlite::Statement<'c>,
}

impl Statement<'_> {
    /// Each row the query reads, mapped as it is read.
    pub fn query_map<T, P, F>(&mut self, params: P, f: F) -> Result<MappedRows<'_, F>>
    where
        P: Params,
        F: FnMut(&Row<'_>) -> Result<T>,
    {
        let values = params.values();
        let binds = bound(&values);
        let rows = self.sqlite.query(bind_refs(&binds).as_slice())?;
        Ok(MappedRows { rows, f })
    }

    /// The first row the query reads, mapped; `no_rows` where it reads none.
    pub fn query_row<T, P, F>(&mut self, params: P, f: F) -> Result<T>
    where
        P: Params,
        F: FnOnce(&Row<'_>) -> Result<T>,
    {
        let values = params.values();
        let binds = bound(&values);
        let mut rows = self.sqlite.query(bind_refs(&binds).as_slice())?;
        match rows.next()? {
            Some(r) => f(&Row { sqlite: r }),
            None => Err(Error::no_rows()),
        }
    }

    /// The rows of the query, read one at a time.
    pub fn query<P: Params>(&mut self, params: P) -> Result<Rows<'_>> {
        let values = params.values();
        let binds = bound(&values);
        Ok(Rows {
            sqlite: self.sqlite.query(bind_refs(&binds).as_slice())?,
        })
    }

    /// The rows the statement changed.
    pub fn execute<P: Params>(&mut self, params: P) -> Result<usize> {
        let values = params.values();
        let binds = bound(&values);
        Ok(self.sqlite.execute(bind_refs(&binds).as_slice())?)
    }

    pub fn column_count(&self) -> usize {
        self.sqlite.column_count()
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

/// A connection to the database.
pub struct Connection {
    sqlite: rusqlite::Connection,
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Connection")
    }
}

/// How a write transaction begins: taking the write lock at once, or when
/// it first writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behavior {
    Deferred,
    Immediate,
}

impl Connection {
    /// A database in memory, a sandbox's.
    pub fn open_in_memory() -> Result<Connection> {
        Ok(Connection {
            sqlite: rusqlite::Connection::open_in_memory()?,
        })
    }

    /// A database file, made where none is.
    pub fn open(path: impl AsRef<Path>) -> Result<Connection> {
        Ok(Connection {
            sqlite: rusqlite::Connection::open(path)?,
        })
    }

    /// A database file opened as asked, never made.
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
            sqlite: rusqlite::Connection::open_with_flags(path, f)?,
        })
    }

    pub fn execute<P: Params>(&self, sql: &str, params: P) -> Result<usize> {
        self.prepare(sql)?.execute(params)
    }

    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        Ok(self.sqlite.execute_batch(sql)?)
    }

    pub fn prepare(&self, sql: &str) -> Result<Statement<'_>> {
        Ok(Statement {
            sqlite: self.sqlite.prepare(sql)?,
        })
    }

    /// A statement kept prepared for the connection's next use of it.
    pub fn prepare_cached(&self, sql: &str) -> Result<CachedStatement<'_>> {
        Ok(CachedStatement {
            sqlite: self.sqlite.prepare_cached(sql)?,
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

    fn begin(&self, behavior: Behavior) -> Result<Transaction<'_>> {
        self.execute_batch(match behavior {
            Behavior::Deferred => "BEGIN DEFERRED",
            Behavior::Immediate => "BEGIN IMMEDIATE",
        })?;
        Ok(Transaction {
            conn: self,
            done: false,
        })
    }

    /// A savepoint inside the transaction open, or a transaction of its
    /// own, released unless it rolls back.
    pub fn savepoint(&mut self) -> Result<Savepoint<'_>> {
        self.execute_batch("SAVEPOINT objectflow_savepoint")?;
        Ok(Savepoint {
            conn: self,
            done: false,
        })
    }

    /// Whether no transaction is open.
    pub fn is_autocommit(&self) -> bool {
        self.sqlite.is_autocommit()
    }

    /// How long a statement waits for another connection's lock.
    pub fn busy_timeout(&self, timeout: Duration) -> Result<()> {
        Ok(self.sqlite.busy_timeout(timeout)?)
    }

    /// That closing the connection carries no log into the file, a
    /// checked copy keeping both as they were.
    pub fn keep_log_on_close(&self) -> Result<()> {
        self.sqlite.set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )?;
        Ok(())
    }

    /// The file the connection opened, none for a database in memory.
    pub fn path(&self) -> Option<&str> {
        self.sqlite.path()
    }

    /// A handle that stops the statement running on the connection from
    /// another thread.
    pub fn interrupt_handle(&self) -> InterruptHandle {
        InterruptHandle {
            sqlite: self.sqlite.get_interrupt_handle(),
        }
    }
}

/// Stops the statement running on a connection, from another thread.
pub struct InterruptHandle {
    sqlite: rusqlite::InterruptHandle,
}

impl InterruptHandle {
    pub fn interrupt(&self) {
        self.sqlite.interrupt();
    }
}

/// A statement the connection keeps prepared.
pub struct CachedStatement<'c> {
    sqlite: rusqlite::CachedStatement<'c>,
}

impl CachedStatement<'_> {
    pub fn query_map<T, P, F>(&mut self, params: P, f: F) -> Result<MappedRows<'_, F>>
    where
        P: Params,
        F: FnMut(&Row<'_>) -> Result<T>,
    {
        let values = params.values();
        let binds = bound(&values);
        let rows = self.sqlite.query(bind_refs(&binds).as_slice())?;
        Ok(MappedRows { rows, f })
    }

    pub fn execute<P: Params>(&mut self, params: P) -> Result<usize> {
        let values = params.values();
        let binds = bound(&values);
        Ok(self.sqlite.execute(bind_refs(&binds).as_slice())?)
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

/// A savepoint: released when it commits, rolled back to otherwise.
pub struct Savepoint<'c> {
    conn: &'c Connection,
    done: bool,
}

impl Savepoint<'_> {
    pub fn commit(mut self) -> Result<()> {
        self.done = true;
        self.conn.execute_batch("RELEASE objectflow_savepoint")
    }

    pub fn rollback(&mut self) -> Result<()> {
        self.conn.execute_batch("ROLLBACK TO objectflow_savepoint")
    }

    /// Ended as it would be dropped: what it wrote undone, and released.
    pub fn finish(mut self) -> Result<()> {
        self.done = true;
        self.conn
            .execute_batch("ROLLBACK TO objectflow_savepoint; RELEASE objectflow_savepoint")
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
        }
    }
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
    CachedStatement<'c>,
    Transaction<'c>,
    Savepoint<'c>
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

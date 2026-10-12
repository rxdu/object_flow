//! The store's objects as a request reads them: a snapshot taken inside the
//! request's transaction, which the evaluator reads and the request changes
//! before writing what changed back (`DESIGN.md` §6).

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Library, Type};
use crate::store::Event;
use crate::value::Value;

/// One object, as its type's row and the directory hold it.
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub id: String,
    pub type_: String,
    pub state: String,
    pub version: i64,
    /// Its stored attributes; an absent one has no entry.
    pub attrs: BTreeMap<String, Value>,
    /// When the creation happened, in seconds since 1970.
    pub created_at: i64,
    pub created_by_kind: String,
    /// Its place in creation order, which ascending id order is (ADR-0038).
    pub order: i64,
    /// The object that supersedes it, once a `supersede` step names one.
    pub superseded_by: Option<String>,
    /// The rest of its record, beyond its row: read on first use, through
    /// [`World::parts`], and held from then on (ADR-0165).
    pub parts: std::cell::OnceCell<Parts>,
    /// Whether its creation time is known; an object the import created with
    /// none is undated, and a measure from its creation counts it as a gap.
    pub created_dated: bool,
    /// From when its record is continuous (`DESIGN.md` §5.12).
    pub recorded_from: i64,
    /// The source of the event that last changed its state, as the store
    /// holds it: `observed`, `asserted` or `imported` (`storage-schema.md`
    /// §3, `state_source`), which `exceptions` reads; none for an object the
    /// operation made, and a request's working copy does not move it.
    pub state_source: Option<String>,
}

/// The parts of an object's record beyond its row, which a read of the store
/// gives on first use (ADR-0165).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parts {
    /// Its flow data: for the state and each tracked member, the spans it
    /// held each value for, in order (`storage-schema.md`, `of_interval`).
    pub spans: BTreeMap<String, Vec<Span>>,
    /// The attribute write index: for each attribute, part relationship or
    /// collection, the position of the event that last wrote it
    /// (`storage-schema.md`, `of_attribute_write`).
    pub writes: BTreeMap<String, i64>,
    /// The invariants whose violation an admission lets stand on it, until
    /// they hold again (ADR-0054, `of_admission`).
    pub admissions: BTreeSet<String>,
    /// Its own erasure is recorded, so a later write of a personal attribute
    /// is refused (`DESIGN.md` §8).
    pub erased: bool,
}

impl Parts {
    /// Parts already held, as an object an operation makes holds them.
    pub fn held(self) -> std::cell::OnceCell<Parts> {
        std::cell::OnceCell::from(self)
    }
}

/// How long an object held a state, or a value of a tracked member, as one
/// row of the interval index holds it (ADR-0083, ADR-0101).
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    /// The state, or the member's value; absent where it had none.
    pub value: Value,
    /// The event that set it, and the one that ended it, while it is not
    /// the current value.
    pub entered_position: i64,
    pub left_position: Option<i64>,
    pub entered_at: i64,
    pub left_at: Option<i64>,
    pub entered_by: Option<String>,
    pub entered_by_kind: String,
    /// Opened by a publish's migration, for a member newly tracked
    /// (ADR-0146, `of_interval.migrated`).
    pub migrated: bool,
    /// The version it was entered under: the one installed when the change
    /// was recorded, and a publish's new one for a span the publish opened
    /// (`declaration-syntax.md` §6.9, `of_interval.declaration_version`).
    pub declaration_version: i64,
    /// Supplied by the import's mapping: an earlier value the legacy record
    /// gives, at position 0 (ADR-0086, `of_interval.legacy`).
    pub legacy: bool,
    /// The event of the latest correction that changed its value or its
    /// `.held` (`declaration-syntax.md` §6.4, ADR-0147, `of_interval.corrected_by`).
    pub corrected_by: Option<i64>,
}

impl Object {
    pub fn attr(&self, name: &str) -> Value {
        self.attrs.get(name).cloned().unwrap_or(Value::Absent)
    }

    /// The parts of its record it holds already, for code given an object
    /// whose record was read, or made with it: an error where it was not,
    /// never parts empty in its place (ADR-0165).
    pub fn held_parts(&self) -> Result<&Parts, crate::eval::NotRun> {
        self.parts.get().ok_or_else(|| {
            crate::eval::NotRun::Unsupported(format!(
                "the record of {}, which was not read",
                self.id
            ))
        })
    }

    /// The parts of its record it holds already, to change: an error where
    /// they have not been read, never parts empty in their place (ADR-0165).
    pub fn parts_mut(&mut self) -> Result<&mut Parts, crate::eval::NotRun> {
        let id = self.id.clone();
        self.parts.get_mut().ok_or_else(|| {
            crate::eval::NotRun::Unsupported(format!(
                "a change to the record of {id}, whose record was not read"
            ))
        })
    }
}

/// A legacy entry attached to an object at import (ADR-0015, ADR-0163): its
/// ordinal, kind and occurred time, its fields, and the fields its kind keeps
/// through an erasure, as it was ported.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyEntry {
    pub ordinal: i64,
    pub kind: String,
    pub occurred_at: i64,
    pub payload: serde_json::Map<String, serde_json::Value>,
    pub kept: Vec<String>,
}

impl LegacyEntry {
    /// Its fields as an erasure leaves them: those its kind keeps, every
    /// other replaced by absence (`storage-schema.md` §9 step 6, ADR-0078).
    pub fn redacted(&self) -> serde_json::Map<String, serde_json::Value> {
        self.payload
            .iter()
            .map(|(k, v)| {
                let v = if self.kept.contains(k) {
                    v.clone()
                } else {
                    serde_json::Value::Null
                };
                (k.clone(), v)
            })
            .collect()
    }
}

/// A row of the attempt rollup: the counted fields, and the count.
pub type RolledUp = (BTreeMap<String, crate::value::Value>, i64);

/// External identifiers, by source and value, and the object each names.
pub type ExternalIds = BTreeMap<(String, String), String>;

/// The number of chunks [`Slots`] has, the k-th holding 2^k slots.
const CHUNKS: usize = 48;

/// Slots that only grow, each written once: the k-th chunk holds 2^k of
/// them, so the n-th is found in constant time, and one written never
/// moves, so a read through a shared reference can add an object and hand
/// out references to it that live as long as the world.
/// A chunk of [`Slots`], each slot written at most once.
type Chunk<T> = Box<[std::cell::OnceCell<Box<T>>]>;

#[derive(Clone, Debug)]
struct Slots<T> {
    chunks: [std::cell::OnceCell<Chunk<T>>; CHUNKS],
    len: std::cell::Cell<usize>,
}

impl<T> Default for Slots<T> {
    fn default() -> Self {
        Slots {
            chunks: std::array::from_fn(|_| std::cell::OnceCell::new()),
            len: std::cell::Cell::new(0),
        }
    }
}

impl<T> Slots<T> {
    /// The chunk the n-th slot is in, and its place there.
    fn place(n: usize) -> Option<(usize, usize)> {
        let one = n.checked_add(1)?;
        let k = one.ilog2();
        Some((usize::try_from(k).ok()?, one - 1usize.checked_shl(k)?))
    }

    /// Every value written, taken out, in the order of their slots.
    fn into_values(self) -> Vec<T> {
        self.chunks
            .into_iter()
            .filter_map(std::cell::OnceCell::into_inner)
            .flat_map(|chunk| chunk.into_vec())
            .filter_map(std::cell::OnceCell::into_inner)
            .map(|b| *b)
            .collect()
    }

    /// A slot written with the value given, and its number; none past the
    /// last chunk.
    fn push(&self, value: T) -> Option<usize> {
        let n = self.len.get();
        let (k, at) = Self::place(n)?;
        let size = 1usize.checked_shl(u32::try_from(k).ok()?)?;
        let chunk = self
            .chunks
            .get(k)?
            .get_or_init(|| (0..size).map(|_| std::cell::OnceCell::new()).collect());
        chunk.get(at)?.set(Box::new(value)).ok()?;
        self.len.set(n + 1);
        Some(n)
    }

    fn get(&self, n: usize) -> Option<&T> {
        let (k, at) = Self::place(n)?;
        self.chunks.get(k)?.get()?.get(at)?.get().map(|b| &**b)
    }

    fn get_mut(&mut self, n: usize) -> Option<&mut T> {
        let (k, at) = Self::place(n)?;
        self.chunks
            .get_mut(k)?
            .get_mut()?
            .get_mut(at)?
            .get_mut()
            .map(|b| &mut **b)
    }

    fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        (0..self.len.get()).filter_map(|n| self.get(n))
    }
}

/// The objects an operation reads and makes: each fetched from the store
/// on its first read, alone by id or with its type for a scan, and held for
/// the operation, which changes and adds to them (ADR-0162 step 2).
#[derive(Clone, Debug, Default)]
struct Held {
    slots: Slots<Object>,
    /// Each id asked for, and its slot; none where the store holds no such
    /// object.
    by_id: std::cell::RefCell<BTreeMap<String, Option<usize>>>,
    /// The types whose every object in the store is held.
    whole: std::cell::RefCell<BTreeSet<String>>,
    /// The highest place in creation order the store holds, read on first
    /// use.
    stored_max: std::cell::OnceCell<Result<i64, String>>,
    /// The objects the operation changed or made, by id.
    written: BTreeSet<String>,
    /// The records read alone, by type, past [`BATCH`] of which the rest
    /// of a type's are read together.
    alone: std::cell::RefCell<BTreeMap<String, usize>>,
    /// The held objects by a column's value, each column built on its
    /// first use and kept as objects are held and changed (PERF-SCAN).
    index: std::cell::RefCell<BTreeMap<(String, String), Column>>,
    /// Each slot changed or replaced since the operation began, in order,
    /// which a column keys again from where it last read.
    changed: std::cell::RefCell<Vec<usize>>,
    /// How many slots the columns have keyed, which a test holds linear in
    /// what is held.
    keyed: std::cell::Cell<usize>,
}

/// The held objects of one type by one attribute's value, a key of each
/// value an index can hold and `None` for an absent one (PERF-SCAN): a scan
/// of every held object for each object read, as a dry run's invariants
/// made over a whole store, is one lookup. It is kept, not rebuilt: a
/// lookup keys the slots held since it last looked and those changed
/// since, so its cost is what changed, where rebuilding it on any change
/// made an import's checks, each holding an object more, quadratic again
/// (IMPORT-GROWTH).
#[derive(Clone, Debug, Default)]
struct Column {
    /// How many slots it has keyed, and how far into the changed slots it
    /// has read.
    slots: usize,
    changes: usize,
    by: BTreeMap<Option<String>, BTreeSet<usize>>,
    /// Each slot's key, so a changed slot's old one is taken out.
    key_of: BTreeMap<usize, Option<String>>,
}

impl Column {
    /// A slot keyed as the object it holds now is: taken out under its old
    /// key, and put under its new one where it is of the column's type and
    /// its value is one an index holds.
    fn key(&mut self, n: usize, o: Option<&Object>, type_: &str, attr: &str) {
        if let Some(old) = self.key_of.remove(&n)
            && let Some(slots) = self.by.get_mut(&old)
        {
            slots.remove(&n);
        }
        if let Some(o) = o
            && o.type_ == type_
            && let Some(k) = index_key(&o.attr(attr))
        {
            self.by.entry(k.clone()).or_default().insert(n);
            self.key_of.insert(n, k);
        }
    }
}

/// The key an index holds a value by, where it holds one: an object's id,
/// a text or an integer.
fn index_key(v: &Value) -> Option<Option<String>> {
    match v {
        Value::Absent => Some(None),
        Value::Object(id) => Some(Some(format!("o:{id}"))),
        Value::Text(t) => Some(Some(format!("t:{t}"))),
        Value::Int(n) => Some(Some(format!("i:{n}"))),
        _ => None,
    }
}

/// How many of a type's records an operation reads one by one before it
/// reads the rest it holds together (ADR-0165).
pub(crate) const BATCH: usize = 8;

/// No earlier version's library: a store with one version, or a world
/// built without them.
static NO_EARLIER: BTreeMap<i64, Library> = BTreeMap::new();

/// The objects of a store, and the time a request reads.
#[derive(Clone, Debug)]
pub struct World<'l> {
    pub lib: &'l Library,
    /// Each earlier version's library, by version, which an erasure reads
    /// what was personal under (ADR-0146 decision 2); empty where the store
    /// has published one version.
    pub earlier: &'l BTreeMap<i64, Library>,
    held: Held,
    pub now: i64,
    /// Every external identifier, by source and value, and the object that
    /// holds it, a legacy key among them (`storage-schema.md`,
    /// `of_external_id`), read whole on first use by an operation that reads
    /// them all, as an import does.
    stored_external: std::cell::OnceCell<Result<ExternalIds, String>>,
    /// The events the operation appended, in position order, after the log
    /// the store holds, which [`World::log`] reads with them.
    pub appended: Vec<Event>,
    /// The log the store holds, read on first use and kept for the
    /// operation, so one that never reads it never loads it (ADR-0162 step
    /// 1); an error where it could not be read.
    stored: std::cell::OnceCell<Result<Vec<Event>, String>>,
    /// The store's connection, which the log and the objects are read
    /// through; none for a world built without a store.
    conn: Option<&'l rusqlite::Connection>,
    /// The position of each event that recorded an admission, which is an
    /// override (`declaration-syntax.md` §6.9, `of_admission`).
    pub admitting: BTreeSet<i64>,
    /// The position of each event the import recorded, whose spans may begin
    /// before it, at the legacy time of the change (ADR-0106), read on first
    /// use.
    stored_imported: std::cell::OnceCell<Result<BTreeSet<i64>, String>>,
    /// The attempt log, in the order written, which a metric over
    /// `attempts` or `attempt_counts` reads (`storage-schema.md` §6), read
    /// on first use (ADR-0162 step 1).
    stored_attempts: std::cell::OnceCell<Result<Vec<crate::store::Attempt>, String>>,
    /// Each subscription's acknowledged cursor, where one was acknowledged:
    /// runtime state, not versioned (`DESIGN.md` §7).
    pub acknowledged: BTreeMap<String, i64>,
    /// The daily counts of the attempt rows `maintain` pruned, each a row of
    /// the `attempt_counts` source as the rows it counted were
    /// (`storage-schema.md` §6, `of_attempt_rollup`), read on first use.
    stored_rollup: std::cell::OnceCell<Result<Vec<RolledUp>, String>>,
    /// When a read must have answered by, which its evaluation stops at;
    /// none for a write, whose rules the flow bounds, or a sandbox's read
    /// (ADR-0184 decision 3). Read every [`TICKS`] steps.
    pub deadline: Option<std::time::Instant>,
    ticks: std::cell::Cell<u32>,
}

/// How many steps of a read pass between two readings of the clock against
/// its deadline.
const TICKS: u32 = 256;

impl<'l> World<'l> {
    pub fn new(lib: &'l Library, now: i64) -> World<'l> {
        World {
            lib,
            earlier: &NO_EARLIER,
            held: Held::default(),
            now,
            stored_external: std::cell::OnceCell::new(),
            appended: Vec::new(),
            stored: std::cell::OnceCell::new(),
            conn: None,
            admitting: BTreeSet::new(),
            stored_imported: std::cell::OnceCell::new(),
            stored_attempts: std::cell::OnceCell::new(),
            acknowledged: BTreeMap::new(),
            stored_rollup: std::cell::OnceCell::new(),
            deadline: None,
            ticks: std::cell::Cell::new(0),
        }
    }

    /// Whether the read is within its deadline, where it has one: a filter,
    /// a metric or a sweep with no bound held a reader, and an erasure's
    /// truncation behind it, for minutes (ADR-0184 decision 3).
    pub fn on_time(&self) -> Result<(), crate::eval::NotRun> {
        let Some(at) = self.deadline else {
            return Ok(());
        };
        let n = self.ticks.get().wrapping_add(1);
        self.ticks.set(n);
        if n.is_multiple_of(TICKS) && std::time::Instant::now() > at {
            return Err(crate::eval::NotRun::TimedOut(
                "the read passed its deadline; narrow it, a filter by an indexed member or a page"
                    .into(),
            ));
        }
        Ok(())
    }

    /// A type as the version given declared it: the installed version's,
    /// or an earlier one's where the store holds it.
    pub fn type_at(&self, version: i64, name: &str) -> Option<&'l Type> {
        match self.earlier.get(&version) {
            Some(lib) if version != self.lib.version => lib.type_(name),
            _ => self.lib.type_(name),
        }
    }

    /// Whether an attribute of a type, named as the installed version names
    /// it, was personal under the version given (ADR-0146 decision 2): its
    /// name taken back through each later version's renames first, so a
    /// value written before a rename is judged as the attribute it was.
    pub fn personal_under(&self, version: i64, type_: &str, attribute: &str) -> bool {
        let mut name = attribute.to_string();
        for k in (version + 1..=self.lib.version).rev() {
            let lib = if k == self.lib.version {
                Some(self.lib)
            } else {
                self.earlier.get(&k)
            };
            if let Some((_, from, _)) = lib.and_then(|l| {
                l.migration
                    .renamed_attributes
                    .iter()
                    .find(|(t, _, to)| t == type_ && *to == name)
            }) {
                name.clone_from(from);
            }
        }
        self.type_at(version, type_)
            .and_then(|t| t.attribute(&name))
            .is_some_and(|a| a.personal)
    }

    /// A world that reads its log and its objects through the connection
    /// given.
    pub(crate) fn reading_from(mut self, conn: &'l rusqlite::Connection) -> World<'l> {
        self.conn = Some(conn);
        self
    }

    /// The log, in position order: every event the store holds, read on first
    /// use, then those the operation appended (`storage-schema.md`, `of_event`,
    /// ADR-0162). A log that cannot be read stops the operation as not run,
    /// naming why, never as an empty log.
    pub fn log(&self) -> Result<impl DoubleEndedIterator<Item = &Event> + '_, crate::eval::NotRun> {
        let stored = self.stored.get_or_init(|| match self.conn {
            Some(conn) => crate::store::read_log(conn).map_err(|e| e.to_string()),
            None => Ok(Vec::new()),
        });
        match stored {
            Ok(events) => Ok(events.iter().chain(self.appended.iter())),
            Err(e) => Err(crate::eval::NotRun::Unreadable(format!(
                "the store's log, which could not be read: {e}"
            ))),
        }
    }

    /// The attempt log, in the order written, read on first use; a log that
    /// cannot be read stops the operation as not run, naming why.
    pub fn attempts(&self) -> Result<&[crate::store::Attempt], crate::eval::NotRun> {
        let stored = self.stored_attempts.get_or_init(|| match self.conn {
            Some(conn) => crate::store::read_attempts(conn).map_err(|e| e.to_string()),
            None => Ok(Vec::new()),
        });
        stored.as_deref().map_err(|e| {
            crate::eval::NotRun::Unreadable(format!(
                "the store's attempt log, which could not be read: {e}"
            ))
        })
    }

    /// The pruned attempt rows' daily counts, read on first use.
    pub fn rollup(&self) -> Result<&[RolledUp], crate::eval::NotRun> {
        let stored = self.stored_rollup.get_or_init(|| match self.conn {
            Some(conn) => crate::store::read_rollup(conn).map_err(|e| e.to_string()),
            None => Ok(Vec::new()),
        });
        stored.as_deref().map_err(|e| {
            crate::eval::NotRun::Unreadable(format!(
                "the store's attempt rollup, which could not be read: {e}"
            ))
        })
    }

    /// Every external identifier the store holds, by source and value, read
    /// whole on first use, as a batch import reads them.
    pub fn external_all(&self) -> Result<&BTreeMap<(String, String), String>, crate::eval::NotRun> {
        let stored = self.stored_external.get_or_init(|| match self.conn {
            Some(conn) => crate::store::read_external_all(conn).map_err(|e| e.to_string()),
            None => Ok(BTreeMap::new()),
        });
        stored.as_ref().map_err(|e| {
            crate::eval::NotRun::Unreadable(format!(
                "the store's external identifiers, which could not be read: {e}"
            ))
        })
    }

    /// The object an external identifier names, read by its key (ADR-0162
    /// step 1), or from every identifier where an operation has read them.
    pub fn external_id(
        &self,
        source: &str,
        value: &str,
    ) -> Result<Option<String>, crate::eval::NotRun> {
        if let Some(Ok(all)) = self.stored_external.get() {
            return Ok(all.get(&(source.to_string(), value.to_string())).cloned());
        }
        match self.conn {
            Some(conn) => crate::store::read_external(conn, source, value).map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "the store's external identifier {source} {value}, which could not be read: {e}"
                ))
            }),
            None => Ok(None),
        }
    }

    /// An object's legacy entries, by ordinal, read by its id.
    pub fn legacy_of(&self, id: &str) -> Result<Vec<LegacyEntry>, crate::eval::NotRun> {
        match self.conn {
            Some(conn) => crate::store::read_legacy(conn, id).map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "{id}'s legacy entries, which could not be read: {e}"
                ))
            }),
            None => Ok(Vec::new()),
        }
    }

    /// The position of each event the import recorded, read on first use.
    pub fn imported(&self) -> Result<&BTreeSet<i64>, crate::eval::NotRun> {
        let stored = self.stored_imported.get_or_init(|| match self.conn {
            Some(conn) => crate::store::read_imported(conn).map_err(|e| e.to_string()),
            None => Ok(BTreeSet::new()),
        });
        stored.as_ref().map_err(|e| {
            crate::eval::NotRun::Unreadable(format!(
                "the store's imported events, which could not be read: {e}"
            ))
        })
    }

    /// What a key an actor applied recorded, the request's digest and its
    /// verdict, read by the key alone (ADR-0162 step 1).
    pub fn idempotency_of(
        &self,
        actor: &str,
        key: &str,
    ) -> Result<Option<(String, serde_json::Value)>, crate::eval::NotRun> {
        match self.conn {
            Some(conn) => crate::store::read_idempotency(conn, actor, key).map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "the store's idempotency records, which could not be read: {e}"
                ))
            }),
            None => Ok(None),
        }
    }

    /// An object, fetched from the store on its first read and held for
    /// the operation; none where neither holds it. A store that cannot be
    /// read stops the operation as not run, naming why, never as an object
    /// that is not there.
    pub fn get(&self, id: &str) -> Result<Option<&Object>, crate::eval::NotRun> {
        self.on_time()?;
        let known = self.held.by_id.borrow().get(id).copied();
        if let Some(slot) = known {
            return Ok(slot.and_then(|n| self.held.slots.get(n)));
        }
        let fetched = match self.conn {
            Some(conn) => crate::store::read_object(conn, self.lib, id).map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "the store's object {id}, which could not be read: {e}"
                ))
            })?,
            None => None,
        };
        let slot = match fetched {
            Some(o) => Some(self.hold(o)?),
            None => None,
        };
        self.held.by_id.borrow_mut().insert(id.to_string(), slot);
        Ok(slot.and_then(|n| self.held.slots.get(n)))
    }

    /// The object an id names where one is given, as [`World::get`] reads
    /// it; none where none is.
    pub fn get_if(&self, id: Option<&str>) -> Result<Option<&Object>, crate::eval::NotRun> {
        match id {
            Some(id) => self.get(id),
            None => Ok(None),
        }
    }

    /// An object held in a new slot, which is not yet known by its id.
    fn hold(&self, o: Object) -> Result<usize, crate::eval::NotRun> {
        self.held.slots.push(o).ok_or_else(|| {
            crate::eval::NotRun::Unsupported("more objects than an operation can hold".into())
        })
    }

    /// Every object of a concrete type the store holds, fetched in one read
    /// and held, each held already, as the operation may have changed it,
    /// kept as it is.
    fn hold_type(&self, type_: &str) -> Result<(), crate::eval::NotRun> {
        if self.held.whole.borrow().contains(type_) {
            return Ok(());
        }
        if let Some(conn) = self.conn {
            let fetched = crate::store::read_type(conn, self.lib, type_).map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "the store's objects of {type_}, which could not be read: {e}"
                ))
            })?;
            self.hold_fetched(fetched)?;
        }
        self.held.whole.borrow_mut().insert(type_.to_string());
        Ok(())
    }

    /// Objects read from the store, held, each held already, as the
    /// operation may have changed it, kept as it is.
    fn hold_fetched(&self, fetched: Vec<Object>) -> Result<(), crate::eval::NotRun> {
        for o in fetched {
            if matches!(self.held.by_id.borrow().get(&o.id), Some(Some(_))) {
                continue;
            }
            let id = o.id.clone();
            let slot = self.hold(o)?;
            self.held.by_id.borrow_mut().insert(id, Some(slot));
        }
        Ok(())
    }

    /// Every object held, in ascending id order: creation order.
    /// The held objects of a type, its own or a subtype's, whose attribute
    /// holds the value given or none, by the index, in ascending id order;
    /// none where the value is one the index holds no key for.
    fn held_where(&self, type_: &str, attr: &str, v: &Value) -> Option<Vec<&Object>> {
        let key = index_key(v)?.map(Some)?;
        let mut idx = self.held.index.borrow_mut();
        let changed = self.held.changed.borrow();
        let slots = self.held.slots.len.get();
        let mut found: BTreeSet<usize> = BTreeSet::new();
        for t in self.lib.types.keys().filter(|t| self.lib.is_a(t, type_)) {
            let column = idx.entry((t.clone(), attr.to_string())).or_default();
            // the slots held since it last looked, then those changed since
            let fresh = changed.get(column.changes..).unwrap_or_default();
            self.held
                .keyed
                .set(self.held.keyed.get() + (slots - column.slots) + fresh.len());
            for n in column.slots..slots {
                column.key(n, self.held.slots.get(n), t, attr);
            }
            for &n in fresh {
                column.key(n, self.held.slots.get(n), t, attr);
            }
            (column.slots, column.changes) = (slots, changed.len());
            for k in [key.clone(), None] {
                if let Some(ns) = column.by.get(&k) {
                    found.extend(ns);
                }
            }
        }
        let mut out: Vec<&Object> = found
            .into_iter()
            .filter_map(|n| self.held.slots.get(n))
            .collect();
        out.sort_by_key(|o| o.order);
        Some(out)
    }

    /// Every object the world holds, taken out of it, never copied.
    pub fn into_objects(self) -> Vec<Object> {
        self.held.slots.into_values()
    }

    /// How many objects the operation holds, which a read of one object's
    /// parts keeps to its parts rather than its parts' whole type
    /// (PERF-SCAN).
    pub fn held_count(&self) -> usize {
        self.held.slots.len.get()
    }

    /// How many slots the operation's column indexes have keyed: linear in
    /// what it holds, where an index rebuilt whenever an object was held
    /// made a sweep of a store quadratic (READ-COST).
    pub fn keyed_count(&self) -> usize {
        self.held.keyed.get()
    }

    fn held_in_order(&self, of: impl Fn(&Object) -> bool) -> Vec<&Object> {
        let mut all: Vec<&Object> = self.held.slots.iter().filter(|o| of(o)).collect();
        all.sort_by_key(|o| o.order);
        all
    }

    /// Every object, in ascending id order: creation order. It reads the
    /// whole store, in one read of each table, where any type is not yet
    /// held whole.
    pub fn in_order(&self) -> Result<Vec<&Object>, crate::eval::NotRun> {
        let whole = {
            let held = self.held.whole.borrow();
            self.lib.types.keys().all(|t| held.contains(t))
        };
        if !whole {
            if let Some(conn) = self.conn {
                let fetched = crate::store::read_all(conn, self.lib).map_err(|e| {
                    crate::eval::NotRun::Unreadable(format!(
                        "the store's objects, which could not be read: {e}"
                    ))
                })?;
                self.hold_fetched(fetched)?;
            }
            self.held
                .whole
                .borrow_mut()
                .extend(self.lib.types.keys().cloned());
        }
        Ok(self.held_in_order(|_| true))
    }

    /// Every object of a type, its own or a subtype's, in ascending id
    /// order, each type read in one fetch.
    pub fn of_type(&self, type_: &str) -> Result<Vec<&Object>, crate::eval::NotRun> {
        for t in self.lib.types.keys() {
            if self.lib.is_a(t, type_) {
                self.hold_type(t)?;
            }
        }
        Ok(self.held_in_order(|o| self.lib.is_a(&o.type_, type_)))
    }

    /// The ids of the observations of a kind, or of its subtypes, that one
    /// of them corrects: read by the store's index on the stored end, and
    /// those the operation holds, never the kind whole.
    pub fn corrected(&self, kind: &str) -> Result<BTreeSet<String>, crate::eval::NotRun> {
        let mut out = BTreeSet::new();
        if let Some(conn) = self.conn {
            for t in self.lib.types.keys().filter(|t| self.lib.is_a(t, kind)) {
                out.extend(
                    crate::store::read_corrected(conn, self.lib, t).map_err(|e| {
                        crate::eval::NotRun::Unreadable(format!(
                            "the corrections of {t}, which could not be read: {e}"
                        ))
                    })?,
                );
            }
        }
        for o in self.held.slots.iter() {
            if self.lib.is_a(&o.type_, kind)
                && let crate::value::Value::Object(id) = o.attr("corrects")
            {
                out.insert(id);
            }
        }
        Ok(out)
    }

    /// The objects of a type, its own or a subtype's, whose state an
    /// assertion last set or that hold a standing admission, in ascending id
    /// order: read by the store's indexes, never the type whole, what the
    /// operation holds kept over what the store says (`DESIGN.md` §10,
    /// `exceptions`).
    pub fn excepted(&self, type_: &str) -> Result<Vec<&Object>, crate::eval::NotRun> {
        let family: Vec<&str> = self
            .lib
            .types
            .keys()
            .filter(|t| self.lib.is_a(t, type_))
            .map(String::as_str)
            .collect();
        let (asserted, admitted) = match self.conn {
            Some(conn) => crate::store::read_excepted(conn, &family).map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "the exceptions of {type_}, which could not be read: {e}"
                ))
            })?,
            None => Default::default(),
        };
        // the store's answer, and every object held whose record the
        // operation read, its admissions perhaps changed since; an object
        // held without its record is as the store holds it
        let mut ids: BTreeSet<String> = asserted.union(&admitted).cloned().collect();
        ids.extend(
            self.held
                .slots
                .iter()
                .filter(|o| self.lib.is_a(&o.type_, type_) && o.parts.get().is_some())
                .map(|o| o.id.clone()),
        );
        let mut out = Vec::new();
        for id in &ids {
            let Some(o) = self.get(id)? else {
                continue;
            };
            let holds = match o.parts.get() {
                Some(p) => !p.admissions.is_empty(),
                None => admitted.contains(id),
            };
            if o.state_source.as_deref() == Some("asserted") || holds {
                out.push(o);
            }
        }
        out.sort_by_key(|o| o.order);
        Ok(out)
    }

    /// The parts of an object's record beyond its row, read from the store
    /// on first use and held from then on; a world without a store holds
    /// what it was given, and an object it was given without them is an
    /// error, never an object with no history (ADR-0165).
    pub fn parts<'a>(&self, o: &'a Object) -> Result<&'a Parts, crate::eval::NotRun> {
        if let Some(p) = o.parts.get() {
            return Ok(p);
        }
        let Some(conn) = self.conn else {
            return Err(crate::eval::NotRun::Unsupported(format!(
                "the record of {}, which a world without a store was given without it",
                o.id
            )));
        };
        // an operation reaching record after record of a type, as a scan's
        // filter does, reads the rest it holds of the type together
        let alone = {
            let mut alone = self.held.alone.borrow_mut();
            let n = alone.entry(o.type_.clone()).or_default();
            *n += 1;
            *n
        };
        if alone > BATCH {
            let lacking: Vec<&Object> = self
                .held
                .slots
                .iter()
                .filter(|x| x.type_ == o.type_ && x.parts.get().is_none())
                .collect();
            if lacking.len() > BATCH {
                self.read_together(conn, &o.type_, &lacking)?;
            }
            if let Some(p) = o.parts.get() {
                return Ok(p);
            }
        }
        let read = crate::store::read_parts_of(conn, self.lib, &o.id, &o.type_).map_err(|e| {
            crate::eval::NotRun::Unreadable(format!(
                "the record of {}, which could not be read: {e}",
                o.id
            ))
        })?;
        Ok(o.parts.get_or_init(|| read))
    }

    /// The parts of the records of the objects given, read in one read of
    /// each part's table a type, where many of a type lack them, and else
    /// each alone (ADR-0165).
    pub fn with_parts(&self, objects: &[&Object]) -> Result<(), crate::eval::NotRun> {
        let mut lacking: BTreeMap<&str, Vec<&Object>> = BTreeMap::new();
        for o in objects {
            if o.parts.get().is_none() {
                lacking.entry(o.type_.as_str()).or_default().push(o);
            }
        }
        for (t, os) in lacking {
            match self.conn {
                Some(conn) if os.len() > BATCH => self.read_together(conn, t, &os)?,
                _ => {
                    for o in os {
                        self.parts(o)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// The records of objects of one concrete type, in one read of each
    /// part's table.
    fn read_together(
        &self,
        conn: &rusqlite::Connection,
        type_: &str,
        objects: &[&Object],
    ) -> Result<(), crate::eval::NotRun> {
        let mut read = crate::store::read_type_parts(conn, self.lib, type_).map_err(|e| {
            crate::eval::NotRun::Unreadable(format!(
                "the records of {type_}, which could not be read: {e}"
            ))
        })?;
        for o in objects {
            let p = read.remove(&o.id).unwrap_or_default();
            let _ = o.parts.set(p);
        }
        Ok(())
    }

    /// Every object of a type, its own or a subtype's, in one of the states
    /// given, in ascending id order, as [`World::of_type_where`] reads them.
    pub fn of_type_in(
        &self,
        type_: &str,
        states: &BTreeSet<String>,
    ) -> Result<Vec<&Object>, crate::eval::NotRun> {
        self.of_type_where(type_, Some(states), &[])
    }

    /// Every object of a type, its own or a subtype's, in one of the states
    /// given, where any are, and whose attributes named each hold the value
    /// given or none, an absent value comparing as unknown, in ascending id
    /// order: a type held whole is read from what is held, and any other
    /// through its state index and its columns, read whole only where a value
    /// has none to compare in, the objects the operation holds kept as it
    /// holds them (ADR-0162 step 3).
    pub fn of_type_where(
        &self,
        type_: &str,
        states: Option<&BTreeSet<String>>,
        columns: &[(String, Value)],
    ) -> Result<Vec<&Object>, crate::eval::NotRun> {
        let wanted: Option<Vec<String>> = states.map(|s| s.iter().cloned().collect());
        let pairs: Vec<(&str, &Value)> = columns.iter().map(|(a, v)| (a.as_str(), v)).collect();
        for t in self.lib.types.keys() {
            let whole = self.held.whole.borrow().contains(t);
            if self.lib.is_a(t, type_)
                && !whole
                && let Some(conn) = self.conn
            {
                let fetched =
                    crate::store::read_type_where(conn, self.lib, t, wanted.as_deref(), &pairs)
                        .map_err(|e| {
                            crate::eval::NotRun::Unreadable(format!(
                                "the store's objects of {t} narrowed, which could not be read: {e}"
                            ))
                        })?;
                match fetched {
                    Some(fetched) => self.hold_fetched(fetched)?,
                    None => self.hold_type(t)?,
                }
            }
        }
        let holds = |o: &Object| {
            self.lib.is_a(&o.type_, type_)
                && states.is_none_or(|s| s.contains(&o.state))
                && pairs.iter().all(|(a, v)| {
                    let x = o.attr(a);
                    x.is_absent() || x.equals(v)
                })
        };
        // the held objects by the index on a pair's column, where its value
        // is one the index holds, never every held object (PERF-SCAN)
        if let Some(found) = pairs.iter().find_map(|(a, v)| self.held_where(type_, a, v)) {
            return Ok(found.into_iter().filter(|o| holds(o)).collect());
        }
        Ok(self.held_in_order(holds))
    }

    /// Every object of a type, its own or a subtype's, whose attributes
    /// named hold values equal to those given, in ascending id order: a type
    /// held whole is read from what is held, any other by its columns, read
    /// whole only where a value has none to compare in, the objects the
    /// operation holds kept as it holds them (ADR-0162 step 3).
    pub fn with_key(
        &self,
        type_: &str,
        attrs: &[String],
        key: &[Value],
    ) -> Result<Vec<&Object>, crate::eval::NotRun> {
        let pairs: Vec<(&str, &Value)> = attrs.iter().map(String::as_str).zip(key).collect();
        for t in self.lib.types.keys() {
            let whole = self.held.whole.borrow().contains(t);
            if self.lib.is_a(t, type_)
                && !whole
                && let Some(conn) = self.conn
            {
                let ids = crate::store::read_ids_with(conn, self.lib, t, &pairs).map_err(|e| {
                    crate::eval::NotRun::Unreadable(format!(
                        "the store's objects of {t} by {attrs:?}, which could not be read: {e}"
                    ))
                })?;
                match ids {
                    Some(ids) => {
                        for id in ids {
                            self.get(&id)?;
                        }
                    }
                    None => self.hold_type(t)?,
                }
            }
        }
        let holds = |o: &Object| {
            self.lib.is_a(&o.type_, type_) && pairs.iter().all(|(a, v)| o.attr(a).equals(v))
        };
        // the held objects by the index on a pair's column, where its value
        // is one the index holds, never every held object: a uniqueness
        // checked on each of a store's objects was a scan of all of them
        // for each (PERF-SCAN)
        if let Some(found) = pairs.iter().find_map(|(a, v)| self.held_where(type_, a, v)) {
            return Ok(found.into_iter().filter(|o| holds(o)).collect());
        }
        Ok(self.held_in_order(holds))
    }

    /// An object to change, fetched first where it is not held.
    pub fn get_mut(&mut self, id: &str) -> Result<Option<&mut Object>, crate::eval::NotRun> {
        // an object to change holds the rest of its record, so a change to
        // it changes what the store holds and not parts never read
        let slot = match self.get(id)? {
            Some(o) => {
                self.parts(o)?;
                self.held.by_id.get_mut().get(id).copied().flatten()
            }
            None => None,
        };
        if let Some(n) = slot {
            self.held.written.insert(id.to_string());
            self.held.changed.get_mut().push(n);
        }
        Ok(slot.and_then(|n| self.held.slots.get_mut(n)))
    }

    /// An object the operation makes or replaces, held from now on as it
    /// is given: refused where the rest of its record was not read, which a
    /// write would replace with what it holds (ADR-0165).
    pub fn insert(&mut self, o: Object) -> Result<(), crate::eval::NotRun> {
        o.held_parts()?;
        self.held.written.insert(o.id.clone());
        let held = self.held.by_id.get_mut().get(&o.id).copied().flatten();
        if let Some(n) = held {
            self.held.changed.get_mut().push(n);
        }
        match held.and_then(|n| self.held.slots.get_mut(n)) {
            Some(slot) => *slot = o,
            None => {
                let id = o.id.clone();
                let n = self.hold(o)?;
                self.held.by_id.get_mut().insert(id, Some(n));
            }
        }
        Ok(())
    }

    /// The ids of the objects the operation changed or made, through
    /// [`World::get_mut`] or [`World::insert`], whether or not a change
    /// altered them.
    pub fn written(&self) -> impl Iterator<Item = &String> + '_ {
        self.held.written.iter()
    }

    /// An object with the rest of its record read, as [`World::get`] and
    /// [`World::parts`] together give it.
    pub fn whole(&self, id: &str) -> Result<Option<&Object>, crate::eval::NotRun> {
        match self.get(id)? {
            Some(o) => {
                self.parts(o)?;
                Ok(Some(o))
            }
            None => Ok(None),
        }
    }

    /// The objects held so far, in the order first held, without reading
    /// the store.
    pub fn held(&self) -> impl Iterator<Item = &Object> + '_ {
        self.held.slots.iter()
    }

    /// The version an object's last event was recorded under, the
    /// installed one for an object with none.
    pub fn version_of(&self, id: &str) -> Result<i64, crate::eval::NotRun> {
        Ok(self
            .log()?
            .rfind(|e| e.object_id == id)
            .map_or(self.lib.version, |e| e.declaration_version))
    }

    /// The next place in creation order: after every object the store
    /// holds and every one the operation made.
    pub fn next_order(&self) -> Result<i64, crate::eval::NotRun> {
        let stored = self.held.stored_max.get_or_init(|| match self.conn {
            Some(conn) => crate::store::read_max_order(conn).map_err(|e| e.to_string()),
            None => Ok(0),
        });
        let stored = *stored.as_ref().map_err(|e| {
            crate::eval::NotRun::Unreadable(format!(
                "the store's directory, which could not be read: {e}"
            ))
        })?;
        let made = self.held.slots.iter().map(|o| o.order).max().unwrap_or(0);
        Ok(stored.max(made) + 1)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn slots_grow_without_moving_what_they_hold() {
        assert_eq!(Slots::<i64>::place(0), Some((0, 0)));
        assert_eq!(Slots::<i64>::place(1), Some((1, 0)));
        assert_eq!(Slots::<i64>::place(2), Some((1, 1)));
        assert_eq!(Slots::<i64>::place(3), Some((2, 0)));
        assert_eq!(Slots::<i64>::place(6), Some((2, 3)));
        assert_eq!(Slots::<i64>::place(7), Some((3, 0)));
        let mut slots = Slots::default();
        let first: *const i64 = {
            let n = slots.push(0).unwrap();
            slots.get(n).unwrap()
        };
        for v in 1..1000 {
            assert_eq!(slots.push(v), Some(usize::try_from(v).unwrap()));
        }
        // the first value stayed where it was written
        assert!(std::ptr::eq(first, slots.get(0).unwrap()));
        assert_eq!(
            slots.iter().copied().collect::<Vec<_>>(),
            (0..1000).collect::<Vec<_>>()
        );
        *slots.get_mut(511).unwrap() = -1;
        assert_eq!(slots.get(511), Some(&-1));
        assert_eq!(slots.get(1000), None);
        // a clone holds its own values
        let copy = slots.clone();
        *slots.get_mut(0).unwrap() = 7;
        assert_eq!(copy.get(0), Some(&0));
    }
}

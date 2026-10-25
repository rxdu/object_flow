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

/// A name an object's record repeats, an attribute's, a tracked member's or
/// an actor's, held once and shared by every object a read of the store
/// gives it to, where each held a `String` of its own: a million of them
/// on the two-year profile store (MEMORY). A `&str` finds it in a map.
pub type Name = std::rc::Rc<str>;

/// The names one read of the store gives its objects, each held once, so
/// every object of the read shares it (MEMORY).
#[derive(Default)]
pub(crate) struct Names(std::collections::HashSet<Name>);

impl Names {
    /// The name, held once.
    pub(crate) fn of(&mut self, s: &str) -> Name {
        if let Some(n) = self.0.get(s) {
            return n.clone();
        }
        let n: Name = s.into();
        self.0.insert(n.clone());
        n
    }
}

/// The parts of an object's record beyond its row, which a read of the store
/// gives on first use (ADR-0165).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parts {
    /// Its flow data: for the state and each tracked member, the spans it
    /// held each value for, in order (`storage-schema.md`, `of_interval`).
    pub spans: BTreeMap<Name, Vec<Span>>,
    /// The attribute write index: for each attribute, part relationship or
    /// collection, the position of the event that last wrote it
    /// (`storage-schema.md`, `of_attribute_write`).
    pub writes: BTreeMap<Name, i64>,
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
    pub entered_by: Option<Name>,
    pub entered_by_kind: Name,
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
/// out references to it that live as long as the world. Each value is held
/// behind an `Rc`, so another world may hold the same one, and a change
/// through [`Slots::get_mut`] copies a value only while it is shared: a
/// publish's simulation holds the objects its version leaves as they were
/// without a copy of each (MEMORY, ADR-0222).
/// A chunk of [`Slots`], each slot written at most once.
type Chunk<T> = Box<[std::cell::OnceCell<std::rc::Rc<T>>]>;

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

    /// Every value written, taken out, in the order of their slots, each
    /// still shared where another holds it.
    fn into_shared(self) -> Vec<std::rc::Rc<T>> {
        self.chunks
            .into_iter()
            .filter_map(std::cell::OnceCell::into_inner)
            .flat_map(|chunk| chunk.into_vec())
            .filter_map(std::cell::OnceCell::into_inner)
            .collect()
    }

    /// A slot written with the value given, and its number; none past the
    /// last chunk.
    fn push(&self, value: T) -> Option<usize> {
        self.push_shared(std::rc::Rc::new(value))
    }

    /// A slot written with a value another may hold too, and its number.
    fn push_shared(&self, value: std::rc::Rc<T>) -> Option<usize> {
        let n = self.len.get();
        let (k, at) = Self::place(n)?;
        let size = 1usize.checked_shl(u32::try_from(k).ok()?)?;
        let chunk = self
            .chunks
            .get(k)?
            .get_or_init(|| (0..size).map(|_| std::cell::OnceCell::new()).collect());
        chunk.get(at)?.set(value).ok()?;
        self.len.set(n + 1);
        Some(n)
    }

    fn get(&self, n: usize) -> Option<&T> {
        self.get_shared(n).map(|r| &**r)
    }

    /// The n-th value, to hold in another world as well.
    fn get_shared(&self, n: usize) -> Option<&std::rc::Rc<T>> {
        let (k, at) = Self::place(n)?;
        self.chunks.get(k)?.get()?.get(at)?.get()
    }

    /// The n-th slot itself, to write another value into.
    fn slot_mut(&mut self, n: usize) -> Option<&mut std::rc::Rc<T>> {
        let (k, at) = Self::place(n)?;
        self.chunks.get_mut(k)?.get_mut()?.get_mut(at)?.get_mut()
    }
}

impl<T: Clone> Slots<T> {
    /// Every value written, taken out, in the order of their slots, one
    /// another still holds copied.
    fn into_values(self) -> Vec<T> {
        self.into_shared()
            .into_iter()
            .map(std::rc::Rc::unwrap_or_clone)
            .collect()
    }

    /// The n-th value, to change: copied first where another holds it, so
    /// a change here is never seen there.
    fn get_mut(&mut self, n: usize) -> Option<&mut T> {
        self.slot_mut(n).map(std::rc::Rc::make_mut)
    }
}

impl<T> Slots<T> {
    fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        (0..self.len.get()).filter_map(|n| self.get(n))
    }
}

/// What a world says when it can hold no more objects.
fn too_many() -> crate::eval::NotRun {
    crate::eval::NotRun::Unsupported("more objects than an operation can hold".into())
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
    /// Whether a scan of many rows is reading, which reads whole a type it
    /// reaches past [`BATCH`] times, by id or narrowed: see [`World::scan`].
    scanning: std::cell::Cell<bool>,
    /// The reads of each type, by id or narrowed, during a scan.
    scan_reads: std::cell::RefCell<BTreeMap<String, usize>>,
    /// The held objects by a column's value, each column built on its
    /// first use and kept as objects are held and changed (PERF-SCAN).
    index: std::cell::RefCell<BTreeMap<(String, String), Column>>,
    /// Each slot changed or replaced since the operation began, in order,
    /// which a column keys again from where it last read.
    changed: std::cell::RefCell<Vec<usize>>,
    /// How many slots the columns have keyed, which a test holds linear in
    /// what is held.
    keyed: std::cell::Cell<usize>,
    /// How many candidates the columns have given lookups, which a test
    /// holds to what the lookups match.
    gathered: std::cell::Cell<usize>,
    /// How many held objects the operation's sweeps have looked at, which a
    /// test holds to a constant times what a lookup by index would read
    /// (seventh production review PUBLISH-SLOW).
    enumerated: std::cell::Cell<usize>,
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
    /// Each slot's keys, so a changed slot's old ones are taken out.
    key_of: BTreeMap<usize, Vec<Option<String>>>,
}

impl Column {
    /// A slot keyed as the object it holds now is: taken out under its old
    /// keys, and put under its new ones where it is of the column's type and
    /// its value is one an index holds.
    fn key(&mut self, n: usize, o: Option<&Object>, type_: &str, attr: &str, keying: Keying) {
        for old in self.key_of.remove(&n).unwrap_or_default() {
            if let Some(slots) = self.by.get_mut(&old) {
                slots.remove(&n);
            }
        }
        if let Some(o) = o
            && o.type_ == type_
        {
            let keys = keying.keys(&o.attr(attr));
            for k in &keys {
                self.by.entry(k.clone()).or_default().insert(n);
            }
            if !keys.is_empty() {
                self.key_of.insert(n, keys);
            }
        }
    }
}

/// How a column keys the objects it holds by an attribute: by its value,
/// or by its text with its ASCII letters in lower case, as SQLite's `lower`
/// folds them, so the store's index on an identity's folded column and the
/// operation's agree (ADR-0194 decision 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Keying {
    Value,
    Folded,
}

impl Keying {
    /// The keys a column of this keying holds a value by: none where no
    /// index holds it.
    fn keys(self, v: &Value) -> Vec<Option<String>> {
        match self {
            Keying::Value => index_key(v).into_iter().collect(),
            Keying::Folded => match v {
                Value::Absent => vec![None],
                Value::Text(t) => vec![Some(format!("t:{}", t.to_ascii_lowercase()))],
                _ => Vec::new(),
            },
        }
    }

    /// The column's name beside the attribute's, so columns of two keyings
    /// of one attribute are never one.
    fn named(self, attr: &str) -> String {
        match self {
            Keying::Value => attr.to_string(),
            Keying::Folded => format!("lower({attr})"),
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

/// A scan of many rows, which ends when it is dropped: see [`World::scan`].
pub(crate) struct Scan<'a> {
    flag: &'a std::cell::Cell<bool>,
    was: bool,
}

impl Drop for Scan<'_> {
    fn drop(&mut self) {
        self.flag.set(self.was);
    }
}

/// No earlier version's library: a store with one version, or a world
/// built without them.
static NO_EARLIER: BTreeMap<i64, Library> = BTreeMap::new();

thread_local! {
    /// How many times this thread has read the store's whole log: what a
    /// test of an operation that must read only part of it reads (fourth
    /// production review).
    static WHOLE_LOG_READS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// How many times this thread has read each type's objects whole, from
    /// the store (ADR-0191 decision 5).
    static WHOLE_TYPE_READS: std::cell::RefCell<BTreeMap<String, u64>> =
        const { std::cell::RefCell::new(BTreeMap::new()) };
    /// How many events this thread has read whole from the store: what a
    /// test of a read that must hold few events in memory reads (tenth
    /// production review).
    static EVENTS_READ: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// How many objects' records this thread has read from the store: what
    /// a test of a read that must read the records of its page alone reads
    /// (thirteenth production review N3).
    static RECORDS_READ: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// How many times this thread has read the store's whole attempt log
    /// (ADR-0205 decision 12).
    static WHOLE_ATTEMPT_READS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// How many times this thread has read the store's whole attempt log.
pub fn whole_attempt_reads() -> u64 {
    WHOLE_ATTEMPT_READS.with(std::cell::Cell::get)
}

/// How many objects' records this thread has read from the store.
pub fn records_read() -> u64 {
    RECORDS_READ.with(std::cell::Cell::get)
}

/// Counts objects' records read from the store (`store::read_parts`).
pub(crate) fn records_counted(n: usize) {
    RECORDS_READ.with(|c| c.set(c.get() + n as u64));
}

/// How many events this thread has read whole from the store.
pub fn events_read() -> u64 {
    EVENTS_READ.with(std::cell::Cell::get)
}

fn counted(n: usize) {
    EVENTS_READ.with(|c| c.set(c.get() + n as u64));
}

/// Counts events read whole from the store outside a world, as a history
/// reads them.
pub(crate) fn events_counted(n: usize) {
    counted(n);
}

/// How many times this thread has read the store's whole log.
pub fn whole_log_reads() -> u64 {
    WHOLE_LOG_READS.with(std::cell::Cell::get)
}

/// How many times this thread has read a type's objects whole.
pub fn whole_type_reads(type_: &str) -> u64 {
    WHOLE_TYPE_READS.with(|n| n.borrow().get(type_).copied().unwrap_or(0))
}

/// Each attribute whose value differs between two of an object's states,
/// with its value after in the wire's notation, null where it is gone: what
/// an event wrote (ADR-0209 decision 1).
pub fn wrote_between(
    was: &BTreeMap<String, Value>,
    now: &BTreeMap<String, Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    for (a, v) in now {
        if was.get(a) != Some(v) {
            out.insert(a.clone(), crate::wire::encode(v));
        }
    }
    for a in was.keys().filter(|a| !now.contains_key(*a)) {
        out.insert(a.clone(), serde_json::Value::Null);
    }
    out
}

/// An event's payload with what it wrote, where it wrote anything; an event
/// that changed no attribute holds none (ADR-0209 decision 1).
pub fn with_wrote(
    mut payload: serde_json::Value,
    wrote: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Value {
    if !wrote.is_empty() {
        payload["wrote"] = serde_json::Value::Object(wrote);
    }
    payload
}

/// The objects of a store, and the time a request reads.
#[derive(Clone, Debug)]
pub struct World<'l> {
    /// Each type's family, itself and the types extending it in the
    /// library's order, found once per world, where each lookup walked every
    /// type of the library: a change adding thousands of types made each of
    /// a store's objects' checks walk them all, about 13 ms a type on 37k
    /// objects (tenth production review).
    families: std::cell::RefCell<BTreeMap<String, std::sync::Arc<Vec<String>>>>,
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
    conn: Option<&'l crate::db::Connection>,
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
    /// Each actor's object, by its identity folded to lower case, where an
    /// actor type marks its identity personal; none where none does. Built
    /// on first use from the actor types, which hold few objects.
    actors_by_identity: std::cell::OnceCell<Result<Option<BTreeMap<String, String>>, String>>,
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
    /// The service's cut epoch and the one the read began in: a read sees
    /// it move when the store's log must be cut and stops, answered as one
    /// past its deadline (seventh production review CUTSTALL, ADR-0194
    /// decision 4).
    pub stop: Option<(std::sync::Arc<std::sync::atomic::AtomicU64>, u64)>,
    ticks: std::cell::Cell<u32>,
    /// Each changed object's attributes as its last event in the operation
    /// left them, or as they were before the operation first changed it, an
    /// object it made having none: what its next event wrote is measured
    /// against them (ADR-0209 decision 1).
    last_written: BTreeMap<String, BTreeMap<String, Value>>,
}

/// How many steps of a read pass between two readings of the clock against
/// its deadline.
const TICKS: u32 = 256;

impl<'l> World<'l> {
    /// How a metric's row names an actor its record names by identity: by
    /// the actor's object id, where an actor type marks its identity
    /// personal, as a metric reads no personal value; by the identity where
    /// none does (the audit's personal data R8, ADR-0203 decision 1). An
    /// identity no actor holds, as an erased one renamed already is, is kept.
    pub(crate) fn actor_named(&self, identity: &str) -> Result<String, crate::eval::NotRun> {
        let map = self.actors_by_identity.get_or_init(|| {
            if !crate::eval::identities_personal(self.lib) {
                return Ok(None);
            }
            let mut out = BTreeMap::new();
            for (tn, t) in &self.lib.types {
                let Some((attr, _)) = t.actor_identity() else {
                    continue;
                };
                for o in self.of_type(tn).map_err(|e| e.to_string())? {
                    if let Value::Text(held) = o.attr(attr) {
                        out.insert(held.to_ascii_lowercase(), o.id.clone());
                    }
                }
            }
            Ok(Some(out))
        });
        match map {
            Ok(None) => Ok(identity.to_string()),
            Ok(Some(m)) => Ok(m
                .get(&identity.to_ascii_lowercase())
                .cloned()
                .unwrap_or_else(|| identity.to_string())),
            Err(e) => Err(crate::eval::NotRun::Unreadable(format!(
                "the actors, by identity: {e}"
            ))),
        }
    }

    pub fn new(lib: &'l Library, now: i64) -> World<'l> {
        World {
            families: std::cell::RefCell::new(BTreeMap::new()),
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
            actors_by_identity: std::cell::OnceCell::new(),
            acknowledged: BTreeMap::new(),
            stored_rollup: std::cell::OnceCell::new(),
            deadline: None,
            stop: None,
            ticks: std::cell::Cell::new(0),
            last_written: BTreeMap::new(),
        }
    }

    /// What an object's event wrote: each attribute changed since its last
    /// event in the operation, or since the operation first changed it,
    /// none where the operation never did; and what its next event wrote is
    /// measured from here (ADR-0209 decision 1).
    pub fn wrote(
        &mut self,
        id: &str,
    ) -> Result<serde_json::Map<String, serde_json::Value>, crate::eval::NotRun> {
        let now = self.get(id)?.map(|o| o.attrs.clone()).unwrap_or_default();
        let was = self
            .last_written
            .insert(id.to_string(), now.clone())
            .unwrap_or_else(|| now.clone());
        Ok(wrote_between(&was, &now))
    }

    /// What an event on an object changed apart from the world wrote, as
    /// [`World::wrote`] measures it, `was` being its attributes before the
    /// change where the operation had not changed it already; and what its
    /// next event wrote is measured from `now` (ADR-0209 decision 1).
    pub fn wrote_apart(
        &mut self,
        id: &str,
        was: &BTreeMap<String, Value>,
        now: &BTreeMap<String, Value>,
    ) -> serde_json::Map<String, serde_json::Value> {
        let base = self
            .last_written
            .insert(id.to_string(), now.clone())
            .unwrap_or_else(|| was.clone());
        wrote_between(&base, now)
    }

    /// A type's family, itself and the types extending it, in the library's
    /// order.
    pub fn family(&self, type_: &str) -> std::sync::Arc<Vec<String>> {
        if let Some(f) = self.families.borrow().get(type_) {
            return f.clone();
        }
        let f: std::sync::Arc<Vec<String>> = std::sync::Arc::new(
            self.lib
                .types
                .keys()
                .filter(|t| self.lib.is_a(t, type_))
                .cloned()
                .collect(),
        );
        self.families
            .borrow_mut()
            .insert(type_.to_string(), f.clone());
        f
    }

    /// Whether the read is within its deadline, where it has one: a filter,
    /// a metric or a sweep with no bound held a reader, and an erasure's
    /// truncation behind it, for minutes (ADR-0184 decision 3).
    pub fn on_time(&self) -> Result<(), crate::eval::NotRun> {
        if self.deadline.is_none() && self.stop.is_none() {
            return Ok(());
        }
        let n = self.ticks.get().wrapping_add(1);
        self.ticks.set(n);
        if !n.is_multiple_of(TICKS) {
            return Ok(());
        }
        self.due()
    }

    /// Whether the read is within its deadline and not stopped, read now:
    /// for a step that reads much of the store at once, where
    /// [`World::on_time`] reads it every [`TICKS`] steps.
    pub fn due(&self) -> Result<(), crate::eval::NotRun> {
        if let Some((epoch, began)) = &self.stop
            && epoch.load(std::sync::atomic::Ordering::SeqCst) != *began
        {
            return Err(crate::eval::NotRun::TimedOut(
                "the read was stopped so the store's log could be cut, after an erasure or past its size; send it again"
                    .into(),
            ));
        }
        if self
            .deadline
            .is_some_and(|at| std::time::Instant::now() > at)
        {
            return Err(crate::eval::NotRun::TimedOut(
                "the read passed its deadline; narrow it, a filter by an indexed member or a page"
                    .into(),
            ));
        }
        Ok(())
    }

    /// The library as the version given declared it: the installed one, or
    /// an earlier one where the store holds it.
    pub fn library_at(&self, version: i64) -> &'l Library {
        match self.earlier.get(&version) {
            Some(lib) if version != self.lib.version => lib,
            _ => self.lib,
        }
    }

    /// A type as the version given declared it: the installed version's,
    /// or an earlier one's where the store holds it.
    pub fn type_at(&self, version: i64, name: &str) -> Option<&'l Type> {
        self.library_at(version).type_(name)
    }

    /// Whether an attribute of a type, named as the installed version names
    /// it, was personal under the version given (ADR-0146 decision 2): its
    /// name taken back through each later version's renames first, so a
    /// value written before a rename is judged as the attribute it was.
    pub fn personal_under(&self, version: i64, type_: &str, attribute: &str) -> bool {
        let name = self.named_under(version, type_, attribute);
        self.type_at(version, type_)
            .and_then(|t| t.attribute(&name))
            .is_some_and(|a| a.personal)
    }

    /// An attribute of a type, named as the installed version names it, as
    /// the version given named it: taken back through each later version's
    /// renames.
    pub fn named_under(&self, version: i64, type_: &str, attribute: &str) -> String {
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
        name
    }

    /// A world that reads its log and its objects through the connection
    /// given.
    pub(crate) fn reading_from(mut self, conn: &'l crate::db::Connection) -> World<'l> {
        self.conn = Some(conn);
        self
    }

    /// The events a clause over the log's indexes selects, the operation's
    /// own among them, in position order; the held log filtered where it is
    /// held already.
    fn events_where(
        &self,
        clause: &str,
        value: &dyn crate::db::ToSql,
        keep: impl Fn(&Event) -> bool,
    ) -> Result<Vec<Event>, crate::eval::NotRun> {
        let Some(conn) = self.conn.filter(|_| self.stored.get().is_none()) else {
            return Ok(self.log()?.filter(|e| keep(e)).cloned().collect());
        };
        let mut out = crate::store::read_events(conn, clause, value).map_err(|e| {
            crate::eval::NotRun::Unreadable(format!(
                "the store's events where {clause}, which could not be read: {e}"
            ))
        })?;
        counted(out.len());
        out.extend(self.appended.iter().filter(|e| keep(e)).cloned());
        Ok(out)
    }

    /// The heads of the events after a position, as `log_after` reads their
    /// events, without their payloads (tenth production review).
    pub fn log_heads_after(
        &self,
        after: i64,
        types: Option<&[String]>,
        limit: usize,
    ) -> Result<Vec<crate::store::Head>, crate::eval::NotRun> {
        let held = self.stored.get().is_some();
        if held || self.conn.is_none() {
            return Ok(self
                .log_after(after, types, None, limit)?
                .iter()
                .map(crate::store::Head::of)
                .collect());
        }
        let of_types = |e: &Event| -> Result<bool, crate::eval::NotRun> {
            Ok(match types {
                None => true,
                Some(types) => self
                    .get(&e.object_id)?
                    .is_some_and(|o| types.contains(&o.type_)),
            })
        };
        let mut out = match self.conn {
            Some(conn) => {
                crate::store::read_log_heads_after(conn, after, types, limit).map_err(|e| {
                    crate::eval::NotRun::Unreadable(format!(
                        "the store's log after {after}, which could not be read: {e}"
                    ))
                })?
            }
            None => Vec::new(),
        };
        for e in &self.appended {
            if out.len() >= limit {
                break;
            }
            if e.position > after && of_types(e)? {
                out.push(crate::store::Head::of(e));
            }
        }
        Ok(out)
    }

    /// The event at a position, none where the log has none there.
    pub fn event_at(&self, position: i64) -> Result<Option<Event>, crate::eval::NotRun> {
        Ok(self
            .events_where("position = ?1", &position, |e| e.position == position)?
            .into_iter()
            .next())
    }

    /// An object's events, in position order.
    pub fn events_of(&self, id: &str) -> Result<Vec<Event>, crate::eval::NotRun> {
        self.events_where("object_id = ?1", &id, |e| e.object_id == id)
    }

    /// The events one event caused, in position order.
    pub fn caused_by(&self, position: i64) -> Result<Vec<Event>, crate::eval::NotRun> {
        self.events_where("cause_position = ?1", &position, |e| {
            e.cause == Some(position)
        })
    }

    /// The log, in position order: every event the store holds, read on first
    /// use, then those the operation appended (`storage-schema.md`, `of_event`,
    /// ADR-0162). A log that cannot be read stops the operation as not run,
    /// naming why, never as an empty log.
    pub fn log(&self) -> Result<impl DoubleEndedIterator<Item = &Event> + '_, crate::eval::NotRun> {
        let stored = self.stored.get_or_init(|| match self.conn {
            Some(conn) => {
                WHOLE_LOG_READS.with(|n| n.set(n.get() + 1));
                crate::store::read_log(conn).map_err(|e| e.to_string())
            }
            None => Ok(Vec::new()),
        });
        match stored {
            Ok(events) => Ok(events.iter().chain(self.appended.iter())),
            Err(e) => Err(crate::eval::NotRun::Unreadable(format!(
                "the store's log, which could not be read: {e}"
            ))),
        }
    }

    /// The events after a position, in position order, at most `limit`, of
    /// the objects of the types given where some are, the operation's own
    /// among them: a page read from a cursor, never the log whole, where the
    /// log is not held already (fourth production review FEED).
    pub fn log_after(
        &self,
        after: i64,
        types: Option<&[String]>,
        entering: Option<&[String]>,
        limit: usize,
    ) -> Result<Vec<Event>, crate::eval::NotRun> {
        let of_types = |e: &Event| -> Result<bool, crate::eval::NotRun> {
            Ok(entering.is_none_or(|s| s.contains(&e.to_state))
                && match types {
                    None => true,
                    Some(types) => self
                        .get(&e.object_id)?
                        .is_some_and(|o| types.contains(&o.type_)),
                })
        };
        let mut out = Vec::new();
        let held = self.stored.get().is_some();
        if held || self.conn.is_none() {
            for e in self.log()? {
                if out.len() >= limit {
                    break;
                }
                if e.position > after && of_types(e)? {
                    out.push(e.clone());
                }
            }
            return Ok(out);
        }
        if let Some(conn) = self.conn {
            out =
                crate::store::read_log_after(conn, after, types, entering, limit).map_err(|e| {
                    crate::eval::NotRun::Unreadable(format!(
                        "the store's log after {after}, which could not be read: {e}"
                    ))
                })?;
            counted(out.len());
        }
        for e in &self.appended {
            if out.len() >= limit {
                break;
            }
            if e.position > after && of_types(e)? {
                out.push(e.clone());
            }
        }
        Ok(out)
    }

    /// The intervals of the types given whose cursor, the event that last
    /// wrote each, is past `after` and at most `upto`, read by the store's
    /// indexes (`store::read_interval_page`), never the log or a type whole
    /// (eleventh production review N1). A world reading no store holds none.
    pub fn intervals_between(
        &self,
        types: &[String],
        after: i64,
        upto: i64,
    ) -> Result<Vec<crate::store::CursoredSpan>, crate::eval::NotRun> {
        let Some(conn) = self.conn else {
            return Err(crate::eval::NotRun::Unsupported(
                "an export of intervals by a world that reads no store".into(),
            ));
        };
        crate::store::read_interval_page(conn, self.lib, types, after, upto).map_err(|e| {
            crate::eval::NotRun::Unreadable(format!(
                "the intervals written after {after}, which could not be read: {e}"
            ))
        })
    }

    /// The ids of a type's objects whose last event is past `after`, with it,
    /// at most `limit`, by its index; for the labels, those on objects of
    /// the types given (the audit's cost B12). None for a world that reads
    /// no store, which reads its objects as it holds them.
    pub fn ids_after(
        &self,
        type_: &str,
        subjects: Option<&[String]>,
        after: i64,
        limit: usize,
    ) -> Result<Option<Vec<(String, i64)>>, crate::eval::NotRun> {
        let Some(conn) = self.conn.filter(|_| self.appended.is_empty()) else {
            return Ok(None);
        };
        crate::store::read_ids_after(conn, self.lib, type_, subjects, after, limit)
            .map(Some)
            .map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "the objects of {type_} written after {after}, which could not be read: {e}"
                ))
            })
    }

    /// The interval rows of the types given a commit past `after` and at
    /// most `upto` retracted (ADR-0205 decision 9).
    pub fn retractions_between(
        &self,
        types: &[String],
        after: i64,
        upto: i64,
    ) -> Result<Vec<crate::store::Retraction>, crate::eval::NotRun> {
        let Some(conn) = self.conn else {
            return Ok(Vec::new());
        };
        crate::store::read_interval_retractions(conn, types, after, upto).map_err(|e| {
            crate::eval::NotRun::Unreadable(format!(
                "the intervals retracted after {after}, which could not be read: {e}"
            ))
        })
    }

    /// Whether the operation has read the whole log, which a test holds a
    /// page of the feed from doing (FEED).
    pub fn log_read_whole(&self) -> bool {
        self.stored.get().is_some()
    }

    /// The log's last position, the operation's own events among it.
    pub fn last_position(&self) -> Result<i64, crate::eval::NotRun> {
        let stored = match (self.stored.get(), self.conn) {
            (Some(Ok(events)), _) => events.last().map_or(0, |e| e.position),
            (_, Some(conn)) => crate::store::read_last_position(conn).map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "the store's last position, which could not be read: {e}"
                ))
            })?,
            _ => 0,
        };
        Ok(self
            .appended
            .last()
            .map_or(stored, |e| e.position.max(stored)))
    }

    /// The restores the store has had, oldest first; none in a sandbox's,
    /// which is never restored (ADR-0207).
    pub fn restores(&self) -> Result<Vec<crate::store::Restore>, crate::eval::NotRun> {
        match self.conn {
            Some(conn) => crate::store::read_restores(conn).map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "the store's restores, which could not be read: {e}"
                ))
            }),
            None => Ok(Vec::new()),
        }
    }

    /// Whether a position falls in a gap a restore skipped, which the copy
    /// never saw, so no cursor there is the store's (ADR-0207 decision 5).
    pub fn skipped_position(&self, p: i64) -> Result<bool, crate::eval::NotRun> {
        Ok(self.restores()?.iter().any(|r| r.skipped_position(p)))
    }

    /// Whether a transaction falls in a gap a restore skipped.
    pub fn skipped_txn(&self, t: i64) -> Result<bool, crate::eval::NotRun> {
        Ok(self.restores()?.iter().any(|r| r.skipped_txn(t)))
    }

    /// The last transaction number the store has handed out, an attempt's
    /// cursor; none bounds it where no store is read.
    pub fn last_txn(&self) -> Result<i64, crate::eval::NotRun> {
        match self.conn {
            Some(conn) => crate::store::read_counter(conn, "txn").map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "the store's transaction counter, which could not be read: {e}"
                ))
            }),
            None => Ok(i64::MAX),
        }
    }

    /// An object's first event's position, none where it has none.
    pub fn first_position(&self, id: &str) -> Result<Option<i64>, crate::eval::NotRun> {
        if self.stored.get().is_some() || self.conn.is_none() {
            return Ok(self.log()?.find(|e| e.object_id == id).map(|e| e.position));
        }
        let stored = match self.conn {
            Some(conn) => crate::store::read_first_position(conn, id).map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "the first event of {id}, which could not be read: {e}"
                ))
            })?,
            None => None,
        };
        Ok(stored.or_else(|| {
            self.appended
                .iter()
                .find(|e| e.object_id == id)
                .map(|e| e.position)
        }))
    }

    /// The attempt log, in the order written, read on first use; a log that
    /// cannot be read stops the operation as not run, naming why.
    pub fn attempts(&self) -> Result<&[crate::store::Attempt], crate::eval::NotRun> {
        let stored = self.stored_attempts.get_or_init(|| match self.conn {
            Some(conn) => {
                WHOLE_ATTEMPT_READS.with(|n| n.set(n.get() + 1));
                crate::store::read_attempts(conn).map_err(|e| e.to_string())
            }
            None => Ok(Vec::new()),
        });
        stored.as_deref().map_err(|e| {
            crate::eval::NotRun::Unreadable(format!(
                "the store's attempt log, which could not be read: {e}"
            ))
        })
    }

    /// An object's attempts, in the order written, read by their index,
    /// where a rule reading `o.attempts` read the whole attempt log for each
    /// object (the audit's cost B1, ADR-0205 decision 12).
    pub fn attempts_of(&self, id: &str) -> Result<Vec<crate::store::Attempt>, crate::eval::NotRun> {
        self.attempts_where("object_id = ?1", &id, |a| {
            a.object_id.as_deref() == Some(id)
        })
    }

    /// A type's own attempts, in the order written, read by the index on
    /// their type (the audit's cost B5).
    pub fn attempts_of_type(
        &self,
        type_: &str,
    ) -> Result<Vec<crate::store::Attempt>, crate::eval::NotRun> {
        self.attempts_where("type = ?1", &type_, |a| a.type_.as_deref() == Some(type_))
    }

    /// The attempts an approval of a proposal left, in the order written,
    /// read by their index (B14).
    pub fn attempts_proposing(
        &self,
        id: &str,
    ) -> Result<Vec<crate::store::Attempt>, crate::eval::NotRun> {
        self.attempts_where("proposal = ?1", &id, |a| a.proposal.as_deref() == Some(id))
    }

    fn attempts_where(
        &self,
        clause: &str,
        value: &dyn crate::db::ToSql,
        keep: impl Fn(&crate::store::Attempt) -> bool,
    ) -> Result<Vec<crate::store::Attempt>, crate::eval::NotRun> {
        let Some(conn) = self.conn.filter(|_| self.stored_attempts.get().is_none()) else {
            return Ok(self
                .attempts()?
                .iter()
                .filter(|a| keep(a))
                .cloned()
                .collect());
        };
        crate::store::read_attempts_where(conn, clause, value).map_err(|e| {
            crate::eval::NotRun::Unreadable(format!(
                "the store's attempts where {clause}, which could not be read: {e}"
            ))
        })
    }

    /// The attempts on objects of the types given after a place in the order
    /// written and in a transaction at most `upto`, at most `limit`, each with
    /// its id (`store::read_attempts_after`), never the attempt log whole.
    pub fn attempts_after(
        &self,
        types: &[String],
        after: (i64, i64),
        upto: i64,
        limit: usize,
    ) -> Result<Vec<(i64, crate::store::Attempt)>, crate::eval::NotRun> {
        let Some(conn) = self.conn else {
            return Err(crate::eval::NotRun::Unsupported(
                "an export of attempts by a world that reads no store".into(),
            ));
        };
        crate::store::read_attempts_after(conn, types, after, upto, limit).map_err(|e| {
            crate::eval::NotRun::Unreadable(format!(
                "the attempts after transaction {}, which could not be read: {e}",
                after.0
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
        let type_ = fetched.as_ref().map(|o| o.type_.clone());
        let slot = match fetched {
            Some(o) => Some(self.hold(o)?),
            None => None,
        };
        self.held.by_id.borrow_mut().insert(id.to_string(), slot);
        if let Some(t) = type_
            && self.scanned_past(&t)
        {
            self.hold_type(&t)?;
        }
        Ok(slot.and_then(|n| self.held.slots.get(n)))
    }

    /// A scan of many rows, until the guard is dropped: a type its rows
    /// reach object by object, by id or narrowed, past [`BATCH`] times is
    /// read whole once and served from what is held, so a row's reach, a
    /// claim's lines summed for its total, is a lookup, where each row read
    /// its own by a query of its own. Only the types the rows reach are read,
    /// where reading every type a row's type refers to made a metric that
    /// reads none three times slower (the audit's cost B2, ADR-0205
    /// decision 13). A write never scans: its reads stay its own.
    pub(crate) fn scan(&self) -> Scan<'_> {
        Scan {
            flag: &self.held.scanning,
            was: self.held.scanning.replace(true),
        }
    }

    /// Whether a scan has now read a type alone past [`BATCH`] times.
    fn scanned_past(&self, type_: &str) -> bool {
        if !self.held.scanning.get() {
            return false;
        }
        let mut reads = self.held.scan_reads.borrow_mut();
        let n = reads.entry(type_.to_string()).or_default();
        *n += 1;
        *n > BATCH
    }

    /// The type of the object an id names: the held object's, or the store's
    /// directory's, holding nothing more, where [`World::get`] holds the
    /// object whole, a change's source and all (eleventh production review).
    pub fn type_of(&self, id: &str) -> Result<Option<String>, crate::eval::NotRun> {
        self.on_time()?;
        let known = self.held.by_id.borrow().get(id).copied();
        if let Some(slot) = known {
            return Ok(slot
                .and_then(|n| self.held.slots.get(n))
                .map(|o| o.type_.clone()));
        }
        match self.conn {
            Some(conn) => crate::store::read_type_of(conn, id).map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "the store's object {id}, which could not be read: {e}"
                ))
            }),
            None => Ok(None),
        }
    }

    /// The place in creation order of each object the ids name: a held
    /// object's, and the others' from the store's directory in reads of a
    /// few hundred ids, holding nothing more; an id neither holds is left
    /// out, as [`World::get`] finds no object for it. Where each was got
    /// whole, a metric ranking its groups by the 600 claimants they name
    /// made 1,200 round trips on PostgreSQL (ADR-0213).
    pub fn orders<'i>(
        &self,
        ids: impl IntoIterator<Item = &'i str>,
    ) -> Result<BTreeMap<String, i64>, crate::eval::NotRun> {
        self.on_time()?;
        let mut out = BTreeMap::new();
        let mut unread = BTreeSet::new();
        {
            let by_id = self.held.by_id.borrow();
            for id in ids {
                match by_id.get(id) {
                    Some(slot) => {
                        if let Some(o) = slot.and_then(|n| self.held.slots.get(n)) {
                            out.insert(id.to_string(), o.order);
                        }
                    }
                    None => {
                        unread.insert(id.to_string());
                    }
                }
            }
        }
        if let Some(conn) = self.conn
            && !unread.is_empty()
        {
            let unread: Vec<String> = unread.into_iter().collect();
            out.extend(crate::store::read_orders(conn, &unread).map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "the store's directory, which could not be read: {e}"
                ))
            })?);
        }
        Ok(out)
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
        self.held.slots.push(o).ok_or_else(too_many)
    }

    /// [`World::hold`] of an object another world may hold too.
    fn hold_shared(&self, o: std::rc::Rc<Object>) -> Result<usize, crate::eval::NotRun> {
        self.held.slots.push_shared(o).ok_or_else(too_many)
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
            WHOLE_TYPE_READS.with(|n| *n.borrow_mut().entry(type_.to_string()).or_default() += 1);
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
    fn held_where(
        &self,
        type_: &str,
        attr: &str,
        v: &Value,
        absent_too: bool,
        keying: Keying,
    ) -> Option<Vec<&Object>> {
        let key = Some(keying.keys(v).into_iter().next()??);
        let mut idx = self.held.index.borrow_mut();
        let changed = self.held.changed.borrow();
        let slots = self.held.slots.len.get();
        let mut found: BTreeSet<usize> = BTreeSet::new();
        for t in self.family(type_).iter() {
            let column = idx.entry((t.clone(), keying.named(attr))).or_default();
            // the slots held since it last looked, then those changed since
            let fresh = changed.get(column.changes..).unwrap_or_default();
            self.held
                .keyed
                .set(self.held.keyed.get() + (slots - column.slots) + fresh.len());
            for n in column.slots..slots {
                column.key(n, self.held.slots.get(n), t, attr, keying);
            }
            for &n in fresh {
                column.key(n, self.held.slots.get(n), t, attr, keying);
            }
            (column.slots, column.changes) = (slots, changed.len());
            // the objects holding no value, which a narrowing keeps and an
            // exact match never wants: gathered for every lookup of an
            // optional identifier, they made a dry run's check of each
            // object's quadratic (PUBLISH-SCALE)
            let absent = absent_too.then_some(None);
            for k in std::iter::once(key.clone()).chain(absent) {
                if let Some(ns) = column.by.get(&k) {
                    found.extend(ns);
                }
            }
        }
        let mut out: Vec<&Object> = found
            .into_iter()
            .filter_map(|n| self.held.slots.get(n))
            .collect();
        self.held.gathered.set(self.held.gathered.get() + out.len());
        out.sort_by_key(|o| o.order);
        Some(out)
    }

    /// Every object the world holds, taken out of it, copied only where
    /// another world holds it too.
    pub fn into_objects(self) -> Vec<Object> {
        self.held.slots.into_values()
    }

    /// Every object the world holds, taken out of it, each still shared
    /// with any other world that holds it, never copied (ADR-0222).
    pub fn into_shared(self) -> Vec<std::rc::Rc<Object>> {
        self.held.slots.into_shared()
    }

    /// An object this world holds, to hold in another as well without a
    /// copy, which a change there copies first (ADR-0222); none where it
    /// holds none of the id.
    pub fn shared(&self, id: &str) -> Option<std::rc::Rc<Object>> {
        let n = self.held.by_id.borrow().get(id).copied().flatten()?;
        self.held.slots.get_shared(n).cloned()
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

    /// How many candidates the operation's column indexes have given its
    /// lookups: what an exact lookup matches, where the objects holding no
    /// value, given to every lookup of an optional identifier, made a dry
    /// run's check of each object quadratic (PUBLISH-SCALE).
    pub fn gathered_count(&self) -> usize {
        self.held.gathered.get()
    }

    /// How many held objects the operation's sweeps have looked at.
    pub fn enumerated_count(&self) -> usize {
        self.held.enumerated.get()
    }

    fn held_in_order(&self, of: impl Fn(&Object) -> bool) -> Vec<&Object> {
        self.held
            .enumerated
            .set(self.held.enumerated.get() + self.held.slots.len.get());
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
                // every type read whole, counted as a type's whole read is,
                // where a test of a read that must read none could not see
                // this one (ADR-0194 decision 3)
                WHOLE_TYPE_READS.with(|n| {
                    let mut n = n.borrow_mut();
                    for t in self.lib.types.keys() {
                        *n.entry(t.clone()).or_default() += 1;
                    }
                });
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

    /// Whether the world reads no store, holding only what it was given.
    pub(crate) fn reads_no_store(&self) -> bool {
        self.conn.is_none()
    }

    /// The spans of the dimensions named of every object of a concrete type
    /// the store holds, by object, read by the type and never each object's
    /// record whole; none for a world without a store (the audit's cost B3).
    pub fn type_spans(
        &self,
        type_: &str,
        dimensions: &[&str],
    ) -> Result<Option<BTreeMap<String, crate::eval::Spans>>, crate::eval::NotRun> {
        let Some(conn) = self.conn else {
            return Ok(None);
        };
        crate::store::read_type_spans(conn, self.lib, type_, dimensions)
            .map(Some)
            .map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "the spans of {type_}, which could not be read: {e}"
                ))
            })
    }

    /// The ids of the observations of a kind, or of its subtypes, that one
    /// of them corrects: read by the store's index on the stored end, and
    /// those the operation holds, never the kind whole.
    pub fn corrected(&self, kind: &str) -> Result<BTreeSet<String>, crate::eval::NotRun> {
        let mut out = BTreeSet::new();
        if let Some(conn) = self.conn {
            for t in self.family(kind).iter() {
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

    /// The records of objects of one concrete type, these alone, a few
    /// hundred to each read of each part's table.
    fn read_together(
        &self,
        conn: &crate::db::Connection,
        type_: &str,
        objects: &[&Object],
    ) -> Result<(), crate::eval::NotRun> {
        let ids: Vec<String> = objects.iter().map(|o| o.id.clone()).collect();
        let mut read =
            crate::store::read_parts_of_ids(conn, self.lib, type_, &ids).map_err(|e| {
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
        self.of_type_where_since(type_, states, columns, None)
    }

    /// The objects [`World::of_type_where`] gives, and of them, where
    /// `since` names a time or a date attribute, those whose value is at or
    /// after the one given, read by its column: a window's rows, never the
    /// type whole (ADR-0210 decision 2).
    pub fn of_type_where_since(
        &self,
        type_: &str,
        states: Option<&BTreeSet<String>>,
        columns: &[(String, Value)],
        since: Option<(&str, &Value)>,
    ) -> Result<Vec<&Object>, crate::eval::NotRun> {
        let wanted: Option<Vec<String>> = states.map(|s| s.iter().cloned().collect());
        let pairs: Vec<(&str, &Value)> = columns.iter().map(|(a, v)| (a.as_str(), v)).collect();
        for t in self.family(type_).iter() {
            let whole = self.held.whole.borrow().contains(t);
            if !whole && self.scanned_past(t) {
                self.hold_type(t)?;
            } else if !whole && let Some(conn) = self.conn {
                let fetched = crate::store::read_type_where_since(
                    conn,
                    self.lib,
                    t,
                    wanted.as_deref(),
                    &pairs,
                    since,
                )
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
                && since.is_none_or(|(a, from)| match (o.attr(a), from) {
                    (Value::Timestamp(x), Value::Timestamp(f)) => x >= *f,
                    (Value::Date(x), Value::Date(f)) => x >= *f,
                    _ => false,
                })
        };
        // the held objects by the index on a pair's column, where its value
        // is one the index holds, never every held object (PERF-SCAN)
        if let Some(found) = pairs
            .iter()
            .find_map(|(a, v)| self.held_where(type_, a, v, true, Keying::Value))
        {
            return Ok(found.into_iter().filter(|o| holds(o)).collect());
        }
        Ok(self.held_in_order(holds))
    }

    /// The objects of a type, its own or a subtype's, after a place in
    /// creation order, in the states and holding the values given as
    /// [`World::of_type_where`] narrows, read from each concrete type's
    /// table at most `first` at a time: those certain to come next, up to
    /// the earliest last place among the tables that gave as many as asked,
    /// so none is passed over, in ascending id order; and whether more may
    /// follow. A query's page reads from its cursor so, where each page read
    /// its type whole, which made reading a result page by page quadratic
    /// (READ-PAGES).
    pub fn of_type_from(
        &self,
        type_: &str,
        states: Option<&BTreeSet<String>>,
        columns: &[(String, Value)],
        after: i64,
        first: usize,
    ) -> Result<(Vec<&Object>, bool), crate::eval::NotRun> {
        let wanted: Option<Vec<String>> = states.map(|s| s.iter().cloned().collect());
        let pairs: Vec<(&str, &Value)> = columns.iter().map(|(a, v)| (a.as_str(), v)).collect();
        // the earliest last place among the tables read to their limit
        let mut cut: Option<i64> = None;
        for t in self.family(type_).iter() {
            let whole = self.held.whole.borrow().contains(t);
            if !whole && let Some(conn) = self.conn {
                let fetched = crate::store::read_type_from(
                    conn,
                    self.lib,
                    t,
                    wanted.as_deref(),
                    &pairs,
                    after,
                    first,
                )
                .map_err(|e| {
                    crate::eval::NotRun::Unreadable(format!(
                        "the store's objects of {t} from a place, which could not be read: {e}"
                    ))
                })?;
                match fetched {
                    Some(fetched) => {
                        if fetched.len() >= first
                            && let Some(last) = fetched.last()
                        {
                            cut = Some(cut.map_or(last.order, |c| c.min(last.order)));
                        }
                        self.hold_fetched(fetched)?;
                    }
                    None => self.hold_type(t)?,
                }
            }
        }
        let holds = |o: &Object| {
            o.order > after
                && cut.is_none_or(|c| o.order <= c)
                && self.lib.is_a(&o.type_, type_)
                && states.is_none_or(|s| s.contains(&o.state))
                && pairs.iter().all(|(a, v)| {
                    let x = o.attr(a);
                    x.is_absent() || x.equals(v)
                })
        };
        Ok((self.held_in_order(holds), cut.is_some()))
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
        for t in self.family(type_).iter() {
            let whole = self.held.whole.borrow().contains(t);
            if !whole && let Some(conn) = self.conn {
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
        if let Some(found) = pairs
            .iter()
            .find_map(|(a, v)| self.held_where(type_, a, v, false, Keying::Value))
        {
            return Ok(found.into_iter().filter(|o| holds(o)).collect());
        }
        Ok(self.held_in_order(holds))
    }

    /// Every object of a type, its own or a subtype's, whose text attribute
    /// is the text given but for the case of its ASCII letters, in ascending
    /// id order: read by the store's index on the folded column and the
    /// operation's folded column of what it holds, never the type whole,
    /// where a sweep of every actor for each actor checked made a publish
    /// grow with the square of the actors, 70 s of approval with 3,600
    /// (seventh production review PUBLISH-SLOW, ADR-0194 decision 1).
    pub fn with_folded(
        &self,
        type_: &str,
        attr: &str,
        text: &str,
    ) -> Result<Vec<&Object>, crate::eval::NotRun> {
        for t in self.family(type_).iter() {
            let whole = self.held.whole.borrow().contains(t);
            if !whole && let Some(conn) = self.conn {
                let ids = crate::store::read_ids_folded(conn, self.lib, t, attr, text).map_err(|e| {
                    crate::eval::NotRun::Unreadable(format!(
                        "the store's objects of {t} by {attr} without case, which could not be read: {e}"
                    ))
                })?;
                for id in ids {
                    self.get(&id)?;
                }
            }
        }
        let want = text.to_ascii_lowercase();
        let found = self
            .held_where(
                type_,
                attr,
                &Value::Text(text.to_string()),
                false,
                Keying::Folded,
            )
            .unwrap_or_default();
        Ok(found
            .into_iter()
            .filter(|o| {
                self.lib.is_a(&o.type_, type_)
                    && matches!(o.attr(attr), Value::Text(t) if t.to_ascii_lowercase() == want)
            })
            .collect())
    }

    /// The objects of a type with a superseding state that an object
    /// superseded, in id order, by the index on their successor's column,
    /// never the type whole (ADR-0191 decision 5).
    pub fn superseded_into(
        &self,
        type_: &str,
        id: &str,
    ) -> Result<Vec<&Object>, crate::eval::NotRun> {
        let whole = self.held.whole.borrow().contains(type_);
        if !whole && let Some(conn) = self.conn {
            let ids = crate::store::read_superseded_into(conn, type_, id).map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "the store's objects of {type_} superseded by {id}, which could not be read: {e}"
                ))
            })?;
            for i in ids {
                self.get(&i)?;
            }
        }
        Ok(self.held_in_order(|o| o.type_ == type_ && o.superseded_by.as_deref() == Some(id)))
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
            if !self.last_written.contains_key(id)
                && let Some(o) = self.held.slots.get(n)
            {
                self.last_written.insert(id.to_string(), o.attrs.clone());
            }
            self.held.written.insert(id.to_string());
            self.held.changed.get_mut().push(n);
        }
        Ok(slot.and_then(|n| self.held.slots.get_mut(n)))
    }

    /// An object the operation makes or replaces, held from now on as it
    /// is given: refused where the rest of its record was not read, which a
    /// write would replace with what it holds (ADR-0165).
    pub fn insert(&mut self, o: Object) -> Result<(), crate::eval::NotRun> {
        self.insert_shared(std::rc::Rc::new(o))
    }

    /// [`World::insert`] of an object another world may hold too, held
    /// without a copy until a change here makes one (ADR-0222).
    pub fn insert_shared(&mut self, o: std::rc::Rc<Object>) -> Result<(), crate::eval::NotRun> {
        o.held_parts()?;
        self.held.written.insert(o.id.clone());
        let held = self.held.by_id.get_mut().get(&o.id).copied().flatten();
        if !self.last_written.contains_key(&o.id) {
            let was = held
                .and_then(|n| self.held.slots.get(n))
                .map(|p| p.attrs.clone())
                .unwrap_or_default();
            self.last_written.insert(o.id.clone(), was);
        }
        if let Some(n) = held {
            self.held.changed.get_mut().push(n);
        }
        match held.and_then(|n| self.held.slots.slot_mut(n)) {
            Some(slot) => *slot = o,
            None => {
                let id = o.id.clone();
                let n = self.hold_shared(o)?;
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
        // the operation's own last, then the store's by the object's index,
        // where every object's version scanned the whole log back (the
        // audit's cost B1, ADR-0205 decision 12)
        if let Some(e) = self.appended.iter().rfind(|e| e.object_id == id) {
            return Ok(e.declaration_version);
        }
        let stored = match self.conn.filter(|_| self.stored.get().is_none()) {
            Some(conn) => crate::store::read_last_version(conn, id).map_err(|e| {
                crate::eval::NotRun::Unreadable(format!(
                    "{id}'s last event, which could not be read: {e}"
                ))
            })?,
            None => self
                .log()?
                .rfind(|e| e.object_id == id)
                .map(|e| e.declaration_version),
        };
        Ok(stored.unwrap_or(self.lib.version))
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

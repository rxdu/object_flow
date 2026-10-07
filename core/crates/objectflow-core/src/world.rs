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
    /// Its flow data: for the state and each tracked member, the spans it
    /// held each value for, in order (`storage-schema.md`, `of_interval`).
    pub spans: BTreeMap<String, Vec<Span>>,
    /// The attribute write index: for each attribute, part relationship or
    /// collection, the position of the event that last wrote it
    /// (`storage-schema.md`, `of_attribute_write`).
    pub writes: BTreeMap<String, i64>,
    /// The invariants whose violation an admission lets stand on it, until
    /// they hold again (ADR-0054, `of_admission`).
    pub admissions: std::collections::BTreeSet<String>,
    /// Its own erasure is recorded, so a later write of a personal attribute
    /// is refused (`DESIGN.md` §8).
    pub erased: bool,
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
}

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
}

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
        }
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

    /// An object to change, fetched first where it is not held.
    pub fn get_mut(&mut self, id: &str) -> Result<Option<&mut Object>, crate::eval::NotRun> {
        let slot = match self.get(id)? {
            Some(_) => self.held.by_id.get_mut().get(id).copied().flatten(),
            None => None,
        };
        if slot.is_some() {
            self.held.written.insert(id.to_string());
        }
        Ok(slot.and_then(|n| self.held.slots.get_mut(n)))
    }

    /// An object the operation makes or replaces, held from now on as it
    /// is given.
    pub fn insert(&mut self, o: Object) -> Result<(), crate::eval::NotRun> {
        self.held.written.insert(o.id.clone());
        let held = self.held.by_id.get_mut().get(&o.id).copied().flatten();
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

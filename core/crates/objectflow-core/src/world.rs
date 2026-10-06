//! The store's objects as a request reads them: a snapshot taken inside the
//! request's transaction, which the evaluator reads and the request changes
//! before writing what changed back (`DESIGN.md` §6).

use std::collections::{BTreeMap, BTreeSet};

use crate::model::Library;
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

/// The objects of a store, and the time a request reads.
#[derive(Clone, Debug)]
pub struct World<'l> {
    pub lib: &'l Library,
    pub objects: BTreeMap<String, Object>,
    pub now: i64,
    /// Each external identifier, by source and value, and the object that
    /// holds it, a legacy key among them (`storage-schema.md`, `of_external_id`).
    pub external: BTreeMap<(String, String), String>,
    /// The log, in position order: every event recorded, which a metric over
    /// transitions reads (`storage-schema.md`, `of_event`).
    pub events: Vec<Event>,
    /// The position of each event that recorded an admission, which is an
    /// override (`declaration-syntax.md` §6.9, `of_admission`).
    pub admitting: BTreeSet<i64>,
    /// The position of each event the import recorded, whose spans may begin
    /// before it, at the legacy time of the change (ADR-0106).
    pub imported: BTreeSet<i64>,
    /// The attempt log, in the order written, which a metric over
    /// `attempts` or `attempt_counts` reads (`storage-schema.md` §6).
    pub attempts: Vec<crate::store::Attempt>,
}

impl<'l> World<'l> {
    pub fn new(lib: &'l Library, now: i64) -> World<'l> {
        World {
            lib,
            objects: BTreeMap::new(),
            now,
            external: BTreeMap::new(),
            events: Vec::new(),
            admitting: BTreeSet::new(),
            imported: BTreeSet::new(),
            attempts: Vec::new(),
        }
    }

    pub fn get(&self, id: &str) -> Option<&Object> {
        self.objects.get(id)
    }

    /// Every object, in ascending id order: creation order.
    pub fn in_order(&self) -> Vec<&Object> {
        let mut all: Vec<&Object> = self.objects.values().collect();
        all.sort_by_key(|o| o.order);
        all
    }

    /// Every object of a type, in ascending id order.
    pub fn of_type(&self, type_: &str) -> Vec<&Object> {
        self.in_order()
            .into_iter()
            .filter(|o| self.lib.is_a(&o.type_, type_))
            .collect()
    }

    /// The version an object's last event was recorded under, the
    /// installed one for an object with none.
    pub fn version_of(&self, id: &str) -> i64 {
        self.events
            .iter()
            .rev()
            .find(|e| e.object_id == id)
            .map_or(self.lib.version, |e| e.declaration_version)
    }

    /// The next place in creation order.
    pub fn next_order(&self) -> i64 {
        self.objects.values().map(|o| o.order).max().unwrap_or(0) + 1
    }
}

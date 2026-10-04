//! The store's objects as a request reads them: a snapshot taken inside the
//! request's transaction, which the evaluator reads and the request changes
//! before writing what changed back (`DESIGN.md` §6).

use std::collections::BTreeMap;

use crate::model::Library;
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
}

impl<'l> World<'l> {
    pub fn new(lib: &'l Library, now: i64) -> World<'l> {
        World {
            lib,
            objects: BTreeMap::new(),
            now,
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
            .filter(|o| o.type_ == type_)
            .collect()
    }

    /// The next place in creation order.
    pub fn next_order(&self) -> i64 {
        self.objects.values().map(|o| o.order).max().unwrap_or(0) + 1
    }
}

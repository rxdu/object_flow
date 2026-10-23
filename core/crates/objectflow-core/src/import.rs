//! The import: a port of another system's records into this store's mirror
//! types, one transaction per batch, each object written through the
//! built-in assertion, each class of violation decided by a person in the
//! disposition file (`publish-and-import.md` §4, §6, ADR-0154).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value as Json, json};

use crate::eval::NotRun;
use crate::model::{ANY_TYPE, Declared, Library, Type};
use crate::request::{failing_invariants, resolve_actor, text_of};
use crate::store::{Allocator, Change, Event, Store, StoreError};
use crate::value::Value;
use crate::wire;
use crate::world::{Object, Span, World};

/// When an imported object took its current value of one dimension, and who
/// changed it, where the legacy record says (`library-api.md` §5).
#[derive(Clone, Debug, PartialEq)]
pub struct EnteredAt {
    pub at: i64,
    pub by: Option<String>,
    pub by_kind: Option<String>,
}

/// A legacy entry as a batch gives it (ADR-0163 decisions 2 and 4): its
/// kind, when it happened, its fields, and each field about another object
/// mapped to that object's `<Type>:<key>`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntryIn {
    pub kind: String,
    pub occurred_at: i64,
    pub payload: serde_json::Map<String, Json>,
    pub about: BTreeMap<String, String>,
}

/// One object of a port, or of a mirror's refresh, as the mapping produced
/// it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportedObject {
    pub type_: String,
    pub legacy_key: String,
    pub state: String,
    /// Each attribute's value in the wire's notation, a reference by the
    /// legacy key of the object it names.
    pub attributes: BTreeMap<String, Json>,
    /// `state` or a tracked member, and when it took its current value.
    pub entered: BTreeMap<String, EnteredAt>,
    pub created_at: Option<i64>,
    pub created_by_kind: Option<String>,
    /// Earlier values of the state and of tracked members, which the legacy
    /// record gives (ADR-0086, ADR-0106).
    pub legacy_intervals: Vec<LegacyInterval>,
    /// Where the legacy record is silent, each from its start to its end; no
    /// legacy interval may overlap one (ADR-0106, ADR-0147).
    pub silent_spans: Vec<(i64, i64)>,
    /// An observation's occurred time, where the legacy record gives it; a
    /// label's is when it was recorded (`library-api.md` §5).
    pub occurred_at: Option<i64>,
    /// Its history from before the store, each entry its own fields and,
    /// in `about`, those describing another object (ADR-0163).
    pub legacy_entries: Vec<EntryIn>,
    /// What the object carries that the core does not yet import, by name.
    pub beyond: Vec<String>,
}

/// A span the legacy record gives, of a value an object held before its
/// current one (`library-api.md` §5, ADR-0106): its value in the wire's
/// notation, a reference by legacy key; its end, where the record gives
/// none, the next value's entry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LegacyInterval {
    pub dimension: String,
    pub value: Json,
    pub entered_at: i64,
    pub left_at: Option<i64>,
    pub entered_by: Option<String>,
    pub entered_by_kind: Option<String>,
}

/// The last value the legacy system minted from a sequence in a scope, the
/// scope empty for an unscoped sequence (ADR-0154, C36).
#[derive(Clone, Debug, PartialEq)]
pub struct Raise {
    pub sequence: String,
    pub scope: String,
    pub last: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportBatch {
    pub objects: Vec<ImportedObject>,
    pub sequences: Vec<Raise>,
    /// Each legacy entry kind's fields kept through an erasure; a kind it
    /// does not list keeps nothing (ADR-0078, ADR-0163 decision 3).
    pub legacy_kinds: BTreeMap<String, Vec<String>>,
}

/// What kind of rule a class of objects fails (`publish-and-import.md` §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ViolationKind {
    Invariant,
    Guard,
    Reference,
    Field,
}

impl ViolationKind {
    pub fn text(self) -> &'static str {
        match self {
            ViolationKind::Invariant => "invariant",
            ViolationKind::Guard => "guard",
            ViolationKind::Reference => "reference",
            ViolationKind::Field => "field",
        }
    }

    pub fn parse(s: &str) -> Option<ViolationKind> {
        Some(match s {
            "invariant" => ViolationKind::Invariant,
            "guard" => ViolationKind::Guard,
            "reference" => ViolationKind::Reference,
            "field" => ViolationKind::Field,
            _ => return None,
        })
    }
}

/// What a person decided for a class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    Undecided,
    CleanUpstream,
    Admit,
    Exclude,
    Drop,
}

impl Disposition {
    pub fn text(self) -> &'static str {
        match self {
            Disposition::Undecided => "undecided",
            Disposition::CleanUpstream => "clean_upstream",
            Disposition::Admit => "admit",
            Disposition::Exclude => "exclude",
            Disposition::Drop => "drop",
        }
    }

    pub fn parse(s: &str) -> Option<Disposition> {
        Some(match s {
            "undecided" => Disposition::Undecided,
            "clean_upstream" => Disposition::CleanUpstream,
            "admit" => Disposition::Admit,
            "exclude" => Disposition::Exclude,
            "drop" => Disposition::Drop,
            _ => return None,
        })
    }
}

/// Every object failing the same rule in the same way, as one decision; its
/// identity is its type, kind, rule and shape (ADR-0154, C31).
#[derive(Clone, Debug, PartialEq)]
pub struct ViolationClass {
    pub id: String,
    pub type_: String,
    pub kind: ViolationKind,
    pub rule: String,
    pub shape: String,
    pub count: usize,
    /// Legacy keys, since nothing is created yet.
    pub sample: Vec<String>,
    pub disposition: Disposition,
    pub decided_by: Option<String>,
    pub reason: Option<String>,
    /// What excluding it would uncover: each reference class the exclusion
    /// would add, followed through references as if each were excluded in
    /// turn; none where the file excludes it, its consequences being
    /// reported as classes of their own (ADR-0212). Report data, as `count`
    /// and `sample` are, which deciding the class leaves aside.
    pub uncovers: Vec<Uncovered>,
}

/// A reference class excluding another would add, and how many objects it
/// would hold (ADR-0212).
#[derive(Clone, Debug, PartialEq)]
pub struct Uncovered {
    pub id: String,
    pub type_: String,
    pub rule: String,
    pub count: usize,
}

impl ViolationClass {
    fn same(&self, other: &ViolationClass) -> bool {
        (&self.type_, self.kind, &self.rule, &self.shape)
            == (&other.type_, other.kind, &other.rule, &other.shape)
    }
}

/// The first object an import could not place, and why (ADR-0154, D525).
#[derive(Clone, Debug, PartialEq)]
pub struct ImportRefusal {
    pub type_: String,
    pub legacy_key: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportReport {
    /// Objects written; zero on a dry run and on a refused batch.
    pub written: usize,
    pub findings: Vec<ViolationClass>,
    /// Owned types the batch named, which the import may not write.
    pub refused_types: Vec<String>,
    pub refusal: Option<ImportRefusal>,
}

/// The earliest time an import takes, 1900-01-01: a legacy system's
/// empty date, year 0001 among them, refused rather than taken as when
/// something happened (fourth production review A-17).
const IMPORT_FLOOR: i64 = -2_208_988_800;

/// The shapes of a class, fixed texts, never a value (ADR-0154, C31).
const DOES_NOT_HOLD: &str = "does not hold";
const IS_ABSENT: &str = "is absent";
const NOT_DECLARED: &str = "is not declared";
const NOT_OF_ITS_TYPE: &str = "is not of its type";
const IS_SHARED: &str = "is shared";
const NAMES_EXCLUDED: &str = "names an excluded object";
/// A `REFERENCE` class's shape: a legacy key neither the batch nor the store
/// has, which the person decides as a reference into an excluded object is
/// (ADR-0176 decision 3).
const NAMES_NOTHING: &str = "names nothing";
/// A `FIELD` class's shape: a field an entry says is about an object the
/// batch, as it stands, and the store do not hold (ADR-0163 decision 4).
const ABOUT_NOT_HELD: &str = "is about an object neither the batch nor the store holds";

/// Import a batch, or report what importing it would decide (`dry_run`).
pub fn import_batch(
    store: &mut Store,
    actor: &str,
    batch: &ImportBatch,
    file: &[ViolationClass],
    dry_run: bool,
) -> Result<Result<ImportReport, NotRun>, StoreError> {
    store.transact(|world, alloc| decide(world, alloc, actor, batch, file, dry_run))
}

/// A reference an object gives by legacy key, resolved once every object of
/// the batch has its id.
struct Reference {
    index: usize,
    attribute: String,
    named: Vec<(String, String)>,
    optional: bool,
}

/// One object as the import will write it.
struct Placed {
    index: usize,
    id: String,
    /// The object as it stands before the import, for a refresh.
    held: Option<Object>,
    state: String,
    values: BTreeMap<String, Value>,
}

/// The legacy key a field's `about` names, `<Type>:<key>`, qualified as the
/// store keeps it; none where it names no type.
fn about_target(lib: &Library, target: &str) -> Option<(String, String)> {
    let (tn, key) = target.split_once(':')?;
    lib.type_(tn).map(|_| legacy(lib, tn, key))
}

/// Whether the store, or the batch as it stands, holds the object a legacy
/// key names.
fn held_key(
    key: &(String, String),
    ids: &BTreeMap<(String, String), String>,
    excluded: &BTreeSet<(String, String)>,
    external: &BTreeMap<(String, String), String>,
) -> bool {
    external.contains_key(key) || (ids.contains_key(key) && !excluded.contains(key))
}

/// A legacy entry of an object, its ordinal given when it is attached.
fn entry(
    e: &EntryIn,
    payload: serde_json::Map<String, Json>,
    kept: &[String],
) -> crate::world::LegacyEntry {
    crate::world::LegacyEntry {
        ordinal: 0,
        kind: e.kind.clone(),
        occurred_at: e.occurred_at,
        payload,
        kept: kept.to_vec(),
    }
}

/// A refusal of the batch at an object.
fn refusal(o: &ImportedObject, reason: impl Into<String>) -> ImportRefusal {
    ImportRefusal {
        type_: o.type_.clone(),
        legacy_key: o.legacy_key.clone(),
        reason: reason.into(),
    }
}

/// The type a family's legacy keys are qualified by: its farthest base, or
/// itself where it extends nothing (ADR-0154, C23).
fn root<'l>(lib: &'l Library, type_: &'l str) -> &'l str {
    let mut at = type_;
    let mut seen = BTreeSet::new();
    while let Some(base) = lib.type_(at).and_then(|t| t.extends.as_deref()) {
        if !seen.insert(base) || lib.type_(base).is_none() {
            break;
        }
        at = base;
    }
    at
}

fn legacy(lib: &Library, type_: &str, key: &str) -> (String, String) {
    ("legacy".to_string(), format!("{}:{key}", root(lib, type_)))
}

fn decide(
    world: &World<'_>,
    alloc: &mut Allocator,
    actor: &str,
    batch: &ImportBatch,
    file: &[ViolationClass],
    dry_run: bool,
) -> Result<(ImportReport, Option<Change>), NotRun> {
    let lib = world.lib;
    // every external identifier, which an import reads whole, as it reads
    // the keys of every object it places and every reference it resolves
    let external = world.external_all()?;
    let mut report = ImportReport::default();
    if let Some(what) = batch.objects.iter().find_map(|o| o.beyond.first()) {
        return Err(NotRun::Unsupported(format!("an import of {what}")));
    }
    let refused = |mut report: ImportReport, r: ImportRefusal| {
        report.refusal = Some(r);
        Ok((report, None))
    };
    let who = match resolve_actor(world, actor)? {
        Some(who) if !who.ended => who,
        found => {
            let reason = if found.is_some() {
                format!("actor_active: the actor {actor} is in a final state")
            } else {
                format!("actor_known: the store holds no actor {actor}")
            };
            return refused(
                report,
                ImportRefusal {
                    type_: String::new(),
                    legacy_key: String::new(),
                    reason,
                },
            );
        }
    };
    // a raise names a sequence the flow's identifiers number by, in a scope
    // they take, with a last value that leaves a next: one naming no such
    // sequence, or past what a sequence draws, is refused, where it was
    // stored, wrapped, and stopped every write (fourth production review
    // A-15)
    for r in &batch.sequences {
        let numbering: Vec<&crate::model::Identifier> = lib
            .types
            .values()
            .flat_map(|t| {
                t.attributes
                    .iter()
                    .filter_map(|(_, a)| a.identifier.as_ref())
            })
            .filter(|i| i.sequence == r.sequence)
            .collect();
        let why = if numbering.is_empty() && !lib.sequences.contains(&r.sequence) {
            Some(format!(
                "sequences: {} is no sequence the flow declares",
                r.sequence
            ))
        } else if r.last < 0 || r.last >= crate::store::SEQUENCE_MOST {
            Some(format!(
                "sequences: {}'s last value {} leaves it no next number",
                r.sequence, r.last
            ))
        } else if !r.scope.is_empty() && numbering.iter().all(|i| i.scope.is_none()) {
            Some(format!(
                "sequences: {} numbers in no scope, not {}",
                r.sequence, r.scope
            ))
        } else {
            None
        };
        if let Some(reason) = why {
            return refused(
                report,
                ImportRefusal {
                    type_: String::new(),
                    legacy_key: String::new(),
                    reason,
                },
            );
        }
    }
    // owned types, which the import may not write (C33); an observation or
    // a label it writes on a mirror's object (publish-and-import.md §4)
    let importable = |t: &Type| t.mirror || t.observation_on.is_some();
    for o in &batch.objects {
        if lib.type_(&o.type_).is_some_and(|t| !importable(t))
            && !report.refused_types.contains(&o.type_)
        {
            report.refused_types.push(o.type_.clone());
        }
    }
    // every legacy key resolved before anything is placed
    let mut ids: BTreeMap<(String, String), String> = BTreeMap::new();
    let mut placed: Vec<Placed> = Vec::new();
    for (index, o) in batch.objects.iter().enumerate() {
        let Some(t) = lib.type_(&o.type_) else {
            return refused(report, refusal(o, format!("names no type {}", o.type_)));
        };
        if !importable(t) {
            return refused(
                report,
                refusal(
                    o,
                    format!("{} is owned by this store, not a mirror", o.type_),
                ),
            );
        }
        if o.occurred_at.is_some() && t.observation_on.is_none() {
            return refused(
                report,
                refusal(o, "an occurred time, which only an observation takes"),
            );
        }
        if o.occurred_at.is_some_and(|at| at > world.now) {
            return refused(report, refusal(o, "an occurred time after the import"));
        }
        // nor a creation, an entry into a state, an interval or a silent span
        // after it, where a creation in 9999 was taken; and no time of the
        // record before 1900, a legacy system's empty date rather than when
        // anything happened, where one in 0001 skewed a metric for good
        // (fourth production review A-17)
        let times: Vec<i64> = o
            .created_at
            .into_iter()
            .chain(o.occurred_at)
            .chain(o.entered.values().map(|e| e.at))
            .chain(
                o.legacy_intervals
                    .iter()
                    .flat_map(|i| std::iter::once(i.entered_at).chain(i.left_at)),
            )
            .chain(o.silent_spans.iter().flat_map(|(a, b)| [*a, *b]))
            .chain(o.legacy_entries.iter().map(|e| e.occurred_at))
            .collect();
        if times.iter().any(|at| *at > world.now) {
            return refused(report, refusal(o, "a time after the import"));
        }
        if times.iter().any(|at| *at < IMPORT_FLOOR) {
            return refused(
                report,
                refusal(
                    o,
                    "a time before 1900, which a legacy system writes for a date it lacks",
                ),
            );
        }
        // a kind of actor a record names is one an actor has, and an actor
        // it names an identity as a request's is, where `robot` and one of
        // 70,000 characters were taken (seventh production review,
        // ADR-0194 decision 6)
        let kinds = o
            .created_by_kind
            .iter()
            .chain(o.entered.values().filter_map(|e| e.by_kind.as_ref()))
            .chain(
                o.legacy_intervals
                    .iter()
                    .filter_map(|i| i.entered_by_kind.as_ref()),
            );
        if let Some(k) = kinds
            .into_iter()
            .find(|k| !matches!(k.as_str(), "human" | "agent" | "service" | "unknown"))
        {
            return refused(
                report,
                refusal(
                    o,
                    format!("a kind of actor {k}, where one is human, agent, service or unknown"),
                ),
            );
        }
        let named = o.entered.values().filter_map(|e| e.by.as_deref()).chain(
            o.legacy_intervals
                .iter()
                .filter_map(|i| i.entered_by.as_deref()),
        );
        if let Some(why) = named.into_iter().find_map(|by| {
            (by.len() > crate::wire::TEXT_BYTES)
                .then(|| format!("past {} bytes", crate::wire::TEXT_BYTES))
                .or_else(|| crate::wire::identity_fault(by, lib))
        }) {
            return refused(report, refusal(o, format!("an actor it names: {why}")));
        }
        // a legacy entry names its kind, and happened before the import and
        // not before a creation time the mapping supplies (§4)
        for e in &o.legacy_entries {
            if e.kind.trim().is_empty() {
                return refused(report, refusal(o, "a legacy entry that names no kind"));
            }
            if e.occurred_at > world.now {
                return refused(report, refusal(o, "a legacy entry after the import"));
            }
            if o.created_at.is_some_and(|c| e.occurred_at < c) {
                return refused(
                    report,
                    refusal(
                        o,
                        "a legacy entry earlier than the creation time it is given",
                    ),
                );
            }
        }
        if let Some(what) = t.beyond.first() {
            return Err(NotRun::Unsupported(what.clone()));
        }
        let key = legacy(lib, &o.type_, &o.legacy_key);
        if ids.contains_key(&key) {
            return refused(
                report,
                refusal(o, "its legacy key appears twice in the batch"),
            );
        }
        let held = match external.get(&key) {
            Some(id) => match world.get(id)? {
                // a refresh carries the held object's record, read first
                Some(h) if h.type_ == o.type_ => {
                    world.parts(h)?;
                    Some(h.clone())
                }
                Some(h) => {
                    return refused(
                        report,
                        refusal(o, format!("its legacy key names a {}", h.type_)),
                    );
                }
                None => None,
            },
            None => None,
        };
        let id = match &held {
            Some(h) => h.id.clone(),
            None => alloc.object_id(&o.type_),
        };
        ids.insert(key, id.clone());
        placed.push(Placed {
            index,
            id,
            held,
            state: o.state.clone(),
            values: BTreeMap::new(),
        });
    }
    // each object's values; what is not of its declaration is a guard class
    // its objects can be excluded by (C35)
    let mut classes: Vec<(ViolationClass, BTreeSet<usize>)> = Vec::new();
    let found = |classes: &mut Vec<(ViolationClass, BTreeSet<usize>)>,
                 o: &ImportedObject,
                 index: usize,
                 kind: ViolationKind,
                 rule: String,
                 shape: &str| {
        let class = ViolationClass {
            id: format!("{}.{rule}: {shape}", o.type_),
            type_: o.type_.clone(),
            kind,
            rule,
            shape: shape.to_string(),
            count: 0,
            sample: Vec::new(),
            disposition: Disposition::Undecided,
            decided_by: None,
            reason: None,
            uncovers: Vec::new(),
        };
        match classes.iter_mut().find(|(c, _)| c.same(&class)) {
            Some((_, objects)) => {
                objects.insert(index);
            }
            None => classes.push((class, BTreeSet::from([index]))),
        }
    };
    // references kept by legacy key until every object is placed
    let mut references: Vec<Reference> = Vec::new();
    for p in &mut placed {
        let o = &batch.objects[p.index];
        let Some(t) = lib.type_(&o.type_) else {
            continue;
        };
        if t.state(&o.state).is_none() {
            found(
                &mut classes,
                o,
                p.index,
                ViolationKind::Guard,
                "state".into(),
                NOT_DECLARED,
            );
        }
        // an observation the store records at the import, occurred when the
        // legacy record says, or then
        if t.observation_on.is_some() {
            let at = |v: i64| Value::Timestamp(v);
            p.values.insert("recorded_at".into(), at(world.now));
            p.values
                .insert("occurred_at".into(), at(o.occurred_at.unwrap_or(world.now)));
        }
        for (k, raw) in &o.attributes {
            let Some(a) = t.attribute(k) else {
                return refused(
                    report,
                    refusal(o, format!("{} declares no attribute {k}", o.type_)),
                );
            };
            if raw.is_null() {
                continue;
            }
            // an erased object's personal values are not written again
            let erased = match &p.held {
                Some(h) => h.held_parts()?.erased,
                None => false,
            };
            if a.personal && erased {
                continue;
            }
            if let Declared::Reference(target) = &a.declared {
                let keys: Vec<&Json> = match raw {
                    Json::Array(items) => items.iter().collect(),
                    one => vec![one],
                };
                let mut named = Vec::new();
                for key in keys {
                    let Some(key) = key.as_str() else {
                        found(
                            &mut classes,
                            o,
                            p.index,
                            ViolationKind::Guard,
                            k.clone(),
                            NOT_OF_ITS_TYPE,
                        );
                        continue;
                    };
                    // a reference to any type, a label's subject, names its
                    // object's type with its key, `<Type>:<key>`, the form
                    // the store keeps a legacy key in
                    if target == ANY_TYPE {
                        match key.split_once(':') {
                            Some((tn, k)) if lib.type_(tn).is_some() => {
                                named.push(legacy(lib, tn, k));
                            }
                            _ => found(
                                &mut classes,
                                o,
                                p.index,
                                ViolationKind::Guard,
                                k.clone(),
                                NOT_OF_ITS_TYPE,
                            ),
                        }
                        continue;
                    }
                    named.push(legacy(lib, target, key));
                }
                references.push(Reference {
                    index: p.index,
                    attribute: k.clone(),
                    named,
                    optional: a.optional,
                });
                continue;
            }
            match wire::decode(raw, &a.declared, a.many, lib) {
                Ok(v) => {
                    p.values.insert(k.clone(), v);
                }
                Err(_) => {
                    let declared_enum = matches!(&a.declared, Declared::Scalar(s) if lib.enumerations.contains_key(s));
                    let shape = if declared_enum {
                        NOT_DECLARED
                    } else {
                        NOT_OF_ITS_TYPE
                    };
                    found(
                        &mut classes,
                        o,
                        p.index,
                        ViolationKind::Guard,
                        k.clone(),
                        shape,
                    );
                }
            }
        }
    }
    // a key naming nothing in the batch or the store, an earlier file's
    // exclusion included since the store keeps no record of one, is a class
    // of its own, so one dry run lists every such reference (ADR-0176
    // decision 3, C29)
    let names_nothing = |named: &[(String, String)]| {
        named
            .iter()
            .any(|key| !ids.contains_key(key) && !external.contains_key(key))
    };
    // an observation or a label is on a mirror's object, in the batch or the
    // store: the import writes nothing an owned type's own requests would
    let placed_types: BTreeMap<&str, &str> = placed
        .iter()
        .map(|p| (p.id.as_str(), batch.objects[p.index].type_.as_str()))
        .collect();
    for r in references.iter().filter(|r| r.attribute == "subject") {
        let o = &batch.objects[r.index];
        if lib
            .type_(&o.type_)
            .is_none_or(|t| t.observation_on.is_none())
        {
            continue;
        }
        for key in &r.named {
            let Some(id) = ids.get(key).or_else(|| external.get(key)) else {
                continue;
            };
            let subject = match placed_types.get(id.as_str()) {
                Some(t) => (*t).to_string(),
                None => world.get(id)?.map(|s| s.type_.clone()).unwrap_or_default(),
            };
            if lib.type_(&subject).is_none_or(|t| !t.mirror) {
                return refused(
                    report,
                    refusal(
                        o,
                        format!(
                            "an observation on a {subject}, which this store owns, not a mirror"
                        ),
                    ),
                );
            }
        }
    }
    // place what is not excluded, find the classes over it, and exclude
    // what the file excludes, until nothing more is
    let decided = |c: &ViolationClass| file.iter().find(|f| f.same(c));
    let excluded_by_file = |classes: &[(ViolationClass, BTreeSet<usize>)]| -> BTreeSet<usize> {
        classes
            .iter()
            .filter(|(c, _)| decided(c).is_some_and(|f| f.disposition == Disposition::Exclude))
            .flat_map(|(_, objects)| objects.iter().copied())
            .collect()
    };
    let structural = classes.clone();
    let mut excluded: BTreeSet<usize> = excluded_by_file(&structural);
    let (mut work, mut found_classes);
    loop {
        work = world.clone();
        let mut these = structural.clone();
        // references into an excluded object, which the store does not hold
        let excluded_keys: BTreeSet<(String, String)> = excluded
            .iter()
            .map(|i| {
                let o = &batch.objects[*i];
                legacy(lib, &o.type_, &o.legacy_key)
            })
            .filter(|k| !external.contains_key(k))
            .collect();
        let mut dropped: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
        for Reference {
            index,
            attribute: k,
            named,
            ..
        } in &references
        {
            if excluded.contains(index) {
                continue;
            }
            if names_nothing(named) {
                let o = &batch.objects[*index];
                found(
                    &mut these,
                    o,
                    *index,
                    ViolationKind::Reference,
                    k.clone(),
                    NAMES_NOTHING,
                );
                dropped.entry(*index).or_default().insert(k.clone());
            } else if named.iter().any(|key| excluded_keys.contains(key)) {
                let o = &batch.objects[*index];
                found(
                    &mut these,
                    o,
                    *index,
                    ViolationKind::Reference,
                    k.clone(),
                    NAMES_EXCLUDED,
                );
                dropped.entry(*index).or_default().insert(k.clone());
            }
        }
        // a field an entry says is about another object, where neither the
        // batch, as it stands, nor the store holds that object (ADR-0163)
        for p in placed.iter().filter(|p| !excluded.contains(&p.index)) {
            let o = &batch.objects[p.index];
            for e in &o.legacy_entries {
                for (field, target) in &e.about {
                    if about_target(lib, target)
                        .is_none_or(|k| !held_key(&k, &ids, &excluded_keys, external))
                    {
                        found(
                            &mut these,
                            o,
                            p.index,
                            ViolationKind::Field,
                            format!("{}.{field}", e.kind),
                            ABOUT_NOT_HELD,
                        );
                    }
                }
            }
        }
        for p in placed.iter().filter(|p| !excluded.contains(&p.index)) {
            let o = &batch.objects[p.index];
            let mut obj = match &p.held {
                Some(h) => h.clone(),
                None => Object {
                    id: p.id.clone(),
                    type_: o.type_.clone(),
                    state: p.state.clone(),
                    version: 0,
                    attrs: BTreeMap::new(),
                    created_at: world.now,
                    created_by_kind: "unknown".into(),
                    order: work.next_order()?,
                    superseded_by: None,
                    parts: crate::world::Parts::default().held(),
                    state_source: None,
                    created_dated: false,
                    recorded_from: world.now,
                },
            };
            obj.state = p.state.clone();
            if let Some(t) = lib.type_(&o.type_) {
                for (a, spec) in &t.attributes {
                    if spec.stored_set() && !obj.attrs.contains_key(a) {
                        obj.attrs.insert(a.clone(), Value::Set(Vec::new()));
                    }
                }
            }
            for (k, v) in &p.values {
                obj.attrs.insert(k.clone(), v.clone());
            }
            for r in &references {
                if r.index != p.index
                    || dropped
                        .get(&r.index)
                        .is_some_and(|d| d.contains(&r.attribute))
                {
                    continue;
                }
                let k = &r.attribute;
                let resolved: Vec<Value> = r
                    .named
                    .iter()
                    .filter_map(|key| ids.get(key).or_else(|| external.get(key)))
                    .map(|id| Value::Object(id.clone()))
                    .collect();
                let many = lib
                    .type_(&o.type_)
                    .and_then(|t| t.attribute(k))
                    .is_some_and(|a| a.many);
                let v = if many {
                    Value::Set(resolved)
                } else {
                    resolved.into_iter().next().unwrap_or(Value::Absent)
                };
                obj.attrs.insert(k.clone(), v);
            }
            work.insert(obj)?;
        }
        // the types holding each external identifier's source, which its
        // uniqueness reads through their columns, never the store whole,
        // which made every batch read every object (PUBLISH-FREEZE)
        let holding = |a: &str, source: &str| -> Vec<String> {
            lib.types
                .iter()
                .filter(|(_, xt)| {
                    xt.attribute(a)
                        .is_some_and(|xa| xa.external.as_deref() == Some(source))
                })
                .map(|(n, _)| n.clone())
                .collect()
        };
        // the classes, found once over every object the batch writes (C34)
        for p in placed.iter().filter(|p| !excluded.contains(&p.index)) {
            let o = &batch.objects[p.index];
            let (Some(t), Some(obj)) = (lib.type_(&o.type_), work.get(&p.id)?) else {
                continue;
            };
            if t.state(&obj.state).is_none() {
                continue; // its guard class is found already
            }
            for (a, spec) in &t.attributes {
                let required = !spec.optional && spec.default.is_none() && !spec.many;
                let counter = matches!(&spec.declared, Declared::Scalar(s) if s == "counter");
                if required
                    && !counter
                    && obj.attr(a).is_absent()
                    && !dropped.get(&p.index).is_some_and(|d| d.contains(a))
                {
                    found(
                        &mut these,
                        o,
                        p.index,
                        ViolationKind::Guard,
                        format!("{a}_provided"),
                        IS_ABSENT,
                    );
                }
                // an external identifier is unique for its source (C22)
                if let (Some(source), v) = (&spec.external, obj.attr(a))
                    && !v.is_absent()
                {
                    let value = text_of(&v);
                    let mut shared = false;
                    for tn in holding(a, source) {
                        shared |= work
                            .with_key(&tn, std::slice::from_ref(a), std::slice::from_ref(&v))?
                            .iter()
                            .any(|x| {
                                x.id != obj.id && x.type_ == tn && text_of(&x.attr(a)) == value
                            });
                    }
                    if shared {
                        found(
                            &mut these,
                            o,
                            p.index,
                            ViolationKind::Guard,
                            format!("{a}_unique"),
                            IS_SHARED,
                        );
                    }
                }
            }
            let admissions = &work.parts(obj)?.admissions;
            for n in failing_invariants(&work, obj)? {
                if !admissions.contains(&n) {
                    found(
                        &mut these,
                        o,
                        p.index,
                        ViolationKind::Invariant,
                        n,
                        DOES_NOT_HOLD,
                    );
                }
            }
        }
        // a reference the file drops is cleared; one it excludes excludes
        let more = excluded_by_file(&these);
        found_classes = these;
        if more.is_subset(&excluded) {
            // apply the drops to the placed objects
            for (index, attrs) in &dropped {
                if let Some(p) = placed.iter().find(|p| p.index == *index)
                    && let Some(obj) = work.get_mut(&p.id)?
                {
                    for a in attrs {
                        obj.attrs.remove(a);
                    }
                }
            }
            break;
        }
        excluded.extend(more);
    }
    // an invariant of an object the batch does not place, which its writes
    // could break through a reference or a scan, refuses the batch when it
    // held before: no disposition admits it, the object being none of the
    // batch's, where a mirror's refresh left an owned laptop held by a
    // person who had left (the audit's write path R-04)
    let placed_ids: BTreeSet<String> = placed.iter().map(|p| p.id.clone()).collect();
    for id in crate::request::affected_by_writes(&work, world)? {
        if placed_ids.contains(&id) {
            continue;
        }
        let (Some(now), Some(was)) = (work.get(&id)?, world.get(&id)?) else {
            continue;
        };
        let held: BTreeSet<String> = failing_invariants(world, was)?.into_iter().collect();
        let admitted = work.parts(now)?.admissions.clone();
        if let Some(n) = failing_invariants(&work, now)?
            .into_iter()
            .find(|n| !held.contains(n) && !admitted.contains(n))
        {
            let type_ = now.type_.clone();
            return refused(
                report,
                ImportRefusal {
                    reason: format!(
                        "the batch breaks {type_}.{n} on {id}, an object it does not place"
                    ),
                    type_,
                    legacy_key: String::new(),
                },
            );
        }
    }
    // what excluding a class would uncover, a round at a time as the dry
    // runs after it would find it: the objects whose references name what
    // the round before excluded, by the class each joins, each then taken
    // as excluded too, so a chain is told whole in the first dry run, where
    // the hiring review's took three (E24, ADR-0212)
    let mut naming: BTreeMap<&(String, String), Vec<&Reference>> = BTreeMap::new();
    for r in &references {
        for key in &r.named {
            naming.entry(key).or_default().push(r);
        }
    }
    let uncovered_by = |objects: &BTreeSet<usize>| -> Vec<Uncovered> {
        let mut gone: BTreeSet<usize> = excluded.union(objects).copied().collect();
        let mut round: BTreeSet<usize> = objects.difference(&excluded).copied().collect();
        let mut joined: BTreeMap<(String, String), BTreeSet<usize>> = BTreeMap::new();
        while !round.is_empty() {
            let mut next = BTreeSet::new();
            for i in &round {
                let o = &batch.objects[*i];
                let key = legacy(lib, &o.type_, &o.legacy_key);
                // one the store holds stays, and nothing naming it is lost
                if external.contains_key(&key) {
                    continue;
                }
                for r in naming.get(&key).into_iter().flatten() {
                    if gone.contains(&r.index) {
                        continue;
                    }
                    joined
                        .entry((batch.objects[r.index].type_.clone(), r.attribute.clone()))
                        .or_default()
                        .insert(r.index);
                    next.insert(r.index);
                }
            }
            gone.extend(next.iter().copied());
            round = next;
        }
        joined
            .into_iter()
            .map(|((type_, rule), held)| Uncovered {
                id: format!("{type_}.{rule}: {NAMES_EXCLUDED}"),
                type_,
                rule,
                count: held.len(),
            })
            .collect()
    };
    // each class as the file decides it, counted and sampled
    for (c, objects) in &found_classes {
        let mut class = c.clone();
        class.count = objects.len();
        class.sample = objects
            .iter()
            .take(5)
            .map(|i| batch.objects[*i].legacy_key.clone())
            .collect();
        if let Some(f) = decided(c) {
            class.disposition = f.disposition;
            class.decided_by = f.decided_by.clone();
            class.reason = f.reason.clone();
        }
        // one the file excludes starts from nothing, its objects excluded
        class.uncovers = uncovered_by(objects);
        report.findings.push(class);
    }
    // a class the file leaves to be decided, or decides wrongly, refuses; a
    // dry run reports a wrong decision too, and leaves the undecided to the
    // person deciding (publish-and-import.md §6)
    for (c, objects) in &found_classes {
        let Some(first) = objects.iter().next().map(|i| &batch.objects[*i]) else {
            continue;
        };
        let f = decided(c);
        let disposition = f.map_or(Disposition::Undecided, |f| f.disposition);
        let reason = f
            .and_then(|f| f.reason.clone())
            .filter(|r| !r.trim().is_empty());
        let wrong = match (c.kind, disposition) {
            (_, Disposition::Undecided) => (!dry_run).then(|| "is undecided".to_string()),
            (_, Disposition::CleanUpstream) => {
                (!dry_run).then(|| "was to be cleaned upstream".to_string())
            }
            // an identity two actors hold, the first made answering for both,
            // which no reason makes right (ADR-0192 decision 6)
            (ViolationKind::Invariant, Disposition::Admit)
                if c.rule == crate::request::IDENTITY_TAKEN =>
            {
                Some(
                    "is an actor's identity another actor holds, which nothing admits: the first made would answer for both"
                        .to_string(),
                )
            }
            (ViolationKind::Invariant, Disposition::Admit) | (_, Disposition::Exclude)
                if reason.is_none() =>
            {
                Some(format!("is decided {} with no reason", disposition.text()))
            }
            (ViolationKind::Invariant, Disposition::Admit) | (_, Disposition::Exclude) => None,
            (ViolationKind::Field, Disposition::Admit) => {
                Some("is a field, which nothing admits".to_string())
            }
            (_, Disposition::Admit) => Some("is a guard, which nothing admits".to_string()),
            (ViolationKind::Reference, Disposition::Drop) => {
                let required = references
                    .iter()
                    .any(|r| objects.contains(&r.index) && r.attribute == c.rule && !r.optional);
                required.then(|| "drops a required reference".to_string())
            }
            (ViolationKind::Field, Disposition::Drop) => None,
            (_, Disposition::Drop) => Some("drops what is not a reference".to_string()),
        };
        if let Some(why) = wrong {
            return refused(
                report,
                refusal(
                    first,
                    format!("its class {}.{} ({}) {why}", c.type_, c.rule, c.shape),
                ),
            );
        }
    }
    // the writes, in the batch's order, which a dry run decides too, every
    // check of them among its own, where it answered clean a batch whose
    // legacy interval of an untracked dimension the run refused (thirteenth
    // production review); it writes nothing
    let mut change = Change::default();
    let port = world.now;
    // the legacy entries: each entry's own fields stay with its object, a
    // field about another object moves to that object's history where the
    // batch or the store holds it, and one a FIELD class decides DROP is left
    // behind; an entry the store already holds is left as it is, and one
    // attached to an erased object is written redacted (ADR-0163, ADR-0078)
    let excluded_keys: BTreeSet<(String, String)> = excluded
        .iter()
        .map(|i| legacy(lib, &batch.objects[*i].type_, &batch.objects[*i].legacy_key))
        .filter(|k| !external.contains_key(k))
        .collect();
    let mut entries: BTreeMap<String, Vec<crate::world::LegacyEntry>> = BTreeMap::new();
    for p in placed.iter().filter(|p| !excluded.contains(&p.index)) {
        let o = &batch.objects[p.index];
        for e in &o.legacy_entries {
            let kept = batch.legacy_kinds.get(&e.kind).cloned().unwrap_or_default();
            let mut own = e.payload.clone();
            for (field, target) in &e.about {
                let value = own.remove(field);
                let held = about_target(lib, target)
                    .filter(|k| held_key(k, &ids, &excluded_keys, external))
                    .and_then(|k| ids.get(&k).or_else(|| external.get(&k)).cloned());
                if let (Some(id), Some(v)) = (held, value) {
                    let payload = serde_json::Map::from_iter([(field.clone(), v)]);
                    entries
                        .entry(id)
                        .or_default()
                        .push(entry(e, payload, &kept));
                }
            }
            if !own.is_empty() {
                entries
                    .entry(p.id.clone())
                    .or_default()
                    .push(entry(e, own, &kept));
            }
        }
    }
    // each object's new entries, after those it holds, the held ones left
    let mut attached: BTreeMap<String, Vec<crate::world::LegacyEntry>> = BTreeMap::new();
    for (id, new) in entries {
        let held_entries = world.legacy_of(&id)?;
        let held = held_entries.as_slice();
        let mut next = held.iter().map(|h| h.ordinal).max().unwrap_or(0);
        let erased = match world.get(&id)? {
            Some(w) => world.parts(w)?.erased,
            None => false,
        };
        for mut n in new {
            let same = |h: &crate::world::LegacyEntry| {
                h.kind == n.kind
                    && h.occurred_at == n.occurred_at
                    && (h.payload == n.payload || h.payload == n.redacted())
            };
            if held.iter().any(same) {
                continue;
            }
            next += 1;
            n.ordinal = next;
            if erased {
                n.payload = n.redacted();
            }
            attached.entry(id.clone()).or_default().push(n);
        }
    }
    for (id, new) in &attached {
        for n in new {
            change.legacy.push((id.clone(), n.clone()));
        }
    }
    for p in placed.iter().filter(|p| !excluded.contains(&p.index)) {
        let o = &batch.objects[p.index];
        let (Some(t), Some(mut obj)) = (lib.type_(&o.type_), work.get(&p.id)?.cloned()) else {
            continue;
        };
        let resolve = |key: &str, target: &str| {
            let k = legacy(lib, target, key);
            ids.get(&k).or_else(|| external.get(&k)).cloned()
        };
        // a refresh's intermediate values, which are a change of their own
        let intermediate = match &p.held {
            Some(h) => match intermediate_spans(lib, t, h.held_parts()?, o, &resolve, port) {
                Ok(spans) => spans,
                Err(r) => return refused(report, refusal(o, r)),
            },
            None => BTreeMap::new(),
        };
        let changed = match &p.held {
            None => true,
            Some(h) => {
                h.state != obj.state
                    || h.attrs != obj.attrs
                    || !intermediate.is_empty()
                    || attached.contains_key(&p.id)
            }
        };
        if !changed {
            continue; // a refresh that changes nothing writes nothing
        }
        let position = alloc.position();
        alloc.txn();
        let refresh = p.held.as_ref().map(|h| (h, &intermediate));
        if let Some(r) = open_spans(lib, t, &mut obj, refresh, o, position, port)? {
            return refused(report, refusal(o, r));
        }
        if p.held.is_none() {
            // the earlier values the legacy record gives, at position 0, and
            // the times the record supplies held to its creation and its
            // silent spans (publish-and-import.md §4)
            match legacy_spans(lib, t, obj.held_parts()?, o, &resolve) {
                Ok(spans) => {
                    for (d, sp) in spans {
                        let row = obj.parts_mut()?.spans.entry(d.as_str().into()).or_default();
                        let at = row.iter().position(|x| !x.legacy).unwrap_or(row.len());
                        row.insert(at, sp.clone());
                        change.spans.push((obj.id.clone(), d, sp));
                    }
                }
                Err(r) => return refused(report, refusal(o, r)),
            }
            created(lib, t, &mut obj, o, port)?;
        }
        // admissions: what an ADMIT class holds, with its reason; one that
        // holds again is discharged (ADR-0054, C14)
        let failing = failing_invariants(&work, &obj)?;
        for n in obj.held_parts()?.admissions.clone() {
            if !failing.contains(&n) {
                obj.parts_mut()?.admissions.remove(&n);
                change.discharged.push((obj.id.clone(), n, position));
            }
        }
        for (c, objects) in &found_classes {
            if c.kind == ViolationKind::Invariant && objects.contains(&p.index) {
                let reason = decided(c)
                    .and_then(|f| f.reason.clone())
                    .unwrap_or_default();
                obj.parts_mut()?.admissions.insert(c.rule.clone());
                change
                    .admitted
                    .push((obj.id.clone(), c.rule.clone(), position, reason));
            }
        }
        obj.version += 1;
        let from_state = p.held.as_ref().map(|h| h.state.clone());
        let before = p.held.as_ref().map(|h| h.attrs.clone()).unwrap_or_default();
        let event = Event {
            position,
            retries: 0,
            batch: None,
            object_id: obj.id.clone(),
            object_seq: obj.version,
            transition: "import".into(),
            from_state: from_state.clone(),
            to_state: obj.state.clone(),
            changes_state: from_state.as_deref() != Some(obj.state.as_str()),
            source: "asserted".into(),
            actor_id: who.id.clone(),
            actor_kind: who.kind.clone(),
            principal: who.principal.clone(),
            cause: None,
            declaration_version: lib.version,
            recorded_at: port,
            occurred_at: Some(port),
            imported: true,
            // what it wrote, which a fold of the log reproduces the row by
            // (ADR-0209 decision 1)
            payload: crate::world::with_wrote(
                json!({ "inputs": o.attributes }),
                crate::world::wrote_between(&before, &obj.attrs),
            ),
            reads: json!({}),
            context: None,
        };
        // what it wrote, in the write index
        for (a, v) in &obj.attrs {
            if before.get(a) != Some(v) || p.held.is_none() {
                change.index.push((obj.id.clone(), a.clone(), position));
            }
        }
        // its legacy key, and the external identifiers its type declares
        let key = legacy(lib, &o.type_, &o.legacy_key);
        change
            .external
            .push((key.0, key.1, obj.id.clone(), String::new()));
        for (a, spec) in &t.attributes {
            if let Some(source) = &spec.external
                && !obj.attr(a).is_absent()
            {
                change.external.push((
                    source.clone(),
                    text_of(&obj.attr(a)),
                    obj.id.clone(),
                    a.clone(),
                ));
            }
        }
        let created = p.held.is_none();
        change.writes.push((obj, created, event));
        report.written += 1;
    }
    for r in &batch.sequences {
        change
            .raised
            .push((r.sequence.clone(), r.scope.clone(), r.last));
    }
    if dry_run {
        report.written = 0;
        return Ok((report, None));
    }
    Ok((report, Some(change)))
}

/// A new object's creation time and kind, whether it is dated, and its
/// `recorded_from` (`publish-and-import.md` §4, ADR-0154, C24, C37): its
/// legacy creation where its state's intervals run unbroken from it, the end
/// of its last silent span, and the port where its state's current interval
/// begins there, the latest of those that apply; for an object none
/// describes, the earliest entry its state's intervals and those supplied
/// for a tracked member give.
fn created(
    lib: &Library,
    t: &Type,
    obj: &mut Object,
    o: &ImportedObject,
    port: i64,
) -> Result<(), NotRun> {
    let tracked = lib.tracked(t);
    let spans = &obj.held_parts()?.spans;
    let entries: Vec<i64> = spans
        .iter()
        .filter(|(m, _)| tracked.iter().any(|t| t.as_str() == &***m))
        .flat_map(|(m, spans)| {
            spans
                .iter()
                .filter(move |sp| &**m == "state" || sp.legacy || o.entered.contains_key(&**m))
                .map(|sp| sp.entered_at)
        })
        .collect();
    let earliest = entries.iter().copied().min();
    let state = spans.get("state").map_or(&[][..], Vec::as_slice);
    let whole = o.created_at.is_some_and(|c| {
        state.first().is_some_and(|first| first.entered_at == c)
            && state
                .windows(2)
                .all(|w| w[0].left_at == Some(w[1].entered_at))
    });
    obj.created_dated = o.created_at.is_some();
    obj.created_at = o.created_at.or(earliest).unwrap_or(port).min(port);
    obj.created_by_kind = o
        .created_by_kind
        .clone()
        .unwrap_or_else(|| "unknown".into());
    let mut apply = Vec::new();
    if whole {
        apply.extend(o.created_at);
    }
    apply.extend(o.silent_spans.iter().map(|(_, end)| *end).max());
    if !o.entered.contains_key("state") {
        apply.push(port);
    }
    obj.recorded_from = apply.into_iter().max().or(earliest).unwrap_or(port);
    Ok(())
}

/// The earlier values the legacy record gives, as spans marked legacy at
/// position 0 (`storage-schema.md`, `of_interval`), or why they are refused:
/// a dimension that is not tracked, a value the declaration does not hold, a
/// span that ends before it begins, overlaps another of its dimension, or
/// reaches into the current one, and any span or entry time earlier than a
/// creation time the mapping supplies or overlapping a silent span
/// (publish-and-import.md §4, ADR-0106, ADR-0147).
fn legacy_spans(
    lib: &Library,
    t: &Type,
    parts: &crate::world::Parts,
    o: &ImportedObject,
    resolve: &dyn Fn(&str, &str) -> Option<String>,
) -> Result<Vec<(String, Span)>, String> {
    for (start, end) in &o.silent_spans {
        if end <= start {
            return Err("a silent span that ends before it begins".into());
        }
    }
    if let Some(c) = o.created_at
        && let Some((m, _)) = o.entered.iter().find(|(_, e)| e.at < c)
    {
        return Err(format!(
            "its {m} entered before the creation time it is given"
        ));
    }
    let tracked = lib.tracked(t);
    // an entry time of a dimension the type does not track is refused, as a
    // legacy interval of one is, where it was taken and its time dropped
    // (fourteenth production review, minor)
    if let Some(m) = o.entered.keys().find(|m| !tracked.contains(m)) {
        return Err(format!(
            "an entry time of {m}, which {} does not track",
            t.name
        ));
    }
    let mut by_dimension: BTreeMap<&str, Vec<&LegacyInterval>> = BTreeMap::new();
    for l in &o.legacy_intervals {
        by_dimension
            .entry(l.dimension.as_str())
            .or_default()
            .push(l);
    }
    let mut out = Vec::new();
    for (d, mut intervals) in by_dimension {
        if !tracked.iter().any(|m| m == d) {
            return Err(format!(
                "a legacy interval of {d}, which {} does not track",
                t.name
            ));
        }
        intervals.sort_by_key(|l| l.entered_at);
        // the current interval opens at its entry, which no earlier one may
        // reach past
        let current = parts
            .spans
            .get(d)
            .and_then(|spans| spans.iter().find(|sp| !sp.legacy))
            .map(|sp| sp.entered_at);
        for (i, l) in intervals.iter().enumerate() {
            let next = intervals.get(i + 1).map(|n| n.entered_at).or(current);
            let left = match l.left_at.or(next) {
                Some(left) => left,
                None => return Err(format!("a legacy interval of {d} with no end")),
            };
            if left < l.entered_at {
                return Err(format!(
                    "a legacy interval of {d} that ends before it begins"
                ));
            }
            if next.is_some_and(|n| left > n) {
                return Err(format!(
                    "a legacy interval of {d} that overlaps the one after it"
                ));
            }
            if o.created_at.is_some_and(|c| l.entered_at < c) {
                return Err(format!(
                    "a legacy interval of {d} earlier than the creation time it is given"
                ));
            }
            if o.silent_spans
                .iter()
                .any(|(start, end)| l.entered_at < *end && *start < left)
            {
                return Err(format!(
                    "a legacy interval of {d} that overlaps a silent span"
                ));
            }
            let value = earlier_value(lib, t, d, &l.value, resolve, "a legacy interval")?;
            out.push((
                d.to_string(),
                Span {
                    value,
                    entered_position: 0,
                    left_position: Some(0),
                    entered_at: l.entered_at,
                    left_at: Some(left),
                    entered_by: l.entered_by.as_deref().map(Into::into),
                    entered_by_kind: l.entered_by_kind.as_deref().unwrap_or("unknown").into(),
                    migrated: false,
                    declaration_version: lib.version,
                    legacy: true,
                    corrected_by: None,
                },
            ));
        }
    }
    Ok(out)
}

/// The value an earlier span holds, decoded as its dimension declares it, a
/// reference resolved by legacy key, or why it is refused: a state or an
/// attribute the type does not declare, a key no object of the batch or the
/// store has, or a value its type cannot hold. `what` names the span in the
/// reason, a legacy interval or a refresh's intermediate value.
fn earlier_value(
    lib: &Library,
    t: &Type,
    d: &str,
    raw: &Json,
    resolve: &dyn Fn(&str, &str) -> Option<String>,
    what: &str,
) -> Result<Value, String> {
    if d == "state" {
        return match raw.as_str() {
            Some(s) if t.state(s).is_some() => Ok(Value::State {
                type_: t.name.clone(),
                name: s.to_string(),
            }),
            _ => Err(format!("{what} in a state {} does not declare", t.name)),
        };
    }
    let Some(a) = t.attribute(d) else {
        return Err(format!("{what} of {d}, which {} does not declare", t.name));
    };
    match (&a.declared, raw) {
        (_, Json::Null) => Ok(Value::Absent),
        (Declared::Reference(target), Json::String(key)) => match resolve(key, target) {
            Some(id) => Ok(Value::Object(id)),
            None => Err(format!(
                "{what} of {d} naming {key}, which no object of the batch or the store has as its legacy key"
            )),
        },
        (declared, raw) => wire::decode(raw, declared, false, lib)
            .map_err(|_| format!("{what} of {d} holding a value it cannot")),
    }
}

/// A refresh's intermediate values (`publish-and-import.md` §4, ADR-0106):
/// the values a dimension held in the legacy record between the one the
/// store holds and the one the refresh brings, each opened and closed on the
/// refresh's event, so a change made in the legacy system while the type is
/// a mirror keeps its time and who made it. One the store already holds is
/// left as it is, so a batch sent again writes nothing; the rest are refused
/// where the dimension is not tracked, a value is not declared, one enters
/// before the latest interval the store holds or at or after the value
/// following it, or ends before it begins or after the next one begins.
/// Their positions are the event's, which `open_spans` gives them.
fn intermediate_spans(
    lib: &Library,
    t: &Type,
    held_parts: &crate::world::Parts,
    o: &ImportedObject,
    resolve: &dyn Fn(&str, &str) -> Option<String>,
    port: i64,
) -> Result<BTreeMap<String, Vec<Span>>, String> {
    let tracked = lib.tracked(t);
    // an entry time of a dimension the type does not track is refused, as a
    // legacy interval of one is, where it was taken and its time dropped
    // (fourteenth production review, minor)
    if let Some(m) = o.entered.keys().find(|m| !tracked.contains(m)) {
        return Err(format!(
            "an entry time of {m}, which {} does not track",
            t.name
        ));
    }
    let mut by_dimension: BTreeMap<&str, Vec<&LegacyInterval>> = BTreeMap::new();
    for l in &o.legacy_intervals {
        by_dimension
            .entry(l.dimension.as_str())
            .or_default()
            .push(l);
    }
    let mut out = BTreeMap::new();
    for (d, mut given) in by_dimension {
        if !tracked.iter().any(|m| m == d) {
            return Err(format!(
                "an intermediate value of {d}, which {} does not track",
                t.name
            ));
        }
        given.sort_by_key(|l| l.entered_at);
        let spans = held_parts.spans.get(d).map_or(&[][..], Vec::as_slice);
        let mut fresh = Vec::new();
        for l in given {
            let value = earlier_value(lib, t, d, &l.value, resolve, "an intermediate value")?;
            // one a first import gave as a legacy interval is held too, so
            // a batch sent again unchanged writes nothing (D585)
            let holds = spans
                .iter()
                .any(|sp| sp.entered_at == l.entered_at && sp.value == value);
            if !holds {
                fresh.push((l, value));
            }
        }
        let Some(latest) = spans.last().map(|sp| sp.entered_at) else {
            return Err(format!(
                "an intermediate value of {d}, of which the store holds no interval"
            ));
        };
        // the value the refresh brings opens where `entered` says, or at
        // the import
        let current = o.entered.get(d).map_or(port, |e| e.at);
        let mut made = Vec::new();
        for (i, (l, value)) in fresh.iter().enumerate() {
            if l.entered_at < latest {
                return Err(format!(
                    "an intermediate value of {d} entered before the latest interval the store holds"
                ));
            }
            let next = fresh.get(i + 1).map_or(current, |(n, _)| n.entered_at);
            if l.entered_at >= next {
                return Err(format!(
                    "an intermediate value of {d} entered at or after the value following it"
                ));
            }
            let left = l.left_at.unwrap_or(next);
            if left < l.entered_at {
                return Err(format!(
                    "an intermediate value of {d} that ends before it begins"
                ));
            }
            if left > next {
                return Err(format!(
                    "an intermediate value of {d} that overlaps the value following it"
                ));
            }
            made.push(Span {
                value: value.clone(),
                entered_position: 0,
                left_position: Some(0),
                entered_at: l.entered_at,
                left_at: Some(left),
                entered_by: l.entered_by.as_deref().map(Into::into),
                entered_by_kind: l.entered_by_kind.as_deref().unwrap_or("unknown").into(),
                migrated: false,
                declaration_version: lib.version,
                legacy: false,
                corrected_by: None,
            });
        }
        if !made.is_empty() {
            out.insert(d.to_string(), made);
        }
    }
    Ok(out)
}

/// The intervals an import opens: for a new object, one for the state and
/// every tracked member; for a refresh, one for each value it changes, the
/// current one ending. Each opens when `entered` says, by the changer it
/// names, and otherwise at the import, by a changer of kind `unknown`
/// (ADR-0106, C28). A refresh's intermediate values go between the value
/// it ends and the one it opens, so a value that came back to the one held
/// opens again. A refresh whose entry precedes the latest entry of the same
/// dimension is refused, its reason returned.
fn open_spans(
    lib: &Library,
    t: &Type,
    obj: &mut Object,
    refresh: Option<(&Object, &BTreeMap<String, Vec<Span>>)>,
    o: &ImportedObject,
    position: i64,
    port: i64,
) -> Result<Option<String>, NotRun> {
    let held = refresh.map(|(h, _)| h);
    for m in lib.tracked(t) {
        let now = if m == "state" {
            Value::State {
                type_: obj.type_.clone(),
                name: obj.state.clone(),
            }
        } else {
            obj.attr(&m)
        };
        let was = held.map(|h| {
            if m == "state" {
                Value::State {
                    type_: h.type_.clone(),
                    name: h.state.clone(),
                }
            } else {
                h.attr(&m)
            }
        });
        let between = refresh
            .and_then(|(_, between)| between.get(&m))
            .map_or(&[][..], Vec::as_slice);
        if was.as_ref() == Some(&now) && between.is_empty() {
            continue;
        }
        let entered = o.entered.get(&m);
        let at = entered.map_or(port, |e| e.at);
        let spans = obj.parts_mut()?.spans.entry(m.as_str().into()).or_default();
        if let Some(last) = spans.last_mut() {
            let ends = between.first().map_or(at, |sp| sp.entered_at);
            if ends < last.entered_at {
                return Ok(Some(format!(
                    "its {m} entered before the latest interval the store holds"
                )));
            }
            if last.left_position.is_none() {
                last.left_position = Some(position);
                last.left_at = Some(ends);
            }
        }
        // each opened and closed on this event
        spans.extend(between.iter().map(|sp| Span {
            entered_position: position,
            left_position: Some(position),
            ..sp.clone()
        }));
        spans.push(Span {
            value: now,
            entered_position: position,
            left_position: None,
            entered_at: at,
            left_at: None,
            entered_by: entered.and_then(|e| e.by.as_deref()).map(Into::into),
            entered_by_kind: entered
                .and_then(|e| e.by_kind.as_deref())
                .unwrap_or("unknown")
                .into(),
            migrated: false,
            declaration_version: lib.version,
            legacy: false,
            corrected_by: None,
        });
    }
    Ok(None)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::model::Remedy;
    use crate::model::{Library, OPERATOR};
    use crate::request::{Request, request};
    use crate::verdict::Verdict;

    const HR: &str = r#"
module: hr
categories: [active, closed]
sequences:
  badge: { description: Badges. }
types:
  Employee:
    mirror: true
    attributes:
      employee_id: { type: string, external: hr }
      name: { type: string, optional: true, personal: true }
      department: { type: string, optional: true }
      grade: { type: int, optional: true }
    observations:
      notes:
        kind: Note
        attributes:
          text: { type: string }
    states:
      EMPLOYED: { category: active }
      LEFT: { category: closed, final: true }
    invariants:
      graded: { expression: grade is null or grade >= 1 }
    transitions:
      forget: { kind: erasure, inputs: { reason: { type: string } } }
  Team:
    mirror: true
    attributes:
      code: { type: string, external: hr }
      lead: { reference: Employee, optional: true }
    states:
      ACTIVE: { category: active }
  Laptop:
    attributes:
      tag: { type: string }
    states:
      IN_USE: { category: active }
    transitions:
      receive: { kind: initial, to: IN_USE, required_inputs: [tag] }
"#;

    fn store() -> Store {
        let lib = Library::load(&[("hr.yaml".into(), HR.into())]).unwrap();
        Store::sandbox(lib, 1_767_225_600).unwrap()
    }

    fn employee(key: &str, state: &str, attributes: Json) -> ImportedObject {
        ImportedObject {
            type_: "Employee".into(),
            legacy_key: key.into(),
            state: state.into(),
            attributes: attributes
                .as_object()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect(),
            ..ImportedObject::default()
        }
    }

    fn run(
        s: &mut Store,
        objects: Vec<ImportedObject>,
        file: &[ViolationClass],
        dry_run: bool,
    ) -> ImportReport {
        let batch = ImportBatch {
            objects,
            sequences: Vec::new(),
            legacy_kinds: BTreeMap::new(),
        };
        import_batch(s, OPERATOR, &batch, file, dry_run)
            .unwrap()
            .unwrap()
    }

    fn decided(c: &ViolationClass, d: Disposition, reason: &str) -> ViolationClass {
        ViolationClass {
            disposition: d,
            reason: Some(reason.into()),
            ..c.clone()
        }
    }

    fn id_of(s: &Store, key: &str) -> String {
        let w = s.snapshot().unwrap();
        w.external_all()
            .unwrap()
            .get(&("legacy".to_string(), format!("Employee:{key}")))
            .cloned()
            .unwrap()
    }

    /// A batch whose writes break an invariant of an object it does not
    /// place, through a reference to what it refreshes, is refused, naming
    /// the object and the invariant, and writes nothing, where it was
    /// answered clean and the laptop stayed out with a person who had left
    /// (the audit's write path R-04).
    #[test]
    fn an_import_breaking_an_owned_objects_invariant_is_refused() {
        const LENT: &str = r#"
module: hr
categories: [active, closed]
types:
  Employee:
    mirror: true
    attributes:
      employee_id: { type: string, external: hr }
    states:
      EMPLOYED: { category: active }
      LEFT: { category: closed, final: true }
  Laptop:
    attributes:
      tag: { type: string }
      holder: { reference: Employee, optional: true }
    states:
      IN: { category: active }
      OUT: { category: active }
    invariants:
      held_by_staff: { expression: "state != Laptop.OUT or holder.state == Employee.EMPLOYED" }
    transitions:
      receive: { kind: initial, to: IN, required_inputs: [tag] }
      lend: { kind: external, from: [IN], to: OUT, required_inputs: [holder] }
"#;
        let lib = Library::load(&[("hr.yaml".into(), LENT.into())]).unwrap();
        let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
        let r = run(
            &mut s,
            vec![employee("E1", "EMPLOYED", json!({"employee_id": "E1"}))],
            &[],
            false,
        );
        assert_eq!(r.written, 1, "{r:?}");
        let ana = id_of(&s, "E1");
        let laptop = match request(
            &mut s,
            &crate::request::Request {
                transition: "receive".into(),
                actor: OPERATOR.into(),
                object_id: None,
                type_: Some("Laptop".into()),
                inputs: [("tag".to_string(), json!("L-1"))].into_iter().collect(),
                occurred_at: None,
                propose: false,
                expires_at: None,
                expected_version: None,
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            },
        )
        .unwrap()
        .unwrap()
        {
            Verdict::Satisfied { object_id, .. } => object_id,
            other => panic!("{other:?}"),
        };
        let lent = request(
            &mut s,
            &crate::request::Request {
                transition: "lend".into(),
                actor: OPERATOR.into(),
                object_id: Some(laptop.clone()),
                type_: None,
                inputs: [("holder".to_string(), json!(ana))].into_iter().collect(),
                occurred_at: None,
                propose: false,
                expires_at: None,
                expected_version: None,
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            },
        )
        .unwrap()
        .unwrap();
        assert!(matches!(lent, Verdict::Satisfied { .. }), "{lent:?}");
        let r = run(
            &mut s,
            vec![employee("E1", "LEFT", json!({"employee_id": "E1"}))],
            &[],
            false,
        );
        let refusal = r.refusal.clone().unwrap_or_else(|| panic!("{r:?}"));
        assert!(
            refusal.reason.contains("Laptop.held_by_staff") && refusal.reason.contains(&laptop),
            "{refusal:?}"
        );
        assert_eq!(r.written, 0);
        let w = s.snapshot().unwrap();
        assert_eq!(w.get(&ana).unwrap().unwrap().state, "EMPLOYED");
    }

    #[test]
    fn a_new_object_is_written_through_the_built_in_assertion_and_refreshed_by_its_key() {
        let mut s = store();
        let r = run(
            &mut s,
            vec![employee(
                "E-1",
                "EMPLOYED",
                json!({"employee_id": "1", "name": "Dana"}),
            )],
            &[],
            false,
        );
        assert_eq!((r.written, r.refusal.clone()), (1, None), "{r:?}");
        let id = id_of(&s, "E-1");
        let w = s.snapshot().unwrap();
        let o = w.whole(&id).unwrap().unwrap();
        assert_eq!(
            (o.state.as_str(), o.attr("name")),
            ("EMPLOYED", Value::Text("Dana".into()))
        );
        assert_eq!(
            w.external_all()
                .unwrap()
                .get(&("hr".to_string(), "1".to_string())),
            Some(&id)
        );
        // undated, its creation the import, by a kind nobody named
        assert!(!o.created_dated);
        assert_eq!(o.created_by_kind, "unknown");
        let e = s.history(&id).unwrap().remove(0);
        assert_eq!(
            (e.transition.as_str(), e.source.as_str(), e.imported),
            ("import", "asserted", true)
        );
        // what it wrote, which a fold of its events reproduces it by
        // (ADR-0209 decision 1)
        crate::store::assert_log_folds(&s, &id);
        // a refresh that changes nothing writes nothing; one that changes a value, one event
        let same = run(
            &mut s,
            vec![employee(
                "E-1",
                "EMPLOYED",
                json!({"employee_id": "1", "name": "Dana"}),
            )],
            &[],
            false,
        );
        assert_eq!(same.written, 0);
        let moved = run(
            &mut s,
            vec![employee(
                "E-1",
                "LEFT",
                json!({"employee_id": "1", "name": "Dana"}),
            )],
            &[],
            false,
        );
        assert_eq!(moved.written, 1);
        assert_eq!(s.history(&id).unwrap().len(), 2);
        assert_eq!(
            s.snapshot().unwrap().whole(&id).unwrap().unwrap().state,
            "LEFT"
        );
        crate::store::assert_log_folds(&s, &id);
        // a refresh changing a value writes it, and only it
        let renamed = run(
            &mut s,
            vec![employee(
                "E-1",
                "LEFT",
                json!({"employee_id": "1", "name": "Dee"}),
            )],
            &[],
            false,
        );
        assert_eq!(renamed.written, 1);
        let last = s.history(&id).unwrap().pop().unwrap();
        assert_eq!(last.payload["wrote"], json!({"name": "Dee"}));
        crate::store::assert_log_folds(&s, &id);
    }

    #[test]
    fn a_batch_is_refused_whole_at_the_first_object_it_cannot_place() {
        let mut s = store();
        let laptop = ImportedObject {
            type_: "Laptop".into(),
            legacy_key: "L-1".into(),
            state: "IN_USE".into(),
            ..ImportedObject::default()
        };
        let r = run(
            &mut s,
            vec![
                employee("E-1", "EMPLOYED", json!({"employee_id": "1"})),
                laptop,
            ],
            &[],
            false,
        );
        assert_eq!(r.written, 0);
        assert_eq!(r.refused_types, vec!["Laptop".to_string()]);
        assert_eq!(r.refusal.unwrap().legacy_key, "L-1");
        assert!(
            s.snapshot().unwrap().external_all().unwrap().is_empty(),
            "nothing is written"
        );
    }

    #[test]
    fn a_class_says_what_excluding_it_uncovers() {
        const ATS: &str = r#"
module: ats
categories: [live]
types:
  Application:
    mirror: true
    attributes:
      code: { type: string, external: ats }
      last_interview: { reference: Interview, optional: true }
    states:
      OPEN: { category: live }
  Interview:
    mirror: true
    attributes:
      code: { type: string, external: ats }
      application: { reference: Application }
    states:
      HELD: { category: live }
  Note:
    mirror: true
    attributes:
      code: { type: string, external: ats }
      interview: { reference: Interview }
    states:
      KEPT: { category: live }
  Offer:
    mirror: true
    attributes:
      code: { type: string, external: ats }
      application: { reference: Application }
    states:
      MADE: { category: live }
"#;
        let lib = Library::load(&[("ats.yaml".into(), ATS.into())]).unwrap();
        let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
        let object = |type_: &str, key: &str, state: &str, refs: &[(&str, &str)]| {
            let mut attributes: BTreeMap<String, Json> =
                [("code".to_string(), json!(key.to_lowercase()))].into();
            for (a, k) in refs {
                attributes.insert((*a).to_string(), json!(k));
            }
            ImportedObject {
                type_: type_.into(),
                legacy_key: key.into(),
                state: state.into(),
                attributes,
                ..ImportedObject::default()
            }
        };
        // an application on hold, which the flow does not declare, with an
        // interview naming it back, a note on it and an offer; and one open,
        // with its own
        let batch = vec![
            object(
                "Application",
                "A-1",
                "ON_HOLD",
                &[("last_interview", "I-1")],
            ),
            object("Application", "A-2", "OPEN", &[]),
            object("Interview", "I-1", "HELD", &[("application", "A-1")]),
            object("Interview", "I-2", "HELD", &[("application", "A-2")]),
            object("Note", "N-1", "KEPT", &[("interview", "I-1")]),
            object("Offer", "O-1", "MADE", &[("application", "A-1")]),
        ];
        let told = |c: &ViolationClass| {
            c.uncovers
                .iter()
                .map(|u| (u.id.clone(), u.count))
                .collect::<Vec<_>>()
        };
        let excluded = |rule: &str| format!("{rule}: {NAMES_EXCLUDED}");
        // the first dry run tells what excluding the held one would uncover,
        // the note behind its interview included, where E24 met each class a
        // round later
        let first = run(&mut s, batch.clone(), &[], true);
        assert_eq!(first.findings.len(), 1, "{:?}", first.findings);
        let held = first.findings[0].clone();
        assert_eq!(
            told(&held),
            vec![
                (excluded("Interview.application"), 1),
                (excluded("Note.interview"), 1),
                (excluded("Offer.application"), 1),
            ]
        );
        // excluded, it tells nothing more, its consequences being classes of
        // their own, each saying what excluding it would uncover in turn
        let file = [decided(&held, Disposition::Exclude, "on hold")];
        let second = run(&mut s, batch.clone(), &file, true);
        let by_id = |r: &ImportReport| {
            r.findings
                .iter()
                .map(|c| (c.id.clone(), told(c)))
                .collect::<BTreeMap<_, _>>()
        };
        assert_eq!(
            by_id(&second),
            BTreeMap::from([
                ("Application.state: is not declared".to_string(), vec![]),
                (
                    excluded("Interview.application"),
                    vec![(excluded("Note.interview"), 1)]
                ),
                (excluded("Offer.application"), vec![]),
            ])
        );
        // and deciding every class the first dry run told of converges
        let mut file = file.to_vec();
        for c in &second.findings {
            if c.kind == ViolationKind::Reference {
                file.push(decided(c, Disposition::Exclude, "with the application"));
            }
        }
        let third = run(&mut s, batch.clone(), &file, true);
        let note = third
            .findings
            .iter()
            .find(|c| c.id == excluded("Note.interview"))
            .unwrap()
            .clone();
        file.push(decided(&note, Disposition::Exclude, "with the interview"));
        let run_ = run(&mut s, batch, &file, false);
        assert_eq!((run_.written, run_.refusal), (2, None));
        // excluding an object the store holds loses nothing naming it
        let again = run(
            &mut s,
            vec![
                object("Application", "A-2", "ON_HOLD", &[]),
                object("Interview", "I-5", "HELD", &[("application", "A-2")]),
            ],
            &[],
            true,
        );
        assert_eq!(
            again
                .findings
                .iter()
                .map(|c| (c.id.clone(), told(c)))
                .collect::<Vec<_>>(),
            vec![("Application.state: is not declared".to_string(), vec![])]
        );
    }

    #[test]
    fn a_reference_naming_nothing_is_a_class_to_decide() {
        let mut s = store();
        // two teams naming leads neither the batch nor the store has, and a
        // third whose lead is in the batch
        let team = |key: &str, lead: &str| ImportedObject {
            type_: "Team".into(),
            legacy_key: key.into(),
            state: "ACTIVE".into(),
            attributes: [
                ("code".to_string(), json!(key.to_lowercase())),
                ("lead".to_string(), json!(lead)),
            ]
            .into(),
            ..ImportedObject::default()
        };
        let batch = vec![
            employee("E-1", "EMPLOYED", json!({"employee_id": "1"})),
            team("T-1", "E-404"),
            team("T-2", "E-405"),
            team("T-3", "E-1"),
        ];
        // one dry run lists every such reference, as one class
        let dry = run(&mut s, batch.clone(), &[], true);
        assert_eq!(dry.refusal, None, "{dry:?}");
        let class = dry
            .findings
            .iter()
            .find(|c| c.kind == ViolationKind::Reference)
            .unwrap()
            .clone();
        assert_eq!(
            (
                class.type_.as_str(),
                class.rule.as_str(),
                class.shape.as_str()
            ),
            ("Team", "lead", NAMES_NOTHING)
        );
        assert_eq!(
            (class.count, class.sample.clone()),
            (2, vec!["T-1".to_string(), "T-2".to_string()])
        );
        // undecided, it refuses the import, naming its first object
        let r = run(&mut s, batch.clone(), &[], false);
        let refusal = r.refusal.unwrap();
        assert_eq!(refusal.legacy_key, "T-1");
        assert!(
            refusal.reason.contains("(names nothing) is undecided"),
            "{}",
            refusal.reason
        );
        // dropped, the two teams come in without a lead
        let r = run(
            &mut s,
            batch.clone(),
            &[decided(&class, Disposition::Drop, "leads unknown")],
            false,
        );
        assert_eq!((r.written, r.refusal.clone()), (4, None), "{r:?}");
        let w = s.snapshot().unwrap();
        for (key, lead) in [("T-1", false), ("T-2", false), ("T-3", true)] {
            let id = w
                .external_all()
                .unwrap()
                .get(&("legacy".to_string(), format!("Team:{key}")))
                .unwrap()
                .clone();
            assert_eq!(
                !w.whole(&id).unwrap().unwrap().attr("lead").is_absent(),
                lead,
                "{key}"
            );
        }
        // excluded, in a store that does not hold them, they are not written
        let mut s = store();
        let r = run(
            &mut s,
            batch,
            &[decided(
                &class,
                Disposition::Exclude,
                "teams that never ran",
            )],
            false,
        );
        assert_eq!((r.written, r.refusal.clone()), (2, None), "{r:?}");
    }

    /// An identity two actors hold, in any case, is admitted by nothing: a
    /// disposition admitting `identity_taken` refuses the batch, where it
    /// wrote a second holder and the first made answered for both (sixth
    /// production review IDENTITY, ADR-0192 decision 6).
    #[test]
    fn an_identity_two_actors_hold_is_admitted_by_nothing() {
        const PEOPLE: &str = r#"
module: people
categories: [active]
types:
  Person:
    mirror: true
    attributes:
      person_id: { type: string, external: hr }
      login: { type: identity, actor_kind: human }
    states:
      HERE: { category: active }
"#;
        let lib = Library::load(&[("people.yaml".into(), PEOPLE.into())]).unwrap();
        let mut s = Store::sandbox(lib, 1_767_225_600).unwrap();
        let person = |key: &str, login: &str| ImportedObject {
            type_: "Person".into(),
            legacy_key: key.into(),
            state: "HERE".into(),
            attributes: [
                ("person_id".to_string(), json!(key)),
                ("login".to_string(), json!(login)),
            ]
            .into(),
            ..ImportedObject::default()
        };
        let batch = vec![person("P-1", "ann"), person("P-2", "ANN")];
        let dry = run(&mut s, batch.clone(), &[], true);
        let taken = dry
            .findings
            .iter()
            .find(|c| c.rule == crate::request::IDENTITY_TAKEN)
            .unwrap()
            .clone();
        let r = run(
            &mut s,
            batch,
            &[decided(
                &taken,
                Disposition::Admit,
                "two logins of one person",
            )],
            false,
        );
        assert_eq!(r.written, 0, "{r:?}");
        assert!(r.refusal.unwrap().reason.contains("nothing admits"));
    }

    #[test]
    fn each_class_is_applied_as_the_file_decides_it() {
        let mut s = store();
        let missing = employee("E-2", "EMPLOYED", json!({"name": "Bo"}));
        let low = employee("E-3", "EMPLOYED", json!({"employee_id": "3", "grade": 0}));
        let team = ImportedObject {
            type_: "Team".into(),
            legacy_key: "T-1".into(),
            state: "ACTIVE".into(),
            attributes: [
                ("code".to_string(), json!("ops")),
                ("lead".to_string(), json!("E-2")),
            ]
            .into(),
            ..ImportedObject::default()
        };
        let batch = vec![missing.clone(), low.clone(), team.clone()];
        // the dry run finds the classes and writes nothing
        let dry = run(&mut s, batch.clone(), &[], true);
        assert_eq!(dry.written, 0);
        let guard = dry
            .findings
            .iter()
            .find(|c| c.kind == ViolationKind::Guard)
            .unwrap()
            .clone();
        let invariant = dry
            .findings
            .iter()
            .find(|c| c.kind == ViolationKind::Invariant)
            .unwrap()
            .clone();
        assert_eq!(
            (guard.rule.as_str(), guard.shape.as_str()),
            ("employee_id_provided", IS_ABSENT)
        );
        assert_eq!((invariant.rule.as_str(), invariant.count), ("graded", 1));
        // undecided, it refuses
        assert!(run(&mut s, batch.clone(), &[], false).refusal.is_some());
        // a dry run reports a wrong decision, as the import would refuse it
        let wrong = run(
            &mut s,
            batch.clone(),
            &[decided(&guard, Disposition::Admit, "no")],
            true,
        );
        assert!(wrong.refusal.unwrap().reason.contains("nothing admits"));
        // excluding the guard's object makes the team's lead a reference class
        let file = vec![
            decided(&guard, Disposition::Exclude, "left before the port"),
            decided(&invariant, Disposition::Admit, "graded on paper"),
        ];
        let dry = run(&mut s, batch.clone(), &file, true);
        let reference = dry
            .findings
            .iter()
            .find(|c| c.kind == ViolationKind::Reference)
            .unwrap()
            .clone();
        assert_eq!(reference.rule, "lead");
        let mut file = file;
        file.push(decided(&reference, Disposition::Drop, "lead unknown"));
        let r = run(&mut s, batch, &file, false);
        assert_eq!((r.written, r.refusal.clone()), (2, None), "{r:?}");
        let w = s.snapshot().unwrap();
        assert!(
            !w.external_all()
                .unwrap()
                .contains_key(&("legacy".to_string(), "Employee:E-2".to_string()))
        );
        let team_id = w
            .external_all()
            .unwrap()
            .get(&("legacy".to_string(), "Team:T-1".to_string()))
            .unwrap();
        assert_eq!(
            w.whole(team_id).unwrap().unwrap().attr("lead"),
            Value::Absent
        );
        // the admitted violation stands, and a refresh that cures it discharges it
        let low_id = id_of(&s, "E-3");
        assert!(
            s.snapshot()
                .unwrap()
                .whole(&low_id)
                .unwrap()
                .unwrap()
                .held_parts()
                .unwrap()
                .admissions
                .contains("graded")
        );
        let cured = employee("E-3", "EMPLOYED", json!({"employee_id": "3", "grade": 2}));
        assert_eq!(run(&mut s, vec![cured], &[], false).written, 1);
        assert!(
            s.snapshot()
                .unwrap()
                .whole(&low_id)
                .unwrap()
                .unwrap()
                .held_parts()
                .unwrap()
                .admissions
                .is_empty()
        );
    }

    #[test]
    fn an_import_dates_its_intervals_and_creation_as_the_record_says() {
        let mut s = store();
        let at = s.now - 86_400 * 30;
        let mut e = employee("E-1", "EMPLOYED", json!({"employee_id": "1"}));
        e.entered.insert(
            "state".into(),
            EnteredAt {
                at,
                by: Some("hr-admin".into()),
                by_kind: Some("human".into()),
            },
        );
        e.created_at = Some(at);
        let batch = ImportBatch {
            objects: vec![e],
            sequences: vec![Raise {
                sequence: "badge".into(),
                scope: String::new(),
                last: 512,
            }],
            legacy_kinds: BTreeMap::new(),
        };
        let r = import_batch(&mut s, OPERATOR, &batch, &[], false)
            .unwrap()
            .unwrap();
        assert_eq!(r.written, 1);
        let w = s.snapshot().unwrap();
        let o = w.whole(&id_of(&s, "E-1")).unwrap().unwrap().clone();
        let span = o.held_parts().unwrap().spans["state"][0].clone();
        assert_eq!(
            (
                span.entered_at,
                span.entered_by.as_deref(),
                &*span.entered_by_kind
            ),
            (at, Some("hr-admin"), "human")
        );
        // its record whole from its creation, which it was given
        assert!(o.created_dated);
        assert_eq!((o.created_at, o.recorded_from), (at, at));
        // a sequence is raised past the legacy value, never lowered
        let mut alloc_value = None;
        s.transact(|_, alloc| {
            alloc_value = alloc.draw("badge", "");
            Ok::<_, NotRun>(((), None))
        })
        .unwrap()
        .unwrap();
        assert_eq!(alloc_value, Some(513));
    }

    /// A raise names a sequence a module declares, with a last value that
    /// leaves a next: one naming none, or past what a sequence draws, is
    /// refused and nothing written, where i64::MAX was stored wrapped and
    /// stopped every write; at its last number a sequence is exhausted
    /// (fourth production review A-15).
    #[test]
    fn a_raise_naming_no_sequence_or_past_its_last_is_refused() {
        let raise = |s: &mut Store, sequence: &str, last: i64| {
            let batch = ImportBatch {
                objects: Vec::new(),
                sequences: vec![Raise {
                    sequence: sequence.into(),
                    scope: String::new(),
                    last,
                }],
                ..ImportBatch::default()
            };
            import_batch(s, OPERATOR, &batch, &[], false)
                .unwrap()
                .unwrap()
        };
        let mut s = store();
        for (sequence, last, why) in [
            ("junk", 7, "is no sequence the flow declares"),
            ("badge", i64::MAX, "leaves it no next number"),
            (
                "badge",
                crate::store::SEQUENCE_MOST,
                "leaves it no next number",
            ),
            ("badge", -1, "leaves it no next number"),
        ] {
            let r = raise(&mut s, sequence, last);
            let refusal = r.refusal.expect("refused");
            assert!(
                refusal.reason.contains(why),
                "{sequence} {last}: {}",
                refusal.reason
            );
        }
        let r = raise(&mut s, "badge", crate::store::SEQUENCE_MOST - 1);
        assert!(r.refusal.is_none(), "{r:?}");
        let mut drawn = Vec::new();
        s.transact(|_, alloc| {
            drawn.push(alloc.draw("badge", ""));
            drawn.push(alloc.draw("badge", ""));
            Ok::<_, NotRun>(((), None))
        })
        .unwrap()
        .unwrap();
        assert_eq!(drawn, [Some(crate::store::SEQUENCE_MOST), None]);
    }

    #[test]
    fn a_shared_identifier_an_erasure_and_a_mirror_s_observations_are_kept_to_their_rules() {
        let mut s = store();
        let r = run(
            &mut s,
            vec![
                employee("E-1", "EMPLOYED", json!({"employee_id": "1"})),
                employee("E-2", "EMPLOYED", json!({"employee_id": "1"})),
            ],
            &[],
            true,
        );
        assert!(
            r.findings
                .iter()
                .any(|c| c.rule == "employee_id_unique" && c.shape == IS_SHARED)
        );
        run(
            &mut s,
            vec![employee(
                "E-1",
                "EMPLOYED",
                json!({"employee_id": "1", "name": "Dana"}),
            )],
            &[],
            false,
        );
        let id = id_of(&s, "E-1");
        // a recording on a mirror's object is the import's to write
        let note = Request {
            transition: "record".into(),
            actor: OPERATOR.into(),
            object_id: None,
            type_: Some("Note".into()),
            inputs: [
                ("subject".to_string(), json!(id)),
                ("text".to_string(), json!("hi")),
            ]
            .into(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
        };
        assert_eq!(
            request(&mut s, &note).unwrap().unwrap(),
            Verdict::Unsatisfied {
                clause: "subject_owned".into(),
                remedy: Some(Remedy::UnreachableFromHere)
            }
        );
        // an erased object's personal value is not written again
        let forget = Request {
            transition: "forget".into(),
            actor: OPERATOR.into(),
            object_id: Some(id.clone()),
            type_: None,
            inputs: [("reason".to_string(), json!("asked"))].into(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
        };
        assert!(matches!(
            request(&mut s, &forget).unwrap().unwrap(),
            Verdict::Satisfied { .. }
        ));
        run(
            &mut s,
            vec![employee(
                "E-1",
                "EMPLOYED",
                json!({"employee_id": "1", "name": "Dana", "department": "Ops"}),
            )],
            &[],
            false,
        );
        let w = s.snapshot().unwrap();
        let o = w.whole(&id).unwrap().unwrap();
        assert_eq!(
            (o.attr("name"), o.attr("department")),
            (Value::Absent, Value::Text("Ops".into()))
        );
    }

    fn entered_at(at: i64) -> EnteredAt {
        EnteredAt {
            at,
            by: None,
            by_kind: Some("human".into()),
        }
    }

    fn earlier(dimension: &str, value: Json, from: i64, to: Option<i64>) -> LegacyInterval {
        LegacyInterval {
            dimension: dimension.into(),
            value,
            entered_at: from,
            left_at: to,
            entered_by: None,
            entered_by_kind: None,
        }
    }

    /// Legacy intervals and silent spans (`publish-and-import.md` §4): each
    /// earlier value a span marked legacy at position 0, and `recorded_from`
    /// from each source the rule names; a ported span's `.held` read by time
    /// from the index, legacy intervals included; and a span reaching back
    /// before its record a gap in a metric.
    #[test]
    fn a_batch_sent_again_unchanged_writes_nothing() {
        // a go-live resumed after a timeout sends its batch again, legacy
        // intervals and all: what the first run wrote is held, so nothing
        // is written and nothing refused (publish-and-import.md §4, D585)
        let mut s = store();
        let port = s.now;
        let day = 86_400;
        let batch = || {
            let mut e1 = employee("E-1", "EMPLOYED", json!({"employee_id": "1"}));
            e1.created_at = Some(port - 100 * day);
            e1.entered
                .insert("state".into(), entered_at(port - 60 * day));
            e1.legacy_intervals = vec![earlier(
                "state",
                json!("LEFT"),
                port - 100 * day,
                Some(port - 60 * day),
            )];
            vec![e1]
        };
        let first = run(&mut s, batch(), &[], false);
        assert!(first.refusal.is_none() && first.written == 1, "{first:?}");
        let again = run(&mut s, batch(), &[], false);
        assert!(again.refusal.is_none(), "{again:?}");
        assert_eq!(again.written, 0, "{again:?}");
        let id = id_of(&s, "E-1");
        let w = s.snapshot().unwrap();
        let o = w.whole(&id).unwrap().unwrap();
        assert_eq!(o.held_parts().unwrap().spans["state"].len(), 2);
        drop(w);
        // a refresh moving it is a second imported event, and its legacy span
        // is still exported by the first, its object's import, as the walk of
        // every event read it (eleventh production review N1)
        let mut moved = batch();
        moved[0].state = "LEFT".into();
        moved[0].entered.insert("state".into(), entered_at(port));
        let refreshed = run(&mut s, moved, &[], false);
        assert!(
            refreshed.refusal.is_none() && refreshed.written == 1,
            "{refreshed:?}"
        );
        let imports = s
            .history(&id)
            .unwrap()
            .iter()
            .filter(|e| e.imported)
            .count();
        assert_eq!(imports, 2);
        crate::export::assert_intervals_as_walked(&s, "Employee");
    }

    /// A dry run refuses what the run refuses, a legacy interval of a
    /// dimension the type does not track among them, where it answered the
    /// batch clean (thirteenth production review).
    #[test]
    fn a_dry_run_refuses_what_the_run_refuses() {
        let mut s = store();
        let port = s.now;
        let mut e1 = employee("E-1", "EMPLOYED", json!({"employee_id": "1"}));
        e1.legacy_intervals = vec![earlier(
            "nickname",
            json!("Ed"),
            port - 100,
            Some(port - 50),
        )];
        let dry = run(&mut s, vec![e1.clone()], &[], true);
        let wet = run(&mut s, vec![e1], &[], false);
        assert!(wet.refusal.is_some(), "{wet:?}");
        assert_eq!(dry.refusal, wet.refusal, "{dry:?}");
        assert_eq!(dry.written, 0);
        // an entry time of a dimension the type does not track, on a new
        // object and on a refresh, refused as a legacy interval of one is,
        // where it was taken and its time dropped (fourteenth production
        // review, minor)
        let untracked = |r: &ImportReport| {
            r.refusal
                .as_ref()
                .is_some_and(|f| f.reason.contains("an entry time of nickname"))
        };
        let mut e2 = employee("E-2", "EMPLOYED", json!({"employee_id": "2"}));
        e2.entered.insert("nickname".into(), entered_at(port - 10));
        let dry = run(&mut s, vec![e2.clone()], &[], true);
        let wet = run(&mut s, vec![e2], &[], false);
        assert!(untracked(&wet), "{wet:?}");
        assert_eq!(dry.refusal, wet.refusal, "{dry:?}");
        let e3 = employee("E-3", "EMPLOYED", json!({"employee_id": "3"}));
        assert_eq!(run(&mut s, vec![e3.clone()], &[], false).written, 1);
        let mut again = e3;
        again
            .entered
            .insert("nickname".into(), entered_at(port - 10));
        let refreshed = run(&mut s, vec![again], &[], false);
        assert!(untracked(&refreshed), "{refreshed:?}");
    }

    #[test]
    fn legacy_intervals_reach_back_and_silent_spans_bound_the_record() {
        let mut s = store();
        let port = s.now;
        let day = 86_400;
        // whole from its legacy creation: rehired after a stretch away
        let mut e1 = employee("E-1", "EMPLOYED", json!({"employee_id": "1"}));
        e1.created_at = Some(port - 100 * day);
        e1.entered
            .insert("state".into(), entered_at(port - 60 * day));
        e1.legacy_intervals = vec![earlier(
            "state",
            json!("LEFT"),
            port - 100 * day,
            Some(port - 60 * day),
        )];
        // undated, its earliest entry a legacy one, with a stretch the record
        // leaves unexplained
        let mut e2 = employee("E-2", "EMPLOYED", json!({"employee_id": "2"}));
        e2.entered
            .insert("state".into(), entered_at(port - 40 * day));
        e2.legacy_intervals = vec![earlier(
            "state",
            json!("LEFT"),
            port - 90 * day,
            Some(port - 50 * day),
        )];
        // silent from 20 to 15 days before the port
        let mut e3 = employee("E-3", "EMPLOYED", json!({"employee_id": "3"}));
        e3.entered
            .insert("state".into(), entered_at(port - 30 * day));
        e3.silent_spans = vec![(port - 20 * day, port - 15 * day)];
        // nothing said: from the port
        let e4 = employee("E-4", "EMPLOYED", json!({"employee_id": "4"}));
        // a team led by E-2, then by nobody, now by E-1
        let mut team = employee("T-1", "ACTIVE", json!({"code": "T1", "lead": "E-1"}));
        team.type_ = "Team".into();
        team.entered
            .insert("state".into(), entered_at(port - 50 * day));
        team.entered
            .insert("lead".into(), entered_at(port - 10 * day));
        team.legacy_intervals = vec![
            earlier("lead", json!("E-2"), port - 80 * day, Some(port - 30 * day)),
            earlier("lead", Json::Null, port - 30 * day, None),
        ];
        // a team whose lead's earlier values are given, its current lead and
        // state from the port: dated by the legacy lead, recorded from the
        // port
        let mut quiet = employee("T-2", "ACTIVE", json!({"code": "T2"}));
        quiet.type_ = "Team".into();
        quiet.legacy_intervals = vec![earlier(
            "lead",
            json!("E-1"),
            port - 70 * day,
            Some(port - 5 * day),
        )];
        let r = run(&mut s, vec![e1, e2, e3, e4, team, quiet], &[], false);
        assert!(r.refusal.is_none(), "{r:?}");
        // a legacy span exported by its object's import, read by the index
        // on imported events, as the walk of every event read it (eleventh
        // production review N1)
        crate::export::assert_intervals_as_walked(&s, "Employee");
        crate::export::assert_intervals_as_walked(&s, "Team");
        let w = s.snapshot().unwrap();
        let (e1, e2, e3, e4) = (
            id_of(&s, "E-1"),
            id_of(&s, "E-2"),
            id_of(&s, "E-3"),
            id_of(&s, "E-4"),
        );
        let t1 = w.external_all().unwrap()[&("legacy".to_string(), "Team:T-1".to_string())].clone();
        let o1 = w.whole(&e1).unwrap().unwrap();
        let first = &o1.held_parts().unwrap().spans["state"][0];
        assert!(first.legacy && first.entered_position == 0 && first.left_position == Some(0));
        assert_eq!(
            (first.entered_at, first.left_at),
            (port - 100 * day, Some(port - 60 * day))
        );
        assert_eq!(o1.held_parts().unwrap().spans["state"].len(), 2);
        assert!(o1.created_dated);
        assert_eq!(
            (o1.created_at, o1.recorded_from),
            (port - 100 * day, port - 100 * day)
        );
        let ev = |id: &str, text: &str| {
            let t = w.lib.type_(&w.whole(id).unwrap().unwrap().type_);
            crate::eval::Evaluator::new(&w, t, Some(id))
                .eval(&crate::expr::parse(text).unwrap())
                .unwrap()
        };
        assert_eq!(ev(&e1, "time_in(LEFT)"), Value::Duration(40 * day));
        let o2 = w.whole(&e2).unwrap().unwrap();
        assert!(!o2.created_dated);
        assert_eq!(
            (o2.created_at, o2.recorded_from),
            (port - 90 * day, port - 90 * day)
        );
        assert_eq!(
            w.whole(&e3).unwrap().unwrap().recorded_from,
            port - 15 * day
        );
        assert_eq!(w.whole(&e4).unwrap().unwrap().recorded_from, port);
        let t2 = w
            .get(&w.external_all().unwrap()[&("legacy".to_string(), "Team:T-2".to_string())])
            .unwrap()
            .unwrap();
        assert!(!t2.created_dated);
        assert_eq!((t2.created_at, t2.recorded_from), (port - 70 * day, port));
        // the team's assignee, earlier values resolved by legacy key
        let lead: Vec<Value> = w.whole(&t1).unwrap().unwrap().held_parts().unwrap().spans["lead"]
            .iter()
            .map(|sp| sp.value.clone())
            .collect();
        assert_eq!(
            lead,
            vec![
                Value::Object(e2.clone()),
                Value::Absent,
                Value::Object(e1.clone())
            ]
        );
        // its state's span, opened by the import fifty days back, held E-2
        let Value::Set(rows) = ev(&t1, "this.intervals") else {
            panic!()
        };
        let Value::Row(row) = &rows[0] else { panic!() };
        assert_eq!(
            row["held"],
            Value::row(BTreeMap::from([
                ("lead".to_string(), Value::Object(e2.clone())),
                (
                    "state".to_string(),
                    Value::State {
                        type_: "Team".into(),
                        name: "ACTIVE".into()
                    }
                ),
            ]))
        );
        // E-3's span entered before the end of its silent span: a gap
        let page = crate::metric::page(
            &w,
            None,
            "Employee.time_in_state",
            &BTreeMap::new(),
            Some(&["state".to_string()]),
            None,
            None,
        )
        .unwrap();
        let gaps: Vec<(Value, Vec<String>)> = page
            .rows
            .iter()
            .map(|r| (r.dimensions[0].1.clone(), r.gaps.clone()))
            .collect();
        assert_eq!(
            gaps,
            vec![
                (
                    Value::State {
                        type_: "Employee".into(),
                        name: "EMPLOYED".into()
                    },
                    vec![e3.clone()]
                ),
                (
                    Value::State {
                        type_: "Employee".into(),
                        name: "LEFT".into()
                    },
                    vec![]
                ),
            ]
        );
    }

    #[test]
    fn a_legacy_record_that_does_not_hold_together_is_refused() {
        let day = 86_400;
        type Planted = Box<dyn Fn(i64) -> ImportedObject>;
        let cases: Vec<(&str, Planted)> = vec![
            (
                "overlaps a silent span",
                Box::new(move |port| {
                    let mut e = employee("E-1", "EMPLOYED", json!({"employee_id": "1"}));
                    e.entered
                        .insert("state".into(), entered_at(port - 10 * day));
                    e.legacy_intervals = vec![earlier(
                        "state",
                        json!("LEFT"),
                        port - 30 * day,
                        Some(port - 20 * day),
                    )];
                    e.silent_spans = vec![(port - 25 * day, port - 22 * day)];
                    e
                }),
            ),
            (
                "overlaps the one after it",
                Box::new(move |port| {
                    let mut e = employee("E-1", "EMPLOYED", json!({"employee_id": "1"}));
                    e.entered
                        .insert("state".into(), entered_at(port - 10 * day));
                    e.legacy_intervals = vec![
                        earlier(
                            "state",
                            json!("LEFT"),
                            port - 40 * day,
                            Some(port - 25 * day),
                        ),
                        earlier(
                            "state",
                            json!("EMPLOYED"),
                            port - 30 * day,
                            Some(port - 20 * day),
                        ),
                    ];
                    e
                }),
            ),
            (
                "overlaps the one after it",
                Box::new(move |port| {
                    let mut e = employee("E-1", "EMPLOYED", json!({"employee_id": "1"}));
                    e.entered
                        .insert("state".into(), entered_at(port - 30 * day));
                    e.legacy_intervals = vec![earlier(
                        "state",
                        json!("LEFT"),
                        port - 40 * day,
                        Some(port - 20 * day),
                    )];
                    e
                }),
            ),
            (
                "earlier than the creation time",
                Box::new(move |port| {
                    let mut e = employee("E-1", "EMPLOYED", json!({"employee_id": "1"}));
                    e.created_at = Some(port - 30 * day);
                    e.entered
                        .insert("state".into(), entered_at(port - 10 * day));
                    e.legacy_intervals = vec![earlier(
                        "state",
                        json!("LEFT"),
                        port - 40 * day,
                        Some(port - 10 * day),
                    )];
                    e
                }),
            ),
            (
                "entered before the creation time",
                Box::new(move |port| {
                    let mut e = employee("E-1", "EMPLOYED", json!({"employee_id": "1"}));
                    e.created_at = Some(port - 30 * day);
                    e.entered
                        .insert("state".into(), entered_at(port - 40 * day));
                    e
                }),
            ),
            (
                "does not track",
                Box::new(move |port| {
                    let mut e = employee("E-1", "EMPLOYED", json!({"employee_id": "1"}));
                    e.legacy_intervals = vec![earlier(
                        "department",
                        json!("Sales"),
                        port - 40 * day,
                        Some(port - 20 * day),
                    )];
                    e
                }),
            ),
            (
                "does not declare",
                Box::new(move |port| {
                    let mut e = employee("E-1", "EMPLOYED", json!({"employee_id": "1"}));
                    e.legacy_intervals = vec![earlier(
                        "state",
                        json!("RETIRED"),
                        port - 40 * day,
                        Some(port - 20 * day),
                    )];
                    e
                }),
            ),
            (
                "no object of the batch or the store",
                Box::new(move |port| {
                    let mut t = employee("T-1", "ACTIVE", json!({"code": "T1"}));
                    t.type_ = "Team".into();
                    t.legacy_intervals = vec![earlier(
                        "lead",
                        json!("E-404"),
                        port - 40 * day,
                        Some(port - 20 * day),
                    )];
                    t
                }),
            ),
        ];
        for (words, object) in cases {
            let mut s = store();
            let port = s.now;
            let r = run(&mut s, vec![object(port)], &[], false);
            let reason = r.refusal.map(|f| f.reason).unwrap_or_default();
            assert!(reason.contains(words), "{words}: {reason}");
            assert_eq!(r.written, 0);
            assert!(
                s.snapshot().unwrap().external_all().unwrap().is_empty(),
                "{words}: something was written"
            );
        }
    }

    /// A refresh's intermediate values (`publish-and-import.md` §4): the
    /// leads a team had in the legacy system between two imports open and
    /// close on the refresh's event, in order, with who made each change; a
    /// batch sent again writes nothing, a lead that came back opens again,
    /// and values out of order are refused.
    #[test]
    fn a_refresh_opens_and_closes_its_intermediate_values_in_order() {
        let day = 86_400;
        let mut s = store();
        let port = s.now;
        let team = |lead: &str| {
            let mut t = employee("T-1", "ACTIVE", json!({"code": "T1", "lead": lead}));
            t.type_ = "Team".into();
            t
        };
        let staff: Vec<_> = ["E-1", "E-2", "E-3"]
            .iter()
            .map(|k| employee(k, "EMPLOYED", json!({"employee_id": k})))
            .chain([team("E-1")])
            .collect();
        assert_eq!(run(&mut s, staff, &[], false).written, 4);
        let id = |s: &Store, key: &str| {
            s.snapshot().unwrap().external_all().unwrap()[&("legacy".to_string(), key.to_string())]
                .clone()
        };
        let (t1, e1, e2, e3) = (
            id(&s, "Team:T-1"),
            id(&s, "Employee:E-1"),
            id(&s, "Employee:E-2"),
            id(&s, "Employee:E-3"),
        );
        // ten days on, the legacy record shows E-2 led from day 2 to day 6,
        // by an HR clerk, and E-3 from day 6
        s.now = port + 10 * day;
        let mut refresh = team("E-3");
        refresh.entered.insert(
            "lead".into(),
            EnteredAt {
                at: port + 6 * day,
                by: None,
                by_kind: None,
            },
        );
        let mut between = earlier("lead", json!("E-2"), port + 2 * day, None);
        between.entered_by = Some("hr-clerk".into());
        between.entered_by_kind = Some("human".into());
        refresh.legacy_intervals = vec![between];
        assert_eq!(run(&mut s, vec![refresh.clone()], &[], false).written, 1);
        let leads = |s: &Store| {
            s.snapshot()
                .unwrap()
                .whole(&t1)
                .unwrap()
                .unwrap()
                .held_parts()
                .unwrap()
                .spans["lead"]
                .iter()
                .map(|sp| {
                    (
                        sp.value.clone(),
                        sp.entered_at - port,
                        sp.left_at.map(|l| l - port),
                        sp.entered_by_kind.to_string(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let unknown = || "unknown".to_string();
        let expected = vec![
            (Value::Object(e1.clone()), 0, Some(2 * day), unknown()),
            (
                Value::Object(e2.clone()),
                2 * day,
                Some(6 * day),
                "human".into(),
            ),
            (Value::Object(e3.clone()), 6 * day, None, unknown()),
        ];
        assert_eq!(leads(&s), expected);
        // the closed spans are the refresh's event's, as the store reads them
        let w = s.snapshot().unwrap();
        let spans = &w.whole(&t1).unwrap().unwrap().held_parts().unwrap().spans["lead"];
        assert_eq!(spans[1].entered_position, spans[1].left_position.unwrap());
        assert_eq!(spans[0].left_position, Some(spans[1].entered_position));
        drop(w);
        // sent again, it writes nothing, and is not refused: what the store
        // holds is left as it is (publish-and-import.md §4)
        let events = s.snapshot().unwrap().log().unwrap().count();
        let again = run(&mut s, vec![refresh], &[], false);
        assert_eq!((again.written, again.refusal), (0, None));
        assert_eq!(s.snapshot().unwrap().log().unwrap().count(), events);
        assert_eq!(leads(&s), expected);
        // E-1 led again for a day before E-3 came back: the current value is
        // unchanged, and the refresh still writes the stretch between
        s.now = port + 20 * day;
        let mut back = team("E-3");
        back.entered.insert(
            "lead".into(),
            EnteredAt {
                at: port + 13 * day,
                by: None,
                by_kind: None,
            },
        );
        back.legacy_intervals = vec![earlier("lead", json!("E-1"), port + 12 * day, None)];
        assert_eq!(run(&mut s, vec![back], &[], false).written, 1);
        assert_eq!(
            leads(&s)[2..],
            [
                (
                    Value::Object(e3.clone()),
                    6 * day,
                    Some(12 * day),
                    unknown()
                ),
                (
                    Value::Object(e1.clone()),
                    12 * day,
                    Some(13 * day),
                    unknown()
                ),
                (Value::Object(e3), 13 * day, None, unknown()),
            ]
        );
        // refused: before the latest interval the store holds, at or after
        // the value following it, past it, and of what is not tracked
        s.now = port + 30 * day;
        let cases: Vec<(&str, LegacyInterval, i64)> = vec![
            (
                "before the latest interval",
                earlier("lead", json!("E-2"), port + 11 * day, None),
                25,
            ),
            (
                "at or after the value following it",
                earlier("lead", json!("E-2"), port + 25 * day, None),
                25,
            ),
            (
                "overlaps the value following it",
                earlier("lead", json!("E-2"), port + 21 * day, Some(port + 27 * day)),
                25,
            ),
            (
                "does not track",
                earlier("code", json!("T9"), port + 21 * day, None),
                25,
            ),
        ];
        for (words, l, entered) in cases {
            let mut r = team("E-2");
            r.entered.insert(
                "lead".into(),
                EnteredAt {
                    at: port + entered * day,
                    by: None,
                    by_kind: None,
                },
            );
            r.legacy_intervals = vec![l];
            let report = run(&mut s, vec![r], &[], false);
            let reason = report.refusal.map(|f| f.reason).unwrap_or_default();
            assert!(reason.contains(words), "{words}: {reason}");
            assert_eq!(report.written, 0, "{words}");
        }
        assert_eq!(leads(&s).len(), 5);
    }

    /// Observations and labels port with their subjects
    /// (`publish-and-import.md` §4): a mirror's object's note, dated when the
    /// legacy record says and recorded at the import, and a label naming its
    /// subject by `<Type>:<key>`; an occurred time only on an observation,
    /// and never after the import.
    #[test]
    fn observations_and_labels_port_with_their_mirrors_objects() {
        let day = 86_400;
        let mut s = store();
        let port = s.now;
        let mut note = employee(
            "N-1",
            "RECORDED",
            json!({"subject": "E-1", "text": "joined"}),
        );
        note.type_ = "Note".into();
        note.occurred_at = Some(port - 30 * day);
        let mut label = employee(
            "L-1",
            "RECORDED",
            json!({"subject": "Employee:E-1", "name": "mentor", "subject_state": "EMPLOYED"}),
        );
        label.type_ = "label".into();
        let batch = vec![
            employee("E-1", "EMPLOYED", json!({"employee_id": "1"})),
            note.clone(),
            label.clone(),
        ];
        let r = run(&mut s, batch, &[], false);
        assert_eq!((r.written, r.refusal), (3, None));
        let w = s.snapshot().unwrap();
        let id =
            |key: &str| w.external_all().unwrap()[&("legacy".to_string(), key.to_string())].clone();
        let (e1, n1, l1) = (id("Employee:E-1"), id("Note:N-1"), id("label:L-1"));
        let n = w.whole(&n1).unwrap().unwrap();
        assert_eq!(n.attr("subject"), Value::Object(e1.clone()));
        assert_eq!(n.attr("occurred_at"), Value::Timestamp(port - 30 * day));
        assert_eq!(n.attr("recorded_at"), Value::Timestamp(port));
        assert_eq!(
            w.whole(&l1).unwrap().unwrap().attr("subject"),
            Value::Object(e1)
        );
        drop(w);
        // refused: a label's subject without its type, an occurred time on
        // what is not an observation, and one after the import
        let mut bare = label.clone();
        bare.legacy_key = "L-2".into();
        bare.attributes.insert("subject".into(), json!("E-1"));
        let mut dated = employee("E-2", "EMPLOYED", json!({"employee_id": "2"}));
        dated.occurred_at = Some(port - day);
        let mut ahead = note;
        ahead.legacy_key = "N-2".into();
        ahead.occurred_at = Some(port + day);
        // a creation, an entry into a state, an interval or a silent span
        // after the import (fourth production review A-17)
        let mut made = employee("E-3", "EMPLOYED", json!({"employee_id": "3"}));
        made.created_at = Some(port + day);
        let mut entered = employee("E-4", "EMPLOYED", json!({"employee_id": "4"}));
        entered.entered.insert(
            "EMPLOYED".into(),
            EnteredAt {
                at: port + day,
                by: None,
                by_kind: None,
            },
        );
        let mut interval = employee("E-5", "EMPLOYED", json!({"employee_id": "5"}));
        interval.legacy_intervals.push(LegacyInterval {
            dimension: "state".into(),
            value: json!("EMPLOYED"),
            entered_at: port - day,
            left_at: Some(port + day),
            entered_by: None,
            entered_by_kind: None,
        });
        let mut silent = employee("E-6", "EMPLOYED", json!({"employee_id": "6"}));
        silent.silent_spans.push((port - day, port + day));
        let mut empty = employee("E-7", "EMPLOYED", json!({"employee_id": "7"}));
        empty.created_at = Some(-62_135_596_800);
        // a kind of actor no actor has, and an actor named past a text's
        // bound (seventh production review, ADR-0194 decision 6)
        let mut robot = employee("E-9", "EMPLOYED", json!({"employee_id": "9"}));
        robot.created_by_kind = Some("robot".into());
        let mut long = employee("E-10", "EMPLOYED", json!({"employee_id": "10"}));
        long.entered.insert(
            "EMPLOYED".into(),
            EnteredAt {
                at: port - day,
                by: Some("x".repeat(70_000)),
                by_kind: Some("human".into()),
            },
        );
        let mut hidden = employee("E-11", "EMPLOYED", json!({"employee_id": "11"}));
        hidden.legacy_intervals.push(LegacyInterval {
            dimension: "state".into(),
            value: json!("EMPLOYED"),
            entered_at: port - 2 * day,
            left_at: Some(port - day),
            entered_by: Some("jo\u{200B}@corp".into()),
            entered_by_kind: Some("human".into()),
        });
        for (words, o) in [
            ("is undecided", bare),
            ("only an observation takes", dated),
            ("after the import", ahead),
            ("a time after the import", made),
            ("a time after the import", entered),
            ("a time after the import", interval),
            ("a time after the import", silent),
            ("a time before 1900", empty),
            ("a kind of actor robot", robot),
            ("past 65536 bytes", long),
            ("an actor it names", hidden),
        ] {
            let r = run(&mut s, vec![o], &[], false);
            let reason = r.refusal.map(|f| f.reason).unwrap_or_default();
            assert!(reason.contains(words), "{words}: {reason}");
            assert_eq!(r.written, 0, "{words}");
        }
        // 1900 itself is taken
        let mut floor = employee("E-8", "EMPLOYED", json!({"employee_id": "8"}));
        floor.created_at = Some(IMPORT_FLOOR);
        let r = run(&mut s, vec![floor], &[], false);
        assert_eq!((r.written, r.refusal), (1, None));
    }

    /// Legacy entries (ADR-0163 decisions 2 to 4, `publish-and-import.md` §4):
    /// each entry's own fields stay with its object, a field about another
    /// object moves to that object's history, the kind's kept fields are
    /// written with each entry, a batch sent again writes nothing, a field
    /// about an object nobody holds is a `FIELD` class, and an erasure leaves
    /// an entry its kept fields alone, counting them on its event.
    #[test]
    fn legacy_entries_port_move_and_are_erased_by_their_kept_fields() {
        let day = 86_400;
        let mut s = store();
        let port = s.now;
        let audit = |at: i64, payload: Json, about: &[(&str, &str)]| EntryIn {
            kind: "audit".into(),
            occurred_at: at,
            payload: payload.as_object().cloned().unwrap_or_default(),
            about: about
                .iter()
                .map(|(f, t)| (f.to_string(), t.to_string()))
                .collect(),
        };
        let mut ana = employee(
            "E-1",
            "EMPLOYED",
            json!({"employee_id": "1", "name": "Ana"}),
        );
        ana.legacy_entries = vec![
            audit(port - 10 * day, json!({"action": "hired", "by": "hr"}), &[]),
            audit(
                port - 5 * day,
                json!({"action": "moved", "team_name": "Tools"}),
                &[("team_name", "Team:T-1")],
            ),
        ];
        let mut team = employee("T-1", "ACTIVE", json!({"code": "T1"}));
        team.type_ = "Team".into();
        let batch = ImportBatch {
            objects: vec![ana.clone(), team.clone()],
            sequences: Vec::new(),
            legacy_kinds: BTreeMap::from([("audit".to_string(), vec!["action".to_string()])]),
        };
        let r = import_batch(&mut s, OPERATOR, &batch, &[], false)
            .unwrap()
            .unwrap();
        assert_eq!((r.written, r.refusal.clone()), (2, None));
        let entries = |s: &Store, key: &str| {
            let w = s.snapshot().unwrap();
            let id = w.external_all().unwrap()[&("legacy".to_string(), key.to_string())].clone();
            w.legacy_of(&id).unwrap()
        };
        let e = entries(&s, "Employee:E-1");
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].ordinal, e[1].ordinal), (1, 2));
        assert_eq!(
            serde_json::Value::Object(e[1].payload.clone()),
            json!({"action": "moved"})
        );
        assert_eq!(e[0].kept, vec!["action".to_string()]);
        let t = entries(&s, "Team:T-1");
        assert_eq!(t.len(), 1);
        assert_eq!(
            serde_json::Value::Object(t[0].payload.clone()),
            json!({"team_name": "Tools"})
        );
        assert_eq!(t[0].occurred_at, port - 5 * day);
        // sent again, it writes nothing and is not refused
        let again = import_batch(&mut s, OPERATOR, &batch, &[], false)
            .unwrap()
            .unwrap();
        assert_eq!((again.written, again.refusal), (0, None));
        assert_eq!(entries(&s, "Employee:E-1").len(), 2);
        // a field about a team neither the batch nor the store holds is a
        // FIELD class: reported by a dry run, refused undecided, and left
        // behind when the file drops it
        let mut bo = employee("E-2", "EMPLOYED", json!({"employee_id": "2"}));
        bo.legacy_entries = vec![audit(
            port - day,
            json!({"action": "moved", "team_name": "Ghosts"}),
            &[("team_name", "Team:T-9")],
        )];
        let lone = ImportBatch {
            objects: vec![bo],
            sequences: Vec::new(),
            legacy_kinds: BTreeMap::new(),
        };
        let dry = import_batch(&mut s, OPERATOR, &lone, &[], true)
            .unwrap()
            .unwrap();
        let class = dry
            .findings
            .iter()
            .find(|c| c.kind == ViolationKind::Field)
            .cloned()
            .expect("a FIELD class");
        assert_eq!(class.rule, "audit.team_name");
        let undecided = import_batch(&mut s, OPERATOR, &lone, &[], false)
            .unwrap()
            .unwrap();
        assert!(undecided.refusal.unwrap().reason.contains("is undecided"));
        let file = [decided(&class, Disposition::Drop, "no such team")];
        let r = import_batch(&mut s, OPERATOR, &lone, &file, false)
            .unwrap()
            .unwrap();
        assert_eq!((r.written, r.refusal), (1, None));
        assert_eq!(
            serde_json::Value::Object(entries(&s, "Employee:E-2")[0].payload.clone()),
            json!({"action": "moved"})
        );
        // an entry that names no kind is refused
        let mut nameless = employee("E-3", "EMPLOYED", json!({"employee_id": "3"}));
        nameless.legacy_entries = vec![EntryIn {
            kind: " ".into(),
            ..audit(port - day, json!({}), &[])
        }];
        let r = run(&mut s, vec![nameless], &[], false);
        assert!(r.refusal.unwrap().reason.contains("names no kind"));
        // an erasure leaves each entry its kept fields, and counts them
        let id = id_of(&s, "E-1");
        let erase = Request {
            transition: "forget".into(),
            actor: OPERATOR.into(),
            object_id: Some(id.clone()),
            type_: None,
            inputs: [("reason".to_string(), json!("asked"))]
                .into_iter()
                .collect(),
            occurred_at: None,
            propose: false,
            expires_at: None,
            expected_version: None,
            target_version: None,
            principal: None,
            idempotency_key: None,
            context: None,
        };
        assert!(matches!(
            request(&mut s, &erase).unwrap().unwrap(),
            Verdict::Satisfied { .. }
        ));
        let e = entries(&s, "Employee:E-1");
        assert_eq!(
            serde_json::Value::Object(e[0].payload.clone()),
            json!({"action": "hired", "by": null})
        );
        let forgot = s.history(&id).unwrap().pop().unwrap();
        assert_eq!(forgot.payload["legacy_entries"], json!(2));
        // the team's entry, a field moved from the employee's, is the team's
        assert_eq!(
            entries(&s, "Team:T-1")[0].payload["team_name"],
            json!("Tools")
        );
    }
}

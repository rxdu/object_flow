//! The import: a port of another system's records into this store's mirror
//! types, one transaction per batch, each object written through the
//! built-in assertion, each class of violation decided by a person in the
//! disposition file (`publish-and-import.md` §4, §6, ADR-0154).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value as Json, json};

use crate::eval::NotRun;
use crate::model::{Declared, Library, Type};
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

/// The shapes of a class, fixed texts, never a value (ADR-0154, C31).
const DOES_NOT_HOLD: &str = "does not hold";
const IS_ABSENT: &str = "is absent";
const NOT_DECLARED: &str = "is not declared";
const NOT_OF_ITS_TYPE: &str = "is not of its type";
const IS_SHARED: &str = "is shared";
const NAMES_EXCLUDED: &str = "names an excluded object";

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
    let mut report = ImportReport::default();
    if let Some(what) = batch.objects.iter().find_map(|o| o.beyond.first()) {
        return Err(NotRun(format!("an import of {what}")));
    }
    let refused = |mut report: ImportReport, r: ImportRefusal| {
        report.refusal = Some(r);
        Ok((report, None))
    };
    let Some(who) = resolve_actor(world, actor) else {
        return refused(
            report,
            ImportRefusal {
                type_: String::new(),
                legacy_key: String::new(),
                reason: format!("actor_known: the store holds no actor {actor}"),
            },
        );
    };
    // owned types, which the import may not write (C33)
    for o in &batch.objects {
        if lib.type_(&o.type_).is_some_and(|t| !t.mirror)
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
        if !t.mirror {
            return refused(
                report,
                refusal(
                    o,
                    format!("{} is owned by this store, not a mirror", o.type_),
                ),
            );
        }
        if let Some(what) = t.beyond.first() {
            return Err(NotRun(what.clone()));
        }
        let key = legacy(lib, &o.type_, &o.legacy_key);
        if ids.contains_key(&key) {
            return refused(
                report,
                refusal(o, "its legacy key appears twice in the batch"),
            );
        }
        let held = match world.external.get(&key) {
            Some(id) => match world.get(id) {
                Some(h) if h.type_ == o.type_ => Some(h.clone()),
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
            if a.personal && p.held.as_ref().is_some_and(|h| h.erased) {
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
    // a key naming nothing in the batch or the store refuses the batch,
    // whatever an earlier file excluded (C29)
    for Reference {
        index,
        attribute: k,
        named,
        ..
    } in &references
    {
        for key in named {
            if !ids.contains_key(key) && !world.external.contains_key(key) {
                let o = &batch.objects[*index];
                return refused(
                    report,
                    refusal(
                        o,
                        format!("{k} names {}, which the batch and the store lack", key.1),
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
            .filter(|k| !world.external.contains_key(k))
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
            if named.iter().any(|key| excluded_keys.contains(key)) {
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
                    order: work.next_order(),
                    superseded_by: None,
                    spans: BTreeMap::new(),
                    writes: BTreeMap::new(),
                    admissions: BTreeSet::new(),
                    erased: false,
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
                    .filter_map(|key| ids.get(key).or_else(|| world.external.get(key)))
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
            work.objects.insert(obj.id.clone(), obj);
        }
        // the classes, found once over every object the batch writes (C34)
        for p in placed.iter().filter(|p| !excluded.contains(&p.index)) {
            let o = &batch.objects[p.index];
            let (Some(t), Some(obj)) = (lib.type_(&o.type_), work.get(&p.id)) else {
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
                    let shared = work.objects.values().any(|x| {
                        x.id != obj.id
                            && lib
                                .type_(&x.type_)
                                .and_then(|xt| xt.attribute(a))
                                .is_some_and(|xa| xa.external.as_ref() == Some(source))
                            && text_of(&x.attr(a)) == value
                    });
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
            for n in failing_invariants(&work, obj)? {
                if !obj.admissions.contains(&n) {
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
                    && let Some(obj) = work.objects.get_mut(&p.id)
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
            (ViolationKind::Invariant, Disposition::Admit) | (_, Disposition::Exclude)
                if reason.is_none() =>
            {
                Some(format!("is decided {} with no reason", disposition.text()))
            }
            (ViolationKind::Invariant, Disposition::Admit) | (_, Disposition::Exclude) => None,
            (_, Disposition::Admit) => Some("is a guard, which nothing admits".to_string()),
            (ViolationKind::Reference, Disposition::Drop) => {
                let required = references
                    .iter()
                    .any(|r| objects.contains(&r.index) && r.attribute == c.rule && !r.optional);
                required.then(|| "drops a required reference".to_string())
            }
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
    if dry_run {
        return Ok((report, None));
    }
    // the writes, in the batch's order
    let mut change = Change::default();
    let port = world.now;
    for p in placed.iter().filter(|p| !excluded.contains(&p.index)) {
        let o = &batch.objects[p.index];
        let (Some(t), Some(mut obj)) = (lib.type_(&o.type_), work.get(&p.id).cloned()) else {
            continue;
        };
        let changed = match &p.held {
            None => true,
            Some(h) => h.state != obj.state || h.attrs != obj.attrs,
        };
        if !changed {
            continue; // a refresh that changes nothing writes nothing
        }
        let position = alloc.position();
        alloc.txn();
        if let Some(r) = open_spans(lib, t, &mut obj, p.held.as_ref(), o, position, port) {
            return refused(report, refusal(o, r));
        }
        if p.held.is_none() {
            // the earlier values the legacy record gives, at position 0, and
            // the times the record supplies held to its creation and its
            // silent spans (publish-and-import.md §4)
            let resolve = |key: &str, target: &str| {
                let k = legacy(lib, target, key);
                ids.get(&k).or_else(|| world.external.get(&k)).cloned()
            };
            match legacy_spans(lib, t, &obj, o, &resolve) {
                Ok(spans) => {
                    for (d, sp) in spans {
                        let row = obj.spans.entry(d.clone()).or_default();
                        let at = row.iter().position(|x| !x.legacy).unwrap_or(row.len());
                        row.insert(at, sp.clone());
                        change.spans.push((obj.id.clone(), d, sp));
                    }
                }
                Err(r) => return refused(report, refusal(o, r)),
            }
            created(lib, t, &mut obj, o, port);
        } else if !o.legacy_intervals.is_empty() {
            return Err(NotRun(format!(
                "a refresh's intermediate values, on {} {}",
                o.type_, o.legacy_key
            )));
        }
        // admissions: what an ADMIT class holds, with its reason; one that
        // holds again is discharged (ADR-0054, C14)
        let failing = failing_invariants(&work, &obj)?;
        for n in obj.admissions.clone() {
            if !failing.contains(&n) {
                obj.admissions.remove(&n);
                change.discharged.push((obj.id.clone(), n, position));
            }
        }
        for (c, objects) in &found_classes {
            if c.kind == ViolationKind::Invariant && objects.contains(&p.index) {
                let reason = decided(c)
                    .and_then(|f| f.reason.clone())
                    .unwrap_or_default();
                obj.admissions.insert(c.rule.clone());
                change
                    .admitted
                    .push((obj.id.clone(), c.rule.clone(), position, reason));
            }
        }
        obj.version += 1;
        let from_state = p.held.as_ref().map(|h| h.state.clone());
        let event = Event {
            position,
            object_id: obj.id.clone(),
            object_seq: obj.version,
            transition: "import".into(),
            from_state: from_state.clone(),
            to_state: obj.state.clone(),
            changes_state: from_state.as_deref() != Some(obj.state.as_str()),
            source: "asserted".into(),
            actor_id: who.id.clone(),
            actor_kind: who.kind.clone(),
            declaration_version: lib.version,
            recorded_at: port,
            occurred_at: Some(port),
            imported: true,
            payload: json!({ "inputs": o.attributes }),
        };
        // what it wrote, in the write index
        let before = p.held.as_ref().map(|h| h.attrs.clone()).unwrap_or_default();
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
    Ok((report, Some(change)))
}

/// A new object's creation time and kind, whether it is dated, and its
/// `recorded_from` (`publish-and-import.md` §4, ADR-0154, C24, C37): its
/// legacy creation where its state's intervals run unbroken from it, the end
/// of its last silent span, and the port where its state's current interval
/// begins there, the latest of those that apply; for an object none
/// describes, the earliest entry its state's intervals and those supplied
/// for a tracked member give.
fn created(lib: &Library, t: &Type, obj: &mut Object, o: &ImportedObject, port: i64) {
    let tracked = lib.tracked(t);
    let entries: Vec<i64> = obj
        .spans
        .iter()
        .filter(|(m, _)| tracked.contains(*m))
        .flat_map(|(m, spans)| {
            spans
                .iter()
                .filter(move |sp| m == "state" || sp.legacy || o.entered.contains_key(m))
                .map(|sp| sp.entered_at)
        })
        .collect();
    let earliest = entries.iter().copied().min();
    obj.created_dated = o.created_at.is_some();
    obj.created_at = o.created_at.or(earliest).unwrap_or(port).min(port);
    obj.created_by_kind = o
        .created_by_kind
        .clone()
        .unwrap_or_else(|| "unknown".into());
    let state = obj.spans.get("state").map_or(&[][..], Vec::as_slice);
    let whole = o.created_at.is_some_and(|c| {
        state.first().is_some_and(|first| first.entered_at == c)
            && state
                .windows(2)
                .all(|w| w[0].left_at == Some(w[1].entered_at))
    });
    let mut apply = Vec::new();
    if whole {
        apply.extend(o.created_at);
    }
    apply.extend(o.silent_spans.iter().map(|(_, end)| *end).max());
    if !o.entered.contains_key("state") {
        apply.push(port);
    }
    obj.recorded_from = apply.into_iter().max().or(earliest).unwrap_or(port);
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
    obj: &Object,
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
        let current = obj
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
            let value = if d == "state" {
                match l.value.as_str() {
                    Some(s) if t.state(s).is_some() => Value::State {
                        type_: t.name.clone(),
                        name: s.to_string(),
                    },
                    _ => {
                        return Err(format!(
                            "a legacy interval in a state {} does not declare",
                            t.name
                        ));
                    }
                }
            } else {
                let Some(a) = t.attribute(d) else {
                    return Err(format!(
                        "a legacy interval of {d}, which {} does not declare",
                        t.name
                    ));
                };
                match (&a.declared, &l.value) {
                    (_, Json::Null) => Value::Absent,
                    (Declared::Reference(target), Json::String(key)) => {
                        match resolve(key, target) {
                            Some(id) => Value::Object(id),
                            None => {
                                return Err(format!(
                                    "a legacy interval of {d} naming {key}, which no object of the batch or the store has as its legacy key"
                                ));
                            }
                        }
                    }
                    (declared, raw) => match wire::decode(raw, declared, false, lib) {
                        Ok(v) => v,
                        Err(_) => {
                            return Err(format!(
                                "a legacy interval of {d} holding a value it cannot"
                            ));
                        }
                    },
                }
            };
            out.push((
                d.to_string(),
                Span {
                    value,
                    entered_position: 0,
                    left_position: Some(0),
                    entered_at: l.entered_at,
                    left_at: Some(left),
                    entered_by: l.entered_by.clone(),
                    entered_by_kind: l
                        .entered_by_kind
                        .clone()
                        .unwrap_or_else(|| "unknown".into()),
                    migrated: false,
                    declaration_version: lib.version,
                    legacy: true,
                },
            ));
        }
    }
    Ok(out)
}

/// The intervals an import opens: for a new object, one for the state and
/// every tracked member; for a refresh, one for each value it changes, the
/// current one ending. Each opens when `entered` says, by the changer it
/// names, and otherwise at the import, by a changer of kind `unknown`
/// (ADR-0106, C28). A refresh whose entry precedes the latest entry of the
/// same dimension is refused, its reason returned.
fn open_spans(
    lib: &Library,
    t: &Type,
    obj: &mut Object,
    held: Option<&Object>,
    o: &ImportedObject,
    position: i64,
    port: i64,
) -> Option<String> {
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
        if was.as_ref() == Some(&now) {
            continue;
        }
        let entered = o.entered.get(&m);
        let at = entered.map_or(port, |e| e.at);
        let spans = obj.spans.entry(m.clone()).or_default();
        if let Some(last) = spans.last_mut() {
            if at < last.entered_at {
                return Some(format!(
                    "its {m} entered before the latest interval the store holds"
                ));
            }
            if last.left_position.is_none() {
                last.left_position = Some(position);
                last.left_at = Some(at);
            }
        }
        spans.push(Span {
            value: now,
            entered_position: position,
            left_position: None,
            entered_at: at,
            left_at: None,
            entered_by: entered.and_then(|e| e.by.clone()),
            entered_by_kind: entered
                .and_then(|e| e.by_kind.clone())
                .unwrap_or_else(|| "unknown".into()),
            migrated: false,
            declaration_version: lib.version,
            legacy: false,
        });
    }
    None
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
        w.external
            .get(&("legacy".to_string(), format!("Employee:{key}")))
            .cloned()
            .unwrap()
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
        let o = w.get(&id).unwrap();
        assert_eq!(
            (o.state.as_str(), o.attr("name")),
            ("EMPLOYED", Value::Text("Dana".into()))
        );
        assert_eq!(
            w.external.get(&("hr".to_string(), "1".to_string())),
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
        assert_eq!(s.snapshot().unwrap().get(&id).unwrap().state, "LEFT");
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
            s.snapshot().unwrap().external.is_empty(),
            "nothing is written"
        );
        // a reference naming nothing in the batch or the store
        let team = ImportedObject {
            type_: "Team".into(),
            legacy_key: "T-1".into(),
            state: "ACTIVE".into(),
            attributes: [
                ("code".to_string(), json!("ops")),
                ("lead".to_string(), json!("E-404")),
            ]
            .into(),
            ..ImportedObject::default()
        };
        let r = run(&mut s, vec![team], &[], false);
        assert!(r.refusal.unwrap().reason.contains("E-404"));
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
            !w.external
                .contains_key(&("legacy".to_string(), "Employee:E-2".to_string()))
        );
        let team_id = w
            .external
            .get(&("legacy".to_string(), "Team:T-1".to_string()))
            .unwrap();
        assert_eq!(w.get(team_id).unwrap().attr("lead"), Value::Absent);
        // the admitted violation stands, and a refresh that cures it discharges it
        let low_id = id_of(&s, "E-3");
        assert!(
            s.snapshot()
                .unwrap()
                .get(&low_id)
                .unwrap()
                .admissions
                .contains("graded")
        );
        let cured = employee("E-3", "EMPLOYED", json!({"employee_id": "3", "grade": 2}));
        assert_eq!(run(&mut s, vec![cured], &[], false).written, 1);
        assert!(
            s.snapshot()
                .unwrap()
                .get(&low_id)
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
        };
        let r = import_batch(&mut s, OPERATOR, &batch, &[], false)
            .unwrap()
            .unwrap();
        assert_eq!(r.written, 1);
        let w = s.snapshot().unwrap();
        let o = w.get(&id_of(&s, "E-1")).unwrap().clone();
        let span = o.spans["state"][0].clone();
        assert_eq!(
            (
                span.entered_at,
                span.entered_by.as_deref(),
                span.entered_by_kind.as_str()
            ),
            (at, Some("hr-admin"), "human")
        );
        // its record whole from its creation, which it was given
        assert!(o.created_dated);
        assert_eq!((o.created_at, o.recorded_from), (at, at));
        // a sequence is raised past the legacy value, never lowered
        let mut alloc_value = None;
        s.transact(|_, alloc| {
            alloc_value = Some(alloc.draw("badge", ""));
            Ok::<_, NotRun>(((), None))
        })
        .unwrap()
        .unwrap();
        assert_eq!(alloc_value, Some(513));
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
        let o = w.get(&id).unwrap();
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
        let w = s.snapshot().unwrap();
        let (e1, e2, e3, e4) = (
            id_of(&s, "E-1"),
            id_of(&s, "E-2"),
            id_of(&s, "E-3"),
            id_of(&s, "E-4"),
        );
        let t1 = w.external[&("legacy".to_string(), "Team:T-1".to_string())].clone();
        let o1 = w.get(&e1).unwrap();
        let first = &o1.spans["state"][0];
        assert!(first.legacy && first.entered_position == 0 && first.left_position == Some(0));
        assert_eq!(
            (first.entered_at, first.left_at),
            (port - 100 * day, Some(port - 60 * day))
        );
        assert_eq!(o1.spans["state"].len(), 2);
        assert!(o1.created_dated);
        assert_eq!(
            (o1.created_at, o1.recorded_from),
            (port - 100 * day, port - 100 * day)
        );
        let ev = |id: &str, text: &str| {
            let t = w.lib.type_(&w.get(id).unwrap().type_);
            crate::eval::Evaluator::new(&w, t, Some(id))
                .eval(&crate::expr::parse(text).unwrap())
                .unwrap()
        };
        assert_eq!(ev(&e1, "time_in(LEFT)"), Value::Duration(40 * day));
        let o2 = w.get(&e2).unwrap();
        assert!(!o2.created_dated);
        assert_eq!(
            (o2.created_at, o2.recorded_from),
            (port - 90 * day, port - 90 * day)
        );
        assert_eq!(w.get(&e3).unwrap().recorded_from, port - 15 * day);
        assert_eq!(w.get(&e4).unwrap().recorded_from, port);
        let t2 = w
            .get(&w.external[&("legacy".to_string(), "Team:T-2".to_string())])
            .unwrap();
        assert!(!t2.created_dated);
        assert_eq!((t2.created_at, t2.recorded_from), (port - 70 * day, port));
        // the team's assignee, earlier values resolved by legacy key
        let lead: Vec<Value> = w.get(&t1).unwrap().spans["lead"]
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
            Value::Row(BTreeMap::from([
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
                s.snapshot().unwrap().external.is_empty(),
                "{words}: something was written"
            );
        }
        // a refresh's intermediate values are not yet run
        let mut s = store();
        let port = s.now;
        run(
            &mut s,
            vec![employee("E-1", "EMPLOYED", json!({"employee_id": "1"}))],
            &[],
            false,
        );
        let mut again = employee("E-1", "LEFT", json!({"employee_id": "1"}));
        again.legacy_intervals = vec![earlier("state", json!("EMPLOYED"), port, Some(port))];
        let batch = ImportBatch {
            objects: vec![again],
            sequences: Vec::new(),
        };
        let not_run = import_batch(&mut s, OPERATOR, &batch, &[], false)
            .unwrap()
            .unwrap_err();
        assert!(not_run.0.contains("intermediate values"), "{}", not_run.0);
    }
}

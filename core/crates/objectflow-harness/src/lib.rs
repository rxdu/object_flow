//! The adversarial harness, the acceptance test PRD N4 names
//! (`design/adversarial-harness.md`, ADR-0204 §6).
//!
//! A fallible actor, driven deterministically from a seed, sends the core
//! the requests a caller could: it guesses transitions and inputs, retries
//! with and without its keys, acts on a stale version, backdates, erases,
//! and lets the clock run. After each request the store is checked from its
//! read surface for what the declaration and ADR-0204 §3 say cannot happen.
//! A failing run is reduced to the shortest transcript that still fails.
//!
//! Its threat model is the design's, mistakes and not malice; and the store
//! is SQLite, whose writers are serial, so the racer of §2 finds little and
//! is left to a later slice.

use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use objectflow_core::eval::Evaluator;
use objectflow_core::model::{Declared, Kind, Library, OPERATOR, Type};
use objectflow_core::request::{NoVerdict, Request, request};
use objectflow_core::store::Store;
use objectflow_core::value::Value;
use objectflow_core::verdict::Verdict;
use serde_json::{Value as Json, json};

/// The store's clock at a run's start, 2027-01-01T00:00:00Z.
const START: i64 = 1_798_761_600;

/// A deterministic generator, splitmix64, so a run is a function of its
/// seed and needs no dependency.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            usize::try_from(self.next() % n as u64).unwrap_or(0)
        }
    }

    fn percent(&mut self, p: u64) -> bool {
        self.next() % 100 < p
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> Option<&'a T> {
        if xs.is_empty() {
            None
        } else {
            let i = self.below(xs.len());
            xs.get(i)
        }
    }
}

/// A set of flows a run is driven against: the shipped examples, each with
/// the modules it imports, the first consumer's types among them
/// (`adversarial-harness.md` §3).
#[derive(Clone, Debug)]
pub struct Fixture {
    pub name: String,
    pub modules: Vec<(String, String)>,
}

/// The fixtures, read from the examples' directory.
pub fn fixtures(examples: &std::path::Path) -> std::io::Result<Vec<Fixture>> {
    let sets: [(&str, &[&str]); 8] = [
        ("service", &["people.yaml", "service.yaml"]),
        ("issues", &["people.yaml", "issues.yaml"]),
        ("delivery", &["inventory.yaml", "delivery.yaml"]),
        ("reimbursement", &["reimbursement.yaml"]),
        ("servicedesk", &["servicedesk.yaml"]),
        ("customers", &["customers.yaml"]),
        ("approvals", &["approvals.yaml"]),
        ("bookings", &["bookings.yaml"]),
    ];
    let mut out = Vec::new();
    for (name, files) in sets {
        let mut modules = Vec::new();
        for f in files {
            modules.push((f.to_string(), std::fs::read_to_string(examples.join(f))?));
        }
        out.push(Fixture {
            name: name.to_string(),
            modules,
        });
    }
    // the unit's journey, the first consumer's lifecycle written from its
    // production code, which the design names as the fixture (§3)
    let doc = std::fs::read_to_string(examples.join("../../unit-journey.md"))?;
    out.push(Fixture {
        name: "unit journey".to_string(),
        modules: journey(&doc),
    });
    Ok(out)
}

/// The modules a design document holds, assembled as `check-flow-docs.py`
/// assembles them: a ```yaml block beginning `module: <name>` begins one,
/// a block beginning `# <name>, continued` continues it, a first line
/// naming the block's file is no part of it, and a module begun again is
/// its next version, which replaces the earlier.
fn journey(doc: &str) -> Vec<(String, String)> {
    let mut modules: Vec<(String, String)> = Vec::new();
    let mut lines = doc.lines();
    while let Some(line) = lines.next() {
        if line.trim_end() != "```yaml" {
            continue;
        }
        let mut body: Vec<&str> = lines.by_ref().take_while(|l| l.trim() != "```").collect();
        let names_file = |l: &str| {
            l.strip_prefix("# ")
                .is_some_and(|f| f.trim_end().ends_with(".yaml") && !f.contains(' '))
        };
        if body.first().is_some_and(|l| names_file(l)) {
            body.remove(0);
        }
        let Some(first) = body.first() else {
            continue;
        };
        let text = body.join("\n") + "\n";
        if let Some(name) = first.strip_prefix("module:").map(str::trim) {
            modules.retain(|(n, _)| n != name);
            modules.push((name.to_string(), text));
        } else if let Some(name) = first
            .strip_prefix("# ")
            .and_then(|r| r.trim_end().strip_suffix(", continued"))
            && let Some((_, held)) = modules.iter_mut().find(|(n, _)| n == name)
        {
            held.push_str(&text);
        }
    }
    modules
        .into_iter()
        .map(|(n, t)| (format!("{n}.yaml"), t))
        .collect()
}

/// One step of a run, as concrete as a caller's: a request, or the clock
/// moving on.
#[derive(Clone, Debug)]
pub enum Step {
    Request(Box<Request>),
    Advance(i64),
    /// An evaluator function's answer changed: `<evaluator>.<function>` to
    /// `satisfied`, `unsatisfied` or `unanswered`, as a sandbox's caller sets
    /// it (PRD F13).
    Answer(String, String),
}

/// A state the declaration or ADR-0204 §3 says cannot happen, the step it
/// was found after, and what was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub condition: &'static str,
    pub detail: String,
    pub at: usize,
}

/// What a run did, so a run that tested nothing is seen.
#[derive(Clone, Debug, Default)]
pub struct Counts {
    pub satisfied: usize,
    pub refused: usize,
    pub not_run: usize,
    pub key_reused: usize,
    pub replayed: usize,
    pub erasures: usize,
    /// What the engine did not run, by its reason: on a flow both checkers
    /// accept, each is a broken K of ADR-0204 §3, for the audit to close.
    pub not_run_reasons: BTreeSet<String>,
}

/// A run's transcript and its first failure, if any.
#[derive(Debug)]
pub struct Outcome {
    pub steps: Vec<Step>,
    pub failure: Option<Failure>,
    pub counts: Counts,
}

/// A run in progress: the store, and what the checks remember of the
/// requests it took.
struct Run {
    store: Store,
    /// Each object's personal values, given in a request it satisfied.
    markers: BTreeMap<String, BTreeSet<String>>,
    /// Each actor's keys, with the request first sent under it, the object
    /// it answered and whether it gave that object a personal value.
    keys: BTreeMap<(String, String), (Request, String, bool)>,
    /// The objects an erasure has erased.
    erased: BTreeSet<String>,
    counts: Counts,
    /// Where the store is copied before each request, to decide it again
    /// over what its record says it read.
    fork: ForkFile,
}

/// The file a run copies its store to, a name no other run of the process
/// uses, removed with the run.
struct ForkFile(std::path::PathBuf);

impl ForkFile {
    fn new() -> ForkFile {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // memory's file system where there is one, a copy each request
        let shm = std::path::Path::new("/dev/shm");
        let dir = if shm.is_dir() {
            shm.to_path_buf()
        } else {
            std::env::temp_dir()
        };
        ForkFile(dir.join(format!("objectflow-harness-{}-{n}.db", std::process::id())))
    }
}

impl Drop for ForkFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The objects a read set names: those read, and each scan's matches.
fn read_set(reads: &Json, kept: &mut BTreeSet<String>) {
    if let Some(objects) = reads.get("objects").and_then(Json::as_object) {
        kept.extend(objects.keys().cloned());
    }
    for types in reads
        .get("scans")
        .and_then(Json::as_object)
        .into_iter()
        .flat_map(|rules| rules.values())
        .filter_map(Json::as_object)
    {
        for ids in types.values().filter_map(Json::as_array) {
            kept.extend(ids.iter().filter_map(Json::as_str).map(str::to_string));
        }
    }
}

impl Run {
    fn new(fx: &Fixture) -> Result<Run, String> {
        let lib =
            Library::load(&fx.modules).map_err(|e| format!("{} does not load: {e:?}", fx.name))?;
        let store = Store::sandbox(lib, START).map_err(|e| format!("{e:?}"))?;
        Ok(Run {
            store,
            markers: BTreeMap::new(),
            keys: BTreeMap::new(),
            erased: BTreeSet::new(),
            counts: Counts::default(),
            fork: ForkFile::new(),
        })
    }

    /// A step taken, and what the checks found after it.
    fn take(&mut self, step: &Step, at: usize) -> Option<Failure> {
        let fail = |condition: &'static str, detail: String| Failure {
            condition,
            detail,
            at,
        };
        match step {
            Step::Advance(s) => {
                self.store.now += s;
                None
            }
            Step::Answer(call, said) => {
                self.store.answers.insert(call.clone(), said.clone());
                None
            }
            Step::Request(r) => {
                let logged = self.attempts_held();
                // the store as the request finds it, to decide it again over
                // its read set alone
                // a copy that fails fails the run, where it was dropped and the
                // conditions deciding over it skipped without a word, as they
                // were on PostgreSQL before its fork (ADR-0213)
                let before = match self.store.fork(&self.fork.0) {
                    Ok(copy) => Some(copy),
                    Err(e) => {
                        return Some(fail(
                            "fork",
                            format!("the store could not be copied to decide again: {e:?}"),
                        ));
                    }
                };
                let answered = catch_unwind(AssertUnwindSafe(|| request(&mut self.store, r)));
                let verdict = match answered {
                    Err(_) => return Some(fail("panic", format!("{r:?}"))),
                    Ok(Err(e)) => return Some(fail("store error", format!("{e:?} for {r:?}"))),
                    Ok(Ok(Err(NoVerdict::NotRun(e)))) => {
                        self.counts.not_run += 1;
                        // an evaluator the run set unanswered is the sandbox's,
                        // not the engine's (PRD F13)
                        let said = format!("{e:?}");
                        if !said.contains("Unanswered(") {
                            self.counts
                                .not_run_reasons
                                .insert(format!("{}: {said}", r.transition));
                        }
                        return self.check(at);
                    }
                    Ok(Ok(Err(NoVerdict::KeyReused { .. }))) => {
                        self.counts.key_reused += 1;
                        if let Some(f) = self.keyed(r, None) {
                            return Some(fail("key", f));
                        }
                        return self.check(at);
                    }
                    Ok(Ok(Ok(v))) => v,
                };
                if let Some(f) = self.keyed(r, Some(&verdict)) {
                    return Some(fail("key", f));
                }
                match &verdict {
                    Verdict::Satisfied {
                        object_id,
                        events,
                        replayed,
                        ..
                    } => {
                        self.counts.satisfied += 1;
                        if *replayed {
                            self.counts.replayed += 1;
                        }
                        if let Some(f) = self.recorded(r, object_id, events, *replayed) {
                            return Some(fail("record", f));
                        }
                        if !*replayed
                            && let Some(f) = before.and_then(|b| self.redecided(r, b, Some(events)))
                        {
                            return Some(fail("read set", f));
                        }
                        self.remember(r, object_id);
                        if self.erasure(r, object_id) && !*replayed {
                            self.counts.erasures += 1;
                            self.erased.insert(object_id.clone());
                            if let Some(f) = self.erased(object_id) {
                                return Some(fail("erasure", f));
                            }
                        }
                    }
                    _ => {
                        self.counts.refused += 1;
                        if let Some(f) = self.attempt_logged(r, logged) {
                            return Some(fail("attempts", f));
                        }
                        if let Some(f) = before.and_then(|b| self.redecided(r, b, None)) {
                            return Some(fail("refusal", f));
                        }
                    }
                }
                self.check(at)
            }
        }
    }

    /// A decision taken again over what its record says it read (§1, "an
    /// event whose rules, re-evaluated over its read set, give a different
    /// verdict" and "a refusal that, replayed over its read set and the
    /// request values it recorded, does not refuse"; ADR-0088, ADR-0105):
    /// the store as the request found it, keeping only the objects its
    /// events' read sets name, or its attempt row's, its own objects, and
    /// the actor's object, whose resolution no rule reads (ADR-0114), and the
    /// objects the values the request and each of its steps recorded name,
    /// whose being there is an input's check (ADR-0105), must satisfy the
    /// request again where it was satisfied, unless an effect stops, which
    /// reads what no rule read, and refuse it where it was refused. A
    /// refusal naming a personal input as withheld is the condition's
    /// exception, its values not all recorded.
    fn redecided(&self, r: &Request, mut before: Store, events: Option<&[i64]>) -> Option<String> {
        let world = self.store.snapshot().ok()?;
        let mut kept: BTreeSet<String> = BTreeSet::new();
        // the values the request and each step of it recorded, whose
        // objects' being there is the input's check, no rule's (ADR-0105)
        let mut values: Vec<Json> = r.inputs.values().cloned().collect();
        match events {
            Some(positions) => {
                for e in world
                    .log()
                    .ok()?
                    .filter(|e| positions.contains(&e.position))
                {
                    read_set(&e.reads, &mut kept);
                    kept.insert(e.object_id.clone());
                    if let Some(given) = e.payload.get("inputs").and_then(Json::as_object) {
                        values.extend(given.values().cloned());
                    }
                }
            }
            None => {
                let row = world.attempts().ok()?.last()?.clone();
                if !row.withheld.is_empty() {
                    return None;
                }
                read_set(&row.reads, &mut kept);
                kept.extend(row.object_id.clone());
                if let Some(given) = row.request_values.get("inputs").and_then(Json::as_object) {
                    values.extend(given.values().cloned());
                }
            }
        }
        kept.extend(r.object_id.clone());
        // each object those values name
        fn named(v: &Json, out: &mut Vec<String>) {
            match v {
                Json::String(s) => out.push(s.clone()),
                Json::Array(xs) => xs.iter().for_each(|x| named(x, out)),
                _ => {}
            }
        }
        let mut ids = Vec::new();
        values.iter().for_each(|v| named(v, &mut ids));
        for id in ids {
            if matches!(world.get(&id), Ok(Some(_))) {
                kept.insert(id);
            }
        }
        // the actor's object, by its identity, as the engine resolves it
        for o in world.in_order().ok()? {
            let Some((attr, _)) = world.lib.type_(&o.type_).and_then(|t| t.actor_identity()) else {
                continue;
            };
            if matches!(o.attr(attr), Value::Text(id) if id.eq_ignore_ascii_case(&r.actor)) {
                kept.insert(o.id.clone());
            }
        }
        drop(world);
        before.keep_only(&kept).ok()?;
        let again = catch_unwind(AssertUnwindSafe(|| request(&mut before, r)))
            .ok()?
            .ok()?;
        let satisfied = matches!(again, Ok(Verdict::Satisfied { .. }));
        // what an effect stopped at, which reads what no rule read and the
        // read set does not record (ADR-0088 records the rules' reads): a
        // step not run, a call or creation refused, or arithmetic past its
        // bound, which an effect computes as a guard may
        let by_effect = matches!(
            &again,
            Err(NoVerdict::NotRun(_)) | Ok(Verdict::CallRefused { .. })
        ) || matches!(&again, Ok(Verdict::Unsatisfied { clause, .. }) if clause == "value_in_range");
        match (events.is_some(), satisfied) {
            (true, false) if !by_effect => Some(format!(
                "{} was satisfied, and over its read set alone, {kept:?}, answered {again:?}",
                r.transition
            )),
            (false, true) => Some(format!(
                "{} was refused, and over its read set alone, {kept:?}, satisfied",
                r.transition
            )),
            _ => None,
        }
    }

    /// How many rows the attempt log holds.
    fn attempts_held(&self) -> Option<usize> {
        let world = self.store.snapshot().ok()?;
        world.attempts().ok().map(<[_]>::len)
    }

    /// A refusal is a row of the attempt log, one, naming its transition
    /// and its verdict (§1, "a request's refusal missing from the attempt
    /// log"; ADR-0083); the run prunes none.
    fn attempt_logged(&self, r: &Request, before: Option<usize>) -> Option<String> {
        let before = before?;
        let world = self.store.snapshot().ok()?;
        let rows = world.attempts().ok()?;
        if rows.len() != before + 1 {
            return Some(format!(
                "a refusal of {} left the attempt log {} rows, where it held {before} and owed one more",
                r.transition,
                rows.len()
            ));
        }
        let last = rows.last()?;
        if last.transition.as_deref() != Some(r.transition.as_str()) || last.verdict == "satisfied"
        {
            return Some(format!(
                "a refusal of {} was logged as {:?}, {}",
                r.transition, last.transition, last.verdict
            ));
        }
        None
    }

    /// A key's promise (ADR-0204 §3 A): sent again with the request it was
    /// first sent with, it replays that request's answer; sent with
    /// another transition, it is refused. Once an erasure has erased the
    /// personal value the request gave its object, every request under the
    /// key is refused, so it confirms no guess of the value (ADR-0205
    /// decision 3). A key is first recorded by the request it satisfied.
    fn keyed(&mut self, r: &Request, verdict: Option<&Verdict>) -> Option<String> {
        let key = r.idempotency_key.clone()?;
        let at = (r.actor.clone(), key);
        match (self.keys.get(&at), verdict) {
            (Some((first, object, gave)), v) => {
                let spent = *gave && self.erased.contains(object);
                let same = first.transition == r.transition
                    && first.object_id == r.object_id
                    && first.type_ == r.type_
                    && first.inputs == r.inputs
                    && first.occurred_at == r.occurred_at
                    && first.propose == r.propose
                    && first.expires_at == r.expires_at
                    && first.principal == r.principal
                    && first.context == r.context;
                match v {
                    None if spent => None,
                    Some(Verdict::Satisfied {
                        object_id,
                        replayed: true,
                        ..
                    }) if same && object_id == object && !spent => None,
                    None if !same && first.transition != r.transition => None,
                    _ if !same && first.transition == r.transition => None,
                    _ => Some(format!(
                        "{:?} under the key of {:?}, which answered {object}, was answered {v:?}",
                        r, first
                    )),
                }
            }
            (None, Some(Verdict::Satisfied { object_id, .. })) => {
                let gave = !self.given(r, object_id).is_empty();
                self.keys.insert(at, (r.clone(), object_id.clone(), gave));
                None
            }
            (None, None) => Some(format!(
                "{r:?} was refused key_reused under a key none used"
            )),
            _ => None,
        }
    }

    /// A satisfied verdict names events of its request, made by the actor
    /// it named (ADR-0204 §3 A, `adversarial-harness.md` §1).
    fn recorded(
        &self,
        r: &Request,
        object: &str,
        events: &[i64],
        replayed: bool,
    ) -> Option<String> {
        if events.is_empty() && !replayed {
            return Some(format!("{r:?} was satisfied on {object} with no event"));
        }
        let world = self.store.snapshot().ok()?;
        for p in events {
            match world.event_at(*p) {
                Ok(Some(e)) => {
                    if !replayed && !r.actor.eq_ignore_ascii_case(OPERATOR) && e.actor_id != r.actor
                    {
                        return Some(format!(
                            "{r:?} was recorded at {p} as {}, not as the actor it named",
                            e.actor_id
                        ));
                    }
                }
                _ => {
                    return Some(format!(
                        "{r:?} was satisfied naming event {p}, which the log has not"
                    ));
                }
            }
        }
        None
    }

    /// The personal values a satisfied request gave its object, so an
    /// erasure of the object can be checked for them.
    fn remember(&mut self, r: &Request, object: &str) {
        let given = self.given(r, object);
        if !given.is_empty() {
            self.markers
                .entry(object.to_string())
                .or_default()
                .extend(given);
        }
    }

    /// The personal values a request gave its object, as markers.
    fn given(&self, r: &Request, object: &str) -> BTreeSet<String> {
        let mut given = BTreeSet::new();
        let Ok(world) = self.store.snapshot() else {
            return given;
        };
        let Some(t) = world
            .type_of(object)
            .ok()
            .flatten()
            .and_then(|tn| world.lib.type_(&tn))
        else {
            return given;
        };
        let Some((_, x)) = t.transitions.iter().find(|(n, _)| *n == r.transition) else {
            return given;
        };
        let personal = |n: &str| {
            t.attribute(n).is_some_and(|a| a.personal)
                || x.inputs.iter().any(|(i, s)| i == n && s.personal)
        };
        for (n, v) in &r.inputs {
            if let Some(s) = v.as_str()
                && s.starts_with("pm")
                && s.ends_with('z')
                && personal(n)
            {
                given.insert(s.to_string());
            }
        }
        given
    }

    fn erasure(&self, r: &Request, object: &str) -> bool {
        let Ok(world) = self.store.snapshot() else {
            return false;
        };
        world
            .type_of(object)
            .ok()
            .flatten()
            .and_then(|tn| world.lib.type_(&tn))
            .and_then(|t| t.transitions.iter().find(|(n, _)| *n == r.transition))
            .is_some_and(|(_, x)| x.kind == Kind::Erasure)
    }

    /// After its erasure, none of an object's personal values is held
    /// anywhere the store keeps a value: an object, an interval, the log,
    /// an attempt (ADR-0204 §3 P, `adversarial-harness.md` §1).
    fn erased(&self, object: &str) -> Option<String> {
        let markers = self.markers.get(object)?;
        let world = self.store.snapshot().ok()?;
        let mut held = String::new();
        for o in world.in_order().ok()? {
            held.push_str(&format!("{:?}", o.attrs));
            if let Ok(p) = world.parts(o) {
                held.push_str(&format!("{:?}", p.spans));
            }
        }
        for e in world.log().ok()? {
            held.push_str(&e.payload.to_string());
            held.push_str(&e.reads.to_string());
        }
        for a in world.attempts().ok()? {
            held.push_str(&format!("{a:?}"));
        }
        markers
            .iter()
            .find(|m| held.contains(m.as_str()))
            .map(|m| format!("{object}'s personal value {m} is still held after its erasure"))
    }

    /// The checks every step is followed by (`adversarial-harness.md` §1,
    /// ADR-0204 §3 R and E).
    fn check(&self, at: usize) -> Option<Failure> {
        let fail = |condition: &'static str, detail: String| {
            Some(Failure {
                condition,
                detail,
                at,
            })
        };
        let Ok(world) = self.store.snapshot() else {
            return fail("store error", "no snapshot".into());
        };
        let Ok(objects) = world.in_order() else {
            return fail("store error", "no objects".into());
        };
        // each object's events, for its intervals against the log
        let Ok(all) = world.log() else {
            return fail("store error", "no log".into());
        };
        let mut events: BTreeMap<String, Vec<objectflow_core::store::Event>> = BTreeMap::new();
        for e in all {
            events
                .entry(e.object_id.clone())
                .or_default()
                .push(e.clone());
        }
        for o in &objects {
            if let Some(detail) = state_spans_against(&world, o, events.get(&o.id)) {
                return fail("intervals", detail);
            }
            if let Some(detail) = row_against(o, events.get(&o.id)) {
                return fail("replay", detail);
            }
            if let Some(detail) = tracked_spans_against(&world, o, events.get(&o.id)) {
                return fail("intervals", detail);
            }
            let Some(t) = world.lib.type_(&o.type_) else {
                continue;
            };
            if t.mirror {
                continue;
            }
            // an object in a state some transition of its type reaches; a
            // built-in type's objects the engine makes in the state it
            // gives them, a proposal filed pending among them
            if !t.module.is_empty() && !reached(t).contains(&o.state) {
                return fail(
                    "state",
                    format!(
                        "{} is in {}, which no transition of {} reaches",
                        o.id, o.state, t.name
                    ),
                );
            }
            let admitted = world
                .parts(o)
                .map(|p| p.admissions.clone())
                .unwrap_or_default();
            // every invariant holds, but one an admission lets stand, a
            // scan of a type over every other object, as the engine decides
            // it, where the harness scanned the object too and found every
            // booking overlapping itself (ADR-0061 decision 1, D696)
            for (name, e) in &t.invariants {
                let v = Evaluator::new(&world, Some(t), Some(&o.id))
                    .over_others()
                    .eval(e);
                if matches!(v, Ok(Value::Bool(false))) && !admitted.contains(name) {
                    return fail("invariant", format!("{}'s invariant {name} is false", o.id));
                }
            }
            // every required attribute, and every one its state requires, held
            let required = t
                .state(&o.state)
                .map(|s| s.required_attributes.clone())
                .unwrap_or_default();
            for (n, a) in &t.attributes {
                let needed = (!a.optional && !a.many) || required.contains(n);
                if needed && !a.personal && o.attr(n).is_absent() {
                    return fail("required", format!("{}'s required {n} is absent", o.id));
                }
            }
            // a stored end of a relationship its far side agrees with
            for (n, a) in &t.attributes {
                let (Declared::Reference(_), Some(far)) = (&a.declared, &a.opposite) else {
                    continue;
                };
                let near: Vec<String> = match o.attr(n) {
                    Value::Object(id) => vec![id],
                    Value::Set(xs) => xs
                        .into_iter()
                        .filter_map(|x| match x {
                            Value::Object(id) => Some(id),
                            _ => None,
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                for id in near {
                    let Ok(Some(other)) = world.get(&id) else {
                        continue;
                    };
                    let agrees = match other.attr(far) {
                        Value::Object(back) => back == o.id,
                        Value::Set(xs) => xs.contains(&Value::Object(o.id.clone())),
                        // an end computed from the other, never stored
                        Value::Absent => true,
                        _ => false,
                    };
                    if !agrees {
                        return fail(
                            "relationship",
                            format!(
                                "{}.{n} names {id}, whose {far} does not name {}",
                                o.id, o.id
                            ),
                        );
                    }
                }
            }
        }
        // the log's export carries every event, in the log's order, each
        // object's in its own (ADR-0204 §3 E)
        let Ok(log) = world.log() else {
            return fail("store error", "no log".into());
        };
        let positions: Vec<i64> = log.map(|e| e.position).collect();
        let mut exported = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let page =
                match objectflow_core::export::export_page(&world, "log", cursor.as_deref(), 50) {
                    Ok(p) => p,
                    Err(e) => return fail("events", format!("the log's export is not run: {e:?}")),
                };
            if let Some(r) = &page.refused {
                return fail("events", format!("the log's export was refused {r}"));
            }
            for line in &page.lines {
                let row: Json = serde_json::from_str(line).unwrap_or(Json::Null);
                if let Some(p) = row.get("position").and_then(Json::as_i64) {
                    exported.push(p);
                }
            }
            if !page.more || page.lines.is_empty() {
                break;
            }
            cursor = Some(page.cursor.clone());
        }
        if exported != positions {
            return fail(
                "events",
                format!(
                    "the log holds {} events and its export {}",
                    positions.len(),
                    exported.len()
                ),
            );
        }
        None
    }
}

/// Where a fold of an object's events does not reproduce its row (§1, "an
/// event whose replay does not reproduce the row", ADR-0033 decision 2,
/// ADR-0209 decision 1): its state, the last event's, and its attributes,
/// each event's `wrote` applied in its order, a null clearing one.
fn row_against(
    o: &objectflow_core::world::Object,
    events: Option<&Vec<objectflow_core::store::Event>>,
) -> Option<String> {
    let events = events?;
    let mut folded: BTreeMap<String, Json> = BTreeMap::new();
    let mut state = None;
    for e in events {
        state = Some(e.to_state.clone());
        if let Some(wrote) = e.payload.get("wrote").and_then(Json::as_object) {
            for (a, v) in wrote {
                if v.is_null() {
                    folded.remove(a);
                } else {
                    folded.insert(a.clone(), v.clone());
                }
            }
        }
    }
    if state.as_deref() != Some(o.state.as_str()) {
        return Some(format!(
            "{}'s events end in {state:?}, and it is in {}",
            o.id, o.state
        ));
    }
    let held: BTreeMap<String, Json> = o
        .attrs
        .iter()
        .filter(|(_, v)| !v.is_absent())
        .map(|(a, v)| (a.clone(), objectflow_core::wire::encode(v)))
        .collect();
    if folded != held {
        let differ: Vec<&String> = held
            .keys()
            .chain(folded.keys())
            .filter(|a| held.get(*a) != folded.get(*a))
            .collect();
        return Some(format!(
            "{}'s events do not reproduce its row: {differ:?} differ, held {:?}, folded {:?}",
            o.id,
            differ.iter().map(|a| held.get(*a)).collect::<Vec<_>>(),
            differ.iter().map(|a| folded.get(*a)).collect::<Vec<_>>()
        ));
    }
    None
}

/// Where an object's state spans disagree with its log (§1, "an interval
/// the log does not reproduce"): a span not supplied by an import or a
/// migration opened by no event of its object, or holding another state
/// than the event that opened it, or the correction that last relabelled
/// it, entered; an event that changed the state opening no span; and other
/// than one span open, in the object's state.
fn state_spans_against(
    world: &objectflow_core::world::World<'_>,
    o: &objectflow_core::world::Object,
    events: Option<&Vec<objectflow_core::store::Event>>,
) -> Option<String> {
    let parts = world.parts(o).ok()?;
    let spans = parts.spans.get("state")?;
    let none = Vec::new();
    let events = events.unwrap_or(&none);
    let at = |p: i64| events.iter().find(|e| e.position == p);
    let name = |v: &Value| match v {
        Value::State { name, .. } => Some(name.clone()),
        _ => None,
    };
    for sp in spans {
        if sp.legacy || sp.migrated {
            continue;
        }
        let Some(e) = at(sp.entered_position) else {
            return Some(format!(
                "{}'s span in {:?} was opened by event {}, which is none of its own",
                o.id,
                name(&sp.value),
                sp.entered_position
            ));
        };
        let entered = sp
            .corrected_by
            .and_then(at)
            .map_or_else(|| e.to_state.clone(), |c| c.to_state.clone());
        if name(&sp.value).as_deref() != Some(entered.as_str()) {
            return Some(format!(
                "{}'s span opened by event {} holds {:?}, where the log entered {entered}",
                o.id,
                sp.entered_position,
                name(&sp.value)
            ));
        }
    }
    let opened: BTreeSet<i64> = spans.iter().map(|s| s.entered_position).collect();
    for e in events {
        let changed = e.changes_state || e.from_state.is_none();
        if changed && !e.imported && !opened.contains(&e.position) {
            return Some(format!(
                "{}'s event {} ({}, into {}) opened no span",
                o.id, e.position, e.transition, e.to_state
            ));
        }
    }
    let open: Vec<&objectflow_core::world::Span> =
        spans.iter().filter(|s| s.left_position.is_none()).collect();
    if open.len() != 1 || name(&open[0].value).as_deref() != Some(o.state.as_str()) {
        return Some(format!(
            "{} is in {}, and {} of its state spans are open, in {:?}",
            o.id,
            o.state,
            open.len(),
            open.iter().map(|s| name(&s.value)).collect::<Vec<_>>()
        ));
    }
    None
}

/// Where an object's tracked members' intervals disagree with its log (§1,
/// "an interval the log does not reproduce"), each member's value after
/// each event folded from what the events wrote (ADR-0209): a span not
/// supplied by an import or a migration opened by no event of its object,
/// or holding another value than the log gives it after that event, or
/// after the correction that last relabelled it; an event, other than an
/// import's or a correction's, that changed the member opening no span; and
/// an open span holding another value than the object's.
fn tracked_spans_against(
    world: &objectflow_core::world::World<'_>,
    o: &objectflow_core::world::Object,
    events: Option<&Vec<objectflow_core::store::Event>>,
) -> Option<String> {
    let t = world.lib.type_(&o.type_)?;
    let parts = world.parts(o).ok()?;
    let none = Vec::new();
    let events = events.unwrap_or(&none);
    for d in world.lib.tracked(t).into_iter().filter(|d| d != "state") {
        // the member's value after each event, and whether the event changed it
        let mut value = Json::Null;
        let mut after: BTreeMap<i64, (Json, bool)> = BTreeMap::new();
        for e in events {
            let was = value.clone();
            if let Some(v) = e.payload.get("wrote").and_then(|w| w.get(&d)) {
                value = v.clone();
            }
            after.insert(e.position, (value.clone(), value != was));
        }
        let spans = parts.spans.get(d.as_str()).cloned().unwrap_or_default();
        for sp in &spans {
            if sp.legacy || sp.migrated {
                continue;
            }
            let at = sp.corrected_by.unwrap_or(sp.entered_position);
            let Some((logged, _)) = after.get(&at) else {
                return Some(format!(
                    "{}'s span of {d} was opened by event {}, which is none of its own",
                    o.id, sp.entered_position
                ));
            };
            let held = objectflow_core::wire::encode(&sp.value);
            if held != *logged {
                return Some(format!(
                    "{}'s span of {d} opened by event {} holds {held}, where the log gives {logged}",
                    o.id, sp.entered_position
                ));
            }
        }
        let opened: BTreeSet<i64> = spans.iter().map(|s| s.entered_position).collect();
        for e in events {
            let changed = after.get(&e.position).is_some_and(|(_, c)| *c);
            if changed && !e.imported && e.source != "corrected" && !opened.contains(&e.position) {
                return Some(format!(
                    "{}'s event {} ({}) changed {d} and opened no span",
                    o.id, e.position, e.transition
                ));
            }
        }
        let open: Vec<_> = spans.iter().filter(|s| s.left_position.is_none()).collect();
        let now = objectflow_core::wire::encode(&o.attr(&d));
        if open.len() > 1
            || open
                .first()
                .is_some_and(|s| objectflow_core::wire::encode(&s.value) != now)
        {
            return Some(format!(
                "{}'s {d} is {now}, and {} of its spans are open, holding {:?}",
                o.id,
                open.len(),
                open.iter()
                    .map(|s| objectflow_core::wire::encode(&s.value))
                    .collect::<Vec<_>>()
            ));
        }
    }
    None
}

/// The states an object of a type may stand in: every one a transition of
/// its type reaches, and a superseded one.
fn reached(t: &Type) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = t
        .transitions
        .iter()
        .flat_map(|(_, x)| x.to.iter().cloned())
        .collect();
    for (n, s) in &t.states {
        if s.superseding {
            out.insert(n.clone());
        }
    }
    out
}

/// A day of the calendar from days since 1970-01-01, (year, month, day).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The fallible actor: what it sends next, from what the store holds now
/// (`adversarial-harness.md` §2).
struct Actor<'r> {
    rng: &'r mut Rng,
    made: usize,
    /// The evaluator functions the fixture's flows call, each answered first
    /// and changed now and then.
    calls: Vec<String>,
}

impl Actor<'_> {
    fn next(&mut self, run: &Run, history: &[Step]) -> Option<Step> {
        let world = run.store.snapshot().ok()?;
        let lib = world.lib;
        let unanswered = self
            .calls
            .iter()
            .find(|c| !run.store.answers.contains_key(*c))
            .cloned();
        if let Some(call) = unanswered.or_else(|| {
            self.rng
                .percent(3)
                .then(|| self.rng.pick(&self.calls).cloned())
                .flatten()
        }) {
            let said = ["satisfied", "satisfied", "unsatisfied", "unanswered"];
            let s = self.rng.pick(&said).copied().unwrap_or("satisfied");
            return Some(Step::Answer(call, s.to_string()));
        }
        if self.rng.percent(8) {
            let secs = [60, 3600, 86_400, 7 * 86_400, 40 * 86_400];
            return self.rng.pick(&secs).map(|s| Step::Advance(*s));
        }
        // the retrier: a keyed request sent again, as it was or with
        // another transition
        if self.rng.percent(10) {
            let keyed: Vec<&Request> = history
                .iter()
                .filter_map(|s| match s {
                    Step::Request(r) if r.idempotency_key.is_some() => Some(r.as_ref()),
                    _ => None,
                })
                .collect();
            if let Some(r) = self.rng.pick(&keyed) {
                let mut again = (*r).clone();
                if self.rng.percent(30) {
                    again.transition = format!("{}_other", again.transition);
                }
                return Some(Step::Request(Box::new(again)));
            }
        }
        let objects = world.in_order().ok()?;
        let actors: Vec<String> = objects
            .iter()
            .filter_map(|o| {
                let t = lib.type_(&o.type_)?;
                let (attr, _) = t.actor_identity()?;
                match o.attr(attr) {
                    Value::Text(s) => Some(s),
                    _ => None,
                }
            })
            .collect();
        let actor = if actors.is_empty() || self.rng.percent(60) {
            OPERATOR.to_string()
        } else {
            self.rng
                .pick(&actors)
                .cloned()
                .unwrap_or_else(|| OPERATOR.to_string())
        };
        // the eraser: an object that was given personal values, erased, so
        // erasure is exercised wherever the fixture has personal data
        if self.rng.percent(6) {
            let held: Vec<(&String, &Type, &String)> = run
                .markers
                .keys()
                .filter_map(|id| {
                    let o = world.get(id).ok().flatten()?;
                    let t = lib.type_(&o.type_)?;
                    let (xn, _) = t
                        .transitions
                        .iter()
                        .find(|(_, x)| x.kind == Kind::Erasure)?;
                    Some((id, t, xn))
                })
                .collect();
            if let Some((id, t, xn)) = self.rng.pick(&held) {
                let mut inputs = BTreeMap::new();
                if let Some((_, x)) = t.transitions.iter().find(|(n, _)| n == *xn) {
                    for (n, i) in &x.inputs {
                        inputs.insert(
                            n.clone(),
                            self.value(&world, &i.declared, i.many, i.personal),
                        );
                    }
                }
                return Some(Step::Request(Box::new(Request {
                    transition: (*xn).clone(),
                    actor: OPERATOR.to_string(),
                    object_id: Some((*id).clone()),
                    type_: None,
                    inputs,
                    occurred_at: None,
                    propose: false,
                    expires_at: None,
                    expected_version: None,
                    target_version: None,
                    principal: None,
                    idempotency_key: None,
                    context: None,
                })));
            }
        }
        let creatable: Vec<&Type> = lib
            .types
            .values()
            .filter(|t| {
                !t.abstract_
                    && !t.mirror
                    && t.transitions
                        .iter()
                        .any(|(_, x)| x.kind == Kind::Initial && x.only_via.is_empty())
            })
            .collect();
        let live: Vec<&&objectflow_core::world::Object> = objects
            .iter()
            .filter(|o| lib.type_(&o.type_).is_some_and(|t| !t.mirror))
            .collect();
        let create = live.is_empty() || self.rng.percent(30);
        let (t, object_id) = if create {
            (*self.rng.pick(&creatable)?, None)
        } else {
            let o = self.rng.pick(&live)?;
            (lib.type_(&o.type_)?, Some(o.id.clone()))
        };
        let choices: Vec<&(String, objectflow_core::model::Transition)> = t
            .transitions
            .iter()
            .filter(|(_, x)| {
                (x.kind == Kind::Initial) == create && (!create || x.only_via.is_empty())
            })
            .collect();
        let (xn, x) = self.rng.pick(&choices)?;
        let mut inputs: BTreeMap<String, Json> = BTreeMap::new();
        for n in &x.required_inputs {
            if self.rng.percent(3) {
                continue;
            }
            if let Some(a) = t.attribute(n) {
                inputs.insert(
                    n.clone(),
                    self.value(&world, &a.declared, a.many, a.personal),
                );
            }
        }
        for n in &x.optional_inputs {
            if self.rng.percent(50)
                && let Some(a) = t.attribute(n)
            {
                inputs.insert(
                    n.clone(),
                    self.value(&world, &a.declared, a.many, a.personal),
                );
            }
        }
        for (n, i) in &x.inputs {
            if (!i.optional || self.rng.percent(50)) && !self.rng.percent(3) {
                inputs.insert(
                    n.clone(),
                    self.value(&world, &i.declared, i.many, i.personal),
                );
            }
        }
        let version = object_id
            .as_deref()
            .and_then(|id| world.version_of(id).ok());
        let r = Request {
            transition: xn.clone(),
            actor,
            object_id: object_id.clone(),
            type_: if create { Some(t.name.clone()) } else { None },
            inputs,
            // the backdater, past any bound
            occurred_at: self.rng.percent(5).then(|| run.store.now - 400 * 86_400),
            propose: x.proposable && self.rng.percent(10),
            expires_at: None,
            // the skipper, acting on a version the world has moved past
            expected_version: version.filter(|_| self.rng.percent(10)).map(|v| v - 1),
            target_version: None,
            principal: None,
            idempotency_key: self.rng.percent(15).then(|| {
                self.made += 1;
                format!("k{}", self.made)
            }),
            context: None,
        };
        Some(Step::Request(Box::new(r)))
    }

    /// A value of a declared type as the wire carries it; now and then one
    /// of the wrong type, as a guesser sends.
    fn value(
        &mut self,
        world: &objectflow_core::world::World<'_>,
        d: &Declared,
        many: bool,
        personal: bool,
    ) -> Json {
        if many {
            let one = self.value(world, d, false, personal);
            return if one.is_null() {
                json!([])
            } else {
                json!([one])
            };
        }
        if self.rng.percent(4) {
            return if self.rng.percent(50) {
                json!(42)
            } else {
                json!("x")
            };
        }
        self.made += 1;
        let n = self.made;
        match d {
            Declared::Reference(target) => {
                let ids: Vec<String> = world
                    .of_type(target.trim_end_matches("[]"))
                    .map(|os| os.iter().map(|o| o.id.clone()).collect())
                    .unwrap_or_default();
                self.rng.pick(&ids).map_or(Json::Null, |id| json!(id))
            }
            Declared::Scalar(t) => match t.as_str() {
                // closed by a letter, so one marker is never inside another
                "string" | "text" if personal => json!(format!("pm{n}z")),
                "string" | "text" => json!(format!("text {n}")),
                "identity" => json!(format!("u.{n}")),
                "int" => json!(i64::try_from(self.rng.below(200)).unwrap_or(0) - 20),
                "boolean" | "bool" => json!(self.rng.percent(50)),
                "duration" => {
                    let ds = ["30 min", "2 h", "1 day", "3 days", "1 week"];
                    json!(self.rng.pick(&ds).copied().unwrap_or("1 day"))
                }
                "date" => {
                    let days = START / 86_400 + i64::try_from(self.rng.below(400)).unwrap_or(0);
                    let (y, m, dd) = civil(days);
                    json!(format!("{y:04}-{m:02}-{dd:02}"))
                }
                "timestamp" => {
                    let days = START / 86_400 + i64::try_from(self.rng.below(400)).unwrap_or(0);
                    let (y, m, dd) = civil(days);
                    json!(format!("{y:04}-{m:02}-{dd:02}T10:00:00Z"))
                }
                s if s.starts_with("decimal(") => {
                    let scale = s
                        .trim_start_matches("decimal(")
                        .trim_end_matches(')')
                        .split(',')
                        .nth(1)
                        .and_then(|x| x.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    let whole = self.rng.below(1000);
                    if scale == 0 {
                        json!(whole.to_string())
                    } else {
                        json!(format!("{whole}.{}", "5".repeat(scale)))
                    }
                }
                s if s.starts_with("money(") => {
                    let ccy = s.trim_start_matches("money(").trim_end_matches(')');
                    json!(format!("{ccy} {}.25", self.rng.below(5000)))
                }
                "file" => Json::Null,
                other => match world.lib.enumerations.get(other) {
                    Some(members) => self.rng.pick(members).map_or(Json::Null, |m| json!(m)),
                    None => json!(format!("text {n}")),
                },
            },
        }
    }
}

/// Each evaluator function a fixture's flows call, `<evaluator>.<function>`,
/// read from their text.
fn calls(fx: &Fixture, evaluators: &[String]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for (_, text) in &fx.modules {
        for e in evaluators {
            let at = format!("{e}.");
            let mut rest = text.as_str();
            while let Some(i) = rest.find(&at) {
                let after = &rest[i + at.len()..];
                let name: String = after
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                let preceded = rest[..i]
                    .chars()
                    .last()
                    .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'));
                if preceded && !name.is_empty() && after[name.len()..].starts_with('(') {
                    out.insert(format!("{e}.{name}"));
                }
                rest = &rest[i + at.len()..];
            }
        }
    }
    out.into_iter().collect()
}

/// A run of a fixture from a seed, at most `steps` long, stopping at its
/// first failure.
pub fn run(fx: &Fixture, seed: u64, steps: usize) -> Result<Outcome, String> {
    let mut state = Run::new(fx)?;
    let mut rng = Rng::new(seed);
    let mut actor = Actor {
        rng: &mut rng,
        made: 0,
        calls: calls(fx, &state.store.lib.evaluators),
    };
    let mut transcript = Vec::new();
    for at in 0..steps {
        let Some(step) = actor.next(&state, &transcript) else {
            break;
        };
        let failure = state.take(&step, at);
        transcript.push(step);
        if let Some(f) = failure {
            return Ok(Outcome {
                steps: transcript,
                failure: Some(f),
                counts: state.counts,
            });
        }
    }
    Ok(Outcome {
        steps: transcript,
        failure: None,
        counts: state.counts,
    })
}

/// A transcript replayed, and its first failure.
pub fn replay(fx: &Fixture, steps: &[Step]) -> Result<Option<Failure>, String> {
    let mut state = Run::new(fx)?;
    for (at, step) in steps.iter().enumerate() {
        if let Some(f) = state.take(step, at) {
            return Ok(Some(f));
        }
    }
    Ok(None)
}

/// The shortest transcript found that still fails as the run did: its
/// prefix to the failure, then each step left out that the failure does
/// not need (`adversarial-harness.md` §4).
pub fn reduce(fx: &Fixture, steps: &[Step], failure: &Failure) -> Result<Vec<Step>, String> {
    let mut kept: Vec<Step> = steps.iter().take(failure.at + 1).cloned().collect();
    let fails = |s: &[Step]| -> Result<bool, String> {
        Ok(replay(fx, s)?.is_some_and(|f| f.condition == failure.condition))
    };
    let mut changed = true;
    while changed {
        changed = false;
        let mut i = 0;
        while i < kept.len() {
            let mut fewer = kept.clone();
            fewer.remove(i);
            if fails(&fewer)? {
                kept = fewer;
                changed = true;
            } else {
                i += 1;
            }
        }
    }
    Ok(kept)
}

//! Step 6 of the flow checker: each example of an examples file run by the
//! core against the version under publish (ADR-0156 decision 3,
//! `flow-format.md` §11.5).
//!
//! Each example runs in an empty in-memory store holding version 0 and the
//! modules checked, its clock starting where every example's does, and is
//! played as the Flow Studio's replay plays one through the wire
//! (`apps/flow-studio/server/tests/replay.py`): each step a request, a
//! decision on a proposal or an import, and each expectation matched against
//! the verdict, the object and the events, as `flow-format.md` §11.4 states.
//! This is a port of that matcher, the wire's calls replaced by the core's
//! own: an outcome is `passed`, `failed` or `not run`, the last where the
//! core reaches what it does not yet run.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value as Json, json};

use super::doc::{list, obj, py_str, re, str_or};
use super::logic::Rat;
use crate::import::{ImportBatch, ImportedObject, import_batch};
use crate::model::{Library, OPERATOR};
use crate::reads;
use crate::request::{Request, request};
use crate::store::{Event, Store};
use crate::value::{parse_duration, parse_timestamp, timestamp_text};
use crate::verdict::Verdict;
use crate::wire::encode;

/// Where every example's clock starts (`flow-format.md` §11.3).
pub const CLOCK_START: &str = "2026-01-01T00:00:00Z";

const VERBS: &[&str] = &["request", "propose", "approve", "reject", "withdraw"];

/// The members every object has, and their kinds (`flow-format.md` §5).
const BUILT_IN: &[(&str, &str)] = &[
    ("id", "text"),
    ("open", "bool"),
    ("created_at", "timestamp"),
    ("created_by_kind", "text"),
    ("recorded_from", "timestamp"),
    ("declaration_version", "int"),
];

static INSTANT: LazyLock<Regex> = LazyLock::new(|| re(r"^now(?: ([+-]) ([0-9]+) (\w+))?$"));

/// How an example's run ends: an outcome, `passed` or `failed`, or `not
/// run`, with why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed(String),
    NotRun(String),
}

/// Why a run stopped before its expectation was matched.
enum Stop {
    Failed(String),
    NotRun(String),
}

type Played<T> = Result<T, Stop>;

fn failed<T>(why: impl Into<String>) -> Played<T> {
    Err(Stop::Failed(why.into()))
}

impl From<crate::store::StoreError> for Stop {
    fn from(e: crate::store::StoreError) -> Stop {
        Stop::Failed(format!("the store failed: {e}"))
    }
}

impl From<crate::eval::NotRun> for Stop {
    fn from(e: crate::eval::NotRun) -> Stop {
        Stop::NotRun(e.0)
    }
}

/// One example's run: the store it runs in, and the aliases it has bound.
struct Player {
    store: Store,
    aliases: BTreeMap<String, String>,
    types: BTreeMap<String, String>,
}

impl Player {
    // ── reading ──────────────────────────────────────────────────────────
    /// A type's declaration as the module gives it.
    fn decl(&self, tn: &str) -> Played<Json> {
        match self.store.lib.type_(tn) {
            Some(t) => Ok(t.source.clone()),
            None => failed(format!("no type {tn} is declared")),
        }
    }

    fn type_of(&mut self, oid: &str) -> Played<String> {
        if let Some(t) = self.types.get(oid) {
            return Ok(t.clone());
        }
        let world = self.store.snapshot()?;
        let Some(o) = world.get(oid) else {
            return failed(format!("no object {oid}"));
        };
        let t = o.type_.clone();
        drop(world);
        self.types.insert(oid.to_string(), t.clone());
        Ok(t)
    }

    fn alias(&self, a: &str) -> Played<String> {
        match self.aliases.get(a) {
            Some(id) => Ok(id.clone()),
            None => failed(format!("no object is named {a}")),
        }
    }

    /// An object as the wire's `get` reports it: its state, its attributes
    /// and derived attributes encoded as the wire carries them, and the
    /// members every object has (the service's `object_json`).
    fn object(&self, id: &str) -> Played<Json> {
        let world = self.store.snapshot()?;
        let Some(view) = reads::get(&world, id)? else {
            return failed(format!("no object {id}"));
        };
        let o = &view.object;
        let Some(t) = self.store.lib.type_(&o.type_) else {
            return failed(format!("no type {} is declared", o.type_));
        };
        let declaration_version = self
            .store
            .history(&o.id)?
            .last()
            .map_or(self.store.lib.version, |e| e.declaration_version);
        let mut attributes = serde_json::Map::new();
        for (n, _) in &t.attributes {
            attributes.insert(n.clone(), encode(&o.attr(n)));
        }
        for (n, v) in &view.derived {
            attributes.insert(n.clone(), encode(v));
        }
        Ok(json!({
            "id": o.id, "type": o.type_, "state": o.state, "declaration_version": declaration_version,
            "created_at": timestamp_text(o.created_at), "created_by_kind": o.created_by_kind,
            "attributes": attributes, "recorded_from": timestamp_text(o.recorded_from), "open": view.open,
        }))
    }

    // ── an example's value as the wire carries it ────────────────────────
    fn instant(&self, v: &Json) -> Played<String> {
        let Some(text) = v.as_str() else {
            return failed(format!("{} is not a time", py_str(v)));
        };
        let Some(m) = INSTANT.captures(text) else {
            return match parse_timestamp(text) {
                Some(t) => Ok(timestamp_text(t)),
                None => failed(format!("{text} is not a time")),
            };
        };
        let shift = match (m.get(1), m.get(2), m.get(3)) {
            (Some(sign), Some(n), Some(unit)) => {
                let Some(s) = parse_duration(&format!("{} {}", n.as_str(), unit.as_str())) else {
                    return failed(format!("{text} is not a time"));
                };
                if sign.as_str() == "-" { -s } else { s }
            }
            _ => 0,
        };
        Ok(timestamp_text(self.store.now + shift))
    }

    fn wire_value(&self, v: &Json, spec: Option<&Json>) -> Played<Json> {
        let spec = spec.cloned().unwrap_or(Json::Null);
        if v.is_null() {
            return Ok(Json::Null);
        }
        if let Some(r) = spec.get("reference") {
            if py_str(r).ends_with("[]") {
                let items = v.as_array().map(Vec::as_slice).unwrap_or_default();
                return items
                    .iter()
                    .map(|a| self.alias(&py_str(a)).map(Json::from))
                    .collect::<Played<Vec<_>>>()
                    .map(Json::Array);
            }
            return self.alias(&py_str(v)).map(Json::from);
        }
        let typ = spec.get("type").map(py_str).unwrap_or_default();
        if let Some(item) = typ.strip_suffix("[]") {
            let mut one = spec.clone();
            if let Some(o) = one.as_object_mut() {
                o.insert("type".into(), json!(item));
            }
            let items = v.as_array().map(Vec::as_slice).unwrap_or_default();
            return items
                .iter()
                .map(|x| self.wire_value(x, Some(&one)))
                .collect::<Played<Vec<_>>>()
                .map(Json::Array);
        }
        if typ == "timestamp" {
            return self.instant(v).map(Json::from);
        }
        // a decimal is a string on the wire
        if typ.starts_with("decimal") && v.is_number() {
            return Ok(json!(py_str(v)));
        }
        // an enumeration member, written qualified
        if let Some(s) = v.as_str()
            && typ.starts_with(|c: char| c.is_ascii_uppercase())
            && let Some((_, member)) = s.split_once('.')
        {
            return Ok(json!(member));
        }
        Ok(v.clone())
    }

    // ── a step ───────────────────────────────────────────────────────────
    fn advance(&mut self, after: Option<&Json>) -> Played<()> {
        let Some(after) = after.filter(|a| !a.is_null()) else {
            return Ok(());
        };
        match parse_duration(&py_str(after)) {
            Some(s) if s >= 0 => {
                self.store.now += s;
                Ok(())
            }
            _ => failed(format!(
                "advance takes a duration such as '1 h': {}",
                py_str(after)
            )),
        }
    }

    /// The type a family's legacy keys are qualified by: its farthest base,
    /// or itself (ADR-0154, C23).
    fn root(&self, tn: &str) -> Played<String> {
        let mut tn = tn.to_string();
        let mut seen = std::collections::BTreeSet::new();
        while seen.insert(tn.clone()) {
            match self.decl(&tn)?.get("extends").filter(|e| !e.is_null()) {
                Some(base) => tn = py_str(base),
                None => break,
            }
        }
        Ok(tn)
    }

    /// An import step: a one-object batch, its legacy key the step's alias,
    /// admitting nothing; the object is then found by its legacy key
    /// (ADR-0154).
    fn imported(&mut self, im: &Json) -> Played<()> {
        self.advance(im.get("after"))?;
        let (tn, key) = (str_or(im, "type", ""), str_or(im, "as", ""));
        let specs = obj(&self.decl(&tn)?, "attributes").clone();
        let mut attributes = BTreeMap::new();
        for (k, v) in obj(im, "values") {
            let spec = specs.get(k);
            // a reference names another imported object by its legacy key,
            // which is its alias
            let value = if spec.is_some_and(|s| s.get("reference").is_some()) {
                v.clone()
            } else {
                self.wire_value(v, spec)?
            };
            attributes.insert(k.clone(), value);
        }
        let batch = ImportBatch {
            objects: vec![ImportedObject {
                type_: tn.clone(),
                legacy_key: key.clone(),
                state: str_or(im, "state", ""),
                attributes,
                ..ImportedObject::default()
            }],
            sequences: Vec::new(),
        };
        let report = import_batch(&mut self.store, OPERATOR, &batch, &[], false)??;
        if let Some(r) = report.refusal {
            return failed(format!("the import of {key} is refused: {}", r.reason));
        }
        let root = self.root(&tn)?;
        let world = self.store.snapshot()?;
        let Some(id) = world
            .external
            .get(&("legacy".to_string(), format!("{root}:{key}")))
            .cloned()
        else {
            return failed(format!("the import of {key} wrote nothing lookup finds"));
        };
        let t = world.get(&id).map(|o| o.type_.clone()).unwrap_or(tn);
        drop(world);
        self.aliases.insert(key, id.clone());
        self.types.insert(id, t);
        Ok(())
    }

    fn play(&mut self, st: &Json) -> Played<Option<Verdict>> {
        if let Some(im) = st.get("import") {
            self.imported(im)?;
            return Ok(None);
        }
        let Some(verb) = VERBS.iter().copied().find(|v| st.get(*v).is_some()) else {
            return failed("a step is a request, a decision on a proposal or an import");
        };
        let r = &st[verb];
        self.advance(r.get("after"))?;
        let actor = self.alias(&str_or(r, "actor", OPERATOR))?;
        let mut req = Request {
            transition: verb.to_string(),
            actor,
            object_id: None,
            type_: None,
            inputs: BTreeMap::new(),
            occurred_at: None,
            propose: verb == "propose",
            expires_at: None,
            expected_version: None,
            principal: None,
        };
        if matches!(verb, "approve" | "reject" | "withdraw") {
            req.object_id = Some(self.alias(&str_or(r, "proposal", ""))?);
            return Ok(Some(request(&mut self.store, &req)??));
        }
        req.transition = str_or(r, "transition", "");
        let tn = if let Some(t) = r.get("type") {
            req.type_ = Some(py_str(t));
            py_str(t)
        } else {
            let id = self.alias(&str_or(r, "object", ""))?;
            req.object_id = Some(id.clone());
            self.type_of(&id)?
        };
        let d = self.decl(&tn)?;
        let x = obj(&d, "transitions")
            .get(&req.transition)
            .cloned()
            .unwrap_or(Json::Null);
        let mut specs = obj(&d, "attributes").clone();
        specs.extend(obj(&x, "inputs").clone());
        for (k, v) in obj(r, "inputs") {
            req.inputs
                .insert(k.clone(), self.wire_value(v, specs.get(k))?);
        }
        if let Some(at) = r.get("occurred_at").filter(|a| super::doc::truthy(Some(a))) {
            let text = self.instant(at)?;
            req.occurred_at = parse_timestamp(&text);
        }
        let v = request(&mut self.store, &req)??;
        if let Verdict::Satisfied { object_id, .. } = &v {
            if verb == "propose" {
                self.aliases
                    .insert(str_or(r, "proposal", ""), object_id.clone());
            } else if let Some(alias) = r.get("as") {
                self.aliases.insert(py_str(alias), object_id.clone());
                self.types.insert(object_id.clone(), tn.clone());
            }
            let names: Vec<String> = list(r, "creates").iter().map(py_str).collect();
            if !names.is_empty() {
                self.name_created(&names, &v)?;
            }
        }
        Ok(Some(v))
    }

    /// The events a satisfied request appended, read by position (ADR-0151).
    fn appended(&self, v: &Verdict) -> Played<Vec<Option<Event>>> {
        match v {
            Verdict::Satisfied { events, .. } => Ok(self.store.events(events)?),
            _ => Ok(Vec::new()),
        }
    }

    /// Alias the objects the request created, other than its own, in the
    /// order it created them: its creation events after the first
    /// (`flow-format.md` §11.3).
    fn name_created(&mut self, names: &[String], v: &Verdict) -> Played<()> {
        let made: Vec<String> = self
            .appended(v)?
            .into_iter()
            .skip(1)
            .flatten()
            .filter(|e| e.from_state.is_none())
            .map(|e| e.object_id)
            .collect();
        if names.len() > made.len() {
            return failed(format!(
                "a step names {} objects its request creates, and it creates {}",
                names.len(),
                made.len()
            ));
        }
        let mut kinds = std::collections::BTreeSet::new();
        for m in &made[..names.len()] {
            kinds.insert(self.type_of(m)?);
        }
        if kinds.len() > 1 {
            return failed(
                "a step names objects its request creates, and they are of more than one type",
            );
        }
        for (alias, oid) in names.iter().zip(made) {
            self.aliases.insert(alias.clone(), oid);
        }
        Ok(())
    }

    /// The transitions a request caused, in the order each began: its events
    /// after the first, a redaction left out (`flow-format.md` §11.4).
    fn caused(&mut self, v: &Verdict) -> Played<Vec<String>> {
        // a proposal's own events, an approval's and an invalidation's, are
        // not transitions the request took (flow-format.md §11.4)
        let mut taken = Vec::new();
        for e in self.appended(v)?.into_iter().flatten() {
            let t = self.type_of(&e.object_id)?;
            if t != "Proposal" {
                taken.push((t, e.transition));
            }
        }
        Ok(taken
            .into_iter()
            .skip(1)
            .filter(|(_, x)| x != "redact")
            .map(|(t, x)| format!("{t}.{x}"))
            .collect())
    }

    // ── an expectation ───────────────────────────────────────────────────
    fn requested_type(&mut self, tested: &Json) -> Played<Option<String>> {
        let Some(verb) = VERBS.iter().copied().find(|v| tested.get(*v).is_some()) else {
            return Ok(None);
        };
        let r = &tested[verb];
        if verb == "approve" {
            let proposal = self.object(&self.alias(&str_or(r, "proposal", ""))?)?;
            return match proposal["attributes"].get("target").and_then(Json::as_str) {
                Some(target) => Ok(Some(self.type_of(target)?)),
                None => Ok(None),
            };
        }
        if let Some(t) = r.get("type") {
            return Ok(Some(py_str(t)));
        }
        let id = self.alias(&str_or(r, "object", ""))?;
        Ok(Some(self.type_of(&id)?))
    }

    #[allow(clippy::too_many_lines)]
    fn compare(&mut self, want: &Json, v: &Verdict, tested: &Json) -> Played<Option<String>> {
        let wanted = str_or(want, "verdict", "");
        let text = |r: Option<crate::model::Remedy>| {
            r.map_or("None".to_string(), |r| r.text().to_string())
        };
        let Verdict::Satisfied {
            object_id,
            events,
            flags,
            ..
        } = v
        else {
            let got = v.kind();
            if wanted == "applied" {
                return Ok(Some(format!(
                    "expected it to apply, and it is refused as {got}"
                )));
            }
            if got != wanted {
                return Ok(Some(format!("expected {wanted}, and it is {got}")));
            }
            let (clause, remedy) = match v {
                Verdict::Unsatisfied { clause, remedy } => (Some(clause.clone()), text(*remedy)),
                Verdict::InvariantViolated { failures, remedy } => {
                    let Some(first) = failures.first() else {
                        return Ok(Some(
                            "an invariant fails, and the verdict names none".to_string(),
                        ));
                    };
                    let owner = first.type_.clone();
                    let clause = if Some(&owner) == self.requested_type(tested)?.as_ref() {
                        first.invariant.clone()
                    } else {
                        format!("{owner}.{}", first.invariant)
                    };
                    (Some(clause), text(*remedy))
                }
                Verdict::CallRefused { clause, .. } => (clause.clone(), text(v.remedy())),
                _ => (None, text(v.remedy())),
            };
            for (k, have) in [
                ("clause", clause.unwrap_or_else(|| "None".to_string())),
                ("remedy", remedy),
            ] {
                if let Some(w) = want.get(k).map(py_str)
                    && have != w
                {
                    return Ok(Some(format!("expected the {k} {w}, and it is {have}")));
                }
            }
            return Ok(None);
        };
        if wanted != "applied" {
            return Ok(Some(format!("expected {wanted}, and the request applies")));
        }
        let o = self.object(object_id)?;
        if let Some(state) = want.get("state").map(py_str)
            && o["state"].as_str() != Some(state.as_str())
        {
            return Ok(Some(format!(
                "expected the state {state}, and it is {}",
                py_str(&o["state"])
            )));
        }
        for (k, expected) in obj(want, "values") {
            if let Some(problem) = self.value_differs(&o, k, expected)? {
                return Ok(Some(problem));
            }
        }
        if let Some(at) = want.get("occurred_at") {
            let first = events.first().copied();
            let mine = self
                .store
                .history(object_id)?
                .into_iter()
                .find(|e| Some(e.position) == first);
            let expected = self.instant(at)?;
            if mine.and_then(|e| e.occurred_at).map(timestamp_text) != Some(expected) {
                return Ok(Some("the occurred time differs".to_string()));
            }
        }
        if let Some(cascaded) = want.get("cascaded") {
            let expected: Vec<String> = cascaded
                .as_array()
                .map(|a| a.iter().map(py_str).collect())
                .unwrap_or_default();
            let caused = self.caused(v)?;
            if expected != caused {
                return Ok(Some(format!(
                    "expected the request to cause {expected:?}, and it causes {caused:?}"
                )));
            }
        }
        if let Some(warned) = want.get("warned") {
            let expected: Vec<String> = warned
                .as_array()
                .map(|a| a.iter().map(py_str).collect())
                .unwrap_or_default();
            let got: Vec<String> = flags.iter().map(|f| f.clause.clone()).collect();
            if expected != got {
                return Ok(Some(format!(
                    "expected warnings {expected:?}, and they are {got:?}"
                )));
            }
        }
        Ok(None)
    }

    fn value_differs(&self, o: &Json, k: &str, expected: &Json) -> Played<Option<String>> {
        let tn = py_str(&o["type"]);
        let d = self.decl(&tn)?;
        let mut spec = obj(&d, "attributes").get(k).cloned();
        let got = if let Some(g) = o["attributes"].get(k) {
            g.clone()
        } else if let Some((_, kind)) = BUILT_IN.iter().find(|(n, _)| *n == k) {
            spec = Some(json!({ "type": kind }));
            o.get(k).cloned().unwrap_or(Json::Null)
        } else {
            return Ok(Some(format!("{tn} has no member {k} the wire reports")));
        };
        let same = match &spec {
            None => loose_equal(expected, &got), // a derived attribute
            Some(spec) => self.same(expected, &got, spec)?,
        };
        Ok((!same).then(|| {
            format!(
                "expected {k} to hold {}, and it holds {}",
                py_str(expected),
                py_str(&got)
            )
        }))
    }

    fn same(&self, expected: &Json, got: &Json, spec: &Json) -> Played<bool> {
        if expected.is_null() || got.is_null() {
            return Ok(expected.is_null() && got.is_null());
        }
        let typ = spec.get("type").map(py_str).unwrap_or_default();
        if spec.get("reference").is_some() || typ.ends_with("[]") {
            return Ok(self.wire_value(expected, Some(spec))? == *got);
        }
        if typ == "timestamp" {
            let at = parse_timestamp(&self.instant(expected)?);
            return Ok(at.is_some() && at == got.as_str().and_then(parse_timestamp));
        }
        if typ == "duration" {
            let d = parse_duration(&py_str(expected));
            return Ok(d.is_some() && d == got.as_str().and_then(parse_duration));
        }
        if typ.starts_with("decimal") || typ.starts_with("money") {
            let amount = |v: &Json| py_str(v).split_whitespace().last().and_then(Rat::parse);
            let a = amount(expected);
            return Ok(a.is_some() && a == amount(got));
        }
        if typ == "date" {
            return Ok(Some(py_str(expected).as_str()) == got.as_str());
        }
        Ok(self.wire_value(expected, Some(spec))? == *got)
    }
}

/// A derived attribute's value, which no declaration types: compared as the
/// reference runner compares it, a number exactly.
fn loose_equal(expected: &Json, got: &Json) -> bool {
    if expected.is_boolean() || got.is_boolean() {
        return expected == got;
    }
    if expected.is_number() && (got.is_i64() || got.is_u64() || got.is_string()) {
        let (a, b) = (Rat::parse(&py_str(expected)), Rat::parse(&py_str(got)));
        return a.is_some() && a == b;
    }
    expected == got
}

/// The setups an example or a setup is given, the farthest first.
fn chain<'a>(
    setups: &'a serde_json::Map<String, Json>,
    name: &str,
    seen: &mut Vec<String>,
) -> Vec<&'a Json> {
    let Some(s) = setups.get(name) else {
        return Vec::new();
    };
    if seen.iter().any(|n| n == name) {
        return Vec::new();
    }
    seen.push(name.to_string());
    let mut out = match s.get("given") {
        Some(g) => chain(setups, &py_str(g), seen),
        None => Vec::new(),
    };
    out.push(s);
    out
}

fn run_one(
    lib: &Library,
    now: i64,
    setups: &serde_json::Map<String, Json>,
    ex: &Json,
) -> Played<Option<String>> {
    let store = Store::sandbox(lib.clone(), now)?;
    let mut p = Player {
        store,
        aliases: BTreeMap::from([(OPERATOR.to_string(), OPERATOR.to_string())]),
        types: BTreeMap::new(),
    };
    let mut parts = match ex.get("given") {
        Some(g) => chain(setups, &py_str(g), &mut Vec::new()),
        None => Vec::new(),
    };
    parts.push(ex);
    let mut answers = BTreeMap::new();
    for part in parts {
        // the store asks its evaluators through these answers (PRD F13)
        for (k, a) in obj(part, "evaluators") {
            if let Some(a) = a.as_str() {
                answers.insert(k.clone(), a.to_string());
            }
        }
        p.store.answers = answers.clone();
        for (i, st) in list(part, "steps").iter().enumerate() {
            if let Some(v) = p.play(st)?
                && !matches!(v, Verdict::Satisfied { .. })
            {
                return failed(format!("step {} is refused as {}", i + 1, v.kind()));
            }
        }
    }
    let tested = match ex.get("approve") {
        Some(a) => json!({ "approve": a }),
        None => json!({ "request": ex.get("request").cloned().unwrap_or(Json::Null) }),
    };
    let Some(v) = p.play(&tested)? else {
        return failed("the example requests nothing");
    };
    let want = ex.get("expect").cloned().unwrap_or(Json::Null);
    let mut problem = p.compare(&want, &v, &tested)?;
    if problem.is_none()
        && let Some(expected) = want.get("proposal").map(py_str)
    {
        let id = p.alias(&str_or(&ex["approve"], "proposal", ""))?;
        let state = py_str(&p.object(&id)?["state"]);
        // the format writes the state in lower case (§11.4, D521)
        if state.to_lowercase() != expected {
            problem = Some(format!(
                "expected the proposal {expected}, and it is {state}"
            ));
        }
    }
    Ok(problem)
}

/// Each example of an examples file, in the order written, with its outcome,
/// run against the library the modules checked load into.
pub fn run_examples(lib: &Library, doc: &Json) -> Vec<(String, Outcome)> {
    let now = parse_timestamp(CLOCK_START).unwrap_or(0);
    let setups = obj(doc, "setups").clone();
    obj(doc, "examples")
        .iter()
        .map(|(name, ex)| {
            let outcome = match run_one(lib, now, &setups, ex) {
                Ok(None) => Outcome::Passed,
                Ok(Some(problem)) | Err(Stop::Failed(problem)) => Outcome::Failed(problem),
                Err(Stop::NotRun(what)) => Outcome::NotRun(what),
            };
            (name.clone(), outcome)
        })
        .collect()
}

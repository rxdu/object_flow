//! ObjectFlow's HTTP service (ADR-0150): one `POST /v1/<operation>` per library
//! operation, and a sandbox's controls under `/sandbox/`, which a deployment's
//! service would not route. The core is synchronous; this is its one
//! asynchronous layer (ADR-0132), and every operation runs the core under one
//! lock, since a sandbox serves one caller.
//!
//! A refusal is a `200` carrying the verdict, its shape named by `kind`; a
//! fault is a `4xx` or `5xx` carrying `fault`; a construct the core's first
//! slice does not run is a `501` naming it (ADR-0153).
//!
//! Run as `objectflow-service --port <n>`; `--port 0` takes a free port, and
//! the address listened on is written to standard error.

use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use objectflow_core::eval::NotRun;
use objectflow_core::import::{
    self, Disposition, EnteredAt, ImportBatch, ImportReport, ImportedObject, LegacyInterval, Raise,
    ViolationClass, ViolationKind,
};
use objectflow_core::model::{Declared, Library};
use objectflow_core::reads::{self, Offer};
use objectflow_core::request::{self, Request};
use objectflow_core::store::{Event, Store};
use objectflow_core::value::{parse_duration, parse_timestamp, timestamp_text};
use objectflow_core::verdict::{Flag, Verdict};
use objectflow_core::wire::encode;
use serde_json::{Map, Value as J, json};

/// Where an examples file's clock starts (`flow-format.md` §11.3).
const CLOCK_START: &str = "2026-01-01T00:00:00Z";

/// A sandbox: the modules loaded, the store they run in, and the answers its
/// evaluators give (PRD F13).
#[derive(Default)]
struct Sandbox {
    files: Vec<(String, String)>,
    store: Option<Store>,
    answers: Map<String, J>,
}

type Shared = Arc<Mutex<Sandbox>>;

/// What the service answers a request it cannot: the wire's 4xx or 5xx.
enum Fault {
    Bad(String),
    Missing(String),
    Conflict(String),
    NotRun(String),
    Internal(String),
}

impl IntoResponse for Fault {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Fault::Bad(m) => (StatusCode::BAD_REQUEST, m),
            Fault::Missing(m) => (StatusCode::NOT_FOUND, m),
            Fault::Conflict(m) => (StatusCode::CONFLICT, m),
            Fault::NotRun(m) => (StatusCode::NOT_IMPLEMENTED, format!("not run: {m}")),
            Fault::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        (status, Json(json!({ "fault": message }))).into_response()
    }
}

impl From<NotRun> for Fault {
    fn from(e: NotRun) -> Fault {
        Fault::NotRun(e.0)
    }
}

impl From<objectflow_core::store::StoreError> for Fault {
    fn from(e: objectflow_core::store::StoreError) -> Fault {
        Fault::Internal(e.to_string())
    }
}

type Answer = Result<Json<J>, Fault>;

fn locked(shared: &Shared) -> Result<std::sync::MutexGuard<'_, Sandbox>, Fault> {
    shared
        .lock()
        .map_err(|_| Fault::Internal("a previous request panicked".to_string()))
}

fn store(sandbox: &mut Sandbox) -> Result<&mut Store, Fault> {
    sandbox
        .store
        .as_mut()
        .ok_or_else(|| Fault::Conflict("no modules are loaded; load them first".to_string()))
}

fn text<'a>(body: &'a J, key: &str) -> Option<&'a str> {
    body.get(key).and_then(J::as_str)
}

// ── the verdict, the object and the event as the wire carries them ─────────
fn remedy(r: Option<objectflow_core::model::Remedy>) -> J {
    r.map_or(J::Null, |r| json!(r.text()))
}

fn flags(fs: &[Flag]) -> J {
    J::Array(
        fs.iter()
            .map(|f| json!({"clause": f.clause, "remedy": remedy(f.remedy)}))
            .collect(),
    )
}

fn verdict(v: &Verdict) -> J {
    let mut out = json!({ "kind": v.kind(), "remedy": remedy(v.remedy()) });
    let fields = match v {
        Verdict::Satisfied {
            object_id,
            version,
            events,
            flags: fs,
        } => {
            json!({"object_id": object_id, "version": version, "events": events, "replayed": false, "flags": flags(fs),
                    "not_computed": ["flags[].unknown", "flags[].objects"]})
        }
        Verdict::Unsatisfied { clause, .. } => {
            json!({"clause": clause, "not_computed": ["unknown", "objects", "proposable", "consulted"]})
        }
        Verdict::NotFound => json!({}),
        Verdict::Stale { expected, actual } => {
            json!({ "cause": "expected_version", "expected": expected, "actual": actual })
        }
        Verdict::Unavailable { state } => json!({ "state": state }),
        Verdict::UnknownTransition { transition } => json!({ "transition": transition }),
        Verdict::InvalidInput { inputs } => json!({ "inputs": inputs }),
        Verdict::NotRequestable { parents } => json!({ "parents": parents }),
        Verdict::CallRefused {
            step,
            object_id,
            verdict: inner,
            clause,
        } => json!({
            "step": step, "object_id": object_id,
            // the call's own refusal, its clause qualified by the type it
            // targeted, as an expectation names it (flow-format.md §11.4)
            "verdict": {"kind": inner.kind(), "clause": clause, "remedy": remedy(inner.remedy())},
            "not_computed": ["the call's verdict beyond its clause"],
        }),
        Verdict::OverLimit { loop_, bound } => json!({ "loop": loop_, "bound": bound }),
        Verdict::InvariantViolated { failures, .. } => json!({
            "failures": failures.iter().map(|f| json!({"object_id": f.object_id, "type": f.type_,
                "invariant": f.invariant, "remedy": remedy(f.remedy)})).collect::<Vec<_>>(),
            "not_computed": ["objects"],
        }),
    };
    if let (Some(o), J::Object(f)) = (out.as_object_mut(), fields) {
        o.extend(f);
    }
    if matches!(v, Verdict::Satisfied { .. })
        && let Some(o) = out.as_object_mut()
    {
        o.remove("remedy");
    }
    out
}

fn event(e: &Event) -> J {
    json!({
        "position": e.position, "object_id": e.object_id, "transition": e.transition,
        "from_state": e.from_state, "to_state": e.to_state, "changes_state": e.changes_state,
        "actor_id": e.actor_id, "actor_kind": e.actor_kind, "principal": e.principal,
        "declaration_version": e.declaration_version,
        "recorded_at": timestamp_text(e.recorded_at),
        "occurred_at": e.occurred_at.map(timestamp_text), "imported": e.imported,
        "payload": e.payload, "cause": e.cause, "reads": e.reads,
    })
}

const EVENT_NOT_COMPUTED: [&str; 7] = [
    "object_seq",
    "source",
    "context",
    "taint_version",
    "txn",
    "retries",
    "flags",
];

fn offer(o: &Offer) -> J {
    let inputs: Map<String, J> = o
        .inputs
        .iter()
        .map(|(k, t)| (k.clone(), json!(t)))
        .collect();
    json!({
        "transition": o.transition, "creates": o.creates, "availability": o.availability.text(),
        "verdict": o.verdict.as_ref().map(verdict), "inputs": inputs,
        "unevaluated": o.unevaluated, "flags": flags(&o.flags),
    })
}

// ── the sandbox's controls ────────────────────────────────────────────────
async fn load(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let mut files = Vec::new();
    for f in body
        .get("files")
        .and_then(J::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(name), Some(text)) = (text(f, "name"), text(f, "text")) else {
            return Err(Fault::Bad(
                "load takes files, each with a name and a text".to_string(),
            ));
        };
        // an examples file is part of version 1's source, and no module
        files.push((name.to_string(), text.to_string()));
    }
    if files.is_empty() {
        return Err(Fault::Bad(
            "load takes the module files, in import order".to_string(),
        ));
    }
    let lib = match Library::load(&files) {
        Ok(lib) => lib,
        Err(e) => {
            return Ok(Json(json!({"loaded": false, "findings": [
                {"file": e.file, "line": e.line, "code": "load", "message": e.message}]})));
        }
    };
    let types: Vec<String> = lib
        .types
        .keys()
        .filter(|t| *t != "Operator")
        .cloned()
        .collect();
    let answer = json!({"loaded": true, "modules": lib.modules, "notices": 0, "now": CLOCK_START,
                        "types": types, "evaluators": lib.evaluators});
    let now = parse_timestamp(CLOCK_START).unwrap_or(0);
    let store = Store::sandbox(lib, now)?;
    let mut sandbox = locked(&shared)?;
    sandbox.files = files;
    sandbox.store = Some(store);
    sandbox.answers = Map::new();
    Ok(Json(answer))
}

async fn reset(State(shared): State<Shared>) -> Answer {
    let mut sandbox = locked(&shared)?;
    if sandbox.files.is_empty() {
        return Err(Fault::Conflict(
            "no modules are loaded; load them first".to_string(),
        ));
    }
    let lib = Library::load(&sandbox.files).map_err(|e| Fault::Internal(e.to_string()))?;
    let now = parse_timestamp(CLOCK_START).unwrap_or(0);
    sandbox.store = Some(Store::sandbox(lib, now)?);
    sandbox.answers = Map::new();
    Ok(Json(json!({ "now": CLOCK_START })))
}

async fn clock(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let target = if let Some(by) = text(&body, "advance") {
        let step = parse_duration(by)
            .ok_or_else(|| Fault::Bad(format!("advance takes a duration such as '1 h': {by}")))?;
        s.now + step
    } else if let Some(at) = text(&body, "set") {
        parse_timestamp(at)
            .ok_or_else(|| Fault::Bad(format!("set takes an RFC 3339 timestamp: {at}")))?
    } else {
        s.now
    };
    if target < s.now {
        return Err(Fault::Bad(
            "the clock never moves back, or an interval would".to_string(),
        ));
    }
    s.now = target;
    Ok(Json(json!({ "now": timestamp_text(s.now) })))
}

async fn evaluators(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let mut sandbox = locked(&shared)?;
    store(&mut sandbox)?;
    if let Some(answers) = body.get("answers").and_then(J::as_object) {
        sandbox.answers.extend(answers.clone());
    }
    // the store asks its evaluators through these answers (PRD F13)
    let given = sandbox
        .answers
        .iter()
        .filter_map(|(k, v)| v.as_str().map(|a| (k.clone(), a.to_string())))
        .collect();
    store(&mut sandbox)?.answers = given;
    Ok(Json(json!({ "answers": sandbox.answers })))
}

// ── the library's operations ──────────────────────────────────────────────
async fn request_op(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let req = request_of(&body)?;
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let v = request::request(s, &req)??;
    Ok(Json(verdict(&v)))
}

/// The verdict a request would receive, simulated and never written, with
/// the guards it did not evaluate (`library-api.md` §6, ADR-0054).
async fn check(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let req = request_of(&body)?;
    if req.propose {
        return Err(Fault::Bad(
            "a check simulates a request, not a proposal".to_string(),
        ));
    }
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let c = request::check(s, &req)??;
    Ok(Json(
        json!({ "verdict": verdict(&c.verdict), "unevaluated": c.unevaluated }),
    ))
}

/// A settled cursor as the wire carries it, an opaque string, which on
/// SQLite is a position (ADR-0089).
fn cursor_of(body: &J) -> Result<Option<i64>, Fault> {
    match body.get("cursor") {
        None | Some(J::Null) => Ok(None),
        Some(J::String(c)) => c
            .parse::<i64>()
            .map(Some)
            .map_err(|_| Fault::Bad(format!("{c} is no cursor this store handed out"))),
        Some(other) => Err(Fault::Bad(format!(
            "{other} is no cursor this store handed out"
        ))),
    }
}

/// A page of the events a subscription selects after a cursor, or its
/// acknowledged one, and the settled cursor (`library-api.md` §6).
async fn pull(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let (Some(_actor), Some(subscription)) = (
        body.get("actor").and_then(|a| text(a, "id")),
        text(&body, "subscription"),
    ) else {
        return Err(Fault::Bad(
            "pull names its actor and the subscription".to_string(),
        ));
    };
    let cursor = cursor_of(&body)?;
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let world = s.snapshot()?;
    let page = objectflow_core::subscription::pull(&world, subscription, cursor);
    Ok(Json(json!({
        "events": page.events.iter().map(event).collect::<Vec<_>>(),
        "settled": page.settled.to_string(),
        "refused": page.refused,
        "not_computed": EVENT_NOT_COMPUTED,
    })))
}

/// A page of objects as the wire gives it: each object's id, type and
/// fields, or a refusal, a value.
fn page_json(page: &reads::QueryPage) -> J {
    json!({
        "items": page.items.iter().map(|i| json!({
            "id": i.id, "type": i.type_,
            "fields": J::Object(i.fields.iter().map(|(f, v)| (f.clone(), encode(v))).collect()),
        })).collect::<Vec<_>>(),
        "cursor": J::Null,
        "refused": page.refused,
    })
}

/// The objects an assertion put in their state, the import's excepted, and
/// those holding an admitted violation (`DESIGN.md` §10).
async fn exceptions(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let (Some(_actor), Some(type_)) = (
        body.get("actor").and_then(|a| text(a, "id")),
        text(&body, "type"),
    ) else {
        return Err(Fault::Bad(
            "exceptions names its actor and the type".to_string(),
        ));
    };
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let world = s.snapshot()?;
    Ok(Json(page_json(&reads::exceptions(&world, type_))))
}

/// The objects of a type a transition is offered to, asking no evaluator
/// (`DESIGN.md` §10, ADR-0048).
async fn available(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let (Some(actor), Some(type_), Some(transition)) = (
        body.get("actor").and_then(|a| text(a, "id")),
        text(&body, "type"),
        text(&body, "transition"),
    ) else {
        return Err(Fault::Bad(
            "available names its actor, the type and the transition".to_string(),
        ));
    };
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let world = s.snapshot()?;
    Ok(Json(page_json(&reads::available(
        &world, actor, type_, transition,
    )?)))
}

/// A source's rows after a cursor, as JSON lines, every personal value
/// omitted; a refusal is a value (`library-api.md` §6, ADR-0094).
async fn export(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let (Some(_actor), Some(source)) = (
        body.get("actor").and_then(|a| text(a, "id")),
        text(&body, "source"),
    ) else {
        return Err(Fault::Bad(
            "export names its actor and the source".to_string(),
        ));
    };
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let world = s.snapshot()?;
    let page = objectflow_core::export::export(&world, source, text(&body, "cursor"))?;
    Ok(Json(json!({
        "lines": page.lines,
        "cursor": page.cursor,
        "refused": page.refused,
    })))
}

/// A subscription's progress moved, back as well as forward; a refusal is
/// a value (`library-api.md` §6).
async fn acknowledge(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let (Some(_actor), Some(subscription), Some(cursor)) = (
        body.get("actor").and_then(|a| text(a, "id")),
        text(&body, "subscription"),
        cursor_of(&body)?,
    ) else {
        return Err(Fault::Bad(
            "acknowledge names its actor, the subscription and a cursor".to_string(),
        ));
    };
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let refused = s.acknowledge(subscription, cursor)?;
    Ok(Json(json!({ "refused": refused })))
}

/// A request as the wire carries it (`library-api.md` §3).
fn request_of(body: &J) -> Result<Request, Fault> {
    if body.get("idempotency_key").is_some_and(|v| !v.is_null()) {
        return Err(Fault::NotRun("a request's idempotency_key".to_string()));
    }
    // the version of its object the caller read (library-api.md §3)
    let expected_version = match body.get("expected_version") {
        None | Some(J::Null) => None,
        Some(v) => Some(v.as_i64().ok_or_else(|| {
            Fault::Bad("expected_version takes an object's version, an integer".to_string())
        })?),
    };
    let propose = body.get("propose").and_then(J::as_bool) == Some(true);
    // a proposal's expiry, with `propose` only (library-api.md §3)
    let expires_at =
        match text(body, "expires_at") {
            Some(_) if !propose => {
                return Err(Fault::Bad(
                    "expires_at is a proposal's, with propose".to_string(),
                ));
            }
            Some(t) => Some(parse_timestamp(t).ok_or_else(|| {
                Fault::Bad(format!("expires_at takes an RFC 3339 timestamp: {t}"))
            })?),
            None => None,
        };
    let (Some(transition), Some(actor)) = (
        text(body, "transition"),
        body.get("actor").and_then(|a| text(a, "id")),
    ) else {
        return Err(Fault::Bad(
            "a request names its transition and its actor".to_string(),
        ));
    };
    let object_id = text(body, "object_id").map(str::to_string);
    let type_ = text(body, "type").map(str::to_string);
    if object_id.is_some() == type_.is_some() {
        return Err(Fault::Bad(
            "a request names an object_id or, to create, a type".to_string(),
        ));
    }
    let occurred_at =
        match text(body, "occurred_at") {
            Some(t) => Some(parse_timestamp(t).ok_or_else(|| {
                Fault::Bad(format!("occurred_at takes an RFC 3339 timestamp: {t}"))
            })?),
            None => None,
        };
    let inputs = body
        .get("inputs")
        .and_then(J::as_object)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let req = Request {
        transition: transition.to_string(),
        actor: actor.to_string(),
        object_id,
        type_,
        inputs,
        occurred_at,
        propose,
        expires_at,
        expected_version,
        // the principal the actor acts for, which the store records (library-api.md §3)
        principal: body
            .get("actor")
            .and_then(|a| text(a, "principal"))
            .map(str::to_string),
    };
    Ok(req)
}

// ── the import (ADR-0154) ─────────────────────────────────────────────────
fn instant(v: Option<&J>, what: &str) -> Result<Option<i64>, Fault> {
    match v {
        None | Some(J::Null) => Ok(None),
        Some(J::String(t)) => parse_timestamp(t)
            .map(Some)
            .ok_or_else(|| Fault::Bad(format!("{what} takes an RFC 3339 timestamp: {t}"))),
        Some(other) => Err(Fault::Bad(format!(
            "{what} takes an RFC 3339 timestamp: {other}"
        ))),
    }
}

fn imported_object(o: &J) -> Result<ImportedObject, Fault> {
    let field = |k: &str| text(o, k).map(str::to_string);
    let (Some(type_), Some(legacy_key), Some(state)) =
        (field("type"), field("legacy_key"), field("state"))
    else {
        return Err(Fault::Bad(
            "an imported object names its type, legacy_key and state".to_string(),
        ));
    };
    // removed by ADR-0154: recorded_from is computed (C24)
    if o.get("recorded_from").is_some_and(|v| !v.is_null()) {
        return Err(Fault::Bad(
            "recorded_from is computed by the import, not given (ADR-0154)".to_string(),
        ));
    }
    let mut entered = std::collections::BTreeMap::new();
    for (dimension, e) in o
        .get("entered")
        .and_then(J::as_object)
        .into_iter()
        .flatten()
    {
        let Some(at) = instant(e.get("at"), "entered.at")? else {
            return Err(Fault::Bad(format!(
                "entered.{dimension} names its time, at"
            )));
        };
        entered.insert(
            dimension.clone(),
            EnteredAt {
                at,
                by: text(e, "by").map(str::to_string),
                by_kind: text(e, "by_kind").map(str::to_string),
            },
        );
    }
    // what the core does not yet import, named (ADR-0154 decision 6)
    let mut beyond = Vec::new();
    if o.get("legacy_entries")
        .and_then(J::as_array)
        .is_some_and(|a| !a.is_empty())
    {
        beyond.push(format!("legacy entries, on {type_} {legacy_key}"));
    }
    if o.get("occurred_at").is_some_and(|v| !v.is_null()) {
        beyond.push(format!(
            "an observation's occurred time, on {type_} {legacy_key}"
        ));
    }
    // the earlier values the legacy record gives, and where it is silent
    let mut legacy_intervals = Vec::new();
    for l in o
        .get("legacy_intervals")
        .and_then(J::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(dimension), Some(entered_at)) = (
            text(l, "dimension"),
            instant(l.get("entered_at"), "legacy_intervals.entered_at")?,
        ) else {
            return Err(Fault::Bad(
                "a legacy interval names its dimension and when it was entered".to_string(),
            ));
        };
        legacy_intervals.push(LegacyInterval {
            dimension: dimension.to_string(),
            value: l.get("value").cloned().unwrap_or(J::Null),
            entered_at,
            left_at: instant(l.get("left_at"), "legacy_intervals.left_at")?,
            entered_by: text(l, "entered_by").map(str::to_string),
            entered_by_kind: text(l, "entered_by_kind").map(str::to_string),
        });
    }
    let mut silent_spans = Vec::new();
    for span in o
        .get("silent_spans")
        .and_then(J::as_array)
        .into_iter()
        .flatten()
    {
        let ends = span.as_array().map(|a| a.as_slice()).unwrap_or_default();
        let (Some(start), Some(end)) = (
            instant(ends.first(), "silent_spans")?,
            instant(ends.get(1), "silent_spans")?,
        ) else {
            return Err(Fault::Bad(
                "a silent span is a pair of times, its start and its end".to_string(),
            ));
        };
        silent_spans.push((start, end));
    }
    Ok(ImportedObject {
        legacy_intervals,
        silent_spans,
        type_,
        legacy_key,
        state,
        attributes: o
            .get("attributes")
            .and_then(J::as_object)
            .map(|m| m.clone().into_iter().collect())
            .unwrap_or_default(),
        entered,
        created_at: instant(o.get("created_at"), "created_at")?,
        created_by_kind: field("created_by_kind"),
        beyond,
    })
}

fn violation_class(c: &J) -> Result<ViolationClass, Fault> {
    let field = |k: &str| text(c, k).map(str::to_string);
    let kind = field("kind").as_deref().and_then(ViolationKind::parse);
    let disposition = field("disposition")
        .as_deref()
        .map_or(Some(Disposition::Undecided), Disposition::parse);
    let (Some(type_), Some(kind), Some(rule), Some(shape), Some(disposition)) = (
        field("type"),
        kind,
        field("rule"),
        field("shape"),
        disposition,
    ) else {
        return Err(Fault::Bad(
            "a class names its type, kind, rule and shape, and a disposition of publish-and-import.md §6".to_string(),
        ));
    };
    Ok(ViolationClass {
        id: field("id").unwrap_or_default(),
        type_,
        kind,
        rule,
        shape,
        count: c
            .get("count")
            .and_then(J::as_u64)
            .map_or(0, |n| usize::try_from(n).unwrap_or(usize::MAX)),
        sample: c
            .get("sample")
            .and_then(J::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        disposition,
        decided_by: field("decided_by"),
        reason: field("reason"),
    })
}

fn report(r: &ImportReport) -> J {
    json!({
        "written": r.written,
        "findings": r.findings.iter().map(|c| json!({
            "id": c.id, "type": c.type_, "kind": c.kind.text(), "rule": c.rule, "shape": c.shape,
            "count": c.count, "sample": c.sample, "disposition": c.disposition.text(),
            "decided_by": c.decided_by, "reason": c.reason,
        })).collect::<Vec<_>>(),
        "refused_types": r.refused_types,
        "refusal": r.refusal.as_ref().map(|f| json!({"type": f.type_, "legacy_key": f.legacy_key, "reason": f.reason})),
    })
}

async fn import_op(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let Some(actor) = body.get("actor").and_then(|a| text(a, "id")) else {
        return Err(Fault::Bad("an import names its actor".to_string()));
    };
    let Some(b) = body.get("batch") else {
        return Err(Fault::Bad("an import names its batch".to_string()));
    };
    // removed by ADR-0154: the file is the one way an import admits (C32)
    if b.get("admitted").is_some_and(|v| !v.is_null()) {
        return Err(Fault::Bad(
            "a batch admits through its disposition file, not admitted (ADR-0154)".to_string(),
        ));
    }
    let mut batch = ImportBatch::default();
    for o in b.get("objects").and_then(J::as_array).into_iter().flatten() {
        batch.objects.push(imported_object(o)?);
    }
    for r in b
        .get("sequences")
        .and_then(J::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(sequence), Some(last)) = (text(r, "sequence"), r.get("last").and_then(J::as_i64))
        else {
            return Err(Fault::Bad(
                "a sequence names its sequence and the last value minted".to_string(),
            ));
        };
        batch.sequences.push(Raise {
            sequence: sequence.to_string(),
            scope: text(r, "scope").unwrap_or_default().to_string(),
            last,
        });
    }
    let mut file = Vec::new();
    for c in body
        .get("disposition")
        .and_then(J::as_array)
        .into_iter()
        .flatten()
    {
        file.push(violation_class(c)?);
    }
    let dry_run = body.get("dry_run").and_then(J::as_bool) == Some(true);
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let r = import::import_batch(s, actor, &batch, &file, dry_run)??;
    Ok(Json(report(&r)))
}

/// An object as `get` reads it, or null where the store holds none.
fn object_json(s: &Store, id: &str) -> Answer {
    let world = s.snapshot()?;
    let Some(view) = reads::get(&world, id)? else {
        return Ok(Json(J::Null));
    };
    let o = &view.object;
    let Some(t) = s.lib.type_(&o.type_) else {
        return Err(Fault::Internal(format!("the type {}", o.type_)));
    };
    // the version its row was last written under, which its last event
    // carries (storage-schema.md §3)
    let declaration_version = s
        .history(&o.id)?
        .last()
        .map_or(s.lib.version, |e| e.declaration_version);
    let mut attributes = Map::new();
    for (n, _) in &t.attributes {
        attributes.insert(n.clone(), encode(&o.attr(n)));
    }
    for (n, v) in &view.derived {
        attributes.insert(n.clone(), encode(v));
    }
    Ok(Json(json!({
        "id": o.id, "type": o.type_, "state": o.state, "version": o.version, "declaration_version": declaration_version,
        "created_at": timestamp_text(o.created_at), "created_by_kind": o.created_by_kind,
        "created_dated": o.created_dated, "attributes": attributes,
        "recorded_from": timestamp_text(o.recorded_from), "open": view.open,
    })))
}

async fn get(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    object_json(s, text(&body, "id").unwrap_or_default())
}

/// A change's dry run, its report the impact a publish would have, or its
/// approval, which installs it (`library-api.md` §6, ADR-0155).
async fn publish(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let (Some(actor), Some(change)) = (
        body.get("actor").and_then(|a| text(a, "id")),
        text(&body, "change"),
    ) else {
        return Err(Fault::Bad(
            "publish names its actor and the change".to_string(),
        ));
    };
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    if body.get("dry_run").and_then(J::as_bool) != Some(true) {
        // the version of the change the approver read, required (ADR-0101)
        let Some(expected) = body.get("expected_version").and_then(J::as_i64) else {
            return Err(Fault::Bad(
                "a publish names the expected_version of the change it approves".to_string(),
            ));
        };
        let (v, report) = s.publish(actor, change, expected)??;
        return Ok(Json(json!({
            "verdict": verdict(&v),
            "report": report.as_ref().map(objectflow_core::publish::report_json),
        })));
    }
    let world = s.snapshot()?;
    let Some(c) = world
        .get(change)
        .filter(|c| c.type_ == objectflow_core::model::DECLARATION_CHANGE)
    else {
        return Err(Fault::Missing(format!("no change {change}")));
    };
    let attr = |a: &str| match c.attr(a) {
        objectflow_core::value::Value::Text(t) => t,
        _ => String::new(),
    };
    let source: Vec<(String, String)> = serde_json::from_str::<Map<String, J>>(&attr("source"))
        .unwrap_or_default()
        .into_iter()
        .map(|(n, t)| (n, t.as_str().unwrap_or_default().to_string()))
        .collect();
    let report = objectflow_core::publish::dry_run(
        &world,
        &objectflow_core::publish::Draft {
            change_id: Some(change),
            source: &source,
            source_ref: Some(&attr("source_ref")),
            source_digest: Some(&attr("source_digest")),
            actor,
        },
    )?;
    Ok(Json(
        json!({ "verdict": J::Null, "report": objectflow_core::publish::report_json(&report) }),
    ))
}

/// The object whose external identifier for a source has a value, a legacy
/// key among them (`library-api.md` §6, ADR-0154).
async fn lookup(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let (Some(source), Some(value)) = (text(&body, "source"), text(&body, "value")) else {
        return Err(Fault::Bad("lookup names a source and a value".to_string()));
    };
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let world = s.snapshot()?;
    match world.external.get(&(source.to_string(), value.to_string())) {
        Some(id) => object_json(s, &id.clone()),
        None => Ok(Json(J::Null)),
    }
}

async fn history(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let events = s.history(text(&body, "id").unwrap_or_default())?;
    Ok(Json(
        json!({"events": events.iter().map(event).collect::<Vec<_>>(), "not_computed": EVENT_NOT_COMPUTED}),
    ))
}

async fn events(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let Some(positions) = body
        .get("positions")
        .and_then(J::as_array)
        .and_then(|ps| ps.iter().map(J::as_i64).collect::<Option<Vec<i64>>>())
    else {
        return Err(Fault::Bad(
            "events takes positions, a list of integers".to_string(),
        ));
    };
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let found = s.events(&positions)?;
    Ok(Json(json!({
        "events": found.iter().map(|e| e.as_ref().map_or(J::Null, event)).collect::<Vec<_>>(),
        "not_computed": EVENT_NOT_COMPUTED,
    })))
}

async fn declaration(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let name = text(&body, "type").unwrap_or_default();
    let Some(t) = s.lib.type_(name) else {
        return Err(Fault::Missing(format!("no type {name} is declared")));
    };
    let mut out = t.source.clone();
    // the members of each enumeration the type's attributes and inputs are of
    let mut used = Map::new();
    let decl = t.attributes.iter().map(|(_, a)| &a.declared).chain(
        t.transitions
            .iter()
            .flat_map(|(_, x)| x.inputs.iter().map(|(_, i)| &i.declared)),
    );
    for d in decl {
        if let Declared::Scalar(e) = d
            && let Some(members) = s.lib.enumerations.get(e)
        {
            used.insert(e.clone(), json!(members));
        }
    }
    if let Some(o) = out.as_object_mut() {
        o.insert("enumerations".to_string(), J::Object(used));
    }
    Ok(Json(out))
}

async fn availability(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let (Some(actor), Some(id)) = (
        body.get("actor").and_then(|a| text(a, "id")),
        text(&body, "id"),
    ) else {
        return Err(Fault::Bad(
            "availability names its actor and an object's id".to_string(),
        ));
    };
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let world = s.snapshot()?;
    match reads::availability(&world, actor, id)? {
        Some(offers) => Ok(Json(J::Array(offers.iter().map(offer).collect()))),
        None => Err(Fault::Missing(format!("no object {id}"))),
    }
}

/// A reader's read of a metric: one page holding every group, so its
/// cursor is none (`library-api.md` §5, `MetricPage`).
/// A page of objects of a type or its family, filtered and ordered
/// (`library-api.md` §6, ADR-0127): every object the filter holds for, on
/// one page, so no cursor; a refusal is a value.
async fn query(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let (Some(_actor), Some(type_)) = (
        body.get("actor").and_then(|a| text(a, "id")),
        text(&body, "type"),
    ) else {
        return Err(Fault::Bad("query names its actor and the type".to_string()));
    };
    if body.get("cursor").is_some_and(|c| !c.is_null()) {
        return Err(Fault::Bad(
            "a query's page holds every object, so it has no cursor to continue from".to_string(),
        ));
    }
    let strings = |k: &str| -> Result<Option<Vec<String>>, Fault> {
        match body.get(k) {
            None | Some(J::Null) => Ok(None),
            Some(J::String(one)) => Ok(Some(vec![one.clone()])),
            Some(J::Array(items)) => items
                .iter()
                .map(|i| i.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
                .map(Some)
                .ok_or_else(|| Fault::Bad(format!("{k} takes a name or a list of names"))),
            Some(_) => Err(Fault::Bad(format!("{k} takes a name or a list of names"))),
        }
    };
    let order = strings("order")?.unwrap_or_default();
    let fields = strings("fields")?;
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let world = s.snapshot()?;
    let page = reads::query(
        &world,
        type_,
        text(&body, "filter"),
        &order,
        fields.as_deref(),
    )?;
    Ok(Json(json!({
        "items": page.items.iter().map(|i| json!({
            "id": i.id, "type": i.type_,
            "fields": J::Object(i.fields.iter().map(|(f, v)| (f.clone(), encode(v))).collect()),
        })).collect::<Vec<_>>(),
        "cursor": J::Null,
        "refused": page.refused,
    })))
}

async fn metric(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let (Some(_actor), Some(name)) = (
        body.get("actor").and_then(|a| text(a, "id")),
        text(&body, "name"),
    ) else {
        return Err(Fault::Bad(
            "metric names its actor and the metric".to_string(),
        ));
    };
    if body.get("cursor").is_some_and(|c| !c.is_null()) {
        return Err(Fault::Bad(
            "a metric's page holds every group, so it has no cursor to continue from".to_string(),
        ));
    }
    let bind = body
        .get("bind")
        .and_then(J::as_object)
        .map(|b| b.clone().into_iter().collect())
        .unwrap_or_default();
    let keep: Option<Vec<String>> = match body.get("keep") {
        None | Some(J::Null) => None,
        Some(J::Array(items)) => Some(
            items
                .iter()
                .map(|d| d.as_str().map(str::to_string))
                .collect::<Option<_>>()
                .ok_or_else(|| Fault::Bad("keep takes dimension names".to_string()))?,
        ),
        Some(_) => return Err(Fault::Bad("keep takes dimension names".to_string())),
    };
    let over =
        match text(&body, "over") {
            None => None,
            Some(by) => Some(parse_duration(by).ok_or_else(|| {
                Fault::Bad(format!("over takes a duration such as '30 days': {by}"))
            })?),
        };
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let world = s.snapshot()?;
    let page = objectflow_core::metric::page(
        &world,
        Some(&s.answers),
        name,
        &bind,
        keep.as_deref(),
        over,
        text(&body, "filter"),
    )?;
    let group = |pairs: &[(String, objectflow_core::value::Value)]| {
        J::Object(pairs.iter().map(|(d, v)| (d.clone(), encode(v))).collect())
    };
    Ok(Json(json!({
        "rows": page.rows.iter().map(|r| json!({
            "dimensions": group(&r.dimensions), "value": encode(&r.value), "gaps": r.gaps,
            "time_basis": "UTC calendar",
        })).collect::<Vec<_>>(),
        "cursor": J::Null,
        "flags": page.flags.iter().map(|f| json!({
            "flag": f.flag, "group": group(&f.group), "unknown": f.unknown,
        })).collect::<Vec<_>>(),
        "history_complete": page.history_complete,
        "refused": page.refused,
    })))
}

fn app(shared: Shared) -> Router {
    Router::new()
        .route("/v1/request", post(request_op))
        .route("/v1/get", post(get))
        .route("/v1/history", post(history))
        .route("/v1/events", post(events))
        .route("/v1/declaration", post(declaration))
        .route("/v1/availability", post(availability))
        .route("/v1/metric", post(metric))
        .route("/v1/query", post(query))
        .route("/v1/check", post(check))
        .route("/v1/pull", post(pull))
        .route("/v1/export", post(export))
        .route("/v1/exceptions", post(exceptions))
        .route("/v1/available", post(available))
        .route("/v1/acknowledge", post(acknowledge))
        .route("/v1/import_batch", post(import_op))
        .route("/v1/lookup", post(lookup))
        .route("/v1/publish", post(publish))
        .route("/sandbox/load", post(load))
        .route("/sandbox/reset", post(reset))
        .route("/sandbox/clock", post(clock))
        .route("/sandbox/evaluators", post(evaluators))
        .with_state(shared)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> std::io::Result<()> {
    let mut port: u16 = 8712;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--port" {
            port = args
                .next()
                .and_then(|p| p.parse().ok())
                .ok_or_else(|| std::io::Error::other("--port takes a number"))?;
        }
    }
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    eprintln!("listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app(Shared::default())).await
}

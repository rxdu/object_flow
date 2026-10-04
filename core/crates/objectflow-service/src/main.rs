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
        "actor_id": e.actor_id, "actor_kind": e.actor_kind, "declaration_version": e.declaration_version,
        "recorded_at": timestamp_text(e.recorded_at),
        "occurred_at": e.occurred_at.map(timestamp_text), "imported": e.imported,
    })
}

const EVENT_NOT_COMPUTED: [&str; 13] = [
    "object_seq",
    "source",
    "principal",
    "context",
    "cause",
    "taint_version",
    "txn",
    "retries",
    "payload",
    "reads",
    "consulted",
    "as_of",
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
        if !name.ends_with(".examples.yaml") {
            files.push((name.to_string(), text.to_string()));
        }
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
    for k in ["expected_version", "idempotency_key"] {
        if body.get(k).is_some_and(|v| !v.is_null()) {
            return Err(Fault::NotRun(format!("a request's {k}")));
        }
    }
    let propose = body.get("propose").and_then(J::as_bool) == Some(true);
    // a proposal's expiry, with `propose` only (library-api.md §3)
    let expires_at =
        match text(&body, "expires_at") {
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
        text(&body, "transition"),
        body.get("actor").and_then(|a| text(a, "id")),
    ) else {
        return Err(Fault::Bad(
            "a request names its transition and its actor".to_string(),
        ));
    };
    let object_id = text(&body, "object_id").map(str::to_string);
    let type_ = text(&body, "type").map(str::to_string);
    if object_id.is_some() == type_.is_some() {
        return Err(Fault::Bad(
            "a request names an object_id or, to create, a type".to_string(),
        ));
    }
    let occurred_at =
        match text(&body, "occurred_at") {
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
    };
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let v = request::request(s, &req)??;
    Ok(Json(verdict(&v)))
}

async fn get(State(shared): State<Shared>, Json(body): Json<J>) -> Answer {
    let mut sandbox = locked(&shared)?;
    let s = store(&mut sandbox)?;
    let id = text(&body, "id").unwrap_or_default();
    let world = s.snapshot()?;
    let Some(view) = reads::get(&world, id)? else {
        return Ok(Json(J::Null));
    };
    let o = &view.object;
    let Some(t) = s.lib.type_(&o.type_) else {
        return Err(Fault::Internal(format!("the type {}", o.type_)));
    };
    let mut attributes = Map::new();
    for (n, _) in &t.attributes {
        attributes.insert(n.clone(), encode(&o.attr(n)));
    }
    for (n, v) in &view.derived {
        attributes.insert(n.clone(), encode(v));
    }
    Ok(Json(json!({
        "id": o.id, "type": o.type_, "state": o.state, "version": o.version, "declaration_version": 1,
        "created_at": timestamp_text(o.created_at), "created_by_kind": o.created_by_kind,
        "attributes": attributes, "recorded_from": timestamp_text(o.created_at), "open": view.open,
        "not_computed": ["created_dated"],
    })))
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

async fn metric() -> Answer {
    Err(Fault::NotRun(
        "a metric read, which the core's first slice does not offer".to_string(),
    ))
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

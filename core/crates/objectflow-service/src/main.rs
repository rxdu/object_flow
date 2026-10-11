//! ObjectFlow's HTTP service (ADR-0150): one `POST /v1/<operation>` per library
//! operation, and a sandbox's controls under `/sandbox/`, which a deployment's
//! service does not route (ADR-0174). The core is synchronous; this is its one
//! asynchronous layer (ADR-0132), and every operation runs the core under one
//! lock, one request at a time.
//!
//! A refusal is a `200` carrying the verdict, its shape named by `kind`; a
//! fault is a `4xx` or `5xx` carrying `fault`; a construct the core's first
//! slice does not run is a `501` naming it (ADR-0153).
//!
//! Run as `objectflow-service --port <n>` for a sandbox, or with `--store
//! <file>` for a deployment's store (ADR-0174); `--port 0` takes a free port,
//! and the address listened on is written to standard error.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use objectflow_core::eval::{Call, NotRun};
use objectflow_core::import::{
    self, Disposition, EnteredAt, ImportBatch, ImportReport, ImportedObject, LegacyInterval, Raise,
    ViolationClass, ViolationKind,
};
use objectflow_core::model::{Declared, Library};
use objectflow_core::reads::{self, Availability, Offer};
use objectflow_core::request::{self, Request};
use objectflow_core::store::{BUILTINS, ENGINE_MISMATCH, Event, Readers, Store};
use objectflow_core::value::{parse_duration, parse_timestamp, timestamp_text};
use objectflow_core::verdict::{Flag, Told, Verdict};
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
    /// The retentions the store is built with, which a reset keeps
    /// (ADR-0101, ADR-0103): none unless the load names them.
    retentions: (Option<i64>, Option<i64>),
    /// Whether the store is a deployment's, opened from its file, whose clock
    /// is the wall clock and whose controls are not routed (ADR-0174).
    deployment: bool,
    /// A deployment's evaluator endpoints, by namespace, and how long an
    /// answer is waited for (ADR-0175 decision 5).
    endpoints: std::collections::BTreeMap<String, String>,
    evaluator_timeout: std::time::Duration,
    /// A deployment's store file, which an upgrade opens.
    path: Option<std::path::PathBuf>,
    /// How long its writes wait out another process's write lock
    /// (ADR-0178 decision 2).
    busy_timeout: std::time::Duration,
    /// The release a deployment's store is at where it is due an upgrade to
    /// this one: every operation but the upgrade is refused until it runs
    /// (ADR-0177 decision 4).
    due: Option<String>,
}

/// The most calls one request asks its endpoints, after which a call is
/// unknown (ADR-0175 decision 2).
const MAX_CALLS: usize = 16;

/// An evaluator's verdict on a call, asked at its endpoint (ADR-0175
/// decision 4): `satisfied` or `unsatisfied` as it answers, and
/// `unanswered`, its unknown, for any other answer, a fault, a timeout or no
/// connection, each said on stderr.
fn asked(endpoint: &str, timeout: std::time::Duration, call: &Call) -> &'static str {
    let body = json!({"evaluator": call.evaluator, "function": call.function, "arguments": call.arguments})
        .to_string();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into();
    // the call by its function alone: its arguments and the endpoint's
    // answer may hold personal data, which the log does not (production
    // review, minor)
    let unknown = |why: String| {
        eprintln!(
            "objectflow-service: {}.{} of {} argument(s) is unknown: {why}",
            call.evaluator,
            call.function,
            call.arguments.len()
        );
        "unanswered"
    };
    let mut answer = match agent
        .post(endpoint)
        .header("content-type", "application/json")
        .send(&body)
    {
        Ok(a) => a,
        Err(e) => return unknown(format!("{endpoint} was not reached: {e}")),
    };
    let status = answer.status().as_u16();
    let text = match answer.body_mut().read_to_string() {
        Ok(t) => t,
        Err(e) => return unknown(format!("{endpoint}'s answer was not read: {e}")),
    };
    if status != 200 {
        return unknown(format!("{endpoint} answered {status}"));
    }
    match serde_json::from_str::<J>(&text)
        .ok()
        .as_ref()
        .and_then(|a| a.get("verdict"))
        .and_then(J::as_str)
    {
        Some("satisfied") => "satisfied",
        Some("unsatisfied") => "unsatisfied",
        _ => unknown(format!(
            "{endpoint} answered no verdict, in {} byte(s)",
            text.len()
        )),
    }
}

/// An attempt's outcome, run again with a verdict where it stopped at a call
/// its answers lack, which a deployment asks at the evaluator's endpoint
/// outside the request's transaction (ADR-0175 decision 2), and with the
/// service's lock let go, so every other request is served while an
/// endpoint answers (D563); the verdicts are the request's alone.
fn consulting<T>(
    shared: &Shared,
    read: bool,
    mut attempt: impl FnMut(&mut Store) -> Result<T, Fault>,
    stopped: impl Fn(&T) -> Option<Call>,
) -> Result<T, Fault> {
    let mut asked_so_far: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut calls = 0;
    let timeout = shared.evaluator_timeout;
    loop {
        // a check on a read of its own, a request on the writer (ADR-0178)
        let out = {
            let mut run = |s: &mut Store| {
                let given = s.answers.clone();
                s.answers.extend(asked_so_far.clone());
                let out = attempt(s);
                s.answers = given;
                out
            };
            if read {
                let mut r = reading(shared)?;
                run(r.store()?)
            } else {
                let mut sandbox = locked(shared)?;
                run(store(&mut sandbox)?)
            }
        };
        let call = match &out {
            Ok(t) if shared.deployment => stopped(t),
            _ => None,
        };
        let endpoint = call
            .as_ref()
            .and_then(|c| shared.endpoints.get(&c.evaluator).cloned());
        // a call that changes at every attempt stops the request, unrun
        let Some(call) = call.filter(|_| calls < 2 * MAX_CALLS) else {
            return out;
        };
        let verdict = match endpoint {
            Some(endpoint) if calls < MAX_CALLS => asked(&endpoint, timeout, &call),
            _ => "unanswered",
        };
        calls += 1;
        asked_so_far.insert(call.key(), verdict.to_string());
    }
}

/// UTC now, in seconds since 1970: a deployment's clock (ADR-0174 decision
/// 4).
fn wall_clock() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// The service's state (ADR-0178): its writer, behind the lock every write
/// takes, and for a deployment what its reads need without that lock; and
/// what it was started with.
struct Service {
    writer: Mutex<Sandbox>,
    reads: Option<Reads>,
    /// A deployment's evaluator endpoints, by namespace, and how long an
    /// answer is waited for, fixed when it starts (ADR-0175 decision 5).
    endpoints: std::collections::BTreeMap<String, String>,
    evaluator_timeout: std::time::Duration,
    deployment: bool,
    /// What it has answered, by route and status: how many, and their
    /// seconds, which `GET /metrics` gives (production review OBSERVE).
    served: Mutex<std::collections::BTreeMap<(String, u16), (u64, f64)>>,
}

/// What a deployment's reads need beside its writer (ADR-0178): its read
/// connections, what the writer last held, and the latest time the store
/// recorded or read, behind which no read's clock goes (ADR-0174 decision
/// 4).
struct Reads {
    pool: Readers,
    /// The store's file, which `GET /health` opens afresh.
    path: std::path::PathBuf,
    seen: std::sync::RwLock<Seen>,
    floor: AtomicI64,
}

/// The library a store runs and the earlier versions', as a read shares
/// them with the writer.
type Rules = (Arc<Library>, Arc<std::collections::BTreeMap<i64, Library>>);

/// What the writer held when it last let its lock go: the library and the
/// earlier versions', its evaluator answers, and an upgrade it is due.
#[derive(Clone, Default)]
struct Seen {
    lib: Option<Rules>,
    answers: std::collections::BTreeMap<String, String>,
    due: Option<String>,
}

impl Seen {
    fn of(sandbox: &Sandbox) -> Seen {
        Seen {
            lib: sandbox.store.as_ref().map(Store::shared),
            answers: sandbox
                .store
                .as_ref()
                .map(|s| s.answers.clone())
                .unwrap_or_default(),
            due: sandbox.due.clone(),
        }
    }
}

type Shared = Arc<Service>;

/// The writer, held while a write runs; when it is let go, what reads need
/// is refreshed from it, so a read after a publish reads its version
/// (ADR-0178 decision 3).
struct Writing<'a> {
    guard: std::sync::MutexGuard<'a, Sandbox>,
    reads: Option<&'a Reads>,
}

impl std::ops::Deref for Writing<'_> {
    type Target = Sandbox;
    fn deref(&self) -> &Sandbox {
        &self.guard
    }
}

impl std::ops::DerefMut for Writing<'_> {
    fn deref_mut(&mut self) -> &mut Sandbox {
        &mut self.guard
    }
}

impl Drop for Writing<'_> {
    fn drop(&mut self) {
        if let Some(reads) = self.reads {
            if let Some(s) = &self.guard.store {
                reads.floor.fetch_max(s.now, Ordering::SeqCst);
            }
            if let Ok(mut seen) = reads.seen.write() {
                *seen = Seen::of(&self.guard);
            }
        }
    }
}

/// A read's store (ADR-0178): a deployment's own read connection, in its
/// own read transaction and given back when the read ends; or a sandbox's
/// one store, read under its lock.
enum Reading<'a> {
    Own(Option<Box<Store>>, &'a Reads),
    Locked(Writing<'a>),
}

impl Reading<'_> {
    fn store(&mut self) -> Result<&mut Store, Fault> {
        match self {
            Reading::Own(Some(s), _) => Ok(s),
            Reading::Own(None, _) => {
                Err(Fault::Internal("a read's store was given back".to_string()))
            }
            Reading::Locked(w) => store(w),
        }
    }
}

impl Drop for Reading<'_> {
    fn drop(&mut self) {
        if let Reading::Own(s, reads) = self
            && let Some(s) = s.take()
        {
            reads.pool.give_back(*s);
        }
    }
}

/// A read of the store as it stands, beside the writes and never behind
/// them, for a deployment; under the lock for a sandbox.
fn reading(shared: &Shared) -> Result<Reading<'_>, Fault> {
    let Some(reads) = &shared.reads else {
        return Ok(Reading::Locked(locked(shared)?));
    };
    let seen = reads
        .seen
        .read()
        .map_err(|_| Fault::Internal("a previous request panicked".to_string()))?
        .clone();
    if let Some(at) = seen.due {
        return Err(due(&at));
    }
    let Some((lib, earlier)) = seen.lib else {
        return Err(Fault::Conflict("no store is open".to_string()));
    };
    // an evaluator no endpoint answers is unknown (ADR-0175 decision 5)
    let mut answers = seen.answers;
    for f in &lib.evaluators {
        let namespace = f.split_once('.').map_or(f.as_str(), |(n, _)| n);
        if !shared.endpoints.contains_key(namespace) {
            answers
                .entry(f.clone())
                .or_insert_with(|| "unanswered".to_string());
        }
    }
    let now = reads
        .floor
        .fetch_max(wall_clock(), Ordering::SeqCst)
        .max(wall_clock());
    Ok(Reading::Own(
        Some(Box::new(reads.pool.read(lib, earlier, now, answers)?)),
        reads,
    ))
}

/// The refusal of everything but the upgrade on a store due one.
fn due(at: &str) -> Fault {
    Fault::Mismatch(format!(
        "{ENGINE_MISMATCH}: the store is at {at}, and this core is {BUILTINS}: upgrade it with POST /v1/upgrade"
    ))
}

/// What the service answers a request it cannot: the wire's 4xx or 5xx.
enum Fault {
    Bad(String),
    Missing(String),
    Method(String),
    Conflict(String),
    NotRun(String),
    Internal(String),
    /// `EngineMismatch`: the store is at another engine release
    /// (`library-api.md` §7, ADR-0177).
    Mismatch(String),
}

impl IntoResponse for Fault {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Fault::Bad(m) => (StatusCode::BAD_REQUEST, m),
            Fault::Missing(m) => (StatusCode::NOT_FOUND, m),
            Fault::Method(m) => (StatusCode::METHOD_NOT_ALLOWED, m),
            Fault::Conflict(m) => (StatusCode::CONFLICT, m),
            Fault::NotRun(m) => (StatusCode::NOT_IMPLEMENTED, format!("not run: {m}")),
            Fault::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
            Fault::Mismatch(m) => (StatusCode::SERVICE_UNAVAILABLE, m),
        };
        (status, Json(json!({ "fault": message }))).into_response()
    }
}

/// A construct the core does not run is 501; a read of the store that
/// failed is unavailable storage, the service's own fault, 500, as the
/// store's other errors are (`library-api.md` §7, ADR-0164).
impl From<NotRun> for Fault {
    fn from(e: NotRun) -> Fault {
        match e {
            NotRun::Unsupported(w) => Fault::NotRun(w),
            NotRun::Unanswered(c) => Fault::NotRun(c.said),
            NotRun::Unreadable(w) => Fault::Internal(format!("storage unavailable: {w}")),
        }
    }
}

/// A key reused for a different request is the caller's fault, a conflict
/// with what the key already applied (`library-api.md` §7, ADR-0077).
impl From<request::NoVerdict> for Fault {
    fn from(e: request::NoVerdict) -> Fault {
        match e {
            request::NoVerdict::NotRun(n) => Fault::from(n),
            request::NoVerdict::KeyReused { key } => Fault::Conflict(format!(
                "key_reused: this actor applied the idempotency key {key} to a different request"
            )),
        }
    }
}

impl From<objectflow_core::store::StoreError> for Fault {
    fn from(e: objectflow_core::store::StoreError) -> Fault {
        if e.is_mismatch() {
            return Fault::Mismatch(e.0);
        }
        Fault::Internal(e.to_string())
    }
}

type Answer = Result<Json<J>, Fault>;

/// The keys each operation takes, its arguments as `library-api.md` names
/// them, a request's the fields of `Request` (§3); a key outside them is a
/// fault, so a misspelt one is never read as absent (ADR-0173 decision 12,
/// D555).
const TAKES: &[(&str, &[&str])] = &[
    ("/v1/request", REQUEST_KEYS),
    ("/v1/check", REQUEST_KEYS),
    ("/v1/get", &["actor", "id", "follow"]),
    (
        "/v1/query",
        &[
            "actor", "type", "filter", "order", "cursor", "fields", "limit",
        ],
    ),
    ("/v1/lookup", &["actor", "source", "value"]),
    ("/v1/availability", &["actor", "id"]),
    (
        "/v1/available",
        &["actor", "type", "transition", "cursor", "limit"],
    ),
    ("/v1/history", &["actor", "id", "follow"]),
    ("/v1/events", &["actor", "positions"]),
    ("/v1/exceptions", &["actor", "type", "cursor"]),
    ("/v1/declaration", &["actor", "type", "version"]),
    ("/v1/tools", &["actor", "actor_kind"]),
    ("/v1/pull", &["actor", "subscription", "cursor", "limit"]),
    ("/v1/acknowledge", &["actor", "subscription", "cursor"]),
    (
        "/v1/metric",
        &["actor", "name", "bind", "keep", "over", "filter", "cursor"],
    ),
    ("/v1/export", &["actor", "source", "cursor", "limit"]),
    ("/v1/diagnostics", &["actor", "type", "window"]),
    (
        "/v1/import_batch",
        &["actor", "batch", "disposition", "dry_run"],
    ),
    ("/v1/maintain", &["actor", "task"]),
    ("/v1/upgrade", &["actor", "dry_run"]),
    (
        "/v1/publish",
        &["actor", "change", "expected_version", "dry_run"],
    ),
    (
        "/sandbox/load",
        &["files", "attempt_retention", "idempotency_retention"],
    ),
    ("/sandbox/reset", &[]),
    ("/sandbox/clock", &["advance", "set"]),
    ("/sandbox/evaluators", &["answers"]),
];
const REQUEST_KEYS: &[&str] = &[
    "transition",
    "actor",
    "object_id",
    "type",
    "inputs",
    "occurred_at",
    "propose",
    "expires_at",
    "expected_version",
    "principal",
    "idempotency_key",
    "context",
];

/// A request's body: a JSON object of the keys its operation takes, or a
/// fault saying what is wrong with it, as every fault is a JSON `fault`
/// (ADR-0150 decision 1, D555).
struct Body(J);

impl<S: Send + Sync> axum::extract::FromRequest<S> for Body {
    type Rejection = Fault;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Fault> {
        let path = req.uri().path().to_string();
        let body = match Json::<J>::from_request(req, state).await {
            Ok(Json(J::Object(m))) => m,
            Ok(_) => return Err(Fault::Bad("the body is a JSON object".to_string())),
            Err(e) => return Err(Fault::Bad(e.body_text())),
        };
        if let Some((_, takes)) = TAKES.iter().find(|(p, _)| *p == path)
            && let Some(k) = body.keys().find(|k| !takes.contains(&k.as_str()))
        {
            return Err(Fault::Bad(format!("{path} takes no key {k}")));
        }
        Ok(Body(J::Object(body)))
    }
}

fn locked(shared: &Shared) -> Result<Writing<'_>, Fault> {
    Ok(Writing {
        guard: shared
            .writer
            .lock()
            .map_err(|_| Fault::Internal("a previous request panicked".to_string()))?,
        reads: shared.reads.as_ref(),
    })
}

fn store(sandbox: &mut Sandbox) -> Result<&mut Store, Fault> {
    if let Some(at) = &sandbox.due {
        return Err(due(at));
    }
    let deployment = sandbox.deployment;
    let endpoints = &sandbox.endpoints;
    let s = sandbox
        .store
        .as_mut()
        .ok_or_else(|| Fault::Conflict("no modules are loaded; load them first".to_string()))?;
    if deployment {
        // the wall clock, never moving backwards (ADR-0174 decision 4), and
        // an evaluator no endpoint answers unknown (ADR-0175 decision 5)
        s.now = s.now.max(wall_clock());
        for f in s.lib.evaluators.clone() {
            let namespace = f.split_once('.').map_or(f.as_str(), |(n, _)| n);
            if !endpoints.contains_key(namespace) {
                s.answers
                    .entry(f)
                    .or_insert_with(|| "unanswered".to_string());
            }
        }
    }
    Ok(s)
}

/// A flag of a request's body: absent or null is false, and anything but
/// a JSON boolean is refused, never read as false, since `"dry_run":
/// "true"` read as false installed a version (production review F6).
fn flag(body: &J, key: &str) -> Result<bool, Fault> {
    match body.get(key) {
        None | Some(J::Null) => Ok(false),
        Some(J::Bool(b)) => Ok(*b),
        Some(_) => Err(Fault::Bad(format!("{key} takes true or false"))),
    }
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
            .map(|f| {
                json!({"clause": f.clause, "remedy": remedy(f.remedy),
                            "unknown": f.unknown, "objects": f.objects})
            })
            .collect(),
    )
}

/// A verdict on the wire with what its refusal tells beside it, where it
/// is known (`library-api.md` §4): an unsatisfied clause unknown or false,
/// the objects its remedy points at, whether a proposal would be accepted
/// and what it consulted; an invariant violation's conflicting objects.
fn verdict_told(v: &Verdict, told: Option<&Told>) -> J {
    let mut out = json!({ "kind": v.kind(), "remedy": remedy(v.remedy()) });
    let fields = match v {
        Verdict::Satisfied {
            object_id,
            version,
            events,
            flags: fs,
            replayed,
        } => {
            json!({"object_id": object_id, "version": version, "events": events, "replayed": replayed, "flags": flags(fs)})
        }
        Verdict::Unsatisfied { clause, .. } => match told {
            Some(t) => {
                let mut f = json!({"clause": clause, "unknown": t.unknown, "objects": t.objects,
                                   "proposable": t.proposable});
                match &t.consulted {
                    Some(c) => f["consulted"] = c.clone(),
                    None => f["not_computed"] = json!(["consulted"]),
                }
                f
            }
            None => {
                json!({"clause": clause, "not_computed": ["unknown", "objects", "proposable", "consulted"]})
            }
        },
        Verdict::NotFound => json!({}),
        Verdict::Stale { expected, actual } => {
            json!({ "cause": "expected_version", "expected": expected, "actual": actual })
        }
        // a race the lock's retries could not win: neither version
        // (library-api.md §4, ADR-0178)
        Verdict::RetriesExhausted => json!({ "cause": "retries_exhausted" }),
        Verdict::Unavailable { state } => json!({ "state": state }),
        Verdict::UnknownTransition { transition } => json!({ "transition": transition }),
        Verdict::InvalidInput { inputs, reasons } => json!({
            "inputs": inputs,
            // why each input is at fault (ADR-0173 decision 9)
            "reasons": inputs.iter().zip(reasons).map(|(i, r)| (i.clone(), json!(r.as_str())))
                .collect::<serde_json::Map<String, serde_json::Value>>(),
        }),
        Verdict::NotRequestable { parents } => json!({ "parents": parents }),
        Verdict::NotProposable => json!({}),
        Verdict::CallRefused {
            step,
            object_id,
            verdict: inner,
            clause,
        } => match told {
            // the call's own refusal whole, its clause qualified by the type
            // it targeted, as an expectation names it (flow-format.md §11.4)
            Some(t) => {
                let mut called = verdict_told(inner, Some(t));
                if let Some(o) = called.as_object_mut() {
                    o.insert("clause".into(), json!(clause));
                    // a call is no request a caller files a proposal of
                    if o.contains_key("proposable") {
                        o.insert("proposable".into(), json!(false));
                    }
                }
                json!({"step": step, "object_id": object_id, "verdict": called})
            }
            None => json!({
                "step": step, "object_id": object_id,
                "verdict": {"kind": inner.kind(), "clause": clause, "remedy": remedy(inner.remedy())},
                "not_computed": ["the call's verdict beyond its clause"],
            }),
        },
        Verdict::OverLimit { loop_, bound } => json!({ "loop": loop_, "bound": bound }),
        Verdict::InvariantViolated { failures, .. } => {
            let mut f = json!({
                "failures": failures.iter().map(|f| json!({"object_id": f.object_id, "type": f.type_,
                    "invariant": f.invariant, "remedy": remedy(f.remedy)})).collect::<Vec<_>>(),
            });
            match told {
                Some(t) => {
                    f["objects"] = json!(t.objects);
                    // whether the transition accepts a proposal, as a
                    // `taken_by` refusal's does (D582)
                    f["proposable"] = json!(t.proposable);
                }
                None => f["not_computed"] = json!(["objects", "proposable"]),
            }
            f
        }
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
    // what its guards consulted, the verdict or value each was given by the
    // reference with its bindings, and its as-of time, each by guard, as the
    // library's `Event` carries them (`library-api.md` §5, D562)
    let mut payload = e.payload.clone();
    let (mut consulted, mut as_of) = (Map::new(), Map::new());
    if let Some(J::Object(rules)) = payload.as_object_mut().and_then(|p| p.remove("consulted")) {
        for (rule, c) in rules {
            if let Some(at) = c.get("as_of") {
                as_of.insert(rule.clone(), at.clone());
            }
            consulted.insert(rule, c.get("values").cloned().unwrap_or(J::Null));
        }
    }
    json!({
        "position": e.position, "object_id": e.object_id, "transition": e.transition,
        "from_state": e.from_state, "to_state": e.to_state, "changes_state": e.changes_state,
        "actor_id": e.actor_id, "actor_kind": e.actor_kind, "principal": e.principal,
        "declaration_version": e.declaration_version,
        "recorded_at": timestamp_text(e.recorded_at),
        "occurred_at": e.occurred_at.map(timestamp_text), "imported": e.imported,
        "payload": payload, "cause": e.cause, "reads": e.reads, "context": e.context,
        "consulted": consulted, "as_of": as_of,
        // the serialisation retries its request took (ADR-0178 decision 2)
        "retries": e.retries,
    })
}

const EVENT_NOT_COMPUTED: [&str; 5] = ["object_seq", "source", "taint_version", "txn", "flags"];

fn offer(o: &Offer, actor_dependent: bool) -> J {
    let inputs: Map<String, J> = o
        .inputs
        .iter()
        .map(|(k, t)| (k.clone(), json!(t)))
        .collect();
    json!({
        "transition": o.transition, "creates": o.creates, "availability": o.availability.text(),
        "verdict": o.verdict.as_ref().map(|v| verdict_told(v, Some(&o.told))), "inputs": inputs,
        "unevaluated": o.unevaluated, "flags": flags(&o.flags),
        // it may be refused for who takes it (ADR-0173 decision 1)
        "actor_dependent": actor_dependent,
    })
}

// ── the sandbox's controls ────────────────────────────────────────────────
async fn load(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        // the deployment's attempt and idempotency retentions, durations
        let retention = |k: &str| -> Result<Option<i64>, Fault> {
            match text(&body, k) {
                None => Ok(None),
                Some(d) => parse_duration(d)
                    .map(Some)
                    .ok_or_else(|| Fault::Bad(format!("{k} takes a duration such as '90 days'"))),
            }
        };
        let retentions = (
            retention("attempt_retention")?,
            retention("idempotency_retention")?,
        );
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
        // the types the load installed, as the reference runner lists them
        let answer = json!({"loaded": true, "modules": lib.modules, "notices": 0, "now": CLOCK_START,
                        "types": lib.declared, "evaluators": lib.evaluators});
        let now = parse_timestamp(CLOCK_START).unwrap_or(0);
        let mut store = Store::sandbox(lib, now)?;
        (store.attempt_retention, store.idempotency_retention) = retentions;
        let mut sandbox = locked(&shared)?;
        sandbox.files = files;
        sandbox.retentions = retentions;
        sandbox.store = Some(store);
        sandbox.answers = Map::new();
        Ok(Json(answer))
    })
}

async fn reset(State(shared): State<Shared>) -> Answer {
    tokio::task::block_in_place(move || {
        let mut sandbox = locked(&shared)?;
        if sandbox.files.is_empty() {
            return Err(Fault::Conflict(
                "no modules are loaded; load them first".to_string(),
            ));
        }
        let lib = Library::load(&sandbox.files).map_err(|e| Fault::Internal(e.to_string()))?;
        let now = parse_timestamp(CLOCK_START).unwrap_or(0);
        let mut store = Store::sandbox(lib, now)?;
        (store.attempt_retention, store.idempotency_retention) = sandbox.retentions;
        sandbox.store = Some(store);
        sandbox.answers = Map::new();
        Ok(Json(json!({ "now": CLOCK_START })))
    })
}

async fn clock(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let mut sandbox = locked(&shared)?;
        let s = store(&mut sandbox)?;
        let target = if let Some(by) = text(&body, "advance") {
            let step = parse_duration(by).ok_or_else(|| {
                Fault::Bad(format!("advance takes a duration such as '1 h': {by}"))
            })?;
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
    })
}

async fn evaluators(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
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
    })
}

// ── the library's operations ──────────────────────────────────────────────
async fn request_op(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let req = request_of(&body)?;
        // a change's dry run, which a `submit` or `refresh` records, computed
        // on a read beside the writes, so no write waits for it; the request
        // takes it only where nothing it read has changed (DRYRUN-LOCK)
        let prepared = match (&req.object_id, req.transition.as_str()) {
            (Some(id), "submit" | "refresh") if shared.reads.is_some() => {
                let mut r = reading(&shared)?;
                let s = r.store()?;
                // a read that cannot prepare it leaves it to the request,
                // which computes it under the lock and says why it cannot
                objectflow_core::publish::prepare(&s.snapshot()?, id, &req.actor).unwrap_or(None)
            }
            _ => None,
        };
        let (v, told) = consulting(
            &shared,
            false,
            |s| {
                s.prepared = prepared.clone().map(Box::new);
                Ok(request::request_told(s, &req)?)
            },
            |out| match out {
                Err(request::NoVerdict::NotRun(NotRun::Unanswered(c))) => Some((**c).clone()),
                _ => None,
            },
        )??;
        Ok(Json(verdict_told(&v, Some(&told))))
    })
}

/// The verdict a request would receive, simulated and never written, with
/// the guards it did not evaluate (`library-api.md` §6, ADR-0054).
async fn check(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let req = request_of(&body)?;
        if req.propose {
            return Err(Fault::Bad(
                "a check simulates a request, not a proposal".to_string(),
            ));
        }
        let c = consulting(
            &shared,
            true,
            |s| Ok(request::check(s, &req)?),
            |out| match out {
                Err(NotRun::Unanswered(c)) => Some((**c).clone()),
                _ => None,
            },
        )??;
        Ok(Json(
            json!({ "verdict": verdict_told(&c.verdict, Some(&c.told)), "unevaluated": c.unevaluated }),
        ))
    })
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
async fn pull(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let (Some(_actor), Some(subscription)) = (
            body.get("actor").and_then(|a| text(a, "id")),
            text(&body, "subscription"),
        ) else {
            return Err(Fault::Bad(
                "pull names its actor and the subscription".to_string(),
            ));
        };
        let cursor = cursor_of(&body)?;
        let limit = limit_of(&body)?;
        let mut r = reading(&shared)?;
        let s = r.store()?;
        let world = s.snapshot()?;
        let page = objectflow_core::subscription::pull_page(&world, subscription, cursor, limit)?;
        Ok(Json(json!({
            "events": page.events.iter().map(event).collect::<Vec<_>>(),
            "settled": page.settled.to_string(),
            "refused": page.refused,
            "not_computed": EVENT_NOT_COMPUTED,
        })))
    })
}

/// A page of objects as the wire gives it: each object's id, type and
/// fields, the cursor of the next page, none where this is the last, or a
/// refusal, a value.
fn page_json(page: &reads::QueryPage) -> J {
    json!({
        "items": page.items.iter().map(|i| json!({
            "id": i.id, "type": i.type_,
            "fields": J::Object(i.fields.iter().map(|(f, v)| (f.clone(), as_held(&i.type_, f, encode(v)))).collect()),
        })).collect::<Vec<_>>(),
        "cursor": page.cursor,
        "refused": page.refused,
    })
}

/// A page's size as a read is asked for it: 1 to `PAGE_LIMIT`, and
/// `PAGE_DEFAULT` where none is given (ADR-0179).
fn limit_of(body: &J) -> Result<usize, Fault> {
    match body.get("limit") {
        None | Some(J::Null) => Ok(reads::PAGE_DEFAULT),
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| (1..=reads::PAGE_LIMIT).contains(n))
            .ok_or_else(|| {
                Fault::Bad(format!(
                    "limit takes a page size, 1 to {}",
                    reads::PAGE_LIMIT
                ))
            }),
    }
}

/// The objects an assertion put in their state, the import's excepted, and
/// those holding an admitted violation (`DESIGN.md` §10).
async fn exceptions(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let (Some(_actor), Some(type_)) = (
            body.get("actor").and_then(|a| text(a, "id")),
            text(&body, "type"),
        ) else {
            return Err(Fault::Bad(
                "exceptions names its actor and the type".to_string(),
            ));
        };
        let mut r = reading(&shared)?;
        let s = r.store()?;
        let world = s.snapshot()?;
        Ok(Json(page_json(&reads::exceptions(&world, type_)?)))
    })
}

/// The objects of a type a transition is offered to, asking no evaluator
/// (`DESIGN.md` §10, ADR-0048).
async fn available(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let (Some(actor), Some(type_), Some(transition)) = (
            body.get("actor").and_then(|a| text(a, "id")),
            text(&body, "type"),
            text(&body, "transition"),
        ) else {
            return Err(Fault::Bad(
                "available names its actor, the type and the transition".to_string(),
            ));
        };
        let mut r = reading(&shared)?;
        let s = r.store()?;
        let world = s.snapshot()?;
        let mut page = page_json(&reads::available_page(
            &world,
            actor,
            type_,
            transition,
            text(&body, "cursor"),
            limit_of(&body)?,
        )?);
        // a row is swept on the state and the guards, so one the transition may
        // refuse for who takes it says so, rather than each row simulated
        // (ADR-0176 decision 2)
        let dependent = s
            .lib
            .type_(type_)
            .and_then(|t| t.transition(transition).map(|x| (t, x)))
            .is_some_and(|(t, x)| objectflow_core::request::actor_dependent(&s.lib, t, x));
        page["actor_dependent"] = json!(dependent);
        Ok(Json(page))
    })
}

/// Where a type's flow and reality disagree, over a window (`DESIGN.md`
/// §10, ADR-0160); a type it does not declare is refused, a value.
async fn diagnostics(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let (Some(_actor), Some(type_), Some(window)) = (
            body.get("actor").and_then(|a| text(a, "id")),
            text(&body, "type"),
            text(&body, "window"),
        ) else {
            return Err(Fault::Bad(
                "diagnostics names its actor, the type and a window".to_string(),
            ));
        };
        let window = parse_duration(window).ok_or_else(|| {
            Fault::Bad(format!(
                "window takes a duration such as '30 days': {window}"
            ))
        })?;
        let mut r = reading(&shared)?;
        let s = r.store()?;
        let world = s.snapshot()?;
        Ok(Json(
            match objectflow_core::diagnostics::diagnostics(&world, type_, window)? {
                Ok(found) => json!({
                    "diagnostics": found.iter().map(objectflow_core::diagnostics::Diagnostic::json).collect::<Vec<_>>(),
                    "refused": J::Null,
                }),
                Err(why) => json!({ "diagnostics": [], "refused": why }),
            },
        ))
    })
}

/// A maintenance task run outside the retention the store was built with:
/// the rows it moved or removed, or its refusal, a value (`DESIGN.md` §10).
async fn maintain(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let task = body.get("task");
        let (Some(actor), Some(name), Some(before)) = (
            body.get("actor").and_then(|a| text(a, "id")),
            task.and_then(|t| text(t, "name")),
            task.and_then(|t| text(t, "before")),
        ) else {
            return Err(Fault::Bad(
                "maintain names its actor and a task, its name and a time before which it prunes"
                    .to_string(),
            ));
        };
        let before = parse_timestamp(before)
            .ok_or_else(|| Fault::Bad(format!("before takes a timestamp: {before}")))?;
        let mut sandbox = locked(&shared)?;
        let s = store(&mut sandbox)?;
        let task = objectflow_core::store::MaintenanceTask {
            name: name.to_string(),
            before,
        };
        Ok(Json(match s.maintain(actor, &task)?? {
            Ok(moved) => json!({ "moved": moved, "refused": J::Null }),
            Err(why) => json!({ "moved": 0, "refused": why }),
        }))
    })
}

/// A source's rows after a cursor, as JSON lines, every personal value
/// omitted; a refusal is a value (`library-api.md` §6, ADR-0094).
async fn export(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let (Some(_actor), Some(source)) = (
            body.get("actor").and_then(|a| text(a, "id")),
            text(&body, "source"),
        ) else {
            return Err(Fault::Bad(
                "export names its actor and the source".to_string(),
            ));
        };
        let mut r = reading(&shared)?;
        let s = r.store()?;
        let world = s.snapshot()?;
        let page = objectflow_core::export::export_page(
            &world,
            source,
            text(&body, "cursor"),
            limit_of(&body)?,
        )?;
        Ok(Json(json!({
            "lines": page.lines,
            "cursor": page.cursor,
            "refused": page.refused,
        })))
    })
}

/// A subscription's progress moved, back as well as forward; a refusal is
/// a value (`library-api.md` §6).
async fn acknowledge(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
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
    })
}

/// A request as the wire carries it (`library-api.md` §3).
fn request_of(body: &J) -> Result<Request, Fault> {
    // the key that makes a retry safe, scoped to the actor (library-api.md §3)
    let idempotency_key = match body.get("idempotency_key") {
        None | Some(J::Null) => None,
        Some(J::String(k)) if !k.is_empty() => Some(k.clone()),
        Some(_) => {
            return Err(Fault::Bad(
                "idempotency_key takes a non-empty string".to_string(),
            ));
        }
    };
    // the version of its object the caller read (library-api.md §3)
    let expected_version = match body.get("expected_version") {
        None | Some(J::Null) => None,
        Some(v) => Some(v.as_i64().ok_or_else(|| {
            Fault::Bad("expected_version takes an object's version, an integer".to_string())
        })?),
    };
    let propose = flag(body, "propose")?;
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
    // the transition's inputs by name; anything but an object is refused,
    // never read as none (production review F6)
    let inputs = match body.get("inputs") {
        None | Some(J::Null) => Default::default(),
        Some(J::Object(m)) => m.clone().into_iter().collect(),
        Some(_) => {
            return Err(Fault::Bad(
                "inputs takes an object of the transition's inputs by name".to_string(),
            ));
        }
    };
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
        idempotency_key,
        // why the request was sent and by which route (library-api.md §3)
        context: match body.get("context") {
            None | Some(J::Null) => None,
            Some(J::String(c)) => Some(c.clone()),
            Some(_) => return Err(Fault::Bad("context takes a string".to_string())),
        },
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
    // its history from before the store (ADR-0163 decision 2)
    let mut legacy_entries = Vec::new();
    for e in o
        .get("legacy_entries")
        .and_then(J::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(kind), Some(occurred_at)) = (
            text(e, "kind").map(str::to_string),
            instant(e.get("occurred_at"), "legacy_entries.occurred_at")?,
        ) else {
            return Err(Fault::Bad(format!(
                "a legacy entry of {type_} {legacy_key} names its kind and occurred_at"
            )));
        };
        let payload = e
            .get("payload")
            .and_then(J::as_object)
            .cloned()
            .unwrap_or_default();
        let mut about = std::collections::BTreeMap::new();
        for (field, target) in e.get("about").and_then(J::as_object).into_iter().flatten() {
            let Some(target) = target.as_str() else {
                return Err(Fault::Bad(format!(
                    "a legacy entry's about maps {field} to <Type>:<key>"
                )));
            };
            about.insert(field.clone(), target.to_string());
        }
        legacy_entries.push(import::EntryIn {
            kind,
            occurred_at,
            payload,
            about,
        });
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
        occurred_at: instant(o.get("occurred_at"), "occurred_at")?,
        legacy_entries,
        // everything a batch carries is imported (ADR-0154 decision 6)
        beyond: Vec::new(),
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

async fn import_op(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
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
        // each entry kind's fields kept through an erasure (ADR-0163 decision 3)
        for (kind, fields) in b
            .get("legacy_kinds")
            .and_then(J::as_object)
            .into_iter()
            .flatten()
        {
            let fields = fields
                .as_array()
                .and_then(|a| {
                    a.iter()
                        .map(|f| f.as_str().map(str::to_string))
                        .collect::<Option<Vec<_>>>()
                })
                .ok_or_else(|| {
                    Fault::Bad(format!(
                        "legacy_kinds.{kind} lists the field names it keeps"
                    ))
                })?;
            batch.legacy_kinds.insert(kind.clone(), fields);
        }
        for r in b
            .get("sequences")
            .and_then(J::as_array)
            .into_iter()
            .flatten()
        {
            let (Some(sequence), Some(last)) =
                (text(r, "sequence"), r.get("last").and_then(J::as_i64))
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
        let dry_run = flag(&body, "dry_run")?;
        let mut sandbox = locked(&shared)?;
        let s = store(&mut sandbox)?;
        let r = import::import_batch(s, actor, &batch, &file, dry_run)??;
        Ok(Json(report(&r)))
    })
}

/// An object as `get` reads it, or null where the store holds none.
/// The built-in attributes that hold JSON, kept as text: the wire answers
/// each as the JSON it holds, as its input was written (ADR-0182 decision
/// 9).
const HELD_AS_JSON: &[(&str, &str)] = &[
    ("DeclarationChange", "source"),
    ("DeclarationChange", "report"),
    ("DeclarationChange", "evidence"),
    ("Proposal", "inputs"),
];

/// An attribute's value as the wire answers it: one `HELD_AS_JSON` names
/// as the JSON its text holds, any other as encoded.
fn as_held(type_: &str, name: &str, encoded: J) -> J {
    match encoded {
        J::String(text) if HELD_AS_JSON.contains(&(type_, name)) => {
            serde_json::from_str(&text).unwrap_or(J::String(text))
        }
        other => other,
    }
}

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
    // each as a reader reads it, an end the other side stores by its
    // members, so a set is never null (`flow-format.md` §4.3, D552)
    let reader = objectflow_core::eval::Evaluator::new(&world, Some(t), Some(&o.id));
    for (n, _) in &t.attributes {
        attributes.insert(
            n.clone(),
            as_held(&o.type_, n, encode(&reader.attribute(o, n)?)),
        );
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

async fn get(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        // the argument it needs, a fault and never an absent object (D570)
        let id =
            text(&body, "id").ok_or_else(|| Fault::Bad("get names an object's id".to_string()))?;
        let mut r = reading(&shared)?;
        let s = r.store()?;
        object_json(s, id)
    })
}

/// A change's dry run, its report the impact a publish would have, or its
/// approval, which installs it (`library-api.md` §6, ADR-0155).
async fn publish(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let (Some(actor), Some(change)) = (
            body.get("actor").and_then(|a| text(a, "id")),
            text(&body, "change"),
        ) else {
            return Err(Fault::Bad(
                "publish names its actor and the change".to_string(),
            ));
        };
        let dry_run = flag(&body, "dry_run")?;
        let mut sandbox = locked(&shared)?;
        let s = store(&mut sandbox)?;
        if !dry_run {
            // the version of the change the approver read, required (ADR-0101)
            let Some(expected) = body.get("expected_version").and_then(J::as_i64) else {
                return Err(Fault::Bad(
                    "a publish names the expected_version of the change it approves".to_string(),
                ));
            };
            let (v, report) = s.publish(actor, change, expected)??;
            // a publish's own refusal, current, fatal, impact_unchanged or
            // decision_unresolved, names no object and takes no proposal
            let told = Told::default();
            return Ok(Json(json!({
                "verdict": verdict_told(&v, Some(&told)),
                "report": report.as_ref().map(objectflow_core::publish::report_json),
            })));
        }
        let world = s.snapshot()?;
        let Some(c) = world
            .get(change)?
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
    })
}

/// The object whose external identifier for a source has a value, a legacy
/// key among them (`library-api.md` §6, ADR-0154).
async fn lookup(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let (Some(source), Some(value)) = (text(&body, "source"), text(&body, "value")) else {
            return Err(Fault::Bad("lookup names a source and a value".to_string()));
        };
        let mut r = reading(&shared)?;
        let s = r.store()?;
        let world = s.snapshot()?;
        match world.external_id(source, value)?.as_ref() {
            Some(id) => object_json(s, &id.clone()),
            None => Ok(Json(J::Null)),
        }
    })
}

async fn history(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let id = text(&body, "id")
            .ok_or_else(|| Fault::Bad("history names an object's id".to_string()))?;
        let mut r = reading(&shared)?;
        let s = r.store()?;
        // an object that does not exist has no history, null as get's (ADR-0173)
        if reads::get(&s.snapshot()?, id)?.is_none() {
            return Ok(Json(J::Null));
        }
        let events = s.history(id)?;
        // its legacy entries first, which predate the store, then its events
        // (library-api.md §6, ADR-0015)
        let entries = s.legacy_entries(id)?.into_iter().map(|e| {
            json!({"legacy": true, "object_id": id, "ordinal": e.ordinal, "kind": e.kind,
               "occurred_at": timestamp_text(e.occurred_at), "payload": e.payload})
        });
        Ok(Json(json!({
            "events": entries.chain(events.iter().map(event)).collect::<Vec<_>>(),
            "not_computed": EVENT_NOT_COMPUTED,
        })))
    })
}

async fn events(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let Some(positions) = body
            .get("positions")
            .and_then(J::as_array)
            .and_then(|ps| ps.iter().map(J::as_i64).collect::<Option<Vec<i64>>>())
        else {
            return Err(Fault::Bad(
                "events takes positions, a list of integers".to_string(),
            ));
        };
        let mut r = reading(&shared)?;
        let s = r.store()?;
        let found = s.events(&positions)?;
        Ok(Json(json!({
            "events": found.iter().map(|e| e.as_ref().map_or(J::Null, event)).collect::<Vec<_>>(),
            "not_computed": EVENT_NOT_COMPUTED,
        })))
    })
}

/// The tool schemas an agent of a kind is handed, rendered from the
/// installed version's declaration (`renderers.md` §3, ADR-0173 decision 10).
async fn tools(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let kind = text(&body, "actor_kind")
            .filter(|k| objectflow_core::tools::ACTOR_KINDS.contains(k))
            .ok_or_else(|| {
                Fault::Bad("tools names an actor_kind: human, agent or service".to_string())
            })?
            .to_string();
        let mut r = reading(&shared)?;
        let s = r.store()?;
        Ok(Json(
            json!({"tools": objectflow_core::tools::tools(&s.lib, &kind)}),
        ))
    })
}

async fn declaration(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let mut r = reading(&shared)?;
        let s = r.store()?;
        let name = text(&body, "type").unwrap_or_default();
        // the version asked for, the installed one where none is
        // (library-api.md §6)
        let earlier = match body.get("version") {
            None | Some(J::Null) => None,
            Some(v) => {
                let n = v.as_i64().ok_or_else(|| {
                    Fault::Bad("version takes a declaration version, an integer".to_string())
                })?;
                Some(
                    s.library_at(n)?
                        .ok_or_else(|| Fault::Missing(format!("no version {n} was published")))?,
                )
            }
        };
        let lib = earlier.as_ref().unwrap_or(&s.lib);
        let Some(t) = lib.type_(name) else {
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
                && let Some(members) = lib.enumerations.get(e)
            {
                used.insert(e.clone(), json!(members));
            }
        }
        // the version read, where its source came from and its digest, neither
        // for version 0 (ADR-0135), and the metric dimensions that name an
        // actor, for an upper layer's door (library-api.md §6, D451)
        let version = lib.version;
        let (source_ref, source_digest) = if version == 0 {
            (None, None)
        } else {
            s.declaration_source(version)?.unwrap_or((None, None))
        };
        let person = objectflow_core::metric::person_dimensions(lib, name)?;
        if let Some(o) = out.as_object_mut() {
            o.insert("enumerations".to_string(), J::Object(used));
            o.insert("version".to_string(), json!(version));
            o.insert("source_ref".to_string(), json!(source_ref));
            o.insert("source_digest".to_string(), json!(source_digest));
            o.insert("person_dimensions".to_string(), json!(person));
        }
        Ok(Json(out))
    })
}

async fn availability(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let (Some(actor), Some(id)) = (
            body.get("actor").and_then(|a| text(a, "id")),
            text(&body, "id"),
        ) else {
            return Err(Fault::Bad(
                "availability names its actor and an object's id".to_string(),
            ));
        };
        let mut r = reading(&shared)?;
        let s = r.store()?;
        let decided = {
            let world = s.snapshot()?;
            let t = reads::get(&world, id)?.and_then(|v| s.lib.type_(&v.object.type_));
            let dependent = |o: &Offer| {
                o.creates.is_none()
                    && t.and_then(|t| t.transition(&o.transition).map(|x| (t, x)))
                        .is_some_and(|(t, x)| {
                            objectflow_core::request::actor_dependent(&s.lib, t, x)
                        })
            };
            reads::availability(&world, actor, id)?.map(|offers| {
                offers
                    .into_iter()
                    .map(|o| {
                        let d = dependent(&o);
                        (o, d)
                    })
                    .collect::<Vec<_>>()
            })
        };
        // an object that does not exist offers nothing, null as get's (ADR-0173)
        let Some(decided) = decided else {
            return Ok(Json(J::Null));
        };
        let mut out = Vec::new();
        for (mut o, dependent) in decided {
            // one that may be refused for who takes it is decided for who asks,
            // where it takes no input and asks no evaluator: the request with no
            // inputs simulated whole, as `check` does, every evaluator unasked,
            // and a simulation that would ask one leaves it as decided (ADR-0176
            // decision 2)
            if dependent && o.availability == Availability::Available && o.unevaluated.is_empty() {
                let req = request_of(
                    &json!({"transition": o.transition, "actor": {"id": actor}, "object_id": id}),
                )?;
                let given = std::mem::take(&mut s.answers);
                let checked = request::check(s, &req);
                s.answers = given;
                if let Ok(c) = checked?
                    && !matches!(c.verdict, Verdict::Satisfied { .. })
                {
                    o.availability = Availability::Unavailable;
                    o.verdict = Some(c.verdict);
                    // what it tells, but nothing consulted, which an offer
                    // never reports (ADR-0048)
                    o.told = Told {
                        consulted: None,
                        ..c.told
                    };
                    o.flags = Vec::new();
                }
            }
            out.push(offer(&o, dependent));
        }
        Ok(Json(J::Array(out)))
    })
}

/// A reader's read of a metric: one page holding every group, so its
/// cursor is none (`library-api.md` §5, `MetricPage`).
/// A page of objects of a type or its family, filtered and ordered
/// (`library-api.md` §6, ADR-0127): every object the filter holds for, on
/// one page, so no cursor; a refusal is a value.
async fn query(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let (Some(_actor), Some(type_)) = (
            body.get("actor").and_then(|a| text(a, "id")),
            text(&body, "type"),
        ) else {
            return Err(Fault::Bad("query names its actor and the type".to_string()));
        };
        let limit = limit_of(&body)?;
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
        let mut r = reading(&shared)?;
        let s = r.store()?;
        let world = s.snapshot()?;
        let page = reads::query_page(
            &world,
            type_,
            text(&body, "filter"),
            &order,
            fields.as_deref(),
            text(&body, "cursor"),
            limit,
        )?;
        Ok(Json(page_json(&page)))
    })
}

async fn metric(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
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
                "a metric's page holds every group, so it has no cursor to continue from"
                    .to_string(),
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
        let over = match text(&body, "over") {
            None => None,
            Some(by) => Some(parse_duration(by).ok_or_else(|| {
                Fault::Bad(format!("over takes a duration such as '30 days': {by}"))
            })?),
        };
        let mut r = reading(&shared)?;
        let s = r.store()?;
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
        let mut out = objectflow_core::metric::page_json(&page);
        if let Some(o) = out.as_object_mut() {
            o.insert("cursor".to_string(), J::Null);
        }
        Ok(Json(out))
    })
}

/// `GET /health`: the release, the store's version and its clock (ADR-0174
/// decision 6).
async fn health(State(shared): State<Shared>) -> Answer {
    tokio::task::block_in_place(move || {
        // a deployment's health is read beside its writes, never behind one
        // (ADR-0178), from what the writer last held
        if let Some(reads) = &shared.reads {
            let seen = reads
                .seen
                .read()
                .map_err(|_| Fault::Internal("a previous request panicked".to_string()))?
                .clone();
            if let Some(at) = seen.due {
                return Ok(Json(
                    json!({"release": BUILTINS, "shape": "deployment", "version": J::Null,
                                  "now": J::Null, "store_release": at, "upgrade_due": true}),
                ));
            }
            // the store read, so a store that cannot be read answers 500
            // here rather than at the next request (production review
            // OBSERVE): its file opened afresh, which a read connection,
            // whose pages stay cached while no write moves the log, never
            // rereads, and its release found to be the one served
            match objectflow_core::release::store_release_at(&reads.path) {
                Ok(Some(r)) if r == BUILTINS => {}
                Ok(at) => {
                    return Err(Fault::Internal(format!(
                        "storage unavailable: {} holds {}",
                        reads.path.display(),
                        at.map_or("no store".to_string(), |r| format!("a store at {r}"))
                    )));
                }
                Err(e) => {
                    return Err(Fault::Internal(format!(
                        "storage unavailable: {}: {e}",
                        reads.path.display()
                    )));
                }
            }
            let mut r = reading(&shared)?;
            let s = r.store()?;
            let version = s.lib.version;
            s.snapshot()?.get(objectflow_core::model::OPERATOR)?;
            let now = s.now;
            drop(r);
            return Ok(Json(
                json!({"release": BUILTINS, "shape": "deployment", "version": version,
                              "now": timestamp_text(now), "store_release": BUILTINS,
                              "upgrade_due": false, "clock_lead_seconds": clock_lead(&shared)}),
            ));
        }
        let mut sandbox = locked(&shared)?;
        let shape = if sandbox.deployment {
            "deployment"
        } else {
            "sandbox"
        };
        // a store due an upgrade answers both releases and nothing it holds
        // (ADR-0177 decision 4)
        if let Some(at) = &sandbox.due {
            return Ok(Json(
                json!({"release": BUILTINS, "shape": shape, "version": J::Null,
                              "now": J::Null, "store_release": at, "upgrade_due": true}),
            ));
        }
        let (version, now) = match sandbox.store {
            Some(_) => {
                let s = store(&mut sandbox)?;
                (s.lib.version, J::from(timestamp_text(s.now)))
            }
            None => (0, J::Null),
        };
        Ok(Json(
            json!({"release": BUILTINS, "shape": shape, "version": version, "now": now,
               "store_release": BUILTINS, "upgrade_due": false, "clock_lead_seconds": 0}),
        ))
    })
}

/// `upgrade` (`storage-schema.md` §12, ADR-0177): a deployment's store
/// carried from its release to this core's, the operator's act, or with
/// `dry_run` every step run and rolled back; once upgraded the store is
/// opened and served. A store at this release already answers that
/// nothing is due; a sandbox's store is always this release's.
async fn upgrade(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let Some(actor) = body.get("actor").and_then(|a| text(a, "id")) else {
            return Err(Fault::Bad(
                "an upgrade names its actor, an operator of the store".to_string(),
            ));
        };
        let dry_run = flag(&body, "dry_run")?;
        let mut sandbox = locked(&shared)?;
        let Some(path) = sandbox.path.clone().filter(|_| sandbox.deployment) else {
            return Err(Fault::Bad(
                "a sandbox's store is this release's; only a deployment's is upgraded".to_string(),
            ));
        };
        if sandbox.due.is_none() {
            return Ok(Json(json!({"from": BUILTINS, "to": BUILTINS, "steps": [],
                              "dry_run": dry_run, "upgraded": false})));
        }
        let report = match objectflow_core::release::upgrade(&path, wall_clock(), actor, dry_run)? {
            Ok(r) => r,
            Err(why) => return Err(Fault::Conflict(why)),
        };
        if report.upgraded {
            let mut s = Store::open(&path, wall_clock())?;
            (s.attempt_retention, s.idempotency_retention) = sandbox.retentions;
            s.set_busy_timeout(sandbox.busy_timeout)?;
            sandbox.store = Some(s);
            sandbox.due = None;
        }
        let steps: Vec<J> = report
            .steps
            .iter()
            .map(|(release, changes)| json!({"release": release, "changes": changes}))
            .collect();
        Ok(Json(
            json!({"from": report.from, "to": report.to, "steps": steps,
                   "dry_run": report.dry_run, "upgraded": report.upgraded}),
        ))
    })
}

fn app(shared: Shared, deployment: bool) -> Router {
    let routes = Router::new()
        .route("/v1/request", post(request_op))
        .route("/v1/get", post(get))
        .route("/v1/history", post(history))
        .route("/v1/events", post(events))
        .route("/v1/declaration", post(declaration))
        .route("/v1/tools", post(tools))
        .route("/v1/availability", post(availability))
        .route("/v1/metric", post(metric))
        .route("/v1/query", post(query))
        .route("/v1/check", post(check))
        .route("/v1/pull", post(pull))
        .route("/v1/export", post(export))
        .route("/v1/maintain", post(maintain))
        .route("/v1/diagnostics", post(diagnostics))
        .route("/v1/exceptions", post(exceptions))
        .route("/v1/available", post(available))
        .route("/v1/acknowledge", post(acknowledge))
        .route("/v1/import_batch", post(import_op))
        .route("/v1/lookup", post(lookup))
        .route("/v1/publish", post(publish))
        .route("/v1/upgrade", post(upgrade))
        .route("/health", axum::routing::get(health))
        .route("/metrics", axum::routing::get(metrics));
    // a deployment's service routes none of the sandbox's controls, so no
    // caller can load, reset or move its clock (ADR-0174 decision 1)
    let routes = if deployment {
        routes
    } else {
        routes
            .route("/sandbox/load", post(load))
            .route("/sandbox/reset", post(reset))
            .route("/sandbox/clock", post(clock))
            .route("/sandbox/evaluators", post(evaluators))
    };
    routes
        // an operation that does not exist, or one asked by another method,
        // is a JSON fault as every other is (ADR-0150 decision 1, D555)
        .fallback(|uri: axum::http::Uri| async move {
            Fault::Missing(format!("no operation {}", uri.path()))
        })
        .method_not_allowed_fallback(|| async {
            Fault::Method("an operation is a POST".to_string())
        })
        .layer(axum::middleware::from_fn_with_state(shared.clone(), logged))
        .with_state(shared)
}

/// How far a deployment's clock leads the wall clock, in seconds: the
/// store's clock never moves back (ADR-0174 decision 4), so after the host's
/// clock ran ahead and came back every event is stamped at the latest time
/// it reached until the wall clock passes it (production review CLOCK-PIN).
fn clock_lead(shared: &Service) -> i64 {
    shared.reads.as_ref().map_or(0, |r| {
        (r.floor.load(Ordering::SeqCst) - wall_clock()).max(0)
    })
}

/// Each answer, logged as one line on stderr, its time, method, route,
/// status and duration, and the clock's lead where a deployment's has one;
/// and counted for `GET /metrics` (production review OBSERVE). A health
/// check that answers 200 and a scrape of the metrics are counted, not
/// logged.
async fn logged(
    State(shared): State<Shared>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let started = std::time::Instant::now();
    let method = req.method().clone();
    // the route, never the path asked, so a caller cannot grow the counts
    // by asking paths no route takes
    let route = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map_or("unrouted".to_string(), |m| m.as_str().to_string());
    let answer = next.run(req).await;
    let took = started.elapsed().as_secs_f64();
    let status = answer.status().as_u16();
    if let Ok(mut served) = shared.served.lock() {
        let row = served.entry((route.clone(), status)).or_insert((0, 0.0));
        row.0 += 1;
        row.1 += took;
    }
    let quiet = (route == "/health" && status == 200) || route == "/metrics";
    if !quiet {
        let lead = clock_lead(&shared);
        eprintln!(
            "{} {method} {route} {status} {:.1} ms{}",
            timestamp_text(wall_clock()),
            took * 1000.0,
            if lead > 0 {
                format!(" clock_lead={lead}s")
            } else {
                String::new()
            }
        );
    }
    answer
}

/// `GET /metrics`: what the service has answered, by route and status, and
/// its store's version and clock lead, in Prometheus's text format
/// (production review OBSERVE).
async fn metrics(State(shared): State<Shared>) -> Response {
    let mut out = String::from(
        "# HELP objectflow_requests_total Requests answered, by route and status.\n\
         # TYPE objectflow_requests_total counter\n",
    );
    let served = shared.served.lock().map(|s| s.clone()).unwrap_or_default();
    for ((route, status), (n, _)) in &served {
        out.push_str(&format!(
            "objectflow_requests_total{{route=\"{route}\",status=\"{status}\"}} {n}\n"
        ));
    }
    out.push_str(
        "# HELP objectflow_request_seconds_total Seconds spent answering, by route and status.\n\
         # TYPE objectflow_request_seconds_total counter\n",
    );
    for ((route, status), (_, secs)) in &served {
        out.push_str(&format!(
            "objectflow_request_seconds_total{{route=\"{route}\",status=\"{status}\"}} {secs:.6}\n"
        ));
    }
    let version = shared.reads.as_ref().and_then(|r| {
        r.seen
            .read()
            .ok()
            .and_then(|s| s.lib.as_ref().map(|(lib, _)| lib.version))
    });
    if let Some(v) = version {
        out.push_str(&format!(
            "# HELP objectflow_store_version The flow version the store runs.\n\
             # TYPE objectflow_store_version gauge\nobjectflow_store_version {v}\n"
        ));
    }
    out.push_str(&format!(
        "# HELP objectflow_clock_lead_seconds How far the store's clock leads the wall clock.\n\
         # TYPE objectflow_clock_lead_seconds gauge\nobjectflow_clock_lead_seconds {}\n",
        clock_lead(&shared)
    ));
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        out,
    )
        .into_response()
}

const USAGE: &str = "usage: objectflow-service [--port <n> | --bind <host:port> | --socket <path>]
                          [--store <file> [--create]
                                          [--attempt-retention <duration>]
                                          [--idempotency-retention <duration>]
                                          [--evaluator <name>=<url> ...]
                                          [--evaluator-timeout <duration>]
                                          [--busy-timeout <duration>]]

Serves the engine's operations, POST /v1/<operation>, GET /health and GET
/metrics, on 127.0.0.1 at port n (8712 unless given; 0 takes a free one), at
the address --bind names, or on the Unix socket --socket names, made 0660 so
its owner and group alone reach it (ADR-0183); it writes where it listens
to stderr once it answers, and then one line for each answer.

Without --store the store is a sandbox's, held in memory, with its controls,
POST /sandbox/<control>. With --store it is a deployment's: the SQLite file
named, which must hold a store unless --create is given, which makes one at
version 0, its files 0600, whose flow is published through the wire; the
wall clock; and no controls (ADR-0174, ADR-0183). Each --evaluator names
the endpoint a namespace's calls are posted to, answered within the timeout,
5 s unless given, and an evaluator no endpoint answers is unknown
(ADR-0175). Its reads run beside its writes, each on a connection of its
own, and its writes one at a time, each waiting out another process's
write lock for the busy timeout, 5 s unless given, before it is retried
and then refused stale (ADR-0178). It authenticates nothing: bind it beyond loopback only behind
the application that does (core/README.md).";

/// The command line: where to listen, and a deployment's store and
/// retentions where it names one.
struct Args {
    bind: String,
    /// A Unix socket listened on in the address's place (ADR-0183
    /// decision 4).
    socket: Option<std::path::PathBuf>,
    store: Option<std::path::PathBuf>,
    /// Whether a store is made where the path holds none (ADR-0183
    /// decision 3).
    create: bool,
    retentions: (Option<i64>, Option<i64>),
    endpoints: std::collections::BTreeMap<String, String>,
    evaluator_timeout: Option<std::time::Duration>,
    busy_timeout: Option<std::time::Duration>,
}

fn args() -> Result<Args, String> {
    let mut out = Args {
        bind: "127.0.0.1:8712".to_string(),
        socket: None,
        store: None,
        create: false,
        retentions: (None, None),
        endpoints: std::collections::BTreeMap::new(),
        evaluator_timeout: None,
        busy_timeout: None,
    };
    let mut retained = false;
    let mut addressed = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or_else(|| format!("{a} takes a value"));
        match a.as_str() {
            "--port" => {
                let port: u16 = value()?
                    .parse()
                    .map_err(|_| "--port takes a number".to_string())?;
                out.bind = format!("127.0.0.1:{port}");
                addressed = true;
            }
            "--bind" => {
                out.bind = value()?;
                addressed = true;
            }
            "--socket" => out.socket = Some(value()?.into()),
            "--store" => out.store = Some(value()?.into()),
            "--create" => out.create = true,
            "--attempt-retention" | "--idempotency-retention" => {
                let d = value()?;
                let kept = parse_duration(&d)
                    .ok_or_else(|| format!("{a} takes a duration such as '90 days': {d}"))?;
                if a == "--attempt-retention" {
                    out.retentions.0 = Some(kept);
                } else {
                    out.retentions.1 = Some(kept);
                }
                retained = true;
            }
            "--evaluator" => {
                let named = value()?;
                let (name, url) = named
                    .split_once('=')
                    .filter(|(n, u)| {
                        !n.is_empty() && (u.starts_with("http://") || u.starts_with("https://"))
                    })
                    .ok_or_else(|| format!("--evaluator takes <name>=<http(s) url>: {named}"))?;
                out.endpoints.insert(name.to_string(), url.to_string());
            }
            "--evaluator-timeout" => {
                let d = value()?;
                let secs = parse_duration(&d).filter(|s| *s > 0).ok_or_else(|| {
                    format!("--evaluator-timeout takes a duration such as '5 s': {d}")
                })?;
                out.evaluator_timeout = Some(std::time::Duration::from_secs(secs.unsigned_abs()));
            }
            "--busy-timeout" => {
                let d = value()?;
                let secs = parse_duration(&d)
                    .filter(|s| *s >= 0)
                    .ok_or_else(|| format!("--busy-timeout takes a duration such as '5 s': {d}"))?;
                out.busy_timeout = Some(std::time::Duration::from_secs(secs.unsigned_abs()));
            }
            "--help" | "-h" => return Err(String::new()),
            "--version" | "-V" => return Err(VERSION.to_string()),
            // an argument it does not take is an error, never ignored (D558)
            other => return Err(format!("no argument {other}")),
        }
    }
    if addressed && out.socket.is_some() {
        return Err("--socket listens in the address's place; give one or the other".to_string());
    }
    if out.create && out.store.is_none() {
        return Err("--create makes a deployment's store, which --store names".to_string());
    }
    if retained && out.store.is_none() {
        return Err("a retention is a deployment's; a sandbox's load names its own".to_string());
    }
    if (!out.endpoints.is_empty() || out.evaluator_timeout.is_some()) && out.store.is_none() {
        return Err("an evaluator's endpoint is a deployment's; a sandbox's evaluators answer as it sets them".to_string());
    }
    Ok(out)
}

/// The stack each thread of the service runs a request on: an expression
/// nests at most `expr::MAX_NESTING` levels and its tree is at most
/// `expr::MAX_DEPTH` deep, and evaluating one level of a tree took about
/// 8 KiB of stack in a release build, measured 2026-10-11; 16 MiB holds
/// those many times over, where the runtime's default 2 MiB did not hold a
/// 256-level tree (F1).
const WORKER_STACK: usize = 16 * 1024 * 1024;

fn main() -> std::io::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(WORKER_STACK)
        .enable_all()
        .build()?
        .block_on(serve())
}

/// What `--version` answers: the service's release and the release of the
/// stores it serves.
const VERSION: &str = concat!(
    "objectflow-service ",
    env!("CARGO_PKG_VERSION"),
    ", serving stores of objectflow-core ",
    env!("CARGO_PKG_VERSION")
);

/// How long a stopping service answers the requests in flight before it
/// exits with them unanswered (ADR-0183 decision 9): longer than an import
/// batch near the body limit took on a store of 36.7k objects, 23 s, and
/// shorter than systemd's default stop timeout, 90 s.
const GRACE: std::time::Duration = std::time::Duration::from_secs(30);

/// Resolved when the service is asked to stop, by SIGTERM as a supervisor
/// asks or SIGINT as a terminal does: it takes no new connection, answers
/// the requests in flight, and exits; past [`GRACE`] it exits without them
/// (ADR-0183 decision 9).
fn stopping() -> std::io::Result<impl std::future::Future<Output = ()>> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    Ok(async move {
        tokio::select! {
            _ = term.recv() => {}
            _ = int.recv() => {}
        }
        eprintln!(
            "{} stopping: no new connection, and the requests in flight answered for at most {} s",
            timestamp_text(wall_clock()),
            GRACE.as_secs()
        );
        std::thread::spawn(|| {
            std::thread::sleep(GRACE);
            eprintln!(
                "{} stopped with requests still in flight after {} s",
                timestamp_text(wall_clock()),
                GRACE.as_secs()
            );
            std::process::exit(1);
        });
    })
}

/// Where the service listens: an address, or a Unix socket.
enum Listening {
    Tcp(tokio::net::TcpListener),
    Unix(tokio::net::UnixListener),
}

/// A Unix socket bound at a path, replacing one a stopped service left
/// there and never another kind of file, and made 0660: its owner and its
/// group alone reach the service, the boundary the design leaves to the
/// deployment (ADR-0114, ADR-0183 decision 4).
fn bind_socket(path: &std::path::Path) -> std::io::Result<tokio::net::UnixListener> {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_socket() => std::fs::remove_file(path)?,
        Ok(_) => {
            return Err(std::io::Error::other(format!(
                "{} holds a file that is not a socket, which the service does not replace",
                path.display()
            )));
        }
        Err(_) => {}
    }
    let listener = tokio::net::UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660))?;
    Ok(listener)
}

/// A deployment's store file, checked before it is opened: a path that holds
/// no store, absent or empty, is refused unless the start asks to create
/// one, so a mistyped path never becomes an empty store served (ADR-0183
/// decision 3); a store made here is its owner's alone, 0600, which SQLite
/// gives its `-wal` and `-shm` too; and one others can read is said. What
/// it has to say is returned, said once the service listens, whose first
/// line says where.
fn store_file(path: &std::path::Path, create: bool) -> std::io::Result<Option<String>> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let held = std::fs::metadata(path)
        .map(|m| m.len() > 0)
        .unwrap_or(false);
    if !held {
        if !create {
            return Err(std::io::Error::other(format!(
                "{} holds no store; to make one there, start once with --create",
                path.display()
            )));
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        return Ok(Some(format!("a store made at {}", path.display())));
    }
    let mode = std::fs::metadata(path)?.permissions().mode();
    Ok((mode & 0o077 != 0).then(|| {
        format!(
            "the store {} is readable by others than its owner (mode {:o}); its personal data is too",
            path.display(),
            mode & 0o777
        )
    }))
}

async fn serve() -> std::io::Result<()> {
    let a = match args() {
        Ok(a) => a,
        Err(why) if why.is_empty() => {
            println!("{USAGE}");
            return Ok(());
        }
        Err(why) if why == VERSION => {
            println!("{VERSION}");
            return Ok(());
        }
        Err(why) => {
            eprintln!("objectflow-service: {why}\n{USAGE}");
            std::process::exit(2);
        }
    };
    let mut sandbox = Sandbox::default();
    let mut noted = None;
    if let Some(path) = &a.store {
        // a deployment's store, opened before anything is served (ADR-0174);
        // one at a release this core upgrades from is served only the
        // upgrade until it runs, and one at any other refuses to start, so
        // there is no downgrade (ADR-0177 decision 4)
        noted = store_file(path, a.create)?;
        let at = objectflow_core::release::store_release_at(path)
            .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))?;
        let busy = a
            .busy_timeout
            .unwrap_or(objectflow_core::store::BUSY_TIMEOUT);
        sandbox.deployment = true;
        sandbox.path = Some(path.clone());
        sandbox.retentions = a.retentions;
        match at {
            Some(r) if r != BUILTINS => {
                if objectflow_core::release::steps_from(&r, objectflow_core::release::RELEASES)
                    .is_none()
                {
                    return Err(std::io::Error::other(format!(
                        "{} is at {r}, which {BUILTINS} does not upgrade from",
                        path.display()
                    )));
                }
                sandbox.due = Some(r);
            }
            _ => {
                let mut s = Store::open(path, wall_clock())
                    .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))?;
                (s.attempt_retention, s.idempotency_retention) = a.retentions;
                s.set_busy_timeout(busy)
                    .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))?;
                sandbox.store = Some(s);
            }
        }
        sandbox.busy_timeout = busy;
        sandbox.endpoints = a.endpoints.clone();
        sandbox.evaluator_timeout = a
            .evaluator_timeout
            .unwrap_or(std::time::Duration::from_secs(5));
    }
    let deployment = sandbox.deployment;
    // a stop asked for once it listens is a graceful one
    let stop = stopping()?;
    let listening = match &a.socket {
        Some(path) => {
            let l = bind_socket(path)?;
            eprintln!("listening on unix:{}", path.display());
            Listening::Unix(l)
        }
        None => {
            let l = tokio::net::TcpListener::bind(&a.bind).await?;
            eprintln!("listening on http://{}", l.local_addr()?);
            Listening::Tcp(l)
        }
    };
    if let Some(note) = noted {
        eprintln!("{note}");
    }
    if let (Some(path), Some(at)) = (&a.store, &sandbox.due) {
        eprintln!(
            "the store {} is at {at}, and this core is {BUILTINS}: every operation is refused until POST /v1/upgrade runs (ADR-0177)",
            path.display()
        );
    }
    if let (Some(path), Some(s)) = (&a.store, &sandbox.store) {
        eprintln!(
            "the store {} at version {}, under {BUILTINS}",
            path.display(),
            s.lib.version
        );
        // a clock that ran ahead and came back (ADR-0174 decision 4)
        let lead = s.now - wall_clock();
        if lead > 0 {
            eprintln!(
                "the store's clock leads the wall clock by {lead} s: every event is stamped at {} until the wall clock passes it",
                timestamp_text(s.now)
            );
        }
        let unanswered: Vec<&String> = s
            .lib
            .evaluators
            .iter()
            .filter(|f| {
                !a.endpoints
                    .contains_key(f.split_once('.').map_or(f.as_str(), |(n, _)| n))
            })
            .collect();
        if !unanswered.is_empty() {
            eprintln!(
                "unknown, no endpoint answering them (ADR-0175 decision 5): {}",
                unanswered
                    .iter()
                    .map(|f| f.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        for (name, url) in &a.endpoints {
            eprintln!("the evaluator {name} asked at {url}");
        }
    }
    // a deployment's reads, beside its writer (ADR-0178)
    let reads = a.store.as_ref().map(|path| Reads {
        path: path.clone(),
        pool: Readers::new(
            path,
            a.busy_timeout
                .unwrap_or(objectflow_core::store::BUSY_TIMEOUT),
        ),
        seen: std::sync::RwLock::new(Seen::of(&sandbox)),
        floor: AtomicI64::new(sandbox.store.as_ref().map_or(0, |s| s.now)),
    });
    let service = Service {
        endpoints: sandbox.endpoints.clone(),
        evaluator_timeout: sandbox.evaluator_timeout,
        deployment,
        reads,
        writer: Mutex::new(sandbox),
        served: Mutex::new(std::collections::BTreeMap::new()),
    };
    let router = app(Arc::new(service), deployment);
    match listening {
        Listening::Tcp(l) => axum::serve(l, router).with_graceful_shutdown(stop).await?,
        Listening::Unix(l) => {
            axum::serve(l, router).with_graceful_shutdown(stop).await?;
            // the socket goes with the service; a start replaces one left
            // by a service that was killed
            if let Some(path) = &a.socket {
                std::fs::remove_file(path)?;
            }
        }
    }
    eprintln!("{} stopped", timestamp_text(wall_clock()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `/v1`, and a sandbox's controls, take what `guides/wire-v1.json`
    /// promises (ADR-0180 decision 7, ADR-0181 decision 6): an operation or
    /// a key the baseline lists and the service no longer takes is a
    /// break; one the service takes and the baseline does not list is an
    /// addition to promise there, deliberately.
    #[test]
    #[allow(clippy::expect_used)]
    fn v1_takes_what_it_promised() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/guides/wire-v1.json");
        let baseline: J =
            serde_json::from_str(&std::fs::read_to_string(path).expect("wire-v1.json"))
                .expect("the baseline");
        let promised = baseline["operations"].as_object().expect("its operations");
        let taken: std::collections::BTreeMap<&str, &[&str]> = TAKES
            .iter()
            .filter(|(p, _)| p.starts_with("/v1/") || p.starts_with("/sandbox/"))
            .map(|(p, k)| (*p, *k))
            .collect();
        let mut wrong = Vec::new();
        for (op, keys) in promised {
            let Some(takes) = taken.get(op.as_str()) else {
                wrong.push(format!(
                    "{op} is promised and no longer served: a break of /v1"
                ));
                continue;
            };
            for k in keys.as_array().expect("its keys") {
                let k = k.as_str().expect("a key");
                if !takes.contains(&k) {
                    wrong.push(format!("{op} no longer takes {k}: a break of /v1"));
                }
            }
        }
        for (op, takes) in &taken {
            let listed = promised.get(*op).and_then(J::as_array);
            for k in *takes {
                if !listed.is_some_and(|l| l.iter().any(|x| x == k)) {
                    wrong.push(format!(
                        "{op} takes {k}, which wire-v1.json does not promise"
                    ));
                }
            }
            if listed.is_none() && takes.is_empty() {
                wrong.push(format!(
                    "{op} is served, and wire-v1.json does not promise it"
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    #[test]
    fn a_failed_read_is_unavailable_storage_and_a_construct_not_run_is_501() {
        let status = |f: Fault| f.into_response().status();
        let unreadable = || NotRun::Unreadable("the store's log, which could not be read".into());
        assert_eq!(
            status(Fault::from(unreadable())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status(Fault::from(request::NoVerdict::NotRun(unreadable()))),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status(Fault::from(NotRun::Unsupported(
                "a metric in a filter".into()
            ))),
            StatusCode::NOT_IMPLEMENTED
        );
    }
}

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

/// When the line being written to the log began, in milliseconds since
/// 1970, 0 while none is: a pipe left full stopped the service at its next
/// line while `/health` answered 200 (fourth production review).
static LOGGING_SINCE: AtomicI64 = AtomicI64::new(0);

/// When the service started, in seconds since 1970, for `/metrics`
/// (ADR-0191 decision 4).
static STARTED_AT: AtomicI64 = AtomicI64::new(0);

/// When a write last failed for a full volume, in milliseconds, which
/// `/health` says until a write commits after it (ADR-0192 decision 7).
static DISK_FULL_AT: AtomicI64 = AtomicI64::new(0);

/// Whether a write failed for a full volume after the last that committed,
/// which `/health` says (ADR-0192 decision 7).
fn disk_full_since(written_at: i64) -> bool {
    let full = DISK_FULL_AT.load(Ordering::SeqCst);
    full != 0 && full > written_at
}

/// The store's lock, held for the process's life, so a second service on
/// the store is said at its start (ADR-0192 decision 7).
static STORE_LOCK: std::sync::OnceLock<std::fs::File> = std::sync::OnceLock::new();
/// The lines not written while the log was blocked.
static LINES_DROPPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// How long a line may take before the log is held blocked.
const LOG_BLOCKED_MS: i64 = 5_000;

/// Whether a line begun at `since` has blocked the log too long.
fn log_blocked(since: i64, now: i64) -> bool {
    since != 0 && now - since > LOG_BLOCKED_MS
}

/// A line on the log, standard error, stamped with the wall clock, every
/// line the service writes among them, where its start's carried no time;
/// dropped and counted while the log has been blocked longer than
/// `LOG_BLOCKED_MS`, so a reader of the log that stopped reading stops no
/// request beyond it (fourth production review).
fn say(line: std::fmt::Arguments<'_>) {
    let now = wall_millis();
    if log_blocked(LOGGING_SINCE.load(Ordering::SeqCst), now) {
        LINES_DROPPED.fetch_add(1, Ordering::SeqCst);
        return;
    }
    let mine = LOGGING_SINCE
        .compare_exchange(0, now, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok();
    // a log whose reader has gone drops the line: `eprintln!` panicked on
    // the broken pipe, losing answers already committed and leaving the
    // log held blocked until a restart (fifth production review)
    let written = {
        use std::io::Write;
        writeln!(
            std::io::stderr().lock(),
            "{} {line}",
            timestamp_text(wall_clock())
        )
    };
    if written.is_err() {
        LINES_DROPPED.fetch_add(1, Ordering::SeqCst);
    }
    if mine {
        LOGGING_SINCE.store(0, Ordering::SeqCst);
    }
}

/// `say` with `format!`'s arguments.
macro_rules! say {
    ($($arg:tt)*) => { say(format_args!($($arg)*)) };
}

/// An endpoint as the log shows it, a password in its address hidden,
/// where the start's line and each failed call wrote it whole (fourth
/// production review).
fn shown(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    match authority.rsplit_once('@') {
        Some((user, host)) => {
            let name = user.split(':').next().unwrap_or(user);
            let tail = &rest[authority.len()..];
            format!("{scheme}://{name}:***@{host}{tail}")
        }
        None => url.to_string(),
    }
}

/// Each evaluator's calls that answered nothing it could use, which
/// `/metrics` gives (fourth production review).
static EVALUATOR_FAILURES: Mutex<std::collections::BTreeMap<String, u64>> =
    Mutex::new(std::collections::BTreeMap::new());

/// The most calls one request asks its endpoints, after which a call is
/// unknown (ADR-0175 decision 2).
const MAX_CALLS: usize = 16;

/// An evaluator's verdict on a call, asked at its endpoint (ADR-0175
/// decision 4): `satisfied` or `unsatisfied` as it answers, and
/// `unanswered`, its unknown, for any other answer, a fault, a timeout or no
/// connection, each said on stderr.
fn asked(
    endpoint: &str,
    token: Option<&str>,
    timeout: std::time::Duration,
    call: &Call,
) -> &'static str {
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
        if let Ok(mut failures) = EVALUATOR_FAILURES.lock() {
            *failures.entry(call.evaluator.clone()).or_insert(0) += 1;
        }
        say!(
            "objectflow-service: {}.{} of {} argument(s) is unknown: {why}",
            call.evaluator,
            call.function,
            call.arguments.len()
        );
        "unanswered"
    };
    let mut posting = agent
        .post(endpoint)
        .header("content-type", "application/json");
    // the evaluator's token, where it is given one, never said (ADR-0188)
    if let Some(token) = token {
        posting = posting.header("authorization", format!("Bearer {token}"));
    }
    let mut answer = match posting.send(&body) {
        Ok(a) => a,
        Err(e) => return unknown(format!("{} was not reached: {e}", shown(endpoint))),
    };
    let status = answer.status().as_u16();
    let text = match answer.body_mut().read_to_string() {
        Ok(t) => t,
        Err(e) => return unknown(format!("{}'s answer was not read: {e}", shown(endpoint))),
    };
    if status != 200 {
        return unknown(format!("{} answered {status}", shown(endpoint)));
    }
    // an answer naming a key twice is no verdict, as a request's body is
    // refused (ADR-0192 decision 7)
    match strict_json(text.as_bytes())
        .ok()
        .as_ref()
        .and_then(|a| a.get("verdict"))
        .and_then(J::as_str)
    {
        Some("satisfied") => "satisfied",
        Some("unsatisfied") => "unsatisfied",
        _ => unknown(format!(
            "{} answered no verdict, in {} byte(s)",
            shown(endpoint),
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
            Some(endpoint) if calls < MAX_CALLS => asked(
                &endpoint,
                shared.tokens.get(&call.evaluator).map(String::as_str),
                timeout,
                &call,
            ),
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
    /// Each evaluator's bearer token (ADR-0188 decision 2).
    tokens: std::collections::BTreeMap<String, String>,
    evaluator_timeout: std::time::Duration,
    deployment: bool,
    /// How long a deployment's read may take before it is stopped
    /// (ADR-0184 decision 3).
    read_timeout: std::time::Duration,
    /// The analytics, metrics, exceptions, diagnostics and exports, bounded
    /// in number (ADR-0189 decision 2); the worklists, queries, the objects
    /// available and a subscription's pages, bounded alike beside them; and a
    /// change's dry runs, one at a time (fifth production review N-1 and
    /// DRYRUN-MEMORY, ADR-0190 decision 2).
    admission: Admission,
    worklists: Admission,
    dry_runs: Admission,
    /// Every read an agent makes that waits its turn, whatever it reads, so
    /// an agent's sweeping reads never hold a person's worklist slot or an
    /// analytic's (sixth production review P5, ADR-0192 decision 3).
    agents: Admission,
    /// What it has answered, by route and status: how many, and their
    /// seconds, which `GET /metrics` gives (production review OBSERVE).
    served: Mutex<std::collections::BTreeMap<(String, u16), (u64, f64)>>,
    /// The requests being answered now, and when the writer was taken, in
    /// milliseconds since 1970, none while it is free: a writer held for
    /// minutes answered health 200 and showed nothing (OBSERVE-2).
    in_flight: std::sync::atomic::AtomicUsize,
    writer_since: AtomicI64,
    /// The connections open, those the cap closed answering nothing to make
    /// room, and those it refused, every one answering a request: silent
    /// connections at the cap refused every newcomer, `/health` among them
    /// (third production review C-1).
    connections: Connections,
    /// A deployment's store file, whose size and its write-ahead log's
    /// `/metrics` gives, read without the writer (third production review
    /// N6).
    store_path: Option<std::path::PathBuf>,
    /// The memory given back after a request that held much, how many
    /// times, and whether a giving back is running (ADR-0187 decision 2).
    trims: std::sync::atomic::AtomicU64,
    trimming: std::sync::atomic::AtomicBool,
    /// The store's file as the service opened it, its device and inode,
    /// which every write and `/health` compare with the path's (fourth
    /// production review).
    store_identity: Option<(u64, u64)>,
    /// What `/health` and `/metrics` say of the writes and the answers.
    watch: Watch,
}

/// What the service watches of its writes and answers (fourth production
/// review): when a write last committed and last met another process's
/// lock past its retries, each verdict's kind, and each route's durations.
#[derive(Debug, Default)]
struct Watch {
    written_at: AtomicI64,
    stalled_at: AtomicI64,
    verdicts: Mutex<std::collections::BTreeMap<&'static str, u64>>,
    durations: Mutex<std::collections::BTreeMap<String, [u64; BUCKETS.len()]>>,
    answered: std::sync::atomic::AtomicU64,
    /// The write-ahead log's cuts, those a read prevented, and those left
    /// for the next look where a read ran past a moment's hold.
    cuts: std::sync::atomic::AtomicU64,
    cuts_missed: std::sync::atomic::AtomicU64,
    cuts_deferred: std::sync::atomic::AtomicU64,
    /// The epoch each read begins in, moved to stop the reads running when
    /// a cut is urgent (ADR-0194 decision 4).
    cut_epoch: Arc<std::sync::atomic::AtomicU64>,
}

/// The upper bounds, in seconds, of `/metrics`' duration histogram.
const BUCKETS: [f64; 8] = [0.005, 0.025, 0.1, 0.5, 1.0, 2.5, 10.0, 30.0];

/// How recently a write must have met another process's lock past its
/// retries, with none committed since, for `/health` to say writes stall.
const STALLED_MS: i64 = 60_000;

impl Watch {
    /// A request's verdict: committed, or stopped by another process's lock.
    fn note(&self, v: &Verdict) {
        let now = wall_millis();
        match v {
            Verdict::Satisfied { .. } => self.written_at.store(now, Ordering::SeqCst),
            Verdict::RetriesExhausted => self.stalled_at.store(now, Ordering::SeqCst),
            _ => {}
        }
        if let Ok(mut kinds) = self.verdicts.lock() {
            *kinds.entry(v.kind()).or_insert(0) += 1;
        }
    }

    /// Whether writes stall: the last one met another process's lock past
    /// its retries, lately, and none committed since.
    fn stalled(&self, now: i64) -> bool {
        let stalled = self.stalled_at.load(Ordering::SeqCst);
        stalled != 0
            && stalled > self.written_at.load(Ordering::SeqCst)
            && now - stalled < STALLED_MS
    }

    /// A route's answer, by how long it took.
    fn timed(&self, route: &str, took: f64) {
        self.answered.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut durations) = self.durations.lock() {
            let row = durations
                .entry(route.to_string())
                .or_insert([0; BUCKETS.len()]);
            for (i, bound) in BUCKETS.iter().enumerate() {
                if took <= *bound {
                    row[i] += 1;
                }
            }
        }
    }
}

/// A file's device and inode, as `/health` and every write compare them.
fn file_identity(path: &std::path::Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| (m.dev(), m.ino()))
}

/// Whether the store's path still names the file the service opened, where
/// a file moved or replaced under it kept every write acknowledged into a
/// file no one would read (fourth production review).
fn store_replaced(shared: &Service) -> Option<String> {
    let (Some(path), Some(opened)) = (&shared.store_path, shared.store_identity) else {
        return None;
    };
    (file_identity(path) != Some(opened)).then(|| {
        format!(
            "store_replaced: {} is no longer the file the service opened, moved or replaced under it; stop the service and start it on the store to serve",
            path.display()
        )
    })
}

#[derive(Debug, Default)]
struct Connections {
    open: std::sync::atomic::AtomicUsize,
    made_room: std::sync::atomic::AtomicU64,
    refused: std::sync::atomic::AtomicU64,
}

/// Now, in milliseconds since 1970.
fn wall_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// How long the writer has been held, in seconds, or none while it is free.
fn writer_held(shared: &Service) -> f64 {
    match shared.writer_since.load(Ordering::SeqCst) {
        0 => 0.0,
        since => (wall_millis() - since).max(0) as f64 / 1000.0,
    }
}

/// The process's resident memory in bytes, as Linux reports it.
fn resident_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kb = status
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))?
        .trim()
        .strip_suffix("kB")?
        .trim()
        .parse::<u64>()
        .ok()?;
    kb.checked_mul(1024)
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
    since: &'a AtomicI64,
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
        self.since.store(0, Ordering::SeqCst);
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

/// The reads whose cost grows with the store running now, and those
/// waiting, at most `most` running: each takes a slot before its reader, so
/// their memory is bounded by their number, where 32 metrics on six years
/// took 3.3 GB (fourth production review READS, ADR-0189 decision 2).
struct Admission {
    running: Mutex<usize>,
    freed: std::sync::Condvar,
    most: usize,
    waiting: std::sync::atomic::AtomicUsize,
    /// New reads held back while the write-ahead log is cut back.
    barred: std::sync::atomic::AtomicBool,
}

impl Admission {
    fn new(most: usize) -> Self {
        Admission {
            running: Mutex::new(0),
            freed: std::sync::Condvar::new(),
            most,
            waiting: std::sync::atomic::AtomicUsize::new(0),
            barred: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// New reads held back, and those running waited out until `until`:
    /// whether none is running then. `release` lets the held ones in.
    fn hold(&self, until: std::time::Instant) -> bool {
        self.barred.store(true, Ordering::SeqCst);
        let Ok(mut running) = self.running.lock() else {
            return false;
        };
        while *running > 0 {
            let left = until.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return false;
            }
            running = match self.freed.wait_timeout(running, left) {
                Ok((r, _)) => r,
                Err(_) => return false,
            };
        }
        true
    }

    /// The reads held back let in.
    fn release(&self) {
        self.barred.store(false, Ordering::SeqCst);
        self.freed.notify_all();
    }

    /// A slot, waited for until `until`, and refused `read_timeout` past it.
    fn take(
        &self,
        until: std::time::Instant,
        timeout: std::time::Duration,
    ) -> Result<Slot<'_>, Fault> {
        let panicked = || Fault::Internal("a previous request panicked".to_string());
        self.waiting.fetch_add(1, Ordering::SeqCst);
        let taken = (|| {
            let mut running = self.running.lock().map_err(|_| panicked())?;
            while *running >= self.most || self.barred.load(Ordering::SeqCst) {
                let left = until.saturating_duration_since(std::time::Instant::now());
                if left.is_zero() {
                    return Err(Fault::TimedOut(format!(
                        "{} reads that grow with the store ran for the whole of this one's deadline, --read-timeout ({} s), so it was not started; --max-reads bounds how many run at once",
                        self.most,
                        timeout.as_secs()
                    )));
                }
                running = self
                    .freed
                    .wait_timeout(running, left)
                    .map_err(|_| panicked())?
                    .0;
            }
            *running += 1;
            Ok(())
        })();
        self.waiting.fetch_sub(1, Ordering::SeqCst);
        taken.map(|()| Slot(Some(self)))
    }

    fn running(&self) -> usize {
        self.running.lock().map_or(0, |n| *n)
    }
}

/// A heavy read's slot, given back when the read ends; none for a
/// sandbox's, which take the writer.
struct Slot<'a>(Option<&'a Admission>);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        if let Some(a) = self.0 {
            if let Ok(mut running) = a.running.lock() {
                *running = running.saturating_sub(1);
            }
            // a read waiting its turn, and a truncation waiting for none
            a.freed.notify_all();
        }
    }
}

/// A read whose cost grows with the store: a query, a metric, the objects
/// available, the exceptions, the diagnostics, a page of a subscription or
/// an export, a change's dry run. It takes a slot first, waited for within
/// its own deadline, and the read is given what is left of the deadline; a
/// read of one object, of the events by position or of the declaration
/// never waits (ADR-0189 decision 2).
fn heavy_reading<'s>(
    shared: &'s Shared,
    pool: &'s Admission,
    actor: Option<&str>,
) -> Result<(Slot<'s>, Reading<'s>), Fault> {
    let until = std::time::Instant::now() + shared.read_timeout;
    let slot = match shared.reads {
        Some(_) => pool_of(shared, pool, actor)?.take(until, shared.read_timeout)?,
        None => Slot(None),
    };
    Ok((slot, reading_until(shared, until)?))
}

/// The pool a read waits in: an agent's in the agents', whatever it reads,
/// so four of an agent's queries never hold every person's worklist slot,
/// a person waiting 28.6 s (sixth production review P5, ADR-0192 decision
/// 3); a change's dry run in its one slot, whoever asks.
fn pool_of<'s>(
    shared: &'s Shared,
    pool: &'s Admission,
    actor: Option<&str>,
) -> Result<&'s Admission, Fault> {
    let Some(actor) = actor.filter(|_| !std::ptr::eq(pool, &shared.dry_runs)) else {
        return Ok(pool);
    };
    let mut r = reading(shared)?;
    let s = r.store()?;
    let kind = objectflow_core::request::actor_kind(&s.snapshot()?, actor)?;
    Ok(if kind.as_deref() == Some("agent") {
        &shared.agents
    } else {
        pool
    })
}

/// Whether a request is a change of the flow's `submit` or `refresh`, which
/// runs a dry run: a transition of that name on any other type is an
/// ordinary write, which took the dry run's slot and queued behind every
/// metric, a claim's submit waiting up to 29 s and refused `503` (fifth
/// production review N-1, ADR-0190 decision 2).
fn changes_the_flow(shared: &Shared, req: &Request) -> Result<bool, Fault> {
    let (Some(id), "submit" | "refresh") = (&req.object_id, req.transition.as_str()) else {
        return Ok(false);
    };
    let mut r = reading(shared)?;
    let s = r.store()?;
    Ok(s.snapshot()?
        .get(id)?
        .is_some_and(|o| o.type_ == objectflow_core::model::DECLARATION_CHANGE))
}

/// A read of the store as it stands, beside the writes and never behind
/// them, for a deployment; under the lock for a sandbox.
fn reading(shared: &Shared) -> Result<Reading<'_>, Fault> {
    reading_until(shared, std::time::Instant::now() + shared.read_timeout)
}

/// A read, stopped at `until` (ADR-0184 decision 3).
fn reading_until(shared: &Shared, until: std::time::Instant) -> Result<Reading<'_>, Fault> {
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
    // a store moved, gone or replaced under the service answers a read as it
    // answers a write, where a read answered 500 (ADR-0192 decision 7)
    if let Some(replaced) = store_replaced(shared) {
        return Err(Fault::Unavailable(replaced));
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
    let mut store = reads.pool.read(lib, earlier, now, answers)?;
    // a read answers within its deadline or is stopped there, so no read
    // holds a reader, and an erasure's truncation behind it, without bound
    // (ADR-0184 decision 3); and is stopped where an urgent cut of the log
    // moves the epoch it began in (ADR-0194 decision 4)
    store.deadline = Some(until);
    store.stop = Some((
        shared.watch.cut_epoch.clone(),
        shared.watch.cut_epoch.load(Ordering::SeqCst),
    ));
    Ok(Reading::Own(Some(Box::new(store)), reads))
}

/// The code of the fault last said, when, and how many of it were left
/// unsaid since: a full disk said its fault for every write, at the clients'
/// retry rate (fourth production review).
static LAST_FAULT: Mutex<(String, i64, u64)> = Mutex::new((String::new(), 0, 0));

/// How long after a fault is said another of its code is counted, not said.
const FAULT_QUIET_MS: i64 = 1_000;

/// Whether a fault of this code is said, with how many of the last one's
/// code were left unsaid before it, or, within `FAULT_QUIET_MS` of the last
/// said of its code, only counted.
fn fault_said(last: &Mutex<(String, i64, u64)>, code: &str, now: i64) -> Option<u64> {
    let Ok(mut last) = last.lock() else {
        return Some(0);
    };
    if last.0 == code && now - last.1 < FAULT_QUIET_MS {
        last.2 += 1;
        return None;
    }
    let unsaid = last.2;
    *last = (code.to_string(), now, 0);
    Some(unsaid)
}

/// A fault's code, which a client branches on rather than its reason's
/// text, which is not promised: the reason's own prefix where it has one,
/// `read_timeout`, `busy`, `engine_mismatch`, `key_reused`, `disk_low`,
/// `disk_full`, `store_replaced` and the like, else its status's name
/// (fourth production review).
fn fault_code(status: u16, message: &str) -> String {
    if let Some((head, _)) = message.split_once(':')
        && !head.is_empty()
        && head.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
    {
        return head.to_string();
    }
    match status {
        400 => "bad_request",
        404 => "not_found",
        405 => "method_not_allowed",
        408 => "slow_body",
        409 => "conflict",
        413 => "too_large",
        501 => "not_run",
        503 => "unavailable",
        _ => "internal",
    }
    .to_string()
}

/// A fault's reason as the log says it, at most 1 KiB of it, where a filter
/// of 1.9 MB was written whole into one line (fourth production review
/// A-10).
fn said(message: &str) -> std::borrow::Cow<'_, str> {
    const MOST: usize = 1024;
    if message.len() <= MOST {
        return message.into();
    }
    let mut end = MOST;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… ({} bytes)", &message[..end], message.len()).into()
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
    /// A read that passed its deadline (ADR-0184 decision 3).
    TimedOut(String),
    /// A request whose body did not arrive in time (ADR-0184 decision 4).
    SlowBody(String),
    /// What the service cannot do now and may later, its code its reason's
    /// prefix: `disk_full`, `store_replaced` (fourth production review).
    Unavailable(String),
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
            Fault::TimedOut(m) => (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("read_timeout: {m}"),
            ),
            Fault::SlowBody(m) => (StatusCode::REQUEST_TIMEOUT, m),
            Fault::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, m),
        };
        // the service's own faults said with their reason, which the line
        // of the answer, its route and status alone, does not (OBSERVE-2)
        let code = fault_code(status.as_u16(), &message);
        if status.is_server_error() {
            match fault_said(&LAST_FAULT, &code, wall_millis()) {
                Some(0) => say!("fault {}: {}", status.as_u16(), said(&message)),
                Some(unsaid) => say!(
                    "fault {}: {} (and {unsaid} like the last fault said, left unsaid)",
                    status.as_u16(),
                    said(&message)
                ),
                None => {}
            }
        }
        (status, Json(json!({ "fault": message, "code": code }))).into_response()
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
            NotRun::TimedOut(w) => Fault::TimedOut(w),
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
        // a full volume answers 503, which a client may retry, where it
        // answered 500, the service's own fault (fourth production review)
        if e.is_disk_full() {
            DISK_FULL_AT.store(wall_millis(), Ordering::SeqCst);
            return Fault::Unavailable(e.0);
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
    ("/v1/history", &["actor", "id", "follow", "limit", "cursor"]),
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

/// How long a request's body may take to arrive once its head has, set
/// from `--idle-timeout` as the service starts: a body half-sent held its
/// connection and its task for ever (ADR-0184 decision 4).
static BODY_READ: std::sync::OnceLock<std::time::Duration> = std::sync::OnceLock::new();

/// A JSON body read refusing an object that names a key twice, where the
/// last one won, a second `actor` changing who was recorded (fifth
/// production review, ADR-0190 decision 8).
fn strict_json(bytes: &[u8]) -> Result<J, String> {
    use serde::de::{DeserializeSeed, Deserializer, Error, MapAccess, SeqAccess, Visitor};
    struct Strict;
    impl<'de> DeserializeSeed<'de> for Strict {
        type Value = J;
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<J, D::Error> {
            d.deserialize_any(Strict)
        }
    }
    impl<'de> Visitor<'de> for Strict {
        type Value = J;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a JSON value")
        }
        fn visit_bool<E: Error>(self, v: bool) -> Result<J, E> {
            Ok(J::Bool(v))
        }
        fn visit_i64<E: Error>(self, v: i64) -> Result<J, E> {
            Ok(J::from(v))
        }
        fn visit_u64<E: Error>(self, v: u64) -> Result<J, E> {
            Ok(J::from(v))
        }
        fn visit_f64<E: Error>(self, v: f64) -> Result<J, E> {
            Ok(serde_json::Number::from_f64(v).map_or(J::Null, J::Number))
        }
        fn visit_str<E: Error>(self, v: &str) -> Result<J, E> {
            Ok(J::String(v.to_string()))
        }
        fn visit_string<E: Error>(self, v: String) -> Result<J, E> {
            Ok(J::String(v))
        }
        fn visit_unit<E: Error>(self) -> Result<J, E> {
            Ok(J::Null)
        }
        fn visit_none<E: Error>(self) -> Result<J, E> {
            Ok(J::Null)
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<J, A::Error> {
            let mut items = Vec::new();
            while let Some(v) = seq.next_element_seed(Strict)? {
                items.push(v);
            }
            Ok(J::Array(items))
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<J, A::Error> {
            let mut m = Map::new();
            while let Some(k) = map.next_key::<String>()? {
                if m.contains_key(&k) {
                    return Err(A::Error::custom(format!("the key {k} is named twice")));
                }
                let v = map.next_value_seed(Strict)?;
                m.insert(k, v);
            }
            Ok(J::Object(m))
        }
    }
    let mut read = serde_json::Deserializer::from_slice(bytes);
    let v = Strict.deserialize(&mut read).map_err(|e| e.to_string())?;
    read.end().map_err(|e| e.to_string())?;
    Ok(v)
}

impl<S: Send + Sync> axum::extract::FromRequest<S> for Body {
    type Rejection = Fault;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Fault> {
        let path = req.uri().path().to_string();
        let bound = BODY_READ
            .get()
            .copied()
            .unwrap_or(std::time::Duration::from_secs(30));
        let json = req
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.split(';')
                    .next()
                    .is_some_and(|t| t.trim().eq_ignore_ascii_case("application/json"))
            });
        if !json {
            return Err(Fault::Bad(
                "Expected request with `Content-Type: application/json`".to_string(),
            ));
        }
        let read =
            match tokio::time::timeout(bound, axum::body::Bytes::from_request(req, state)).await {
                Ok(read) => read,
                Err(_) => {
                    return Err(Fault::SlowBody(format!(
                        "the body did not arrive within {} s",
                        bound.as_secs()
                    )));
                }
            };
        let bytes = match read {
            Ok(bytes) => bytes,
            Err(e) => return Err(Fault::Bad(e.body_text())),
        };
        let body = match strict_json(&bytes) {
            Ok(J::Object(m)) => m,
            Ok(_) => return Err(Fault::Bad("the body is a JSON object".to_string())),
            Err(e) => return Err(Fault::Bad(e)),
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
    let guard = match shared.writer.lock() {
        Ok(guard) => guard,
        // a request that panicked holding the writer left it poisoned,
        // which made every later write 500 until a restart: taken back, and
        // a deployment's store opened again from its file, so nothing the
        // panic left half-changed in memory stays, the file's transaction
        // having been rolled back (POISON)
        Err(poisoned) => {
            shared.writer.clear_poison();
            let mut guard = poisoned.into_inner();
            say!(
                "a request panicked holding the writer; {}",
                reopened(&mut guard)
            );
            guard
        }
    };
    if let Some(replaced) = store_replaced(shared) {
        return Err(Fault::Unavailable(replaced));
    }
    shared.writer_since.store(wall_millis(), Ordering::SeqCst);
    Ok(Writing {
        guard,
        reads: shared.reads.as_ref(),
        since: &shared.writer_since,
    })
}

/// A deployment's store opened again from its file, as the service opens
/// it at its start; what was done, said.
fn reopened(sandbox: &mut Sandbox) -> String {
    let Some(path) = sandbox
        .path
        .clone()
        .filter(|_| sandbox.deployment && sandbox.due.is_none())
    else {
        return "the sandbox's store kept as it is".to_string();
    };
    let opened = Store::open(&path, wall_clock()).and_then(|mut s| {
        (s.attempt_retention, s.idempotency_retention) = sandbox.retentions;
        s.set_busy_timeout(sandbox.busy_timeout)?;
        Ok(s)
    });
    match opened {
        Ok(s) => {
            sandbox.store = Some(s);
            format!("the store opened again from {}", path.display())
        }
        Err(e) => format!(
            "the store could not be opened again from {}: {e}",
            path.display()
        ),
    }
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

/// A key of a body that holds a string: absent or null is none, and a
/// value of any other JSON type is refused, never read as absent, where a
/// numeric `filter` returned every object unfiltered (production review
/// F6, TYPES).
fn text<'a>(body: &'a J, key: &str) -> Result<Option<&'a str>, Fault> {
    match body.get(key) {
        None | Some(J::Null) => Ok(None),
        Some(J::String(t)) => Ok(Some(t)),
        Some(_) => Err(Fault::Bad(format!("{key} takes a string"))),
    }
}

/// A key that holds an integer, as [`text`] holds a string.
fn int(body: &J, key: &str) -> Result<Option<i64>, Fault> {
    match body.get(key) {
        None | Some(J::Null) => Ok(None),
        Some(v) => v
            .as_i64()
            .map(Some)
            .ok_or_else(|| Fault::Bad(format!("{key} takes an integer"))),
    }
}

/// A key that holds an object, as [`text`] holds a string.
fn object<'a>(body: &'a J, key: &str) -> Result<Option<&'a Map<String, J>>, Fault> {
    match body.get(key) {
        None | Some(J::Null) => Ok(None),
        Some(J::Object(m)) => Ok(Some(m)),
        Some(_) => Err(Fault::Bad(format!("{key} takes an object"))),
    }
}

/// A key that holds an array, as [`text`] holds a string.
fn array<'a>(body: &'a J, key: &str) -> Result<Option<&'a Vec<J>>, Fault> {
    match body.get(key) {
        None | Some(J::Null) => Ok(None),
        Some(J::Array(a)) => Ok(Some(a)),
        Some(_) => Err(Fault::Bad(format!("{key} takes an array"))),
    }
}

/// A body's actor, `{"id": …, "principal": …}`: its id, where it names one;
/// an actor of another shape, or with a key it does not take, is refused,
/// never read as no actor or with the key dropped (TYPES).
fn actor_id(body: &J) -> Result<Option<&str>, Fault> {
    let Some(actor) = object(body, "actor")? else {
        return Ok(None);
    };
    if let Some(k) = actor.keys().find(|k| *k != "id" && *k != "principal") {
        return Err(Fault::Bad(format!("actor takes no key {k}")));
    }
    let actor = &body["actor"];
    text(actor, "principal")?;
    text(actor, "id")
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
            match text(&body, k)? {
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
        for f in array(&body, "files")?.into_iter().flatten() {
            let (Some(name), Some(text)) = (text(f, "name")?, text(f, "text")?) else {
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
        let target = if let Some(by) = text(&body, "advance")? {
            let step = parse_duration(by).ok_or_else(|| {
                Fault::Bad(format!("advance takes a duration such as '1 h': {by}"))
            })?;
            s.now + step
        } else if let Some(at) = text(&body, "set")? {
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
        if let Some(answers) = object(&body, "answers")? {
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
        let prepared = match &req.object_id {
            Some(id) if shared.reads.is_some() && changes_the_flow(&shared, &req)? => {
                let (_slot, mut r) = heavy_reading(&shared, &shared.dry_runs, actor_id(&body)?)?;
                let s = r.store()?;
                match objectflow_core::publish::prepare(&s.snapshot()?, id, &req.actor) {
                    Ok(prepared) => prepared,
                    // past the read's deadline it is answered so, never
                    // computed again under the lock, where every write
                    // waited for it unbounded and unsaid (PUBLISH-SCALE)
                    Err(NotRun::TimedOut(_)) => return Err(dry_run_late(&shared)),
                    // any other read that cannot prepare it leaves it to the
                    // request, which says why it cannot
                    Err(_) => None,
                }
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
        shared.watch.note(&v);
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
        // a check of a change's submit runs its dry run, one at a time as a
        // submit's does, where six at once took 9.3 GB outside every bound
        // (fifth production review DRYRUN-MEMORY, ADR-0190 decision 2)
        let _slot = if shared.reads.is_some() && changes_the_flow(&shared, &req)? {
            let until = std::time::Instant::now() + shared.read_timeout;
            Some(shared.dry_runs.take(until, shared.read_timeout)?)
        } else {
            None
        };
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
        let (Some(_actor), Some(subscription)) = (actor_id(&body)?, text(&body, "subscription")?)
        else {
            return Err(Fault::Bad(
                "pull names its actor and the subscription".to_string(),
            ));
        };
        let cursor = cursor_of(&body)?;
        let limit = limit_of(&body)?;
        let (_slot, mut r) = heavy_reading(&shared, &shared.worklists, actor_id(&body)?)?;
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
    let mut out = json!({
        "items": page.items.iter().map(|i| json!({
            "id": i.id, "type": i.type_,
            "fields": J::Object(i.fields.iter().map(|(f, v)| (f.clone(), as_held(&i.type_, f, encode(v)))).collect()),
        })).collect::<Vec<_>>(),
        "cursor": page.cursor,
        "refused": page.refused,
    });
    // what a malformed filter did, a sentence beside its code (ADR-0191)
    if let (Some(reason), Some(o)) = (&page.reason, out.as_object_mut()) {
        o.insert("reason".to_string(), J::String(reason.clone()));
    }
    out
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
        let (Some(_actor), Some(type_)) = (actor_id(&body)?, text(&body, "type")?) else {
            return Err(Fault::Bad(
                "exceptions names its actor and the type".to_string(),
            ));
        };
        let (_slot, mut r) = heavy_reading(&shared, &shared.admission, actor_id(&body)?)?;
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
            actor_id(&body)?,
            text(&body, "type")?,
            text(&body, "transition")?,
        ) else {
            return Err(Fault::Bad(
                "available names its actor, the type and the transition".to_string(),
            ));
        };
        let (_slot, mut r) = heavy_reading(&shared, &shared.worklists, actor_id(&body)?)?;
        let s = r.store()?;
        let world = s.snapshot()?;
        let mut page = page_json(&reads::available_page(
            &world,
            actor,
            type_,
            transition,
            text(&body, "cursor")?,
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
            actor_id(&body)?,
            text(&body, "type")?,
            text(&body, "window")?,
        ) else {
            return Err(Fault::Bad(
                "diagnostics names its actor, the type and a window".to_string(),
            ));
        };
        let window = parse_duration(window).filter(|s| *s > 0).ok_or_else(|| {
            Fault::Bad(format!(
                "window takes a positive duration such as '30 days': {window}"
            ))
        })?;
        let (_slot, mut r) = heavy_reading(&shared, &shared.admission, actor_id(&body)?)?;
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
        let task = object(&body, "task")?.map(|_| &body["task"]);
        let (Some(actor), Some(name), Some(before)) = (
            actor_id(&body)?,
            task.map(|t| text(t, "name")).transpose()?.flatten(),
            task.map(|t| text(t, "before")).transpose()?.flatten(),
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
        let (Some(_actor), Some(source)) = (actor_id(&body)?, text(&body, "source")?) else {
            return Err(Fault::Bad(
                "export names its actor and the source".to_string(),
            ));
        };
        let (_slot, mut r) = heavy_reading(&shared, &shared.admission, actor_id(&body)?)?;
        let s = r.store()?;
        let world = s.snapshot()?;
        let page = objectflow_core::export::export_page(
            &world,
            source,
            text(&body, "cursor")?,
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
            actor_id(&body)?,
            text(&body, "subscription")?,
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
/// The principal the actor acts for, which the store records and checks as
/// it checks the actor (library-api.md §3, ADR-0182): the request's own
/// `principal`, as `wire.md` lists it, or the actor's, which must agree; the
/// request's was taken and dropped, so a principal who had left was never
/// refused (third production review PRINCIPAL).
fn principal_of(body: &J) -> Result<Option<String>, Fault> {
    let own = text(body, "principal")?;
    let actors = object(body, "actor")?
        .map(|_| text(&body["actor"], "principal"))
        .transpose()?
        .flatten();
    match (own, actors) {
        (Some(a), Some(b)) if a != b => Err(Fault::Bad(format!(
            "principal {a} and actor.principal {b} name different principals; send one"
        ))),
        (a, b) => Ok(a.or(b).map(str::to_string)),
    }
}

/// The most bytes an idempotency key holds, and a request's context.
const KEY_BYTES: usize = 256;
const CONTEXT_BYTES: usize = 16 * 1024;

fn request_of(body: &J) -> Result<Request, Fault> {
    // the key that makes a retry safe, scoped to the actor (library-api.md §3)
    let idempotency_key = match body.get("idempotency_key") {
        None | Some(J::Null) => None,
        // a key is a retry's handle, kept per actor: one of a megabyte was
        // stored as given (third production review, minor)
        Some(J::String(k)) if k.len() > KEY_BYTES => {
            return Err(Fault::Bad(format!(
                "idempotency_key takes at most {KEY_BYTES} bytes"
            )));
        }
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
        match text(body, "expires_at")? {
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
    let (Some(transition), Some(actor)) = (text(body, "transition")?, actor_id(body)?) else {
        return Err(Fault::Bad(
            "a request names its transition and its actor".to_string(),
        ));
    };
    let object_id = text(body, "object_id")?.map(str::to_string);
    let type_ = text(body, "type")?.map(str::to_string);
    if object_id.is_some() == type_.is_some() {
        return Err(Fault::Bad(
            "a request names an object_id or, to create, a type".to_string(),
        ));
    }
    let occurred_at =
        match text(body, "occurred_at")? {
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
        principal: principal_of(body)?,
        idempotency_key,
        // why the request was sent and by which route (library-api.md §3)
        context: match body.get("context") {
            None | Some(J::Null) => None,
            // recorded on each event the request writes: 1.9 MB was stored
            // as given (third production review, minor)
            Some(J::String(c)) if c.len() > CONTEXT_BYTES => {
                return Err(Fault::Bad(format!(
                    "context takes at most {CONTEXT_BYTES} bytes"
                )));
            }
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
    let field = |k: &str| -> Result<Option<String>, Fault> { Ok(text(o, k)?.map(str::to_string)) };
    let (Some(type_), Some(legacy_key), Some(state)) =
        (field("type")?, field("legacy_key")?, field("state")?)
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
    for (dimension, e) in object(o, "entered")?.into_iter().flatten() {
        let Some(at) = instant(e.get("at"), "entered.at")? else {
            return Err(Fault::Bad(format!(
                "entered.{dimension} names its time, at"
            )));
        };
        entered.insert(
            dimension.clone(),
            EnteredAt {
                at,
                by: text(e, "by")?.map(str::to_string),
                by_kind: text(e, "by_kind")?.map(str::to_string),
            },
        );
    }
    // its history from before the store (ADR-0163 decision 2)
    let mut legacy_entries = Vec::new();
    for e in array(o, "legacy_entries")?.into_iter().flatten() {
        let (Some(kind), Some(occurred_at)) = (
            text(e, "kind")?.map(str::to_string),
            instant(e.get("occurred_at"), "legacy_entries.occurred_at")?,
        ) else {
            return Err(Fault::Bad(format!(
                "a legacy entry of {type_} {legacy_key} names its kind and occurred_at"
            )));
        };
        let payload = object(e, "payload")?.cloned().unwrap_or_default();
        let mut about = std::collections::BTreeMap::new();
        for (field, target) in object(e, "about")?.into_iter().flatten() {
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
    for l in array(o, "legacy_intervals")?.into_iter().flatten() {
        let (Some(dimension), Some(entered_at)) = (
            text(l, "dimension")?,
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
            entered_by: text(l, "entered_by")?.map(str::to_string),
            entered_by_kind: text(l, "entered_by_kind")?.map(str::to_string),
        });
    }
    let mut silent_spans = Vec::new();
    for span in array(o, "silent_spans")?.into_iter().flatten() {
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
        attributes: object(o, "attributes")?
            .map(|m| m.clone().into_iter().collect())
            .unwrap_or_default(),
        entered,
        created_at: instant(o.get("created_at"), "created_at")?,
        created_by_kind: field("created_by_kind")?,
        occurred_at: instant(o.get("occurred_at"), "occurred_at")?,
        legacy_entries,
        // everything a batch carries is imported (ADR-0154 decision 6)
        beyond: Vec::new(),
    })
}

fn violation_class(c: &J) -> Result<ViolationClass, Fault> {
    let field = |k: &str| -> Result<Option<String>, Fault> { Ok(text(c, k)?.map(str::to_string)) };
    let kind = field("kind")?.as_deref().and_then(ViolationKind::parse);
    let disposition = field("disposition")?
        .as_deref()
        .map_or(Some(Disposition::Undecided), Disposition::parse);
    let (Some(type_), Some(kind), Some(rule), Some(shape), Some(disposition)) = (
        field("type")?,
        kind,
        field("rule")?,
        field("shape")?,
        disposition,
    ) else {
        return Err(Fault::Bad(
            "a class names its type, kind, rule and shape, and a disposition of publish-and-import.md §6".to_string(),
        ));
    };
    Ok(ViolationClass {
        id: field("id")?.unwrap_or_default(),
        type_,
        kind,
        rule,
        shape,
        count: match int(c, "count")? {
            None => 0,
            Some(n) => usize::try_from(n)
                .map_err(|_| Fault::Bad("count takes a count, 0 or more".to_string()))?,
        },
        sample: array(c, "sample")?
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        disposition,
        decided_by: field("decided_by")?,
        reason: field("reason")?,
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
        let Some(actor) = actor_id(&body)? else {
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
        for o in array(b, "objects")?.into_iter().flatten() {
            batch.objects.push(imported_object(o)?);
        }
        // each entry kind's fields kept through an erasure (ADR-0163 decision 3)
        for (kind, fields) in object(b, "legacy_kinds")?.into_iter().flatten() {
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
        for r in array(b, "sequences")?.into_iter().flatten() {
            let (Some(sequence), Some(last)) = (text(r, "sequence")?, int(r, "last")?) else {
                return Err(Fault::Bad(
                    "a sequence names its sequence and the last value minted".to_string(),
                ));
            };
            batch.sequences.push(Raise {
                sequence: sequence.to_string(),
                scope: text(r, "scope")?.unwrap_or_default().to_string(),
                last,
            });
        }
        let mut file = Vec::new();
        for c in array(&body, "disposition")?.into_iter().flatten() {
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
            text(&body, "id")?.ok_or_else(|| Fault::Bad("get names an object's id".to_string()))?;
        let follow = flag(&body, "follow")?;
        let mut r = reading(&shared)?;
        let s = r.store()?;
        // with follow, where its supersession chain ends: taken and ignored
        // until the third production review read it (DESIGN.md §5)
        if follow {
            let end = reads::followed(&s.snapshot()?, id)?;
            return object_json(s, &end);
        }
        object_json(s, id)
    })
}

/// A change's dry run, its report the impact a publish would have, or its
/// approval, which installs it (`library-api.md` §6, ADR-0155).
async fn publish(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let (Some(actor), Some(change)) = (actor_id(&body)?, text(&body, "change")?) else {
            return Err(Fault::Bad(
                "publish names its actor and the change".to_string(),
            ));
        };
        if flag(&body, "dry_run")? {
            // a preview is a read: a deployment's runs beside the writes,
            // under the read's deadline, where it held every write (third
            // production review PUBLISH-SCALE); a sandbox's reads take the
            // writer, as every one of its reads does
            if shared.reads.is_some() {
                let (_slot, mut r) = heavy_reading(&shared, &shared.dry_runs, actor_id(&body)?)?;
                let s = r.store()?;
                return preview(&s.snapshot()?, actor, change).map_err(|e| match e {
                    Fault::TimedOut(_) => dry_run_late(&shared),
                    other => other,
                });
            }
            let mut sandbox = locked(&shared)?;
            let s = store(&mut sandbox)?;
            return preview(&s.snapshot()?, actor, change);
        }
        let mut sandbox = locked(&shared)?;
        let s = store(&mut sandbox)?;
        // the version of the change the approver read, required (ADR-0101)
        let Some(expected) = int(&body, "expected_version")? else {
            return Err(Fault::Bad(
                "a publish names the expected_version of the change it approves".to_string(),
            ));
        };
        let (v, report) = s.publish(actor, change, expected)??;
        // a publish's own refusal, current, fatal, impact_unchanged or
        // decision_unresolved, names no object and takes no proposal; one by
        // the kinds that approve a version names the change, as a
        // transition's taken_by names its object (ADR-0187 decision 1)
        let told = match &v {
            Verdict::InvariantViolated { failures, .. } => Told {
                objects: failures
                    .iter()
                    .filter_map(|f| f.object_id.clone())
                    .collect(),
                ..Told::default()
            },
            _ => Told::default(),
        };
        Ok(Json(json!({
            "verdict": verdict_told(&v, Some(&told)),
            "report": report.as_ref().map(objectflow_core::publish::report_json),
        })))
    })
}

/// A change's dry run stopped at the read's deadline, said as such: a
/// submit's and a preview's run on a read beside the writes, and are never
/// computed again under the lock (PUBLISH-SCALE).
fn dry_run_late(shared: &Shared) -> Fault {
    Fault::TimedOut(format!(
        "the change's dry run passed --read-timeout ({} s) and was stopped; on a store this size a publish needs a longer one",
        shared.read_timeout.as_secs()
    ))
}

/// A change's dry run as a publish previews it, over the store as `world`
/// reads it.
fn preview(world: &objectflow_core::world::World<'_>, actor: &str, change: &str) -> Answer {
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
        world,
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
async fn lookup(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let (Some(source), Some(value)) = (text(&body, "source")?, text(&body, "value")?) else {
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
        let id = text(&body, "id")?
            .ok_or_else(|| Fault::Bad("history names an object's id".to_string()))?;
        let follow = flag(&body, "follow")?;
        let limit = limit_of(&body)?;
        // a page goes on after the last event given, by its position, as a
        // pull's does (ADR-0192 decision 5)
        let after = match text(&body, "cursor")? {
            None => None,
            Some(c) => Some(c.parse::<i64>().ok().filter(|p| *p >= 0).ok_or_else(|| {
                Fault::Bad(format!(
                    "history's cursor is the position its last page ended at: {c}"
                ))
            })?),
        };
        let (_slot, mut r) = heavy_reading(&shared, &shared.worklists, actor_id(&body)?)?;
        let s = r.store()?;
        // an object that does not exist has no history, null as get's (ADR-0173)
        if reads::get(&s.snapshot()?, id)?.is_none() {
            return Ok(Json(J::Null));
        }
        // with follow, the whole group supersession joins, every member
        // whose chain ends where this one's does (library-api.md §5)
        let members = if follow {
            reads::superseded_group(&s.snapshot()?, id)?
        } else {
            vec![id.to_string()]
        };
        // their legacy entries on the first page, which predate the store,
        // by member, then their events in position order, a page of them
        // (library-api.md §6, ADR-0015)
        let mut entries = Vec::new();
        if after.is_none() {
            for m in &members {
                entries.extend(s.legacy_entries(m)?.into_iter().map(|e| {
                    json!({"legacy": true, "object_id": m, "ordinal": e.ordinal, "kind": e.kind,
                       "occurred_at": timestamp_text(e.occurred_at), "payload": e.payload})
                }));
            }
        }
        let events = s.history_page(&members, after.unwrap_or(0), limit)?;
        let cursor = (events.len() == limit)
            .then(|| events.last().map(|e| e.position.to_string()))
            .flatten();
        Ok(Json(json!({
            "events": entries.into_iter().chain(events.iter().map(event)).collect::<Vec<_>>(),
            "cursor": cursor,
            "not_computed": EVENT_NOT_COMPUTED,
        })))
    })
}

async fn events(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let Some(positions) = array(&body, "positions")?
            .and_then(|ps| ps.iter().map(J::as_i64).collect::<Option<Vec<i64>>>())
        else {
            return Err(Fault::Bad(
                "events takes positions, a list of integers".to_string(),
            ));
        };
        // a page's worth at most, where one position repeated 200,000 times
        // built an answer of gigabytes (second production review)
        if positions.len() > reads::PAGE_LIMIT {
            return Err(Fault::Bad(format!(
                "events takes at most {} positions",
                reads::PAGE_LIMIT
            )));
        }
        // each position once, where one named a thousand times answered a
        // 7 kB request with 201 MB (fifth production review, ADR-0190
        // decision 6)
        let mut seen = std::collections::BTreeSet::new();
        if let Some(twice) = positions.iter().find(|p| !seen.insert(**p)) {
            return Err(Fault::Bad(format!(
                "events takes each position once, not {twice} twice"
            )));
        }
        // waiting its turn, as a worklist does: a thousand large positions
        // answered 16.5 MB, eight at once 572 MB (ADR-0192 decision 5)
        let (_slot, mut r) = heavy_reading(&shared, &shared.worklists, actor_id(&body)?)?;
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
        let kind = text(&body, "actor_kind")?
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
        let name = text(&body, "type")?.unwrap_or_default();
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
        let (Some(actor), Some(id)) = (actor_id(&body)?, text(&body, "id")?) else {
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
        let (Some(_actor), Some(type_)) = (actor_id(&body)?, text(&body, "type")?) else {
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
        let (_slot, mut r) = heavy_reading(&shared, &shared.worklists, actor_id(&body)?)?;
        let s = r.store()?;
        let world = s.snapshot()?;
        let page = reads::query_page(
            &world,
            type_,
            text(&body, "filter")?,
            &order,
            fields.as_deref(),
            text(&body, "cursor")?,
            limit,
        )?;
        Ok(Json(page_json(&page)))
    })
}

async fn metric(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let (Some(_actor), Some(name)) = (actor_id(&body)?, text(&body, "name")?) else {
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
        let bind = object(&body, "bind")?
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
        let over = match text(&body, "over")? {
            None => None,
            // a window back from now is positive, where one of "-5 days"
            // answered an empty page (fifth production review, ADR-0191
            // decision 3)
            Some(by) => Some(parse_duration(by).filter(|s| *s > 0).ok_or_else(|| {
                Fault::Bad(format!(
                    "over takes a positive duration such as '30 days': {by}"
                ))
            })?),
        };
        let (_slot, mut r) = heavy_reading(&shared, &shared.admission, actor_id(&body)?)?;
        let s = r.store()?;
        let world = s.snapshot()?;
        let page = objectflow_core::metric::page(
            &world,
            Some(&s.answers),
            name,
            &bind,
            keep.as_deref(),
            over,
            text(&body, "filter")?,
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
async fn health(State(shared): State<Shared>) -> Result<Response, Fault> {
    // a store due an upgrade serves nothing but the upgrade, so its health
    // is 503, with what is due (OBSERVE-2)
    let due = |body: J| Ok((StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response());
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
                return due(
                    json!({"release": BUILTINS, "shape": "deployment", "version": J::Null,
                                  "now": J::Null, "store_release": at, "upgrade_due": true,
                                  "code": "upgrade_due"}),
                );
            }
            // a store file moved, gone or replaced under the service, said
            // before the path is read, which reads the file that replaced
            // it, and answered 503 as a write is, where one gone answered
            // 500 (fourth and fifth production reviews, ADR-0191 decision 4)
            if let Some(replaced) = store_replaced(&shared) {
                let body = json!({"release": BUILTINS, "shape": "deployment",
                                  "code": fault_code(503, &replaced), "fault": replaced});
                return Ok((StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response());
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
            let free = disk_free(&reads.path);
            let mut body = json!({"release": BUILTINS, "shape": "deployment", "version": version,
                              "now": timestamp_text(now), "store_release": BUILTINS,
                              "upgrade_due": false, "clock_lead_seconds": clock_lead(&shared),
                              "writer_held_seconds": writer_held(&shared),
                              "requests_in_flight": shared.in_flight.load(Ordering::SeqCst),
                              "connections_open": shared.connections.open.load(Ordering::SeqCst),
                              "disk_free_bytes": free});
            // a volume too full for a checkpoint answers 503, said, where a
            // full disk answered 200 and a write then failed (ADR-0188
            // decision 3); and a store file moved or replaced, a log no one
            // reads, and writes stopped behind another process's lock, each
            // answered 200 (fourth production review)
            let now = wall_millis();
            let fault = if let Some(bytes) = free.filter(|b| disk_low(*b)) {
                Some(format!(
                    "disk_low: {bytes} byte(s) free on the store's volume, below the {DISK_FLOOR} a checkpoint needs"
                ))
            } else if disk_full_since(shared.watch.written_at.load(Ordering::SeqCst)) {
                // a volume full though its free space did not show it, a
                // quota's or a reserve's, said until a write commits
                // (sixth production review, ADR-0192 decision 7)
                Some(
                    "disk_full: the last write failed for a full volume, and none has committed since"
                        .to_string(),
                )
            } else if log_blocked(LOGGING_SINCE.load(Ordering::SeqCst), now) {
                Some(format!(
                    "log_blocked: a line of the log has waited more than {} s to be written; its lines are dropped until it is read",
                    LOG_BLOCKED_MS / 1000
                ))
            } else if shared.watch.stalled(now) {
                Some(
                    "writes_stalled: the last write met another process's lock past its retries, and none has committed since"
                        .to_string(),
                )
            } else {
                None
            };
            if let Some(fault) = fault {
                body["code"] = json!(fault_code(503, &fault));
                body["fault"] = json!(fault);
                return Ok((StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response());
            }
            return Ok(Json(body).into_response());
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
            return due(
                json!({"release": BUILTINS, "shape": shape, "version": J::Null,
                              "now": J::Null, "store_release": at, "upgrade_due": true}),
            );
        }
        let (version, now) = match sandbox.store {
            Some(_) => {
                let s = store(&mut sandbox)?;
                (s.lib.version, J::from(timestamp_text(s.now)))
            }
            None => (0, J::Null),
        };
        // a log no one reads, said whatever the shape (fourth production
        // review)
        if log_blocked(LOGGING_SINCE.load(Ordering::SeqCst), wall_millis()) {
            let fault = format!(
                "log_blocked: a line of the log has waited more than {} s to be written; its lines are dropped until it is read",
                LOG_BLOCKED_MS / 1000
            );
            let body = json!({"release": BUILTINS, "shape": shape, "version": version, "now": now,
                              "code": fault_code(503, &fault), "fault": fault});
            return Ok((StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response());
        }
        Ok(Json(
            json!({"release": BUILTINS, "shape": shape, "version": version, "now": now,
               "store_release": BUILTINS, "upgrade_due": false, "clock_lead_seconds": 0,
               "writer_held_seconds": 0, "requests_in_flight": shared.in_flight.load(Ordering::SeqCst),
               "connections_open": shared.connections.open.load(Ordering::SeqCst)}),
        )
        .into_response())
    })
}

/// `upgrade` (`storage-schema.md` §12, ADR-0177): a deployment's store
/// Whether an actor names an operator of the store, by its id or its
/// identity, as an upgrade reads it (ADR-0177).
fn is_operator(world: &objectflow_core::world::World<'_>, actor: &str) -> Result<bool, NotRun> {
    use objectflow_core::model::OPERATOR_TYPE;
    if world.get(actor)?.is_some_and(|o| o.type_ == OPERATOR_TYPE) {
        return Ok(true);
    }
    Ok(world.of_type(OPERATOR_TYPE)?.iter().any(
        |o| matches!(o.attr("identity"), objectflow_core::value::Value::Text(t) if t == actor),
    ))
}

/// carried from its release to this core's, the operator's act, or with
/// `dry_run` every step run and rolled back; once upgraded the store is
/// opened and served. A store at this release already answers that
/// nothing is due; a sandbox's store is always this release's.
async fn upgrade(State(shared): State<Shared>, Body(body): Body) -> Answer {
    tokio::task::block_in_place(move || {
        let Some(actor) = actor_id(&body)? else {
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
            // an operator's act whether or not one is due: another actor was
            // answered that nothing was due (third production review)
            let s = store(&mut sandbox)?;
            if !is_operator(&s.snapshot()?, actor)? {
                return Err(Fault::Conflict(format!(
                    "an upgrade is an operator's act, and {actor} is no operator of the store"
                )));
            }
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
    // counted while it is answered, a panic's unwinding included
    struct Flying<'a>(&'a std::sync::atomic::AtomicUsize);
    impl Drop for Flying<'_> {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    shared.in_flight.fetch_add(1, Ordering::SeqCst);
    let _flying = Flying(&shared.in_flight);
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
    shared.watch.timed(&route, took);
    // what a publish, an import or a long read freed, given back to the
    // system, which glibc kept: 975 MB held after one publish on 37k
    // objects, 23 MB once given back (third production review MEMORY-2)
    if route == "/v1/publish" || route == "/v1/import_batch" || took >= HEAVY {
        give_back(&shared);
    }
    let quiet = (route == "/health" && status == 200) || route == "/metrics";
    if !quiet {
        let lead = clock_lead(&shared);
        say!(
            "{method} {route} {status} {:.1} ms{}",
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

/// The free bytes below which `/health` answers 503: the space a checkpoint
/// needs at the write-ahead log's limit (ADR-0188 decision 3).
const DISK_FLOOR: u64 = objectflow_core::store::WAL_LIMIT.unsigned_abs();

/// Whether a volume holds too little for a checkpoint.
fn disk_low(free: u64) -> bool {
    free < DISK_FLOOR
}

/// The bytes an unprivileged writer has free on the volume a path is on,
/// read through `statvfs`; none where it cannot be read (ADR-0188 decision
/// 3).
#[cfg(target_os = "linux")]
#[allow(unsafe_code, clippy::unnecessary_cast)]
fn disk_free(path: &std::path::Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st = std::mem::MaybeUninit::<libc::statvfs>::zeroed();
    // SAFETY: `c` is a NUL-terminated path that outlives the call, and `st`
    // points to a statvfs-sized buffer, zeroed, which the call fills on
    // success; neither is kept past it.
    let rc = unsafe { libc::statvfs(c.as_ptr(), st.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    // SAFETY: the buffer was zeroed, a valid statvfs of integers, and the
    // call that returned 0 filled it.
    let st = unsafe { st.assume_init() };
    // the field types differ between targets, hence the casts
    Some((st.f_bavail as u64).saturating_mul(st.f_frsize as u64))
}

/// Elsewhere the volume is not read.
#[cfg(not(target_os = "linux"))]
fn disk_free(_path: &std::path::Path) -> Option<u64> {
    None
}

/// How long a request runs, in seconds, before what it freed is given back
/// after it (ADR-0187 decision 2).
const HEAVY: f64 = 0.5;

/// The memory the allocator keeps of what was freed, given back to the
/// system on a blocking thread, one giving back at a time, after the answer
/// that freed it.
fn give_back(shared: &Shared) {
    if shared.trimming.swap(true, Ordering::SeqCst) {
        return;
    }
    let shared = shared.clone();
    tokio::task::spawn_blocking(move || {
        trim();
        shared.trims.fetch_add(1, Ordering::SeqCst);
        shared.trimming.store(false, Ordering::SeqCst);
    });
}

/// glibc's `malloc_trim(0)`: every free page of every arena returned, which
/// its tunables never do for pages amid an arena (ADR-0187 decision 2).
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[allow(unsafe_code)]
fn trim() {
    // SAFETY: `malloc_trim` takes a size by value and touches only the
    // allocator's own state, under its own locks, so any thread may call
    // it at any time; it reads no pointer of ours and its result, whether
    // memory was released, needs no handling.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// Elsewhere the allocator is not glibc's, and nothing is trimmed.
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn trim() {}

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
    out.push_str(&format!(
        "# HELP objectflow_requests_in_flight Requests being answered now.\n\
         # TYPE objectflow_requests_in_flight gauge\nobjectflow_requests_in_flight {}\n\
         # HELP objectflow_writer_held_seconds How long the writer has been held, 0 while free.\n\
         # TYPE objectflow_writer_held_seconds gauge\nobjectflow_writer_held_seconds {:.3}\n",
        shared.in_flight.load(Ordering::SeqCst),
        writer_held(&shared)
    ));
    // each verdict's kind, a refusal's among them, which an answer's status
    // does not tell, each route's durations, each evaluator's failed calls
    // and the log's dropped lines (fourth production review)
    out.push_str(
        "# HELP objectflow_verdicts_total Requests' verdicts, by kind.\n\
         # TYPE objectflow_verdicts_total counter\n",
    );
    if let Ok(kinds) = shared.watch.verdicts.lock() {
        for (kind, n) in kinds.iter() {
            out.push_str(&format!(
                "objectflow_verdicts_total{{kind=\"{kind}\"}} {n}\n"
            ));
        }
    }
    out.push_str(
        "# HELP objectflow_request_duration_seconds How long answers took, by route.\n\
         # TYPE objectflow_request_duration_seconds histogram\n",
    );
    if let (Ok(durations), Ok(served)) = (shared.watch.durations.lock(), shared.served.lock()) {
        for (route, row) in durations.iter() {
            for (bound, n) in BUCKETS.iter().zip(row) {
                out.push_str(&format!(
                    "objectflow_request_duration_seconds_bucket{{route=\"{route}\",le=\"{bound}\"}} {n}\n"
                ));
            }
            let (count, sum) = served
                .iter()
                .filter(|((r, _), _)| r == route)
                .fold((0, 0.0), |(c, s), (_, (n, t))| (c + n, s + t));
            out.push_str(&format!(
                "objectflow_request_duration_seconds_bucket{{route=\"{route}\",le=\"+Inf\"}} {count}\n\
                 objectflow_request_duration_seconds_sum{{route=\"{route}\"}} {sum:.6}\n\
                 objectflow_request_duration_seconds_count{{route=\"{route}\"}} {count}\n"
            ));
        }
    }
    out.push_str(
        "# HELP objectflow_evaluator_failures_total Calls an evaluator answered nothing usable to, by evaluator.\n\
         # TYPE objectflow_evaluator_failures_total counter\n",
    );
    if let Ok(failures) = EVALUATOR_FAILURES.lock() {
        for (evaluator, n) in failures.iter() {
            out.push_str(&format!(
                "objectflow_evaluator_failures_total{{evaluator=\"{evaluator}\"}} {n}\n"
            ));
        }
    }
    out.push_str(&format!(
        "# HELP objectflow_wal_cuts_total The write-ahead log's cuts back past its limit or after an erasure.\n\
         # TYPE objectflow_wal_cuts_total counter\nobjectflow_wal_cuts_total {}\n\
         # HELP objectflow_wal_cuts_missed_total Cuts a read held the log through.\n\
         # TYPE objectflow_wal_cuts_missed_total counter\nobjectflow_wal_cuts_missed_total {}\n\
         # HELP objectflow_wal_cuts_deferred_total Cuts left for the next look, a read running past a moment's hold.\n\
         # TYPE objectflow_wal_cuts_deferred_total counter\nobjectflow_wal_cuts_deferred_total {}\n",
        shared.watch.cuts.load(Ordering::SeqCst),
        shared.watch.cuts_missed.load(Ordering::SeqCst),
        shared.watch.cuts_deferred.load(Ordering::SeqCst)
    ));
    out.push_str(&format!(
        "# HELP objectflow_log_lines_dropped_total Lines not written while the log was blocked.\n\
         # TYPE objectflow_log_lines_dropped_total counter\nobjectflow_log_lines_dropped_total {}\n",
        LINES_DROPPED.load(Ordering::SeqCst)
    ));
    // the reads that grow with the store, bounded in number (ADR-0189)
    // by pool, the worklists', the analytics' and the dry runs' (ADR-0190)
    out.push_str(
        "# HELP objectflow_reads_running Reads that grow with the store running now, by pool.\n\
         # TYPE objectflow_reads_running gauge\n\
         # HELP objectflow_reads_waiting Reads that grow with the store waiting for their turn, by pool.\n\
         # TYPE objectflow_reads_waiting gauge\n\
         # HELP objectflow_reads_most How many reads of a pool run at once.\n\
         # TYPE objectflow_reads_most gauge\n",
    );
    for (pool, a) in [
        ("worklists", &shared.worklists),
        ("analytics", &shared.admission),
        ("dry_runs", &shared.dry_runs),
        ("agents", &shared.agents),
    ] {
        out.push_str(&format!(
            "objectflow_reads_running{{pool=\"{pool}\"}} {}\n\
             objectflow_reads_waiting{{pool=\"{pool}\"}} {}\n\
             objectflow_reads_most{{pool=\"{pool}\"}} {}\n",
            a.running(),
            a.waiting.load(Ordering::SeqCst),
            a.most
        ));
    }
    let c = &shared.connections;
    out.push_str(&format!(
        "# HELP objectflow_connections_open The connections open.\n\
         # TYPE objectflow_connections_open gauge\nobjectflow_connections_open {}\n\
         # HELP objectflow_connections_made_room_total Connections answering nothing closed to make room at the cap.\n\
         # TYPE objectflow_connections_made_room_total counter\nobjectflow_connections_made_room_total {}\n\
         # HELP objectflow_connections_refused_total Connections refused at the cap, every one open answering a request.\n\
         # TYPE objectflow_connections_refused_total counter\nobjectflow_connections_refused_total {}\n",
        c.open.load(Ordering::SeqCst),
        c.made_room.load(Ordering::SeqCst),
        c.refused.load(Ordering::SeqCst)
    ));
    // the store's file and its write-ahead log, which a checkpoint cuts
    // back to its limit, and the volume they are on (N6, ADR-0188)
    if let Some(free) = shared.store_path.as_deref().and_then(disk_free) {
        out.push_str(&format!(
            "# HELP objectflow_disk_free_bytes Free bytes on the store's volume.\n\
             # TYPE objectflow_disk_free_bytes gauge\nobjectflow_disk_free_bytes {free}\n"
        ));
    }
    if let Some(path) = &shared.store_path {
        let size = |p: &std::path::Path| std::fs::metadata(p).map_or(0, |m| m.len());
        let mut wal = path.clone().into_os_string();
        wal.push("-wal");
        out.push_str(&format!(
            "# HELP objectflow_store_bytes The store's file.\n\
             # TYPE objectflow_store_bytes gauge\nobjectflow_store_bytes {}\n\
             # HELP objectflow_wal_bytes The store's write-ahead log.\n\
             # TYPE objectflow_wal_bytes gauge\nobjectflow_wal_bytes {}\n",
            size(path),
            size(std::path::Path::new(&wal))
        ));
    }
    out.push_str(&format!(
        "# HELP objectflow_memory_given_back_total Times the memory a heavy request freed was given back to the system.\n\
         # TYPE objectflow_memory_given_back_total counter\nobjectflow_memory_given_back_total {}\n",
        shared.trims.load(Ordering::SeqCst)
    ));
    if let Some(bytes) = resident_bytes() {
        out.push_str(&format!(
            "# HELP objectflow_resident_bytes The process's resident memory.\n\
             # TYPE objectflow_resident_bytes gauge\nobjectflow_resident_bytes {bytes}\n"
        ));
    }
    // which release answers, and since when, so a restart shows (fifth
    // production review, ADR-0191 decision 4)
    out.push_str(&format!(
        "# HELP objectflow_build_info The release serving, always 1.\n\
         # TYPE objectflow_build_info gauge\nobjectflow_build_info{{release=\"{BUILTINS}\",commit=\"{}\"}} 1\n\
         # HELP objectflow_start_time_seconds When the service started, in seconds since 1970.\n\
         # TYPE objectflow_start_time_seconds gauge\nobjectflow_start_time_seconds {}\n",
        option_env!("OBJECTFLOW_COMMIT").unwrap_or("unknown"),
        STARTED_AT.load(Ordering::SeqCst)
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
                          [--idle-timeout <duration>] [--max-connections <n>]
                          [--metrics-bind <host:port>]
                          [--store <file> [--create]
                                          [--attempt-retention <duration>]
                                          [--idempotency-retention <duration>]
                                          [--evaluator <name>=<url> ...]
                                          [--evaluator-timeout <duration>]
                                          [--evaluator-token <name>=<file> ...]
                                          [--busy-timeout <duration>]
                                          [--read-timeout <duration>]
                                          [--max-reads <n>]]

Serves the engine's operations, POST /v1/<operation>, GET /health and GET
/metrics, on 127.0.0.1 at port n (8712 unless given; 0 takes a free one), at
the address --bind names, or on the Unix socket --socket names, made 0660 so
its owner and group alone reach it (ADR-0183); it writes where it listens
to stderr once it answers, and then one line for each answer, each line
stamped; --metrics-bind serves GET /metrics and GET /health alone on an
address of their own; under systemd it tells a Type=notify unit when it is
ready and when it stops.

Without --store the store is a sandbox's, held in memory, with its controls,
POST /sandbox/<control>. With --store it is a deployment's: the SQLite file
named, which must hold a store unless --create is given, which makes one at
version 0, its files 0600, whose flow is published through the wire; the
wall clock; and no controls (ADR-0174, ADR-0183). Each --evaluator names
the endpoint a namespace's calls are posted to, answered within the timeout,
5 s unless given, and an evaluator no endpoint answers is unknown
(ADR-0175), sent the token each --evaluator-token's file holds (ADR-0188).
Its reads run beside its writes, each on a connection of its own, stopped
at the read timeout, 30 s unless given, those that grow with the store at
most n at once, 4 unless given, a read waiting for its turn within its
timeout (ADR-0189); and its writes one at a time, each waiting out another process's
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
    /// Each evaluator's bearer token, read from its file at start, never
    /// said (ADR-0188 decision 2).
    tokens: std::collections::BTreeMap<String, String>,
    evaluator_timeout: Option<std::time::Duration>,
    busy_timeout: Option<std::time::Duration>,
    read_timeout: Option<std::time::Duration>,
    /// How long a connection may wait for a request's head, and a request
    /// for its body; and how many connections are served at once
    /// (ADR-0184 decision 4).
    idle_timeout: std::time::Duration,
    max_connections: usize,
    /// How many reads that grow with the store run at once (ADR-0189
    /// decision 2).
    max_reads: Option<usize>,
    /// An address `/metrics` and `/health` alone are served on (fourth
    /// production review).
    metrics_bind: Option<String>,
}

/// The reads that grow with the store run at once unless `--max-reads`
/// says otherwise (ADR-0189 decision 2).
const MAX_READS: usize = 4;

fn args() -> Result<Args, String> {
    let mut out = Args {
        bind: "127.0.0.1:8712".to_string(),
        socket: None,
        store: None,
        create: false,
        retentions: (None, None),
        endpoints: std::collections::BTreeMap::new(),
        tokens: std::collections::BTreeMap::new(),
        evaluator_timeout: None,
        busy_timeout: None,
        read_timeout: None,
        idle_timeout: std::time::Duration::from_secs(30),
        max_connections: 512,
        max_reads: None,
        metrics_bind: None,
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
            "--evaluator-token" => {
                let named = value()?;
                let (name, file) = named
                    .split_once('=')
                    .filter(|(n, f)| !n.is_empty() && !f.is_empty())
                    .ok_or_else(|| format!("--evaluator-token takes <name>=<file>: {named}"))?;
                out.tokens
                    .insert(name.to_string(), token_from(std::path::Path::new(file))?);
            }
            "--evaluator-timeout" => {
                let d = value()?;
                let secs = parse_duration(&d).filter(|s| *s > 0).ok_or_else(|| {
                    format!("--evaluator-timeout takes a duration such as '5 s': {d}")
                })?;
                out.evaluator_timeout = Some(std::time::Duration::from_secs(secs.unsigned_abs()));
            }
            "--idle-timeout" => {
                let d = value()?;
                let secs = parse_duration(&d).filter(|s| *s > 0).ok_or_else(|| {
                    format!("--idle-timeout takes a duration such as '30 s': {d}")
                })?;
                out.idle_timeout = std::time::Duration::from_secs(secs.unsigned_abs());
            }
            "--max-connections" => {
                let n = value()?;
                out.max_connections =
                    n.parse::<usize>().ok().filter(|n| *n > 0).ok_or_else(|| {
                        format!("--max-connections takes a count, 1 or more: {n}")
                    })?;
            }
            "--metrics-bind" => out.metrics_bind = Some(value()?),
            "--max-reads" => {
                let n = value()?;
                out.max_reads = Some(
                    n.parse::<usize>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or_else(|| format!("--max-reads takes a count, 1 or more: {n}"))?,
                );
            }
            "--read-timeout" => {
                let d = value()?;
                let secs = parse_duration(&d).filter(|s| *s > 0).ok_or_else(|| {
                    format!("--read-timeout takes a duration such as '30 s': {d}")
                })?;
                out.read_timeout = Some(std::time::Duration::from_secs(secs.unsigned_abs()));
            }
            "--busy-timeout" => {
                let d = value()?;
                let secs = parse_duration(&d)
                    .filter(|s| *s >= 0)
                    .ok_or_else(|| format!("--busy-timeout takes a duration such as '5 s': {d}"))?;
                out.busy_timeout = Some(std::time::Duration::from_secs(secs.unsigned_abs()));
            }
            "--help" | "-h" => return Err(String::new()),
            "--version" | "-V" => return Err(version()),
            // an argument it does not take is an error, never ignored (D558)
            other => return Err(format!("no argument {other}")),
        }
    }
    if addressed && out.socket.is_some() {
        return Err("--socket listens in the address's place; give one or the other".to_string());
    }
    if out.read_timeout.is_some() && out.store.is_none() {
        return Err(
            "a read's timeout is a deployment's; a sandbox's reads take the writer's lock"
                .to_string(),
        );
    }
    if out.max_reads.is_some() && out.store.is_none() {
        return Err(
            "how many reads run at once is a deployment's; a sandbox's reads take the writer's lock"
                .to_string(),
        );
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
    if let Some(name) = out.tokens.keys().find(|n| !out.endpoints.contains_key(*n)) {
        return Err(format!(
            "--evaluator-token names {name}, which no --evaluator gives"
        ));
    }
    Ok(out)
}

/// An evaluator's bearer token, from a file its owner alone reads: one
/// others may read is refused, as is an empty one (ADR-0188 decision 2).
fn token_from(file: &std::path::Path) -> Result<String, String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(file)
        .map_err(|e| format!("{}: {e}", file.display()))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        return Err(format!(
            "{} is readable by others than its owner (mode {:o}); an evaluator's token is its owner's alone, 0600",
            file.display(),
            mode & 0o777
        ));
    }
    let token = std::fs::read_to_string(file)
        .map_err(|e| format!("{}: {e}", file.display()))?
        .trim()
        .to_string();
    if token.is_empty() {
        return Err(format!("{} holds no token", file.display()));
    }
    Ok(token)
}

/// The stack each thread of the service runs a request on: an expression
/// nests at most `expr::MAX_NESTING` levels and its tree is at most
/// `expr::MAX_DEPTH` deep, and evaluating one level of a tree took about
/// 8 KiB of stack in a release build, measured 2026-10-11; 16 MiB holds
/// those many times over, where the runtime's default 2 MiB did not hold a
/// 256-level tree (F1).
const WORKER_STACK: usize = 16 * 1024 * 1024;

fn main() {
    let ran = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(WORKER_STACK)
        .enable_all()
        .build()
        .and_then(|rt| rt.block_on(serve()));
    // a start or a stop that fails says why in a sentence, never in Rust's
    // debug form (second production review, minor)
    if let Err(e) = ran {
        say!("objectflow-service: {e}");
        std::process::exit(1);
    }
}

/// What `--version` answers: the service's release and the release of the
/// stores it serves.
/// The release, and the commit it was built from where the build was told,
/// as a release's build is, so `0.2.0` names one build (sixth production
/// review, ADR-0192 decision 7).
fn version() -> String {
    match option_env!("OBJECTFLOW_COMMIT") {
        Some(commit) if !commit.is_empty() => format!("{VERSION}, commit {commit}"),
        _ => VERSION.to_string(),
    }
}

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
        notify("STOPPING=1");
        say!(
            "stopping: no new connection, and the requests in flight answered for at most {} s",
            GRACE.as_secs()
        );
        std::thread::spawn(|| {
            std::thread::sleep(GRACE);
            say!(
                "stopped with requests still in flight after {} s",
                GRACE.as_secs()
            );
            std::process::exit(1);
        });
    })
}

/// How often the write-ahead log is looked at, and cut back where due.
const CUT_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// How long a truncation waits for the short reads still open on the log.
const CUT_WAIT: std::time::Duration = std::time::Duration::from_secs(1);

/// How long a cut holds new reads that grow with the store, and gives those
/// running to end, before it is left for the next look or, urgent, stops
/// them (ADR-0194 decision 4).
const CUT_HOLD: std::time::Duration = std::time::Duration::from_secs(1);

/// How many times its limit the log grows before a cut stops the reads
/// running rather than wait for a moment none runs (ADR-0194 decision 4).
const WAL_URGENT: u64 = 4;

/// The write-ahead log looked at every `CUT_EVERY`, and cut back where due.
async fn keep_the_log_short(shared: Shared) {
    let mut every = tokio::time::interval(CUT_EVERY);
    loop {
        every.tick().await;
        let s = shared.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let _ = cut_back(&s, objectflow_core::store::WAL_LIMIT.unsigned_abs());
        })
        .await;
    }
}

/// The write-ahead log cut back where it holds more than `limit` bytes or
/// an erasure's truncation is still due: new reads that grow with the store
/// held for a moment, `CUT_HOLD`, for those running to end, and the log
/// truncated under the writer, the short reads waited for a moment. Where
/// one still runs, the cut is left for the next look and the held reads go
/// on, unless it is urgent, an erasure's truncation due or the log past
/// `WAL_URGENT` times its limit: then the reads running are stopped, each
/// answered `503 read_timeout`, and the log is cut. Reads that always
/// overlapped kept it growing until they paused, 3.2 GB in 150 s, and kept
/// an erasure's frames in it (fourth production review A-5, A-14); waiting
/// out the longest held every new read behind it, a person's worklist 25 s
/// behind an agent's sweep (seventh production review CUTSTALL, ADR-0194
/// decision 4). Whether it was cut.
fn cut_back(shared: &Service, limit: u64) -> bool {
    let Some(path) = &shared.store_path else {
        return false;
    };
    let mut wal = path.clone().into_os_string();
    wal.push("-wal");
    let bytes = std::fs::metadata(&wal).map_or(0, |m| m.len());
    let erasure_due = shared
        .writer
        .lock()
        .ok()
        .and_then(|w| w.store.as_ref().map(Store::checkpoint_due))
        .unwrap_or(false);
    if bytes <= limit && !erasure_due {
        return false;
    }
    let urgent = erasure_due || bytes > limit.saturating_mul(WAL_URGENT);
    let pools = [
        &shared.admission,
        &shared.worklists,
        &shared.dry_runs,
        &shared.agents,
    ];
    // every pool held a moment, one that does not drain in it holding the
    // rest too
    let moment = std::time::Instant::now() + CUT_HOLD;
    let mut drained = true;
    for p in &pools {
        drained &= p.hold(moment);
    }
    if !drained && !urgent {
        for p in pools {
            p.release();
        }
        shared.watch.cuts_deferred.fetch_add(1, Ordering::SeqCst);
        return false;
    }
    if !drained {
        // the reads running stopped at their next step, and waited for
        // within the read deadline
        shared.watch.cut_epoch.fetch_add(1, Ordering::SeqCst);
        let until = std::time::Instant::now() + shared.read_timeout;
        drained = true;
        for p in &pools {
            drained &= p.hold(until);
        }
    }
    let cut = drained
        && shared.writer.lock().ok().is_some_and(|mut w| {
            w.store
                .as_mut()
                .is_some_and(|s| s.truncate_log(CUT_WAIT).unwrap_or(false))
        });
    for p in pools {
        p.release();
    }
    if cut {
        shared.watch.cuts.fetch_add(1, Ordering::SeqCst);
    } else {
        shared.watch.cuts_missed.fetch_add(1, Ordering::SeqCst);
        say!(
            "the write-ahead log, {bytes} bytes, was not cut back: {}",
            if drained {
                "a read held it past the wait"
            } else {
                "a long read ran past the read deadline"
            }
        );
    }
    cut
}

/// The connections the metrics listener serves at once.
const METRICS_CONNECTIONS: usize = 16;

/// How often an idle service gives back what its requests freed.
const IDLE_TRIM: std::time::Duration = std::time::Duration::from_secs(60);

/// What the service's requests freed, given back to the system every
/// `IDLE_TRIM` that it answered something and has nothing in flight.
async fn idle_trims(shared: Shared) {
    let mut every = tokio::time::interval(IDLE_TRIM);
    let mut seen = 0;
    loop {
        every.tick().await;
        let answered = shared.watch.answered.load(Ordering::SeqCst);
        if trim_due(seen, answered, shared.in_flight.load(Ordering::SeqCst)) {
            give_back(&shared);
        }
        seen = answered;
    }
}

/// Whether an idle trim is due: something was answered since the last
/// look, and nothing is in flight.
fn trim_due(seen: u64, answered: u64, in_flight: usize) -> bool {
    answered != seen && in_flight == 0
}

/// A state told to systemd's notify socket where the unit names one,
/// `Type=notify`: `READY=1` once the service answers and `STOPPING=1` as it
/// stops, where a unit could only guess when the socket was ready (fourth
/// production review).
fn notify(state: &str) {
    let Some(path) = std::env::var_os("NOTIFY_SOCKET") else {
        return;
    };
    let Ok(socket) = std::os::unix::net::UnixDatagram::unbound() else {
        return;
    };
    let sent = match path.as_encoded_bytes().strip_prefix(b"@") {
        #[cfg(target_os = "linux")]
        Some(name) => {
            use std::os::linux::net::SocketAddrExt;
            std::os::unix::net::SocketAddr::from_abstract_name(name)
                .and_then(|at| socket.send_to_addr(state.as_bytes(), &at))
        }
        #[cfg(not(target_os = "linux"))]
        Some(_) => Ok(0),
        None => socket.send_to(state.as_bytes(), &path),
    };
    if let Err(e) = sent {
        say!("systemd was not told {state}: {e}");
    }
}

/// Where the service listens: an address, or a Unix socket.
enum Listening {
    Tcp(tokio::net::TcpListener),
    Unix(tokio::net::UnixListener),
}

impl Listening {
    async fn accept(&self) -> std::io::Result<Conn> {
        match self {
            Listening::Tcp(l) => l.accept().await.map(|(s, _)| Conn::Tcp(s)),
            Listening::Unix(l) => l.accept().await.map(|(s, _)| Conn::Unix(s)),
        }
    }
}

/// A connection accepted on either listener.
enum Conn {
    Tcp(tokio::net::TcpStream),
    Unix(tokio::net::UnixStream),
}

impl tokio::io::AsyncRead for Conn {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Tcp(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            Conn::Unix(s) => std::pin::Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl tokio::io::AsyncWrite for Conn {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Conn::Tcp(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            Conn::Unix(s) => std::pin::Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Tcp(s) => std::pin::Pin::new(s).poll_flush(cx),
            Conn::Unix(s) => std::pin::Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Conn::Tcp(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            Conn::Unix(s) => std::pin::Pin::new(s).poll_shutdown(cx),
        }
    }
}

/// A connection's requests being answered, each until its response's body
/// is written, or [`CLOSING`] once the cap took it to make room, when none
/// may begin (third production review C-1).
type Busy = Arc<std::sync::atomic::AtomicIsize>;
const CLOSING: isize = -1;

/// Whether a request may take its connection: not once the cap has taken it
/// to make room, which then begins nothing.
fn taken(busy: &std::sync::atomic::AtomicIsize) -> bool {
    busy.try_update(Ordering::SeqCst, Ordering::SeqCst, |b| {
        (b >= 0).then_some(b + 1)
    })
    .is_ok()
}

/// A request's hold on its connection, let go when its response's body is
/// written or dropped.
#[derive(Debug)]
struct Holding(Busy);

impl Drop for Holding {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A response's body, holding its connection until it is written, so the
/// cap never closes a connection whose answer has not left.
#[derive(Debug)]
struct Held {
    body: axum::body::Body,
    _holding: Option<Holding>,
}

impl hyper::body::Body for Held {
    type Data = hyper::body::Bytes;
    type Error = axum::Error;

    fn poll_frame(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        hyper::body::Body::poll_frame(std::pin::Pin::new(&mut self.get_mut().body), cx)
    }

    fn is_end_stream(&self) -> bool {
        hyper::body::Body::is_end_stream(&self.body)
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        hyper::body::Body::size_hint(&self.body)
    }
}

/// The router as one connection serves it: a request takes the connection
/// for as long as it is answered, and one arriving once the cap has taken
/// the connection is answered `503` and begins nothing, so closing a
/// connection to make room never cuts a request short.
#[derive(Clone, Debug)]
struct OnConnection {
    router: hyper_util::service::TowerToHyperService<Router>,
    busy: Busy,
}

impl hyper::service::Service<hyper::Request<hyper::body::Incoming>> for OnConnection {
    type Response = hyper::Response<Held>;
    type Error = std::convert::Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn call(&self, req: hyper::Request<hyper::body::Incoming>) -> Self::Future {
        if !taken(&self.busy) {
            let answer =
                busy("busy: this connection was closed to make room; send the request on another");
            return Box::pin(async move { Ok(answer) });
        }
        let holding = Holding(self.busy.clone());
        let answering = hyper::service::Service::call(&self.router, req);
        Box::pin(async move {
            let answer = answering.await?;
            Ok(answer.map(|body| Held {
                body,
                _holding: Some(holding),
            }))
        })
    }
}

/// A `503` that closes its connection, saying why the request was not
/// answered.
fn busy(fault: &str) -> hyper::Response<Held> {
    let mut answer = hyper::Response::new(Held {
        body: axum::body::Body::from(
            json!({ "fault": fault, "code": fault_code(503, fault) }).to_string(),
        ),
        _holding: None,
    });
    *answer.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
    let headers = answer.headers_mut();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        axum::http::header::CONNECTION,
        axum::http::HeaderValue::from_static("close"),
    );
    answer
}

/// At most this many connections past the cap are being told so at once,
/// each for a second at most, so a flood of them holds no more descriptors
/// than this beyond the cap.
const REFUSING: usize = 16;

/// Each connection served by hyper's HTTP/1 with a timer, so a connection
/// whose request's head is not read within the idle timeout, silent, idle
/// between requests or half-sent, is closed, which `axum::serve`, setting
/// no timer, never did; once the service is stopping no connection is
/// taken, those open finish the request they are answering and close
/// (ADR-0184 decision 4). At the cap the oldest connection answering
/// nothing is closed to make room, so silent connections, a leaking pool's
/// or an agent's, never shut the application and `/health` out; only when
/// every connection is answering a request is a newcomer refused, told so
/// with a `503` (third production review C-1, ADR-0186).
async fn served(
    listening: Listening,
    router: Router,
    shared: Shared,
    stop: impl std::future::Future<Output = ()>,
    idle: std::time::Duration,
    cap: usize,
) {
    let mut http = hyper::server::conn::http1::Builder::new();
    http.timer(hyper_util::rt::TokioTimer::new())
        .header_read_timeout(idle);
    let (closing, closed) = tokio::sync::watch::channel(false);
    // the connections open, by the order they were taken: what each is
    // answering, and how it is told to close
    type Open = std::collections::BTreeMap<u64, (Busy, Arc<tokio::sync::Notify>)>;
    let open: Arc<Mutex<Open>> = Arc::new(Mutex::new(Open::new()));
    let refusing = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut taken: u64 = 0;
    let mut tasks = tokio::task::JoinSet::new();
    tokio::pin!(stop);
    loop {
        let accepted = tokio::select! {
            () = &mut stop => break,
            accepted = listening.accept() => accepted,
        };
        let io = match accepted {
            Ok(io) => io,
            Err(e) => {
                // descriptors exhausted among the causes: said, and tried
                // again after a pause rather than in a loop
                say!("a connection was not accepted: {e}");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }
        };
        // this listener's own connections against its own cap, where the
        // count both listeners share left --metrics-bind refusing every
        // health check once the main listener held 16 (sixth production
        // review F9)
        let at_cap = open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
            >= cap;
        if at_cap {
            // the oldest answering nothing, which no request may begin on
            // once taken, is closed to make room
            let room = {
                let held = open
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                held.values()
                    .find(|(busy, _)| {
                        busy.compare_exchange(0, CLOSING, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                    })
                    .map(|(_, close)| close.clone())
            };
            if let Some(close) = room {
                close.notify_one();
                shared.connections.made_room.fetch_add(1, Ordering::SeqCst);
                say!(
                    "a connection answering nothing closed to make room: {cap} are open, as --max-connections allows"
                );
            } else {
                shared.connections.refused.fetch_add(1, Ordering::SeqCst);
                say!(
                    "a connection refused: {cap} are open, every one answering a request, as --max-connections allows"
                );
                if refusing.load(Ordering::SeqCst) < REFUSING {
                    refusing.fetch_add(1, Ordering::SeqCst);
                    let refusing = refusing.clone();
                    // its request read and answered 503, so closing does
                    // not reset the connection before the client reads why
                    let telling = http.serve_connection(
                        hyper_util::rt::TokioIo::new(io),
                        hyper::service::service_fn(|_| async {
                            Ok::<_, std::convert::Infallible>(busy(
                                "busy: every connection --max-connections allows is answering a request",
                            ))
                        }),
                    );
                    tasks.spawn(async move {
                        let _ =
                            tokio::time::timeout(std::time::Duration::from_secs(1), telling).await;
                        refusing.fetch_sub(1, Ordering::SeqCst);
                    });
                }
                continue;
            }
        }
        shared.connections.open.fetch_add(1, Ordering::SeqCst);
        let busy: Busy = Arc::new(std::sync::atomic::AtomicIsize::new(0));
        let close = Arc::new(tokio::sync::Notify::new());
        let id = taken;
        taken += 1;
        open.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(id, (busy.clone(), close.clone()));
        let serving = http.serve_connection(
            hyper_util::rt::TokioIo::new(io),
            OnConnection {
                router: hyper_util::service::TowerToHyperService::new(router.clone()),
                busy,
            },
        );
        let (mut closed, open, shared) = (closed.clone(), open.clone(), shared.clone());
        tasks.spawn(async move {
            tokio::pin!(serving);
            tokio::select! {
                _ = serving.as_mut() => {}
                _ = closed.changed() => {
                    serving.as_mut().graceful_shutdown();
                    let _ = serving.await;
                }
                // taken to make room: answering nothing, and nothing may
                // begin, so it is dropped, which closes it
                () = close.notified() => {}
            }
            open.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&id);
            shared.connections.open.fetch_sub(1, Ordering::SeqCst);
        });
        while tasks.try_join_next().is_some() {}
    }
    drop(listening);
    let _ = closing.send(true);
    while tasks.join_next().await.is_some() {}
}

/// A Unix socket bound at a path, replacing one a stopped service left
/// there and never another kind of file, and made 0660: its owner and its
/// group alone reach the service, the boundary the design leaves to the
/// deployment (ADR-0114, ADR-0183 decision 4).
fn bind_socket(path: &std::path::Path) -> std::io::Result<tokio::net::UnixListener> {
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
    match std::fs::symlink_metadata(path) {
        // a socket a service still answers on is that service's, never
        // taken over (second production review, minor)
        Ok(m)
            if m.file_type().is_socket()
                && std::os::unix::net::UnixStream::connect(path).is_ok() =>
        {
            return Err(std::io::Error::other(format!(
                "{} is the socket of a service still answering on it",
                path.display()
            )));
        }
        Ok(m) if m.file_type().is_socket() => std::fs::remove_file(path)
            .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))?,
        Ok(_) => {
            return Err(std::io::Error::other(format!(
                "{} holds a file that is not a socket, which the service does not replace",
                path.display()
            )));
        }
        Err(_) => {}
    }
    let named = |e: std::io::Error| std::io::Error::other(format!("{}: {e}", path.display()));
    let listener = tokio::net::UnixListener::bind(path).map_err(named)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660)).map_err(named)?;
    Ok(listener)
}

/// Whether a file that holds something is a store: a SQLite database with
/// the store's declarations' table.
fn is_store(path: &std::path::Path) -> std::io::Result<bool> {
    objectflow_core::release::holds_store(path)
        .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))
}

/// A deployment's store file, checked before it is opened: a path that holds
/// no store, absent or empty, is refused unless the start asks to create
/// one, so a mistyped path never becomes an empty store served (ADR-0183
/// decision 3); a store made here is its owner's alone, 0600, which SQLite
/// gives its `-wal` and `-shm` too; and one others can read is said. What
/// it has to say is returned, said once the service listens, whose first
/// line says where.
fn store_file(path: &std::path::Path, create: bool) -> std::io::Result<Vec<String>> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let held = std::fs::metadata(path)
        .map(|m| m.len() > 0)
        .unwrap_or(false);
    let mut notes = Vec::new();
    // a log holding what the file does not, before anything opens it: the
    // last stop was not clean, which a clean one never leaves (fifth
    // production review, ADR-0191 decision 4)
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    if let Ok(m) = std::fs::metadata(&wal)
        && m.len() > 0
    {
        notes.push(format!(
            "the store's log, {}, holds {} byte(s) its file does not: the last stop was not clean, or another process has the store open; the store takes them in as it opens",
            std::path::Path::new(&wal).display(),
            m.len()
        ));
    }
    if !held {
        if !create {
            return Err(std::io::Error::other(format!(
                "{} holds no store; to make one there, start once with --create",
                path.display()
            )));
        }
        // a directory that does not exist, among the faults, names the path
        // it was asked for (third production review)
        let named = |e: std::io::Error| std::io::Error::other(format!("{}: {e}", path.display()));
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)
            .map_err(named)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(named)?;
        notes.push(format!("a store made at {}", path.display()));
        return Ok(notes);
    }
    // a flag kept in a unit opens what is there, a volume that did not mount
    // among it (fifth production review, ADR-0183 decision 3)
    if create {
        notes.push(format!(
            "--create was given and {} holds a store, which is served as it is; a unit keeping the flag no longer refuses an empty path, a volume that did not mount among them",
            path.display()
        ));
    }
    // a store the service cannot write is refused, where it started, said
    // 200 at /health and answered every write 500 (ADR-0192 decision 7)
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|e| {
            std::io::Error::other(format!(
                "{} is not writable by the service: {e}; give the service's user the store, its directory and their write permission",
                path.display()
            ))
        })?;
    // a file that is a database but not a store, another application's, is
    // never made one: its tables are not ours to add to (second production
    // review, minor)
    if !is_store(path)? {
        return Err(std::io::Error::other(format!(
            "{} holds a file that is not an ObjectFlow store, which the service does not make one",
            path.display()
        )));
    }
    let mode = std::fs::metadata(path)
        .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        notes.push(format!(
            "the store {} is readable by others than its owner (mode {:o}); its personal data is too",
            path.display(),
            mode & 0o777
        ));
    }
    Ok(notes)
}

/// The store's lock, `<store>.lock` beside it, taken for the process's life;
/// where another service holds it, what the start says of it.
fn hold_store(path: &std::path::Path) -> std::io::Result<Option<String>> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut name = path.as_os_str().to_owned();
    name.push(".lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .mode(0o600)
        .open(&name)
        .map_err(|e| {
            std::io::Error::other(format!("{}: {e}", std::path::Path::new(&name).display()))
        })?;
    match lock.try_lock() {
        Ok(()) => {
            let _ = STORE_LOCK.set(lock);
            Ok(None)
        }
        Err(std::fs::TryLockError::WouldBlock) => Ok(Some(format!(
            "another service has the store {} open: the two take turns on its write lock (ADR-0178 decision 2); unless this is an upgrade's overlap, stop one of them",
            path.display()
        ))),
        Err(std::fs::TryLockError::Error(e)) => Err(e),
    }
}

/// Whether an endpoint's host is this machine's loopback.
fn loopback(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split('/').next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if let Some(v6) = host.strip_prefix('[') {
        v6.split(']').next().unwrap_or_default()
    } else {
        host.rsplit_once(':').map_or(host, |(h, _)| h)
    };
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// The interval at which systemd asks to be told the service lives, half
/// its `WatchdogSec`, where the unit sets one for this process.
fn watchdog() -> Option<std::time::Duration> {
    let usec: u64 = std::env::var("WATCHDOG_USEC").ok()?.parse().ok()?;
    if let Ok(pid) = std::env::var("WATCHDOG_PID")
        && pid.parse::<u32>().ok() != Some(std::process::id())
    {
        return None;
    }
    (usec > 0).then(|| std::time::Duration::from_micros(usec / 2))
}

async fn serve() -> std::io::Result<()> {
    let a = match args() {
        Ok(a) => a,
        Err(why) if why.is_empty() => {
            println!("{USAGE}");
            return Ok(());
        }
        Err(why) if why == version() => {
            println!("{why}");
            return Ok(());
        }
        Err(why) => {
            // stamped as every other line (fifth production review)
            say!("objectflow-service: {why}");
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    STARTED_AT.store(wall_millis() / 1000, Ordering::SeqCst);
    // a token sent where anyone between may read it, said after where the
    // service listens (fifth production review, ADR-0191 decision 4)
    let told: Vec<String> = a
        .tokens
        .keys()
        .filter_map(|name| {
            let url = a.endpoints.get(name)?;
            (url.starts_with("http://") && !loopback(url)).then(|| {
                format!(
                    "the evaluator {name}'s token is sent over plain http to {}, beyond loopback, where anyone on the way may read it; give an https:// endpoint",
                    shown(url)
                )
            })
        })
        .collect();
    let mut sandbox = Sandbox::default();
    let mut noted = Vec::new();
    if let Some(path) = &a.store {
        // a deployment's store, opened before anything is served (ADR-0174);
        // one at a release this core upgrades from is served only the
        // upgrade until it runs, and one at any other refuses to start, so
        // there is no downgrade (ADR-0177 decision 4)
        noted = store_file(path, a.create)?;
        // a second service on the store said at the start, where it started
        // with an ambiguous line; it is served, as ADR-0178 decision 2 lets
        // an upgrade's old copy share the store (ADR-0192 decision 7)
        noted.extend(hold_store(path)?);
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
                // the file and its log checked before anything opens them to
                // write, so a refused start leaves both as they were, where it
                // wrote another state's log into a restored copy (fourth
                // production review A-16)
                let checked = Store::checked_file(path)
                    .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))?;
                if let Some(fault) = checked {
                    return Err(std::io::Error::other(format!(
                        "{} fails SQLite's quick check ({fault}); a copy restored beside the -wal or -shm of another state does this: nothing was written, so restore the copy alone, with neither beside it (operate.md, Backups and restore)",
                        path.display()
                    )));
                }
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
            say!("listening on unix:{}", path.display());
            Listening::Unix(l)
        }
        None => {
            let l = tokio::net::TcpListener::bind(&a.bind)
                .await
                .map_err(|e| std::io::Error::other(format!("{}: {e}", a.bind)))?;
            let at = l.local_addr()?;
            say!("listening on http://{at}");
            // the service authenticates nobody, so an address beyond
            // loopback is said, where it started without a word (fourth
            // production review)
            if !at.ip().is_loopback() {
                say!(
                    "listening beyond loopback, on {at}: the service authenticates nobody, so whoever reaches this address may act as any actor; bind 127.0.0.1 or give --socket, behind the application that authenticates (core/README.md)"
                );
            }
            Listening::Tcp(l)
        }
    };
    for note in noted.into_iter().chain(told) {
        say!("{note}");
    }
    if let (Some(path), Some(at)) = (&a.store, &sandbox.due) {
        say!(
            "the store {} is at {at}, and this core is {BUILTINS}: every operation is refused until POST /v1/upgrade runs (ADR-0177)",
            path.display()
        );
    }
    if let (Some(path), Some(s)) = (&a.store, &sandbox.store) {
        say!(
            "the store {} at version {}, under {BUILTINS}",
            path.display(),
            s.lib.version
        );
        // a clock that ran ahead and came back (ADR-0174 decision 4)
        let lead = s.now - wall_clock();
        if lead > 0 {
            say!(
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
            say!(
                "unknown, no endpoint answering them (ADR-0175 decision 5): {}",
                unanswered
                    .iter()
                    .map(|f| f.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        for (name, url) in &a.endpoints {
            let token = if a.tokens.contains_key(name) {
                ", with its token"
            } else {
                ""
            };
            say!("the evaluator {name} asked at {}{token}", shown(url));
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
        tokens: a.tokens.clone(),
        evaluator_timeout: sandbox.evaluator_timeout,
        read_timeout: a.read_timeout.unwrap_or(std::time::Duration::from_secs(30)),
        admission: Admission::new(a.max_reads.unwrap_or(MAX_READS)),
        worklists: Admission::new(a.max_reads.unwrap_or(MAX_READS)),
        dry_runs: Admission::new(1),
        agents: Admission::new(a.max_reads.unwrap_or(MAX_READS)),
        deployment,
        reads,
        writer: Mutex::new(sandbox),
        served: Mutex::new(std::collections::BTreeMap::new()),
        in_flight: std::sync::atomic::AtomicUsize::new(0),
        writer_since: AtomicI64::new(0),
        connections: Connections::default(),
        store_identity: a.store.as_deref().and_then(file_identity),
        store_path: a.store.clone(),
        trims: std::sync::atomic::AtomicU64::new(0),
        trimming: std::sync::atomic::AtomicBool::new(false),
        watch: Watch::default(),
    };
    let shared = Arc::new(service);
    let router = app(shared.clone(), deployment);
    let _ = BODY_READ.set(a.idle_timeout);
    // `/metrics` and `/health` alone on an address of their own, where a
    // service on a socket could not be scraped (fourth production review)
    if let Some(bind) = &a.metrics_bind {
        let l = tokio::net::TcpListener::bind(bind)
            .await
            .map_err(|e| std::io::Error::other(format!("{bind}: {e}")))?;
        say!("metrics and health on http://{}", l.local_addr()?);
        let router = Router::new()
            .route("/health", axum::routing::get(health))
            .route("/metrics", axum::routing::get(metrics))
            .with_state(shared.clone());
        tokio::spawn(served(
            Listening::Tcp(l),
            router,
            shared.clone(),
            std::future::pending::<()>(),
            a.idle_timeout,
            METRICS_CONNECTIONS,
        ));
    }
    // what a run of short requests left held, given back once the service
    // is idle, where it waited for a heavy request (fourth production
    // review)
    tokio::spawn(idle_trims(shared.clone()));
    if shared.store_path.is_some() {
        tokio::spawn(keep_the_log_short(shared.clone()));
    }
    notify("READY=1");
    // a unit of WatchdogSec is told the service lives at half its interval,
    // while its runtime answers; whether writes move is /health's
    // writes_stalled (fifth production review, ADR-0191 decision 4)
    if let Some(every) = watchdog() {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(every);
            loop {
                tick.tick().await;
                notify("WATCHDOG=1");
            }
        });
    }
    served(
        listening,
        router,
        shared.clone(),
        stop,
        a.idle_timeout,
        a.max_connections,
    )
    .await;
    // the socket goes with the service; a start replaces one left by a
    // service that was killed
    if let Some(path) = &a.socket {
        std::fs::remove_file(path)?;
    }
    // the write-ahead log carried into the store's file, so a clean stop
    // leaves the store whole in one file (second production review, minor)
    if let Ok(writer) = shared.writer.lock()
        && let Some(s) = &writer.store
    {
        match s.checkpoint() {
            Ok(true) => {}
            Ok(false) => say!(
                "the write-ahead log was not carried into the store's file: a read was still open"
            ),
            Err(e) => say!("the write-ahead log was not carried into the store's file: {e}"),
        }
    }
    say!("stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A volume holding less than a checkpoint needs is low; one holding
    /// that much is not (ADR-0188 decision 3).
    #[test]
    fn a_volume_is_low_below_what_a_checkpoint_needs() {
        assert!(disk_low(DISK_FLOOR - 1));
        assert!(!disk_low(DISK_FLOOR));
        assert!(disk_free(std::path::Path::new(".")).is_some_and(|b| b > 0));
        assert_eq!(
            disk_free(std::path::Path::new("/no/such/volume/here")),
            None
        );
    }

    /// Writes stall where the last one met another process's lock past its
    /// retries, lately, and none committed since; an idle trim is due where
    /// something was answered and nothing is in flight; and a full volume is
    /// a 503 a client may retry (fourth production review).
    #[test]
    fn a_stall_an_idle_trim_and_a_full_disk_are_told() {
        let w = Watch::default();
        let now = wall_millis();
        assert!(!w.stalled(now));
        w.note(&Verdict::RetriesExhausted);
        assert!(w.stalled(wall_millis()));
        assert!(!w.stalled(wall_millis() + STALLED_MS));
        w.note(&Verdict::Satisfied {
            object_id: "x-1".into(),
            version: 1,
            events: Vec::new(),
            flags: Vec::new(),
            replayed: false,
        });
        assert!(!w.stalled(wall_millis()));
        assert!(trim_due(0, 3, 0));
        assert!(!trim_due(3, 3, 0));
        assert!(!trim_due(0, 3, 1));
        let full = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_FULL),
            Some("database or disk is full".into()),
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
        let fault = Fault::from(objectflow_core::store::StoreError::from(full));
        let answer = fault.into_response();
        assert_eq!(answer.status(), StatusCode::SERVICE_UNAVAILABLE);
        // said by /health until a write commits after it (ADR-0192
        // decision 7)
        assert!(disk_full_since(w.written_at.load(Ordering::SeqCst)));
        std::thread::sleep(std::time::Duration::from_millis(5));
        w.note(&Verdict::Satisfied {
            object_id: "x-1".into(),
            version: 2,
            events: Vec::new(),
            flags: Vec::new(),
            replayed: false,
        });
        assert!(!disk_full_since(w.written_at.load(Ordering::SeqCst)));
    }

    /// A fault's code is its reason's prefix where it has one, else its
    /// status's name; an endpoint's password is never shown; and a log
    /// blocked longer than its bound is held so, a line still writing
    /// within it not (fourth production review).
    #[test]
    fn a_fault_is_coded_a_password_hidden_and_a_blocked_log_known() {
        assert_eq!(
            fault_code(503, "read_timeout: the read took 30 s"),
            "read_timeout"
        );
        assert_eq!(fault_code(503, "busy: every connection"), "busy");
        assert_eq!(fault_code(409, "key_reused: k"), "key_reused");
        assert_eq!(fault_code(400, "pull names its actor"), "bad_request");
        assert_eq!(fault_code(500, "storage unavailable: x"), "internal");
        assert_eq!(fault_code(501, "not run: a metric"), "not_run");
        assert_eq!(
            shown("https://bridge:s3cret@acct.example/verify?x=1"),
            "https://bridge:***@acct.example/verify?x=1"
        );
        assert_eq!(shown("http://127.0.0.1:9000/x"), "http://127.0.0.1:9000/x");
        // a fault of one code said once a second, the rest counted
        let last = Mutex::new((String::new(), 0, 0));
        assert_eq!(fault_said(&last, "disk_full", 10_000), Some(0));
        assert_eq!(fault_said(&last, "disk_full", 10_500), None);
        assert_eq!(fault_said(&last, "disk_full", 10_900), None);
        assert_eq!(fault_said(&last, "disk_full", 11_000), Some(2));
        assert_eq!(fault_said(&last, "internal", 11_100), Some(0));
        assert!(!log_blocked(0, 1_000_000));
        assert!(!log_blocked(1_000_000, 1_000_000 + LOG_BLOCKED_MS));
        assert!(log_blocked(1_000_000, 1_000_000 + LOG_BLOCKED_MS + 1));
    }

    /// A body naming a key twice is refused, where the last one won; one
    /// naming each once reads as it did (ADR-0190 decision 8).
    #[test]
    fn a_body_names_each_key_once() {
        assert!(
            strict_json(br#"{"actor": {"id": "a"}, "actor": {"id": "b"}}"#)
                .is_err_and(|e| e.contains("actor is named twice"))
        );
        assert!(strict_json(br#"{"a": {"b": 1, "b": 2}}"#).is_err());
        assert_eq!(
            strict_json(
                br#"{"a": [1, -2, 2.5, "x", null, true], "b": {"c": 18446744073709551615}}"#
            ),
            Ok(json!({"a": [1, -2, 2.5, "x", null, true], "b": {"c": 18446744073709551615_u64}}))
        );
        assert!(strict_json(b"{} {}").is_err());
    }

    /// A fault's reason is said in the log at most 1 KiB long, cut on a
    /// character's boundary, where a filter of 1.9 MB was written whole
    /// (fourth production review A-10).
    #[test]
    fn a_faults_reason_is_said_bounded() {
        assert_eq!(said("short"), "short");
        let long = "é".repeat(1000);
        let cut = said(&long);
        assert!(cut.len() < 1100 && cut.ends_with("… (2000 bytes)"), "{cut}");
    }

    /// A read that grows with the store waits for its turn within its
    /// deadline and is refused past it, its slot given back when the read
    /// ends, a waiter counted while it waits (ADR-0189 decision 2).
    #[test]
    #[allow(clippy::panic)]
    fn a_heavy_read_waits_its_turn_within_its_deadline() {
        use std::time::{Duration, Instant};
        let a = Admission::new(1);
        let timeout = Duration::from_secs(30);
        let Ok(first) = a.take(Instant::now() + timeout, timeout) else {
            panic!("no slot free")
        };
        assert_eq!(a.running(), 1);
        std::thread::scope(|scope| {
            let started = Instant::now();
            let waiter =
                scope.spawn(|| a.take(Instant::now() + timeout, timeout).map(drop).is_ok());
            while a.waiting.load(Ordering::SeqCst) == 0 {
                std::thread::yield_now();
            }
            std::thread::sleep(Duration::from_millis(100));
            drop(first);
            assert!(waiter.join().unwrap_or(false), "a freed slot was not taken");
            assert!(started.elapsed() >= Duration::from_millis(100));
        });
        assert_eq!((a.running(), a.waiting.load(Ordering::SeqCst)), (0, 0));
        let Ok(held) = a.take(Instant::now() + timeout, timeout) else {
            panic!("no slot free")
        };
        let started = Instant::now();
        let late = a.take(Instant::now() + Duration::from_millis(200), timeout);
        assert!(
            matches!(&late, Err(Fault::TimedOut(m)) if m.contains("--max-reads")),
            "a read waited past its deadline"
        );
        assert!(started.elapsed() >= Duration::from_millis(200));
        drop(late);
        assert_eq!((a.running(), a.waiting.load(Ordering::SeqCst)), (1, 0));
        drop(held);
        assert_eq!(a.running(), 0);
        // held for a cut of the write-ahead log, a new read waits though a
        // slot is free, the cut waits for the one running, and once
        // released the new read is let in (fourth production review A-14)
        let two = Admission::new(2);
        let Ok(running) = two.take(Instant::now() + timeout, timeout) else {
            panic!("no slot free")
        };
        std::thread::scope(|scope| {
            let holding = scope.spawn(|| two.hold(Instant::now() + timeout));
            let began = Instant::now();
            while !two.barred.load(Ordering::SeqCst) && began.elapsed() < Duration::from_secs(2) {
                std::thread::yield_now();
            }
            assert!(
                matches!(
                    two.take(Instant::now() + Duration::from_millis(100), timeout),
                    Err(Fault::TimedOut(_))
                ),
                "a read was let in while the cut held the gate"
            );
            drop(running);
            assert!(
                holding.join().unwrap_or(false),
                "the cut never saw none running"
            );
        });
        two.release();
        assert!(two.take(Instant::now() + timeout, timeout).is_ok());
    }

    /// The log's cut holds new reads a moment: where a read runs past it, a
    /// cut not yet urgent is left for the next look and the held reads are
    /// let in, and an urgent one stops the read running and cuts, where it
    /// held every new read until the longest ended, a person's worklist 25 s
    /// behind an agent's sweep (seventh production review CUTSTALL,
    /// ADR-0194 decision 4).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[allow(clippy::panic)]
    async fn a_cut_holds_new_reads_a_moment_and_stops_long_ones_when_urgent() {
        use std::time::{Duration, Instant};
        let dir = std::env::temp_dir().join(format!("objectflow-cut-hold-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        let path = dir.join("store.sqlite");
        store_file(&path, true).unwrap_or_else(|e| panic!("{e}"));
        let Ok(store) = Store::open(&path, wall_clock()) else {
            panic!("the store did not open")
        };
        let sandbox = Sandbox {
            deployment: true,
            path: Some(path.clone()),
            store: Some(store),
            ..Default::default()
        };
        let reads = Reads {
            path: path.clone(),
            pool: Readers::new(&path, objectflow_core::store::BUSY_TIMEOUT),
            seen: std::sync::RwLock::new(Seen::of(&sandbox)),
            floor: AtomicI64::new(0),
        };
        let shared: Shared = Arc::new(Service {
            writer: Mutex::new(sandbox),
            reads: Some(reads),
            endpoints: std::collections::BTreeMap::new(),
            tokens: std::collections::BTreeMap::new(),
            evaluator_timeout: Duration::from_secs(5),
            deployment: true,
            read_timeout: Duration::from_secs(30),
            admission: Admission::new(1),
            worklists: Admission::new(1),
            dry_runs: Admission::new(1),
            agents: Admission::new(1),
            served: Mutex::new(std::collections::BTreeMap::new()),
            in_flight: std::sync::atomic::AtomicUsize::new(0),
            writer_since: AtomicI64::new(0),
            connections: Connections::default(),
            store_path: Some(path.clone()),
            trims: std::sync::atomic::AtomicU64::new(0),
            trimming: std::sync::atomic::AtomicBool::new(false),
            store_identity: None,
            watch: Watch::default(),
        });
        // a write, so the log holds frames
        let drafted = request_op(
            State(shared.clone()),
            Body(json!({"actor": {"id": "operator"}, "type": "DeclarationChange", "transition": "draft",
                        "inputs": {"source": {"m": "module: m"}, "source_ref": "git:m@1"}})),
        )
        .await;
        assert!(drafted.is_ok(), "the draft faulted");
        let mut wal = path.clone().into_os_string();
        wal.push("-wal");
        let bytes = std::fs::metadata(&wal).map_or(0, |m| m.len());
        assert!(bytes >= 64, "the log holds {bytes} bytes");
        // a long read: a worklist slot held until the cut's epoch moves, or 10 s
        let (taken, held) = std::sync::mpsc::channel();
        let reader = {
            let s = shared.clone();
            std::thread::spawn(move || {
                let began = s.watch.cut_epoch.load(Ordering::SeqCst);
                let slot = s.worklists.take(
                    Instant::now() + Duration::from_secs(60),
                    Duration::from_secs(60),
                );
                let _ = taken.send(slot.is_ok());
                let start = Instant::now();
                while s.watch.cut_epoch.load(Ordering::SeqCst) == began
                    && start.elapsed() < Duration::from_secs(10)
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                drop(slot);
            })
        };
        assert_eq!(held.recv().ok(), Some(true), "the long read took no slot");
        // not urgent: left for the next look within a moment, the reads let in
        let began = Instant::now();
        assert!(!cut_back(&shared, bytes - 1));
        assert!(
            began.elapsed() < Duration::from_secs(3),
            "{:?}",
            began.elapsed()
        );
        assert_eq!(shared.watch.cuts_deferred.load(Ordering::SeqCst), 1);
        let let_in = shared.admission.take(
            Instant::now() + Duration::from_millis(200),
            Duration::from_secs(1),
        );
        assert!(let_in.is_ok(), "a new read was held past the cut");
        drop(let_in);
        // urgent: the read running stopped, and the log cut
        let began = Instant::now();
        assert!(cut_back(&shared, bytes / 5));
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "{:?}",
            began.elapsed()
        );
        assert_eq!(shared.watch.cuts.load(Ordering::SeqCst), 1);
        assert!(reader.join().is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An agent's reads wait in a pool of their own: with every worklist
    /// slot held an agent's query is answered, and with the agents' held a
    /// person's is, where four of an agent's queries held every worklist slot
    /// and a person waited 28.6 s (sixth production review P5, ADR-0192
    /// decision 3).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[allow(clippy::panic)]
    async fn an_agents_reads_wait_in_a_pool_of_their_own() {
        use std::time::{Duration, Instant};
        let dir =
            std::env::temp_dir().join(format!("objectflow-agents-pool-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        let path = dir.join("store.sqlite");
        store_file(&path, true).unwrap_or_else(|e| panic!("{e}"));
        let Ok(store) = Store::open(&path, wall_clock()) else {
            panic!("the store did not open")
        };
        let sandbox = Sandbox {
            deployment: true,
            path: Some(path.clone()),
            store: Some(store),
            ..Default::default()
        };
        let reads = Reads {
            path: path.clone(),
            pool: Readers::new(&path, objectflow_core::store::BUSY_TIMEOUT),
            seen: std::sync::RwLock::new(Seen::of(&sandbox)),
            floor: AtomicI64::new(0),
        };
        let shared: Shared = Arc::new(Service {
            writer: Mutex::new(sandbox),
            reads: Some(reads),
            endpoints: std::collections::BTreeMap::new(),
            tokens: std::collections::BTreeMap::new(),
            evaluator_timeout: Duration::from_secs(5),
            deployment: true,
            read_timeout: Duration::from_secs(1),
            admission: Admission::new(1),
            worklists: Admission::new(1),
            dry_runs: Admission::new(1),
            agents: Admission::new(1),
            served: Mutex::new(std::collections::BTreeMap::new()),
            in_flight: std::sync::atomic::AtomicUsize::new(0),
            writer_since: AtomicI64::new(0),
            connections: Connections::default(),
            store_path: Some(path.clone()),
            trims: std::sync::atomic::AtomicU64::new(0),
            trimming: std::sync::atomic::AtomicBool::new(false),
            store_identity: None,
            watch: Watch::default(),
        });
        let op = json!({"id": "operator"});
        let module = "module: m\ncategories: [live]\ntypes:\n  Bot:\n    description: An agent.\n    tracking: record\n    attributes:\n      key: { type: identity, actor_kind: agent }\n    states:\n      ON: { category: live, description: On. }\n      OFF: { category: live, final: true, description: Off. }\n    transitions:\n      issue: { kind: initial, to: ON, required_inputs: [key] }\n      retire: { kind: external, from: ON, to: OFF }\n";
        let asked = |body: J| {
            let shared = shared.clone();
            async move {
                match request_op(State(shared), Body(body)).await {
                    Ok(Json(v)) => v,
                    Err(_) => panic!("a request faulted"),
                }
            }
        };
        let drafted = asked(
            json!({"actor": op, "type": "DeclarationChange", "transition": "draft",
            "inputs": {"source": {"m": module}, "source_ref": "git:m@1"}}),
        )
        .await;
        let change = drafted["object_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let submitted =
            asked(json!({"actor": op, "object_id": change, "transition": "submit", "inputs": {}}))
                .await;
        assert_eq!(submitted["kind"], "satisfied", "{submitted}");
        let Ok(Json(published)) = publish(
            State(shared.clone()),
            Body(json!({"actor": op, "change": change, "expected_version": submitted["version"]})),
        )
        .await
        else {
            panic!("not published")
        };
        assert_eq!(published["verdict"]["kind"], "satisfied", "{published}");
        let issued = asked(
            json!({"actor": op, "type": "Bot", "transition": "issue", "inputs": {"key": "scout"}}),
        )
        .await;
        assert_eq!(issued["kind"], "satisfied", "{issued}");
        let read_as = |who: &str| {
            let shared = shared.clone();
            let body = json!({"actor": {"id": who}, "type": "Bot"});
            async move {
                let began = Instant::now();
                (
                    query(State(shared), Body(body)).await.is_ok(),
                    began.elapsed(),
                )
            }
        };
        let quick = Duration::from_millis(500);
        // every worklist slot held: the agent is answered, the person waits
        // out the deadline
        let held = shared.worklists.take(
            Instant::now() + Duration::from_secs(60),
            Duration::from_secs(60),
        );
        assert!(held.is_ok(), "no worklist slot free");
        let (answered, took) = read_as("scout").await;
        assert!(
            answered && took < quick,
            "the agent waited {took:?} behind the worklists"
        );
        let (answered, _) = read_as("operator").await;
        assert!(!answered, "a person's read took a held worklist slot");
        drop(held);
        // every agent's slot held: the person is answered, the agent waits
        let held = shared.agents.take(
            Instant::now() + Duration::from_secs(60),
            Duration::from_secs(60),
        );
        assert!(held.is_ok(), "no agent slot free");
        let (answered, took) = read_as("operator").await;
        assert!(
            answered && took < quick,
            "the person waited {took:?} behind the agents"
        );
        let (answered, _) = read_as("scout").await;
        assert!(!answered, "an agent's read took a held agent slot");
        drop(held);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every read that grows with the store waits for a slot and is
    /// answered `503 read_timeout` at its deadline while none frees, an
    /// object's history and the events at positions among them; a read of
    /// one object or of the declaration never waits (ADR-0189 decision 2,
    /// ADR-0192 decision 5).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[allow(clippy::panic)]
    async fn only_the_reads_that_grow_with_the_store_wait_their_turn() {
        use std::time::{Duration, Instant};
        let dir = std::env::temp_dir().join(format!("objectflow-admission-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        let path = dir.join("store.sqlite");
        store_file(&path, true).unwrap_or_else(|e| panic!("{e}"));
        let Ok(store) = Store::open(&path, wall_clock()) else {
            panic!("the store did not open")
        };
        let sandbox = Sandbox {
            deployment: true,
            path: Some(path.clone()),
            store: Some(store),
            ..Default::default()
        };
        let reads = Reads {
            path: path.clone(),
            pool: Readers::new(&path, objectflow_core::store::BUSY_TIMEOUT),
            seen: std::sync::RwLock::new(Seen::of(&sandbox)),
            floor: AtomicI64::new(0),
        };
        let shared: Shared = Arc::new(Service {
            writer: Mutex::new(sandbox),
            reads: Some(reads),
            endpoints: std::collections::BTreeMap::new(),
            tokens: std::collections::BTreeMap::new(),
            evaluator_timeout: Duration::from_secs(5),
            deployment: true,
            read_timeout: Duration::from_secs(1),
            admission: Admission::new(1),
            worklists: Admission::new(1),
            dry_runs: Admission::new(1),
            agents: Admission::new(1),
            served: Mutex::new(std::collections::BTreeMap::new()),
            in_flight: std::sync::atomic::AtomicUsize::new(0),
            writer_since: AtomicI64::new(0),
            connections: Connections::default(),
            store_path: Some(path.clone()),
            trims: std::sync::atomic::AtomicU64::new(0),
            trimming: std::sync::atomic::AtomicBool::new(false),
            store_identity: None,
            watch: Watch::default(),
        });
        let op = json!({"id": "operator"});
        // a change of the flow to submit, drafted before any slot is held
        let Ok(Json(drafted)) = request_op(
            State(shared.clone()),
            Body(
                json!({"actor": op, "type": "DeclarationChange", "transition": "draft",
                        "inputs": {"source": {"m": "module: m\ncategories: [live]\n"}, "source_ref": "git:m@1"}}),
            ),
        )
        .await
        else {
            panic!("no change drafted")
        };
        assert_eq!(drafted["kind"], "satisfied", "{drafted}");
        let change = drafted["object_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        // every pool's one slot held, a read of each to wait for it
        let held = [&shared.admission, &shared.worklists, &shared.dry_runs].map(|pool| {
            pool.take(
                Instant::now() + Duration::from_secs(600),
                Duration::from_secs(600),
            )
        });
        assert!(held.iter().all(Result::is_ok), "no slot free");
        let waited = |answer: Answer, started: Instant, route: &str| {
            assert!(
                matches!(&answer, Err(Fault::TimedOut(m)) if m.contains("--max-reads")),
                "{route} did not wait for its turn"
            );
            assert!(
                started.elapsed() >= Duration::from_millis(900),
                "{route} answered early"
            );
        };
        macro_rules! heavy {
            ($route:ident, $body:expr) => {{
                let started = Instant::now();
                let answer = $route(State(shared.clone()), Body($body)).await;
                waited(answer, started, stringify!($route));
            }};
        }
        heavy!(query, json!({"actor": op, "type": "Subscription"}));
        // an object's history and the events at positions, each answering
        // as much as the log holds (ADR-0192 decision 5)
        heavy!(history, json!({"actor": op, "id": "subscription-1"}));
        heavy!(events, json!({"actor": op, "positions": [1]}));
        heavy!(metric, json!({"actor": op, "name": "Subscription.count"}));
        heavy!(
            available,
            json!({"actor": op, "type": "Subscription", "transition": "end"})
        );
        heavy!(exceptions, json!({"actor": op, "type": "Subscription"}));
        heavy!(
            diagnostics,
            json!({"actor": op, "type": "Subscription", "window": "30 days"})
        );
        heavy!(export, json!({"actor": op, "source": "log"}));
        heavy!(pull, json!({"actor": op, "subscription": "subscription-1"}));
        heavy!(
            publish,
            json!({"actor": op, "change": change, "dry_run": true})
        );
        heavy!(
            request_op,
            json!({"actor": op, "object_id": change, "transition": "submit", "inputs": {}})
        );
        heavy!(
            check,
            json!({"actor": op, "object_id": change, "transition": "submit", "inputs": {}})
        );
        macro_rules! light {
            ($route:ident, $body:expr) => {{
                let started = Instant::now();
                let answer = $route(State(shared.clone()), Body($body)).await;
                assert!(
                    !matches!(&answer, Err(Fault::TimedOut(_))),
                    "{} waited for a heavy read's turn",
                    stringify!($route)
                );
                // the body taken, so the route reached its read
                if let Err(Fault::Bad(m)) = &answer {
                    panic!("{} refused its body: {m}", stringify!($route));
                }
                assert!(
                    started.elapsed() < Duration::from_millis(900),
                    "{} waited",
                    stringify!($route)
                );
            }};
        }
        light!(get, json!({"actor": op, "id": "subscription-1"}));
        light!(declaration, json!({"actor": op}));
        light!(lookup, json!({"source": "legacy", "value": "x"}));
        // a transition named submit on what is no change of the flow is an
        // ordinary write, taking no slot (fifth production review N-1)
        light!(
            request_op,
            json!({"actor": op, "object_id": "operator", "transition": "submit", "inputs": {}})
        );
        drop(held);
        let started = Instant::now();
        let answer = query(
            State(shared.clone()),
            Body(json!({"actor": op, "type": "Subscription"})),
        )
        .await;
        assert!(
            !matches!(&answer, Err(Fault::TimedOut(_))),
            "a freed slot was not taken"
        );
        assert!(started.elapsed() < Duration::from_millis(900));
        drop(answer);
        // the pools apart: a worklist never waits behind the analytics, nor
        // an analysis behind the worklists (fifth production review N-1)
        let analytics = shared.admission.take(
            Instant::now() + Duration::from_secs(600),
            Duration::from_secs(600),
        );
        assert!(analytics.is_ok(), "no slot free");
        light!(query, json!({"actor": op, "type": "Subscription"}));
        light!(
            available,
            json!({"actor": op, "type": "Subscription", "transition": "end"})
        );
        light!(pull, json!({"actor": op, "subscription": "subscription-1"}));
        drop(analytics);
        let worklists = shared.worklists.take(
            Instant::now() + Duration::from_secs(600),
            Duration::from_secs(600),
        );
        assert!(worklists.is_ok(), "no slot free");
        light!(metric, json!({"actor": op, "name": "Subscription.count"}));
        light!(export, json!({"actor": op, "source": "log"}));
        light!(exceptions, json!({"actor": op, "type": "Subscription"}));
        drop(worklists);
        // what a read waited for its turn is counted against its deadline
        let Ok(held) = shared.admission.take(
            Instant::now() + Duration::from_secs(600),
            Duration::from_secs(600),
        ) else {
            panic!("no slot free")
        };
        let started = Instant::now();
        std::thread::scope(|scope| {
            scope.spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                drop(held);
            });
            let Ok((_slot, mut r)) = heavy_reading(&shared, &shared.admission, None) else {
                panic!("a slot freed within the deadline was not taken")
            };
            let Ok(store) = r.store() else {
                panic!("no store read")
            };
            let Some(deadline) = store.deadline else {
                panic!("a read with no deadline")
            };
            assert!(started.elapsed() >= Duration::from_millis(300));
            assert!(
                deadline <= started + Duration::from_millis(1050),
                "the read was given a whole deadline after waiting"
            );
        });
        // a cut of the write-ahead log holds new reads back, waits for the
        // one running, and then truncates the log; none is cut below its
        // limit with no erasure due (fourth production review A-14)
        let drafted = request_op(
            State(shared.clone()),
            Body(
                json!({"actor": op, "type": "DeclarationChange", "transition": "draft",
                        "inputs": {"source": {"m": "module: m\ncategories: [live]\n"}, "source_ref": "git:m@1"}}),
            ),
        )
        .await;
        assert!(
            matches!(&drafted, Ok(Json(v)) if v["kind"] == "satisfied"),
            "a write before the cut"
        );
        let mut wal = path.clone().into_os_string();
        wal.push("-wal");
        let wal_len = || std::fs::metadata(&wal).map_or(0, |m| m.len());
        assert!(wal_len() > 0, "a write-ahead log to cut");
        assert!(!cut_back(&shared, u64::MAX));
        let Ok(held) = shared.admission.take(
            Instant::now() + Duration::from_secs(600),
            Duration::from_secs(600),
        ) else {
            panic!("no slot free")
        };
        let started = Instant::now();
        std::thread::scope(|scope| {
            let cutting = scope.spawn(|| cut_back(&shared, 0));
            std::thread::sleep(Duration::from_millis(300));
            // a new read waits behind the cut
            assert!(
                matches!(
                    shared.admission.take(
                        Instant::now() + Duration::from_millis(100),
                        Duration::from_secs(1)
                    ),
                    Err(Fault::TimedOut(_))
                ),
                "a read was let in during the cut"
            );
            drop(held);
            assert!(cutting.join().unwrap_or(false), "the log was not cut");
        });
        assert!(started.elapsed() >= Duration::from_millis(300));
        assert_eq!(wal_len(), 0);
        assert_eq!(shared.watch.cuts.load(Ordering::SeqCst), 1);
        assert!(
            shared
                .admission
                .take(
                    Instant::now() + Duration::from_secs(1),
                    Duration::from_secs(1)
                )
                .is_ok(),
            "a read waited after the cut"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A connection's requests take it and let it go; one the cap has taken
    /// to make room takes nothing, so closing it cuts no request short, and
    /// one answering a request is never taken to make room (C-1).
    #[test]
    fn a_connection_taken_to_make_room_begins_nothing() {
        let busy: Busy = Arc::new(std::sync::atomic::AtomicIsize::new(0));
        assert!(taken(&busy));
        let held = Held {
            body: axum::body::Body::empty(),
            _holding: Some(Holding(busy.clone())),
        };
        // answering: not closed to make room
        assert!(
            busy.compare_exchange(0, CLOSING, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        );
        // its body written, the connection answers nothing
        drop(held);
        assert_eq!(busy.load(Ordering::SeqCst), 0);
        assert!(
            busy.compare_exchange(0, CLOSING, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        );
        assert!(!taken(&busy));
        assert_eq!(busy.load(Ordering::SeqCst), CLOSING);
    }

    /// A request that panics holding the writer leaves it poisoned; the
    /// next write takes it back, and a deployment's store is opened again
    /// from its file, where every write after was refused 500 until a
    /// restart (POISON).
    #[test]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    fn a_writer_a_panic_poisoned_is_taken_back() {
        let dir = std::env::temp_dir().join(format!("objectflow-poison-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("store.sqlite");
        let sandbox = Sandbox {
            store: Some(Store::open(&path, wall_clock()).unwrap()),
            deployment: true,
            path: Some(path.clone()),
            busy_timeout: objectflow_core::store::BUSY_TIMEOUT,
            ..Default::default()
        };
        let shared: Shared = Arc::new(Service {
            writer: Mutex::new(sandbox),
            reads: None,
            endpoints: std::collections::BTreeMap::new(),
            tokens: std::collections::BTreeMap::new(),
            evaluator_timeout: std::time::Duration::from_secs(5),
            deployment: true,
            read_timeout: std::time::Duration::from_secs(30),
            admission: Admission::new(MAX_READS),
            worklists: Admission::new(MAX_READS),
            dry_runs: Admission::new(1),
            agents: Admission::new(MAX_READS),
            served: Mutex::new(std::collections::BTreeMap::new()),
            in_flight: std::sync::atomic::AtomicUsize::new(0),
            writer_since: AtomicI64::new(0),
            connections: Connections::default(),
            store_path: None,
            trims: std::sync::atomic::AtomicU64::new(0),
            trimming: std::sync::atomic::AtomicBool::new(false),
            store_identity: None,
            watch: Watch::default(),
        });
        let held = shared.clone();
        let panicked = std::thread::spawn(move || {
            let Ok(mut w) = locked(&held) else {
                panic!("the writer not taken")
            };
            // what a panic leaves half-done in memory: the store dropped
            w.store = None;
            panic!("a request's bug");
        })
        .join();
        assert!(panicked.is_err());
        assert!(shared.writer.is_poisoned());
        let Ok(mut w) = locked(&shared) else {
            panic!("the writer not taken back")
        };
        assert!(store(&mut w).is_ok(), "the store was not opened again");
        drop(w);
        assert!(!shared.writer.is_poisoned());
        let _ = std::fs::remove_dir_all(&dir);
    }

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

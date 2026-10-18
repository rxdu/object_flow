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
    /// A deployment's store on PostgreSQL, where it is kept there
    /// (ADR-0213).
    postgres: Option<objectflow_core::db::PgConfig>,
    /// How long its writes wait out another process's write lock
    /// (ADR-0178 decision 2).
    busy_timeout: std::time::Duration,
    /// The release a deployment's store is at where it is due an upgrade to
    /// this one: every operation but the upgrade is refused until it runs
    /// (ADR-0177 decision 4).
    due: Option<String>,
    /// Since when its writes have waited for another process's lock, which
    /// every store it opens tells and `/health` reads (ADR-0195 decision 6).
    lock_wait: Arc<AtomicI64>,
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
/// line the service writes among them, where its start's carried no time:
/// handed to the log's own thread through a bounded queue, never written by
/// the caller, and dropped and counted where the queue is full, so a reader
/// of the log that stopped reading stops no request; where two requests
/// each wrote their own line, holding standard error's lock, a full pipe
/// stopped both of the runtime's threads and every request with them,
/// `/health` included (tenth production review LOG-FREEZE). The log's
/// thread marks when the line it writes began, which `/health` reads as
/// `log_blocked` past `LOG_BLOCKED_MS` (fourth production review).
fn say(line: std::fmt::Arguments<'_>) {
    // one line, stamped, whatever its text holds: SQLite's fault text holds
    // a line break, which broke a refused start's line in two, the second
    // unstamped, reported as the refusal printed twice (ADR-0202 decision 8)
    let text = format!("{} {line}", timestamp_text(wall_clock())).replace(['\r', '\n'], " ");
    match LOG.get_or_init(start_log).try_send(text) {
        Ok(()) => {
            LOG_QUEUED.fetch_add(1, Ordering::SeqCst);
        }
        Err(_) => {
            LINES_DROPPED.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// How many lines the log's queue holds before a line is dropped.
const LOG_QUEUE: usize = 4096;

/// The log's queue, its thread started with the first line.
static LOG: std::sync::OnceLock<std::sync::mpsc::SyncSender<String>> = std::sync::OnceLock::new();

/// The lines queued and not yet written.
static LOG_QUEUED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The log's thread: each queued line written to standard error, the time
/// its writing began marked while it is written; lines dropped since the
/// last it wrote said first, where the log said nothing of them, only a
/// counter in `/metrics` (eleventh production review).
fn start_log() -> std::sync::mpsc::SyncSender<String> {
    let (send, lines) = std::sync::mpsc::sync_channel::<String>(LOG_QUEUE);
    let _ = std::thread::Builder::new()
        .name("log".into())
        .spawn(move || {
            use std::io::Write;
            let mut told = 0;
            for text in lines {
                LOGGING_SINCE.store(wall_millis(), Ordering::SeqCst);
                let dropped = LINES_DROPPED.load(Ordering::SeqCst);
                if dropped > told
                    && writeln!(
                        std::io::stderr().lock(),
                        "{} the log dropped {} lines while it could not be written (objectflow_log_lines_dropped_total)",
                        timestamp_text(wall_clock()),
                        dropped - told
                    )
                    .is_ok()
                {
                    told = dropped;
                }
                // a log whose reader has gone drops the line: `eprintln!`
                // panicked on the broken pipe, losing answers already
                // committed (fifth production review)
                if writeln!(std::io::stderr().lock(), "{text}").is_err() {
                    LINES_DROPPED.fetch_add(1, Ordering::SeqCst);
                }
                LOGGING_SINCE.store(0, Ordering::SeqCst);
                LOG_QUEUED.fetch_sub(1, Ordering::SeqCst);
            }
        });
    send
}

/// The lines queued written, waited for up to `within`, as the service ends.
fn log_drained(within: std::time::Duration) {
    let until = std::time::Instant::now() + within;
    while LOG_QUEUED.load(Ordering::SeqCst) > 0 && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(10));
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
    // a redirect followed by none: a 3xx is an answer that is not a 200, so
    // the evaluator is unknown, where a redirect sent the call to a host the
    // operator never configured, whose answer decided a guard (eighth
    // production review EV-REDIRECT, ADR-0195 decision 3)
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .max_redirects(0)
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
    // a timeout named by the flag that sets it, where it said "timeout:
    // global" (ninth production review)
    let late = || {
        format!(
            "{} did not answer within --evaluator-timeout ({} s)",
            shown(endpoint),
            timeout.as_secs()
        )
    };
    let mut answer = match posting.send(&body) {
        Ok(a) => a,
        Err(ureq::Error::Timeout(_)) => return unknown(late()),
        Err(e) => return unknown(format!("{} was not reached: {e}", shown(endpoint))),
    };
    let status = answer.status().as_u16();
    let text = match answer.body_mut().read_to_string() {
        Ok(t) => t,
        Err(ureq::Error::Timeout(_)) => return unknown(late()),
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
    consulting_with(
        shared,
        |asked| {
            // a check on a read of its own, a request on the writer (ADR-0178)
            let mut run = |s: &mut Store| {
                let given = s.answers.clone();
                s.answers.extend(asked.clone());
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
        },
        stopped,
    )
}

/// [`consulting`] with how an attempt runs given: on the writer or a read
/// of its own, or queued for a group's commit (ADR-0208), each attempt
/// carrying the evaluators' answers asked so far.
fn consulting_with<T>(
    shared: &Shared,
    mut attempt: impl FnMut(&std::collections::BTreeMap<String, String>) -> Result<T, Fault>,
    stopped: impl Fn(&T) -> Option<Call>,
) -> Result<T, Fault> {
    let mut asked_so_far: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    let mut calls = 0;
    let timeout = shared.evaluator_timeout;
    loop {
        let out = attempt(&asked_so_far);
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
    /// How long a change's dry run may run, or wait for its one slot: its
    /// own, where it shared the reads' and a store whose publish outgrew
    /// 30 s loosened every read's deadline to publish (ADR-0194 decision 2).
    dry_run_timeout: std::time::Duration,
    /// The analytics, metrics, exceptions, diagnostics and exports, bounded
    /// in number (ADR-0189 decision 2); the worklists, queries, the objects
    /// available and a subscription's pages, bounded alike beside them; and a
    /// change's dry runs, one at a time (fifth production review N-1 and
    /// DRYRUN-MEMORY, ADR-0190 decision 2).
    admission: Admission,
    worklists: Admission,
    dry_runs: Admission,
    /// An object's history and events by position, each a page of 8 MiB at
    /// most, never behind the worklists' whole-type scans, where a person's
    /// history waited 2.4 to 4.8 s (eighth production review, ADR-0195
    /// decision 5).
    objects: Admission,
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
    /// The requests waiting for the writer, committed together (ADR-0208).
    groups: Groups,
}

/// What the service watches of its writes and answers (fourth production
/// review): when a write last committed and last met another process's
/// lock past its retries, each verdict's kind, and each route's durations.
#[derive(Debug, Default)]
struct Watch {
    /// `/health`'s read of the store, run beside it ([`probed`]).
    probe: Probe,
    written_at: AtomicI64,
    stalled_at: AtomicI64,
    /// Since when writes have waited for another process's lock, as the
    /// store tells it, and the wait last said in the log, so each is said
    /// once (ADR-0195 decision 6).
    lock_wait: Arc<AtomicI64>,
    lock_wait_told: AtomicI64,
    /// Whether the log last said the store's volume too full for a
    /// checkpoint, so its fall and its recovery are each said once (ADR-0200
    /// decision 7).
    disk_low_told: std::sync::atomic::AtomicBool,
    /// The writes answered 500 since one was answered otherwise, and when
    /// the last was (ADR-0195 decision 6).
    fails: std::sync::atomic::AtomicU64,
    failed_at: AtomicI64,
    verdicts: Mutex<std::collections::BTreeMap<&'static str, u64>>,
    durations: Mutex<std::collections::BTreeMap<String, [u64; BUCKETS.len()]>>,
    answered: std::sync::atomic::AtomicU64,
    /// The write-ahead log's cuts, those a read prevented, and those left
    /// for the next look where a read ran past a moment's hold.
    cuts: std::sync::atomic::AtomicU64,
    cuts_missed: std::sync::atomic::AtomicU64,
    cuts_deferred: std::sync::atomic::AtomicU64,
    /// Why the service serves its store no more, once it found the store
    /// lost: its file replaced or written over, its log removed or
    /// replaced, or the store unreadable; kept until a restart (ADR-0205
    /// decision 8).
    latched: std::sync::OnceLock<String>,
    /// The store's write-ahead log as the writer opened it, its device and
    /// inode, compared at each read and write as the store's file is.
    wal_identity: Mutex<Option<(u64, u64)>>,
    /// Since when an erasure's vacuum on PostgreSQL has been due, 0 while
    /// none is, and whether its long wait was said.
    vacuum_due_since: AtomicI64,
    vacuum_told: std::sync::atomic::AtomicBool,
    /// The database a store on PostgreSQL was first found in, compared
    /// each time a lost connection is opened again, as the store's file is
    /// at each write (durability on PostgreSQL, finding 2).
    database: std::sync::OnceLock<String>,
}

/// The upper bounds, in seconds, of `/metrics`' duration histogram.
const BUCKETS: [f64; 8] = [0.005, 0.025, 0.1, 0.5, 1.0, 2.5, 10.0, 30.0];

/// A request's answer from the writer: its verdict and what a refusal
/// tells, or why it has none.
type Requested = Result<Result<(Verdict, Told), request::NoVerdict>, Fault>;

/// A request waiting for the writer, to be committed with the requests
/// waiting beside it (ADR-0208): what it asks, the evaluators' answers it
/// carries, and its answer once a group has run it.
struct Ticket {
    req: Request,
    asked: std::collections::BTreeMap<String, String>,
    answer: Mutex<Option<Requested>>,
}

impl Ticket {
    fn give(&self, answer: Requested) {
        *self
            .answer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(answer);
    }

    fn taken(&self) -> Option<Requested> {
        self.answer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }
}

/// The requests waiting for the writer, in the order they came, and the
/// groups the writer committed and the requests they held, a request run
/// alone a group of one (ADR-0208 decisions 1 and 5).
#[derive(Default)]
struct Groups {
    waiting: Mutex<Vec<Arc<Ticket>>>,
    commits: std::sync::atomic::AtomicU64,
    held: std::sync::atomic::AtomicU64,
}

/// The most requests one group holds (ADR-0208 decision 1).
const GROUP_MOST: usize = 64;

/// How recently a write must have met another process's lock past its
/// retries, with none committed since, for `/health` to say writes stall.
const STALLED_MS: i64 = 60_000;

/// How many writes in a row answered 500 `/health` says are failing.
const WRITES_FAILING: u64 = 3;

/// The routes that write, whose answers `/health`'s `writes_failing` reads.
const WRITE_ROUTES: [&str; 6] = [
    "/v1/request",
    "/v1/maintain",
    "/v1/acknowledge",
    "/v1/import_batch",
    "/v1/publish",
    "/v1/upgrade",
];

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

    /// A write's answer, by its status: one the service failed, a 500, or
    /// any other, which ends a run of them.
    fn wrote(&self, status: u16, now: i64) {
        if status == 500 {
            self.fails.fetch_add(1, Ordering::SeqCst);
            self.failed_at.store(now, Ordering::SeqCst);
        } else {
            self.fails.store(0, Ordering::SeqCst);
        }
    }

    /// Whether writes fail: three or more answered 500 since one was
    /// answered otherwise, the last of them lately, where `/health` said
    /// 200 while a backup copied over the live file failed every write
    /// (eighth production review, ADR-0195 decision 6). How many.
    fn failing(&self, now: i64) -> Option<u64> {
        let fails = self.fails.load(Ordering::SeqCst);
        (fails >= WRITES_FAILING && now - self.failed_at.load(Ordering::SeqCst) < STALLED_MS)
            .then_some(fails)
    }

    /// Since when a write has waited for another process's lock, where one
    /// is waiting.
    fn waiting(&self) -> Option<i64> {
        Some(self.lock_wait.load(Ordering::SeqCst)).filter(|w| *w != 0)
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

/// Why the service serves its store no more, where it has found it lost:
/// its file moved or replaced, its write-ahead log removed or replaced, its
/// file written over in place, or a read finding it unreadable. Latched
/// until a restart, so no write is acknowledged into a store the service
/// already found bad, where after `/health` answered unreadable the next
/// writes answered 200 and were lost at the next start (the audit's
/// durability S3 to S5, D688, ADR-0205 decision 8). None while it holds.
fn store_lost(shared: &Service) -> Option<String> {
    if let Some(why) = shared.watch.latched.get() {
        return Some(why.clone());
    }
    let why = store_replaced(shared)
        .or_else(|| log_replaced(shared))
        .or_else(|| store_overwritten(shared))?;
    Some(latch(shared, why))
}

/// The store latched lost, said once (ADR-0205 decision 8); what it is
/// latched for, the first cause found.
fn latch(shared: &Service, why: String) -> String {
    if shared.watch.latched.set(why.clone()).is_ok() {
        say!(
            "the store is served no more until a restart, every read and write refused and the stop's checkpoint skipped: {why}"
        );
    }
    shared.watch.latched.get().cloned().unwrap_or(why)
}

/// A store's write-ahead log's path.
fn wal_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    std::path::PathBuf::from(wal)
}

/// The write-ahead log the writer now opens, noted, where the service
/// itself has opened the store again, which may make the log anew.
fn note_log(shared: &Service) {
    if let (Some(path), Ok(mut held)) = (&shared.store_path, shared.watch.wal_identity.lock()) {
        *held = file_identity(&wal_path(path));
    }
}

/// Whether the store's write-ahead log is still the one its writer opened,
/// where one removed under the service took every write acknowledged into
/// it with it at the next start (the audit's durability S4). A log not yet
/// made when the service opened the store is taken as the one it makes.
fn log_replaced(shared: &Service) -> Option<String> {
    let path = shared.store_path.as_ref()?;
    let wal = wal_path(path);
    let now = file_identity(&wal);
    let mut held = shared.watch.wal_identity.lock().ok()?;
    match (*held, now) {
        (Some(was), Some(is)) if was == is => None,
        (None, Some(is)) => {
            *held = Some(is);
            None
        }
        (None, None) => None,
        _ => Some(format!(
            "store_replaced: {}, the store's write-ahead log, is no longer the one the service opened, removed or replaced under it, so a write would go into a log no start reads; stop the service, keep its files, and restore the store (operate.md, Backups and restore)",
            wal.display()
        )),
    }
}

/// Whether the store's file still begins as a SQLite database does, where
/// one written over in place, its device and inode unchanged, took the
/// writes answered after it into a log no start could read (the audit's
/// durability S3).
fn store_overwritten(shared: &Service) -> Option<String> {
    let path = shared.store_path.as_ref()?;
    let mut head = [0u8; 16];
    let whole = std::fs::File::open(path)
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut head))
        .is_ok();
    (!whole || &head != b"SQLite format 3\0").then(|| {
        format!(
            "store_unreadable: {} no longer begins as a SQLite database, written over in place; stop the service, keep its files, and restore the store (operate.md, Backups and restore)",
            path.display()
        )
    })
}

/// Whether a fault's words are SQLite's for a store it cannot read,
/// `SQLITE_CORRUPT`'s or `SQLITE_NOTADB`'s, as any connection, a read's
/// among them, meets them (the audit's durability S5).
fn names_unreadable(text: &str) -> bool {
    text.contains("database disk image is malformed") || text.contains("file is not a database")
}

/// A store a read or a write found unreadable, as it is answered: 503
/// `store_unreadable`, which latches the service (ADR-0205 decision 8).
fn unreadable(text: &str) -> Fault {
    Fault::Unavailable(format!(
        "store_unreadable: the store answered \"{}\"; stop the service, keep its files, and restore the store (operate.md, Backups and restore)",
        said(text)
    ))
}

/// A mark on an answer that found its store lost, which the logging layer
/// latches the service on (ADR-0205 decision 8).
#[derive(Clone, Debug)]
struct Lost(String);

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

/// How long `/health` waits for its read of the store before it answers
/// `503 store_stalled`.
const HEALTH_WAIT_MS: i64 = 5_000;

/// `/health`'s read of the store, one at a time: when the one running
/// started, how many have started and finished, and what the last found.
#[derive(Debug, Default)]
struct Probe {
    state: Mutex<Probed>,
    done: std::sync::Condvar,
}

#[derive(Debug, Default)]
struct Probed {
    since: Option<i64>,
    started: u64,
    finished: u64,
    last: Option<Result<(i64, i64), Fault>>,
}

/// The store read for `/health`, its library's version and its clock, on a
/// thread of its own and waited for [`HEALTH_WAIT_MS`] from its start at
/// most: a store whose database stops answering without closing its
/// connections, frozen or stalled on its disk, answers `503
/// store_stalled`, where `/health` waited on it with every other request
/// and said nothing (durability on PostgreSQL 3). A read still running is
/// joined, never started again, so a stall holds one thread and one read
/// connection however often `/health` is asked.
fn probed(shared: &Shared) -> Result<(i64, i64), Fault> {
    let poisoned = || Fault::Internal("a previous request panicked".to_string());
    let probe = &shared.watch.probe;
    let mut st = probe.state.lock().map_err(|_| poisoned())?;
    if st.since.is_none() {
        let mine = shared.clone();
        std::thread::Builder::new()
            .name("health".to_string())
            .spawn(move || {
                let found = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut r = reading(&mine)?;
                    let s = r.store()?;
                    let version = s.lib.version;
                    s.snapshot()?.get(objectflow_core::model::OPERATOR)?;
                    Ok((version, s.now))
                }))
                .unwrap_or_else(|_| {
                    Err(Fault::Internal(
                        "the health read panicked; the line above says where".to_string(),
                    ))
                })
                // its read stopped at its own deadline by a store not
                // answering, said as the stall it is (ADR-0215)
                .map_err(|f| match f {
                    Fault::TimedOut(why) => Fault::Unavailable(format!(
                        "store_stalled: the store did not answer /health's read by its deadline: {why}"
                    )),
                    other => other,
                });
                let probe = &mine.watch.probe;
                if let Ok(mut st) = probe.state.lock() {
                    st.since = None;
                    st.finished = st.started;
                    st.last = Some(found);
                }
                probe.done.notify_all();
            })
            .map_err(|e| Fault::Internal(format!("the health read could not start: {e}")))?;
        st.since = Some(wall_millis());
        st.started += 1;
    }
    let (want, since) = (st.started, st.since.unwrap_or_else(wall_millis));
    while st.finished < want {
        let left = since + HEALTH_WAIT_MS - wall_millis();
        if left <= 0 {
            return Err(Fault::Unavailable(format!(
                "store_stalled: the store has not answered /health's read in {} s; a database that stops answering holds every request waiting on it (operate.md, PostgreSQL)",
                (wall_millis() - since).max(0) / 1000
            )));
        }
        st = probe
            .done
            .wait_timeout(st, std::time::Duration::from_millis(left.unsigned_abs()))
            .map_err(|_| poisoned())?
            .0;
    }
    st.last.clone().unwrap_or_else(|| {
        Err(Fault::Internal(
            "the health read finished with nothing".to_string(),
        ))
    })
}

/// How long the writer has been held, in seconds, or none while it is free:
/// through a write's retries on another process's lock and the writes
/// refused after them, from the first retry, where it began again at each
/// write and an alert on a minute never fired (eighth production review,
/// ADR-0195 decision 6).
fn writer_held(shared: &Service) -> f64 {
    match shared.writer_since.load(Ordering::SeqCst) {
        0 => 0.0,
        since => {
            let from = shared.watch.waiting().map_or(since, |w| w.min(since));
            (wall_millis() - from).max(0) as f64 / 1000.0
        }
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
    /// The store's file, which `GET /health` opens afresh; none for a store
    /// on PostgreSQL, which its reads reach (ADR-0213).
    path: Option<std::path::PathBuf>,
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
    /// Whether it is a change's dry runs' one slot, whose wait is said by
    /// its own flag, where it named --read-timeout and --max-reads (ninth
    /// production review N-8).
    dry_runs: bool,
    /// The stop of each read running in it, which an urgent cut of the log
    /// moves where it finds the pool not drained; a pool that drained keeps
    /// its reads running, where one epoch for every pool stopped a worklist
    /// of 0.1 s as readily as a 30 s report (the audit's cost E4, ADR-0205
    /// decision 14).
    readers: Mutex<Vec<Arc<std::sync::atomic::AtomicU64>>>,
    /// How long its reads waited for their turn, where only how many were
    /// waiting was said (ADR-0201 decision 11).
    waits: Mutex<Waited>,
}

/// How long reads waited for their turn: how many waited at most each of
/// `BUCKETS`' bounds, as Prometheus reads a histogram's buckets, and their
/// number and sum in seconds.
#[derive(Clone, Debug, Default, PartialEq)]
struct Waited {
    buckets: [u64; BUCKETS.len()],
    count: u64,
    sum: f64,
}

thread_local! {
    /// The stop of the read this thread runs: moved where its client has
    /// gone (ADR-0205 decision 16), or where an urgent cut of the log finds
    /// its pool not drained (decision 14).
    static READ_STOP: std::cell::RefCell<Option<Arc<std::sync::atomic::AtomicU64>>> =
        const { std::cell::RefCell::new(None) };
}

/// The stop of the read this thread runs, made where it has none.
fn read_stop() -> (Arc<std::sync::atomic::AtomicU64>, bool) {
    READ_STOP.with(|s| {
        let mut held = s.borrow_mut();
        match held.as_ref() {
            Some(stop) => (stop.clone(), false),
            None => {
                let stop = Arc::new(std::sync::atomic::AtomicU64::new(0));
                *held = Some(stop.clone());
                (stop, true)
            }
        }
    })
}

/// A read's work, run off its connection's task, so the connection sees
/// its client go and drops the answer it waits for, which moves the read's
/// stop: the read ends at its next step, where it ran to its deadline for
/// no one, and a client retrying filled a pool with reads nobody waited
/// for (D656, ADR-0205 decision 16). A panic in it is answered `500`, as
/// [`answering`]'s is. A write stays on [`answering`], since a write
/// completes whether or not its client waits, its key replaying it.
async fn answered_off<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Fault> + Send + 'static,
) -> Result<T, Fault> {
    struct Gone(Arc<std::sync::atomic::AtomicU64>);
    impl Drop for Gone {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let stop = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let _gone = Gone(stop.clone());
    tokio::task::spawn_blocking(move || {
        READ_STOP.with(|s| *s.borrow_mut() = Some(stop));
        let out =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|_| {
                Err(Fault::Internal(
                    "a request panicked; the line above says where".to_string(),
                ))
            });
        READ_STOP.with(|s| *s.borrow_mut() = None);
        out
    })
    .await
    .unwrap_or_else(|e| Err(Fault::Internal(format!("a read's work ended: {e}"))))
}

impl Admission {
    fn new(most: usize) -> Self {
        Admission {
            running: Mutex::new(0),
            freed: std::sync::Condvar::new(),
            most,
            waiting: std::sync::atomic::AtomicUsize::new(0),
            barred: std::sync::atomic::AtomicBool::new(false),
            dry_runs: false,
            readers: Mutex::new(Vec::new()),
            waits: Mutex::new(Waited::default()),
        }
    }

    /// How long its reads have waited for their turn.
    fn waited(&self) -> Waited {
        self.waits.lock().map(|w| w.clone()).unwrap_or_default()
    }

    /// A change's dry runs' slot, one at a time.
    fn for_dry_runs() -> Self {
        Admission {
            dry_runs: true,
            ..Admission::new(1)
        }
    }

    /// New reads held back, and those running waited out until `until`:
    /// whether none is running then. `release` lets the held ones in.
    fn hold(&self, until: std::time::Instant) -> bool {
        self.bar();
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

    /// New reads held back, those running left to run.
    fn bar(&self) {
        self.barred.store(true, Ordering::SeqCst);
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
        let began = std::time::Instant::now();
        self.waiting.fetch_add(1, Ordering::SeqCst);
        let taken = (|| {
            let mut running = self.running.lock().map_err(|_| panicked())?;
            while *running >= self.most || self.barred.load(Ordering::SeqCst) {
                let left = until.saturating_duration_since(std::time::Instant::now());
                if left.is_zero() && self.dry_runs {
                    return Err(Fault::TimedOut(format!(
                        "another change's dry run ran for the whole of this one's deadline, --dry-run-timeout ({} s), so it was not started; dry runs take their one slot in turn: send it again",
                        timeout.as_secs()
                    )));
                }
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
        // each wait measured, one ended by its deadline among them
        let waited = began.elapsed().as_secs_f64();
        if let Ok(mut w) = self.waits.lock() {
            for (i, bound) in BUCKETS.iter().enumerate() {
                if waited <= *bound {
                    w.buckets[i] += 1;
                }
            }
            w.count += 1;
            w.sum += waited;
        }
        taken.map(|()| {
            let (stop, made) = read_stop();
            if let Ok(mut readers) = self.readers.lock() {
                readers.push(stop.clone());
            }
            Slot(Some((self, stop, made)))
        })
    }

    /// Each read running in it stopped at its next step.
    fn stop_readers(&self) {
        if let Ok(readers) = self.readers.lock() {
            for stop in readers.iter() {
                stop.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    fn running(&self) -> usize {
        self.running.lock().map_or(0, |n| *n)
    }
}

/// A heavy read's slot, given back when the read ends, with its read's
/// stop, which the pool keeps while it runs, and whether taking the slot
/// made it; none for a sandbox's, which take the writer.
struct Slot<'a>(Option<(&'a Admission, Arc<std::sync::atomic::AtomicU64>, bool)>);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        if let Some((a, stop, made)) = &self.0 {
            if let Ok(mut readers) = a.readers.lock() {
                readers.retain(|r| !Arc::ptr_eq(r, stop));
            }
            if *made {
                READ_STOP.with(|s| *s.borrow_mut() = None);
            }
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
    // a change's dry run under a deadline of its own (ADR-0194 decision 2)
    let timeout = if std::ptr::eq(pool, &shared.dry_runs) {
        shared.dry_run_timeout
    } else {
        shared.read_timeout
    };
    let until = std::time::Instant::now() + timeout;
    let slot = match shared.reads {
        Some(_) => pool_of(shared, pool, actor)?.take(until, timeout)?,
        None => Slot(None),
    };
    Ok((slot, reading_until(shared, until)?))
}

/// A slot among the objects' reads, beside `history` and `events`, for a
/// read of one object, what it offers or what a request of it would do,
/// where what it computes of its type's rules grows with the store
/// (`reads::grows_with_store`), where such reads ran in no pool and 64 of
/// them peaked at 4.49 GB (thirteenth production review C); never among
/// the analytics, where a person's `get` waited 25.5 s behind per-person
/// metrics (fourteenth production review); none for any other read, which
/// never waits.
fn object_slot<'s>(
    shared: &'s Shared,
    id: Option<&str>,
    type_: Option<&str>,
    actor: Option<&str>,
    what: objectflow_core::reads::ObjectRead,
) -> Result<Slot<'s>, Fault> {
    if shared.reads.is_none() {
        return Ok(Slot(None));
    }
    let grows = {
        let mut r = reading(shared)?;
        let s = r.store()?;
        let tn = match (id, type_) {
            (Some(id), _) => s.snapshot()?.type_of(id)?,
            (None, Some(t)) => Some(t.to_string()),
            (None, None) => None,
        };
        tn.is_some_and(|t| objectflow_core::reads::grows_with_store(&s.lib, &t, what))
    };
    if !grows {
        return Ok(Slot(None));
    }
    let until = std::time::Instant::now() + shared.read_timeout;
    pool_of(shared, &shared.objects, actor)?.take(until, shared.read_timeout)
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
    if let Some(replaced) = store_lost(shared) {
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
    // its connection bounded by the deadline too, so a database that stops
    // answering holds no read past it (ADR-0215)
    let mut store = reads.pool.read(lib, earlier, now, answers, Some(until))?;
    // a read answers within its deadline or is stopped there, so no read
    // holds a reader, and an erasure's truncation behind it, without bound
    // (ADR-0184 decision 3); and is stopped where an urgent cut of the log
    // moves the epoch it began in (ADR-0194 decision 4)
    store.deadline = Some(until);
    // the epoch of the pool whose slot it holds; a read holding none is
    // short, never held by a cut, and the cut waits for it (ADR-0205
    // decision 14)
    store.stop = READ_STOP.with(|s| {
        s.borrow()
            .as_ref()
            .map(|stop| (stop.clone(), stop.load(Ordering::SeqCst)))
    });
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
#[derive(Clone, Debug)]
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
    /// A read refused for what it reads, answered as a read's refusals are,
    /// `refused` and a `reason` and no items (`errors.md`, ADR-0205
    /// decision 2).
    Refused(J),
}

impl IntoResponse for Fault {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Fault::Refused(body) => return (StatusCode::OK, Json(body)).into_response(),
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
        let lost =
            (code == "store_unreadable" || code == "store_replaced").then(|| message.clone());
        let mut answer = (status, Json(json!({ "fault": message, "code": code }))).into_response();
        if let Some(why) = lost {
            answer.extensions_mut().insert(Lost(why));
        }
        answer
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
            // the store's words carried through a world's read of it
            NotRun::Unreadable(w) if w.contains(objectflow_core::store::DEADLINE_PASSED) => {
                Fault::TimedOut(w)
            }
            NotRun::Unreadable(w) if names_unreadable(&w) => unreadable(&w),
            NotRun::Unreadable(w) => Fault::Internal(format!("storage unavailable: {w}")),
            NotRun::TimedOut(w) => Fault::TimedOut(w),
            // a value a read derives or filters by past what its type
            // holds, refused naming its object and no value, where it was
            // answered null or its object left out (ADR-0205 decision 2)
            NotRun::OutOfRange(id) => Fault::Refused(json!({
                "refused": "value_in_range",
                "reason": "a value it derives or filters by is past what its type holds",
                "object_id": if id.is_empty() { J::Null } else { J::String(id) },
            })),
        }
    }
}

/// Whether a request opts into reading by a personal value, which the
/// gateway sets where the person or agent it acts for may (ADR-0206).
fn opted(body: &J) -> Result<bool, Fault> {
    flag(body, "reads_personal")
}

/// A read refused for selecting by a personal value without the opt-in, as
/// a read's refusals are answered (ADR-0206 decision 2).
fn personal_refused(reason: &str) -> Fault {
    Fault::Refused(json!({ "refused": "personal", "reason": reason }))
}

/// A refusal's answer without the holder of a personal value it names: a
/// uniqueness over a personal member, or `identity_taken` where identities
/// are personal, names no object without the request's opt-in, where a
/// write's refusal named the person holding an address (the audit's
/// personal data R16, ADR-0206 decision 3).
fn without_holder(lib: &objectflow_core::model::Library, mut out: J) -> J {
    let names = |c: &J| {
        c.as_str()
            .is_some_and(|c| reads::clause_names_personal_holder(lib, c))
    };
    let mut held = names(&out["clause"]);
    if let Some(failures) = out.get_mut("failures").and_then(J::as_array_mut) {
        for f in failures.iter_mut() {
            if names(&f["invariant"]) {
                f["object_id"] = J::Null;
                held = true;
            }
        }
    }
    if held && out.get("objects").is_some() {
        out["objects"] = json!([]);
    }
    out
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
        // a read stopped at its deadline on the database, which did not
        // answer in time, answered as a read past its deadline is, where it
        // waited with the database (ADR-0215)
        if e.is_deadline_passed() {
            return Fault::TimedOut(e.0);
        }
        // a full volume answers 503, which a client may retry, where it
        // answered 500, the service's own fault (fourth production review)
        if e.is_disk_full() {
            DISK_FULL_AT.store(wall_millis(), Ordering::SeqCst);
            return Fault::Unavailable(e.0);
        }
        // the store's database out of reach, which a client may retry once
        // it is back, where it answered 500 (durability on PostgreSQL 1)
        if e.is_unavailable() {
            return Fault::Unavailable(e.0);
        }
        // a version whose names PostgreSQL would cut, which this store
        // cannot hold, where it answered 500 or, worse, cut them and merged
        // two tables (records on PostgreSQL, F1)
        if e.is_name_too_long() || e.is_value_too_long() {
            return Fault::Conflict(e.0);
        }
        // a lock another session held past the busy timeout, outside a
        // request, which answers it `stale`: a publish beside a backup or a
        // long read on PostgreSQL, which a caller retries, where it
        // answered 500 (records on PostgreSQL F3, durability 7)
        if e.is_busy() {
            return Fault::Unavailable(e.0);
        }
        if names_unreadable(&e.0) {
            return unreadable(&e.0);
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
            "actor",
            "type",
            "filter",
            "order",
            "cursor",
            "fields",
            "limit",
            "reads_personal",
        ],
    ),
    (
        "/v1/lookup",
        &["actor", "source", "value", "reads_personal"],
    ),
    ("/v1/availability", &["actor", "id", "reads_personal"]),
    (
        "/v1/available",
        &[
            "actor",
            "type",
            "transition",
            "cursor",
            "limit",
            "reads_personal",
        ],
    ),
    ("/v1/history", &["actor", "id", "follow", "limit", "cursor"]),
    ("/v1/events", &["actor", "positions"]),
    ("/v1/referenced", &["actor", "hashes"]),
    ("/v1/exceptions", &["actor", "type", "cursor"]),
    ("/v1/declaration", &["actor", "type", "version"]),
    ("/v1/tools", &["actor", "actor_kind"]),
    ("/v1/pull", &["actor", "subscription", "cursor", "limit"]),
    ("/v1/acknowledge", &["actor", "subscription", "cursor"]),
    (
        "/v1/metric",
        &[
            "actor",
            "name",
            "bind",
            "keep",
            "over",
            "filter",
            "cursor",
            "reads_personal",
        ],
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
        &[
            "files",
            "attempt_retention",
            "idempotency_retention",
            "unchecked",
        ],
    ),
    ("/sandbox/reset", &[]),
    ("/sandbox/clock", &["advance", "set"]),
    ("/sandbox/evaluators", &["answers"]),
];
const REQUEST_KEYS: &[&str] = &[
    "reads_personal",
    "transition",
    "actor",
    "object_id",
    "type",
    "inputs",
    "occurred_at",
    "propose",
    "expires_at",
    "expected_version",
    "target_version",
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
        let body = J::Object(body);
        // a request's text never holds U+0000, which PostgreSQL's text
        // cannot store, and a name it gives is at most 1 KiB, past which
        // PostgreSQL's index of the attempt log refused the refusal's row: a
        // context, a key, an actor or a transition holding either was taken
        // by SQLite and a 500 on PostgreSQL, the refusal unrecorded (records
        // on PostgreSQL, F4 and F5); an input's or an attribute's value is
        // the engine's to refuse, as before
        if let Some(why) = unholdable(&body, "", false) {
            return Err(Fault::Bad(why));
        }
        // an actor, wherever one is given, is an object of an id and a
        // principal, where a read took one of any JSON type, `"actor": [1]`
        // (eighth production review, ADR-0195 decision 7)
        actor_id(&body)?;
        Ok(Body(body))
    }
}

/// The names a request gives, at most [`NAME_BYTES`] each wherever they
/// stand: an actor's id and principal, a type, a transition, an object's
/// id, a legacy key and an idempotency key.
const NAMES: [&str; 7] = [
    "id",
    "principal",
    "type",
    "transition",
    "object_id",
    "legacy_key",
    "idempotency_key",
];

/// How long a name a request gives may be.
const NAME_BYTES: usize = 1024;

/// What in a body no store can hold, by where it stands: U+0000 in a key,
/// or in a value outside an input's or an attribute's, which the engine
/// decodes and refuses itself; a name past [`NAME_BYTES`].
fn unholdable(v: &J, at: &str, decoded: bool) -> Option<String> {
    match v {
        J::String(s) if !decoded && s.contains('\0') => Some(format!(
            "{at} holds U+0000, which a request's text never holds"
        )),
        J::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(i, x)| unholdable(x, &format!("{at}[{i}]"), decoded)),
        J::Object(m) => m.iter().find_map(|(k, x)| {
            let here = if at.is_empty() {
                k.clone()
            } else {
                format!("{at}.{k}")
            };
            if k.contains('\0') {
                return Some(format!(
                    "{here} is a key holding U+0000, which a request's text never holds"
                ));
            }
            if !decoded
                && NAMES.contains(&k.as_str())
                && let J::String(s) = x
                && s.len() > NAME_BYTES
            {
                return Some(format!(
                    "{here} is {} bytes, past the {NAME_BYTES} a name may be",
                    s.len()
                ));
            }
            unholdable(x, &here, decoded || k == "inputs" || k == "attributes")
        }),
        _ => None,
    }
}

/// A request's work, run where it may block, a panic in it answered `500`
/// and said by the log's panic line, never left to unwind the connection's
/// task, which kept its place among the open ones for good: twelve panics
/// under --max-connections 8 answered every request `503 busy` (tenth
/// production review). A writer it held is poisoned, and opened again by
/// the next write, as before.
fn answering<T>(work: impl FnOnce() -> Result<T, Fault>) -> Result<T, Fault> {
    tokio::task::block_in_place(move || {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|_| {
            Err(Fault::Internal(
                "a request panicked; the line above says where".to_string(),
            ))
        })
    })
}

/// A request on the writer, committed with the requests waiting beside it
/// (ADR-0208): queued, then run by whichever request takes the writer
/// next, its own group's leader or a later one, in the order they queued
/// and at most [`GROUP_MOST`] at a time, and answered once its group
/// commits. A request finding the writer free and nothing queued is a
/// group of one and runs alone, as before.
fn queued(
    shared: &Shared,
    req: &Request,
    asked: &std::collections::BTreeMap<String, String>,
) -> Requested {
    let ticket = Arc::new(Ticket {
        req: req.clone(),
        asked: asked.clone(),
        answer: Mutex::new(None),
    });
    let waiting = || {
        shared
            .groups
            .waiting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    };
    waiting().push(ticket.clone());
    loop {
        let mut writing = match locked(shared) {
            Ok(w) => w,
            Err(f) => {
                // answered by a group that ran before the writer refused,
                // or else taken off the queue, so no later group runs a
                // request whose caller was answered a fault
                if let Some(a) = ticket.taken() {
                    return a;
                }
                waiting().retain(|t| !Arc::ptr_eq(t, &ticket));
                return Err(f);
            }
        };
        if let Some(a) = ticket.taken() {
            return a;
        }
        let group: Vec<Arc<Ticket>> = {
            let mut w = waiting();
            let n = w.len().min(GROUP_MOST);
            w.drain(..n).collect()
        };
        run_group(shared, &mut writing, &group);
        drop(writing);
        if let Some(a) = ticket.taken() {
            return a;
        }
    }
}

/// A group of requests run on the writer and committed with one flush, each
/// given its answer once it has (ADR-0208 decisions 1 and 2). A store error
/// or a panic during it, or a commit that fails, rolls it back, and each
/// request runs again alone, so a fault answers only the request that
/// meets it (decision 4). Every request of the group is given an answer.
fn run_group(shared: &Shared, sandbox: &mut Sandbox, group: &[Arc<Ticket>]) {
    shared.groups.commits.fetch_add(1, Ordering::SeqCst);
    shared.groups.held.fetch_add(
        u64::try_from(group.len()).unwrap_or(u64::MAX),
        Ordering::SeqCst,
    );
    if let [one] = group {
        one.give(alone(shared, sandbox, one));
        return;
    }
    let begun = store(sandbox).is_ok_and(|s| s.begin_group().is_ok());
    let mut answers = Vec::with_capacity(group.len());
    let mut whole = begun;
    for t in group.iter().take_while(|_| begun) {
        let ran = {
            let Ok(s) = store(sandbox) else {
                whole = false;
                break;
            };
            let given = s.answers.clone();
            s.answers.extend(t.asked.clone());
            let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                request::request_told(s, &t.req)
            }));
            s.answers = given;
            ran
        };
        match ran {
            Ok(Ok(out)) => answers.push(out),
            Ok(Err(_)) => {
                whole = false;
                break;
            }
            Err(_) => {
                whole = false;
                if let Ok(s) = store(sandbox) {
                    s.abort_group();
                }
                say!(
                    "a request panicked holding the writer in a group; {}",
                    reopened(sandbox)
                );
                note_log(shared);
                break;
            }
        }
    }
    let committed = whole && store(sandbox).is_ok_and(|s| s.commit_group().is_ok());
    if !committed {
        if let Ok(s) = store(sandbox) {
            s.abort_group();
        }
        for t in group {
            t.give(alone(shared, sandbox, t));
        }
        return;
    }
    for (t, out) in group.iter().zip(answers) {
        t.give(Ok(out));
    }
}

/// A request run alone on the writer, with the evaluators' answers it
/// carries; one that panics is answered 500 and the store opened again, as
/// a request holding the writer is (POISON).
fn alone(shared: &Shared, sandbox: &mut Sandbox, t: &Ticket) -> Requested {
    let ran = {
        let s = store(sandbox)?;
        let given = s.answers.clone();
        s.answers.extend(t.asked.clone());
        let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            request::request_told(s, &t.req)
        }));
        s.answers = given;
        ran
    };
    match ran {
        Ok(out) => Ok(out?),
        Err(_) => {
            say!(
                "a request panicked holding the writer; {}",
                reopened(sandbox)
            );
            note_log(shared);
            Err(Fault::Internal(
                "a request panicked; the line above says where".to_string(),
            ))
        }
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
            note_log(shared);
            guard
        }
    };
    if let Some(replaced) = store_lost(shared) {
        return Err(Fault::Unavailable(replaced));
    }
    // a deployment's store on PostgreSQL whose connection a statement
    // found lost, to the database's restart, a terminated session or an
    // idle timeout: opened again before this write, its lock taken again,
    // where every write after the loss answered 500 until a person
    // restarted the service; where it cannot be yet, this write is answered
    // unavailable and the next tries again (durability on PostgreSQL 1)
    let mut guard = guard;
    // a store due an upgrade is not open, deliberately, and is not lost
    if guard.deployment
        && guard.postgres.is_some()
        && guard.due.is_none()
        && guard.store.as_ref().is_none_or(Store::connection_lost)
    {
        let said = reopened(&mut guard);
        say!("the store's connection to its database was lost; {said}");
        note_log(shared);
        // the database found not the store's at its opening, which refuses
        // it: served no more, rather than tried again at every write
        if said.contains(objectflow_core::store::NOT_ITS_DATABASE) {
            return Err(Fault::Unavailable(latch(shared, said)));
        }
        // a database other than the one the store was in, a standby a
        // failover promoted or a restore in its place, which may lack the
        // last writes the store answered: served no more until a person
        // marks it a copy and starts the service again, where the service
        // would write on as if it were the store (ADR-0207)
        let now = guard.store.as_ref().and_then(Store::database_origin);
        if let (Some(was), Some(now)) = (shared.watch.database.get(), &now)
            && was != now
        {
            guard.store = None;
            return Err(Fault::Unavailable(latch(
                shared,
                format!(
                    "the store's database is not the one it was, {was} then and {now} now: a standby a failover promoted, or a restore in its place, which may lack the last writes the store answered; mark it a copy as a restored database is (operate.md) and start the service again"
                ),
            )));
        }
        if guard.store.is_none() {
            return Err(Fault::Unavailable(
                "the store's database could not be reached; the next write tries again".into(),
            ));
        }
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
    if let Some(config) = sandbox.postgres.clone().filter(|_| sandbox.deployment) {
        // the store's connection, holding its lock, let go first, so the
        // one opened takes the lock again
        sandbox.store = None;
        let opened = Store::open_postgres(&config, wall_clock(), false).and_then(|mut s| {
            (s.attempt_retention, s.idempotency_retention) = sandbox.retentions;
            s.lock_wait = Some(sandbox.lock_wait.clone());
            s.set_busy_timeout(sandbox.busy_timeout)?;
            Ok(s)
        });
        return match opened {
            Ok(s) => {
                sandbox.store = Some(s);
                format!("the store opened again from {config:?}")
            }
            Err(e) => format!("the store could not be opened again from {config:?}: {e}"),
        };
    }
    let Some(path) = sandbox
        .path
        .clone()
        .filter(|_| sandbox.deployment && sandbox.due.is_none())
    else {
        return "the sandbox's store kept as it is".to_string();
    };
    let opened = Store::open(&path, wall_clock()).and_then(|mut s| {
        (s.attempt_retention, s.idempotency_retention) = sandbox.retentions;
        s.lock_wait = Some(sandbox.lock_wait.clone());
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
            created,
            flags: fs,
            replayed,
        } => {
            json!({"object_id": object_id, "version": version, "events": events, "created": created,
                   "replayed": replayed, "flags": flags(fs)})
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
        Verdict::UnknownType { type_, .. } => json!({ "type": type_ }),
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
        // its place among its object's events and its source, which the store
        // records and `export` answered while every other read listed them
        // not computed (the guides' audit G-96)
        "object_seq": e.object_seq, "source": e.source,
        "declaration_version": e.declaration_version,
        "recorded_at": timestamp_text(e.recorded_at),
        "occurred_at": e.occurred_at.map(timestamp_text), "imported": e.imported,
        "payload": payload, "cause": e.cause, "reads": e.reads, "context": e.context,
        "consulted": consulted, "as_of": as_of,
        // the serialisation retries its request took (ADR-0178 decision 2)
        "retries": e.retries,
    })
}

const EVENT_NOT_COMPUTED: [&str; 3] = ["taint_version", "txn", "flags"];

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
    answering(move || {
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
        // the flow checked first, its modules without their examples, which
        // a publish runs, as the studio's load checks it: a flow the checker
        // refuses is answered with its findings, where it was installed and
        // its first request reaching what the checker names faulted 500
        // (issue #26); `unchecked` installs it as it was
        let mut notices = 0;
        if !flag(&body, "unchecked")? {
            let modules: Vec<(String, String)> = files
                .iter()
                .filter(|(n, _)| !n.ends_with(".examples.yaml"))
                .cloned()
                .collect();
            let found = objectflow_core::check::check(&modules, &std::collections::BTreeMap::new());
            let fatal: Vec<J> = found
                .iter()
                .filter(|f| f.severity == objectflow_core::check::Severity::Fatal)
                .map(|f| json!({"file": f.file, "line": f.line, "code": f.code, "message": f.message}))
                .collect();
            if !fatal.is_empty() {
                return Ok(Json(json!({"loaded": false, "findings": fatal})));
            }
            notices = found.len();
        }
        let lib = match Library::load(&files) {
            Ok(lib) => lib,
            Err(e) => {
                return Ok(Json(json!({"loaded": false, "findings": [
                {"file": e.file, "line": e.line, "code": "load", "message": e.message}]})));
            }
        };
        // the types the load installed, as the reference runner lists them
        let answer = json!({"loaded": true, "modules": lib.modules, "notices": notices, "now": CLOCK_START,
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
    answering(move || {
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
    answering(move || {
        let mut sandbox = locked(&shared)?;
        let s = store(&mut sandbox)?;
        // one move at a time, where both were taken in an order of their own
        // (the guides' audit G-46)
        if body.get("advance").is_some() && body.get("set").is_some() {
            return Err(Fault::Bad(
                "the clock takes advance or set, not both".to_string(),
            ));
        }
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
    answering(move || {
        let mut sandbox = locked(&shared)?;
        store(&mut sandbox)?;
        if let Some(answers) = object(&body, "answers")? {
            // each answer one an evaluator gives, where any other was taken
            // and then read as unanswered (the guides' audit G-46)
            if let Some((f, a)) = answers.iter().find(|(_, a)| {
                !matches!(a.as_str(), Some("satisfied" | "unsatisfied" | "unanswered"))
            }) {
                return Err(Fault::Bad(format!(
                    "{f} answers {a}: an evaluator answers satisfied, unsatisfied or unanswered"
                )));
            }
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
    answering(move || {
        let req = request_of(&body)?;
        // a change's dry run, which a `submit` or `refresh` records, computed
        // on a read beside the writes, so no write waits for it; the request
        // takes it only where nothing it read has changed (DRYRUN-LOCK)
        let flow = req.object_id.is_some() && changes_the_flow(&shared, &req)?;
        let prepared = match &req.object_id {
            Some(id) if shared.reads.is_some() && flow => {
                // past its deadline, or stopped by an urgent cut, it is
                // answered so, never computed again under the lock, where
                // every write waited for it unbounded and unsaid
                // (PUBLISH-SCALE); any other read that cannot prepare it
                // leaves it to the request, which says why it cannot
                match dry_running(&shared, id, actor_id(&body)?, &req.actor) {
                    Ok(Some((read, report)))
                        if matches!(read.state.as_str(), "DRAFTED" | "SUBMITTED") =>
                    {
                        Some(objectflow_core::publish::Prepared {
                            change: read.change,
                            change_version: read.change_version,
                            at: report.version - 1,
                            report,
                        })
                    }
                    Err(f @ Fault::TimedOut(_)) => return Err(f),
                    _ => None,
                }
            }
            _ => None,
        };
        let stopped = |out: &Result<(Verdict, Told), request::NoVerdict>| match out {
            Err(request::NoVerdict::NotRun(NotRun::Unanswered(c))) => Some((**c).clone()),
            _ => None,
        };
        // a request that leaves the flow as it is commits with those
        // waiting beside it (ADR-0208 decision 3)
        let (v, told) = if prepared.is_none() && !flow {
            consulting_with(&shared, |asked| queued(&shared, &req, asked), stopped)??
        } else {
            consulting(
                &shared,
                false,
                |s| {
                    s.prepared = prepared.clone().map(Box::new);
                    Ok(request::request_told(s, &req)?)
                },
                stopped,
            )??
        };
        shared.watch.note(&v);
        let out = verdict_told(&v, Some(&told));
        if opted(&body)? {
            return Ok(Json(out));
        }
        let mut r = reading(&shared)?;
        let s = r.store()?;
        Ok(Json(without_holder(&s.lib, out)))
    })
}

/// The verdict a request would receive, simulated and never written, with
/// the guards it did not evaluate (`library-api.md` §6, ADR-0054).
async fn check(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answered_off(move || {
        let req = request_of(&body)?;
        if req.propose {
            return Err(Fault::Bad(
                "a check simulates a request, not a proposal".to_string(),
            ));
        }
        // a check decided by a personal member, opted into, as a read by it
        // is (ADR-0206 decision 2)
        {
            let mut r = reading(&shared)?;
            let s = r.store()?;
            let tn = match (&req.type_, &req.object_id) {
                (Some(t), _) => Some(t.clone()),
                (None, Some(id)) => s.snapshot()?.get(id)?.map(|o| o.type_.clone()),
                _ => None,
            };
            if !opted(&body)?
                && tn.is_some_and(|tn| {
                    reads::transition_reads_personal(&s.lib, &tn, &req.transition)
                })
            {
                return Err(personal_refused(
                    "the transition is decided by a personal member, and the request does not set reads_personal",
                ));
            }
        }
        // a check of a change's submit runs its dry run, one at a time as a
        // submit's does, where six at once took 9.3 GB outside every bound
        // (fifth production review DRYRUN-MEMORY, ADR-0190 decision 2)
        // a check of a change's submit runs its dry run as a submit does,
        // its checks with no read open (ADR-0195 decision 1)
        let prepared = match &req.object_id {
            Some(id) if shared.reads.is_some() && changes_the_flow(&shared, &req)? => {
                match dry_running(&shared, id, actor_id(&body)?, &req.actor)? {
                    Some((read, report))
                        if matches!(read.state.as_str(), "DRAFTED" | "SUBMITTED") =>
                    {
                        Some(objectflow_core::publish::Prepared {
                            change: read.change,
                            change_version: read.change_version,
                            at: report.version - 1,
                            report,
                        })
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        // a check of an object whose rules grow with the store waits among
        // the analytics, as a read of it does (thirteenth production review C)
        let _slot = match prepared {
            Some(_) => Slot(None),
            None => object_slot(
                &shared,
                req.object_id.as_deref(),
                req.type_.as_deref(),
                actor_id(&body)?,
                objectflow_core::reads::ObjectRead::Check,
            )?,
        };
        let c = consulting(
            &shared,
            true,
            |s| {
                s.prepared = prepared.clone().map(Box::new);
                Ok(request::check(s, &req)?)
            },
            |out| match out {
                Err(NotRun::Unanswered(c)) => Some((**c).clone()),
                _ => None,
            },
        )??;
        let mut told = verdict_told(&c.verdict, Some(&c.told));
        // a check writes nothing: the positions its simulation took, and a
        // creation's id, the next real request takes for what it writes, so
        // a satisfied check names none and says them not computed, where it
        // named them as its own (the audit's write path A-05)
        if matches!(c.verdict, Verdict::Satisfied { .. }) {
            told["events"] = json!([]);
            let mut not = vec!["events"];
            if req.object_id.is_none() {
                told["object_id"] = J::Null;
                not.push("object_id");
            }
            told["not_computed"] = json!(not);
        }
        let told = if opted(&body)? {
            told
        } else {
            let mut r = reading(&shared)?;
            let s = r.store()?;
            without_holder(&s.lib, told)
        };
        Ok(Json(
            json!({ "verdict": told, "unevaluated": c.unevaluated }),
        ))
    })
    .await
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
        // a cursor of another JSON type said as one, where a number read as
        // a value the store never handed out (the guides' audit G-98)
        Some(other) => Err(Fault::Bad(format!(
            "a cursor is a string, as the store hands it out, and {other} is a {}",
            match other {
                J::Number(_) => "number",
                J::Bool(_) => "boolean",
                J::Array(_) => "list",
                _ => "map",
            }
        ))),
    }
}

/// A page of the events a subscription selects after a cursor, or its
/// acknowledged one, and the settled cursor (`library-api.md` §6).
async fn pull(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answered_off(move || {
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
            "more": page.more,
            "refused": page.refused,
            "not_computed": EVENT_NOT_COMPUTED,
        })))
    })
    .await
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
    answered_off(move || {
        let (Some(_actor), Some(type_)) = (actor_id(&body)?, text(&body, "type")?) else {
            return Err(Fault::Bad(
                "exceptions names its actor and the type".to_string(),
            ));
        };
        let (_slot, mut r) = heavy_reading(&shared, &shared.admission, actor_id(&body)?)?;
        let s = r.store()?;
        let world = s.snapshot()?;
        Ok(Json(page_json(&reads::exceptions_from(
            &world,
            type_,
            text(&body, "cursor")?,
        )?)))
    })
    .await
}

/// The objects of a type a transition is offered to, asking no evaluator
/// (`DESIGN.md` §10, ADR-0048).
async fn available(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answered_off(move || {
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
        if !opted(&body)? && reads::transition_reads_personal(&s.lib, type_, transition) {
            return Err(personal_refused(
                "the transition is decided by a personal member, and the request does not set reads_personal",
            ));
        }
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
    .await
}

/// Where a type's flow and reality disagree, over a window (`DESIGN.md`
/// §10, ADR-0160); a type it does not declare is refused, a value.
async fn diagnostics(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answered_off(move || {
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
    .await
}

/// A maintenance task run outside the retention the store was built with:
/// the rows it moved or removed, or its refusal, a value (`DESIGN.md` §10).
async fn maintain(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answering(move || {
        let task = object(&body, "task")?.map(|_| &body["task"]);
        // the indexes rebuilt, one at a time, the writer taken for each
        // alone, so writes wait at most one index's rebuild, 0.6 s on the
        // six-year store, where the whole took 5.2 s (ADR-0204 decision 9.5)
        if let (Some(actor), Some("rebuild_indexes")) = (
            actor_id(&body)?,
            task.map(|t| text(t, "name")).transpose()?.flatten(),
        ) {
            let (indexes, through) = {
                let mut sandbox = locked(&shared)?;
                match store(&mut sandbox)?.indexes_to_rebuild(actor)? {
                    Ok(due) => due,
                    Err(why) => return Ok(Json(json!({ "moved": 0, "refused": why }))),
                }
            };
            for name in &indexes {
                let mut sandbox = locked(&shared)?;
                store(&mut sandbox)?.rebuild_index(name)?;
            }
            if !indexes.is_empty() {
                let mut sandbox = locked(&shared)?;
                store(&mut sandbox)?.indexes_rebuilt(through)?;
            }
            return Ok(Json(json!({ "moved": indexes.len(), "refused": J::Null })));
        }
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
    answered_off(move || {
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
            "more": page.more,
            "refused": page.refused,
        })))
    })
    .await
}

/// A subscription's progress moved, back as well as forward; a refusal is
/// a value (`library-api.md` §6).
async fn acknowledge(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answering(move || {
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
    // for a proposal's approval, the version of the proposed object its
    // approver read (tenth production review)
    let target_version = match body.get("target_version") {
        None | Some(J::Null) => None,
        Some(v) => Some(v.as_i64().ok_or_else(|| {
            Fault::Bad("target_version takes an object's version, an integer".to_string())
        })?),
    };
    let propose = flag(body, "propose")?;
    if target_version.is_some() && (propose || text(body, "transition")? != Some("approve")) {
        return Err(Fault::Bad(
            "target_version is a proposal's approval's: the version of the proposed object its approver read".to_string(),
        ));
    }
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
    // an object_id names the object, and a type beside it the type it must
    // be of, as a tool bound to its type sends (thirteenth production review
    // B); a type alone creates one
    if object_id.is_none() && type_.is_none() {
        return Err(Fault::Bad(
            "a request names an object_id, a type to create one, or both".to_string(),
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
        target_version,
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

/// A key a part of an import does not take, refused: a misspelt one was
/// read as absent, so an object's `atributes` were dropped without a word
/// (the second production review's minor, found open 2026-10-15).
fn takes_only(v: &J, what: &str, takes: &[&str]) -> Result<(), Fault> {
    match v
        .as_object()
        .and_then(|m| m.keys().find(|k| !takes.contains(&k.as_str())))
    {
        Some(k) => Err(Fault::Bad(format!("{what} takes no key {k}"))),
        None => Ok(()),
    }
}

fn imported_object(o: &J) -> Result<ImportedObject, Fault> {
    takes_only(
        o,
        "an imported object",
        &[
            "type",
            "legacy_key",
            "state",
            "recorded_from",
            "attributes",
            "entered",
            "legacy_entries",
            "legacy_intervals",
            "silent_spans",
            "created_at",
            "created_by_kind",
            "occurred_at",
        ],
    )?;
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
        takes_only(e, &format!("entered.{dimension}"), &["at", "by", "by_kind"])?;
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
        takes_only(
            e,
            "a legacy entry",
            &["kind", "occurred_at", "payload", "about"],
        )?;
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
        takes_only(
            l,
            "a legacy interval",
            &[
                "dimension",
                "value",
                "entered_at",
                "left_at",
                "entered_by",
                "entered_by_kind",
            ],
        )?;
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
    // as the import's report writes a class, which its disposition file
    // sends back
    takes_only(
        c,
        "a class",
        &[
            "id",
            "type",
            "kind",
            "rule",
            "shape",
            "count",
            "sample",
            "disposition",
            "decided_by",
            "reason",
            "uncovers",
        ],
    )?;
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
        // what the report said excluding it uncovers, which deciding it
        // leaves aside (ADR-0212)
        uncovers: Vec::new(),
    })
}

fn report(r: &ImportReport) -> J {
    json!({
        "written": r.written,
        "findings": r.findings.iter().map(|c| json!({
            "id": c.id, "type": c.type_, "kind": c.kind.text(), "rule": c.rule, "shape": c.shape,
            "count": c.count, "sample": c.sample, "disposition": c.disposition.text(),
            "decided_by": c.decided_by, "reason": c.reason,
            "uncovers": c.uncovers.iter().map(|u| json!({
                "id": u.id, "type": u.type_, "rule": u.rule, "count": u.count,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "refused_types": r.refused_types,
        "refusal": r.refusal.as_ref().map(|f| json!({"type": f.type_, "legacy_key": f.legacy_key, "reason": f.reason})),
    })
}

async fn import_op(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answering(move || {
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
        // a misspelt key read as absent raised no sequence, so a go-live's
        // numbers could take a legacy record's (found 2026-10-15)
        takes_only(
            b,
            "a batch",
            &["objects", "legacy_kinds", "sequences", "admitted"],
        )?;
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
            takes_only(r, "a sequence's raise", &["sequence", "last", "scope"])?;
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
    // carries (storage-schema.md §3), read by the object's index alone,
    // where its whole history was read (the audit's cost B11)
    let declaration_version = world.version_of(&o.id)?;
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
    answered_off(move || {
        // the argument it needs, a fault and never an absent object (D570)
        let id =
            text(&body, "id")?.ok_or_else(|| Fault::Bad("get names an object's id".to_string()))?;
        let follow = flag(&body, "follow")?;
        let _slot = object_slot(
            &shared,
            Some(id),
            None,
            actor_id(&body)?,
            objectflow_core::reads::ObjectRead::Get,
        )?;
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
    .await
}

/// A change's dry run, its report the impact a publish would have, or its
/// approval, which installs it (`library-api.md` §6, ADR-0155).
async fn publish(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answering(move || {
        let (Some(actor), Some(change)) = (actor_id(&body)?, text(&body, "change")?) else {
            return Err(Fault::Bad(
                "publish names its actor and the change".to_string(),
            ));
        };
        // an expected_version is a version, read as such even where a
        // preview ignores it, where one of any JSON type was taken
        // (thirteenth production review)
        let expected = int(&body, "expected_version")?;
        if flag(&body, "dry_run")? {
            // a preview is a read: a deployment's runs beside the writes,
            // under the read's deadline, where it held every write (third
            // production review PUBLISH-SCALE); a sandbox's reads take the
            // writer, as every one of its reads does
            if shared.reads.is_some() {
                // its checks with no read open, its impact stopped by an
                // urgent cut (ADR-0195 decision 1)
                return match dry_running(&shared, change, actor_id(&body)?, actor)? {
                    Some((_, report)) => Ok(Json(json!({
                        "verdict": J::Null,
                        "report": objectflow_core::publish::report_json(&report),
                    }))),
                    None => Err(Fault::Missing(format!("no change {change}"))),
                };
            }
            let mut sandbox = locked(&shared)?;
            let s = store(&mut sandbox)?;
            return preview(&s.snapshot()?, actor, change);
        }
        let mut sandbox = locked(&shared)?;
        let s = store(&mut sandbox)?;
        // the version of the change the approver read, required (ADR-0101)
        let Some(expected) = expected else {
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

/// A change's dry run as its three routes run it, a submit's or refresh's,
/// a preview's and a check's: its slot taken; the change read; its checks
/// run with no read of the store open, so an erasure's cut of the
/// write-ahead log never waits for them, where an agent's large change held
/// an erased value in the store's files for the whole of a 38.7 s check;
/// and its impact on a second read, stopped by an urgent cut as every read
/// is (eighth production review ERASE-WAL-DRYRUN, ADR-0195 decision 1).
fn dry_running(
    shared: &Shared,
    change: &str,
    pool_actor: Option<&str>,
    actor: &str,
) -> Result<
    Option<(
        objectflow_core::publish::ChangeRead,
        objectflow_core::publish::PublishReport,
    )>,
    Fault,
> {
    let until = std::time::Instant::now() + shared.dry_run_timeout;
    let _slot = match shared.reads {
        Some(_) => {
            pool_of(shared, &shared.dry_runs, pool_actor)?.take(until, shared.dry_run_timeout)?
        }
        None => Slot(None),
    };
    let late = |e: NotRun| match e {
        NotRun::TimedOut(why) if std::time::Instant::now() < until => Fault::TimedOut(why),
        NotRun::TimedOut(_) => dry_run_late(shared),
        other => Fault::from(other),
    };
    let (read, lib) = {
        let mut r = reading_until(shared, until)?;
        let s = r.store()?;
        let Some(read) =
            objectflow_core::publish::change_read(&s.snapshot()?, change).map_err(late)?
        else {
            return Ok(None);
        };
        (read, s.shared().0)
    };
    let checked =
        objectflow_core::publish::checks_until(&lib, wall_clock(), &read.draft(actor), Some(until))
            .map_err(late)?;
    if std::time::Instant::now() >= until {
        return Err(dry_run_late(shared));
    }
    let mut r = reading_until(shared, until)?;
    let s = r.store()?;
    let report =
        objectflow_core::publish::dry_run_checked(&s.snapshot()?, &read.draft(actor), checked)
            .map_err(late)?;
    Ok(Some((read, report)))
}

/// A change's dry run stopped at the read's deadline, said as such: a
/// submit's and a preview's run on a read beside the writes, and are never
/// computed again under the lock (PUBLISH-SCALE).
fn dry_run_late(shared: &Shared) -> Fault {
    // the store's size or the change's: either may make a dry run long,
    // where it blamed the store when the module was (ninth production
    // review)
    Fault::TimedOut(format!(
        "the change's dry run passed --dry-run-timeout ({} s), waiting for its slot and running, and was stopped; a large store or a large change needs a longer one, and one that waited behind another's may be sent again",
        shared.dry_run_timeout.as_secs()
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
    answered_off(move || {
        let (Some(source), Some(value)) = (text(&body, "source")?, text(&body, "value")?) else {
            return Err(Fault::Bad("lookup names a source and a value".to_string()));
        };
        let mut r = reading(&shared)?;
        let s = r.store()?;
        if !opted(&body)? && reads::lookup_reads_personal(&s.lib, source) {
            return Err(personal_refused(
                "the lookup matches a personal identifier, and its request does not set reads_personal",
            ));
        }
        let world = s.snapshot()?;
        match world.external_id(source, value)?.as_ref() {
            Some(id) => object_json(s, &id.clone()),
            None => Ok(Json(J::Null)),
        }
    })
    .await
}

async fn history(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answered_off(move || {
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
        // in the objects' pool, never behind a worklist (ADR-0195 decision 5)
        let (_slot, mut r) = heavy_reading(&shared, &shared.objects, actor_id(&body)?)?;
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
        // a page ends at its limit or at its bytes, a cursor going on
        // from either (ADR-0194 decision 6)
        let (events, next) = s.history_page(&members, after.unwrap_or(0), limit)?;
        let cursor = next.map(|p| p.to_string());
        Ok(Json(json!({
            "events": entries.into_iter().chain(events.iter().map(event)).collect::<Vec<_>>(),
            "cursor": cursor,
            "not_computed": EVENT_NOT_COMPUTED,
        })))
    })
    .await
}

/// Which of the file hashes given the store still holds, for the
/// application to delete the bytes of the rest (ADR-0211 decision 1): each
/// hash once, well formed, a page's worth at most, as `events` takes its
/// positions; a scan of the store, waiting its turn beside the analytics.
async fn referenced(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answered_off(move || {
        let Some(hashes) = array(&body, "hashes")?.and_then(|hs| {
            hs.iter()
                .map(|h| h.as_str().map(str::to_string))
                .collect::<Option<Vec<String>>>()
        }) else {
            return Err(Fault::Bad(
                "referenced takes hashes, a list of file hashes".to_string(),
            ));
        };
        if hashes.len() > reads::PAGE_LIMIT {
            return Err(Fault::Bad(format!(
                "referenced takes at most {} hashes",
                reads::PAGE_LIMIT
            )));
        }
        if let Some(bad) = hashes
            .iter()
            .find(|h| !objectflow_core::wire::is_file_hash(h))
        {
            return Err(Fault::Bad(format!(
                "a file's hash is sha256:<64 lowercase hex digits>, not {bad}"
            )));
        }
        let mut seen = std::collections::BTreeSet::new();
        if let Some(twice) = hashes.iter().find(|h| !seen.insert(h.as_str())) {
            return Err(Fault::Bad(format!(
                "referenced takes each hash once, not {twice} twice"
            )));
        }
        let Some(actor) = actor_id(&body)? else {
            return Err(Fault::Bad("referenced names its actor".to_string()));
        };
        let (_slot, mut r) = heavy_reading(&shared, &shared.admission, Some(actor))?;
        let s = r.store()?;
        Ok(Json(match s.referenced(actor, &hashes)? {
            Ok(held) => json!({ "referenced": held, "refused": J::Null }),
            Err(why) => json!({ "referenced": [], "refused": why }),
        }))
    })
    .await
}

async fn events(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answered_off(move || {
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
        // waiting its turn: a thousand large positions answered 16.5 MB,
        // eight at once 572 MB (ADR-0192 decision 5); in the objects' pool,
        // never behind a worklist (ADR-0195 decision 5)
        let (_slot, mut r) = heavy_reading(&shared, &shared.objects, actor_id(&body)?)?;
        let s = r.store()?;
        // at most a page's bytes, where a thousand of large inputs answered
        // 108 MB (seventh production review, ADR-0194 decision 6)
        let found = s.events_within(&positions)?.map_err(|fit| {
            Fault::Bad(format!(
                "events' answer would pass {} MiB at its {}th position; ask for fewer",
                objectflow_core::store::EVENT_BYTES / (1024 * 1024),
                fit + 1
            ))
        })?;
        Ok(Json(json!({
            "events": found.iter().map(|e| e.as_ref().map_or(J::Null, event)).collect::<Vec<_>>(),
            "not_computed": EVENT_NOT_COMPUTED,
        })))
    })
    .await
}

/// The tool schemas an agent of a kind is handed, rendered from the
/// installed version's declaration (`renderers.md` §3, ADR-0173 decision 10).
async fn tools(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answered_off(move || {
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
    .await
}

async fn declaration(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answered_off(move || {
        let mut r = reading(&shared)?;
        let s = r.store()?;
        // its type required, as wire.md says, where none was read as a type
        // of no name and answered 404 (the guides' audit G-93)
        let name = text(&body, "type")?
            .ok_or_else(|| Fault::Bad("declaration names a type".to_string()))?;
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
    .await
}

async fn availability(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answered_off(move || {
        let (Some(actor), Some(id)) = (actor_id(&body)?, text(&body, "id")?) else {
            return Err(Fault::Bad(
                "availability names its actor and an object's id".to_string(),
            ));
        };
        let _slot = object_slot(
            &shared,
            Some(id),
            None,
            Some(actor),
            objectflow_core::reads::ObjectRead::Availability,
        )?;
        let mut r = reading(&shared)?;
        let s = r.store()?;
        if !opted(&body)?
            && let Some(o) = s.snapshot()?.get(id)?
            && s.lib.type_(&o.type_).is_some_and(|t| {
                t.transitions
                    .iter()
                    .any(|(xn, _)| reads::transition_reads_personal(&s.lib, &o.type_, xn))
            })
        {
            return Err(personal_refused(
                "an offer is decided by a personal member, and the request does not set reads_personal",
            ));
        }
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
    .await
}

/// A reader's read of a metric: one page holding every group, so its
/// cursor is none (`library-api.md` §5, `MetricPage`).
/// A page of objects of a type or its family, filtered and ordered
/// (`library-api.md` §6, ADR-0127): every object the filter holds for, on
/// one page, so no cursor; a refusal is a value.
async fn query(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answered_off(move || {
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
        // a few keys and fields, each once, where 100k order keys took 5.1 s
        // and 10k fields named again 6.7 s (ninth production review, ADR-0196
        // decision 9)
        for (k, names, most) in [
            ("order", Some(&order), QUERY_ORDER_KEYS),
            ("fields", fields.as_ref(), QUERY_FIELDS),
        ] {
            let Some(names) = names else { continue };
            if names.len() > most {
                return Err(Fault::Bad(format!("{k} names at most {most}")));
            }
            let mut seen = std::collections::BTreeSet::new();
            if let Some(twice) = names.iter().find(|n| !seen.insert(n.as_str())) {
                return Err(Fault::Bad(format!("{k} names {twice} twice")));
            }
        }
        // a query that reads its type whole waits among the analytics, never
        // in a person's worklist slot (ADR-0196 decision 4); and an agent's
        // filtering or ordering by anything personal is refused, where one
        // ranged over people's names to recover what its answers left out
        // (fourteenth production review N4)
        let (scans, personal) = {
            let mut r = reading(&shared)?;
            let s = r.store()?;
            // whoever asks, unless the request opts in, where an agent's
            // alone was refused, by its kind (ADR-0206, superseding ADR-0201
            // decision 4 and ADR-0202 decision 3)
            (
                reads::query_scans(
                    &s.lib,
                    type_,
                    text(&body, "filter")?,
                    &order,
                    fields.as_deref(),
                ),
                !opted(&body)?
                    && reads::query_reads_personal(&s.lib, text(&body, "filter")?, &order),
            )
        };
        if personal {
            return Ok(Json(page_json(&reads::QueryPage {
                items: Vec::new(),
                refused: Some("personal".into()),
                reason: Some(
                    "the query filters or orders by a personal member, and its request does not set reads_personal".into(),
                ),
                cursor: None,
            })));
        }
        let pool = if scans {
            &shared.admission
        } else {
            &shared.worklists
        };
        let (_slot, mut r) = heavy_reading(&shared, pool, actor_id(&body)?)?;
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
    .await
}

async fn metric(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answered_off(move || {
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
        // a dimension kept once, where one named 5,000 times ran to the read's
        // deadline (seventh production review, ADR-0194 decision 6)
        if let Some(keep) = &keep {
            let mut seen = std::collections::BTreeSet::new();
            if let Some(twice) = keep.iter().find(|d| !seen.insert(d.as_str())) {
                return Err(Fault::Bad(format!(
                    "keep names the dimension {twice} twice"
                )));
            }
        }
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
        let page = objectflow_core::metric::page_as(
            &world,
            Some(&s.answers),
            name,
            &bind,
            keep.as_deref(),
            over,
            text(&body, "filter")?,
            opted(&body)?,
        )?;
        let mut out = objectflow_core::metric::page_json(&page);
        if let Some(o) = out.as_object_mut() {
            o.insert("cursor".to_string(), J::Null);
        }
        Ok(Json(out))
    })
    .await
}

/// `GET /health`: the release, the store's version and its clock (ADR-0174
/// decision 6).
async fn health(State(shared): State<Shared>) -> Result<Response, Fault> {
    // a store due an upgrade serves nothing but the upgrade, so its health
    // is 503, with what is due (OBSERVE-2)
    let due = |body: J| Ok((StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response());
    answering(move || {
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
            if let Some(replaced) = store_lost(&shared) {
                let body = json!({"release": BUILTINS, "shape": "deployment",
                                  "code": fault_code(503, &replaced), "fault": replaced});
                return Ok((StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response());
            }
            // the store read, so a store that cannot be read answers 500
            // here rather than at the next request (production review
            // OBSERVE): its file opened afresh, which a read connection,
            // whose pages stay cached while no write moves the log, never
            // rereads, and its release found to be the one served
            match reads.path.as_deref().map_or(
                Ok(Some(BUILTINS.to_string())),
                objectflow_core::release::store_release_at,
            ) {
                Ok(Some(r)) if r == BUILTINS => {}
                // a code an operator acts on, as every one of /health's is,
                // where it answered 500 internal (ninth production review,
                // ADR-0196 decision 9)
                // and latched, as a write's finding is, since `/health` is
                // asked on a listener of its own too (ADR-0205 decision 8)
                Ok(at) => {
                    return Err(Fault::Unavailable(latch(
                        &shared,
                        format!(
                            "store_unreadable: {} holds {}; restore the store with the service stopped (operate.md, Backups and restore)",
                            reads
                                .path
                                .as_deref()
                                .map_or_else(String::new, |p| p.display().to_string()),
                            at.map_or("no store".to_string(), |r| format!("a store at {r}"))
                        ),
                    )));
                }
                Err(e) => {
                    return Err(Fault::Unavailable(latch(
                        &shared,
                        format!(
                            "store_unreadable: {}: {e}; restore the store with the service stopped (operate.md, Backups and restore)",
                            reads
                                .path
                                .as_deref()
                                .map_or_else(String::new, |p| p.display().to_string())
                        ),
                    )));
                }
            }
            // read beside the request, so a stalled store is answered, not
            // waited on (durability on PostgreSQL 3)
            let (version, now) = probed(&shared)?;
            let free = reads.path.as_deref().and_then(disk_free);
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
            } else if let Some(since) = shared
                .watch
                .waiting()
                .filter(|_| shared.writer_since.load(Ordering::SeqCst) != 0)
            {
                // from a write's first retry, where it was said only once
                // a write was refused, 40 s on (ADR-0195 decision 6)
                Some(format!(
                    "writes_stalled: a write has waited {} s for another process's lock on the store",
                    (now - since).max(0) / 1000
                ))
            } else if shared.watch.stalled(now) {
                Some(
                    "writes_stalled: the last write met another process's lock past its retries, and none has committed since"
                        .to_string(),
                )
            } else {
                shared.watch.failing(now).map(|fails| {
                    format!(
                        "writes_failing: the last {fails} writes failed for the service's own fault, answered 500, and none has succeeded since; its log says why"
                    )
                })
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
               "writer_held_seconds": 0.0, "requests_in_flight": shared.in_flight.load(Ordering::SeqCst),
               "connections_open": shared.connections.open.load(Ordering::SeqCst)}),
        )
        .into_response())
    })
}

/// `upgrade` (`storage-schema.md` §12, ADR-0177): a deployment's store
/// Whether an actor names an operator of the store, by its id or its
/// identity, as an upgrade reads it (ADR-0177).
/// Whether an actor is the store's operator, by its object or its identity
/// in any ASCII case, as the core decides it (ADR-0196 decision 8).
fn is_operator(world: &objectflow_core::world::World<'_>, actor: &str) -> Result<bool, NotRun> {
    objectflow_core::store::is_the_operator(world, actor)
}

/// carried from its release to this core's, the operator's act, or with
/// `dry_run` every step run and rolled back; once upgraded the store is
/// opened and served. A store at this release already answers that
/// nothing is due; a sandbox's store is always this release's.
async fn upgrade(State(shared): State<Shared>, Body(body): Body) -> Answer {
    answering(move || {
        let Some(actor) = actor_id(&body)? else {
            return Err(Fault::Bad(
                "an upgrade names its actor, an operator of the store".to_string(),
            ));
        };
        let dry_run = flag(&body, "dry_run")?;
        let mut sandbox = locked(&shared)?;
        // a deployment's store, a file or a PostgreSQL database, upgraded
        // alike (ADR-0177; durability on PostgreSQL 6)
        let postgres = sandbox.postgres.clone().filter(|_| sandbox.deployment);
        let path = sandbox.path.clone().filter(|_| sandbox.deployment);
        if postgres.is_none() && path.is_none() {
            return Err(Fault::Bad(
                "a sandbox's store is this release's; only a deployment's is upgraded".to_string(),
            ));
        }
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
        let (upgraded, at) = match (&postgres, &path) {
            (Some(config), _) => (
                objectflow_core::release::upgrade_postgres(config, wall_clock(), actor, dry_run)?,
                format!("{config:?}"),
            ),
            (None, Some(path)) => (
                objectflow_core::release::upgrade(path, wall_clock(), actor, dry_run)?,
                path.display().to_string(),
            ),
            (None, None) => unreachable!("a deployment's store is a file or a database"),
        };
        let report = match upgraded {
            Ok(r) => r,
            Err(why) => return Err(Fault::Conflict(why)),
        };
        if report.upgraded {
            let mut s = match (&postgres, &path) {
                (Some(config), _) => Store::open_postgres(config, wall_clock(), false)?,
                (None, Some(path)) => Store::open(path, wall_clock())?,
                (None, None) => unreachable!("a deployment's store is a file or a database"),
            };
            (s.attempt_retention, s.idempotency_retention) = sandbox.retentions;
            s.lock_wait = Some(sandbox.lock_wait.clone());
            s.set_busy_timeout(sandbox.busy_timeout)?;
            sandbox.store = Some(s);
            sandbox.due = None;
            note_log(&shared);
        }
        let steps: Vec<J> = report
            .steps
            .iter()
            .map(|(release, changes)| json!({"release": release, "changes": changes}))
            .collect();
        if let (Some(r), false) = (&report.restored, report.dry_run) {
            say!("{}", restore_said(&at, r));
        }
        Ok(Json(
            json!({"from": report.from, "to": report.to, "steps": steps,
                   "dry_run": report.dry_run, "upgraded": report.upgraded,
                   "restored": report.restored.as_ref().map(restore_json)}),
        ))
    })
}

/// A restore as the upgrade answers it: each counter's gap, from the value
/// the copy held to the one it was advanced to (ADR-0207 decision 4).
fn restore_json(r: &objectflow_core::store::Restore) -> J {
    json!({"restored_at": timestamp_text(r.restored_at),
           "copy_last_at": timestamp_text(r.copy_last_at), "margin": r.margin,
           "positions": [r.positions.0, r.positions.1], "txns": [r.txns.0, r.txns.1],
           "objects": [r.objects.0, r.objects.1]})
}

/// A restore said as the start and the upgrade say it, with what an
/// operator checks it against: the highest ids and numbers integrations
/// hold from after the copy (ADR-0207, `operate.md`).
fn restore_said(at: &str, r: &objectflow_core::store::Restore) -> String {
    format!(
        "the store {at} is a restored copy, its last record at {} and its last position {}: every number it hands out was advanced by {}, past what the live store may have handed out since, so positions go on from {}, transactions from {} and object numbers from {}; the gap is recorded in of_restore, and a cursor in it is refused (ADR-0207)",
        timestamp_text(r.copy_last_at),
        r.positions.0,
        r.margin,
        r.positions.1 + 1,
        r.txns.1 + 1,
        r.objects.1 + 1
    )
}

fn app(shared: Shared, deployment: bool) -> Router {
    let routes = Router::new()
        .route("/v1/request", post(request_op))
        .route("/v1/get", post(get))
        .route("/v1/history", post(history))
        .route("/v1/events", post(events))
        .route("/v1/referenced", post(referenced))
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
            // the two GETs said too (the guides' audit G-106)
            Fault::Method("an operation is a POST, and /health and /metrics a GET".to_string())
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
    if let Some(Lost(why)) = answer.extensions().get::<Lost>() {
        latch(&shared, why.clone());
    }
    let took = started.elapsed().as_secs_f64();
    let status = answer.status().as_u16();
    if let Ok(mut served) = shared.served.lock() {
        let row = served.entry((route.clone(), status)).or_insert((0, 0.0));
        row.0 += 1;
        row.1 += took;
    }
    shared.watch.timed(&route, took);
    if WRITE_ROUTES.contains(&route.as_str()) {
        shared.watch.wrote(status, wall_millis());
    }
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
         # TYPE objectflow_writer_held_seconds gauge\nobjectflow_writer_held_seconds {:.3}\n\
         # HELP objectflow_request_groups_total Groups of requests the writer committed together, a request alone a group of one (ADR-0208).\n\
         # TYPE objectflow_request_groups_total counter\nobjectflow_request_groups_total {}\n\
         # HELP objectflow_request_grouped_total The requests those groups held.\n\
         # TYPE objectflow_request_grouped_total counter\nobjectflow_request_grouped_total {}\n",
        shared.in_flight.load(Ordering::SeqCst),
        writer_held(&shared),
        shared.groups.commits.load(Ordering::SeqCst),
        shared.groups.held.load(Ordering::SeqCst)
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
    // by pool, the worklists', the analytics' and the dry runs' (ADR-0190),
    // the agents' (ADR-0192) and the objects' (ADR-0195)
    out.push_str(
        "# HELP objectflow_reads_running Reads that grow with the store running now, by pool.\n\
         # TYPE objectflow_reads_running gauge\n\
         # HELP objectflow_reads_waiting Reads that grow with the store waiting for their turn, by pool.\n\
         # TYPE objectflow_reads_waiting gauge\n\
         # HELP objectflow_reads_most How many reads of a pool run at once.\n\
         # TYPE objectflow_reads_most gauge\n\
         # HELP objectflow_read_wait_seconds How long reads that grow with the store waited for their turn, by pool.\n\
         # TYPE objectflow_read_wait_seconds histogram\n",
    );
    for (pool, a) in [
        ("worklists", &shared.worklists),
        ("analytics", &shared.admission),
        ("dry_runs", &shared.dry_runs),
        ("agents", &shared.agents),
        ("objects", &shared.objects),
    ] {
        out.push_str(&format!(
            "objectflow_reads_running{{pool=\"{pool}\"}} {}\n\
             objectflow_reads_waiting{{pool=\"{pool}\"}} {}\n\
             objectflow_reads_most{{pool=\"{pool}\"}} {}\n",
            a.running(),
            a.waiting.load(Ordering::SeqCst),
            a.most
        ));
        let w = a.waited();
        for (bound, n) in BUCKETS.iter().zip(w.buckets) {
            out.push_str(&format!(
                "objectflow_read_wait_seconds_bucket{{pool=\"{pool}\",le=\"{bound}\"}} {n}\n"
            ));
        }
        out.push_str(&format!(
            "objectflow_read_wait_seconds_bucket{{pool=\"{pool}\",le=\"+Inf\"}} {}\n\
             objectflow_read_wait_seconds_sum{{pool=\"{pool}\"}} {:.6}\n\
             objectflow_read_wait_seconds_count{{pool=\"{pool}\"}} {}\n",
            w.count, w.sum, w.count
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
                          [--store <file> | --store <postgresql://…> [--store-password-file <file>]
                                          [--create]
                                          [--attempt-retention <duration>]
                                          [--idempotency-retention <duration>]
                                          [--evaluator <name>=<url> ...]
                                          [--evaluator-timeout <duration>]
                                          [--evaluator-token <name>=<file> ...]
                                          [--busy-timeout <duration>]
                                          [--read-timeout <duration>]
                                          [--dry-run-timeout <duration>]
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
wall clock; and no controls (ADR-0174, ADR-0183). --store may name a
PostgreSQL database instead, a postgresql:// URL or a connection string, the
store kept there and made in it by --create where it holds none; its
password is read from OBJECTFLOW_STORE_PASSWORD or --store-password-file and
refused on the command line, its connection uses TLS where its sslmode asks,
and its database must compare text by bytes and fold ASCII letters alone, made
with LOCALE_PROVIDER libc LC_COLLATE 'C' LC_CTYPE 'C' (ADR-0213). Each --evaluator names
the endpoint a namespace's calls are posted to, answered within the timeout,
5 s unless given, and an evaluator no endpoint answers is unknown
(ADR-0175), sent the token each --evaluator-token's file holds (ADR-0188).
Its reads run beside its writes, each on a connection of its own, stopped
at the read timeout, 30 s unless given, those that grow with the store at
most n at once, 4 unless given, a read waiting for its turn within its
timeout (ADR-0189); a change's dry run, one at a time, stopped at the
dry-run timeout, 2 min unless given (ADR-0194); and its writes one at a time, each waiting out another process's
write lock for the busy timeout, 5 s unless given, before it is retried
and then refused stale (ADR-0178). It authenticates nothing: give it a --socket its application's
user alone reaches, since any user of the machine reaches a loopback port, and never bind it beyond
loopback (core/README.md, SECURITY.md).";

/// The command line: where to listen, and a deployment's store and
/// retentions where it names one.
struct Args {
    bind: String,
    /// A Unix socket listened on in the address's place (ADR-0183
    /// decision 4).
    socket: Option<std::path::PathBuf>,
    store: Option<std::path::PathBuf>,
    /// A deployment's store on PostgreSQL, the connection string or URL
    /// `--store` names in a file's place, and the file its password is read
    /// from, which is never given on the command line (ADR-0213).
    postgres: Option<String>,
    password_file: Option<std::path::PathBuf>,
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
    dry_run_timeout: Option<std::time::Duration>,
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

impl Args {
    /// Whether the store is a deployment's, a file's or a PostgreSQL
    /// database's, where none is a sandbox's.
    fn deployed(&self) -> bool {
        self.store.is_some() || self.postgres.is_some()
    }
}

/// The reads that grow with the store run at once unless `--max-reads`
/// says otherwise (ADR-0189 decision 2).
const MAX_READS: usize = 4;

/// The most order keys and fields a query names (ADR-0196 decision 9).
const QUERY_ORDER_KEYS: usize = 8;
const QUERY_FIELDS: usize = 256;

fn args() -> Result<Args, String> {
    let mut out = Args {
        bind: "127.0.0.1:8712".to_string(),
        socket: None,
        store: None,
        postgres: None,
        password_file: None,
        create: false,
        retentions: (None, None),
        endpoints: std::collections::BTreeMap::new(),
        tokens: std::collections::BTreeMap::new(),
        evaluator_timeout: None,
        busy_timeout: None,
        read_timeout: None,
        dry_run_timeout: None,
        idle_timeout: std::time::Duration::from_secs(30),
        max_connections: 512,
        max_reads: None,
        metrics_bind: None,
    };
    let mut retained = false;
    let mut addressed = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        // an option where a value belongs, `--store --create` for a path
        // forgotten, refused naming both, where it was taken as the value
        let mut value = || match args.next() {
            Some(v) if v.starts_with("--") => {
                Err(format!("{a} takes a value, and is given the option {v}"))
            }
            Some(v) => Ok(v),
            None => Err(format!("{a} takes a value")),
        };
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
            "--store" => {
                let v = value()?;
                if objectflow_core::store::names_postgres(&v) {
                    out.postgres = Some(v);
                } else {
                    out.store = Some(v.into());
                }
            }
            "--store-password-file" => out.password_file = Some(value()?.into()),
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
                out.max_connections = n
                    .parse::<usize>()
                    .ok()
                    .filter(|n| (1..=MAX_CONNECTIONS).contains(n))
                    .ok_or_else(|| {
                        format!("--max-connections takes a count from 1 to {MAX_CONNECTIONS}: {n}")
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
            "--dry-run-timeout" => {
                let d = value()?;
                let secs = parse_duration(&d).filter(|s| *s > 0).ok_or_else(|| {
                    format!("--dry-run-timeout takes a duration such as '2 min': {d}")
                })?;
                out.dry_run_timeout = Some(std::time::Duration::from_secs(secs.unsigned_abs()));
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
    if out.dry_run_timeout.is_some() && !out.deployed() {
        return Err(
            "a dry run's timeout is a deployment's; a sandbox's dry runs take the writer's lock"
                .to_string(),
        );
    }
    if out.read_timeout.is_some() && !out.deployed() {
        return Err(
            "a read's timeout is a deployment's; a sandbox's reads take the writer's lock"
                .to_string(),
        );
    }
    if out.busy_timeout.is_some() && !out.deployed() {
        return Err(
            "how long a write waits out another process's lock is a deployment's; a sandbox's store is its own"
                .to_string(),
        );
    }
    if out.max_reads.is_some() && !out.deployed() {
        return Err(
            "how many reads run at once is a deployment's; a sandbox's reads take the writer's lock"
                .to_string(),
        );
    }
    if out.password_file.is_some() && out.postgres.is_none() {
        return Err(
            "--store-password-file is a PostgreSQL store's; a file's store has no password"
                .to_string(),
        );
    }
    if out.create && !out.deployed() {
        return Err("--create makes a deployment's store, which --store names".to_string());
    }
    // a socket path no socket can have is refused before anything is made,
    // where --create made the store first (fourteenth production review)
    if let Some(said) = out.socket.as_deref().and_then(socket_too_long) {
        return Err(said);
    }
    if retained && !out.deployed() {
        return Err("a retention is a deployment's; a sandbox's load names its own".to_string());
    }
    if (!out.endpoints.is_empty() || out.evaluator_timeout.is_some()) && !out.deployed() {
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
    // a header's value holds no control character, where a token holding a
    // line break was taken and every call then failed (ninth production
    // review, ADR-0196 decision 9)
    if token.chars().any(char::is_control) {
        return Err(format!(
            "{} holds a token with a control character, such as a line break inside it, which no header can carry",
            file.display()
        ));
    }
    Ok(token)
}

/// The longest path a Unix socket may have on Linux, its `sun_path` less
/// the closing NUL.
const SOCKET_PATH: usize = 107;

/// The stack each thread of the service runs a request on: an expression
/// nests at most `expr::MAX_NESTING` levels and its tree is at most
/// `expr::MAX_DEPTH` deep, and evaluating one level of a tree took about
/// 8 KiB of stack in a release build, measured 2026-10-11; 16 MiB holds
/// those many times over, where the runtime's default 2 MiB did not hold a
/// 256-level tree (F1).
const WORKER_STACK: usize = 16 * 1024 * 1024;

/// The most connections a service may serve at once, each of whose
/// requests may hold a thread of its own while it runs.
const MAX_CONNECTIONS: usize = 4_096;

fn main() {
    // a thread for every request the connection cap lets run, and room
    // beside them, so /health and /metrics always have one: at tokio's 512,
    // 700 requests waiting on an evaluator held /health on --metrics-bind
    // 17.9 s (thirteenth production review N4)
    let ran = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(MAX_CONNECTIONS + 64)
        .thread_stack_size(WORKER_STACK)
        .enable_all()
        .build()
        .and_then(|rt| rt.block_on(serve()));
    // a start or a stop that fails says why in a sentence, never in Rust's
    // debug form (second production review, minor)
    if let Err(e) = ran {
        say!("objectflow-service: {e}");
        log_drained(LOG_DRAIN);
        std::process::exit(1);
    }
    // the log's last lines written before the process ends
    log_drained(LOG_DRAIN);
}

/// How long the service waits, as it ends, for the log's queued lines.
const LOG_DRAIN: std::time::Duration = std::time::Duration::from_secs(2);

/// A store refused at its start, its fault said once, and where a `-wal`
/// or `-shm` lies beside it what a copy restored beside another state's
/// needs: SQLite's own words, "malformed database schema", read as a broken
/// backup, where the copy was whole (seventh production review
/// MSG-RESTORE, ADR-0194 decision 6).
fn refused_start(path: &std::path::Path, fault: &dyn std::fmt::Display) -> std::io::Error {
    // where a log beside it holds frames, as the start's note judges one,
    // and said as what a restore may have done, where every refused start
    // after any earlier run told an operator they had restored (the audit's
    // durability C5)
    let beside = std::fs::metadata(wal_path(path)).is_ok_and(|m| m.len() > 0);
    std::io::Error::other(if beside {
        format!(
            "{}: {fault}; if you restored a copy, a -wal of another state beside it does this: nothing was written, so restore the copy alone, with neither -wal nor -shm beside it (operate.md, Backups and restore)",
            path.display()
        )
    } else {
        format!("{}: {fault}", path.display())
    })
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
/// (ADR-0183 decision 9). SIGHUP, which a log rotation's `kill -HUP` sends,
/// is said and ignored, where it ended the service at once with nothing in
/// flight answered, an end systemd's `Restart=on-failure` counts clean and
/// does not restart (eleventh production review): the log goes to standard
/// error, and there is nothing to reopen.
fn stopping() -> std::io::Result<impl std::future::Future<Output = ()>> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    let mut hup = signal(SignalKind::hangup())?;
    Ok(async move {
        loop {
            tokio::select! {
                _ = term.recv() => break,
                _ = int.recv() => break,
                _ = hup.recv() => say!(
                    "SIGHUP ignored: the log goes to standard error, and there is nothing to reopen; SIGTERM stops the service"
                ),
            }
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
            log_drained(LOG_DRAIN);
            std::process::exit(1);
        });
    })
}

/// How often the write-ahead log is looked at, and cut back where due.
const CUT_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// How long a change's dry run may run unless `--dry-run-timeout` says
/// otherwise (ADR-0194 decision 2).
const DRY_RUN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

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

/// On PostgreSQL, an erasure's vacuum still due tried again every
/// `CUT_EVERY`, where it waited for the next write and an idle store kept
/// the erased rows' old versions (personal data on PostgreSQL 9).
async fn keep_erasures_vacuumed(shared: Shared) {
    let mut every = tokio::time::interval(CUT_EVERY);
    loop {
        every.tick().await;
        let s = shared.clone();
        let _ = tokio::task::spawn_blocking(move || vacuum_back(&s)).await;
    }
}

/// An erasure's vacuum, due, tried once with the writer where it is free,
/// since a write runs it itself; said once it has waited a minute, with
/// why, and once it is done, where a vacuum that stayed due was never said
/// (personal data on PostgreSQL 5).
fn vacuum_back(shared: &Service) {
    let Ok(mut w) = shared.writer.try_lock() else {
        return;
    };
    let Some(s) = w.store.as_mut() else {
        return;
    };
    if !s.checkpoint_due() {
        return;
    }
    let tried = s.after_erasure(false);
    let now = wall_millis();
    let since = &shared.watch.vacuum_due_since;
    if let Ok(true) = tried {
        if since.swap(0, Ordering::SeqCst) != 0
            && shared.watch.vacuum_told.swap(false, Ordering::SeqCst)
        {
            say!(
                "an erasure's vacuum is done: no old version of an erased value is left for a query"
            );
            note_log(shared);
        }
        return;
    }
    let first = match since.compare_exchange(0, now, Ordering::SeqCst, Ordering::SeqCst) {
        Ok(_) => now,
        Err(first) => first,
    };
    if now - first >= 60_000 && !shared.watch.vacuum_told.swap(true, Ordering::SeqCst) {
        let why = match &tried {
            Err(e) => format!("it could not run: {e}"),
            Ok(_) => "a snapshot older than the erasure still holds the erased rows' old versions: a read, a standby's feedback, a replication slot or a prepared transaction".to_string(),
        };
        say!(
            "an erasure's vacuum has been due {} s, tried every {} s: {why}",
            (now - first) / 1000,
            CUT_EVERY.as_secs()
        );
        note_log(shared);
    }
}

/// How often a write's wait for another process's lock is looked for.
const STALL_LOOK: std::time::Duration = std::time::Duration::from_secs(1);

/// A wait for another process's lock looked for on a tick of its own, which
/// reads only what the store tells and never the writer: on the log's cut's
/// tick, which takes the writer, it was said only once the stalled write
/// gave up, and never in a stall of 30 s (ninth production review).
async fn watch_for_stalls(shared: Shared) {
    let mut every = tokio::time::interval(STALL_LOOK);
    loop {
        every.tick().await;
        tell_a_stall(&shared);
        tell_low_disk(
            &shared.watch,
            shared.store_path.as_deref().and_then(disk_free),
        );
    }
}

/// Free space on the store's volume below what a checkpoint needs, said in
/// the log once as it falls below and once as it recovers, where only
/// `/health` and `/metrics` said it (ADR-0200 decision 7). Whether a line
/// was said.
fn tell_low_disk(watch: &Watch, free: Option<u64>) -> bool {
    let Some(free) = free else {
        return false;
    };
    let low = disk_low(free);
    if watch.disk_low_told.swap(low, Ordering::SeqCst) == low {
        return false;
    }
    if low {
        say!(
            "disk_low: {free} byte(s) free on the store's volume, below the {DISK_FLOOR} a checkpoint needs; /health answers 503 until space is freed (operate.md)"
        );
    } else {
        say!(
            "disk space recovered: {free} byte(s) free on the store's volume, at least the {DISK_FLOOR} a checkpoint needs"
        );
    }
    true
}

/// A write waiting for another process's lock on the store, said once in
/// the log as it waits, where nothing named a stall (eighth production
/// review, ADR-0195 decision 6). Whether it was said.
fn tell_a_stall(shared: &Service) -> bool {
    let Some(since) = shared.watch.waiting() else {
        return false;
    };
    if shared.watch.lock_wait_told.swap(since, Ordering::SeqCst) == since {
        return false;
    }
    say!(
        "writes are waiting for another process's lock on the store, {} s so far; each waits --busy-timeout up to four times, then is refused stale after up to four more to record its refusal (writes_stalled)",
        (wall_millis() - since).max(0) / 1000
    );
    true
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
    // a lost store's log is carried nowhere (ADR-0205 decision 8); the
    // latch honoured and never set here, since this tick runs beside an
    // upgrade, whose connection may remove the log a moment before the
    // writer opens it again, and requests find a store lost once they hold
    // it
    if shared.watch.latched.get().is_some() {
        return false;
    }
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
        &shared.objects,
    ];
    // every pool held a moment, one that does not drain in it holding the
    // rest too
    // every pool barred before any is waited for, so no read begins in one
    // while the cut waits on another, to be found running and stopped, as a
    // worklist begun while a report held the cut was (ADR-0205 decision 14)
    let moment = std::time::Instant::now() + CUT_HOLD;
    for p in &pools {
        p.bar();
    }
    // each pool waited on, none skipped once one has not drained, as a
    // short-circuiting `all` would skip it
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
        // the reads still running stopped at their next step, and waited for
        // within the read deadline: those of the pools that did not drain in
        // the moment, since a pool that drained has none running (ADR-0205
        // decision 14)
        for p in &pools {
            p.stop_readers();
        }
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

/// A connection whose writes its client does not take within the idle
/// timeout fails them, so the connection closes: an answer no one read was
/// held without bound, eight of them 750 MB for 40 s and more (seventh
/// production review, ADR-0194 decision 6). A write that moves at all, a
/// slow client's, starts the timeout again.
struct Unread<T> {
    io: T,
    idle: std::time::Duration,
    stalled: Option<std::pin::Pin<Box<tokio::time::Sleep>>>,
}

impl<T> Unread<T> {
    fn new(io: T, idle: std::time::Duration) -> Self {
        Unread {
            io,
            idle,
            stalled: None,
        }
    }

    /// A write its client is not taking: waited for within the idle
    /// timeout, failed past it.
    fn waiting<R>(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<R>> {
        let idle = self.idle;
        let sleep = self
            .stalled
            .get_or_insert_with(|| Box::pin(tokio::time::sleep(idle)));
        match std::future::Future::poll(sleep.as_mut(), cx) {
            std::task::Poll::Ready(()) => std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "the client did not read its answer within the idle timeout",
            ))),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

impl<T: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for Unread<T> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().io).poll_read(cx, buf)
    }
}

impl<T: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for Unread<T> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        match std::pin::Pin::new(&mut this.io).poll_write(cx, buf) {
            std::task::Poll::Pending => this.waiting(cx),
            done => {
                this.stalled = None;
                done
            }
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();
        match std::pin::Pin::new(&mut this.io).poll_flush(cx) {
            std::task::Poll::Pending => this.waiting(cx),
            done => {
                this.stalled = None;
                done
            }
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().io).poll_shutdown(cx)
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

/// A connection's place among the open ones, let go when its task ends,
/// however it ends.
struct Leaving {
    open: Arc<Mutex<Opened>>,
    id: u64,
    shared: Shared,
}

/// The connections open, by the order they were taken: what each is
/// answering, and how it is told to close.
type Opened = std::collections::BTreeMap<u64, (Busy, Arc<tokio::sync::Notify>)>;

impl Drop for Leaving {
    fn drop(&mut self) {
        self.open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.id);
        self.shared.connections.open.fetch_sub(1, Ordering::SeqCst);
    }
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
            hyper_util::rt::TokioIo::new(Unread::new(io, idle)),
            OnConnection {
                router: hyper_util::service::TowerToHyperService::new(router.clone()),
                busy,
            },
        );
        let (mut closed, open, shared) = (closed.clone(), open.clone(), shared.clone());
        tasks.spawn(async move {
            // its place among the open ones let go however the task ends, a
            // panic's unwinding included (tenth production review)
            let _leaving = Leaving {
                open: open.clone(),
                id,
                shared: shared.clone(),
            };
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
/// What a path a socket cannot have is refused with, its limit said, where
/// the system's words named SUN_LEN (twelfth production review).
fn socket_too_long(path: &std::path::Path) -> Option<String> {
    let n = path.as_os_str().len();
    (n > SOCKET_PATH).then(|| {
        format!(
            "{} is {n} bytes long, past the {SOCKET_PATH} a Unix socket's path may hold; give --socket a shorter path, such as /run/objectflow/objectflow.sock",
            path.display()
        )
    })
}

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
    if let Some(said) = socket_too_long(path) {
        return Err(std::io::Error::other(said));
    }
    let named = |e: std::io::Error| std::io::Error::other(format!("{}: {e}", path.display()));
    let listener = tokio::net::UnixListener::bind(path).map_err(named)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660)).map_err(named)?;
    Ok(listener)
}

/// Whether a file that holds something is a store: a SQLite database with
/// the store's declarations' table.
fn is_store(path: &std::path::Path) -> std::io::Result<bool> {
    // with the advice a restore beside another state's log needs, where a
    // backup restored beside a later -wal was refused with SQLite's words
    // alone (eighth production review, ADR-0195 decision 7)
    objectflow_core::release::holds_store(path).map_err(|e| refused_start(path, &e))
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
    let sized = std::fs::metadata(path)
        .map(|m| m.len() > 0)
        .unwrap_or(false);
    // a database holding not a table is no store: what a crash while
    // --create made one leaves, which a start with --create makes, where
    // every start refused it as no ObjectFlow store (the audit's durability
    // W11)
    let empty = sized && objectflow_core::release::holds_nothing(path).unwrap_or(false);
    let held = sized && !empty;
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
            "the store's log, {}, holds {} byte(s) its file does not: the last stop was not clean, or another process has the store open; the store takes them in as it opens, which after a crash keeps every acknowledged write, so leave it; but where you had just put a backup in the store's place, this start has taken the later log into it: stop the service, remove the -wal and -shm, put the backup in the store's place again, and start (operate.md, Backups and restore)",
            std::path::Path::new(&wal).display(),
            m.len()
        ));
    }
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    // a directory that does not exist is said so, where the start told it
    // to use --create, which then failed (ninth production review, ADR-0196
    // decision 9)
    if !dir.is_dir() {
        return Err(std::io::Error::other(format!(
            "the directory {} does not exist; make it, owned by the service's user, then start once with --create",
            dir.display()
        )));
    }
    if !held {
        if !create {
            return Err(std::io::Error::other(if empty {
                format!(
                    "{} holds an empty database and no store, as a start with --create stopped before it made one leaves; to make one there, start once with --create",
                    path.display()
                )
            } else {
                format!(
                    "{} holds no store; to make one there, start once with --create",
                    path.display()
                )
            }));
        }
        // a directory that does not exist, among the faults, names the path
        // it was asked for (third production review)
        // with what to do, where it gave the system's words alone (tenth
        // production review)
        let named = |e: std::io::Error| {
            std::io::Error::other(format!(
                "{}: {e}; give the service's user the directory {} and its write permission",
                path.display(),
                dir.display()
            ))
        };
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
    // and a directory it cannot write in, where SQLite makes the store's
    // -wal and -shm, said so, where it gave SQLite's "attempt to write a
    // readonly database" (ninth production review, ADR-0196 decision 9)
    let probe = dir.join(format!(".objectflow-start-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
        }
        Err(e) => {
            return Err(std::io::Error::other(format!(
                "the service cannot write in {}, where the store's -wal and -shm are made: {e}; give the service's user the directory and its write permission",
                dir.display()
            )));
        }
    }
    // a directory is no store, where it was told to get write permission
    // (eleventh production review)
    if path.is_dir() {
        return Err(std::io::Error::other(format!(
            "{} is a directory; --store names the store's file in it, such as {}",
            path.display(),
            path.join("store.sqlite").display()
        )));
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
    // review, minor); and a store read through another state's -wal may
    // read as no database at all, which is said with the restore's advice,
    // as every check that finds that is (eleventh production review)
    if !is_store(path)? {
        return Err(refused_start(
            path,
            &"it holds a file that is not an ObjectFlow store, which the service does not make one",
        ));
    }
    let mode = std::fs::metadata(path)
        .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        notes.push(format!(
            "the store {} is readable by others than its owner (mode {:o}); its personal data is too, and its -wal and -shm, which are made with the store's mode: make all three 0600",
            path.display(),
            mode & 0o777
        ));
    }
    // a build whose SQLite shares one page cache among its connections makes
    // heavy reads at once each cost what all of them do (ADR-0201)
    if !matches!(objectflow_core::store::page_caches_shared(), Ok(false)) {
        notes.push(
            "this build's SQLite shares one page cache among every connection, so heavy reads at once each take as long as all of them: build from core/, where .cargo/config.toml builds it with a cache for each".to_string(),
        );
    }
    Ok(notes)
}

/// A deployment's store on PostgreSQL (ADR-0213): its password read from
/// `OBJECTFLOW_STORE_PASSWORD` or the file given, never the command line,
/// where a URL holding one is refused; made in the database by `--create`
/// where it holds none, and refused without it.
fn postgres_config(
    url: &str,
    password_file: Option<&std::path::Path>,
) -> std::io::Result<objectflow_core::db::PgConfig> {
    if objectflow_core::db::PgConfig::has_password(url) {
        return Err(std::io::Error::other(
            "--store gives the database's password on the command line, which every user of the machine can read: give it in OBJECTFLOW_STORE_PASSWORD or --store-password-file",
        ));
    }
    let password = match password_file {
        Some(file) => Some(
            std::fs::read_to_string(file)
                .map_err(|e| std::io::Error::other(format!("{}: {e}", file.display())))?
                .trim_end_matches(['\n', '\r'])
                .to_string(),
        ),
        None => std::env::var("OBJECTFLOW_STORE_PASSWORD").ok(),
    };
    objectflow_core::db::PgConfig::parse(url, password.as_deref())
        .map_err(|e| std::io::Error::other(format!("--store: {e}")))
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
            log_drained(LOG_DRAIN);
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
    // a stop asked for once it listens is a graceful one
    let stop = stopping()?;
    // both addresses bound before the store is touched, so a start refused
    // for a busy one changes no file, where it had already turned a restored
    // copy to WAL, made its indexes and folded its log (the audit's
    // durability C3); where it listens said once the store is open, so the
    // line still means it serves
    let mut listening_said = Vec::new();
    let listening = match &a.socket {
        Some(path) => {
            let l = bind_socket(path)?;
            listening_said.push(format!("listening on unix:{}", path.display()));
            Listening::Unix(l)
        }
        None => {
            let l = tokio::net::TcpListener::bind(&a.bind)
                .await
                .map_err(|e| std::io::Error::other(format!("{}: {e}", a.bind)))?;
            let at = l.local_addr()?;
            listening_said.push(format!("listening on http://{at}"));
            // the service authenticates nobody, so an address beyond
            // loopback is said, where it started without a word (fourth
            // production review)
            if !at.ip().is_loopback() {
                listening_said.push(format!(
                    "listening beyond loopback, on {at}: the service authenticates nobody, so whoever reaches this address may act as any actor; bind 127.0.0.1 or give --socket, behind the application that authenticates (core/README.md)"
                ));
            }
            Listening::Tcp(l)
        }
    };
    let metrics_listener = match &a.metrics_bind {
        Some(bind) => Some(
            tokio::net::TcpListener::bind(bind)
                .await
                .map_err(|e| std::io::Error::other(format!("--metrics-bind {bind}: {e}")))?,
        ),
        None => None,
    };
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
            .map_err(|e| refused_start(path, &e))?;
        let busy = a
            .busy_timeout
            .unwrap_or(objectflow_core::store::BUSY_TIMEOUT);
        sandbox.deployment = true;
        sandbox.path = Some(path.clone());
        sandbox.retentions = a.retentions;
        // the file and its log checked before anything opens them to write,
        // so a refused start leaves both as they were, where it wrote another
        // state's log into a restored copy (fourth production review A-16);
        // a store due an upgrade too, which was upgraded, and so written,
        // unchecked (the audit's durability C4)
        if at.is_some() {
            let checked = Store::checked_file(path).map_err(|e| refused_start(path, &e))?;
            if let Some(fault) = checked {
                return Err(refused_start(
                    path,
                    &format!("it fails SQLite's quick check ({fault})"),
                ));
            }
        }
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
                // its opening's fault said as a restore beside another
                // state's log needs, where it gave SQLite's words alone
                // (eighth production review, ADR-0195 decision 7)
                let mut s = Store::open(path, wall_clock()).map_err(|e| refused_start(path, &e))?;
                // a copy restored at its opening, said (ADR-0207)
                if let Some(r) = &s.restored {
                    noted.push(restore_said(&path.display().to_string(), r));
                }
                (s.attempt_retention, s.idempotency_retention) = a.retentions;
                s.lock_wait = Some(sandbox.lock_wait.clone());
                s.set_busy_timeout(busy)
                    .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))?;
                // actors an earlier release took whose identities are equal
                // but for case, which this one takes as one, said with the
                // remedy, by their objects and never their identities
                // (eighth production review CASE-PAIR, ADR-0195 decision 2)
                // refused attempts kept for ever, what their requests sent
                // with them, where no retention prunes them, said (ninth
                // production review N-12, ADR-0196 decision 9)
                if a.retentions.0.is_none() {
                    noted.push(
                        "no --attempt-retention is given, so refused attempts are kept for ever with what their requests sent; give one, as operate.md's unit does".to_string(),
                    );
                }
                let pairs = s
                    .identity_pairs()
                    .map_err(|e| std::io::Error::other(format!("{}: {e}", path.display())))?;
                if !pairs.is_empty() {
                    let shown: Vec<String> =
                        pairs.iter().take(10).map(|ids| ids.join(" and ")).collect();
                    noted.push(format!(
                        "the store holds {} group(s) of actors whose identities are equal but for case, which this release takes as one actor, a request naming one resolved to the first made: {}{}; publish a version whose migration admits <Type>.identity_taken, then give each of them an identity of its own (operate.md, Upgrading the engine)",
                        pairs.len(),
                        shown.join("; "),
                        if pairs.len() > 10 { "; and more" } else { "" }
                    ));
                }
                sandbox.store = Some(s);
            }
        }
        sandbox.busy_timeout = busy;
        sandbox.endpoints = a.endpoints.clone();
        sandbox.evaluator_timeout = a
            .evaluator_timeout
            .unwrap_or(std::time::Duration::from_secs(5));
    }
    if let Some(url) = &a.postgres {
        let config = postgres_config(url, a.password_file.as_deref())?;
        // a store at an earlier release served its upgrade alone, as a
        // file's is, where it was refused at the start and stranded
        // (durability on PostgreSQL 6)
        let at = objectflow_core::release::store_release_postgres(&config)
            .map_err(|e| std::io::Error::other(format!("{config:?}: {e}")))?;
        let busy = a
            .busy_timeout
            .unwrap_or(objectflow_core::store::BUSY_TIMEOUT);
        if let Some(r) = at.filter(|r| r != BUILTINS) {
            if objectflow_core::release::steps_from(&r, objectflow_core::release::RELEASES)
                .is_none()
            {
                return Err(std::io::Error::other(format!(
                    "{config:?} is at {r}, which {BUILTINS} does not upgrade from"
                )));
            }
            noted.push(format!(
                "the store {config:?} is at {r}, and this core is {BUILTINS}: every operation is refused until POST /v1/upgrade runs (ADR-0177)"
            ));
            sandbox.due = Some(r);
            sandbox.deployment = true;
            sandbox.postgres = Some(config);
            sandbox.retentions = a.retentions;
            sandbox.busy_timeout = busy;
            sandbox.endpoints = a.endpoints.clone();
            sandbox.evaluator_timeout = a
                .evaluator_timeout
                .unwrap_or(std::time::Duration::from_secs(5));
        } else {
            let mut s = Store::open_postgres(&config, wall_clock(), a.create)
                .map_err(|e| std::io::Error::other(format!("{config:?}: {e}")))?;
            // a copy restored at its opening, said, as a file's is, where the
            // start said nothing of it (durability on PostgreSQL 2)
            if let Some(r) = &s.restored {
                noted.push(restore_said(&format!("{config:?}"), r));
            }
            (s.attempt_retention, s.idempotency_retention) = a.retentions;
            s.lock_wait = Some(sandbox.lock_wait.clone());
            s.set_busy_timeout(busy)
                .map_err(|e| std::io::Error::other(format!("{config:?}: {e}")))?;
            if a.retentions.0.is_none() {
                noted.push(
                "no --attempt-retention is given, so refused attempts are kept for ever with what their requests sent; give one, as operate.md's unit does".to_string(),
            );
            }
            noted.push(format!(
                "the store {config:?} at version {}, under {BUILTINS}",
                s.lib.version
            ));
            if config.plain_to_another_host() {
                noted.push(
                "the store's connection may run without TLS, its sslmode short of require: give sslmode=require, whose certificate and host are always verified (operate.md, A store on PostgreSQL)".to_string(),
            );
            }
            sandbox.deployment = true;
            sandbox.postgres = Some(config);
            sandbox.retentions = a.retentions;
            sandbox.store = Some(s);
            sandbox.busy_timeout = busy;
            sandbox.endpoints = a.endpoints.clone();
            sandbox.evaluator_timeout = a
                .evaluator_timeout
                .unwrap_or(std::time::Duration::from_secs(5));
        }
    }
    let deployment = sandbox.deployment;
    for line in listening_said {
        say!("{line}");
    }
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
    let read_busy = a
        .busy_timeout
        .unwrap_or(objectflow_core::store::BUSY_TIMEOUT);
    let pool = match (&a.store, &sandbox.postgres) {
        (Some(path), _) => Some(Readers::new(path, read_busy)),
        (None, Some(config)) => Some(Readers::postgres(config, read_busy)),
        (None, None) => None,
    };
    let reads = pool.map(|pool| Reads {
        path: a.store.clone(),
        pool,
        seen: std::sync::RwLock::new(Seen::of(&sandbox)),
        floor: AtomicI64::new(sandbox.store.as_ref().map_or(0, |s| s.now)),
    });
    // the store's lock waits, read by /health and the log beside the writer
    // (ADR-0195 decision 6)
    let lock_wait = sandbox.lock_wait.clone();
    // the database a store on PostgreSQL is in at the start, which a lost
    // connection opened again is compared with
    let database = std::sync::OnceLock::new();
    if let Some(at) = sandbox.store.as_ref().and_then(Store::database_origin) {
        let _ = database.set(at);
    }
    let service = Service {
        endpoints: sandbox.endpoints.clone(),
        tokens: a.tokens.clone(),
        evaluator_timeout: sandbox.evaluator_timeout,
        read_timeout: a.read_timeout.unwrap_or(std::time::Duration::from_secs(30)),
        dry_run_timeout: a.dry_run_timeout.unwrap_or(DRY_RUN_TIMEOUT),
        admission: Admission::new(a.max_reads.unwrap_or(MAX_READS)),
        worklists: Admission::new(a.max_reads.unwrap_or(MAX_READS)),
        objects: Admission::new(a.max_reads.unwrap_or(MAX_READS)),
        dry_runs: Admission::for_dry_runs(),
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
        watch: Watch {
            lock_wait,
            wal_identity: Mutex::new(a.store.as_deref().and_then(|p| file_identity(&wal_path(p)))),
            database,
            ..Watch::default()
        },
        groups: Groups::default(),
    };
    let shared = Arc::new(service);
    let router = app(shared.clone(), deployment);
    let _ = BODY_READ.set(a.idle_timeout);
    // `/metrics` and `/health` alone on an address of their own, where a
    // service on a socket could not be scraped (fourth production review)
    if let Some(l) = metrics_listener {
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
        tokio::spawn(watch_for_stalls(shared.clone()));
    } else if deployment {
        tokio::spawn(keep_erasures_vacuumed(shared.clone()));
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
    // and one already gone said, the stop going on to its checkpoint, where
    // the stop ended in an error and the checkpoint was skipped (the audit's
    // durability C7)
    if let Some(path) = &a.socket
        && let Err(e) = std::fs::remove_file(path)
    {
        say!("the socket {} was not removed: {e}", path.display());
    }
    // the write-ahead log carried into the store's file, so a clean stop
    // leaves the store whole in one file (second production review, minor)
    // skipped where the store was found lost, since a checkpoint would carry
    // its log into a file that is not the store's, or out of one that is
    // gone (ADR-0205 decision 8)
    if let Some(why) = shared.watch.latched.get() {
        // a store on PostgreSQL has no file to carry a log into, where the
        // stop named SQLite's (durability on PostgreSQL 10)
        if shared.store_path.is_some() {
            say!(
                "the write-ahead log was not carried into the store's file, the store being lost: {why}"
            );
        } else {
            say!("the store was served no more: {why}");
        }
    } else if shared.store_path.is_some()
        && let Ok(writer) = shared.writer.lock()
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
    /// A key a part of an import does not take is refused, naming it, where
    /// a misspelt one was read as absent and an object's attributes were
    /// dropped without a word (the second production review's minor).
    #[test]
    fn an_import_refuses_a_key_it_does_not_take() {
        let object = |extra: J| {
            let mut o = json!({"type": "T", "legacy_key": "1", "state": "OPEN"});
            if let (Some(m), Some(e)) = (o.as_object_mut(), extra.as_object()) {
                m.extend(e.clone());
            }
            imported_object(&o)
        };
        let refused = |r: Result<ImportedObject, Fault>| match r {
            Err(Fault::Bad(m)) => m,
            Ok(o) => format!("not refused: {}", o.legacy_key),
            Err(_) => "refused otherwise".to_string(),
        };
        assert!(object(json!({"attributes": {"a": 1}})).is_ok());
        assert_eq!(
            refused(object(json!({"atributes": {"a": 1}}))),
            "an imported object takes no key atributes"
        );
        let entry = json!({"legacy_entries": [
            {"kind": "note", "occurred_at": "2020-01-01T00:00:00Z", "paylod": {}}
        ]});
        assert_eq!(refused(object(entry)), "a legacy entry takes no key paylod");
        let class = json!({"type": "T", "kind": "invariant", "rule": "r", "shape": "s",
                           "disposition": "admit", "reasn": "kept"});
        assert!(
            matches!(violation_class(&class), Err(Fault::Bad(m)) if m == "a class takes no key reasn")
        );
    }

    /// The log says a volume too full for a checkpoint once as it falls
    /// below and once as it recovers, and nothing while it stays either way
    /// or cannot be read (ADR-0200 decision 7).
    #[test]
    fn low_disk_is_said_once_as_it_falls_and_once_as_it_recovers() {
        let watch = Watch::default();
        let plenty = Some(DISK_FLOOR * 16);
        let little = Some(DISK_FLOOR - 1);
        let said: Vec<bool> = [plenty, little, little, None, plenty, plenty, little]
            .into_iter()
            .map(|free| tell_low_disk(&watch, free))
            .collect();
        assert_eq!(said, [false, true, false, false, true, false, true]);
    }

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
            created: Vec::new(),
            flags: Vec::new(),
            replayed: false,
        });
        assert!(!w.stalled(wall_millis()));
        assert!(trim_due(0, 3, 0));
        assert!(!trim_due(3, 3, 0));
        assert!(!trim_due(0, 3, 1));
        let full = objectflow_core::db::Error::disk_full();
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
            created: Vec::new(),
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
        // each wait measured, the two turns taken at once, the one waited
        // for and the one its deadline ended (ADR-0201 decision 11)
        let waited = a.waited();
        assert_eq!(waited.count, 4, "{waited:?}");
        assert_eq!(waited.buckets[0], 2, "{waited:?}");
        assert!(waited.sum >= 0.3, "{waited:?}");
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

    /// A change's dry run reads under a deadline of its own, the dry-run
    /// timeout, and every other read under the read timeout, where a dry run
    /// shared the reads' 30 s and a store whose publish outgrew it loosened
    /// every read's to publish (seventh production review PUBLISH-SLOW,
    /// ADR-0194 decision 2).
    #[test]
    #[allow(clippy::panic)]
    fn a_dry_run_reads_under_a_deadline_of_its_own() {
        use std::time::{Duration, Instant};
        let dir =
            std::env::temp_dir().join(format!("objectflow-dry-deadline-{}", std::process::id()));
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
            path: Some(path.clone()),
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
            dry_run_timeout: Duration::from_secs(600),
            admission: Admission::new(1),
            worklists: Admission::new(1),
            objects: Admission::new(1),
            dry_runs: Admission::for_dry_runs(),
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
            groups: Groups::default(),
        });
        let deadline = |pool: &Admission| -> Duration {
            let Ok((_slot, reading)) = heavy_reading(&shared, pool, None) else {
                panic!("no read")
            };
            let Reading::Own(Some(store), _) = &reading else {
                panic!("no store read")
            };
            store
                .deadline
                .map(|at| at.saturating_duration_since(Instant::now()))
                .unwrap_or_default()
        };
        let dry = deadline(&shared.dry_runs);
        assert!(dry > Duration::from_secs(500), "{dry:?}");
        let worklist = deadline(&shared.worklists);
        assert!(worklist <= Duration::from_secs(1), "{worklist:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A write its client does not take fails within the idle timeout, and
    /// one the client takes, however slowly, does not (seventh production
    /// review, ADR-0194 decision 6).
    #[tokio::test]
    async fn an_answer_not_read_fails_within_the_idle_timeout() {
        use std::time::{Duration, Instant};
        async fn write_all(
            conn: &mut Unread<tokio::net::UnixStream>,
            data: &[u8],
        ) -> std::io::Result<()> {
            let mut off = 0;
            while off < data.len() {
                off += std::future::poll_fn(|cx| {
                    tokio::io::AsyncWrite::poll_write(
                        std::pin::Pin::new(&mut *conn),
                        cx,
                        &data[off..],
                    )
                })
                .await?;
            }
            Ok(())
        }
        let idle = Duration::from_millis(200);
        // past any socket's buffers, and no one reading
        let Ok((near, _far)) = tokio::net::UnixStream::pair() else {
            return;
        };
        let mut conn = Unread::new(near, idle);
        let began = Instant::now();
        // bounded, so a write that waits for good fails the test, never hangs it
        let unread = tokio::time::timeout(
            Duration::from_secs(5),
            write_all(&mut conn, &vec![7u8; 16 << 20]),
        )
        .await;
        assert!(
            matches!(unread, Ok(Err(_))),
            "a write no one read did not fail: {unread:?}"
        );
        assert!(
            began.elapsed() < Duration::from_secs(2),
            "{:?}",
            began.elapsed()
        );
        // a slow reader, taking some every few milliseconds
        let Ok((near, far)) = tokio::net::UnixStream::pair() else {
            return;
        };
        let mut conn = Unread::new(near, idle);
        let sent = 2 << 20;
        let reader = tokio::spawn(async move {
            let (mut got, mut buf) = (0usize, vec![0u8; 64 << 10]);
            while got < sent {
                tokio::time::sleep(Duration::from_millis(5)).await;
                if far.readable().await.is_err() {
                    break;
                }
                match far.try_read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => got += n,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => break,
                }
            }
            got
        });
        assert!(
            write_all(&mut conn, &vec![7u8; sent]).await.is_ok(),
            "a slow reader's write failed"
        );
        assert_eq!(reader.await.ok(), Some(sent));
    }

    /// A store refused at its start beside a `-wal` holding frames says,
    /// once, that if a copy was restored it is to be restored alone,
    /// whatever SQLite said; one beside an empty log or none says SQLite's
    /// fault alone (ADR-0194 decision 6, the audit's durability C5).
    #[test]
    #[allow(clippy::panic)]
    fn a_store_refused_beside_a_log_says_to_restore_the_copy_alone() {
        let dir =
            std::env::temp_dir().join(format!("objectflow-refused-start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{e}"));
        let path = dir.join("store.sqlite");
        let fault = "malformed database schema (Employee:E0) - orphan index";
        let alone = refused_start(&path, &fault).to_string();
        assert!(!alone.contains("restore the copy alone"), "{alone}");
        // an empty log, as any earlier clean run leaves it, is no restore's
        // (the audit's durability C5)
        std::fs::write(dir.join("store.sqlite-wal"), b"").unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(dir.join("store.sqlite-shm"), b"\0").unwrap_or_else(|e| panic!("{e}"));
        let empty = refused_start(&path, &fault).to_string();
        assert!(!empty.contains("restore the copy alone"), "{empty}");
        std::fs::write(dir.join("store.sqlite-wal"), b"frames").unwrap_or_else(|e| panic!("{e}"));
        let beside = refused_start(&path, &fault).to_string();
        assert!(beside.contains("if you restored a copy"), "{beside}");
        assert_eq!(
            beside.matches("restore the copy alone").count(),
            1,
            "{beside}"
        );
        assert_eq!(
            beside.matches("malformed database schema").count(),
            1,
            "{beside}"
        );
        let _ = std::fs::remove_dir_all(&dir);
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
            path: Some(path.clone()),
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
            dry_run_timeout: Duration::from_secs(30),
            admission: Admission::new(1),
            worklists: Admission::new(1),
            objects: Admission::new(1),
            dry_runs: Admission::for_dry_runs(),
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
            groups: Groups::default(),
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
        // a long read: a worklist slot held until its read's stop moves, or
        // 10 s
        let (taken, held) = std::sync::mpsc::channel();
        let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader = {
            let (s, stopped) = (shared.clone(), stopped.clone());
            std::thread::spawn(move || {
                let slot = s.worklists.take(
                    Instant::now() + Duration::from_secs(60),
                    Duration::from_secs(60),
                );
                let (stop, _) = read_stop();
                let began = stop.load(Ordering::SeqCst);
                // the read it begins carries the stop its slot holds
                let carried = reading(&s).ok().is_some_and(|mut r| {
                    r.store()
                        .ok()
                        .and_then(|st| st.stop.clone())
                        .is_some_and(|(e, _)| Arc::ptr_eq(&e, &stop))
                });
                let _ = taken.send(slot.is_ok() && carried);
                let start = Instant::now();
                while stop.load(Ordering::SeqCst) == began
                    && start.elapsed() < Duration::from_secs(10)
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
                stopped.store(stop.load(Ordering::SeqCst) != began, Ordering::SeqCst);
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
        // urgent: every pool barred as the cut's moment begins, so none
        // starts a read the cut would find running (the audit's cost E4,
        // ADR-0205 decision 14); the read running stopped, and the log cut
        let barred = {
            let s = shared.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                [
                    &s.admission,
                    &s.worklists,
                    &s.objects,
                    &s.dry_runs,
                    &s.agents,
                ]
                .iter()
                .all(|p| p.barred.load(Ordering::SeqCst))
            })
        };
        let began = Instant::now();
        assert!(cut_back(&shared, bytes / 5));
        assert_eq!(
            barred.join().ok(),
            Some(true),
            "a pool was not barred in the moment"
        );
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "{:?}",
            began.elapsed()
        );
        assert_eq!(shared.watch.cuts.load(Ordering::SeqCst), 1);
        assert!(reader.join().is_ok());
        assert!(
            stopped.load(Ordering::SeqCst),
            "the long read was not stopped"
        );
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
            path: Some(path.clone()),
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
            dry_run_timeout: Duration::from_secs(1),
            admission: Admission::new(1),
            worklists: Admission::new(1),
            objects: Admission::new(1),
            dry_runs: Admission::for_dry_runs(),
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
            groups: Groups::default(),
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
        // an object's history and its events, in a pool of their own, are
        // answered beside the held worklists (eighth production review,
        // ADR-0195 decision 5)
        let bot = issued["object_id"].clone();
        let began = Instant::now();
        let told = history(State(shared.clone()), Body(json!({"actor": op, "id": bot}))).await;
        let Ok(Json(told)) = told else {
            panic!("the history waited behind the worklists")
        };
        assert!(
            began.elapsed() < quick && told["events"].as_array().is_some_and(|e| !e.is_empty()),
            "{told}"
        );
        let position = told["events"][0]["position"].clone();
        let began = Instant::now();
        let found = events(
            State(shared.clone()),
            Body(json!({"actor": op, "positions": [position]})),
        )
        .await;
        assert!(
            found.is_ok() && began.elapsed() < quick,
            "the events waited behind the worklists"
        );
        // a query that reads its type whole waits among the analytics, one
        // narrowed by its states in the worklists (ninth production review,
        // ADR-0196 decision 4)
        let queried = |body: J| {
            let shared = shared.clone();
            async move {
                let began = Instant::now();
                (
                    query(State(shared), Body(body)).await.is_ok(),
                    began.elapsed(),
                )
            }
        };
        // an order key or a field named twice, or past their most, is a 400,
        // where 100k order keys took 5.1 s (ninth production review,
        // ADR-0196 decision 9)
        for body in [
            json!({"actor": op, "type": "Bot", "order": ["key", "key"]}),
            json!({"actor": op, "type": "Bot", "fields": ["key", "state", "key"]}),
            json!({"actor": op, "type": "Bot", "order": vec!["key"; QUERY_ORDER_KEYS + 1]}),
        ] {
            let bounded = query(State(shared.clone()), Body(body.clone())).await;
            assert!(matches!(bounded, Err(Fault::Bad(_))), "{body}");
        }
        let scan = json!({"actor": op, "type": "Bot", "order": ["key"]});
        let narrowed =
            json!({"actor": op, "type": "Bot", "filter": "state == ON", "order": ["key"]});
        let (answered, took) = queried(scan.clone()).await;
        assert!(
            answered && took < quick,
            "a scan waited {took:?} behind the worklists"
        );
        let (answered, _) = queried(narrowed.clone()).await;
        assert!(!answered, "a narrowed query took a held worklist slot");
        drop(held);
        let held = shared.admission.take(
            Instant::now() + Duration::from_secs(60),
            Duration::from_secs(60),
        );
        assert!(held.is_ok(), "no analytics slot free");
        let (answered, took) = queried(narrowed).await;
        assert!(
            answered && took < quick,
            "a narrowed query waited {took:?} behind the analytics"
        );
        let (answered, _) = queried(scan).await;
        assert!(!answered, "a scan took a held analytics slot");
        drop(held);
        let held = shared.worklists.take(
            Instant::now() + Duration::from_secs(60),
            Duration::from_secs(60),
        );
        assert!(held.is_ok(), "no worklist slot free");
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
    /// object's history and the events at positions among them, in the
    /// objects' pool; a read of one object or of the declaration never waits
    /// (ADR-0189 decision 2, ADR-0192 decision 5, ADR-0195 decision 5).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[allow(clippy::panic)]
    async fn a_read_of_an_object_whose_rules_grow_waits_among_the_objects_reads() {
        use std::time::{Duration, Instant};
        let dir =
            std::env::temp_dir().join(format!("objectflow-object-slot-{}", std::process::id()));
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
            path: Some(path.clone()),
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
            dry_run_timeout: Duration::from_secs(1),
            admission: Admission::new(1),
            worklists: Admission::new(1),
            objects: Admission::new(1),
            dry_runs: Admission::for_dry_runs(),
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
            groups: Groups::default(),
        });
        let op = json!({"id": "operator"});
        // a type whose derived attribute scans its type, and one that reads
        // nothing beyond itself, published through the wire
        let source = "module: m\ncategories: [live, closed]\ntypes:\n  Wide:\n    description: A thing.\n    tracking: record\n    states:\n      ON: { category: live, description: On. }\n      OFF: { category: closed, final: true, description: Off. }\n    derived_attributes:\n      peers: { description: How many., expression: count(w in Wide) }\n    transitions:\n      make: { kind: initial, to: ON }\n      stop: { kind: external, from: ON, to: OFF }\n  Guarded:\n    description: A thing.\n    tracking: record\n    states:\n      ON: { category: live, description: On. }\n      OFF: { category: closed, final: true, description: Off. }\n    conditions:\n      few: { description: Few., expression: count(g in Guarded) < 1000, remedy: dependent }\n    transitions:\n      make: { kind: initial, to: ON }\n      stop: { kind: external, from: ON, to: OFF, guards: { few: deny } }\n  Narrow:\n    description: A thing.\n    tracking: record\n    states:\n      ON: { category: live, description: On. }\n      OFF: { category: closed, final: true, description: Off. }\n    transitions:\n      make: { kind: initial, to: ON }\n      stop: { kind: external, from: ON, to: OFF }\n";
        let Ok(Json(drafted)) = request_op(
            State(shared.clone()),
            Body(
                json!({"actor": op, "type": "DeclarationChange", "transition": "draft",
                        "inputs": {"source": {"m": source}, "source_ref": "git:m@1"}}),
            ),
        )
        .await
        else {
            panic!("no change drafted")
        };
        let change = drafted["object_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let Ok(Json(submitted)) = request_op(
            State(shared.clone()),
            Body(json!({"actor": op, "object_id": change, "transition": "submit"})),
        )
        .await
        else {
            panic!("not submitted")
        };
        assert_eq!(submitted["kind"], "satisfied", "{submitted}");
        let Ok(Json(approved)) = publish(
            State(shared.clone()),
            Body(json!({"actor": op, "change": change, "expected_version": submitted["version"]})),
        )
        .await
        else {
            panic!("not approved")
        };
        assert_eq!(approved["verdict"]["kind"], "satisfied", "{approved}");
        let made = |tn: &str| {
            let shared = shared.clone();
            let body = json!({"actor": op, "type": tn, "transition": "make"});
            async move {
                match request_op(State(shared), Body(body)).await {
                    Ok(Json(v)) => v["object_id"].as_str().unwrap_or_default().to_string(),
                    Err(_) => panic!("not made"),
                }
            }
        };
        let (wide, guarded, narrow) = (
            made("Wide").await,
            made("Guarded").await,
            made("Narrow").await,
        );
        // the objects' one slot held, never the analytics': the wide
        // object's reads wait for it, past their deadline, the guarded one's
        // availability and check, which evaluate its guard, and not its get,
        // which does not, and the narrow one's reads at once (fourteenth
        // production review)
        let held = shared.objects.take(
            Instant::now() + Duration::from_secs(600),
            Duration::from_secs(600),
        );
        assert!(held.is_ok(), "no slot free");
        for (route, answer) in [
            (
                "get",
                get(State(shared.clone()), Body(json!({"id": wide}))).await,
            ),
            (
                "availability",
                availability(
                    State(shared.clone()),
                    Body(json!({"actor": op, "id": wide})),
                )
                .await,
            ),
            (
                "check",
                check(
                    State(shared.clone()),
                    Body(json!({"actor": op, "object_id": wide, "transition": "stop"})),
                )
                .await,
            ),
        ] {
            assert!(
                matches!(&answer, Err(Fault::TimedOut(m)) if m.contains("--max-reads")),
                "{route} of a wide object did not wait its turn"
            );
        }
        let answer = availability(
            State(shared.clone()),
            Body(json!({"actor": op, "id": guarded})),
        )
        .await;
        assert!(
            matches!(&answer, Err(Fault::TimedOut(m)) if m.contains("--max-reads")),
            "availability of a guarded object did not wait its turn"
        );
        let began = Instant::now();
        assert!(
            get(State(shared.clone()), Body(json!({"id": guarded})))
                .await
                .is_ok()
        );
        assert!(
            get(State(shared.clone()), Body(json!({"id": narrow})))
                .await
                .is_ok()
        );
        assert!(
            availability(
                State(shared.clone()),
                Body(json!({"actor": op, "id": narrow}))
            )
            .await
            .is_ok()
        );
        assert!(began.elapsed() < Duration::from_millis(900));
        drop(held);
        let _ = std::fs::remove_dir_all(&dir);
    }

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
            path: Some(path.clone()),
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
            dry_run_timeout: Duration::from_secs(1),
            admission: Admission::new(1),
            worklists: Admission::new(1),
            objects: Admission::new(1),
            dry_runs: Admission::for_dry_runs(),
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
            groups: Groups::default(),
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
        let held = [
            &shared.admission,
            &shared.worklists,
            &shared.dry_runs,
            &shared.objects,
        ]
        .map(|pool| {
            pool.take(
                Instant::now() + Duration::from_secs(600),
                Duration::from_secs(600),
            )
        });
        assert!(held.iter().all(Result::is_ok), "no slot free");
        let waited = |answer: Answer, started: Instant, route: &str| {
            // a change's dry run's wait said by its own flag, where it named
            // --read-timeout and --max-reads (ninth production review N-8)
            let flag = if ["publish", "request_op", "check"].contains(&route) {
                "--dry-run-timeout"
            } else {
                "--max-reads"
            };
            assert!(
                matches!(&answer, Err(Fault::TimedOut(m)) if m.contains(flag)),
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
        light!(declaration, json!({"actor": op, "type": "Subscription"}));
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
    /// A read whose client goes is stopped at its next step: the connection
    /// drops the answer it waited for, which moves the read's stop, where the
    /// read ran to its deadline for no one (D656, ADR-0205 decision 16).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    async fn a_read_whose_client_goes_is_stopped() {
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let shared: Shared = Arc::new(Service {
            writer: Mutex::new(Sandbox::default()),
            reads: None,
            endpoints: std::collections::BTreeMap::new(),
            tokens: std::collections::BTreeMap::new(),
            evaluator_timeout: std::time::Duration::from_secs(5),
            deployment: false,
            read_timeout: std::time::Duration::from_secs(30),
            dry_run_timeout: std::time::Duration::from_secs(30),
            admission: Admission::new(MAX_READS),
            worklists: Admission::new(MAX_READS),
            objects: Admission::new(MAX_READS),
            dry_runs: Admission::for_dry_runs(),
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
            groups: Groups::default(),
        });
        let f = flag.clone();
        let router = Router::new()
            .route(
                "/v1/slow",
                post(move || {
                    let f = f.clone();
                    async move {
                        // a read's work, watching its stop as a read's steps do
                        answered_off(move || {
                            let (stop, _) = read_stop();
                            let began = stop.load(Ordering::SeqCst);
                            let start = std::time::Instant::now();
                            while stop.load(Ordering::SeqCst) == began
                                && start.elapsed() < std::time::Duration::from_secs(5)
                            {
                                std::thread::sleep(std::time::Duration::from_millis(10));
                            }
                            f.store(stop.load(Ordering::SeqCst) != began, Ordering::SeqCst);
                            Ok(Json(json!({})))
                        })
                        .await
                    }
                }),
            )
            .with_state(shared.clone());
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let at = l.local_addr().unwrap();
        tokio::spawn(served(
            Listening::Tcp(l),
            router,
            shared.clone(),
            std::future::pending::<()>(),
            std::time::Duration::from_secs(30),
            16,
        ));
        tokio::task::spawn_blocking(move || {
            use std::io::Write;
            let mut c = std::net::TcpStream::connect(at).unwrap();
            c.write_all(b"POST /v1/slow HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\n\r\n")
                .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(300));
            drop(c);
        })
        .await
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        assert!(
            flag.load(Ordering::SeqCst),
            "the read ran on after its client went"
        );
    }

    /// A `get` reads its object's last event alone for the version it
    /// answers, never the object's history, which grew with it (the audit's
    /// cost B11).
    #[test]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    fn a_get_reads_no_history() {
        let text = "\nmodule: notes\ncategories: [live]\ntypes:\n  Job:\n    attributes:\n      title: { type: string, optional: true }\n    states:\n      ON: { category: live }\n    transitions:\n      open: { kind: initial, to: ON }\n      note: { kind: internal, from: [ON], optional_inputs: [title] }\n";
        let lib =
            objectflow_core::model::Library::load(&[("notes.yaml".into(), text.into())]).unwrap();
        let mut s = Store::sandbox(lib, 0).unwrap();
        let ask = |s: &mut Store, xn: &str, id: Option<&str>| {
            let r = objectflow_core::request::Request {
                transition: xn.into(),
                actor: objectflow_core::model::OPERATOR.into(),
                object_id: id.map(str::to_string),
                type_: id.is_none().then(|| "Job".to_string()),
                inputs: std::collections::BTreeMap::new(),
                occurred_at: None,
                propose: false,
                expires_at: None,
                expected_version: None,
                target_version: None,
                principal: None,
                idempotency_key: None,
                context: None,
            };
            match objectflow_core::request::request(s, &r).unwrap().unwrap() {
                objectflow_core::verdict::Verdict::Satisfied { object_id, .. } => object_id,
                other => panic!("{other:?}"),
            }
        };
        let job = ask(&mut s, "open", None);
        for _ in 0..50 {
            ask(&mut s, "note", Some(&job));
        }
        let before = objectflow_core::world::events_read();
        let Ok(Json(got)) = object_json(&s, &job) else {
            panic!("the get not answered")
        };
        assert_eq!(got["declaration_version"], json!(s.lib.version));
        assert_eq!(objectflow_core::world::events_read(), before);
        // as a history read counts what it reads
        assert_eq!(s.history(&job).unwrap().len(), 51);
        assert_eq!(objectflow_core::world::events_read(), before + 51);
    }

    /// A store a read or a write finds unreadable, by SQLite's words for a
    /// corrupt file or one that is no database, answers 503
    /// `store_unreadable` and marks its answer, which the logging layer
    /// latches the service on; and a service latched answers every write
    /// with why until a restart (the audit's durability S5, ADR-0205
    /// decision 8).
    /// A read stopped at its deadline on the database is answered `503
    /// read_timeout`, as one past its deadline between steps is, whether the
    /// store's error reaches the service itself or through a world's read
    /// of the store, which carries its words, where it answered `500`
    /// (ADR-0215).
    #[test]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    fn a_read_stopped_at_the_database_is_a_read_timeout() {
        let said = format!(
            "{}: a read passed its deadline, the database not answering in time; its connection was closed",
            objectflow_core::store::DEADLINE_PASSED
        );
        for fault in [
            Fault::from(objectflow_core::store::StoreError(said.clone())),
            Fault::from(NotRun::Unreadable(format!(
                "the actors, by identity: {said}"
            ))),
        ] {
            assert!(matches!(&fault, Fault::TimedOut(_)), "{fault:?}");
            let answer = fault.into_response();
            assert_eq!(answer.status(), StatusCode::SERVICE_UNAVAILABLE);
        }
    }

    #[test]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    fn a_store_found_unreadable_latches_the_service() {
        for fault in [
            Fault::from(objectflow_core::store::StoreError(
                "database disk image is malformed".into(),
            )),
            Fault::from(NotRun::Unreadable(
                "the store's last position, which could not be read: file is not a database".into(),
            )),
        ] {
            let answer = fault.into_response();
            assert_eq!(answer.status(), StatusCode::SERVICE_UNAVAILABLE);
            let Some(Lost(why)) = answer.extensions().get::<Lost>() else {
                panic!("the answer is not marked lost")
            };
            assert!(why.starts_with("store_unreadable: "), "{why}");
        }
        // another store fault is the service's own, and latches nothing
        let other = Fault::from(objectflow_core::store::StoreError(
            "no such table: t_x".into(),
        ))
        .into_response();
        assert_eq!(other.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(other.extensions().get::<Lost>().is_none());
        let shared: Shared = Arc::new(Service {
            writer: Mutex::new(Sandbox::default()),
            reads: None,
            endpoints: std::collections::BTreeMap::new(),
            tokens: std::collections::BTreeMap::new(),
            evaluator_timeout: std::time::Duration::from_secs(5),
            deployment: true,
            read_timeout: std::time::Duration::from_secs(30),
            dry_run_timeout: std::time::Duration::from_secs(30),
            admission: Admission::new(MAX_READS),
            worklists: Admission::new(MAX_READS),
            objects: Admission::new(MAX_READS),
            dry_runs: Admission::for_dry_runs(),
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
            groups: Groups::default(),
        });
        assert_eq!(store_lost(&shared), None);
        assert!(locked(&shared).is_ok());
        let why = "store_unreadable: the store answered \"database disk image is malformed\"";
        assert_eq!(latch(&shared, why.into()), why);
        // the first cause kept
        assert_eq!(latch(&shared, "store_replaced: later".into()), why);
        assert_eq!(store_lost(&shared).as_deref(), Some(why));
        let Err(Fault::Unavailable(refused)) = locked(&shared).map(|_| ()) else {
            panic!("a write taken by a latched service")
        };
        assert_eq!(refused, why);
    }

    /// An answer that found its store unreadable latches the service as
    /// the logging layer passes it on, whatever route met it, a read's
    /// among them (the audit's durability S5, ADR-0205 decision 8).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    async fn an_unreadable_answer_latches_the_service_as_it_is_passed_on() {
        let shared: Shared = Arc::new(Service {
            writer: Mutex::new(Sandbox::default()),
            reads: None,
            endpoints: std::collections::BTreeMap::new(),
            tokens: std::collections::BTreeMap::new(),
            evaluator_timeout: std::time::Duration::from_secs(5),
            deployment: true,
            read_timeout: std::time::Duration::from_secs(30),
            dry_run_timeout: std::time::Duration::from_secs(30),
            admission: Admission::new(MAX_READS),
            worklists: Admission::new(MAX_READS),
            objects: Admission::new(MAX_READS),
            dry_runs: Admission::for_dry_runs(),
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
            groups: Groups::default(),
        });
        let router = Router::new()
            .route(
                "/v1/get",
                post(|| async {
                    Fault::from(objectflow_core::store::StoreError(
                        "database disk image is malformed".into(),
                    ))
                }),
            )
            .layer(axum::middleware::from_fn_with_state(shared.clone(), logged))
            .with_state(shared.clone());
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let at = l.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(l, router).await;
        });
        let answer = tokio::task::spawn_blocking(move || {
            use std::io::{Read, Write};
            let mut c = std::net::TcpStream::connect(at).unwrap();
            c.write_all(
                b"POST /v1/get HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
            let mut out = String::new();
            c.read_to_string(&mut out).unwrap();
            out
        })
        .await
        .unwrap();
        assert!(answer.starts_with("HTTP/1.1 503"), "{answer}");
        assert!(
            shared
                .watch
                .latched
                .get()
                .is_some_and(|why| why.starts_with("store_unreadable: ")),
            "{answer}"
        );
    }

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
            dry_run_timeout: std::time::Duration::from_secs(30),
            admission: Admission::new(MAX_READS),
            worklists: Admission::new(MAX_READS),
            objects: Admission::new(MAX_READS),
            dry_runs: Admission::for_dry_runs(),
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
            groups: Groups::default(),
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

    /// A request's panic is answered `500`, never left to unwind its
    /// connection's task, and a connection's place among the open ones is let
    /// go however its task ends, where each panic kept one for good and
    /// twelve under --max-connections 8 answered every request `503 busy`
    /// (tenth production review).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[allow(clippy::panic)]
    async fn a_panic_is_answered_and_its_connection_lets_go_of_its_place() {
        let answered: Result<(), Fault> = answering(|| panic!("a planted panic"));
        assert!(matches!(answered, Err(Fault::Internal(_))));
        let shared: Shared = Arc::new(Service {
            writer: Mutex::new(Sandbox::default()),
            reads: None,
            endpoints: std::collections::BTreeMap::new(),
            tokens: std::collections::BTreeMap::new(),
            evaluator_timeout: std::time::Duration::from_secs(5),
            deployment: false,
            read_timeout: std::time::Duration::from_secs(30),
            dry_run_timeout: std::time::Duration::from_secs(30),
            admission: Admission::new(MAX_READS),
            worklists: Admission::new(MAX_READS),
            objects: Admission::new(MAX_READS),
            dry_runs: Admission::for_dry_runs(),
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
            groups: Groups::default(),
        });
        let open: Arc<Mutex<Opened>> = Arc::new(Mutex::new(Opened::new()));
        shared.connections.open.fetch_add(1, Ordering::SeqCst);
        open.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                7,
                (
                    Arc::new(std::sync::atomic::AtomicIsize::new(0)),
                    Arc::new(tokio::sync::Notify::new()),
                ),
            );
        let task = tokio::spawn({
            let (open, shared) = (open.clone(), shared.clone());
            async move {
                let _leaving = Leaving {
                    open,
                    id: 7,
                    shared,
                };
                panic!("a connection's task panicking");
            }
        });
        assert!(task.await.is_err());
        assert_eq!(shared.connections.open.load(Ordering::SeqCst), 0);
        assert!(
            open.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
        );
    }
}

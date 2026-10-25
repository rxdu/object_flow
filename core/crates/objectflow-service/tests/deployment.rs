//! A deployment's service (ADR-0174), driven as a caller drives it: the
//! service started on a store's file, its flow published through the wire,
//! requests answered, and the store reopened by a second start, with what the
//! first recorded. The binary is the one this package builds.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use serde_json::{Value, json};

/// A service and the address it answers on: one this test started, stopped
/// when dropped, or another thread's view of one, which stops nothing.
struct Service {
    child: Option<Child>,
    addr: String,
    /// What it has written to stderr since it started listening, read as
    /// it is written, so a service logging every answer never fills the
    /// pipe and stops.
    said: std::sync::Arc<std::sync::Mutex<String>>,
}

/// A service's stderr past its first line, drained into a buffer by a thread
/// of its own until the service exits.
fn drained(
    stderr: BufReader<std::process::ChildStderr>,
) -> std::sync::Arc<std::sync::Mutex<String>> {
    let said = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let into = said.clone();
    std::thread::spawn(move || {
        for line in stderr.lines().map_while(Result::ok) {
            if let Ok(mut s) = into.lock() {
                s.push_str(&line);
                s.push('\n');
            }
        }
    });
    said
}

impl Service {
    /// What the service has written to stderr since it began listening.
    fn said(&self) -> String {
        self.said.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// What it has written, once it holds a line containing `want` or after
    /// five seconds: a line the service wrote before answering may not yet
    /// have been read from its pipe when the answer arrives.
    fn said_within(&self, want: &str) -> String {
        let began = std::time::Instant::now();
        loop {
            let said = self.said();
            if said.contains(want) || began.elapsed() > std::time::Duration::from_secs(5) {
                return said;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn start(args: &[&str]) -> Service {
    let mut service = Command::new(env!("CARGO_BIN_EXE_objectflow-service"));
    service.args(args);
    start_command(service)
}

/// The service started by `command`, which runs it, as `nsenter` does
/// within a namespace.
fn start_command(mut command: Command) -> Service {
    let mut child = command
        .stderr(Stdio::piped())
        .spawn()
        .expect("the service starts");
    let mut stderr = BufReader::new(child.stderr.take().expect("its stderr"));
    let mut line = String::new();
    stderr.read_line(&mut line).expect("its first line");
    // the line stamped, as every line is
    let Some(addr) = line
        .trim()
        .split_once("listening on http://")
        .map(|(_, addr)| addr.to_string())
    else {
        // stopped before the test fails, so no service outlives it
        let _ = child.kill();
        let _ = child.wait();
        panic!("the service did not start: {line}");
    };
    Service {
        child: Some(child),
        addr,
        said: drained(stderr),
    }
}

/// One HTTP/1.1 exchange: its status and its JSON body.
fn call(s: &Service, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let mut stream = TcpStream::connect(&s.addr).expect("a connection");
    let text = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        s.addr,
        text.len()
    )
    .expect("the request is written");
    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("the answer");
    let status = raw[9..12].parse().expect("a status");
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .unwrap_or_default();
    // a body is sent whole or chunked; a chunked one is its chunks joined
    let body = if raw
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut out = String::new();
        let mut rest = body;
        while let Some((size, after)) = rest.split_once("\r\n") {
            let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
            if n == 0 {
                break;
            }
            out.push_str(&after[..n]);
            rest = &after[n + 2..];
        }
        out
    } else {
        body.to_string()
    };
    (status, serde_json::from_str(&body).unwrap_or(Value::Null))
}

/// A start the service refuses: its exit status and what it wrote to
/// stderr. One that serves instead is stopped after ten seconds and fails
/// the test, rather than holding it forever.
fn refused(args: &[&str]) -> (Option<i32>, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_objectflow-service"))
        .args(args)
        .stderr(Stdio::piped())
        .spawn()
        .expect("the service runs");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().expect("its status") {
            break status;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the service started with {args:?}, which it should refuse");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let mut said = String::new();
    child
        .stderr
        .take()
        .expect("its stderr")
        .read_to_string(&mut said)
        .expect("what it said");
    (status.code(), said)
}

fn post(s: &Service, path: &str, body: &Value) -> Value {
    let (status, out) = call(s, "POST", path, Some(body));
    assert_eq!(status, 200, "{path}: {out}");
    out
}

/// Each test's directory is named once in this file, so no test removes
/// another's under its running service: two PostgreSQL tests made theirs as
/// the file tests named alike made theirs, and a run of both failed one,
/// its store replaced (2026-10-18, twice).
#[test]
fn every_tests_directory_is_its_own() {
    let source = include_str!("deployment.rs");
    let mut seen = std::collections::BTreeSet::new();
    let quote = '"';
    let opening = format!("fresh_store({quote}");
    for (at, _) in source.match_indices(&opening) {
        let rest = &source[at + opening.len()..];
        let Some(end) = rest.find(quote) else {
            continue;
        };
        let name = &rest[..end];
        assert!(
            seen.insert(name.to_string()),
            "fresh_store({name:?}) is named twice"
        );
    }
    assert!(seen.len() > 20, "{} names read", seen.len());
}

fn fresh_store(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("objectflow-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    sweep_stale(name);
    std::fs::create_dir_all(&dir).expect("a directory");
    dir.join("store.sqlite")
}

/// The directories of this name earlier runs left, their process gone,
/// removed: most tests keep theirs when they pass, and every run of the
/// suite left one each, twenty thousand in /tmp by 2026-10-21, 23 GB. A
/// process alive keeps its own, another run's beside this one; where no
/// `/proc` says which are alive, none is removed.
fn sweep_stale(name: &str) {
    let proc = Path::new("/proc");
    if !proc.join("self").exists() {
        return;
    }
    let prefix = format!("objectflow-{name}-");
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let file = entry.file_name();
        let Some(pid) = file
            .to_str()
            .and_then(|f| f.strip_prefix(&prefix))
            .and_then(|rest| rest.parse::<u32>().ok())
        else {
            continue;
        };
        if pid != std::process::id() && !proc.join(pid.to_string()).exists() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// A test's directory left by a run whose process is gone is removed when
/// the test makes its own again, and one a live process holds is kept
/// (`sweep_stale`).
#[test]
fn a_stale_tests_directory_is_swept() {
    if !Path::new("/proc/self").exists() {
        eprintln!("skipped where no /proc says which processes are alive");
        return;
    }
    let tmp = std::env::temp_dir();
    // a process id past any the kernel hands out, and init's, always alive
    let gone = tmp.join("objectflow-sweeping-4294967295");
    let alive = tmp.join("objectflow-sweeping-1");
    let other = tmp.join("objectflow-sweeping-twice-4294967295");
    for d in [&gone, &alive, &other] {
        std::fs::create_dir_all(d).expect("a directory");
        std::fs::write(d.join("store.sqlite"), b"x").expect("a file");
    }
    let file = fresh_store("sweeping");
    assert!(!gone.exists(), "a dead process's directory kept");
    assert!(alive.exists(), "a live process's directory removed");
    assert!(other.exists(), "another test's directory removed");
    for d in [&alive, &other] {
        let _ = std::fs::remove_dir_all(d);
    }
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

fn example(name: &str) -> String {
    let at = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/design/flow-format/examples")
        .join(name);
    std::fs::read_to_string(&at).unwrap_or_else(|e| panic!("{}: {e}", at.display()))
}

/// The people and service modules published as the store's first version,
/// as every version is: drafted, submitted and approved through the wire.
fn publish_service(s: &Service) {
    publish(
        s,
        &json!({"people": example("people.yaml"), "service": example("service.yaml")}),
    );
}

/// Modules published as the store's next version, as every version is:
/// drafted, submitted and approved through the wire.
fn publish(s: &Service, source: &Value) {
    let op = json!({"id": "operator"});
    let drafted = post(
        s,
        "/v1/request",
        &json!({
        "actor": op, "type": "DeclarationChange", "transition": "draft",
        "inputs": {"source": source, "source_ref": "test:deployment"}}),
    );
    assert_eq!(drafted["kind"], "satisfied", "{drafted}");
    let change = drafted["object_id"]
        .as_str()
        .expect("the change")
        .to_string();
    let submitted = post(
        s,
        "/v1/request",
        &json!({"actor": op, "object_id": change, "transition": "submit"}),
    );
    assert_eq!(submitted["kind"], "satisfied", "{submitted}");
    // a preview's expected_version is a version too, where one of any JSON
    // type was taken (thirteenth production review)
    let (status, said) = call(
        s,
        "POST",
        "/v1/publish",
        Some(&json!({"actor": op, "change": change, "dry_run": true, "expected_version": "two"})),
    );
    assert_eq!(status, 400, "{said}");
    let approved = post(
        s,
        "/v1/publish",
        &json!({"actor": op, "change": change, "expected_version": submitted["version"]}),
    );
    assert_eq!(approved["verdict"]["kind"], "satisfied", "{approved}");
}

#[test]
fn a_deployments_store_survives_a_restart_on_the_wall_clock() {
    let file = fresh_store("restart");
    let path = file.to_str().expect("a path");
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_secs();
    let job;
    {
        let s = start(&["--store", path, "--create", "--port", "0"]);
        let (status, health) = call(&s, "GET", "/health", None);
        assert_eq!(status, 200);
        assert_eq!(
            (health["shape"].as_str(), health["version"].as_i64()),
            (Some("deployment"), Some(0))
        );
        // a deployment routes none of the sandbox's controls
        for control in ["load", "reset", "clock", "evaluators"] {
            let (status, out) = call(&s, "POST", &format!("/sandbox/{control}"), Some(&json!({})));
            assert_eq!(status, 404, "/sandbox/{control}: {out}");
        }
        publish_service(&s);
        // the clock moves with the wall's, without a caller moving it
        let (_, read) = call(&s, "GET", "/health", None);
        let read = objectflow_core::value::parse_timestamp(read["now"].as_str().expect("a clock"))
            .expect("a timestamp");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let op = json!({"id": "operator"});
        let ana = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "User", "transition": "add", "inputs": {"login": "ana", "name": "Ana"}}),
        );
        assert_eq!(ana["kind"], "satisfied", "{ana}");
        let opened = post(
            &s,
            "/v1/request",
            &json!({"actor": {"id": "ana"}, "type": "ServiceJob", "transition": "open", "inputs": {"engineer": ana["object_id"]}}),
        );
        assert_eq!(opened["kind"], "satisfied", "{opened}");
        job = opened["object_id"].as_str().expect("the job").to_string();
        let got = post(&s, "/v1/get", &json!({"id": job}));
        // recorded at the wall clock, not a sandbox's
        let created =
            objectflow_core::value::parse_timestamp(got["created_at"].as_str().expect("a time"))
                .expect("a timestamp");
        assert!(created >= i64::try_from(before).expect("a time"), "{got}");
        assert!(created > read, "{got}");
    }
    let s = start(&["--store", path, "--port", "0"]);
    let (_, health) = call(&s, "GET", "/health", None);
    assert_eq!(health["version"].as_i64(), Some(1), "{health}");
    let got = post(&s, "/v1/get", &json!({"id": job}));
    assert_eq!(got["state"], "OPEN", "{got}");
    let history = post(&s, "/v1/history", &json!({"id": job}));
    assert_eq!(
        history["events"].as_array().map(Vec::len),
        Some(1),
        "{history}"
    );
    drop(s);
    // a machine's clock stepped back behind the store's last record: the
    // store's clock stays at that record
    let conn = rusqlite::Connection::open(&file).expect("the file");
    conn.execute(
        "UPDATE of_event SET recorded_at = '2100-01-01T00:00:00Z' WHERE position = (SELECT max(position) FROM of_event)",
        [],
    )
    .expect("a record moved later");
    drop(conn);
    let s = start(&["--store", path, "--port", "0"]);
    let (_, health) = call(&s, "GET", "/health", None);
    assert_eq!(health["now"], "2100-01-01T00:00:00Z", "{health}");
    // the clock's lead, said where an operator looks (production review
    // CLOCK-PIN): in health, at the start, and on each answer's line
    assert!(
        health["clock_lead_seconds"].as_i64().unwrap_or(0) > 2_000_000_000,
        "{health}"
    );
    post(
        &s,
        "/v1/get",
        &json!({"actor": {"id": "operator"}, "id": "operator"}),
    );
    let said = s.said_within("POST /v1/get 200");
    assert!(
        said.contains("the store's clock leads the wall clock by"),
        "{said}"
    );
    assert!(
        said.contains("POST /v1/get 200") && said.contains(" clock_lead="),
        "{said}"
    );
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// A store at a release this one does not upgrade from, a later one's or
/// one it does not know, does not start, so there is no downgrade
/// (ADR-0174 decision 3, ADR-0177 decision 4).
#[test]
fn a_store_of_a_release_this_one_does_not_upgrade_from_is_refused() {
    for release in ["objectflow-core 9.0.0", "objectflow-core 0.0.0"] {
        let file = fresh_store("release");
        let path = file.to_str().expect("a path");
        drop(start(&["--store", path, "--create", "--port", "0"]));
        let conn = rusqlite::Connection::open(&file).expect("the file");
        conn.execute(
            "INSERT INTO of_engine_release (seq, release, installed_at, installed_by) \
             VALUES (2, ?1, '2026-01-01T00:00:00Z', 'operator')",
            [release],
        )
        .expect("the release written");
        drop(conn);
        let (code, said) = refused(&["--store", path, "--port", "0"]);
        assert_ne!(code, Some(0));
        assert!(
            said.contains(&format!("is at {release}, which objectflow-core")),
            "{said}"
        );
        let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
    }
}

#[test]
fn a_sandbox_still_answers_its_controls_and_its_health() {
    let s = start(&["--port", "0"]);
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(status, 200);
    assert_eq!(
        (health["shape"].as_str(), health["version"].as_i64()),
        (Some("sandbox"), Some(0)),
        "{health}"
    );
    let (status, _) = call(&s, "POST", "/sandbox/reset", Some(&json!({})));
    assert_ne!(status, 404);
}

/// A flag that is not a JSON boolean, and inputs that are not an object,
/// are refused, never read as false or as none: `"dry_run": "true"` read as
/// false installed a version (production review F6). Each body is valid
/// but for the one key.
#[test]
fn a_flag_or_inputs_of_another_type_is_refused() {
    let path = fresh_store("flags");
    let s = start(&[
        "--store",
        path.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let op = json!({"id": "operator"});
    let draft = json!({"actor": op, "type": "DeclarationChange", "transition": "draft",
                       "inputs": {"source": {"people": example("people.yaml")}, "source_ref": "test"}});
    let with = |key: &str, v: Value| {
        let mut b = draft.clone();
        b[key] = v;
        b
    };
    for (path, body) in [
        ("/v1/request", with("propose", json!("yes"))),
        ("/v1/request", with("inputs", json!(["source"]))),
        (
            "/v1/publish",
            json!({"actor": op, "change": "C1", "expected_version": 1, "dry_run": "true"}),
        ),
        (
            "/v1/import_batch",
            json!({"actor": op, "batch": {"objects": []}, "dry_run": 1}),
        ),
        ("/v1/upgrade", json!({"actor": op, "dry_run": "false"})),
    ] {
        let (status, said) = call(&s, "POST", path, Some(&body));
        assert_eq!(status, 400, "{path} {body}: {said}");
    }
    // the draft itself, the flags and inputs as they should be, is taken
    let (status, said) = call(
        &s,
        "POST",
        "/v1/request",
        Some(&with("propose", json!(false))),
    );
    assert_eq!(
        (status, said["kind"].as_str()),
        (200, Some("satisfied")),
        "{said}"
    );
}

/// A request's own `principal`, which `wire.md` lists, is read as the
/// actor's is: typed, refused `actor_known` where it names no one, refused
/// where the two disagree, and recorded on the event; it was taken and
/// dropped, any value satisfied (third production review PRINCIPAL).
#[test]
fn a_requests_principal_is_read_checked_and_recorded() {
    let path = fresh_store("principal");
    let s = start(&[
        "--store",
        path.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    publish_service(&s);
    let op = json!({"id": "operator"});
    let ana = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "User", "transition": "add", "inputs": {"login": "ana", "name": "Ana"}}),
    );
    assert_eq!(ana["kind"], "satisfied", "{ana}");
    let open = |extra: Value| {
        let mut body = json!({"actor": {"id": "ana"}, "type": "ServiceJob", "transition": "open",
                              "inputs": {"engineer": ana["object_id"]}});
        for (k, v) in extra.as_object().expect("keys") {
            body[k] = v.clone();
        }
        call(&s, "POST", "/v1/request", Some(&body))
    };
    for extra in [
        json!({"principal": "nobody"}),
        json!({"actor": {"id": "ana", "principal": "nobody"}}),
    ] {
        let (status, said) = open(extra.clone());
        assert_eq!(
            (status, said["kind"].as_str(), said["clause"].as_str()),
            (200, Some("unsatisfied"), Some("actor_known")),
            "{extra}: {said}"
        );
    }
    for extra in [
        json!({"principal": 7}),
        json!({"principal": "operator", "actor": {"id": "ana", "principal": "ana"}}),
    ] {
        let (status, said) = open(extra.clone());
        assert_eq!(status, 400, "{extra}: {said}");
    }
    let (status, opened) = open(json!({"principal": "operator"}));
    assert_eq!(
        (status, opened["kind"].as_str()),
        (200, Some("satisfied")),
        "{opened}"
    );
    let history = post(&s, "/v1/history", &json!({"id": opened["object_id"]}));
    assert_eq!(history["events"][0]["principal"], "operator", "{history}");
}

/// A batch through the wire applies whole or not at all (ADR-0228): every
/// verdict, in order, where all apply; where one is refused, its index and
/// its verdict, and nothing of the batch kept; and what a batch cannot hold
/// refused 400 before any of it runs, nothing written.
#[test]
fn a_batch_through_the_wire_applies_whole_or_not_at_all() {
    let path = fresh_store("batch");
    let s = start(&[
        "--store",
        path.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    publish_service(&s);
    let op = json!({"id": "operator"});
    let add = |login: &str| json!({"type": "User", "transition": "add", "inputs": {"login": login, "name": login}});
    let alone = |login: &str| {
        post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "User", "transition": "add",
                    "inputs": {"login": login, "name": login}}),
        )
    };
    let applied = post(
        &s,
        "/v1/batch",
        &json!({"actor": op, "requests": [add("ana"), add("bea")]}),
    );
    assert_eq!(applied["applied"], true, "{applied}");
    let verdicts = applied["verdicts"].as_array().expect("the verdicts");
    assert_eq!(verdicts.len(), 2, "{applied}");
    for v in verdicts {
        assert_eq!(v["kind"], "satisfied", "{v}");
    }
    // a login taken: the second refused, and the first not kept either
    let refused = post(
        &s,
        "/v1/batch",
        &json!({"actor": op, "requests": [add("cy"), add("ana")]}),
    );
    assert_eq!(
        (refused["applied"].as_bool(), refused["refused_at"].as_u64()),
        (Some(false), Some(1)),
        "{refused}"
    );
    assert_ne!(refused["verdict"]["kind"], "satisfied", "{refused}");
    assert_eq!(alone("cy")["kind"], "satisfied", "a refused batch kept cy");
    // one past the bound, refused before any runs; at the bound, applied
    let most = objectflow_core::request::BATCH_MOST;
    let many: Vec<Value> = (0..=most).map(|i| add(&format!("u{i}"))).collect();
    let full = post(
        &s,
        "/v1/batch",
        &json!({"actor": op, "requests": many[..most].to_vec()}),
    );
    assert_eq!(full["applied"], true, "a batch at its bound: {full}");
    let many: Vec<Value> = (0..=most).map(|i| add(&format!("w{i}"))).collect();
    for (body, says) in [
        (json!({"actor": op, "requests": []}), "at least one"),
        (
            json!({"actor": op, "requests": [add("dee"), {"type": "User", "transition": "add",
                    "inputs": {"login": "eve"}, "propose": true}]}),
            "request 1 of the batch takes no key propose",
        ),
        (json!({"actor": op, "requests": many}), "over_limit"),
    ] {
        let (status, said) = call(&s, "POST", "/v1/batch", Some(&body));
        assert_eq!(status, 400, "{said}");
        assert!(
            said["fault"].as_str().unwrap_or_default().contains(says),
            "{says}: {said}"
        );
    }
    assert_eq!(
        alone("dee")["kind"],
        "satisfied",
        "a batch refused 400 wrote"
    );
    // each event of a batch names it, by its first event's position
    let first = verdicts[0]["events"][0].clone();
    for v in verdicts {
        for p in v["events"].as_array().expect("its events") {
            let read = post(&s, "/v1/events", &json!({"positions": [p]}));
            assert_eq!(read["events"][0]["batch"], first, "{read}");
        }
    }
    // a batch's key replays it, and a different batch under it is refused
    let keyed = json!({"actor": op, "requests": [add("fay")], "idempotency_key": "batch-1"});
    let applied = post(&s, "/v1/batch", &keyed);
    assert_eq!(applied["applied"], true, "{applied}");
    let again = post(&s, "/v1/batch", &keyed);
    assert_eq!(again["replayed"], true, "{again}");
    assert_eq!(
        again["verdicts"][0]["object_id"], applied["verdicts"][0]["object_id"],
        "{again}"
    );
    assert_eq!(again["verdicts"][0]["replayed"], true, "{again}");
    let (status, said) = call(
        &s,
        "POST",
        "/v1/batch",
        Some(&json!({"actor": op, "requests": [add("gil")], "idempotency_key": "batch-1"})),
    );
    assert_eq!(
        (status, said["code"].as_str()),
        (409, Some("key_reused")),
        "{said}"
    );
}

/// A batch's check answers what the batch then answers, applied or
/// refused at the same request, and writes nothing, not an event nor an
/// attempt row; a satisfied verdict names nothing its run made, each said
/// not computed; and it takes no key (ADR-0228 decision 12).
#[test]
fn a_batchs_check_through_the_wire_answers_as_the_batch_and_writes_nothing() {
    let path = fresh_store("check-batch");
    let s = start(&[
        "--store",
        path.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    publish_service(&s);
    let op = json!({"id": "operator"});
    let add = |login: &str| json!({"type": "User", "transition": "add", "inputs": {"login": login, "name": login}});
    let held = |s: &Service| {
        let attempts = post(
            s,
            "/v1/export",
            &json!({"actor": op, "source": "User.attempts"}),
        );
        (exported_log(s), attempts["lines"].clone())
    };
    for (requests, applies, refused_at) in [
        (json!([add("ana"), add("bea")]), true, None),
        (json!([add("cy"), add("ana")]), false, Some(1)),
    ] {
        let before = held(&s);
        let checked = post(
            &s,
            "/v1/check_batch",
            &json!({"actor": op, "requests": requests}),
        );
        assert_eq!(held(&s), before, "a check wrote: {checked}");
        assert_eq!(checked["applies"], applies, "{checked}");
        assert_eq!(checked["refused_at"].as_u64(), refused_at, "{checked}");
        let ran = post(&s, "/v1/batch", &json!({"actor": op, "requests": requests}));
        assert_eq!(ran["applied"], applies, "{ran}");
        assert_eq!(ran["refused_at"].as_u64(), refused_at, "{ran}");
        if applies {
            let (said, done) = (&checked["verdicts"], &ran["verdicts"]);
            assert_eq!(said.as_array().map(Vec::len), Some(2), "{checked}");
            for (v, w) in said
                .as_array()
                .into_iter()
                .flatten()
                .zip(done.as_array().into_iter().flatten())
            {
                assert_eq!(v["kind"], w["kind"], "{checked} / {ran}");
                assert_eq!(
                    (&v["object_id"], &v["events"], &v["created"]),
                    (&Value::Null, &json!([]), &json!([])),
                    "{v}"
                );
                assert_eq!(
                    v["not_computed"],
                    json!(["events", "object_id", "created"]),
                    "{v}"
                );
            }
        } else {
            assert_eq!(
                checked["verdict"]["kind"], ran["verdict"]["kind"],
                "{checked} / {ran}"
            );
        }
    }
    // cy, which the refused check and batch named, was kept by neither
    let cy = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "User", "transition": "add", "inputs": {"login": "cy", "name": "cy"}}),
    );
    assert_eq!(cy["kind"], "satisfied", "{cy}");
    let (status, said) = call(
        &s,
        "POST",
        "/v1/check_batch",
        Some(&json!({"actor": op, "requests": [add("dee")], "idempotency_key": "k"})),
    );
    assert_eq!(status, 400, "{said}");
}

/// An idempotency key holds at most 256 bytes and a request's context 16
/// KiB, where a megabyte of each was stored as given (third production
/// review, minor).
#[test]
fn a_key_and_a_context_are_bounded() {
    let path = fresh_store("bounded");
    let s = start(&[
        "--store",
        path.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    publish_service(&s);
    let op = json!({"id": "operator"});
    let add = |extra: Value| {
        let mut body = json!({"actor": op, "type": "User", "transition": "add",
                              "inputs": {"login": format!("u{}", extra.to_string().len()), "name": "U"}});
        for (k, v) in extra.as_object().expect("keys") {
            body[k] = v.clone();
        }
        call(&s, "POST", "/v1/request", Some(&body))
    };
    assert_eq!(add(json!({"idempotency_key": "k".repeat(256)})).0, 200);
    let (status, said) = add(json!({"idempotency_key": "k".repeat(257)}));
    assert_eq!(status, 400, "{said}");
    assert_eq!(add(json!({"context": "c".repeat(16 * 1024)})).0, 200);
    let (status, said) = add(json!({"context": "c".repeat(16 * 1024 + 1)}));
    assert_eq!(status, 400, "{said}");
}

/// A value of another JSON type is refused on every key, never read as
/// absent: a numeric `filter` returned every object unfiltered, a numeric
/// `cursor` the first page again, a non-object `bind` the whole metric, and
/// a non-string `principal` was dropped (second production review, TYPES).
/// Each body is valid but for the one key.
#[test]
fn a_value_of_another_type_is_refused_on_every_key() {
    let path = fresh_store("typed-keys");
    let s = start(&[
        "--store",
        path.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    publish_service(&s);
    let op = json!({"id": "operator"});
    for (path, body) in [
        (
            "/v1/query",
            json!({"actor": op, "type": "ServiceJob", "filter": 1}),
        ),
        (
            "/v1/query",
            json!({"actor": op, "type": "ServiceJob", "cursor": 5}),
        ),
        ("/v1/query", json!({"actor": op, "type": 7})),
        (
            "/v1/metric",
            json!({"actor": op, "name": "ServiceJob.time_working", "bind": "x"}),
        ),
        (
            "/v1/request",
            json!({"actor": {"id": "operator", "principal": 9}, "type": "ServiceJob", "transition": "open"}),
        ),
        (
            "/v1/request",
            json!({"actor": {"id": "operator", "role": "admin"}, "type": "ServiceJob", "transition": "open"}),
        ),
        (
            "/v1/request",
            json!({"actor": "operator", "type": "ServiceJob", "transition": "open"}),
        ),
        (
            "/v1/request",
            json!({"actor": op, "object_id": 3, "type": "ServiceJob", "transition": "open"}),
        ),
        ("/v1/get", json!({"actor": op, "id": 4})),
        ("/v1/events", json!({"actor": op, "positions": 1})),
        (
            "/v1/publish",
            json!({"actor": op, "change": "C1", "expected_version": "1"}),
        ),
        (
            "/v1/maintain",
            json!({"actor": op, "task": "prune_attempts"}),
        ),
    ] {
        let (status, said) = call(&s, "POST", path, Some(&body));
        assert_eq!(status, 400, "{path} {body}: {said}");
    }
    // the same keys of their own types are taken
    let (status, said) = call(
        &s,
        "POST",
        "/v1/query",
        Some(&json!({"actor": op, "type": "ServiceJob", "filter": "state == ServiceJob.OPEN"})),
    );
    assert_eq!(status, 200, "{said}");
}

/// `maintain` names an actor the store knows, as a request does: an actor
/// it does not know prunes nothing (production review MAINTAIN), and a
/// known one's prune is refused only by the retention. Who may maintain is
/// the application's (DESIGN.md §10, ADR-0183 decision 5).
#[test]
fn maintain_names_an_actor_the_store_knows() {
    let path = fresh_store("maintain");
    let s = start(&[
        "--store",
        path.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let task = json!({"name": "prune_attempts", "before": "2020-01-01T00:00:00Z"});
    let alice = post(
        &s,
        "/v1/maintain",
        &json!({"actor": {"id": "alice"}, "task": task}),
    );
    assert_eq!(alice, json!({"moved": 0, "refused": "actor_known"}));
    let operator = post(
        &s,
        "/v1/maintain",
        &json!({"actor": {"id": "operator"}, "task": task}),
    );
    // a store started with no retention prunes nothing, which it says
    assert_eq!(operator["refused"], json!("no_retention"), "{operator}");
    drop(s);
    let s = start(&[
        "--store",
        path.to_str().expect("a path"),
        "--port",
        "0",
        "--attempt-retention",
        "30 days",
    ]);
    let now = call(&s, "GET", "/health", None).1["now"].clone();
    let inside = post(
        &s,
        "/v1/maintain",
        &json!({"actor": {"id": "operator"}, "task": {"name": "prune_attempts", "before": now}}),
    );
    assert_eq!(inside["refused"], json!("inside_retention"), "{inside}");
    let outside = post(
        &s,
        "/v1/maintain",
        &json!({"actor": {"id": "operator"}, "task": task}),
    );
    assert_eq!(outside["refused"], json!(null), "{outside}");
}

/// What the third production review's operator met at the edges of a
/// deployment: a start that cannot make its store or socket names the
/// path; a store whose file is damaged does not start, where a copy
/// restored beside another state's `-wal` answered health while its reads
/// failed (N5); an upgrade asked of a store with none due is still the
/// operator's (`409`), where anyone was answered nothing was due; and
/// `/metrics` gives the store's file and its write-ahead log (N6).
#[test]
fn a_deployments_edges_are_said() {
    let dir = fresh_store("edges");
    let dir = dir.parent().expect("its directory").to_path_buf();
    let missing = dir.join("no-such-directory").join("store.sqlite");
    let (code, said) = refused(&[
        "--store",
        missing.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    assert_eq!(code, Some(1));
    // the directory said not to exist, where --create was then suggested
    // (ninth production review, ADR-0196 decision 9)
    let parent = missing.parent().expect("its directory");
    assert!(
        said.contains(&format!(
            "the directory {} does not exist",
            parent.display()
        )),
        "{said}"
    );
    let (code, said) = refused(&["--store", missing.to_str().expect("a path"), "--port", "0"]);
    assert_eq!(code, Some(1));
    assert!(said.contains("does not exist"), "{said}");
    // how many reads run at once is a count, and a deployment's (ADR-0189)
    let (code, said) = refused(&["--max-reads", "0", "--port", "0"]);
    assert_eq!(code, Some(2));
    assert!(
        said.contains("--max-reads takes a count, 1 or more: 0"),
        "{said}"
    );
    // past the threads the runtime keeps for requests, where /health waited
    // behind 700 requests holding them (thirteenth production review N4)
    let (code, said) = refused(&["--max-connections", "5000", "--port", "0"]);
    assert_eq!(code, Some(2));
    assert!(
        said.contains("--max-connections takes a count from 1 to 4096: 5000"),
        "{said}"
    );
    let (code, said) = refused(&["--max-reads", "2", "--port", "0"]);
    assert_eq!(code, Some(2));
    assert!(
        said.contains("how many reads run at once is a deployment's"),
        "{said}"
    );
    // refused before anything is made, where --create made the store first
    // (fourteenth production review)
    let long = dir.join("s".repeat(150));
    let unmade = dir.join("unmade.sqlite");
    let (code, said) = refused(&[
        "--store",
        unmade.to_str().expect("a path"),
        "--create",
        "--socket",
        long.to_str().expect("a path"),
    ]);
    assert_eq!(code, Some(2));
    assert!(said.contains(&"s".repeat(150)), "{said}");
    assert!(
        said.contains("past the 107 a Unix socket's path may hold"),
        "{said}"
    );
    assert!(!unmade.exists(), "the store was made");
    // a store made, then damaged in its first table's page
    let path = dir.join("store.sqlite");
    {
        let s = start(&[
            "--store",
            path.to_str().expect("a path"),
            "--create",
            "--port",
            "0",
        ]);
        let op = json!({"id": "operator"});
        let (status, said) = call(
            &s,
            "POST",
            "/v1/upgrade",
            Some(&json!({"actor": {"id": "alice"}})),
        );
        assert_eq!(status, 409, "{said}");
        let upgraded = post(&s, "/v1/upgrade", &json!({"actor": op}));
        assert_eq!(upgraded["upgraded"], json!(false), "{upgraded}");
        // the operator in another ASCII case is the operator, as a request
        // names it, where upgrade refused it (ninth production review N-7,
        // ADR-0196 decision 8)
        let (status, upgraded) = call(
            &s,
            "POST",
            "/v1/upgrade",
            Some(&json!({"actor": {"id": "OPERATOR"}})),
        );
        assert_eq!(status, 200, "{upgraded}");
        let mut stream = TcpStream::connect(&s.addr).expect("a connection");
        write!(
            stream,
            "GET /metrics HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
        )
        .expect("asked");
        let mut metrics = String::new();
        stream.read_to_string(&mut metrics).expect("the metrics");
        let bytes = metrics
            .lines()
            .find_map(|l| l.strip_prefix("objectflow_store_bytes "))
            .and_then(|v| v.trim().parse::<u64>().ok());
        assert!(bytes.is_some_and(|b| b > 0), "{metrics}");
        assert!(metrics.contains("objectflow_wal_bytes "), "{metrics}");
        // and the volume they are on (ADR-0188 decision 3)
        assert!(metrics.contains("objectflow_disk_free_bytes "), "{metrics}");
        // the reads that grow with the store, none running, 4 at once
        // unless given (ADR-0189 decision 2)
        for pool in ["worklists", "analytics"] {
            for line in [
                format!("objectflow_reads_running{{pool=\"{pool}\"}} 0\n"),
                format!("objectflow_reads_waiting{{pool=\"{pool}\"}} 0\n"),
                format!("objectflow_reads_most{{pool=\"{pool}\"}} 4\n"),
            ] {
                assert!(metrics.contains(&line), "{line} in {metrics}");
            }
        }
        // a change's dry runs one at a time (ADR-0190)
        assert!(
            metrics.contains("objectflow_reads_most{pool=\"dry_runs\"} 1\n"),
            "{metrics}"
        );
        let (_, health) = call(&s, "GET", "/health", None);
        assert!(
            health["disk_free_bytes"].as_u64().is_some_and(|b| b > 0),
            "{health}"
        );
    }
    // the write-ahead log folded into the file first, so the damage is not
    // read past from the log
    {
        let conn = rusqlite::Connection::open(&path).expect("the store");
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .expect("a checkpoint");
    }
    {
        use std::io::{Seek, SeekFrom};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .expect("the store");
        // the second page's b-tree header, overwritten
        file.seek(SeekFrom::Start(4096)).expect("its second page");
        file.write_all(&[0xff; 64]).expect("damaged");
    }
    // and a write-ahead log beside it holding frames no close carried in, as
    // a copy restored beside another state's -wal has
    {
        let conn = rusqlite::Connection::open(&path).expect("the store");
        conn.set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )
        .expect("no checkpoint on close");
        conn.execute_batch(
            "PRAGMA wal_autocheckpoint = 0; CREATE TABLE later (x); INSERT INTO later VALUES (1);",
        )
        .expect("a frame in the log");
    }
    let wal = path.with_file_name("store.sqlite-wal");
    let (file_before, log_before) = (
        std::fs::read(&path).expect("the file"),
        std::fs::read(&wal).expect("the log"),
    );
    assert!(!log_before.is_empty(), "a log to keep");
    let (code, said) = refused(&["--store", path.to_str().expect("a path"), "--port", "0"]);
    assert_eq!(code, Some(1));
    assert!(said.contains("fails SQLite's quick check"), "{said}");
    // said on one stamped line, where SQLite's fault text broke it in two,
    // its second line unstamped, reported as the refusal printed twice
    // (ADR-0202 decision 8)
    let stamped = |l: &str| {
        let b = l.as_bytes();
        b.len() > 20 && b[4] == b'-' && b[10] == b'T' && b[19] == b'Z'
    };
    assert!(said.lines().all(stamped), "{said}");
    assert_eq!(
        said.lines().filter(|l| l.contains("quick check")).count(),
        1,
        "{said}"
    );
    // the refused start changed neither, where it wrote the log into the
    // file and emptied it (fourth and fifth production reviews)
    assert!(
        std::fs::read(&path).expect("the file") == file_before,
        "the refused start changed the file"
    );
    assert!(
        std::fs::read(&wal).expect("the log") == log_before,
        "the refused start changed the log"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A store holding actors whose identities are equal but for case, which
/// an earlier release took, is said at its start by the actors' object
/// ids, never their identities, with the remedy, where the start said
/// nothing (eighth production review CASE-PAIR, ADR-0195 decision 2).
#[test]
fn actors_equal_but_for_case_are_said_at_the_start() {
    let file = fresh_store("case-pair");
    let path = file.to_str().expect("a path");
    let (ana, other) = {
        let s = start(&["--store", path, "--create", "--port", "0"]);
        publish_service(&s);
        let op = json!({"id": "operator"});
        let add = |login: &str| {
            let v = post(
                &s,
                "/v1/request",
                &json!({"actor": op, "type": "User", "transition": "add",
                        "inputs": {"login": login, "name": "A person"}}),
            );
            v["object_id"].as_str().expect("a user").to_string()
        };
        (add("Ana.Lee@example.org"), add("bob@example.org"))
    };
    // as 0.2.0 took it: the second's identity the first's but for case
    rusqlite::Connection::open(&file)
        .and_then(|c| {
            c.execute(
                "UPDATE t_user SET login = 'ana.lee@EXAMPLE.org' WHERE id = ?1",
                [&other],
            )
        })
        .expect("the pair written");
    let s = start(&["--store", path, "--port", "0"]);
    let want = "whose identities are equal but for case";
    let said = s.said_within(want);
    assert!(
        said.contains(want) && said.contains(&format!("{ana} and {other}")),
        "{said}"
    );
    assert!(said.contains("identity_taken"), "{said}");
    assert!(
        !said.to_lowercase().contains("ana.lee@example.org"),
        "{said}"
    );
    drop(s);
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// The ninth review's start and health edges: a directory the service
/// cannot write in is said so, where SQLite said "attempt to write a
/// readonly database"; a token with a control character is refused; no
/// --attempt-retention is said at the start; and a store file that cannot be
/// read is /health's `503 store_unreadable`, where it was 500 internal
/// (ADR-0196 decision 9).
#[test]
fn the_ninth_reviews_start_and_health_edges_are_said() {
    use std::os::unix::fs::PermissionsExt;
    let file = fresh_store("edges9");
    let dir = file.parent().expect("its directory").to_path_buf();
    let path = file.to_str().expect("a path");
    {
        let s = start(&["--store", path, "--create", "--port", "0"]);
        publish_service(&s);
        let said = s.said_within("no --attempt-retention is given");
        assert!(said.contains("no --attempt-retention is given"), "{said}");
    }
    // a token with a line break inside it
    let token = dir.join("token");
    std::fs::write(&token, "abc\r\ndef\n").expect("the token");
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).expect("its mode");
    let (code, said) = refused(&[
        "--store",
        path,
        "--port",
        "0",
        "--evaluator",
        "bank=http://127.0.0.1:9/evaluate",
        "--evaluator-token",
        &format!("bank={}", token.display()),
    ]);
    assert_ne!(code, Some(0));
    assert!(said.contains("control character"), "{said}");
    // a directory the service cannot write in
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).expect("read only");
    let (code, said) = refused(&["--store", path, "--port", "0"]);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).expect("writable");
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains("cannot write in"), "{said}");
    // a store file that cannot be read, its content overwritten in place,
    // the log first folded into it, so no page is read from the log
    let s = start(&[
        "--store",
        path,
        "--port",
        "0",
        "--attempt-retention",
        "30 days",
    ]);
    assert!(!s.said().contains("no --attempt-retention"), "{}", s.said());
    // folded in and emptied, waited for: the service's own cut-back is due
    // at its start, the service before it killed with frames in its log, and
    // a checkpoint meeting another answers busy in its row, not as an error;
    // one ignored left the service's to write the store's pages back over
    // the zeroes below, page 1 among them, and /health found the store whole
    // (a run of CI, 2026-10-20)
    let conn = rusqlite::Connection::open(&file).expect("the store");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let (busy, frames): (i64, i64) = conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .expect("a checkpoint");
        if busy == 0 && frames == 0 {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the log never folded in: busy {busy}, {frames} frame(s)"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    drop(conn);
    {
        use std::io::{Seek, SeekFrom};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&file)
            .expect("the store");
        let len = f.metadata().expect("its size").len();
        f.seek(SeekFrom::Start(0)).expect("its start");
        f.write_all(&vec![0u8; usize::try_from(len).expect("a size")])
            .expect("its pages overwritten");
    }
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(
        (status, health["code"].as_str()),
        (503, Some("store_unreadable")),
        "{health}"
    );
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A backup restored beside the `-wal` of a later state is refused at the
/// start with the advice to restore the copy alone, whichever check finds
/// it, where a store whose schema the log moved was refused with SQLite's
/// words alone (eighth production review, ADR-0195 decision 7).
#[test]
fn a_backup_restored_beside_a_later_log_is_refused_with_the_advice() {
    let file = fresh_store("restored");
    let path = file.to_str().expect("a path");
    let backup = file.with_extension("backup");
    {
        let s = start(&["--store", path, "--create", "--port", "0"]);
        publish_service(&s);
    }
    std::fs::copy(&file, &backup).expect("the backup");
    // a later state's writes in the log a kill left
    {
        let s = start(&["--store", path, "--port", "0"]);
        let op = json!({"id": "operator"});
        for i in 0..200 {
            let v = post(
                &s,
                "/v1/request",
                &json!({"actor": op, "type": "User", "transition": "add",
                        "inputs": {"login": format!("u{i}"), "name": "A person"}}),
            );
            assert_eq!(v["kind"], "satisfied", "{v}");
        }
    }
    let wal = file.with_file_name("store.sqlite-wal");
    std::fs::copy(&backup, &file).expect("the backup restored");
    let (file_before, log_before) = (
        std::fs::read(&file).expect("the file"),
        std::fs::read(&wal).expect("the log"),
    );
    assert!(!log_before.is_empty(), "a later log beside it");
    let (code, said) = refused(&["--store", path, "--port", "0"]);
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains("restore the copy alone"), "{said}");
    assert!(
        std::fs::read(&file).expect("the file") == file_before,
        "the refused start changed the file"
    );
    assert!(
        std::fs::read(&wal).expect("the log") == log_before,
        "the refused start changed the log"
    );
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// A path that holds no store, absent or empty, is refused unless the start
/// asks to make one, so a mistyped path never becomes an empty store served
/// (ADR-0183 decision 3); a store made is its owner's alone, its `-wal`
/// and `-shm` as well; and it is opened again without asking.
#[test]
fn a_path_holding_no_store_is_refused_unless_made() {
    use std::os::unix::fs::PermissionsExt;
    let file = fresh_store("made");
    let path = file.to_str().expect("a path");
    let (code, said) = refused(&["--store", path, "--port", "0"]);
    assert_ne!(code, Some(0));
    assert!(
        said.contains("holds no store") && said.contains("--create"),
        "{said}"
    );
    assert!(!file.exists(), "a refused start made {}", file.display());
    std::fs::write(&file, b"").expect("an empty file");
    let (code, said) = refused(&["--store", path, "--port", "0"]);
    assert_ne!(code, Some(0));
    assert!(said.contains("holds no store"), "{said}");
    std::fs::remove_file(&file).expect("the empty file removed");
    // a database holding not a table, as a crash while --create made a store
    // leaves, its header written and nothing committed, is no store: refused
    // with the advice to create, and made one by --create, where every start
    // refused it as no ObjectFlow store (the audit's durability W11)
    {
        let crashed = rusqlite::Connection::open(&file).expect("a database");
        crashed
            .execute_batch("PRAGMA journal_mode = WAL;")
            .expect("its header");
    }
    assert!(std::fs::metadata(&file).expect("the file").len() > 0);
    let (code, said) = refused(&["--store", path, "--port", "0"]);
    assert_ne!(code, Some(0));
    assert!(
        said.contains("an empty database") && said.contains("--create"),
        "{said}"
    );
    {
        let s = start(&["--store", path, "--create", "--port", "0"]);
        let (status, health) = call(&s, "GET", "/health", None);
        assert_eq!(
            (status, health["shape"].as_str()),
            (200, Some("deployment")),
            "{health}"
        );
    }
    for end in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{path}{end}"));
    }
    // another application's database, or a file that is none, is never made
    // a store, asked or not, and the refusal is a sentence
    let other = rusqlite::Connection::open(&file).expect("a database");
    other
        .execute_batch(
            "CREATE TABLE invoices (id INTEGER PRIMARY KEY); INSERT INTO invoices VALUES (1);",
        )
        .expect("its table");
    drop(other);
    for args in [
        vec!["--store", path, "--port", "0"],
        vec!["--store", path, "--create", "--port", "0"],
    ] {
        let (code, said) = refused(&args);
        assert_ne!(code, Some(0));
        assert!(said.contains("not an ObjectFlow store"), "{said}");
        assert!(
            !said.contains("Custom {") && !said.contains("kind:"),
            "{said}"
        );
    }
    let tables: i64 = rusqlite::Connection::open(&file)
        .expect("the database")
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table'",
            [],
            |r| r.get(0),
        )
        .expect("read");
    assert_eq!(tables, 1, "the other application's database was changed");
    std::fs::write(&file, b"a text file, not a database").expect("a text file");
    let (code, said) = refused(&["--store", path, "--create", "--port", "0"]);
    assert_ne!(code, Some(0));
    assert!(said.contains("not an ObjectFlow store"), "{said}");
    std::fs::remove_file(&file).expect("the file removed");
    {
        let s = start(&["--store", path, "--create", "--port", "0"]);
        publish_service(&s);
        let said = s.said_within("a store made at");
        assert!(said.contains("a store made at"), "{said}");
        for suffix in ["", "-wal", "-shm"] {
            let f = format!("{path}{suffix}");
            let mode = std::fs::metadata(&f)
                .expect("the file")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{f}");
        }
    }
    let s = start(&["--store", path, "--port", "0"]);
    let (_, health) = call(&s, "GET", "/health", None);
    assert_eq!(health["version"].as_i64(), Some(1), "{health}");
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// One HTTP/1.1 exchange over a Unix socket: its status and its JSON body.
fn call_unix(socket: &Path, method: &str, path: &str) -> (u16, Value) {
    let mut stream = std::os::unix::net::UnixStream::connect(socket).expect("a connection");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: objectflow\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is written");
    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("the answer");
    let status = raw[9..12].parse().expect("a status");
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .unwrap_or_default();
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

/// The service on a Unix socket, the boundary the design leaves to the
/// deployment (ADR-0183 decision 4): its owner and group alone may connect,
/// a socket a stopped service left is replaced, and a file of another kind
/// is never.
#[test]
fn the_service_listens_on_a_unix_socket() {
    use std::os::unix::fs::PermissionsExt;
    let file = fresh_store("socket");
    let socket = file.with_file_name("objectflow.sock");
    let started = |args: &[&str]| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_objectflow-service"))
            .args(args)
            .stderr(Stdio::piped())
            .spawn()
            .expect("the service starts");
        let mut stderr = BufReader::new(child.stderr.take().expect("its stderr"));
        let mut line = String::new();
        stderr.read_line(&mut line).expect("its first line");
        assert!(
            line.trim()
                .ends_with(&format!(" listening on unix:{}", socket.display())),
            "{line}"
        );
        Service {
            child: Some(child),
            addr: String::new(),
            said: drained(stderr),
        }
    };
    let args = [
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--socket",
        socket.to_str().expect("a path"),
    ];
    {
        let _s = started(&args);
        let mode = std::fs::metadata(&socket)
            .expect("the socket")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o660);
        // a second service given the socket a live one answers on is refused,
        // never taking it over
        let second: Vec<&str> = args.iter().copied().filter(|a| *a != "--create").collect();
        let (code, said) = refused(&second);
        assert_ne!(code, Some(0));
        assert!(said.contains("still answering"), "{said}");
        let (status, health) = call_unix(&socket, "GET", "/health");
        assert_eq!(
            (status, health["shape"].as_str()),
            (200, Some("deployment")),
            "{health}"
        );
    }
    // killed, it leaves its socket, which the next start replaces
    assert!(socket.exists());
    {
        let _s = started(&args);
        assert_eq!(call_unix(&socket, "GET", "/health").0, 200);
    }
    std::fs::remove_file(&socket).expect("the socket removed");
    std::fs::write(&socket, b"not a socket").expect("a file");
    let (code, said) = refused(&args);
    assert_ne!(code, Some(0));
    assert!(said.contains("not a socket"), "{said}");
    let (code, said) = refused(&["--socket", "/tmp/x.sock", "--port", "0"]);
    assert_ne!(code, Some(0));
    assert!(said.contains("one or the other"), "{said}");
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// Each answer is logged, its time, route, status and duration, and counted
/// in `GET /metrics` by route, a path no route takes counted as one; and
/// `/health` reads the store, so a store that cannot be read answers 500
/// (production review OBSERVE).
#[test]
fn each_answer_is_logged_and_counted_and_health_reads_the_store() {
    let file = fresh_store("observed");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    publish_service(&s);
    for n in 0..3 {
        call(&s, "POST", &format!("/v1/nothing-{n}"), Some(&json!({})));
    }
    let (status, said) = call(&s, "POST", "/v1/request", Some(&json!({"inputs": 1})));
    assert_eq!(status, 400, "{said}");
    let log = s.said_within("POST /v1/request 400");
    let line = log
        .lines()
        .find(|l| l.contains("POST /v1/publish 200"))
        .unwrap_or_else(|| panic!("no publish logged: {log}"));
    assert!(
        line.starts_with("20") && line.contains('T') && line.ends_with(" ms"),
        "{line}"
    );
    assert!(log.contains("POST /v1/request 400"), "{log}");
    assert!(log.contains("POST unrouted 404"), "{log}");
    let mut stream = TcpStream::connect(&s.addr).expect("a connection");
    write!(
        stream,
        "GET /metrics HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    )
    .expect("asked");
    let mut metrics = String::new();
    stream.read_to_string(&mut metrics).expect("the metrics");
    for want in [
        "objectflow_requests_total{route=\"/v1/publish\",status=\"200\"} 1",
        "objectflow_requests_total{route=\"/v1/request\",status=\"400\"} 1",
        "objectflow_requests_total{route=\"unrouted\",status=\"404\"} 3",
        "objectflow_store_version 1",
        "objectflow_clock_lead_seconds 0",
    ] {
        assert!(metrics.contains(want), "{want} in {metrics}");
    }
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(
        (status, health["clock_lead_seconds"].as_i64()),
        (200, Some(0)),
        "{health}"
    );
    // the store's files gone from under the service, as an unmounted
    // volume's are: health says so, where a read connection, its pages
    // cached and the files it holds open still readable, would not; 503
    // store_replaced, as a write is answered (ADR-0191 decision 4)
    std::fs::remove_dir_all(file.parent().expect("its directory")).expect("the files removed");
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(
        (status, health["code"].as_str()),
        (503, Some("store_replaced")),
        "{health}"
    );
}

/// Asked to stop, as a supervisor asks by SIGTERM, the service takes no new
/// connection, answers the request in flight, and exits cleanly, saying so
/// (ADR-0183 decision 9).
#[test]
fn a_stopped_service_answers_the_requests_in_flight() {
    let file = fresh_store("stopping");
    let args = [
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ];
    stopped_in_flight(&args.map(str::to_string));
    // a clean stop leaves the store whole in its one file
    let wal = std::fs::metadata(format!("{}-wal", file.display())).map_or(0, |m| m.len());
    assert_eq!(wal, 0);
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// On PostgreSQL alike, and the stopped service's session closed, so a
/// service started next takes the store's lock at once (durability on
/// PostgreSQL 12, where no test stopped a service on PostgreSQL).
#[test]
fn a_stopped_service_on_postgresql_answers_the_requests_in_flight() {
    let Some((admin, args, schema)) = served_on_postgresql("stopping") else {
        return;
    };
    stopped_in_flight(&args);
    let again: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|a| *a != "--create")
        .collect();
    let s = start(&again);
    let (status, body) = call(&s, "GET", "/health", None);
    assert_eq!(status, 200, "{body}");
    drop(s);
    admin
        .execute_batch(&format!("DROP SCHEMA {schema} CASCADE"))
        .expect("the schema dropped");
}

/// A service started with `store`, stopped by SIGTERM with a slow request
/// in flight: it takes no new connection, answers the request and exits
/// cleanly, saying so.
fn stopped_in_flight(store: &[String]) {
    let (url, _, _) = bank();
    let named = format!("bank={url}");
    let mut args: Vec<&str> = store.iter().map(String::as_str).collect();
    args.extend(["--evaluator", &named, "--evaluator-timeout", "2 s"]);
    let mut s = start(&args);
    publish(&s, &json!({"payments": PAYMENTS}));
    // SIGHUP, as a log rotation sends it, is said and ignored, where it
    // ended the service at once (eleventh production review)
    let pid = s.child.as_ref().expect("its process").id();
    let hup = Command::new("kill")
        .args(["-HUP", &pid.to_string()])
        .status()
        .expect("kill runs");
    assert!(hup.success());
    let said = s.said_within("SIGHUP ignored");
    assert!(said.contains("SIGHUP ignored"), "{said}");
    assert!(TcpStream::connect(&s.addr).is_ok(), "{said}");
    // a settle the bank answers too slowly, held for the timeout's 2 s
    let flight = at(&s);
    let held = std::thread::spawn(move || settle(&flight, "T-4"));
    std::thread::sleep(std::time::Duration::from_millis(700));
    let pid = s.child.as_ref().expect("its process").id();
    let sent = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .expect("kill runs");
    assert!(sent.success());
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        TcpStream::connect(&s.addr).is_err(),
        "a connection was taken after the stop"
    );
    let v = held.join().expect("the request in flight");
    assert_eq!(
        (v["kind"].as_str(), v["remedy"].as_str()),
        (Some("unsatisfied"), Some("temporal")),
        "{v}"
    );
    let status = s
        .child
        .as_mut()
        .expect("its process")
        .wait()
        .expect("it exits");
    assert!(status.success(), "{status}");
    let said = s.said_within(" stopped");
    assert!(
        said.contains(" stopping: ") && said.contains(" stopped"),
        "{said}"
    );
}

/// An attribute a version marks `indexed` has an index on its column, made
/// by the publish, and a store that lacks it, as one a core before D593
/// made, has it made when the service opens it again, so a read by it, an
/// import's legacy key among them, never reads its whole table.
#[test]
fn an_indexed_attribute_s_index_is_made_where_the_store_lacks_it() {
    let file = fresh_store("indexed");
    let path = file.to_str().expect("a path");
    let module = "module: post\ncategories: [open, closed]\ntypes:\n  Parcel:\n    description: A parcel, found by its code.\n    tracking: record\n    attributes:\n      code: { type: string, indexed: true }\n    states:\n      OPEN: { category: open }\n      DONE: { category: closed, final: true }\n    transitions:\n      send: { kind: initial, to: OPEN, required_inputs: [code] }\n      deliver: { kind: external, from: OPEN, to: DONE }\n";
    let plain = module.replace("type: string, indexed: true", "type: string");
    let indexed = || {
        let conn = rusqlite::Connection::open(&file).expect("the file");
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = 't_parcel_code_ix'",
                [],
                |r| r.get(0),
            )
            .expect("read");
        n == 1
    };
    {
        let s = start(&["--store", path, "--create", "--port", "0"]);
        publish(&s, &json!({"post": plain}));
        assert!(!indexed(), "an index on an attribute not marked indexed");
        // a version marking it indexed, which changes nothing else of its table
        publish(&s, &json!({"post": module}));
    }
    assert!(indexed(), "the publish made no index");
    rusqlite::Connection::open(&file)
        .expect("the file")
        .execute_batch("DROP INDEX t_parcel_code_ix")
        .expect("dropped");
    assert!(!indexed());
    drop(start(&["--store", path, "--port", "0"]));
    assert!(indexed(), "the open made no index");
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// A change's submit runs its dry run on a read beside the writes, so no
/// write waits for it: a module of 3,000 types held every write 83 s
/// (DRYRUN-LOCK). The slowest write beside a large submit is a fraction of
/// the submit, where it was all of it.
#[test]
fn a_submits_dry_run_holds_no_write() {
    let file = fresh_store("submit-beside");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let note = "module: tally\ncategories: [live, closed]\ntypes:\n  Note:\n    description: A note.\n    tracking: record\n    attributes:\n      a: { type: string }\n    states:\n      OPEN: { category: live }\n      DONE: { category: closed, final: true }\n    transitions:\n      make: { kind: initial, to: OPEN, required_inputs: [a] }\n      finish: { kind: external, from: OPEN, to: DONE }\n";
    publish(&s, &json!({"tally": note}));
    let op = json!({"id": "operator"});
    // a module three times larger each time the submit ends within 500 ms,
    // too short to tell a write it held from one beside it, as an optimised
    // build ends 600 types' on a fast machine, where the test failed rather
    // than judge (the release of 0.3.0, on its runner)
    let mut types = 600;
    let (took, slowest, submitted) = loop {
        let mut big = String::from("module: big\ncategories: [live, closed]\ntypes:\n");
        for i in 0..types {
            big.push_str(&format!("  T{i}:\n    description: Type {i}.\n    tracking: record\n    attributes:\n      a: {{ type: string }}\n    states:\n      OPEN: {{ category: live }}\n      DONE: {{ category: closed, final: true }}\n    transitions:\n      make: {{ kind: initial, to: OPEN, required_inputs: [a] }}\n      finish: {{ kind: external, from: OPEN, to: DONE }}\n"));
        }
        let drafted = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "DeclarationChange", "transition": "draft",
            "inputs": {"source": {"tally": note, "big": big}, "source_ref": "test:big"}}),
        );
        let change = drafted["object_id"].as_str().expect("a change").to_string();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer = {
            let (w, stop) = (at(&s), stop.clone());
            std::thread::spawn(move || {
                let mut slowest = std::time::Duration::ZERO;
                while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                    let began = std::time::Instant::now();
                    post(
                        &w,
                        "/v1/request",
                        &json!({"actor": {"id": "operator"}, "type": "Note", "transition": "make", "inputs": {"a": "x"}}),
                    );
                    slowest = slowest.max(began.elapsed());
                }
                slowest
            })
        };
        std::thread::sleep(std::time::Duration::from_millis(300));
        let began = std::time::Instant::now();
        let submitted = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "object_id": change, "transition": "submit"}),
        );
        let took = began.elapsed();
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        let slowest = writer.join().expect("the writer");
        // at most 5,400 types, a module of about 1.8 MB, within the 2 MB a
        // request's body is given, where 16,200 types' 5.2 MB were refused
        if took > std::time::Duration::from_millis(500) || types >= 600 * 9 {
            break (took, slowest, submitted);
        }
        types *= 3;
    };
    assert_eq!(submitted["kind"], "satisfied", "{submitted}");
    assert!(
        took > std::time::Duration::from_millis(500),
        "the submit of {types} types took {took:?}, too little to tell"
    );
    assert!(
        slowest * 3 < took,
        "a write took {slowest:?} beside a submit of {took:?}"
    );
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// What a heavy request freed is given back to the system after it: glibc
/// kept it, so a service held 975 MB after one publish on 37k objects and
/// 23 MB once given back (third production review MEMORY-2, ADR-0187).
#[test]
fn what_a_heavy_request_freed_is_given_back() {
    let file = fresh_store("given-back");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let metric = |name: &str| -> u64 {
        let mut stream = TcpStream::connect(&s.addr).expect("a connection");
        write!(
            stream,
            "GET /metrics HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
        )
        .expect("asked");
        let mut out = String::new();
        stream.read_to_string(&mut out).expect("the metrics");
        out.lines()
            .find_map(|l| l.strip_prefix(&format!("{name} ")))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0)
    };
    let given_back = |at_least: u64| {
        let began = std::time::Instant::now();
        while metric("objectflow_memory_given_back_total") < at_least {
            assert!(
                began.elapsed() < std::time::Duration::from_secs(20),
                "nothing given back"
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    };
    let note = "module: tally\ncategories: [live, closed]\ntypes:\n  Note:\n    description: A note.\n    tracking: record\n    attributes:\n      a: { type: string }\n    states:\n      OPEN: { category: live }\n      DONE: { category: closed, final: true }\n    transitions:\n      make: { kind: initial, to: OPEN, required_inputs: [a] }\n      finish: { kind: external, from: OPEN, to: DONE }\n";
    publish(&s, &json!({"tally": note}));
    given_back(1);
    let before = metric("objectflow_resident_bytes");
    let mut big = String::from("module: big\ncategories: [live, closed]\ntypes:\n");
    for i in 0..600 {
        big.push_str(&format!("  T{i}:\n    description: Type {i}.\n    tracking: record\n    attributes:\n      a: {{ type: string }}\n    states:\n      OPEN: {{ category: live }}\n      DONE: {{ category: closed, final: true }}\n    transitions:\n      make: {{ kind: initial, to: OPEN, required_inputs: [a] }}\n      finish: {{ kind: external, from: OPEN, to: DONE }}\n"));
    }
    let op = json!({"id": "operator"});
    let drafted = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "DeclarationChange", "transition": "draft",
        "inputs": {"source": {"tally": note, "big": big}, "source_ref": "test:big"}}),
    );
    let change = drafted["object_id"].as_str().expect("a change").to_string();
    let submitted = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "object_id": change, "transition": "submit"}),
    );
    assert_eq!(submitted["kind"], "satisfied", "{submitted}");
    let approved = post(
        &s,
        "/v1/publish",
        &json!({"actor": op, "change": change, "expected_version": submitted["version"]}),
    );
    assert_eq!(approved["verdict"]["kind"], "satisfied", "{approved}");
    given_back(2);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let after = metric("objectflow_resident_bytes");
    eprintln!("resident before {before}, after {after}");
    assert!(
        after < before + 40 * 1024 * 1024,
        "resident {after} after, {before} before"
    );
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// An erasure beside a change's long dry run leaves the store's file and
/// its `-wal` within the log's cut, while the dry run still runs: its checks
/// hold no read of the store, where the erased value stayed in both for the
/// whole of a 38.7 s check (eighth production review ERASE-WAL-DRYRUN,
/// ADR-0195 decision 1).
#[test]
fn an_erasure_beside_a_long_dry_run_leaves_the_files() {
    use std::time::{Duration, Instant};
    let file = fresh_store("erase-dry");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let note = "module: n\ncategories: [live, closed]\ntypes:\n  Note:\n    description: A note.\n    tracking: record\n    attributes:\n      text: { type: string, personal: true, optional: true }\n    states:\n      OPEN: { category: live, description: Open. }\n      DONE: { category: closed, final: true, description: Done. }\n    transitions:\n      jot: { kind: initial, to: OPEN, optional_inputs: [text] }\n      done: { kind: external, from: OPEN, to: DONE }\n      forget: { kind: erasure, inputs: { reason: { type: string } } }\n";
    publish(&s, &json!({ "n": note }));
    let op = json!({"id": "operator"});
    let files = [file.clone(), file.with_extension("sqlite-wal")];
    // a change three times larger, and a note erased again, where its
    // check ends before the files are clean, too soon to tell a check that
    // held them from one beside them: a reader at the erasure leaves the
    // log to the next cut, up to 5 s later, and an optimised build ended
    // two thousand types' check in 1.04 s (the release of 0.6.0, on its
    // runner); six thousand, 1.76 MB, is the most a request's 2 MiB holds
    let mut types = 2000;
    loop {
        let marker = format!("ERASE-ME-{types}-{}", "Q".repeat(40));
        let jotted = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "Note", "transition": "jot", "inputs": {"text": marker}}),
        );
        let id = jotted["object_id"].as_str().expect("a note").to_string();
        let big: String = std::iter::once(note.to_string())
            .chain((0..types).map(|i| {
                format!(
                    "  T{i}:\n    description: A thing.\n    tracking: record\n    states:\n      ON: {{ category: live, description: On. }}\n      OFF: {{ category: closed, final: true, description: Off. }}\n    transitions:\n      make: {{ kind: initial, to: ON }}\n      stop: {{ kind: external, from: ON, to: OFF }}\n"
                )
            }))
            .collect();
        let drafted = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "DeclarationChange", "transition": "draft",
                    "inputs": {"source": {"n": big}, "source_ref": "test:big"}}),
        );
        let change = drafted["object_id"].as_str().expect("a change").to_string();
        let holds = |p: &PathBuf| {
            std::fs::read(p).is_ok_and(|b| b.windows(marker.len()).any(|w| w == marker.as_bytes()))
        };
        let (still_checking, took, gone) = std::thread::scope(|scope| {
            let checking = scope.spawn(|| {
                let began = Instant::now();
                let (status, _) = call(
                    &s,
                    "POST",
                    "/v1/check",
                    Some(&json!({"actor": op, "object_id": change, "transition": "submit"})),
                );
                (status, began.elapsed())
            });
            std::thread::sleep(Duration::from_secs(1));
            let erased = post(
                &s,
                "/v1/request",
                &json!({"actor": op, "object_id": id, "transition": "forget", "inputs": {"reason": "asked"}}),
            );
            assert_eq!(erased["kind"], "satisfied", "{erased}");
            let at = Instant::now();
            let mut gone = None;
            while at.elapsed() < Duration::from_secs(15) {
                if !files.iter().any(&holds) {
                    gone = Some(at.elapsed());
                    break;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            let still_checking = !checking.is_finished();
            let (status, took) = checking.join().expect("the check");
            assert_eq!(status, 200);
            let gone = gone.unwrap_or_else(|| {
                panic!(
                    "the erased value stayed in the store's files 15 s, the check taking {took:?}"
                )
            });
            (still_checking, took, gone)
        });
        eprintln!("{types} types: the check {took:?}, the files clean {gone:?} after the erasure");
        if still_checking || types >= 2000 * 3 {
            assert!(
                still_checking,
                "the check of {types} types, {took:?}, ended before the files were clean, {gone:?} after the erasure: it held them, or was too short to tell"
            );
            break;
        }
        types *= 3;
    }
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// A submit whose dry run passes its deadline, `--dry-run-timeout`, is
/// answered `503 read_timeout` and records nothing, where it was computed
/// again under the writer's lock, unbounded and unsaid, every write waiting
/// for it (third production review PUBLISH-SCALE, N1; ADR-0194 decision 2).
#[test]
fn a_submits_dry_run_past_the_deadline_is_answered_not_locked() {
    let file = fresh_store("submit-deadline");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
        "--dry-run-timeout",
        "1 s",
    ]);
    let note = |invariant: &str| {
        format!(
            "module: tally\ncategories: [live, closed]\ntypes:\n  Note:\n    description: A note.\n    tracking: record\n    attributes:\n      a: {{ type: int, indexed: true }}\n    states:\n      OPEN: {{ category: live }}\n      DONE: {{ category: closed, final: true }}\n{invariant}    transitions:\n      make: {{ kind: initial, to: OPEN, required_inputs: [a] }}\n      finish: {{ kind: external, from: OPEN, to: DONE }}\n"
        )
    };
    publish(&s, &json!({"tally": note("")}));
    let op = json!({"id": "operator"});
    for i in 0..150 {
        post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "Note", "transition": "make", "inputs": {"a": i}}),
        );
    }
    // an invariant whose cost over the notes is their cube
    let slow = note(
        "    invariants:\n      ranked:\n        description: Every note is ranked.\n        expression: \"count(x in Note where count(y in Note where y.a <= x.a) > 0) > 0\"\n",
    );
    let drafted = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "DeclarationChange", "transition": "draft",
        "inputs": {"source": {"tally": slow}, "source_ref": "test:slow"}}),
    );
    assert_eq!(drafted["kind"], "satisfied", "{drafted}");
    let change = drafted["object_id"].as_str().expect("a change").to_string();
    let began = std::time::Instant::now();
    let (status, said) = call(
        &s,
        "POST",
        "/v1/request",
        Some(&json!({"actor": op, "object_id": change, "transition": "submit"})),
    );
    let took = began.elapsed();
    assert_eq!(status, 503, "{said}");
    assert!(
        said["fault"]
            .as_str()
            .is_some_and(|f| f.starts_with("read_timeout: the change's dry run")),
        "{said}"
    );
    assert!(
        took < std::time::Duration::from_secs(5),
        "answered after {took:?}"
    );
    let got = post(&s, "/v1/get", &json!({"id": change}));
    assert_eq!(got["state"], "DRAFTED", "{got}");
    // a preview is a read, under the same deadline, beside the writes
    let began = std::time::Instant::now();
    let (status, said) = call(
        &s,
        "POST",
        "/v1/publish",
        Some(&json!({"actor": op, "change": change, "dry_run": true})),
    );
    let took = began.elapsed();
    assert_eq!(status, 503, "{said}");
    assert!(
        said["fault"]
            .as_str()
            .is_some_and(|f| f.starts_with("read_timeout: the change's dry run")),
        "{said}"
    );
    assert!(
        took < std::time::Duration::from_secs(5),
        "answered after {took:?}"
    );
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// A held writer and the requests in flight are seen in `/health` and
/// `/metrics` while an approval holds the writer, where a 21-minute stall
/// answered health 200 in 1 ms and showed nothing (OBSERVE-2); and both
/// return to nothing once it is done.
#[test]
fn a_held_writer_is_seen_in_health_and_metrics() {
    let file = fresh_store("held-writer");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let op = json!({"id": "operator"});
    let scrape = || {
        let mut stream = TcpStream::connect(&s.addr).expect("a connection");
        write!(
            stream,
            "GET /metrics HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
        )
        .expect("asked");
        let mut metrics = String::new();
        stream.read_to_string(&mut metrics).expect("the metrics");
        metrics
    };
    let held_in = |metrics: &str| {
        metrics
            .lines()
            .find_map(|l| l.strip_prefix("objectflow_writer_held_seconds "))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    // the approval watched while it runs, where one look after a fixed
    // 600 ms found an optimised build's publish of 600 types already done
    // on the release's runner, and the test failed rather than judge (the
    // release of 0.4.0); a module three times larger each time the
    // approval ends before both reads have seen the writer held
    let mut types = 600;
    let (health, metrics) = loop {
        let mut big = String::from("module: big\ncategories: [live, closed]\ntypes:\n");
        for i in 0..types {
            big.push_str(&format!("  T{i}:\n    description: Type {i}.\n    tracking: record\n    attributes:\n      a: {{ type: string }}\n    states:\n      OPEN: {{ category: live }}\n      DONE: {{ category: closed, final: true }}\n    transitions:\n      make: {{ kind: initial, to: OPEN, required_inputs: [a] }}\n      finish: {{ kind: external, from: OPEN, to: DONE }}\n"));
        }
        let drafted = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "DeclarationChange", "transition": "draft",
            "inputs": {"source": {"big": big}, "source_ref": "test:big"}}),
        );
        let change = drafted["object_id"].as_str().expect("a change").to_string();
        let submitted = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "object_id": change, "transition": "submit"}),
        );
        assert_eq!(submitted["kind"], "satisfied", "{submitted}");
        // the approval computes its dry run again under the writer
        let approving = {
            let (w, change, version) = (at(&s), change.clone(), submitted["version"].clone());
            std::thread::spawn(move || {
                post(
                    &w,
                    "/v1/publish",
                    &json!({"actor": {"id": "operator"}, "change": change, "expected_version": version}),
                )
            })
        };
        let (mut health, mut metrics) = (None, None);
        while !approving.is_finished() && (health.is_none() || metrics.is_none()) {
            let (_, now) = call(&s, "GET", "/health", None);
            if now["writer_held_seconds"].as_f64().unwrap_or(0.0) > 0.2
                && now["requests_in_flight"].as_u64().unwrap_or(0) >= 2
            {
                health.get_or_insert(now);
            }
            let scraped = scrape();
            if held_in(&scraped) > 0.2 {
                metrics.get_or_insert(scraped);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let approved = approving.join().expect("the approval");
        assert_eq!(approved["verdict"]["kind"], "satisfied", "{approved}");
        match (health, metrics) {
            (Some(health), Some(metrics)) => break (health, metrics),
            // at most 5,400 types, within a request body's 2 MB, as above
            _ if types >= 600 * 9 => {
                panic!("an approval of {types} types was never seen holding the writer")
            }
            _ => types *= 3,
        }
    };
    assert!(
        health["writer_held_seconds"].as_f64().unwrap_or(0.0) > 0.2,
        "{health}"
    );
    assert!(held_in(&metrics) > 0.2, "{metrics}");
    assert!(metrics.contains("objectflow_resident_bytes "), "{metrics}");
    let (_, after) = call(&s, "GET", "/health", None);
    assert_eq!(after["writer_held_seconds"].as_f64(), Some(0.0), "{after}");
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// A read that passes its deadline is stopped there and answered 503
/// `read_timeout`, the service answering on, where a filter of nested
/// sums ran past 186 s, holding a reader and an erasure's truncation behind
/// it (ADR-0184 decision 3); and `events` takes a page of positions at most.
#[test]
fn a_read_past_its_deadline_is_stopped() {
    let file = fresh_store("read-deadline");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
        "--read-timeout",
        "1 s",
    ]);
    let note = "module: tally\ncategories: [live, closed]\ntypes:\n  Note:\n    description: A note.\n    tracking: record\n    attributes:\n      a: { type: int, indexed: true }\n    states:\n      OPEN: { category: live }\n      DONE: { category: closed, final: true }\n    transitions:\n      make: { kind: initial, to: OPEN, required_inputs: [a] }\n      finish: { kind: external, from: OPEN, to: DONE }\n";
    publish(&s, &json!({"tally": note}));
    let op = json!({"id": "operator"});
    for i in 0..150 {
        post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "Note", "transition": "make", "inputs": {"a": i}}),
        );
    }
    // a filter whose cost is the cube of the notes
    let began = std::time::Instant::now();
    let (status, said) = call(
        &s,
        "POST",
        "/v1/query",
        Some(&json!({"actor": op, "type": "Note",
        "filter": "count(x in Note where count(y in Note where y.a <= x.a) > 0) > 0"})),
    );
    let took = began.elapsed();
    assert_eq!(status, 503, "{said}");
    assert!(
        said["fault"]
            .as_str()
            .is_some_and(|f| f.starts_with("read_timeout")),
        "{said}"
    );
    assert!(
        took < std::time::Duration::from_secs(5),
        "stopped after {took:?}"
    );
    let (status, _) = call(&s, "GET", "/health", None);
    assert_eq!(status, 200);
    let positions: Vec<i64> = vec![1; 1001];
    let (status, said) = call(
        &s,
        "POST",
        "/v1/events",
        Some(&json!({"actor": op, "positions": positions})),
    );
    assert_eq!(status, 400, "{said}");
    let (code, said) = refused(&["--port", "0", "--read-timeout", "1 s"]);
    assert_ne!(code, Some(0));
    assert!(said.contains("deployment's"), "{said}");
    let (code, said) = refused(&["--port", "0", "--dry-run-timeout", "2 min"]);
    assert_ne!(code, Some(0));
    assert!(said.contains("deployment's"), "{said}");
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// A connection whose request's head is not read within the idle timeout,
/// silent or half-sent, is closed, and a request whose body does not arrive
/// in it is answered 408, where 700 such connections were open after 70 s;
/// past the cap a connection is closed at once and said (ADR-0184 decision
/// 4).
#[test]
fn idle_and_half_sent_connections_are_closed() {
    let s = start(&[
        "--port",
        "0",
        "--idle-timeout",
        "1 s",
        "--max-connections",
        "3",
    ]);
    // how long until the service closes a connection, reading what it sends
    let closes = |mut c: TcpStream| {
        c.set_read_timeout(Some(std::time::Duration::from_secs(6)))
            .expect("a read timeout");
        let began = std::time::Instant::now();
        let mut got = Vec::new();
        let _ = c.read_to_end(&mut got);
        (began.elapsed(), String::from_utf8_lossy(&got).to_string())
    };
    // three silent connections, the cap; a fourth makes room, the oldest
    // silent one closed at once (third production review C-1)
    let mut silent: Vec<TcpStream> = (0..3)
        .map(|_| TcpStream::connect(&s.addr).expect("a connection"))
        .collect();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let fourth = TcpStream::connect(&s.addr).expect("a connection");
    let (took, _) = closes(silent.remove(0));
    assert!(
        took < std::time::Duration::from_millis(900),
        "the oldest silent one open {took:?}"
    );
    silent.push(fourth);
    // the silent ones are closed after the idle timeout
    for c in silent {
        let (took, _) = closes(c);
        assert!(
            took < std::time::Duration::from_secs(4),
            "a silent connection open {took:?}"
        );
    }
    let (status, _) = call(&s, "GET", "/health", None);
    assert_eq!(status, 200);
    // a request's head half-sent
    let mut head = TcpStream::connect(&s.addr).expect("a connection");
    write!(head, "GET /health HTTP/1.1\r\nHost: x\r\n").expect("half a head");
    let (took, _) = closes(head);
    assert!(
        took < std::time::Duration::from_secs(4),
        "a half-sent head open {took:?}"
    );
    // a request's body half-sent
    let mut body = TcpStream::connect(&s.addr).expect("a connection");
    write!(
        body,
        "POST /v1/get HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{{\"actor\":"
    )
    .expect("half a body");
    let (took, said) = closes(body);
    assert!(said.starts_with("HTTP/1.1 408"), "{said}");
    assert!(
        took < std::time::Duration::from_secs(4),
        "a half-sent body open {took:?}"
    );
    let said = s.said_within("a connection answering nothing closed to make room: 3 are open");
    assert!(
        said.contains("a connection answering nothing closed to make room: 3 are open"),
        "{said}"
    );
}

/// At the cap, connections answering nothing, a leaking pool's or silent
/// sockets', are closed to make room, so the application and `/health` are
/// answered, where every newcomer was reset; only when every connection is
/// answering a request is a newcomer refused, told `503 busy` rather than
/// reset, and `/metrics` counts both (third production review C-1,
/// ADR-0186).
///
/// Each step waits on what the service says of its connections, read on
/// its metrics listener, which has a cap of its own, where sleeps of 200
/// and 300 ms let a slow runner's request at the cap find a connection not
/// yet answering, and be answered 200 (run 38999367710).
#[test]
fn a_connection_at_the_cap_makes_room_or_is_told_busy() {
    let s = start(&[
        "--port",
        "0",
        "--idle-timeout",
        "2 s",
        "--max-connections",
        "3",
        "--metrics-bind",
        "127.0.0.1:0",
    ]);
    let said = s.said_within("metrics and health on http://");
    let watch = Service {
        child: None,
        addr: said
            .lines()
            .find_map(|l| {
                l.split_once("metrics and health on http://")
                    .map(|(_, a)| a.trim().to_string())
            })
            .expect("the metrics address"),
        said: std::sync::Arc::new(std::sync::Mutex::new(String::new())),
    };
    // what /health on the metrics listener says, its own connection and
    // request counted in it
    let seen = |key: &str| -> u64 {
        call(&watch, "GET", "/health", None).1[key]
            .as_u64()
            .unwrap_or(0)
    };
    let until = |what: &str, ready: &dyn Fn() -> bool| {
        let began = std::time::Instant::now();
        while !ready() {
            assert!(
                began.elapsed() < std::time::Duration::from_secs(20),
                "never {what}"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };
    let (open, flying) = (seen("connections_open"), seen("requests_in_flight"));
    let silent: Vec<TcpStream> = (0..3)
        .map(|_| TcpStream::connect(&s.addr).expect("a connection"))
        .collect();
    until("the three silent connections taken", &|| {
        seen("connections_open") >= open + 3
    });
    for _ in 0..3 {
        let (status, health) = call(&s, "GET", "/health", None);
        assert_eq!(status, 200, "{health}");
    }
    drop(silent);
    // three requests whose bodies are half-sent, each answering
    let answering: Vec<TcpStream> = (0..3)
        .map(|_| {
            let mut c = TcpStream::connect(&s.addr).expect("a connection");
            write!(
                c,
                "POST /v1/get HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{{\"actor\":"
            )
            .expect("half a body");
            c
        })
        .collect();
    until("the three requests answering", &|| {
        seen("requests_in_flight") >= flying + 3
    });
    let mut told = TcpStream::connect(&s.addr).expect("a connection");
    told.set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .expect("a read timeout");
    write!(told, "GET /health HTTP/1.1\r\nHost: x\r\n\r\n").expect("a request");
    let mut said = Vec::new();
    let _ = told.read_to_end(&mut said);
    let said = String::from_utf8_lossy(&said).to_string();
    assert!(said.starts_with("HTTP/1.1 503"), "{said}");
    assert!(said.contains("busy: every connection"), "{said}");
    // the half-sent bodies time out, and the service answers again
    drop(answering);
    until("the three requests ended", &|| {
        seen("requests_in_flight") <= flying && seen("connections_open") <= open
    });
    let mut stream = TcpStream::connect(&s.addr).expect("a connection");
    write!(
        stream,
        "GET /metrics HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    )
    .expect("asked");
    let mut metrics = String::new();
    stream.read_to_string(&mut metrics).expect("the metrics");
    assert!(metrics.starts_with("HTTP/1.1 200"), "{metrics}");
    let count = |name: &str| -> u64 {
        metrics
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{name} ")))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0)
    };
    assert!(
        // the first /health made room; it closed, so the others had it
        count("objectflow_connections_made_room_total") >= 1,
        "{metrics}"
    );
    assert!(
        count("objectflow_connections_refused_total") >= 1,
        "{metrics}"
    );
}

/// `--version` names the service's release and the stores it serves.
#[test]
fn the_service_says_its_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_objectflow-service"))
        .arg("--version")
        .output()
        .expect("it runs");
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{said}");
    assert!(
        said.starts_with(&format!("objectflow-service {}", env!("CARGO_PKG_VERSION"))),
        "{said}"
    );
}

#[test]
fn a_retention_without_a_store_is_refused() {
    let (code, said) = refused(&["--attempt-retention", "90 days"]);
    assert_eq!(code, Some(2), "{said}");
}

/// A bank whose transfers clear by their reference: `T-1` cleared, `T-2` not,
/// `T-3` a fault, `T-4` an answer slower than the service waits, `T-5` an
/// answer with no verdict, and `T-7` a redirect to a path that clears
/// anything. Each call it is posted is kept, as it arrives.
#[allow(clippy::type_complexity)]
fn bank() -> (
    String,
    std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
    std::sync::Arc<std::sync::Mutex<Vec<Option<String>>>>,
) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let url = format!(
        "http://{}/evaluate",
        listener.local_addr().expect("its address")
    );
    let elsewhere = url.replace("/evaluate", "/elsewhere");
    let posted = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let kept = posted.clone();
    // each call's Authorization header, none where it carried none
    let authorized = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let heard = authorized.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let kept = kept.clone();
            let heard = heard.clone();
            let elsewhere = elsewhere.clone();
            // each call answered on its own thread, so a slow answer holds
            // up no other
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().expect("the stream"));
                let mut length = 0;
                let mut authorization = None;
                let mut asked = String::new();
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if asked.is_empty() {
                        asked = line.clone();
                    }
                    if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = n.trim().parse().unwrap_or(0);
                    }
                    if line.to_ascii_lowercase().starts_with("authorization:") {
                        authorization = line.split_once(':').map(|(_, v)| v.trim().to_string());
                    }
                }
                heard.lock().expect("the headers").push(authorization);
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                let call: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                kept.lock().expect("the calls").push(call.clone());
                // the place a redirect names, which clears anything
                if asked.contains("/elsewhere") {
                    let answer = r#"{"verdict": "satisfied"}"#;
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
                        answer.len()
                    );
                    return;
                }
                if call["arguments"][0].as_str() == Some("T-7") {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 302 X\r\nLocation: {elsewhere}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                    return;
                }
                let (status, answer) = match call["arguments"][0].as_str() {
                    Some("T-1") => (200, r#"{"verdict": "satisfied"}"#),
                    // a person, by an email, known
                    Some(a) if a.contains('@') => (200, r#"{"verdict": "satisfied"}"#),
                    Some("T-2") => (200, r#"{"verdict": "unsatisfied"}"#),
                    Some("T-4") => {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        (200, r#"{"verdict": "satisfied"}"#)
                    }
                    Some("T-5") => (200, r#"{"cleared": true}"#),
                    // a verdict named twice, the last a pass (ADR-0192)
                    Some("T-6") => (200, r#"{"verdict": "unsatisfied", "verdict": "satisfied"}"#),
                    // a fault, whatever its body says
                    _ => (500, r#"{"verdict": "satisfied"}"#),
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
                    answer.len()
                );
            });
        }
    });
    (url, posted, authorized)
}

const PAYMENTS: &str = "module: payments
categories: [live, closed]

evaluators:
  bank:
    description: The bank, which says whether a transfer has cleared.
    functions:
      cleared:
        description: Whether the transfer with this reference has cleared.
        arguments:
          reference: { type: string }

types:
  Transfer:
    description: A transfer sent to the bank, settled once the bank clears it.
    tracking: record
    attributes:
      reference: { type: string }
    states:
      SENT:    { description: Sent to the bank., category: live }
      SETTLED: { description: Cleared by the bank., category: closed, final: true }
    conditions:
      cleared:
        description: The bank has cleared the transfer.
        expression: bank.cleared(reference)
        remedy: dependent
    transitions:
      send: { kind: initial, to: SENT, required_inputs: [reference] }
      settle:
        kind: external
        from: SENT
        to: SETTLED
        guards:
          cleared: deny
";

/// An evaluator given a bearer token is sent it with every call, never
/// said, the token read from a file its owner alone reads; a file others
/// may read, an empty one, or a token for no evaluator, is refused at start
/// (third production review, ADR-0188 decision 2).
#[test]
fn an_evaluator_is_sent_its_token() {
    use std::os::unix::fs::PermissionsExt;
    let (url, _, authorized) = bank();
    let file = fresh_store("token");
    let dir = file.parent().expect("its directory").to_path_buf();
    let secret = dir.join("bank.token");
    std::fs::write(&secret, "s3cret-token\n").expect("the token");
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).expect("0600");
    let named = format!("bank={url}");
    let token = format!("bank={}", secret.display());
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
        "--evaluator",
        &named,
        "--evaluator-token",
        &token,
    ]);
    publish(&s, &json!({"payments": PAYMENTS}));
    let v = settle(&s, "T-1");
    assert_eq!(v["kind"], "satisfied", "{v}");
    assert_eq!(
        authorized.lock().expect("the headers").last().cloned(),
        Some(Some("Bearer s3cret-token".to_string()))
    );
    let said = s.said_within("the evaluator bank asked at");
    assert!(said.contains("with its token"), "{said}");
    assert!(!said.contains("s3cret"), "{said}");
    drop(s);
    // a file others may read, an empty one, a token for no evaluator
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o644)).expect("0644");
    let path = file.to_str().expect("a path");
    let (code, said) = refused(&[
        "--store",
        path,
        "--port",
        "0",
        "--evaluator",
        &named,
        "--evaluator-token",
        &token,
    ]);
    assert_eq!(code, Some(2), "{said}");
    assert!(said.contains("readable by others"), "{said}");
    std::fs::write(&secret, "  \n").expect("emptied");
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).expect("0600");
    let (code, said) = refused(&[
        "--store",
        path,
        "--port",
        "0",
        "--evaluator",
        &named,
        "--evaluator-token",
        &token,
    ]);
    assert_eq!(code, Some(2), "{said}");
    assert!(said.contains("holds no token"), "{said}");
    std::fs::write(&secret, "s3cret-token\n").expect("the token again");
    let other = format!("ledger={}", secret.display());
    let (code, said) = refused(&[
        "--store",
        path,
        "--port",
        "0",
        "--evaluator",
        &named,
        "--evaluator-token",
        &other,
    ]);
    assert_eq!(code, Some(2), "{said}");
    assert!(said.contains("which no --evaluator gives"), "{said}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A transfer sent, and settling it requested, the bank asked by the service.
fn settle(s: &Service, reference: &str) -> Value {
    let op = json!({"id": "operator"});
    let sent = post(
        s,
        "/v1/request",
        &json!({"actor": op, "type": "Transfer", "transition": "send", "inputs": {"reference": reference}}),
    );
    assert_eq!(sent["kind"], "satisfied", "{sent}");
    post(
        s,
        "/v1/request",
        &json!({"actor": op, "object_id": sent["object_id"], "transition": "settle"}),
    )
}

#[test]
fn a_deployment_asks_its_evaluators_at_their_endpoints() {
    let (url, posted, authorized) = bank();
    let file = fresh_store("evaluators");
    let path = file.to_str().expect("a path");
    let named = format!("bank={url}");
    let s = start(&[
        "--store",
        path,
        "--create",
        "--port",
        "0",
        "--evaluator",
        &named,
        "--evaluator-timeout",
        "1 s",
    ]);
    publish(&s, &json!({"payments": PAYMENTS}));
    // cleared: the transfer settles, the call posted with its argument
    let v = settle(&s, "T-1");
    assert_eq!(v["kind"], "satisfied", "{v}");
    // the verdict on the record, by guard, with its as-of time (D562)
    let events = post(&s, "/v1/events", &json!({"positions": v["events"]}));
    let settled = &events["events"][0];
    assert_eq!(
        settled["consulted"],
        json!({"cleared": {"bank.cleared(T-1)": "satisfied"}}),
        "{settled}"
    );
    assert!(settled["as_of"]["cleared"].is_string(), "{settled}");
    assert_eq!(
        posted.lock().expect("the calls").last().cloned(),
        Some(json!({"evaluator": "bank", "function": "cleared", "arguments": ["T-1"]}))
    );
    // given no token, it sends no Authorization header (ADR-0188)
    assert_eq!(
        authorized.lock().expect("the headers").last().cloned(),
        Some(None)
    );
    // not cleared: refused by the guard, with the remedy it declares
    let v = settle(&s, "T-2");
    assert_eq!(
        (
            v["kind"].as_str(),
            v["clause"].as_str(),
            v["remedy"].as_str()
        ),
        (Some("unsatisfied"), Some("cleared"), Some("dependent")),
        "{v}"
    );
    // a fault, an answer too slow, an answer with no verdict and one naming
    // its verdict twice are the evaluator's unknown: refused, and the caller
    // told to wait
    // a redirect followed by none, to a place the operator never named
    // (eighth production review EV-REDIRECT, ADR-0195 decision 3)
    for reference in ["T-3", "T-4", "T-5", "T-6", "T-7"] {
        let v = settle(&s, reference);
        assert_eq!(
            (v["kind"].as_str(), v["remedy"].as_str()),
            (Some("unsatisfied"), Some("temporal")),
            "{reference}: {v}"
        );
    }
    // each said on stderr by its function, never its arguments or the
    // endpoint's answer, which may hold personal data (production review)
    let said = s.said_within("bank.cleared of 1 argument(s) is unknown");
    assert!(
        said.contains("bank.cleared of 1 argument(s) is unknown"),
        "{said}"
    );
    for kept in ["T-3", "T-4", "T-5", "cleared\": true"] {
        assert!(!said.contains(kept), "{kept} in {said}");
    }
    // a slow answer named by the flag that bounds it, where it said
    // "timeout: global" (ninth production review)
    let said = s.said_within("did not answer within --evaluator-timeout");
    assert!(
        said.contains("did not answer within --evaluator-timeout"),
        "{said}"
    );
    // while an endpoint is slow, every other request is served (D563)
    let addr = s.addr.clone();
    let slow = std::thread::spawn(move || {
        let s = Service {
            child: None,
            addr,
            said: Default::default(),
        };
        settle(&s, "T-4")
    });
    std::thread::sleep(std::time::Duration::from_millis(300));
    let began = std::time::Instant::now();
    let (status, _) = call(&s, "GET", "/health", None);
    let waited = began.elapsed();
    assert_eq!(status, 200);
    assert!(
        waited < std::time::Duration::from_millis(500),
        "a read waited {waited:?} behind a slow endpoint"
    );
    let v = slow.join().expect("the slow settle");
    assert_eq!(v["remedy"], "temporal", "{v}");
    // a verdict is the request's alone: asked again by the next
    let before = posted.lock().expect("the calls").len();
    let v = settle(&s, "T-1");
    assert_eq!(v["kind"], "satisfied", "{v}");
    assert_eq!(posted.lock().expect("the calls").len(), before + 1);
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

#[test]
fn an_evaluator_no_endpoint_answers_is_unknown() {
    let file = fresh_store("unanswered");
    let path = file.to_str().expect("a path");
    let s = start(&["--store", path, "--create", "--port", "0"]);
    publish(&s, &json!({"payments": PAYMENTS}));
    let v = settle(&s, "T-1");
    assert_eq!(
        (v["kind"].as_str(), v["remedy"].as_str()),
        (Some("unsatisfied"), Some("temporal")),
        "{v}"
    );
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

#[test]
fn an_endpoint_is_a_deployments() {
    let (code, said) = refused(&["--evaluator", "bank=http://127.0.0.1:1/x"]);
    assert_eq!(code, Some(2), "{said}");
    let (code, said) = refused(&["--store", "/nonexistent/x.sqlite", "--evaluator", "bank"]);
    assert_eq!(code, Some(2), "{said}");
}

/// The releases whose own service made stores kept as fixtures, each with
/// what it answered of them (`fixtures/make-release-store.py`: 0.1.0's at
/// commit f6ee6d6, 0.2.0's at 37e8825, 0.3.0's, 0.4.0's, 0.5.0's, 0.5.1's and
/// 0.5.2's by their published services, built at 77bf468, 9c6cd26, ccc88de,
/// 4048a90 and a04acbd).
const FIXTURE_RELEASES: &[&str] = &[
    "0.1.0", "0.2.0", "0.3.0", "0.4.0", "0.5.0", "0.5.1", "0.5.2", "0.5.3",
];

/// A store a release's own service made, and what it answered of it. Each
/// test copies into a store of its own, `tag` naming it, since tests run at
/// once in one process and share its id.
fn release_fixture(release: &str, name: &str, tag: &str) -> (PathBuf, Value) {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/release-{release}"));
    let path = fresh_store(&format!("upgrade-{release}-{name}-{tag}"));
    std::fs::copy(dir.join(format!("{name}.sqlite")), &path).expect("the fixture copied");
    let reads = std::fs::read_to_string(dir.join(format!("{name}.reads.json")))
        .expect("what the release answered");
    (path, serde_json::from_str(&reads).expect("its reads"))
}

/// The exported log, every page of it.
fn exported_log(s: &Service) -> Vec<Value> {
    let (mut lines, mut cursor) = (Vec::new(), Value::Null);
    loop {
        let page = post(
            s,
            "/v1/export",
            &json!({"actor": {"id": "operator"}, "source": "log", "cursor": cursor}),
        );
        lines.extend(page["lines"].as_array().cloned().unwrap_or_default());
        if page["cursor"].is_null() || page["cursor"] == cursor {
            return lines;
        }
        cursor = page["cursor"].clone();
    }
}

/// A store each earlier release made is served only the upgrade until it
/// runs; a dry run changes nothing; and once upgraded, through each release
/// between, every object reads as that release read it, its history whole,
/// the log the same with the operator's `upgrade` for each step after it,
/// and a restart opens the store at this release (ADR-0177; the audit's
/// durability U8, every fixture where 0.1.0's alone was read).
#[test]
fn a_store_an_earlier_release_made_is_upgraded_and_reads_alike() {
    let op = json!({"id": "operator"});
    // a store this release creates, to compare the upgraded ones with
    let created = fresh_store("upgrade-created");
    drop(start(&[
        "--store",
        created.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]));
    let schema = own_schema(&created);
    for table in ["of_restore", "of_interval_retraction", "of_event"] {
        assert!(schema.contains(&format!("table {table}")), "{schema:?}");
    }
    assert!(
        schema.iter().any(|l| l.contains("moved_position")),
        "{schema:?}"
    );
    for (release, name) in FIXTURE_RELEASES
        .iter()
        .flat_map(|r| ["reimbursement", "servicedesk"].map(|n| (*r, n)))
    {
        let (path, recorded) = release_fixture(release, name, "alike");
        assert_eq!(recorded["release"], format!("objectflow-core {release}"));
        let steps = objectflow_core::release::steps_from(
            recorded["release"].as_str().expect("its release"),
            objectflow_core::release::RELEASES,
        )
        .expect("a chain to this release")
        .len();
        let name = format!("{release} {name}");
        let args = ["--store", path.to_str().expect("a path"), "--port", "0"];
        let s = start(&args);
        let (status, health) = call(&s, "GET", "/health", None);
        // a store due an upgrade serves only the upgrade: its health is 503,
        // coded (ADR-0192 decision 7)
        assert_eq!(status, 503, "{health}");
        assert_eq!(
            (
                &health["upgrade_due"],
                &health["store_release"],
                &health["code"]
            ),
            (&json!(true), &recorded["release"], &json!("upgrade_due"))
        );
        let (status, refused) = call(
            &s,
            "POST",
            "/v1/get",
            Some(&json!({"actor": op, "id": "operator"})),
        );
        assert_eq!(status, 503, "{refused}");
        assert!(
            refused["fault"]
                .as_str()
                .unwrap_or_default()
                .starts_with("engine_mismatch")
        );
        // a dry run reports the step and leaves the store as it was
        let dry = post(&s, "/v1/upgrade", &json!({"actor": op, "dry_run": true}));
        assert_eq!(
            (
                &dry["from"],
                &dry["to"],
                &dry["upgraded"],
                dry["steps"].as_array().map(Vec::len)
            ),
            (
                &recorded["release"],
                &json!(BUILTINS_RELEASE),
                &json!(false),
                Some(steps)
            )
        );
        assert_eq!(
            call(&s, "GET", "/health", None).1["upgrade_due"],
            json!(true)
        );
        let done = post(&s, "/v1/upgrade", &json!({"actor": op}));
        assert_eq!(done["upgraded"], json!(true), "{done}");
        // what the release answered, object by object
        let objects = recorded["objects"].as_object().expect("its objects");
        assert!(objects.len() > 10, "{name}: {}", objects.len());
        for (id, was) in objects {
            let got = post(&s, "/v1/get", &json!({"actor": op, "id": id}));
            let history = post(&s, "/v1/history", &json!({"actor": op, "id": id}));
            assert_eq!(got["state"], was["state"], "{name} {id}");
            for (k, v) in was["attributes"].as_object().expect("its attributes") {
                // a built-in attribute holding JSON, which 0.1.0 answered as
                // its text, is answered as the JSON (ADR-0182 decision 9)
                let held = match (v, &got["attributes"][k]) {
                    (Value::String(text), got) if !got.is_string() && !got.is_null() => {
                        serde_json::from_str(text).unwrap_or_else(|_| v.clone())
                    }
                    _ => v.clone(),
                };
                assert_eq!(got["attributes"][k], held, "{name} {id}.{k}");
            }
            let before = was["history"]["events"].as_array().expect("its events");
            let now = history["events"].as_array().expect("its events");
            if id == "operator" {
                // the operator's act naming each release, after what it did
                for (was, is) in before.iter().zip(now) {
                    assert_answers_alike(was, is, &format!("{name} {id}"));
                }
                assert_eq!(now.len(), before.len() + steps);
                assert!(
                    now[before.len()..]
                        .iter()
                        .all(|e| e["transition"] == "upgrade")
                );
                assert_eq!(
                    got["version"].as_i64(),
                    was["version"]
                        .as_i64()
                        .map(|v| v + i64::try_from(steps).expect("a few steps"))
                );
            } else {
                assert_eq!(now.len(), before.len(), "{name} {id}");
                for (was, is) in before.iter().zip(now) {
                    assert_answers_alike(was, is, &format!("{name} {id}"));
                }
                assert_eq!(got["version"], was["version"], "{name} {id}");
            }
        }
        let log = exported_log(&s);
        let was = recorded["log"].as_array().expect("its log");
        // each line read as the JSON it is, so a key a later release adds
        // beside the ones it held, as an event's `batch` (ADR-0228), is
        // alike, as in every other answer
        let line = |v: &Value| match v {
            Value::String(text) => serde_json::from_str(text).unwrap_or_else(|_| v.clone()),
            other => other.clone(),
        };
        for (was, is) in was.iter().zip(&log) {
            assert_answers_alike(&line(was), &line(is), &format!("{name} log"));
        }
        assert_eq!(log.len(), was.len() + steps, "{name}");
        // a second upgrade has nothing to do
        let again = post(&s, "/v1/upgrade", &json!({"actor": op}));
        assert_eq!(again["upgraded"], json!(false));
        drop(s);
        // restarted, the store opens at this release and serves
        let s = start(&args);
        let (_, health) = call(&s, "GET", "/health", None);
        assert_eq!(
            (&health["upgrade_due"], &health["store_release"]),
            (&json!(false), &json!(BUILTINS_RELEASE))
        );
        let got = post(&s, "/v1/get", &json!({"actor": op, "id": "operator"}));
        assert_eq!(got["state"], "ACTIVE");
        // and holds the store's own tables, columns and indexes as a store
        // this release creates holds them
        drop(s);
        assert_eq!(own_schema(&path), own_schema(&created), "{name}");
    }
}

/// A dry run of an upgrade, then the upgrade, on one service, as
/// `operate.md`'s steps 4 and 5 have it, on a file store with its
/// write-ahead log beside it, as a service of the release before leaves it
/// at its stop: the dry run's connection, the only one open on a store due
/// an upgrade, removed the log as it closed, and the upgrade was refused
/// `503 store_replaced`, telling the operator to restore a store nothing
/// had harmed (issue #27). Each fixture is kept with no log beside it, so
/// the upgrade's test above, which runs the dry run first too, never met
/// one. Run from each release's fixture, 0.4.0's the step to 0.5.0.
#[test]
fn an_upgrade_after_its_dry_run_runs_on_the_same_service() {
    // from each release whose service made a store kept as a fixture
    for release in FIXTURE_RELEASES {
        let (path, _) = release_fixture(release, "reimbursement", "dry-then-up");
        // the store in WAL mode, and its log beside it, empty and held by no
        // one, as a service of the release before leaves it at its stop, its
        // checkpoint having truncated it
        {
            let conn = rusqlite::Connection::open(&path).expect("the store");
            let mode: String = conn
                .query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))
                .expect("its log");
            assert_eq!(mode, "wal");
        }
        let mut wal = path.as_os_str().to_owned();
        wal.push("-wal");
        std::fs::write(&wal, b"").expect("an empty log beside it");
        let s = start(&["--store", path.to_str().expect("a path"), "--port", "0"]);
        let op = json!({"id": "operator"});
        let dry = post(&s, "/v1/upgrade", &json!({"actor": op, "dry_run": true}));
        assert_eq!(dry["upgraded"], json!(false), "{release}: {dry}");
        let (status, done) = call(&s, "POST", "/v1/upgrade", Some(&json!({"actor": op})));
        assert_eq!(status, 200, "{release}: {done}");
        assert_eq!(done["upgraded"], json!(true), "{release}: {done}");
        // and the service serves the upgraded store, writes among it
        let (status, health) = call(&s, "GET", "/health", None);
        assert_eq!(status, 200, "{release}: {health}");
        let v = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "label", "transition": "record",
                    "inputs": {"subject": "operator", "name": "upgraded"}}),
        );
        assert_eq!(v["kind"], "satisfied", "{release}: {v}");
        drop(s);
        let _ = std::fs::remove_dir_all(path.parent().expect("its directory"));
    }
}

/// A store's own tables, `of_*`, each with its columns as `table_info`
/// gives them, and its own indexes by name, to compare an upgraded store
/// with one created.
fn own_schema(path: &Path) -> Vec<String> {
    let conn = rusqlite::Connection::open(path).expect("the store");
    let named: Vec<(String, String)> = conn
        .prepare("SELECT type, name FROM sqlite_master WHERE name LIKE 'of\\_%' ESCAPE '\\' ORDER BY type, name")
        .and_then(|mut q| {
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .and_then(|r| r.collect())
        })
        .expect("its schema");
    let mut out = Vec::new();
    for (kind, name) in named {
        out.push(format!("{kind} {name}"));
        if kind == "table" {
            let columns: Vec<String> = conn
                .prepare(&format!("PRAGMA table_info({name})"))
                .and_then(|mut q| {
                    q.query_map([], |r| {
                        Ok(format!(
                            "  {} {} notnull={} pk={}",
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, i64>(3)?,
                            r.get::<_, i64>(5)?
                        ))
                    })
                    .and_then(|r| r.collect())
                })
                .expect("its columns");
            out.extend(columns);
        }
    }
    out
}

/// An upgrade is the operator's, and refused to anyone else.
#[test]
fn only_an_operator_upgrades() {
    let (path, _) = release_fixture("0.1.0", "reimbursement", "operator");
    let s = start(&["--store", path.to_str().expect("a path"), "--port", "0"]);
    let (status, said) = call(
        &s,
        "POST",
        "/v1/upgrade",
        Some(&json!({"actor": {"id": "alice"}})),
    );
    assert_eq!(status, 409, "{said}");
    assert_eq!(
        call(&s, "GET", "/health", None).1["upgrade_due"],
        json!(true)
    );
}

/// What an earlier release answered, answered alike: every key it gave has
/// the same value, whatever keys a later release's wire adds beside them,
/// such as an event's `retries` (ADR-0178).
fn assert_answers_alike(was: &Value, is: &Value, what: &str) {
    match was {
        Value::Object(keys) => {
            for (k, v) in keys {
                assert!(is.get(k).is_some(), "{what}: {k} is no longer answered");
                assert_answers_alike(v, &is[k], &format!("{what}.{k}"));
            }
        }
        _ => assert_eq!(was, is, "{what}"),
    }
}

/// This release, as a store records it.
const BUILTINS_RELEASE: &str = concat!("objectflow-core ", env!("CARGO_PKG_VERSION"));

// ── more than one request at a time (ADR-0178) ─────────────────────────────

/// The service at an address, for a thread of its own: it stops nothing
/// when dropped.
fn at(s: &Service) -> Service {
    Service {
        child: None,
        addr: s.addr.clone(),
        said: s.said.clone(),
    }
}

/// The concurrency tests' module published, and a tally, two accounts of a
/// hundred each and the pair of them created: their ids.
fn ledger(s: &Service) -> (String, String, String, String) {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ledger.yaml"),
    )
    .expect("the ledger module");
    publish(s, &json!({ "ledger": text }));
    let op = json!({"id": "operator"});
    let made = |tn: &str, xn: &str, inputs: Value| -> String {
        let v = post(
            s,
            "/v1/request",
            &json!({"actor": op, "type": tn, "transition": xn, "inputs": inputs}),
        );
        assert_eq!(v["kind"], "satisfied", "{v}");
        v["object_id"].as_str().expect("its id").to_string()
    };
    let tally = made("Tally", "start", json!({"count": 0}));
    let a = made("Account", "open", json!({"name": "a", "balance": 100}));
    let b = made("Account", "open", json!({"name": "b", "balance": 100}));
    let pair = made("Pair", "make", json!({"left": a, "right": b}));
    (tally, a, b, pair)
}

fn bump(s: &Service, tally: &str, expected: Option<i64>) -> Value {
    let mut body = json!({"actor": {"id": "operator"}, "object_id": tally, "transition": "bump"});
    if let Some(v) = expected {
        body["expected_version"] = json!(v);
    }
    post(s, "/v1/request", &body)
}

/// Another process's write lock on the store's file, held until dropped.
fn held_lock(path: &Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(path).expect("the file");
    conn.execute_batch("BEGIN IMMEDIATE")
        .expect("the write lock");
    conn
}

/// A write waiting out another process's lock holds the writer, and every
/// read answers beside it, never behind it; the write applies once the lock
/// is let go (ADR-0178 decisions 1 and 2).
#[test]
fn reads_answer_beside_a_write_waiting_on_the_lock() {
    let file = fresh_store("beside");
    let path = file.to_str().expect("a path");
    let s = start(&[
        "--store",
        path,
        "--create",
        "--port",
        "0",
        "--busy-timeout",
        "5 s",
    ]);
    let (tally, a, _, pair) = ledger(&s);
    let lock = held_lock(&file);
    let writer = {
        let w = at(&s);
        let tally = tally.clone();
        std::thread::spawn(move || bump(&w, &tally, None))
    };
    std::thread::sleep(std::time::Duration::from_millis(300));
    let op = json!({"actor": {"id": "operator"}});
    let reads: Vec<(&str, Value)> = vec![
        ("/v1/get", json!({"actor": op["actor"], "id": tally})),
        ("/v1/get", json!({"actor": op["actor"], "id": pair})),
        (
            "/v1/query",
            json!({"actor": op["actor"], "type": "Account"}),
        ),
        ("/v1/history", json!({"actor": op["actor"], "id": a})),
        (
            "/v1/availability",
            json!({"actor": op["actor"], "id": tally}),
        ),
        (
            "/v1/declaration",
            json!({"actor": op["actor"], "type": "Tally"}),
        ),
        (
            "/v1/metric",
            json!({"actor": op["actor"], "name": "Tally.work_in_progress"}),
        ),
        (
            "/v1/check",
            json!({"actor": op["actor"], "object_id": tally, "transition": "bump"}),
        ),
    ];
    for (path, body) in &reads {
        let began = std::time::Instant::now();
        let (status, out) = call(&s, "POST", path, Some(body));
        let took = began.elapsed();
        assert_eq!(status, 200, "{path}: {out}");
        assert!(
            took < std::time::Duration::from_millis(500),
            "{path} took {took:?} beside the write"
        );
    }
    let (status, _) = call(&s, "GET", "/health", None);
    assert_eq!(status, 200);
    // the read of the tally saw it as it stood, the write not yet applied
    assert_eq!(
        post(&s, "/v1/get", &json!({"actor": op["actor"], "id": tally}))["attributes"]["count"],
        0
    );
    assert!(!writer.is_finished(), "the write did not wait on the lock");
    drop(lock);
    let v = writer.join().expect("the writer");
    assert_eq!(v["kind"], "satisfied", "{v}");
    assert_eq!(
        post(&s, "/v1/get", &json!({"actor": op["actor"], "id": tally}))["attributes"]["count"],
        1
    );
}

/// A lock held past the busy timeout is waited out again, and the event
/// records the retry; held past every retry, the write is refused `stale`,
/// cause `retries_exhausted`, a verdict, and nothing is written (ADR-0178
/// decision 2, `library-api.md` §4).
#[test]
fn a_write_meeting_a_lock_past_its_timeout_is_retried_then_refused_stale() {
    let file = fresh_store("busy");
    let path = file.to_str().expect("a path");
    let s = start(&[
        "--store",
        path,
        "--create",
        "--port",
        "0",
        "--busy-timeout",
        "1 s",
    ]);
    let (tally, _, _, _) = ledger(&s);
    // let go after one timeout and before the next: one retry
    let lock = held_lock(&file);
    let writer = {
        let w = at(&s);
        let tally = tally.clone();
        std::thread::spawn(move || bump(&w, &tally, None))
    };
    std::thread::sleep(std::time::Duration::from_millis(1500));
    drop(lock);
    let v = writer.join().expect("the writer");
    assert_eq!(v["kind"], "satisfied", "{v}");
    let history = post(
        &s,
        "/v1/history",
        &json!({"actor": {"id": "operator"}, "id": tally}),
    );
    let last = history["events"]
        .as_array()
        .and_then(|e| e.last())
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        (&last["transition"], &last["retries"]),
        (&json!("bump"), &json!(1)),
        "{last}"
    );
    // held past every retry
    drop(s);
    let s = start(&["--store", path, "--port", "0", "--busy-timeout", "0 s"]);
    let lock = held_lock(&file);
    let v = bump(&s, &tally, None);
    drop(lock);
    assert_eq!(
        (&v["kind"], &v["cause"], &v["remedy"]),
        (
            &json!("stale"),
            &json!("retries_exhausted"),
            &json!("temporal")
        ),
        "{v}"
    );
    assert_eq!(
        post(
            &s,
            "/v1/get",
            &json!({"actor": {"id": "operator"}, "id": tally})
        )["attributes"]["count"],
        1
    );
    // an acknowledgement waits and is refused as a request is, where it
    // answered 500 after one timeout (the audit's durability W8b)
    let op = json!({"id": "operator"});
    let kind = post(&s, "/v1/get", &json!({"actor": op, "id": tally}))["type"].clone();
    let subscription = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "Subscription", "transition": "subscribe",
                "inputs": {"target_type": kind}}),
    )["object_id"]
        .as_str()
        .expect("the subscription")
        .to_string();
    let lock = held_lock(&file);
    let (status, acknowledged) = call(
        &s,
        "POST",
        "/v1/acknowledge",
        Some(&json!({"actor": op, "subscription": subscription, "cursor": "0"})),
    );
    drop(lock);
    assert_eq!(
        (status, &acknowledged["refused"]),
        (200, &json!("stale")),
        "{acknowledged}"
    );
    let (status, acknowledged) = call(
        &s,
        "POST",
        "/v1/acknowledge",
        Some(&json!({"actor": op, "subscription": subscription, "cursor": "0"})),
    );
    assert_eq!(
        (status, &acknowledged["refused"]),
        (200, &Value::Null),
        "{acknowledged}"
    );
}

/// Writes from many callers at once are taken one at a time, and none is
/// lost: every bump counts, the tally's versions follow one another, and
/// every event has a position of its own; with `expected_version`, a bump
/// that read a version another moved is refused `stale` and sent again
/// (ADR-0178 decision 7).
#[test]
fn concurrent_writes_lose_no_update() {
    let file = fresh_store("writers");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let (tally, _, _, _) = ledger(&s);
    let (threads, each) = (8, 25);
    let workers: Vec<_> = (0..threads)
        .map(|_| {
            let w = at(&s);
            let tally = tally.clone();
            std::thread::spawn(move || {
                for _ in 0..each {
                    let v = bump(&w, &tally, None);
                    assert_eq!(v["kind"], "satisfied", "{v}");
                }
            })
        })
        .collect();
    for w in workers {
        w.join().expect("a writer");
    }
    let op = json!({"id": "operator"});
    let got = post(&s, "/v1/get", &json!({"actor": op, "id": tally}));
    assert_eq!(got["attributes"]["count"], json!(threads * each));
    assert_eq!(got["version"], json!(threads * each + 1));
    let history = post(
        &s,
        "/v1/history",
        &json!({"actor": op, "id": tally, "limit": 1000}),
    );
    let positions: std::collections::BTreeSet<i64> = history["events"]
        .as_array()
        .expect("its events")
        .iter()
        .filter_map(|e| e["position"].as_i64())
        .collect();
    assert_eq!(positions.len(), threads * each + 1);
    // read, then write naming the version read, again until it applies
    let (threads, each) = (4, 10);
    let stale = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let workers: Vec<_> = (0..threads)
        .map(|_| {
            let w = at(&s);
            let tally = tally.clone();
            let stale = std::sync::Arc::clone(&stale);
            std::thread::spawn(move || {
                let mut applied = 0;
                while applied < each {
                    let read = post(
                        &w,
                        "/v1/get",
                        &json!({"actor": {"id": "operator"}, "id": tally}),
                    );
                    let v = bump(&w, &tally, read["version"].as_i64());
                    match v["kind"].as_str() {
                        Some("satisfied") => applied += 1,
                        Some("stale") => {
                            assert_eq!(v["cause"], "expected_version", "{v}");
                            stale.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                        _ => panic!("{v}"),
                    }
                }
            })
        })
        .collect();
    for w in workers {
        w.join().expect("a writer");
    }
    let got = post(&s, "/v1/get", &json!({"actor": op, "id": tally}));
    assert_eq!(got["attributes"]["count"], json!(200 + threads * each));
    eprintln!(
        "{} bumps naming a version another moved were refused stale and sent again",
        stale.load(std::sync::atomic::Ordering::SeqCst)
    );
}

/// The flow of issue #29: a room holds at most one open booking at any
/// moment, an invariant over every booking of the type, which no backend
/// compiles into a constraint (`storage-schema.md` §8, ADR-0213 decision 7).
const ROOMS: &str = "module: rooms
categories: [live, closed]
types:
  Room:
    description: A meeting room.
    tracking: record
    attributes:
      name: { type: string, unique: true }
    states:
      IN_SERVICE: { description: In service., category: live }
      RETIRED: { description: Retired., category: closed, final: true }
    transitions:
      add: { kind: initial, to: IN_SERVICE, required_inputs: [name] }
      retire: { kind: external, from: IN_SERVICE, to: RETIRED }
  Booking:
    description: A room booked for a window of time.
    tracking: record
    attributes:
      room: { reference: Room }
      starts_at: { type: timestamp, indexed: true }
      ends_at: { type: timestamp, indexed: true }
    states:
      BOOKED: { description: Booked., category: live }
      CANCELLED: { description: Cancelled., category: closed, final: true }
    invariants:
      no_overlap:
        description: A room holds at most one open booking at any moment.
        expression: none(b in Booking where b.room == room and b.starts_at < ends_at and b.ends_at > starts_at and b.open and open)
    transitions:
      book: { kind: initial, to: BOOKED, required_inputs: [room, starts_at, ends_at] }
      cancel: { kind: external, from: BOOKED, to: CANCELLED }
";

/// Sixteen bookings of one room for windows that each overlap every other,
/// sent at once by sixteen callers: one is taken and fifteen are refused
/// `invariant_violated`, naming the one taken, and none is answered `5xx`;
/// sixteen of another room for windows apart, sent at once, are each
/// taken, so what refuses is the invariant, not the contention. The
/// engine's scan in the writing transaction keeps them apart, no database
/// constraint (issue #29; `storage-schema.md` §8).
fn overlapping_bookings_sent_at_once(s: &Service) {
    publish(s, &json!({"rooms": ROOMS}));
    let op = json!({"id": "operator"});
    let room = |name: &str| {
        let v = post(
            s,
            "/v1/request",
            &json!({"actor": op, "type": "Room", "transition": "add", "inputs": {"name": name}}),
        );
        assert_eq!(v["kind"], "satisfied", "{v}");
        v["object_id"].as_str().expect("the room").to_string()
    };
    let at_once = |room: &str, window: fn(usize) -> (String, String)| {
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(16));
        let callers: Vec<_> = (0..16)
            .map(|i| {
                let w = at(s);
                let room = room.to_string();
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let (starts_at, ends_at) = window(i);
                    let body = json!({"actor": {"id": "operator"}, "type": "Booking",
                                      "transition": "book", "inputs": {"room": room,
                                      "starts_at": starts_at, "ends_at": ends_at}});
                    barrier.wait();
                    call(&w, "POST", "/v1/request", Some(&body))
                })
            })
            .collect();
        callers
            .into_iter()
            .map(|c| c.join().expect("a caller"))
            .collect::<Vec<_>>()
    };
    // each window starts a minute after the one before and lasts an hour
    let overlapping = at_once(&room("Kelvin"), |i| {
        (
            format!("2026-11-09T01:{i:02}:00Z"),
            format!("2026-11-09T02:{i:02}:00Z"),
        )
    });
    let taken: Vec<&Value> = overlapping
        .iter()
        .filter(|(_, v)| v["kind"] == "satisfied")
        .map(|(_, v)| v)
        .collect();
    assert_eq!(taken.len(), 1, "{overlapping:?}");
    let booking = &taken[0]["object_id"];
    // a deployment's id, on either backend, a UUIDv7 (ADR-0226)
    let id = booking.as_str().expect("its id");
    let shaped = id.len() == 36
        && id.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            14 => c == '7',
            19 => matches!(c, '8' | '9' | 'a' | 'b'),
            _ => c.is_ascii_hexdigit() && !c.is_ascii_uppercase(),
        });
    assert!(shaped, "{id} is no UUIDv7");
    for (status, v) in &overlapping {
        assert_eq!(*status, 200, "{v}");
        if v["kind"] != "satisfied" {
            assert_eq!(v["kind"], "invariant_violated", "{v}");
            assert_eq!(v["failures"][0]["invariant"], "no_overlap", "{v}");
            assert_eq!(v["objects"], json!([booking]), "{v}");
        }
    }
    // each window an hour of its own, on the 9th from midnight
    let apart = at_once(&room("Faraday"), |i| {
        (
            format!("2026-11-09T{i:02}:00:00Z"),
            format!("2026-11-09T{i:02}:59:00Z"),
        )
    });
    for (status, v) in &apart {
        assert_eq!((*status, &v["kind"]), (200, &json!("satisfied")), "{v}");
    }
}

/// Overlapping bookings sent at once, on a file's store.
#[test]
fn overlapping_bookings_sent_at_once_take_one() {
    let file = fresh_store("overlap");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    overlapping_bookings_sent_at_once(&s);
    drop(s);
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// Overlapping bookings sent at once, on a store on PostgreSQL.
#[test]
fn overlapping_bookings_sent_at_once_take_one_on_postgresql() {
    let Some((admin, args, schema)) = served_on_postgresql("overlap") else {
        return;
    };
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let s = start(&a);
    overlapping_bookings_sent_at_once(&s);
    drop(s);
    admin
        .execute_batch(&format!("DROP SCHEMA {schema} CASCADE"))
        .expect("the schema dropped");
}

/// One request sent at once by many callers under one idempotency key, as
/// a caller retrying while its first send is still in flight, applies once:
/// a creation makes one object, every answer naming it, and a bump counts
/// once (ADR-0178 decision 7, ADR-0076's replay).
#[test]
fn a_request_sent_at_once_under_one_key_applies_once() {
    let file = fresh_store("one-key");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let (tally, _, _, _) = ledger(&s);
    let at_once = |body: Value| -> Vec<Value> {
        let callers: Vec<_> = (0..12)
            .map(|_| {
                let w = at(&s);
                let body = body.clone();
                std::thread::spawn(move || post(&w, "/v1/request", &body))
            })
            .collect();
        callers
            .into_iter()
            .map(|c| c.join().expect("a caller"))
            .collect()
    };
    let op = json!({"id": "operator"});
    let made = at_once(json!({"actor": op, "type": "Tally", "transition": "start",
                               "inputs": {"count": 0}, "idempotency_key": "one"}));
    assert!(made.iter().all(|v| v["kind"] == "satisfied"), "{made:?}");
    let ids: std::collections::BTreeSet<&str> = made
        .iter()
        .filter_map(|v| v["object_id"].as_str())
        .collect();
    assert_eq!(ids.len(), 1, "{made:?}");
    let bumped = at_once(
        json!({"actor": op, "object_id": tally, "transition": "bump",
                                "idempotency_key": "twice"}),
    );
    assert!(
        bumped.iter().all(|v| v["kind"] == "satisfied"),
        "{bumped:?}"
    );
    let got = post(&s, "/v1/get", &json!({"actor": op, "id": tally}));
    assert_eq!(
        (&got["attributes"]["count"], &got["version"]),
        (&json!(1), &json!(2)),
        "{got}"
    );
}

/// A read sees one committed state while writes commit around it: the
/// pair's total, read through both its accounts, is the same at every read
/// while moves between them commit (ADR-0178 decision 1).
#[test]
fn a_read_sees_one_committed_state() {
    let file = fresh_store("snapshot");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let (_, a, b, pair) = ledger(&s);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let movers: Vec<_> = [(a.clone(), b.clone()), (b.clone(), a.clone())]
        .into_iter()
        .map(|(from, to)| {
            let w = at(&s);
            std::thread::spawn(move || {
                let mut moves = 0;
                let mut amount = 1;
                while std::time::Instant::now() < until {
                    let v = post(
                        &w,
                        "/v1/request",
                        &json!({"actor": {"id": "operator"}, "object_id": from,
                        "transition": "move_to", "inputs": {"other": to, "amount": amount}}),
                    );
                    assert_eq!(v["kind"], "satisfied", "{v}");
                    amount = amount % 7 + 1;
                    moves += 1;
                }
                moves
            })
        })
        .collect();
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let r = at(&s);
            let pair = pair.clone();
            std::thread::spawn(move || {
                let mut reads = 0;
                while std::time::Instant::now() < until {
                    let got = post(&r, "/v1/get", &json!({"actor": {"id": "operator"}, "id": pair}));
                    assert_eq!(got["attributes"]["total"], json!(200), "{got}");
                    let page = post(&r, "/v1/query", &json!({"actor": {"id": "operator"}, "type": "Account", "fields": ["balance"]}));
                    let sum: i64 = page["items"]
                        .as_array()
                        .expect("its items")
                        .iter()
                        .filter_map(|i| i["fields"]["balance"].as_i64())
                        .sum();
                    assert_eq!(sum, 200, "{page}");
                    reads += 1;
                }
                reads
            })
        })
        .collect();
    let moves: i64 = movers.into_iter().map(|m| m.join().expect("a mover")).sum();
    let reads: i64 = readers
        .into_iter()
        .map(|r| r.join().expect("a reader"))
        .sum();
    assert!(moves > 10 && reads > 10, "{moves} moves, {reads} reads");
    eprintln!(
        "{moves} moves and {reads} reads of the pair and the accounts in 2 s, every total 200"
    );
}

/// A second process on the same file publishes; the first reads the new
/// version and writes under it, reloading its rules inside its own
/// transactions (ADR-0178 decision 3).
#[test]
fn a_version_another_process_publishes_is_read_and_written_by_this_one() {
    let file = fresh_store("two");
    let path = file.to_str().expect("a path");
    let first = start(&["--store", path, "--create", "--port", "0"]);
    let (tally, _, _, _) = ledger(&first);
    let second = start(&["--store", path, "--port", "0"]);
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ledger.yaml"),
    )
    .expect("the ledger module");
    let twice = text.replace(
        "      finish: { kind: external, from: COUNTING, to: DONE }",
        "      finish: { kind: external, from: COUNTING, to: DONE }\n      bump_twice:\n        kind: internal\n        from: COUNTING\n        effect:\n          - assign: { location: count, expr: count + 2 }",
    );
    assert_ne!(twice, text);
    publish(&second, &json!({ "ledger": twice }));
    let op = json!({"id": "operator"});
    let decl = post(
        &first,
        "/v1/declaration",
        &json!({"actor": op, "type": "Tally"}),
    );
    assert_eq!(decl["version"], json!(2), "{decl}");
    assert!(decl["transitions"].get("bump_twice").is_some(), "{decl}");
    let v = post(
        &first,
        "/v1/request",
        &json!({"actor": op, "object_id": tally, "transition": "bump_twice"}),
    );
    assert_eq!(v["kind"], "satisfied", "{v}");
    let got = post(&first, "/v1/get", &json!({"actor": op, "id": tally}));
    assert_eq!(got["attributes"]["count"], json!(2));
}

/// Reads a second, alone and beside writes, as measured on the machine the
/// test runs on; it asserts only that reads go on while writes run.
#[test]
fn reads_and_writes_a_second_measured() {
    let file = fresh_store("probe");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let (tally, _, _, pair) = ledger(&s);
    let window = std::time::Duration::from_secs(2);
    let read_for = |until: std::time::Instant| -> Vec<std::thread::JoinHandle<i64>> {
        (0..4)
            .map(|_| {
                let r = at(&s);
                let pair = pair.clone();
                std::thread::spawn(move || {
                    let mut n = 0;
                    while std::time::Instant::now() < until {
                        post(
                            &r,
                            "/v1/get",
                            &json!({"actor": {"id": "operator"}, "id": pair}),
                        );
                        n += 1;
                    }
                    n
                })
            })
            .collect()
    };
    let alone: i64 = read_for(std::time::Instant::now() + window)
        .into_iter()
        .map(|h| h.join().expect("a reader"))
        .sum();
    let until = std::time::Instant::now() + window;
    let writers: Vec<_> = (0..2)
        .map(|_| {
            let w = at(&s);
            let tally = tally.clone();
            std::thread::spawn(move || {
                let mut n = 0;
                while std::time::Instant::now() < until {
                    bump(&w, &tally, None);
                    n += 1;
                }
                n
            })
        })
        .collect();
    let beside: i64 = read_for(until)
        .into_iter()
        .map(|h| h.join().expect("a reader"))
        .sum();
    let writes: i64 = writers
        .into_iter()
        .map(|h| h.join().expect("a writer"))
        .sum();
    let per = |n: i64| n as f64 / window.as_secs_f64();
    eprintln!(
        "reads a second, 4 readers: {:.0} alone, {:.0} beside {:.0} writes a second from 2 writers",
        per(alone),
        per(beside),
        per(writes)
    );
    assert!(beside > 0 && writes > 0, "{beside} reads, {writes} writes");
}

// ── pages (ADR-0179) ────────────────────────────────────────────────────────

/// A query read in pages: each holds at most its limit, in the query's
/// order, the last naming no cursor; the pages together are the whole
/// read; a page after a write goes on from where the last stopped, the new
/// object met once in its place; a cursor of another read is refused, a
/// value; and a limit outside 1 to 1000 is a fault.
#[test]
fn a_query_is_read_in_pages() {
    let file = fresh_store("pages");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    ledger(&s);
    let op = json!({"id": "operator"});
    for (i, balance) in [5, 40, 15, 40, 25, 10].iter().enumerate() {
        let v = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "Account", "transition": "open",
            "inputs": {"name": format!("n{i}"), "balance": balance}}),
        );
        assert_eq!(v["kind"], "satisfied", "{v}");
    }
    let read = |cursor: &Value, limit: i64| {
        post(
            &s,
            "/v1/query",
            &json!({"actor": op, "type": "Account", "order": "balance desc",
            "fields": ["balance"], "limit": limit, "cursor": cursor}),
        )
    };
    let whole = read(&Value::Null, 1000);
    let all: Vec<Value> = whole["items"].as_array().expect("its items").clone();
    assert_eq!(all.len(), 8, "{whole}");
    assert!(whole["cursor"].is_null());
    let (mut paged, mut cursor, mut pages) = (Vec::new(), Value::Null, 0);
    loop {
        let page = read(&cursor, 3);
        let items = page["items"].as_array().expect("its items");
        assert!(items.len() <= 3, "{page}");
        paged.extend(items.iter().cloned());
        pages += 1;
        assert!(pages <= 10, "a cursor that never ends: {page}");
        if page["cursor"].is_null() {
            break;
        }
        cursor = page["cursor"].clone();
    }
    assert_eq!((pages, &paged), (3, &all));
    // a write between pages: the next page goes on after the last given
    let first = read(&Value::Null, 3);
    let after = first["items"][2]["fields"]["balance"]
        .as_i64()
        .expect("a balance");
    let made = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "Account", "transition": "open",
        "inputs": {"name": "late", "balance": 1}}),
    );
    let late = made["object_id"].clone();
    let (mut rest, mut cursor) = (Vec::new(), first["cursor"].clone());
    while !cursor.is_null() {
        let page = read(&cursor, 3);
        rest.extend(page["items"].as_array().expect("its items").iter().cloned());
        assert!(rest.len() <= 30, "a cursor that never ends: {page}");
        cursor = page["cursor"].clone();
    }
    assert!(
        rest.iter()
            .all(|i| i["fields"]["balance"].as_i64() <= Some(after)),
        "{rest:?}"
    );
    assert_eq!(
        rest.iter().filter(|i| i["id"] == late).count(),
        1,
        "{rest:?}"
    );
    assert_eq!(rest.len(), 6, "{rest:?}");
    // a cursor continues only its own read
    let other = post(
        &s,
        "/v1/query",
        &json!({"actor": op, "type": "Account", "limit": 3,
        "cursor": first["cursor"]}),
    );
    assert_eq!(other["refused"], "malformed_cursor", "{other}");
    for bad in [json!(0), json!(1001), json!("ten")] {
        let (status, said) = call(
            &s,
            "POST",
            "/v1/query",
            Some(&json!({"actor": op, "type": "Account", "limit": bad})),
        );
        assert_eq!(status, 400, "{said}");
    }
}

/// `available`, `pull` and `export` page as well: available by its cursor,
/// pull by its settled cursor, export by its own, never ending a page
/// inside one cursor's rows.
#[test]
fn available_pull_and_export_are_read_in_pages() {
    let file = fresh_store("more-pages");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let (tally, ..) = ledger(&s);
    let op = json!({"id": "operator"});
    let sub = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "Subscription", "transition": "subscribe",
        "inputs": {"target_type": "Tally"}}),
    );
    let sub = sub["object_id"].clone();
    for _ in 0..7 {
        assert_eq!(bump(&s, &tally, None)["kind"], "satisfied");
    }
    for i in 0..5 {
        post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "Tally", "transition": "start", "inputs": {"count": i}}),
        );
    }
    // available: every counting tally, in pages of two
    let (mut seen, mut cursor) = (Vec::new(), Value::Null);
    loop {
        let page = post(
            &s,
            "/v1/available",
            &json!({"actor": op, "type": "Tally", "transition": "bump",
            "limit": 2, "cursor": cursor}),
        );
        let items = page["items"].as_array().expect("its items");
        assert!(items.len() <= 2, "{page}");
        seen.extend(items.iter().map(|i| i["id"].clone()));
        assert!(seen.len() <= 30, "a cursor that never ends: {page}");
        if page["cursor"].is_null() {
            break;
        }
        cursor = page["cursor"].clone();
    }
    assert_eq!(seen.len(), 6, "{seen:?}");
    // pull: the bumps after the subscription, three at a time
    let (mut events, mut cursor) = (Vec::new(), Value::Null);
    loop {
        let mut body = json!({"actor": op, "subscription": sub, "limit": 3});
        if !cursor.is_null() {
            body["cursor"] = cursor.clone();
        }
        let page = post(&s, "/v1/pull", &body);
        let got = page["events"].as_array().expect("its events").clone();
        assert!(got.len() <= 3, "{page}");
        events.extend(got.iter().cloned());
        assert!(events.len() <= 60, "a cursor that never ends: {page}");
        if got.len() < 3 {
            break;
        }
        cursor = page["settled"].clone();
    }
    let bumps = events.iter().filter(|e| e["transition"] == "bump").count();
    assert_eq!(bumps, 7, "{events:?}");
    // export: the log in pages of four, together the whole of it
    let whole = post(
        &s,
        "/v1/export",
        &json!({"actor": op, "source": "log", "limit": 1000}),
    );
    let whole = whole["lines"].as_array().expect("its lines").len();
    let (mut lines, mut cursor) = (0, Value::Null);
    loop {
        let page = post(
            &s,
            "/v1/export",
            &json!({"actor": op, "source": "log", "limit": 4, "cursor": cursor}),
        );
        let got = page["lines"].as_array().expect("its lines").len();
        if got == 0 {
            break;
        }
        lines += got;
        assert!(lines <= 10 * whole, "a cursor that never ends: {page}");
        cursor = page["cursor"].clone();
    }
    assert_eq!(lines, whole);
}

/// A filter comparing a member with a literal its type cannot hold is
/// refused `malformed_filter`, never answered empty or `501` (D580): a
/// creation time with text, a name with a number, the state with text;
/// each written as its type is, it is answered.
#[test]
fn a_filter_that_does_not_type_is_refused() {
    let file = fresh_store("typed-filters");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    ledger(&s);
    let op = json!({"id": "operator"});
    let read = |filter: &str| {
        post(
            &s,
            "/v1/query",
            &json!({"actor": op, "type": "Account", "filter": filter}),
        )
    };
    for wrong in [
        r#"created_at <= "2026-01-01T00:00:00Z""#,
        "name == 3",
        r#"state == "OPEN""#,
        "balance == true",
    ] {
        let page = read(wrong);
        assert_eq!(page["refused"], "malformed_filter", "{wrong}: {page}");
    }
    for right in [
        "created_at <= now",
        r#"name == "a""#,
        "state == OPEN",
        "balance >= 0",
    ] {
        let page = read(right);
        assert!(page["refused"].is_null(), "{right}: {page}");
    }
}

/// An erasure leaves nothing it erased in the store's files (D584): the
/// pages it rewrote are zeroed by `secure_delete`, and the write-ahead
/// log, whose earlier frames held the values, is truncated after it
/// commits; another person's values stay.
#[test]
fn an_erasure_leaves_no_erased_bytes_in_the_stores_files() {
    let file = fresh_store("erasure-bytes");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let module = "module: erase
categories: [live, closed]
types:
  Candidate:
    description: Someone who applied.
    tracking: record
    attributes:
      name: { type: string, personal: true, optional: true }
      email: { type: string, personal: true, optional: true, unique: true, indexed: true }
    states:
      NEW: { category: live }
      DONE: { category: closed, final: true }
    transitions:
      apply: { kind: initial, to: NEW, optional_inputs: [name, email] }
      close: { kind: external, from: NEW, to: DONE }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
";
    publish(&s, &json!({ "erase": module }));
    let op = json!({"id": "operator"});
    let apply = |name: &str, email: &str| {
        let v = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "Candidate", "transition": "apply",
            "inputs": {"name": name, "email": email}}),
        );
        v["object_id"].as_str().expect("a candidate").to_string()
    };
    let gone = apply("Zebulon Quaxworth", "zebulon@quax.example");
    for i in 0..20 {
        apply(&format!("Other {i}"), &format!("other{i}@kept.example"));
    }
    let v = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "object_id": gone, "transition": "forget",
        "inputs": {"reason": "asked to be forgotten"}}),
    );
    assert_eq!(v["kind"], "satisfied", "{v}");
    let held = |needle: &str| {
        let mut found = Vec::new();
        for suffix in ["", "-wal"] {
            let path = PathBuf::from(format!("{}{suffix}", file.display()));
            if let Ok(bytes) = std::fs::read(&path)
                && bytes.windows(needle.len()).any(|w| w == needle.as_bytes())
            {
                found.push(path.display().to_string());
            }
        }
        found
    };
    assert_eq!(held("Quaxworth"), Vec::<String>::new());
    assert_eq!(held("zebulon@quax"), Vec::<String>::new());
    assert!(!held("other19@kept").is_empty(), "the others' values stay");
}

/// An erasure while a read older than it is open waits for no reader: the
/// writes after it take what they take, never the busy timeout each, and
/// the truncation that takes the erased values out of the `-wal` is made by
/// the first write after the read ends (ERASE-CHECKPOINT, D584).
#[test]
fn an_erasure_beside_an_open_read_holds_no_write() {
    let file = fresh_store("erasure-read");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let module = "module: erase\ncategories: [live, closed]\ntypes:\n  Candidate:\n    description: Someone who applied.\n    tracking: record\n    attributes:\n      name: { type: string, personal: true, optional: true }\n    states:\n      NEW: { category: live }\n      DONE: { category: closed, final: true }\n    transitions:\n      apply: { kind: initial, to: NEW, optional_inputs: [name] }\n      close: { kind: external, from: NEW, to: DONE }\n      forget: { kind: erasure, inputs: { reason: { type: string } } }\n";
    publish(&s, &json!({ "erase": module }));
    let op = json!({"id": "operator"});
    let apply = |name: &str| {
        let v = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "Candidate", "transition": "apply", "inputs": {"name": name}}),
        );
        v["object_id"].as_str().expect("a candidate").to_string()
    };
    let gone = apply("Ysolde Quillfeather");
    // a read older than the erasure, held open on the store's file
    let reader = rusqlite::Connection::open(&file).expect("the file");
    reader.execute_batch("BEGIN").expect("a read");
    let _: i64 = reader
        .query_row("SELECT count(*) FROM of_event", [], |r| r.get(0))
        .expect("read");
    let v = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "object_id": gone, "transition": "forget",
        "inputs": {"reason": "asked to be forgotten"}}),
    );
    assert_eq!(v["kind"], "satisfied", "{v}");
    for i in 0..3 {
        let began = std::time::Instant::now();
        apply(&format!("Other {i}"));
        let took = began.elapsed();
        assert!(
            took < std::time::Duration::from_secs(2),
            "a write took {took:?} with a read open after an erasure"
        );
    }
    reader.execute_batch("COMMIT").expect("the read ended");
    apply("After");
    let wal = std::fs::metadata(format!("{}-wal", file.display())).map_or(0, |m| m.len());
    assert_eq!(wal, 0, "the -wal was not truncated once the read ended");
    let held = std::fs::read(&file).expect("the store's file");
    assert!(!held.windows(10).any(|w| w == b"Quillfeath"));
}

/// A change's source and report are answered as the JSON they hold, as
/// its source was written, never as text holding it (ADR-0182 decision 9).
#[test]
fn a_changes_source_and_report_are_answered_as_json() {
    let file = fresh_store("held-json");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    publish_service(&s);
    let op = json!({"id": "operator"});
    let page = post(
        &s,
        "/v1/query",
        &json!({"actor": op, "type": "DeclarationChange",
        "fields": ["source", "report"]}),
    );
    let change = page["items"][0]["id"].clone();
    assert!(page["items"][0]["fields"]["source"].is_object(), "{page}");
    let got = post(&s, "/v1/get", &json!({"actor": op, "id": change}));
    let attributes = &got["attributes"];
    assert!(attributes["source"]["service"].is_string(), "{got}");
    assert_eq!(attributes["report"]["accepted"], true, "{got}");
}

/// A filter long or nested past the parser's bounds is refused
/// `malformed_filter`, and the service answers on: a 3,000-term `or` chain
/// once aborted the process by overflowing its stack (F1). Within the
/// bounds, a 120-term chain and 60 parentheses are answered, a list of
/// alternatives of any length too.
#[test]
fn a_filter_past_the_parsers_bounds_is_refused_and_the_service_answers_on() {
    let file = fresh_store("deep-filters");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    ledger(&s);
    let op = json!({"id": "operator"});
    let read = |filter: &str| {
        post(
            &s,
            "/v1/query",
            &json!({"actor": op, "type": "Account", "filter": filter}),
        )
    };
    let chain = |n: usize| {
        (0..n)
            .map(|i| format!("balance == {i}"))
            .collect::<Vec<_>>()
            .join(" or ")
    };
    // a set of more than 1,000 members is refused, as the fifth production
    // review's FILTER-COST had one of 10,000 hold a read's slot for 7.8 s
    // (ADR-0190 decision 3)
    let set = |n: usize| {
        format!(
            "balance in {{{}}}",
            (0..n).map(|i| i.to_string()).collect::<Vec<_>>().join(", ")
        )
    };
    for wrong in [
        chain(3000),
        // past 256 terms, an `or` of 120 comparisons, which a set says
        // (ADR-0192 decision 3)
        chain(120),
        format!("{}balance > 0{}", "(".repeat(600), ")".repeat(600)),
        format!("{}true", "not ".repeat(5000)),
        set(3000),
    ] {
        let page = read(&wrong);
        assert_eq!(
            page["refused"],
            "malformed_filter",
            "{}",
            &page.to_string()[..200.min(page.to_string().len())]
        );
    }
    for right in [
        chain(40),
        format!("{}balance >= 0{}", "(".repeat(60), ")".repeat(60)),
        set(1000),
    ] {
        let page = read(&right);
        assert!(
            page["refused"].is_null(),
            "{}",
            &page.to_string()[..200.min(page.to_string().len())]
        );
    }
    let (status, _) = call(&s, "GET", "/health", None);
    assert_eq!(status, 200);
}

/// What the service's operator is told (fourth production review): every
/// line stamped, the start's among them; listening beyond loopback said;
/// an evaluator's password never written, its failed calls counted; each
/// verdict's kind and each route's durations in `/metrics`; a fault's code
/// beside its reason; and `/metrics` and `/health` alone on an address of
/// their own.
#[test]
fn an_operator_is_told_what_the_service_does() {
    let file = fresh_store("told");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--bind",
        "0.0.0.0:0",
        "--metrics-bind",
        "127.0.0.1:0",
        "--evaluator",
        "bank=http://teller:s3cret@127.0.0.1:1/verify",
        "--evaluator-timeout",
        "1 s",
    ]);
    let said = s.said_within("metrics and health on http://");
    assert!(said.contains("listening beyond loopback"), "{said}");
    assert!(
        said.contains("asked at http://teller:***@127.0.0.1:1/verify"),
        "{said}"
    );
    assert!(!said.contains("s3cret"), "{said}");
    // every line stamped: a timestamp, then what was said
    for line in said.lines() {
        assert!(
            line.as_bytes().get(4) == Some(&b'-') && line.contains('T'),
            "{line}"
        );
    }
    let metrics_at = said
        .lines()
        .find_map(|l| {
            l.split_once("metrics and health on http://")
                .map(|(_, a)| a.trim().to_string())
        })
        .expect("the metrics address");
    publish(&s, &json!({"payments": PAYMENTS}));
    let v = settle(&s, "T-9");
    assert_eq!(v["remedy"], "temporal", "{v}");
    let failed = s.said_within("was not reached");
    assert!(
        failed.contains("http://teller:***@127.0.0.1:1/verify was not reached"),
        "{failed}"
    );
    assert!(!failed.contains("s3cret"), "{failed}");
    // a fault's code beside its reason
    let (status, fault) = call(
        &s,
        "POST",
        "/v1/pull",
        Some(&json!({"actor": {"id": "operator"}})),
    );
    assert_eq!(
        (status, fault["code"].as_str()),
        (400, Some("bad_request")),
        "{fault}"
    );
    let (status, fault) = call(&s, "GET", "/v1/nothing", None);
    assert_eq!(
        (status, fault["code"].as_str()),
        (404, Some("not_found")),
        "{fault}"
    );
    // the metrics listener answers /metrics and /health, and nothing else
    let metrics_svc = Service {
        child: None,
        addr: metrics_at,
        said: std::sync::Arc::new(std::sync::Mutex::new(String::new())),
    };
    let (status, health) = call(&metrics_svc, "GET", "/health", None);
    assert_eq!(status, 200, "{health}");
    let mut stream = TcpStream::connect(&metrics_svc.addr).expect("a connection");
    write!(
        stream,
        "GET /metrics HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    )
    .expect("asked");
    let mut metrics = String::new();
    stream.read_to_string(&mut metrics).expect("the metrics");
    assert!(
        metrics.contains("objectflow_verdicts_total{kind=\"satisfied\"}"),
        "{metrics}"
    );
    assert!(
        metrics.contains(
            "objectflow_request_duration_seconds_bucket{route=\"/v1/request\",le=\"+Inf\"}"
        ),
        "{metrics}"
    );
    assert!(
        metrics.contains("objectflow_evaluator_failures_total{evaluator=\"bank\"}"),
        "{metrics}"
    );
    assert!(
        metrics.contains("objectflow_log_lines_dropped_total 0"),
        "{metrics}"
    );
    let (status, _) = call(&metrics_svc, "POST", "/v1/get", Some(&json!({"id": "x"})));
    assert_eq!(status, 404);
}

/// A store file moved or replaced under the running service refuses every
/// write `503 store_replaced` and makes `/health` say so, where a write was
/// acknowledged into a file no one would read (fourth production review).
#[test]
fn a_store_replaced_under_the_service_takes_no_write() {
    let file = fresh_store("replaced");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    publish_service(&s);
    let op = json!({"id": "operator"});
    let add = json!({"actor": op, "type": "User", "transition": "add", "inputs": {"login": "ana", "name": "Ana"}});
    let (status, v) = call(&s, "POST", "/v1/request", Some(&add));
    assert_eq!(
        (status, v["kind"].as_str()),
        (200, Some("satisfied")),
        "{v}"
    );
    let moved = file.with_file_name("moved.sqlite");
    std::fs::rename(&file, &moved).expect("moved");
    std::fs::copy(&moved, &file).expect("a copy in its place");
    let (status, fault) = call(&s, "POST", "/v1/request", Some(&add));
    assert_eq!(
        (status, fault["code"].as_str()),
        (503, Some("store_replaced")),
        "{fault}\n{}",
        s.said()
    );
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(
        (status, health["code"].as_str()),
        (503, Some("store_replaced")),
        "{health}"
    );
    // moved away with nothing in its place, a write is refused alike, and
    // health answers its storage unavailable
    std::fs::remove_file(&file).expect("the copy removed");
    let (status, fault) = call(&s, "POST", "/v1/request", Some(&add));
    assert_eq!(
        (status, fault["code"].as_str()),
        (503, Some("store_replaced")),
        "{fault}"
    );
    // gone, as moved, answers 503 store_replaced, where it answered 500
    // (fifth production review, ADR-0191 decision 4)
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(
        (status, health["code"].as_str()),
        (503, Some("store_replaced")),
        "{health}"
    );
    // and so does a read, where it answered 500 (ADR-0192 decision 7)
    let (status, read) = call(
        &s,
        "POST",
        "/v1/get",
        Some(&json!({"actor": {"id": "operator"}, "id": "operator"})),
    );
    assert_eq!(
        (status, read["code"].as_str()),
        (503, Some("store_replaced")),
        "{read}"
    );
}

/// Writes stopped behind another process's lock make `/health` answer
/// `503 writes_stalled` until one commits again, where it answered 200
/// while every write failed (fourth production review).
#[test]
fn writes_stalled_behind_another_lock_are_said_by_health() {
    let file = fresh_store("stalled");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
        "--busy-timeout",
        "1 s",
    ]);
    publish_service(&s);
    let op = json!({"id": "operator"});
    let add = json!({"actor": op, "type": "User", "transition": "add", "inputs": {"login": "ana", "name": "Ana"}});
    let other = rusqlite::Connection::open(&file).expect("another process's connection");
    other.execute_batch("BEGIN IMMEDIATE").expect("its lock");
    // said as the write waits, from its first retry, where it was said only
    // once a write was refused, 40 s on (eighth production review, ADR-0195
    // decision 6)
    std::thread::scope(|scope| {
        let first = scope.spawn(|| call(&s, "POST", "/v1/request", Some(&add)));
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let (status, health) = call(&s, "GET", "/health", None);
        assert_eq!(
            (status, health["code"].as_str()),
            (503, Some("writes_stalled")),
            "{health}"
        );
        // and said in the log while it waits, where it was said only once
        // the write gave up (ninth production review)
        let said = s.said_within("writes are waiting for another process's lock on the store");
        assert!(
            said.contains("writes are waiting for another process's lock on the store"),
            "{said}"
        );
        assert!(!first.is_finished(), "the write ended before its retries");
        let (status, v) = first.join().expect("the first write");
        assert_eq!((status, v["kind"].as_str()), (200, Some("stale")), "{v}");
        // the next write's wait counted from the first one's retry, where
        // it began again at each write
        let second = scope.spawn(|| call(&s, "POST", "/v1/request", Some(&add)));
        std::thread::sleep(std::time::Duration::from_millis(500));
        let (_, health) = call(&s, "GET", "/health", None);
        let held = health["writer_held_seconds"].as_f64().unwrap_or_default();
        assert!(held >= 3.0, "the writer held {held} s: {health}");
        let (status, v) = second.join().expect("the second write");
        assert_eq!((status, v["kind"].as_str()), (200, Some("stale")), "{v}");
    });
    // the stall said in the log once, as it began
    let want = "writes are waiting for another process's lock on the store";
    let said = s.said_within(want);
    assert_eq!(said.matches(want).count(), 1, "{said}");
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(
        (status, health["code"].as_str()),
        (503, Some("writes_stalled")),
        "{health}"
    );
    other.execute_batch("ROLLBACK").expect("its lock released");
    let (status, v) = call(&s, "POST", "/v1/request", Some(&add));
    assert_eq!(
        (status, v["kind"].as_str()),
        (200, Some("satisfied")),
        "{v}"
    );
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(status, 200, "{health}");
}

/// A write waiting for another process's lock is said in the log as it
/// waits, a second or so after its first retry, whatever the log's cut is
/// doing: with a busy timeout as long as the cut's look, a look always fell
/// within the write's first wait, found no stall yet and then waited for
/// the writer, so the stall was said only once the write gave up (ninth
/// production review).
#[test]
fn a_stall_is_said_in_the_log_as_it_waits() {
    let file = fresh_store("stall-log");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
        "--busy-timeout",
        "5 s",
    ]);
    publish_service(&s);
    let op = json!({"id": "operator"});
    let add = json!({"actor": op, "type": "User", "transition": "add", "inputs": {"login": "ana", "name": "Ana"}});
    let other = rusqlite::Connection::open(&file).expect("another process's connection");
    other.execute_batch("BEGIN IMMEDIATE").expect("its lock");
    let want = "writes are waiting for another process's lock on the store";
    std::thread::scope(|scope| {
        let waiting = scope.spawn(|| call(&s, "POST", "/v1/request", Some(&add)));
        // its first retry, 5 s in, said by /health
        let began = std::time::Instant::now();
        loop {
            let (status, health) = call(&s, "GET", "/health", None);
            if status == 503 {
                assert_eq!(health["code"], "writes_stalled", "{health}");
                break;
            }
            assert!(
                began.elapsed() < std::time::Duration::from_secs(12),
                "no stall said: {health}"
            );
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        // and in the log within seconds, the write still waiting
        let said = s.said_within(want);
        assert!(said.contains(want), "{said}");
        assert!(!waiting.is_finished(), "said only once the write gave up");
        let (status, v) = waiting.join().expect("the write");
        assert_eq!((status, v["kind"].as_str()), (200, Some("stale")), "{v}");
    });
    other.execute_batch("ROLLBACK").expect("its lock released");
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// Writes failing for the service's own fault make `/health` answer `503
/// writes_failing` once three in a row are answered 500, and 200 again once
/// one commits, where `/health` answered 200 while every write failed
/// (eighth production review, ADR-0195 decision 6). The writes fail here
/// by a trigger another process added, reads untouched; the review's own
/// path, a backup copied over the live file in place, was not reproduced:
/// the copies tried either failed `/health` too or served the backup.
#[test]
fn writes_failing_are_said_by_health() {
    let file = fresh_store("failing");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    publish_service(&s);
    let op = json!({"id": "operator"});
    let add = |i: usize| {
        json!({"actor": op, "type": "User", "transition": "add",
               "inputs": {"login": format!("u{i}"), "name": "A person"}})
    };
    assert_eq!(post(&s, "/v1/request", &add(0))["kind"], "satisfied");
    let other = rusqlite::Connection::open(&file).expect("another process's connection");
    other
        .execute_batch(
            "CREATE TRIGGER failing BEFORE INSERT ON of_event BEGIN SELECT RAISE(ABORT, 'failing'); END;",
        )
        .expect("the trigger");
    for i in 1..3 {
        assert_eq!(call(&s, "POST", "/v1/request", Some(&add(i))).0, 500);
        let (status, health) = call(&s, "GET", "/health", None);
        assert_eq!(status, 200, "said after {i} failure(s): {health}");
    }
    assert_eq!(call(&s, "POST", "/v1/request", Some(&add(3))).0, 500);
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(
        (status, health["code"].as_str()),
        (503, Some("writes_failing")),
        "{health}"
    );
    // a write refused for its own reason is no failure of the service's,
    // and ends the run as a commit does
    other
        .execute_batch("DROP TRIGGER failing")
        .expect("the trigger dropped");
    let (status, v) = call(&s, "POST", "/v1/request", Some(&add(0)));
    assert_eq!(
        (status, v["kind"].as_str()),
        (200, Some("invariant_violated")),
        "{v}"
    );
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(status, 200, "{health}");
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// A log no one reads stops no request beyond its bound, and `/health`
/// answers `503 log_blocked` until it is read, where the service stopped at
/// its next line while `/health` answered 200 (fourth production review).
#[test]
fn a_log_no_one_reads_is_said_and_stops_no_request() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_objectflow-service"))
        .args(["--port", "0"])
        .stderr(Stdio::piped())
        .spawn()
        .expect("the service starts");
    let mut stderr = BufReader::new(child.stderr.take().expect("its stderr"));
    let mut line = String::new();
    stderr.read_line(&mut line).expect("its first line");
    let addr = line
        .trim()
        .split_once("listening on http://")
        .map(|(_, a)| a.to_string())
        .expect("its address");
    let s = Service {
        child: Some(child),
        addr,
        said: std::sync::Arc::new(std::sync::Mutex::new(String::new())),
    };
    // answers logged by four clients at once until the pipe, unread, and the
    // log's queue are full: every request still answered, where two that
    // each met the full pipe stopped both of the runtime's threads and every
    // request, /health included (tenth production review LOG-FREEZE)
    let get = json!({"actor": {"id": "operator"}, "id": "nothing-1"});
    let flood: Vec<_> = (0..4)
        .map(|_| {
            let addr = s.addr.clone();
            let get = get.clone();
            std::thread::spawn(move || {
                for _ in 0..2_500 {
                    let mut stream = TcpStream::connect(&addr).expect("a connection");
                    let body = get.to_string();
                    write!(
                        stream,
                        "POST /v1/get HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .expect("asked");
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                        .expect("a timeout");
                    let mut answer = String::new();
                    if stream.read_to_string(&mut answer).is_err() || answer.is_empty() {
                        return false;
                    }
                }
                true
            })
        })
        .collect();
    for client in flood {
        assert!(
            client.join().expect("a client"),
            "a request waited on the log"
        );
    }
    std::thread::sleep(std::time::Duration::from_secs(6));
    let began = std::time::Instant::now();
    let (status, health) = call(&s, "GET", "/health", None);
    assert!(
        began.elapsed() < std::time::Duration::from_secs(2),
        "health waited on the log"
    );
    assert_eq!(
        (status, health["code"].as_str()),
        (503, Some("log_blocked")),
        "{health}"
    );
    // read, the log takes lines again and health is 200, and the log says
    // how many lines it dropped, where only a counter did (eleventh
    // production review)
    let read = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let reading = read.clone();
    std::thread::spawn(move || {
        let mut line = String::new();
        while stderr.read_line(&mut line).is_ok_and(|n| n > 0) {
            if let Ok(mut r) = reading.lock() {
                r.push_str(&line);
            }
            line.clear();
        }
    });
    std::thread::sleep(std::time::Duration::from_millis(500));
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(status, 200, "{health}");
    let told = read
        .lock()
        .map(|r| r.contains(" the log dropped "))
        .unwrap_or(false);
    assert!(told, "the log did not say it dropped lines");
}

/// A unit of `Type=notify` is told the service is ready once it answers,
/// and that it stops on SIGTERM (fourth production review).
#[test]
fn systemd_is_told_ready_and_stopping() {
    let dir = fresh_store("notify");
    let socket_path = dir.with_file_name("notify.sock");
    let socket = std::os::unix::net::UnixDatagram::bind(&socket_path).expect("a notify socket");
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .expect("a timeout");
    let child = Command::new(env!("CARGO_BIN_EXE_objectflow-service"))
        .args(["--port", "0"])
        .env("NOTIFY_SOCKET", &socket_path)
        .env("WATCHDOG_USEC", "200000")
        .stderr(Stdio::null())
        .spawn()
        .expect("the service starts");
    let pid = child.id();
    // stopped however the test ends
    let _s = Service {
        child: Some(child),
        addr: String::new(),
        said: std::sync::Arc::new(std::sync::Mutex::new(String::new())),
    };
    let mut buf = [0u8; 64];
    let n = socket.recv(&mut buf).expect("told it is ready");
    assert_eq!(&buf[..n], b"READY=1");
    // under WatchdogSec, told it lives at half the interval (ADR-0191
    // decision 4)
    for _ in 0..3 {
        let n = socket.recv(&mut buf).expect("told it lives");
        assert_eq!(&buf[..n], b"WATCHDOG=1");
    }
    let sent = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .expect("kill runs");
    assert!(sent.success());
    loop {
        let n = socket.recv(&mut buf).expect("told it stops");
        if &buf[..n] != b"WATCHDOG=1" {
            assert_eq!(&buf[..n], b"STOPPING=1");
            break;
        }
    }
}

/// A log whose reader has gone drops its lines: every request is still
/// answered and `/health` is 200, where `eprintln!` panicked on the broken
/// pipe, losing answers to writes already committed and leaving the log held
/// blocked until a restart (fifth production review).
#[test]
fn a_log_whose_reader_has_gone_stops_nothing() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_objectflow-service"))
        .args(["--port", "0"])
        .stderr(Stdio::piped())
        .spawn()
        .expect("the service starts");
    let mut stderr = BufReader::new(child.stderr.take().expect("its stderr"));
    let mut line = String::new();
    stderr.read_line(&mut line).expect("its first line");
    let addr = line
        .trim()
        .split_once("listening on http://")
        .map(|(_, a)| a.to_string())
        .expect("its address");
    drop(stderr);
    let s = Service {
        child: Some(child),
        addr,
        said: std::sync::Arc::new(std::sync::Mutex::new(String::new())),
    };
    for _ in 0..20 {
        let (status, _) = call(
            &s,
            "POST",
            "/v1/get",
            Some(&json!({"actor": {"id": "operator"}, "id": "nothing-1"})),
        );
        // answered, a sandbox with no module saying so, its line dropped
        assert_eq!(status, 409);
    }
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(status, 200, "{health}");
}

/// A body naming a key twice is refused 400 by the core itself, where the
/// last one won, a second `actor` changing who was recorded (fifth
/// production review, ADR-0190 decision 8).
#[test]
fn a_body_naming_a_key_twice_is_refused() {
    let file = fresh_store("twice");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let body = r#"{"actor": {"id": "operator"}, "actor": {"id": "someone"}, "id": "operator"}"#;
    let mut stream = TcpStream::connect(&s.addr).expect("a connection");
    write!(
        stream,
        "POST /v1/get HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("asked");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("the answer");
    assert!(answer.starts_with("HTTP/1.1 400"), "{answer}");
    assert!(answer.contains("the key actor is named twice"), "{answer}");
}

/// A window back from now is positive: a metric's `over` and a
/// diagnostics' `window` of "-5 days" or "0 s" are `400`, where a negative
/// one answered an empty page (fifth production review, ADR-0191 decision
/// 3).
#[test]
fn a_window_back_from_now_is_positive() {
    let file = fresh_store("window");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let op = json!({"id": "operator"});
    for wrong in ["-5 days", "0 s"] {
        for (path, body) in [
            (
                "/v1/metric",
                json!({"actor": op, "name": "Subscription.count", "over": wrong}),
            ),
            (
                "/v1/diagnostics",
                json!({"actor": op, "type": "Subscription", "window": wrong}),
            ),
        ] {
            let (status, out) = call(&s, "POST", path, Some(&body));
            assert_eq!(status, 400, "{path} {wrong}: {out}");
            assert!(out.to_string().contains("a positive duration"), "{out}");
        }
    }
    post(
        &s,
        "/v1/diagnostics",
        &json!({"actor": op, "type": "Subscription", "window": "5 days"}),
    );
}

/// What the service was given and what it found are said at its start: a
/// `--create` over a store that exists, a last stop that was not clean, and
/// an evaluator's token sent over plain http beyond loopback; `/metrics`
/// gives the release and the start time; an argument it does not take is
/// said stamped; and a malformed filter says why (fifth production review,
/// ADR-0191 decision 4).
#[test]
fn what_the_service_was_given_is_said() {
    use std::os::unix::fs::PermissionsExt;
    let file = fresh_store("given");
    let path = file.to_str().expect("a path");
    let token = file.with_file_name("bank.token");
    std::fs::write(&token, "t0ken\n").expect("a token");
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600))
        .expect("its owner's alone");
    let named = format!("bank={}", token.display());
    let local = format!("teller={}", token.display());
    {
        // a write, its frames in the log, and the process killed
        let s = start(&["--store", path, "--create", "--port", "0"]);
        publish(&s, &json!({"payments": PAYMENTS}));
    }
    let s = start(&[
        "--store",
        path,
        "--create",
        "--port",
        "0",
        "--evaluator",
        "bank=http://192.0.2.1:1/verify",
        "--evaluator-token",
        &named,
        "--evaluator",
        "teller=http://127.0.0.1:1/verify",
        "--evaluator-token",
        &local,
    ]);
    let said = s.said_within(" at version ");
    assert!(said.contains("--create was given and"), "{said}");
    assert!(said.contains("the last stop was not clean"), "{said}");
    assert!(
        said.contains("token is sent over plain http to http://192.0.2.1:1/verify"),
        "{said}"
    );
    // a token to this machine's loopback goes unsaid
    assert!(!said.contains("teller's token"), "{said}");
    let mut stream = TcpStream::connect(&s.addr).expect("a connection");
    write!(
        stream,
        "GET /metrics HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    )
    .expect("asked");
    let mut metrics = String::new();
    stream.read_to_string(&mut metrics).expect("the metrics");
    assert!(
        metrics.contains("objectflow_build_info{release=\""),
        "{metrics}"
    );
    assert!(
        metrics.contains("objectflow_start_time_seconds 1"),
        "{metrics}"
    );
    let page = post(
        &s,
        "/v1/query",
        &json!({"actor": {"id": "operator"}, "type": "Transfer", "filter": "((("}),
    );
    assert_eq!(page["refused"], "malformed_filter", "{page}");
    assert!(
        page["reason"]
            .as_str()
            .is_some_and(|r| r.starts_with("it does not parse")),
        "{page}"
    );
    let page = post(
        &s,
        "/v1/metric",
        &json!({"actor": {"id": "operator"}, "name": "Transfer.work_in_progress", "filter": "((("}),
    );
    assert_eq!(page["refused"], "malformed_filter", "{page}");
    assert!(
        page["reason"]
            .as_str()
            .is_some_and(|r| r.starts_with("it does not parse")),
        "{page}"
    );
    // a clean stop, and a start that finds nothing to say of it
    let pid = s.child.as_ref().expect("its process").id();
    let sent = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .expect("kill runs");
    assert!(sent.success());
    let mut s = s;
    let status = s
        .child
        .as_mut()
        .expect("its process")
        .wait()
        .expect("it exits");
    assert!(status.success(), "{status}");
    drop(s);
    let s = start(&["--store", path, "--port", "0"]);
    let said = s.said_within(" at version ");
    assert!(!said.contains("not clean"), "{said}");
    assert!(!said.contains("--create was given"), "{said}");
    // an argument it does not take, said stamped
    let out = Command::new(env!("CARGO_BIN_EXE_objectflow-service"))
        .args(["--nonsense"])
        .output()
        .expect("it runs");
    assert_eq!(out.status.code(), Some(2));
    let first = String::from_utf8_lossy(&out.stderr)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    assert!(
        first.as_bytes().get(4) == Some(&b'-') && first.contains("no argument --nonsense"),
        "{first}"
    );
    // an option where a value belongs, a path forgotten, refused naming
    // both, where `--store --create --create` made a store named `--create`
    // and served it (run 2026-10-16); here ended by a port no parser takes,
    // so the old one exits too, on that, and the sentence tells them apart
    let dir = fresh_store("option-as-value");
    let dir = dir.parent().expect("its directory");
    let out = Command::new(env!("CARGO_BIN_EXE_objectflow-service"))
        .args(["--store", "--create", "--create", "--port", "nope"])
        .current_dir(dir)
        .output()
        .expect("it runs");
    assert_eq!(out.status.code(), Some(2));
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("--store takes a value, and is given the option --create"),
        "{said}"
    );
    assert!(!dir.join("--create").exists(), "a store named --create");
}

/// The metrics listener answers whatever the main one holds: twenty idle
/// connections on the main listener leave `/health` on `--metrics-bind`
/// answering 200, where it was refused 503 busy past sixteen, the count both
/// listeners share compared with the metrics listener's own cap (sixth
/// production review F9).
#[test]
fn the_metrics_listener_answers_beside_a_busy_main_one() {
    let file = fresh_store("metrics-beside");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
        "--metrics-bind",
        "127.0.0.1:0",
    ]);
    let said = s.said_within("metrics and health on http://");
    let metrics_at = said
        .lines()
        .find_map(|l| {
            l.split_once("metrics and health on http://")
                .map(|(_, a)| a.trim().to_string())
        })
        .expect("the metrics address");
    let held: Vec<TcpStream> = (0..20)
        .map(|_| TcpStream::connect(&s.addr).expect("a connection"))
        .collect();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let metrics = Service {
        child: None,
        addr: metrics_at,
        said: std::sync::Arc::new(std::sync::Mutex::new(String::new())),
    };
    for _ in 0..3 {
        let (status, health) = call(&metrics, "GET", "/health", None);
        assert_eq!(status, 200, "{health}");
    }
    drop(held);
}

const FORGOTTEN: &str = "module: forgotten
categories: [live, closed]
types:
  Person:
    description: A person, signing in by email.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human, unique: true, personal: true, optional: true }
    states:
      ON: { category: live, description: Here. }
      OFF: { category: closed, final: true, description: Gone. }
    transitions:
      add: { kind: initial, to: ON, required_inputs: [login] }
      leave: { kind: external, from: ON, to: OFF }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
  Note:
    description: A note, signed by who wrote it.
    tracking: record
    attributes:
      author: { type: identity, optional: true }
      kept_by: { type: identity, optional: true }
    states:
      OPEN: { category: live, description: Open. }
      DONE: { category: closed, final: true, description: Done. }
    transitions:
      write:
        kind: initial
        to: OPEN
        effect:
          - assign: { location: author, expr: actor.id }
          - create: { type: Receipt, transition: issue, inputs: { by: actor.id } }
      keep:
        kind: internal
        from: [OPEN]
        effect:
          - assign: { location: kept_by, expr: author }
          - create: { type: Receipt, transition: issue, inputs: { by: author } }
      sign: { kind: internal, from: [OPEN], inputs: { witness: { type: identity } } }
      close: { kind: external, from: OPEN, to: DONE, proposable: true }
  Receipt:
    description: A receipt of a note, naming who wrote it.
    tracking: record
    attributes:
      by: { type: identity }
    states:
      KEPT: { category: live, description: Kept. }
      GONE: { category: closed, final: true, description: Gone. }
    transitions:
      issue: { kind: initial, to: KEPT, required_inputs: [by], only_via: [Note.write, Note.keep] }
      drop: { kind: external, from: KEPT, to: GONE }
";

const COPIES: &str = "module: copies
categories: [live, closed]
evaluators:
  crm:
    description: The CRM, which says whether it knows a person.
    functions:
      known:
        description: Whether the CRM knows this person.
        arguments:
          who: { type: identity }
types:
  Person:
    description: A person, signing in by email.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human, unique: true, personal: true, optional: true }
    states:
      ON: { category: live, description: Here. }
      OFF: { category: closed, final: true, description: Gone. }
    transitions:
      add: { kind: initial, to: ON, required_inputs: [login] }
      leave: { kind: external, from: ON, to: OFF }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
  Memo:
    description: A memo, by who wrote it, watched and handed on.
    tracking: record
    attributes:
      author: { type: identity, optional: true }
      owner: { type: identity, optional: true }
      watchers: { type: \"identity[]\", optional: true }
    states:
      OPEN: { category: live, description: Open. }
      DONE: { category: closed, final: true, description: Done. }
    conditions:
      known: { description: The CRM knows its author., expression: crm.known(author), remedy: dependent }
      unknown: { description: The CRM does not know its author., expression: not crm.known(author), remedy: dependent }
    transitions:
      write:
        kind: initial
        to: OPEN
        effect:
          - assign: { location: author, expr: actor.id }
      watch:
        kind: internal
        from: [OPEN]
        effect:
          - add: { location: watchers, expr: actor.id }
      hand:
        kind: internal
        from: [OPEN]
        proposable: true
        inputs: { to: { type: identity } }
        effect:
          - assign: { location: owner, expr: inputs.to }
      fix_author: { kind: internal, from: [OPEN], corrects: [author], required_inputs: [author], inputs: { reason: { type: string } } }
      vouch: { kind: internal, from: [OPEN], guards: { known: deny } }
      doubt: { kind: internal, from: [OPEN], guards: { unknown: deny } }
      close: { kind: external, from: OPEN, to: DONE }
";

/// The flow of issue #28: a booking whose days must be in order.
const DESK: &str = "module: desk
categories: [live, closed]
types:
  Booking:
    description: A desk booked for some days.
    tracking: record
    attributes:
      first_day: { type: date }
      last_day: { type: date }
    states:
      BOOKED: { description: Booked., category: live }
      RELEASED: { description: Released., category: closed, final: true }
    conditions:
      days_in_order:
        description: The booking ends on or after the day it starts.
        expression: inputs.last_day >= inputs.first_day
        remedy: self_serviceable
      started:
        description: Not before its first day is past its last.
        expression: first_day > last_day
        remedy: dependent
    transitions:
      book: { kind: initial, to: BOOKED, required_inputs: [first_day, last_day], guards: { days_in_order: deny } }
      release: { kind: external, from: BOOKED, to: RELEASED, guards: { started: deny } }
";

/// An unsatisfied verdict names the inputs its clause reads, in the order
/// its transition declares them, on a request and on a check alike, so a
/// form marks the fields a guard refused as it marks an `invalid_input`'s
/// (issue #28, ADR-0224); a clause over the stored object names none.
#[test]
fn an_unsatisfied_verdict_names_the_inputs_its_clause_reads() {
    let file = fresh_store("clause-inputs");
    let path = file.to_str().expect("a path");
    let s = start(&["--store", path, "--create", "--port", "0"]);
    publish(&s, &json!({"desk": DESK}));
    let op = json!({"id": "operator"});
    let book = |first: &str, last: &str| {
        json!({"actor": op, "type": "Booking", "transition": "book",
               "inputs": {"first_day": first, "last_day": last}})
    };
    for at in ["/v1/request", "/v1/check"] {
        let said = post(&s, at, &book("2026-11-09", "2026-11-02"));
        // a check's verdict beside what it read
        let said = said.get("verdict").cloned().unwrap_or(said);
        assert_eq!(
            (
                said["kind"].as_str(),
                said["clause"].as_str(),
                &said["inputs"]
            ),
            (
                Some("unsatisfied"),
                Some("days_in_order"),
                &json!(["first_day", "last_day"])
            ),
            "{at}: {said}"
        );
    }
    let booked = post(&s, "/v1/request", &book("2026-11-09", "2026-11-10"));
    assert_eq!(booked["kind"], "satisfied", "{booked}");
    let released = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "object_id": booked["object_id"], "transition": "release"}),
    );
    assert_eq!(
        (released["clause"].as_str(), &released["inputs"]),
        (Some("started"), &json!([])),
        "{released}"
    );
    drop(s);
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// An erased actor's identity leaves every copy the store holds of it, in
/// any case of its letters, as identities compare: a set of identities, a
/// value sent in another case, a proposal's inputs, pending and decided, a
/// guard's consulted key in an event and in an attempt, a refused request's
/// input, an unknown actor's attempt, a correction's record, and the
/// approver of a version; no byte of it is in the store's file or its
/// `-wal` once the log is cut back, where the first audit found each (the
/// audit's personal data S16 to S23, ADR-0205 decision 6).
#[test]
fn an_erased_identity_leaves_every_copy() {
    let (url, _, _) = bank();
    let file = fresh_store("copies");
    let path = file.to_str().expect("a path");
    let crm = format!("crm={url}");
    let s = start(&[
        "--store",
        path,
        "--create",
        "--port",
        "0",
        "--evaluator",
        &crm,
    ]);
    publish(&s, &json!({"copies": COPIES}));
    let email = "ann.secret@corp.example";
    let op = json!({"id": "operator"});
    let me = json!({"id": email});
    let ask = |actor: &Value, body: Value| -> Value {
        let mut body = body;
        body["actor"] = actor.clone();
        post(&s, "/v1/request", &body)
    };
    // before she is known, a request in her name, in another case
    let unknown = ask(
        &json!({"id": "Ann.Secret@Corp.Example"}),
        json!({"type": "Memo", "transition": "write"}),
    );
    assert_eq!(unknown["clause"], "actor_known", "{unknown}");
    let ann = ask(
        &op,
        json!({"type": "Person", "transition": "add", "inputs": {"login": email}}),
    )["object_id"]
        .as_str()
        .expect("the person")
        .to_string();
    let memo = ask(&me, json!({"type": "Memo", "transition": "write"}))["object_id"]
        .as_str()
        .expect("the memo")
        .to_string();
    let took = |actor: &Value, body: Value| {
        let v = ask(actor, body);
        assert_eq!(v["kind"], "satisfied", "{v}");
        v
    };
    took(&me, json!({"object_id": memo, "transition": "watch"}));
    took(&me, json!({"object_id": memo, "transition": "vouch"}));
    let doubted = ask(&me, json!({"object_id": memo, "transition": "doubt"}));
    assert_eq!(doubted["kind"], "unsatisfied", "{doubted}");
    took(
        &op,
        json!({"object_id": memo, "transition": "hand", "inputs": {"to": "ANN.SECRET@CORP.EXAMPLE"}}),
    );
    // a proposal she withdraws keeps what it was filed with, and a second
    // pending one of the same transition is refused (ADR-0211 decision 5)
    let first = took(
        &me,
        json!({"object_id": memo, "transition": "hand", "inputs": {"to": email}, "propose": true}),
    )["object_id"]
        .as_str()
        .expect("the proposal")
        .to_string();
    took(&me, json!({"object_id": first, "transition": "withdraw"}));
    let second = took(
        &me,
        json!({"object_id": memo, "transition": "hand", "inputs": {"to": "Ann.Secret@corp.EXAMPLE"}, "propose": true}),
    )["object_id"]
        .as_str()
        .expect("the proposal")
        .to_string();
    took(&op, json!({"object_id": second, "transition": "approve"}));
    took(
        &op,
        json!({"object_id": memo, "transition": "fix_author", "inputs": {"author": "someone@else.example", "reason": "mistyped"}}),
    );
    took(&op, json!({"object_id": memo, "transition": "close"}));
    let late = ask(
        &op,
        json!({"object_id": memo, "transition": "hand", "inputs": {"to": email}}),
    );
    assert_eq!(late["kind"], "unavailable", "{late}");
    // she approves the next version
    let drafted = took(
        &op,
        json!({"type": "DeclarationChange", "transition": "draft",
               "inputs": {"source": {"copies": COPIES.replace("A memo, by who", "A memo, by whoever")},
                          "source_ref": "test:copies"}}),
    );
    let change = drafted["object_id"]
        .as_str()
        .expect("the change")
        .to_string();
    let submitted = took(&op, json!({"object_id": change, "transition": "submit"}));
    let approved = post(
        &s,
        "/v1/publish",
        &json!({"actor": me, "change": change, "expected_version": submitted["version"]}),
    );
    assert_eq!(approved["verdict"]["kind"], "satisfied", "{approved}");
    let erased = took(
        &op,
        json!({"object_id": ann, "transition": "forget", "inputs": {"reason": "asked"}}),
    );
    assert_eq!(erased["kind"], "satisfied", "{erased}");
    // what the reads answer names her object
    let got = post(&s, "/v1/get", &json!({"actor": op, "id": memo}));
    assert_eq!(got["attributes"]["watchers"], json!([ann]), "{got}");
    assert_eq!(got["attributes"]["owner"], json!(ann), "{got}");
    // and no byte of her address, in any case, once the log is cut back
    let wal = format!("{path}-wal");
    let holds = || {
        [path, wal.as_str()].iter().any(|f| {
            std::fs::read(f)
                .unwrap_or_default()
                .windows(email.len())
                .any(|w| w.eq_ignore_ascii_case(email.as_bytes()))
        })
    };
    let began = std::time::Instant::now();
    while holds() && began.elapsed() < std::time::Duration::from_secs(15) {
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let conn = rusqlite::Connection::open(&file).expect("the store");
    let mut left = Vec::new();
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
        .and_then(|mut q| q.query_map([], |r| r.get(0)).and_then(|r| r.collect()))
        .expect("the tables");
    // where it is left, by table, to say on a failure
    for t in tables {
        let Ok(mut q) = conn.prepare(&format!("SELECT * FROM \"{t}\"")) else {
            continue;
        };
        let columns = q.column_count();
        let Ok(mut rows) = q.query([]) else {
            continue;
        };
        let mut found = 0;
        while let Ok(Some(row)) = rows.next() {
            for i in 0..columns {
                let text = match row.get::<_, rusqlite::types::Value>(i) {
                    Ok(rusqlite::types::Value::Text(t)) => t,
                    Ok(rusqlite::types::Value::Blob(b)) => String::from_utf8_lossy(&b).into_owned(),
                    _ => continue,
                };
                if text.to_ascii_lowercase().contains(email) {
                    found += 1;
                }
            }
        }
        if found > 0 {
            left.push(format!("{t}: {found}"));
        }
    }
    drop(conn);
    assert!(
        !holds(),
        "her address is still in the store's files; rows: {left:?}"
    );
    drop(s);
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

const STAFFED: &str = "module: staffed
categories: [live, closed]
types:
  Person:
    description: A person, by name.
    tracking: record
    attributes:
      name: { type: string, personal: true, optional: true }
    states:
      ON: { category: live, description: Here. }
      OFF: { category: closed, final: true, description: Gone. }
    transitions:
      join: { kind: initial, to: ON, required_inputs: [name] }
      leave: { kind: external, from: ON, to: OFF }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
  Desk:
    description: A desk.
    tracking: record
    attributes:
      label: { type: string }
    states:
      FREE: { category: live, description: Free. }
      GONE: { category: closed, final: true, description: Gone. }
    transitions:
      place: { kind: initial, to: FREE, required_inputs: [label] }
      remove: { kind: external, from: FREE, to: GONE }
";

/// A type a version drops keeps its objects readable, after a restart as
/// before, and keeps its erasure, so the personal data its objects hold can
/// still be erased, where no erasure could reach it once its type was
/// dropped (the audit's personal data S14, ADR-0205).
#[test]
fn a_dropped_types_objects_stay_readable_and_erasable() {
    let file = fresh_store("dropped");
    let path = file.to_str().expect("a path");
    let s = start(&["--store", path, "--create", "--port", "0"]);
    publish(&s, &json!({"staffed": STAFFED}));
    let op = json!({"id": "operator"});
    let secret = "Retired Person Secret";
    let person = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "Person", "transition": "join", "inputs": {"name": secret}}),
    )["object_id"]
        .as_str()
        .expect("the person")
        .to_string();
    // a live person's type retired on purpose, as a version must say
    // (ADR-0229 decision 1)
    let without = format!(
        "{}  Desk:\n{}migration:\n  description: Staff are no longer kept.\n  retired_types:\n    Person: Staff are kept elsewhere from now on.\n",
        &STAFFED[..STAFFED.find("  Person:").expect("Person")],
        &STAFFED[STAFFED.find("  Desk:\n").expect("Desk") + "  Desk:\n".len()..]
    );
    assert!(
        !without.contains("  Person:\n") && without.contains("Desk"),
        "{without}"
    );
    publish(&s, &json!({"staffed": without}));
    let got = post(&s, "/v1/get", &json!({"actor": op, "id": person}));
    assert_eq!(got["attributes"]["name"], secret, "{got}");
    drop(s);
    let s = start(&["--store", path, "--port", "0"]);
    let got = post(&s, "/v1/get", &json!({"actor": op, "id": person}));
    assert_eq!(got["attributes"]["name"], secret, "after a restart: {got}");
    let erased = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "object_id": person, "transition": "forget", "inputs": {"reason": "asked"}}),
    );
    assert_eq!(erased["kind"], "satisfied", "{erased}");
    let left = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "object_id": person, "transition": "leave"}),
    );
    assert_eq!(left["kind"], "unknown_transition", "{left}");
    let got = post(&s, "/v1/get", &json!({"actor": op, "id": person}));
    assert!(got["attributes"]["name"].is_null(), "{got}");
    drop(s);
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// An erased actor's identity leaves the store: an email marked personal,
/// erased with the person, is in no event, attempt, interval, idempotency
/// record or proposal, which name the person's object instead, and neither
/// the store's file nor its `-wal` holds it once the log is cut back (sixth
/// production review ERASED-IDENTITY, ADR-0192 decision 4); nor in an
/// attribute an effect set to `actor.id`, nor in the input a creation was
/// passed it as, where a note's author kept the address (fifteenth
/// production review, ADR-0202 decision 1); nor where another actor copied
/// it, a note's `kept_by` and a receipt's `by` taken from its author
/// (sixteenth production review, ADR-0203 decision 1).
#[test]
fn an_erased_actors_identity_leaves_the_store() {
    let file = fresh_store("forgotten");
    let path = file.to_str().expect("a path");
    let s = start(&["--store", path, "--create", "--port", "0"]);
    publish(&s, &json!({"forgotten": FORGOTTEN}));
    let email = "ann.secret@corp.example";
    let op = json!({"id": "operator"});
    let me = json!({"id": email});
    let ann = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "Person", "transition": "add", "inputs": {"login": email}}),
    )["object_id"]
        .as_str()
        .expect("the person")
        .to_string();
    // a write under a key, a proposal, and a refusal, each recording her
    let note = post(
        &s,
        "/v1/request",
        &json!({"actor": me, "type": "Note", "transition": "write", "idempotency_key": "k-1"}),
    )["object_id"]
        .as_str()
        .expect("the note")
        .to_string();
    let filed = post(
        &s,
        "/v1/request",
        &json!({"actor": me, "object_id": note, "transition": "close", "propose": true}),
    );
    assert_eq!(filed["kind"], "satisfied", "{filed}");
    let done = post(
        &s,
        "/v1/request",
        &json!({"actor": me, "type": "Note", "transition": "write"}),
    )["object_id"]
        .as_str()
        .expect("a second note")
        .to_string();
    post(
        &s,
        "/v1/request",
        &json!({"actor": me, "object_id": done, "transition": "close"}),
    );
    let refused = post(
        &s,
        "/v1/request",
        &json!({"actor": me, "object_id": done, "transition": "close"}),
    );
    assert_eq!(refused["kind"], "unavailable", "{refused}");
    // the operator copies her identity, from the note's author, into the
    // note and into a receipt's input
    let kept = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "object_id": note, "transition": "keep"}),
    );
    assert_eq!(kept["kind"], "satisfied", "{kept}");
    // and a client sends it as an input of type identity
    let signed = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "object_id": note, "transition": "sign", "inputs": {"witness": email}}),
    );
    assert_eq!(signed["kind"], "satisfied", "{signed}");
    let erased = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "object_id": ann, "transition": "forget", "inputs": {"reason": "asked"}}),
    );
    assert_eq!(erased["kind"], "satisfied", "{erased}");
    let conn = rusqlite::Connection::open(&file).expect("the store");
    let count = |sql: &str, v: &str| -> i64 { conn.query_row(sql, [v], |r| r.get(0)).expect(sql) };
    for sql in [
        "SELECT count(*) FROM of_event WHERE actor_id = ?1 OR actor_principal = ?1",
        "SELECT count(*) FROM of_attempt WHERE actor_id = ?1 OR actor_principal = ?1",
        "SELECT count(*) FROM of_interval WHERE entered_by = ?1",
        "SELECT count(*) FROM of_idempotency WHERE actor_id = ?1",
        "SELECT count(*) FROM t_proposal WHERE proposer = ?1",
    ] {
        assert_eq!(count(sql, email), 0, "{sql}");
    }
    for sql in [
        "SELECT count(*) FROM t_note WHERE author = ?1",
        "SELECT count(*) FROM t_note WHERE kept_by = ?1",
        "SELECT count(*) FROM t_receipt WHERE by = ?1",
        "SELECT count(*) FROM of_event WHERE json_extract(payload, '$.inputs.by') = ?1",
        "SELECT count(*) FROM of_event WHERE json_extract(payload, '$.inputs.witness') = ?1",
    ] {
        assert_eq!(count(sql, email), 0, "{sql}");
        assert!(count(sql, &ann) >= 1, "{sql}");
    }
    assert!(count("SELECT count(*) FROM of_event WHERE actor_id = ?1", &ann) >= 4);
    assert!(count("SELECT count(*) FROM of_attempt WHERE actor_id = ?1", &ann) >= 1);
    drop(conn);
    // and the bytes: the log cut back after the erasure, within seconds
    let wal = format!("{path}-wal");
    let holds = || {
        [path, wal.as_str()].iter().any(|f| {
            std::fs::read(f)
                .unwrap_or_default()
                .windows(email.len())
                .any(|w| w == email.as_bytes())
        })
    };
    let began = std::time::Instant::now();
    while holds() && began.elapsed() < std::time::Duration::from_secs(15) {
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    assert!(
        !holds(),
        "the store's file or its -wal still holds the email"
    );
}

const NOTED: &str = "module: noted
categories: [live, closed]
types:
  Person:
    description: A person.
    tracking: record
    attributes:
      login: { type: identity, actor_kind: human, unique: true }
    states:
      ON: { category: live, description: Here. }
      OFF: { category: closed, final: true, description: Gone. }
    transitions:
      add: { kind: initial, to: ON, required_inputs: [login] }
      leave: { kind: external, from: ON, to: OFF }
  Note:
    description: A note.
    tracking: record
    attributes:
      text: { type: string, personal: true, optional: true }
    states:
      OPEN: { category: live, description: Open. }
      DONE: { category: closed, final: true, description: Done. }
    transitions:
      write: { kind: initial, to: OPEN, optional_inputs: [text] }
      close: { kind: external, from: OPEN, to: DONE }
      forget: { kind: erasure, inputs: { reason: { type: string } } }
";

/// The sixth review's minors the service says: an ill-typed filter is
/// refused `malformed_filter` with its reason, neither the answer nor the
/// log holding the stored value it met, where it answered 501 and wrote a
/// person's text into the log; a key sent by an actor named by its object
/// and then by its identity is one request; a store the service cannot
/// write is refused at the start; and a second service on a store is said
/// at its start (ADR-0192 decision 7).
#[test]
fn what_a_filter_a_key_and_a_store_are_told() {
    let file = fresh_store("noted");
    let path = file.to_str().expect("a path");
    let s = start(&["--store", path, "--create", "--port", "0"]);
    publish(&s, &json!({"noted": NOTED}));
    let op = json!({"id": "operator"});
    let secret = "Dana's secret plan";
    post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "Note", "transition": "write", "inputs": {"text": secret}}),
    );
    for filter in [
        "text",
        "text + 1 > 0",
        "state + 1 > 0",
        "text == \"x\" and text",
        "not text",
    ] {
        // opted into, as a filter on a personal member is (ADR-0206)
        let page = post(
            &s,
            "/v1/query",
            &json!({"actor": op, "type": "Note", "filter": filter, "reads_personal": true}),
        );
        assert_eq!(page["refused"], "malformed_filter", "{filter}: {page}");
        assert!(
            page["reason"].as_str().is_some_and(|r| !r.is_empty()),
            "{page}"
        );
        assert!(!page.to_string().contains("Dana"), "{page}");
    }
    assert!(!s.said().contains("Dana"), "{}", s.said());
    // one key, the actor named by its object and then by its identity
    let pat = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "Person", "transition": "add", "inputs": {"login": "pat"}}),
    )["object_id"]
        .as_str()
        .expect("a person")
        .to_string();
    let first = post(
        &s,
        "/v1/request",
        &json!({"actor": {"id": pat}, "type": "Note", "transition": "write", "idempotency_key": "k-1"}),
    );
    let again = post(
        &s,
        "/v1/request",
        &json!({"actor": {"id": "pat"}, "type": "Note", "transition": "write", "idempotency_key": "k-1"}),
    );
    assert_eq!(
        (again["object_id"].clone(), again["replayed"].clone()),
        (first["object_id"].clone(), json!(true)),
        "{again}"
    );
    // a second service on the store is said at its start
    let second = start(&["--store", path, "--port", "0"]);
    let said = second.said_within(" at version ");
    assert!(said.contains("another service has the store"), "{said}");
    drop(second);
    drop(s);
    // a store the service cannot write is refused at the start
    let locked = fresh_store("readonly");
    let locked_path = locked.to_str().expect("a path");
    drop(start(&["--store", locked_path, "--create", "--port", "0"]));
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o400))
            .expect("made read-only");
    }
    // within ten seconds, or killed and failed, never waited on for good
    let mut child = Command::new(env!("CARGO_BIN_EXE_objectflow-service"))
        .args(["--store", locked_path, "--port", "0"])
        .stderr(Stdio::piped())
        .spawn()
        .expect("it runs");
    let began = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("its status") {
            break Some(status);
        }
        if began.elapsed() > std::time::Duration::from_secs(10) {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let mut said = String::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut said);
    }
    assert!(
        status.is_some_and(|s| !s.success()),
        "a store it cannot write was served: {said}"
    );
    assert!(said.contains("is not writable by the service"), "{said}");
    // a directory named as the store is said to be one, where it was told
    // to get write permission (eleventh production review)
    let dir = locked.parent().expect("its directory");
    let out = Command::new(env!("CARGO_BIN_EXE_objectflow-service"))
        .args(["--store", dir.to_str().expect("a path"), "--port", "0"])
        .output()
        .expect("it runs");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{said}");
    assert!(
        said.contains("is a directory; --store names the store's file in it"),
        "{said}"
    );
}

const TILLS: &str = "module: tills
categories: [live, closed]
types:
  Till:
    description: A till due back some days after it opened.
    tracking: record
    attributes:
      opened_at: { type: timestamp }
      days: { type: int }
    states:
      OPEN: { category: live, description: Open. }
      CLOSED: { category: closed, final: true, description: Closed. }
    derived_attributes:
      due_back:
        description: When the till is due back.
        expression: opened_at + days * 1 day
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [opened_at, days] }
      close: { kind: external, from: OPEN, to: CLOSED }
";

/// A derived time past the calendar refuses the read `value_in_range`,
/// naming the object and no value, a query's fields and a `get` alike,
/// where a query answered year 10239 (sixth production review, ADR-0192
/// decision 7) and then null (ADR-0205 decision 2); the store's other
/// objects are read as before.
#[test]
fn a_derived_time_past_the_calendar_refuses_the_read() {
    let file = fresh_store("tills");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    publish(&s, &json!({"tills": TILLS}));
    let op = json!({"id": "operator"});
    let mut tills = Vec::new();
    for days in [30, 3_000_000] {
        let v = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "Till", "transition": "open",
                    "inputs": {"opened_at": "2026-10-13T00:00:00Z", "days": days}}),
        );
        tills.push(v["object_id"].as_str().expect("its id").to_string());
    }
    let refused = |answer: &Value| {
        assert_eq!(answer["refused"], "value_in_range", "{answer}");
        assert_eq!(answer["object_id"], json!(tills[1]), "{answer}");
        assert!(!answer.to_string().contains("10239"), "{answer}");
    };
    refused(&post(
        &s,
        "/v1/query",
        &json!({"actor": op, "type": "Till", "fields": ["due_back"]}),
    ));
    refused(&post(&s, "/v1/get", &json!({"actor": op, "id": tills[1]})));
    let first = post(&s, "/v1/get", &json!({"actor": op, "id": tills[0]}));
    assert_eq!(
        first["attributes"]["due_back"], "2026-11-12T00:00:00Z",
        "{first}"
    );
}

/// A store readable by others is said at its start with its `-wal` and
/// `-shm`, which SQLite makes with the store's mode, where the warning
/// named the store alone (twelfth production review).
#[test]
fn a_store_readable_by_others_is_said_with_its_log() {
    use std::os::unix::fs::PermissionsExt;
    let file = fresh_store("readable");
    let path = file.to_str().expect("a path");
    drop(start(&["--store", path, "--create", "--port", "0"]));
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).expect("0644");
    let s = start(&["--store", path, "--port", "0"]);
    let said = s.said_within("readable by others");
    assert!(
        said.contains("readable by others than its owner (mode 644)")
            && said.contains("and its -wal and -shm"),
        "{said}"
    );
    drop(s);
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// A read selecting by a personal value is refused `personal` unless its
/// request opts in with `reads_personal`, whoever asks, a person and the
/// operator as an agent, where an agent's query alone was refused, by its
/// kind: a query's filter and order, a lookup by a personal identifier,
/// `available`, `availability` and `check` decided by a personal member, a
/// metric's filter and its binding of a person; and a request's refusal by
/// a personal value names its holder only when opted into (ADR-0206; the
/// audit's personal data R3 to R7, R15 to R17).
#[test]
fn a_read_by_a_personal_value_is_opted_into() {
    let file = fresh_store("personal-query");
    let path = file.to_str().expect("a path");
    let s = start(&["--store", path, "--create", "--port", "0"]);
    let people = "module: people\ncategories: [live, closed]\ntypes:\n  Person:\n    description: A person.\n    tracking: record\n    attributes:\n      login: { type: identity, actor_kind: human }\n      name: { type: string, personal: true, optional: true, unique: true }\n      badge: { type: string, personal: true, optional: true, external: badges }\n      team: { type: string }\n    states:\n      ON: { category: live, description: On. }\n      OFF: { category: closed, final: true, description: Off. }\n    conditions:\n      named: { description: Named., expression: name is not null, remedy: self_serviceable }\n    transitions:\n      join: { kind: initial, to: ON, required_inputs: [login, team], optional_inputs: [name, badge] }\n      promote: { kind: internal, from: [ON], guards: { named: deny } }\n      leave: { kind: external, from: ON, to: OFF }\n      forget: { kind: erasure, inputs: { reason: { type: string } } }\n    metrics:\n      by_team: { description: People by team., source: objects, item: p, dimensions: { team: p.team }, expression: count() }\n      by_login: { description: People by login., source: objects, item: p, dimensions: { who: p.login }, expression: count() }\n  Bot:\n    description: An agent.\n    tracking: record\n    attributes:\n      key: { type: identity, actor_kind: agent }\n    states:\n      ON: { category: live, description: On. }\n      OFF: { category: closed, final: true, description: Off. }\n    transitions:\n      add: { kind: initial, to: ON, required_inputs: [key] }\n      retire: { kind: external, from: ON, to: OFF }\n";
    publish(&s, &json!({"people": people}));
    let op = json!({"id": "operator"});
    let v = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "Person", "transition": "join",
                "inputs": {"login": "ana", "name": "Ana Secret", "badge": "B-77", "team": "blue"}}),
    );
    assert_eq!(v["kind"], "satisfied", "{v}");
    let ana = v["object_id"].as_str().expect("her id").to_string();
    let v = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "Bot", "transition": "add", "inputs": {"key": "helper"}}),
    );
    assert_eq!(v["kind"], "satisfied", "{v}");
    let opted = |mut body: Value| {
        body["reads_personal"] = json!(true);
        body
    };
    // each read, by anyone, refused without the opt-in and answered with it
    for actor in [json!({"id": "helper"}), json!({"id": "ana"}), op.clone()] {
        for (path, body) in [
            (
                "/v1/query",
                json!({"actor": actor, "type": "Person", "filter": "name >= \"Ana\""}),
            ),
            (
                "/v1/query",
                json!({"actor": actor, "type": "Person", "order": "name desc"}),
            ),
            (
                "/v1/lookup",
                json!({"actor": actor, "source": "badges", "value": "B-77"}),
            ),
            (
                "/v1/available",
                json!({"actor": actor, "type": "Person", "transition": "promote"}),
            ),
            ("/v1/availability", json!({"actor": actor, "id": ana})),
            (
                "/v1/check",
                json!({"actor": actor, "object_id": ana, "transition": "promote"}),
            ),
            (
                "/v1/check_batch",
                json!({"actor": actor, "requests": [{"object_id": ana, "transition": "promote"}]}),
            ),
            // a target not held yet, judged by every transition of its name
            (
                "/v1/check_batch",
                json!({"actor": actor, "requests": [{"object_id": "person-999", "transition": "promote"}]}),
            ),
            (
                "/v1/metric",
                json!({"actor": actor, "name": "Person.by_team", "filter": "p.name >= \"A\""}),
            ),
            (
                "/v1/metric",
                json!({"actor": actor, "name": "Person.by_login", "bind": {"who": "ana"}}),
            ),
        ] {
            let refused = post(&s, path, &body);
            assert_eq!(refused["refused"], "personal", "{path} {body}: {refused}");
            let answered = post(&s, path, &opted(body.clone()));
            assert_ne!(answered["refused"], "personal", "{path} {body}: {answered}");
            assert!(!answered.is_null(), "{path}: {answered}");
        }
    }
    // a read by what is not personal is answered as it was
    let page = post(
        &s,
        "/v1/query",
        &json!({"actor": {"id": "helper"}, "type": "Person", "filter": "team == \"blue\""}),
    );
    assert_eq!(page["refused"], Value::Null, "{page}");
    // a request's refusal by a personal value names its holder when opted
    // into alone
    let again = json!({"actor": op, "type": "Person", "transition": "join",
                       "inputs": {"login": "anna", "name": "Ana Secret", "team": "red"}});
    let refused = post(&s, "/v1/request", &again);
    assert_eq!(refused["kind"], "invariant_violated", "{refused}");
    assert!(!refused.to_string().contains(&ana), "{refused}");
    let named = post(&s, "/v1/request", &opted(again));
    assert!(named.to_string().contains(&ana), "{named}");
    drop(s);
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// A query's and a metric's filters take their values as `params`, an
/// object by name, at the wire: each bound as the member or the row's field
/// it meets is typed, a parameter params does not give refused
/// `malformed_filter` naming it, and `params` that is no object refused
/// `400` (ADR-0218, ADR-0221), where the service's reading of the key was
/// tested by no request.
#[test]
fn a_reads_parameters_are_taken_at_the_wire() {
    let file = fresh_store("read-params");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let desks = "module: desks\ncategories: [live, closed]\ntypes:\n  Booking:\n    description: A desk booked for a day.\n    tracking: record\n    attributes:\n      day: { type: date }\n    states:\n      HELD: { category: live, description: Held. }\n      DONE: { category: closed, final: true, description: Done. }\n    transitions:\n      book: { kind: initial, to: HELD, required_inputs: [day] }\n      end: { kind: external, from: HELD, to: DONE }\n    metrics:\n      booked: { description: Bookings., source: objects, item: b, expression: count() }\n";
    publish(&s, &json!({"desks": desks}));
    let op = json!({"id": "operator"});
    for day in ["2026-11-08", "2026-11-09", "2026-11-10"] {
        let v = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "Booking", "transition": "book", "inputs": {"day": day}}),
        );
        assert_eq!(v["kind"], "satisfied", "{v}");
    }
    let between = json!({"from": "2026-11-09", "to": "2026-11-10"});
    let page = post(
        &s,
        "/v1/query",
        &json!({"actor": op, "type": "Booking", "filter": "day >= :from and day <= :to", "params": between}),
    );
    assert_eq!(page["items"].as_array().map(Vec::len), Some(2), "{page}");
    let metric = post(
        &s,
        "/v1/metric",
        &json!({"actor": op, "name": "Booking.booked", "keep": [],
                "filter": "b.day >= :from and b.day <= :to", "params": between}),
    );
    assert_eq!(metric["rows"][0]["value"], json!(2), "{metric}");
    // a parameter params does not give, named in the refusal
    for (path, body) in [
        (
            "/v1/query",
            json!({"actor": op, "type": "Booking", "filter": "day >= :from", "params": {}}),
        ),
        (
            "/v1/metric",
            json!({"actor": op, "name": "Booking.booked", "filter": "b.day >= :from", "params": {}}),
        ),
    ] {
        let refused = post(&s, path, &body);
        assert_eq!(refused["refused"], "malformed_filter", "{path}: {refused}");
        assert!(refused.to_string().contains(":from"), "{path}: {refused}");
    }
    // params that is no object
    for (path, body) in [
        (
            "/v1/query",
            json!({"actor": op, "type": "Booking", "filter": "day >= :from", "params": ["2026-11-09"]}),
        ),
        (
            "/v1/metric",
            json!({"actor": op, "name": "Booking.booked", "filter": "b.day >= :from", "params": "2026-11-09"}),
        ),
    ] {
        let (status, said) = call(&s, "POST", path, Some(&body));
        assert_eq!(status, 400, "{path}: {said}");
    }
    drop(s);
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
}

/// A flow numbering its claims for an accounts system, as the audit's
/// durability B4 found a restore drawing them again.
const NUMBERED: &str = "
module: numbered
categories: [live, closed]
sequences:
  claim_number: { description: The numbers the accounts system keeps claims by. }
types:
  Claim:
    description: A claim, numbered for the accounts system.
    tracking: record
    attributes:
      number: { type: string, identifier: { sequence: claim_number, format: \"EXP-{n}\" }, unique: true }
    states:
      OPEN: { category: live, description: Open. }
      PAID: { category: closed, final: true, description: Paid. }
    transitions:
      file: { kind: initial, to: OPEN }
      pay: { kind: external, from: OPEN, to: PAID }
";

/// A backup taken while the service writes, marked as `operate.md`'s
/// commands mark it, and restored after the live store went on: the
/// restored store hands out no object id, claim number or position the
/// live store handed out after the copy, refuses a subscriber's cursor from
/// that period by `pull`, `export` and `acknowledge` once its log has passed
/// it, says the restore at its start, and restores it once (the audit's
/// durability B4 and B5, ADR-0207).
#[test]
fn a_restored_copy_hands_out_nothing_its_copy_never_saw() {
    let file = fresh_store("copy-restored");
    let path = file.to_str().expect("a path");
    let s = start(&["--store", path, "--create", "--port", "0"]);
    publish(&s, &json!({"numbered": NUMBERED}));
    let op = json!({"id": "operator"});
    let filed = |s: &Service| {
        let v = post(
            s,
            "/v1/request",
            &json!({"actor": op, "type": "Claim", "transition": "file"}),
        );
        assert_eq!(v["kind"], "satisfied", "{v}");
        let id = v["object_id"].as_str().expect("its id").to_string();
        let got = post(s, "/v1/get", &json!({"actor": op, "id": id}));
        let number = got["attributes"]["number"]
            .as_str()
            .expect("its number")
            .to_string();
        (id, number)
    };
    let tail = |text: &str| -> i64 {
        text.rsplit('-')
            .next()
            .and_then(|n| n.parse().ok())
            .expect("a number")
    };
    for _ in 0..3 {
        filed(&s);
    }
    let subscription = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "Subscription", "transition": "subscribe",
                "inputs": {"target_type": "Claim"}}),
    )["object_id"]
        .as_str()
        .expect("the subscription")
        .to_string();
    // the backup, taken while the service runs, and marked
    let copy = file.with_file_name("copy.sqlite");
    rusqlite::Connection::open(&file)
        .expect("the store")
        .execute("VACUUM INTO ?1", [copy.to_str().expect("a path")])
        .expect("the backup");
    rusqlite::Connection::open(&copy)
        .expect("the copy")
        .execute(
            "INSERT INTO of_counter (name, value) VALUES ('copied', 1)",
            [],
        )
        .expect("the mark");
    // the period the restore loses: claims an integration kept, and the
    // cursor a subscriber kept
    let lost: Vec<(String, String)> = (0..4).map(|_| filed(&s)).collect();
    let page = post(
        &s,
        "/v1/pull",
        &json!({"actor": op, "subscription": subscription}),
    );
    let cursor = |page: &Value| -> i64 {
        page["settled"]
            .as_str()
            .and_then(|c| c.parse().ok())
            .expect("a cursor")
    };
    let kept = cursor(&page);
    drop(s);
    // restored as operate.md says: the copy in the store's place, with
    // neither -wal nor -shm beside it
    for end in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{path}{end}"));
    }
    std::fs::rename(&copy, &file).expect("the copy restored");
    let s = start(&["--store", path, "--port", "0"]);
    assert!(
        s.said_within("is a restored copy")
            .contains("is a restored copy"),
        "{}",
        s.said()
    );
    let (id, number) = filed(&s);
    for (was, numbered) in &lost {
        // an id the lost period handed out is never handed out again: a
        // deployment's ids are UUIDv7s, the restored store's seed its own
        // (ADR-0226), where a numbered id was carried past the margin
        assert_ne!(&id, was, "the restored store handed out {id} again");
        assert!(tail(&number) > tail(numbered), "{number} after {numbered}");
    }
    // its log now past the kept cursor, which is still none of its own
    let pulled = post(
        &s,
        "/v1/pull",
        &json!({"actor": op, "subscription": subscription}),
    );
    assert!(cursor(&pulled) > kept, "{pulled}");
    for (route, body) in [
        (
            "/v1/pull",
            json!({"actor": op, "subscription": subscription, "cursor": kept.to_string()}),
        ),
        (
            "/v1/export",
            json!({"actor": op, "source": "log", "cursor": kept.to_string()}),
        ),
        (
            "/v1/export",
            json!({"actor": op, "source": "Claim.transitions", "cursor": kept.to_string()}),
        ),
        (
            "/v1/acknowledge",
            json!({"actor": op, "subscription": subscription, "cursor": kept.to_string()}),
        ),
    ] {
        let answer = post(&s, route, &body);
        assert_eq!(answer["refused"], "invalid_cursor", "{route}: {answer}");
    }
    // and an `.attempts` cursor in the transactions' gap, the one below
    // where the start said they go on from
    let said = s.said();
    let txn_from: i64 = said
        .split("transactions from ")
        .nth(1)
        .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse().ok())
        .expect("where transactions go on from");
    let attempts = post(
        &s,
        "/v1/export",
        &json!({"actor": op, "source": "Claim.attempts", "cursor": (txn_from - 1).to_string()}),
    );
    assert_eq!(attempts["refused"], "invalid_cursor", "{attempts}");
    // a cursor the copy held is its own
    let from_copy = post(
        &s,
        "/v1/pull",
        &json!({"actor": op, "subscription": subscription, "cursor": "1"}),
    );
    assert!(from_copy["refused"].is_null(), "{from_copy}");
    drop(s);
    // restored once: started again, it says nothing of a restore
    let s = start(&["--store", path, "--port", "0"]);
    let (again, again_numbered) = filed(&s);
    assert_ne!(again, id);
    // the claim numbers go on by one, no margin added a second time
    assert_eq!(tail(&again_numbered), tail(&number) + 1);
    assert!(!s.said().contains("is a restored copy"), "{}", s.said());
}

/// A store the service finds lost is served no more until a restart, every
/// later read and write answered 503 and the stop's checkpoint skipped:
/// the file replaced and then put back, the write-ahead log removed, and
/// the file written over in place, where writes went on answering 200 into
/// a log no start would read (the audit's durability S3 and S4, D688,
/// ADR-0205 decision 8).
#[test]
fn a_store_found_lost_is_served_no_more_until_a_restart() {
    let op = json!({"id": "operator"});
    let add = |login: &str| {
        json!({"actor": op, "type": "User", "transition": "add",
               "inputs": {"login": login, "name": "A person"}})
    };
    let refused_as = |s: &Service, code: &str| {
        let (status, fault) = call(s, "POST", "/v1/request", Some(&add("late")));
        assert_eq!(
            (status, fault["code"].as_str()),
            (503, Some(code)),
            "{fault}\n{}",
            s.said()
        );
        let (status, read) = call(
            s,
            "POST",
            "/v1/get",
            Some(&json!({"actor": op, "id": "operator"})),
        );
        assert_eq!((status, read["code"].as_str()), (503, Some(code)), "{read}");
        let (status, health) = call(s, "GET", "/health", None);
        assert_eq!(
            (status, health["code"].as_str()),
            (503, Some(code)),
            "{health}"
        );
    };
    for (case, code) in [
        ("put-back", "store_replaced"),
        ("log-removed", "store_replaced"),
        ("written-over", "store_unreadable"),
    ] {
        let file = fresh_store(&format!("lost-{case}"));
        let path = file.to_str().expect("a path");
        let s = start(&["--store", path, "--create", "--port", "0"]);
        publish_service(&s);
        assert_eq!(post(&s, "/v1/request", &add("ana"))["kind"], "satisfied");
        match case {
            "put-back" => {
                let moved = file.with_file_name("moved.sqlite");
                std::fs::rename(&file, &moved).expect("moved");
                std::fs::copy(&moved, &file).expect("a copy in its place");
                refused_as(&s, code);
                // its own file put back, it is served no more all the same
                std::fs::rename(&moved, &file).expect("put back");
            }
            "log-removed" => {
                std::fs::remove_file(format!("{path}-wal")).expect("the log removed");
            }
            _ => {
                use std::io::{Seek, Write};
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .open(&file)
                    .expect("the file");
                f.seek(std::io::SeekFrom::Start(0)).expect("its start");
                f.write_all(&[0u8; 100]).expect("written over");
            }
        }
        refused_as(&s, code);
        refused_as(&s, code);
        // asked to stop, it carries no log into a lost store's file
        let pid = s.child.as_ref().expect("its process").id();
        let sent = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .expect("kill runs");
        assert!(sent.success());
        let said = s.said_within("stopped");
        assert!(
            said.contains("the store is served no more until a restart")
                && said.contains("was not carried into the store's file, the store being lost"),
            "{case}: {said}"
        );
    }
}

/// A start refused for a busy address changes neither the store's file nor
/// its log, where it had turned a restored copy to WAL, made its indexes
/// and folded its log before binding (the audit's durability C3); a store
/// due an upgrade that fails SQLite's quick check is refused, its files as
/// they were, where it was upgraded and served unchecked (C4); and a stop
/// whose socket is already gone says so and still stops, carrying the log
/// into the file, where it ended in an error (C7).
#[test]
fn a_refused_start_and_a_stop_leave_the_store_whole() {
    // a port another holds
    let file = fresh_store("held-port");
    let path = file.to_str().expect("a path");
    {
        let s = start(&["--store", path, "--create", "--port", "0"]);
        publish_service(&s);
    }
    let taken = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = taken.local_addr().expect("its address").port().to_string();
    let wal = format!("{path}-wal");
    let before = (
        std::fs::read(&file).expect("the file"),
        std::fs::read(&wal).unwrap_or_default(),
    );
    let (code, said) = refused(&["--store", path, "--port", &port]);
    assert_eq!(code, Some(1), "{said}");
    assert_eq!(
        (
            std::fs::read(&file).expect("the file"),
            std::fs::read(&wal).unwrap_or_default()
        ),
        before,
        "the refused start changed the store: {said}"
    );
    // a store of 0.2.0 due an upgrade, a page of it written over
    let (damaged, _) = release_fixture("0.2.0", "servicedesk", "damaged");
    {
        use std::io::{Seek, Write};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&damaged)
            .expect("the copy");
        f.seek(std::io::SeekFrom::Start(4096 * 3)).expect("a page");
        f.write_all(&[0x5a; 4096]).expect("written over");
    }
    let before = std::fs::read(&damaged).expect("the copy");
    let (code, said) = refused(&["--store", damaged.to_str().expect("a path"), "--port", "0"]);
    assert_eq!(code, Some(1), "{said}");
    assert!(said.contains("quick check"), "{said}");
    assert_eq!(std::fs::read(&damaged).expect("the copy"), before);
    // a stop whose socket is gone
    let socket = file.with_file_name("service.sock");
    let s = start_on_socket(&[
        "--store",
        path,
        "--socket",
        socket.to_str().expect("a path"),
    ]);
    std::fs::remove_file(&socket).expect("the socket removed");
    let pid = s.child.as_ref().expect("its process").id();
    let sent = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .expect("kill runs");
    assert!(sent.success());
    let said = s.said_within("stopped");
    assert!(
        said.contains("was not removed") && said.contains("stopped"),
        "{said}"
    );
    assert!(
        std::fs::read(&wal).map_or(true, |l| l.is_empty()),
        "the log not carried in"
    );
}

/// A service started on a Unix socket, once it says where it listens.
fn start_on_socket(args: &[&str]) -> Service {
    let mut child = Command::new(env!("CARGO_BIN_EXE_objectflow-service"))
        .args(args)
        .stderr(Stdio::piped())
        .spawn()
        .expect("the service starts");
    let mut stderr = BufReader::new(child.stderr.take().expect("its stderr"));
    let mut line = String::new();
    stderr.read_line(&mut line).expect("its first line");
    assert!(line.contains(" listening on unix:"), "{line}");
    Service {
        child: Some(child),
        addr: String::new(),
        said: drained(stderr),
    }
}

/// A flow tracking a ticket's priority and size, its priority corrected.
const TICKETS: &str = "
module: tickets
categories: [live, closed]
enumerations:
  Priority: [LOW, HIGH]
  Size: [S, L]
types:
  Ticket:
    description: A ticket.
    tracking: record
    attributes:
      priority: { type: Priority }
      size: { type: Size, optional: true }
    states:
      OPEN: { category: live, description: Open. }
      DONE: { category: closed, final: true, description: Done. }
    transitions:
      raise: { kind: initial, to: OPEN, required_inputs: [priority], optional_inputs: [size] }
      escalate: { kind: internal, from: [OPEN], required_inputs: [priority] }
      fix_priority: { kind: internal, from: [OPEN], corrects: [priority], required_inputs: [priority], inputs: { reason: { type: string } } }
      close: { kind: external, from: [OPEN], to: DONE }
";

/// The `.intervals` export, folded by each row's key as a consumer folds
/// it, holds what the store holds after every commit: a correction merging
/// a span into the one before it retracts the merged span, a version
/// renaming a tracked member retracts its rows under the old name and gives
/// them again under the new, and one retyping a member retracts the rows it
/// sets aside, where a consumer kept each (the audit's durability V7a to V8,
/// ADR-0205 decision 9).
#[test]
fn the_intervals_export_retracts_what_a_later_commit_deleted_or_moved() {
    let file = fresh_store("retracted");
    let path = file.to_str().expect("a path");
    let s = start(&["--store", path, "--create", "--port", "0"]);
    publish(&s, &json!({"tickets": TICKETS}));
    let op = json!({"id": "operator"});
    let took = |body: Value| {
        let v = post(&s, "/v1/request", &body);
        assert_eq!(v["kind"], "satisfied", "{v}");
        v["object_id"].as_str().map(str::to_string)
    };
    let ticket = took(json!({"actor": op, "type": "Ticket", "transition": "raise",
                             "inputs": {"priority": "LOW", "size": "S"}}))
    .expect("a ticket");
    took(
        json!({"actor": op, "object_id": ticket, "transition": "escalate",
                "inputs": {"priority": "HIGH"}}),
    );
    type Key = (String, String, i64, String);
    let mut held: std::collections::BTreeMap<Key, Value> = std::collections::BTreeMap::new();
    let mut cursor = Value::Null;
    let mut fold = |held: &mut std::collections::BTreeMap<Key, Value>| loop {
        let page = post(
            &s,
            "/v1/export",
            &json!({"actor": op, "source": "Ticket.intervals", "cursor": cursor}),
        );
        for line in page["lines"].as_array().expect("its lines") {
            let row: Value = serde_json::from_str(line.as_str().expect("a line")).expect("JSON");
            let key = (
                row["object_id"].as_str().expect("its object").to_string(),
                row["dimension"]
                    .as_str()
                    .expect("its dimension")
                    .to_string(),
                row["entered_position"]
                    .as_i64()
                    .expect("its entered position"),
                row["entered_at"]
                    .as_str()
                    .expect("its entered time")
                    .to_string(),
            );
            if row["retracted"] == json!(true) {
                held.remove(&key);
            } else {
                held.insert(key, row["left_at"].clone());
            }
        }
        cursor = page["cursor"].clone();
        if page["more"] != json!(true) {
            break;
        }
    };
    let stored = || -> std::collections::BTreeMap<Key, Value> {
        let conn = rusqlite::Connection::open(&file).expect("the store");
        let mut q = conn
            .prepare(
                "SELECT object_id, dimension, entered_position, entered_at, left_at FROM of_interval \
                 WHERE legacy = 0 AND instr(dimension, '__v') = 0 AND object_id IN (SELECT id FROM of_object WHERE type = 'Ticket')",
            )
            .expect("its intervals");
        q.query_map([], |r| {
            Ok((
                (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?),
                r.get::<_, Option<String>>(4)?
                    .map_or(Value::Null, Value::String),
            ))
        })
        .expect("read")
        .collect::<Result<_, _>>()
        .expect("rows")
    };
    fold(&mut held);
    assert_eq!(held, stored());
    // corrected back, the high span merged into the low one before it
    took(
        json!({"actor": op, "object_id": ticket, "transition": "fix_priority",
                "inputs": {"priority": "LOW", "reason": "mistyped"}}),
    );
    fold(&mut held);
    assert_eq!(held, stored());
    // a version renaming priority and retyping size
    let v2 = TICKETS
        .replace(
            "      priority: { type: Priority }",
            "      urgency: { type: Priority }",
        )
        .replace("[priority]", "[urgency]")
        .replace(
            "      size: { type: Size, optional: true }",
            "      size: { type: string, optional: true }",
        )
        + "migration:\n  description: Priority is urgency, and a size is text.\n  renamed_attributes:\n    Ticket: { priority: urgency }\n";
    publish(&s, &json!({"tickets": v2}));
    fold(&mut held);
    assert_eq!(held, stored());
    assert!(
        held.keys().any(|k| k.1 == "urgency") && !held.keys().any(|k| k.1 == "priority"),
        "{held:?}"
    );
}

/// A backup restored beside the store's own later log, the restore's second
/// step left out: the start takes the log in and says so, and following
/// its note word for word serves the backup's state, where its advice, to
/// remove the log the start had already taken in, left the restore undone
/// (the guides' audit G-83).
#[test]
fn a_restore_undone_by_a_later_log_is_done_again_as_the_start_says() {
    let file = fresh_store("undone");
    let path = file.to_str().expect("a path");
    let op = json!({"id": "operator"});
    let add = |s: &Service, login: &str| {
        let v = post(
            s,
            "/v1/request",
            &json!({"actor": op, "type": "User", "transition": "add",
                    "inputs": {"login": login, "name": "A person"}}),
        );
        assert_eq!(v["kind"], "satisfied", "{v}");
        v["object_id"].as_str().expect("its id").to_string()
    };
    // stopped as a supervisor stops it, waiting for its process to end, as
    // `systemctl stop` does, since its connections close after it says it
    // stopped
    let stopped = |mut s: Service| {
        let pid = s.child.as_ref().expect("its process").id();
        let sent = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .expect("kill runs");
        assert!(sent.success());
        let said = s.said_within("stopped");
        let _ = s.child.as_mut().expect("its process").wait();
        said
    };
    let backup = file.with_file_name("backup.sqlite");
    let kept = {
        let s = start(&["--store", path, "--create", "--port", "0"]);
        publish_service(&s);
        let kept = add(&s, "kept");
        stopped(s);
        std::fs::copy(&file, &backup).expect("the backup");
        kept
    };
    let later = {
        let s = start(&["--store", path, "--port", "0"]);
        let later = add(&s, "later");
        // killed, its log holding the later write
        drop(s);
        later
    };
    // restored, the log left beside it
    std::fs::copy(&backup, &file).expect("restored");
    let s = start(&["--store", path, "--port", "0"]);
    let note = s.said_within("this start has taken the later log into it");
    assert!(
        note.contains("this start has taken the later log into it"),
        "{note}"
    );
    // as the note says: stop, remove the -wal and -shm, the backup again, start
    stopped(s);
    for end in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{path}{end}"));
    }
    std::fs::copy(&backup, &file).expect("restored again");
    let s = start(&["--store", path, "--port", "0"]);
    let get = |id: &str| post(&s, "/v1/get", &json!({"actor": op, "id": id}));
    // the backup's state: what it held, and nothing written after it
    assert_eq!(get(&kept)["id"], json!(kept));
    assert_eq!(get(&later), Value::Null, "the restore undone");
}

/// A backup's copy of an earlier release, marked as a 0.3.0 guide marks
/// one, is restored by its upgrade, before the upgrade records its act, and
/// the upgrade says so (ADR-0207 decision 7).
#[test]
fn a_marked_copy_due_an_upgrade_is_restored_at_it() {
    let (path, _) = release_fixture("0.2.0", "reimbursement", "marked");
    let held: i64 = {
        let conn = rusqlite::Connection::open(&path).expect("the copy");
        conn.execute(
            "INSERT INTO of_counter (name, value) VALUES ('copied', 1)",
            [],
        )
        .expect("the mark");
        conn.query_row(
            "SELECT value FROM of_counter WHERE name = 'position'",
            [],
            |r| r.get(0),
        )
        .expect("its position")
    };
    let s = start(&["--store", path.to_str().expect("a path"), "--port", "0"]);
    let op = json!({"id": "operator"});
    let done = post(&s, "/v1/upgrade", &json!({"actor": op}));
    assert_eq!(done["upgraded"], json!(true), "{done}");
    let restored = &done["restored"];
    assert_eq!(restored["positions"][0], json!(held), "{done}");
    assert!(
        restored["margin"].as_i64().expect("its margin") >= 1_000,
        "{done}"
    );
    // the upgrade's own act past the gap
    let history = post(&s, "/v1/history", &json!({"actor": op, "id": "operator"}));
    let act = history["events"]
        .as_array()
        .and_then(|e| e.last())
        .cloned()
        .unwrap_or_default();
    assert_eq!(act["transition"], "upgrade");
    assert!(
        act["position"].as_i64().expect("its position")
            > restored["positions"][1].as_i64().expect("the gap's end"),
        "{act} {done}"
    );
}

/// A sandbox's load checks the flow first, as a publish does but for its
/// examples, and refuses one the checker refuses with its findings, where
/// it installed it and the first request reaching what the checker names
/// faulted `500` with a store error (issue #26); `unchecked` installs it as
/// it was, and a flow the checker passes loads with its notices counted.
#[test]
fn a_sandbox_load_refuses_what_the_checker_refuses() {
    const ROOMS: &str = "module: rooms\ncategories: [live, closed]\ntypes:\n  Booking:\n    description: A room booked, with whom to call about it.\n    tracking: record\n    attributes:\n      title: { type: string }\n      contact: { type: string, personal: true }\n    states:\n      HELD: { description: Held., category: live }\n      RELEASED: { description: Released., category: closed, final: true }\n    transitions:\n      request: { kind: initial, to: HELD, required_inputs: [title], optional_inputs: [contact] }\n      release: { kind: external, from: HELD, to: RELEASED }\n      forget: { kind: erasure, inputs: { reason: { type: string } } }\n";
    let s = start(&["--port", "0"]);
    let files = json!([{"name": "rooms.yaml", "text": ROOMS}]);
    let refused = post(&s, "/sandbox/load", &json!({"files": files}));
    assert_eq!(refused["loaded"], json!(false), "{refused}");
    let findings = refused["findings"].as_array().expect("findings");
    assert!(
        findings.iter().any(|f| f["file"] == "rooms.yaml"
            && f["message"].as_str().is_some_and(|m| m.contains("contact"))),
        "{refused}"
    );
    // nothing installed: a request is refused for no module, not run
    let (status, _) = call(
        &s,
        "POST",
        "/v1/request",
        Some(
            &json!({"actor": {"id": "operator"}, "type": "Booking", "transition": "request", "inputs": {"title": "Standup"}}),
        ),
    );
    assert_ne!(status, 200);
    // installed as written where asked, as before
    let unchecked = post(
        &s,
        "/sandbox/load",
        &json!({"files": files, "unchecked": true}),
    );
    assert_eq!(unchecked["loaded"], json!(true), "{unchecked}");
    // and a flow the checker passes loads, its notices counted
    let people = post(
        &s,
        "/sandbox/load",
        &json!({"files": [{"name": "people.yaml", "text": example("people.yaml")}]}),
    );
    assert_eq!(people["loaded"], json!(true), "{people}");
    assert!(people["notices"].is_u64(), "{people}");
    let (status, bad) = call(
        &s,
        "POST",
        "/sandbox/load",
        Some(&json!({"files": files, "unchecked": "yes"})),
    );
    assert_eq!(status, 400, "unchecked is a boolean: {bad}");
}

/// A sandbox refuses what is a deployment's or what it would take in an
/// order of its own: `--busy-timeout`, the clock moved by both keys at
/// once, and an evaluator's answer other than the three, where each was
/// taken (the guides' audit G-46).
#[test]
fn a_sandbox_refuses_what_it_cannot_honour() {
    let (code, said) = refused(&["--port", "0", "--busy-timeout", "1 s"]);
    assert_eq!(code, Some(2), "{said}");
    assert!(said.contains("a deployment's"), "{said}");
    let s = start(&["--port", "0"]);
    let loaded = post(
        &s,
        "/sandbox/load",
        &json!({"files": [{"name": "people.yaml", "text": example("people.yaml")}]}),
    );
    assert_eq!(loaded["loaded"], json!(true), "{loaded}");
    let (status, both) = call(
        &s,
        "POST",
        "/sandbox/clock",
        Some(&json!({"advance": "1 h", "set": "2030-01-01T00:00:00Z"})),
    );
    assert_eq!(status, 400, "{both}");
    for (answer, status) in [("maybe", 400), ("satisfied", 200)] {
        let (got, said) = call(
            &s,
            "POST",
            "/sandbox/evaluators",
            Some(&json!({"answers": {"bank.known": answer}})),
        );
        assert_eq!(got, status, "{answer}: {said}");
    }
}

/// Each key `wire-v1.json` records as an operation's required one is
/// required: a body holding every other required key and not that one is
/// answered `400`, as `wire.md`'s tables say, where they marked `actor`
/// optional on operations that refused a request without it (the guides'
/// audit G-93).
#[test]
fn every_required_key_is_required() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/guides/wire-v1.json");
    let baseline: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("wire-v1.json")).expect("JSON");
    let required = baseline["required"].as_object().expect("the required keys");
    let file = fresh_store("required");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    let sample = |key: &str| match key {
        "actor" => json!({"id": "operator"}),
        "positions" => json!([1]),
        "batch" => json!({"objects": []}),
        "task" => json!({"name": "prune_attempts", "before": "2020-01-01T00:00:00Z"}),
        "window" => json!("30 days"),
        "cursor" => json!("1"),
        _ => json!("x"),
    };
    let mut wrong = Vec::new();
    for (op, keys) in required {
        let keys: Vec<&str> = keys
            .as_array()
            .expect("its keys")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        for left in &keys {
            let body: serde_json::Map<String, Value> = keys
                .iter()
                .filter(|k| *k != left)
                .map(|k| (k.to_string(), sample(k)))
                .collect();
            let (status, said) = call(&s, "POST", op, Some(&Value::Object(body)));
            if status != 400 {
                wrong.push(format!("{op} without {left} answered {status}: {said}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Each shipped sample set publishes with its examples beside it, as
/// `integrate.md` drafts a change: drafted, submitted, which runs every
/// example in the dry run, and approved, a set's later version over its
/// first. The service's examples carried two whose `publish` step reads a
/// file no change has, refused `example_not_run`, while the checker passed
/// them (the guides' audit G-18).
#[test]
fn every_sample_publishes_with_its_examples() {
    // each file keyed by the name it declares, as a change's source is
    let declared = |file: &str| -> (String, Value) {
        let text = example(file);
        let key = if file.ends_with(".examples.yaml") {
            "examples_for: "
        } else {
            "module: "
        };
        let name = text
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .unwrap_or_else(|| panic!("{file} declares no name"))
            .trim()
            .to_string();
        let name = if key == "module: " {
            name
        } else {
            format!("{name}.examples")
        };
        (name, Value::String(text))
    };
    let sets: &[&[&[&str]]] = &[
        &[&["people.yaml"]],
        &[&["approvals.yaml", "approvals.examples.yaml"]],
        &[&["bookings.yaml", "bookings.examples.yaml"]],
        &[&["reimbursement.yaml", "reimbursement.examples.yaml"]],
        &[
            &["inventory.yaml", "inventory.examples.yaml"],
            &["inventory-v2.yaml", "inventory-v2.examples.yaml"],
        ],
        &[&[
            "people.yaml",
            "inventory.yaml",
            "delivery.yaml",
            "delivery.examples.yaml",
        ]],
        &[
            &["people.yaml", "service.yaml", "service.examples.yaml"],
            &["people.yaml", "service-v2.yaml", "service-v2.examples.yaml"],
        ],
        &[&["people.yaml", "customers.yaml", "customers.examples.yaml"]],
        &[&[
            "people.yaml",
            "servicedesk.yaml",
            "servicedesk.examples.yaml",
        ]],
        &[&["people.yaml", "issues.yaml", "issues.examples.yaml"]],
    ];
    for (n, versions) in sets.iter().enumerate() {
        let file = fresh_store(&format!("sample-{n}"));
        let s = start(&[
            "--store",
            file.to_str().expect("a path"),
            "--create",
            "--port",
            "0",
        ]);
        for files in *versions {
            let source: serde_json::Map<String, Value> =
                files.iter().map(|f| declared(f)).collect();
            publish(&s, &Value::Object(source));
        }
    }
}

/// Requests sent at once commit together (ADR-0208): every one answered as
/// it would be alone, each its own object, and fewer commits than requests,
/// as `/metrics` counts them.
#[test]
fn requests_sent_at_once_commit_together() {
    let path = fresh_store("grouped");
    let s = start(&[
        "--store",
        path.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    publish_service(&s);
    let metric = |name: &str| -> u64 {
        let mut stream = TcpStream::connect(&s.addr).expect("a connection");
        write!(
            stream,
            "GET /metrics HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
        )
        .expect("the request is written");
        let mut text = String::new();
        stream.read_to_string(&mut text).expect("the metrics");
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{name} ")))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or_else(|| panic!("no {name}"))
    };
    let groups = metric("objectflow_request_groups_total");
    let held = metric("objectflow_request_grouped_total");
    let made: Vec<Value> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..40)
            .map(|i| {
                let s = &s;
                scope.spawn(move || {
                    post(
                        s,
                        "/v1/request",
                        &json!({"actor": {"id": "operator"}, "type": "User", "transition": "add",
                                "inputs": {"login": format!("u{i}"), "name": format!("U {i}")}}),
                    )
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("answered"))
            .collect()
    });
    let ids: std::collections::BTreeSet<&str> = made
        .iter()
        .map(|v| {
            assert_eq!(v["kind"], "satisfied", "{v}");
            v["object_id"].as_str().expect("an id")
        })
        .collect();
    assert_eq!(ids.len(), 40);
    let groups = metric("objectflow_request_groups_total") - groups;
    let held = metric("objectflow_request_grouped_total") - held;
    assert_eq!(held, 40);
    assert!(
        groups < held,
        "{groups} commits for {held} requests: none was shared"
    );
}

/// An upgrade, its dry run too, is refused while a flow version the store
/// records does not load with this release's loader, naming the version,
/// and the store stays at its release, which reads it, where it was carried
/// on and every request touching that version answered 500 (the audit's
/// durability U4).
#[test]
fn an_upgrade_is_refused_over_a_version_this_release_cannot_read() {
    let op = json!({"id": "operator"});
    let (path, recorded) = release_fixture("0.2.0", "reimbursement", "unreadable");
    {
        let conn = rusqlite::Connection::open(&path).expect("the store");
        let changed = conn
            .execute(
                "UPDATE of_declaration SET source = json_set(source, '$.reimbursement', 'types: [') WHERE version = 1",
                [],
            )
            .expect("the source changed");
        assert_eq!(changed, 1);
    }
    let s = start(&["--store", path.to_str().expect("a path"), "--port", "0"]);
    for dry_run in [true, false] {
        let (status, said) = call(
            &s,
            "POST",
            "/v1/upgrade",
            Some(&json!({"actor": op, "dry_run": dry_run})),
        );
        assert_eq!(status, 409, "{said}");
        assert!(
            said["fault"]
                .as_str()
                .unwrap_or_default()
                .contains("version 1's source no longer loads"),
            "{said}"
        );
    }
    let (_, health) = call(&s, "GET", "/health", None);
    assert_eq!(
        (&health["upgrade_due"], &health["store_release"]),
        (&json!(true), &recorded["release"])
    );
}

/// An import batch killed while it runs leaves all of it or none, and sent
/// again after the restart writes what was missing once, never a second
/// copy (the audit's durability W5). Killed at three moments, each on a
/// store of its own: while the writer holds the batch's transaction, which
/// keeps none; long after it committed, which keeps all; and early, either.
#[test]
fn an_import_killed_midway_leaves_all_or_none() {
    killed_imports("", &|trial| {
        let file = fresh_store(&format!("import-kill-{trial}"));
        let args = [
            "--store",
            file.to_str().expect("a path"),
            "--create",
            "--port",
            "0",
        ];
        Some((args.map(str::to_string).to_vec(), None))
    });
}

/// On PostgreSQL alike, and the killed service's session gone with it, so
/// its restart takes the store's lock (durability on PostgreSQL 12, where
/// no test killed a service on PostgreSQL).
#[test]
fn an_import_killed_midway_on_postgresql_leaves_all_or_none() {
    killed_imports(" on PostgreSQL", &|trial| {
        served_on_postgresql(&format!("importkill{trial}"))
            .map(|(admin, args, schema)| (args, Some((admin, schema))))
    });
}

/// A store for each trial, the arguments that make it and, on
/// PostgreSQL, the database and the schema to drop after.
type Trials = dyn Fn(
    usize,
) -> Option<(
    Vec<String>,
    Option<(objectflow_core::db::Connection, String)>,
)>;

fn killed_imports(on: &str, store: &Trials) {
    let op = json!({"id": "operator"});
    let batch: Vec<Value> = (0..3000)
        .map(|i| {
            json!({"type": "Employee", "legacy_key": format!("E-{i}"), "state": "EMPLOYED",
                   "attributes": {"employee_id": format!("{i}"), "department": "Operations"}})
        })
        .collect();
    let body = json!({"actor": op, "batch": {"objects": batch}});
    let count = |s: &Service| -> usize {
        let mut n = 0;
        let mut cursor = Value::Null;
        loop {
            let page = post(
                s,
                "/v1/query",
                &json!({"actor": op, "type": "Employee", "limit": 1000, "cursor": cursor}),
            );
            n += page["items"].as_array().map_or(0, Vec::len);
            if page["cursor"].is_null() {
                return n;
            }
            cursor = page["cursor"].clone();
        }
    };
    // the moment of each kill: the writer held, past the commit, early
    enum At {
        Held,
        After,
        Early,
    }
    for (trial, at) in [At::Held, At::After, At::Early].into_iter().enumerate() {
        let Some((made, postgres)) = store(trial) else {
            return;
        };
        let made: Vec<&str> = made.iter().map(String::as_str).collect();
        let again: Vec<&str> = made.iter().copied().filter(|a| *a != "--create").collect();
        let s = start(&made);
        publish(
            &s,
            &json!({"people": example("people.yaml"), "servicedesk": example("servicedesk.yaml")}),
        );
        let addr = s.addr.clone();
        let sent = body.to_string();
        // the batch sent by hand, its answer read where it comes: the kill
        // ends it but where it is waited for, after the commit
        let mut sender = Some(std::thread::spawn(move || {
            let mut answer = String::new();
            if let Ok(mut stream) = TcpStream::connect(&addr) {
                let _ = write!(
                    stream,
                    "POST /v1/import_batch HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sent}",
                    sent.len()
                );
                let _ = stream.read_to_string(&mut answer);
            }
            answer
        }));
        match at {
            At::Held => {
                let began = std::time::Instant::now();
                while call(&s, "GET", "/health", None).1["writer_held_seconds"]
                    .as_f64()
                    .unwrap_or(0.0)
                    <= 0.0
                {
                    assert!(
                        began.elapsed() < std::time::Duration::from_secs(20),
                        "the writer was never seen held"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
            // its answer, which the service gives once the batch committed,
            // where it slept 8 s and a slower machine's import on
            // PostgreSQL, about 6 s here, was killed within its transaction
            At::After => {
                let answer = sender
                    .take()
                    .map(|h| h.join().expect("the sender"))
                    .unwrap_or_default();
                assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
            }
            At::Early => std::thread::sleep(std::time::Duration::from_millis(80)),
        }
        drop(s);
        if let Some(h) = sender {
            let _ = h.join();
        }
        let s = start(&again);
        let held = count(&s);
        match at {
            At::Held => assert_eq!(held, 0, "killed in the transaction, some kept{on}"),
            At::After => assert_eq!(held, 3000, "killed after the commit, some lost{on}"),
            At::Early => assert!(held == 0 || held == 3000, "{held} of 3000 kept{on}"),
        }
        let sent = post(&s, "/v1/import_batch", &body);
        assert!(sent["refusal"].is_null(), "{sent}");
        assert_eq!(sent["written"], json!(3000 - held), "{sent}");
        assert_eq!(count(&s), 3000);
        drop(s);
        if let Some((admin, schema)) = postgres {
            admin
                .execute_batch(&format!("DROP SCHEMA {schema} CASCADE"))
                .expect("the schema dropped");
        }
    }
}

/// A batch killed, failing on its store or panicking at any of its
/// requests or at its commit keeps none of it after, the service going on
/// to apply it whole; one killed once its answer came keeps all of it
/// (ADR-0228 decision 13). Only where the service has the core's faults,
/// as the gate's `--all-features` builds it.
#[cfg(feature = "faults")]
#[test]
fn a_batch_killed_failing_or_panicking_at_any_point_keeps_all_or_none() {
    batch_faults("", &|| {
        let file = fresh_store("batch-faults");
        let args = [
            "--store",
            file.to_str().expect("a path"),
            "--create",
            "--port",
            "0",
        ];
        Some((args.map(str::to_string).to_vec(), None))
    });
}

/// On PostgreSQL alike, the killed service's session ending its batch's
/// transaction rolled back.
#[cfg(feature = "faults")]
#[test]
fn a_batch_killed_failing_or_panicking_on_postgresql_keeps_all_or_none() {
    batch_faults(" on PostgreSQL", &|| {
        served_on_postgresql("batchfaults")
            .map(|(admin, args, schema)| (args, Some((admin, schema))))
    });
}

/// The service started with a fault made to happen at a point of a batch.
#[cfg(feature = "faults")]
fn start_faulted(args: &[&str], fault: &str) -> Service {
    let mut service = Command::new(env!("CARGO_BIN_EXE_objectflow-service"));
    service.args(args).env("OBJECTFLOW_FAULT", fault);
    start_command(service)
}

#[cfg(feature = "faults")]
type FaultStore = dyn Fn() -> Option<(
    Vec<String>,
    Option<(objectflow_core::db::Connection, String)>,
)>;

#[cfg(feature = "faults")]
fn batch_faults(on: &str, store: &FaultStore) {
    let Some((made, postgres)) = store() else {
        return;
    };
    let made: Vec<&str> = made.iter().map(String::as_str).collect();
    let again: Vec<&str> = made.iter().copied().filter(|a| *a != "--create").collect();
    let op = json!({"id": "operator"});
    let add = |login: &str| json!({"type": "User", "transition": "add", "inputs": {"login": login, "name": "U"}});
    let body = json!({"actor": op, "requests": [add("ann"), add("bea"), add("cy")]});
    let users = |s: &Service| -> usize {
        post(
            s,
            "/v1/query",
            &json!({"actor": op, "type": "User", "limit": 1000}),
        )["items"]
            .as_array()
            .map_or(0, Vec::len)
    };
    let mut held = {
        let s = start(&made);
        publish_service(&s);
        users(&s)
    };
    for point in ["0", "1", "2", "commit"] {
        // killed while it holds the writer there
        {
            let s = start_faulted(&again, &format!("batch_hold:{point}"));
            let (addr, sent) = (s.addr.clone(), body.to_string());
            let sender = std::thread::spawn(move || {
                if let Ok(mut stream) = TcpStream::connect(&addr) {
                    let _ = write!(
                        stream,
                        "POST /v1/batch HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sent}",
                        sent.len()
                    );
                    let _ = stream.read_to_string(&mut String::new());
                }
            });
            let said = s.said_within("a batch held at its");
            assert!(
                said.contains(&format!("a batch held at its {point}, for a kill")),
                "{said}"
            );
            drop(s);
            let _ = sender.join();
        }
        let s = start(&again);
        assert_eq!(users(&s), held, "killed at {point}, some kept{on}");
        drop(s);
        // a store fault, then a panic, there: answered 500, none kept, and
        // the writer recovered, which a request alone shows
        for kind in ["batch_error", "batch_panic"] {
            let s = start_faulted(&again, &format!("{kind}:{point}"));
            let (status, said) = call(&s, "POST", "/v1/batch", Some(&body));
            assert_eq!(status, 500, "{kind} at {point}{on}: {said}");
            assert_eq!(users(&s), held, "{kind} at {point}, some kept{on}");
            let alone = post(
                &s,
                "/v1/request",
                &json!({"actor": op, "type": "User", "transition": "add",
                        "inputs": {"login": format!("{kind}-{point}"), "name": "U"}}),
            );
            assert_eq!(alone["kind"], "satisfied", "{kind} at {point}{on}: {alone}");
            held += 1;
        }
    }
    // killed once its answer came: all of it kept
    {
        let s = start(&again);
        let applied = post(&s, "/v1/batch", &body);
        assert_eq!(applied["applied"], true, "{applied}");
    }
    let s = start(&again);
    assert_eq!(
        users(&s),
        held + 3,
        "killed after the commit, some lost{on}"
    );
    drop(s);
    if let Some((admin, schema)) = postgres {
        admin
            .execute_batch(&format!("DROP SCHEMA {schema} CASCADE"))
            .expect("the schema dropped");
    }
}

/// A satisfied check names no event and no id of its simulation, each said
/// not computed, which the next real request takes for what it writes; a
/// check on an object names the object, which is real (the audit's write
/// path A-05).
#[test]
fn a_satisfied_check_names_nothing_it_simulated() {
    let path = fresh_store("check-names");
    let s = start(&[
        "--store",
        path.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    publish_service(&s);
    let op = json!({"id": "operator"});
    let add = json!({"actor": op, "type": "User", "transition": "add",
                     "inputs": {"login": "ana", "name": "Ana"}});
    let checked = post(&s, "/v1/check", &add)["verdict"].clone();
    assert_eq!(
        (
            &checked["kind"],
            &checked["object_id"],
            &checked["events"],
            &checked["not_computed"]
        ),
        (
            &json!("satisfied"),
            &Value::Null,
            &json!([]),
            &json!(["events", "object_id"])
        ),
        "{checked}"
    );
    let ana = post(&s, "/v1/request", &add)["object_id"].clone();
    let checked = post(
        &s,
        "/v1/check",
        &json!({"actor": op, "object_id": ana, "transition": "leave"}),
    )["verdict"]
        .clone();
    assert_eq!(
        (&checked["object_id"], &checked["not_computed"]),
        (&ana, &json!(["events"])),
        "{checked}"
    );
}

/// `maintain`'s `rebuild_indexes` rebuilds every index once an erasure has
/// been recorded since the last rebuild, one at a time, so an erased value
/// an index page's free space kept is gone, and does nothing otherwise; an
/// actor the store does not know is refused as `maintain` refuses one
/// (ADR-0204 decision 9.5, the audit's personal data F3).
#[test]
fn indexes_are_rebuilt_after_an_erasure() {
    let file = fresh_store("rebuild");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    publish(
        &s,
        &json!({"people": example("people.yaml"), "servicedesk": example("servicedesk.yaml")}),
    );
    let op = json!({"id": "operator"});
    let rebuild = |actor: &Value| {
        post(
            &s,
            "/v1/maintain",
            &json!({"actor": actor, "task": {"name": "rebuild_indexes"}}),
        )
    };
    assert_eq!(rebuild(&op), json!({"moved": 0, "refused": null}));
    let imported = post(
        &s,
        "/v1/import_batch",
        &json!({"actor": op, "batch": {"objects": [{"type": "Employee", "legacy_key": "E-1",
                "state": "EMPLOYED", "attributes": {"employee_id": "1", "name": "Ana Example"}}]}}),
    );
    assert_eq!(imported["written"], json!(1), "{imported}");
    let id = post(
        &s,
        "/v1/lookup",
        &json!({"source": "legacy", "value": "Employee:E-1"}),
    )["id"]
        .clone();
    let erased = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "object_id": id, "transition": "forget", "inputs": {"reason": "asked"}}),
    );
    assert_eq!(erased["kind"], "satisfied", "{erased}");
    let done = rebuild(&op);
    assert!(done["refused"].is_null(), "{done}");
    assert!(done["moved"].as_u64().unwrap_or(0) > 10, "{done}");
    assert_eq!(rebuild(&op), json!({"moved": 0, "refused": null}));
    assert_eq!(
        rebuild(&json!({"id": "nobody"})),
        json!({"moved": 0, "refused": "actor_known"})
    );
    let got = post(&s, "/v1/get", &json!({"actor": op, "id": id}));
    assert_eq!(got["state"], json!("EMPLOYED"), "{got}");
}

/// A request naming no type of the installed version is answered
/// `unknown_type`, as the reads are, unreachable from here where no version
/// is installed, since only the operator's publish helps; it was answered
/// `unknown_transition`, naming only the transition (E3, ADR-0210 decision
/// 6).
#[test]
fn a_request_naming_no_type_is_answered_unknown_type() {
    let file = fresh_store("unknown-type");
    let path = file.to_str().expect("a path");
    let s = start(&["--store", path, "--create", "--port", "0"]);
    let v = post(
        &s,
        "/v1/request",
        &json!({"actor": {"id": "operator"}, "type": "Claim", "transition": "draft"}),
    );
    assert_eq!(
        (v["kind"].as_str(), v["type"].as_str(), v["remedy"].as_str()),
        (
            Some("unknown_type"),
            Some("Claim"),
            Some("unreachable_from_here")
        ),
        "{v}"
    );
}

/// Which file hashes the store still holds, which the application asks of
/// its blob store's inventory to delete the bytes of the rest: a photo two
/// people share held until the last is erased, a hash the store never
/// held named by nothing, and the hashes taken as `events` takes its
/// positions (ADR-0211 decision 1).
#[test]
fn a_store_says_which_files_it_still_holds() {
    const PICS: &str = "module: pics\ncategories: [live, closed]\ntypes:\n  Person:\n    description: Someone with a photo.\n    tracking: record\n    attributes:\n      photo: { type: file, optional: true, personal: true }\n    states:\n      ON: { category: live }\n      GONE: { category: closed, final: true }\n    transitions:\n      add: { kind: initial, to: ON, optional_inputs: [photo], description: Adds them. }\n      leave: { kind: external, from: [ON], to: GONE, description: Ends them. }\n      forget: { kind: erasure, inputs: { reason: { type: string } }, description: Erases them. }\n";
    let file = fresh_store("referenced");
    let path = file.to_str().expect("a path");
    let s = start(&["--store", path, "--create", "--port", "0"]);
    publish(&s, &json!({ "pics": PICS }));
    let op = json!({"id": "operator"});
    let (photo, other) = (
        format!("sha256:{}", "a".repeat(64)),
        format!("sha256:{}", "b".repeat(64)),
    );
    let add = || {
        let v = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "Person", "transition": "add",
                    "inputs": {"photo": {"hash": photo, "size": 10, "media_type": "image/png"}}}),
        );
        assert_eq!(v["kind"], "satisfied", "{v}");
        v["object_id"].as_str().expect("the person").to_string()
    };
    let (ann, bo) = (add(), add());
    let held = || {
        post(
            &s,
            "/v1/referenced",
            &json!({"actor": op, "hashes": [photo, other]}),
        )
    };
    assert_eq!(held(), json!({"referenced": [photo], "refused": null}));
    for who in [&ann, &bo] {
        let v = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "object_id": who, "transition": "forget", "inputs": {"reason": "asked"}}),
        );
        assert_eq!(v["kind"], "satisfied", "{v}");
    }
    // once the last who held it is erased, nothing holds it, every copy gone
    assert_eq!(held(), json!({"referenced": [], "refused": null}));
    // a hash not as the wire writes one, or one given twice, is refused; an
    // actor the store does not know is told so
    for bad in [
        json!(["sha256:ABC"]),
        json!([photo, photo]),
        json!("sha256:"),
    ] {
        let (status, out) = call(
            &s,
            "POST",
            "/v1/referenced",
            Some(&json!({"actor": op, "hashes": bad})),
        );
        assert_eq!(status, 400, "{out}");
    }
    assert_eq!(
        post(
            &s,
            "/v1/referenced",
            &json!({"actor": {"id": "nobody"}, "hashes": [photo]})
        ),
        json!({"referenced": [], "refused": "actor_known"})
    );
}

/// A service started on a store of its own on PostgreSQL, in a schema of
/// its own, with what it was started with and a connection to the database
/// beside it; none on SQLite, said.
fn served_on_postgresql(
    name: &str,
) -> Option<(objectflow_core::db::Connection, Vec<String>, String)> {
    let base = std::env::var("OBJECTFLOW_SANDBOX_POSTGRES").unwrap_or_default();
    if base.trim().is_empty() {
        eprintln!("skipped on SQLite: a deployment's store on PostgreSQL");
        return None;
    }
    let (mut kept, mut password) = (Vec::new(), String::new());
    for part in base.split_whitespace() {
        match part.strip_prefix("password=") {
            Some(p) => password = p.to_string(),
            None => kept.push(part),
        }
    }
    let schema = format!("{name}_{}", std::process::id());
    let admin = objectflow_core::db::Connection::postgres(&base).expect("the database");
    admin
        .execute_batch(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema}"
        ))
        .expect("a schema");
    let url = format!("{} options='-c search_path={schema}'", kept.join(" "));
    // a directory of its own, never one a test on a file makes under the
    // same name, which it would remove under that test's running service
    let dir = fresh_store(&format!("pg-{name}"))
        .parent()
        .expect("a directory")
        .to_path_buf();
    let secret = dir.join("password");
    std::fs::write(&secret, format!("{password}\n")).expect("the password's file");
    let args = [
        "--store",
        &url,
        "--store-password-file",
        secret.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]
    .map(str::to_string)
    .to_vec();
    Some((admin, args, schema))
}

/// A store on PostgreSQL whose connection is lost, its session ended as a
/// database's restart or an idle timeout ends it, is opened again: the
/// write that met the loss is answered 503, which a client retries, and the
/// next applies, the store's lock held again; where every write answered
/// 500 until a person restarted the service (durability on PostgreSQL 1).
#[test]
fn a_store_on_postgresql_whose_connection_is_lost_is_opened_again() {
    let Some((admin, args, schema)) = served_on_postgresql("lost") else {
        return;
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let s = start(&args);
    publish(&s, &json!({"copies": COPIES}));
    let op = json!({"id": "operator"});
    let add = |login: &str| {
        call(
            &s,
            "POST",
            "/v1/request",
            Some(
                &json!({"actor": op, "type": "Person", "transition": "add", "inputs": {"login": login}}),
            ),
        )
    };
    let (status, ann) = add("ann@example.com");
    assert_eq!(
        (status, ann["kind"].as_str()),
        (200, Some("satisfied")),
        "{ann}"
    );
    // the writer's session, the one holding this store's lock, ended
    let ended: i64 = admin
        .query_row(
            &format!(
                "SELECT count(pg_terminate_backend(pid)) FROM pg_locks WHERE locktype = 'advisory' \
                 AND objsubid = 1 AND objid = (hashtext('objectflow {schema}')::bigint & 4294967295)::oid"
            ),
            [],
            |r| r.get(0),
        )
        .expect("the writer's session");
    assert_eq!(ended, 1);
    let (status, met) = add("bea@example.com");
    assert_eq!(status, 503, "the write that met the loss: {met}");
    // said as a lost connection, never as SQLite's log or a full volume,
    // nor by /health, where both named them after a loss (durability on
    // PostgreSQL 10)
    let fault = met["fault"].as_str().unwrap_or_default();
    assert!(
        met["code"] == "unavailable" && !fault.contains("wal") && !fault.contains("full"),
        "{met}"
    );
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(status, 200, "/health after the loss: {health}");
    let (status, cy) = add("cy@example.com");
    assert_eq!(
        (status, cy["kind"].as_str()),
        (200, Some("satisfied")),
        "{cy}"
    );
    // its lock held again: a second service refused
    let (code, said) = refused(&args);
    assert_ne!(code, Some(0));
    assert!(said.contains("another service holds the store"), "{said}");
    drop(s);
    admin
        .execute_batch(&format!("DROP SCHEMA {schema} CASCADE"))
        .expect("the schema dropped");
}

/// A store on PostgreSQL whose database is replaced while it is served, a
/// restore in its place or a standby a failover promoted, is not written:
/// the lost connection opened again finds another database, and the store
/// is served no more until a person marks it a copy and starts the service
/// again, where the service, opening its lost connection again by itself,
/// would write on a copy that may lack its last writes, handing out again
/// what they had (durability on PostgreSQL 2, ADR-0207).
#[test]
fn a_store_on_postgresql_replaced_in_place_is_served_no_more() {
    let base = std::env::var("OBJECTFLOW_SANDBOX_POSTGRES").unwrap_or_default();
    if base.trim().is_empty() {
        eprintln!("skipped on SQLite: a deployment's store on PostgreSQL");
        return;
    }
    let (mut kept, mut password) = (Vec::new(), String::new());
    for part in base.split_whitespace() {
        match part.strip_prefix("password=") {
            Some(p) => password = p.to_string(),
            None => kept.push(part),
        }
    }
    let name = format!("objectflow_replaced_{}", std::process::id());
    let admin = objectflow_core::db::Connection::postgres(&base).expect("the database");
    let gone = |db: &str| {
        admin
            .execute_batch(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
            .expect("a database dropped");
    };
    for db in [name.clone(), format!("{name}_old"), format!("{name}_copy")] {
        gone(&db);
    }
    admin
        .execute_batch(&format!(
            "CREATE DATABASE {name} TEMPLATE template0 ENCODING 'UTF8' LOCALE_PROVIDER libc LC_COLLATE 'C' LC_CTYPE 'C'"
        ))
        .expect("the store's database");
    let url = format!("{} dbname={name}", kept.join(" "));
    // a directory of its own, never the test on a file named alike
    let dir = fresh_store("pg-replaced")
        .parent()
        .expect("a directory")
        .to_path_buf();
    let secret = dir.join("password");
    std::fs::write(&secret, format!("{password}\n")).expect("the password's file");
    let secret = secret.to_str().expect("a path");
    let s = start(&[
        "--store",
        &url,
        "--store-password-file",
        secret,
        "--create",
        "--port",
        "0",
    ]);
    publish(&s, &json!({"copies": COPIES}));
    let op = json!({"id": "operator"});
    let add = |login: &str| {
        call(
            &s,
            "POST",
            "/v1/request",
            Some(
                &json!({"actor": op, "type": "Person", "transition": "add", "inputs": {"login": login}}),
            ),
        )
    };
    let (status, ann) = add("ann@example.com");
    assert_eq!(
        (status, ann["kind"].as_str()),
        (200, Some("satisfied")),
        "{ann}"
    );
    // the database replaced by a copy of itself, as a restore in its place
    // leaves it: the same store, another database
    let ended: i64 = admin
        .query_row(
            &format!("SELECT count(pg_terminate_backend(pid)) FROM pg_stat_activity WHERE datname = '{name}'"),
            [],
            |r| r.get(0),
        )
        .expect("its sessions ended");
    assert!(ended >= 1, "the writer's session among them");
    admin
        .execute_batch(&format!("CREATE DATABASE {name}_copy TEMPLATE {name}"))
        .expect("a copy");
    admin
        .execute_batch(&format!("ALTER DATABASE {name} RENAME TO {name}_old"))
        .expect("the store's database set aside");
    admin
        .execute_batch(&format!("ALTER DATABASE {name}_copy RENAME TO {name}"))
        .expect("the copy in its place");
    let (status, met) = add("bea@example.com");
    assert_eq!(status, 503, "the write that met the loss: {met}");
    let (status, refused) = add("cy@example.com");
    assert_eq!(status, 503, "{refused}");
    assert!(
        refused["fault"]
            .as_str()
            .is_some_and(|f| f.contains("not_its_database")),
        "{refused}"
    );
    let (status, again) = add("dee@example.com");
    assert_eq!(status, 503, "served no more: {again}");
    drop(s);
    for db in [name.clone(), format!("{name}_old")] {
        gone(&db);
    }
}

/// What no store can hold is malformed at the wire, on either backend, so
/// the backends answer it alike: U+0000 in a request's text, which
/// PostgreSQL's text cannot store, where SQLite took it and PostgreSQL
/// answered 500 with the refusal unrecorded; and a name past 1 KiB, which
/// PostgreSQL's index of the attempt log cannot hold. U+0000 in an input's
/// value stays the engine's refusal, `invalid_input` (records on
/// PostgreSQL, F4 and F5).
#[test]
fn what_no_store_can_hold_is_malformed_at_the_wire() {
    let store = fresh_store("unholdable");
    let s = start(&[
        "--store",
        store.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
    ]);
    publish(&s, &json!({"copies": COPIES}));
    let op = json!({"id": "operator"});
    let add = |extra: Value| {
        let mut body = json!({"actor": op, "type": "Person", "transition": "add", "inputs": {"login": "ann@example.com"}});
        for (k, v) in extra.as_object().expect("fields") {
            body[k] = v.clone();
        }
        call(&s, "POST", "/v1/request", Some(&body))
    };
    for (extra, at) in [
        (json!({"context": "why\u{0}not"}), "context"),
        (json!({"idempotency_key": "k\u{0}"}), "idempotency_key"),
        (json!({"actor": {"id": "op\u{0}erator"}}), "actor.id"),
        (json!({"actor": {"id": "x".repeat(1025)}}), "actor.id"),
        (json!({"transition": "a".repeat(1025)}), "transition"),
    ] {
        let (status, said) = add(extra);
        assert_eq!(status, 400, "{at}: {said}");
        assert!(
            said["fault"].as_str().is_some_and(|f| f.starts_with(at)),
            "{at}: {said}"
        );
    }
    // a name of 1 KiB is taken
    let (status, said) = add(json!({"actor": {"id": "x".repeat(1024)}}));
    assert_eq!(status, 200, "{said}");
    // an input's value holding U+0000 is the engine's refusal
    let (status, said) = add(json!({"inputs": {"login": "ann\u{0}@example.com"}}));
    assert_eq!(status, 200, "{said}");
    assert_eq!(said["kind"], "invalid_input", "{said}");
}

/// A value past the 2,704 bytes PostgreSQL holds in an index's entry, an
/// identity or a unique or indexed string, is refused there as a conflict
/// naming the bound, where it answered 500; SQLite has no such bound
/// (records on PostgreSQL, F4).
#[test]
fn a_value_past_postgresqls_index_entry_is_a_conflict() {
    let Some((admin, args, schema)) = served_on_postgresql("longvalue") else {
        return;
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let s = start(&args);
    publish(&s, &json!({"copies": COPIES}));
    // varied, as an index's entry compresses a value of one letter repeated
    // to fit
    let mut x: u32 = 7;
    let varied: String = (0..3000)
        .map(|_| {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            char::from(b'a' + u8::try_from((x >> 16) % 26).expect("a letter"))
        })
        .collect();
    let login = format!("{varied}@example.com");
    let (status, said) = call(
        &s,
        "POST",
        "/v1/request",
        Some(
            &json!({"actor": {"id": "operator"}, "type": "Person", "transition": "add", "inputs": {"login": login}}),
        ),
    );
    assert_eq!(status, 409, "{said}");
    assert!(
        said["fault"]
            .as_str()
            .is_some_and(|f| f.contains("2,704 bytes")),
        "{said}"
    );
    drop(s);
    admin
        .execute_batch(&format!("DROP SCHEMA {schema} CASCADE"))
        .expect("the schema dropped");
}

/// A store on PostgreSQL started on a database that is not the one it was
/// last opened in, a restore in its place or a standby a failover
/// promoted, is refused until marked a copy, since no PostgreSQL backup
/// carries the mark and an unmarked copy handed out ids, positions and
/// numbers again; marked, it starts as a restore and says so, its new
/// database then its own (durability on PostgreSQL 2, ADR-0207).
#[test]
fn a_store_on_postgresql_restored_in_place_is_refused_until_marked() {
    let base = std::env::var("OBJECTFLOW_SANDBOX_POSTGRES").unwrap_or_default();
    if base.trim().is_empty() {
        eprintln!("skipped on SQLite: a deployment's store on PostgreSQL");
        return;
    }
    let (mut kept, mut password) = (Vec::new(), String::new());
    for part in base.split_whitespace() {
        match part.strip_prefix("password=") {
            Some(p) => password = p.to_string(),
            None => kept.push(part),
        }
    }
    let name = format!("objectflow_restored_{}", std::process::id());
    let admin = objectflow_core::db::Connection::postgres(&base).expect("the database");
    let gone = |db: &str| {
        admin
            .execute_batch(&format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)"))
            .expect("a database dropped");
    };
    for db in [name.clone(), format!("{name}_old"), format!("{name}_copy")] {
        gone(&db);
    }
    admin
        .execute_batch(&format!(
            "CREATE DATABASE {name} TEMPLATE template0 ENCODING 'UTF8' LOCALE_PROVIDER libc LC_COLLATE 'C' LC_CTYPE 'C'"
        ))
        .expect("the store's database");
    let url = format!("{} dbname={name}", kept.join(" "));
    // a directory of its own, never the test on a file named alike
    let dir = fresh_store("pg-restored")
        .parent()
        .expect("a directory")
        .to_path_buf();
    let secret = dir.join("password");
    std::fs::write(&secret, format!("{password}\n")).expect("the password's file");
    let secret = secret.to_str().expect("a path");
    let args = |create: bool| {
        let mut a = vec![
            "--store",
            url.as_str(),
            "--store-password-file",
            secret,
            "--port",
            "0",
        ];
        if create {
            a.push("--create");
        }
        a
    };
    {
        let s = start(&args(true));
        publish(&s, &json!({"copies": COPIES}));
        let ann = post(
            &s,
            "/v1/request",
            &json!({"actor": {"id": "operator"}, "type": "Person", "transition": "add", "inputs": {"login": "ann@example.com"}}),
        );
        assert_eq!(ann["kind"], "satisfied", "{ann}");
    }
    // the database replaced by a copy of itself while the service is stopped
    admin
        .execute_batch(&format!(
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{name}'"
        ))
        .expect("its sessions ended");
    admin
        .execute_batch(&format!("CREATE DATABASE {name}_copy TEMPLATE {name}"))
        .expect("a copy");
    admin
        .execute_batch(&format!("ALTER DATABASE {name} RENAME TO {name}_old"))
        .expect("the store's database set aside");
    admin
        .execute_batch(&format!("ALTER DATABASE {name}_copy RENAME TO {name}"))
        .expect("the copy in its place");
    let (code, said) = refused(&args(false));
    assert_ne!(code, Some(0));
    assert!(
        said.contains("is not the database the store was last opened in"),
        "{said}"
    );
    // marked as operate.md says, it starts as a restore and says so
    let copy = objectflow_core::db::Connection::postgres(&format!("{base} dbname={name}"))
        .expect("the copy");
    copy.execute_batch(
        "INSERT INTO of_counter (name, value) VALUES ('copied', 1) ON CONFLICT (name) DO UPDATE SET value = 1",
    )
    .expect("the copy marked");
    drop(copy);
    {
        let s = start(&args(false));
        let said = s.said_within("is a restored copy");
        assert!(said.contains("is a restored copy"), "{said}");
    }
    // its database its own now: the next start is no restore
    let s = start(&args(false));
    let said = s.said_within("at version");
    assert!(
        said.contains("at version") && !said.contains("is a restored copy"),
        "{said}"
    );
    drop(s);
    for db in [name.clone(), format!("{name}_old")] {
        gone(&db);
    }
}

/// A TCP proxy to `upstream` that passes no byte while its switch is set,
/// every connection kept open: a database frozen as `docker pause` freezes
/// one, whose kernel still takes what is sent to it, so no timeout of the
/// connection's fires (durability on PostgreSQL 3).
fn freezing_proxy(upstream: String) -> (u16, std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use std::sync::atomic::Ordering;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = listener.local_addr().expect("an address").port();
    let frozen = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let switch = frozen.clone();
    std::thread::spawn(move || {
        for client in listener.incoming().flatten() {
            let Ok(server) = TcpStream::connect(&upstream) else {
                continue;
            };
            let (Ok(c), Ok(s)) = (client.try_clone(), server.try_clone()) else {
                continue;
            };
            for (mut from, mut to) in [(c, s), (server, client)] {
                let frozen = switch.clone();
                std::thread::spawn(move || {
                    let mut buf = [0u8; 16 * 1024];
                    while let Ok(n) = from.read(&mut buf) {
                        if n == 0 {
                            break;
                        }
                        while frozen.load(Ordering::SeqCst) {
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        if to.write_all(&buf[..n]).is_err() {
                            break;
                        }
                    }
                    let _ = to.shutdown(std::net::Shutdown::Write);
                });
            }
        }
    });
    (port, frozen)
}

/// `GET path` answered within `wait`, its status and how long it took, or
/// none where it was not.
fn get_within(s: &Service, path: &str, wait: std::time::Duration) -> Option<(u16, Value, f64)> {
    asked_within(s, "GET", path, None, wait)
}

/// A request answered within `wait`, its status and how long it took, or
/// none where it was not.
fn asked_within(
    s: &Service,
    method: &str,
    path: &str,
    body: Option<&Value>,
    wait: std::time::Duration,
) -> Option<(u16, Value, f64)> {
    let began = std::time::Instant::now();
    let mut stream = TcpStream::connect(&s.addr).expect("a connection");
    stream.set_read_timeout(Some(wait)).expect("a timeout");
    let text = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        s.addr,
        text.len()
    )
    .expect("the request is written");
    let mut raw = String::new();
    stream.read_to_string(&mut raw).ok()?;
    let status = raw.get(9..12)?.parse().ok()?;
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, b)| b)
        .unwrap_or_default();
    Some((
        status,
        serde_json::from_str(body).unwrap_or(Value::Null),
        began.elapsed().as_secs_f64(),
    ))
}

/// A database that stops answering without closing its connections leaves
/// `/health` answering, `503 store_stalled` within its wait and at once
/// when asked again, the read it started left to finish, and `200` once the
/// database answers again, where `/health` waited on it with every request
/// and said nothing (durability on PostgreSQL 3); and a read answered `503
/// read_timeout` at its deadline, on the connection it held and on one it
/// makes, where it waited with the database (ADR-0215).
#[test]
fn a_stalled_database_is_said_by_health() {
    let Some((admin, args, schema)) = served_on_postgresql("stalled") else {
        return;
    };
    let base = std::env::var("OBJECTFLOW_SANDBOX_POSTGRES").expect("the database");
    let at = |key: &str| {
        base.split_whitespace()
            .find_map(|p| p.strip_prefix(key))
            .map(str::to_string)
    };
    let upstream = format!(
        "{}:{}",
        at("host=").unwrap_or_else(|| "127.0.0.1".to_string()),
        at("port=").unwrap_or_else(|| "5432".to_string())
    );
    let (port, frozen) = freezing_proxy(upstream);
    let through: Vec<String> = args
        .iter()
        .map(|a| {
            a.split_whitespace()
                .map(|p| {
                    if p.starts_with("host=") {
                        "host=127.0.0.1".to_string()
                    } else if p.starts_with("port=") {
                        format!("port={port}")
                    } else {
                        p.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    let mut through: Vec<&str> = through.iter().map(String::as_str).collect();
    // a read's deadline past /health's wait, so the probe is still running
    // when /health is asked again
    through.extend(["--read-timeout", "8 s"]);
    let s = start(&through);
    let wait = std::time::Duration::from_secs(20);
    let (status, body, _) = get_within(&s, "/health", wait).expect("an answer");
    assert_eq!(status, 200, "{body}");
    // a read answered at its deadline: first on the connection the pool
    // kept from the read before, its statement stopped there, then on a new
    // one, whose making is (ADR-0215); and a query, which reads the store
    // through its world
    let op = json!({"id": "operator"});
    let read = || {
        asked_within(
            &s,
            "POST",
            "/v1/get",
            Some(&json!({"actor": op, "id": "operator"})),
            wait,
        )
    };
    let query = || {
        asked_within(
            &s,
            "POST",
            "/v1/query",
            Some(&json!({"actor": op, "type": "DeclarationChange"})),
            wait,
        )
    };
    frozen.store(true, std::sync::atomic::Ordering::SeqCst);
    for (n, answered) in [read(), read(), query()].into_iter().enumerate() {
        let (status, body, took) = answered.expect("a read answered while stalled");
        assert_eq!(
            (status, body["code"].as_str()),
            (503, Some("read_timeout")),
            "read {n}: {body}"
        );
        assert!(took < 12.0, "read {n} answered in {took} s");
    }
    let (status, body, took) = get_within(&s, "/health", wait).expect("an answer while stalled");
    assert_eq!(
        (status, body["code"].as_str()),
        (503, Some("store_stalled")),
        "{body}"
    );
    assert!(took < 10.0, "answered in {took} s");
    // asked again, the read still running is joined, not started again
    let (status, body, took) = get_within(&s, "/health", wait).expect("an answer again");
    assert_eq!(
        (status, body["code"].as_str()),
        (503, Some("store_stalled")),
        "{body}"
    );
    assert!(took < 2.0, "answered again in {took} s");
    frozen.store(false, std::sync::atomic::Ordering::SeqCst);
    let mut answered = None;
    for _ in 0..40 {
        answered = get_within(&s, "/health", wait);
        if answered.as_ref().is_some_and(|(st, _, _)| *st == 200) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let (status, body, _) = answered.expect("an answer once the database answers");
    assert_eq!(status, 200, "{body}");
    let (status, body, _) = read().expect("a read answered");
    assert_eq!(status, 200, "a read once the database answers: {body}");
    // and a later stall is read afresh, never answered from the last read
    frozen.store(true, std::sync::atomic::Ordering::SeqCst);
    let (status, body, _) = get_within(&s, "/health", wait).expect("an answer stalled again");
    assert_eq!(
        (status, body["code"].as_str()),
        (503, Some("store_stalled")),
        "{body}"
    );
    frozen.store(false, std::sync::atomic::Ordering::SeqCst);
    drop(s);
    admin
        .execute_batch(&format!("DROP SCHEMA {schema} CASCADE"))
        .expect("the schema dropped");
}

const NOTES: &str = "module: notes
categories: [live, closed]
types:
  Note:
    description: A note.
    tracking: record
    attributes:
      text: { type: string }
    states:
      KEPT: { category: live }
      GONE: { category: closed, final: true }
    transitions:
      write: { kind: initial, to: KEPT, required_inputs: [text] }
      drop: { kind: external, from: KEPT, to: GONE }
";

/// A store on a volume that fills (the audit's durability S1): the writes
/// past its last space are answered `503 disk_full` and none acknowledged
/// is lost; killed while it is full, the service starts again and serves
/// every acknowledged write; once space is freed, writes apply again. On a
/// tmpfs in a user and mount namespace of its own, where the suite held
/// tests of the predicates alone and the audit probed it by hand; skipped,
/// said, where the system refuses the namespace, and failed so in CI,
/// which allows it (`check.yml`).
#[test]
fn a_full_volume_loses_no_acknowledged_write() {
    let dir = fresh_store("full")
        .parent()
        .expect("a directory")
        .to_path_buf();
    let d = dir.to_str().expect("a path").to_string();
    // the namespace held by a process of its own, the volume mounted over
    // the directory there, a megabyte of ballast on it to free later
    let held = Command::new("unshare")
        .args([
            "-rm",
            "sh",
            "-c",
            "mount -t tmpfs -o size=8m tmpfs \"$0\" && head -c 1048576 /dev/zero > \"$0/ballast\" && echo mounted && exec sleep 600",
            &d,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let refused = |why: String| {
        assert!(
            std::env::var_os("CI").is_none(),
            "CI runs this test, which needs a user and mount namespace: {why}"
        );
        eprintln!("skipped: no user and mount namespace here: {why}");
    };
    let mut holder = match held {
        Ok(h) => h,
        Err(e) => return refused(e.to_string()),
    };
    let mut line = String::new();
    let _ = BufReader::new(holder.stdout.take().expect("its stdout")).read_line(&mut line);
    if line.trim() != "mounted" {
        let _ = holder.kill();
        let _ = holder.wait();
        let mut why = String::new();
        if let Some(mut e) = holder.stderr.take() {
            let _ = e.read_to_string(&mut why);
        }
        return refused(why);
    }
    let pid = holder.id().to_string();
    let inside = |program: &str, args: &[&str]| {
        let mut c = Command::new("nsenter");
        // its credentials kept, since the namespace refuses setgroups
        c.args(["-t", &pid, "-U", "-m", "--preserve-credentials", program])
            .args(args);
        c
    };
    let service = env!("CARGO_BIN_EXE_objectflow-service");
    let store = format!("{d}/store.sqlite");
    let op = json!({"id": "operator"});
    let text = |i: usize| format!("{i:06}:{}", "x".repeat(16 * 1024));
    let write = |s: &Service, i: usize| {
        call(
            s,
            "POST",
            "/v1/request",
            Some(
                &json!({"actor": op, "type": "Note", "transition": "write", "inputs": {"text": text(i)}}),
            ),
        )
    };
    let s = start_command(inside(
        service,
        &["--store", &store, "--create", "--port", "0"],
    ));
    publish(&s, &json!({"notes": NOTES}));
    let (mut kept, mut full) = (Vec::new(), 0);
    for i in 0..2000 {
        let (status, v) = write(&s, i);
        match status {
            200 => {
                assert_eq!(v["kind"], "satisfied", "{v}");
                kept.push((v["object_id"].as_str().expect("an id").to_string(), i));
            }
            503 => {
                assert_eq!(v["code"], "disk_full", "{v}");
                full += 1;
                if full == 5 {
                    break;
                }
            }
            _ => panic!("write {i}: {status} {v}"),
        }
    }
    assert!(
        full == 5 && kept.len() >= 5,
        "{} kept, {full} refused for a full volume",
        kept.len()
    );
    // killed while the volume is full, and started again on it
    drop(s);
    let s = start_command(inside(service, &["--store", &store, "--port", "0"]));
    for (id, i) in &kept {
        let got = post(&s, "/v1/get", &json!({"actor": op, "id": id}));
        assert_eq!(got["attributes"]["text"], json!(text(*i)), "{id}");
    }
    // space freed, writes apply again
    let freed = inside("rm", &[&format!("{d}/ballast")])
        .status()
        .expect("rm runs");
    assert!(freed.success());
    let (status, v) = write(&s, 9999);
    assert_eq!(
        (status, v["kind"].as_str()),
        (200, Some("satisfied")),
        "{v}"
    );
    drop(s);
    let _ = holder.kill();
    let _ = holder.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

/// A store on PostgreSQL at an earlier release is served as due, every
/// operation refused until its upgrade, which carries it to this release
/// with what it held, as a file's is, where it was refused at the start and
/// so stranded (durability on PostgreSQL 6).
#[test]
fn a_store_on_postgresql_at_an_earlier_release_is_upgraded() {
    let Some((admin, args, schema)) = served_on_postgresql("behind") else {
        return;
    };
    let made: Vec<&str> = args.iter().map(String::as_str).collect();
    let again: Vec<&str> = made.iter().copied().filter(|a| *a != "--create").collect();
    let op = json!({"id": "operator"});
    let ann = {
        let s = start(&made);
        publish(&s, &json!({"copies": COPIES}));
        let ann = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "Person", "transition": "add", "inputs": {"login": "ann@example.com"}}),
        );
        ann["object_id"].as_str().expect("a person").to_string()
    };
    // taken back to the shape a store 0.2.0 made has: what the steps from
    // 0.2.0 add, undone, the batch's columns 0.6.0 adds among them, and its
    // release named
    admin
        .execute_batch(&format!(
            "SET search_path TO {schema}; DROP TABLE of_restore; DROP TABLE of_interval_retraction; \
             ALTER TABLE of_interval DROP COLUMN moved_position; \
             ALTER TABLE of_engine_release DROP COLUMN commit_id; \
             ALTER TABLE of_event DROP COLUMN batch; ALTER TABLE of_attempt DROP COLUMN batch_index; \
             UPDATE of_engine_release SET release = 'objectflow-core 0.2.0'; RESET search_path"
        ))
        .expect("the store taken back to 0.2.0");
    let s = start(&again);
    let said = s.said_within("POST /v1/upgrade");
    assert!(said.contains("is at objectflow-core 0.2.0"), "{said}");
    let (status, refused) = call(
        &s,
        "POST",
        "/v1/get",
        Some(&json!({"actor": op, "id": ann})),
    );
    assert_eq!(status, 503, "a read while the upgrade is due: {refused}");
    let up = post(&s, "/v1/upgrade", &json!({"actor": op}));
    assert_eq!(up["upgraded"], true, "{up}");
    assert_eq!(up["from"], "objectflow-core 0.2.0", "{up}");
    assert_eq!(
        up["to"],
        format!("objectflow-core {}", env!("CARGO_PKG_VERSION")),
        "{up}"
    );
    let got = post(&s, "/v1/get", &json!({"actor": op, "id": ann}));
    assert_eq!(got["attributes"]["login"], "ann@example.com", "{got}");
    let bea = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "Person", "transition": "add", "inputs": {"login": "bea@example.com"}}),
    );
    assert_eq!(bea["kind"], "satisfied", "{bea}");
    drop(s);
    admin
        .execute_batch(&format!("DROP SCHEMA {schema} CASCADE"))
        .expect("the schema dropped");
}

/// A publish that rebuilds a table another session reads, a backup's dump
/// or a long read on PostgreSQL, waits for it up to the busy timeout and is
/// then answered 503, which the operator retries, its version not taken;
/// the read ended, the same publish applies. Where it answered 500 (records
/// on PostgreSQL F3, durability 7).
#[test]
fn a_publish_beside_a_read_on_postgresql_is_answered_unavailable() {
    let Some((admin, args, schema)) = served_on_postgresql("beside") else {
        return;
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let s = start(&args);
    publish(&s, &json!({"copies": COPIES}));
    let op = json!({"id": "operator"});
    let wider = COPIES.replace(
        "      login: { type: identity, actor_kind: human, unique: true, personal: true, optional: true }\n",
        "      login: { type: identity, actor_kind: human, unique: true, personal: true, optional: true }\n      nickname: { type: string, optional: true }\n",
    );
    assert_ne!(wider, COPIES);
    let drafted = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "type": "DeclarationChange", "transition": "draft",
                "inputs": {"source": {"copies": wider}, "source_ref": "test:beside"}}),
    );
    assert_eq!(drafted["kind"], "satisfied", "{drafted}");
    let change = drafted["object_id"]
        .as_str()
        .expect("the change")
        .to_string();
    let submitted = post(
        &s,
        "/v1/request",
        &json!({"actor": op, "object_id": change, "transition": "submit"}),
    );
    assert_eq!(submitted["kind"], "satisfied", "{submitted}");
    let approve = json!({"actor": op, "change": change, "expected_version": submitted["version"]});
    admin
        .execute_batch(&format!(
            "BEGIN; LOCK TABLE {schema}.t_person IN ACCESS SHARE MODE"
        ))
        .expect("a read holding the table");
    let (status, said) = call(&s, "POST", "/v1/publish", Some(&approve));
    assert_eq!(status, 503, "{said}");
    admin.execute_batch("ROLLBACK").expect("the read ended");
    let approved = post(&s, "/v1/publish", &approve);
    assert_eq!(approved["verdict"]["kind"], "satisfied", "{approved}");
    drop(s);
    admin
        .execute_batch(&format!("DROP SCHEMA {schema} CASCADE"))
        .expect("the schema dropped");
}

/// A deployment's store on PostgreSQL (ADR-0213): made by `--create` in a
/// database holding none and refused without it, its password read from a
/// file and refused on the command line, written by one service alone, and
/// served again by the next start as it was left.
#[test]
fn a_store_on_postgresql_is_served_made_and_kept() {
    let base = std::env::var("OBJECTFLOW_SANDBOX_POSTGRES").unwrap_or_default();
    if base.trim().is_empty() {
        eprintln!("skipped on SQLite: a deployment's store on PostgreSQL");
        return;
    }
    // the password apart from the connection string, in a file of its own
    let (mut kept, mut password) = (Vec::new(), String::new());
    for part in base.split_whitespace() {
        match part.strip_prefix("password=") {
            Some(p) => password = p.to_string(),
            None => kept.push(part),
        }
    }
    let schema = format!("served_{}", std::process::id());
    let admin = objectflow_core::db::Connection::postgres(&base).expect("the database");
    admin
        .execute_batch(&format!(
            "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema}"
        ))
        .expect("a schema");
    let url = format!("{} options='-c search_path={schema}'", kept.join(" "));
    let dir = fresh_store("postgres")
        .parent()
        .expect("a directory")
        .to_path_buf();
    let secret = dir.join("password");
    std::fs::write(&secret, format!("{password}\n")).expect("the password's file");
    let secret = secret.to_str().expect("a path");
    // a database holding no store, refused without --create; a password on
    // the command line, refused
    let (code, said) = refused(&[
        "--store",
        &url,
        "--store-password-file",
        secret,
        "--port",
        "0",
    ]);
    assert_ne!(code, Some(0));
    assert!(
        said.contains("holds no store; --create makes one"),
        "{said}"
    );
    let (code, said) = refused(&["--store", &base, "--create", "--port", "0"]);
    assert_ne!(code, Some(0));
    assert!(
        said.contains("OBJECTFLOW_STORE_PASSWORD or --store-password-file"),
        "{said}"
    );
    let args = [
        "--store",
        &url,
        "--store-password-file",
        secret,
        "--create",
        "--port",
        "0",
    ];
    let memo = {
        let s = start(&args);
        publish(&s, &json!({"copies": COPIES}));
        let op = json!({"id": "operator"});
        let ann = post(
            &s,
            "/v1/request",
            &json!({"actor": op, "type": "Person", "transition": "add", "inputs": {"login": "ann@example.com"}}),
        );
        assert_eq!(ann["kind"], "satisfied", "{ann}");
        // a second service on the same store is refused while the first holds it
        let (code, said) = refused(&args);
        assert_ne!(code, Some(0));
        assert!(said.contains("another service holds the store"), "{said}");
        ann["object_id"].as_str().expect("the person").to_string()
    };
    // the next start serves the store as it was left, its reads included
    let s = start(&args);
    let got = post(
        &s,
        "/v1/get",
        &json!({"actor": {"id": "operator"}, "id": memo}),
    );
    assert_eq!(got["attributes"]["login"], "ann@example.com", "{got}");
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(status, 200, "{health}");
    drop(s);
    admin
        .execute_batch(&format!("DROP SCHEMA {schema} CASCADE"))
        .expect("the schema dropped");
}

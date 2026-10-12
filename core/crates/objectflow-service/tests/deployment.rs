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
    let mut child = Command::new(env!("CARGO_BIN_EXE_objectflow-service"))
        .args(args)
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

fn fresh_store(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("objectflow-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    dir.join("store.sqlite")
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
    assert!(said.contains(missing.to_str().expect("a path")), "{said}");
    // how many reads run at once is a count, and a deployment's (ADR-0189)
    let (code, said) = refused(&["--max-reads", "0", "--port", "0"]);
    assert_eq!(code, Some(2));
    assert!(
        said.contains("--max-reads takes a count, 1 or more: 0"),
        "{said}"
    );
    let (code, said) = refused(&["--max-reads", "2", "--port", "0"]);
    assert_eq!(code, Some(2));
    assert!(
        said.contains("how many reads run at once is a deployment's"),
        "{said}"
    );
    let long = dir.join("s".repeat(150));
    let (code, said) = refused(&["--socket", long.to_str().expect("a path")]);
    assert_eq!(code, Some(1));
    assert!(said.contains(&"s".repeat(150)), "{said}");
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
    let (url, _, _) = bank();
    let file = fresh_store("stopping");
    let named = format!("bank={url}");
    let mut s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
        "--evaluator",
        &named,
        "--evaluator-timeout",
        "2 s",
    ]);
    publish(&s, &json!({"payments": PAYMENTS}));
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
    // a clean stop leaves the store whole in its one file
    let wal = std::fs::metadata(format!("{}-wal", file.display())).map_or(0, |m| m.len());
    assert_eq!(wal, 0, "{said}");
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
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
    assert_eq!(submitted["kind"], "satisfied", "{submitted}");
    assert!(
        took > std::time::Duration::from_millis(500),
        "the submit took {took:?}, too little to tell"
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

/// A submit whose dry run passes the read's deadline is answered
/// `503 read_timeout` and records nothing, where it was computed again under
/// the writer's lock, unbounded and unsaid, every write waiting for it
/// (third production review PUBLISH-SCALE, N1).
#[test]
fn a_submits_dry_run_past_the_deadline_is_answered_not_locked() {
    let file = fresh_store("submit-deadline");
    let s = start(&[
        "--store",
        file.to_str().expect("a path"),
        "--create",
        "--port",
        "0",
        "--read-timeout",
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
    let mut big = String::from("module: big\ncategories: [live, closed]\ntypes:\n");
    for i in 0..600 {
        big.push_str(&format!("  T{i}:\n    description: Type {i}.\n    tracking: record\n    attributes:\n      a: {{ type: string }}\n    states:\n      OPEN: {{ category: live }}\n      DONE: {{ category: closed, final: true }}\n    transitions:\n      make: {{ kind: initial, to: OPEN, required_inputs: [a] }}\n      finish: {{ kind: external, from: OPEN, to: DONE }}\n"));
    }
    let op = json!({"id": "operator"});
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
    std::thread::sleep(std::time::Duration::from_millis(600));
    let (_, health) = call(&s, "GET", "/health", None);
    let mut stream = TcpStream::connect(&s.addr).expect("a connection");
    write!(
        stream,
        "GET /metrics HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    )
    .expect("asked");
    let mut metrics = String::new();
    stream.read_to_string(&mut metrics).expect("the metrics");
    let approved = approving.join().expect("the approval");
    assert_eq!(approved["verdict"]["kind"], "satisfied", "{approved}");
    assert!(
        health["writer_held_seconds"].as_f64().unwrap_or(0.0) > 0.2,
        "{health}"
    );
    assert!(
        health["requests_in_flight"].as_u64().unwrap_or(0) >= 2,
        "{health}"
    );
    let held = metrics
        .lines()
        .find_map(|l| l.strip_prefix("objectflow_writer_held_seconds "))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    assert!(held > 0.2, "{metrics}");
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
#[test]
fn a_connection_at_the_cap_makes_room_or_is_told_busy() {
    let s = start(&[
        "--port",
        "0",
        "--idle-timeout",
        "2 s",
        "--max-connections",
        "3",
    ]);
    let silent: Vec<TcpStream> = (0..3)
        .map(|_| TcpStream::connect(&s.addr).expect("a connection"))
        .collect();
    std::thread::sleep(std::time::Duration::from_millis(200));
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
    std::thread::sleep(std::time::Duration::from_millis(300));
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
    std::thread::sleep(std::time::Duration::from_millis(500));
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
/// `T-3` a fault, `T-4` an answer slower than the service waits, and `T-5`
/// an answer with no verdict. Each call it is posted is kept, as it arrives.
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
            // each call answered on its own thread, so a slow answer holds
            // up no other
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().expect("the stream"));
                let mut length = 0;
                let mut authorization = None;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
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
                let (status, answer) = match call["arguments"][0].as_str() {
                    Some("T-1") => (200, r#"{"verdict": "satisfied"}"#),
                    Some("T-2") => (200, r#"{"verdict": "unsatisfied"}"#),
                    Some("T-4") => {
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        (200, r#"{"verdict": "satisfied"}"#)
                    }
                    Some("T-5") => (200, r#"{"cleared": true}"#),
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
    // a fault, an answer too slow and an answer with no verdict are the
    // evaluator's unknown: refused, and the caller told to wait
    for reference in ["T-3", "T-4", "T-5"] {
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

/// The stores release 0.1.0's own service made, kept with what it answered
/// of them (`fixtures/make-release-store.py`, at commit f6ee6d6).
/// Each test copies into a store of its own, `tag` naming it, since tests
/// run at once in one process and share its id.
fn release_fixture(name: &str, tag: &str) -> (PathBuf, Value) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/release-0.1.0");
    let path = fresh_store(&format!("upgrade-{name}-{tag}"));
    std::fs::copy(dir.join(format!("{name}.sqlite")), &path).expect("the fixture copied");
    let reads = std::fs::read_to_string(dir.join(format!("{name}.reads.json")))
        .expect("what release 0.1.0 answered");
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

/// A store release 0.1.0 made is served only the upgrade until it runs; a
/// dry run changes nothing; and once upgraded every object reads as 0.1.0
/// read it, its history whole, the log the same with the operator's
/// `upgrade` after it, and a restart opens the store at this release
/// (ADR-0177).
#[test]
fn a_store_an_earlier_release_made_is_upgraded_and_reads_alike() {
    let op = json!({"id": "operator"});
    for name in ["reimbursement", "servicedesk"] {
        let (path, recorded) = release_fixture(name, "alike");
        assert_eq!(recorded["release"], "objectflow-core 0.1.0");
        let args = ["--store", path.to_str().expect("a path"), "--port", "0"];
        let s = start(&args);
        let (status, health) = call(&s, "GET", "/health", None);
        // a store due an upgrade serves only the upgrade: its health is 503
        assert_eq!(status, 503, "{health}");
        assert_eq!(
            (&health["upgrade_due"], &health["store_release"]),
            (&json!(true), &recorded["release"])
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
                Some(1)
            )
        );
        assert_eq!(
            call(&s, "GET", "/health", None).1["upgrade_due"],
            json!(true)
        );
        let done = post(&s, "/v1/upgrade", &json!({"actor": op}));
        assert_eq!(done["upgraded"], json!(true), "{done}");
        // what 0.1.0 answered, object by object
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
                // the operator's act naming the release, after what it did
                for (was, is) in before.iter().zip(now) {
                    assert_answers_alike(was, is, &format!("{name} {id}"));
                }
                assert_eq!(now.len(), before.len() + 1);
                assert_eq!(now[before.len()]["transition"], "upgrade");
                assert_eq!(
                    got["version"],
                    json!(was["version"].as_i64().unwrap_or(0) + 1)
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
        for (was, is) in was.iter().zip(&log) {
            assert_answers_alike(was, is, &format!("{name} log"));
        }
        assert_eq!(log.len(), was.len() + 1, "{name}");
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
    }
}

/// An upgrade is the operator's, and refused to anyone else.
#[test]
fn only_an_operator_upgrades() {
    let (path, _) = release_fixture("reimbursement", "operator");
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
    let history = post(&s, "/v1/history", &json!({"actor": op, "id": tally}));
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
        chain(120),
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
    let (status, v) = call(&s, "POST", "/v1/request", Some(&add));
    assert_eq!((status, v["kind"].as_str()), (200, Some("stale")), "{v}");
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
    // answers logged until the pipe, unread, is full and a line waits
    let get = json!({"actor": {"id": "operator"}, "id": "nothing-1"});
    let blocked = std::thread::spawn({
        let addr = s.addr.clone();
        move || {
            for _ in 0..20_000 {
                let mut stream = TcpStream::connect(&addr).expect("a connection");
                let body = get.to_string();
                write!(
                    stream,
                    "POST /v1/get HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .expect("asked");
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .expect("a timeout");
                let mut answer = String::new();
                if stream.read_to_string(&mut answer).is_err() || answer.is_empty() {
                    return true;
                }
            }
            false
        }
    });
    assert!(blocked.join().expect("the flood"), "the log never filled");
    std::thread::sleep(std::time::Duration::from_secs(6));
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(
        (status, health["code"].as_str()),
        (503, Some("log_blocked")),
        "{health}"
    );
    // read, the log takes lines again and health is 200
    std::thread::spawn(move || {
        let mut sink = String::new();
        let _ = stderr.read_to_string(&mut sink);
    });
    std::thread::sleep(std::time::Duration::from_millis(500));
    let (status, health) = call(&s, "GET", "/health", None);
    assert_eq!(status, 200, "{health}");
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
}

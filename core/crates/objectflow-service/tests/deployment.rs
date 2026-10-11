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
    let mut line = String::new();
    BufReader::new(child.stderr.as_mut().expect("its stderr"))
        .read_line(&mut line)
        .expect("its first line");
    let addr = line
        .trim()
        .strip_prefix("listening on http://")
        .unwrap_or_else(|| panic!("the service did not start: {line}"))
        .to_string();
    Service {
        child: Some(child),
        addr,
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
        let s = start(&["--store", path, "--port", "0"]);
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
        drop(start(&["--store", path, "--port", "0"]));
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

#[test]
fn a_retention_without_a_store_is_refused() {
    let (code, said) = refused(&["--attempt-retention", "90 days"]);
    assert_eq!(code, Some(2), "{said}");
}

/// A bank whose transfers clear by their reference: `T-1` cleared, `T-2` not,
/// `T-3` a fault, `T-4` an answer slower than the service waits, and `T-5`
/// an answer with no verdict. Each call it is posted is kept, as it arrives.
fn bank() -> (String, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let url = format!(
        "http://{}/evaluate",
        listener.local_addr().expect("its address")
    );
    let posted = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let kept = posted.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let kept = kept.clone();
            // each call answered on its own thread, so a slow answer holds
            // up no other
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().expect("the stream"));
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = n.trim().parse().unwrap_or(0);
                    }
                }
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
    (url, posted)
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
    let (url, posted) = bank();
    let file = fresh_store("evaluators");
    let path = file.to_str().expect("a path");
    let named = format!("bank={url}");
    let s = start(&[
        "--store",
        path,
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
    // while an endpoint is slow, every other request is served (D563)
    let addr = s.addr.clone();
    let slow = std::thread::spawn(move || {
        let s = Service { child: None, addr };
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
    let s = start(&["--store", path, "--port", "0"]);
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
        let (_, health) = call(&s, "GET", "/health", None);
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
                assert_eq!(&got["attributes"][k], v, "{name} {id}.{k}");
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
    let s = start(&["--store", path, "--port", "0", "--busy-timeout", "5 s"]);
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
    let s = start(&["--store", path, "--port", "0", "--busy-timeout", "1 s"]);
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
    let s = start(&["--store", file.to_str().expect("a path"), "--port", "0"]);
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
    let s = start(&["--store", file.to_str().expect("a path"), "--port", "0"]);
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
    let first = start(&["--store", path, "--port", "0"]);
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
    let s = start(&["--store", file.to_str().expect("a path"), "--port", "0"]);
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
    let s = start(&["--store", file.to_str().expect("a path"), "--port", "0"]);
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
    let s = start(&["--store", file.to_str().expect("a path"), "--port", "0"]);
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

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

#[test]
fn a_store_another_release_created_is_refused() {
    let file = fresh_store("release");
    let path = file.to_str().expect("a path");
    drop(start(&["--store", path, "--port", "0"]));
    let conn = rusqlite::Connection::open(&file).expect("the file");
    conn.execute(
        "UPDATE of_declaration SET builtins = 'objectflow-core 0.0.0' WHERE version = 0",
        [],
    )
    .expect("the release rewritten");
    drop(conn);
    let (code, said) = refused(&["--store", path, "--port", "0"]);
    assert_ne!(code, Some(0));
    assert!(
        said.contains("was created by objectflow-core 0.0.0"),
        "{said}"
    );
    let _ = std::fs::remove_dir_all(file.parent().expect("its directory"));
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

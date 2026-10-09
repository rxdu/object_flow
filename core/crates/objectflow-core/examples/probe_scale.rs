//! Indicative evidence for PRD C5 and N3: how the core's request and read
//! times grow with the store, which ADR-0162's steps were each measured by
//! (`docs/design/scoped-reads.md` §7); a request and a `get` read what they
//! reach, a query and a metric read their type.
//!
//! A probe, not a test: it asserts nothing. Its limits, stated so it is not
//! over-read: one type, five events an object, an in-memory SQLite store,
//! one machine; times are the median of nine runs at each size, in
//! milliseconds. Run with `cargo run --release --example probe_scale`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::time::Instant;

use objectflow_core::model::{Library, OPERATOR};
use objectflow_core::request::{Request, request};
use objectflow_core::store::Store;
use objectflow_core::verdict::Verdict;
use serde_json::{Value as Json, json};

const DESK: &str = r#"
module: desk
categories: [live, closed]
types:
  Ticket:
    attributes:
      size: { type: int, optional: true }
    states:
      NEW: { category: live }
      WORKING: { category: live }
      PARKED: { category: live }
      DONE: { category: closed, final: true }
    conditions:
      small: { expression: size is null or size < 10, remedy: self_serviceable }
    transitions:
      open: { kind: initial, to: NEW, optional_inputs: [size] }
      start: { kind: external, from: [NEW, PARKED], to: WORKING, guards: { small: deny } }
      park: { kind: external, from: [WORKING], to: PARKED }
      finish: { kind: external, from: [WORKING], to: DONE }
    metrics:
      new_now:
        description: The tickets new now, by size, which only the probe's are.
        source: objects
        item: t
        filter: t.state == Ticket.NEW
        dimensions:
          size: t.size
        expression: count()
"#;

fn ask(s: &mut Store, xn: &str, object: Option<&str>, inputs: Json) -> String {
    let r = Request {
        transition: xn.into(),
        actor: OPERATOR.into(),
        object_id: object.map(str::to_string),
        type_: object.is_none().then(|| "Ticket".to_string()),
        inputs: inputs
            .as_object()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect(),
        occurred_at: None,
        propose: false,
        expires_at: None,
        expected_version: None,
        principal: None,
        idempotency_key: None,
        context: None,
    };
    match request(s, &r).unwrap().unwrap() {
        Verdict::Satisfied { object_id, .. } => object_id,
        other => panic!("{other:?}"),
    }
}

/// One ticket's life: opened, started, parked, started again, and left
/// working or finished, five events.
fn life(s: &mut Store, i: usize) -> String {
    let t = ask(s, "open", None, json!({"size": 3}));
    for xn in ["start", "park", "start"] {
        ask(s, xn, Some(&t), json!({}));
    }
    if i.is_multiple_of(2) {
        ask(s, "finish", Some(&t), json!({}));
    }
    t
}

fn median_ms(mut runs: Vec<f64>) -> f64 {
    runs.sort_by(f64::total_cmp);
    runs[runs.len() / 2]
}

fn time<F: FnMut()>(mut f: F) -> f64 {
    let runs: Vec<f64> = (0..9)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    median_ms(runs)
}

fn main() {
    let lib = Library::load(&[("desk.yaml".into(), DESK.into())]).unwrap();
    let mut s = Store::sandbox(lib, 0).unwrap();
    let mut made = 0usize;
    let mut last = String::new();
    println!(
        "objects  events  request_ms  get_ms  query_ms  metric_ms  snapshot_ms  narrow_query_ms  narrow_metric_ms"
    );
    for target in [100usize, 250, 500, 1000, 2000, 4000, 8000] {
        while made < target {
            last = life(&mut s, made);
            made += 1;
        }
        let events = s.snapshot().unwrap().log().unwrap().count();
        // a request on a live ticket, its open and park alternating so each
        // run applies
        let probe = ask(&mut s, "open", None, json!({}));
        let mut working = false;
        let request_ms = time(|| {
            let xn = if working { "park" } else { "start" };
            ask(&mut s, xn, Some(&probe), json!({}));
            working = !working;
        });
        let get_ms = time(|| {
            let w = s.snapshot().unwrap();
            objectflow_core::reads::get(&w, &last).unwrap();
        });
        let query_ms = time(|| {
            let w = s.snapshot().unwrap();
            objectflow_core::reads::query(&w, "Ticket", Some("state == Ticket.WORKING"), &[], None)
                .unwrap();
        });
        let metric_ms = time(|| {
            let w = s.snapshot().unwrap();
            let keep = vec!["state".to_string()];
            objectflow_core::metric::page(
                &w,
                None,
                "Ticket.time_in_state",
                &BTreeMap::new(),
                Some(&keep),
                None,
                None,
            )
            .unwrap();
        });
        let snapshot_ms = time(|| {
            s.snapshot().unwrap();
        });
        // a filter only the probe's tickets meet: the query narrows by the
        // state's index, and a metric over objects reads the type whole
        // before its filter judges them (TODO, "a metric reads its family
        // whole"), so the gap is what narrowing the metric would save
        let narrow_query_ms = time(|| {
            let w = s.snapshot().unwrap();
            objectflow_core::reads::query(&w, "Ticket", Some("state == Ticket.NEW"), &[], None)
                .unwrap();
        });
        let narrow_metric_ms = time(|| {
            let w = s.snapshot().unwrap();
            objectflow_core::metric::page(
                &w,
                None,
                "Ticket.new_now",
                &BTreeMap::new(),
                None,
                None,
                None,
            )
            .unwrap();
        });
        println!(
            "{made:>7}  {events:>6}  {request_ms:>10.1}  {get_ms:>6.1}  {query_ms:>8.1}  {metric_ms:>9.1}  {snapshot_ms:>11.1}  {narrow_query_ms:>15.2}  {narrow_metric_ms:>16.2}"
        );
        if request_ms > 2000.0 {
            println!("stopped: a request passed two seconds");
            break;
        }
    }
}

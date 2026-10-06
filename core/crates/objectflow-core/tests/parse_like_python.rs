//! The core's parser reads every expression as `scripts/flowexpr.py` does
//! (ADR-0132 decision 2): every expression in the design's modules, and a
//! hand-written set that reaches each construct of the grammar, each held to
//! the tree the Python parser gives it, or to its refusing it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use objectflow_core::expr::parse;
use serde_json::Value;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn python(args: &[&str], input: Option<&str>) -> Vec<Value> {
    let mut child = Command::new(common::interpreter())
        .arg(repo().join("scripts/dump-expressions.py"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the pinned Python runs the reference parser");
    if let Some(text) = input {
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(text.as_bytes())
            .expect("written");
    } else {
        drop(child.stdin.take());
    }
    let out = child.wait_with_output().expect("the reference parser ends");
    assert!(out.status.success(), "the reference parser failed");
    String::from_utf8(out.stdout)
        .expect("UTF-8")
        .lines()
        .map(|l| serde_json::from_str(l).expect("a JSON line"))
        .collect()
}

#[test]
fn every_expression_of_the_design_parses_as_python_parses_it() {
    let lines = python(&[], None);
    assert!(lines.len() > 200, "the design's expressions are dumped");
    let mut compared = 0;
    for line in &lines {
        let Some(text) = line["value"].as_str() else {
            continue; // a YAML boolean or number, which the loader converts
        };
        let tree = parse(text).unwrap_or_else(|e| panic!("{}: {e}", line["module"]));
        assert_eq!(tree.tree(), line["tree"], "{text}");
        compared += 1;
    }
    assert!(compared > 200);
}

const CONSTRUCTS: &[&str] = &[
    "metric(pass_rate, engineer := engineer, over last 30 days) >= 0.900",
    "metric(Job.open_work.engineer, assignee := inputs.engineer, fresh 1 h) < 2",
    "count(where r.outcome == Outcome.PASS) * 1.000 / count()",
    "count(distinct o.customer where o.open)",
    "count(distinct l in lines where l.open: l.product)",
    "sum(l in lines where l.state != ClaimLine.REMOVED: l.amount)",
    "all(c in checks: c.passed) and none(a in approvals where a.approver == inputs.approver)",
    "changed_since([lines, approvals], approved_at_event)",
    "-x + -(y * 2)",
    "a - b - c + d",
    "a * b / c * d",
    "a or b or c and d",
    "a and b and c or d",
    "a implies b or c implies d",
    "not not a",
    "if a then b else if c then d else e",
    "x in {A, B} or y in Priority",
    "a is not null and b is null",
    "date(now) + 1 day >= expense_date",
    "this.intervals(engineer)",
    "entered_at(OPEN) + 14 days <= now",
    "USD 0.00 < total",
    "1.5 weeks",
    "\"text with \\\" a quote\"",
    "a.b.c.d(e, f.g)",
    "x … y",
    "a < b < c",
    "a and",
    "(a",
    "{a, b",
    "count(x in)",
    "and",
    "\"open",
    "1.",
    "a $ b",
];

#[test]
fn every_construct_parses_or_fails_as_python_does() {
    let input: String = CONSTRUCTS
        .iter()
        .map(|c| format!("{}\n", serde_json::to_string(c).expect("JSON")))
        .collect();
    let lines = python(&["--parse"], Some(&input));
    assert_eq!(lines.len(), CONSTRUCTS.len());
    for (text, line) in CONSTRUCTS.iter().zip(&lines) {
        match (parse(text), line.get("tree")) {
            (Ok(tree), Some(want)) => assert_eq!(&tree.tree(), want, "{text}"),
            (Err(_), None) => {}
            (got, _) => panic!("{text}: the core gives {got:?}, Python gives {line}"),
        }
    }
}

//! The core's checker held to the flow checker (ADR-0156 decision 4): every
//! check `scripts/check-flows.py`'s run makes, over the design's modules and
//! every plant of its self-test, exported to `tests/plants` with what it
//! found, run again through the core and compared by file, line, code and
//! message, for the codes the core's checker covers so far.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use objectflow_core::check::{Severity, checked};
use serde_json::Value;

/// The codes of the steps the core runs in a module: 1 to 3 (C1), the load,
/// the schemas, the expressions and the imports, the names and relationships
/// of a module, its migration, and the notices beside them; 4 (C2), the
/// sixty-four checks; and 5 (C3), the contradictions within one type.
const COVERED: &[&str] = &[
    "yaml",
    "schema",
    "syntax",
    "imports",
    "names",
    "order",
    "required",
    "migration",
    "metric",
    "audit",
    "warn",
    "self_serviceable",
    "unknown",
    "before",
    // step 4 (C2), by group as each is ported
    "check 2",
    "check 7",
    "check 8",
    "check 11",
    "check 13",
    "check 15",
    "check 17",
    "check 18",
    "check 20",
    "check 21",
    "check 26",
    "check 29",
    "check 35",
    "check 38",
    "check 41",
    "check 47",
    "check 53",
    "check 54",
    "check 55",
    "check 56",
    "check 57",
    "check 58",
    "check 59",
    "check 60",
    "check 61",
    "check 62",
    "check 64",
    // step 5 (C3)
    "contradiction",
    "neverpasses",
    "undecided",
    "check 16",
    "check 19",
    "check 31",
    "check 34",
    "check 40",
    "check 42",
    "check 43",
];
/// In an examples file, its load, its shape, what its setups and examples
/// name, with the transitions no example requests, and each example run (C4).
const COVERED_IN_EXAMPLES: &[&str] = &[
    "yaml", "schema", "names", "order", "coverage", "example", "notrun",
];

/// The codes compared without their messages: an example's failure, which
/// the core's engine and the reference runner each explain in their own
/// words, and what each does not yet run (ADR-0156 decision 4).
const UNWORDED: &[&str] = &["example", "notrun"];

/// What the core does not yet run, by the words its notice names it with:
/// where an example reaches one, the core reports it not run and the
/// reference runner decides it, and the oracle leaves that example's outcome
/// uncompared (ADR-0156 decision 3). Each is a capability to build, not a
/// difference to keep.
const NOT_YET_RUN: &[&str] = &["it reaches a metric read in a guard,"];

/// The examples, across every case, that reach what the core does not yet
/// run: the three runner plants whose guards read a metric.
const NOT_YET_RUN_EXAMPLES: usize = 22;

/// A finding's message as the oracle compares it.
fn worded(code: &str, message: &str) -> String {
    if UNWORDED.contains(&code) {
        String::new()
    } else {
        message.to_string()
    }
}

fn covered(file: &str, code: &str) -> bool {
    if file.ends_with(".examples.yaml") {
        COVERED_IN_EXAMPLES.contains(&code)
    } else {
        COVERED.contains(&code)
    }
}

fn plants() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/plants")
}

#[test]
fn the_core_finds_what_the_flow_checker_finds() {
    let dir = plants();
    let cases: Vec<Value> = serde_json::from_str(
        &std::fs::read_to_string(dir.join("cases.json")).expect("the exported plants"),
    )
    .expect("cases.json");
    let blob = |key: &str| {
        std::fs::read_to_string(dir.join("blobs").join(format!("{key}.yaml"))).expect("a blob")
    };
    let mut wrong = Vec::new();
    let mut compared = 0;
    let mut allowed = 0;
    for case in &cases {
        let label = case["label"].as_str().unwrap_or_default();
        let files: Vec<(String, String)> = case["files"]
            .as_array()
            .expect("files")
            .iter()
            .map(|f| {
                (
                    f[0].as_str().unwrap_or_default().to_string(),
                    blob(f[1].as_str().unwrap_or_default()),
                )
            })
            .collect();
        let previous: BTreeMap<String, String> = case["previous"]
            .as_object()
            .expect("previous")
            .iter()
            .map(|(n, k)| (n.clone(), blob(k.as_str().unwrap_or_default())))
            .collect();
        // each finding by its file, line, severity, code and message, so
        // findings sharing a line are told apart and a message the core words
        // differently is a difference
        let checks = checked(&files, &previous);
        // each type's count of the guard conditions step 5 decided in full
        let counts: BTreeMap<String, (usize, usize)> = case["analysed"]
            .as_object()
            .expect("analysed")
            .iter()
            .map(|(tn, c)| {
                let n = |i: usize| {
                    usize::try_from(c[i].as_u64().unwrap_or_default()).unwrap_or_default()
                };
                (tn.clone(), (n(0), n(1)))
            })
            .collect();
        if checks.analysed != counts {
            wrong.push(format!(
                "{label}: step 5 decided {:?}, and the flow checker {counts:?}",
                checks.analysed
            ));
        }
        let found = checks.findings;
        // the examples the core does not yet run, by file and line
        let unrun: BTreeSet<(String, u64)> = found
            .iter()
            .filter(|f| f.code == "notrun" && NOT_YET_RUN.iter().any(|w| f.message.contains(w)))
            .map(|f| (f.file.clone(), f.line as u64))
            .collect();
        let outcome = |code: &str| UNWORDED.contains(&code);
        let want: BTreeSet<(String, u64, String, String)> = case["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .filter(|f| {
                let (file, code) = (
                    f[0].as_str().unwrap_or_default(),
                    f[3].as_str().unwrap_or_default(),
                );
                covered(file, code)
                    && !(outcome(code)
                        && unrun.contains(&(file.to_string(), f[1].as_u64().unwrap_or_default())))
            })
            .map(|f| {
                (
                    f[0].as_str().unwrap_or_default().to_string(),
                    f[1].as_u64().unwrap_or_default(),
                    format!(
                        "{} {}",
                        f[2].as_str().unwrap_or_default(),
                        f[3].as_str().unwrap_or_default()
                    ),
                    worded(
                        f[3].as_str().unwrap_or_default(),
                        f[4].as_str().unwrap_or_default(),
                    ),
                )
            })
            .collect();
        allowed += unrun.len();
        let got: BTreeSet<(String, u64, String, String)> = found
            .into_iter()
            .filter(|f| {
                covered(&f.file, &f.code)
                    && !(outcome(&f.code) && unrun.contains(&(f.file.clone(), f.line as u64)))
            })
            .map(|f| {
                let sev = if f.severity == Severity::Fatal {
                    "fatal"
                } else {
                    "notice"
                };
                let message = worded(&f.code, &f.message);
                (f.file, f.line as u64, format!("{sev} {}", f.code), message)
            })
            .collect();
        compared += want.len();
        for k in want.difference(&got) {
            wrong.push(format!("{label}: missed {k:?}"));
        }
        for k in got.difference(&want) {
            wrong.push(format!("{label}: extra {k:?}"));
        }
    }
    // how many examples the core does not yet run, fixed, so that one more
    // or one fewer is a change someone made on purpose
    assert_eq!(
        allowed, NOT_YET_RUN_EXAMPLES,
        "the examples the core does not yet run, across every case"
    );
    assert!(
        wrong.is_empty(),
        "{} differences over {} cases ({compared} findings of the covered codes expected):\n{}",
        wrong.len(),
        cases.len(),
        wrong.join("\n")
    );
}

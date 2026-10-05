//! The core's checker held to the flow checker (ADR-0156 decision 4): every
//! check `scripts/check-flows.py`'s run makes, over the design's modules and
//! every plant of its self-test, exported to `tests/plants` with what it
//! found, run again through the core and compared by file, line and code,
//! for the codes the core's checker covers so far.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use objectflow_core::check::{Severity, check};
use serde_json::Value;

/// The codes of the steps the core runs, 1 to 3 (C1): the load, the
/// schemas, the expressions and the imports, the names and relationships of
/// a module, its migration, and the notices beside them.
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
/// In an examples file, only its load and its shape so far: what its
/// examples name is checked with the examples (C4).
const COVERED_IN_EXAMPLES: &[&str] = &["yaml", "schema"];

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
        let want: BTreeMap<(String, u64, String), String> = case["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .filter(|f| {
                covered(
                    f[0].as_str().unwrap_or_default(),
                    f[3].as_str().unwrap_or_default(),
                )
            })
            .map(|f| {
                (
                    (
                        f[0].as_str().unwrap_or_default().to_string(),
                        f[1].as_u64().unwrap_or_default(),
                        format!(
                            "{} {}",
                            f[2].as_str().unwrap_or_default(),
                            f[3].as_str().unwrap_or_default()
                        ),
                    ),
                    f[4].as_str().unwrap_or_default().to_string(),
                )
            })
            .collect();
        let got: BTreeMap<(String, u64, String), String> = check(&files, &previous)
            .into_iter()
            .filter(|f| covered(&f.file, &f.code))
            .map(|f| {
                let sev = if f.severity == Severity::Fatal {
                    "fatal"
                } else {
                    "notice"
                };
                (
                    (f.file, f.line as u64, format!("{sev} {}", f.code)),
                    f.message,
                )
            })
            .collect();
        compared += want.len();
        let w: BTreeSet<_> = want.keys().collect();
        let g: BTreeSet<_> = got.keys().collect();
        for k in w.difference(&g) {
            wrong.push(format!("{label}: missed {k:?}: {}", want[*k]));
        }
        for k in g.difference(&w) {
            wrong.push(format!("{label}: extra {k:?}: {}", got[*k]));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} differences over {} cases ({compared} findings of the covered codes expected):\n{}",
        wrong.len(),
        cases.len(),
        wrong.join("\n")
    );
}

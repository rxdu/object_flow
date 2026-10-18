//! The core's loader reads every one of the design's modules as the flow
//! checker's strict loader does (ADR-0132 decision 2): the same enumerations,
//! types, attributes, states, conditions, invariants and transitions, each
//! transition with the same kind, states, guards and inputs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::PathBuf;
use std::process::Command;

use objectflow_core::model::{Enforce, From, Kind, Module, load_module};
use serde_json::{Value, json};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn summary(m: &Module) -> Value {
    let mut types = serde_json::Map::new();
    // the types the description declares; an observation kind's expansion is
    // the core's, which the flow checker's loader does not make
    for t in m.types.iter().filter(|t| t.observation_on.is_none()) {
        let mut transitions = serde_json::Map::new();
        for (n, x) in &t.transitions {
            let kind = match x.kind {
                Kind::Initial => "initial",
                Kind::External => "external",
                Kind::Internal => "internal",
                Kind::Assertion => "assertion",
                Kind::Erasure => "erasure",
            };
            let from = match &x.from {
                Some(From::Any) => json!("any"),
                Some(From::States(s)) => json!(s),
                None => json!([]),
            };
            let guards: serde_json::Map<String, Value> = x
                .guards
                .iter()
                .map(|(g, e)| {
                    let mode = match e {
                        Enforce::Deny => "deny",
                        Enforce::Audit => "audit",
                        Enforce::Warn => "warn",
                    };
                    (g.clone(), json!(mode))
                })
                .collect();
            transitions.insert(
                n.clone(),
                json!({
                    "kind": kind, "from": from, "to": x.to, "guards": guards,
                    "required_inputs": x.required_inputs, "optional_inputs": x.optional_inputs,
                    "inputs": x.inputs.iter().map(|(n, _)| n).collect::<Vec<_>>(),
                    "only_via": x.only_via,
                }),
            );
        }
        types.insert(
            t.name.clone(),
            json!({
                "attributes": t.attributes.iter().map(|(n, _)| n).collect::<Vec<_>>(),
                "derived": t.derived.iter().map(|(n, _)| n).collect::<Vec<_>>(),
                "states": t.states.iter().map(|(n, _)| n).collect::<Vec<_>>(),
                "conditions": t.conditions.iter().map(|(n, _)| n).collect::<Vec<_>>(),
                "invariants": t.invariants.iter().map(|(n, _)| n).collect::<Vec<_>>(),
                "transitions": transitions,
            }),
        );
    }
    let enums: serde_json::Map<String, Value> = m
        .enumerations
        .iter()
        .map(|(n, v)| (n.clone(), json!(v)))
        .collect();
    json!({"module": m.name, "enumerations": enums, "types": types})
}

#[test]
fn every_module_of_the_design_loads_as_python_reads_it() {
    let out = Command::new(common::interpreter())
        .arg(repo().join("scripts/dump-modules.py"))
        .output()
        .expect("the pinned Python runs the reference loader");
    assert!(out.status.success(), "the reference loader failed");
    let lines: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 12, "the design's twelve modules");
    let dir = repo().join("docs/design/flow-format/examples");
    for line in &lines {
        let file = line["file"].as_str().unwrap();
        let text = std::fs::read_to_string(dir.join(file)).unwrap();
        let m = load_module(file, &text).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(summary(&m), line["summary"], "{file}");
    }
}

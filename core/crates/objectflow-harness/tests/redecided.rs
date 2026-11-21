//! A decision over a type scan is decided again from the scan's matches its
//! record keeps (`adversarial-harness.md` §1, ADR-0088, ADR-0158): the
//! examples' scans refuse too rarely for a seeded run to reach one, so a
//! transcript reaches both sides here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use objectflow_core::model::OPERATOR;
use objectflow_core::request::Request;
use objectflow_harness::{Fixture, Step, fixtures, replay};

const SCANNED: &str = "\nmodule: s\ncategories: [live]\ntypes:\n  Window:\n    attributes:\n      frozen: { type: bool, default: true, indexed: true }\n    states:\n      ON: { category: live }\n    transitions:\n      open: { kind: initial, to: ON }\n  Change:\n    conditions:\n      no_freeze: { description: No window is frozen., expression: none(w in Window where w.frozen), remedy: dependent }\n      a_freeze: { description: A window is frozen., expression: any(w in Window where w.frozen), remedy: dependent }\n    states:\n      DRAFT: { category: live }\n      DONE: { category: live }\n    transitions:\n      draft: { kind: initial, to: DRAFT }\n      ship: { kind: external, from: [DRAFT], to: DONE, guards: { no_freeze: deny } }\n      hold: { kind: internal, from: [DRAFT], guards: { a_freeze: deny } }\n";

fn step(transition: &str, object_id: Option<&str>, type_: Option<&str>) -> Step {
    given(transition, object_id, type_, BTreeMap::new())
}

fn given(
    transition: &str,
    object_id: Option<&str>,
    type_: Option<&str>,
    inputs: BTreeMap<String, serde_json::Value>,
) -> Step {
    Step::Request(Box::new(Request {
        transition: transition.into(),
        actor: OPERATOR.into(),
        object_id: object_id.map(Into::into),
        type_: type_.map(Into::into),
        inputs,
        occurred_at: None,
        propose: false,
        expires_at: None,
        expected_version: None,
        target_version: None,
        principal: None,
        idempotency_key: None,
        context: None,
    }))
}

#[test]
fn a_decision_over_a_scan_is_decided_again_from_its_matches() {
    let fx = Fixture {
        name: "scanned".into(),
        modules: vec![("s.yaml".into(), SCANNED.into())],
    };
    let steps = [
        step("open", None, Some("Window")),
        step("draft", None, Some("Change")),
        // refused by the frozen window, which its scan matched
        step("ship", Some("change-2"), None),
        // satisfied by it
        step("hold", Some("change-2"), None),
    ];
    assert_eq!(replay(&fx, &steps).unwrap(), None);
}

/// A user given an identity an agent holds is refused `identity_taken`, and
/// its refusal replays over its read set, which records the agent, where it
/// recorded nothing of the actors it read (D693, found by the harness's
/// forty seeds of 300, the service's twenty-sixth).
#[test]
fn an_identity_another_actor_holds_is_refused_over_what_it_read() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/design/flow-format/examples");
    let fx = fixtures(&examples)
        .unwrap()
        .into_iter()
        .find(|f| f.name == "service")
        .unwrap();
    let inputs = |v: serde_json::Value| serde_json::from_value(v).unwrap();
    let steps = [
        given(
            "issue",
            None,
            Some("Agent"),
            inputs(serde_json::json!({"key_id": "x", "name": "text 1"})),
        ),
        given(
            "add",
            None,
            Some("User"),
            inputs(serde_json::json!({"login": "x", "name": "text 9"})),
        ),
    ];
    assert_eq!(replay(&fx, &steps).unwrap(), None);
}

const OBSERVED: &str = "\nmodule: o\ncategories: [live]\nenumerations: { Outcome: [PASS, FAIL] }\ntypes:\n  Job:\n    observations:\n      checks:\n        kind: Check\n        attributes:\n          outcome: { type: Outcome }\n    states:\n      OPEN: { category: live }\n    transitions:\n      open: { kind: initial, to: OPEN }\n";

/// A second correction of an observation is refused `corrects_current`,
/// and its refusal replays over its read set, which records the correction
/// already made, where it recorded nothing of the observations it read
/// (D694, found by the harness's forty seeds of 300, the service desk's
/// eighteenth).
#[test]
fn a_second_correction_is_refused_over_what_it_read() {
    let fx = Fixture {
        name: "observed".into(),
        modules: vec![("o.yaml".into(), OBSERVED.into())],
    };
    let check = |v: serde_json::Value| {
        given(
            "record",
            None,
            Some("Check"),
            serde_json::from_value(v).unwrap(),
        )
    };
    let steps = [
        step("open", None, Some("Job")),
        check(serde_json::json!({"subject": "job-1", "outcome": "PASS"})),
        // corrected once
        check(serde_json::json!({"subject": "job-1", "outcome": "FAIL", "corrects": "check-2"})),
        // and refused a second time, by the correction its scan found
        check(serde_json::json!({"subject": "job-1", "outcome": "PASS", "corrects": "check-2"})),
    ];
    assert_eq!(replay(&fx, &steps).unwrap(), None);
}

const SLOTS: &str = "\nmodule: hub\ncategories: [live, closed]\ntypes:\n  Slot:\n    attributes:\n      pieces: { reference: \"Piece[]\", opposite: slot, aggregation: composite, cascade: [ { on: [cancel], transition: drop, limit: 2 } ] }\n    states:\n      OPEN: { category: live }\n      CANCELLED: { category: closed, final: true }\n    transitions:\n      open: { kind: initial, to: OPEN }\n      add_piece: { kind: internal, from: [OPEN], effect: [ { create: { type: Piece, transition: add, inputs: { slot: this } } } ] }\n      cancel: { kind: external, from: [OPEN], to: CANCELLED }\n  Piece:\n    attributes:\n      slot: { reference: Slot, opposite: pieces }\n    states:\n      ON: { category: live }\n      SET: { category: live }\n      OFF: { category: closed, final: true }\n    transitions:\n      add: { kind: initial, to: ON, required_inputs: [slot], only_via: [Slot.add_piece] }\n      set_aside: { kind: external, from: [ON], to: SET }\n      drop: { kind: external, from: [ON], to: OFF }\n";

const HOLDS: &str = "\nmodule: holds\nimports: { hub: [Slot] }\ncategories: [live, closed]\ntypes:\n  Hold:\n    attributes:\n      slot: { reference: Slot, follows: [ { on: [cancel], transition: release, limit: 2 } ] }\n    states:\n      HELD: { category: live }\n      PAUSED: { category: live }\n      RELEASED: { category: closed, final: true }\n    transitions:\n      take: { kind: initial, to: HELD, required_inputs: [slot] }\n      pause: { kind: external, from: [HELD], to: PAUSED }\n      release: { kind: external, from: [HELD], to: RELEASED }\n";

fn slots() -> Fixture {
    Fixture {
        name: "slots".into(),
        modules: vec![
            ("hub.yaml".into(), SLOTS.into()),
            ("holds.yaml".into(), HOLDS.into()),
        ],
    }
}

/// A cancel its followers or its parts refuse replays over its read set,
/// which records each follower the lookup by the reference found and each
/// part the cascade read, where it recorded none and a replay without them
/// satisfied it (#103, the integrity review's finding 3): three holds past
/// a follow's limit of two, a hold paused where its release is not taken
/// from, and the same of a cascade's parts.
#[test]
fn a_refusal_by_followers_or_parts_is_decided_again_from_them() {
    let take = |slot: &str| {
        given(
            "take",
            None,
            Some("Hold"),
            serde_json::from_value(serde_json::json!({"slot": slot})).unwrap(),
        )
    };
    let cases: [(&str, Vec<Step>); 4] = [
        (
            "three followers past the limit",
            vec![
                step("open", None, Some("Slot")),
                take("slot-1"),
                take("slot-1"),
                take("slot-1"),
                step("cancel", Some("slot-1"), None),
            ],
        ),
        (
            "a follower its transition is not taken from",
            vec![
                step("open", None, Some("Slot")),
                take("slot-1"),
                step("pause", Some("hold-2"), None),
                step("cancel", Some("slot-1"), None),
            ],
        ),
        (
            "three parts past the limit",
            vec![
                step("open", None, Some("Slot")),
                step("add_piece", Some("slot-1"), None),
                step("add_piece", Some("slot-1"), None),
                step("add_piece", Some("slot-1"), None),
                step("cancel", Some("slot-1"), None),
            ],
        ),
        (
            "a part its transition is not taken from",
            vec![
                step("open", None, Some("Slot")),
                step("add_piece", Some("slot-1"), None),
                step("set_aside", Some("piece-2"), None),
                step("cancel", Some("slot-1"), None),
            ],
        ),
    ];
    let failed: Vec<String> = cases
        .into_iter()
        .filter_map(|(name, steps)| {
            replay(&slots(), &steps)
                .unwrap()
                .map(|f| format!("{name}: {f:?}"))
        })
        .collect();
    assert!(failed.is_empty(), "{failed:#?}");
}

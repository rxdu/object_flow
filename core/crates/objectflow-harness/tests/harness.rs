//! The adversarial harness over every fixture (PRD N4, ADR-0204 §6): each
//! seed's run must reach no state the declaration says is impossible, and a
//! failing run is reported as the shortest transcript that still fails.
//! `OBJECTFLOW_HARNESS_SEEDS` and `OBJECTFLOW_HARNESS_STEPS` widen a run
//! beyond the budget the gates spend.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use objectflow_harness::{Counts, fixtures, reduce, run};

fn setting(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

#[test]
fn no_run_reaches_an_impossible_state() {
    let seeds = setting("OBJECTFLOW_HARNESS_SEEDS", 6);
    let steps = usize::try_from(setting("OBJECTFLOW_HARNESS_STEPS", 120)).unwrap();
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/design/flow-format/examples");
    let mut total = Counts::default();
    for fx in fixtures(&examples).expect("the examples") {
        let mut satisfied = 0;
        for seed in 0..seeds {
            let out = run(&fx, seed, steps).unwrap_or_else(|e| panic!("{e}"));
            if let Some(f) = &out.failure {
                let reduced = reduce(&fx, &out.steps, f).unwrap_or_else(|e| panic!("{e}"));
                panic!(
                    "{} seed {seed}: {} at step {}: {}\nreduced to {} step(s):\n{reduced:#?}",
                    fx.name,
                    f.condition,
                    f.at,
                    f.detail,
                    reduced.len()
                );
            }
            satisfied += out.counts.satisfied;
            total.satisfied += out.counts.satisfied;
            total.refused += out.counts.refused;
            total.not_run += out.counts.not_run;
            total.key_reused += out.counts.key_reused;
            total.replayed += out.counts.replayed;
            total.erasures += out.counts.erasures;
            total.not_run_reasons.extend(
                out.counts
                    .not_run_reasons
                    .iter()
                    .map(|r| format!("{}: {r}", fx.name)),
            );
        }
        // a fixture whose requests were all refused tested nothing
        assert!(satisfied > 0, "{}: no request was satisfied", fx.name);
    }
    eprintln!("the harness: {total:?}");
}

/// A run is a function of its fixture and its seed, so a reported transcript
/// can be replayed and reduced (`adversarial-harness.md` §4).
#[test]
fn a_run_is_a_function_of_its_seed() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../docs/design/flow-format/examples");
    for fx in fixtures(&examples).expect("the examples") {
        let first = run(&fx, 7, 60).unwrap_or_else(|e| panic!("{e}"));
        let again = run(&fx, 7, 60).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            format!("{:?}", first.steps),
            format!("{:?}", again.steps),
            "{}",
            fx.name
        );
        assert_eq!(first.failure, again.failure, "{}", fx.name);
    }
}

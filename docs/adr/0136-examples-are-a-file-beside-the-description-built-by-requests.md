# ADR-0136: Examples are a file beside the description, built by requests, and run by the engine at publish

- **Status:** Accepted by the author, 2026-09-28: "accept all your recommendations", on the five questions put the same day about the examples ADR-0131 decisions 4 to 6 left to design.
- **Date:** 2026-09-28
- **Refined by:** ADR-0138 — an example may stub an evaluator as `unanswered`. ADR-0140 — an example may assert the warnings its verdict lists and an invariant violation's remedy.
- **Refines:** ADR-0131 (how flows are authored and checked), ADR-0135 (the source a change records)
- **Relates to:** PRD F10, F11, UC-15; `flow-format.md` §7, §10, §11; `publish-and-import.md` §1, §2; `DESIGN.md` §5.9; ADR-0132 decision 2

## Context

ADR-0131 decided that a claim about behaviour is an example the engine runs: a setup, a request and the expected verdict, with its clause and remedy class, or the state reached, run at publish against an empty in-memory store with the clock and the id source injected; coverage per transition reported, never required; and a change that breaks an example refused until it updates it. It left the form to design. Five questions were put to the author on 2026-09-28: where examples live, how a setup builds its state, what an example asserts, how time and outside systems enter, and what runs examples before the Rust core exists.

Writing the recommendation found that nothing in the repository can run an arbitrary example today: `scripts/check-scenarios.py` replays its history with each guard transcribed by hand into Python and checked against the expression's text, and does not evaluate a module's expressions.

## Decision

1. **Examples live in a file beside the description**, `<module>.examples.yaml`, opening with `examples_for: <module>`, so it is never taken for a description. It is part of a change's source, and the source digest names it `<module>.examples`, which no module's name can be (ADR-0135).
2. **A setup is built by requests from an empty store**, one holding version 0 with its operator and the version under check. Mirror objects arrive by an import step, the one route into a mirror. Setups are named and may be given one declared before them, and every setup step must apply.
3. **An example asserts the verdict.** A refusal names its verdict, and an `unsatisfied` one the clause that refuses first and its remedy, which must be the one the clause declares. An applied request names the state its object reaches and, optionally, attribute values and the transitions its cascades took.
4. **Time and outside systems are injected.** The clock starts at `2026-01-01T00:00:00Z` and moves only by a step's `after`. Every evaluator function a step or the request calls answers as the example stubs it, `satisfied` or `unsatisfied`, and a call with no stubbed answer fails the example.
5. **Examples are checked in two stages.** Now, the flow checker checks their shape against the modules: every name resolves, aliases are given before they are named, inputs have their type's form, setup steps can apply, and each expectation is one the request can give; it reports coverage as a notice. A Python reference runner, an evaluator of the expression language and the request semantics, is to run them at check time as the first slice of ADR-0132's differential tests, and the engine runs them at every publish, a failure refusing it as `example_failed`.
6. **The format is specified in `flow-format.md` §11**, with its own schema, `flow-format/examples.schema.json`, and its keys and values listed in §10 and held to that schema.

An evaluator that does not answer is not stubbed. ADR-0049 says one that is "slow or down blocks the transitions that reference it", and not with which verdict and remedy class, so an example could not state what to expect; the stubs give no third answer until that is said (`TODO.md`).

## Alternatives rejected

- **Examples inside the description**, under an `examples` key. The person approving a flow reads its rules, and twenty examples would bury them; tests are kept beside code, not in it, by most people and agents.
- **Fixture objects written directly into a setup.** It is a second write path into governed state, and it can build an object the rules could never produce, about which an example would prove nothing.
- **An expectation naming only a verdict.** A refusal for the wrong clause would pass; the clause is what makes the example a claim about a rule.
- **A real clock, or evaluators called live.** An example would give different results on different days and machines, and depend on another system being up.
- **Examples run only by the Rust core.** Nothing would run them until the core exists, and the differential tests ADR-0132 requires need the same Python interpreter, so building it once serves both.
- **Growing `check-scenarios.py`'s hand transcriptions into a runner.** Each guard would still be transcribed by hand, so an example would test the transcription, not the rule.

## Consequences

- `flow-format.md` §1, §7, §10 and §11, `examples.schema.json`, and `scripts/check-flow-format-doc.py`, which holds §10's example vocabulary to the schema.
- `scripts/check-flows.py` checks examples files after the modules, reports `coverage`, includes them in the source digest, and plants ten mistakes in its self-test; `flow-format/examples/service.examples.yaml` and `service-v2.examples.yaml` are the first examples, the second updated in the change that is version 2.
- `publish-and-import.md` §1 runs the examples at publish and §2's report lists them per transition; `DESIGN.md` §5.9 says so.
- `TODO.md` holds the Python reference runner, and the verdict and remedy of a guard whose evaluator does not answer. PRD F11 states the requirement in the author's words of 2026-09-25.

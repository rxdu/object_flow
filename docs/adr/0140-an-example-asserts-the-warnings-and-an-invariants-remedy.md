# ADR-0140: An example asserts the warnings its verdict lists and an invariant's remedy, and the design's own examples all run

- **Status:** Accepted by the author in advance, 2026-09-29: "for all similar items that you have confidence in an recommendation, just accept", the standing direction under which ADR-0128 and ADR-0139 were accepted.
- **Date:** 2026-09-29
- **Refined by:** ADR-0141 — the design documents' examples all run too, as the corpus's do.
- **Refines:** ADR-0136 (examples), ADR-0131 decision 5 (what a check reports rather than refuses)
- **Relates to:** ADR-0111 (a rule may be a flag), ADR-0122 (an invariant's remedy); `DESIGN.md` §5.5; `flow-format.md` §4.8, §11.4, §11.5

## Context

ADR-0136 decision 3 has an example assert the verdict. Writing examples for the inventory, a flow the reference runner was not written against, found two parts of the verdict an example could not state, and a way an example could stop being run without anything failing.

1. **The warnings an applied verdict lists.** The format's `warn` is the model's `flag`, and "a satisfied verdict lists each flag raised, with its clause and the remedy class that would clear it" (`DESIGN.md` §5.5). An applied expectation named its state, values and cascades, not its warnings. The inventory's second version warns of a sale below the list price, and its example passed whether the warning was raised or not.
2. **An invariant violation's remedy.** Every refusal carries a remedy, and an invariant violation's is inferred from the transition requested by a rule `DESIGN.md` §5.5 states. `flow-format.md` §11.4 let an example name the remedy only for `unsatisfied` and `call_refused`, and the runner gave the refusal none. So the rule was run by nothing.
3. **An example the runner does not run is a notice.** That is right for a description being written, which should publish before the runner is complete (ADR-0131 decision 5). For the design's own examples it meant an example could fall out of the runner's reach, as the inventory's duplicate serial did while the runner could not yet read a uniqueness on another object, and the corpus run still passed.

## Decision

1. **An applied expectation may name the warnings its verdict lists**, as `warned`: the `warn` guards that failed, in the order they were decided. The transition's own are named bare, and a caused transition's as `<Type>.<condition>`. An empty list says none failed, and leaving the key out checks nothing, as with `values` and `cascaded`. Step 3 refuses a name that is not a `warn` guard of the transition, or not a condition of the type it names.
2. **An `invariant_violated` expectation may name its remedy**, the one `DESIGN.md` §5.5 infers for the first invariant named. The reference runner infers it by that rule. Where the invariant fails on an object other than the one requested, the runner infers it only for a uniqueness, which reads the requested object through its scan. It reports an example asking for any other as not run.
3. **The design's own examples all run.** An example of the format's corpus (`flow-format/examples/`) that the runner does not run fails the corpus run of the flow checker. A description being written is still only told, by the notice `notrun`.

## Alternatives rejected

- **An `audited` key for the `audit` guards that failed.** The verdict does not list them, since the caller is not told. They are recorded only in the attempt log, and an example asserts the verdict. An audit guard is a rule on trial, and its examples come when it is enforced.
- **Naming each warning's remedy.** The remedy is the condition's own, fixed when the flow is published, so the clause's name says it.
- **Taking a missing `warned` to mean no warnings.** It would change what every existing applied example asserts, and `values` and `cascaded` already read a missing key as unchecked.
- **Requiring an invariant violation's remedy.** It is a property of the transition, not of the request, so one example per transition and invariant proves it. Requiring it on every example would repeat that proof. A `call_refused` expectation's remedy is optional for the same reason.
- **Making `notrun` fatal for every description.** A flow could then not be checked until the runner covered every construct it uses. The engine runs every example at publish regardless (ADR-0136 decision 5).

## Consequences

- `flow-format.md` §11.4 and §10, and `examples.schema.json`, gain `warned` and the remedy of `invariant_violated`. The flow checker checks both at step 3, and the reference runner compares them.
- `scripts/check-flows.py`'s corpus run fails on an example it does not run. Its self-test plants a warning left out, a warning not raised, a denying guard named as warning, and an invariant's remedy the rule does not give.

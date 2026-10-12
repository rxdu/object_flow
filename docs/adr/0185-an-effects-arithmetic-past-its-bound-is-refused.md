# ADR-0185: An effect's arithmetic past its bound is refused

- **Status:** **Decided by the author**, 2026-10-12 ("go with your recommendation on F5"), on the recommendation `TODO.md` states with question 5, after the first production review's F5, which the second found not fixed. Built the same day.
- **Date:** 2026-10-12
- **Refines:** ADR-0138 (the generated guards' remedies), `DESIGN.md` §5.5 (the generated guards) and §6 (the order of refusals), `design/declaration-syntax.md` §8.2
- **Relates to:** ADR-0143 (the core's decimal type), ADR-0180 (what a release promises), ADR-0184, `guides/wire.md`

## Context

An arithmetic whose operands are present but whose result is not representable yields unknown (`declaration-syntax.md` §8.2). That covers an integer past 64 bits, a decimal or an amount past 38 digits, a duration or a time past its range, a date out of range, and a division by zero. In a guard, unknown fails closed, as it should.

In an effect, the unknown value reached the step that writes it as an absent value. The core then stopped the request `501 not run`, saying an absent value was assigned, which only an unsupplied optional input may be. That message misnamed the cause and gave no remedy. The reference runner's integers had no bound, so it stored the value, and the two backends answered the same request differently. Both production reviews recorded this as F5.

## Decision

1. **An effect's value that an arithmetic of present values left without one refuses the request** as the generated clause `value_in_range`: `unsatisfied`, with the remedy `unreachable_from_here`, naming the object whose effect computed it. The rule applies wherever an effect evaluates an expression: an assignment, a call's or a creation's argument, a loop's collection, a condition. It covers only an arithmetic over present values. An arithmetic over an absent operand is unknown as before, and a step whose value is absent for that reason behaves as before: an argument is not passed, and an assignment is not run.
2. **Everywhere else, the same arithmetic stays unknown.** That covers a guard, an invariant, a derivation and a filter, each answering as §8.2's table says. A guard fails closed with `unknown: true`, and an invariant holds.
3. **Both backends behave alike.** The reference runner bounds its integers to 64 bits as the core does: a sum, difference, product, quotient or negation past them has no value. It also admits the least 64-bit integer as an input, as the core does (D602), and holds an integer that the wire carries as a string as an integer (D603).
4. **`value_in_range` is the name of a generated guard.** A condition may not take it (`flow-format.md` §3), and an example may expect it.
5. **`/v1` takes the new clause name before 1.0.0**, while ADR-0180's promise does not yet bind. A client that treats an unfamiliar clause like any other refusal's needs no change.

## Alternatives rejected

- **Keep `501 not run`.** Rejected: `not run` says the engine cannot run a construct, whereas here it ran the construct and the request cannot be satisfied. A caller reading `501` would report a defect in the engine, not a refusal of its request.
- **Wrapping or saturating arithmetic.** Rejected: it stores a wrong value and says nothing, which hides the failure the refusal exists to report.
- **Wider integers in the core, as the runner had.** Rejected: the store holds 64-bit integers (ADR-0143) and so does every client's typed reading of the wire. A wider type is a storage and wire change, and it only moves the bound.
- **The remedy `self_serviceable`**, since an input often carries a value past the bound. Rejected: the engine cannot tell whether the input or the stored value did. `n + inputs.by` overflows on a stored `n` near the bound just as on a large `by`, and a remedy that sends a caller to correct an input that is not wrong misleads.
- **`invalid input`.** Rejected: each input is valid on its own, and only the computation it takes part in is not.

## Consequences

- `DESIGN.md` §5.5 names `value_in_range` among the generated guards, with its remedy; §6 places it in the order of refusals, as the effect runs; and §5.7's three-valued evaluation names an arithmetic past its bound. `declaration-syntax.md` §8.2's table gains the effect's row, and `flow-format.md` §3 reserves the name.
- Writing the test turned up three more defects, resolved with it. The core's wire encoding and duration text took the magnitude of the least integer by `abs()`, which a debug build panics on and a release build wraps (D601). The runner refused the least integer as an input (D602), and it held a string-carried integer input as a string (D603).
- **Open:** the runner still keeps durations, decimals and amounts as unbounded fractions. A duration past 2⁶³ seconds, or a decimal or amount past 38 digits, is refused by the core and stored by the runner. Bounding them needs each value's type where two values combine, which the runner does not carry there. This is recorded in `TODO.md`.
- Held by `an_effects_arithmetic_past_its_bound_is_refused` (core), `test_an_effects_arithmetic_past_its_bound_is_refused_alike` (both backends, through the Studio's comparison), `an_int_leaves_json_past_the_safe_range`, and a plant of a condition named `value_in_range` that the oracle holds alike in both checkers. Each was mutation-checked: the core's refusal, its flag on a negation, the runner's bound, its decoding, its admission of the least integer, the core's reserved name, and the duration text's magnitude.

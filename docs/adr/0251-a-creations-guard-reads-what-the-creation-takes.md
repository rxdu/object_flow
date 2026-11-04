# ADR-0251: A creation's guard reads a member the creation takes as the value supplied

- **Status:** **Decided** 2026-11-04 under the author's direction ("go with your recommendations"), on the recommendation given the same day for issue #64. Built the same day.
- **Date:** 2026-11-04
- **Refines:** `flow-format.md` §7 (the `before` notice) and §7.1 (a creation reads no earlier value of its object)
- **Relates to:** PRD C2 and M7 (a rule declared once); issue #64

## Context

Issue #64, filed 2026-11-04 at 0.12.0, found that a creation's guard cannot read a member of the object it creates. A guard reads the object as it stood before the request, and a creation's object has no earlier value, so the guard reads `inputs.<name>` instead. A rule that a creation and a later act both check is then declared twice, once over the inputs and once over the object, under two names. One consumer holds six such pairs, and their names drifted apart.

Reproduced 2026-11-04 on the published 0.14.0 with the issue's probe (`TODO.md`, #64): `make`, guarded by `positive: n > 0` and taking `n`, was refused `positive` with `n: 1`, and `make_by_input`, guarded by `inputs.n > 0`, applied. The publish noticed `check:before`. The PRD asks that a rule be declared once (C2, M7).

## Decision

1. **On a creation, a guard reading a member the creation takes as an input reads the value supplied**, its object having no value before it. It reads it so by its bare name and as `this.<name>`, and through it, as `person.state` reads the supplied person's state.
2. **A member the creation writes only in its effect stays absent to its guards**, as before, since its guards run before its effect.
3. **A later act's guard reads the stored value**, as before. The `before` notice stays for it, and a creation gets none, its guard now reading the value supplied.
4. **Both checkers' step 5 reads such a member as the input on a creation**, so a guard on `n` and one on `inputs.n` constrain one value there. Two that contradict leave the creation one no request can take, which refuses the publish. On a later act they are two values.
5. **The issue's second ask, a remedy apart for creations, is left for now.** A pair whose remedy differs stays two conditions, which the issue accepts.

## Alternatives rejected

- **A new form reading the value a request leaves, on every transition.** It is larger than the need, and a second way to read a member.
- **The pairs kept.** A rule declared twice drifts, as the consumer's names did.
- **The `before` notice made fatal on a creation.** It refuses what a reader means, a guard on the member a creation is given.

## Consequences

- Built in the core's `eval.rs` (`Evaluator::with_creation`), `request.rs` (a creation's guards), `check/logic.rs` (step 5) and `check/notices.rs`, and in `scripts/flowrun.py`, `scripts/flowlogic.py` and `scripts/check-flows.py`; `flow-format.md` §7 and §7.1 say so.
- **It may refuse what 0.14.0 took.** A creation's guard reading such a member read it as absent, so `n is null or n > 0` took `n: -1` and now refuses it. The `before` notice warned of that reading.
- **Tested** on both backends: a creation guarded by `person.state == Person.ACTIVE` and `this.n > 0` taking a person and `n`, refused for a person gone and for `n: 0`; a member its effect alone writes absent to its guards; and the same condition refusing a later act once the person left (`a_creations_guard_reads_a_member_it_takes_as_supplied`). Both checkers agree on three plants: no `before` notice on a creation, two contradicting guards over a creation's member and its input refusing it, and the same guards on a later act refusing nothing. The Flow Studio's test passes through both runners alike. Six planted defects were each caught.

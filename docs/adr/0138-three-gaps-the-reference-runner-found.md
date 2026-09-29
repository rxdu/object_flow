# ADR-0138: Three gaps the reference runner found: generated guards' remedies, the order of calls and cascades, and a silent evaluator

- **Status:** Accepted by the author, 2026-09-29: "accept all three", on the recommendations put the same day.
- **Date:** 2026-09-29
- **Refines:** ADR-0038 (cascades apply sequentially), ADR-0049 (external evaluators), ADR-0122 (a flow recoverable from its description), ADR-0136 (examples)
- **Relates to:** PRD F4; `DESIGN.md` §5.5, §6; `flow-format.md` §4.8, §4.18, §11.3; `declaration-syntax.md` §5.1

## Context

The reference runner of ADR-0136 is an executable model of a request, and writing its second slice needed three answers the record did not give. It reports an example that reaches any of them as not run rather than guessing (D479 was the fourth finding, a verdict `DESIGN.md` did not list, fixed in place).

1. **The generated guards had no stated remedy.** `actor_known` is `dependent` and `<attribute>_provided` `self_serviceable`, and the other six, `occurred_within`, `whole_open`, `not_erased`, `subject_open`, `corrects_current` and `subject_owned`, had none. The inference of `declaration-syntax.md` §5.1 serves the internal form, and it would call `occurred_within` `temporal`, "wait, and it will pass", though waiting only makes a date further in the past worse.
2. **The order of an effect's calls against a composite's cascades was unstated.** `DESIGN.md` §6 step 6 and ADR-0038 take "each cascaded transition or creation in turn, depth-first in declaration order", and an effect's steps and a composite end's cascade clauses are declared in different places, so where one transition has both, which runs first was open.
3. **A guard whose evaluator did not answer had no verdict.** ADR-0049 says an evaluator "slow or down blocks the transitions that reference it", not with which verdict and remedy, so an example could not stub the case.

## Decision

1. **Each generated guard has a stated remedy:**

   | Guard | Refuses | Remedy | Why |
   |---|---|---|---|
   | `actor_known` | a request whose actor, or principal, the store holds no object for | `dependent` | as before (`DESIGN.md` §5.8) |
   | `occurred_within` | an occurred time outside the backdating bound, or before a current interval | `self_serviceable` | the caller gives another time, an input of the request |
   | `whole_open` | a re-parent into a whole in a final state | `self_serviceable` | the caller names an open whole, the input it writes |
   | `not_erased` | a write of a personal attribute of an erased object | `unreachable_from_here` | nothing undoes an erasure |
   | `subject_open` | a recording on a subject in a final state | `unreachable_from_here` | another subject would record a different fact, not correct this one |
   | `corrects_current` | a correction of a datapoint already corrected | `self_serviceable` | the caller corrects the newest one |
   | `subject_owned` | a recording on a mirror's object | `unreachable_from_here` | only the publish that cuts the type over changes it |
   | `<attribute>_provided` | a required input of an optional or defaulted attribute left out | `self_serviceable` | as before (`flow-format.md` §6) |

2. **An effect's steps run first, in their order, then the cascades.** A transition applies its new state and its effect, each step in turn and each call or creation depth-first as it is reached, and then the cascades of its composite ends, in the order the type declares the ends, its own before those it gives in `inherited_parts`. That reads ADR-0038's "its own outcome applied in full, then the cascaded transitions" literally.
3. **An evaluator that does not answer refuses its guard as unknown, with the remedy `temporal`.** The guard is `unsatisfied`, the verdict saying the value was unknown rather than false (`DESIGN.md` §5.5), and the caller may try again, as a verdict too old to use is already refused as `temporal` (ADR-0049). An example stubs the case with the answer `unanswered`.

## Alternatives rejected

- **Inferring the generated guards' remedies by `declaration-syntax.md` §5.1.** It answers `temporal` for `occurred_within`, which misleads a caller exactly where the inference itself warns that deadlines must be declared.
- **`self_serviceable` for `subject_open`.** Naming another subject changes what is recorded, so it is not a remedy for the request made.
- **`dependent` for `whole_open`.** The whole is final and cannot change; only the destination the caller names can.
- **The cascades before the effect.** It would drive the parts before the whole's own outcome is applied, against ADR-0038's order, and an effect that reads its parts would then read them already disposed of.
- **Refusing a transition that has both.** Nothing about the combination is wrong, and a delivery that releases its unit and discards its checklist in one cancellation is ordinary.
- **A silent evaluator refused as `unreachable_from_here`.** The evaluator may answer a moment later, which is what `temporal` says.

## Consequences

- `DESIGN.md` §5.5 lists the generated guards' remedies and the silent evaluator, and §6 step 6 the order; `flow-format.md` §4.8, §4.18 and §11.3, and `examples.schema.json`, which gains the stub answer `unanswered`.
- The reference runner gives these remedies and this order, and runs an example that stubs `unanswered`; `TODO.md`'s three questions close.

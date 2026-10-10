# ADR-0152: What an offer decides, before its inputs are given

- **Status:** Accepted, 2026-10-04, at the author's direction ("go ahead with slice 2"), as what the Flow Studio's second slice needs and the record did not state; accepted by the author the same day ("go with your recommendations").
- **Date:** 2026-10-04
- **Refined by:** ADR-0176 decision 2 — an `available` offer that is `actor_dependent` and asks no evaluator is simulated whole for who asks.
- **Refines:** ADR-0048 (`available` sweeps on guards and asks no evaluator), ADR-0099 (an offer lists recording on a subject), ADR-0111 (an offer lists the flags it would raise)
- **Relates to:** `DESIGN.md` §6, §10; `design/library-api.md` §5; `design/renderers.md` §4; `design/flow-studio.md` §6

## Context

`availability(id)` returns, for each requestable transition, one of three answers, `available`, `available_with_input` or `unavailable`, with the verdict, the input schema, the guards it did not evaluate and the flags it would raise (`DESIGN.md` §10, `library-api.md` §5). `available_with_input` is "the transition is permitted and the caller has not supplied everything yet" (`renderers.md` §4).

Nothing said what an offer decides when no inputs are given: whether it runs the effect and the invariants, as `check` does, and what becomes of a guard that reads an input, or of a generated guard. The reference runner could not offer `availability` until it was said, and the Flow Studio's second slice needs it.

## Decision

1. **An offer decides what a request decides that reads no input, in the order of refusals** (`DESIGN.md` §6):
   - the actor, by `actor_known`;
   - the object's state;
   - `not_erased` on what the transition's effect writes;
   - each of the transition's own guards that reads no input, in its order.

   The first that refuses is the offer's verdict, and the offer is `unavailable`. **It does not run the effect or check the invariants**: that is `check`, which takes the inputs. An offer is decided as `available` sweeps, on the state and the guards (ADR-0048).
2. **What reads an input waits for it.** That covers a guard that reads `inputs`, the generated guards on inputs (`whole_open`, `successor_not_superseded`, `occurred_within`, `corrects_current`, the `<attribute>_provided` guards), and `not_erased` on what an input would write. A transition that takes a required input, or one of whose guards waits, is `available_with_input` once the rest passes. One that takes none and has no guard waiting is `available`.
3. **No evaluator is asked.** A guard that would ask one is listed in `unevaluated` and refuses nothing (ADR-0048, ADR-0049). A guard whose evaluator would be asked nothing, since an argument is absent, is decided as a request decides it, unknown and so failing (ADR-0146).
4. **A `warn` guard that fails is a flag** on the offer, with its condition's remedy. It never makes the transition unavailable, and an `audit` guard is not listed, since it is recorded and not told (ADR-0111).
5. **For a subject, an offer of `record` for each observation kind on it**, its `creates` naming the kind. It is decided on the actor and on `subject_open` and `subject_owned`, and is `available_with_input` where the kind has a required field.

## Alternatives rejected

- **Simulating the whole request with no inputs**, effect and invariants included, which refuses every transition that takes an input as `invalid_input`, the case `available_with_input` exists to tell apart.
- **Simulating the whole request for a transition that takes no input**, which makes one operation decide two ways, and makes an offer cost what `check` costs on every listing a screen draws.
- **Leaving a guard that reads an input out of the offer silently**, which would call `available` a transition its inputs may still refuse; it makes the offer `available_with_input` instead.

## Consequences

- `DESIGN.md` §10's row states it.
- The reference runner offers `availability`, each rule proven by a self-test that fails when the rule is reverted.
- The Flow Studio shows the live offer for any object, and fires what it offers.

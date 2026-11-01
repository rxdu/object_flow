# ADR-0240: A creation says on which object it is offered, and that object's offer decides it

- **Status:** **Decided** 2026-11-01 under the author's direction ("go with your recommendations"), on the recommendation given the same day for issue #54; built the same day.
- **Date:** 2026-11-01
- **Refines:** ADR-0152 (what an offer decides before its inputs are given), decision 5 (`record` offered on a subject); ADR-0176 decision 2 (an offer simulated whole for who asks)
- **Relates to:** PRD F3; ADR-0217 (`only_via`); `docs/research/offering-creations-on-a-record.md`

## Context

`availability(id)` offers an object's own transitions, and `record` for each observation kind on it and for `label` (ADR-0152 decision 5). It offers no creation of another type that takes the object as an input. Issue #54, filed 2026-11-01 at 0.9.0, has the case: naming someone for an open slot creates a record of a type in a module above the slot's. So a consumer decides from the slot's state whether that is open, restating the creation's own guard, and the copy drifts when the flow changes. PRD F3 has an actor discover through the interface what it may do now.

The module graph points down: a module imports the modules below it, so the slot's type cannot name a creation above it. Record platforms place such an act on the parent explicitly, Salesforce's object-specific actions and Dataverse's subgrids, and state its condition again there; CMMN declares the act in a planning table and has the engine decide its applicability rule per case. No source read infers the act from the child's schema (`docs/research/offering-creations-on-a-record.md`). The issue asks the engine to infer it from every creation with a required reference of the object's type. In the shipped samples that would offer every `User` `Space.create`, as its lead, and `WorkItem.create`, as its reporter, and it would change every deployment's offers with no change to its flows. Today's clients read an offer's `creates` as a recording, filling the input `subject` (the Flow Studio's `Offers.tsx`, `wire.md`).

## Decision

1. **A creation says where it is offered.** An initial transition may declare `offered_on`: a list of at least one of its inputs, none twice (step 2, `schema`), each a required reference to one object: a declared input with `reference` and no `[]`, not optional and with no default, or an attribute the transition lists in `required_inputs` that is a reference with no `[]`. Step 3 (`names`) refuses `offered_on` on a transition that is not initial, on one with `only_via`, which no caller requests, and naming any other input.
2. **`availability(id)` offers each creation marked on an input whose referenced type the object is of, or extends**, one offer for each such input, decided as ADR-0152 decides an offer, with that input bound to the object:
   - the actor, refused `actor_known` or `actor_active` as for the object's own transitions;
   - each of the creation's guards, in order, that reads no input but the bound one and nothing but its inputs, `now`, and qualified values and states: evaluated with the bound input given and no evaluator asked, a `deny` one that fails refusing the offer, a `warn` one a flag, an `audit` one nothing, and one that would ask an evaluator listed in `unevaluated`;
   - every other guard waits, as a guard reading an input does;
   - `available_with_input` where the creation takes another required input or a guard waits, and `available` otherwise; an `available` one that an invariant may refuse for who takes it is simulated whole for who asks, as `check` would with the bound input, as ADR-0176 decision 2 simulates the object's own.
   The offer lists the creation's other inputs. A guard that would ask an evaluator reads a name that is no type and no enumeration, so it waits. A refusal names the bound input its clause reads, as ADR-0224 has a clause name the inputs it reads, and the object, as a creation's request does.
3. **The offer names what it creates and the input it binds:** `creates: <Type>` and `binds: <input>`. A client requests `{"type": <Type>, "transition": <name>, "inputs": {<binds>: <id>, …}}`. Each `record` offer names `binds: "subject"`, which `wire.md` documents for it already, and each offer of the object's own transitions `binds: null`.
4. **What is decided once is not decided again.** A creation marked on two inputs a type's objects fill is two offers. A creation `only_via` names is offered on no object, as today.
5. **An offer decided by a personal member is gated as the object's own are**: `availability` refuses without `reads_personal` where a marked creation's guards read a personal member, as it refuses for the object's own transitions.

## Alternatives rejected

- **The issue's inferred rule**, every creation taking a required reference of the object's type offered on it. It offers what no author placed, and every deployment's offers would change with no change to its flows, today's clients filling `subject` for each.
- **The creation named on the object's type.** The object's module cannot import the module above it.
- **A type's creations offered on the type**, decided for none of its objects. It offers a creation on objects whose state refuses it.
- **A `check` of the creation with the object bound and nothing else given.** It answers `invalid_input` for the missing inputs before any guard, the case `available_with_input` exists to tell apart (ADR-0152).

## Consequences

- Built in the core (`model.rs`, `check/names.rs`, `reads.rs`, the service's `availability` and its offers' `binds`) and in the reference, the checker `scripts/check-flows.py`, the runner `scripts/flowrun.py` and the Studio's sandbox, with `flow.schema.json`; the Flow Studio's offers request a creation with the object as the input it binds. `flow-format.md` §4.8 and §10, the card, `wire.md`, `library-api.md`, `DESIGN.md` §10 and `CHANGELOG.md` say so.
- **The two checkers agree** on all 695 of the oracle's cases, the new plants among them, a machine's transitions included (`checker_oracle.rs`).
- **Tested in the core:** `an_object_is_offered_each_creation_marked_on_its_type`, the offers in their order, one an input, a base's creation on its subtype, a guard on the bound input deciding, a warn guard flagging, guards reading another input or the would-be object's own attribute waiting, another required input waiting, an unknown actor refused, `binds` on a recording and on none of the object's own, and nothing unmarked, `only_via` or asked for one transition alone; and through the service, `an_offer_holds_the_creations_marked_on_its_object`, the wire's `binds`, a refusal naming its input, the simulation for an agent refused by `taken_by`, the gate on a personal guard, and the request the offer describes applying. In the reference, the checker's `offered_on_plants`, the runner's `offered_self_test` and the Studio's `test_offered_creations.py`; and the Studio's `test/offered.test.ts` against both backends.
- **Planted defects, each caught:** thirteen in the core and the service, one at each check of decisions 2 to 5, and twenty-one in the reference.
- **Not done, and why.** A version's report counts no creation offer among the offers it gains or loses, in either publish, as it counts no recording. `available`, a page of the objects a transition is open on, offers no creation, until a case needs one. The Studio's sandbox gates no read on `reads_personal`, the object's own offers included, as before.

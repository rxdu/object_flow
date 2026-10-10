# ADR-0176: The second procurement review's questions, decided

- **Status:** Decided by the author, 2026-10-10 ("go with your recommendations on the seven questions"), on the recommendation stated with each question.
- **Date:** 2026-10-10
- **Refines:** ADR-0114 and ADR-0173 decision 1 (who acted, recorded and never evaluated), ADR-0152 (what an offer decides), ADR-0154 (the import's classes), ADR-0141 and ADR-0173 decision 6 (the examples format)
- **Relates to:** `design/procurement-second-review.md` §5; `DESIGN.md` §5.5, §10; `design/flow-format.md` §4.6, §4.8, §11.3; `design/publish-and-import.md` §4, §6; `design/renderers.md` §3; `TODO.md`

## Context

The second cold procurement review (`procurement-second-review.md`) found seventeen defects, repaired the same day, and asked seven questions, each changing an accepted decision or adding a construct, so each the author's. The author answered "go with your recommendations on the seven questions".

## Decision

1. **A transition may be declared `taken_by` kinds of actor**, `taken_by: [human]`, sugar for the record and the invariant the guide had each author write by hand: it generates the attribute `<transition>_by_kind`, of type `actor_kind`, optional, which the transition's effect assigns `actor.kind`, and the invariant `<transition>_taken_by`, that the attribute is absent or one of the kinds named. The engine still judges only the record (ADR-0114): the refusal of an agent is `invariant_violated`, its remedy `delegable` (ADR-0173 decision 1), `tools` leaves the transition out for a kind not named, and its offers are `actor_dependent`.
2. **An offer that may be refused for who takes it is decided for who asks.** An offer that is `available`, `actor_dependent`, and asks no evaluator is simulated whole for the asking actor, as `check` simulates a request with no inputs; where the simulation is refused, the offer is `unavailable` with that verdict. One awaiting inputs stays `available_with_input` and `actor_dependent`, and `available` marks its page `actor_dependent` rather than simulating each object. ADR-0152 decision 1's "it does not run the effect or check the invariants" holds for every other offer.
3. **A reference to a legacy key the batch and the store lack is a class**, of kind `REFERENCE`, its shape `names nothing`, each object naming the key in its sample, decidable by `EXCLUDE`, which excludes those objects and makes theirs a class of their own as an excluded object's references are (ADR-0147). The dry run goes on past it, so one dry run lists every unresolvable reference; an import whose class is undecided is refused, as for any class.
4. **An example's `import` may name an alias an earlier import gave**, of the same family: it refreshes that object, as `import_batch` refreshes an object whose legacy key it holds, so an example can show another system changing its record.
5. **The flow checker notices an invariant that can only refuse other objects' changes**: one of a type whose every state is final, which no transition of its own can break once it is created, that reads through a reference. The notice names the reference and the fix the guide gives, copying the fact onto the record when it is made.
6. **No transition whose guards choose its target.** Routing by amount stays a pair of transitions with complementary guards per level; the guide says so and that `availability` tells a screen which to show. It is reconsidered if a third review meets it.
7. **A mirror references no type the store owns**, as before (`flow-format.md` §4.15); the guide says to keep such a link on the new side (D575).

## Alternatives rejected

- **For 1, a declared actor kind evaluated as a guard**, rejected again for the reason ADR-0173 gave: the record and an invariant already enforce it, and a condition reading the actor is what ADR-0114 refuses; the sugar changes what an author writes and nothing the engine decides.
- **For 2, every `available` offer simulated whole.** It runs every effect and invariant of every offer for every reader, and an evaluator the simulation would ask turns an offer unavailable for want of an answer the offer never asks for (ADR-0152 decision 3).
- **For 3, a key naming nothing left refused**, as it was: a dry run is where every problem should be listed, and the decision belongs in the disposition file with the rest.
- **For 6, a choice pseudostate**: a transition's target decided by guards would make a refusal's meaning depend on which branch was chosen, for a pattern two transitions already express.

## Consequences

- `flow-format.md` §4.8 gains `taken_by`, its schema the marking, and both checkers its generated names; `DESIGN.md` §5.5 and the guides say it replaces the hand-written pattern.
- ADR-0152 decision 1 is refined by decision 2; `library-api.md` §5 and the wire reference say an `actor_dependent` offer is decided for who asks, and that `available` carries `actor_dependent`.
- `publish-and-import.md` §4 and §6 gain the class; the wire reference shows it.
- `flow-format.md` §11.3 lets an import name an existing alias; both checkers, the runner, the core's run, the replay and the web player refresh.
- `flow-format.md` §7 lists the new notice.
- Each is built in its own commit, tracked in `TODO.md`.

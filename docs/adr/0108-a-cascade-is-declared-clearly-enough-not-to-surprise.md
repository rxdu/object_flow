# ADR-0108: A cascade is declared clearly enough not to surprise, and is shown where it lands

- **Status:** Accepted. The principle is the author's, 2026-09-24: "I think transition cascading should be supported, as long as the rule is defined by the user clearly enough that the transition should not appear as a surprise". The mechanism below is the recommendation put to the author with it, written at the author's direction ("ADR-0012, go ahead and write them"); the alternative of the affected type's consent was put to the author and not answered separately. Against `docs/PRD.md` revision 6: F8, F1 and UC-21; repairs D378.
- **Date:** 2026-09-24
- **Refines:** ADR-0012, ADR-0019, ADR-0020
- **Relates to:** ADR-0010 (the declaration is inspectable at runtime), ADR-0038 (cascades apply sequentially), ADR-0046 (the outcome grammar), ADR-0104 (the governed core)

## Context

Asked about ADR-0012, the author said that transition cascading should be supported, as long as the rule is defined clearly enough that the transition does not appear as a surprise.

The design already supports it. ADR-0019 lets a transition's outcome cause transitions and creations on related objects in the same transaction, and ADR-0012 already records that auto-fill on receipt would be "a declared cascade of the batch commit the caller requested, not a trigger". What ADR-0012 refuses is a transition the engine starts by itself, from a clock or an event, with no request behind it. A cascade has a request behind it: it is part of that request, declared on the transition that was requested.

Read against "not a surprise", the design already meets the principle for the requester and for history:
- a cascade is declared on the transition that causes it, and printed under that transition's `then` (`renderers.md` §2);
- it is straight-line, bounded and acyclic (DESIGN §5.4);
- it runs as the requesting actor, under the target's own actor guards unless the target is `only via` (ADR-0019, ADR-0020);
- if any cascaded guard fails, the whole request is refused, naming the object, transition and guard (ADR-0038);
- `check` simulates the whole request, cascades included, and returns its verdict before the request is made (`library-api.md` §6);
- every cascaded event records the event that caused it (DESIGN §6).

It does not meet the principle for the reader of the type a cascade reaches. An `only via` transition names its parents in its own declaration. A requestable transition names nothing: any transition in the store's declaration may `call` it (`declaration-syntax.md` §5.2), and the renderer prints a cascade only where it starts. The unit's journey has one. `Shipment.commit` inventorises every unit still in `INTAKE` (`unit-journey.md` §2), while the unit's printed rules show `inventorize` only as a transition that holders of `EDIT` request. A reviewer reading the unit's rules cannot learn from them that committing a shipment puts units on offer (D378).

## Decision

### 1. A cascade is supported where it is declared clearly enough not to surprise

A transition may cause transitions and creations on related objects when its declaration says so. "Clearly enough" means four things, each of which the design must keep true:
- **declared where it starts:** on the transition that causes it, as a step of its outcome;
- **shown where it lands:** in the printed rules of every type it reaches (§2);
- **checkable before the request:** `check` simulates the request with its cascades and names any cascaded rule that would refuse it;
- **recorded with its cause:** each cascaded event carries the event that caused it.

ADR-0019's own rules stay: a cascade is straight-line, bounded, acyclic, applied atomically and run as the requesting actor. ADR-0012 stays as well: nothing moves unless a request caused it. The PRD states the principle as F8, and UC-21 tests it.

`check` returns a verdict, not a list of what the request would change. Whether it should also return the objects and transitions a request would reach is left open. It would be an additive change to `Checked`, and it meets T5, since a cascade reaches objects its requester may not be able to see (DESIGN §5.8).

### 2. The rule set prints every cause of a transition, including those declared elsewhere

Under each transition, the printed rule set lists every transition in the store's declaration whose outcome can cause it, with the path, filter and bound as declared:
- for an `only via` transition, these are its parents, and the rule set says that it is not requestable;
- for a requestable transition, they are printed as "also caused by", so a reader sees both routes;
- a part's disposition by its whole, the `cascades on …` of a part collection, is printed on the part's type as well as on the whole's.

The list is derived, not written. Publishing already builds the graph of which transitions call which, in order to refuse a cycle (ADR-0019), and the renderer reads that graph from the other end. The list is deterministic like the rest of the rule set: causes appear in the declaration order of their types and transitions. A rule-set diff renders what publishing compares (`renderers.md` §6), so a publish that adds a cascade shows the new cause on the type it reaches. That is where that type's reviewer looks, and under F7 it is part of what the approver reads.

## Alternatives rejected

- **The affected type consents, as `only via` does.** Every transition would list which outside transitions may call it. Rejected for two reasons. Each new cascade would edit two declarations and advance two versions, which is the cost D245 removed from adding a datapoint kind. And it protects the reader no better than printing does, since the reader reads the rendered rules. `only via` stays for the case it exists for: a transition that must not be requestable at all.
- **Printing a cascade only where it starts.** This was the design's state. Rejected: the reader of the type a cascade reaches is the one it surprises.
- **Refusing at publish a cascade into a requestable transition.** Rejected: a transition that is both requested and cascaded is legitimate. A unit is inventorised by a person or by its shipment's commit, and refusing that would force two transitions with the same rules, the duplication the project exists to end.
- **Skipping a cascaded transition whose guard fails.** Rejected again, as ADR-0038 and `declaration-syntax.md` §3.2 already reject it. A silent skip surprises everyone and leaves the orphan the coverage rule exists to prevent. A cascade that fails refuses its request, loudly.
- **Transitions the engine starts on an event or a clock.** The author's principle does not cover them, and ADR-0012's reasons stand: a request is behind every change. Reacting to events and deciding when belong to upper-layer applications (ADR-0104).

## Consequences

- PRD revision 6 adds F8 and UC-21, and its §5 defines a cascade. The first release carries F8, because the first consumer's delivery completion sells its units and creates their warranties as one change (UC-21).
- `renderers.md` §2 prints the causes of each transition, with the journey's `inventorize` as its example.
- ADR-0019's principle is confirmed by the author. Its details remain in the author's review queue with the other decisions of iteration 1.
- **Flow-review decision 3 is a business choice again.** Reserving a unit at commit for the delivery its earmark names would be a declared cascade of `Shipment.commit`. It follows a choice a person recorded when the earmark was set, so it is allowed. What cannot be declared is choosing the most urgent order when the unit arrives: iteration runs in creation order and nothing ranks, so that choice stays with an upper-layer application (DESIGN §12, "Selection").
- **A cascade through a single reference cannot be conditioned.** `where` filters only a `for … in <collection>` (`declaration-syntax.md` §5.2). A unit filling the last slot of its delivery reaches the delivery through `binding`, a single reference, so it cannot cascade the delivery into `READY` only when its slot is the last one. Unconditioned, the cascade would refuse every fill while the delivery cannot yet be ready. This is recorded in `TODO.md` as a question to settle before a flow declares such a cascade.

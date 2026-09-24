# ADR-0019: A transition's outcome may cascade transitions and creations across relationships, atomically

- **Status:** Accepted — taken in autonomous design iteration 1 (2026-09-07). **Its principle was confirmed by the author on 2026-09-24:** "I think transition cascading should be supported, as long as the rule is defined by the user clearly enough that the transition should not appear as a surprise" (ADR-0108, PRD F8). The rest is pending author review
- **Date:** 2026-09-07
- **Refined by:** ADR-0066 — a cascade clause carries arguments. ADR-0038 — cascades apply sequentially, each seeing the writes of those before it, superseding the "All guards first" rule. ADR-0046 — the outcome grammar replaces the outcome description; atomicity, causality and acyclicity stand. ADR-0108 — a cascade is declared clearly enough not to surprise: the rule set prints it on every type it reaches, as well as where it starts.

## Context

The first consumer completes a Delivery and, in the same transaction, moves every bound unit to `SOLD` and creates a warranty contract per robot. Reserving a unit writes the slot on the Delivery. Committing an intake batch inventorises its units and, for each one carrying a soft peg, reserves it to its slot so that a backorder fills without a double-booking window. In the model as written, an outcome was confined to one object, and cross-object consequences reached consumers through at-least-once events (ADR-0007, ADR-0012, ADR-0013). That is neither atomic nor able to block the parent when a related object's guard fails, and it leaves a window in which a Delivery is `DELIVERED` while its units are `RESERVED` — the kind of state the project exists to prevent. Full analysis: `docs/design/first-consumer-walkthrough.md` §2.5, §3.3, §4 A–B; TODO.md challenges 1 and 2.

## Decision

A transition's **outcome** may contain, besides its own new state and controlled-attribute writes:

1. **transitions on related objects**, reached through declared relationships including declared inverses — `for s in slots: s.unit.sell()`;
2. **creation of new objects** — `create WarrantyContract(robot := slot.unit, …)`.

Execution:

- ~~**All guards first.**~~ **Superseded by ADR-0038**, which applies cascades sequentially so each sees the writes of those before it. The original text: the parent's guards, then each cascaded transition's guards, depth-first in declaration order, are evaluated before anything is written. If any fails, the whole request is blocked and the verdict names the failing object, transition and guard, with that guard's remedy class.
- **One transaction.** If all pass, every outcome commits atomically.
- **Recorded with cause.** Each cascaded transition is recorded as its own event, carrying the parent transition's event as its cause — the causal-lineage mechanism of ADR-0014, now used inside a single transaction. The log receives N+1 events from one request.
- **Straight-line.** *(The grammar is given by ADR-0046.)* An outcome may iterate a relationship, optionally filtered by a predicate (`for s in slots where s.unit is not null`), but may not choose between alternative outcomes. Where behaviour differs by a value, declare one transition per case, each guarded on that value; the availability list then shows the applicable one, and a value that matches no transition is visibly stuck rather than silently mishandled.
- **Finite.** The graph of transitions that reference each other in outcomes must be acyclic; a cycle is a declaration error. Depth is visible from the declaration.
- **Same actor.** A cascaded transition runs as the requesting actor. Its actor guards apply unless it is *only-via* (ADR-0020), in which case the parent's guards are its authority.
- **Nothing initiates.** The caller requested the parent; the cascade is its declared consequence. ADR-0012 is untouched.

## Alternatives rejected

### Consumers react to events

The model's prior position. Rejected because it cannot make the parent and its consequences one atomic fact, cannot let a related object's guard block the parent, and reopens the double-booking window that auto-fill on receipt exists to close.

### In-process handlers inside the transaction

Already rejected by ADR-0014: application code inside the write transaction. A cascade is not application code; it is a declared reference to another declared transition, gated by that transition's own declared guards.

### Conditional outcomes

`if type == DIRECT_SALE then unit.sell else unit.deliver_internal`. Rejected: the first consumer's single `else` branch is the hazard its own ADR-0002 recorded — any new delivery type silently fell through to `DEVELOPMENT` (`wr:app/core/state_registry.py:101-114`). Branching in guards makes every case a named, listable transition.

## Consequences

- ADR-0004's cascade-close, which drives terminal transitions onto each part, generalises from parts to any related object; composition remains the case where cascading is implied by the relationship rather than declared per transition.
- ADR-0007's boundary is unchanged: a cascaded transition is inside the store; an *effect* is outside it.
- ADR-0013's log records one transaction as several causally linked events; subscribers see them in that order.
- ~~An outcome iterates only declared relationships, never a query over a type.~~ **Widened by ADR-0052:** the iteration source is any expression yielding a collection, including a set-valued input and a relationship of an input. Anything needing a type scan is a guard (`none(Service where …)`) or a consumer's job.
- The readable rule set shows, for each transition, what else it causes.
- TODO.md challenges 1 and 2 are closed.

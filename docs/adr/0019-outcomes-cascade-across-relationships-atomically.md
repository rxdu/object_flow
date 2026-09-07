# ADR-0019: A transition's outcome may cascade transitions and creations across relationships, atomically

- **Status:** Accepted — taken in autonomous design iteration 1 (2026-09-07); pending author review
- **Date:** 2026-09-07

## Context

The first consumer completes a Delivery and, in the same transaction, moves every bound unit to `SOLD` and creates a warranty contract per robot. Reserving a unit writes the slot on the Delivery. Committing an intake batch inventorises its units and, for each one carrying a soft peg, reserves it to its slot so that a backorder fills without a double-booking window. In the model as written, an outcome was confined to one object, and cross-object consequences reached consumers through at-least-once events (ADR-0007, ADR-0012, ADR-0013). That is neither atomic nor able to block the parent when a related object's guard fails, and it leaves a window in which a Delivery is `DELIVERED` while its units are `RESERVED` — the kind of state the project exists to prevent. Full analysis: `docs/design/first-consumer-walkthrough.md` §2.5, §3.3, §4 A–B; TODO.md challenges 1 and 2.

## Decision

A transition's **outcome** may contain, besides its own new state and controlled-attribute writes:

1. **transitions on related objects**, reached through declared relationships including declared inverses — `for each slot.unit: unit.sell`;
2. **creation of new objects** — `create WarrantyContract(robot := slot.unit, …)`.

Execution:

- **All guards first.** The parent's guards, then each cascaded transition's guards, depth-first in declaration order, are evaluated before anything is written. If any fails, the whole request is blocked and the verdict names the failing object, transition and guard, with that guard's remedy class.
- **One transaction.** If all pass, every outcome commits atomically.
- **Recorded with cause.** Each cascaded transition is recorded as its own event, carrying the parent transition's event as its cause — the causal-lineage mechanism of ADR-0014, now used inside a single transaction. The log receives N+1 events from one request.
- **Straight-line.** An outcome may iterate a relationship, optionally filtered by a predicate (`for each slot with unit != null`), but may not choose between alternative outcomes. Where behaviour differs by a value, declare one transition per case, each guarded on that value; the availability list then shows the applicable one, and a value that matches no transition is visibly stuck rather than silently mishandled.
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

- ADR-0004's "a cascade-close proposes terminal transitions on each part" generalises from parts to any related object; composition remains the case where cascading is implied by the relationship rather than declared per transition.
- ADR-0007's boundary is unchanged: a cascaded transition is inside the store; an *effect* is outside it.
- ADR-0013's log records one transaction as several causally linked events; subscribers see them in that order.
- An outcome iterates only declared relationships, never a query over a type. Anything needing a type scan is a guard (`none(Service where …)`) or a consumer's job.
- The readable rule set shows, for each transition, what else it causes.
- TODO.md challenges 1 and 2 are closed.

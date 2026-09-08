# ADR-0022: Time is a value in guards, and the read surface answers "for which objects is this transition available"

- **Status:** Accepted — taken in autonomous design iteration 1 (2026-09-07); pending author review
- **Date:** 2026-09-07

## Context

ADR-0012 places all initiation above ObjectKeeper, including time-driven rules, and TODO.md asked whether timing rules could nonetheless live in the declaration so they do not drift from the type they concern. The first consumer has such rules from day one: a warranty contract expires when its end date passes; procurement is overdue past its expected arrival; a lease is overdue past its expected return (`wr:docs/proposals/operations-system-design.md` §6). Walkthrough §4 E; TODO.md challenge 5.

## Decision

1. **`now` is a value in the expression language** (ADR-0021). A time-driven transition is an ordinary transition whose guard reads it: `WarrantyContract.expire: ACTIVE → EXPIRED, guard end_date <= now`. A guard that fails on such a clause reports remedy class `temporal`.
2. **ObjectKeeper never initiates it.** ADR-0012 stands unchanged.
3. **The read surface provides an availability query:** given a type and a transition name, the objects for which that transition is currently available, or available-with-input, for a given actor. **No external guard, eager or deferred, is evaluated by this query, and the result says so (ADR-0048, ADR-0049).** The query also requires the transition to be *sweepable* (ADR-0048); a transition that is not is refused rather than scanned.
4. A scheduler above ObjectKeeper asks that query on its own cadence and requests the transition for each answer, with an idempotency key derived from the object and the period, so a repeated sweep is harmless (ADR-0014).

The guard *is* the timing metadata: one declared rule, inspectable, and no scheduler in the store.

## Alternatives rejected

### Timing rules as separate, unexecuted metadata on the type

The form TODO.md suggested. Rejected as redundant once `now` is a guard value: the rule would duplicate the guard and could drift from it.

### Durable timers in the store

Rejected by ADR-0012; nothing here reopens it.

## Consequences

- The availability query is a required part of the read surface, not an optional convenience; it also answers "which objects are blocked, and why" for the same price.
- Alerts that are not transitions — "lease overdue" — are derived attributes the automation layer queries by filtering the stored operand the expression compares against `now`, since a clock-dependent derived value cannot itself be indexed (ADR-0048).
- TODO.md's "inspectable but unexecuted timing metadata" item and challenge 5 are closed.

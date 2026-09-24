# ADR-0043: Subscription progress is runtime state, not object state; lag and death are derived, so nothing initiates

- **Status:** Accepted — repair of D07, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refined by:** ADR-0100 — progress is an acknowledged settled cursor, and lag is the age of the oldest unacknowledged event the filter selects, with duration thresholds. ADR-0105 — a subscription names its reader, the one actor who may pull and acknowledge it, and creating one requires `OK_SUBSCRIBE`; there is no delivery worker in the core, so what this decision says a worker does is a relay's, above it.
- **Refines:** ADR-0034

## Context

Defect D07 found that the built-in `Subscription` could not be operated under its own rules, in three ways. Its cursor was a controlled attribute with no declared writer, while `pull` was a read-surface operation that would have to advance it. Its `lagging` and `dead-lettered` states were to be advanced "by the delivery worker", which is part of an ObjectKeeper deployment and therefore forbidden from initiating anything by ADR-0012. And every acknowledgement, being an action, would record a permanent event at delivery rate.

The mistake was modelling delivery bookkeeping as object state. A cursor is not a business fact about a subscription; it is operational progress.

## Decision

1. **The Subscription object holds only its declaration and its lifecycle**: the filter, the lag thresholds, and states `active → revoked`. Both transitions are ordinary and are requested by a consumer or an operator.
2. **Progress is runtime state.** The acknowledged position lives in its own store alongside the idempotency record, is not an object, is not versioned, and emits no events. `acknowledge(position)` and `pull` write and read it directly. *(ADR-0089: it is now an acknowledged settled cursor.)* High-frequency bookkeeping never enters the permanent history.
3. **Lag and death are derived, not states.** `lag := log_head - acknowledged`, `lagging := lag > warn_threshold`, `dead := lag > fail_threshold`. They are computed on read from the position and the thresholds. *(Replaced by ADR-0100 §6: the lag is the age of the oldest event the filter selects after the acknowledged cursor, and the thresholds `lag_warn` and `lag_fail` are durations, since a cursor has no arithmetic.)*
4. **Nothing initiates.** Because lag and death are derived, no actor has to move a subscription into them. An operator queries for lagging subscriptions and decides; a delivery worker reads `dead` and stops delivering. ADR-0012 is untouched, and the same reasoning applies to `Proposal.expired` (ADR-0044).
5. **Revival needs no special case.** A revived subscription sets its acknowledged position to any value; nothing was pruned, so any position is valid (ADR-0033).

## Alternatives rejected

### Make `pull` a transition so the cursor has a writer

Rejected: it would make reading the feed a recorded write, and every consumer's poll would land in the permanent log.

### Let the delivery worker request the lifecycle transitions

Rejected: it makes an ObjectKeeper deployment an actor with its own agency, which ADR-0012 rejects and which would give a class of state change no external actor asked for.

### Keep lagging and dead as real states, advanced by an operator

Workable but worse: the state would be stale between sweeps, and "is this subscription behind" is a question with an exact answer at any instant. A derived value cannot go stale.

## Consequences

- ADR-0034's rule 4 is superseded on the shape of the object; its ordering guarantees and filter design stand.
- The storage schema gains a subscription-progress table beside the idempotency table, and neither is an object type.
- A general principle worth carrying: **operational bookkeeping is not object state.** Anything written at delivery or request rate, rather than at business-event rate, belongs outside the log.
- D07 is resolved.

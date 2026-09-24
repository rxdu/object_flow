# ADR-0034: Events are ordered per object and per cause; subscriptions are a built-in object type with a declared filter and their own lifecycle

- **Status:** Accepted — taken in autonomous design iteration 4 (2026-09-08); pending author review
- **Date:** 2026-09-08
- **Refined by:** ADR-0089 — the log is read by a settled cursor ordered by transaction and position. ADR-0043 — subscription progress is runtime state, not object state, and lag and death are derived rather than states. ADR-0105 — there is no delivery worker in the core; a relay above it pulls as its own actor, under its own visibility.

## Context

TODO.md left open the ordering scope of the log, subscription granularity, and whether subscriptions are a built-in object type. A busy order log needs all three answered. Case study: `docs/design/case-study-orders.md` §3.

## Decision

**Ordering.**

1. Events of one object are strictly ordered by a per-object sequence.
2. A cascaded event is ordered after its cause (ADR-0019), so causal order holds across objects within one request.
3. Every event has a **global position**, monotonic, used by cursors. It is not a promise of commit order under concurrency: a pull cursor must tolerate a bounded window in which a lower position becomes visible after a higher one, and the pull interface states that window. No total order across unrelated objects is promised beyond that. *(Refined by ADR-0089: cursors are now (transaction id, position) pairs bounded by the reader's snapshot, which cannot skip a late commit.)*

**Subscriptions.**

4. `Subscription` is a built-in object type. **ADR-0043 supersedes the shape below**: progress is runtime state, not object state, and lag and death are derived rather than states. Original text: its controlled attributes are the filter, the cursor position and the acknowledged position; its actions are `acknowledge(position)`; its lifecycle is `active → lagging → dead-lettered`, with `lagging` entered when the unacknowledged span exceeds a declared threshold and `dead-lettered` when it exceeds a second one, both time-driven through the availability query (ADR-0022) by the delivery worker or an operator.
5. The **filter** is a type or family (ADR-0026), an optional set of transition names, and an optional `changes_state` requirement. Attribute-level filters are not offered; they would need the query engine per event, and the consumer can filter after delivery.
6. A subscriber acts as an actor (ADR-0025); visibility (ADR-0030) applies to what it is delivered.
7. Because the log is permanent (ADR-0033), a subscription that has fallen behind can be revived at any position; a gap is reported, never silently skipped. *(This said "dead-lettered"; ADR-0043 made lag and death derived rather than states.)*

## Alternatives rejected

### A global total order

Rejected: it serialises every commit through one point and buys nothing the per-object and causal guarantees do not already give consumers.

### Attribute-filtered subscriptions

Rejected for now: per-event predicate evaluation over arbitrary attributes is the read surface's query engine running on the write path.

### Subscriptions as consumer-managed cursors outside the store

*Substantially adopted later. ADR-0043 moved the acknowledged position out of the object, so it is not versioned and emits no events, and made lag and death derived rather than states. The reasoning below is what that ADR overturned.*

Rejected: a cursor's lag and death are operational facts that need history and alerts, which is what an object type provides.

## Consequences

- TODO.md's ordering, granularity and built-in-type questions are closed.
- The recursion terminates: `Subscription` is built in and not user-definable, so no subscription subscribes to subscriptions.
- The delivery worker is a consumer of the log like any other, with an actor that sees everything.

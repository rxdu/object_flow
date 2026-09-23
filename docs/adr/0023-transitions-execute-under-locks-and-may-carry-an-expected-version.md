# ADR-0023: A transition executes under locks on everything it writes, and a request may carry the version the caller last read

- **Status:** Accepted — taken in autonomous design iteration 1 (2026-09-07); pending author review
- **Date:** 2026-09-07
- **Refined by:** ADR-0039 — serialisable isolation replaces rules 2, 3 and 6; ADR-0076 — the idempotency replay is checked before `expected_version`, so `stale` is possible only for a request that has not been applied.

## Context

Double-booking was one of the first consumer's three motivating pains. Its fix was a row lock on reservation and a `version` column on every entity, with a stale write refused (`wr:docs/DESIGN.md` §2.6); its ADR-0002 notes the check-then-act hole a service-layer check leaves. DESIGN.md said nothing about concurrency. Walkthrough §4 F; TODO.md challenge 4.

## Decision

1. **One transaction per request.** A transition request — the parent and every cascaded transition and creation (ADR-0019) — runs in one database transaction.
2. ~~**Lock what is written.**~~ **Superseded by ADR-0039**: the lock set cannot be computed in advance, so locks are taken as objects are reached and correctness comes from serialisable isolation. Original text: every object the request will write is locked for update before any guard is evaluated. Concurrent conflicting requests serialise; the later one evaluates its guards against the earlier one's committed result and fails or succeeds honestly.
3. ~~**Guards read a consistent snapshot.**~~ **Superseded by ADR-0039**, and this rule was the source of defect D02: granting serialisable isolation to invariants alone left every cross-object *guard* unprotected, which defeated ADR-0024's deletion guard by construction. Under ADR-0039 the whole transition transaction is serialisable. Original text: objects that are only read are not locked; a type-level invariant that a concurrent request could violate from the other side is enforced by a database constraint where one can express it, and otherwise under serialisable isolation with a bounded retry.
4. **Every object carries a version**, incremented on every recorded change and returned by every read.
5. **A request may carry `expected_version`.** If the object's version differs, the request is refused with a **`stale`** verdict before any guard runs. `stale` is a distinct verdict class, not a guard failure: its remedy is "re-read and decide again", which is neither supply, wait, ask, nor work on another object.
6. ~~Locks are taken in a fixed order (by object id) so cascades cannot deadlock against each other.~~ **Withdrawn by ADR-0039**: created objects have no id to order by, and the set is discovered by traversal. Deadlock is handled by retry.

## Alternatives rejected

### Optimistic versioning only

Rejected: a version check happens at write time, after guards have read possibly stale state; reservation needs the lock. Retrying agents make the race routine rather than rare.

### Locking only

Rejected: the lock makes the request safe but says nothing about whether the caller's decision was made on current information. An agent that listed available transitions, then requested one a minute later, should be told the world moved.

## Consequences

- The verdict taxonomy is: satisfied; unsatisfied with a reason and remedy class; `stale`; not requestable (ADR-0020); and, since ADR-0030, not found. *(ADR-0041 is the current list, adding `over-limit`; DESIGN.md §5.5 carries all six.)* *(Corrected 2026-09-23: `DESIGN.md` §5.5 carries seven, ADR-0041's six and invariant violated, which names the invariant and the conflicting objects.)*
- The read surface returns `version` with every object.
- The first consumer's `delivery_locking` and `reserve_inventory_item` patterns are absorbed; no consumer writes its own lock.
- TODO.md challenge 4 and the concurrency item are closed.

# ADR-0049: External evaluators are consulted outside the write transaction, and their verdict carries an as-of time

- **Status:** Accepted — repair of D32, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refined by:** ADR-0054 — four semantics the repair left open — parent ordering, partial `check`, built-in assertion, and discharged admissions; ADR-0069 — an evaluator returns a verdict; an external system that assigns is a mirror. ADR-0084 — a metric guard is consulted before the transaction like an evaluator, with its arguments resolved again inside it.
- **Refines:** ADR-0008

## Context

ADR-0008 lets a guard reference a named external evaluator, marked eager or deferred, where deferred means "checked only at execution". Neither DESIGN.md §6 nor ADR-0023 gave it a place in the sequence. Read literally, a deferred guard makes a third-party network call inside the write transaction while holding row locks, which is the cost ADR-0014 rejected in-process handlers for, and which under ADR-0039's serialisable isolation now also lengthens the window for serialisation failures.

There is a deeper point. Serialisable isolation protects reads of the store. It cannot protect a fact that lives in Xero. No placement in the sequence makes an external fact true at commit.

## Decision

1. **No external call happens inside the write transaction.** A request that names external guards consults them first, outside the transaction, then opens the transaction and evaluates everything else.
2. **An external verdict carries an as-of time**, and that time is recorded on the event beside the verdict. The guarantee for an external fact is explicitly *as of T*, not *at commit*.
3. **A declaration may set a freshness bound** per evaluator. A verdict older than the bound is refused rather than used, with remedy class `temporal`.
4. **`available` and `check` do not call evaluators at all** and say so in their results (ADR-0048).
5. The eager and deferred marking of ADR-0008 keeps its meaning for *when in a caller's workflow* the evaluator is consulted, not for where in the transaction, since the answer to that is now always "before it".

## Alternatives rejected

- **Call inside the transaction, as the literal reading implied.** Rejected: it holds locks across a network, and it buys nothing, since the external fact can change immediately after the call regardless.
- **Mirror every external fact locally and guard on the mirror.** Already available as a modelling choice, and ADR-0008 notes the cache that can silently lie. Rejected as the general rule because it requires a sync path per fact.
- **Refuse external guards entirely.** Rejected: ADR-0008's case stands, and the Xero invoice check is a real requirement of the first consumer.

## Consequences

- The honest statement of the guarantee gains a clause: for guards over external facts, the store enforces the verdict it was given, as of the time it was given.
- A deployment whose evaluator is slow or down blocks the transitions that reference it, which is correct and visible rather than silent.
- D32 is resolved.

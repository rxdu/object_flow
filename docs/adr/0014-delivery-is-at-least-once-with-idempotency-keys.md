# ADR-0014: Delivery is at-least-once; exactly-once effect comes from idempotency keys

- **Status:** Accepted
- **Date:** 2026-09-07

## Context

The question raised was whether requiring subscribers to acknowledge receipt could yield exactly-once delivery.

It cannot. If an acknowledgement is lost — network drop, or the subscriber crashing after processing but before acknowledging — the sender cannot distinguish *"never received"* from *"received, ack lost"*. Both present identically as no acknowledgement, yet demand opposite responses. Acknowledging the acknowledgement does not help; that message can be lost too, without limit. This is the Two Generals Problem, and it is provably unsolvable over an unreliable channel.

The only available choice is which error to make:

| Choice | Guarantee | Failure mode |
|---|---|---|
| Retry when no ack arrives | at-least-once | duplicates |
| Do not retry | at-most-once | silent loss |

Given the no-loss requirement of ADR-0013, the choice is forced.

## Decision

**Delivery is at-least-once.** Subscriber handlers must be idempotent; this is part of the interface contract, not an implementation detail.

Exactly-once *effect* is achieved at the receiver, by deduplicating on the event identifier within the same transaction as the side effect.

**Transitions accept an idempotency key.** For the common case — an event causing a transition on another object — the source event identifier is used as that key, and ObjectKeeper refuses a second attempt carrying a key it has already applied.

**Acknowledgements are still required**, for progress tracking, lag and stuck-subscriber detection, safe pruning of the log, and backpressure. They are not a correctness mechanism for exactly-once.

## Alternatives rejected

### Acknowledgement-based exactly-once delivery

Rejected as impossible rather than merely difficult; see Context.

### Requiring every subscriber to maintain its own deduplication table

Workable, and still necessary for subscribers whose effects are external (raising an invoice in another system). Rejected as the *general* answer because the most common effect is a transition back into ObjectKeeper, which is precisely where deduplication can be done centrally, in a component that already has the transaction and already mediates every change.

### In-process handlers inside the transition's transaction

Genuinely exactly-once, because there is no network and no separate failure domain — the handler commits with the transition or rolls back with it. Available only in library mode.

Not adopted as a default: it runs application code inside the write transaction, which means long transactions, lock contention, and an application bug becoming a failed state change. It also places an effect inside the store, brushing against ADR-0007.

## Consequences

- The documented guarantee is **at-least-once delivery with idempotent handling**, stated in the interface contract so that non-idempotent handlers are a caller error rather than a surprise. A handler that raises an invoice without a reprocessing guard will raise two.
- Idempotency keys pay twice: they deduplicate, and because the key is recorded on the transition they give **causal lineage** in history — "this deployment advanced because of event 10471, which was Task 512 completing." Provenance becomes not only *who* but *what caused this*, which is the reconstruct-how-it-got-here property the original brief asked for, arriving as a by-product.

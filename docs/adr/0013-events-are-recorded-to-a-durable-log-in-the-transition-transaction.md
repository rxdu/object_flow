# ADR-0013: Events are written to a durable ordered log in the transition's own transaction

- **Status:** Accepted
- **Date:** 2026-09-07
- **Open challenge (2026-09-07):** at-least-once delivery to a consumer is the path by which a cross-object consequence (delivery completed, units sold) would become eventually rather than atomically consistent. See TODO.md, "Challenges from the first consumer", item 1.

## Context

Because ObjectKeeper initiates nothing (ADR-0012), applications must learn what happened in order to react. The requirement stated was stronger than convenience: **no registered subscriber may lose an event.**

The naive implementations both fail silently:

```text
commit the transition, then notify   → process dies between the two; event lost
notify, then commit the transition   → commit fails; subscriber acted on a change
                                       that never happened
```

Neither failure is visible in testing.

## Decision

Every recorded transition writes its event to a **durable, ordered log inside the same database transaction that records the state change**. They commit atomically: either both landed or neither did. This is the transactional outbox pattern.

That log is the single substrate for two delivery modes:

```text
            recorded transition
                    │
         durable ordered log        ← written in the same transaction
                    │
        ┌───────────┴───────────┐
     pull                     push
  consumer holds          delivery worker
  a cursor                calls subscribers
```

Push delivery is inside the scope boundary of ADR-0007. Delivering a record is not making a decision: the application decided in advance that it wanted to know, and the payload says *this happened*, not *do this*. What would fall outside is ObjectKeeper deciding an outcome and acting on it.

## Alternatives rejected

### Notify without a durable log

Call subscribers at the moment of the transition, keeping no queue.

Rejected: it cannot satisfy no-loss. Any design that guarantees delivery at the moment of the call is guaranteeing something it does not control — the network, the subscriber's uptime, its own process lifetime. Moving the guarantee into the transaction that records the change is the only place it can hold.

### A log only when push delivery is needed

Earlier in discussion the log was treated as optional for consumers happy to poll.

Rejected once no-loss became a requirement: polling sees *state*, a feed sees *transitions*. An object that goes `open → escalated → open` between two polls is observed as `open` both times, and the escalation is never learned. For a system whose value is a trustworthy record, that gap is unacceptable.

## Consequences

- The log is mandatory, not an optional feature.
- Retention interacts with subscriber progress: an event not yet consumed by a registered subscriber cannot be pruned, so one dead subscriber would otherwise pin the log forever. A retention and slow-consumer policy is required (open in TODO.md).
- Push delivery needs a worker draining the log, which reintroduces a background process — **for push only**. Pull-only deployments remain library-shaped (ADR-0012).
- This admits a staging that costs nothing later: build the log and the pull interface first, add the delivery worker when an application actually wants callbacks. The core does not change when that happens — a consumer is added, not modified.

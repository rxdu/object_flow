# ADR-0033: Current state is stored; the event log is the permanent history and is never pruned; provenance attaches to events

- **Status:** Accepted — taken in autonomous design iteration 4 (2026-09-08); pending author review
- **Date:** 2026-09-08

## Context

TODO.md asked whether current state is a stored column or a fold over events, where provenance attaches, and how retention interacts with slow consumers when no event a subscriber has not consumed may be pruned. The order case makes the log the busiest table in the system and guards the busiest readers. Case study: `docs/design/case-study-orders.md` §3.

## Decision

1. **Current state is stored.** Each object is one row: id, type and declaration version, state, controlled and free attributes, `version`, and the id of the last event that changed it. Guards, derived attributes, visibility predicates, type-scan invariants and the read surface read this row. Indexes are declared on it.
2. **The event log is the history**, written in the same transaction (ADR-0013). A fold of an object's events must reproduce its row; a repair tool may verify and rebuild rows from the log, never the reverse.
3. **Provenance attaches to the event**: actor and principal (ADR-0025), `context`, cause (the parent event for a cascade, ADR-0019; the source event for an event-driven request, ADR-0014), declaration version (ADR-0027), and a `source` of `observed` (a transition produced it), `asserted` (import or override), `migrated` (ADR-0027), `corrected`, or `erased` (ADR-0031).
4. **The log is never pruned.** History is the recorded property; a subscriber's position is a cursor over a permanent log, so a dead subscriber pins nothing. Retention is **archival tiering**: old events may move to cheaper storage but remain readable by id and by object, and remain reachable by erasure.
5. Object rows are never deleted either; deleted and superseded objects keep their row in their terminal state (ADR-0024, ADR-0028).

## Alternatives rejected

### Fold state from events on read

Rejected: every guard evaluation would replay history; at order volume that is the whole database on every request, and indexes on current values become impossible.

### Prune the log past all subscribers' cursors

The assumption behind TODO.md's retention question. Rejected: the log is the history, and pruning it would make the recorded property depend on subscriber behaviour.

### Two stores — a compact "history" and a prunable "outbox"

Rejected: two writers for one fact. The outbox is a cursor over the history, not a copy.

## Consequences

- TODO.md's "state: stored or folded" and "retention versus slow consumers" are closed.
- Slow-consumer handling is entirely about the subscription's own lifecycle (ADR-0034), not about the log.
- Storage grows monotonically; archival tiering and partitioning by time are implementation concerns the spec must name.

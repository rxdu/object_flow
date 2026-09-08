# ADR-0035: An approval is a recorded part; "needs approval" is a guard over those parts, invalidated by content change through `changed_since`

- **Status:** Accepted — taken in autonomous design iteration 5 (2026-09-08); pending author review
- **Date:** 2026-09-08

## Context

DESIGN.md listed approvals as in scope and never defined them; the terminology table carried a working reading. Approval flows come in fixed shapes — one approver, N-of-M, sequential chains, thresholds, separation of duties — and share one hard requirement: a material edit after approval must invalidate it, without every editing action having to remember to reset something. Case study: `docs/design/case-study-approvals-and-bookings.md` §2.

## Decision

1. **An approval is a part** of the approved object: an `Approval` object with approver, principal, decision, kind, comment, and the event that recorded it, created by an `approve` or `reject` action on the parent (ADR-0016, ADR-0019). It is history, not a flag.
2. **"Needs approval" is a guard** on the transition that needs it, counting valid approvals. *(Written in the current grammar, ADR-0047 and ADR-0052: `count(a in approvals where a.decision == APPROVED and a.kind == MANAGER and not changed_since([amount, vendor], a.event)) >= 1`. The attribute list is inline, so the named `relevant` set this ADR implied is not a declaration element.)* N-of-M, all-of-a-set, sequential chains and thresholds are guards of this shape, on the gated transition or on the later `approve` actions.
3. **`changed_since(attributes, event)`** is added to the expression language: true if any of the named attributes of `this` has been written by an event later than the given one. *(An earlier draft added "a type may name a set `relevant`" as a declaration element; ADR-0052 removed the need for it, since the attribute list is written inline.)* This is the only new construct; it reads the log, which is permanent (ADR-0033).
4. **Who may approve** is an actor guard on the action (ADR-0025). Separation of duties is `actor.id != requested_by.id` and `none(a in approvals where a.approver == actor.id …)`.
5. The verdict for a missing approval is `delegable`, naming the capability and kind required.

## Alternatives rejected

### An approval flag on the object, reset by editing actions

Rejected: every editing action must remember the reset, which is the one-side-covered hazard ADR-0009 exists to prevent, and the flag has no history.

### A workflow-engine approval step

Rejected: it moves the rule out of the type and into an orchestration layer (ADR-0007), where it drifts from the transition it gates.

### Approval as a separate top-level object with a reference

Rejected: an approval cannot outlive what it approves and belongs to exactly one object; that is composition (ADR-0003).

## Consequences

- The terminology row for **Approval** in DESIGN.md is replaced by this definition.
- The language grows by one predicate over history; ADR-0021's rule that growth is by decision is honoured.
- Approval age and escalation are derived attributes and the availability query (ADR-0022).

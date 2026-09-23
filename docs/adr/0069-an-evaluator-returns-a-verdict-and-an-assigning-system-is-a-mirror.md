# ADR-0069: An evaluator returns a verdict; an external system that assigns is a mirror

- **Status:** Accepted — recommendation accepted by the author 2026-09-08
- **Date:** 2026-09-08
- **Refined by:** ADR-0080 — the thing decided here is an externally owned type, an ordinary type the sync writes; `mirror` is reserved for ADR-0075's cutover marking.
- **Refines:** ADR-0007, ADR-0008, ADR-0014, ADR-0049
- **Answers:** open question 8

## Context

An external evaluator returns a verdict and never a value. A reviewer observed that this leaves the integrations which *assign* something unhandled: a randomisation service choosing a treatment arm, a pricing service quoting. The caller supplies the value and the evaluator can only be asked whether it is consistent, so the store records a value it did not verify — which is the assertion the store exists to avoid trusting.

Evidence from the first consumer. Four integrations exist in code and **every one is confirm-only**:

- **Jira** verifies an issue key the operator supplied. Its `create_issue` call exists and is deliberately unused, with a comment saying so (`wr:app/services/procurement_order_service.py:301`), and a Jira outage is logged while the link is accepted.
- The **label print server** returns a status; only a confirmed normal status lets the application stamp its own printed-at from local time.
- **S3** is passive storage for a key the application generates.
- **Redis** is infrastructure.

The one system that would assign a value is **Xero**, and it is not integrated: searching the application for it returns nothing. It exists as an accepted-but-unimplemented ADR in that repository, and what that ADR proposes is a stored contact id, re-pointed when Xero reports a merge — which is a **mirrored column**, not an evaluator return.

## Decision

An evaluator returns a verdict. An external system that decides or assigns a value is modelled as a **mirror**: an object here with an external identifier, written by the consumer on observing the event, reconciled with the external system's own identifier as the idempotency key (ADR-0008, ADR-0014). *(ADR-0080 renamed this an **externally owned type** and reserved `mirror` for ADR-0075's cutover marking. The mechanism here is unchanged: an ordinary type whose sync-driven transitions the sync requests.)*

The syntax document states this where a reader meets evaluators, so the boundary is met at the point of confusion rather than found later.

## Alternatives rejected

- **Let an evaluator return a typed value the store writes.** Rejected because it creates a second way to import external truth, and the two would differ in what they record: a mirror is an object with a history, an evaluator return is a value with a verdict attached to one event. The design already chose the one that leaves a record.
- **A hybrid: a verdict carrying an opaque payload.** Rejected as the worst of both — a value the store holds, cannot type, and cannot check.

## Consequences

- A randomisation arm, a gateway's authorisation code and a quoted price are all mirrored objects. *(In ADR-0080's terms, objects of an **externally owned type**: an ordinary type with an external identifier, whose sync-driven transitions the sync requests; `mirror` now means only the cutover marking; `DESIGN.md` §5.5, §14.)* Each has a lifecycle, which is usually what the domain wanted anyway.
- The window between the store's transition and the external action stays real and visible, as `design/edge-cases.md` already records under atomicity.
- The first consumer's Xero work, when it happens, is a mirror by its own design intent, so nothing in this decision is new to it. *(ADR-0080 §3: the customer is a `mirror` only while the legacy system owns it, and at its cutover stage becomes an externally owned type whose sync is Xero's; `DESIGN.md` §11.)*

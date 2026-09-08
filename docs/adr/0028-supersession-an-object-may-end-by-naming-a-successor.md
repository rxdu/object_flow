# ADR-0028: Supersession — an object may end in a terminal state that names its successor

- **Status:** Accepted — taken in autonomous design iteration 2 (2026-09-07); pending author review
- **Date:** 2026-09-07

## Context

Real systems change an object's kind: an issue moves to another project and takes a new workflow; a subtask is converted to an issue; a duplicate customer is merged into the canonical one (iteration 3). An object's type is fixed here — one machine per type (ADR-0003), and a history read under two machines is unreadable — and its id never changes (ADR-0018). Case study §3.

## Decision

1. A type may declare a terminal state as **superseding**. Its transition takes a `successor` input, an object id of the same or another type, or a name bound to an object the same outcome created (ADR-0046, which resolved the contradiction between this clause and clause 2), and records `superseded_by`. The successor may declare the inverse, `supersedes`.
2. The old object keeps its id and its whole history. The successor is created, or already exists, by an ordinary transition; the superseding transition may cascade that creation (ADR-0019).
3. **The read surface follows supersession on request.** Resolving a superseded id returns the old object with its pointer; a caller may ask to follow to the live successor. External identifiers (a ticket key) are re-pointed to the successor by the consumer or by declared cascade.
4. References from other objects are re-pointed only where the superseding transition declares a cascaded action on the inverse relationship; otherwise they remain, pointing at the superseded object and its pointer.
5. A duplicate is **not** superseded: it closes with a `duplicate_of` reference and both objects remain live in history. Supersession is for "this object continues as that one".

## Alternatives rejected

### Change the type in place

Rejected: the object's events would span two machines with different states, and the printable rule set for either would not explain the history.

### Delete and recreate

Rejected: lineage is lost; ADR-0018 forbids reusing the id.

## Consequences

- Move-to-project and convert-subtask are the same mechanism. So is a **merge**, which supersedes the losing record (iteration 3) — and a *duplicate* is not a merge: it closes with a `duplicate_of` reference and both records stay live, which is the distinction the paragraph above draws.
- A split — one object becoming two — is not supersession; see `docs/design/edge-cases.md`.
- Availability queries exclude superseded objects, as they exclude deleted ones (ADR-0024).

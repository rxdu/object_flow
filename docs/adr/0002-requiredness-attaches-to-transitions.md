# ADR-0002: Requiredness attaches to transitions, not to attributes

- **Status:** Accepted
- **Date:** 2026-09-07

## Context

The initial model marked each attribute required or optional. That produces an immediate contradiction: a field needed to complete an object is not needed to create it. Marking it required makes the object uncreatable; marking it optional loses the check exactly where it mattered.

## Decision

Two distinct notions, separated:

- **Structural** requiredness — the object is nonsense without this attribute. A property of the attribute, and in practice just the guard set on the creation transition.
- **Gate** requiredness — this attribute must be present to perform a particular transition. A property of the transition.

Guards on transitions are the general mechanism; structural requiredness is a special case of it.

## Alternatives rejected

### Flat required/optional flag on the attribute

Rejected as unable to express the common case where a field is needed only at a particular point in an object's life.

### Requiredness per editing context (Jira's screen-scheme model)

Attach requiredness to the form or dialog through which editing happens.

Rejected because it ties the rule to *how* the caller is editing rather than to *what* they are trying to do, so the same field is required or not depending on the route taken. Attaching it to the transition makes it a property of the intent, which is what a caller — human or agent — is actually expressing.

## Consequences

- `validate(object)` has no answer. The meaningful call is `validate(object, intent)`.
- Unsatisfied gates *are* the blocked-action explanation; it is not a separately built feature.
- Storage must be permissive, so the database is not a safety net (see ADR-0001).
- Creation stops being a special case: it is the transition from nothing into the initial state, carrying its own guards.

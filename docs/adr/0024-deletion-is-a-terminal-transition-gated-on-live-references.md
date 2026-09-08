# ADR-0024: Deletion is a terminal transition gated on live references; parts cascade, references block

- **Status:** Accepted — taken in autonomous design iteration 1 (2026-09-07); pending author review
- **Date:** 2026-09-07
- **Amended by:** ADR-0056 §3 — a composition declares its delete cascade. ADR-0058 then requires every terminal transition of the whole to be covered, and ADR-0066 lets the cascade carry arguments.

## Context

DESIGN.md never said what deletion is. The first consumer has a soft-delete flag on every entity and a cascade registry beside its state machine whose pre-delete hooks call a `force_transition` that bypasses guards (`wr:docs/design/state-transition-system.md`, "Integration with Cascade System"). Its ADR-0003 found that soft-deleting one redundant customer row would have cancelled 102 deliveries, 43 services and 120 warranty contracts. Every legacy table's soft-deleted rows must also land somewhere on import (ADR-0015). Walkthrough §4 G; TODO.md challenge 6.

## Decision

1. A type that can be deleted declares a **terminal state** for it — `DELETED`, or a domain word such as `RETIRED` or `CANCELLED` — and an ordinary transition into it, with guards like any other.
2. That transition is guarded on **no live references**: `none(r in <referencing type> where r.<reference> == this and r.state.category != closed)`, using a state category (ADR-0026) rather than terminality, which has no expression form, for every declared inverse relationship. A Customer with live deliveries is blocked with remedy class `dependent`, naming them.
3. **Parts are cascaded** (ADR-0019): the whole's delete transition cascades the part's, because a part cannot outlive its whole (ADR-0003). **References are never cascaded** by deletion.
4. A deleted object stays readable in history and by id, is excluded from default reads and from availability queries, and admits no further transitions.
5. There is no `force_transition`. Administrative repair is a declared asserting transition (ADR-0040), which carries its own capability and reason guards rather than the type's.
6. Legacy rows with `is_deleted = true` import into the type's deleted state with provenance `asserted` (ADR-0015).

Physical erasure — removing attribute values from history for a legal reason — is a separate concern, not deletion, and is left to the CRM case study (iteration 3).

## Alternatives rejected

### Hard delete

Rejected: it contradicts the recorded property and destroys the references that history needs.

### Cascade through references

The first consumer's registry does this. Rejected: it is how one customer row deletes 265 records, and it is exactly the second unguarded path ADR-0001 forbids.

### No deletion

Rejected: retiring, cancelling and archiving are real; they are terminal transitions, which is what this ADR says deletion is.

## Consequences

- The model needs no deletion mechanism beyond terminal states, inverse relationships and collection guards.
- The import path's "soft-deleted rows" item is closed.
- Whether a type's terminal state is called `DELETED` or something domain-specific is the type author's choice; the runtime only needs to know it is terminal.
- TODO.md challenge 6 and the deletion item are closed.

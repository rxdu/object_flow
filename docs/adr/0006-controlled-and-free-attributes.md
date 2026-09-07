# ADR-0006: Attributes are classified controlled or free

- **Status:** Proposed — working assumption, not yet confirmed
- **Date:** 2026-09-07

## Context

Every decision so far governs state changes, but most edits to a real object are not state changes: someone records a root cause, corrects an address, adds a note. If transitions are the only mediated path, these writes are ungoverned, which contradicts ADR-0001.

## Decision (provisional)

Classify the attribute rather than the write:

- **controlled** — writable only via specific actions or transitions. Any attribute referenced by a guard must be controlled, or the guard is trivially bypassable.
- **free** — editable by any permitted actor. Recorded with provenance, but not gated.

## Alternatives rejected

### All writes free

Any permitted actor writes any attribute at any time. Rejected: reopens the unchecked path that ADR-0001 exists to close.

### All writes actioned

Every attribute write is a named operation with its own guards. Rejected as too strict in the general case — correcting a typo in a description should not require inventing an operation — though it remains correct for controlled attributes specifically.

### Attributes mutable only as transition arguments

Maximally consistent, and rejected for the same reason: it forces a state change for edits that are not state changes.

## Consequences

- The controlled/free boundary is partly self-enforcing: guard references pull attributes into the controlled set automatically.
- Free attributes still produce history and provenance; they simply do not require a bespoke action each.

## Open

Not explicitly confirmed in discussion. Revisit before it becomes load-bearing.

# ADR-0029: Named, scoped sequences mint business identifiers at creation

- **Status:** Accepted — taken in autonomous design iteration 2 (2026-09-07); pending author review. **Withdraws** the "no sequence primitive" consequence of ADR-0018.
- **Date:** 2026-09-07
- **Refined by:** ADR-0076 — the allocation is outside the creating transaction, on a second connection, which on SQLite opens a separate file; the gap of rule 4 is what that buys. ADR-0103 — a `format` may interpolate a string or enum attribute, pad the number, and omit a segment whose placeholder is absent; an import raises a sequence past the legacy system's last value.

## Context

ADR-0018 correctly separates the store's opaque id from business identifiers, and left minting the latter to the consumer. A ticket key `PROJ-123`, an order number, an invoice number, or the first consumer's asset serial is minted, not chosen, and is the one identifier every human sees. Minting it outside the store makes the consumer keep an atomic counter somewhere — a second store and a second write path for a value ObjectKeeper then treats as authoritative. Case study §3.

## Decision

1. The store provides **named sequences**: a declared name, an optional scope expression (`project.id`), a monotonically increasing integer per scope, taken atomically inside the creating transaction, never reused. *(D181 and ADR-0076 moved the allocation out of the creating transaction, onto a connection of its own that commits first, so a rolled-back creation leaves the gap rule 4 requires; atomicity and monotonicity stand.)*
2. A business-identifier attribute may declare `from sequence <name> [scoped by <expression>] format "<literal>{n}<literal>"`. The value is minted at creation, is unique by construction within its scope, and is otherwise an ordinary controlled attribute.
3. Formatting is declaration metadata, not expression-language arithmetic or string operations. ADR-0021's version-1 language is unchanged.
4. Sequences are **monotonic, not gapless**: a rolled-back creation leaves a gap.

ADR-0018's decision — the store-assigned opaque id — is untouched; only its consequence "ObjectKeeper offers no sequence primitive" is withdrawn.

## Alternatives rejected

### Consumer-side counter

Rejected: it is outside the sole write path and must be kept consistent with the store by the consumer.

### Arithmetic in outcomes (`counter := counter + 1` on a counter object)

Rejected here: a sequence is a store primitive with atomicity requirements, not an expression; whether outcomes get arithmetic is iteration 4's question and is independent.

### Gapless sequences

Rejected as the default: gapless requires serialising every creation in the scope. Recorded as an edge case.

## Consequences

- Ticket keys, order numbers and asset serials are declared, not coded.
- The first consumer's `serial_number_service` and its "serial at birth" timing rule become a declaration on the unit type.
- Author review: this ADR changes a consequence the author's own ADR-0018 stated; the decision there stands.

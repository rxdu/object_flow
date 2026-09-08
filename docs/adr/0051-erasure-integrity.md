# ADR-0051: Erasure integrity — personal values may not be copied into non-personal attributes, and the redaction marker is absence

- **Status:** Accepted — repair of D35, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refines:** ADR-0031

## Context

Two holes in erasure, both with legal consequence since ADR-0031 exists for a legal requirement.

Nothing forbade `display_name := contact.email` where the source is personal and the target is not. Erasure would redact the source on the object and in history and leave the copy in both, so the erasure would be incomplete and the fold of the log would no longer reproduce a consistent picture.

Second, ADR-0031 defines the redaction marker only at expression level, as reading like null. Its storage representation was undecided. ADR-0037 makes external-identifier uniqueness an automatic invariant, which ADR-0041 may compile to a constraint. If a personal attribute is also an external identifier, which an email used as a CRM key is exactly, erasing two records writes the marker twice, and a sentinel value would make a legally mandated and irreversible operation impossible.

## Decision

1. **Personal values are tainted, and the taint is checked at publish.** An outcome may not write an expression reading a personal attribute into a non-personal one. The check is syntactic over the declaration and rejects the type rather than discovering the leak at erasure time. Deliberate derivation is possible by declaring the target personal too.
2. **The redaction marker is absence, stored as SQL NULL**, not a sentinel. It therefore collides with no unique constraint, and it reads as unknown under the three-valued semantics of ADR-0047, which is what makes a guard over an erased value fail rather than pass.
3. **A uniqueness invariant over a personal attribute is partial**, ignoring absent values, and publishing rejects a personal attribute declared as a required external identifier, since erasure would have to violate the requirement.
4. **Erasure lists the attributes it redacted and the files it deleted, never their values**, as ADR-0031 already requires, and the event records the taint check's version so a later audit can tell which rules were in force.

## Alternatives rejected

- **Encrypt and discard the key.** Still a valid storage implementation of redaction, and rejected as the model-level answer for the reason ADR-0031 gave: the redaction must be visible as such in the record.
- **Allow copying and chase the copies at erasure time.** Rejected: it requires a runtime dataflow trace through arbitrary history, and a missed copy is a legal failure discovered by an auditor.
- **A sentinel marker.** Rejected: see the uniqueness collision.

## Consequences

- Type authors will occasionally have to mark a derived field personal that they did not think of as personal, which is the correct outcome.
- The publish report gains the taint check alongside the invariant and sweepability reports.
- D35 is resolved.

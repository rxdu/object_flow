# Edge cases: what the model does not cover, or covers with a caveat

Status: maintained by the design iterations; started 2026-09-07. Each entry says whether the case is **not covered**, **covered with caveat**, or **deferred** to a named place. The bar for listing is "a real system does this and a type author would look for it".

## From the inventory system (iteration 1)

- **One unit becomes two, or two become one.** Not covered. Supersession (ADR-0028) records one successor; a physical split or merge of serialised units is two creations or one creation plus two supersessions, with history staying on the originals.
- **Correcting the past.** Covered with caveat. History is immutable; a wrong delivery date is corrected by a recorded action that writes the corrected value with provenance `corrected` and a reason. The read surface shows the current value and the correction; nothing rewrites the earlier event.
- **The physical world diverges from the record** (a unit is stolen, or found in a state the machine cannot reach). Covered: the administrative override of ADR-0001 asserts the state with provenance `asserted`, an actor and a reason. It is a transition, recorded and gated on authority, not a database edit.
- **Quantity-tracked consumables** (spare parts counted, not serialised). Deferred to iteration 4; the first consumer serialises everything, so it has not needed this.

## From issue tracking (iteration 2)

- **Changing an object's type in place.** Not covered. A type has one machine; a history read under two machines is unreadable. Supersession (ADR-0028) creates the successor and links it.
- **Splitting an issue into two with shared history.** Not covered. Two creations that reference the source; the source's history stays on the source.
- **Gapless sequences.** Not covered. ADR-0029 sequences are monotonic and never reuse a value, but a rolled-back creation leaves a gap. A jurisdiction that requires gapless invoice numbers must assign the number in a later action, after the object exists, and accept that the assignment is serialised.
- **Re-deriving what was allowed under an older declaration version.** Covered with caveat. Each event records the declaration version in force (ADR-0027); the rule set for that version remains printable; no tool will replay a historical request against it.
- **Per-object read visibility** (issue-level security). Deferred to the read-surface design (TODO.md).

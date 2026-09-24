# ADR-0057: `changed_since` may name a part relationship, so an approval is invalidated by an edit to a part

- **Status:** Accepted — repair of D60, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refined by:** ADR-0058 — every terminal transition of a whole must dispose of its parts, and re-parenting is checked against both wholes. ADR-0082 — the part position is recorded per relationship, so `changed_since` names one part relationship precisely.
- **Refines:** ADR-0035

## Context

ADR-0035 exists so that a material edit after an approval invalidates it automatically, without every editing action having to remember a reset. The mechanism is `changed_since([attributes], approval.event)`, reading the attributes of `this`.

Writing a real model exposed the gap. A purchase order's approval should be invalidated by a change to any of its **lines**, and lines are composition parts with their own events. `changed_since` names attributes of `this` only, and ADR-0056 removed the runtime-maintained inverse that used to stamp the whole when a part changed. So editing a line left the whole untouched and its approvals standing.

The workaround a reviewer had to invent was three moving parts: an extra timestamp attribute on the whole, an action on the whole that touches it, and a cascade from every line-editing transition. That is the duplication ADR-0009 and ADR-0035 both exist to remove, reappearing in the design's most-cited guard.

## Decision

**`changed_since` accepts a part relationship name alongside attribute names.** `changed_since([amount, vendor, lines], a.at_event)` is true if any of those attributes of `this` changed after the event, **or if any object in the `lines` composition was created, changed or removed after it.**

It reaches parts and not references, because a part's lifetime is bounded by its whole and a part is conceptually a piece of it, whereas a referenced object is a separate thing whose changes are its own business. A guard that must react to a referenced object's change reads that object's state directly.

The store answers it from the per-attribute last-written index (ADR-0048) extended to carry, on each whole, the position of the last event on any of its parts. *(Replaced by ADR-0082 §7, repairing D202: one position per part relationship, a row of the same index keyed by the relationship's name, so `changed_since` naming `lines` is not invalidated by a change to another part relationship, such as the approval's own creation; `DESIGN.md` §5.3.)* That is one more maintained position, updated in the transaction that writes the part, and it is not a second write path: it is an index over the log, like the per-attribute index beside it.

## Alternatives rejected

### Leave it to the type author

The three-part workaround above. Rejected: it is per-editing-transition duplication that drifts, which is the defect class ADR-0009 was written about, and it is silent when forgotten.

### Restore a stamp on the whole when a part changes

This is what ADR-0056 removed, and for good reason: it was a write to another object with an event carrying no transition name. An index is not a write.

### Let `changed_since` reach any relationship

Rejected: across a reference it would make an approval on one object depend on the edit history of another the author may not control, and the affected set becomes unbounded. Composition is the boundary because it already means "part of this".

## Consequences

- The declaration syntax's `changed_since` accepts part names, and publishing rejects a name that is neither an attribute nor a part.
- The storage schema carries a last-part-event position per object that has parts. *(Replaced by ADR-0082 §7: the schema keeps the position per part relationship in `of_attribute_write`, and `last_part_event` is retired; `storage-schema.md`.)*
- DESIGN.md §5.7's `changed_since` row is amended.
- D60 is resolved.

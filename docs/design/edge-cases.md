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

## From customer records (iteration 3)

- **Unmerge.** Not built in. A merge is a supersession plus re-pointed links, each recorded with its previous target; a consumer can reverse it from history by creating a new record and re-pointing back, but the original id does not return to life, because a superseding state is terminal.
- **Acyclicity of a self-referential hierarchy** (a company cannot be its own ancestor). Not covered in version 1: the expression language has no transitive closure. A depth-bounded guard (`parent != this and parent.parent != this …`) is a workaround, not a guarantee.
- **Erasing a value another object's guard or derived attribute depends on.** Covered with caveat. After erasure the value reads as the redaction marker, which compares as null; a guard that needed it will fail with its usual remedy class. Erasure never silently satisfies a guard.
- **Erasure and content-addressed files.** Covered with caveat. The blob is deleted; the reference's content hash remains in the event skeleton. A hash of a personal document is not the document, but a deployment that treats it as personal must declare the whole `file` attribute `personal` so the reference is redacted too.
- **Moving a deal between pipelines yields a new object id.** Covered with caveat. Supersession creates a new object; links that consumers hold to the old id resolve through the pointer, but any external system that stored the id must follow it.

## From orders at volume (iteration 4)

- **Hot rows.** Covered with caveat. Every reservation on one product takes that product's row lock (ADR-0023), which is what prevents overselling and also what serialises a flash sale. Throughput on a single SKU is bounded by lock hold time. Sharding stock into several bucket objects, each with its own quantity, is a modelling choice the consumer makes; the store does not do it.
- **Multi-warehouse or multi-region stock.** Not covered as one store. ObjectKeeper is one database; stock split across regions that must not share a transaction is several stores, and reconciling them is a consumer concern outside the mediated guarantee.
- **Very large cascades.** Covered with caveat. A placement that creates thousands of lines and reserves thousands of products is one transaction and one lock set; it will succeed, slowly, and hold locks meanwhile. A declaration may state a maximum fan-out for a cascaded relationship; a request exceeding it is refused with `self-serviceable`, meaning split the request.
- **Gapless order numbers.** Not covered; see iteration 2. A rolled-back placement leaves a gap in the sequence.
- **Analytics over the whole log.** Out of scope. The read surface answers questions about objects; questions about millions of events belong in a warehouse fed from the log by a subscriber.
- **Erasure across archived events.** Covered with caveat. ADR-0033's archival tiering must keep archived events reachable by ADR-0031 erasure, or archive only events that carry no personal attributes.

# Edge cases: what the model does not cover, or covers with a caveat

Status: maintained by the design iterations; started 2026-09-07. Each entry says whether the case is **not covered**, **covered with caveat**, or **deferred** to a named place. The bar for listing is "a real system does this and a type author would look for it".

## General

- **Atomicity across ObjectKeeper and an external system.** Not covered. A transition and a call to Xero, a payment gateway or a mail server cannot commit together. The design's answer is the mirror-plus-idempotency pattern: the external system is recorded as a mirror object, the consumer performs the external action on observing the event and reports back with the external event id as idempotency key (ADR-0008, ADR-0014). The window between the two is real and visible in the record.
- **The clock.** Covered with caveat. `now` in a guard is the database's transaction time, not the caller's clock; a guard such as `end_date <= now` is evaluated at commit. A caller that reasons about time from its own clock can be refused by a few seconds either way.
- **Several ObjectKeeper stores.** Not covered as one system. Each store guarantees its own data; references across stores are external identifiers, and no invariant spans them.
- **Very long histories on one object.** Covered with caveat. `changed_since` and `history` read an object's events; an object with millions of events (a counter, a queue) is a modelling smell — the events belong on the things being counted.

## From the inventory system (iteration 1)

- **One unit becomes two, or two become one.** *Now covered by ADR-0046:* an outcome may create the new object, bind it to a name and re-parent parts to it. History stays on the originals, which is what a split means.
- **Correcting the past.** Covered with caveat. History is immutable; a wrong delivery date is corrected by a recorded action that writes the corrected value with provenance `corrected` and a reason. The read surface shows the current value and the correction; nothing rewrites the earlier event.
- **The physical world diverges from the record** (a unit is stolen, or found in a state the machine cannot reach). Covered: the administrative override of ADR-0001 asserts the state with provenance `asserted`, an actor and a reason. It is a transition, recorded and gated on authority, not a database edit.
- **Quantity-tracked consumables** (spare parts counted, not serialised). *Now covered by ADR-0050:* a type declares its tracking mode, and a slot declares which fill form it takes, so one delivery may carry a serialised robot and a counted quantity of cable ties.

## From issue tracking (iteration 2)

- **Changing an object's type in place.** Not covered. A type has one machine; a history read under two machines is unreadable. Supersession (ADR-0028) creates the successor and links it.
- **Splitting an issue into two with shared history.** *Now covered by ADR-0046* for the objects; history is still not shared, and each successor references the source.
- **Gapless sequences.** Not covered. ADR-0029 sequences are monotonic and never reuse a value, but a rolled-back creation leaves a gap. A jurisdiction that requires gapless invoice numbers must assign the number in a later action, after the object exists, and accept that the assignment is serialised.
- **Re-deriving what was allowed under an older declaration version.** Covered with caveat. Each event records the declaration version in force (ADR-0027); the rule set for that version remains printable; no tool will replay a historical request against it.
- **Per-object read visibility** (issue-level security). Deferred to the read-surface design (TODO.md).

## From the expression language

- **Dynamic attribute writes.** Not covered, by decision (ADR-0046). A transition writes the attributes it names, so a merge declares the fields it resolves. An attribute set chosen at request time could not be checked at publish or printed in a rule set.
- **Grouped aggregation.** Not covered, by decision (ADR-0047). Group-by belongs to the read surface; a guard that appears to need it usually wants a relationship aggregate declared on the other end.

## From customer records (iteration 3)

- **Unmerge.** Not built in. A merge is a supersession plus re-pointed links, each recorded with its previous target; a consumer can reverse it from history by creating a new record and re-pointing back, but the original id does not return to life, because a superseding state is terminal.
- **Acyclicity of a self-referential hierarchy** (a company cannot be its own ancestor). Not covered in version 1: the expression language has no transitive closure. A depth-bounded guard (`parent != this and parent.parent != this …`) is a workaround, not a guarantee.
- **Erasing a value another object's guard or derived attribute depends on.** Covered. Under the three-valued semantics of ADR-0047 the redacted value reads as absent, any comparison with it is unknown, and a guard evaluating to unknown fails. This holds for exclusionary guards too, so erasing a requester does not grant them self-approval, which an earlier two-valued reading would have.
- **Erasure and content-addressed files.** Covered with caveat. The blob is deleted; the reference's content hash remains in the event skeleton. A hash of a personal document is not the document, but a deployment that treats it as personal must declare the whole `file` attribute `personal` so the reference is redacted too.
- **Moving a deal between pipelines yields a new object id.** Covered with caveat. Supersession creates a new object; links that consumers hold to the old id resolve through the pointer, but any external system that stored the id must follow it.

## From orders at volume (iteration 4)

- **Hot rows.** Covered with caveat. Every reservation on one product takes that product's row lock (ADR-0023), which is what prevents overselling and also what serialises a flash sale. Throughput on a single SKU is bounded by lock hold time. Sharding stock into several bucket objects, each with its own quantity, is a modelling choice the consumer makes; the store does not do it.
- **Multi-warehouse or multi-region stock.** Not covered as one store. ObjectKeeper is one database; stock split across regions that must not share a transaction is several stores, and reconciling them is a consumer concern outside the mediated guarantee.
- **Very large cascades.** Covered with caveat. A placement that creates thousands of lines and reserves thousands of products is one transaction and one lock set; it will succeed, slowly, and hold locks meanwhile. A declaration may state a maximum fan-out for a cascaded relationship; a request exceeding it is refused with `self-serviceable`, meaning split the request.
- **Gapless order numbers.** Not covered; see iteration 2. A rolled-back placement leaves a gap in the sequence.
- **Analytics over the whole log.** Out of scope. The read surface answers questions about objects; questions about millions of events belong in a warehouse fed from the log by a subscriber.
- **Erasure across archived events.** Covered with caveat. ADR-0033's archival tiering must keep archived events reachable by ADR-0031 erasure, or archive only events that carry no personal attributes.

## From approvals and bookings (iteration 5)

- **Approving several objects in one decision.** Not built in. A proposal targets one request; a batch approval is several approvals, or a parent object whose approval the children's guards read through the relationship.
- **An approver who later loses authority.** Covered with caveat. An approval is a recorded fact; it stays valid unless the type declares that it is invalidated by content change (`changed_since`) or by age. Re-checking the approver's current capabilities is not built in; a type that needs it records the approver's capability at approval and a scheduler re-validates.
- **Four-eyes across different objects** (the same person may not approve both the request and its invoice). Covered only where the two objects are related by a declared relationship the guard can traverse; otherwise it is the consumer's rule.
- **Amending a proposal before approving.** Not covered. A proposal's inputs are fixed at submission; the approver rejects it, or executes the transition directly with different inputs, and the record shows both.
- **Editing a recurring series after instances exist.** Consumer logic: instances are objects the consumer created; which to change is a selection.
- **Waitlist promotion.** Consumer logic: "the first in the queue" is a selection with an ordering the language does not express.
- **Business hours and local time.** Not covered. Timestamps are absolute; calendar arithmetic is a consumer input.
- **Substitution bookings** ("any robot of this model"). Consumer logic: choosing the unit is a selection; once chosen, the booking is ordinary.

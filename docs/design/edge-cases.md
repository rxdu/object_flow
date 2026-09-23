# Edge cases: what the model does not cover, or covers with a caveat

Status: maintained by the design iterations; started 2026-09-07. Each entry says whether the case is **not covered**, **covered with caveat**, or **deferred** to a named place. The bar for listing is "a real system does this and a type author would look for it".

## General

- **Atomicity across ObjectKeeper and an external system.** Not covered. A transition and a call to Xero, a payment gateway or a mail server cannot commit together. The design's answer is an externally owned type plus idempotency: the external system's record is an ordinary type here with an external identifier, the consumer performs the external action on observing the event and reports back with the external event id as idempotency key (ADR-0008, ADR-0014, ADR-0080). The window between the two is real and visible in the record.
- **The clock.** Covered with caveat. `now` in a guard is the database's transaction time, not the caller's clock; a guard such as `end_date <= now` is evaluated at commit. A caller that reasons about time from its own clock can be refused by a few seconds either way.
- **Several ObjectKeeper stores.** Not covered as one system. Each store guarantees its own data; references across stores are external identifiers, and no invariant spans them.
- **Very long histories on one object.** Covered with caveat. `history` reads an object's events, and since ADR-0042 every edit is a transition, an ordinary record accrues events at edit rate rather than at lifecycle rate. That is the honest cost of the recorded property. `changed_since` is unaffected, being a constant-time comparison against a per-attribute index (ADR-0048). An object with millions of events, such as a counter or a queue, is still a modelling smell.

## From the inventory system (iteration 1)

- **One unit becomes two, or two become one.** *Now covered by ADR-0046:* an outcome may create the new object, bind it to a name and re-parent parts to it. History stays on the originals, which is what a split means.
- **Correcting the past.** Covered with caveat. History is immutable; a wrong delivery date is corrected by a recorded action that writes the corrected value with provenance `corrected` and a reason. The read surface shows the current value and the correction; nothing rewrites the earlier event.
- **The physical world diverges from the record** (a unit is stolen, or found in a state the machine cannot reach). Covered: a declared asserting transition sets the state with provenance `asserted`, an actor and a reason (ADR-0040). It is a transition, recorded and gated on a capability, not a database edit.
- **Quantity-tracked consumables** (spare parts counted, not serialised). *Now covered by ADR-0050:* a type declares its tracking mode, and a slot declares which fill form it takes, so one delivery may carry a serialised robot and a counted quantity of cable ties.

## From issue tracking (iteration 2)

- **Changing an object's type in place.** Not covered. A type has one machine; a history read under two machines is unreadable. Supersession (ADR-0028) creates the successor and links it.
- **Splitting an issue into two with shared history.** *Now covered by ADR-0046* for the objects; history is still not shared, and each successor references the source.
- **Gapless sequences.** Not covered. ADR-0029 sequences are monotonic and never reuse a value, but a rolled-back creation leaves a gap. A jurisdiction that requires gapless invoice numbers must assign the number in a later action, after the object exists, and accept that the assignment is serialised.
- **Re-deriving what was allowed under an older declaration version.** Covered with caveat. Each event records the declaration version in force (ADR-0027); the rule set for that version remains printable; no tool will replay a historical request against it.
- **Per-object read visibility** (issue-level security). *Now covered by ADR-0030:* a declared predicate every read applies, where failing it means not found.

## From the expression language

- **Dynamic attribute writes.** Not covered, by decision (ADR-0046). A transition writes the attributes it names, so a merge declares the fields it resolves. An attribute set chosen at request time could not be checked at publish or printed in a rule set.
- **Grouped aggregation.** Not covered, by decision (ADR-0047). Group-by belongs to the read surface; a guard that appears to need it usually wants a relationship aggregate declared on the other end.

## From customer records (iteration 3)

- **Unmerge.** Not built in. A merge is a supersession plus re-pointed links, each recorded with its previous target; a consumer can reverse it from history by creating a new record and re-pointing back, but the original id does not return to life, because a superseding state is terminal.
- **Acyclicity of a self-referential hierarchy** (a company cannot be its own ancestor). Not covered in version 1: the expression language has no transitive closure. A depth-bounded guard (`parent != this and parent.parent != this …`) is a workaround, not a guarantee.
- **Erasing a value another object's guard or derived attribute depends on.** Covered, with a caveat since ADR-0060: an erasure also carries an automatic admission for every invariant reading what it erased, so an object can stand in recorded violation until it holds again. Under the three-valued semantics of ADR-0047 the redacted value reads as absent, any comparison with it is unknown, and a guard evaluating to unknown fails. This holds for exclusionary guards too, so erasing a requester does not grant them self-approval, which an earlier two-valued reading would have.
- **Erasure and content-addressed files.** Covered with caveat. The blob is deleted; the reference's content hash remains in the event skeleton. A hash of a personal document is not the document, but a deployment that treats it as personal must declare the whole `file` attribute `personal` so the reference is redacted too.
- **Moving a deal between pipelines yields a new object id.** Covered with caveat. Supersession creates a new object; links that consumers hold to the old id resolve through the pointer, but any external system that stored the id must follow it.

## From orders at volume (iteration 4)

- **Hot rows.** Covered with caveat. Overselling is prevented by serialisable isolation (ADR-0039), not by the row lock. *Corrected by ADR-0090:* this entry used to say the lock makes contention block rather than abort; probed on PostgreSQL 16.8, a contended request at serialisable isolation waits for the lock and then fails with a serialisation error, so it pays the wait **and** the retry. The lock only makes the loser fail sooner. Throughput on a single SKU is therefore bounded by the serialisation-failure rate: a contended product retries to a declared bound and then refuses with `stale`, each event records its retries, and a rising exhaustion rate is the signal that a declaration has a contention problem. Sharding stock into several bucket objects, each with its own quantity, is a modelling choice the consumer makes; the store does not do it.
- **Multi-warehouse or multi-region stock.** Not covered as one store. ObjectKeeper is one database; stock split across regions that must not share a transaction is several stores, and reconciling them is a consumer concern outside the mediated guarantee.
- **Very large cascades.** Covered with caveat. A placement that creates thousands of lines and reserves thousands of products is one transaction and one lock set; it will succeed, slowly, and hold locks meanwhile. Every loop and every cascade clause must state a bound (ADR-0071); a request exceeding one is refused with the `over-limit` verdict naming that loop (ADR-0041).
- **Gapless order numbers.** Not covered; see iteration 2. A rolled-back placement leaves a gap in the sequence.
- **Analytics over the whole log.** *Reversed by ADR-0081, 2026-09-23.* Declared metrics are in scope and computed on read (ADR-0084), with standard ones for every type. What stays out is open-ended exploration, which the engine feeds by export rather than performing. This entry used to say that questions about millions of events belong in a warehouse fed by a subscriber.
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

## Refused at publish, or refused by decision

The repair added several checks that reject a declaration rather than accept one that cannot be enforced. A type author meeting one of these is not looking at a bug.

- **An invariant traversing a one-way relationship** is rejected; declare the inverse (ADR-0045).
- **A non-symmetric type-scan invariant** is rejected. "At most three live bookings across the type" is not symmetric and must be restated against a relationship (ADR-0052).
- **A declaration whose derived attributes form a cycle** is rejected (ADR-0047).
- **A guard naming an undeclared input**, and a bare `null` in a comparison, are rejected (ADR-0047, ADR-0053).
- **A personal value written into a non-personal attribute** is rejected, because erasure could not reach the copy (ADR-0051).
- **Any required attribute marked `personal`** is rejected, external identifier or not, because erasure writes absence and would have to violate the requirement (ADR-0051, check 9).
- **A bare `create <Type>` where the type has several creation transitions** is rejected (ADR-0052).
- **A non-sweepable transition** is refused by `available` rather than scanned, so a time-driven transition whose guards cannot be prefiltered has nothing to find its objects (ADR-0048). This is the sharpest of these: an automation can be declared and then have no way to run.
- **A transition gated on an external evaluator can never be swept**, since `available` calls no evaluator (ADR-0048, ADR-0049).
- **Applying a transition twice to one object in one request** is not special-cased: the second evaluation sees the first's result and usually fails, so two delivery slots bound to one unit block completion rather than selling it twice (ADR-0038).
- **A pending proposal the current declaration cannot express** is invalidated at publish rather than left pending (ADR-0044).
- **Changing a type's tracking mode** is a new type; objects move by supersession one at a time (ADR-0050).
- **A many-to-many `ref` pair** is rejected — neither end can store it, so declare a type for the association (check 41).
- **An invariant that reads `now`** is rejected, since no after-write check can enforce it; "no booking in the past" is a guard, not an invariant (check 32).
- **A type-scan guard, a traversal or type-scan invariant, or a `visible when` predicate reading an unindexed attribute** is rejected (check 7).
- **An actor guard on an `only via` transition**, and `only via` with `proposable`, are rejected (check 14).
- **`if` in a guard** is rejected, since a conditional would hide which clause failed; it is allowed in a derived attribute and in an outcome value (check 21).
- **An `act` declared at a terminal state** is rejected (check 40).
- **`clear` on a required attribute, a counter or a relationship end** is rejected (check 17, ADR-0073).

## From the author's rulings on the open questions (2026-09-08)

- **A multi-currency balance in one attribute.** Not covered, by decision. `money(ccy)` fixes its currency at declaration so a mismatch is a publish error; a model holding several currencies declares an account per currency, which is what double-entry systems do anyway (ADR-0068).
- **An external system that assigns a value.** Covered, by a different mechanism than the one people reach for. An evaluator returns a verdict and never a value; a system that decides — a randomisation service, a pricing service, Xero minting a contact id — is an externally owned type with an external identifier — an ordinary type whose sync-driven transitions the sync requests (ADR-0069, ADR-0080).
- **Personal data inside another object's free text.** Not covered. Erasure reaches the object's attributes, its events, its legacy entries and the proposals that carry its values (ADR-0078); a customer's phone number typed into a service note is a value the declaration does not know is personal. The import mapping should not carry free text it cannot classify into a type that will outlive the person.
- **Confidentiality of one field within a visible row.** Not covered. Visibility is per object. Hiding one attribute from one role costs a separate type with its own lifecycle, its own visibility predicate and a cascade clause per terminal transition of the whole (ADR-0072).
- **Authority attached to undoing a commitment.** Not distinguished. The first consumer gates the reversal of a decision that has become real — cancelling a placed order, reverting a committed intake — and treats every correction before that point as ordinary. Nothing in the model tells a transition that undoes a commitment from any other transition, and a capability guard is what expresses it today (ADR-0070).
- **A consumer that maintains a counter.** *Corrected 2026-09-24:* this entry said there was none in the record and that the first consumer stores no stock level. It keeps one for bulk accessories, counted down when a service consumes them and clamped at zero (`wr:app/services/service_service.py:751-773`). The design would model a bulk lot as a `tracking quantity` type with a counter and a `nonneg` invariant, declared by the table-to-type mapping, so the first consumer would exercise the counter, and production's clamp would become a refusal naming `nonneg` (D279, ADR-0103). Whether a counter should hold money stays deferred (ADR-0072).

## From a payments ledger (written after the model)

- **A posting whose legs each carry their own account, amount and sign, supplied by the request.** An input may be a set of references, so a selection can be passed; a set of anonymous structures cannot, since every input is typed. Each posting shape is therefore its own creation.
- **Exact allocation of an amount into parts that must sum back.** Not expressible, deliberately. Who absorbs the remainder is a decision rather than a calculation, and ADR-0007 puts it with the consumer.
- **A compensating entry as a correction.** Not what `corrects` is. `corrects` rewrites an attribute of one object in place with provenance; a ledger forbids that, and its compensating entry is an ordinary new posting linked to the original by a reference.

## Evidence from the first consumer, counted 2026-09-09

Not edge cases. Measurements taken to settle open questions, kept because they are what the design's claims rest on.

- **How far a guard reaches.** 454 guard sites; 238 of them one permission predicate repeated. Of the 205 that read data: 94 local, 27 one hop, 31 aggregating over an owned collection, 30 type-wide existence, 23 two hops of which 21 aggregate. **Nothing reaches three hops.** This is the measurement behind DESIGN.md §5.7's boundary, and it corrects a five-predicate sample that had suggested guards were almost entirely relational.
- **How much a rule is repeated.** 176 of 454 sites are redundant, and 51 distinct rules appear at more than one site. 23 of the duplications cross a layer — a schema check repeated in a service, a service check repeated as a database constraint. Every one of the 98 service-level permission checks duplicates one already enforced on its route.
- **A uniqueness rule with no backstop.** "One warranty contract per robot" was a partial unique index until a migration dropped it inside an autogenerated block and nothing recreated it. It is now enforced only by a check-then-act in a service method, which two concurrent requests defeat. This is the failure ADR-0039 exists to prevent, in production, found by counting rather than by an incident.
- **Two dead gates.** One transition mode is declared and used by none of the 74 transition rows. One condition function read a relationship that does not exist and returned true unconditionally from the day it was written, so a checklist gate never gated anything. Both are the kind of thing a declaration that is data, and checked at publish, does not permit.

## From auditing production against the design (2026-09-24)

Found by reading the first consumer's production code rule by rule ([`first-consumer-audit.md`](first-consumer-audit.md)); what the design changed for is in ADR-0103.

- **Text that must match a pattern** — a Jira key, a link's scheme — **or be trimmed**. Not covered, by decision (ADR-0103). A pattern's meaning differs between Python and each backend's SQL, so it would make a rule's meaning depend on the backend. The consumer's schema checks the format and turns blank input into absence at its edge, or an evaluator asks the system the text refers to. `length` bounds text in the store.
- **A share of time per month.** Covered over a whole history, not per period. A metric buckets a span by when it began, so a unit that joined the lending pool in January puts all its pool time in January, and a per-month utilisation divides the wrong numbers. Apportioning a span across the buckets it crosses would need intervals split at bucket boundaries, which the metric form does not do (ADR-0103).
- **Who may see who acted.** Covered as the first consumer does it: a reader who may see a record sees who acted on it, and the cross-record trail its administrators read — every entity's activity, one user's activity — is the export and a subscription's `pull`, served from routes the consumer authorises. A rule withholding who acted from ordinary readers was considered and withdrawn: it was stricter than production and left the assignment metrics incomplete for them (ADR-0103, D285).
- **A list filtered on a status derived from other objects**, such as an order's status from its units. Not covered as a query filter: a derivation that reads other objects cannot be indexed (check 46), and a stored copy written as a side effect of other objects' transitions would be a second write path. At the first consumer's size the reader filters the page; a status that needs to be a filter is modelled as the object's own state.
- **Reacting to a derived status changing**, such as a Jira comment when an order's status moves. Covered with caveat: a derived value emits no event, so a subscriber to the transitions that change it reads the derived value after each and acts on a change, which is how UC-12's order-ready notice works.
- **Requesting the state an object is already in.** Production answers it as a no-op success; the store refuses it, since a transition is requested by name from a state. A retry is safe through the idempotency key, so callers that relied on the no-op change at cutover.
- **Accepting a reference while the system that checks it is down**, as production accepts a Jira key during an outage. Covered with caveat: a deferred evaluator must answer satisfied during the outage, and the record cannot tell that answer from a real check.
- **A keyed map as one attribute**, such as a warranty's allowance per component category. Not covered, by decision: a part per key expresses it, and gives each allowance its own history.
- **Notes attached to eleven kinds of record.** Open in [`first-consumer-cutover.md`](first-consumer-cutover.md): a record part owned by an abstract base the eleven extend, or a label, which cannot be pinned.

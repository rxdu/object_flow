# ADR-0080: `mirror` is the cutover marking; a type another system owns for good is an ordinary type the sync writes

- **Status:** Accepted — decided 2026-09-09 at the author's direction ("rule on D194"), with the recommendation and its evidence below; pending author review
- **Date:** 2026-09-09
- **Refines:** ADR-0069, ADR-0075
- **Refined by:** ADR-0110 — where a deployment's user system owns its roles, `User` is externally owned this way and kept in step by a sync.

## Context

Two decisions used one word for two write paths. ADR-0069 made an external system that assigns a value "a mirror: an object here with an external identifier, written by the consumer on observing the event" — an ordinary type, written through its declared transitions by a sync acting as an actor. ADR-0075 made `mirror` a type marking whose objects have no transitions and are written only by the import path, so that nothing in this store can write a type the legacy application still owns during a staged cutover. The first consumer's customer is both at once — legacy-owned until its stage, Xero-owned by design intent for good — and D194 asked which kind it is.

The first consumer's own Xero design answers (`wr:docs/adr/0003-xero-as-source-of-truth-for-customer-identity.md`). Xero owns existence, legal name and `ContactID`; **`is_active` stays ours**, and a separate `xero_archived_at` records Xero's archive state, which **must never cascade**; the sync **follows `MergedToContactID` and rewrites our column**; and before any duplicate is collapsed a merge operation must re-point the children and only then retire the row. So the Xero-owned customer has local state, a local terminal transition, and a merge that is supersession (ADR-0028). A type with no transitions can hold none of that.

## Decisions

### 1. `mirror` names one thing: the cutover marking

A type the legacy system still owns, held here with attributes, states and an external identifier, no transitions, written only by the import path, and removed by a version advance that declares the real transitions and changes no object's id (ADR-0075). It is temporary by construction.

### 2. A type another system owns for good is an externally owned type, which is an ordinary type

It declares an `external` identifier for the owning system, sync-driven transitions guarded on the sync's capability — `act sync_from_xero at any { require may: actor.has(XERO_SYNC) … }` — and whatever local transitions the domain needs. The sync consumer requests those transitions like any actor, with the external system's own change identifier as idempotency key (ADR-0014). Every change has a transition name, a recorded actor and an event a subscription can filter on, and a merge upstream is a `merged_into` supersession here. This is what ADR-0069 meant; the word `mirror` is withdrawn from that use.

### 3. The first consumer's customer is a mirror while legacy owns it, and cuts over to the Xero-synced type

Its stage in `first-consumer-cutover.md` stands. What the version advance at that stage installs is not an owned type but the externally owned type of decision 2, whose sync is Xero's; "mirrored longest" there refers to the marking. The mechanism of ADR-0075 §3 is the same: remove the marking, declare the transitions, keep every id.

### 4. Erasure applies to both, and the owning system erases its own copy

An externally owned type declares its own `erase` like any type. The syntax document's note that a personal attribute on a mirror must be erased in both places holds for both kinds.

## Alternatives rejected

- **One kind: a permanently external type is `mirror`, and repeated import is its sync.** Rejected: every Xero change would be an `asserted` event with no transition name, every customer would sit in `exceptions(type)` for ever, no subscription could filter the feed, and a Xero merge — a supersession — could not be performed at all. The mechanism ADR-0075 built to keep a type unwritable is the wrong tool for a type that must be written on every poll.
- **A `mirror` with a declared sync path.** Rejected as a third write path beside transitions and import, for a case an ordinary type already covers.
- **Keep both senses and add a note.** Rejected: one word for two write paths is the defect, not a wording problem.

## Consequences

- `DESIGN.md` §5.5 says "externally owned type", §11 says the marking is for cutover only and what the customer cuts over to, and §14 defines both terms.
- ADR-0069 is annotated in place; ADR-0075 says the marking is for the duration of a cutover.
- `declaration-syntax.md` §2, `edge-cases.md`, the walkthrough, the orders case study and the cutover document use the two terms as defined here.
- D194 is resolved.

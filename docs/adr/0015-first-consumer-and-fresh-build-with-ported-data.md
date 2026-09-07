# ADR-0015: The first consumer is the Weston Robot operations platform, rebuilt on ObjectKeeper with its production data ported

- **Status:** Accepted
- **Date:** 2026-09-07

## Context

The design documents used Deployment, Site, Robot, Task, UAT and Deal as illustrative names and said so. No document named the actual first consumer, so a reader of this repository could not tell which domain the design was being judged against, and the domain questions in TODO.md were listed as unanswered while several of them had answers in a sibling repository.

The Weston Robot inventory management system (`~/RduWs/wr_inventory_management`; FastAPI, SQLAlchemy, Vue 3, PostgreSQL) is in production. It tracks serialised robots, accessories and spare parts through deliveries, services, warranties, product configurations and staged intake. A proposal in that repository (`docs/proposals/operations-system-design.md`) repositions it as an operations platform: order commitment with available-to-promise, two-strength allocation, procurement with pegging and receiving reconciliation, intake as a unit status, pre-delivery inspection, engagements and leasing (its ADR-0002), and customer identity owned by Xero (its ADR-0003). Parts of that proposal have shipped; leasing, inspection and most of the Jira model have not.

The existing system already contains a hand-built version of ObjectKeeper's concerns:

| Concern | Where it lives today | Size |
|---|---|---|
| State machines, guards, side effects | `app/core/state_registry.py`, `app/core/state_transition.py` | 1421 + 787 lines |
| Cascading soft delete | `app/core/cascade_registry.py`, `app/core/soft_delete_cascade.py` | 359 + 420 lines |
| Actors and authority | `app/core/permissions.py`; API keys for agents; effective permissions = role ∪ key allowlist | 4 roles, 41 permissions |
| Idempotency | `app/models/idempotency_key.py`; `Idempotency-Key` required on every agent POST | — |
| Concurrency | `version` column on every entity; row locks on reservation | — |
| History | unified audit log with before/after values, principal, source | — |

It has also met the failure modes this design's ADRs predict. Its ADR-0002 records three inventory transitions that were declared in the registry, tested at the registry level, and reachable from no API route, so a terminal state existed for months only as a badge and a counter fixed at zero. Guards are Python callables that can return only a string reason. Side effects assign status on other objects directly. A `force_transition` bypass exists for cascades. Its ADR-0003 found that soft-deleting one redundant customer row would have cancelled 102 deliveries, 43 services and 120 warranty contracts through the cascade.

Line counts and permission counts were observed on 2026-09-07. The delivery, service and warranty counts are from the inventory repository's ADR-0003, measured 2026-09-01.

## Decision

1. **The first consumer is the extended Weston Robot inventory system** — the operations platform described in that repository's proposal. Its repository is the source of domain evidence for this design, and its object types (Robot, Accessory, SparePart, Delivery, Service, WarrantyContract, ProductConfiguration, IntakeBatch, ProcurementOrder, ShippingRecord, and the planned Engagement and Lease) are the ones the model must carry.
2. **It is a fresh build on ObjectKeeper, not an in-place extension.** ObjectKeeper does not need to coexist with the existing SQLAlchemy models, Alembic migrations or live schema.
3. **The production data is ported and preserved.** Every object, its history and its external identifiers survive the move. Import is therefore a designed path in ObjectKeeper, not a one-off script beside it.
4. **The threat model is mistakes, not malice.** The purpose, in the author's words: data only moves to a different state when the right trigger is emitted and every required condition is met.

## Alternatives rejected

### Extend the existing system in place, adopting ObjectKeeper underneath it

Keeps the production schema and the additive-only migration discipline the existing repository enforces.

Rejected because ObjectKeeper's mediated property cannot hold while another application writes the same tables. In-place adoption would mean ObjectKeeper owning some tables and SQLAlchemy others, with cross-object guards spanning the boundary, and the guarantee would be exactly as strong as the weaker side. It would also carry the existing schema's compromises — permissive columns holding placeholder values, status assigned by side effects — into the starting point of the new model.

### Keep growing the hand-built registry

The existing registry is centralised, tested and in production, and the operations proposal's single-path rules already require every new status to register there.

Rejected because the registry has already failed in the ways this design exists to prevent, and each failure is structural rather than a bug: a declaration that is not the API can be declared and unreachable; a guard that is a callable cannot explain itself; a side effect that assigns status is an unmediated write; a force bypass is a second path. Adding procurement, allocation, inspection and leasing multiplies the objects and the cross-object rules without fixing any of that.

### Treat the strawman domain as the first consumer

Rejected because it was never real; see Context.

## Consequences

**The design is judged against this domain.** Several open domain questions in TODO.md now have evidence, recorded there with file references into the inventory repository. Several accepted ADRs are challenged by the same evidence; the challenges are recorded in TODO.md and noted on the affected ADRs, and none has been resolved yet.

**Import is a first-class path.** Every ported object arrives mid-lifecycle — units in `SOLD`, deliveries in `DELIVERED`, warranties in `ACTIVE` — and none of that state passed through a guard. Creation in this model is the transition into the initial state, so a `SOLD` unit cannot be created; replaying its history as transitions is not viable either, since legacy state was set by side effects and the inventory repository's own tracker records audit-logging gaps. Import must therefore use the authorised, recorded override that DESIGN.md's Known limits already requires for administrative repair, at scale, with these properties:

- **Provenance is `asserted`.** Imported state is recorded as asserted by the importer from a named legacy record, distinct from state ObjectKeeper observed a transition produce. This is the first concrete use of the provenance classes listed in TODO.md.
- **Validate and report; never admit silently.** The importer evaluates the declaration against every incoming object and produces a per-object report of violated invariants and unsatisfied structural guards. The inventory repository's ADR-0003 shows what will be found: a customer table whose 123 live rows collapse to 76 organisations, and placeholder emails satisfying a `NOT NULL` column. Whether a class of violation is cleaned upstream or admitted with a recorded flag is a decision per class, taken by a person, not a default in the importer.
- **Legacy history is preserved as read-only entries of kind `legacy`.** The existing audit log carries before/after values, principal, source and IP address; it does not have the shape of ObjectKeeper's event log, which begins at import. The inventory repository already used this pattern once: free-text notes were migrated as structured notes of type `LEGACY`.
- **External identifiers survive.** Serial numbers, the Xero invoice numbers held in a delivery's `order_id`, and the planned Xero `ContactID` are referenced from outside the system; a Jira task link exists in the proposal but whether it is populated is uncertain. Objects need a stable external identifier, and every cross-reference — delivery to units, service to parts, contract to robot — lands intact. Under ADR-0018 each imported object receives a new store-assigned id, its legacy primary key is preserved as an external identifier, and cross-references are re-pointed through that mapping.
- **Soft-deleted rows must land somewhere.** Every legacy table carries `is_deleted`; those rows exist for audit and cannot be dropped. This makes the question of what deletion is in this model concrete (TODO.md).
- **Schema evolution and import are one mechanism.** TODO.md asks what happens to objects mid-lifecycle when a type gains a guard. Import is that situation for every object at once; one design should serve both.

**Cutover cannot be dual-write.** A sole write path means the legacy application and the rebuilt one cannot share a store during a transition. Cutover is per deployment, possibly staged by object type with ObjectKeeper read-only for the rest. The plan is a project concern; the design must not assume a soft landing.

**Adversarial testing follows the threat model.** The acceptance test is a fallible actor — one that guesses, retries and skips steps — trying every route to an invalid state, not a hostile actor with credentials.

The import-path properties above were derived in discussion on 2026-09-07 as consequences of decisions 2 and 3. They have not been individually confirmed and are listed under Confirmations outstanding in TODO.md.

## Open

- **Whether the project is an open-source library** was left undecided on 2026-09-07. Nothing in this ADR depends on it.
- **What "preserved" covers beyond object rows.** The existing system stores photos (robot, delivery, service, intake), packing-list PDFs and label templates. ObjectKeeper does not store blobs. Whether it holds references to files kept elsewhere, and who migrates the files, is analysed in ADR-0017 (proposed) and not yet decided.
- **Cutover staging**: big-bang or per object type.

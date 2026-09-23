# Case study: customer records (HubSpot-shaped)

Status: design iteration 3, 2026-09-08. Companion to [`first-consumer-walkthrough.md`](first-consumer-walkthrough.md) and [`case-study-tickets.md`](case-study-tickets.md). Decisions taken here are ADR-0030 and ADR-0031 plus three clarifications, all pending author review.

> **Re-expressed 2026-09-08** against the grammar of ADR-0046, the semantics of ADR-0047 and the amendments of ADR-0052, which this re-expression is what found. Declarations here are current; the surrounding prose records how the study reached them.
## 1. Why this case

A CRM stresses what the previous two did not. Its objects are **joined many-to-many with labelled, attributed links** — a contact is the decision maker on one deal and the billing contact on another, and has one primary company among several. Its lifecycle is thin and its data is thick: most of the work is property edits, not transitions. It is the natural home of **duplicate merging**, of **record ownership** that governs who may see and edit what, and of **legal erasure** that must reach into history — the first requirement anywhere in these studies that pushes against the recorded property. And its first consumer overlap is real: the inventory system has decided to mirror its customers from Xero and has not built it — `wr:docs/adr/0003` is accepted design intent with no external-id column and no Xero code (`wr:docs/adr/0003`).

## 2. Mapping

| CRM concept | In this model |
|---|---|
| Contact, Company, Deal, Ticket, custom object | object types |
| Property; property group; property type and options | attribute; grouping is presentation; type and option set are attribute declaration metadata |
| Property history with source (user, form, import, API, workflow) | the object's recorded events; actor (ADR-0025) plus a request `context` string recorded on the event (clarification C2) |
| Lifecycle stage (subscriber → lead → MQL → SQL → opportunity → customer), forward-only | the Contact's state machine; backward moves are admin transitions; lead status is a plain attribute (ADR-0003, "plain fields") |
| Deal pipeline; stages; a deal in one pipeline | a named state machine per pipeline; a type per pipeline extending `Deal` (ADR-0026) |
| Required properties when entering a stage | requiredness on the transition (ADR-0002) |
| Closed won / closed lost, reopenable | states whose terminality the type author chooses |
| Moving a deal to another pipeline | supersession (ADR-0028): a new deal in the other type, the old one ending with a pointer |
| Association with labels ("decision maker", "billing") and a primary flag | a **link object** — a type with two references and its own attributes (clarification C1); "at most one primary company per contact" is a type-level invariant on it (ADR-0009), enforced by serialisable isolation and additionally compiled to a partial unique index where the backend supports one (ADR-0039, ADR-0041) |
| Timeline activity (email, call, meeting, note) associated to several records | an object with references whose only transitions are creation and `invalidate`, which is what append-only means here; not a part, because it is not exclusive to one record |
| Record owner; teams | a reference to a user object; guards `actor.id == owner.id or actor.has(EDIT_ALL)` (ADR-0025) |
| "View only owned or team records" | a **read visibility predicate** on the type (ADR-0030) |
| Duplicate detection on email | a uniqueness invariant, partial over absent values because email is personal and erasure writes absence (ADR-0051); the creation verdict names the conflicting object |
| Merge duplicates | an action on the surviving record that takes the resolved values as inputs and cascades the loser's supersession and the re-pointing of its links (§3) |
| Unmerge | not built in; see `edge-cases.md` |
| GDPR delete | **erasure** — redaction of declared personal attributes across the object and its history; distinct from deletion (ADR-0031) |
| Rollup: number of associated deals | derived attribute `count(d in deals)` (ADR-0021, ADR-0047) |
| Rollup: total deal amount; days since last activity; most recent activity date | derived attributes: `sum(d in deals: d.amount)`, `now - max(v in activities: v.at)` (ADR-0032, ADR-0047). Queried by filtering the stored operand rather than the derived value (ADR-0048) |
| Workflows, sequences, lead scoring, auto-association by email domain | consumers subscribed to events; selection and effects stay outside (ADR-0007, ADR-0012). *(ADR-0081: a score computed only from the store's own data is a declared formula the store may own; one needing outside data or a model stays outside.)* |
| Forms, bulk import, upsert by email | creation transitions with idempotency keys; import path (ADR-0015); upsert is two requests the consumer sequences |
| Company hierarchy (parent company) | a self-reference; acyclicity is **not expressible** in version 1 (see edge cases) |

## 3. Merge, fully declared

Which value wins is domain logic, so the consumer resolves the values and the store records the merge. The surviving fields are **declared**, not passed as a map: ADR-0046 refuses a dynamic attribute set because it cannot be checked at publish or printed in a rule set, and ADR-0052 makes an unsupplied optional input skip its write rather than clear the field.

```text
capability CONTACT_MERGE

act merge_in at ACTIVE accepts email, phone, company {
  input loser : Contact
  require distinct: inputs.loser != this                 because self_serviceable
  require live:     inputs.loser.state == Contact.ACTIVE because dependent
  require may:      actor.has(CONTACT_MERGE)             because delegable
  for a in inputs.loser.associations limit 1000 { call a.repoint(contact := this) }
  for v in inputs.loser.activities   limit 5000 { call v.repoint(contact := this) }
  call inputs.loser.merged_into(successor := this)
}
```

`merged_into` is a superseding terminal transition (ADR-0028). The loser keeps its id and history; the survivor's history records the merge with the loser as cause; the read surface presents a combined timeline by following `supersedes`. Every re-pointed link records its previous target in its own history, which is what a later, consumer-built unmerge would read.

## 4. What the model could not say, and what was decided

**Who may see a record.** Guards say who may change an object; nothing said who may read one, and a CRM's team scoping, like a tracker's issue-level security, is exactly that. **ADR-0030**: a type may declare a visibility predicate over the actor and the object; every read, query and availability result applies it; an object the actor cannot see is *not found*, not *blocked*, so the verdict does not leak existence.

**Erasing a person.** A right-to-erasure request must remove personal values from the current record *and from history*. Deletion (ADR-0024) keeps history by design. **ADR-0031**: attributes may be declared `personal`; an erasure transition, authorised and recorded, replaces those values with a redaction marker on the object and in every event that carried them, keeps the event skeleton (who, when, which transition, which states), deletes referenced content-addressed files (ADR-0017), and is irreversible. It is not deletion: the object may continue to exist, erased.

**Three clarifications** that needed no new mechanism:

- **C1. References carry no attributes.** A relationship with labels, a primary flag, dates or a lifecycle is a link object: a type with the two references and its own attributes and machine. The inventory's engagement lines and the tracker's typed links are already this shape.
- **C2. A request may carry a `context`** — a free string such as `form:contact-us` or `import:batch-42` — recorded on the event beside the actor. Provenance then answers "through what" as well as "by whom".
- **C3. An invariant-violation verdict names the conflicting objects.** A duplicate email on creation reports the existing contact's id, so the caller can merge or link rather than guess.

## 5. What held without change

Pipelines are named machines bound by per-pipeline types. Stage requirements are transition requiredness. Ownership guards are actor guards. Associations are link objects with a type-level invariant. Merge is an action cascading a supersession and re-points. Rollup counts are derived attributes. Import, idempotency and provenance applied as designed. The availability query changed twice afterwards: only-via transitions are no longer listed (ADR-0041), and the sweep `available(type, transition)` now requires the transition to be sweepable (ADR-0048), which `availability(id)`, listing one object's transitions, does not.

## 6. Rare cases, recorded in `edge-cases.md`

- Unmerge.
- Acyclicity of a self-referential hierarchy.
- Erasing a value that a guard or derived attribute elsewhere depends on.
- Erasure versus content-addressed file references.
- Moving a deal between pipelines yields a new object id.

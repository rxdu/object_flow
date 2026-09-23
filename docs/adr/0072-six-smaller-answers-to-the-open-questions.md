# ADR-0072: Six smaller answers to the open questions

- **Status:** Accepted — recommendations accepted by the author 2026-09-08
- **Date:** 2026-09-08
- **Refined by:** ADR-0103 — the first consumer does keep a stock level, for bulk accessories, so the counter is exercised; question 10's premise is corrected.
- **Refines:** ADR-0030, ADR-0050, ADR-0052, ADR-0060 §8 — whose non-negative counter §5 below reverses
- **Answers:** open questions 2, 3, 5, 6, 10, 11

## Context

Six of the thirteen open questions are answered without a substantial change to the model. They are recorded together so that each has a reason attached and none is re-litigated from the question alone.

## Decisions

### 1. A `referrers` element gains a comparable type name (question 2)

Not full filtering. An element already carries `.id` and `.state`; it gains `.type`, comparable against a declared type name, which is enough for a deletion guard to treat one referencing type differently.

The case is real. The first consumer's `Note` is polymorphic over `(entity_type, entity_id)` with **no foreign key** (`wr:app/models/note.py:37-38`), sits in no cascade configuration, and is therefore orphaned rather than disposed of when its subject goes. Modelling that correctly here needs a deletion guard that can say "notes do not block, delivery items do".

Rejected: heterogeneous member access, which would let a guard read an attribute the element's type may not have.

### 2. An inline state list and a machine's states share both spellings (question 3)

`states A category live, B category closed terminal` and one `state` per line are both legal in both places. Two spellings for one concept was friction with no compensating clarity.

### 3. Question 5 is already answered; it is closed (question 5)

§3.1 says a stored relationship end is always available to a query filter and rejects marking it `indexed`. That is an index on the stored end. Declaring an inverse therefore already implies one, and the question was open only because the implication was never stated as the answer.

### 4. Confidentiality stays per-object (question 6)

No per-attribute mechanism. The survey of the first consumer found **no field-level access control anywhere**, and no cost, margin, supplier-terms or salary column to protect: permissions are resource-and-action only (`wr:app/core/permissions.py:23-52`), and every role that may read a row reads all of it, price included (`wr:app/api/spare_parts.py:51`). The only case is a reviewer's trial-blinding model.

The cost is recorded rather than paid: hiding one field from one role costs a separate type with its own lifecycle, its own visibility predicate and a cascade clause per terminal transition of the whole.

### 5. Counters are deferred, and non-negativity moves to an invariant (question 10)

No money counter, and no signed counter, for now. The survey found the counter mechanism **unexercised by the consumer the design was drawn from**: it stores no stock level at all, computing availability on demand as available units minus allocations plus inbound minus soft pegs (`wr:app/services/atp.py:9`), and its only count-based things have no stock record.

One change is made because it costs a line and removes a special case: **non-negativity is not built into `counter`; it is an invariant the type declares.** Non-negativity is a fact about stock, not about counters, and the model already has a mechanism for facts about a type.

The deeper observation is recorded in `design/edge-cases.md`: a design offering a maintained counter as the remedy for a slow scan should be able to point at a consumer that maintains one.

### 6. An authority list may name a family, but not yet (question 11)

Agreed in principle, not built. No family in the first consumer needs it. The issue-tracking study does, where one abstract base has many members and a shared child lists members times triggers in three places that must be kept in step. When it is built, the family must be named explicitly, so that a new member gaining authority is visible in one place — the objection ADR-0020 raises against an unlisted list applies to a form that grants silently.

## Consequences

- Questions 2 and 3 change the syntax document; 5 closes an implication already made; 6, 10 and 11 are recorded refusals or deferrals with their evidence, which is what stops them being re-asked from first principles.
- Two of these answers moved after the production survey. Per-attribute confidentiality looked likely and turned out to have no need at all, and the money counter turned out to rest on a mechanism the first consumer does not use.

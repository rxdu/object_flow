# ADR-0086: Assignment is a declared role on a reference, kept from its first write, with its metrics generated

- **Status:** Accepted by the author, 2026-09-23 — evaluated at the author's direction against `docs/PRD.md`, the baseline for every design choice
- **Date:** 2026-09-23
- **Refined by:** ADR-0096 — `.held(<member>)` attributes time to whoever held the work then, and cycle time by assignee is a standard metric. ADR-0098 — the assignment metrics are declared in declaration-syntax.md §6.11, with `.actor_id` for acting by someone other than the assignee. ADR-0106 — `.held` on a transition is the value before its writes.
- **Refines:** ADR-0083

## Context

The author, 2026-09-23: "assignee change is also important and I'd like to keep track of this kind of data from the beginning." The PRD records it as D10: every change of assignee is captured from the start, with who made it, when, and ported history where the legacy audit trail has it. It adds D11, the same for other data of that kind, and M6, standard assignment metrics. UC-18 is the test.

**The first consumer.** A service job has a required engineer-of-record, `engineer_id` (`wr:app/models/service.py:44`). A reassignment is validated like a creation and audited with before-and-after snapshots (`wr:app/services/service_service.py:357-372`). An agent may not be engineer-of-record (`wr:docs/agent-operations.md` §3). A user with active services must have them reassigned before removal (`wr:app/models/users.py:34`).

ADR-0083 keeps intervals as an index the log rebuilds, and this decision widens them. `design/data-driven-engine.md` §3.9 evaluates the options.

## Decision

### 1. "From the beginning" means written through the store from the beginning

Every singular reference has intervals from its first write, and the permanent log can rebuild them, so an `assignee` marking added in version 5 has metrics back to version 1. What cannot be recovered is an assignment that never passed through the store: one kept in Jira, in free text, or in someone's head. So assignment is modelled as a reference that transitions write, from the first version of a type that has one.

### 2. Intervals cover every enum attribute and every singular stored reference

This is ADR-0083's index, widened from state to value. Absence is a value, so time unassigned is an interval.

### 3. `assignee` marks a singular reference as a responsibility

`ref engineer : User assignee`. The referenced type marks one `identity` attribute as the actor's id (`attr login : identity actor`), so the engine can tell when someone other than the assignee acts. A type may mark more than one reference, for instance an engineer-of-record and a reviewer, and each is a separate dimension named by its reference.

### 4. The marking adds no write path

Assignment is whatever declared transitions write the reference: a creation that `accepts engineer`, a `reassign` action. Each carries its own guards on who may assign and who may be assigned. "An agent may not be engineer-of-record" is such a guard.

### 5. Every assignee dimension gets metrics without declaring any

- open work per assignee;
- time unassigned;
- time to first assignment;
- time with each assignee;
- handoffs per object, and reassignment back to an earlier assignee;
- how often a transition is requested by someone other than the assignee.

*(ADR-0096 §5 adds cycle time by the assignee at completion, and time in each state by whoever held the object then, which UC-18's "whether cycle time differs by engineer" needs.)*

Each can be split by the kind of actor that made the assignment. Diagnostics report reassignment loops and work done by non-assignees (ADR-0085).

### 6. Choosing the assignee stays outside the engine

ADR-0007 and ADR-0081 keep selection out. The engine guards and records the choice and supplies the workload data; which person or agent is assigned is decided by a person or an agent.

### 7. Legacy reassignments are ported as intervals

The first consumer's service updates record `engineer_id` in before-and-after audit snapshots, so the import mapping rebuilds them as legacy intervals. How far back that trail reaches is to be sampled against production.

### 8. It belongs to the first release

History can only begin on the day it is written through the store.

## Alternatives rejected

- **An ordinary reference, with metrics written per type by hand.** It captures the writes, but every type names its assignee differently, so no metric or diagnostic can be written once, and the numbers exist only where someone thought to instrument them (M6 fails).
- **A built-in assignee on every object, with a built-in `assign` operation.** Not every type has one, some have two, and the first consumer's is required at creation. And a built-in operation writing a reference no declared transition writes is the second write path ADR-0042 refuses.
- **Track assignment in an observation kind** (ADR-0082) recorded beside the reference. That is two records of one fact, which can disagree; the reference already is the fact, and its intervals are its history.
- **Track every attribute, numbers and text included.** A time-in-value over a price or a free-text field is not a question anyone asks, and it multiplies the index for nothing. Their changes stay in the events.

## Consequences

- On acceptance, `DESIGN.md` §5.2, §5.3 and §7 describe value intervals and the marking.
- `design/declaration-syntax.md` gains `assignee` and the `actor` identity marking, with checks: `assignee` only on a singular reference, and its target type marks exactly one `actor` identity.
- `design/storage-schema.md` widens `of_interval` to a dimension and value.
- `design/publish-and-import.md` lets the mapping supply legacy intervals.
- `design/renderers.md` names the assignee in the rule set.
- ADR-0083 carries the back-link.

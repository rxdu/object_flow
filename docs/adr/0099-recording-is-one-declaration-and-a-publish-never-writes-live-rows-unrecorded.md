# ADR-0099: Recording is one declaration, dated by the request and offered like any transition, and a publish never writes a live row without an event

- **Status:** Accepted — decided 2026-09-23 at the author's direction, against `docs/PRD.md` D3, D5, D10, D11, F3, F5, T1, T3, UC-4, UC-6, UC-8, UC-19; repairs D245 to D250
- **Date:** 2026-09-23
- **Refines:** ADR-0027, ADR-0073, ADR-0082, ADR-0083, ADR-0095, ADR-0096

## Context

A review of the record against the PRD found six gaps in how datapoints are recorded, and in how a publish meets objects that already exist (`design/defects.md`).

- **D245. Adding a datapoint kind took two edits.** Check 54 required the subject to declare the part, so adding a kind edited the subject's text as well, and advanced its version. D3 says adding one is "one declaration and no code".
- **D246. An observation's occurred time had two channels and no verdict.**
  - The syntax made `occurred_at` an input of the generated creation, while the API made it a request field "only on a backdatable transition".
  - The observation table had no column for it.
  - Nothing named a clause for an occurred time outside its bound, so the refusal UC-19's second route relies on had no clause, no remedy and no count.
- **D247. Recording could not be discovered.** `availability(id)` lists an object's own transitions, and `check` needs an id, so nothing told a caller whether recording, or any creation, was permitted. The observation tool nevertheless told agents to call `availability(subject_id)`, and put `subject_id` where the request has no field.
- **D248. What recording writes to the subject was unsaid.** Recording "does not write the subject". Yet a later result must invalidate `changed_since([inspections])`, which reads a position stamped per part relationship.
- **D249. An optional reference could never be cleared.** ADR-0073 forbade `clear` on a relationship end, and said to write the reference instead. There is no `null` to write, and an unsupplied optional input skips its write. So an assignee could not be removed, and D10 and D11 could never record an interval of absence after creation.
- **D250. A publish meets live objects without recording what it does to them.**
  - A new attribute was added by DDL "nullable, or with the declared `default`". With a default, every live row changed with no event, and replaying the log no longer reproduced the row (T3). Without one, live objects lacked a required attribute (T1).
  - Nothing could supply the values: the mapping forms were only `removed state` and `renamed attr`.
  - A removed enum member had no mapping.
  - Nothing said what removing a type or an observation kind means.

## Decision

### 1. A kind names its collection, and the subject's text is not edited

`observation <Kind> version <n> on <Type> as <collection> { … }` gives the subject a set-valued part named `<collection>`, with inverse `subject`. The subject does not declare it. A subject member of the same name is refused (check 33).

Adding a kind is then one declaration, and the subject's version does not advance, since its text has not changed. The declaration version, which every object and event records, still identifies the rules in force.

### 2. Occurred time is a request field, and its bound is a named clause

- `Request.occurred_at` supplies the occurred time for two things: a `backdatable` transition, and an observation's `record`. It is never an input.
- Both carry a generated guard, `occurred_within`, with remedy `self_serviceable`. It refuses an occurred time further back than the declared bound, later than the time the request is recorded, or, for a transition, before the start of the current interval of anything the request changes.
- A request that supplies `occurred_at` where neither applies is refused by the same clause.
- Every observation table has an `occurred_at` column.
- A label's occurred time is its recorded time.

### 3. Recording is offered and checkable like any transition

- `availability(subject)` lists, beside the subject's own transitions, the `record` of each kind on it and `label`, with the same three-way availability, verdict and input schema.
- `check` accepts a type in place of an object id for any creation.
- The observation tool carries the subject as an input named `subject`, as the syntax declares it.

### 4. Recording stamps one thing on the subject

It writes the subject's part-relationship position for its collection (ADR-0082 §7), and nothing else of the subject: no version, no attribute. A transition whose guard read that collection conflicts with it under serialisable isolation, and is retried. Every other transition does not conflict.

### 5. A re-inspection is a new result, and a correction fixes a record

A correction says a recorded fact was wrong. A later result says the thing changed. A gate that must pass on the latest result per item reads the latest one, as §6.10's example now does. First-pass yield is then the first result per item, and a correction never erases it from the record.

### 6. `clear` reaches an optional reference

`clear <reference>` writes absence to an optional, singular, stored `ref` end. It is refused on a required end, a set, a `part` or an `owner` (check 17). This refines ADR-0073, whose reason — "a reference has a value to write" — does not hold for absence.

### 7. A publish changes a live row only by a recorded migration

- **Defaults.** DDL never writes a live row. A new column is added nullable, and a declared `default` applies at creation only, as the syntax already says.
- **New required attributes.** One on a type with live objects needs a `backfill <attribute> := <expression>` mapping. The expression reads the object's own members and literals, and publishing applies it to each live object as a recorded migration transition, provenance `migrated`. Without the mapping, the report counts the objects and the publish is refused (check 23).
- **New observation fields.** A field added to an existing kind must be optional, since an observation is born final (check 22).
- **Removed enum members.** Removing a member that live rows hold needs `removed member <Enum>.<MEMBER> -> <MEMBER>`, applied the same way. Without it, the publish is refused. Events keep the old value, read under the version that wrote them.
- **Removed types and kinds.** Removing a type or an observation kind retires it. No object is created in it and no transition applies, but its objects, table and history stay readable under the versions that wrote them.

## Alternatives rejected

- **Keep the subject's part line, and exempt it from versioning.** The subject's text would then say one thing and its version another.
- **Keep `occurred_at` as an input of the record.** Two channels for one fact, and the tool schema and the API would go on disagreeing about which one a caller uses.
- **A separate operation for "may I record".** `availability` already answers "what may I do to this object", and recording is something done about it.
- **Treat a later result as a correction.** The first result would vanish from every read, and a first-pass yield could not be measured.
- **Let the DDL write defaults, and record nothing.** The log would no longer reproduce the row, which T3 and the harness both require.
- **Allow a `null` on the right of `set` for references.** ADR-0073's reason for refusing it for attributes applies unchanged.

## Consequences

- `declaration-syntax.md` §4.2, §5.2, §6.6 and §6.8, the §6.10 example and checks 17, 22, 23, 33 and 54 change.
- `DESIGN.md` §5.4, §5.11, §6 and §10, and `library-api.md`'s `Request` and `check`, change.
- `storage-schema.md` §3 and §10 change.
- `renderers.md` §3's observation tool changes.
- `publish-and-import.md` §2 and §3 describe the new mappings.
- D245 to D250 are resolved.

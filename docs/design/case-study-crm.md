# Case study: customer records (HubSpot-shaped)

Status: design iteration 3, 2026-09-08. Companion to [`first-consumer-walkthrough.md`](first-consumer-walkthrough.md) and [`case-study-tickets.md`](case-study-tickets.md). Decisions taken here are ADR-0030 and ADR-0031 plus three clarifications, accepted by the author on 2026-09-28, ADR-0030 as superseded by ADR-0114.

> **Amended 2026-09-25 for ADR-0114.** ObjectFlow records who acted and never evaluates it. The declarations below lost every clause that read who is asking or declared who may do or see something — capabilities, actor guards, visibility — which the upper layer now decides; their mapping rows say so. The narrative records the model as the study found it, before ADR-0114.

*Vocabulary, noted 2026-09-24:* written before PRD revision 5, this document says "consumer" for an application built on the store, which PRD §5 now calls an upper-layer application, and sometimes for the deployment or a reader; "the first consumer" keeps its meaning (D368).

> **Re-expressed 2026-09-08** against the grammar of ADR-0046, the semantics of ADR-0047 and the amendments of ADR-0052, which this re-expression is what found. Declarations here are current; the surrounding prose records how the study reached them.
>
> **Rewritten 2026-09-26 in the flow description format** (ADR-0116). The merge of §3 is an excerpt of the module in the appendix, written then so that the excerpt is part of a checked module (ADR-0121); `scripts/check-flow-docs.py` checks both.
## 1. Why this case

A CRM stresses what the previous two did not. Its objects are **joined many-to-many with labelled, attributed links** — a contact is the decision maker on one deal and the billing contact on another, and has one primary company among several. Its lifecycle is thin and its data is thick: most of the work is property edits, not transitions. It is the natural home of **duplicate merging**, of **record ownership** that governs who may see and edit what, and of **legal erasure** that must reach into history — the first requirement anywhere in these studies that pushes against the recorded property. And its first consumer overlap is real: the inventory system has decided to mirror its customers from Xero and has not built it — `wr:docs/adr/0003` is accepted design intent with no external-id column and no Xero code (`wr:docs/adr/0003`).

## 2. Mapping

| CRM concept | In this model |
|---|---|
| Contact, Company, Deal, Ticket, custom object | object types |
| Property; property group; property type and options | attribute; grouping is presentation; type and option set are attribute declaration metadata |
| Property history with source (user, form, import, API, workflow) | the object's recorded events; actor (ADR-0025) plus a request `context` string recorded on the event (clarification C2) |
| Lifecycle stage (subscriber → lead → MQL → SQL → opportunity → customer), forward-only | the Contact's state machine; backward moves are transitions only an administrator is let request, by the upper layer (ADR-0114); lead status is a plain attribute (ADR-0003, "plain fields") |
| Deal pipeline; stages; a deal in one pipeline | a named state machine per pipeline; a type per pipeline extending `Deal` (ADR-0026) |
| Required properties when entering a stage | requiredness on the transition (ADR-0002) |
| Closed won / closed lost, reopenable | states whose terminality the type author chooses |
| Moving a deal to another pipeline | supersession (ADR-0028): a new deal in the other type, the old one ending with a pointer |
| Association with labels ("decision maker", "billing") and a primary flag | a **link object** — a type with two references and its own attributes (clarification C1); "at most one primary company per contact" is a type-level invariant on it (ADR-0009), enforced by serialisable isolation and additionally compiled to a partial unique index where the backend supports one (ADR-0039, ADR-0041) |
| Timeline activity (email, call, meeting, note) associated to several records | an object with references whose only transitions are creation and `invalidate`, which is what append-only means here; not a part, because it is not exclusive to one record |
| Record owner; teams | a reference to a user object; who may edit is the upper layer's (ADR-0114) |
| "View only owned or team records" | the upper layer's, which filters reads on the owner or team reference (ADR-0114) |
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

```yaml
Contact:
  conditions:
    distinct:
      description: The contact merged in is another contact.
      expression: inputs.loser != this
      remedy: self_serviceable
    live:
      description: The contact merged in is still live.
      expression: inputs.loser.state == Contact.ACTIVE
      remedy: dependent
  transitions:
    merge_in:
      kind: internal
      from: ACTIVE
      optional_inputs: [email, phone, company]
      inputs:
        loser: { reference: Contact }
      guards:
        distinct: deny
        live: deny
      effect:
        - foreach:
            item: a
            array: inputs.loser.associations
            limit: 1000
            steps:
              - call: { target: a, transition: repoint, inputs: { contact: this } }
        - foreach:
            item: v
            array: inputs.loser.activities
            limit: 5000
            steps:
              - call: { target: v, transition: repoint, inputs: { contact: this } }
        - call: { target: inputs.loser, transition: merged_into, inputs: { successor: this } }
```

The merge is an excerpt of `Contact` in the module of the appendix, which holds what the merge needs and no more. `merged_into` enters `MERGED`, a superseding final state, and names the survivor with `supersede` (ADR-0028). The loser keeps its id and history; the survivor's history records the merge with the loser as cause; the read surface presents a combined timeline by following `supersedes`. Every re-pointed link records its previous target in its own history, which is what a later, consumer-built unmerge would read.

## 4. What the model could not say, and what was decided

**Who may see a record.** Guards say who may change an object; nothing said who may read one, and a CRM's team scoping, like a tracker's issue-level security, is exactly that. **ADR-0030**: a type may declare a visibility predicate over the actor and the object; every read, query and availability result applies it; an object the actor cannot see is *not found*, not *blocked*, so the verdict does not leak existence. *(Superseded by ADR-0114: no read is filtered by who asks, and who may see a record is the upper layer's.)*

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

## Appendix: the module the merge belongs to

What §3's merge needs and no more: a contact, the company it may belong to, and the two kinds of record the merge re-points. A contact's email is unique among live contacts only, since a merged contact keeps its email, which its survivor may take; unique across every contact, the survivor could never take it. A contact holds personal data, so it declares an erasure, which the format requires of such a type (ADR-0133).

```yaml
module: crm
categories: [live, closed]

types:
  Company:
    description: A company contacts belong to.
    tracking: record

    attributes:
      name:     { type: string, indexed: true }
      contacts: { reference: "Contact[]", opposite: company }

    states:
      ACTIVE:   { category: live }
      ARCHIVED: { category: closed, final: true }

    transitions:
      create:
        kind: initial
        to: ACTIVE
        required_inputs: [name]
      archive:
        kind: external
        from: ACTIVE
        to: ARCHIVED

  Contact:
    description: A person the business is in touch with, merged into another contact when it turns out to be a duplicate.
    tracking: record

    attributes:
      email:
        type: string
        optional: true
        personal: true
        indexed: true
        unique: { where: state == ACTIVE }
        description: Unique among live contacts, since a merged contact keeps its email, which its survivor may take.
      phone:        { type: string, optional: true, personal: true }
      company:      { reference: Company, optional: true, opposite: contacts }
      associations: { reference: "Association[]", opposite: contact }
      activities:   { reference: "Activity[]", opposite: contact }

    states:
      ACTIVE: { category: live }
      MERGED: { category: closed, final: true, superseding: true }

    conditions:
      distinct:
        description: The contact merged in is another contact.
        expression: inputs.loser != this
        remedy: self_serviceable
      live:
        description: The contact merged in is still live.
        expression: inputs.loser.state == Contact.ACTIVE
        remedy: dependent

    transitions:
      create:
        kind: initial
        to: ACTIVE
        optional_inputs: [email, phone, company]
      merge_in:
        kind: internal
        from: ACTIVE
        optional_inputs: [email, phone, company]
        inputs:
          loser: { reference: Contact }
        guards:
          distinct: deny
          live: deny
        effect:
          - foreach:
              item: a
              array: inputs.loser.associations
              limit: 1000
              steps:
                - call: { target: a, transition: repoint, inputs: { contact: this } }
          - foreach:
              item: v
              array: inputs.loser.activities
              limit: 5000
              steps:
                - call: { target: v, transition: repoint, inputs: { contact: this } }
          - call: { target: inputs.loser, transition: merged_into, inputs: { successor: this } }
      merged_into:
        kind: external
        from: ACTIVE
        to: MERGED
        only_via: [Contact.merge_in]
        inputs:
          successor: { reference: Contact }
        effect:
          - supersede: inputs.successor
      forget:
        kind: erasure
        inputs:
          reason: { type: string }

  Association:
    description: A labelled link between a contact and a company, such as the billing contact, with a primary flag.
    tracking: record

    attributes:
      contact:  { reference: Contact, opposite: associations }
      target:   { reference: Company }
      label:    { type: string }
      primary:  { type: bool, default: false }

    states:
      ACTIVE:  { category: live }
      REMOVED: { category: closed, final: true }

    transitions:
      link:
        kind: initial
        to: ACTIVE
        required_inputs: [contact, target, label, primary]
      repoint:
        kind: internal
        from: ACTIVE
        only_via: [Contact.merge_in]
        required_inputs: [contact]
      unlink:
        kind: external
        from: ACTIVE
        to: REMOVED

  Activity:
    description: An email, call, meeting or note on a contact's timeline, which is never edited, only invalidated.
    tracking: record

    attributes:
      contact: { reference: Contact, opposite: activities }
      at:      { type: timestamp, indexed: true }
      summary: { type: string, optional: true, personal: true }

    states:
      RECORDED:    { category: closed }
      INVALIDATED: { category: closed, final: true }

    transitions:
      record:
        kind: initial
        to: RECORDED
        required_inputs: [contact, at]
        optional_inputs: [summary]
      repoint:
        kind: internal
        from: RECORDED
        only_via: [Contact.merge_in]
        required_inputs: [contact]
      invalidate:
        kind: external
        from: RECORDED
        to: INVALIDATED
      forget:
        kind: erasure
        description: Erases the activity's summary, which may name a person, at a person's request.
        inputs:
          reason: { type: string }
```

*(Added 2026-09-28: `forget` on `Activity`, since every type holding a personal value declares its erasure (ADR-0133, PRD D8); the summary is personal, and before this nothing could erase it.)*

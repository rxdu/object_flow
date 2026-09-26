# Case study: approvals, delegation, proposals, and bookings

Status: design iteration 5, 2026-09-08. Companion to the earlier case studies. Decisions taken here are ADR-0035 and ADR-0036, all pending author review. The shapes are the common ones — purchase-request approval, document review, leave requests, room and equipment booking — and the first consumer's own engagement axis (`wr:docs/adr/0002`) is a booking system it has not yet built.

*Vocabulary, noted 2026-09-24:* written before PRD revision 5, this document says "consumer" for an application built on the store, which PRD §5 now calls an upper-layer application, and sometimes for the deployment or a reader; "the first consumer" keeps its meaning (D368).

> **Amended 2026-09-25 for ADR-0114.** ObjectFlow records who acted and never evaluates it. The declarations below lost every clause that read who is asking or declared who may do or see something — capabilities, actor guards, visibility — which the upper layer now decides; their mapping rows say so. The narrative records the model as the study found it, before ADR-0114.

> **Re-expressed 2026-09-08** against the grammar of ADR-0046, the semantics of ADR-0047 and the amendments of ADR-0052, which this re-expression is what found. Declarations here are current; the surrounding prose records how the study reached them.
>
> **Rewritten 2026-09-26 in the flow description format** (ADR-0116). The purchase request of §2.2 is an excerpt of the module in the appendix, written then so that the excerpt is part of a checked module (ADR-0121); `scripts/check-flow-docs.py` checks both.
## 1. Why this case

Every earlier case had a single actor per transition. Approval is the case where a transition's guard is satisfied by **other actors having acted**, in a stated number, order or role, and where that satisfaction can be **invalidated by a later edit**. Delegation asks where authority comes from and whether the store must know. A proposal asks what happens when a caller lacks authority: an error, or a pending request someone else can carry. Bookings add **interval conflicts**, the one invariant shape that is easy to state and hard to enforce concurrently, and future-dated state, which the first consumer explicitly deferred.

## 2. Approvals

### 2.1 Mapping

| Approval concept | In this model |
|---|---|
| An approval | a **part** of the approved object: `Approval { approver, principal, decision, kind, comment, event }` created by an `approve` or `reject` action on the parent (ADR-0016, ADR-0019) |
| Who may approve | the upper layer's, which decides who may request `approve` (ADR-0114) |
| Separation of duties | the upper layer's where it concerns who is asking (ADR-0114); between recorded actors, such as two approvals by different approvers, a guard over the approvals' recorded `approver` |
| "Needs approval" gate on the real transition | a guard counting valid approvals: `count(a in approvals where a.decision == Decision.APPROVED and not changed_since([amount, vendor], a.at_event)) >= 1` (ADR-0035, ADR-0047) |
| N-of-M | `>= N` |
| All of a set | one guard per required `kind` |
| Sequential chain (manager, then finance) | the finance `approve` action is guarded on a valid manager approval existing |
| Threshold (director above an amount) | `amount > SGD 10000.00 implies any(a in approvals where a.kind == ApprovalKind.DIRECTOR and …)` (ADR-0032) |
| Approval invalidated by a material edit | the same guard, through `changed_since`: an approval older than the last change to the declared relevant attributes does not count (ADR-0035) |
| Withdraw request | an ordinary transition by the requester |
| Approval expires; escalate after N days | queried by filtering the stored `approval.at_event` against the supplied time, since a `now`-dependent derived attribute is not itself indexable (ADR-0048); escalation is a scheduler asking the availability query (ADR-0022) |
| Remedy when approval is missing | `delegable`, naming the kind of approval required, not a person |

### 2.2 A purchase request, declared

```yaml
# PurchaseRequest: DRAFT -> SUBMITTED -> APPROVED -> ORDERED | REJECTED | WITHDRAWN
PurchaseRequest:
  conditions:
    manager:
      description: A manager approved the request, and neither its amount nor its vendor has changed since.
      expression: >-
        any(a in approvals where a.kind == ApprovalKind.MANAGER
            and a.decision == Decision.APPROVED
            and not changed_since([amount, vendor], a.at_event))
      remedy: delegable
    director:
      description: Above SGD 10,000, a director approved the request too, and neither its amount nor its vendor has changed since.
      expression: >-
        amount > SGD 10000.00
        implies any(a in approvals where a.kind == ApprovalKind.DIRECTOR
                    and a.decision == Decision.APPROVED
                    and not changed_since([amount, vendor], a.at_event))
      remedy: delegable
  transitions:
    submit:
      kind: external
      from: DRAFT
      to: SUBMITTED
    approve:
      kind: internal
      from: SUBMITTED
      inputs:
        kind:    { type: ApprovalKind }
        comment: { type: string, optional: true }
      effect:
        - create:
            type: Approval
            transition: record
            inputs:
              request: this
              approver: actor.id
              principal: actor.principal
              kind: inputs.kind
              decision: Decision.APPROVED
              comment: inputs.comment
              at_event: this_event
    mark_approved:
      kind: external
      from: SUBMITTED
      to: APPROVED
      guards:
        manager: deny
        director: deny
    edit:
      kind: internal
      from: SUBMITTED
      required_inputs: [amount, vendor]
```

The request is an excerpt of `PurchaseRequest` in the module of the appendix, which holds what approving needs and no more. Editing `amount` after a manager approved does not delete the approval and needs no reset in `edit`; `mark_approved` simply stops being available, and the verdict says which approval is now stale. That is ADR-0009's argument again: the rule is stated once, at the type, not repeated in every editing action.

*(Corrected 2026-09-26, rewriting this declaration in YAML: `edit` accepted `amount` and `vendor` with the comment "each write skipped when not supplied", but both are required attributes, and an input on an attribute takes the attribute's optionality (`declaration-syntax.md` §5), so both were required and neither write could be skipped, which only an optional attribute's input allows (ADR-0052). `edit` now requires both (D393). The approval's comment, which `approve` accepted and so also wrote onto the request, is now an input carried to the approval alone.)*

## 3. Delegation

*(Since ADR-0114 the store holds no authority at all, so delegation, and whether a deputy may approve up to a limit, are wholly the upper layer's; this section records the model before it.)* Authority arrives in the actor descriptor (ADR-0025); the consumer's authentication computes it. A manager on leave who delegates to a deputy is, to the store, a deputy whose descriptor now carries `APPROVE_PURCHASE` for two weeks, with `principal` set to the manager. Attenuation — the deputy may approve only up to a limit — is not a descriptor attribute, since the descriptor has none (ADR-0079); it is the `Delegation` object below, which the guard reads: `any(d in Delegation where d.delegate == actor.id and d.state == Delegation.ACTIVE and d.limit >= amount)`.

The delegation is therefore an object type in the store, governed and recorded like anything else — `Delegation { delegator, delegate, capability, limit, from, until }` with a lifecycle `active → revoked | expired` — and the consumer's auth reads live delegations when computing descriptors. The store offers no delegation mechanism of its own; that pattern is enough (ADR-0036).

## 4. Proposals

*(Since ADR-0114 a proposal is a request held for approval whoever files it, and who may approve one is the upper layer's (DESIGN.md §9); this section records the model before it.)* A caller without authority for a transition receives `delegable`. Sometimes the right response is to carry the request to someone who has it. **ADR-0036**: `Proposal` is a built-in object type — the target object, the transition, the inputs, the proposer, and a lifecycle `pending → executed | rejected | withdrawn | invalidated` (ADR-0044). A transition opts in with `proposable`; a `delegable` verdict on a proposable transition says so. An authorised actor's `approve` on the proposal executes the recorded request as that actor, with the proposal's approval event as cause; **every** guard is re-evaluated then, the actor guards included, which the approver satisfies; if one fails the proposal stays `pending` carrying that verdict. The proposer is recorded on the proposal, not as principal of the execution: the approver is not acting on the proposer's behalf, they are authorising.

The first consumer's list of transitions where authority differs from capability has two entries, so `proposable` is opt-in and off by default.

## 5. Bookings

### 5.1 Mapping

| Booking concept | In this model |
|---|---|
| Resource (room, robot, vehicle) | an object type |
| Booking for an interval `[start, end)` | an object referencing the resource; lifecycle `requested → confirmed → active → returned → closed`, plus `cancelled` and `no_show` — the first consumer's engagement (`SCHEDULED → OUT → RETURNING → CLOSED`) is one |
| No two live bookings of one resource overlap | a type-level invariant: `none(b in Booking where b.resource == resource and b.state.category == live and state.category == live and b.start < end and b.end > start)`. A type-scan ranges over every *other* object, so there is no `id` exclusion (ADR-0061); the `live` filter is applied to both sides, which is what makes it **symmetric** under the swap test ADR-0052 requires so the affected set is computable. Concurrent safety comes from serialisable isolation (ADR-0039); on PostgreSQL it also compiles to an exclusion constraint, which is an optimisation, not the guarantee (ADR-0041) |
| "Is it free from X to Y?" | the `check(id, transition, inputs)` operation on the read surface, which evaluates without executing. Note that it takes an object id, and "is this room free" is asked before the booking exists, so this question has no answer on the current read surface. *(Answered since by ADR-0099: `check` takes a type for a creation, so "is this room free from X to Y" is `check(Booking, create, {resource, start, end})`, D350.)* |
| Capacity bookings (10 seats) | a quantity-tracked resource, which ADR-0050 makes a declared tracking mode rather than an improvisation |
| No-show after start plus grace | `no_show: confirmed → no_show, guard start + 15 min <= now`, requested by a scheduler (ADR-0022) |
| Recurring series | a `Series` object; instances are bookings the consumer generates and creates, each referencing the series |
| Waitlist | a queue of `Waitlist` objects; who is promoted on a cancellation is a **selection** the consumer makes (ADR-0007) |
| Business hours, local time | timestamps are absolute; calendar rules are consumer inputs |

### 5.2 What held

The interval invariant is a symmetric type-scan predicate, which ADR-0052 admits. Its correctness comes from serialisable isolation (ADR-0039) on every backend, and on PostgreSQL it additionally compiles to an exclusion constraint as an optimisation (ADR-0041). The earlier claim that a database constraint was the mechanism was wrong on SQLite, which has none.

## 6. Rare cases, recorded in `edge-cases.md`

- Approving several objects in one decision.
- An approver who loses authority after approving.
- Four-eyes across different objects.
- A proposal whose inputs the approver wants to amend.
- Editing a recurring series after instances exist; waitlist promotion; local-time rules; substitution bookings ("any robot of this model").

## Appendix: the module the purchase request belongs to

What §2.2's purchase request needs and no more: the request, its lifecycle to an order, and the approvals recorded as its parts, each with the event it was given at. An approval is created through its request, as a part is (the model's check 11), and the approvals survive the request's end, since they are its record.

```yaml
module: purchasing
categories: [live, closed]

enumerations:
  ApprovalKind: [MANAGER, DIRECTOR]
  Decision: [APPROVED, REJECTED]

types:
  PurchaseRequest:
    description: A request to buy something, approved by a manager, and by a director above an amount, before it is ordered.
    tracking: record

    attributes:
      amount: { type: money(SGD) }
      vendor: { type: string }
      approvals:
        reference: "Approval[]"
        aggregation: composite
        opposite: request
        survives: [order, reject, withdraw]

    states:
      DRAFT:     { category: live }
      SUBMITTED: { category: live }
      APPROVED:  { category: live }
      ORDERED:   { category: closed, final: true }
      REJECTED:  { category: closed, final: true }
      WITHDRAWN: { category: closed, final: true }

    conditions:
      manager:
        description: A manager approved the request, and neither its amount nor its vendor has changed since.
        expression: >-
          any(a in approvals where a.kind == ApprovalKind.MANAGER
              and a.decision == Decision.APPROVED
              and not changed_since([amount, vendor], a.at_event))
        remedy: delegable
      director:
        description: Above SGD 10,000, a director approved the request too, and neither its amount nor its vendor has changed since.
        expression: >-
          amount > SGD 10000.00
          implies any(a in approvals where a.kind == ApprovalKind.DIRECTOR
                      and a.decision == Decision.APPROVED
                      and not changed_since([amount, vendor], a.at_event))
        remedy: delegable

    transitions:
      create:
        kind: initial
        to: DRAFT
        required_inputs: [amount, vendor]
      submit:
        kind: external
        from: DRAFT
        to: SUBMITTED
      approve:
        kind: internal
        from: SUBMITTED
        inputs:
          kind:    { type: ApprovalKind }
          comment: { type: string, optional: true }
        effect:
          - create:
              type: Approval
              transition: record
              inputs:
                request: this
                approver: actor.id
                principal: actor.principal
                kind: inputs.kind
                decision: Decision.APPROVED
                comment: inputs.comment
                at_event: this_event
      mark_approved:
        kind: external
        from: SUBMITTED
        to: APPROVED
        guards:
          manager: deny
          director: deny
      edit:
        kind: internal
        from: SUBMITTED
        required_inputs: [amount, vendor]
      reject:
        kind: external
        from: SUBMITTED
        to: REJECTED
      withdraw:
        kind: external
        from: [DRAFT, SUBMITTED]
        to: WITHDRAWN
      order:
        kind: external
        from: APPROVED
        to: ORDERED

  Approval:
    description: One approval of a purchase request, recorded with the event it was given at, so a later edit makes it stale.
    tracking: record

    attributes:
      request:   { reference: PurchaseRequest, opposite: approvals }
      approver:  { type: identity }
      principal: { type: identity }
      kind:      { type: ApprovalKind }
      decision:  { type: Decision }
      comment:   { type: string, optional: true }
      at_event:  { type: event }

    states:
      RECORDED: { category: closed, final: true }

    transitions:
      record:
        kind: initial
        to: RECORDED
        only_via: [PurchaseRequest.approve]
        required_inputs: [request, approver, principal, kind, decision, at_event]
        optional_inputs: [comment]
```

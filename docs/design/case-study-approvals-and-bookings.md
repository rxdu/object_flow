# Case study: approvals, delegation, proposals, and bookings

Status: design iteration 5, 2026-09-08. Companion to the earlier case studies. Decisions taken here are ADR-0035 and ADR-0036, all pending author review. The shapes are the common ones — purchase-request approval, document review, leave requests, room and equipment booking — and the first consumer's own engagement axis (`wr:docs/adr/0002`) is a booking system it has not yet built.

> **Re-expressed 2026-09-08** against the grammar of ADR-0046, the semantics of ADR-0047 and the amendments of ADR-0052, which this re-expression is what found. Declarations here are current; the surrounding prose records how the study reached them.
## 1. Why this case

Every earlier case had a single actor per transition. Approval is the case where a transition's guard is satisfied by **other actors having acted**, in a stated number, order or role, and where that satisfaction can be **invalidated by a later edit**. Delegation asks where authority comes from and whether the store must know. A proposal asks what happens when a caller lacks authority: an error, or a pending request someone else can carry. Bookings add **interval conflicts**, the one invariant shape that is easy to state and hard to enforce concurrently, and future-dated state, which the first consumer explicitly deferred.

## 2. Approvals

### 2.1 Mapping

| Approval concept | In this model |
|---|---|
| An approval | a **part** of the approved object: `Approval { approver, principal, decision, kind, comment, event }` created by an `approve` or `reject` action on the parent (ADR-0016, ADR-0019) |
| Who may approve | actor guards on the action: `actor.has(APPROVE_PURCHASE)` (ADR-0025) |
| Separation of duties | `actor.id != requested_by.id`; `none(a in approvals where a.approver == actor.id)`. Under three-valued logic an erased requester makes the first clause unknown, so it fails rather than passing (ADR-0047) |
| "Needs approval" gate on the real transition | a guard counting valid approvals: `count(a in approvals where a.decision == APPROVED and not changed_since([amount, vendor], a.event)) >= 1` (ADR-0035, ADR-0047) |
| N-of-M | `>= N` |
| All of a set | one guard per required `kind` |
| Sequential chain (manager, then finance) | the finance `approve` action is guarded on a valid manager approval existing |
| Threshold (director above an amount) | `amount > 10000 implies any(a in approvals where a.kind == DIRECTOR and …)` (ADR-0032) |
| Approval invalidated by a material edit | the same guard, through `changed_since`: an approval older than the last change to the declared relevant attributes does not count (ADR-0035) |
| Withdraw request | an ordinary transition by the requester |
| Approval expires; escalate after N days | queried by filtering the stored `approval.at` against the supplied time, since a `now`-dependent derived attribute is not itself indexable (ADR-0048); escalation is a scheduler asking the availability query (ADR-0022) |
| Remedy when approval is missing | `delegable`, naming the capability and kind required, not a person |

### 2.2 A purchase request, declared

```text
PurchaseRequest: DRAFT → SUBMITTED → APPROVED → ORDERED | REJECTED | WITHDRAWN

  submit: DRAFT → SUBMITTED
    guards: actor.id == requested_by.id                                    [delegable]

  approve: SUBMITTED → SUBMITTED                              an action (ADR-0016)
    inputs: kind (enum ApprovalKind), comment (string, optional)
    guards:
      actor.has(APPROVE_PURCHASE)                                          [delegable]
      actor.id != requested_by.id                                          [delegable]
      none(a in approvals where a.approver == actor.id
                            and not changed_since([amount, vendor], a.event))
                                                                           [delegable]
      inputs.kind == DIRECTOR implies actor.has(APPROVE_DIRECTOR)                [delegable]
    outcome:
      create Approval.record(approver  := actor.id,
                             principal := actor.principal,
                             kind      := inputs.kind,
                             decision  := APPROVED,
                             comment   := inputs.comment,
                             event     := this_event)

  mark_approved: SUBMITTED → APPROVED
    guards:
      any(a in approvals where a.kind == MANAGER and a.decision == APPROVED
                           and not changed_since([amount, vendor], a.event))
                                                                           [delegable]
      amount > 10000 implies any(a in approvals where a.kind == DIRECTOR
                                            and a.decision == APPROVED
                                            and not changed_since([amount, vendor], a.event))
                                                                           [delegable]

  edit: SUBMITTED → SUBMITTED                                 an action (ADR-0042)
    inputs: amount (money, optional), vendor (reference Vendor, optional)
    guards: actor.id == requested_by.id                                    [delegable]
    outcome:
      amount := inputs.amount              skipped when not supplied (ADR-0052)
      vendor := inputs.vendor
```

Editing `amount` after a manager approved does not delete the approval and needs no reset in `edit`; `mark_approved` simply stops being available, and the verdict says which approval is now stale. That is ADR-0009's argument again: the rule is stated once, at the type, not repeated in every editing action.

## 3. Delegation

Authority arrives in the actor descriptor (ADR-0025); the consumer's authentication computes it. A manager on leave who delegates to a deputy is, to the store, a deputy whose descriptor now carries `APPROVE_PURCHASE` for two weeks, with `principal` set to the manager. Attenuation — the deputy may approve only up to a limit — is a descriptor attribute a guard reads: `amount <= actor.attributes.approval_limit`.

If the delegation itself must be governed and recorded, it is modelled as an object type in the store — `Delegation { delegator, delegate, capability, limit, from, until }` with a lifecycle `active → revoked | expired` — and the consumer's auth reads live delegations when computing descriptors. The store offers no delegation mechanism of its own; that pattern is enough (ADR-0036).

## 4. Proposals

A caller without authority for a transition receives `delegable`. Sometimes the right response is to carry the request to someone who has it. **ADR-0036**: `Proposal` is a built-in object type — the target object, the transition, the inputs, the proposer, and a lifecycle `pending → executed | rejected | withdrawn | expired`. A transition opts in with `proposable`; a `delegable` verdict on a proposable transition says so. An authorised actor's `approve` on the proposal executes the recorded request as that actor, with the proposal's approval event as cause; every non-actor guard is re-evaluated then, and if one fails the proposal stays `pending` carrying that verdict. The proposer is recorded on the proposal, not as principal of the execution: the approver is not acting on the proposer's behalf, they are authorising.

The first consumer's list of transitions where authority differs from capability has two entries, so `proposable` is opt-in and off by default.

## 5. Bookings

### 5.1 Mapping

| Booking concept | In this model |
|---|---|
| Resource (room, robot, vehicle) | an object type |
| Booking for an interval `[start, end)` | an object referencing the resource; lifecycle `requested → confirmed → active → returned → closed`, plus `cancelled` and `no_show` — the first consumer's engagement (`SCHEDULED → OUT → RETURNING → CLOSED`) is one |
| No two live bookings of one resource overlap | a type-level invariant: `none(b in Booking where b.resource == resource and b.state.category == live and state.category == live and b.start < end and b.end > start)`. A type-scan ranges over every *other* object, so there is no `id` exclusion (ADR-0061); the `live` filter is applied to both sides, which is what makes it **symmetric** under the swap test ADR-0052 requires so the affected set is computable. Concurrent safety comes from serialisable isolation (ADR-0039); on PostgreSQL it also compiles to an exclusion constraint, which is an optimisation, not the guarantee (ADR-0041) |
| "Is it free from X to Y?" | the check operation on the read surface: evaluate `book(resource, start, end)` without executing (DESIGN.md, `validate(object, intent)`) |
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

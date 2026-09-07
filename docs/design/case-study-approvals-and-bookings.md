# Case study: approvals, delegation, proposals, and bookings

Status: design iteration 5, 2026-09-08. Companion to the earlier case studies. Decisions taken here are ADR-0035 and ADR-0036, all pending author review. The shapes are the common ones — purchase-request approval, document review, leave requests, room and equipment booking — and the first consumer's own engagement axis (`wr:docs/adr/0002`) is a booking system it has not yet built.

## 1. Why this case

Every earlier case had a single actor per transition. Approval is the case where a transition's guard is satisfied by **other actors having acted**, in a stated number, order or role, and where that satisfaction can be **invalidated by a later edit**. Delegation asks where authority comes from and whether the store must know. A proposal asks what happens when a caller lacks authority: an error, or a pending request someone else can carry. Bookings add **interval conflicts**, the one invariant shape that is easy to state and hard to enforce concurrently, and future-dated state, which the first consumer explicitly deferred.

## 2. Approvals

### 2.1 Mapping

| Approval concept | In this model |
|---|---|
| An approval | a **part** of the approved object: `Approval { approver, principal, decision, kind, comment, event }` created by an `approve` or `reject` action on the parent (ADR-0016, ADR-0019) |
| Who may approve | actor guards on the action: `actor.has(APPROVE_PURCHASE)` (ADR-0025) |
| Separation of duties | `actor.id != requested_by.id`; `none(approvals where approver == actor.id)` |
| "Needs approval" gate on the real transition | a guard counting valid approvals: `count(approvals where decision == approved and not changed_since(relevant, approval.event)) >= 1` (ADR-0035) |
| N-of-M | `>= N` |
| All of a set | one guard per required `kind` |
| Sequential chain (manager, then finance) | the finance `approve` action is guarded on a valid manager approval existing |
| Threshold (director above an amount) | `amount > 10000 → any(approvals where kind == director and …)` (ADR-0032) |
| Approval invalidated by a material edit | the same guard, through `changed_since`: an approval older than the last change to the declared relevant attributes does not count (ADR-0035) |
| Withdraw request | an ordinary transition by the requester |
| Approval expires; escalate after N days | derived `stale := now - approval.at > 30 days`; escalation is a scheduler asking the availability query (ADR-0022) |
| Remedy when approval is missing | `delegable`, naming the capability and kind required, not a person |

### 2.2 A purchase request, declared

```text
PurchaseRequest: draft → submitted → approved → ordered | rejected | withdrawn
  relevant := [amount, vendor, lines]                      attributes whose change invalidates approval

  submit: draft → submitted            guard actor.id == requested_by.id
  approve(kind, comment): submitted → submitted             an action
    guards: actor.has(APPROVE_PURCHASE); actor.id != requested_by.id
            none(approvals where approver == actor.id and not changed_since(relevant, event))
            kind == director → actor.has(APPROVE_DIRECTOR)
    outcome: create Approval(approver := actor.id, principal := actor.principal, kind := inputs.kind,
                             decision := approved, comment := inputs.comment, event := this_event)
  mark_approved: submitted → approved
    guards: any(approvals where kind == manager and decision == approved and not changed_since(relevant, event))
            amount > 10000 → any(approvals where kind == director and decision == approved
                                               and not changed_since(relevant, event))
  edit_lines(lines): submitted → submitted   guard actor.id == requested_by.id   outcome lines := inputs.lines
```

Editing `lines` after a manager approved does not delete the approval and does not need a reset in `edit_lines`; `mark_approved` simply stops being available, and the verdict says which approval is now stale. That is ADR-0009's argument again: the rule is stated once, at the type, not repeated in every editing action.

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
| No two live bookings of one resource overlap | a type-level invariant (ADR-0009): `none(Booking where resource == this.resource and state in (confirmed, active) and start < this.end and end > this.start and id != this.id)`; concurrently safe because interval exclusion per key is a database constraint (ADR-0023 rule 3) |
| "Is it free from X to Y?" | the check operation on the read surface: evaluate `book(resource, start, end)` without executing (DESIGN.md, `validate(object, intent)`) |
| Capacity bookings (10 seats) | a quantity, as stock (ADR-0032) |
| No-show after start plus grace | `no_show: confirmed → no_show, guard start + 15 min <= now`, requested by a scheduler (ADR-0022) |
| Recurring series | a `Series` object; instances are bookings the consumer generates and creates, each referencing the series |
| Waitlist | a queue of `Waitlist` objects; who is promoted on a cancellation is a **selection** the consumer makes (ADR-0007) |
| Business hours, local time | timestamps are absolute; calendar rules are consumer inputs |

### 5.2 What held

Nothing new was needed. The interval invariant is a type-scan predicate (ADR-0021) with comparisons; its concurrent enforcement is the database-constraint path ADR-0023 already names, and the implementation note is that declared invariants of known shapes — uniqueness, and interval exclusion per key — should be compiled to constraints rather than to serialisable retry.

## 6. Rare cases, recorded in `edge-cases.md`

- Approving several objects in one decision.
- An approver who loses authority after approving.
- Four-eyes across different objects.
- A proposal whose inputs the approver wants to amend.
- Editing a recurring series after instances exist; waitlist promotion; local-time rules; substitution bookings ("any robot of this model").

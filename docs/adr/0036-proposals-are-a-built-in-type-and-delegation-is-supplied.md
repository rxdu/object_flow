# ADR-0036: Proposals are a built-in, opt-in type; delegation and attenuation arrive in the actor descriptor

- **Status:** Accepted — taken in autonomous design iteration 5 (2026-09-08); pending author review
- **Date:** 2026-09-08

## Context

TODO.md asked whether a denied transition should produce a pending proposal rather than an error, and whether delegation must attenuate authority. ADR-0025 settled the actor descriptor and deferred both. The first consumer's list of transitions where authority differs from capability has two entries. Case study: `docs/design/case-study-approvals-and-bookings.md` §3–§4.

## Decision

**Proposals.**

1. `Proposal` is a **built-in object type**: target object, transition, inputs, proposer, and a lifecycle `pending → executed | rejected | withdrawn | expired | invalidated` (ADR-0044 added the last, and made `expired` derived rather than driven).
2. A transition **opts in** with `proposable`. A `delegable` verdict on a proposable transition says the request may be proposed; on any other transition it does not.
3. An authorised actor's `approve` on a proposal **executes the recorded request as that actor**, with the approval event as cause (ADR-0019 lineage). Every non-actor guard is re-evaluated at execution; if one fails, the proposal stays `pending` and carries the verdict. The proposer is recorded on the proposal, not as the execution's principal: the approver authorises, they do not act on the proposer's behalf.
4. `expired` is time-driven through the availability query (ADR-0022).

**Delegation.**

5. The store has **no delegation mechanism**. Delegated authority is a capability in the delegate's descriptor with `principal` set to the delegator; attenuation is a descriptor attribute a guard reads (`amount <= actor.attributes.approval_limit`).
6. A consumer that must govern and record delegations models them as an object type — `Delegation { delegator, delegate, capability, limit, from, until }`, `active → revoked | expired` — and its authentication reads live delegations when computing descriptors. That is a pattern, and the case study shows it.

## Alternatives rejected

### Every denied transition becomes a proposal

Rejected: agents would generate proposals for everything they are not allowed to do, and the two-entry list says the need is narrow.

### Proposals as a consumer-declared type

Rejected: a proposal's target transition is a value chosen at submission, and outcomes cascade only declared transitions (ADR-0019); executing a recorded request needs a built-in.

### Delegation inside the store

Rejected: authority is computed by the consumer's authentication (ADR-0025); a second authority system inside the store would have to be reconciled with the first.

## Consequences

- TODO.md's delegation-and-proposals item is closed.
- `Proposal` joins `Subscription` (ADR-0034) as a built-in type; neither is user-definable.
- The verdict taxonomy is unchanged; `delegable` gains a detail flag.

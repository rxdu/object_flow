# ADR-0070: An authority list is not mandatory at a closed state, and the boundary worth naming is the undo

- **Status:** Accepted — recommendation accepted by the author 2026-09-08
- **Date:** 2026-09-08
- **Refined by:** ADR-0096 — `closed` is built into every vocabulary, and says when work is open and when it completes.
- **Refines:** ADR-0020
- **Answers:** open question 1

## Context

The question asked whether `only via` should be mandatory on any transition entering a closed-category state, on the reasoning that this is where authority matters most.

It is not, and the first consumer says so twice over.

**Its own machine has requestable transitions into closed states that must stay requestable.** `retire` into `RETIRED` and `cancel` into `CANCELLED` are ordinary operations gated by a capability, not consequences of another object's transition. Making the list mandatory would force each to invent a parent it does not have.

**And the boundary its authority actually tracks is a different one.** `wr:docs/design/procurement-error-recovery.md` states the organising principle as "correct freely before it's real; gate the undo of a committed decision", with three commitment boundaries. The admin-only permission `PROCUREMENT_CANCEL` gates exactly two operations, and both are undos of a crossed boundary: cancelling a placed order (`wr:app/api/procurement.py:242`) and reverting a committed intake batch (`wr:app/api/intake_batches.py:312`). Every other correction in the same workflow sits on the ordinary create permission. That system also runs a second authority axis on the state machine itself, `required_role="ADMIN"` on cancel and revert transitions (`wr:app/core/state_registry.py:778,834`), separate from its route permissions.

So entering a closed state is not where authority concentrates. Leaving one, or undoing something that has become real, is.

## Decision

`only via` stays optional. **Publishing reports** which transitions enter a closed-category state without an authority list, so the choice is visible at the moment it is made rather than mandatory.

No new construct for the commitment boundary. The observation is recorded because it is the shape a consumer's authority takes, and a future question about it should start from evidence rather than from the closed-state framing this question used.

## Alternatives rejected

- **Make it mandatory.** Rejected: it breaks two transitions in the first consumer's own lifecycle and would be satisfied by inventing parents, which makes the declaration less honest rather than more governed.
- **Add a marking for a commitment boundary.** Rejected as premature. One consumer names the concept and gates two operations with it; a capability guard already expresses it, and the model does not need a second vocabulary for the same thing until a second consumer asks.

## Consequences

- The publish report gains a line, joining the others that name a choice rather than refusing it.
- `design/edge-cases.md` records that authority in practice attaches to undoing a commitment, which nothing in the model currently distinguishes from any other transition.

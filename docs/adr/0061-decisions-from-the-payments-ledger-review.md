# ADR-0061: Ten decisions from the payments-ledger review

- **Status:** Accepted — repair of D114–D133, 2026-09-08; pending author review
- **Refines:** ADR-0021, ADR-0032, ADR-0047, ADR-0050, ADR-0052, ADR-0056, ADR-0058, ADR-0060
- **Date:** 2026-09-08

## Context

The third review of the declaration syntax modelled a payments ledger: money movement with authorisation, capture, settlement, refund, chargeback and reversal. It is the first domain reviewed that is high-volume, short-lived and money-carrying, and every previous case study was the opposite on all three counts. The design was built from a robotics inventory outward, and `docs/LESSONS.md` already records what a boundary frozen on an atypical first consumer costs.

Two findings were not defects in the design but in the state of the record: the checker rejected, in both available spellings, the abstract-base composition that §3.2 and §4.2 recommend by name. Prose and tool were drifting in opposite directions on one construct, which is worse than either being wrong alone, because a reader cannot tell which is authoritative.

## Decisions

### 1. A type-scan invariant ranges over every *other* object

The object being written is never in the scanned set. Uniqueness over two attributes could not be written at all before this: the obvious form matched the object against itself and was false for every object ever created, and the `id != this.id` clause that repaired it is an inequality, which is not one of ADR-0052's symmetric shapes. One rule removes both problems and removes a clause every author would otherwise have had to write.

### 2. `unique with <attr>, …`

The compound form. An idempotency key is unique per caller and not globally, which is the shape `unique`, `unique in scope` and `unique where` could not express.

### 3. A guard clause may be a negated evaluator call

§8.3 already prescribed writing a conditional external check as two transitions with opposite guards. The opposite guard could not be written, so a decline could be gated on nobody having asked the card network and never on the network having said no. A stale verdict is still refused as `temporal` either way, since `not stale` is not `satisfied`.

### 4. `money(ccy)` has the currency's minor-unit scale, and scalar arithmetic rounds half to even

Two for `USD`, zero for `JPY`, three for `BHD`. The document went out of its way to state that integer division truncates *because* money is the trap that would create, and then left money's own arithmetic undefined. Half to even rather than half up because it does not bias a long run of roundings upward, which is the property that matters when the roundings are a ledger. Exact allocation of an amount into parts that must sum back is named as not expressible: it is a decision about who absorbs the remainder, and that belongs to the consumer.

### 5. A terminal transition is a `do` whose to-state is terminal

A creation into a terminal state is not one. The cascade coverage rule turns on this, and a type whose objects are born final — a ledger entry, an audit record — has no terminal transitions, so the rule asks nothing of its wholes. Without the definition, such a part had to be marked `survives`, which §3.2 calls a modelling error, or given a fictitious lifecycle nobody would ever drive.

### 6. `if` is allowed in the value expression of an outcome step

Still not in a guard, where a conditional would hide which clause failed. A debit and a credit differ by a sign; with `if` confined to derivations that one two-valued choice split the writing transition, then its caller, then every `only via` list naming them. Straight-line outcomes are about control flow, and a conditional value is not control flow.

### 7. A family member satisfies its base's type, and not the reverse

Stated where it had been assumed. A machine shared by two types passes `this` to something declared over their common base, which is the ordinary shape of a shared lifecycle.

### 8. The terminal-state skip belongs to a cascade, not to a `call`

A `call` to a transition its target cannot take fails. Under the other reading an erasure's `call c.forget(…)` silently skipped a part already disposed of, leaving personal data in place while the erasure reported success and check 39 still passed.

### 9. A sequence scope may name an indexed attribute

A tenant is often an identifier mirrored from another system rather than an object in this store, and requiring a reference meant inventing a type solely to be scoped by.

### 10. `tracking` is inherited, and an abstract type needs none

It was stated twice in a family with nothing checking that the two agreed, on a type that has no objects to track.

## Alternatives rejected

- **Let a type-scan include the object being written and require the exclusion clause.** Rejected: it is a clause on every uniqueness invariant, it is the kind of thing that is forgotten once and then wrong forever, and ADR-0052's symmetry rule rejects it anyway.
- **Leave money rounding to the backend.** Rejected: two backends would disagree, and the disagreement surfaces as a reconciliation break months later rather than as an error.
- **Allow `not` on any verdict operand rather than on a whole guard clause.** Rejected: it reopens mixing a verdict with boolean logic, which ADR-0049 closed so that an external consultation stays one identifiable thing on the event.
- **Add a sixth remedy class for an expired window.** Rejected: `unreachable_from_here` already means it, and the actual defect was that inference silently chose `temporal` and the publish report trusted the inference. The report now uses the declared class.
- **Make the checker accept `only via <Base>.<transition>`.** Rejected: an abstract base declares no transitions, so the name would resolve to nothing. The subtype spelling is the correct one and the checker was wrong to reject it.

## Consequences

- `docs/design/declaration-syntax.md` is at iteration 13, and §3.2 carries a worked example of the abstract-base composition so the checker regression-tests the pattern rather than the prose merely recommending it.
- Three checker defects fixed: check 11 resolves an abstract whole to its family, the top-level `cascade <part>` clause counts as a call site, and a wrapped `provides capability` line is read whole.
- Four findings are not resolved here and became open questions 9 to 12, all since answered by the author: runtime currency refused (ADR-0068), the money counter deferred (ADR-0072), the family form agreed but not built (ADR-0072), and the third tracking mode added (ADR-0067). They were: money with a runtime currency, a counter that holds money and may go negative, an `only via` form that names a family, and whether `serial` and `quantity` are the right two tracking modes. The last reopens ADR-0050 and the first three are each a model change, so all four are the author's to rule on.
- The payments domain should become a case study in `docs/design/`. It is the first that is high-volume and short-lived, and three of the four open questions above exist only because nothing in the record was shaped like it.

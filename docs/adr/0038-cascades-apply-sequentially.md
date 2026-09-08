# ADR-0038: Cascaded transitions apply sequentially; each sees the writes of those before it

- **Status:** Accepted — repair of D01, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Supersedes:** the "All guards first" rule of ADR-0019

## Context

ADR-0019 required every guard in a request, the parent's and each cascade's, to be evaluated before anything was written. Defect D01 (`docs/design/defects.md`) established that the rule breaks three things, two of which are the scenarios the cascade mechanism was introduced to serve:

1. `IntakeBatch.commit` cascades `inventorize` then `reserve`. Reserve requires the unit available, which only the first cascade produces. Against a pre-write snapshot its guard always fails. This is ADR-0019's own motivating example.
2. Order placement cascades a reservation per line, guarded on `on_hand - reserved >= inputs.qty`. Two lines on one product both read the pre-write value, both pass, both apply. A single order oversells itself.
3. Repeated writes to one attribute from one snapshot have undefined accumulation. `reserved := reserved + qty` applied twice from zero is either 12 or a lost update to 6.

The rule was chosen so that a request could fail before any write. That property was worth having, but it was bought at the price of correctness, and it was not needed: everything happens in one transaction, so nothing is observable outside the request until commit.

## Decision

1. **Sequential application.** The parent's guards are evaluated first. Then, depth-first in declaration order, each cascaded transition's guards are evaluated **against the state produced by everything applied before it**, and its outcome is applied immediately.
2. **Any failure aborts the whole request.** The transaction rolls back, nothing is committed, and the verdict names the failing object, transition and guard. The guarantee is *fail before commit*, not *fail before write*, and outside the transaction the two are indistinguishable.
3. **Iteration order is defined.** A cascade over a relationship applies to its elements in ascending object-id order. A request is therefore reproducible, and a declaration's behaviour does not depend on storage order.
4. **Accumulation is read-modify-write.** An outcome write reads the current value at the moment it is applied. Two cascaded increments from zero yield twelve, not six.
5. **Applying a transition twice to one object in one request is not special-cased.** The second evaluation sees the first's result and fails if its guards no longer hold. Two delivery slots bound to one unit therefore block completion rather than selling it twice, which is the correct outcome for what is a modelling error.
6. **`check` simulates the same sequence** (ADR-0037) so that the verdict it returns is the verdict a real request would receive.

## Alternatives rejected

### All guards first (the superseded rule)

Rejected on the evidence above: it makes the flagship cascade inexpressible and admits an oversell inside one request.

### Forbid cascades that depend on each other's writes

Would preserve the old rule by declaration-time analysis. Rejected: the dependency is the point. Auto-fill on receipt exists precisely because inventorising a unit is what makes reserving it legal, and the two must be one atomic step.

### Two passes, guards then outcomes, with guards evaluated against a projected state

Rejected as the worst of both: it requires simulating every outcome to build the projection, which is sequential application with an extra copy, and it reintroduces the proposed-state semantics ADR-0020 rejected for guards.

## Consequences

- ADR-0019's Execution section is amended: "All guards first" is replaced by this ADR. Its other rules stand.
- DESIGN.md §6 becomes a single interleaved loop rather than a guard phase followed by a write phase.
- Guards can now observe states that no caller could have requested directly, within one atomic step. That is intended and is what makes a two-stage cascade expressible.
- Deep cascades do more work before discovering a failure. Acceptable: the work is discarded by the same rollback that already covered a late invariant violation.
- D01 is resolved.

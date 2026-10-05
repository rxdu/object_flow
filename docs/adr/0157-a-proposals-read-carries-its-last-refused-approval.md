# ADR-0157: A proposal's read carries its last refused approval

- **Status:** Decided by the author, 2026-10-05 ("go with option 1"), given two options for how a read of a pending proposal returns the verdict its refused approval left attached.
- **Date:** 2026-10-05
- **Refines:** ADR-0044 decision 4 (a guard failure leaves the proposal pending, with the verdict attached), ADR-0147 (a refused approval is an attempt row naming the proposal, and the proposal gets no event)
- **Relates to:** `DESIGN.md` §9; `design/library-api.md` §5; `design/storage-schema.md` §4 (`t_proposal`), §6 (`of_attempt`)

## Context

`DESIGN.md` §9 says an approval its request refuses "leaves the proposal pending: it is one attempt row of the proposed transition, on its object, carrying the clause and naming the proposal, and the proposal gets no event, the verdict attached being that row, which a read of the proposal returns". Building the core's attempt log on 2026-10-05 wrote that row, and found that `library-api.md` §5 gives no field by which `get` of a proposal returns it. `storage-schema.md` §4 gave `t_proposal` a column, `last_verdict`, "the verdict that left it pending (ADR-0044)", which predates ADR-0147: writing it would change the proposal's row without an event, which the refused approval does not record.

## Decision

**A read of a proposal carries `last_verdict`**: the verdict, clause and remedy of the latest approval its request refused, and when it was refused, read from that approval's attempt row; absent until an approval is refused. It is computed on read, as a derived attribute is, and never stored, so the proposal's row changes only by its own events. `t_proposal.last_verdict` is removed.

## Alternatives rejected

- **A separate read of a proposal's attempts.** It adds an operation to the read surface for one value, and a caller deciding whether to approve again reads the proposal anyway.
- **Storing the verdict on the proposal's row.** The refused approval records no event on the proposal, so the write would be a change to a governed row with no event, which no other write path makes.

## Consequences

- `library-api.md` §5 says a proposal's `get` carries `last_verdict`; `storage-schema.md` §4 drops the column; `DESIGN.md` §9 names the attribute.
- The core derives it in `reads::get` from the attempt log, and the service's `get` returns it with the proposal's attributes.

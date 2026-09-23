# ADR-0089: The log is appended complete, and read by a cursor that cannot skip a late commit

- **Status:** Accepted by the author, 2026-09-23, after a check of every decision against the design — decided at the author's direction, against `docs/PRD.md` T3, M5 and D1; repairs D210 and D211; the PostgreSQL mechanism observed by probe
- **Date:** 2026-09-23
- **Refines:** ADR-0034, ADR-0046, ADR-0077

## Context

**D210.** `pull` promised a settled position — "the highest position below which no transaction is still in flight" — and nothing in the schema could compute it.

Probed 2026-09-23 on PostgreSQL 16: transaction A allocated position 1 and stayed in flight; transaction B allocated position 2 and committed. The highest visible position was then 2. A consumer acknowledging it would never see position 1, which failed the export PRD M5 asks for and every pull subscriber.

**D211.** ADR-0046 said an event's content is "completed at commit" after its identity is allocated. `storage-schema.md` said no row of `ok_event` is ever updated. Both cannot be true, and T3's permanent history depends on the second.

## Decision

### 1. An event row is inserted once, complete

Its position is allocated when its transition begins applying, so `this_event` is known to the outcome (ADR-0046):
- **on PostgreSQL** from the log's sequence with `nextval`;
- **on SQLite** from a counter row the transaction increments. The transaction holds the write lock from its first statement (ADR-0090), so no other writer can interleave.

The row is inserted when the transition's outcome is done, in the same transaction. Nothing updates a row of `ok_event` afterwards, erasure's redaction excepted. The append-only statement is then true.

### 2. Each event records its writing transaction

On PostgreSQL that is the transaction id, `pg_current_xact_id()`, an `xid8`. On SQLite it is a store-assigned number, monotonic because writers are serial.

### 3. The log is read by a settled cursor, ordered by transaction and then position

A reader takes its snapshot's `xmin`. It returns only events whose transaction id is below it, which are all finished, in (transaction id, position) order, and hands back the last pair it returned as the cursor.

Probed: while A was in flight, that query returned nothing, because B's transaction id is above the snapshot's `xmin` too. After A committed it returned both, A first. A later commit therefore always sorts after any cursor already handed out.

On SQLite, transaction order is commit order and the cursor is the position.

### 4. Per-object order survives the cursor order

Under serialisable isolation, a transaction cannot write an object that another committed after its snapshot (ADR-0090's probe). So a later writer of an object took its snapshot after the earlier writer committed, and its transaction id is the higher of the two. This is reasoned from that probe, and ADR-0091's harness keeps it tested.

### 5. `pull` returns the settled cursor, and so does the export

`pull(subscription, cursor)` returns that cursor, and a subscriber acknowledges it. The export of ADR-0094 pages the same way.

## Alternatives rejected

- **A single counter row allocating positions in commit order.** It serialises every commit through one row, which ADR-0034 rejected by name.
- **The oldest in-flight transaction's age**, as `storage-schema.md` §7 had it. It subtracts a duration from a position, and no reader can see another transaction's uncommitted rows to measure it.
- **Insert early and update at commit.** Every reader of the log must then treat a row as provisional, and the append-only property, which an auditor relies on, becomes false.
- **Logical decoding or `LISTEN`/`NOTIFY` for delivery.** Both are PostgreSQL-only, and the second loses messages across a disconnect. The cursor works on both backends from ordinary reads.

## Consequences

- `DESIGN.md` §7 replaces the settled position with the settled cursor.
- `storage-schema.md` gives `ok_event` a `txn` column, drops the early insert, and adds SQLite's counter row.
- `library-api.md`'s `EventPage.settled` becomes a cursor, and `pull` and `acknowledge` take one.
- ADR-0046's "completed at commit" and ADR-0034's position window are annotated.
- D210 and D211 are resolved.

# ADR-0225: The sequence store is the store's own table, written by its one writer

- **Status:** **Decided** 2026-10-21 under the author's direction ("go with your recommendations", on recording the build).
- **Date:** 2026-10-21
- **Refines:** ADR-0076 decision 1 (the sequence store a second connection, its own file on SQLite)
- **Relates to:** ADR-0029, ADR-0122 decision 8 (a sequence is monotonic and not gapless, a refused request's draws not reused); ADR-0178 and ADR-0213 decision 4 (one writer); ADR-0207 decision 3 (a restore's floor); `storage-schema.md` §6

## Context

ADR-0076 put the sequence store on a second connection, so a number drawn by a request survives the request's rollback without the request's transaction holding it: on PostgreSQL to the same database, on SQLite to a separate file beside the store, since a second connection to the store's own file waits behind the request's write lock. A SQLite store became two files, backed up in order, the store first.

The build drew sequences differently from its first slice (`d2327f9`, 2026-10-05), and no decision recorded it: `of_sequence` is a table of the store on both backends. A request reads every sequence's last number with the store's counters, draws in memory, and writes its draws in its own transaction where it applies; where it is refused, the transaction rolls back and its draws are written after, in a short transaction of their own, or on a savepoint of their own in a group of requests (ADR-0208). Checking `storage-schema.md` against the build on 2026-10-21 found the difference.

What lets the store take its draws itself is that one writer writes one transaction at a time, a service's writer holding the store (ADR-0178) and one instance writing a PostgreSQL store (ADR-0213 decision 4): no other request can draw between a refusal's rollback and the write of its draws.

## Decision

1. **`of_sequence` is a table of the store, on both backends, written by its one writer**: in the request's transaction where the request applies, and after its rollback where it is refused. A sequence stays monotonic and not gapless, a refused request's draws not reused (ADR-0029, ADR-0122 decision 8).
2. **A SQLite store is one file and its log.** A backup copies the store alone, and a restore skips the numbers the live store may have drawn after the copy (ADR-0207 decision 3), which the second file's ordering had served.
3. **The one window, stated:** a crash after a refusal's rollback and before its draws are written loses those draws, and the next request draws the same numbers again. No object holds them, the refused request having made none, so no identifier is duplicated; only the gap the refusal would have left is not left.
4. **ADR-0076's second connection is kept as the design for several writing instances**, which ADR-0213 decision 4 leaves to later work: with writers racing, a request's draws must commit apart from its transaction, and the second connection is how.

## Alternatives rejected

- **Building ADR-0076 as decided**: a second connection and, on SQLite, a second file to open, back up and restore in order, for a race one writer cannot have.
- **Writing a refusal's draws before its rollback, on a savepoint kept through it**: SQLite and PostgreSQL both roll a savepoint back with the transaction that holds it, so the draws would be lost with the refusal.

## Consequences

- `storage-schema.md` §6: its note on the sequence store as built cites this decision. ADR-0076 carries a Refined-by line.
- No code changes: the decision records what `store.rs` does (`Allocator::read`, `write_sequences`). Its existing tests hold it: a refused request's draws kept (the core's sequence tests since `d2327f9`) and a restore's floor (ADR-0207's).

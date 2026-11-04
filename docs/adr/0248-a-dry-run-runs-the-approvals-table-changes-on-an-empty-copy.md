# ADR-0248: A dry run runs the approval's table changes on an empty copy, and on PostgreSQL counts their locks

- **Status:** **Decided** 2026-11-04 under the author's direction ("go with your recommendations"), on the recommendations given the same day with the measured cost, for issue #59's second ask and for the lock-table defect found measuring it. Decision 2 is a notice where the recommendation said a refusal, for the reason it gives; the deviation is the author's to confirm. Built the same day.
- **Date:** 2026-11-04
- **Refines:** ADR-0195 decision 1 (a change's dry run runs under its slot and its deadline, its impact on a read); `publish-and-import.md` §1 step 3 (the dry run)
- **Relates to:** issue #59; ADR-0213 (PostgreSQL in the release); `docs/research/dry-running-table-changes.md`

## Context

Issue #59, filed 2026-11-03 at 0.12.0, asked that a dry run catch what its approval faults on. The dry run simulates the mappings in memory and runs no table change, so a version whose approval sent a statement the database refused passed it, and its approval answered `500`. That case was fixed in 0.13.0. The second ask, a dry run that runs the approval's statements, was decided 2026-11-03 to be measured first (`TODO.md`, #59).

Tools that catch a refused statement before an apply run it on an empty copy: `pt-online-schema-change`'s dry run alters an empty copy, and Atlas plans on a dev database (the research, §2). A change applied to the store and rolled back would hold the writer and lock each changed table for as long as its rows take to copy (§3).

Measured 2026-11-04 with a prototype (`TODO.md`, #59): run on an empty copy, the approval's table changes refused 0.12.0's defect, planted back, on both backends, naming the statement. The copy cost grew with the tables a version changes, on SQLite faster than linearly, since each `ALTER` reads the whole schema. On PostgreSQL it held `ACCESS SHARE` on each store table it copied, which no write waits on.

The measurement found a second defect (`TODO.md`). On PostgreSQL at its defaults a version changing 600 types passed its dry run, and its approval failed `53200: out of shared memory`, installing nothing. The approval takes, in its one transaction, a lock on each table it changes and on each of its indexes and set tables. PostgreSQL's lock table "has space for `max_locks_per_transaction` objects (e.g., tables) per server process or prepared transaction", and "individual transactions can lock more objects as long as the locks of all transactions fit in the lock table"; the setting "can only be set at server start" (PostgreSQL 16 documentation, §20.12).

## Decision

1. **A change's dry run runs the approval's table changes on an empty copy of the tables they touch, and rolls the copy back.**
   - The statements are the approval's own: its changes before the mappings, then each changed table rebuilt to the version's exact shape, as the approval does after them (`store.rs`, `change_tables`, `rebuild`).
   - The types they touch are each new one and each whose table a rename reaches or the version changes. Each is copied with its own tables, a set's and a set-aside's among them, and the built-in tables the changes read and write.
   - They are taken 32 types at a time. On SQLite each batch is a database of its own in memory, made from the store's catalog, its keys held off as the approval holds them, so the copy holds no lock on the store. On PostgreSQL each batch is a schema of its own, `of_dry_copy_<pid>`, made on a connection of its own in one transaction rolled back, so the copy holds few locks at a time. The scratch schema is the only one on its path, the catalog a rebuild reads made in it, so a name the copy does not hold refuses the statement and reaches no table of the store's. A store table it copies is waited on for 5 s at most, past which the copy is not made.
   - A statement the copy refuses is fatal, `table_change:<Type>`, naming the statement, so the submit is refused `dry_run_clean` where its approval would fail.
   - A copy that cannot be made, its schema or a table of it refused, is a notice, `table_change_not_run`, and refuses nothing.
   - It runs only where nothing else is fatal, in every dry run, within the dry run's deadline, and never in the approval, which runs the statements for real.
2. **On PostgreSQL the locks the copy's batches take, summed, are the approval's lock need**, and past the lock table the server's settings size, `max_locks_per_transaction` for each of `max_connections` and `max_prepared_transactions`, a notice, `table_change_locks`, names the setting to raise.
   - It is a notice, not the refusal recommended, because the server's table holds more than its settings size it for. Measured 2026-11-04 at the defaults, sized for 6,400 locks: approvals holding 8,465 and 12,672 locks passed, and one of 200 types failed `53200`. A refusal at 6,400 would refuse versions that 0.13.0 approves, and no setting gives the larger figure.
   - An approval that fails so installs nothing (`storage-schema.md` §10).

## Alternatives rejected

- **The approval's statements applied on the store and rolled back.** No tool read uses it as a dry run, and it would hold the writer and lock each changed table for as long as its rows take to copy (the research, §3).
- **One copy of every table in one transaction.** On PostgreSQL a copy of 600 changed types exhausted the server's lock table, as its approval did, so a dry run could fill the table for every other session. On SQLite one copy of 1,205 tables took 126 s, where batches of fresh databases take 17 s for the whole dry run.
- **The copy on a read connection.** A read connection to PostgreSQL is read-only, so it runs no statement that makes a table.
- **The copy on the writer.** It holds the writer for the copy's length, which a submit's dry run was moved off for (DRYRUN-LOCK, `production-review.md`).
- **A refusal past the lock table the settings size.** It refuses approvals the server carries (decision 2).

## Consequences

- Built in the core's `store.rs` (`tables_on_empty_copy`, `change_tables`, a test's plant of 0.12.0's defect, and a lock table set smaller in a test), `db.rs` (`Connection::beside`), `world.rs` and `publish.rs`. The reference runner holds no tables and runs no copy.
- **Tested** on both backends: a planted 0.12.0 defect refused, naming its statement, and the version without it accepted, the store's tables, indexes and schemas as they were; a collision in the second batch of 40 types refused; and on PostgreSQL the lock notice past the table, none within it, and every batch's locks counted. Nine planted defects in the copy were each caught.
- **Measured** 2026-11-04, debug build, on this machine: a dry run renaming a column in each of 10, 100 and 600 types took 0.16, 2.4 and 17.3 s on SQLite, and 0.36 and 2.4 s for 10 and 100 on PostgreSQL. The copy's lock count was 93% of the approval's own at 100 and 150 types.
- A dry run on PostgreSQL holds one connection more while it runs (`operate.md`).
- **Isolation**, made after a review of the commit found the scratch schema first on a path that went on to the store's: a statement planted to read `of_declaration`, a store table no batch copies, was refused, where that path would have read the store's; every core test passes with the scratch schema alone on its path.

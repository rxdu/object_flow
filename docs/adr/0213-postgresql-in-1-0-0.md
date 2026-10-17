# ADR-0213: PostgreSQL in 1.0.0

- **Status:** **Decided** 2026-10-17 by the author ("we need postgresql in 1.0.0 for the first deployment"), option (a) of the question `TODO.md` recorded the same day. Decisions 2 to 7 are the recommendations given with it, which the author did not answer and set going ("iterate until the postgresql support is complete"); they are taken as recommended under that direction, and stand for the author to change. Decision 8, erasure's standard on PostgreSQL, which changes what property P promises, was decided by the author the same day as recommended ("go with your recommendation on erasure").
- **Date:** 2026-10-17
- **Supersedes in part:** ADR-0174 decision 2 (a deployment's store is one SQLite file)
- **Refines:** ADR-0204 §7 (the release gate); ADR-0001 (storage delegated to PostgreSQL, or SQLite for embedded use)
- **Relates to:** `storage-schema.md` (both backends' columns, §8's constraints, §9's erasure); ADR-0039, ADR-0089, ADR-0090 (serialisable isolation, PostgreSQL's contended rows); ADR-0076 (the sequence store); ADR-0207 (a restored copy); ADR-0208 (grouped commits)

## Context

The design names PostgreSQL as the store and SQLite for embedded use (ADR-0001, `DESIGN.md` §2), and `storage-schema.md` gives both backends' columns, isolation and constraints. The build made a deployment's store one SQLite file (ADR-0174 decision 2, which rejected PostgreSQL first on the first consumer's sizing), and wrote the core against SQLite directly: no layer stands between the engine and the database, 252 SQL calls in `store.rs` and `release.rs` and 71 SQLite constructs in `store.rs` alone (counted 2026-10-17). Beyond the dialect, erasure's removal of bytes, the one writer, the sequence store, backup and restore, the upgrade and the release's fingerprint are built on SQLite's behaviour, and every piece of the release's evidence, the audit, the harness, Gates P and I, was taken on SQLite.

The first deployment, the operations platform's, runs on PostgreSQL.

## Decision

1. **1.0.0 ships PostgreSQL as a deployment's store, and SQLite beside it**, for sandboxes, the Flow Studio, tests and small deployments; both are held to the same tests, the core's and the service's run on each.
2. **One store implementation over a connection seam.** The store's SQL is written once, over a connection with two implementations, SQLite's and PostgreSQL's, and a dialect for the constructs that differ; the engine above the store does not know which it runs on.
3. **The client is the `postgres` crate**, synchronous as the core is, connecting over TLS through rustls, which the tree already carries.
4. **One active writing instance**, writing one transaction at a time as the service does on SQLite, serialisable isolation the database's safety net beneath it (ADR-0039); availability from the database's own failover, the service restarted against the new primary. Several instances writing at once, transactions racing and retried (`DESIGN.md` §6), is later work.
5. **PostgreSQL 16 is the floor**, the version ADR-0089 and ADR-0090 probed; CI runs the suite on it.
6. **A first deployment starts empty.** Moving a store from SQLite to PostgreSQL is later work.
7. **Invariants stay the engine's on both backends**: a database constraint is an optimisation (`storage-schema.md` §8, ADR-0041), and 1.0.0 compiles on PostgreSQL only what it compiles on SQLite, no exclusion constraint.
8. **Erasure on PostgreSQL leaves the live database clean, and backups hold erased values until their bounded retention ends** (option (i)). SQLite's erasure leaves no erased byte in its file or log; PostgreSQL keeps dead row versions until VACUUM, and its log archives, replicas and backups keep copies. An erasure on PostgreSQL rewrites every copy the store holds, as on SQLite, then VACUUMs the store's tables and takes a checkpoint where its role may (`pg_checkpoint`), so no dead version of an erased value stays in the data files; a read open when the erasure committed keeps those versions, and the vacuum is retried after each write until none does, as SQLite's truncation of its log is. *Refined 2026-10-17 by a probe on PostgreSQL 16.8, where the recommendation had said "the live database clean" unmeasured: three values, one first written, one in the middle of a page and one stored out of line in TOAST, each found in its table's or TOAST's file before the erasure, were in neither after a VACUUM and a checkpoint, and each was still in one segment of the live write-ahead log, which keeps them until its segments are recycled.* The write-ahead log, and a backup or a log archive taken before the erasure, hold the erased values until they are recycled or their retention ends, which `operate.md` states as a limit, with the retention a deployment must bound. A replica holds what its primary holds until it has replayed the erasure and its vacuum.

## Alternatives rejected

- **1.0.0 on SQLite, PostgreSQL in 1.1** (option (b)): the first deployment needs PostgreSQL.
- **The seam carved and 1.0.0 shipped on SQLite** (option (c)): it ships no PostgreSQL.
- **Two store implementations behind a trait of the store's operations**: the store's logic written twice, about 7,000 lines, the two left to drift as D696 lived in two engines.
- **`tokio-postgres` or `sqlx`**: the core is synchronous (`rules/rust.md`), and `sqlx`'s checked queries need a database when the core builds.
- **Several writing instances in 1.0.0**: a concurrency model no evidence was taken under.
- **Erasure by VACUUM FULL of the tables written** (option (ii)): an exclusive lock on each table for the rewrite, and backups still holding the values.
- **Personal values encrypted under a key per subject, destroyed at erasure** (option (iii)), the one way to reach backups: no personal value could be indexed or compared in SQL, and a key store would be the deployment's to run. Recorded as later work, for a deployment whose backups must not outlive an erasure.

## Consequences

- The release gate (ADR-0204 §7) runs its soak and its confirming review on PostgreSQL, the audit's storage areas, durability, records and personal data, run again on it, and Gates P and I measured on it.
- CI runs a PostgreSQL 16 service beside the suite.
- `DESIGN.md` §2, `README.md`, `core/README.md`, `operate.md` and `integrate.md` say a deployment's store is PostgreSQL or SQLite, and ADR-0174 decision 2 is marked superseded in part.
- *Built 2026-10-17, blocks 2 to 5.* The store reaches its database through one facade, `db.rs`, its SQL written for SQLite and translated for PostgreSQL, SQLite's catalog, pragmas and JSON functions given to PostgreSQL in the connection's schema; the suite runs on both, CI with a PostgreSQL 16.8 service. `--store` takes a `postgresql://` URL or a connection string. Two choices the recommendations left open were made with it: **the password** is read from `--store-password-file` or `OBJECTFLOW_STORE_PASSWORD`, and one written in `--store` is refused, since a command line is shown to every user of the machine; **TLS** goes through `tokio-postgres-rustls` on rustls's ring provider with the platform's trust store, as the evaluator client has it, the client taking `sslmode` `disable`, `prefer` and `require` alone, `require` verifying the certificate and the host, and a start warning where a store on another host may run without it. The one writer holds an advisory lock on its schema for its connection's life; each read is a connection of its own, read-only; a restored database is marked as a file's copy is, and opened as a restore (ADR-0207). `operate.md` has a section, A store on PostgreSQL.

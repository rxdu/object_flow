# ADR-0090: Concurrency as it actually behaves on each backend

- **Status:** Accepted — decided 2026-09-23 at the author's direction, against `docs/PRD.md` N1, N3, F4 and T1; repairs D203, D204 and D213, each observed or reasoned as stated
- **Date:** 2026-09-23
- **Refines:** ADR-0039, ADR-0076

## Context

Three findings of the design review concern how the store behaves under concurrency (`design/defects.md`).

- **D204, observed on SQLite 3.37.2.** Under the default deferred `BEGIN`, a request that reads and then writes fails with `database is locked` if another commits between its read and its write. `library-api.md` §7 would raise that as `StorageUnavailable`: a fault where PRD F4 requires a verdict. It fails N1 on the backend N1 names.
- **D203, observed on PostgreSQL 16.8.** At serialisable isolation, a row lock does not make a contended row queue: the second request waits for the first, then fails with a serialisation error. ADR-0039 and four documents said it queues.
- **D213, reasoned.** The sequence mint's second connection, drawn from the request pool, can starve that pool.

## Decision

### 1. SQLite: every transition transaction begins `IMMEDIATE`

The write lock is taken at the first statement, so a request's guard reads and its writes are serialised with every other writer's. A contended request waits out the busy timeout rather than failing on a stale snapshot, which the probe showed is what `IMMEDIATE` does.

A busy result that still occurs, the timeout exceeded, is classified as a serialisation failure: retried within the bound, then `stale`. That makes it a verdict, as F4 requires, not a fault.

### 2. PostgreSQL: a hot row waits and then retries, and the design says so

The retry is what makes a contended request succeed. Row locks, taken as objects are reached, make the losing request fail at its first touch of the hot row rather than after doing its work. They do not queue it. ADR-0039 decision 3's "block instead of aborting" is corrected, and so is every document repeating it.

The cost is measured, not argued: each event records its retries (ADR-0083). At PRD N3's operations rate no mitigation is designed. If measurement ever shows a retry storm, the recorded next step is a session lock, taken before the transaction's snapshot, on objects known from the request. The probe shows why that is the one thing a lock inside the transaction cannot do.

### 3. The mint has a connection of its own

The sequence mint of ADR-0076 draws from a pool separate from the request pool. On SQLite it is a single dedicated connection to the sequence file. So a burst of creations cannot hold every request connection while waiting for a mint connection.

## Alternatives rejected

- **Classify SQLite's busy as `StorageUnavailable`**, as the design implied. It turns an ordinary race into an exception every caller must special-case, which F4 rules out.
- **Keep deferred `BEGIN` and retry the busy error.** The deferred transaction's snapshot is already stale, so the retry is the same work done twice. `IMMEDIATE` waits instead.
- **Drop row locks on PostgreSQL**, since they save no retry. They still end the losing request sooner, before it does work it will throw away.
- **A pre-snapshot lock now.** Nothing at N3's rate asks for it, and it covers only objects known before the transaction, not those a cascade reaches.

## Consequences

- `DESIGN.md` §6 says how each backend begins and what a contended request experiences.
- `storage-schema.md` §6 and §7 name the mint's pool and SQLite's `IMMEDIATE`.
- `edge-cases.md`'s hot-row entry and `case-study-orders.md` §4 are corrected.
- ADR-0039 decision 3 is annotated.
- The two probes are kept beside `scripts/check-schema-doc.py`'s sequence probe: the SQLite one runs there on every corpus run, and the PostgreSQL one is written down to run wherever a PostgreSQL is available.
- D203, D204 and D213 are resolved.

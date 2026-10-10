# ADR-0178: More than one request at a time

- **Status:** Built at the author's direction ("go ahead with the concurrency ADR and go according to your recommendations based on the design requirements and industrial practices", 2026-10-10), for the author's review.
- **Date:** 2026-10-10
- **Refines:** ADR-0174 (a deployment's service), ADR-0090 (contention on each backend), ADR-0175 decision 2 (a request's calls asked outside its transaction)
- **Relates to:** `DESIGN.md` §2, §6; `design/storage-schema.md` §7; `design/library-api.md` §4, §7; `design/procurement-second-review.md` §2

## Context

The service runs every operation under one lock around its one SQLite connection, reads included, so a publish or a large import holds every user, and a dashboard's reads queue behind a clerk's write. Both second-round procurement reviewers named it among what stands between a pilot and production, and the integrator measured a read waiting on a slow write before D563 took the evaluator calls out of the lock.

The design already says what concurrency must preserve, and the core does not yet do all of it:

- **Serialisable isolation** for a request's transaction, which makes a guard's reads safe (ADR-0039, `DESIGN.md` §6); on SQLite, a write transaction **begun `IMMEDIATE`**, a contended one waiting out the busy timeout, and a busy past it retried within a bound and then refused `stale`, cause `retries_exhausted`, remedy `temporal` (ADR-0090, `storage-schema.md` §7, `library-api.md` §4). The core begins its transactions deferred, has no busy timeout, and has no such verdict; one connection behind one lock never met the case.
- **A read sees one committed state.** The core's reads run statement by statement on the shared connection outside a transaction, which a lock made safe and concurrency would not.
- **Every operation reads the store's release** (ADR-0177) and records the declaration version it ran under.

Industrial practice for a server on one SQLite file is settled: write-ahead logging, which the store already uses, lets any number of readers run beside the one writer, each seeing the snapshot its transaction began on, so a server keeps **one write connection behind a lock and a pool of read connections**; writes stay one at a time because SQLite has one writer, and are queued in the process rather than left to the database's busy handler, which polls. A server whose writes must run in parallel moves to a database with row-level concurrency, PostgreSQL for this design (`DESIGN.md` §2).

## Decision

1. **A deployment's service has one writer and many readers.** Its write connection stays behind the lock every write takes: a request, a proposal's filing or decision, `import_batch`, `publish` (its dry run included), `maintain`, `acknowledge` and `upgrade`. Every other operation runs on a read connection of its own, each in one read transaction, so it sees one committed state, never waits for a write, and holds up none: `get`, `query`, `lookup`, `history`, `events`, `declaration`, `tools`, `availability`, `available`, `check`, `metric`, `pull`, `export`, `diagnostics` and `exceptions`. A long publish or import still holds the writes behind it, and no longer any read. A sandbox, one author's store in memory, keeps its one connection and one lock.
2. **A write transaction begins `IMMEDIATE`, with a busy timeout**, as `storage-schema.md` §7 specifies: the file's lock serialises a second process on the same store, the upgrade's old copy or a mistake, and a busy past the timeout, five seconds unless `--busy-timeout` says otherwise, is retried, up to three times, and then refused **`stale`, cause `retries_exhausted`, remedy `temporal`**, a verdict rather than a fault (ADR-0090); the event of a request that committed records the retries it took.
3. **A read uses the rules its snapshot was written under.** Each read transaction reads the store's declaration version and uses that version's library, the writer's where they are the same, so a read racing a publish never applies one version's rules to another's rows; a write reads it in its transaction too, and reloads the library where another process published since.
4. **The clock never moves backwards across them**: a read's `now` is the wall clock or the last time the store recorded, the later, as the writer's is (ADR-0174 decision 4).
5. **No operation's store work holds a thread another needs**: each runs where the asynchronous runtime replaces the thread it blocks, so writes queued on the lock cannot starve reads of threads.
6. **Read connections are pooled in the service**, a few kept idle and more opened on demand, with no new dependency.
7. **Concurrency is held by tests and measured, not argued**: reads answer within a bound while a long write holds the writer; concurrent writers on one object, with and without `expected_version`, lose no update and leave every version and position distinct; a read sees one committed state while writes commit around it; a held lock from another process meets the busy timeout and then `stale`; and a load probe reports reads and writes a second with and without the other running, as measured on the machine it ran on.

## Alternatives rejected

- **A connection per write, each beginning `IMMEDIATE` and waiting out the busy handler**, the shape of some web frameworks' SQLite defaults: the busy handler polls with sleeps, so writers in one process are served unfairly and late; a lock in the process queues them at no cost, and the file lock still guards against another process.
- **A writer thread fed by a queue.** The same serialisation as a lock, with more code, and a request's consultation of its evaluators already takes and releases the writer per attempt (D563).
- **PostgreSQL now.** It is the backend that runs writes in parallel, and a second storage backend and a new dependency; reads are what a pilot's dashboards and agents need to run beside its writes, and `TODO.md` keeps PostgreSQL for a consumer that outgrows one file.
- **A connection-pool crate.** About forty lines of the service do what a pilot needs, without a dependency to approve and pin.
- **A shared-cache in-memory database for the sandbox.** It locks whole tables rather than giving readers snapshots, and a sandbox serves one author.

## Consequences

- `storage-schema.md` §7, `DESIGN.md` §2 and §6, `library-api.md` §4, the wire reference, the integration guide and the core's README say what runs at once, and that writes are one at a time.
- `Verdict` gains `stale` with cause `retries_exhausted`; the wire carries it as `{"kind": "stale", "cause": "retries_exhausted", "remedy": "temporal"}`.
- The service takes `--busy-timeout <duration>`.
- `TODO.md` closes "more than one request at a time" and keeps PostgreSQL.
- An event's `retries` is computed on the wire, on both backends, the runner's always 0.

## As measured

On the development machine, 2026-10-11, a debug build over a store on local disk with `synchronous = FULL` (`deployment.rs`, `reads_and_writes_a_second_measured`): four readers of a pair's derived total read 3,148 times a second alone and 3,932 a second beside two writers, who wrote 94 times a second; two writers moving amounts between two accounts made 170 moves in two seconds while four readers read 3,628 times, every total the same. Each write's durable commit bounds the writes; the reads did not slow beside them. A read without its own transaction, the mutation of decision 1, was observed to see half of a move, the total reading 198 to 204 for 200, and reads behind the writer's lock, the service before this decision, waited out a held lock, 39.8 seconds for one `get`. These are measurements on one machine, not a capacity a deployment may assume.

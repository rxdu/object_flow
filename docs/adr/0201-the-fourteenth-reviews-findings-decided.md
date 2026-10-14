# ADR-0201: The fourteenth review's findings, decided

- **Status:** **Decided** 2026-10-14 on the author's standing instruction, "keep the bar and continue the loop", each decision the recommendation stated here; built the same day
- **Date:** 2026-10-14
- **Refined by:** ADR-0202 — the build's configuration at the repository's root, its flag required by a build script, and the personal guard on every actor the store does not know as a person or a service. ADR-0203 — a metric read's filter on a derived attribute reading a personal value refused too. ADR-0204 — decision 4's read guard by actor kind to be replaced by a guard that reads no actor (decision 9.3). ADR-0205 — decision 6's single masked digest replaced by two, the personal one deleted by an erasure.
- **Refines:** ADR-0200 decision 4 (where object reads that grow wait); ADR-0198 decision 1 (the bound on derived attributes, now on their depth too); ADR-0190 decision 7 (a reach a limit may stop); ADR-0197 (a derived value past its bound of hops)
- **Relates to:** `design/production-review.md` §18; `guides/operate.md`; `guides/agents.md`; `guides/wire.md`; `guides/errors.md`; `design/flow-format.md`

## Context

The fourteenth production review (`production-review.md` §18) found no blocker. Its operator found two majors and its adversary two, so the bar is not met.

**Object reads classified by type.** ADR-0200 decision 4 sent a `get`, `availability` or `check` to the analytics pool where any derived attribute, condition or invariant of its type read what grows with the store. A `get` evaluates no condition, yet one metric guard on `Claim.submit` made every claim's `get` wait behind reports: 12.7 s behind four per-person metrics, 25.5 s behind eight, `503` behind sixteen. The guard's metric, bound to one claimant, read every claim of the store for each request.

**Concurrent reads.** A filter costing 2.05 CPU-s alone cost 5.60 each with two at once and 11.07 each with four. ADR-0200 recorded the cause as perhaps SQLite's memory-statistics mutex, unmeasured. A probe running the operator's filter on one, two and four threads, each on its own connection, reproduced it: 2.2, 5.5 and 11.0 s each. Built with memory statistics off, the figures were the same, and so they were with glibc's allocator tuned, so neither is the cause. Built without `SQLITE_ENABLE_MEMORY_MANAGEMENT`, which the bundled SQLite of `libsqlite3-sys` sets, four at once took 2.17 s each. That option puts every connection's page cache in one group, so a page can be freed for any connection, behind one mutex for the whole process, and every page a read touches takes it. The engine never asks SQLite to free memory that way.

**Personal members in an agent's query.** `agents.md` says to strip personal fields from what an agent's reads hand the model. An agent's `query` could still filter and order by them, so ranges over `name` recovered every hidden name. A metric's filter reading a personal value was already refused.

**The minors**, each measured by one of the two reviewers:
- derived attributes chained 9,500 deep, which both checkers accepted, answered null past the 65th;
- after an erasure, a key sent with a different request replayed the first verdict, the erasure having set the digest to `erased` (fourth production review A-5);
- an assignment of `count + inputs.by` where `count` is unset answered `501 not_run`;
- work for a client that has gone ran to its end in its pool;
- a query asking for a field that scans its type ran in the worklists pool;
- a claim of 201 lines could never be withdrawn, paid or erased, its cascades' limits 200, and the checkers noticed only an erasure's;
- an import dropped an entry time for a dimension its type does not track;
- a long `--socket` path was refused after `--create` made the store;
- the backup timer kept every copy;
- a process id's file was committed.

## Decision

1. **A read of one object waits for a pool only where what it computes grows with the store, and among the objects' reads, never the analytics.** A `get` computes its type's derived attributes, an `availability` its conditions too, and a `check` its invariants as well; the growth of each is as ADR-0200 decision 4 set it. The objects pool already holds the reads of one object's history and events. (D649)
2. **A metric over objects bound to an attribute its item stores reads only the objects holding the value**, through that column, the binding then deciding among them as before. `claimant := claimant` over `c.claimant` reads one claimant's claims, never the type. A metric bound otherwise, or not at all, reads its type as before. (D650)
3. **The bundled SQLite is built without `SQLITE_ENABLE_MEMORY_MANAGEMENT`**, by `core/.cargo/config.toml`, which every build from `core/` reads. A store test fails a build without it, and the start's log says so. Heavy reads at once each cost what one alone does, up to the machine's cores. (D651)
4. **An agent's query whose `filter` or `order` reads a member marked personal, by its name in any type as a consulted reference is masked, is refused `personal`**, as a metric's filter is. A person's query is answered. (D652)
5. **A derived attribute's reads may go at most 64 deep, each reading the next, in both checkers**, the `eval::DERIVED_HOPS` an evaluation goes through. The depth is found in the walk that counts what an expression unfolds to, carried across modules beside it, and the first derived attribute past it on each path is refused `order`. A read past the depth, which only a version installed before may reach, is `not_run`, never null. (D653)
6. **A request's idempotency digest counts the values of its personal inputs by name alone**, so the store keeps no digest of personal data, and an erasure leaves it as it is. A different request under the key is `key_reused`, after an erasure too, and a key confirms no guess of a personal value, before an erasure or after it. A digest made before, with personal values in it, is still set to `erased` by an erasure. (D654)
7. **An assignment whose value is absent, other than an unsupplied optional input's, refuses its request `value_in_range`**, as arithmetic past its bound does, in both backends. (D655)
8. **A query computing a field, asked for or in its type's summary, that is a derived attribute growing with the store waits among the analytics.** (D657)
9. **A cascade or a loop over a set that no invariant bounds at its limit is noticed on every transition**, `unbounded_reach`, in both checkers: past the limit an object holding more can never take it. A loop with `where` counts its limit against what it keeps, so on a transition other than an erasure it is the author's rule and is not noticed; an erasure's is, since bounding the set suffices for it to be carried out. Three cascades in two of the repository's example flows, `delivery.yaml` and `issues.yaml`, and seven cascades and loops of the scenarios' inventory module (`metric-scenarios.md`, version 15), are now noticed. (D658)
10. **The minors:**
    - an import's entry time for a dimension its type does not track is refused, on a new object and on a refresh (D659);
    - a `--socket` path past what a socket holds is refused with the arguments (D660);
    - the backup timer deletes local copies older than a day, after a command of the operator's own copies them off the machine (D661);
    - the guides say that a read runs on after its client has gone, and how long a client should wait before sending it again (D656);
    - the guides list the agents' edges: the gateway sets each write's `principal` and `idempotency_key`, budgets refusals by bytes, and reads `taken_by` from the tools;
    - the process id's file is removed (D662).
11. **Unchanged:**
    - A read runs to its end or its deadline after its client has gone (D656, open). Stopping it needs the read moved off its connection's task, so the connection can notice its client leave while the read runs; that is recorded for later.
    - An agent's request without `type` takes any object of its id: the gateway sends each tool's `type` (ADR-0200 decision 3).
    - An approval holds writes, 44.9 s under load on six years; erasures stop long reads; the evaluator's token is read at start; the log is silent as free space falls; recovery is the last backup; a clock that ran ahead is not brought back.
    - No histogram says how long reads wait for each pool; the waiting count does.

## Alternatives rejected

- **A cap on heavy reads across every pool** (D651), the operator's proposal. The collapse was one lock, not too many reads: without it, reads scale to the machine's cores, and `--max-reads` per pool sizes the rest.
- **SQLite's memory statistics off, or glibc's allocator tuned** (D651). Each was measured, and neither changed the figures.
- **A pool of its own for one-object reads** (D649). The objects pool already is one.
- **Indexing a windowed metric by its time dimension** (D650), the operator's proposal. A guard's metric is bound to the person or object it guards far more often than it is windowed narrowly, and the binding narrows it to that person's objects whatever the window.
- **Refusing personal filters for every actor** (D652). A person's search by name is the application's to allow, its gateway filtering people's reads by its own rules.
- **A masked digest kept beside the full one** (D654). It would keep a digest of personal data until an erasure; masked from the first, the store keeps none. The cost: a key sent again with only a personal value changed replays, where it was `key_reused`.
- **Refusing arithmetic on an optional attribute in the checkers** (D655). Guarded by `is not null`, it is sound; telling the guarded from the unguarded is a notice recorded for later.

## Consequences

- A flow with a derived attribute more than 64 deep, an agent's query by a personal member, an assignment of no value, an import's entry time for an untracked dimension, and a `--socket` path past 107 bytes with `--create`, are refused, each a change that may refuse what an earlier release accepted. Before 1.0.0 this costs no promise (ADR-0180).
- A key sent again with only a personal input changed replays where it was `key_reused`.
- A build of the service must read `core/.cargo/config.toml`, as every build from `core/` does.
- Each fix is held by a test that fails without it (`defects.md` D649 to D662), each mutated.

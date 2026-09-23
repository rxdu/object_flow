# ADR-0091: A synchronous core with an injected connection source, which is how the harness interleaves requests

- **Status:** Accepted by the author, 2026-09-23, after a check of every decision against the design — decided at the author's direction, against `docs/PRD.md` N4, N2 and UC-19; repairs D209 and settles `library-api.md`'s open async question
- **Date:** 2026-09-23
- **Refines:** ADR-0077

## Context

PRD N4 requires behaviour to be deterministic under test, with the adversarial harness as the acceptance test. The harness's racer must interleave concurrent requests at statement boundaries, in an order its seed chooses, against a real database (`adversarial-harness.md` §4).

D209 found that it could not. `Store.request` is synchronous and runs to completion on its caller's thread, and a synchronous call cannot be paused between two of its statements within one thread. The API's open question — synchronous or asynchronous core — had cited the harness as a reason for the synchronous one without resolving this.

## Decision

### 1. The core is synchronous

`request` and every read run to completion on the calling thread. The first consumer's FastAPI service uses a thin asynchronous wrapper that runs requests on a thread pool. Both backends have mature synchronous drivers, and SQLite has only synchronous ones.

### 2. The store takes its connection source as an injected dependency

It already takes its id source and its clock (ADR-0077). With all three injected, a test controls everything nondeterministic about a request.

### 3. The harness supplies connections whose statements wait at a barrier

The harness runs each in-flight request on its own thread, and its connection source wraps every statement so that it first waits for the harness's permission. The harness releases exactly one waiting statement at a time, in the order its seed chooses, and waits for that statement to finish, or to block on a database lock, before choosing the next.

- **Deterministic.** Only one thread is ever runnable, so the interleaving is a function of the seed and the transcript, which is what the shortest-failing-prefix reduction needs.
- **Faithful.** The database is real, so the locks, the serialisation failures and the isolation are the database's own.

### 4. A statement that blocks inside the database is detected, not waited out

When a released statement does not return within a short bound, the harness records it as blocked on a lock and releases the next choice. The lock's holder can then proceed, which is exactly the D203 interleaving the racer exists to produce.

## Alternatives rejected

- **A coroutine core, with every database call a suspension point.** One thread could order it deterministically. But it doubles the surface — asynchronous drivers on PostgreSQL, a thread-backed wrapper for SQLite anyway — and it changes every caller's code, for a test concern an injected connection answers.
- **A hook inside the store**, pausing between its own steps. It tests the harness's idea of where the steps are, and it adds a test-only path to production code.
- **Simulating the isolation level in a single thread.** Rejected in `adversarial-harness.md` §4 already: it tests the harness's model of serialisability rather than the database's.
- **Real threads without a barrier.** Faithful, and not reproducible from a seed, so a failure could not be reduced.

## Consequences

- `DESIGN.md` §2 and §13 say how the core runs and how it is tested.
- `library-api.md` records the async question as settled, and adds the connection source to the injected dependencies.
- `adversarial-harness.md` §4 describes the barrier and the blocked-statement rule.
- D209 is resolved.

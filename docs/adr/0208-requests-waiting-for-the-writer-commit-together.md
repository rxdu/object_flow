# ADR-0208: Requests waiting for the writer commit together

- **Status:** **Decided** 2026-10-14 on the author's standing instruction ("go with your recommendations"), the reduction ADR-0205 decision 17 asked for
- **Date:** 2026-10-14
- **Refines:** ADR-0205 decision 17; `DESIGN.md` §6 step 8 and the attempt log's "own short transaction"
- **Refined by:** ADR-0228 — decisions 1 and 3, of this record's decision 4: a group's rollback that fails gives the store's connection up, so no request after it runs in the transaction it may leave open
- **Relates to:** ADR-0204 §1 (the profile and its month-end peak) and the durability promise of its criteria; ADR-0178 (one writer, reads beside it); `design/audit/cost.md` A1; `guides/operate.md` (metrics)

## Context

The audit's A1 found the writer the bottleneck at the month-end peak, twice the steady load: requests' p95 was 603 ms on the two-year store and 785 ms on the six-year one, against C5's 100 ms. ADR-0205 decision 17 decided that the writer's cost per commit is reduced, measured first, and that a peak still missing C5 is brought to the author by name.

Measured on 2026-10-14 on the reference machine (`scripts/profile/`, a two-year synthetic store, the release binary at `622d44f`):

- **Writes alone.** Offered 50 drafts a second, the writer answered all of them at p95 16 ms. Offered 100 a second, it answered 87 a second at p50 372 ms and p95 406 ms: it was saturated, and the rest was queueing.
- **One draft's cost**, 200 drafts one at a time, by phase at p50: beginning the transaction 0.1 ms, the snapshot 0.2 ms, deciding 1.4 ms, writing the rows 1.7 ms, and the commit 7.2 ms. The cost did not grow with the store: 12.7 ms a draft on a small store and 12.9 ms on the two-year one.
- **The commit is the disk's flush.** A bare SQLite commit of one small row, in WAL mode with `synchronous = FULL` on the same disk, took 6.7 ms; with `synchronous = NORMAL`, 0.08 ms. `strace` counted one `fsync` a draft.
- **A draft writes about 83 KiB of `-wal`**, some 20 pages, measured over 50 drafts before a checkpoint ran.

So about 70% of the writer's time per request is the flush, and a writer committing one request at a time cannot pass about 140 a second however little each request does. Halving the rest would take it from about 87 to about 105 a second, which at the peak's 100 is a queue that never drains.

The flush cannot be skipped. ADR-0204's criteria promise that an acknowledged write survives any crash, kill or stop, and power loss on a disk that honours its flushes.

## Decision

1. **Requests waiting for the writer are committed together.** When the writer is free, the request that takes it also takes every request that queued while it was busy, up to 64. It applies them in the order they queued, in one SQLite transaction, then commits once, with one flush, and answers each. No request is answered before the commit that holds it, so the durability promise holds as it did. A request that finds the writer free and nothing queued commits alone, as before, without waiting for company.
2. **Each request is still its own.** Each is decided against the store as the requests before it in the group left it, and keeps its own transaction number, positions, events, read set and idempotency record. Each runs on a savepoint of the group's transaction. One refused, or failing with a verdict, is rolled back to its savepoint alone, and its attempt row and the sequence numbers it drew are written on a savepoint of their own in the same commit, where they had a transaction of their own after the rollback. Readers see a group's requests all at once, in order, as they would see the same requests committed one by one in quick succession.
3. **Only plain requests share a commit.** A group holds requests made through `/v1/request`, other than those that change the flow: a change's `submit` or `refresh` with its prepared dry run. Every other write, publish, import, maintenance, acknowledge and upgrade among them, takes the writer alone as it does now. An erasure joins a group, and the `-wal` cut it asks for (D584) runs once after the group's commit.
4. **One request's fault answers only that request.**
   - A request that stops to consult an evaluator leaves the group rolled back to its savepoint, consults outside the writer as it does now, and queues again.
   - A store error during a request, or at the commit, rolls the whole group back. Each request is then run again alone, so the fault answers the request that meets it and the others are answered as if the group had not been formed.
   - A panic during a group likewise rolls the group back and runs each request again alone, under the existing recovery for a panic holding the writer, so only the request that panics again is answered 500.
5. **Groups are observable.** `/metrics` counts the commits of requests and the requests they held, so the requests per commit can be read, beside `writer_held_seconds`.

## Alternatives rejected

- **`synchronous = NORMAL`.** It removes the flush from each commit, 6.7 ms to 0.08 ms measured, and loses the last commits on power loss. It breaks the durability promise of ADR-0204's criteria.
- **Answering before the flush, an asynchronous commit.** The same loss, with a narrower window that still contains answered writes.
- **Reducing each request's work alone.** It is about 3.4 ms of the writer's 10.6 at p50 measured. Halving it gives about 105 a second, short of the peak with room to spare. It remains worth doing, measured, beside this.
- **A dedicated writer thread fed by a channel.** It works, but every other write takes the writer's lock as now, so the thread would share the lock rather than own the store, and its lifecycle, a panic in it, a stop draining it, is machinery of its own. The request that takes the lock leading its group keeps one writer gate, and needs no thread.
- **Waiting a little for a group to form.** A lone request would pay the wait for nothing. Requests queue on their own exactly when the writer is busy, which is when a group helps.
- **Asking clients to batch.** `import_batch` exists for one client's many objects. The profile's writers are independent people and agents, whom no client can batch.

## Consequences

- A request's latency under load is bounded by one group's run rather than by a queue of single commits. At the steady load nothing changes: a request rarely waits, and commits alone.
- `DESIGN.md` §6 step 8 says "commit" of a request's transaction; it now commits with the requests beside it. The attempt log's "own short transaction after the rollback" becomes a savepoint of the group's.
- The suite gains a test that concurrent requests share commits and answer as the same requests one by one would, and Gate P at twice the load is the measure of whether A1 is closed. A peak still missing C5 after this is brought to the author by name, as decision 17 says.

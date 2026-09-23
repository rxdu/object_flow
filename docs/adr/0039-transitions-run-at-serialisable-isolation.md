# ADR-0039: The transition transaction runs at serialisable isolation

- **Status:** Accepted — repair of D02 and D04, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refined by:** ADR-0090 — a contended row waits and then retries on PostgreSQL; row locks end the loser sooner, they do not queue it.
- **Refines:** ADR-0023

## Context

ADR-0023 locks what a transition writes, states that objects only read are not locked, and grants serialisable isolation to invariants alone. It never states the isolation level of the transaction itself. Defect D02 established that this defeats ADR-0024 by construction: its delete guard reads the referencing type, the reference lives on the referencing row, so deleting a customer and creating a delivery for that customer lock disjoint rows, conflict on nothing, and both commit. The result is a deleted object with a live reference, the failure ADR-0024 exists to prevent.

The hole is general. Every cross-object guard has it, and the design deliberately pushes cross-object rules into guards. ADR-0023's own context cites "the check-then-act hole a service-layer check leaves"; an unisolated type-scan guard is that hole.

Defect D04 established a second problem: the lock set is discovered by evaluating filter expressions that read data, yet locks were to be taken before any guard ran and in id order. Those rules cannot all hold, and created objects have no id to sort by.

## Decision

1. **The transition transaction runs at serialisable isolation.** Reads a guard performs are then protected by the database's own conflict detection, whether or not the objects read are in the lock set.
2. **A serialisation failure is retried**, bounded by a declared attempt count. On exhaustion the request is refused with the `stale` verdict, whose remedy of re-read and decide again is exactly right.
3. **Row locks remain**, taken as objects are reached during sequential application (ADR-0038), not computed in advance. They serve contention rather than correctness: they make hot-row conflicts block instead of aborting and retrying, which is cheaper for the flash-sale case. *(Corrected by ADR-0090, 2026-09-23: probed on PostgreSQL 16.8, a contended request at serialisable isolation waits for the lock and then fails with a serialisation error. The lock makes the loser fail sooner; it does not make it queue.)*
4. **Deadlock is handled by the database**, not avoided by lock ordering. ADR-0023's fixed-id ordering rule is withdrawn: it was unimplementable, since the set is discovered by traversal and created objects have no id yet. A deadlock surfaces as a serialisation failure and is retried under rule 2.
5. **Constraint-compiled invariants stay** (ADR-0009, ADR-0023) as an optimisation and a second line of defence, not as the only protection for concurrent violation.
6. **The isolation level is not a deployment choice.** A deployment that lowers it loses the mediated guarantee, and the declaration cannot express that it has.

## Alternatives rejected

### Lock everything a guard reads

Correct in principle and unimplementable: a type-scan guard reads a predicate, not a set of rows, so locking it means predicate locks. Serialisable isolation is the database's implementation of exactly that.

### Leave guards unisolated and rely on invariants

The superseded position. Rejected: it requires every cross-object guard to be restated as an invariant to be safe, which is the duplication ADR-0009 exists to remove, and it silently defeats ADR-0024.

### Snapshot isolation plus explicit conflict declarations

Have each transition declare what it reads so the runtime can lock or check it. Rejected: it is serialisable isolation with the analysis moved into the declaration, where an omission is silent.

## Consequences

- Throughput is bounded by the serialisation-failure rate, not by lock contention alone. The orders case study's hot-row analysis needs revisiting: a popular product now queues on the row lock rather than aborting and retrying, per decision 3, so the failure mode is latency rather than a retry storm, and the row lock of rule 3 is what keeps that bearable. *(Corrected by ADR-0090 §2: on PostgreSQL a popular product does not queue. The second request waits for the lock and then fails with a serialisation error, which the retry of decision 2 absorbs, so a contended request pays the wait and the retry; the lock only makes the loser fail at its first touch of the row. Each event records its retries (ADR-0083), so the cost is measured rather than argued; `DESIGN.md` §6.)*
- SQLite serialises writers already, so this costs nothing there. *(Corrected by ADR-0090 §1: under SQLite's default deferred `BEGIN`, a request that reads and then writes fails with `database is locked` when another commits in between, observed on SQLite 3.37.2 (D204). Every transition transaction therefore begins `IMMEDIATE`, a contended request waits out the busy timeout, and a timeout exceeded counts as a serialisation failure, retried and then refused as `stale`; `DESIGN.md` §6.)* PostgreSQL pays for predicate tracking, which is the honest price of the guarantee.
- The retry count is a deployment setting with a declared default, and exhaustion must be observable, since a rising rate is the signal that a declaration has a contention problem.
- ADR-0023 rules 2 and 6 are superseded. Its rules 4 and 5 on versions and the `stale` verdict stand.
- D02 and D04 are resolved.

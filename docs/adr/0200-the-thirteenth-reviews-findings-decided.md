# ADR-0200: The thirteenth review's findings, decided

- **Status:** **Decided** 2026-10-14 on the author's standing instruction, "keep the bar and continue the loop", each decision the recommendation stated here; built the same day
- **Date:** 2026-10-14
- **Refined by:** ADR-0201 — a read of one object classified by what that operation computes, among the objects' reads.
- **Refines:** ADR-0198 decision 2; ADR-0199 decision 2; ADR-0189 decision 1 (an agent's tools); ADR-0158 (what an event records of its rules)
- **Relates to:** `design/production-review.md` §17; `guides/operate.md`; `guides/agents.md`; `guides/wire.md`; `design/renderers.md` §3

## Context

The thirteenth production review (`production-review.md` §17) found no blocker. Its operator found one major and its adversary five, so the bar is not met. The operator's stores were synthetic, built to the provided stores' size: its copy of the provided stores was refused by the environment's privacy check, and it did not work around that.

**N3.** Once an operation had read the records of eight objects of a type one by one, it read the records of every object of the type. A worklist ordered by when each claim entered its state, the guides' own example, cost 0.8 s a page on six years whatever its limit, sixteen at once making everyone's worklists wait 10 to 18 s. A page of `available` past eight items, and a cascade over a claim's lines under the writer, paid the same.

**The adversary's five:**
- **A.** An erasure missed personal data the engine keeps. An evaluator's argument was written into a consulted key, `crm.known(an address)`, which outlived the erasure in history, the log, the attempts and the store's file, and which the log's export carried while it left personal inputs out. A proposal already decided kept its personal inputs.
- **B.** An agent's tool was not bound to its type. A request could not name a type beside an object, so `customer_revoke` revoked a person's subscription.
- **C.** `get`, `availability` and `check` of a type whose rules read a metric or count referrers ran in no pool: 64 at once peaked at 4.49 GB.
- **D.** A change's checks grew with the square of the types it refused. Each finding was compared with every one before it, and each finding's line was found by searching every type's key. 60,000 types checked in 374 s, and ADR-0198 decision 2's deadline, read only while unfolding derived attributes, did not stop them.

## Decision

1. **A record is read for the objects an operation holds, by id, a few hundred to a read, never every object of the type.** A test reads a page of five of 1,000 objects in about ten records, failing with the whole type read. (N3)
2. **A consulted reference writes `(personal)` for an evaluator's argument, or a metric's binding, that reads anything the library marks personal.** That is an attribute or an input marked personal, or a derived attribute reading one, matched by name in any type. The evaluator is still sent the value; the store keeps none. **An erasure also redacts the inputs personal to its transition from every proposal decided on its object**, one `redact` event each, as it invalidates and empties those pending. The reference runner now empties a pending proposal's inputs too, which it had kept, unseen by any test. (A)
3. **A request may name a `type` beside its `object_id`, and the object must then be of that type's family, else `not_found`.** Each write tool names its `request`, a type and a transition, which the application sends with the call's arguments, so a tool takes no other type's object of the id it is given. This holds in both backends. (B)
4. **A `get`, `availability` or `check` of an object whose type's rules grow with the store waits among the analytics.** Its type's derived attributes, conditions or invariants then read a metric, a type scan or the objects referring to it, itself or through a derived attribute read by name. Any other never waits. (C)
5. **A change's checks are linear in what it holds.** Findings are kept once by a set, and a large mapping's keys are found by an index made on its first lookup. 60,000 types check in 5.9 s, where 374 s. ADR-0198 decision 2's deadline is read while derived attributes unfold. The rest of the checks are linear in a change that the 2 MiB request bounds, which this decision states where that one's text implied every check. (D)
6. **The minors:**
   - the runtime keeps a thread for every request the connection cap lets run, the cap at most 4,096, so `/health` always has one;
   - an import's dry run decides every check of its writes, and writes nothing;
   - a publish preview's `expected_version` is an integer, as an approval's is;
   - the guides cover:
     - the analytics pool's object reads;
     - a filter narrowed by a crowded state;
     - erasures in a loop stopping long reads;
     - a backfill's approval holding writes 48.7 s under load on six years;
     - rotating an evaluator token, which needs a restart;
     - `SSL_CERT_FILE` in the unit;
     - the backup directory and a backup's volume;
     - a `-wal` replicator, untested beside the service;
     - the changelog's claim that releases are tagged, which none is yet.
7. **Unchanged:**
   - Concurrent heavy reads cost more CPU than one alone, eight queries 83 CPU-s against 3. The cause is inferred, perhaps SQLite's memory statistics mutex, and not measured, so it is recorded to measure first.
   - `available` where few objects in its states qualify still reads on through the state.
   - A proposal may be filed with an `expires_at` past, then unavailable while it reads pending.
   - Whoever reaches the socket is the operator, so the gateway is the trust boundary (ADR-0183).
   - An approval checks its impact under the writer (ADR-0199 decision 7).
   - The log says nothing as free space falls below 64 MiB, though `/health` and the metrics do.

## Alternatives rejected

- **Writing no argument in a consulted key** (decision 2). Two calls of one function in one rule would then share a key; only what is personal is masked.
- **Redacting consulted keys at an erasure by the values erased** (decision 2). That would read every event a value may have been passed from, and match text. Never writing the value needs no such search.
- **A deadline read in every per-type loop of the checks** (decision 5). There are dozens of them, and a check found slow stays slow behind one. The two quadratics are removed, and a test holds the refused shape linear.
- **A pool of its own for object reads that grow** (decision 4). It would be a flag more; the analytics pool is where store-sized computation already waits.

## Consequences

- `/v1/request` takes `type` beside `object_id`, and each write tool carries `request`. A store recorded before this release keeps the consulted keys it wrote; an erasure does not reach those. Before 1.0.0 this costs no promise (ADR-0180).
- `--max-connections` past 4,096 is refused at start.
- Each fix is held by a test that fails without it (`defects.md` D642 to D648).

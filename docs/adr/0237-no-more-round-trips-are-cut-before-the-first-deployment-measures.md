# ADR-0237: No more round trips are cut from a write before the first deployment measures its peak

- **Status:** **Decided** 2026-11-01 under the author's direction ("go with your recommendations"), on the recommendation given the same day for the question `TODO.md` put on 2026-10-29.
- **Date:** 2026-11-01
- **Relates to:** ADR-0208 (requests waiting for the writer commit together); ADR-0234 decision 3 (the next release accepts the peak's writes' miss by name, measured again on the first deployment's hosts); ADR-0177 decision 3 (every operation reads the store's release in its transaction); `research/write-throughput.md` §6 and §8

## Context

In 0.9.0's build, a draft on PostgreSQL sent 29 statements, each a round trip on the writer's connection. Two cuts left 22 (`write-throughput.md` §6):

- **The first cut (29 to 25)** raised the writer's ceiling at 32 sessions on cores kept awake, from between 200 and 250 writes a second to between 250 and 300.
- **The second cut (25 to 22)** did not move it. At 250 writes a second, one run of each build passed C5 and one failed. Past the ceiling, the writer answered 309 and 315 writes a second against 295 and 299, about 5% more.

With the cores idle, as on the gate's machine, runs at one rate scattered too widely to place a ceiling for either build of the first cut. At the month-end peak, the writes at 100 a second passed C5 in nine runs of nineteen (ADR-0234). pgbench's `SELECT 1` from the host to the database's container took 0.47 ms a round trip at 10 a second with the cores idle, and 0.059 ms beside busy loops.

Four further cuts were listed (`TODO.md`, *Write scale past one writer*), each one round trip a draft:

1. the engine's release read again for the answer, after the commit, which ADR-0177 decision 3 reads in each operation's transaction;
2. `BEGIN` sent with the first statement;
3. the actor's name tried as an object's id before its identity;
4. an object got by id in two reads, its type from the directory and then its row, one round trip an object.

*Derived:* together they save four round trips of 22, about 1.9 ms of the writer's time a write with the cores idle at 0.47 ms each, and 0.24 ms on awake cores.

## Decision

1. **The next release cuts no more round trips from a write.** The four stay listed in `TODO.md`.
2. **They are cut when the first deployment's hosts miss the peak's writes**, which ADR-0234 decision 3 has those hosts measure. Those hosts' cores and power settings, not this machine's, then set what a round trip costs.

## Alternatives rejected

- **All four now, as one change.** It changes the write path in four places, and the first changes ADR-0177 decision 3. The second cut moved no ceiling. Telling four more round trips from the idle cores' scatter would take many runs a build at the peak, against a miss ADR-0234 already accepts by name.
- **The two that extend the second cut's pattern** (3 and 4), each read made one statement. The same objection holds, at half the saving.

## Consequences

- `TODO.md`'s *Write scale past one writer* records the decision and keeps the four cuts listed.
- `write-throughput.md` §8 item 1 says it. Its later steps, a standby service, read replicas, stores by domain and several writers, stay a recommendation the author has not decided.

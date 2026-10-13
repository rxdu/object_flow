# ADR-0199: The twelfth review's findings, decided

- **Status:** **Decided** 2026-10-13 on the author's standing instruction, "keep the bar and continue the loop", each decision the recommendation stated here; built the same day
- **Date:** 2026-10-13
- **Refines:** ADR-0198 decision 1; ADR-0196 decision 4 (the read pools)
- **Relates to:** `design/production-review.md` §16; `guides/operate.md`; `design/flow-format.md`

## Context

The twelfth production review (`production-review.md` §16) found no blocker. Its operator found no major and gave GO, having measured every finding of the eleventh review fixed. Its adversary found one major, so the bar is not met.

ADR-0198's bound on what an expression unfolds to was counted within each module. A doubling chain split across a module and one importing it passed each module's bound, 13 links in each. An invariant on the far end then made a dry run's checks burn a core for the whole `--dry-run-timeout`, holding the one slot, and the reference checker ran without end. The deadline ADR-0198 decision 2 added held, as the backstop it was meant to be.

The minors:
- A query filter aggregating over a collection for each object was narrowed by a state's index and so waited in the worklists pool, 8 s a read on six years; eight at once made a plain worklist wait 16 s.
- `available` read every object in its transition's states on every page, 0.75 s on six years whatever its limit, in the same pool.
- A transition calling itself down a reference chain, which both checkers refuse, aborted a sandbox at 3,000 links: request execution had no depth bound of its own.
- Both checkers read upper-case text inside a string literal, a default of `"EU"`, as a state's name and refused it.
- The rest were said badly or not at all:
  - the checker's count of guard conditions decided in full was printed where none was left open;
  - a long `--socket` path was refused in the system's words;
  - a store copied in at 0644 was warned about without its `-wal` and `-shm`;
  - the guides lacked the commit in the install's build, an evaluator under a private certificate authority, a backup timer, withheld request bodies, and a backfill's approval hold.

## Decision

1. **The bound counts across modules.** A name after a dot also reads the derived attributes of that name the modules checked before declare, at the most each unfolds to, in both checkers. A module past the bound is refused by its first link, and the split chain's importer is refused in milliseconds. A version carries every module it imports, each checked before its importers, so every module a read can reach is counted. (the major)
2. **A query whose filter aggregates over a collection for each object waits among the analytics**, itself or through a derived attribute it reads, however an index narrows its page. **A page of `available` reads from its cursor a chunk at a time** until it holds its limit and one more, as a query's page does, so its cost follows its page where most objects in its states are open to the actor.
3. **A request takes at most 64 transitions each caused by the one before**, past which it is not run, as derived attributes are bounded at 64 hops. A supersession group's members are each one deeper than the erasure walking it, never one another, so no flow the checkers accept comes near it. On a request thread's 16 MiB stack, 64 links take about a fortieth of it, inferred from the review's overflow at 3,000 links, not measured; a test runs 64 on a debug build's larger frames in 16 MiB.
4. **A string literal's text names nothing**, in both checkers. The checker prints its count of guard conditions decided in full only for a type where it left some open.
5. **The minors said:**
   - a socket path past the 107 bytes a Unix socket may hold;
   - a store readable by others, with its `-wal` and `-shm`, which take the store's mode.
6. **The guides:**
   - the commit in the install's build;
   - an evaluator under a private certificate authority, trusted through the system's store or `SSL_CERT_FILE`;
   - a backup timer, verified with `systemd-analyze verify` and a read-only backup of a live store;
   - a reverse proxy, or a lower `--idle-timeout`, where clients that hold back a body share the socket;
   - a backfill's approval holding writes about 20 s on six years, and pausing creations of the type while it runs;
   - the pools as decision 2 sets them.
7. **Unchanged:**
   - A store's clock that jumped ahead is not brought back (`clock_lead_seconds` says it; ADR-0188).
   - Recovery is the last backup, which hands ids out again (ADR-0197 decision 8).
   - An approval checks its impact under the writer, so a creation during it refuses it after the hold. The guide says to pause creations meanwhile; checking first is recorded for later.
   - An upgrade changing built-in types is built before a release that changes them (ADR-0177 decision 6).
   - An agent takes any transition a flow leaves open, the flow's `taken_by` and the gateway closing it (ADR-0189).
   - The adversary's cosmetics:
     - a duration taking a doubled space;
     - a JSON integer past 2^53 taken exactly;
     - `POST /health`'s wording.

## Alternatives rejected

- **A wall-clock bound on the reference checker** (decision 1). With the bound composed, nothing it walks is exponential. A timeout would make an authoring tool's verdict depend on the machine it runs on.
- **A pool of its own for `available`** (decision 2). It would be a flag more, where reading to the page makes its cost follow the page.
- **Refusing aggregates in a worklist's filter** (decision 2). They are legitimate; where they wait is enough.

## Consequences

- A change whose modules together unfold an expression past 10,000 reads of derived attributes is refused, a change that may refuse what an earlier release accepted. Before 1.0.0 this costs no promise (ADR-0180).
- A request past 64 caused transitions is not run, where it overflowed the stack.
- Each fix is held by a test that fails without it (`defects.md` D637 to D641).

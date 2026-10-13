# ADR-0198: The eleventh review's findings, decided

- **Status:** **Decided** 2026-10-13 on the author's standing instruction, "continue the loop to get the stack ready for production, follow your recommendations", each decision the recommendation stated here; built the same day
- **Date:** 2026-10-13
- **Refines:** ADR-0197 decisions 1, 2 and 5; ADR-0196 decision 1; ADR-0179 (paging); the derived attributes of `flow-format.md` §4 and its check table
- **Relates to:** `design/production-review.md` §15; `guides/operate.md`; `guides/errors.md`

## Context

The eleventh production review (`production-review.md` §15) found no blocker and two majors. The adversary's F1: a chain of derived attributes each reading the one before twice, `d1: d0 + d0` on to `d40`, held the store's one dry-run slot past six minutes on a single core. The checkers typed each derived attribute again at each read, 2^n times, so ADR-0197's bound of 64 on depth bounded nothing; the reference checker hung the same way, and any later change's submit waited the whole `--dry-run-timeout` and was refused. The operator's N1: a page of a type's intervals read every event into memory and walked them once for each of the type's objects, whatever its cursor or limit. That cost 15 to 19 s and a gigabyte a page on the six-year store, past `--read-timeout`, with an erased name left in the `-wal` for 43 s, past the erasure's urgent cut.

Beside them were three of the kind the tenth review left:
- A published chain's reads cost a second of a core each, the same exponential at run time (F2).
- A recursion through a set, which an installed version may hold, still aborted the process on a leaf's read. Typing the zero of a sum over nothing recursed with no bound (the operator's D-RECURSE residual).
- A page of the log's export still held each event's object whole, 550 MB for 178 KB of changes' sources (the MEM-EVENTS residual).

Measuring the fix found more of F1's kind. An invariant or a condition reading the top of such a chain was walked exponentially by the optional reads, the inliner and the logic solver. An invariant reading one derived attribute a thousand times was inlined a thousand times. And many expressions, each within any bound, multiplied an unmemoized walk.

## Decision

1. **No expression may unfold to more than 10,000 reads of derived attributes, each counted as often as it is made, in both checkers.** A name after a dot reads every derived attribute of that name the module declares, as the cycle check reads it, and a bare name the one its own type declares. Each derived attribute's count is found once, iteratively, and capped. The first derived attribute past the bound on each path is refused, `order`, and so is any other expression past it through none that is. A module holding one is refused before any later step reads it, as one that does not parse is. Real flows in the repository unfold to 3 at most, and the scaling tests' chains to 3,000. The checkers also type each derived attribute once per module. (F1)
2. **A dry run's checks are held to its deadline**, its `--dry-run-timeout`, as its impact already was. A deadline is read at each step that unfolds a derived attribute. Past it, nothing more is unfolded, the checks end, and the dry run is answered as late, its findings discarded. Decision 1 removes the exponential. This is the backstop that frees the one slot whatever check proves slow next. (F1)
3. **An evaluation computes each derived value once.** The derived values it has computed are shared with the evaluations inside it. A derived value's type, given to a zero, is found once per member and never through itself or past 64 derived members. A chain of 22 reads in milliseconds, where uncached it took 16 s, and a recursion through a set answers a leaf's read where it aborted the service. (F2, the D-RECURSE residual)
4. **A page of `export` reads the store by its indexes.** A type's intervals are read by the event that last wrote each, legacy spans by their object's first import, and attempts by their transaction. Each is read a window of positions or transactions at a time: a few attempts at a time to the page's bytes, never a transaction split, with the read's deadline and its stop checked between windows. A page of the log reads each event's object's type, never the object. Three indexes are made beside the schema when a store opens, as the actor's are: an expression index on an interval's cursor, a partial one on imported events, and one on an attempt's transaction. A test compares every paging, at several limits, with the old reads. (N1, the MEM-EVENTS residual, the operator's minor on attempts)
5. **A decimal an effect writes past its digits refuses its request `value_in_range`**, as an integer and money do, where it answered `501 not_run`. (F3)
6. **The minors:**
   - SIGHUP is said and ignored. It ended the service at once, and systemd's `Restart=on-failure` counts that end clean.
   - The log says how many lines it dropped once it is read again.
   - `--store` naming a directory says it is one.
   - A store that reads as no database beside a `-wal` or `-shm` is given the restore's advice, as every other check that finds it is.
   - The guides gain the unit's `StateDirectoryMode`, a backup check that needs no `sqlite3` program and leaves nothing beside the copy, a deployment's first publish from a shell (run against a fresh store as written), `disk_full` among `/health`'s codes, and how an export page reads.
7. **Unchanged:**
   - A refused request is written as an attempt, so an actor's refusals in a loop are the gateway's to limit.
   - At six years, 24 concurrent per-person metrics exceed the analytics pool, which `--max-reads` sizes.
   - An erasure stops the long reads it meets, each answered to be sent again.
   - `/health` answers `200` while the clock leads (`clock_lead_seconds`) and while an evaluator refuses the token, counted in its metrics.
   - A second service on the same store is said at its own start.
   - A restore hands out ids again (ADR-0197 decision 8).
   - An agent's `forget` or `corrects` of a person's label, and the opt-in `target_version`, are the gateway's (ADR-0197).
   - An approval at six years holds writes about 16 s under load.

## Alternatives rejected

- **A memo in each walk instead of a bound** (decision 1). The inliner's tree is the unfolding itself, so no memo shrinks it. A memo in each walk also leaves the next walk written unmemoized. One bound, applied before any walk, covers them all.
- **Bounding derived attributes alone** (decision 1). An invariant reading one derived attribute a thousand times inlines it a thousand times.
- **A deadline inside the checks as the fix** (decision 2), rejected by ADR-0196 and ADR-0197 because it leaves a slow check slow. It is adopted here only as a backstop beside decision 1, so `--dry-run-timeout` covers both of a dry run's phases.
- **Checks run on a thread abandoned at the deadline** (decision 2). The thread would go on burning a core, one for every such submit.
- **Intervals exported by walking the log for the type's events** (decision 4). A publish's migration opens spans at an event of the change, and a correction may be another object's event, so both would be missed.
- **Reopening a log file on SIGHUP** (decision 6). The service writes to standard error, and has no file to reopen.

## Consequences

- A flow with an expression unfolding past 10,000 reads of derived attributes is refused, a change that may refuse what an earlier release accepted. Before 1.0.0 this costs no promise (ADR-0180). One installed before keeps serving: its reads compute each value once.
- `--dry-run-timeout` bounds a dry run's checks too: a change whose checks pass it is answered late, as one whose impact does.
- A store opened by this release is given three indexes, made once, which change no answer.
- Each fix is held by a test that fails without it (`defects.md` D631 to D636).

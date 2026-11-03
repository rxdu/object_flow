# ADR-0245: `check-all` runs its gates in concurrent lanes, after its builds

- **Status:** **Decided** 2026-11-03 under the author's direction ("go with your recommendations"), on the recommendations given the same day when the author asked for a way to shorten the gate. Built the same day, CI's jobs by lane excepted (decision 4).
- **Date:** 2026-11-03
- **Refines:** ADR-0183 decision 2 (CI runs the one command a change is checked by)
- **Relates to:** ADR-0213 (the core's tests on PostgreSQL); ADR-0235 and ADR-0236 (Gates P and I on an idle machine)

## Context

The author asked on 2026-11-03 for a way to shorten the gate, since "right now it takes very long to make changes and cut a release". Measured that day (`TODO.md`, *The gate's running time*):

- `scripts/check-all` with PostgreSQL took 39 to 44 min on each of three runs, on a machine of 12 threads and 31 GB. CI's `check` took 35 to 58 min over its last six runs.
- `check-all` ran its 17 gates one after another, though the core's tests on SQLite, those on PostgreSQL, the Flow Studio's suite and the design's checks read nothing the others write.
- `check-corpus` took 5.7 min. Under a profiler, 5.9 of its 6.1 min were the checks it runs as subprocesses, nearly all of it `check-scenarios` and `check-flow-docs`, which `check-all` ran again as gates of their own.
- Each run built the service twice into `core/target/debug`: with every feature for the core's tests, and without any for the Flow Studio, the guides and the replays. The two builds replaced each other's binary, so run at once they would replace it under a test running it.
- One test is known to fail under the suite's load, `a_connection_at_the_cap_makes_room_or_is_told_busy` (`TODO.md`).

## Decision

1. **`check-all` runs in three steps.**
   1. The builds, one at a time: the core's format and clippy, its tests built with every feature, and the service built without any, apart in `core/target/plain`.
   2. The lanes, at once, each its gates in turn: `core` (the tests on SQLite), `postgres` (the tests on PostgreSQL), `studio` (the Flow Studio's server and web app), `checks` (the design's checks) and `scenarios` (`check-scenarios` and the two replays). Their lines are printed once every lane ends, in that order.
   3. The tests a load can fail, `ALONE` in the script, one at a time on each backend. The lanes skip them, and a name no test has any more fails the step rather than running nothing.
2. **Each gate's line says its elapsed seconds,** and the last line the whole run's.
3. **The service without features is read where cargo built it.** The Flow Studio's server and tests, `replay-scenarios.py` and `check-guides.py` read `CARGO_TARGET_DIR` where it is set, and `check-all` sets it for the lanes that run that build.
4. **`check-all --lane <lane>` runs one lane's gates in turn,** with the builds and the lone tests it needs, and `--serial` runs every lane in turn. CI runs `--serial` in its one `check-all` job until its jobs are split by lane, since `release.yml` publishes only a commit whose check run named `check-all` passed, and a runner of 4 vCPUs and 16 GB has not been measured with the lanes at once.
5. **`check-corpus` no longer runs `check-scenarios` and `check-flow-docs`,** which `check-all` runs.

No gate is removed, no check is weakened and no dependency is added.

## Alternatives rejected

- **Every gate at once.** The core's tests on SQLite and on PostgreSQL share a build and the database, and a test known to fail under load would fail more often.
- **The Flow Studio's core built with every feature,** so one build serves every gate. The Flow Studio would then run the service with the faults a test makes happen compiled in, which no release build has.
- **`pytest-xdist` for the Flow Studio's suite.** It is a new dependency, which needs the author's approval.
- **CI split into one job per lane in this release.** Its check runs would be renamed under `release.yml`'s condition, and a workflow cannot be tried before it is pushed.

## Consequences

- Built in `scripts/check-all`, `core/scripts/check` (its `lint`, `build` and `test` stages), `scripts/check-corpus.py`, `scripts/check-guides.py`, `scripts/replay-scenarios.py`, the Flow Studio's `flowstudio/core.py` and two of its tests, and `.github/workflows/check.yml`. `core/README.md` and `README.md` say so.
- **Measured 2026-11-03,** at `a451202` with the change on the same machine, both backends: every gate passed in 940 s, 15.7 min, where three runs took 39 to 44 min before it. The builds took 56 s, the lanes 882 s at most, `studio` with its web app the longest beside `postgres`'s 878 s, and the lone test 2 s. `check-corpus` took 16 s in its lane, where it took 5.7 min. The lanes in turn, `--serial`, were run over `checks` alone, with the builds and the lone tests, passing in 181 s. The two longest lanes now set the time: the Flow Studio's suite, and the core's tests on PostgreSQL at four tests at a time.
- CI's jobs by lane, and `release.yml` reading them, follow 0.13.0 (`TODO.md`).

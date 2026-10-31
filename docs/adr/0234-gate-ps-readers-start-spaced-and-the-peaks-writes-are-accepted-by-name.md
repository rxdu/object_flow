# ADR-0234: Gate P's readers start spaced, and the peak's writes on PostgreSQL are accepted by name

- **Status:** **Decided** 2026-10-31 under the author's direction ("go with your recommendations"), given after the two questions were put in `TODO.md` the same day.
- **Date:** 2026-10-31
- **Refines:** ADR-0210 decision 1 (Gate P's readers have a pace)
- **Relates to:** ADR-0204 §1 (the profile, its month-end peak, a restart among its bursts) and §7 (the soak); ADR-0205 decision 17 and ADR-0208 (a peak still missing C5 is brought to the author by name); ADR-0214 decision 2 (a release's gate accepts each miss by name); `design/audit/cost.md` §3 (Gate P); `research/write-throughput.md` §6

## Context

Gate P ran at scale 1 alone from 0.3.0 to 0.9.0. At scale 2, ADR-0204 §1's month-end peak, it was measured on 2026-10-31 in ten runs on PostgreSQL and two on SQLite (`TODO.md`, *Gate P at the month-end peak*). SQLite passed both runs. PostgreSQL missed C5 in two ways.

**The reports missed in every run.** In each run, two to five of the six reports had a p95 past 1 s, at 1.03 to 1.79 s. Each report's p95 is over 22 to 38 answers, so its second-slowest answer is its p95. `load.py` started every report reader and agent at the same moment, and each sent its first read at once. In three runs that kept each answer's start, the twelve slowest answers each began in the first 1.5 s, each beside 7 to 9 other reports. Past the first 2 s, every report's p95 was within 0.83 s.

**The writes passed in five runs and missed in five**, the misses at p95 106.8 to 126.4 ms, with the machine's cores idle. Writes alone at the same rate missed in seven runs of eight. The writer's round trips bound it, and cutting more of them is open (`TODO.md`, *Write scale past one writer*).

## Decision

1. **Gate P's report readers and agents start spaced.** Each pauses before its first read as before every later one, its pause drawn alike. The pauses are exponential, so the time to a reader's first read is distributed about as from any later moment of the run. The gate thus plays the paced peak from its first second. `scripts/profile/load.py` does so. The sessions start together as before: each of their classes holds hundreds of answers or more in a run, so a start's few answers do not set its p95.
2. **Every reader at once, as after a restart, is the soak's.** ADR-0204 §1 counts a restart among the profile's bursts, and the soak plays the profile (ADR-0204 §7), as `load.py`'s scope already said. The soak gates 1.0.0 (ADR-0214 decision 2). `operate.md` states what every reader at once cost.
3. **The next release accepts by name the writes' miss at the peak on PostgreSQL**, passing in about half the runs on the gate's machine with its cores idle (ADR-0214 decision 2). `operate.md` states it. The first deployment measures it again on its own hosts, as it does 0.3.0's metrics (`TODO.md`, *Author: Gate P on PostgreSQL*). Cutting the write's round trips goes on beside it.

## Alternatives rejected

- **The gate keeps every reader at once, and the next release accepts the reports' miss by name.** The miss would be the gate's start, which the profile's paced peak does not give. Each release would accept a miss that names nothing to fix in the reports.
- **The reports made cheaper first.** Past the start they met C5 already. Alone, a report's cost is mostly the service's CPU, 101 to 476 ms of 126 to 556 ms. Making them cheaper stays open, measured on its own.
- **The run's first seconds left out of the result**, as a warm-up. The burst would still run, and its cost would be hidden rather than gone, as a summary skipping a run's start once hid it (`docs/LESSONS.md`). Nothing would set how many seconds to leave out.
- **Each reader's first read at a uniform point of its first 10 s.** It spaces the start about as well. Drawing the first pause as every later one needs no second rule.
- **The next release waits until the writes pass at the peak.** No cut measured yet does so. The first deployment's hosts, not this machine's idle cores, set what its peak needs.

## Consequences

- `load.py` pauses each report reader and agent before its first read, and its docstring says so. `CHANGELOG.md` lists the change under the next release.
- `operate.md`'s Capacity section gives the peak's reports with the readers spaced, and what every reader at once cost.
- `TODO.md` records both decisions and the runs measured after the change in *Gate P at the month-end peak*. The next release's gate names the writes' miss.

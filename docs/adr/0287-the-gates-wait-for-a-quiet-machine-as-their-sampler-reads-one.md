# ADR-0287: The gates wait for a quiet machine as their sampler reads one

- **Status:** **Decided** by the author 2026-11-18 ("go with your recommendations"), on the point put the same day, with a recommendation, for the finding that the gates' wait for a quiet machine read the 1-minute load average, found in 0.30.0's release gate and addressed at the author's word ("address the three follow-ups in TODO"). Built the same day.
- **Date:** 2026-11-18
- **Refines:** ADR-0235 (a Gate P run that other work shared is run again); ADR-0236 (Gate I alike)
- **Relates to:** ADR-0258 (the gates keep their machine); `scripts/release/gates`; `scripts/profile/load.py`

## Context

Before each attempt of a Gate P or Gate I run, `scripts/release/gates` waited for the machine's 1-minute load average to fall under 1.0, checked every 20 s. The run's own sampler then decided whether other work had shared it: past 0.5 threads in any 10 s of the run (ADR-0235 decision 2).

ADR-0235 rejected the load average for deciding a run, since it averages over a minute and lags a burst of 10 s, but left the wait before a run as it was. On 2026-11-17 and 2026-11-18, in 0.30.0's release gate, twelve attempts of Gate P on SQLite began under a 1-minute load of 1.0 and were each left undecided, other work at 0.68 to 6.59 threads in its busiest 10 s: another project's compiles, a Python process, and other sessions. A wait on the busy ticks the sampler reads, 300 s in which no 10 s passed 0.4 threads, then decided both runs at their first attempts (`TODO.md`, *F. The release gate*).

## Decision

1. **The wait reads the machine's busy ticks**, from `/proc/stat` each second, as `load.py`'s sampler does, by `scripts/profile/quiet.py`.
2. **A machine is quiet once no 10 s of the last 300 s had its threads busy past 0.4.** 10 s is the window the run's sampler judges. 0.4 lies below the run's bound of 0.5, so other work at the wait's bound leaves the run room. 300 s is the span measured to work on 2026-11-18.
3. **The run's sampler still decides each run**, and a run that other work shared is run again, three attempts at most, as before. The wait gives up after `--wait` seconds, 10,800 by default, and the run is then not decided.

## Consequences

- On an idle machine each attempt now waits 300 s before it begins, where it began at once: about 30 minutes more for a release gate's six runs.
- If the gate's machine is kept free of other work, as ADR-0258 asks, a shorter span may serve, measured first by a gate whose runs are each decided at their first attempt.

## Alternatives rejected

- **The 1-minute load average, as before.** It lags a burst of 10 s, as ADR-0235 says, and twelve attempts began under it beside work the sampler then counted.
- **A span as long as a run, 150 s.** It halves the wait's cost, but no gate has measured it; 300 s is the one measured.
- **No wait, the sampler alone.** Each attempt beside other work costs a run of 150 s and its store's copy, and three in a row leave the run undecided.

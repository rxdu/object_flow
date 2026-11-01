# ADR-0235: A Gate P run that other work shared is run again

- **Status:** **Decided** 2026-11-01 under the author's direction ("go with your recommendations"), given after the question was put in `TODO.md` the same day.
- **Date:** 2026-11-01
- **Refined by:** ADR-0236 — Gate I samples and refuses alike, and the sampler takes a last sample at its stop, weighing each sample by its length
- **Relates to:** ADR-0214 decision 2 (a release's gates, each miss accepted by name); ADR-0234 (Gate P's readers start spaced); `design/audit/cost.md` §3 (Gate P); `research/write-throughput.md` §6; `docs/LESSONS.md`, "A quiet start is not a quiet run"

## Context

A Gate P run's result is read as the service's. Before each run, the gate waited for the machine's load average to fall below 1.0. It saw nothing that started during the run (`TODO.md`, *Gate P at the month-end peak*).

**Other work moved the result both ways.** On 2026-10-31, six runs at scale 2 on PostgreSQL measured the machine's threads busy over each run. Two of them slowed in one 11 s window each, where reports took about 1.5 times as long as elsewhere in the run. Those two had 2.79 and 3.04 threads busy, against 1.43 to 1.61 in three others and 2.11 in the sixth. In one of the two, the system journal showed another workload's image build and container starting. The two passed the writes, which the three idlest runs failed.

**At rest, other work used little.** Three later runs sampled each process's CPU every second, from a process of their own. Over each run's 150 s, the service used 0.84 threads, its database 0.68 to 0.69, and `load.py` with the sampler 0.13 to 0.14. Every other process together used 0.06 to 0.07 threads over the run, and 0.07 to 0.19 in its busiest 10 s, sliding by the second.

**`load.py`'s docstring said, from 2026-10-14, that a shared machine gives higher numbers than a quiet one.** The runs above contradict it for the writes. It now says that other work moves the result both ways.

**Practice.** pyperf records, among a benchmark's metadata, the load average over a minute and the runnable threads at a moment. For stable results, it advises isolating CPUs and checking the isolation by a run beside a synthetic load. hyperfine, when a command's first run is an outlier despite a warmup, suggests running it again on a quiet system. In the pages read, neither samples other work through a run (Sources, below).

## Decision

1. **`load.py` samples the machine through each run, and prints with its result the threads each part used.** Each second it reads the machine's busy ticks from `/proc/stat` and each process's from `/proc/<pid>/stat`. It prints the threads busy over the run: the service's, its store's server's (processes named `postgres`, for a PostgreSQL store), its own, and other work's. It also prints other work's busiest 10 s and the processes that used it. Other work is the machine's threads less the run's own. A process that starts and ends between two samples, as a build's compiler does, is thus counted, though not named.
2. **A run that other work shared is not decided, and is run again.** Other work shares a run where it passes 0.5 threads in any 10 s of the run, sliding by the second. `load.py` then says that the run is not Gate P's result and exits 3. Its numbers print as measured, each miss as `MISSED`. A run in which `/proc` cannot be read is not decided either.
3. **The window is 10 s.** A report's p95 is over 22 to 38 answers, so its second-slowest answer sets it. A burst of 11 s set it on 2026-10-31.
4. **The bound is 0.5 threads, a choice.** At rest, other work reached 0.19, and 0.20 with a second sampler beside the run (Consequences, below). The two runs other work moved had 1.2 to 1.6 threads more busy over the whole run than the three idlest, by work not sampled. The bound lies between. A run costs about 3 minutes, so the bound runs a run again rather than keep a doubtful result. If a quiet run on the gate's machine passes it, the bound is measured again.

## Alternatives rejected

- **The gates run only on a machine kept free of other work, by a procedure.** The gate's machine is shared, and a procedure leaves no record that it held through a run. The 2026-10-31 runs began quiet by the procedure then followed, and did not stay quiet. The samples check what a procedure would promise.
- **As before.** A release's result could be the other work's, in either direction.
- **The load average, at the start and at the end.** It averages over a minute and counts the run's own threads. It lags a burst of 10 s, and it cannot tell the run's threads from others'.
- **A mean over the run.** Over 150 s it dilutes a burst of 10 s fifteen times, while a burst that long can set a report's p95.
- **Each process's samples alone.** A process that lives less than a second is seen in at most one sample, often in none. Beside processes living 0.3 s each, they saw about 0.4 of the 3 threads those used (Consequences, below).
- **A shared run fails, exiting 1.** It would say the service missed C5 where its machine was shared, and a shared run's pass would still stand.
- **The service and its database pinned to isolated cores**, as pyperf advises. Isolating cores takes a kernel parameter at boot (`isolcpus`) on a machine others use. `operate.md` sets no CPU affinity for the service, so the gate would measure a host the guide does not describe.

## Consequences

- `load.py` samples each run and prints its parts, and a shared run exits 3. Its docstring says so, and `CHANGELOG.md` lists the change under the next release.
- `TODO.md`'s *F. The release gate* runs again a Gate P run that exits 3.
- `docs/LESSONS.md`'s "A quiet start is not a quiet run" stays for Gate I and the ceiling runs, which do not sample.
- A sample of 337 processes costs about 6.5 ms of `load.py`'s CPU each second on the gate's machine.
- **Checked on 2026-11-01**, the release build of `3b02188` on a PostgreSQL store of 2,000 claims on `scripts/setup/postgres-gate-db`, at scale 2 for 90 s a run. The store is small, so the latencies are no gate result. Each run had a sampler of its own beside it, in another process, as its oracle.
  - At rest, other work used 0.14 threads over the run and 0.20 in its busiest 10 s, the oracle's sampler among it, and the run was decided. The service used 0.51 threads and the database 0.61, against 0.50 and 0.59 by the oracle over its own 95 s.
  - Two busy loops from 30 s to 50 s: other work's busiest 10 s was 2.08 threads, `python3` named for 2.00, and the run exited 3.
  - Three lanes of processes each living 0.3 s, from 30 s to 50 s: other work's busiest 10 s was 3.09 threads, `sh` named for 0.39 of them, and the run exited 3.
  - `/proc` made unreadable to the sampler: the run said it was not sampled and exited 3.

## Sources

| Source | Retrieved | Version |
|---|---|---|
| pyperf, [`doc/api.rst`](https://github.com/psf/pyperf/blob/main/doc/api.rst), metadata `load_avg_1min` and `runnable_threads` | 2026-11-01 | `main` at `00556e1` |
| pyperf, [`doc/system.rst`](https://github.com/psf/pyperf/blob/main/doc/system.rst), "CPU pinning and CPU isolation" and "Check stability of a benchmark" | 2026-11-01 | `main` at `00556e1` |
| hyperfine, [`src/output/warnings.rs`](https://github.com/sharkdp/hyperfine/blob/master/src/output/warnings.rs), the first run's outlier warning | 2026-11-01 | `master` at `c516152` |

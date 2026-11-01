# ADR-0236: Gate I is not decided on a shared machine either, and `throughput.py` says the other work beside it

- **Status:** **Decided** 2026-11-01 under the author's direction ("go with your recommendations"), on the recommendation given the same day for the question `TODO.md` put.
- **Date:** 2026-11-01
- **Refines:** ADR-0235 (a Gate P run that other work shared is run again)
- **Relates to:** ADR-0214 decision 2 (a release's gates); `design/audit/cost.md` §3 (Gate I); `core/README.md` (`throughput.py`'s figures); `docs/LESSONS.md`, "A quiet start is not a quiet run"

## Context

ADR-0235 made Gate P sample the machine through each run, and leave undecided a run that other work shared. Two other scripts measure on the same machine:

- **Gate I** (`idle.py`) times each read of the profile alone, five times, and fails where a read's median passes C5. It is a release's gate too (ADR-0214 decision 2). How far other work moves its results is not measured. *Inferred:* a burst of other work across three of a read's five runs sets its median, as a burst sets a report's p95 in Gate P.
- **`throughput.py`** measures the write and import figures `core/README.md` states. They are a measurement stated with its conditions, not a gate.

**Correction.** `TODO.md` and `docs/LESSONS.md` named `throughput.py` as the writer's ceiling runs, from `0ea96d5`. The ceiling runs played `load.py --writes` (`research/write-throughput.md` §6), which samples from ADR-0235.

## Decision

1. **Gate I samples each run, and a run that other work shared is not decided.** `idle.py` prints the threads busy by part and other work's busiest 10 s, as `load.py` does. It exits 3 where other work passes 0.5 threads in any 10 s, or where `/proc` cannot be read. The window and the bound are Gate P's (ADR-0235 decisions 3 and 4), not measured for Gate I.
2. **`throughput.py` prints the same parts beside its figures, and refuses nothing.** Where other work passes the bound, it says so.
3. **One sampler serves the three scripts:** `load.py`'s `Machine`. It samples once more at its stop, so a run shorter than a second is sampled too. It weighs each sample by its length, so the last, shorter sample counts as long as it lasted. A run shorter than 10 s is its own window.

## Alternatives rejected

- **Gate I alone.** `throughput.py`'s figures would lack the one condition that moved Gate P's, and they move with it (Consequences, below).
- **Neither, as before.** Gate I would decide a release on a run other work may have set.
- **A window and a bound of Gate I's own.** Nothing measured would set them. Gate P's stand until a Gate I run beside other work is measured.
- **`throughput.py` refusing a shared run.** Its figures are no gate, and a measurement beside other work stays one, said with its conditions.

## Consequences

- `idle.py`'s and `throughput.py`'s docstrings say so, and `CHANGELOG.md` lists the change under the next release.
- `TODO.md`'s *F. The release gate* runs again a Gate I run that exits 3.
- `docs/LESSONS.md`'s "A quiet start is not a quiet run" stays for one-off measurement scripts, which do not sample.
- A ceiling run beside the busy loops that keep the cores awake (`write-throughput.md` §6) exits 3, its numbers printed as measured.
- **Checked on 2026-11-01**, the release build of `0ea96d5`'s core, on stores of 2,000 claims and 200 employees `build-store.py` built, so the latencies are no gate result:
  - Gate I at rest on SQLite passed, other work at 0.05 threads; beside two busy loops it exited 3, `python3` named for 2.00 threads; with `/proc` made unreadable it exited 3. On the small store a Gate I run lasted 1.5 s. Before the sampler took a last sample at its stop, a run shorter than a second would have had none.
  - On PostgreSQL, before that last change to the sampler, Gate I at rest passed, the store's server at 0.24 threads.
  - `throughput.py` at rest printed other work's busiest 10 s at 0.40 threads, 0.24 of them the session running the checks, and refused nothing. Beside two busy loops it printed `SHARED` and exited 0. Its writes there ran 136 and 469 a second, from one client and from eight, against 96 and 261 at rest.
  - Gate P with the shared sampler passed at rest and exited 3 beside two busy loops.

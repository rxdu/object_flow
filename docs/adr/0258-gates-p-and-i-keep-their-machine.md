# ADR-0258: Gates P and I keep their machine, and development moves off it

- **Status:** **Decided** 2026-11-05 under the author's direction ("go with your recommendations"), on the recommendation put the same day in `TODO.md`.
- **Date:** 2026-11-05
- **Relates to:** ADR-0214 decision 2 (a release's gates); ADR-0235 and ADR-0236 (a run that other work shared is not decided); `design/audit/cost.md` §3 (Gates P and I); `core/README.md`, *Releases* (`scripts/release/gate`)

## Context

Gates P and I hold each release to PRD C5's targets at the profile, and each release's figures are compared with the release's before (`TODO.md`, *F. The release gate*). Every figure since 0.3.0 was measured on one machine: an i7-8700 of 6 cores and 12 threads, NVMe, 31 GiB, with PostgreSQL in Docker beside the service.

**Other work held 0.17.0's gate.** On 2026-11-05 another project's browser tests ran on that machine for most of the morning. Ten runs were left undecided, nine beside those tests, and the gate was stopped from 11:05 to 13:15 until the machine was quiet. The undecided runs' slowest reports' p95 were 709 to 1,212 ms. The decided runs on the same build and stores gave 595.5 ms at scale 1 and 686.7 ms at scale 2 on SQLite (`TODO.md`, *0.17.0's Gates P and I*).

**The author moves development to another computer**, which also runs the application, and asked how to keep the gate consistent. The gate's scripts are in the repository from the same day (`scripts/release/`), so either machine can run it.

## Decision

1. **Gates P and I keep their machine**, the i7-8700 above. A release's gate runs there, with `scripts/release/gate`.
2. **Development moves off it.** While a gate runs, the machine runs no other work: no other project's tests, builds or containers, and no other agent sessions doing heavy work.
3. **The sampler still decides each run** (ADR-0235, ADR-0236). A quiet machine is what the gate needs, and the sampler is the record that it was.

## Alternatives rejected

- **The gates on the development computer.** Gate P's bounds are absolute, so a faster machine would hide a regression under them and a slower one would fail with no code change. That machine would first need its own baseline, the published previous release's Gates P and I measured on it. And it also runs the application, so its runs would often be undecided.
- **A looser bound on other work.** The undecided runs show other work moving the figures by up to 1.8 times. A looser bound would let a release's result be the other work's.
- **The service pinned to cores.** ADR-0235 rejected it: `operate.md` sets no CPU affinity, so the gate would measure a host the guide does not describe.

## Consequences

- `TODO.md`'s question on the gate's machine is closed by this decision. `CLAUDE.md` points here.
- Three changes were put with the recommendation and are not decided here; each would need its own ADR: each release measured beside the previous one in the same session, so a regression is told apart from the machine; runs longer than 150 s, since a report's p95 is over 10 to 38 answers; and a fixed CPU frequency, its effect not measured.
- Should the gate's machine change, a new baseline comes first: the published previous release's Gates P and I on the new machine, recorded with its hardware.

# ADR-0153: The core's first slice

- **Status:** Accepted, 2026-10-04, at the author's direction ("go with your recommendations"), given the recommendation of a first slice for the Rust core: a SQLite store, the expression language, a request on one object with its guards and invariants, the reads, and the service's `/v1` and `/sandbox` routes, judged by replaying the design's examples through the Flow Studio.
- **Date:** 2026-10-04
- **Refines:** ADR-0132 (the core in Rust, behind an HTTP service), ADR-0143 (its YAML parser and decimal type), ADR-0150 (the wire it serves)
- **Relates to:** `design/storage-schema.md` §2, §3; `DESIGN.md` §6, §10; `design/declaration-syntax.md` §8; `design/flow-studio.md` §6, slice 4

## Context

ADR-0132 chose Rust for the core, with the Python checkers as its independent oracle, and ADR-0143 settled its YAML parser and decimal type. No engine code exists. The Flow Studio now plays every example of the design through the engine's wire against the Python reference runner (ADR-0149 to ADR-0152). So the studio can play them against the core as soon as the core serves the same wire. Any example on which the two disagree is then a finding against one of them.

## Decision

1. **A Cargo workspace in `core/`, of two crates.**
   - `objectflow-core` is the engine, synchronous, as ADR-0132 decision 1 requires.
   - `objectflow-service` is the HTTP service and the one asynchronous layer.

   The toolchain is pinned by `core/rust-toolchain.toml`, with `rustfmt` and `clippy`. Lints are set once in the workspace, and `core/scripts/check` runs the format check, clippy with warnings as errors, and the tests (the author's `rules/rust.md`).
2. **Its dependencies, each for one thing:**
   - **`saphyr-parser`** reads YAML under the core's own strict loader (ADR-0143).
   - **`rusqlite`**, with SQLite compiled in (`bundled`), is the store, so the SQLite the store runs on is the core's release and not the host's.
   - **`serde` and `serde_json`** carry the wire's JSON.
   - **`axum` and `tokio`** serve it.

   Decimals are the core's own 128-bit fixed point, with no crate (ADR-0143).
3. **What the first slice runs:**
   - **Types:** their attributes, states, conditions and invariants.
   - **Transitions:** initial, external and internal, from listed states or `any`, `only_via` included.
   - **A request on one object, in the order of refusals** (`DESIGN.md` §6):
     - the actor;
     - existence, the transition, the state;
     - the inputs;
     - the generated guards on them;
     - the transition's own guards, which deny, audit or warn;
     - the outcome's state and the attributes its inputs write;
     - the declared invariants, each state's required attributes, and uniqueness.
   - **The reads:** `get`, `history`, `events`, `declaration` and `availability` (ADR-0152).

   A request that reaches anything beyond this is answered as not run, a `501` naming the construct, and is never guessed (ADR-0150). That covers an effect, a cascade, a machine, a family, an observation, an assertion, an erasure, a sequence, a metric, an evaluator and supersession.
4. **The store follows `storage-schema.md`:**
   - `of_object`, `of_event` and `of_counter`;
   - one `t_<type>` table of typed columns per declared type;
   - absence as `NULL`, decimals scaled to integers, money in minor units, timestamps as ISO-8601 UTC text.

   A sandbox is an in-memory database, made anew on each load and reset (PRD F13).
5. **The core does not check a module before it loads it in this slice.** The flow checker stays what checks one: the studio's editor and its `load` both run it. The core refuses only a module it cannot read, and the publish checks come with publishing.
6. **The slice is judged by the studio's replay.** Every example of the design is played through the core's service as through the reference runner's. **None may fail**: each passes with the runner's outcome, or is not run, naming what it reaches. A failure is a finding against the core or the runner, settled by the specification (ADR-0132 decision 2). The count of examples run is recorded with the slice, and each later slice raises it.

## Where the first slice stands

Recorded 2026-10-04, when the slice was built.
- **Outcome:** of the design's 163 examples, the core passes 21 with the reference runner's outcome, fails none, and does not run 140. The remaining 2 wait for importing, as they do against the runner.
- **What the not-run examples reach**, by count:
  - an identifier minted from a sequence: 57;
  - an effect: 48;
  - an observation kind: 16;
  - a machine a type binds: 12;
  - a request's occurred time: 4;
  - an object's intervals: 3.

  Those are the next slices.
- **The ratchet:** a replay judged only by "none fails" let a broken state check through, the example it decided turning from passed to not run. So `apps/flow-studio/server/tests/core_passing.txt` lists every example the core passes, and one that stops passing fails the replay.

## The later slices

Each later slice is recorded here with the replay's figures when it was committed, of the same 163 examples, 2 of which wait for importing throughout. None has failed.

| Date | Slice | Passed | Not run |
|---|---|---|---|
| 2026-10-04 | The first slice | 21 | 140 |
| 2026-10-05 | Identifiers minted from sequences | 21 | 140 |
| 2026-10-05 | Effects and cascades, a request taken depth-first | 55 | 106 |
| 2026-10-05 | Supersession, and a sandbox's evaluators | 76 | 85 |
| 2026-10-05 | The interval index, and the flow data read from it | 97 | 64 |
| 2026-10-05 | Observation kinds, recorded and read by their subjects | 122 | 39 |
| 2026-10-05 | Shared state machines, bound into their types at load | 138 | 23 |
| 2026-10-05 | A request dated by when it happened, and `whole_open` | 145 | 16 |
| 2026-10-05 | The attribute write index, and `changed_since` read from it | 149 | 12 |
| 2026-10-05 | Assertions, and the admissions they record | 152 | 9 |
| 2026-10-05 | Erasure, its reach and what it redacts | 156 | 5 |
| 2026-10-05 | `referrers`, for a deletion guard | 157 | 4 |
| 2026-10-05 | Families, a type extending a base | 158 | 3 |
| 2026-10-05 | Proposals, filed, decided and invalidated | 161 | 0 |

Minting raised no count: the examples that had stopped at minting went on to stop at an effect. With proposals, every example that runs against the reference runner passes against the core; the 2 that wait for importing wait against both. What the core does not yet do is listed in `TODO.md`, the design and not the replay now saying what comes next. `TODO.md`'s list of the core's next slices keeps those counts.

## Alternatives rejected

- **For decision 1:** one crate, which would let the service's asynchronous code reach the core's; a crate per subsystem now, ahead of any second user of one.
- **For decision 2:**
  - `sqlx`, asynchronous, against a synchronous core;
  - linking the host's SQLite, which makes the store's behaviour a property of the machine;
  - `tiny_http`, which would serve the first slice with fewer dependencies but leave the service to be rewritten when it needs concurrency and an OpenAPI description;
  - `serde_yaml`, believed unmaintained (ADR-0143).
- **For decision 3:** an in-memory store first, which would show verdicts sooner but postpone the storage design's first real test, where the PRD's guarantees live.
- **For decision 5:** porting the flow checker into the core now, which doubles the slice for checks the Python checker already makes.

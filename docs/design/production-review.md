# Is ObjectFlow production-ready? The review before 1.0.0

Status: **NO-GO**, 2026-10-11, at commit `0e734d6`, before release 1.0.0 was cut, at the author's request ("before we cut 1.0.0, I want you to review again and check if it's production ready"). Release 1.0.0 is not cut; the work this review asks for is in `TODO.md`, "Production readiness".

## 1. What "production" meant, and how it was reviewed

One company's live system of record for a business process, hiring for 600 staff or procurement for 400, run by a small team on one Linux machine, with hundreds of users through the team's own application, AI agents acting through the generated tools, and tens of thousands of objects a year, kept for years; the engine behind an application that authenticates users, as designed.

Two agents new to the code reviewed it cold, barred from the earlier reviews and the decisions after them: an **operator** who built the release binary, wrote a unit file, imported two years of a 600-person company's claims (12,000 claims, 36,611 objects, a 126 MB store), measured it under load and ran failure drills; and an **adversary** who attacked the service within the design's threat model, a trusted application in front, a buggy or manipulated agent and client. Each finding below that is marked *reproduced* was reproduced again by the session that commissioned the review, with a probe of its own on a debug build; *read* means confirmed in the code; *measured by the reviewer* means taken from the reviewer's log on the release build (an i7-8700, 12 threads, NVMe, shared with another project's CI, load 5 to 9), not repeated.

## 2. The verdict

**Not production-ready.** The core's durability is production-grade, and both reviewers said so with evidence: 2,456 acknowledged writes survived `kill -9`, a file-size kill and SIGTERM with nothing lost; two processes on one store lost nothing; an evaluator that hangs, answers garbage or is down becomes a `temporal` refusal and starves nothing; online backups take under a second on a 126 MB store; an erasure leaves nothing it erased on disk; input is validated strictly, cursors are sound, concurrency holds invariants. What stands in the way is four kinds of thing: **one request can stop the process**; **one arithmetic path stores wrong numbers**; **cost grows with the store's size**, so a store two years old is already slow and a flow change freezes writes for minutes; and **what an operator needs to run it** (graceful shutdown, a safe restore, refusing a mistyped store, logs and metrics, a release process).

## 3. The findings, each checked

| | Finding | Checked | Severity |
|---|---|---|---|
| F1 | A filter of a few kilobytes, a 3,000-term `or` chain or 600 nested parentheses, overflows the stack and aborts the process; the parser, the evaluator and the drop of the tree all recurse with no depth bound, and the agent's `query_objects` tool takes any filter | reproduced: `stack overflow, aborting`, the service gone | blocker |
| F4 | A decimal computed in an effect is stored at the wrong scale: 2 × 3 in `decimal(18,6)` reads `6000000.000000` on the core; the reference runner keeps `0.111110888889` where `declaration-syntax.md` §8.3 rounds a product half to even to the target's scale, `0.111111` | reproduced on both backends | blocker |
| PERF-SCAN | Reading a one-to-many end loads every object of the related type (`eval.rs:801`, `opposite_end`, `world.of_type`), the index on its column unused; on the 36.7k-object store a `get` of one claim took 0.18 s idle, writes ran at about 8 a second, RSS rose to 1.95 GB with 20 readers | read; measured by the reviewer | blocker |
| PUBLISH-FREEZE | Adding one optional attribute on the large store took 270 s of dry run and 267 s of publish under the writer's lock, every write waiting; the go-live import's batches grew from 8 s to 132 s, the import scanning the store per batch (`import.rs:768-799`) | measured by the reviewer | blocker |
| F3 | Through the wire, an agent drafted, submitted and published a flow version, and retired the operator, after which `actor_active` refuses the operator for good; the generated tools offer no built-in type, so an agent held to its tools cannot | reproduced | major |
| F2 | The samples, `people.yaml` and `customers.yaml`, mark nothing `taken_by`, so an agent is handed `user_leave`, `agent_revoke`, `service_retire` and erasures | reproduced in `tools` | major |
| F6 | `dry_run` and `propose` are read as true only when the JSON boolean `true`: `"dry_run": "true"` installed a version | reproduced | major |
| MAINTAIN | `maintain` checks no actor: an actor the store does not know pruned the attempt log | read (`main.rs`, its actor unused) | major |
| RESTORE-WAL | A restore as `integrate.md` §9 words it, the copy put in the store's place, replayed the old `-wal` onto it: `integrity_check` failed and the service refused to start; a restore into a clean directory worked | measured by the reviewer | major |
| STORE-CREATE | A mistyped `--store` path, or a zero-byte file, becomes a new, empty store at version 0, served | read (ADR-0174 decision 1 creates a store where none is) | major |
| SIGTERM | No graceful shutdown: the process exits about 50 ms after SIGTERM, cutting requests in flight | measured by the reviewer | major |
| OBSERVE | No metrics; `/health` answered from memory, never touching the store; the log has three start-up lines and evaluator failures, no timestamps, nothing for a 5xx, a publish, an upgrade or a shutdown | read; measured by the reviewer | major |
| CLOCK-PIN | After the host's clock ran a year ahead and came back, every event was stamped the same future instant, the store's clock never moving backwards (ADR-0174 decision 4) | measured by the reviewer | major |
| RELEASE | No CI configuration in the repository, though `core/scripts/check` says CI runs it; no tags, artifacts, checksums, CHANGELOG, security policy or advisory scanning; no test at a realistic store size | read | major |
| F7 | An erasure erases declared personal values only: text in a declared `reason` not marked `personal`, a request's `context`, and a refused request's unmarked inputs keep what a clerk typed; a declared input can be marked `personal` already, `context` cannot | reproduced in part (marked values are erased; unmarked text is kept, as designed) | major (documentation and a checker notice) |
| F8 | The agent's read tools return personal attributes, reads authorizing nothing by design | read | minor |
| F5 | An integer overflow in an effect answers `501 not run`, with no remedy | reported | minor |
| | The 2 MiB body limit is undocumented and fixed; no per-attribute length bound; store files are created 0644; idle and half-sent connections are never timed out; a panic would poison the writer's lock until a restart; an evaluator failure logs its arguments and the endpoint's body, personal data among them; no `--version` | read; measured by the reviewers | minor |

## 4. What it would take

The blockers first, each with a test at the size or the input that broke it: F1, a depth bound on every expression a request or a module gives; F4, the rounding §8.3 states, on both backends; PERF-SCAN, a one-to-many end read by its index, then the publish's dry run and the import measured again on the 36.7k-object store, since both are expected to fall with it; and a test of request, read, publish and import times at that size, so a regression shows. Then the majors, then a production review run again on the same store before 1.0.0 is cut. `TODO.md` holds the list, and the author's decisions it needs (§5).

## 5. Questions for the author

1. **Hold 1.0.0 until the blockers and the majors are fixed and a second production review finds none?** Recommended: yes; the promise ADR-0180 makes is worth more on a release that runs.
2. **Where CI runs.** The repository has no CI configuration, and the author's machine runs a GitHub Actions runner for another project. Recommended: a GitHub Actions workflow running `core/scripts/check`, the design checks, the Studio's and the web app's tests and the scenarios' replay, on every push, on that runner or GitHub's.
3. **What a deployment's store does when its path holds none.** Recommended: refuse unless `--create` is given, so a mistyped path never becomes an empty store.
4. **Authentication on the service.** By design it has none (ADR-0114), and both reviewers named it. Recommended: keep the design, and bind to a Unix socket as well as loopback, with the socket's permissions as the boundary, so only the application's user reaches it.

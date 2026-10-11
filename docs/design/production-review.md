# Is ObjectFlow production-ready? The review before 1.0.0

Status: **NO-GO**, 2026-10-11, at commit `0e734d6`, before release 1.0.0 was cut, at the author's request ("before we cut 1.0.0, I want you to review again and check if it's production ready"). Release 1.0.0 is not cut; the work this review asks for is in `TODO.md`, "Production readiness". **The second review (§6), at commit `cd1b536` after the fixes, is NO-GO too**, as ADR-0183 decision 1 required it to find nothing.

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

## 6. The second review, after the fixes

At commit `cd1b536`, 2026-10-11, as ADR-0183 decision 1 requires before 1.0.0 is cut: two new cold reviewers, an operator and an adversary, on copies of the same 37,154-object store (39,443 events, flow version 3, its `-wal` empty when copied, so complete), the release binary built at that commit, each given the first review's findings to re-check and barred from the decisions on them (ADR-0183 among them). Their logs, reproductions and reports are in the session's scratchpad (`pr2/operator`, `pr2/adversary`); the machine was shared with another project's CI, at load 1.2 to 7.0. **Both: NO-GO.**

### The first review's findings, re-checked

Fixed, measured by one or both: F1 (deep and long filters refused in milliseconds), F4, F2 for the two samples it named, F6 for the three keys it named, MAINTAIN, RESTORE-WAL as the guide now words it, STORE-CREATE for an absent or empty path, SIGTERM, the body limit, file modes, the evaluator log, `--version`; PERF-SCAN for what it named (a `get` 0.9 ms, the 50-claim worklist 9.7 ms, 49 writes and 381 reads a second together); PUBLISH-FREEZE for the `operator` actor (dry run 4 s, approval 6 s, the go-live import 36 s). Partly fixed: OBSERVE, CLOCK-PIN (said everywhere, events still stamped ahead), RELEASE (written, never run), F7 (a correction's reason noticed; inputs under undeclared names kept). Not fixed, by design and now documented: F3, F8. Not fixed: F5, idle connections, the poisoned lock.

### What they found

| | Finding | Checked | Severity |
|---|---|---|---|
| YAML-DEPTH | A module nesting YAML block sequences 300,000 deep (600 KB, under the body limit) overflows the stack and aborts the process; an agent may draft a change (`integrate.md` §6). F1's bound covers expressions; `yaml.rs` has none | measured by the adversary; read (`yaml.rs` has no depth) | blocker |
| DRYRUN-ACTOR | A publish's dry run, preview or approval by any actor but `operator` holds every write for minutes, growing quadratically: an Employee's submit still running after 18.5 minutes at 39.8k objects, an agent's after 32 minutes; the same change as `operator` 9.8 s. It computes the submitting actor's offers for every object in both versions inside the writer's transaction | measured by the operator; read (`publish.rs` `clauses_and_offers`) | blocker |
| DRYRUN-LOCK | A submit's dry run runs under the single writer lock, unbounded: a 3,000-type module (1.45 MB) held every write 83 s, the checker superlinear in the module (500 types 2.5 s) | measured by the adversary | blocker |
| ERASE-CHECKPOINT | After an erasure, every request retries `PRAGMA wal_checkpoint(TRUNCATE)` under the writer lock, waiting the 5 s busy timeout while any read older than the erasure is open: an ordinary metric read made every write take 5 s for 74 s, and the erased values stayed in the `-wal` that long | measured by the adversary; read (`store.rs` `after_erasure`) | blocker |
| READ-COST | `available`, filters on derived attributes and per-person metrics read a type superlinearly: at 12k claims `available approve` 41 s, `query total > USD 1000.00` 84 s, a metric 77 s; a 40-deep nested `sum` ran past 186 s; `events` with one position repeated 200,000 times built 2.8 GB; nothing bounds a read's time or memory, and a disconnected client's read runs on | measured by both | major |
| MEMORY | Memory rises to several GB with publishes and is never given back: 334 MB to 1.79 GB on one publish, 3.5 GB peak, 1.44 GB after 45 minutes idle | measured by the operator | major |
| OBSERVE-2 | A 21-minute writer stall left `/health` answering 200 in 1 ms and `/metrics` showing nothing in flight; `/health` answered 200 with the disk full and with an upgrade due; a 5xx's line carries no reason | measured by the operator | major |
| RELEASE-2 | Nothing pushed, tagged or run; the private vulnerability reporting `SECURITY.md` names is disabled; the advisory scan runs only on a push | read by the operator from GitHub | major |
| TYPES | A value of the wrong JSON type is still read as absent outside the keys F6 fixed: a non-string `filter` returns every object unfiltered, a numeric `cursor` the first page again, a non-object `bind` the whole metric; a non-string `principal` is dropped; a numeric `object_id` beside a `type` creates | measured by the adversary | major |
| CONN | Half-sent, idle and silent connections are never closed (700 open after 70 s); with the descriptors exhausted `/health` times out and nothing is logged | measured by both | major |
| STORE-ACTS | F3 again: an agent published a version and retired the operator, who can then do nothing and cannot be installed again; one missed gateway rule is a silent, total failure | measured by the adversary | major |
| SAMPLE-AGENTS | `reimbursement.yaml` hands an agent `claim_approve`, `claim_pay`, its erasures, onboarding and a policy change; `customers.yaml`'s assertion `correct_state` too, against `write-a-flow.md` §5; the checker says nothing of an unmarked erasure or assertion | measured by the adversary | major |
| ATTEMPT-INPUTS | An input sent under an undeclared name, or to a misspelt transition, is kept verbatim in the attempt log, exported by `<Type>.attempts`, and reached by no erasure | measured by the adversary | major |

Minors, as the reviewers listed them: a foreign SQLite file is converted and served without `--create`; start errors in Rust's `Debug` form; backups made with the guide's commands are `0644`; a clean stop leaves a 9.9 MB `-wal`; with no retention flag every prune is refused `inside_retention` without saying why; one log line per answer passes journald's default rate limit at 430 answers a second; a second copy given the same `--socket` takes the live socket over; arithmetic failures answer `501` or `500` (F5); money computed in an effect can exceed the wire's 18 digits; a timestamp past year 9999 is answered in a form no RFC 3339 parser reads; timestamps with milliseconds are refused; lenient date and decimal forms are accepted; no bound on an idempotency key's or a text input's size; extra keys inside `actor` are ignored; `wire.md` names the export source `events` where the service takes `log`; a refused `submit` stores no report; the evaluator call has no credential option, so a secret goes in its URL, which is logged.

### What the second review asks

The blockers and the majors fixed, each with a test at the size or input that broke it, then a third production review. The adversary's point about ADR-0180 holds: several fixes change a status code or refuse what is accepted now (module bounds, the typed keys, a bound on reads), which the promise would make a major release's after 1.0.0, so they are made before it. `TODO.md` holds the list, and the questions it needs of the author.


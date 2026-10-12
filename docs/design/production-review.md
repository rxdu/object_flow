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

At commit `cd1b536`, 2026-10-11, as ADR-0183 decision 1 requires before 1.0.0 is cut: two new cold reviewers, an operator and an adversary, on copies of the same 37,154-object store (39,443 events, flow version 3, its `-wal` empty when copied, so complete), the release binary built at that commit, each given the first review's findings to re-check and barred from the decisions on them (ADR-0183 among them). Their logs, reproductions and reports stayed in the session's scratch directory, which is not kept, so this section is their record; the machine was shared with another project's CI, at load 1.2 to 7.0. **Both: NO-GO.**

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


## 7. The third review, after the second's fixes

At commit `77ed874`, 2026-10-12: two new cold reviewers, an operator and an adversary, the release binary built at that commit, each given the second review's findings to re-check and barred from the decisions on them, working in a detached checkout of the commit, which the repository's later changes did not reach. The operator worked on copies of the same 37,154-object store ("1x"), a fresh two-year go-live, and a six-year go-live it built to measure growth, 109,602 objects in 387 MB ("3x"). The harness refused the adversary reading the store's objects, which hold names, so its scale and privacy numbers are on a synthetic store of about 12k claims it built from the sample flows. Their logs stayed in the session's scratch directory, which is not kept; this section is the record of what they measured.

**Verdicts: the operator NO-GO, the adversary GO from its reach with C-1 fixed or accepted.** Neither met a panic, a crash, a hang or a lost acknowledged write: kill -9 during writes lost none of 532 acknowledged, a killed approval rolled back, online backups, a full disk, the evaluator's failures, two services on one store and every hostile input held.

### The second review's findings, re-checked

Fixed, measured by one or both: YAML-DEPTH, ERASE-CHECKPOINT's write latency, OBSERVE-2, TYPES (but PRINCIPAL below), CONN as written (but C-1 below), STORE-ACTS' retirement, SAMPLE-AGENTS, ATTEMPT-INPUTS; READ-COST at 12k (0.2, 0.93 and 1.4 s, each linear per page); of the minors, a foreign file, start errors (but a missing directory and a long socket path name no path), a clean stop's `-wal`, a live socket, backups' modes, money past 18 digits, `actor`'s keys and the export source's name. Partly fixed: DRYRUN-ACTOR and DRYRUN-LOCK, a person's and the operator's publish now costing the same and a submit's dry run running beside the writes, but an approval and a `dry_run: true` preview still hold every write, about 6 s at 1x and 36 to 38 s at 3x, and a submit whose dry run passes the read deadline computes it again under the lock (PUBLISH-SCALE); a read bounded in time, not in memory; MEMORY (MEMORY-2); timestamps, milliseconds and offsets refused but a year past 9999 parsed; journald's limit documented, one line an answer still. Not fixed: RELEASE-2, which is the author's to act on; an agent's publishing, the gateway's by ADR-0183 decision 7 and ADR-0184; lenient dates, key and text sizes, an unset retention's refusal unexplained, a refused submit's report, the evaluator's credential.

### What they found

| | Finding | Checked | Severity |
|---|---|---|---|
| RELEASE-3 | Nothing pushed, tagged or run: `77ed874` 25 commits past `origin/main`, at `f6ee6d6`; no workflow, run, tag or release on GitHub; private vulnerability reporting disabled. A push on 2026-10-12 did not land (`git ls-remote`) | read by the operator from GitHub; confirmed by `git ls-remote` | blocker |
| PUBLISH-SCALE | An approval and a `dry_run: true` preview hold every write, 6 s at 1x and 36 to 38 s at 3x, six times for three times the data; a submit whose off-lock dry run passes `--read-timeout` computes it again under the lock, unbounded and unsaid (`main.rs` `prepare(…).unwrap_or(None)`, then `request.rs`): at 3x every write held 33 to 37 s from 30.7 s into the submit, so a publish freezes writes about 70 s; SIGTERM's 30 s drain cuts a 36 s approval, which rolls back | measured by the operator; read | major |
| MEMORY-2 | Memory is never given back: 12 MB to 975 MB after one publish at 1x, 2.08 GB after six, 2.48 GB after 45 idle minutes; 3x peaks 6.3 GB; reads alone 1.48 GB at 3x; past `integrate.md`'s sizing | measured by the operator | major |
| PRINCIPAL | `request`'s top-level `principal`, which `wire.md` and `wire-v1.json` list, is taken, untyped, and dropped: any value is satisfied, every event records none, and the principal's `actor_active` is never checked; only `actor.principal` is read | measured by the operator; read (`main.rs`) | major |
| C-1 | Connections answering nothing fill the cap, and every connection after them, `/health` and `/metrics` among them, is reset until the idle timeout closes them: a leaking pool or stalled agents shut out the application and its monitoring for up to 30 s | measured by the adversary; reproduced | major |
| AGENT-APPROVE | An agent approves a `DeclarationChange` and installs a version; "agents draft, people approve" holds only where the gateway refuses it | measured by the operator | major (design) |
| READ-PAGES | A derived filter read whole, page by page, is quadratic: 12.6 s at 1x, 112.6 s at 3x | measured by the operator | major |

Minors: a restore leaving the old `-wal` starts and answers `/health` 200 while queries and writes answer 500, where `integrate.md` says it does not start; the `-wal` grows unbounded (792 MB beside a 133 MB store) and no file size is in `/metrics`; an import batch's time grows with the store (3,000 objects 1.6 s at the start, 12.1 s at 100k); `export` never answers a `null` cursor, against `wire.md`'s pages; `upgrade` by a non-operator answers 200 where none is due, where `wire.md` says 409; `follow` not typed; `decimal(p,s)` enforcing its scale and 18 digits but not `p`; durations only as `30 s`; a full disk reported as `disk I/O error` with `/health` 200; refusals answered 200, so `/metrics` cannot count them; no way to bring a store's clock back; no 0.2.0 upgrade fixture; `release.yml` testing nothing, needing no CI pass and signing nothing; and the minors not fixed above.

### What the third review asks

RELEASE-3 is the author's to act on. The majors fixed, each with a test at the size that broke it, PUBLISH-SCALE and READ-PAGES measured at 3x; AGENT-APPROVE decided by the author; then a fourth review, or the author's acceptance of what remains. `TODO.md` holds the list and the questions.

## 8. The fourth review, after the third's fixes

At commit `092f644`, 2026-10-12, the first commit pushed and run on CI: two new cold reviewers, an operator and an adversary, the release binary built at that commit, each given the third review's findings to re-check and the items the authors had recorded as open, working in a detached checkout, on copies of the same 37,154-object store and of the six-year store of about 110k objects the third review built. Their logs stayed in the session's scratch directory, which is not kept; this section is the record.

**Verdicts: both NO-GO.** The operator found no blocker; the adversary one, fixed the same day. Neither met a crash, a hang or a lost acknowledged write: kill -9 and SIGTERM during writes and publishes lost nothing acknowledged, restart took 0.8 to 1.6 s on six years, 2,838 fuzzed requests answered no 500, writes stayed 8 to 13 ms from two years to six and a 4-minute soak of 1,887 claim lifecycles ended with no error.

### The third review's findings, re-checked

Fixed, measured by both: PUBLISH-SCALE (a submit and a preview beside the writes, the slowest write 0.06 s; an approval 2.9 s on 37k objects and 8.6 to 9.4 s on six years, linear), MEMORY-2 (34 to 60 MB after a publish), PRINCIPAL, C-1, READ-PAGES (8.5 to 12.9 s for every claim of six years, page by page), and every minor named but the ones below. RELEASE-3: pushed, `check-all` and the advisory scan passed on their first runs; private vulnerability reporting was enabled at 16:41 during the review, which one reviewer read before and the other after; no release is cut yet, which is the author's act, and `main` is unprotected. AGENT-APPROVE partly: the approval is refused where a flow declares `publishing.taken_by`, even against a change deleting the line, but neither review store declares it, and an agent installed a version on each.

### What they found

| | Finding | Checked | Severity |
|---|---|---|---|
| A-15 | One `import_batch`, raising an undeclared sequence to `last` = i64::MAX, stopped every write for good: `last + 1` wrapped, and every write's allocator read `next_value - 1` in SQL, which overflowed to a REAL; `/health` stayed 200 | measured by the adversary; read | blocker, fixed the same day (D607) |
| AGENTS-OPEN | An agent's bounds fail open: a flow declaring no `taken_by`, as the review store's, hands an agent every transition through its generated tools, and as an agent the reviewer recorded a bill, raised a policy limit, onboarded people and agents, erased a name, moved an employee to a final state and dissolved a function; the checker says nothing of agents, nor of a version that drops every `taken_by` and the `publishing` rule | measured by the adversary | major |
| READS | Nothing bounds concurrent heavy reads: a per-person metric on six years takes 310 MB alone, 8 at once 1.8 to 2.0 GB, 32 at once 3.3 GB and every one `503 read_timeout`; 16 `available` reads 3.7 GB; read connections open without a cap; `integrate.md`'s sizing would be killed by 12 to 16 such reads | measured by both | major |
| FEED | `pull` and `export` read the whole event log on each call, and `export`'s `log` every object first: an empty pull 1.3 s and 640 MB on six years, an export page about 5 s whatever its size, a whole export about 12 minutes, quadratic | measured by the operator; read (`world.rs` `log`) | major |

Minors: the `-wal` past its limit under sustained heavy load (3.2 to 3.4 GB in minutes at about 100 writes a second with reads always overlapping; held at 64 MiB at 3.7 writes a second); a malformed filter answered 501 and echoed whole into the log; a filter reading an evaluator, `actor` or `inputs`, or dividing by zero, answering an empty page; an idempotency replay confirming an erased value; `acknowledge` taking any cursor; a restore beside another state's `-wal` refused only after the copy is changed; an evaluator URL's password logged; implausible import timestamps taken; a `record` transition offered where it can never be taken; an erasure reading the whole log under the writer; memory given back only after a heavy request; `/health` 200 with the stderr pipe full or every write stale behind another process's lock; `--bind 0.0.0.0` without a warning; a store file moved under the service still written; a full disk answered 500 rather than 503; `--help` without `--evaluator-token`; start lines without a timestamp; `/metrics` without histograms, evaluator failures or refusal kinds; 503 kinds told apart by text alone; no `sd_notify`, no metrics listener beside `--socket`; the README's "not yet for production". The known items (a clock brought back, refusals as 200, durations as `30 s`, unsigned releases, the 0.2.0 fixture) were ranked minor.

### What the fourth review asks

A-15 is fixed, and FEED, which needed no decision, after it. AGENTS-OPEN and READS were the author's, decided as recommended the same day and built (ADR-0189), with `main` protected against force-pushes and deletion; then the minors, and a fifth review, or the author's acceptance of what remains.

# Is ObjectFlow production-ready? The review before 1.0.0

Status: **NO-GO**, 2026-10-11, at commit `0e734d6`, before release 1.0.0 was cut, at the author's request ("before we cut 1.0.0, I want you to review again and check if it's production ready"). Release 1.0.0 is not cut; the work this review asks for is in `TODO.md`, "Production readiness". **Every review since has been NO-GO too**, as ADR-0183 decision 1 requires a review to find nothing: the second at `cd1b536` (§6), the third (§7), the fourth (§8), the fifth at `879fb2c` (§9), the sixth at `fc6c0e7` (§10), the seventh at `7d050a6` (§11), the eighth at `13f9bbf` (§12), the ninth at `0ce1b3c` (§13), the tenth at `093d743` (§14) and the eleventh at `3d0e07f` (§15).

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

## 9. The fifth review, after the fourth's fixes

At commit `879fb2c`, 2026-10-12, not yet pushed, every gate of `scripts/check-all` passed on it: two new cold reviewers, an operator and an adversary, each in a checkout of its own with the release binary built from it, given the fourth review's findings to re-check, on copies of the same 37,154-object store and the six-year store. Their logs stayed in the session's scratch directory, which is not kept; this section is the record.

**Verdicts: both NO-GO.** The operator found no blocker and one major; the adversary one blocker and six majors. Neither lost an acknowledged write: 1,600 acknowledged through kill -9 and SIGTERM during writes, a publish and an import, none missing; about 2,450 malformed bodies across every operation answered no 5xx.

### The fourth review's findings, re-checked

Fixed, measured by both: A-15 (and a store an earlier release left wrapped repaired by a raise); READS (32 per-person metrics on six years peaked at 1.1 to 1.2 GB, where 3.3 GB); FEED (an empty pull 0.5 to 1 ms; the whole log of 117k events exported in 7.9 s, where about 12 minutes); every minor but the two below. AGENTS-OPEN partly: an agent's tools fail closed, four reads on the review flow, and both notices work; on the wire an agent still takes what a flow leaves open, by design, approving a version among it where no `publishing` rule is declared, which no notice names. RELEASE-3: `main` protected; no required checks, no tag ruleset, actions pinned by tag, releases unsigned. **The refused restore not fixed**: the fix, tested at its function, missed the start's earlier connection, and the documents claimed it whole (D609, resolved the same day). A filter's zero computed per object, or an integer overflow, still answers an empty page.

### What they found

| | Finding | Checked | Severity |
|---|---|---|---|
| BRICK | An effect moving a date or a time past year 9999 wrote text the store cannot read back: `ends_at + 30 days` from the legacy "no end", 9999-12-31, answered satisfied and stored year 10000; then every read of the object and every scan of its type, and every later submit and preview, answered 500, for good, `/health` 200 | measured by the adversary | blocker |
| N-1 | A transition named `submit` or `refresh` on any type took a slot meant for a change's dry run, and worklists shared the slots with per-person metrics: beside 8 to 12 looping metrics on six years every claim submit waited 10 to 29 s, one refused `503 read_timeout`, a 12 ms worklist 17 s and then 503 | measured by the operator; read | major |
| FILTER-COST | One caller's filter starves every heavy read: nested aggregates of depth 6 over a claim's lines took over 30 s each, an `in` of 10,000 values 7.8 s; with 8 such queries a 1 ms worklist took 29 s; an agent's `query_objects` takes a free filter | measured by the adversary | major |
| IDENTITY | An identity unique only within its type: an agent onboarded `Employee agentx`, the operator then registered `AssistantAgent agentx`, which took a `taken_by: [human]` erasure as a human; in reverse a person is taken as an agent; identities not normalised, zero-width and bidi characters distinct | measured by the adversary | major |
| NEW-CORRECTION | A correction cannot take `taken_by`, and a type's `taken_by` is refused on any type with a correction: the agent rewrote a recorded claim's bill id | measured by the adversary | major |
| DRYRUN-MEMORY | Four concurrent submits of a change on six years peaked at 6.2 GB; `check` of a change's submit runs its dry run outside every bound, six at once 9.3 GB | measured by the adversary | major |
| EVENTS | `events` answers a position as often as it is named, outside every read bound: a 7 kB request answered 201 MB, eight at once 5.9 GB | measured by the adversary | major |
| ERASE-LIMIT | A claim with 201 lines cannot be withdrawn, rejected, paid or erased in the review flow, its cascade and its erasure's loop each `limit: 200`, which the checker does not notice; the repository's sample declares the same | measured by the adversary; read | major |

Minors: a stderr reader that exits loses answers for 5 s and leaves `/health` `log_blocked` until a restart; duplicate JSON keys taken, the last winning, an `actor` among them; an agent may reject a person's proposal; a supersession group's erasure growing as the group times its type; an evaluator token sent over `http://` without a word; a metric over a negative window answered empty, negative durations taken; derived times answered past year 9999; bidi and zero-width characters taken in a flow's source; `--create` on an existing store served without a word; `/health` 200 while the read gate refuses writes; `malformed_filter` without a reason; no record of an unclean stop, no build-info metric, no watchdog; argument errors unstamped; the first write behind another process's lock waiting 24 to 40 s; CI's required checks, tag protection, SHA pins, Dependabot.

### What the fifth review asks

The author's standing instruction, "follow your recommendations", decides what the findings ask (ADR-0190); the blocker and the majors are fixed, then a sixth review, or the author's acceptance of what remains.

## 10. The sixth review, after the fifth's fixes

At commit `fc6c0e7`, 2026-10-13, not yet pushed, every gate of `scripts/check-all` passed on it: two new cold reviewers, an operator and an adversary, each in a checkout of its own with the release binary built from it (the operator found it byte-identical to a `--locked` build of the checkout), given the fifth review's findings to re-check, on copies of the same 37,154-object store and the six-year store, and the third review's go-live tools without their runs. Their logs and reports stayed in the session's scratch directory, which is not kept; what they found is here.

**Verdicts: both NO-GO.** The operator found one blocker and two majors; the adversary no blocker and three majors. Neither lost an acknowledged write: `kill -9` and SIGTERM during writes and an approval lost none, idempotent retries applied once; 336 malformed bodies and 2,160 wrong-typed keys across all 21 operations answered no 5xx.

### The fifth review's findings, re-checked

Fixed, measured by both: BRICK; N-1 (16 saturating metrics on six years: writes p99 32 ms, worklists p99 60 ms); DRYRUN-MEMORY (one at a time, peak 1.58 GB where 6.2 and 9.3); NEW-CORRECTION; EVENTS; the refused restore (the file and its `-wal` byte-identical after a refused start); every minor but those below. Partly fixed: FILTER-COST (within the new bounds, a filter calling a metric per object and an `or`-tree of 10,000 comparisons still ran to the 30 s deadline, holding a worklist slot); IDENTITY (other invisible characters, a trailing or non-breaking space, case, NFC and NFD forms, a Cyrillic letter and the empty string still made distinct identities; an identity shaped as another actor's object id was taken; an import's `admit` wrote a second holder); ERASE-LIMIT (the notice's remedy, a bounding invariant, made every write sweep its type, P8 below; an erasure past a limit is still refused); derived times past 9999 (`get` answered null, `query`'s fields did not); a moved store (`/health` 503, a read 500); a supersession group's erasure (no longer the type's size, but superlinear in the group's). Not fixed: `--metrics-bind` refused at the main listener's connections (F9 below); the GitHub settings, the author's and undecided.

### What they found

| | Finding | Checked | Severity |
|---|---|---|---|
| P8 | An invariant reading beyond its own object made every write of the store re-check every object of its type, under the one writer: on a go-live of the shipped `reimbursement.yaml`, whose `lines_within_reach` the fifth review's notice asks for, a draft took 2.8 s and a write to an unrelated type 2.3 s (13 ms without it), 0.4 writes a second with four writers; on six years 7.7 s; a one-hop path invariant, `category.name != ""`, 5.3 s a draft. `DESIGN.md` §5.6 says such an invariant is checked from the related objects; the core swept its type | measured by the operator; read | blocker |
| F9 | `--metrics-bind`'s listener refused `/health` and `/metrics` `503 busy` once the main listener held 16 connections, idle keep-alive ones among them: the count both share was compared with the metrics listener's own cap | measured by the operator; read | major |
| P5, FILTER-COST class | A person's worklist queued behind reads the same pool holds: four agent queries on a derived attribute (3.3 s each on six years) took a 9 ms read to p50 4.9 s, eight to 26 s and past 30 s 503; four of an agent's filters within the bounds held every worklist slot, a person waiting 28.6 s | measured by both | major |
| ERASED-IDENTITY | An erased actor's identity stays in the store and is answered: a person's email as their identity, marked `personal`, erased with them from their object, stayed in the events' actor (7 rows), attempts, intervals' `entered_by` and an idempotency record, in the file and its `-wal`, and `history` and `export` answer it, which `SECURITY.md` names a vulnerability | measured by the adversary | major |
| HISTORY | `history` answers an object's whole history unpaged, outside every read bound: 10,000 events of 16 KB each answered 164.6 MB, six at once took the service to 3.75 GB; `events` of 1,000 large positions answered 16.5 MB, eight at once 572 MB | measured by the adversary | major |

Minors: an ill-typed filter answers `501` and writes a stored value, a personal one among them, into the log; agents on the wire take what a flow leaves open, approving a version among it, which the design leaves to the gateway; a metric in a guard runs under the writer, 0.12 s a submit; the one dry-run slot is not fair, an agent's looping previews holding it; the same idempotency key from an actor named by its object's id and then its identity answered `409`; an evaluator's answer naming a key twice took the last; a division by a money zero and a nested set answered empty pages; a store file the service cannot write starts and answers every write 500; a full disk the volume's free space does not show leaves `/health` 200; the clock pin has no recovery and the guide no `time-sync.target`; a second service on one store gets an ambiguous line; `--version` names no commit; a store an earlier build bricked is not found at start and its fault prints the stored value; an upgrade-due 503 has no `code`; the stalled write's wait is not stated; the CHANGELOG's count of reviews; no tag, release or CI run since `092f644` on GitHub; the binary needs glibc 2.34.

### What the sixth review asks

The author's standing instruction decides what the findings ask (ADR-0192); the blocker and the majors are fixed, then a seventh review, or the author's acceptance of what remains.

## 11. The seventh review, after the sixth's fixes

At commit `7d050a6`, 2026-10-13, not yet pushed, every gate of `scripts/check-all` passed on it: two new cold reviewers, an operator and an adversary, each in a checkout of its own with the release binary built from it, given the sixth review's findings to re-check, on copies of the same 37,154-object store and the six-year store, and the third review's go-live tools without their runs. Their logs and reports stayed in the session's scratch directory, which is not kept; what they found is here.

**Verdicts: both NO-GO**, neither finding a blocker: the operator found one major, the adversary two. Neither lost an acknowledged write: `kill -9` and SIGTERM during writes lost none, retries applied once, `kill -9` during an approval rolled back, restart took 0.33 s; 2,130 malformed requests across every operation answered no 5xx; HTTP framing, smuggling, header and URL limits, the evaluator transport (no redirect followed, the token never logged nor leaked, answers capped at 10 MiB) and both `unsafe` blocks held.

### The sixth review's findings, re-checked

Fixed, measured by both: P8 (on a go-live of the shipped sample with its bound, a draft 12.5 ms and a write to another type 11 to 24 ms, four writers 79.6 writes a second; ten concurrent `add_line` at 199 lines, one applied); F9 (the metrics listener's `/health` 200 in 2 to 4 ms with the main one saturated); ERASED-IDENTITY (the erased actor's rows name its object's id, the file and `-wal` hold no copy); IDENTITY, for every case the sixth review named; every minor re-checked. Partly fixed: P5 (an agent's reads wait in their own pool, a person's worklist 23 ms beside eight agents' sweeps, but `referrers`, below, and between people a filter on a value derived from a set read its type whole on each page, four people's making others wait 5.2 to 5.7 s, ranked minor); HISTORY (paged, but a page bounded by count and not by bytes, 123 MB for 1,000 events of large inputs, ranked minor).

### What they found

| | Finding | Checked | Severity |
|---|---|---|---|
| PUBLISH-SLOW | A publish grew with the square of the actors: adding one optional attribute took 10 to 15 s of submit and 11 to 16 s of approval on 37k objects and 45 to 50 and 48 to 55 s on 110k, where `092f644` took 2.2 and 3.0, 6.6 and 9.4; 70 s of approval, holding every write, with 3,600 actors. Bisected to `a2148f4`, the sixth review's case-folded identity, which replaced the indexed lookup of an identity by a scan of every actor for each actor checked. On six years the submit passed the 30 s read deadline and the approval the 30 s a stop waits | measured and bisected by the operator; read | major |
| REFERRERS | `referrers` read every object of the store for each object it was evaluated on, where `storage-schema.md` §5 names each relationship's stored-end index as its reverse index: a filter `count(r in referrers) > 0` ran to the 30 s deadline at about 1 GB each, four making a person's worklist wait 29.2 s; as a guard on six years 2.94 s a write under the writer, a publish of the guard 130.6 s of submit and 128.4 s of approval; through `availability`, which waits in no pool, 2.5 s a call, eight at once 7.4 GB | measured by the adversary; read | major |
| CUTSTALL | While a cut of the write-ahead log is due, after an erasure or past 64 MiB, every new read that grows with the store waits until the longest one running ends, an agent's among them: after an erasure beside an agent's 30 s sweep a person's worklist took 25.4 s, under writes with agents sweeping 25.7 and 26.1 s, which reopens P5 whenever a cut is due | measured by the adversary | major |

Minors: attempts sent under another case of an erased identity, or after the erasure, keep the text sent, an actor being resolved with its case where uniqueness ignores it; a page of `history` and an answer of `events` bounded by count, not bytes; U+2800, U+17B4, private-use characters and noncharacters accepted in an identity; an import taking `created_by_kind` `"robot"` and an `entered_by` of 70,000 characters; a metric's `keep` naming a dimension 5,000 times running to the deadline; `title == "x" and title` answered empty, and a time computed past 9999 compared in a filter where a date has no value; an answer its client does not read held without a bound, eight holding 750 MB; any caller pulling, acknowledging or revoking another's subscription, which `integrate.md` did not list among what to withhold from an agent; `/sandbox/load` without a deadline; `After=` and `Wants=time-sync.target`, as `integrate.md` said, waiting for nothing without a wait-sync service, and `Wants=` against `systemd.special(7)`; a copy restored beside a stale `-wal` refused with SQLite's "malformed database schema", twice, not the guide's advice; a stalled write answered after 34.5 s where the guide said 20 to 25; a second service on a store said only at its start; a write to an object many others read through re-checking each under the writer, 1.6 to 2 s for a category nine thousand lines read, which is what the invariant asks.

### What the seventh review asks

The author's standing instruction decides what the findings ask (ADR-0194); the majors are fixed, then an eighth review, or the author's acceptance of what remains.

## 12. The eighth review, after the seventh's fixes

At commit `13f9bbf`, 2026-10-13, not yet pushed, every gate of `scripts/check-all` passed on it: two new cold reviewers, an operator and an adversary, each in a checkout of its own with the release binary built from it (the operator found it byte-identical to a source build), given the seventh review's findings to re-check, on copies of the same 37,154-object store and the six-year store, and the third review's go-live tools without their runs; the operator also followed the quickstart and the operate guide as a newcomer. Their logs and reports stayed in the session's scratch directory, which is not kept; what they found is here.

**Verdicts: both NO-GO by the bar, each finding one major and no blocker**; the operator's was GO for a deployment made by this build, its major reaching only a store carried from 0.2.0. Neither met a panic, a crash, a hang, a corrupted store, an erased value returned by a read, or a flow rule bypassed: `kill -9` and SIGTERM during writes and an approval lost nothing acknowledged, `synchronous=FULL` was confirmed by tracing fsync, a 200 s soak at 85 writes and 102 reads a second failed nothing, and every answer of the quickstart matched its page.

### The seventh review's findings, re-checked

Fixed, measured by both: PUBLISH-SLOW (on six years 6.4 s of submit and 9.0 s of approval, with 3,600 actors 2.2 and 3.0 s, where it took 70 s); REFERRERS (a filter on `referrers` 0.36 s a page of employees and 1.3 to 3.1 s over a type, a guarded write 14 to 22 ms, `availability` 9 to 11 ms, eight at once 90 MB, where they took 2.9 s, 2.5 s and 7.4 GB); CUTSTALL (a person's worklist at most 1.1 to 1.8 s through erasures and agents' sweeps, where 25 s); and the minors but these: a stalled write answered after 40 s where the guide said 34.5; the restore beside a stale `-wal` refused, its files untouched, but in one path still with SQLite's words and no advice; a filter on a value derived from a set still reading its type on each page, as documented.

### What they found

| | Finding | Checked | Severity |
|---|---|---|---|
| ERASE-WAL-DRYRUN | An erased value stayed in the store's file and `-wal` for as long as a change's dry run ran, 38.7 s for a `check` of a large change, cut about 5 s after it ended, `objectflow_wal_cuts_missed_total` climbing to 7: the dry run's checks ran inside its read, which no cut could stop, and its simulation's world did not carry the stop an urgent cut sends | measured by the adversary; read | major |
| CASE-PAIR | A store carried from 0.2.0 holding two identities equal but for ASCII case, which 0.2.0 took: the start says nothing; `leave` and the erasure of either actor refused `identity_taken`; every publish refused `decision_unresolved`, its report's remedy, to admit the violation, refused by the checkers, and no transition writing an identity; the flow frozen and the two people not erasable | measured by the operator | major |

Minors: the evaluator's transport following a redirect to a host the operator never configured, its answer deciding a guard (the seventh review's adversary had reported redirects not followed); a version declaring an agent and no `publishing.taken_by` letting the agent approve its own change; an agent on the wire recording a label on any object, held by no `taken_by`, and `wire.md` naming the wrong request for a label; a read taking an `actor` of any JSON type; an `export` page past its limit by a few lines; cheap reads, an object's history among them, queued behind whole-type scans in the worklists' pool, 2.4 to 4.8 s, and raising `--max-reads` past the guide's `MemoryMax`, whose rule counted two pools of three; a stalled write saying so late, `/health` 200 for 40 s and `writer_held_seconds` reset at each retry; `/health` 200 while every write failed, after a backup was copied over the live file; the guides' `--create` step, unit, restore (ids and positions handed out after a backup are handed out again), `--version` of a source build, the `-wal`'s peak (306 MiB) and DESIGN.md §2's sizing sentence; `cargo-audit` installed unpinned in CI; integers written as `"+5"` or `"007"` taken.

### What the eighth review asks

The author's standing instruction decides what the findings ask (ADR-0195); the majors are fixed, then a ninth review, or the author's acceptance of what remains.

## 13. The ninth review, after the eighth's fixes

At commit `0ce1b3c`, 2026-10-13, not yet pushed, every gate of `scripts/check-all` passed on it: two new cold reviewers, an operator and an adversary, each in a checkout of its own with the release binary built from it, given the eighth review's findings to re-check, on copies of the same 37,154-object store and the six-year store, and the third review's go-live tools without their runs; the operator also followed the quickstart and the operate guide as a newcomer, and built release 0.2.0 to make a store as it took one. Their logs and reports stayed in the session's scratch directory, which is not kept; what they found is here.

**Verdicts: the operator GO, finding no major and no blocker; the adversary NO-GO, four majors and no blocker.** Neither met a panic, a crash, a hang or a corrupted store from a request: about 3,400 hostile values across every key of every operation, deep and huge JSON and YAML, forged cursors and limits answered without a `500`; `kill -9` and SIGTERM during writes and an approval lost nothing acknowledged; an erasure left nothing in files, answers, exports, attempts or the log once cut; with `taken_by` declared, the flow failed closed for agents.

### The eighth review's findings, re-checked

Fixed, measured by both: CASE-PAIR (a pair written by a 0.2.0 build named at the start, decided by one publish admitting `identity_taken`, both actors then leaving and erased, a third case acting as the first made); the redirect, the agent's own approval, a read's `actor` of any type, history and events behind the worklists (3.2 ms and 2.4 ms under 24 saturating scans), the guides' create step, unit, restore and `--version`, and `cargo-audit` pinned. ERASE-WAL-DRYRUN fixed for a change's dry run (the erased value gone 0.1 to 3.3 s after the erasure) and open through a `check` of a draft (N-10 below). Partly fixed: a stalled write said by `/health` from its first retry and `writer_held_seconds` counting through, but its log line only once the write gave up, the log's tick waiting on the writer; a backup copied over the live file in place still taking writes into the swapped file, lost at the next stop, `/health` answering `500` or `200`.

### What they found

| | Finding | Checked | Severity |
|---|---|---|---|
| N-4 | One draft of a flow change held every write for tens of seconds: the YAML parser compared each key of a mapping with every earlier one, and a draft parses its source holding the writer; a mapping of 100,000 keys held writes 37.8 s, a person's request waiting 35.8 s | measured by the adversary; read | major |
| N-10 | A `check` of such a draft held a read past every deadline, a parse touching no store and a read's deadline and stop being checked only where it does: an erased title in the store's files 38 s after its erasure, and the `-wal` grown to 1,895 MB in 100 s beside four writers | measured by the adversary; read | major |
| N-9 | A page of `query`, `pull` or `export` had no bound in bytes: 1,000 objects of three 64 KiB strings answered 197 MB, four pulls and four exports at once peaked at 4.78 GB, past the guide's `MemoryMax` | measured by the adversary | major |
| N-8 | A change's dry-run checks were not stoppable and grew past linear: a literal comparing `state` 128,000 times held the dry run's slot 229 s, past its deadline; measuring it found a change's checks quadratic in the types it declares, 8,000 running past eleven minutes | measured by the adversary and in the fix | minor, near major |

Minors: an agent on the wire rejecting, withdrawing, reopening, refreshing and revising a person's change, a version naming `agent` in `publishing` after none raising no notice, and an agent forgetting a person's label; a request of a change's `publish` answering `501`; a metric filter that is no boolean answering `501`; whole-type queries in the worklists' pool starving a plain worklist (4 of 6 tries timing out, `available` at 17.5 s); the guide's `MemoryMax=4G` short of a six-year store (3.97 GB peak); the operator named `OPERATOR` taken by requests and refused by `upgrade`; a version dropping `personal` unnoticed; refused attempts kept for ever without `--attempt-retention`; a restore beside its own later `-wal` silently undone; the dry-run slot's wait naming the wrong flags; `/health` answering `500` for an unreadable file; a token with a line break taken at start; an evaluator's timeout logged as "timeout: global"; a missing or unwritable store directory said in SQLite's words or with the wrong advice; 100,000 `order` keys and 10,000 `fields` taking seconds; the guides' install step, releases, unit hardening, the socket as the access control, the `-wal`'s peak (346 MiB), and `errors.md`'s `409` and `404`; pools chosen by the actor a read names; erasures every 3 s stopping dry runs.

### What the ninth review asks

The author's standing instruction decides what the findings ask (ADR-0196); the majors are fixed, then a tenth review, or the author's acceptance of what remains.

## 14. The tenth review, after the ninth's fixes

At commit `093d743`, 2026-10-13, not yet pushed, every gate of `scripts/check-all` passed on it: two new cold reviewers, an operator and an adversary, each in a checkout of its own with the release binary built from it, given the ninth review's findings to re-check, on copies of the same 37,154-object store and the six-year store, and the third review's go-live tools without their runs. Their logs and reports stayed in the session's scratch directory, which is not kept; what they found is here.

**Verdicts: both NO-GO; the adversary found one blocker and four majors, the operator one major.** Neither met a corrupted store or a lost acknowledged write: `kill -9` and SIGTERM during writes and a publish lost nothing, about 3,400 hostile values on every key answered without a `500`, an erasure left nothing in files, answers, exports, attempts or the log once cut, and with `taken_by` and `publishing` declared the flow failed closed for agents.

### The ninth review's findings, re-checked

Fixed, measured by both: N-4 (a draft of 100,000 keys answered in 0.14 to 0.18 s, writes beside it at most 0.16 s), N-10 (an erasure beside a long check gone from both files within 5.2 s, the `-wal` cut back), the stall's log line as the wait begins, `publish` as a request `not_requestable`, a metric filter that is no boolean `malformed_filter`, whole-type queries among the analytics, the operator as `OPERATOR` for `upgrade`, `personal_dropped`, the dry-run slot's message, a token's line break, the evaluator's timeout, the query's bounds, the start's notices and the guides. N-9 fixed for answers and open for memory (a page read its limit whole before cutting at 8 MiB); N-8 fixed for the shapes named and open for a type's transitions (8,000 checked in 70 s and their approval held writes 71 s). Still open as decided: a backup copied over a running service's file.

### What they found

| | Finding | Checked | Severity |
|---|---|---|---|
| D-RECURSE | A derived attribute reading itself through a reference, which both checkers accepted, overflowed the stack and aborted the whole service on a `get`: a chain of about 4,000 nodes, or two nodes in a cycle, which persists, so every restart died at the next read | measured by the adversary; read | blocker |
| MEM-EVENTS | A subscription's lag read 256 events whole on every `get` and `availability` (eight `get`s at once 3.3 GB on 2 MB events), and `pull` and `export` read their limit before cutting a page (four exports at once 4.4 GB) | measured by the adversary; read | major |
| PROPOSAL-RACE | A proposal executed on its object as approval found it: an agent proposed sending USD 5.00, grew the payment, and a person's approval sent USD 5,000,000; the `expected_version` sent with the proposal was not kept | measured by the adversary | major |
| PANIC-SLOT | A panic in a request kept its connection's place for good: an identifier's width of a million, which the checkers accepted, panicked each creation, and twelve panics under `--max-connections 8` answered every request `503 busy` until a restart | measured by the adversary; read | major |
| N-8 rest | A change's checks quadratic in a type's transitions, 8,000 checked in 70 s and their approval holding every write 71 s | measured by the adversary | major |
| LOG-FREEZE | A log reader that stopped reading froze every request, `/health` included, for as long as it stopped: two requests each holding standard error's lock stop both of the runtime's threads | measured by the operator; read | major |

Measuring the fixes found two more of N-8's kind: a chain of derived attributes typed in cubic time (1,000 in 171 s), and a dry run on the 37k-object store growing about 13 ms for each type a change adds (1,000 types in 14.5 s). Minors: an agent's `submit` of a person's draft and its `corrects` of a person's label; a failed `--create` in an unwritable directory said in the system's words; a small change waiting behind a large one's dry run told its store or change was large; an export page's memory 24 times its size; the approval holding writes 25 s on six years under load; a check answering `satisfied` beside an evaluator it did not ask; derived money past 18 digits; `history` with `follow` reading the group on each page; an idempotency key outliving an erasure; an imported identifier not matching its format; refused attempts and ids after a restore; `--create` before the socket's path is checked; a copied-in store's `-wal` and `-shm` modes; a failed checkpoint blamed on a read; logs without a request's id; `/metrics` and the log unpromised; the guides' call over the socket, backup schedule, `maintain` schedule and log volume.

### What the tenth review asks

The author's standing instruction decides what the findings ask (ADR-0197); the blocker and the majors are fixed, then an eleventh review, or the author's acceptance of what remains.

## 15. The eleventh review, after the tenth's fixes

At commit `3d0e07f`, 2026-10-13, not yet pushed, every gate of `scripts/check-all` passed on it. Two new cold reviewers, an operator and an adversary, each worked in a checkout of its own with the release binary built from it. They were given the tenth review's findings to re-check, on copies of the same 37,154-object store and the six-year store, and the third review's go-live tools without their runs. Their logs and reports stayed in the session's scratch directory, which is not kept; what they found is here.

**Verdicts: both NO-GO; no blocker, a major each.**

Neither met a corrupted store or a lost acknowledged write:
- `kill -9` during writes lost none of 595 acknowledged writes, and a publish killed or stopped rolled back or finished.
- A fuzz of every operation by every key with about 25 JSON shapes, 2,509 cases, answered no `500`.
- Online backup of 409 MB took 0.78 s, without stalling a write.
- A full disk, a clock ahead, evaluators down, hung or refusing, and the connection cap were each handled.
- An erasure's value left the store's files, answers and exports once cut, except under N1.

### The tenth review's findings, re-checked

- **Fixed, measured by both:**
  - D-RECURSE on every path that publishes: both checkers refuse a self-read through a reference, a cycle across types and a recursion through a set, and a 5,000-node chain and a two-node cycle answer `null`.
  - LOG-FREEZE: about 280 requests a second into an unread pipe, and `/health` on `--metrics-bind` answering `503 log_blocked`.
  - PROPOSAL-RACE, as the opt-in decided.
  - N-8: 8,000 transitions check in 0.55 s, and a chain of 1,000 derived attributes in 0.39 s.
  - The minors decided, and the guides.
- **Fixed by reading:** PANIC-SLOT; no panic was found to trigger.
- **Partly fixed:**
  - MEM-EVENTS: a page of the log's export still held each event's object (D636).
  - D-RECURSE where a store was installed before the fix: a recursion through a set still aborted the process on a leaf's read (D633).
  - The approval's hold on writes: 16.2 s under load at six years, 8.9 s idle.
  - The backup and `maintain` schedules: advice, with no unit given.

### What they found

| | Finding | Checked | Severity |
|---|---|---|---|
| F1 | A chain of derived attributes each reading the one before twice, `d1: d0 + d0` on to `d40`, held the store's one dry-run slot past six minutes on a core: typed 2^n times, past every bound on depth, the reference checker hanging alike, and a later change's submit refused after `--dry-run-timeout` | measured by the adversary; read | major |
| N1 | A page of a type's intervals read every event into memory and walked them once for each of the type's objects: 15 to 19 s and a gigabyte a page at six years, past `--read-timeout`, and an erased name left in the `-wal` 43 s, past the urgent cut | measured by the operator; read | major |
| F2 | A published chain of 20 cost 1.11 s of a core on each `get` | measured by the adversary | minor |
| F3 | A decimal written past its 18 digits answered `501 not_run`, where an integer and money answer `value_in_range` | measured by the adversary | minor |

Measuring F1's fix found the same unbounded fan-out in four more walks over derived attributes, and an invariant reading one derived attribute many times inlined as often (D631).

The minors:
- **Signals and the log:** SIGHUP ended the service at once, an end systemd's `Restart=on-failure` counts clean. The log never said it had dropped lines.
- **Load:** a refused request is a durable write, so one actor's refusals in a loop slowed a person's writes. At six years, half of 24 concurrent per-person metrics timed out. Erasures at one a second stopped long reads.
- **`/health`:** it answered `200` while the clock led by three days, and while an evaluator refused the token. Its `disk_full` was in neither guide.
- **The store at start:** a second service on the store was said only at its own start, and a restore's reissued ids not at all. A directory given as `--store` was told to get write permission. The `.attempts` export read every attempt on each page.
- **The guides:** the backup check needed the `sqlite3` program and left `-wal` and `-shm` beside the copy; the unit's `StateDirectory` reset its directory's mode; there was no shell recipe for a deployment's first publish.
- **Outside the engine:** an agent's `forget` and `corrects` of a person's label, left to the gateway.

### What the eleventh review asks

The author's standing instruction decides what the findings ask (ADR-0198); the majors and the minors that cost little are fixed, then a twelfth review, or the author's acceptance of what remains.


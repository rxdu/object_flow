# ADR-0204: What production readiness means, and when the reviews stop

- **Status:** **Decided** 2026-10-14 by the author. Proposed the same day, it was critiqued by an independent reviewer, and the author accepted the six recommendations made from that critique ("go with your recommendations on all six"). It is written here as revised by the critique.
- **Date:** 2026-10-14
- **Refined by:** ADR-0205 — decisions 9.3 and 9.4 widened by the audit. ADR-0206 — decision 9.3's design. ADR-0207 — decision 9.4's design. ADR-0210 — §1's report readers read at a pace, back to back a stress run beside the gate.
- **Supersedes:** ADR-0183 decision 1, the bar of a production review that finds no blocker and no major
- **Relates to:**
  - ADR-0180 (what a release promises); ADR-0183 (the production review, the release workflow, `/metrics`' names); ADR-0174, ADR-0178, ADR-0197 and ADR-0199 (the limits of section 10); ADR-0201 decision 4 and ADR-0202 decision 3 (the read guards this ADR replaces);
  - `PRD.md` N1, N3, N4, N7, C5 and T5;
  - `design/adversarial-harness.md`; `design/production-review.md`;
  - `guides/operate.md`; `guides/agents.md`; `SECURITY.md`; `README.md`

## Context

Sixteen production reviews have run, each a cold operator and a cold adversary on the release binary. ADR-0183 decision 1 held 1.0.0 until one found no blocker and no major. The record in `production-review.md`, which states counts from the fifth review:

| Review | Operator | Adversary |
|---|---|---|
| 5th | 1 major | 1 blocker, 6 majors |
| 6th | 1 blocker, 2 majors | 3 majors |
| 7th | 1 major | 2 majors |
| 8th | 1 major | 1 major |
| 9th | GO | 4 majors |
| 10th | 1 major | 1 blocker, 4 majors |
| 11th | 1 major | 1 major |
| 12th | GO | 1 major |
| 13th | 1 major | 5 majors |
| 14th | 2 majors | 2 majors |
| 15th | GO | 1 major |
| 16th | GO | 2 majors |

No blocker has been found since the tenth review, and no review has lost an acknowledged write. The operator gave GO in two of the last four reviews and four of the last eight. The adversary found majors in every round, and the series did not converge.

The record shows three causes:

1. **Severity was decided afresh by each reviewer**, against no written criteria.
2. **An adversary's search has no end**, and the bar asked for a round with none.
3. **Fixes were made by instance, not by class.** Several majors were the next instance of a class an earlier fix had closed only at the reported point:
   - an identity written from `actor.id` (fifteenth), then its copies (sixteenth);
   - the personal guard on queries (fourteenth), then on metrics (sixteenth);
   - object reads pooled by type (thirteenth), then per operation (fourteenth).

The first draft of this ADR was critiqued by an independent reviewer, who found:
- three conflicts with the PRD: N1 (PostgreSQL in production), N4 (the adversarial harness is the acceptance test, and none was built) and T5 (the engine filters no read by who is asking);
- promises missing that the project's own harness design lists;
- a rubric giving two or more defensible grades to about 17 of 42 recent findings;
- a performance promise met by a `503`;
- a stopping rule that repeated the per-instance pattern.

The draft also said the engine reads an actor's kind but never its identity. That was wrong: `upgrade` checks the operator's identity. This revision corrects all of these.

## Decision

### 1. The production reviewed, and its load profile

One company's live system of record, run by a small team on one Linux machine, with hundreds of users behind the team's own application and AI agents acting through the generated tools. It holds tens of thousands of objects a year, kept for years (`production-review.md` §1, PRD N3). Its store is one SQLite file on a local filesystem (decision 9.1).

The profile a review tests against:

- **Stores:** the two-year store, 36.7k objects, and the six-year store, 110k.
- **Hardware:** the reference machine is the one the reviews have used, a six-core, twelve-thread i7-8700 with NVMe, shared.
- **Steady load:**
  - sixteen people's sessions at once, each a mix of one-object reads, worklists and writes;
  - four of them reading reports, such as per-person metrics;
  - two agents sweeping through their tools;
  - writes at up to 50 a second.
- **Peaks and bursts:** a month-end hour at twice the steady load; clients retrying after their timeouts; a restart of the service.
- **Operations:** a publish a day, erasures in batches, a backup every 15 minutes and a daily prune.
- **Duration:** a soak of at least 24 hours of the above.

### 2. The trust model

- **The gateway is trusted to authenticate and to name the actor, and to nothing else.** The other promises hold for every request on the socket, including any tool call a manipulated agent makes. The gateway also sets a request's principal, idempotency key and backdated time.
- **Outside the model is only:**
  - harm that consists of an actor doing what the flow allows and the application should have refused (PRD N7);
  - the operator's own acts on the store's files and the service's flags, unless the service detected them;
  - an attacker who reaches the socket, who is the operator (ADR-0183).
- **Evaluators** are trusted with the arguments they are sent and untrusted in their answers.
- **Subscribers** are trusted with what they pull.

### 3. What the engine promises, which a review grades

- **D, durability.**
  - An acknowledged write survives any crash, kill or stop, and power loss on a disk that honours its flushes.
  - On loss of the volume, recovery is the last backup, so the recovery point is the backup interval.
  - No request, crash or stop corrupts the store, and a refused start leaves every existing file as it was.
  - A keyed retry applies a write at most once.
  - A restore and an upgrade done as the guides say work, and a restored store never hands out an id or a position it handed out before (decision 9.4).
- **R, the rules.** A flow's rules hold for every request on the wire, through every route a request reaches an object by: guards, invariants, `taken_by`, uniqueness, bounds and `only_via`. The routes are its id and type, cascades, loops, proposals, imports, migrations, idempotency and races.
- **A, the record.**
  - Every change and every refusal is recorded with the actor the request named and the kind its type declares.
  - A `satisfied` verdict names an event of that request.
  - An identity names one actor.
  - A key replays only the request it was first sent with.
- **P, personal data.**
  - After its erasure, what a flow marks personal, and a personal identity wherever a value of type `identity` holds it, is gone from every read, at once, and from the store's file and `-wal` within the documented cut.
  - It is gone from an index page's free space within the deployment's maintenance interval (decision 9.5).
  - Backups are governed by their expiry, and erasures are asked again after a restore.
  - No metric reads a personal value, and a consulted reference masks one.
  - A read whose filter or order reads a personal member is refused unless its request opts in (decision 9.3).
- **V, values.** What the engine stores, derives and answers, a filter's matches included, is what `flow-format.md` and `declaration-syntax.md` define, and the same on the core and the reference runner. A value it cannot compute is refused, never answered wrong or as null.
- **E, events.** Every committed change reaches `pull` and `export` at least once, in each object's order. A cursor the store cannot honour is refused.
- **C, cost.** At the profile, on the reference machine:
  - every request is answered, not refused `503`;
  - a request and a read of one object take at most 100 ms, and a metric read at most 1 s, at the 95th percentile (PRD C5, decision 9.6);
  - every other read is within the figure `operate.md` gives it.

  Past the profile, the service refuses quickly, never crashes or exhausts memory, and recovers when the load falls. A figure the guide gives does not excuse a miss of C5: a read that cannot meet C5 is accepted only by name, by the author, in `operate.md`'s limits.
- **K, the checkers and the engine agree.** A flow both checkers accept is one the engine runs.
- **G, the guides.** The procedures in `quickstart.md`, `operate.md`, `integrate.md` and `agents.md` work as written, and every instruction the service prints counts as one. The guides describe what the engine does.

### 4. Severity

- **Blocker:**
  - a broken D, R, P, A, V or E in normal operation;
  - a request that crashes or hangs the service;
  - an exhaustion of memory or a crash at any load within the profile.
- **Major:**
  - a broken D, R, P, A, V or E that needs an unusual but legitimate flow, load or sequence;
  - a broken C at the profile;
  - a crash, a hang or an exhaustion of memory past the profile but within the connection cap;
  - a procedure in the guides that, followed as written, loses data or leaves personal data an erasure should reach.
- **Minor:**
  - a broken K that refuses at run time without losing data or a rule;
  - a guide that is incomplete but safe to follow, or a message that misleads without risk;
  - a cost past a guide's figure but within C5 and its deadline;
  - behaviour of the sandbox alone.
- **Not a finding:**
  - what section 2 puts outside the model;
  - loads more than twice the profile's concurrency, or inputs larger than any flow of section 1 holds;
  - the limits listed in section 10.

**Definitions:**
- **Normal operation** is any pattern a README, a guide or a shipped sample shows, or one a flow of section 1 would plausibly use: an email as an identity, an evaluator called with an attribute, a metric grouped by a reference's attribute, a requester copied to a child.
- **Where two bullets apply, the higher grade wins.**
- Each grade names the promise and the bullet it applies.
- A guide's edit does not change a grade it was graded against, except where section 3 names the guide as the measure.

**Calibration.** Recent findings graded by this rubric:

| Finding | Grade | By |
|---|---|---|
| D643 an evaluator's argument kept after an erasure | blocker | P, normal operation |
| D663 an identity written from `actor.id` kept after an erasure | blocker | P, normal operation (the README's pattern) |
| D677 a copied identity kept after an erasure | blocker | P, normal operation (a requester copied) |
| D678 a metric reading a person's salary through a reference | blocker | P, normal operation |
| D654 a reused key replaying a request that never ran | major | A, an unusual sequence |
| D679 `pull` after a restore answering nothing, for good | major | E, an unusual sequence |
| D672 a printed instruction to delete the `-wal` after a crash | major | G, a procedure that loses data |
| D688 writes answered after the service found the store unreadable | major | D, an unusual sequence the service detected |
| 16th review: 12 of 16 per-person metric reads refused `503` | major | C, at the profile |
| D642 a worklist page 0.8 s on six years | major | C, at the profile |
| D660 `--create` made a store before a long socket path refused the start | minor | G, a message; no existing file changed |
| D687 the checkers accepting a construct the engine refuses `501` | minor | K, refused at run time |
| D658 a 201-line claim unerasable under a bound the checker noticed | minor | K and P, the flow's own bound, noticed at publish |

### 5. Evidence

- A finding is measured on the release binary, or reproduced.
- A finding read from the code alone is graded by its consequence and marked unconfirmed. It is closed only by a reproduction, a fix, or a written argument that it cannot occur.

### 6. The harness, and the audit through it

**The harness.** The adversarial harness of `adversarial-harness.md` is built (decision 9.2):
- Rust, against the core in process, on SQLite;
- its five behaviours and its failure conditions;
- the failure conditions extended by A, V and E as section 3 states them;
- run deterministically from a seed, a failing run reduced to its shortest failing prefix.

It runs in `scripts/check-all` with a fixed budget of seeds, and is the acceptance test PRD N4 names.

**The audit.** Six reviewers work at once, each owning one area:
1. **Personal data end to end:** storage, copies, indexes, exports, consulted keys, logs, backups and restore, and every read that reveals a value or allows one to be inferred (filters, order, `check`, metrics, failures naming holders, `actor_id`).
2. **Rules and the record on the write path:** every PRD F2 route against every kind of rule and every way a request reaches an object, actor and identity resolution included.
3. **Values and agreement:** every construct on both checkers and both runtimes, run against the reference runner.
4. **Cost, pools and robustness:** every read path, input bound, overload, recovery and memory, against C.
5. **Durability, recovery, upgrade, events and release.**
6. **The guides and the gateway contract:** every example in a README, a guide or a sample, run as production.

Each reviewer states how it enumerated its area, from the schema's tables, `wire-v1.json` or the expression's variants, and why that enumeration is complete. It returns the whole inventory, each item graded by section 4. **Each item is delivered as a check of the harness, or of the suites, kept in the repository**, so a finding outlives its review.

The fixes land area by area, each class closed whole, mutation-tested, with the gates run after each area.

### 7. The release gate

The confirming review runs on the 1.0.0 candidate:
- built by `release.yml` from a pushed tag whose `check.yml` run passed;
- with its release entry and a 0.2.0-to-1.0.0 upgrade fixture;
- after the soak of section 1.

Pushing and tagging are the author's acts.

### 8. The stopping rule

After the confirming review, one cold operator and one cold adversary graded by sections 3 to 5:
- **A blocker, or a major in an audited class,** means that class's inventory was incomplete. The class is audited again, its fixes land, and a cold reviewer re-checks that class alone.
- **A major in a class no audit covered** is audited as in section 6, and fixed.
- **A minor** goes to `TODO.md`.
- **Every fix** lands on a pushed commit with CI green.
- **At most one further confirming review** follows.
- **The reviewers' grades stand** unless the author changes one in writing.

**The author then decides 1.0.0, with a floor:**
- no open blocker;
- no open major in D, R, P, A, V or E;
- an open C or G major only if the author accepts it by name in `operate.md`'s limits.

### 9. The author's six decisions

1. **N1, storage.** PRD revision 25 makes N1 read: SQLite for a single-machine deployment of N3's size; PostgreSQL when a deployment outgrows one file. 1.0.0 ships on SQLite.
2. **N4, the harness.** It is built first, and the audit is delivered through it (section 6).
3. **T5, the read guards.** ADR-0201 decision 4 and ADR-0202 decision 3 read an actor's kind to refuse a read, which T5 forbids. They are replaced by a guard that reads no actor and fails closed:
   - a `query`, a `metric` read or a `check` whose filter, order or answer would read a personal member is refused `personal`, unless its request carries an opt-in the gateway sets, as a request input;
   - rules may read request inputs (N7), so this is within T5;
   - it closes the same leak for an application that forwards a person's filter;
   - it changes a default, so it lands before 1.0.0 (ADR-0180).

   Its design is an ADR of its own.
4. **Restore and ids.** A restore advances every counter the store hands out from, object numbers, event positions and transactions, past a margin. An id an integration kept from before the backup then never names another object. Its design is an ADR of its own.
5. **Erased values in an index's free space.** A `maintain` task rebuilds the indexes an erasure touched, measured first for what it holds the writer. P holds within the deployment's maintenance interval, its timer in `operate.md`.
6. **C5.** The engine is made to meet C5 on the six-year store at the profile. A read that cannot is accepted only by name, by the author.

### 10. The limits accepted

Accepting this ADR accepts these as limits of a 1.0.0 production. `README.md` and `SECURITY.md` state them:
- one machine, one writer and no replica (ADR-0178);
- recovery is the last backup, its recovery point the backup interval (ADR-0197 decision 8), ids never handed out again (decision 9.4);
- an approval holds writes for its run (ADR-0199 decision 7);
- the engine authenticates nobody (ADR-0183, PRD N7);
- a store's clock that ran ahead is not brought back (ADR-0174 decision 4, ADR-0183);
- performance figures beyond C5, and log text, are not promised (ADR-0180). `/metrics`' names are, as `/health`'s fields are (ADR-0183).

D656, a read running on after its client has gone, is not a limit: it is an open minor with a planned fix (ADR-0201).

### 11. Changes that refuse what 0.2.0 accepted

The audit lists every fix that refuses what 0.2.0 accepted, and each lands before 1.0.0. After 1.0.0, ADR-0180 lets a new refusal arrive only as a notice first. A refusal that restores D, R, P, A, V or E is to be proposed to land in a minor release, as an amendment of ADR-0180.

## Alternatives rejected

- **Another review with the bar unchanged.** The record shows no convergence, and rounds re-graded what the guides assign to the gateway.
- **A wider review whose findings are fixed one by one.** It repeats the per-instance fixes that made several majors.
- **Releasing on the operator's GO alone.** R, P and A are where the adversary's majors have been, and they are promises the engine makes.
- **No written severity.** Without it, "major" is each reviewer's word, and no round's verdict can be compared with the last.
- **Reviewers' probes thrown away, as before.** A finding that leaves no check behind can return. The audit's items, kept as checks, cannot.
- **Building PostgreSQL before 1.0.0** (decision 9.1). It is a second store under every promise above, for deployments larger than N3's first consumer needs.
- **Read guards by actor kind** (decision 9.3). They contradict T5, and leave an application forwarding a person's filter unguarded.

## Consequences

- ADR-0183 decision 1 is superseded; its other decisions stand.
- PRD revision 25 revises N1.
- ADR-0201 decision 4 and ADR-0202 decision 3 are to be superseded by decision 9.3's design.
- `TODO.md` holds the plan: the harness, the audit, the six decisions' changes, the release gate.
- The review briefs carry sections 1 to 5 as their rubric.
- `README.md` and `SECURITY.md` state section 10's limits once the release is cut.

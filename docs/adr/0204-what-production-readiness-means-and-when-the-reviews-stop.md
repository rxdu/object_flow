# ADR-0204: What production readiness means, and when the reviews stop

- **Status:** **Proposed** 2026-10-14, for the author's review ("yes to all three, write the criteria ADR first"). On acceptance it supersedes ADR-0183 decision 1, the bar of a review that finds no blocker and no major.
- **Date:** 2026-10-14
- **Relates to:** ADR-0183 (the production review and its bar); ADR-0180 (what a release promises); `PRD.md` §6.8 (N3, N4, N7); `design/production-review.md`; `guides/operate.md`; `guides/agents.md`

## Context

Sixteen production reviews have run, each a cold operator and a cold adversary on the release binary. ADR-0183 decision 1 holds 1.0.0 until one finds no blocker and no major. The record in `production-review.md`:

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

The first four reviews are recorded in other terms. No blocker has been found since the tenth review, and no review has lost an acknowledged write. The operator has given GO in two of the last four reviews and four of the last eight. The adversary has found majors in every round, and the series has not converged.

Three causes are visible in the record:

1. **Severity is decided afresh by each reviewer**, against no written criteria. Items the guides assign to the gateway, or that earlier ADRs decided, have been ranked again each round.
2. **The adversary's search has no end.** An hour on a system of this size will find the next edge, and the bar asked for a round with none.
3. **Fixes were made by instance, not by class.** Several majors were the next instance of a class an earlier fix had closed only at the reported point:
   - an identity written from `actor.id` (fifteenth), then its copies (sixteenth);
   - the personal guard on queries (fourteenth), then on metrics (sixteenth);
   - object reads pooled by type (thirteenth), then per operation (fourteenth).

The PRD frames readiness differently. Under N7 the upper layers own authentication, "what an actor may do and what it may see", and the engine "evaluates no actor". The engine reads an actor's kind, never its identity or role: for `taken_by`, for the agents' pool, and for the personal guard on reads. N4 makes "the adversarial harness" the acceptance test, a test that runs, not a reviewer's verdict. N3 sizes the engine for operations rate.

## Decision

### 1. The production reviewed

As `production-review.md` §1 defines it: one company's live system of record, run by a small team on one Linux machine, with hundreds of users behind the team's own application and AI agents acting through the generated tools. It holds tens of thousands of objects a year, kept for years.

The load profile is what the reviews have measured against:
- the two-year store, 36.7k objects, and the six-year store, 110k;
- eight to sixteen writers and sixteen readers at once;
- a few agents.

### 2. The trust model

**What reaches the socket is the operator** (ADR-0183). The application in front of it, the gateway:
- authenticates people and agents;
- sets each request's actor, principal, idempotency key and backdated time;
- decides what each actor may do beyond the flow's rules, and what it may see (PRD N7);
- closes to agents the operations `agents.md` lists, `check` among them;
- keeps `maintain`, `upgrade` and `import_batch` to the operators' tools.

A finding that needs an attacker past the gateway, or an operator acting against their own store, is outside the model.

### 3. What the engine promises, which the review grades

- **D, durability.** An acknowledged write is never lost. No request, crash, stop or refused start leaves the store corrupted or its files changed. A restore and an upgrade done as the guides say work.
- **R, rules.** A flow's rules hold for every request on the wire: guards, invariants, `taken_by`, uniqueness, bounds. No sequence of requests bypasses them.
- **P, personal data.** After its erasure, what a flow marks personal, and a personal identity wherever a value of type `identity` holds it, is gone:
  - from every read;
  - from the store's file and `-wal` within the documented cut;
  - from an index page's free space after the documented `VACUUM`.

  Beyond erasure, no metric reads a personal value, a consulted reference masks one, and the engine's guards on reads by an agent or an unknown actor hold as `agents.md` states them.
- **C, bounded cost.** Every request ends by its deadline, waits in its pool, and makes no other kind of request wait past the guide's figures at the load profile. Memory stays within the guide's sizing.
- **K, the checkers and the engine agree.** A flow both checkers accept is run by the engine.
- **G, the guides.** The procedures in `quickstart.md` and `operate.md` work as written, and the guides describe what the engine does.

### 4. Severity

- **Blocker:**
  - a broken D, R or P reached in normal operation, or by any actor inside the trust model;
  - a request that crashes or hangs the service.
- **Major:**
  - a broken D, R or P that needs an unusual but legitimate flow, load or sequence;
  - a broken C at the load profile: a common operation an order of magnitude past the guide's figure, or past its deadline;
  - a procedure in the guides that, followed as written, loses data or leaves personal data an erasure should have reached.
- **Minor:**
  - every other defect: a broken K that refuses at run time without losing data or a rule;
  - a guide that is incomplete but safe to follow, or a message that misleads;
  - a cost past a figure but within its deadline;
  - behaviour of the sandbox alone.
- **Not a finding:**
  - what section 2 gives the gateway or the operator;
  - loads well past the profile;
  - behaviour reached only by editing the store's files;
  - the limits already decided, each in its ADR:
    - one machine, one writer and no replica;
    - recovery is the last backup, which hands ids out again;
    - an approval holds writes for its run;
    - the engine authenticates nobody;
    - a store's clock that ran ahead is not brought back;
    - a read runs to its end or its deadline after its client has gone (D656);
    - performance figures, metric names and log text are not promised (ADR-0180).
- **Evidence.** A finding is measured on the release binary, or reproduced. One read from the code alone is graded at most minor, unless its reproduction is given.

### 5. The audit

One pass audits the engine by property, not by instance. Six reviewers work at once, each owning one area:

1. Personal data and erasure: every place a value is stored, copied, derived, indexed, exported, consulted or logged.
2. The read surface for agents and unknown actors: every operation and tool.
3. The checkers against the engine: every construct the engine does not run.
4. Cost and the pools: every read path and every loop.
5. Durability, recovery and upgrade.
6. The guides against the engine.

Each reviewer returns the whole inventory of its area, every instance graded by section 4, not the first instance found. The fixes are then made as one batch, each class closed whole and mutation-tested as before.

### 6. The confirming review, and the stopping rule

**The review.** One cold operator and one cold adversary review the result, given sections 1 to 4 as their rubric and the audit's inventory to re-check.

**After it:**
- A blocker is fixed and verified by a targeted test and measurement. It needs no new full review unless the fix changes a class's design.
- A major in an audited class is fixed and verified the same way.
- A major in a class no audit covered is audited as in section 5, and fixed.
- A minor goes to `TODO.md`.

**Then the author decides 1.0.0** on the list that remains, in place of ADR-0183 decision 1's bar.

### 7. Questions for the author

1. Is section 1's load profile the production you mean, or should it be larger or smaller?
2. Under PRD N7, the engine's guards on reads by an agent or an unknown actor (ADR-0201 decision 4, ADR-0202 decision 3) read an actor's kind. That is the reading `taken_by` and the agents' pool already use, and section 3 treats the guards as a promise. Should they stay, or should what an agent may see be the gateway's alone?
3. Section 3 counts an erased value in an index page's free space as gone once the guide's `VACUUM` has run. Is that enough, or should the engine rebuild an index an erasure touched, at its cost under the writer?

## Alternatives rejected

- **Another review with the bar unchanged.** The record shows no convergence, and it has spent rounds re-grading what the guides assign to the gateway.
- **A wider review, its findings fixed one by one.** It repeats the per-instance fixes that made several majors.
- **Releasing on the operator's GO alone.** P and R are where the adversary's majors have been, and they are promises the engine makes.
- **No written severity.** Without it, "major" is each reviewer's word, and a round's verdict cannot be compared with the last.

## Consequences

- ADR-0183 decision 1 is superseded on acceptance. Its other decisions stand.
- The review briefs carry sections 1 to 4 as their rubric.
- `production-review.md` records the audit and the confirming review as the series' last two sections.

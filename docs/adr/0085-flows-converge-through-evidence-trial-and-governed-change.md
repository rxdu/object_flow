# ADR-0085: Flows converge through evidence, trial and governed change, and the publish checks stay

- **Status:** Proposed — evaluated 2026-09-23 at the author's direction, against `docs/PRD.md`; pending author review
- **Date:** 2026-09-23
- **Refines:** ADR-0027

## Context

The PRD requires that a flow can start minimal (V1), and that the engine show where a flow and reality disagree (V2). A rule must be triallable before it is enforced (V3). Changes must be proposable with evidence and applied through a governed path (V4), and agents may author them (F6). The engine must never change a flow by itself.

Publishing today is an operation any caller of `publish` performs, with a report (ADR-0027, `design/publish-and-import.md`). The only record of a publish is the `ok_declaration` row, and the "publish event" migrations cite has no object to belong to (D212). `design/data-driven-engine.md` §3.6 evaluates the options.

## Decision

### 1. `diagnostics(actor, type)` reports where a flow and reality disagree

It is built on the standard metrics (ADR-0084) and reports a fixed, deterministic list:

- transitions and states unused in a window;
- the clauses that refuse most, by remedy and actor kind;
- states entered mostly by override;
- labels by frequency and by the state they were applied in;
- the longest waits;
- rework loops, and reassignment back to an earlier assignee;
- work done by someone other than the assignee;
- backdating rates;
- each observing guard's would-be refusals.

It infers nothing about intent. Agents build proposals on it.

### 2. A guard clause may be marked `observe`

The clause is evaluated. A false or unknown result records a would-be refusal as an attempt (ADR-0083), marked not enforced and linked to the event that applied, and the request proceeds. The printed rule set shows the clause as not enforced, the harness treats it as absent, and the publish report lists every observing clause, so no guarantee is ever claimed for one. An observing capability guard is allowed and reported prominently. Enforcing a clause is a publish that removes the marking.

### 3. A flow changes through a built-in `DeclarationChange` object

Any actor holding the drafting capability creates one, carrying:

- the source;
- the dry-run publish report;
- links to its evidence.

A **different** actor holding the publish capability approves it, and approval publishes it. It is superseded when the installed version moves under it. **By default, a change an agent drafted is approved by a human.** A deployment may require that of every change. Being a built-in object like `Proposal` and `Subscription`, every step has an actor and an event, the loop from evidence to change is measurable, and the publish event has its object, which resolves D212.

### 4. The engine never drafts a change

Drafting is an actor's job, usually an agent reading diagnostics (ADR-0012).

### 5. The publish checks stay

The smallest flow the checker accepts is a six-line type body plus two module lines; `scripts/check-syntax-doc.py` reports it clean against the 26 checks it implements. What a new flow lacks is knowledge of its own rules, and the observations, labels, observing guards and diagnostics of ADR-0082 to ADR-0085 are what supply it.

## Alternatives rejected

- **Leave the evidence to agents entirely.** Every agent would compute its own version of "unused" and "refuses most". That is the drift ADR-0081 exists to end, applied to the evidence a flow change is argued from.
- **Evaluate a whole candidate declaration in the shadow of live traffic.** It doubles the evaluation of every request, and a candidate's outcomes cannot be applied, so only guard verdicts could be compared. That is what `observe` gives, clause by clause, at no cost to the rest.
- **Replay recorded requests against a candidate at publish.** Attempts do not record inputs (ADR-0083), and recording them would put personal data in a table built to have none.
- **Any holder of the publish capability publishes directly.** No evidence is attached, and no second actor sees the change before it applies. For a change an agent drafted, that means rules nobody has read.
- **Relax the publish checks for early versions.** They catch real defects and protect T1, and the smallest valid flow is already small.
- **Let the engine suggest changes by inference.** Suggestion is judgement, and judgement belongs to actors above the store, as selection does (ADR-0007, ADR-0081).

## Consequences

- On acceptance, `DESIGN.md` §5.9 and §9 describe governed publishing and `DeclarationChange`.
- `design/publish-and-import.md` §1 makes a publish the approval of a `DeclarationChange`.
- `design/declaration-syntax.md` gains the `observe` marking.
- `design/library-api.md` gains `diagnostics`, and `publish` takes an approved change.
- `design/adversarial-harness.md` excludes observing clauses from the guarantee it tests.
- D212 is resolved on acceptance.
- ADR-0027 carries the back-link.

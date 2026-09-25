# The engine against the first consumer's own requirements

Status: **check, awaiting the author**, 2026-09-25. At the author's request, the operations platform's PRD was read in full and set against the engine's PRD, its recent decisions and the flow review's open decisions (`flow-review.md` §7). Nothing here changes a requirement: each finding is a question for the author, and the PRD changes only once the author answers.

**The source.** The operations platform's PRD is a Claude document, "Weston Robot Operations Platform — PRD", revision 17. It says it is drawn from its own repository's `DESIGN.md`, ADRs 0001–0037 and conversations up to 23 September 2026. That repository is not on this machine, so its design documents were not read. Cited below as `ops PRD`, by section and requirement number.

**Why it matters.** Until now, the engine's evidence about its first consumer came from two places. One is the production inventory system (`wr_inventory_management`), read at `4109939`. The other is `wr:docs/proposals/operations-system-design.md`, a draft dated 2026-07-19 that says "Nothing here is committed to implementation yet"; the engine PRD cites it four times (§2; D11; M7; UC-12). The ops PRD is two months newer and is the author's reviewed statement of what the first consumer is. Where the two disagree, the ops PRD should win.

## 1. What the first consumer is, in the engine's terms

The ops PRD describes "an operations platform that tells Weston Robot whether it can take work on, at the moment of quoting — and that learns from its own history".
- **Work is one model.** A request is made of lines, and a match turns lines into reservations of people and hardware. Every kind of work, twelve of them, runs on that one model.
- **The inventory system becomes one module**, "the facts about physical items", and its switch-over is the last delivery stage (stage 9).
- **The first workflow built is reimbursement** (stage 1): "nothing to migrate, no time slots to fight over; proves requests, lines, lifecycles and a verified Xero write".
- **Its lifecycle requirements are what ObjectFlow provides.** R13 says "track every managed object through a declared lifecycle", R14 "let each workflow declare which transitions it performs, as configuration", and R15 "keep lifecycles configurable, reviewable and checked automatically". Its recording requirements (R17–R19) and its audiences (R9, R10) are what the engine's data capture and metrics serve.
- **Two of its non-goals bear on the engine.** "A general workflow engine" means the platform does not build one: it uses one. "Rules written in advance to detect problems already seen" is discussed at §3.2.

## 2. Where the ops PRD supports the engine's requirements

| Ops PRD | Engine requirement or decision it supports |
|---|---|
| R13–R15, lifecycles declared, configurable and checked | F1, F5, F7 and the checked declaration language |
| Principle "Record what happened"; R18, "record structure — states, who each is waiting on, handoffs, links, decisions"; journey 3, "time adds up by waiting party" | D1, and **D11**: who each status waits on is a tracked value whose time adds up. The ops PRD gives D11 a use case it lacked |
| "A status someone clicks later carries the time of the click … evidence and clicks are compared, so recording lag is itself measured" | **D5**, occurred time kept apart from recorded time, and recording lag as a measure |
| R8, agents have "the same API and permissions as people, except that they can never commit a financial write" | **T6**: a rule that depends on the kind of actor, which the declared kind makes trustworthy |
| "Agents … the same API and MCP interface as people"; one developer plus agents | N7, one interface for every caller; F3 |
| R5, "preserve every row of the inventory system"; "row counts reconcile per table in every migration rehearsal" | N5, and it answers N5's open question: what the engine holds no object for must still survive, in an archive the platform keeps, so the counts reconcile |
| "Purchases link back to the work that caused them" (journey 5) | `procured_for`, the link from a unit to the order it was bought for (flow review §2.2) |
| "Goods only move one way … internal moves are faked with a customer record named after ourselves" | the flow review's transfer to the pool, in place of an internal delivery |
| R2, "refuse double-booking in the database itself, not in application code" | invariants, enforced at serialisable isolation and compiled to an exclusion constraint on PostgreSQL (ADR-0041); with N7 no application code writes around them |
| Management needs "where time goes"; the team sees "where time goes, by who it waits on" | C3 over periods: time apportioned to the months it spans, which ADR-0103 §5 records as missing |

## 3. Where the engine reads the first consumer differently

Each finding names what the ops PRD says, what the engine does, and the question for the author.

### 3.1 Flag rather than block

- **What the ops PRD says:** "Permissive by default. Anything not forbidden is allowed. The system flags rather than blocks, and blocks only what is irreversible and lands outside the team — an external financial write, or a double-booking." R6: "communicate and enforce every restriction; default to flagging".
- **What the engine does:** a guard refuses. A clause marked `observe` records a would-be refusal without refusing, but the PRD frames it as a trial before enforcement (V3). T1 says it "guarantees nothing", the rule set prints it as a trial, and the caller is not told: `Satisfied` carries no flags (`library-api.md` §4).
- **The question:** should the engine support a **flag** as a permanent kind of rule? It would be evaluated on every request, recorded, returned to the caller in the verdict, and printed as a flag rather than a trial. For the first consumer, most rules would be flags, and only external financial writes and double-booking would block.

### 3.2 Problems found in the data, not declared in advance

- **What the ops PRD says:** R18, "derive problems from it, never hard-code them"; a non-goal, "rules written in advance to detect problems already seen"; a principle, "Observed, never declared: expectations … come from history, not from someone's statement".
- **What the engine does:** M7 declares business exceptions as conditions with thresholds (`flag slow when over 10 days`). UC-12 takes its six conditions from the July draft's alert catalogue.
- **The question:** should M7 be re-read? On this reading:
  - "Overdue against a date a person committed to" stays, since it is a fact about a commitment, not a hard-coded problem.
  - "Ageing past a threshold" is measured against the observed norm, for example beyond the usual range of time in that state, rather than a declared number.
  - Diagnostics (V2) are how new problems are found.
  - UC-12's catalogue is kept as history, not as the first consumer's requirement.

### 3.3 Per-person numbers behind a separate door

- **What the ops PRD says:** R9, "serve three audiences — team, individual, management — from one log"; R10, performance "behind its own door"; the team sees "views about the work, not about people". Its assumptions add that team-visible per-person numbers "would reverse a recorded decision (ADR 0006)".
- **What the engine does:** M6's standard metrics include open work per assignee, cycle time per assignee and time with each assignee. Every reader who can see the jobs can read them, since T5 restricts a metric by the objects it counts, not by who asks. No declaration can restrict who may read a metric (searched in `DESIGN.md` and `declaration-syntax.md`).
- **The question:** should a metric, or its split by person, be readable only by holders of a capability, the management door? T5 and M6 would gain an audience rule.

### 3.4 The first release is reimbursement, not the inventory port

- **What the ops PRD says:** stage 0 is the foundation (the event log, an outbox, people records, the migration map). Stage 1 is reimbursement, stages 2–3 people, calendars and vehicles, stages 4–6 the match. The switch-over from the inventory system is stage 9, "last, because it is the only step that can't be undone".
- **What the engine does:** PRD §10 makes its first release "the first consumer's port": F1–F5, all of D, M1 and M6, and N5. Its worked examples, the unit's journey and the flow review, are all about inventory.
- **The question:** should the engine's first release be re-cut around what reimbursement needs, with the port (N5) moved to the release before stage 9? Reimbursement needs:
  - requests with lines, and lifecycles;
  - approval by a function ("gates are functions, not people");
  - an agent that may prepare a financial write but never commit one (T6);
  - recording lag;
  - the management door of §3.3.

  It would also be worth a second worked example, since reimbursement exercises mechanisms the inventory journey does not.

### 3.5 Who allocates a unit bought for an order

- **What the ops PRD says:** the match "decides whether the lines can be met, when, and by whom". "A yes becomes a commitment", and a firm reservation is made when a quote is accepted. Who decides between competing priorities is its open decision 9, "by principle … a function, not a named person".
- **What the engine does:** the flow review offered two options: a person assigns at arrival, or the commit reserves at arrival (ADR-0108).
- **What the ops PRD implies:** neither. The line was reserved against incoming supply when the work was committed, so at arrival there is nothing left to decide. The match, an upper-layer application, owns allocation, and a function owns priority. The engine keeps the link from the unit to the work that caused it, and records the reservation the match requests. No cascade reserves at arrival.

### 3.6 A kind of work as configuration

- **What the ops PRD says:** R11, "add a new kind, function, group, model, template or connected system as data, not as a release"; the request's kind is one of twelve.
- **What the engine does:** a new kind is either a published declaration or catalogue data. Both are "not a release" of code, and a publish is governed by F7.
- **What the ops PRD implies:** with one request model and twelve kinds, a kind reads as data: a catalogue entry (D7) with its template, not a type per kind. This is compatible, and worth stating in the first consumer's declarations when they are written.

## 4. The flow review's open decisions, checked

| Flow review §7 | What the ops PRD says | Settled? |
|---|---|---|
| 1, owners and target times on the delivery, the shipment and the missing unit | Each status "names who it is waiting on", and gates belong to functions. Due dates are set by people, and expectations are "observed, never declared" | **Yes.** Record who each status waits on, a person or a function. Declare no target times; compare against due dates people set and against history |
| 2, quarantine and inspect returns; hold units that keep failing | Flag rather than block (R6); whether a unit works is an inventory fact | **Yes, as flags.** Record the condition, and flag an uninspected return and a unit that keeps failing; block nothing |
| 3, explicit assignment or reserve at arrival | The match allocates at commitment; a function decides priority | **Yes, neither** (§3.5). Keep the link to the causing work |
| 4, a `READY` stage, physical fulfilment stages, where `SOLD` falls | Capture what tools already produce (R17); ask for a status only at a handoff a person is already making | **In part.** Shipped and delivered come from shipment and carrier records. `READY` only if it is a real handoff. Where `SOLD` falls is still finance's |
| 5a, the pool as finished fulfilment | Internal moves faked as sales are a named problem | **Yes**, with the transfer to the pool |
| 5b, time apportioned across months | Management's "where time goes"; the team's "where time goes, by who it waits on" | **Yes, needed.** This reverses the earlier reading that no use case asked for it; the ADR it needs is now justified |
| 6, the rest | A promised date is central: "people set due dates", and "promises are kept" is measured. Late receipt is recording lag, measured and flagged, not an approval gate | **Mostly.** Promised date yes; late receipt as a flag; pool transfer yes; waiting as "who it is waiting on" rather than on-hold states. Partial commit, write-off and overrides into intake are silent |
| 7, a manager's view of the page | Three audiences from one log (R9) | Not a product question; the page's own |

## 5. What it answers of the requirement questions put to the author

These are the questions put to the author on 2026-09-25, before this check was made.

| Question | Answered by the ops PRD? |
|---|---|
| 1, who ObjectFlow is for | No: the ops PRD does not mention ObjectFlow |
| 2, who writes flows | In part: "one developer plus agents", and new kinds "as data" (R11) |
| 3, what the first release must do | Yes: reimbursement first, the inventory switch-over last (§3.4) |
| 4, how much agents run | In part: same API and permissions; the first job is classifying expense claims; never committing a financial write |
| 5, L2, decisions re-evaluable from the record | No support found. It requires one event record and verified external writes (R7), not re-evaluating a rule's decision. A candidate to relax |
| 6, D11, value history | Yes, needed: time by who it waits on |
| 7, D5, occurred against recorded time | Yes, needed: recording lag is measured |
| 8, L5, rules that read metrics | As flags against norms taken from history, yes (§3.2); as refusals, no |
| 9, C5, a latency target | No |
| 10, the archive of what the port leaves | Yes: every row survives (R5) |
| 11, telemetry | Not mentioned; stays out of scope |

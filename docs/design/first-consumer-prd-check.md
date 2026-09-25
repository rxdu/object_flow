# The first consumer as a case study: what its own requirements ask of the engine

Status: **check, awaiting the author**, 2026-09-25. At the author's request, the operations platform's PRD was read in full and set against the engine's PRD, its recent decisions and the flow review's open decisions (`flow-review.md` §7). Nothing here changes a requirement: each finding is a question for the author, and the PRD changes only once the author answers.

**The source.** The operations platform's PRD is a Claude document, "Weston Robot Operations Platform — PRD", revision 17. It says it is drawn from its own repository's `DESIGN.md`, ADRs 0001–0037 and conversations up to 23 September 2026. That repository is not on this machine, so its design documents were not read. Cited below as `ops PRD`, by section and requirement number.

**How it is used.** The author intends to build the operations platform on ObjectFlow, and said on 2026-09-25: "objectflow should not be specialized just for operations platform, I'd rather treat it as a case study because it provides real use cases instead of overly simplified mockup ones". So the ops PRD is evidence, not a second specification:
- its needs are use cases the engine must be able to serve;
- where it asks for something the engine lacks, the question is whether the engine should offer it as a **general mechanism**, one any deployment could use;
- its policies — which mechanism it uses, and how strictly — stay in its own declarations.

*(Corrected 2026-09-25, after the author's framing: this paragraph first said that "where the two disagree, the ops PRD should win". That would have written one deployment's policy into the engine, which is the opposite of the author's intent, and §3's questions were framed the same way. They are rewritten below.)*

**Why it matters.** Until now, the engine's evidence about its first consumer came from two places. One is the production inventory system (`wr_inventory_management`), read at `4109939`. The other is `wr:docs/proposals/operations-system-design.md`, a draft dated 2026-07-19 that says "Nothing here is committed to implementation yet"; the engine PRD cites it four times (§2; D11; M7; UC-12). The ops PRD is two months newer and is the author's reviewed statement of what the first consumer needs.

## 1. What the first consumer is, in the engine's terms

The ops PRD describes "an operations platform that tells Weston Robot whether it can take work on, at the moment of quoting — and that learns from its own history".
- **Work is one model.** A request is made of lines, and a match turns lines into reservations of people and hardware. There are twelve kinds of work.
- **The inventory system becomes one module**, "the facts about physical items", and its switch-over is the last delivery stage (stage 9).
- **The first workflow built is reimbursement** (stage 1): "nothing to migrate, no time slots to fight over; proves requests, lines, lifecycles and a verified Xero write".
- **Its lifecycle requirements are what ObjectFlow provides.** R13 says "track every managed object through a declared lifecycle", R14 "let each workflow declare which transitions it performs, as configuration", and R15 "keep lifecycles configurable, reviewable and checked automatically". Its recording requirements (R17–R19) and its audiences (R9, R10) are what the engine's data capture and metrics serve.
- **Its non-goal "a general workflow engine"** means the platform does not build one: it uses one.

## 2. Where the case study supports the engine's requirements

| Ops PRD | Engine requirement or decision it supports |
|---|---|
| R13–R15, lifecycles declared, configurable and checked | F1, F5, F7 and the checked declaration language |
| Principle "Record what happened"; R18, "record structure — states, who each is waiting on, handoffs, links, decisions"; journey 3, "time adds up by waiting party" | D1, and **D11**: who each status waits on is a tracked value whose time adds up. The case study gives D11 a use case it lacked |
| "A status someone clicks later carries the time of the click … evidence and clicks are compared, so recording lag is itself measured" | **D5**, occurred time kept apart from recorded time, and recording lag as a measure |
| R8, agents have "the same API and permissions as people, except that they can never commit a financial write" | **T6**: a rule that depends on the kind of actor, which the declared kind makes trustworthy |
| "Agents … the same API and MCP interface as people"; one developer plus agents | N7, one interface for every caller; F3 |
| R5, "preserve every row of the inventory system"; "row counts reconcile per table in every migration rehearsal" | N5, and it answers N5's open question: what the engine holds no object for must still survive, in an archive the platform keeps, so the counts reconcile |
| "Purchases link back to the work that caused them" (journey 5) | `procured_for`, the link from a unit to the order it was bought for (flow review §2.2) |
| "Goods only move one way … internal moves are faked with a customer record named after ourselves" | the flow review's transfer to the pool, in place of an internal delivery |
| R2, "refuse double-booking in the database itself, not in application code" | invariants, enforced at serialisable isolation and compiled to an exclusion constraint on PostgreSQL (ADR-0041); with N7 no application code writes around them |
| Management needs "where time goes"; the team sees "where time goes, by who it waits on" | C3 over periods: time apportioned to the months it spans, which ADR-0103 §5 records as missing |

## 3. Where the case study asks for more than the engine offers

Each finding separates the **general need**, a mechanism any deployment could use, from the platform's own **policy**, which stays in its declarations.

### 3.1 Flags as well as refusals

- **What the ops PRD says:** "Permissive by default … The system flags rather than blocks, and blocks only what is irreversible and lands outside the team". R6: "communicate and enforce every restriction; default to flagging".
- **What the engine does:** a guard refuses. A clause marked `observe` records a would-be refusal without refusing, but only as a trial before enforcement (V3): the rule set prints it as a trial, and the caller is not told, since `Satisfied` carries no flags (`library-api.md` §4).
- **The general need:** a rule a deployment declares as a **flag**, evaluated on every request, never refusing, returned to the caller with its verdict, recorded and counted. Any deployment has rules it wants seen rather than enforced.
- **The platform's policy:** flags by default, with refusals only for external financial writes and double-booking. That is a choice among its declarations, not an engine default.
- **The question:** should the engine offer flags as a permanent kind of rule?

### 3.2 Norms observed in history

- **What the ops PRD says:** R18, "derive problems from it, never hard-code them"; a principle, "Observed, never declared: expectations … come from history".
- **What the engine does:** M7's conditions compare against a date or a threshold: a declared value (`flag slow when over 10 days`), or one held as data (a reorder point, L4).
- **The general need:** a condition that compares against the **norm observed in history**, for example beyond the usual range of time in a state for that kind of work, beside a committed date, a threshold held as data and a fixed declared value. A contractual service level of four hours is a legitimate fixed value for another deployment. That gives L5, rules that read metrics, its first real use.
- **The platform's policy:** it uses committed dates and observed norms, and no fixed expectations.
- **The question:** should M7 be widened to include observed norms? UC-12 stays a use case, taken from the July draft as an illustration rather than from the first consumer's current requirements.

### 3.3 Who may read a metric

- **What the ops PRD says:** R9, three audiences from one log; R10, performance "behind its own door"; the team sees "views about the work, not about people", and team-visible per-person numbers "would reverse a recorded decision (ADR 0006)".
- **What the engine does:** M6's standard metrics include open work and cycle time per assignee. Every reader who can see the jobs can read them, since T5 restricts a metric by the objects it counts, not by who asks. No declaration can restrict who may read a metric (searched in `DESIGN.md` and `declaration-syntax.md`).
- **The general need:** a metric, or its split by person, readable only by holders of a capability. Performance, pay or health figures are sensitive in most organisations. Because every caller reaches the engine through one interface (N7), the restriction has to be in the engine, or any caller's agent could ask for the figure directly.
- **The platform's policy:** its management door.
- **The question:** should the engine offer an audience rule on metrics? And should per-person splits sit behind one by default, as a safe general default?

### 3.4 What the engine delivers first

- **What the ops PRD says:** stage 0 is the foundation (the event log, an outbox, people records, the migration map). Stage 1 is reimbursement, stages 2–3 people, calendars and vehicles, stages 4–6 the match. The switch-over from the inventory system is stage 9, last.
- **What the engine does:** PRD §10 makes its first release "the first consumer's port": F1–F5, all of D, M1 and M6, and N5. N3 sizes the first release by the first consumer's inventory, and N5 requires porting that consumer's data.
- **The general need:** a release order in which the general core comes first and each optional capability arrives when a real deployment needs it. The first deployment's sequence is evidence for that order, not its definition. On that evidence the first release is the general core: declared lifecycles, the record, standard metrics, the service, actor types, flags and external checks. Importing legacy data (N5) comes before the platform's stage 9.
- **The question:** should the release order be re-cut this way? And should N3 and N5 be reworded as general capabilities — sized for operations rate, and importing a legacy system's data and history — with the first consumer as their acceptance case rather than their subject? A reimbursement worked example would test mechanisms the inventory journey does not: line items as parts, approval by a function, an agent classifying items, a verified external write, and flags.

### 3.5 Who allocates a unit bought for an order

- **What the ops PRD says:** the match "decides whether the lines can be met, when, and by whom". "A yes becomes a commitment", and a firm reservation is made when a quote is accepted. Who decides between competing priorities is its open decision 9.
- **What it means:** this is the platform's own logic, an upper-layer application, and asks nothing new of the engine. The engine records the reservations the match requests, and keeps the link from a purchase to the work that caused it. The flow review's decision 3 closes, for the case study, as neither of its options.

### 3.6 A kind of work, declared

- **What the ops PRD says:** R11, "add a new kind, function, group, model, template or connected system as data, not as a release"; R14, "let each workflow declare which transitions it performs, as configuration".
- **What it means for the case study:** each kind is a declared type in a `Request` family (ADR-0026). That way each kind declares its own transitions (R14), its rules print on their own (F1), and work across kinds is still one query. Templates are catalogue data (D7). A publish is configuration, not a code release, which meets R11. Nothing new is asked of the engine.
- *(Corrected 2026-09-25: this section first read a kind as catalogue data rather than a type per kind. That ignored R14, since kinds with different steps need different declared transitions.)*

## 4. The flow review's open decisions, checked

These settle the case study's own declarations, the unit's journey as the platform would write it. They are not engine requirements, except where a general need is named.

| Flow review §7 | What the ops PRD says | Settled? |
|---|---|---|
| 1, owners and target times on the delivery, the shipment and the missing unit | Each status "names who it is waiting on", and gates belong to functions. Due dates are set by people, and expectations are "observed, never declared" | **Yes.** Record who each status waits on, a person or a function. Declare no target times; compare against due dates people set and against history, which needs §3.2 |
| 2, quarantine and inspect returns; hold units that keep failing | Flag rather than block (R6); whether a unit works is an inventory fact | **Yes, as flags**, which needs §3.1. Record the condition, and flag an uninspected return and a unit that keeps failing |
| 3, explicit assignment or reserve at arrival | The match allocates at commitment; a function decides priority | **Yes, neither** (§3.5). Keep the link to the causing work |
| 4, a `READY` stage, physical fulfilment stages, where `SOLD` falls | Capture what tools already produce (R17); ask for a status only at a handoff a person is already making | **In part.** Shipped and delivered come from shipment and carrier records. `READY` only if it is a real handoff. Where `SOLD` falls is still finance's |
| 5a, the pool as finished fulfilment | Internal moves faked as sales are a named problem | **Yes**, with the transfer to the pool |
| 5b, time apportioned across months | Management's "where time goes"; the team's "where time goes, by who it waits on" | **Yes, needed.** It is a general need as well (C3 over periods), so the ADR it needs is now justified |
| 6, the rest | A promised date is central: "people set due dates", and "promises are kept" is measured. Late receipt is recording lag, measured and flagged, not an approval gate | **Mostly.** Promised date yes; late receipt as a flag; pool transfer yes; waiting as "who it is waiting on" rather than on-hold states. Partial commit, write-off and overrides into intake are silent |
| 7, a manager's view of the page | Three audiences from one log (R9) | Not a product question; the page's own |

## 5. What it answers of the requirement questions put to the author

These are the questions put to the author on 2026-09-25, before this check was made.

| Question | Answered by the case study? |
|---|---|
| 1, who ObjectFlow is for | Answered by the author instead: a general engine, with the operations platform as its first consumer and case study |
| 2, who writes flows | In part: "one developer plus agents", and new kinds as configuration (R11, R14) |
| 3, what the first release must do | Evidence for the order: the platform needs reimbursement first and the inventory port last (§3.4) |
| 4, how much agents run | In part: same API and permissions; the first job is classifying expense claims; never committing a financial write |
| 5, L2, decisions re-evaluable from the record | No support found. The case study requires one event record and verified external writes (R7), not re-evaluating a rule's decision. A candidate to relax |
| 6, D11, value history | Yes, needed: time by who it waits on |
| 7, D5, occurred against recorded time | Yes, needed: recording lag is measured |
| 8, L5, rules that read metrics | As conditions against norms taken from history, yes (§3.2); as refusals, not in this case study |
| 9, C5, a latency target | No |
| 10, the archive of what the port leaves | Yes: every row survives (R5) |
| 11, telemetry | Not mentioned; stays out of scope |

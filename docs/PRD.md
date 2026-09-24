# ObjectFlow — Product requirements

**Status: baseline, revision 7, 2026-09-24.** Revision 4 states, at the author's direction, what the engine is and where the work of managing a flow belongs (§1, N6, UC-20); revision 5 makes the document agree with itself, after a review that read it alone; revision 6 states, at the author's direction, that a transition may cause transitions on related objects when its rule says so clearly enough that none comes as a surprise (F8, UC-21); revision 7 accepts, at the author's direction, the eight revisions the design had proposed (§11). The author made this document the baseline on 2026-09-23: every design choice is checked against it, never the reverse. Its Inferred rows and the two questions of §10 stay open to the author's decision, and a design that finds the PRD wrong proposes a revision in §12 rather than working around it; until the author decides one, the design meets its proposed wording, and the traceability map marks those rows as such. Written from the author's statements of 2026-09-23 and 2026-09-24 and the standing objective of [`DESIGN.md`](DESIGN.md) §1. Revision 2 followed a review of revision 1 for validity and clarity, and revision 3 a check of revision 2 against the same review; §11 says what changed in each and why. This document states what the product must do. It does not choose designs: [`design/data-driven-engine.md`](design/data-driven-engine.md) evaluates them against the use cases in §7, the decisions begin with ADR-0081 to ADR-0086, accepted 2026-09-23, and [`design/traceability.md`](design/traceability.md) maps each requirement to the decisions that meet it.

**How to review it.** Every requirement in §6 names its source: **Said** is the author's own statement of 2026-09-23 or 2026-09-24, **Carried** is a standing requirement of the existing record, and **Inferred** is this document's reading of what the other two imply. Inferred rows are where it is most likely to be wrong, and where a Said row rests on an interpretation of the author's words, the row says which. Each requirement also names the use cases that test it; the few that no use case tests say why. §10 has the two questions this document asks the author, and §12 is where a revision the design proposes waits for them; none is waiting now.

## 1. Summary

ObjectFlow is the foundational layer on which people and AI agents **define flows** and **collect data** about them, so that the business logic built on top can be **driven by data**, whichever kind of actor is driving it.

A flow is the declared lifecycle of one or more object types — states, transitions and the rules on them — together with the datapoint kinds and formulas declared on those types (§5). Data is everything the flow produces by running — every transition, every refusal, every override, every change of hands — together with what users choose to record, and values computed from both by formulas users define. Metrics over that data evaluate the flows whenever anyone asks, and that evaluation is what lets a user start with an imperfect flow and **converge** on a good one, with the engine showing where the flow and reality disagree.

**A governed core, not a platform.** ObjectFlow is the core that operations applications are built on, and not the application itself. It holds the declared flows, enforces their rules on every request, records everything they produce and computes the formulas declared over that data. How a flow is **managed** — who requests which transition and when, chasing, scheduling, assigning, notifying, the worklists and screens people work from — and how its datapoints are **put to use** — dashboards, alerts, analysis beyond the engine's declared metrics and diagnostics, decisions taken outside the declared rules — are built in **upper-layer applications** on top of it, which act through the same interface and rules as anyone else (N6, UC-20). The flows themselves are declared by the deployment and executed by the core; declaring them is not an upper-layer concern.

**Beside platforms, not one of them.** Full metadata-driven stacks, such as [ObjectStack](https://objectstack.ai/docs/getting-started), ship the data model together with automation flows, scheduled jobs, approvals, notifications and screens. ObjectFlow does not compete on automation, scheduling or screens, and an application layer — a platform among them — may be built on it. What it offers beneath them is the guarantee that governed state changes only as F2 allows and never reaches a condition an enforced rule forbids (T1), and the record that explains every decision the engine's rules make and measures every flow. Recorded at the author's direction, 2026-09-24 (ADR-0104).

It keeps the objective the record has had from the start: **trust under delegation**. Nothing a person or an agent does can put the governed state into a condition its rules forbid, except through an override — one a type declares, or the data import's — which is capability-gated and recorded.

## 2. The problem

**Rules are scattered, and nobody can see them.** In the first consumer, 454 sites refuse an operation; 176 of them restate a rule found at another site, 51 distinct rules are written more than once, and one of them five times in four files ([`README.md`](../README.md)). What is allowed cannot be printed, reviewed or handed to an agent.

**Flows are defined once and never evaluated.** The first consumer's own operations design names its root cause as *"the moment an order is committed produces no durable, structured record"*: the availability check is done by hand, re-done repeatedly, and "never recorded" (`wr:docs/proposals/operations-system-design.md` §1). A system of record that records the result of each step and nothing about the flow cannot say where work gets stuck, which rule is in the way, or whether a change to the process helped.

**What has not been modelled ends up in free text.** Order lines arrive as prose — *"G1 U2 + Dex1-1"* — that engineers must interpret (same, §1). The first consumer holds 684 free-text notes attached to eleven different kinds of record, and 6,391 audit rows ([`design/publish-and-import.md`](design/publish-and-import.md) §8). Information arrives before anyone has decided its shape, and a system that only accepts modelled data loses it or drives it somewhere unstructured.

**Nobody defines a flow perfectly the first time.** A system that demands the complete rules before it runs is worked around. The first consumer's own cascade hooks bypass its guards through a `force_transition` (`TODO.md`, "Challenges from the first consumer", item 6), and transitions were declared that nothing could reach (the inventory's ADR-0002).

**Agents now act on business systems.** The first consumer already runs agents against its API with scoped keys, mandatory idempotency keys and optimistic versions (`wr:docs/agent-operations.md`). They need rules they cannot bypass, answers they can act on when refused, and data they can reason over — the same data a person sees.

## 3. Goals and non-goals

**Goals**

| # | Goal |
|---|---|
| G1 | People and agents define flows declaratively, and the definition is the only authority on what may change |
| G2 | Every flow produces data about itself with no extra effort from its author |
| G3 | Users record their own datapoints easily, including before they have decided the data's final shape |
| G4 | Users define formulas that turn raw data into derived datapoints and metrics, declared once and read identically by every reader over the same data |
| G5 | Business logic can depend on that data, whether the engine's rules execute it or people and agents do. Every decision the engine's rules make can be explained from the record for as long as the record of it is kept (T3), and every action people and agents take is recorded |
| G6 | The engine helps users converge: it shows where a flow and reality disagree, lets a change be trialled and measured, and keeps history intact across changes |
| G7 | Trust under delegation holds throughout (§1) |

**Non-goals**, each marked with its source:

| Non-goal | Source |
|---|---|
| Managing a flow and putting its data to use, which upper-layer applications do (N6): rendering the interfaces and worklists people work from; causing external effects — sending mail, raising invoices, posting to Jira — by observing events (L6); initiating work — schedules, timers, reacting to events; choosing among alternatives — which engineer, which unit, which order first; dashboards and alerts. The engine supplies what they need, answers their questions by query, and records what they choose | Said, 2026-09-24, for placing all of this in upper-layer applications: "the way we manage the transitions and make use of the datapoints should be built in upper-layer applications"; each exclusion listed is Carried |
| Formulas that need data or rules the store does not hold, such as a tax table or a pricing engine | Carried (ADR-0007) |
| Open-ended exploratory analysis — ad-hoc querying, charting, notebooks; the engine may export its data in an analysable shape for tools built for that (M5) | Inferred |
| Working-hours calendars and local-time rules in durations and time buckets, in the first release | Carried ([`design/edge-cases.md`](design/edge-cases.md), "Business hours and local time") |
| High-rate machine telemetry — sensor streams at seconds rate; a summary of one may be recorded as a datapoint | Inferred, and §10 asks |

## 4. Who uses it

| Actor | Kind | What they do |
|---|---|---|
| Flow author | human | Defines and changes types, flows, rules, datapoint kinds and metrics |
| Flow author | agent | Drafts flow changes from evidence (F6, F7) |
| Operator | human | Requests transitions and records datapoints in the course of work — an engineer running an inspection, a coordinator committing an order |
| Operating agent | agent | The same, through the same interface and the same rules; today's first-consumer agents create deliveries, reserve units and attach service parts |
| Owner or reviewer | human | Reads the printed rules and the metrics, approves flow changes, and reviews overrides |
| Upper-layer application | service or agent | Manages the flow and puts its data to use: asks the engine what may be done, what is due and who holds what, chases and allocates, notifies, and shows people their worklists and dashboards. Integrations are among them: keeping an externally owned type in step with its owner (Xero), reflecting state outward (Jira). It acts through the same requests as any caller and has no path of its own (N6) |

The engine treats human, agent and service alike except where a rule says otherwise, and records which kind acted every time.

## 5. Concepts

| Term | Meaning in this document |
|---|---|
| **Flow** | The declared lifecycle of one or more object types — states, transitions, the rules on them and the outcomes they apply — together with the datapoint kinds, formulas and conditions declared on those types. All of it is versioned together and changes only by publishing (F5), governed as F7 says. Catalogues that operators maintain as data (D7) are not part of it |
| **Rule** | A condition the engine checks: a guard a transition must satisfy — who may request it among them — or an invariant a type must keep |
| **Governed state** | An object's state and attributes, which change only as F2 allows: through a declared transition, an override, a flow change's declared mapping (F5), or the erasure of a personal value (D8), each recorded |
| **Override** | A change to governed state that does not satisfy the rules: available only where a type declares one, or to the data import and migration path under a deployment capability; always with a reason; always recorded as an override |
| **Admitted violation** | An invariant that an override is allowed to break, named by the override; it stays listed with it until the invariant holds again (T4). It is part of an override, not a separate route |
| **Cascade** | A transition or creation that another transition causes on a related object, in the same request, because the causing transition's declaration says so (F8). It is part of the request that caused it, never a transition the engine starts by itself |
| **Declared limit** | A bound a declaration puts on how much one request may touch, such as the number of objects a cascade may reach |
| **Exception** | Two senses, both in scope. An **engine exception** is a request the engine refused, an override applied, a conflict between concurrent requests, or a request stopped at a declared limit (D2). A **business exception** is work deviating from what the flow expects: overdue, ageing, a service level breached (M7) |
| **Datapoint** | A fact about a flow or an object, time-stamped. Three kinds follow; the first two are also attributed to the actor behind them |
| **Flow-generated datapoint** | Produced by the engine as the flow runs: a transition, an engine exception, a change of assignee. Time spent in a state or with an assignee is derived from these |
| **Recorded datapoint** | Recorded by a person, agent or service about an object: an inspection result, a measurement, an observation, a label |
| **Derived datapoint** | A value a user-defined formula computes for one object, such as how long a job has waited for parts |
| **Metric** | A value a formula computes across many objects, grouped and windowed as it declares, such as median lead time per supplier per quarter; a formula over one object is a derived datapoint. The engine supplies standard metrics; users define others |
| **Assignee** | Whoever is responsible for an object at a given time — a service job's engineer-of-record, for instance — as distinct from whoever happens to act on it |
| **Convergence** | Improving a flow over time from the evidence its own data provides, without losing history |
| **Engine** | ObjectFlow itself, also called the core or the store |
| **Deployment** | An organisation running the engine, and what it declares: its flows, rules, datapoint kinds and metrics. The **first consumer** is the first deployment: the author's operations platform, whose inventory system this document draws on |
| **Upper-layer application** | An application built on the engine that manages a flow or puts its data to use — schedules, chasing, allocation, notifications, worklists, screens, dashboards, integrations. It acts through the engine's interface like any caller (N6) |
| **Reader** | Anyone or anything that reads through the engine's interface: a screen, an agent, a report, or a rule |
| **Adversarial harness** | The acceptance test of N4: fallible actors and races run against the engine, checking from outside that none of its failure conditions is reachable |
| **Slot** | A line of a delivery that one unit fills |

## 6. Requirements

Priorities: **Must** is required for the product to meet its purpose; **Should** is expected, and its absence needs a reason; **Could** is worth having if the design makes it cheap. No Must depends on a Should.

### 6.1 Flows

| ID | Requirement | Priority | Source | Use cases |
|---|---|---|---|---|
| F1 | Users define object types, lifecycles, transitions and rules — who may request each transition among them — as a declaration that is inspectable at runtime and printable for review. A person or an agent may write a declaration; it takes effect only when published (F5), and drafting a change to a published flow is F6 | Must | Said, Carried; the authorship clause is Inferred from G1 | — carried; the existing design's renderers and harness test it |
| F2 | Governed state changes only through a declared transition whose rules pass, through an override, through a flow change's declared mapping (F5), or through the erasure of a personal value (D8), and each is recorded | Must | Carried | UC-13, UC-17, UC-19 |
| F3 | People, agents and services act through one interface, under one set of rules, and discover through it what they may do now; the kind of actor matters only where a rule says so | Must | Said in part ("regardless of driven by human or AI agents"); services and this form are Inferred | UC-11, UC-16, UC-20 |
| F4 | A refusal says which rule refused, and whether the caller should supply something, ask someone, wait, work on another object first, or take a different path | Must | Carried | UC-2 |
| F5 | A flow changes by publishing a new version. The new rules apply from publication, including to objects already in flight; an object in a state the new version removes is moved by a declared mapping; history stays readable under the rules that applied when it was written | Must | Carried | UC-13 |
| F6 | Agents, as well as people, may draft changes to a published flow through the governed path of F7 | Should | Inferred from "users (human or AI agents) define flows"; writing a first declaration is F1 | UC-15 |
| F7 | A change to a flow is governed: who may draft and who may approve are declared, the approver is not the drafter, a change an agent drafted is approved by a person, and an impact report — what the change affects among live objects and pending work — exists before it applies | Should | Inferred | UC-14, UC-15 |
| F8 | A transition may cause transitions and creations on related objects, in the same request and all or nothing, when its declaration says so clearly enough that none comes as a surprise: the consequence is declared on the transition that causes it, printed in the rules of every type it reaches, checkable before the request, and recorded with its cause. The engine starts no transition that no request caused (§3) | Must | Said, 2026-09-24: "transition cascading should be supported, as long as the rule is defined by the user clearly enough that the transition should not appear as a surprise"; all or nothing is Carried (ADR-0019, from the first consumer's delivery completion); the four marks of "clearly enough" are Inferred | UC-21 |

### 6.2 Data capture

| ID | Requirement | Priority | Source | Use cases |
|---|---|---|---|---|
| D1 | Every transition is captured with no effort from the flow's author: which object, which transition, from and to which state, who, which kind of actor, when, what caused it, and which version of the rules applied | Must | Said, Carried | UC-1, UC-16, UC-21 |
| D2 | Every engine exception (§5) is captured the same way: refusals with the rule and remedy, overrides with their reason, admitted violations, conflicts between concurrent requests, and requests over a declared limit | Must | Said ("exceptions"), read in the engine's sense; the business sense is M7 | UC-2, UC-3 |
| D3 | Users record their own datapoints about an object easily: adding a kind of datapoint is one declaration and no code, and recording one is a single request with no transition declared for it. Values may be numbers, choices from a list, text, files and times, and a number's unit is stated where its kind is declared | Must | Said; the measure of "easily" and the value types are Inferred | UC-4, UC-5 |
| D4 | A recorded datapoint is never edited: a mistake is corrected by a newer datapoint linked to it, and both stay visible. Erasure (D8) is the one exception, and removes only personal values | Must | Inferred from the recorded property | UC-8 |
| D5 | A datapoint of a declared kind — a transition as much as a recorded one — distinguishes when something happened from when it was recorded, within a bound the declaration sets, because people record after the fact and durations computed from recording time are wrong; an override is dated when the engine was told, and a label, which records that someone noticed something, when it was recorded | Should | Inferred; the clause on overrides and labels accepted by the author, 2026-09-24 | UC-6, UC-19 |
| D6 | Users can record information before it has been modelled — a label or a note on an object — and the engine can count it and relate it to the flow, so that it can be formalised later. Labels and notes are built into every type, so a flow that declares no datapoint kinds (V1) can still be labelled; who may label is declared once for every type, and narrowed where a type says so (D12) | Must | Inferred from "users should not need to define everything perfectly"; Must because V2 depends on it | UC-13 |
| D7 | Operators maintain catalogues of what to record — an inspection checklist per product configuration, for instance — as data, without publishing a new version of the flow. A catalogue's entries are governed objects, changed only by their own recorded transitions under their own rules (F2), so a rule that reads a catalogue is loosened only through a recorded, attributed change | Should | Inferred from the first consumer's inspection design | UC-4, UC-19 |
| D8 | Personal data inside datapoints is erasable, like personal data anywhere else in the store | Must | Carried | UC-17 |
| D9 | Recording a datapoint does not change the state or attributes of the object it describes; it affects that object's flow only through a rule that reads it | Must | Inferred from F2 and G7 | UC-4, UC-19 |
| D10 | Every change of assignee is captured from the start — at creation and at every reassignment — with who was assigned, who made the change, which kind of actor that was, and when | Must | Said: "assignee change is also important and I'd like to keep track of this kind of data from the beginning" | UC-18 |
| D11 | For every reference to a single other object — beyond the assignee, which M6 requires — and every choice-from-a-list attribute holding one choice, how long an object held each value is available without its author declaring which to track: who, where and which bucket an object was in over time. References to a set of objects, such as a team, and attributes holding several choices are not covered yet | Should | Inferred from "this kind of data"; the first consumer's design asks for "an append-only movement history" answering "where did this unit go" (`wr:docs/proposals/operations-system-design.md` §3); the exclusion of attributes holding several choices accepted by the author, 2026-09-24 | UC-18, for who; where and which bucket have no use case yet |
| D12 | Recording a datapoint is authorised and validated like any other write: who may record each kind is declared, and values are checked against their declared types and bounds when recorded | Must | Inferred from T1 | UC-19 |

### 6.3 Formulas and derived data

| ID | Requirement | Priority | Source | Use cases |
|---|---|---|---|---|
| C1 | Users define formulas over raw data — attributes, flow-generated datapoints and recorded datapoints — producing derived datapoints for one object and metrics across many | Must | Said; the split into derived datapoints and metrics is this document's | UC-5, UC-8 |
| C2 | A formula is declared once, inspectable and versioned with the flow, and every reader — a screen, an agent, a report, a rule — gets the same value from the same definition over the same data; a reader that may see only part of the data gets the value over that part, marked as partial | Must | Inferred from the declared property; the clause for a reader who sees part of the data accepted by the author, 2026-09-24 | UC-8, UC-11, UC-12 |
| C3 | Formulas aggregate over time windows and group by dimensions such as model, customer, supplier and month; splitting by kind of actor and by flow version is M4 and M3 | Must | Inferred from the metrics the use cases name; Must because M7's conditions across many objects depend on it | UC-1, UC-5, UC-8, UC-12 |
| C4 | A formula defined today applies to the history recorded before it existed | Should | Inferred | UC-1, UC-9 |
| C5 | Reading a derived datapoint or a metric is fast enough for interactive use at the first consumer's scale. The target is to be set by measurement, and until it is set this requirement cannot be verified | Should | Inferred | — not yet verifiable |
| C6 | Durations and time buckets are computed in UTC calendar time, and every metric says so | Should | Inferred; follows the working-hours non-goal | UC-8 |

### 6.4 Metrics and evaluation

| ID | Requirement | Priority | Source | Use cases |
|---|---|---|---|---|
| M1 | Every flow has a standard set of metrics without declaring any: time in each state, throughput, work in progress, age of the oldest open items, how often each transition is taken, refusal rates per rule, override counts per target state and per reason, and rework — returning to an earlier state | Must | Said ("metrics to constantly evaluate the flows"); the list is Inferred | UC-1, UC-2, UC-3, UC-14 |
| M2 | Metrics, standard and user-defined, are available through the same interface to people and agents, under the same visibility rules as the objects they are computed from | Must | Inferred | UC-8, UC-11 |
| M3 | Any metric whose subject exists in both versions of a flow — cycle time, time in a state both versions keep — can be split by version, so the effect of a change can be measured | Should | Inferred | UC-13 |
| M4 | Any metric can be split by the kind of actor that requested the transitions it counts, recorded the datapoints it counts, or created the objects it counts | Should | Inferred | UC-2, UC-16 |
| M5 | The engine exports its data in a shape suited to exploratory analysis elsewhere, and does not attempt the exploration itself | Could | Inferred from "potentially dig insights" | — Could |
| M6 | Every type with an assignee has assignment metrics without declaring any: open work per assignee, time unassigned, time to first assignment, time with each assignee, cycle time per assignee, handoffs per object and reassignment back to an earlier assignee, and how often someone other than the assignee acts | Must | Inferred from D10; the priority is this document's | UC-18, UC-20 |
| M7 | Business exceptions — work overdue against a date, ageing past a threshold, a service level breached — are declared once as conditions and are answerable by a query that a scheduler or an agent runs | Must | Said ("exceptions"), in the sense the first consumer's design uses: "Exception management — proactive alerts on reorder points, late POs, aging backorders, SLA breaches" (`wr:docs/proposals/operations-system-design.md` §3) | UC-12, UC-20 |

### 6.5 Data-driven logic

| ID | Requirement | Priority | Source | Use cases |
|---|---|---|---|---|
| L1 | A flow's rules can read recorded datapoints of a declared kind about the object and the objects related to it, as they read attributes; a label, which no one declares, is read by metrics and diagnostics and never by a rule | Must | Said ("build data driven business logics"), read as the engine's own rules; grounded in the first consumer's gate on inspection results; the exclusion of labels accepted by the author, 2026-09-24 | UC-4 |
| L2 | Whenever a rule's decision depends on data, the record holds the values it used, so the decision can be re-evaluated from the record alone for as long as that record is kept (T3) | Must | Inferred from the recorded property | UC-4, UC-10 |
| L3 | People and agents deciding outside the engine get the data they need through its interface, and the actions they take are recorded as any other | Must | Said ("build data driven business logics", read as decisions outside the engine too), Carried | UC-11, UC-12, UC-18, UC-20 |
| L4 | The thresholds and parameters of a data-driven rule, or of a declared condition (M7), are visible, and change only through a governed, recorded path: a flow change (F7), or a transition on the object that holds the value, such as a reorder point per model | Should | Inferred | UC-10, UC-12 |
| L5 | A flow's rules can read metrics aggregated across many objects and over time. No first-consumer use case needs this yet; UC-10 is hypothetical | Should | Inferred from "build data driven business logics" | UC-10 |
| L6 | Upper-layer applications can observe what the engine records: every change is available as an event they can subscribe to, delivered at least once and in order for each object, so that effects and reactions are built outside the engine | Must | Carried; Must because N6 and the effects non-goal depend on it | UC-12, UC-20 |

### 6.6 Convergence

| ID | Requirement | Priority | Source | Use cases |
|---|---|---|---|---|
| V1 | A flow can start minimal and be refined: a flow whose transitions carry no guards, and whose types declare no invariants, formulas or datapoint kinds, can be published and run | Must | Said; the measure is Inferred | UC-13 |
| V2 | The engine shows where a flow and reality disagree: transitions and states never used, rules that refuse most, states reached mostly by override, unmodelled information recorded often, states where work waits longest | Must | Said ("the engine helps them converge"); the list is Inferred | UC-2, UC-3, UC-13 |
| V3 | A new rule can be trialled before it is enforced: the refusals it would have made are recorded and countable, and enforcing it is a flow change (F7) | Should | Inferred | UC-14, UC-19 |
| V4 | A person or an agent can draft a flow change with the engine's evidence attached (F6, F7). The engine supplies the evidence and never drafts or applies a change by itself | Should | Inferred; the second sentence is Carried | UC-15 |
| V5 | Information recorded before it was modelled can be promoted into the model — a label becoming a state or a typed attribute — with its history intact | Should | Inferred | UC-13 |

### 6.7 Trust and governance

| ID | Requirement | Priority | Source | Use cases |
|---|---|---|---|---|
| T1 | Nothing a person or an agent does puts governed state into a condition its enforced rules forbid, except through an override. A rule being trialled (V3) is not enforced, and guarantees nothing | Must | Carried | UC-19 |
| T2 | The threat model is mistakes, not malice: actors are trusted and fallible, and one holding database credentials is out of scope | Must | Carried; an assumption, not a behaviour | — assumption |
| T3 | Every change to governed state, every recorded datapoint and every flow change is attributed to an actor, kept permanently and reconstructable, erasure of personal values (D8) excepted. Refusal records are attributed too, and are kept for a retention period the deployment sets, with their counts kept permanently by day, rule, remedy, kind of actor and flow version | Must | Carried; the retention clause is Inferred | UC-15, UC-18 |
| T4 | Overrides and admitted violations are reviewable at any time, and a data import does not bury them | Must | Carried; the second half is `design/defects.md` D205 | UC-3 |
| T5 | Visibility rules apply to every read, including datapoints and metrics; a metric does not reveal what its reader could not see | Must | Carried; the extension to datapoints and metrics is Inferred | UC-11 |

### 6.8 Non-functional

N1 is a constraint carried from the existing design rather than a behaviour a test can fail, listed so that a design breaking it is visibly out of bounds. N2 is carried the same way, and UC-20 now tests it.

| ID | Requirement | Priority | Source | Use cases |
|---|---|---|---|---|
| N1 | Runs on PostgreSQL in production and SQLite for development, tests and small deployments | Must | Carried; a constraint | — constraint |
| N2 | The core stays a library: correctness never depends on a background process | Must | Carried; a constraint | UC-12, UC-20 |
| N3 | The first release is sized for the first consumer — 279 live deliveries, 81 services, 203 warranty contracts and 123 live customers as of 2026-09-01 (inventory ADR-0003), and 6,391 audit rows over the system's life: operations rate, not telemetry rate. Event and datapoint rates are to be measured before cutover. The orders-at-volume case study, one of the design's own worked examples, stays a design reference, not a release target | Must | Carried (the counts, from inventory ADR-0003); measuring the rates before cutover is Inferred | UC-7 |
| N4 | Behaviour is deterministic under test, and the adversarial harness is the acceptance test | Must | Carried | — the harness is the test |
| N5 | The first consumer's production data and legacy history are ported and preserved, including past reassignments wherever its audit trail recorded them, except values about people and things the engine holds no object for — such as the IP address of the employee who acted — and with the stretches the legacy record is silent on reported rather than filled in | Must | Carried; the reassignment clause is Inferred from D10; what is not ported, and the silent stretches, accepted by the author, 2026-09-24 | UC-1, UC-18 |
| N6 | The engine is the governed core that upper-layer applications are built on. It contains no scheduler, workflow runner or notifier, and nothing that presents work to people — no worklist or screen — though it answers by query what such a worklist would list: what may be done now, what is due or overdue, what waits and on whom. An application that manages the flow gets no privileged path: it acts through the same requests, under the same rules, and is recorded like any caller | Must | Said, 2026-09-24 ("stay the governed core … the way we manage the transitions and make use of the datapoints should be built in upper-layer applications"), for the core and the upper layer; the list of what the core contains, the answering by query, and the no-privileged-path clause, from T1, are Inferred | UC-20 |

## 7. Use cases

Each use case is drawn from the first consumer where it can be, and says so where it cannot. Each is a test the design must pass; the acceptance line is what "passes" means. The requirements each one tests are listed against the requirements in §6, in one place, so the two cannot disagree. Where a use case's acceptance also exercises a Should, a Must it tests is accepted on the clauses that concern the Must, and the Should's clauses show whether the Should is met; so no Must waits on a Should to be accepted.

### Evaluating flows

**UC-1. Where do deliveries get stuck?** An operations lead asks which delivery states hold work longest, by customer and by product configuration, and which open deliveries are oldest in their current state. Nobody declared anything for the states, ages and waits; splitting them by customer or by configuration is one metric declaration, which also covers the history recorded before it. *Acceptance:* time in each state, current age and work in progress are available for every flow from the day it is published, with nothing declared; for imported objects, from as far back as their ported history allows; and a split by customer or configuration, once declared, covers the history recorded before it.

**UC-2. Which rule is in the way?** Deliveries are often refused at completion. The lead asks which rule refuses most, with which remedy, and whether the callers are people or agents — finding, for instance, an agent that keeps requesting completion before the checklist is done. *Acceptance:* every refusal is counted per rule, remedy and kind of actor, per week.

**UC-3. Overrides as a symptom.** Units are often put into `AVAILABLE` by override rather than through `INTAKE`, because, say, the label-and-photo rule does not fit one supplier's parts. *Acceptance:* override counts per target state and per reason, excluding the objects placed there by the data import, and each override reviewable.

### Capturing data

**UC-4. Record a pre-delivery inspection.** An engineer inspects a configured robot against its configuration's checklist — each check pass, fail or not applicable, with a measured value where the check has one, a photo and a note — and delivery is gated on every check having a result. Operations staff maintain the checklists per configuration. The first consumer's operations design states the goal: "who performed which predefined check, when, and with what result — and gate delivery on their completion" (§2). *Acceptance:* results are recorded without a transition per check; the gate reads them; the record of each completed delivery shows which results the gate read; changing a checklist needs no new version of the flow.

**UC-5. Keep what used to be thrown away.** When an order is committed, the availability check's result — what is in stock and what must be procured — is recorded, and the shortfall over time becomes a metric. This is the first consumer's first pain. *Acceptance:* the check's result is a datapoint on the order, and "open shortfall by model" is a declared metric.

**UC-6. Recorded late.** An engineer records a shipment's receipt the morning after it arrived. *Acceptance:* time in `PROCUREMENT` uses when the receipt happened, within a bound the declaration sets, and both times are kept.

**UC-7. Where machine data stops.** A robot reports battery health every second. *Acceptance:* the design says where that data lives and what, if anything, is recorded here — a daily summary per unit, for instance.

### Deriving and using data

**UC-8. A formula, declared once.** A quality lead defines first-pass yield per robot model per month: the share of inspected configured robots whose every check passed at its first recorded result. A procurement lead defines supplier lead time, ordered to received, at the median and the eightieth percentile. Operations defines the on-time delivery rate against each delivery's promised date. *Acceptance:* each is declared once; a screen, an agent and a report reading it over the same data get identical values, and one that may see only part of the data gets the value over that part, marked as partial; and each says it is computed in UTC calendar time

**UC-9. A new metric over old data.** Supplier lead time is defined today, and the lead wants the last twelve months. *Acceptance:* the metric covers history recorded before it was defined, as far as the data exists.

**UC-10. A rule that reads a metric.** *Hypothetical; no first-consumer process asks for this yet.* Deliveries of a robot model whose first-pass yield over the last thirty days is below a threshold need a second sign-off. *Acceptance:* the refusal names the threshold, and names the value wherever the requester may see what it is computed from; the record of every completion holds the value it was decided on; the threshold changes only through a governed path

**UC-11. An agent decides with data.** A procurement agent ranks open purchase orders by risk of being late, from supplier lead-time metrics and each order's age, and escalates the riskiest. *Acceptance:* the agent reads the same metric definitions a person would, under the same visibility; its escalations are recorded as its actions; the ranking itself is the agent's, not the engine's.

**UC-12. Business exceptions from declared conditions.** The first consumer's alert catalogue (`wr:docs/proposals/operations-system-design.md` §6) holds six business exceptions — low stock, procurement overdue, backorder ageing, delivery date at risk, lease overdue, warranty expiring — and one notice that is not an exception at all: order ready, which is an event, the last slot filled. *Acceptance:* each of the six conditions is declared once and answerable by a query a scheduler or an agent runs; a threshold one of them reads, such as a reorder point per model, is held on an object and changes only by that object's recorded transition; order ready is a subscription to the transition that fills the last slot; sending any alert is an upper-layer application's.

### Converging

**UC-13. Start coarse, then refine.** Service jobs start as `OPEN → DONE`. Engineers label jobs "waiting for parts" as it happens. After a month the engine shows how many jobs carry the label, in which state it was applied, and how long those jobs waited. The team publishes a `WAITING_PARTS` state with its transitions, and moves the jobs in flight into it. Later a version drops a state nobody uses any more, and the few jobs still in it are moved by a declared mapping. *Acceptance:* the label is countable and relatable to states from the first day; the new version moves in-flight jobs by recorded transitions; the jobs in a removed state are moved by the declared mapping, and each move is recorded; time-to-done is comparable across the two versions.

**UC-14. Trial a rule.** A proposed rule — a photo is required before a unit of one model becomes `AVAILABLE` — runs for two weeks without enforcing. *Acceptance:* the refusals it would have made are recorded and countable, including who would have been refused; enforcing it is a flow change.

**UC-15. An agent drafts a change.** An agent reads UC-2's and UC-3's evidence and drafts a flow change. The impact report says what it would affect; a person holding the authority approves it; it is published and attributed to both. *Acceptance:* no flow change applies without passing the governed path; the evidence and the approval are part of the record.

**UC-16. People and agents on the same flow.** The lead compares cycle time, refusal rate and rework for deliveries created by agents with those created by people. *Acceptance:* every metric can be split by the kind of actor.

### Governance

**UC-17. Erase a customer.** A customer asks to be erased. *Acceptance:* their personal values are removed from objects, history and datapoints; metrics built from those datapoints keep their counts and expose no personal value.

### Assignment

**UC-18. Who has what, and where do handoffs hurt?** Service jobs have an engineer-of-record, which an agent may not be (`wr:docs/agent-operations.md` §3), and a user with active services must have them reassigned before being removed (`wr:app/models/users.py:34`). A service lead asks how much open work each engineer holds, how long jobs wait before anyone is assigned, which jobs changed hands more than once or went back to an earlier engineer, whether cycle time differs by engineer, and how often a job is completed by someone other than its assignee. An agent balancing the workload reads the same numbers before proposing a reassignment, and a person makes it. *Acceptance:* every assignment from the first day the flow runs, including the one made at creation, is recorded with who made it; every question above is a standard metric for any type that declares an assignee; the legacy system's reassignments arrive with the port wherever its audit trail recorded them; choosing the engineer stays with the person or agent.

### Trust

**UC-19. The new paths do not open a way around the rules.** An agent that guesses and skips steps tries the routes this document adds. It records a passing inspection result it is not permitted to record, so that a delivery's gate would open. It backdates a receipt beyond the declared bound, to shorten a supplier's lead time. It completes a transition that a rule on trial would have refused, and then relies on that rule as if it were enforced. It retires a check from a configuration's checklist so that a delivery's gate opens. *Acceptance:* the first is refused; the second is refused; the third proceeds, is recorded as a would-be refusal, and the printed rules show the trialled rule as not enforced; the fourth is refused unless the agent holds the authority the catalogue's own rules require, and if it does, the change is recorded and attributed like any transition. Governed state never reaches a condition an enforced rule forbids.

### Building on the core

**UC-20. An operations application runs the floor.** An application built on the engine manages the unit's journey day to day. Each hour it asks which deliveries may be marked ready, which units bought for an order still wait to be allocated to it, which units have been missing for a week, which leases have run over, and how much open work each person holds. It marks what may be marked, allocates where its own policy says to, and tells people what is theirs. The worklist, the schedule and the messages are the application's; drawn from the operations review of the first consumer's flow ([`design/flow-review.md`](design/flow-review.md) §2.1). *Acceptance:* given declarations that record the facts it asks about, each of those questions is answered by one query to the engine, not by a scan the application computes; every action it takes is a request by an actor of kind `service` or `agent`, refused by the same rules and recorded with its reason like anyone's; the engine keeps no schedule, sends nothing and takes no step of its own; and stopping the application stops the chasing and loses nothing from the record.

### Consequences across objects

**UC-21. Completing a sale sells its units.** When sales ops completes a delivery, every unit bound to it becomes sold and gains its warranty, as one change. The first consumer does this in the completion's side effects (`wr:app/core/state_registry.py:771`, read at `4109939`). A reviewer reading only the unit's rules, and an agent about to complete a delivery, can each tell that it will happen. *Acceptance:* completing a delivery sells each bound unit and creates its warranty in one request, or refuses and changes nothing, naming the unit and the rule that refused; the unit's printed rules say that a delivery's completion sells it; the completion can be checked before it is requested, its cascades included; and each unit's sale records the completion as its cause.

## 8. How we would know it works

Measures to take rather than targets to meet; none of the numbers below is a goal someone has set.

- The first consumer runs on the engine, and its 454 refusal sites and 51 duplicated rules become declared rules, each stated once.
- On cutover day every flow has the metrics of M1, and every type with an assignee those of M6, with no instrumentation declared by anyone.
- UC-4, UC-5 and the six conditions of UC-12 are expressed without an upper-layer application computing its own copy of any metric definition.
- The time from a metric showing a problem to a published flow change, measured over the first months.
- The share of transitions requested by agents, and how their refusal and rework rates compare with people's.
- The adversarial harness passes: none of its failure conditions is reachable.

## 9. What this changes in the current design

When revision 1 was written, the existing record met F1 to F5, T1, T2, the part of T3 about changes to governed state, and most of N1 to N5; it also already met N6 and L6, which later revisions stated. These positions of the existing record conflict with the requirements above; `design/data-driven-engine.md` §5 resolves each one.

| The record says | The requirement | Where |
|---|---|---|
| Analytics over the log is out of scope and belongs to a warehouse fed by a subscriber | M1 to M4 bring flow metrics in scope | [`design/edge-cases.md`](design/edge-cases.md), "Analytics over the whole log" |
| A refused request is rolled back and nothing is recorded | D2 records every refusal | `DESIGN.md` §5.4 and §6 |
| Every write goes through a declared transition | D3 and D6 need recording to be light | ADR-0042 |
| The store decides and records; consumers compute | C1 to C3 put user-defined formulas in the engine | ADR-0007 |
| The expression language has no grouped aggregation and no time windows | C3 | ADR-0047 |
| Beyond the declared indexes the store maintains no projection | M1, M6 and D11 may need maintained flow data, such as time in state or with an assignee | `DESIGN.md` §10 |
| People write the declaration, and legibility of the printed rules is the defence against rules that say the wrong thing | F6 lets agents draft flow changes | `DESIGN.md` §13 |
| Publishing refuses on any failed check | V1 needs a minimal flow to be small and publishable, without weakening the checks that protect T1 | `design/declaration-syntax.md` §10 |

Two findings of the design review of 2026-09-23 become prerequisites rather than defects to schedule: defect D205, since T4 and UC-3 fail while imported objects crowd the override list; and defect D202, since a rule asking whether recorded datapoints changed after a sign-off — which L1 allows and UC-4's gate invites — depends on it, and the specification's own delivery approval fails without it.

*Corrected 2026-09-23.* This paragraph first named defect D210 as a third prerequisite, "since any metric computed from the log needs a reliable settled position". That was wrong. A metric computed on read sees committed rows only, so work still in flight is invisible to it by construction. The settled position matters to a consumer advancing a cursor, such as an export, and not to metrics ([`design/data-driven-engine.md`](design/data-driven-engine.md) §6).

## 10. Questions for the author

The author asked for the details to be settled by evaluating designs against the use cases, not by asking. These two are about the product rather than the design, and each has a default this document already assumes.

1. **Machine telemetry.** Assumed out of scope, with summaries recordable as datapoints (UC-7). If robots or other machines are to record directly at seconds rate, the storage design changes more than anything else in this document.
2. **Release order.** Assumed, in three releases, each depending on data the one before records:
   - **First:** the first consumer's port — F1 to F5 and F8, all of the data capture in D, the standard metrics of M1 and M6 readable by people and agents, with the grouping, windows and UTC time buckets they use (C3 and C6, as far as they apply to standard metrics) and their splits by version and kind of actor (M3 and M4, likewise), derived datapoints for one object (the part of C1 the existing design already provides) and the per-object conditions of M7 written in them, rules reading recorded datapoints with the values they read kept on the record (L1, which the inspection gate of UC-4 needs, and L2 for those rules), L3, L6, V1, T and N. Assignment is in it because the author asked for it from the beginning and history can only start on the day it is written through the store; labels are in it because the evidence for convergence starts accumulating only when recording does.
   - **Second:** formulas across many objects and user-defined metrics (the rest of C, and M2 to M5 as they apply to them), the conditions of M7 across many objects, rules that depend on derived values (L2 for them, L4, L5). Until the third release, the flow-change path of L4 is a publish, without F7's governance.
   - **Third:** convergence tooling and agents drafting flow changes — V2 to V5, F6 and F7.

## 11. Revision history

**Revision 7, 2026-09-24**, at the author's direction: "accept the §12 proposals as you reasoned". The eight rows §12 proposed now carry its wording, and §12 is empty. The reasoning the author accepted weighs each proposal by this document's own rules: an Inferred row is where it is most likely to be wrong, and no Must depends on a Should.

- **C2, G4 and UC-8.** C2's promise of one value to every reader was an Inferred row. Against it stood T5's Carried core, that visibility applies to every read, and M2; a value computed from objects a reader may not see is a read of them.
- **UC-10.** The use case is hypothetical by its own label and tests only L5, a Should, while T5 is a Must.
- **D5 and D11** now agree with themselves. D11 already excluded sets of objects, and an attribute holding several choices is a set. D5's reason, that durations computed from recording time are wrong, does not reach an override, which states what is true now, or a label, which records that someone noticed something.
- **L1** narrows a Said row. A label becomes logic by promotion (V5), and D3 makes the declared kind that replaces it one declaration.
- **N5.** D8, a Carried Must, requires every personal value to be erasable, and a value the engine holds no object for has no erasure path. Whether to keep what the port leaves behind in an archive outside the engine is left open; this document does not forbid it.

Why each was proposed, as §12 gave it:

| ID | Why |
|---|---|
| G4 | G4 promises what C2 does, and has C2's conflict with T5 for a reader who may see only part of the data |
| C2 | As written, C2 and T5 conflict for any reader who may not see every object a metric reads: giving them the same value reveals what they may not see, and T5 is a Must. The proposed wording keeps what C2 is for — one definition, never a reimplementation per reader — and says what a partial reader gets. *(Reworded 2026-09-24: this proposal said "consumer", which §5 no longer uses for a reader.)* |
| UC-8 | The same conflict as C2's, in the use case that tests it |
| UC-10 | A rule reads a metric over every row, so its value may aggregate objects the requester cannot see; naming it would breach T5. UC-10 is hypothetical and T5 a Must, so the acceptance should yield where they meet. The record still holds the value, for a reader who may see it |
| D5 | An override states what is true now, not when it became true. A label is the unmodelled residue: where the time of what it notes matters, it is promoted to a declared kind, which takes an occurred time. As written, D5 asks both to carry a second time they cannot honestly have (ADR-0106) |
| D11 | An attribute holding several choices is a set, like a team, and is tracked by the same extension, membership intervals per element, which nothing in the first consumer needs yet. As written, D11 covers it and a team does not (ADR-0106) |
| L1 | A label's name is a vocabulary nobody declared, so a rule over it would change meaning with a typo. The design reads labels only in metrics and diagnostics and makes a label logic by promoting it to a declared kind (`DESIGN.md` §5.11, check 55). §5 counts a label as a recorded datapoint, so L1 as written asks for what the design refuses (ADR-0082, ADR-0095) |
| N5 | The port keeps each legacy entry's fields about its own object, moves fields about another ported object to that object's history, and leaves behind the rest, which is third-party personal data with nowhere to live and no erasure path. The legacy record also has holes, which the engine reports as gaps rather than inventing. As written, N5 promises both are preserved (ADR-0100, ADR-0106) |

**Revision 6, 2026-09-24**, at the author's direction: "I think transition cascading should be supported, as long as the rule is defined by the user clearly enough that the transition should not appear as a surprise".

- **Cascades were assumed and never required.** The design has had them since ADR-0019, the first consumer needs them to complete a sale, and this document mentioned them only inside the declared limit. F8 states the principle with the author's condition, §5 defines a cascade, UC-21 tests it on the first consumer's delivery completion, and the first release carries it.
- **"Clearly enough" is this document's reading**, and F8 marks it Inferred: declared where it starts, printed where it lands, checkable before the request, and recorded with its cause. Checking the design against it found that a type's printed rules did not show a cascade declared on another type (ADR-0108).

**Revision 5, 2026-09-24**, after a review that read the document alone, with nothing else open, for whether it agrees with itself and reads as one product. It found thirty-one things; every one was checked against the text before it was acted on. None changes what the product is for. What changed, grouped by kind:

- **The release order contradicted the requirements it schedules.** UC-4's gate shipped in the first release without L2, which it tests; C6's UTC rule came after the metrics it governs; the grouping and the splits by version and kind of actor that the first release's standard metrics use were all in later releases; and L4's governed path, F7, did not exist until the third. §10 now puts each where its first user is.
- **Musts whose only use case also needed a Should.** F2, T1, F4, F5, V1, D6 and L1 could be accepted only through use cases that exercise Shoulds. §7 now says a Must is accepted on the clauses that concern it.
- **Absolutes with unstated exceptions, again.** F2 and the concept of governed state said "only a transition or an override", while a flow change's mapping (F5) and erasure (D8) change governed state too. §1 said an override is "declared", while the data import's is capability-gated and not declared by a type. Each now names them, and UC-13 now tests the mapping.
- **Revision 4 was added and not woven in.** The upper layer duplicated four older non-goals, an actor row and a second positioning paragraph, and "consumer" meant three things: the one who declares flows, the upper layer, and any reader. The non-goals are one row, the positioning is in §1, the integration actor is part of the upper layer, and §5 defines deployment, upper-layer application, reader, engine and the first consumer. The positioning also said ObjectFlow does not compete on "flows", which are its product; it now says automation, scheduling and screens, cites F2 beside T1, and says the record explains decisions the engine's rules make, as G5 has since revision 2.
- **Undefined or doubly defined terms.** Flow had two definitions and no word on what is versioned with it; admitted violation and declared limit were used and never defined; a metric was "across many objects or over time", overlapping derived datapoints; "worklist" in N6 was ambiguous against UC-20's queries. Each is now defined once in §5, or reworded.
- **Missing and wrong cross-references.** D4 cited UC-4, which corrects nothing, and now cites UC-8; UC-3, UC-2, UC-11 and UC-20 now cite, and are cited by, the requirements they test (M1, V2, C2, F3, M6, N2); D11 says which of its parts no use case tests; M1 lists overrides per reason and M6 cycle time per assignee, which UC-3 and UC-18 ask for; M4 covers the kind of actor that created an object, which UC-16 compares.
- **Two things the document assumed and never required.** Events, which the effects non-goal and UC-12 rely on, are L6; and who may request a transition is part of its rules (F1, §5).
- **Other contradictions.** Labels had to be declared (D12) on a flow that declares nothing (V1); D6 now makes them built in. Refusal records are pruned, so G5 and L2 now explain a decision for as long as its record is kept, and T3 names the dimensions of the permanent counts. D7's catalogues could loosen a gate unseen; they are governed objects now, and UC-19 tries that route. The export non-goal promised what M5, a Could, only permits. F1 and F6 both claimed agents defining flows; F1 is writing a declaration, F6 drafting a governed change.
- **Labels and counts.** Said now covers 2026-09-24; N6's list and query obligation, the non-goal's enumeration, T5's extension and N3's measurement clause are marked Inferred; C3 no longer claims metrics the author named; §2's duplicate counts are reconciled with §8's; §9's defect numbers no longer collide with the D requirements; §9 says what revision 1 was measured against; the status line points to the traceability map rather than one range of decisions; the owner's row no longer implies overrides are approved; F3 names services.


**Revision 4, 2026-09-24**, at the author's direction after an operations review of the worked example and a comparison with full metadata-driven platforms: "stay the governed core, write it into the PRD, the way we manage the transitions and make use of the datapoints should be built in upper-layer applications".

- **The position was only implied.** The non-goals excluded a user interface, effects, initiating work and selection one by one, without saying what the engine is instead or where that work goes, and a sound recommendation — design a scheduler and operations agent — nearly crossed the line. §1 now states the position, §3 names the upper layer as a non-goal with its source and notes platforms beside the engine, §4 adds it as an actor, N6 makes it a requirement, and UC-20 tests it.
- **The no-privileged-path clause is this document's reading.** N6 marks it Inferred from T1: an upper layer that could skip the guards would be the second write path the design refuses.
- **The proposal awaiting the author was renumbered** revision 5 (§12), unchanged; revision 5 then took that number, and §12 is now named for what it is, the proposed revision.

**Revision 3, 2026-09-23**, after checking revision 2 against the review that produced it. Every issue it listed was addressed; eleven residuals were found, several of them fresh instances of the classes revision 2 fixed:

- **An inference inside a Said row, again.** D3 asked for "numbers with units", which nobody said and the proposed design did not model; the value types are now marked Inferred and a unit is stated where a kind is declared. L1's Said is now marked as read in the engine's sense, like F3's. C1's split into derived datapoints and metrics is marked as this document's.
- **Absolutes with an unstated exception.** D4 ("never edited") and T3 ("kept permanently") both contradicted erasure (D8); each now names it. The Override concept said "available only where declared", which the import path is not.
- **A Must resting on a hypothetical use case.** L2's only use case was UC-10; UC-4 now tests it, its acceptance asking which results the gate read.
- **The release order contradicted itself.** The first release had M7's per-object conditions and deferred C, which they are written in; per-object derived datapoints, which the existing design already has, are now in the first release.
- **A requirement hidden in the actors table** — an agent's draft approved by a person — is now stated in F7.
- **"Order ready" is an event, not an exception;** UC-12 now has six conditions and one subscription, and §8 counts six.
- **D11 promised every reference,** and the proposed design covers references to a single object; sets are now said to be uncovered.
- **"Attributed" in the datapoint concept** cannot hold of a derived value, which has no actor.
- **V1 could not be verified;** it now says what a minimal flow is.
- **§9 justified D202 by the hypothetical UC-10,** and said the existing record meets T3, which revision 2 had extended past it.
- **Two use-case mappings were wrong** (C6 to UC-1, T5 to UC-17), and two were missing (L4 to UC-12, L2 to UC-4). UC-8 now says what on-time is measured against, and UC-16 what "driven by" means.

**Revision 2, 2026-09-23**, after a review of revision 1 for validity and clarity. What changed, and why:

- **"Exceptions" had one reading, and the author's domain uses the other.** D2 read it only as the engine's refusals and overrides; the first consumer's own design means overdue, ageing and service-level breaches. Both are now in scope: D2 for the engine's sense, the new M7 for the business sense, and §5 defines both.
- **Four "Said" attributions overstated what was said.** F3, M1, V2 and D3 each carried an inference inside a Said row; each now says which part is interpretation. D10's porting clause was not said at all and moved to N5, marked Inferred.
- **L1 held two requirements with different evidence.** Rules reading recorded datapoints (L1) are grounded in the first consumer's inspection gate. Rules reading metrics across many objects (new L5) rest on UC-10, which is hypothetical, so L5 is a Should and UC-10 says so.
- **Four requirements contradicted the design proposed for them, or each other.** V4 let the engine propose changes, which ADR-0085 forbids; it now says a person or agent drafts and the engine supplies evidence. L4 allowed thresholds to change only by publishing, while a reorder point per model is data; both governed paths are now named. T3 kept every datapoint permanently, while refusal records are pruned (ADR-0083); T3 now says so. §10's first release omitted the capture it most needs, labels among it.
- **Two Musts depended on Shoulds.** V2 requires showing "unmodelled information recorded often", which needs D6; and M7's conditions across many objects, such as low stock per model, need C3's grouping. D6 and C3 are now Musts, and §6 states the rule that no Must depends on a Should.
- **Six requirements could not be verified as written.** "Easily" (D3) now has a measure; C5 says its target is unset and so it cannot yet be verified; F5 says which rules objects in flight follow; M3 and M4 say which metrics can be split; N3 drops "without precluding", which no test could fail.
- **Three things were missing.** Recording is authorised and validated (D12). Durations use UTC calendar time (C6), because business hours are a recorded limit (`design/edge-cases.md`). Formulas needing outside data are a non-goal.
- **T1, the objective, had no use case,** while every new capability — recording, backdating, trial rules — adds a route to test. UC-19 tests them.
- **Coverage was checked per group, which hid gaps.** Each requirement now lists its use cases in one place, and the six that have none say why.
- **Smaller.** F6 split into F6 (agents draft) and F7 (governed change). §5 gains Rule, Override and Exception, and separates derived datapoints (one object) from metrics (many). UC-17 and UC-18 are in order. §8 no longer counts the harness's failure conditions, which ADR-0083 changes. G5 no longer promises to explain decisions made outside the engine.

## 12. Proposed revisions

A design that finds a requirement cannot hold as written proposes its revision here, and meets the proposed wording until the author decides (ADR-0096 §3, ADR-0098 §6). None is awaiting the author. The eight proposed on 2026-09-23 and 2026-09-24 — G4, C2, UC-8, UC-10, D5, D11, L1 and N5 — were accepted in revision 7, and §11 records why each was proposed and why it was accepted.

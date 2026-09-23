# ObjectKeeper — Product requirements

**Status: draft, 2026-09-23, awaiting the author's review.** Written from the author's statements of 2026-09-23 and the standing objective of [`DESIGN.md`](DESIGN.md) §1. It states what the product must do. It does not choose designs: the design that follows it is evaluated against the use cases in §7, and recorded as decisions in [`adr/`](adr/).

**How to review it.** Every requirement in §6 names its source: **Said** is the author's own statement of 2026-09-23, **Carried** is a standing requirement of the existing record, and **Inferred** is this document's reading of what the other two imply. Inferred rows are where it is most likely to be wrong. §9 lists what the current design will have to change, and §10 has the only two questions this document asks the author.

## 1. Summary

ObjectKeeper is the foundational layer on which people and AI agents **define flows** and **collect data** about them, so that the business logic built on top can be **driven by data**, whichever kind of actor is driving it.

A flow is a set of object types with their lifecycles, transitions and rules. Data is everything the flow produces by running — every transition, every refusal, every override — together with what users choose to record, and values computed from both by formulas users define. Metrics over that data evaluate the flows continuously, and the evaluation is what lets a user start with an imperfect flow and **converge** on a good one, with the engine showing where the flow and reality disagree.

It keeps the objective the record has had from the start: **trust under delegation**. Nothing a person or an agent does can put the governed state into a condition that has to be cleaned up afterwards, except through an override that is declared, capability-gated and recorded.

## 2. The problem

**Rules are scattered, and nobody can see them.** In the first consumer, 454 sites refuse an operation; 176 are the same rule restated elsewhere, and one rule is written five times in four files ([`README.md`](../README.md)). What is allowed cannot be printed, reviewed or handed to an agent.

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
| G4 | Users define formulas that turn raw data into compound datapoints and metrics, declared once and read identically by every consumer |
| G5 | Business logic, whether executed by the engine's rules or by humans and agents deciding, can depend on that data, and every data-driven decision can be explained afterwards |
| G6 | The engine helps users converge: it shows where a flow and reality disagree, lets a change be trialled and measured, and keeps history intact across changes |
| G7 | Trust under delegation holds throughout (§1) |

**Non-goals**, each marked with its source:

| Non-goal | Source |
|---|---|
| Rendering a user interface; the engine supplies what a UI needs, not the UI | Carried |
| Causing external effects — sending mail, raising invoices, posting to Jira; consumers do that by observing events | Carried |
| Initiating work by itself — schedules, timers, reacting to its own events; a scheduler or agent above it asks and acts | Carried |
| Choosing among alternatives on a user's behalf — which engineer, which unit, which order first; the engine supplies the data and records the choice | Carried |
| Open-ended exploratory analysis — ad-hoc querying, charting, notebooks; the engine exports its data in an analysable shape for tools built for that | Inferred |
| High-rate machine telemetry — sensor streams at seconds rate; a summary of one may be recorded as a datapoint | Inferred, and §10 asks |

## 4. Who uses it

| Actor | Kind | What they do |
|---|---|---|
| Flow author | human | Defines and changes types, flows, rules, datapoint kinds and metrics |
| Flow author | agent | Proposes flow changes from evidence; may publish where granted |
| Operator | human | Requests transitions and records datapoints in the course of work — an engineer running an inspection, a coordinator committing an order |
| Operating agent | agent | The same, through the same interface and the same rules; today's first-consumer agents create deliveries, reserve units and attach service parts |
| Owner or reviewer | human | Reads the printed rules and the metrics, approves flow changes and overrides |
| Integration | service | Keeps an externally owned type in step with its owner (Xero), reflects state outward (Jira), runs schedules that ask what is due |

The engine treats human, agent and service alike except where a rule says otherwise, and records which kind acted every time.

## 5. Concepts

| Term | Meaning in this document |
|---|---|
| **Flow** | The declared lifecycle of one or more object types: states, transitions, the rules on them and the outcomes they apply |
| **Governed state** | An object's state and attributes, which only a declared transition or a recorded override may change |
| **Datapoint** | A recorded, attributed, time-stamped fact. Three kinds follow |
| **Flow-generated datapoint** | Produced by the engine as the flow runs: a transition, a refusal, an override, an admitted violation, a conflict, time spent in a state |
| **Recorded datapoint** | Recorded by a user about an object: an inspection result, a measurement, an observation, a label |
| **Derived datapoint** | Computed by a user-defined formula from other data |
| **Metric** | A derived value aggregated over many objects or over time, such as a median, a rate or a count per month |
| **Convergence** | Improving a flow over time from the evidence its own data provides, without losing history |

## 6. Requirements

Priorities: **Must** is required for the product to meet its purpose; **Should** is expected, and its absence needs a reason; **Could** is worth having if the design makes it cheap.

### 6.1 Flows

| ID | Requirement | Priority | Source |
|---|---|---|---|
| F1 | Users define object types, lifecycles, transitions and rules as a declaration that is inspectable at runtime and printable for review | Must | Said, Carried |
| F2 | Governed state changes only through a declared transition whose rules pass, or through a declared, capability-gated, recorded override | Must | Carried |
| F3 | Humans and agents act through one interface under one set of rules; the kind of actor matters only where a rule says so | Must | Said |
| F4 | A refusal says which rule refused, and whether the caller should supply something, ask someone, wait, work on another object first, or take a different path | Must | Carried |
| F5 | A flow changes by publishing a new version; objects in flight continue, and history stays readable under the rules that applied when it was written | Must | Carried |
| F6 | Agents may author flow changes. Changing a flow is itself governed: who may propose, who may publish, and an impact report before anything applies | Should | Inferred from "users (human or AI agents) define flows" |

### 6.2 Data capture

| ID | Requirement | Priority | Source |
|---|---|---|---|
| D1 | Every transition is captured with no effort from the flow's author: which object, which transition, from and to which state, who, which kind of actor, when, what caused it, and which version of the rules applied | Must | Said, Carried |
| D2 | Every exception the flow produces is captured the same way: refusals with the rule and remedy, overrides with their reason, admitted violations, conflicts between concurrent requests, and requests over a declared limit | Must | Said |
| D3 | Users record their own datapoints about an object easily — numbers with units, choices from a list, text, files, times — without modelling a new transition for each kind of fact | Must | Said |
| D4 | A recorded datapoint is attributed and never edited: a mistake is corrected by a newer datapoint linked to it, and both stay visible | Must | Inferred from the recorded property |
| D5 | A datapoint distinguishes when something happened from when it was recorded, within limits the declaration sets, because people record after the fact and durations computed from recording time are wrong | Should | Inferred |
| D6 | Users can record information before it has been modelled — a label or a note on an object — and the engine can count it and relate it to the flow, so that it can be formalised later | Should | Inferred from "users should not need to define everything perfectly" |
| D7 | Operators maintain catalogues of what to record — an inspection checklist per product configuration, say — as data, without a developer publishing a new declaration | Should | Inferred from the first consumer's inspection design |
| D8 | Personal data inside datapoints is erasable, like personal data anywhere else in the store | Must | Carried |
| D9 | Recorded datapoints cannot change governed state by being recorded; they influence a flow only through a rule that reads them (§6.5) | Must | Inferred from F2 and G7 |

### 6.3 Formulas and derived data

| ID | Requirement | Priority | Source |
|---|---|---|---|
| C1 | Users define compound datapoints with formulas over raw data: attributes, flow-generated datapoints and recorded datapoints | Must | Said |
| C2 | A formula is declared once, inspectable and versioned with the flow, and every consumer — a screen, an agent, a report, a rule — gets the same value from the same definition | Must | Inferred from the declared property |
| C3 | Formulas aggregate over time windows and group by dimensions such as model, supplier, month, kind of actor and flow version | Should | Inferred from the metrics the author named |
| C4 | A formula defined today applies to the history recorded before it existed | Should | Inferred |
| C5 | Reading a derived value is fast enough to use interactively at the first consumer's scale (§6.8) | Should | Inferred |

### 6.4 Metrics and evaluation

| ID | Requirement | Priority | Source |
|---|---|---|---|
| M1 | Every flow has a standard set of metrics without declaring any: time in each state, throughput, work in progress, age of the oldest open items, how often each transition is taken, refusal and override rates per rule, and rework — returning to an earlier state | Must | Said: "metrics to constantly evaluate the flows" |
| M2 | User-defined metrics are available through the same interface to people and agents, under the same visibility rules as the objects they are computed from | Must | Inferred |
| M3 | Any metric can be compared across versions of a flow, so the effect of a change can be measured | Should | Inferred |
| M4 | Any metric can be split by the kind of actor, so a flow run by agents can be compared with the same flow run by people | Should | Inferred |
| M5 | The engine exports its data in a shape suited to exploratory analysis elsewhere, and does not attempt the exploration itself | Could | Inferred from "potentially dig insights" |

### 6.5 Data-driven logic

| ID | Requirement | Priority | Source |
|---|---|---|---|
| L1 | A flow's rules can depend on data beyond the object's own attributes, including recorded and derived datapoints | Must | Said: "build data driven business logics" |
| L2 | Whenever a decision depends on data, the record shows which values it used, so the decision can be explained and reproduced afterwards | Must | Inferred from the recorded property |
| L3 | People and agents deciding outside the engine get the data they need through its interface, and the actions they take are recorded as any other | Must | Said, Carried |
| L4 | The thresholds and parameters of a data-driven rule are declared, visible, and changed only through the governed path of F6 | Should | Inferred |

### 6.6 Convergence

| ID | Requirement | Priority | Source |
|---|---|---|---|
| V1 | A flow can start minimal and be refined; the engine never needs the complete rules before it runs | Must | Said |
| V2 | The engine shows where a flow and reality disagree: transitions and states never used, rules that refuse most, states reached mostly by override, unmodelled information recorded often, states where work waits longest | Must | Said: "the engine helps them converge" |
| V3 | A new rule can be trialled before it is enforced: the refusals it would have made are recorded, and it is enforced when its author decides | Should | Inferred |
| V4 | The engine, or an agent reading its evidence, can propose a flow change with that evidence attached; applying it goes through F6. The engine never changes a flow by itself | Should | Inferred; the second sentence is Carried |
| V5 | Information recorded before it was modelled can be promoted into the model — a label becoming a state or a typed attribute — with its history intact | Should | Inferred |

### 6.7 Trust and governance

| ID | Requirement | Priority | Source |
|---|---|---|---|
| T1 | Nothing a person or an agent does puts governed state into a condition needing cleanup, except through a declared, capability-gated, recorded override | Must | Carried |
| T2 | The threat model is mistakes, not malice: actors are trusted and fallible, and one holding database credentials is out of scope | Must | Carried |
| T3 | Every change, datapoint and flow change is attributed to an actor and reconstructable | Must | Carried |
| T4 | Overrides and admitted violations are reviewable at any time, and a data import does not bury them | Must | Carried; the second half is `design/defects.md` D205 |
| T5 | Visibility rules apply to every read, including datapoints and metrics; a metric does not reveal what its reader could not see | Must | Carried, extended |

### 6.8 Non-functional

| ID | Requirement | Priority | Source |
|---|---|---|---|
| N1 | Runs on PostgreSQL in production and SQLite for development, tests and small deployments | Must | Carried |
| N2 | The core stays a library: correctness never depends on a background process | Must | Carried |
| N3 | The first release is sized for the first consumer — 279 live deliveries, 81 services, 203 warranty contracts and 123 live customers as of 2026-09-01 (inventory ADR-0003), which is operations rate, not telemetry rate — without precluding the orders-at-volume case study | Must | Carried, Inferred |
| N4 | Behaviour is deterministic under test, and the adversarial harness is the acceptance test | Must | Carried |
| N5 | The first consumer's production data and legacy history are ported and preserved | Must | Carried |

## 7. Use cases

Each use case is drawn from the first consumer where it can be, and each is a test the design must pass. The acceptance line is what "passes" means.

### Evaluating flows

**UC-1. Where do deliveries get stuck?** An operations lead asks which delivery states hold work longest, by customer and by product configuration, and which open deliveries are oldest in their current state. Nobody declared anything to make this possible. *Acceptance:* time in each state, current age and work in progress are available for every flow from the day it is published; for imported objects, from as far back as their ported history allows. *Exercises* D1, M1.

**UC-2. Which rule is in the way?** Deliveries are often refused at completion. The lead asks which rule refuses most, with which remedy, and whether the callers are people or agents — and learns that an agent keeps requesting completion before the checklist is done. *Acceptance:* every refusal is counted per rule, remedy and kind of actor, per week. *Exercises* D2, M1, M4.

**UC-3. Overrides as a symptom.** Units are often put into `AVAILABLE` by override rather than through `INTAKE`, because the label-and-photo rule does not fit one supplier's parts. *Acceptance:* override counts per target state and per reason, excluding the objects placed there by the data import, and each override reviewable. *Exercises* D2, T4, V2.

### Capturing data

**UC-4. Record a pre-delivery inspection.** An engineer inspects a configured robot against its configuration's checklist — each check pass, fail or not applicable, with a measured value where the check has one, a photo and a note — and delivery is gated on every check having a result. Operations staff maintain the checklists per configuration without a developer. The first consumer's operations design states this goal in these words: "who performed which predefined check, when, and with what result — and gate delivery on their completion" (§2). *Acceptance:* results recorded without a transition per check; the gate reads them; a checklist change needs no publish. *Exercises* D3, D4, D7, L1.

**UC-5. Keep what used to be thrown away.** When an order is committed, the availability check's result — what is in stock and what must be procured — is recorded, and the shortfall over time becomes a metric. This is the first consumer's first pain. *Acceptance:* the check's result is a datapoint on the order, and "open shortfall by model" is a declared metric. *Exercises* D3, C1, C3.

**UC-6. Recorded late.** An engineer records a shipment's receipt the morning after it arrived. *Acceptance:* time in `PROCUREMENT` uses when the receipt happened, within a bound the declaration sets, and both times are kept. *Exercises* D5, M1.

**UC-7. Where machine data stops.** A robot reports battery health every second. *Acceptance:* the design says where that data lives and what, if anything, is recorded here — a daily summary per unit, for instance. *Exercises* the telemetry non-goal, N3.

### Deriving and using data

**UC-8. A formula, declared once.** A quality lead defines inspection first-pass yield per robot model per month, as checks passed at the first attempt over checks inspected; a procurement lead defines supplier lead time, ordered to received, at the median and the eightieth percentile; operations defines the on-time delivery rate. *Acceptance:* each is declared once, and a screen, an agent and a report reading it get identical values. *Exercises* C1, C2, C3, M2.

**UC-9. A new metric over old data.** Supplier lead time is defined today, and the lead wants the last twelve months. *Acceptance:* the metric covers history recorded before it was defined, as far as the data exists. *Exercises* C4.

**UC-10. A rule that reads data.** Deliveries of a robot model whose inspection first-pass yield over the last thirty days is below a declared threshold need a second sign-off. *Acceptance:* the refusal names the value and the threshold; the record of every completion shows the value it was decided on; the threshold changes only by publishing. *Exercises* L1, L2, L4.

**UC-11. An agent decides with data.** A procurement agent ranks open purchase orders by risk of being late, from supplier lead-time metrics and each order's age, and escalates the riskiest. *Acceptance:* the agent reads the same metric definitions a person would, under the same visibility; its escalations are recorded as its actions; the ranking itself is the agent's, not the engine's. *Exercises* L3, M2.

**UC-12. Alerts from declared thresholds.** The first consumer's alert catalogue: low stock, procurement overdue, backorder ageing, delivery date at risk, lease overdue, warranty expiring, order ready (`wr:docs/proposals/operations-system-design.md` §6). *Acceptance:* each alert's condition is declared once and answerable by a query a scheduler or agent runs; sending the alert is the consumer's. *Exercises* C2, L3, N2.

### Converging

**UC-13. Start coarse, then refine.** Service jobs start as `OPEN → DONE`. Engineers label jobs "waiting for parts" as it happens. After a month the engine shows how many jobs carry the label, in which state it was applied, and how long those jobs waited. The team publishes a `WAITING_PARTS` state with its transitions, and moves the jobs in flight into it. *Acceptance:* the label is countable and relatable to states from the first day; the new version moves in-flight jobs by recorded transitions; time-to-done is comparable across the two versions. *Exercises* D6, V1, V2, V5, M3.

**UC-14. Trial a rule.** A proposed rule — a photo is required before a unit of one model becomes `AVAILABLE` — runs for two weeks without enforcing. *Acceptance:* the refusals it would have made are recorded and countable, including who would have been refused; enforcing it is a publish. *Exercises* V3, M1.

**UC-15. An agent proposes a change.** An agent reads UC-2's and UC-3's evidence and drafts a flow change. The publish dry run reports what it would affect; a person holding the authority approves it; it is published and attributed to both. *Acceptance:* no flow change applies without passing the governed path; the evidence and the approval are part of the record. *Exercises* F6, V4, T3.

**UC-16. People and agents on the same flow.** The lead compares cycle time, refusal rate and rework for deliveries driven by agents with those driven by people. *Acceptance:* every metric can be split by the kind of actor. *Exercises* M4.

### Governance

**UC-17. Erase a customer.** A customer asks to be erased. *Acceptance:* their personal values are removed from objects, history and datapoints; metrics built from those datapoints keep their counts and expose no personal value. *Exercises* D8, T5.

### Coverage

| Requirement group | Use cases |
|---|---|
| Flows, F | UC-10, UC-15 |
| Capture, D | UC-1 to UC-7, UC-13, UC-17 |
| Formulas, C | UC-5, UC-8, UC-9, UC-12 |
| Metrics, M | UC-1, UC-2, UC-8, UC-11, UC-13, UC-14, UC-16 |
| Logic, L | UC-4, UC-10, UC-11, UC-12 |
| Convergence, V | UC-3, UC-13, UC-14, UC-15 |
| Trust, T | UC-3, UC-15, UC-17 |

## 8. How we would know it works

Measures to take rather than targets to meet; none of the numbers below is a goal someone has set.

- The first consumer runs on the engine, and its 454 refusal sites and 51 duplicated rules become declared rules, each stated once.
- On cutover day every flow has the metrics of M1 with no instrumentation declared by anyone.
- UC-4, UC-5 and the seven alerts of UC-12 are expressed without a consumer computing its own copy of any metric definition.
- The time from a metric showing a problem to a published flow change, measured over the first months.
- The share of transitions requested by agents, and how their refusal and rework rates compare with people's.
- The adversarial harness passes: none of its eight failure conditions is reachable.

## 9. What this changes in the current design

The existing record meets F1 to F5, T1 to T3 and most of N1 to N5. These positions of the existing record conflict with the requirements above, and the design evaluation has to resolve each one.

| The record says | The requirement | Where |
|---|---|---|
| Analytics over the log is out of scope and belongs to a warehouse fed by a subscriber | M1 to M4 bring flow metrics in scope | [`design/edge-cases.md`](design/edge-cases.md), "Analytics over the whole log" |
| A refused request is rolled back and nothing is recorded | D2 records every refusal | `DESIGN.md` §5.4 and §6 |
| Every write goes through a declared transition | D3 and D6 need a lighter way to record | ADR-0042 |
| The store decides and records; consumers compute | C1 to C3 put user-defined formulas in the engine | ADR-0007 |
| The expression language has no grouped aggregation and no time windows | C3 | ADR-0047 |
| Beyond the declared indexes the store maintains no projection | M1 may need maintained flow data, such as time in state | `DESIGN.md` §10 |
| People write the declaration, and legibility of the printed rules is the defence against rules that say the wrong thing | F6 lets agents author flow changes | `DESIGN.md` §13 |
| Publishing refuses on any failed check | V1 needs a minimal flow to be small and publishable, without weakening the checks that protect T1 | `design/declaration-syntax.md` §10 |

Three findings of the design review of 2026-09-23 become prerequisites rather than defects to schedule: D205, since T4 and UC-3 fail while imported objects crowd the override list; D210, since any metric computed from the log needs a reliable settled position; and D202, since UC-10's second sign-off is an approval, and approvals do not work as stored.

## 10. Questions for the author

The author asked for the details to be settled by evaluating designs against the use cases, not by asking. These two are about the product rather than the design, and each has a default this document already assumes.

1. **Machine telemetry.** Assumed out of scope, with summaries recordable as datapoints (UC-7). If robots or other machines are to record directly at seconds rate, the storage design changes more than anything else in this document.
2. **Release order.** Assumed: the first release is the first consumer's port with F, D1 to D3, M1 and T in full; formulas, user-defined metrics and data-driven rules (C, M2 to M4, L) follow; convergence tooling and agent authorship (V, F6) come third. Each later phase depends on the data the earlier one records, which is why the flow-generated data comes first.

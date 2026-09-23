# The data-driven engine: design evaluation

Draft, 2026-09-23, written at the author's direction. The requirements and the nineteen use cases are [`../PRD.md`](../PRD.md)'s. This document owns the **evaluation**: for each design question, the options considered, how each fared against the use cases, and why the chosen one was chosen. The **decisions** are ADR-0081 to ADR-0086, which own them; where this document and an ADR disagree, the ADR wins.

**All six ADRs are Proposed, not accepted.** Accepting them rewrites about ten sections of [`../DESIGN.md`](../DESIGN.md) and four implementation documents (§8), and those edits should follow the author's reading of the direction rather than precede it. The two defaults of PRD §10 are taken as given: machine telemetry is out of scope, and the release order is the one the PRD proposes.

## 1. The shape

```
        humans · agents · services      one interface, one set of rules

   define                    act and record         read and decide
   draft, approve, publish   request                get, query, metric,
          │                         │               diagnostics
          ▼                         ▼                      ▲
┌──────────────────────────────────────────────────────────────────┐
│ declaration   types · flows · guards · observation kinds ·       │
│               metrics and their flags                (versioned) │
├────────────────────────────────┬─────────────────────────────────┤
│ governed state                 │ flow data the engine keeps       │
│  objects and attributes,       │  events: history, permanent      │
│  parts, including recorded     │  intervals: time in each state,  │
│  observations and labels       │    assignee and other value;     │
│                                │    an index the log rebuilds     │
│                                │  attempts: refusals and          │
│                                │    would-be refusals, prunable   │
├────────────────────────────────┴─────────────────────────────────┤
│ metrics: declared formulas, computed on read over both halves,   │
│ under the reader's visibility                                     │
└──────────────────────────────────────────────────────────────────┘
                   PostgreSQL / SQLite, one store
```

Three things are new. The engine **keeps data about its own flows** — time in each state and in each assignee's hands, and the requests it refused. User datapoints are **observations**, recorded as born-final parts of the object they describe. **Metrics** are a declared construct computed on read. One thing moves: a guard may read a metric, consulted before the transaction as an external evaluator is. Two things support convergence: a guard that **observes before it enforces**, and a flow change that is **drafted, approved and published through the store's own machinery**.

Nothing added writes governed state by a second path. That was the hardest constraint and the one that decided the most (§3.1).

## 2. Method

Nine questions. For each, the options are the obvious designs plus any the existing record already rejected for a reason that still applies. Each option is walked through the use cases the question touches, and it **fails** a use case when that use case's acceptance line in PRD §7 cannot be met. Ties are broken in this order: T1, the trust guarantee; then fewer new mechanisms, since the newest mechanism is the least-checked thing in a document (`LESSONS.md`); then fit with decisions already taken.

Two claims below were probed rather than reasoned, in keeping with the lesson about unprobed database claims: the size of the smallest flow the checker accepts (§3.6), and a percentile computed without a percentile function on SQLite 3.37.2 (§3.4).

## 3. The questions

### 3.1 How does a user record a datapoint?

Touches D3 to D7, D9, F2 and T1; UC-4, UC-5, UC-6, UC-17.

| Option | UC-4 inspection | UC-5 keep the check | UC-6 recorded late | UC-17 erasure | One write path (T1) | Easy (D3) |
|---|---|---|---|---|---|---|
| **A.** An attribute and an action per kind — the design as it stands | fails | partial | fails | passes | passes | fails |
| **B.** A `record` operation beside `request`, writing observation tables, no events | passes | passes | passes | passes if erasure is extended | **fails** | passes |
| **C.** An observation kind that expands to a born-final part, recorded by an ordinary creation request | passes | passes | passes | passes | passes | passes |
| **D.** Schemaless key–value datapoints on any object | partial | passes | passes | fails | partial | passes |

**A fails UC-4** because an inspection is one result per check per configured robot, the checklist is data that operations staff maintain, and neither fits attributes: an attribute holds one current value, and a set of anonymous structures is a limit the syntax document records. It fails UC-6 for want of an occurred time, and it fails D3 because every kind of fact costs an attribute, an action and a publish.

**B fails T1, and the record already says so.** A separate operation for writes that are not transitions is the `update` operation ADR-0042 rejected: "a second write path with its own authority model, parallel to the one that exists, and every consumer and every subscriber would have to handle both." Observations written that way would need their own idempotency, their own erasure, their own subscription feed and their own harness checks.

**D fails UC-17**, since erasure cannot know which keys hold personal values, and it lets a guard come to depend on a key nobody declared.

**Decided: C** (ADR-0082). A kind is declared once:

```
observation InspectionResult version 1 on ConfiguredRobot {
  field check   : InspectionCheck
  field outcome : CheckOutcome
  field value   : decimal(10,3)?
  field photo   : file?
  field note    : string?
  recorded by   actor.has(PDI_RECORD)
  occurred within 7 days
}
```

and the subject names the collection in one line, `part inspections : InspectionResult[] inverse subject`, which is what its guards read. The language expands the kind into what it is: a `tracking record` type with one terminal state, an `owner` end to its subject, and a generated `record` creation whose guard is the `recorded by` clause. Recording is a creation request, so an observation has an actor, an event, an idempotency key and a place in `history` like anything else, and nothing about requests, subscriptions, erasure or the harness changes.

- **It is a part because approvals already are.** ADR-0035's approval is a recorded part carrying who, a decision and the event that recorded it — an observation kind in all but name. Being a part also makes erasure reach it through the subject's `erase` (check 39), and makes it visible to `changed_since` once D202's repair records part positions per relationship, so a sign-off can name `inspections` and be invalidated by a later failing result.
- **Two part rules bend, and the reasons behind them hold.** Check 11 requires a part's creation to be `only via` its whole, so that nothing is added to a settled whole. An observation is recordable directly, and instead may not be recorded on a subject in a **terminal** state — which is what "settled" means, since a terminal state "says nothing further will ever be recorded" (`declaration-syntax.md` §4.2). And a born-final part cannot be driven by a cascade, so the language supplies `survives` for it (check 37).
- **Recording does not write the subject**, so it does not bump the subject's version and does not conflict with a concurrent transition on it. Where a flow needs a recorded fact to matter, a guard reads it (§3.5).
- **Recording is validated** (D12). An observation kind's fields are typed, and a kind may declare invariants over its own fields — a voltage within a range — checked when it is recorded like any local invariant. Who may record it is its `recorded by` guard.
- **Nothing is edited.** A correction is a new observation of the same kind and subject that names the one it corrects, and aggregates read the effective ones by default. Both stay in history (D4).
- **A transition can record what it decided on**, with an ordinary `create` step in its outcome. That is UC-5: order commitment records the availability check it just made.
- **Operator-maintained catalogues are objects.** The checklist is `InspectionCheck` objects per product configuration, created by an ordinary transition; the kind references one. Changing a checklist is data, not a publish (D7).
- **The cost is honest and small.** An observation is a directory row, a type row and an event. At an inspection rate of tens per robot that is nothing; at a sensor rate it is the wrong tool, which is one reason telemetry is not an observation (§3.7).

### 3.2 How is information recorded before anyone has modelled it?

Touches D6, D8, V2 and V5; UC-13.

| Option | UC-13 countable and relatable | Guards stay on declared data | Erasable |
|---|---|---|---|
| **A.** Free-text notes, as the first consumer has | fails | passes | fails |
| **B.** A built-in `label` observation on every type: a normalised name, an optional note, and the subject's state when it was applied | passes | passes, by rule | passes |
| **C.** Schemaless datapoints, §3.1 D | passes | fails | fails |

**Decided: B** (ADR-0082), with three rules.

- **Labels are read by metrics and diagnostics, never by guards, invariants or visibility.** A rule that depends on a vocabulary nobody declared changes meaning with a typo. A label becomes logic by being promoted — to a state, an observation kind or an attribute — and promotion is a publish.
- **A label's note is treated as personal.** Unclassified free text is where personal data hides, which ADR-0078 records as the limit erasure cannot reach; treating the note as personal means the subject's erasure redacts it.
- **Who may label** is a deployment capability, and a type may narrow it.

**Promotion moves objects by ordinary requests.** In UC-13 the new version declares `WAITING_PARTS`, and a person or an agent finds the jobs carrying the label and requests `wait_for_parts` on each. History then says who moved each job and when, rather than a migration claiming it. Labels are not copied into the new form; they stay as the record of the period before, and a metric spanning both versions counts both.

### 3.3 What flow data does the engine keep, and where?

Touches D1, D2, D5, M1 and T4; UC-1, UC-2, UC-3, UC-6, UC-14, UC-16.

**Refusals.**

| Option | UC-2 per-attempt detail | The log stays history | A refused creation has somewhere to go | Personal data |
|---|---|---|---|---|
| **A.** Refusals as events in the permanent log | passes | **fails** | fails | inputs kept for ever |
| **B.** A separate attempt log, outside history | passes | passes | passes | none kept |
| **C.** Counters per clause per day only | **fails** | passes | passes | none kept |

A fails because a fold of the log must reproduce the row (ADR-0033) and a refusal has nothing to fold; because `ok_event.object_id` has no object for a refused creation; and because a guessing agent's mistakes would become permanent history. C cannot show UC-2's pattern of one agent repeating the same refused request.

**Decided: B** (ADR-0083). One row per request that returned anything but *satisfied*, written in its own short transaction after the request's rolls back. It holds the time, the actor's id, kind and principal, the context, the type, the object if there is one, the transition, the verdict kind, the failing clause, its remedy class, whether it was unknown, and the declaration version. The two faults a caller causes, `UnknownTransition` and `KeyReused`, are recorded too. **Inputs are never recorded**, so the attempt log holds no personal value and erasure has nothing to do there. It is not history: `history` does not return it and subscriptions do not deliver it, and a deployment prunes it on a retention period, keeping monthly rollups. It is the second thing written outside the request's transaction, after the sequence mint (`storage-schema.md` §6). A crash between the rollback and the write loses one row of evidence and no history. The first consumer arrived at the same shape on its own: its audit writer uses a separate short session so that a failed operation still leaves a row (`wr:app/core/unified_audit.py:80-110`).

**Time in state.** Computing it on read from state-changing events passes UC-1, but makes current age and work in progress a window function over every event of a type. **Decided:** an **interval** table maintained in the transition's transaction — object, state, entered and left (position and occurred time), the entering transition, its actor's kind, and the declaration version (ADR-0083). It is an index over the log in the sense of ADR-0048 and ADR-0057: the log can rebuild it, so it is not a second source of truth, and the harness can check it the way it checks a row. `DESIGN.md` §10's "the store maintains no projection" becomes "no projection the log cannot rebuild". An import may supply each object's current-state entry time, and may supply earlier intervals from legacy history marked as legacy, which is UC-1's "as far back as ported history allows". §3.9 widens the same index from state to every enum attribute and singular reference, which is what makes time with each assignee a standard metric.

**When it happened.** With recorded time only, UC-6 fails. **Decided:** a transition may be marked `backdatable within <duration>`, and then accepts an occurred time inside that bound. The event keeps both times. An occurred time may not precede the object's previous state change, so no interval is negative. Guards still read `now` as the clock. Diagnostics report how often each transition is backdated, which is the check on the mistake it invites. Observations do the same with `occurred within`.

**Overrides without imports** (T4, UC-3). `state_source` gains `imported`, while the import's events keep the provenance `asserted` that ADR-0015 requires. `exceptions(type)` and every override metric read `asserted` only, and a migration mapping leaves an asserted object's `state_source` as it was. This is D205's resolution.

**Contention, measured.** Each event records how many serialisation retries its request took. D203 showed that a hot row at serialisable isolation waits and then retries; this makes the cost visible in production instead of argued.

### 3.4 Where are formulas and metrics defined, and how are they computed?

Touches C1 to C5, M1 to M5, D8 and T5; UC-5, UC-8, UC-9, UC-11, UC-12, UC-16, UC-17.

| Option | UC-8 one definition | UC-9 over old data | UC-11 agents, under visibility | UC-17 erasure | C5 speed at N3 |
|---|---|---|---|---|---|
| **A.** Per-object derived attributes only | fails | passes | passes | passes | passes |
| **B.** A declared `metric` form, computed on read | passes | passes | passes | passes | passes |
| **C.** User-written SQL views | passes | passes | **fails** | fails | passes |
| **D.** An external BI tool fed from the log, as today | **fails** | passes | fails | fails | passes |
| **E.** Metrics maintained incrementally at write time | passes | fails unless backfilled | passes | needs repair | passes |

A cannot express a cross-object aggregate per model per month. C cannot be held to the visibility predicate or the personal-data taint at publish, couples every consumer to a physical schema the storage design wants free to change, and differs in dialect across the two backends. D is the drift PRD G4 exists to end, and it leaves rules nothing to read. E fails UC-9 unless backfilled. Its stored aggregates would need repair after an erasure, and every recorded result updates one model-level row: a hot row by construction, with the cost D203 measured.

**Decided: A and B** (ADR-0084). Per-object derived attributes stay, and gain time over the object's own intervals — `entered_at(STATE)` and `time_in(STATE)`. Cross-object measurement is a new declared form:

```
metric supplier_lead_time version 1 {
  from  i in ProcurementOrder.intervals where i.state == ORDERED
  by    supplier = i.object.supplier, quarter = quarter(i.left_at)
  value percentile(0.8, i.duration)
  flag  slow when value > 21 days
}
```

The grammar is the syntax document's to write; what is decided here is the semantics.

- **Sources:** any type (its current objects), any observation kind, and three datasets per type — `intervals`, `transitions` (the events) and `attempts`.
- **Dimensions:** paths up to two hops, which is the reach the first consumer's guards were measured to need (`DESIGN.md` §5.7); the actor's kind; the declaration version; and time buckets over any timestamp.
- **Aggregates:** the existing set plus `avg`, `median` and `percentile`. Lead time is skewed, and a mean alone misleads. A value may combine aggregates arithmetically, as a rate does, and division by zero is unknown, as it already is.
- **Flags:** named conditions on a value, declared once, which a scheduler or an agent queries. That is UC-12's alert catalogue. The store never sends one.
- **Time** is UTC calendar time, and every metric says so (C6); working-hours calendars are out of scope for the first release, as `edge-cases.md` already records.
- **Windows** relative to `now` are allowed in a metric. No invariant may read a metric, for check 32's reason.
- **Computation:** each metric compiles to SQL over the store's tables and is evaluated on read, under the reader's visibility. Source rows are filtered by the source type's `visible when` predicate, which check 7 already requires to be expressible as a query filter; observations, intervals and attempts inherit their subject's. A metric is retroactive by construction. A cache may come later, keyed by visibility, if measurement asks for one.
- **Percentile on SQLite.** SQLite 3.37.2 has no percentile function. Nearest-rank computed with `ROW_NUMBER()` and `COUNT(*)` over a partition gave the right eightieth percentile on two test groups (ten values and three), and PostgreSQL has `percentile_cont`. Probed, not yet in a checker.
- **Personal data:** a personal attribute or field may not be a dimension, a flag's operand or an unaggregated output, and it may be counted. A metric then stores nothing personal, and erasure needs nothing from it (UC-17).
- **Standard metrics (M1)** are generated for every type from its datasets: time in each state, work in progress, throughput, the age of the oldest open objects, transition counts, refusals by clause, remedy and actor kind, overrides by target state and reason, and rework as re-entries per object. They are ordinary metrics the engine declares, listed with the type, so a person reading the rule set and an agent calling `metric()` see the same ones.

**ADR-0007's line moves.** It has been "arithmetic in, formulas out". It becomes "**the store's own data in, everything else out**": any declared formula over data the store holds is in, including aggregates across objects and time and scores built from them. A formula that needs data or rules the store does not hold, such as a tax table or a pricing engine, stays outside, as do selection, effects and orchestration.

### 3.5 How does a rule depend on data?

Touches L1 to L5 and T1; UC-4, UC-10, UC-12. L5, rules reading metrics, is a Should, and its one use case, UC-10, is hypothetical: nothing in the first consumer asks for it yet. The choice below is made so that the mechanism stays small if it is built, and it belongs to the second release.

Most data-driven rules need nothing new. UC-4's gate reads the configured robot's own inspections, an aggregate over a part, which guards do today. The question is UC-10: a guard over a metric, an aggregate across many objects over a window.

| Option | UC-10 value named and recorded | Concurrency | Sweepable | New pieces |
|---|---|---|---|---|
| **A.** The guard evaluates the metric inside the transaction | partial | **fails** | no | none |
| **B.** The metric is consulted before the transaction, like an evaluator, and its value and as-of time go on the event | passes | passes | no | small |
| **C.** A snapshot: a declared transition writes the metric's value to an attribute, and guards read that | passes | passes | yes | a refresher per rule |
| **D.** Guards never read metrics | fails L5 | — | — | — |

**A manufactures contention.** Under serialisable isolation the guard's read set is every inspection result of that model in the window, so recording any one of them conflicts with every delivery completion reading it: the wait-then-retry cost D203 measured, created on purpose.

**C works, but costs a refresher above the store** (ADR-0012) plus an attribute and an action for every rule. It is the right choice when the transition must be sweepable, or when a person should set the value.

**Decided: B, with C as the documented pattern for those two cases** (ADR-0084).

- A metric in a guard names its dimensions from the object and the inputs: `metric(inspection_pass_rate, model := this.robot_model, over last 30 days) >= 0.90`.
- **Arguments are resolved against committed state before the transaction, and resolved again inside it.** A mismatch means the world moved in between, and the request consults again within its serialisation retry bound. ADR-0049 does not say this for external evaluators, and it should for the same reason (D215).
- The value and its as-of time go on the event beside the guard's name, as an evaluator's verdict does, so UC-10's "the value it was decided on" is in the record.
- A metric guard may declare a freshness bound. `check` and `availability` compute internal metrics, unlike external evaluators, since no third party is involved.
- Thresholds are literals in the declaration, or attributes on objects changed by governed transitions, such as a reorder point per model. Either way they change only through a governed path (L4).

### 3.6 How does the engine help a flow converge?

Touches V1 to V5 and F6; UC-3, UC-13, UC-14, UC-15.

**Evidence.** A built-in `diagnostics(type)` read over the standard metrics, or leave it to agents? **Decided:** a thin built-in (ADR-0085), deterministic and fixed. It reports:

- transitions and states unused in a window;
- the clauses that refuse most, by remedy and actor kind;
- states entered mostly by override;
- labels by frequency and by the state they were applied in;
- the longest waits;
- rework loops;
- backdating rates;
- each observing guard's would-be refusals.

Nothing in it guesses at intent. Agents build proposals on top of it.

**Trialling a rule.**

| Option | UC-14 | Cost |
|---|---|---|
| **A.** An `observe` marking on a guard clause: evaluated, a would-be refusal recorded as an attempt linked to the applied event, the request proceeds | passes | one marking |
| **B.** Evaluate a whole candidate declaration in the shadow of live traffic | passes | every request evaluated twice, and candidate outcomes cannot be applied |
| **C.** Replay recorded requests against a candidate at publish | **fails** | attempts do not record inputs (§3.3), and recording them would put personal data in a table built to hold none |

**Decided: A** (ADR-0085). The printed rule set shows an observing clause as not enforced, the harness treats it as absent, and the publish report lists every one, so no guarantee is ever claimed for one. An observing capability guard is allowed and reported loudly, because it lets someone act whom it would refuse — which is exactly what trialling a capability requirement means.

**Changing a flow.**

| Option | UC-15 | F6 and F7 |
|---|---|---|
| **A.** People publish, through tooling only | fails | fails |
| **B.** Any actor holding a publish capability publishes | partial: no evidence attached, no second pair of eyes | partial |
| **C.** A built-in `DeclarationChange` object: drafted by any actor with the capability, carrying the source, the dry-run report and the evidence; approved by a *different* actor holding the publish capability; published, or superseded when the installed version moves under it | passes | passes |

**Decided: C** (ADR-0085), built on the store's own machinery as `Proposal` and `Subscription` are, so every step is attributed and the loop from evidence to change is itself measurable (PRD §8). The default approval rule: a change an agent drafted is approved by a human. The publish event then has an object to belong to, which resolves D212. The engine never drafts a change itself (ADR-0012); an agent does.

**Starting small.** Relax the publish checks for early versions, or keep them? A flow the checker accepts takes a six-line type body and two module lines — a tracking mode, two states with categories, a creation and one transition. Probed: `scripts/check-syntax-doc.py` reports it clean against the 26 checks it implements. The checks catch real defects and protect T1. What a new flow lacks is knowledge of its own rules, which observations, labels, observing guards and diagnostics supply. **Decided: keep the checks** (ADR-0085).

### 3.7 Where does the data live?

Touches N1 to N3; UC-7.

One store; a separate analytic store fed from the log; or one store plus an export. At N3's scale — hundreds of live objects per type, thousands of events a year — one store is ample. A second store would put a lag between governed state and the metrics a guard reads (§3.5), and add a third backend to test. **Decided: one store**, with M5's export: the log, the datasets and the observations, written in a file format suited to exploratory tools (ADR-0081). **Telemetry stays outside:** a service summarises it and records the summary as an observation, a daily battery-health reading per unit for instance (UC-7).

### 3.8 How do people and agents reach all this?

Touches M2, L3 and F3; UC-11.

**Through the one interface.** `metric(actor, name, filter?)` and `diagnostics(actor, type)` join the read surface, and recording is a creation request (ADR-0084, ADR-0085). The renderers give each observation kind a tool, and give metrics one tool that names them. Their governing rule extends unchanged: a tool description never restates a metric's definition. It names the metric and points at `metric()`, for the reason it never restates a guard (`renderers.md` §1).

### 3.9 How is assignment kept from the beginning?

Touches D10, D11, M6, L3 and N5; UC-18. Added the same day, when the author named assignee changes as data to keep from the beginning.

**The evidence.** A first-consumer service job has a required engineer-of-record, `engineer_id` (`wr:app/models/service.py:44`). A reassignment is validated like a creation and recorded in the audit log with before-and-after snapshots (`wr:app/services/service_service.py:357-372`). An agent may not be engineer-of-record (`wr:docs/agent-operations.md` §3). A user with active services must have them reassigned before being removed (`wr:app/models/users.py:34`).

**What "from the beginning" depends on.** Every singular reference has intervals from its first write, and the log can rebuild them (§3.3), so it does **not** depend on a marking being present from the start: an `assignee` marking added in a later version has metrics back to the first version's events. It depends on the assignee being **written through the store** from the start — a reference that transitions change — rather than living in Jira, in free text, or in someone's head. So the question is how to make assignment the obvious thing to model on the first day, and how to make its metrics exist without anyone declaring them.

| Option | UC-18 recorded from the first day | M6 metrics without declaring any | D11 other data of the kind | Fits ADR-0042, one role or several |
|---|---|---|---|---|
| **A.** An ordinary reference changed by actions, as today; metrics written per type by hand | passes | **fails** | fails | passes |
| **B.** A built-in assignee on every object, with a built-in `assign` operation | passes | passes | fails | **fails** |
| **C.** Intervals kept for every enum attribute and singular stored reference, not only state; and an `assignee` marking on a reference, naming it as a responsibility so assignment metrics are generated | passes | passes | passes | passes |

**A fails M6.** Every type names its assignee differently — `engineer`, `owner`, `handler` — so no metric or diagnostic can be written once for all of them, and the numbers exist only for the types someone thought to instrument.

**B fails on fit.**
- Not every type has an assignee: a warranty contract has none.
- Some have two, such as an engineer-of-record and a reviewer.
- The first consumer's is required at creation, not assigned later.
- A built-in `assign` operation writes a reference the type's author never declared a transition for, which is the second write path ADR-0042 refuses.

**Decided: C** (ADR-0086, with ADR-0083's intervals widened).

- **Value intervals.** The interval index covers state, every enum attribute and every singular stored reference: who, where, and which bucket. Absence is a value, so time unassigned is an interval like any other. Numbers and free text are not tracked. Their changes are in the events, but time-in-value over a price is not a question anyone asks, and a text field changes too freely to have meaningful intervals. Set-valued references, such as a team, are left open (§10).
- **The `assignee` marking.** `ref engineer : User assignee` goes on a singular reference to a type that marks one `identity` attribute as the actor's id, for example `attr login : identity actor`. A type may mark more than one reference, and each is its own dimension, named by the reference. The marking adds no write path. Assignment is whatever transitions write the reference — a creation that `accepts engineer`, a `reassign` action — each with its own guards on who may assign and who may be assigned. The first consumer's rule that an agent may not be engineer-of-record is one such guard.
- **Generated metrics (M6).** For each assignee dimension:
  - open work per assignee;
  - time unassigned;
  - time to first assignment;
  - time with each assignee;
  - handoffs per object, and reassignment back to an earlier assignee;
  - how often a transition is requested by someone other than the assignee — which the `actor` identity marking is what makes computable.

  Every one can be split by the kind of actor who made the assignment, so an agent balancing workload can be compared with a person doing it.
- **Choosing stays outside** (ADR-0007, ADR-0081). The engine guards and records the choice and supplies the workload data. Which engineer is chosen is a person's or an agent's decision (UC-18's agent proposes, a person reassigns).
- **Legacy reassignments are ported as intervals.** The first consumer's service updates record `engineer_id` in before-and-after snapshots, so the import mapping can rebuild past reassignments as legacy intervals. How far back that audit trail reaches is a sample to take against production, not a claim made here.
- **It is in the first release** (§9), because history can only start from the day it is written through the store.

## 4. The use cases, through the chosen design

| Use case | Met by | Result |
|---|---|---|
| UC-1 where deliveries get stuck | intervals and the standard metrics (§3.3, §3.4) | passes; imported history back to the entry times the mapping supplies |
| UC-2 which rule is in the way | the attempt log (§3.3) | passes |
| UC-3 overrides as a symptom | `imported` separated from `asserted` (§3.3) | passes |
| UC-4 record an inspection | an observation kind, a checklist as data, a guard over the part (§3.1) | passes |
| UC-5 keep the availability check | a `create` step in the commit's outcome; a shortfall metric (§3.1, §3.4) | passes |
| UC-6 recorded late | `backdatable within` (§3.3) | passes |
| UC-7 machine data | a service records a summary as an observation (§3.7) | passes, by the boundary |
| UC-8 a formula declared once | the `metric` form (§3.4) | passes |
| UC-9 a new metric over old data | computed on read (§3.4) | passes, back to where data exists |
| UC-10 a rule that reads data | a metric consulted before the transaction and recorded (§3.5) | passes |
| UC-11 an agent decides with data | `metric()` under visibility; the agent's actions are requests (§3.8) | passes |
| UC-12 alerts | flags on metrics, and per-object derived attributes queried by `query` (§3.4) | passes; sending is the consumer's |
| UC-13 start coarse, refine | labels, diagnostics, ordinary moves, metrics across versions (§3.2, §3.6) | passes |
| UC-14 trial a rule | the `observe` marking (§3.6) | passes |
| UC-15 an agent proposes a change | `DeclarationChange` (§3.6) | passes |
| UC-16 people and agents compared | the actor-kind dimension; "driven by" is the kind of actor that created the object, stated in the metric | passes |
| UC-17 erase a customer | no inputs in attempts, nothing stored in metrics, observations erased through the subject (§3.1, §3.3, §3.4) | passes |
| UC-18 who has what, and where handoffs hurt | value intervals and the `assignee` marking; legacy reassignments ported as intervals (§3.9) | passes; how far the legacy audit trail reaches is to be sampled |
| UC-19 the new paths do not open a way around the rules | an observation's `recorded by` guard and typed fields (§3.1); the backdating bound (§3.3); an observing clause shown and treated as not enforced (§3.6) | passes |

## 5. The conflicts of PRD §9, resolved

| The record says | Becomes | ADR |
|---|---|---|
| Analytics over the log is out of scope | Declared metrics are in; exploration is exported | ADR-0081, ADR-0084 |
| A refused request records nothing | Refusals go to the attempt log, which is not history | ADR-0083 |
| Every write goes through a declared transition | **Kept**: an observation is a creation | ADR-0082 |
| The store decides and records; consumers compute | The store's own data in; outside data, selection, effects and orchestration out | ADR-0081 |
| No grouped aggregation, no time windows | In metrics only; a guard reaches them through a metric reference | ADR-0084 |
| The store maintains no projection | No projection the log cannot rebuild | ADR-0083 |
| People write the declaration | Anyone with the capability drafts; a different actor with authority approves; an agent's draft needs a human | ADR-0085 |
| Publishing refuses on any failed check | **Kept** | ADR-0085 |

## 6. The open findings

- **D202** is required by §3.1, because an observation invalidates a sign-off by naming its relationship. ADR-0082 adopts its repair: part positions recorded per relationship in `ok_attribute_write`, which retires `last_part_event`.
- **D205** is resolved by §3.3's `imported` source, on acceptance.
- **D212** is resolved by §3.6's `DeclarationChange`, on acceptance.
- **D203** shaped §3.5, whose pre-transaction consultation avoids creating a contention point, and §3.3 records retries so that it can be measured.
- **D210.** PRD §9 called it a prerequisite for metrics, and that was wrong: a metric computed on read sees committed rows only, so work still in flight is invisible to it by construction. The settled position matters to a consumer advancing a cursor, which is the export of §3.7, and to nothing else here. The PRD line is corrected.
- **D204 and D208** are untouched. Observations do inherit erasure through their subject, and label notes are treated as personal, but erasure across supersession is still D208's question.
- **D215**, found while writing §3.5: ADR-0049 consults an evaluator before the transaction and never says what happens when the evaluator's argument depends on state the transaction changes.

## 7. What the choices cost, and what to measure

Seven new constructs: the observation kind, the label, the `metric` form with its flags, the `observe` marking, the `backdatable` marking, the `assignee` marking with its `actor` identity, and the `DeclarationChange` type. Each is where the next defect is most likely to be, so the syntax document should grow them in one iteration, with checker fixtures, before any is believed.

To measure rather than assume:

- metric latency at the first consumer's scale, on both backends;
- how far back the first consumer's audit trail records reassignments;
- percentile in a checker, rather than one probe;
- attempt-log volume under the harness's guessing actor;
- the cost of an observation as an object at the inspection rate;
- how often backdating is used, once it can be.

## 8. What acceptance would change

On acceptance, `DESIGN.md` §1, §3 (a fourth property: every flow produces data about itself, and users add their own), §5, §5.5, §5.7, §7, §10, §12, §13 and §14; `storage-schema.md` (attempts, intervals, observation tables, `ok_attribute_write` keyed per relationship); `library-api.md` (`metric`, `diagnostics`, the attempt and interval shapes); `renderers.md` (observation and metric tools); `declaration-syntax.md` (the seven constructs, check 11's exemption and new checks); the adversarial harness (intervals rebuilt from the log as a ninth failure condition); `edge-cases.md`'s analytics entry; and the back-links in ADR-0007, ADR-0027, ADR-0033, ADR-0047, ADR-0049, ADR-0057, ADR-0058 and ADR-0077.

## 9. Release order, mapped

| Phase | Decisions | Why this order |
|---|---|---|
| 1, the port | intervals over state and every tracked value, the **`assignee` marking** and its metrics, attempts, occurred time, the `imported` source, observation kinds, **labels**; legacy assignment and state history ported as intervals; the repairs of D202, D204 and D205 | Evidence for convergence can only start from the day it is recorded, so the things that record it come first, labels included, even though what uses them comes third |
| 2 | the `metric` form and flags, metric guards, `metric()` and the standard metrics | They read what phase 1 recorded |
| 3 | `observe`, `diagnostics`, `DeclarationChange` | They act on evidence phases 1 and 2 made visible |

## 10. Still open

- The grammar of the seven constructs, which belongs to the syntax document.
- Set-valued assignment, such as a team rather than a person: intervals of membership per element are the obvious extension, and nothing in the first consumer needs it yet.
- The attempt log's default retention: a number to measure, not to choose.
- Whether an observation kind ever needs a visibility predicate of its own rather than its subject's. It inherits until a case needs otherwise.
- Whether a change a human drafted also needs a second approver, or only one an agent drafted. The proposed default requires the second approver only for an agent's draft.

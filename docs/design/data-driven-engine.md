# The data-driven engine: design evaluation

Draft, 2026-09-23, written at the author's direction and aligned with **revision 3** of [`../PRD.md`](../PRD.md) — revision 4 (2026-09-24) added the engine's position as a governed core, N6 and UC-20, which change none of the evaluations below — whose requirements and nineteen use cases it evaluates designs against. This document owns the **evaluation**: for each design question, the options considered, how each fared against the use cases, and why the chosen one was chosen. The **decisions** are ADR-0081 to ADR-0086, which own them; where this document and an ADR disagree, the ADR wins. §11 says what changed when the PRD was revised.

**The author accepted all six ADRs on 2026-09-23**, and made the PRD the baseline every design choice is checked against. [`../DESIGN.md`](../DESIGN.md) and every implementation document §8 lists were amended the same day, and the design was then iterated until every PRD requirement was covered ([`traceability.md`](traceability.md)). The two defaults of PRD §10 are taken as given: machine telemetry is out of scope, and the release order is the one the PRD proposes (§9).

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

Three things are new. The engine **keeps data about its own flows**: time in each state and in each assignee's hands, and the requests it refused. User datapoints are **observations**, recorded as born-final parts of the object they describe. **Metrics** are a declared construct computed on read, and a business exception is a declared condition over them or over one object. Two things support convergence: a guard that **observes before it enforces**, and a flow change that is **drafted, approved and published through the store's own machinery**. One thing is offered with less evidence behind it than the rest: a guard may read a metric, consulted before the transaction as an external evaluator is (§3.5).

Nothing added writes governed state by a second path. That was the hardest constraint, and the one that decided the most (§3.1).

## 2. Method

Ten questions. For each, the options are the obvious designs plus any the existing record already rejected for a reason that still applies. Each option is walked through the use cases the question touches, and it **fails** a use case when that use case's acceptance line in PRD §7 cannot be met. Ties are broken in this order: T1, the trust guarantee; then fewer new mechanisms, since the newest mechanism is the least-checked thing in a document (`LESSONS.md`); then fit with decisions already taken.

Where the PRD marks a requirement as resting on a hypothetical use case (L5, UC-10) or as not yet verifiable (C5), the evaluation says so where it relies on it, and does not score options against it as though it had evidence.

Five of the PRD's fifty-two requirements are touched by no question here. F1, F4 and F5 are met by the existing design and no decision below changes them, T2 is an assumption rather than a behaviour, and N4, the adversarial harness, is how every decision below is eventually tested rather than something a decision could meet.

Two claims below were probed rather than reasoned, in keeping with the lesson about unprobed database claims: the smallest flow the checker accepts (§3.6), and a percentile computed without a percentile function on SQLite 3.37.2 (§3.4).

## 3. The questions

### 3.1 How does a user record a datapoint?

Touches D3, D4, D5, D7, D8, D9, D12, F2 and T1; UC-4, UC-5, UC-6, UC-17, UC-19.

| Option | UC-4 inspection | UC-5 keep the check | UC-6 recorded late | UC-17 erasure | Validated (D12) | One write path (T1) | Easy (D3) |
|---|---|---|---|---|---|---|---|
| **A.** An attribute and an action per kind — the design as it stands | fails | partial | fails | passes | passes | passes | fails |
| **B.** A `record` operation beside `request`, writing observation tables, no events | passes | passes | passes | passes if erasure is extended | passes if built again | **fails** | passes |
| **C.** An observation kind that expands to a born-final part, recorded by an ordinary creation request | passes | passes | passes | passes | passes | passes | passes |
| **D.** Schemaless key–value datapoints on any object | partial | passes | passes | fails | **fails** | partial | passes |

**A fails UC-4.** An inspection is one result per check per configured robot, the checklist is data operations staff maintain, and neither fits attributes: an attribute holds one current value, and a set of anonymous structures is a limit the syntax document records. A also fails UC-6 for want of an occurred time, and D3's measure, since every kind of fact costs an attribute, an action and a publish.

**B fails T1, and the record already says so.** A separate operation for writes that are not transitions is the `update` operation ADR-0042 rejected: "a second write path with its own authority model, parallel to the one that exists, and every consumer and every subscriber would have to handle both." Observations written that way would need their own authorisation, validation, idempotency, erasure, subscription feed and harness checks.

**D fails D12 and UC-17.** A value with no declared type cannot be validated, erasure cannot know which keys hold personal values, and a guard would come to depend on a key nobody declared.

**Decided: C** (ADR-0082). A kind is declared once:

```
observation InspectionResult version 1 on ConfiguredRobot {
  field check   : InspectionCheck
  field outcome : CheckOutcome
  field value   : decimal(10,3)? unit "V"
  field photo   : file?
  field note    : string?
  recorded by   actor.has(PDI_RECORD)
  occurred within 7 days
}
```

The subject names the collection in one line, `part inspections : InspectionResult[] inverse subject`, which is what its guards read. The language expands the kind into what it is:

- a `tracking record` type with one terminal state;
- an `owner` end to its subject;
- a generated `record` creation, whose guard is the `recorded by` clause.

Recording is a creation request, so an observation has an actor, an event, an idempotency key and a place in `history`, like anything else. Nothing about requests, subscriptions, erasure or the harness changes. That also meets D3's measure: one declaration, no code, and a single request per recording.

- **It is a part because approvals already are.** ADR-0035's approval is a recorded part carrying who, a decision and the event that recorded it — an observation kind in all but name. Being a part also makes erasure reach it through the subject's `erase` (check 39). And it makes it visible to `changed_since` once D202's repair records part positions per relationship, so a sign-off can name `inspections` and be invalidated by a later failing result.
- **Two part rules bend, and the reasons behind them hold.** Check 11 requires a part's creation to be `only via` its whole, so that nothing is added to a settled whole. An observation is recordable directly, and instead may not be recorded on a subject in a **terminal** state. That is what "settled" means, since a terminal state "says nothing further will ever be recorded" (`declaration-syntax.md` §4.2). And a born-final part cannot be driven by a cascade, so the language supplies `survives` for it (check 37).
- **Recording does not write the subject** (D9). It does not bump the subject's version and does not conflict with a concurrent transition on it. A recorded fact matters to the flow only through a guard that reads it (§3.5).
- **Recording is authorised and validated** (D12). Who may record a kind is its `recorded by` guard. Its fields are typed. A numeric field may state its unit, which the rule set prints and a read returns beside the value; units are not converted or checked in arithmetic. A kind may declare invariants over its own fields — a voltage within a range — checked when it is recorded, like any local invariant.
- **Nothing is edited** (D4). A correction is a new observation of the same kind and subject that names the one it corrects, and every read of the kind sees only the uncorrected ones (as made exact by ADR-0095 and ADR-0096). Both stay in history. Erasure is the one exception, and it removes only personal values (D8).
- **A transition can record what it decided on**, with an ordinary `create` step in its outcome. That is UC-5: order commitment records the availability check it just made.
- **Operator-maintained catalogues are objects** (D7). The checklist is `InspectionCheck` objects per product configuration, created by an ordinary transition, and the kind references one. Changing a checklist is data, not a publish.
- **The cost is honest and small.** An observation is a directory row, a type row and an event. At an inspection rate of tens per robot that is nothing. At a sensor rate it is the wrong tool, which is one reason telemetry is not an observation (§3.7).

### 3.2 How is information recorded before anyone has modelled it?

Touches D6, D8, V2 and V5; UC-13. D6 is a Must, because V2's evidence includes "unmodelled information recorded often".

| Option | UC-13 countable and relatable | Guards stay on declared data | Erasable |
|---|---|---|---|
| **A.** Free-text notes, as the first consumer has | fails | passes | fails |
| **B.** A built-in `label` observation on every type: a normalised name, an optional note, and the subject's state when it was applied | passes | passes, by rule | passes |
| **C.** Schemaless datapoints, §3.1 D | passes | fails | fails |

**Decided: B** (ADR-0082), with three rules.

- **Labels are read by metrics and diagnostics, never by guards, invariants or visibility.** A rule that depends on a vocabulary nobody declared changes meaning with a typo. A label becomes logic by being promoted — to a state, an observation kind or an attribute — and promotion is a publish.
- **A label's note is treated as personal.** Unclassified free text is where personal data hides, which ADR-0078 records as the limit erasure cannot reach. Treating the note as personal means the subject's erasure redacts it.
- **Who may label** is a deployment capability, and a type may narrow it.

**Promotion moves objects by ordinary requests** (V5). In UC-13 the new version declares `WAITING_PARTS`, and a person or an agent finds the jobs carrying the label and requests `wait_for_parts` on each. History then says who moved each job and when, rather than a migration claiming it. Labels are not copied into the new form; they stay as the record of the period before, and a metric spanning both versions counts both.

### 3.3 What flow data does the engine keep, and where?

Touches D1, D2, D5, M1, T3 and T4; UC-1, UC-2, UC-3, UC-6, UC-14, UC-16, UC-19. D2 is the engine's exceptions; business exceptions are §3.10.

**Refusals.**

| Option | UC-2 per-attempt detail | The log stays history | A refused creation has somewhere to go | Personal data |
|---|---|---|---|---|
| **A.** Refusals as events in the permanent log | passes | **fails** | fails | inputs kept for ever |
| **B.** A separate attempt log, outside history | passes | passes | passes | none kept |
| **C.** Counters per clause per day only | **fails** | passes | passes | none kept |

A fails for three reasons:
- a fold of the log must reproduce the row (ADR-0033), and a refusal has nothing to fold;
- `ok_event.object_id` has no object for a refused creation;
- a guessing agent's mistakes would become permanent history.

C cannot show UC-2's pattern of one agent repeating the same refused request.

**Decided: B** (ADR-0083). One row per request that returned anything but *satisfied*, written in its own short transaction after the request's rolls back. It holds the time; the actor's id, kind and principal; the context; the type, and the object if there is one; the transition; the verdict kind; the failing clause, its remedy class, and whether it was unknown; and the declaration version. The two faults a caller causes, `UnknownTransition` and `KeyReused`, are recorded too.

- **Inputs are never recorded**, so the attempt log holds no personal value and erasure has nothing to do there.
- **It is not history.** `history` does not return it, and subscriptions do not deliver it.
- **It is pruned.** A deployment keeps it for a retention period, with monthly rollups kept permanently, which is T3's clause for refusal records. *(ADR-0096 and ADR-0100: the rollups are daily, per object and per version, written by `maintain`'s prune in the same transaction, and read by the `<Type>.attempt_counts` source.)*
- **It is written outside the request's transaction**, the second thing that is after the sequence mint (`storage-schema.md` §6). A crash between the rollback and the write loses one row of evidence and no history.

The first consumer arrived at the same shape on its own: its audit writer uses a separate short session so that a failed operation still leaves a row (`wr:app/core/unified_audit.py:80-110`).

**Time in state.** Computing it on read from state-changing events passes UC-1, but makes current age and work in progress a window function over every event of a type. **Decided:** an **interval** table maintained in the transition's transaction (ADR-0083). Each row holds the object, the state, entry and exit (position and occurred time), the entering transition, its actor's kind, and the declaration version.

It is an index over the log in the sense of ADR-0048 and ADR-0057: the log can rebuild it, so it is not a second source of truth, and the harness can check it the way it checks a row. `DESIGN.md` §10's "the store maintains no projection" becomes "no projection the log cannot rebuild". An import may supply each object's current-state entry time, and earlier intervals from legacy history, marked as legacy. That is UC-1's "as far back as ported history allows". §3.9 widens the same index from state to every enum attribute and singular reference.

**When it happened** (D5). With recorded time only, UC-6 fails. **Decided:** a transition may be marked `backdatable within <duration>`, and then accepts an occurred time inside that bound.
- The event keeps both times.
- An occurred time may not precede the start of the current interval of any state or tracked value the request changes, nor be later than the time it is recorded, so no interval is negative (as amended by ADR-0095 and ADR-0096).
- A time past the bound is refused, which is UC-19's second route.
- Guards still read `now` as the clock.
- Diagnostics report how often each transition is backdated, which is the check on the mistake it invites.

Observations do the same with `occurred within`.

**Overrides without imports** (T4, UC-3). `state_source` gains `imported`, while the import's events keep the provenance `asserted` that ADR-0015 requires. `exceptions(type)` and every override metric read `asserted` only. A migration mapping leaves an asserted object's `state_source` as it was. This is D205's resolution.

**Contention, measured.** Each event records how many serialisation retries its request took. D203 showed that a hot row at serialisable isolation waits and then retries; this makes the cost visible in production instead of argued.

### 3.4 Where are formulas and metrics defined, and how are they computed?

Touches C1 to C6, M1 to M5, D8 and T5; UC-5, UC-8, UC-9, UC-11, UC-12, UC-16, UC-17.

| Option | UC-8 one definition | UC-9 over old data | UC-11 agents, under visibility | UC-17 erasure | Hot rows created |
|---|---|---|---|---|---|
| **A.** Per-object derived attributes only | fails | passes | passes | passes | none |
| **B.** A declared `metric` form, computed on read | passes | passes | passes | passes | none |
| **C.** User-written SQL views | passes | passes | **fails** | fails | none |
| **D.** An external BI tool fed from the log, as today | **fails** | passes | fails | fails | none |
| **E.** Metrics maintained incrementally at write time | passes | fails unless backfilled | passes | needs repair | **one per aggregate** |

C5, speed at the first consumer's scale, is not a column: the PRD leaves its target unset, so no option can be scored against it. E would be the fastest to read, and B is the one whose latency §7 measures.

**Why the others fail:**
- **A** cannot express a cross-object aggregate per model per month.
- **C** cannot be held to the visibility predicate or the personal-data taint at publish, couples every consumer to a physical schema the storage design wants free to change, and differs in dialect across the two backends.
- **D** is the drift PRD G4 exists to end, and it leaves rules nothing to read.
- **E** fails UC-9 unless backfilled, and its stored aggregates would need repair after an erasure. Every recorded result also updates one aggregate row: a hot row by construction, with the cost D203 measured.

**Decided: A and B** (ADR-0084). Per-object derived attributes stay — a derived datapoint for one object, in the PRD's terms — and gain time over the object's own intervals: `entered_at(STATE)` and `time_in(STATE)`. Measurement across many objects is a new declared form:

```
metric supplier_lead_time version 1 {
  from  i in ProcurementOrder.intervals where i.state == ORDERED
  by    supplier = i.object.supplier, quarter = quarter(i.left_at)
  value percentile(0.8, i.duration)
  flag  slow when value > 21 days
}
```

The grammar is the syntax document's to write. What is decided here is the semantics:

- **Sources:** any type (its current objects), any observation kind, and three datasets per type — `intervals`, `transitions` (the events) and `attempts`.
- **Dimensions:** paths up to two hops, which is the reach the first consumer's guards were measured to need (`DESIGN.md` §5.7); the kind of actor that requested a transition or recorded a datapoint (M4); the declaration version; and time buckets over any timestamp. A split by declaration version is meaningful where the metric's subject exists in both versions (M3).
- **Aggregates:** the existing set plus `avg`, `median` and `percentile`. Lead time is skewed, and a mean alone misleads. A value may combine aggregates arithmetically, as a rate does, and division by zero is unknown, as it already is.
- **Flags:** named conditions on a value, declared once, which a scheduler or an agent queries — a business exception across many objects (§3.10). The store never sends one.
- **Time** is UTC calendar time, and every metric says so (C6). Working-hours calendars are out of scope for the first release, as `edge-cases.md` already records.
- **Windows** relative to `now` are allowed in a metric. No invariant may read a metric, for check 32's reason.
- **Computation:** each metric compiles to SQL over the store's tables and is evaluated on read, under the reader's visibility. Source rows are filtered by the source type's `visible when` predicate, which check 7 already requires to be expressible as a query filter; observations, intervals and attempts inherit their subject's. A metric is therefore retroactive by construction (C4). A cache may come later, keyed by visibility, if measurement asks for one. *(Amended by ADR-0096: a path through an object the reader cannot see yields absence, and every result says whether it is complete for its reader, so a partial value is never taken for the metric's own, which C2 requires.)*
- **Percentile on SQLite.** SQLite 3.37.2 has no percentile function. Nearest-rank computed with `ROW_NUMBER()` and `COUNT(*)` over a partition gave the right eightieth percentile on two test groups (ten values and three), and PostgreSQL has `percentile_cont`. Probed, not yet in a checker.
- **Personal data:** a personal attribute or field may be counted, but may not be a dimension, a flag's operand or an unaggregated output. A metric then stores nothing personal, and erasure needs nothing from it (UC-17).
- **Standard metrics (M1)** are generated for every type from its datasets:
  - time in each state;
  - throughput;
  - work in progress;
  - the age of the oldest open objects;
  - how often each transition is taken;
  - refusal rates per rule, by remedy and actor kind;
  - override counts per target state and reason;
  - rework, as re-entries per object.

  A type with an assignee also gets §3.9's metrics. They are ordinary metrics the engine declares, listed with the type, so a person reading the rule set and an agent calling `metric()` see the same ones.

**ADR-0007's line moves.** It has been "arithmetic in, formulas out". It becomes "**the store's own data in, everything else out**". Any declared formula over data the store holds is in, including aggregates across objects and time and scores built from them. A formula that needs data or rules the store does not hold, such as a tax table or a pricing engine, stays outside, as do selection, effects and orchestration.

### 3.5 How does a rule depend on data?

Touches L1 to L5 and T1; UC-4, UC-10, UC-12.

Data-driven rules come in two kinds, and the PRD gives them different evidence.

**Rules reading recorded datapoints (L1, a Must, grounded in UC-4).** The inspection gate reads the configured robot's own inspections, an aggregate over a part, which guards do today. Nothing new is needed once observations are parts (§3.1).

L2 — the record holds the values a decision used — needs one small addition. The obvious shortcut is to derive the set the gate read from positions: every observation whose event precedes the completion's. That is wrong on PostgreSQL, where a position is allocated when a transition begins and is not commit order (D210). An inspection result allocated a lower position can commit after the completion's snapshot was taken, and the derived set would include a result the gate never saw. So a guard that aggregates over observations **records, on its event, the observations it read** — a list as long as the checklist — and re-evaluating the guard over that list gives the same answer. That is UC-4's "which results the gate read" (ADR-0082).

**Thresholds** (L4). A threshold is a literal in the declaration, or an attribute on an object changed by a governed transition, such as a reorder point per model (UC-12). Either way it changes only through a governed, recorded path.

**Rules reading metrics (L5, a Should).** Its only use case, UC-10, is hypothetical: nothing in the first consumer asks for it yet. The options are evaluated so that the mechanism stays small if it is built, and it belongs to the second release.

| Option | UC-10 value named and recorded | Concurrency | Sweepable | New pieces |
|---|---|---|---|---|
| **A.** The guard evaluates the metric inside the transaction | partial | **fails** | no | none |
| **B.** The metric is consulted before the transaction, like an evaluator, and its value and as-of time go on the event | passes | passes | no | small |
| **C.** A snapshot: a declared transition writes the metric's value to an attribute, and guards read that | passes | passes | yes | a refresher per rule |
| **D.** Guards never read metrics | fails L5 | — | — | — |

**A manufactures contention.** Under serialisable isolation the guard's read set is every inspection result of that model in the window, so recording any one of them conflicts with every delivery completion reading it: the wait-then-retry cost D203 measured, created on purpose.

**C works, but costs a refresher above the store** (ADR-0012), plus an attribute and an action for every rule. It is the right choice when the transition must be sweepable, or when a person should set the value.

**Decided: B, with C as the documented pattern for those two cases** (ADR-0084).

- A metric in a guard names its dimensions from the object and the inputs: `metric(inspection_pass_rate, model := this.robot_model, over last 30 days) >= 0.90`.
- **Arguments are resolved against committed state before the transaction, and resolved again inside it.** A mismatch means the world moved in between, and the request consults again within its serialisation retry bound. ADR-0049 does not say this for external evaluators, and it should, for the same reason (D215).
- **The value and its as-of time go on the event** beside the guard's name, as an evaluator's verdict does. A metric is computed outside the transaction, so unlike a recorded datapoint it cannot be re-read from the log at the decision's position. Recording it is how L2 holds for L5.
- A metric guard may declare a freshness bound. `check` and `availability` compute internal metrics, unlike external evaluators, since no third party is involved.

### 3.6 How does the engine help a flow converge?

Touches V1 to V5, F6, F7 and T1; UC-3, UC-13, UC-14, UC-15, UC-19.

**Evidence** (V2). A built-in `diagnostics(type)` read over the standard metrics, or leave it to agents? **Decided:** a thin built-in (ADR-0085), deterministic and fixed. It reports:

- transitions and states unused in a window;
- the clauses that refuse most, by remedy and actor kind;
- states entered mostly by override;
- labels by frequency, and by the state they were applied in;
- the longest waits;
- rework loops, and reassignment back to an earlier assignee;
- work done by someone other than the assignee;
- backdating rates;
- each observing guard's would-be refusals.

Nothing in it guesses at intent. Agents build their drafts on top of it (V4).

**Trialling a rule** (V3).

| Option | UC-14 | Cost |
|---|---|---|
| **A.** An `observe` marking on a guard clause: evaluated, a would-be refusal recorded as an attempt linked to the applied event, the request proceeds | passes | one marking |
| **B.** Evaluate a whole candidate declaration in the shadow of live traffic | passes | every request evaluated twice, and candidate outcomes cannot be applied |
| **C.** Replay recorded requests against a candidate at publish | **fails** | attempts do not record inputs (§3.3), and recording them would put personal data in a table built to hold none |

**Decided: A** (ADR-0085).
- The printed rule set shows an observing clause as not enforced.
- The harness treats it as absent.
- The publish report lists every one.

So no guarantee is ever claimed for one, which is T1's clause that a rule on trial guarantees nothing, and UC-19's third route. An observing capability guard is allowed and reported prominently: it lets someone act whom it would refuse, which is exactly what trialling a capability requirement means. Enforcing a clause is a flow change (F7) that removes the marking.

**Changing a flow** (F6, F7, V4).

| Option | UC-15 | F6 | F7 |
|---|---|---|---|
| **A.** People publish, through tooling only | fails | fails | fails |
| **B.** Any actor holding a publish capability publishes | partial: no evidence attached | passes | **fails**: the drafter publishes |
| **C.** A built-in `DeclarationChange` object: drafted by any actor with the capability, carrying the source, the dry-run report and the evidence; approved by a *different* actor holding the publish capability, a person when an agent drafted it; published, or superseded when the installed version moves under it | passes | passes | passes |

**Decided: C** (ADR-0085). It is built on the store's own machinery, as `Proposal` and `Subscription` are, so every step is attributed and the loop from evidence to change is itself measurable (PRD §8).
- F7's rules are its rules: the approver is never the drafter, and a change an agent drafted is approved by a person.
- The dry-run report is the impact report F7 asks for.
- The publish event then has an object to belong to, which resolves D212.
- The engine never drafts a change itself (ADR-0012, V4); an agent does.

**Starting small** (V1). Should the publish checks be relaxed for early versions, or kept? V1's measure is that a flow whose transitions carry no guards, and whose types declare no invariants, formulas or datapoint kinds, can be published and run. Probed: a six-line type body and two module lines — a tracking mode, two states with categories, a creation and one transition, none guarded — is reported clean by `scripts/check-syntax-doc.py` against the 26 checks it implements. The checks catch real defects and protect T1. What a new flow lacks is knowledge of its own rules, which observations, labels, observing guards and diagnostics supply. **Decided: keep the checks** (ADR-0085).

### 3.7 Where does the data live?

Touches N1 to N3 and M5; UC-7.

The options are one store; a separate analytic store fed from the log; or one store plus an export. At N3's scale — hundreds of live objects per type, and 6,391 audit rows over the first consumer's life — one store is ample. A second store would put a lag between governed state and the metrics a guard reads (§3.5), and add a third backend to test.

**Decided: one store**, with M5's export of the log, the datasets and the observations, in a file format suited to exploratory tools (ADR-0081). **Telemetry stays outside:** a service summarises it and records the summary as an observation, a daily battery-health reading per unit for instance (UC-7).

### 3.8 How do people and agents reach all this?

Touches M2, L3 and F3; UC-11.

**Through the one interface.** `metric(actor, name, filter?)` and `diagnostics(actor, type)` join the read surface, and recording is a creation request (ADR-0084, ADR-0085). F3's "discover what they may do now" is the existing `availability`. The renderers give each observation kind a tool, and give metrics one tool that names them. Their governing rule extends unchanged: a tool description never restates a metric's definition. It names the metric and points at `metric()`, for the reason it never restates a guard (`renderers.md` §1).

### 3.9 How is assignment kept from the beginning?

Touches D10, D11, M6, L3 and N5; UC-18.

**The evidence.**
- A first-consumer service job has a required engineer-of-record, `engineer_id` (`wr:app/models/service.py:44`).
- A reassignment is validated like a creation, and recorded in the audit log with before-and-after snapshots (`wr:app/services/service_service.py:357-372`).
- An agent may not be engineer-of-record (`wr:docs/agent-operations.md` §3).
- A user with active services must have them reassigned before being removed (`wr:app/models/users.py:34`).

**What "from the beginning" depends on.** Every singular reference has intervals from its first write, and the log can rebuild them (§3.3). So it does **not** depend on a marking being present from the start: an `assignee` marking added in a later version has metrics back to the first version's events. It depends on the assignee being **written through the store** from the start — a reference that transitions change — rather than living in Jira, in free text, or in someone's head. The question is how to make assignment the obvious thing to model on the first day, and how to make its metrics exist without anyone declaring them.

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

- **Value intervals.** The interval index covers state, every enum attribute and every singular stored reference: who, where, and which bucket. Absence is a value, so time unassigned is an interval like any other. Numbers and free text are not tracked: their changes are in the events, but time-in-value over a price is not a question anyone asks, and a text field changes too freely to have meaningful intervals. Set-valued references, such as a team, are left open, as D11 now says (§10).
- **The `assignee` marking.** `ref engineer : User assignee` goes on a singular reference to a type that marks one `identity` attribute as the actor's id, for example `attr login : identity actor`. A type may mark more than one reference, and each is its own dimension, named by the reference. The marking adds no write path. Assignment is whatever transitions write the reference — a creation that `accepts engineer`, a `reassign` action — each with its own guards on who may assign and who may be assigned. The first consumer's rule that an agent may not be engineer-of-record is one such guard.
- **Generated metrics (M6).** For each assignee dimension:
  - open work per assignee;
  - time unassigned;
  - time to first assignment;
  - time with each assignee;
  - handoffs per object, and reassignment back to an earlier assignee;
  - how often a transition is requested by someone other than the assignee, which is what the `actor` identity marking makes computable.

  Every one can be split by the kind of actor who made the assignment, so an agent balancing workload can be compared with a person doing it.
- **Choosing stays outside** (ADR-0007, ADR-0081). The engine guards and records the choice and supplies the workload data. Which engineer is chosen is a person's or an agent's decision (UC-18's agent proposes, a person reassigns).
- **Legacy reassignments are ported as intervals** (N5). The first consumer's service updates record `engineer_id` in before-and-after snapshots, so the import mapping can rebuild past reassignments as legacy intervals. How far back that audit trail reaches is a sample to take against production, not a claim made here.
- **It is in the first release** (§9), because history can only start from the day it is written through the store.

### 3.10 How is a business exception declared and found?

Touches M7, C2, C3, L4 and N2; UC-12. The PRD reads the author's word "exceptions" in two senses (PRD §5). This question is the business sense: work deviating from what the flow expects. The engine's own exceptions are §3.3.

**The evidence.** The first consumer's alert catalogue (`wr:docs/proposals/operations-system-design.md` §6) holds six conditions and one notice:

| Condition | Grain | What it reads |
|---|---|---|
| Procurement overdue | one purchase order | its expected arrival against `now` |
| Backorder ageing | one slot | how long its reference to a unit has been empty — the entry time of the interval in which that reference is absent (§3.3, §3.9) |
| Delivery date at risk | one delivery | its delivery date against `now` |
| Lease overdue | one lease | its expected return against `now` |
| Warranty expiring | one contract | its end date against `now` |
| Low stock | one robot model, across its units | its available-to-promise, counted over the model's units, against the model's reorder point |
| Order ready | — | an event, the last slot filled; not a condition |

| Option | Declared once (M7) | Engine initiates nothing (N2) | New constructs |
|---|---|---|---|
| **A.** Consumer code computes alerts, as the first consumer's greenfield automation layer would | **fails** | passes | none |
| **B.** An `alert` construct in the engine, with schedules and channels | passes | **fails**: it schedules and sends | one, duplicating flags and derived attributes |
| **C.** No new construct: a per-object condition is a derived attribute over the object and `now`; a condition across many objects is a metric flag; a notice is a subscription | passes | passes | none |

**Decided: C**, using what ADR-0084 and the existing design already provide.

- **Five of the six are per-object and queryable.** Each is a derived attribute over a stored operand and `now`, found by `query` filtering that operand, as ADR-0048 requires for anything that reads a clock. None needs the metric form, so all five are in the first release.
- **Backorder ageing stretches ADR-0048 slightly.** A slot's state is derived in the first consumer, not a lifecycle state (`TODO.md`, first-consumer challenge 3), so its age is how long the slot's reference to a unit has been empty: `unit is null and entered_at(unit) + 14 days <= now`, with `entered_at` over a tracked member (ADR-0084). Its operand is stored, but in the interval index rather than the slot's own row, so `query` must be allowed to filter on the entry time of a tracked value's current interval. ADR-0084 states that extension.
- **Low stock is across many objects.** In the second release it is a metric flag per model (§3.4). In the first, it is a per-model derived attribute counting the model's units, which is per-object in form but reads other objects. So it cannot be indexed, and it is answered by reading each model rather than by one query. The flag replaces it when the metric form arrives.
- **Order ready is a subscription** to the transition that fills the last slot (ADR-0034). It is an event, not a deviation.
- **Thresholds are governed** (L4). A reorder point is an attribute of the model, changed by a transition; a fixed window is a literal in the declaration.
- **Sending is the consumer's.** A scheduler or an agent runs the queries and routes the alerts to Jira or Slack, as the first consumer's design already intends.

## 4. The use cases, through the chosen design

| Use case | Met by | Result |
|---|---|---|
| UC-1 where deliveries get stuck | intervals and the standard metrics (§3.3, §3.4) | passes; imported history back to the entry times the mapping supplies |
| UC-2 which rule is in the way | the attempt log (§3.3) | passes |
| UC-3 overrides as a symptom | `imported` kept apart from `asserted` (§3.3) | passes |
| UC-4 record an inspection | an observation kind, a checklist as data, a guard over the part; the observations it read recorded on the completion's event (§3.1, §3.5) | passes |
| UC-5 keep the availability check | a `create` step in the commit's outcome; a shortfall metric (§3.1, §3.4) | passes |
| UC-6 recorded late | `backdatable within` (§3.3) | passes |
| UC-7 machine data | a service records a summary as an observation (§3.7) | passes, by the boundary |
| UC-8 a formula declared once | the `metric` form, in UTC calendar time (§3.4) | passes |
| UC-9 a new metric over old data | computed on read (§3.4) | passes, back to where data exists |
| UC-10 a rule that reads a metric | a metric consulted before the transaction, its value recorded (§3.5) | passes; hypothetical, and in the second release |
| UC-11 an agent decides with data | `metric()` under visibility; the agent's actions are requests (§3.8) | passes |
| UC-12 business exceptions | five per-object derived attributes found by `query`, and one metric flag; order ready as a subscription (§3.10) | passes; low stock is read model by model until the second release |
| UC-13 start coarse, then refine | labels, diagnostics, ordinary moves, metrics across versions (§3.2, §3.6) | passes |
| UC-14 trial a rule | the `observe` marking; enforcing it is a `DeclarationChange` (§3.6) | passes |
| UC-15 an agent drafts a change | `DeclarationChange`, approved by a person (§3.6) | passes |
| UC-16 people and agents on the same flow | the actor-kind dimension, over the kind of actor whose request created each object (§3.4) | passes |
| UC-17 erase a customer | no inputs in attempts, nothing stored in metrics, observations erased through the subject (§3.1, §3.3, §3.4) | passes |
| UC-18 who has what, and where handoffs hurt | value intervals and the `assignee` marking; legacy reassignments ported as intervals (§3.9) | passes; how far the legacy audit trail reaches is to be sampled |
| UC-19 the new paths do not open a way around the rules | the `recorded by` guard and typed fields (§3.1); the backdating bound (§3.3); an observing clause shown and treated as not enforced (§3.6) | passes |

## 5. The conflicts of PRD §9, resolved

| The record says | Becomes | ADR |
|---|---|---|
| Analytics over the log is out of scope | Declared metrics are in; exploration is exported | ADR-0081, ADR-0084 |
| A refused request records nothing | Refusals go to the attempt log, which is not history | ADR-0083 |
| Every write goes through a declared transition | **Kept**: an observation is a creation | ADR-0082 |
| The store decides and records; consumers compute | The store's own data in; outside data, selection, effects and orchestration out | ADR-0081 |
| No grouped aggregation, no time windows | In metrics only; a guard reaches them through a metric reference | ADR-0084 |
| The store maintains no projection | No projection the log cannot rebuild | ADR-0083 |
| People write the declaration | Anyone with the capability drafts; a different actor approves, and a person when an agent drafted | ADR-0085 |
| Publishing refuses on any failed check | **Kept** | ADR-0085 |

## 6. The open findings

- **D202** is required by §3.1, because an observation invalidates a sign-off by naming its relationship. ADR-0082 adopts its repair: part positions recorded per relationship in `ok_attribute_write`, which retires `last_part_event`.
- **D205** is resolved by §3.3's `imported` source, on acceptance.
- **D212** is resolved by §3.6's `DeclarationChange`, on acceptance.
- **D203** shaped §3.5, whose pre-transaction consultation avoids creating a contention point, and §3.3 records retries so that it can be measured.
- **D210.** PRD §9 once called it a prerequisite for metrics, and that was wrong. A metric computed on read sees committed rows only, so work still in flight is invisible to it by construction. The settled position matters to a consumer advancing a cursor, which is the export of §3.7. It would also have mattered to §3.5 had the set a gate read been derived from positions, and that near miss is why §3.5 records the set instead. The PRD line is corrected.
- **D204 and D208** are untouched. Observations do inherit erasure through their subject, and label notes are treated as personal, but erasure across supersession is still D208's question.
- **D215**, found while writing §3.5: ADR-0049 consults an evaluator before the transaction and never says what happens when the evaluator's argument depends on state the transaction changes.

## 7. What the choices cost, and what to measure

Seven new constructs:
- the observation kind, with its `unit` and `occurred within`;
- the label;
- the `metric` form with its flags;
- the `observe` marking;
- the `backdatable` marking;
- the `assignee` marking with its `actor` identity;
- the `DeclarationChange` type.

§3.10 adds none. Each is where the next defect is most likely to be, so the syntax document should grow them in one iteration, with checker fixtures, before any is believed.

To measure rather than assume:

- metric latency at the first consumer's scale, on both backends, which is what sets C5's target. An indicative first measurement, `scripts/probe-metric-latency.py` on 2026-09-23: SQLite 3.37.2 in memory, synthetic data, 97,142 state intervals over 20,000 objects (roughly thirty times the first consumer's live objects), no visibility filter. Median of seven runs: 284 ms for an 80th-percentile time in state by state and month, 36 ms for work in progress with the oldest open, 16 ms for weekly throughput by actor kind. Disk, PostgreSQL and visibility are unmeasured, so this bounds nothing; it says the percentile is the metric to watch;
- the first consumer's event and datapoint rates before cutover (N3);
- how far back its audit trail records reassignments;
- percentile in a checker, rather than one probe;
- attempt-log volume under the harness's guessing actor;
- the cost of an observation as an object at the inspection rate;
- how often backdating is used, once it can be.

## 8. What acceptance changes

Accepted 2026-09-23, and every change below was made the same day. The syntax took iteration 19, which also recorded the six decisions spelling required as ADR-0095. Mapping the design against the PRD then found and repaired more (ADR-0087 to ADR-0095, [`traceability.md`](traceability.md)). What acceptance changed:
- **`DESIGN.md`:** §1, §3 (a fourth property: every flow produces data about itself, and users add their own), §5, §5.5, §5.7, §7, §10, §12, §13 and §14.
- **`storage-schema.md`:** attempts, intervals, observation tables, and `ok_attribute_write` keyed per relationship.
- **`library-api.md`:** `metric`, `diagnostics`, and the attempt and interval shapes.
- **`renderers.md`:** observation and metric tools.
- **`declaration-syntax.md`:** the seven constructs, check 11's exemption, and new checks.
- **The adversarial harness:** intervals rebuilt from the log as a failure condition, and observing clauses excluded from the guarantee.
- **`edge-cases.md`:** the analytics entry.
- **Back-links:** in ADR-0007, ADR-0027, ADR-0033, ADR-0047, ADR-0049, ADR-0057, ADR-0058 and ADR-0077.

## 9. Release order, mapped

The PRD's three releases (§10), with what each needs from the decisions above.

| Release | PRD scope | Decisions it needs | Why this order |
|---|---|---|---|
| 1, the port | F1 to F5; all of D; M1 and M6, readable by people and agents; derived datapoints for one object and the per-object conditions of M7; L1, L3, V1, T and N | observation kinds with `unit` and `occurred within`; **labels**; the attempt log; intervals over state and every tracked value; `backdatable`; the `imported` source; the **`assignee` marking** and its metrics; the standard metrics and `metric()` to read them; `entered_at` and `time_in`, and `query` filtering on an interval's entry time; legacy state and assignment history ported as intervals; the repairs of D202, D204 and D205 | Evidence for convergence can only start from the day it is recorded, so everything that records it comes first — labels and assignment included — even though what uses it comes later |
| 2 | the rest of C; M2 to M5 for user-defined metrics; M7 across many objects; L2, L4 and L5 | the user-defined `metric` form and its flags; metric guards and their recorded values | They read what release 1 recorded |
| 3 | V2 to V5, F6, F7 | `observe`, `diagnostics`, `DeclarationChange`, and promotion by ordinary requests | They act on the evidence releases 1 and 2 made visible |

## 10. Still open

- The grammar of the seven constructs, which belongs to the syntax document.
- Set-valued assignment, such as a team rather than a person. D11 now says it is not covered. Intervals of membership per element are the obvious extension, and nothing in the first consumer needs it yet.
- The attempt log's default retention: a number to measure, not to choose.
- C5's latency target, which §7's measurement sets.
- Whether an observation kind ever needs a visibility predicate of its own rather than its subject's. It inherits until a case needs otherwise.

## 11. Changes for PRD revision 3

Rewritten whole rather than patched, since the evaluation had absorbed four rounds of edits in one day. What changed against the version aligned with PRD revision 1:

- **§3.10 is new.** The PRD now reads "exceptions" in the business sense too (M7), and nothing had evaluated how an overdue order or an ageing backorder is declared and found. Five of the first consumer's six conditions turn out to be per-object and queryable in the first release. Low stock is read model by model until the metric form arrives, and order ready is an event. Backorder ageing needs `query` to filter on an interval's entry time, which ADR-0084 now states rather than assuming ADR-0048 already allowed it.
- **§3.5 separates the two data-driven rules.** L1, grounded in UC-4, needs nothing new to decide. To meet L2, a guard over observations records which ones it read: deriving the set from positions was the first draft of this revision, and it is wrong wherever a position is not commit order (D210). L5, on a hypothetical use case, keeps its mechanism but is marked as resting on less evidence, and moves to the second release.
- **§3.6 applies F7 as a rule, not a default.** Every change needs an approver other than its drafter, and a person for an agent's draft. That settles the open question §10 used to carry. Option B now fails F7 outright. The diagnostics list gains assignment. The minimal-flow probe is stated against V1's new measure.
- **§3.1 scores D12**, which option D now fails, and names D4's erasure exception and D3's measure.
- **§3.4 stops scoring C5**, which the PRD leaves unverifiable, and scores hot rows instead. It adds C6 and states where M3's split by version is meaningful.
- **§9 follows the PRD's releases.** The standard metrics and `metric()` move to the first release, with the per-object conditions and the time functions they need. The user-defined `metric` form and metric guards stay in the second.
- **Every "Touches" line** now names the requirements revision 3 added or changed: D12, C6, F7, T3, M7. The use-case table uses revision 3's names and acceptance lines for UC-4, UC-8, UC-10, UC-12, UC-15 and UC-16.

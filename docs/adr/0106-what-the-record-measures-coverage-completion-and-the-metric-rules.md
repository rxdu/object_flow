# ADR-0106: What the record measures: history coverage, completion, and the metric rules revision 5 exposed

- **Status:** Accepted — decided 2026-09-24 at the author's direction ("go ahead with the five reviewers once the PRD is fixed"), against `docs/PRD.md` revision 5: C1, C2, D5, D8, D11, M1, M3, M4, M5, M6, N1, N5, T5, V3 and UC-1, UC-9, UC-14, UC-16, UC-17, UC-18; repairs D299 to D314, and, as corrected by an independent verification, D356, D360, D362 and D366. The author's own acceptance is pending.
- **Date:** 2026-09-24
- **Refines:** ADR-0026, ADR-0048, ADR-0083, ADR-0084, ADR-0086, ADR-0096, ADR-0098, ADR-0100, ADR-0101
- **PRD:** the four revisions it proposes (D5, D11, L1, N5) were accepted by the author on 2026-09-24, as PRD revision 7.

## Context

The same five-slice review as ADR-0105, read against PRD revision 5, found that several things the record measures were defined loosely enough to give wrong numbers.

1. **Gaps in history.** Four documents promise that a metric covering a gap in legacy history "counts it as unknown time and reports as incomplete":
   - `DESIGN.md` §11;
   - `publish-and-import.md` §4;
   - `first-consumer-cutover.md` §4;
   - ADR-0100.

   `complete`, as `DESIGN.md` §5.12, the syntax §6.9 and `MetricPage` define it, is about visibility alone. A gap is an absent row, and a reader who sees everything was told the result was complete over the first consumer's five-month audit hole. Three reviewers found this independently.
2. **Creation time.** `.created_at` was "taken from its creation event" without saying which of its two times. Standard metrics subtract it from occurred times.
3. **Splits at the port.** A port split every object's current stay in two: a legacy interval ending at the import, and a recorded one starting there.
   - `entered_at(state)` then returned the import time for every ported object.
   - `oldest_open` restarted at cutover.
   - `handoffs_by_object` and `returns_by_object`, which count intervals, gave every ported job an extra handoff.
   - A mirror's refresh had no rule at all, so a reassignment made in the legacy system while a type was a mirror was dated at the refresh and attributed to the importer.
4. **New tracked members.** A tracked member added by a later publish had no interval for any live object, and nothing wrote one.
5. **Derived values across objects.** A derived attribute that aggregates over other objects was specified under visibility only when a metric reads it. `get` and `query` said nothing, so a stock count either counted hidden objects (T5) or varied by reader without saying so (C2).
6. **Override reasons.** Override counts "per reason" (M1, revision 5) existed only where an assertion's reason happened to be an enum. Check 29 required only that a reason exist.
7. **Personal values in metrics.** A personal value could be read by a metric's filter. Erasure then changed a count, which UC-17 says it must not.
8. **Splits by actor kind and version.** Only the standard metrics could be split by kind of actor and by version (M3, M4, UC-16). A combined metric a deployment declares has no binder a reader's filter could use. A legacy row whose record names no actor took the importer's kind, filing legacy work under `service`.
9. **Completion.** `.completes` marked the first entry into a closed state, so:
   - throughput counted a cancellation as a completion;
   - a reopened job was never counted again;
   - `.open` was undefined for it.
10. **Percentiles by backend.** Percentiles were computed by nearest rank on SQLite and interpolated on PostgreSQL, so a metric guard near its threshold could pass on one backend and refuse on the other. The start of a week, and the time zone of a bucket, were left to the database.
11. **Smaller gaps:**
    - `.held` on a transition row did not say whether it is read before or after the transition's own write;
    - `avg` of a duration became a `decimal`;
    - the unknown-filter rule of ADR-0103 missed a reader's `filter` and a row's own collections;
    - `export` could not read the permanent refusal counts or labels.
12. **Per-object formulas over flow data.** A formula over one object's own transitions and intervals — handoffs on this job — could be written only as a metric grouped by object. PRD C1 calls a formula over one object a derived datapoint.
13. **Pruning trials.** Pruning the attempt log removed who an observing clause would have refused, while the clause was still on trial (UC-14).
14. **Unstated limits.** Three limits were unstated:
    - a label's name is free text that erasure does not reach;
    - a label's occurred time is its recorded time;
    - a set of enum values is not tracked.

This decision's first draft was verified with ADR-0105's. The verification found that a gap could go unreported — an undated creation, and a current stay across a silent span (D356); that "every count is the same after an erasure" was false as stated (D360); that D5 was marked covered while labels do not meet it (D362); and that the explanation input was said to be personal and marked so nowhere (D366). This version is the repaired one.

## Decision

### 1. A metric reports how far the record covers what it measures, beside whether the reader sees all of it

Each object records **`recorded_from`**, the time from which its record is continuous:
- for an object created here, its creation;
- for an imported object, the earliest time the mapping can vouch for:
  - the legacy creation, where its history is whole;
  - the end of the last silent span, where the legacy record has one;
  - the port, where the mapping supplies no history at all.

**Silent spans.** The mapping declares the legacy record's known silent spans. For the first consumer:
- transition audits were not written from 2026-02-03 to 2026-07-13, one span for every object alive then;
- side-effect status changes were never audited. The mapping finds these per object: where one audit row's value after the change is not the next row's value before it, the span between the two rows is silent.

The import cuts each legacy interval at a silent span, so no row lies inside one, and nothing is invented to fill it.

**Tracked members added later.** A member tracked from a later publish is tracked from that publish, as §4 says.

**`gaps`.** Every metric row reports `gaps`: the number of objects for which it read something from before what their record vouches for. That is one rule, stated once, with four cases:
- a window that begins before the object's `recorded_from`;
- an interval, legacy or current, that began before the object's `recorded_from` — a stay the legacy record says began in January, across a silent spring, included;
- a `.created_at` the port could not date (§2);
- a tracked member read from before its tracking began.

*(Restated by the verification, D356: the first draft compared `recorded_from` with `.created_at`, which are equal for an undated port, and missed a current interval that began before a silent span.)*

`MetricPage.history_complete` is false when any row has gaps.

**Two separate questions.** `complete` still says whether the reader sees every object. The two flags answer different questions:
- a partial reader over whole history;
- a whole reader over a silent span.

`gaps` is a comparison of stored times, not a scan for holes.

### 2. `.created_at` is when the creation happened

`.created_at` is the creation's occurred time:
- the backdated time, for a creation marked `backdatable`;
- the legacy creation, for a port that supplies one;
- otherwise the earliest time the port knows of the object — its first legacy interval, entry or entry time — or the port itself where it knows none, and the object is marked **undated**, so a metric that reads its `.created_at` counts it as a gap rather than reporting a short cycle.

The import refuses an entry time or a legacy interval earlier than a creation time the mapping supplies, so no measure from `.created_at` is negative.

### 3. A port's and a refresh's intervals begin when the legacy change happened

**The first import.** An imported object carries, for its state and for each tracked member:
- the time it took its current value;
- who made that change and their kind, where the legacy record says.

The import event opens the current interval at that time and carries it, so a fold of the log reproduces it. Legacy intervals cover only earlier values, and the last of them ends where the current one begins.

**A refresh.** A refresh of a mirror that changes a tracked value carries the same. It may also carry the intermediate values the legacy record shows since the last import, which the import opens and closes in order on its event. Every such row is marked as imported, and the harness checks them for order and overlap as it checks legacy rows.

**No-op writes.** A write that leaves a tracked value unchanged opens no interval, so counting intervals counts changes of hands.

### 4. A tracked member added by a publish is tracked from that publish

A publish that makes a member tracked opens, for every live object, an interval holding the member's current value, starting at the publish event. It is part of the publish's migration step and in its transaction. The fold opens it at the same event, so the log still rebuilds the index.

Time before the publish is not tracked, and `gaps` says so for any row that reads it.

### 5. A derived attribute that reads other objects is read under the reader's visibility, and says when it is partial

Wherever a reader reads a derivation — by `get`, by `query`, or in a metric row — its aggregates range over the objects that reader can see.

`Object.partial` names each derived attribute that reads a type the reader cannot see in full: the same rule that makes a metric's `complete` false. A derivation over the object's own members is never partial.

A guard reads a derivation over every object, as a type-scan guard does, because a rule's verdict may not depend on who asked (ADR-0096).

### 6. An override's reason is a declared enum

Check 29 requires an assertion's `reason` input to be of an `enum` type, so override counts per reason exist for every flow (PRD M1). A publish's `admit` names an enum member too, and an erasure's events carry the fixed reason `erasure`, since an erasure's own reason is free text that may name the person (ADR-0105 §4). An explanation in words is a further optional input, `detail : string? personal`, marked personal in every example, since free text may name someone.

### 7. A metric reads no personal value

A personal attribute or field may not be read anywhere in a metric:
- a filter;
- a `count(where …)`;
- a `count(distinct …)`;
- a dimension;
- a flag;
- a value.

Counting rows does not read one, and erasure removes no row and changes no value a metric reads. So every count over what was recorded before an erasure is the same after it (PRD UC-17), and a label's note is not readable by a metric at all. The erasure adds its own events: the `erase` request is a transition like any other, and the event an erasure records on an object reached by the taint analysis is marked `.redacted`, which the standard metrics leave out as they leave out imports and migrations, except the override counts where it carries an admission. *(Restated by the verification, D360: the first draft said every count is the same after an erasure, which the erasure's own events falsify.)*

### 8. Every metric can be split by version and by kind of actor

**Implicit dimensions.** Every metric, declared or standard, has the dimensions `version` and `actor_kind` unless it declares them. They are taken from its rows:
- the row's declaration version;
- the kind of actor that created the object, entered the interval, requested the transition or attempt, or recorded the observation or label.

`combine` passes both through from the metrics it combines. A reader drops them with `keep`.

**Silent legacy rows.** A legacy row whose record names no actor has the kind **`unknown`**. It is a value that appears only in data and never as a request's actor, so legacy work is not filed under the importer's `service`.

### 9. An object is open by its current state, and completes every time it closes

- `.open` is true while the object's current state is neither `closed` nor terminal, so a reopened job is open again.
- `.completes` marks every transition that takes an object from an open state into a closed or terminal one, so a job that is finished, reopened and finished again completes twice. Cycle time measures each completion from `.created_at`.
- `throughput` and `cycle_time_by_assignee` gain the dimension `state`, the state entered. A cancellation and a finish are therefore counted apart, and a reader chooses which closed states are success. The engine cannot know that.

### 10. Percentiles are nearest-rank, weeks are ISO, and every bucket is UTC

**Percentiles.** `percentile(p, body)` is the smallest value whose rank is at least `p × n`, computed as:
- `percentile_disc` on PostgreSQL;
- `ROW_NUMBER()` over the ordered group on SQLite.

`median` is `percentile(0.5, …)`, the lower middle of an even count.

**Buckets.**
- A week is the ISO 8601 week, beginning on Monday.
- Every bucket is computed in UTC, whatever a session's time zone. The store sets its sessions to UTC.

A probe of both backends over the same rows is owed, and `data-driven-engine.md` §7 lists it among the measurements to take.

### 11. Four smaller rules

- **`.held` on a transition row** is the value before the transition's own writes. A creation's is absent.
  - "Acting" in `acted_by_non_assignee` means requesting a transition on the object.
  - Recording an observation or a label on it is counted by that kind's own recorder, not as acting.
- **`avg` keeps its body's type.**
  - A duration's average is a duration.
  - An amount's average is an amount of that currency, rounded half to even.
  - A number's average is a `decimal`.
- **An unknown filter selects nothing** in a reader's `filter` argument and in an aggregate over a row's own collections inside a metric's value, as in a metric's `from` (ADR-0103).
- **`export` reads** `<Type>.attempt_counts` and `<Type>.labels` as well, so the permanent refusal counts and the labels can leave the store after a prune (PRD M5).

### 12. A derivation may aggregate over its own object's flow data

A derived attribute may range over `this.intervals`, `this.intervals(<member>)` and `this.transitions`, with the members those datasets have in a metric (syntax §6.9). For example, `derive handoffs = count(i in this.intervals(engineer) where i.value is not null) - 1` gives the handoffs on one job as a derived datapoint (PRD C1).

Such a derivation:
- reads the clock where it reads a current span's `.duration`, so no invariant may use it;
- is never `indexed` (check 46).

### 13. A would-be refusal is kept while its clause is on trial

`maintain(prune_attempts)` keeps every row with `enforced = 0` whose clause is still observing in the installed version, so who a trialled rule would have refused stays answerable for the whole trial (UC-14). Once a publish enforces or removes the clause, the next prune rolls those rows up like any other.

### 14. Three limits the record states rather than removes

- **A label's name is free text the store treats as vocabulary.** It is not personal, and erasure does not reach it: the known limit `DESIGN.md` §8 names for every free-text value, which PRD D8 asks for "like anywhere else in the store". A label's note is personal, as before.
- **A label's occurred time is its recorded time.** A label records that someone noticed something then. Where when it happened matters, the fact is promoted to an observation kind with `occurred within`. That promotion is the path the PRD's convergence requirements describe. So D5, a Should, is not met for labels, and PRD §12 proposes that D5 say so, beside the override, which is dated when the engine was told (D362).
- **A set of enum values is not tracked**, like a set of references (D11 already says so of teams). Membership intervals per element are the extension for both, and nothing in the first consumer needs either yet.

## Alternatives rejected

- **Withdraw the promise that gaps are reported.** A reader could then not tell a real zero from a silent record. UC-1 asks for figures "as far back as their ported history allows", which needs the store to say how far that is.
- **Record a gap as an interval with an unknown value.** Every aggregate over intervals would have to handle a row that is not a fact. The row would also look like history, which is what the harness exists to stop the import inventing.
- **Make `.created_at` absent where the legacy creation is unknown.** An aggregate leaves out an absent body silently. Keeping the port time, and saying the record starts there, keeps the object counted and says why its figure is short.
- **Keep the port's split, and count handoffs by change of value.** That repairs two metrics and leaves `entered_at`, `oldest_open` and every ageing condition restarting at cutover.
- **Evaluate a derivation over every object for every reader.** A count of units by model would tell a reader how many exist that they may not see (T5).
- **Withhold a derivation from a partial reader.** PRD M2 gives a reader who sees part of the data the figures over that part.
- **Count overrides per free-text reason.** It groups nothing, and it may hold a person's name.
- **Let a metric count a personal value's presence.** The count would fall when the person is erased, and UC-17 says it must not.
- **Let a reader's filter reach each component of a combined metric.** The reader would have to know every component's binder, which is what a combined metric exists to hide.
- **Interpolated percentiles.** SQLite has no such function. A hand-written interpolation is a second implementation to keep equal to PostgreSQL's, and it returns a value no row holds.
- **Only the first entry into a closed state completes.** Reopened work is real in the first consumer (`ServiceJob.reopen`, `Robot.accept_return`), and it would never be counted twice.
- **Exclude cancellations from throughput by name.** Which closed states are success is the deployment's judgement, and a dimension lets each reader make it.
- **Keep per-object formulas over flow data as metrics grouped by object.** A reader of one job would have to call `metric()` with a bound object id to learn what the PRD calls a datapoint of that job.
- **Give labels an `occurred within`.** It would duplicate an observation kind's one distinguishing feature, and make the unmodelled residue look modelled.
- **Treat label names as personal.** Metrics and diagnostics group by name, which is what labels are for, and a personal value may not be a dimension.

## Consequences

- **`DESIGN.md`:**
  - §5.2 defines `.created_at`, reads derivations under visibility, and lets a derivation read its own flow data;
  - §5.12 separates `gaps` from `complete`, adds the implicit dimensions, redefines `.open` and `.completes`, and defines percentiles and weeks;
  - §7 describes how intervals start at a port, at a refresh and at a publish;
  - §8 and §13 state the three limits;
  - §11 defines `recorded_from` and the silent spans.
- **`declaration-syntax.md`:**
  - §6.8 covers the label limits;
  - §6.9 covers `gaps`, `.held`, `avg`, percentiles, the implicit dimensions and personal values;
  - §6.11 adds the `state` dimension to throughput and cycle time, and redefines `override_counts`;
  - §8.1 redefines `.created_at` and `.open`;
  - §8.2 extends the filter rule;
  - §8.3 lets a derivation read its own flow data;
  - checks 29, 46 and 56 change.
- **`storage-schema.md`:**
  - `of_object.recorded_from`;
  - the refresh's and the publish's interval rows;
  - `txn` becomes an integer;
  - the rollup's `type` defaults to `''`;
  - the prune keeps rows still on trial.
- **`library-api.md`:**
  - `ActorKind.UNKNOWN`;
  - `Object.partial`;
  - `MetricRow.gaps`;
  - `MetricPage.history_complete`;
  - `ImportedObject.entered`;
  - `ImportedObject.recorded_from` and `ImportedObject.silent_spans`;
  - `export` sources.
- **`publish-and-import.md`:** §4's intervals and created rows, and the silent spans.
- **`adversarial-harness.md`:** checks imported and publish-opened intervals for order.
- **`first-consumer-cutover.md`:** names the two silent spans as the mapping's input.
- **`docs/PRD.md` §12** proposes four revisions the design now depends on, each awaiting the author:
  - D5, to say that an override and a label are dated when recorded;
  - D11, to say that attributes holding several choices are not tracked yet, as teams are not;
  - L1, to say that rules read recorded datapoints of a declared kind, so a label is never read by a rule;
  - N5, to say that values the engine holds no object for are not ported, and that silent stretches are reported rather than filled.

  `traceability.md` marks the four rows `revision proposed`. *(Decided 2026-09-24: the author accepted all four as PRD revision 7, and the rows are covered.)*

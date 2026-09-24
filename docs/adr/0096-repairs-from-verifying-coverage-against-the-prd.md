# ADR-0096: Repairs from verifying the coverage claims against the PRD

- **Status:** Accepted by the author, 2026-09-23, after a check of every decision against the design — decided at the author's direction, against `docs/PRD.md` D4, D5, D8, F7, C2, L2, M1, M6, N2, N5, T3, T5 and UC-2, UC-3, UC-10, UC-16, UC-18, UC-19; repairs D218 to D235
- **Date:** 2026-09-23
- **Refined by:** ADR-0097 — approval re-checks the drafted base version; ADR-0098 — standard metrics are declared, and `metric()` reads as a guard does; ADR-0099 — occurred time is a request field with a named clause; ADR-0100 — a verdict's objects and an attempt's read set are filtered like an event's. ADR-0106 — `.created_at` is the creation's occurred time; an object is open by its current state and completes every time it closes.
- **Refines:** ADR-0070, ADR-0082, ADR-0083, ADR-0084, ADR-0085, ADR-0086, ADR-0087, ADR-0092, ADR-0095

## Context

`design/traceability.md` claimed every PRD requirement covered, except C5, which the PRD leaves unverifiable. `scripts/check-traceability.py` checks that a citation exists, not what it says. So an independent reviewer read each cited passage against each clause of the PRD, and tried to write the use cases in the grammar. The first pass returned twenty findings (D218 to D229). A second pass over the repairs returned twelve more (D230 to D234), including one leak the first repairs introduced and one older defect both passes had missed. A third pass found that the second round's remedy for C2 broke M2, and D235 records it. Each finding was confirmed by reading the cited text before anything was changed.

Most findings sat in constructs spelled the same day, which is where the record's own lessons say the next defect is most likely to be. Three were older: ADR-0087's retry of a failed file deletion, a publish's approval not re-checking its dry run, and a personal enum kept in the interval index.

## Decision

### 1. An observation kind is erasable through a generated `forget`

Each kind expands with `erase forget`, taking a `reason`, which erases the observation's personal fields. For a label, that is its note. The subject's `erase` runs it on every observation in its parts without a declared step, and check 39 counts an observation part as reached. It may also be requested on one observation, by an actor holding the deployment's erasure capability. That is how a note about someone other than the subject is removed, and how a label's note is erasable on a type that declares no `erase`. (D218)

### 2. A correction names an uncorrected observation of its own kind and subject, and may follow a finished subject

The generated creation gains a third guard, `corrects_current`: a `corrects` must name an observation of this kind, on the same subject, that nothing has corrected yet. Three things follow:
- a correction is a recording, so `recorded_by` applies to it, and no one can hide another subject's results by naming them;
- `subject_open` applies to a new observation and not to a correction, since a correction adds no fact to a settled whole but replaces one already recorded, and PRD D4 allows no exception;
- the observation tool gains a `corrects` input, so an agent can correct as a person can.

(D219, D231)

### 3. A metric is computed over what its reader can see, and says whether that is everything

ADR-0084 filtered a metric's source rows by the reader's visibility, and said nothing about paths. Two defects followed. A path could read related objects the reader cannot see, against T5. And two readers got different values from one definition with nothing to tell them apart, against C2's "every consumer … gets the same value".

The repair has three parts:
- A path that reaches an object the reader cannot see yields absence at that step, so nothing of a hidden object shapes a value.
- The line is drawn between a value and an object. A stored reference is a value of the object that holds it, and its id is visible where that object is, as `get` returns it. What visibility protects is the referenced object.
- Every result says whether it is **complete**: complete when the reader can see every current object of each type the metric reads, and otherwise a value over the reader's own rows, marked as such. `diagnostics` follows the same rule.

So every reader who gets a complete value gets the same one, and a partial value is never taken for it (C2). A reader who sees part of the data still gets metrics over that part (M2), and nothing hidden shapes a value (T5). The flag discloses only that some current object of those types is hidden from the reader — not which, how many, or anything of it — which is what ADR-0095's read-set flag discloses too. `metric()`'s `filter` narrows the rows before they are aggregated.

A guard still computes over every row, as a type-scan guard reads every object of its type (ADR-0030). The value it records on the event, and carries in a refusal, is shown only to a reader or requester whose view of it would be complete. That widens ADR-0095's test from the source type to every type the metric reads.

ADR-0095's filter also covers an attempt's read set and consulted values. Where UC-10 would have a refusal name a value its requester may not see, T5 prevails, being a Must where UC-10 is hypothetical. (D220, D230, D235)

### 4. A refusal carries, and the attempt log records, what the failing clause consulted

`Unsatisfied.consulted`, `Attempt.consulted` and `ok_attempt.consulted` hold the metric values and evaluator verdicts the failing clause was decided on, under decision 3's visibility. A metric value is never personal (ADR-0084), and a verdict is not a value. The clause the verdict names holds the threshold. A refused data-driven decision therefore re-evaluates from the record as an applied one does. (D221, UC-10, L2)

### 5. The flow datasets carry what the use cases ask of them

- `.held(<member>)` on an interval or transition row is the value a tracked member held when the span began or the transition happened. Time in a state is then attributed to whoever held the object then, not to whoever holds it now (UC-18).
- `.created_at` and `.created_by_kind` are members of every object, taken from its creation event, and `ok_object` stores the kind (UC-16).
  - For a ported object, the import mapping may supply both from the legacy record. Where it supplies neither, the object counts from its port.
  - A member declared under either name, or as `id` or `state`, would shadow a built-in, and is refused by check 33.
- `.imported` marks the import's events, which keep provenance `asserted`, so an override count excludes them (UC-3).
- `.reason` is an assertion's `reason` input where it is a declared enum. Free text is not a dimension: it groups nothing and may hold personal data. The specification's own assertion example now declares its reasons (UC-3).
- The standard metrics gain override counts per target state and declared reason, with the import's excluded. They also gain cycle time by the assignee at completion, and time in each state by whoever held the object then (M1, M6).

(D222, D223, D234)

### 6. `closed` and terminal states say what is open

Categories are a consumer's vocabulary, but the standard metrics need to know when work is finished. Two places already relied on `closed` having a meaning of its own: ADR-0070's publish report, and the `RECORDED` state an observation kind expands into.

So every vocabulary has `closed`, whether or not a module declares it. Work is open until it enters a `closed` or a terminal state, and completes when it first does. Work in progress, the oldest open work, cycle time and open work per assignee are all read from that. A terminal state counts as finished whatever its category, so no lifecycle leaves work open forever by accident. (D233, D235)

### 7. An occurred time is never later than its recording

Backdating and `occurred within` bound how far back an occurred time may reach. It also may not be later than the time the change or observation is recorded, in the specification's §4.2 as well as `DESIGN.md`. So no interval is negative, and lead time cannot be shortened by dating forward. (D224, D234)

### 8. Refusal counts are daily, per object, permanent and readable

`ok_attempt_rollup` is kept per UTC day and per object, and carries the verdict. A deployment's prune deletes a day's attempt rows and adds their counts in the same transaction.

The metric source `<Type>.attempt_counts` reads the rollup for pruned days and counts the retained rows for the rest. It is therefore complete over the whole history, whether pruning has run or not.

Keeping the object keeps visibility applicable after a prune, so a prune changes no reader's value. A refused creation has no object, and counts for a reader only if they can see every current object of the type. (D225, D230)

### 9. A personal enum is tracked, and erasure redacts it in the interval index

The interval index kept every enum's values. Erasure skipped the index on the grounds that it held no personal value, which was false for a personal enum. The erased value then stayed readable through `intervals(<member>)` and `.held`.

Erasure now sets a personal enum's values to absence in the index, as it does in the events the index is rebuilt from. The rebuilt index and the redacted one therefore agree. PRD D11 keeps every choice-from-a-list attribute tracked, and D8 is met. (D232, D235)

### 10. Legacy intervals are imported history, checked for shape

The log rebuilds every interval it recorded. Intervals the import supplies from the legacy system are read-only and marked legacy, as legacy entries are, and they are not a projection. The harness checks them for shape, since nothing else holds a copy to compare against:
- none overlaps another of its dimension;
- the last ends where the object's first recorded interval begins.

(D226, D234)

### 11. A failed file deletion is retried by the next erasure

ADR-0087 left a failed deletion to "a sweep", which no operation owns and which would be a background process correctness depends on (N2). Every erasure request retries the pending deletions before its own. `diagnostics` lists them under the type of each object the erasure reached. A deployment that sees one re-requests an erasure, which is idempotent. (D227, D234)

### 12. A publish recomputes its impact report, and refuses if it names anything new

Approval runs the dry run's steps again inside the publish transaction. The report keeps the ids it names, and the publish is refused if the recomputed report names any object or proposal the approved one did not. The change then returns with the new report, to be approved again. An approval is of the impact it was shown, and a set of the same size can hold different objects. (D228, D234)

### 13. Smaller points

- A metric guard that leaves a declared dimension unbound aggregates over it, so one monthly metric serves a guard's window without a second definition (C2).
- Check 54 reserves every generated member's name.
- Four statements that ADR-0095 superseded are corrected.
- A covered row in the traceability map must cite a document that owns an implementation, which the checker now enforces.

(D229)

## Alternatives rejected

- **Filter rows as ADR-0084 did, and say nothing more.** A path reads hidden objects, and a partial value is indistinguishable from the complete one.
- **Withhold every group the reader cannot wholly see.** This was the first draft of this decision. A reader who can see 99 of 100 objects then gets nothing from any standard metric, since those metrics group by state and week, not by whatever the reader's visibility selects. That fails M2. And a withheld group still disclosed its dimension values, which a group made wholly of hidden rows should not.
- **Require every subject with a personal observation field to declare `erase`.** It leaves a label's note, which every type carries, unerasable on any type without one, and it cannot remove a note about someone other than the subject.
- **Refuse a correction on a terminal subject.** D4 has no exception. The rule it protected is that nothing new is added to a settled whole, and a correction adds nothing new.
- **Hide every stored reference to an object the reader cannot see.** The object would read differently to different actors, a caller could not write the reference it cannot read, and `expected_version` would guard a value the caller never saw.
- **Attribute time to the current assignee.** A reassigned job would move its whole history to its latest engineer.
- **Let each type declare which states count as finished.** That adds a marking to every type in order to answer a question that one reserved category already answers, and the record already relied on that category.
- **Keep refusal counts monthly, or per type only.** UC-2 counts per week, and a count with no object cannot be filtered by visibility once its rows are gone.
- **Update the rollup on every refusal.** An observing clause's would-be refusal is written inside the request's transaction, so every such request would contend on one row per day.
- **Never track a personal attribute.** This was the first draft of this decision. It narrows PRD D11's "every choice-from-a-list attribute" without a revision, while redaction costs one statement in an erasure that already rewrites the events.
- **Keep the sweep and name it an operation.** It would be an eighteenth operation that exists to be called on a schedule, which is a background process by another name.
- **Compare the recomputed report's counts at approval.** A different set of violating objects of the same size would pass, and a violation nobody saw would be installed.

## Consequences

- `DESIGN.md` §5.1, §5.2, §5.4, §5.7, §5.11, §5.12, §5.13, §6, §7, §8, §9 and §10 state these decisions.
- `declaration-syntax.md` spells them in §1, §2, §4.2, §6.8, §6.9, §6.10 and §8.1, with check rows 33, 39, 54, 56 and 58 amended.
- `storage-schema.md` gains `ok_object.created_by_kind`, `ok_attempt.consulted`, a daily per-object rollup with its prune rule, a personal enum's intervals redacted at erasure, and the retried deletion.
- `library-api.md` gives `Unsatisfied`, `Attempt`, `Object` and `Diagnostic` their new fields, and `metric` a `MetricPage` that says whether it is complete, and filters attempts as events.
- `renderers.md` §3's observation tool gains `corrects`.
- `publish-and-import.md` §1 compares the ids at approval, and §4's mapping supplies legacy creation times.
- `adversarial-harness.md` §1 checks legacy intervals for shape.
- D218 to D235 are resolved.

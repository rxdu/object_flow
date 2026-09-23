# ADR-0083: The engine keeps its own flow data — attempts, intervals and occurred time — and keeps imported state apart from overrides

- **Status:** Accepted by the author, 2026-09-23 — evaluated at the author's direction against `docs/PRD.md`, the baseline for every design choice
- **Date:** 2026-09-23
- **Refined by:** ADR-0086 — assignment is a declared role on a tracked reference, with its metrics generated. ADR-0095 — backdating is bounded by every state and tracked value the request changes, cascades share the request's occurred time, and `backdatable` is refused on an `assert`, an `erase` or an `only via` transition. ADR-0096 — an occurred time is never later than its recording; refusal counts are daily, per object and readable; legacy intervals are imported history; a personal enum's intervals are redacted at erasure.
- **Refines:** ADR-0033, ADR-0077

## Context

The PRD requires that the data a flow generates be captured with no effort from its author (D1). That covers every transition and every exception: refusals, overrides, admitted violations, conflicts and requests over a limit (D2). It also requires time in each state, work in progress and age for every flow (M1), durations computed from when things happened rather than when they were typed in (D5), and overrides reviewable without an import burying them (T4).

Today a refused request is rolled back and nothing is recorded (`DESIGN.md` §5.4, §6). Time in state is recoverable only by folding events. Every imported object sits in `exceptions(type)` indefinitely (D205). `design/data-driven-engine.md` §3.3 evaluates the options.

## Decision

### 1. An attempt log records every request that did not apply

It gets one row per request whose verdict was anything but *satisfied*, and one per `UnknownTransition` or `KeyReused` fault. Each row carries:

- the time;
- the actor's id, kind and principal;
- the context;
- the type, and the object if there is one;
- the transition;
- the verdict kind;
- the failing clause, its remedy class, and whether it was unknown;
- the declaration version.

**Inputs are never recorded**, so the log holds no personal value and erasure has nothing to do there. It is written in its own short transaction after the request's rolls back, making it the second thing written outside the request's transaction, after the sequence mint. It is not history: `history` does not return it, subscriptions do not deliver it, and a deployment prunes it on a retention period while keeping monthly rollups. Observing guards record their would-be refusals here as well (ADR-0085), marked not enforced and linked to the event that applied.

### 2. Intervals record time in each state and in each value of a tracked attribute, as an index the log rebuilds

An interval table is maintained in the transition's transaction. It covers the state, every enum attribute and every singular stored reference: who, where, and which bucket an object is in over time. Each row holds the object, the dimension, the value (absence being one), entry and exit (as position and occurred time), the transition that set it, that transition's actor's kind, and the declaration version. Numbers and free text are not tracked; their changes remain in the events. The log can rebuild it, so it is an index in the sense of ADR-0048 and ADR-0057, not a second source of truth — and a marking added in a later version, such as `assignee` (ADR-0086), reads intervals back to the reference's first write. `DESIGN.md` §10's "no projection" becomes "no projection the log cannot rebuild". The harness checks intervals against the log as it checks rows. An import may supply each object's current-state entry time, and earlier intervals from legacy history, marked as legacy.

### 3. A transition may be backdated within a declared bound

A transition marked `backdatable within <duration>` accepts an occurred time inside that bound. The event keeps both times, and intervals use the occurred one. An occurred time may not precede the object's previous state change, so no interval is negative. Guards still read `now` as the clock.

### 4. Imported state is not an override

`state_source` gains `imported`. The import's events keep the provenance `asserted` that ADR-0015 requires, but `exceptions(type)` and every override metric read `asserted` alone. A migration mapping leaves `state_source` unchanged, so an asserted object does not drop off the list by being migrated. This is D205's repair.

### 5. Each event records its serialisation retries

So the contention D203 measured is visible in production, per transition.

## Alternatives rejected

- **Refusals as events in the permanent log.** A refusal has nothing for a fold to reproduce (ADR-0033). A refused creation has no object for the event to belong to. And a guessing agent's mistakes would become permanent history.
- **Refusal counters only.** They cannot show one actor repeating the same refused request, which is UC-2.
- **Record refusals with their inputs.** It would enable replaying requests against a candidate declaration, but it puts personal data in a table built to have none (`design/data-driven-engine.md` §3.6).
- **Compute time in state from events on read.** Correct, but it makes current age and work in progress a window function over every event of a type. The index costs one update and one insert per change of a tracked value.
- **Intervals for state only.** Time with each assignee, time in each location and time at each priority are then per-type folds of event payloads, and the assignment metrics of ADR-0086 cannot be generated.
- **Recorded time only.** Time in a state then includes however long someone took to type it in (UC-6).
- **Identify imported objects with a query rather than a column.** The imported object and the repaired one both hold `asserted`, and the difference exists only at the moment of writing.

## Consequences

- On acceptance, `DESIGN.md` §5.4, §6, §7 and §10 describe the attempt log, intervals and occurred time.
- `design/storage-schema.md` gains `ok_attempt` and `ok_interval`, and a retry count and occurred time on `ok_event`.
- `design/library-api.md` gains the attempt and interval shapes.
- `design/adversarial-harness.md` gains a ninth failure condition: an interval the log does not reproduce.
- `design/publish-and-import.md` lets the mapping supply entry times.
- D205 is resolved on acceptance.
- ADR-0033 and ADR-0077 carry the back-link.

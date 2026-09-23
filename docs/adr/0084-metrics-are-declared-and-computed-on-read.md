# ADR-0084: Metrics are declared and computed on read; a guard may read one as of the start of its request

- **Status:** Proposed — evaluated 2026-09-23 at the author's direction, against `docs/PRD.md`; pending author review
- **Date:** 2026-09-23
- **Refines:** ADR-0047, ADR-0049

## Context

The PRD requires users to define compound datapoints with formulas over raw data (C1). A formula must be declared once and read identically by every consumer (C2). It must aggregate over time windows and dimensions (C3), and apply to history recorded before it existed (C4). Every flow must have a standard set of metrics (M1), readable by people and agents under visibility (M2), comparable across flow versions (M3), and splittable by actor kind (M4). A flow's rules may depend on such data, with the value each decision used recorded (L1, L2).

The expression language refused grouped aggregation (ADR-0047). ADR-0049 set the pattern for a guard over a value from outside the transaction. `design/data-driven-engine.md` §3.4 and §3.5 evaluate five ways to define metrics and four ways for a rule to read one.

## Decision

### 1. A metric is a declared form

```
metric supplier_lead_time version 1 {
  from  i in ProcurementOrder.intervals where i.state == ORDERED
  by    supplier = i.object.supplier, quarter = quarter(i.left_at)
  value percentile(0.8, i.duration)
  flag  slow when value > 21 days
}
```

Its semantics, with the grammar left to `design/declaration-syntax.md`:

- **Sources:** a type's current objects, an observation kind (ADR-0082), or a type's three datasets — `intervals`, `transitions` and `attempts` (ADR-0083).
- **Filters:** the existing expression language.
- **Dimensions:** paths up to two hops, the actor's kind, the declaration version, and time buckets over any timestamp.
- **Aggregates:** the existing set plus `avg`, `median` and `percentile`.
- **Values:** a value may combine aggregates arithmetically.
- **Windows:** a window relative to `now` is allowed.
- **Flags:** named conditions on the value, declared once, which a scheduler or an agent queries.

No invariant may read a metric.

### 2. Metrics are computed on read, under the reader's visibility

A metric compiles to SQL over the store's tables. Source rows are filtered by the source type's visibility predicate, which check 7 already requires to be expressible as a query filter; the three datasets and observations inherit their subject's. A metric is therefore retroactive by construction, and stores nothing. A cache keyed by visibility may be added if measurement asks for one.

### 3. Personal data is never a dimension or an output

A personal attribute or field may be counted, and may not be a dimension, a flag's operand or an unaggregated output. Erasure then needs nothing from any metric.

### 4. Every type gets the standard metrics

They are generated from its datasets:

- time in each state;
- work in progress;
- throughput;
- age of the oldest open objects;
- transition counts;
- refusals by clause, remedy and actor kind;
- overrides by target state and reason;
- rework.

They are listed with the type like any declared metric. A type with an assignee also gets the assignment metrics of ADR-0086, and every tracked value gets time-in-value and change counts from the same intervals.

### 5. Per-object derived attributes stay, and gain time over the object's own intervals

`entered_at(STATE)` and `time_in(STATE)`. A per-object alert, such as a backorder ageing past a threshold, is a derived attribute queried as ADR-0048 allows.

### 6. A guard may read a metric, consulted before the transaction as an evaluator is

A guard names the metric and its dimension values from the object and the inputs: `metric(inspection_pass_rate, model := this.robot_model, over last 30 days) >= 0.90`.

- **Arguments are resolved twice.** They are resolved against committed state before the transaction and again inside it. A mismatch means the world moved in between, and the request consults again within its serialisation retry bound.
- **The value and its as-of time go on the event** beside the guard's name.
- A metric guard may declare a freshness bound.
- `check` and `availability` compute internal metrics, since no third party is involved.
- A transition with a metric guard is not sweepable.

### 7. A snapshot is the pattern for the two cases a metric guard does not serve

When a transition must be sweepable, or a person must set the value, a declared transition writes the metric's value into an attribute, and guards read the attribute.

### 8. The read surface gains `metric(actor, name, filter?)`

It returns the rows the reader may see, with any flags that hold.

## Alternatives rejected

- **Per-object derived attributes only.** They cannot express a cross-object aggregate per model per month (UC-8).
- **User-written SQL views.** They cannot be held to visibility or to the personal-data taint at publish, they couple consumers to a physical schema, and their dialect differs across the backends.
- **An external BI tool, as today.** Definitions drift, rules cannot read the results, and agents read a different definition from people.
- **Metrics maintained incrementally at write time.** They are not retroactive without a backfill, and their stored aggregates need repair after an erasure. Every recorded datapoint also updates one aggregate row, a hot row by construction, with the cost D203 measured.
- **A guard that evaluates a metric inside the transaction.** Under serialisable isolation its read set is every row the metric aggregates, so recording any one of them conflicts with every request reading the metric (`design/data-driven-engine.md` §3.5).
- **No metric may be read by a guard.** That fails L1.

## Consequences

- On acceptance, `DESIGN.md` §5.5, §5.7 and §10 describe metrics, metric guards and the new read; `design/declaration-syntax.md` gains the form and its checks; `design/library-api.md` gains `metric`; `design/renderers.md` gains a metrics tool whose description names metrics and never restates one.
- Percentile on SQLite is computed by nearest rank with window functions, which a probe confirmed on SQLite 3.37.2 and which a checker should keep tested.
- ADR-0049's own evaluators have the argument gap point 6 closes for metrics (D215).
- ADR-0047 and ADR-0049 carry the back-link.

# ADR-0118: Metrics are written under their type, in the terms of semantic layers

- **Status:** Accepted by the author, 2026-09-26: "verify the terms first, then go with your recommendations", in answer to four questions on writing the metric language (construct 7 of ADR-0116's plan).
- **Date:** 2026-09-26
- **Refines:** ADR-0115 (established terms), ADR-0116 (the format replaces the text language)
- **Relates to:** ADR-0084 (metrics), ADR-0098 (a reader reads a metric as a guard does), `declaration-syntax.md` §6.9, `flow-format.md` §4.9

## Context

Until this construct the format wrote two fixed measures under a type, `median_time_in_state` and `transition_count`, and nothing else of the model's metric language: a source of rows with a binder and a filter, dimensions, a timestamp to window on, a value, flags, and `combine`, a metric over other metrics. Four questions were put to the author: where a metric is declared, which words it uses, whether `combine` comes now, and whether a row is read through a name the metric declares. The author asked that the terms be verified before any was adopted.

**What was verified**, on 2026-09-26, from each product's current documentation:

- **dbt (MetricFlow).** A simple metric is declared under the semantic model it reads, and a metric across models (ratio, derived) under a top-level `metrics` block ([latest spec](https://docs.getdbt.com/docs/build/latest-metrics-spec)). A metric's `filter` is "WHERE clause equivalent"; a derived metric combines others with `expr` and `input_metrics`; a filter reads a metric by dimensions with `{{ Metric('metric_name', group_by=['entity_name']) }}` ([metrics overview](https://docs.getdbt.com/docs/build/metrics-overview)). "Dimensions represent the non-aggregatable columns in your data set … that describe or categorize data", of type categorical or time, with granularities from `day` to `year` ([dimensions](https://docs.getdbt.com/docs/build/dimensions)). A metric is aggregated against its `agg_time_dimension`, and `agg` is one of `sum`, `max`, `min`, `average`, `median`, `count`, `count_distinct`, `percentile` and `sum_boolean` ([simple metrics](https://docs.getdbt.com/docs/build/simple)).
- **Cube.** "Dimensions are attributes related to measures", and a time dimension's default granularities are "second, minute, hour, day, week (starting on Monday), month, quarter, year" ([dimensions](https://docs.cube.dev/reference/data-modeling/dimensions)). A measure's types include `count`, `count_distinct`, `sum`, `avg`, `min` and `max`, and it narrows its rows with `filters` ([measures](https://docs.cube.dev/reference/data-modeling/measures)). A query's `dimensions` is "An array of dimensions", and `timeDimensions` is "a convenient shortcut to pass a dimension and a filter" for time ([query format](https://docs.cube.dev/reference/core-data-apis/rest-api/query-format)). *Retracted 2026-09-26: this sentence first quoted Cube as listing "dimensions to group by". That wording came from a summarising fetch and is not on the page; it was invented. The conclusion rests on the dimensions page quoted above.*
- **Looker (LookML).** A measure's types include `count`, `count_distinct`, `sum`, `average`, `median` and `percentile`, and an aggregate measure narrows its rows with `filters` ([measure types](https://docs.cloud.google.com/looker/docs/reference/param-measure-types)). A time `dimension_group` generates `date`, `week`, `month`, `quarter` and `year` timeframes, and "by default, weeks in Looker start on Monday" ([dimension groups](https://docs.cloud.google.com/looker/docs/reference/param-field-dimension-group)).

None of the three has a threshold declared with a metric, the model's `flag`.

## Decision

1. **A metric is declared under the type whose rows it reads.** Every source of the model is rooted in one type: its objects, its intervals, its transitions, its attempts and their daily counts, and the observation kinds declared on it. dbt declares a simple metric the same way, under the semantic model it reads. The source is named relative to the type: `objects`, `intervals`, `intervals(<member>)`, `transitions`, `attempts`, `attempt_counts`, or an observation kind's name.
2. **The keys are the semantic layers' where they name the same thing**: `dimensions` for the model's `by`, a mapping from a dimension's name to its expression; `filter` for the `where` of its `from`; `time_dimension` for `window on`. The value is written under `expression`, the format's key for an expression, and flags keep the format's `flag_when`, since no standard has them. The aggregates inside an expression keep the model's names (`avg`, `median`, `percentile`, `count(distinct …)`), since the format does not change the expression language (`flow-format.md` §5), and the time buckets `day`, `week`, `month`, `quarter` and `year` are the same in all three products and the model; Cube and Looker begin a week on Monday, as the model does, and dbt's week start was not verified.
3. **A row is read through a name the metric declares**, `item`, as a `foreach` step names its element.
4. **The two fixed measures remain** as a shorthand, and their `group_by` names the dimensions to group by, as `group_by` does in dbt's `Metric()` reference; the metric language declares its dimensions with `dimensions`.
5. **`combine` is not written yet.** A metric over other metrics may cross types, and dbt declares it at the top level; it is written when a flow needs it, as a module-level section.

## Alternatives rejected

- **A module-level `metrics` section for every metric**, as the model declares them. It is closer to the model, but a metric would then be declared in one of two places, and every source is one type's anyway.
- **The model's keywords**, `by`, `window on`, `value` and `where`. ADR-0115 adopts an established term where one exists; the semantic layers agree on dimensions and a filter.
- **dbt's aggregation names** (`average`, `count_distinct`) in the value. They are keys of a YAML form in dbt; here the value is an expression in the model's language, whose `avg` is SQL's and Cube's.
- **A fixed name for the row**, such as `row`, in place of `item`. An explicit name reads as the `foreach` step reads, and lets a value nest an aggregate over the row's own flow data with a name of its own.

## Consequences

- `flow-format.md` §4.9 describes both forms, §3 states a metric's key order, and Appendix A lists these terms with their sources; Appendix B marks the metric language written, `combine` excepted.
- `scripts/check-flows.py` checks what the internal checker does not: the source, the item, each row member read, what a flag reads, and enumeration values; dimension hops, personal members, metric-only functions, tracked members and a guard's bindings stay with step 4 (checks 56 and 58).
- The service example has the model's own `inspection_pass_rate`, which converts to the model's text line for line, and a metric over a derived attribute.
- *Refined 2026-09-26, at the author's direction ("go ahead with the three waiting constructs too"):* `combine` is written as decision 5 foresaw, in the module's own `metrics` section, and names its inputs `input_metrics`, dbt's key for a derived metric's inputs, verified with this ADR's sources; it groups with `group_by`, as the fixed measures do. `flow-format.md` §4.9 describes it.

# ADR-0124: A combined metric read by fewer dimensions combines its inputs over them

- **Status:** Decided at the author's direction, 2026-09-27: "write the scenarios for UC-16 to UC-18", under the standing direction to "continue with your inference, as long as the inference is based clear requirements we've already discussed and the use case we've reviewed". Awaiting the author's review.
- **Date:** 2026-09-27
- **Refines:** ADR-0098 (combined metrics, and a reader reads a metric as a guard does), ADR-0122 decision 6 (a combined metric aggregates its inputs to its own dimensions)
- **Relates to:** `declaration-syntax.md` §6.9; `flow-format.md` §4.9; `metric-scenarios.md` §30, §33

## Context

UC-16 compares the refusal rate of agents with people's for `complete_sale`. The standard `refusal_rate`, the one standard combined metric, combines `refusals` and `transition_counts` by five dimensions: transition, clause, week, version and actor kind (`declaration-syntax.md` §6.11, as its ServiceJob instance declares them). The lead binds the transition and keeps the actor kind. The model says a reader "may name the dimensions to keep with `keep`, aggregating over the rest", and a guard "aggregates over every dimension it leaves unbound" (§6.9).

For a metric with rows, aggregating over a dimension means computing the value over more rows. A combined metric has no rows of its own, so it was unstated which of two things "aggregating over the rest" means:
- aggregating the combined values: summing or averaging the rates of each clause and week;
- aggregating each input over the rows the read selects, then combining.

A rate of rates is not a rate. People's refusal rate over two weeks is the refusals over the attempts of both weeks, not the mean of each week's rate. Found writing UC-16's scenario (D449).

## Decision

**A combined metric read by fewer dimensions than it groups by, or with some bound, is computed from its inputs.** Each input is aggregated over the rows the read selects, those matching every bound dimension, to the dimensions the read keeps. The expression then combines them, and the join over groups is as ADR-0122 decision 18 says. A guard's reference is read the same way, keeping no dimension. This is ADR-0122 decision 6 applied to a reader's `keep` and `bind` as to the metric's own `group_by`.

## Alternatives rejected

- **Aggregating the combined values.** An average of rates weighs a week with one attempt as heavily as a week with a hundred, and a sum of rates has no meaning. Two readers keeping different dimensions would also get values that do not agree with each other.
- **Refusing a `keep` narrower than a combined metric's `group_by`.** A reader could not ask the standard `refusal_rate` for a rate per transition without declaring a second metric for it, the duplication PRD C2 exists to prevent.

## Consequences

- `declaration-syntax.md` §6.9 and `flow-format.md` §4.9 state the rule.
- `metric-scenarios.md` §30 reads the refusal rate this way, and `scripts/check-scenarios.py` computes it so.

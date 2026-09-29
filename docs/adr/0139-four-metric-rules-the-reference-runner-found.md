# ADR-0139: Four metric rules the reference runner found: a fixed measure's dimensions, an average of integers, a dimension bound to an unknown value, and a window's ends

- **Status:** Accepted by the author in advance, 2026-09-29: "for all similar items that you have confidence in an recommendation, just accept", the standing direction under which ADR-0128 was accepted.
- **Date:** 2026-09-29
- **Refines:** ADR-0084 (a guard reads a metric), ADR-0106 (`avg` keeps its type), ADR-0118 (a fixed measure under its type), ADR-0128 (a metric's value rounded to its expression's scale)
- **Relates to:** ADR-0136 (examples, and the reference runner); `declaration-syntax.md` §6.9, §8.2, §8.3; `flow-format.md` §4.9, §6; `DESIGN.md` §5.12

## Context

The reference runner's fourth slice computes a metric a guard reads over an example's own history (ADR-0136 decision 5). It computes one from its declaration, not from a transcription, and four things the declaration needs were not stated. The runner reported an example reaching any of them as not run rather than guessing.

1. **A fixed measure's dimensions and window.** `flow-format.md` §6 converts `median_time_in_state` to "a metric over the type's intervals in the state, valued `median(i.duration)`", and `transition_count` to "a metric over the type's transitions along the transition's states, valued `count()`". It does not say what each `group_by` entry becomes, or what `over last` windows on. `declaration-syntax.md` §6.9 writes service's `time_working` out in full, `by engineer = i.held(engineer), month = month(i.entered_at)` and `window on i.entered_at`, which answers the first measure for a tracked attribute and a month; nothing answers the second.
2. **The scale of an average of integers.** ADR-0106 made an average keep its body's type, except that "a number's average is a `decimal`", so that the average of 1 and 2 is not an integer. ADR-0128 rounds a metric's value to its expression's scale. An average of integers has no scale, and two of the design's metrics are one: service's `handoffs_per_job`, `avg(o.handoffs)`, and the service desk's `satisfaction`, `avg(s.score)`.
3. **A dimension bound to an unknown value.** A guard binds `engineer := engineer`, and a job may have no engineer. §6.9 says a row whose dimension has no value is counted in a group of its own, for a reader who keeps the dimension. Whether a guard binding an absent value reads that group, or reads nothing, was unstated.
4. **A window's ends.** `over last <duration>` keeps the rows whose time "falls within that long before `now`". Whether a row exactly that old is within it, and whether a row whose time dimension is absent is within any window, were unstated. The example clock moves by whole units, so an example lands on the boundary as easily as not.

## Decision

1. **A fixed measure reads what `time_working`'s text form shows, and `transition_count` the same way over its transitions.**

   | `group_by` | `median_time_in_state` | `transition_count` |
   |---|---|---|
   | `month`, `week` | the bucket of `i.entered_at` | the bucket of `t.occurred_at` |
   | `actor` | not allowed (flow-format §4.9) | `t.actor_id` |
   | a tracked attribute | `i.held(<attribute>)`, as it was when the span began | `t.held(<attribute>)`, as it was before the transition's writes |
   | any other attribute | `i.object.<attribute>`, as it is now | `t.object.<attribute>`, as it is now |
   | the window | `i.entered_at` | `t.occurred_at` |

   A `transition_count`'s rows are the transitions along its transition's states: from one of its `from` states, or any for `from: any`, into its `to`, or back into the same state for an internal transition, and from no state for an initial one.
2. **An average of integers is a `decimal(19,6)`, rounded half to even**, the type §8.3 gives a quotient of two like quantities, which is also what an average is. An average of decimals keeps their type, as ADR-0106 has an average keep its body's. So `handoffs_per_job` over jobs with 1, 2 and 1 handoffs is 1.333333.
3. **A dimension bound to an unknown value leaves the metric `unknown`.** A guard reading it is unsatisfied and says `unknown`, as a guard over a metric with no data already does (§6.9), and as any operator with an unknown operand is unknown (§8.2). A reader asks for the rows whose dimension is absent by keeping the dimension, as before.
4. **A window holds both its ends, and a row with no time is in none.** `over last d` keeps the rows whose time dimension is from `now - d` to `now`, both included. A row whose time dimension is absent is outside every window.

## Alternatives rejected

- **Reading every `group_by` attribute as it is now.** It contradicts `time_working`'s text form, and ADR-0106's `.held`, under which time is attributed to whoever was assigned then, not whoever is now.
- **Windowing a fixed measure on a span's exit.** A current span has none, so the work in progress would leave every window.
- **Counting a `transition_count` by the transition's name.** `flow-format.md` §4.9 counts "along its transition's source and target states", which is why it refuses a transition sharing both with another.
- **An average of integers as an integer.** It is the truncation ADR-0106 made the average a decimal to avoid.
- **Two places, as a figure is usually shown.** How a value is shown belongs to whoever shows it, not the engine. Two places is also a scale no other rule of §8.3 produces.
- **An exact fraction, unrounded.** A value with no declared scale is stored and compared differently by the two backends, the difference ADR-0128 rounds to avoid.
- **Reading the absent group for a dimension bound to an unknown value.** A job with no engineer would be decided on the pass rate of results no one is named for, a figure about nobody. Refusing such a binding at publish cannot work either, since whether the value is absent is known only when the request is made.
- **A window that leaves out its start.** A row exactly `d` old is "within `d`" as the phrase is read. The metric scenarios' checker, `scripts/check-scenarios.py`, already counts it.
- **Counting a row with no time in every window.** A window would then select rows it cannot date.

## Consequences

- `declaration-syntax.md` §6.9 states decisions 2 to 4, and `flow-format.md` §4.9 and §6 state decision 1. `DESIGN.md` §5.12 states decisions 2 and 4.
- The reference runner, `scripts/flowrun.py`, computes a metric a guard reads from its declaration: declared formulas, fixed measures, combined metrics and the standard metrics of §6.11. It reads the store as the request found it, whichever transition of the request reads it, as ADR-0084 §6 consults a metric before the transaction. A metric over `attempts` or `attempt_counts` is reported as not run, since the runner keeps no attempt log.

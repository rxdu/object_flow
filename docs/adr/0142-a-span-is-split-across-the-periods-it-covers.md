# ADR-0142: A metric may split each span across the calendar periods it covers

- **Status:** Accepted by the author, 2026-09-29: "accept all three recommendations", on the three questions the proposal put, which decisions 2, 4 and 9 now answer.
- **Date:** 2026-09-29
- **Refines:** ADR-0103 §5 (a share of time), ADR-0101 (where a span ends), ADR-0106 (calendar buckets), ADR-0123 (a group with no body)
- **Relates to:** PRD C3; ADR-0084 (metrics computed on read); `declaration-syntax.md` §6.9; `flow-format.md` §4.9; `flow-review.md` §4.2; `edge-cases.md`; `unit-journey.md` §2

## Context

A metric groups a span by a calendar bucket of one timestamp, so `month(i.entered_at)` puts the whole of a span in the month it began. A share of time is right over a unit's whole history and wrong per period: a unit that joined the lending pool in January puts all its pool time in January, and a monthly utilisation divides the wrong numbers (ADR-0103 §5, `edge-cases.md`).

The operations review found that monthly fleet utilisation, the rental headline, cannot be declared, and called it "the one finding that asks the engine itself to grow" (`flow-review.md` §4.2). The first consumer's PRD needs it as a general need, PRD C3 over periods (`first-consumer-prd-check.md` §4). The unit's journey shows the gap in its own metrics. `Robot.time_in_pool` and `EngagementLine.time_on_loan` are grouped "by month" as `month(i.entered_at)`, so each is already wrong per month, and `pool_utilisation`, which divides one by the other, is declared over the whole history only (`unit-journey.md` §2).

## Decision

1. **A metric over a type's spans may split them by a calendar bucket.** A formula whose source is `intervals` or `intervals(<member>)` may declare `split_by: <bucket>`, one of `day`, `week`, `month`, `quarter` or `year`, in the model `from i in <T>.intervals split by <bucket>`. Each of its rows is then one **piece** of a span: the part of it that falls in one bucket, the span cut at every bucket boundary it crosses.
2. **A piece has every member of its span, and three of its own.** Its span's members are unchanged: `.object`, `.state` or `.value`, `.entered_at`, `.left_at`, `.entered_by_kind`, `.declaration_version`, `.legacy` and `.held(<member>)`. It adds `.period`, the bucket it falls in, as the bucket function writes it, and `.piece_start` and `.piece_end`, its bounds. **Its `.duration` is the piece's**, the part of the span's duration inside its bucket. So `sum(i.duration)` grouped by `i.period` is the time per period, and a metric changes only by declaring the split and grouping by `.period`.
3. **The pieces cover the span's duration as ADR-0101 defines it, and no more.** A current span is cut up to `now` while its object is open. On a finished object, a span runs to the entry of the finishing state. A span with no duration yields one piece, in the bucket it began, with no duration, so its group is reported as ADR-0123 reports a group with no body.
4. **A window clips a split metric's pieces.** `over last <duration>` keeps the part of each piece that lies between `now` less the duration and `now`, both included, and leaves out a piece with none there, so "time in the last 30 days" is exact at any split. A split metric's time is its pieces', so it declares no `time_dimension`, and `over last` needs none on it. A piece with no duration is kept where its start lies in the window.
5. **Buckets are those of ADR-0106:** UTC calendar time, whatever a session's time zone, and a week is the ISO 8601 week that begins on Monday. Working-hours calendars stay a non-goal (PRD §3).
6. **What a split metric counts is pieces.** `count()` counts a span once in each period it covers, which is the number of spans active in the period, such as the units in the pool during a month.
7. **Step 3 refuses `split_by`** on a source that is not a type's spans, a `time_dimension` on a metric that splits, and `.period`, `.piece_start` or `.piece_end` read by a metric that does not. The model's check 56 does the same.
8. **The journey declares monthly utilisation.** `time_in_pool` and `time_on_loan` split by month and group by `i.period`, and `pool_utilisation` groups by `[model, month]`. `flow-review.md` §4.2's finding closes, and ADR-0103 §5 and `edge-cases.md` say the limit is lifted where a metric splits.
9. **It belongs to the first release**, since PRD C3, which it serves, is a Must, and the first consumer names monthly utilisation as its rental headline.

## Alternatives rejected

- **A per-row function, such as `i.duration_in(month)`.** Each row still belongs to one group, so a span that crosses three months could not count in each of them.
- **Splitting every span source always.** Every existing metric would change meaning. `count()` would count pieces rather than spans, and a median time in state would be a median of pieces, which is not a time anything spent.
- **A fixed measure only, time in state per period.** It covers time in a state, but not the time a tracked member held a value, or a combined metric such as a utilisation.
- **Periodic snapshots written by an upper-layer job.** They lose what ADR-0084 decided: a metric computed on read over every row, retroactive by construction. A changed definition would not recompute the past.
- **Splitting at working hours or local midnight.** Calendars are a non-goal (PRD §3), and a UTC bucket is the one both backends compute alike (ADR-0106).
- **A window that selects whole pieces by their start**, as every other source's window selects rows. It is exact only to the bucket, so a month split read over the last 30 days would count one or two whole months. A split metric's rows are pieces of time, and a window is a span of time, so it clips them.
- **Keeping `.duration` the span's and adding `.piece_duration`.** Every existing metric over spans would need its value rewritten as well as its grouping to split. With the piece's `.duration`, a reader of a split metric reads a span's whole duration as `.left_at - .entered_at`, absent for a current span.

## Consequences

- `declaration-syntax.md` §6.9 gains the split source and the piece's members, and check 56 decision 7's refusals. `flow-format.md` §4.9 gains `split_by`, and the schema, the flow checker's step 3 and the text checker learn it.
- The reference runner computes pieces from the spans it already keeps, and its self-test proves a span cut at a month boundary, a current span cut at `now`, a window clipping a piece, and `count()` over pieces.
- A store computes the pieces on read. PostgreSQL can do it with `generate_series` over the buckets a span crosses, and SQLite with a recursive common table expression; both are unmeasured. `data-driven-engine.md` §7 gains the measurement, beside the metric latency it already owes.
- The journey's metrics and `pool_utilisation` change as decision 8 says.


# ADR-0158: What an event records of its rules, and how an export pages

- **Status:** Decided by the author, 2026-10-06 ("go with your recommendations on 1 to 3"), given three questions the core's read sets and export raised.
- **Date:** 2026-10-06
- **Refines:** ADR-0088 (the read set), ADR-0094 (the export by the settled cursor)
- **Relates to:** ADR-0049 and ADR-0084 (a consulted value recorded beside its guard); `DESIGN.md` §6, §7; `design/storage-schema.md` §4; `design/library-api.md` §4, §6; `design/metric-scenarios.md` §9, §16

## Context

Building the core's read sets and its export found three things the record did not fix.

- `storage-schema.md` §4 keys a consulted value "by the guard's name", and one guard can consult a metric more than once: `Delivery.yield_or_signed` reads `first_pass_rate` once for each unit's model, and `metric-scenarios.md` quotes each value by the metric and its bindings, `first_pass_rate(model: M-ARM)`.
- ADR-0088 records "what a type-scan matched", keyed by clause, and one guard can scan more than one type; nor did it say whether an object a scan's `where` examined and rejected was read.
- ADR-0094 pages an export by the settled cursor, and an interval row changes after it is written, its end set by a later event, so a cursor over rows by when each was first written would miss the change.

## Decision

1. **A consulted value is keyed by the guard's name, and within it by the reference with its bindings**: a metric as `<metric>(<dimension>: <value>, …)`, its bound values as the wire writes them, and an evaluator as `<evaluator>.<function>(<argument>, …)`. Its as-of time is kept per guard.
2. **A read set's scans are keyed by the guard and the type scanned**, each recording the objects that satisfied the scan's `where`. An object a scan examined and rejected is not recorded as read; an object whose member a rule read, and each element of a collection a rule aggregated, is.
3. **An export pages each row by the position of the event that last wrote it**, so a row written again, an interval given its end, comes again, and a reader keeps the latest by the row's key: an interval's object, dimension and entry, an attempt's id.

## Alternatives rejected

- **A consulted value keyed by the guard's name alone.** A guard consulting a metric for each of several objects would keep one value of many.
- **Keyed by the reference alone.** Two guards consulting the same metric would share one entry, and which guard decided on it would be lost.
- **A scan keyed by its clause alone.** A guard scanning two types would merge their matches.
- **Recording every object a scan examined.** A scan over a large type would record the whole type, which the matched set and serialisable isolation make needless for re-evaluation.
- **Paging an export by when each row was first written.** A row's later change, an interval's end, would never be exported.

## Consequences

- `storage-schema.md` §4 and `library-api.md` §4 and §6 state the keys and the export's order; `DESIGN.md` §6 says a scan records what satisfied its `where`.
- The core records each event's read set and consulted values, and an attempt row's, and serves `export`.
- Building it found that an evaluator stopping early records less than `DESIGN.md` §6 consults: in `count(s in second_sign_offs) >= 1 or all(… metric(first_pass_rate, …) …)` a sign-off decides, and the rate, which `metric-scenarios.md` §16 quotes on D14's record, would never be read. So a rule recording what it reads is evaluated whole, the side of an `and` or `or` that does not decide and an `all`'s elements after one fails evaluated for the record alone, its verdict the one stopping early gives and an evaluator it asked there unable to make the guard's remedy temporal. This follows §6's consultation of every metric a guard names, and is stated there.

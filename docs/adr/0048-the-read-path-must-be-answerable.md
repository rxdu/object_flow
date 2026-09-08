# ADR-0048: The read path must be answerable — sweepable guards, a per-attribute write index, and time-dependent predicates rewritten to their operands

- **Status:** Accepted — repair of D25, D26 and D30, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refines:** ADR-0037, ADR-0022

## Context

Three defects share one cause: the read surface promised operations whose cost nobody had bounded.

`available(type, transition)` returns every object for which a transition is available. It is not a convenience; ADR-0022 makes it the mechanism by which all time-driven work happens, and ADR-0043 by which lag is observed. As specified it is a loop over candidates evaluating arbitrary guards, so its cost is unbounded, and ADR-0037 calls it cheap.

`changed_since` reads the log, and every index in the design is on the object row. It runs inside guards, which run on every request and every availability listing.

Derived attributes cannot be queried at all: filters take indexed attributes, derived values are never stored, and a `now`-dependent one can never be indexed because it changes with no write. ADR-0022's consequences nonetheless have the automation layer polling exactly those, so overdue and low-stock alerts had no path.

## Decision

1. **A per-attribute last-written index.** Alongside each object row the store maintains, per attribute, the position of the event that last wrote it. It is derived from the log and updated in the writing transaction, so it can never disagree. `changed_since([attributes], event)` becomes a constant-time comparison instead of a history scan. *Refined by ADR-0057: each object that has parts carries one further position, the last event on any of its parts, so the predicate reaches a composition at the same cost.*
2. **Time-dependent predicates are rewritten to their operands.** A query does not evaluate `overdue`; it filters on the stored attribute the derived expression compares against `now`. `overdue := state == OUT and expected_return < now` is answered as a filter on the indexed `expected_return` with the current time supplied. Publishing reports which derived attributes are queryable this way and which are not.
3. **Derived attributes over stored, indexed attributes of the same object, with no aggregate and no `now`, are themselves indexable** and may be queried directly.
4. **A transition is `sweepable` or it is not, and publishing says which.** A sweepable transition's guards decompose into an indexable prefilter over stored attributes, state and category, plus a residual evaluated only on the prefiltered candidates. `available` runs the prefilter as a query. A transition that is not sweepable is refused by `available` rather than silently scanning a type.
5. **`available` never calls an external evaluator**, eager or deferred, and its result says which guards it did not evaluate. An eager evaluator in a sweep would be one third-party round trip per object.
6. **Cursor pagination orders by object id**, which is stable and total, rather than by the computed predicate.

## Alternatives rejected

- **Materialised views for derived attributes.** Rejected again, as in ADR-0037: a projection the declaration does not name is an unrecorded second writer.
- **Letting `available` scan.** Rejected: it makes an operation on the critical path of every scheduler O(type), and the failure appears in production rather than at publish.
- **Storing `changed_since` results.** Unnecessary once the last-written position is maintained; that is the smallest thing that answers the question.

## Consequences

- The storage schema gains the per-attribute write index, which is the second thing the log is folded into after the object row.
- A type author learns at publish which of their transitions can be swept and which of their derived attributes can be queried. That is the right time to learn it.
- A time-driven transition must be sweepable, or nothing can find its objects. This is a real constraint on how such guards are written and it is checkable.
- D25, D26 and D30 are resolved.

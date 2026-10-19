# ADR-0221: A metric's filter takes parameters

- **Status:** **Decided** 2026-10-19 under the author's direction ("address the two follow-ups in TODO"), the follow-up ADR-0218 decision 6 recorded.
- **Date:** 2026-10-19
- **Refines:** ADR-0218 decision 6 (a metric's filter takes none yet)
- **Relates to:** ADR-0218 (a query's filter takes parameters); `declaration-syntax.md` §6.9 (each source's rows); D569 (a metric's filter reads its rows through its item); issue #24

## Context

ADR-0218 let a query's filter state its values as parameters, `:name`, bound by the member each meets, so a caller states an absolute day as data instead of an offset from the engine's clock. It left a metric's filter without them: a query's operands are members of one type, and a metric's are fields of its rows, which differ by its source, an object, a span, an event, an attempt, a day's count of attempts, a label or an observation. Typing a parameter there needs those fields' types. So a report of the claims submitted in November, or of the refusals on one day, still wrote the day as an offset from `now`, with the two clocks of issue #24.

The core's metric typer (`Kind` in `metric.rs`) types a row's numbers and times, and leaves its text fields untyped, a transition's `.to_state` or an attempt's `.clause` among them, so it cannot say how to read a value compared with them.

## Decision

1. **`/v1/metric` takes `params`**, an object by name, and a metric's filter names them as a query's does, `:name`.
2. **Each parameter is bound as the field of the rows it meets is typed**, by `declaration-syntax.md` §6.9's table, through the metric's item: an object's members as its type declares them, with `.id`, `.state`, `.open`, `.created_at`, `.created_by_kind`, `.recorded_from` and `.declaration_version`; a span's `.object`, `.state` or a tracked member's `.value`, its times, `.duration`, `.period` and flags; an event's `.object`, `.transition`, `.from_state` and `.to_state`, its times, actor and flags; an attempt's and a day's count's fields; a label's own attributes, its subject's state as the text it records; an observation's fields, subject and times, as its kind's type declares them. A path through single references reaches each target's members in turn, so `t.object.engineer == :who` reads the id of a user; a query's operand reaches one reference away, and a row's starts a hop further back, at the row, whose object or subject is often the first.
3. **A field with no one type reads as nothing**, so a parameter compared with it is refused: an override's `.reason`, an enumeration's member or a text by the transition. `.held(<member>)` is not read either: a reader's filter names no bare member, so it cannot be written there (D569).
4. **The refusals are a query's** (ADR-0218 decision 3): a parameter `params` does not give, one given and never named, one compared with nothing that says how to read it, a value not of its field's type, and `params` without a filter are each refused `malformed_filter`, naming the parameter and never its value; `params` that is no object is `400`.
5. **The parameters are bound before any guard reads the filter**: its bounds, its literals' ranges, its rows-only reading and the personal guard read the bound filter, so a parameter on a personal member is personal, as the member is.
6. **A metric's page has no cursor**, so nothing carries the parameters, as a query's cursor must (ADR-0218 decision 5).

## Alternatives rejected

- **Typing a parameter by the metric typer's `Kind`**: it leaves text untyped, so a parameter compared with a transition's name, a state or an attempt's clause could not be read.
- **Typing a parameter by its JSON alone**: rejected for queries by ADR-0218, for the same reason; `"2026-11-09"` compared with a text field would read a date.
- **A metric's filter keeping offsets from `now`** for dates and instants: the two clocks of issue #24, for every report a caller dates.

## Consequences

- `reads.rs`: the query's binder, `bound_by`, takes how its operands read, a query's by its type (`bound`) and a metric's by its rows; `member_reads` reads an object's member for both.
- `metric.rs`: `row_reads` types an operand by its source's rows; `page_as` takes `params` and binds them before its guards. The service takes `params` on `/v1/metric`, and `wire-v1.json` lists it.
- `wire.md`'s metric filter says so.
- The Flow Studio's server takes neither a query's nor a metric's `params` yet (`TODO.md`), its reference runner binding none.
- Tests: `a_metric_filters_operands_read_as_its_rows_fields` (each source's fields, paths through references, and what reads as nothing), `a_metric_filters_parameters_are_read_as_its_rows_fields` (seventeen filters with parameters answering as the same filter written with the values does, over objects, spans, transitions, labels, observations and attempt counts; each refusal naming the parameter and never its value; a parameter on a personal member refused `personal`), and the service's `a_reads_parameters_are_taken_at_the_wire` (a query's and a metric's, at the wire, which no request tested for the query either); ten mutants of the final code killed, one of them a label's subject state typed as a state, a defect written and caught on the way, and one survivor, an observation's subject dropped, which showed the observation's own declaration already typed it, so the arm was removed.

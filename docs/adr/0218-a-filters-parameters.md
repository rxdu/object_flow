# ADR-0218: A query's filter takes parameters

- **Status:** **Decided** 2026-10-18 under the author's direction ("we need to address the issues properly so that they don't have to work around it"), for issue #24, which the operations platform's developer filed.
- **Date:** 2026-10-18
- **Refines:** ADR-0179 (a query's filter and its pages); ADR-0206 decision 2 (a read by a personal member opted into)
- **Refined by:** ADR-0221 — decision 6, a metric's filter takes parameters
- **Relates to:** `wire.md`, Filters and order; issue #24

## Context

A filter is the flow's expression language over the type's members, and the language has no date or instant literal: `day <= 2026-11-09` parses as arithmetic, `"2026-11-09"` is text, and `date("…")` reads no text. An application asking for the bookings between two days had to write each day as an offset from the engine's clock, `day <= date(now) + 22 days`, computed on its own clock and evaluated on the engine's, so the answer was right only while both agreed on the day, and silently another set of objects when not: a sandbox whose clock a test set, or a caller reckoning today in its own zone (issue #24, reproduced on 0.3.0). And every filter was built by formatting values into its text, each caller escaping quotes and backslashes itself.

## Decision

1. **A query's filter names parameters, `:name`, and its body gives their values in `params`**, an object by name. Only a read's filter parses `:name` (`expr::parse_filter`); a flow's expression does not.
2. **Each parameter is bound after the parse to the value its member's type reads it as**: compared with a member of the queried type, or one reference away, it is decoded as that member is declared, as a request's input of that type is decoded (`wire::decode`): a date as `2026-11-09`, an instant in RFC 3339, a reference by the object's id, text as itself; compared with the state, by a state's name; among the elements of a set a member is compared with, as that member. The bound value is a node of the tree, so the checks after the parse, the narrowing by an index and the evaluation read it as any literal.
3. **What cannot be bound is refused `malformed_filter`, naming the parameter and never its value**: a parameter `params` does not give, one given and never named, one compared with nothing that says how to read it or read elsewhere, and a value not of its member's type, the refusal naming the type. A value may be personal, so it is never said.
4. **The guards read a filter as the query does**: the personal guard and the cost guard parse its parameters too, so a parameter on a personal member is personal, where a filter that did not parse read as reading nothing personal.
5. **A cursor carries the parameters** with the filter, so a page continues only the read given the same values.
6. **A metric's filter takes none yet**: its rows' fields depend on its source, and typing a parameter there needs their table, a change of its own (`TODO.md`). *(Refined 2026-10-19 by ADR-0221: a metric's filter takes them, each bound by the field of the rows it meets.)*

## Alternatives rejected

- **Date and instant literals in the language** (`@2026-11-09`): fixes the two clocks and leaves every caller splicing and escaping text; they may still come, for filters written by hand in flows.
- **Typing a parameter by its JSON alone** (a string that looks like a date is a date): guesses, and a member of type `string` compared with `"2026-11-09"` would read a date.
- **Parameters in any expression, a flow's included**: a flow's values are its own, and the binding is the request's.

## Consequences

- `expr.rs`: `Expr::Param`, `Expr::Lit`, `parse_filter`. `reads.rs`: `bound`, `query_page` taking `params`, the guards parsing with `parse_filter`. The service and `wire-v1.json` take `params` on `/v1/query`.
- `wire.md`, Filters and order, says how; the sentence that there is no timestamp literal now says what offsets from `now` are for.
- Test: `a_filters_parameters_are_read_as_their_members`, on the issue's desk bookings, by day, instant, reference, a text holding quotes and a backslash, the state and a set, the same answer after the clock moves 400 days, each refusal, and the personal guard.

# ADR-0285: Text tests and a date's parts join the expression library

- **Status:** **Proposed** 2026-11-17, for the author to decide; recommended for acceptance, since both standards define these and each is exact. Written at the author's word that day ("go with your recommendations"), on the point put with a recommendation: whether to propose, as one ADR, a text's contains, starts and ends with, and a date's year, month, day and weekday.
- **Date:** 2026-11-17
- **Refines:** `declaration-syntax.md` §8.3 (*These are all the functions*)
- **Relates to:** `docs/research/growing-the-expression-library.md`; ADR-0249 (`trim`); ADR-0283 (`date`)

## Context

An expression outside a metric calls a closed list of functions, and a call of anything else does not type (check 24). Four of a consumer's issues each added one: `trim` (#63), a text that is never blank (#69), `add_months` (#96), and `whole` with `is_whole` (#97). Each was decided with research and built in both runners and both checkers, and each reached the deployment only in a release after it met the absence.

The research note compares the library with CEL's standard definitions and with FEEL's built-in functions, DMN's expression language. Two needs are met by both and not by ObjectFlow: whether a text contains, starts with or ends with another, and a date's year, month, day and weekday. Each is exact: it needs no rounding, no locale and no time zone.

Camunda's FEEL documentation gives a date the properties `year`, `month`, `day` and `weekday`, each a number, `weekday` from 1, Monday, to 7. In a metric, `year`, `month` and `day` already name time buckets (§6.9), which yield a bucket, not a number.

## Decision

1. **`contains(<string>, <string>)`, `starts_with(<string>, <string>)` and `ends_with(<string>, <string>)` yield a `bool`.** Each says whether the second text's code points occur in the first, anywhere, at its start or at its end. The comparison is exact: no case folding, no Unicode normalisation, no locale. An empty second text is contained in, starts and ends every text, as in CEL and FEEL. An absent argument gives `unknown`, as `length` does.
2. **A date has the members `year`, `month`, `day` and `weekday`, each an `int`.** `month` runs from 1, January, to 12; `day` is the day of the month; `weekday` runs from 1, Monday, to 7, Sunday, as ISO 8601 numbers it. A timestamp's are read through `date(<timestamp>)`, its UTC day (ADR-0283), so no zone enters. A member of an absent date gives `unknown`.
3. **Check 24 types each.** A text function given anything but two strings does not type. Nor does a member of a date other than these four, or one of these on a value that is not a date.
4. **An `indexed` derivation may not use them (check 46),** as it may not use `trim`, since the generated column its index is to be would compute them in SQL.
5. **A pattern match stays out**, as §8.3 says: a pattern's meaning differs between Python, Rust and each backend's SQL.

## Alternatives rejected

- **Adding each when an issue names it,** as before. A deployment meets each absence, then waits a release. These two are the needs both standards meet, so the likeliest next.
- **Functions `year(d)`, `month(d)`, `day(d)` and `weekday(d)`.** In a metric `year`, `month` and `day` name time buckets, so one name would have two meanings. A date's members follow a construct the language has, a file input's `media_type` and `size`, and FEEL's own properties.
- **A case-insensitive `contains`.** Unicode case folding differs by version and by locale, so the two runners and the two backends could disagree. A flow wanting it compares a copy it stores normalised.
- **`weekday` as a name, `"Monday"`.** A name is a language's; an `int` numbered as ISO 8601 numbers it is not.
- **`matches` with a regular expression.** CEL names RE2, and Python's and Rust's regular expressions differ from it and from each other, so both runners would need holding to one dialect. That is a decision of its own, for when a need names it.

## Consequences

- **What a flow meets.** A guard may read `starts_with(code, "INV-")`, and an invariant `date(created_at).weekday <= 5`. Each was a call or a member that did not type, so no module 0.29.0 accepted is refused.
- **What it costs.** Both runners and both checkers, plants through the core's oracle, §8.3, `flow-format.md` §7 and the guides, as each function before them.
- **If decided,** the status records who decided and when, and the work goes to `TODO.md`.

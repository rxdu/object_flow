# Growing the expression library ahead of demand

Which functions two established expression languages define, beside ObjectFlow's own, so the next additions can be planned rather than met one issue at a time: CEL's standard definitions and FEEL's built-in functions, DMN's expression language, with the functions a vendor adds told apart from the standard's. Asked 2026-11-17 at the author's word ("address the design weakness as you identified"), after a review of the application developer's issues found the library growing one function an issue.

## The question

ObjectFlow's expressions call a closed list of functions (`declaration-syntax.md` §8.3, *These are all the functions*), and a call of anything else does not type (check 24). Four of the application developer's issues each added one: `trim` (#63), a text that is never blank (#69), `add_months` (#96), and `whole` with `is_whole` (#97). Each was right for its need, decided with research and built in both runners and both checkers. The question is which needs are likely next, judged by what two established languages define, so each can be decided before a deployment meets its absence.

## Findings

### CEL: the standard definitions

CEL's language definition lists every function an implementation must provide: "All predefined operators, functions and constants are listed in the table below." Its standard environment groups them; among them:

- **Strings:** `size`, `contains`, `startsWith`, `endsWith` and `matches`. "Regular expressions follow the RE2 syntax."
- **Dates and times:** `getFullYear`, `getMonth`, `getDayOfMonth`, `getDayOfWeek`, `getDayOfYear`, and the parts of a time of day.
- **Conversions:** `int`, `uint`, `double`, `string`, `bool`, `bytes`, `duration`, `timestamp`.

Its arithmetic has a modulus of integers, `%`: "Compute the modulus of one integer into another." Absolute values, rounding, case and substrings are not among the standard definitions.

### FEEL: DMN's built-in functions

"FEEL is specified in the DMN specification of the Object Management Group (OMG)", and is "designed to write expressions in a way that is easily understood by both business professionals and developers." Camunda's documentation lists its built-in functions by group, and marks those that are Camunda's own as a *Camunda Extension*, not the standard's. Of the standard's:

- **Strings:** `substring`, `string length`, `upper case`, `lower case`, `substring before`, `substring after`, `contains`, `starts with`, `ends with`, `matches`, `replace`, `split`.
- **Numbers:** `decimal(n, scale)`, `floor` and `ceiling` with or without a scale, `round up`, `round down`, `round half up`, `round half down`, `abs`, `modulo`, `sqrt`, `log`, `exp`, `odd`, `even`.
- **Dates:** `date(year, month, day)`, `years and months duration(from, to)`, and among the temporal functions `today()`, `day of week`, `day of year`, `week of year`, `month of year`.

Camunda marks as its own extensions, among others, `trim`, `is blank`, `last day of month`, `get or else` and `is empty`.

## ObjectFlow beside them

Observed in `declaration-syntax.md` §8.3 and the expression parser at `fed7371`: an expression outside a metric calls `length`, `trim`, `date`, `whole`, `is_whole`, `entered_at`, `time_in` and `add_months`, and the member functions `intervals`, `entered_at`, `time_in` and `held`; its arithmetic is `+`, `-`, `*` and `/`. An attribute may be marked `not_blank`. §8.3 names `abs`, `round`, `upper` and `coalesce` as calls that do not type.

| Need | CEL standard | FEEL (DMN) | Camunda extension | ObjectFlow |
|---|---|---|---|---|
| a text's length | `size` | `string length` | | `length` |
| a text without its surrounding spaces | | | `trim` | `trim` |
| a text that is blank | | | `is blank` | `not_blank` marking |
| a text contains, starts or ends with another | `contains`, `startsWith`, `endsWith` | `contains`, `starts with`, `ends with` | | none |
| a text matches a pattern | `matches`, RE2 | `matches` | | none |
| a text's case, or a part of it | | `upper case`, `lower case`, `substring` | | none |
| a date's year, month, day or weekday | `getFullYear`, `getMonth`, `getDayOfMonth`, `getDayOfWeek` | `day of week`, `month of year`, … | `last day of month` | none |
| a date months later | | | | `add_months` |
| the months between two dates | | `years and months duration` | | none |
| a decimal rounded, its mode named | | `decimal`, `floor`, `ceiling`, `round half up`, … | | none, by design |
| a decimal as an integer | `int` | | | `whole`, `is_whole` |
| an absolute value, a modulus | `%`, of integers | `abs`, `modulo` | | none |
| a value, or another where it is absent | | | `get or else` | `if … is null then … else …` |

## What it suggests

This is a recommendation for the author, not a decision; each addition below would be decided on its own, with an ADR, both runners and both checkers.

- **Inferred:** the needs both standards meet and ObjectFlow does not are the likeliest next: a text containing, starting or ending with another, and a date's year, month, day and weekday. Both are exact, need no rounding and no locale, and a guard or an invariant reads them as it reads `length`.
- **Inferred:** a pattern match is worth its cost only with one dialect both runners hold alike. CEL names RE2; ObjectFlow's runners are Python and Rust, whose regular expressions differ, so the dialect would be chosen and both runners held to it.
- **Inferred:** rounding stays where §8.3 puts it, permitted where it is declared. If a need arrives, FEEL's shape fits it: the mode named at each use, `round half up(n, scale)`, as `add_months` names its rule.
- **Inferred:** ObjectFlow's `trim` and `not_blank` match what a vendor added to FEEL, `trim` and `is blank`, not the standard: a sign that the standards lag what applications ask, so the table is a floor for planning, not a ceiling.

## Sources

| Source | URL | Retrieved | Version |
|---|---|---|---|
| CEL language definition, `google/cel-spec` `doc/langdef.md` | https://raw.githubusercontent.com/google/cel-spec/master/doc/langdef.md | 2026-11-16 | branch `master`, last changed by commit `21ec6f4e52e8` (2026-10-27) |
| Camunda 8 docs, *What is FEEL?* | https://docs.camunda.io/docs/components/modeler/feel/what-is-feel/ | 2026-11-17 | the page as served; it gives no date |
| Camunda 8 docs, FEEL built-in functions: conversion, boolean, string, numeric, list, context, temporal, range | https://docs.camunda.io/docs/components/modeler/feel/builtin-functions/feel-built-in-functions-introduction/ and its sibling pages | 2026-11-17 | the pages as served; they give no date |

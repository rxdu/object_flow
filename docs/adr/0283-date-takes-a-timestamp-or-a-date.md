# ADR-0283: `date` takes a timestamp or a date

- **Status:** **Decided** by the author 2026-11-17 ("go with your recommendations"), on the point put 2026-11-16 for the finding that `date(…)` types any argument, with a recommendation. Built the same day.
- **Date:** 2026-11-17
- **Refines:** ADR-0145 (`date(<timestamp>)`); `declaration-syntax.md` §8.3's *Dates*; `flow-format.md` §7's rule for a date
- **Relates to:** ADR-0249 (`trim`), ADR-0276 (`add_months`) and ADR-0278 (`whole`), each of whose arguments check 24 types; `docs/research/typing-a-shared-condition-at-each-use.md`

## Context

`date(<timestamp>)` is the UTC calendar day of an instant (ADR-0145). Check 24 types each other function's argument, `trim(5)` refused, "trim takes a string, and is given a number", but gives `date(…)` the type `date` without reading its argument (`check-flows.py`'s typing of a call, `check/names.rs`).

Found 2026-11-14 by #96's specialist look, and reproduced 2026-11-16 at `87a6ae8` on both checkers, the core's through `/sandbox/load`: `date(title)`, `date(this.title)`, `date(5)` and `date(true)`, `title` a string, each load clean. Run, the core answers `date` of a text `501 not_run`, "the date of a text". The reference runner raises `TypeError` on it, and reads `date(5)` as five seconds past the epoch, since it holds a timestamp as a number (`flowrun.py`). Both runners answer an absent argument, a date and a timestamp alike.

The research note finds CEL's checker resolving a function's overloads before it runs, so that `no_matching_overload` is found "ahead of runtime".

## Decision

1. **`date` takes a timestamp or a date.** Of a timestamp it is the UTC calendar day, as ADR-0145 decides, and of a date that date. An absent argument gives absence, as before.
2. **Check 24 refuses any other argument whose type it can tell**: "date takes a timestamp or a date, and is given a string". Where it cannot tell the type, a runner stops the request as not run, as for `add_months` (ADR-0278 decision 3).
3. **The reference runner answers any other value not run, as the core does.** It holds a timestamp as a fraction, as it holds a decimal, an amount and a duration, so an `int`, text and a bool are never one. A fraction is one where the argument is held as one or its kind cannot be told, as `whole` tells a decimal by the kind it is held at (ADR-0278).

## Alternatives rejected

- **A number as seconds since the epoch**, the reference runner's reading. The core does not run it, and a number has no unit, so `date(5)` would name a day by accident.
- **Leaving it.** A module the checks pass stops every request its guard reaches, and crashes the reference runner.

## Consequences

- **What a flow meets.** `date` of a string, a number, a bool or any other type check 24 can tell is refused at step 3. A module 0.28.0 accepted may be refused: one whose guard the core cannot run.
- **The code:** `check-flows.py`'s typing of a call and `check/names.rs`'s alike; `flowrun.py`'s `date`.
- **The docs:** `declaration-syntax.md` §8.3's *Dates* and `flow-format.md` §7 name `date(<date>)` and refuse any other argument.
- **Tested** by cases added to the reference's planted checks, which the core's oracle replays: `date` of a string attribute, of one read through `this`, of a number, of an `int` attribute and of a bool, each refused; and of `now`, a timestamp attribute and a date attribute, each clean. The reference runner's self-test answers `date(now)` a date, and `date` of an `int` attribute, an `int` literal, a decimal literal, a duration literal, and inputs of no told kind holding text and an `int` not run. The Studio's test loads, with the checks skipped, guards reading the date of an `int` input and of a text input, which both runners answer not run alike.

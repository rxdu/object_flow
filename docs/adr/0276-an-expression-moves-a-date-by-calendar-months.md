# ADR-0276: An expression moves a date by calendar months

- **Status:** **Decided** by the author 2026-11-14 ("go with your recommendations"), on the four points put the same day for issue #96, each with a recommendation. Built the same day.
- **Date:** 2026-11-14
- **Refines:** `declaration-syntax.md` §8.3 (*Units*, *Dates* and its list of functions); ADR-0164 decision 2 and ADR-0255 decision 4 (what a kind's and a value's invariant may call)
- **Relates to:** ADR-0249 (`trim`, refused in an indexed derivation); ADR-0205 decision 2 and ADR-0190 decision 1 (a value past its type's bound refuses); PRD F3, UC-21 and UC-12; `docs/research/adding-calendar-months-to-a-date.md`; issue #96

## Context

Issue #96, filed 2026-11-14 at 0.25.0, found that a flow cannot compute a date a whole number of calendar months after another. A warranty or a support plan runs for whole months from its start, so its end date is computed outside the engine and passed in. The engine then holds that the end comes after the start, and not that it is the length stated, which a writer computing it holds for its own writes alone.

Reproduced 2026-11-14 at `94a1ff7` on both checkers, on a flow of the repository's own: a `Plan` guarded by `inputs.ends_on == add_months(inputs.starts_on, inputs.months)` is refused, "add_months is no function the language has" (check 24). Thirty days a month loads clean, and takes 2026-03-02 as one month after 2026-01-31.

A duration has no months, "because neither has a fixed length and a guard that silently changes meaning in February is worse than one that cannot be written" (§8.3, *Units*). The issue keeps that reason. It asks for `add_months(<date>, <int>)`, a day the target month lacks becoming that month's last day, as PostgreSQL gives.

The research note finds two rules among twelve platforms. PostgreSQL, Java, .NET, chrono, dateutil, Temporal's default and XML Schema keep the day of the month and take the target month's last day where it lacks the day. Oracle, Snowflake's `ADD_MONTHS` and Salesforce also keep a start on a month's last day on the last day. Spark dropped that rule in 3.0, and SQLite moves into the next month by default.

## Decision

1. **`add_months(<date>, <int>)` yields the `date` moved by that many calendar months**, back for a negative count. It is a function, and the duration units stay as §8.3 states them. Years are twelve months.
2. **The day of the month is kept where the target month has it, and is that month's last day where it does not.** A start on a month's last day moves as any other: 2026-04-30 plus a month is 2026-05-30. The design states that a move twice need not equal one by the sum, and a move back need not undo a move, so a rule computes an end from its start.
3. **It may be called anywhere an expression computes a value**: a guard, an effect, a derived attribute, and an invariant of a type, an observation kind or a value, since it reads its arguments alone. A rule such as `ends_on == add_months(starts_on, months)` is then an invariant every writer meets. **An `indexed` derivation may not call it** (check 46). The generated column its index is to be (`storage-schema.md` §3.4) would compute it in SQL, where SQLite moves a day a month lacks into the next month and PostgreSQL makes a timestamp.
4. **It takes a date and an `int` alone** (check 24). A timestamp has a month only in a zone, and `add_months(date(now), 1)` moves the UTC day.
5. **An absent argument gives `unknown`**, as `length` does. **A date moved outside the calendar**, 0001-01-01 to 9999-12-31, has no value and refuses as `value_in_range` wherever it is evaluated, as a date moved by days does (§8.2, ADR-0205 decision 2). Each runner refuses so itself, since a call's result does not pass through the binary operators' check.

## Alternatives rejected

- **Keeping a start on a month's last day on the last day**, as Oracle, Snowflake and Salesforce do. It moves days the target month has, 2026-04-30 to 2026-05-31 and 2027-02-28 to 2027-03-31. A month-end anchor can come later as a function of its own, changing no published flow, where changing this rule would change them.
- **Moving into the next month**, SQLite's default. The end would leave the month the count names.
- **No value where the day is missing**, Temporal's `reject`. A plan starting on the 31st would have no end in each shorter month.
- **A `months` duration unit.** A duration would have no fixed length, which §8.3 refuses.
- **A calendar-duration type**, as XML Schema's year-month duration. A new type, with its own arithmetic and comparisons, for one use.
- **`unknown` past the calendar.** An invariant held under it and admitted the write it should refuse (ADR-0205).
- **Allowing it in an indexed derivation now.** Each backend's SQL would need proving against both runners, for no case yet.

## Consequences

- **The code:** `value.rs`'s `add_months`; `eval.rs`'s call; `check/names.rs`'s functions, their arity, its typing and check 46; `check/data.rs`'s check 54; `check/raw.rs`'s value invariants; `request.rs`'s reach of an invariant's calls; `metric.rs`'s type of a call. The reference's `flowrun.py`, `check-flows.py` and `check-syntax-doc.py`. Both checkers' message for an unknown function names `trim` too, which it left out since ADR-0249.
- **The docs:** `declaration-syntax.md` §8.3, check 46's and check 54's rows and §9.5's reserved words; `flow-format.md` §4.11 and §4.20.
- **What a flow meets:** a module 0.25.0 accepted reads as before. A name `add_months` stays usable as a member's name (§9.2).
- **Tested** by `value.rs`'s `a_date_moves_by_months_keeping_its_day_or_the_months_last`, every day of the calendar against the rule; `request.rs`'s `a_date_moved_by_months_holds_a_plans_length`, on both backends; plants both checkers agree on through the core's oracle; and the Studio's test through both runners alike.

# Adding calendar months to a date

**The question.** Issue #96, filed 2026-11-14 at 0.25.0, asks for a function that moves a date by a whole number of calendar months. A warranty or a support plan runs for whole months from its start. Today its end date is computed outside the engine and passed in, so the engine holds that the end comes after the start, and not that it is the length stated. The issue asks for `add_months(<date>, <int>)`, with a stated rule for a day the target month lacks: that month's last day, as PostgreSQL gives. This note asks how established systems add months to a date: what they do with a day the target month lacks, whether a start on a month's last day is treated apart, and what they say of order and of the calendar's edges.

## Findings per source

### PostgreSQL: the day kept, else the month's last day

PostgreSQL adds an interval to a date: "a nonzero months field advances or decrements the date of the timestamp by the indicated number of months, keeping the day of month the same unless it would be past the end of the new month, in which case the last day of that month is used. (For example, March 31 plus 1 month becomes April 30, but March 31 plus 2 months becomes May 31.)" Its operator table types `date + interval` as a `timestamp`.

### SQLite: the next month by default, the month's last day by a modifier

SQLite's `date()` takes a `'+N months'` modifier and names the problem: "what is the date that is two months after 2023-12-31? Is it 2024-02-29 or 2024-03-02? There is no consensus on how to resolve this ambiguity, so the "ceiling" and "floor" modifiers (14 and 15) are available to let the programmer decide." "The "floor" modifier resolves ambiguities by resolving to the last day of the previous month. The default behavior is "ceiling"." The modifiers came in 3.46.0: "New modifiers 'ceiling' and 'floor' control the algorithm used to resolve ambiguous dates when shifting a date by an integer number of months and/or years."

### Oracle and Snowflake: a month's last day kept as the last day

Oracle's `ADD_MONTHS` treats a start on a month's last day apart: "If date is the last day of the month or if the resulting month has fewer days than the day component of date, then the result is the last day of the resulting month. Otherwise, the result has the same day component as date."

Snowflake keeps the two rules apart, by function: "For both ADD_MONTHS and DATEADD, if the result month has fewer days than the original day, the result day of the month is the last day of the result month. For ADD_MONTHS only, if the original day is the last day of the month, the result day of month will be the last day of the result month."

### Salesforce: a month's last day kept, in a formula

Salesforce's formula function `ADDMONTHS`: "If the specified date is the last day of the month, the resulting date is the last day of the resulting month. Otherwise, the result has the same date component as the specified date." Its example: "If the start date is September 30, 2017, the resulting date is February 28, 2018."

### Spark: the last-day rule dropped in 3.0

Spark's migration guide: "In Spark 3.0, the add_months function does not adjust the resulting date to a last day of month if the original date is a last day of months. For example, select add_months(DATE'2019-02-28', 1) results 2019-03-28. In Spark version 2.4 and below, the resulting date is adjusted when the original date is a last day of months."

### Java, .NET, Rust's chrono and Python's dateutil: the day kept, else the month's last day

- **Java**'s `LocalDate.plusMonths`: "Adjust the day-of-month to the last valid day if necessary. For example, 2007-03-31 plus one month would result in the invalid date 2007-04-31. Instead of returning an invalid result, the last valid day of the month, 2007-04-30, is selected instead." It throws `DateTimeException` "if the result exceeds the supported date range".
- **.NET**'s `DateTime.AddMonths`: "If the resulting day is not a valid day in the resulting month, the last valid day of the resulting month is used."
- **chrono**'s `NaiveDate::checked_add_months`: "Uses the last day of the month if the day does not exist in the resulting month." It "Returns None if the resulting date would be out of range."
- **dateutil**'s `relativedelta`: "If the result falls on a day after the last one of the month, the last day of the month is used instead."

### Temporal: the day constrained by default, or refused

JavaScript's Temporal makes the rule an option of `PlainDate.add`: "In 'constrain' mode (the default), out-of-range values are clamped to the nearest in-range value. In 'reject' mode, an addition that would result in an out-of-range value fails, and a RangeError is thrown."

### XML Schema 1.1: the day pinned, and the order significant

XML Schema's algorithm for adding a duration to a date: "If the day is out of range for the new month value, it is pinned to be within range. Thus April 31 turns into April 30." It states that order matters: the addition "differs from addition on integers or real numbers in not being commutative. The order of addition of durations to instants is significant."

## What the sources share, and where they part

- **Every source moves the month and keeps the day where the target month has it.**
- **A day the target month lacks** becomes that month's last day in PostgreSQL, Java, .NET, chrono, dateutil, Temporal's default, XML Schema, Oracle and Snowflake. SQLite alone moves into the next month by default, and takes the last day by its `floor` modifier since 3.46.0. Temporal alone offers to refuse the date instead. Salesforce's page states the case only for a start on a month's last day.
- **A start on a month's last day** stays on the last day in Oracle, Snowflake's `ADD_MONTHS` and Salesforce. PostgreSQL, Java, .NET, chrono, dateutil, Temporal and XML Schema keep its day of the month, and Spark dropped the last-day rule in 3.0.
- *Inferred*, from Oracle's stated rule: under the last-day rule, a start on 28 February moves to 31 March in a common year, where it is the month's last day, and to 28 March in a leap year, where it is not.
- **Moving twice is not moving once by the sum.** XML Schema says so. *Inferred* from PostgreSQL's own example: March 31 plus a month is April 30, and April 30 plus a month is May 30, where March 31 plus two months is May 31.
- *Derived*, under the rule that keeps the day or takes the month's last day: **a move is not undone by the move back.** 2026-03-31 plus a month is 2026-04-30, and 2026-04-30 less a month is 2026-03-30.
- **Past the calendar's edge**, Java throws and chrono answers none.

## ObjectFlow beside the sources

- **A duration has no months.** Its units are a closed set, "with no months or years, because neither has a fixed length and a guard that silently changes meaning in February is worse than one that cannot be written" (`declaration-syntax.md` §8, *Units*). A date moves by days or weeks alone (*Dates*), and the language's functions are `length`, `trim`, `date`, `entered_at` and `time_in` (§8).
- **Reproduced 2026-11-14 at `94a1ff7` (0.25.0)**, on a flow of the repository's own: a `Plan` with a `starts_on` date, a `months` int and an `ends_on` date, its creation guarded by `inputs.ends_on == add_months(inputs.starts_on, inputs.months)`. Both checkers refuse it, "add_months is no function the language has" (check 24), the core's through `/sandbox/load`. Thirty days a month loads clean on both, and takes 2026-03-02 as one month after 2026-01-31.
- **A date is a day count**, days since 1970-01-01 in the core (`value.rs`'s `days_from_civil` and `civil_from_days`) and a `datetime.date` in the reference runner, both in the calendar 0001-01-01 to 9999-12-31. A date moved past it has no value (ADR-0190 decision 1).
- **SQL would compute one place an expression is written**: the design makes an `indexed` derivation a generated column (`storage-schema.md` §3.4), which is why `trim` may not be one (ADR-0249 decision 4). As built, neither backend makes one, and a query computes the derivation for each object. Observed 2026-11-14 on the system's SQLite 3.37.2, which Python links: `date('2026-01-31','+1 month')` is `2026-03-03`, and `'floor'` is not taken there. The core bundles its own SQLite (`core/Cargo.toml`, rusqlite's `bundled`), 3.54.1, which takes `floor`. PostgreSQL's `date + interval` is a timestamp.
- **An arithmetic past its type's bound has no value**, and refuses the request as `value_in_range` wherever it is evaluated, a guard, an invariant, a derivation or an effect, and a read of the object (`declaration-syntax.md` §8.2, ADR-0205 decision 2). Observed 2026-11-14 at `94a1ff7`: an effect writing a date moved past the calendar is refused so, and nothing is written, and a `get` of an object deriving one is refused.
- **A rule every writer meets** is an invariant: a type's, a kind's (check 54) or a value's (ADR-0255), which reads the record's own fields and calls `length` or `trim` alone today.

## What it suggests

A recommendation for the author, not a decision.

1. **A function, `add_months(<date>, <int>)`, yielding a `date`**, and no duration unit of months. The name is Oracle's, Snowflake's and Spark's. The duration units stay as §8 states them.
2. **The day kept where the target month has it, and that month's last day where it does not**, a start on a month's last day treated as any other. That is the majority's rule, and the one the issue asks for; Spark dropped the last-day rule in 3.0. Under it, a day the target month has is never moved: every start on the 28th or before keeps its day. The last-day rule moves days the target month has, 2026-04-30 to 2026-05-31 and 2027-02-28 to 2027-03-31. A month-end anchor could come later as a function of its own, with no published flow changed, where moving from one rule to the other would change them.
3. **The order and the inverse stated**: moving twice by a month need not equal moving once by two, and a move back need not undo a move, so a rule computes the end from the start, never the start from the end.
4. **Negative months move back.** An absent argument gives `unknown`, as `length` gives. A date moved past the calendar has no value, and refuses the request as `value_in_range`, as a date moved by days does.
5. **A date alone**: a timestamp has a month only in a zone, and `add_months(date(now), 1)` names the UTC day.
6. **Anywhere an expression computes a value, but an indexed derivation**, whose generated column, once built, would need SQLite's `floor` and PostgreSQL's interval to agree with the runners. That includes the invariants of a type, of an observation kind and of a value, so the length holds for every writer; the last two call `length` or `trim` alone today (ADR-0164, ADR-0255).

## Sources

Each page was fetched with `curl -sL` on 2026-11-14 and its markup stripped, except Salesforce's, which a script draws: it was rendered by a headless Chromium on 2026-11-14 and its text read the same way.

| Source | URL | Version |
|---|---|---|
| PostgreSQL, date/time functions | https://www.postgresql.org/docs/current/functions-datetime.html | 18 |
| SQLite, date and time functions | https://sqlite.org/lang_datefunc.html | current on 2026-11-14 |
| SQLite, release 3.46.0 | https://sqlite.org/releaselog/3_46_0.html | 3.46.0, 2024-05-23 |
| Oracle, `ADD_MONTHS` | https://docs.oracle.com/en/database/oracle/oracle-database/26/sqlrf/ADD_MONTHS.html | 26 |
| Snowflake, `ADD_MONTHS` | https://docs.snowflake.com/en/sql-reference/functions/add_months | current on 2026-11-14 |
| Salesforce, `ADDMONTHS` | https://help.salesforce.com/s/articleView?id=platform.customize_functions_addmonths.htm&type=5 | current on 2026-11-14 |
| Spark, SQL migration guide | https://spark.apache.org/docs/latest/sql-migration-guide.html | 4.3.0 |
| Java, `LocalDate` | https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/time/LocalDate.html | Java SE 21 |
| .NET, `DateTime.AddMonths` | https://learn.microsoft.com/en-us/dotnet/api/system.datetime.addmonths?view=net-9.0 | .NET 9 |
| Rust chrono, `NaiveDate` | https://docs.rs/chrono/latest/chrono/naive/struct.NaiveDate.html | 0.4.45 |
| Python dateutil, `relativedelta` | https://dateutil.readthedocs.io/en/stable/relativedelta.html | 4.0.0 |
| Temporal, `PlainDate` | https://tc39.es/proposal-temporal/docs/plaindate.html | proposal, current on 2026-11-14 |
| XML Schema 1.1 Part 2 | https://www.w3.org/TR/xmlschema11-2/ | W3C Recommendation 5 April 2012 |

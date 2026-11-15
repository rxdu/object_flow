# ADR-0278: A whole decimal is used as an `int`, and tested whole

- **Status:** **Decided** by the author 2026-11-15 ("go with your recommendations"), on the first three of four points put the same day for issue #97, each with a recommendation. Built the same day.
- **Date:** 2026-11-15
- **Refines:** `declaration-syntax.md` §8.2 (what has no value) and §8.3 (its list of functions); ADR-0205 decision 2 (an arithmetic past its bound refuses wherever it is evaluated); ADR-0164 decision 2 and ADR-0255 decision 4 (what a kind's and a value's invariant may call)
- **Relates to:** ADR-0277 (a division by zero through a derivation, the fourth point); ADR-0276 (`add_months`, refused in an indexed derivation alike); ADR-0249 (`trim`); PRD F3 and UC-21; `docs/research/using-a-whole-decimal-as-an-integer.md`; issue #97

## Context

Issue #97, filed 2026-11-14 at 0.25.0, found that a decimal that is a whole number cannot be used where an `int` is needed. A sales line's quantity counts whole units of a product and may be fractional for time sold. As a `decimal`, it cannot drive the loop that opens a placeholder per unit, whose `range` takes an `int`, and no expression can say it has no fraction. The language has no conversion from a decimal to an `int` and no rounding, by design: rounding is "permitted exactly where it is unavoidable and refused where it would be silent loss" (§8.3).

Reproduced 2026-11-15 at `b5ecfab` on both checkers, on a flow of the repository's own: `range: inputs.quantity` is refused, "loops over a range of decimal, and a range is an int", and `whole(inputs.quantity)`, "whole is no function the language has" (check 24).

The research note finds that a conversion dropping a fraction silently is the norm: .NET, SQL Server, XPath, rust_decimal and Python's `int` truncate, and PostgreSQL's cast rounds. Java alone converts exactly, throwing on a fraction. .NET, rust_decimal and JavaScript test wholeness with a function of its own. Dynamics 365, Odoo and SAP hold one decimal quantity and the precision its product or unit allows.

The issue asked for `whole(<decimal>)`, with no value for a fraction. #97's specialist look found that the no value of a division by zero, which an effect refuses, reached an effect through a derived attribute as plain absence (ADR-0277): `derive units = whole(quantity)` with `range: units` would have opened no unit for 1.5.

## Decision

1. **`whole(<decimal>)` yields the `int` a decimal equals where it has no fraction**, whatever its scale. **A decimal with a fraction, or a whole one past 64 bits, has no value, and refuses wherever it is evaluated**, as an arithmetic past its bound does (ADR-0205 decision 2): a guard, an invariant, a derivation a request reads, a loop's filter and an effect refuse the request as `value_in_range`, `unreachable_from_here`, and a read of the object, a metric's among them, is refused `value_in_range`. The side of an `and`, an `or` or an `implies` that does not decide refuses nothing, as for an overflow.
2. **`is_whole(<decimal>)` yields whether a decimal has no fraction, and never refuses.** It is the test, so a rule names wholeness: `by_time or is_whole(quantity)` is an invariant every writer meets.
3. **Each takes a decimal of any scale alone.** An `int`, an amount or a duration given either does not type (check 24), and neither does `whole` of a literal with a fraction, as a division by a literal zero does not. Where check 24 cannot tell an argument's type, a runner stops the request as not run, as for `add_months` given a timestamp. An absent argument gives `unknown`, as `length` does.
4. **Either may be called anywhere an expression computes a value**: a guard, an effect, a derived attribute, a metric, and an invariant of a type, of an observation kind or of a value. **An `indexed` derivation may call neither** (check 46): its generated column (`storage-schema.md` §3.4) would compute it in SQL, where PostgreSQL's cast to an integer rounds.

## Alternatives rejected

- **No value, as a division by zero**, the issue's shape: `unknown` where a rule reads it, so `whole(q) is not null` would test wholeness, and refused where an effect computes it. A division by zero read through a derived attribute reached an effect as plain absence (ADR-0277), and an invariant comparing `whole(q)`, as `whole(q) <= 10`, would hold on 10.5, since an invariant reading `unknown` holds. ADR-0205 refused `unknown` for an overflow for that reason, and ADR-0276 for a date past the calendar.
- **Truncation or rounding**, as .NET, SQL Server, XPath, rust_decimal and Python's `int` truncate and PostgreSQL's cast rounds: silent loss, which §8.3 refuses.
- **A test alone, and `range` taking a whole decimal.** It changes `range`'s type, and leaves no way to write a whole decimal to an `int` attribute or input.
- **A precision declared per product or unit**, as Dynamics 365's *Decimals Supported*, Odoo's and SAP's units of measure. The engine knows no product or unit, and which lines are whole is the flow's own rule, which an invariant with `is_whole` states.
- **`int(<decimal>)`**, after `date(<timestamp>)`: `date` drops the time of day, and the casts above truncate or round, so a reader would take `int(2.5)` for 2. **`int_exact`**, after Java's `intValueExact`: longer, and new to every reader but Java's.
- **Taking an `int` too**, yielding it unchanged: it hides a call that does nothing.

## Consequences

- **What it costs.** Two functions, not one. A derived attribute `whole(quantity)` over values some of which have a fraction refuses each read of a fractional one, so it is written `if by_time then 0 else whole(quantity)`; a conditional evaluates the branch it takes alone. A flow that never tests with `is_whole` refuses a fraction as `value_in_range`, not under a name of its own.
- **The code:** `value.rs`'s `Decimal::is_whole`; `eval.rs`'s calls; `check/names.rs`'s functions, a dispatcher typing `length`, `trim`, `add_months`, `whole` and `is_whole` through one arm of the typing's recursion, since an arm of its own each grew its frame past the oracle's stack (`docs/LESSONS.md`), and check 46; `check/data.rs`'s check 54; `check/raw.rs`'s value invariants; `request.rs`'s reach of an invariant's calls; `metric.rs`'s type of a call. The reference's `flowrun.py`, which holds a decimal, an amount, a duration and a time each as a fraction and tells them apart by the type it infers, `check-flows.py` and `check-syntax-doc.py`.
- **The docs:** `declaration-syntax.md` §8.2, §8.3, check 46's and check 54's rows and §9.5's reserved words; `flow-format.md` §4.11 and §4.20; `DESIGN.md` §5.7's list of what the language holds.
- **What a flow meets:** a module 0.26.0 accepted reads as before. A name `whole` or `is_whole` stays usable as a member's name (§9.2).
- **Tested** by `value.rs`'s `a_decimal_is_whole_at_any_scale_and_an_int_within_64_bits`; `request.rs`'s `a_whole_decimal_counts_units_and_a_fraction_refuses`, on both backends; `whole_plants` (thirteen cases), which the core's oracle agrees with; and the Studio's `test_a_whole_decimal_counts_units_and_a_fraction_refuses_alike` through both runners, a sum, an average and a request stopped as not run among it; and twenty-two planted defects across both runners and both checkers, each caught.

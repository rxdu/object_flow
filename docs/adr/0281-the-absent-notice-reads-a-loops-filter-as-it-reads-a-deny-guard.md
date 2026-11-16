# ADR-0281: The `absent` notice reads a loop's filter as it reads a deny guard

- **Status:** **Decided** by the author 2026-11-16 ("go with your recommendations"), on the point put the same day for issue #99, with a recommendation. Built the same day.
- **Date:** 2026-11-16
- **Refines:** ADR-0209 decision 5 (the narrower `absent` notice); `flow-format.md` §7's `absent`
- **Relates to:** ADR-0201 decision 7 (an absent value refuses an assignment, D655); `docs/research/reading-a-filters-test-of-an-optional.md`; issue #99

## Context

Step 3's notice `absent` marks an assignment whose value can be absent through an optional value it never tests, since an absent value refuses the request `value_in_range` (ADR-0201 decision 7). It counts a value as tested where the assignment itself, or a `deny` guard of its transition, tests it with `is null` or `is not null`. Each checker reads what a guard tests through one walk of its expression, which counts any such test it holds, and passes those tests to every step of the effect.

Issue #99, filed 2026-11-16 at 0.27.0, found that a loop's `where` is not read so. Observed the same day at `e9b83ba` on both checkers: an assignment reading an optional `model.months` inside `foreach: { range: 1, where: "model.months is not null", … }` is noticed, and the same assignment behind a `deny` guard testing `model.months` is not. So is one inside a loop nested within the filtered one, one under the filter `not (model.months is null)`, and one reading `p.months` in a loop over a set of references filtered `p.months is not null`. The core, run on the issue's flow, received a unit whose model has no months without running the step, and gave one whose model has twelve months its date. A step inside a loop runs only where the filter holds (`flow-format.md` §4.8), as the effect runs only where every `deny` guard holds.

The research note finds TypeScript's control flow analysis narrowing a value at each test it meets, and, from 5.5, following a filter's test into the elements it keeps, of code it had flagged: "This code is perfectly fine: we've filtered all the undefined values out of the list."

## Decision

1. **The `absent` notice reads a loop's `where` as it reads a `deny` guard**, through the same walk. What the filter tests counts as tested for that loop's steps and for the loops within it, and not for a step after the loop.
2. **Any test of the read counts, as a guard's does.** A filter `model.months is null` counts as testing `model.months`, as a guard reading it does.

## Alternatives rejected

- **Only a filter's `is not null` and a negated `is null`**, the issue's reading. A filter would then read a test unlike a guard, two rules where one serves. The notice refuses nothing, and a guard's reading counts any test.
- **Leaving it.** A flow that writes a conditional step as a loop of one, as #99's does, keeps a notice it cannot clear.

## Consequences

- **What a flow meets.** A notice is gone where a loop's filter tests what an assignment within it reads. It is a notice, so no module is newly refused or newly accepted.
- **The code:** `check-flows.py`'s `absents`, and `check/notices.rs`'s `assigned`; the notice's words name a loop's filter.
- **The docs:** `flow-format.md` §7's `absent`.
- **Tested** by eight cases added to the reference's planted checks of the notice, which the core's oracle replays: a filtered loop, a loop within one, a negated filter, a filter testing for absence and a filter on the loop's item, none noticed; and a filter testing another value, the item unfiltered and a step after a filtered loop, each still noticed.
- **What it leaves.** A loop of one filtered on a value is how #99's flow writes a step that runs where the value is present, which `DESIGN.md` §5.4's straight-line effect does not otherwise say. Whether §5.4 endorses that, or the language gains a straight-line spelling, is tracked in `TODO.md`.

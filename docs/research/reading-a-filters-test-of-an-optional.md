# Reading a filter's test of an optional value

How a checker follows a test that a value is present into the code that runs only where the test holds: TypeScript's control flow analysis narrows a variable's type at each test it meets, and from 5.5 follows a filter's test into the elements it keeps. Asked by issue #99, filed 2026-11-16 at 0.27.0 and reviewed the same day at the author's word ("review open issues and address properly").

## The question

The `absent` notice (`flow-format.md` §7, step 3's notices) marks an assignment whose value can be absent through an optional value it never tests, since an absent value refuses the request `value_in_range` (ADR-0201 decision 7). It counts a value as tested where the assignment itself, or a `deny` guard of its transition, tests it with `is null` or `is not null`. #99 reports that a `foreach`'s `where` is not read so: a step inside a loop whose filter is `model.months is not null` runs only where the value is present, and its assignment reading `model.months` is still noticed.

## Findings

### TypeScript: control flow analysis

TypeScript's handbook (*Narrowing*) describes how the checker follows a test into the code it guards: "This analysis of code based on reachability is called control flow analysis, and TypeScript uses this flow analysis to narrow types as it encounters type guards and assignments." Inside `if (bird)`, a value declared `Bird | undefined` has the type `Bird`.

### TypeScript 5.5: a filter's test followed into what it keeps

The release notes of TypeScript 5.5 (*Inferred Type Predicates*) show that, before 5.5, a loop over values already filtered with `bird !== undefined` still reported "error: 'bird' is possibly 'undefined'.", and say of that code: "This code is perfectly fine: we’ve filtered all the undefined values out of the list. But TypeScript hasn’t been able to follow along." From 5.5, "the type checker is fine with this code", and "This works because TypeScript now infers a type predicate for the filter function."

## ObjectFlow beside it

Observed 2026-11-16 at `e9b83ba`: both checkers notice the issue's flow, a loop `range: 1, where: "model.months is not null"` around an assignment reading `model.months`, and do not notice the same assignment behind a `deny` guard testing `model.months is not null`. They notice it alike inside a loop nested within the filtered one, with the filter written `not (model.months is null)`, and in a loop over a set of references filtered `p.months is not null` around an assignment reading `p.months`. The core, run on the issue's flow, receives a unit whose model has no months without running the step, leaving its date absent, and gives a unit whose model has twelve months its date.

Each checker reads what a `deny` guard tests through one walk of its expression, which counts any `is null` or `is not null` it holds (`check-flows.py`'s `OptionalReads`, `check/notices.rs`'s port), and passes those tests to each step of the effect, a loop's steps among them; neither reads the loop's `where` (`check-flows.py`'s `absents`, `check/notices.rs`'s `assigned`).

## What it suggests

This is a recommendation for the author, not a decision.

- **Read a loop's `where` as a `deny` guard is read**, its tests counted for that loop's steps and for the loops within it, as TypeScript follows a test into the code it guards and, from 5.5, a filter's test into the elements it keeps. A step inside the loop runs only where the filter holds (`flow-format.md` §4.8, `foreach`), as the effect runs only where every `deny` guard holds.

## Sources

| Source | URL | Retrieved | Version |
|---|---|---|---|
| TypeScript Handbook, "Narrowing" | https://www.typescriptlang.org/docs/handbook/2/narrowing.html | 2026-11-16 | the page as served; it gives no date |
| "Announcing TypeScript 5.5", TypeScript blog | https://devblogs.microsoft.com/typescript/announcing-typescript-5-5/ | 2026-11-16 | published 2024-06-20, modified 2024-06-22 |

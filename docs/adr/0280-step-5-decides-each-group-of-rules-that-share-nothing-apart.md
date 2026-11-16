# ADR-0280: Step 5 decides each group of rules that share nothing apart

- **Status:** **Decided** by the author 2026-11-16 ("go with your recommendations"), on the second of two points put the same day for issue #98, with a recommendation. Built the same day, after the first point (ADR-0279).
- **Date:** 2026-11-16
- **Refines:** ADR-0137 decision 3 (a set of conditions past 4096 cases reported `undecided`); `flow-format.md` §7.1
- **Relates to:** ADR-0279 (#98's first point); ADR-0176 and ADR-0274 (invariants left out of step 5, which stay); ADR-0274's plant, rebuilt; `docs/research/deciding-conditions-that-share-nothing-apart.md`; issue #98

## Context

Step 5 expands each rule it decides into cases, each a conjunction of simple facts, and tries every combination of one case from each rule. Past 4,096 combinations it gives the notice `undecided`, and the rules are still enforced on each request (ADR-0137 decision 3).

Issue #98, filed 2026-11-15 at 0.27.0, has six invariants on one type, `is_whole(q1) or item.f1` to `is_whole(q6) or item.f6`, no two reading a common part. Each `or` that must not be false takes five cases, so the six take 5⁶ = 15,625 combinations, and the type was `undecided` in every state on both checkers, at `e9b83ba`. Five of them, or each written as one call, were decided. The issue asked that a combination of opaque parts, none read elsewhere in the set, be read as one opaque part, as Z3's `elim-uncnstr` does.

The research note finds KLEE's constraint independence, which "divides constraint sets into disjoint independent subsets based on the symbolic variables they reference" and decides each alone. A prototype in the reference, run 2026-11-16 over the design's modules, the self-tests, the 948 planted checks and the metric scenarios, changed one answer: ADR-0274's plant, which it decided. The review's specialist look found two refinements it needs.

## Decision

1. **Step 5 splits each set of rules it decides into groups that share nothing, and decides each group alone.** Two rules share something where they read one value, an attribute, an input, a state or a count over one of the object's own collections, or an opaque part keyed alike (ADR-0279). It applies to every set step 5 decides: a state's invariants, a transition's blocking guards and what it writes, and whether an `audit` or `warn` guard can ever pass. Rules that share nothing cannot constrain each other, so the set can hold exactly when each group can.
2. **A group that cannot hold refuses the module, whatever the other groups give.** Otherwise a group past 4,096 combinations gives `undecided`, the bound applying to each group alone.
3. **The smallest set that cannot hold, which a refusal names, is sought within the first group that cannot hold.** Sought over the whole set, as before, a trial would decide a group past the bound, and turn what was proved into `undecided`.
4. **Bounds on numbers are decided for each part they join, apart from the constant zero, each part testing alone whether its values are whole numbers.** A negative cycle through zero that visits two parts is two cycles, each within one part, one of them negative, so this is exact. Before, one decimal in a combination stopped whole-number reasoning for all of it, so `n > 3` and `n < 4`, `n` an `int`, passed beside `amount > 0`, a decimal. So the answer does not depend on how the rules are split into guards.

## Alternatives rejected

- **#98's merge: a combination of opaque parts read as one opaque part**, as Z3's `elim-uncnstr` does. It is exact where each part is read once in it and nowhere else in the set, and it covers one rule whose own cases pass the bound, which the corpus does not hold. But it needs every opaque part of a set counted before any is expanded, changes each `and`, `or` and `not`, and leaves out rules that mix in a part step 5 decides, such as one optional value each.
- **Both mechanisms.** The widest reach, but two mechanisms to keep alike in two checkers, for a case the corpus does not hold.
- **The rules encoded for a solver's search, as the Tseitin transformation does.** No expansion and no bound, at the cost of rewriting step 5 in both checkers or taking the solver dependency ADR-0137 rejected.
- **Raising the bound.** Every type pays for it, and ADR-0274 rejected it.
- **Leaving it.** #98's workarounds stay: the rule hidden in one call, or a guard on every transition writing the quantity.

## Consequences

- **What a flow meets.** #98's flow is decided, and so is #87's type with ADR-0274's exclusion off; ADR-0176's and ADR-0274's exclusions stay. One rule whose own cases pass 4,096 stays `undecided`. **A module 0.27.0 accepted may be refused**, of two kinds, neither in the corpus: a contradiction the bound left `undecided`, and one among whole-number bounds that an unrelated decimal hid. A refusal may also name a different smallest set where two groups cannot hold.
- **ADR-0274's plant is rebuilt.** Its four invariants, each reading one text, now each form a group with the `not_blank` invariant it keeps, and are decided. Each also reads one flag, so the eight are one group, and still pass the bound only with the `not_blank` invariants kept.
- **The code:** `scripts/flowlogic.py`'s `reads_of`, `groups`, `any_satisfiable`, `core`, `consistent` and `cycle_free`; `check/logic.rs`'s port.
- **The docs:** `flow-format.md` §7.1.
- **Tested** by seven cases added to `flowlogic.py`'s self-test, and to its mirror in `check/logic.rs`, one of them checking the refusal's words. Seven planted defects in each checker were each caught by a failing case.

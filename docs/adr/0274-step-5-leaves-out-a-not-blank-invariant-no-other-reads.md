# ADR-0274: Step 5 leaves out a `not_blank` invariant no other invariant reads

- **Status:** **Decided** by the author 2026-11-12 ("proceed with all 3"), on the recommendation put the same day for issue #87. Built the same day.
- **Date:** 2026-11-12
- **Refines:** ADR-0176's rule leaving `taken_by`'s invariant out of step 5; ADR-0257 (a text never blank)
- **Relates to:** `flow-format.md` §7, step 5 (each state's invariants held together, within 4,096 cases); issue #87

## Context

Issue #87, filed 2026-11-12 at 0.22.0, reported that step 5 expands every invariant a text's `not_blank` generates together with a state's other invariants. On an optional text each one multiplies the cases, so a type with eight optional `not_blank` texts is reported `undecided` in every state. While the notice stands, step 5 decides nothing about the type's invariants there, and a real contradiction among them would go unseen.

Reproduced 2026-11-12 at `6e63017` on the reference checker: a type with one required and eight optional `not_blank` texts and no invariant of its own was `undecided` in both its states; with four, decided.

ADR-0176 leaves out of a state's invariants an invariant `taken_by` generates where no other invariant reads its attribute, since absence satisfies it and it reads its attribute alone, so it holds together with any other. The invariant `not_blank` generates, `<attribute> is null or trim(<attribute>) != ""`, is of the same shape: it reads its text alone, and holds where the text is absent or not blank (ADR-0257).

## Decision

1. **Step 5 leaves out of a state's invariants each one a text's `not_blank` generates, where no other invariant reads its text**, in both checkers (`flowlogic.py`'s and `check/logic.rs`'s `state_invariants`). It knows one by the marker the checker's view sets on it, and the attribute's `not_blank: true`.
2. **One whose text another invariant reads is kept**, as `taken_by`'s is, a state's `required_attributes` among them, so a contradiction it takes part in is still found.

## Alternatives rejected

- **Raising the bound past 4,096 cases.** Every type would pay for what one kind of invariant needs, and the next such invariant would meet the bound again.
- **Dropping `not_blank` from some texts, or making them required**, the issue's workarounds. Each weakens or misstates the model to suit the checker.
- **Leaving it, the notice standing.** Step 5 would decide nothing of the type's invariants in those states.

## Consequences

- `flowlogic.py`, `check/logic.rs` and `flow-format.md`'s step 5 say so. Step 5 gives notices alone, so no module is newly refused.
- **Tested**: `not_blank_step5_plants` in `check-flows.py`, which both checkers agree on through the core's oracle: the issue's type, eight optional texts read by nothing else, decided in each state; four texts each read by another invariant, their `not_blank` invariants kept, past the bound, where the four other invariants alone are decided. Four planted defects were each caught, two in each checker: every `not_blank` invariant kept, and one another invariant reads left out. A first plant of eight read texts let the second defect survive, the other invariants alone passing the bound, and was replaced.

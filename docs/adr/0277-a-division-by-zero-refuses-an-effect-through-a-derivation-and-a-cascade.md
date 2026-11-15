# ADR-0277: A division by zero refuses an effect through a derivation and a cascade

- **Status:** **Decided** by the author 2026-11-15 ("go with your recommendations"), on the fourth of four points put the same day for issue #97, each with a recommendation. Built the same day.
- **Date:** 2026-11-15
- **Refines:** ADR-0185 decision 1 (an effect's value an arithmetic left with none refuses), `declaration-syntax.md` §8.2 (the effect's row)
- **Relates to:** ADR-0047 (a division by zero is unknown), ADR-0205 decision 2 (an arithmetic past its bound refuses wherever it is evaluated), ADR-0209 decision 5 (a `set` of a plain read clears an optional attribute), issue #97

## Context

A division by zero is `unknown` (§8.2, ADR-0047): a guard reading it is unsatisfied, and a derivation reading it answers no value. An effect whose value a division by zero left with none refuses the request as `value_in_range`, naming its object, since no value can be stored for it (ADR-0185). Each runner notes the division where it evaluates it, and refuses an effect's value that has none and carries the note (`eval.rs`'s `failed_arithmetic`, `request.rs`'s `effect_value`; `flowrun.py`'s `effects`).

#97's specialist look found that the note reaches an effect only from a division written in it. Observed 2026-11-15 at `b5ecfab` on the core, on a flow deriving `units = total / per`, with `per` 0:

- `range: total / per` was refused `value_in_range`, and `range: this.units` ran no step.
- A create given `opt: total / per` was refused, and given `opt: this.units` it created its object without `opt`.
- `set copied := this.units` cleared a stored `copied` of 1, the request satisfied, by ADR-0209 decision 5's rule for a `set` of a plain read.
- A cascade passing `opt: "total / per"` drove its transition without `opt`.
- An argument `(total / per) is not null or this.flag`, `flag` absent, was refused, where the same without the division is not passed.

A derivation is evaluated apart and keeps its bare value (`eval.rs`, the derived member's read; `flowrun.py`'s `member`), and a cascade's inputs are evaluated without the check (`request.rs`'s and `flowrun.py`'s `cascades`). The note, once set, is never cleared, though a presence test has answered it. So a division by zero cleared stored data in one place, and refused a value absent for another reason in another.

## Decision

1. **A division by zero a derived attribute reads reaches what reads it.** An effect reading it refuses `value_in_range`, as one written in the effect does, a loop's `range`, a `set`, an argument and a cached second read of it alike. A guard reading it is unsatisfied and a read of the derivation answers no value, as before.
2. **A cascade's inputs are an effect's values.** One a division by zero left with none refuses the request `value_in_range`, naming the object whose transition cascades, and nothing is driven.
3. **An answer that is decided answers the division it read.** `is null` and `is not null`, an `if` whose condition is decided, and an `and`, an `or` or an `implies` whose answer is decided each answer a division by zero inside them, which then refuses nothing: a value absent for another reason is not passed, as where no division was read. An answer left `unknown` keeps the note, and an effect taking it refuses.

## Alternatives rejected

- **Leaving it, and stating in §8.2 that a derived division by zero reaches an effect as plain absence.** A `set` would then clear stored data on a failed arithmetic, which ADR-0185 decided to refuse (PRD F2).
- **A derivation refusing a division by zero wherever it is read**, as an arithmetic past its bound does (ADR-0205 decision 2). A read of an object whose derived divisor is zero would be refused, and §8.2's rule that a division by zero is `unknown` would change for derivations alone, a larger change than the defect asks.
- **Never clearing the note.** A value a presence test had answered refused a later value absent for another reason, which §8.2 does not refuse.

## Consequences

- **The code:** `eval.rs`'s derived member, its cache of derived values each with whether a division left it with none, and its `is null`, `if` and logical operators; `request.rs`'s `cascades`. The reference's `flowrun.py` alike.
- **The docs:** `declaration-syntax.md` §8.2's effect row; `DESIGN.md` §5.7's evaluation, whose sentence also said an arithmetic past its bound yields `unknown`, which ADR-0205 decision 2 replaced.
- **What a flow meets:** a request a division by zero left with no value to store, through a derived attribute or a cascade's input, is refused `value_in_range` where 0.26.0 ran no step, passed nothing or cleared the stored value; and one whose division a presence test or a decided answer read is not refused, where 0.26.0 refused it.
- **Tested** by `request.rs`'s `a_division_by_zero_refuses_through_a_derivation_and_a_cascade`, on both backends, and the Studio's `test_a_division_by_zero_refuses_through_a_derivation_and_a_cascade_alike` through both runners; eleven planted defects, each part of the fix undone in either runner, each caught.

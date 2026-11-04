# ADR-0255: A value's invariant is its holder's invariant `<attribute>_<invariant>`

- **Status:** **Decided** 2026-11-04 under the author's direction ("go with your recommendations"), on the recommendation given the same day once building ADR-0254 decision 4 found its name conflicting with how an invariant is named elsewhere. Built the same day.
- **Date:** 2026-11-04
- **Refines:** ADR-0254 decision 4, which named a value's invariant broken on a holder `<attribute>.<invariant>`
- **Relates to:** ADR-0176 decision 1 (`<transition>_taken_by`, a name the engine generates); ADR-0164 (an observation kind's invariant reads its own fields); issue #65

## Context

ADR-0254 decision 4 holds a value's invariant for each attribute holding it, and names a write breaking it the holder's invariant `<attribute>.<invariant>`, such as `days.in_order`. Wherever an invariant is named, a dot already qualifies it by its type:

- A transition's `may_admit` names `<invariant>` or `<Type>.<invariant>`, by the schema's pattern `^([A-Z][A-Za-z0-9]*\.)?[a-z][a-z0-9_]*$`, which `days.in_order` does not match.
- A migration's `admit` names `<Type>.<invariant>`, and `Booking.days.in_order` would not match it either.

The engine already generates invariants a type does not declare, under names joined by an underscore: `<transition>_taken_by` (ADR-0176) and `<attribute>_unique`.

## Decision

1. **A value's invariant `i`, held by attribute `a` of type `T`, is `T`'s invariant `a_i`.** Its expression is the value's, each field `f` read as `a.f`, and it holds where the value is absent: `a is null or (<expression>)`.
2. **It is named as any invariant of `T` is:** `may_admit: [a_i]`, `admit: [{ invariant: T.a_i, … }]`, and a refusal's clause.
3. **A type MUST NOT declare an invariant named `a_i` for a value one of its attributes `a` holds**, as it declares none named `<attribute>_unique` (step 3, `names`).
4. **A value's invariant reads its own fields**, literals and enumeration members, and calls `length` or `trim` alone, as an observation kind's does (ADR-0164). A finding on the invariant is reported at the value that declares it.

## Alternatives rejected

- **The dotted name `<attribute>.<invariant>`, the schema's patterns widened to take it.** The dot would mean two things, a type's qualifier and an attribute's, told apart only by the case of a letter.
- **The value's own name, `<Value>.<invariant>`.** Two attributes holding one value would break one invariant of one name, and a refusal could not say which value was wrong.

## Consequences

- Built in the core's `expr.rs` (`held_through`), `model.rs` (the library giving each holder its invariants) and its checker's `view.rs` and `raw.rs`, and in `scripts/flowexpr.py` and `scripts/check-flows.py`; the schema, `flow-format.md` §4.20 and `declaration-syntax.md` §3.5 say so. The reference runner reads the view's invariants and needed nothing of its own.
- **Tested** on both backends: a value's invariant refusing each of two holders by its own name, a string holding a field's name left as the string, and an absent value holding it (`a_values_invariant_holds_for_each_attribute_holding_it`). Both checkers agree on five plants: an invariant over its fields, one reading what is no field, one calling a function but `length` or `trim`, one that does not type, reported at the value, and a type's invariant of a held one's name. Examples on both runners and the Flow Studio's test through both alike; a planted wrong invariant name was caught by both runners.

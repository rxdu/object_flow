# ADR-0275: A derived set defaults to the empty set, and a derivation carries through its target

- **Status:** **Decided** by the author 2026-11-13 ("proceed with your recommendations"), on the five points put the same day for issue #90, each with a recommendation. Built the same day.
- **Date:** 2026-11-13
- **Refines:** ADR-0271 decisions 2 and 8 (what a derived input is, and the loaders that expand it)
- **Relates to:** ADR-0122 decisions 10, 11 and 27 (an optional input absent is skipped; a set is never absent); ADR-0271 decisions 6 and 7 (a name the step must give; a call takes none); the model's check 12 (no transition causes itself) and check 17 (what `clear` may empty); `declaration-syntax.md` §8; issue #90

## Context

Issue #90, filed 2026-11-13 at 0.24.0, found three gaps where an act derives a record's set fields by `rest: inputs`. Each was reproduced at `b799001` on both checkers and both runners, on a flow of the repository's own: a `Thing` with a kind `Custody`, which a `Mover` records.

1. **A kind's optional set field derives an input step 3 refuses.** ADR-0271 decision 2 derives an input the target does not require as optional. A set input is never optional: a caller supplies the empty set instead, so a set never stands absent in a guard or an effect (ADR-0122 decisions 10, 11 and 27). A recording that leaves such a field out holds it empty.
2. **No expression seemed to give an empty set.** `[]` is no expression. The collection literal `{}` already is the empty set: both checkers take it as a call's or a create's set input and as a set input's default, and both runners pass it empty. Neither design document says so.
3. **A derivation stops at a create whose target takes the rest of its own inputs.** Each loader read its targets as declared, before their own `rest: inputs` was expanded. Step 3 then refused the create for the inputs the target derives, though ADR-0271 decision 4 makes a derived input the transition's own.

Two of the issue's three paths are calls, which ADR-0271 decision 7 leaves without `rest: inputs`. And `optional` on a type's set attribute is what lets `clear` empty it (check 17), where `flow-format.md` §4.3 said a set is never optional.

## Decision

1. **A derived set the target need not be given defaults to the empty set**, `default: "{}"`. A request leaving it out records what the target would. A set the target requires stays required.
2. **A set the target gives a default of its own is not derived.** That default may be a declared input's or an attribute's. The step names the set instead (step 3, `names`), as ADR-0271 decision 6 makes a clashing name. Deriving `{}` would replace the target's default unseen.
3. **A derivation reads each target with its own `rest: inputs` expanded**, so a chain carries. Each loader expands a transition after the transitions it creates by. Each derived input is marked with the step's own target, as ADR-0271 decision 2 marks it, so a form groups a step's inputs by that step.
4. **A cycle of such creates is refused by the model's check 12**, as before. Each loader reads a transition it reaches again as it stands, so each expansion ends.
5. **`{}` is the empty set.** `declaration-syntax.md` §8 says so, and both parsers name it where a set is written as a list.
6. **`optional` on a set stays.** On a type's set it lets `clear` empty the set. On a kind's field it lets a recording leave the set out, recorded empty. `flow-format.md` §4.3 and §4.4 say so, and the notice `unwritten` calls such a set always empty.
7. **A `call` still takes no `rest: inputs`** (ADR-0271 decision 7). Deriving a call's inputs needs its target's type at load, which the runtime loader does not read. It stays a follow-up in `TODO.md` until a second case needs it.

## Alternatives rejected

- **Step 3 refusing `optional` on a set attribute**, the issue's first option. A type's set would lose `clear`, and a kind's set field the choice that its recording may leave it out.
- **A derived set required of every request.** It holds the act to more than the record it makes, and each caller sends `{}` for nothing.
- **An optional set input, left out passing nothing on.** The set would stand absent in the act's guards and effect, which ADR-0122 decision 27 rules out.
- **Copying the target's own default onto the derived input.** A default is read where it is declared, and the act's scope differs from the target's.
- **The marker naming the first record through a chain**, `Custody.record`. A step's derived inputs would then split across two marks.
- **`rest: inputs` on a call now.** Decision 7 gives the reason.

## Consequences

- **The loaders:** `check-flows.py`'s `with_rests`, the core's `check/view.rs` `with_rests` and `model.rs` `with_rests` with `derived_input`. Both parsers' message for `[`, and both checkers' notice `unwritten`.
- **The docs:** `flow-format.md` §4.3, §4.4, §4.8 and §7; `declaration-syntax.md` §8; `library-api.md` §6; the format's card.
- **What a flow meets:** a module 0.24.0 accepted reads as before. One it refused for a derived optional set, or for a chain, is now taken. A set with a default of its own, derived, was refused before and is refused now, by another message.
- **Tested** by `rest_sets_and_chains_plants` in `check-flows.py`, which both checkers agree on through the core's oracle; the core's `rest_inputs_derives_sets_and_through_another`; its `a_derived_set_left_out_is_empty_and_a_derivation_carries_through`, so on both backends; the studio's `test_derived_sets_and_chains_are_served_and_taken_alike`; and the core's parser agreeing with the reference's on `{}` and `[]`. Sixteen planted defects were each caught: six in the reference checker and parser, six in the core's checker and parser, and four in the core's loader.

# ADR-0217: A transition taken only as a consequence, `only_via: [any]`

- **Status:** **Decided** 2026-10-18 under the author's direction ("we need to address the issues properly so that they don't have to work around it"), for issue #25, which the operations platform's developer filed.
- **Date:** 2026-10-18
- **Refines:** the model's checks 11, 13 and 14 (`declaration-syntax.md` §9), `flow-format.md` §4.8's `only_via`
- **Relates to:** ADR-0163 (one store holds every module); the audit's write path R-05 (`only_via` held at every depth); issue #25

## Context

`only_via` makes a transition unrequestable and names the transitions that alone may take it, `Type.transition`, the type declared in the module or imported by it. A module can therefore restrict its transitions only to modules it imports, which come before it. A base module whose type other modules keep in step with their own records, a shared `Absence` that leave, travel or training open and close in the same step as theirs, cannot say "never moved by itself": its importers come after it in the import order, two modules cannot import each other, and naming them would make every new importer edit the base. Its transitions stay requestable by any actor `taken_by` admits, so the rule holds only by every caller's convention (issue #25, reproduced on 0.3.0).

## Decision

1. **`only_via: [any]`** lets every transition of any type take the transition, by a `create`, a `call` or a cascade, at any depth, and refuses a direct request `not_requestable`, as `only_via` does, its `parents` `["any"]`.
2. **`any` stands alone** in the list; beside a named transition it is a `names` finding, in both checkers. The word is the format's own for "every", as in `from: any`, and no transition may be named it (`NOT_A_TRANSITION`).
3. **A part's creation still names its whole's transitions** (check 11): `any` names none, so it does not satisfy it. `only via any` names no parent, so the checks a named parent meets (check 13, that it exists and reaches the transition) have nothing to verify, and a caller is never refused for not being named.
4. **The text language writes it `only via any`**, which the model's checks read as naming no parent.

## Alternatives rejected

- **`only_via: cascade`**, the issue's proposal: the format already calls a whole's final transitions reaching its parts a cascade, so the word would mean both that and every consequence.
- **`taken_by: [cascade]`**: `taken_by` names kinds of actor, which a consequence is not.
- **Each importer declaring at its import which of its transitions may take the imported one**, unioned into the effective `only_via`: more precise, and more machinery than the case needs; it remains open if a type needs to tell its importers apart.

## Consequences

- `flow.schema.json`'s `only_via` items admit `any`; `check/names.rs` and `check-flows.py` refuse it beside another name and skip it among names checked; `check/model.rs`'s reach check reads it as no parent, as `check-syntax-doc.py`'s pattern does; `request.rs`'s `take` and `flowrun.py`'s let any transition take it.
- `flow-format.md` §4.8 and the text mapping, `declaration-syntax.md` §9 and the card say so.
- Tests: `a_transition_only_via_any_is_taken_by_any_other` (the issue's `people` and `leave` modules, a direct request refused, a creation and a call taking it, the core's checker passing them), and a plant of `any` beside a named transition, which both checkers refuse alike.

# ADR-0282: A qualified name is read whatever its member's case

- **Status:** **Decided** by the author 2026-11-17 ("go with your recommendations"), on the point put 2026-11-16 for the finding that a qualified name whose member is lower case passes step 3, with a recommendation. Built the same day.
- **Date:** 2026-11-17
- **Refines:** `flow-format.md` §7's rule for a qualified value (step 3, `names`)
- **Relates to:** `declaration-syntax.md` check 19 (a name that resolves to nothing), which the checkers enforced for an upper-case member alone; ADR-0269 (`ActorKind.<kind>`); ADR-0254 (values); `flow.schema.json`'s `typeName`, `stateName` and `memberName`

## Context

Step 3 refuses a qualified name that names nothing: `state == Foo.BAR`, where no type, enumeration or value is named `Foo`, is refused, "names Foo.BAR, and no enumeration or type Foo is declared in this module or imported". The typers leave an owner nothing declares to that rule (`check-flows.py`'s typing of a path, `check/names.rs`), and the rule reads only a member written in upper case (`check-flows.py`'s `value_errors`, `check/names.rs`'s port).

Found 2026-11-12 building #85 and reproduced 2026-11-16 at `87a6ae8` on both checkers, the core's through `/sandbox/load`: an invariant `open_by_kind is null or open_by_kind == Foo.bar` loads clean. So does `kind == Kind.small`, `Kind` an enumeration of `SMALL` and `LARGE`: the typer gives an enumeration's type without reading its member. The core answers every creation the invariant checks `501 not_run`, "`Foo.bar`" or "`Kind.small`". Building this decision found a third such name: `Bundle.size`, `Bundle` a value, loads clean on both checkers, and the core answers every creation `501 not_run`, "`Bundle.size`", the bundle left out too.

The schema gives a capitalised name only to a type, an enumeration, a value or a machine (`typeName`), and every enumeration value is upper case (`stateName`). A value's and a machine's name own nothing an expression names through them: with an upper-case member each is refused today, "no enumeration or type Bundle is declared". A lower-case member after a capitalised owner is legal in an expression only as a kind of actor, `ActorKind.human` (ADR-0269). Outside expressions a module names a type's transition, invariant or metric so, as `only_via: [Customer.forget]` does, and the rule does not read those.

## Decision

1. **The qualified names' rule reads a member of any case.** A member written in upper case is read as before.
2. **An owner that is no enumeration or type of the module, no name it imports, and not `ActorKind` is refused whatever its member's case.** `Foo.bar` is refused as `Foo.BAR` is, and a value's or a machine's name with a member of any case, `Bundle.size`, as `Bundle.FOO` is.
3. **An enumeration's member must be one of its values, whatever its case.** `Kind.small` is refused, "the enumeration Kind has no small".
4. **A type's lower-case member stays the typer's**, which refuses it, "names no state". **`ActorKind`'s member stays the typer's** whatever the module declares, as ADR-0269 decides. **An imported owner's member is left as it is in upper case.**

## Alternatives rejected

- **Only the owner nothing declares**, as the finding first recorded. `Kind.small` would still pass step 3 and fail every run.
- **Leaving it.** A module the checks pass fails every request its expression reaches.

## Consequences

- **What a flow meets.** A qualified name whose owner nothing declares, whose owner is a value or a machine, or whose enumeration lacks its member, is refused at step 3. A module 0.28.0 accepted may be refused: one whose expression the core cannot run.
- **The code:** `check-flows.py`'s `value_errors` and `check/names.rs`'s port.
- **The docs:** `flow-format.md` §7's rule for a qualified value names the owner as well as the value. Check 19 already refuses a name that resolves to nothing, so its row is unchanged.
- **Tested** by cases added to the reference's planted checks, which the core's oracle replays: an owner nothing declares with a lower-case member and with a mixed-case one, and an enumeration's member in lower case, each refused; a value's lower-case member, refused; `ActorKind.agent`, still clean, and a module declaring an enumeration `ActorKind` refused by ADR-0269's finding alone; and a type's lower-case member, refused once, by the typer, "names no state", and not again by the rule.

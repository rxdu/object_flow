# ADR-0247: A version renames an enumeration, and a rename keeps a change of type the publish carries

- **Status:** **Decided** 2026-11-03 under the author's direction ("go with your recommendations"), on the recommendation given the same day for issue #58. Built 2026-11-04.
- **Date:** 2026-11-04
- **Refines:** ADR-0146 (a rename keeps its attribute's type and optionality), ADR-0229 decision 5 (a reference to a moved type keeps its values)
- **Relates to:** ADR-0120 (a migration is written with the version it publishes); ADR-0230 decision 2 (an entry with nothing to act on is a notice); issue #58; `docs/research/moving-objects-between-types.md` §8

## Context

Issue #58, filed 2026-11-03 at 0.12.0, asks for two renames that change no value. A deployment renaming its declared names to its industry's words cannot make them. Reproduced the same day on the published 0.12.0 (`TODO.md`, issue #58), with a sample whose enumeration `Format` is renamed `Binding`, its members unchanged, and whose `Genre` is moved to `Category`:

1. Each attribute typed by the renamed enumeration was noticed as a change of type, `check:migration`, its values starting absent, and a required one was refused `required_absent`. A move giving `Category.binding` from `format` was refused `mapping:moved_types:Genre.Category.binding`, "what it cannot hold".
2. `renamed_attributes: { Book: { genre: category } }` beside `moved_types: { Genre: [ { to: Category } ] }` was refused `check:names`, "which changes its type or whether it is optional".
3. Sequenced over two versions, the rename and then the move published in turn. The second version's source, with the rename kept in it, was then refused `check:names` over a store at the first, so no one source served both, against ADR-0120.

The flow format had no key renaming an enumeration (`flow-format.md` §4.16). A rename had to keep its attribute's type by the declared type's name, in both checkers, though a reference keeping its name already followed a move (ADR-0229 decision 5).

The store holds a member as its name, and each column's `CHECK` is named by its table and attribute (`store.rs`, `table_parts`). So a renamed enumeration with its members kept changes no column, no stored value and no interval.

PostgreSQL renames an enum type in place, and each of its values apart. Rails wraps both statements. Avro keeps an old name as an alias on the type itself. Django's autodetector repoints a renamed model's references before it compares a renamed field, so a field renamed with its target is still a rename (the research, §8).

## Decision

1. **A version renames an enumeration by `migration.renamed_enumerations: { <Old>: <New> }`**, in the module that declares the new one.
   - Every attribute typed by the old enumeration, in any module of the version, is typed by the new one and carried. Its values, intervals and history stay as they were, each member by its name. The rename writes no row and records no event.
   - A member the version also drops is mapped by `removed_members` under the new name, since renames apply first (`publish-and-import.md` §1).
   - A move's or a backfill's value of the old enumeration fits the new one by its member's name.
   - Step 3 (`names`) refuses a rename that is no rename: a new name this module does not declare, two renames to one name, an old name another module of the version still declares or renames too, an old name the version before did not have, and a new name it had, since a rename does not merge two enumerations.
   - Over a store whose installed version lacks the old enumeration, the entry renames nothing, a notice, `migration_not_applicable:renamed_enumerations:<Old>`, so one source serves a store at either version (ADR-0230 decision 2).
2. **A `renamed_attributes` entry whose only change of type the publish carries is still a rename:** a reference following a type any module of the version moves (ADR-0229 decision 5), or an enumeration renamed by (1). Its optionality must be unchanged, and any other change of type is refused, as before. A referent landing in a type the new reference does not take is refused `moved_reference`, as decision 5 refuses one.
3. **Both checkers, both publishes and the format carry it:** the core's and the reference's checker and publish, `flow.schema.json`, `flow-format.md` §4.16, the card and `declaration-syntax.md` §6.6.

## Alternatives rejected

- **An enumeration identified by its members.** Two enumerations with the same members would become one with no one saying so. One whose version also adds a member could not be renamed.
- **Avro's alias, kept in the type.** It stays in the declaration for good, where a migration entry is dropped once no store needs it (`flow-format.md` §4.16).
- **The issue's workaround, a new attribute backfilled member by member.** It needs both enumerations declared at once, and the new attribute's intervals start at the publish.
- **Two versions.** No one source serves both (probed, above).
- **Any change of type under a rename.** It ends ADR-0146's guarantee that a rename carries its values unchanged.

## Consequences

- Built in the core's `model.rs` (`Migration::renamed_enumerations`), `publish.rs` (`follows_rename`, `carries`, `renamed_member`, the simulation's values and intervals, a move's and a backfill's fit, the removed members and the notice), `store.rs` (`install`'s set-aside and a migration's `wrote`) and `check/migration.rs`; in `scripts/check-flows.py` and `scripts/flowpublish.py`; and in `flow.schema.json`.
- **Tested.** Both checkers agree on the oracle's 725 cases, 20 of them this decision's (`check-flows.py`, `renamed_enumeration_plants`). Six core tests pass on both backends:
  - `a_version_renaming_an_enumeration_keeps_every_value_and_interval`
  - `a_renamed_enumerations_dropped_member_is_mapped_under_its_new_name`
  - `an_enumeration_rename_over_a_store_that_took_it_renames_nothing`
  - `a_failed_enumeration_rename_installs_nothing`
  - `a_move_gives_a_renamed_enumerations_value_by_its_member`
  - `a_reference_renamed_as_its_referents_type_moves_is_carried`
- Two Flow Studio tests pass through both backends alike. Eleven defects planted in the core's publish and approval were each caught by those tests. The reference publish of 0.13.0, put back, failed the Studio's.
- **Integrity (F2):** a rename writes no row, and an approval that fails installs nothing, the same change applying once the fault is gone (`a_failed_enumeration_rename_installs_nothing`).

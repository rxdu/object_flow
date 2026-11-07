# ADR-0268: A migration clears an attribute where a condition holds

- **Status:** **Decided** 2026-11-07 under the author's direction ("check the latest issues and address them"), on the recommendation formed the same day for issue #82, accepted in advance as the author directed for a confident recommendation. Built the same day.
- **Date:** 2026-11-07
- **Refines:** ADR-0073 (`clear` writes absence); ADR-0230 decision 4 (a backfill runs once on a store); ADR-0231 (a backfill rewriting a held value)
- **Relates to:** ADR-0053 (no bare `null`); ADR-0147 (the order of a publish's mappings); ADR-0257 (a text never blank); ADR-0262 (a clearable input); issue #82

## Context

Issue #82, filed 2026-11-07 at 0.20.0, reported that a version's migration cannot give an attribute no value. A backfill gives every object a value by an expression, and the language writes no value for none. So a store that kept an empty text for "none" cannot move its objects to an optional text marked `not_blank`.

Reproduced 2026-11-07 at `fed4fb4`, on a flow of the repository's own. Version 1 declares `Thing.code` a required text, and two objects hold `""` and `"A"`. Version 2 makes `code` optional and `not_blank`:

- The backfill `if code == "" then null else code` is refused by both checkers: "an operand, found 'null'". The core refuses the draft `malformed`.
- With no backfill, the publish is refused: the empty code violates `code_not_blank`.
- The backfill `if code == "" then never_given else code`, reading an optional attribute nothing writes, published on the core's in-memory SQLite store and through the reference runner. `""` became none, under `backfill_rewrites`, and `"A"` was kept.

The issue named two mechanisms: a `null` operand allowed in a backfill alone, or a migration key clearing an attribute where an expression holds.

The language spells absence one way. ADR-0053 made a bare `null` a publish error, so that presence is tested with `is null` and never compared. ADR-0073 then made `clear` the write of absence, and rejected a `null` literal on the write side, since it would "put the token back in the language for readers to confuse".

How established tools clear a value in a data migration: SQL writes `UPDATE t SET code = NULL WHERE code = ''`, and Rails `where(code: "").update_all(code: nil)`. Each names what it clears and the rows it clears it on.

## Decision

1. **A migration's `clear` gives an attribute no value on each object where a condition holds**: `clear: { Thing: { code: 'code == ""' } }`. Each entry names a type, an attribute and the condition.
2. **It clears what `clear` may clear**: an optional, singular attribute or stored reference of the type, no composite end. Its condition is a `bool`, reading what a backfill reads: the object's own members and literals, never `now` or an input. Both checkers refuse any other at step 3, `names`.
3. **Clears run after the backfills**, in the order written. Each condition reads the object as the mappings and the backfills left it. So a backfill reading the attribute reads its value from before the clear, and a clear after a backfill of the same attribute clears what the backfill wrote.
4. **A clear acts where its condition is true** and the attribute holds a value. A condition that is `unknown` clears nothing, as a filter selects nothing it cannot decide.
5. **Each object it clears is named**, a notice, `mapping:clear:<Type>.<attribute>`. The notice is recorded on the object's migration as every mapping's is, and the clear is a write its event records.
6. **A clear runs once on a store**, as a backfill does. One the installed version's migration holds, its condition unchanged, is not run again, a notice, `migration_applied:clear:<Type>.<attribute>`.
7. **A clear keyed by an abstract base reaches every type of its family**, as a backfill does.
8. **An object a clear leaves in violation is a decision**, as any mapping's is (ADR-0146). Such an object is in a state requiring the attribute, or breaks another invariant. It is fixed first or admitted, or the approval is refused `decision_unresolved`. Probed 2026-11-07 on both runners: a clear leaving an object in a state requiring the attribute gave the decision `invariant:Thing.open_invariant` in the core's dry run, and the reference runner refused the approval `decision_unresolved`.

## Alternatives rejected

- **A `null` operand allowed in a backfill alone**, `if code == "" then null else code`. ADR-0073 rejected a `null` on the write side for the reason above, and a backfill is a write. Both parsers would also read `null` as an operand in one place alone.
- **An attribute declared only to be absent**, the reproduction's third backfill. It declares a member that means nothing, and the reader of the backfill learns of the clear only by tracing an absent read.
- **Fixing the objects first**: a version adds a transition clearing the text, a script takes it on each such object, and a later version marks the text `not_blank`. It takes two releases, a transition kept only for a migration, and a sweep of each store between them.
- **Admitting the invariant.** The empty texts stay, which is what the change is meant to end.
- **Clears before the backfills.** A backfill could then not read the value a clear removes, and a backfill trimming a text could not be followed by a clear of what is left blank.
- **Naming a clear's objects under `backfill_rewrites`.** A clear removes a held value on every object it names, so its own notice already says what `backfill_rewrites` would add.

## Consequences

- **The core.**
  - `model.rs`: `Migration::clear`.
  - `publish.rs` runs the clears after the backfills.
  - `check/migration.rs` holds decision 2's names, and `check/names.rs` its type.
  - `check/lines.rs` renders each clear.
- **The reference runner**, `flowpublish.py`, runs them alike. `check-flows.py` holds decision 2 in the same words as the core.
- `flow.schema.json`, `flow-format.md` §4.16 and §7, `declaration-syntax.md` §6.6, `publish-and-import.md` §1 and the format's card state the key.
- **It is additive.** A module without the key behaves as before, and format 1's frozen corpus is unchanged.
- **Tested.**
  - `a_migrations_clear_gives_an_attribute_no_value_where_its_condition_holds`, on the library, so on both backends:
    - a code of white space trimmed by a backfill and then cleared, with an empty code, each named, the migration recording the write;
    - `"A"` kept, its condition `unknown`;
    - the clear run once on a store, and again once changed.
  - `a_clear_keyed_by_an_abstract_base_reaches_its_family`, on the library: both types of the family cleared.
  - `clear_plants` in `check-flows.py`:
    - seven cases at step 3, which both checkers agree on through the core's oracle;
    - the same publish run by the reference runner's examples, a kept clear and an abstract base's among them, and an expectation changed failing its example.
  - The Flow Studio's approval of the fleet with a clear, alike on the core and the reference runner, the grades it clears read back.
  - Seventeen planted defects were each caught, ten in the core and seven in the reference runner and `check-flows.py`:
    - in each runner, a kept clear run again, a clear on an `unknown` condition, clears run before the backfills, and a base's clear reaching its own type alone;
    - in the core, no clear loaded and no object named, and in the reference runner, no clear run;
    - in the core's step 3, a required attribute cleared, a condition reading beyond the object, a member that does not exist, and a condition that is no `bool`; in `check-flows.py`, the first and the last.

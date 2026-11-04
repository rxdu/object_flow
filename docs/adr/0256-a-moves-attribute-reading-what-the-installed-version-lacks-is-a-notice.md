# ADR-0256: A move's attribute reading what the installed version lacks is a notice, and a move noticed where it leaves an attribute behind

- **Status:** **Decided** 2026-11-04 under the author's direction ("check the latest issues and address all of them properly"), on the recommendation formed the same day for issue #68, accepted in advance as the author directed for a confident recommendation. Built the same day.
- **Date:** 2026-11-04
- **Refines:** ADR-0229 decision 4 (a move's attributes given or carried) and ADR-0252 (a move's state map over a store past the state)
- **Relates to:** PRD F2; ADR-0230 decision 5 (a publish's mapping naming what the installed version lacks is a notice); ADR-0147 (a drop where an attribute of its type is added, which may be a rename); issue #68

## Context

Issue #68, filed 2026-11-04 at 0.16.0, found that one source moving a type cannot carry an attribute an earlier version renamed, over stores at both versions. Reproduced 2026-11-04 on the published 0.16.0 (`TODO.md`, #68), on fresh SQLite stores:

1. Version 1 declares `Span` with an optional `sort` of an enumeration, and a span is opened with `sort: A`.
2. Version 2 renames `sort` to `kind`, by `renamed_attributes`.
3. Version 3 moves `Span` to `Period`, which has `kind`.

Over a store at version 2, a move giving `kind` from `sort`, `attributes: { kind: sort }`, was refused: *"the move of Span to Period reads 'sort'; a move reads the old object's own members and literals"*. Over a store at version 1, a move without it published and left `kind` absent. Its findings named only the move, `mapping:moved_types:Span.Period`, and nothing of `sort`'s values. The same drop in place, `sort` replaced by `kind` in one type, is noticed twice: the checker's `migration` notice, *"Span.sort is gone and Span.kind is new, of the same type: if it is a rename, map it, or its values stay behind"*, and the publish's `possible_rename:Span.sort`.

ADR-0252 made a move's `states` naming a state the installed old type lacks a notice in a publish. A move's `attributes` stayed fatal in every case.

## Decision

1. **In a publish, a move's `attributes` entry whose expression reads a member the installed version's old type lacks is a notice**, `migration_not_applicable:moved_types:<Type>.<Target>.<attribute>`, naming the installed version. The entry gives nothing, and the target attribute is carried as one the move gives nothing is: from the old attribute of its name and type, else absent (ADR-0229 decision 4).
2. **Given `--previous`, it stays fatal**, as ADR-0230 decision 5 and ADR-0252 decision 2 keep every other mapping, since there the version named is the one before and the mapping is a mistake in the source.
3. **A move's `where` reading what the installed version lacks stays fatal.** It decides which entry takes an object, and an entry that cannot decide would move an object by a rule its author did not write.
4. **A move that leaves an old attribute behind, where the target declares an attribute of its type that takes nothing, is noticed as the same drop in place is.** The old attribute is carried into no attribute of the target and read by no expression of the entry; the target's attribute is neither given nor carried. The checker gives its `migration` notice, and a publish gives `possible_rename:<Type>.<attribute>`, naming the target's attribute.

## Alternatives rejected

- **A rename keyed by the new type applying to the moved objects**, the issue's form (a). It makes `renamed_attributes` mean a rename of the old type when keyed by the new one, where the move's `attributes` already say what each target attribute takes.
- **A refusal where a move leaves an attribute behind.** A removed attribute needs nothing, its values staying in history (`declaration-syntax.md` §6.6), and a drop in place is a notice; a move is no different.
- **Two releases in order, every store first taking the rename's version.** It works, but ADR-0230 exists so that one source serves every store.

## Consequences

- Built in the core's `check/migration.rs`, `model.rs` (a move entry keeping what each attribute's expression reads, as the checker reads it) and `publish.rs` (`not_applicable`, `moves`, `moved_attribute_notices`), and in `scripts/check-flows.py` and `scripts/flowpublish.py`, which reads an expression with the checker's `reads`; `flow-format.md` §4.16 and `publish-and-import.md` say so.
- **It refuses nothing 0.16.0 accepted.** It adds notices to publishes 0.16.0 published.
- **Tested** on both backends: the issue's three versions, a move giving `kind` from `sort` and a move alone, each over a store at version 1 and one at version 2 (`a_moves_attribute_over_a_store_past_a_rename_is_a_notice_and_one_left_behind_noticed`). Giving `kind` from `sort`, the span kept `kind: A` over both, with the notice over version 2 alone. The move alone kept it over version 2 and left it absent over version 1, where it gave `possible_rename:Span.sort` and the checker's notice. Both checkers agree on five plants: the attribute fatal given `--previous` and clean over the installed version, a `where` so reading fatal over the installed version, an attribute left behind noticed, and one carried by its name not. The Flow Studio's test passes through both runners alike over the four cases. Eight planted defects were each caught, four in each implementation: the old refusal, the entry evaluated anyway, the publish's notice and the checker's notice.

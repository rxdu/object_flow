# ADR-0252: A move's state map naming a state the installed version lacks is a notice

- **Status:** **Decided** 2026-11-04 under the author's direction ("go with your recommendations"), on the recommendation given the same day for issue #67. Built the same day.
- **Date:** 2026-11-04
- **Refines:** ADR-0230 decision 2 (an entry with nothing to act on is a notice), which listed no move's state map
- **Relates to:** PRD F2; ADR-0229 decision 2 (a move's `states`); issue #67

## Context

Issue #67, filed 2026-11-04 at 0.15.0, found that one source cannot carry a store at two versions when a move maps a state that an earlier version renamed. Reproduced 2026-11-04 on the published 0.15.0 (`TODO.md`, #67), on fresh SQLite stores:

1. Version 1 declares `Span`, with `OPEN` and a final `CLOSED`, and a span is closed.
2. Version 2 renames `CLOSED` to `CANCELLED`, by `removed_states`.
3. Version 3 moves `Span` to `Period`, its `states` mapping `CLOSED` to `CANCELLED`.

Over a store at version 1, version 3 published, and the span became a `Period` in `CANCELLED`. Over a store at version 2, its submit was refused: *"the move of Span to Period maps Span.CLOSED, which the previous version did not have"*. Leaving `CLOSED` out of the map would strand the store at version 1.

ADR-0230 decision 2 made each entry naming what the installed version lacks a notice, so one source serves a store at any earlier version. It listed renames, removed states and removed members, and left out a move's state map. `flow-format.md` §4.16 already said that where a publish's mapping names what the installed version lacks, *"that is the notice below, not a refusal"*.

## Decision

1. **In a publish, a move's `states` naming a state the installed old type lacks is a notice**, `migration_not_applicable:moved_types:<Type>.<state>`, naming the installed version. The move carries nothing for that state, since no object of the installed version can be in it.
2. **Given `--previous`, it stays fatal**, as ADR-0230 decision 5 keeps every other mapping, since there the previous version is the one before and the mapping is a mistake in the source.
3. **A type the installed version does not hold gives its own notice**, `migration_not_applicable:moved_types:<Type>`, as before, and no notice for its states.

## Alternatives rejected

- **Two releases in order, every store first taking the version between.** It works, but ADR-0230 exists so that one source serves every store.
- **A move's state map keyed by either name.** It adds a second way to name a state, where the notice already says what the entry carries.

## Consequences

- Built in the core's `check/migration.rs` and `publish.rs` (`not_applicable`), and in `scripts/check-flows.py` and `scripts/flowpublish.py`; `flow-format.md` §4.16 and `publish-and-import.md` say so.
- **It refuses nothing 0.15.0 accepted.** A source 0.15.0 refused now publishes over a store already past the state.
- **Tested** on both backends: the issue's three versions over a store at version 1, the span carried into `CANCELLED` with no notice, and over one at version 2, with the notice, the span carried as it stands (`a_moves_state_map_over_a_store_past_the_state_is_a_notice`). Both checkers agree on two plants: the move fatal given `--previous`, and clean over the installed version. The Flow Studio's test passes through both runners alike. Four planted defects were each caught: the check and the notice in each implementation.

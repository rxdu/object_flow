# ADR-0250: An event's read names what a later version renamed of its payload

- **Status:** **Decided** 2026-11-04 under the author's direction ("go with your recommendations"), on the recommendation given the same day for issue #62. Built the same day.
- **Date:** 2026-11-04
- **Refines:** ADR-0244 (a read names the declaration an event was written under)
- **Relates to:** ADR-0146 (a rename carries its values unchanged); ADR-0209 (an event's payload records what it wrote); ADR-0229 (a removed type's objects moved); issue #62; `docs/research/naming-the-schema-a-record-was-written-under.md` §2

## Context

Issue #62, filed 2026-11-03 at 0.12.0, found that a reader of an object's history must know every name each attribute ever had. Reproduced 2026-11-04 on the published 0.14.0 (`TODO.md`, #62): after a version renamed `Memo.a` to `b`, `get` answered `b`, and `history` answered both earlier events with `inputs` and `wrote` under `a`. Only the engine holds the renames, in each version's `renamed_attributes`.

Each event's payload is answered as written, and its `declaration_type` and `declaration_version` name the declaration that wrote it, so a gateway filtering personal values reads that declaration (ADR-0244). Avro resolves a rename when it reads, through the reader's schema; Iceberg follows a renamed column by an id that does not change; Axon maps a renamed event when it reads it. None rewrites the stored record (the research, §2).

## Decision

1. **The payload stays as written.** Its names are the ones the declaration its `declaration_type` and `declaration_version` name gave, which a gateway filtering by that declaration reads.
2. **Each event read answers `renamed` beside it:** each name its payload holds, in `inputs` and `wrote`, that a later version renamed, mapped to the installed version's name. `history`, `events` and `pull` answer it, and so does each row of `export`'s log, for the names its payload keeps. An event with no name renamed since answers none.
3. **A chain of renames is composed.** `a` renamed `b`, then `b` renamed `c`, answers `{"a": "c"}`.
4. **Each version's renames are read for the type the object was of under it,** a base's reaching its family as in a publish. A version that moved the object renames none of its names, since a moved object takes its target's by name (ADR-0229 decision 2).
5. **The core's library gives it too:** `TypesThen::renamed`, beside `TypesThen::of`.

## Alternatives rejected

- **The payload answered under the installed version's names.** The declaration the event's `declaration_type` and `declaration_version` name would no longer describe it, so a gateway filtering by that declaration would misread each renamed value.
- **Each reader keeping every name an attribute had.** It restates the engine's renames in every reader, a name more with each rename.
- **A reader reading each version's migration itself.** It restates the engine's mapping outside it, a move's skip among it.

## Consequences

- Built in the core's `world.rs` (`TypesThen::renamed`) and `export.rs`, and the service's event reads; `wire.md`, `integrate.md` and `library-api.md` say so. The reference runner keeps no event payload, so its sandbox lists `renamed` as not computed, as it lists `payload`.
- **Tested** on both backends: a memo's `a` renamed `b` and then `c`, its events answering `{"a": "c"}` and `{"b": "c"}` in `history`, `events`, `pull` and `export`, and none for an event under the installed version, before a restart and after it; and a genre merged into categories in a version renaming a category's alias, the genre's events keeping its alias's name and taking a later rename of the name. Three planted defects were each caught.

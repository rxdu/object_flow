# ADR-0230: A migration entry with nothing to act on is a notice, and a backfill runs once on a store

- **Status:** **Decided** 2026-10-25 under the author's direction ("go with your recommendations", twice: first for moves and retirements, then for the scope the kept-entry probes found), after the review of issue #41.
- **Date:** 2026-10-25
- **Supersedes in part:** ADR-0229 decision 7 (a publish refuses a moved or retired type that no module of the installed version declares); ADR-0120 decision 1 (a migration is "replaced or removed in the next" version)
- **Refines:** ADR-0147 (a backfill and the order of a publish's mappings)
- **Relates to:** PRD F2 and F5; ADR-0105 (a migration is F2's own route), ADR-0229 decisions 1 to 3 (a removed type's objects moved or retired); `docs/research/moving-objects-between-types.md` §6

## Context

**The issue.** Issue #41, filed 2026-10-25, found that one flow source cannot be published both to a new store and to an older one. A consumer that keeps its flows as code publishes the same source to every test's empty store, to a new deployment's, and to stores at older versions. With a move in the source's `migration`, the empty stores refuse it; without the move, the older stores refuse it `removed_type`.

**What was reproduced.** Each case ran on 2026-10-25 against the published v0.6.0 (`eff4a6d`), on a file store made with `--create`:

- A `moved_types` or `retired_types` entry naming a type the installed version lacks is refused `moved_type`: over an empty store, and over a store that retired the type, the second by the rule 0.6.0's security review added.
- A kept entry, published again after it applied, is refused: `moved_types` with `moved_type`; `renamed_attributes`, `removed_states` and `removed_members` with `check:names`, "which the previous version did not have". Over an empty store these three publish.
- A kept `backfill` publishes and runs again at every publish: `if count is null then 1 else count + 1` took a count from 10 to 11, 12 and 13 over three publishes of one source.
- Over a store that retired the type, the move's dry run would carry the retired objects into the target, so a source naming the type revives what the store retired.

**What the record decided.** ADR-0120 writes a migration with the version it publishes, "replaced or removed in the next". ADR-0229 decision 7 refuses a move or retirement of a type that no module of the installed version declares. Both assume each store takes every version in turn.

**The constraint.** PRD F2: governed state changes only through a declared route, each recorded. An entry that acts on nothing writes nothing, so a notice in its place opens no route. What protects a store's live objects is the rule that a removed type, state or member holding them needs a mapping (ADR-0229 decision 1, `flow-format.md` §4.16), which stays.

**How tools do it** (`docs/research/moving-objects-between-types.md` §6). Terraform's `moved` blocks are declared in the configuration and kept there: a module instance that never had the old address ignores the block, and HashiCorp recommends retaining every historical block, since removing one is a breaking change. Liquibase records each changeset it ran in its DATABASECHANGELOG table, and Flyway each migration in its schema history table, so neither runs one twice; Flyway's repeatable migrations run again when their checksum changes.

## Decision

1. **A migration may stay in the source.** It is written with the version that first publishes it and may be kept in later ones, as Terraform's `moved` blocks are. One source then serves an empty store, a store at any earlier version, and one that has taken the change.
2. **In a publish, an entry with nothing to act on is a notice**, `migration_not_applicable:<key>:<name>`, and changes nothing: a `moved_types` or `retired_types` entry whose type the installed version does not declare, or holds retired; a `renamed_attributes` entry whose attribute the installed type lacks; a `removed_states` or `removed_members` entry whose state or member the installed version lacks. The notice names the installed version, and for a type says whether that version never declared it or holds it retired. A retired type's objects stay retired.
3. **An entry that acts covers every object, as before.** A removed type holding objects not in a final state still needs a move or a retirement, a removed state or a removed member that objects hold still needs a mapping, and each object of a moved type is taken by exactly one entry. Each is fatal where it fails.
4. **A backfill runs once on a store.** A backfill the installed version's migration holds, its expression unchanged, is not run again, a notice, `migration_applied:backfill:<Type>.<attribute>`. A store that missed the version that brought it runs it at the publish that reaches it, and a changed expression runs again, as Flyway runs a repeatable migration whose checksum changed.
5. **The checker given `--previous` keeps its findings.** There the previous version is the one before, so a mapping naming a state, member or attribute it lacks is a mistake in the source, fatal; a moved or retired type it lacks is a notice, since another module may declare it (ADR-0229 decision 7). A publish runs the checker against the installed version (`check::checked_installed`), and leaves these to decision 2.

## Alternatives rejected

- **Moves and retirements alone** (the issue's proposal without its second bullet). A kept rename, removed state or removed member is refused again once it has applied, so a source with any of them still cannot serve both stores.
- **Saying in the documentation that a kept backfill must be idempotent.** Whoever forgets meets a value rewritten at every publish; the store already records which backfill ran, in its installed version's migration.
- **A table of applied backfills**, as Liquibase's DATABASECHANGELOG. The installed version's migration already records it, and a table would be a second record of one fact.
- **Every version's source published in turn** (the issue's first alternative). Every test publishes the history first, and every past source stays alive: the burden a declared migration exists to remove.
- **Stripping the migration before publishing to a new store** (the issue's second). What was tested is then not what was published.
- **A publish option for a store at version 0** (the issue's third). It serves new stores, not a store that already took the change.
- **A kept move carrying a retired type's objects.** It would revive objects the store retired, on the strength of a source that only names the type.
- **The checker's `--previous` accepting a mapping the previous version lacks.** It would hide a mistyped name where the previous version is the one before.

## Consequences

- `flow-format.md` §4.16 says a migration may be kept, what an entry with nothing to act on reports, and that a backfill runs once on a store; `publish-and-import.md` §1 names both notices; `CHANGELOG.md` lists the change as accepting what 0.6.0 refused, and a kept backfill no longer running again.
- The core's publish and the reference runner's report both notices and skip both kinds of entry; both checkers leave the three name rules to the publish when checking against the installed version, and the plants the checker exports say which way each was checked. Tested in the core's own tests and through both backends.
- A mistyped attribute in a kept rename, over a store holding the real one, is not applicable: the real attribute is dropped as a forgotten rename drops it, with the notice naming the entry. The checker's `--previous` refuses the typo where the change is authored.
- A consumer keeps an entry as long as any store it serves may be behind it, as Terraform's guide advises for `moved` blocks; a store further behind than the source reaches takes the change only through a source that still holds it.

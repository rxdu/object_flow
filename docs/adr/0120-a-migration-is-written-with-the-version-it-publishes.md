# ADR-0120: A migration is written with the version it publishes

- **Status:** Decided at the author's direction, 2026-09-26 ("go ahead with the rest constructs"); the author's own acceptance pending.
- **Date:** 2026-09-26
- **Refines:** ADR-0116 (the format replaces the text language), ADR-0099 (a publish changes a live object only by a recorded migration)
- **Relates to:** ADR-0027 (declaration versions and mappings), ADR-0101 (migrations reach terminal objects), ADR-0105 (a migration is checked against every invariant), `declaration-syntax.md` §6.6, `flow-format.md` §4.16

## Context

A publish that changes a type under live objects moves them by a recorded migration: a removed state is mapped to one that remains, a removed enumeration member to one that remains, a renamed attribute to its new name, a new required attribute is backfilled, and an invariant the migrated objects break may be admitted with a reason (`declaration-syntax.md` §6.6). The model writes these as lines "at the top level of a publish", and its line for a removed state names no type: `removed state LEGACY_HOLD -> AVAILABLE`. Construct 9 of ADR-0116's plan gives them a written form.

Two facts shape it. A migration belongs to one publish, from the version before to this one, and means nothing once published. And most of what the model's checks 22 and 23 decide needs the previous version, and some of it the live objects: whether a new required attribute needs a backfill depends on whether the type has any.

## Decision

1. **A migration is a section of the module it publishes**, `migration`, the last in the module's order since it names the types and enumerations above it. It is written with the version it publishes and replaced or removed in the next, as the model writes it at the top level of the declaration published.
2. **A mapping names its type.** `removed_states` and `renamed_attributes` and `backfill` are grouped by type, `removed_members` by enumeration, since two types may share a state's or an attribute's name. The model's own line names none; that is recorded as a defect of its grammar in `TODO.md`, and the conversion writes the model's line.
3. **The format checks a migration twice.** Against this version alone, at every run: what a mapping names is gone from this version and what it maps to is here, a backfill writes a stored attribute and reads the object's own members and literals, and an admission names a real invariant and reason. Against the version before, given as `--previous=<file>` and in the examples' version pairs: every removed state is mapped, every mapping names what that version had, and a field added to an observation kind is optional (the static part of checks 22 and 23).
4. **What only the live objects can decide is a notice**, `migration`: a new required attribute with no backfill, a removed member with no mapping, and an attribute gone where one of its type is new, which may be a rename. The publish, which counts the objects, refuses them where the model says so.

## Alternatives rejected

- **A migration in a file of its own**, beside the module. It keeps the module free of history, but a publish would then be two files that must travel together, and the model writes the mapping in the declaration it publishes.
- **The model's lines as they are, naming no type.** They are ambiguous in a module whose types share a state name, as `issues.yaml`'s and `servicedesk.yaml`'s do.
- **Refusing a new required attribute without a backfill outright.** A type with no live objects needs none, and the checker cannot know; a refusal would make an author invent a backfill no object will receive.
- **Checking the migration only at publish.** An author would then learn of an unmapped state only when publishing; the previous version is in the repository beside the new one.

## Consequences

- `flow-format.md` §4.16 describes the section, §3 puts it last in the module, §7 adds the `migration` notice, and Appendix B marks migrations written.
- `scripts/check-flows.py` takes `--previous=<file>`, and checks `inventory-v2.yaml` and `service-v2.yaml` against their first versions. `service-v2.yaml` now carries the constructs its first version gained, and a migration that renames an attribute, backfills a new one, merges an enumeration member and admits an invariant.
- `TODO.md` records the model's type-less migration lines as a defect of its grammar.

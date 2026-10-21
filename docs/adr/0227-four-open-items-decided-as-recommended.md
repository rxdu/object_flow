# ADR-0227: Four open items decided as recommended

- **Status:** **Decided** 2026-10-21 under the author's direction ("go with your recommendations", on the four items `TODO.md` held for the author that day).
- **Date:** 2026-10-21
- **Refines:** ADR-0211 decision 2 (the repository's settings), as applied
- **Relates to:** ADR-0116 and `flow-format.md` §2 (the schema), §4.5 (what a state requires); ADR-0180 (format 1 only adds); `procurement-second-review.md` E3 and I2-5

## Context

Four items in `TODO.md` waited on the author: the release-tag ruleset ADR-0211 decision 2 chose and nobody applied; the flow format's schema, its `$id` relative "until the schema is published at a stable address", deferred to the first release, which is out; the second procurement review's E3, a guard proving an attribute present not counting for what a state requires; and its I2-5, a query's `fields` following references.

## Decision

1. **Release tags are protected**, as ADR-0211 decision 2 (b) decided: `scripts/setup/github-settings.sh` made the ruleset "release tags", active, refusing the creation, move and deletion of a `v*` tag to all but the repository's administrators.
2. **The schema's address is the latest release's download**, `https://github.com/rxdu/object_flow/releases/latest/download/flow.schema.json`, its `$id`. The release workflow publishes `flow.schema.json` beside each release's archive, so each release's own is at `releases/download/v<version>/flow.schema.json`; v0.5.1's own was attached to it the same day, so the address answers before the next release. Format 1 only adds (ADR-0180), so the latest schema validates every format-1 description.
3. **A `deny` guard requiring an attribute present sets what a state requires** (E3): for a transition from a state, a `deny` guard of its own whose condition has `<attribute> is not null`, or `this.<attribute> is not null`, among its top-level `and` terms counts as setting the attribute, where the effect neither clears it nor assigns it. The guard is decided before the effect, on an object that already holds its values, so the attribute is present as the state is entered. Both checkers apply it, and the finding names the guard as a way to meet it.
4. **A query's `fields` do not follow references** (I2-5), until a consumer's question needs it: a second query, `id in [...]`, reads the referenced objects, and a caller joins them.

## Alternatives rejected

- **The schema at `raw.githubusercontent.com/…/main/…`**: it serves what `main` holds, edits no release has shipped; **a site of the project's own (GitHub Pages)**: a setting and a site to keep for one file; **a `$id` per release**: format 1 is one schema, and every release would change its name.
- **E3 left as it was**: the reviewer weakened the state's invariant to pass the check, so the rule asked for less than the flow meant; **any guard reading the attribute**: a guard `a is null or b` lets a request through with `a` absent; **a `warn` guard**: it refuses nothing.
- **I2-5 built now**: a field across a reference reads another type's object, its personal values under that type's marking, for each row, a cost and a rule for a convenience no consumer has asked for.

## Consequences

- GitHub: the ruleset "release tags" (id 25532543); a release asset `flow.schema.json` on v0.5.1. `.github/workflows/release.yml` builds and publishes the schema with each archive.
- `flow.schema.json`'s `$id`; `flow-format.md` §2 (the address) and §4.5 (the presence guard); `write-a-flow.md` (pointing an editor at the schema).
- `check-flows.py` `guarded_present` and `core`'s `check/names.rs` `guarded_present`; `guarded_presence_plants`, six cases, exported to the core's oracle: two clean, a whole condition and an `and` term through `this`, and four still refused, a `warn` guard, a presence under `or`, an effect clearing the attribute and one writing it from what may be absent. The oracle fails with the core's half removed.

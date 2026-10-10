# ADR-0177: An engine release's upgrade, as built

- **Status:** Built at the author's direction ("go ahead with the upgrade", 2026-10-10), for the author's review.
- **Date:** 2026-10-10
- **Refines:** ADR-0110 §4 (an upgrade between releases), ADR-0174 decision 3 (a store of another release refused)
- **Relates to:** `design/storage-schema.md` §6 and §12; `design/library-api.md` §7 (`EngineMismatch`); `DESIGN.md` §2; `design/procurement-second-review.md` §2

## Context

Both second-round procurement reviewers named the missing upgrade between engine releases the first thing standing between a pilot and production: a store a release creates is refused by the next (ADR-0174 decision 3), so the first engine fix after a pilot begins would strand its data. `storage-schema.md` §12 specifies the step, `upgrade`, run by a core of the new release over the store in one transaction, and how a deployment drains the old release's copies.

Building it found three things the specification assumes and the core does not have.

1. **The release table.** §6 declares `of_engine_release`; the core never created it, and recorded a store's release only as version 0's `of_declaration.builtins`.
2. **A release that means something.** The release is `objectflow-core <workspace version>`, and the workspace version has been 0.1.0 since the core's first commit, through every change to the store's tables and to how values are stored. It was observed to matter: `c494d5c` (D559) changed how a `file` column is stored, from a name to the reference's JSON, so a deployment store holding a file, made by the build of the day before, does not read back under the build after it, and both call themselves 0.1.0.
3. **A copy that starts.** The specification's drain starts copies of the new release that refuse with `EngineMismatch` until the step runs from one of them; ADR-0174 decision 3 refuses to start on a store of another release at all.

## Decision

1. **A release is the workspace version, and it moves exactly when what a store holds or means changes**: the store's own tables, the tables a type maps to, how a value is stored in a column, or the built-in types and standard metrics. Every such change is a new release with a migration from the one before, including a change made between two pilots' installs, since any build can create a store. Two tests hold it: the current release records a fingerprint of the store's own schema and the built-ins as the store creates them, which fails a change to either made without a release; and stores a real earlier release made, kept as fixtures with what that release answered of them, are upgraded and must answer the same, which catches a change in how a value is stored that no fingerprint sees.
2. **The releases are a registry in the core**, `objectflow-core::release`: each release names the releases it upgrades from, its fingerprint, what it changes, in a sentence for the report, whether it changes the built-ins, and its migration. A store further behind is upgraded through the releases between, one step each, each its own transaction. **This release, 0.2.0**, is the first with an upgrade: its migration from 0.1.0 creates `of_engine_release`, recording 0.1.0 as the release that created the store, when the store's operator was installed, and itself as installed by the upgrade.
3. **Every operation reads the store's release in its transaction** and refuses with `EngineMismatch` where it is not this core's, so once an upgrade commits, no copy of the release before it writes or reads again. A read of the store as it stands, a request's, a check's, a maintenance task's and an attempt's row each read it. Release 0.1.0 reads no release, so an upgrade from it is made with its copies stopped, which the guide says; from 0.2.0 on the drain of §12 holds.
4. **A service started on a store of a release it upgrades from starts, and serves only the upgrade**: `GET /health` answers both releases and `upgrade_due`, every other operation is refused `EngineMismatch`, a `503`, and **`POST /v1/upgrade`** `{actor, dry_run}` runs the step. On a store of a release it does not upgrade from, a later one among them, it still refuses to start, so there is no downgrade (amending ADR-0174 decision 3). The upgrade is the operator's act: its actor is an object of the built-in `Operator` type, and each step records an `upgrade` event on that operator, naming the release, beside the release row.
5. **A dry run runs every step and rolls it back**, so a migration that would fail is found before the store is touched, and answers the report the step would: from which release to which, each step and what it changes. A store at this release already answers that nothing is due.
6. **Step 3 of §12, the installed flows re-recorded under new built-ins, is not built**: no release yet changes the built-ins. A release that says it does is refused by the upgrade, naming this decision, rather than installed without the check, and is built with the first such release.

## Alternatives rejected

- **Keep 0.1.0 and fingerprint only.** A fingerprint sees the schema's text, not how a value is stored, which is the change D559 made; and a release that never moves cannot name what it upgrades from.
- **An upgrade as a command-line step, the service never started on an old store.** Simpler for one machine with one copy, and the shape a single SQLite file suggests; but the specification's drain needs copies of the new release up and refusing, a PostgreSQL deployment will have several, and two paths to one step are two to keep alike.
- **Re-record the installed flows on every upgrade.** It would write a new declaration version for a release that changes nothing a flow reads, and the version is the record an auditor reads for when the rules changed.
- **A generated fixture store, rather than one a real release made.** A store emulated by the current code shares its mistakes; the fixtures were made by release 0.1.0's own binary, at commit `f6ee6d6`, by `core/crates/objectflow-service/tests/fixtures/make-release-store.py`.

## Consequences

- The workspace version is 0.2.0; `of_engine_release` is created with every store and by the upgrade from 0.1.0.
- `storage-schema.md` §12 says what is built and that step 3 waits for the first release changing the built-ins; `DESIGN.md` §2, `library-api.md` §7, the wire reference, the integration guide and the core's README say how a deployment upgrades.
- A change to the store's tables, a type's mapping, a value's storage or the built-ins now fails the fingerprint test until it is a release with a migration, which `core/README.md` says how to add.
- A deployment store made before `c494d5c` holding a file value does not upgrade: its file column holds a name, which no release reads. None exists outside the reviewers' scratch directories.

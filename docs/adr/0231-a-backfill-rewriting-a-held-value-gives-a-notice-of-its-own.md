# ADR-0231: A backfill rewriting a value an object holds gives a notice of its own

- **Status:** **Decided** 2026-10-26 under the author's direction ("go with your recommendations"), after the comment on issue #41.
- **Date:** 2026-10-26
- **Refines:** ADR-0147 (a backfill and the order of a publish's mappings); ADR-0230 decision 4 (a backfill runs once on a store)
- **Relates to:** PRD F2; `docs/research/moving-objects-between-types.md` §7

## Context

**The comment.** Issue #41's filer commented on 2026-10-25 that a backfill names every object it writes under one notice, `mapping:backfill:<Type>.<attribute>`, whether the object held no value or one the backfill replaces. A backfill written for stores of the version before an attribute, published over a store already past it, rewrites what the store holds under the notice a fill gets. In the filer's case a person who joined as `COLLABORATOR` then read `STAFF`, which widened what the person may access.

**What was reproduced.** Probed 2026-10-26 on the published v0.7.0, on a file store at a version holding `membership`, its migration without a backfill of it, one object holding `COLLABORATOR` and one `STAFF`. Published with the backfill `Level.STAFF`, the first object was rewritten and named under `mapping:backfill`, as an object filled from no value is; the second already held `STAFF`, so it was neither written nor named. Published again, the backfill wrote nothing (ADR-0230 decision 4). Guarded, `if membership is null then Level.STAFF else membership`, it wrote nothing either time.

**The constraint.** PRD F2: governed state changes only through a declared route, each recorded. A backfill is a declared route, and its writes are recorded on each object's migration, so a rewrite is not ungated. The gap is in review: the dry run's report is what the approver reads before the version applies, and a rewrite there reads as a fill.

**How tools do it** (`docs/research/moving-objects-between-types.md` §7). Terraform's plan previews a change before it is applied, and marks a value set where there was none apart from one changed: `+ "Name" = "api"` against `"ami-02c…" -> "ami-0d7…"`, with a summary counting "2 to add, 1 to change, 2 to destroy".

## Decision

1. **A backfill that replaces a value an object holds gives a notice of its own**, `backfill_rewrites:<Type>.<attribute>`, naming each such object. An object holds a value where its attribute has one as the mappings before the backfill left it: a value a rename or a move carried counts, and so does a set, empty or not. An attribute the version adds, or whose type it changes, starts absent (`storage-schema.md` §10), so a backfill of it fills. A backfill giving no value where one was held is a rewrite.
2. **Beside `mapping:backfill`, not in place of it.** `mapping:backfill:<Type>.<attribute>` still names every object the backfill writes, filled or rewritten, so whatever reads it for what a backfill writes is unchanged. `backfill_rewrites` names the objects among them that held a value.
3. **A notice, not fatal and not a decision.** A deliberate rewrite is legitimate, such as a correction of values a version means to change, and the publish's approval admits it. A backfill meant to fill only an absent value is guarded, `if x is null then … else x`, as `flow-format.md` §4.16 says.
4. **The report's alone.** The object's migration records `mapping:backfill`, as before; `backfill_rewrites` is for the approver and changes nothing a publish records.

## Alternatives rejected

- **A code of its own for a rewrite in place of `mapping:backfill`.** Whatever reads `mapping:backfill` for what a backfill writes would stop seeing the rewrites, the writes it most needs to see; the store records an object's `mapping:` codes on its migration, so a rewritten object would lose that record too.
- **Refusing a rewrite, or holding it as a decision.** A backfill gives every object of its type a value, and correcting held values is one of its uses; a refusal would make a deliberate correction guard against itself, and the approval already admits the publish.
- **One notice per attribute with a count, naming no object.** The filer asked for the objects, and every other mapping notice names them.
- **Comparing with the value the installed store holds under the attribute's name.** A value a rename carries is held under the new name only once the mappings run, so compared with the store it would read as a fill.
- **The guarded form alone, said in the documentation.** The guard is the author's to remember; the report is the approver's to read, and it could not tell the two apart.

## Consequences

- `flow-format.md` §4.16's `backfill` row and `publish-and-import.md` §1's notices name `backfill_rewrites`; `CHANGELOG.md` lists it under the next release.
- The core's publish and the reference runner's report it alike, tested in the core's own tests on both stores and through both backends.

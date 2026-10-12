# ADR-0181: The hiring review's findings decided

- **Status:** **Accepted** by the author, 2026-10-11, by ADR-0182 ("go with your recommendations on the fifteen questions"). Before: Built at the author's direction ("go ahead and address all of them", 2026-10-11, on a plan whose last step was the third cold review), for the author's review; the questions that reverse an accepted decision are put to the author in `design/before-1.0.md` §4.
- **Date:** 2026-10-11
- **Refined by:** ADR-0190 — as for an assertion and an erasure, a correction's `taken_by` record is exempt from what it must write exactly.
- **Refines:** ADR-0176 decision 1 (`taken_by`), ADR-0179 decisions 1 and 3 (a refusal's objects, a query's filter), ADR-0180 decision 1 (what is promised), ADR-0174 decision 2 (the store's file)
- **Relates to:** `design/hiring-third-review.md`; `design/defects.md` D578 to D592; `DESIGN.md` §5.5, §10; `design/flow-format.md` §3, §4.8; `guides/wire.md`; `guides/integrate.md` §9

## Context

Two agents new to ObjectFlow built a hiring system's flow and its application from the documentation at `874457c`, the third cold review (`design/hiring-third-review.md`). Both judged it buildable and fit for a pilot, and not yet for production. Each claim was reproduced before it was classified, and one was wrong as stated (§3 of the review). Five findings asked for a decision rather than a repair: which transitions `taken_by` may mark, what a refusal says about an object its own request was creating, what an erasure leaves in the store's files, whether a query's filter is typed, and whether a sandbox's controls are promised.

## Decision

1. **`taken_by` marks any transition a request takes, an erasure and an assertion included** (D579). ADR-0176 limited it to initial, external and internal transitions, which left the erasure of a person in every agent's tools, and an override too; an erasure and an assertion record `actor.kind` as any effect does, so the sugar applies unchanged. No `may_admit` admits an invariant `taken_by` generates: who may take a transition is never admitted.
2. **A refusal names no object that does not exist** (D581): a failure on the object the refused request was creating carries `object_id: null`, and a refusal's `objects` holds only objects the store holds, since a refused creation's id is the next object's. An invariant refusal tells `proposable`, as an unsatisfied one does (D582), since a `taken_by` refusal of a proposable transition is exactly where an agent should propose.
3. **A query's filter is typed against its literals** (D580): one comparing a member with a literal its type cannot hold, a time with text, a name with a number, the state with text, is refused `malformed_filter`, as a reference compared with text already was (D556); a comparison the values still cannot make is refused the same way, never a `501`. There is no timestamp literal: a time is compared with `now`.
4. **An erasure leaves nothing it erased in the store's files** (D584): a deployment's connection runs with `secure_delete`, so the pages a write rewrites hold no freed value, and the write-ahead log is truncated after an erasure commits, so its earlier frames, which held the values, leave the files too; a truncation a reader blocks is due again after the next write. A backup taken before the erasure still holds them, which `integrate.md` §9 says.
5. **An attribute named `<reference>_id` beside a single reference is refused** (D578), by both checkers and by the core's load, since the store keeps the reference in that column; the core alone found it before, as a `500`.
6. **ADR-0180's promise covers a sandbox's controls** (`/sandbox/load`, `reset`, `clock`, `evaluators`), since an application's tests of its time rules and its evaluators run on them, and the generated tools' names, which follow their rule (`renderers.md` §3); the import batch's JSON is `/v1`'s already, as `import_batch`'s body. The scripts (`mirror-version.py` and the rest) are not promised, and moving a store to another backend is not an upgrade: the release that brings PostgreSQL says how a store moves.

## Alternatives rejected

- **Leaving erasure out of `taken_by` and filtering it from an agent's tools by kind**: every erasure would be withheld from every agent, a decision the flow, not the engine, makes; and an assertion would stay unguarded.
- **Never reusing a refused creation's id**, by drawing ids outside the transaction: a refused request would then leave gaps in every type's ids and write outside its transaction, where naming the object by no id says the truth at no cost.
- **A full type check of a filter**, by the checker's typing over the modules' source on every query: the narrower check refuses every case the reviewers met and reads only the loaded library; a derived attribute compared wrongly is still refused, when the comparison fails.
- **Documenting erased bytes as the operator's**, with `VACUUM` after an erasure: a full rewrite of the store per erasure, where `secure_delete` and a truncation touch only what changed.

## Consequences

- `flow-format.md` §3 and §4.8, the card, `library-api.md` §4 and §5, `DESIGN.md` §5.5, `wire.md`, `integrate.md` and `renderers.md` say each.
- ADR-0180 decision 1 is amended to name the sandbox's controls and the tools' names, and `guides/wire-v1.json` and its test hold the sandbox's controls too.
- `design/before-1.0.md` puts six questions to the author: agents denied by default, an actor in a final state refused, filters on members no index serves, support and deprecation windows, JSON held as text in two built-in attributes, and engine-owned timers.

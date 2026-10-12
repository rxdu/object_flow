# ADR-0187: The third production review's two decisions

- **Status:** **Decided by the author**, 2026-10-12 ("go with your recommendations on questions 7 and 8"), on the recommendations `TODO.md` states with each, after the third production review (`design/production-review.md` §7). Both built the same day.
- **Date:** 2026-10-12
- **Refines:** ADR-0182 (`taken_by`), ADR-0183 decision 7 and ADR-0184 decision 1 (the store's own acts), ADR-0126 (the built-in `DeclarationChange`)
- **Relates to:** `PRD.md` T6, N7, UC-15; ADR-0176; `flow-format.md` §4; `guides/integrate.md` §6, §9

## Context

The third production review left two majors that needed the author.

**AGENT-APPROVE.** All three reviews found that an agent, through the wire, can approve a `DeclarationChange` and install a version. Only the gateway stands in the way. The author twice left this to the application: PRD UC-15 says "the upper layer deciding who may", and ADR-0183 decision 7 and ADR-0184 rejected a refusal by the requester's kind as one PRD T6 and N7 forbid.

**MEMORY-2.** After a publish on the 37k-object store, the service held 975 MB and never gave it back. Measured 2026-10-12: the same process held 23 MB once `malloc_trim(0)` was called through a preloaded test library. So nothing holds that memory alive; glibc keeps what was freed in its per-thread arenas. Its tunables (`MALLOC_ARENA_MAX`, `MALLOC_TRIM_THRESHOLD_`, `MALLOC_MMAP_THRESHOLD_`) measured no different, since only an explicit trim returns free pages from the middle of an arena.

## Decision

1. **A store's flow may declare who approves its versions.** A module declares `publishing: { taken_by: [<kind>, …] }`. The approval of a version, the built-in `DeclarationChange`'s `publish`, is then taken only by an actor of those kinds, exactly as a transition's `taken_by` works (ADR-0176, ADR-0182). An actor of another kind is refused `invariant_violated`, naming the invariant `publish_taken_by` on the change, with the remedy `delegable`.
   - **Which declaration counts.** The rule is read from the **installed** version, never from the one under approval, so a change cannot remove its own rule and then be approved under it.
   - **No declaration, no refusal.** Where no installed module declares it, nothing is refused and the gateway decides, as before.
   - **Several modules.** Modules of one version that declare it must declare the same kinds, or the checkers refuse the version.
   - **Why the PRD allows it.** The engine reads an actor's kind only where the flow's author declared it, as `taken_by` already does, so no PRD revision is needed: the author is the upper layer's voice in the flow.
2. **The service gives back what a heavy request freed.** After a `publish`, an `import_batch`, or any request that ran 0.5 s or longer, the service calls glibc's `malloc_trim(0)` on a blocking thread, one call at a time, after the answer has been made.
   - **Dependencies.** `libc` (0.2.190, already in the lockfile through tokio) becomes the service's direct dependency, which adds no crate.
   - **The `unsafe` call.** There is one call. The workspace's `unsafe_code` goes from `forbid` to `deny`, so the service's one `allow` can stand beside its safety comment, and clippy's `undocumented_unsafe_blocks` is denied everywhere.
   - **Where it runs.** Only on glibc targets; elsewhere nothing is trimmed.
   - **What `/metrics` shows.** `objectflow_memory_given_back_total` counts the trims.

## Alternatives rejected

- **(1a) The gateway alone, as decided twice.** Rejected now: three reviews showed that one missed gateway rule installs a version, and the flow's author had no way to say otherwise.
- **(1c) A deployment's flag naming the kinds that approve.** Rejected: it would put a second policy channel outside the flow, unrecorded in the version and absent from the reference runner.
- **(1) Reading the rule from the version under approval.** Rejected: a change could drop its own rule.
- **(2b) A global allocator that returns memory itself, mimalloc or jemalloc.** Rejected: a new crate with C built from source, where one call into the allocator already linked did the same: 975 MB to 25 MB measured.
- **(2c) A scheduled restart, documented.** Rejected: it is a workaround that drops connections and adds a moving part.
- **(2) A background trim on a timer.** Rejected: it trims whether or not anything was freed. Trimming after the request that freed is the cheaper way to the same effect.

## Consequences

- **Decision 2, measured 2026-10-12 with a release build.** Ten seconds after a publish, the service holds 25 MB on the 37k-object store, where it held 975 MB, and 36 MB on 110k, where it held 2.95 GB. A publish's peak is unchanged (about 1.1 GB at 37k objects, 3 GB at 110k), and `integrate.md` §9 sizes a unit by it.
- **No Miri job, though `rust.md` calls for one over code with `unsafe`.** Miri cannot execute a call into glibc, and this block reads no pointer, so the tests run it natively instead.
- Held by `what_a_heavy_request_freed_is_given_back`. In it, resident memory rose 12 MB over a submit and approval of 600 types with the giving back, and 62 MB without it. Two mutations each fail it: no trim, and a publish that triggers none.
- Decision 1 adds `publishing` to the flow format (§4.1, §10, the card), and to both checkers, which refuse modules that disagree (`names`). It also reaches the engine's approval and the reference runner's, which refuse before the dry run is computed again, so a refused approval costs nothing.
- **The store's operator always may approve.** The decision did not state this exception; it was added while building it. The operator's kind is `human`, so a version declaring `[agent]` or `[service]` alone would otherwise shut out every approver wherever no actor of those kinds exists, with no way to undo it.
- `integrate.md` §6, `wire.md` and `DESIGN.md` §9 and §10 say all this, and the samples declare `[human]`.
- Decision 1 is held by `a_version_is_approved_only_by_the_kinds_the_installed_one_declares`, by `test_a_version_is_approved_only_by_the_declared_kinds_alike` (both backends), and by a plant of two modules disagreeing that the oracle holds in both checkers. Each of five mutations fails one of them.

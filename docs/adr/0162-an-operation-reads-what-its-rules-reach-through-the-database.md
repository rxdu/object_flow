# ADR-0162: An operation reads what its rules reach, through the database

- **Status:** Proposed, 2026-10-06, for the author's discussion; nothing in it is decided. The author asked for a design note and an ADR on reads that load only what a request reaches and on PRD C5's target ("go with your recommendations"), and decides both.
- **Date:** 2026-10-06
- **Relates to:** ADR-0039 (serialisable isolation), ADR-0090 (concurrency on each backend), ADR-0132 (the core in Rust); `DESIGN.md` §2; `design/storage-schema.md` §5, §6; `design/scoped-reads.md`; PRD C5, N3

## Context

Every operation of the core loads the whole store into memory before it runs: every object, every span, the whole event log, the attempt log, the idempotency records and the rollup (`design/scoped-reads.md` §1). It was the first slice's choice, never a decision. Measured with `core/crates/objectflow-core/examples/probe_scale.rs`, one load costs about 20 µs an object, a request about two loads and a metric read about three: at 4,000 objects a request took 149 ms and a metric read 276 ms, growing linearly with the store's total size, history included, and not with what the operation reads (`design/scoped-reads.md` §2). At the first consumer's scale it is inside interactive time; at N3's upper range, and with every year of history, it is not. C5's target was left to be set by measurement, and the measurement now exists.

## Decision (proposed)

1. **An operation reads what its rules reach, through the database.** Objects by id from their type's table, held for the operation; a type scan as a query on its type's table; intervals, writes, attempts and events by the indexes `storage-schema.md` §5 and §6 lay out for them; idempotency and external identifiers by key. Every read reaches the database, so ADR-0039's guarantee holds on both backends.
2. **It is reached in steps, each measured with the probe and each standing alone:** the log, the attempt log, the idempotency records and the rollup read by query, never preloaded; then objects fetched by id; then scans, uniqueness and metrics by index.
3. **The reference runner stays a whole-store evaluator**, the oracle for what an operation decides, and the replays and the whole-answer comparison hold every step to the same answers.
4. **C5's target is set by the author.** Proposed for discussion: a request and a `get` within 100 ms and a metric read within 1 s, at the 95th percentile, on the deployment's hardware, over a store of N3's upper range with several years of history; the store size is the author's statement about the first consumer's growth.

## Alternatives rejected (proposed)

- **Keeping the whole store in memory between operations and applying each change to it.** Cheapest to build, and sound on SQLite, but on PostgreSQL with several copies of the service, which `DESIGN.md` §2 allows, a decision taken on an in-memory copy escapes serialisable isolation unless each request re-reads what it read, and its memory grows with history for good.
- **Planning each transition's reach from the declaration and loading those types whole.** Any rule with a scan or a metric reaches its whole type, which is most of the cost this removes.
- **Keeping the whole-store load and setting C5 where it holds.** The first release meets interactive time at the first consumer's scale, but the cost grows with history without bound and every application on the engine inherits it.

## Consequences (if accepted)

- `World` becomes a view that fetches, and every walk of its maps a fetch; the evaluator's semantics do not change, which the replays and the comparison check at every step.
- `TODO.md`'s entry on whole-store reads becomes the three steps, each with the probe's figures before and after.
- C5 becomes verifiable, and the probe, given the target's store size, its test.

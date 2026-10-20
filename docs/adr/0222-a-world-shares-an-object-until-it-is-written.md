# ADR-0222: A world shares an object until it is written

- **Status:** **Decided** 2026-10-20 under the author's direction ("go with your recommendation", on the recommendation to copy only what a version changes, over smaller objects first), for `TODO.md`'s MEMORY.
- **Date:** 2026-10-20
- **Refines:** ADR-0162 step 2 (a world holds the objects an operation reads and makes); ADR-0165 (an object's record beyond its row, read on first use)
- **Relates to:** ADR-0187 decision 2 (what a heavy request freed is given back); `operate.md`, Memory; `scripts/profile/publish-memory.py`, `massif-sites.py`

## Context

A publish holds every object of the store while its dry run simulates the version, and again while its approval simulates and installs it. Measured on the two-year profile store (36.6k objects, a 208 MB file) with an optimised build, after the install's own copy was moved rather than cloned and each object's spans held at their length (`d93a820`): a dry run peaked at 509 MiB resident and an approval at 530 MiB. At the approval's peak massif counted 383 MiB of heap: the store read whole 39%, and a copy of every object 37%, which the mapping made of each object before applying the version's mappings, whether a mapping then wrote it or not. A version that adds an optional attribute, the commonest kind, writes no object, yet copied the store.

The mapping copied because a world owned its objects (`Box<Object>` in its slots), so the simulation's world could not hold the store's own object beside the world that read it.

## Decision

1. **A world holds each object behind an `Rc`**, and a change through `World::get_mut` copies it first where another world holds it (`Rc::make_mut`). `World::shared` hands out a world's object, `World::insert_shared` holds one another world may hold too, and `World::into_shared` takes the objects out still shared. A world's other readers see `&Object` as before. Safe Rust gives no way to write through a shared `Rc` without that copy, so a write in one world is never seen in another: the separation holds by construction, not by care.
2. **A publish's mapping starts from the store's own object** and copies one only where a mapping step writes it: a rename of an attribute or of a tracked member's spans it holds, a retyped attribute it holds, a removed enumeration member or state it holds. The simulation holds the rest shared, a backfill copies what it writes through `get_mut`, and the install receives both the simulated objects and the old ones shared, so an object the version leaves as it was is held once.
3. **The cost accepted: a simulation reading shared objects reads them where the store's read laid them out.** `read_all` builds each object's attributes in one pass and its spans and write index in later ones, where a copy laid each object out together, and the simulation, which reads every object's attributes and spans many times, is slower over the scattered ones. Measured: with a version writing no object, a dry run's simulation 2.11 s to 2.71 s and the whole submit 3.8 s to 4.0 s, an approval 4.7 s to 5.0 s; with a backfill writing every claim, no slower (4.0 s to 3.8 s, 6.3 s to 6.2 s), the copies laid out fresh. A publish is an occasional act, and its peak is what a deployment's `MemoryMax` must hold.

## Alternatives rejected

- **Copying every object, as before:** a second copy of the store held for every publish, 37% of an approval's peak, for no version that leaves its objects as they were.
- **`Arc` in place of `Rc`:** no world crosses threads (the workspace builds with `Rc`, which a world sent across one would refuse), and `Rc`'s count is not atomic.
- **Laying out each object's record together in the store's read**, so a shared object reads as fast as a copy: it changes the store's read paths, which every read shares; recorded in `TODO.md` as the next step for the time this costs.
- **Smaller objects first:** names held once and small maps flat would shrink the store read and every copy, but reach about two hundred sites; recorded in `TODO.md`, to be measured on the spans and the write index first.

## Consequences

- `world.rs`: `Slots` holds `Rc`, `get_mut` copies on write, `shared`, `insert_shared` and `into_shared`. `publish.rs`: the mapping's steps write through `Rc::make_mut` where they change something, and `Simulated::install` holds shared objects. `store.rs`: the install takes both maps shared, and the world an erased object's redaction checks its invariants in holds every object shared, where it copied each for each erased object.
- Measured on the profile store, the same change and build otherwise: a version writing no object, the dry run 509 to 339 MiB and the approval 530 to 338 MiB; a backfill writing every claim, 510 to 387 MiB and 621 to 488 MiB, each against the build before this decision. Gate P at the profile passed with the same figures as 0.4.0 (writes p95 36.4 ms, one-object reads 9.0 ms, worklists 53.4 ms).
- `operate.md`'s sizing of a publish says what was measured, where it said ten times the store's file.
- Tests: the existing suites, which a mutant of each mapping guard fails (a rename of an attribute and a retyped attribute by the core's tests, a rename of a tracked member's spans by the service's `the_intervals_export_retracts_what_a_later_commit_deleted_or_moved`).

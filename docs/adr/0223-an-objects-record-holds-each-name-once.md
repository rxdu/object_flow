# ADR-0223: An object's record holds each name it repeats once

- **Status:** **Decided** 2026-10-20 under the author's direction ("go with your recommendation", on smaller objects next, measured on an object's spans and write index before its attributes), for `TODO.md`'s MEMORY.
- **Date:** 2026-10-20
- **Refines:** ADR-0165 (an object's record beyond its row); ADR-0222 (its alternative of smaller objects, taken up)
- **Relates to:** `storage-schema.md`, `of_interval` and `of_attribute_write`; `scripts/profile/publish-memory.py`, `massif-sites.py`

## Context

After ADR-0222 a publish held the store read whole and little else, and the read was 73% of an approval's heap at its peak on the two-year profile store, with the allocator's overhead for its blocks 40% of what was resident. An object's record repeats a few names many times: each of its 6.9 spans held the actor that entered it and that actor's kind as strings of its own, 601 distinct actors and one kind across 251,629 spans; each of its 10.9 write index entries named its attribute, and each of its 4.9 tracked members its dimension, as strings of their own. About a million allocations of a few distinct names.

## Decision

1. **A name an object's record repeats is a `Name`, an `Rc<str>`**: the keys of an object's spans and of its write index, and a span's actor and actor's kind. A `&str` finds one in a map, as a `String` did.
2. **A read of the store holds each name once** (`Names`, a set the read keeps), so every object it reads shares it: an object's parts, a metric's spans of a type, and an export's page of intervals. A name made one at a time elsewhere, a request's or an import's, is its own `Rc`.
3. **An object's attributes are not changed yet**: they are read and written at about a hundred sites, and the same step for them, their keys and the state's and enumeration's names a `Value` holds, is to be measured on its own.

## Alternatives rejected

- **A small enumeration for an actor's kind:** the kinds are a closed set, but a span's kind is read and written as text across the wire and the store, and a `Name` held once costs the same.
- **A process-wide interner:** it would hold every name a store ever read for the life of the service, and need a lock; a read's own set is dropped with the read, and the names its objects hold live as long as they do.
- **Flat small maps in place of `BTreeMap` for the parts:** the maps' nodes are a smaller share than the names were; to be measured after the attributes.

## Consequences

- `world.rs`: `Name` and `Names`; `Parts::spans` and `Parts::writes` keyed by `Name`, `Span::entered_by` and `Span::entered_by_kind` `Name`s. `store.rs`: the three span readers and the write index's read share one `Names` each. The other sites read by `&str` or make a `Name` from a `String`.
- Measured on the profile store, against ADR-0222's build: a version writing no object, the dry run 339 to 303 MiB and the approval 338 to 305 MiB, the dry run 4.0 s to 3.4 s, faster than before ADR-0222's sharing, with a million fewer allocations; a backfill writing every claim, 387 to 324 and 488 to 407 MiB. Gate P back to back with ADR-0222's build: peak resident 666 against 758 MiB, writes p95 34.3 against 37.0 ms, the metrics alike or faster.
- Tests: the existing suites, the core's 304 among them; the change is one of types, which the compiler held at every site.

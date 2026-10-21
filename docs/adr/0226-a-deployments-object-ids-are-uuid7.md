# ADR-0226: A deployment's object ids are UUIDv7s, a sandbox's are numbered

- **Status:** **Decided** 2026-10-21 under the author's direction ("go with your recommendations", on building what ADR-0018 and ADR-0077 decided, before 1.0.0).
- **Date:** 2026-10-21
- **Refines:** ADR-0077 decision 7 (object ids are UUIDv7, the store's id source injected), as built
- **Relates to:** ADR-0018 (an object's id store-assigned, globally unique and opaque), which this builds; ADR-0207 decision 3 (a restore's margin); ADR-0180 (what a release promises, from 1.0.0); ADR-0038 (a cascade in id order); `storage-schema.md` §2; `library-api.md` §3, `new_id`

## Context

ADR-0018 decided that every object carries an id the store assigns, globally unique, across deployments that might later be merged, so no per-type or per-table sequence, and opaque, encoding no meaning; ADR-0077 decision 7 made it a UUIDv7, the store taking its id source as an injected dependency so a test can make it deterministic. The build minted `<type>-<n>` in every store, a deployment's included (`store.rs`, `Allocator::object_id`, a number from `of_counter`'s `object`): unique within one store only, naming the object's type, and showing how many objects the store has made. Checking `storage-schema.md` against the build on 2026-10-21 found it.

An id is on the wire, in every answer, reference, event and link a caller keeps, and what a release promises binds from 1.0.0 (ADR-0180). Changing what a new id looks like is a change a 0.x release may make and 1.0.0 may not.

## Decision

1. **A store's id source is its own**, `Ids`: `Numbered` in a sandbox, in memory or in a sandbox's PostgreSQL schema, and so wherever examples and the core's tests run, `note-1` readable and the same on every run; `Uuid7` in a store a deployment opens, a file or a PostgreSQL database. A test sets a store's source as it needs, which is ADR-0077's injection.
2. **A deployment's id is a UUIDv7** (RFC 9562 §5.7) in its canonical lower-case text: the milliseconds since 1970 by the system clock in its first 48 bits, its version and variant, and 74 random bits, the first 80 bits of SHA-256 over a 16-byte seed and the object's number, the version and variant set over them. The seed is read from `/dev/urandom` when the store is opened, and an open that cannot read it is refused, since an id made without it could repeat another deployment's. The object's number is `of_counter`'s `object`, taken in the writing transaction, which no two objects of a store share, two services on one store during an upgrade's overlap included; so two of a store's ids are alike only where SHA-256 maps two inputs to the same 74 bits, and two stores' only where their seeds or that hash collide.
3. **An id is unique, not secret.** It authorises nothing: who may request what of an object is the upper layer's (PRD F1). Its random bits do not reveal the seed.
4. **An id never changes.** A store upgraded keeps the `<type>-<n>` ids it holds, which every reference, event and caller's link names, and mints UUIDv7s from then on; an id being opaque, a store holding both forms is no defect, and a client parses neither.
5. **An order by id stays creation order**, each object's own, which the store reads (`in_id_order`), never the id's text: neither a number's text nor a UUIDv7 made in one millisecond orders as its objects were made.

## Alternatives rejected

- **The build as the decision**, ADR-0018 narrowed to one store: two deployments' objects could never be merged without renaming every id, and every id would go on naming its type and counting the store's objects.
- **A UUIDv4**: ADR-0077 chose a time-ordered id for an index's locality; nothing since has argued otherwise.
- **The `uuid` crate**: a dependency, with `getrandom` behind its v7, for thirty lines the store writes and a test holds to RFC 9562's own example.
- **Reading the system's randomness for every id**: a read per object made, and a failure inside minting, which cannot fail today; one seed per opening and a hash over the object's number give every id its own bits.
- **Numbering in a deployment and UUIDs in a sandbox**, or one source everywhere: examples and tests read `note-1` in their expectations and transcripts, and the harness replays by them.

## Consequences

- `store.rs`: `Ids`, `Ids::deployment`, `uuid7`; `Store::ids`, numbered in `sandbox` and `reading`, a deployment's in `opened` behind `open` and `open_postgres`, a fork's its store's; `Allocator::object_id` minting by it.
- Tests: `a_uuid7_is_written_as_rfc_9562_writes_it`, RFC 9562's example; `a_deployment_mints_uuid7_ids_and_a_sandbox_numbers_them`, fifty ids of a deployment's source each a UUIDv7 and all unlike, a sandbox's `note-1`, two seeds unlike; and the service's tests, which read every id from an answer.
- `storage-schema.md` §2's note says what is built; `wire.md` says an id is opaque and what each store mints; `CHANGELOG.md` says a deployment's new ids change form, which a client that parsed `<type>-<n>` meets.
- A restore (ADR-0207) hands out no id the lost period handed out: the restored store's seed is its own, where a numbered id was carried past a margin; `a_restored_copy_hands_out_nothing_its_copy_never_saw` checks the id unlike each lost one, and the claim numbers, still a sequence's, past the margin once.
- Found running the tests with it: a store file copied without its log and restored beside a later state's log, an empty database beside frames, was refused advising a start with `--create`, which would take that log in, where it now says to restore the copy alone, as a restored store's other refusals do; `a_backup_restored_beside_a_later_log_is_refused_with_the_advice` had passed only while the later log carried the schema's pages, which the ids' form decided.
- Measured 2026-10-21, the published 0.5.0 and 0.5.1 each building the two-year profile store (`scripts/profile/build-store.py`, 12,000 claims) and publishing on it (`publish-memory.py`, three runs, the same in each, an idle i7-8700): the file 208 MB to 275 MB, inferred to be an id's 36 characters wherever one is written, not measured apart; a publish's peak memory 305 to 309 MiB adding an attribute and 407 to 412 MiB with a backfill of every claim; the approval adding an attribute 4.7 s to 4.2 s, and with the backfill 6.2 s to 7.2 s. `operate.md`'s sizing, twice the store's file for a publish, holds on both.

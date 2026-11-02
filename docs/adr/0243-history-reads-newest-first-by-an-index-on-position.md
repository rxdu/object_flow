# ADR-0243: `history` reads newest first, and an object's events are indexed by their position

- **Status:** **Decided** 2026-11-02 under the author's direction ("go with your recommendations"), on the recommendation given the same day for issue #57; built the same day.
- **Date:** 2026-11-02
- **Refines:** ADR-0192 decision 5 (`history` answers pages), ADR-0194 decision 6 (a page ends at its bytes)
- **Relates to:** ADR-0015 and ADR-0163 (legacy entries before the events); ADR-0177 decision 1 (a change to the store's tables is a release); ADR-0181 decision 6 (`wire-v1.json` promises each key); ADR-0234 decision 3 (PostgreSQL's writes at scale 2, a named miss); `docs/research/reading-history-newest-first.md`

## Context

`history` answers an object's events a page at a time, oldest first, its `cursor` the position the page ended at (`wire.md`, *Reads* and *Pages*). Issue #57, filed 2026-11-02 at 0.11.0, has a record page that shows a record's activity newest first. With an oldest-first read alone, the newest events are on the last page, so every open of a long-lived record reads its whole history and reverses it.

The issue assumed the store reads an object's events by an index on their position. It did not. Its one index on an object's events, `of_event_by_object`, ran by `(object_id, object_seq)` and repeated the index the unique constraint `of_event_per_object` makes on the same columns, on both backends. A page therefore read past itself. SQLite sorted every event of the object beyond the cursor, and PostgreSQL's planner scanned the log by position, dropping other objects' events. Measured 2026-11-02 (`docs/research/reading-history-newest-first.md` §3, with its conditions): a newest-first page of 100 from one object's 10,000 events took 123.1 ms on SQLite, against 0.6 ms by an index on `(object_id, position)`. On PostgreSQL, a page of one object's 1,000 events in a log of 500,000 read 49,900 rows, against 100 by that index. A group's page, one statement over several objects, kept the scan even with the index.

KurrentDB reads a stream backwards from its end, Stripe's lists answer newest first, and Jira's comments take `-created`; PostgreSQL and SQLite answer a page of either order from an index matching it, unsorted (the research, §1 and §2).

## Decision

1. **`history` takes `newest_first`**, true or false, false unless given.
   - Newest first, it answers the same events from the latest position down. Its `cursor` is still the position the page ended at, and the next page reads the events before it.
   - Its pages together are the oldest-first read reversed. The legacy entries, on the first page oldest first, come last, reversed, on the page whose `cursor` is `null`.
   - An event recorded after the first page is not met by a later one, since each reads below the last. A reader sends the read again without a cursor to see it.
   - A cursor is a position either way, as before, so one from an oldest-first read reads the events before that position newest first.
2. **An object's events are indexed by their position:** `of_event_by_object` is `(object_id, position)`, in place of the copy of the unique constraint's index.
   - A page reads each object's events by it, in either order, a page's limit and bytes of them at most, and merges them by position. So a page reads about the events it holds, a group's included.
   - It changes the store's tables, so it is a release (ADR-0177 decision 1): 0.12.0, the release in progress, its step from 0.11.0 dropping the old index and building the new one over the whole log.
   - Each write maintains as many indexes as before. Whether its time moves is measured by 0.12.0's Gate P, not assumed.

## Alternatives rejected

- **`newest_first` on the index by `object_seq`.** It answers the wire, but each page still costs the object's whole history on SQLite and a scan of the log on PostgreSQL, the issue's cost moved into the store.
- **The index on position added at a store's open, beside the schema**, as the export's and the actor's are. No release, but every write maintains one index more, on the path where PostgreSQL's writes at scale 2 are a named miss (ADR-0234 decision 3), and the copy of the unique constraint's index stays.
- **`order`, as `query` takes it.** `query`'s `order` names attributes, and `history` has one order to reverse; one key would mean two things.
- **Relay's `last` and `before`.** Its pages keep their order (the research, §1), so the caller still reverses each one.
- **A cursor that marks its direction.** `wire.md` promises `history`'s cursor is a position, and a reader may build one from an event it holds.
- **One statement over a group's objects.** PostgreSQL's planner kept its scan of the log for a list of two (the research, §3).

## Consequences

- Built in the core (`store.rs`, `history_page` and `history_read`, and the schema's index; `release.rs`, the 0.12.0 entry and `object_position_index`), the service (`main.rs`, `history` and its keys), the Flow Studio's sandbox and its key list, and `wire-v1.json`. `wire.md`, `integrate.md`, `operate.md`, `library-api.md`, `storage-schema.md`, `DESIGN.md` §10 and `CHANGELOG.md` say so.
- **Tested on both backends.**
  - `a_history_newest_first_is_its_oldest_first_reversed`: one object and a group, at every limit and from every position, and pages cut by their bytes, one object's large events beside another's small ones, either way.
  - `a_page_of_events_ends_at_its_bytes`, newest first too.
  - `a_page_of_an_objects_history_is_read_by_its_index_unsorted`, on SQLite: the plan, in both orders, is a search of the index with no sort.
  - `a_history_is_read_newest_first_its_legacy_entries_last`, on the wire: the legacy entries last, the cursor and the refusal of a value that is not true or false.
  - The upgrade tests: every fixture store read newest first after its upgrade, an upgraded store's indexes compared with a new one's by their columns, where they were compared by name, and a PostgreSQL store's index read by its columns before and after its upgrade.
  - The Flow Studio's `test_a_history_newest_first_is_read_alike`: the sandbox and the core answer alike at every limit.
- **Planted defects, each caught:** in the store, the step migrating nothing (on both backends), the schema's index left by `object_seq`, newest first read oldest first, a group merged oldest first either way, and an object's read stopping before the event that reached its bytes; in the service, the legacy entries on the first page, not reversed, before the events, and `newest_first` not read; in the sandbox, `newest_first` not read.
- **The upgrade to 0.12.0 builds the index over the log once.** A million events of about 1 KB took 1.27 s on SQLite 3.37.2 and 0.68 s on PostgreSQL 16.8 with `fsync` off, each log already in memory (`TODO.md`, 0.12.0's entry). Reading a log from disk adds its read.

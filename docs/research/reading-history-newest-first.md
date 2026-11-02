# Reading a record's history newest first

**The question.** How do established systems let a reader page through a record's history from its latest entry, and what lets the store answer such a page without reading the whole history? Issue #57 (filed 2026-11-02) asks for it: `history` answers an object's events oldest first only (`guides/wire.md`, *Reads*), so a page that shows a record's latest events must read every page first. This document records how event stores and APIs read backwards, how databases serve a page of a descending order, and what ObjectFlow's store did.

## 1. Event stores and APIs read backwards

**KurrentDB, formerly EventStoreDB** (S1). A read of a stream takes its direction: "When reading events, pick the name of the stream from which you want to read the events and choose whether to read the stream forwards or backwards." Reading from the end gives the latest first: "In addition to reading a stream forwards, streams can be read backwards. To read all the events backwards, set the stream position to the end:" `client.ReadStreamAsync(Direction.Backwards, "order-123", StreamPosition.End)`. Its page adds: "Read one event backwards to find the last position in the stream."

**Stripe's list API** (S2). Lists answer newest first, with a cursor each way: "Both parameters accept an existing object ID value (see below) and return objects in reverse chronological order." Of the second: "parameter returns objects listed before the named object."

**Jira Cloud's REST API** (S3). An issue's changelog answers oldest first only: "Returns a paginated list of all changelogs for an issue sorted by date, starting from the oldest." Its comments take an order, `orderBy`, one of `created`, `-created` and `+created`: "Order the results by a field. Accepts created to sort comments by their created date."

**The GraphQL Cursor Connections Specification** (S4) pages backwards without reversing a page. "To enable backward pagination, two arguments are required. last takes a non-negative integer. before takes the cursor type as described in the cursor field section." The edges keep their order: "The ordering of edges should be the same when using first / after as when using last / before, all other arguments being equal. It should not be reversed when using last / before."

## 2. A database answers a page of a descending order from an index

**PostgreSQL 16, Indexes and ORDER BY** (S5). "An important special case is ORDER BY in combination with LIMIT n : an explicit sort will have to process all the data to identify the first n rows, but if there is an index matching the ORDER BY , the first n rows can be retrieved directly, without scanning the remainder at all." Either direction serves: "The index can also be scanned backward, producing output satisfying ORDER BY x DESC".

**SQLite, Query Planning** (S6). "The Idx1 index is scanned from top to bottom (or from bottom to top if "ORDER BY fruit DESC" is used) in order to find the rowids for each item in order by fruit."

**Use The Index, Luke, Paging Through Results** (S7). The seek method pages by the last value given: "The seek method avoids both problems because it uses the values of the previous page as a delimiter." Descending, it reads below it: "To select the sales that must come behind a particular date you must use a less than condition ( < ) because of the descending sort order." The index then skips the pages before: "This has a huge benefit in terms of performance because the database can use the SALE_DATE < ? condition for index access."

## 3. ObjectFlow beside them

- **`history` read one way.** It took `id`, `actor`, `follow`, `limit` and `cursor`, its cursor the position the page ended at, and read `position > cursor ORDER BY position` (`objectflow-core/src/store.rs`, `history_page`, at `v0.11.0`). Jira's changelog reads the same way.
- **The store's only index on an object's events ran by `object_seq`.** `of_event_by_object (object_id, object_seq)` repeated the index the unique constraint `of_event_per_object` makes on the same columns, on both backends (`sqlite_master` and `pg_indexes` read 2026-11-02). No index matched `ORDER BY position` for one object, so a page sorted, as S5 says an explicit sort does.
- **Measured 2026-11-02, a page of 100.** *SQLite 3.37.2*, Python's library, not the core's bundled one: one object's 10,000 events of 16 KB in a log of 20,000. A newest-first page took 123.1 ms by `(object_id, object_seq)` and 0.6 ms by `(object_id, position)`; an oldest-first first page took 7.3 ms and 1.5 ms. The plan read `USE TEMP B-TREE FOR ORDER BY` by the first index and no sort by the second. *PostgreSQL 16.8*, the image CI pins: one object's 1,000 events in a log of 500,000. By the first index the planner scanned the log by position and dropped the other objects' events, 49,900 rows read for 100, 5.4 ms. By the second it read the 100, 0.09 ms forward and 0.12 ms backward. A list of two objects kept the scan of the log, 6.6 ms, so a group read in one statement does not use the index. SQLite's figures are each the best of five runs, PostgreSQL's one `EXPLAIN ANALYZE` each, on a workstation of 12 threads, the events generated for the measurement.
- *Inferred:* a newest-first read is KurrentDB's backwards read, and its cursor is S7's seek method, a position read below. Relay's backward pages would leave each page for the caller to reverse.

## 4. What it suggests

These are recommendations for the author, not decisions.

1. **`history` takes a direction**, newest first, as KurrentDB's read and Jira's comments take one. The latest events are on its first page, and its cursor is still the position the page ended at, read below.
2. **An object's events are indexed by their position**, so either direction reads a page by the index, as S5 to S7 describe. The index on `object_seq` repeated the unique constraint's, so it can be replaced rather than added beside, and each write maintains as many indexes as before.
3. **A group's page reads each member by the index and merges them**, since one statement over several objects does not use it.

## Sources

Each retrieved 2026-11-02 with `curl -sL` into its own directory, its markup stripped, and every quotation above found in the stripped text, where the stripping left a space beside markup, as in "LIMIT n :", the quotation keeps it. S3 is the published OpenAPI document; its quotations are its `description` fields as they render, a link's text kept and its target left out, and emphasis dropped.

| | Source | Version |
|---|---|---|
| S1 | Kurrent Docs, .NET client, Reading events: https://docs.kurrent.io/clients/dotnet/v1.0/reading-events | client v1.0; the page gives no date |
| S2 | Stripe API Reference, Pagination: https://docs.stripe.com/api/pagination | API version `2026-10-28.endive`, as the page names it |
| S3 | Atlassian, Jira Cloud platform REST API v3, OpenAPI document: https://developer.atlassian.com/cloud/jira/platform/swagger-v3.v3.json, `GET /rest/api/3/issue/{issueIdOrKey}/changelog` and `GET /rest/api/3/issue/{issueIdOrKey}/comment` | `1001.0.0-SNAPSHOT-25cde55b2229d7a79c3fdeb6b9d8abe3bc137957` |
| S4 | GraphQL Cursor Connections Specification: https://relay.dev/graphql/connections.htm | the page gives no version |
| S5 | PostgreSQL 16 Documentation, 11.4. Indexes and ORDER BY: https://www.postgresql.org/docs/16/indexes-ordering.html | PostgreSQL 16 |
| S6 | SQLite, Query Planning: https://www.sqlite.org/queryplanner.html | last updated 2026-03-22 10:09:38Z |
| S7 | Markus Winand, Use The Index, Luke, Paging Through Results: Fetching the Next Page: https://use-the-index-luke.com/sql/partial-results/fetch-next-page | the page gives no version; copyright 2010-2026 |

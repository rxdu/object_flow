# Reads that reach only what a request needs

**Status: decided by the author, 2026-10-07** (ADR-0162), as recommended in §5 and §6, the day after it was proposed for discussion. It sets out what the core did when this was proposed, what that cost as measured, the options, the decision, the target PRD revision 22 gives C5, and, in §7, what has been built since and what each step measured.

## 1. What the core did when this was proposed

*(As of 2026-10-06. None of it holds now: §7 records each step that replaced it.)*

Every operation began by loading the whole store into memory. `Store::snapshot` (`core/crates/objectflow-core/src/store.rs`) read the object directory and every type's table, every object's interval spans and attribute writes, the whole event log, the attempt log, the idempotency records, the attempt rollup and the external identifiers into one `World`, and the operation ran over that. A request took its snapshot inside its transaction, decided on it and wrote what it changed (`Store::transact`); a read took one and answered from it.

This was the first slice's choice, never a decision: it kept the evaluator a function over plain data while the semantics were being built and held to the reference runner. It is recorded as an open item in `TODO.md` ("a request reads the whole store into memory").

## 2. What it costs, as measured

`core/crates/objectflow-core/examples/probe_scale.rs` grows one store of one type, five events an object, and times each operation at each size, the median of nine runs, on one machine, a release build, SQLite in memory. Indicative only: no disk, no PostgreSQL, one type.

| objects | events | request | `get` | `query` | metric read | one load of the store |
|---|---|---|---|---|---|---|
| 100 | 451 | 3.0 ms | 1.9 ms | 2.0 ms | 5.5 ms | 2.0 ms |
| 500 | 2,271 | 15.6 ms | 9.1 ms | 9.3 ms | 29.8 ms | 9.0 ms |
| 1,000 | 4,531 | 34.9 ms | 19.3 ms | 19.8 ms | 60.7 ms | 18.8 ms |
| 2,000 | 9,041 | 66.1 ms | 38.8 ms | 41.7 ms | 125.7 ms | 39.0 ms |
| 4,000 | 18,051 | 149.3 ms | 83.0 ms | 86.7 ms | 276.1 ms | 82.0 ms |

Every operation costs at least one load of the store, about 20 µs an object at this shape. Where that load's time goes, measured 2026-10-07 by timing each stage of `Store::snapshot` over 2,000 objects and 9,041 events, a release build, the third of three successive loads: the event log 17.7 ms, 53% of the load; the objects and their interval spans 13.0 ms, 39%; the attribute write index, creation dates, erased objects and the rest about 2.6 ms together; and the attempt log, the idempotency records and the rollup, under 0.1 ms, small at this shape since the probe's requests are refused rarely and carry no key. A request costs about two loads and a metric read about three. The cost grows with everything the store holds, closed objects and history included, and not with what the operation reads: a `get` of one object costs what a query over all of them does.

At the first consumer's scale (PRD N3: 279 live deliveries, 81 services, 203 warranty contracts and 123 live customers on 2026-09-01, and 6,391 audit rows over its system's life), the line puts a request in the tens of milliseconds and a metric read near or above a hundred. The store never shrinks, since history is kept, so both grow with every year of operation, and N3's own range, "hundreds to thousands of live objects per type, and their history", reaches several hundred milliseconds.

## 3. What a request actually reaches

What a request reads is set by the declaration, and most of it is a small neighbourhood of the object requested:

- the object, and the objects its guards, effects and invariants reach through references and parts;
- its own intervals and attribute writes, for `entered_at`, `time_in` and `changed_since`;
- its idempotency record, by key, and its external identifiers, by source;
- the actor, and the principal if one is named.

Some reaches are whole types or long histories by their nature:

- a type scan in an expression (`count(Ticket where …)`), and a uniqueness checked across a type or a scope;
- `referrers`, every object holding a reference to this one;
- a metric a guard reads, over its type's history, and the attempt log a metric over `attempts` reads.

`storage-schema.md` §5 and §6 lay out the indexes for exactly these reads (`of_interval_current`, `of_interval_entered`, `of_attempt_by_rule`, each type's unique and indexed columns), and the core does not use them yet, since it reads everything.

## 4. The options

**A. Keep the whole store in memory between operations, and keep it current.** The service holds one `World` and applies each committed change to it; an operation first checks the log's last position and catches up on anything another writer committed. Each operation then costs what it reaches, plus the catching up. The evaluator and every read stay as they are, and the change is confined to the store. Against it: memory holds the whole store for good, history included; the incremental update is a second path that must give exactly what a fresh load gives, which a test can hold it to but which doubles where a fault can live; and on PostgreSQL with several copies of the service, ADR-0039's serialisable isolation guards only the reads that reach the database, so a decision taken on an in-memory copy can write-skew against another copy's unless each request re-reads or locks what it read. On SQLite, whose single writer serialises every request, it is sound.

**B. Read through the database, by what the rules reach.** The `World` becomes a view that fetches on demand: an object by id from its type's table, kept for the rest of the operation; a type scan as a query on the type's table, its filter in SQL where it compiles and its rows loaded where it does not; intervals, writes, attempts and events by the indexes laid out for them; idempotency and external identifiers by key. Each operation costs what it reaches through an index, memory holds only that, and every read goes through the database, so ADR-0039 holds on both backends. Against it: it is the largest change, since every place that walks the world's maps becomes a fetch, and a metric over a long window still reads many rows, by index ranges rather than everything.

**C. Plan each transition's reach from the declaration, and load that.** Before deciding, compute from the transition's guards, effects and invariants which types and relationships it can reach, and load those whole. It keeps the evaluator over plain data. Against it: the analysis follows paths, referrers, scans and metrics, and is conservative where it cannot tell, which for any rule with a scan or a metric is the whole type; it reduces the load to the types reached, not to the objects.

**D. Keep the whole-store load, and set C5's target where it holds.** Nothing changes, and the first consumer's cutover is inside interactive time by the measurements above. Against it: the cost grows with history without bound, and every application built on the engine inherits it.

## 5. Recommendation

**B, reached in steps that each stand alone and each is measured with the probe**, and C5's target set now so each step is judged against it:

1. The log, the attempt log, the idempotency records and the rollup are read by query where an operation needs them, never preloaded. Most requests read none of them beyond one key, and the event log is the store's largest table.
2. Objects are fetched by id from their type's table, held for the operation, with a type scan loading its type's rows.
3. Type scans and uniqueness checks use each type's indexed columns, intervals and writes use `of_interval_current` and `of_interval_entered`, and a metric reads its rows by index range.

A is rejected as the target because it breaks ADR-0039's guarantee on PostgreSQL with more than one copy of the service, which `DESIGN.md` §2 allows, and because its memory grows with history for good. It could serve as a stopgap on SQLite, but the steps of B remove the largest costs first, so no stopgap is needed. C is rejected because its result for any rule with a scan or a metric is the whole type, which is most of the cost B removes. D is rejected for the engine; it remains true that the first release meets interactive time at the first consumer's scale, which is why the steps can be taken in order rather than all at once.

The reference runner stays a whole-store evaluator: it is the oracle for what an operation decides, not for how it reads, and the replays and the whole-answer comparison (`apps/flow-studio/server/tests/test_backends_agree.py`) hold every step to the same answers.

## 6. C5's target

PRD C5 asks that reading a derived datapoint or a metric be fast enough for interactive use at the first consumer's scale, its target "to be set by measurement". The measurement now exists, and **the target is the author's to set**. A proposal, for discussion: a request and a `get` within 100 ms, and a metric read within 1 s, at the 95th percentile, on the deployment's hardware, over a store of N3's upper range with several years of its history. The two bounds are the response-time limits usability practice gives for an action that feels immediate and for one that keeps a person's flow of thought (Nielsen, "Response Times: The 3 Important Limits", 1993); the store size is the part only the author can fix, since it is a statement about the first consumer's growth.

## 7. Decided, and built so far

The author decided B and C5's target as recommended on 2026-10-07 (ADR-0162, PRD revision 22), and the steps proceed in order, each measured with the probe.

**Step 1 is built** (2026-10-07). The log is read on an operation's first use of it and kept for the operation, which appends its own events beside it; the attempt log and the rollup likewise; and a request carrying an idempotency key reads that key's record alone. A store whose log cannot be read stops the operation that reads it as not run, naming why, never as an empty log. An erasure reads the log once and walks it. Measured with the same probe, before and after, the log's change alone:

| objects | request | `get` | query | metric read | one load |
|---|---|---|---|---|---|
| 500 | 15.6 → 6.2 ms | 9.1 → 4.1 ms | 9.3 → 4.5 ms | 29.8 → 24.8 ms | 9.0 → 4.1 ms |
| 1,000 | 34.9 → 12.6 ms | 19.3 → 8.0 ms | 19.8 → 9.8 ms | 60.7 → 51.6 ms | 18.8 → 8.1 ms |
| 2,000 | 66.1 → 27.9 ms | 38.8 → 18.5 ms | 41.7 → 19.5 ms | 125.7 → 105.9 ms | 39.0 → 17.1 ms |
| 4,000 | 149.3 → 59.9 ms | 83.0 → 37.9 ms | 86.7 → 41.6 ms | 276.1 → 224.6 ms | 82.0 → 38.5 ms |

A request and a read now cost about 40% of what they did, the objects and their spans being what remains of a load; a metric read over transitions still reads the whole log, which step 3's index ranges remove. Step 2, objects by id, is next. Its first part reads each type's rows, and each set's items, in one query rather than one an object, which measured neutral on SQLite in memory, a request at 2,000 objects 28.7 ms against 27.9 and one load 17.5 against 17.1, and saves a query an object on a backend reached over a connection. Runs of the probe differ by up to a quarter from one another, so a change smaller than that is not shown by one run.

**Step 2 is built** (2026-10-07). A load reads no object. An object is read on its first use, alone by id, with every part of its record, and held for the operation, which changes and adds to what it holds; a scan reads its type, and a read of every object the whole store, each table in one read, as the load did. What the operation holds is kept over what a later scan reads, so a change it made is never read back from the store. An object that cannot be read stops the operation as not run, naming it, never as an object that is not there, and a type the directory names that the library lacks is refused, never passed over. Each read is a store query inside the operation's transaction, so ADR-0039's guarantee holds as it did. Measured with the same probe, after step 1 and after step 2:

| objects | request | `get` | query | metric read | one load |
|---|---|---|---|---|---|
| 500 | 6.2 → 4.5 ms | 4.1 → 0.2 ms | 4.5 → 4.7 ms | 24.8 → 25.0 ms | 4.1 → 0.1 ms |
| 1,000 | 12.6 → 8.9 ms | 8.0 → 0.3 ms | 9.8 → 9.5 ms | 51.6 → 51.8 ms | 8.1 → 0.2 ms |
| 2,000 | 27.9 → 18.6 ms | 18.5 → 0.5 ms | 19.5 → 20.1 ms | 105.9 → 107.3 ms | 17.1 → 0.5 ms |
| 4,000 | 59.9 → 37.7 ms | 37.9 → 1.0 ms | 41.6 → 43.6 ms | 224.6 → 223.4 ms | 38.5 → 1.0 ms |

A `get` no longer grows with the objects, what remains of it being the load's reads of the imported events' positions, the external identifiers and the legacy entries. A query scans its type, as it must until step 3 reads it by index. A request then still grew, because step 7 checked the invariants of every object (`request.rs`, as the reference runner does), where `DESIGN.md` §5.6 and ADR-0045 check only those the written objects could violate. Scoping it found that the two were not equivalent: a publish left standing an admission its backfill cured, which the next request on any object discharged (D531), and a kind's new invariant its old observations violate refused every later request (D532). With the publish discharging what it cures, step 7 now checks the objects a request changed or made, and every object of a type whose invariants reach them, a type whose reach is not told read whole and a kind's invariants on the recording alone; every answer is the whole-store check's wherever every violation in the store is admitted, which every route now keeps but one: the observations a kind's new invariant finds violating stand unadmitted, by ADR-0146, and are never checked again, where the whole-store check refused every request over them. Measured with the same probe, after step 2 and after the scoped check:

| objects | request |
|---|---|
| 500 | 4.5 → 0.4 ms |
| 1,000 | 8.9 → 0.6 ms |
| 2,000 | 18.6 → 0.8 ms |
| 4,000 | 37.7 → 1.2 ms |

The load's last whole reads went the same day: the imported events' positions are read on first use, as the log is, by the reads of flow data that ask for them; an external identifier by its key, or whole by an import, which resolves every key a batch names; and an object's legacy entries by its id. By the probe a request now takes 0.3 ms and a `get` 0.1 ms at every size from 500 to 4,000 objects, and a load rounds to 0.0 ms: neither grows with the store. A query, 40.7 ms at 4,000 objects, and a metric read, 226.7 ms, scan their type and its history, which is step 3's. Where an operation reads every object, it reads them in the load's whole-table reads, since reading each type's parts apart cost the examples' run of many small stores about a fifth more.

**Before step 3** (2026-10-07). Timed in phases at 4,000 objects by a scratch probe built from `probe_scale.rs` and not kept, a release build as before: reading the type's objects took 38.1 ms, reading the log 53.5 ms, the type's interval rows 57.5 ms with that read included, and the metric read `Ticket.time_in_state` 227.9 ms. So about 170 ms of the metric read is spent after its rows are read, on what it computes from them, whether or not that includes reading the log, which this did not separate. Step 3's index ranges shorten the reads of a windowed metric and of a query's filter; they do not shorten that computation, which is where C5's metric target is most at stake at N3's upper range. Whether step 3 begins with the reads it names or with the evaluator's cost is the author's to weigh.

**Step 3, begun with the evaluator** (2026-10-07, ADR-0164 decision 3). Profiled with `valgrind --tool=callgrind`, collecting inside three reads of `Ticket.time_in_state` over 1,000 objects, the metric read spent most of its instructions copying rows: a dimension read through a row copied the whole environment and the row at each step, and the reader copied each row again into each environment and group, the copies, their drops and their allocations together dominating, with reading the store under a fifth. A row's fields are now shared, so a row is copied by its reference, and a dimension reads a row's member by the evaluator's own rule without binding it in a copied environment. The instructions of those three reads fell from 1.31 billion to 420 million, and by the probe:

| objects | metric read |
|---|---|
| 500 | 25.2 → 10.0 ms |
| 1,000 | 51.2 → 26.0 ms |
| 2,000 | 107.4 → 56.6 ms |
| 4,000 | 226.7 → 122.0 ms |

What the profile now shows is building the interval rows, a dozen named fields a span, about a third, and the store's read of the type in the first of the reads; the index ranges come next, measured the same way.

**A query's filter by its states** (2026-10-07). A query whose filter requires states of its objects, by a conjunct `state == S` or `state == T.S` and through any `and`, reads only the objects in those states, through each type's state index, and decides the filter among them as it would among all, since no other object can satisfy it; a bare state name counts only where every type of the family reads it as its state. What the operation holds is kept over what the read returns, as a scan's is. By the probe, whose tickets end half working and half done, a query `state == WORKING` at 4,000 objects fell from 44.4 ms to 24.7, and at 500 from 4.6 to 2.7; a store whose objects are mostly closed would read a smaller share, which is inferred and not measured.

**A windowed metric over history has no index to read by.** `storage-schema.md` §5's interval indexes, `of_interval_current` and `of_interval_entered`, are partial, `WHERE left_position IS NULL`, so they serve the current intervals alone: a metric over a window of closed intervals, such as time in state over the last thirty days, reads its family's intervals whole, as it does now. Reading such a metric by range would need an index over every interval by dimension and entry time, a change to the schema the author decides; until then the evaluator's cost, halved above, is what moves it.

**Uniqueness by its key** (2026-10-07). A type with a unique attribute made a request quadratic: step 7 checked every object of the type, and each object's uniqueness scanned the type again. Measured with the probe's ticket given a unique `ref`, a request took 29.1 ms at 500 objects and 108.9 at 1,000. A uniqueness, and an external identifier, now reads the objects holding its key by their columns, compared as each column encodes its declared type, so equal as the language's equality finds them; and step 7 reaches, for such a type, the objects sharing a key with what the request wrote, before the write or after it, rather than the type. A request on a ticket holding a key then took 0.4 ms at 500 objects, 0.5 at 1,000, 0.6 at 2,000 and 0.9 at 4,000. What still grew was SQLite reading the key's column whole, about 0.1 µs a row, since the core built no index on a unique column; the author decided on 2026-10-07 that every uniqueness key carries a plain index, not a constraint (ADR-0165), and the same request then took 0.4 ms at every size from 500 to 4,000 objects.

**A rule's scan by its states** (2026-10-07). A type scan in a guard, an invariant or a derivation, `count(w in T where w.state == S and …)`, now reads as a query does: where its filter requires states of what it binds, only the objects in them are read, through the type's state index, any other failing a conjunct outright and so neither selected nor undecided; a name counts as a state only where the evaluator would read it as one. Measured with the probe's `start` given a guard `count(w in Ticket where w.state == WORKING) < 100000`, about half the tickets working, the request took at 500, 1,000, 2,000 and 4,000 objects 5.5, 11.3, 24.6 and 52.3 ms without the reads by state and 3.4, 6.8, 15.5 and 33.1 with them. It still grows with the objects in the state, each read whole to be judged by its row. A filter's conjuncts on stored attributes, such as `w.space == space` on a reference end, now narrow the read too: the other side, reading neither what the scan binds nor any call, aggregate or metric, is evaluated as the filter evaluates it, without being read, and the store reads the objects whose column holds that value or none, since an absent value compares as unknown and leaves the count undecided. A number is compared by the store only in a column holding it in its own units, an integer in an integer column and a decimal in a decimal column of its scale, the language comparing the two numerically; any other is read by the type. Tests hold each case against the same filter read whole, mutations of the absent rows, the units and the columns each failing them; the probe, whose tickets all hold one size, does not measure this.

**An object's record on first use** (2026-10-07, ADR-0165 decision 2). A read of the store now fetches an object's row alone, and the rest of its record, its intervals, write index, standing admissions and erasure, where an operation first reaches one of them, so the reads before it, step 2's "with every part of its record" among them, no longer describe the core. A read that reaches many records reads them a type at a time: a metric's family, a query's page, `exceptions`, `diagnostics`, an export and a publish's simulation say so before they loop, and an operation reaching more than eight of a type's records one at a time, as a scan's filter calling a flow function does, reads the rest it holds of the type together. An object reaching a change or a write without its record is refused, never written as one with no history. By the probe, the last commit and this one built side by side on the same machine, one run of each, a query at 500, 1,000, 2,000, 4,000 and 8,000 objects took 2.8, 5.4, 11.6, 24.3 and 51.1 ms before and 0.9, 1.7, 3.4, 6.7 and 14.1 after; a request and a `get` were unchanged. A metric read, which needs every record of its family, took 10.0, 26.3, 49.3, 122.3 and 251.2 ms before and 10.9, 27.7, 52.0, 125.6 and 257.0 after, from 2 to 9% slower at every size: rows and records are now two reads of the type where they were one. Reading the records of the objects held alone, without the type's ids, was tried and withdrawn, since the read of a type's intervals refuses one whose object the type does not hold, which a damaged store is, and a subset would make every other object's intervals look so. A test holds that a read by id and a scan leave records unread, that the first eight are read alone and the ninth reads the rest, each as it would be read alone, and that an object without its record is refused by a change, an insertion and a world without a store; each of eight mutations of those rules fails it. The scenarios' replay found what the tests had not: a publish approved by an actor of a declared type, held without its record since the approval read it by id, answered 500, the install refusing to index its identifiers from a record it had not read; a publish now reads every record before it simulates and before it installs, and a test approves as such an actor.

**Exceptions by their indexes** (2026-10-07). `exceptions(type)` read its type whole, and every record of it, to list the objects whose state an assertion last set and those holding a standing admission. Every type's table now carries `storage-schema.md` §3.2's asserted-state index, `t_<type>_asserted_ix` on `state_source`, and the read takes the asserted objects by it and the admitted ones from the standing admissions, reading each by id; what the operation holds is kept over the store's answer, an admission it added or discharged counting as it holds it. Timed by a scratch copy of the probe, the last commit and this change side by side, `exceptions(Ticket)` at 500, 1,000, 2,000, 4,000 and 8,000 objects took 4.5, 9.0, 19.1, 40.5 and 85.4 ms before and under 0.1 after; the probe's tickets hold no exception, so this is the read's floor, and it now grows with the exceptions, not the type. A store built before gains the index when a publish next rebuilds the table, as the key index. A test holds the asserted, the admitted as stored and as held, a discharged admission, the query plan's use of the index and that the read holds only what it lists; each of eight mutations fails it.

**A metric's observations matched by sets** (2026-10-07). A metric over labels, or over one of a type's observation kinds, kept each observation whose subject was among the family's objects and that no observation corrects, finding both in lists, so its time grew with the observations times the objects. Both are sets now. Timed by a scratch copy of the probe, each ticket carrying one label of four names and the metric counting the tickets labelled `late` by the state each was in, the read took 4.6, 10.4, 30.9 and 109.2 ms at 500, 1,000, 2,000 and 4,000 tickets before and 3.3, 6.6, 14.6 and 34.3 after, growing with the store as its reads do. The labels test gained a label on an object of another type, without which a mutation keeping every subject went unnoticed.

**A required column narrowed by its index** (2026-10-07). A read narrowed by a column compared it as `(c = ? OR c IS NULL)`, since an absent value compares as unknown; on a column the table declares `NOT NULL`, SQLite 3.53 does not drop the null half and scans the table, while it serves `c = ?` alone, and the nullable form by a two-way search of the index (observed with `EXPLAIN QUERY PLAN` on the bundled library). A narrowed read now asks the table which of its columns hold no null, `PRAGMA table_info`, so a table built relaxed keeps its nulls, and compares those alone. A test holds both forms and their plans, and that a column marked holding no null when it may hold one fails a query's test; the time was not measured, the scan it replaces having been about 0.1 µs a row by the uniqueness measurement above.

**Labels by their name** (2026-10-07). `storage-schema.md` §6 indexes the label table by name and subject state, `t_label_by_name`, which the core did not build, and a metric over labels read every label. The table now carries the index, and such a metric narrows its read as a scan does: where the metric's filter, or a reader's, requires values of a label's stored attributes, `l.name == "late"` among them, only the labels holding them are read, by the index, while the corrections are read apart by the index on their stored end, since a correction may carry another name than the label it corrects. Timed by the same scratch probe, one label of four names a ticket, the metric took 3.3, 6.6, 14.6 and 34.3 ms at 500, 1,000, 2,000 and 4,000 tickets before and 2.0, 4.0, 8.4 and 19.4 after. It still reads the ticket family whole, to keep the labels on it, as a metric over objects reads its family whatever its filter; `TODO.md` records that. Tests hold the counts under a name filter, a correction carrying another name, one the operation holds and has not stored, that the read holds only the labels named, by the metric's filter and by a reader's, and the index in SQLite's plan; and a new test holds a declared kind's corrections, whose mutation went unnoticed before it. Each of seven mutations fails one.

**A metric over objects, its filter narrow** (2026-10-09). Measured, and not built. A metric whose `source` is `objects` reads every object of its type before its filter judges them (`metric.rs`, `rows`), where a query with the same filter narrows by the state's index. `probe_scale.rs` now times both, a filter `state == Ticket.NEW` that only the probe's own tickets meet, a release build as before, the median of nine runs:

| objects | narrow query | narrow metric over objects |
|---|---|---|
| 100 | 0.05 ms | 0.67 ms |
| 1,000 | 0.03 ms | 2.70 ms |
| 4,000 | 0.03 ms | 11.62 ms |
| 8,000 | 0.04 ms | 24.72 ms |

The metric grows by about 3 µs an object of its type whatever its filter keeps, and narrowing it as a query is would make it about as flat. At N3's range, hundreds to thousands of live objects a type, that is tens of milliseconds against C5's second for a metric read, so the narrowing is left until a deployment's types grow far past N3, the measurement standing as what it would save.

**The cursors and the admissions on first use** (2026-10-29). A load read every subscription's acknowledged cursor and every event that recorded an admission, for every operation. One subscription's read and a transition row's `overrides` use them (`subscription.rs`, `eval.rs`). Each is now read whole on an operation's first use of it and kept for the operation, inside its transaction as every read is. A cursor the store cannot read stops the operation reading it, named, where it stopped every operation. On PostgreSQL this cut four of a draft's 29 round trips; `research/write-throughput.md` §6 holds the count and its time. A test holds that a request, a `get` and a pull from a cursor given read neither table; a pull from its acknowledged cursor and a transition's rows each read one once and find what the store holds. Restoring the load's reads fails it.


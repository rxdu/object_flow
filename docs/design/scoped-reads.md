# Reads that reach only what a request needs

**Status: decided by the author, 2026-10-07** (ADR-0162), as recommended in §5 and §6, the day after it was proposed for discussion. It sets out what the core does today, what that costs as measured, the options, and the decision, and the target PRD revision 22 gives C5.

## 1. What the core does today

Every operation begins by loading the whole store into memory. `Store::snapshot` (`core/crates/objectflow-core/src/store.rs`) reads the object directory and every type's table, every object's interval spans and attribute writes, the whole event log, the attempt log, the idempotency records, the attempt rollup and the external identifiers into one `World`, and the operation runs over that. A request takes its snapshot inside its transaction, decides on it and writes what it changed (`Store::transact`); a read takes one and answers from it.

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


# ADR-0244: A read names the type an object was of under each version it answers, and an export judges a value as it was written

- **Status:** **Decided** 2026-11-03 under the author's direction ("go with your recommendations"), on the recommendation given the same day for issue #61; decision 3 was recommended there as a probe and a fix if the export filtered by the current type, which it did. Built the same day.
- **Date:** 2026-11-03
- **Refined by:** ADR-0250 — beside each event's `declaration_type` and `declaration_version`, `renamed` maps each name of its payload a later version renamed to the installed version's.
- **Refines:** ADR-0229 decision 3 (a move's event names the type it left), ADR-0094 §3 (an export omits personal values)
- **Relates to:** ADR-0146 decision 2 (a value personal under the version that wrote it stays so); ADR-0206 decision 5 (the gateway filters what reads answer); ADR-0209 decision 2 (an erasure judges what an event wrote as its version declared it); ADR-0196 decision 6 (a version may stop marking an attribute personal); `docs/research/naming-the-schema-a-record-was-written-under.md`

## Context

A gateway filters the personal values that `get`, `history`, `events`, `pull` and `export` answer (ADR-0206 decision 5). It learns which values are personal from `declaration`, with the pair a read answers: the object's type and a `declaration_version`, the version its values were written under.

Issue #61, filed 2026-11-03 at 0.12.0, found that a version moving a type (`moved_types`, ADR-0229) breaks the pair. Reproduced the same day on the published 0.12.0 (`TODO.md`, issue #61): after `Genre` moved to `Category`, `get` answered `Category` at version 1, and `declaration {Category, 1}` answered `404`. The case is worse when the move merges into a type the version before declared too (ADR-0229 decision 1). `declaration {Category, 1}` then answered `200` with Category's declaration, which lacks Genre's personal `alias`. The object's version-1 event held `alias`, so a gateway reading the pair was answered and filtered nothing there. Only the core resolved the type an object was of under a version, from the object's move events (`request.rs`, `type_then`).

Probing the issue's case found a second defect, in the engine itself. `export`, which leaves personal values out (`integrate.md`), judged each value by the installed version and the object's current type. After the merge, or after a version dropping `alias` from `Genre` with no move at all, the export gave the event's `alias`. A personal enumeration dropped alike was given too. An erasure judges a value personal under the installed version or under the version that wrote it, for the type the object was of then (ADR-0146 decision 2, ADR-0209 decision 2), and the export did not.

Platforms keep, with each record, a key naming exactly the schema that wrote it: Confluent's schema ID, Avro's writer's schema, Iceberg's schema and field IDs, and an event store's event type name as written. Each resolves a later rename when it reads, never rewriting the stored records (the research, §1 to §3). Confluent warns that a name with a version is not the same key as a schema's identity (S2).

## Decision

1. **Every read that answers a `declaration_version` of an object's writes answers `declaration_type` beside it:** the type the object was of under that version.
   - The reads are `get`, each event of `history`, `events` and `pull`, and each row of `export` holding one: the log's and `.transitions`' events, an observation kind's and `.labels`' records, and `.intervals`' spans.
   - The pair names the declaration the values were written under: `declaration {type: declaration_type, version: declaration_version}`.
   - An attempt's `type` is already the type it was made on, kept through a later move, and an attempt count holds no value an object wrote, so neither gains the field.
   - `declaration` itself is unchanged.
2. **The type is computed from the object's own move events,** as `type_then` reads them (ADR-0229 decision 3).
   - A read reads an object's move events only where some version moved a type into the object's current one, by the index on the object's events, once per object per read. Any other object is of its own type under every version.
   - Nothing is stored, so no table changes and no upgrade step carries it.
3. **An export judges each value as an erasure does:** omitted where the installed version marks it personal, or where the version that wrote it marked it personal for the type the object was of then.
   - An event's written values are judged under the version its writes were declared by, a migration's being the version it installed. Its inputs are judged under the transition as its own version declared it.
   - A value the installed version marks personal under a name a later rename gave it is omitted under its old name.
   - A span is judged under its own version.
   - A record's attribute is judged under the version that last wrote its value. The record's events are read only for an attribute some earlier version marked personal.
4. **A store holds its earlier versions' libraries from its open,** and keeps them through each publish.
   - A read judges a value by them and finds an object's moves through them. Before this, only a request loaded them, so a read before a store's first request found none.
   - The open already parsed each earlier version to find the types it retired, and now keeps them, as a first request did.
   - A publish loads the version it replaces before it writes, so a source that no longer loads refuses the publish and installs nothing. The publish adds that version once it is installed.
   - Where a store finds another process published, it loads the versions it lacks with the library.

## Alternatives rejected

- **`declaration` taking an object** (the issue's first example). A declaration read becomes an object read, and a gateway makes one call per object, where a pair is read once for every object it names.
- **`declaration` resolving the old name,** answering `{Category, 1}` with Genre's declaration. A split makes the old name ambiguous. A merge into a kept type makes `{Category, 1}` Category's own declaration, which it must still answer.
- **The type stored on each event,** as an event store stores its event type. That needs a column, an upgrade deriving it from the moves, and every writer changed, while the moves determine it already. Every event also stays as it was written (ADR-0229 decision 8).
- **The gateway following `moved.from` itself** (the issue's first alternative). It restates `type_then` outside the engine, reading an object's whole history before any value.
- **An export judged by the version that wrote a value alone.** A version may newly mark an attribute personal (`DESIGN.md` §8), and the export would then give values the installed version marks personal.
- **The earlier versions loaded by each read, or by the writer as it lets its lock go.** A read would parse every earlier source. The writer letting its lock go cannot answer a source that fails to load, so the failure would be lost.

## Consequences

- Built in the core (`world.rs`, `moved_into`, `moves_of` and `TypesThen`; `request.rs`, `object_type_then` reading the moves alone; `export.rs`, the rows' type and `WrittenUnder`; `store.rs`, the earlier versions at the open, the publish and a reload), the service (`main.rs`, `get`, `history`, `events` and `pull`), the Flow Studio's sandbox (`get`, `history` and `events`, as the reference runner's `type_then` reads them) and the web client's wire types.
- `wire.md`, `integrate.md`, `quickstart.md`, `library-api.md`, `DESIGN.md` §10 and `CHANGELOG.md` say so.
- **Tested on both backends:**
  - `a_read_names_the_type_an_object_was_of_under_each_version`: a chain of moves, `Genre` to `Category` to `Topic`, and a version moving nothing after them.
  - `an_export_leaves_out_what_was_personal_when_it_was_written`: a dropped attribute's written value and input, a personal enumeration's span and an observation's remark no longer marked personal, each omitted, beside the same kinds of value written under the later version, each given.
  - `an_export_after_a_merge_judges_each_event_as_the_type_it_was_of`.
  - `a_moved_objects_reads_name_the_declaration_it_was_written_under` on SQLite and on PostgreSQL. A merge, then a version moving nothing, each read's pair and the declaration it names, the export's rows, before a restart and after it.
  - The Flow Studio's `test_a_moved_objects_reads_name_the_type_it_was_of_alike`: the sandbox and the core answer alike.
- **Planted defects, each caught on both backends:**
  - The open keeping no earlier version: by the deployment test, after its restart.
  - A publish keeping no replaced version: by the chain, the merge's export, the deployment test and the Studio's.
  - `TypesThen` answering the type now, and the moves read as none: each by the same four.
  - An event's values judged by the installed version alone: by both export tests and the deployment test.
  - A span, or a record's attribute, judged by the installed version alone: each by `an_export_leaves_out_what_was_personal_when_it_was_written`.
  - `history` answering no type: by the deployment test and the Studio's.
  - The sandbox answering the type now: by the Studio's.
- **Not tested:** the reload where a read finds a version published after the writer let its lock go. It is a race no test can arrange, and it reads as the open does.
- **A read of an object of a type something moved into reads that object's move events,** by the index on its events. Every other object's read is unchanged: `a_get_reads_no_history` still holds.

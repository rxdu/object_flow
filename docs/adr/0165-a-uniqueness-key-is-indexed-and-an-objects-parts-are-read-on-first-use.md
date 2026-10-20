# ADR-0165: A uniqueness key is indexed, and an object's other parts are read on first use

- **Status:** Decided by the author, 2026-10-07 ("go with your recommendations on 1 and 2"), on recommendations stated with the questions, after building ADR-0162's step 3.
- **Date:** 2026-10-07
- **Refines:** ADR-0162 (reads by what rules reach), ADR-0164 decision 3 (step 3's order)
- **Refined by:** ADR-0222 — an object's record, read once, is held shared by every world that holds the object until one writes it. ADR-0223 — the names an object's record repeats held once for every object a read gives
- **Relates to:** `design/storage-schema.md` §3, §5, §8; `design/scoped-reads.md` §7; ADR-0041 (compilation is an optimisation), ADR-0074 (no constraint where an admission may stand)

## Context

Building ADR-0162's step 3 left two costs measured and unremoved.

- **A uniqueness reads its key by a column no index serves.** Since step 3 a uniqueness reads the objects holding its key by their columns, and a request on a type with a unique attribute takes 0.9 ms at 4,000 objects, SQLite reading the column whole at about 0.1 µs a row. `storage-schema.md` §8 compiles a `UNIQUE` constraint, which carries an index, only where no admission may stand (ADR-0074), and the core compiles none.
- **A scan reads every part of each object it judges.** Profiled at 1,000 objects, reading a type was about 3% of a metric read's instructions for the rows and about 15% for the other parts of each record, the flow data, the write index, the standing admissions and the erasure, while a scan's or a query's filter usually reads the rows alone.

## Decision

1. **Every uniqueness key's columns carry a plain index**, not a constraint: `t_<type>_<attribute>_key_ix` over the columns the key compares, a reference's key excepted, whose column is indexed already as a stored end. It is built with the type's table and rebuilt with it, so a publish that adds or drops a uniqueness changes the table and rebuilds it with the index or without. Being no constraint, it refuses nothing an admission lets stand.
2. **An object's other parts are read on first use.** A read of the store fetches an object's row, and its flow data, write index, standing admissions and own erasure where an operation first reaches one of them, for the one object or for every object a read holds of its type at once; what the operation holds of them is kept as it holds it, as an object is.

## Alternatives rejected

- **Compiling the uniqueness as a `UNIQUE` constraint.** An admitted conflict, an import's `ADMIT` and a mirror's invariants all stand where the database would refuse them (ADR-0074, ADR-0147); a constraint serves only where none can, and the core would need to know where before it could index anything.
- **No index.** The read stays linear in the type, small per row but growing with every object the store ever held.
- **An index chosen by how many objects a type holds.** A schema follows the declaration, not the data, or two stores of one version would differ.
- **Two shapes of an object, light and whole.** Code holding the light one where it needed the whole would read absent flow data as no flow data, the silent error lazy parts exist to prevent.
- **Compiling scans to SQL.** The language's three-valued comparisons and aggregates would have to be written again for each backend, a far larger change than reading less.

## Consequences

- `storage-schema.md` §3 and §5 name the key index; `design/scoped-reads.md` §7 records what each decision measured.
- A store whose tables were built before decision 1 gains a key index when a publish next rebuilds the table, or by an engine release's installation (`storage-schema.md` §12).
- The core's `Object` keeps its parts behind accessors that read them on first use, and every reader goes through them. A read that reaches many objects' records, a metric's family, a query's page, an export or a publish's simulation, reads them together, a type at a time; and an operation that reaches more than eight of a type's records one at a time, as a scan's filter calling a flow function does, reads the rest it holds of the type together, so no loop reads record by record. An object reaching a change or a write without its record is an error, never an object with no history.

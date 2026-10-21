# Storage schema

Draft, 2026-09-09, amended 2026-09-23. How a published declaration becomes tables, and how the guarantees of [`../DESIGN.md`](../DESIGN.md) rest on them. The model owns what a rule means, [`flow-format.md`](flow-format.md) owns how a flow is written and [`declaration-syntax.md`](declaration-syntax.md) the internal form it is converted to (ADR-0116); this document owns only where the bytes go.

**Amended 2026-09-23** for ADR-0082 to ADR-0094:
- the event gains its writing transaction, read set, occurred time and retry count, and is inserted once, complete, with its position allocated first;
- part positions are kept per relationship;
- the attempt log, the interval index, labels, the file reference index and `DeclarationChange` are new tables;
- SQLite transactions begin `IMMEDIATE`;
- the log is read by a settled cursor.

**Amended again 2026-09-23** for ADR-0097 to ADR-0101, from a review of the whole record against the PRD; each change cites the decision it carries.

**Amended 2026-09-24** for ADR-0103: the idempotency prune, and an import raising a sequence past the legacy system's last value.

**Amended 2026-09-25** for ADR-0110: `of_engine_release`, the release a store is at and every upgrade, and `of_actor`, every actor by the identity a request names.

**Amended 2026-09-25** for ADR-0114: the read set holds no capability answers and is never filtered for a reader; a subscription has no reader; a change's approver is recorded and not constrained; and no index exists for visibility.

**Amended 2026-09-25** for ADR-0111: an attempt row and its daily rollup mark a raised flag, and an event's payload names the flags it raised.

**Amended 2026-09-24** for ADR-0105 and ADR-0106, from a review of the whole record against PRD revision 5:
- a subscription names its reader and has no endpoint, since the core posts nothing (ADR-0114 later removed the reader);
- `state_source` loses `migrated`, and `of_migration` gains `admit`;
- an attempt keeps the request's non-personal values and everything it read, so it replays;
- every object records `recorded_from`;
- the writing transaction is an integer;
- a store's version 0;
- an erasure's event on every object it redacts;
- intervals opened at a port, a refresh and a publish;
- the prune keeps a would-be refusal while its clause is on trial.

**What is verified.** `scripts/check-schema-doc.py` executes every statement of SQLite DDL below against SQLite 3.37 on every corpus run. It also keeps three probes of what SQLite does at runtime:
- the two-connection sequence probe of §6, which was reasoned rather than run until D187 found it false as first written;
- the deferred-foreign-key probe of §3, which replaced a premise D191 found false;
- the transaction-mode probe of §7, which is D204's.

The PostgreSQL column is reasoned from its documentation, except two behaviours probed on PostgreSQL 16 in a throwaway container on 2026-09-23 and recorded in ADR-0089 and ADR-0090: a contended row at serialisable isolation waits and then fails, and reading below the snapshot's `xmin` cannot skip a late commit. The exclusion constraints of §8 remain unexecuted, and are the part most worth running before anyone relies on them. *(Corrected 2026-10-21, the PostgreSQL column checked against the backend ADR-0213 built, where this paragraph said the column was reasoned from documentation and probes alone. That backend runs the store's SQL as written for SQLite, translated by `db.rs`'s `to_postgres`: an `INTEGER` as `BIGINT`, a `REAL` as `DOUBLE PRECISION`, a `BLOB` as `BYTEA`, placeholders numbered, `INSERT OR IGNORE` as `ON CONFLICT DO NOTHING`, a stored end's key naming the directory (§3); its schema adds only `of_object`'s `rowid`, an identity column, where SQLite keeps creation order as its own `rowid`. One instance writes, one transaction at a time (decision 4), so a store on PostgreSQL holds the tables, columns and counters a file holds. Where a passage below gave PostgreSQL a mechanism of its own, a correction beside it says what is built; the core's and the service's tests run on both backends (decision 1). No exclusion constraint is built, by decision 7, §8.)*

## 1. Three layers, and why not fewer

| Layer | Holds | Why it is separate |
|---|---|---|
| **the directory**, `of_object` | id, type, creation time | `get(id)` and `referrers` need to resolve an opaque id to a type without knowing it in advance (ADR-0018) |
| **one table per declared type**, `t_<type>` | state, version, every stored attribute and every stored relationship end | a constraint can only span columns of one table, and §8 compiles invariants into constraints |
| **the log**, `of_event` | every recorded change, permanently | it is the history, and a fold of it must reproduce the row (ADR-0033) |

The obvious alternative is one shared object table with the attributes in a JSON column. It was rejected on §8: a uniqueness constraint over two attributes, or an exclusion constraint over a range, needs real typed columns. Without them every invariant falls back to the dynamic check, and the declaration's promise that some compile is empty.

The second alternative is to put state and version in the directory rather than in the type table. Also rejected on §8, and this one is easy to get wrong. "No two **live** bookings of one resource overlap" is one constraint over `state`, `resource_id`, `start` and `end`, and it cannot be written if `state` lives in a different table from the rest. So the directory holds nothing a constraint might need, and everything else is in the type table.

Beside the three layers sit the **flow data** of ADR-0083: `of_interval`, an index the log rebuilds, and `of_attempt`, evidence that is not history. Neither is a fourth layer, since neither is a source of truth; §6 defines both.

## 2. The directory and the log

```sql
-- The directory: the only table that knows every object exists.
CREATE TABLE of_object (
  id              TEXT    PRIMARY KEY,
  type            TEXT    NOT NULL,
  created_at      TEXT    NOT NULL,         -- when the creation happened: its occurred time,
                                            -- the legacy creation, or the port (ADR-0106)
  created_by_kind TEXT    NOT NULL,         -- the creating request's actor kind (ADR-0096)
  created_dated   INTEGER NOT NULL DEFAULT 1, -- 0 where a port could not date the creation
  recorded_from   TEXT    NOT NULL          -- from when its record is continuous: created_at
                                            -- here, or what a port vouches for (ADR-0106)
);

-- The log. Each row is inserted once, complete, at the end of its
-- transition; nothing updates it afterwards except erasure's redaction of
-- its payload (ADR-0089).
CREATE TABLE of_event (
  position            INTEGER PRIMARY KEY,          -- allocated first; see below
  txn                 INTEGER NOT NULL,             -- the writing transaction's number, of_counter's
  object_id           TEXT    NOT NULL REFERENCES of_object(id),
  object_seq          INTEGER NOT NULL,
  transition          TEXT    NOT NULL,
  from_state          TEXT,
  to_state            TEXT    NOT NULL,
  changes_state       INTEGER NOT NULL,
  source              TEXT    NOT NULL,
  actor_id            TEXT    NOT NULL,
  actor_kind          TEXT    NOT NULL,
  actor_principal     TEXT,
  context             TEXT,
  cause_position      INTEGER REFERENCES of_event(position),
  declaration_version INTEGER NOT NULL,
  taint_version       INTEGER NOT NULL,
  recorded_at         TEXT    NOT NULL,
  occurred_at         TEXT,                         -- a backdatable transition's or a record's (ADR-0099)
  imported            INTEGER NOT NULL DEFAULT 0,   -- written by import_batch (ADR-0100)
  retries             INTEGER NOT NULL DEFAULT 0,   -- serialisation retries its request took
  payload             TEXT    NOT NULL,
  reads               TEXT    NOT NULL,             -- the read set, as JSON (ADR-0088)
  CONSTRAINT of_event_per_object UNIQUE (object_id, object_seq),
  CONSTRAINT of_event_source CHECK (source IN
    ('observed','asserted','migrated','corrected','erased'))
);
CREATE INDEX of_event_by_object ON of_event (object_id, object_seq);
CREATE INDEX of_event_by_cause  ON of_event (cause_position);
CREATE INDEX of_event_by_cursor ON of_event (txn, position);
CREATE INDEX of_event_by_actor ON of_event (actor_id);  -- an erased actor's events (§9 step 15)
CREATE INDEX of_event_imported ON of_event (position) WHERE imported = 1;  -- a legacy span's export cursor, made at open (ADR-0198)
CREATE INDEX of_event_by_principal ON of_event (actor_principal) WHERE actor_principal IS NOT NULL;

-- Positions, transaction numbers and objects' numbers, allocated by the
-- one writer's transaction, on both backends (§7; corrected below).
CREATE TABLE of_counter (
  name   TEXT    PRIMARY KEY,                         -- 'position', 'txn', 'object'; 'copied' in a backup's copy
  value  INTEGER NOT NULL
);

-- changed_since: the event that last wrote each attribute, and the last
-- event on each part relationship, one row per name (ADR-0057, ADR-0082).
CREATE TABLE of_attribute_write (
  object_id  TEXT    NOT NULL REFERENCES of_object(id),
  attribute  TEXT    NOT NULL,                        -- an attribute or a part relationship
  position   INTEGER NOT NULL REFERENCES of_event(position),
  PRIMARY KEY (object_id, attribute)
);
```

`position` is both the event's **identity** and its **order within a transaction**. An `event`-typed attribute stores it, `cause_position` points at it, and the write index compares it; one column serves all three because positions are assigned once and never renumbered. It is **not** commit order across transactions, which is why a cursor reads by `(txn, position)` (§7).

**A position is allocated when its transition begins applying**, so `this_event` is known to the outcome (ADR-0046), and the row is inserted when the outcome is done, in the same transaction (ADR-0089).
- **On PostgreSQL** the position comes from the log's sequence by `nextval`, and `txn` is `pg_current_xact_id()`, stored as `xid8`, which compares numerically.
- **On SQLite** both come from `of_counter`, incremented by the request's own transaction. That transaction holds the write lock from its first statement, so no other writer can interleave.

*(Corrected 2026-10-21: the backend ADR-0213 built takes both from `of_counter` on PostgreSQL too, as on SQLite, its one writing instance writing one transaction at a time (decision 4); no sequence, `pg_current_xact_id()` or `xid8` is used, and `of_counter` is not SQLite's alone, as its comment above said.)*

Nothing is inserted early and completed later, so the append-only statement below is true. **`txn` is a number on both backends**, never text, since a settled cursor compares it: as text, `'10'` sorts before `'9'`, and a cursor would skip or reorder events once the counter crossed a power of ten (ADR-0106).

`id` is a **UUIDv7**, stored in its canonical text form on both backends; PostgreSQL may use its `uuid` type. *(As built, checked 2026-10-21: an object's id is `<type>-<n>`, its type's name in lower case and a number from `of_counter`'s `object`, text on both backends, and no `uuid` column; ADR-0077 decision 7's UUIDv7 is not what the store mints, a departure brought to the author in `TODO.md`. The store's order by id, `in_id_order`, which orders the objects a refusal names, is each object's creation order, not its id's text.)* It is time-ordered rather than random because ADR-0038 iterates a cascade in ascending id order, and the harness reduces a failing run by replaying its transcript; both need that order to be creation order. The embedded time is never read back as a fact about the object, which is what `created_at` is for. The store takes its id source, its clock and its connection source as injected dependencies, so a test can make all three deterministic (ADR-0077, ADR-0091).

`object_seq` is the per-object order. The unique constraint on `(object_id, object_seq)` is what makes "strictly ordered per object" a property of the schema rather than of the writer. It is also what makes a read set re-evaluable: `reads` records each object a rule read with its version, and a version is a point in that strict order, so no global order is needed (ADR-0088). `cause_position` is the parent event of a cascade, so causal order across objects is reconstructable without a separate table.

**The log is append-only with exactly one exception**, and the exception is why `payload` is a column rather than an immutable blob: erasure rewrites the personal values inside past events, in place, keeping the event, its shape and its position (§9). `reads` holds ids and versions, never a value (ADR-0114 withdrew the capability answers it once held), so erasure has nothing to do there.

## 3. A type becomes a table

Every declared type gets one table. Its identity columns are the same in every one:

| Column | From | Note |
|---|---|---|
| `id` | the store | primary key, and a foreign key into the directory |
| `state` | the machine | with a `CHECK` over the declared state set, so an unreachable value is refused by the database |
| `version` | the store | incremented on every recorded change; what `expected_version` compares against, and what a read set records |
| `declaration_version` | the declaration | the publish whose rules the row was last written under (ADR-0027, ADR-0077); the same number the event carries |
| `last_event` | the store | the event that last wrote this object |
| `superseded_by` | the store | present only on a type with a `superseding` state, and indexed, `t_<type>_superseded_by_ix`; `get(id, follow)` walks it (ADR-0028), and erasure follows it (ADR-0087), finding each member's predecessors by the index, where it read every superseding type whole for each member; a store opened that lacks the index is given it (ADR-0191 decision 5) |
| `state_source` | the store | the `source` of the event that last **changed** `state`, a migration's excepted: `observed`, `asserted`, or `imported` for the import's state change. A migration leaves it as it was, even where its mapping moves the state, which is why `migrated` is not a value, and an object an assertion put in its state stays listed by `exceptions` until a transition changes it (ADR-0105, ADR-0146). Indexed; an action leaves it alone. `exceptions(type)` reads `asserted` (ADR-0083) |

There is no `last_part_event` column. A part relationship's last event is a row of `of_attribute_write` keyed by the relationship's name, so a guard naming `checklist_items` is not invalidated by a change to `approvals` (ADR-0082, repairing D202).

**Absence is SQL `NULL` and never a sentinel** (ADR-0051). That is not a stylistic choice: the erasure marker must collide with no uniqueness constraint and must read as unknown, and a sentinel does neither. It follows that every uniqueness over a `personal` or `external` attribute is **partial**, ignoring absent values.

A declared-required attribute takes `NOT NULL`. The database is not the backstop — a guard is (ADR-0001) — and check 8 is what actually enforces requiredness at publish. `NOT NULL` is the same second line of defence a compiled constraint is in §8: redundant when everything works, and the thing that catches a runtime with a bug in it.

Then one column per stored attribute, one per counter, and one per **stored** relationship end. There is nothing for a derived inverse, for a derivation that is not indexed, or for a `part` or `ref` whose other end stores the value. Those are queries, and putting a column there would be the second write path the model refuses (ADR-0056 §1).

A stored end's foreign key names its target's table, or the directory, `of_object`, where the target has subtypes, whose objects are in their own tables; the runtime checks a reference's type either way, and the constraint is the backstop that the object exists (D520). On PostgreSQL every stored end's key names the directory, since a flow's references may form a cycle and PostgreSQL refuses a key to a table not yet made: the key holds there that the object exists, and its type is the runtime's check alone (the PostgreSQL records review, accepted 2026-10-19). The foreign key on a stored end is declared **`DEFERRABLE INITIALLY DEFERRED`** on both backends and checked at commit. The runtime resolves every reference itself and the constraint is the backstop, so checking it at commit loses nothing. It is what lets one transaction insert two rows that require each other: an outcome that creates a whole and a part which each require the other, or an import writing a cycle of required references (ADR-0077). SQLite enforces foreign keys only with `PRAGMA foreign_keys = ON`, which the store sets on every connection it opens. `scripts/check-schema-doc.py` demonstrates the deferred case, the same pair failing without the clause, and a dangling reference still failing at commit.

**An observation kind is a type** and gets a table like any other (ADR-0082):
- its fields are columns;
- its `owner` end is `subject_id`;
- it has one state;
- a correction's link to the observation it corrects is a nullable `corrects_id`;
- its occurred time is `occurred_at`, since metrics filter and bucket on it (ADR-0099).

**A mirror type's table has `imported_at`**, the time its last import wrote the row, which a guard may bound (ADR-0100). *(Withdrawn 2026-10-16 by ADR-0210 decision 3: no store kept it, and "the last import that wrote it" meant the last that changed it, which no freshness guard can use; the checkers refuse reading it and keep the name.)*

A numeric field's declared unit is metadata in `of_declaration`, not a column, since it never varies by row.

### 3.1 What each declared type becomes

| Declared | PostgreSQL | SQLite | Why |
|---|---|---|---|
| `string` | `TEXT` | `TEXT` | |
| `bool` | `BIGINT` 0/1 | `INTEGER` 0/1 | |
| `int` | `BIGINT` | `INTEGER` | |
| `decimal(p,s)` | `BIGINT`, scaled by 10^s | `INTEGER`, scaled by 10^s | SQLite's numeric affinity stores a decimal as a float, which is not exact; the scale is in the declaration, so the scaling is recoverable |
| `money(ccy)` | `BIGINT`, minor units | `INTEGER`, minor units | exact, orderable and indexable on both. The currency is fixed by the declaration (ADR-0068) so it is not stored per row |
| `timestamp` | `TEXT`, ISO-8601 UTC | `TEXT`, ISO-8601 UTC | held to the second, the engine reading its clock to the second, so a duration is whole seconds (ADR-0147) |
| `date` | `TEXT`, ISO-8601 date | `TEXT`, ISO-8601 date | a calendar day with no zone, which orders as text as the day does (ADR-0145) |
| `duration` | `BIGINT` seconds | `INTEGER` seconds | the unit set is closed and none is calendar-dependent (ADR-0061) |
| `identity` | `TEXT` | `TEXT` | |
| `actor_kind` | `TEXT` + `CHECK` | `TEXT` + `CHECK` | `human`, `agent` or `service`, the kind of actor that acted (ADR-0145) |
| an enum | `TEXT` + `CHECK` | `TEXT` + `CHECK` | a native enum type would need a migration to add a member; a check constraint is replaced with the declaration |
| `event` | `BIGINT` → `of_event.position` | `INTEGER` | which is what makes `changed_since([…], a.at_event)` a comparison of two integers |
| `file` | `TEXT` | `TEXT`, the hash, size and media type as JSON | content-addressed; the store holds the reference, never the content (ADR-0017), and keeps no file table (§6) *(corrected 2026-10-15: it named `of_file` and `of_file_ref`, which the core never made)* |
| `<T>[]` | a side table | a side table | §3.3 |
| `ref`/`owner`, singular and stored | column + foreign key, deferred to commit + index | same | the index is not optional; §5 |
| `ref`/`part`, the derived end | **nothing** | **nothing** | it is a query against the other end's index |
| `counter` | `BIGINT NOT NULL DEFAULT 0` | `INTEGER NOT NULL DEFAULT 0` | non-negativity is an invariant the type declares, not a property of the column (ADR-0072) |
| `derive`, not indexed | **nothing** | **nothing** | evaluated on read |
| `derive`, indexed | generated column + index | generated column + index | §3.4 |
| a scalar marked `indexed`, or an external identifier | column + index | column + index | a filter by it, and an external identifier's uniqueness, which every import and publish checks, read through the index, not the table whole (D593, D606); a store opened that lacks one is given it |

*(Corrected 2026-10-21: the PostgreSQL column gave `bool` as `BOOLEAN`, `decimal(p,s)` as `NUMERIC(p,s)`, `timestamp` as `TIMESTAMPTZ` and `date` as `DATE`. The backend ADR-0213 built holds each as SQLite does, an `INTEGER` as `BIGINT` (`store.rs`'s `sql_type`, `db.rs`'s `to_postgres`), so a timestamp is ISO-8601 text on both and orders as text does. The row for an indexed derived attribute is not built on either backend: no generated column is made, which `TODO.md` records to settle.)*

### 3.2 Worked, from the specification's own types

```sql
-- type Robot version 3, binding the UnitLifecycle machine
CREATE TABLE t_robot (
  id                   TEXT    PRIMARY KEY REFERENCES of_object(id),
  state                TEXT    NOT NULL,
  version              INTEGER NOT NULL,
  declaration_version  INTEGER NOT NULL,
  last_event           INTEGER NOT NULL REFERENCES of_event(position),
  state_source         TEXT    NOT NULL DEFAULT 'observed',
  serial               TEXT    NOT NULL,          -- identifier, unique in scope
  manufacturer_serial  TEXT,
  label_printed_at     TEXT,                      -- timestamp
  retirement_reason    TEXT,
  model_id             TEXT    NOT NULL REFERENCES t_robotmodel(id) DEFERRABLE INITIALLY DEFERRED,
  binding_id           TEXT    REFERENCES t_delivery(id) DEFERRABLE INITIALLY DEFERRED,
  CONSTRAINT t_robot_state CHECK (state IN
    ('REQUESTED','PROCUREMENT','INTAKE','AVAILABLE','RESERVED',
     'SOLD','DEVELOPMENT','RETIRED','CANCELLED')),
  CONSTRAINT t_robot_retirement_reason CHECK (retirement_reason IS NULL
     OR retirement_reason IN ('FAILED','DAMAGED','OBSOLETE','LOST')),
  CONSTRAINT t_robot_state_source CHECK (state_source IN
    ('observed','asserted','imported')),
  CONSTRAINT t_robot_serial_in_scope UNIQUE (model_id, serial)
);
CREATE INDEX t_robot_state_ix   ON t_robot (state);
CREATE INDEX t_robot_asserted_ix ON t_robot (state_source);  -- exceptions(type)
CREATE INDEX t_robot_model_ix   ON t_robot (model_id);      -- stored end
CREATE INDEX t_robot_binding_ix ON t_robot (binding_id);    -- stored end, and the inverse `units`
CREATE INDEX t_robot_serial_key_ix ON t_robot (serial, model_id);  -- the key `unique in scope` reads by
```

Four things to read off it:
- **`serial` carries `unique in scope`,** and the scope is the `model` reference, so the constraint is over both columns. The declaration said "scoped by model", and the schema says the same thing in the only language the database enforces.
- **`binding_id` is indexed** because it is the stored end of `Robot.binding`, and `Delivery.units` is a query against that index. §5 explains why the index is a correctness requirement, not a tuning choice.
- **`engagement_lines` and `leasable` have no columns** at all.
- **`retirement_reason`, `model_id` and `binding_id` are tracked**, as an enum and two singular references. Their intervals are rows of `of_interval` (§6), not columns here.

### 3.3 A set-valued attribute is its own table

```sql
CREATE TABLE t_robot__photos (
  object_id  TEXT    NOT NULL REFERENCES t_robot(id),
  ordinal    INTEGER NOT NULL,
  value      TEXT    NOT NULL,          -- a file, as its column holds one (§3.1)
  PRIMARY KEY (object_id, ordinal)
);
```

An array column would hold the same data. A table is chosen for two reasons:
- `count(p in photos)` is then an ordinary indexed count rather than a function over an array;
- `add` and `remove` are read-modify-write against rows the transaction already locks;

*(Corrected 2026-10-15: a third reason was a foreign key into a file table, which the core never made; see §6.)*

### 3.4 An indexed derivation is a generated column

```sql
CREATE TABLE t_stock (
  id        TEXT PRIMARY KEY,
  state     TEXT NOT NULL,
  on_hand   INTEGER NOT NULL DEFAULT 0,
  reserved  INTEGER NOT NULL DEFAULT 0,
  available INTEGER GENERATED ALWAYS AS (on_hand - reserved) STORED
);
CREATE INDEX t_stock_available_ix ON t_stock (available);
```

This is the case ADR-0048 allows: a derivation over stored indexed columns of the same object, with no aggregate and no clock. The database computes it and **refuses to let anyone write it**; attempting to produces `cannot UPDATE generated column`, verified. So "derived attributes are never stored, evaluated on read" stops being a discipline the runtime has to keep, and becomes something the storage layer enforces on it.

A derivation that reads a clock or another object gets no column. That is why a time-dependent predicate is answered by filtering the stored operand it compares against `now` instead (ADR-0048). Where that operand is when a tracked value was entered — `entered_at(unit) + 14 days <= now` — it is `of_interval.entered_at`, stored and indexed, and `query` joins to it (ADR-0084).

## 4. The event payload

`payload` is the writes and the inputs of one transition, as JSON: attribute name to value, in the types of §3.1. Its `inputs` are what the request sent, and its `wrote` each attribute of its object the event changed, by an input, an effect, a default, a minted value, a correction, an import, a migration or an erasure, with its value after and `null` where it cleared one; an event that changed none holds no `wrote`. A fold of an object's events, its state from each `to_state` and its attributes from each `wrote`, reproduces its row (ADR-0033 decision 2, ADR-0209 decision 1). *(The core recorded the inputs alone until 2026-10-16, so its events before then hold no `wrote`, D692.)* Where a guard consulted an external evaluator or a metric, it also holds the verdict or value it was given and that value's as-of time, keyed by the guard's name and, within it, by the reference with its bindings, `first_pass_rate(model: <id>)` or `vault.cleared(<argument>)`, since one guard may consult a metric for each of several objects (ADR-0049, ADR-0084, ADR-0095, ADR-0158), and the clauses marked `flag` that failed for it, so what a request was told outlives the prune of its attempt rows (ADR-0111). An override's payload holds its `reason`, by which overrides are counted (`flow-format.md` §4.8). A correction's payload also holds `corrected`: for each attribute it lists, the value it replaced, the value it wrote, and the entered position of the interval it relabelled, null for an untracked attribute, which has none; and its `reason`, unless that input is personal (`declaration-syntax.md` §6.4). Erasure nulls a personal attribute's replaced and written values there as it does its value among the inputs (§9 step 2).

It is JSON rather than columns for two reasons. Its shape is per transition. And a fold of the log must reproduce a row written under a **declaration version that may no longer exist**; a column set fixed by today's declaration could not hold yesterday's event.

`reads` is the read set of ADR-0088, also JSON:
- the objects the transition's rules read, each with the version read, but the event's own object, whose version read is the one before the event's (`DESIGN.md` §6);
- for each guard, each type it scanned and the ids satisfying the scan's `where`, an empty match recorded as empty, an object the scan examined and rejected not being read (ADR-0158);
- the `now` the request fixed.

It holds no value, only identities and versions, and every reader gets it whole (ADR-0114).

That is also why `declaration_version` is on the event and not inferred: reading history means reading each event under the rules that were in force when it was recorded.

## 5. The indexes the model requires to be correct

Most indexes are performance. These are not: a stated guarantee is false without them.

| Index | Without it |
|---|---|
| the stored end of every relationship | a derived inverse becomes a table scan, so `Delivery.units` is O(all robots). This index **is** the reverse index `referrers` needs, one per declared reference, covering the ends with no declared `inverse` too — which is the case that made `referrers` necessary. ADR-0056 states the cost plainly: a deployment pays index maintenance on every reference write, and takes it because deletion correctness is load-bearing and deletion is rare |
| `of_attribute_write (object_id, attribute)` | `changed_since` is a scan of the object's whole history, and it is the most-cited guard in the model (ADR-0035). Keyed by part relationship too, it answers the half of `changed_since` that reaches a composition, per relationship (ADR-0057, ADR-0082) |
| every attribute a type-scan invariant reads | the invariant's affected set is a full scan on every write |
| every uniqueness's key, its columns together, a plain index and not a constraint: `t_<type>_<attribute>_key_ix`, a reference's key excepted, its column indexed as a stored end | a uniqueness is a scan of its type on every write of a key, and of every other object holding the key; as an index and not a `UNIQUE` constraint, it refuses nothing an admission lets stand (§8, ADR-0165) |
| `of_interval` on the current interval of each dimension, and on its entry time | work in progress, current age and every ageing condition become scans of the whole index (ADR-0083, ADR-0084) |
| `of_file_ref (hash)` | erasure cannot tell whether another object still holds a file, and either deletes it too (D207) or never deletes anything (ADR-0087). *Never made: the core keeps no file table (§6). Replaced by ADR-0211 decision 1: `referenced` reads every place a value is kept for the hashes asked, so a file two objects share is held while either holds it, and the application deletes the bytes.* |
| `of_event (txn, position)` | the settled cursor becomes a sort of the whole log on every `pull` and `export` (ADR-0089) |
| `superseded_by` on a type with a superseding state | a supersession group's erasure reads each superseding type whole for each member, under the writer, 1.43 s for a group of 41 (ADR-0191 decision 5) |

The third is why check 7 rejects a type-scan over an unindexed attribute at publish: the schema cannot be built to satisfy it afterwards.

**`exceptions(type)` reads two things and no third.**
- **Admissions** come from `of_admission` where `discharged_position` is null.
- **Asserted state** comes from `state_source = 'asserted'` on the type table, indexed. The import's state changes are `imported` and are not listed (ADR-0083, repairing D205).

A column is chosen over scanning the log because the query is per type, and the log is the busiest table in the system. It costs one indexed column on every object, and it is written by the same statement that writes the state, so it cannot disagree with the event.

`state_source` changes in three ways:
- **An ordinary transition that changes state** sets it back to `observed`, which is right: an object whose state was asserted and has since moved through the machine normally is no longer an exception, and nobody has to remember to clear a flag.
- **An action leaves it alone.** Editing a note on an asserted unit does not make the unit's state any less asserted, and the object stays in `exceptions(type)` until the machine has moved it (ADR-0077).
- **A migration mapping leaves it alone** too, so an asserted object does not drop off the list by being migrated (ADR-0083).

`of_attribute_write` deserves its shape stated plainly. It has one row per object per attribute or part relationship ever written, holding the position of the event that last wrote it. It grows with the object's *width*, not with its history, so it is bounded by the declaration.

## 6. The built-in tables

```sql
-- Named sequences, scoped. Allocated on a SECOND CONNECTION in its own
-- transaction, committed before the request continues, so the allocation
-- survives a rollback of the request and leaves the gap ADR-0029 requires.
-- Updating this row inside the request's transaction would make sequences
-- gapless, which that decision rejected: it serialises every creation in
-- the scope. On SQLite this table lives in a SEPARATE DATABASE FILE beside
-- the store: SQLite's write lock is per file, so a second connection to the
-- store's own file blocks behind the request's transaction (ADR-0076, D187).
-- The mint's connection comes from a pool of its own, never the request
-- pool, so a burst of creations cannot starve itself (ADR-0090, D213).
CREATE TABLE of_sequence (
  name       TEXT    NOT NULL,
  scope_key  TEXT    NOT NULL,
  next_value INTEGER NOT NULL,
  PRIMARY KEY (name, scope_key)
);
-- An import batch may carry, per sequence and scope key, the last value the
-- legacy system minted; the import raises next_value past it and never lowers
-- it, so a serial minted after cutover follows the last one ported (ADR-0103,
-- ADR-0154).

-- External identifiers, which lookup(source, value) answers from. A legacy
-- key is kept under source 'legacy', its value qualified by its family's root
-- type, '<Type>:<key>', and its attribute '', since no attribute holds it
-- (ADR-0154).
CREATE TABLE of_external_id (
  source     TEXT NOT NULL,
  value      TEXT NOT NULL,
  object_id  TEXT NOT NULL REFERENCES of_object(id),
  attribute  TEXT NOT NULL,
  PRIMARY KEY (source, value)
);

-- Idempotency. The recorded verdict is replayed verbatim.
-- Scoped to the actor's id, not global: a key is unique per caller, which is
-- what the first consumer's own evidence shows and what `unique with` was
-- added to the declaration syntax to express, and `id` is the one member of
-- the descriptor that is never absent (ADR-0077). A global key space would
-- let one caller's retry collide with another's. A key reused by the same
-- actor with a different request_digest is refused as KeyReused.
CREATE TABLE of_idempotency (
  actor_id       TEXT    NOT NULL,
  key            TEXT    NOT NULL,
  request_digest TEXT    NOT NULL,
  first_position INTEGER REFERENCES of_event(position),
  verdict        TEXT    NOT NULL,
  applied_at     TEXT    NOT NULL,
  PRIMARY KEY (actor_id, key)
);
-- maintain(prune_idempotency(before)) deletes rows applied before `before`,
-- refusing a `before` inside the store's idempotency retention; a retry with a
-- pruned key is then a new request (ADR-0103).

-- Admissions: an invariant a transition was permitted to violate.
-- exceptions(type) is a query over this table.
CREATE TABLE of_admission (
  object_id           TEXT    NOT NULL REFERENCES of_object(id),
  invariant           TEXT    NOT NULL,
  position            INTEGER NOT NULL REFERENCES of_event(position),
  reason              TEXT    NOT NULL,
  discharged_position INTEGER REFERENCES of_event(position),
  PRIMARY KEY (object_id, invariant, position)
);
CREATE INDEX of_admission_by_object ON of_admission (object_id);

-- Files. The core holds a file value as its hash, size and media type, in
-- its attribute's column and the events that carried it (§3.1), and keeps no
-- file table and no content: the bytes are the application's. The design gave
-- the engine the deletion of an erased file's bytes through of_file and
-- of_file_ref (ADR-0087); the core never made either. The application deletes
-- the bytes, asking `referenced` which hashes the store still holds, a scan of
-- every table's text (ADR-0211 decision 1). (Corrected 2026-10-15: the two
-- tables were sketched here as built.)

-- The published declaration, one row per version. Never deleted: a recorded
-- event names the version whose rules applied, and history must stay readable.
CREATE TABLE of_declaration (
  version       INTEGER PRIMARY KEY,  -- 0 is the row the store is created with (ADR-0105)
  module        TEXT    NOT NULL,
  source        TEXT    NOT NULL,   -- the source's files as published, a JSON object from each
                                    -- module's name to its description, and from <module>.examples
                                    -- to its examples file (ADR-0135, ADR-0136)
  source_ref    TEXT,               -- where the source came from, as the change named it, and
  source_digest TEXT,               -- its digest; neither for version 0 (ADR-0135)
  builtins      TEXT    NOT NULL,   -- the built-in module it was published with: the engine
                                    -- release whose built-in types and standard metrics apply
  published_at  TEXT    NOT NULL,
  published_by  TEXT    NOT NULL,   -- for version 0, the engine release that created the store
  change_id     TEXT,               -- the DeclarationChange it installed (ADR-0085); none for 0
  release       TEXT,               -- the engine release an upgrade's version names as its
                                    -- cause; none for a publish's (ADR-0147, ADR-0148)
  report        TEXT    NOT NULL    -- what publishing reported, kept for audit; for version 0,
                                    -- the built-in module's own check report
);
-- (Corrected 2026-10-15: the sketch had a `parsed` column, the checked form,
-- and a CHECK tying source_ref to source_digest, neither of which the core
-- made: it parses `source` as it loads a version, and its publish writes the
-- two together.)

-- The engine releases this store has been at: the one that created it, then one
-- row per explicit upgrade (ADR-0110). Every request reads the last row in its
-- transaction, and a core of any other release refuses, so two releases never
-- serve one store at once.
CREATE TABLE of_engine_release (
  seq           INTEGER PRIMARY KEY,  -- 1 when the store is created, one more per upgrade
  release       TEXT    NOT NULL,     -- the release whose tables and meanings apply
  installed_at  TEXT    NOT NULL,
  installed_by  TEXT    NOT NULL,     -- the operator who created the store or asked for the upgrade (§12)
  commit_id     TEXT                  -- the commit the build that wrote the row was made from, where its build was told, as a release's build is; none before 0.3.0 (the audit's durability U7)
);

-- Each interval row a later commit deleted or moved, which the `.intervals`
-- export retracts at that commit's position (ADR-0205 decision 9): a
-- correction merging a span into the one before it, a version renaming a
-- member, whose rows it moves to the new name, setting of_interval's
-- moved_position to the publish's, or retyping one, whose rows it sets aside.
CREATE TABLE of_interval_retraction (
  position          INTEGER NOT NULL,
  object_id         TEXT    NOT NULL,
  dimension         TEXT    NOT NULL,   -- the name the export gave the row
  entered_position  INTEGER NOT NULL,
  entered_at        TEXT    NOT NULL,
  PRIMARY KEY (position, object_id, dimension, entered_position, entered_at)
);

-- Each restore of a copy (ADR-0207): a backup's copy carries a 'copied' row in
-- of_counter, which the service never writes into a store it serves, and a store
-- opened with it advances every counter and sequence past the most it recorded in
-- any period as long as the copy's age, doubled, to a multiple of 1,000, in the
-- transaction that records the restore here and removes the mark. Each gap runs
-- from the value the copy held, exclusive, to the one it was advanced to; a cursor
-- in one is refused, and a sequence's scope the copy never held starts past
-- sequence_floor.
CREATE TABLE of_restore (
  seq             INTEGER PRIMARY KEY,
  restored_at     TEXT    NOT NULL,
  copy_last_at    TEXT    NOT NULL,   -- the copy's last recorded event or attempt
  margin          INTEGER NOT NULL,
  position_from   INTEGER NOT NULL,
  position_to     INTEGER NOT NULL,
  txn_from        INTEGER NOT NULL,
  txn_to          INTEGER NOT NULL,
  object_from     INTEGER NOT NULL,
  object_to       INTEGER NOT NULL,
  sequence_floor  INTEGER NOT NULL
);

-- Actors and migrations have no tables of their own. An actor is an object of
-- a type marking an actor identity (ADR-0110), and a request's actor is found
-- by that identity through its type's folded index, `t_<type>_<attr>_fold_ix`
-- (ADR-0194 decision 1). A publish's migration is part of its source, in
-- of_declaration.source, its effects the `migrate` events it records and the
-- of_admission rows its admits write. (Corrected 2026-10-15: of_actor and
-- of_migration were sketched here, and the core never made them.)

-- Subscription is a built-in object. Its progress is not (ADR-0043). It is read
-- by pull alone; posting to an endpoint is an upper-layer relay (ADR-0105).
CREATE TABLE t_subscription (
  id             TEXT    PRIMARY KEY REFERENCES of_object(id),
  state          TEXT    NOT NULL,
  version        INTEGER NOT NULL,
  declaration_version INTEGER NOT NULL,
  last_event     INTEGER NOT NULL REFERENCES of_event(position),
  state_source   TEXT    NOT NULL DEFAULT 'observed',   -- as every type's (§3)
  target_type    TEXT    NOT NULL,
  target_family  INTEGER,
  changes_state_only INTEGER,
  lagging_after  INTEGER,                    -- seconds: lag is the age of the oldest
  dead_after     INTEGER,                    -- unacknowledged event the filter selects (ADR-0100)
  CONSTRAINT t_subscription_state CHECK (state IN ('ACTIVE','REVOKED'))
);
-- the transitions it selects, a set of values in a table of its own (§3.3);
-- none means every transition
CREATE TABLE t_subscription__transitions (
  object_id  TEXT    NOT NULL REFERENCES t_subscription(id),
  ordinal    INTEGER NOT NULL,
  value      TEXT    NOT NULL,
  PRIMARY KEY (object_id, ordinal)
);
-- (Corrected 2026-10-15: `transitions` was a column, and `state_source`
-- missing.)
-- (Corrected 2026-10-06: the thresholds were lag_warn and lag_fail and the
-- states lower case, where DESIGN.md §7, the rule's home, names them
-- lagging_after and dead_after and the states ACTIVE and REVOKED; building
-- the core's subscriptions found it.)

-- Runtime state, deliberately not an object: no version, no events, no history.
CREATE TABLE of_subscription_position (
  subscription_id  TEXT    PRIMARY KEY REFERENCES t_subscription(id),
  acknowledged     TEXT    NOT NULL,         -- a settled cursor (ADR-0089)
  acknowledged_at  TEXT    NOT NULL
);

-- Proposal is a built-in object type.
CREATE TABLE t_proposal (
  id             TEXT    PRIMARY KEY REFERENCES of_object(id),
  state          TEXT    NOT NULL,
  version        INTEGER NOT NULL,
  declaration_version INTEGER NOT NULL,
  last_event     INTEGER NOT NULL REFERENCES of_event(position),
  state_source   TEXT    NOT NULL DEFAULT 'observed',   -- as every type's (§3)
  target         TEXT,                      -- the object's id; none for a proposed creation
  target_type    TEXT    NOT NULL,
  transition     TEXT    NOT NULL,
  inputs         TEXT    NOT NULL,
  proposer       TEXT    NOT NULL,
  context        TEXT,                      -- the proposer's reason, which its execution's events
                                            -- record (DESIGN.md §9 lists it; added 2026-10-06)
  occurred       TEXT,                      -- the occurred time the proposed request gave
  filed_at       TEXT    NOT NULL,
  -- the verdict that left it pending is not stored: a read derives it from
  -- the refused approval's of_attempt row, which names it in its proposal
  -- column, the proposal getting no event (ADR-0044, ADR-0157)
  expires_at     TEXT,          -- expiry is derived from this, never a state
  CONSTRAINT t_proposal_state CHECK (state IN
    ('PENDING','EXECUTED','REJECTED','WITHDRAWN','INVALIDATED'))
);
-- an erasure reads the pending proposals on the erased object by it
CREATE INDEX t_proposal_target_ix ON t_proposal (target, state);
-- (Corrected 2026-10-15: the sketch had `target_id`, a reference, and
-- `submitted_under`, and lacked `occurred`, `filed_at` and `state_source`;
-- and the core made the index only that day, an erasure scanning every
-- proposal until then.)

-- DeclarationChange is a built-in object type: how a flow changes (ADR-0085).
-- The publish event belongs to it (D212).
CREATE TABLE t_declarationchange (
  id             TEXT    PRIMARY KEY REFERENCES of_object(id),
  state          TEXT    NOT NULL,
  version        INTEGER NOT NULL,
  declaration_version INTEGER NOT NULL,
  last_event     INTEGER NOT NULL REFERENCES of_event(position),
  state_source   TEXT    NOT NULL DEFAULT 'observed',   -- as every type's (§3)
  source         TEXT    NOT NULL,          -- the modules proposed, as of_declaration.source
  source_ref     TEXT    NOT NULL,          -- where they came from, such as a commit, opaque to the
                                            -- engine, which checks its form, 1 to 512 characters (ADR-0135)
  source_digest  TEXT    NOT NULL,          -- sha256 over the modules, which the repository's side
                                            -- recomputes at the reference (ADR-0135)
  base_version   INTEGER NOT NULL,          -- the version it was drafted against
  report         TEXT,                      -- the dry run: the impact report, with its ids
  evidence       TEXT,                      -- the references the draft cites, each a read's arguments,
                                            -- and each read's page or refusal as it answered at
                                            -- submission or refresh, with its as-of time (ADR-0097, ADR-0159)
  drafted_by     TEXT    NOT NULL,
  drafted_by_kind TEXT   NOT NULL,
  approved_by    TEXT,                      -- recorded; who may approve is the upper layer's
  approved_by_kind TEXT,                    -- (ADR-0114)
  CONSTRAINT t_declarationchange_state CHECK (state IN
    ('DRAFTED','SUBMITTED','PUBLISHED','REJECTED','WITHDRAWN','SUPERSEDED'))
);
-- (Corrected 2026-10-15: the table is named for the type, DeclarationChange,
-- as every type's is; the sketch had `drafted_principal` and two CHECKs the
-- core does not declare, the source reference's length being the engine's
-- input check.)

-- Operator is a built-in object type: the people answerable for a store
-- (ADR-0126). Creating the store writes its first object, whose install event
-- names the operator's own identity as its actor (ADR-0134).
CREATE TABLE t_operator (
  id             TEXT    PRIMARY KEY REFERENCES of_object(id),
  state          TEXT    NOT NULL,
  version        INTEGER NOT NULL,
  declaration_version INTEGER NOT NULL,
  last_event     INTEGER NOT NULL REFERENCES of_event(position),
  state_source   TEXT    NOT NULL DEFAULT 'observed',   -- as every type's (§3)
  identity       TEXT    NOT NULL,          -- marked actor human; an identity names
                                            -- one actor across every type (DESIGN.md §5.8)
  CONSTRAINT t_operator_state CHECK (state IN ('ACTIVE','RETIRED'))
);
-- the identity held unique by the engine's uniqueness, through its key's
-- index, and found by any case through its folded index (ADR-0165, ADR-0194)
CREATE INDEX t_operator_identity_key_ix  ON t_operator (identity);
CREATE INDEX t_operator_identity_fold_ix ON t_operator (lower(identity));
-- (Corrected 2026-10-15: the sketch had a UNIQUE constraint, and lacked
-- `state_source`.)

-- Imported history that predates the store (ADR-0015): read-only, not events.
-- Erasure of the object redacts every payload field the import mapping did
-- not list as kept for the entry's kind (ADR-0078).
CREATE TABLE of_legacy_entry (
  object_id   TEXT    NOT NULL REFERENCES of_object(id),
  ordinal     INTEGER NOT NULL,
  kind        TEXT    NOT NULL,              -- its entry kind (ADR-0163)
  occurred_at TEXT    NOT NULL,
  payload     TEXT    NOT NULL,
  kept        TEXT    NOT NULL,              -- the fields its kind kept through an erasure when it
                                             -- was ported, a JSON list; none for a kind not listed
  PRIMARY KEY (object_id, ordinal)
);

-- Labels: the built-in observation kind every type may carry (ADR-0082).
-- A label is an object, recorded by a creation request, like any observation.
CREATE TABLE t_label (
  id             TEXT    PRIMARY KEY REFERENCES of_object(id),
  state          TEXT    NOT NULL,
  version        INTEGER NOT NULL,
  declaration_version INTEGER NOT NULL,
  last_event     INTEGER NOT NULL REFERENCES of_event(position),
  state_source   TEXT    NOT NULL DEFAULT 'observed',   -- as every type's (§3)
  name           TEXT    NOT NULL,          -- normalised: lowercase, trimmed
  note           TEXT,                      -- treated as personal: erasure redacts it
  subject_state  TEXT    NOT NULL,          -- the subject's state when it was applied
  subject_id     TEXT    NOT NULL REFERENCES of_object(id) DEFERRABLE INITIALLY DEFERRED,
  corrects_id    TEXT    REFERENCES t_label(id) DEFERRABLE INITIALLY DEFERRED,
  recorded_at    TEXT    NOT NULL,
  occurred_at    TEXT    NOT NULL,
  CONSTRAINT t_label_state CHECK (state IN ('RECORDED'))
);
CREATE INDEX t_label_subject_id_ix  ON t_label (subject_id);
CREATE INDEX t_label_corrects_id_ix ON t_label (corrects_id);
CREATE INDEX t_label_by_name        ON t_label (name, subject_state);
CREATE INDEX t_label_last_event_ix  ON t_label (last_event);   -- an export pages by it
-- (Corrected 2026-10-15: the sketch lacked `state_source` and `recorded_at`,
-- gave `occurred_at` as optional and `state` a default, and named the
-- subject's index t_label_by_subject.)

-- The interval index: how long each object held each state and each value
-- of each tracked attribute and reference (ADR-0083, ADR-0086). Maintained
-- in the transition's transaction; the log can rebuild it.
CREATE TABLE of_interval (
  object_id        TEXT    NOT NULL REFERENCES of_object(id),
  dimension        TEXT    NOT NULL,        -- 'state', or the tracked member's name
  value            TEXT,                    -- NULL for absence: time unassigned is an interval
  entered_position INTEGER NOT NULL,        -- the event that set it: a request's, the import's
                                            -- or a publish's (ADR-0106); legacy intervals use 0
  left_position    INTEGER,                 -- NULL while it is the current value
  entered_at       TEXT    NOT NULL,        -- the occurred time where backdated
  left_at          TEXT,
  entered_by       TEXT,                    -- who made the change; a legacy row's where known
  entered_by_kind  TEXT    NOT NULL,        -- 'unknown' where a legacy record names no actor
  declaration_version INTEGER NOT NULL,
  legacy           INTEGER NOT NULL DEFAULT 0,   -- supplied by the import mapping
  migrated         INTEGER NOT NULL DEFAULT 0,   -- opened by a publish's migration (ADR-0146)
  corrected_by     INTEGER REFERENCES of_event(position),  -- the latest correction that changed
                                            -- its value or its .held (ADR-0147, D522)
  moved_position   INTEGER,                 -- the publish that renamed its member, which the
                                            -- export gives it again at (ADR-0205 decision 9)
  PRIMARY KEY (object_id, dimension, entered_position, entered_at)
);
CREATE INDEX of_interval_current ON of_interval (dimension, value) WHERE left_position IS NULL;
CREATE INDEX of_interval_entered ON of_interval (dimension, entered_at) WHERE left_position IS NULL;
CREATE INDEX of_interval_by_object ON of_interval (object_id, dimension, left_position);
CREATE INDEX of_interval_by_entered ON of_interval (entered_by) WHERE entered_by IS NOT NULL;  -- an erased actor's entries (§9 step 15)
CREATE INDEX of_interval_by_cursor ON of_interval (max(entered_position, coalesce(left_position, 0), coalesce(corrected_by, 0)));
                                            -- the event that last wrote it, an export's cursor, made at open (ADR-0198)

-- The attempt log: every request that did not apply, every observing
-- clause's would-be refusal, and every raised flag. No personal value. Not history: pruned after a
-- retention period, its daily counts kept in of_attempt_rollup (ADR-0083, ADR-0096).
CREATE TABLE of_attempt (
  id                  INTEGER PRIMARY KEY,
  at                  TEXT    NOT NULL,
  actor_id            TEXT    NOT NULL,     -- as the request named it, where no actor holds it (ADR-0125)
  actor_kind          TEXT    NOT NULL,     -- 'unknown' where the request named an actor the store does not know
  actor_principal     TEXT,
  context             TEXT,
  txn                 INTEGER NOT NULL,     -- its writing transaction, for the settled cursor
  type                TEXT,                 -- none where the request named an unknown id
  object_id           TEXT,                 -- none for a refused creation
  transition          TEXT,                 -- none for a request naming no transition
  verdict             TEXT    NOT NULL,     -- the verdict kind, or the fault's name
  clause              TEXT,
  remedy              TEXT    NOT NULL,     -- every refusal names one, a reused key
                                            -- self_serviceable (ADR-0105)
  unknown             INTEGER NOT NULL DEFAULT 0,
  enforced            INTEGER NOT NULL DEFAULT 1,   -- 0 for an observing clause or a flag
  flagged             INTEGER NOT NULL DEFAULT 0,   -- 1 for a raised flag (ADR-0111)
  applied_position    INTEGER,              -- the applied event of an observing clause or a flag
  call                TEXT,                 -- for a refused call or creation, the object it targeted and
                                            -- the kind of the verdict it received, as JSON (ADR-0159)
  proposal            TEXT,                 -- for an approval its held request refused, the proposal,
                                            -- whose last_verdict the row is (ADR-0157; added 2026-10-06)
  reads               TEXT,                 -- everything the request had read when refused, its effects' reads under `effect` among it (ADR-0088, ADR-0105, ADR-0210)
  consulted           TEXT,                 -- its metric values and evaluator verdicts (ADR-0096)
  request_values      TEXT,                 -- every non-personal input and the occurred time of the
                                            -- request; personal ones by name, withheld (ADR-0105)
  declaration_version INTEGER NOT NULL
);
CREATE INDEX of_attempt_by_rule ON of_attempt (type, transition, clause, at);
CREATE INDEX of_attempt_by_actor ON of_attempt (actor_id, at);
CREATE INDEX of_attempt_by_principal ON of_attempt (actor_principal) WHERE actor_principal IS NOT NULL;  -- §9 step 15
CREATE INDEX of_attempt_by_txn ON of_attempt (txn);  -- an export's cursor, made at open (ADR-0198)

CREATE TABLE of_attempt_rollup (
  day          TEXT    NOT NULL,            -- UTC calendar day
  object_id    TEXT    NOT NULL DEFAULT '', -- '' for a refused creation; keeps each pruned count per object
  verdict      TEXT    NOT NULL,
  type         TEXT    NOT NULL DEFAULT '', -- '' where the request named an unknown id
  transition   TEXT    NOT NULL DEFAULT '',
  clause       TEXT    NOT NULL DEFAULT '',
  remedy       TEXT    NOT NULL DEFAULT '',
  actor_kind   TEXT    NOT NULL,
  enforced     INTEGER NOT NULL,
  flagged      INTEGER NOT NULL DEFAULT 0,
  declaration_version INTEGER NOT NULL,
  count        INTEGER NOT NULL,
  PRIMARY KEY (day, type, object_id, verdict, transition, clause, remedy, actor_kind, enforced,
               flagged, declaration_version)
);
```

`of_subscription_position` is the one place the model deliberately keeps runtime state outside an object: no version, no events, no history. An acknowledged cursor moving thousands of times a minute is not something anyone wants a permanent record of, and its lag and death are derived rather than states (ADR-0043). `t_subscription` next to it is an ordinary object, because its filter is configuration someone changes and should answer for; who may pull it is the upper layer's (ADR-0114). It holds no endpoint and its position no delivery error: the core posts nothing, and a relay that posts records its own failures (ADR-0105).

**Version 0 is written when the store is created** (ADR-0105), the one write that precedes every operation, attributed to the engine release rather than to an actor, since no one wrote its rules. The same step writes the store's operator, an `Operator` object whose identity is the actor a request names, so the first request has an actor to name (ADR-0126) *(corrected 2026-10-15: it said "with its `of_actor` row", a table the core never made, §6)*. Its creation event, `install`, names the operator's own identity as its actor, of kind `human`, with provenance `observed` and the context `store-creation:<release>`, since an event's actor is required and a release is not one (ADR-0134). Its `of_declaration` row holds the built-in types, the `label` kind, the `closed` category and the standard metric definitions of the engine release, with `builtins` naming that release. The first `DeclarationChange`'s `base_version` references it, so the first flow is drafted and published by the same path as every later one.

`of_declaration` is never deleted. Every event names the version whose rules applied, so deleting one makes that stretch of history unreadable.

**`of_interval` is an index, and says so in its shape.** A creation opens a row for the state and for every tracked member, absent ones included, so an object never assigned has an interval of absence from its creation (ADR-0101). A state change, or a write that changes a tracked member's value, closes the current row for that dimension — a write that leaves the value as it was opens nothing (ADR-0106) — setting `left_position` and `left_at`, and opens a new one in the same statement group that writes the event. Folding the log reproduces it exactly, which the harness checks (ADR-0083). The one kind of row the log cannot rebuild is a `legacy = 1` row, supplied by the import mapping from the legacy system's history. Those are marked so that nothing mistakes them for recorded history.

**`of_attempt` is written outside the request's transaction**, in its own short transaction after the rollback. The exceptions are an observing clause's would-be refusal and a raised flag, each written with the event it is linked to. Either way it holds no personal value — `request_values` holds the request's non-personal inputs and names the personal ones as withheld — so erasure has nothing to do there, and the refusal still replays from the record (ADR-0083, ADR-0105). Its `consulted` holds the metric values and evaluator verdicts the failing clause was decided on, neither of which can be personal, since a metric's value never is (ADR-0084, ADR-0096).

**Pruning rolls up in the same transaction.** A deployment prunes `of_attempt` after its retention period with `maintain(prune_attempts)` (ADR-0100), which refuses a cut-off inside the retention the store was built with (ADR-0101), and the statement that deletes a day's rows adds their counts to `of_attempt_rollup` in the same transaction, so a count is never lost and never counted twice. The metric source `<Type>.attempt_counts` reads the rollup for the days already pruned and counts the retained rows for the rest, so it is complete over the whole history whenever pruning runs, or whether it runs at all. Correctness therefore never waits on it (PRD N2, T3, ADR-0096). The rollup keeps the object, so a pruned day is counted per object exactly as a retained one; a refused creation has no object, and neither has a request that named an unknown id, whose `type` is `''`. **A would-be refusal is kept while its clause is on trial**: the prune skips a row with `enforced = 0` whose clause is still observing in the installed version, so who a trialled rule would have refused stays answerable for the whole trial, and rolls it up at the first prune after the clause is enforced or removed (ADR-0106, PRD UC-14). A raised flag, `flagged = 1`, is not on trial and rolls up at the usual prune: a flag is permanent, and its daily counts are what a deployment reads of it (ADR-0111).

**The sequence table and the attempt log are the two things written outside the request's transaction.** Everything else — the object row, the event, the write index, the interval index, the file reference index, the idempotency record — commits with the transition or not at all.

The sequence is the exception because a decision requires it to be: a monotonic sequence with gaps needs its allocation to survive a rollback, and one that rolls back is gapless and serialises the scope (ADR-0029). It is allocated on a **second connection**, and where that connection points depends on how the backend locks.
- **PostgreSQL locks rows.** The second connection opens the same database and updates the `of_sequence` row, which the request's transaction never touches. An unscoped sequence may be a native `SEQUENCE` instead, whose `nextval` is already non-transactional.
- **SQLite locks the file.** Once the request's connection has begun — and it begins `IMMEDIATE` (§7) — every other connection's write to that file waits out the busy timeout and fails with `database is locked`. So on SQLite `of_sequence` lives in a **separate database file** beside the store, opened on its own connection and created by the same DDL. A write there does not contend with the store's lock, and a crash between the mint and the request's commit leaves a gap, which ADR-0029 already accepts (ADR-0076, D187).

**A SQLite store is therefore two files, and they are backed up and restored together.** A sequence file older than the store would hand out numbers the store's objects already carry: a duplicate identifier where the identifier is not unique, and where it is, a creation refused until the numbers pass them. A sequence file newer than the store only leaves a gap, which ADR-0029 accepts. So a backup copies the store first and the sequence file second, each with SQLite's online backup or with the service stopped, and a restore puts back both files of one backup. Taken in that order, the sequence file is never older than the store it is restored with. On PostgreSQL the sequence is a table or a `SEQUENCE` of the same database, which one backup holds whole.

`scripts/check-schema-doc.py` runs that scenario on every corpus run, in both journal modes: a second connection to the same file blocks behind an open write transaction, one to a separate file does not, and the value it allocated survives the request's rollback.

*(As built, checked 2026-10-21: on both backends `of_sequence` is a table of the store itself, with no second connection and no second file. A request's draws are read and written in its own transaction where it applies, and written after its rollback in a short transaction of their own where it is refused (`store.rs`, `write_sequences`, since `d2327f9`), so a refused request's numbers are not reused, as ADR-0029 and ADR-0122 decide; one writer writing one transaction at a time (ADR-0178, ADR-0213 decision 4) is what lets the store take them without a second connection. A SQLite store is one file and its log, not two files, and a backup copies it alone, a restore skipping the numbers the live store may have drawn after the copy (`operate.md`, Backups and restore; ADR-0207 decision 3). ADR-0076's second connection and its own file on SQLite are not what is built, a departure brought to the author in `TODO.md`; the probe above still holds of SQLite.)*

## 7. Concurrency

The transition transaction runs at **serialisable isolation**, which is what makes a guard's reads safe whether or not the objects it read are ones the transition writes (ADR-0039). What each backend does with a contended request differs, and is stated as observed:

- **SQLite: every transition transaction begins `IMMEDIATE`** (ADR-0090). The write lock is taken at the first statement, so a request's guard reads and its writes are serialised with every other writer's, and a contended request waits out the busy timeout. Under a deferred `BEGIN` in WAL mode, a request that read and then wrote after another committed failed with `database is locked`, which no timeout resolves; `scripts/check-schema-doc.py` keeps both halves as a probe. A busy result past the timeout counts as a serialisation failure: retried within the bound, then `stale`. *As built* (ADR-0178): a deployment's service keeps one write connection behind its lock, each write begun `IMMEDIATE`, the busy timeout `--busy-timeout`, 5 s unless given, and three retries, each event recording the retries it took; every read runs on a read connection of its own in one read transaction, so it sees one committed state and is never held behind a write, and reads under the library of the declaration version that state is at, as a write does inside its transaction. A read without its transaction was observed to see half of a move between two accounts: a pair's derived total read 198 to 204 for a constant 200.
- **PostgreSQL: a contended row waits and then retries** (ADR-0090). Probed on PostgreSQL 16.8: at `SERIALIZABLE`, a request taking `SELECT … FOR UPDATE` on a row another transaction updated waits for it to commit, then fails with `could not serialize access due to concurrent update`. Row locks are taken as objects are reached. They make the loser fail at its first touch of the row rather than after doing its work, and they do not make it queue. The retry is what makes it succeed, and each event records how many it took, so the cost is measured rather than argued (ADR-0083). Deadlock is the database's to detect. *(As built, checked 2026-10-21: the backend ADR-0213 built writes from one instance, one transaction at a time behind the service's writer, as on SQLite (decision 4); a write begins `ISOLATION LEVEL SERIALIZABLE` and a read `REPEATABLE READ`, no statement takes `FOR UPDATE`, and no two of its writes contend. The probe stands as PostgreSQL's behaviour for the several writing instances decision 4 leaves to later work.)*

What the schema owes that:

- **`of_event.position` is monotonic and is not a promise of commit order.** A `BIGSERIAL` allocates out of order under concurrency, so a lower position can become visible after a higher one. On PostgreSQL, assigning positions from a single counter row inside the transaction would serialise every commit through one row, which ADR-0034 rejected by name. On SQLite, where writers are serial anyway, the counter row costs nothing, and is what the store uses. *(As built, checked 2026-10-21: on PostgreSQL too, its writes serial from one instance (ADR-0213 decision 4), so the counter row costs nothing there either; ADR-0034's objection returns with several writing instances.)*
- **The log is read by a settled cursor** (ADR-0089). A reader takes its snapshot's `xmin` and returns only events whose `txn` is below it — every one of them finished — in `(txn, position)` order, handing back the last pair as the cursor. Probed on PostgreSQL 16: while a transaction holding position 1 was in flight and one holding position 2 had committed, the highest visible position was 2, and a reader acknowledging it would never have seen 1. The query below `xmin` returned nothing until both were finished, then both in order. `pull` and `export` return that cursor, and `acknowledge` stores it. On SQLite, where writers are serial, `txn` order is commit order and the cursor is the position. *(As built, checked 2026-10-21: on PostgreSQL too the cursor is the position, its writers serial (ADR-0213 decision 4), so `txn` order is commit order there as well (`subscription.rs`); no reader takes `xmin`. The settled read above is for several writing instances.)*
- **Per-object order survives the cursor's order.** Under serialisable isolation a transaction cannot write an object another committed after its snapshot, which the hot-row probe shows, so a later writer of an object has the higher `txn`. Reasoned from that probe; the harness keeps it tested.
- **The idempotency row is written in the transition's transaction**, so a replay of a request that committed returns the recorded verdict, and a replay of one that did not is a fresh attempt.
- **Nothing that a guard reads is written outside the transaction** — the write index, the interval index, the file reference index — or a crash between them would leave a guard reading a stale answer.

## 8. Which invariants become constraints

Enforcement is dynamic and correctness comes from serialisable isolation; a constraint is an optimisation, and publishing reports which ones the configured backend can take (ADR-0041).

| Invariant shape | PostgreSQL | SQLite |
|---|---|---|
| `unique` on one attribute | `UNIQUE` | `UNIQUE` |
| `unique in scope`, `unique with` | `UNIQUE` over the columns | same |
| `unique where <expr>` | partial `UNIQUE` index | partial `UNIQUE` index |
| a local invariant over columns of one row | `CHECK` | `CHECK` |
| non-negativity of a counter | `CHECK` | `CHECK` |
| overlap of two ranges, per key | **none** — dynamic only (ADR-0213 decision 7) | **none** — dynamic only |
| a traversal or type-scan invariant | none | none |
| any invariant that an assertion may `admit`, or of which an admission stands, whatever recorded it: a publish's `admit` or an import's `ADMIT`; and every invariant of a mirror, which the import may admit (ADR-0147) | **none, on either** | see below |
| any invariant that reads a personal attribute, which an erasure admits (`DESIGN.md` §5.6, ADR-0060) | **none, on either** | none |

*(As built, checked 2026-10-21: no backend compiles an invariant into a constraint. A uniqueness key is a plain index, not `UNIQUE`, since an admitted conflict, an import's `ADMIT` or a mirror's invariant would be refused by one (ADR-0165 decision 1, which this table predates); no `CHECK` is compiled from a local invariant or a counter's non-negativity, the only `CHECK`s being a state's, an enumeration's and an event's source; and a publish reports each invariant's form, `invariant_forms`, not a constraint. Every row is dynamic only, on both backends, which ADR-0041 allows, a constraint being an optimisation; the table stands as which shapes could compile, were compilation built.)*

The row on admissions is **ADR-0074**, derived here because this is the first document that had to hold ADR-0041 and ADR-0054 at once. An admission suppresses **one** invariant for **one** object (ADR-0054), and a database constraint cannot yield for one row. So an invariant a type's assertion names in `may admit` must not be compiled, or an admitted violation would be refused by the database after the runtime allowed it. Publishing knows both facts and can decide it; this is the schema's constraint on that decision. *(Added 2026-09-28, D457: an erasure admits every invariant reading an attribute it erased, for the object it erases, so an invariant that reads a personal attribute is not compiled either, or the database would refuse an erasure the engine allows.)*

The overlap row is the one most asked about. No backend compiles a constraint for it (ADR-0213 decision 7): a booking-overlap invariant is a type scan, which a publish's report classes `type_scan`, and the engine enforces it the same way on both backends, inside the writing transaction at serialisable isolation (ADR-0041), the database refusing nothing. The core's test `an_invariant_scans_every_other_object` refuses an overlapping booking, naming the one it overlaps, one request after another. Sixteen bookings of one room for windows that each overlap every other, sent at once, take one and are refused fifteen times `invariant_violated`, naming the one taken, with no `5xx`, and sixteen for windows apart, sent at once, are each taken, on both backends (`overlapping_bookings_sent_at_once_take_one` and its `_on_postgresql` twin, 2026-10-21, issue #29). Within one service no two writes run at once, its writer holding the store, so each booking's scan sees every booking taken before it. An exclusion constraint on PostgreSQL would be an optimisation under ADR-0041, worth building only where a type scan's contention is measured to need it, and none is planned. *(Corrected 2026-10-20, issue #29: this paragraph said a booking-overlap invariant compiles to an exclusion constraint on PostgreSQL, enforced by the database on one backend and the runtime on the other. ADR-0213 decision 7 decided against it and the core never built one.)*

## 9. Erasure, and what archival tiering means

Erasure (§8 of the model, ADR-0087) does these things to storage:

1. Sets each declared `personal` column to `NULL` on the object row, and on every member of its **supersession chain**, predecessors and successors, each through its own type's `erase`. So does every value personal under the version that wrote it, under any version the object's history was written under (ADR-0146 decision 2): a current value a later version no longer marks personal, judged by the event that last wrote it, its name taken back through later renames; and every column a version kept for history, a dropped attribute's under its own name and a type change's `<name>__v<n>`, with its side table, where the attribute was personal under that version.
2. Rewrites the same values inside every past event's `payload`, in place, keeping the event, its position and its shape: its inputs and what it wrote, every event before the erasure's own, whose events write what it left (ADR-0209 decision 2). `reads` needs nothing, since it holds no value.
3. Walks each of those events up its `cause_position` chain, and redacts in each **caller's event** the inputs that flowed into the erased objects' personal attributes. Check 10's taint analysis identifies which, per declaration version. The erased object's own events lose, besides their personal inputs, each input they passed into another object's personal attribute, a create's or a call's, and the walk does not follow it, the value it reached being that object's to erase (ADR-0210 decision 4).
4. *Replaced by ADR-0211 decision 1: the core keeps no file table and deletes no file's content, the bytes being the application's, which asks `referenced` which hashes the store still holds and deletes the rest; steps 2, 3, 7, 11 and 12 redact every copy of a personal file's value, so a hash only the erased held is held by nothing after them. As ADR-0087 designed it, and never built:* Marks every `of_file_ref` row it reaches as erased. After commit it deletes the content of any hash whose references are now **all** erased, setting `of_file.erased_at`. A deletion that fails sets `erase_pending`. Every later erasure request retries the pending deletions before its own, and `diagnostics` lists them under the type of each object whose `of_file_ref` rows the erasure reached, so a deployment that sees one re-requests an erasure, which is idempotent: it records an `erased` event naming what it retried, and writes nothing else (ADR-0146); no background process is needed (ADR-0096, PRD N2). A file another object still holds survives, because its reference is not erased (D207). **The blob store's own lifecycle expiry must be off** (ADR-0017): the log is permanent and a reference outlives any expiry policy, so a bucket rule that deletes after ninety days silently breaks history.
5. Records an event with source `erased`, carrying the names of the attributes erased, and, as ADR-0087 designed and the core never built, the files deleted (step 4), and **never their values**, and an admission for every invariant that read what it erased (ADR-0060).
6. Redacts every legacy entry attached to the object. Each payload field the import mapping did not list as **kept** for that entry kind, as the entry's `kept` records it, is replaced by absence, and a kind with no list loses its whole payload. The event of step 5 records how many (ADR-0078, ADR-0163).
7. Redacts `t_proposal.inputs` on every proposal targeting an erased object, or whose inputs flow into an erased object's personal attributes, and invalidates the pending ones (ADR-0044, ADR-0078, ADR-0087).
8. Redacts personal **inputs** as well as personal attribute values in step 2 — an input marked `personal`, or one that flows into a personal attribute — so a value passed only to an evaluator does not survive in a payload (ADR-0078).
9. Sets `value` to `NULL` in every `of_interval` row of an erased object's personal enum attributes, as step 2 does in the events the index is rebuilt from, so a rebuilt index and the redacted one agree (ADR-0096). Each row is judged by the version it was written under, and a set-aside dimension, `<name>__v<n>`, by version `n`.
10. Deletes the `of_external_id` rows of the erased objects' personal external identifiers, so `lookup` no longer resolves them (ADR-0100).
11. Redacts each affected proposal's inputs in the proposal's own creation event, as step 7 does in its record (ADR-0100).
12. Follows each erased object's events down their caused events, and erases what flowed from it into other objects' personal attributes, in those events and on those objects, by check 10's taint analysis (ADR-0100), the caused event's write of the copy among them (ADR-0209 decision 2). Each object whose value it redacts gets an event with source `erased`, caused by the erasure's event and naming the attributes and never their values, and its `version` advances, so a subscriber sees the change and an old `expected_version` is refused (ADR-0105).
13. Runs `forget` on every observation in the erased objects' collections, corrected ones included (ADR-0096, ADR-0100).
14. Redacts the `note` of every label on an erased object, in its column and in the label's own creation event, since unclassified free text is treated as personal (ADR-0082), and records a `redact` event on the label, caused by the erasure, as on every object whose value an erasure redacts (`DESIGN.md` §8). 
15. Replaces an erased actor's identity, where its type's identity attribute is personal, by the actor's object's id wherever the store keeps it: `of_event`'s `actor_id` and `actor_principal`, `of_attempt`'s, `of_interval`'s `entered_by`, `of_idempotency`'s `actor_id`, and a proposal's `proposer` and a change's `drafted_by` and `approved_by`, the request's own rows among them, each by its index, which a store opened without them is given (ADR-0192 decision 4); and what each event wrote of it, an identity attribute's value, an identity set's elements and a proposal's inputs as their text (ADR-0209 decision 2).
It does **not** touch `of_attribute_write`, `of_attempt`, or `of_interval` beyond step 9:
- redacting a value inside an event does not change which event last wrote the attribute, so `changed_since` answers the same after an erasure as before;
- a tracked reference holds an id, and a personal enum's values are redacted by step 9;
- an attempt holds no personal input, nor one its transition does not declare, naming each as withheld (ADR-0105); the identity a refused request named for an actor the store does not hold is kept as it was sent, until the attempt retention prunes it (ADR-0125), a limit `DESIGN.md` §8 names.

The log is never pruned. **Archival tiering** is therefore not deletion but a second table with the same shape on cheaper storage, plus a view over both: events older than a threshold move, by `maintain(archive_events)` (ADR-0100), and `pull`, `export` and `history` read the view.

**Erasure reaches the archive.** The catalogue of edge cases offered the alternative — archive only events that carry no personal attribute — and it is rejected here. It requires knowing, at archive time, which of a future declaration's attributes will be personal. `personal` can be added to an attribute by a later publish, at which point events already archived under the old answer are in the wrong place. Reaching the archive costs a slower erasure, an operation measured in minutes and performed rarely. Choosing the other way costs correctness on a schedule nobody controls.

The consequence for the archive's storage is the real cost: it must support update, not only append, so a write-once object store is not enough on its own.

## 10. What publishing does

A publish is a declaration version and a set of DDL statements derived from it, applied in the same transaction as the `of_declaration` row. It is the approval of a `DeclarationChange`, whose id the row records (ADR-0085). **This section owns the DDL mapping only.** What publishing checks, what it reports and when it refuses is [`publish-and-import.md`](publish-and-import.md), which cites this table for the emission step.

| Declaration change | DDL |
|---|---|
| a new type, or a new observation kind | `CREATE TABLE`, its indexes, its constraints |
| a new attribute | `ADD COLUMN`, nullable. DDL never writes a live row: a `default` applies at creation, and a new required attribute is filled by a `backfill` mapping, applied as recorded migration transitions (ADR-0099), its `NOT NULL` applied after the mappings, by a table rebuild on SQLite; an attribute made required is treated the same way (ADR-0147) |
| a new indexed attribute, or one made indexed | `ADD COLUMN` then `CREATE INDEX`, `<table>_<column>_ix`, unless a reference's or a single-attribute key's index serves it; opening a store makes any such index it lacks, as a store a core before D593 made does, since an index changes no answer |
| a removed attribute | **nothing**, but that a required one's `NOT NULL` is dropped, by a table rebuild on SQLite. A dropped attribute hides; its recorded values stay in history (ADR-0027, ADR-0148) |
| an attribute made optional | its `NOT NULL` dropped, by a table rebuild on SQLite (ADR-0148) |
| an attribute whose type changed | before the mappings, its column renamed aside, to `<name>__v<version>` of the last version that declared it, keeping its values readable for history, and a new column of the name added, nullable, every object's value starting absent (ADR-0148), each object that held a value given a `migrate` event naming the change (the audit's write path A-06); a tracked attribute's intervals set aside under the same name, the open one closed at the publish event (ADR-0155, D528) |
| a renamed attribute | `RENAME COLUMN`, carrying the mapping; its intervals and its write-index rows carried under the new name, so `changed_since` and an erasure read who last wrote it (D530) |
| a removed state | no DDL; the `CHECK` is widened to the union of both versions' states before the mappings, which move every object out of it, a mirror's included, and replaced by the new version's after (ADR-0147) |
| a removed enum member | the `CHECK` is widened to the union of both versions' members before the mappings, and replaced after a `removed member` mapping has moved every row off it by recorded migration transitions (ADR-0099, ADR-0147); a retired type's table keeps its `CHECK` |
| a removed type or observation kind | **nothing.** It is retired: no creation and no transition but its erasure, so the personal data its objects hold can still be erased; its table and history stay readable, after a restart as before, the store rebuilding its retired types from the versions that declared them (ADR-0099; the audit's personal data S14) |
| a new invariant of compilable shape | `ADD CONSTRAINT`, after the mappings, and only where no `admit` covers it, since a constraint cannot yield for one row (§8) |
| a removed or changed compiled invariant | its constraint dropped before the mappings, and a changed one's added after them where no `admit` covers it (ADR-0148) |
| an `admit` mapping | an `of_admission` row per violating object, on its migration event; where the admitted invariant is compiled, its constraint is dropped first, in the same transaction, and stays dropped while any admission of it stands (§8, ADR-0074, ADR-0105) |
| a new tracked member | no DDL; an `of_interval` row per object holding its current value, opened at the event that first gives it a value in the publish, the object's migration where a backfill writes it and else the publish event, marked `migrated`, which a fold reproduces (ADR-0106, ADR-0147) |

**A migration reaches every row the change affects, terminal ones included, and no observation's** (ADR-0101): it is the publish's recorded change, not a transition the object takes, and an observation is born final, so a publish removing an enum member an observation holds is refused. **A request or an import never writes a personal column of an object whose erasure is recorded**, so neither a refresh of a mirror nor an externally owned type's sync can restore an erased value (ADR-0101, ADR-0105).

A removed attribute leaving its column in place is deliberate: the column is how history stays readable, and dropping it would make a fold of the log fail on an event the store still holds.

**On SQLite, the DDL around the mappings is a table rebuild before them and another after**, since `ALTER TABLE` cannot change a constraint: each made with foreign keys off, as SQLite's documented procedure requires, and every key checked by `PRAGMA foreign_key_check` before the transaction commits, a dangling one failing the transaction, so the publish installs nothing. With foreign keys on, rebuilding a table another references fails, deferred or not (observed 2026-10-05). The core's rebuild before the mappings drops the state and enumeration `CHECK`s for the mappings' span rather than widening them to the union, which nothing outside the transaction can tell apart (ADR-0155).

## 11. Decided since the first draft, and still open

**Decided.** Each followed from a decision the record already carried, or from what the design made unavoidable.

- **The directory stays.** ADR-0018 chose an opaque id and added that a rendering may carry a type prefix but must never be parsed back into meaning; encoding the type in the id is exactly that parsing.
- **Partitioning is by position range, and the boundary is the archival boundary.** Position is monotonic, so a range partition never moves a row for reordering, and the partition that ages out is the unit that tiers.
- **Erasure reaches the archive**, rather than the archive holding only events with no personal attribute; §9 gives the reasoning.
- **An idempotency key is scoped to the actor's `id`**, the one member of the descriptor that is never absent; a key reused with a different request digest is refused as `KeyReused` (ADR-0077).
- **The object id is a UUIDv7**, and the store takes its id source, clock and connection source as injected dependencies (ADR-0077, ADR-0091).
- **A stored end's foreign key is deferred to commit** on both backends, so two rows that require each other can be written in one transaction (ADR-0077).
- **The sequence store is a second connection, and on SQLite a separate file, drawn from a pool of its own** (ADR-0076, ADR-0090).
- **SQLite transactions begin `IMMEDIATE`**, and a contended row on PostgreSQL waits and then retries; both observed (ADR-0090).
- **An event row is inserted once, complete**, and the log is read by a settled cursor over `(txn, position)` (ADR-0089).
- **Part positions are kept per relationship** in `of_attribute_write`, and `last_part_event` is gone (ADR-0082, D202).
- **Erasure deletes a file's content only when its last unerased reference goes** (ADR-0087, D207).

**Still open.**

- **The size of a partition**, the retention window for an idempotency record, and the attempt log's retention; `maintain` applies both retentions (ADR-0101, ADR-0103). These are numbers to measure against a real event rate and a real retry chain, not designs.
- **Whether one table per type survives many types.** A hundred types is a hundred tables, and the first consumer has perhaps thirty. The case to check is a family with many members: an issue tracker with a type per project reaches hundreds. This is the measurement that could send the whole arrangement back to a shared table.
- **No exclusion constraint is built** for an overlap invariant (§8, ADR-0213 decision 7); whether one would pay is a measurement of a type scan's contention, to take before building it. *(Corrected 2026-10-20, issue #29: this said the exclusion constraint of §8 was unexecuted, as was the PostgreSQL column apart from §7's two probed behaviours. The rest of the PostgreSQL column was checked line by line against the backend ADR-0213 built on 2026-10-21, a correction beside each passage it found wrong: the preamble, §2, §3.1, §6, §7 and §8.)*

## 12. Installing an engine release

*Added 2026-09-30, specifying the step ADR-0110 §4 left to `TODO.md` (D382).* A release is installed over a store by one explicit step, `upgrade`, which a core of the new release runs, as a core creates a store: an operation of the service, not a request on a type, since no flow declares it.

**What the step does**, in one serialisable transaction:
1. It reads the last row of `of_engine_release`. At the new release already, it does nothing and says so, so a repeated upgrade is harmless. At a release the new one does not name as one it upgrades from, it refuses, naming both, and changes nothing: each release names the releases it upgrades from, and a store further behind is upgraded through the releases between, one step each.
2. It runs the new release's migration of the store's own tables, the `of_` tables of §6 and the tables §3 makes of each type, which is that release's own work and all this step writes besides its record. Both backends take a schema change inside a transaction, so a migration that fails leaves the store whole, at the old release.
3. Where the new release's built-in module, its built-in types and standard metrics, differs from the one the installed version was published with (`of_declaration.builtins`), it checks the installed flows against the new one. Where they pass, it records them as the next version, with the installed version's source, reference and digest and the new built-ins, naming the release as its cause in `release` — over a store still at version 0, version 1 with no source, reference or digest, which only an upgrade's version may lack beside version 0 (ADR-0148) — and supersedes every open `DeclarationChange`, as a publish does; where they do not, it refuses, naming the findings, and the deployment changes its flows under the old release first. **`upgrade --dry-run`** runs these checks and writes nothing, producing an impact report of what the new built-ins change, a standard metric's definition or a built-in type's, of which installed flows fail, and of the open changes it would supersede; running the step is the operator's recorded act on that report, so the built-ins change only through a reported, recorded act (ADR-0105, ADR-0147).
4. It appends a row to `of_engine_release`: the new release, when, and `installed_by`, the operator who asked, recorded as that operator's act naming the release, as creating the store is (ADR-0126). Before that act takes a position, a store marked as a backup's copy that holds `of_restore` is restored, as a start restores one (§6, ADR-0207 decision 7).

**Draining the old release.** Every request reads the release row in its transaction (§7), and the step writes it, so under serialisable isolation no request of the old release commits after the upgrade: one that read the row before the upgrade committed fails its serialisation, on either backend, and, retried, reads the new release and refuses with `EngineMismatch` (`library-api.md` §7). A deployment therefore upgrades in three moves:
1. it starts copies of the new release, which refuse with `EngineMismatch` while the store is at the old one, and so take no traffic;
2. it runs `upgrade` once, from one of them;
3. it stops the copies of the old release, which from that commit refuse every request.

No timing of a load balancer can then let two releases both write. A copy of a release older than the store's refuses as well, so there is no downgrade: a release that fails in service is replaced by restoring the backup taken before the step, both files of it on SQLite (§6).

**As built** (ADR-0177, 2026-10-10). The releases are a registry in the core, `objectflow-core::release`, each naming the releases it upgrades from, its migration, what it changes and a fingerprint of the store's own schema and built-ins, which a test holds, so a change to either is a new release; the workspace version names the release. A store further behind passes through each release between, one transaction each, and a dry run runs every step and rolls it back. A copy started on a store an earlier release made serves only `POST /v1/upgrade`, as the operator's act, recorded as an `upgrade` event on the operator beside the release row, and every other operation is refused `engine_mismatch`; every operation reads the release, a read, a request, a check, an attempt's row and a maintenance task each. Release 0.2.0 is the first with the table: its step from 0.1.0 creates it, recording 0.1.0 when the store's operator was installed, and 0.1.0 reads no release, so its copies are stopped before an upgrade from it. **Step 3 is not built**: no release yet changes the built-ins, and a step that says it does is refused until it is. Stores release 0.1.0 made are kept as fixtures with what it answered of them, and an upgraded store must answer the same.

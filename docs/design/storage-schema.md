# Storage schema

Draft, 2026-09-09. How a published declaration becomes tables, and how the guarantees of [`../DESIGN.md`](../DESIGN.md) rest on them. The model owns what a rule means and [`declaration-syntax.md`](declaration-syntax.md) owns how it is written; this document owns only where the bytes go.

**What is verified.** Every statement of SQLite DDL below was executed against SQLite 3.37 before being written down, including a cascade round trip and a refused state value. The PostgreSQL column is reasoned from its documentation and **was not executed** — there is no PostgreSQL in the environment this was written in, and the exclusion constraints of §8 are the part most worth running before anyone relies on them.

## 1. Three layers, and why not fewer

| Layer | Holds | Why it is separate |
|---|---|---|
| **the directory**, `ok_object` | id, type, creation time | `get(id)` and `referrers` need to resolve an opaque id to a type without knowing it in advance (ADR-0018) |
| **one table per declared type**, `t_<type>` | state, version, every stored attribute and every stored relationship end | a constraint can only span columns of one table, and §8 compiles invariants into constraints |
| **the log**, `ok_event` | every recorded change, permanently | it is the history, and a fold of it must reproduce the row (ADR-0033) |

The obvious alternative is one shared object table with the attributes in a JSON column. It was rejected on §8: a uniqueness constraint over two attributes, or an exclusion constraint over a range, needs real typed columns, and without them every invariant falls back to the dynamic check and the declaration's promise that some compile is empty.

The second alternative is to put state and version in the directory rather than in the type table. Also rejected on §8, and this one is easy to get wrong: "no two **live** bookings of one resource overlap" is one constraint over `state`, `resource_id`, `start` and `end`, and it cannot be written if `state` lives in a different table from the rest. So the directory holds nothing a constraint might need, and everything else is in the type table.

## 2. The directory and the log

```sql
-- The directory: the only table that knows every object exists.
CREATE TABLE ok_object (
  id          TEXT    PRIMARY KEY,
  type        TEXT    NOT NULL,
  created_at  TEXT    NOT NULL
);

-- The log. Append-only except for erasure, which rewrites payloads in place.
CREATE TABLE ok_event (
  position            INTEGER PRIMARY KEY AUTOINCREMENT,
  object_id           TEXT    NOT NULL REFERENCES ok_object(id),
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
  cause_position      INTEGER REFERENCES ok_event(position),
  declaration_version INTEGER NOT NULL,
  taint_version       INTEGER NOT NULL,
  recorded_at         TEXT    NOT NULL,
  payload             TEXT    NOT NULL,
  CONSTRAINT ok_event_per_object UNIQUE (object_id, object_seq),
  CONSTRAINT ok_event_source CHECK (source IN
    ('observed','asserted','migrated','corrected','erased'))
);
CREATE INDEX ok_event_by_object ON ok_event (object_id, object_seq);
CREATE INDEX ok_event_by_cause  ON ok_event (cause_position);

-- changed_since, half one: the event that last wrote each attribute.
CREATE TABLE ok_attribute_write (
  object_id  TEXT    NOT NULL REFERENCES ok_object(id),
  attribute  TEXT    NOT NULL,
  position   INTEGER NOT NULL REFERENCES ok_event(position),
  PRIMARY KEY (object_id, attribute)
);
```

`position` is both the event's **identity** and its **order**. An `event`-typed attribute stores it, `cause_position` points at it, the write index compares it and a cursor pages by it; one column serves all four because positions are assigned once and never renumbered. Nothing else in the schema needs an event id.

`position` is the global order and `object_seq` the per-object one; the unique constraint on the pair is what makes "strictly ordered per object" a property of the schema rather than of the writer. `cause_position` is the parent event of a cascade, so causal order across objects is reconstructable without a separate table (§7 of the model).

**The log is append-only with exactly one exception**, and the exception is why `payload` is a column rather than an immutable blob: erasure rewrites the personal values inside past events, in place, keeping the event, its shape and its position (§8 of the model). Nothing else ever updates a row of `ok_event`, and nothing ever deletes one.

## 3. A type becomes a table

Every declared type gets one table. Its identity columns are the same in every one:

| Column | From | Note |
|---|---|---|
| `id` | the store | primary key, and a foreign key into the directory |
| `state` | the machine | with a `CHECK` over the declared state set, so an unreachable value is refused by the database |
| `version` | the store | incremented on every recorded change; what `expected_version` compares against |
| `type_version` | the declaration | which version's rules the row was last written under (ADR-0027) |
| `last_event` | the store | the event that last wrote this object |
| `last_part_event` | the store | present only on a type that declares a `part`; §5 |
| `superseded_by` | the store | present only on a type with a `superseding` state; `get(id, follow)` walks it (ADR-0028) |
| `state_source` | the store | the `source` of the event that last set `state`, indexed. `exceptions(type)` reads it |

**Absence is SQL `NULL` and never a sentinel** (ADR-0051). That is not a stylistic choice: the erasure marker must collide with no uniqueness constraint and must read as unknown, and a sentinel does neither. It follows that every uniqueness over a `personal` or `external` attribute is **partial**, ignoring absent values.

A declared-required attribute takes `NOT NULL`. The database is not the backstop — a guard is (ADR-0001) — and check 8 is what actually enforces requiredness at publish. `NOT NULL` is the same second line of defence a compiled constraint is in §8: redundant when everything works, and the thing that catches a runtime with a bug in it.

Then one column per stored attribute, one per counter, and one per **stored** relationship end. Nothing for a derived inverse, a derivation that is not indexed, or a `part`/`ref` whose other end stores the value — those are queries, and putting a column there would be the second write path the model refuses (ADR-0056 §1).

### 3.1 What each declared type becomes

| Declared | PostgreSQL | SQLite | Why |
|---|---|---|---|
| `string` | `TEXT` | `TEXT` | |
| `bool` | `BOOLEAN` | `INTEGER` 0/1 | |
| `int` | `BIGINT` | `INTEGER` | |
| `decimal(p,s)` | `NUMERIC(p,s)` | `INTEGER`, scaled by 10^s | SQLite's numeric affinity stores a decimal as a float, which is not exact; the scale is in the declaration, so the scaling is recoverable |
| `money(ccy)` | `BIGINT`, minor units | `INTEGER`, minor units | exact, orderable and indexable on both. The currency is fixed by the declaration (ADR-0068) so it is not stored per row |
| `timestamp` | `TIMESTAMPTZ` | `TEXT`, ISO-8601 UTC | |
| `duration` | `BIGINT` seconds | `INTEGER` seconds | the unit set is closed and none is calendar-dependent (ADR-0061) |
| `identity` | `TEXT` | `TEXT` | |
| an enum | `TEXT` + `CHECK` | `TEXT` + `CHECK` | a native enum type would need a migration to add a member; a check constraint is replaced with the declaration |
| `event` | `BIGINT` → `ok_event.position` | `INTEGER` | which is what makes `changed_since([…], a.at_event)` a comparison of two integers |
| `file` | `TEXT` → `ok_file.hash` | `TEXT` | content-addressed; the store holds the reference (ADR-0017) |
| `<T>[]` | a side table | a side table | §3.3 |
| `ref`/`owner`, singular and stored | column + foreign key + index | same | the index is not optional; §5 |
| `ref`/`part`, the derived end | **nothing** | **nothing** | it is a query against the other end's index |
| `counter` | `BIGINT NOT NULL DEFAULT 0` | `INTEGER NOT NULL DEFAULT 0` | non-negativity is an invariant the type declares, not a property of the column (ADR-0072) |
| `derive`, not indexed | **nothing** | **nothing** | evaluated on read |
| `derive`, indexed | generated column + index | generated column + index | §3.4 |

### 3.2 Worked, from the specification's own types

```sql
-- type Robot version 3, binding the UnitLifecycle machine
CREATE TABLE t_robot (
  id                   TEXT    PRIMARY KEY REFERENCES ok_object(id),
  state                TEXT    NOT NULL,
  version              INTEGER NOT NULL,
  type_version         INTEGER NOT NULL,
  last_event           INTEGER NOT NULL REFERENCES ok_event(position),
  last_part_event      INTEGER,
  state_source         TEXT    NOT NULL DEFAULT 'observed',
  serial               TEXT    NOT NULL,          -- identifier, unique in scope
  manufacturer_serial  TEXT,
  label_printed_at     TEXT,                      -- timestamp
  retirement_reason    TEXT,
  model_id             TEXT    NOT NULL REFERENCES t_robotmodel(id),
  binding_id           TEXT    REFERENCES t_delivery(id),
  CONSTRAINT t_robot_state CHECK (state IN
    ('REQUESTED','PROCUREMENT','INTAKE','AVAILABLE','RESERVED',
     'SOLD','DEVELOPMENT','RETIRED','CANCELLED')),
  CONSTRAINT t_robot_retirement_reason CHECK (retirement_reason IS NULL
     OR retirement_reason IN ('FAILED','DAMAGED','OBSOLETE','LOST')),
  CONSTRAINT t_robot_serial_in_scope UNIQUE (model_id, serial)
);
CREATE INDEX t_robot_state_ix   ON t_robot (state);
CREATE INDEX t_robot_asserted_ix ON t_robot (state_source);  -- exceptions(type)
CREATE INDEX t_robot_model_ix   ON t_robot (model_id);      -- stored end
CREATE INDEX t_robot_binding_ix ON t_robot (binding_id);    -- stored end, and the inverse `units`
```

Three things to read off it. `serial` carries `unique in scope`, and the scope is the `model` reference, so the constraint is over both columns — the declaration said "scoped by model" and the schema says the same thing in the only language the database enforces. `binding_id` is indexed because it is the stored end of `Robot.binding`, and `Delivery.units` is a query against that index; §5 explains why the index is a correctness requirement and not a tuning choice. And `engagement_lines` and `leasable` have no columns at all.

### 3.3 A set-valued attribute is its own table

```sql
CREATE TABLE t_robot__photos (
  object_id  TEXT    NOT NULL REFERENCES t_robot(id),
  ordinal    INTEGER NOT NULL,
  value      TEXT    NOT NULL REFERENCES ok_file(hash),
  PRIMARY KEY (object_id, ordinal)
);
```

An array column would hold the same data. A table is chosen because `count(p in photos)` is then an ordinary indexed count rather than a function over an array, because `add` and `remove` are read-modify-write against rows the transaction already locks, and because a set of `file` values keeps a real foreign key into the content table, which erasure walks.

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

This is the case ADR-0048 allows: a derivation over stored indexed columns of the same object, with no aggregate and no clock. The database computes it and **refuses to let anyone write it** — attempting to produces `cannot UPDATE generated column`, verified. So "derived attributes are never stored, evaluated on read" stops being a discipline the runtime has to keep and becomes something the storage layer enforces on it.

A derivation that reads a clock or another object gets no column, which is why a time-dependent predicate is answered by filtering the stored operand it compares against `now` instead (ADR-0048).

## 4. The event payload

`payload` is the writes and the inputs of one transition, as JSON: attribute name to value, in the types of §3.1. It is JSON rather than columns because its shape is per transition, and because a fold of the log must reproduce a row written under a **declaration version that may no longer exist** — a column set fixed by today's declaration could not hold yesterday's event.

That is also why `declaration_version` is on the event and not inferred: reading history means reading each event under the rules that were in force when it was recorded.

## 5. The indexes the model requires to be correct

Most indexes are performance. These four are not: a stated guarantee is false without them.

| Index | Without it |
|---|---|
| the stored end of every relationship | a derived inverse becomes a table scan, so `Delivery.units` is O(all robots). This index **is** the reverse index `referrers` needs, one per declared reference, covering the ends with no declared `inverse` too — which is the case that made `referrers` necessary. ADR-0056 states the cost plainly: a deployment pays index maintenance on every reference write, and takes it because deletion correctness is load-bearing and deletion is rare |
| `ok_attribute_write (object_id, attribute)` | `changed_since` is a scan of the object's whole history, and it is the most-cited guard in the model (ADR-0035) |
| `t_<type>.last_part_event` | the half of `changed_since` that reaches a composition has nothing to compare, so an approval cannot be invalidated by an edit to a line (ADR-0057) |
| every attribute a type-scan invariant or a `visible when` predicate reads | the invariant's affected set is a full scan on every write, and visibility stops being a query filter and becomes a per-row test, which the read surface cannot page |

The last one is why check 7 rejects a type-scan or a visibility predicate over an unindexed attribute at publish: the schema cannot be built to satisfy it afterwards.

**`exceptions(type)` reads two things and no third.** Admissions come from `ok_admission` where `discharged_position` is null. Asserted state comes from `state_source = 'asserted'` on the type table, indexed. The design record promises the operation and never says what it reads; a column is chosen over scanning the log because the query is per type and the log is the busiest table in the system. It costs one indexed column on every object and it is written by the same statement that writes the state, so it cannot disagree with the event.

An ordinary transition afterwards sets `state_source` back to `observed`, which is right: an object whose state was asserted and has since moved through the machine normally is no longer an exception, and nobody has to remember to clear a flag.

`ok_attribute_write` deserves its shape stated plainly. One row per object per attribute ever written, holding the position of the event that last wrote it. It grows with the object's *width*, not with its history, so it is bounded by the declaration.

## 6. The built-in tables

```sql
-- Named sequences, scoped. Allocated in its OWN transaction on its own
-- connection, committed before the request continues, so the allocation
-- survives a rollback of the request and leaves the gap ADR-0029 requires.
-- Updating this row inside the request's transaction would make sequences
-- gapless, which that decision rejected: it serialises every creation in
-- the scope.
CREATE TABLE ok_sequence (
  name       TEXT    NOT NULL,
  scope_key  TEXT    NOT NULL,
  next_value INTEGER NOT NULL,
  PRIMARY KEY (name, scope_key)
);

-- External identifiers, which lookup(source, value) answers from.
CREATE TABLE ok_external_id (
  source     TEXT NOT NULL,
  value      TEXT NOT NULL,
  object_id  TEXT NOT NULL REFERENCES ok_object(id),
  attribute  TEXT NOT NULL,
  PRIMARY KEY (source, value)
);

-- Idempotency. The recorded verdict is replayed verbatim.
-- Scoped to the principal, not global: a key is unique per caller, which is
-- what the first consumer's own evidence shows and what `unique with` was
-- added to the declaration syntax to express. A global key space would let
-- one caller's retry collide with another's.
CREATE TABLE ok_idempotency (
  principal      TEXT    NOT NULL,
  key            TEXT    NOT NULL,
  request_digest TEXT    NOT NULL,
  first_position INTEGER REFERENCES ok_event(position),
  verdict        TEXT    NOT NULL,
  applied_at     TEXT    NOT NULL,
  PRIMARY KEY (principal, key)
);

-- Admissions: an invariant a transition was permitted to violate.
-- exceptions(type) is a query over this table.
CREATE TABLE ok_admission (
  object_id           TEXT    NOT NULL REFERENCES ok_object(id),
  invariant           TEXT    NOT NULL,
  position            INTEGER NOT NULL REFERENCES ok_event(position),
  reason              TEXT    NOT NULL,
  discharged_position INTEGER REFERENCES ok_event(position),
  PRIMARY KEY (object_id, invariant, position)
);
CREATE INDEX ok_admission_by_object ON ok_admission (object_id);

-- Content-addressed files. The store holds the reference; erasure deletes the
-- content and keeps the row, so a reference resolves to something that says so.
CREATE TABLE ok_file (
  hash        TEXT PRIMARY KEY,
  size_bytes  INTEGER NOT NULL,
  media_type  TEXT    NOT NULL,
  erased_at   TEXT
);

-- The published declaration, one row per version. Never deleted: a recorded
-- event names the version whose rules applied, and history must stay readable.
CREATE TABLE ok_declaration (
  version       INTEGER PRIMARY KEY,
  module        TEXT    NOT NULL,
  source        TEXT    NOT NULL,   -- the .ok text as published
  parsed        TEXT    NOT NULL,   -- the checked form the runtime reads
  published_at  TEXT    NOT NULL,
  published_by  TEXT    NOT NULL,
  report        TEXT    NOT NULL    -- what publishing reported, kept for audit
);

-- Migration mappings carried by a publish that removes a state or renames an attribute.
CREATE TABLE ok_migration (
  version     INTEGER NOT NULL REFERENCES ok_declaration(version),
  type_name   TEXT    NOT NULL,
  kind        TEXT    NOT NULL,     -- 'removed state' | 'renamed attr'
  from_name   TEXT    NOT NULL,
  to_name     TEXT    NOT NULL,
  PRIMARY KEY (version, type_name, kind, from_name)
);

-- Subscription is a built-in object. Its progress is not (ADR-0043).
CREATE TABLE t_subscription (
  id             TEXT    PRIMARY KEY REFERENCES ok_object(id),
  state          TEXT    NOT NULL,
  version        INTEGER NOT NULL,
  type_version   INTEGER NOT NULL,
  last_event     INTEGER NOT NULL REFERENCES ok_event(position),
  target_type    TEXT    NOT NULL,
  target_family  INTEGER NOT NULL DEFAULT 0,
  transitions    TEXT,                       -- null means every transition
  changes_state_only INTEGER NOT NULL DEFAULT 0,
  endpoint       TEXT,                       -- null for a pull subscription
  lag_threshold  INTEGER,
  CONSTRAINT t_subscription_state CHECK (state IN ('active','revoked'))
);

-- Runtime state, deliberately not an object: no version, no events, no history.
CREATE TABLE ok_subscription_position (
  subscription_id  TEXT    PRIMARY KEY REFERENCES t_subscription(id),
  acknowledged     INTEGER NOT NULL,
  acknowledged_at  TEXT    NOT NULL,
  last_error       TEXT,
  last_error_at    TEXT
);

-- Proposal is a built-in object type.
CREATE TABLE t_proposal (
  id             TEXT    PRIMARY KEY REFERENCES ok_object(id),
  state          TEXT    NOT NULL,
  version        INTEGER NOT NULL,
  type_version   INTEGER NOT NULL,
  last_event     INTEGER NOT NULL REFERENCES ok_event(position),
  target_id      TEXT    NOT NULL REFERENCES ok_object(id),
  transition     TEXT    NOT NULL,
  inputs         TEXT    NOT NULL,
  proposer       TEXT    NOT NULL,
  submitted_under INTEGER NOT NULL REFERENCES ok_declaration(version),
  last_verdict   TEXT,          -- the verdict that left it pending (ADR-0044)
  expires_at     TEXT,          -- expiry is derived from this, never a state
  CONSTRAINT t_proposal_state CHECK (state IN
    ('pending','executed','rejected','withdrawn','invalidated'))
);
CREATE INDEX t_proposal_target_ix ON t_proposal (target_id, state);

-- Imported history that predates the store (ADR-0015): read-only, not events.
CREATE TABLE ok_legacy_entry (
  object_id   TEXT    NOT NULL REFERENCES ok_object(id),
  ordinal     INTEGER NOT NULL,
  occurred_at TEXT    NOT NULL,
  payload     TEXT    NOT NULL,
  PRIMARY KEY (object_id, ordinal)
);
```

`ok_subscription_position` is the one place the model deliberately keeps runtime state outside an object: no version, no events, no history, because an acknowledged cursor moving thousands of times a minute is not something anyone wants a permanent record of, and its lag and death are derived rather than states (ADR-0043). `t_subscription` next to it is an ordinary object because its filter and endpoint are configuration someone changes and should answer for.

`ok_declaration` is never deleted. Every event names the version whose rules applied, so deleting one makes that stretch of history unreadable.

**The sequence table is the one thing here written outside the request's transaction.** Everything else — the object row, the event, the write index, the part-event position, the idempotency record — commits with the transition or not at all. The sequence is the exception because a decision requires it to be: a monotonic sequence with gaps needs its allocation to survive a rollback, and one that rolls back is gapless and serialises the scope (ADR-0029). On PostgreSQL this can be a native `SEQUENCE`, whose `nextval` is already non-transactional; on SQLite it is a second connection.

## 7. Concurrency

The transition transaction runs at **serialisable isolation**, which is what makes a guard's reads safe whether or not the objects it read are ones the transition writes (ADR-0039). Row locks are taken as objects are reached during sequential application, for contention rather than correctness, so a contended row queues rather than aborting and retrying. Deadlock is the database's to detect; the ordering rule an earlier decision imposed was withdrawn as unimplementable, since the lock set is discovered by traversal and not known in advance.

What the schema owes that:

- **`ok_event.position` is monotonic and is not a promise of commit order.** ADR-0034 decided this and the schema implements it rather than reopening it: a `BIGSERIAL` on PostgreSQL allocates out of order under concurrency, so a lower position can become visible after a higher one, and a pull cursor must tolerate a bounded window. Assigning the position from a single counter row inside the transaction would close the window and serialise every commit through one row, which is the alternative that ADR-0034 rejected by name.

  What the schema therefore owes the pull path is **the window's bound**, which is the oldest in-flight transaction's age. A consumer reading "everything after N" must not advance its acknowledged position past `head - window`, and `pull` returns that bound so it does not have to guess. On SQLite, which serialises writers, the window is empty.
- **The idempotency row is written in the transition's transaction**, so a replay of a request that committed returns the recorded verdict and a replay of one that did not is a fresh attempt.
- **Nothing is written outside the transaction**, including the write index and the part-event position, or a crash between them would leave a guard reading a stale answer.

## 8. Which invariants become constraints

Enforcement is dynamic and correctness comes from serialisable isolation; a constraint is an optimisation, and publishing reports which ones the configured backend can take (ADR-0041).

| Invariant shape | PostgreSQL | SQLite |
|---|---|---|
| `unique` on one attribute | `UNIQUE` | `UNIQUE` |
| `unique in scope`, `unique with` | `UNIQUE` over the columns | same |
| `unique where <expr>` | partial `UNIQUE` index | partial `UNIQUE` index |
| a local invariant over columns of one row | `CHECK` | `CHECK` |
| non-negativity of a counter | `CHECK` | `CHECK` |
| overlap of two ranges, per key | `EXCLUDE USING gist` | **none** — dynamic only |
| a traversal or type-scan invariant | none | none |
| any invariant that an assertion may `admit` | **none, on either** | see below |

The last row is **ADR-0074**, derived here because this is the first document that had to hold ADR-0041 and ADR-0054 at once. An admission suppresses **one** invariant for **one** object (ADR-0054) and a database constraint cannot yield for one row. So an invariant a type's assertion names in `may admit` must not be compiled, or an admitted violation would be refused by the database after the runtime allowed it. Publishing knows both facts and can decide it; this is the schema's constraint on that decision.

The overlap row is the one that matters and the one not executed here. A booking-overlap invariant compiles to an exclusion constraint on PostgreSQL and has no equivalent in SQLite, so the same declaration is enforced by the database on one backend and by the runtime on the other. That is legitimate under ADR-0041 and it should be measured before it is believed, because a type-scan under serialisable isolation is where contention will actually appear.

## 9. Erasure, and what archival tiering means

Erasure (§8 of the model) does four things to storage:

1. sets each declared `personal` column to `NULL` on the object row;
2. rewrites the same values inside every past event's `payload`, in place, keeping the event, its position and its shape;
3. sets `ok_file.erased_at` and deletes the content behind the hash, leaving the row so that a reference to it resolves to something that says it was erased rather than to nothing. **The blob store's own lifecycle expiry must be off** (ADR-0017): the log is permanent and a reference outlives any expiry policy, so a bucket rule that deletes after ninety days silently breaks history;
4. records an event with source `erased`, carrying the names of the attributes erased and the files deleted and **never their values**, and an admission for every invariant that read what it erased (ADR-0060).

It does **not** touch `ok_attribute_write`. Redacting a value inside an event does not change which event last wrote the attribute, so `changed_since` answers the same after an erasure as before, and an approval that was valid stays valid. The design record does not say this; it follows from what the index means.

The log is never pruned. **Archival tiering** is therefore not deletion but a second table with the same shape on cheaper storage, plus a view over both: events older than a threshold move, and `pull` and `history` read the view.

**Erasure reaches the archive.** The catalogue of edge cases offered the alternative — archive only events that carry no personal attribute — and it is rejected here. It requires knowing, at archive time, which of a future declaration's attributes will be personal, and `personal` can be added to an attribute by a later publish, at which point events already archived under the old answer are in the wrong place. Reaching the archive costs a slower erasure, which is an operation measured in minutes and performed rarely. Choosing the other way costs correctness on a schedule nobody controls.

The consequence for the archive's storage is the real cost: it must support update, not only append, so a write-once object store is not enough on its own.

## 10. What publishing does

A publish is a declaration version and a set of DDL statements derived from it, applied in the same transaction as the `ok_declaration` row. **This section owns the DDL mapping only.** What publishing checks, what it reports and when it refuses is [`publish-and-import.md`](publish-and-import.md), which cites this table for the emission step.

| Declaration change | DDL |
|---|---|
| a new type | `CREATE TABLE`, its indexes, its constraints |
| a new attribute | `ADD COLUMN`, nullable, or with the declared `default` |
| a new indexed attribute | `ADD COLUMN` then `CREATE INDEX` |
| a removed attribute | **nothing.** A dropped attribute hides; its recorded values stay in history (ADR-0027) |
| a renamed attribute | `RENAME COLUMN`, carrying the mapping |
| a removed state | no DDL; the `CHECK` is replaced, and the mapping moves live objects out of it first |
| a new invariant of compilable shape | `ADD CONSTRAINT`, after the report says how many live objects violate it |

A removed attribute leaving its column in place is deliberate: the column is how history stays readable, and dropping it would make a fold of the log fail on an event the store still holds.

## 11. Decided since the first draft, and still open

**Decided.** Each followed from a decision the record already carried, or from what the design made unavoidable.

- **The directory stays.** ADR-0018 chose an opaque id and added that a rendering may carry a type prefix but must never be parsed back into meaning; encoding the type in the id is exactly that parsing. This was listed as open by a document written from the model that missed a line the model states.
- **Partitioning is by position range, and the boundary is the archival boundary.** Position is monotonic, so a range partition never moves a row for reordering, and the partition that ages out is the unit that tiers — an archive becomes a partition detached and reattached rather than a row-by-row copy.
- **Erasure reaches the archive**, rather than the archive holding only events with no personal attribute; §9 gives the reasoning.
- **An idempotency key is scoped to the principal**, so one caller's retry cannot collide with another's.

**Still open.**

- **The size of a partition**, and the retention window for an idempotency record. Both are numbers to measure against a real event rate and a real retry chain, not designs.
- **Whether one table per type survives many types.** A hundred types is a hundred tables; the first consumer has perhaps thirty. The case to check is a family with many members — an issue tracker with a type per project reaches hundreds. This is the measurement that could send the whole arrangement back to a shared table.
- **The exclusion constraint of §8 is unexecuted**, as is everything in the PostgreSQL column. There is no PostgreSQL in the environment this was written in.

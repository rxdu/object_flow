# Storage schema

Draft, 2026-09-09, amended 2026-09-23. How a published declaration becomes tables, and how the guarantees of [`../DESIGN.md`](../DESIGN.md) rest on them. The model owns what a rule means and [`declaration-syntax.md`](declaration-syntax.md) owns how it is written; this document owns only where the bytes go.

**Amended 2026-09-23** for ADR-0082 to ADR-0094:
- the event gains its writing transaction, read set, occurred time and retry count, and is inserted once, complete, with its position allocated first;
- part positions are kept per relationship;
- the attempt log, the interval index, labels, the file reference index and `DeclarationChange` are new tables;
- SQLite transactions begin `IMMEDIATE`;
- the log is read by a settled cursor.

**What is verified.** `scripts/check-schema-doc.py` executes every statement of SQLite DDL below against SQLite 3.37 on every corpus run. It also keeps three probes of what SQLite does at runtime:
- the two-connection sequence probe of §6, which was reasoned rather than run until D187 found it false as first written;
- the deferred-foreign-key probe of §3, which replaced a premise D191 found false;
- the transaction-mode probe of §7, which is D204's.

The PostgreSQL column is reasoned from its documentation, except two behaviours probed on PostgreSQL 16 in a throwaway container on 2026-09-23 and recorded in ADR-0089 and ADR-0090: a contended row at serialisable isolation waits and then fails, and reading below the snapshot's `xmin` cannot skip a late commit. The exclusion constraints of §8 remain unexecuted, and are the part most worth running before anyone relies on them.

## 1. Three layers, and why not fewer

| Layer | Holds | Why it is separate |
|---|---|---|
| **the directory**, `ok_object` | id, type, creation time | `get(id)` and `referrers` need to resolve an opaque id to a type without knowing it in advance (ADR-0018) |
| **one table per declared type**, `t_<type>` | state, version, every stored attribute and every stored relationship end | a constraint can only span columns of one table, and §8 compiles invariants into constraints |
| **the log**, `ok_event` | every recorded change, permanently | it is the history, and a fold of it must reproduce the row (ADR-0033) |

The obvious alternative is one shared object table with the attributes in a JSON column. It was rejected on §8: a uniqueness constraint over two attributes, or an exclusion constraint over a range, needs real typed columns. Without them every invariant falls back to the dynamic check, and the declaration's promise that some compile is empty.

The second alternative is to put state and version in the directory rather than in the type table. Also rejected on §8, and this one is easy to get wrong. "No two **live** bookings of one resource overlap" is one constraint over `state`, `resource_id`, `start` and `end`, and it cannot be written if `state` lives in a different table from the rest. So the directory holds nothing a constraint might need, and everything else is in the type table.

Beside the three layers sit the **flow data** of ADR-0083: `ok_interval`, an index the log rebuilds, and `ok_attempt`, evidence that is not history. Neither is a fourth layer, since neither is a source of truth; §6 defines both.

## 2. The directory and the log

```sql
-- The directory: the only table that knows every object exists.
CREATE TABLE ok_object (
  id              TEXT    PRIMARY KEY,
  type            TEXT    NOT NULL,
  created_at      TEXT    NOT NULL,
  created_by_kind TEXT    NOT NULL          -- the creating request's actor kind (ADR-0096)
);

-- The log. Each row is inserted once, complete, at the end of its
-- transition; nothing updates it afterwards except erasure's redaction of
-- its payload (ADR-0089).
CREATE TABLE ok_event (
  position            INTEGER PRIMARY KEY,          -- allocated first; see below
  txn                 TEXT    NOT NULL,             -- the writing transaction
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
  occurred_at         TEXT,                         -- a backdatable transition's or a record's (ADR-0099)
  imported            INTEGER NOT NULL DEFAULT 0,   -- written by import_batch (ADR-0100)
  retries             INTEGER NOT NULL DEFAULT 0,   -- serialisation retries its request took
  payload             TEXT    NOT NULL,
  reads               TEXT    NOT NULL,             -- the read set, as JSON (ADR-0088)
  CONSTRAINT ok_event_per_object UNIQUE (object_id, object_seq),
  CONSTRAINT ok_event_source CHECK (source IN
    ('observed','asserted','migrated','corrected','erased'))
);
CREATE INDEX ok_event_by_object ON ok_event (object_id, object_seq);
CREATE INDEX ok_event_by_cause  ON ok_event (cause_position);
CREATE INDEX ok_event_by_cursor ON ok_event (txn, position);

-- SQLite only: positions and transaction numbers, allocated by the
-- transaction that holds the write lock from its first statement (§7).
CREATE TABLE ok_counter (
  name   TEXT    PRIMARY KEY,                         -- 'position', 'txn'
  value  INTEGER NOT NULL
);

-- changed_since: the event that last wrote each attribute, and the last
-- event on each part relationship, one row per name (ADR-0057, ADR-0082).
CREATE TABLE ok_attribute_write (
  object_id  TEXT    NOT NULL REFERENCES ok_object(id),
  attribute  TEXT    NOT NULL,                        -- an attribute or a part relationship
  position   INTEGER NOT NULL REFERENCES ok_event(position),
  PRIMARY KEY (object_id, attribute)
);
```

`position` is both the event's **identity** and its **order within a transaction**. An `event`-typed attribute stores it, `cause_position` points at it, and the write index compares it; one column serves all three because positions are assigned once and never renumbered. It is **not** commit order across transactions, which is why a cursor reads by `(txn, position)` (§7).

**A position is allocated when its transition begins applying**, so `this_event` is known to the outcome (ADR-0046), and the row is inserted when the outcome is done, in the same transaction (ADR-0089).
- **On PostgreSQL** the position comes from the log's sequence by `nextval`, and `txn` is `pg_current_xact_id()`.
- **On SQLite** both come from `ok_counter`, incremented by the request's own transaction. That transaction holds the write lock from its first statement, so no other writer can interleave.

Nothing is inserted early and completed later, so the append-only statement below is true.

`id` is a **UUIDv7**, stored in its canonical text form on both backends; PostgreSQL may use its `uuid` type. It is time-ordered rather than random because ADR-0038 iterates a cascade in ascending id order, and the harness reduces a failing run by replaying its transcript; both need that order to be creation order. The embedded time is never read back as a fact about the object, which is what `created_at` is for. The store takes its id source, its clock and its connection source as injected dependencies, so a test can make all three deterministic (ADR-0077, ADR-0091).

`object_seq` is the per-object order. The unique constraint on `(object_id, object_seq)` is what makes "strictly ordered per object" a property of the schema rather than of the writer. It is also what makes a read set re-evaluable: `reads` records each object a rule read with its version, and a version is a point in that strict order, so no global order is needed (ADR-0088). `cause_position` is the parent event of a cascade, so causal order across objects is reconstructable without a separate table.

**The log is append-only with exactly one exception**, and the exception is why `payload` is a column rather than an immutable blob: erasure rewrites the personal values inside past events, in place, keeping the event, its shape and its position (§9). `reads` holds ids, versions and capability answers, never a value, so erasure has nothing to do there.

## 3. A type becomes a table

Every declared type gets one table. Its identity columns are the same in every one:

| Column | From | Note |
|---|---|---|
| `id` | the store | primary key, and a foreign key into the directory |
| `state` | the machine | with a `CHECK` over the declared state set, so an unreachable value is refused by the database |
| `version` | the store | incremented on every recorded change; what `expected_version` compares against, and what a read set records |
| `declaration_version` | the declaration | the publish whose rules the row was last written under (ADR-0027, ADR-0077); the same number the event carries |
| `last_event` | the store | the event that last wrote this object |
| `superseded_by` | the store | present only on a type with a `superseding` state; `get(id, follow)` walks it (ADR-0028), and erasure follows it (ADR-0087) |
| `state_source` | the store | the `source` of the event that last **changed** `state`: `observed`, `asserted`, `migrated`, or `imported` for the import's state change. Indexed; an action leaves it alone. `exceptions(type)` reads `asserted` (ADR-0083) |

There is no `last_part_event` column. A part relationship's last event is a row of `ok_attribute_write` keyed by the relationship's name, so a guard naming `checklist_items` is not invalidated by a change to `approvals` (ADR-0082, repairing D202).

**Absence is SQL `NULL` and never a sentinel** (ADR-0051). That is not a stylistic choice: the erasure marker must collide with no uniqueness constraint and must read as unknown, and a sentinel does neither. It follows that every uniqueness over a `personal` or `external` attribute is **partial**, ignoring absent values.

A declared-required attribute takes `NOT NULL`. The database is not the backstop — a guard is (ADR-0001) — and check 8 is what actually enforces requiredness at publish. `NOT NULL` is the same second line of defence a compiled constraint is in §8: redundant when everything works, and the thing that catches a runtime with a bug in it.

Then one column per stored attribute, one per counter, and one per **stored** relationship end. There is nothing for a derived inverse, for a derivation that is not indexed, or for a `part` or `ref` whose other end stores the value. Those are queries, and putting a column there would be the second write path the model refuses (ADR-0056 §1).

The foreign key on a stored end is declared **`DEFERRABLE INITIALLY DEFERRED`** on both backends and checked at commit. The runtime resolves every reference itself and the constraint is the backstop, so checking it at commit loses nothing. It is what lets one transaction insert two rows that require each other: an outcome that creates a whole and a part which each require the other, or an import writing a cycle of required references (ADR-0077). SQLite enforces foreign keys only with `PRAGMA foreign_keys = ON`, which the store sets on every connection it opens. `scripts/check-schema-doc.py` demonstrates the deferred case, the same pair failing without the clause, and a dangling reference still failing at commit.

**An observation kind is a type** and gets a table like any other (ADR-0082):
- its fields are columns;
- its `owner` end is `subject_id`;
- it has one state;
- a correction's link to the observation it corrects is a nullable `corrects_id`;
- its occurred time is `occurred_at`, since metrics filter and bucket on it (ADR-0099).

**A mirror type's table has `imported_at`**, the time its last import wrote the row, which a guard may bound (ADR-0100).

A numeric field's declared unit is metadata in `ok_declaration`, not a column, since it never varies by row.

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
| `file` | `TEXT` → `ok_file.hash` | `TEXT` | content-addressed; the store holds the reference (ADR-0017), and every write of one is a row of `ok_file_ref` (§6) |
| `<T>[]` | a side table | a side table | §3.3 |
| `ref`/`owner`, singular and stored | column + foreign key, deferred to commit + index | same | the index is not optional; §5 |
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
  declaration_version  INTEGER NOT NULL,
  last_event           INTEGER NOT NULL REFERENCES ok_event(position),
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
    ('observed','asserted','migrated','imported')),
  CONSTRAINT t_robot_serial_in_scope UNIQUE (model_id, serial)
);
CREATE INDEX t_robot_state_ix   ON t_robot (state);
CREATE INDEX t_robot_asserted_ix ON t_robot (state_source);  -- exceptions(type)
CREATE INDEX t_robot_model_ix   ON t_robot (model_id);      -- stored end
CREATE INDEX t_robot_binding_ix ON t_robot (binding_id);    -- stored end, and the inverse `units`
```

Four things to read off it:
- **`serial` carries `unique in scope`,** and the scope is the `model` reference, so the constraint is over both columns. The declaration said "scoped by model", and the schema says the same thing in the only language the database enforces.
- **`binding_id` is indexed** because it is the stored end of `Robot.binding`, and `Delivery.units` is a query against that index. §5 explains why the index is a correctness requirement, not a tuning choice.
- **`engagement_lines` and `leasable` have no columns** at all.
- **`retirement_reason`, `model_id` and `binding_id` are tracked**, as an enum and two singular references. Their intervals are rows of `ok_interval` (§6), not columns here.

### 3.3 A set-valued attribute is its own table

```sql
CREATE TABLE t_robot__photos (
  object_id  TEXT    NOT NULL REFERENCES t_robot(id),
  ordinal    INTEGER NOT NULL,
  value      TEXT    NOT NULL REFERENCES ok_file(hash),
  PRIMARY KEY (object_id, ordinal)
);
```

An array column would hold the same data. A table is chosen for three reasons:
- `count(p in photos)` is then an ordinary indexed count rather than a function over an array;
- `add` and `remove` are read-modify-write against rows the transaction already locks;
- a set of `file` values keeps a real foreign key into the content table, which erasure walks.

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

A derivation that reads a clock or another object gets no column. That is why a time-dependent predicate is answered by filtering the stored operand it compares against `now` instead (ADR-0048). Where that operand is when a tracked value was entered — `entered_at(unit) + 14 days <= now` — it is `ok_interval.entered_at`, stored and indexed, and `query` joins to it (ADR-0084).

## 4. The event payload

`payload` is the writes and the inputs of one transition, as JSON: attribute name to value, in the types of §3.1. Where a guard consulted an external evaluator or a metric, it also holds the verdict or value it was given and that value's as-of time, keyed by the guard's name (ADR-0049, ADR-0084, ADR-0095).

It is JSON rather than columns for two reasons. Its shape is per transition. And a fold of the log must reproduce a row written under a **declaration version that may no longer exist**; a column set fixed by today's declaration could not hold yesterday's event.

`reads` is the read set of ADR-0088, also JSON:
- the objects the transition's rules read, each with the version read;
- each capability an `actor.has` asked about, with its answer;
- the `now` the request fixed.

It holds no value, only identities, versions and answers. A read filters it for the reader: an object the reader cannot see is left out, and `withheld` is set (ADR-0095).

That is also why `declaration_version` is on the event and not inferred: reading history means reading each event under the rules that were in force when it was recorded.

## 5. The indexes the model requires to be correct

Most indexes are performance. These are not: a stated guarantee is false without them.

| Index | Without it |
|---|---|
| the stored end of every relationship | a derived inverse becomes a table scan, so `Delivery.units` is O(all robots). This index **is** the reverse index `referrers` needs, one per declared reference, covering the ends with no declared `inverse` too — which is the case that made `referrers` necessary. ADR-0056 states the cost plainly: a deployment pays index maintenance on every reference write, and takes it because deletion correctness is load-bearing and deletion is rare |
| `ok_attribute_write (object_id, attribute)` | `changed_since` is a scan of the object's whole history, and it is the most-cited guard in the model (ADR-0035). Keyed by part relationship too, it answers the half of `changed_since` that reaches a composition, per relationship (ADR-0057, ADR-0082) |
| every attribute a type-scan invariant or a `visible when` predicate reads | the invariant's affected set is a full scan on every write, and visibility stops being a query filter and becomes a per-row test, which the read surface cannot page |
| `ok_interval` on the current interval of each dimension, and on its entry time | work in progress, current age and every ageing condition become scans of the whole index (ADR-0083, ADR-0084) |
| `ok_file_ref (hash)` | erasure cannot tell whether another object still holds a file, and either deletes it too (D207) or never deletes anything (ADR-0087) |
| `ok_event (txn, position)` | the settled cursor becomes a sort of the whole log on every `pull` and `export` (ADR-0089) |

The third is why check 7 rejects a type-scan or a visibility predicate over an unindexed attribute at publish: the schema cannot be built to satisfy it afterwards.

**`exceptions(type)` reads two things and no third.**
- **Admissions** come from `ok_admission` where `discharged_position` is null.
- **Asserted state** comes from `state_source = 'asserted'` on the type table, indexed. The import's state changes are `imported` and are not listed (ADR-0083, repairing D205).

A column is chosen over scanning the log because the query is per type, and the log is the busiest table in the system. It costs one indexed column on every object, and it is written by the same statement that writes the state, so it cannot disagree with the event.

`state_source` changes in three ways:
- **An ordinary transition that changes state** sets it back to `observed`, which is right: an object whose state was asserted and has since moved through the machine normally is no longer an exception, and nobody has to remember to clear a flag.
- **An action leaves it alone.** Editing a note on an asserted unit does not make the unit's state any less asserted, and the object stays in `exceptions(type)` until the machine has moved it (ADR-0077).
- **A migration mapping leaves it alone** too, so an asserted object does not drop off the list by being migrated (ADR-0083).

`ok_attribute_write` deserves its shape stated plainly. It has one row per object per attribute or part relationship ever written, holding the position of the event that last wrote it. It grows with the object's *width*, not with its history, so it is bounded by the declaration.

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
-- Scoped to the actor's id, not global: a key is unique per caller, which is
-- what the first consumer's own evidence shows and what `unique with` was
-- added to the declaration syntax to express, and `id` is the one member of
-- the descriptor that is never absent (ADR-0077). A global key space would
-- let one caller's retry collide with another's. A key reused by the same
-- actor with a different request_digest is refused as KeyReused.
CREATE TABLE ok_idempotency (
  actor_id       TEXT    NOT NULL,
  key            TEXT    NOT NULL,
  request_digest TEXT    NOT NULL,
  first_position INTEGER REFERENCES ok_event(position),
  verdict        TEXT    NOT NULL,
  applied_at     TEXT    NOT NULL,
  PRIMARY KEY (actor_id, key)
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

-- Content-addressed files. The store holds the reference; erasure deletes
-- the content only once no unerased reference to the hash remains, and keeps
-- the row, so a reference resolves to something that says so (ADR-0087).
CREATE TABLE ok_file (
  hash           TEXT PRIMARY KEY,
  size_bytes     INTEGER NOT NULL,
  media_type     TEXT    NOT NULL,
  erased_at      TEXT,                       -- the content has been deleted
  erase_pending  INTEGER NOT NULL DEFAULT 0  -- every reference erased, deletion not yet done
);

-- The reference index per hash: one row per place a hash is written, an
-- attribute, a set's side table or an event payload. An index the log
-- rebuilds, not a second source of truth (ADR-0087).
CREATE TABLE ok_file_ref (
  hash       TEXT    NOT NULL REFERENCES ok_file(hash),
  object_id  TEXT    NOT NULL REFERENCES ok_object(id),
  member     TEXT    NOT NULL,                -- the attribute, or the event payload's field
  position   INTEGER NOT NULL REFERENCES ok_event(position),
  erased     INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (hash, object_id, member, position)
);
CREATE INDEX ok_file_ref_live ON ok_file_ref (hash, erased);

-- The published declaration, one row per version. Never deleted: a recorded
-- event names the version whose rules applied, and history must stay readable.
CREATE TABLE ok_declaration (
  version       INTEGER PRIMARY KEY,
  module        TEXT    NOT NULL,
  source        TEXT    NOT NULL,   -- the .ok text as published
  parsed        TEXT    NOT NULL,   -- the checked form the runtime reads
  published_at  TEXT    NOT NULL,
  published_by  TEXT    NOT NULL,
  change_id     TEXT,               -- the DeclarationChange it installed (ADR-0085)
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
  declaration_version INTEGER NOT NULL,
  last_event     INTEGER NOT NULL REFERENCES ok_event(position),
  target_type    TEXT    NOT NULL,
  target_family  INTEGER NOT NULL DEFAULT 0,
  transitions    TEXT,                       -- null means every transition
  changes_state_only INTEGER NOT NULL DEFAULT 0,
  endpoint       TEXT,                       -- null for a pull subscription
  deliver_as     TEXT,                       -- push only: the descriptor the worker presents, as JSON (ADR-0025)
  lag_warn       TEXT,                       -- durations: lag is the age of the oldest
  lag_fail       TEXT,                       -- unacknowledged event the filter selects (ADR-0100)
  CONSTRAINT t_subscription_state CHECK (state IN ('active','revoked'))
);

-- Runtime state, deliberately not an object: no version, no events, no history.
CREATE TABLE ok_subscription_position (
  subscription_id  TEXT    PRIMARY KEY REFERENCES t_subscription(id),
  acknowledged     TEXT    NOT NULL,         -- a settled cursor (ADR-0089)
  acknowledged_at  TEXT    NOT NULL,
  last_error       TEXT,
  last_error_at    TEXT
);

-- Proposal is a built-in object type.
CREATE TABLE t_proposal (
  id             TEXT    PRIMARY KEY REFERENCES ok_object(id),
  state          TEXT    NOT NULL,
  version        INTEGER NOT NULL,
  declaration_version INTEGER NOT NULL,
  last_event     INTEGER NOT NULL REFERENCES ok_event(position),
  target_id      TEXT    REFERENCES ok_object(id),   -- none for a proposed creation
  target_type    TEXT    NOT NULL,
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

-- DeclarationChange is a built-in object type: how a flow changes (ADR-0085).
-- The publish event belongs to it (D212).
CREATE TABLE t_declaration_change (
  id             TEXT    PRIMARY KEY REFERENCES ok_object(id),
  state          TEXT    NOT NULL,
  version        INTEGER NOT NULL,
  declaration_version INTEGER NOT NULL,
  last_event     INTEGER NOT NULL REFERENCES ok_event(position),
  base_version   INTEGER NOT NULL REFERENCES ok_declaration(version),
  source         TEXT    NOT NULL,          -- the .ok text proposed
  report         TEXT,                      -- the dry run: the impact report, with its ids
  evidence       TEXT,                      -- the evidence as it read at submission (ADR-0097)
  drafted_by     TEXT    NOT NULL,
  drafted_by_kind TEXT   NOT NULL,
  drafted_principal TEXT,                   -- the drafter's principal, if any
  approved_by    TEXT,
  approved_by_kind TEXT,
  CONSTRAINT t_declaration_change_state CHECK (state IN
    ('DRAFTED','SUBMITTED','PUBLISHED','REJECTED','WITHDRAWN','SUPERSEDED')),
  CONSTRAINT t_declaration_change_not_drafter CHECK (approved_by IS NULL
    OR (approved_by <> drafted_by AND approved_by IS NOT drafted_principal)),
  CONSTRAINT t_declaration_change_person_for_agent CHECK (approved_by IS NULL
    OR drafted_by_kind <> 'agent' OR approved_by_kind = 'human')
);

-- Imported history that predates the store (ADR-0015): read-only, not events.
-- Erasure of the object redacts every payload field the import mapping did
-- not list as kept for the entry's kind (ADR-0078).
CREATE TABLE ok_legacy_entry (
  object_id   TEXT    NOT NULL REFERENCES ok_object(id),
  ordinal     INTEGER NOT NULL,
  occurred_at TEXT    NOT NULL,
  payload     TEXT    NOT NULL,
  PRIMARY KEY (object_id, ordinal)
);

-- Labels: the built-in observation kind every type may carry (ADR-0082).
-- A label is an object, recorded by a creation request, like any observation.
CREATE TABLE t_label (
  id             TEXT    PRIMARY KEY REFERENCES ok_object(id),
  state          TEXT    NOT NULL DEFAULT 'RECORDED',
  version        INTEGER NOT NULL,
  declaration_version INTEGER NOT NULL,
  last_event     INTEGER NOT NULL REFERENCES ok_event(position),
  subject_id     TEXT    NOT NULL REFERENCES ok_object(id) DEFERRABLE INITIALLY DEFERRED,
  name           TEXT    NOT NULL,          -- normalised: lowercase, trimmed
  note           TEXT,                      -- treated as personal: erasure redacts it
  subject_state  TEXT    NOT NULL,          -- the subject's state when it was applied
  occurred_at    TEXT,
  corrects_id    TEXT    REFERENCES t_label(id),
  CONSTRAINT t_label_state CHECK (state = 'RECORDED')
);
CREATE INDEX t_label_by_subject ON t_label (subject_id);
CREATE INDEX t_label_by_name    ON t_label (name, subject_state);

-- The interval index: how long each object held each state and each value
-- of each tracked attribute and reference (ADR-0083, ADR-0086). Maintained
-- in the transition's transaction; the log can rebuild it.
CREATE TABLE ok_interval (
  object_id        TEXT    NOT NULL REFERENCES ok_object(id),
  dimension        TEXT    NOT NULL,        -- 'state', or the tracked member's name
  value            TEXT,                    -- NULL for absence: time unassigned is an interval
  entered_position INTEGER NOT NULL,        -- the event that set it; legacy intervals use 0
  left_position    INTEGER,                 -- NULL while it is the current value
  entered_at       TEXT    NOT NULL,        -- the occurred time where backdated
  left_at          TEXT,
  entered_by       TEXT,                    -- who made the change; a legacy row's where known
  entered_by_kind  TEXT    NOT NULL,
  declaration_version INTEGER NOT NULL,
  legacy           INTEGER NOT NULL DEFAULT 0,   -- supplied by the import mapping
  PRIMARY KEY (object_id, dimension, entered_position, entered_at)
);
CREATE INDEX ok_interval_current ON ok_interval (dimension, value) WHERE left_position IS NULL;
CREATE INDEX ok_interval_entered ON ok_interval (dimension, entered_at) WHERE left_position IS NULL;
CREATE INDEX ok_interval_by_object ON ok_interval (object_id, dimension, left_position);

-- The attempt log: every request that did not apply, and every observing
-- clause's would-be refusal. Never the inputs. Not history: pruned after a
-- retention period, its daily counts kept in ok_attempt_rollup (ADR-0083, ADR-0096).
CREATE TABLE ok_attempt (
  id                  INTEGER PRIMARY KEY,
  at                  TEXT    NOT NULL,
  actor_id            TEXT    NOT NULL,
  actor_kind          TEXT    NOT NULL,
  actor_principal     TEXT,
  context             TEXT,
  txn                 TEXT    NOT NULL,     -- its writing transaction, for the settled cursor
  type                TEXT,                 -- none where the request named an unknown id
  object_id           TEXT,                 -- none for a refused creation
  transition          TEXT,                 -- none for a request naming no transition
  verdict             TEXT    NOT NULL,     -- the verdict kind, or the fault's name
  clause              TEXT,
  remedy              TEXT,
  unknown             INTEGER NOT NULL DEFAULT 0,
  enforced            INTEGER NOT NULL DEFAULT 1,   -- 0 for an observing clause
  applied_position    INTEGER,              -- an observing clause's applied event
  reads               TEXT,                 -- the failing clause's read set (ADR-0088)
  consulted           TEXT,                 -- its metric values and evaluator verdicts (ADR-0096)
  declaration_version INTEGER NOT NULL
);
CREATE INDEX ok_attempt_by_rule ON ok_attempt (type, transition, clause, at);
CREATE INDEX ok_attempt_by_actor ON ok_attempt (actor_id, at);

CREATE TABLE ok_attempt_rollup (
  day          TEXT    NOT NULL,            -- UTC calendar day
  object_id    TEXT    NOT NULL DEFAULT '', -- '' for a refused creation; keeps visibility applicable
  verdict      TEXT    NOT NULL,
  type         TEXT    NOT NULL,
  transition   TEXT    NOT NULL DEFAULT '',
  clause       TEXT    NOT NULL DEFAULT '',
  remedy       TEXT    NOT NULL DEFAULT '',
  actor_kind   TEXT    NOT NULL,
  enforced     INTEGER NOT NULL,
  declaration_version INTEGER NOT NULL,
  count        INTEGER NOT NULL,
  PRIMARY KEY (day, type, object_id, verdict, transition, clause, remedy, actor_kind, enforced,
               declaration_version)
);
```

`ok_subscription_position` is the one place the model deliberately keeps runtime state outside an object: no version, no events, no history. An acknowledged cursor moving thousands of times a minute is not something anyone wants a permanent record of, and its lag and death are derived rather than states (ADR-0043). `t_subscription` next to it is an ordinary object, because its filter and endpoint are configuration someone changes and should answer for.

`ok_declaration` is never deleted. Every event names the version whose rules applied, so deleting one makes that stretch of history unreadable.

**`ok_interval` is an index, and says so in its shape.** A state change or a write to a tracked member closes the current row for that dimension, setting `left_position` and `left_at`, and opens a new one in the same statement group that writes the event. Folding the log reproduces it exactly, which the harness checks (ADR-0083). The one kind of row the log cannot rebuild is a `legacy = 1` row, supplied by the import mapping from the legacy system's history. Those are marked so that nothing mistakes them for recorded history.

**`ok_attempt` is written outside the request's transaction**, in its own short transaction after the rollback. The exception is an observing clause's would-be refusal, which is written with the event it is linked to. Either way it holds no input, so erasure has nothing to do there (ADR-0083). Its `consulted` holds the metric values and evaluator verdicts the failing clause was decided on, neither of which can be personal, since a metric's value never is (ADR-0084, ADR-0096).

**Pruning rolls up in the same transaction.** A deployment prunes `ok_attempt` after its retention period with `maintain(prune_attempts)` (ADR-0100), and the statement that deletes a day's rows adds their counts to `ok_attempt_rollup` in the same transaction, so a count is never lost and never counted twice. The metric source `<Type>.attempt_counts` reads the rollup for the days already pruned and counts the retained rows for the rest, so it is complete over the whole history whenever pruning runs, or whether it runs at all. Correctness therefore never waits on it (PRD N2, T3, ADR-0096). The rollup keeps the object, so a reader's visibility applies to a pruned day exactly as to a retained one and a prune changes no reader's value; a refused creation has no object, and its count is visible only to a reader who can see every current object of the type.

**The sequence table and the attempt log are the two things written outside the request's transaction.** Everything else — the object row, the event, the write index, the interval index, the file reference index, the idempotency record — commits with the transition or not at all.

The sequence is the exception because a decision requires it to be: a monotonic sequence with gaps needs its allocation to survive a rollback, and one that rolls back is gapless and serialises the scope (ADR-0029). It is allocated on a **second connection**, and where that connection points depends on how the backend locks.
- **PostgreSQL locks rows.** The second connection opens the same database and updates the `ok_sequence` row, which the request's transaction never touches. An unscoped sequence may be a native `SEQUENCE` instead, whose `nextval` is already non-transactional.
- **SQLite locks the file.** Once the request's connection has begun — and it begins `IMMEDIATE` (§7) — every other connection's write to that file waits out the busy timeout and fails with `database is locked`. So on SQLite `ok_sequence` lives in a **separate database file** beside the store, opened on its own connection and created by the same DDL. A write there does not contend with the store's lock, and a crash between the mint and the request's commit leaves a gap, which ADR-0029 already accepts (ADR-0076, D187).

`scripts/check-schema-doc.py` runs that scenario on every corpus run, in both journal modes: a second connection to the same file blocks behind an open write transaction, one to a separate file does not, and the value it allocated survives the request's rollback.

## 7. Concurrency

The transition transaction runs at **serialisable isolation**, which is what makes a guard's reads safe whether or not the objects it read are ones the transition writes (ADR-0039). What each backend does with a contended request differs, and is stated as observed:

- **SQLite: every transition transaction begins `IMMEDIATE`** (ADR-0090). The write lock is taken at the first statement, so a request's guard reads and its writes are serialised with every other writer's, and a contended request waits out the busy timeout. Under a deferred `BEGIN` in WAL mode, a request that read and then wrote after another committed failed with `database is locked`, which no timeout resolves; `scripts/check-schema-doc.py` keeps both halves as a probe. A busy result past the timeout counts as a serialisation failure: retried within the bound, then `stale`.
- **PostgreSQL: a contended row waits and then retries** (ADR-0090). Probed on PostgreSQL 16.8: at `SERIALIZABLE`, a request taking `SELECT … FOR UPDATE` on a row another transaction updated waits for it to commit, then fails with `could not serialize access due to concurrent update`. Row locks are taken as objects are reached. They make the loser fail at its first touch of the row rather than after doing its work, and they do not make it queue. The retry is what makes it succeed, and each event records how many it took, so the cost is measured rather than argued (ADR-0083). Deadlock is the database's to detect.

What the schema owes that:

- **`ok_event.position` is monotonic and is not a promise of commit order.** A `BIGSERIAL` allocates out of order under concurrency, so a lower position can become visible after a higher one. On PostgreSQL, assigning positions from a single counter row inside the transaction would serialise every commit through one row, which ADR-0034 rejected by name. On SQLite, where writers are serial anyway, the counter row costs nothing, and is what the store uses.
- **The log is read by a settled cursor** (ADR-0089). A reader takes its snapshot's `xmin` and returns only events whose `txn` is below it — every one of them finished — in `(txn, position)` order, handing back the last pair as the cursor. Probed on PostgreSQL 16: while a transaction holding position 1 was in flight and one holding position 2 had committed, the highest visible position was 2, and a consumer acknowledging it would never have seen 1. The query below `xmin` returned nothing until both were finished, then both in order. `pull` and `export` return that cursor, and `acknowledge` stores it. On SQLite, where writers are serial, `txn` order is commit order and the cursor is the position.
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
| overlap of two ranges, per key | `EXCLUDE USING gist` | **none** — dynamic only |
| a traversal or type-scan invariant | none | none |
| any invariant that an assertion may `admit` | **none, on either** | see below |

The last row is **ADR-0074**, derived here because this is the first document that had to hold ADR-0041 and ADR-0054 at once. An admission suppresses **one** invariant for **one** object (ADR-0054), and a database constraint cannot yield for one row. So an invariant a type's assertion names in `may admit` must not be compiled, or an admitted violation would be refused by the database after the runtime allowed it. Publishing knows both facts and can decide it; this is the schema's constraint on that decision.

The overlap row is the one that matters and the one not executed here. A booking-overlap invariant compiles to an exclusion constraint on PostgreSQL and has no equivalent in SQLite, so the same declaration is enforced by the database on one backend and by the runtime on the other. That is legitimate under ADR-0041, and it should be measured before it is believed, because a type-scan under serialisable isolation is where contention will actually appear.

## 9. Erasure, and what archival tiering means

Erasure (§8 of the model, ADR-0087) does these things to storage:

1. Sets each declared `personal` column to `NULL` on the object row, and on every member of its **supersession chain**, predecessors and successors, each through its own type's `erase`.
2. Rewrites the same values inside every past event's `payload`, in place, keeping the event, its position and its shape. `reads` needs nothing, since it holds no value.
3. Walks each of those events up its `cause_position` chain, and redacts in each **caller's event** the inputs that flowed into the erased objects' personal attributes. Check 10's taint analysis identifies which, per declaration version.
4. Marks every `ok_file_ref` row it reaches as erased. After commit it deletes the content of any hash whose references are now **all** erased, setting `ok_file.erased_at`. A deletion that fails sets `erase_pending`. Every later erasure request retries the pending deletions before its own, and `diagnostics` lists them under the type of each object whose `ok_file_ref` rows the erasure reached, so a deployment that sees one re-requests an erasure, which is idempotent; no background process is needed (ADR-0096, PRD N2). A file another object still holds survives, because its reference is not erased (D207). **The blob store's own lifecycle expiry must be off** (ADR-0017): the log is permanent and a reference outlives any expiry policy, so a bucket rule that deletes after ninety days silently breaks history.
5. Records an event with source `erased`, carrying the names of the attributes erased and the files deleted and **never their values**, and an admission for every invariant that read what it erased (ADR-0060).
6. Redacts every legacy entry attached to the object. Each payload field the import mapping did not list as **kept** for that entry kind is replaced by absence, and a kind with no list loses its whole payload. The event of step 5 records how many (ADR-0078).
7. Redacts `t_proposal.inputs` on every proposal targeting an erased object, or whose inputs flow into an erased object's personal attributes, and invalidates the pending ones (ADR-0044, ADR-0078, ADR-0087).
8. Redacts personal **inputs** as well as personal attribute values in step 2 — an input marked `personal`, or one that flows into a personal attribute — so a value passed only to an evaluator does not survive in a payload (ADR-0078).
9. Sets `value` to `NULL` in every `ok_interval` row of an erased object's personal enum attributes, as step 2 does in the events the index is rebuilt from, so a rebuilt index and the redacted one agree (ADR-0096).
10. Deletes the `ok_external_id` rows of the erased objects' personal external identifiers, so `lookup` no longer resolves them (ADR-0100).
11. Redacts each affected proposal's inputs in the proposal's own creation event, as step 7 does in its record (ADR-0100).
12. Follows each erased object's events down their caused events, and erases what flowed from it into other objects' personal attributes, in those events and on those objects, by check 10's taint analysis (ADR-0100).
13. Runs `forget` on every observation in the erased objects' collections, corrected ones included (ADR-0096, ADR-0100).
14. Redacts the `note` of every label on an erased object, since unclassified free text is treated as personal (ADR-0082). 
It does **not** touch `ok_attribute_write`, `ok_attempt`, or `ok_interval` beyond step 9:
- redacting a value inside an event does not change which event last wrote the attribute, so `changed_since` answers the same after an erasure as before;
- a tracked reference holds an id, and a personal enum's values are redacted by step 9;
- an attempt never held an input.

The log is never pruned. **Archival tiering** is therefore not deletion but a second table with the same shape on cheaper storage, plus a view over both: events older than a threshold move, by `maintain(archive_events)` (ADR-0100), and `pull`, `export` and `history` read the view.

**Erasure reaches the archive.** The catalogue of edge cases offered the alternative — archive only events that carry no personal attribute — and it is rejected here. It requires knowing, at archive time, which of a future declaration's attributes will be personal. `personal` can be added to an attribute by a later publish, at which point events already archived under the old answer are in the wrong place. Reaching the archive costs a slower erasure, an operation measured in minutes and performed rarely. Choosing the other way costs correctness on a schedule nobody controls.

The consequence for the archive's storage is the real cost: it must support update, not only append, so a write-once object store is not enough on its own.

## 10. What publishing does

A publish is a declaration version and a set of DDL statements derived from it, applied in the same transaction as the `ok_declaration` row. It is the approval of a `DeclarationChange`, whose id the row records (ADR-0085). **This section owns the DDL mapping only.** What publishing checks, what it reports and when it refuses is [`publish-and-import.md`](publish-and-import.md), which cites this table for the emission step.

| Declaration change | DDL |
|---|---|
| a new type, or a new observation kind | `CREATE TABLE`, its indexes, its constraints |
| a new attribute | `ADD COLUMN`, nullable. DDL never writes a live row: a `default` applies at creation, and a new required attribute is filled by a `backfill` mapping, applied as recorded migration transitions (ADR-0099) |
| a new indexed attribute | `ADD COLUMN` then `CREATE INDEX` |
| a removed attribute | **nothing.** A dropped attribute hides; its recorded values stay in history (ADR-0027) |
| a renamed attribute | `RENAME COLUMN`, carrying the mapping |
| a removed state | no DDL; the `CHECK` is replaced, and the mapping moves live objects out of it first |
| a removed enum member | the `CHECK` is replaced after a `removed member` mapping has moved every live row off it by recorded migration transitions (ADR-0099) |
| a removed type or observation kind | **nothing.** It is retired: no creation, no transition, and its table and history stay readable (ADR-0099) |
| a new invariant of compilable shape | `ADD CONSTRAINT`, after the report says how many live objects violate it |
| a new tracked member | no DDL; its intervals are rebuilt from the log (ADR-0083) |

A removed attribute leaving its column in place is deliberate: the column is how history stays readable, and dropping it would make a fold of the log fail on an event the store still holds.

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
- **Part positions are kept per relationship** in `ok_attribute_write`, and `last_part_event` is gone (ADR-0082, D202).
- **Erasure deletes a file's content only when its last unerased reference goes** (ADR-0087, D207).

**Still open.**

- **The size of a partition**, the retention window for an idempotency record, and the attempt log's retention. These are numbers to measure against a real event rate and a real retry chain, not designs.
- **Whether one table per type survives many types.** A hundred types is a hundred tables, and the first consumer has perhaps thirty. The case to check is a family with many members: an issue tracker with a type per project reaches hundreds. This is the measurement that could send the whole arrangement back to a shared table.
- **The exclusion constraint of §8 is unexecuted**, as is the PostgreSQL column apart from the two probed behaviours of §7.

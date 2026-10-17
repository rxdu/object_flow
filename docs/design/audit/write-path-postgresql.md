<!-- The audit's area `write-path.md` taken again on PostgreSQL for ADR-0213's block 6, 2026-10-17, by a reviewer agent on the release build of `952c28c`'s tree. Its probe scripts and their outputs were in the session's scratchpad and were not kept; each finding names its probe, the method and what it printed. Findings and their fixes are tracked in TODO.md, PostgreSQL in 1.0.0, block 7. -->

# Rules and the record on the write path, on PostgreSQL (promises R and A)

Re-run of `docs/design/audit/write-path.md` (taken on SQLite at 698034b) against the core on PostgreSQL. Reviewer: records re-run, 2026-10-17.

**What was tested.** Release binary `core/target/release/objectflow-service`, modified 20:55, after the uncommitted working-tree edits to `db.rs` (20:52) and `store.rs` (20:49) on top of HEAD f6bca3a; so it was presumably built from the working tree, not a commit (inferred from the file times). PostgreSQL 16.8 in `objectflow-pg16`, databases of my own named `review_records_*`, all dropped at the end. Every probe ran the same requests against a SQLite store and a PostgreSQL store (sandbox: in memory vs `OBJECTFLOW_SANDBOX_POSTGRES` pointed at my own database; deployment: `--store <file> --create` vs `--store "host=… dbname=review_records_…" --create`) and compared the answers. Probes and outputs are in `scratchpad/rr/` (helper `rr/of.py`); no repository file was edited, cargo was not run.

**Method.** (1) Every SQL statement on the write path read against the dialect list (translation in `db.rs:1150-1285`, catalog functions `db.rs:1045-1104`). (2) The audit's own 28 probes (`scratchpad/audit/a2/probes`, copied to `rr/a2/probes`) run against a SQLite and a PostgreSQL sandbox, unchanged but for p31's path to the examples, and their outputs diffed (`rr/run_a2.py`, outputs `rr/a2/out/*.{sqlite,pg}.txt`): 26 identical, 2 differing only in a database error's text (p27) and in JSON key order (p30b, F6). Four of them (p1, p4, p13, p28) stop part-way, identically on both, where the current build answers a step otherwise than the 698034b-era probe expected (a fixed item refusing what it then took), so their later steps ran on neither backend; p31 read the repository's current `customers.yaml`. (3) `scripts/replay-scenarios.py` (copied to `rr/replay/replay.py`, patched only to use the release binary) replayed the metric scenarios' 297 requests and 131 reads on each backend: output identical. (4) Targeted probes for each dialect difference and each record promise, below.

## Findings, most severe first

### F1. Names past PostgreSQL's 63-byte identifier limit are truncated: two set attributes share one side table, silently, and a publish SQLite takes is a 500 (major)

- **Difference.** PostgreSQL truncates every identifier to 63 bytes (NAMEDATALEN − 1); SQLite has no limit. The store builds names by concatenation and never bounds them: set tables `t_<type>__<attr>` (`store.rs:744-751`), indexes `t_<type>_<col>_ix`, `_key_ix`, `_fold_ix` (`store.rs:774`, `827`, `873`, `889`), set-aside renames `t_<type>__<attr>__v<n>` and columns `<col>__v<n>` (`store.rs:4016-4021`, `4092-4097`). The set tables are made `CREATE TABLE IF NOT EXISTS`, so a second name truncating to the first is skipped without an error, and every later `DELETE`/`INSERT`/`SELECT` on either attribute reaches the one table (`store.rs:6292-6310`, read `store.rs:5694-5698`).
- **Probe (observed).** `rr/p1b_longnames_deploy.py`, flow `rr/longnames.yaml`: a type `CustomerOnboardingComplianceReviewCase` (38 characters) with two `string[]` attributes `supporting_documents_received` and `supporting_documents_requested`, published through a DeclarationChange on both deployment stores (checks clean, publish `satisfied` on both). `ask_for passport`, then `receive utility bill`:
  ```
  sqlite-dep get attributes: {"supporting_documents_received": ["utility bill"], "supporting_documents_requested": ["passport"]}
  pg-dep get attributes:     {"supporting_documents_received": ["passport"],     "supporting_documents_requested": ["passport"]}
  sqlite-dep history wrote: [..., {'supporting_documents_requested': ['passport']}, {'supporting_documents_received': ['utility bill']}]
  pg-dep history wrote:     [..., {'supporting_documents_requested': ['passport']}, {'supporting_documents_received': ['passport', 'utility bill']}]
  PG tables: t_customeronboardingcompliancereviewcase / t_customeronboardingcompliancereviewcase__supporting_documents_
  PG rows: customeronboardingcompliancereviewcase-2|0|passport
  ```
  "utility bill" is gone from the store, and the `receive` event records a write of `['passport', 'utility bill']` that the flow never made. Every answer was `satisfied`.
- **Probe (observed), index names.** `rr/p1c_longidx_deploy.py`, flow `rr/longidx.yaml`: the same type with two optional references `reviewer_assigned_from_team` and `reviewer_assigned_from_region`. SQLite: publish `satisfied`, version 1. PostgreSQL: `500 {"fault": "the store: 42P07: relation \"t_customeronboardingcompliancereviewcase_reviewer_assigned_from\" already exists (in `CREATE TABLE …"}`.
- **Inferred, not probed.** A set-aside rename whose source name is already 63 bytes renames a table to itself (an error, the publish refused); one whose `__v<n>` suffix is cut leaves `clear_kept`'s parse of `<attr>__v<n>` (`store.rs:1065-1070`, `1116-1140`) unable to find the attribute, so an erasure would not clear a set-aside personal set (P, area 1's).
- **Consequence.** R, A and V broken silently on a flow both checkers accept: state lost and the log recording a write that did not happen; or a version that cannot be published on PostgreSQL. Names this long are unusual but legitimate (a 38-character type and attributes sharing a 22-character prefix), so major.
- **Fix.** Bound every generated identifier: where `t_…` names exceed 63 bytes, use a stable short form (a prefix plus a hash of the full name) on both backends, kept in the catalog the store reads, or refuse at publish (both checkers) any type, attribute or generated name whose identifier would exceed 63 bytes. Make the set-table DDL plain `CREATE TABLE` so a collision fails loudly. Suite: a flow with names at and past 63 bytes, run on PostgreSQL.

### F2. The store accepts a database whose `LC_CTYPE` or ICU provider folds case beyond ASCII; then `identity_taken` is bypassed, a known actor is refused, and an erasure overwrites another person's identity (major)

- **Difference.** The engine folds identities by their ASCII letters only (`wire.md`; Rust `to_ascii_lowercase`, `world.rs:1955-1969`), and SQLite's `lower()` is ASCII-only. PostgreSQL's `lower()` follows the database's `LC_CTYPE`, or its ICU locale. The guard checks only `datcollate` (`db.rs:1593-1604`), so a database made `LC_COLLATE 'C'` with any other `LC_CTYPE`, or with `LOCALE_PROVIDER icu ICU_LOCALE 'en-US' LC_COLLATE 'C' LC_CTYPE 'C'`, is accepted. `operate.md:69` says to set `LC_CTYPE 'C'`, but the service's refusal and the help text mention only the collation. The store's SQL relies on `lower()` being ASCII in: `read_ids_folded` (`store.rs:5360-5373`, binds `text.to_ascii_lowercase()` against `lower(col)`), `identity_groups` (`store.rs:5328`), and an erasure's renames (`store.rs:6451-6501`, `6513`, `6579-6596`).
- **Probe (observed).** `rr/p2_ctype.py`, people + customers flows, sandboxes on SQLite, a C/C database and `review_records_ctype` (`LC_COLLATE 'C' LC_CTYPE 'en_US.utf8'`); `rr/p2d_icu.py` the same on `review_records_icu` (ICU en-US, datcollate `C`, where `select 'a' < 'B', lower('É')` gave `t|é`). After `add user Émile`:
  ```
  request by ÉMILE (other ASCII case): sqlite and pg(ctype C) satisfied; pg(ctype en_US) and pg(ICU) unsatisfied, clause actor_known
  add user ÉMILE (equal but for ASCII case): sqlite and pg(ctype C) invariant_violated identity_taken; pg(ctype en_US) and pg(ICU) satisfied, user-3 created
  ```
- **Probe (observed), erasure.** `rr/p2b_ctype_erasure.py`, flow `rr/staffp.yaml` (an Employee whose `user_id` identity is personal): employees `Émile` and `émile` (distinct under ASCII folding, on every backend), each makes a refused request, then `émile` (employee-2) is erased. On SQLite and pg(ctype C) Émile keeps `user_id: "Émile"` and acts afterwards. On pg(ctype en_US): `employee-1 attributes: {"user_id": "employee-2", …}`, Émile's next request is refused `actor_known`, and Émile's attempt row is re-attributed to `employee-2`.
- **Consequence.** A3 (an identity names one actor) and A1 (refusals recorded with the actor named) broken; an erasure destroys another person's identity. It needs a database configuration the service accepts plus identities with non-ASCII letters (names, IDN emails), so major.
- **Fix.** Refuse at connect a `datctype` other than `C`/`POSIX` and any `datlocprovider` other than libc (`db.rs:1593-1604`), saying both in the message and `--help`; or make the folding independent of the database by writing `lower(x COLLATE "C")` in the translation, the indexes included. Suite: the probes above on a C-collate/en_US-ctype database.

### F3. On PostgreSQL a lock wait is never retried: a write meeting another session's lock is a `500 internal`, not `stale`/`retries_exhausted`, a publish fails whenever any read is open past 5 s, and a committed erasure is answered 500 (major)

- **Difference.** SQLite meets contention at `BEGIN IMMEDIATE`, which `begin_write!` retries `BUSY_RETRIES` times (`store.rs:111-149`) and `request_told` answers `RetriesExhausted` (`request.rs:6390`). PostgreSQL's `BEGIN ISOLATION LEVEL SERIALIZABLE` takes no lock; waits happen at statements, under `lock_timeout` 5 s (`db.rs:1758-1766`). `db::Error::is_busy` recognises 55P03/40001/40P01 (`db.rs:139-154`), but `StoreError::from(db::Error)` drops it (`store.rs:316-324`) and `StoreError::is_busy` only tests the `busy` prefix (`store.rs:59-61`), so every statement-level busy becomes `Fault::Internal` (`objectflow-service/src/main.rs:1511-1526`), and the busy branches at `store.rs:2974` (an erasure's vacuum), `store.rs:3571` (`acknowledge`) and `request.rs:6390` never fire on PostgreSQL. `db.rs:140-141` says these are "each retried as SQLite's busy is"; they are not. Every type table's `id` references `of_object` (`store.rs:663`), and a read open on `of_object` alone blocks a publish's rebuild of a type table (observed below); that the waiting statement is the batch's `DROP TABLE`, which removes the foreign key's triggers on `of_object` and so needs its ACCESS EXCLUSIVE lock, is inferred from the probe, the fault naming the whole batch.
- **Probe (observed), a request.** `rr/p3c_request_busy.py`: another holder of the write lock (SQLite: a second connection's `BEGIN IMMEDIATE`; PostgreSQL: `LOCK TABLE t_ticket IN SHARE MODE`, as a DBA's `CREATE INDEX` takes), then `hit`:
  ```
  sqlite-dep: 200 40.0 s {"kind": "stale", "remedy": "temporal", "cause": "retries_exhausted"}
  pg-dep:     500 5.0 s {"fault": "the store: 55P03: canceling statement due to lock timeout (in `UPDATE t_ticket SET …`)", "code": "internal"}
  ```
  The PostgreSQL store was left whole: counters `position 5, txn 5`, ticket version 1.
- **Probe (observed), a publish.** `rr/p3_publish_vs_reader.py`: a `REPEATABLE READ` read holding a snapshot on `t_note`, then a publish adding an optional attribute to Note: SQLite `200 … satisfied`; PostgreSQL `500 5.0 s {"fault": "the store: 55P03: canceling statement due to lock timeout (in `INSERT INTO t_note__rebuild …"}`. `rr/p11_snapshot_vs_rebuild.py`: the read touched only `of_object` (`SELECT count(*) FROM of_object WHERE type = 'Note'`) and the publish still failed the same way, at `… DROP TABLE t_note; ALTER TABLE t_note__rebuild RENAME TO t_note;`. After the lock went, the store was whole and the same publish succeeded (`rr/p3d_publish_fails_late.py`: a publish failed at its `INSERT INTO of_declaration`, then requests, and the publish again, all satisfied; counters 15 = 15 events).
- **Probe (observed), an erasure.** `rr/p3b_erasure_vacuum.py`: another session holds `SHARE UPDATE EXCLUSIVE` on `of_event` (what a manual VACUUM or ANALYZE takes), then `forget` with key `erase-1`:
  ```
  erasure answered: 500 5.0 s {"fault": "the store: 55P03: canceling statement due to lock timeout (in `VACUUM \"of_admission\", …"}
  after: version 2 name None forget_by_kind human
  history transitions: [(5, 'onboard'), (6, 'forget')]
  client retry under its key: 200 … "events": [6], "replayed": true
  client retry without a key: 200 … "version": 3, "events": [7], "replayed": false
  ```
  The erasure committed and was answered as a fault; an unkeyed retry recorded a second erasure.
- **Consequence.** The busy contract of ADR-0178 decision 2 is SQLite's alone: on PostgreSQL contention is a 500 the caller cannot tell from a bug, never retried, and a committed erasure is reported as failed (A: the caller gets no verdict for a change that happened; an unkeyed retry duplicates it). The publish case is reachable in the profile's operations, a daily publish beside a `pg_dump` (`operate.md`'s backup, which by PostgreSQL's documentation holds ACCESS SHARE on every table it dumps for its whole run; not measured here) or a read past 5 s (an export, a long metric, a dry run of another change, up to 2 min). Major. Inferred, not probed: while the publish waits for its lock, new reads queue behind it for up to 5 s (C); in a group, one lock wait aborts the group and each request then waits again alone.
- **Fix.** Map a statement-level busy to a `busy:` `StoreError` (in `From<db::Error>`, using `db::Error::is_busy`), and on PostgreSQL retry the whole transaction (not only `BEGIN`) up to `BUSY_RETRIES`, answering `RetriesExhausted` past them. Never let `after_erasure`'s failure turn a committed request into an error: in `request_told`, log it and leave the vacuum due, as `commit_group` already does (`store.rs:3759`). For publishes, take the needed locks first with `LOCK TABLE … NOWAIT` in a retry loop, or rebuild without `DROP TABLE` on a table referencing `of_object` (`ALTER TABLE … ADD/DROP/ALTER COLUMN`), and say in `operate.md` that a publish waits for open reads and backups. Suite: each of the three probes on PostgreSQL.

### F4. Text longer than about 2.7 KB in any indexed column is a 500 on PostgreSQL: a unique or indexed string, an identity, and the attempt row of a request naming a long unknown actor or transition (major by A1, nothing applied)

- **Difference.** PostgreSQL's btree index entry is at most 2704 bytes; SQLite has no such limit. The wire takes strings and identities up to 64 KiB, and a transition or actor name up to the 2 MiB body. Indexed text columns: uniqueness keys (`store.rs:806-829`), `indexed`/external attributes and the identity fold index (`store.rs:861-891`), `of_attempt_by_rule (type, transition, clause, at)` and `of_attempt_by_actor (actor_id, at)` (`store.rs:1979-1980`), the actor indexes (`store.rs:6109-6116`).
- **Probe (observed).** `rr/p10_big_index.py`, flow `rr/big.yaml`, 8000 random alphanumeric characters each. SQLite satisfied or refused as a verdict with an attempt row; PostgreSQL `500 {"fault": "the store: 54000: index row size 8016 exceeds btree version 4 maximum 2704 for index …"}` for: a unique serial (`t_asset_serial_key_ix`), an indexed note (`t_asset_note_ix`), an identity (`t_person_login_fold_ix`), an unknown actor (`of_attempt_by_actor`), an unknown transition (`of_attempt_by_rule`). `Asset.attempts` then held two rows on SQLite and none on PostgreSQL.
- **Consequence.** A write SQLite takes is refused as a fault on PostgreSQL; a refusal of an oversize actor or transition is answered 500 and not recorded (A1, for any caller on the socket, a manipulated agent's tool call included). Nothing is applied or misattributed. Major by A1's bullet; the author may judge the stored-value half under §4's "inputs larger than any flow of section 1 holds".
- **Fix.** Bound what reaches an index on both backends: refuse a name, actor or transition string past a small bound at the wire (`400`), and an indexed, unique or identity value past ~2 KB as `out_of_range`; or index a hash (`md5(col)`, or a hash index for the equality reads the store makes) instead of the text.

### F5. A U+0000 in `context`, `idempotency_key`, the actor or the transition is stored by SQLite and a 500 on PostgreSQL, with no attempt row (major by A1, nothing applied)

- **Difference.** PostgreSQL `text` cannot hold U+0000 (22021); SQLite stores it. The wire refuses U+0000 only in string values (`wire.rs:290`), not in the request's other text.
- **Probe (observed).** `rr/p9_nul.py`: SQLite `satisfied` for a context `"why\u0000not"` and for a key `"k\u0000"`, and verdicts `actor_known`, `invariant_violated` and `unknown_transition` (each with its attempt row, exported) for an actor, a refused request's context and a transition holding U+0000; PostgreSQL `500` for every one (`22021: invalid byte sequence for encoding "UTF8": 0x00`), and none of the three attempt rows exported.
- **Consequence.** As F4: a refusal answered 500 and unrecorded on PostgreSQL, a request SQLite satisfies faulted, and the backends differ for the same requests.
- **Fix.** Refuse U+0000 in every request string at the wire, `400`, on both backends: actor, principal, type, object id, transition, input names, `context`, `idempotency_key`.

### F6. An erasure rewrites a whole event payload through `jsonb`, so on PostgreSQL its keys come back reordered (minor)

- **Difference.** `json_set` translates to `jsonb_set(t::jsonb, …)::text` (`db.rs:1095-1103`), which re-serialises the whole payload in jsonb's order (keys by length, then bytes) and normal form (`1e2` → `100`, `-0` → `0`, checked in psql: `'{"b": 1, "aa": 2, "a": 3}'::jsonb::text` gave `{"a": 3, "b": 1, "aa": 2}`); SQLite's `json_set` keeps the order. The core reads payloads with serde_json's `preserve_order` (`core/Cargo.toml:30`), so the order reaches the wire.
- **Probe (observed).** `rr/p5_payload_order.py`, customers flow, `register`, `add_contact`, `correct_email`, `forget`: history and `export log` semantically equal, textually different, e.g. event 3 SQLite `{"inputs": {"email": null, "name": null}, "wrote": {...}}` vs PostgreSQL `{"wrote": {"name": null, "email": null, "number": "C-00001"}, "inputs": {"name": null, "email": null}}`. The audit's p30b diff is the same difference.
- **Consequence.** V/K: the same requests give different bytes on the two backends and against the reference runner; a consumer hashing or diffing export lines sees a change. No value differs.
- **Fix.** Rewrite the payload in Rust as `rewrite_written` already does (`store.rs:6127-6155`), or keep `json`/`jsonb` out of the round trip; or document that the order of a payload's keys is not part of the answer.

### F7. Latent, inferred: PostgreSQL answers `COMMIT` of an aborted transaction with `ROLLBACK` and no error, and the store treats it as committed (minor, hardening)

- **Difference.** After any error, a PostgreSQL transaction is aborted and `COMMIT` rolls it back without an error; SQLite rolls back only the failed statement. Shown in psql: `BEGIN; SAVEPOINT request; SELECT 1; RELEASE request; SELECT 1/0; COMMIT;` printed `ERROR: division by zero` then `ROLLBACK`, and the script went on. `Transaction::commit` (`db.rs:1848-1851`) and `commit_group` (`store.rs:3747-3760`) send `COMMIT` by `batch_execute`, which ignores the command tag.
- **Route (inferred, not reproduced).** `told()` swallows read errors, `world.get(id).ok().flatten()` (`request.rs:6270`, `6287`), on the writer's connection inside a group's transaction. A read error there (a lock timeout on a type table another session holds ACCESS EXCLUSIVE, say), in the last request of a group, would leave the transaction aborted; `commit_group` would then report success and answer every request of the group with its verdict while nothing was committed. I found no reachable trigger through the wire (a U+0000 id fails earlier, in the decision, and is rolled back to its savepoint, `rr/p9_nul.py`), so this is a hazard, not an observed failure. A failed `COMMIT` also leaves `Pg::open` true (`db.rs:990-1006` updates it only on success).
- **Fix.** On PostgreSQL send `COMMIT` with `simple_query` and refuse anything but a `COMMIT` tag, or track an aborted state in `Pg` (set when a statement fails while a transaction is open, cleared by `ROLLBACK`/`ROLLBACK TO`) and refuse `COMMIT` on it; propagate read errors in `told()` instead of dropping them.

## The inventory on PostgreSQL

Each item was run by the audit's own probe on both backends (outputs in `rr/a2/out/`), identical unless said. The current build has fixed most of the audit's broken items; each now behaves the same on PostgreSQL as on SQLite.

| # | On PostgreSQL | Evidence |
|---|---|---|
| A-01 | holds as on SQLite (fixed: the label gets a `redact` event, version 2) | p3 identical |
| R-01 | holds as on SQLite (fixed: `not_found`) | p1 identical |
| R-02 | holds as on SQLite (fixed: `not_erased` in a `foreach`) | p2 identical |
| R-03 | holds as on SQLite (fixed: `whole_open` on both routes) | p12, p12b identical |
| R-04 | holds as on SQLite (fixed: the import refuses the batch) | p16 identical |
| R-05 | holds as on SQLite (fixed: a call into `only_via` refused `not_requestable`) | p20 identical |
| I-01 | holds as on SQLite (fixed: `identity_unchanged`) | p4 identical |
| I-02 | holds as on SQLite (the publish adding the type is refused `dry_run_clean`; the person stays human) | p5 identical |
| I-03 | holds as on SQLite (fixed: the backfill is fatal) | p29 identical |
| K-01, K-02 | holds as on SQLite (fixed: `key_reused`) | p26 identical; `rr/p4_keys.py` identical |
| K-03 | not applicable on PostgreSQL: a PostgreSQL store is made at this release (`operate.md`), so no 0.2.0 digest exists | reading |
| A-02 | holds as on SQLite (fixed: refused publishes write attempt rows) | p18 identical |
| A-03 | holds as on SQLite (fixed: the erased caller gets a `redact` event); payload key order differs (F6) | p30b differs only in key order |
| K-04 | holds as on SQLite (fixed: principal by identity, id and case replay) | p7 identical |
| R-06 | holds as on SQLite (fixed: `not_proposable`) | p8 identical |
| A-04 | holds as on SQLite (fixed: `delegable` through a cascade) | p6 identical |
| A-05 | holds as on SQLite (fixed: a check lists events and ids `not_computed`) | p28 identical |
| A-06 | holds as on SQLite (each object gets a `migrate` event) | p17 identical |
| I-04 | not applicable on PostgreSQL, as K-03 | reading |
| R-07 | broken as on SQLite (an unchecked flow's cleared required attribute is a 500); only the database's message differs | p27 |
| H-01, H-02, H-07, H-08, H-11, H-12, H-14, H-34 | hold as on SQLite, except that on PostgreSQL a refusal naming an oversize or U+0000 actor or transition is a 500 with no attempt row (F4, F5) | p9, p18, p21 identical; `rr/p9_nul.py`, `rr/p10_big_index.py` |
| H-03 to H-06, H-09, H-10, H-13, H-17 to H-33, H-35, H-38 | hold as on SQLite | p1, p6, p8, p12, p13, p13b, p16, p16b, p17, p19, p20, p21, p25, p31 identical; the scenario replay identical |
| H-15 | holds as on SQLite for values under ~2.7 KB; past that a unique or indexed string is a 500 (F4) | p9 identical; `rr/p10_big_index.py` |
| H-16 | holds on a C/C database; broken on a C-collate database with another `LC_CTYPE`, or ICU, which the store accepts (F2) | `rr/p2_ctype.py`, `rr/p2d_icu.py` |
| H-36 | holds as on SQLite | p7, p26 identical; `rr/p4_keys.py` |
| H-37 | holds on a C/C database; on a non-C `LC_CTYPE` the rename over-matches another actor (F2) | `rr/p2b_ctype_erasure.py` |
| H-39 | holds: one writer by the advisory lock, writes serialised, no lost update | second service refused (`another service holds the store … one service writes a store`); `rr/p7b_lost_update.py`: 10 threads, 451 satisfied hits, `hits 451`, version 452, the same on both |
| N-01 to N-06 | same on both | p7, p8, p21 identical |

## The record's promises on PostgreSQL

- **A request atomic with its event and its counters: holds.** `rr/p7_groups.py` (8 writers, 320 grouped requests, 84 groups on SQLite and 89 on PostgreSQL): on both, positions 1..203 with counter 203, no object with a gap in `object_seq`, every version equal to its last `object_seq`. A request failed by a lock (`rr/p3c_request_busy.py`) or a publish failed late (`rr/p3d_publish_fails_late.py`) left nothing: counters equal to the events written.
- **Positions, object sequences and the sequence store across a refusal and a savepoint rollback: hold.** Same probe: tickets 104 distinct, `of_sequence.next_value` 161, the 56 refused creations' draws kept unused, on both. Positions and object numbers come from `of_counter`, never from an identity column; `of_object.rowid` (an identity column on PostgreSQL, `store.rs:1876-1877`) and `of_attempt.id` take gaps on rollback, which no answer exposes (`rr/p12_cursor_rowid.py`: query, exceptions and available cursors identical).
- **Across a request retried as busy: no retry happens on PostgreSQL (F3);** the failed request leaves the counters whole.
- **Idempotency keys: hold as on SQLite.** `rr/p4_keys.py`: replay, 409 for another personal or non-personal value, 409 for every request under a key once its personal input was erased, replay of an unerased key, identical. The digest's `substr(…, 1, length(?3))` binds as text on PostgreSQL (checked with `PREPARE`, `pg_prepared_statements`: `{text}`).
- **Deferred keys checked at commit: hold by reading, not probed.** References are `DEFERRABLE INITIALLY DEFERRED` (`store.rs:686`, `739`, `769`); a group's failed `COMMIT` is rolled back and each request run alone (`store.rs:3751-3753`, `main.rs:1880-1888`); DDL inside a transaction checks pending ones first (`db.rs:979-983`). No wire request makes the engine write a dangling reference. On PostgreSQL a reference to a type table is translated to `REFERENCES of_object` (`db.rs:1265-1267`), a weaker check than SQLite's, so PostgreSQL compiles less than SQLite, against the letter of ADR-0213 decision 7; harmless while the engine checks the type.
- **One writer: holds** (above).

## Dialect differences checked and found to hold

- Scalar `max`/`min` → `GREATEST`/`LEAST`: every use has non-null arguments (`store.rs:3284`, `5963-5964`, `6100`, `6728`).
- `ORDER BY` with nulls: no SQL ordering on a nullable column; orderings of objects, sets and pages are by non-null keys or done in Rust.
- `LIKE`: only on catalog names the store generates in lower case (`store.rs:1120`, `3325`; `db.rs:1336`).
- `lower()`: ASCII-only on a C/C database (`select lower('ÉÀ Ö')` unchanged), so equal to SQLite there; see F2 otherwise.
- Integer division: none in SQL. Overflow: only `value + 1` on counters (`store.rs:4829`), unreachable.
- Text comparison: byte order under `C` equals SQLite's BINARY for UTF-8; the guard misses ICU (F2).
- Numbers as text or real: attribute columns are only `INTEGER`/`TEXT` (`store.rs:1852-1862`); no `REAL` column, no `sum`/`avg` in SQL.
- JSON: `json_type` names, `json_extract` text, paths and `json_valid` agree on everything the store asks (the erasure probes identical in value); jsonb refuses `\u0000` (`pg_input_is_valid` gave `f`), unreachable since the wire refuses U+0000 in values; reordering is F6.
- `GROUP BY`/`DISTINCT`: over single-typed columns only; the rollup groups by exactly its conflict key (`store.rs:3427-3434`), so `ON CONFLICT DO UPDATE` never meets a row twice.
- `||` with NULL: no nullable operand. `INSERT OR IGNORE` → `ON CONFLICT DO NOTHING`: every use inserts non-null values, so SQLite's wider ignore never differs.
- Empty `IN ()`: every write-path list is chunked from a non-empty slice or guarded (`store.rs:5466-5476`, `5513-5527`, `5535-5553`, `5787-5795`).
- Booleans as integers, triggers, `RETURNING`: no difference (no trigger or `RETURNING` in the store's SQL; booleans bind and read as integers, `db.rs:511-513`, `638-641`).
- Parameter inference: the PostgreSQL types inferred for the risky statements are the text and integer the store binds; the erasure's and digest's statements ran on PostgreSQL in the probes above.

## Not checked, and why

- **The core's test suite on PostgreSQL:** not run. A gate run held the default target directory, and a build in a separate one was not needed for a differential review with the release binary; the gate's own run covers it.
- **The silent-`COMMIT` route of F7:** inferred from the code and psql; no wire trigger found.
- **Anti-wraparound autovacuum** as the holder in F3's erasure case: not probed; I relied on an explicit `SHARE UPDATE EXCLUSIVE` lock. Ordinary autovacuum is cancelled by PostgreSQL when it blocks a lock request, from recollection of its documented behaviour, not measured here.
- **Load at the profile, failover, replicas, TLS:** outside a records review; not probed.
- **K-03 and I-04 on PostgreSQL:** they need 0.2.0-era rows, which a PostgreSQL store cannot hold.

## Seen outside this area, for its owner

- **P (area 1):** F1's truncated set-aside names would leave `clear_kept` unable to clear a set-aside personal set on PostgreSQL (inferred, `store.rs:1065-1140`).
- **C (area 4):** a publish waiting for its ACCESS EXCLUSIVE lock on `of_object` queues every new read behind it for up to the 5 s lock timeout (inferred from PostgreSQL's lock queue, F3).
- **E (area 5):** `acknowledge`'s busy branch (`store.rs:3571`) is dead on PostgreSQL, as F3's; `read_log_after`, `read_attempts_after` and `read_interval_page` build `o.type IN (…)` from a type list they do not guard against empty (`store.rs:1398-1410`, `1278-1289`, `6024-6041`), which PostgreSQL refuses as a syntax error where SQLite reads nothing; not probed.

## Reproducing

```
SP=/tmp/claude-1000/-home-rdu-RduWs-object-flow/371fe136-73a0-4d6c-a955-ba419d6aff93/scratchpad
cd $SP/rr
# each deployment probe needs its database first, e.g.:
docker exec -i objectflow-pg16 psql -U objectflow -d objectflow -c "CREATE DATABASE review_records_dep1;"
# sandboxes use review_records_sbx; F2 needs:
#   CREATE DATABASE review_records_ctype LC_COLLATE 'C' LC_CTYPE 'en_US.utf8' TEMPLATE template0;
#   CREATE DATABASE review_records_icu LOCALE_PROVIDER icu ICU_LOCALE 'en-US' LC_COLLATE 'C' LC_CTYPE 'C' TEMPLATE template0;
python3 p1b_longnames_deploy.py      # F1 (review_records_dep1)    python3 p1c_longidx_deploy.py  # F1 (dep2)
python3 p2_ctype.py; python3 p2d_icu.py; python3 p2b_ctype_erasure.py   # F2
python3 p3c_request_busy.py (dep6); python3 p3_publish_vs_reader.py (dep3); python3 p11_snapshot_vs_rebuild.py (dep9); python3 p3b_erasure_vacuum.py (dep4)   # F3
python3 p10_big_index.py; python3 p9_nul.py; python3 p5_payload_order.py   # F4, F5, F6
python3 run_a2.py                     # the audit's probes on both backends
python3 p7_groups.py (dep5); python3 p7b_lost_update.py (dep8); python3 p4_keys.py; python3 p12_cursor_rowid.py
```

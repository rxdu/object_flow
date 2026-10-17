<!-- The audit's area `personal-data.md` taken again on PostgreSQL for ADR-0213's block 6, 2026-10-17, by a reviewer agent on the release build of `952c28c`'s tree. Its probe scripts and their outputs were in the session's scratchpad and were not kept; each finding names its probe, the method and what it printed. Findings and their fixes are tracked in TODO.md, PostgreSQL in the release, block 7. -->

# Personal data end to end on PostgreSQL (P, and A where an identity is concerned)

The area `docs/design/audit/personal-data.md` covers, taken again on PostgreSQL 16.8 (ADR-0213). Repository at `f6bca3a` plus the uncommitted working tree (its changes to `db.rs`/`store.rs` are the read cache and `read_orders`, not erasure). Probes used the release binary `core/target/release/objectflow-service` (built 2026-10-17 20:55 local; it reports `objectflow-core 1.0.0`). They ran against my own container `review-personal-pg` (postgres:16.8, `--locale=C`, port 55441, started with `shared_preload_libraries=pg_stat_statements` and `max_prepared_transactions=5`, everything else at its defaults). The container is now stopped and removed. I did not touch `objectflow-pg16` or `objectflow-pg16-profile`, ran no cargo, and edited no repository file. All data was synthetic (`ZZ-MARKER-7f3a-…`, `…@synthetic.invalid`).

Scripts and service logs are in `scratchpad/pd/`. `ofpg.py` is the helper (start the service, post, grep the container's files, map a file to its relation). `where_bytes.py` uses `pageinspect` to sort each occurrence on a page into one of four places: a live tuple, a dead tuple, or the free space between `pd_lower` and `pd_upper`. Each probe is `cd scratchpad/pd && python3 <script>`, and the scripts are named below.

**How to read the evidence.** "Observed" means a probe I ran, with what it printed. "Read" means a `file:line` I read. "Inferred" means reasoning from PostgreSQL's documented behaviour that I did not measure.

## Summary: decision 8 does not hold as written

ADR-0213 decision 8 (`docs/adr/0213-postgresql-in-1-0-0.md:24`) and `operate.md:93` say that after the erasure's VACUUM and CHECKPOINT the data files hold no erased value, citing a three-value measurement. On this binary I observed the opposite every time I looked:

- **After every erasure, the erased bytes stayed in the data files' free space.** 6 of 6 flows and shapes, heap and index pages alike.
- **The store's own statistics kept erased values readable in SQL.** `pg_stats` answered them.
- **The index-free-space maintenance fails with a 500 on PostgreSQL.**

SQL-level erasure, the reads, the record under a C ctype, the logs and the password handling hold.

## Findings, most severe first

### 1. Blocker: erased values stay in the free space of data and index pages after the erasure's VACUUM, including copies the erasure itself writes

- **Inventory items.** F1 (freed pages) and S1 to S25 at the byte level.
- **What fails.** Plain `VACUUM` prunes dead versions but does not zero the space they leave. Each surviving copy lies between `pd_lower` and `pd_upper` of a heap or B-tree page. The erasure also redacts one attribute per `UPDATE` (`core/crates/objectflow-core/src/store.rs:6366-6390`). Each pass writes another version of the event row that still holds the remaining personal values. In the 4 KB TOAST case the payload moves back in-line as it shrinks. So the erasure puts new copies into the heap, and the WAL, before its VACUUM.
- **Evidence: four attribute shapes (observed, `p1_bytes.py`).** A type with `name`, `email` (unique, indexed), `note` and an optional 4 KB `bio`. I ran a `CHECKPOINT`, swept, erased through `/v1/request`, waited 3 s (the service's VACUUM ran: `vacuum_count` was 1 on every table) and swept again:
  - all four (`p1small`): `of_event`'s heap held the name ×7, email ×2, note ×11, all in free space (`where_bytes.py`). Before the erasure `of_event` held none, because the values were in TOAST then. The TOAST copies themselves were gone.
  - name, email and note only (`p1nobio`): `of_event` name ×5, email ×1, note ×9.
  - name only (`p1nameonly`): `of_event` ×1, `page 1 offset 447: free space (lower 92, upper 896)`.
  - 6,000 other objects (`p1large`): `of_event` name ×7, note ×11, email ×3. The email index's live entry also stayed (finding 2).
- **Evidence: an identity with every kind of copy (observed, `p16_identity.py`).** Author, a watchers set, an owner sent in upper case, a withdrawn and an approved proposal, a correction, a refused request, and a version she approved:
  - SQL level: nine tables held her identity before the erasure and none after. The rename holds.
  - Byte level: after the service's VACUUM, 10 relations still held it, all in free space: `of_event` ×20, `t_memo` ×3, `t_proposal` ×4, `t_memo__watchers` ×4, `of_interval` ×2, `t_declarationchange` ×1, and the indexes `of_event_by_actor` ×5, `of_interval_by_entered` ×4, `t_person_login_fold_ix` ×1, `t_person_login_key_ix` ×1.
- **Evidence: an external identifier (observed, `p19_external.py`).** The badge stayed in the heaps of `of_external_id` and `of_event`, and in `of_external_id_pkey`.
- **How long (observed, `p10_persist.py`).** They last until a later tuple overwrites that free space. After 2,000 more creations on `p1large`, 2 of the 7 name copies and 2 of the 11 note copies were still in `of_event`. Pages that receive no new rows keep them with no bound (inferred).
- **Fixes I checked (observed).** `REINDEX INDEX` on the four indexes and `VACUUM FULL of_event`, then a `CHECKPOINT`, removed those relations' copies. The tables I did not rewrite kept theirs.
- **Compared with SQLite.** `secure_delete` zeroes freed space and the `-wal` is cut (`an_erasure_leaves_no_erased_bytes_in_the_stores_files`, `deployment.rs:3455`). The PostgreSQL test `an_erasure_on_postgresql_vacuums_once_no_older_snapshot_holds_it` (`request.rs:9860`) only asserts that `last_vacuum IS NOT NULL`. No PostgreSQL test checks bytes.
- **Consequence.** Any process or person that reads the server's data directory, a physical backup or a volume snapshot reads erased values. That includes a physical backup taken after the erasure (finding 6). This breaks P's first bullet ("gone … from the store's file … within the documented cut") in normal operation (the `customers.yaml` pattern), and it contradicts decision 8 and its measurement.
- **Suggested fix.**
  - Write each event row once per erasure (build the redacted payload in Rust, one `UPDATE`), so the erasure stops minting copies.
  - Make the housekeeping rewrite the tables an erasure wrote: `VACUUM (FULL)` per table, which also rebuilds every index, constraint-backed ones included, or an online rewrite. Run it in the daily maintenance window, as SQLite's `rebuild_indexes` already runs.
  - State the bound in `operate.md`: until the next housekeeping run.
  - Correct ADR-0213 decision 8's text and its "refined" measurement. Retract `operate.md:93`'s sentence "three erased values were in no table's or TOAST's file": this probe contradicts it.
  - Add a deployment test on PostgreSQL that greps the data directory after an erasure.

### 2. Blocker: on a table of about 50 pages or more, the erasure's VACUUM skips index cleanup, so the erased value stays as a live index entry

- **Inventory items.** F1 and F3 (indexes), and S1 for a `unique`/`indexed` personal attribute.
- **What fails.** `vacuum_erased` runs a plain `VACUUM t1, t2, …` (`store.rs:2964-2971`). Its default `INDEX_CLEANUP AUTO` skips index vacuuming when under 2% of the table's pages have dead items.
- **Evidence (observed, `p1large`, 6,000 rows, `t_candidate` at 109 pages).** After the erasure, `bt_page_items` showed the email as a live entry (`dead=f`) of `t_candidate_email_key_ix`, pointing at heap TID (53,35), whose line pointer is `LP_DEAD` (flags 3, length 0). `VACUUM (VERBOSE) t_candidate` printed `index scan bypassed: 1 pages from table (0.92% of total) have 1 dead item identifiers`. `VACUUM (INDEX_CLEANUP ON)` then removed the entry (`index scan needed … 1 dead item identifiers removed`), but the bytes stayed in that index page's free space (`page 2 offset 961: free space`), which is finding 1 again.
- **Consequence.** The same as finding 1, for any personal attribute with `unique`/`indexed` (the `an_erasure_leaves_no_erased_bytes…` fixture's `email`) on a type of a few thousand objects. Autovacuum uses the same bypass rule (inferred), so the entry can stay with no bound.
- **Suggested fix.** `VACUUM (INDEX_CLEANUP ON)` limited to the tables the erasure wrote, and finding 1's rewrite in the housekeeping. Test it at 50 pages or more.

### 3. Blocker: the planner's statistics keep erased personal values, and `pg_stats` answers them in SQL

- **Inventory item.** None in the SQLite inventory. SQLite keeps no value samples: the store runs no `ANALYZE` (grep of `core/`).
- **Evidence (observed, `p18_stats.py`).** 300 candidates plus one target, then 70 s, after which autovacuum's auto-analyze had run with default settings (`t_candidate:0/1`). `pg_stats` then held the target's name and email in `t_candidate.name`/`.email`'s `histogram_bounds`. After the erasure and the service's VACUUM they were still there (`…,"Other 0297",ZZ-MARKER-7f3a-S-NAME}`), while `t_candidate` held 0 rows with them. `pg_statistic`'s file (2619) held the name.
- **Consequence.** The store's own role, which owns the tables, and any superuser can read erased values by SQL. They last until the next `ANALYZE` of the table, which by default comes after 50 changes plus 10% of the table, so it is unbounded for a large, stable type (inferred). This breaks P's first bullet in normal operation: autoanalyze is on by default.
- **Suggested fix.** At table creation, `ALTER TABLE … ALTER COLUMN … SET STATISTICS 0` on every column that may hold a personal value or an identity:
  - the personal and identity columns of each type, and set tables' `value`;
  - `of_event.payload`, `actor_id` and `actor_principal`;
  - `of_attempt.request_values`, `consulted` and `actor_id`;
  - `t_proposal.inputs`;
  - `of_interval.value` and `entered_by`;
  - `of_external_id.value`;
  - `of_sequence.scope_key`;
  - `of_idempotency.actor_id`;
  - `of_declaration.published_by`;
  - `of_admission.reason`;
  - `of_legacy_entry`'s fields;
  - the expression index `lower(login)` (`ALTER INDEX … ALTER COLUMN 1 SET STATISTICS 0`).

  It must be set before the first ANALYZE. I observed that setting it later and then running `ANALYZE` *keeps* the old `name` row (`name|t|t`), while a column re-analyzed normally dropped the value (`email|t|f`). Decision 6 says the first deployment starts empty, so setting it at creation is enough for 1.0.0.

### 4. Blocker: `maintain`'s `rebuild_indexes` fails with a 500 on PostgreSQL, and once fixed it would still skip 28 indexes

- **Inventory items.** F3 and G5. This is P's second bullet, whose instrument is decision 9.5.
- **Evidence (observed, `p17_rebuild.py`).** `POST /v1/maintain {"task":{"name":"rebuild_indexes"}}` answered `500 {"fault": "the store: 42601: syntax error at or near \"\"of_admission_by_object\"\" (in \`REINDEX \"of_admission_by_object\"\`)", "code": "internal"}`, also in the service's log. PostgreSQL needs `REINDEX INDEX`. The cause is `store.rs:3273`, `REINDEX {}`.
- **It would still skip indexes (read, then observed).** The task lists indexes from `objectflow_catalog`, which leaves out every index that backs a constraint (`db.rs:1058`; `store.rs:3260`). On `p16` that is 43 listed of 71, with the 28 distinct primary and unique keys skipped, among them `of_external_id_pkey`, `of_idempotency_pkey` and `of_sequence_pkey`. In `p19_external.py` I rebuilt all 40 listed indexes with `REINDEX INDEX` and `of_external_id_pkey` still held the erased badge, in free space. Heap free space is untouched in any case (finding 1).
- **Consequence.** `operate.md`'s daily housekeeping (`operate.md:422`, the script at `:443-447` with `curl -sf`) fails at this step every day on PostgreSQL. "None is left past a day" (`operate.md:485`) is false there. Index free-space copies after an erasure are normal (finding 1, `p16`), so P's second bullet is broken in normal operation.
- **Suggested fix.** `REINDEX INDEX` on PostgreSQL. Better, finding 1's per-table `VACUUM (FULL)`, which covers constraint indexes and heaps. Add a PostgreSQL deployment test of the task.

### 5. Major: the vacuum is assumed complete, never checked, and three kinds of holder are missed

- **Inventory items.** F1 and F2: the "documented cut" on PostgreSQL.
- **What fails (read).** `vacuum_erased` (`store.rs:2946-2992`) counts holders only as `pg_stat_activity` rows of `current_database()` whose `backend_xmin` is older than the erasure. It then sets `checkpoint_due = held > 0` whatever VACUUM achieved, and logs nothing either way.
- **Control: a read in the same database (observed, `p2_horizon.py` (A)).** A REPEATABLE READ session in the store's database is counted. The dead versions stayed while it was open and were gone after it ended and one more write.
- **Missed: a hot standby with `hot_standby_feedback = on` (observed, `p2_horizon.py` (B)).** The standby was built inside my container with a slot. With a snapshot open on it, the walsender showed `datname` and `backend_xmin` as `(null)|(null)`, the slot showed `xmin 6999`, and the store's own count query answered `0`. After the erasure, whole dead versions (not free space) stayed: `t_candidate` 1 dead tuple and `of_event` 3. After the standby's read ended and five more writes, they were still dead tuples, `vacuum_count 3`, `n_dead_tup 1`. My manual `VACUUM` then removed them (`tuples: 1 removed`), so they had been removable and the service had stopped trying.
- **Missed: an inactive slot keeping that xmin (observed, `p4_slot.py`).** With the standby stopped (slot `active=f`, `xmin 7009`), a new erasure's dead versions stayed (`t_candidate` 1, `of_event` 3) and the service said nothing.
- **Missed: a newer reader pinning the page (observed, `p23_pin.py`).** The retry ran while a cursor opened after the erasure pinned the page. VACUUM could not prune it (`tuples missed: 1 dead from 1 pages not removed due to cleanup lock contention` from `VACUUM (VERBOSE)`). The store counted no holder, so it never tried again, and after the cursor closed and three writes the dead version, with the full name, was still there. The service's own reads pin pages the same way while they scan (inferred).
- **Missed: prepared transactions.** Inferred, not probed.
- **Consequence.** Decision 4's failover standby with feedback on, or a reader racing the retry, leaves whole erased rows as dead versions in the primary's files, and the service reports nothing. They last until autovacuum's thresholds trigger, which for a large table with few changes is unbounded.
- **Suggested fix.**
  - After VACUUM, check `pg_stat_user_tables.n_dead_tup` of the tables the erasure wrote, which counted the left-over version here, and keep the vacuum due while it is above 0.
  - Count `pg_replication_slots.xmin`, walsenders' `backend_xmin` and `pg_prepared_xacts` among the holders.
  - Retry on a timer. On PostgreSQL `cut_back` returns at once, since there is no store path (`objectflow-service/src/main.rs:5340`).
  - Log, and expose a metric, when an erasure's vacuum stays due.

### 6. Major (G): `operate.md`'s PostgreSQL limit misstates where erased values stay and for how long

- **Inventory items.** F2, F4 and G4, against decision 8's requirement that the guide state the retention a deployment must bound.
- **"Until its segments are recycled" is not the bound (observed).** A recycled segment keeps its old content under a new name until new WAL overwrites it:
  - On `p1large` the target's insert, made before a `CHECKPOINT`, was found only in segment file `…06`, a recycled future file, while the current segment was `…04`.
  - After three `pg_switch_wal()` and `CHECKPOINT` cycles, the erasure's copies (name ×7, email ×5) were still in the renamed current file `…07`.
  - Copies from my earlier runs were gone only after the large run had written enough WAL. The bound is the volume of WAL written afterwards (`min_wal_size` 80 MB up to `max_wal_size` 1 GB here), not time.
- **A replication slot keeps WAL with no bound (observed, `p4_slot.py`).** With an inactive slot, the segment holding the erasure (`…09`) was kept through 8 switches and checkpoints (`wal_status reserved`, 144 MB in 9 segments). Slots are not mentioned.
- **"A log archive … taken before the erasure" misses the erasure's own WAL (observed, `p13_walrange.py`).** The 486,032 bytes of WAL written by the erasure, its VACUUM and its CHECKPOINT held the name ×4 and the note ×5. So the archive of the erasure's own segment, completed after it, holds them, and so does a replica's stream.
- **"A backup taken before" misses physical backups taken after (observed).** A `pg_basebackup` taken after the erasure and 2,000 more writes held the name ×2 (`base/17775/17799`). A `pg_dump` taken then held 0, in both custom and plain format. `operate.md` recommends the provider's snapshots and PITR first.
- **Consequence.** A deployment that bounds only "backups and archives taken before the erasure" keeps personal data in snapshots, base backups and archives taken after it. That is a guide procedure that leaves personal data, which ADR-0204 §4 grades major.
- **Suggested fix.** Rewrite the PostgreSQL limit to say:
  - live WAL keeps erased values until about `max_wal_size` of later WAL has overwritten its recycled files;
  - a replication slot holds them until it is consumed, so set `max_slot_wal_keep_size`;
  - the erasure's own WAL carries them, so every WAL archive up to and including the erasure's segment, and every physical backup or snapshot taken before the next housekeeping run, must have bounded retention;
  - logical dumps taken after the erasure hold none.

### 7. Major (A, P): a database whose `LC_CTYPE` is not C, or that uses the ICU provider, is accepted, and an erasure then renames another person's identity

- **Inventory items.** S22 (case-folded rename), A1 and A2.
- **What fails (read).** The start checks only `datcollate` (`db.rs:1597-1604`), and its refusal tells the DBA to use `LC_COLLATE 'C'` and nothing else. Every folded identity match uses SQL `lower()`: the rename at `store.rs:6513`, `6579`, `6585` and `6595-6596`, and `read_ids_folded` at `store.rs:5366`. `lower()` follows the database's ctype, while the engine folds ASCII only.
- **Evidence (observed, `p8_ctype.py`).** A = `zz-marker-ä@synthetic.invalid`, B = `zz-marker-Ä@synthetic.invalid`. The engine treats them as two identities: both `add`s succeeded on every database. I wrote a memo as each, handed A's memo to B, then erased A:
  - `LC_CTYPE 'C'`: B's login and copies were kept (`{"login": "zz-marker-Ä@synthetic.invalid"}`). This holds.
  - `LC_COLLATE 'C' LC_CTYPE 'en_US.utf8'`, accepted at start: B's own login became `person-2` (A's erased object id), B's memo's author became `person-2`, and the owner of A's memo, which was B, became `person-2`.
  - `LOCALE_PROVIDER icu ICU_LOCALE 'en-US' LC_COLLATE 'C' LC_CTYPE 'C'`, accepted at start: the same.
- **Consequence.** Erasing one person rewrites another, unerased person's identity: B can no longer be named by it, and B's record is attributed to the erased A. That breaks A ("an identity names one actor") and P. It needs identities that differ only in the case of a non-ASCII letter, which is unusual but legitimate; `operate.md:485` itself mentions Unicode identities. Following the service's own refusal message on a server whose default ctype is not C leads there.
- **Suggested fix.**
  - Refuse at start unless `datctype` is `C` or `POSIX` and `datlocprovider = 'c'`, and name `LC_CTYPE` in the message.
  - Or fold with `lower(x COLLATE "C")` in every comparison and fold index. I observed that it is ASCII-only on both databases (`zz-Ä` stays `zz-Ä`).

### 8. Minor (G): statement logging writes personal values into the server's log, and `operate.md` does not say so

- **Inventory item.** F6, extended to the server's log.
- **Evidence (observed, `p5_log.py`).** With `log_min_duration_statement = 0`, one create and one erasure gave 10 lines such as `DETAIL: parameters: $1 = 'zz-marker-7f3a-d@synthetic.invalid'`, and also a whole row's parameters for the `of_event` insert.
- **The default configuration holds (observed).** Across every other run, the server log held 0 markers. Errors are logged as `CONTEXT: unnamed portal parameter $19` and a `STATEMENT` with placeholders, without values (`log_parameter_max_length_on_error = 0`). I found no default path where a DETAIL quotes a value: attribute uniqueness is the engine's (the type tables have no unique index), and a required personal attribute is refused at submit (`p14_required.py`: `dry_run_clean`).
- **Consequence.** A deployment that turns on `log_statement`, `log_min_duration_statement`, `auto_explain` or pgaudit keeps every personal value in its logs, out of the erasure's reach.
- **Suggested fix.** Add to `operate.md`'s PostgreSQL limits: keep statement and parameter logging off on the store's database, or bound the logs' retention as a backup's.

### 9. Minor: on an idle PostgreSQL store, a deferred vacuum waits for the next write

- **Evidence (observed, `p15_idle.py`).** A read older than the erasure was opened, then ended. After 30 s with no write, the dead versions (`t_candidate` 1, `of_event` 3) were still there. One write cleared them.
- **Compared with SQLite.** Its cut retries every 5 s (`main.rs:5235`). `operate.md:93` documents "after the next write", so it holds as documented, but nothing bounds how long an idle store waits.
- **Suggested fix.** The timer from finding 5.

## Items that hold on PostgreSQL

- **S1 to S25 at the SQL level (observed).**
  - `p16_identity.py`: 9 tables held the identity before the erasure, `{}` after; the memo read `owner: person-2`, `watchers: ["person-2"]`.
  - `p1_*`: `get` answered nulls and no row of `of_event` or `t_candidate` matched a marker.
  - This covers sets (S16), proposals (S18), refused inputs (S21), other ASCII cases (S22), the approver (S23) and the actor columns (S24).
- **S11, an external identifier, at the SQL level (observed).** `p19`: the row was deleted. Its bytes are covered by findings 1 and 4.
- **TOAST (observed).** `p1small`: the 4 KB value was in no TOAST file after the VACUUM.
- **F5, restore and ask again (observed, `p20_restore.py`).** `pg_dump --format=custom`, then `pg_restore`, then `operate.md`'s `INSERT INTO of_counter … 'copied'` mark: the restore brought the value back, and the erasure asked again left 0 rows.
- **Logical backups (observed).** A `pg_dump` taken after the erasure held 0 copies.
- **F6, the service's log (observed).** 18 service logs held no marker. Its faults quote the SQL (`with_sql`, `db.rs:124`), whose values are always bound, and the one fault seen named no value.
- **F7, `/health` and `/metrics` (observed).** No marker, no password, no connection string.
- **`pg_stat_statements` (observed).** 887 statements, 0 with a marker; `pgss_query_texts.stat` held 0.
- **`pg_stat_activity` (inferred).** It shows the same parameterised texts.
- **The password (observed).**
  - `postgresql://…:<pw>@…` and `password=<pw>` are both refused with the password unechoed.
  - A malformed string holding one answers `--store: invalid connection string`, without the password.
  - A wrong password answers `28P01: password authentication failed for user "objectflow"`.
  - The start line reads `the store PostgreSQL database p2 on 127.0.0.1`, built by `PgConfig::describe`/`Debug` at `db.rs:1398-1420`.
- **`pg_stat_activity` visibility for the store's role (observed).** A role that is neither superuser nor the session's owner saw another role's `backend_xmin` (`bi|7009|`), so the holder count works for `operate.md`'s role.
- **The P5 opt-in, ADR-0206 (observed, `p11_reads.py`).** `name >= …`, `order: [name]` and an agent's `email == …` were each refused `personal` without `reads_personal`; with it, the query was answered.
- **Sandbox schemas (observed).** A deployment service made none: 0 `sandbox%` schemas in all 18 databases.
- **U+0000 in a personal input (observed, `p21_nul.py`).** Refused `invalid_input` / `malformed` before reaching the store.
- **A1 and A2 under a C ctype (observed).** `p8c` and `p16`: the erased actor is named by its object id, and other identities are kept.

## Not checked, and why

- **Guards' consulted keys (S19, S20) on PostgreSQL.** They need an evaluator endpoint; I did not build one. Their SQL is the `strpos(lower(json_extract(…)))` at `store.rs:1541` and `1559`, so finding 7 applies to them too (inferred).
- **Whether the core suite passes on PostgreSQL.** A gate run was in progress, and I did not run cargo.
- **Prepared transactions as a horizon holder.** Inferred only.
- **The `pg_checkpoint`-less path.** My role was a superuser. It does not matter given finding 1.
- **The service's reads pinning pages during a retry.** Shown only with a psql cursor (`p23`).
- **A standby's own data files.** Not checked after it had replayed.
- **What replication from a standby or logical CDC carries.** Not checked beyond the slot's WAL retention.
- **Filesystem blocks of files that `REINDEX` or `VACUUM FULL` unlink.** Not checked, as on SQLite.
- **R12 to R21 (export and the reads' inferences).** Not run again: they are the engine's code, the same on both backends.

## Seen outside my area

- **Ordering (V, D).** The ICU-provider database passes the `datcollate = 'C'` check, but text there does not sort by bytes (`'a' < 'B'` answered `t` on `p8i`), which breaks ADR-0213's byte-order assumption.
- **A 500 from a client's input.** A `context` string or an idempotency key holding U+0000 is answered `500 internal` (`22021: invalid byte sequence for encoding "UTF8": 0x00`, `p22_nul_context.py`), where SQLite stores it.
- **Payload text after an erasure.** The rewritten event payload comes back in jsonb's text form (`{"wrote": {"name": null}, "inputs": …}`, with spaces and keys reordered) rather than the engine's compact form. I did not check whether anything compares or hashes payload text.
- **Shared test servers (read).** `drop_orphaned_sandboxes` (`db.rs:1334-1351`) drops `sandbox_<pid>_…` schemas whose pid does not exist on *this* machine, so a sandbox started on one host can drop a live sandbox of another host that shares the server.
- **Cost (C).** Each erasure VACUUMs every table of the store, and on this small store wrote 486 KB of WAL (`p13`). I did not measure it at the profile.

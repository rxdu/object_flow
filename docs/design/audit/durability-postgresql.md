<!-- The audit's area `durability.md` taken again on PostgreSQL for ADR-0213's block 6, 2026-10-17, by a reviewer agent on the release build of `952c28c`'s tree. Its probe scripts and their outputs were in the session's scratchpad and were not kept; each finding names its probe, the method and what it printed. Findings and their fixes are tracked in TODO.md, PostgreSQL in 1.0.0, block 7. -->

# Durability, recovery, upgrade, events and release on PostgreSQL: the audit's area 5 re-run

Re-run of `docs/design/audit/durability.md` (taken on SQLite at 698034b) against ADR-0213's PostgreSQL store, 2026-10-17.

**What was tested.** The release binary `core/target/release/objectflow-service` (`objectflow-service 1.0.0, serving stores of objectflow-core 1.0.0`, built 20:55 from the working tree that became commit `952c28c`; the edits to `db.rs` and `store.rs` after the build are a rustfmt reflow and do not touch the paths below). Line numbers are HEAD `952c28c`. Every probe ran against a PostgreSQL 16.8 container of my own (`review-durability-pg`, 127.0.0.1:55440, C collation), plus two more for single probes (`review-durability-pg-full` with its data on a 128 MiB tmpfs, and `review-durability-svc` to run the service on a network of its own). All three are removed, with every database they held. Nothing was created on `objectflow-pg16`, and `objectflow-pg16-profile` was not touched. Data is synthetic: the shipped reimbursement flow, with made-up employees and claims.

**Where the evidence is.** Each probe is a script in `scratchpad/rd/` (`pN_*.py`, helpers in `ofc.py`), and its output is beside it (`pN.out`). Run one as `cd scratchpad/rd && python3 pN_….py`. "Observed" means the output shows it. "Inferred" means it is read from code or from PostgreSQL's documented behaviour and was not reproduced. I ran no cargo.

**Grading.** The grades follow ADR-0204 §4, and the promise IDs follow the audit's: D1 to D5, E1 and E2, P1, G and C.

## Findings, most severe first

### 1. A lost database connection is never reopened, so writes fail until a person restarts the service (major)

**Inventory items.** New on PostgreSQL. It touches W1/W16 (recovery), S1 and C6, and ADR-0213 decision 4.

**What fails.** The writer's connection, which holds the advisory lock, is opened once at start. It is reopened only after a panic: `reopened` at `main.rs:1956-1990` is called only from `main.rs:1873`, `:1914` and `:1937`. If the connection drops for any other reason, every later write answers `500 internal` and keeps doing so, even after the database is back. The process stays up and keeps pinging the systemd watchdog (`main.rs:6563-6570`), so `Restart=on-failure` (`operate.md:121`) never acts. Reads recover, because each read connection is new.

**Evidence (observed).**
- `p3_terminate.py`: `pg_terminate_backend` of the lock holder while 4 keyed writers ran. Result: `80 x (500, 'the store: connection closed (in BEGIN ISOLATION LEVEL SERIALIZABLE)', "code": "internal")`. 24 s later, 0 of the last 80 writes were satisfied. `/health` answered `503 writes_failing … the last 480 writes failed`, and `get` still answered 200. The advisory lock count dropped to 0, so a second instance started while the first still ran and wrote `claim-419`.
- `p4_dbrestart.py`: `docker restart` of the database (down about 2.2 s). Afterwards: `300 x (500, … connection closed …)` and 0 writes satisfied. Reads were back within 3 s (`12 x 200`).
- `p18`, in `p18.out`: `ALTER DATABASE … SET idle_session_timeout = '3s'`, then 6 s idle. Result: `57P05: terminating connection due to idle-session timeout`, then `connection closed` on every write, and `/health 503 writes_failing`. The same happens whenever a proxy, NAT or firewall drops an idle connection.
- `p13_full.py`, full volume: after space was freed and PostgreSQL restarted, `writes after space was freed: {(500, 'internal', '… connection closed …'): 10}`.

**What still holds.** Every one of these probes kept atomicity and exactly-once behaviour. After a service restart:
- p3: 484 failed keys, 0 had committed, each applied once, no title twice, events `906|906|906`.
- p4: 361 acknowledged, 0 missing, 344 failed keys each applied once.
- p13: 221 of 221 acknowledged writes present.

**Consequence.** Routine database events stop every write until someone runs `systemctl restart`: a minor-version restart, a provider's maintenance window, a failover, an idle-connection reaper or a connection reset. Clients are told the fault is the service's own (`500 internal`), not something to retry. `operate.md:84` asks for a restart only "after a failover".

**Grade.** Major. C ("every request is answered") breaks in a legitimate sequence, and it breaks ADR-0213 decision 4's availability model. Its triggers are routine for a server database.

**Fix.**
1. When a writer statement fails with a connection-level fault (SQLSTATE class 08, 57P01-57P05, or the client's "connection closed"), drop the connection. Then reconnect with backoff, re-take the advisory lock, and refuse writes `503 store_unavailable` until the lock is held again. Never write on a connection that does not hold the lock.
2. Alternatively, exit non-zero and let systemd restart the service, with a `RestartSec=` above the time a stale lock takes to go (finding 4).
3. Map those faults to 503 rather than `internal` in `From<StoreError> for Fault` (`main.rs:1511-1526`).
4. Add a test to the deployment suite that terminates the writer's backend and restarts the database.

### 2. A copy that is not marked is served as the live store: ids, positions, claim numbers and cursors are reused (major)

**Inventory items.** B4, B5, V6, and ADR-0207 decisions 1 and 4 on PostgreSQL.

**What fails.** ADR-0207 decision 1 makes the backup carry the mark, so a copy knows it is a copy. It rejected "an act at restore" as "the act most likely to be forgotten". On PostgreSQL no backup can carry the mark, because the live store never holds it: provider snapshots, point-in-time recovery and `pg_dump` all copy a database without it. `operate.md:86-91` instead asks for an `INSERT` into `of_counter` at restore time, which is exactly the rejected act. If that act is forgotten, B4 and B5 come back.

**Evidence (observed).**
- `p7_backup.py`: pg_dump ran while writes continued, then 40 more writes, then the dump was restored twice. The marked copy works: `ids handed out again: 0 of 40; positions again: 0; claim numbers again: 0`, the kept cursor 250 is refused `invalid_cursor` by pull, export and acknowledge, and a second start adds no second restore.
- `p7b_compare.py`, the unmarked copy: `lost period: 121 ids, 121 positions, 121 numbers; handed out again after the restart: ids 41/41 (e.g. ['claim-127', 'claim-128']), positions 41/41, numbers 41/41 (e.g. ['EXP-00118', 'EXP-00119'])`.
- `p7c_cursor.py`, same unmarked copy: once its log passed the kept cursor, `pull with the kept cursor 250: 200 refused=None delivered 20 of the 100 Claim events written since the restore`, and `export log` the same 20. So 80 events were passed over.

**The start is silent too (observed).** On PostgreSQL the start says nothing of a restore it did. p7b's marked copy logged only `listening` and `the store … at version 1`, although `of_restore` recorded `2|2041|4000|2041|4000|1000`. The PostgreSQL start arm (`main.rs:6395-6430`) has no `restore_said`, which the file arm has (`main.rs:6354-6355`). So `operate.md:378`'s "the start says so in one line … Check that line against the highest ids" cannot be followed on PostgreSQL.

**Inferred: a failover to an asynchronous standby.** ADR-0213 decision 4 names failover as the availability mechanism, and `operate.md:84` says "after a failover, start the service again against the new primary". PostgreSQL's streaming replication is asynchronous by default. A promoted standby can lack the last acknowledged commits (a D1 loss), and it is an unmarked copy, so it hands out those ids again. Nothing in `operate.md` or ADR-0213 asks for synchronous replication (`grep -i synchronous` found only the client crate's "synchronous").

**Consequence.** Integrations and accounting systems receive the same object ids and claim numbers for different objects (bill references `EXP-…` twice). Subscribers silently skip events.

**Grade.** Major. D5, E1 and E2 break in legitimate sequences: a provider restore done from its console, or a failover. This matches ADR-0207's own calibration of B4 and B5. For an asynchronous failover, the guide's procedure followed as written loses acknowledged writes (inferred).

**Fix.**
1. Make the PostgreSQL restore not depend on memory. At create, record an identity of the database in the store (for example `pg_control_system().system_identifier` and the database oid). At each start, refuse a database that does not match unless it is marked, or unless `--restored` is given. Also refuse after a timeline change (`pg_control_checkpoint().timeline_id`) that no `--failover-ok` acknowledges. Without detection, at least make the restore's `INSERT` part of a documented restore command block.
2. Say the restore at start, as the file arm does.
3. Have `operate.md` require synchronous replication (`synchronous_commit = on` with `synchronous_standby_names`) for a failover-based deployment, or treat a promotion as a restore.

### 3. A database stall hangs every request and /health, ignoring the read deadline (major)

**Inventory items.** New on PostgreSQL. It touches C (reads' deadline) and the S class.

**What fails.** No client timeout of any kind is set. `grep -n "connect_timeout\|keepalive\|statement_timeout\|tcp_user_timeout"` over `db.rs`, `store.rs` and `main.rs` finds nothing. The read deadline's interrupt is a cancel request (`db.rs:1824-1837`), which a stalled server never acts on, and a connection that is still being opened has no cancel handle at all.

**Evidence (observed).**
- `p5_stall.py 40`: `docker pause` of the database for 40 s, under 2 writers, a reader and a `/health` poller, with `--read-timeout 5 s`. During the stall, nothing sent after the pause was answered except one `/health`. After resuming, `280 x write 200 … longest took 40.1 s`, `50 x read 200 … longest took 40.0 s` and `16 x health 200 … longest took 39.1 s`. No fault was logged.
- `p5b`: `query`, `export` and `get`, each sent into a 20 s stall with `--read-timeout 5 s`, took `20.1 s` each and answered 200.

**After the stall.** The service recovered once the connection itself survived the stall.

**Inferred: a real partition.** Under a network partition, packets on established connections are dropped. Such a hang lasts until Linux's TCP retransmission gives up (about 15 min at `tcp_retries2=15`), and then turns into finding 1.

**Consequence.** One stalled or unreachable database freezes the whole service, its health endpoint included. The writer mutex is held for the whole stall.

**Grade.** Major (a hang within the connection cap).

**Fix.**
- Give the client `connect_timeout`, keepalives and `tcp_user_timeout`.
- Bound read connections with `SET statement_timeout` at the read deadline, so the server ends the statement even when a cancel is lost.
- Give the writer a `statement_timeout` above the longest legitimate write, or at least a client-side socket timeout.
- Let `/health` answer from a cached view, or with its own short timeout, rather than wait on the database.

### 4. After the service's host vanishes, its stale session keeps the store's lock for about 2 hours (major)

**Inventory items.** Second-instance refusal and restart after a crash, W17 and ADR-0213 decision 4.

**What fails.** The lock is `pg_try_advisory_lock(hashtext('objectflow ' || current_schema()))`, held at session level (`store.rs:2692-2702`). If the client host disappears without closing its socket (power loss, kernel panic, partition), the backend stays idle and keeps the lock. It is freed only when the server's TCP keepalive gives up. The server's `tcp_keepalives_idle` is 0, which means the OS defaults: 7200 s + 9 × 75 s, about 2 h 11 min. `operate.md:84` says "the lock goes with a crashed service's connection" and gives no remedy.

**Evidence.**
- `p15_replace.py` (observed): the service ran in a container of its own and held the lock (`2158|172.21.0.3|idle`), then its network was disconnected. A replacement started from the host was refused at 0, 30 and 90 s: `another service holds the store …; lock held by: 2158|172.21.0.3|idle|00:01:42.569736`.
- After `pg_terminate_backend` (observed): `replacement … listening True`.
- Server settings (observed): `SHOW tcp_keepalives_idle` gave `0`, and `/proc/sys/net/ipv4/tcp_keepalive_time` gave `7200`.
- A clean `kill -9` frees the lock in 0.08 s when the backend is idle (`p1.out`).
- `p19b` (observed): a kill in the middle of a statement leaves the lock with the backend until that statement ends. An immediate `--create` was refused `another service holds the store`, and 2 s later it was taken.
- Inferred: with systemd's default `RestartSec=100ms` and its start limit (5 starts in 10 s, `operate.md:142`), a kill during a long statement can leave the unit failed. Examples are an erasure's VACUUM of every table, a large publish rebuild, or an import batch.

**Consequence.** No service can write the store for about two hours after a host loss, unless a DBA finds and terminates the stale backend.

**Grade.** Major (availability in a legitimate sequence).

**Fix.**
1. On the writer's session, set `SET tcp_keepalives_idle = 10, tcp_keepalives_interval = 5, tcp_keepalives_count = 3, tcp_user_timeout = 30000`. These are per-session settings; I confirmed all four are accepted: `10|5|3|20000`.
2. Give operate.md the remedy (`SELECT pg_terminate_backend(pid) FROM pg_locks WHERE locktype = 'advisory' …`).
3. Give the unit a `RestartSec=` of a few seconds.

### 5. An erasure's pending vacuum is forgotten after a kill (W6b on PostgreSQL, major, area 1's P)

**What fails.** `open_postgres` passes `frames = false` to `opened` (`store.rs:2703`). `opened` then sets `checkpoint_due = frames` (`store.rs:2823`). So a vacuum still due when the service is killed is never run again.

**Evidence (observed).**
- `p6_erase.py kill`: an outside REPEATABLE READ snapshot was open, an employee's synthetic name was erased (`200 satisfied`, read back `null`), the service was killed with `-9`, then the snapshot ended, the service restarted and three writes were made. Result: `files holding the name after the read ended and three writes: 1`, and 70 s later still `1`. The file is `base/19833/20348 -> t_employee`, with `n_dead_tup` 1.
- `p6_erase.py control`, the same run without the kill: `0` after the next write.

**Grade.** Major (P1, an unusual sequence), as the original W6b.

**Fix.** On PostgreSQL, run `vacuum_erased` at open whenever the store holds an erasure event after the last recorded vacuum, or record "vacuum due" in `of_counter` inside the erasure's transaction and clear it when the vacuum completes.

### 6. Upgrades do not run on PostgreSQL, so every PostgreSQL store is stranded at the first release after 1.0.0 (major then; minor (G) in 1.0.0)

**Inventory items.** U1, U3, U4 and U8.

**What fails.**
- A store whose release table names a release this core upgrades from is refused at start (`store.rs:2766-2774`) instead of being served `upgrade_due`.
- `/v1/upgrade` on PostgreSQL always answers that nothing is due (`main.rs:4262-4274`).
- `release::upgrade` takes a file path (`release.rs:217-224`) and opens it as SQLite (`release.rs:239`).
- `operate.md:95` nevertheless says "a later release's upgrade runs on it as on a file". Its steps at `:393-398` fail at step 3 on PostgreSQL.

**Evidence (observed).** `p8_upgrade.py`:
- The upgrade route answered `{"from": "objectflow-core 1.0.0", "to": "objectflow-core 1.0.0", "steps": [] …}`.
- With the release table's last row set to `objectflow-core 0.2.0`, the start ended with `listening False exit 1 … the store: engine_mismatch: … is at objectflow-core 0.2.0, and this core is objectflow-core 1.0.0`.

**Consequence.** Nothing breaks in 1.0.0 itself: every PostgreSQL store is made at 1.0.0 (ADR-0213 decision 6). But the first release that adds a `RELEASES` entry refuses to start on every PostgreSQL store, which breaks `operate.md`'s "A store is upgraded, never stranded".

**Grade.** D5 major as a gate item for any release after 1.0.0. G minor now.

**Fix.**
1. Make `release::upgrade` take a `Connection` (or a store locator) and run the same steps over the facade.
2. Let the PostgreSQL start serve `upgrade_due` as the file arm does.
3. Add a PostgreSQL fixture at 1.0.0 to the alike test. Until then, correct `operate.md:95`.

### 7. A lock wait past the lock timeout answers `500 internal`, not `stale` (W8b on PostgreSQL), and a publish fails during a backup (minor)

**What fails.**
- On PostgreSQL a lock is waited for at a statement, not at `BEGIN`, which never blocks. `begin_write!` retries only `BEGIN` (`store.rs:114-148`).
- `db::Error::is_busy` knows 55P03, 40001 and 40P01 (`db.rs:142-154`). But `From<db::Error> for StoreError` turns them into plain text (`store.rs:316-324`), and `StoreError::is_busy` checks only for the prefix `busy`.
- A publish that alters or rebuilds a table needs that table exclusively, so it fails whenever a read transaction has held the table for 5 s. That includes `pg_dump`, the backup `operate.md` recommends, for its whole run, and any service read for up to `--read-timeout`. While the publish waits, that table's reads queue behind it.

**Evidence (observed).** `p9_locks.py`:
- (a) Acknowledge under another session's row lock: `500 {"fault": "the store: 55P03: canceling statement due to lock timeout", "code": "internal"} after 5.0 s`.
- (b) A request under a table lock: the same `500` after 5.0 s, with `/health 200`.
- (c) A publish adding an attribute while a read held `t_claim`: `500 … 55P03 … (in INSERT INTO t_claim__rebuild …) after 5.1 s`. Claim reads meanwhile were slowest at `4.88 s`, the version stayed at 1 (atomic), and the same publish succeeded once the read ended.

**Grade.** Minor. G: a message that misleads. No data is lost, and the publish can be retried.

**Fix.**
1. Keep the busy classification through `StoreError`, for example `StoreError(format!("{BUSY}: {e}"))` when `e.is_busy()`, and retry the whole transaction as on SQLite.
2. Have a publish take its table locks with `lock_timeout` and retry, answering `stale`.
3. Say in operate.md that a publish waits for a running backup.

### 8. `synchronous_commit` is not pinned, so a database set to `off` loses acknowledged writes in a crash (W2 on PostgreSQL, minor)

**What fails.** The store's `PRAGMA synchronous = FULL` becomes nothing on PostgreSQL: a pragma containing `=` translates to `""` (`db.rs:1138-1139`). No `SET synchronous_commit` exists anywhere (grep).

**Evidence (observed).** `p11_synccommit.py`: the database was set to `synchronous_commit = off`. The start said nothing about it, 30 claims were acknowledged, and then a backend was `kill -9`'d (the server reset and recovered). Result: `after the server's recovery: 29 of 30 acknowledged claims present`.

**Grade.** Minor. By §2's letter it is the operator's setting, but the service could make the property its own with one statement. W2's proposed check would pass vacuously on PostgreSQL.

**Fix.** Run `SET synchronous_commit = on` on the writer's connection. Warn at start when `SHOW fsync` or `full_page_writes` is `off`.

### 9. Every connection rewrites the store's catalog, so a refused start changes the database (C3 on PostgreSQL, minor)

**What fails.** Each connect runs `PG_CATALOG`, which is `CREATE OR REPLACE VIEW objectflow_catalog` and ten `CREATE OR REPLACE FUNCTION`s (`db.rs:1047`, run at `db.rs:1614`). It runs before the advisory lock is tried (`store.rs:2692`) and before any `lock_timeout` is set.

**Evidence.**
- `p16.out` (observed): a second service refused by the lock still rewrote the catalog rows. Their xmin went `9009 9009` to `9010 9010`.
- `p17.out` (observed): a start waited `14.2 s` behind an outside transaction that had read the view. The writer's erasures read it inside their transaction (`store.rs:1118-1120`).
- Concurrent replacement in PostgreSQL (observed): 72 parallel psql sessions running `PG_CATALOG` gave 8 failures, `ERROR: tuple concurrently updated`.
- Through the service (observed): 200 concurrent first reads over 5 fresh starts all answered 200 (`p10.out`), so a read failure from this was not reproduced through the service.

**Grade.** Minor (D3 by the letter; derived objects only, identical definitions).

**Fix.** Create the catalog once, at `--create` and at an upgrade, inside a transaction. At connect, only check that it exists. Set `lock_timeout` before any statement.

### 10. Guides and messages on PostgreSQL name SQLite's files, or are incomplete (minor, G)

- The stop's message on a lost connection, observed in `p3.out` and `p4.out`: `the write-ahead log was not carried into the store's file: the store: connection closed (in SELECT 0, 0, 0)` (`main.rs:6608`, and `PRAGMA wal_checkpoint` maps to `SELECT 0, 0, 0` at `db.rs:1132-1134`).
- `/health` blamed the full volume after the database was back, observed in `p13.out`: `503 ('disk_full', 'disk_full: the last write failed for a full volume, and none has committed since')`. The real cause was the dead connection of finding 1.
- `operate.md:380` says "The service runs SQLite's quick check at each start". The PostgreSQL section says everything else on the page holds, but on PostgreSQL `PRAGMA quick_check` is `SELECT 'ok'` (`db.rs:1130-1131`). The guide could recommend `initdb --data-checksums` or amcheck instead.
- The PostgreSQL backup section (`operate.md:86-91`) has no restore steps and no check of a copy (the B2 and B3 analogues). The missing steps are: a database with `LC_COLLATE 'C'`, then `pg_restore`, the mark, and `--store` pointed at it. A check would be, for example, `pg_restore --list` or a restore into a scratch database reading `of_engine_release`. Observed in `p7.out`: `pg_restore` into a C database worked (exit 0). A restore into an `en_US.utf8` database was refused at start: `a PostgreSQL database whose text is compared by its collation en_US.utf8 …`.

### 11. The release fingerprint does not cover what PostgreSQL holds (U6 on PostgreSQL, minor, inferred)

`release_fingerprint` hashes SQLite's `SCHEMA` text (`store.rs:185-188`). It does not hash `PG_SCHEMA` (`store.rs:1876-1877`), the translation `to_postgres` (`db.rs:1152`) or `PG_CATALOG`. A change to what a PostgreSQL store's tables are can therefore pass the gate with no `RELEASES` entry or migration. This is read, not reproduced.

**Fix.** Hash `to_postgres(SCHEMA)`, `PG_SCHEMA`, `PG_CATALOG` and the PostgreSQL DDL of the fingerprint flow.

### 12. The service's durability tests do not run on PostgreSQL (R, minor)

ADR-0213 decision 1 says the core's and the service's tests run on each backend.
- The deployment suite has 83 tests, and its `fresh_store` makes `store.sqlite` (`deployment.rs:178-183`, 75 uses). Exactly one test, `a_store_on_postgresql_is_served_made_and_kept` (`deployment.rs:6457`), runs on PostgreSQL, so no PostgreSQL test kills, restarts the database, restores or upgrades.
- The harness runs 1 seed on PostgreSQL against 6 on SQLite (`scripts/check-all:49-52`).
- `release.yml:59` tests on SQLite alone. It does require the commit's `check-all`, which includes the PostgreSQL gate (`check.yml:17-36`).

This gap is what let findings 1 to 4 through. Fix: parameterise `fresh_store` over the backend for the restart, stop, health, restore and kill tests, and add the probes of findings 1, 3 and 4.

## Items that hold on PostgreSQL

- **W1:** holds (`p2.out`). 4 kills under 6 keyed writers gave 2,279 acknowledged and 0 missing. 24 unanswered keys each applied once (3 had committed and replayed). Events `2606|1|2606|2606`, with 0 txn order breaks. Claim numbers ran `EXP-00001` to `EXP-01729` with 1,729 distinct values, and the sequence's next value was 1730.
- **W2:** see finding 8.
- **W3:** holds (`p12.out`). A kill at 0.05, 0.15 and 0.3 s into a 0.45 s rebuild publish left `version 1, memo column 0, change SUBMITTED`. A kill after it left `version 2, memo column 1, PUBLISHED`. A second rebuilding publish works (`p12b.out`).
- **W4 and W5:** not probed. They take the same `transact` path as W1 (inferred).
- **W6:** holds (`p20.out`). 60 keyed erasures over 4 kills: each erased once, 0 names left, 2 of 4 unanswered keys replayed, events `129|129|129`.
- **W6b:** see finding 5.
- **W7 and W8 under a kill:** not probed. They use the same `begin_write!` path (inferred). W8b is finding 7.
- **W9:** not a finding, as before.
- **W10:** no upgrade runs on PostgreSQL (finding 6).
- **W11:** holds (`p19.out`). A kill 20 to 200 ms into `--create` left 0 tables. The next start without `--create` said "holds no store; --create makes one", and `--create` made the store once the killed backend had ended (finding 4).
- **W12:** inferred to hold (transactional DDL, `IF NOT EXISTS`).
- **W13:** not applicable. `cut_back` returns at once without a file (`main.rs:5340`).
- **W14:** holds. SIGTERM gave "stopped", exit 0 (`p1.out`).
- **W15:** not applicable as such. A write waiting on a lock fails at `lock_timeout` 5 s (`p9.out`), well inside the 30 s grace.
- **W16:** holds by reading. `reopened` re-takes the lock (`main.rs:1956-1977`). Unconfirmed, since I had no way to trigger a panic.
- **W17:** a second service is refused (`p1.out`: "another service holds the store … one service writes a store"), as ADR-0213 decision 4 intends. The upgrade steps stop the service first (`operate.md:393`).
- **S1:** D1 holds (`p13.out`: 221 of 221 acknowledged present after restart), and the first failure answered `503 disk_full … xlogtemp … No space left on device (in COMMIT)`. PostgreSQL then panicked and stopped, and recovery needs a service restart (finding 1).
- **S2 to S5:** not applicable (no file under the service). The database analogues, a database dropped or restored in place while served, are operator acts outside §2 and were not probed.
- **S6:** holds (`p19.out`). A read-only database is refused at start, with and without a store: `25006: cannot execute CREATE VIEW in a read-only transaction`.
- **S7:** not applicable.
- **C1:** holds. PostgreSQL recovers itself, and restarts after a kill served every acknowledged write (p2).
- **C2:** not applicable.
- **C3:** see finding 9.
- **C4:** not applicable (finding 6, finding 10).
- **C5:** holds. Each PostgreSQL refusal names its cause: lock, no store, collation, read-only.
- **C6:** not applicable. The stop's message misleads on a dead connection (finding 10).
- **C7 and C8:** do not depend on the backend; not re-run.
- **B1:** holds (`p7.out`). `pg_dump` ran while 168 claims were written, and the copy was consistent: `129|129|129`.
- **B2 and B3:** see finding 10.
- **B4 and B5:** hold when the copy is marked (`p7.out`). Unmarked, they fail (finding 2).
- **B6:** does not depend on the backend.
- **B7:** area 1's.
- **U2:** holds (`p8.out`). With a later release in `of_engine_release`, the write, the read and `/health` each answered `503 engine_mismatch`.
- **U5:** unchanged.
- **U7:** holds by reading (`commit_id` is recorded). The local build carries none, because it was built without `OBJECTFLOW_COMMIT`.
- **U9:** holds by reading (`RELEASES` ends at 1.0.0).
- **U6:** see finding 11.
- **U1, U3, U4, U8:** see finding 6.
- **V1:** holds (`p2.out`). `pull delivered 2303; Claim events in log 2303; undelivered 0; in order True` after 4 kills.
- **V2:** holds (`p14.out`). `export log: 3 pages, 2606 lines of 2606 events, contiguous 1..n: True`.
- **V3:** holds. 0 events were out of their object's order.
- **V4:** holds. Cursors `-1` and `2607` were refused `invalid_cursor` by pull, export and acknowledge, and `abc` got `400` or `invalid_cursor`.
- **V5:** holds. `.attempt_counts` with `2999-01-01` was refused `invalid_cursor`.
- **V11 and V15:** hold. Pages were walked to `more: false`, and `.transitions` paged.
- **V7 to V10, V13, V14:** engine logic that does not depend on the backend. The core suite runs it on PostgreSQL in CI; not re-probed.
- **Gap-free across rolled-back and retried writes:** holds. The p3 and p4 stores after dropped connections and a database restart: positions `906|906` and `717|717`, claim numbers contiguous (`EXP-00001` to `00894` and `00705`, the sequence's next value 895 and 706), 0 txn order breaks.
- **R1 to R3:** hold (`check.yml:17-36` with a PostgreSQL 16.8 service, `check-all:48-55`, and `release.yml` requires the commit's `check-all`). R4 is not a finding. For R5, see findings 11 and 12.

## What I could not check, and why

- **Failover to an asynchronous standby, and point-in-time or provider-snapshot restores.** I set up no replica and had no provider. Inferred to be unmarked copies, with the mechanism observed in p7b (finding 2).
- **A real packet-dropping partition of established connections.** That needs root for iptables. The lock case was simulated by a docker network disconnect (p15) and the hang by `docker pause` (p5). The 15-minute retransmission bound is inferred.
- **systemd's start-limit behaviour against a lock held by a dying backend.** Inferred from p19b and the unit's defaults (finding 4).
- **TLS (`sslmode=require`) connections through a loss.** All probes used plain TCP on loopback or the docker network.
- **W16's panic path, and W4, W5, W7 and W8 under a kill.** There was no way to trigger a panic, and the others take the same commit path as W1 and W6, which were probed.
- **The concurrent-connect catalog failure through the service.** Not reproduced in 200 reads (finding 9).
- **The six-year store's VACUUM and rebuild durations,** which set how long a stale lock outlives a kill. I built no profile-sized store, and the cost areas own them.

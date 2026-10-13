# Operate a deployment

How to run ObjectFlow as a live service on one Linux machine: install it, start it under systemd, watch it, back it up, upgrade it and size it. For the model, read [Concepts](concepts.md). For your application's side, read [Build an application](integrate.md).

## What you are running

One process, `objectflow-service`, serving one **store**: a SQLite file and the files SQLite keeps beside it (`-wal`, `-shm`), plus `<store>.lock`, which the service holds while it runs. It writes one request at a time and reads beside its writes. A second service started on the same store says so at start and shares it, as an upgrade's overlap needs; stop one otherwise.

**It authenticates nobody.** Whoever can reach it can act as any actor. Put it behind the application that authenticates your users: on a Unix socket only your application can open, or on loopback. Never put it on a network your users reach.

**Requirements:** Linux on x86-64 with the glibc of Ubuntu 22.04 or later (`ldd --version`), and disk for the store, its `-wal` (up to 64 MiB in steady state) and your backups.

## Install

Build the release binary from source (rustup installs the pinned toolchain on the first build):

```sh
(cd core && cargo build --release -p objectflow-service)
core/target/release/objectflow-service --version
```

Tagged releases are published on the repository's GitHub Releases page as an archive with its SHA-256 checksum, built on Ubuntu 22.04. Check the checksum before installing. Releases are not signed yet. `--version` names the commit a release was built from.

## Create the store

Make the store once, with `--create`:

```sh
objectflow-service --store /var/lib/objectflow/store.sqlite --create
```

Leave `--create` out of every later start. Without it, a path that holds no store (a typo, or a volume that did not mount) is refused instead of being served as a new, empty store. A file that holds something else, such as another program's database, is refused even with `--create`, and left untouched. The service makes the store's files `0600`, since they hold personal data, and warns at start if they are readable by others.

A new store is at version 0, with no flow. Publish its first version as you would any later one: a `DeclarationChange` drafted, submitted and approved (see [Build an application](integrate.md#changing-the-flow)).

## Run it under systemd

A unit that serves the store on a Unix socket only the service's group can open:

```ini
[Unit]
Description=ObjectFlow
After=time-sync.target

[Service]
Type=notify
User=objectflow
Group=objectflow
StateDirectory=objectflow
RuntimeDirectory=objectflow
RuntimeDirectoryMode=0750
UMask=0077
ExecStart=/usr/local/bin/objectflow-service \
    --store /var/lib/objectflow/store.sqlite \
    --socket /run/objectflow/objectflow.sock \
    --metrics-bind 127.0.0.1:9712 \
    --attempt-retention "90 days" --idempotency-retention "30 days"
TimeoutStopSec=45
LimitNOFILE=4096
MemoryMax=4G
LogRateLimitIntervalSec=0

[Install]
WantedBy=multi-user.target
```

Why each setting:

- `Type=notify`: the service tells systemd when it is ready to answer and when it stops, so your application's unit can start after it.
- `After=time-sync.target`: start only once the clock is synchronised. The store's clock never moves back, so a host clock that jumps ahead and returns would stamp events in the future. The ordering waits only if a service delays the target until synchronisation: enable `systemd-time-wait-sync.service` with systemd-timesyncd, or `chrony-wait.service` with chrony. Don't add `Wants=time-sync.target`. See [The clock](#the-clock).
- `--socket`: the socket is made `0660`, so only the service's user and group connect. Add your application's user to the `objectflow` group.
- `--metrics-bind`: `/health` and `/metrics` on an address of their own, so a monitor can reach them and is never shut out by a full main listener.
- `TimeoutStopSec`: above the service's own 30 s drain.
- `LimitNOFILE`: above `--max-connections` (512 by default), with room for the 16 refusals it may be writing at once.
- `MemoryMax`: see [Memory](#memory).
- `LogRateLimitIntervalSec=0`: the service logs one line per answer, which passes journald's default rate limit at about 430 answers a second.

You may add `WatchdogSec=`: the service pings at half the interval while its runtime answers. That says nothing about whether writes move; `/health`'s `writes_stalled` does.

## Flags

Durations are a count and a unit, with a space: `30 s`, `2 min`, `1 h`, `90 days`, `2 weeks`.

| Flag | Default | Sets |
|---|---|---|
| `--port <n>` | `8712` | The loopback port to listen on. `0` takes a free one. |
| `--bind <host:port>` | | An address to listen on instead. Beyond loopback, the service warns at start. |
| `--socket <path>` | | A Unix socket to listen on instead, made `0660`. A socket a stopped service left is replaced; a file of another kind never. |
| `--idle-timeout <duration>` | `30 s` | How long a connection may take to send a request's head. A body that does not arrive in time is answered `408`. |
| `--max-connections <n>` | `512` | Connections served at once. At the cap, the oldest idle connection is closed to make room; only when every one is busy is a newcomer answered `503 busy`. |
| `--metrics-bind <host:port>` | | Serves `GET /health` and `GET /metrics` alone on a second address. |
| `--store <file>` | | Serves the deployment store in this file. Without it, the service runs a sandbox. |
| `--create` | | Makes the store, the first time. *Deployment only.* |
| `--attempt-retention <duration>` | none | How long refused attempts are kept before `maintain` may prune them. *Deployment only.* |
| `--idempotency-retention <duration>` | none | How long idempotency records are kept before `maintain` may prune them. *Deployment only.* |
| `--evaluator <name>=<url>` | | The endpoint an evaluator's calls are posted to. Repeat it for each evaluator. *Deployment only.* |
| `--evaluator-timeout <duration>` | `5 s` | How long an evaluator has to answer before its guard is unknown. *Deployment only.* |
| `--evaluator-token <name>=<file>` | | A file holding a bearer token for that evaluator. Make the file readable by the service's user alone. It is never logged. *Deployment only.* |
| `--busy-timeout <duration>` | `5 s` | How long a write waits for another process's lock on the store, before each of its retries. *Deployment only.* |
| `--read-timeout <duration>` | `30 s` | How long a read may run, or wait for its turn. *Deployment only.* |
| `--max-reads <n>` | `4` | Reads that grow with the store allowed at once, in each pool. *Deployment only.* |

`--help` prints the usage. An argument the service does not take is refused at start.

## Health

`GET /health` answers `200` while the store serves, opening the store's file to answer:

```json
{"release": "objectflow-core 0.2.0", "shape": "deployment", "version": 3, "now": "2026-10-13T01:03:28Z", "store_release": "objectflow-core 0.2.0", "upgrade_due": false, "clock_lead_seconds": 0, "writer_held_seconds": 0, "requests_in_flight": 1, "connections_open": 1, "disk_free_bytes": 52428800000}
```

`release` is the engine's, `store_release` the store's; `version` is the installed flow version and `now` the store's clock; `writer_held_seconds` is `0` while no write runs. A health check that answers `200` is not logged.

It answers `503` with a `code` when an operator must act:

| `code` | Means | Do |
|---|---|---|
| `upgrade_due` | The store was made by an earlier engine release. | Run the upgrade. |
| `disk_low` | Under 64 MiB free on the store's volume, the space a checkpoint needs. | Free space. |
| `store_replaced` | The store's file was moved, removed or replaced under the running service. Writes are refused too. | Restart the service on the right file. |
| `log_blocked` | A log line has waited over 5 s to be written: nothing is reading the service's standard error. | Fix the log reader. journald does this for you. |
| `writes_stalled` | The last write met another process's lock past its retries, and none has committed since, within the minute. | Find the other process holding the store. |

## Metrics

`GET /metrics` answers in Prometheus's text format:

| Metric | What it shows |
|---|---|
| `objectflow_requests_total` | Answers, by route and status. |
| `objectflow_request_seconds_total` | Seconds spent answering, by route and status. |
| `objectflow_request_duration_seconds` | A histogram of answer times, by route. |
| `objectflow_verdicts_total` | Verdicts by kind. A refusal is a `200`, so this is where refusals show. |
| `objectflow_requests_in_flight` | Requests being answered now. |
| `objectflow_connections_open` | Connections open now. |
| `objectflow_connections_made_room_total` | Idle connections closed to make room at the cap. |
| `objectflow_connections_refused_total` | Newcomers refused `503 busy` at the cap. |
| `objectflow_reads_running` | Reads running, by pool. |
| `objectflow_reads_waiting` | Reads waiting for a turn, by pool. |
| `objectflow_reads_most` | Each pool's limit. |
| `objectflow_writer_held_seconds` | How long the current write has held the writer. |
| `objectflow_evaluator_failures_total` | Evaluator calls that gave no usable answer, by evaluator. |
| `objectflow_store_version` | The installed flow version. |
| `objectflow_store_bytes` | The store's file size. |
| `objectflow_wal_bytes` | The `-wal` file's size. |
| `objectflow_wal_cuts_total` | Times the `-wal` was cut back. |
| `objectflow_wal_cuts_missed_total` | Cuts a long read held off past their wait. |
| `objectflow_disk_free_bytes` | Free bytes on the store's volume. |
| `objectflow_clock_lead_seconds` | How far the store's clock is ahead of the wall clock. |
| `objectflow_resident_bytes` | The process's resident memory. |
| `objectflow_memory_given_back_total` | Times freed memory was returned to the system. |
| `objectflow_log_lines_dropped_total` | Log lines dropped because nothing read them for 5 s. |
| `objectflow_build_info` | The release and commit serving. |
| `objectflow_start_time_seconds` | When the process started. |

**Alert on:**

- `/health` not `200`, on the `--metrics-bind` address;
- `objectflow_disk_free_bytes` well before 64 MiB;
- `objectflow_clock_lead_seconds` above 60;
- `objectflow_writer_held_seconds` above a minute: a publish or an import is running, or a write is stuck;
- any rise in `objectflow_log_lines_dropped_total`, `objectflow_wal_cuts_missed_total` or `objectflow_connections_refused_total`;
- `objectflow_resident_bytes` near your `MemoryMax`;
- `objectflow_evaluator_failures_total` rising: an outside system is down, and its guards refuse `temporal`.

## Logs

The service writes to its standard error, each line stamped with its time:

- where it listens, once it answers;
- one line per answer: `<time> <method> <route> <status> <duration> ms`, with `clock_lead=<n>s` while the clock leads;
- each fault it meets, with its reason. A 5xx of one code is written at most once a second, the number left unsaid given with the next, so a full disk does not write a line per retried write;
- an evaluator's failure, naming its function but never the arguments or the answer, which may hold personal data, and its endpoint with any password hidden;
- at start: `--create` given for a store that exists, a last stop that was not clean, a `-wal` holding frames before opening, a store readable by others, another service holding the store, an evaluator token to be sent over plain `http://` beyond loopback, and a clock that leads.

Read the standard error to its end, as journald does. If nothing reads it, a line waits; after 5 s the service drops lines (counted in `objectflow_log_lines_dropped_total`) so no request waits on the log, and `/health` answers `503 log_blocked`.

## Capacity

**Writes** run one at a time, each a durable commit of about 10 ms. A write that meets another process's lock on the store waits `--busy-timeout`, up to four times, then is refused `stale` with cause `retries_exhausted`, and `/health` says `writes_stalled`. In all that took up to 34.5 s in a measurement: give clients a write timeout above it.

**Reads** run beside writes, each with its own deadline (`--read-timeout`). Reads whose cost grows with the store take a turn in one of four pools, so no kind of read waits behind another:

| Pool | Reads | Limit |
|---|---|---|
| worklists | `query`, `available`, `pull`, `history`, `events` | `--max-reads` |
| analytics | `metric`, `exceptions`, `diagnostics`, `export` | `--max-reads` |
| agents | every such read an agent actor makes | `--max-reads` |
| dry runs | a change's `submit` or `refresh`, a `publish` preview, a `check` of a change's submit | one at a time |

`get`, `lookup`, `availability`, `declaration`, `tools`, every other `check` and every write never wait for a pool, nor do `/health` and `/metrics`. A read that waits past its deadline is answered `503 read_timeout`, having done nothing. People polling `available` over a crowded state share the worklists pool: raise `--max-reads` for them, or poll less often.

**Requests** are at most 2 MiB. Import in batches under that, about 3,000 objects of an expense claim's size each.

**Measured** on a shared Intel i7-8700 with NVMe, release build, 2026-10-12. Measure your own deployment.

| | 2 years: 36.7k objects, 126 MB | 6 years: 110k objects, 387 MB |
|---|---|---|
| a `get` or a worklist query | 1 to 11 ms | |
| a write | about 10 ms | |
| publishing a version that adds an attribute: submit (runs beside writes) | about 2 s | about 6 s |
| … approval (holds writes) | about 3 s | about 9 s |
| peak memory: submit / approval | 0.5 GB / 1.1 GB | 1.5 GB / 3 GB |
| a go-live import of 36k objects, 3,000 a batch | 36 s, each batch about 1.6 s | |

Publishing grows linearly with the store: do it outside the busiest hours. Its approval holds writes for its whole run, and a stop waits only 30 s, so stop the service after a publish, never during one. A dry run that passes `--read-timeout` is answered `503 read_timeout` and records nothing: give a store whose dry run takes longer a longer deadline.

Some flow constructs cost more than they look:

- **A guard that reads a metric** computes the metric under the writer for each request it guards. A per-claimant monthly limit took 0.12 s a submit on 37k objects, and twenty at once held other writes about 2.7 s. Put such a guard on a rarely taken transition, or read the metric over a short window.
- **An attribute without `indexed`** is scanned when a filter reads it. Mark `indexed` what worklists and lookups filter by. References, unique attributes and external identifiers are indexed anyway.

## Memory

A publish holds the store's objects in memory while it simulates and installs the version: plan for about ten times the store's file size. Reads that grow with the store add their own, at most `--max-reads` per pool at once. On 110k objects, sixteen per-person metrics sent at once peaked at 1.15 GB with the default limit of 4.

Size `MemoryMax` at a publish's peak plus twice `--max-reads` times your heaviest read's peak, and measure your own. After a publish, an import or any request of half a second or more, the service returns freed memory to the system (`objectflow_memory_given_back_total`).

## Backups and restore

**Back up a running store** with SQLite's online backup, which copies a consistent store while the service writes. Run it as the service's user, so the copy is readable by that user alone:

```sh
umask 077
python3 -c "import sqlite3; sqlite3.connect('/var/lib/objectflow/store.sqlite').backup(sqlite3.connect('/backup/store.sqlite'))"
```

`VACUUM INTO '/backup/store.sqlite'`, run on a connection of your own, works too, and writes a compacted copy.

Copying the file alone is safe only with the service stopped: a clean stop folds the `-wal` into the file. A backup holds personal data, and an erasure cannot reach it: keep backups `0600`, and expire or encrypt them within the period your privacy rules allow.

**Restore:**

1. Stop the service.
2. Remove the store's `-wal` and `-shm` files.
3. Put the backup in the store's place, with the same owner and mode `0600`.
4. Start the service.

Step 2 matters: a `-wal` left beside a restored copy would be replayed onto it. The service runs SQLite's quick check at each start, before anything opens the store for writing; if it fails, the service does not start, says why, and leaves every file as it was. The check reads the whole file: about 0.3 s for 126 MB, 0.8 s for 387 MB.

## Upgrading the engine

What a later release promises, from 1.0.0:

- Within `/v1`, a release only adds: operations, keys, fields, verdict kinds and reasons. A breaking change is `/v2`, served beside `/v1`.
- A flow valid under one release stays valid under every later release of the same major version. A check that would refuse it arrives first as a notice.
- A store is upgraded, never stranded.
- A major version gets security fixes for 12 months after the next major's first release.

**To upgrade:**

1. Stop the service.
2. Back up the store's file, and its `-wal` if one is beside it.
3. Start the new release on the store. It answers every operation `503 engine_mismatch`, and `/health` `503 upgrade_due`, until the store is upgraded.
4. As the operator, see the steps: `POST /v1/upgrade {"actor": {"id": "operator"}, "dry_run": true}`.
5. Run them: the same without `dry_run`. The service then serves the store.

There is no downgrade: if the new release fails in service, restore the backup and run the old release. Once an upgrade commits, an old copy still running is refused; a copy of release 0.1.0 is not, so stop it before upgrading.

## The clock

A deployment uses the wall clock, but the store's clock never moves back. If the host's clock runs ahead and returns, every event is stamped at the latest time reached until the wall clock catches up. The service says so at start, on each log line, in `/health` (`clock_lead_seconds`, still `200`) and in `objectflow_clock_lead_seconds`. Fix the host's clock and the lead runs down. No operation moves the store's clock back, so keep the host synchronised and start the service after `time-sync.target`, with a wait-sync service enabled as [above](#run-it-under-systemd).

## Stopping

On SIGTERM or SIGINT the service stops taking connections, answers the requests in flight, and exits, saying so. One still running after 30 s is cut, and rolled back. A process killed outright loses no acknowledged write: every acknowledged write is durable. Its clients may get no answer, so send every write with an idempotency key and retry an unanswered one with it.

## Housekeeping

Refused attempts and idempotency records grow with use. Prune them with `maintain`, within the retention the service was started with:

```json
POST /v1/maintain
{"actor": {"id": "operator"}, "task": {"name": "prune_attempts", "before": "2026-07-01T00:00:00Z"}}
```

`prune_idempotency` works the same way. A prune inside the retention is refused `inside_retention`, and one for a task the service has no retention for, `no_retention`. The event log is kept whole: `archive_events` answers `501`.

## Personal data on disk

An erasure overwrites what it erased in the store's file and truncates the `-wal` after it commits, so neither file holds an erased value. A read open at that moment keeps the `-wal`'s earlier frames: within 5 s, the service holds back new reads that grow with the store, waits for the running ones (up to their deadline), and truncates the log. It logs any read that held it longer, counted in `objectflow_wal_cuts_missed_total`. The same cut runs whenever the `-wal` passes 64 MiB. **While a cut is due, a new read that grows with the store waits for the longest one running**, a person's worklist behind an agent's sweep included: a short `--read-timeout` bounds that wait.

What an erasure reaches is what the flow marks `personal`, and nothing else. Free text in inputs not marked `personal`, and a request's `context`, keep what was typed, refused attempts included. Where an actor's identity attribute is `personal`, erasing the actor replaces its identity with its object id everywhere the store keeps it. Better: give actors opaque identities, such as an employee number, never an email or a name, written in one Unicode form (the engine compares identities as written, ignoring case only).

An erasure that a loop or cascade cannot carry out within its declared bound is refused `over_limit`, naming the loop, and erases nothing. Publish a version whose bound covers the set, or that bounds the set with the invariant the checker's `unbounded_reach` notice names, then ask again.

# Operate a deployment

How to run ObjectFlow as a live service on one Linux machine: install it, start it under systemd, watch it, back it up, upgrade it and size it. For the model, read [Concepts](concepts.md). For your application's side, read [Build an application](integrate.md).

## What you are running

One process, `objectflow-service`, serving one **store**: a SQLite file and the files SQLite keeps beside it (`-wal`, `-shm`), plus `<store>.lock`, which the service holds while it runs. It writes one request at a time and reads beside its writes. A second service started on the same store says so at start and shares it, as an upgrade's overlap needs; stop one otherwise. A store may instead be a PostgreSQL database, which [A store on PostgreSQL](#a-store-on-postgresql) describes.

**It authenticates nobody.** Whoever can reach it can act as any actor. Put it behind the application that authenticates your users: on a Unix socket only your application can open, or on loopback. Never put it on a network your users reach.

**Requirements:** Linux on x86-64 with the glibc of Ubuntu 22.04 or later (`ldd --version`), and disk for the store, its `-wal` (64 MiB in steady state, up to about 350 MiB while long reads overlap writes; 346 MiB was measured) and your backups. Keep the store on a local filesystem: SQLite's locks, which keep a second process from writing beside the service, do not hold on a network one.

## Install

Build the release binary from source (rustup installs the pinned toolchain on the first build):

```sh
(cd core && OBJECTFLOW_COMMIT=$(git rev-parse --short HEAD) cargo build --release -p objectflow-service)
core/target/release/objectflow-service --version
sudo install -m 0755 core/target/release/objectflow-service /usr/local/bin/
```

Build from `core/`, as above: rustup takes the toolchain `core/rust-toolchain.toml` pins, and the repository's `.cargo/config.toml` builds SQLite with a page cache for each connection, without which heavy reads at once each take as long as all of them. A build made where that file is not read stops and says so; one from the repository's root reads it but takes rustup's default toolchain.

Tagged releases will be published on the repository's GitHub Releases page as an archive with its SHA-256 checksum, built on Ubuntu 22.04; none is published yet, so build from source. Check the checksum before installing. Releases are not signed yet. `--version` names the commit a release was built from, and a build from source the one `OBJECTFLOW_COMMIT` names, as above; without it, none.

## Create the store

Make the store once, with `--create`, as the user the service runs as, so the store's files are that user's:

```sh
sudo useradd --system --home-dir /var/lib/objectflow --shell /usr/sbin/nologin objectflow
sudo install -d -o objectflow -g objectflow -m 0750 /var/lib/objectflow
sudo -u objectflow objectflow-service --store /var/lib/objectflow/store.sqlite --create --port 0
```

It makes the store and then serves it: once its first line says where it listens, stop it with Ctrl-C. Leave `--create` out of every later start. Without it, a path that holds no store (a typo, or a volume that did not mount) is refused instead of being served as a new, empty store. A file that holds something else, such as another program's database, is refused even with `--create`, and left untouched. The service makes the store's files `0600`, since they hold personal data, and warns at start if they are readable by others.

A new store is at version 0, with no flow. Publish its first version as you would any later one: a `DeclarationChange` drafted, submitted and approved (see [Build an application](integrate.md#changing-the-flow)).

From a shell on the host, as the operator, with `curl` and `jq`, a module `leave.yaml` checked first with `scripts/check-flows.py`, once the service runs under the unit of [Run it under systemd](#run-it-under-systemd), which makes the socket:

```sh
SOCK=/run/objectflow/objectflow.sock
of() { curl -s --unix-socket "$SOCK" "http://objectflow/v1/$1" -H 'Content-Type: application/json' -d "$2"; echo; }
# 1. draft the change: each module's text under its name
CHANGE=$(jq -n --rawfile leave leave.yaml \
    '{actor: {id: "operator"}, type: "DeclarationChange", transition: "draft",
      inputs: {source: {leave: $leave}, source_ref: "first"}}' \
  | curl -s --unix-socket "$SOCK" http://objectflow/v1/request -H 'Content-Type: application/json' -d @- \
  | jq -r .object_id)
# 2. submit it, which runs the dry run
of request "{\"actor\": {\"id\": \"operator\"}, \"object_id\": \"$CHANGE\", \"transition\": \"submit\"}"
# 3. approve the change at the version you read, which installs it
VERSION=$(of get "{\"actor\": {\"id\": \"operator\"}, \"id\": \"$CHANGE\"}" | jq .version)
of publish "{\"actor\": {\"id\": \"operator\"}, \"change\": \"$CHANGE\", \"expected_version\": $VERSION}"
```

Once it is approved, `/health` answers the new `version`. A submit refused `dry_run_clean` says why in a preview: `of publish "{\"actor\": {\"id\": \"operator\"}, \"change\": \"$CHANGE\", \"dry_run\": true}"`.

## A store on PostgreSQL

The store may be a PostgreSQL database instead of a file (ADR-0213): `--store` names it, a `postgresql://` URL or a connection string, and everything else on this page holds but what this section says differs.

**The database.** PostgreSQL 16 or later, a database of the store's own whose text is compared by bytes and folded in its ASCII letters alone, which the service refuses otherwise, since the store's ids sort as their bytes do and an identity is matched in another case as the engine folds it: under another character type or ICU's, `lower()` folds `Ä` to `ä`, and two people the engine holds apart would match. Name the provider, since a server whose default is ICU makes an ICU database without it. And a role that owns it:

```sql
CREATE ROLE objectflow LOGIN PASSWORD '…';
CREATE DATABASE objectflow OWNER objectflow TEMPLATE template0 ENCODING 'UTF8' LOCALE_PROVIDER libc LC_COLLATE 'C' LC_CTYPE 'C';
GRANT pg_checkpoint TO objectflow;   -- an erasure's checkpoint, below; PostgreSQL 15 and later
```

**Start it** with the password in a file the service's user alone reads, or in `OBJECTFLOW_STORE_PASSWORD`; a URL holding one is refused, since a command line is shown to every user of the machine. The first start makes the store with `--create`, and a database holding none is refused without it:

```sh
sudo install -o objectflow -m 0600 /dev/stdin /etc/objectflow/db-password <<< '…'
sudo -u objectflow objectflow-service --create --port 0 \
    --store 'postgresql://objectflow@db.internal:5432/objectflow?sslmode=require' \
    --store-password-file /etc/objectflow/db-password
```

In the unit, `--store` and `--store-password-file` take `--store <file>`'s place. **TLS:** give `sslmode=require`; the service then verifies the server's certificate and its host against the machine's trust store, so install your provider's certificate authority there (`/usr/local/share/ca-certificates`, then `update-ca-certificates`). The client takes `disable`, `prefer` and `require`, not `verify-full`, `require` verifying as `verify-full` does; `prefer`, the default, falls back to a plain connection, which the start says where the host is not this machine.

**One service writes it.** The store's connection holds an advisory lock on it for its life, so a second service started on the same store is refused. The lock goes with the connection: at once when the service stops or crashes, and within about half a minute when its host vanishes without a word, since the service sets the server's keepalives on its sessions (a probe after 10 s idle, three 5 s apart, and 30 s for data to be acknowledged), where the server's defaults kept a vanished host's lock two hours. To free it at once, end the old session: `SELECT pg_terminate_backend(pid) FROM pg_locks WHERE locktype = 'advisory' AND database = (SELECT oid FROM pg_database WHERE datname = current_database());`. Several services writing one store at once is not supported.

**A lost connection**, to the database's restart, a terminated session or an idle timeout, answers the write that met it 503, which a client may retry, and the next write opens the store again and takes its lock again; a read that met it is answered 503 likewise, and the next opens a connection of its own. **A database that is not the store's**, which a lost connection opened again finds, a standby a failover promoted (another timeline) or a restore in its place (another cluster or database), may lack the last writes the store answered: the store is served no more, every read and write answered 503, until you mark it a copy as a restored database is (below) and start the service again. So after a failover, mark the new primary a copy before the service writes on it. **Each commit is flushed** before it is answered, whatever the database's `synchronous_commit`, which the service sets on its sessions; an asynchronous standby may still lack what the primary flushed, which marking a promoted one a copy accounts for. **A database that stops answering** without closing its connections, frozen or stalled on its disk: a read is answered `503 read_timeout` at its deadline, `--read-timeout`, its connection closed and another made for the next, a new connection given up by the read's deadline too; `/health` answers `503 store_stalled` once its read of the store has waited 5 s, and at once while that read still runs, so a monitor sees the stall. **A write waits** for the database to answer, since a commit cut short has an outcome no one knows, and the writes queued behind it with it; a writer reconnecting is given up after 30 s. Watch the database's own health too. A connection is waited for 5 s.

**Measured** at the profile (`scripts/profile/`, 12,000 claims, the profile's steady load, one 12-thread machine with the database beside the service): every read and write within PRD C5, the per-person metric `claimant_over_policy_lines` at p95 985 ms against its 1 s, which missed it from 0.3.0 to 0.5.0 (1,029 to 1,039 ms) and so stays to be measured on a deployment's own hosts; writes p95 88.4 ms, where the same load on a file's store gives 36.5 ms (0.5.1's release build). A database across a network adds its round trip to each of a write's statements, so measure on your own hosts before relying on these.

**A publish** that changes a type's columns rebuilds its table, which waits for the reads holding the table, a backup's dump among them, up to the busy timeout, and is answered 503 past it with its version not taken; publish outside a backup's run, or again. **Connections:** the writer's, and one for each read running at once, up to `--max-reads` beside the reads every request makes and eight kept idle; size the server's `max_connections` for them.

**Back it up** with your provider's snapshots and point-in-time recovery, or `pg_dump --format=custom`. A database restored from a backup is a copy, which must be marked before the service starts on it, as a file's copy is, so the restore moves the store's counters past what the lost store may have given out (ADR-0207):

```sh
psql 'postgresql://objectflow@db.internal/objectflow?sslmode=require' \
    -c "INSERT INTO of_counter (name, value) VALUES ('copied', 1) ON CONFLICT (name) DO UPDATE SET value = 1"
```

**Restore** a dump into a database made as above, mark it, then start the service on it:

```sh
pg_restore --no-owner --role=objectflow --dbname='postgresql://objectflow@db.internal/objectflow_restored?sslmode=require' objectflow.dump
psql 'postgresql://objectflow@db.internal/objectflow_restored?sslmode=require' \
    -c "INSERT INTO of_counter (name, value) VALUES ('copied', 1) ON CONFLICT (name) DO UPDATE SET value = 1"
```

The start says the restore in one line, as a file's does. **A forgotten mark stops the start** rather than handing out again what the live store gave: the store keeps the database it was last opened in, its cluster's system identifier, its timeline and its oid, and a start in another, a restore in its place, a standby a failover promoted or a dump restored anywhere, is refused until the copy is marked. **Check a backup** as you check a file's: restore it into a scratch database, made as above, and read its release, `SELECT release FROM of_engine_release ORDER BY seq DESC LIMIT 1`; then drop it.

**An erasure** rewrites every copy the store holds, as on a file, then vacuums the store's tables, cleaning their indexes and sampling them afresh for the planner, and takes a checkpoint, so neither a row a query can read nor the planner's samples (`pg_stats`) hold an erased value once no older snapshot remains. A snapshot older than the erasure keeps the old versions: a read open then, a standby's feedback, a replication slot or a prepared transaction. The vacuum is then tried again every 5 s, and the service says so once it has waited a minute; one due when the service stops runs at its next start. **The bytes stay in the data files' free space** until the daily housekeeping writes every table anew (`rebuild_indexes`, below), since a vacuum frees space without zeroing it: none is left past a day of it. *Retracted 2026-10-17: this paragraph said no erased value stays in the data files once vacuumed, measured on three values; the personal-data area taken again on PostgreSQL found them in free space after every erasure it probed ([`personal-data-postgresql.md`](../design/audit/personal-data-postgresql.md)), and a test of the store's files now finds them there until the housekeeping has run.*

**Limits**, which bound what an erasure on PostgreSQL removes; bound their retention to what your erasure policy allows (ADR-0213 decision 8):

- **The write-ahead log** keeps erased values, the erasure's own log among them, until later log overwrites its recycled files, about `max_wal_size` of it later; a replication slot keeps the log until it is consumed, so set `max_slot_wal_keep_size`.
- **A log archive, a physical backup or a volume snapshot** taken before the next housekeeping holds them, one taken after the erasure as well as one before; a logical dump (`pg_dump`) taken after the erasure holds none.
- **The server's log** holds what a request sent where statements are logged with their parameters (`log_statement`, `log_min_duration_statement`, `auto_explain`, pgaudit): keep it off on the store's database, or bound the log's retention as a backup's. The defaults log no value.
- **Whoever can read the data directory** reads erased values in free space until the housekeeping.

Without `pg_checkpoint` the data files lose the old versions at the server's next scheduled checkpoint.

**What differs from a file's store.** No `-wal`, `-shm` or `.lock` beside it, and no file mode to check; the start's quick check is SQLite's and does nothing here, so make the cluster with data checksums (`initdb --data-checksums`, or your provider's setting) and check it with PostgreSQL's own tools; `/health` reports no `disk_free_bytes`, the store's volume being the server's, so watch its disk there. **Names and values PostgreSQL bounds.** It keeps 63 bytes of a name, so a version whose tables or indexes, named from a type's name and an attribute's, would pass them is refused `409` there, where SQLite takes it; and an index's entry holds 2,704 bytes, so a value of a unique, indexed or identity attribute past them is refused `409` (a value that compresses well may fit). **A store on PostgreSQL is upgraded as a file's is** (To upgrade, below): a later release started on it serves the upgrade alone until it runs, and the upgrade holds the store's lock, so it is refused while an older service still writes the store. **`maintain`'s `rebuild_indexes`** writes every table anew on PostgreSQL, below.

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
StateDirectoryMode=0750
RuntimeDirectory=objectflow
RuntimeDirectoryMode=0750
UMask=0077
ExecStart=/usr/local/bin/objectflow-service \
    --store /var/lib/objectflow/store.sqlite \
    --socket /run/objectflow/objectflow.sock \
    --metrics-bind 127.0.0.1:9712 \
    --attempt-retention "90 days" --idempotency-retention "30 days"
TimeoutStopSec=45
Restart=on-failure
LimitNOFILE=4096
MemoryMax=4G
LogRateLimitIntervalSec=0
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
ReadWritePaths=/var/lib/objectflow

[Install]
WantedBy=multi-user.target
```

Why each setting:

- `Type=notify`: the service tells systemd when it is ready to answer and when it stops, so your application's unit can start after it.
- `After=time-sync.target`: start only once the clock is synchronised. The store's clock never moves back, so a host clock that jumps ahead and returns would stamp events in the future. The ordering waits only if a service delays the target until synchronisation: enable `systemd-time-wait-sync.service` with systemd-timesyncd, or `chrony-wait.service` with chrony. Don't add `Wants=time-sync.target`. See [The clock](#the-clock).
- `--socket`: the socket is made `0660`, so only the service's user and group connect. Add your application's user to the `objectflow` group. The socket is the access control: the service authenticates no one, so on a TCP port, loopback included, every user of the machine can call it as any actor, the operator among them. Serve TCP only on a machine no one else uses. Whatever holds the socket acts as any actor it names, so keep `maintain`, `upgrade` and `import_batch` to your operators' own tools, away from the paths people and agents reach: an import may admit invariant violations, and records the `decided_by` it is sent as given.
- `--metrics-bind`: `/health` and `/metrics` on an address of their own, so a monitor can reach them and is never shut out by a full main listener.
- `TimeoutStopSec`: above the service's own 30 s drain.
- `Restart=on-failure`: a service that exits on a fault, or is killed, is started again. A start the service refuses, for a store it cannot open, fails again each time and stays failed once systemd's start limit is reached (five starts in ten seconds by default), saying why in the journal.
- `LimitNOFILE`: above `--max-connections` (512 by default), with room for the 16 refusals it may be writing at once.
- `MemoryMax`: 4G suits a store of about two years; see [Memory](#memory) for more.
- `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`, `PrivateTmp`: the service writes only its store's directory, which `ReadWritePaths` names, and the socket's, which `RuntimeDirectory` makes.
- `LogRateLimitIntervalSec=0`: the service logs one line per answer, which passes journald's default rate limit at about 430 answers a second.

You may add `WatchdogSec=`: the service pings at half the interval while its runtime answers. That says nothing about whether writes move; `/health`'s `writes_stalled` does.

Call it over the socket with `curl`, as the service's user (`sudo -u objectflow`) or a member of its group, since the socket's directory is `0750` and the socket `0660`, the body's type named, as every operation takes JSON:

```sh
curl -s --unix-socket /run/objectflow/objectflow.sock http://localhost/health
curl -s --unix-socket /run/objectflow/objectflow.sock http://localhost/v1/get \
  -H 'Content-Type: application/json' -d '{"id": "operator"}'
```

## Flags

Durations are a count and a unit, with a space: `30 s`, `2 min`, `1 h`, `90 days`, `2 weeks`.

| Flag | Default | Sets |
|---|---|---|
| `--port <n>` | `8712` | The loopback port to listen on. `0` takes a free one. |
| `--bind <host:port>` | | An address to listen on instead. Beyond loopback, the service warns at start. |
| `--socket <path>` | | A Unix socket to listen on instead, made `0660`. A socket a stopped service left is replaced; a file of another kind never. |
| `--idle-timeout <duration>` | `30 s` | How long a connection may take to send a request's head, or leave its answer unread. A body that does not arrive in time is answered `408`; an answer not read in time closes the connection. |
| `--max-connections <n>` | `512` | Connections served at once, at most 4,096. At the cap, the oldest idle connection is closed to make room; only when every one is busy is a newcomer answered `503 busy`. |
| `--metrics-bind <host:port>` | | Serves `GET /health` and `GET /metrics` alone on a second address. |
| `--store <file>` | | Serves the deployment store in this file, or in the PostgreSQL database a `postgresql://` URL or a connection string names ([A store on PostgreSQL](#a-store-on-postgresql)). Without it, the service runs a sandbox. |
| `--store-password-file <file>` | | The file a PostgreSQL store's password is read from, else `OBJECTFLOW_STORE_PASSWORD`. A password in `--store` itself is refused. |
| `--create` | | Makes the store, the first time. *Deployment only.* |
| `--attempt-retention <duration>` | none | How long refused attempts are kept before `maintain` may prune them. *Deployment only.* |
| `--idempotency-retention <duration>` | none | How long idempotency records are kept before `maintain` may prune them. *Deployment only.* |
| `--evaluator <name>=<url>` | | The endpoint an evaluator's calls are posted to. Repeat it for each evaluator. *Deployment only.* |
| `--evaluator-timeout <duration>` | `5 s` | How long an evaluator has to answer before its guard is unknown. *Deployment only.* |
| `--evaluator-token <name>=<file>` | | A file holding a bearer token for that evaluator, read at start: restart the service to rotate it. Make the file readable by the service's user alone. It is never logged. *Deployment only.* |
| `--busy-timeout <duration>` | `5 s` | How long a write waits for another process's lock on the store, before each of its retries. *Deployment only.* |
| `--read-timeout <duration>` | `30 s` | How long a read may run, or wait for its turn. *Deployment only.* |
| `--dry-run-timeout <duration>` | `2 min` | How long a change's dry run, its `submit`, `refresh`, `publish` preview or a `check` of its submit, may run, or wait for its one slot. *Deployment only.* |
| `--max-reads <n>` | `4` | Reads that grow with the store allowed at once, in each pool. *Deployment only.* |

An evaluator served over HTTPS under your own certificate authority is trusted through the system's store, or a bundle `SSL_CERT_FILE` names; its certificate must be one the authority issued, since the authority's own certificate is refused as the endpoint's (`CaUsedAsEndEntity`). Under systemd, give the bundle in the unit, `Environment=SSL_CERT_FILE=/etc/objectflow/ca-bundle.pem`; an evaluator it cannot verify is logged as `UnknownIssuer`.

`--help` prints the usage. An argument the service does not take is refused at start.

## Health

`GET /health` answers `200` while the store serves, opening the store's file to answer:

```json
{"release": "objectflow-core 0.5.1", "shape": "deployment", "version": 3, "now": "2026-10-13T01:03:28Z", "store_release": "objectflow-core 0.5.1", "upgrade_due": false, "clock_lead_seconds": 0, "writer_held_seconds": 0.0, "requests_in_flight": 1, "connections_open": 1, "disk_free_bytes": 52428800000}
```

`release` is the engine's, `store_release` the store's; `version` is the installed flow version and `now` the store's clock; `writer_held_seconds` is seconds with a fraction, `0.0` while no write runs, in a sandbox too; while writes wait for another process's lock, it counts from the first retry, through every write refused since. A health check that answers `200` is not logged.

It answers `503` with a `code` when an operator must act:

| `code` | Means | Do |
|---|---|---|
| `upgrade_due` | The store was made by an earlier engine release. | Run the upgrade. |
| `disk_low` | Under 64 MiB free on the store's volume, the space a checkpoint needs. The log says it once as free space falls below, and once as it recovers. | Free space. |
| `store_replaced` | The store's file was moved, removed or replaced under the running service. Writes are refused too. | Restart the service on the right file. |
| `store_unreadable` | The store's file cannot be read as a store, as after one written over in place while the service ran. | Stop the service and restore the store. |
| `disk_full` | The last write failed for a full volume, though its free space may not show it (a quota or a reserve), and none has committed since. | Free space; it clears with the next write that commits. |
| `log_blocked` | A log line has waited over 5 s to be written: nothing is reading the service's standard error. | Fix the log reader. journald does this for you. |
| `writes_stalled` | A write is waiting for another process's lock on the store, from its first retry; or the last write was refused after its retries, and none has committed since, within the minute. | Find the other process holding the store. |
| `store_stalled` | The store did not answer `/health`'s read within 5 s: its database stopped answering without closing its connections, frozen or stalled on its disk, or its volume did, and every request waiting on it waits with it. While that read runs, `/health` answers this at once. | Check the database's host and disk; it clears once the store answers. |
| `writes_failing` | Three writes or more in a row failed for the service's own fault, answered `500`, the last within the minute, and none has succeeded since. | Read the log for the fault; check the store's file was not changed under the service. |

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
| `objectflow_read_wait_seconds` | How long reads waited for their turn, by pool, a histogram: a read refused `read_timeout` counted at its deadline. |
| `objectflow_reads_most` | Each pool's limit. |
| `objectflow_writer_held_seconds` | How long the current write has held the writer. |
| `objectflow_request_groups_total` | Commits of requests: the requests that wait for the writer while it commits are committed together, with one flush, and a request alone is a group of one (ADR-0208). |
| `objectflow_request_grouped_total` | The requests those commits held; divided by the groups, the requests each commit carried, about 1 at a light load and rising as writes queue. |
| `objectflow_evaluator_failures_total` | Evaluator calls that gave no usable answer, by evaluator. |
| `objectflow_store_version` | The installed flow version. |
| `objectflow_store_bytes` | The store's file size. |
| `objectflow_wal_bytes` | The `-wal` file's size. |
| `objectflow_wal_cuts_total` | Times the `-wal` was cut back. |
| `objectflow_wal_cuts_missed_total` | Cuts a long read held off past their wait. |
| `objectflow_wal_cuts_deferred_total` | Cuts left for the next look, because a read was still running after the second a cut holds new reads. |
| `objectflow_disk_free_bytes` | Free bytes on the store's volume. |
| `objectflow_clock_lead_seconds` | How far the store's clock is ahead of the wall clock. |
| `objectflow_resident_bytes` | The process's resident memory. |
| `objectflow_memory_given_back_total` | Times freed memory was returned to the system. |
| `objectflow_log_lines_dropped_total` | Log lines dropped because nothing read them for 5 s. |
| `objectflow_build_info` | The release and commit serving. |
| `objectflow_start_time_seconds` | When the process started. |

**Alert on** these. Their names are held as `/health`'s fields are, from 1.0.0 (ADR-0183, ADR-0180). `/health` stays `200` while the clock leads, while an evaluator fails, while a publish holds the writer, and while the main listener refuses newcomers `503 busy`, so watch the metrics too:

- `/health` not `200`, on the `--metrics-bind` address;
- `objectflow_disk_free_bytes` well before 64 MiB;
- `objectflow_clock_lead_seconds` above 60;
- `objectflow_writer_held_seconds` above a minute: a publish or an import is running, or writes wait for another process's lock;
- the rate of `5xx` answers in `objectflow_requests_total`: a fault of the service's own, a full disk or a store changed under it;
- any rise in `objectflow_log_lines_dropped_total`, `objectflow_wal_cuts_missed_total` or `objectflow_connections_refused_total`;
- `objectflow_resident_bytes` near your `MemoryMax`;
- `objectflow_evaluator_failures_total` rising: an outside system is down, and its guards refuse `temporal`.

## Logs

The service writes to its standard error, each line stamped with its time:

- where it listens, once it answers;
- one line per answer: `<time> <method> <route> <status> <duration> ms`, with `clock_lead=<n>s` while the clock leads;
- each fault it meets, with its reason. A 5xx of one code is written at most once a second, the number left unsaid given with the next, so a full disk does not write a line per retried write;
- an evaluator's failure, naming its function but never the arguments or the answer, which may hold personal data, and its endpoint with any password hidden;
- at start: `--create` given for a store that exists, a last stop that was not clean, a `-wal` holding frames before opening, a store readable by others, another service holding the store, actors whose identities are equal but for case (see [Upgrading the engine](#upgrading-the-engine)), no `--attempt-retention`, an evaluator token to be sent over plain `http://` beyond loopback, and a clock that leads;
- writes waiting for another process's lock on the store, once, as the wait begins.

Read the standard error to its end, as journald does. The service writes the log from a thread of its own, through a queue of 4,096 lines, so no request ever waits on it: if nothing reads it, the queue fills, later lines are dropped (counted in `objectflow_log_lines_dropped_total`, and said in the next line written once the log is read again), and after 5 s `/health` answers `503 log_blocked`. Every answer is one line of 45 to 55 bytes, so a service answering 100 requests a second writes about 450 MB a day: size journald's limits, or `StandardError=append:<file>` with rotation, to it.

## Capacity

**Writes** run one at a time, each a durable commit of about 10 ms. A write that meets another process's lock on the store waits `--busy-timeout` up to four times, then is refused `stale` with cause `retries_exhausted`, after up to four more waits to record its refusal: up to eight times `--busy-timeout` in all; `/health` says `writes_stalled` from its first retry. Under load that took about 40 s in a measurement: give clients a write timeout above 45 s.

**Reads** run beside writes, each with its own deadline (`--read-timeout`). Reads whose cost grows with the store take a turn in one of five pools, so no kind of read waits behind another:

| Pool | Reads | Limit |
|---|---|---|
| worklists | `query` narrowed by an index and aggregating nothing, `available`, `pull` | `--max-reads` |
| objects | `history`, `events`, and a `get`, `availability` or `check` of an object whose rules it computes grow with the store | `--max-reads` |
| analytics | `metric`, `exceptions`, `diagnostics`, `export`, a `query` that reads its type whole, aggregates over a collection for each object, or computes a field that grows with the store | `--max-reads` |
| agents | every such read an agent actor makes | `--max-reads` |
| dry runs | a change's `submit` or `refresh`, a `publish` preview, a `check` of a change's submit | one at a time |

`get`, `lookup`, `availability`, `declaration`, `tools`, every other `check` and every write never wait for a pool, nor do `/health` and `/metrics`, except that a read of one object waits among the objects' reads where what it computes reads a metric, a type scan or the objects referring to it: a `get` where its type's derived attributes do, an `availability` where they or its conditions do, and a `check` where they, its conditions or its invariants do (64 `availability`s at once of a type guarded by a monthly metric peaked at 4.49 GB on six years in no pool). A `get` of a type with a metric guard therefore never waits, and none waits behind reports. Keep such rules off the transitions people's screens and agents ask about most. A read that waits past its deadline is answered `503 read_timeout`, having done nothing. A `query` is narrowed by an index where its filter requires the states it holds (`state == SUBMITTED`) or holds a column equal to a value (`claim.id == "claim-4"`); one ordered or filtered without either reads its type whole and waits among the analytics, so it never holds a person's worklist slot (a filter narrowed by a state that holds most of the type, `state == PAID and paid_at < …`, reads that state on each page in the worklists pool: mark `indexed` what such a filter compares), as does one whose filter aggregates over a collection for each object, itself or through a derived attribute it reads (`count(l in lines where …) > 3`), however an index narrows it. A page of `available` reads from its cursor until it holds its limit, so it costs what its page does where most objects in its states are open to the actor; where few are, it reads on through the state: raise `--max-reads` for people polling it, or build their list from a narrowed `query` and each object's `availability`.

**Heavy reads at once share the machine's cores**, each costing what it does alone: four filters that each took 2.2 s alone took 2.2 s each at once. Built with SQLite's shared page cache, which the bundled SQLite turns on and the repository's `.cargo/config.toml` turns off, they took 11.2 s each, every connection's page cache behind one lock: build inside the repository, where a build without the setting stops. Four pools take reads that grow with the store, so up to four times `--max-reads` run at once; past the cores, each slows in proportion, so on a machine with fewer cores than that, lower `--max-reads`.

**A read stops when its client goes**, at its next step, and gives its place in its pool to the next (ADR-0205 decision 16), where reads sent again after half a second each ran on to nobody until a seventeenth waited 30 s and was answered `503`. A write runs to its end whether its client waits or not, its key replaying it to a retry. Still give clients a read timeout above `--read-timeout`, so a read that would answer is not abandoned. An urgent cut of the `-wal` stops only the reads of a pool that did not drain in its moment's hold, never a short worklist beside a long report (decision 14).

**Requests** are at most 2 MiB. Import in batches under that, about 3,000 objects of an expense claim's size each. A client that sends a request's head and holds back its body keeps its connection's place until `--idle-timeout`, so a few such clients at the cap keep newcomers answered `503 busy` until then (`/health` on `--metrics-bind` still answers): put a reverse proxy that reads whole requests in front of the socket, or lower `--idle-timeout`, where clients you do not control share it.

**Measured** on a shared Intel i7-8700 with NVMe, release build, 2026-10-12, the stores' files then 126 MB and 387 MB; the publishing rows again on 0.5.1, idle, 2026-10-21, on stores `scripts/profile/build-store.py` built, of 275 MB and 824 MB, the same in each of three runs at two years and the second of two at six, the backfill's approval on the build after it, which made it grow with the store rather than with the square of the objects it writes (7.2 s and 32.5 s on 0.5.1). Measure your own deployment.

| | 2 years: 36.7k objects | 6 years: 110k objects |
|---|---|---|
| a `get` or a worklist query | 1 to 11 ms | |
| a write | about 10 ms | |
| publishing a version that adds an attribute: submit (runs beside writes) | 3.3 s | 10 s |
| … approval (holds writes) | 4.2 s | 12.5 s |
| … approval of a version whose backfill writes every claim (holds writes) | 5.4 s | 16.3 s |
| peak memory: submit / approval, adding an attribute (2026-10-20 and 21; see [Memory](#memory)) | 0.3 GB / 0.3 GB | 0.9 GB / 0.9 GB |
| a go-live import of 36k objects, 3,000 a batch | 36 s, each batch about 1.6 s | |
| a report, a metric over a type's whole history, p95 (2026-10-16, Gate P paced; 2026-10-14, each alone) | 0.22 to 0.78 s | 1.17 to 1.59 s |

Publishing grows linearly with the store: do it outside the busiest hours, and pause the creation of objects of a type whose migration is being approved: a creation during the approval refuses it `impact_unchanged`, after it has held writes for most of its run. Its approval holds writes for its whole run, longer while the read pools are busy: on six years, 25 s for an added attribute under load against 9 s idle, and 48.7 s for a new required attribute filled on every claim while sixteen metrics and eight `available` polls ran, against 17.9 s idle; measured again idle, 12.5 s on 0.5.1 and 16.3 s for a backfill of every claim on the build after it (32.5 s on 0.5.1), the loaded figures not measured again. Stop heavy readers (metrics, exports, pollers) before approving a version that fills an attribute on a large store, since a held write outlasts a client's 45 s timeout and a stop waits only 30 s: stop the service after a publish, never during one. A dry run has a deadline of its own, `--dry-run-timeout` (2 min unless given); one that passes it is answered `503 read_timeout` and records nothing: give a store whose dry run takes longer a longer one, which leaves every other read's deadline as it is.

**A report over a type's whole history** is a scan, linear in the store: past about two years at the profile's rate it does not answer within C5's 1 s, which is a stated limit and not a promise (ADR-0210 decision 2). Ask `referenced` which of your blob store's hashes the store still holds, after an erasure or on a schedule, a page's worth at a time, and delete the bytes of the rest: it reads the store whole, in 0.3 s for a 184 MiB store and 0.9 s for 550 MiB (measured 2026-10-17 on stores the release service made with `--create` and filled by imports of synthetic mirror objects, each a row and an import event holding a photo's hash and a 600-byte note, on a 12-core machine at a load of about 1, the file in the page cache), and costs a write nothing.

Read a team's reports over a window, as a dashboard does, with `over`: a metric over objects whose time dimension is the item's own time or date reads only the objects in the window, by that column, so its cost follows the window's activity and not the store's age (ADR-0210 decision 2); index the column with `indexed`. Measured 2026-10-16 on a sandbox store of 36,000 objects spread over six years, release build: a 90-day window, 1,480 objects, read in 5 ms at the median, where the whole history took 132 ms. A metric over transitions or intervals does not yet narrow by its window.

Some flow constructs cost more than they look:

- **A guard that reads a metric** computes the metric under the writer for each request it guards. A metric over objects bound to an attribute its item stores, as `claimant := claimant` binds `c.claimant`, reads only the objects holding the value, through that column; one bound otherwise, or not at all, reads its type whole, a per-claimant limit so computed taking 0.12 s a submit on 37k objects and twenty at once holding other writes about 2.7 s. Bind such a guard's metric to a stored attribute of its item, and put it on a rarely taken transition.
- **An attribute without `indexed`** is scanned when a filter reads it. Mark `indexed` what worklists and lookups filter by. References, unique attributes and external identifiers are indexed anyway.
- **A derived attribute computed from a set**, such as a claim's count of lines over policy, cannot be indexed, so a filter on it reads its type whole on every page: 3.4 s a page over six years of claims, and people running it at once waited for each other. Where a worklist filters by such a value, have the flow store it, written by the transitions that change it, and mark it `indexed`.
- **A filter comparing a time with `now`**, such as `entered_at(state) + 14 days <= now` for what has waited too long, is computed for each object of its type, so a page that finds few matches reads the type whole: 1.9 s over six years of claims where none matched. Narrow it by an indexed attribute, `state == Claim.SUBMITTED and entered_at(state) + 14 days <= now`, and it reads only those objects: 107 ms on the same store (2026-10-21).

## Memory

A publish holds the store's objects in memory while it simulates and installs the version, once, and a second copy of each object the version writes: on the two-year profile store, a 208 MB file of 36.6k objects, a change's dry run and its approval peaked at about 305 MiB resident where the version wrote no object, and at 325 and 410 MiB where a backfill wrote every claim (ADR-0222, ADR-0223); on the six-year store, an 824 MB file of 108.6k objects, at 873 and 872 MiB, and 940 and 1,192 MiB with the backfill (0.5.1, 2026-10-21), about 8 KiB an object on both. Plan for about twice the store's file size, measured on those two stores; `objectflow_resident_bytes` during a publish of yours shows its own. Reads that grow with the store add their own, at most `--max-reads` per pool at once. On 110k objects, sixteen per-person metrics sent at once peaked at 1.15 GB with the default limit of 4.

Size `MemoryMax` at twice a publish's peak, since one change's dry run may run beside another's approval, plus three times `--max-reads` times your heaviest read's peak, for the worklists, analytics and agents pools, plus `--max-reads` times the larger of about 25 MB and your heaviest one-object read's peak for the objects pool (forty pages of history held at once peaked at 1.07 GB; 64 `availability`s of a type guarded by a monthly metric, four at a time, 1.23 GB). A page of `query`, `history`, `pull` or `export` stops at 8 MiB, though building one costs more than its size (an `export` page of 8.5 MB added about 200 MB while it was answered), so a read's peak is about its page's size times three, plus what it reads to choose it, which on objects heavy with text is far more: a 9.8 MB page of them took 145 MB, twelve such pages at once across three pools 1.1 GB, given back about a minute after they ended. A page of `export` reads the store by its indexes, a few rows or a window of the log at a time, never the log or a type whole: a type's intervals by the event that last wrote each, its attempts by their transaction, the log's rows by position, an observation kind's records and a type's labels by their last event, and its attempt counts a day at a time. On six years of a 600-person company's claims a publish beside saturated reads peaked at 3.97 GB: give such a store 8G. Measure your own. After a publish, an import or any request of half a second or more, the service returns freed memory to the system (`objectflow_memory_given_back_total`).

## Backups and restore

**Back up a running store** with SQLite's online backup, which copies a consistent store while the service writes. Run it as the service's user, so the copy is readable by that user alone:

```sh
umask 077
python3 -c "import sqlite3; c = sqlite3.connect('/backup/store.sqlite'); sqlite3.connect('file:/var/lib/objectflow/store.sqlite?mode=ro', uri=True).backup(c); c.execute('INSERT OR REPLACE INTO of_counter (name, value) VALUES (?, 1)', ('copied',)); c.commit(); c.close()"
```

The command marks its copy as one, a row of the store's own `of_counter`, so a store started from it knows it is a restore and skips what the live store may have handed out after the copy (below); the service never marks a store it serves. It opens the store read-only, so a mistyped path is refused rather than made into an empty store and an empty copy. `VACUUM INTO '/backup/store.sqlite'`, run on a connection of your own, works too, and writes a compacted copy: mark it as the command does.

Copying the file alone is safe only with the service stopped: a clean stop folds the `-wal` into the file. A backup holds personal data, and an erasure cannot reach it: keep backups `0600`, and expire or encrypt them within the period your privacy rules allow.

**Schedule them.** There is no replica: what you can recover is the last backup. Run the backup from a timer as often as the data you can afford to lose allows (every 15 minutes is a common choice), copy each off the machine, encrypted, and check each copy before trusting it, opened as immutable so the check leaves no `-wal` or `-shm` beside it and needs no `sqlite3` program: `python3 -c "import sqlite3; c = sqlite3.connect('file:/backup/store.sqlite?immutable=1', uri=True); print(c.execute('PRAGMA quick_check').fetchone()[0], c.execute('SELECT release FROM of_engine_release ORDER BY seq DESC LIMIT 1').fetchone()[0])"` prints `ok` and the store's release, and refuses a copy that holds no store, which a quick check alone passes. Expire copies on the schedule your privacy rules set, so a value erased in the store does not outlive it in a backup.

A pair of units that takes one every 15 minutes, into a directory the service's user owns, and deletes the local copies older than a day; copying each off the machine and checking it are a command of your own, run after the backup and before the expiry:

```ini
# /etc/systemd/system/objectflow-backup.service
[Unit]
Description=ObjectFlow backup

[Service]
Type=oneshot
User=objectflow
UMask=0077
ExecStart=/usr/bin/python3 -c "import sqlite3, time; c = sqlite3.connect(time.strftime('/var/backups/objectflow/store-%%Y%%m%%dT%%H%%M.sqlite')); sqlite3.connect('file:/var/lib/objectflow/store.sqlite?mode=ro', uri=True).backup(c); c.execute('INSERT OR REPLACE INTO of_counter (name, value) VALUES (?, 1)', ('copied',)); c.commit(); c.close()"
# your off-machine copy and its check, for example:
# ExecStartPost=/usr/local/bin/ship-objectflow-backup /var/backups/objectflow
ExecStartPost=/usr/bin/find /var/backups/objectflow -name store-*.sqlite -mmin +1440 -delete

# /etc/systemd/system/objectflow-backup.timer
[Unit]
Description=ObjectFlow backup every 15 minutes

[Timer]
OnCalendar=*:0/15
Persistent=true

[Install]
WantedBy=timers.target
```

Make the directory first, `sudo install -d -o objectflow -g objectflow -m 0700 /var/backups/objectflow`, then enable it with `sudo systemctl enable --now objectflow-backup.timer`. The copy is consistent and one file, made from a read-only connection while the service writes, and marked as the one-off command marks its copy. Each copy is the whole store, so four an hour of a 130 MB store are about 12 GB a day, and of a 390 MB store about 38 GB: the expiry keeps a day of them, so size the volume for a day's copies and shorten `-mmin` if it is smaller, never lengthen it past what your copies off the machine keep. A replicator that ships the `-wal` as it grows, such as Litestream, is not tested beside the service, which checkpoints and truncates the `-wal` itself: check one against that before relying on it.

**Restore:**

1. Stop the service.
2. Remove the store's `-wal` and `-shm` files.
3. Put the backup in the store's place, with the same owner and mode `0600`.
4. Mark it as a copy, as the service's user, unless one of the commands above made it, which marked it already; a copy of the file taken with the service stopped is one to mark: `python3 -c "import sqlite3; c = sqlite3.connect('/var/lib/objectflow/store.sqlite'); c.execute('INSERT OR REPLACE INTO of_counter (name, value) VALUES (?, 1)', ('copied',)); c.commit(); c.close()"`. Marking a marked copy changes nothing.
5. Start the service.

Step 2 matters: a `-wal` left beside a restored copy would be replayed onto it, and where it is the copy's own later log, the restore is silently undone; the start then says the log held frames its file did not. By then it has taken them in, so removing the log afterwards does not help: stop the service, remove the `-wal` and `-shm`, put the backup in the store's place again, and start. Never copy a backup over the store's file while the service runs: its writes go on into a log that no longer matches the file, and are lost at the next stop, which the service cannot tell from its own checkpoints.

Step 4 matters too. A restored store holds nothing the live store did after the copy, though applications kept its ids and numbers: started from a marked copy, it advances every object number, event position, transaction and flow sequence past what the live store may have handed out since, so it hands none of them out twice (ADR-0207). The margin is the most the store recorded in any period as long as the copy's age, doubled, and at least 1,000, each counter going on from the next multiple of 1,000; the start says so in one line, naming the copy's last record and position and where each counter goes on from, and `of_restore` records the gap. Check that line against the highest ids and numbers your applications hold from after the backup: a period busier than twice any in the store's history is the one the margin does not cover. Unmarked, the copy hands those ids and numbers out again, as a release before 0.3.0 did.

What the restore lost is then the applications' to reconcile, and the store says where. An id kept from after the backup names nothing, `not_found`. A request retried under a key the store applied after the backup applies again, under a new id. A cursor a consumer kept from after the backup is refused `invalid_cursor` by `pull`, `export` and `acknowledge`: restart each subscriber with a `pull` sent without a cursor, which reads from the position the backup holds as acknowledged, and each `export` consumer from the copy's last position, which the start names. The service runs SQLite's quick check at each start, before anything opens the store for writing; if it fails, the service does not start, says why, and leaves every file as it was. The check reads the whole file: about 0.3 s for 126 MB, 0.8 s for 387 MB.

## Upgrading the engine

What a later release promises, from 1.0.0:

- Within `/v1`, a release only adds: operations, keys, fields, verdict kinds and reasons. A breaking change is `/v2`, served beside `/v1`.
- A flow valid under one release stays valid under every later release of the same major version. A check that would refuse it arrives first as a notice.
- A store is upgraded, never stranded.
- A major version gets security fixes for 12 months after the next major's first release.

**To upgrade:**

1. Stop the service.
2. Back up the store's file, and its `-wal` if one is beside it; on PostgreSQL, its database, `pg_dump --format=custom` or your provider's snapshot.
3. Start the new release on the store. It answers every operation `503 engine_mismatch`, and `/health` `503 upgrade_due`, until the store is upgraded.
4. As the operator, see the steps: `POST /v1/upgrade {"actor": {"id": "operator"}, "dry_run": true}`.
5. Run them: the same without `dry_run`. The service then serves the store.

**Identities equal but for case.** Release 0.2.0 took two actors whose identities differ only in the case of their ASCII letters, `Ana@x.org` and `ana@x.org`; later releases take them as one actor, so neither may leave or be erased, and every publish is refused `decision_unresolved`. A store holding such a pair says so at its start, naming the actors' object ids. Publish a version whose `migration`, which takes a `description`, admits the actor type's `identity_taken`, as `admit: [{ invariant: Employee.identity_taken, reason: Reason.LEGACY }]` with your type and a member of an enumeration of yours; the two actors may then leave and be erased, and a later version may give each an identity of its own. Until then a request naming the identity in a third case acts as the actor made first.

There is no downgrade: if the new release fails in service, restore the backup and run the old release. Once an upgrade commits, an old copy still running is refused; a copy of release 0.1.0 is not, so stop it before upgrading.

## The clock

A deployment uses the wall clock, but the store's clock never moves back. If the host's clock runs ahead and returns, every event is stamped at the latest time reached until the wall clock catches up. The service says so at start, on each log line, in `/health` (`clock_lead_seconds`, still `200`) and in `objectflow_clock_lead_seconds`. Fix the host's clock and the lead runs down. No operation moves the store's clock back, so keep the host synchronised and start the service after `time-sync.target`, with a wait-sync service enabled as [above](#run-it-under-systemd).

## Stopping

On SIGTERM or SIGINT the service stops taking connections, answers the requests in flight, and exits, saying so. SIGHUP, as a log rotation's `kill -HUP` sends it, is logged and ignored: the log goes to standard error, so there is nothing to reopen; rotate a log file with `copytruncate`. One still running after 30 s is cut, and rolled back. A process killed outright loses no acknowledged write: every acknowledged write is durable. Its clients may get no answer, so send every write with an idempotency key and retry an unanswered one with it.

## Housekeeping

Refused attempts and idempotency records grow with use. Prune them with `maintain`, within the retention the service was started with:

```json
POST /v1/maintain
{"actor": {"id": "operator"}, "task": {"name": "prune_attempts", "before": "2026-07-01T00:00:00Z"}}
```

`prune_idempotency` works the same way. A prune inside the retention is refused `inside_retention`, and one for a task the service has no retention for, `no_retention`, each answered `200` with its reason in `refused`. The event log is kept whole: `archive_events` answers `501`. A prune of 15,000 attempts took 0.1 s.

`rebuild_indexes` takes no `before`. Once an erasure has been recorded since the last rebuild, it rebuilds every index, so a value an erasure removed that an index page's free space kept is gone (below, Personal data on disk); with none recorded, it does nothing. It answers `moved`, the number of indexes rebuilt. It rebuilds them one at a time, each taking the writer alone: on a six-year store, 61 indexes took 4.1 s in all and the longest 0.6 s, which is the most a write waits for it. **On PostgreSQL** it writes every table anew instead (`VACUUM FULL`), its indexes with it, those backing keys among them, and the planner's samples last, since a vacuum frees what an erasure rewrote without zeroing it; `moved` counts them. Each table is locked against every read and write while it is written, which waits for the reads holding it and is answered `503` past the busy timeout, so run it in a quiet hour; how long it takes at the profile's size is not yet measured. The store's role must own its database, as the setup above makes it.

Run both daily, `before` set to the retention's edge. A script, installed as `/usr/local/bin/objectflow-prune`, `0755`, its retentions those of the unit above, which fails where a prune is refused:

```sh
#!/bin/sh
# Prunes refused attempts and idempotency records past the retentions the
# service runs with, and rebuilds the indexes after an erasure (operate.md,
# Housekeeping); fails where a task is refused, which the service answers
# 200 with its reason.
set -eu
SOCK=${SOCK:-/run/objectflow/objectflow.sock}
prune() {
  before=$(date -u -d "-$2" +%Y-%m-%dT%H:%M:%SZ)
  said=$(curl -sf --unix-socket "$SOCK" http://localhost/v1/maintain \
    -H 'Content-Type: application/json' \
    -d "{\"actor\": {\"id\": \"operator\"}, \"task\": {\"name\": \"$1\", \"before\": \"$before\"}}")
  echo "$1: $said"
  case "$said" in *'"refused":null'*) ;; *) exit 1 ;; esac
}
prune prune_attempts "${ATTEMPTS:-90 days}"
prune prune_idempotency "${KEYS:-30 days}"
said=$(curl -sf --unix-socket "$SOCK" http://localhost/v1/maintain \
  -H 'Content-Type: application/json' \
  -d '{"actor": {"id": "operator"}, "task": {"name": "rebuild_indexes"}}')
echo "rebuild_indexes: $said"
case "$said" in *'"refused":null'*) ;; *) exit 1 ;; esac
```

and a pair of units that runs it:

```ini
# /etc/systemd/system/objectflow-prune.service
[Unit]
Description=ObjectFlow prune of refused attempts and idempotency records
Requires=objectflow.service
After=objectflow.service

[Service]
Type=oneshot
User=objectflow
ExecStart=/usr/local/bin/objectflow-prune

# /etc/systemd/system/objectflow-prune.timer
[Unit]
Description=ObjectFlow prune, daily

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target
```

Enable it with `sudo systemctl enable --now objectflow-prune.timer`; `systemctl status objectflow-prune` shows a refused prune as a failed run.

## Personal data on disk

An erasure overwrites what it erased in the store's file and truncates the `-wal` after it commits, so neither file holds an erased value. A read open at that moment keeps the `-wal`'s earlier frames, so the service cuts the log back: within 5 s it holds new reads that grow with the store for at most a second, for the running ones to end, and truncates the log. The same cut runs whenever the `-wal` passes 64 MiB. Where a read is still running after that second, the cut is left for the next look, 5 s later, and the held reads go on (`objectflow_wal_cuts_deferred_total`); a person's read waits about a second for a cut, never for an agent's long sweep. A cut is urgent after an erasure, or once the `-wal` passes 256 MiB: then the reads still running are stopped, each answered `503 read_timeout` to be sent again, and the log is cut. A change's dry run is one of them: its checks hold no read, and its run over the objects stops as any read does. So is every long read, a metric or an export page among them: erasures in a steady loop, one every few seconds, stop the long reads they meet, so run a retention's erasures in batches, off-hours. A cut a read still held off is logged and counted in `objectflow_wal_cuts_missed_total`.

What an erasure reaches is what the flow marks `personal`, and nothing else. Free text in inputs not marked `personal`, and a request's `context`, keep what was typed, refused attempts included. Where an actor's identity attribute is `personal`, erasing the actor replaces its identity with its object id, in any case of its letters, wherever the engine records an actor and wherever a value of type `identity` holds it, however it got there: an attribute, a set, an event's and a proposal's inputs, a refused request's, a correction's record, a guard's consulted key and the approver of a version. An identity sent in an input of another type, a `string`, keeps what was sent: declare such inputs `identity`, or mark them `personal`. `history`, `events`, `pull` and `export` answer each event's actor by its identity, every other value of type `identity`, and each guard's `consulted` values, until then: where identities are personal, filter `actor_id`, every value of type `identity` and `consulted` from what you pass on, or give actors opaque identities.

SQLite overwrites what an erasure removes, but a value an index entry held may stay in a page's free space after the entry moves: 14 copies of an erased address were found in one index page after 4,000 rows holding it were rewritten. Nothing reads them through the service. The daily housekeeping above rebuilds every index after an erasure, `maintain`'s `rebuild_indexes`, so none is left past a day; `VACUUM` with the service stopped writes the whole file afresh as well. Keep backups encrypted. Better: give actors opaque identities, such as an employee number, never an email or a name, written in one Unicode form (the engine compares identities as written, ignoring only the case of ASCII letters).

A restore brings back what was erased after the backup was taken, so keep a record of erasure requests, outside the store, and ask again for each one made since the backup's time once a restore is serving.

An erasure that a loop or cascade cannot carry out within its declared bound is refused `over_limit`, naming the loop, and erases nothing. Publish a version whose bound covers the set, or that bounds the set with the invariant the checker's `unbounded_reach` notice names, then ask again.

# Write throughput: how comparable platforms scale it

Research, 2026-10-22. Every source was retrieved on 2026-10-22, and each figure below was checked against the page's raw text ([`README.md`](README.md)).

**The question.** The author recorded one writer per store as a known limitation, not sufficient long-term (`TODO.md`, "Write scale past one writer"), and asked how ObjectFlow's write throughput compares with platforms such as Jira, and how such platforms scale writes.

**The answer, in short.** The load profile Atlassian gives Jira app vendors writes at under two actions a second, and Jira Cloud limits one issue to about three writes a second sustained. ObjectFlow's one writer was offered 100 writes a second and answered them all within its latency target. Its ceiling, measured 2026-10-27, is about 400 writes a second on SQLite and between 100 and 125 on PostgreSQL with 32 sessions (§6). A cut to a PostgreSQL write's round trips has since raised it on cores kept awake (§6). The platforms that scale writes do it with one database behind many application nodes (Jira Data Center) or with many partitions of one writer each (Temporal), not with several writers on shared rows. These are orders of magnitude, not a benchmark: a Jira action and an ObjectFlow write are different units, on different hardware.

## 1. Jira Data Center: the app vendors' load profile

Atlassian's Data Center App Performance Toolkit is the load test that Marketplace app vendors run against Jira Data Center [S1, S2].

- **Load**, in the toolkit's `jira.yml` at commit `54943bdf90a4` [S1]: `concurrency: 200` ("number of concurrent virtual users for jmeter or locust scenario"), `test_duration: 45m`, `ramp-up: 3m`, `total_actions_per_hour: 54500`.
- **The action mix**, weights summing to 100 [S1]: `view_issue: 34`, `search_jql: 11`, `view_dashboard: 10`, `browse_projects: 9`, `view_scrum_board: 8`, `view_kanban_board: 7`, `view_backlog: 6`, `edit_issue: 5`, `create_issue: 4`, `view_project_summary: 3`, `add_comment: 2`, `browse_boards: 1`.
- **The dataset**, as the user guide's table names it, "Value for an enterprise-scale dataset" [S2]: Issues ~1 000 000, Comments ~6 000 000, Attachments ~2 000 000, Users ~21 000, Projects 500.
- **Derived here:** 54,500 actions an hour is 15.1 a second. The writes, `create_issue`, `edit_issue` and `add_comment`, are 11 of the 100 weights: about 6,000 an hour, **1.7 a second**.
- **Inferred, not checked:** a toolkit action is a JMeter or Locust user step, which may send several HTTP requests.

## 2. Jira Data Center: Atlassian's own scale test

Atlassian's performance and scale testing page for the Jira 10.3 LTS release [S3]:

- **Method:** scripted browsers, each running one scenario "repeatedly and without thinking time until at least 1000 results were gathered per scenario".
- **Environment:** 1 node, EC2 `c5d.9xlarge`, Java 17.0.11; database `db.m5.4xlarge`, Postgres 14; a load generator of 2 CPU cores.
- **Dataset:** Issues 5,407,147; Comments 8,408,998; Attachments 2,100,000; Users 82,725; Projects 4,299.
- **Write actions, achieved:** Create issue 828 ms at the 50th percentile and 838 ms at the 90th; Edit issue 595 ms and 718 ms; Add comment 630 ms and 789 ms; Edit sprint on backlog 403 ms and 425 ms.
- **No request rate is published.** These are browser-level times for an action, page rendering included.

An earlier test of clustered Jira, with no version stated [S4]: an Apache load balancer, a Postgres database, Jira on one, two or four machines, "9 GB of memory was allocated to the JVM", 450,000 issues and 10,000 users, each test 20 minutes. Its summary: "Throughput and capacity show a near linear increase as nodes are added to the cluster." Its own caveat: "you should not interpret the number of browsers to be equal to the number of real world users". Its action mix is mostly views, its writes being Edit Issue and Add Comment at 2 to 5 parts per user type.

## 3. Jira Data Center: how the cluster is built

- **One database behind active nodes** [S5]: "A database that all nodes read and write to. All application nodes are active and process requests." A load balancer spreads the traffic, and "A user will access the same Jira node for all requests until their session times out, they log out, or a node is removed from the cluster."
- **Availability first** [S5]: "If one node in your cluster goes down, the other take on the load"; "Running Jira Data Center in a cluster removes the application server as a single point of failure."
- **A search index per node, eventually consistent** [S6]: "when a user updates a Jira ticket on one Jira node, that node updates the Lucene index on both its local cache and in the shared database, and then tells the other nodes that it updated the index." Then: "The Lucene index is eventually consistent, meaning there will be some delay in updating the index across all nodes."
- **Caches replicated** [S6]: "each node in Jira Data Center replicates its memory cache across all other nodes using a specially configured Ehcache and RMI."
- **Shared and local files** [S5]: each node's local home holds "logs, caches, Lucene indexes and configuration files"; the shared home holds the rest, attachments among them.
- **Inferred:** every node's writes meet in the one database, so the cluster adds availability and read and search capacity, and its writes serialise where that database serialises them.

## 4. Jira Cloud: the limits it enforces

Jira Cloud's REST API documentation [S7]: "Jira Cloud enforces three independent rate limiting systems that work simultaneously".

- **Points per hour:** an app shares "a single 65,000 point hourly quota across all tenants" by default. After review, a per-tenant pool gives Standard "100,000 + 10 × users points/hour", Premium "130,000 + 20 × users", and Enterprise "150,000 + 30 × users". "Per-tenant rate limits are capped at 500,000 points per hour for Standard, Premium, and Enterprise editions."
- **Bursts per endpoint**, a token bucket per tenant and endpoint, default steady state in requests a second: GET 100, POST 100, PUT 50, DELETE 50.
- **Writes per issue:** "Short window : 20 write operations per 2 seconds"; "Long window : 100 write operations per 30 seconds". Past either, "429 Too Many Requests" with `RateLimit-Reason` `jira-per-issue-on-write`.
- **Scope:** "API token-based traffic is not affected by this change, and will continue to be governed by existing burst rate limits."
- **Derived here:** 100 writes per 30 seconds is 3.3 writes a second to one issue, sustained.

## 5. Temporal: partitions of one writer each

Temporal's server documentation [S8]:

- "A History Shard is an important unit within a Temporal Service by which concurrent Workflow Execution throughput can be scaled."
- "Each History Shard maps to a single persistence partition. A History Shard assumes that only one concurrent operation can be within a partition at a time. In essence, the number of History Shards represents the number of concurrent database operations that can occur for a Temporal Service."
- "Temporal recommends starting at a ratio of 1 History Service process for every 500 History Shards." "the total number of History Shards remains static for the life of the Temporal Service".

Temporal's blog, "Scaling Temporal: The basics" (May 23, 2023) [S9]:

- "Before a Workflow history can be created or updated, there is a shard lock that must be obtained."
- On a test cluster at the development default of 4 shards, "shard lock latency p95 is nearly 50ms", where "we'd expect shard lock latency to be less than 5ms, ideally around 1ms"; rebuilt with 512 shards, it "dropped from around 50ms to around 1ms".
- **How it measures capacity:** "State Transitions per second" as the throughput metric, "qualify it with some other metrics for business or operational goals (commonly called Service Level Objectives or SLOs)", the load test's being a `StartWorkflowExecution` latency "of less than 150ms".

## 6. ObjectFlow beside them

| | Writes a second | Conditions |
|---|---|---|
| Jira Data Center, the toolkit's enterprise-scale profile (§1) | about 1.7 (derived) | 200 virtual users, ~1M issues |
| Jira Cloud, one issue's sustained limit (§4) | 3.3 (derived) | a limit, not a capacity |
| ObjectFlow, writes alone at [ADR-0204](../adr/0204-what-production-readiness-means-and-when-the-reviews-stop.md) §1's month-end peak, twice its steady 50 | 100 offered, 101 answered at p95 34 ms | one writer, requests waiting for it committed together ([ADR-0208](../adr/0208-requests-waiting-for-the-writer-commit-together.md)); SQLite, the two-year profile store, a shared i7-8700 with NVMe, 2026-10-14 (`TODO.md`, A1) |
| ObjectFlow, one request a commit | 87 answered at p95 406 ms | the same, before ADR-0208 |
| ObjectFlow's grouped writer, its ceiling (§7 item 4) | SQLite: 400 at p95 94 ms. PostgreSQL: 100 at p95 53 ms with 32 sessions, 75 at p95 83 ms with 64 | the first table below, 2026-10-27 |
| ObjectFlow, eight stores on one PostgreSQL server | 800 together, each store's p95 within 26 ms | the second table below, 2026-10-27 |

When the writer committed one request at a time, the disk's flush bounded it: 7.2 ms of a draft's 10.6 ms at p50, the deciding and writing about 3.4 ms (ADR-0208, measured 2026-10-14).

Grouped, the writer's ceiling on this machine is about 400 writes a second on SQLite: 400 passed with 32 sessions and with 64, and 500 failed with 64. On PostgreSQL it is between 100 and 125 with the month-end peak's 32 sessions, and between 75 and 100 with 64. Where its sessions could offer them, each backend answered at least 99.6% of the writes offered, up to 600 a second on SQLite and 200 on PostgreSQL: a rate past the ceiling is answered late, not refused. A session waiting on its write offers no other, so 64 sessions offered SQLite at most 693 a second, and 32 offered it at most about 465. PostgreSQL's commits held 1.2 to 1.6 requests each, read from the service's `/metrics`.

The machine's idle cores account for much of PostgreSQL's tail. Its CPUs ran the `powersave` governor, with idle states down to C10. With busy loops at the lowest priority on every thread, one store offered 100 writes a second by 16 sessions had a p95 of 25 ms in two runs, against 76 to 134 ms in three runs without them. With the loops, PostgreSQL's ceiling at 32 sessions rose to between 150 and 200. The loops also take each hyperthread's sibling, which slows the work itself: past the ceiling they made the p95 worse, 186 ms at 200 a second against 129 ms without. Which of the cores' idle states or clock speeds accounts for the gain is not measured. *Inferred:* each of a write's round trips to PostgreSQL waits on a core's wake-up, and SQLite, inside the service's process, makes none. What a write on PostgreSQL spends its time on is below.

Each cell is the writes' p95, past C5's 100 ms in bold:

| Writes offered a second | SQLite, 64 sessions | SQLite, 32 | SQLite, 32, cores kept awake³ | PostgreSQL, 64 | PostgreSQL, 32 | PostgreSQL, 32, cores kept awake³ |
|---|---|---|---|---|---|---|
| 50 | | | | 35 ms¹ | | |
| 75 | | | | 83 ms² | | |
| 100 | 48 ms² | 49 ms² | | **136 ms², 142 ms², 149 ms¹** | 53 ms² | |
| 125 | | | | | **137 ms** | |
| 150 | | | | **182 ms** | **153 ms** | 95 ms |
| 200 | 33 ms² | 67 ms¹ | | | **129 ms** | **186 ms** |
| 300 | 39 ms², 64 ms¹ | 74 ms¹ | | | | **241 ms**, 218 answered a second |
| 400 | 89 ms², 95 ms | 94 ms | 72 ms | | | |
| 500 | **118 ms** | | 89 ms, 463 answered a second | | | |
| 600 | **109 ms², 126 ms** | | 88 ms, 467 answered a second | | | |
| 800 | **125 ms²**, 693 answered a second | | | | | |
| 1,000 | **129 ms²**, 680 answered a second | | | | | |

¹ Other processes took a mean of half a thread or more during the run. ² Other processes' load was not recorded during the run. ³ Beside busy loops at nice 19 on every thread, which took a mean of 10 to 11 threads. During every other run, other processes took a mean of 17% to 32% of one thread.

**Several stores on one PostgreSQL server.** Eight stores answered 800 writes a second together, each store's p95 within 26 ms. Each store's p95 fell as stores were added, as the busy loops lowered a lone store's. The server's predicate-lock table held at most 1,297 of its 15,616 locks, 8%. Its 100 connections filled first: in the first of three eight-store runs at 50 a second each, the server refused five connections, and five writes were answered `500`. A write's only new connection was the read its answer took after its commit, so they had applied, a defect since fixed (`CHANGELOG.md`).

| Stores | Writes offered a second, each | Each store's writes' p95 | SIRead locks at their peak, of 15,616 |
|---|---|---|---|
| 1 | 50 | 60 ms¹ | 38 |
| 2 | 50 | 49 ms | 136 |
| 4 | 50 | 34 to 38 ms | 417 |
| 8 | 50 | 23 to 30 ms; 22 to 25 ms, and 22 to 25 ms¹ | 854; 1,252 and 1,297 |
| 1 | 100 | **134 ms**, **123 ms**, 76 ms¹ | 52, 47, 41 |
| 1, cores kept awake³ | 100 | 25 ms, 25 ms | 43, 51 |
| 2 | 100 | 51 to 52 ms | 170 |
| 4 | 100 | 34 to 44 ms | 434 |
| 8 | 100 | 22 to 25 ms | 971 |

During its unmarked runs, other processes took a mean of 14% to 39% of one thread. Each store had 16 sessions and its own copy of the two-year store in a database of its own, the copies checkpointed before each case. The locks were sampled every 0.2 s over each run from `pg_locks`. The table holds `max_pred_locks_per_transaction` × (`MaxBackends` + `max_prepared_transactions`) targets and twice as many locks (PostgreSQL 16's `predicate.c` and `postinit.c`): 64 × 122 and 15,616 at the server's defaults.

Measured 2026-10-27 with `scripts/profile/load.py --writes`, 90 s a rate. Each write is a claim's `draft`, offered among the sessions' reads, with no report readers or agents. The service was the release build of `127de44`, on two-year stores `build-store.py` built that morning. PostgreSQL 16.8 ran in Docker beside the service at its default settings, its data on disk, reached at the container's address. Each PostgreSQL run used its own copy of the store, checkpointed before the run. The machine was a shared Intel i7-8700, 12 threads, with NVMe.

**A PostgreSQL write's cost by part.** In 0.9.0's build, a draft sends the server 29 statements, each its own round trip: the server's log holds 29 a draft, and strace counted 29 sends. `pg_stat_statements` counts 30, the 30th a foreign key's check, which the server runs itself. The server committed about four transactions a draft (3.7 to 4.1): the write's, and three statements after it, each a transaction of its own. On cores kept awake, a draft took a mean of 7.1 ms at the client. The WAL's flush, about one a draft, took 2.8 ms, the server's execution of its statements 1.0 ms and the service's CPU 1.6 ms. Its 29 round trips at pgbench's 0.059 ms make 1.7 ms. *Inferred:* these four parts are the whole draft, since they sum to its mean. With the cores idle between drafts, every part grew: of a 24.1 ms mean, the flush took 9.4 ms, the execution 2.6 ms and the service's CPU 5.7 ms. Which of the cores' clock speeds, their idle states or the disk's power states accounts for each part is not measured.

| A draft, one at a time | Cores idle, 0.1 s apart | Cores idle, back to back | Cores kept awake, 0.1 s apart |
|---|---|---|---|
| At the client: p50, p95, mean | 28.2, 33.0, 24.1 ms | 15.9, 19.9, 12.7 ms | 6.5, 11.4, 7.1 ms |
| The WAL's flushes, and their time | 1.04, 9.4 ms | 1.01, 2.3 ms | 0.98, 2.8 ms |
| The server's execution of the draft's statements | 2.6 ms | 1.9 ms | 1.0 ms |
| The service's CPU | 5.7 ms | 4.1 ms | 1.6 ms |
| The database container's CPU, the pauses included | 7.7 ms | 5.4 ms | 2.3 ms |

pgbench's prepared `SELECT 1`, from the host's network to the container, took 0.47 ms a round trip at 10 a second with the cores idle and 0.059 ms beside the busy loops. Back to back, it took 0.025 ms.

**Where a draft's round trips go.** In 0.9.0's build, all 29 run on the writer's connection, in this order:

| Statements | Round trips |
|---|---|
| `BEGIN`, and the declared version | 2 |
| The snapshot: the engine's release, every subscription's cursor, every admission | 3 |
| The allocator: the counter, every sequence | 2 |
| The decision's reads: the actor, in four; the claimant, in two; the store's last row; the new claim's id; its number, twice; its lines | 11 |
| The writes, and `COMMIT` | 8 |
| After the commit, the answer's snapshot: the engine's release, every subscription's cursor, every admission | 3 |

The answer's snapshot is `request_told`'s, taken on the writer's store once its transaction ends (`request.rs`); the service holds the writer until the request is answered (`run_group`, `main.rs`). In a group, each request takes both snapshots inside the group's transaction (`transact_grouped`, `store.rs`). In 0.9.0's build every snapshot read all of `of_subscription_position` and every position in `of_admission`, which one subscription's read and one expression use (`subscription.rs`, `eval.rs`). The claim's number is read twice with the same value. *Inferred:* a group's commit shares the flush (ADR-0208), and not these round trips, which each write makes in turn on the one writer. Which to cut is §8's.

Measured 2026-10-27 with a one-off script, 200 drafts a condition after 10 to warm the service, one at a time. Each condition ran on its own copy of the two-year store, checkpointed before. The parts are the server's counts over the drafts: `pg_stat_statements` for the statements, `pg_stat_wal` with `track_wal_io_timing` on for the flushes, and `pg_stat_database` for the commits. The CPU is the container's cgroup and the service's `/proc` ticks. A separate run of 50 drafts under strace counted the sends; its times are not used. The statements' order and transactions are PostgreSQL's log of every statement (`log_statement = 'all'`), over five drafts on a store of 200 claims the same build made, 2026-10-29: four drafts' statements were alike, and the first after the service started made one more. The busy loops ran at nice 19 on every thread, their CPU not sampled. The service was the release build of `2c5ea72`'s tree, on a store it built that hour. PostgreSQL 16.8 ran in Docker as above, at its defaults but for `pg_stat_statements` loaded and WAL timing on. The governor was `powersave`. Other processes' load was not sampled during the runs; the load average at each start was 0.7 to 1.0.

**The first cut, before against after.** An operation now reads the cursors and the admissions where it reaches them, as ADR-0162 reads the objects and the log (`design/scoped-reads.md` §7). A draft then sends 25 statements in two transactions; each snapshot reads only the engine's release. One draft at a time, the cut is within the runs' spread: the WAL's flush alone varied from 2.7 to 4.0 ms a draft between runs. The server committed two transactions a draft where it committed four, and its container's CPU a draft fell by about 0.15 ms. At 32 sessions and 150 writes a second the writes' p95 did not move. At 200 a second both runs after answered faster than both runs before. *Inferred:* near the ceiling, the writer's shorter hold shortens the queue behind it. The ceiling with the cut is measured after the next table.

| Condition | Before | After |
|---|---|---|
| One at a time: p50 | 6.38, 6.51 ms | 6.51, 6.29 ms |
| One at a time: mean | 7.13, 7.68 ms | 8.94, 6.58 ms |
| One at a time: commits a draft | 3.89, 4.05 | 2.04, 1.96 |
| One at a time: the database container's CPU a draft | 2.27, 2.35 ms | 2.16, 2.15 ms |
| 32 sessions, 150 writes a second: writes' p95 | 43.8, 49.0 ms | 47.1, 41.9 ms |
| 32 sessions, 200 writes a second: writes' p50 | 16.2, 16.0 ms | 13.0, 10.4 ms |
| 32 sessions, 200 writes a second: writes' p95 | 82.4, 86.0 ms | 58.4, 46.5 ms |

Measured 2026-10-29 with a one-off script. Two release builds of `15ee16e`'s tree ran, without the cut and with it. Each run used a copy of one two-year store the first built that hour, marked and checkpointed as above. Each condition ran in the order before, after, after, before, so a drift through the session weighs on both builds alike. The drafts were 200 one at a time, 0.1 s apart, after 10 to warm, counted as above. The load was `load.py --writes` with 32 sessions, no report readers and no agents, for 90 s. The busy loops ran at nice 19 on every thread throughout, and the load average rose from 2.1 to 15 over the session. PostgreSQL 16.8 ran in Docker as above, with `pg_stat_statements` loaded and WAL timing on, and the governor was `powersave`. `pg_stat_statements` counted 30 and 26 statements a draft, the foreign key's check among them. The statement count is PostgreSQL's log of every statement over five drafts, as above, on the build with the cut: four drafts sent 25, and the first after the service started 26. At 200 a second the build before passed C5 here; on 2026-10-27 the same condition failed at 186 ms. Only runs within one session compare.

**The ceiling after the first cut.** At 32 sessions on cores kept awake, the cut raised the writer's ceiling from between 200 and 250 writes a second to between 250 and 300. At 250 a second both runs with the cut passed C5, at 98.5 and 97.9 ms, and both runs without it failed, at 110.0 and 108.7 ms. Past its ceiling each build answered about one rate, whatever was offered: 285 to 288 writes a second without the cut, 301 to 311 with it. The same client at 32 sessions, on cores kept awake, had SQLite answer 463 and 467 a second (the SQLite columns above), so the client did not set those rates. *Inferred:* there the one writer is busy throughout, and the rate it answers is its capacity: a write takes 3.5 ms of it without the cut and 3.2 ms with it. Each session then waits on one write, so the writes' p50 is near the sessions over the rate answered: 32 over 287 a second is 111 ms, and the build without the cut measured 109 to 111 ms.

Each cell is a build's two runs, in the order they ran, the writes' p95 past C5's 100 ms in bold:

| Writes offered a second | Before: writes' p95 | Before: answered a second | After: writes' p95 | After: answered a second |
|---|---|---|---|---|
| 200 | 78.7, 78.9 ms | 202.2, 202.2 | 59.1, 57.3 ms | 202.2, 202.1 |
| 250 | **110.0**, **108.7 ms** | 252.7, 252.8 | 98.5, 97.9 ms | 252.7, 252.6 |
| 300 | **128.2**, **129.4 ms** | 287.3, 284.7 | **121.4**, **114.3 ms** | 303.2, 303.2 |
| 350 | **130.6**, **127.5 ms** | 285.5, 287.8 | **120.0**, **119.4 ms** | 308.7, 309.3 |
| 400 | **127.9**, **127.9 ms** | 288.1, 286.2 | **118.8**, **130.2 ms** | 310.8, 301.0 |

A rate answered above the rate offered comes from the client's first second, whose bucket starts full (`load.py`). Measured 2026-10-30 with a one-off script, in one session, as the runs before against after above, at the rates in the table. The two builds were those of 2026-10-29, their SHA-256 the same. Each run used a copy of one two-year store the build before built that hour. Each rate ran in the order before, after, after, before, for 90 s with 32 sessions, no report readers and no agents. The busy loops ran throughout; the load average was 2.5 at the first run's start and 12.9 to 16.9 at every later one. PostgreSQL 16.8 ran as above, and the governor was `powersave`.

**With the cores idle.** Without the busy loops, the runs scattered too widely to place a ceiling for either build. At 100 writes a second every run failed C5, at 118.7 to 150.1 ms. At 125 both runs with the cut passed and both without it failed. At 150 one run of each build passed. At 200 and 250 every run failed, past 230 ms, and at 250 both builds answered 232 to 234 writes a second. The first two runs had a p50 about three times the later runs' at 100 a second; what slowed them is not measured. Gate P on PostgreSQL passed at 0.9.0, at the same 100 writes a second with its report readers and agents, with a writes' p95 of 65.6 ms (`TODO.md`). Why these runs of writes alone answered slower is not measured.

| Writes offered a second | Before: writes' p95 | After: writes' p95 |
|---|---|---|
| 100 | **149.5**, **118.7 ms** | **150.1**, **123.6 ms** |
| 125 | **102.0**, **106.9 ms** | 69.8, 96.8 ms |
| 150 | 73.5, **121.7 ms** | **121.4**, 88.2 ms |
| 200 | **231.5**, **272.2 ms** | **292.4**, **308.4 ms** |
| 250 | **313.3**, **307.4 ms** | **303.8**, **296.6 ms** |

Measured 2026-10-30 in a second session, as the ceiling above but without the busy loops. The builds were the same, on copies of a store the build before built that hour. Each rate ran in the order before, after, after, before. The load average at each run's start was 0.9 to 5.5.

## 7. The patterns, and where each applies

1. **Many application nodes over one database** (Jira Data Center, §3). It buys availability and read and search capacity across nodes. The database's transactions order the writes. ObjectFlow keeps this design as reference for several writing instances: row locks and serialisable retries ([ADR-0090](../adr/0090-concurrency-as-it-behaves.md)), sequences on a second connection ([ADR-0076](../adr/0076-the-sequence-store-is-a-second-connection-and-a-replay-precedes-the-version-check.md), kept by [ADR-0225](../adr/0225-the-sequence-store-is-the-stores-own-table.md) decision 4), and the cursor read below `xmin` ([ADR-0089](../adr/0089-the-log-is-appended-in-transaction-order-and-read-by-a-settled-cursor.md)). With today's schema such writers would still serialise on `of_counter`'s row, which every write updates (`storage-schema.md` §7, [ADR-0034](../adr/0034-event-ordering-and-subscriptions-as-a-built-in-type.md)).
2. **Partitions of one writer each** (Temporal, §5). Throughput is the number of partitions, and each partition stays serial. It fits work keyed to one partition, a workflow execution there. ObjectFlow's guarantees reach across objects: a guard reads other objects, an invariant may scan a type, and a cascade writes many objects (`DESIGN.md` §5.6, §6). A request touching two partitions would need a transaction across them. *Inferred:* this pattern fits ObjectFlow at the level of the store, one service per store (PRD N7), and not inside a store.
3. **Limits per tenant and per object** (Jira Cloud, §4). A tenant's quota and one issue's write limit keep a hot spot from taking the shared capacity. ObjectFlow's service caps connections and bounds reads by pool (`docs/guides/operate.md`, "Capacity"), and limits no object's or caller's write rate. A hot row is a known limit, which serialisable isolation keeps correct (`edge-cases.md`, "Hot rows").
4. **Capacity as the highest load that meets an objective** (Temporal, §5). Throughput is measured in the system's own unit of work and qualified by a latency objective. For ObjectFlow, the unit is a write and the objective is C5's 100 ms at p95 (PRD C5). The writer's ceiling is the highest offered rate whose p95 stays within it.

## 8. What it suggests (for the author)

A recommendation, not a decision. Each step adds what the platforms above add, in the order they add it:

1. **Measure the grouped writer's ceiling** on both backends, as §7 item 4 defines it. *Measured 2026-10-27, §6, with a PostgreSQL write's cost by part and where its 29 round trips go. A group's commit shares the flush and not the round trips. The first cut, the cursors and the admissions read where an operation reaches them, was made on 2026-10-29 and leaves 25 (§6). On cores kept awake it raised the ceiling at 32 sessions from between 200 and 250 to between 250 and 300 (§6, measured 2026-10-30). Which further round trips to cut is a change to the write path, the author's (`TODO.md`, "Write scale past one writer").*
2. **A standby service**, for availability, which is the first thing Jira's cluster gives. A second service on a PostgreSQL store is refused today, not held waiting (`docs/guides/operate.md`, "A store on PostgreSQL").
3. **Read replicas** for reports and metrics, a cost the production reviews met repeatedly (READ-COST, READS and MEMORY in `TODO.md`).
4. **Stores by domain**, the partitioning of §7 item 2 at the store's level. *Measured 2026-10-27: eight stores on one PostgreSQL server answered 800 writes a second together within C5 (§6).*
5. **Several writers over one PostgreSQL database**, §7 item 1, only on a measured need, positions moved off `of_counter` first.

## Sources

| | Source | Version | What it supports |
|---|---|---|---|
| S1 | [`app/jira.yml`](https://raw.githubusercontent.com/atlassian/dc-app-performance-toolkit/master/app/jira.yml), atlassian/dc-app-performance-toolkit | commit `54943bdf90a4d3de3f876aac0c921a354014b490`, 2026-06-17 | §1 load and mix |
| S2 | [DC Apps Performance Toolkit user guide for Jira](https://developer.atlassian.com/platform/marketplace/dc-apps-performance-toolkit-user-guide-jira/) | none stated | §1 dataset |
| S3 | [Performance and scale testing](https://confluence.atlassian.com/adminjiraserver/performance-and-scale-testing-1483246093.html), Jira Data Center | Jira 10.3 LTS | §2 |
| S4 | [Jira Data Center Performance](https://confluence.atlassian.com/display/ENTERPRISE/Jira+Data+Center+Performance) | none stated | §2, the earlier test |
| S5 | [Running Jira Data Center in a cluster](https://confluence.atlassian.com/adminjiraserver/running-jira-data-center-in-a-cluster-993929598.html) | the unversioned page | §3 topology, availability, homes |
| S6 | [Scaling Jira Data Center](https://confluence.atlassian.com/enterprise/scaling-jira-data-center-939699677.html) | none stated | §3 index, caches |
| S7 | [Rate limiting](https://developer.atlassian.com/cloud/jira/platform/rate-limiting/), Jira Cloud platform | none stated | §4 |
| S8 | [Temporal Server](https://docs.temporal.io/temporal-service/temporal-server) | none stated | §5 shards |
| S9 | [Scaling Temporal: The basics](https://temporal.io/blog/scaling-temporal-the-basics), Rob Holland | May 23, 2023 | §5 shard lock, SLOs |

# Write throughput: how comparable platforms scale it

Research, 2026-10-22. Every source was retrieved on 2026-10-22, and each figure below was checked against the page's raw text ([`README.md`](README.md)).

**The question.** The author recorded one writer per store as a known limitation, not sufficient long-term (`TODO.md`, "Write scale past one writer"), and asked how ObjectFlow's write throughput compares with platforms such as Jira, and how such platforms scale writes.

**The answer, in short.** The load profile Atlassian gives Jira app vendors writes at under two actions a second, and Jira Cloud limits one issue to about three writes a second sustained. ObjectFlow's one writer was offered 100 writes a second and answered them all within its latency target, and its ceiling is not yet measured. The platforms that scale writes do it with one database behind many application nodes (Jira Data Center) or with many partitions of one writer each (Temporal), not with several writers on shared rows. These are orders of magnitude, not a benchmark: a Jira action and an ObjectFlow write are different units, on different hardware.

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

What bounds ObjectFlow's writer is the disk's flush: 7.2 ms of a draft's 10.6 ms at p50, the deciding and writing about 3.4 ms (ADR-0208, measured 2026-10-14). No rate above 100 a second has been offered, so the grouped writer's ceiling is unknown (`TODO.md`, "Write scale past one writer").

## 7. The patterns, and where each applies

1. **Many application nodes over one database** (Jira Data Center, §3). It buys availability and read and search capacity across nodes. The database's transactions order the writes. ObjectFlow keeps this design as reference for several writing instances: row locks and serialisable retries ([ADR-0090](../adr/0090-concurrency-as-it-behaves.md)), sequences on a second connection ([ADR-0076](../adr/0076-the-sequence-store-is-a-second-connection-and-a-replay-precedes-the-version-check.md), kept by [ADR-0225](../adr/0225-the-sequence-store-is-the-stores-own-table.md) decision 4), and the cursor read below `xmin` ([ADR-0089](../adr/0089-the-log-is-appended-in-transaction-order-and-read-by-a-settled-cursor.md)). With today's schema such writers would still serialise on `of_counter`'s row, which every write updates (`storage-schema.md` §7, [ADR-0034](../adr/0034-event-ordering-and-subscriptions-as-a-built-in-type.md)).
2. **Partitions of one writer each** (Temporal, §5). Throughput is the number of partitions, and each partition stays serial. It fits work keyed to one partition, a workflow execution there. ObjectFlow's guarantees reach across objects: a guard reads other objects, an invariant may scan a type, and a cascade writes many objects (`DESIGN.md` §5.6, §6). A request touching two partitions would need a transaction across them. *Inferred:* this pattern fits ObjectFlow at the level of the store, one service per store (PRD N7), and not inside a store.
3. **Limits per tenant and per object** (Jira Cloud, §4). A tenant's quota and one issue's write limit keep a hot spot from taking the shared capacity. ObjectFlow's service caps connections and bounds reads by pool (`docs/guides/operate.md`, "Capacity"), and limits no object's or caller's write rate. A hot row is a known limit, which serialisable isolation keeps correct (`edge-cases.md`, "Hot rows").
4. **Capacity as the highest load that meets an objective** (Temporal, §5). Throughput is measured in the system's own unit of work and qualified by a latency objective. For ObjectFlow, the unit is a write and the objective is C5's 100 ms at p95 (PRD C5). The writer's ceiling is the highest offered rate whose p95 stays within it.

## 8. What it suggests (for the author)

A recommendation, not a decision. Each step adds what the platforms above add, in the order they add it:

1. **Measure the grouped writer's ceiling** on both backends, as §7 item 4 defines it.
2. **A standby service**, for availability, which is the first thing Jira's cluster gives. A second service on a PostgreSQL store is refused today, not held waiting (`docs/guides/operate.md`, "A store on PostgreSQL").
3. **Read replicas** for reports and metrics, a cost the production reviews met repeatedly (READ-COST, READS and MEMORY in `TODO.md`).
4. **Stores by domain**, the partitioning of §7 item 2 at the store's level.
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

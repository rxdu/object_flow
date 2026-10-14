<!-- The audit of ADR-0204 §6, area as named, at commit 698034b, 2026-10-14: the reviewer's inventory as returned, kept so each item's fix and check can be traced (ADR-0205). Its scripts and stores stayed in the session's scratch directory, which is not kept. -->

# Audit 4: cost, pools and robustness (promise C), the inventory

Checkout `698034b`, the release binary `bin/objectflow-service` (`objectflow-service 0.2.0 … commit 698034b`), measured 2026-10-14 on the reference machine (i7-8700, 12 threads, NVMe, 31 GiB). Graded by ADR-0204 §§1–5.

**Measurement conditions, stated once.** The machine was shared throughout: five other auditors and a GitHub Actions runner kept the load average at 5–20 (logged per run in `results/*.md`). Every latency below is as measured under that load; a quieter machine would give lower numbers, most of all for the CPU-bound reads. Each run lasted 60–160 s, not 24 h: the soak of §1 was not run. Stores: `run/g2a.sqlite` and `run/g6a.sqlite` (deleted after the runs; the provided originals remain), copies of the provided `g2.sqlite` (12,000 claims, 36.6k objects) and `g6.sqlite` (36,000 claims, 108.6k objects). Each ran as a deployment on a Unix socket with `--attempt-retention "90 days" --idempotency-retention "30 days"`, the stub evaluator `tools/evaluator.py` answering `satisfied`, and an agent `claims-agent` registered. Drivers, all in this directory: `perf2.py` (one client, every operation), `load2.py` (the §1 profile), `flood.py`, `heavymem.py`, `bounds.py`, `ops.py`, `attempts.py`, `labels.py`. Raw results are in `results/` and `logs/`. "Reproduced" means measured on the release binary with the command named; "unconfirmed" means read from the code (ADR-0204 §5).

## 1. How the area was enumerated, and why that is complete

- **Operations.** The router `app()` (`main.rs:3448-3495`) is the only way into the service: 21 `POST /v1/<operation>`, `GET /health` and `GET /metrics`, plus four sandbox controls. I cross-checked it against `TAKES` (`main.rs:1170-1216`) and `wire.md`'s tables. For each handler I recorded:
  - the pool it waits in: `heavy_reading` `main.rs:849`, `object_slot` `main.rs:876`, `pool_of` `main.rs:907`, `dry_running` `main.rs:2687`;
  - the read it takes: `reading`/`reading_until` `main.rs:943-993`;
  - or the writer: `locked` `main.rs:1381`;
  - its deadline and its input bounds.

  I measured every operation one client at a time on both stores (Table A), and the ones the profile uses under the profile (Table B).
- **Pools and concurrency.** These are the five `Admission`s, built at `main.rs:5381-5385` (`worklists`, `analytics`, `objects`, `agents`, `dry_runs`); the writer mutex; the read connections `Readers` (`store.rs:1844-1887`); the connection cap (`served`, `main.rs:4767-4899`); and the runtime's threads (`main.rs:4153-4158`).
- **Every loop over a type or the log.** A read of a set that grows with the store can reach SQLite only through `World`'s and `Store`'s accessors, since `World::conn` is private (`world.rs:453`). I listed every non-test call of the unbounded ones:

  | Accessor | What it reads whole |
  |---|---|
  | `World::log` (`world.rs:703`) | the log |
  | `attempts` (`world.rs:850`) | the attempt log |
  | `rollup` (`world.rs:886`) | the attempt rollup |
  | `in_order` (`world.rs:1166`) | every object |
  | `of_type` (`world.rs:1199`) | a type |
  | `external_all` (`world.rs:900`) | every external id |
  | `imported` (`world.rs:945`) | every imported position |
  | `version_of` (`world.rs:1677`), and `Store::history` from `get` | an object's history |

  The grep is in §3's B rows. Each call is placed on a read path or on the writer.
- **Input bounds.** Every limiting constant and flag of `main.rs`, `reads.rs`, `expr.rs`, `yaml.rs` and `store.rs`, listed in the C rows; each probed once with `bounds.py`.
- **Deadlines.** Every timeout constant and flag (the D rows).
- **The profile.** ADR-0204 §1, scenario by scenario:
  - steady load, and the month-end peak at twice the steady load;
  - clients retrying after their timeouts;
  - a restart;
  - a publish, an erasure batch, a backup and a prune during the load;
  - and past the profile: the connection cap, 300 concurrent writers, and 880 reads against a file limit set by the guide's rule.

**Why this is complete.**
- Operations: the router lists them all.
- Loops: they must go through the accessors grepped.
- Bounds and deadlines: they are named constants or flags, all grepped (`grep -n "^const\|^pub const"` over the four files, and `args()`, `main.rs:3926-4096`).

**Left out, and why.**
- **The 24-hour soak.** Not runnable in this audit. Runs were 1–2.5 minutes, so slow growth (the `-wal` under days of load, fragmentation) is unmeasured.
- **PostgreSQL.** Not built (ADR-0204 §9.1).
- **Import throughput at six years.** The cut-over flow has no mirror type, so `import_batch` into it is refused by design. `operate.md:262`'s figure stands unconfirmed at this commit.
- **An evaluator that hangs.** Its 5 s per call is the evaluator's, outside §2's model; the lock is released while the service waits (`main.rs:308-359`, read).
- **The adversarial harness.** It has no cost conditions (`adversarial-harness.md`, "Built"), so nothing in this area is held by it.
- **The sandbox's own costs.** Graded minor by §4 if found; none was pursued.
- **The gateway.**

## 2. What was measured

### Table A: every operation, one client, idle store (p50 / p95 ms)

Commands: `python3 perf2.py g2.sock run/ids-g2.json g2-idle <pid>` and `… g6.sock run/ids-g6.json g6-idle <pid>`. Results in `results/perf-g2-idle.md` and `results/perf-g6-idle.md`; the load average was 1–7.

| Operation | 2y p50 | 2y p95 | 6y p50 | 6y p95 | Answer (6y) |
|---|---|---|---|---|---|
| get claim / claim line / employee | 0.9 / 0.5 / 0.4 | 1.4 / 0.5 / 0.4 | 0.9 / 0.5 / 0.4 | 1.5 / 0.6 / 0.5 | 0.8 kB |
| availability claim (person / agent) | 0.8 / 0.8 | 1.1 / 1.0 | 0.9 / 0.9 | 1.2 / 1.1 | 4 kB |
| check submit / approve / draft | 0.8 / 0.4 / 0.5 | 1.0 / 0.5 / 0.5 | 1.0 / 0.5 / 0.6 | 1.1 / 0.6 / 0.7 | |
| history claim / with follow | 0.7 / 0.8 | 0.8 / 1.1 | 0.8 / 1.0 | 1.1 / 1.2 | |
| events 10 / 1000 positions | 0.7 / 35.1 | 0.8 / 50.0 | 0.8 / 36.6 | 1.0 / 49.4 | 828 kB |
| lookup (legacy / xero) | 0.7 / 0.7 | 0.7 / 0.9 | 0.9 / 0.8 | 1.0 / 1.0 | |
| declaration (current / v1) | 0.6 / 1.8 | 0.6 / 2.3 | 0.6 / 1.9 | 0.8 / 2.8 | 12 kB |
| tools (human / agent) | 1.0 / 0.3 | 1.3 / 0.4 | 1.1 / 0.4 | 1.5 / 0.4 | 32 kB |
| query worklist `state == SUBMITTED` 50 | 9.0 | 9.6 | 16.8 | 22.8 | |
| query my claims `claimant.id ==` | 4.1 | 5.3 | 14.6 | 16.3 | |
| query by number (unique) | 0.8 | 0.9 | 1.0 | 1.3 | |
| query worklist ordered `entered_at(state) desc` | 13.6 | 16.7 | 23.4 | 35.6 | |
| query PAID page 100 / 1000 | 23.8 / 157.5 | 39.8 / 167.5 | 32.8 / 182.6 | 36.1 / 187.4 | 166 kB |
| query `order created_at desc`, no filter | 88.2 | 101.7 | 298.1 | 368.8 | |
| query derived `total > USD 5900` (rare) | 1002.5 | 1041.3 | 3453.5 | 3562.5 | |
| available approve / submit / record_in_xero | 0.5 / 16.1 / 8.6 | 0.6 / 23.1 / 11.1 | 0.5 / 24.8 / 14.5 | 0.7 / 32.2 / 20.1 | |
| pull 1000 (events from cursor 0, 2y) | 53 | 82 | | | 800 kB |
| **metric** `submission_to_payment` | 145 | 159 | 573 | 600 | |
| **metric** `claimant_amount_claimed` (per person, all) | **1506** | **1936** | **8057** | **8318** | 3.4 MB |
| **metric** `claimant_over_policy_lines` | **1867** | **2209** | **8380** | **10441** | 3.1 MB |
| **metric** `claimant_amount_claimed` keep claimant | **1397** | **1508** | **3443** | **3523** | |
| **metric** `claimant_amount_claimed` bound to one | 3.8 | 13.1 | 5.9 | 23.0 | |
| **metric** `ClaimLine.amount_claimed_by_category` | **1009** | **1239** | **2650** | **2771** | |
| **metric** standard `time_in_state` / `_p80` | 498 / 685 | 524 / 785 | **1308 / 1319** | **1316 / 1360** | |
| **metric** standard `throughput` / `transition_counts` | 788 / 769 | 856 / 771 | **2049 / 2023** | **2065 / 2124** | |
| **metric** standard `refusal_rate` / `rework` | 914 / **1071** | 956 / **1146** | **2017 / 2062** | **2082 / 2142** | |
| **metric** standard `oldest_open` / `work_in_progress` | 337 / 118 | 341 / 130 | **1033** / 306 | **1038** / 339 | |
| **metric** `approval_policy_flags` / `refusals` (no attempts) | 148 / 99 | 154 / 164 | 266 / 271 | 445 / 285 | |
| exceptions Claim | 0.4 | 0.5 | 0.4 | 0.5 | |
| diagnostics Claim (30 / 3650 days) | 1033 / 1105 | 1133 / 1199 | 2108 / 2095 | 2116 / 2098 | |
| export log 1000 / intervals 1000 / transitions 1000 | 48 / 75 / 38 | 49 / 84 / 57 | 25 / 51 / 31 | 26 / 51 / 32 | 794 kB |
| export `Claim.labels` (no labels at all) | 437 | 442 | 1090 | 1173 | |
| writes: draft / add_line / submit / approve | 8.2 / 8.9 / 8.6 / 8.7 | 12.0 / 16.6 / 15.7 / 18.0 | 12.6 / 17.6 / 14.8 / 15.0 | 23.9 / 28.5 / 27.9 / 19.4 | |
| writes: verify / pay (evaluator; pay cascades) | 10.0 / 11.0 | 23.0 / 18.2 | 19.3 / 25.2 | 32.8 / 37.3 | |
| withdraw (cascade) / erase_personal_data (claim and its lines) | 9.4 / 48 | 18.6 / 62 | 14.0 / ~105 | 35.7 / ~136 | |
| acknowledge / maintain (both prunes) / upgrade (none due) | 0.4 / 0.4 / 0.4 | 7.0 / 0.5 / 0.4 | 0.8 / 0.8 / 0.7 | 9.6 / 1.0 / 0.7 | |
| publish: submit / approval (adds a derived attribute and a metric) | 1.9 s / 2.7 s | | 5.7 s / 7.9 s | | |
| GET /health / /metrics | 1.2 / 0.3 | 1.4 / 0.4 | 1.1 / 0.2 | 1.9 / 0.3 | |

### Table B: the §1 profile

Commands:

```sh
python3 load2.py <sock> <ids> <label> <pid> <metrics-port> 150 --writes 30
python3 load2.py <sock> <ids> <label> <pid> <metrics-port> 120 --sessions 32 --reporters 8 --agents 4 --writes 60   # 2x
```

The profile is 16 sessions with no think time, 4 report readers back to back and 2 agents. Writes are Poisson. Sessions write 30 a second and agents about 20, for about 50–60 writes a second in all. Results are in `results/g2-profile-p.md`, `results/g6-profile-p.md`, `results/g2-peak2x.md` and `results/g6-peak2x.md`.

| p50 / p95 ms | 2y profile | 6y profile | 2y peak 2x | 6y peak 2x |
|---|---|---|---|---|
| **all requests (C5 100 ms)** | 25 / 64 | 24 / 67 | **261 / 603** | **377 / 785** |
| writes per second answered | 60.0 | 48.3 | 72.5 | 64.5 |
| request `pay` / `verify_xero_record` | 41 / 106 · 43 / 111 | 40 / 111 · 36 / 112 | 481 / 802 · 489 / 793 | 732 / 1026 · 727 / 1011 |
| **one-object reads (C5 100 ms)**: get, availability, check, history, lookup | 2.3 / 4.3 | 2.0 / 5.5 | 2.8 / 7.9 | 1.9 / 4.8 |
| **metric reads (C5 1 s)**, pooled | 356 / **6578** | 1041 / **30197** | 4393 / **16407** | 1255 / **30142** |
| per-person `claimant_amount_claimed` | **6216 / 6727**, 5 of 54 refused 503 | **28424 / 30647**, 11 of 18 refused 503 | **11212 / 18865**, 8 of 40 refused | **29351 / 30292**, 15 of 17 refused |
| `submission_to_payment` (person / agent) | 353 / 855 · 349 / 386 | **1009 / 1163 · 1032 / 1163** | | **1284 / 30119** |
| worklists (query, available, agents' query_objects) | 105 / 285, **8 refused 503** | 123 / 341, **11 refused 503** | 119 / 483, **15 refused** | 48 / 254, **7 refused** |
| `/health` on `--metrics-bind` | 8.8 / 14.3 | 5.0 / 13.5 | 9.1 / 17.5 | 4.4 / 10.2 |
| service RSS, maximum | 1.0 GB | 1.1 GB | 0.66 GB | 1.2 GB |

Writes alone on 6y (`--reporters 0 --agents 0 --writes-only`, Poisson): 50 a second gives p50 25, p95 70–139 ms. Offering 70 or 100 a second, the writer answered about 65 a second, at p95 700–800 ms (`results/g6-writes{50p,70,100}.md`).

### Scenarios beside the load

All on 6y unless marked, at the steady profile:

- **Erasure batch.** 20 `erase_personal_data` at 40 s (`ops.py`, `results/g6-ops.md`): each took 61–138 ms. In the 5 s after the batch, 6 metric reads and 3 worklist reads were refused `503 read_timeout`, "stopped so the store's log could be cut".
- **Backup.** Online, python `sqlite3` as `operate.md`'s unit does, at 90 s: 1.31 s, with no change to p95 and no 503.
- **Publish.** A publish under the load (`results/g6-publish.md`):
  - submit 12.2 s, 13.5 s, and once refused `503 read_timeout` by a cut;
  - approval 11.5 s, holding writes (a §10 limit).
- **Restart.** SIGTERM under load on 2y (`results/g2-restart.md`): the stop drained in 6.2 s and the start listened in 1.6 s, with no 5xx after the restart. While stopped, connections were refused.
- **Retrying clients.** Reporters with a 10 s timeout, retrying (`--report-timeout 10`, `results/g6-retry10.md`): none of 31 per-person reads answered in time; 7 of 16 bound per-person reads (6 ms alone) timed out behind abandoned reads; all 45 failures were reports.
- **Past the cap.** 600 connections against `--max-connections 512` on 2y: `flood.py … 60 60 get:300 worklist:150 metric:100 draft:50`.
  - The service stayed up, peaking at 830 MB.
  - It answered `503 busy` with p95 99–107 ms.
  - Pooled reads were refused only at their 30 s deadline: refusal p95 30.8 s.
  - `/health` on `--metrics-bind` answered 200, p95 0.9 s.
  - Afterwards get took 0.7 ms and a worklist 9.9 ms: it recovered.
- **300 concurrent writers** on 2y (`flood.py … 25 120 draft:300 get:20`): p50 4.1 s, p95 4.5 s, max 13 s, about 83 writes a second, none refused. Gets alongside: p95 13 ms.

## 3. The inventory

Grades follow ADR-0204 §4. "Held by" names a test that holds the item today, or "none". The last column is the check that would hold it. Two gates are named so often that they are defined once:

- **Gate P (profile).** A script beside `scripts/check-all`, run before a release (§7):
  1. build the two- and six-year stores with `tools/golive2.py`, or reuse cached copies;
  2. start the release binary on copies;
  3. run `load2.py`'s profile for 150 s at 1× and 120 s at 2×.

  It fails if any of these holds:
  - pooled request p95 > 100 ms;
  - pooled one-object read p95 > 100 ms;
  - any metric's p95 > 1 s;
  - any 5xx or 503;
  - service RSS above `operate.md`'s MemoryMax for that store.
- **Gate I (idle).** `perf2.py` on both stores. It fails where an operation `operate.md` gives a figure for exceeds it by more than 2×.

### A. The operations

| # | Instance | Where | Held by | Evidence | Grade | Check that would hold it |
|---|---|---|---|---|---|---|
| A1 | `request`: on the writer, unbounded wait; a change's submit or refresh takes the dry-run slot | `main.rs:1808-1853`, `locked` `1381` | `reads_and_writes_a_second_measured` (small store) | Reproduced, Tables A and B | **Major** (C at the profile: the 2× peak's p95 is 603 and 785 ms). Holds at steady (p95 64–67 ms). | Gate P at 2× |
| A2 | `check` | `main.rs:1857-1916`: `object_slot` where its rules grow, `consulting(read=true)` | `a_read_of_an_object_whose_rules_grow_waits_among_the_objects_reads` | Reproduced: idle 0.4–1.1 ms, profile p95 4.5–5.8 ms | Holds (C5), except B1 | Gate P |
| A3 | `get`, `lookup` | `main.rs:2592-2615`, `2783-2796`; `object_json` reads the whole history, `2567-2570` | `a_read_of_an_object_whose_rules_grow_waits_among_the_objects_reads` | Reproduced: profile p95 4.7–6.2 ms; see B1 and B14 | Holds, except B1 and B14 | Gate P |
| A4 | `availability` | `main.rs:2976-3048` | the same | Reproduced: profile p95 4.3–5.6 ms | Holds, except B1 | Gate P |
| A5 | `history` (objects pool, limit and 8 MiB) | `main.rs:2798-2850` | `a_page_of_events_ends_at_its_bytes` (`request.rs:10572`) | Reproduced: profile p95 5.4–6.9 ms; an object of about 8k events: 5.3 ms | Holds | none needed |
| A6 | `events` (objects pool, ≤1000 positions, ≤8 MiB) | `main.rs:2852-2897` | the same; `bounds.py` | Reproduced: 1000 positions p95 50 ms, 0.8 MB | Holds | none needed |
| A7 | `declaration`, `tools` | `main.rs:2901-2974` | none | Reproduced: ≤2.8 ms | Holds | Gate I |
| A8 | `query` (worklists or analytics, filter bounds) | `main.rs:3055-3151`, `reads.rs:691-731`, `873` | `a_query_is_read_in_pages`, `a_filter_past_the_parsers_bounds_is_refused_and_the_service_answers_on` | Reproduced: worklist p95 9.6 / 22.8 ms idle and 285–420 ms at the profile, against `operate.md:256`'s "1 to 11 ms"; 503s under E4 | **Minor** (a cost past a guide's figure, within its deadline); the 503s are E4's | Gate I; Gate P on 503 |
| A9 | `available` (worklists, limit) | `main.rs:2014-2047`, `reads.rs:273-372` | `a_page_of_available_reads_to_its_page` | Reproduced: idle ≤32 ms; profile p95 284–388 ms | Holds (no figure in the guide) | Gate P |
| A10 | `metric` (analytics or agents) | `main.rs:3153-3219`, `metric.rs:2354` | `an_objects_metric_bound_to_a_column_reads_by_it` (bound reads only) | Reproduced: Tables A and B | **Major**: see B2, B3, B5, B6 | Gate P |
| A11 | `pull` (worklists, limit, 8 MiB) | `main.rs:1935-1957` | `a_page_of_the_feed_reads_from_its_cursor` | Reproduced: 1000 events 53 / 82 ms | Holds | none needed |
| A12 | `export` (analytics, limit, 8 MiB) | `main.rs:2114-2137`, `export.rs:47-162` | `a_page_of_a_query_a_pull_or_an_export_ends_at_its_bytes`, `an_export_pages_by_what_last_wrote_each_row…` | Reproduced: log, intervals and transitions ≤90 ms a page; labels and attempt counts in B12 and B13 | Holds, except B12 and B13 | see B12, B13 |
| A13 | `exceptions` (analytics, one page, `cursor` taken and ignored) | `main.rs:1998-2010`, `reads.rs:222-238`, `TAKES` `main.rs:1188` | `exceptions_and_available_page_objects_and_refuse_as_values` | Unconfirmed: no excepted objects in either store; 0.4 ms empty | **Minor**: the page is unbounded by count or bytes | Suite: 2,000 admitted objects answer in pages of `limit` with a cursor and ≤8 MiB |
| A14 | `diagnostics` (analytics) | `main.rs:2051-2080`, `diagnostics.rs:54-61,139` | none | Reproduced: 1.1 s / 2.1 s idle; about 1.06 GB peak each on 6y (`heavymem.py … diag:1`) | **Minor**: no figure in the guide, within deadline; its memory belongs to B7 | Gate I; B7's suite test |
| A15 | `maintain`, `acknowledge`, `upgrade` (writer) | `main.rs:2084-2110`, `2141-2157`, `3398-3446` | `maintain_names_an_actor_the_store_knows` | Reproduced: ≤1 ms; a prune of nothing 0.4 ms | Holds | none needed |
| A16 | `import_batch` (writer, ≤2 MiB) | `main.rs:2476-2532` | none at scale | Unconfirmed at this commit (`operate.md:262`: 1.6 s a batch; ADR-0183: 23 s for a batch near the limit, holding writes) | Not graded: not a §1 operation, and no mirror type in the cut-over flow | Gate I with a mirror module |
| A17 | `publish`: dry run (one slot, 120 s); approval (writer) | `main.rs:2619-2746` | `a_submits_dry_run_holds_no_write`, `a_submits_dry_run_past_the_deadline_is_answered_not_locked` | Reproduced: idle 1.9 / 2.7 s (2y) and 5.7 / 7.9 s (6y); under load on 6y a submit took 12–13.5 s (`operate.md:258` says about 6 s), the approval held writes 11.5 s | Submit: **minor** (past the guide's figure). The approval's hold is §10's limit, not a finding. A submit refused 503 is E4's. | Gate I |
| A18 | `GET /health`, `/metrics` (no pool; `/health` opens the file) | `main.rs:3223-3383`, `3646-3843` | `each_answer_is_logged_and_counted_and_health_reads_the_store` | Reproduced: profile p95 ≤17.5 ms; past the cap p95 0.9 s on `--metrics-bind` | Holds | Gate P |
| A19 | sandbox controls | `main.rs:1694-1805` | — | not measured | — (sandbox; §4 minor at most) | — |

### B. Loops over a type, the log or the attempt log; the broken classes

| # | Instance | Where | Held by | Evidence | Grade | Check that would hold it |
|---|---|---|---|---|---|---|
| **B1** | **A one object's rule reads a set growing with the store, and `grows_with_store` does not see it, so `get`, `availability`, `check` and `lookup` wait in no pool and read it whole.** The rules are: `o.transitions` (the whole log), `o.attempts` (the whole attempt log), `o.declaration_version` (the whole log, scanned back per object), an observation collection (the whole kind), and a Proposal's `last_verdict` (the whole attempt log). The same rules run under the writer in a request's guards. | `eval.rs:792-800`, `802-808`, `785` → `world.rs:1677-1682`, `eval.rs:1511-1523`; `reads.rs:113-118`. Classification: `reads.rs:753-790` (`wide` matches metrics, type scans and `referrers` only). | `a_types_reads_grow_where_its_rules_read_the_store` holds four kinds, none of these | Reproduced, see below | **Blocker** (C; §4: an exhaustion of memory at a load within the profile). Its normal-operation instances (the servicedesk sample) are also **Major** (C5, one-object read, at the profile). | Suite: extend `a_types_reads_grow_where_its_rules_read_the_store` with each construct, and better, a test that a `get`, `availability` and `check` of a type deriving each one reads `world::whole_log_reads()` == 0, no whole attempts and `whole_type_reads(kind)` == 0. Gate P with a variant flow. |
| B2 | Per-person and line metrics over `objects` recompute derived values per object (`total` through `lines`) and answer every group in one page | `metric.rs:919-940` (rows), `2354-2480` (page) | none at scale | Reproduced: Table A, 8.1 / 8.3 s idle on 6y; Table B, 11 of 18 refused at their deadline | **Major** (C, the C5 metric bullet, at the profile; 503 refusals, C bullet 1) | Gate P |
| B3 | Standard metrics over `transitions` and `intervals` read the owner type whole and the whole log (`transitions`), or every object's parts (`intervals`) | `metric.rs:937-968`, `1027-1042` | none at scale | Reproduced: 6y idle 1.3–2.1 s, about 1.06 GB peak each | **Major** (C5 metric bullet; reports and the agents' `read_metric` read standard metrics) | Gate P with standard metrics in the report mix; suite test: a transitions metric reads events by its family, not `whole_log_reads()` |
| B4 | Every non-`objects` metric source reads its owner type whole first, attempt counts included | `metric.rs:937` | none | Reproduced: `approval_policy_flags` with 0 attempts took 148 / 266 ms (2y / 6y) | **Minor** (cost within C5) | Suite: `whole_type_reads(owner)` == 0 for an attempt-count metric |
| **B5** | `attempt_count_rows` merges each attempt into a `Vec` by linear search (quadratic), reads the whole attempt log and rollup, and checks no deadline in its loop. It serves the declared `approval_policy_flags` and `line_policy_flags`, the standard `refusals`, `flags_raised` and `refusal_rate`, and `export <T>.attempt_counts`. | `metric.rs:48-105` (search at `:70`); `export.rs:206-246` | none | Reproduced (`attempts.py`): 10k attempts made `refusals` 0.66–0.77 s (2y); 30k made it 7.3 s, `refusal_rate` 10.2 s, `approval_policy_flags` (0 rows) 7.4 s, `export Claim.attempt_counts` 7.0 s (6y). Overrunning the deadline is unconfirmed. | **Major** (C5 metric bullet at the profile, where the 90-day retention holds tens of thousands of refusals and flags) | Suite: rows of 20k and 40k distinct keys take < 3× the time; a past deadline stops the loop |
| **B6** | A metric's `filter` takes none of a query filter's cost bounds (terms, aggregates nested, a set's members); only the parser's depth applies | the bounds `reads.rs:946-953`, absent from `metric.rs:2410-2440` | `a_metric_filter_a_row_cannot_answer_is_malformed` (shape only) | Reproduced (`bounds.py`, then the inline probe of §2): a set of 1,001 took 6.7 s; of 5,000 (39 KB) and 20,000, `503` at 30 s; aggregates 3 deep 28.4 s. The agents' `read_metric` tool takes a filter. | **Major** (C at the profile: one tool call or forwarded filter holds an agents or analytics slot for the read deadline; the FILTER-COST class, fixed for `query` alone) | Suite: the `reads.rs:2094` cases run through `metric::page` give `malformed_filter` |
| B7 | Pooled reads that hold the whole log in memory: diagnostics, standard metrics over transitions, `export` of labels or an observation kind. Each costs about 1.06 GB on 6y, growing with the log; pools bound how many run, not how much each holds. | `diagnostics.rs:61`, `metric.rs:960`, `export.rs:271` | none | Reproduced (`heavymem.py`): 8 at once (the analytics 4 and agents 4, as at the 2× peak) peaked at 8.56 GB RSS on g6a. Under `systemd-run --scope -p MemoryMax=8G` on g6m: 6.1–6.4 GB, `memory.events max 0`. | **Major, unconfirmed** as an exhaustion: `operate.md:276`'s "give such a store 8G" is under the measured 8.56 GB, though its own formula gives about 19 GB | Gate P with standard metrics and diagnostics in the 2× report mix, under the guide's MemoryMax |
| B8 | A query that orders or filters without an index reads its type whole (analytics pool) | `reads.rs:1063` | `a_query_aggregating_for_each_object_waits_among_the_analytics` | Reproduced: `order created_at desc` 298 / 369 ms on 6y, about 300 MB; a rare derived filter 3.45 s (`operate.md:270`: 3.4 s) | Holds (documented; within the guide's figure) | Gate I |
| B9 | `history` with `follow` reads every superseding type whole | `reads.rs:46-75` | none | Unconfirmed: no superseding state in the cut-over flow | **Minor** (objects pool, deadline) | Suite: `whole_type_reads` == 0 for a follow over a 50-member group |
| B10 | Write-path scans under the writer, all read: correcting an observation scans every kind with `corrects`; an erasure scans every Proposal (twice), its observation kinds whole and every label; an invariant that reaches anything scans its type. | `request.rs:1310-1318`, `2476-2481`, `2531-2537`, `3545-3552`, `3572-3577`, `4887-4893` | `a_groups_erasure_reads_no_type_whole` (Account only) | Reproduced for labels (`labels.py`): erasure p50 48 ms before, 42 ms after 20,000 labels, p95 62 → 88 ms (2y); the rest unconfirmed | Holds at 20k labels. **Minor**, unconfirmed, for proposals and kinds at scale. | Suite: `whole_type_reads(LABEL)` and `whole_type_reads(PROPOSAL)` == 0 during an erasure |
| B11 | `get` and `lookup` read an object's whole history to find its last event's version | `main.rs:2567-2570` → `store.rs:3228-3235` | none (HISTORY was fixed for `history` only) | Reproduced: one Employee with about 2k events of 1 KB: get p95 15.9 ms; about 8k: 66 ms; `history`'s page 5.3 ms | **Minor** (C5 holds for §1's objects; cost and memory grow with one object's history) | Suite: a `get` of an object of 10k events reads ≤ 1 event (`world::events_read()`) |
| B12 | `export` of `<Type>.labels` or an observation kind reads the whole log, every label or kind, and the family, on every page | `export.rs:89-97`, `119-131`, `264-283` | none; contradicts `operate.md:276` ("never the log or a type whole") | Reproduced: with no labels, 437 ms and +420 MB (2y), 1.09–1.43 s and about 680 MB (6y); with 20k labels, 670 ms a 100-line page (2y) | **Minor** (C within deadline; G, the guide describes otherwise) | Suite: a labels page reads `whole_log_reads()` == 0 and the labels by subject index |
| B13 | `export <T>.attempt_counts` answers every settled day in one page, `more: false`, with no limit or byte bound | `export.rs:206-246` | none | Unconfirmed (0 settled days in the stores); cost from B5 reproduced | **Minor** | Suite: a page ends at `limit` and 8 MiB with a day cursor |
| B14 | A Proposal's `get` reads the attempt log (B1's last instance) | `reads.rs:113-118` | none | Reproduced in a sandbox: 0 attempts 0.4 ms, 10k 22 / 41 ms, 30k 65 / 70 ms | Within C5 to about 45k attempts. Part of B1. | B1's suite test |

B1's evidence, in full:

- **The class at the profile: a derived attribute over `this.transitions`.** I published a variant of the cut-over flow, `run/reimb-variant.yaml`, adding `times_returned: count(t in this.transitions where t.returns)`, a derivation `declaration-syntax.md:1355` documents. The objects pool's running count stayed 0 throughout.
  - 2y: `get` of a claim 211 / 276 ms; 16 at once p95 1.5 s, RSS 1.74 GB.
  - 6y: `get` 768 / 849 ms; 16 at once p95 5.8 s, 5.24 GB.
  - 6y, 32 at once under `systemd-run --scope -p MemoryMax=8G` (the guide's six-year figure): `memory.events max 332982`, 12M anonymous refaults (swapping), every `get` 87–91 s.
  - 6y at the steady profile with think time (`load2.py … --think 1 --report-think 10`, the variant installed): RSS 8.66 GB; sessions' `get` p50 14.5 s, `history` 17 s; requests p95 224 ms.
- **The same class in normal operation: an observation collection in the shipped servicedesk sample.** `Request.time_to_first_response: min(r in replies: r.occurred_at) - opened_at` (`servicedesk.yaml:158-160`). A sandbox loaded with the sample, then a `get` of one Request with no replies of its own, as other requests' replies grew: 0 replies 0.6 ms; 5k 21 / 25 ms; 20k 97 / 117 ms; 40k 208 / 240 ms. `availability` cost the same.
- **The same class in a metric: `o.declaration_version`.** A metric `claims_by_version` grouping by `c.declaration_version` took 934 ms on 2y. It scans the whole log back once per object (`world.rs:1679-1681`).

### C. Input bounds

`bounds.py`'s results are in `logs/bounds-g6.out`.

| # | Bound | Where | Held by | Evidence | Grade | Check |
|---|---|---|---|---|---|---|
| C1 | Body ≤ 2 MiB (axum's default limit) | `main.rs:1333` (`Bytes::from_request`); `wire.md:7` | none (ADR-0183 measured it once) | Reproduced: 2 MiB + 1 → `400` in 9.7 ms | Holds | Suite: a body of 2 MiB + 1 → 400 `bad_request` |
| C2 | The body arrives within `--idle-timeout` | `main.rs:1241`, `1314-1341` | `idle_and_half_sent_connections_are_closed` | test | Holds | — |
| C3 | Head read timeout; headers; URL | `main.rs:4776-4777`; hyper's limits | the same | Reproduced: a 200 KB header `400`, 150 headers `431`, a 100 KB URL `414`, each < 6 ms | Holds | — |
| C4 | JSON nesting (serde's recursion limit, 128) | `main.rs:1246-1307` | none | Reproduced: 127 and 200 deep → `400 recursion limit exceeded`, 0.2 ms | Holds | Suite: nesting 200 → 400 |
| C5 | A key named twice; unknown keys; key types | `main.rs:1290-1355`, `1467-1529` | `a_body_naming_a_key_twice_is_refused`, `a_flag_or_inputs_of_another_type_is_refused` | test | Holds | — |
| C6 | `idempotency_key` ≤ 256 B; `context` ≤ 16 KiB | `main.rs:2180-2181` | `a_key_and_a_context_are_bounded` | Reproduced: `400`, 0.3 ms | Holds | — |
| C7 | A string ≤ 64 KiB | the value layer, ADR-0188 | the value tests | Reproduced: `invalid_input out_of_range`, 0.8 ms | Holds | — |
| C8 | 100,000 input keys (bounded only by the body) | `main.rs:2259-2267` | none | Reproduced: 212 ms, refused `invalid_input` | Holds | — |
| C9 | Page `limit` 1–1000, default 100; 8 MiB pages | `main.rs:1980-1994`, `reads.rs:173-180`, `store.rs:3326` | `a_query_is_read_in_pages`, `a_page_of_a_query_a_pull_or_an_export_ends_at_its_bytes` | Reproduced: 0 or 1001 → `400` | Holds; except a metric's page (B2: 3.4 MB on 6y, unbounded by bytes), `exceptions` (A13) and `.attempt_counts` (B13) | Suite: a metric page ends at 8 MiB or is refused |
| C10 | `events` ≤ 1000 positions, each once, ≤ 8 MiB | `main.rs:2861-2891` | `a_page_of_events_ends_at_its_bytes` | Reproduced: `400` | Holds | — |
| C11 | `order` ≤ 8 keys; `fields` ≤ 256, each once | `main.rs:3076-3091`, `3923-3924` | none named | Reproduced: `400`, 0.2 ms | Holds | Suite: 9 keys and 257 fields → 400 |
| C12 | Query filter: 256 terms, 2 nested aggregates, sets of 1,000, nesting 64, depth 128 | `reads.rs:477-484`, `946-953`; `expr.rs:361,366` | `a_filter_past_the_parsers_bounds_is_refused_and_the_service_answers_on`, `reads.rs:2094` | Reproduced: each refused in < 1.3 ms; a 1.9 MB literal 54 ms; a set of 1,000 26 ms | Holds | — |
| C13 | A metric's filter | `metric.rs:2410-2440` | — | **B6** | **Major** | B6 |
| C14 | A metric's `keep` named once; `bind` and `keep` against the dimensions; `over` positive | `main.rs:3166-3200`, `metric.rs:2400-2404` | `a_window_back_from_now_is_positive` | Reproduced: 100,000 names or keys → `unknown_dimension` in 59–69 ms; `over` of 10⁹ weeks 561 ms | Holds | — |
| C15 | Diagnostics' `window` positive | `main.rs:2062-2066` | the same | Reproduced: 10⁹ weeks 2.8 s, the same as any window (A14) | Holds | — |
| C16 | A flow's YAML: anchors and aliases refused, nesting 64 | `yaml.rs:255`, `333-363` | `a_description_nested_past_its_bound_is_refused`, `anchors_aliases_and_bad_yaml_are_refused` | Reproduced: a draft is a cheap write; checking happens at submit | Holds | — |
| C17 | A change's size, under the 2 MiB body (60,000 types refused at the dry-run deadline) | ADR-0200 decision 5 | D646's tests | not re-measured | Holds (dry-run deadline) | — |
| C18 | Calls to evaluators: ≤ 16 a request, each within `--evaluator-timeout` | `main.rs:211`, `308-359` | `a_deployment_asks_its_evaluators_at_their_endpoints` | read | Holds | — |

### D. Deadlines

| # | Deadline | Where | Held by | Evidence | Grade | Check |
|---|---|---|---|---|---|---|
| D1 | `--read-timeout` 30 s: on a read and on its wait for a slot | `main.rs:3379`, `785-820`, `948-993`; `world.rs:534-568` | `a_read_past_its_deadline_is_stopped`, `a_heavy_read_waits_its_turn_within_its_deadline` | Reproduced: 503 at 30.0 s | Holds. Not checked inside B5's loop or during a single whole read such as `read_log` (unconfirmed). | B5's suite test |
| D2 | `--dry-run-timeout` 120 s, one slot | `main.rs:4265`, `2687-2733` | `a_dry_run_reads_under_a_deadline_of_its_own` | test | Holds | — |
| D3 | A write's wait for the writer: no deadline | `main.rs:1381-1408` | none | Reproduced: 300 writers p95 4.5 s, max 13 s, none refused | **Minor** (C past the profile: refuses quickly; bounded by the cap × a write, no hang) | Gate: at 10× write concurrency the p99 wait stays under the clients' 45 s (`operate.md:232`), or past N waiting the request is refused 503 |
| D4 | A pooled read's wait for its slot: refused only at its deadline | `main.rs:785-820` | `a_heavy_read_waits_its_turn_within_its_deadline` | Reproduced: past the cap, pooled refusals p95 30.8 s | **Minor** (C past the profile: refuses quickly) | Gate: past the cap, p95 of a 503 < 1 s |
| D5 | A read whose client has gone runs on (D656, open) | `main.rs:1371-1379` | none | Reproduced (§2, retrying clients) | **Major** at the profile, with B2: "clients retrying after their timeouts" left reports unanswerable | Service test: 16 abandoned metric reads leave a 17th answered within its own time (D656's planned test) |
| D6 | Busy timeout 5 s × 4; evaluator 5 s; idle 30 s; grace 30 s | `store.rs:77`, `main.rs:4042-4048`, `4218`; `operate.md:232` | `a_write_meeting_a_lock_past_its_timeout_is_retried_then_refused_stale`, `a_stopped_service_answers_the_requests_in_flight` | Reproduced: stop under load drained in 6.2 s | Holds | — |

### E. The cap, threads, connections and the `-wal`

| # | Instance | Where | Held by | Evidence | Grade | Check |
|---|---|---|---|---|---|---|
| E1 | `--max-connections` 512 (≤ 4096); the oldest idle connection closed to make room; 16 refusals at once | `main.rs:3940`, `4146`, `4755`, `4806-4857` | `a_connection_at_the_cap_makes_room_or_is_told_busy`, `a_connection_taken_to_make_room_begins_nothing` | Reproduced: 600 connections, `503 busy` p95 ≤ 107 ms, service up | Holds | — |
| E2 | Threads: 2 workers, blocking threads up to 4096 + 64, 16 MiB stacks | `main.rs:4148-4158` | `the_metrics_listener_answers_beside_a_busy_main_one` | Reproduced: `/health` answered under every flood | Holds | — |
| E3 | Read connections: one SQLite connection per concurrent read, 8 kept idle, no bound; about 3 file descriptors per busy connection | `store.rs:1838`, `1861-1887` | none | Reproduced: `prlimit --nofile=1024` with `--max-connections 900` (the guide's rule, `operate.md:107`), 880 concurrent gets: 37,631 of 69,449 answered `500` "unable to open database file"; accepts failed with "Too many open files"; `/health` 500 and 503 | Not a finding for C (past twice the profile's concurrency). **Minor** for G: the guide's `LimitNOFILE` rule is short by about 2 descriptors a read, and overload answers 500, not 503. | Service test: with `RLIMIT_NOFILE` = cap + 32 and every connection reading, no 5xx |
| E4 | The `-wal` cut: held 1 s for all pools, then urgent past 4 × 64 MiB or after an erasure, which stops every running read | `main.rs:4261-4408`, `store.rs:1804` | `a_cut_holds_new_reads_a_moment_and_stops_long_ones_when_urgent` | Reproduced: see below | **Major** (C bullet 1, every request answered, not refused 503, at the profile) | Gate P (no 503); service test: with 4 continuous 2-s reads and 50 writes a second for 120 s, no read under 1 s is stopped |
| E5 | Memory given back after heavy requests and when idle | `main.rs:3549-3551`, `3613-3641`, `4418-4429` | `what_a_heavy_request_freed_is_given_back` | Reproduced: RSS back to 70–300 MB after every run | Holds | — |
| E6 | The log's queue, 4096 lines | `main.rs:127-171` | `a_log_no_one_reads_is_said_and_stops_no_request` | test | Holds | — |

E4's evidence, in full:

- The urgent cut's 503s at the profile: 2y, 8 worklist and 10 metric reads in 150 s; 6y, 11 and 23.
- The log says "the read was stopped so the store's log could be cut", about every 25–40 s. In each run the cut was deferred 39–53 times first.
- The `-wal` reaches 256 MiB in about 30 s at 50–60 writes a second. That is about 140 KB a write, worked out from `objectflow_wal_cuts_total` and the runs.
- After an erasure batch, the cut is urgent at once.
- One of three `submit`s under load was refused this way.
- A worklist read of 0.1–0.4 s is stopped as readily as a 30 s report, since the epoch is one for all pools (`main.rs:4375-4384`).

### F. Memory per page and per read

| # | Instance | Evidence | Grade |
|---|---|---|---|
| F1 | A page of `query`, `pull`, `export`, `history`, `events`: 8 MiB, building it costs more (D683, documented) | Reproduced: a 1000-item page 166–828 kB; RSS steady | Holds (`operate.md:276` documents it) |
| F2 | A metric page holds every group, unbounded by bytes | Reproduced: 3.4 MB on 6y | **Minor** (memory is per read and pooled); check under C9 |
| F3 | Per-read memory of a whole-log read | About 1.06 GB on 6y (B7) | **Major, unconfirmed** (B7) |
| F4 | Per-read memory of B1's unpooled reads | 250 MB (2y) to 450 MB (6y) a `get`; 5.2–8.7 GB at 16–32 concurrent | **Blocker** (B1) |
| F5 | A per-person metric about 335 MB, `time_in_state` about 450 MB, a type scan about 300 MB (6y) | Reproduced (`heavymem.py`) | Holds (within the guide's formula) |

## 4. The broken items, most severe first

1. **B1, blocker: a one-object read whose rules read a growing set runs unpooled and reads it whole.** The constructs are `transitions`, `attempts`, `declaration_version`, observation collections, and a Proposal's `last_verdict`; the same rules run in request guards under the writer.
   - **Smallest fix:** read each by its index, as `opposite_end` and `referrers` already are:
     - `o.transitions` by `World::events_of(id)` (`world.rs:688`, `object_id = ?1`), not `log()`;
     - `o.attempts` and a Proposal's last refusal by an attempt-log index on `object_id` and `proposal`;
     - `version_of` from the object's last event by index (`ORDER BY position DESC LIMIT 1`);
     - an observation collection by `with_key(kind, ["subject"], [id])`.
   - **Then:** extend `wide()` so anything still unbounded is classified.
   - **What it refuses:** nothing 0.2.0 accepted.
2. **B2 and B3, major: metric reads miss C5 at the profile.** Per-person metrics took 6.7 s p95 on 2y and 30 s, refused 503, on 6y; line metrics 2.8–10.8 s; standard metrics 1.0–2.1 s idle on 6y.
   - **Smallest fix:** compute an `objects` metric's derived inputs in one batched read per part table (`with_parts`, then group in one pass), and transitions metrics from the family's events by index. Measure again.
   - **If C5 is still missed:** materialise the per-object values metrics group, such as `total`, as stored columns the writes maintain (PRD's §12 note under C4), or accept each read by name in `operate.md`'s limits (ADR-0204 §3 C, §9.6).
   - **What it refuses:** nothing.
3. **E4, major: the urgent `-wal` cut answers 503 to every running read at the profile.** That includes worklists and a change's submit, every 25–40 s while reports run.
   - **Smallest fix:** give each pool its own epoch. When urgent, stop only the reads of pools that did not drain within the 1 s hold; short worklists and object reads end within it. Fixing B2 removes most cases, since reads then fit in the hold.
   - **What it refuses:** nothing.
4. **A1, major: at the month-end peak, request p95 is 0.6–0.8 s.** The writer answers about 65 writes a second on 6y even alone, and about 72 on 2y beside the 2× reads.
   - **Smallest fix:** make the writer's commit cheaper, about 140 KB of `-wal` a write as measured; for example, group the fsyncs of writes queued behind one another, acknowledging each after the shared commit, so durability is kept. Otherwise accept by name.
   - **What it refuses:** nothing.
5. **D5 with B2, major: clients retrying after their timeouts leave reports unanswerable.** D656 stays open.
   - **Smallest fix:** D656's planned stop: run the read where its connection's closing cancels it (ADR-0201 decision 11).
   - **What it refuses:** nothing.
6. **B5, major: attempt counts are quadratic, with no deadline check in the loop.**
   - **Smallest fix:** merge into a `BTreeMap` keyed by a canonical encoding of the key; read only the family's attempts (`read_attempts_after`); call `world.on_time()` in the loop.
   - **What it refuses:** nothing.
7. **B6, major: metric filters carry no cost bounds.**
   - **Smallest fix:** apply `nodes() > FILTER_NODES` and `costly()` (`reads.rs:946-953`) in `metric::page` after parsing.
   - **What it refuses:** a metric filter of more than 256 terms, aggregates nested more than 2 deep, or a set over 1,000 members, all of which 0.2.0 accepted. It restores C, not D, R, P, A, V or E, so it lands before 1.0.0 (§11).
8. **B7, major, unconfirmed: pooled whole-log reads hold about 1 GB each on 6y.** They are diagnostics, transitions metrics, and label and observation exports; 8 at once came to 6.1–8.6 GB against the guide's 8G.
   - **Smallest fix:** B3's indexed reads for metrics and the export, and diagnostics reading its window's events by an `occurred_at` index. Correct `operate.md:276`'s "8G" example by its own formula.
   - **What it refuses:** nothing.
9. **The minors, smallest fix each.** None refuses anything 0.2.0 accepted.
   - **A8:** worklist p95 at the profile is past the guide's figure. State figures under load in `operate.md:252-262`.
   - **A13:** `exceptions` is one unbounded page. Give it pages and its cursor, as `available`.
   - **A17:** submit under load takes 12–13.5 s against the guide's 6 s. State it.
   - **B4:** attempt metrics read their owner type first. Skip `of_type(owner)` for attempt sources.
   - **B9:** `history` with `follow` scans types. Use the successor index (`superseded_into`).
   - **B10:** the erasure's proposal and kind scans. Read by the `target` and `subject` indexes.
   - **B11:** `get` reads the whole history. Read the last event only.
   - **B12:** the labels and observation export scans. Read by the subject index and each row's latest event by index.
   - **B13:** the attempt-counts export is one unbounded page. Page it by day with `limit` and 8 MiB.
   - **D3:** writes queue unboundedly. Refuse `503` once more than N writes wait, N a flag.
   - **D4:** pooled waits refuse only at the deadline. Bound each pool's waiting queue and refuse at once past it.
   - **E3:** descriptors. Bound the read connections, and say in `operate.md:107` that each busy connection takes about 3 descriptors.
   - **F2:** metric pages. End them at 8 MiB or say so in `wire.md`.

## 5. Outside this area, one line each

- **Area 6:** `operate.md:107`'s `LimitNOFILE` rule and `operate.md:276`'s "give such a store 8G" both misstate what a deployment needs (E3, B7).
- **Area 6:** a service whose `--metrics-bind` port was taken says "listening on" for the socket, exits, and leaves the socket file behind (`logs/g2a.log` 02:39:32).
- **Area 3:** the checkers accept derivations over `this.transitions`, `o.attempts`, `declaration_version` and observation collections without noting their cost (K).
- **Area 2:** in `perf2.py` a reviewer's `label record` on a just-paid claim was refused `unsatisfied` 30 times. The clause was not inspected.
- **Area 5:** a `kill -9` under the 2× load and recovery after it was not run.

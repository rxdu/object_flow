#!/usr/bin/env python3
"""Indicative evidence for PRD C5: how long typical flow metrics take on SQLite
when computed on read from the interval index, as ADR-0084 decides.

This is a probe, not a checker: it asserts nothing and fails nothing. The PRD
leaves C5's target to be set by measurement, and this is one measurement, with
its limits stated so it is not over-read:

  - synthetic data, whose shape follows ADR-0083's interval index;
  - SQLite in memory, so no disk, and no PostgreSQL at all;
  - no visibility filter, which a metric read applies (ADR-0084);
  - 20,000 objects, roughly thirty times the first consumer's live objects
    (PRD N3), each with up to five state intervals.

The seed is fixed, so the data is reproducible; the timings depend on the
machine and are reported as the median and maximum of seven runs.
"""
import random
import sqlite3
import statistics
import time

random.seed(7)
c = sqlite3.connect(":memory:")
c.executescript("""
CREATE TABLE of_interval(object_id INTEGER, type TEXT, dim TEXT, value TEXT,
  entered_at REAL, left_at REAL, actor_kind TEXT, decl INTEGER);
CREATE INDEX iv_type_dim ON of_interval(type, dim, value, entered_at);
CREATE INDEX iv_open ON of_interval(type, dim, left_at);
""")
STATES = ["PREPARATION", "WAITING", "PDI", "READY", "DELIVERED"]
rows, t0 = [], 1.7e9
N_OBJ = 20000
for o in range(N_OBJ):
    t = t0 + random.random() * 3e7
    for s in STATES:
        dur = random.expovariate(1 / (3 * 86400))
        left = t + dur if s != "DELIVERED" else None
        if o % 7 == 0 and s == "READY":
            left = None                                   # some still open
        rows.append((o, "Delivery", "state", s, t, left,
                     random.choice(["human", "agent"]), 1))
        if left is None:
            break
        t = left
c.executemany("INSERT INTO of_interval VALUES (?,?,?,?,?,?,?,?)", rows)
print(f"sqlite {sqlite3.sqlite_version}: {len(rows)} intervals over {N_OBJ} objects")

Q = {
    "time in state, p80 by state and month": """
      WITH d AS (SELECT value, strftime('%Y-%m', entered_at, 'unixepoch') m,
                        left_at - entered_at dur
                 FROM of_interval
                 WHERE type='Delivery' AND dim='state' AND left_at IS NOT NULL),
           r AS (SELECT value, m, dur,
                        ROW_NUMBER() OVER (PARTITION BY value, m ORDER BY dur) rn,
                        COUNT(*) OVER (PARTITION BY value, m) n FROM d)
      SELECT value, m, MIN(dur) FROM r
      WHERE rn >= CAST(0.8*n AS INTEGER) + (0.8*n > CAST(0.8*n AS INTEGER))
      GROUP BY value, m""",
    "work in progress and oldest open, by state": """
      SELECT value, COUNT(*), MIN(entered_at) FROM of_interval
      WHERE type='Delivery' AND dim='state' AND left_at IS NULL GROUP BY value""",
    "throughput per week, by actor kind": """
      SELECT strftime('%Y-%W', entered_at, 'unixepoch'), actor_kind, COUNT(*)
      FROM of_interval
      WHERE type='Delivery' AND dim='state' AND value='DELIVERED' GROUP BY 1, 2""",
}
for name, sql in Q.items():
    ts = []
    for _ in range(7):
        a = time.perf_counter()
        c.execute(sql).fetchall()
        ts.append((time.perf_counter() - a) * 1000)
    print(f"  {name}: median {statistics.median(ts):.1f} ms, max {max(ts):.1f} ms")

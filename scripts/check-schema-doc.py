#!/usr/bin/env python3
"""Run every SQL block in the storage schema document against SQLite, and
probe the three claims the document makes about SQLite's runtime behaviour.

The specification's examples are checked against its own rules; this does the
same for the schema's.  A DDL statement that does not execute is a defect in the
document, and the only way to know is to run it.

SQLite resolves foreign keys lazily, so a fragment may reference a table the
document defines elsewhere or not at all.  That is deliberate: the fragments are
excerpts, and this checks that each is well formed, not that the excerpt set is
closed.

The probe (ADR-0076, D187): the sequence is allocated on a second connection
while the request's transaction is open.  SQLite locks the file, so that
connection must open a separate database file, or it deadlocks against the
request.  Both halves are asserted -- the same-file connection fails, the
separate-file one succeeds and its allocation survives the request's rollback --
because a probe that showed only the working case would keep passing after the
reason for the design had quietly changed.
"""
import re
import sys
import sqlite3
import pathlib
import tempfile

DOC = pathlib.Path(__file__).resolve().parents[1] / "docs/design/storage-schema.md"


def statements_of(text):
    blocks = [(text[: m.start()].count("\n") + 2, m.group(1))
              for m in re.finditer(r"```sql\n(.*?)```", text, flags=re.S)]
    out = []
    for line, block in blocks:
        # strip comments before splitting: a comment may contain a semicolon
        bare = "\n".join(re.sub(r"--.*$", "", ln) for ln in block.split("\n"))
        out.extend((line, s.strip()) for s in bare.split(";") if s.strip())
    return len(blocks), out


def probe_sequence_isolation(object_ddl, sequence_ddl):
    """The document's own DDL, two connections, both journal modes.

    Returns the list of ways the claim failed; empty means it holds.
    """
    mint = ("UPDATE ok_sequence SET next_value = next_value + 1 "
            "WHERE name = 'unit_serial' AND scope_key = 'RB1'")
    failures = []
    for journal in ("delete", "wal"):
        with tempfile.TemporaryDirectory() as d:
            store = pathlib.Path(d) / "store.db"
            seqfile = pathlib.Path(d) / "store.seq"

            req = sqlite3.connect(store, isolation_level=None, timeout=0.2)
            req.execute(f"PRAGMA journal_mode={journal}")
            req.execute(object_ddl)
            req.execute(sequence_ddl)
            req.execute("INSERT INTO ok_sequence VALUES ('unit_serial', 'RB1', 1)")

            same = sqlite3.connect(store, isolation_level=None, timeout=0.2)
            sep = sqlite3.connect(seqfile, isolation_level=None, timeout=0.2)
            sep.execute(f"PRAGMA journal_mode={journal}")
            sep.execute(sequence_ddl)
            sep.execute("INSERT INTO ok_sequence VALUES ('unit_serial', 'RB1', 1)")

            # the request has begun and has written: the directory row here,
            # the event position in a real request, before any outcome step
            req.execute("BEGIN")
            req.execute("INSERT INTO ok_object VALUES ('obj_1', 'Robot', '2026-09-09', 'human')")

            try:
                same.execute(mint)
                failures.append(f"{journal}: a second connection to the store's own file "
                                "wrote while the request held the lock, so the reason for "
                                "a separate file no longer holds")
            except sqlite3.OperationalError as e:
                if "locked" not in str(e):
                    failures.append(f"{journal}: same-file mint failed for an unexpected "
                                    f"reason: {e}")

            try:
                sep.execute(mint)
            except sqlite3.OperationalError as e:
                failures.append(f"{journal}: mint on the separate file failed: {e}")

            req.execute("ROLLBACK")
            value = sep.execute("SELECT next_value FROM ok_sequence").fetchone()[0]
            if value != 2:
                failures.append(f"{journal}: the allocation did not survive the request's "
                                f"rollback (next_value = {value}, expected 2)")
            for c in (req, same, sep):
                c.close()
    return failures


def probe_deferred_foreign_keys():
    """ADR-0077, D191: a stored end's foreign key is DEFERRABLE INITIALLY
    DEFERRED and checked at commit, so one transaction can insert two rows that
    require each other.  Asserted three ways: the deferred pair commits; the
    same pair without the clause fails on its first insert, so the clause is
    what makes it work; and a dangling reference under a deferred constraint
    still fails, at commit, so the constraint is still a backstop."""
    failures = []

    def fresh(deferrable):
        c = sqlite3.connect(":memory:", isolation_level=None)
        c.execute("PRAGMA foreign_keys = ON")
        d = " DEFERRABLE INITIALLY DEFERRED" if deferrable else ""
        c.execute(f"CREATE TABLE t_a (id TEXT PRIMARY KEY, b_id TEXT NOT NULL REFERENCES t_b(id){d})")
        c.execute(f"CREATE TABLE t_b (id TEXT PRIMARY KEY, a_id TEXT NOT NULL REFERENCES t_a(id){d})")
        return c

    c = fresh(True)
    try:
        c.execute("BEGIN")
        c.execute("INSERT INTO t_a VALUES ('a1', 'b1')")
        c.execute("INSERT INTO t_b VALUES ('b1', 'a1')")
        c.execute("COMMIT")
    except sqlite3.Error as e:
        failures.append(f"deferred: a mutually required pair did not commit: {e}")

    c = fresh(False)
    try:
        c.execute("BEGIN")
        c.execute("INSERT INTO t_a VALUES ('a1', 'b1')")
        c.execute("INSERT INTO t_b VALUES ('b1', 'a1')")
        c.execute("COMMIT")
        failures.append("immediate: the pair committed without the clause, so the clause is not what makes it work")
    except sqlite3.IntegrityError:
        pass

    c = fresh(True)
    try:
        c.execute("BEGIN")
        c.execute("INSERT INTO t_a VALUES ('a2', 'nowhere')")
        c.execute("COMMIT")
        failures.append("deferred: a dangling reference committed, so the constraint is no backstop")
    except sqlite3.IntegrityError:
        try:
            c.execute("ROLLBACK")
        except sqlite3.Error:
            pass
    return failures


def probe_transaction_mode():
    """ADR-0090, D204: every SQLite transition transaction begins IMMEDIATE.

    A request reads (its guards), another request commits a write, and the
    first then writes (its outcome).  Asserted both ways, so the probe fails if
    the reason for the decision ever changes: under a deferred BEGIN in WAL
    mode the first request's write fails with 'locked', which no timeout
    resolves, since its snapshot is stale; under BEGIN IMMEDIATE the second
    request cannot begin while the first holds the lock, so the first commits.
    """
    failures = []
    for begin in ("DEFERRED", "IMMEDIATE"):
        with tempfile.TemporaryDirectory() as d:
            path = pathlib.Path(d) / "store.db"
            setup = sqlite3.connect(path, isolation_level=None)
            setup.execute("PRAGMA journal_mode=WAL")
            setup.execute("CREATE TABLE t_stock (id INTEGER PRIMARY KEY, reserved INTEGER)")
            setup.execute("INSERT INTO t_stock VALUES (1, 0)")
            setup.close()
            a = sqlite3.connect(path, isolation_level=None, timeout=0.2)
            b = sqlite3.connect(path, isolation_level=None, timeout=0.2)
            a.execute(f"BEGIN {begin}")
            a.execute("SELECT reserved FROM t_stock WHERE id = 1").fetchone()
            b_committed = True
            try:
                b.execute(f"BEGIN {begin}")
                b.execute("UPDATE t_stock SET reserved = reserved + 1 WHERE id = 1")
                b.execute("COMMIT")
            except sqlite3.OperationalError:
                b_committed = False
                try:
                    b.execute("ROLLBACK")
                except sqlite3.OperationalError:
                    pass
            try:
                a.execute("UPDATE t_stock SET reserved = reserved + 1 WHERE id = 1")
                a.execute("COMMIT")
                a_committed = True
            except sqlite3.OperationalError:
                a_committed = False
            if begin == "DEFERRED" and not (b_committed and not a_committed):
                failures.append("deferred: the read-then-write request was not refused after "
                                "another committed, so the reason to begin IMMEDIATE no longer holds")
            if begin == "IMMEDIATE" and not (a_committed and not b_committed):
                failures.append("immediate: the first request did not hold the lock from its "
                                "first statement")
            a.close(); b.close()
    return failures


def main():
    text = DOC.read_text()
    nblocks, stmts = statements_of(text)
    if not nblocks:
        print("no SQL blocks found"); return 1

    conn = sqlite3.connect(":memory:")
    failed = []
    for line, stmt in stmts:
        try:
            conn.execute(stmt)
        except sqlite3.Error as e:
            first = stmt.split("\n")[0][:60]
            failed.append(f"  line {line:>4}  {e}  in: {first}…")

    tables = [r[0] for r in conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")]
    print(f"{DOC.name}: {nblocks} SQL blocks, {len(stmts)} statements, "
          f"{len(tables)} tables created")
    if failed:
        print("\n".join(failed))
        print(f"{len(failed)} statement(s) failed")
        return 1
    print("every statement executes")

    ddl = {name: s for _, s in stmts for name in re.findall(r"^CREATE TABLE (\w+)", s)}
    missing = [t for t in ("ok_object", "ok_sequence") if t not in ddl]
    if missing:
        print(f"the probe needs {missing} declared in the document and cannot find them")
        return 1
    probe = probe_sequence_isolation(ddl["ok_object"], ddl["ok_sequence"])
    if probe:
        print("\n".join("  " + f for f in probe))
        print("the sequence-isolation claim of §6 does not hold")
        return 1
    print("sequence probe: a second connection to the store's file blocks behind the "
          "request in both journal modes; a separate file does not, and its allocation "
          "survives the rollback")
    fk = probe_deferred_foreign_keys()
    if fk:
        print("\n".join("  " + f for f in fk))
        print("the deferred-foreign-key claim of §3 does not hold")
        return 1
    print("foreign-key probe: a deferred pair that require each other commits, the same "
          "pair without the clause does not, and a dangling reference still fails at commit")
    tm = probe_transaction_mode()
    if tm:
        print("\n".join("  " + f for f in tm))
        print("the transaction-mode claim of §7 does not hold")
        return 1
    print("transaction-mode probe: under a deferred BEGIN a read-then-write fails after another "
          "commit; under BEGIN IMMEDIATE the first holds the lock and commits")
    return 0


sys.exit(main())

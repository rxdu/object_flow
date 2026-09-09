#!/usr/bin/env python3
"""Run every SQL block in the storage schema document against SQLite.

The specification's examples are checked against its own rules; this does the
same for the schema's.  A DDL statement that does not execute is a defect in the
document, and the only way to know is to run it.

SQLite resolves foreign keys lazily, so a fragment may reference a table the
document defines elsewhere or not at all.  That is deliberate: the fragments are
excerpts, and this checks that each is well formed, not that the excerpt set is
closed.
"""
import re
import sys
import sqlite3
import pathlib

DOC = pathlib.Path(__file__).resolve().parents[1] / "docs/design/storage-schema.md"


def main():
    text = DOC.read_text()
    blocks = [(text[: m.start()].count("\n") + 2, m.group(1))
              for m in re.finditer(r"```sql\n(.*?)```", text, flags=re.S)]
    if not blocks:
        print("no SQL blocks found"); return 1

    conn = sqlite3.connect(":memory:")
    failed = []
    statements = 0
    for line, block in blocks:
        # strip comments before splitting: a comment may contain a semicolon
        bare = "\n".join(re.sub(r"--.*$", "", ln) for ln in block.split("\n"))
        for stmt in [s.strip() for s in bare.split(";") if s.strip()]:
            statements += 1
            try:
                conn.execute(stmt)
            except sqlite3.Error as e:
                first = stmt.split("\n")[0][:60]
                failed.append(f"  line {line:>4}  {e}  in: {first}…")

    tables = [r[0] for r in conn.execute(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")]
    print(f"{DOC.name}: {len(blocks)} SQL blocks, {statements} statements, "
          f"{len(tables)} tables created")
    if failed:
        print("\n".join(failed))
        print(f"{len(failed)} statement(s) failed")
        return 1
    print("every statement executes")
    return 0


sys.exit(main())

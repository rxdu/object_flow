# ADR-0094: The export pages the log and the flow data by the settled cursor, and omits every personal value

- **Status:** Accepted by the author, 2026-09-23, after a check of every decision against the design — decided at the author's direction, against `docs/PRD.md` M5, T5, D8 and N2
- **Date:** 2026-09-23
- **Refined by:** ADR-0100 — attempt rows carry their writing transaction, and an interval is re-emitted when it closes.
- **Refines:** ADR-0081

## Context

PRD M5 (Could): "The engine exports its data in a shape suited to exploratory analysis elsewhere, and does not attempt the exploration itself." ADR-0081 decided that exploration is exported, not built, and said nothing of how. Two constraints shape the answer.

- **An export must not miss a late commit.** It is a consumer advancing a cursor, which D210 showed was unsafe until ADR-0089.
- **An export must not become a copy erasure cannot reach.** D8 and UC-17 require personal values to be removable, and a file on an analyst's laptop is outside the store.

## Decision

### 1. `export(source, cursor)` joins the read surface

It pages rows of one source, in the settled cursor's order (ADR-0089), and returns the next cursor. The source is one of:
- the log;
- a type's `intervals`, `transitions` or `attempts`;
- an observation kind.

### 2. Rows are JSON lines

One row per line, the format the import's extract already uses (`publish-and-import.md` §8). It streams, and every exploratory tool reads it.

### 3. The export omits every personal value

Personal attributes and fields, personal inputs, and label notes are left out of every row. The export is therefore never a copy that erasure would have to chase. An analysis that needs a personal value is not exploration, and belongs inside the store, under its visibility and its erasure.

### 4. The reader's visibility applies

As it does to every read (ADR-0037). An export is an actor's read, not a privileged dump.

### 5. The store does nothing more with it

There are no schedules, destinations or formats beyond JSON lines. A consumer runs the export and puts the file where its tools are (N2, ADR-0012).

## Alternatives rejected

- **Export with personal values, and mark exported files for erasure.** The store cannot reach a file it does not hold, so the mark would be a promise it cannot keep.
- **A database replica for analysts.** It bypasses visibility and the personal-value rule, and it is the second read path this design keeps refusing.
- **Parquet or another columnar format.** Better for large analysis. But at N3's scale JSON lines is enough, and it adds no dependency. A consumer can convert.

## Consequences

- `DESIGN.md` §10 gains `export`, and §12's exploratory-analysis row says what feeds it.
- `library-api.md` gains `export`, which `scripts/check-api-doc.py` holds against `DESIGN.md` §10.

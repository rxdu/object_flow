# ADR-0246: The upgrade to 0.13.0 carries the write index a renamed set or end left under its old name

- **Status:** **Decided** 2026-11-03 under the author's direction ("go with your recommendations"), on the recommendation given the same day. Built the same day.
- **Date:** 2026-11-03
- **Refines:** ADR-0177 (a store's upgrade, one transaction a release)
- **Relates to:** ADR-0230 (`renamed_attributes`); issue #59 and the defect found beside it, fixed for 0.13.0 (`CHANGELOG.md`); `storage-schema.md` §10 (a renamed attribute's write-index rows are carried)

## Context

A store's write index, `of_attribute_write`, holds each object's latest write of each attribute, which `changed_since` reads. A version renaming a stored set, such as a card's `tags`, renamed the set's table and left its write-index rows under the old name. A version renaming an end without a column, alone in its type, left them too. A column's rename carried its rows. 0.13.0's publish carries every rename's rows (`store.rs`, `install`), but a store renamed at 0.12.0 or before keeps the old rows.

Probed 2026-11-03 (`TODO.md`, 0.13.0's version and release entry): on a SQLite store the published 0.12.0 made, a card's `tags` renamed `keywords` after its sign-off left the row `('tags', 7)`. 0.13.0's build, serving that store, still answered `close` satisfied, which the guard reading `changed_since([keywords], signed_at)` refused before the rename.

## Decision

1. **0.13.0's step from 0.12.0 carries those rows,** in the upgrade's one transaction, so its dry run shows it and a failure installs nothing.
   - It reads each version from the second, in order, from the sources `of_declaration` keeps, so a chain of renames is followed.
   - For each rename the version made of a stored set or an end without a column, each object whose type under that version the rename reaches has its row of the old name moved to the new name. The object's type is read through its own moves.
   - Only a row written before the version's publish moves. A row written after it belongs to a later version's attribute of the old name, and stays.
   - Where the object also holds a row of the new name, that row is a later write's, and the old one is dropped.
2. **A column's rows are not touched,** since every release's publish carried them.

## Alternatives rejected

- **No repair, the stale rows described in `CHANGELOG.md` and `operate.md` with a query to find them.** A guard reading `changed_since` of such an attribute stays wrong on each object the rename reached, until the attribute is next written.
- **Every row of the old name renamed, with no bound.** It is wrong where a later version declared the old name again and wrote it.

## Consequences

- Built in `release.rs`: the 0.13.0 entry and `renamed_write_index`.
- **Tested on both backends** by `the_upgrade_to_0_13_0_carries_the_write_index_a_rename_left`. Its stale rows are planted as 0.12.0 left them, a set's and a composite end's. It holds a later write of the new name, a later version declaring the old name again, a chain of two renames, and a memo's column of the old name. Each row ends as the decision says, and each object's `close` is refused again.
- **Planted defects, each caught on both backends:** the step moving nothing, its rows unbounded by the publish, its types unchecked, a newer row of the new name overwritten, the versions read newest first, and the ends without a column left out.
- **Checked 2026-11-03 on stores the published 0.12.0 made,** on SQLite and on PostgreSQL. 0.12.0 left the card's row `('tags', 7)` and let `close` through after the rename. 0.13.0's dry run left the row, and its upgrade carried it to `('keywords', 7)`, after which `close` was refused, as before the rename.
- `operate.md` says what the step does, and `CHANGELOG.md` lists it.

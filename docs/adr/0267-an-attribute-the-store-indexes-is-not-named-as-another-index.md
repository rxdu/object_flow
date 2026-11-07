# ADR-0267: An attribute the store indexes is not named as another index

- **Status:** **Decided** 2026-11-07 by the author, as recommended ("go with your recommendations"), on the options put the same day in `TODO.md`. Built the same day.
- **Date:** 2026-11-07
- **Refines:** ADR-0265 (a name the store keeps a table or a column under is refused at step 3)
- **Relates to:** D578 (an attribute named as a reference's column); ADR-0194 decision 1 (an actor identity's folded index); `storage-schema.md` §5 (the store's indexes)

## Context

Addressing issue #72, ADR-0265 found that the names of the store's indexes, which it derives from an attribute's name, may also collide. Probed 2026-11-07 on fresh SQLite stores at `4d125be`, each module passing both checkers:

- An indexed attribute `asserted` was published with no index of its own. The store's index on `state_source` is `t_<type>_asserted_ix`, and an attribute's index is made `IF NOT EXISTS` (`store.rs`, `attribute_indexes`).
- An indexed `login_fold` beside the actor identity `login` was published, and the identity's folded index was lost. `t_member_login_fold_ix` was made on `login_fold`, and no index on `lower(login)`, which every request naming an actor by another case reads by.
- A unique `code` beside an indexed `code_key` was refused by the dry run in the store's words: "index t_thing_code_key_ix already exists".

The store indexes an attribute marked `indexed` or `external` that holds one value and is no reference, as `t_<type>_<attribute>_ix`. A uniqueness's key is `t_<type>_<attribute>_key_ix`, and an actor identity's folded index `t_<type>_<attribute>_fold_ix`. Its other indexes are on names ADR-0265 refuses already, `state`, `last_event` and `superseded_by`, or on a built-in type alone.

## Decision

1. **An attribute the store indexes by its own name is not named so that its index takes a name the store gives another.** It is not named `asserted`, nor `<a>_key` beside an attribute `a` declaring `unique`, nor `<a>_fold` beside the actor identity `a`. Both checkers refuse it at step 3, `names`, naming the index it would take.
2. **An observation kind's attribute is not held to it**, since step 3 refuses marking one `indexed`.

## Alternatives rejected

- **Naming the store's own indexes in a form no attribute's can take**, such as `t_<type>__<what>__<attribute>`. It closes the class, but needs an upgrade step renaming every index of every released store, on both backends. It also lengthens names toward the 63 bytes PostgreSQL keeps. It is the choice should the store gain further indexes of its own, each of which would add to decision 1's list.
- **Leaving it**, the dry run refusing the third. The first two are silent: a store loses an index and serves every read by the attribute, or every request naming an actor by another case, without it.
- **Exempting an attribute that is itself unique alone**, whose plain index the store does not make, nor a `<a>_key` beside a reference's uniqueness, which makes no key index. Each would hold one more rule in both checkers for a name an author can choose freely.

## Consequences

- **The core.** `check/view.rs`: `column_errors` holds decision 1.
- **`check-flows.py`** holds the same, in the same words.
- **It refuses what 0.20.0 accepted**: a module naming an indexed attribute so. Such a module was published with an index lost, or refused by the dry run.
- `flow-format.md` §3 rule 3 states decision 1.
- **Tested.** `index_name_plants` in `check-flows.py`, which both checkers agree on through the core's oracle:
  - each name refused where it is indexed;
  - each name clean unindexed, an indexed `asserted_by` clean, and an indexed `code_key` beside a `code` with no uniqueness clean.

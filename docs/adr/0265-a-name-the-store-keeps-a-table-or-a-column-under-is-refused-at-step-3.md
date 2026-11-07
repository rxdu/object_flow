# ADR-0265: A name the store keeps a table or a column under is refused at step 3

- **Status:** **Decided** 2026-11-07 under the author's direction ("review latest open issues and address them properly"), on the recommendation formed the same day for issue #72 and the defects found addressing it, accepted in advance as the author directed for a confident recommendation. Built the same day.
- **Date:** 2026-11-07
- **Refines:** `flow-format.md` §3 rule 3 (the names a member may not take); ADR-0163 decision 5 (a store holds one type of a name)
- **Relates to:** D578 (an attribute named as a reference's column); ADR-0254 decision 6 (a value's field columns); ADR-0082 (an observation kind is a type); `DESIGN.md` §9 (the built-in types are not user-definable); issue #72

## Context

Issue #72, filed 2026-11-07 at 0.18.0, reported that an attribute named `version`, `last_event`, `state_source` or `superseded_by` passes step 3. Every row of a type's table has those columns (`storage-schema.md` §3.2), `superseded_by` only on a type with a superseding state. The publish then refuses it with "duplicate column name", naming no attribute, type or rule. Step 3 refuses `id`, `state` and `declaration_version` as names, and D578 refuses an attribute named as a reference's column.

Reproduced 2026-11-07 on a fresh SQLite store at 0.19.0's commit `4d125be`. A type with an attribute `version` was refused `example_failed` with an example, and `table_change:Thing` without one, both in the store's words, after both checkers passed it.

The same probe found more names the store keeps that step 3 passes:

- **An observation kind's attribute** named `version`, refused alike by the dry run. One named `subject_id` or `corrects_id`, the columns its subject and correction are kept in, passed both checkers and was refused only at the core's load.
- **Two types named alike but for case**, `ThingA` and `Thinga`. A store keeps a type's objects in a table named by its name in lower case, `t_thinga` for both, so the dry run refused it: "table t_thinga already exists". SQLite compares table names ignoring case, quoted or not (observed on SQLite 3.37.2).
- **An observation kind named as a type**, or one kind declared on two types. Both passed step 3 and the dry run, one kind or type then standing for the other in the library.
- **A type named as a built-in one.** `DESIGN.md` §9 says `Operator`, `Proposal`, `DeclarationChange` and `Subscription` are not user-definable, and `label` is a built-in observation kind. Both checkers passed each, and the dry run passed each named exactly so. **A version declaring a type `Operator` was published, and every request after it was answered `500`**: the store's operator could no longer be read, "no such column: t.note". So a fixing version could not be published either. A version declaring `DeclarationChange` failed its approval `500` and was rolled back, the store left at version 0. One declaring `Subscription` was published. Each name in another case, `LABEL` or `PROPOSAL`, was refused by the dry run in the store's words.

## Decision

1. **An attribute the store keeps under its own name is not named as a column every row of its table has**: `version`, `last_event`, `state_source` or `superseded_by`. Such an attribute holds a single value that is no reference, since a reference is kept as `<name>_id` and a set in a table of its own. This holds on every type, whether or not it has a superseding state.
2. **An observation kind's attribute likewise, and not `subject_id` or `corrects_id`.** Check 54 already keeps `subject`, `corrects` and `occurred_at`.
3. **A type or an observation kind is not named as another, ignoring case, that any module checked with it declares, nor as a built-in type.** The built-in types are `Operator`, `Proposal`, `DeclarationChange`, `Subscription` and `label`. Two types of exactly one name stay refused as ADR-0163 and check 33 refuse them, once.
4. **Both checkers refuse each at step 3, `names`**, saying the store keeps a table or a column under the name. The core's load does not, so an installed version naming one is served as before, and its next version is refused until it renames it.

## Alternatives rejected

- **`superseded_by` refused only on a type with a superseding state.** A later version adding such a state would be refused in the store's words, and would first need the attribute renamed. A name reserved now can be released later without breaking anyone; one reserved later breaks whoever took it.
- **Every member of these names refused**, a reference and a set among them. Their columns are `<name>_id` or a table of their own, so nothing collides. A reference named `superseded_by` is natural for a document that names its replacement without being superseded.
- **Leaving it to the dry run.** Its words name a database column. It also passed a built-in's name, and a store that published `Operator` answered every later request `500`.
- **Refusing at the core's load.** A store that installed a version declaring `Subscription` would no longer start.
- **Naming each table by its type's exact case.** SQLite compares table names ignoring case even quoted, and every store's tables would need an upgrade step.

## Consequences

- **The core.** `check/view.rs`: `column_errors` holds decisions 1 and 2. `check/mod.rs`: decision 3, the built-in names taken from `model.rs`.
- **`check-flows.py`** holds the same, in the same words.
- **It refuses what 0.19.0 accepted**: a module naming one of these. Such a module could not be published, except for a type or a kind named exactly as another, or as a built-in type, as `Operator` and `Subscription` were.
- A store that installed a version declaring `Operator` is not repaired by this decision.
- `flow-format.md` §3 rule 3 states decisions 1 to 3, and `storage-schema.md` §3.2 points to them.
- **Found and not decided here:** index names the store derives from an attribute's name collide too. An indexed attribute named `asserted` is published without its index, since the store's own index on `state_source` is `t_<type>_asserted_ix`. An indexed `login_fold` beside the actor identity `login` takes the name of the identity's folded index, which is then not made. A unique `code` beside an indexed `code_key` is refused by the dry run in the store's words. `TODO.md` puts the fix to the author.
- **Tested.** `store_name_plants` in `check-flows.py`, sixteen cases both checkers agree on through the core's oracle:
  - a reference and a set of these names are clean;
  - each column name is refused, an observation kind's three among them;
  - two types alike but for case are refused, one in each of two modules among them;
  - a kind named as a type, one kind on two types, and three built-in names are refused;
  - two types of one name in two modules are refused once, by ADR-0163.

  `renamed_enumeration_plants` named a type `Label`, which no store could hold beside its labels, and names it `Sticker`.

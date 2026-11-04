# ADR-0254: A value of several fields is declared once with its rules

- **Status:** **Decided** 2026-11-04 under the author's direction ("go with your recommendations"), on the recommendation and five questions given the same day after researching issue #65's second and third cases. Its first slice, decision 9's, built 2026-11-04: decisions 1 to 3, 4's invariants as ADR-0255 names them, 5, 6 and 7.
- **Date:** 2026-11-04
- **Refined by:** ADR-0255 — decision 4's invariant broken on a holder is named `<attribute>_<invariant>`, as the engine's other generated invariants are, where `<attribute>.<invariant>` conflicted with a dot naming a type
- **Relates to:** PRD C2 and M7 (a rule declared once), F10 (flows clearly described); issue #65; `docs/research/declaring-a-value-of-several-fields.md`; ADR-0251 (a creation's guard reads what the creation takes)

## Context

Issue #65, filed 2026-11-04, asked whether a flow may name a repeated part once. Its second case is a group of inputs that several transitions take, each transition declaring it again. Its third is a group of attributes that a type holds twice, such as proposed dates beside the dates they would replace, with the same conditions declared on each. The format has no form for either:

- A part is an object with an identity of its own (`flow-format.md` §4.3).
- A family shares members among types, never within one (§4.14).
- A condition is declared on one type (§4.7).

Its first case, a named list of steps, already has a form: an internal transition with `only_via`, called on the object itself (`TODO.md`, #65).

The research (`docs/research/declaring-a-value-of-several-fields.md`) found that every source separates a value, equal by its fields, from an object with an identity, and gives the value its own rules:

- UML's structured data type owns its constraints.
- PostgreSQL puts a composite type's rules on a domain over it.
- Jakarta Validation checks an embeddable's rules wherever an entity holds it.

The sources store such a value in two ways: its fields as columns of the holder's table (Jakarta Persistence), or one composite column (PostgreSQL). Only the first fits both of ObjectFlow's backends, since SQLite has no composite type.

## Decision

1. **A module declares `values`**, each named in upper camel case, as a type is. Each value declares a `description`, its fields under `attributes`, and its rules under `invariants` and `conditions`. A field takes a built-in type or an enumeration, and may be `optional`.
2. **An attribute or a declared input names a value as its `type`**, as many times as needed, whether optional or required. The value is absent as a whole, or present with every field it requires.
3. **A value is equal by its fields**, as UML's structured data type is. It has no id, no state and no events of its own; its holder's events record it.
4. **Its rules are declared once, and apply wherever it is held.**
    - A value's invariant holds for each attribute holding it. A write breaking it is refused as the holder's invariant `<attribute>.<invariant>`.
    - A value's condition is named by a guard through the attribute or input holding it.
5. **An expression reads a field through its holder**, as `proposed_days.first` and `inputs.proposal.first`. An effect assigns or clears a value whole.
6. **The store keeps each field as a column of the holder's table**, named for the attribute and the field. A filter, an index and an invariant read a field as they read an attribute. The checker refuses a member whose name the store would give a field's column, as it refuses `<reference>_id` (D578).
7. **The wire and an event's payload answer a value as a nested object** of its fields. An absent value answers as an absent attribute does.
8. **A field may be `personal` by itself.** Erasure redacts it, the value's other fields staying, since erasure already works column by column (`storage-schema.md` §9).
9. **The first slice, for 0.16.0**, builds decisions 1 to 3, 4's invariants, 5, 6 and 7, on both backends and in both runners and both checkers. Later slices build:
    - 4's conditions, with the notation a guard uses to name a value's condition;
    - decision 8, `personal` fields and their erasure;
    - a version renaming a field, carried into every holder's columns;
    - references, sets and files as fields;
    - writing one field of a held value.
10. **Check 40 and the order of a request stay as they are**: the state is written, then the effect runs (`DESIGN.md` §6, step 5). A transition into a final state still cannot call an internal transition on its own object. With a value, clearing a group is one step, `clear: [proposed_days]`, which such a transition writes itself.

## Alternatives rejected

- **A part type for the group.** It is an object with an identity and a lifecycle of its own. Its rules are still declared beside the holder's own, since a condition belongs to one type, as #65 found.
- **A family.** It shares members among types, never within one type held twice.
- **One JSON column per value.** A filter, an index and an invariant reading a field would read JSON, and lose what a column gives them on both backends.
- **PostgreSQL's composite types.** SQLite has none, and both backends store alike.
- **Named groups of inputs alone.** They answer #65's second case and not its third, two mechanisms where one value serves both.
- **Invariants alone as a value's rules**, as UML's constraints are. #65's repeated rules are conditions that guards read.
- **A value personal only as a whole.** It erases fields that hold nothing personal, where erasure already works column by column.
- **A flat dotted name on the wire**, such as `proposed_days.first`. Avro and Protocol Buffers answer a record as a nested object, and a reader reads the value whole.
- **A transition into a final state running the shared steps a self-call names.** It needs either the effect to run before the state is written, against `DESIGN.md` §6, or an internal transition taken in a final state, against check 40. A value removes the case #65 had.

## Consequences

- To be built in the format's schema, `flow-format.md` §4 and `declaration-syntax.md`, both checkers, both runners, the store on both backends, the wire, and the Flow Studio. `TODO.md` tracks the first slice and the later ones.
- **It refuses nothing 0.15.0 accepted**, since `values` is a new key of format 1, as ADR-0180 decision 3 allows a release to add.

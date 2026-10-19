# ADR-0219: A query's personal members judged by their types

- **Status:** **Decided** 2026-10-18 under the author's direction ("we need to address the issues properly so that they don't have to work around it"), for issue #23, which the operations platform's developer filed; the issue's first proposal.
- **Date:** 2026-10-18
- **Refines:** ADR-0206 decision 1 (what a personal member is) and decision 2 (a query refused `personal` by its filter or order)
- **Refined by:** ADR-0220 — decision 4's guards of a transition's answer judged by type, and its claim about the reference runner retracted
- **Relates to:** ADR-0203 decision 1 (a copy of an identity is the identity); ADR-0218 (a filter's parameters); issue #23

## Context

ADR-0206 makes a query that filters or orders by a personal member refused `personal` unless its request opts in. The core judged a member personal by its **name in any type**: a contact's personal `name` made a room's plain `name` personal too, so ordering rooms by name was refused once another module marked a person's name personal, in a module that never mentions people, and only at query time (issue #23, reproduced on 0.3.0). The built-in `Label`'s personal `note` did the same to every type's `note`. ADR-0206 gave no reason for the name-wide rule; it was how the guard, run before anything is evaluated, read the filter's names. It fails safe, and it made a personal attribute's name reserved for the whole deployment, which nothing checked or said, and pushed callers to set `reads_personal` broadly, eroding the opt-in.

## Decision

1. **A query judges each member as the type it belongs to declares it.** A bare member, or one read through `this`, is the queried type's; a path through single references, `host.name`, is the type the path reaches; a name an aggregate binds, `g` in `count(g in guests where g.name == …)` or `c` in `count(c in Contact where …)`, ranges over the collection's element type, a set of references' target or the type scanned.
2. **A member is personal on its type** where the type marks it `personal`, where it is a derived attribute reading a personal member its expression reaches, judged the same way to a fixpoint, and, where an actor type marks its identity personal, where it is of type `identity` (ADR-0206 decision 1, unchanged).
3. **Only a member whose type the filter does not tell is judged by its name in any type**, as every member was: a path through something not a single reference, or a binding over a collection whose elements are not objects of a known type. The guard still runs before anything is evaluated, from the flow's declarations alone.
4. **The guards of a transition's answer** (`available`, `check`, `availability`) are judged as before, by name; judging them by type is a change to both backends, recorded in `TODO.md`. *(Retracted 2026-10-19: "a change to both backends" was written without being checked, and is wrong. The reference runner has no guard on these three reads, as ADR-0206's consequences say, so the change is the core's alone. ADR-0220 makes it.)*

## Alternatives rejected

- **A notice at publish when a personal attribute shares its name with a non-personal member elsewhere**, the issue's fallback: says the collision where it is made, and still refuses the room list.
- **Keeping the name-wide rule and telling callers to set `reads_personal`**: the opt-in is for a caller entitled to read by a personal value, not for reading a room by its name.

## Consequences

- `reads.rs`: `Personal`, its `reads` walk and `member`; `query_reads_personal` takes the queried type. The service passes it.
- `wire.md` states the rule, which the guides did not, the issue having found it in the code.
- Tests: `a_querys_members_are_personal_as_their_own_types_declare` (the issue's office: a room's own name not personal, filtered, ordered or through a derived attribute; a contact's name personal on the contact, through a reference, through two aggregates' bindings and through a derived attribute), and `a_filters_parameters_are_read_as_their_members`, whose `note` the built-in `Label` no longer makes personal.

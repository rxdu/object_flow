# ADR-0253: A move renames an actor's type, its actors kept

- **Status:** **Decided** 2026-11-04 under the author's direction ("go with your recommendations"), on the recommendation given the same day for issue #66. Built the same day.
- **Date:** 2026-11-04
- **Refines:** ADR-0229 decision 6 (an actor's type does not move), which its rejected alternatives left until one asked
- **Relates to:** PRD F2 and F5; ADR-0110 and ADR-0194 (an actor is an object of a type marking an identity, held once across every such type); issue #66

## Context

Issue #66, filed 2026-11-04 at 0.13.0, found that a deployment cannot rename its actors' type. The only way to rename a type is a move (ADR-0229), and a move from or to an actor's type is refused. Reproduced 2026-11-04 on the published 0.15.0 (`TODO.md`, #66): `Member` declared `principal_id: { type: identity, actor_kind: human, unique: true }`, and actor `u7` acted. A version moving `Member` to `Account` was refused twice: *"such a type does not move yet"*, and *"such a type takes no moved objects yet"*.

ADR-0229 refused an actor's type *"whose objects are the actors every event names"*, and left moving one until a case asked. What names an actor does not name its type:

- An event and an attempt record `actor_id` and `actor_kind` (`storage-schema.md`).
- A request finds its actor through each actor type the installed version declares, by its identity's index (`request.rs`, `resolve_actor`).
- A moved object keeps its id, and its row moves to the target's table (ADR-0229 decision 3).
- An actor's identity is held once across every actor type, by the generated invariant `identity_taken` (ADR-0194).

## Decision

1. **An actor's type moves as a rename.** One entry, without a `where`, into a type new in the version that no other type moves into. The target's identity attribute keeps the old one's name and `actor_kind`, and is `unique`. The move carries it and gives it no value.
2. **Each actor keeps its id, its identity and its kind.** So every event, attempt and proposal naming it reads as before, and a request naming its identity resolves to the moved object. Its state is kept unless the move's `states` maps it, and its other attributes are carried or given as in any move (ADR-0229 decision 4).
3. **Everything else stays refused, naming why:**
    - a split;
    - a merge, into a type the version before declared or with another type;
    - a renamed identity attribute;
    - a change of kind;
    - an identity that is not unique;
    - a given identity;
    - a move between an actor's type and another type.

    A split would decide by a condition which type an identity resolves to. A change of kind would change what every rule reading the kind decides.
4. **The publish needs nothing of its own.** Lifting the checker's refusal is the whole change, since the move already carries an object's id and row (ADR-0229 decision 3).

## Alternatives rejected

- **A new actor type beside the old.** An identity names one actor (ADR-0194), so an actor cannot be held by both, and retiring the old type leaves every actor retired.
- **Splits, merges and changes of kind now.** Each changes which type or kind an identity resolves to, which no case asks for.

## Consequences

- Built in the core's `check/migration.rs` and `scripts/check-flows.py`, `actor_move_errors` in each; `flow-format.md` §4.16 says so.
- **It refuses nothing 0.15.0 accepted.** A move from or to an actor's type was refused whole before.
- **Tested** on both backends: the issue's probe, approved by `u7`, whom the publish moves. Both actors kept their ids and states, `u7` wrote a note after the publish as before it, an archived actor stayed refused `actor_active`, an unknown identity stayed `actor_known`, and a later move changing the actors' kind was refused at its submit (`a_move_renames_an_actors_type_and_its_actors_act_as_before`). Both checkers agree on ten plants: the plain rename, and each refusal of decision 3. The Flow Studio's test passes through both runners alike. Two planted defects, the old refusal in each checker, were each caught.

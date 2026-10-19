# ADR-0220: A transition's guards judged personal by type

- **Status:** **Decided** 2026-10-19 under the author's direction ("address the two follow-ups in TODO"), the follow-up ADR-0219 decision 4 recorded.
- **Date:** 2026-10-19
- **Refines:** ADR-0219 decision 4 (a transition's guards judged by name); ADR-0206 decision 2 (`available`, `availability` and `check` refused `personal` by a guard or an invariant)
- **Relates to:** ADR-0206 decision 1 (what a personal member is); ADR-0203 decision 1 (a copy of an identity is the identity)

## Context

`available`, `availability` and `check` are refused `personal` without the opt-in where a guard the transition enforces, or an invariant of its type, reads a personal member (ADR-0206 decision 2). ADR-0219 judged a query's members by the types they belong to and left these three judged by name: every name personal in any type or transition. So a guard on a room's own `name` refused all three to every caller once another module marked a contact's `name` personal, as the room list was refused before issue #23; and a guard on `inputs.name` was personal wherever any transition's input `name` was, though its own transition writes it into a plain attribute. With the office flow of the test below, a room's `tidy`, guarded by its own `name`, and its `add`, guarded by its own input, were both personal, and are not.

ADR-0219 decision 4 said judging them by type is "a change to both backends". It is not: the reference runner has no guard on these three reads, as ADR-0206's consequences say, its one personal guard being a metric filter's (`flowrun.py`). That sentence was not checked when it was written, and ADR-0219 now says so.

## Decision

1. **A transition's guards and its type's invariants are judged as a query's filter is** (ADR-0219 decisions 1 to 3): each member as the type it belongs to declares it, by a bare name, a path through references or an aggregate's binding; a derived attribute personal where it reads a personal member it reaches; a member whose type the expression does not tell by its name in any type.
2. **An input, `inputs.<name>`, is judged by the transition asked about**: personal where that transition marks it `personal`, writes or passes it into a personal attribute, or declares it of type `identity` where an actor type marks its identity personal (ADR-0206 decision 1). An invariant reading `inputs` is judged by the same transition, the one whose request it would be evaluated in.
3. **The actor's members**, `actor.id`, `actor.kind` and `actor.principal`, are judged by name, as they were: no flow type declares them, so nothing tells their type.
4. **The guard is the core's alone**, as it was; the reference runner keeps none (ADR-0206).

## Alternatives rejected

- **Inputs judged by their name across the library**, as before: it fails safe, and it refused a room's `add` to every caller because a contact's `add` takes a personal `name`.
- **The actor's members judged never personal**: they are the request's own values, but a principal may be a person's identity, and judging them by name, as a query does, changes nothing a caller has met.

## Consequences

- `reads.rs`: `Personal` takes the transition whose inputs it judges, and `transition_reads_personal` judges guards and invariants with it, where it read `personal_members` by name.
- `wire.md` states the rule for the three reads beside the query's.
- Test: `a_transitions_guards_are_personal_as_their_own_types_declare` (a room's own member and its own input not personal; a contact's name through a reference, a derived attribute and an aggregate's binding personal; an input marked personal and one of type `identity`; an invariant by what it reads); four mutants killed: inputs judged by name, the identity input dropped, invariants not judged, and the old judgement whole.

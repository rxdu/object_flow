# ADR-0293: A follow's refused arithmetic, its missing limit and a machine's reference

- **Status:** **Decided** 2026-11-22, accepted in advance under the author's direction of 2026-09-28 as confident recommendations, the build of ADR-0292 having found each point open in its text. Each may still change before 0.32.0 is cut, and is fixed once it is (ADR-0180 decision 2).
- **Date:** 2026-11-22
- **Refines:** ADR-0292 decisions 1, 5 and 10; ADR-0277 decision 2
- **Relates to:** ADR-0185 (an effect's value an arithmetic left with none refuses); `docs/guides/errors.md`; issue #103

## Context

ADR-0292 lets a reference declare `follows`, each clause driving the objects holding the reference when the object it refers to takes a transition. Building it found three points its text leaves open, each answerable two ways.

- **A clause's input left with no value by a division by zero.** ADR-0277 decision 2 refuses a cascade's such input `value_in_range`, "naming the object whose transition cascades". For a cascade, that object is also the one the expression is read over. For a follow, the two differ. The expression is read over the follower (ADR-0292 decision 6), and the transition that triggers it is the referred-to object's.
- **A clause without `limit`.** ADR-0292 decision 2 requires one. A cascade clause without one is refused at step 2, `schema`.
- **A follow on an attribute a machine requires.** ADR-0292 decision 5 lets only a concrete type declare a follow. A machine declares the attributes it requires, and each type binding it declares them again (`flow-format.md` §4.10).

## Decision

1. **A clause's input a division by zero left with no value refuses the request as `value_in_range`, naming the follower**, the object its expression is read over, and nothing is driven or written. The clause is `value_in_range`, unqualified, as a cascade's is.
2. **A clause without `limit` is refused at step 2, `schema`**, as a cascade clause without one is.
3. **A machine's required attribute declares no follow**, since a machine is no concrete type. A type binding the machine may declare one on its own declaration of the attribute.

## Alternatives rejected

- **The referred-to object named.** Its declaration holds no part of the expression, so a requester reading it would find nothing to correct.
- **`<Type>.value_in_range`, qualified by the follower's type.** The clause is generated, not the follower's own guard, and a cascade's is unqualified.
- **A missing `limit` refused at step 3.** One rule would then be refused at two steps, by the kind of clause.
- **A follow on a machine's required attribute, inherited by every binder.** Each binder's transitions differ, so one clause could name a transition some binder lacks.

## Consequences

- `DESIGN.md`'s evaluation rules name a follow's inputs beside a cascade's, and `errors.md` says what `value_in_range` names.
- A test in the core and the reference runner holds decision 1 on both backends.

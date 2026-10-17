# ADR-0212: A dry run's class says what excluding it uncovers

- **Status:** **Decided** 2026-10-17 by the author ("go with your recommendation on the new E24 question"), the question and its recommendation being those `TODO.md` recorded the same day under the hiring review's E24.
- **Date:** 2026-10-17
- **Refines:** ADR-0211 decision 4; ADR-0147 (a dry run's dispositions)
- **Relates to:** `design/hiring-third-review.md` E24; `publish-and-import.md` §6; ADR-0154 decision 8

## Context

ADR-0211 decision 4 took a dry run that applies its exclusions to a fixed point as a change to make; the dry run already did so, and the decision was corrected the same day. What the third hiring review's E24 met is a round earlier. Its first dry run reported the on-hold applications as an undecided class, and nothing in it said that excluding them would leave the interviews and offers naming excluded objects: those classes appeared only in the dry run after the decision, and the importer took three rounds where it could have decided in one.

## Decision

1. **Each class a dry run reports that the disposition file does not exclude names what excluding it would uncover**, `uncovers`: each reference class its exclusion would add, by type and attribute, with the count of objects in it. The walk follows the batch's references to a fixed point, as if each class it uncovers were excluded in turn, so a chain, an application's interviews and an interview's notes, is reported whole in the first round.
2. **It is report data, as `count` and `sample` are.** A disposition file sending it back is taken and the value ignored; deciding a class still decides it alone, and what it uncovers is decided as its own classes.
3. **An invariant an exclusion would break is not in it.** A count or a uniqueness over what remains is found by the next dry run, as today; the walk reads references alone, and costs a pass over them, beside a dry run's placing of every object.
4. **Before 1.0.0.** It is additive, and could come later; it is built now, being small and the review's stated cost.

## Alternatives rejected

- **A per-object exclusion list** (E24's other ask): a second place to decide per object beside the mapping (ADR-0211).
- **Only the first step of the walk.** It leaves a chain to be found one round at a time, the cost E24 named.
- **Placing the batch again for each class**, which would find invariants too: a dry run's whole cost once per class.
- **Deferring it past 1.0.0**, which the recommendation allowed and the author did not take.

## Consequences

- `ViolationClass` gains `uncovers` in the core, and the service's import report writes it and its disposition file takes it back; the Studio's sandbox runs no dry run (ADR-0154 decision 8) and is unchanged.
- `publish-and-import.md` §6 and `wire.md` say what it is.

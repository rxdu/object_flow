# ADR-0266: A loop runs over a set or a range, never over a type

- **Status:** **Decided** 2026-11-07 under the author's direction ("review latest open issues and address them properly"), on the recommendation formed the same day for issue #73, accepted in advance as the author directed for a confident recommendation. Built the same day.
- **Date:** 2026-11-07
- **Refines:** `flow-format.md` §4.8 (a `foreach` step, and the types a call's target is known by)
- **Relates to:** `declaration-syntax.md` §5.2 (a loop's collection, and what a path is rooted at); `DESIGN.md` §5.4 (an outcome is straight-line); ADR-0071 (each loop's own bound); issue #73

## Context

Issue #73, filed 2026-11-07 at 0.18.0, reported that a `foreach` whose `array` is a type, every object of it, passes step 3, and that the core does not run it. An example reaching the loop is refused `example_not_run`. Without one, the version publishes, `availability` offers the transition, and every request of it is answered `501 not_run`. The issue asked for one of two fixes: run the loop, or refuse it at step 3 and say what a flow uses instead.

Reproduced 2026-11-07 on a fresh SQLite store at 0.19.0's commit `4d125be`. Its change was submitted `satisfied`, then published, and a request of the loop's transition was answered `501`: "not run: an expression whose value is a type". The reference runner, through the Flow Studio's sandbox, answered the same request `not_run` too: "a call whose target is not one object".

The grammar is `declaration-syntax.md` §5.2's, which `DESIGN.md` §5.4 says owns it. It roots a loop's collection, a path, at `this`, a relationship, a loop binder or an input, never at a type. `flow-format.md` §4.8 lists "a type scan" among what a call's target is known by, a loop's item over one. That sentence came with a change typing call targets at step 3 (`2dd604b`), not with a decision to run such a loop.

A rule's aggregate does scan a type, through the type's state index and the indexed attributes its `where` reads (check 7). A loop's `limit` bounds the elements its `where` selects, not the objects scanned, so a loop over a type reads its whole table in the writer's transaction.

## Decision

1. **A loop's `array` is not a type.** Both checkers refuse it at step 3, `names`: a loop runs over a set or a range, never over every object of a type. An enumeration is refused already, by check 24.
2. **`flow-format.md` §4.8 drops the type scan** from what a call's target is known by.
3. **A request that reaches every object of a type is the client's.** `available` pages the objects a transition is available to, and `batch` applies a request on each of them, all or none, each recording its own event. Within the flow, a loop runs over a relationship's set end or a set input.

## Alternatives rejected

- **Running the loop**, as the core runs a rule's aggregate. Its scan would read the whole table in the writer's transaction, since `limit` bounds only what is selected. It would add a construct the grammar never had, in the core and the runner both.
- **Refusing a reaching example alone**, as now. The engine requires no example, so a version passing every check is offered, and every request of the transition fails.

## Consequences

- **The core.** `check/names.rs`: decision 1, beside the loop's typing.
- **`check-flows.py`** holds the same, in the same words.
- **It refuses what 0.19.0 accepted**: a module looping over a type, which no request of that transition could run. An installed version holding one is served as before, its transition still answered `501`.
- `flow-format.md` §4.8 states decisions 1 and 2.
- **Tested.** `type_loop_plants` in `check-flows.py`, which both checkers agree on through the core's oracle: a loop over a type is refused, and one over a set input is clean.

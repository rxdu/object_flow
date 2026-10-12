# ADR-0171: A traversal invariant reads through any relationship end

- **Status:** Decided by the author, 2026-10-08 ("go with your recommendations on 1 and 2"), on a recommendation stated with the question (`TODO.md`).
- **Date:** 2026-10-08
- **Refined by:** ADR-0192 — a written object's related objects found through the end it stores or the indexed column of the other.
- **Refines:** ADR-0045 (decision 3), ADR-0055 (decision 1, the traversal form)
- **Relates to:** ADR-0056, ADR-0059 (one end stored, the inverse a view over it); D453 (`ServiceJob.engineer_active`); `DESIGN.md` §5.6; `design/declaration-syntax.md` §3.1, §3.4, §10 check 4; `design/core-checker.md` §2

## Context

ADR-0045 decision 3 let an invariant traverse only relationships with declared inverses, "so the affected set is computable by reverse traversal from any written object", and check 4 refuses a traversal invariant "reading through a relationship end whose far end is not declared, so a write to the far object cannot find the invariant to re-check". Neither checker enforced it.

Building it on 2026-10-08 refused the design's own invariant. `unit-journey.md`'s `ServiceJob.engineer_active`, `not open or engineer.state == User.ACTIVE`, reads through `ServiceJob.engineer`, a reference to `User`, which `operations_shared` declares. D453 put the rule there on purpose (2026-09-28): production refuses to remove a user with open jobs, and a guard on `User.leave` reading the journey's jobs would make `operations_shared` import the journey back. Declaring the far end would do the same, since both ends of a relationship are declared in one module.

The reason check 4 gives predates ADR-0056 and ADR-0059. Since then one end of every relationship stores the value and the declared inverse is a derived view over it, and "a stored end is always available to a filter" (`declaration-syntax.md` §3.1, §11 question 5). A write to a `User` therefore finds the jobs naming it through their stored `engineer`, whether or not `User` declares the far end, as it finds the jobs a declared inverse would list. Both backends enforce the invariant today: the example `an_engineer_with_an_open_job_does_not_leave` is refused through both, the core's re-check reaching any object an invariant of this form could read (`request.rs`, `invariant_reach`).

## Decision

1. **Check 4 is withdrawn, and a traversal invariant may read through any relationship end.** Its affected set stays computable: a write to an object reaches every object whose invariant reads through a relationship to it, found by the relationship's stored end, whichever side stores it. ADR-0045 decision 3's restriction to declared inverses is withdrawn with it. The three forms of ADR-0055 stand, a traversal still the form that reads related objects.

2. **The documents say so.** `DESIGN.md` §5.6's traversal form, `declaration-syntax.md` §3.4 and §10 check 4, `core-checker.md` §2, and the status of ADR-0045 and ADR-0055 are changed to this, and the first consumer's walkthrough, a record of its time, is annotated where it required declared inverses.

## Alternatives rejected

- **Check 4 enforced as written.** It refuses D453's invariant, and the rule has no other statement across modules: a guard on `User.leave` or a declared far end both make the shared module import the journey.
- **The far end required only where both types are in one module.** The rule would hold within a module and lapse across one, where D453 needs it, and the reason it gives would be false in both.

## Consequences

- No checker change: neither enforced check 4. `TODO.md`'s question of check 4 is decided, and its inventory of checks neither checker enforces keeps check 6 and the typing parts of checks 2, 24 and 47.
- How cheaply a write finds the invariants reading through it, by an indexed lookup on the stored end rather than the whole type, is the runtime's to optimise, and the scale item in `TODO.md` records the core's conservative re-check.

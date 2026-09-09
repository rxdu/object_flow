# ADR-0044: A proposal executes under the current declaration and is invalidated when that becomes impossible

- **Status:** Accepted — repair of D08, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refined by:** ADR-0078 — an erasure invalidates a pending proposal whose inputs carried a value it removed, a fourth cause.
- **Refines:** ADR-0036

## Context

ADR-0036 re-evaluates a proposal's non-actor guards at execution but never says under which declaration version. Defect D08 found the consequences unhandled. A version that adds a required input makes the stored request unexecutable; ADR-0036's lifecycle offers no terminal state for that, and routes failure back to `pending`, so it pends forever. ADR-0027 says removing a transition affects no object, but it strands any proposal naming it. A declaration migration can move the target object out of the proposal's from-state.

## Decision

1. **Execution uses the declaration version in force at execution time**, not at submission. A proposal is a request waiting for authority, and the rules that apply to a request are the rules that apply now. The submission version is recorded for history.
2. **The lifecycle gains a terminal state `invalidated`**: `pending → executed | rejected | withdrawn | invalidated`. `expired` is not a state; see rule 5.
3. **A proposal is invalidated, not left pending, when the current declaration cannot express it**: its transition no longer exists, it now requires an input the proposal does not carry, or its target state is no longer reachable from where the object now stands. The invalidating event records which of these it was. *(ADR-0078 adds a fourth cause: its inputs carried a personal value an erasure removed.)*
4. **A guard failure still leaves it `pending`**, with the verdict attached. The distinction is between "not yet" and "not any more": a guard that fails today may pass tomorrow, whereas a removed transition will not return.
5. **`expired` is derived, not driven** (ADR-0043): a proposal past its expiry is reported as expired by the read surface, and nothing has to move it there.

## Alternatives rejected

### Execute under the submission version

Rejected: it would let a proposal carry an obsolete rule set past a deliberate declaration change, which is the drift the whole project exists to remove. A proposal is not a contract about the rules.

### Leave unexecutable proposals pending

The prior state. Rejected: an unbounded set of permanently pending requests that no actor can resolve is an operational leak, and it hides a real signal that a declaration change broke outstanding work.

### Migrate proposals alongside objects at publish

Attractive, since ADR-0027 already migrates objects. Rejected as disproportionate: a proposal is a short-lived request, and inventing a mapping language for stored inputs to satisfy a new declaration is a large mechanism for a small case. Invalidating and telling the proposer is honest and cheap.

## Consequences

- Publishing a declaration should report how many pending proposals it invalidates, alongside the object migration report (ADR-0027).
- D08 is resolved.

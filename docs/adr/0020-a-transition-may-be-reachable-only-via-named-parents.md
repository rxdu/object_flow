# ADR-0020: A transition may be reachable only via named parent transitions

- **Status:** Accepted — taken in autonomous design iteration 1 (2026-09-07); pending author review
- **Date:** 2026-09-07

## Context

Some transitions must never be requested directly. A unit becomes `SOLD` only because a Delivery completed or a Lease converted; a unit becomes `RESERVED` only by being bound to a slot. If `unit.sell` were requestable, a unit could be sold with no delivery — the backdoor the first consumer's ADR-0002 guarded against by hand for `DEVELOPMENT → SOLD` ("an unguarded transition turning any company asset into a sale with no commercial evidence"). A guard on the parent's state cannot express this, because at the moment of the cascade the parent's new state is not yet visible. Walkthrough §4 C.

## Decision

A transition may declare `only via` a list of parent transitions on other types. Such a transition:

- is **not requestable by callers** and does not appear in any caller's availability list;
- may occur **only as a cascaded outcome** (ADR-0019) of one of the named parents;
- carries **no actor guards of its own** — the parent's guards are its authority;
- keeps its other guards, which are evaluated in the cascade like any other.

A direct request for it is refused with remedy class `unreachable-from-here`, naming the parent transitions that lead to it.

## Alternatives rejected

### Convention

Leave `sell` requestable and trust callers not to call it. Rejected: the mediated property is the whole point, and an agent will call whatever is listed.

### A guard on the parent's proposed state

`sell` guarded on `binding.delivery.state == DELIVERED`. Rejected: within the cascade the Delivery is still `PREPARATION` until commit, so the guard would need to see proposed state, which makes guard semantics depend on evaluation order.

## Consequences

- The readable rule set for a type states, per state, "entered only via …" — an authority statement, inspectable at runtime (ADR-0010).
- The reachability test the first consumer had to write (`wr:tests/unit/test_transition_reachability.py`) is subsumed: a transition is either requestable, or only-via named parents, and the declaration says which.
- Only-via is a property of the transition, not of the state: a state may be entered by one requestable transition and one only-via transition (`AVAILABLE` by `inventorize` and by `release`).

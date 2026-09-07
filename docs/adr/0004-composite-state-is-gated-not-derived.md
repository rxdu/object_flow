# ADR-0004: Composite state is gated by its parts, not derived from them

- **Status:** Accepted
- **Date:** 2026-09-07
- **Open challenge (2026-09-07):** first-consumer evidence bears on this ADR on two points — derived read-only views (slot state, computed and not stored) and atomic outcomes on *referenced* objects rather than parts. See TODO.md, "Challenges from the first consumer", items 1 and 3. The decision stands until those are resolved.

## Context

Once composites exist, a composite's state could either be computed from its parts' states, or be its own state with guards that read the parts.

## Decision

Composites have their own states and their own transitions. Guards on those transitions read part state. A composite observes and constrains its parts; it does not contain their behaviour.

## Alternatives rejected

### Derived state

`Deployment.state = f(states of its Tasks)` — all tasks done implies the deployment is done. Always consistent, no manual step.

Rejected on two grounds. First, the composite loses all agency: it cannot be held in a state for a reason external to its parts, such as the customer not yet having been on site. Second, and more seriously, a derived state change has no actor, no decision timestamp and no provenance — it happens because a counter reached zero. That defeats the invariant the whole design rests on, that every state change is a transition some actor performed and that passed a guard. Derived state is a second, unchecked path to changing state.

## Consequences

- "Why is this stuck?" gets a useful answer: `accepted` is blocked with `3 of 7 Tasks not done`, remedy class *dependent*.
- Cascade termination needs no special machinery. "A Deployment cannot close with open Tasks" is just a guard on the transition into the terminal state; a cascade-close is a transition that proposes terminal transitions on each part, each gated normally.

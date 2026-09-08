# ADR-0042: There are no free attributes; every write is a transition, and ordinary editing is an action

- **Status:** Accepted — repair of D09, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Supersedes:** ADR-0006

## Context

ADR-0006 split attributes into **controlled**, written only through transitions, and **free**, "editable by any permitted actor, recorded but not gated". It was never confirmed, and said of itself: revisit before it becomes load-bearing. It became load-bearing in seven later ADRs and in the model block.

Defect D09 found that the free half has no design at all. No operation writes one: the read surface has no write, and transition execution is defined for transitions only. "Permitted" is undefined, because permissions are guards and guards attach to transitions, so "any permitted actor" reduces to any actor. No provenance class fits, since the closed set's `observed` means a transition produced it. And the event shape assumes a transition, so a free write has no transition name to filter on and an undefined `changes_state`.

ADR-0006 rejected making every write an operation as "too strict in the general case — correcting a typo in a description should not require inventing an operation." That reasoning predates ADR-0016. Once an action is just a transition whose from-state equals its to-state, the cost of an operation is one declaration, not a bespoke mechanism.

## Decision

**Every attribute is controlled. The free class is removed.** Every write to an object goes through a declared transition, and ordinary editing is an action.

A type that wants broad editing declares one action for it:

```text
edit(description, notes): any non-terminal → same state
  guards:  actor.has(EDIT_X)
  outcome: description := inputs.description
           notes       := inputs.notes
```

That is one declaration per type, not one per attribute. It costs a line and buys the whole machinery: a named operation, a guard saying who may do it, a recorded event with an actor and a reason for `changed_since` to see, a subscription filter that can name it, and an availability listing that shows it.

The Mediated property becomes literally true: no path to the data except through declared transitions. The qualifier "every other write is recorded with provenance" is removed, because there is no other write.

## Alternatives rejected

### Define an `update` operation for free attributes

Give free writes their own request shape, permission model, provenance class and event kind. Rejected: it is a second write path with its own authority model, parallel to the one that exists, and every consumer and every subscriber would have to handle both. The design's own escape-hatch discipline (ADR-0008) says an extension must return the same shape as the built-ins; a second write path fails that test at the top level.

### Keep free attributes and gate them by a type-level permission

Rejected: a type-level permission cannot express "the owner may edit this, others may not", which is the ordinary case in every system studied. A guard can, and a guard needs a transition to attach to.

### Keep ADR-0006 unchanged and leave the write path unspecified

Rejected: it is the largest hole in the design and it silently contradicts the headline property.

## Consequences

- ADR-0006 is superseded. The controlled/free split disappears from the model block, and "any attribute a guard reads must be controlled" becomes vacuous and is removed.
- ADR-0016's rationale strengthens: actions are now the mechanism for *all* non-lifecycle writes, which is what makes removing the free class affordable.
- Type authors must declare an edit action for attributes they want editable. A type that declares none is read-only after creation, which is now visible in its printed rule set rather than implied.
- Every write appears in the log, so `changed_since` and subscription filters see all of them. Edit volume enters the permanent history; that is the honest cost of the recorded property, and the alternative was writes nobody could audit.
- One of the three Proposed ADRs is resolved by supersession rather than confirmation (D34).
- D09 is resolved.

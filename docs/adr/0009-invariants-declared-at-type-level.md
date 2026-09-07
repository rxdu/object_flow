# ADR-0009: Invariants are declared at type level, enforced at transitions

- **Status:** Accepted
- **Date:** 2026-09-07

## Context

Guards attach to transitions. Some rules have no transition to attach to — they are properties of the world that must hold continuously. Illustratively: a Site may never have two active Deployments; a Robot may not be assigned to a Deployment while in maintenance.

## Decision

Invariants are a fifth declarative element alongside attributes, states, transitions and guards. They are declared once against the type, as a property. The runtime determines which transitions could violate them and enforces there.

Enforcement still happens only at transitions, since transitions are the only thing that changes state. The difference is in declaration, not in mechanism.

## Alternatives rejected

### Express invariants as per-transition guards

Attach the condition to every transition that could violate it.

Rejected for two reasons. It is duplication, so the copies drift. More importantly, an invariant spanning objects can be violated from either side — by deploying to a busy site, or by reactivating a site that already has a deployment — and hand-written guards will reliably cover one and miss the other. That is the class of defect that is invisible in review, because each transition looks correct on its own.

## Consequences

- This is the same relationship a database `UNIQUE` constraint has to insert paths: the property is stated, not its enforcement points.
- Requires the runtime to analyse which transitions can affect an invariant, which is a real piece of machinery rather than a notation convenience.
- If it turns out that everything worth enforcing attaches naturally to a specific transition, this concept should be dropped rather than carried.

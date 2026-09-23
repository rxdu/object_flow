# ADR-0009: Invariants are declared at type level, enforced at transitions

- **Status:** Accepted
- **Date:** 2026-09-07
- **Refined by:** ADR-0045 — enforcement is dynamic, the static analysis is a publish-time report, and invariants may only traverse declared inverses.

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
- ~~Requires the runtime to analyse which transitions can affect an invariant.~~ **Superseded by ADR-0045**: enforcement is dynamic, the static analysis is demoted to a publish-time report, and invariants may traverse only relationships with declared inverses so the affected set is computable.
- If it turns out that everything worth enforcing attaches naturally to a specific transition, this concept should be dropped rather than carried.

## Evidence from the first consumer

Two type-level invariants exist in the inventory system, so the "drop it if nothing needs it" consequence does not apply.

- **One open engagement per unit**, enforced by a partial unique index (`WHERE actual_end IS NULL`) on the engagement-lines table (inventory ADR-0002). The stated reason is the one this ADR gives: a service-layer check leaves a race that the database does not, and "in two places at once" is a fact the store should refuse rather than a rule each transition remembers to check.
- **Exactly one active version per configuration SKU**, enforced in the same transaction as activation by superseding every other live version (inventory `docs/design/business-processes.md`, Configuration Lifecycle). Three separate activation paths all had to honour it — the duplication this ADR predicts when an invariant is written as per-transition guards.

The domain question in TODO.md is answered accordingly.

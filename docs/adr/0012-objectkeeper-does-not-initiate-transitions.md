# ADR-0012: ObjectKeeper does not initiate transitions

- **Status:** Accepted
- **Date:** 2026-09-07
- **Refined by:** ADR-0019 — auto-fill on receipt is a declared cascade of the batch commit the caller requested, not a trigger; ADR-0022 — time-driven transitions are ordinary guarded transitions that a scheduler above requests, using the availability query. The decision here is unchanged.

## Context

The originating definition of the project said "there are triggers and constraints that **drive** the lifecycle of the object from starting to end of life." Constraints were resolved as guards (ADR-0002, ADR-0005, ADR-0009); triggers were left open, and the word "drive" implied ObjectKeeper might advance objects on its own.

It does not. Deciding *when* something should happen belongs to the layer above — applications, agents, or a workflow automation platform.

## Decision

ObjectKeeper does three things: **check, transition, record.** It never initiates a transition, and it holds no schedule, timer or reactive rule.

Every state change happens because a caller asked for it. There is no path by which an object moves without an external actor requesting the move.

## Alternatives rejected

### Triggers inside the model

Declarative rules that react to events and perform transitions, in the manner of Jira post-functions or Salesforce workflow rules.

Rejected because a trigger that performs a transition creates cascades, evaluation-order dependence, and the loss of a single explanation for why an object is in the state it is in. It also makes the store an actor with its own agency, which contradicts the property that every change is attributable to someone who asked for it.

### Triggers that propose transitions

A softer version, considered and set aside: a rule proposes a transition that is then gated normally, so automation becomes just another actor and nothing bypasses the guards. This preserved the safety property, but still required ObjectKeeper to decide *when* to propose, which is the responsibility being placed above it.

### Time-based rules with a scheduler

Rules of the form "escalate if not acknowledged within four hours". Attractive because *absence* is what usually goes wrong operationally — nobody acknowledged, nobody signed off, the object has sat in one state for weeks — and event-driven reaction cannot see it.

Rejected as a consequence of the decision above, but at a real cost, recorded under Consequences.

## Consequences

- **The core is library-shaped.** No scheduler, no durable timers, no recovery-after-downtime semantics for the core. It embeds in a process or is wrapped in a service, per deployment.
- **Deterministic and testable without infrastructure.** A call goes in, guards evaluate, a transition and a record come out. No background work means no ordering surprises and no unexplained overnight state changes.
- **The read path becomes load-bearing.** Detecting that nothing happened is still real operational work; it moves upstairs, and the upper layer needs queries such as "which cases are in `awaiting_customer` and have not moved in four hours". That capability is now required, not optional. *Provided by ADR-0022's availability query and ADR-0037's read surface.*
- ~~**ADR-0010's property acquires an exception.**~~ *Superseded by ADR-0022: the timing rule is the guard itself (`end_date <= now`), declared on the type and inspectable; only the act of asking moves upstairs. No exception to ADR-0010 remains.*
- ADR-0004's "gates rather than propels" phrasing, and ADR-0011's rejection of names implying lifecycle guidance, both stand as written.

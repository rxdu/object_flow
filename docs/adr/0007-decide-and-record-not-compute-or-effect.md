# ADR-0007: The store decides and records; it does not compute or cause effects

- **Status:** Accepted
- **Date:** 2026-09-07
- **Refined by:** ADR-0019 — a transition's outcome may cascade transitions on related objects inside the store; that is not an *effect*, which remains something caused outside it. ADR-0021 — derived attributes are named expressions, not the stored computation this ADR excludes. ADR-0032 — the store evaluates declared arithmetic over its own data (a stock level, an order total); what stays outside is the *domain formula* — the tax rate in the example below, pricing, scoring, conversion.; ADR-0068 — money keeps its currency in the declaration; ADR-0069 — an evaluator returns a verdict; an external system that assigns is a mirror; ADR-0081 — any declared formula over the store's own data is in scope; outside data, selection, effects and orchestration stay out.

## Context

"Business logic" is the part of a system that is famously not generic, and the project requires genericity from the start. The question is which parts of business logic the declarative model can carry, and where the rest goes.

The state machine is a structure; the business logic lives in the guards on its transitions. So the real variable is what the guard language must express.

## Decision

The store answers *may this happen*, enforces it, and records that it did. It does not calculate values and it does not cause things to happen in other systems. Those belong to the consuming application.

Four categories are explicitly out of scope, because none of them is a guard:

| Category | Example | Why it is not a guard |
|---|---|---|
| Computation | `total = Σ line items × tax rate` | Produces a value; a guard consumes one |
| Effects | On acceptance, raise a draft invoice in Xero | Happens after, and reaches outside |
| Selection | Route this case to the nearest engineer with capacity | Decides a value, not permission |
| Orchestration | Multi-step processes spanning days and objects | A sibling concern (Temporal-shaped) |

## Rationale

The split is not arbitrary, and it is why the state machine generalises at all. Preconditions and permissions have the same shape across every domain — "required field", "needs approval", "no open children" look alike whether the object is a deployment or a loan application. Computation and effects are exactly where domains differ irreducibly; there is no generic form of "how tax is calculated". The model generalises because it sits at the layer where domains resemble each other, and the residue resists generalisation because it sits where they do not.

## Alternatives rejected

### Include computed attributes and effect execution

Would make the store a more complete application platform.

Rejected because it imports precisely the domain-specific parts, which either forces code into the model (see ADR-0008) or produces an expression language that grows without limit. It also blurs the boundary with workflow orchestration, which is a solved problem elsewhere.

## Relationship to event delivery

Push delivery of recorded events (ADR-0013) is not an exception to this ADR. Delivering a record is not making a decision: the subscriber registered its interest in advance, and the payload says *this happened*, not *do this*. What remains out of scope is ObjectKeeper deciding an outcome and acting on it — the Xero example above stays excluded, because raising an invoice is an effect ObjectKeeper would be choosing to cause.

## Consequences

- Consuming applications compute values and then supply them as transition inputs, where guards can check them.
- Effects are triggered by consumers observing recorded events, not by the store reaching outward.

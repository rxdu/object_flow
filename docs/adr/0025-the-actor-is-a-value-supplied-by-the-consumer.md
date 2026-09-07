# ADR-0025: The actor is a value supplied by the consumer; ObjectKeeper does not authenticate

- **Status:** Accepted — taken in autonomous design iteration 1 (2026-09-07); pending author review
- **Date:** 2026-09-07

## Context

Guards reference actors and roles, DESIGN.md listed permissions and approvals as in scope, and the model block had no actor. The first consumer's shape is concrete: a principal is a human (JWT) or an agent (an API key bound to a user of role `AGENT`); effective permissions are role ∪ per-key allowlist; four roles and 41 permissions named `<RESOURCE>_<ACTION>`; a transition may require a role; an agent may create a Service but may not be its engineer-of-record; revoking a key is instant (`wr:docs/DESIGN.md` §7.1, `wr:docs/agent-operations.md`). Walkthrough §4 H.

## Decision

Every request carries an **actor descriptor** that the consumer's authentication produces and ObjectKeeper accepts as given:

| Field | Meaning |
|---|---|
| `id` | a stable identifier for the acting party |
| `kind` | `human`, `agent`, or `service` |
| `principal` | optional: the party on whose behalf an agent or service acts |
| `capabilities` | a set of opaque strings the consumer defines |
| `attributes` | optional further values the consumer's guards may reference |

- ObjectKeeper validates the descriptor's shape, not its truth. Authentication, key management, roles and how capabilities are computed belong to the consumer.
- Guards reference it through `actor.*` (ADR-0021): `actor.has(DELIVERY_COMPLETE)`, `actor.kind == human`, `actor.id == owner`.
- Where a guard needs a referenced person — a Service's engineer must be a human user — the person is an ordinary object in the store, and the guard reads its attributes.
- Every recorded event carries the actor, and the principal when present. Consumers can then answer "what did agents change" the way the first consumer does with `source = agent`.
- Delegation that attenuates authority, and proposals for a denied transition, are **deferred**: the first consumer's list of transitions where authority differs from capability has two entries.

## Alternatives rejected

### ObjectKeeper manages users, roles and keys

Rejected: every consumer already has authentication (the first consumer has JWTs and API keys), and the store would then own a second identity system to keep in sync.

### Roles only

Rejected: the first consumer moved from roles to fine-grained permissions precisely because roles could not express per-key agent scopes. Opaque capabilities cover both.

### Actor as an object in the store, always

Rejected as the general rule: a service account or a webhook has no reason to be an object. It is allowed as a specific case whenever a guard needs the person's attributes.

## Consequences

- The model block gains the actor descriptor. DESIGN.md's in-scope "permissions" means guards over `actor.*`.
- Provenance on every event includes actor and principal.
- Approval was defined in ADR-0035 in terms of this descriptor; delegation, attenuation and proposals in ADR-0036.

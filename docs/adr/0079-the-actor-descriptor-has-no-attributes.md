# ADR-0079: The actor descriptor has no free-form attributes; attenuation is a capability or an object

- **Status:** Accepted — decided 2026-09-09 at the author's direction ("rule on D193"), with the recommendation and its evidence below; pending author review
- **Date:** 2026-09-09
- **Refines:** ADR-0025, ADR-0030, ADR-0036

## Context

ADR-0025 gave the descriptor optional `attributes`, "further values the consumer's guards may reference". The language then made them unreadable — a guard reads `.id`, `.kind`, `.principal` and `.has(...)` and nothing else, because an untyped value cannot be checked at publish. ADR-0036 §5 nonetheless rested attenuation on them (`amount <= actor.attributes.approval_limit`), the approvals case study repeated it, ADR-0030's example read `actor.teams`, and `library-api.md`'s `Actor` never had the field. D193: a decision with no spelling, in three documents, three ways.

Evidence from the first consumer. Its authority model is a role plus a per-key permission allowlist (`wr:app/models/api_key.py:56`, `permissions = Column(JSON)`); `wr:app/core/permissions.py` and the key model contain no limit, ceiling or attenuation of any kind. The two transitions where authority differs from capability (ADR-0025) are both capability-shaped. Nothing in the system reads a valued fact about the caller.

## Decision

The descriptor is `id`, `kind`, optional `principal`, and `capabilities`. **It has no attributes.** The field is removed rather than kept unreadable: it was unvalidated, unrecorded (the log stores id, kind and principal), and an invitation to exactly the reading ADR-0036 made.

A fact about an actor is expressed one of two ways:

- **A small finite fact is a capability.** Team, tier, role: the consumer's authentication emits `TEAM_SALES` as it emits `DELIVERY_EDIT`, and a guard or visibility predicate reads `actor.has(TEAM_SALES)`. Capabilities are opaque strings the consumer defines (ADR-0025); nothing new is needed.
- **A fact with a value is an object the guard reads.** An approval limit, a delegation window, a quota: a `Delegation { delegate, capability, limit, from, until }` with a lifecycle, and a guard `any(d in Delegation where d.delegate == actor.id and d.state == Delegation.ACTIVE and d.limit >= amount)`. This is the pattern ADR-0036 §6 already gave for governed delegation, and it is now the only one. It is recorded, revocable by a transition, and visible in history, which a number in a descriptor never was.

Delegation still arrives in the descriptor as capabilities with `principal` set to the delegator (ADR-0036 §5's first half stands). Only attenuation-by-attribute is withdrawn.

## Alternatives rejected

- **Typed actor attributes declared in the module** — `actor attr approval_limit money(SGD)` — so a guard over one could be checked at publish. Rejected: a second declaration form and a second place types live, for a value the store cannot verify and does not record, so a limit that changed would leave no history; and no consumer needs it. ADR-0072 refused per-attribute confidentiality on the same ground.
- **Keep `attributes` as unreadable metadata for the consumer's own use.** Rejected: dead weight with a misleading name. A consumer that wants to attach data to a request has `context`.

## Consequences

- `DESIGN.md` §5.8 and the Actor row of §14 drop the attributes and say how a fact about an actor is expressed.
- ADR-0025's table row is struck in place; ADR-0036 §5 and ADR-0030's example are annotated.
- `case-study-approvals-and-bookings.md` §3 writes the attenuation guard over a `Delegation` object.
- `declaration-syntax.md` §8.1 says the descriptor has no other members.
- `library-api.md`'s `Actor` already matches and is unchanged.
- D193 is resolved.

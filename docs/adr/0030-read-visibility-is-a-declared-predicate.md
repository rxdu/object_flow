# ADR-0030: Read visibility is a declared predicate over actor and object; an invisible object is not found

- **Status:** Accepted — taken in autonomous design iteration 3 (2026-09-08); pending author review
- **Date:** 2026-09-08
- **Refined by:** ADR-0072 — six smaller answers to the open questions; §4 keeps confidentiality per object and records the cost of the workaround; ADR-0079 — the `actor.teams` example in decision 1 read a descriptor attribute, which no longer exists. ADR-0095 — a metric guard reads every row as a type-scan guard does, and a reader is shown an event's read set and recorded values only as far as their visibility reaches.

## Context

Guards decide who may change an object. Nothing decided who may read one. Two case studies needed it: issue-level security in a tracker, and owner- or team-scoped records in a CRM. The first consumer gates reads by per-resource `_VIEW` permissions, which is a type-level answer and not a per-object one. Case study: `docs/design/case-study-crm.md` §4.

## Decision

1. A type may declare a **visibility predicate** in the expression language (ADR-0021) over `actor.*` and the object: `actor.has(CONTACT_VIEW_ALL) or owner.id == actor.id or owner.team in actor.teams`. *(The last clause reads a descriptor attribute; ADR-0079 removed those. Team membership is a capability the consumer's authentication emits, `actor.has(TEAM_SALES)`, or a `Membership` object the predicate reads.)*
2. The read surface applies it to **every** read by id, every query result, every family query, every availability result (ADR-0022), and every event delivered to a pull consumer or push subscriber acting as that actor.
3. An object the actor cannot see is **not found**. A transition request on it is refused as not found, before guards run, so the verdict does not reveal that the object exists.
4. Visibility is evaluated under the same snapshot as the request; a cascaded transition (ADR-0019) is not subject to visibility — the parent's guards are its authority (as with only-via, ADR-0020).
5. Absence of a predicate means visible to any actor the consumer admits.

## Alternatives rejected

### Leave reads to the consumer

Rejected: the availability query, the event feed and family queries all return objects; a consumer filtering afterwards would leak through every surface it forgot, which is the five-copies problem again (ADR-0010).

### Blocked rather than not found

Report an invisible object as existing but blocked. Rejected: existence is information; a tracker's security level is meant to hide the issue, not its actions.

## Consequences

- The read surface has one more required input: the actor.
- Event delivery is per-subscriber-actor when a type declares visibility; a subscriber that must see everything is given an actor that can.
- Type-scan guards (ADR-0021) evaluate over all objects regardless of the requesting actor's visibility; a guard is a rule of the type, not a view.

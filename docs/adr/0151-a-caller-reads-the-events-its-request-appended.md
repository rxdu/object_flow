# ADR-0151: A caller reads the events its request appended, and an invariant failure names its object's type

- **Status:** Accepted, 2026-10-04, by the author ("go with your recommendations on both findings"), deciding the two findings of ADR-0150's Consequences as recommended.
- **Date:** 2026-10-04
- **Refines:** ADR-0150 (its two findings against the interface), ADR-0122 (what a refusal names)
- **Relates to:** `design/library-api.md` §4, §6; `DESIGN.md` §6, §10; `design/flow-studio.md` §6

## Context

The Flow Studio's first slice plays every example through the engine's wire, using only what a caller is given (ADR-0150). Two things the examples check could not be learned that way.

1. **What a request did beyond its own object.** `Satisfied.events` gives the positions of every event the request appended, the parent's first, and `Event.cause` links each to the one that caused it. No operation read an event by its position: `history` reads by object and `pull` through a subscription. So a caller was handed positions it could not resolve, and could not learn which objects its request created or which transitions it caused. 42 of the 163 examples name the one or expect the other.
2. **The type of the object an invariant fails on.** `InvariantFailure` named its object by id alone. When that object is one the refused request would have created, nothing of it was committed, so no read finds it and the caller cannot tell its type. The type is what an expectation's clause is qualified by, and what a message to a person names. One example meets it.

## Decision

1. **A twentieth operation, `events(actor, positions)`, reads the events at the positions given.** It returns one entry per position, in the order given. Each entry is the event the log holds at that position, or nothing where it holds none, as `get` returns nothing for an id no object has. Like every read, it is filtered by no actor (ADR-0114), and an event's erased values are redacted as `history` redacts them. It is how a caller resolves any position the store hands it, a verdict's `events` and an event's `cause` among them. The objects a request created are the objects of its creation events after the first; the transitions it caused are its events after the first, a `redact` event left out (`flow-format.md` §11.4).
2. **`InvariantFailure` carries the type of its object**, beside the object's id, so a caller knows what failed even where no read can find the object.

## Alternatives rejected

- **For decision 1:**
  - `Satisfied` naming the objects its events are on, which answers what a request created and leaves every `cause` unresolvable;
  - the caller subscribing and pulling, which needs no change, but makes every application keep a subscription to learn what its own request did;
  - refusing a position the log does not hold as a fault, which would make one stale position fail a read of many, where `get` answers the same question about an id with nothing.
- **For decision 2:**
  - leaving the id alone and having the caller infer the type from the clause, a guess wherever two types declare invariants of one name;
  - not naming an object the refused request would have created, which would hide which object broke the rule.

## Consequences

- `library-api.md` §4 and §6 give the shapes, and `DESIGN.md` §10 lists the operation and §6 the type a failure names.
- The studio's server offers `events` over the reference runner, and both of its players resolve what a request created and caused through it, so the 43 examples that waited on these findings are replayed. The two that use an import step still wait for importing, a later slice of the studio.

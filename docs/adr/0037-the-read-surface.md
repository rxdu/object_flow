# ADR-0037: The read surface — get, query, availability, check, history, declaration, pull, batch; declared indexes and external identifiers; no hidden projections

- **Status:** Accepted — taken in autonomous design iteration 6 (2026-09-08); pending author review
- **Date:** 2026-09-08

## Context

The read surface was the last undesigned part of the model and had become load-bearing: ADR-0012 moved staleness detection above the store, ADR-0022 requires an availability query, ADR-0026 needs family queries, ADR-0028 needs supersession following, ADR-0030 applies visibility to every read, ADR-0034 defines the pull cursor, and the booking case needs a check without execution. Two open questions belonged here too: whether type-scan guards get indexes or maintained projections, and whether external identifiers are marked in the declaration. Requirements were gathered across all five case studies and are listed in TODO.md.

## Decision

The read surface is one API with these operations. Every operation takes an actor (ADR-0025) and applies visibility (ADR-0030); every returned object carries its id, type, declaration version, state, `version`, stored and derived attributes.

| Operation | Returns |
|---|---|
| **get**(id, follow?) | the object; if superseded and `follow`, the live successor with the chain (ADR-0028); deleted and superseded objects are returned by id, never by query |
| **query**(type or family, filter, order, cursor, fields) | a page of objects; `filter` is an expression over the object (ADR-0021, ADR-0032) restricted to indexed attributes, state and category; `order` over indexed attributes; `fields` selects attributes to reduce payload; a type's declared `summary` set is the default |
| **lookup**(source, value) | the object whose declared external identifier for `source` equals `value` |
| **availability**(id) | every *requestable* transition of the object's type, excluding only-via ones per ADR-0020 and ADR-0041, with its three-way availability for this actor and, for unavailable ones, the verdict with remedy class and, for `available-with-input`, the derived parameter schema (ADR-0005) |
| **available**(type or family, transition, cursor) | the objects for which that transition is available or available-with-input now (ADR-0022); deferred external guards are not evaluated and the result says so |
| **check**(id, transition, inputs) | the verdict a request would receive, without executing: `validate(object, intent)`; evaluated on a snapshot without locks, so it is advice, not a reservation |
| **history**(id, follow?) | the object's events with provenance (ADR-0033), legacy entries (ADR-0015), and, with `follow`, the combined timeline through `supersedes` |
| **declaration**(type, version?) | the inspectable declaration (ADR-0010): attributes, relationships, invariants, derived attributes, the bound machine, every transition with its guards and derived parameter schema, only-via and proposable flags; also rendered as the readable rule set and as agent tool schemas |
| **pull**(subscription, cursor) | a page of events per the subscription's filter (ADR-0034) |
| **batch**(requests) | N independent requests each in its own transaction, N verdicts in order; there is no atomic batch — atomic multi-object semantics are declared cascades (ADR-0019) |

**Indexes, not hidden projections.** A type declares which attributes are indexed. Type-scan predicates in guards, invariants and visibility, and query filters, run as queries over indexed attributes; publishing a declaration that scans an unindexed attribute produces a warning in the publish report (ADR-0027). The store maintains no projection it does not declare. Where a scan is too slow, the type author declares a counter or summary attribute and the cascaded actions that maintain it (ADR-0019) — a projection that is declared, recorded and therefore honest.

**External identifiers are declared.** An attribute may be declared `external: <source>`; uniqueness per source is an automatic invariant (ADR-0009) and `lookup` finds by it. The importer writes `external: legacy` for every ported key (ADR-0015, ADR-0018).

**Transport.** The surface is the library's API. HTTP, gRPC or an agent tool binding are deployment shapes that expose it one-to-one; none adds operations.

## Alternatives rejected

### A general query language over any attribute

Rejected: unindexed scans over a busy type are the read surface's version of the double-booking window — correct, and unusable. Declared indexes make cost visible at publish time.

### Store-maintained materialised views

Rejected: a projection the declaration does not name is a second, unrecorded writer whose lies are silent. The declared-counter pattern gives the same speed with a record.

### An atomic batch

Rejected: it would let a caller compose multi-object transactions the declaration never described, which defeats the printable rule set. Anything that must be atomic is declared as a cascade.

## Consequences

- TODO.md's read-surface, global-queries and external-identifiers items are closed; no open model questions remain.
- The agent-facing tool schema is a rendering of `declaration` plus `availability`; it is not a separate artefact.
- `check` and `availability` are the two operations agents call before every request; both are cheap and lock-free.
- A `summary` attribute set per type is declaration metadata for payload control.

# ADR-0018: Every object carries a store-assigned, globally unique identifier

- **Status:** Accepted
- **Date:** 2026-09-07
- **Refined by:** ADR-0077 — the id is a UUIDv7, and the store takes its id source and clock as injected dependencies.

## Context

Objects are referenced from many places at once: from other objects across types, from every event in the log, from idempotency keys and causal lineage, from consumers' URLs and agents' tool calls, and from external systems that hold a link back (Jira, Xero, a printed label). Every imported object arrives with a primary key from the legacy database that must not collide with anything the new store creates (ADR-0015).

The first consumer shows what happens when identity is left to the consumer. Its serial numbers do three jobs at once — identity, business identifier, and label text — and the design of the operations extension had to fix *when* a serial is minted ("serial at birth, label at receipt") so that abandoned drafts would not burn serials. Accessories and spare parts get a generated serial that the receiver may **edit** at intake (`wr:docs/proposals/operations-system-design.md` §4, "Intake capture"). An identifier that can be edited is not an identity.

The question raised in the walkthrough (`docs/design/first-consumer-walkthrough.md` §6) was whether ObjectKeeper should offer a sequence primitive for minting. The answer chosen is stronger and simpler.

## Decision

ObjectKeeper assigns every object it manages a **globally unique identifier** at creation, including at import. The identifier is:

- **store-assigned** — the consumer never supplies it and cannot choose it;
- **globally unique** — across types, across time, and across deployments that might later be merged, so no per-type or per-table sequence;
- **immutable and never reused** — it is the handle every reference, event, and external link holds;
- **opaque** — it encodes no business meaning. Type, serial, and any other meaning are attributes.

Three kinds of identifier are therefore distinct:

| Kind | Who assigns | Where it lives | Example |
|---|---|---|---|
| **Object id** | ObjectKeeper | on every object; in every reference and event | the handle an agent or a URL uses |
| **Business identifier** | the consumer | a controlled attribute with a uniqueness invariant | asset serial `GO2-W-38172` |
| **External identifier** | another system | a controlled attribute naming its source | Xero `ContactID`, Jira key, legacy primary key |

## Alternatives rejected

### Consumer-supplied identity, with or without a sequence primitive

The first consumer's serial-minting service is the working example, and it shows the cost: the mint moment becomes a design decision entangled with labelling, the value is a business artefact that people want to edit, and every place that references an object inherits the business rules of the serial. A sequence primitive would make minting easier without fixing any of that.

### Natural keys

Using the serial, or a (type, serial) pair, as identity. Rejected because the first consumer edits serials at intake, and because a natural key that is unique per type is not unique across the event log, the idempotency table, or an import from a database with its own keys.

### Per-type sequences (integer primary keys)

The legacy database's scheme. Rejected because integers collide on import and on merge, leak counts, and are guessable — an agent that mistakes a robot id for an accessory id gets a different valid object rather than an error.

## Consequences

- **References and events use the object id.** Relationships are stable under any attribute edit, including a serial correction. The event log, idempotency keys and causal lineage all reference objects and events by identifiers of this kind.
- **Business identifiers are ordinary controlled attributes.** Uniqueness is a declared invariant (ADR-0009). *The sentence "ObjectKeeper offers no sequence primitive" that stood here was withdrawn by ADR-0029 in design iteration 2 (pending author review): a business identifier may be minted from a named, scoped sequence at creation.*
- **External identifiers are ordinary controlled attributes** that name their source. ADR-0037 decided that the declaration marks them (`external: <source>`), with uniqueness per source automatic and a `lookup` operation on the read surface.
- **Import assigns new ids and preserves legacy keys.** Every ported object receives an object id; its legacy primary key is kept as an external identifier with source `legacy`, so the preserved audit history and any surviving links still resolve. Cross-references are re-pointed through the legacy-key-to-object-id mapping during import (ADR-0015).
- **Agents and UIs hold one stable handle.** Every read returns the object id and every request names it; there is no natural-key lookup to get wrong.

## Open

Representation details, not model questions:

- **Format.** Random (UUID v4) or time-ordered (UUID v7 / ULID). Time-ordered gives index locality and a creation order for free; the recommendation is time-ordered, to be confirmed when storage is designed. *(Decided by ADR-0077: UUIDv7, in canonical text form; storage-schema.md §2.)*
- **Rendering.** Whether the id is shown to humans and agents with a type prefix (`robot_…`) for legibility, as a presentation of the same opaque value. Rendering must never be parsed back into meaning.

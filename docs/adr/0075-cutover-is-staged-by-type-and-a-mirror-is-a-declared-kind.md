# ADR-0075: Cutover is staged by object type, and a type another system still owns is declared as a `mirror`

- **Status:** **Accepted** — the author chose staged cutover 2026-09-09; the `mirror` marking is the consequence, derived here and pending review
- **Date:** 2026-09-09
- **Refined by:** ADR-0077 — import writes each row complete in one pass; the two-pass sentence below is annotated; ADR-0080 — the marking is for the duration of a cutover, and a type another system owns for good is an ordinary type the sync writes. ADR-0093 — the stage order counts the legacy system's writes as well as its references.
- **Refines:** ADR-0015, ADR-0027, ADR-0040

## Context

ADR-0015 left cutover open: "per deployment, possibly staged by object type with ObjectKeeper read-only for the rest. The plan is a project concern; the design must not assume a soft landing." The author has chosen **staged by object type**.

The constraint that shapes everything else is also ADR-0015's: **cutover cannot be dual-write**, because a sole write path means the legacy application and the rebuilt one cannot share a store. Staging by type is compatible with that only if ownership is per type and absolute: at any moment each type is owned by exactly one system, and nothing writes a type it does not own.

That is coherent. What it needs, and what the design did not have, is a way to declare a type that this store holds and does not own.

## The problem staging creates

Take types A and B where A holds a reference to B.

| A | B | Works? |
|---|---|---|
| legacy | legacy | yes |
| migrated | migrated | yes |
| **migrated** | **legacy** | yes — B must be present here for A to reference, as something read-only |
| **legacy** | **migrated** | **no** — the legacy row's foreign key points at rows it no longer owns, and making it work means either dual-write or a new integration inside the system being retired |

So the ordering follows: **a type may migrate only after every type that references it has migrated.** Referrers before referents. The most-referenced types go last, which for the first consumer means the customer moves near the end and is mirrored for longest — acceptable, since it is also the type that changes least. *(Refined by ADR-0093: a legacy transition that writes another type is a second kind of edge, and the stage is a component of the combined graph.)*

Where the reference graph has a cycle, its members have no valid order between them, so **a cycle is one stage**. ~~Unless the cycle can be broken: a cycle every one of whose edges is optional can be, by importing the members with those references absent and writing them in a second pass, which the two-pass import already requires for every reference; only a cycle containing a required reference forces its members into one stage.~~ **Withdrawn, D201 (2026-09-09).** The struck sentences answered the import question and not the stage question. The stage rule rests on ownership: a legacy referrer cannot point at rows a migrated referent now owns, and a nullable column does not change that, since the business still needs the reference filled — a legacy delivery item that cannot bind to a new unit is broken whether or not its column admits null. Nullability mattered only to the import, where a cycle once needed a second pass, and ADR-0077 removed the second pass; a cycle now costs nothing at import either way. The cutover document's stage table had already placed the one cycle in a single stage while its prose said otherwise.

The first consumer's graph has exactly one cycle — the soft peg on an inbound unit against the hard bind on a delivery slot, joining units and delivery items. All seven of its edges are nullable, which matters to nothing now: its four tables are one stage. See `first-consumer-cutover.md` §2.

## Decision

### 1. Cutover is staged by object type, one owner per type at any moment

Nothing writes a type it does not own. There is no window in which both systems write one type.

### 2. A type this store holds and does not own is declared `mirror`

```text
type Customer version 1 mirror {
  tracking record
  states ACTIVE category live, ARCHIVED category closed
  attr legacy_key string external "legacy" indexed
  attr name       string
}
```

For the duration of a cutover (ADR-0080), a mirror declares its attributes, its states and its external identifier, and **no transitions at all**. It is written only by the import path — the built-in assertion, capability-gated and recorded (ADR-0040, ADR-0054) — so every change to it is attributable to the import that made it, and none of it looks like an ordinary transition.

Checks 15, 34 and the terminal-state rule do not apply to a mirror: they are about a lifecycle, and a mirror's lifecycle belongs to another system. **Check 53 is new** and is what makes the marking mean something. Nothing here may write a mirror: no outcome may `call` or `create` into one, and no `part` or `owner` may compose with one, because a composition binds the part's lifetime to the whole and a mirror's lifetime is not this store's to bind.

Four further routes to a writable mirror were found by probing rather than by reasoning, and each is refused: a mirror binding a machine, which would hand it that machine's transitions; a mirror declaring transitions of its own; a mirror extending a type; and an ordinary type extending a mirror, which would let a type this store owns inherit the shape of one it does not. The first was the important one — a mirror with a bound machine is writable through every transition the machine supplies, which defeats the marking completely.

An ordinary type may **reference** a mirror, read its attributes and its state in a guard, and require it in an invariant. That is the whole point: a migrated delivery can be guarded on its not-yet-migrated customer.

### 3. Cutting a type over is a version advance

The marking is removed and the real transitions are declared, which is an ordinary declaration change with the ordinary migration mapping (ADR-0027). The objects already exist with their ids and their external identifiers; nothing is re-imported and no id changes. That is the payoff of importing a mirror as real objects rather than as a cache.

## Alternatives rejected

- **Declare the legacy lifecycle as import-gated transitions.** Expressible today, and it was the first thing tried. Rejected because it restates the lifecycle being retired, in a second place, where it will drift — and because every guard on it would be `actor.has(LEGACY_IMPORT)`, which says nothing about the domain. A marking says the true thing instead: this type is not ours.
- **Hold the not-yet-migrated types outside ObjectKeeper and reach them with an external evaluator.** An evaluator returns a verdict and never a value (ADR-0069), so a guard could ask "is this customer active" but a delivery could not reference a customer, and `referrers`, deletion guards and invariants would all stop at the boundary.
- **Big-bang.** Not rejected on its merits; the author chose otherwise. It remains the simpler path and this ADR is the cost of not taking it.
- **Allow dual-write for the duration.** Rejected by ADR-0015 and not reopened. It is the one thing that would dissolve the problem and it dissolves the mediated property with it.

## Consequences

- The declaration syntax gains `mirror` on a type header and check 53. `scripts/check-syntax-doc.py` implements both, confirmed by probe in each direction: a composition with a mirror and a `call` into one are refused, and a reference to one with a guard reading its state is clean.
- **The stage order is derivable from the declaration**, since the reference graph is in it. Publishing can compute it and should: a proposed stage that migrates a type before one of its referrers is an error the tooling can name.
- The first consumer's graph was extracted on 2026-09-09 and is written up in `design/first-consumer-cutover.md`: one cycle, which is one stage, and seven stages over its tables. The caveat there is the important one — the stage unit is an ObjectKeeper **type**, and its tables are not its types.
- **A legacy read of a migrated type is not solved by this**, only a legacy *write*. Where the retiring system must still display data that has moved, that is a read-only projection out of the read surface, and whether each such screen is worth building is a per-stage judgement about that codebase.

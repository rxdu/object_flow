# ADR-0050: A type declares whether it is serial-tracked or quantity-tracked, and a slot may be filled by either

- **Status:** Accepted — repair of D33, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Amended by:** ADR-0067 — a third mode, `record`, for a type that tracks no physical thing. Twelve or more of the first consumer's entities are records rather than tracked things, and none of them is quantity-tracked.

## Context

The design carries two incompatible inventory shapes and never reconciles them. The first consumer serialises everything: one object per physical unit, with a lifecycle from available through reserved to sold. The orders case study models stock as counters on a product, with reserve and release adjusting numbers. The edge-case catalogue deferred quantity-tracked consumables to iteration 4; iteration 4 modelled quantity and never met a serialised unit, so the deferral pointed at work that was not done.

Real catalogues have both. Capital equipment is serialised because each unit has a history; screws are counted because they do not. They are frequently reserved through one flow onto one order.

## Decision

1. **A type declares its tracking mode**, `serial` or `quantity`.
   - **Serial-tracked**: one object per physical thing, its own lifecycle, reserved by binding.
   - **Quantity-tracked**: one stock object per (model, location), with `on_hand` and `reserved` counters and actions that adjust them under the guards of ADR-0032's arithmetic.
2. **A slot declares which fill form it takes.** A serial slot binds an object. A quantity slot holds a number and references a stock object. A configuration may contain both, which is the ordinary case: a robot slot and a cable-tie quantity in one kit.
3. **Reserve means the same thing in both**: the thing is spoken for and another order cannot have it. Only the representation differs, and the difference is visible in the declaration rather than implied.
4. **Neither mode is a migration path to the other.** Changing a type's tracking mode is a new type, and every object moves to it by supersession one at a time (ADR-0028), because the identity of the things being tracked changes. There is no type-level supersession.

## Alternatives rejected

- **Serialise everything**, as the first consumer does. Rejected: it is that consumer's luxury, as ADR-0032 already noted, and minting an object per washer is absurd.
- **Count everything**, as the orders study does. Rejected: it destroys per-unit history, which is the whole point of a serialised asset.
- **Infer the mode from whether a serial attribute is declared.** Rejected: inference from an incidental property is exactly the kind of implicit rule the project exists to remove.

## Consequences

- The first consumer can model spare parts by quantity and robots by serial in one delivery, which its own operations design wanted and could not express.
- Available-to-promise has two shapes, one an aggregate over objects and one a counter read, and the read surface must offer both.
- The edge-case entry deferring quantity tracking is removed.
- D33 is resolved.

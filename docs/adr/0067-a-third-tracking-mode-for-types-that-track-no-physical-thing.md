# ADR-0067: A third tracking mode, for a type that tracks no physical thing

- **Status:** Accepted — recommendation accepted by the author 2026-09-08
- **Date:** 2026-09-08
- **Amends:** ADR-0050
- **Answers:** open question 12

## Context

ADR-0050 gave every type a mandatory tracking mode, `serial` or `quantity`, on the reasoning that inferring it from an incidental property is the implicit rule the model rejects by name. That reasoning is sound and the two modes are real: capital equipment is serialised because each unit has a history, screws are counted because they do not.

It is the coverage that is wrong, and the evidence is in the first consumer rather than in any reviewer's model. Of the entities in `~/RduWs/wr_inventory_management`:

- **three** are serial-identified — robot, accessory, spare part, each with a minted serial (`wr:app/models/inventory.py:28,112`, `wr:app/models/spare_part.py:18`);
- **twelve or more** are not physical things at all — delivery, delivery item, service, service part, procurement order, shipping record, warranty contract, packing list, intake batch, note, audit log, api key;
- **none** is quantity-tracked with a stored counter. There is no `on_hand` column anywhere; availability is computed on demand (`wr:app/services/atp.py:9`), and the only count-based things are non-inventoried supplies whose quantities live on the lines that reference them.

Worse, all three serial types carry a vestigial `quantity` column fixed at one, annotated "Always 1 for individual tracking" (`wr:app/models/spare_part.py:32`). The declaration already says something untrue in the system this design was drawn from, and porting it as written would oblige a delivery, a service and an audit record each to claim it is one physical thing per row.

## Decision

A third mode, **`record`**: the type tracks no physical thing. It declares no counters, and nothing about it is inferred. `tracking` stays mandatory, so the choice remains explicit, which is ADR-0050's reason and is untouched.

```text
tracking serial | quantity | record
```

## Alternatives rejected

- **Make `tracking` optional, absent meaning "tracks nothing".** Rejected for ADR-0050's original reason: absence would be the commonest case and would carry meaning by default, which is the implicit rule the model rejects.
- **Let a record type declare `serial` harmlessly.** This is the status quo. Rejected because the mode is read by check 31 and by the storage layer, and because a declaration that is inspectable at runtime must not contain a claim its author knew to be false.
- **Reconsider `quantity` at the same time.** The survey shows the first consumer does not use it, which is a reason to watch it rather than to remove it: the orders case study needs it, and ADR-0050's cable-tie case is real even if the current system computes rather than stores. Left as it is, with the observation recorded.

## Consequences

- `docs/design/declaration-syntax.md` §2 and §7 admit the third mode; check 31 says a `record` type declares no counter; check 42 accepts it.
- `docs/DESIGN.md` §5.10 is updated.
- Roughly four fifths of the first consumer's types will declare `record`, which is a fair reflection of an operations platform: most of what it holds is a record of something that happened, not a thing on a shelf.
- ADR-0050's rule that changing tracking mode is a new type still holds, and now covers a change into or out of `record`.

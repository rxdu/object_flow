# ADR-0046: The outcome grammar — cascade inputs, named creations, bound iteration, repeat, `this_event`, and part re-parenting

- **Status:** Accepted — repair of D11 to D15, D18 and D29, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refines:** ADR-0019

## Context

The five case studies were written in a notation the model never defined. Seven constructs they rely on do not exist, and the gap was invisible because the notation reads like the design. Every defect below was verified against the case study that uses it and the ADR that fails to grant it.

An outcome could not pass inputs to a cascaded transition, though every study does it. It could not name an object it created, which makes ADR-0028 self-contradictory, since supersession takes the successor as an input id and also permits creating it in the same outcome. It could not create N objects from a count, which procurement needs. It had no `this_event`, on which every approval guard depends and which was chronologically impossible anyway. There was no event-reference attribute type, so `Approval` could not be declared at all. Splitting an object was recorded as uncovered three times and asserted as expressible once.

## Decision

An outcome is a sequence of steps, applied in order (ADR-0038). The grammar is:

```text
outcome:
  <attribute> := <expression>                     write on this object
  let <name> = create <Type>(<attr> := <expr>, …) create, bound for later steps
  <path>.<transition>(<input> := <expr>, …)       cascade, with inputs
  for <name> in <relationship> [where <predicate>]: <steps>
  for <name> in 1..<expression>: <steps>
```

1. **Cascades take inputs.** `slot.unit.reserve(slot := s)`. Inputs are evaluated in the state prevailing when that step runs.
2. **Creations may be bound.** `let w = create WarrantyContract(...)` makes `w` usable in later steps of the same outcome. Names are single-assignment and scoped to the outcome. This is what makes ADR-0028's cascaded successor expressible: `let d2 = create Delivery(...)` then `supersede(successor := d2)`.
3. **Iteration binds an element name explicitly.** `for s in slots where s.unit != null: s.unit.sell()`. There is no implicit rebinding of `this`, which removes the scoping ambiguity of D22. Order is ascending object id (ADR-0038).
4. **Repeat over a count.** `for i in 1..inputs.quantity: create Unit(...)`, bounded by the same declared fan-out cap as any iteration, refused with `over-limit` (ADR-0041). This makes procurement expressible.
5. **`this_event` is available.** An event's identity is allocated when its transition begins applying, and its content is completed at commit, so an outcome may reference the event that will record it. `event` becomes an attribute type, which is what lets a consumer declare `Approval.event` and write the guards of ADR-0035.
6. **Dynamic attribute writes are not supported.** A transition writes the attributes it names. The CRM merge, which wrote `<each attribute in values>`, declares the fields it resolves, exactly as any edit action does under ADR-0042. A merge that must cover every attribute is a code generator's job, not the runtime's.
7. **A composition part may be re-parented** by a transition that writes its owner reference, subject to guards like any write. Exclusive membership holds at every instant, and the part never outlives all wholes. This makes **split** expressible: create the new whole, move the chosen parts, leave the source complete. Split therefore moves out of the edge-case catalogue.

## Alternatives rejected

### Keep outcomes as a flat unordered set of writes and cascades

The prior shape. Rejected: it cannot express a two-step cascade, cannot name a created object, and its lack of order made ADR-0038's sequencing undefinable.

### Allow dynamic writes with a map-typed input

Rejected: an attribute set decided at request time cannot be checked at publish, cannot appear in a printed rule set, and makes the parameter schema unstable. It is the one place where the case studies asked for something the design should refuse.

### Leave split uncovered and remove the walkthrough's claim

Rejected once re-parenting turned out to cost nothing. Split appeared in three of five studies and is the one-to-many mirror of supersession; refusing it while granting supersession was an accident of which one got an ADR.

## Consequences

- ADR-0019's outcome description is superseded by this grammar; its atomicity, causality and acyclicity rules stand.
- ADR-0028's contradiction is resolved: the successor may be an existing id or a name bound earlier in the same outcome.
- `event` joins the attribute types in DESIGN.md §5.2, and `Approval` becomes declarable, so ADR-0035 works.
- The case studies must be re-expressed in this grammar. Their claims are not evidence until they are.
- `docs/design/edge-cases.md` loses the two split entries and the merge entry, and gains an entry for dynamic writes.
- D11, D12, D13, D14, D15, D18 and D29 are resolved.

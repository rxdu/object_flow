# ADR-0093: The cutover order counts the legacy system's writes as well as its references

- **Status:** Accepted by the author, 2026-09-23, after a check of every decision against the design — decided at the author's direction, against `docs/PRD.md` N5; repairs D206
- **Date:** 2026-09-23
- **Refines:** ADR-0075

## Context

PRD N5 requires the first consumer's data and legacy history to be ported and preserved. ADR-0075, the author's choice of a cutover staged by type, rests on "nothing writes a type it does not own", and derived the order from references alone.

D206 found the legacy system's own cross-type writes. Completing a delivery marks its units sold and creates their warranty contracts (`wr:app/core/state_registry.py:771`). So while deliveries stay legacy-owned, so must units and warranty contracts, which the reference-only order had migrated earlier.

## Decision

### 1. The stage order is computed over two kinds of edge

- **A reference:** type A holds a reference to type B, so A migrates no later than B. This is ADR-0075's rule.
- **A write:** a legacy transition of A writes B, so B migrates no earlier than A. The store's side is the same: check 53 forbids an owned type's outcome from reaching a mirror, so a migrated type's cascade targets migrate no later than it.

A strongly connected component of the combined graph is one stage.

### 2. The write edges are extracted, not guessed

The legacy side's edges come from the first consumer's transition registry, its `side_effects=` lists, and the store's side from each declaration's `call` and `create` steps. The order is recomputed after the table-to-type mapping `TODO.md` already queues.

### 3. The consequence for staging is stated, and the choice stays the author's

At least seven tables form one stage: deliveries, delivery items, delivery configurations, robots, accessories, spare parts and warranty contracts. That is most of the operational core, so staging buys less than `first-consumer-cutover.md` §3 suggested.

Staging and big-bang both meet N5. Which to use was the author's choice in ADR-0075, and this decision corrects the rule the choice was made under without remaking the choice.

## Alternatives rejected

- **Keep the reference-only order, and have the legacy system call the store for the types it no longer owns.** That is a new integration inside the system being retired, which ADR-0075 rejected.
- **Decide big-bang here.** It is the author's decision, and nothing in N5 forces it.

## Consequences

- `first-consumer-cutover.md` states the combined rule and the collapsed core, and marks its table-level order as superseded until it is recomputed over types.
- ADR-0075's rule is annotated.
- `publish-and-import.md` §7 states both edges.
- D206 is resolved as a rule. The recomputation is the queued mapping work.

# ADR-0052: Grammar amendments found by re-expressing the case studies

- **Status:** Accepted — repair of D37 to D41, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refined by:** ADR-0055 — a third invariant form for single-object properties, and outcome steps that add to and remove from a set; ADR-0060 — twelve decisions from the iteration-11 syntax review; ADR-0061 — ten decisions from the payments-ledger review; ADR-0062 — a normative statement must cite the check that enforces it; ADR-0063 — the swap test names its normalisations, and check 52's claim is retracted; ADR-0073 — `clear` writes absence, and is the only way to.
- **Amends:** ADR-0046, ADR-0047, ADR-0045

## Context

ADR-0046 and ADR-0047 replaced the notation the case studies were written in, and the studies were marked as no longer evidence until re-expressed. Re-expressing them is the honest test of the grammar, and it found five things the grammar could not say. Each was found by trying to write a declaration that a study needs, not by inspection.

## Decisions

### 1. Iteration takes any collection-valued expression (D37)

ADR-0046 allowed `for <name> in <relationship>` and `for <name> in 1..<expr>`. Splitting a delivery iterates a **set-valued input** naming the slots to move, and the merge iterates a relationship **of an input**. Neither is a relationship of `this`.

The iteration source becomes any expression yielding a collection: a relationship of `this` or of an input, a set-valued input, or either with a `where` filter. The `1..<expr>` form stays for counts. Fan-out caps and ascending-id ordering apply to all of them.

### 2. A creation names its transition when the type has more than one (D38)

`create Unit(...)` is ambiguous for a type with several creation transitions, which the first consumer has: a procured unit is born `REQUESTED`, a manual add `INTAKE`, opening stock `AVAILABLE`. Creation is a transition (ADR-0002), so the form is `create <Type>.<transition>(...)`. The bare form remains legal only where the type has exactly one creation transition, and publishing rejects it otherwise.

### 3. A write from an unsupplied optional input is skipped (D39)

`email := inputs.email` where `email` is an optional input would, under three-valued semantics, write absence and clear the field. That is wrong for every partial-update case, and the merge and edit actions of ADR-0042 are exactly that shape.

**An outcome write whose right-hand side is an unsupplied optional input is skipped.** This is a rule about optionality, not a branch, so outcomes stay straight-line. Clearing a field deliberately is a distinct declared input or a distinct action, which is better anyway: "leave alone" and "set to nothing" are different intents and should not share a wire form.

### 4. `this` is available in a creation outcome (D40)

ADR-0046 made `this_event` available by allocating an event's identity before its outcome runs. The same must be said of the object: in a creation transition, the new object's id is allocated before the outcome runs, so the outcome may write its attributes, pass `this` to a cascade, and be referenced by parts it creates. Without this an order cannot create its own lines.

### 5. A type-scan invariant must be symmetric (D41)

ADR-0045 restricted invariants to relationships with declared inverses so the affected set is computable by reverse traversal. That does not cover an invariant that scans a type, such as the booking overlap rule, where there is no relationship to invert.

A type-scan invariant must be **symmetric**: the predicate that finds a conflict from a newly written object must be the same predicate that would find it from the other side. Overlap, equality on a shared key, and their conjunctions are symmetric; publishing recognises those shapes and rejects others, which must be restated as relationship invariants. Symmetry is what makes "recheck the objects that conflict with what I wrote" equal to "recheck what I wrote".

## Alternatives rejected

- **A conditional write in outcomes**, to solve 3. Rejected: it is branching, which ADR-0019 and ADR-0038 exclude to keep an outcome readable as one path.
- **Requiring all inputs on an edit**, so absence never arises. Rejected: it forces every caller to read and echo the whole object, which is the lost-update pattern versioning exists to prevent.
- **Inferring the creation transition from the initial state**, to solve 2. Rejected: it is inference from an incidental property, the same objection ADR-0050 made to inferring a tracking mode.
- **Allowing arbitrary type-scan invariants and scanning on every write**, to solve 5. Rejected for the reason ADR-0045 gave: the cost of a write would depend invisibly on an unrelated declaration.

## Consequences

- The five case studies are re-expressed against this grammar and their banners are lifted. Their declarations are now testable against a future parser.
- `relevant`, the named attribute set ADR-0035 implied, is no longer needed: `changed_since` takes an inline attribute list, so no new declaration element is required.
- D37 to D41 are resolved, and the re-expression that found them is complete.

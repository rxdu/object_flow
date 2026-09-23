# ADR-0087: Erasure reaches everything that holds the person, and destroys nothing that belongs to someone else

- **Status:** Accepted — decided 2026-09-23 at the author's direction ("iterate the design until all requirements can be covered"), against `docs/PRD.md` D8, T3, N5 and UC-17; repairs D207 and D208
- **Date:** 2026-09-23
- **Refined by:** ADR-0096 — a failed file deletion is retried by the next erasure request, not by a sweep.
- **Refines:** ADR-0017, ADR-0031, ADR-0078

## Context

PRD D8 requires personal data in datapoints to be erasable "like personal data anywhere else in the store", and UC-17's acceptance is that a customer's personal values "are removed from objects, history and datapoints". Two findings of the design review break that (`design/defects.md`).

**D207.** Erasure deleted the content behind a file's hash. Files are content-addressed so that duplicates collapse (ADR-0017), so one object's erasure deleted every other object's identical file. That fails N5 (data preserved) and T3 (history reconstructable) for the objects that were not erased.

**D208.** Erasure stopped at the erased object. A person's data also sits in two places the first consumer will produce:
- **predecessors in a supersession chain.** Xero merges customers, and ADR-0080 models a merge as supersession, so an erased customer's merged duplicate keeps the person's values.
- **the event of a caller** that passed a personal value into the erased object through a `call` or `create` argument.

ADR-0078 claimed a personal input was redacted "wherever it was recorded", which the design did not do.

## Decision

### 1. A file is deleted when its last unerased reference goes, not when one reference does

The store keeps a **reference index per hash**: one row per place a hash is written — an attribute, a set-valued side table, or an event payload — marked erased when erasure reaches it. The log can rebuild the index, so it is an index, not a second source of truth.

1. Erasure marks the references it reaches as erased and commits.
2. It then deletes the content of any hash whose references are now all erased.
3. A deletion that fails leaves the hash marked pending. A sweep retries it, and the pending set is readable.

A file two objects share survives the erasure of one of them.

### 2. Erasure follows the supersession chain

Superseding an object declares that its successor continues the same thing — merged, moved or converted (ADR-0028). So erasing any member of a chain erases every member, predecessors and successors, each through its own type's `erase` transition, in one request. Publishing rejects a type that declares personal attributes, takes part in supersession and declares no `erase`, since the chain could not be erased through it.

### 3. Erasure reaches the caller's event

When a personal value reaches the erased object through a `call` or `create` argument, the caller's event recorded it as an input. Erasure walks each of the erased object's events up its `cause_position` chain, and redacts in each ancestor event the inputs that flowed into the erased object's personal attributes. Which inputs those are is what check 10's taint analysis already computes, per declaration version.

### 4. The proposal clause names whose values

ADR-0078 §2 redacted proposals "whose inputs carry a personal value" without saying whose. It now reads: every proposal targeting an erased object, and every proposal whose inputs flow into an erased object's personal attributes, by the same taint analysis.

### 5. What erasure still cannot reach is said once

Two things, each in `DESIGN.md` §8:
- a personal value typed into another object's free text, which no declaration marks as personal;
- anything a consumer copied out of the store. The export of ADR-0094 omits personal values so that it is never one of those.

## Alternatives rejected

- **Per-reference blobs, salted by owner, so nothing is shared.** Deduplication goes, which ADR-0017 chose on purpose. And a legacy photo attached twice would then port as two blobs.
- **Delete by hash and accept the collateral.** It fails N5 and T3 for objects nobody asked to erase.
- **Erase only the named object, and leave the chain to the requester.** A right-to-erasure request names a person, not an id, and the store is the only party that knows which ids the person became. UC-17 fails for any merged customer.
- **Redact caller events by searching payloads for the erased values.** Matching values is guesswork: a value can recur by coincidence, or be transformed on the way. The taint analysis knows the flow exactly.

## Consequences

- `DESIGN.md` §8 describes all four reaches and the two limits.
- `storage-schema.md` gains the reference index, with `ok_file.erase_pending`, and erasure's steps change.
- `declaration-syntax.md` gains a check: a type with personal attributes that takes part in supersession must declare `erase`.
- ADR-0017's "deletes referenced file content" and ADR-0078's "wherever it was recorded" are annotated.
- D207 and D208 are resolved.

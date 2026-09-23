# ADR-0056: Ten model amendments found by writing the declaration syntax

- **Status:** Accepted — repair of D50 to D59, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refined by:** ADR-0060 — twelve decisions from the iteration-11 syntax review; ADR-0061 — ten decisions from the payments-ledger review; ADR-0066 — a cascade clause carries arguments; ADR-0077 — the recorded number is the declaration version, which fixes every type's version. ADR-0087 — a type with a personal attribute that takes part in supersession must declare `erase`, so decision 7's "stays optional" no longer holds for it. ADR-0101 — a publish's migrations are the second exception, beside erasure, to a terminal object admitting nothing further.
- **Amends:** ADR-0024, ADR-0026, ADR-0027, ADR-0031, ADR-0042, ADR-0046, ADR-0053

## Context

Four review rounds on the declaration syntax found ten places where the model was silent, stale or wrong. Each surfaced only because someone tried to write the thing down. They are recorded together because each is one decision, and a decision that lives only in a syntax draft is not a decision.

## Decisions

### 1. Only one end of a relationship is stored; the inverse is derived

Iteration 3 of the syntax had the runtime maintain both ends of a declared relationship, writing the far object and recording an event on it. That is a **second write path**: the far event has no transition name, so no subscription can filter it, `changes_state` is undefined for it, and it stamps the far object's last-written index, so binding a unit would invalidate approvals on the delivery.

**One end is the stored reference; the declared inverse is a derived view over it.** There is nothing to maintain, no second write, no event on the far object, and no cycle. `Robot.binding` is stored; `DeliveryItem.unit` reads "the robot whose binding is this". Invariants traverse the derived direction exactly as ADR-0045 requires, since it resolves to a query the store can run.

### 2. `supersede` is an outcome step

DESIGN.md §5.4 lists the outcome steps and omits it, while §8 requires supersession and ADR-0028 defines it. `supersede <expression>` joins the grammar.

### 3. A composition declares its delete cascade

"Parts cascade" (ADR-0024) is asserted everywhere and declared nowhere, so the cascade names no transition, carries no bound, and is invisible to the acyclicity check. A `part` end declares the child transition the whole's deletion drives and its limit. Where a whole has several terminal transitions, each names its own cascade or none.

### 4. An object records its type's version, and dependencies propagate

*(ADR-0077: the number an object and an event record is the **declaration version** of the publish in force, which fixes every type's version at that instant; the propagation rule below is about type versions and stands.)*

ADR-0027 versions type declarations. But a type's behaviour depends on the machine, enums, sequences, evaluators and base types it uses, so a recorded type version that does not move when those move identifies nothing. **Advancing any of them requires advancing every dependent type**, which publishing enforces. The cost is real and is the point: changing a widely bound machine is a change to every type that binds it, and should look like one.

### 5. A machine declares what it requires of its binders

A machine holds transitions; the attributes, references and invariants they read live on the type. With two types binding one machine, nothing said what the second must provide, and a check like "an admission naming an invariant the type declares" was undecidable from the machine body. A machine declares its requirements and publishing verifies each binder.

### 6. `referrers` is a built-in aggregate

ADR-0024's deletion guard is written per referencing type, by hand, in other modules, forever. `referrers` is every object holding a live reference to this one. It covers references with no declared inverse, which closes the hole where a one-way reference escaped the guard entirely.

The cost, stated plainly: this needs a reverse index over every declared reference, which ADR-0045 declined to assume for invariants. Deletion is rare and correctness here is load-bearing, so the trade differs; a deployment pays index maintenance on every reference write.

### 7. Erasure runs from any state, including terminal, and stays optional

ADR-0024 says a deleted object admits no further transitions. Erasure requests arrive precisely for closed accounts and retired units, so **erasure is carved out of that rule**. It also stays optional: ADR-0031 says a type *may* declare an erasure transition, and a syntax draft had made it mandatory wherever a personal attribute existed. *(Narrowed by ADR-0087 §2: erasure follows the supersession chain through each member's own `erase`, so a type with a personal attribute that takes part in supersession must declare one, and publishing rejects it otherwise (check 60). Elsewhere it stays optional; `declaration-syntax.md` §6.4.)*

### 8. The conditional is `if … then … else`, and implication is `implies`

`? :` collides with `?` as the optionality marker, and `→` cannot be typed on a keyboard and collides with the to-state arrow in any ASCII rendering. Both are notation, both were chosen before there was a syntax, and both are replaced. DESIGN.md §5.7 and ADR-0053 are amended.

### 9. Inputs are written `inputs.<name>`

The model has always written `inputs.*`. A syntax draft introduced `$name` to break a collision with the loop binder `in`, but `inputs` has no such collision, and fifty sites across the ADRs and case studies already use the long form. The short form is withdrawn.

### 10. Remedy class names are single tokens

`self-serviceable` and `unreachable-from-here` contain hyphens, which lex as subtraction. They become `self_serviceable` and `unreachable_from_here`.

## Alternatives rejected

- **Keeping runtime-maintained inverses and exempting them from the mediated property.** Rejected: the exemption would be invisible in the declaration and in the log, which is what the property exists to prevent.
- **Requiring both ends stored and reconciled by paired transitions.** This is what decision 1 replaces. It costs a transition per direction, forces the authority to be restated on each, and makes the release path a cascade cycle that terminates but the acyclicity check rejects.
- **Leaving the delete cascade implicit.** Rejected: an unbounded, unnamed cascade is exactly what the fan-out cap and the acyclicity check exist to bound.
- **Versioning only types.** Rejected: it makes a recorded version meaningless the first time a shared machine changes.

## Consequences

- DESIGN.md §5.3, §5.4, §5.7, §5.9, §8 and §10 are amended, and ADR-0053's conditional example with it.
- The syntax loses `$`, gains `inputs.`, and drops the runtime-maintained inverse.
- The reverse index of decision 6 is a storage-schema requirement, not just a checker one.
- D50 to D59 are resolved.

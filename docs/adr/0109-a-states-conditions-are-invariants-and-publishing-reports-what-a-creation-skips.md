# ADR-0109: What every object in a state must carry is an invariant, and publishing reports what a creation skips

- **Status:** Accepted — decided 2026-09-24 at the author's direction ("go with your D345 reasoning: invariants and a publish report"), against `docs/PRD.md` revision 7: T1, T4, F1, F2, V1, V3, N5 and §2; closes the question D345 left open, and repairs D379 and D380. The recommendation put to the author named the photo rule among the invariants; production shows it is checked only at intake, so it stays a guard (§1).
- **Date:** 2026-09-24
- **Refines:** ADR-0009 (invariants declared at type level), ADR-0065 (a machine's creation guards bind a creation that replaces it)
- **Amends:** ADR-0102, whose module is still proposed
- **Relates to:** ADR-0045 (invariants enforced after the writes), ADR-0054 (an admission is discharged when the invariant holds), ADR-0085 (observing clauses)

## Context

D345 found that `add_opening_stock` created a unit straight into `AVAILABLE` past `inventorize`'s rules. An `ASSERT` holder could therefore put a unit on offer without the manufacturer serial its model requires, which production refuses, and no override count would show it. It was fixed in place by copying the serial guard onto the creation. The general question was left open: a creation that lands partway through a lifecycle skips the rules of the path it jumps, and nothing in the engine says so. An assertion that did the same would be counted as an override; a creation is not.

The PRD bears on it in three places:
- a rule is "a guard a transition must satisfy … or an invariant a type must keep" (§5);
- nothing puts governed state into a condition its enforced rules forbid, except through an override (T1);
- the problem the project exists to fix is a rule written in more than one place (§2).

The copy on the creation is intake's rule written a second time, and the next route into `AVAILABLE` would need a third.

Production, read at `4109939`, treats the three rules on the way into stock differently:
- **The manufacturer serial** is stated for every unit reaching `AVAILABLE`, at its birth or at the intake gate (`wr:app/services/inventory_item_service.py:277-286`, `:410-416`). It is enforced where a unit is born with the serial in hand: opening stock (`:287-293`) and a walk-in intake (`wr:app/services/intake_batch_service.py:398-404`).
- **The confirmation photo** is checked only for an intake batch, when its labelling is confirmed and again at commit (`wr:app/services/intake_batch_service.py:675-706`, `:816-821`).
- **The label print** is gated in the state registry behind a flag that defaults to off (`wr:app/core/state_registry.py:695-724`). The module enforces it on both routes, as production intends (`unit-journey.md` §4).

## Decision

### 1. A condition every object in a state must meet is an invariant over that state

It is written `state != S or <condition>`, a local or traversal invariant. It is checked after every write that could break it (ADR-0045), so it binds every route into `S`:
- ordinary transitions and creations;
- an override, which must name it to admit it;
- a publish's migrations, which are checked against invariants;
- the import, whose report lists each violation for a disposition.

A guard binds only the transition it is on.

In the unit's journey:
- **The serial and the label become invariants.** `labelled` and `mfr_serial` are invariants of `Robot`, and `UnitLifecycle` requires every binder to declare them, so its creations can rely on them. `add_opening_stock` no longer copies intake's guards.
- **The photo stays a guard.** Production checks it only at intake, so it belongs to that path and not to the state. It stays a guard on `inventorize` alone, and opening stock does not carry it.
- **`inventorize` keeps guards of the same names, for two reasons.** `availability` evaluates guards and not invariants (DESIGN §10), so the offer can say what is missing before anyone asks. And a guard's declared remedy is the one a caller needs: an invariant violation's remedy is inferred, `self_serviceable` where only this object conflicts, which is wrong for a label print that another transition records.

The guard and the invariant are one rule in two roles. The invariant guarantees it on every route, and the guard explains it on the common one. Giving both the same name is what lets the report of §2 relate them.

**An invariant is not trialled directly.** A rule meant to become an invariant is trialled as an `observe` clause on the path's guard of the same name (ADR-0085). The invariant is published when the rule is enforced, and the dry run lists the live objects it would find broken, each resolved by their own transitions, a mapping or an `admit` (DESIGN §5.9).

### 2. Publishing reports what a creation skips

A lifecycle's **first state** is the first state its declaration lists. For each creation into any other state `S`, publishing reports the path the creation skips: the `do` transitions into `S` from the states the first state leads to without passing through `S`. For each of those transitions it reports:
- the guard clauses the creation carries, compared by name;
- the ones the type holds as invariants of the same name;
- the ones nothing carries;
- the parents of an `only via` transition.

It also reports every invariant of the type, since a creation writes the whole object and is checked against all of them.

**It is a notice, not a refusal.** Opening stock, a walk-in intake and a ported unit start partway through a lifecycle on purpose, and a minimal flow stays publishable (PRD V1). The report is part of the dry-run report a `DeclarationChange` carries, so under F7 the approver reads it.

`scripts/check-syntax-doc.py` computes it and verifies any report a document quotes; `unit-journey.md` quotes the journey's:
- `add_opening_stock` carries `may`;
- the invariants `labelled` and `mfr_serial` hold two of intake's clauses;
- `photo` is not carried;
- a walk-in `add_to_intake` skips a shipment's receipt, which is what it is for.

### 3. Two repairs the invariants exposed

- **D379.** Nothing wrote a unit's manufacturer serial or photos after its creation. A unit ordered through `request` whose model requires either could never have been inventorised, even before this decision. `record_manufacturer_serial`, at any state, and `add_photo`, at `INTAKE`, follow production, which edits both on the intake item and the serial on an existing robot.
- **D380.** Check 19 resolved `<Type>.<STATE>` literals but not a bare state compared with the object's own state (`state != AVAILABL`), so a misspelt state in the new invariants passed. It now resolves both, proven by a fixture.

## Alternatives rejected

- **Copy the path's guards onto every creation**, the fix D345 applied. It writes the rule a second time, every new route needs its own copy, and a missing copy is silent, which is what D345 found.
- **A publish check that refuses a creation landing past the first state without the path's guards.** Legitimate creations skip rules on purpose: opening stock skips the photo, as production's does, and a walk-in skips the shipment. The check would force them to carry rules they must not. It would also make a minimal flow with several entry points unpublishable (V1).
- **Count a creation past the first state as an override.** A creation is a declared route, authorised by its own guards, and counting routine opening stock as overrides would drown the signal UC-3 reads. Counting creations by the state they land in, beside overrides, is a diagnostics question. It was put to the author with this recommendation, was not chosen, and is recorded in `TODO.md`.
- **Drop the path's guards and rely on the invariants.** `availability` would offer `inventorize` to a unit with no label, and the request would then be refused with an inferred remedy that points the wrong way.
- **Relate a guard to an invariant by comparing expressions.** Equivalence of two expressions is not decidable in general. The report compares names and leaves the meaning to the reviewer, who reads both side by side.
- **Make the photo an invariant too**, as first recommended. Opening stock would then refuse units production accepts. The photo is intake's rule, not a property of every unit on offer.

## Consequences

- `DESIGN.md` §5.6 states the rule for conditions of a state, and §5.9 lists the report.
- `declaration-syntax.md` §10 lists the report among what is reported without failing, and check 19 covers a bare state; iteration 23.
- `publish-and-import.md` §2 carries each creation's entry as a `NOTICE`.
- `unit-journey.md` and the flow review's appendix declare the invariants, `Robot.model`'s inverse `RobotModel.units`, the `indexed` markings a traversal invariant needs (check 7), and the two new acts. The journey quotes its report, which the checker verifies.
- **Behaviour.** A unit put back on offer by a return, a release from the pool, an undo or an override must carry a label print and any serial its model requires. A ported unit on offer without them is reported by the import and admitted or cleaned upstream (DESIGN §11). Its admission is discharged when the unit leaves `AVAILABLE` (ADR-0054), so it is labelled before it goes back on offer. Turning on a model's `manufacturer_serial_required` is refused while a unit of that model is on offer without one, naming the units, until `record_manufacturer_serial` records it.
- D345's question is closed, and its entry says so.
- **Left open:** whether diagnostics count creations by the state they land in, beside overrides (PRD V2).

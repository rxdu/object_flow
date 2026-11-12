# ADR-0270: An effect records an observation, and a module imports a kind

- **Status:** **Decided** by the author 2026-11-12 ("proceed with your recommendations"), on the recommendation put the same day for issue #84. Built the same day.
- **Date:** 2026-11-12
- **Refined by:** ADR-0271 — proposed: a `create` takes the rest of its target's inputs from the act's, `rest: inputs`.
- **Refines:** ADR-0099 (recording is a request, `declaration-syntax.md` §6.8)
- **Relates to:** ADR-0078 and ADR-0263 (a personal value reaches only a personal attribute); ADR-0146, ADR-0147 (a recording's times, its correction and its erasure); issue #84

## Context

Issue #84, filed 2026-11-11 at 0.21.0, reported that no transition can record a fact about an object in its own request. `declaration-syntax.md` §6.8 says `record` "is requested like any creation, by a caller or by an outcome step", and both checkers refuse the outcome step. A deployment records each change of a unit's hands as an observation of the unit. Each act that changes hands should make that record in the same request, and some of those acts belong to modules that import the unit's.

Reproduced 2026-11-12 at `d21a457` on the reference checker:

- one module: `Thing` declares `custody`, of kind `Custody`, and `Mover.move`'s effect creates `Custody.record`. Refused: "Mover.move creates a Custody, which is neither declared in this module nor imported";
- two modules: `probe_b` declares `imports: { probe_a: [Thing, Custody] }`. Refused: "imports 'Custody' from probe_a, which does not declare it".

The core takes an effect's `create` by the `take` a caller's `record` request takes (`request.rs`). That path dates a recording by its request and stamps its subject's collection. So the runtime records from an effect as from a caller. A library test that bypasses the checkers holds it (below).

Each checker's library holds types alone, never an observation kind. So the checks that follow a `create` into its target, the personal guard and the arguments' types, found nothing and passed it silently. Opening the `create` alone would let a personal value into an observation's field unchecked.

## Decision

1. **An effect's `create` may record an observation** by its kind's generated `record`, with `subject`, the kind's fields and optionally `corrects`. The kind is one a type of the module declares, or one it imports (`flow-format.md` §4.8).
2. **A module may import an observation kind by name** from the module whose type declares it, as it imports a type.
3. **The checks follow such a `create` into the kind as into any type.** Its inputs MUST be the record's, each fitting its field, the subject of its type (the model's check 2). A personal value MUST reach only a personal field (the model's check 10).
4. **The recording is the request's.** It is dated by that request, has its own event, and stamps its subject's collection, as a caller's recording does.

## Alternatives rejected

- **Correcting §6.8 instead**, so that only a caller records. A client's second request in a batch cannot hold that every act makes its record, and a guard reading the collection only approximates it.
- **Recording by an ordinary born-final type.** It runs today, and loses what an observation gives: `occurred_at`, `corrects`, the generated `forget`, the collection `changed_since` reads, and the part rules' exemptions. It would be a second way to record one kind of fact.
- **An import of the subject type bringing its kinds unnamed.** A `create` names what it creates as declared or imported, so a kind recorded from another module is named in its imports.

## Consequences

- **One builder of an observation kind, as the checks see one.**
  - The core's `check/view.rs` `observation_kinds`; the examples checker, the `create` rule, the personal guard and the arguments' types build by it.
  - `check-flows.py`'s `observation_kinds`, alike.
- **The import rule.** The core's `check/mod.rs` and `check-flows.py` take a kind a type of the named module declares.
- **The runtime is unchanged.** The core and the reference runner take an effect's recording by the path a caller's takes.
- `flow-format.md` §4.1's `imports`, §4.4 and §4.8, and the format's card, state it.
- **What a flow meets.** A flow the checks accepted reads as before: the change opens what step 3 refused.
- **Tested.**
  - `effect_recording_plants` in `check-flows.py`, which both checkers agree on through the core's oracle:
    - clean: a create of a kind the module declares; a create of a kind imported by name;
    - refused: a kind of another module not imported, an import naming no kind, a creation other than `record`, a field left out, an argument that does not fit, a subject of another type, and a personal value passed to a field that is not personal;
    - the move's examples run by the reference runner, its `cascaded` naming `Custody.record`, an expectation changed failing its run.
  - `an_effect_records_an_observation`, on the library, so on both backends: the move applies; the custody holds its subject, its field and both its times; its `record` has its own event, the move's next; and the thing's collection is stamped with that position.
  - Ten planted defects were each caught, five in each checker: a kind its module declares refused, a kind imported refused, the kind's inputs unread, the personal guard blind to the kind, and the arguments untyped.

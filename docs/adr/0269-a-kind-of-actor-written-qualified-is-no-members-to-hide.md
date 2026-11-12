# ADR-0269: A kind of actor written qualified is no member's to hide

- **Status:** **Decided** by the author 2026-11-12 ("proceed with your recommendations"), on the recommendation put the same day for issue #85, option 1. Built the same day.
- **Date:** 2026-11-12
- **Refines:** ADR-0176 decision 1 (the invariant `taken_by` generates); ADR-0145 (a kind of actor, written bare)
- **Relates to:** ADR-0216 (a member yields where a built-in would hide it); ADR-0180 (a flow valid under one release stays valid); ADR-0265 (names a store keeps); issue #85

## Context

Issue #85, filed 2026-11-11 at 0.21.0, reported that a type declaring `taken_by` and a member named `human`, `agent` or `service` is refused. A service's line names its whole `service` naturally, and its type declares `taken_by: [human, service]`.

Reproduced 2026-11-12 at `d21a457` on the reference checker. `Line` declares `taken_by: [human, service]` and a reference to `Job`:

- named `service` or `human`, the type is refused: "invariant open_taken_by does not type: it compares an actor kind with an object of Job (the model's check 24)";
- named `job`, it is clean.

The invariant `taken_by` generates writes the kinds bare: `open_by_kind is null or open_by_kind == human or open_by_kind == service`. Both checkers and both evaluators read a bare name as the type's member before a kind of actor (`eval.rs`'s `name`, `check/names.rs`'s typers, `flowrun.py`, `check-flows.py`). So the member hides the kind. A quoted kind is no way out: both checkers refuse an actor kind compared with a string.

The issue named two mechanisms: the generated invariant reading its kinds as kinds whatever the members, or a refusal of the three names as members. Format 1's frozen `servicedesk.yaml` declares a member `service`, so a refusal breaks a flow format 1 accepted. ADR-0216 rejected refusing a name for the same reason.

The format already qualifies a value a bare name would make ambiguous: a state of another type is `<Type>.<STATE>` and an enumeration's member `<Enumeration>.<VALUE>` (`declaration-syntax.md` §8). `library-api.md` names the type of a kind of actor `ActorKind`.

## Decision

1. **A kind of actor may be written qualified**, `ActorKind.human`, `ActorKind.agent` or `ActorKind.service`, wherever a bare one may. A member of a type never hides it. The bare spelling stays, read as before: a member of its name first.
2. **The invariant `taken_by` generates writes each kind qualified**: `<transition>_by_kind is null or <transition>_by_kind == ActorKind.human or …`.
3. **`ActorKind.<x>` naming no kind is refused** at step 3, as the model's check 24 types it.
4. **A bare kind a member hides, compared with an actor kind, is refused naming the member** and the qualified spelling: "service names Line's member service, which hides the kind of actor service: write ActorKind.service".
5. **Nothing a module declares is named `ActorKind`**: a type, an observation kind, an enumeration, a value, a sequence, an evaluator or a machine (step 3, `names`).
6. **The agent tools read a kind written qualified as one written bare**, so what an agent is offered under `taken_by` is unchanged (ADR-0189).

## Alternatives rejected

- **The generated invariant alone reads its kinds as kinds**, the issue's first ask taken literally. It gives the engine's generated text a resolution rule an author's text lacks, so the same expression would read two ways.
- **A bare kind compared with an actor kind reads as the kind wherever the member of its name is no actor kind.** It changes no valid expression's meaning, but both evaluators resolve names as they run, untyped. Each would have to type the other operand first.
- **Refusing the three names as members**, the issue's second ask. It breaks format 1's `servicedesk.yaml`, which ADR-0180 forbids from 1.0.0. A natural name would cost a rename.
- **Kinds quoted, `== "service"`**, so that typing an actor kind against a string passes. It loosens what step 3 holds for every comparison of an actor kind.

## Consequences

- **The core.**
  - `model.rs` and `check/view.rs` generate the qualified kinds.
  - `eval.rs` reads `ActorKind.<kind>` and holds `ACTOR_KIND`.
  - `request.rs`'s rule reading only the object, and `metric.rs`'s reader filter, take it.
  - `check/names.rs` types it, refuses another member of `ActorKind`, and names a member hiding a kind.
  - `check/mod.rs` refuses the name `ActorKind`.
  - `tools.rs` reads it.
- **The reference runner and its tools.** `flowrun.py` reads it; `flowtools.py` reads it; `check-flows.py` holds decisions 2 to 5 in the core's words.
- `flow-format.md` §3, §4.8 and §5, `declaration-syntax.md` §4.2 and §8.1, and the format's card state it.
- **What a flow meets.** A type that named a member `human`, `agent` or `service` beside `taken_by` publishes. A comparison an author wrote bare reads as before. Only a module declaring `ActorKind` is newly refused, and no module in the repository or format 1's corpus does.
- **Tested.**
  - `actor_kind_plants` in `check-flows.py`, which both checkers agree on through the core's oracle:
    - `taken_by` beside a member named `service` and one named `human`, clean;
    - a comparison written qualified, clean;
    - a bare kind a member hides, `ActorKind.robot`, and an enumeration named `ActorKind`, each refused;
    - the `taken_by` sample's examples run by the reference runner beside a member `service`, an agent's rejection refused by `reject_taken_by`, an expectation changed failing its run.
  - `a_member_named_like_a_kind_hides_none_from_taken_by`, on the library, so on both backends: beside a reference named `service`, an agent's closing refused, and a service's, which the member hid, and a person's applied.
  - The Flow Studio's `test_an_agent_may_only_propose_what_a_person_takes`, alike on the core and the reference runner: an agent is not handed what `taken_by: [human, service]` keeps from it.
  - Thirteen planted defects were each caught, six in the reference runner, its tools and `check-flows.py`, and seven in the core: each generator writing the kinds bare, each typer taking any member of `ActorKind`, no finding for a hidden kind, the name `ActorKind` not reserved, and each evaluator and each agent tools reading no qualified kind.

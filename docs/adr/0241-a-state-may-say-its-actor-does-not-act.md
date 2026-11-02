# ADR-0241: A state that is not final may say its actor does not act

- **Status:** **Decided** 2026-11-02 under the author's direction ("go with your recommendations"), on the recommendation given the same day for issue #55; built the same day.
- **Date:** 2026-11-02
- **Refines:** ADR-0182 decision 6 (an actor in a final state is refused `actor_active`)
- **Relates to:** PRD F2, UC-22; ADR-0114 (who may act is decided outside the flow); ADR-0240 (no inferred change to every deployment); `docs/research/suspending-an-actor.md`

## Context

A request whose actor or principal is in a final state is refused `actor_active` (ADR-0182 decision 6), and so are an offer to it, its approval of a version, an import by it and the store's maintenance. A final state is one no transition leaves (`flow-format.md` §4.5). Issue #55, filed 2026-11-01 at 0.10.0, has an account deactivated until its person returns: reactivation leaves the state, so it cannot be final, and the actor acts in the store as an active one does. Only the consumer's gateway refuses it, so a second writer would have to restate the rule. No flow can state it: a condition may not read `actor` (check 64, ADR-0114).

SCIM's `active`, Entra's `accountEnabled`, Okta's suspended and deactivated statuses and GitHub's suspension each mark explicitly that an account may not act, reversibly and apart from removal (`docs/research/suspending-an-actor.md`, each quotation found in its source's page). The issue's alternative, every `closed` state refusing its actor, would give `closed` a second meaning: work is open until it enters a closed or final state, which the standard metrics read (`flow-format.md` §4.1).

## Decision

1. **A state that is not final may declare `acts: false`**, on a type that marks an actor identity, its own or inherited, or on a machine, whose binders that hold actors take it. A state declares `acts` only where it is not final: a final state acts never. A type whose objects are no actors declares none. Step 3 (`names`) refuses either; `acts: true` on a state that is not final states the default.
2. **An actor or principal whose object is in such a state is refused `actor_active`**, remedy `dependent`, wherever a final state's is: a request, every offer to it, a publish's approval, an import by it, the store's maintenance and an example's steps. Reads are not refused, as they are not for a final state. A transition out of the state lets the actor act again.
3. **The installed version decides**, as it decides every rule: a publish marking a state refuses its actors from that version's install on.

## Alternatives rejected

- **Every closed state refusing its actor**, the issue's alternative. `closed` already means work closed to the standard metrics, and it would refuse, with no change to their flows, the actors of every deployment with such a state, as ADR-0240 declined for offers.
- **The consumer refusing alone**, as now. The rule then binds one writer, and every other writer restates it.
- **The state made final.** Reactivation leaves it.
- **A new actor on reactivation**, the old one archived. An identity names one actor and is never reused, so the person would act under a second identity, their history split across two actors.
- **An attribute saying whether the actor acts**, as SCIM's `active` is. The state already says it, and transitions would have to keep the two in step.

## Consequences

- Built in the core (`model.rs`, a state's `acts`; `request.rs`, `resolve_actor` and `active`; `import.rs`; the checker's `raw.rs`, `acts_errors`), the reference checker `scripts/check-flows.py`, the reference runner `scripts/flowrun.py`, the scenario checker `scripts/check-scenarios.py` and `flow.schema.json`. The release fingerprint is unchanged: no built-in state declares `acts`. `flow-format.md` §4.5 and §10, the card, `DESIGN.md` §5.8 and §6, `wire.md`, `errors.md`, `integrate.md`, `library-api.md` and `CHANGELOG.md` say so.
- **The two checkers agree** on all 702 of the oracle's cases, the seven new plants among them, a machine's state and a subtype's inherited identity included (`checker_oracle.rs`, the checker's `acts_plants`).
- **Tested in the core on both backends:** `an_actor_in_a_state_that_does_not_act_is_refused_as_in_a_final_one`, the actor refused at its request, as a principal, at every offer, at an import and at maintenance, acting again once it leaves the state, and refused in its final state; in the reference, the runner's `acts_self_test`.
- **Planted defects, each caught:** in the core, the actor's state read as final alone, the loader ignoring `acts`, and the import taking any actor; in the Rust checker, `acts` taken on a final state and on a type with no actors; in the reference checker, `acts` taken on a final state; and in the runner, `acts` ignored.
- **Not done, and why.** A version's report counts no actor its publish stops, until a case needs it.

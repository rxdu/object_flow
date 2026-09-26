# ADR-0115: The flow description format uses established state-machine terms

- **Status:** Accepted by the author, 2026-09-26. Asked whether the format "is the proper and well-recognized way to describe a state machine", adding "I don't want to invent new terminologies if existing conventions are already there", the author read the review in `docs/design/flow-format.md` Appendix A and answered "adopt standards and update our specs".
- **Date:** 2026-09-26
- **Refines:** ADR-0016 (actions), and the YAML trial of `docs/design/authoring-flows.md` §3
- **Relates to:** ADR-0046 (the outcome grammar), ADR-0085 (observing clauses), ADR-0111 (flags)

## Context

The YAML form of a flow, specified in `flow-format.md`, had named its constructs as the design had grown. A review against UML 2.5.1, W3C SCXML, XState v5, the Symfony Workflow component, AASM, django-fsm, Kubernetes admission policies, OPA Gatekeeper and Jira, each read in its primary form, found the core terms standard (states, transitions, guards, conditions, invariants, `from` and `to`), one term in conflict with its standard meaning, and several invented where a standard term exists.

The conflict is **action**. In UML, SCXML and XState an action is behaviour that a transition runs. The design used the word for a transition that changes an object without changing its state, which UML calls an **internal transition**: it "occurs without exiting or entering the source State (i.e., it does not cause a state change)" (UML 2.5.1 §14.5.12.3). ADR-0016 called the same thing a self-transition, which in UML exits and re-enters its state; `DESIGN.md`'s glossary already said an action has "no exit or entry semantics, unlike a statechart self-transition".

## Decision

1. **The YAML form uses the established term wherever one exists**, and `flow-format.md` cites the source for each:

   | Construct | Was | Is | Source |
   |---|---|---|---|
   | a transition from nothing into a first state | `kind: creation` | `kind: initial` | UML initial pseudostate and its transition, §14.2.4.6; SCXML `<initial>`; XState `initial` |
   | a transition from one state to another | `kind: state_change` | `kind: external` | UML `TransitionKind::external` |
   | a transition that does not change the state | `kind: action` | `kind: internal` | UML `TransitionKind::internal`, §14.5.12.3; XState "targetless transition" |
   | a state no transition leaves | `terminal: true` | `final: true` | UML `FinalState`; SCXML `<final>`; XState `type: 'final'` |
   | what a transition writes | `outcome` | `effect` | UML `Transition::effect`, §14.5.11.6 |
   | a write of a value | `copy: { from: a, to: b }` | `assign: { location: b, expr: a }` | SCXML `<assign location expr>`, §5.4; XState `assign` |
   | a guard that refuses | `enforced` | `deny` | Kubernetes `ValidatingAdmissionPolicyBinding.validationActions` |
   | a guard whose failures are recorded and refuse nothing | `observed` | `audit` | the same |
   | a guard whose failure is reported to the caller and refuses nothing | `flagged` | `warn` | the same |
   | the invariant a state's required attributes generate | `<state>_attributes_present` | `<state>_invariant` | UML `State::stateInvariant`, §14.5.9 |

   `clear` stays, having no standard counterpart. Terms already standard stay as they were.

2. **An `assign` takes an expression**, as SCXML's does, so a value may be computed as well as copied.

3. **The lifecycle diagram follows UML's notation** (§14.2.4): internal transitions are listed inside the states they occur in, not drawn as loops, which in UML are self-transitions that exit and re-enter; a final state is a circle around a filled circle; and a remark about several states is a UML comment, a note anchored to them, not a frame, which in UML is a composite state.

4. **In prose, the design says "internal transition"**, and "action" only when quoting what it was called. `DESIGN.md` is amended. The text language keeps its keywords (`act`, `terminal`, `observe`, `flag` and the outcome of ADR-0046) until the author decides whether the YAML form replaces it (`authoring-flows.md` §7, question 3); the conversion maps each YAML term onto them.

## Alternatives rejected

- **Keep the terms the design grew.** They are consistent with the design documents, but a reader who knows state machines meets words that mean something else to them, and the author asked not to invent terms where conventions exist.
- **Rename the text language and every design document now.** It would make the vocabulary one throughout, but the text language may be replaced by the YAML form, and rewriting its grammar, its checker and every example before that is decided would be work done twice.
- **Keep `kind: state_change` rather than UML's `external`**, since this domain also speaks of external delivery. Rejected: the kind and a transition's name are read in different places, and the standard word is what a reader who knows UML will look for.
- **OPA Gatekeeper's `deny`, `dryrun` and `warn`** for the enforcements. Kubernetes' `Audit` names what an audited guard does, record its failures, where `dryrun` names a mode of the whole policy.
- **A frame for the delivered states**, drawn as UML's composite state, or a category `delivered`. The first implies a hierarchy the format does not have; the second would stop those states counting as `closed`, on which the metrics' notion of finished work rests (`declaration-syntax.md` §1).

## Consequences

- `flow-format.md`, `flow.schema.json`, `scripts/check-yaml-trial.py` and the five trial modules use the terms above; its Appendix A records the review and what was adopted.
- `DESIGN.md` says "internal transition" where it said "action"; ADR-0016 carries a note pointing here.
- The walkthrough's diagram lists internal transitions inside their states, draws `RETIRED` as a final state, and replaces the frame around the delivered states with a note.
- If the YAML form becomes the written form, the text language's keywords are retired with it; if not, whether the text language adopts the same terms is a separate decision.

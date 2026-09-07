# ADR-0016: Actions are self-transitions

- **Status:** Accepted — analysis requested and the recommendation accepted 2026-09-07
- **Date:** 2026-09-07

## Context

ADR-0006 needs a way to write a controlled attribute without changing lifecycle state, or every controlled write forces a state change. DESIGN.md's terminology table calls that an **action** and leaves its modelling open.

The first consumer shows that actions are most of the work, not an edge case. Everything that happens to a Delivery while it is `PREPARATION` is an action: binding a unit to a slot, adding or removing a slot, ticking a packing-checklist item, recording a pre-delivery check result, changing the delivery date. Everything that happens to a unit while it is `INTAKE` is an action: capturing the manufacturer serial, attaching a confirmation photo, recording the label print. None changes the object's state; each is gated on being *in* that state; each writes something the next transition's guard reads. The inventory system implements each as a separate service method that re-checks status by hand — `remove_configuration` calls `lock_delivery_for_change`, which re-reads the status under a row lock and requires `PREPARATION` (`wr:docs/design/business-processes.md`, "Invariant: an applied configuration never changes by itself"). That per-method duplication is what a declaration exists to remove.

Two ways to model it were compared.

## Decision

An action is a transition whose from-state and to-state are the same. It is declared, guarded, addressed, recorded and listed exactly as any other transition. Two rules make that safe and legible:

1. **Transitions are addressed by name, never by target state.** A request "move to `PREPARATION`" for an object already in `PREPARATION` is not a request and is rejected; a request "`bind_slot`" is. This closes the hazard the first consumer recorded, where a same-state request was treated as a successful no-op and wrote an audit row asserting a change that did not happen (`wr:TODO.md`, ST.6 findings).
2. **`changes_state` is derived, not declared.** It is true when from ≠ to. The readable rule set lists actions under their state rather than as arrows on the lifecycle diagram, and the recorded event carries the flag so a subscriber can ask for lifecycle changes only.

A from-state may be a set of states, or any non-terminal state. That is not an action-specific feature: the first consumer's intake batch cancels from three states, and "cancel from any non-terminal state" is common.

## Alternatives

### A. Self-transition (chosen)

Pros:

- **One mechanism.** Guards, structured verdicts, remedy classes, inputs, the derived parameter schema, recording, the event log, idempotency keys, availability listing, invariant analysis and the printable rule set all apply with no second implementation. ADR-0010's one-declaration-many-projections holds for actions for free.
- **State scoping falls out.** "Allowed only in `INTAKE`" is the from-state. Nothing new to declare.
- **Availability is one list.** "What can I do to this object now" returns transitions and actions with the same three-way availability, which is what an agent needs — one tool shape.
- **Invariants stay covered.** ADR-0009's runtime analysis of which transitions could violate an invariant covers self-transitions on the same path. A second element would need a second analysis, which is the one-side-covered-other-side-missed hazard ADR-0009 exists to close.
- **Smaller language, smaller runtime, smaller test surface.**

Cons:

- **The word.** A transition that does not transition. Without rule 2 the lifecycle diagram fills with self-loops that are not lifecycle, and legibility is load-bearing (DESIGN.md, Known limits). Rule 2 is the mitigation and must be designed into the renderer, not left to convention.
- **Same-state ambiguity.** If transitions could be requested by target state, an action would be indistinguishable from a no-op. Rule 1 is the mitigation and is a constraint on the API shape.
- **Event noise.** Subscribers who care only about lifecycle changes see attribute events too. Mitigated by the derived `changes_state` flag, which ADR-0034 made part of the subscription filter.
- **Nothing structural stops an action becoming a lifecycle change.** Editing the to-state of a self-transition turns it into a real transition. The change is visible in the declaration diff and invariant analysis re-runs, but the declaration shape does not forbid it. A distinct element would.
- **Statechart semantics differ.** In Harel statecharts a self-transition exits and re-enters the state, firing exit and entry actions. ObjectKeeper has no entry or exit actions (ADR-0007), so the difference is inert, but readers who know statecharts will expect it and the docs should say so.

### B. A distinct `actions` element on the object type

An `actions` list beside `transitions`: name, allowed-in states, inputs, guards, and an outcome restricted to attribute writes.

Pros:

- **Vocabulary is clean.** The lifecycle diagram shows only state changes; actions are listed per state by construction.
- **The restriction is structural.** An action's outcome cannot contain a state change because the declaration shape has no slot for one.
- **Filtering is structural.** Subscribers and the read path distinguish lifecycle events from attribute events without a derived flag.
- **Allowed-in as a set is natural**, rather than a feature added to from-state.

Cons:

- **Two mechanisms for everything.** Guards, verdicts, inputs, schema derivation, recording, idempotency, availability and invariant analysis are implemented, tested and documented twice, or generalised over an abstraction that is the self-transition by another name. ADR-0008's rule — the escape hatch must return the same shape as the built-ins — applies by analogy: two classes of operation is what makes a system non-uniform for every consumer.
- **Drift between the two.** A feature added to transitions (deferred guards, idempotency keys, a new remedy class) must be remembered for actions. The first consumer's soft-delete cascade beside its state machine is a live example of two mechanisms for adjacent concerns drifting apart, ending in a `force_transition` bypass.
- **Two availability lists to merge**, and either two tool shapes for agents or a merged shape with a kind flag — which is option A's derived flag, on the wrong side.
- **Invariant analysis needs a second path**, exactly the hazard ADR-0009 names.
- **Bigger language.**

## Consequences

- The API addresses transitions by name. `validate(object, intent)` in DESIGN.md already implies this.
- The transition declaration allows a from-state set and an any-non-terminal wildcard.
- The readable rule set renders self-transitions as actions under their state. The recorded event carries `changes_state`.
- DESIGN.md's Terminology row for **Action** is rewritten to "a transition whose from-state equals its to-state", and the model block notes it.
- ADR-0006's "written only via specific actions or transitions" becomes "written only via transitions", actions being one kind.
- The docs state explicitly that a self-transition here has no exit/entry semantics.

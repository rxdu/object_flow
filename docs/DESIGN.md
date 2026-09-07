# ObjectKeeper — Design

Status: early design. Nothing implemented. This document records the model as agreed so far; individual decisions and their rejected alternatives live in [`docs/adr/`](adr/), open questions in [`../TODO.md`](../TODO.md).

## Purpose

A reusable substrate for building business applications, where the rules about data are declared alongside the data rather than implemented in each consumer.

The objective is **trust under delegation**: being able to hand a system to people who are not supervised and to AI agents whose behaviour cannot be fully predicted, and know that nothing they do can put the data into a state that has to be cleaned up afterwards. Stated as a rule: **data only moves to a different state when the right trigger is emitted and every required condition is met.** Reduced setup time is the mechanism, not the goal — a version that halves the work of defining a data model but leaves one unguarded write path is a failure, not a partial success.

The threat model is **mistakes, not malice**. The actors are trusted but fallible: a person who skips a step, an agent that retries a timed-out request, a script that sets a flag directly. The design defends against those by construction. It does not defend against an actor who holds database credentials and intends harm; that is an access-control and deployment concern (see [Known limits](#known-limits)).

Genericity is a requirement from the start. The first consumer is the author's own operations platform (see [First consumer](#first-consumer)), but it is a first customer rather than a domain to specialise for: the design is judged by whether it makes building that application faster and safer, not by whether it looks general.

## Position in the stack

```text
agents · applications · human UI          consumers
─────────────────────────────────────
ObjectKeeper                              this project
─────────────────────────────────────
PostgreSQL / SQLite                       storage

siblings:  workflow orchestration (Temporal-shaped)
           external systems of record (e.g. Xero)
           agent frameworks
```

## Core properties

| Property | Meaning | Why it matters |
|---|---|---|
| **Mediated** | No path to the data except through ObjectKeeper. State changes pass through declared transitions and their guards; every other write is recorded with provenance (ADR-0006) | The peace-of-mind guarantee |
| **Declared** | What is allowed is data, inspectable at runtime — not code | UI and agent API stop being separate work |
| **Recorded** | Every change is attributed and reconstructable | Trust after the fact, not only while watching |

The distinction from an ORM is deliberate: an ORM abstracts *mechanism* (it hides SQL) and will faithfully execute whatever the caller asks. This layer abstracts *authority* — what may change, when, and by whom. A system can have an excellent ORM and no guarantees at all.

## Model

```text
ObjectType
  attributes
    controlled      referenced by guards; written only via transitions
    free            editable with permission; recorded, not gated
  invariants        type-level properties; the runtime enforces them at any
                    transition that could violate them, on either side
  state machine     exactly one per object type
    states
    transitions     addressed by name; from a state, a set of states, or
                    any non-terminal state
      inputs        arguments supplied by the caller
      guards        evaluated over current state + inputs
      outcome       the new state and the controlled-attribute writes; recorded
    actions         transitions whose from-state equals their to-state
                    (ADR-0016); not a separate element
  relationships
    composition     exclusive membership + lifetime bounded by the whole
    reference       everything else

Actor               whoever requests a transition or action; referenced by
                    guards. Shape undecided — see TODO.md, "Actors and authority".
```

### Terminology

Words that were used loosely in earlier drafts now have one meaning each.

| Term | Meaning |
|---|---|
| **Lifecycle** | The observable shape — the states an object passes through. What is seen. |
| **State machine** | The mechanism — states, transitions and the guards on them. What does it. |
| **Transition** | A named, guarded, recorded request to move an object from one state to another, addressed by name and never by target state. The only way state or controlled attributes change. |
| **Action** | A transition whose from-state equals its to-state (ADR-0016): it writes controlled attributes without changing lifecycle state. Declared, guarded, recorded and listed like any transition; rendered under its state in the readable rule set rather than as an arrow. A self-transition here has no exit or entry semantics, unlike a statechart. |
| **Guard** | A predicate over current state and transition inputs that must hold for a transition or action to proceed. Returns a structured verdict with a remedy class, never only a boolean. |
| **Invariant** | A type-level property the runtime enforces at every transition that could violate it (ADR-0009). |
| **Outcome** | What a transition or action writes and records: the new state, the controlled attributes it sets, and the event. Earlier drafts called this "effects"; renamed so the word is free for the next row. |
| **Effect** | Something caused outside ObjectKeeper — raising an invoice, sending a message. Out of scope (ADR-0007). Consumers cause effects by observing recorded events. |
| **Actor** | Whoever requests a transition or action: a person, an agent acting for a principal, a service. Referenced by guards; the model is open. |
| **Approval** | Listed as in scope but not yet defined. The working reading is a guard of the form "a prior transition was performed by an actor holding authority X", reported with remedy class `delegable`. Open in TODO.md. |
| **Available transitions** | The transitions whose guards are satisfied, or satisfiable with input, for a given object and actor now. Earlier drafts said "available actions". |

## Behaviour that falls out of the model

These are consequences of the structure above, not separately designed features.

- **Creation** is the transition from nothing into the initial state, carrying its own guards. **End of life** is the transition into a terminal state, gated on outstanding parts. Neither is a special case. Imported objects are the one exception: they enter mid-lifecycle through the override path, not through creation (see [Data import](#data-import)).
- **Validity is relative to intent.** The meaningful call is `validate(object, intent)`; `validate(object)` has no answer once requiredness attaches to transitions.
- **Availability is three-way**: available / available-with-input / blocked. Blocked carries a reason and a remedy class. Transitions and actions appear in one list, since an action is a transition.
- **A same-state request is not a no-op.** Because transitions are addressed by name, "move to `PREPARATION`" for an object already in `PREPARATION` names no transition and is rejected; nothing is recorded for a change that did not happen (ADR-0016).
- **A transition's parameter list is derived from its own guards**, so the agent-facing tool schema and the validation rules cannot drift apart — they are the same declaration read two ways.
- **Causal lineage.** Because an event-driven transition carries the source event's identifier as its idempotency key, history records not only *who* changed an object but *what caused* the change.
- **One rule, many projections.** A single guard declaration serves the storage constraint, the API validation, the UI's disabled-button reason, the error message, and the agent's tool description. The cost of a business rule today scales with the number of surfaces, not the number of rules; this collapses it.

### Remedy classes

An unsatisfied guard reports why it failed in a form a caller can act on:

| Class | Meaning | Caller's next move |
|---|---|---|
| `self-serviceable` | Satisfiable by a transition argument | Supply it |
| `delegable` | Another actor must act | Ask them |
| `temporal` | Only time will satisfy it | Come back later |
| `dependent` | Another object must change state | Work on that first |
| `unreachable-from-here` | Wrong state; another transition comes first | Take a different path |

## Events and delivery

ObjectKeeper initiates nothing — every state change happens because a caller asked for it, and the deciding of *when* belongs to the layer above (ADR-0012). Applications therefore learn what happened by reading what was recorded.

Every recorded transition writes its event to a durable, ordered log **inside the same transaction that records the state change**, so no registered subscriber can lose an event (ADR-0013):

```text
            recorded transition
                    │
         durable ordered log        written in the same transaction
                    │
        ┌───────────┴───────────┐
     pull                     push
  consumer holds          delivery worker
  a cursor                calls subscribers
```

Every recorded event carries `changes_state`, derived from whether the transition's from-state and to-state differ, so a subscriber can ask for lifecycle changes only and ignore actions (ADR-0016).

Pull-only deployments need no background process and stay library-shaped. Push delivery requires a worker draining the log, and is the point at which a deployment becomes a service.

The guarantee is **at-least-once delivery with idempotent handling** (ADR-0014). Exactly-once delivery is impossible across a process boundary; exactly-once *effect* comes from deduplicating on the event identifier in the same transaction as the side effect. For the common case — an event causing a transition on another object — transitions accept an idempotency key, so the deduplication happens centrally rather than in every application.

Acknowledgements exist for progress tracking, lag detection, safe pruning of the log, and backpressure. They are not a correctness mechanism for exactly-once.

## Scope boundaries

The store **decides and records**. It does not compute values and it does not cause external effects.

| In scope | Out of scope |
|---|---|
| Preconditions, permissions, approvals | Computation (`total = Σ items × rate`) |
| Required information per transition | External effects (raise an invoice in Xero) |
| Part-state conditions, thresholds | Selection logic (route to nearest engineer) |
| Type-level invariants | Long-running process orchestration |
| History, attribution | Human UI, agent framework |
| Event log and delivery to subscribers | Deciding *when* a transition should happen |

This split is not arbitrary. Preconditions and permissions have the same shape across every domain — "required field", "needs approval", "no open children" look alike whether the object is a deployment or a loan application. Computation and effects are exactly where domains differ irreducibly. The state machine generalises because it sits at the layer where domains resemble each other.

## First consumer

The first consumer is an extended version of the Weston Robot inventory management system: an operations platform covering procurement, allocation, pre-delivery inspection, leasing, and customer identity mirrored from Xero. The existing system is in production. Its repository sits beside this one at `~/RduWs/wr_inventory_management` and is the source of domain evidence for this design. It will be rebuilt on ObjectKeeper rather than extended in place, and its production data will be ported and preserved (ADR-0015).

The existing system already contains a hand-built version of ObjectKeeper's concerns — a centralised state-transition registry, a soft-delete cascade registry, fine-grained permissions with agent API keys, mandatory idempotency keys for agent writes, optimistic locking, and a unified audit log — and it has already met the failure modes the ADRs describe. Its own ADR-0002 documents transitions that were declared in the registry but reachable from no route. That record is the strongest evidence that the declaration must be the API (ADR-0010).

The Deployment, Site, Robot, Task, UAT and Deal names used in the ADRs are illustrative and predate this decision. They are not the first consumer's model. The real object types include Robot, Accessory, SparePart, Delivery, Service, WarrantyContract, ProductConfiguration, IntakeBatch, ProcurementOrder and ShippingRecord, with Engagement and Lease designed but not yet built.

Where the first consumer's evidence challenges an accepted decision, the challenge is recorded in TODO.md under "Challenges from the first consumer" and noted on the affected ADR. None has been resolved yet.

## Data import

Because the first consumer is a rebuild with ported data, import is a designed path rather than a script beside the store (ADR-0015). Every imported object arrives mid-lifecycle and none of its state passed through a guard, so import is the first use, at scale, of the authorised and recorded override that [Known limits](#known-limits) already requires for administrative repair.

Imported state is recorded with provenance `asserted`, distinct from state that ObjectKeeper observed a transition produce. The importer evaluates the declaration against every incoming object and reports each invariant the legacy data violates; it never admits a violation silently, and what to do with each class of violation is a decision taken by a person. Legacy history is preserved as read-only entries of kind `legacy` attached to the object. External identifiers that other systems reference survive the port. Files — photos, PDFs, templates — are production data too; ADR-0017 proposes carrying them as content-addressed references with the bytes in external storage, and is not yet decided. The design of the import path, and its relationship to schema evolution, is open in TODO.md.

## Known limits

The guarantee is narrower than it first sounds: the system can guarantee that **no state change bypassed the guards**. It cannot guarantee that **the guards say what was meant**. Risk moves from implementation bugs — scattered across four codebases, unreviewable — to specification gaps, which are in one place and can be read. That is a better position, but it is a relocation of risk, not an elimination of it.

Two consequences follow:

1. The rule set for an object type should be printable as a readable artifact, so that someone who knows the business process can check it. Legibility is as load-bearing for trust as enforcement. That artifact lists actions under the state they apply in, not as self-loops on the lifecycle diagram (ADR-0016).
2. Acceptance testing should be adversarial rather than happy-path: point an agent at the system and try to get it to produce an invalid state. "Can it complete the workflow" is the weaker question. Adversarial here follows the threat model — a fallible actor that guesses, retries and skips steps, trying every route — not a hostile one with credentials.

The mediated property is also an operational commitment, not only an architectural one. If anything else can reach the database — a migration script, an admin tool, a colleague with `psql`, a reporting job that "just" updates a flag — the guarantee is gone. Administrative repair needs a designed answer (an authorised, recorded override transition), because the unplanned 11pm database fix is how the history quietly becomes fiction. The data import of ADR-0015 is the first use of that path.

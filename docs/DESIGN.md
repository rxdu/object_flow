# ObjectKeeper — Design

Status: early design. Nothing implemented. This document records the model as agreed so far; individual decisions and their rejected alternatives live in [`docs/adr/`](adr/), open questions in [`../TODO.md`](../TODO.md).

## Purpose

A reusable substrate for building business applications, where the rules about data are declared alongside the data rather than implemented in each consumer.

The objective is **trust under delegation**: being able to hand a system to people who are not supervised and to AI agents whose behaviour cannot be fully predicted, and know that nothing they do can put the data into a state that has to be cleaned up afterwards. Reduced setup time is the mechanism, not the goal — a version that halves the work of defining a data model but leaves one unguarded write path is a failure, not a partial success.

Genericity is a requirement from the start. The first consumers are the author's own projects, but they are first customers rather than a domain to specialise for: the design is judged by whether it makes building those applications faster and safer, not by whether it looks general.

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
| **Mediated** | No path to the data except through defined transitions | The peace-of-mind guarantee |
| **Declared** | What is allowed is data, inspectable at runtime — not code | UI and agent API stop being separate work |
| **Recorded** | Every change is attributed and reconstructable | Trust after the fact, not only while watching |

The distinction from an ORM is deliberate: an ORM abstracts *mechanism* (it hides SQL) and will faithfully execute whatever the caller asks. This layer abstracts *authority* — what may change, when, and by whom. A system can have an excellent ORM and no guarantees at all.

## Model

```text
ObjectType
  attributes
    controlled      referenced by guards; written only via actions/transitions
    free            editable with permission; recorded, not gated
  invariants        type-level properties; the runtime enforces them at any
                    transition that could violate them, on either side
  state machine     exactly one per object type
    states
    transitions
      inputs        arguments supplied by the caller
      guards        evaluated over current state + inputs
      effects       what gets recorded
  relationships
    composition     exclusive membership + lifetime bounded by the whole
    reference       everything else
```

Terminology: the **lifecycle** is the observable shape — the states an object passes through. The **state machine** is the mechanism — states, transitions, and the guards on them. The lifecycle is what is seen; the machine is what does it.

## Behaviour that falls out of the model

These are consequences of the structure above, not separately designed features.

- **Creation** is the transition from nothing into the initial state, carrying its own guards. **End of life** is the transition into a terminal state, gated on outstanding parts. Neither is a special case.
- **Validity is relative to intent.** The meaningful call is `validate(object, intent)`; `validate(object)` has no answer once requiredness attaches to transitions.
- **Availability is three-way**: available / available-with-input / blocked. Blocked carries a reason and a remedy class.
- **A transition's parameter list is derived from its own guards**, so the agent-facing tool schema and the validation rules cannot drift apart — they are the same declaration read two ways.
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

## Scope boundaries

The store **decides and records**. It does not compute values and it does not cause external effects.

| In scope | Out of scope |
|---|---|
| Preconditions, permissions, approvals | Computation (`total = Σ items × rate`) |
| Required information per transition | External effects (raise an invoice in Xero) |
| Part-state conditions, thresholds | Selection logic (route to nearest engineer) |
| Type-level invariants | Long-running process orchestration |
| History, attribution | Human UI, agent framework |

This split is not arbitrary. Preconditions and permissions have the same shape across every domain — "required field", "needs approval", "no open children" look alike whether the object is a deployment or a loan application. Computation and effects are exactly where domains differ irreducibly. The state machine generalises because it sits at the layer where domains resemble each other.

## Known limits

The guarantee is narrower than it first sounds: the system can guarantee that **no state change bypassed the guards**. It cannot guarantee that **the guards say what was meant**. Risk moves from implementation bugs — scattered across four codebases, unreviewable — to specification gaps, which are in one place and can be read. That is a better position, but it is a relocation of risk, not an elimination of it.

Two consequences follow:

1. The rule set for an object type should be printable as a readable artifact, so that someone who knows the business process can check it. Legibility is as load-bearing for trust as enforcement.
2. Acceptance testing should be adversarial rather than happy-path: point an agent at the system and try to get it to produce an invalid state. "Can it complete the workflow" is the weaker question.

The mediated property is also an operational commitment, not only an architectural one. If anything else can reach the database — a migration script, an admin tool, a colleague with `psql`, a reporting job that "just" updates a flag — the guarantee is gone. Administrative repair needs a designed answer (an authorised, recorded override transition), because the unplanned 11pm database fix is how the history quietly becomes fiction.

## Examples in this document

Object names used illustratively throughout the ADRs — Deployment, Site, Robot, Task, UAT, Deal — are a strawman drawn from the project brief to exercise the model. They are not a description of any real domain model, and none of the specific gates shown have been confirmed against a real workflow.

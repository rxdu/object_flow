# ObjectKeeper — Design

Status: early design. Nothing implemented. This document records the model as agreed so far; individual decisions and their rejected alternatives live in [`docs/adr/`](adr/), open questions in [`../TODO.md`](../TODO.md). Decisions taken in autonomous design iterations are marked as such in the ADR index and listed in TODO.md under "Author review queue".

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
ObjectType            declared under a version (ADR-0027); may extend a base
                      declaration for attributes, relationships, invariants and
                      derived attributes (ADR-0026)
  id                store-assigned, globally unique, immutable, opaque (ADR-0018)
  version           incremented on every recorded change (ADR-0023)
  attributes
    controlled      referenced by guards; written only via transitions
      identifier    optionally minted from a named, scoped sequence (ADR-0029)
      personal      optionally marked; subject to erasure (ADR-0031)
    free            editable with permission; recorded, not gated
    derived         a named expression, never stored, evaluated on read (ADR-0021)
  visibility        optional predicate over actor and object; an invisible
                    object is not found (ADR-0030)
  invariants        type-level properties; the runtime enforces them at any
                    transition that could violate them, on either side
  state machine     exactly one per object type; a named declaration the
                    type binds whole (ADR-0026)
    states          each with a category; at least one terminal; deletion is
                    a terminal state (ADR-0024); a terminal state may be
                    superseding, naming a successor (ADR-0028)
    transitions     addressed by name; from a state, a set of states, or
                    any non-terminal state
      inputs        arguments supplied by the caller
      guards        evaluated over current state + inputs + actor + now
      outcome       own new state and controlled writes (attribute :=
                    expression), plus cascaded transitions and creations
                    across relationships, all in one transaction (ADR-0019)
      only via      optional: reachable only as a cascade from named
                    parent transitions (ADR-0020)
    actions         transitions whose from-state equals their to-state
                    (ADR-0016); not a separate element
  relationships   carry no attributes; a link with labels, dates or a
                    lifecycle of its own is a link object — a type with two
                    references
    composition     exclusive membership + lifetime bounded by the whole
    reference       everything else; inverses may be declared

Actor               supplied by the consumer with every request: id, kind
                    (human | agent | service), optional principal,
                    capabilities (ADR-0025). Referenced by guards as actor.*
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
| **Outcome** | What a transition or action writes and records: its new state, the controlled attributes it sets, any cascaded transitions and creations on related objects (ADR-0019), and the events. Earlier drafts called this "effects"; renamed so the word is free for the next row. |
| **Cascaded transition** | A transition on a related object declared as part of another transition's outcome. Gated by its own guards, committed in the same transaction, recorded as caused by the parent. |
| **Only via** | A declaration that a transition is not requestable and occurs only as a cascade from named parents (ADR-0020). |
| **Derived attribute** | A named expression on a type, never stored, evaluated on read; readable by guards (ADR-0021). |
| **Verdict** | The result of evaluating a request: satisfied; unsatisfied with a reason and remedy class; `stale` (ADR-0023); or not requestable (ADR-0020). |
| **Type family** | A base declaration and every type extending it; a query target (ADR-0026). |
| **State category** | A consumer-defined classification every state declares, such as open / in progress / done, so family-wide guards and views need not know state names (ADR-0026). |
| **Declaration version** | The version of a type's declaration; recorded on every object and event; removals need a mapping applied as recorded migrations (ADR-0027). |
| **Supersession** | Ending an object in a terminal state that names its successor, keeping id and history; how an object changes kind (ADR-0028). |
| **Sequence** | A named, scoped, monotonic counter the store maintains to mint business identifiers at creation (ADR-0029). |
| **Link object** | A type whose purpose is to relate two objects and carry attributes or a lifecycle about that relation — an association with labels, an engagement line with dates. References themselves carry nothing. |
| **Visibility** | A declared predicate over actor and object that every read applies; failing it means not found (ADR-0030). |
| **Erasure** | Redaction of declared personal attributes across an object and its history, recorded and irreversible; distinct from deletion (ADR-0031). |
| **Effect** | Something caused outside ObjectKeeper — raising an invoice, sending a message. Out of scope (ADR-0007). Consumers cause effects by observing recorded events. |
| **Actor** | Whoever requests a transition or action, as a descriptor the consumer's authentication supplies: id, kind, optional principal, capabilities (ADR-0025). ObjectKeeper does not authenticate. |
| **Approval** | Listed as in scope but not yet defined. The working reading is a guard of the form "a prior transition was performed by an actor holding authority X", reported with remedy class `delegable`. Open in TODO.md. |
| **Available transitions** | The transitions whose guards are satisfied, or satisfiable with input, for a given object and actor now. Earlier drafts said "available actions". |
| **Object id** | The identifier ObjectKeeper assigns to every object at creation or import: globally unique, immutable, opaque (ADR-0018). Every reference, event and external link holds it. |
| **Business identifier** | An identifier the consumer assigns with business meaning, such as an asset serial. A controlled attribute with a uniqueness invariant; never the object's identity. |
| **External identifier** | An identifier another system owns, such as a Xero contact id or a legacy primary key. A controlled attribute naming its source. |

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
| `unreachable-from-here` | Wrong state; another transition comes first, or this one is only-via | Take a different path |

A request can also be refused with `stale` — the object changed since the caller read it (ADR-0023) — whose remedy is to re-read and decide again.

## Transition execution

A request names an object, a transition, its inputs, the actor (ADR-0025), and optionally the version the caller last read, an idempotency key (ADR-0014), and a free-form `context` recorded on the event beside the actor. If the type declares visibility and the actor cannot see the object, the request is refused as not found before anything else (ADR-0030). It then executes in one transaction (ADR-0023):

1. If an `expected_version` is given and differs from the object's, refuse with `stale`.
2. Lock, in id order, every object the outcome will write: the target and every object reached by a cascaded transition or creation (ADR-0019).
3. Evaluate every guard — the parent's, then each cascaded transition's, depth-first in declaration order — over a consistent snapshot. Any failure blocks the whole request; the verdict names the object, transition and guard, with its remedy class. Only-via transitions contribute their non-actor guards (ADR-0020).
4. Check every invariant the written objects could violate (ADR-0009); those expressible as database constraints are enforced by the database. A violation verdict names the conflicting objects.
5. Write all outcomes; increment each written object's version; record one event per transition, cascaded ones carrying the parent's event as cause; write them to the log (ADR-0013). Commit.

Outcomes are straight-line: they may iterate a declared relationship with a filter, never choose between alternatives. Branching is expressed as separate transitions with distinguishing guards.

## Time

`now` is a value in the expression language (ADR-0021). A time-driven transition is an ordinary transition whose guard reads it — `expire: ACTIVE → EXPIRED, guard end_date <= now` — and ObjectKeeper never requests it (ADR-0012). The read surface answers, for a type and a transition, which objects have that transition available now; a scheduler above asks and requests (ADR-0022). The guard is the timing rule, in one place.

## Declarations: composition, versioning, supersession

A declaration is a versioned artefact (ADR-0027). Types share attributes, relationships, invariants and derived attributes by `extends`; state machines are named and bound whole, never inherited piecemeal (ADR-0026). Publishing a new version is recorded; additions apply forward; removing a state requires a mapping that is applied as recorded migration transitions, and a new invariant is checked against live objects and reported before it is enforced. Import is this mechanism from version zero (ADR-0015).

An object never changes type. When it must continue as something else — moved to another project, converted from a subtask, merged into a canonical record — it ends in a superseding terminal state that names its successor, and the read surface follows the pointer on request (ADR-0028).

## Deletion

A deletable type declares a terminal state and a transition into it, guarded on there being no live object referencing this one; parts are cascaded, references block with remedy class `dependent` (ADR-0024). Deleted objects stay readable by id and in history and take no further transitions. There is no bypass; repair is the recorded override of ADR-0001.

Erasure is different from deletion. Attributes may be declared personal, and a declared erasure transition replaces their values with a redaction marker on the object and in every event that carried them, deletes referenced file content, keeps the event skeleton, and records that it happened. It is the one place the log is rewritten, and it is irreversible (ADR-0031).

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
| Cascaded consequences declared on a transition (ADR-0019) | Effects outside the store |

This split is not arbitrary. Preconditions and permissions have the same shape across every domain — "required field", "needs approval", "no open children" look alike whether the object is a deployment or a loan application. Computation and effects are exactly where domains differ irreducibly. The state machine generalises because it sits at the layer where domains resemble each other.

## First consumer

The first consumer is an extended version of the Weston Robot inventory management system: an operations platform covering procurement, allocation, pre-delivery inspection, leasing, and customer identity mirrored from Xero. The existing system is in production. Its repository sits beside this one at `~/RduWs/wr_inventory_management` and is the source of domain evidence for this design. It will be rebuilt on ObjectKeeper rather than extended in place, and its production data will be ported and preserved (ADR-0015).

The existing system already contains a hand-built version of ObjectKeeper's concerns — a centralised state-transition registry, a soft-delete cascade registry, fine-grained permissions with agent API keys, mandatory idempotency keys for agent writes, optimistic locking, and a unified audit log — and it has already met the failure modes the ADRs describe. Its own ADR-0002 documents transitions that were declared in the registry but reachable from no route. That record is the strongest evidence that the declaration must be the API (ADR-0010).

The Deployment, Site, Robot, Task, UAT and Deal names used in the ADRs are illustrative and predate this decision. They are not the first consumer's model. The real object types include Robot, Accessory, SparePart, Delivery, Service, WarrantyContract, ProductConfiguration, IntakeBatch, ProcurementOrder and ShippingRecord, with Engagement and Lease designed but not yet built.

Where the first consumer's evidence challenges an accepted decision, the challenge is recorded in TODO.md under "Challenges from the first consumer" and noted on the affected ADR. A worked walkthrough of the unit and delivery lifecycles against this model, with proposed resolutions, is in [`design/first-consumer-walkthrough.md`](design/first-consumer-walkthrough.md) (draft). None has been resolved yet.

## Data import

Because the first consumer is a rebuild with ported data, import is a designed path rather than a script beside the store (ADR-0015). Every imported object arrives mid-lifecycle and none of its state passed through a guard, so import is the first use, at scale, of the authorised and recorded override that [Known limits](#known-limits) already requires for administrative repair.

Every imported object receives a new object id; its legacy primary key is kept as an external identifier so preserved history and surviving links still resolve, and cross-references are re-pointed through that mapping (ADR-0018). Imported state is recorded with provenance `asserted`, distinct from state that ObjectKeeper observed a transition produce. The importer evaluates the declaration against every incoming object and reports each invariant the legacy data violates; it never admits a violation silently, and what to do with each class of violation is a decision taken by a person. Legacy history is preserved as read-only entries of kind `legacy` attached to the object. External identifiers that other systems reference survive the port. Files — photos, PDFs, templates — are production data too; ADR-0017 proposes carrying them as content-addressed references with the bytes in external storage, and is not yet decided. The design of the import path, and its relationship to schema evolution, is open in TODO.md.

## Known limits

The guarantee is narrower than it first sounds: the system can guarantee that **no state change bypassed the guards**. It cannot guarantee that **the guards say what was meant**. Risk moves from implementation bugs — scattered across four codebases, unreviewable — to specification gaps, which are in one place and can be read. That is a better position, but it is a relocation of risk, not an elimination of it.

Two consequences follow:

1. The rule set for an object type should be printable as a readable artifact, so that someone who knows the business process can check it. Legibility is as load-bearing for trust as enforcement. That artifact lists actions under the state they apply in, not as self-loops on the lifecycle diagram (ADR-0016).
2. Acceptance testing should be adversarial rather than happy-path: point an agent at the system and try to get it to produce an invalid state. "Can it complete the workflow" is the weaker question. Adversarial here follows the threat model — a fallible actor that guesses, retries and skips steps, trying every route — not a hostile one with credentials.

The mediated property is also an operational commitment, not only an architectural one. If anything else can reach the database — a migration script, an admin tool, a colleague with `psql`, a reporting job that "just" updates a flag — the guarantee is gone. Administrative repair needs a designed answer (an authorised, recorded override transition), because the unplanned 11pm database fix is how the history quietly becomes fiction. The data import of ADR-0015 is the first use of that path.

# ObjectKeeper — Design

**Status: under review.** The model is settled and 46 defects found by an implementation-readiness review have been resolved; every resolution is recorded in [`design/defects.md`](design/defects.md) and marked pending author review in [`../TODO.md`](../TODO.md). Nothing is implemented, and the artefacts an implementation needs — a declaration syntax, a storage schema, a library API — do not exist yet.

This document is the single description of the model. Each decision, with the alternatives it rejected, is an ADR in [`adr/`](adr/); the case studies that shaped it and the catalogue of what it deliberately does not cover are in [`design/`](design/).

## 1. Purpose

A reusable substrate for building business applications, where the rules about data are declared alongside the data rather than implemented in each consumer.

The objective is **trust under delegation**: hand a system to people who are not supervised and to AI agents whose behaviour cannot be fully predicted, and know that nothing they do can put the data into a state that has to be cleaned up afterwards. As a rule: **an object's state changes only when someone requests a named transition and every condition on it is met.**

The threat model is **mistakes, not malice**. Actors are trusted but fallible: a person who skips a step, an agent that retries a timed-out request, a script that sets a flag directly. The design defends against those by construction. It does not defend against an actor holding database credentials who intends harm; that is an access-control and deployment concern (§13).

Genericity is a requirement from the start. The first consumer is the author's own operations platform (§4); it is a first customer, not a domain to specialise for.

## 2. Position in the stack and deployment shapes

```text
agents · applications · human UI          consumers
─────────────────────────────────────
ObjectKeeper                              this project
─────────────────────────────────────
PostgreSQL / SQLite                       storage

siblings:  workflow orchestration (Temporal-shaped)
           external systems of record (e.g. Xero)
           agent frameworks
           file storage (S3-compatible or filesystem)
```

ObjectKeeper is a layer over a database, not a database (ADR-0001). The core is **library-shaped**: a call goes in, guards evaluate, a transition and its record come out; there is no scheduler, timer or background process (ADR-0012). A deployment becomes a **service** when it adds a delivery worker for push subscriptions (ADR-0013) or exposes the API over a transport (ADR-0037).

## 3. Core properties

| Property | Meaning | Why it matters |
|---|---|---|
| **Mediated** | No path to the data except through declared transitions and their guards. There is no second write path (ADR-0042), and the one way to set state without satisfying a guard is a declared, recorded assertion (ADR-0040) | The peace-of-mind guarantee |
| **Declared** | What is allowed is data, inspectable at runtime, not code (ADR-0010) | UI, API and agent tools are projections of one declaration |
| **Recorded** | Every change is attributed, caused and reconstructable | Trust after the fact, not only while watching |

An ORM abstracts *mechanism*: it hides SQL and faithfully executes whatever the caller asks. This layer abstracts *authority*: what may change, when, by whom, and what follows.

## 4. First consumer and case studies

The first consumer is an extended version of the Weston Robot inventory management system: an operations platform covering procurement, allocation, pre-delivery inspection, leasing and customer identity mirrored from Xero. The existing system is in production at `~/RduWs/wr_inventory_management`, already contains a hand-built version of every concern below, and will be rebuilt on ObjectKeeper with its production data ported and preserved (ADR-0015).

The model was tested against four further shapes, each recorded with what it forced: issue tracking, customer records, orders at volume, and approvals with bookings, in [`design/`](design/). The worked walkthrough of the first consumer's own lifecycles is [`design/first-consumer-walkthrough.md`](design/first-consumer-walkthrough.md).

## 5. The model

```text
ObjectType            declared under a version (§5.9); may extend a base;
                      declares a tracking mode, serial or quantity (§5.10)
  id                store-assigned, globally unique, immutable, opaque
  version           incremented on every recorded change
  attributes        written only by transition outcomes; each has a declared
                    type, and may be marked identifier, external, personal,
                    indexed, or part of the summary set (§5.2)
  derived           named expressions, never stored, evaluated on read (§5.2)
  visibility        optional predicate over actor and object (§5.8)
  invariants        type-level properties, enforced dynamically (§5.6)
  relationships     composition or reference; carry no attributes (§5.3)
  state machine     exactly one, a named declaration bound whole (§5.4)
    states          each with a category; at least one terminal
    transitions     named; declare inputs, guards and an outcome (§5.4)
                    optional markings: only via · proposable · asserting
    actions         transitions whose from-state equals their to-state
```

Every request additionally carries an **actor** the consumer supplies (§5.8).

### 5.1 Objects and identity

Every object carries a store-assigned, globally unique, immutable, opaque id, assigned at creation and at import (ADR-0018). Business identifiers such as a serial or a ticket key, and external identifiers such as a Xero contact id or a legacy primary key, are ordinary attributes, never identity. A business identifier may be minted at creation from a named, scoped, monotonic sequence, which is monotonic but not gapless (ADR-0029). An object never changes type; when it must continue as something else it is superseded (§8).

### 5.2 Attributes

An attribute has a declared type: string, boolean, integer, decimal with scale, money with currency, timestamp, duration, enum with declared options, reference, event reference, file, or a set of any of these.

**Every attribute is written only by a transition outcome** (ADR-0042). There is no ungated write. A type makes attributes editable by declaring an action for them, which costs one declaration and gives the edit a guard, an actor and an event. Attributes may additionally be marked:

- **identifier**, minted from a sequence (ADR-0029);
- **external**, naming the system that owns the value, with uniqueness per source automatic (ADR-0037);
- **personal**, subject to erasure (§8). A personal value may not be written into a non-personal attribute, and publishing rejects a declaration that does (ADR-0051);
- **indexed**, which is what query filters, type-scan predicates and visibility predicates may use (ADR-0048);
- part of the **summary** set, the attributes a listing returns by default.

**Derived** attributes are named expressions, never stored, evaluated on read; their dependency graph must be acyclic (ADR-0021, ADR-0047). A **file** attribute holds a content-addressed reference to a blob in external storage with its hash, size, media type and provenance. ObjectKeeper never reads, streams or serves the bytes; its one byte-level operation is deleting them during erasure (ADR-0017, ADR-0041). A deployment must disable its blob store's own lifecycle expiry, since the log is permanent and a reference outlives any expiry policy.

### 5.3 Relationships

Relationships are **composition**, meaning exclusive membership and a lifetime bounded by the whole, or **reference**, meaning everything else (ADR-0003). A composite's state is its own, gated by guards that read its parts and never derived from them (ADR-0004); "why is this stuck" is then answered by a `dependent` verdict naming the parts. A composition part may be re-parented by a transition that writes its owner, which is what makes splitting an object expressible (ADR-0046).

References carry no attributes. A relation with labels, dates or a lifecycle of its own is a **link object**: a type with two references. **One end of a relationship is stored and its declared inverse is a derived view over it** (ADR-0056), so there is only ever one write and one event; guards, invariants and outcomes traverse either direction. A composition declares the child transition its whole's deletion drives, and that cascade's bound.

### 5.4 State machines, transitions and outcomes

Each type binds exactly one **named** state machine, whole; two types that share a lifecycle bind the same machine (ADR-0003, ADR-0026). Every state has a **category** from a consumer-defined set, so guards and views that span a family need not know state names, and at least one state is terminal.

A **transition** is a named, guarded, recorded request to move an object from one state to another. It is requested **by name, never by target state**. Its from-state may be one state, a set, or any non-terminal state. It declares **inputs** with types and optionality, **guards**, and an **outcome**. An **action** is a transition whose from- and to-state are equal; its outcome is unrestricted like any other (ADR-0016, ADR-0041). A request naming no declared transition is refused, so nothing is recorded for a change that did not happen.

An **outcome** is an ordered sequence of steps (ADR-0046, ADR-0052):

```text
  <attribute> := <expression>                        write on this object
  add    <attribute> := <expression>                 insert into a set-valued attribute
  remove <attribute> := <expression>                 delete from a set-valued attribute
  supersede <expression>                             name this object's successor
  let <name> = create <Type>.<transition>(…)         create, bound for later steps
  <path>.<transition>(<input> := <expression>, …)    cascade, with inputs
  for <name> in <collection expression> [where …]:   iterate, binding the element
  for <name> in 1..<expression>:                     repeat over a count
```

Rules that make an outcome readable and safe:

- **Straight-line.** No step chooses between alternatives. Where behaviour differs by a value, declare one transition per case, each guarded on that value, so the applicable one appears in the availability listing and a value matching none is visibly stuck.
- **Sets change element-wise.** `add` and `remove` are read-modify-write like any outcome write, so two adds in one request accumulate and two concurrent adds serialise. The expression language gains no set algebra; `set` on a set-valued attribute still replaces it wholly (ADR-0055).
- **Optional inputs skip.** A write whose right-hand side is an unsupplied optional input is skipped, not applied. Clearing a value deliberately is a separate input or a separate action (ADR-0052).
- **`this` and `this_event` are available**, including inside a creation outcome: the new object's id and its event's identity are allocated before the outcome runs (ADR-0046, ADR-0052).
- **Bounded.** A declaration may cap iteration fan-out; exceeding it is refused with `over-limit`. The graph of transitions that reference each other in outcomes must be acyclic (ADR-0019).

A transition may be marked **only via** named parent transitions, meaning it is not requestable and carries no actor guards of its own (ADR-0020); **proposable**, meaning a caller lacking authority may file a Proposal (§9); or **asserting** (§8).

Creation is a transition from nothing into an initial state, carrying its own guards. A type with several creation transitions is named explicitly at the creation site. Requiredness attaches to transitions, so `validate(object)` has no answer and `check(object, transition, inputs)` does (ADR-0002, ADR-0005).

### 5.5 Guards, verdicts and remedy classes

A guard is an expression that must hold for a transition to proceed. Each guard clause **declares its remedy class**, inferred where the shape is unambiguous (ADR-0047). A transition **declares its inputs**; publishing rejects a guard naming an undeclared input and warns on an input nothing uses, so the tool schema and the validation rules cannot drift, because they are one declaration.

Evaluating a request yields one **verdict**:

| Verdict | Meaning |
|---|---|
| satisfied | The request proceeds |
| unsatisfied | A guard failed; carries the failing clause, its object and a remedy class |
| `stale` | The caller's `expected_version` is out of date, or the transaction exhausted its serialisation retries (ADR-0023, ADR-0039) |
| `not found` | Visibility hides the object; existence is not disclosed (ADR-0030) |
| `not requestable` | The transition is only via named parents, which are named in the verdict (ADR-0020, ADR-0041) |
| `over-limit` | A cascade exceeded a declared fan-out cap, which is named (ADR-0041) |
| invariant violated | Names the invariant and the conflicting objects |

An unsatisfied verdict's **remedy class** tells a caller what to do next:

| Remedy class | Meaning | Caller's next move |
|---|---|---|
| `self_serviceable` | Satisfiable by a transition argument | Supply it |
| `delegable` | Another actor must act; may name a capability and, if proposable, offer a Proposal | Ask them, or propose |
| `temporal` | Only time will satisfy it | Come back later |
| `dependent` | Another object must change state; names it | Work on that first |
| `unreachable_from_here` | Wrong state; another transition comes first | Take a different path |

Availability is therefore three-way for a listing: available, available-with-input naming what must be supplied, or blocked with a verdict.

**Approval** is not a separate mechanism. An approval is a recorded part of the approved object, carrying the approver, a kind, a decision and the event that recorded it, created by an `approve` action. "Needs approval" is then an ordinary guard counting the approvals that are still valid, where validity is `not changed_since([…], a.event)`, so a material edit invalidates an approval without every editing action having to remember a reset. N-of-M, sequential chains, thresholds and separation of duties are guards of that shape, and a missing approval reports `delegable` (ADR-0035).

A guard that depends on facts outside the store references a **named external evaluator**, which must return the same verdict shape (ADR-0008). Evaluators are consulted **before** the write transaction opens, never inside it, and their verdict carries an as-of time recorded on the event; a declaration may set a freshness bound, past which the verdict is refused as `temporal` (ADR-0049). The guarantee for an external fact is as of that time, not at commit.

### 5.6 Invariants

A type-level property, declared once against the type rather than restated on every transition that could break it (ADR-0009). **Enforcement is dynamic**: after a request applies its outcomes, every invariant the written objects could violate is checked (ADR-0045). Three forms are permitted, and each has a rule that makes the affected set computable (ADR-0055):

- a **local** invariant reads only the object's own attributes and state, so the affected set is the object just written. This is the commonest and cheapest form;
- a **traversal** invariant reads related objects, and may traverse only relationships with **declared inverses**, so the affected objects are found by reverse traversal;
- a **type-scan** invariant reads other objects of a type and must be **symmetric**, so the predicate that finds a conflict from a written object is the same one that would find it from the other side (ADR-0052). Overlap and equality on a shared key are symmetric; the booking-overlap and one-active-version invariants are of this form.

Publishing classifies each invariant, reports which form it decided and which transitions could violate it, and rejects anything that fits none. Correctness comes from serialisable isolation (ADR-0039), so an invariant is enforced whether or not it compiles to a database constraint. Compilation is an optimisation whose available shapes depend on the backend, and publishing reports which of a declaration's invariants the configured backend can compile (ADR-0041).

### 5.7 The expression language

One language serves guards, invariants, derived attributes, visibility predicates, outcome values and filters (ADR-0021, ADR-0032, ADR-0047, ADR-0053):

| Construct | Example |
|---|---|
| literals; attribute paths across relationships; `this`, `this_event`, `inputs.*`, `actor.*`, `now` | `binding.delivery.state`, `actor.has(DELIVERY_COMPLETE)` |
| comparison, membership; boolean logic and implication | `reason in CancellationReason`, `required implies count(p in photos) >= 1` |
| `is null`, `is not null` — definite presence tests, never unknown | `s.unit is not null` |
| `count`, `all`, `any`, `none`, `sum`, `min`, `max` over a relationship or a type, binding the element | `none(s in Service where s.unit == this and s.state != CANCELLED)`, `sum(l in lines: l.qty * l.unit_price)` |
| arithmetic on numbers; durations, and `+ -` with timestamps | `on_hand - reserved >= inputs.qty`, `placed_at + 30 min <= now` |
| `changed_since(attributes, event)` over the object's history | `not changed_since([amount, vendor], a.event)` |
| conditional expression, in derived attributes only | `if unit is null then UNFILLED else FILLED` |
| a declared external evaluator | `xero.invoice_valid(order_id)` |

Evaluation is **three-valued**: comparison with an absent value, and division by zero, yield unknown; unknown propagates; and a guard that evaluates to unknown **fails**, naming the clause. Presence is therefore tested with `is null` and `is not null`, and comparing against a bare `null` literal is a publish error (ADR-0053). Over an empty set, `count` and `sum` are zero, `all` and `none` are true, `any` is false, and `min` and `max` are unknown. `this` always means the object the expression is declared on and never rebinds, so every aggregate binds its element explicitly.

Not in the language: grouped aggregation, string operations beyond equality and membership, user-defined functions, recursion or transitive closure, any call other than a declared evaluator.

**The line with computation** (ADR-0007, ADR-0032): the store evaluates **declared arithmetic over its own data**, where the result is a guard, a view, an invariant or a quantity an outcome writes. It does not own **domain formulas** — tax, pricing, discounts, scoring, conversion — which consumers compute and supply as inputs for guards to check. The language grows only by an ADR naming the case that forced it.

### 5.8 Actors and visibility

Every request carries an **actor descriptor** the consumer's authentication produces: id, kind (human, agent or service), optional principal, a set of opaque capabilities, and optional attributes (ADR-0025). ObjectKeeper validates its shape, not its truth. Guards read it, and every event records it. Delegation and attenuation arrive in the descriptor; a consumer that must govern delegations models them as objects (ADR-0036). Where a guard needs a referenced person, that person is an ordinary object.

A type may declare a **visibility** predicate over actor and object; every read, query, availability result and delivered event applies it, and an invisible object is **not found**, never blocked, so a verdict never discloses existence (ADR-0030). A cascaded transition is not subject to visibility: the parent's guards are its authority.

### 5.9 Declarations: composition and versioning

A type may **extend** a base declaration for attributes, relationships, invariants and derived attributes; state machines are bound whole and never inherited piecemeal. A **family** is a base and everything extending it, and is a query target (ADR-0026).

Declarations are **versioned**, and every object and event records the version of its **type**. Because a type's behaviour depends on the machine, enums, sequences, evaluators and base types it uses, advancing any of those requires advancing every dependent type, which publishing enforces (ADR-0027, ADR-0056). A machine declares what it requires of its binders, and publishing verifies each one. Publishing is a designed operation with a report:

- **additions apply forward**; a new guard bites at the next transition;
- a **new invariant** is checked against live objects and every violation reported, then resolved by a mapping or admitted (§8);
- **removing a state** requires a mapping, applied as recorded migration transitions with provenance `migrated`;
- pending **proposals** the new version cannot express are invalidated (§9);
- the report also names which invariants compile on this backend, which transitions are sweepable, and which derived attributes are queryable (§10).

Import is this mechanism from version zero (§11). The printable rule set is per version.

### 5.10 Tracking mode

A type declares whether it is **serial-tracked**, meaning one object per physical thing with its own lifecycle, or **quantity-tracked**, meaning a stock object carrying counters that actions adjust (ADR-0050). A slot in a configuration declares which fill form it takes, so one kit may carry a serialised robot and a counted quantity of consumables. Reserve means the same in both: the thing is spoken for. Changing a type's mode is a new type, and objects move by supersession one at a time.

## 6. Transition execution

A request names an object, a transition, its inputs and the actor, and optionally the version the caller last read, an idempotency key (ADR-0014), and a free-form `context` string naming the route it came by, recorded on the event. A creation request names the type and the creation transition instead of an object, and skips steps 1 and 2.

External evaluators named by any guard in the request are consulted **first, outside the transaction**, and their verdicts carried in with their as-of times (ADR-0049). The rest runs in one transaction at **serialisable isolation** (ADR-0039):

1. If the type declares visibility and the actor cannot see the object, refuse as `not found`.
2. If an `expected_version` is given and differs, refuse with `stale`.
3. If the idempotency key has been applied before, return the original result, marked as a replay (ADR-0041).
4. Evaluate the parent transition's guards. A failure refuses the request, naming the clause, its object and its remedy class.
5. Apply the parent's outcome in full: its new state and its attribute writes, each value read at the moment it is applied (ADR-0054).
6. Then, depth-first in declaration order and over collection elements in ascending object-id order, take each cascaded transition or creation in turn: evaluate its guards **against the state produced so far**, then apply its outcome immediately (ADR-0038). Only-via transitions contribute their non-actor guards; other cascades run as the requesting actor. Any failure aborts the whole request and rolls back.
7. Check every invariant the written objects could violate, except those with an admitted violation still standing (ADR-0045, ADR-0054).
8. Increment each written object's version; record one event per transition, each cascaded event carrying the parent's event as its cause; append them to the log in the same transaction (ADR-0013). Commit.

Row locks are taken as objects are reached, for contention rather than correctness: serialisable isolation is what makes a guard's reads safe, including reads of objects the request never writes. A serialisation failure is retried to a declared bound and then refused with `stale`.

An **asserting** transition (§8) differs in two ways: step 4 evaluates only its own guards, which are its capability and reason requirements rather than the type's; and step 7 refuses on any invariant violation the request did not explicitly admit.

## 7. History, events and delivery

**Current state is stored**, one row per object with its attributes, state, version, declaration version and last event. **The event log is the permanent history**, written in the transition's transaction; a fold of the log reproduces the row, never the reverse (ADR-0033). A per-attribute index of the event that last wrote it makes `changed_since` a constant-time comparison (ADR-0048).

Every event carries `changes_state`, true when from and to differ, and its **provenance**: actor, principal, `context`, cause, declaration version, and a `source` of

| Source | Produced by |
|---|---|
| `observed` | An ordinary transition |
| `asserted` | An assertion, including import (§8, §11) |
| `migrated` | A declaration migration (§5.9) |
| `corrected` | An action that records a corrected value for an earlier mistake, with a reason; the earlier event is not rewritten |
| `erased` | An erasure (§8) |

The log is **never pruned**; retention is archival tiering that keeps events readable and reachable by erasure. Events are strictly ordered per object and causally ordered across a cascade; a global position serves cursors and is monotonic but not a commit order, and the pull interface states the window a cursor must tolerate (ADR-0034).

```text
            recorded transition
                    │
         permanent ordered log        written in the same transaction
                    │
        ┌───────────┴───────────┐
     pull                     push
  consumer reads with      delivery worker posts to
  a position               a subscription's endpoint
```

A **Subscription** is a built-in object holding a filter over a type or family, transition names and `changes_state`; a delivery endpoint the worker posts to; lag thresholds; and a lifecycle of `active → revoked`. Delivery progress is **runtime state, not object state**: the acknowledged position is stored beside the idempotency record, is not versioned and emits no events, so a poll never lands in the permanent log. Lag and death are **derived** from that position and the thresholds, so nothing has to move a subscription into them, and ADR-0012 is untouched. A subscription may be revived at any position, because nothing is pruned (ADR-0034, ADR-0043).

Delivery is **at-least-once with idempotent handling**. Exactly-once *effect* comes from deduplicating on the event id at the receiver, and transitions accept an idempotency key so the common case, an event causing a transition, deduplicates centrally and records causal lineage (ADR-0014). A subscriber acts as an actor, and visibility applies to what it is delivered.

**Time.** `now` is a value in guards. A time-driven transition is an ordinary one, `expire: ACTIVE → EXPIRED, guard end_date <= now`, and ObjectKeeper never requests it. A scheduler above asks the availability query and requests each answer with a periodic idempotency key (ADR-0022). The guard is the timing rule, in one place. ObjectKeeper initiates nothing (ADR-0012).

## 8. Ends of life, repair and erasure

**Deletion** is a terminal transition gated on there being no live object referencing this one; parts cascade, references block with `dependent` naming them. Deleted objects stay readable by id and in history and take no further transitions (ADR-0024).

**Supersession** ends an object in a terminal state that names its successor, which may be an existing object or one the same outcome created. Id and history stay, the read surface follows the pointer on request, and references are re-pointed only by declared cascade. It is how an object changes kind: moved, converted, merged (ADR-0028, ADR-0046). A duplicate is closed with a reference, not superseded.

**Assertion** is the only way to set state without satisfying the type's guards, and it is declared, not ambient (ADR-0040). An asserting transition names the states it may assert; is addressed by name with the target state as a constrained input, so §5.4's by-name rule holds; carries its own guards, at minimum a capability and a mandatory reason; and records what it stepped over. A type that declares none cannot be repaired this way at all. Invariants are not bypassed by default: an assertion marked `may_admit` accepts an explicit list of the invariants it knowingly violates, and each **admission** names an object and an invariant, suppresses that invariant's check for that object, and is discharged automatically when the invariant holds again (ADR-0054). Import and declaration migration use a **built-in** assertion gated on a deployment capability, so no type is unimportable by omission.

**Erasure** replaces the values of declared personal attributes with absence, on the object and in every event that carried them, deletes referenced file content, keeps the event skeleton, records that it happened, and is irreversible (ADR-0031). It runs from any state, terminal included, since erasure requests arrive for closed accounts; it is the one carve-out from the rule that a deleted object admits no further transitions (ADR-0056). The marker is absence rather than a sentinel, so it collides with no unique constraint and reads as unknown in the expression language, which makes a guard over an erased value fail rather than pass (ADR-0051). It is the one place the log is rewritten, and it is not deletion.

## 9. Built-in types

`Subscription` (§7) and `Proposal` are built in and not user-definable.

A **Proposal** holds a request a caller lacked authority for: target, transition, inputs and proposer, with a lifecycle `pending → executed | rejected | withdrawn | invalidated`. An actor holding the capability the target transition requires may approve it, which executes the recorded request **as that actor**, with the approval as cause. Every guard is re-evaluated at execution under the **current** declaration, including the actor guards, which the approver satisfies. A guard that fails leaves the proposal pending with the verdict attached; a declaration that can no longer express the request invalidates it (ADR-0036, ADR-0044). Expiry is derived, not driven.

## 10. The read surface

One API. Every operation takes an actor and applies visibility (ADR-0037).

| Operation | Returns |
|---|---|
| **get**(id, follow?) | The object; optionally following supersession to the live successor |
| **query**(type or family, filter, order, cursor, fields) | A page of objects. The filter uses indexed attributes, state and category, and derived attributes that are themselves indexable; `fields` defaults to the type's summary set |
| **lookup**(source, value) | The object whose external identifier for that source matches |
| **availability**(id) | Each requestable transition with its three-way availability, verdict and declared input schema. Only-via transitions are not listed |
| **available**(type, transition, cursor) | The objects for which that transition is available now. Requires the transition to be **sweepable**, meaning its guards decompose into an indexable prefilter and a residual; one that is not is refused rather than scanned (ADR-0048) |
| **check**(id, transition, inputs) | The verdict a request would receive, simulating the same sequence internally. Lock-free and side-effect-free, so it is advice and not a reservation, and its verdict is **partial**: it names the external guards it did not evaluate (ADR-0054) |
| **history**(id, follow?) | Events with provenance, legacy entries, and optionally the combined timeline through supersession |
| **exceptions**(type) | The objects holding asserted state or an admitted invariant violation, which is what keeps the escape hatch of §8 reviewable |
| **declaration**(type, version?) | The inspectable rule set, rendered also as readable text and as agent tool schemas |
| **pull**(subscription, position) | A page of events per the subscription's filter |
| **batch**(requests) | N independent requests, **each in its own transaction**, returning N verdicts in order |

Neither `available`, `check` nor `availability` calls an external evaluator; each says which guards it did not evaluate. Time-dependent derived attributes are queried by filtering the stored operand the expression compares against `now`, since a clock-dependent value cannot be indexed. There is **no atomic batch**: atomic multi-object semantics are declared cascades (§5.4). Beyond the indexes a type declares, the store maintains no projection; where a scan is too slow, the type declares a counter and the cascade that maintains it.

Writes go through the request of §6. Operational calls that are not object operations are `acknowledge`, which advances a subscription's progress (§7), and `publish`, which installs a declaration version (§5.9).

## 11. Import and migration

The first consumer's production data is ported by a designed path (ADR-0015). Every imported object receives a new id and keeps its legacy key as an external identifier with source `legacy`; cross-references are re-pointed through that mapping (ADR-0018). Imported state is written by the built-in assertion with provenance `asserted` (§8). The importer evaluates the declaration and reports every violated invariant and unsatisfied structural guard, and a person decides each class, either cleaning upstream or admitting it. Legacy history is preserved as read-only entries of kind `legacy`, which carry their original payload and are returned by `history` alongside ObjectKeeper's own events. Soft-deleted rows land in the type's deleted state. Files are content-addressed into the deployment's blob store. Cutover cannot be dual-write, because there is one write path.

## 12. Scope boundaries

| In scope | Out of scope |
|---|---|
| Preconditions, permissions, approvals, visibility | Domain formulas: tax, pricing, scoring, conversion |
| Required information per transition | External effects: raising an invoice, sending mail |
| Cross-object conditions and declared cascades | Selection: which engineer, which unit, who is next |
| Type-level invariants, derived views, declared arithmetic | Long-running process orchestration |
| History, provenance, the event log, delivery | Deciding *when* a transition should happen |
| Import, migration, supersession, erasure | Human UI, agent framework, blob bytes |

Preconditions and permissions have the same shape across domains; formulas, effects and selection are where domains differ irreducibly. The store decides and records; consumers compute, choose and cause (ADR-0007).

## 13. Known limits

The guarantee is that **no state change bypasses the guards except through a declared, capability-gated, recorded assertion**, and that the objects holding asserted state or admitted violations are queryable at any time (§8, §10). It cannot guarantee that **the guards say what was meant**. Risk relocates from scattered implementation bugs to specification gaps in one readable place. Hence two obligations: the rule set for a type must be printable for review by someone who knows the process, and acceptance testing must be adversarial in the threat model's sense, a fallible actor that guesses, retries and skips steps, trying every route to an invalid state.

The mediated property is an operational commitment. Anything else that can reach the database, a migration script, `psql`, a reporting job, voids it. Repair is a declared assertion, and the data import is its first use at scale.

Concrete cases the model does not cover, or covers with a caveat, are catalogued in [`design/edge-cases.md`](design/edge-cases.md). They include in-place type change, gapless sequences, hot-row throughput, multi-region stock, acyclicity of self-referential hierarchies, unmerge, local-time rules, and the several things a declaration is refused at publish for.

## 14. Terminology

| Term | Meaning |
|---|---|
| **Lifecycle** | The observable shape: the states an object passes through. |
| **State machine** | The mechanism: states, transitions and guards, as a named declaration a type binds. |
| **State category** | A consumer-defined classification every state declares, so family-wide guards need not know state names. |
| **Transition** | A named, guarded, recorded request to move an object between states; addressed by name, never by target state. |
| **Action** | A transition whose from- and to-state are equal. Its outcome is unrestricted. It has no exit or entry semantics, unlike a statechart self-transition. |
| **Outcome** | The ordered sequence of steps a transition applies: attribute writes, creations, cascaded transitions, and iteration. |
| **Cascaded transition** | A transition on a related object declared in another's outcome; gated by its own guards, applied in the same transaction, recorded as caused by the parent. |
| **Only via** | A transition reachable only as a cascade from named parents, and never listed. |
| **Proposable** | A transition a caller lacking authority may file a Proposal for. |
| **Asserting** | A transition that may set a declared set of states without satisfying the type's other guards, gated on a capability and recording what it stepped over. |
| **Guard** | An expression that must hold for a transition, carrying a declared remedy class. |
| **Approval** | A recorded part of the approved object; "needs approval" is a guard counting approvals still valid under `changed_since` (ADR-0035). |
| **Verdict** | The result of evaluating a request: satisfied, unsatisfied with a remedy class, `stale`, `not found`, `not requestable`, `over-limit`, or invariant violated. |
| **Remedy class** | What an unsatisfied guard tells the caller to do: supply, ask, wait, work elsewhere, or take another path. |
| **Invariant** | A type-level property, checked after every request against the objects it wrote. |
| **Admission** | A recorded decision to tolerate one invariant violation on one object, discharged when the invariant holds again. |
| **Derived attribute** | A named expression, never stored, evaluated on read. |
| **Expression language** | The one language of guards, invariants, derived attributes, visibility, outcome values and filters. |
| **External evaluator** | A named consultant for facts the store does not hold, returning the same verdict shape, consulted before the transaction with an as-of time. |
| **Composition / reference** | Exclusive membership with a bounded lifetime, versus every other relationship. |
| **Link object** | A type that relates two objects and carries attributes or a lifecycle about the relation. |
| **Actor** | The descriptor the consumer supplies with a request: id, kind, principal, capabilities, attributes. |
| **Visibility** | A declared predicate every read applies; failing it means not found. |
| **Object id / business identifier / external identifier** | Store-assigned identity; a consumer-minted meaningful value such as a serial; a value another system owns. |
| **Sequence** | A named, scoped, monotonic counter that mints business identifiers; not gapless. |
| **Tracking mode** | Whether a type is serial-tracked, one object per thing, or quantity-tracked, counters on a stock object. |
| **Type family** | A base declaration and everything extending it. |
| **Declaration version** | The version of a type's declaration, recorded on every object and event. |
| **Publish** | Installing a declaration version, with a report of migrations, admissions, compiled invariants and sweepability. |
| **Deletion / supersession / erasure** | Ending an object; continuing it as another; removing personal values from it and its history. |
| **Provenance** | What an event records about itself: actor, principal, context, cause, declaration version, source. |
| **Idempotency key** | A caller-supplied key that makes a repeated request return the original result rather than acting twice. |
| **Subscription** | A built-in object holding a filter, an endpoint and lag thresholds; its progress is runtime state, not object state. |
| **Proposal** | A built-in object holding a request the caller lacked authority for; an authorised actor's approval executes it as themselves. |
| **Sweepable** | A transition whose guards decompose into an indexable prefilter and a residual, so `available` can answer for it. |
| **Effect** | Something caused outside ObjectKeeper; out of scope. Consumers cause effects by observing events. |

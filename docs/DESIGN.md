# ObjectKeeper — Design

Status: **under repair, not implementation-ready.** An implementation-readiness review on 2026-09-08 found 36 verified defects, 10 of which break the model or a running system; they are tracked in [`design/defects.md`](design/defects.md). Statements in this document that a defect contradicts are not yet corrected. Nothing implemented. This document is the single description of the model. Each decision, with the alternatives it rejected, is an ADR in [`docs/adr/`](adr/); the case studies that shaped it are in [`docs/design/`](design/); what the model deliberately does not cover is in [`design/edge-cases.md`](design/edge-cases.md); open work is in [`../TODO.md`](../TODO.md). Decisions taken in the autonomous design iterations of 2026-09-07/08 (ADR-0019 onward) are marked in the ADR index and listed in TODO.md under "Author review queue".

## 1. Purpose

A reusable substrate for building business applications, where the rules about data are declared alongside the data rather than implemented in each consumer.

The objective is **trust under delegation**: hand a system to people who are not supervised and to AI agents whose behaviour cannot be fully predicted, and know that nothing they do can put the data into a state that has to be cleaned up afterwards. As a rule: **data only moves to a different state when the right trigger is emitted and every required condition is met.** Reduced setup time is the mechanism, not the goal — a version that halves the work of defining a data model but leaves one unguarded write path is a failure.

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

ObjectKeeper is a layer over a database, not a database (ADR-0001). The core is **library-shaped**: a call goes in, guards evaluate, a transition and its record come out; there is no scheduler, timer or background process (ADR-0012). A deployment becomes a **service** only when it adds a delivery worker for push subscriptions (ADR-0013) or exposes the read surface over a transport (ADR-0037).

## 3. Core properties

| Property | Meaning | Why it matters |
|---|---|---|
| **Mediated** | No path to the data except through declared transitions and their guards. There is no second write path (ADR-0042) | The peace-of-mind guarantee |
| **Declared** | What is allowed is data, inspectable at runtime — not code (ADR-0010) | UI, API and agent tools are projections of one declaration |
| **Recorded** | Every change is attributed, caused and reconstructable | Trust after the fact, not only while watching |

An ORM abstracts *mechanism* — it hides SQL and faithfully executes whatever the caller asks. This layer abstracts *authority*: what may change, when, by whom, and what follows.

## 4. First consumer and case studies

The first consumer is an extended version of the Weston Robot inventory management system: an operations platform covering procurement, allocation, pre-delivery inspection, leasing and customer identity mirrored from Xero. The existing system is in production at `~/RduWs/wr_inventory_management`, already contains a hand-built version of every concern below, and will be rebuilt on ObjectKeeper with its production data ported and preserved (ADR-0015). Its object types — Robot, Accessory, SparePart, Delivery, Service, WarrantyContract, ProductConfiguration, IntakeBatch, ProcurementOrder, ShippingRecord, and the planned Engagement and Lease — are the ones the model must carry; the Deployment and Site names in the early ADRs are illustrative.

The model was then tested against four further shapes, each recorded as a case study with what it forced: issue tracking (`design/case-study-tickets.md`), customer records (`design/case-study-crm.md`), orders at volume (`design/case-study-orders.md`), and approvals with bookings (`design/case-study-approvals-and-bookings.md`). The worked walkthrough of the first consumer's own lifecycles is `design/first-consumer-walkthrough.md`.

## 5. The model

```text
ObjectType            declared under a version (§5.9); may extend a base for
                      attributes, relationships, invariants, derived attributes;
                      declares a tracking mode, serial or quantity (ADR-0050)
  id                store-assigned, globally unique, immutable, opaque (ADR-0018)
  version           incremented on every recorded change (ADR-0023)
  attributes        all written only via transitions (ADR-0042)
      identifier    optionally minted from a named, scoped sequence (ADR-0029)
      external      optionally marked with a source; unique per source (ADR-0037)
      personal      optionally marked; subject to erasure (ADR-0031)
      file          a content-addressed reference; bytes external (ADR-0017, proposed)
    derived         a named expression, never stored, evaluated on read (ADR-0021)
    indexed         declared per attribute; scans and query filters use these
    summary         the attribute set returned by default in listings
  visibility        optional predicate over actor and object (ADR-0030)
  invariants        type-level properties enforced at any transition that could
                    violate them, on either side (ADR-0009)
  relationships     carry no attributes; anything richer is a link object
    composition     exclusive membership + lifetime bounded by the whole
    reference       everything else; inverses may be declared
  state machine     exactly one, a named declaration bound whole (ADR-0026)
    states          each with a category; at least one terminal; a terminal
                    state may be the deleted state (ADR-0024) or superseding,
                    naming a successor (ADR-0028)
    transitions     addressed by name; from a state, a set, or any non-terminal
      inputs        arguments supplied by the caller
      guards        over current state, inputs, actor and now
      outcome       own state and controlled writes (attribute := expression),
                    plus cascaded transitions and creations across
                    relationships, all in one transaction (ADR-0019)
      only via      reachable only as a cascade from named parents (ADR-0020)
      proposable    a caller lacking authority may file a Proposal (ADR-0036)
      asserting     may set a declared set of states without satisfying the
                    type's other guards; capability-gated, reason required,
                    records what it stepped over (ADR-0040)
    actions         transitions whose from-state equals their to-state (ADR-0016)

Actor                 supplied by the consumer with every request: id, kind
                      (human | agent | service), optional principal,
                      capabilities, attributes (ADR-0025)
```

### 5.1 Objects and identity

Every object carries a store-assigned, globally unique, immutable, opaque id, assigned at creation and at import (ADR-0018). Business identifiers (a serial, a ticket key) and external identifiers (a Xero contact id, a legacy primary key) are controlled attributes, never identity; a business identifier may be minted from a named, scoped, monotonic sequence at creation (ADR-0029). An object never changes type; when it must continue as something else it is superseded (§8).

### 5.2 Attributes

An attribute has a declared type: string, integer, decimal with scale, money with currency, timestamp, duration, enum with declared options, reference, event reference, file, or a set of any of these. **Every attribute is written only by a transition outcome** (ADR-0042); there is no ungated write. A type makes attributes editable by declaring an action for them, which costs one declaration and gives the edit a guard, an actor, an event and a place in the availability listing. **Derived** attributes are named expressions, never stored (ADR-0021). Attributes marked **personal** are subject to erasure (ADR-0031). A **file** attribute holds a content-addressed reference to a blob in external storage with its hash, size, media type and provenance; ObjectKeeper never reads, streams or serves the bytes; its only byte-level operation is deleting them during erasure, which ADR-0031 requires (ADR-0017, proposed; ADR-0041).

### 5.3 Relationships

Relationships are **composition** — exclusive membership and a lifetime bounded by the whole, so parts cascade with the whole — or **reference** — everything else (ADR-0003). A composite's state is its own, gated by guards that read its parts, never derived from them (ADR-0004); "why is this stuck" is then answered by a `dependent` verdict naming the parts. References carry no attributes; a relation with labels, dates or a lifecycle of its own is a **link object**, a type with two references. Inverses may be declared, and outcomes and guards may traverse them.

### 5.4 State machines and transitions

Each type binds exactly one **named** state machine, whole; two types that share a lifecycle bind the same machine (ADR-0003, ADR-0026). Every state has a **category** from a consumer-defined set, so family-wide guards and views need not know state names.

A **transition** is a named, guarded, recorded request to move an object from one state to another. It is requested **by name, never by target state** (ADR-0016). Its from-state may be one state, a set, or any non-terminal state. It declares **inputs**, **guards** over current state, inputs, actor and clock, and an **outcome**. An **action** is a transition whose from- and to-state are equal. Its outcome is unrestricted like any other: it may write attributes, cascade transitions and create objects, and it is declared, listed and recorded the same way (ADR-0016, ADR-0041). A same-state request that names no transition is refused; nothing is recorded for a change that did not happen.

An **outcome** is an ordered sequence of steps (ADR-0046): attribute writes `attribute := expression`; creations, optionally bound to a name later steps may use; cascaded transitions with their own inputs; and iteration over a relationship or a count, binding its element explicitly. It is straight-line: no step chooses between alternative outcomes, and branching is two transitions with distinguishing guards. A declaration may cap iteration fan-out, and exceeding it is refused with `over-limit`. A composition part may be re-parented by a transition that writes its owner, which is what makes splitting an object expressible.

A transition may be **only via** named parent transitions: not requestable, reachable only as their cascade, carrying no actor guards of its own (ADR-0020). It may be **proposable**: a caller lacking authority may file a Proposal instead of receiving an error (ADR-0036).

Creation is the transition from nothing into an initial state, carrying its own guards; requiredness attaches to transitions, so `validate(object)` has no answer and `validate(object, intent)` does (ADR-0002, ADR-0005).

### 5.5 Guards, verdicts and remedy classes

A guard is an expression that must hold for a transition to proceed. Evaluating a request yields a **verdict**: satisfied; unsatisfied with a reason and a remedy class; `stale` (ADR-0023); `not found` (ADR-0030); `not requestable` (ADR-0020), returned when a caller names an only-via transition, which is never listed; or `over-limit`, when a cascade exceeds a declared fan-out cap (ADR-0041). Availability is three-way: available, available-with-input naming what must be supplied, blocked. A transition **declares** its inputs, and its guards reference them (ADR-0047); publishing rejects a guard naming an undeclared input and warns on an input nothing uses, so the tool schema and the validation rules cannot drift because they are one declaration. Each guard clause declares its remedy class, inferred where the shape is unambiguous.

| Remedy class | Meaning | Caller's next move |
|---|---|---|
| `self-serviceable` | Satisfiable by a transition argument | Supply it |
| `delegable` | Another actor must act; may name a capability and, if proposable, offer a Proposal | Ask them, or propose |
| `temporal` | Only time will satisfy it | Come back later |
| `dependent` | Another object must change state; names it | Work on that first |
| `unreachable-from-here` | Wrong state; another transition or action comes first, or this one is only-via | Take a different path |

A guard that depends on facts outside the store references a **named external evaluator** that must return the same verdict shape (ADR-0008, proposed). Evaluators are consulted **before** the write transaction opens, never inside it, and their verdict carries an as-of time recorded on the event; a declaration may set a freshness bound, past which the verdict is refused as `temporal` (ADR-0049). The guarantee for an external fact is therefore as of that time, not at commit. An invariant violation names the conflicting objects.

### 5.6 Invariants

A type-level property declared once. Enforcement is dynamic: after a request applies its outcomes, every invariant the written objects could violate is checked (ADR-0045). An invariant may traverse only relationships with declared inverses, so the affected set is found by reverse traversal rather than a type scan; publishing rejects a declaration that breaks this and reports, per invariant, which transitions could violate it. Since transitions run at serialisable isolation (ADR-0039), invariants are enforced correctly whether or not they compile to a database constraint. Compilation is an optimisation: which shapes compile is a property of the backend, and publishing reports which of a declaration's invariants the configured backend can compile (ADR-0041). PostgreSQL compiles uniqueness, uniqueness per external source and interval exclusion per key; SQLite compiles fewer. "No two open engagements for one unit" and "one active configuration version per SKU" are the first consumer's instances.

### 5.7 The expression language

One language serves guards, invariants, derived attributes, visibility predicates, outcome values and filters (ADR-0021, ADR-0032, ADR-0035):

| Construct | Example |
|---|---|
| literals; attribute paths across relationships; `this`, `inputs.*`, `actor.*`, `now` | `binding.delivery.state`, `actor.has(DELIVERY_COMPLETE)` |
| comparison, null test, membership; boolean logic and implication | `reason in CancellationReason`, `required → count(photos) >= 1` |
| `count`, `all`, `any`, `none`, `sum`, `min`, `max` over a relationship or a type, binding the element | `none(s in Service where s.unit == this and s.state != CANCELLED)`, `sum(l in lines: l.qty * l.unit_price)` |
| arithmetic on numbers; durations, and `+ -` with timestamps | `on_hand - reserved >= inputs.qty`, `placed_at + 30 min <= now` |
| `changed_since(attributes, event)` over the object's history | `not changed_since([amount, vendor], a.event)` |
| conditional expression, in derived attributes only | `unit == null ? UNFILLED : …` |
| a declared external evaluator | `xero.invoice_valid(order_id)` |

Evaluation is **three-valued** (ADR-0047): comparison with an absent value, and division by zero, yield unknown; unknown propagates; and a guard that evaluates to unknown **fails**, naming the clause. `this` always means the object the expression is declared on and never rebinds, so every aggregate binds its element explicitly. Derived attributes must form an acyclic graph, checked at publish.

Not in the language: grouped aggregation, string operations beyond equality and membership, user-defined functions, recursion or transitive closure, any call other than a declared evaluator. The line with computation (ADR-0007, ADR-0032): the store evaluates **declared arithmetic over its own data**; it does not own **domain formulas** — tax, pricing, discounts, scoring, conversion — which consumers compute and supply as inputs for guards to check. The language grows only by an ADR naming the case that forced it.

### 5.8 Actors and visibility

Every request carries an **actor descriptor** the consumer's authentication produces: id, kind (human, agent, service), optional principal, a set of opaque capabilities, optional attributes (ADR-0025). ObjectKeeper validates its shape, not its truth. Guards read it; every event records it. Delegation and attenuation arrive in the descriptor; a consumer that must govern delegations models them as objects (ADR-0036). Where a guard needs a referenced person, that person is an ordinary object.

A type may declare a **visibility** predicate over actor and object; every read, query, availability result and delivered event applies it, and an invisible object is **not found**, never "blocked" (ADR-0030).

### 5.9 Declarations: composition and versioning

A type may **extend** a base declaration for attributes, relationships, invariants and derived attributes; a **family** is a base and everything extending it, and is a query target (ADR-0026). Declarations are **versioned**; every object and event records the version in force. Additions apply forward and a new guard bites at the next transition; a new invariant is checked against live objects at publish and reported; removing a state requires a mapping applied as recorded **migration** transitions with provenance `migrated` (ADR-0027). Publishing is a designed operation with a report. The printable rule set is per version.

## 6. Transition execution

A request names an object, a transition, its inputs and the actor, and optionally the version the caller last read, an idempotency key (ADR-0014), and a free-form `context` recorded on the event. It runs in one transaction at **serialisable isolation** (ADR-0039):

1. If the type declares visibility and the actor cannot see the object, refuse as not found (ADR-0030).
2. If an `expected_version` is given and differs, refuse with `stale`.
3. If the idempotency key has been applied before, return the original result, marked as a replay (ADR-0041).
4. Evaluate the parent transition's guards. A failure refuses the request, naming the guard and its remedy class.
5. Apply the parent's outcome: its new state and its controlled writes, each value read at the moment it is applied.
6. Then, depth-first in declaration order and over relationship elements in ascending object-id order, take each cascaded transition or creation in turn: evaluate its guards **against the state produced so far**, then apply its outcome immediately (ADR-0038). Only-via transitions contribute their non-actor guards; other cascades run as the requesting actor. Any failure aborts the whole request and rolls back.
7. Check every invariant the written objects could violate; those compiled to constraints are enforced by the database (ADR-0009).
8. Increment each written object's version; record one event per transition, each cascaded event carrying the parent's event as its cause; append them to the log in the same transaction (ADR-0013). Commit.

Row locks are taken as objects are reached, for contention rather than correctness: serialisable isolation is what makes a guard's reads safe, including reads of objects the request never writes. A serialisation failure is retried to a declared bound and then refused with `stale` (ADR-0039).

An **asserting** transition (ADR-0040) follows the same sequence with two differences: step 4 evaluates only its own guards, which are its capability and reason requirements rather than the type's; and step 7 refuses on any invariant violation the request did not explicitly admit.

The cascade graph declared across transitions must be acyclic (ADR-0019).

## 7. History, events and delivery

**Current state is stored**, one row per object with its attributes, state, version, declaration version and last event; **the event log is the permanent history**, written in the transition's transaction, and a fold of the log reproduces the row, never the reverse (ADR-0033). Every event carries `changes_state` (from ≠ to) and its **provenance**: actor, principal, context, cause, declaration version, and source — `observed`, `asserted`, `migrated`, `corrected` or `erased`. The log is **never pruned**; retention is archival tiering that keeps events readable and reachable by erasure.

Events are strictly ordered per object and causally ordered across a cascade; a global position serves cursors and is monotonic but not a commit order, and the pull interface states the window a cursor must tolerate (ADR-0034).

```text
            recorded transition
                    │
         permanent ordered log        written in the same transaction
                    │
        ┌───────────┴───────────┐
     pull                     push
  Subscription holds       delivery worker
  a cursor                 calls subscribers
```

**Subscriptions** are a built-in object type holding a filter over a type or family, transition names and `changes_state`, plus lag thresholds and a lifecycle of `active → revoked`. Delivery progress is **runtime state, not object state**: the acknowledged position is stored beside the idempotency record, is not versioned and emits no events, so a poll never lands in the permanent log. Lag and death are **derived** from that position and the thresholds, so nothing has to move a subscription into them and ADR-0012 is untouched. A subscription may be revived at any position because nothing is pruned (ADR-0034, ADR-0043). Delivery is **at-least-once with idempotent handling**; exactly-once *effect* comes from deduplicating on the event id at the receiver, and transitions accept an idempotency key so the common case — an event causing a transition — deduplicates centrally and records causal lineage (ADR-0014). Acknowledgements track progress and lag; they are not a correctness mechanism.

**Time.** `now` is a value in guards. A time-driven transition is an ordinary one — `expire: ACTIVE → EXPIRED, guard end_date <= now` — and ObjectKeeper never requests it; a scheduler above asks the availability query and requests each answer with a periodic idempotency key (ADR-0022). The guard is the timing rule, in one place. ObjectKeeper initiates nothing (ADR-0012).

## 8. Ends of life: deletion, supersession, erasure

**Deletion** is a terminal transition gated on there being no live object referencing this one; parts cascade, references block with `dependent` naming them; deleted objects stay readable by id and in history and take no further transitions; there is no silent bypass, and repair is a declared asserting transition (ADR-0040).

**Supersession** ends an object in a terminal state that names its successor; id and history stay; the read surface follows the pointer on request; references are re-pointed only by declared cascade. It is how an object changes kind — moved, converted, merged (ADR-0028). A duplicate is closed with a reference, not superseded.

**Erasure** replaces the values of declared personal attributes with a redaction marker on the object and in every event that carried them, deletes referenced file content, keeps the event skeleton, records that it happened, and is irreversible. It is the one place the log is rewritten, and it is not deletion (ADR-0031).

## 9. Built-in types

`Subscription` (§7) and `Proposal` are built in and not user-definable. A **Proposal** holds a request a caller lacked authority for — target, transition, inputs, proposer — with a lifecycle `pending → executed | rejected | withdrawn | expired | invalidated`; an authorised actor's approval executes the recorded request as that actor with the approval as cause, re-evaluating every non-actor guard under the **current** declaration. A guard that fails leaves it pending; a declaration that can no longer express the request invalidates it (ADR-0036, ADR-0044). `expired` is derived, not driven.

## 10. The read surface

One API, every operation taking an actor and applying visibility (ADR-0037): **get** by id with optional supersession following; **query** over a type or family with a filter over indexed attributes, state and category, ordering, cursor pagination and field selection; **lookup** by external identifier; **availability** of the requestable transitions for one object, with verdicts and parameter schemas, excluding only-via transitions (ADR-0041); **available** objects for a given transition, which requires that transition to be **sweepable**, meaning its guards decompose into an indexable prefilter and a residual, as publishing reports (ADR-0048); **check**, the verdict a request would receive without executing; **history** with provenance and the combined timeline through supersession; **declaration**, the inspectable rule set per version, rendered also as readable text and as agent tool schemas; **pull** over a subscription's cursor; **batch**, N independent requests with N verdicts. There is no atomic batch: atomic multi-object semantics are declared cascades. Neither `available` nor `check` calls an external evaluator, and both say which guards they did not evaluate. Time-dependent derived attributes are queried by filtering the stored operand they compare against `now`, and a per-attribute last-written index makes `changed_since` a constant-time comparison (ADR-0048). Beyond those, the store maintains no projection it does not declare; where a scan is too slow, the type declares a counter and the cascade that maintains it.

## 11. Import and migration

The first consumer's production data is ported by a designed path (ADR-0015). Every imported object receives a new id and keeps its legacy key as `external: legacy`; cross-references are re-pointed through that mapping (ADR-0018). Imported state is written by an asserting transition with provenance `asserted` (ADR-0040); the importer evaluates the declaration and reports every violated invariant and unsatisfied structural guard, and a person decides each class; legacy history is preserved as read-only entries of kind `legacy`; soft-deleted rows land in the type's deleted state; files are content-addressed into the deployment's blob store. Import is declaration migration from version zero (ADR-0027). Cutover cannot be dual-write.

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

The guarantee is that **no state change bypasses the guards except through a declared, capability-gated, recorded assertion** (ADR-0040), and that the objects holding asserted state or admitted invariant violations are queryable at any time. It cannot guarantee that **the guards say what was meant**. Risk relocates from scattered implementation bugs to specification gaps in one readable place. Hence: the rule set for a type must be printable for review by someone who knows the process, and acceptance testing must be adversarial in the threat model's sense — a fallible actor that guesses, retries and skips steps, trying every route to an invalid state.

The mediated property is an operational commitment: anything else that can reach the database — a migration script, `psql`, a reporting job — voids it. Administrative repair is the recorded override, and the data import is its first use at scale.

Concrete cases the model does not cover, or covers with a caveat — splitting an object, in-place type change, gapless sequences, hot-row serialisation, multi-region stock, acyclicity of self-referential hierarchies, unmerge, local-time rules, waitlist promotion, and others — are catalogued in [`design/edge-cases.md`](design/edge-cases.md).

## 14. Terminology

| Term | Meaning |
|---|---|
| **Lifecycle** | The observable shape — the states an object passes through. |
| **State machine** | The mechanism — states, transitions and guards — as a named declaration a type binds. |
| **Transition** | A named, guarded, recorded request to move an object between states; addressed by name. |
| **Action** | A transition whose from- and to-state are equal. Its outcome is unrestricted. No exit or entry semantics. |
| **Guard** | An expression that must hold for a transition; returns a structured verdict. |
| **Invariant** | A type-level property enforced at every transition that could violate it. |
| **Outcome** | What a transition writes and records: state, attribute writes, cascaded transitions and creations, events. |
| **Cascaded transition** | A transition on a related object declared in another's outcome; gated by its own guards, same transaction, recorded as caused by the parent. |
| **Only via** | A transition reachable only as a cascade from named parents. |
| **Effect** | Something caused outside ObjectKeeper; out of scope; consumers cause effects by observing events. |
| **Derived attribute** | A named expression, never stored, evaluated on read. |
| **Verdict** | The result of evaluating a request: satisfied, unsatisfied with remedy class, `stale`, `not found`, `not requestable`, `over-limit`. |
| **Available transitions** | Transitions satisfied or satisfiable with input for an object and actor now. |
| **Actor** | The descriptor the consumer supplies with a request: id, kind, principal, capabilities, attributes. |
| **Visibility** | A declared predicate every read applies; failing it means not found. |
| **Approval** | A recorded part — approver, kind, decision, event; "needs approval" is a guard counting approvals still valid under `changed_since`. |
| **Proposal** | A built-in object holding a request the caller lacked authority for; approval executes it. |
| **Object id / business identifier / external identifier** | Store-assigned identity; a consumer-minted meaningful identifier such as a serial; an identifier another system owns. |
| **Sequence** | A named, scoped, monotonic counter that mints business identifiers. |
| **Link object** | A type that relates two objects and carries attributes or a lifecycle about the relation. |
| **Type family** | A base declaration and everything extending it. |
| **State category** | A consumer-defined classification every state declares. |
| **Declaration version** | The version of a type's declaration, recorded on every object and event. |
| **Supersession** | Ending an object in a terminal state naming its successor. |
| **Erasure** | Redaction of personal attributes across an object and its history; recorded, irreversible. |
| **Assertion** | A declared, capability-gated transition that may set a state without satisfying the type's other guards, recording its reason and what it stepped over (ADR-0040). |
| **Tracking mode** | Whether a type is serial-tracked, one object per physical thing, or quantity-tracked, counters on a stock object (ADR-0050). |
| **Sweepable** | A transition whose guards decompose into an indexable prefilter and a residual, so `available` can answer for it (ADR-0048). |
| **Provenance** | What an event records about itself: actor, principal, context, cause, declaration version, source. |
| **Subscription** | A built-in object holding a filter and a cursor over the log, with its own lifecycle. |
| **Expression language** | The one language of guards, invariants, derived attributes, visibility, outcome values and filters. |

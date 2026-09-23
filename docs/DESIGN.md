# ObjectKeeper — Design

**Status: under review.** Every finding of every review is recorded in [`design/defects.md`](design/defects.md), 235 entries and five cosmetics; 235 are closed and 0 are open. The author accepted ADR-0081 to ADR-0096 on 2026-09-23 — ADR-0087 to ADR-0096 decided at the author's direction to cover every PRD requirement ([`design/traceability.md`](design/traceability.md)), and accepted after each decision was checked against this design; the author has ruled on ADR-0065 to ADR-0073; ADR-0019 to ADR-0064, ADR-0074 to ADR-0080 and the six implementation documents of 2026-09-09 await review, ADR-0078 to ADR-0080 first, being the three decided at the author's direction rather than by the author.

**[`PRD.md`](PRD.md) is the baseline.** It states what the product must do; this document states the model that does it. A design choice here is justified by the PRD requirement or use case it serves and is checked against the PRD, never the reverse: where the two disagree, the design changes, or the author revises the PRD first (the author, 2026-09-23). [`design/data-driven-engine.md`](design/data-driven-engine.md) records how each choice of ADR-0081 to ADR-0086 was tested against the PRD's use cases, and this document incorporates them. [`design/traceability.md`](design/traceability.md) maps every PRD requirement and use case to the sections and decisions that meet it, and `scripts/check-traceability.py` holds the map against the PRD.

This document is the single description of the **model**: what an object is, what a transition guarantees, how a request executes, what the store refuses. The **language** those things are written in belongs to [`design/declaration-syntax.md`](design/declaration-syntax.md), which owns every grammar, every spelling and the sixty-one publish checks.

Five further documents carry the parts an implementation needs, each owning what it names:

| Document | Owns |
|---|---|
| [`design/declaration-syntax.md`](design/declaration-syntax.md) | the language, every grammar and the publish checks |
| [`design/storage-schema.md`](design/storage-schema.md) | the tables, the indexes a guarantee rests on, and the DDL a change emits |
| [`design/library-api.md`](design/library-api.md) | the operations a program calls and the shapes they take |
| [`design/publish-and-import.md`](design/publish-and-import.md) | what publishing checks and reports, and how production data arrives |
| [`design/renderers.md`](design/renderers.md) | the rule set, agent tool schemas and form hints |
| [`design/adversarial-harness.md`](design/adversarial-harness.md) | the acceptance test, and what would falsify the guarantee |

**Amended for ADR-0081 to ADR-0096.** Those decisions changed all six documents above and the cutover plan, and each has been amended and says so in its header: the syntax gained seven constructs and checks 54 to 61; the schema the attempt log, the interval index, observation and label tables, the file reference index and the settled cursor; the API `metric`, `diagnostics` and `export`; the renderers the observation and metric tools; publish and import the `DeclarationChange` and imported entry times; and the harness its checks on intervals, read sets and erasure, and the barrier that interleaves requests.

Where a rule appears in both, this document states what it means and the syntax document states how it is written. That division exists because it failed twice: the outcome grammar was restated here and had gone stale in three of six lines, and a later audit found thirty-two rules stated in both documents of which six had drifted to the superseded version. When the two disagree, the syntax document is the one with a checker.

Each decision, with the alternatives it rejected, is an ADR in [`adr/`](adr/); the case studies that shaped it and the catalogue of what it deliberately does not cover are in [`design/`](design/).

## 1. Purpose

A reusable foundation on which people and AI agents **define flows and collect data**, so that the business logic built on top can be driven by data, whichever kind of actor drives it (PRD §1). A flow is a set of object types with their lifecycles, transitions and rules, declared alongside the data rather than implemented in each consumer. Its data is what it produces by running — every transition, refusal, override and change of hands — together with what users record and what their formulas compute from both. Metrics over that data let a flow start imperfect and **converge**, with the engine showing where the flow and reality disagree (ADR-0081).

The objective beneath all of it is **trust under delegation**: hand a system to people who are not supervised and to AI agents whose behaviour cannot be fully predicted, and know that nothing they do can put the data into a state that has to be cleaned up afterwards. As a rule: **an object's state changes only when someone requests a named transition and every enforced condition on it is met.** A condition on trial is not enforced (§5.5), and the exceptions are the recorded assertions of §8.

The threat model is **mistakes, not malice**. Actors are trusted but fallible: a person who skips a step, an agent that retries a timed-out request, a script that sets a flag directly. The design defends against those by construction. It does not defend against an actor holding database credentials who intends harm; that is an access-control and deployment concern (§13).

Genericity is a requirement from the start. The first consumer is the author's own operations platform (§4); it is a first customer, not a domain to specialise for.

## 2. Position in the stack and deployment shapes

```
agents · applications · human UI          consumers
─────────────────────────────────────
ObjectKeeper                              this project
─────────────────────────────────────
PostgreSQL / SQLite                       storage

siblings:  workflow orchestration (Temporal-shaped)
           external systems of record (e.g. Xero)
           agent frameworks
           file storage (S3-compatible or filesystem)
           exploratory analysis tools, fed by export
```

ObjectKeeper is a layer over a database, not a database (ADR-0001). The core is **library-shaped**: a call goes in, guards evaluate, a transition and its record come out; there is no scheduler, timer or background process (ADR-0012). A deployment becomes a **service** when it adds a delivery worker for push subscriptions (ADR-0013) or exposes the API over a transport (ADR-0037). Measurement adds no background process: metrics are computed on read, and a business exception is a condition a scheduler or an agent asks about (ADR-0081, ADR-0084).

The core is **synchronous**: a request runs to completion on its caller's thread, and an asynchronous service wraps it on a thread pool. The store takes its **id source, clock and connection source** as injected dependencies, which is what lets the adversarial harness interleave requests deterministically against a real database (ADR-0077, ADR-0091).

## 3. Core properties

| Property | Meaning | Why it matters |
|---|---|---|
| **Mediated** | No path to the data except through declared transitions and their guards. There is no second write path (ADR-0042): recording a datapoint is a creation request (ADR-0082). The two ways to set state without satisfying an enforced guard are both assertions — a clause on trial is not enforced, and guarantees nothing (§5.5) — capability-gated and recorded: one the type declares (ADR-0040), and the built-in one import and migration use (ADR-0054) | The peace-of-mind guarantee |
| **Declared** | What is allowed is data, inspectable at runtime, not code (ADR-0010) | UI, API and agent tools are projections of one declaration |
| **Recorded** | Every change and every recorded datapoint is attributed, caused and reconstructable | Trust after the fact, not only while watching |
| **Measured** | Every flow produces data about itself — transitions, refusals, overrides, time in each state and in each assignee's hands — users record their own, and everyone reads both through the same declared metrics (ADR-0081) | A flow can be evaluated and improved from evidence, and a person and an agent see the same numbers |

An ORM abstracts *mechanism*: it hides SQL and faithfully executes whatever the caller asks. This layer abstracts *authority*: what may change, when, by whom, and what follows.

## 4. First consumer and case studies

The first consumer is an extended version of the Weston Robot inventory management system: an operations platform covering procurement, allocation, pre-delivery inspection, leasing and customer identity mirrored from Xero. The existing system is in production at `~/RduWs/wr_inventory_management`, already contains a hand-built version of every concern below, and will be rebuilt on ObjectKeeper with its production data ported and preserved (ADR-0015). The PRD's use cases are drawn from it wherever they can be.

The model was tested against five further shapes, each recorded with what it forced: issue tracking, customer records, orders at volume, approvals with bookings, and a payments ledger, in [`design/`](design/). The ledger is the one written after the model rather than before it, and it is the only case that is high-volume, short-lived and money-carrying; four of the thirteen questions the author ruled on came from it. The worked walkthrough of the first consumer's own lifecycles is [`design/first-consumer-walkthrough.md`](design/first-consumer-walkthrough.md).

## 5. The model

```
ObjectType            declared under a version (§5.9); may extend a base;
                      declares a tracking mode, serial, quantity or record (§5.10);
                      or is marked mirror: held here, owned elsewhere, written
                      only by import, no transitions (§11)
  id                store-assigned, globally unique, immutable, opaque
  version           incremented on every recorded change
  attributes        written only by transition outcomes; each has a declared
                    type, and may be marked identifier, external, personal,
                    indexed, or part of the summary set (§5.2)
  derived           named expressions, never stored, evaluated on read (§5.2)
  visibility        optional predicate over actor and object (§5.8)
  invariants        type-level properties, enforced dynamically (§5.6)
  relationships     composition or reference; carry no attributes; a part names
                    a cascade or survives per terminal transition of the whole;
                    a reference may be marked assignee (§5.3, §5.13)
  observations      recorded datapoints: born-final parts of a declared kind,
                    recorded by a creation request; labels on every type (§5.11)
  state machine     exactly one, named or inline; a binder adds its own transitions
                    and replaces the machine's creations (§5.4)
    states          each with a category; at least one terminal
    transitions     named; declare inputs, guards and an outcome (§5.4)
                    optional markings: only via · proposable · asserting ·
                    backdatable; a guard clause may observe instead of enforce
    actions         transitions whose from-state equals their to-state

Metric                a declared formula across many objects or over time, with
                      dimensions and flags, computed on read (§5.12)
```

Every request additionally carries an **actor** the consumer supplies (§5.8). Beside the objects, the engine keeps an **interval index** of how long each object held each state and each tracked value, and an **attempt log** of the requests it did not apply (§7).

### 5.1 Objects and identity

Every object carries a store-assigned, globally unique, immutable, opaque id, assigned at creation and at import (ADR-0018). It is a UUIDv7, so ascending id order is creation order, which is what makes a cascade's iteration order deterministic (§6); its embedded time is never read back as a fact about the object, which is what `created_at` is for (ADR-0077). Every object has `.created_at` and `.created_by_kind`, the kind of actor whose request created it, both readable by rules and metrics (ADR-0096). Business identifiers such as a serial or a ticket key, and external identifiers such as a Xero contact id or a legacy primary key, are ordinary attributes, never identity. A business identifier may be minted at creation from a named, scoped, monotonic sequence, which is monotonic but not gapless (ADR-0029). An object never changes type; when it must continue as something else it is superseded (§8).

### 5.2 Attributes

An attribute has a declared type: string, boolean, integer, decimal with scale, money with currency, timestamp, duration, identity, enum with declared options, event reference, file, or a set of any of these. A **reference is not an attribute type**; it is declared as `ref`, `part` or `owner` (§5.3), though an input may be reference-typed.

**Every attribute is written only by a transition outcome** (ADR-0042). There is no ungated write. A type makes attributes editable by declaring an action for them, which costs one declaration and gives the edit a guard, an actor and an event. Attributes may additionally be marked:

- **identifier**, minted from a sequence (ADR-0029);
- **external**, naming the system that owns the value, with uniqueness per source automatic (ADR-0037);
- **personal**, subject to erasure (§8). A personal value may not be written into a non-personal attribute, and publishing rejects a declaration that does (ADR-0051);
- **indexed**, which is what query filters, type-scan predicates and visibility predicates may use (ADR-0048);
- part of the **summary** set, the attributes a listing returns by default.

**Derived** attributes are named expressions, never stored, evaluated on read; their dependency graph must be acyclic (ADR-0021, ADR-0047). A derived attribute for one object is what the PRD calls a derived datapoint, and it may read the object's own intervals (§5.7). A **file** attribute holds a content-addressed reference to a blob in external storage with its hash, size, media type and provenance. ObjectKeeper never reads, streams or serves the bytes; its one byte-level operation is deleting them during erasure (ADR-0017, ADR-0041). A deployment must disable its blob store's own lifecycle expiry, since the log is permanent and a reference outlives any expiry policy.

Every **enum** attribute, like every singular stored reference (§5.3), is **tracked**: how long an object held each value is kept in the interval index (§7) without its author declaring which (ADR-0083, ADR-0086). Numbers and free text are not tracked; their changes remain in the events. A personal enum is tracked like any other, and erasure redacts its values in the interval index as it does in the events the index is rebuilt from (ADR-0096).

### 5.3 Relationships

Relationships are **composition**, meaning exclusive membership and a lifetime bounded by the whole, or **reference**, meaning everything else (ADR-0003). A composite's state is its own, gated by guards that read its parts and never derived from them (ADR-0004); "why is this stuck" is then answered by a `dependent` verdict naming the parts. A composition part may be re-parented by a transition that writes its owner, which is what makes splitting an object expressible (ADR-0046).

References carry no attributes. A relation with labels, dates or a lifecycle of its own is a **link object**: a type with two references. **One end of a relationship is stored and its declared inverse is a derived view over it** (ADR-0056), so there is only ever one write and one event; guards, invariants and outcomes traverse either direction. A composition declares, for **every terminal transition of the whole**, the child transition it drives and that cascade's bound, or marks the part `survives` to say deliberately that it outlives its whole; publishing rejects a terminal transition covered by neither (ADR-0058). A cascade clause may carry arguments (ADR-0066).

**`changed_since` reaches a part relationship by its own position.** The last event on each part relationship of an object is kept per relationship, so a guard naming `checklist_items` is not invalidated by a change to `approvals` (ADR-0082, repairing D202). An **observation** is a part of a special shape (§5.11). A singular reference may be marked **`assignee`** (§5.13), and every singular stored reference is tracked (§5.2).

### 5.4 State machines, transitions and outcomes

Each type binds exactly one **named** state machine, or declares its lifecycle inline; two types that share a lifecycle bind the same machine (ADR-0003, ADR-0026). A binder may add transitions of its own, and its own **creations replace the machine's** rather than adding to them, since birth is where a type's obligations are established (ADR-0064). The machine's creation **guards** still bind the replacement, so a lifecycle's entry condition cannot be dropped by declaring a creation (ADR-0065). Every state has a **category** from a consumer-defined set, so guards and views that span a family need not know state names, and at least one state is terminal.

A **transition** is a named, guarded, recorded request to move an object from one state to another. It is requested **by name, never by target state**. Its from-state may be one state, a set, or any non-terminal state. It declares **inputs** with types and optionality, **guards**, and an **outcome**. An **action** is a transition whose from- and to-state are equal; its outcome is unrestricted like any other, and there are **no exit or entry semantics** for a state, so an action neither leaves nor re-enters one (ADR-0016, ADR-0041). A request naming no declared transition is refused, so nothing is recorded in history for a change that did not happen; the refusal itself goes to the attempt log (§7), which is not history (ADR-0083).

An **outcome** is an ordered sequence of steps that write this object, create another, reach another by `call`, supersede, or loop over a bounded collection (ADR-0046, ADR-0052). **The grammar is `docs/design/declaration-syntax.md` §5.2, which owns it**; this section states what an outcome is and not how it is spelled, because two statements of one grammar drift and the syntax document is the one publishing checks. Until iteration 14 the grammar was restated here and had gone stale in three of its six lines.

Rules that make an outcome readable and safe:

- **Straight-line.** No *step* chooses between alternatives, and no step is conditional. Where the **behaviour** differs by a value — a different set of writes, a different object created — declare one transition per case, each guarded on that value, so the applicable one appears in the availability listing and a value matching none is visibly stuck. Where only a **value** differs, a conditional expression is allowed on the right of a write (`docs/design/declaration-syntax.md` §8.3), since it changes what is written and not what happens.
- **Sets change element-wise.** `add` and `remove` are read-modify-write like any outcome write, so two adds in one request accumulate and two concurrent adds serialise. The expression language gains no set algebra; `set` on a set-valued attribute still replaces it wholly (ADR-0055).
- **Absence skips, in two places only.** A write whose right-hand side is an unsupplied optional input is skipped, not applied, and a `call` whose path runs through an absent optional end is skipped because there is no object to reach. Everywhere else absence in a path is an error. Clearing a value deliberately is the `clear` step, which writes absence and is a write like any other (ADR-0073). Until iteration 18 this sentence promised something the syntax had no spelling for.
- **`this` and `this_event` are available**, including inside a creation outcome: the new object's id and its event's identity are allocated before the outcome runs (ADR-0046, ADR-0052).
- **Bounded.** Every loop and every cascade clause must declare a bound; exceeding one is refused with `over-limit` naming that loop (ADR-0071). The graph of transitions that reference each other in outcomes must be acyclic (ADR-0019).

A transition may be marked **only via** named parent transitions, meaning it is not requestable and carries no actor guards of its own (ADR-0020); **proposable**, meaning a caller lacking authority may file a Proposal (§9); or **asserting** (§8). It may also be marked **backdatable within** a duration: it then accepts the time the change actually happened, within that bound, never later than the time it is recorded, and never before the start of the current interval of any state or tracked value the request changes, and the event keeps both times, because a duration computed from when someone got round to recording a change is wrong (ADR-0083). Every transition the request cascades to records the same occurred time, since one request is one change (ADR-0095).

Creation is a transition from nothing into an initial state, carrying its own guards. A type with several creation transitions is named explicitly at the creation site. Requiredness attaches to transitions, so `validate(object)` has no answer and `check(object, transition, inputs)` does (ADR-0002, ADR-0005).

### 5.5 Guards, verdicts and remedy classes

A guard is an expression that must hold for a transition to proceed. Each guard clause may **declare its remedy class**; omitted, one is always inferred, by a stated priority order because most guards match several rules, and the inference never contradicts a declared class (ADR-0063). A transition **declares its inputs**; publishing rejects a guard naming an undeclared input and warns on an input nothing uses, so the tool schema and the validation rules cannot drift, because they are one declaration.

Evaluating a request yields one **verdict**:

| Verdict | Meaning |
|---|---|
| satisfied | The request proceeds |
| unsatisfied | A guard failed; carries the failing clause, a remedy class, whether the clause was **false or unknown** — so a caller can tell a refusal from a value nobody has yet — and what the remedy points at: the objects to work on, the capability that would satisfy it, and whether the transition accepts a proposal (ADR-0092) |
| `stale` | The caller's `expected_version` is out of date, or the transaction exhausted its serialisation retries; the verdict says which (ADR-0023, ADR-0039, ADR-0092) |
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

**Approval** is not a separate mechanism. An approval is a recorded part of the approved object, carrying the approver, a kind, a decision and the event that recorded it, created by an `approve` action. "Needs approval" is then an ordinary guard counting the approvals that are still valid, where validity is `not changed_since([…], a.at_event)`, so a material edit invalidates an approval without every editing action having to remember a reset. N-of-M, sequential chains, thresholds and separation of duties are guards of that shape, and a missing approval reports `delegable` (ADR-0035). In shape an approval is an observation kind (§5.11).

A guard clause may be marked **`observe`**. It is evaluated, and where it would refuse, the request proceeds and the would-be refusal is recorded in the attempt log, linked to the event that applied. An observing clause guarantees nothing: the printed rule set shows it as not enforced, the adversarial harness treats it as absent, and enforcing it is a flow change (§9, ADR-0085).

A guard that depends on facts outside the store references a **named external evaluator**, which must return the same verdict shape and **never a value** (ADR-0008, ADR-0069). An external system that decides or assigns something is modelled as an **externally owned type** instead: an ordinary type with an external identifier, whose sync-driven transitions the sync consumer requests with the external system's own change identifier as idempotency key, and whose local transitions are whatever the domain needs (ADR-0080). Two ways to import external truth would differ in what they record, and the type is the one that leaves a history with transition names. Evaluators are consulted **before** the write transaction opens, never inside it, and their verdict carries an as-of time recorded on the event; a declaration may set a freshness bound, past which the verdict is refused as `temporal` (ADR-0049). An evaluator's arguments are resolved against committed state before the transaction and again inside it; a mismatch means the world moved, and the request consults again within its retry bound, and an argument the request's own outcome writes is refused at publish (ADR-0092). The guarantee for an external fact is as of that time, not at commit.

A guard may also read a **metric** (§5.12), consulted the same way: before the transaction opens, its arguments resolved against committed state then and again inside the transaction, and its value and as-of time recorded on the event beside the guard's name, with an optional freshness bound (ADR-0084). No first-consumer process needs this yet (PRD L5), so it is the least-evidenced mechanism in the model. A guard over one object's own observations needs none of this: it reads parts in the transaction, and records which observations it read (§5.11).

### 5.6 Invariants

A type-level property, declared once against the type rather than restated on every transition that could break it (ADR-0009). **Enforcement is dynamic**: after a request applies its outcomes, every invariant the written objects could violate is checked (ADR-0045). Three forms are permitted, and each has a rule that makes the affected set computable (ADR-0055):

- a **local** invariant reads only the object's own attributes and state, so the affected set is the object just written. This is the commonest and cheapest form;
- a **traversal** invariant reads related objects, and may traverse only relationships with **declared inverses**, so the affected objects are found by reverse traversal;
- a **type-scan** invariant reads other objects of a type and must be **symmetric**, so the predicate that finds a conflict from a written object is the same one that would find it from the other side (ADR-0052). Symmetry is decided by a **swap test** whose four normalisations are `docs/design/declaration-syntax.md` §3.4, which owns it: exchange the two objects and require the same predicate back, comparing conjuncts as a set and comparisons modulo operand order and direction. Overlap, equality on a shared key and a status filter applied to both objects all pass, and only under those normalisations — stating the test without them rejects the shapes it is meant to accept (ADR-0063). The scan never includes the object being written (ADR-0061).

Publishing classifies each invariant, reports which form it decided and which transitions could violate it, and rejects anything that fits none. An erasure admits every invariant reading an attribute it erased, and records the admission, since writing absence may break one and refusing the erasure is not available (ADR-0060). Correctness comes from serialisable isolation (ADR-0039), so an invariant is enforced whether or not it compiles to a database constraint. Compilation is an optimisation whose available shapes depend on the backend, and publishing reports which of a declaration's invariants the configured backend can compile (ADR-0041). No invariant may read a metric, as none may read `now`. An observation kind may declare invariants over its own fields, checked when an observation is recorded (§5.11).

### 5.7 The expression language

One language serves guards, invariants, derived attributes, visibility predicates, outcome values and filters (ADR-0021, ADR-0032, ADR-0047, ADR-0053):

| Construct | Example |
|---|---|
| literals; attribute paths across relationships; `this`, `this_event`, `inputs.*`, `actor.*`, `now` | `binding.delivery.state`, `actor.has(DELIVERY_COMPLETE)` |
| comparison, membership; boolean logic and implication | `reason in CancellationReason`, `required implies count(p in photos) >= 1` |
| `is null`, `is not null` — definite presence tests, never unknown | `s.unit is not null` |
| `count`, `all`, `any`, `none`, `sum`, `min`, `max` over a relationship or a type, binding the element | `none(s in Service where s.unit == this and s.state != CANCELLED)`, `sum(l in lines: l.qty * l.unit_price)` |
| arithmetic on numbers; durations, and `+ -` with timestamps | `on_hand - reserved >= inputs.qty`, `placed_at + 30 min <= now` |
| `changed_since([attributes or parts], event)` over the object's history | `not changed_since([amount, vendor, lines], a.at_event)` |
| conditional expression, in a derived attribute or an outcome value, never in a guard | `if unit is null then UNFILLED else FILLED` |
| a declared external evaluator | `xero.invoice_valid(order_id)` |
| `entered_at(STATE or tracked member)`, `time_in(STATE)`, over the object's own intervals | `unit is null and entered_at(unit) + 14 days <= now` |
| a declared metric, in a guard only (§5.12) | `metric(inspection_pass_rate, engineer := engineer, over last 30 days) >= 0.900` |

Evaluation is **three-valued**: comparison with an absent value, and division by zero, yield unknown. Unknown propagates, with the two Kleene exceptions that make presence tests useful — `and` is false if either side is false, `or` is true if either side is true — and a guard that evaluates to unknown **fails**, naming the clause and reporting the value as unknown rather than false. An **invariant** that evaluates to unknown **holds**, which is the rule a database `CHECK` follows and the only one under which a partially filled object is workable. Presence is therefore tested with `is null` and `is not null`, and comparing against a bare `null` literal is a publish error (ADR-0053). Over an empty set, `count` and `sum` are zero, `all` and `none` are true, `any` is false, and `min` and `max` are unknown. `this` always means the object the expression is declared on and never rebinds, so every aggregate binds its element explicitly.

Not in the language of guards, invariants and derived attributes: grouped aggregation, string operations beyond equality and membership, user-defined functions, recursion or transitive closure, any call other than a declared evaluator or a metric. Grouped aggregation, time windows, `avg`, `median` and `percentile` exist in the metric form alone (§5.12).

**How far a guard actually reaches, measured.** Every guard in the first consumer was counted on 2026-09-09. Of the 205 that read data, 94 are local to the object and 111 reach beyond it. Of those 111: 27 are one hop to a single object, 31 aggregate over a collection the object owns, 30 are type-wide existence or uniqueness, and 23 are two hops — 21 of which also aggregate. **Nothing in that system reaches three hops.** So the language needs one and two hops, aggregates over an owned collection, and a type-wide existence test, and it needs no transitive closure — which is the boundary ADR-0021 drew by argument and this measures.

**The line with computation** (ADR-0007, ADR-0032, ADR-0081): the store evaluates **any declared formula over its own data** — a guard, a view, an invariant, a quantity an outcome writes, a derived datapoint, a metric, a score built from them. It does not own a formula that needs data or rules it does not hold, such as a tax table or a pricing engine, and it does not choose, cause effects or orchestrate; consumers do those and supply results as inputs for guards to check. The language grows only by an ADR naming the case that forced it, and the case must be a PRD requirement.

### 5.8 Actors and visibility

Every request carries an **actor descriptor** the consumer's authentication produces: id, kind (human, agent or service), optional principal, and a set of opaque capabilities (ADR-0025, ADR-0079). ObjectKeeper validates its shape, not its truth. A guard reads **four** members of it — `.id`, `.kind`, `.principal` and `.has(<capability>)` — and nothing else: the descriptor carries no declared types, so a guard over any other value could be neither checked at publish nor recorded in the log. Guards read it, and every event records it. Delegation arrives in the descriptor as capabilities with `principal` set to the delegator. Attenuation is a capability where the fact is one of a few fixed values, and otherwise an object the guard reads — a `Delegation` with a limit and a lifecycle — which is recorded, revocable and the pattern ADR-0036 gives for governed delegation. Where a guard needs a referenced person, that person is an ordinary object.

A type may declare a **visibility** predicate over actor and object; every read, query, availability result and delivered event applies it, and an invisible object is **not found**, never blocked, so a verdict never discloses existence (ADR-0030). A cascaded transition is not subject to visibility: the parent's guards are its authority.

### 5.9 Declarations: composition and versioning

A type may **extend** a base declaration for attributes, relationships, invariants and derived attributes; state machines are bound by name and never inherited piecemeal, though a binder adds its own transitions and replaces the machine's creations (ADR-0064). A **family** is a base and everything extending it, and is a query target (ADR-0026).

Every type, machine, enum, sequence and evaluator carries its own **type version** in the text, and each publish installs the whole closure under one **declaration version**, per store and monotonic. Every object and event records the declaration version in force, which fixes the version of every type at that instant; a type version alone would not, since a machine or enum it depends on may have moved (ADR-0077). Because a type's behaviour depends on the machine, enums, sequences, evaluators and base types it uses, advancing any of those requires advancing every dependent type, which publishing enforces (ADR-0027, ADR-0056). A machine declares what it requires of its binders, and publishing verifies each one. Publishing is a designed operation with a report:

- **additions apply forward**; a new guard bites at the next transition;
- a **new invariant** is checked against live objects and every violation reported, then resolved by a mapping or admitted (§8);
- **removing a state** requires a mapping, applied as recorded migration transitions with provenance `migrated`;
- pending **proposals** the new version cannot express are invalidated (§9);
- the report also names which invariants compile on this backend, which transitions are sweepable, which derived attributes are queryable (§10), and every guard clause that is observing rather than enforced (§5.5);
- a flow changes only through a **`DeclarationChange`** (§9): drafted, approved by an actor other than its drafter — a person, when an agent drafted it — and published, its dry-run report being the impact report the draft carries (ADR-0085).

Import is this mechanism from version zero (§11). The printable rule set is per version.

A flow can **start minimal** (PRD V1): one whose transitions carry no guards, and whose types declare no invariants, formulas or datapoint kinds, publishes and runs. The publish checks stay whole, since they protect the guarantee; what a new flow lacks is knowledge of its own rules, which observations, labels, observing clauses and diagnostics supply (ADR-0085).

### 5.10 Tracking mode

A type declares one of three modes (ADR-0050, ADR-0067). **Serial-tracked** is one object per physical thing with its own lifecycle. **Quantity-tracked** is a stock object carrying counters that actions adjust. **Record** tracks no physical thing at all — a delivery, a service job, an audit entry — and carries no counters, which is what most of an operations platform holds: in the first consumer, three entities are serial-identified and twelve or more are records. A slot in a configuration declares which fill form it takes, so one kit may carry a serialised robot and a counted quantity of consumables. Reserve means the same in both: the thing is spoken for. Changing a type's mode is a new type, and objects move by supersession one at a time.

### 5.11 Datapoints: observations and labels

A **datapoint** is a time-stamped fact about a flow or an object (PRD §5). The engine keeps the flow-generated ones — transitions, refusals, overrides, time in each state and value — without anyone declaring them (§7). **Recorded** datapoints are observations and labels (ADR-0082).

An **observation kind** is declared once, `on` a subject type. It has typed fields — a numeric field may state its unit, which is printed and returned but never converted — a `recorded by` guard and an `occurred within` bound. The subject names the collection as a part: `part inspections : InspectionResult[] inverse subject`. The kind expands into what it is: a `record` type with one terminal state, an `owner` end to its subject, and a generated `record` creation carrying the `recorded by` guard. Recording is therefore an ordinary creation request, with an actor, an event, an idempotency key and a place in history, and there is still no second write path. A transition's outcome may record one with an ordinary `create` step, which is how a transition records what it decided on.

- **Two part rules bend for observations.** An observation is recordable directly rather than only via its whole, and may not be recorded on a subject in a terminal state, which is what "settled" means. As a born-final part it is implied `survives`.
- **Recording does not write the subject.** It neither bumps the subject's version nor conflicts with a concurrent transition on it; a recorded fact matters to the flow only through a guard that reads it.
- **Recording is validated.** Fields are typed, and a kind may declare invariants over its own fields, checked when it is recorded.
- **Nothing is edited.** A correction is a new observation of the same kind and subject, naming the one it corrects, which must not already be corrected; whoever may not record on the subject may not correct it either, and a correction may be recorded after the subject is finished, since it replaces a fact rather than adding one. Every read of the kind — a guard through the subject's part, or a metric — sees only the uncorrected ones, and `history` and `export` keep both (ADR-0095, ADR-0096). Erasure is the one exception, and removes only personal values: each kind has a generated `forget`, which the subject's `erase` runs and which may be requested on one observation alone.
- **A guard that aggregates over observations records, on its event, the observations it read**, so the decision can be re-evaluated from the record. Deriving the set from positions instead would be wrong wherever a position is not commit order (§7).
- **Catalogues of what to record are data.** An inspection checklist is objects a transition creates, and an observation references one, so changing a checklist needs no publish.

A **label** is a built-in observation available on every type: a normalised name, an optional note, and the subject's state when it was applied. It records what nobody has modelled yet. Labels are read by metrics and diagnostics and **never by a guard, an invariant or a visibility predicate**, since a rule over a vocabulary nobody declared changes meaning with a typo. A label becomes logic by promotion to a state, an observation kind or an attribute, which is a publish, and objects in flight then move by ordinary requests, so history says who moved each. A label's note is treated as personal. Who may label is a deployment capability, which a type may narrow.

### 5.12 Metrics and business exceptions

A **metric** is a declared formula across many objects or over time (ADR-0084). It names:

- a **source**: a type's current objects, an observation kind, or one of a type's three datasets — `intervals`, `transitions` and `attempts`;
- a **filter** in the expression language;
- **dimensions**: paths up to two hops, the kind of actor that requested a transition or recorded a datapoint, the declaration version, and time buckets over any timestamp, in UTC calendar time;
- a **value**, which may combine `count`, `sum`, `min`, `max`, `avg`, `median` and `percentile` arithmetically, over a window relative to `now` if it wants one;
- optional **flags**: named conditions on the value.

Metrics are **computed on read, over the rows the reader can see**: source rows are filtered by the source type's visibility predicate, datasets and observations inherit their subject's, and a path from a row to an object the reader cannot see yields absence at that step, so nothing of a hidden object is read. A stored reference is a value of the object that holds it, so its id is visible where that object is; what visibility protects is the referenced object. **Every result says whether it is complete**: complete when the reader can see every current object of each type the metric reads, and otherwise a value over the reader's own rows, marked as such. So every reader who gets a complete value gets the same value from one definition, no one mistakes a partial value for it (PRD C2), a reader who sees part of the data still gets metrics over that part (PRD M2), and none learns anything from what they may not see (PRD T5) (ADR-0096). A guard computes a metric over every row its source holds, as a type-scan guard reads every object of its type, because a rule whose verdict depended on who asked would not be a rule. Its value is recorded on the event and carried in a refusal, and shown to a later reader, or to the requester, only if their view of it would be complete (ADR-0095, ADR-0096). Where UC-10 would have a refusal name a value its requester may not see, T5 prevails, being a Must where UC-10 is hypothetical. A metric is retroactive by construction and stores nothing, which is why erasure needs nothing from one. A personal value may be counted, and never be a dimension, a flag's operand or an unaggregated output.

Every type gets **standard metrics** without declaring any. Open and finished are read from the one category with a meaning of its own, `closed`, which every vocabulary has: work is open until it enters a `closed` or a terminal state, and completes when it first does (ADR-0096).

- time in each state;
- throughput;
- work in progress;
- age of the oldest open objects;
- how often each transition is taken;
- refusal rates per rule;
- override counts per target state and declared reason, the import's own excluded;
- rework.

A type with an assignee also gets the metrics of §5.13. All of them are listed with the type, like any declared metric, so a person reading the rule set and an agent calling `metric()` see the same definitions.

A **threshold** in a data-driven rule or a condition is either a literal in the declaration, changed only by a flow change (§9), or an attribute on an object, such as a reorder point per model, changed only by that object's governed transitions (PRD L4).

A **business exception** — work overdue, ageing past a threshold, a service level breached — is a declared condition and not a new construct. Over one object it is a derived attribute over a stored operand and `now`, found by `query`. Across many objects it is a metric's flag. The store never sends one: a scheduler or an agent asks and acts (ADR-0012).

### 5.13 Assignment

An **assignee** is whoever is responsible for an object at a given time, as distinct from whoever acts on it (ADR-0086). A singular reference marked `assignee` names that responsibility. Its target type marks one `identity` attribute as the actor's id, so the engine can tell when someone other than the assignee acts. A type may mark more than one — an engineer-of-record and a reviewer — and each is a separate dimension. The marking adds no write path: assignment is whatever declared transitions write the reference, each with its own guards on who may assign and who may be assigned.

Every singular reference is tracked from its first write (§5.2), and the log can rebuild the intervals. So assignment history does not depend on the marking being present from the start; it depends on the assignee being written through the store from the start.

Every assignee dimension gets metrics without declaring any:

- open work per assignee;
- time unassigned;
- time to first assignment;
- time with each assignee;
- cycle time by the assignee at completion, and time in each state by whoever held the object then;
- handoffs;
- reassignment back to an earlier assignee;
- how often someone other than the assignee acts.

Choosing the assignee stays outside the store (§12).

## 6. Transition execution

A request names an object, a transition, its inputs and the actor, and optionally the version the caller last read, an idempotency key (ADR-0014), and a free-form `context` string naming the route it came by, recorded on the event. A creation request names the type and the creation transition instead of an object, and skips steps 1 and 3.

External evaluators and metrics named by any guard in the request are consulted **first, outside the transaction**, and their verdicts or values carried in with their as-of times (ADR-0049, ADR-0084). A metric's arguments are resolved against committed state now and again inside the transaction; a mismatch means the world moved, and the request consults again within its retry bound. The rest runs in one transaction at **serialisable isolation** (ADR-0039). On SQLite the transaction begins `IMMEDIATE`, taking the write lock at its first statement, so a request's guard reads and its writes are serialised with every other writer's (ADR-0090):

1. If the type declares visibility and the actor cannot see the object, refuse as `not found`.
2. If this actor's idempotency key has been applied before to this request, return the original result, marked as a replay, **whatever `expected_version` the retry carries**: a retry is the original request re-sent, and its version is the one the original was checked against (ADR-0041, ADR-0076). A key applied before to a different request is a fault, not a verdict (ADR-0077).
3. If an `expected_version` is given and differs, refuse with `stale`. This is possible only for a request that has not been applied.
4. Evaluate the parent transition's guards. A failure refuses the request, naming the clause, its object and its remedy class. A clause marked `observe` that fails refuses nothing; its would-be refusal is written to the attempt log in this transaction, linked to the event of step 8 (ADR-0085).
5. Apply the parent's outcome in full: its new state and its attribute writes, each value read at the moment it is applied (ADR-0054).
6. Then, depth-first in declaration order and over collection elements in ascending object-id order, which is creation order (§5.1), take each cascaded transition or creation in turn, **skipping a part already in a terminal state**, since its disposition has happened and that is the only skip on account of state; for each of the rest evaluate its guards **against the state produced so far**, then apply its outcome immediately (ADR-0038). Only-via transitions contribute their non-actor guards; other cascades run as the requesting actor. Any failure aborts the whole request and rolls back.
7. Check every invariant the written objects could violate — where a transition writes a part's `owner`, **both** the source whole and the destination whole count as written, so the source's invariants are re-checked and both have their part-event position stamped (ADR-0058), except those with an admitted violation still standing (ADR-0045, ADR-0054).
8. Increment each written object's version; update the interval index for every changed state and tracked value, using the occurred time where the transition is backdatable; record one event per transition, each cascaded event carrying the parent's event as its cause, and each carrying the request's serialisation retry count, its writing transaction, and the **read set** of the rules evaluated for it (below); insert each event row once, complete, in the same transaction — its position was allocated when its transition began, so nothing is completed later (ADR-0013, ADR-0083, ADR-0088, ADR-0089). Commit.

The **read set** makes every decision re-evaluable from the record (PRD L2). It lists every object a guard or invariant read, with the version it read — the object, related objects, an aggregate's elements, and what a type-scan matched, an empty match included — the capability each `actor.has` asked about and its answer, and the `now` the request fixed. Re-evaluating the rules over those versions, folded in each object's own strict order, gives the same verdict, without any global order (ADR-0088). A reader is shown a read set, and the evaluator verdicts and metric values an event records, only as far as their visibility reaches: an object they cannot see is left out, a metric value is left out unless their view of it would be complete — they can see every current object of each type the metric reads — and the read set says that something was withheld, so a partial record is never mistaken for a complete one. An auditor who must re-evaluate every decision reads as an actor that can see everything (ADR-0095).

**A request that does not apply** — refused by a guard, `stale`, `not found`, `not requestable`, over a limit, violating an invariant, naming no declared transition, or reusing a key — is written to the **attempt log** in its own short transaction after the rollback. The row holds who asked, their kind, the transition, the verdict, the failing clause with its remedy class, that clause's read set, and the metric values and evaluator verdicts it consulted, and never the inputs (ADR-0083, ADR-0088, ADR-0096). Rows are pruned after the deployment's retention period, and the prune adds each day's counts to a permanent daily rollup in the same transaction, so refusals stay countable per week over the whole history (PRD T3). A crash between the rollback and that write loses one row of evidence and no history.

Row locks are taken as objects are reached, for contention rather than correctness: serialisable isolation is what makes a guard's reads safe, including reads of objects the request never writes. A serialisation failure is retried to a declared bound and then refused with `stale`. **A contended row does not queue** on PostgreSQL: the second request waits for the first and then fails with a serialisation error, which the retry absorbs, and the lock only makes the loser fail at its first touch of the row rather than after doing its work (ADR-0090, probed). On SQLite a contended request waits out the busy timeout, and a timeout exceeded counts as a serialisation failure, so either backend answers a race with a verdict, never a fault. The sequence mint draws from a pool of its own, so a burst of creations cannot starve the request pool (ADR-0090).

An **asserting** transition (§8) differs in two ways: step 4 evaluates only its own guards, which are its capability and reason requirements rather than the type's; and step 7 refuses on any invariant violation the request did not explicitly admit.

## 7. History, events and delivery

**Current state is stored**, one row per object with its attributes, state, version, declaration version and last event. **The event log is the permanent history**, written in the transition's transaction; a fold of the log reproduces the row, never the reverse (ADR-0033). A per-attribute index of the event that last wrote it makes `changed_since` a constant-time comparison (ADR-0048), and the same index, keyed by part relationship, does it for the half that reaches a composition: one position per relationship rather than one per whole (ADR-0057, ADR-0082).

Beside the log the engine keeps two more records (ADR-0083):

- **The interval index** records how long each object held each state and each value of each tracked attribute and reference: entry and exit by position and by occurred time, the transition that set it, its actor's kind, and the declaration version. The log can rebuild every interval it recorded, so it is an index and not a second source of truth, and the store maintains **no projection the log cannot rebuild**. Intervals the import supplies from the legacy system's history are not a projection: they are imported history, read-only and marked legacy like the legacy entries of §11, and the harness checks them for shape rather than against the log: none overlaps another of the same dimension, and the last ends where the object's first recorded interval begins (ADR-0096).
- **The attempt log** records every request that did not apply, and every would-be refusal of an observing clause. It holds no inputs. It is not history: `history` does not return it, and subscriptions do not deliver it. It is pruned after a retention period the deployment sets, with its counts kept permanently.

Every event carries `changes_state`, true when from and to differ, and its **provenance**: actor, principal, `context`, cause, declaration version, the version of the taint check that passed the write, so a later audit can tell which rules were in force (ADR-0051), the time a backdated change happened beside the time it was recorded, the number of serialisation retries its request took (ADR-0083), its writing transaction (ADR-0089), the read set of its rules (ADR-0088), and a `source` of

| Source | Produced by |
|---|---|
| `observed` | An ordinary transition |
| `asserted` | An assertion, including import (§8, §11) |
| `migrated` | A declaration migration (§5.9) |
| `corrected` | An action that records a corrected value for an earlier mistake, with a reason; the earlier event is not rewritten |
| `erased` | An erasure (§8) |

The log is **never pruned**; retention is archival tiering that keeps events readable and reachable by erasure. Events are strictly ordered per object and causally ordered across a cascade. A global position is monotonic but not a commit order, so the log is read by a **settled cursor**: a reader returns only events whose writing transaction had finished when its snapshot was taken — on PostgreSQL, those below the snapshot's `xmin` — in (transaction, position) order, and hands back the last pair as the cursor. A later commit always sorts after any cursor already handed out, so a consumer acknowledging a cursor never skips an event; on SQLite, whose writers are serial, the cursor is the position (ADR-0089, probed). `pull` and `export` both return it, and `acknowledge` takes it.

```
            recorded transition
                    │
         permanent ordered log        written in the same transaction
                    │
        ┌───────────┴───────────┐
     pull                     push
  consumer reads with      delivery worker posts to
  a position               a subscription's endpoint
```

A **Subscription** is a built-in object holding a filter over a type or family, transition names and `changes_state`; a delivery endpoint the worker posts to; lag thresholds; and a lifecycle of `active → revoked`. Delivery progress is **runtime state, not object state**: the acknowledged position is stored beside the idempotency record, is not versioned and emits no events, so a poll never lands in the permanent log. Lag and death are **derived** from that position and the thresholds, so nothing has to move a subscription into them, and ADR-0012 is untouched. A subscription may be revived at any cursor, because nothing is pruned (ADR-0034, ADR-0043).

Delivery is **at-least-once with idempotent handling**. Exactly-once *effect* comes from deduplicating on the event id at the receiver, and transitions accept an idempotency key so the common case, an event causing a transition, deduplicates centrally and records causal lineage (ADR-0014). A subscriber acts as an actor, and visibility applies to what it is delivered.

**Time.** `now` is a value in guards. A time-driven transition is an ordinary one, `expire: ACTIVE → EXPIRED, guard end_date <= now`, and ObjectKeeper never requests it. A scheduler above asks the availability query and requests each answer with a periodic idempotency key (ADR-0022). The guard is the timing rule, in one place. ObjectKeeper initiates nothing (ADR-0012).

## 8. Ends of life, repair and erasure

**Deletion** is a terminal transition gated on there being no live object referencing this one; parts cascade, references block with `dependent` naming them. Deleted objects stay readable by id and in history and take no further transitions (ADR-0024).

**Supersession** ends an object in a terminal state that names its successor, which may be an existing object or one the same outcome created. Id and history stay, the read surface follows the pointer on request, and references are re-pointed only by declared cascade. It is how an object changes kind: moved, converted, merged (ADR-0028, ADR-0046). A duplicate is closed with a reference, not superseded.

**Assertion** is the only way to set state without satisfying the type's guards, and it is declared, not ambient (ADR-0040). An asserting transition names the states it may assert; is addressed by name with the target state as a constrained input, so §5.4's by-name rule holds; carries its own guards, at minimum a capability and a mandatory reason; and records what it stepped over. A type that declares none cannot be repaired this way at all. Invariants are not bypassed by default: an assertion marked `may admit` accepts an explicit list of the invariants it knowingly violates, and each **admission** names an object and an invariant, suppresses that invariant's check for that object, and is discharged automatically when the invariant holds again (ADR-0054). Import and declaration migration use a **built-in** assertion gated on a deployment capability, so no type is unimportable by omission. `exceptions(type)` lists the objects whose state an assertion last set, and those holding an admitted violation. An import's state change is recorded as `imported` in the object's `state_source`, while its event keeps provenance `asserted`, so the list shows repairs and admissions and is not buried by the legacy population (ADR-0083, repairing D205).

**Erasure** replaces the values of declared personal attributes with absence, on the object and in every event that carried them, deletes the content of a file once no unerased reference to it remains — so an identical file another object holds survives (ADR-0087) — keeps the event skeleton, records that it happened, and is irreversible (ADR-0031). It runs from any state, terminal included, since erasure requests arrive for closed accounts; it is the one carve-out from the rule that a deleted object admits no further transitions (ADR-0056). The marker is absence rather than a sentinel, so it collides with no unique constraint and reads as unknown in the expression language, which makes a guard over an erased value fail rather than pass (ADR-0051). It is the one place the log is rewritten, and it is not deletion. It also reaches the legacy entries attached to the object, redacting every payload field the import mapping did not list as kept; the inputs of every proposal that targets the object or whose inputs flow into its personal attributes, invalidating the pending ones (ADR-0087); and every event payload that carried a personal input, whether or not that input was written to an attribute, an input being personal by marking or by flowing into a personal attribute (ADR-0078). It reaches observations through each one's generated `forget`, which the subject's `erase` runs without a declared step, redacts label notes, which are treated as personal, and redacts a personal enum's values in the interval index (ADR-0096). A file deletion that fails is retried by the next erasure request, and `diagnostics` of the erased object's type lists it, so no background process is needed (PRD N2). The attempt log holds no inputs, a metric stores nothing, a read set holds only ids and versions, and the export omits every personal value, so none of them needs erasing (ADR-0082 to ADR-0084, ADR-0088, ADR-0094).

Erasure follows the person, not only the id (ADR-0087). It erases every member of the object's **supersession chain**, predecessors and successors, each through its own type's `erase`, since superseding declares that the successor continues the same thing, and publishing rejects a type that holds personal attributes, takes part in supersession and declares no `erase`. It redacts, in every **caller's event** up the erased object's cause chain, the inputs that flowed into the erased object's personal attributes, which check 10's taint analysis identifies. And it redacts every proposal targeting an erased object or carrying inputs that flow into one. What it cannot reach is a personal value typed into another object's free text, which no declaration marks as personal, and a copy a consumer made outside the store; those are the known limits.

## 9. Built-in types

`Subscription` (§7), `Proposal` and `DeclarationChange` are built in and not user-definable, and `label` is a built-in observation kind (§5.11).

A **Proposal** holds a request a caller lacked authority for: target, transition, inputs and proposer, with a lifecycle `pending → executed | rejected | withdrawn | invalidated`. Approving it executes the recorded request **as the approver**, with the approval as cause, and every guard is re-evaluated at execution under the **current** declaration, actor guards included. An approval therefore succeeds only if the approver holds whatever authority the transition's guards demand; nothing reads a capability off a guard in advance, because the guard itself is the test. A guard that fails leaves the proposal pending with the verdict attached; a declaration that can no longer express the request invalidates it (ADR-0036, ADR-0044). Expiry is derived, not driven.

A **DeclarationChange** is how a flow changes (ADR-0085). Any actor holding the drafting capability creates one, carrying:

- the source;
- the dry-run publish report, which is the impact report;
- links to its evidence.

An actor other than the drafter, holding the publish capability, approves it — a person, when an agent drafted it — and approval publishes it. The publish recomputes the impact report inside its own transaction, and if the live objects it would affect have changed since the report the approver read, it is refused and the change returns with the new report to be approved again (ADR-0096, PRD F7). It is superseded when the installed version moves under it. The engine never drafts one; an agent reading `diagnostics` usually does. Being an object, every step is attributed, and the publish event has an object to belong to (repairing D212).

## 10. The read surface

One API. Every operation takes an actor and applies visibility (ADR-0037).

| Operation | Returns |
|---|---|
| **get**(id, follow?) | The object; optionally following supersession to the live successor |
| **query**(type or family, filter, order, cursor, fields) | A page of objects. The filter uses indexed attributes, state and category, derived attributes that are themselves indexable, and the entry time of a tracked value's current interval (§5.13, ADR-0084); `fields` defaults to the type's summary set |
| **lookup**(source, value) | The object whose external identifier for that source matches |
| **availability**(id) | Each requestable transition with its three-way availability, verdict and declared input schema. Only-via transitions are not listed |
| **available**(type, transition, cursor) | The objects for which that transition is available now. Requires the transition to be **sweepable**, meaning its guards decompose into an indexable prefilter and a residual; one that is not is refused rather than scanned (ADR-0048) |
| **check**(id, transition, inputs) | The verdict a request would receive, simulating the same sequence internally. Lock-free and side-effect-free, so it is advice and not a reservation, and its verdict is **partial**: it names the external guards it did not evaluate (ADR-0054) |
| **history**(id, follow?) | Events with provenance, legacy entries, and optionally the combined timeline through supersession |
| **exceptions**(type) | The objects whose state an assertion last set, the data import excluded, and those holding an admitted invariant violation; what keeps the escape hatch of §8 reviewable |
| **declaration**(type, version?) | The inspectable rule set at a declaration version, the number an event records; rendered also as readable text and as agent tool schemas |
| **pull**(subscription, cursor) | A page of events per the subscription's filter, and the settled cursor (§7) |
| **batch**(requests) | N independent requests, **each in its own transaction**, returning N verdicts in order |
| **metric**(name, filter?) | A declared or standard metric computed over the rows the reader may see, narrowed by the filter before it aggregates: each group with its dimensions, its value and the flags that hold, in UTC calendar time, and whether the result is complete (§5.12, ADR-0084, ADR-0096) |
| **export**(source, cursor) | A page of JSON lines from the log, a type's `intervals`, `transitions` or `attempts`, or an observation kind, in settled-cursor order, under the reader's visibility, with every personal value omitted, and the next cursor (ADR-0094) |
| **diagnostics**(type) | Where the flow and reality disagree: transitions and states unused in a window, the clauses that refuse most, states entered mostly by override, labels by frequency and state, the longest waits, rework and reassignment loops, work done by non-assignees, backdating rates, and observing clauses' would-be refusals (ADR-0085); and file deletions still pending after an erasure of the type's objects (ADR-0096). Computed like a metric, over what the reader may see, and saying whether it is complete |

Neither `available`, `check` nor `availability` calls an external evaluator; each says which guards it did not evaluate. `check` and `availability` do compute internal metrics, since no third party is involved. Time-dependent derived attributes are queried by filtering the stored operand the expression compares against `now`, since a clock-dependent value cannot be indexed. There is **no atomic batch**: atomic multi-object semantics are declared cascades (§5.4). Beyond the indexes a type declares and the interval index (§7), the store maintains no projection; where a scan is too slow and the type is `quantity`-tracked, it declares a counter and the cascade that maintains it. That remedy is **not available to a `serial` or `record` type**, which is open question 10 and is why a ledger balance has nothing tying it to the entries that produced it (ADR-0072).

Writes go through the request of §6. Operational calls that are not object operations are `acknowledge`, which advances a subscription's progress (§7), and `publish`, which installs the declaration version of an approved `DeclarationChange`, and whose dry run is the impact report a draft carries (§9).

## 11. Import and migration

The first consumer's production data is ported by a designed path (ADR-0015). Every imported object receives a new id and keeps its legacy key as an external identifier with source `legacy`; cross-references are re-pointed through that mapping (ADR-0018). Imported state is written by the built-in assertion with provenance `asserted` (§8). The importer evaluates the declaration and reports every violated invariant and unsatisfied structural guard, grouped into classes, and a person decides each class: cleaning the source, supplying a value in the mapping, admitting an invariant violation, or excluding the objects. A guard class cannot be admitted, since an admission names an invariant (ADR-0077). Legacy history is preserved as read-only entries of kind `legacy`, which carry their original payload and are returned by `history` alongside ObjectKeeper's own events. Soft-deleted rows land in the type's deleted state. Files are content-addressed into the deployment's blob store. Cutover cannot be dual-write, because there is one write path. An import's state change is recorded as `imported` (§8). The mapping may supply each object's current-state entry time, and earlier intervals from legacy history, marked as legacy. That covers state and assignment alike: the first consumer's past reassignments come from its audit trail's before-and-after snapshots (ADR-0083, ADR-0086).

Import writes every row complete in one pass: every object's id is assigned from the legacy-key mapping before any row is written, so every reference resolves at insert, and a cycle of required references commits with its foreign keys checked at commit (ADR-0077). Cutover is **staged by object type**, with one owner per type at any moment. A type this store holds and does not yet own is declared **`mirror`**: attributes, states and an external identifier, no transitions, written only by the import path; it is cut over by a version advance that removes the marking and changes no object's id (ADR-0075). The stage order counts two kinds of edge: a reference, so a referrer migrates no later than what it references, and a legacy transition's write, so a type the legacy system writes as a side effect migrates no earlier than the writer; a strongly connected component of the combined graph is one stage (ADR-0093). The marking is for cutover only. A type another system owns for good — the first consumer's customer, which Xero owns — is an externally owned type (§5.5), and cutting the customer over means replacing the marking with that type and its sync-driven transitions (ADR-0080).

## 12. Scope boundaries

| In scope | Out of scope |
|---|---|
| Preconditions, permissions, approvals, visibility | Formulas that need data or rules the store does not hold: tax tables, pricing engines |
| Required information per transition | External effects: raising an invoice, sending mail, sending an alert |
| Cross-object conditions and declared cascades | Selection: which engineer, which unit, who is next |
| Type-level invariants, derived views, any declared formula over the store's own data | Long-running process orchestration |
| History, provenance, the event log, delivery | Deciding *when* a transition should happen |
| Flow data: intervals and the attempt log; recorded observations and labels; assignment; the export | Open-ended exploratory analysis, which the export feeds |
| Declared and standard metrics, business exceptions as declared conditions, diagnostics | High-rate machine telemetry; working-hours calendars, in the first release |
| Import, migration, supersession, erasure; governed flow change | Human UI, agent framework, blob bytes |

Preconditions, permissions and the measurement of a flow have the same shape across domains; formulas that need outside rules, effects and selection are where domains differ irreducibly. The store decides, records and measures; consumers choose and cause (ADR-0007, ADR-0081).

## 13. Known limits

The guarantee is that **no state change bypasses the enforced guards except through a capability-gated, recorded assertion — either one the type declares, or the built-in one import and migration use**, and that the objects holding asserted state or admitted violations are queryable at any time (§8, §10). A guard clause on trial is not enforced and guarantees nothing, and the publish report lists every one (§5.5). A guard reading a metric is enforced against the value it was given, as of the time it was computed and not at commit, as for an external evaluator. It cannot guarantee that **the guards say what was meant**. Risk relocates from scattered implementation bugs to specification gaps in one readable place. Hence two obligations: the rule set for a type must be printable for review by someone who knows the process, and acceptance testing must be adversarial in the threat model's sense, a fallible actor that guesses, retries and skips steps, trying every route to an invalid state. The harness drives real connections through the injected connection source, releasing one statement at a time in an order its seed chooses, so a concurrent failure reproduces from its transcript (ADR-0091).

The mediated property is an operational commitment. Anything else that can reach the database, a migration script, `psql`, a reporting job, voids it. Repair is a declared assertion, and the data import is its first use at scale.

Concrete cases the model does not cover, or covers with a caveat, are catalogued in [`design/edge-cases.md`](design/edge-cases.md). They include in-place type change, gapless sequences, hot-row throughput, multi-region stock, acyclicity of self-referential hierarchies, unmerge, local-time rules, and the several things a declaration is refused at publish for.

Two things measure less than they appear to. A metric is only as good as the recording behind it: a change recorded late distorts time in state unless its transition is backdatable, and backdating is itself reported. And references to a set of objects, such as a team, are not tracked, so assignment to a team has no intervals yet.

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
| **Guard** | An expression that must hold for a transition, carrying a declared remedy class; enforced unless marked `observe`. |
| **Observing clause** | A guard clause marked `observe`: evaluated, never enforced, its would-be refusals recorded in the attempt log. |
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
| **Actor** | The descriptor the consumer supplies with a request: id, kind, principal, capabilities; nothing else is readable, and a fact about an actor is a capability or an object (ADR-0079). |
| **Visibility** | A declared predicate every read applies; failing it means not found. |
| **Object id / business identifier / external identifier** | Store-assigned identity; a consumer-minted meaningful value such as a serial; a value another system owns. |
| **Sequence** | A named, scoped, monotonic counter that mints business identifiers; not gapless. |
| **Tracking mode** | Whether a type is serial-tracked, one object per thing; quantity-tracked, counters on a stock object; or a record, tracking no physical thing at all (ADR-0067). |
| **Mirror** | The cutover marking: a type the legacy system still owns, held here with no transitions and written only by import until a version advance removes the marking (ADR-0075, ADR-0080). |
| **Externally owned type** | An ordinary type another system owns for good, with an external identifier, whose sync-driven transitions the sync consumer requests (ADR-0080). |
| **Type family** | A base declaration and everything extending it. |
| **Declaration version** | The number of a publish: one per installed closure, per store, monotonic; recorded on every object and event, and what `declaration(type, version)` takes. |
| **Type version** | The number a type, machine, enum, sequence or evaluator carries in its text; advancing one advances every type that depends on it (check 22). |
| **Settled cursor** | A (transaction, position) pair up to which every writing transaction had finished when the reader's snapshot was taken; the furthest a pull or export consumer may acknowledge, and never ahead of a late commit (ADR-0089). |
| **Read set** | What an event records of the rules evaluated for it: every object read with its version, capability answers and the fixed `now`, so the decision re-evaluates from the record (ADR-0088). |
| **Publish** | Installing a declaration version, as the approval of a `DeclarationChange`, with a report of migrations, admissions, compiled invariants, sweepability and observing clauses. |
| **DeclarationChange** | A built-in object through which a flow changes: drafted with its evidence and impact report, approved by an actor other than its drafter, published. |
| **Deletion / supersession / erasure** | Ending an object; continuing it as another; removing personal values from it and its history. |
| **Provenance** | What an event records about itself: actor, principal, context, cause, declaration version, source, and where they apply the time a backdated change happened and the serialisation retries its request took. |
| **Idempotency key** | A caller-supplied key that makes a repeated request return the original result rather than acting twice. |
| **Subscription** | A built-in object holding a filter, an endpoint and lag thresholds; its progress is runtime state, not object state. |
| **Proposal** | A built-in object holding a request the caller lacked authority for; an authorised actor's approval executes it as themselves. |
| **Sweepable** | A transition whose guards decompose into an indexable prefilter and a residual, so `available` can answer for it. |
| **Effect** | Something caused outside ObjectKeeper; out of scope. Consumers cause effects by observing events. |
| **Measured** | The fourth core property: every flow produces data about itself, users record their own, and everyone reads both through the same declared metrics. |
| **Datapoint** | A time-stamped fact about a flow or an object: flow-generated, recorded, or derived by a formula (PRD §5). |
| **Observation** | A recorded datapoint: a born-final part of a declared kind, recorded by a creation request. |
| **Label** | A built-in observation recording what nobody has modelled yet; read by metrics and diagnostics, never by a rule. |
| **Tracked** | Of an enum attribute or a singular reference: how long an object held each of its values is kept, without its author declaring it. |
| **Interval** | A span during which an object held one state, or one value of a tracked member; kept in an index the log rebuilds. |
| **Attempt log** | The record of requests that did not apply and of observing clauses' would-be refusals; not history, without inputs, pruned after a retention period. |
| **Backdatable** | Of a transition: it accepts when a change actually happened, within a declared bound, and the event keeps both times. |
| **Metric** | A declared formula across many objects or over time, computed on read over the rows the reader may see, and saying whether that is all of them; standard ones exist for every type. |
| **Flag** | A named condition on a metric's value. |
| **Business exception** | Work deviating from what its flow expects — overdue, ageing, a service level breached — declared as a derived attribute over one object or a metric's flag. |
| **Assignee** | Whoever is responsible for an object at a given time, named by a reference marked `assignee`. |

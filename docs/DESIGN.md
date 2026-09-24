# ObjectFlow — Design

**Status: under review.** Every finding of every review is recorded in [`design/defects.md`](design/defects.md), 378 entries and five cosmetics; 378 are closed and 0 are open. The PRD is at revision 6, with a proposed revision awaiting the author (PRD §12).

Two kinds of acceptance appear below. The author accepts a decision themselves; or a decision is taken at the author's direction and marked Accepted in its own file, with the author's own acceptance still to come.

**Accepted by the author:**
- ADR-0081 to ADR-0096, on 2026-09-23. ADR-0087 to ADR-0096 were decided at the author's direction to cover every PRD requirement ([`design/traceability.md`](design/traceability.md)), and accepted after each was checked against this design.
- ADR-0104, on 2026-09-24: the engine is the governed core.
- ADR-0107, on 2026-09-24: the project is named ObjectFlow, superseding ADR-0011.
- ADR-0108, on 2026-09-24: a cascade is declared clearly enough not to surprise, and is shown where it lands. The principle is the author's; the mechanism was written at the author's direction.
- ADR-0065 to ADR-0073, which the author ruled on.

**Decided at the author's direction, the author's own acceptance pending:**
- ADR-0097 to ADR-0101, from a five-slice review of the whole record and its verification, 2026-09-23.
- ADR-0103, from reviewing the design against the PRD with the unit's journey, 2026-09-24.
- ADR-0105 and ADR-0106, from a five-slice review of the record against PRD revision 5, 2026-09-24.

**Proposed and awaiting the author:** ADR-0102, which writes the unit's whole journey from the first consumer's production code.

**Awaiting review:** ADR-0019 to ADR-0064, ADR-0074 to ADR-0080 and the six implementation documents of 2026-09-09, with ADR-0078 to ADR-0080 first, being the three decided at the author's direction rather than by the author.

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

**Amended for ADR-0081 to ADR-0106.** Those decisions changed all six documents above and the cutover plan, and each has been amended and says so in its header:
- **the syntax:** seven constructs, checks 54 to 61, the request rule, `length` and formatted serials;
- **the schema:** the attempt log, the interval index, the observation and label tables, the file reference index, the settled cursor and each object's `recorded_from`;
- **the API:** `metric`, `diagnostics` and `export`, and a remedy class on every verdict;
- **the renderers:** the observation and metric tools;
- **publish and import:** the `DeclarationChange`, checked mappings with `admit`, and imported entry times;
- **the harness:** its checks on intervals, read sets, refusals and erasure, and the barrier that interleaves requests.

Where a rule appears in both, this document states what it means and the syntax document states how it is written. That division exists because it failed twice: the outcome grammar was restated here and had gone stale in three of six lines, and a later audit found thirty-two rules stated in both documents of which six had drifted to the superseded version. When the two disagree, the syntax document is the one with a checker.

Each decision, with the alternatives it rejected, is an ADR in [`adr/`](adr/); the case studies that shaped it and the catalogue of what it deliberately does not cover are in [`design/`](design/).

## 1. Purpose

A reusable foundation on which people and AI agents **define flows and collect data**, so that the business logic built on top can be driven by data, whichever kind of actor drives it (PRD §1).

**A flow** is what PRD §5 defines, and nothing narrower:
- the object types, their lifecycles, transitions and rules;
- the datapoint kinds recorded about them;
- the formulas and conditions declared over them.

All of it is versioned together and changed only by publishing (§5.9), declared alongside the data rather than implemented in each application that uses it.

Its data is what it produces by running — every transition, refusal, override and change of hands — together with what users record and what their formulas compute from both. Metrics over that data let a flow start imperfect and **converge**, with the engine showing where the flow and reality disagree (ADR-0081).

The objective beneath all of it is **trust under delegation**: hand a system to people who are not supervised and to AI agents whose behaviour cannot be fully predicted, and know that nothing they do can put the data into a state that has to be cleaned up afterwards.

As a rule, **governed state — an object's state and attributes — changes only by one of PRD F2's four routes, each recorded**:
- a named transition whose enforced conditions are all met;
- an override (§8);
- a flow change's declared mapping (§5.9);
- the erasure of a personal value (§8).

A condition on trial is not enforced (§5.5), and nothing else writes (ADR-0105).

The threat model is **mistakes, not malice**. Actors are trusted but fallible: a person who skips a step, an agent that retries a timed-out request, a script that sets a flag directly. The design defends against those by construction. It does not defend against an actor holding database credentials who intends harm; that is an access-control and deployment concern (§13).

**A governed core.** ObjectFlow is the core that operations applications are built on, not the application (PRD §1, N6; ADR-0104). How a flow is managed — who requests which transition and when, chasing, scheduling, assigning, notifying, worklists and screens — and how its datapoints are put to use are built in upper-layer applications, which act through the same requests and rules as any caller and have no path of their own. The core answers the questions they ask by query:
- what may be done now (§10);
- what is due, overdue or ageing (§5.12);
- what waits, and on whom (§5.13).

Genericity is a requirement from the start. The first consumer is the author's own operations platform (§4); it is a first customer, not a domain to specialise for.

## 2. Position in the stack and deployment shapes

```
agents · operations applications ·       upper-layer applications
human UI · schedulers · event relays
─────────────────────────────────────
ObjectFlow                              the governed core
─────────────────────────────────────
PostgreSQL / SQLite                       storage

siblings:  external systems of record (e.g. Xero)
           agent frameworks
           file storage (S3-compatible or filesystem)
           exploratory analysis tools, fed by export
```

ObjectFlow is a layer over a database, not a database (ADR-0001). The core is **library-shaped**: a call goes in, guards evaluate, a transition and its record come out; there is no scheduler, timer or background process (ADR-0012), and the core sends nothing.
- **A deployment becomes a service** by exposing the API over a transport (ADR-0037), and in no other way.
- **Posting events to an endpoint** is a relay: an upper-layer application that pulls, posts and acknowledges as its own actor (ADR-0105). Workflow orchestration of a Temporal shape is an upper-layer application too.
- **Measurement adds no background process.** Metrics are computed on read, and a business exception is a condition an upper-layer application asks about (ADR-0081, ADR-0084).

**The first release is sized for the first consumer** (PRD N3): a few hundred live objects per type and a few thousand audit rows over the system's life, an operations rate and not a telemetry rate. Its event and datapoint rates are measured against production before cutover ([`design/first-consumer-cutover.md`](design/first-consumer-cutover.md) §5). The orders-at-volume case study stays a design reference, not a target.

The core is **synchronous**: a request runs to completion on its caller's thread, and an asynchronous service wraps it on a thread pool. The store takes its **id source, clock, connection source and evaluator source** as injected dependencies, which is what lets the adversarial harness interleave requests deterministically against a real database and answer every external guard the same way on every run (ADR-0077, ADR-0091, ADR-0100).

## 3. Core properties

| Property | Meaning | Why it matters |
|---|---|---|
| **Mediated** | No path to governed state but PRD F2's four, each recorded: a declared transition whose enforced guards pass; an override — an assertion the type declares (ADR-0040), the built-in one the import uses (ADR-0054), or an admission; a flow change's migration, authorised by the publish's approval (§5.9); and an erasure (§8, ADR-0105). There is no second write path (ADR-0042): recording a datapoint is a creation request (ADR-0082). A clause on trial is not enforced, and guarantees nothing (§5.5) | The peace-of-mind guarantee |
| **Declared** | What is allowed is data, inspectable at runtime, not code (ADR-0010) | UI, API and agent tools are projections of one declaration |
| **Recorded** | Every change and every recorded datapoint is attributed, caused and reconstructable | Trust after the fact, not only while watching |
| **Measured** | Every flow produces data about itself — transitions, refusals, overrides, time in each state and in each assignee's hands — users record their own, and everyone reads both through the same declared metrics (ADR-0081) | A flow can be evaluated and improved from evidence, and a person and an agent see the same numbers |

An ORM abstracts *mechanism*: it hides SQL and faithfully executes whatever the caller asks. This layer abstracts *authority*: what may change, when, by whom, and what follows.

## 4. First consumer and case studies

The first consumer is an extended version of the Weston Robot inventory management system: an operations platform covering procurement, allocation, pre-delivery inspection, leasing and customer identity mirrored from Xero. The existing system is in production at `~/RduWs/wr_inventory_management`, already contains a hand-built version of every concern below, and will be rebuilt on ObjectFlow with its production data ported and preserved (ADR-0015). The PRD's use cases are drawn from it wherever they can be.

The model was tested against five further shapes, each recorded with what it forced: issue tracking, customer records, orders at volume, approvals with bookings, and a payments ledger, in [`design/`](design/). The ledger is the one written after the model rather than before it, and it is the only case that is high-volume, short-lived and money-carrying; four of the thirteen questions the author ruled on came from it.

The first consumer's own flows are written in two places:
- **The unit's whole journey**, from request to service and loan, is a checked module in [`design/unit-journey.md`](design/unit-journey.md) (ADR-0102). It supersedes the unit lifecycle of the earlier walkthrough, and [`design/flow-review.md`](design/flow-review.md) reads it as an operations manager would.
- **The rest of the first consumer's lifecycles** are in [`design/first-consumer-walkthrough.md`](design/first-consumer-walkthrough.md).
- **A returns flow that starts with no rules**, published twice — first bare, then with a label promoted to a state, a rule that reads a metric and a derived value read under visibility — is a checked module in [`design/returns-module.md`](design/returns-module.md). It holds what the worked example's cases need beyond the unit's journey.

All of these exist to test the design, not to specify the first consumer: each is judged by the mechanisms and requirements it puts under load, and by the defects it finds.

## 5. The model

```
ObjectType            declared under a version (§5.9); may extend a base;
                      declares a tracking mode, serial, quantity or record (§5.10);
                      or is marked mirror: held here, owned elsewhere, written
                      only by import, no transitions but erase (§11)
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

Metric                a declared formula across many objects, grouped and windowed
                      as it declares, with flags, computed on read (§5.12); a
                      formula over one object is a derived attribute
```

Every request additionally carries an **actor** the caller supplies (§5.8). Beside the objects, the engine keeps an **interval index** of how long each object held each state and each tracked value, and an **attempt log** of the requests it did not apply (§7).

### 5.1 Objects and identity

Every object carries a store-assigned, globally unique, immutable, opaque id, assigned at creation and at import (ADR-0018). It is a UUIDv7, so ascending id order is creation order, which is what makes a cascade's iteration order deterministic (§6); its embedded time is never read back as a fact about the object, which is what `created_at` is for (ADR-0077). Every object has `.created_at`, when its creation happened — the occurred time of a backdated creation, the legacy creation of a ported one where the port supplies it, and otherwise the earliest time the port knows of the object, which is then marked as undated (§11) — and `.created_by_kind`, the kind of actor whose request created it, both readable by rules and metrics (ADR-0096, ADR-0106). Business identifiers such as a serial or a ticket key, and external identifiers such as a Xero contact id or a legacy primary key, are ordinary attributes, never identity. A business identifier may be minted at creation from a named, scoped, monotonic sequence, which is monotonic but not gapless (ADR-0029). An object never changes type; when it must continue as something else it is superseded (§8).

### 5.2 Attributes

An attribute has a declared type: string, boolean, integer, decimal with scale, money with currency, timestamp, duration, identity, enum with declared options, event reference, file, or a set of any of these. A **reference is not an attribute type**; it is declared as `ref`, `part` or `owner` (§5.3), though an input may be reference-typed.

**Every attribute is written only by a transition outcome** (ADR-0042). There is no ungated write. A type makes attributes editable by declaring an action for them, which costs one declaration and gives the edit a guard, an actor and an event. Attributes may additionally be marked:

- **identifier**, minted from a sequence (ADR-0029);
- **external**, naming the system that owns the value, with uniqueness per source automatic (ADR-0037);
- **personal**, subject to erasure (§8). A personal value may not be written into a non-personal attribute, and publishing rejects a declaration that does (ADR-0051);
- **indexed**, which is what query filters, type-scan predicates and visibility predicates may use (ADR-0048);
- part of the **summary** set, the attributes a listing returns by default.

**Derived** attributes are named expressions, never stored, evaluated on read; their dependency graph must be acyclic (ADR-0021, ADR-0047).
- **A derived datapoint.** A derived attribute for one object is what the PRD calls a derived datapoint. It may read the object's own intervals and transitions (§5.7), so the handoffs on one job are a derivation of that job (ADR-0106).
- **Read under visibility.** One that aggregates over other objects is read under its reader's visibility wherever it is read — `get`, `query` or a metric row — counting only what the reader can see. A read names it as **partial** where the reader cannot see every object of a type it reads, as a metric says it is not complete (ADR-0106).
- **A guard reads every object.** A guard reads a derivation over every object, since a rule's verdict may not depend on who asked (ADR-0096).

A **file** attribute holds a content-addressed reference to a blob in external storage with its hash, size, media type and provenance. ObjectFlow never reads, streams or serves the bytes; its one byte-level operation is deleting them during erasure (ADR-0017, ADR-0041). A deployment must disable its blob store's own lifecycle expiry, since the log is permanent and a reference outlives any expiry policy.

Every **enum** attribute, like every singular stored reference (§5.3), is **tracked**: how long an object held each value is kept in the interval index (§7) without its author declaring which (ADR-0083, ADR-0086). Numbers, free text and sets are not tracked, a set of enum values included; their changes remain in the events (§13). Every tracked member has an interval from its object's creation, absence included, so an object never assigned counts as unassigned from the start; one tracked from a later publish has one from that publish (§7). A write that leaves a tracked value unchanged opens no interval, so counting a member's intervals counts its changes (ADR-0106). A personal enum is tracked like any other, and erasure redacts its values in the interval index as it does in the events the index is rebuilt from (ADR-0096, ADR-0101).

### 5.3 Relationships

Relationships are **composition**, meaning exclusive membership and a lifetime bounded by the whole, or **reference**, meaning everything else (ADR-0003). A composite's state is its own, gated by guards that read its parts and never derived from them (ADR-0004); "why is this stuck" is then answered by a `dependent` verdict naming the parts. A composition part may be re-parented by a transition that writes its owner, which is what makes splitting an object expressible (ADR-0046).

References carry no attributes. A relation with labels, dates or a lifecycle of its own is a **link object**: a type with two references. **One end of a relationship is stored and its declared inverse is a derived view over it** (ADR-0056), so there is only ever one write and one event; guards, invariants and outcomes traverse either direction. A composition declares, for **every terminal transition of the whole**, the child transition it drives and that cascade's bound, or marks the part `survives` to say deliberately that it outlives its whole; publishing rejects a terminal transition covered by neither (ADR-0058). A cascade clause may carry arguments (ADR-0066).

**`changed_since` reaches a part relationship by its own position.** The last event on each part relationship of an object is kept per relationship, so a guard naming `checklist_items` is not invalidated by a change to `approvals` (ADR-0082, repairing D202). An **observation** is a part of a special shape (§5.11). A singular reference may be marked **`assignee`** (§5.13), and every singular stored reference is tracked (§5.2).

### 5.4 State machines, transitions and outcomes

Each type binds exactly one **named** state machine, or declares its lifecycle inline; two types that share a lifecycle bind the same machine (ADR-0003, ADR-0026). A binder may add transitions of its own, and its own **creations replace the machine's** rather than adding to them, since birth is where a type's obligations are established (ADR-0064). The machine's creation **guards** still bind the replacement, so a lifecycle's entry condition cannot be dropped by declaring a creation (ADR-0065). Every state has a **category** from a deployment-defined set, so guards and views that span a family need not know state names, and at least one state is terminal.

A **transition** is a named, guarded, recorded request to move an object from one state to another. It is requested **by name, never by target state**. Its from-state may be one state, a set, or any non-terminal state. It declares **inputs** with types and optionality, **guards**, and an **outcome**. An **action** is a transition whose from- and to-state are equal; its outcome is unrestricted like any other, and there are **no exit or entry semantics** for a state, so an action neither leaves nor re-enters one (ADR-0016, ADR-0041). A request naming no declared transition is refused, so nothing is recorded in history for a change that did not happen; the refusal itself goes to the attempt log (§7), which is not history (ADR-0083).

An **outcome** is an ordered sequence of steps that write this object, create another, reach another by `call`, supersede, or loop over a bounded collection (ADR-0046, ADR-0052). **The grammar is `docs/design/declaration-syntax.md` §5.2, which owns it**; this section states what an outcome is and not how it is spelled, because two statements of one grammar drift and the syntax document is the one publishing checks. Until iteration 14 the grammar was restated here and had gone stale in three of its six lines.

Rules that make an outcome readable and safe:

- **Straight-line.** No *step* chooses between alternatives, and no step is conditional. Where the **behaviour** differs by a value — a different set of writes, a different object created — declare one transition per case, each guarded on that value, so the applicable one appears in the availability listing and a value matching none is visibly stuck. Where only a **value** differs, a conditional expression is allowed on the right of a write (`docs/design/declaration-syntax.md` §8.3), since it changes what is written and not what happens.
- **Sets change element-wise.** `add` and `remove` are read-modify-write like any outcome write, so two adds in one request accumulate and two concurrent adds serialise. The expression language gains no set algebra; `set` on a set-valued attribute still replaces it wholly (ADR-0055).
- **Absence skips, in two places only.** A write whose right-hand side is an unsupplied optional input is skipped, not applied, and a `call` whose path runs through an absent optional end is skipped because there is no object to reach. Everywhere else absence in a path is an error. Clearing a value deliberately is the `clear` step, which writes absence and is a write like any other (ADR-0073). Until iteration 18 this sentence promised something the syntax had no spelling for.
- **`this` and `this_event` are available**, including inside a creation outcome: the new object's id and its event's identity are allocated before the outcome runs (ADR-0046, ADR-0052).
- **Bounded.** Every loop and every cascade clause must declare a bound; exceeding one is refused with `over-limit` naming that loop (ADR-0071). The graph of transitions that reference each other in outcomes must be acyclic (ADR-0019).
- **Shown where it lands.** A cascade is declared on the transition that causes it, and the printed rules of every type it reaches name it too: under each transition, every transition whose outcome can cause it, `only via` parents and other callers alike (ADR-0108, `docs/design/renderers.md` §2). That is what lets a cascade be supported without surprising anyone, which the author made the condition for it (PRD F8).

A transition may be marked **only via** named parent transitions, meaning it is not requestable and carries no actor guards of its own (ADR-0020); **proposable**, meaning a caller lacking authority may file a Proposal (§9); or **asserting** (§8). It may also be marked **backdatable within** a duration: it then accepts the time the change actually happened, within that bound, never later than the time it is recorded, and never before the start of the current interval of any state or tracked value the request changes, and the event keeps both times, because a duration computed from when someone got round to recording a change is wrong (ADR-0083). Every transition the request cascades to records the same occurred time, since one request is one change (ADR-0095). The occurred time is a field of the request, and its bound is a generated guard, `occurred_within`, so a request outside it is refused naming a rule and a remedy like any other (ADR-0099).

Creation is a transition from nothing into an initial state, carrying its own guards. A type with several creation transitions is named explicitly at the creation site. Requiredness attaches to transitions, so `validate(object)` has no answer and `check(object, transition, inputs)` does (ADR-0002, ADR-0005).

### 5.5 Guards, verdicts and remedy classes

A guard is an expression that must hold for a transition to proceed. Each guard clause may **declare its remedy class**; omitted, one is always inferred, by a stated priority order because most guards match several rules, and the inference never contradicts a declared class (ADR-0060 §11). A transition **declares its inputs**; publishing rejects a guard naming an undeclared input and warns on an input nothing uses, so the tool schema and the validation rules cannot drift, because they are one declaration.

Evaluating a request yields one **verdict**, and every refusal names a remedy class, so a caller always knows its next move (PRD F4, ADR-0105):

| Verdict | Meaning | Remedy class |
|---|---|---|
| satisfied | The request proceeds | — |
| unsatisfied | A guard failed; carries the failing clause, whether it was **false or unknown** — so a caller can tell a refusal from a value nobody has yet — and what the remedy points at: the objects to work on, the capability that would satisfy it, and whether the transition accepts a proposal (ADR-0092) | the clause's, declared or inferred |
| `stale` | The caller's `expected_version` is out of date, or the transaction exhausted its serialisation retries; the verdict says which (ADR-0023, ADR-0039, ADR-0092) | `self_serviceable` for a moved version; `temporal` for exhausted retries |
| `not found` | Visibility hides the object; existence is not disclosed (ADR-0030) | `unreachable_from_here` |
| `not requestable` | The transition is only via named parents, which are named in the verdict (ADR-0020, ADR-0041) | `unreachable_from_here` |
| `over-limit` | A cascade exceeded a declared fan-out cap, which is named (ADR-0041) | `unreachable_from_here` |
| invariant violated | Names the invariant and the conflicting objects | `dependent` where it names other objects; `self_serviceable` where only this object's values conflict |

The **remedy class** tells a caller what to do next:

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

A guard that depends on facts outside the store references a **named external evaluator**, which must return the same verdict shape and **never a value** (ADR-0008, ADR-0069). An external system that decides or assigns something is modelled as an **externally owned type** instead: an ordinary type with an external identifier, whose sync-driven transitions a sync service requests with the external system's own change identifier as idempotency key, and whose local transitions are whatever the domain needs (ADR-0080). Like any request, a sync cannot write back a personal value an erasure removed (§8). Two ways to import external truth would differ in what they record, and the type is the one that leaves a history with transition names. Evaluators are consulted **before** the write transaction opens, never inside it, and their verdict carries an as-of time recorded on the event; a declaration may set a freshness bound, past which the verdict is refused as `temporal` (ADR-0049). An evaluator's arguments are resolved against committed state before the transaction and again inside it; a mismatch means the world moved, and the request consults again within its retry bound, and an argument the request's own outcome writes is refused at publish (ADR-0092). The guarantee for an external fact is as of that time, not at commit.

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
| `entered_at(STATE or tracked member)`, `time_in(STATE)`, and aggregates over `this.intervals(…)` and `this.transitions`, the object's own flow data | `unit is null and entered_at(unit) + 14 days <= now` |
| a declared metric, in a guard only (§5.12) | `metric(inspection_pass_rate, engineer := engineer, over last 30 days) >= 0.900` |

Evaluation is **three-valued**: comparison with an absent value, and division by zero, yield unknown. Unknown propagates, with the two Kleene exceptions that make presence tests useful — `and` is false if either side is false, `or` is true if either side is true — and a guard that evaluates to unknown **fails**, naming the clause and reporting the value as unknown rather than false. An **invariant** that evaluates to unknown **holds**, which is the rule a database `CHECK` follows and the only one under which a partially filled object is workable. Presence is therefore tested with `is null` and `is not null`, and comparing against a bare `null` literal is a publish error (ADR-0053). Over an empty set, `count` and `sum` are zero, `all` and `none` are true, `any` is false, and `min` and `max` are unknown. `this` always means the object the expression is declared on and never rebinds, so every aggregate binds its element explicitly.

Not in the language of guards, invariants and derived attributes: grouped aggregation, string operations beyond equality, membership and `length`, user-defined functions, recursion or transitive closure, any call other than a declared evaluator or a metric. Grouped aggregation, time windows, `avg`, `median` and `percentile` exist in the metric form alone (§5.12). `length` bounds a text value as an invariant bounds a number (PRD D12), and a string never holds U+0000, which neither backend counts the same way; a quotient of two durations, or of two amounts of one currency, is a decimal, so a utilisation or a share can be declared; and in a metric's filter or an outcome's loop an element whose filter is unknown is not selected, as SQL's `WHERE` leaves out a row, while inside a guard such an element leaves the aggregate unknown, so the guard still fails closed (ADR-0047, ADR-0103). Whether text matches a pattern, and trimming blank input, are left to the caller's edge or an evaluator, since a pattern means different things in Python and in each backend's SQL.

**How far a guard actually reaches, measured.** Every guard in the first consumer was counted on 2026-09-09. Of the 205 that read data, 94 are local to the object and 111 reach beyond it. Of those 111: 27 are one hop to a single object, 31 aggregate over a collection the object owns, 30 are type-wide existence or uniqueness, and 23 are two hops — 21 of which also aggregate. **Nothing in that system reaches three hops.** So the language needs one and two hops, aggregates over an owned collection, and a type-wide existence test, and it needs no transitive closure — which is the boundary ADR-0021 drew by argument and this measures.

**The line with computation** (ADR-0007, ADR-0032, ADR-0081): the store evaluates **any declared formula over its own data** — a guard, a view, an invariant, a quantity an outcome writes, a derived datapoint, a metric, a score built from them. It does not own a formula that needs data or rules it does not hold, such as a tax table or a pricing engine, and it does not choose, cause effects or orchestrate; upper-layer applications and callers do those, and supply results as inputs for guards to check. The language grows only by an ADR naming the case that forced it, and the case must be a PRD requirement.

### 5.8 Actors and visibility

Every request carries an **actor descriptor** the deployment's authentication produces: id, kind (human, agent or service), optional principal, and a set of opaque capabilities (ADR-0025, ADR-0079). ObjectFlow validates its shape, not its truth. A guard reads **four** members of it — `.id`, `.kind`, `.principal` and `.has(<capability>)` — and nothing else: the descriptor carries no declared types, so a guard over any other value could be neither checked at publish nor recorded in the log. Guards read it, and every event records it. Delegation arrives in the descriptor as capabilities with `principal` set to the delegator. Attenuation is a capability where the fact is one of a few fixed values, and otherwise an object the guard reads — a `Delegation` with a limit and a lifecycle — which is recorded, revocable and the pattern ADR-0036 gives for governed delegation. Where a guard needs a referenced person, that person is an ordinary object.

A type may declare a **visibility** predicate over actor and object; every read, query, availability result and delivered event applies it, and an invisible object is **not found**, never blocked, so a verdict never discloses existence (ADR-0030). A cascaded transition is not subject to visibility: the parent's guards are its authority.

A module line states a rule about requests across the closure (ADR-0103). **`requests by <kind> require version, key`** refuses a request by that kind of actor that lacks an `expected_version` on an existing object, or an idempotency key, naming the generated clause `versioned` or `keyed`. It covers what an actor sends, a proposal's approval included but not the recorded request the approval executes, and not `publish`, which has its own required version. It states the first consumer's rule that agents send both, more strictly than production, which asks for the key only on an agent's POSTs.

**Who acted** is shown to every reader who may see the object, as the first consumer shows each record's activity to anyone who may view the record. The cross-record trail it keeps for administrators — every entity's activity, one user's activity — is the export and a subscription's `pull`, which the deployment serves from routes it authorises, as the first consumer does today (ADR-0103).

### 5.9 Declarations: composition and versioning

A type may **extend** a base declaration for attributes, relationships, invariants and derived attributes; state machines are bound by name and never inherited piecemeal, though a binder adds its own transitions and replaces the machine's creations (ADR-0064). A **family** is a base and everything extending it, and is a query target (ADR-0026).

Every type, machine, enum, sequence, evaluator, observation kind and metric carries its own **version** in the text, and each publish installs the whole closure under one **declaration version**, per store and monotonic. Every object and event records the declaration version in force, which fixes the version of every type at that instant; a type version alone would not, since a machine or enum it depends on may have moved (ADR-0077). Because a type's behaviour depends on the machine, enums, sequences, evaluators and base types it uses, advancing any of those requires advancing every dependent type, which publishing enforces (ADR-0027, ADR-0056). A machine declares what it requires of its binders, and publishing verifies each one. Publishing is a designed operation with a report:

- **additions apply forward**; a new guard bites at the next transition. A publish changes a live object only by a recorded migration transition, never by DDL: a new required attribute needs a `backfill` mapping, a removed enum member that live objects hold needs a mapping, a field added to an observation kind must be optional, and a removed type is retired, its objects and history staying readable (ADR-0099);
- **a migration is F2's third route**, not an assertion: one event per object with source `migrated`, caused by the publish event, whose approval is its authority (ADR-0105);
- **every mapping is checked.** The dry run applies each mapping in simulation — a removed state, a removed member, a `backfill` — and evaluates every invariant over what it writes, and the publish repeats the check in its transaction, so what the dry run reports is what the publish does (ADR-0105);
- **a new invariant** is checked against live objects and every violation reported, as is a violation a mapping would cause. Each is resolved in one of three ways (ADR-0105):
  - the objects' own transitions, before the publish;
  - a different mapping;
  - an `admit <Type>.<invariant> because "<reason>"` the publish carries. This records an admission with its reason on each object's migration event, and is an override (§8);
- **removing a state** requires a mapping, applied as recorded migration transitions;
- a member the publish makes **tracked** is tracked from the publish: its current interval opens then for every live object (§7, ADR-0106);
- pending **proposals** the new version cannot express are invalidated (§9);
- the report also names which invariants compile on this backend, which transitions are sweepable, which derived attributes are queryable (§10), and every guard clause that is observing rather than enforced (§5.5);
- a flow changes only through a **`DeclarationChange`** (§9): drafted, approved by an actor other than its drafter — a person, when an agent drafted it — and published, its dry-run report being the impact report the draft carries (ADR-0085).

**A store begins at declaration version 0** (ADR-0105). Creating it installs:
- the built-in types and capabilities of §9;
- the `label` kind and the `closed` category;
- the standard metric definitions of the engine release that created it.

The first `DeclarationChange` is drafted against version 0 and published like any other, under F7's approval rules from the release that ships them (PRD §10). Every version records the built-in module it was published with, so an engine release that changes a standard metric's definition or a built-in type takes effect in a store only through a publish, whose report lists the change. The meaning of the language itself — what `.completes` marks, how a percentile is computed — is the engine's: a metric is computed on read under the engine's meaning over all history, and a decision that changes that meaning, as ADR-0106 did, says so.

Import applies this mechanism to objects arriving from outside (§11). The printable rule set is per version.

A flow can **start minimal** (PRD V1): one whose transitions carry no guards, and whose types declare no invariants, formulas or datapoint kinds, publishes and runs. The publish checks stay whole, since they protect the guarantee; what a new flow lacks is knowledge of its own rules, which observations, labels, observing clauses and diagnostics supply (ADR-0085).

### 5.10 Tracking mode

A type declares one of three modes (ADR-0050, ADR-0067). **Serial-tracked** is one object per physical thing with its own lifecycle. **Quantity-tracked** is a stock object carrying counters that actions adjust. **Record** tracks no physical thing at all — a delivery, a service job, an audit entry — and carries no counters, which is what most of an operations platform holds: in the first consumer, three entities are serial-identified and twelve or more are records. A slot in a configuration declares which fill form it takes, so one kit may carry a serialised robot and a counted quantity of consumables. Reserve means the same in both: the thing is spoken for. Changing a type's mode is a new type, and objects move by supersession one at a time.

### 5.11 Datapoints: observations and labels

A **datapoint** is a time-stamped fact about a flow or an object (PRD §5). The engine keeps the flow-generated ones — transitions, refusals, overrides, time in each state and value — without anyone declaring them (§7). **Recorded** datapoints are observations and labels (ADR-0082).

An **observation kind** is declared once, `on` a subject type, and names the collection it adds to that subject with `as`, so adding one is a single declaration that leaves the subject's text alone (ADR-0099). It has typed fields — a numeric field may state its unit, which is printed and returned but never converted — a `recorded by` guard and an `occurred within` bound. The kind names the collection it adds to the subject — `as inspections` — and the subject does not declare it. The kind expands into what it is: a `record` type with one terminal state, an `owner` end to its subject, and a generated `record` creation carrying the `recorded by` guard. Recording is therefore an ordinary creation request, with an actor, an event, an idempotency key and a place in history, and there is still no second write path. A transition's outcome may record one with an ordinary `create` step, which is how a transition records what it decided on.

- **Two part rules bend for observations.** An observation is recordable directly rather than only via its whole, and may not be recorded on a subject in a terminal state, which is what "settled" means. As a born-final part it is implied `survives`.
- **Recording writes one thing on the subject**: the position of its collection, which `changed_since` reads. It advances no version and writes no attribute, so it conflicts only with a transition whose guard read that collection; a recorded fact matters to the flow only through a guard that reads it (ADR-0099).
- **Recording is offered and dated like a transition.** `availability(subject)` lists the recording of each kind on it and labelling, `check` answers for any creation given its type, and the occurred time is a request field bounded by a generated `occurred_within` guard (ADR-0099).
- **Recording is validated.** Fields are typed, and a kind may declare invariants over its own fields, checked when it is recorded.
- **Nothing is edited.** A correction is a new observation of the same kind and subject, naming the one it corrects, which must not already be corrected; whoever may not record on the subject may not correct it either, and a correction may be recorded after the subject is finished, since it replaces a fact rather than adding one. Every read of the kind — a guard through the subject's part, or a metric — sees only the uncorrected ones, and `history` and `export` keep both (ADR-0095, ADR-0096). Erasure is the one exception, and removes only personal values: each kind has a generated `forget`, which the subject's `erase` runs and which may be requested on one observation alone.
- **A guard that aggregates over observations records, on its event, the observations it read**, so the decision can be re-evaluated from the record. Deriving the set from positions instead would be wrong wherever a position is not commit order (§7).
- **Catalogues of what to record are data, and governed.** An inspection checklist is objects with their own transitions and guards, and an observation references one. So changing a checklist needs no publish, and a rule that reads a catalogue is loosened only through a recorded, attributed transition on it, under the catalogue's own authority (PRD D7, UC-19).

A **label** is a built-in observation available on every type: a normalised name, an optional note, and the subject's state when it was applied. It records what nobody has modelled yet.
- **Read by measures, never by rules.** Labels are read by metrics and diagnostics and **never by a guard, an invariant or a visibility predicate**, since a rule over a vocabulary nobody declared changes meaning with a typo. A label becomes logic by promotion to a state, an observation kind or an attribute, which is a publish, and objects in flight then move by ordinary requests, so history says who moved each.
- **Note and name.** A label's note is treated as personal. Its name is vocabulary, not personal, and erasure does not reach it: the known limit on free text of §8 (ADR-0106).
- **Occurred time.** A label's occurred time is its recorded time, since it records that someone noticed something then. Where when a thing happened matters, the fact is promoted to an observation kind with `occurred within` (ADR-0106).
- **Who may label** is a deployment capability, which a type may narrow.

### 5.12 Metrics and business exceptions

A **metric** is a declared formula across many objects, grouped and windowed as it declares; a formula over one object is a derived attribute (§5.2, ADR-0084, PRD §5). It names:

- a **source**: a type's current objects, an observation kind, a type's labels, or one of a type's flow datasets — its `intervals`, `transitions`, `attempts` and daily `attempt_counts` — or, for a **combined** metric, other metrics joined by their dimensions, which is how a rate divides one dataset by another (ADR-0098);
- a **filter** in the expression language;
- **dimensions**:
  - paths up to two hops;
  - the kind of actor that created an object, requested a transition or recorded a datapoint;
  - the declaration version;
  - time buckets over any timestamp, in UTC calendar time with ISO weeks beginning on Monday.

  Every metric, declared or standard, has the `version` and `actor_kind` dimensions from its rows unless it declares them, and a combined metric passes them through. A legacy row whose record names no actor has the kind `unknown` (ADR-0106);
- a **value**, which may combine `count`, `sum`, `min`, `max`, `avg`, `median` and `percentile` arithmetically, over a window relative to `now` if it wants one. A percentile is nearest-rank, the smallest value whose rank reaches `p × n`, and a median is the fiftieth percentile, so both backends give the same value; `avg` keeps its body's type, so an average duration is a duration (ADR-0106);
- optional **flags**: named conditions on the value.

Metrics are **computed on read, over the rows the reader can see**: source rows are filtered by the source type's visibility predicate, datasets and observations inherit their subject's, and a path from a row to an object the reader cannot see yields absence at that step, so nothing of a hidden object is read. A stored reference is a value of the object that holds it, so its id is visible where that object is; what visibility protects is the referenced object. **Every result says whether it is complete**: complete when the reader can see every current object of each type the metric reads, and otherwise a value over the reader's own rows, marked as such. So every reader who gets a complete value gets the same value from one definition, no one mistakes a partial value for it (PRD C2), a reader who sees part of the data still gets metrics over that part (PRD M2), and none learns anything from what they may not see (PRD T5) (ADR-0096).

**Every result also says how far the record covers it** (ADR-0106).
- **`recorded_from`.** Each object records the time from which its record is continuous: its creation, or for a ported object the earliest time its legacy history vouches for (§11).
- **`gaps`.** A row counts, as its `gaps`, the objects for which it read something from before what their record vouches for: a window beginning before the object's `recorded_from`; an interval, legacy or current, that began before it; an undated `.created_at`; or a member read from before its tracking began. The page says whether any row has one.
- **Two questions, two answers.** Being complete is about what the reader may see; coverage is about what was ever recorded. A reader who sees everything over a silent stretch of legacy history is told so, rather than told the figure is whole.

A guard computes a metric over every row its source holds, as a type-scan guard reads every object of its type, because a rule whose verdict depended on who asked would not be a rule. Its value is recorded on the event and carried in a refusal, and shown to a later reader, or to the requester, only if their view of it would be complete (ADR-0095, ADR-0096). Where UC-10 would have a refusal name a value its requester may not see, T5 prevails, being a Must where UC-10 is hypothetical. A metric is retroactive by construction and stores nothing, which is why erasure needs nothing from one. **A metric reads no personal value**, in its filter, a dimension, a flag or its value. So an erasure removes no row a metric reads and changes no value it reads, and every count over what was recorded before it stays as it was (PRD UC-17, ADR-0106). The erasure adds its own events: the `erase` request is counted as the transition it is, and the event it records on an object reached by the taint analysis is marked `.redacted` and counted by no standard metric but the override counts, where it carries an admission.

Every type gets **standard metrics** without declaring any. They are declarations, given in `design/declaration-syntax.md` §6.11 with the names `metric()` reads them by, and checked like any other (ADR-0098).

**Open and finished** are read from the one category with a meaning of its own, `closed`, which every vocabulary has (ADR-0096, ADR-0106).
- An object is open while its current state is neither `closed` nor terminal, so reopened work is open again.
- It completes on every transition from an open state into a closed or terminal one, so work finished, reopened and finished again completes twice.
- Throughput and cycle time carry the state entered as a dimension, so a cancellation and a finish are counted apart, and the reader decides which closed states are success.

**What the standard metrics count:**
- none counts the import's events, a publish's migrations or an erasure's redactions of other objects, which are the port, the publish and the erasure, not the flow — except the override counts, which count every override, a migration's or a redaction's admission included (ADR-0105);
- a span still open on an object in a `closed` or terminal state stops at its entry into that state, so an engineer still named on a closed job accrues no time;
- each can be split by version and actor kind;
- two name the objects, so the jobs that changed hands more than once can be listed (ADR-0101).

The standard metrics of every type are:

- time in each state;
- throughput;
- work in progress;
- age of the oldest open objects;
- how often each transition is taken;
- refusal rates per rule;
- override counts per target state and reason — assertions, and the admissions an erasure or a publish records — the import's own excluded (ADR-0105);
- rework.

A type with an assignee also gets the metrics of §5.13. All of them are listed with the type, like any declared metric, so a person reading the rule set and an agent calling `metric()` see the same definitions. **A reader reads a metric as a guard does**: `metric()` binds dimension values, applies a window and a filter, and returns one row per group of the dimensions it keeps — all of them by default — where a guard aggregates over every dimension it leaves unbound to get its one value (ADR-0101), so a screen, an agent and a rule read one value from one definition over the same data, and a reader who may see only part of it gets the value over that part, marked as such (ADR-0098). A derived attribute a metric row reads is evaluated under the reader's visibility, as everywhere a reader reads one, and the types it reads count toward whether the result is complete (§5.2).

A **threshold** in a data-driven rule or a condition is either a literal in the declaration, changed only by a flow change (§9), or an attribute on an object, such as a reorder point per model, changed only by that object's governed transitions (PRD L4).

A **business exception** — work overdue, ageing past a threshold, a service level breached — is a declared condition and not a new construct.
- **Over one object** it is a derived attribute over a stored operand and `now`. `query` accepts it by name and the store rewrites it to a filter on that operand, so the threshold is declared once and no caller restates it (ADR-0048, PRD M7).
- **Across many objects** it is a metric's flag.

The store never sends one: an upper-layer application asks and acts (ADR-0012, ADR-0104).

### 5.13 Assignment

An **assignee** is whoever is responsible for an object at a given time, as distinct from whoever acts on it (ADR-0086). A singular reference marked `assignee` names that responsibility. Its target type marks one `identity` attribute as the actor's id, so the engine can tell when someone other than the assignee acts. A type may mark more than one — an engineer-of-record and a reviewer — and each is a separate dimension. The marking adds no write path: assignment is whatever declared transitions write the reference, each with its own guards on who may assign and who may be assigned.

Every singular stored reference is tracked from its object's creation, absence included (§5.2), and the log can rebuild the intervals. So assignment history does not depend on the marking being present from the start; it depends on the assignee being written through the store from the start.

Every assignee dimension gets metrics without declaring any, declared in `design/declaration-syntax.md` §6.11:

- open work per assignee;
- time unassigned;
- time to first assignment;
- time with each assignee;
- cycle time by the assignee at completion, and time in each state by whoever held the object then;
- handoffs;
- reassignment back to an earlier assignee;
- how often someone other than the assignee acts, meaning requests a transition on the object; recording an observation or a label on it is counted by that kind's own recorder (ADR-0106).

The assignee a transition row is attributed to is the one who held the object before the transition's own writes, so a reassignment is attributed to the engineer it took the job from (ADR-0106). Choosing the assignee stays outside the store (§12).

## 6. Transition execution

A request names an object, a transition, its inputs and the actor, and optionally the version the caller last read, an idempotency key (ADR-0014), and a free-form `context` string: the caller's statement of why the request was sent and by which route, such as the rule or job of an upper-layer application that caused it, recorded on the event or the attempt row. It is what "recorded with its reason" means for an application's actions (PRD UC-20, ADR-0105). A creation request names the type and the creation transition instead of an object, and skips steps 1 and 3.

External evaluators and metrics named by any guard in the request are consulted **first, outside the transaction**, and their verdicts or values carried in with their as-of times (ADR-0049, ADR-0084). An evaluator's or a metric's arguments are resolved against committed state now and again inside the transaction (ADR-0092); a mismatch means the world moved, and the request consults again within its retry bound. The rest runs in one transaction at **serialisable isolation** (ADR-0039). On SQLite the transaction begins `IMMEDIATE`, taking the write lock at its first statement, so a request's guard reads and its writes are serialised with every other writer's (ADR-0090):

1. If the type declares visibility and the actor cannot see the object, refuse as `not found`.
2. If this actor's idempotency key has been applied before to this request, return the original result, marked as a replay, **whatever `expected_version` the retry carries**: a retry is the original request re-sent, and its version is the one the original was checked against (ADR-0041, ADR-0076). A key applied before to a different request is a fault, not a verdict (ADR-0077). Then, if a module requires fields of this kind of actor's requests and one is missing, refuse, naming `versioned` or `keyed` (§5.8, ADR-0103).
3. If an `expected_version` is given and differs, refuse with `stale`. This is possible only for a request that has not been applied.
4. Evaluate the parent transition's guards, the generated ones included — `occurred_within`, `whole_open`, `not_erased`, and for a recording `subject_open`, `corrects_current` and `subject_owned` (ADR-0099, ADR-0100, ADR-0101, ADR-0105). A failure refuses the request, naming the clause, its object and its remedy class. A clause marked `observe` that fails refuses nothing; its would-be refusal is written to the attempt log in this transaction, linked to the event of step 8 (ADR-0085).
5. Apply the parent's outcome in full: its new state and its attribute writes, each value read at the moment it is applied (ADR-0054).
6. Then, depth-first in declaration order and over collection elements in ascending object-id order, which is creation order (§5.1), take each cascaded transition or creation in turn, **skipping a part already in a terminal state**, since its disposition has happened and that is the only skip on account of state; for each of the rest evaluate its guards **against the state produced so far**, then apply its outcome immediately (ADR-0038). Only-via transitions contribute their non-actor guards; other cascades run as the requesting actor. Any failure aborts the whole request and rolls back, except a clause marked `observe`, which refuses nothing at any depth and is logged as in step 4 (ADR-0100).
7. Check every invariant the written objects could violate — where a transition writes a part's `owner`, **both** the source whole and the destination whole count as written, so the source's invariants are re-checked and both have their part-event position stamped (ADR-0058), except those with an admitted violation still standing (ADR-0045, ADR-0054); an admitted violation whose invariant now holds is discharged, and the discharging position recorded. A `do` or `act` that writes an `owner` on an existing object — a re-parent — carries a generated guard, `whole_open`: the destination whole is not in a terminal state, as check 11 requires for a creation (ADR-0100, ADR-0101).
8. Increment each written object's version; update the interval index for every changed state and tracked value, using the occurred time where the transition is backdatable; record one event per transition, each cascaded event carrying the parent's event as its cause, and each carrying the request's serialisation retry count, its writing transaction, and the **read set** of the rules evaluated for it (below); insert each event row once, complete, in the same transaction — its position was allocated when its transition began, so nothing is completed later (ADR-0013, ADR-0083, ADR-0088, ADR-0089); write the idempotency record where the request carried a key. Commit.

The **read set** makes every decision re-evaluable from the record (PRD L2). It lists every object a guard or invariant read, with the version it read — the object, related objects, an aggregate's elements, and what a type-scan matched, an empty match included — the capability each `actor.has` asked about and its answer, and the `now` the request fixed. Re-evaluating the rules over those versions, folded in each object's own strict order, gives the same verdict, without any global order (ADR-0088). The one exception is a value erased since: it reads as absent, the record says an erasure happened, and a clause that read it may now be unknown, which is T3's exception and not a failure of the record. A reader is shown a read set, and the evaluator verdicts and metric values an event records, only as far as their visibility reaches: an object they cannot see is left out, a metric value is left out unless their view of it would be complete — they can see every current object of each type the metric reads — and the read set says that something was withheld, so a partial record is never mistaken for a complete one. An auditor who must re-evaluate every decision reads as an actor that can see everything (ADR-0095).

**A request that does not apply** — refused by a guard, `stale`, `not found`, `not requestable`, over a limit, violating an invariant, naming no declared transition, or reusing a key — is written to the **attempt log** in its own short transaction after the rollback. The row holds:
- who asked, their kind and their `context`;
- the transition and the verdict;
- the failing clause with its remedy class, and the read set of everything the request had read when it was refused, the objects its outcome and cascades reached included;
- the request's own values: every non-personal input and its occurred time. A personal input is recorded by name as withheld (ADR-0105);
- the metric values and evaluator verdicts it consulted (ADR-0083, ADR-0088, ADR-0096).

A refusal is re-evaluated by replaying the request over those versions and values. That matters because an invariant, or a cascaded guard, refuses over state the request itself produced and then rolled back, which only a replay reconstructs. So a refusal re-evaluates from the record as an applied request does, except where a personal value decided it, which the record says. The row holds no personal value, so erasure needs nothing from it. Rows are pruned after the deployment's retention period, and the prune adds each day's counts to a permanent daily rollup in the same transaction, so refusals stay countable per week over the whole history (PRD T3). A crash between the rollback and that write loses one row of evidence and no history.

Row locks are taken as objects are reached, for contention rather than correctness: serialisable isolation is what makes a guard's reads safe, including reads of objects the request never writes. A serialisation failure is retried to a declared bound and then refused with `stale`. **A contended row does not queue** on PostgreSQL: the second request waits for the first and then fails with a serialisation error, which the retry absorbs, and the lock only makes the loser fail at its first touch of the row rather than after doing its work (ADR-0090, probed). On SQLite a contended request waits out the busy timeout, and a timeout exceeded counts as a serialisation failure, so either backend answers a race with a verdict, never a fault. The sequence mint draws from a pool of its own, so a burst of creations cannot starve the request pool (ADR-0090).

An **asserting** transition (§8) differs in two ways: step 4 evaluates only its own guards, which are its capability and reason requirements rather than the type's; and step 7 refuses on any invariant violation the request did not explicitly admit. A **migration** a publish applies runs steps 5 to 8 for each object as part of the publish's transaction, with the publish's approval as its authority and the publish's `admit` lines as its admissions (§5.9, ADR-0105).

## 7. History, events and delivery

**Current state is stored**, one row per object with its attributes, state, version, declaration version and last event. **The event log is the permanent history**, written in the transition's transaction; a fold of the log reproduces the row, never the reverse (ADR-0033). A per-attribute index of the event that last wrote it makes `changed_since` a constant-time comparison (ADR-0048), and the same index, keyed by part relationship, does it for the half that reaches a composition: one position per relationship rather than one per whole (ADR-0057, ADR-0082).

Beside the log the engine keeps two more records (ADR-0083):

- **The interval index** records how long each object held each state and each value of each tracked attribute and reference: entry and exit by position and by occurred time, the transition that set it, the actor who made the change and its kind, and the declaration version. The log can rebuild every interval it recorded, so it is an index and not a second source of truth, and the store maintains **no projection the log cannot rebuild**. Intervals the import supplies from the legacy system's history are not a projection: they are imported history, read-only and marked legacy like the legacy entries of §11, and the harness checks them for shape rather than against the log: none overlaps another of the same dimension, and the last ends where the object's first recorded interval begins (ADR-0096).

Three rows begin somewhere other than at a request (ADR-0106):
- **A port.** The import's event opens each current interval at the time the legacy record says the object took that value, and carries that time, so one stay is never split in two at cutover.
- **A mirror's refresh.** A refresh that changes a tracked value does the same, and may carry the intermediate values the legacy record shows since the last import, opened and closed in order on its event.
- **A publish.** A member a publish makes tracked opens its current interval at the publish event, for every live object.

The fold of the log reproduces all three.
- **The attempt log** records every request that did not apply, and every would-be refusal of an observing clause. It holds no personal value. It is not history: `history` does not return it, and subscriptions do not deliver it. It is pruned after a retention period the deployment sets, with its counts kept permanently.

Every event carries `changes_state`, true when from and to differ, and its **provenance**: actor, principal, `context`, cause, declaration version, the version of the taint check that passed the write, so a later audit can tell which rules were in force (ADR-0051), the time a backdated change happened beside the time it was recorded, the number of serialisation retries its request took (ADR-0083), its writing transaction (ADR-0089), the read set of its rules (ADR-0088), and a `source` of

| Source | Produced by |
|---|---|
| `observed` | An ordinary transition |
| `asserted` | An assertion, including import (§8, §11) |
| `migrated` | A flow change's mapping (§5.9): F2's third route, with the publish as its cause |
| `corrected` | An action that records a corrected value for an earlier mistake, with a reason; the earlier event is not rewritten |
| `erased` | An erasure (§8), on the erased object and on every object whose personal value it redacted |

The log is **never pruned**; retention is archival tiering that keeps events readable and reachable by erasure. Events are strictly ordered per object and causally ordered across a cascade. A global position is monotonic but not a commit order, so the log is read by a **settled cursor**: a reader returns only events whose writing transaction had finished when its snapshot was taken — on PostgreSQL, those below the snapshot's `xmin` — in (transaction, position) order, and hands back the last pair as the cursor. A later commit always sorts after any cursor already handed out, so a reader acknowledging a cursor never skips an event; on SQLite, whose writers are serial, the cursor is the position (ADR-0089, probed). `pull` and `export` both return it, and `acknowledge` takes it.

**Each object's events arrive in its own order.** A later writer of an object commits in a later transaction, which serialisable isolation guarantees (`design/storage-schema.md` §7), so settled-cursor order keeps every object's events in their strict order. A reader that acknowledges only what it has handled is delivered every event at least once, in order for each object (PRD L6).

```
            recorded transition
                    │
         permanent ordered log        written in the same transaction
                    │
                  pull                 the core's only delivery
          a reader, with a settled cursor
                    │
      upper layer:  a relay posts to an endpoint and acknowledges;
                    a scheduler or an agent acts on what it read
```

A **Subscription** is a built-in object holding:
- a filter over a type or family, transition names and `changes_state`;
- its **reader**, the one actor who may `pull` and `acknowledge` it, the creator by default;
- lag thresholds;
- a lifecycle of `active → revoked`.

Creating one requires `OF_SUBSCRIBE` (§9, ADR-0105). Delivery progress is **runtime state, not object state**: the acknowledged cursor is stored beside the idempotency record, is not versioned and emits no events, so a poll never lands in the permanent log. Lag and death are **derived** from that cursor and the thresholds: the lag is the age of the oldest event the filter selects after the acknowledged cursor, and its two thresholds are durations (ADR-0100), so nothing has to move a subscription into them, and ADR-0012 is untouched. A subscription may be revived at any cursor, because nothing is pruned (ADR-0034, ADR-0043).

Delivery is **at-least-once with idempotent handling**. Exactly-once *effect* comes from deduplicating on the event id at the receiver, and transitions accept an idempotency key so the common case, an event causing a transition, deduplicates centrally and records causal lineage (ADR-0014). The reader pulls as itself, and visibility applies to what it reads.

**The core posts nothing** (ADR-0105). A deployment that wants events at a URL runs a relay, an upper-layer application that pulls as its own actor, posts, and acknowledges once the post succeeded. Its failures are its own to record and alert on, and stopping it loses nothing, since the cursor waits where it was acknowledged.

**Time.** `now` is a value in guards. A time-driven transition is an ordinary one, `expire: ACTIVE → EXPIRED, guard end_date <= now`, and ObjectFlow never requests it. An upper-layer scheduler asks the availability query and requests each answer with a periodic idempotency key (ADR-0022). The guard is the timing rule, in one place. ObjectFlow initiates nothing (ADR-0012).

**A time-driven transition may record when it fell due** (ADR-0105). Marked `backdatable within` the longest delay the deployment tolerates, it takes the deadline the scheduler read — `occurred_at := end_date` — as its occurred time, so a late or stopped scheduler does not overstate time in the earlier state. Beyond the bound it is refused and sent undated, and the lateness stays readable from the stored deadline.

## 8. Ends of life, repair and erasure

**Deletion** is a terminal transition gated on there being no live object referencing this one; parts cascade, references block with `dependent` naming them. Deleted objects stay readable by id and in history and take no further transitions (ADR-0024).

**Supersession** ends an object in a terminal state that names its successor, which may be an existing object or one the same outcome created. Id and history stay, the read surface follows the pointer on request, and references are re-pointed only by declared cascade. It is how an object changes kind: moved, converted, merged (ADR-0028, ADR-0046). A duplicate is closed with a reference, not superseded.

**Assertion** is the only way to set state without satisfying the type's guards, and it is declared, not ambient (ADR-0040). An asserting transition names the states it may assert; is addressed by name with the target state as a constrained input, so §5.4's by-name rule holds; carries its own guards, at minimum a capability and a mandatory reason; and records what it stepped over. A type that declares none cannot be repaired this way at all. Invariants are not bypassed by default: an assertion marked `may admit` accepts an explicit list of the invariants it knowingly violates, and each **admission** names an object and an invariant, suppresses that invariant's check for that object, and is discharged automatically when the invariant holds again (ADR-0054). The import uses a **built-in** assertion gated on `OF_IMPORT`, so no type is unimportable by omission. A flow change's migration is not an assertion: it is F2's third route, authorised by the publish's approval, and the one part of it that is an override is an `admit` the publish carries (§5.9, ADR-0105). `exceptions(type)` lists the objects whose state an assertion last set, and those holding an admitted violation.

**An override**, in the PRD's sense, is either:
- an assertion's event;
- any event that records an admission: an assertion's, an erasure's, or a publish's `admit`.

Each carries a reason, and the standard override counts count every one but the import's (ADR-0105). An import's state change is recorded as `imported` in the object's `state_source`, while its event keeps provenance `asserted`, so the list shows repairs and admissions and is not buried by the legacy population (ADR-0083, repairing D205).

**Erasure** replaces the values of declared personal attributes with absence, on the object and in every event that carried them, deletes the content of a file once no unerased reference to it remains — so an identical file another object holds survives (ADR-0087) — keeps the event skeleton, records that it happened, and is irreversible (ADR-0031). It runs from any state, terminal included, since erasure requests arrive for closed accounts; it is the one transition that runs at a terminal state; a publish's migration, which is the publish's recorded change and not a transition the object takes, is the other thing that reaches one (ADR-0056, ADR-0101). The marker is absence rather than a sentinel, so it collides with no unique constraint and reads as unknown in the expression language, which makes a guard over an erased value fail rather than pass (ADR-0051). It is the one place the log is rewritten, and it is not deletion. It also reaches the legacy entries attached to the object, redacting every payload field the import mapping did not list as kept; the inputs of every proposal that targets the object or whose inputs flow into its personal attributes, invalidating the pending ones (ADR-0087); and every event payload that carried a personal input, whether or not that input was written to an attribute, an input being personal by marking or by flowing into a personal attribute (ADR-0078). It reaches observations through each one's generated `forget`, which the subject's `erase` runs without a declared step, redacts label notes, which are treated as personal, and redacts a personal enum's values in the interval index (ADR-0096). A file deletion that fails is retried by the next erasure request, and `diagnostics` of the erased object's type lists it, so no background process is needed (PRD N2); that listing ships with erasure in the first release, ahead of the rest of `diagnostics` (`design/data-driven-engine.md` §9). The attempt log holds no personal value, a metric stores nothing, a read set holds only ids and versions, and the export omits every personal value, so none of them needs erasing (ADR-0082 to ADR-0084, ADR-0088, ADR-0094).

Erasure follows the person, not only the id (ADR-0087). It erases every member of the object's **supersession chain**, predecessors and successors, each through its own type's `erase`, since superseding declares that the successor continues the same thing, and publishing rejects a type that holds personal attributes, takes part in supersession and declares no `erase`. It redacts, in every **caller's event** up the erased object's cause chain, the inputs that flowed into the erased object's personal attributes, which check 10's taint analysis identifies. And it redacts every proposal targeting an erased object or carrying inputs that flow into one. **The known limits.** What erasure cannot reach is:
- a personal value typed into free text no declaration marks as personal — another object's text field, or a label's name, which the store treats as vocabulary (ADR-0106);
- a copy an upper-layer application made outside the store.

**What else it reaches** (ADR-0100):
- it deletes the external identifiers held in the erased object's personal attributes, so `lookup` no longer finds them;
- it redacts an affected proposal's inputs in the proposal's own event as well as its record;
- it follows the erased object's events down to the events they caused, and erases what flowed from it into other objects' personal attributes, by the same taint analysis;
- it reaches every observation in the subject's collections, corrected ones included.

**Every object whose value an erasure redacts gets an event.** The event has source `erased`, is caused by the erasure's own event, and names the attributes redacted and never their values. The object's version advances, so a subscriber sees the change and a request carrying the old `expected_version` is refused as `stale` (ADR-0105).

**What was erased stays erased.** Once an object's own erasure is recorded — through its type's `erase`, or as a member of an erased supersession chain — a request whose outcome would write one of its personal attributes is refused by the generated guard `not_erased`, as the import already is. An object whose value the taint analysis redacted was not itself erased, and may be given a new value like any other. So an externally owned type's sync cannot restore what the other system has not erased yet, and a person who returns is a new object (ADR-0105, PRD UC-17).

An import keeps only an entry's own object's fields in legacy history, so no legacy entry carries another object's values (§11).

**Erasure's admissions are overrides.** An erasure admits the invariants it breaks, which is an override in the PRD's sense (ADR-0100, ADR-0105, PRD T1). It:
- exists only through a type's declared `erase`, whose authority covers every object the erasure reaches;
- carries a reason;
- is recorded as an admission on the erased object, or on the object whose redacted value broke the invariant;
- is listed by `exceptions(type)` and counted by the override counts.

**A mirror may declare `erase`**, the one transition it may declare, since erasure is this store's obligation over its own copy and not a write of the state another system owns; the import never rewrites what it erased (ADR-0101). **A publish's migrations** reach every object the change affects, terminal ones included — they are the publish's recorded change, not a transition the object takes — and never an observation, which is born final (ADR-0101).

## 9. Built-in types

`Subscription` (§7), `Proposal` and `DeclarationChange` are built in and not user-definable, and `label` is a built-in observation kind (§5.11). All of them are in declaration version 0, which a store is created with (§5.9).

A **Proposal** holds a request a caller lacked authority for: target, transition, inputs and proposer, with a lifecycle `pending → executed | rejected | withdrawn | invalidated`.
- **Approval.** Approving it executes the recorded request **as the approver**, with the approval as cause, and every guard is re-evaluated at execution under the **current** declaration, actor guards included. An approval therefore succeeds only if the approver holds whatever authority the transition's guards demand; nothing reads a capability off a guard in advance, because the guard itself is the test. A guard that fails leaves the proposal pending with the verdict attached; a declaration that can no longer express the request invalidates it (ADR-0036, ADR-0044). Expiry is derived, not driven.
- **Ending it** (ADR-0105). Its proposer may `withdraw` it. An actor who passes the proposed transition's actor guards — whoever could carry it out — may `reject` it.
- **Visibility.** A proposal is visible to its proposer and to whoever can see its target. A proposed creation, which has no target, is visible to its proposer and to those who pass the creation's actor guards. Within one, an input naming an object the reader cannot see is withheld, as a verdict's are.

A **DeclarationChange** is how a flow changes (ADR-0085, ADR-0097). It is a built-in type with a declared lifecycle, printed in the rule set like any type:

- `draft` creates one, `DRAFTED`, from the source and the evidence it cites — metric and diagnostic references — for an actor with `OF_DRAFT_CHANGE`; the drafter may `revise` it;
- `submit` moves it to `SUBMITTED` and runs the dry run, attaching the impact report and a snapshot of the evidence it cites — the metric and diagnostic values as they then read, not links to values computed on read; `refresh` reruns the dry run;
- `publish` is the approval, and installs the version. It names the version of the change the approver read, and `refresh` advances it, so the report compared at commit is the one the approver was shown (ADR-0101). It requires `OF_APPROVE_CHANGE`; an approver who is neither the drafter nor the drafter's principal, nor acting for the drafter; a person, where an agent drafted it; a change drafted against the installed version; and a recomputed report naming nothing the attached one did not;
- `reject` and `withdraw` end it, and a publish **supersedes** every other open change drafted against an older version, in the same transaction, so a stale change can never reinstall text that drops a newer guard.

The impact report names, keeping the ids of each:
- the live objects violating a new invariant, or an invariant a mapping would break;
- those in a removed state;
- those each mapping will rewrite;
- those lacking a value a mapping must supply, or holding a removed enum member with no mapping;
- the pending proposals invalidated;
- the live objects that would gain or lose an available transition under the new guards;
- those a clause newly enforced would refuse;
- the other open changes the publish will supersede (PRD F7, ADR-0105). The engine never drafts a change; an agent reading `diagnostics` usually does. Being an object, every step is attributed, and the publish event has an object to belong to (repairing D212). A change is visible to actors holding `OF_DRAFT_CHANGE` or `OF_APPROVE_CHANGE`, and within it the report's ids and the evidence's values are shown only as far as the reader's visibility reaches, as a verdict's are (ADR-0101).

**Built-in capabilities** gate the built-in types' transitions, and every vocabulary has them whether or not a module declares them: `OF_DRAFT_CHANGE`, `OF_APPROVE_CHANGE`, `OF_LABEL`, `OF_ERASE`, `OF_IMPORT`, `OF_MAINTAIN` and `OF_SUBSCRIBE` (ADR-0097, ADR-0105). Who may draft, approve, label, erase one observation, import, maintain and subscribe is therefore declared, and printed; who may end a proposal is read from the proposed transition's own guards, and who may read a subscription is the reader it names.

## 10. The read surface

One API. Every operation takes an actor and applies visibility (ADR-0037).

| Operation | Returns |
|---|---|
| **get**(id, follow?) | The object, naming as partial each derived attribute that read what the reader cannot see in full (§5.2, ADR-0106); optionally following supersession to the live successor |
| **query**(type or family, filter, order, cursor, fields) | A page of objects. The filter uses indexed attributes, state and category, derived attributes that are themselves indexable, the time-dependent derived attributes the publish report lists as queryable — which the store rewrites to a filter on the stored operand they compare against `now` (ADR-0048) — and the entry time of a tracked value's current interval (§5.13, ADR-0084); `fields` defaults to the type's summary set |
| **lookup**(source, value) | The object whose external identifier for that source matches |
| **availability**(id) | Each requestable transition with its three-way availability, verdict and declared input schema. Only-via transitions are not listed; and for a subject, the recording of each observation kind on it and labelling (ADR-0099) |
| **available**(type, transition, cursor) | The objects for which that transition is available now. Requires the transition to be **sweepable**, meaning its guards decompose into an indexable prefilter and a residual; one that is not is refused rather than scanned (ADR-0048) |
| **check**(id or type, transition, inputs) | The verdict a request would receive, simulating the same sequence internally. Lock-free and side-effect-free, so it is advice and not a reservation, and its verdict is **partial**: it names the external guards it did not evaluate (ADR-0054). A creation is checked given its type (ADR-0099) |
| **history**(id, follow?) | Events with provenance, legacy entries, and optionally the combined timeline through supersession |
| **exceptions**(type, cursor) | The objects whose state an assertion last set, the data import excluded, and those holding an admitted invariant violation; what keeps the escape hatch of §8 reviewable |
| **declaration**(type, version?) | The inspectable rule set at a declaration version, the number an event records; rendered also as readable text and as agent tool schemas |
| **pull**(subscription, cursor) | For the subscription's reader only: a page of events per its filter, in each object's order, and the settled cursor (§7) |
| **batch**(requests) | N independent requests, **each in its own transaction**, returning N verdicts in order |
| **metric**(name, bind?, keep?, over?, filter?, cursor) | A declared or standard metric computed over the rows the reader may see, with dimensions bound and a window applied as a guard's reference does, narrowed by the filter before it aggregates, one row per group of the dimensions kept — every declared one unless `keep` names fewer — and aggregated over the rest: each group with its dimensions, its value, the flags that hold and its `gaps`, in UTC calendar time, and whether the result is complete and whether its history is (§5.12, ADR-0084, ADR-0096, ADR-0098, ADR-0106) |
| **export**(source, cursor) | A page of JSON lines from the log, a type's `intervals`, `transitions`, `attempts`, `attempt_counts` or `labels`, or an observation kind, in settled-cursor order, under the reader's visibility, with every personal value omitted, and the next cursor (ADR-0094, ADR-0106) |
| **diagnostics**(type, window) | Where the flow and reality disagree: transitions and states unused in a window, the clauses that refuse most, states entered mostly by override, labels by frequency and state, the longest waits, rework and reassignment loops, work done by non-assignees, backdating rates, and observing clauses' would-be refusals (ADR-0085); and file deletions still pending after an erasure of the type's objects (ADR-0096). Computed like a metric, over what the reader may see, and saying whether it is complete |
| **import_batch**(batch, dry_run?) | Writes a port through the built-in assertion, for `OF_IMPORT`, into mirrors and the observations and labels on a mirror's objects, which no request may record there; never rewrites what an erasure removed; every event marked `imported` (§11, ADR-0100, ADR-0101) |
| **maintain**(task) | Prunes attempt rows past the retention the store was built with, keeping their counts in the rollup; prunes idempotency records past the replay window the store was built with, after which a retry with a pruned key is a new request; or moves events to the archive the log's reads include; for `OF_MAINTAIN`; changes no governed state or history, and refuses a prune inside its retention (ADR-0100, ADR-0101, ADR-0103) |

Neither `available`, `check` nor `availability` calls an external evaluator; each says which guards it did not evaluate. `check` and `availability` do compute internal metrics, since no third party is involved. A time-dependent derived attribute is queried by name, and the store filters on the stored operand the expression compares against `now`, since a clock-dependent value cannot be indexed; the caller never restates the threshold (ADR-0048). There is **no atomic batch**: atomic multi-object semantics are declared cascades (§5.4). Beyond the indexes a type declares and the interval index (§7), the store maintains no projection; where a scan is too slow and the type is `quantity`-tracked, it declares a counter and the cascade that maintains it. That remedy is **not available to a `serial` or `record` type**, which is open question 10 and is why a ledger balance has nothing tying it to the entries that produced it (ADR-0072).

Writes go through the request of §6. Operational calls that are not object operations are `acknowledge`, which advances a subscription's progress and is the subscription's reader's alone (§7); `publish`, which requests a `DeclarationChange`'s `publish` transition, and whose dry run is the impact report (§9); `import_batch`, which is how the port and the refresh of a mirror write (§11); and `maintain`, which prunes attempt rows past the store's retention into the rollup, prunes idempotency records past the replay window, and moves events to the archive, changes no governed state or history, and is never needed for correctness (ADR-0100, ADR-0101, ADR-0103). Nothing else writes the database, except creating the store, which installs declaration version 0 before any operation can run (§5.9).

## 11. Import and migration

The first consumer's production data is ported by a designed path (ADR-0015). Every imported object receives a new id and keeps its legacy key as an external identifier with source `legacy`; cross-references are re-pointed through that mapping (ADR-0018). Imported state is written by the built-in assertion with provenance `asserted` (§8), through `import_batch`, which requires `OF_IMPORT` and carries each object's legacy entries, legacy intervals, and legacy creation time and kind (ADR-0100). It writes a type only while the type is a `mirror`, and the observations and labels on a mirror's objects, so legacy datapoints port with their subjects: every type is ported as a mirror and cut over by the publish that removes the marking, so no import reaches an owned type. It never rewrites a personal value an erasure removed, and the legacy entries and intervals it attaches to an erased object arrive redacted as the erasure would have left them (ADR-0101), and every event it writes is marked `imported`, which is what `exceptions` and the override metrics exclude.

The importer evaluates the declaration and reports every violated invariant and unsatisfied structural guard, grouped into classes, and a person decides each class: cleaning the source, supplying a value in the mapping, admitting an invariant violation, or excluding the objects. A guard class cannot be admitted, since an admission names an invariant (ADR-0077). Soft-deleted rows land in the type's deleted state. Files are content-addressed into the deployment's blob store. Cutover cannot be dual-write, because there is one write path. An import's state change is recorded as `imported` (§8).

**Legacy history is carried whole, as its own objects' history.**
- **Legacy entries.** Every legacy entry is ported as a read-only entry of kind `legacy`, returned by `history` alongside ObjectFlow's own events.
- **Fields about another object.** An entry keeps the payload fields that describe its own object. A field that describes another ported object moves to that object's history.
- **Fields the store holds no object for** are not ported, such as the IP address of the employee who acted. PRD §12 proposes that N5 say so (ADR-0100).

**Intervals and creation times reach back as far as the legacy record does** (ADR-0083, ADR-0086, ADR-0106).
- **What the mapping supplies.** It may supply each object's legacy creation time and creator's kind, and for its state and each tracked member when it took its current value and who changed it. Where it supplies no creation time, `.created_at` is the earliest time the port knows of the object, from its legacy intervals, entries and entry times, or the port itself, and the object is marked **undated**; an entry time or a legacy interval earlier than a creation time the mapping does supply is refused. The import's event opens the current interval at that time, so a stay is never split at cutover. Earlier values arrive as legacy intervals, marked as such, with who made each change where the legacy record says.
- **Assignment.** State and assignment are covered alike: the first consumer's past reassignments come from its audit trail's before-and-after snapshots.
- **Silent legacy rows.** A legacy row whose record names no actor has the kind `unknown`.
- **A mirror's refresh** carries the same for what changed since the last import.

**Where the legacy record is silent, the store says so and fills nothing in.** The mapping declares the record's known silent spans, and the import leaves no legacy row inside one. Each object records `recorded_from`, the earliest time from which its record is continuous. A current interval that began before it, whose value the legacy record could not vouch for across a silent span, stays as the record gives it, and a metric that reads it counts the object as a gap, as it does for every object it read something of from before `recorded_from` (§5.12, ADR-0106).

Import writes every row complete in one pass: every object's id is assigned from the legacy-key mapping before any row is written, so every reference resolves at insert, and a cycle of required references commits with its foreign keys checked at commit (ADR-0077). Cutover is **staged by object type**, with one owner per type at any moment. A type this store holds and does not yet own is declared **`mirror`**: attributes, states and an external identifier, no transitions but `erase`, written only by the import path (ADR-0101); it is cut over by a version advance that removes the marking and changes no object's id (ADR-0075). Mirrors may compose with and extend each other, since every type is ported as one, and a composition or a family is cut over by one publish, since an owned type may not compose with or extend a mirror (check 53, ADR-0101). The stage order counts two kinds of edge: a reference, so a referrer migrates no later than what it references, and a legacy write, so a type the legacy system writes — through its transition registry or its service layer — migrates no earlier than the writer (ADR-0100); a strongly connected component of the combined graph is one stage (ADR-0093). The marking is for cutover only. A type another system owns for good — the first consumer's customer, which Xero owns — is an externally owned type (§5.5), and cutting the customer over means replacing the marking with that type and its sync-driven transitions (ADR-0080), which a sync service requests and which cannot write back what an erasure removed (§8).

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

Everything in the right-hand column that manages a flow or uses its data — scheduling, chasing, assigning, notifying, worklists, dashboards — belongs to an upper-layer application built on the store, with no privileged path into it (PRD N6, ADR-0104). Preconditions, permissions and the measurement of a flow have the same shape across domains; formulas that need outside rules, effects and selection are where domains differ irreducibly. The store decides, records and measures; upper-layer applications and the people using them choose and cause (ADR-0007, ADR-0081).

## 13. Known limits

The guarantee is that **governed state changes only by PRD F2's four routes, each recorded, and bypasses the enforced guards only through a capability-gated override**:
- an assertion the type declares;
- the built-in one the import uses;
- an admission — by an assertion, by an erasure's declared `erase`, or by a publish's approved `admit`.

A flow change's migration is authorised by the publish's approval, and the objects holding asserted state or admitted violations are queryable at any time (§5.9, §8, §10, ADR-0105). A guard clause on trial is not enforced and guarantees nothing, and the publish report lists every one (§5.5). A guard reading a metric is enforced against the value it was given, as of the time it was computed and not at commit, as for an external evaluator. It cannot guarantee that **the guards say what was meant**. Risk relocates from scattered implementation bugs to specification gaps in one readable place. Hence two obligations: the rule set for a type must be printable for review by someone who knows the process, and acceptance testing must be adversarial in the threat model's sense, a fallible actor that guesses, retries and skips steps, trying every route to an invalid state. The harness drives real connections through the injected connection source, releasing one statement at a time in an order its seed chooses, so a concurrent failure reproduces from its transcript (ADR-0091).

The mediated property is an operational commitment. Every write has an operation — `request`, `batch`, `acknowledge`, `publish`, `import_batch` and `maintain` — beside creating the store, which installs version 0 and is attributed to the engine release that did it, since no one wrote that version's rules; anything else that can reach the database, a migration script, `psql`, a reporting job, voids it. A guard reading a **mirror** reads it as of its last import: every mirror object carries `.imported_at`, which a guard may bound as a freshness bound does an evaluator's verdict (ADR-0100). Repair is a declared assertion, and the data import is its first use at scale.

Concrete cases the model does not cover, or covers with a caveat, are catalogued in [`design/edge-cases.md`](design/edge-cases.md). They include in-place type change, gapless sequences, hot-row throughput, multi-region stock, acyclicity of self-referential hierarchies, unmerge, local-time rules, and the several things a declaration is refused at publish for.

Three things the first consumer does stay with it (ADR-0103, [`design/first-consumer-audit.md`](design/first-consumer-audit.md)):
- **Checking that text matches a pattern**, such as a Jira key or a link's scheme, which its schema does at the edge. This is by decision.
- **Filtering a list on a status derived from other objects**, which a derivation that reads other objects cannot be indexed for. Where the status gates a transition, `available` answers it in one query. Otherwise the reader filters the page, which is a scan. None of the questions PRD N6 and UC-20 name needs it, but ADR-0104 §4 counts any need an upper layer meets by scanning as a **gap in the core**, and `TODO.md` records it as one.
- **Reacting to such a status changing**, which an upper-layer application pulling the transitions that change it does, as UC-12's order-ready notice does.

A metric buckets a span by when it began, so a share of time — a utilisation, the fraction of a job's life spent waiting — is right over a whole history and not per month: a span is not apportioned across the buckets it crosses (ADR-0103).

Four things measure less than they appear to:
- **Late recording.** A metric is only as good as the recording behind it: a change recorded late distorts time in state unless its transition is backdatable, and backdating is itself reported.
- **Time-driven transitions.** One requested by a scheduler is dated correctly only within its declared backdating bound. An upper-layer application stopped for longer overstates time in the earlier state by the excess (§7, ADR-0105).
- **Sets are not tracked.** Neither a set of references, such as a team, nor a set of enum values is tracked, so assignment to a team and a multi-choice attribute have no intervals yet (ADR-0106).
- **Ported history.** It covers only as much as the legacy record held, and a metric reports its `gaps` rather than hiding them (§11).

## 14. Terminology

PRD §5 defines the product's terms and this table uses them: **engine**, the governed core; **deployment**, one installation and the declarations it publishes; **upper-layer application**, anything built on the engine that manages a flow or uses its data; **reader**, whoever reads through the API; **governed state**, an object's state and attributes; **override**, **admitted violation** and **declared limit**. The rows below add the design's own.

| Term | Meaning |
|---|---|
| **Flow** | The declared types, lifecycles, transitions and rules, with the datapoint kinds, formulas and conditions over them, versioned together and changed only by publishing (PRD §5). |
| **Lifecycle** | The observable shape: the states an object passes through. |
| **State machine** | The mechanism: states, transitions and guards, as a named declaration a type binds. |
| **State category** | A deployment-defined classification every state declares, so family-wide guards need not know state names. |
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
| **Verdict** | The result of evaluating a request: satisfied, unsatisfied, `stale`, `not found`, `not requestable`, `over-limit`, or invariant violated; every refusal carries a remedy class. |
| **Remedy class** | What a refusal tells the caller to do: supply, ask, wait, work elsewhere, or take another path. |
| **Invariant** | A type-level property, checked after every request against the objects it wrote. |
| **Admission** | A recorded decision to tolerate one invariant violation on one object, discharged when the invariant holds again; made by an assertion, an erasure or a publish's `admit`, and always an override. |
| **Override** | A change to governed state that does not satisfy the rules: an assertion's event, or any event recording an admission; always with a reason, and counted (PRD §5, ADR-0105). |
| **Migration** | A flow change's mapping applied to one object: F2's third route, recorded `migrated` and caused by the publish. |
| **Derived attribute** | A named expression, never stored, evaluated on read. |
| **Expression language** | The one language of guards, invariants, derived attributes, visibility, outcome values and filters. |
| **External evaluator** | A named consultant for facts the store does not hold, returning the same verdict shape, consulted before the transaction with an as-of time. |
| **Composition / reference** | Exclusive membership with a bounded lifetime, versus every other relationship. |
| **Link object** | A type that relates two objects and carries attributes or a lifecycle about the relation. |
| **Actor** | The descriptor a caller supplies with a request: id, kind, principal, capabilities; nothing else is readable, and a fact about an actor is a capability or an object (ADR-0079). |
| **Visibility** | A declared predicate every read applies; failing it means not found. |
| **Object id / business identifier / external identifier** | Store-assigned identity; a meaningful value such as a serial, supplied as input or minted from a sequence (ADR-0029); a value another system owns. |
| **Sequence** | A named, scoped, monotonic counter that mints business identifiers; not gapless. |
| **Tracking mode** | Whether a type is serial-tracked, one object per thing; quantity-tracked, counters on a stock object; or a record, tracking no physical thing at all (ADR-0067). |
| **Mirror** | The cutover marking: a type the legacy system still owns, held here with no transitions but `erase` and written only by import until a version advance removes the marking (ADR-0075, ADR-0080, ADR-0101). |
| **Externally owned type** | An ordinary type another system owns for good, with an external identifier, whose sync-driven transitions a sync service requests (ADR-0080). |
| **Type family** | A base declaration and everything extending it. |
| **Declaration version** | The number of a publish: one per installed closure, per store, monotonic from version 0, which holds the built-ins a store is created with; recorded on every object and event, and what `declaration(type, version)` takes. |
| **Type version** | The number a type, machine, enum, sequence or evaluator carries in its text; advancing one advances every type that depends on it (check 22). |
| **Settled cursor** | A (transaction, position) pair up to which every writing transaction had finished when the reader's snapshot was taken; the furthest a pull or export reader may acknowledge, and never ahead of a late commit (ADR-0089). |
| **Read set** | What an event records of the rules evaluated for it: every object read with its version, capability answers and the fixed `now`, so the decision re-evaluates from the record (ADR-0088). |
| **Publish** | Installing a declaration version, as the approval of a `DeclarationChange`, with a report of migrations, admissions, compiled invariants, sweepability and observing clauses. |
| **DeclarationChange** | A built-in object through which a flow changes: drafted with its evidence and impact report, approved by an actor other than its drafter, published. |
| **Deletion / supersession / erasure** | Ending an object; continuing it as another; removing personal values from it and its history. |
| **Provenance** | What an event records about itself: actor, principal, context, cause, declaration version, source, and where they apply the time a backdated change happened and the serialisation retries its request took. |
| **Idempotency key** | A caller-supplied key that makes a repeated request return the original result rather than acting twice, for as long as the store's idempotency retention keeps it (ADR-0103). |
| **Subscription** | A built-in object holding a filter, the reader who alone may pull and acknowledge it, and lag thresholds; its progress is runtime state, not object state. |
| **Relay** | An upper-layer application that pulls a subscription and posts what it reads to an endpoint; the core has none (ADR-0105). |
| **Proposal** | A built-in object holding a request the caller lacked authority for; an authorised actor's approval executes it as themselves. |
| **Sweepable** | A transition whose guards decompose into an indexable prefilter and a residual, so `available` can answer for it. |
| **Effect** | Something caused outside ObjectFlow; out of scope. Upper-layer applications cause effects by observing events. |
| **Measured** | The fourth core property: every flow produces data about itself, users record their own, and everyone reads both through the same declared metrics. |
| **Datapoint** | A time-stamped fact about a flow or an object: flow-generated, recorded, or derived by a formula (PRD §5). |
| **Observation** | A recorded datapoint: a born-final part of a declared kind, recorded by a creation request. |
| **Label** | A built-in observation recording what nobody has modelled yet; read by metrics and diagnostics, never by a rule. |
| **Tracked** | Of an enum attribute or a singular reference: how long an object held each of its values is kept, without its author declaring it. |
| **Interval** | A span during which an object held one state, or one value of a tracked member; kept in an index the log rebuilds. |
| **Attempt log** | The record of requests that did not apply and of observing clauses' would-be refusals; not history, holding no personal value, pruned after a retention period. |
| **Backdatable** | Of a transition: it accepts when a change actually happened, within a declared bound, and the event keeps both times. |
| **Metric** | A declared formula across many objects, computed on read over the rows the reader may see, saying whether that is all of them and how far the record covers them; standard ones exist for every type. |
| **Coverage** | How far an object's record reaches back: `recorded_from`, from which its history is continuous; a metric row counts as `gaps` the objects whose record does not cover what it measures (ADR-0106). |
| **Flag** | A named condition on a metric's value. |
| **Business exception** | Work deviating from what its flow expects — overdue, ageing, a service level breached — declared as a derived attribute over one object or a metric's flag. |
| **Assignee** | Whoever is responsible for an object at a given time, named by a reference marked `assignee`. |

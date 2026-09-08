# Defect register

Findings from the implementation-readiness review of 2026-09-08. Every entry was verified against the file cited; the review ran as four independent passes (one direct, three delegated) and only findings confirmed by reading the source are recorded here.

**How to read this.** `D01`–`D10` break the model or a running system and must be resolved before a runtime is built. `D11`–`D26` are things the design cannot express or has no algorithm for. `D27`–`D36` are contradictions and scope errors. `C01`–`C05` are cosmetic. Status is `open`, `resolved by ADR-xxxx`, or `wontfix` with a reason.

**Provenance.** ADR-0019 to ADR-0037 and the five case studies were produced in the autonomous design iterations of 2026-09-07/08. Defect density is highest there, and the case-study notation problem (`D11`–`D18`) originates entirely in that work.

| ID | Defect | Status |
|---|---|---|
| [D01](#d01) | Cascaded transitions cannot see each other's writes | **resolved by ADR-0038** |
| [D02](#d02) | Reads outside the lock set are unisolated | **resolved by ADR-0039** |
| [D03](#d03) | The guarantee is false as written; the override is undeclared | **resolved by ADR-0040** |
| [D04](#d04) | The lock set cannot be computed when the spec says | **resolved by ADR-0039** |
| [D05](#d05) | Availability both does and does not list only-via transitions | **resolved by ADR-0041** |
| [D06](#d06) | Idempotent replay is specified as both refuse and replay | **resolved by ADR-0041** |
| [D07](#d07) | The built-in Subscription cannot be operated under its own rules | **resolved by ADR-0043** |
| [D08](#d08) | Proposals across declaration versions are undefined | **resolved by ADR-0044** |
| [D09](#d09) | Free attributes have no write path | **resolved by ADR-0042** |
| [D10](#d10) | Two different invariant-enforcement specifications | **resolved by ADR-0045** |
| [D11](#d11) | Cascaded transitions cannot take inputs | open |
| [D12](#d12) | An outcome cannot name an object it creates | open |
| [D13](#d13) | `this_event` is undefined and chronologically impossible | open |
| [D14](#d14) | Repeat-N creation is not expressible | open |
| [D15](#d15) | Map-typed inputs and dynamic writes are not expressible | open |
| [D16](#d16) | `sum` over a type is required but not granted | open |
| [D17](#d17) | Grouped aggregation is used but undefined | open |
| [D18](#d18) | There is no event-reference attribute type | open |
| [D19](#d19) | Division by zero yields a verdict, not a value | open |
| [D20](#d20) | Null semantics are unstated | open |
| [D21](#d21) | Derived attributes are not required to be acyclic | open |
| [D22](#d22) | Expression scoping and name resolution are unstated | open |
| [D23](#d23) | Parameter-schema derivation has no algorithm | open |
| [D24](#d24) | Remedy-class assignment has no rule | open |
| [D25](#d25) | The availability query has unbounded cost | open |
| [D26](#d26) | `changed_since` has no index story | open |
| [D27](#d27) | The spec's definition of an action is the option ADR-0016 rejected | **resolved by ADR-0041** |
| [D28](#d28) | Files are both never touched and deleted | **resolved by ADR-0041** |
| [D29](#d29) | Splitting is both uncovered and expressible | open |
| [D30](#d30) | Derived attributes cannot be queried | open |
| [D31](#d31) | Constraint compilation is mis-cited and backend-dependent | **resolved by ADR-0041** |
| [D32](#d32) | External evaluators have no slot in the execution sequence | open |
| [D33](#d33) | Quantity and serial tracking are incompatible | open |
| [D34](#d34) | Three Proposed ADRs are load-bearing | open |
| [D35](#d35) | Erasure has no taint rule and may collide with uniqueness | open |
| [D36](#d36) | `self-serviceable` is misused for the fan-out cap | **resolved by ADR-0041** |

---

## Severity 1: breaks the model or a running system

### D01
**Cascaded transitions cannot see each other's writes.**
`docs/adr/0019-outcomes-cascade-across-relationships-atomically.md:14` requires "All guards first ... evaluated before anything is written", and `docs/DESIGN.md:167`/`:169` separate guard evaluation (step 5) from writing (step 7).

Breaks three things:
1. `IntakeBatch.commit` cascades `inventorize` then `reserve` (`docs/design/first-consumer-walkthrough.md:191`). Reserve requires the unit `AVAILABLE`, which only the first cascade produces. Against a pre-write snapshot the guard always fails. This is ADR-0019's own motivating example (`0019:8`) and the scenario that closes the double-booking window.
2. Order placement cascades `product.reserve(qty)` per line (`docs/design/case-study-orders.md:16`) guarded on `on_hand - reserved >= inputs.qty` (`:20`). Two lines on one product both read the same snapshot, both pass, both apply. One order oversells itself.
3. `reserved := reserved + qty` applied twice from one snapshot is either 12 or a lost update to 6. No rule states which.

**Resolution required:** a decision on sequential versus simultaneous cascade application, and on repeated writes to one attribute.

**Resolved by ADR-0038.** Sequential application: each cascade's guards see the writes of those before it; failure aborts before commit; iteration in ascending id order; writes are read-modify-write.

### D02
**Reads outside the lock set are unisolated.**
`docs/adr/0023-transitions-execute-under-locks-and-may-carry-an-expected-version.md:13` locks only what is written; `:14` states "Objects that are only read are not locked" and grants serialisable isolation to *invariants* alone. No document states the isolation level of the transition transaction.

Defeats ADR-0024 by construction. Its delete guard is `none(<referencing type> where <reference> == this and state not terminal)` (`docs/adr/0024-deletion-is-a-terminal-transition-gated-on-live-references.md:13`). The reference lives on the referencing row, so `Customer.delete` and `Delivery.create` lock disjoint rows, share no conflict, and both commit. The result is a deleted customer with a live delivery, which is the failure ADR-0024 exists to prevent.

Applies to every cross-object guard, including `none(Service where unit == this and state != CANCELLED)` (`docs/DESIGN.md:140`).

**Resolution required:** the isolation level, and what the store does about reads the lock set does not cover.

**Resolved by ADR-0039.** The transition transaction runs at serialisable isolation, so guard reads outside the lock set are protected; serialisation failures retry then refuse with `stale`.

### D03
**The guarantee is false as written, and the override path is undeclared.**
`docs/DESIGN.md:229` states the guarantee absolutely: "no state change bypassed the guards." Against that:
- `docs/DESIGN.md:212` — imported state is "asserted through the override path".
- `docs/adr/0015-first-consumer-and-fresh-build-with-ported-data.md:56` — "none of that state passed through a guard."
- `docs/adr/0027-declarations-are-versioned-and-removals-require-a-mapping.md:14` — invariant violations may be "admitted with a recorded flag, by a person."

So the store can hold objects that failed their guards and objects that violate their invariants. Separately, the override is named in `DESIGN.md:198`, `:212`, `:231` and `docs/adr/0024:16` but declared nowhere: it appears in no model block, has no guards, no stated authority and no limit on which states it may assert. `docs/adr/0024:16` calls it "itself a transition with guards", but it must assert states the type's guards forbid, so its guards are not the type's.

It also collides with `docs/adr/0016-actions-are-self-transitions.md:18` ("Transitions are addressed by name, never by target state"), since the override and ADR-0027's migration mapping both address a target state.

**Resolution required:** declare the override as a first-class element, and qualify the guarantee.

**Resolved by ADR-0040.** The override is declared as an asserting transition: named, capability-gated, reason required, invariants admitted explicitly, and the guarantee in §13 is restated truthfully.

### D04
**The lock set cannot be computed when the spec says.**
`docs/DESIGN.md:166` locks "every object the outcome will write" before guards run; `docs/adr/0023:17` requires locks in id order. The cascade target set is a filtered iteration (`docs/adr/0019:22`), so it is discovered by reading, and `0023:14` does not lock what is read. Three rules cannot all hold.

Two further problems: a created object has no id to sort by, and `docs/adr/0018-every-object-carries-a-store-assigned-globally-unique-identifier.md:57` leaves time-ordering undecided, so deadlock freedom rests on an open storage choice. Sequence values are taken atomically inside the transaction (`docs/adr/0029-named-sequences-mint-business-identifiers.md:12`) but sequences are not objects and sit outside the id-ordered discipline.

**Resolved by ADR-0039.** The lock set is no longer computed in advance. Locks are taken as objects are reached; ADR-0023's id-ordering rule is withdrawn and deadlock is handled by retry.

### D05
**Availability both does and does not list only-via transitions.**
`docs/adr/0020-a-transition-may-be-reachable-only-via-named-parents.md:14` — an only-via transition "does not appear in any caller's availability list". `docs/adr/0037-the-read-surface.md:19` and `docs/DESIGN.md:208` — availability returns "every transition of the object's type". `docs/DESIGN.md:116` makes "not requestable" a verdict, implying such transitions do appear.

This decides whether an agent is shown `unit.sell`, which is the hazard ADR-0020 names at `0020:25`: "an agent will call whatever is listed."

**Resolved by ADR-0041.** ADR-0020 wins: only-via transitions are not listed; `not requestable` is returned when one is named explicitly.

### D06
**Idempotent replay is specified as both refuse and replay.**
`docs/adr/0014-delivery-is-at-least-once-with-idempotency-keys.md:27` — "ObjectKeeper refuses a second attempt carrying a key it has already applied." `docs/DESIGN.md:165` — "If the idempotency key has been applied before, return the original result."

Different observable behaviour. `docs/adr/0022-time-is-a-guard-value-and-availability-is-queryable.md:15` depends on the replay reading ("so a repeated sweep is harmless"). ADR-0014 itself warns at `:54` against conflating request replay with transition deduplication.

**Resolved by ADR-0041.** Replay wins: an applied key returns the original result, marked as a replay. ADR-0014's wording corrected in place.

### D07
**The built-in Subscription cannot be operated under its own rules.**
Three defects in one place.
1. `docs/adr/0034-event-ordering-and-subscriptions-as-a-built-in-type.md:20` makes the cursor a controlled attribute and declares only one action, `acknowledge`. Under `docs/DESIGN.md:56` a controlled attribute is written only via transitions, so the cursor has no writer. Yet `docs/adr/0037:24` makes `pull` a read-surface operation, so a read would advance a controlled attribute.
2. `docs/adr/0034:20` advances the `lagging` and `dead-lettered` states "by the delivery worker", which is part of an ObjectKeeper deployment (`docs/DESIGN.md:30`). `docs/adr/0012-objectkeeper-does-not-initiate-transitions.md:17` forbids any path by which an object moves without an external actor. Same problem for `Proposal.expired` (`docs/adr/0036-proposals-are-a-built-in-type-and-delegation-is-supplied.md:17`).
3. Every acknowledgement is an action, so it records an event, and the log is never pruned (`docs/adr/0033-current-state-is-stored-and-the-log-is-permanent-history.md:15`). Subscription acknowledgements become permanent history at delivery rate.

**Resolved by ADR-0043.** Progress is runtime state outside the object model; lag and death are derived, so nothing initiates and acknowledgements never enter the log.

### D08
**Proposals across declaration versions are undefined.**
`docs/adr/0036:16` re-evaluates guards at execution but never says under which declaration version. Consequences unhandled: a version that adds a required input makes the stored request unexecutable, and `0036:14` offers no terminal state for it, so it pends forever; `docs/adr/0027:15` says removing a transition affects no object, but it strands a proposal naming it; a migration can move the target out of the proposal's from-state.

**Resolved by ADR-0044.** Execution uses the current declaration; a new terminal state `invalidated` catches requests the declaration can no longer express.

### D09
**Free attributes have no write path.**
`docs/DESIGN.md:61`, `:96` and `docs/adr/0006-controlled-and-free-attributes.md:15` assert free attributes are written by any permitted actor and recorded. Nothing defines how.
- No operation exists. `docs/adr/0037:15-25` enumerates the whole API and contains no write; `docs/DESIGN.md:161-169` defines execution for transitions only.
- "Permitted" is undefined. `docs/adr/0025-the-actor-is-a-value-supplied-by-the-consumer.md:44` reduces permissions to guards over `actor.*`, and guards attach to transitions.
- No provenance class fits. `docs/adr/0033:14` fixes a closed set whose `observed` means "a transition produced it".
- The event shape assumes a transition: `changes_state` is undefined and the subscription filter keys on transition names (`docs/adr/0034:21`).

ADR-0006 is still Proposed and says at `:38` "Revisit before it becomes load-bearing." It is now load-bearing in ADR-0016, 0017, 0021, 0031, 0033, 0034 and 0035.

**Resolved by ADR-0042.** The free class is removed. Every attribute is controlled, and editing is a declared action, which costs one line and buys a guard, an actor and an event.

### D10
**Two different invariant-enforcement specifications.**
`docs/adr/0009-invariants-declared-at-type-level.md:12` is static: "The runtime determines which transitions could violate them and enforces there." `docs/DESIGN.md:168` is dynamic: "Check every invariant the written objects could violate." Different algorithms, different cost, different coverage. Neither is marked normative, and `0009:27` concedes the static analysis is "a real piece of machinery" without specifying it.

---

## Severity 2: cannot be expressed, or has no algorithm

**Resolved by ADR-0045.** Dynamic enforcement is normative; the static analysis becomes a publish-time report; invariants may traverse only declared inverses.

### D11
**Cascaded transitions cannot take inputs.** Used in every case study (`walkthrough:135`, `:191`, `:198`; `crm:46-48`; `orders:16`). Neither `docs/DESIGN.md:108` nor `docs/adr/0019:14` permits it, and ADR-0019's only example passes none.

### D12
**An outcome cannot name an object it creates.** `docs/adr/0019:15` shows `create WarrantyContract(...)` with no binding. This makes `docs/adr/0028-supersession-an-object-may-end-by-naming-a-successor.md` self-contradictory: `:1` takes the successor as an input object id, `:2` permits the transition to cascade its creation. Both cannot hold for move and convert, which is supersession's headline use. It also blocks split (see D29).

### D13
**`this_event` is undefined and chronologically impossible.** Used once, at `docs/design/case-study-approvals-and-bookings.md:40`, and defined nowhere. Every guard in ADR-0035 keys on the approval carrying it. Events are written at `docs/DESIGN.md:169` step 7, after the outcome that would read it.

### D14
**Repeat-N creation is not expressible.** `docs/design/first-consumer-walkthrough.md:187` raises a procurement order "cascading `create Unit(...)` ×N". `docs/adr/0019:22` permits iterating a relationship only, and `:46` forbids iterating a query over a type. An integer input has nothing to iterate.

### D15
**Map-typed inputs and dynamic writes are not expressible.** `docs/design/case-study-crm.md:45` writes `<each attribute in values> := inputs.values.<attribute>`. `docs/DESIGN.md:96` gives a closed attribute-type list with no map, and `:108` fixes outcome writes as static `attribute := expression`. The merge is that study's centrepiece.

### D16
**`sum` over a type is required but not granted.** `docs/adr/0032-expression-language-version-2-adds-arithmetic.md:18` and `docs/DESIGN.md:141` grant `sum`/`min`/`max` over a relationship. `docs/adr/0032:37` states as its own consequence that available-to-promise needs "a type-scan sum over inbound units".

### D17
**Grouped aggregation is used but undefined.** `docs/design/case-study-orders.md:24` declares the invariant `sum(shipments.qty for a line) <= line.qty`. No `for a <group>` construct exists in ADR-0021 or ADR-0032.

### D18
**There is no event-reference attribute type.** `docs/adr/0035-approval-is-a-guard-over-recorded-approval-parts.md:12` requires an Approval to carry "the event that recorded it", and `docs/DESIGN.md:143` reads `approval.event`. The closed list at `docs/DESIGN.md:96` has no event reference, and `docs/DESIGN.md:204` makes only Subscription and Proposal built in, so Approval must be consumer-declared. It cannot be.

### D19
**Division by zero yields a verdict, not a value.** `docs/adr/0032:16` — "division by zero is a guard failure, never a value." Expression evaluation is therefore not compositional: the rule is undefined inside `none(...)`, inside an implication's antecedent, and inside a derived attribute the read surface returns.

### D20
**Null semantics are unstated.** `docs/adr/0021-derived-attributes-and-the-expression-language.md:19` has a null test and `docs/DESIGN.md:144` compares against null, but nothing says whether comparison with null is three-valued. `docs/adr/0031-erasure-redacts-declared-personal-attributes-across-history.md:17` makes the redaction marker read as null. Under a two-valued reading, erasing a person makes `actor.id != requested_by.id` and `none(approvals where approver == actor.id)` true (`docs/design/case-study-approvals-and-bookings.md:17`), granting the erased person self-approval and repeat-approval. `docs/design/edge-cases.md:31` claims "Erasure never silently satisfies a guard", which is false for exclusionary guards.

### D21
**Derived attributes are not required to be acyclic.** `docs/adr/0019:23` and `docs/DESIGN.md:171` require the cascade graph to be acyclic. `docs/adr/0021:12` imposes nothing, so `a := b + 1; b := a + 1` is a legal declaration and non-terminating.

### D22
**Expression scoping and name resolution are unstated.** `docs/adr/0035:14` defines `changed_since` over "the named attributes of `this`", `docs/DESIGN.md:143` qualifies the element (`approval.event`), and `docs/design/case-study-approvals-and-bookings.md:37`,`:42` use a bare `event` inside `none(approvals where ...)` while `relevant` is declared on the parent. Whether `this` rebinds inside a predicate is never stated, and the two readings give opposite results.

### D23
**Parameter-schema derivation has no algorithm.** `docs/adr/0005-guards-evaluate-over-state-plus-inputs.md:23` asserts a transition's parameter list is derived from its guards, and `docs/adr/0037:19` returns it. Deriving input constraints from an arbitrary boolean guard such as `inputs.qty <= on_hand - reserved` is abduction, not extraction. ADR-0010's headline claim that tool schemas and validation cannot drift rests on this. Related inconsistency: `docs/DESIGN.md:76` says a transition declares `inputs` while `:116` says the schema is derived, and under the derived reading an input used only in an outcome write is invisible to callers.

### D24
**Remedy-class assignment has no rule.** `docs/DESIGN.md:120-124` defines five classes. `docs/adr/0022:12` implies inference from expression shape, but `end_date <= now` is `temporal` when stored and `self-serviceable` when supplied, and a conjunction of clauses with different classes has no stated rule. Whether the class is declared per guard or inferred is never said.

### D25
**The availability query has unbounded cost.** `docs/adr/0037:20` returns objects for which a transition is available. Guards may contain type scans, aggregates and `changed_since`, so cost is O(candidates × unbounded). Only *deferred* external evaluators are excluded (`0037:20`, `0022:14`), so an eager one (`docs/adr/0008-guard-escape-hatch-is-a-named-external-evaluator.md:14`) means a third-party round trip per object. Visibility is an arbitrary actor predicate and cannot generally be indexed. Cursor pagination over a computed predicate needs a stable total order that is not specified. `docs/adr/0037:51` calls the operation cheap.

### D26
**`changed_since` has no index story.** `docs/adr/0035:14` reads the log. Every indexing statement in the record is about the object row (`docs/adr/0033:12`, `docs/adr/0037:27`). Answering "which event last wrote attribute X of object O" requires per-attribute writes to be queryable in the log, which the storage schema does not yet exist to say.

---

## Severity 3: contradictions and scope errors

### D27
**The spec's definition of an action is the option ADR-0016 rejected.** `docs/DESIGN.md:106` and `:242` say an action "writes controlled attributes without changing lifecycle state", which is the shape of ADR-0016's rejected option B (`docs/adr/0016:45`, "an outcome restricted to attribute writes"). ADR-0035 requires actions to create Approvals (`0035:12`) and the CRM merge requires an action to cascade (`crm:47`).

**Resolved by ADR-0041.** ADR-0016 wins: an action's outcome is unrestricted. DESIGN.md §5.4 and the terminology entry corrected.

### D28
**Files are both never touched and deleted.** `docs/DESIGN.md:96` — "ObjectKeeper never touches the bytes"; `:200` — erasure "deletes referenced file content"; `docs/adr/0031:14` the same; `docs/adr/0017-file-attachments-are-content-addressed-references.md:20` gives the backend interface as "put, get-URL, exists, delete". Also, an Accepted ADR (0031) depends on a Proposed one (0017), and `docs/DESIGN.md:212` states file porting as settled while `docs/adr/0015:74` leaves it open.

**Resolved by ADR-0041.** Erasure wins: ObjectKeeper never reads, streams or serves bytes, and deletes them only during erasure.

### D29
**Splitting is both uncovered and expressible.** Recorded as not covered at `docs/design/edge-cases.md:14`, `:22` and `docs/DESIGN.md:233`; asserted as expressible at `docs/design/first-consumer-walkthrough.md:201`. It appeared in three of five case studies, it is the one-to-many mirror of supersession, and it needs exactly D12 plus a declared write path for a composition part's owner.

### D30
**Derived attributes cannot be queried.** `docs/adr/0037:17` restricts query filters to indexed attributes, state and category; `docs/adr/0021:12` makes derived attributes never stored; `now`-dependent ones can never be indexed because the value changes with no write. ADR-0022's consequences say the automation layer polls such predicates "through the ordinary read surface". Overdue and low-stock alerts therefore have no path. The same unsupported claim appears at `case-study-tickets.md:40` and `case-study-approvals-and-bookings.md:25`.

### D31
**Constraint compilation is mis-cited and backend-dependent.** `docs/DESIGN.md:130` and `docs/adr/0001-store-owns-persistence.md:34` attribute a three-shape list to ADR-0023, which names only "uniqueness, a partial unique index" (`0023:14`). "Uniqueness per external source" comes from `docs/adr/0037:29`; "interval exclusion per key" appears only in `case-study-approvals-and-bookings.md:70`, which itself mis-cites ADR-0023. SQLite, a stated target (`docs/adr/0001:16`), has no exclusion constraint, so the same declaration gives different guarantees per backend with no caveat. `docs/adr/0002-requiredness-attaches-to-transitions.md:35` still asserts unannotated that "the database is not a safety net".

**Resolved by ADR-0041.** Serialisable isolation (ADR-0039) makes correctness backend-independent; compilation is an optimisation, reported at publish.

### D32
**External evaluators have no slot in the execution sequence.** `docs/DESIGN.md:161-169` never mentions them. A deferred guard is "checked only at execution" (`docs/adr/0008:14`), which places a third-party network call inside the write transaction while holding row locks, the cost ADR-0014 rejected in-process handlers for (`0014:45`).

### D33
**Quantity and serial tracking are incompatible.** `docs/design/edge-cases.md:17` defers quantity-tracked consumables to iteration 4; iteration 4 modelled quantity as counters on a Product (`orders:14`, `:20`) and never met a serialised unit. `docs/DESIGN.md:44` names SparePart among the types the model must carry and `walkthrough:9` serialises it. Nothing says how a delivery slot binds to a quantity, or how `reserve` means two things per tracking mode.

### D34
**Three Proposed ADRs are load-bearing.** ADR-0006, ADR-0008 and ADR-0017 are Proposed. `docs/DESIGN.md` states their content in the model block, the language table and the import path, marked "proposed" only in §5.2 and §5.5 prose. `docs/adr/0037:49` and `TODO.md:48` claim no open model questions remain while `TODO.md:105-108` still lists confirming all three.

### D35
**Erasure has no taint rule and may collide with uniqueness.** Nothing forbids `display_name := contact.email` where the source is personal and the target is not; erasure then misses the copy, in the row and in the event that wrote it. Separately, `docs/adr/0031:17` defines the redaction marker only at expression level, while `docs/adr/0037:29` makes external-identifier uniqueness an automatic invariant compiled to a constraint. Erasing two records whose personal attribute is also an external identifier writes the marker twice, and a sentinel would make a legally mandated operation impossible.

### D36
**`self-serviceable` is misused for the fan-out cap.** `docs/design/edge-cases.md:39` refuses an oversized cascade with `self-serviceable`, defined at `docs/DESIGN.md:120` as "Satisfiable by a transition argument". A fan-out cap is satisfiable by no argument, and for order placement "split the request" means creating two orders, a different business fact.

---

## Cosmetic

- **C01** `docs/adr/0013-events-are-recorded-to-a-durable-log-in-the-transition-transaction.md:34` diagram still reads "consumer holds a cursor", which ADR-0034 explicitly rejected at `0034:35`. **Resolved: diagram corrected.**
- **C02** `docs/DESIGN.md:83` lays out `actions` as a sibling of `transitions`, the visual shape of ADR-0016's rejected option B, though the text is correct.
- **C03** `docs/adr/0011-project-name-objectkeeper.md:42` attributes "gates rather than propels" to ADR-0004; it is ADR-0012's decision. **Resolved: citation corrected to ADR-0012.**
- **C04** `docs/adr/0004-composite-state-is-gated-not-derived.md:26` says a cascade-close "proposes" terminal transitions; since ADR-0036 "propose" is a reserved concept. **Resolved: reworded to "cascades".**
- **C05** The iteration-4 clarifications (outcomes may iterate an input's relationships; declared indexes; the fan-out cap) and the request `context` field live only in DESIGN.md and TODO.md with no ADR, against the convention that decisions live in ADRs.

**Resolved by ADR-0041.** A new `over-limit` verdict replaces the misuse of `self-serviceable`.

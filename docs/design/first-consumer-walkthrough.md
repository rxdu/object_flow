# First consumer walkthrough: the unit, the delivery, and what the model needs

Status: **worked example**, written 2026-09-07 as a draft and adopted in design iteration 1. It works the first consumer's real lifecycles through the model as it stood, finds where that model could not express them, and proposes the additions that became ADR-0019 to ADR-0025. It is kept as the rationale behind those ADRs; the current model is [`DESIGN.md`](../DESIGN.md).

> **Re-expressed 2026-09-08** against the grammar of ADR-0046, the semantics of ADR-0047 and the amendments of ADR-0052, which this re-expression is what found. Declarations here are current with the grammar; the surrounding prose records how the study reached them. *(A review on 2026-09-23 found three places where they differ from the production system's behaviour — `complete_sale` gates on the checklist and inspections where production does not, a unit's `CANCELLED` is terminal where production has transitions out of it, and `reserve` binds a unit only to a delivery where production also reserves for a service. They are listed in `TODO.md` for the table-to-type mapping, which re-derives the declarations from production and is what the harness fixture will be built from.)*
Source material: `wr:app/core/state_registry.py`, `wr:docs/proposals/operations-system-design.md` §4–§5, `wr:docs/adr/0002-unit-engagement-and-leasing-model.md`, `wr:docs/adr/0003-xero-as-source-of-truth-for-customer-identity.md`. The `wr:` prefix is defined in [`TODO.md`](../../TODO.md).

## 1. Method

Take the two object types that carry most of the first consumer's rules — the inventory **unit** (Robot, Accessory and SparePart share one lifecycle) and the **Delivery** — and write their transitions, guards and outcomes as the model would declare them. Where a declaration cannot be written, that is a gap. Then check the operations extension (procurement, allocation, intake, inspection, engagement, leasing) against the same mechanisms, and see whether anything else is needed.

Guards and outcomes below are written in the grammar of ADR-0046 and ADR-0052 and the semantics of ADR-0047, ADR-0053 and DESIGN.md §5.7.

## 2. The unit

### 2.1 Lifecycle, as extended by the operations design

> *Superseded 2026-09-24 by [`unit-journey.md`](unit-journey.md), which writes every unit transition production now registers as a checked module, with engagements, leases and service jobs beside the lifecycle. Production has changed since this was written: a unit flagged missing is cancelled rather than left in `PROCUREMENT`, and committing a batch releases its pegs rather than filling them (`design/defects.md` D280).*

```text
[*] ─ request ──────────▶ REQUESTED ─ ship ─────▶ PROCUREMENT ─ receive ─▶ INTAKE
[*] ─ add ──────────────────────────────────────────────────────────────▶ INTAKE
[*] ─ add_opening_stock ────────────────────────────────────────────────▶ AVAILABLE

INTAKE ─ inventorize ───▶ AVAILABLE ─ reserve(slot) ──▶ RESERVED ─ release ──▶ AVAILABLE
AVAILABLE ─ revert_intake ──▶ INTAKE                     (batch revert; guarded "pristine")

RESERVED ─ sell ──────────────▶ SOLD          RESERVED ─ deliver_internal ──▶ DEVELOPMENT
SOLD ─ accept_return ─────────▶ AVAILABLE     DEVELOPMENT ─ release_to_stock ─▶ AVAILABLE
DEVELOPMENT ─ retire(reason) ─▶ RETIRED       DEVELOPMENT ─ convert_lease ────▶ SOLD

REQUESTED | PROCUREMENT | INTAKE ─ cancel(reason) ──▶ CANCELLED
```

Terminal states: `RETIRED`, `CANCELLED`. Every transition is named and requested by name; none is addressed by its target state (ADR-0016).

### 2.2 Attributes

| Attribute | Class | Written by |
|---|---|---|
| `id` | store-assigned | ObjectKeeper, at creation or import (ADR-0018); never the serial |
| `serial` | attribute | the creation transitions, as input; uniqueness is an invariant |
| `model` | `ref` | the creation transitions |
| `manufacturer_serial` | attribute | action `capture_manufacturer_serial`, in `INTAKE` |
| `label_printed_at` | attribute | action `record_label_print`, in `INTAKE` |
| `photos` | `file[]` | action `attach_photo`, in `INTAKE` (ADR-0017) |
| `cancellation_reason`, `retirement_reason` | attribute | `cancel`, `retire`, as input |
| `procurement_order`, `shipment` | `ref` | `request`, `ship` |
| `binding` — the slot this unit is promised to | `ref` | `reserve` / `release`, and the pegging action while inbound |
| `notes` | attribute | action `edit(notes)`, guarded on a capability (ADR-0042) |

### 2.3 Guards, as the model would declare them

| Transition | Guard | Reads | Remedy class on failure |
|---|---|---|---|
| `inventorize` INTAKE → AVAILABLE | `label_printed_at is not null` | own attribute | `unreachable_from_here` — do `record_label_print` |
| | `model.manufacturer_serial_required → manufacturer_serial is not null` | own + referenced model | `unreachable_from_here` |
| | `model.label_photo_required → count(p in photos) >= 1` | own + model | `unreachable_from_here` |
| `reserve(slot)` AVAILABLE → RESERVED | `slot.model == model` | input + referenced | `self_serviceable` — choose another slot |
| | `slot.unit is null` | input | `dependent` |
| | `slot.delivery.state == PREPARATION` | referenced | `dependent` |
| | `actor.has(DELIVERY_EDIT)` | actor | `delegable` |
| `retire(reason)` DEVELOPMENT → RETIRED | `actor.has(ADMIN)` | actor | `delegable` |
| | `reason is not null` | input | `self_serviceable` |
| `sell` RESERVED → SOLD | *only via* `Delivery.complete_sale` | — | not requestable |
| `convert_lease` DEVELOPMENT → SOLD | *only via* `Lease.convert` | — | not requestable |
| `revert_intake` AVAILABLE → INTAKE | `binding is null or binding.delivery.state == PREPARATION` | referenced | `dependent` |
| | `none(s in Service where s.unit == this and s.state != CANCELLED)` | collection over a type | `dependent` |
| `cancel(reason)` | `reason in CancellationReason` | input | `self_serviceable` |

Everything in this table is a comparison, a null test, a membership test, a `count`, or an `all`/`none` over a relationship or a type, with the actor as a value. There is no arithmetic.

### 2.4 Invariants

- `serial` is unique within the type.
- At most one open engagement line per unit: `count(l in engagement_lines where l.engagement.state != CLOSED) <= 1`. It traverses two relationships, so ADR-0045 governs it and both hops need declared inverses; it is not the type-scan form of ADR-0052.
- A unit in `RESERVED` has exactly one `binding`; a unit in any other state has at most one (a soft peg).

### 2.5 What the model as written cannot express

Two rows above had no mechanism in the model as it stood when this was written. Both were closed in iteration 1, by ADR-0019 and ADR-0046.

1. **`sell` and `convert_lease` are "only via" another object's transition.** `RESERVED → SOLD` happens when the Delivery completes, in the same transaction, for every bound unit at once. The model routes cross-object consequences to consumers through events (ADR-0007, ADR-0012, ADR-0013). That is neither atomic nor able to block the delivery when a unit's guard fails. And if `sell` were requestable directly, a unit could be `SOLD` with no delivery — the backdoor the first consumer's ADR-0002 refused for `DEVELOPMENT → SOLD`.
2. **The outcome of `reserve` writes the other end of the binding.** `slot.unit := this` is a write on a *part* of a different object (the Delivery). The outcome of a transition must be allowed to reach across a declared relationship.

Both are the same gap seen from two sides: an outcome confined to one object.

## 3. The Delivery

### 3.1 Lifecycle and actions

```text
[*] ─ create(customer, type, date, address, configurations) ─▶ PREPARATION
PREPARATION ─ complete_sale ──────▶ DELIVERED     guard: type == DIRECT_SALE
PREPARATION ─ complete_internal ──▶ DELIVERED     guard: type == INTERNAL_DEVELOPMENT
PREPARATION ─ cancel ─────────────▶ CANCELLED
DELIVERED ─ cancel_delivered ─────▶ CANCELLED     admin; reverses the completion
CANCELLED ─ reopen ───────────────▶ PREPARATION   admin; re-reserves

actions in PREPARATION: add_slot, remove_slot, bind(slot, unit), unbind(slot),
                        tick_checklist(item), record_check(item, result),
                        apply_configuration, set_delivery_date
```

Two completion transitions rather than one with a branch. The inventory system's single `if DIRECT_SALE … else …` is the hazard its own ADR-0002 flagged (`wr:app/core/state_registry.py:101-114`): any new delivery type silently falls through to `DEVELOPMENT`. Splitting by guard makes the applicable transition appear in the availability list, and makes a type with no completion visibly stuck rather than silently wrong.

### 3.2 Parts and references

| Related object | Relationship | Why |
|---|---|---|
| Slot (`DeliveryItem`) | **composition** | exclusive to one delivery; dies with it |
| Checklist item, check record | **composition** | same |
| Unit | **reference** | outlives the delivery |
| Customer | **reference** | Xero-owned: an externally owned type (ADR-0080), and a `mirror` only while the legacy system still owns it (first consumer's ADR-0003) |
| WarrantyContract | **reference**, *created by* completion | outlives the delivery |

### 3.3 `complete_sale`, fully declared

```text
do complete_sale PREPARATION -> DELIVERED {
  require direct:   type == DeliveryType.DIRECT_SALE          because unreachable_from_here
  require checked:  all(c in checklist_items: c.checked)      because self_serviceable
  require filled:   none(s in slots
                         where s.role in { PRIMARY, INCLUDED } and s.unit is null)
                                                              because dependent
  require inspected: all(r in check_records where r.required: r.result == Result.PASS)
                                                              because dependent
  require invoiced: xero.invoice_valid(order_id) deferred     because dependent
  require may:      actor.has(DELIVERY_COMPLETE)              because delegable

  for s in slots where s.unit is not null limit 200 {
    call s.unit.sell()
  }
  for s in slots where s.warranty_product is not null limit 200 {
    create WarrantyContract.issue(robot    := s.unit,
                                  product  := s.warranty_product,
                                  customer := customer)
  }
}
```

Slot fill afterwards is a **derived view**, never stored: `derive fill = if unit is null then UNFILLED else if unit.state in { Robot.SOLD, Robot.DEVELOPMENT } then FULFILLED else FILLED`. It is named `fill` rather than `state` so that it cannot be mistaken for lifecycle state, which ADR-0004 keeps distinct. Guards and the read surface evaluate it.

`cancel_delivered` mirrors the outcome with `for s in slots where s.unit is not null limit 200 { call s.unit.accept_return() }` and a second loop calling `c.void()` on each warranty contract. `reopen` cascades `call s.unit.reserve(slot := s)` on each slot's remembered unit — and if any unit is no longer `AVAILABLE`, the whole `reopen` is blocked with a `dependent` verdict naming that unit. Today the inventory system's `_reserve_delivery_items` side effect raises part-way through.

## 4. Proposed mechanisms

Eight additions, each the smallest that closes a gap in §2.5 and §3 or a challenge in TODO.md. **Adopted in design iteration 1 as ADR-0019 (A and B), ADR-0020 (C), ADR-0021 (D), ADR-0022 (E), ADR-0023 (F), ADR-0024 (G) and ADR-0025 (H), pending author review.** The text below is retained as the worked rationale.

### A. Cascaded outcomes

A transition's outcome may include, besides its own state and attribute writes, **transitions on objects reached through declared relationships** (`for s in slots: s.unit.sell()`) and **creation of new objects** (`create WarrantyContract.issue(...)`). The full grammar is ADR-0046 as amended by ADR-0052.

**Superseded by ADR-0038**, which applies cascades sequentially: the parent's outcome lands first, then each cascade's guards are evaluated against the state produced so far. The original proposal here was that every guard be evaluated before anything is written, which made this walkthrough's own auto-fill example inexpressible. If all pass, every outcome commits in one transaction, and every cascaded transition is recorded as *caused by* the parent — the causal-lineage mechanism of ADR-0014, now used inside one transaction. Nothing initiates: the caller requested the parent, and the cascade is its declared consequence.

Consequences: ADR-0004's "a cascade-close proposes terminal transitions on each part" generalises from parts to any related object. ADR-0007's boundary is unchanged — a cascaded transition is inside the store; an *effect* is outside it. ADR-0013's log receives N+1 events from one transaction. Challenge 1 closes. Challenge 2 closes too, because auto-fill on receipt is `IntakeBatch.commit`'s cascade (§5), not a trigger.

### B. Outcomes are straight-line; branching lives in guards

An outcome may iterate any collection-valued expression, optionally filtered (`for s in slots where s.unit is not null`), but may not choose between different outcomes. Where behaviour differs by a value, declare one transition per case, each guarded on that value. The availability list then shows the applicable one, and a value that matches no transition is visible as an object with no way forward rather than a silent fall-through.

Optional declaration-time check: for an attribute with a closed value set, warn when some value matches no transition out of a state.

### C. Entry restriction — "only via"

A transition may declare that it is **not requestable by callers** and may occur only as a cascade from named parent transitions: `sell: RESERVED → SOLD, only via Delivery.complete_sale`. The readable rule set for the unit then says "`SOLD` is entered only via `Delivery.complete_sale`, `Lease.convert`" — an authority statement, inspectable, and the one that closes the `DEVELOPMENT → SOLD` backdoor the first consumer's ADR-0002 guarded against by hand.

### D. Derived attributes

A named expression in the guard language, declared on a type, **never stored**, evaluated on read. Guards may read it; the read surface returns it. Examples: `slot.fill` above; `unit.leasable := state == DEVELOPMENT and none(open engagement lines) and none(open blocking services)`; `engagement.overdue := state == OUT and expected_return < now`.

This is not the computation ADR-0007 excludes. Nothing is stored, so nothing can drift, and the expression is as inspectable as a guard. It resolves challenge 3 by wording: ADR-0004 rejects derived *lifecycle state* — a state with transitions nobody requested — and a derived attribute has no transitions.

Boundary to confirm: the guard language observed in the first consumer needs comparison, null tests, membership, `count`, `all`/`any`/`none` over a relationship or a type, the actor, and `now`. It did not need arithmetic *in this domain*, which is unit-tracked. ADR-0032 added arithmetic once three later case studies needed it, and ADR-0047 granted aggregates over types specifically so that available-to-promise is a derived attribute rather than a consumer query.

### E. Time is a value in guards, not a scheduler in the store

`now` is available to guards. A time-driven transition is an ordinary one — `WarrantyContract.expire: ACTIVE → EXPIRED, guard end_date ≤ now` — and ObjectKeeper never initiates it (ADR-0012). What it provides instead is a read query: **the objects for which transition T is currently available** — and only where T is *sweepable*, its guards decomposing into an indexable prefilter (ADR-0048). A time-driven transition that is not sweepable is refused by the query rather than scanned, which is a real constraint on how such a guard is written. A scheduler above asks that question and requests `expire` on each answer. One declared rule; no scheduler in the store; no drift between the type and its timing rule. This answers TODO.md's "inspectable but unexecuted timing metadata" — the guard *is* the metadata — and closes challenge 5.

### F. Locking and versions

**Superseded by ADR-0039.** The lock set cannot be computed in advance, because it is discovered by evaluating filters; locks are taken as objects are reached and correctness comes from serialisable isolation. The original proposal here was to lock every object the request would write before evaluating any guard. Concurrent conflicting requests serialise, and the second sees the first's result and fails its guards honestly. A request may also carry the version of the object the caller last read; if the object has moved since, the request is refused with a **stale** verdict, a distinct class from a guard failure because the remedy is "re-read", not "supply / wait / ask". Closes challenge 4.

### G. Deletion is a terminal transition gated on live references

Each deletable type declares a terminal state (`DELETED`, or a domain word such as `RETIRED`) and a transition into it, guarded on `none(referencing objects in a live state)`. Parts are cascaded, since they die with the whole; references are **not** — a Customer with live deliveries is blocked with a `dependent` verdict naming them, which is what would have prevented the 102-delivery cascade in the first consumer's ADR-0003. Legacy soft-deleted rows import into that state with provenance `asserted`. Closes challenge 6 and the soft-deleted-rows import item.

### H. The actor is a value supplied by the consumer

A request carries an actor: an identity, a kind (human, agent, service), the principal it acts for, and a set of capabilities. ObjectKeeper does not authenticate or manage users; the consumer's auth does, and hands the descriptor in. Guards test it: `actor.has(DELIVERY_COMPLETE)`, `actor.kind == human`. Where a guard needs a referenced person — `Service.create` requires an engineer who is a human user — that person is an ordinary object in the store and the guard reads its attributes. Delegation is deferred; proposals became a built-in type (ADR-0036): the first consumer's authority-versus-capability list is two items long.

## 5. The operations extension against A–H

> *The shipment and commit rows below describe production as it stood on 2026-09-07. [`unit-journey.md`](unit-journey.md) §5 lists what has changed since (`design/defects.md` D280).*

| Operation | Declared as |
|---|---|
| Raise a procurement order for N units | `for i in 1..inputs.quantity limit 500 { create Unit.request(model := inputs.model, order := this) }` (ADR-0046, ADR-0052) |
| Group units into a shipment | `ShippingRecord.create(units)` cascading `unit.ship` on each (A) |
| Receive a shipment | `ShippingRecord.arrive` cascading `unit.receive` on each reconciled unit; `unit.flag_missing` is an action that leaves it `PROCUREMENT` (A, B) |
| Intake work | actions on the unit in `INTAKE`: `record_label_print`, `attach_photo`, `capture_manufacturer_serial` (ADR-0016) |
| Commit a batch, auto-fill pegs | `for i in items limit 500 { call i.unit.inventorize() }` then a second loop calling `i.unit.reserve(slot := i.unit.binding)` where the peg is set. Sequential application (ADR-0038) is what makes the second loop's from-state guard pass |
| Revert a batch | `IntakeBatch.revert` cascading `unit.revert_intake`; blocked with the pinning unit named if any is not pristine (A, F) |
| Soft-peg an inbound unit | action `unit.peg(slot)` allowed in `REQUESTED`/`PROCUREMENT`/`INTAKE`/`AVAILABLE`; writes `binding` (ADR-0016) |
| Removing a slot frees inventory | two transitions, `remove_filled_slot` cascading `s.unit.release()` and `remove_pegged_slot` clearing the peg, each guarded on the slot's state. The "or" was a branch, which outcomes exclude |
| Pre-delivery inspection | `CheckRecord` parts of the delivery; `record_check` action; completion guard `all(required, PASS)` (composition) |
| Engagement out and return | `Engagement` object with its own lifecycle; invariant one-open-per-unit (ADR-0009); `unit.leasable` derived (D) |
| Lease-to-own | `Lease.convert` cascading `unit.convert_lease`, which is only-via (A, C) |
| Retire an engaged unit | `unit.retire` cascading `engagement_line.close(reason)` (A) |
| Xero-owned customer | Customer is an externally owned type with an external id, whose sync transitions Xero's sync requests (ADR-0080); `complete_sale` carries a deferred external guard (ADR-0008) |
| Jira reflection, alerts | consumers subscribed to the event log; overdue is a derived attribute queried by filtering its stored operand against the supplied time, and low stock by filtering the counter (ADR-0048) |
| Split delivery | `create d2 = Delivery.open(…)` then `for s in inputs.slots limit 200 { call s.reparent(delivery := d2) }` (ADR-0046, ADR-0052). Exclusive membership holds at every instant |

Every row is expressible, though several need mechanisms A–H did not have: the grammar of ADR-0046 and ADR-0052, sequential application (ADR-0038) and the read-path rules of ADR-0048. The rows above name them.

## 6. Still open after this

- **Identifier minting — closed by ADR-0018 and ADR-0029.** ObjectKeeper assigns every object a globally unique id, and a business identifier such as a serial is minted from a named, scoped sequence at creation. ADR-0029 withdrew ADR-0018's "no sequence primitive" consequence.
- **Guard language size — closed.** The floor stated here was the unit-tracked domain's. ADR-0032 added arithmetic, durations and aggregates; ADR-0047 fixed the semantics; ADR-0053 added the presence tests. DESIGN.md §5.7 is the current language.
- **Cascade depth and cycles — closed by ADR-0019.** The transition-reference graph must be acyclic; depth is visible in the declaration.
- **Which of A–H become ADRs — done.** ADR-0019 to ADR-0025.

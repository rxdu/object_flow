# First consumer walkthrough: the unit, the delivery, and what the model needs

Status: **worked example**, written 2026-09-07 as a draft and adopted in design iteration 1. It works the first consumer's real lifecycles through the model as it stood, finds where that model could not express them, and proposes the additions that became ADR-0019 to ADR-0025. It is kept as the rationale behind those ADRs; the current model is [`DESIGN.md`](../DESIGN.md).

> **Superseded notation.** This study was written before ADR-0046 gave outcomes a grammar and ADR-0047 gave the expression language a semantics. Several constructs it uses do not exist in the model as it now stands, and its conclusions are not evidence until it is re-expressed. See `defects.md` D11 to D24 and TODO.md.
Source material: `wr:app/core/state_registry.py`, `wr:docs/proposals/operations-system-design.md` §4–§5, `wr:docs/adr/0002-unit-engagement-and-leasing-model.md`, `wr:docs/adr/0003-xero-as-source-of-truth-for-customer-identity.md`. The `wr:` prefix is defined in [`TODO.md`](../../TODO.md).

## 1. Method

Take the two object types that carry most of the first consumer's rules — the inventory **unit** (Robot, Accessory and SparePart share one lifecycle) and the **Delivery** — and write their transitions, guards and outcomes as the model would declare them. Where a declaration cannot be written, that is a gap. Then check the operations extension (procurement, allocation, intake, inspection, engagement, leasing) against the same mechanisms, and see whether anything else is needed.

Guards below are written in a placeholder syntax. The guard language is not designed; the point is what it must be able to say.

## 2. The unit

### 2.1 Lifecycle, as extended by the operations design

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
| `serial` | controlled | the creation transitions, as input; uniqueness is an invariant |
| `model` | controlled reference | the creation transitions |
| `manufacturer_serial` | controlled | action `capture_manufacturer_serial`, in `INTAKE` |
| `label_printed_at` | controlled | action `record_label_print`, in `INTAKE` |
| `photos` | controlled, `file[]` | action `attach_photo`, in `INTAKE` (ADR-0017, proposed) |
| `cancellation_reason`, `retirement_reason` | controlled | `cancel`, `retire`, as input |
| `procurement_order`, `shipment` | controlled references | `request`, `ship` |
| `binding` — the slot this unit is promised to | controlled reference | `reserve` / `release`, and the pegging action while inbound |
| `notes` | free | any permitted actor; recorded |

### 2.3 Guards, as the model would declare them

| Transition | Guard | Reads | Remedy class on failure |
|---|---|---|---|
| `inventorize` INTAKE → AVAILABLE | `label_printed_at != null` | own attribute | `unreachable-from-here` — do `record_label_print` |
| | `model.manufacturer_serial_required → manufacturer_serial != null` | own + referenced model | `unreachable-from-here` |
| | `model.label_photo_required → count(photos) ≥ 1` | own + model | `unreachable-from-here` |
| `reserve(slot)` AVAILABLE → RESERVED | `slot.model == model` | input + referenced | `self-serviceable` — choose another slot |
| | `slot.unit == null` | input | `dependent` |
| | `slot.delivery.state == PREPARATION` | referenced | `dependent` |
| | `actor.has(DELIVERY_EDIT)` | actor | `delegable` |
| `retire(reason)` DEVELOPMENT → RETIRED | `actor.role == ADMIN` | actor | `delegable` |
| | `reason != null` | input | `self-serviceable` |
| `sell` RESERVED → SOLD | *only via* `Delivery.complete_sale` | — | not requestable |
| `convert_lease` DEVELOPMENT → SOLD | *only via* `Lease.convert` | — | not requestable |
| `revert_intake` AVAILABLE → INTAKE | `binding == null or binding.delivery.state == PREPARATION` | referenced | `dependent` |
| | `none(Service where unit == this and state != CANCELLED)` | collection over a type | `dependent` |
| `cancel(reason)` | `reason in CancellationReason` | input | `self-serviceable` |

Everything in this table is a comparison, a null test, a membership test, a `count`, or an `all`/`none` over a relationship or a type, with the actor as a value. There is no arithmetic.

### 2.4 Invariants

- `serial` is unique within the type.
- At most one open engagement line per unit: `count(engagement_lines where engagement.state != CLOSED) ≤ 1`.
- A unit in `RESERVED` has exactly one `binding`; a unit in any other state has at most one (a soft peg).

### 2.5 What the model as written cannot express

Two rows above have no mechanism in DESIGN.md today.

1. **`sell` and `convert_lease` are "only via" another object's transition.** `RESERVED → SOLD` happens when the Delivery completes, in the same transaction, for every bound unit at once. The model routes cross-object consequences to consumers through events (ADR-0007, ADR-0012, ADR-0013). That is neither atomic nor able to block the delivery when a unit's guard fails. And if `sell` were requestable directly, a unit could be `SOLD` with no delivery — the backdoor the first consumer's ADR-0002 refused for `DEVELOPMENT → SOLD`.
2. **The outcome of `reserve` writes the other end of the binding.** `slot.unit := this` is a controlled write on a *part* of a different object (the Delivery). The outcome of a transition must be allowed to reach across a declared relationship.

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
| Customer | **reference** | mirrored from Xero (first consumer's ADR-0003) |
| WarrantyContract | **reference**, *created by* completion | outlives the delivery |

### 3.3 `complete_sale`, fully declared

```text
complete_sale: PREPARATION → DELIVERED
  guards:
    type == DIRECT_SALE
    all(checklist_items, checked)
    none(slots where role in (PRIMARY, INCLUDED) and unit == null)      "no unfilled required slot"
    all(check_records where required, result == PASS)                   pre-delivery inspection, planned
    xero.invoice_valid(order_id)                                        external, deferred (ADR-0008)
    actor.has(DELIVERY_COMPLETE)
  outcome:
    state := DELIVERED
    for each slot with unit != null:        unit.sell                   cascaded transition; its own guards apply
    for each slot with warranty_product:    create WarrantyContract(robot := slot.unit,
                                                                     product := slot.warranty_product,
                                                                     customer := customer)
```

Slot state afterwards is a **derived view**, never stored: `slot.state := unit == null ? UNFILLED : unit.state in (SOLD, DEVELOPMENT) ? FULFILLED : FILLED`. Guards and the read surface evaluate it.

`cancel_delivered` mirrors the outcome: `for each slot with unit: unit.accept_return`; `for each contract: contract.void`. `reopen` cascades `unit.reserve(slot)` on each slot's remembered unit — and if any unit is no longer `AVAILABLE`, the whole `reopen` is blocked with a `dependent` verdict naming that unit. Today the inventory system's `_reserve_delivery_items` side effect raises part-way through.

## 4. Proposed mechanisms

Eight additions, each the smallest that closes a gap in §2.5 and §3 or a challenge in TODO.md. **Adopted in design iteration 1 as ADR-0019 (A and B), ADR-0020 (C), ADR-0021 (D), ADR-0022 (E), ADR-0023 (F), ADR-0024 (G) and ADR-0025 (H), pending author review.** The text below is retained as the worked rationale.

### A. Cascaded outcomes

A transition's outcome may include, besides its own state and attribute writes, **transitions on objects reached through declared relationships** (`for each slot.unit: unit.sell`) and **creation of new objects** (`create WarrantyContract(...)`).

Every guard — the parent's and each cascaded transition's — is evaluated before anything is written. If any fails, the whole request is blocked and the verdict names the object and the guard. If all pass, every outcome commits in one transaction, and every cascaded transition is recorded as *caused by* the parent — the causal-lineage mechanism of ADR-0014, now used inside one transaction. Nothing initiates: the caller requested the parent, and the cascade is its declared consequence.

Consequences: ADR-0004's "a cascade-close proposes terminal transitions on each part" generalises from parts to any related object. ADR-0007's boundary is unchanged — a cascaded transition is inside the store; an *effect* is outside it. ADR-0013's log receives N+1 events from one transaction. Challenge 1 closes. Challenge 2 closes too, because auto-fill on receipt is `IntakeBatch.commit`'s cascade (§5), not a trigger.

### B. Outcomes are straight-line; branching lives in guards

An outcome may iterate a relationship, optionally filtered (`for each slot with unit != null`), but may not choose between different outcomes. Where behaviour differs by a value, declare one transition per case, each guarded on that value. The availability list then shows the applicable one, and a value that matches no transition is visible as an object with no way forward rather than a silent fall-through.

Optional declaration-time check: for an attribute with a closed value set, warn when some value matches no transition out of a state.

### C. Entry restriction — "only via"

A transition may declare that it is **not requestable by callers** and may occur only as a cascade from named parent transitions: `sell: RESERVED → SOLD, only via Delivery.complete_sale`. The readable rule set for the unit then says "`SOLD` is entered only via `Delivery.complete_sale`, `Lease.convert`" — an authority statement, inspectable, and the one that closes the `DEVELOPMENT → SOLD` backdoor the first consumer's ADR-0002 guarded against by hand.

### D. Derived attributes

A named expression in the guard language, declared on a type, **never stored**, evaluated on read. Guards may read it; the read surface returns it. Examples: `slot.state` above; `unit.leasable := state == DEVELOPMENT and none(open engagement lines) and none(open blocking services)`; `engagement.overdue := state == OUT and expected_return < now`.

This is not the computation ADR-0007 excludes. Nothing is stored, so nothing can drift, and the expression is as inspectable as a guard. It resolves challenge 3 by wording: ADR-0004 rejects derived *lifecycle state* — a state with transitions nobody requested — and a derived attribute has no transitions.

Boundary to confirm: the guard language observed in the first consumer needs comparison, null tests, membership, `count`, `all`/`any`/`none` over a relationship or a type, the actor, and `now`. It does not need arithmetic. Available-to-promise (on-hand − allocated + inbound) is arithmetic over a whole type and stays a consumer query over the read surface, which is how the first consumer already treats it (`wr:app/services/atp.py`).

### E. Time is a value in guards, not a scheduler in the store

`now` is available to guards. A time-driven transition is an ordinary one — `WarrantyContract.expire: ACTIVE → EXPIRED, guard end_date ≤ now` — and ObjectKeeper never initiates it (ADR-0012). What it provides instead is a read query: **the objects for which transition T is currently available.** A scheduler above asks that question and requests `expire` on each answer. One declared rule; no scheduler in the store; no drift between the type and its timing rule. This answers TODO.md's "inspectable but unexecuted timing metadata" — the guard *is* the metadata — and closes challenge 5.

### F. Locking and versions

A transition request runs in one transaction that locks every object it will write — the target and every cascaded object — evaluates all guards under those locks, then writes. Concurrent conflicting requests serialise, and the second sees the first's result and fails its guards honestly. A request may also carry the version of the object the caller last read; if the object has moved since, the request is refused with a **stale** verdict, a distinct class from a guard failure because the remedy is "re-read", not "supply / wait / ask". Closes challenge 4.

### G. Deletion is a terminal transition gated on live references

Each deletable type declares a terminal state (`DELETED`, or a domain word such as `RETIRED`) and a transition into it, guarded on `none(referencing objects in a live state)`. Parts are cascaded, since they die with the whole; references are **not** — a Customer with live deliveries is blocked with a `dependent` verdict naming them, which is what would have prevented the 102-delivery cascade in the first consumer's ADR-0003. Legacy soft-deleted rows import into that state with provenance `asserted`. Closes challenge 6 and the soft-deleted-rows import item.

### H. The actor is a value supplied by the consumer

A request carries an actor: an identity, a kind (human, agent, service), the principal it acts for, and a set of capabilities. ObjectKeeper does not authenticate or manage users; the consumer's auth does, and hands the descriptor in. Guards test it: `actor.has(DELIVERY_COMPLETE)`, `actor.kind == human`. Where a guard needs a referenced person — `Service.create` requires an engineer who is a human user — that person is an ordinary object in the store and the guard reads its attributes. Delegation and proposals are deferred: the first consumer's authority-versus-capability list is two items long.

## 5. The operations extension against A–H

| Operation | Declared as |
|---|---|
| Raise a procurement order for N units | `ProcurementOrder.create(...)` cascading `create Unit(state := REQUESTED, serial := input)` ×N (A) |
| Group units into a shipment | `ShippingRecord.create(units)` cascading `unit.ship` on each (A) |
| Receive a shipment | `ShippingRecord.arrive` cascading `unit.receive` on each reconciled unit; `unit.flag_missing` is an action that leaves it `PROCUREMENT` (A, B) |
| Intake work | actions on the unit in `INTAKE`: `record_label_print`, `attach_photo`, `capture_manufacturer_serial` (ADR-0016) |
| Commit a batch, auto-fill pegs | `IntakeBatch.commit` cascading `unit.inventorize` on each unit, then `unit.reserve(binding.slot)` on each unit carrying a soft peg — no trigger, one transaction (A) |
| Revert a batch | `IntakeBatch.revert` cascading `unit.revert_intake`; blocked with the pinning unit named if any is not pristine (A, F) |
| Soft-peg an inbound unit | action `unit.peg(slot)` allowed in `REQUESTED`/`PROCUREMENT`/`INTAKE`/`AVAILABLE`; writes `binding` (ADR-0016) |
| Removing a slot frees inventory | `Delivery.remove_slot` cascading `unit.release`, or clearing the peg (A) |
| Pre-delivery inspection | `CheckRecord` parts of the delivery; `record_check` action; completion guard `all(required, PASS)` (composition) |
| Engagement out and return | `Engagement` object with its own lifecycle; invariant one-open-per-unit (ADR-0009); `unit.leasable` derived (D) |
| Lease-to-own | `Lease.convert` cascading `unit.convert_lease`, which is only-via (A, C) |
| Retire an engaged unit | `unit.retire` cascading `engagement_line.close(reason)` (A) |
| Xero-owned customer | Customer is a mirror with an external id; `complete_sale` carries a deferred external guard (ADR-0008) |
| Jira reflection, alerts | consumers subscribed to the event log; overdue and low-stock predicates are derived attributes or consumer queries polled by the automation layer (D, E) |
| Split delivery (deferred there) | a transition on the source delivery cascading slot moves to a new delivery; exclusive membership holds at every instant (A) |

Nothing in the table needs a mechanism beyond A–H.

## 6. Still open after this

- **Identifier minting — closed by ADR-0018.** ObjectKeeper assigns every object a globally unique id; serials are business identifiers the consumer mints and supplies as creation input, with uniqueness an invariant. No sequence primitive.
- **Guard language size.** D states the observed floor. Confirm "no arithmetic" before the language is designed; the first counter-example decides it.
- **Cascade depth and cycles — closed by ADR-0019.** The transition-reference graph must be acyclic; depth is visible in the declaration.
- **Which of A–H become ADRs — done.** ADR-0019 to ADR-0025.

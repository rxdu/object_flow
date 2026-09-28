# Case study: orders at volume (e-commerce-shaped)

Status: design iteration 4, 2026-09-08. Companion to the earlier case studies. Decisions taken here are ADR-0032 to ADR-0034 plus two clarifications, accepted by the author on 2026-09-28. No sibling repository holds an order system, so this study uses the standard order-to-cash shape rather than observed code; it is the second structurally different case TODO.md asked for — many, short-lived objects — and the one that forced the arithmetic decision.

> **Amended 2026-09-25 for ADR-0114.** ObjectFlow records who acted and never evaluates it. The declarations below lost every clause that read who is asking or declared who may do or see something — capabilities, actor guards, visibility — which the upper layer now decides; their mapping rows say so. The narrative records the model as the study found it, before ADR-0114.

*Vocabulary, noted 2026-09-24:* written before PRD revision 5, this document says "consumer" for an application built on the store, which PRD §5 now calls an upper-layer application, and sometimes for the deployment or a reader; "the first consumer" keeps its meaning (D368).

> **Re-expressed 2026-09-08** against the grammar of ADR-0046, the semantics of ADR-0047 and the amendments of ADR-0052, which this re-expression is what found. Declarations here are current; the surrounding prose records how the study reached them.
>
> **Rewritten 2026-09-26 in the flow description format** (ADR-0116). The placement of §2a is an excerpt of the module in the appendix, written then so that the excerpt is part of a checked module (ADR-0121); `scripts/check-flow-docs.py` checks both.
## 1. Why this case

Every earlier case has few, long-lived, richly related objects. An order system has millions of small ones that live for days, arrive in bursts, and are written by anonymous callers who retry. Stock is a **quantity**, not a serialised unit, so a guard compares numbers and an outcome subtracts them. Money is everywhere, and money means sums. Payment truth lives in a **gateway**, not the store. The event log becomes the busiest table in the system, which forces the ordering, retention and stored-versus-folded questions TODO.md had left open.

## 2. Mapping

| Order-system concept | In this model |
|---|---|
| Product, variant (SKU) | object types; a variant is a part or a separate type extending `Product` (ADR-0026) |
| Quantity stock: on hand, reserved, available | a quantity-tracked type (ADR-0050); `available := on_hand - reserved` is derived and indexable, since it reads stored attributes and no clock (ADR-0048) |
| Cart; add / remove / change line | an object in a draft-like state; lines are parts; edits are actions (ADR-0016) |
| Place order | a creation transition on `Order` cascading `create OrderLine` from the cart's lines and `product.reserve(qty)` on each product, in one transaction (ADR-0019); the cart ends `converted`, superseded by the order (ADR-0028) |
| Line price captured at placement | an input written to the line, never a reference to the product's current price |
| Order total, line subtotal | derived: `sum(l in lines: l.qty * l.unit_price)` (ADR-0032) |
| Tax, discount, shipping cost | domain formulas: the consumer computes and supplies them as inputs; guards check ranges and consistency (`inputs.tax >= 0`) (ADR-0007) |
| Reserve stock; oversell prevention | `Product.reserve`: guard `on_hand - reserved >= inputs.qty`; outcome `reserved := reserved + inputs.qty`, read-modify-write at the moment it is applied, so two lines on one product cannot both pass (ADR-0038) |
| Payment authorised / captured / refunded | a `Payment` object that is an externally owned type for the gateway (ADR-0008, ADR-0080); the consumer receives the gateway's webhook and requests the transition with the gateway event id as idempotency key (ADR-0014) |
| Order paid | `Order.mark_paid` cascaded from `Payment.capture`, only-via (ADR-0020); guard `payment.amount == total` |
| Unpaid order auto-cancels after 30 minutes | `cancel_unpaid: placed → cancelled, guard placed_at + 30 min <= now`; a scheduler requests it through the availability query (ADR-0022, ADR-0032 for the duration) |
| Fulfilment, partial shipment | `Shipment` objects with line quantities as link objects; the invariant is declared on the line as a relationship aggregate, `sum(a in allocations: a.qty) <= qty`, since ADR-0047 refuses grouped aggregation |
| Return, refund | transitions on order lines and `Payment.refund(amount)` with `sum(r in refunds: r.amount) <= captured` |
| Fraud hold, manual review | states; who may review is the upper layer's (ADR-0114) |
| Guest checkout | the consumer's own actor makes the request, since every request names an actor the store holds (`DESIGN.md` §5.8, `actor_known`); the guest is only the order's `customer`, an identity the consumer mints. *(Corrected 2026-09-28, D475: this row gave the guest an ephemeral actor id, which the store would refuse.)* |
| Retrying client placing the same order twice | idempotency key on the placement request (ADR-0014); the cascade is one request, so one key |
| Fulfilment system, email, analytics | upper-layer applications pulling the log (ADR-0013, ADR-0105); open-ended analytics exports from the log. *(Amended 2026-09-24, D350: declared metrics are computed in the store since ADR-0081 and ADR-0084, so only open-ended analysis is outside it.)* |
| Stock ledger / movement history | the event log is the ledger — each reserve, release, receive is a recorded event; no second stream (ADR-0033) |

## 2a. Placement, declared

```yaml
Order:
  conditions:
    open:
      description: The cart is still open.
      expression: inputs.cart.state == Cart.ACTIVE
      remedy: dependent
    lines:
      description: The cart has at least one line.
      expression: count(l in inputs.cart.lines) > 0
      remedy: self_serviceable
  transitions:
    place:
      kind: initial
      to: PLACED
      required_inputs: [customer, address]
      inputs:
        cart: { reference: Cart }
      guards:
        open: deny
        lines: deny
      effect:
        - foreach:
            item: l
            array: inputs.cart.lines
            limit: 500
            steps:
              - create: { type: OrderLine, transition: add, inputs: { order: this, product: l.product, qty: l.qty, unit_price: l.product.price } }
              - call: { target: l.product, transition: reserve, inputs: { qty: l.qty } }
        - call: { target: inputs.cart, transition: convert, inputs: { successor: this } }
```

The placement is an excerpt of `Order` in the module of the appendix, which holds what placing an order needs and no more. Three things in that declaration exist only because of the repair. `this` is usable inside a creation outcome, so the order can create its own lines (ADR-0052). The reservation cascade takes an input (ADR-0046). And because cascades apply sequentially and writes are read-modify-write (ADR-0038), two lines for one product are evaluated against each other, so an order can no longer oversell itself. Under the superseded rule both guards read the same pre-write count and both passed.

The price is snapshotted onto the line rather than referenced, for the reason the first consumer learned about applied configurations: a delivery records what it promised, not what the catalogue says today.

## 3. What the model could not say, and what was decided

**Numbers.** `reserved := reserved + qty`, `sum(lines.qty * unit_price)`, `placed_at + 30 min <= now`: three shapes of arithmetic the version-1 language (ADR-0021) refused, and the third, second and first case studies had each asked for one of them (available-to-promise, deal totals and activity recency, and now stock). **ADR-0032** adds arithmetic on numbers and durations, and `sum`, `min`, `max` over a relationship. It draws ADR-0007's line more precisely: the store evaluates *declared arithmetic over its own data*; it does not own *domain formulas* — tax, pricing, scoring, currency conversion — which remain consumer inputs. The language still has no string operations, no user-defined functions and no external calls beyond ADR-0008 evaluators. *Moved by ADR-0081 and ADR-0084:* the line is now the store's own data, so any declared formula over it is in scope, scores and cross-object metrics included, and a guard may also read a declared metric; tax, pricing and currency conversion stay consumer inputs because they need rules or data the store does not hold (`DESIGN.md` §5.7).

**Where current state lives.** Guards read state on every request; at this volume a fold over events per read is not an option, and derived attributes and type-scan guards need indexes on current values. **ADR-0033**: current state is stored — one row per object with its attributes, state, version and the id of its last event — and the event log is the permanent history; a fold of the log must reproduce the row, which a repair tool can check. Provenance attaches to the event: actor, principal, context, cause, and a `source` of `observed`, `asserted`, `migrated`, `corrected` or `erased`. And because the log *is* the history, it is **never pruned**; retention is archival tiering that keeps old events readable. That dissolves "retention versus slow consumers": nothing is pinned because nothing is removed.

**Order and subscribers.** A busy log needs a stated ordering guarantee and a subscription model that survives a dead consumer. **ADR-0034**: events are strictly ordered per object and causally ordered across a cascade; a global position exists for cursors and is monotonic, but a pull cursor must tolerate a bounded window in which a lower position becomes visible after a higher one. Subscriptions are a built-in object type with a filter over type or family, transition names, and the `changes_state` flag, and a lifecycle of `active → revoked`, with lag and death **derived** from the acknowledged position rather than states anything drives (ADR-0043 superseded the three-state form proposed here). Attribute-level filters are not offered; the consumer filters after delivery.

**Two clarifications.** An outcome may iterate any collection-valued expression, including a relationship reached from an input (a `foreach` over `inputs.cart.lines` with a `limit` of 500, ADR-0046, ADR-0052). And a type may declare which attributes are **indexed**; publishing reports which transitions are sweepable and which derived attributes are queryable, and **rejects** a type-scan guard or visibility predicate that reads an unindexed attribute (check 7).

## 4. What held without change

*Revisited after the repair.* Cascaded creation of lines and reservation in one transaction is ADR-0019, now applied sequentially per ADR-0038, which is what closes the self-oversell this study originally missed. The row lock on a product is now for contention rather than correctness: ADR-0039 makes serialisable isolation the guarantee. *Corrected by ADR-0090:* this sentence used to say a contended product queues on the row lock rather than aborting and retrying; probed on PostgreSQL 16.8, it waits and then fails with a serialisation error, which the retry absorbs, so the hot-row analysis below should be read as wait-then-retry. Payment as an externally owned type (ADR-0069, ADR-0080), with the gateway event as idempotency key, is ADR-0008 and ADR-0014. Auto-cancel is ADR-0022. Guest actors fit ADR-0025. Snapshotting the price at placement is the same rule the inventory system learned about applied configurations. Lifecycles were unaffected. Guards, verdicts and only-via all changed later: three-valued semantics and declared remedy classes (ADR-0047), the `over-limit` verdict (ADR-0041), and only-via transitions no longer appearing in availability (ADR-0041).

## 5. Rare cases, recorded in `edge-cases.md`

- Hot rows: a flash-sale product serialises every reservation on one lock. *Corrected by ADR-0090:* on PostgreSQL the reservations do not queue on the lock; a contended one waits, then fails with a serialisation error and is retried to a declared bound, so it pays the wait and the retry, and each event records its retries. On SQLite every transaction begins `IMMEDIATE`, so writers are serial and a contended one waits out the busy timeout.
- Multi-warehouse or multi-region stock.
- Very large cascades (a cart of thousands of lines).
- Gapless order numbering under rollback.
- Analytics over the whole log.

## Appendix: the module the placement belongs to

What §2a's placement needs and no more: a product whose stock is counted, a cart and its lines, and an order and its lines. A cart line is created through its cart, as a part is (the model's check 11), and the cart's lines survive its conversion, since the order supersedes the cart and does not take its lines. The customer is an `identity`, since a guest is an id the upper layer mints (§2).

```yaml
module: orders
categories: [live, closed]

types:
  Product:
    description: A product whose stock is counted, not tracked unit by unit.
    tracking: quantity

    attributes:
      name:     { type: string, indexed: true }
      price:    { type: money(USD) }
      on_hand:  { type: counter, indexed: true }
      reserved: { type: counter, indexed: true }

    states:
      ACTIVE:       { category: live }
      DISCONTINUED: { category: closed, final: true }

    derived_attributes:
      available:
        description: The stock on hand and not reserved; a query may filter on it.
        expression: on_hand - reserved
        indexed: true

    conditions:
      in_stock:
        description: Enough stock is available for the quantity reserved.
        expression: on_hand - reserved >= inputs.qty
        remedy: dependent

    transitions:
      create:
        kind: initial
        to: ACTIVE
        required_inputs: [name, price]
      receive_stock:
        kind: internal
        from: ACTIVE
        inputs:
          qty: { type: int }
        effect:
          - assign: { location: on_hand, expr: on_hand + inputs.qty }
      reserve:
        kind: internal
        from: ACTIVE
        only_via: [Order.place]
        inputs:
          qty: { type: int }
        guards:
          in_stock: deny
        effect:
          - assign: { location: reserved, expr: reserved + inputs.qty }
      discontinue:
        kind: external
        from: ACTIVE
        to: DISCONTINUED

  Cart:
    description: A shopper's cart, converted into the order placed from it.
    tracking: record

    attributes:
      lines:
        reference: "CartLine[]"
        aggregation: composite
        opposite: cart
        survives: [convert]

    states:
      ACTIVE:    { category: live }
      CONVERTED: { category: closed, final: true, superseding: true }

    transitions:
      open:
        kind: initial
        to: ACTIVE
      add_line:
        kind: internal
        from: ACTIVE
        inputs:
          product: { reference: Product }
          qty:     { type: int }
        effect:
          - create: { type: CartLine, transition: add, inputs: { cart: this, product: inputs.product, qty: inputs.qty } }
      convert:
        kind: external
        from: ACTIVE
        to: CONVERTED
        only_via: [Order.place]
        inputs:
          successor: { reference: Order }
        effect:
          - supersede: inputs.successor

  CartLine:
    description: A quantity of one product in a cart.
    tracking: record

    attributes:
      cart:    { reference: Cart, opposite: lines }
      product: { reference: Product }
      qty:     { type: int }

    states:
      IN_CART: { category: live }
      REMOVED: { category: closed, final: true }

    transitions:
      add:
        kind: initial
        to: IN_CART
        only_via: [Cart.add_line]
        required_inputs: [cart, product, qty]
      change_qty:
        kind: internal
        from: IN_CART
        required_inputs: [qty]
      remove:
        kind: external
        from: IN_CART
        to: REMOVED

  Order:
    description: An order placed from a cart, which snapshots each line's price and reserves its stock as it is placed.
    tracking: record

    attributes:
      customer: { type: identity, indexed: true, description: "The customer's id, a guest's included." }
      address:  { type: string }
      lines:
        reference: "OrderLine[]"
        aggregation: composite
        opposite: order
        survives: [cancel, complete]

    states:
      PLACED:    { category: live }
      COMPLETED: { category: closed, final: true }
      CANCELLED: { category: closed, final: true }

    derived_attributes:
      total:
        description: The order's total, from the prices its lines captured.
        expression: "sum(l in lines: l.qty * l.unit_price)"

    conditions:
      open:
        description: The cart is still open.
        expression: inputs.cart.state == Cart.ACTIVE
        remedy: dependent
      lines:
        description: The cart has at least one line.
        expression: count(l in inputs.cart.lines) > 0
        remedy: self_serviceable

    transitions:
      place:
        kind: initial
        to: PLACED
        required_inputs: [customer, address]
        inputs:
          cart: { reference: Cart }
        guards:
          open: deny
          lines: deny
        effect:
          - foreach:
              item: l
              array: inputs.cart.lines
              limit: 500
              steps:
                - create: { type: OrderLine, transition: add, inputs: { order: this, product: l.product, qty: l.qty, unit_price: l.product.price } }
                - call: { target: l.product, transition: reserve, inputs: { qty: l.qty } }
          - call: { target: inputs.cart, transition: convert, inputs: { successor: this } }
      complete:
        kind: external
        from: PLACED
        to: COMPLETED
      cancel:
        kind: external
        from: PLACED
        to: CANCELLED

  OrderLine:
    description: A quantity of one product on an order, at the price it was placed at.
    tracking: record

    attributes:
      order:      { reference: Order, opposite: lines }
      product:    { reference: Product }
      qty:        { type: int }
      unit_price: { type: money(USD) }

    states:
      PLACED: { category: closed, final: true }

    transitions:
      add:
        kind: initial
        to: PLACED
        only_via: [Order.place]
        required_inputs: [order, product, qty, unit_price]
```

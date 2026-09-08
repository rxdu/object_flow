# Case study: orders at volume (e-commerce-shaped)

Status: design iteration 4, 2026-09-08. Companion to the earlier case studies. Decisions taken here are ADR-0032 to ADR-0034 plus two clarifications, all pending author review. No sibling repository holds an order system, so this study uses the standard order-to-cash shape rather than observed code; it is the second structurally different case TODO.md asked for — many, short-lived objects — and the one that forced the arithmetic decision.

> **Superseded notation.** This study was written before ADR-0046 gave outcomes a grammar and ADR-0047 gave the expression language a semantics. Several constructs it uses do not exist in the model as it now stands, and its conclusions are not evidence until it is re-expressed. See `defects.md` D11 to D24 and TODO.md.
## 1. Why this case

Every earlier case has few, long-lived, richly related objects. An order system has millions of small ones that live for days, arrive in bursts, and are written by anonymous callers who retry. Stock is a **quantity**, not a serialised unit, so a guard compares numbers and an outcome subtracts them. Money is everywhere, and money means sums. Payment truth lives in a **gateway**, not the store. The event log becomes the busiest table in the system, which forces the ordering, retention and stored-versus-folded questions TODO.md had left open.

## 2. Mapping

| Order-system concept | In this model |
|---|---|
| Product, variant (SKU) | object types; a variant is a part or a separate type extending `Product` (ADR-0026) |
| Quantity stock: on hand, reserved, available | controlled numeric attributes; `available := on_hand - reserved` is derived (ADR-0032) |
| Cart; add / remove / change line | an object in a draft-like state; lines are parts; edits are actions (ADR-0016) |
| Place order | a creation transition on `Order` cascading `create OrderLine` from the cart's lines and `product.reserve(qty)` on each product, in one transaction (ADR-0019); the cart ends `converted`, superseded by the order (ADR-0028) |
| Line price captured at placement | an input written to the line, never a reference to the product's current price |
| Order total, line subtotal | derived: `sum(lines.qty * lines.unit_price)` (ADR-0032) |
| Tax, discount, shipping cost | domain formulas: the consumer computes and supplies them as inputs; guards check ranges and consistency (`inputs.tax >= 0`) (ADR-0007) |
| Reserve stock; oversell prevention | `Product.reserve(qty)`: guard `on_hand - reserved >= inputs.qty`; outcome `reserved := reserved + inputs.qty`; under the row lock (ADR-0023) |
| Payment authorised / captured / refunded | a `Payment` object mirroring the gateway (ADR-0008 mirror shape); the consumer receives the gateway's webhook and requests the transition with the gateway event id as idempotency key (ADR-0014) |
| Order paid | `Order.mark_paid` cascaded from `Payment.capture`, only-via (ADR-0020); guard `payment.amount == total` |
| Unpaid order auto-cancels after 30 minutes | `cancel_unpaid: placed → cancelled, guard placed_at + 30 min <= now`; a scheduler requests it through the availability query (ADR-0022, ADR-0032 for the duration) |
| Fulfilment, partial shipment | `Shipment` objects with line quantities as link objects; invariant `sum(shipments.qty for a line) <= line.qty` (ADR-0032) |
| Return, refund | transitions on order lines and `Payment.refund(amount)` with `sum(refunds) <= captured` |
| Fraud hold, manual review | states plus actor guards (ADR-0025) |
| Guest checkout | an actor of kind `human` with an ephemeral id the consumer mints; nothing in the store cares |
| Retrying client placing the same order twice | idempotency key on the placement request (ADR-0014); the cascade is one request, so one key |
| Fulfilment system, email, analytics | subscribers to the log (ADR-0013); analytics exports from the log, it does not query the store |
| Stock ledger / movement history | the event log is the ledger — each reserve, release, receive is a recorded event; no second stream (ADR-0033) |

## 3. What the model could not say, and what was decided

**Numbers.** `reserved := reserved + qty`, `sum(lines.qty * unit_price)`, `placed_at + 30 min <= now`: three shapes of arithmetic the version-1 language (ADR-0021) refused, and the third, second and first case studies had each asked for one of them (available-to-promise, deal totals and activity recency, and now stock). **ADR-0032** adds arithmetic on numbers and durations, and `sum`, `min`, `max` over a relationship. It draws ADR-0007's line more precisely: the store evaluates *declared arithmetic over its own data*; it does not own *domain formulas* — tax, pricing, scoring, currency conversion — which remain consumer inputs. The language still has no string operations, no user-defined functions and no external calls beyond ADR-0008 evaluators.

**Where current state lives.** Guards read state on every request; at this volume a fold over events per read is not an option, and derived attributes and type-scan guards need indexes on current values. **ADR-0033**: current state is stored — one row per object with its attributes, state, version and the id of its last event — and the event log is the permanent history; a fold of the log must reproduce the row, which a repair tool can check. Provenance attaches to the event: actor, principal, context, cause, and a `source` of `observed`, `asserted`, `migrated`, `corrected` or `erased`. And because the log *is* the history, it is **never pruned**; retention is archival tiering that keeps old events readable. That dissolves "retention versus slow consumers": nothing is pinned because nothing is removed.

**Order and subscribers.** A busy log needs a stated ordering guarantee and a subscription model that survives a dead consumer. **ADR-0034**: events are strictly ordered per object and causally ordered across a cascade; a global position exists for cursors and is monotonic, but a pull cursor must tolerate a bounded window in which a lower position becomes visible after a higher one. Subscriptions are a built-in object type with a filter over type or family, transition names, and the `changes_state` flag, and a lifecycle `active → lagging → dead-lettered` driven by acknowledgement lag. Attribute-level filters are not offered; the consumer filters after delivery.

**Two clarifications.** An outcome may iterate a relationship reached from an *input* as well as from `this` (`for each inputs.cart.lines: create OrderLine`). And a type may declare which attributes are **indexed**; publishing a declaration whose type-scan guard or visibility predicate uses an unindexed attribute produces a warning in the publish report (ADR-0027).

## 4. What held without change

Cascaded creation of lines and reservation in one transaction is ADR-0019. The row lock on a product is ADR-0023 doing what the inventory system's reservation lock did. Payment as a mirror with the gateway event as idempotency key is ADR-0008 and ADR-0014. Auto-cancel is ADR-0022. Guest actors fit ADR-0025. Snapshotting the price at placement is the same rule the inventory system learned about applied configurations. Nothing about lifecycles, guards, verdicts or only-via changed.

## 5. Rare cases, recorded in `edge-cases.md`

- Hot rows: a flash-sale product serialises every reservation on one lock.
- Multi-warehouse or multi-region stock.
- Very large cascades (a cart of thousands of lines).
- Gapless order numbering under rollback.
- Analytics over the whole log.

# The first consumer's cutover

Draft, 2026-09-09, amended 2026-09-23. The stage order for `~/RduWs/wr_inventory_management`, extracted from its schema. ADR-0075 decided that cutover is staged by object type and that a type migrates only after every type referencing it has; this is that order for this system, and what it depends on.

**How it was obtained.** The SQLAlchemy metadata was imported and walked programmatically rather than read: 45 tables, 67 declared foreign keys, verified by asserting set equality between the extraction and a hand-typed list. Cross-checked against the Alembic chain, which is linear with a single head. **The live database was not reachable**, so everything here comes from source and a reflection of the running schema is the right check before anyone commits to an order.

**Amended 2026-09-23 for ADR-0093.** The order below counts references only, and the rule now counts the legacy system's writes too: a type that a legacy transition writes as a side effect migrates no earlier than the writer. §4a says what that does to this system. The table-level order of §3 is **superseded** until it is recomputed over types, with both kinds of edge, once the per-type mapping exists; its findings about shape stand except where §4a overturns them.

## 1. The caveat that governs everything below

**A table is not a type.** The order below is over 45 tables and the stage unit is an ObjectKeeper type, and the mapping between them is neither one-to-one nor mechanical:

| In that schema | Here |
|---|---|
| seven photo tables | set-valued `file[]` attributes on their parents, not types |
| six association tables | a derived relationship end, or an association type where the link carries payload |
| `audit_logs` | the event log. Not a type at all |
| `token_blacklist`, `idempotency_keys`, `system_settings` | infrastructure the store either owns or does not hold |
| `notes` | a `part`, or a type of its own, depending on whether a note outlives its subject |

So the type graph is perhaps twenty nodes rather than forty-five, and the order must be recomputed over it once the per-type mapping exists. What follows is not that order. It is strong evidence about its **shape** — where the cycles are, what is most referenced, how much freedom there is — and every one of those findings survives the remapping, because they are properties of the domain rather than of the table layout.

## 2. There is one cycle, and it is one stage

Exactly one strongly connected component with more than one member, and no self-reference anywhere:

```
{ robots, accessories, spare_parts }  ↔  delivery_items
```

Three two-node cycles sharing `delivery_items`. The mechanism is the operations design's own: a **soft peg** lives on the inbound unit, a **hard bind** lives on the delivery slot, and the two point at each other deliberately.

**Its four tables move as one stage.** The first draft of this section argued that because all seven edges are nullable the cycle "breaks", and that its members could therefore sit in different stages. That answered the wrong question (D201). The stage rule rests on ownership: a legacy referrer cannot point at rows a migrated referent now owns, and a nullable column does not change that, since a delivery item that cannot bind to a new unit is broken whether or not its column admits null. Nullability mattered to the import, where a cycle once needed a second pass, and ADR-0077 removed the second pass, so a cycle now costs nothing at import either way. The cycle is one stage, which is where the table in §3 had put it all along; the reasoning now matches the table. Contracting the cycle to one node leaves the residual graph acyclic, which is what makes the order below computable.

## 3. The order, over tables

Seven stages, 42 components. Referrers first, most-referenced last.

| Stage | Tables |
|---|---|
| 1 | the seven photo tables, the six association tables, `audit_logs`, `notes`, `service_part`, `packing_lists`, `api_keys`, `token_blacklist`, `idempotency_keys` — 21 in all, nothing references them |
| 2 | `intake_items`, `non_inventoried_items`, `service_warranty_usage`, `stock_sizes`, `system_settings` |
| 3 | `intake_batches`, `service`, `warranty_contracts` |
| 4 | `label_templates`, `users`, and the cycle of §2 |
| 5 | `accessory_models`, `delivery_configurations`, `procurement_orders`, `robot_models`, `shipping_records`, `spare_part_models` |
| 6 | `deliveries`, `product_configurations` |
| 7 | `customers`, `warranty_products` |

**Only nine of the 42 components are pinned to a stage**; the rest have one to six stages of slack. The seven-stage depth comes from a single longest chain — a service photo, its service, the robot it names, that robot's model, and that model's default warranty product. Any linear extension of the graph is valid, so the plan has room to group stages by what is convenient to cut over together rather than by what the graph forces.

`customers` and `warranty_products` migrate last and are therefore mirrored longest. That is a good outcome rather than a cost: they are also the types that change least, so the mirror is cheapest exactly where it lives longest. For `customers` the marking is followed not by an owned type but by the Xero-synced externally owned type of ADR-0080, since Xero owns customer identity for good; the version advance is the same.

## 4. What would change the answer

Three things, in order of how much:

**A JSON column that carries row ids.** `delivery_configurations.packing_checklist` is documented as keyed by delivery-item id. If that counts as a reference, `delivery_configurations` joins the cycle and it becomes a five-member component. This is the single largest ambiguity and it is settled by sampling the real rows, not by reading the code.

**Whether the polymorphic references count.** `audit_logs.entity_id` reaches 21 tables and `notes.entity_id` reaches 11, neither with a foreign key. Counting them adds one to the in-degree of nearly everything and reorders the most-referenced list. They should count — an unenforced reference constrains an order exactly as an enforced one does — but if the audit log is treated as an archive copied last rather than a live referrer, its edges drop out.

**Two tables that exist in the models and in no migration.** The accessory-model and spare-part-model photo tables are declared, wired into the API, and created by no revision in the chain. A rebuild from migrations would not produce them. Worth understanding before an extract depends on them.

## 4a. The write edges, and the core they collapse

Counting references alone, §3 put `warranty_contracts` in stage 3 and `deliveries` in stage 6. But completing a delivery marks its units sold and creates their warranty contracts (`wr:app/core/state_registry.py:771`, side effects `_update_delivery_items_to_sold` and `_create_delivery_warranties`), and cancelling reverts the units and voids the warranties (`:779`, `:788`). While deliveries are legacy-owned, so must those be.

A write edge from deliveries to warranty contracts, and the reference from `warranty_contracts.delivery_id` back to deliveries (`wr:app/models/warranty.py:112`), put the two in one stage. The same holds for the unit cycle of §2, which the completion writes, and so for `delivery_configurations`, which lies on the reference path between that cycle and `deliveries`. At least seven tables therefore move together: `deliveries`, `delivery_items`, `delivery_configurations`, `robots`, `accessories`, `spare_parts` and `warranty_contracts` — most of the operational core (ADR-0093, D206).

That changes the finding of §3 that "the plan has room": around the core it does not. Staging still separates the periphery — the photo and association tables, users, the catalogues, customers — from the core, and whether that is worth the mirror machinery against a big-bang cutover is the author's choice, which ADR-0075 made under the reference-only rule and ADR-0093 leaves with the author.

The service side has write edges too (`_update_service_parts_to_sold`, `_reserve_service_parts`, `_revert_service_parts_to_available`, lines 818 to 843), which tie spare parts to services; they point the same way as the references already do, so they add no collapse here, and the recomputation over types will confirm it. *(Corrected by ADR-0100: under ADR-0093's two-sided rule a write edge puts writer and written in one stage whichever way it points, so these tie `services` into the core as well.)*

**The registry's side-effect lists are not all the writes** (ADR-0100). The service layer writes units and warranties directly, through `execute_transition` and helpers, outside those lists:
- intake commit makes units `AVAILABLE` (`wr:app/services/intake_batch_service.py:848-851`);
- a shipment's dispatch makes its units `PROCUREMENT` (`wr:app/services/shipment_service.py:281-285`);
- cancelling a procurement order moves its units (`wr:app/services/procurement_order_service.py:488-491`);
- completing a service applies its warranty deduction (`wr:app/services/service_service.py:928`).

Each is a write edge. With them, the core stage takes in intake batches, shipments, procurement orders and services as well, which is most of the operational system. The recomputation over types must extract edges from every write, not from the registry alone, and staging then separates the periphery from one large core.

**Time-driven transitions need a scheduler before their types migrate.** The legacy system has none, and reconciles warranty expiry when warranties are read (`wr:app/models/warranty.py:345-372`). The store never writes on read (ADR-0012), so a contract reaches `EXPIRED` only when something requests the transition, and until then work in progress and time in state are wrong for warranties. A scheduler that polls `available` for the sweepable time-driven transitions is therefore a cutover prerequisite for any type that has them (ADR-0100).

**The legacy history has known holes**, which the port records as gaps rather than filling in:
- transition audit rows were not written from 2026-02-03 to 2026-07-13 (the legacy repository's commit `e675165`);
- unit status changes made as side effects were never audited (`wr:app/core/state_registry.py:85-131`).

Intervals in those spans are missing, a metric that covers them says it is incomplete, and the harness reports the gaps (ADR-0100).

## 5. What to do with this

1. **Map tables to types first.** The order above cannot be used directly, and §1 says why. `publish-and-import.md` §5 gives the rules; most of the 45 go by shape, and what needs deciding is the three judgements it names — for this system, principally whether a note outlives its subject, since notes attach polymorphically to eleven types with no foreign key and nothing in the schema answers it.
2. **Sample the JSON columns** named in §4 against production rows.
3. **Reflect the live schema** and compare, since none of this touched a running database.
4. Then recompute the order over types, counting both references and the legacy system's writes (ADR-0093). Of the three findings that matter, two should carry over — one cycle that moves as one stage, and the most-referenced types being the least-changing — and the third does not: around the operational core there is little ordering freedom (§4a).

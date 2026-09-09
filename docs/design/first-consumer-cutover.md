# The first consumer's cutover

Draft, 2026-09-09. The stage order for `~/RduWs/wr_inventory_management`, extracted from its schema. ADR-0075 decided that cutover is staged by object type and that a type migrates only after every type referencing it has; this is that order for this system, and what it depends on.

**How it was obtained.** The SQLAlchemy metadata was imported and walked programmatically rather than read: 45 tables, 67 declared foreign keys, verified by asserting set equality between the extraction and a hand-typed list. Cross-checked against the Alembic chain, which is linear with a single head. **The live database was not reachable**, so everything here comes from source and a reflection of the running schema is the right check before anyone commits to an order.

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

## 5. What to do with this

1. **Map tables to types first.** The order above cannot be used directly, and §1 says why. `publish-and-import.md` §5 gives the rules; most of the 45 go by shape, and what needs deciding is the three judgements it names — for this system, principally whether a note outlives its subject, since notes attach polymorphically to eleven types with no foreign key and nothing in the schema answers it.
2. **Sample the JSON columns** named in §4 against production rows.
3. **Reflect the live schema** and compare, since none of this touched a running database.
4. Then recompute the order over types, at which point the three findings that matter — one cycle that moves as one stage, the most-referenced types being the least-changing, and a great deal of ordering freedom — should carry over unchanged.

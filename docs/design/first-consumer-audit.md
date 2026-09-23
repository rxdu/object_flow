# Production against the design: the first consumer's business logic, rule by rule

Status: **audit**, 2026-09-24, of `wr` at HEAD `4109939` against the design as of ADR-0101. It answers two questions: does the design cover every business rule the first consumer's production code enforces, and can it take the transitions production has proposed and not yet built? The unit's lifecycle, the slice with the most rules, is written out in full in [`unit-journey.md`](unit-journey.md). Findings that are defects in the design are in [`defects.md`](defects.md) D275 to D281.

## 1. Method

Four read-only audits, one per slice, each reading production's code and its own design documents and citing a line for every rule:

1. units, stock, intake and labels;
2. deliveries, allocation, availability and warranty;
3. procurement, shipments, service and customers;
4. the cross-cutting machinery: the transition engine, permissions, audit, idempotency, Jira and alerts.

Each rule was classed as **covered** (a construct expresses it), **partial** (expressible with a stated caveat or remodelling), a **gap** (the design lacks what it needs), a **known limit** (already recorded as out of scope or refused), or **outside by decision** (ADR-0025's authentication, effects). A slice audit is an assertion, so every gap in §3 was re-read at its cited lines in both repositories before it was recorded here; the partial fits in §4 are as the audits reported them, with the ones this document relies on re-read.

## 2. The result

| Slice | Covered | Partial | Gap | Known limit |
|---|---|---|---|---|
| Units, stock, intake, labels | 9 rule groups, including the whole unit machine | 10 | 4 | 2 |
| Deliveries, allocation, warranty | 26 of 27 rule rows | 1, and 2 remodelled | 4 | 5 groups |
| Procurement, service, customer | every procurement, shipment and service transition | 8 | 2 | 9 |
| Engine, permissions, audit | transitions, cascades, idempotency, per-action permissions, the core audit trail | 8 | 4 | 6, and 4 outside by decision |

Deduplicated across slices, **eleven gaps** remained (§3). Read against the PRD, three needed the language to grow, five were defects, one needed no rule once production's code was read more closely, one is a known limit no requirement asks to close, and one is better remodelled; ADR-0103 disposed of each. Seven are one rule reported by several slices. None of them stops a production transition from being declared: every state machine production registers is expressible, including every transition out of a state production calls terminal. The gaps are in what surrounds a transition: text formats, request shape, reading history, and a few semantics the specification leaves unsaid.

## 3. Gaps, verified

| # | Production rule | What the design lacks | Disposition |
|---|---|---|---|
| 1 | An agent's write must carry the entity's `version`; a person's need not (`wr:app/services/base_service.py:236-251`). Agents' POSTs must carry an `Idempotency-Key` (`wr:app/core/idempotency.py:149-179`) | `expected_version` and `idempotency_key` are optional request fields (`library-api.md` §3), and no guard reads a request field. Reported by all four slices | ADR-0103 §1: `requests by agent require version, key`, stricter than production (§5) |
| 2 | A serial is `{prefix}-{MFR}-{NNNNNN}`, the prefix from the unit type or accessory category and `MFR` from the manufacturer's name, one sequence per type (`wr:app/services/serial_number_service.py:37-41,199-271`) | `format` holds literal text and `{n}` (check 44); the language has no string operation beyond equality and membership (`DESIGN.md` §5.7) | ADR-0103 §4: `format` interpolates a model's code and pads the number |
| 3 | Text is trimmed to absent when blank, and names are bounded in length; Jira keys are canonicalised from a pasted key or URL to one project; `javascript:` and `data:` links are refused (`wr:app/services/inventory_item_service.py:102-114`; `wr:app/services/jira_client.py:38-75`) | the same: no string operation. Only an evaluator could check a format | ADR-0103 §3: `length` bounds text; a pattern and trimming stay at the consumer's edge (`edge-cases.md`) |
| 4 | Reading the audit trail needs `AUDIT_VIEW`, though every person can read the objects; a user may read their own trail (`wr:app/core/permissions.py:33-51`; `wr:app/api/audit.py:287`) | `history` applies the object's visibility and nothing else; no read lists events by actor | no new rule (ADR-0103 §2): the audit read `AUDIT_VIEW` too broadly. Production shows each record's activity to anyone who may view the record (`wr:app/api/deliveries.py:966-1035`) and gates only the cross-record trail, which is the export and `pull`, served from the consumer's own routes |
| 5 | A Jira comment is posted when an order's derived status changes (`wr:app/services/procurement.py:339-375`), and that status is stored so a list can filter on it | a derived value emits no event, and a derivation reading other objects cannot be `indexed` (check 46), so it cannot be a query filter | a known limit by decision (`edge-cases.md`): no requirement asks for the filter, and a subscriber reacts to the change |
| 6 | Pool utilisation (production ADR-0002) | a duration cannot be divided by a duration | D275, resolved by ADR-0103 §5 |
| 7 | A slot with no role never blocks completion (`wr:app/core/state_registry.py:544-545`) | an `unknown` filter inside an aggregate is undefined | D276, resolved by ADR-0103 §6 |
| 8 | Idempotency keys accumulate unless a maintenance script prunes them (`wr:app/core/idempotency.py:55-58`; `wr:scripts/maintenance/prune_idempotency_keys.py`) | no operation removes idempotency records | D277, resolved by ADR-0103 §7 |
| 9 | A retired unit, which has no exit, can still be edited and relabelled (`wr:app/services/inventory_item_service.py:480-536`) | check 15 forces an exit from any non-terminal state | D278, resolved by ADR-0103 §8 |
| 10 | Accessories consumed by a service decrement `Accessory.quantity` (`wr:app/services/service_service.py:751-773`) | the record says the consumer keeps no stock level; check 31 refuses a counter on a serial row | D279, resolved by ADR-0103 §9 |
| 11 | Keyed maps: a warranty's `component_allowance` and `component_usage`, `{category: qty}` (`wr:app/models/warranty.py:52-58,132-136`) | no map attribute. A part per category expresses it, and is a better model, since each allowance then has its own history | remodel, recorded here |

## 4. Partial fits

Each is expressible; the caveat is what a declaration author must know.

| Production rule | How it is declared, and the caveat |
|---|---|
| A guard switched off by an environment flag, default off: the label gate (`wr:app/core/state_registry.py:695-724`; `wr:app/core/config.py:104-105`) | an `observe` marking, enforced later by a publish. The switch becomes governed, not a restart |
| Soft delete cascades along references: deleting a model deletes its units, deleting a customer cancels its deliveries (`wr:app/core/cascade_registry.py:142-167,237-250`) | a delete is refused as `dependent` while live referrers exist, or loops over the referrers calling their own guarded transitions. A behaviour change at cutover, by design (`first-consumer-walkthrough.md` §4G) |
| Side effects that apply only if the target is in a given state: unreserve only if reserved, unpeg only if pegged (`wr:app/services/inventory_helpers.py:395-450`; `wr:app/services/allocation.py:153-167`) | a `for … where` over a collection; a singular reference needs one transition per case, since a `call` that cannot apply refuses |
| A configuration version's activation deactivates every other active version of its SKU (`wr:app/services/product_configuration_service.py:324-369`) | a loop cannot range over a type scan (ADR-0019, ADR-0052). The caller passes the siblings as a set input with a guard that it is complete, or an `Sku` whole owns its versions |
| Notes attach to eleven kinds of record, and can be pinned and invalidated (`wr:app/services/note_service.py:139-227`) | a record part owned by an abstract base the eleven extend, or a label, which cannot be pinned; open in `first-consumer-cutover.md` |
| A Jira key is accepted when Jira is unreachable (`wr:app/services/procurement_order_service.py:315-366`) | a deferred evaluator must answer "satisfied" during the outage, and the record cannot tell that answer from a real check |
| An audit row holds IP, user agent, endpoint, session, request id, duration and the user's role (`wr:app/models/audit.py:21-55`) | the event holds actor, kind, principal and a free-form `context` the consumer fills |
| Requesting the state an entity is already in succeeds and does nothing (`wr:app/core/state_transition.py:358-366`) | refused, since a transition is requested by name from a state; callers change at cutover |
| A committed intake batch overwrites the unit's `created_at` and `created_by` (`wr:app/services/intake_batch_service.py:864-866`) | `.created_at` is the creation event's and never changes; `entered_at(AVAILABLE)` is the commit time |
| An engineer defaults to the caller when the caller is an engineer (`wr:app/services/service_service.py:488-558`) | no expression maps `actor.id` to a user object, so the consumer supplies the input |
| Calendar months and local dates: a warranty ends `start + n months` (`wr:app/services/warranty_service.py:316,455,641`) | a known limit: no month duration and no date type (`edge-cases.md`) |

## 5. What production does that the design would refuse

These are behaviour changes at cutover, each towards the rule production's own documents state:

- Revoking a delivered delivery sets its units to `AVAILABLE` whatever they became since (`wr:app/core/state_registry.py:169-187`); a unit returned and reserved elsewhere is taken back. Here the cascade refuses, naming the unit.
- States production marks terminal are left by explicit override: a cancelled unit, a delivered delivery, a completed service, a committed batch. Here each is `closed` and not terminal (`unit-journey.md` §4).
- Status is written directly by cascade hooks and by `force_transition`, with no audit (`wr:app/core/state_transition.py:577-619`; `wr:app/core/cascade_registry.py:64-95`). Here the one write path records every change, and the one override is a declared assertion with a reason.
- An audit row is committed in its own session before the transition commits, so a rolled-back transition can leave one (`wr:app/core/state_transition.py:554-567`). Here the event is in the transaction.
- A warranty upgrade voids the old contract before validating the new product (`wr:app/services/warranty_service.py:403-442`). Here the guards run first and a refusal changes nothing.
- A transition gated by both a permission and `required_role="ADMIN"` refuses an agent key holding the permission (`wr:app/core/state_registry.py:775-800`). Here both clauses are written and printed, so the double gate is visible.
- An agent must send an expected version and an idempotency key with every request, where production asks for the key only on an agent's POSTs and the version only where a service method checks one (`wr:app/core/idempotency.py:153-161`; `wr:app/services/base_service.py:236-251`). An agent that did not is refused naming `versioned` or `keyed` (ADR-0103 §1).

## 6. Where the design's examples differ from production

Recorded for the table-to-type mapping in `TODO.md`, which re-derives the declarations the harness uses:

1. The unit: the lifecycle is rewritten against production in `unit-journey.md`, which also records the three changes since the walkthrough.
2. `Delivery` in the specification is terminal at `DELIVERED`; production revokes and reopens deliveries, so `DELIVERED` and `CANCELLED` are `closed` and not terminal (`unit-journey.md` §2).
3. The specification's `complete_sale` requires an approval, and the walkthrough's an invoice check; production requires neither, only filled slots and, advisorily, the packing checklist.
4. The specification's `ServiceJob` is terminal at `DONE`, and requires the engineer to act; production cancels and reopens services, and anyone holding `SERVICE_COMPLETE` completes one.
5. A delivery's checklist lives on each configuration in production, not on the delivery.
6. Production issues warranties on internal deliveries, from the delivery's product rather than the slot's, and skips units that already hold a contract.

## 7. Open for more: what production has proposed and not built

Every item is a publish over the current design: new states, transitions, types and metrics, with no change to the engine. Items 5 and 6 also need a consumer that schedules or sends.

| Proposal | Source | What it takes |
|---|---|---|
| 1. Engagements, leases, lease-to-own, retiring an engaged unit | `wr:docs/adr/0002-unit-engagement-and-leasing-model.md` | declared in `unit-journey.md` |
| 2. A structured retirement reason; a `QUARANTINE` state; per-unit intake revert | `wr:TODO.md:243`; `wr:docs/proposals/operations-implementation-plan.md:12,31,55` | an enum and an `accepts`; a state and two transitions; a transition `only via` the batch |
| 3. Pre-delivery inspection per configured robot, a second reviewer, a failed inspection opening rework, a `READY_FOR_DELIVERY` state | `wr:docs/proposals/operations-system-design.md` §8.8 | an observation kind, an approval guard, a `create` step, a state |
| 4. Split delivery; splitting and combining orders and shipments | same, §5.3; `wr:docs/design/procurement-receiving-reconciliation.md:73-77` | a creation and a loop re-parenting slots or units, `only via` |
| 5. Alerts: low stock, procurement overdue, backorder ageing, delivery at risk, lease overdue, warranty expiring; order ready | same, §6 | metric flags and indexed ageing queries (PRD UC-12); order ready is a subscription. Scheduling and sending are the consumer's |
| 6. Demand from reorder points and service parts; Jira sync at the seams; Xero as the owner of customer identity | same, §7, §8.12; `wr:docs/adr/0003-xero-as-source-of-truth-for-customer-identity.md` | a metric the consumer reads to request a creation; external identifiers and an externally owned type (ADR-0080) |
| 7. A movement ledger; a can-revert preflight | same, §9.0; `wr:docs/design/procurement-error-recovery.md:82` | nothing: the events and intervals are the ledger, and `check` is the preflight |

## 8. Discrepancies inside production

Found in passing and outside this design's scope; listed so they are not lost.

- `delete_user` never calls the check that blocks deleting an engineer with active services (`wr:app/services/user_service.py:284-315` against `wr:app/api/users.py:30-35`), so the rule production's docs state is not enforced.
- The completion side effect dispatches `DIRECT_SALE` and sends every other delivery type to `DEVELOPMENT`, so a new delivery type would silently land there (`wr:app/core/state_registry.py:101-114`), a hazard production's ADR-0002 names.
- Production's docs say the packing checklist gates completion and the packing list is generated automatically (`wr:docs/design/business-processes.md:183`); the code does neither.
- The permission on `/users/engineers` is stated three ways across code, comment and docs.

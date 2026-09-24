# An operations review of the unit's journey: where the flow waits, and how to smooth it

Status: **review, proposals awaiting the author**, 2026-09-24. Nothing here is decided: each proposal changes the first consumer's process, and that is the author's call. The author has since placed the operating layer of §2.1 in an upper-layer application built on the store, not in the store (ADR-0104, PRD N6), which answers the first question of §7.

This review reads the worked example — the page built from [`unit-journey.md`](unit-journey.md) and its replay — as a general operations manager would. It asks:
- where work waits, is handed off, is batched, is redone, or depends on someone remembering;
- what a well-run operation would expect that is missing.

Two readers did it:
- this record's author, with the whole design in context;
- a reader with no context, briefed as an experienced operations manager and given only the page's text: everything visible, and everything a click reveals.

Every finding below was checked against the page, the module and production before it was recorded, and "*(both)*", "*(context)*" or "*(cold read)*" says who raised it. The cold read also found a defect in the module, D289, and four contradictions on the page; both are fixed (§6).

Proposals marked **declared** are written into a variant of the module, reproduced whole in the appendix, which `scripts/check-syntax-doc.py` checks on every corpus run. Proposals without the mark are prose.

The page's metric figures are illustrative. The findings rest on the structure of the flow, which the figures only illustrate. Where a finding proposes a metric, the metric is what will measure the effect once the flow runs.

## 1. The verdict

The lifecycle is a sound record and a strict one:
- it separates what a unit is to the business from who has it and whether it works;
- it makes every wait visible;
- it refuses rather than corrupts when something is out of order.

Those are the properties an operation needs before it can be improved.

It is not yet an operation. The cold reader's summary is fair: the page describes a rule engine that records and refuses. It does not describe who works each queue and by when, what the customer was promised, where the robot physically is, or what condition it is in. Some of that is the engine's design: it never acts on its own, and screens, schedules and messages are an upper-layer application's (ADR-0104). But the example is meant to show the whole operation, and the missing operating layer is the largest gap between this design and a smooth flow.

Within the lifecycle, the flow will wait in four predictable places:
- behind queues nobody owns;
- between a unit arriving and being assigned to the order it was bought for;
- behind a shipment that one faulty unit holds up;
- in a strict three-person chain at the end of every sale.

It also lacks controls an operation expects: condition, physical fulfilment, write-off, and a customer date. And two of its measures would mislead a manager.

## 2. Where the flow waits

**2.1 Nobody owns the queues** *(both)*. Only a service job has an owner (`assignee`). A delivery, a shipment and a missing unit have none. A refusal names the records that must change first (CHK-7, CHK-9, SJ-81), not the people who must act, and a lease that runs over is "a flag, not a job".

The engine never initiates (PRD, non-goals). So every rule that should move work forward on a fact needs someone to ask, and no document yet designs who asks. Production's own operations design calls this missing piece the workflow-automation layer. Until it exists, the flow's speed depends on people remembering to look. The facts that need asking about:
- a delivery becoming ready;
- a unit awaiting assignment;
- a unit missing for a week;
- a lease running over;
- a shipment past its eta.

*Proposals:*
- an `assignee` on the delivery, the shipment and the missing unit, with a target time per state. The engine already records assignment from the first write and gives every assignee's open work and waits as standard metrics (M6).
- one exceptions board over the flags that exist: overdue, ageing, missing, stale reservations, leases running over, and refusals nobody followed up. The board should route a refusal that names another object to that object's assignee.
- **an operations application above the store** — decided by the author as an upper-layer application, not part of the core (ADR-0104). It is the loop that asks the store's questions each hour — `available(...)`, the flags, the ageing and assignment queries, from which it builds its own worklists — and acts through `request` or notifies, with no path of its own, and an alert for when the loop itself stops. The store's part is to answer each of its questions with one query (PRD N6, UC-20).

*Risk:* noise. Start with a daily review of the board and tune the thresholds from it.

**2.2 A unit bought for an order forgets the order when it arrives** *(both)*. Committing a shipment releases each unit's earmark (`inventorize` clears `peg`). Production chose this so that scarce stock is assigned by a person, most urgent order first (`wr:docs/design/explicit-delivery-assignment.md`). The cost is that the unit sits in general stock, nothing says which order was waiting for it, and any rep can take it first. The readers disagree on the remedy, and the author must choose:
- **keep production's rule, keep the fact** *(context; declared)*. `inventorize` writes `procured_for := peg` before clearing the earmark, and `reserve` clears it. The unit derives `awaiting_assignment`, so the people who assign stock have a worklist, read in order of the delivery's promised date as production's contention rule orders it. `assignment_wait` measures how long units wait. `procured_for` is tracked, so every diversion to another order is in its intervals.
- **reverse production's rule** *(cold read)*. The earmark becomes a reservation at commit when its order is still open and in date. This is faster, and it restores the auto-fill production retired because it filled the less urgent of two orders first. In the engine it is a declared cascade of `Shipment.commit`, which the author's principle on cascades allows (PRD F8, ADR-0108).

Either way, when a unit goes missing, the operations application should notify the order's owner and queue the order for the next unit, instead of the earmark being silently released; the store's part is the `flag_missing` event it pulls and the query that finds the order *(cold read)*. Two more holes *(cold read)*:
- sales cannot hold a unit for a quote without opening a delivery, which invites placeholder deliveries. A time-limited quote hold would close it;
- a reservation never expires, so reservations past an age should be flagged.

**2.3 Receiving is batched, recorded late and confirmed by software** *(both)*. `Shipment.commit` puts every received unit on offer or none, so one missing label holds eleven sellable units (hard case "One unlabelled unit in a commit of twelve"). In the replay the receipt is recorded two days late, by an agent, with no person named. After a long weekend the two-day backdating bound forces a false arrival date, and recording it as "now" charges our own backlog to the supplier's lead time.
- *Proposals:*
  - `commit_ready` puts each ready unit on offer and leaves the rest in `INTAKE` *(context; declared)*;
  - `receive_unit_late`, backdatable within 14 days, for a supervisor, with a reason *(cold read; declared)*;
  - `recording_lag`, the median of recorded time less occurred time, flagged above a day *(context; declared)*;
  - receipt scanned at the dock, the label printed, applied and scan-checked as one step, and a named person accountable for each receipt *(cold read)*.
- *Risk:* releasing units one by one loses the whole-shipment check. It moves to the batch's closing step, and the batch revert keeps production's test: every unit the batch put on offer is still untouched (`wr:app/services/intake_batch_service.py:1072-1085`).

**2.4 Completing a sale is a chain of three people** *(both)*. In the replay an engineer ticks, then the ops lead approves, then sales ops completes (steps 9 to 13). Any tick voids the approval, so approval is forced last, and the ops agent learns the order by being refused (step 10). This is the specification's example delivery, not production's, which has no approval. But it is the shape the example recommends.
- *Proposals:*
  - an explicit `READY` state, entered by `mark_ready` once every unit is bound and nothing is still on order *(context; declared)*. Time in `PREPARATION` and time in `READY` then separate waiting for stock and work from waiting for sign-off, which is UC-1's question;
  - an approval voided only by a material change, and not by a tick *(both)*. A material change is the total, beyond a tolerance, or the checklist's composition, which a `checklist_revision` attribute counts;
  - approval required only above a value or risk threshold, or approve-and-complete as one action for the ops lead, with a sample of small deals audited *(cold read)*;
  - a standard checklist per model, not per delivery, and checklist results kept on the delivery rather than deleted at completion, as the example's cascade does today *(cold read)*;
  - explicit decisions on whether one person may both approve and complete, and on whether an invoice must exist before shipping, which usually differs between prepaid and credit customers *(cold read)*.

**2.5 Lending a stock unit takes a paper delivery** *(context; declared)*. A unit enters the pool only through `Delivery.complete_internal`: a delivery opened, bound and completed for a unit that never moves. Production's own leasing decision names the same friction. The proposal is `transfer_to_pool`, an admin action recorded like any other, with the internal delivery kept for units that really travel.

## 3. What a well-run operation would expect, and the flow lacks

**3.1 Nothing checks a unit's condition when it comes back** *(both)*. `accept_return` puts a returned unit straight on offer, with no inspection and no reason. `release_to_stock` puts a used pool unit on sale beside new ones, and a lease closes with no check-in. The replay's own robot is repaired, returned, fails again at a show and is sold on, and nothing flags it. A unit returned with a fault, or worn by months on loan, reaches its next customer as if it were new.
- *Proposals:*
  - `RETURNED`, an inbound quarantine, entered by returns and pool releases *(both; declared)*;
  - `restock` out of `RETURNED` on a passing `ReturnCheck` recorded after the return *(both; declared)*;
  - a lease closes only when each unit has a check recorded since the return began *(both; declared)*;
  - a repeat-failure flag that holds a unit twice repaired within 90 days out of lending until reviewed *(cold read; declared)*;
  - a condition grade (new, demo, refurbished) that availability and price read, and pre-delivery inspection as recorded results against a template per model, the observation kind the specification already uses for service inspections *(cold read)*.
- *Risk:* the quarantine is a queue. It needs a target time and its own ageing flag. Production withdrew a `RETURNED` state once, for two reasons (its ADR-0002): it duplicated a deferred quarantine, and lease returns re-enter the pool directly. This proposal is that quarantine; a lease return still re-enters the pool directly, with its check recorded as a datapoint.

**3.2 Nothing tracks the robot physically leaving** *(cold read)*. A delivery goes from `PREPARATION` to `DELIVERED`, and the unit becomes `SOLD` when sales ops completes it: a commercial close. There is no pick, pack, ship date, carrier, tracking, proof of delivery or acceptance, and an internal transfer is also a "delivery". Production has none of these stages either (its `DeliveryStatus` is `PREPARATION`, `DELIVERED`, `CANCELLED`). A refused delivery, transit damage or a wrong address has no path but an admin revoke.
- *Proposal:* `READY` → `SHIPPED` (carrier, tracking, driven by the carrier's data) → `DELIVERED` (proof of delivery). Finance decides which of them makes a unit `SOLD`, since that sets revenue timing. Rename the internal delivery "transfer to pool".

**3.3 Write-off and end of life are thin** *(both)*.
- `retire` leaves only the pool, and the only other exit from stock is a soft delete, with no reason asked.
- A unit damaged on the shelf or lost has no honest path out.
- Missing and rejected units need claims against a carrier or supplier before a deadline.
- *Proposals:*
  - `retire` from every state in which we hold the unit, with its reason *(both; declared for stock, returns and the pool)*;
  - no `delete` for a unit that physically existed; a value, and an approval above a threshold; a claim record with a deadline, linked to the missing or rejected unit; disposal recorded, batteries in particular *(cold read)*.

**3.4 No customer date** *(context)*. Production's delivery has a required `delivery_date` (`wr:app/models/delivery.py:31`). The module leaves it out, so nothing can say whether a delivery is late: not UC-8's on-time rate, not UC-12's "delivery date at risk". This is an omission of the module, not of production. The proposal is `promised_on` on the delivery, an `at_risk` flag over it, and the promised date as the order of the assignment worklist.

**3.5 Service has no clock, no waiting and no custody** *(both)*. A repair has no priority, no response or completion target, no warranty decision at opening, and no waiting-for-parts or waiting-for-customer state. So repair time mixes work with waiting. A customer's robot in our workshop is recorded nowhere, because custody tracks only our robots going out. A swapped unit's faulty twin is not tracked home. And voiding a finished job returns a consumed part to stock even though it is installed in a robot.
- *Proposals:*
  - on-hold states with a reason, a priority and a target clock, with `WAITING_PARTS` introduced the UC-13 way: labelled, measured, then published *(both)*;
  - a custody record for a customer's robot in our care *(cold read)*;
  - a swap that tracks the faulty unit's return *(cold read)*;
  - a void that asks whether the part came back, moving its use to the remaining job by default *(cold read)*.
  - A late result on a finished job is already allowed, since `DONE` is closed but not terminal.

**3.6 Loans and leases** *(cold read)*.
- A loan has no kit list: the chargers, batteries and controllers that go out, and their count and condition at return.
- An overdue lease has no escalation ladder with an owner: reminder, call, charge, recovery.
- A buyout, `Lease.convert`, checks only permission and that the lease is out. It has none of a sale's price, approval or invoice controls.
- A robot failing at an event raises the real question: how to get a replacement there. `swap_unit` covers the record, not the logistics.

**3.7 Returns and traceability** *(cold read)*.
- A return has no authorisation number, no reason code, and no link to a credit.
- Nothing records a unit's hardware revision or firmware version, so a recall or a firmware campaign cannot find every affected unit — in stock, in the pool or with customers.
- Who owned a unit is now in `sold_to`'s intervals (D289).

**3.8 Stock accuracy** *(cold read)*.
- A unit found on the shelf is corrected straight to `AVAILABLE` by override ("Fixing reality"), skipping every intake check. The override should reach `INTAKE`, and that should be the default for found units *(declared)*.
- There is no location, no cycle count and no accuracy measure. Without a location, a count is a search.

**3.9 Purchasing is dark until dispatch** *(cold read)*. `REQUESTED` covers "we want one", "order sent", "supplier confirmed" and "supplier backordered". An expected date exists only once the supplier ships.
- *Proposal:* the procurement order, which production has and the module leaves out, gains `ORDERED` and `CONFIRMED`, with a confirmed date carried by every inbound unit from the day it is requested. The supplier's shipping notice arrives from a feed rather than by retyping. That also gives production's "procurement overdue" its date.

## 4. What would mislead a manager

**4.1 The company's pool counts as unfinished work** *(context; declared)*. `DEVELOPMENT` is `live`, so a robot delivered into the pool is open work until it is sold or retired, possibly for years. Work in progress, the oldest open work and cycle time — M1's standard metrics — would be dominated by the fleet. The proposal is `DEVELOPMENT category closed`: delivery into the pool completes fulfilment, and the pool is measured by its engagements and service jobs.

**4.2 Monthly fleet utilisation cannot be produced** *(both)*. It is the headline rental KPI, and ADR-0103 §5 records why it is missing: a metric buckets a span by the month it began. This is the one finding that asks the engine itself to grow. The proposal is an intervals source split at bucket boundaries — each span cut into its pieces per month, with the piece's duration. It would make every share of time per period declarable. It needs an ADR, and the PRD's C3 is its baseline.

**4.3 One working state per delivery** *(context)*. With `PREPARATION` the only working state, UC-1's "where do deliveries get stuck" has one answer. `READY` and the fulfilment stages of §3.2 split it.

**4.4 The page's charts track the system, not the business** *(cold read)*. Refusals per rule tell a manager where people trip over rules, not whether customers get robots on time. The minimum set to steer by, each with an owner and a target:
- order-to-delivery time, and on-time against the promised date;
- supplier on-time-in-full and lead time;
- dock-to-stock time and inventory accuracy;
- the age of stock, of reservations and of backorders;
- fleet utilisation and revenue per unit, by month;
- repair time with waiting shown separately, first-time fix and repeat repairs within 90 days;
- dead-on-arrival and early-failure rates, and the return rate;
- loans and leases overdue;
- write-offs by reason and value;
- overrides by reason.

The declarable ones need the dates and fields of §3; the monthly utilisation needs §4.2.

## 5. What the page asks of its reader

A floor team would not know the words, and the page shows production and proposal side by side without saying which is which:
- **state names that mean something else:**
  - `DEVELOPMENT` is the loaner and demo pool;
  - `PROCUREMENT` is in transit;
  - `SOLD` also covers a part used up in a repair;
  - `DELIVERED` covers a transfer and an unshipped sale;
  - `CANCELLED` covers four outcomes;
- **engine terms:** peg, bind, `only via`, cascade, remedy classes, read sets, and references to decision and defect numbers;
- **cases about the software rather than the floor:** only 7 of the 38 hard cases are about supply, loans and service, and most of the rest are about the software. The floor's own exceptions are absent: dead on arrival, wrong model shipped, damaged in transit, a refused delivery, theft at an event, a recall, parts on backorder, a lease never returned;
- **proposal presented alongside production:** loans, leases and lease-to-own are production's proposal, not its practice, and the page does not say so where it shows them.

*Proposal:* a manager's view, beside the engineer's:
- a one-page state map in business words, giving for each stage what it means, who owns it, its target time, the next actions and the ways out;
- the everyday transitions, with the corrections out of sight;
- a daily exceptions board;
- a clear mark on what production does today and what is proposed.

A renderer can print the stage map from the categories and a display name per state. That belongs to the page, not to the lifecycle, whose state names the port keeps.

## 6. Fixed during the review

- **D289, a defect of the module:** a unit bought out of a lease could not be serviced for its buyer. The fix is `sold_to`, recorded on every route into `SOLD` and compared by the service job's `theirs` guard. It is in `unit-journey.md` and the appendix.
- **Four contradictions on the page:**
  - a case relied on an `invoiced` guard that the page's `complete_sale` does not declare;
  - a case called `DONE` terminal;
  - a case said `accept_return` needs `RETIRE`, where it needs `ADMIN`;
  - the header counted the unbuilt lease-to-own among the "transitions production performs today".
- **The legend's wording for `unreachable_from_here`**, which dropped "another transition comes first" from DESIGN's definition, so its example fix — print the label first — looked contradictory.

## 7. For the author

In the order the review would take them:

1. **The operating layer (§2.1).** *Answered 2026-09-24: an upper-layer application, not part of the core (ADR-0104).* What remains for the core is whether the delivery, the shipment and the missing unit get owners and target times, which are declarations.
2. **Condition (§3.1).** Should everything that comes back pass a quarantine and an inspection before it goes on offer, with a repeat-failure hold on lending?
3. **Assignment (§2.2).** Should production's explicit assignment be kept, with a worklist and `procured_for`, or should the earmark become a reservation at commit? *Both can be declared (2026-09-24, PRD F8, ADR-0108): a reservation at commit is a declared cascade of `Shipment.commit` following the earmark a person set, and prints on the unit's rules. What cannot be declared is choosing the most urgent order when the unit arrives, which stays with the operations application. So this is a business choice: speed, against production's reason for retiring the fill.*
4. **The end of a sale (§2.4, §3.2).** Should there be `READY` and physical fulfilment stages, and where should `SOLD` happen?
5. **Measurement (§4.1, §4.2).** Should the pool count as finished fulfilment work? And should the metric form learn to split a span across periods, which needs an ADR, so that monthly utilisation can be declared?
6. **The rest as they stand.** Partial commit, late receipt with approval, the pool transfer, write-off from stock, overrides into intake, the promised date, and the service clock.
7. **A manager's view of the page (§5)**, beside the engineer's.

## 8. The proposals, as declarations

The appendix is the module of `unit-journey.md` with the proposals marked **declared** applied, and the checker reports it clean. The changes:

- `RETURNED`, `inbound`. `accept_return` and `release_to_stock` enter it, and `restock` leaves it on a passing `ReturnCheck`. `Engagement.close` requires a return check per unit.
- `retire` from `AVAILABLE`, `RETURNED` and `DEVELOPMENT`. `transfer_to_pool`. `DEVELOPMENT` is `closed`. `correct_state` may target `INTAKE`.
- `procured_for`, written by `inventorize` and cleared by `reserve`, and `awaiting_assignment`. `intake_ready`, `Shipment.commit_ready` and `Shipment.receive_unit_late`.
- `repeat_failure`, which `fit` reads.
- The delivery's `READY`, `mark_ready` and `back_to_preparation`.
- The observation kind `ReturnCheck`, and the metrics `recording_lag` and `assignment_wait`.
- `sold_to`, which is in `unit-journey.md` itself (D289).

The promised date, the service clock, the checklist revision, the fulfilment stages and the procurement order are prose only. The module's delivery carries no dates and no checklist, and the procurement order is outside it.

## Appendix: the module with the declared proposals applied

A variant of [`unit-journey.md`](unit-journey.md) §2, to be deleted from here once the author has decided and the module is amended.

```text
module inventory_journey
use   inventory.{User, UserRole, Customer, RetirementReason, OverrideReason}

capability INVENTORY_DELETE, PROCUREMENT_CANCEL, ADMIN,
           DELIVERY_CANCEL, DELIVERY_DELETE,
           SERVICE_CREATE, SERVICE_COMPLETE, SERVICE_CANCEL_REQUESTED,
           SERVICE_CANCEL_COMPLETED, SERVICE_DELETE,
           LEASE_EDIT, LEASE_CONVERT

enum CancellationReason version 2 { ORDER_CANCELLED, DISCARDED, REJECTED_QA,
                                    RETURNED_SUPPLIER, OTHER }
enum EngagementKind version 1 { LEASE, EVENT, DEV_HOLD }
enum ServiceKind    version 1 { REPAIR, MAINTENANCE, UPGRADE }
enum CheckOutcome   version 1 { PASS, FAIL, NOT_APPLICABLE }
sequence unit_serial version 1

requests by agent require version, key
```

**The model catalogue.** The unit's rules read its model — whether a manufacturer serial or a label photo is required, and the maker's code a serial carries — so the model is declared here with the unit. Production's `robot_models` holds a name, a manufacturer, a warranty period and the two flags (`wr:app/models/robot_models.py:25-47`). The reorder point, and the two derivations that read it, are not production's: they are added to test PRD L4 and M7, a threshold that is governed data and a business exception over one object, and production defers reorder logic (`wr:app/models/procurement.py:15`).

```text
type RobotModel version 1 {
  tracking record
  states   ACTIVE category live, DISCONTINUED category closed terminal
  summary  name, manufacturer, state

  attr name                         string indexed unique
  attr manufacturer                 string?
  attr maker_code                   string?
  attr warranty_months              int
  attr label_photo_required         bool default false
  attr manufacturer_serial_required bool default false
  attr reorder_point                int default 0

  derive available_units = count(u in Robot where u.model == this and u.state == Robot.AVAILABLE)
  derive low_stock       = available_units < reorder_point

  invariant reorder_point_nonneg: reorder_point >= 0

  create add -> ACTIVE accepts name, manufacturer, maker_code, warranty_months,
                               label_photo_required, manufacturer_serial_required {
    require may: actor.has(INVENTORY_CREATE) because delegable
  }
  act edit at ACTIVE accepts manufacturer, maker_code, warranty_months,
                             label_photo_required, manufacturer_serial_required {
    require may: actor.has(INVENTORY_CREATE) because delegable
  }
  act set_reorder_point at ACTIVE accepts reorder_point {
    require may: actor.has(PO_EDIT) because delegable
  }
  do discontinue ACTIVE -> DISCONTINUED {
    require may: actor.has(INVENTORY_DELETE) because delegable
  }
}
```

```text
machine UnitLifecycle version 4 {
  requires attr label_printed_at timestamp?
  requires attr manufacturer_serial string?
  requires attr photos file[]
  requires attr cancellation_reason CancellationReason?
  requires attr retirement_reason RetirementReason?
  requires ref  model : RobotModel
  requires ref  shipment : Shipment?
  requires ref  peg : Delivery?
  requires ref  procured_for : Delivery?
  requires ref  binding : Delivery?
  requires ref  used_in : ServiceJob?
  requires ref  sold_to : Customer?
  requires ref  engagement_lines : EngagementLine[]
  requires invariant one_open_engagement
  requires capability EDIT, ADMIN, DELETE, ASSERT, CANCEL

  state REQUESTED   category inbound
  state PROCUREMENT category inbound
  state MISSING     category inbound
  state RETURNED    category inbound
  state INTAKE      category inbound
  state AVAILABLE   category live
  state RESERVED    category live
  state SOLD        category closed
  state DEVELOPMENT category closed
  state RETIRED     category closed
  state CANCELLED   category closed
  state DELETED     category closed terminal

  create request -> REQUESTED accepts model {
    require may: actor.has(EDIT) because delegable
  }
  create add_to_intake -> INTAKE accepts model, manufacturer_serial {
    require may: actor.has(EDIT) because delegable
  }
  create add_opening_stock -> AVAILABLE accepts model, manufacturer_serial, label_printed_at {
    require may:        actor.has(ASSERT) because delegable
    require labelled:   inputs.label_printed_at is not null because self_serviceable
    require mfr_serial: inputs.model.manufacturer_serial_required
                        implies inputs.manufacturer_serial is not null because self_serviceable
  }

  do ship REQUESTED -> PROCUREMENT only via Shipment.dispatch, Shipment.add_unit {
    input via_shipment : Shipment
    set shipment := inputs.via_shipment
  }
  do unship PROCUREMENT -> REQUESTED only via Shipment.remove_unit {
    clear shipment
  }
  do receive PROCUREMENT -> INTAKE only via Shipment.receive_unit, Shipment.receive_unit_late { }
  do unreceive INTAKE -> PROCUREMENT only via Shipment.unreceive_unit { }
  do flag_missing PROCUREMENT -> MISSING only via Shipment.flag_missing {
    clear peg
  }
  do restore MISSING -> PROCUREMENT only via Shipment.restore_missing { }
  do rerequest MISSING -> REQUESTED {
    require may: actor.has(EDIT) because delegable
    clear shipment
  }
  do discard MISSING -> CANCELLED {
    require may: actor.has(EDIT) because delegable
    set cancellation_reason := CancellationReason.DISCARDED
  }
  do cancel { REQUESTED, PROCUREMENT, INTAKE } -> CANCELLED {
    input reason : CancellationReason
    require may: actor.has(CANCEL) because delegable
    set cancellation_reason := inputs.reason
    clear peg
  }

  act peg_to at { REQUESTED, PROCUREMENT, MISSING, INTAKE } only via Delivery.peg_slot {
    input slot : Delivery
    set peg := inputs.slot
  }
  act unpeg at { REQUESTED, PROCUREMENT, MISSING, INTAKE } only via Delivery.cancel {
    clear peg
  }
  act record_label_print at any {
    require may: actor.has(EDIT) because delegable
    set label_printed_at := now
  }

  do inventorize INTAKE -> AVAILABLE {
    require may:        actor.has(EDIT)                            because delegable
    require labelled:   label_printed_at is not null               because unreachable_from_here
    require mfr_serial: model.manufacturer_serial_required
                        implies manufacturer_serial is not null    because unreachable_from_here
    require photo:      model.label_photo_required
                        implies count(p in photos) >= 1            because unreachable_from_here
    set procured_for := peg
    clear peg
  }
  do revert_intake AVAILABLE -> INTAKE only via Shipment.revert_commit { }

  do reserve AVAILABLE -> RESERVED only via Delivery.bind_slot {
    input slot : Delivery
    require open: inputs.slot.state == Delivery.PREPARATION because dependent
    set binding := inputs.slot
    clear procured_for
  }
  do reserve_for_service AVAILABLE -> RESERVED only via ServiceJob.add_part {
    input job : ServiceJob
    set used_in := inputs.job
  }
  do release RESERVED -> AVAILABLE only via Delivery.unbind_slot, Delivery.cancel,
                                           ServiceJob.remove_part, ServiceJob.cancel {
    clear binding
    clear used_in
  }

  do sell RESERVED -> SOLD only via Delivery.complete_sale {
    input buyer : Customer
    set sold_to := inputs.buyer
  }
  do deliver_internal RESERVED -> DEVELOPMENT only via Delivery.complete_internal { }
  do consume RESERVED -> SOLD only via ServiceJob.finish {
    input buyer : Customer
    set sold_to := inputs.buyer
  }
  do unconsume SOLD -> AVAILABLE only via ServiceJob.void {
    clear used_in
    clear sold_to
  }
  do unsell SOLD -> AVAILABLE only via Delivery.revoke {
    clear binding
    clear sold_to
  }
  do recall_internal DEVELOPMENT -> AVAILABLE only via Delivery.revoke {
    clear binding
  }
  do accept_return SOLD -> RETURNED {
    require may: actor.has(EDIT) because delegable
    clear binding
    clear sold_to
  }
  do restock RETURNED -> AVAILABLE {
    require may:       actor.has(EDIT) because delegable
    require inspected: any(r in return_checks where r.outcome == CheckOutcome.PASS
                           and r.occurred_at >= entered_at(RETURNED)) because unreachable_from_here
  }

  do release_to_stock DEVELOPMENT -> RETURNED {
    require may:  actor.has(ADMIN) because delegable
    require home: none(l in engagement_lines where l.open) because dependent
    clear binding
  }
  do convert_lease DEVELOPMENT -> SOLD only via Lease.convert {
    input buyer : Customer
    set sold_to := inputs.buyer
  }
  do transfer_to_pool AVAILABLE -> DEVELOPMENT {
    require may: actor.has(ADMIN) because delegable
  }
  do retire { AVAILABLE, RETURNED, DEVELOPMENT } -> RETIRED {
    input reason : RetirementReason
    require may:   actor.has(DELETE) because delegable
    require admin: actor.has(ADMIN)  because delegable
    set retirement_reason := inputs.reason
    for l in engagement_lines where l.open limit 1 { call l.end() }
  }
  do delete { AVAILABLE, CANCELLED } -> DELETED {
    require may: actor.has(DELETE) because delegable
  }

  assert correct_state -> { INTAKE, AVAILABLE, DEVELOPMENT, RETIRED } {
    input to : state
    input reason : OverrideReason
    input detail : string? personal
    input admits : invariant[]?
    require may: actor.has(ASSERT) because delegable
    may admit one_open_engagement
  }
}

type Robot version 5 {
  tracking serial
  machine  UnitLifecycle
  provides capability EDIT = INVENTORY_CREATE, ADMIN = ADMIN, DELETE = INVENTORY_DELETE,
                      ASSERT = INVENTORY_ASSERT, CANCEL = PROCUREMENT_CANCEL
  summary  serial, model, state

  attr serial string identifier from unit_serial
                     format "RBT-[{model.maker_code}-]{n:6}" indexed unique
  attr manufacturer_serial string?
  attr label_printed_at    timestamp?
  attr photos              file[]
  attr cancellation_reason CancellationReason?
  attr retirement_reason   RetirementReason?

  ref  model            : RobotModel
  ref  shipment         : Shipment? inverse units
  ref  peg              : Delivery? inverse pegged
  ref  procured_for     : Delivery? inverse procured
  ref  binding          : Delivery? inverse units
  ref  used_in          : ServiceJob? inverse parts_used
  ref  sold_to          : Customer?
  ref  services         : ServiceJob[] inverse robot
  ref  engagement_lines : EngagementLine[] inverse robot

  derive repeat_failure = count(s in services where s.kind == ServiceKind.REPAIR
                                and s.created_at >= now - 90 days) >= 2
  derive fit        = state == DEVELOPMENT
                  and none(s in services where s.open and s.blocking)
                  and not repeat_failure
  derive leasable   = fit and none(l in engagement_lines where l.open)
  derive on_loan    = any(l in engagement_lines where l.open)
  derive in_service = any(s in services where s.open)
  derive awaiting_assignment = state == AVAILABLE and procured_for is not null
  derive intake_ready = label_printed_at is not null
                    and (not model.manufacturer_serial_required or manufacturer_serial is not null)
                    and (not model.label_photo_required or count(p in photos) >= 1)

  invariant one_open_engagement: count(l in engagement_lines where l.open) <= 1
  invariant one_claim:           binding is null or used_in is null
}
```

```text
type Shipment version 2 {
  tracking record
  states   IN_TRANSIT category inbound, ARRIVED category live,
           COMMITTED category closed, VOIDED category closed terminal
  summary  tracking_no, carrier, state

  attr tracking_no string?
  attr carrier     string?
  attr eta         timestamp? indexed
  ref  units : Robot[] inverse shipment

  derive overdue = state == IN_TRANSIT and eta < now

  create dispatch -> IN_TRANSIT accepts tracking_no, carrier, eta {
    input with_units : Robot[]
    require may:     actor.has(INVENTORY_CREATE) because delegable
    require ordered: all(u in inputs.with_units: u.state == Robot.REQUESTED) because self_serviceable
    for u in inputs.with_units limit 200 { call u.ship(via_shipment := this) }
  }
  act add_unit at IN_TRANSIT {
    input robot : Robot
    require may: actor.has(INVENTORY_CREATE) because delegable
    call inputs.robot.ship(via_shipment := this)
  }
  act remove_unit at IN_TRANSIT {
    input robot : Robot
    require may:  actor.has(INVENTORY_CREATE) because delegable
    require ours: inputs.robot.shipment == this because self_serviceable
    require last: count(u in units where u.state == Robot.PROCUREMENT) >= 2 because self_serviceable
    call inputs.robot.unship()
  }
  do void IN_TRANSIT -> VOIDED {
    require may:   actor.has(INVENTORY_CREATE) because delegable
    require empty: none(u in units where u.state == Robot.PROCUREMENT) because self_serviceable
  }
  do arrive IN_TRANSIT -> ARRIVED backdatable within 2 days {
    require may: actor.has(INVENTORY_CREATE) because delegable
  }
  act receive_unit at ARRIVED backdatable within 2 days {
    input robot : Robot
    require may:  actor.has(INVENTORY_CREATE) because delegable
    require ours: inputs.robot.shipment == this because self_serviceable
    call inputs.robot.receive()
  }
  act receive_unit_late at ARRIVED backdatable within 14 days {
    input robot  : Robot
    input reason : string
    require may:  actor.has(INVENTORY_ASSERT) because delegable
    require ours: inputs.robot.shipment == this because self_serviceable
    call inputs.robot.receive()
  }
  act unreceive_unit at ARRIVED {
    input robot : Robot
    require may:  actor.has(INVENTORY_CREATE) because delegable
    require ours: inputs.robot.shipment == this because self_serviceable
    call inputs.robot.unreceive()
  }
  act flag_missing at ARRIVED {
    input robot : Robot
    require may:  actor.has(INVENTORY_CREATE) because delegable
    require ours: inputs.robot.shipment == this because self_serviceable
    call inputs.robot.flag_missing()
  }
  act restore_missing at ARRIVED {
    input robot : Robot
    require may:  actor.has(INVENTORY_CREATE) because delegable
    require ours: inputs.robot.shipment == this because self_serviceable
    call inputs.robot.restore()
  }
  act commit_ready at ARRIVED {
    require may: actor.has(INVENTORY_CREATE) because delegable
    for u in units where u.state == Robot.INTAKE and u.intake_ready limit 200 {
      call u.inventorize()
    }
  }
  do commit ARRIVED -> COMMITTED {
    require may:        actor.has(INVENTORY_CREATE) because delegable
    require reconciled: none(u in units where u.state == Robot.PROCUREMENT) because self_serviceable
    for u in units where u.state == Robot.INTAKE limit 200 { call u.inventorize() }
  }
  do revert_commit COMMITTED -> ARRIVED {
    input reason : string
    require may:      actor.has(PROCUREMENT_CANCEL) because delegable
    require pristine: all(u in units where u.state != Robot.MISSING
                                        and u.state != Robot.CANCELLED:
                          u.state == Robot.AVAILABLE)             because dependent
    for u in units where u.state == Robot.AVAILABLE limit 200 { call u.revert_intake() }
  }
}

type Delivery version 4 {
  tracking record
  states   PREPARATION category live, READY category live, DELIVERED category closed,
           CANCELLED category closed, DELETED category closed terminal
  summary  customer, state

  ref  customer : Customer
  ref  pegged   : Robot[] inverse peg
  ref  procured : Robot[] inverse procured_for
  ref  units    : Robot[] inverse binding
  attr internal bool default false

  create open -> PREPARATION accepts customer, internal {
    require may: actor.has(DELIVERY_EDIT) because delegable
  }
  act peg_slot at PREPARATION {
    input robot : Robot
    require may: actor.has(DELIVERY_EDIT) because delegable
    call inputs.robot.peg_to(slot := this)
  }
  act bind_slot at PREPARATION {
    input robot : Robot
    require may: actor.has(DELIVERY_EDIT) because delegable
    call inputs.robot.reserve(slot := this)
  }
  act unbind_slot at PREPARATION {
    input robot : Robot
    require may:  actor.has(DELIVERY_EDIT) because delegable
    require ours: inputs.robot.binding == this because self_serviceable
    call inputs.robot.release()
  }
  do mark_ready PREPARATION -> READY {
    require filled: count(u in units) >= 1 and none(u in pegged) because dependent
    require may:    actor.has(DELIVERY_EDIT)                      because delegable
  }
  do back_to_preparation READY -> PREPARATION {
    require may: actor.has(DELIVERY_EDIT) because delegable
  }
  do complete_sale READY -> DELIVERED {
    require not_internal: not internal                                  because unreachable_from_here
    require may:          actor.has(DELIVERY_COMPLETE)                  because delegable
    for u in units limit 500 { call u.sell(buyer := customer) }
  }
  do complete_internal READY -> DELIVERED {
    require is_internal: internal                                       because unreachable_from_here
    require may:         actor.has(DELIVERY_COMPLETE)                   because delegable
    for u in units limit 500 { call u.deliver_internal() }
  }
  do cancel { PREPARATION, READY } -> CANCELLED {
    require may:   actor.has(DELIVERY_CANCEL) because delegable
    require admin: actor.has(ADMIN)           because delegable
    for u in units limit 500 { call u.release() }
    for u in pegged limit 500 { call u.unpeg() }
  }
  do revoke DELIVERED -> CANCELLED {
    require may:   actor.has(DELIVERY_CANCEL) because delegable
    require admin: actor.has(ADMIN)           because delegable
    for u in units where u.state == Robot.SOLD limit 500 { call u.unsell() }
    for u in units where u.state == Robot.DEVELOPMENT limit 500 { call u.recall_internal() }
  }
  do delete CANCELLED -> DELETED {
    require may: actor.has(DELIVERY_DELETE) because delegable
  }
}
```

```text
type ServiceJob version 4 {
  tracking record
  states   OPEN category inbound, WORKING category live,
           DONE category closed, CANCELLED category closed,
           DELETED category closed terminal
  summary  kind, robot, state

  attr kind ServiceKind
  ref  robot      : Robot inverse services
  ref  customer   : Customer
  ref  engineer   : User assignee
  ref  parts_used : Robot[] inverse used_in

  derive blocking = kind == ServiceKind.REPAIR

  create open -> OPEN accepts kind, robot, customer, engineer {
    require may:        actor.has(SERVICE_CREATE) because delegable
    require assignable: inputs.engineer.state == User.ACTIVE
                        and inputs.engineer.role in {UserRole.ADMIN, UserRole.ENGINEERING}
                                                                         because self_serviceable
    require delivered: inputs.robot.state == Robot.SOLD
                       or inputs.robot.state == Robot.DEVELOPMENT        because self_serviceable
    require theirs:    inputs.robot.state == Robot.DEVELOPMENT
                       or inputs.robot.sold_to == inputs.customer        because self_serviceable
  }
  act reassign at OPEN accepts engineer {
    require may:        actor.has(SERVICE_EDIT) because delegable
    require assignable: inputs.engineer.state == User.ACTIVE
                        and inputs.engineer.role in {UserRole.ADMIN, UserRole.ENGINEERING}
                                                                         because self_serviceable
  }
  act add_part at { OPEN, WORKING } {
    input part : Robot
    require may: actor.has(SERVICE_EDIT) because delegable
    call inputs.part.reserve_for_service(job := this)
  }
  act remove_part at { OPEN, WORKING } {
    input part : Robot
    require may:  actor.has(SERVICE_EDIT) because delegable
    require ours: inputs.part.used_in == this because self_serviceable
    call inputs.part.release()
  }
  do start OPEN -> WORKING backdatable within 2 days {
    require mine: engineer.login == actor.id because delegable
  }
  do finish WORKING -> DONE {
    require may: actor.has(SERVICE_COMPLETE) because delegable
    for p in parts_used limit 50 { call p.consume(buyer := customer) }
  }
  do cancel { OPEN, WORKING } -> CANCELLED {
    require may: actor.has(SERVICE_CANCEL_REQUESTED) because delegable
    for p in parts_used limit 50 { call p.release() }
  }
  do void DONE -> CANCELLED {
    require may:   actor.has(SERVICE_CANCEL_COMPLETED) because delegable
    require admin: actor.has(ADMIN)                    because delegable
    for p in parts_used limit 50 { call p.unconsume() }
  }
  do reopen CANCELLED -> OPEN {
    require admin: actor.has(ADMIN) because delegable
  }
  do delete CANCELLED -> DELETED {
    require may: actor.has(SERVICE_DELETE) because delegable
  }
}

type Engagement version 2 {
  tracking record
  states   SCHEDULED category inbound, OUT category live, RETURNING category live,
           CLOSED category closed terminal
  summary  kind, expected_return, state

  attr kind            EngagementKind
  attr expected_return timestamp? indexed
  part lines : EngagementLine[] inverse engagement
       cascade on { close, cancel, settle } to EngagementLine.end limit 20
  ref  lease : Lease? inverse engagement

  derive overdue = state == OUT and expected_return < now

  create schedule -> SCHEDULED accepts kind, expected_return {
    require may: actor.has(LEASE_EDIT) because delegable
  }
  act add_unit at SCHEDULED {
    input robot : Robot
    require may:      actor.has(LEASE_EDIT) because delegable
    require leasable: inputs.robot.leasable because dependent
    create EngagementLine.open(for_engagement := this, robot := inputs.robot)
  }
  do dispatch SCHEDULED -> OUT backdatable within 1 days {
    require may:          actor.has(LEASE_EDIT) because delegable
    require nonempty:     count(l in lines) >= 1 because self_serviceable
    require fit:          all(l in lines: l.robot.fit) because dependent
    require return_known: kind != EngagementKind.LEASE or expected_return is not null
                                                     because self_serviceable
  }
  act swap_unit at OUT {
    input out_robot : Robot
    input in_robot  : Robot
    require may:      actor.has(LEASE_EDIT) because delegable
    require leasable: inputs.in_robot.leasable because dependent
    for l in lines where l.robot == inputs.out_robot and l.open limit 1 { call l.end() }
    create EngagementLine.open(for_engagement := this, robot := inputs.in_robot)
  }
  do start_return OUT -> RETURNING backdatable within 1 days {
    require may: actor.has(LEASE_EDIT) because delegable
  }
  do close RETURNING -> CLOSED {
    require may:     actor.has(LEASE_EDIT) because delegable
    require checked: all(l in lines where l.open:
                         any(r in l.robot.return_checks
                             where r.occurred_at >= entered_at(RETURNING))) because dependent
  }
  do cancel SCHEDULED -> CLOSED {
    require may: actor.has(LEASE_EDIT) because delegable
  }
  do settle OUT -> CLOSED only via Lease.convert { }
}

type EngagementLine version 1 {
  tracking record
  states   OPEN category live, ENDED category closed terminal

  owner engagement : Engagement inverse lines
  ref   robot : Robot inverse engagement_lines

  create open -> OPEN only via Engagement.add_unit, Engagement.swap_unit accepts robot {
    input for_engagement : Engagement
    set engagement := inputs.for_engagement
  }
  do end OPEN -> ENDED only via Engagement.close, Engagement.cancel, Engagement.settle,
                                Engagement.swap_unit, Robot.retire { }
}

type Lease version 1 {
  tracking record
  states   ACTIVE category live, CONVERTED category closed terminal,
           ENDED category closed terminal
  summary  customer, state

  ref customer   : Customer
  ref engagement : Engagement inverse lease stored

  create sign -> ACTIVE accepts customer, engagement {
    require may:      actor.has(LEASE_EDIT) because delegable
    require is_lease: inputs.engagement.kind == EngagementKind.LEASE because self_serviceable
  }
  do convert ACTIVE -> CONVERTED {
    require may: actor.has(LEASE_CONVERT) because delegable
    require out: engagement.state == Engagement.OUT because dependent
    for l in engagement.lines where l.open limit 20 { call l.robot.convert_lease(buyer := customer) }
    call engagement.settle()
  }
  do end ACTIVE -> ENDED {
    require may:      actor.has(LEASE_EDIT) because delegable
    require returned: engagement.state == Engagement.CLOSED because dependent
  }
}
```

```text
observation ReturnCheck version 1 on Robot as return_checks {
  field outcome : CheckOutcome
  field note    : string?
  recorded by   actor.has(INVENTORY_CREATE)
  occurred within 2 days
}

metric recording_lag version 1 {
  from      t in Robot.transitions where not t.imported and not t.migrated
  by        transition = t.transition, actor_kind = t.actor_kind, week = week(t.recorded_at)
  window on t.recorded_at
  value     median(t.recorded_at - t.occurred_at)
  flag      late when value > 1 days
}

metric assignment_wait version 1 {
  from      i in Robot.intervals(procured_for) where i.value is not null
  by        model = i.object.model, month = month(i.entered_at)
  window on i.entered_at
  value     median(i.duration)
  flag      slow when value > 3 days
}

metric inbound_dwell version 1 {
  from      i in Robot.intervals where i.state == Robot.PROCUREMENT
                                    or i.state == Robot.MISSING or i.state == Robot.INTAKE
  by        stage = i.state, model = i.object.model, month = month(i.entered_at)
  window on i.entered_at
  value     median(i.duration)
  flag      slow when value > 14 days
}

metric missing_units version 1 {
  from      u in Robot where u.state == Robot.MISSING
  by        shipment = u.shipment
  value     max(u.time_in(MISSING))
  flag      unresolved when value > 7 days
}

metric repairs version 1 {
  from      s in ServiceJob where s.kind == ServiceKind.REPAIR
  by        model = s.robot.model, month = month(s.created_at)
  window on s.created_at
  value     count()
}

metric retirements version 1 {
  from      u in Robot where u.state == Robot.RETIRED
  by        model = u.model, reason = u.retirement_reason
  window on u.entered_at(RETIRED)
  value     count()
}

metric time_on_loan version 1 {
  from      i in EngagementLine.intervals where i.state == EngagementLine.OPEN
  by        model = i.object.robot.model, kind = i.object.engagement.kind,
            month = month(i.entered_at)
  window on i.entered_at
  value     sum(i.duration)
}

metric time_in_pool version 1 {
  from      i in Robot.intervals where i.state == Robot.DEVELOPMENT
  by        model = i.object.model, month = month(i.entered_at)
  window on i.entered_at
  value     sum(i.duration)
}

metric pool_utilisation version 1 {
  combine   on_loan = time_on_loan, pool = time_in_pool
  by        model
  value     on_loan / pool
  flag      idle when value < 0.300
}

metric engagements_overdue version 1 {
  from      e in Engagement where e.overdue
  by        kind = e.kind
  value     count()
  flag      any_overdue when value > 0
}
```

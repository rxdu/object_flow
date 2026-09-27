# An operations review of the unit's journey: where the flow waits, and how to smooth it

Status: **review, proposals awaiting the author**, 2026-09-24. Nothing here is decided: each proposal changes the first consumer's process, and that is the author's call. The author has since placed the operating layer of §2.1 in an upper-layer application built on the store, not in the store (ADR-0104, PRD N6), which answers the first question of §7.

This review reads the worked example — the page built from [`unit-journey.md`](unit-journey.md) and its replay — as a general operations manager would. It asks:
- where work waits, is handed off, is batched, is redone, or depends on someone remembering;
- what a well-run operation would expect that is missing.

Two readers did it:
- this record's author, with the whole design in context;
- a reader with no context, briefed as an experienced operations manager and given only the page's text: everything visible, and everything a click reveals.

Every finding below was checked against the page, the module and production before it was recorded, and "*(both)*", "*(context)*" or "*(cold read)*" says who raised it. The cold read also found a defect in the module, D289, and four contradictions on the page; both are fixed (§6).

Proposals marked **declared** are written into a variant of the module, reproduced whole in the appendix, which `scripts/check-flow-docs.py` checks on every corpus run. Proposals without the mark are prose.

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
- A buyout, `Lease.convert`, checks only that the lease is out; who may request it is the upper layer's (ADR-0114). It has none of a sale's price, approval or invoice controls.
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

*Checked 2026-09-25 against the first consumer's own PRD, as a case study, which settles most of these for the platform's own declarations, several differently from the options below: `first-consumer-prd-check.md` §4.*

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

- `RETURNED`, `inbound`. `accept_return` and `release_to_stock` enter it, and `restock` leaves it on a passing `ReturnCheck`. `restock` is `Robot`'s own transition and not the machine's, since a machine requires attributes, parts and invariants of its binders and never an observation kind, and its guard reads the unit's return checks (D389). `Engagement.close` requires a return check per unit.
- `retire` from `AVAILABLE`, `RETURNED` and `DEVELOPMENT`. `transfer_to_pool`. `DEVELOPMENT` is `closed`. `correct_state` may target `INTAKE`.
- `procured_for`, written by `inventorize` and cleared by `reserve`, and `awaiting_assignment`. `intake_ready`, `Shipment.commit_ready` and `Shipment.receive_unit_late`.
- `repeat_failure`, which `fit` reads.
- The delivery's `READY`, `mark_ready` and `back_to_preparation`.
- The observation kind `ReturnCheck`, and the metrics `recording_lag` and `assignment_wait`.
- `sold_to`, which is in `unit-journey.md` itself (D289).
- the invariants `labelled` and `mfr_serial`, and the internal transitions `record_manufacturer_serial` and `add_photo`, which are in `unit-journey.md` itself (ADR-0109, D379).

The promised date, the service clock, the checklist revision, the fulfilment stages and the procurement order are prose only. The module's delivery carries no dates and no checklist, and the procurement order is outside it.

## Appendix: the module with the declared proposals applied

A variant of [`unit-journey.md`](unit-journey.md) §2, to be deleted from here once the author has decided and the module is amended. It is written in the flow description format, in the journey's order, and `scripts/check-flow-docs.py` checks it with all four steps of the flow checker (ADR-0121). It carries the journey's corrections of 2026-09-27 (D408, `unit-journey.md` §2).

```yaml
module: inventory_journey
imports: { operations_shared: [User, Customer, RetirementReason, OverrideReason] }
categories: [inbound, live, closed]

enumerations:
  CancellationReason: [ORDER_CANCELLED, DISCARDED, REJECTED_QA, RETURNED_SUPPLIER, OTHER]
  EngagementKind: [LEASE, EVENT, DEV_HOLD]
  ServiceKind: [REPAIR, MAINTENANCE, UPGRADE]
  CheckOutcome: [PASS, FAIL, NOT_APPLICABLE]

sequences:
  unit_serial:
    description: Numbers the robots, in one series for the type, as production's serial service does.
```

```yaml
# inventory_journey, continued
machines:
  UnitLifecycle:
    description: What a unit is to the business, from its request through intake and sale or the company's own pool, to its retirement or deletion.
    requires:
      attributes:
        label_printed_at:    { type: timestamp, optional: true }
        manufacturer_serial: { type: string, optional: true }
        photos:              { type: "file[]" }
        cancellation_reason: { type: CancellationReason, optional: true }
        retirement_reason:   { type: RetirementReason, optional: true }
        model:               { reference: RobotModel }
        shipment:            { reference: Shipment, optional: true }
        peg:                 { reference: Delivery, optional: true }
        procured_for:        { reference: Delivery, optional: true }
        binding:             { reference: Delivery, optional: true }
        used_in:             { reference: ServiceJob, optional: true }
        sold_to:             { reference: Customer, optional: true }
        engagement_lines:    { reference: "EngagementLine[]" }
      invariants: [one_open_engagement, labelled, mfr_serial]

    states:
      REQUESTED:   { category: inbound }
      PROCUREMENT: { category: inbound }
      MISSING:     { category: inbound }
      RETURNED:    { category: inbound }
      INTAKE:      { category: inbound }
      AVAILABLE:   { category: live }
      RESERVED:    { category: live }
      SOLD:        { category: closed }
      DEVELOPMENT: { category: closed }
      RETIRED:     { category: closed }
      CANCELLED:   { category: closed }
      DELETED:     { category: closed, final: true }

    conditions:
      labelled:
        description: The unit's label has been printed.
        expression: label_printed_at is not null
        remedy: unreachable_from_here
      mfr_serial:
        description: The unit has the manufacturer serial its model requires.
        expression: model.manufacturer_serial_required implies manufacturer_serial is not null
        remedy: dependent
      photo:
        description: The unit has the label photo its model requires.
        expression: model.label_photo_required implies count(p in photos) >= 1
        remedy: dependent
      open:
        description: The delivery the unit is reserved for is still being prepared.
        expression: inputs.slot.state == Delivery.PREPARATION
        remedy: dependent
      home:
        description: The unit is on no open engagement.
        expression: none(l in engagement_lines where l.open)
        remedy: dependent

    transitions:
      request:
        kind: initial
        to: REQUESTED
        required_inputs: [model]
      add_to_intake:
        kind: initial
        to: INTAKE
        required_inputs: [model]
        optional_inputs: [manufacturer_serial]
      add_opening_stock:
        kind: initial
        to: AVAILABLE
        required_inputs: [model]
        optional_inputs: [manufacturer_serial, label_printed_at]

      ship:
        kind: external
        from: REQUESTED
        to: PROCUREMENT
        only_via: [Shipment.dispatch, Shipment.add_unit]
        inputs:
          via_shipment: { reference: Shipment }
        effect:
          - assign: { location: shipment, expr: inputs.via_shipment }
      unship:
        kind: external
        from: PROCUREMENT
        to: REQUESTED
        only_via: [Shipment.remove_unit]
        effect:
          - clear: [shipment]
      receive:
        kind: external
        from: PROCUREMENT
        to: INTAKE
        only_via: [Shipment.receive_unit, Shipment.receive_unit_late]
      unreceive:
        kind: external
        from: INTAKE
        to: PROCUREMENT
        only_via: [Shipment.unreceive_unit]
      flag_missing:
        kind: external
        from: PROCUREMENT
        to: MISSING
        only_via: [Shipment.flag_missing]
        effect:
          - clear: [peg]
      restore:
        kind: external
        from: MISSING
        to: PROCUREMENT
        only_via: [Shipment.restore_missing]
      rerequest:
        kind: external
        from: MISSING
        to: REQUESTED
        effect:
          - clear: [shipment]
      discard:
        kind: external
        from: MISSING
        to: CANCELLED
        effect:
          - assign: { location: cancellation_reason, expr: CancellationReason.DISCARDED }
          - clear: [peg]
      cancel:
        kind: external
        from: [REQUESTED, PROCUREMENT, INTAKE]
        to: CANCELLED
        inputs:
          reason: { type: CancellationReason }
        effect:
          - assign: { location: cancellation_reason, expr: inputs.reason }
          - clear: [peg]

      peg_to:
        kind: internal
        from: [REQUESTED, PROCUREMENT, MISSING, INTAKE]
        only_via: [Delivery.peg_slot]
        inputs:
          slot: { reference: Delivery }
        effect:
          - assign: { location: peg, expr: inputs.slot }
      unpeg:
        kind: internal
        from: any
        description: Releases the unit's peg, in whatever state it is, so that its delivery can be cancelled.
        only_via: [Delivery.cancel]
        effect:
          - clear: [peg]
      record_label_print:
        kind: internal
        from: any
        effect:
          - assign: { location: label_printed_at, expr: now }
      record_manufacturer_serial:
        kind: internal
        from: any
        optional_inputs: [manufacturer_serial]
      add_photo:
        kind: internal
        from: INTAKE
        inputs:
          photo: { type: file }
        effect:
          - add: { location: photos, expr: inputs.photo }

      inventorize:
        kind: external
        from: INTAKE
        to: AVAILABLE
        guards:
          labelled: deny
          mfr_serial: deny
          photo: deny
        effect:
          - assign: { location: procured_for, expr: peg }
          - clear: [peg]
      revert_intake:
        kind: external
        from: AVAILABLE
        to: INTAKE
        only_via: [Shipment.revert_commit]

      reserve:
        kind: external
        from: AVAILABLE
        to: RESERVED
        only_via: [Delivery.bind_slot]
        inputs:
          slot: { reference: Delivery }
        guards:
          open: deny
        effect:
          - assign: { location: binding, expr: inputs.slot }
          - clear: [procured_for]
      reserve_for_service:
        kind: external
        from: AVAILABLE
        to: RESERVED
        only_via: [ServiceJob.add_part]
        inputs:
          job: { reference: ServiceJob }
        effect:
          - assign: { location: used_in, expr: inputs.job }
      release:
        kind: external
        from: RESERVED
        to: AVAILABLE
        only_via: [Delivery.unbind_slot, Delivery.cancel, ServiceJob.remove_part, ServiceJob.cancel]
        effect:
          - clear: [binding, used_in]

      sell:
        kind: external
        from: RESERVED
        to: SOLD
        only_via: [Delivery.complete_sale]
        inputs:
          buyer: { reference: Customer }
        effect:
          - assign: { location: sold_to, expr: inputs.buyer }
      deliver_internal:
        kind: external
        from: RESERVED
        to: DEVELOPMENT
        only_via: [Delivery.complete_internal]
      consume:
        kind: external
        from: RESERVED
        to: SOLD
        only_via: [ServiceJob.finish]
        inputs:
          buyer: { reference: Customer }
        effect:
          - assign: { location: sold_to, expr: inputs.buyer }
      unconsume:
        kind: external
        from: SOLD
        to: AVAILABLE
        only_via: [ServiceJob.void]
        effect:
          - clear: [used_in, sold_to]
      unsell:
        kind: external
        from: SOLD
        to: AVAILABLE
        only_via: [Delivery.revoke]
        effect:
          - clear: [binding, sold_to]
      recall_internal:
        kind: external
        from: DEVELOPMENT
        to: AVAILABLE
        only_via: [Delivery.revoke]
        guards:
          home: deny
        effect:
          - clear: [binding]
      accept_return:
        kind: external
        from: SOLD
        to: RETURNED
        effect:
          - clear: [binding, used_in, sold_to]

      release_to_stock:
        kind: external
        from: DEVELOPMENT
        to: RETURNED
        guards:
          home: deny
        effect:
          - clear: [binding]
      convert_lease:
        kind: external
        from: DEVELOPMENT
        to: SOLD
        only_via: [Lease.convert]
        inputs:
          buyer: { reference: Customer }
        effect:
          - assign: { location: sold_to, expr: inputs.buyer }
          - clear: [binding]
      transfer_to_pool:
        kind: external
        from: AVAILABLE
        to: DEVELOPMENT
      retire:
        kind: external
        from: [AVAILABLE, RETURNED, DEVELOPMENT]
        to: RETIRED
        inputs:
          reason: { type: RetirementReason }
        effect:
          - assign: { location: retirement_reason, expr: inputs.reason }
          - foreach:
              item: l
              array: engagement_lines
              where: l.open
              limit: 1
              steps:
                - call: { target: l, transition: end }
      delete:
        kind: external
        from: [AVAILABLE, CANCELLED]
        to: DELETED
        effect:
          - clear: [peg]

      correct_state:
        kind: assertion
        to: [INTAKE, AVAILABLE, DEVELOPMENT, RETIRED]
        inputs:
          reason: { type: OverrideReason }
          detail: { type: string, optional: true, personal: true }
        description: Puts the unit into the state it is really in when the record is wrong, releasing it from any delivery or job that pegged, bound or claimed it, so neither finds it afterwards.
        effect:
          - clear: [peg, binding, used_in]

types:
  Robot:
    description: A robot, one physical unit, tracked by its serial from its request to its end.
    tracking: serial
    state_machine: UnitLifecycle

    attributes:
      serial:
        type: string
        identifier: { sequence: unit_serial, format: "RBT-[{model.maker_code}-]{n:6}" }
        indexed: true
        unique: true
      manufacturer_serial: { type: string, optional: true, indexed: true }
      label_printed_at:    { type: timestamp, optional: true }
      photos:              { type: "file[]" }
      cancellation_reason: { type: CancellationReason, optional: true }
      retirement_reason:   { type: RetirementReason, optional: true }
      model:               { reference: RobotModel, opposite: units }
      shipment:            { reference: Shipment, optional: true, opposite: units }
      peg:                 { reference: Delivery, optional: true, opposite: pegged }
      procured_for:        { reference: Delivery, optional: true, opposite: procured }
      binding:             { reference: Delivery, optional: true, opposite: units }
      used_in:             { reference: ServiceJob, optional: true, opposite: parts_used }
      sold_to:             { reference: Customer, optional: true }
      services:            { reference: "ServiceJob[]", opposite: robot }
      engagement_lines:    { reference: "EngagementLine[]", opposite: robot }

    observations:
      return_checks:
        kind: ReturnCheck
        description: A check of a unit that came back, recorded before it goes on offer again.
        attributes:
          outcome: { type: CheckOutcome }
          note:    { type: string, optional: true }
        max_recording_delay: 2 days

    derived_attributes:
      repeat_failure:
        description: The unit has needed two or more repairs in ninety days.
        expression: >-
          count(s in services where s.kind == ServiceKind.REPAIR
                and s.created_at >= now - 90 days) >= 2
      fit:
        description: The unit is in the pool, no open service job blocks it, and it is not failing repeatedly.
        expression: >-
          state == DEVELOPMENT and none(s in services where s.open and s.blocking)
          and not repeat_failure
      leasable:
        description: The unit is fit and on no open engagement, which is ADR-0002's "leasable now".
        expression: fit and none(l in engagement_lines where l.open)
      on_loan:
        description: An open engagement line names the unit.
        expression: any(l in engagement_lines where l.open)
      in_service:
        description: An open service job names the unit.
        expression: any(s in services where s.open)
      awaiting_assignment:
        description: The unit is on offer and still carries the delivery it was procured for.
        expression: state == AVAILABLE and procured_for is not null
      intake_ready:
        description: The unit carries everything intake checks, so it can be inventorised.
        expression: >-
          label_printed_at is not null
          and (not model.manufacturer_serial_required or manufacturer_serial is not null)
          and (not model.label_photo_required or count(p in photos) >= 1)

    summary: [serial, model, state]

    invariants:
      one_open_engagement:
        description: The unit is on at most one open engagement.
        expression: count(l in engagement_lines where l.open) <= 1
      one_claim:
        description: The unit is bound to a delivery or reserved for a service job, never both.
        expression: binding is null or used_in is null
      labelled:
        description: A unit on offer has a recorded label print.
        expression: state != AVAILABLE or label_printed_at is not null
      mfr_serial:
        description: A unit on offer has the manufacturer serial its model requires.
        expression: >-
          state != AVAILABLE or not model.manufacturer_serial_required
          or manufacturer_serial is not null

    conditions:
      inspected:
        description: A passing return check was recorded since the unit came back.
        expression: >-
          any(r in return_checks where r.outcome == CheckOutcome.PASS
              and r.occurred_at >= entered_at(RETURNED))
        remedy: unreachable_from_here

    transitions:
      restock:
        kind: external
        from: RETURNED
        to: AVAILABLE
        description: Puts a returned unit back on offer once it has passed a return check; the unit's own, since a machine cannot require an observation kind (D389).
        guards:
          inspected: deny

    metrics:
      inbound_dwell:
        description: The median time units spend in procurement, missing or in intake, by stage, model and month.
        source: intervals
        item: i
        filter: i.state == Robot.PROCUREMENT or i.state == Robot.MISSING or i.state == Robot.INTAKE
        dimensions:
          stage: i.state
          model: i.object.model
          month: month(i.entered_at)
        time_dimension: i.entered_at
        expression: median(i.duration)
        flag_when: { slow: "value > 14 days" }
      missing_units:
        description: The longest total time a unit of each shipment, now missing, has spent missing.
        source: objects
        item: u
        filter: u.state == Robot.MISSING
        dimensions:
          shipment: u.shipment
        expression: max(u.time_in(MISSING))
        flag_when: { unresolved: "value > 7 days" }
      retirements:
        description: The units retired, by model and reason.
        source: objects
        item: u
        filter: u.state == Robot.RETIRED
        dimensions:
          model: u.model
          reason: u.retirement_reason
        time_dimension: u.entered_at(RETIRED)
        expression: count()
      time_in_pool:
        description: The time units have spent in the company-owned pool, by model and month.
        source: intervals
        item: i
        filter: i.state == Robot.DEVELOPMENT
        dimensions:
          model: i.object.model
          month: month(i.entered_at)
        time_dimension: i.entered_at
        expression: sum(i.duration)
      recording_lag:
        description: The median time between a change happening and its being recorded, by transition, kind of actor and week.
        source: transitions
        item: t
        filter: not t.imported and not t.migrated
        dimensions:
          transition: t.transition
          actor_kind: t.actor_kind
          week: week(t.recorded_at)
        time_dimension: t.recorded_at
        expression: median(t.recorded_at - t.occurred_at)
        flag_when: { late: "value > 1 days" }
      assignment_wait:
        description: The median time a unit on offer waits for the delivery it was procured for to take it, by model and month.
        source: intervals(procured_for)
        item: i
        filter: i.value is not null
        dimensions:
          model: i.object.model
          month: month(i.entered_at)
        time_dimension: i.entered_at
        expression: median(i.duration)
        flag_when: { slow: "value > 3 days" }
```

**The model catalogue.** The unit's rules read its model — whether a manufacturer serial or a label photo is required, and the maker's code a serial carries — so the model is declared here with the unit. Production's `robot_models` holds a name, a manufacturer, a warranty period and the two flags (`wr:app/models/robot_models.py:25-47`). The reorder point, and the two derivations that read it, are not production's: they are added to test PRD L4 and M7, a threshold that is governed data and a business exception over one object, and production defers reorder logic (`wr:app/models/procurement.py:15`).

```yaml
# inventory_journey, continued
  RobotModel:
    description: "A model of robot: its serial flag holds every unit on offer, and its photo flag a unit inventorized from intake."
    tracking: record

    attributes:
      name:                         { type: string, indexed: true, unique: true }
      manufacturer:                 { type: string, optional: true }
      maker_code:                   { type: string, optional: true }
      warranty_months:              { type: int }
      label_photo_required:         { type: bool, default: false }
      manufacturer_serial_required: { type: bool, default: false, indexed: true }
      reorder_point:                { type: int, default: 0 }
      units:                        { reference: "Robot[]", opposite: model }

    states:
      ACTIVE:       { category: live }
      DISCONTINUED: { category: closed, final: true }

    derived_attributes:
      available_units:
        description: The units of this model on offer.
        expression: count(u in Robot where u.model == this and u.state == Robot.AVAILABLE)
      low_stock:
        description: Fewer units of this model are on offer than its reorder point.
        expression: available_units < reorder_point

    summary: [name, manufacturer, state]

    invariants:
      reorder_point_nonneg:
        description: The reorder point is not negative.
        expression: reorder_point >= 0

    transitions:
      add:
        kind: initial
        to: ACTIVE
        required_inputs: [name, warranty_months]
        optional_inputs: [manufacturer, maker_code, label_photo_required, manufacturer_serial_required]
      edit:
        kind: internal
        from: ACTIVE
        required_inputs: [warranty_months]
        optional_inputs: [manufacturer, maker_code, label_photo_required, manufacturer_serial_required]
      set_reorder_point:
        kind: internal
        from: ACTIVE
        required_inputs: [reorder_point]
      discontinue:
        kind: external
        from: ACTIVE
        to: DISCONTINUED
```

```yaml
# inventory_journey, continued
  Shipment:
    description: A receiving batch of units, reconciled unit by unit when it arrives.
    tracking: record

    attributes:
      tracking_no: { type: string, optional: true }
      carrier:     { type: string, optional: true }
      eta:         { type: timestamp, optional: true, indexed: true }
      units:       { reference: "Robot[]", opposite: shipment }

    states:
      IN_TRANSIT: { category: inbound }
      ARRIVED:    { category: live }
      COMMITTED:  { category: closed }
      VOIDED:     { category: closed, final: true }

    derived_attributes:
      overdue:
        description: The shipment is still in transit after its expected arrival.
        expression: state == IN_TRANSIT and eta < now

    summary: [tracking_no, carrier, state]

    conditions:
      ordered:
        description: Every unit dispatched is one that was requested.
        expression: "all(u in inputs.with_units: u.state == Robot.REQUESTED)"
        remedy: self_serviceable
      ours:
        description: The unit is on this shipment.
        expression: inputs.robot.shipment == this
        remedy: self_serviceable
      last:
        description: At least two of the shipment's units are still in procurement, so removing a unit cannot leave it with none.
        expression: count(u in units where u.state == Robot.PROCUREMENT) >= 2
        remedy: unreachable_from_here
      empty:
        description: No unit of the shipment is still in procurement.
        expression: none(u in units where u.state == Robot.PROCUREMENT)
        remedy: dependent
      reconciled:
        description: No unit of the shipment is still in procurement.
        expression: none(u in units where u.state == Robot.PROCUREMENT)
        remedy: dependent
      pristine:
        description: Every unit of the shipment that is not missing, cancelled or deleted is still on offer, so the commit can be undone.
        expression: >-
          all(u in units where u.state != Robot.MISSING
                           and u.state != Robot.CANCELLED
                           and u.state != Robot.DELETED:
              u.state == Robot.AVAILABLE)
        remedy: dependent

    transitions:
      dispatch:
        kind: initial
        to: IN_TRANSIT
        optional_inputs: [tracking_no, carrier, eta]
        inputs:
          with_units: { reference: "Robot[]" }
        guards:
          ordered: deny
        effect:
          - foreach:
              item: u
              array: inputs.with_units
              limit: 200
              steps:
                - call: { target: u, transition: ship, inputs: { via_shipment: this } }
      update_tracking:
        kind: internal
        from: IN_TRANSIT
        description: Records the shipment's tracking number, carrier or expected arrival as they become known.
        optional_inputs: [tracking_no, carrier, eta]
      add_unit:
        kind: internal
        from: IN_TRANSIT
        inputs:
          robot: { reference: Robot }
        effect:
          - call: { target: inputs.robot, transition: ship, inputs: { via_shipment: this } }
      remove_unit:
        kind: internal
        from: IN_TRANSIT
        inputs:
          robot: { reference: Robot }
        guards:
          ours: deny
          last: deny
        effect:
          - call: { target: inputs.robot, transition: unship }
      void:
        kind: external
        from: IN_TRANSIT
        to: VOIDED
        guards:
          empty: deny
      arrive:
        kind: external
        from: IN_TRANSIT
        to: ARRIVED
        backdating_limit: 2 days
      receive_unit:
        kind: internal
        from: ARRIVED
        backdating_limit: 2 days
        inputs:
          robot: { reference: Robot }
        guards:
          ours: deny
        effect:
          - call: { target: inputs.robot, transition: receive }
      receive_unit_late:
        kind: internal
        from: ARRIVED
        backdating_limit: 14 days
        inputs:
          robot:  { reference: Robot }
          reason: { type: string }
        guards:
          ours: deny
        effect:
          - call: { target: inputs.robot, transition: receive }
      unreceive_unit:
        kind: internal
        from: ARRIVED
        inputs:
          robot: { reference: Robot }
        guards:
          ours: deny
        effect:
          - call: { target: inputs.robot, transition: unreceive }
      flag_missing:
        kind: internal
        from: ARRIVED
        inputs:
          robot: { reference: Robot }
        guards:
          ours: deny
        effect:
          - call: { target: inputs.robot, transition: flag_missing }
      restore_missing:
        kind: internal
        from: ARRIVED
        inputs:
          robot: { reference: Robot }
        guards:
          ours: deny
        effect:
          - call: { target: inputs.robot, transition: restore }
      commit:
        kind: external
        from: ARRIVED
        to: COMMITTED
        guards:
          reconciled: deny
        effect:
          - foreach:
              item: u
              array: units
              where: u.state == Robot.INTAKE
              limit: 200
              steps:
                - call: { target: u, transition: inventorize }
      commit_ready:
        kind: internal
        from: ARRIVED
        effect:
          - foreach:
              item: u
              array: units
              where: u.state == Robot.INTAKE and u.intake_ready
              limit: 200
              steps:
                - call: { target: u, transition: inventorize }
      revert_commit:
        kind: external
        from: COMMITTED
        to: ARRIVED
        inputs:
          reason: { type: string }
        guards:
          pristine: deny
        effect:
          - foreach:
              item: u
              array: units
              where: u.state == Robot.AVAILABLE
              limit: 200
              steps:
                - call: { target: u, transition: revert_intake }

  Delivery:
    description: A delivery being prepared for a customer, or for the company's own pool, which binds units and pegs inbound ones.
    tracking: record

    attributes:
      customer: { reference: Customer }
      pegged:   { reference: "Robot[]", opposite: peg }
      units:    { reference: "Robot[]", opposite: binding }
      procured: { reference: "Robot[]", opposite: procured_for }
      internal: { type: bool, default: false }

    states:
      PREPARATION: { category: live }
      READY:       { category: live }
      DELIVERED:   { category: closed }
      CANCELLED:   { category: closed }
      DELETED:     { category: closed, final: true }

    summary: [customer, state]

    conditions:
      ours:
        description: The unit is bound to this delivery.
        expression: inputs.robot.binding == this
        remedy: self_serviceable
      not_internal:
        description: The delivery is to a customer, not to the company's own pool.
        expression: not internal
        remedy: unreachable_from_here
      is_internal:
        description: The delivery is to the company's own pool.
        expression: internal
        remedy: unreachable_from_here
      filled:
        description: At least one unit is bound, and no unit is still only pegged.
        expression: count(u in units) >= 1 and none(u in pegged)
        remedy: dependent

    transitions:
      open:
        kind: initial
        to: PREPARATION
        required_inputs: [customer]
        optional_inputs: [internal]
      peg_slot:
        kind: internal
        from: PREPARATION
        inputs:
          robot: { reference: Robot }
        effect:
          - call: { target: inputs.robot, transition: peg_to, inputs: { slot: this } }
      bind_slot:
        kind: internal
        from: PREPARATION
        inputs:
          robot: { reference: Robot }
        effect:
          - call: { target: inputs.robot, transition: reserve, inputs: { slot: this } }
      unbind_slot:
        kind: internal
        from: PREPARATION
        inputs:
          robot: { reference: Robot }
        guards:
          ours: deny
        effect:
          - call: { target: inputs.robot, transition: release }
      mark_ready:
        kind: external
        from: PREPARATION
        to: READY
        guards:
          filled: deny
      back_to_preparation:
        kind: external
        from: READY
        to: PREPARATION
      complete_sale:
        kind: external
        from: READY
        to: DELIVERED
        guards:
          not_internal: deny
        effect:
          - foreach:
              item: u
              array: units
              limit: 500
              steps:
                - call: { target: u, transition: sell, inputs: { buyer: customer } }
      complete_internal:
        kind: external
        from: READY
        to: DELIVERED
        guards:
          is_internal: deny
        effect:
          - foreach:
              item: u
              array: units
              limit: 500
              steps:
                - call: { target: u, transition: deliver_internal }
      cancel:
        kind: external
        from: [PREPARATION, READY]
        to: CANCELLED
        effect:
          - foreach:
              item: u
              array: units
              where: u.state == Robot.RESERVED
              limit: 500
              steps:
                - call: { target: u, transition: release }
          - foreach:
              item: u
              array: pegged
              limit: 500
              steps:
                - call: { target: u, transition: unpeg }
      revoke:
        kind: external
        from: DELIVERED
        to: CANCELLED
        effect:
          - foreach:
              item: u
              array: units
              where: u.state == Robot.SOLD
              limit: 500
              steps:
                - call: { target: u, transition: unsell }
          - foreach:
              item: u
              array: units
              where: u.state == Robot.DEVELOPMENT
              limit: 500
              steps:
                - call: { target: u, transition: recall_internal }
      delete:
        kind: external
        from: CANCELLED
        to: DELETED
```

```yaml
# inventory_journey, continued
  ServiceJob:
    description: A job servicing one unit, which reserves the units it consumes as parts.
    tracking: record

    attributes:
      kind:       { type: ServiceKind }
      robot:      { reference: Robot, opposite: services }
      customer:   { reference: Customer }
      engineer:   { reference: User, assignee: true }
      parts_used: { reference: "Robot[]", opposite: used_in }

    states:
      OPEN:      { category: inbound }
      WORKING:   { category: live }
      DONE:      { category: closed }
      CANCELLED: { category: closed }
      DELETED:   { category: closed, final: true }

    derived_attributes:
      blocking:
        description: The job is a repair, which keeps the unit from being lent.
        expression: kind == ServiceKind.REPAIR

    summary: [kind, robot, state]

    conditions:
      active:
        description: The engineer assigned is active.
        expression: inputs.engineer.state == User.ACTIVE
        remedy: self_serviceable
      delivered:
        description: The unit has been sold or delivered into the pool.
        expression: inputs.robot.state == Robot.SOLD or inputs.robot.state == Robot.DEVELOPMENT
        remedy: self_serviceable
      theirs:
        description: A sold unit is serviced for the customer who bought it.
        expression: inputs.robot.state == Robot.DEVELOPMENT or inputs.robot.sold_to == inputs.customer
        remedy: self_serviceable
      still_serviceable:
        description: The unit is still in the pool, or still sold to the job's customer.
        expression: robot.state == Robot.DEVELOPMENT or (robot.state == Robot.SOLD and robot.sold_to == customer)
        remedy: dependent
      ours:
        description: The part is reserved for this job.
        expression: inputs.part.used_in == this
        remedy: self_serviceable

    transitions:
      open:
        kind: initial
        to: OPEN
        required_inputs: [kind, robot, customer, engineer]
        guards:
          active: deny
          delivered: deny
          theirs: deny
      reassign:
        kind: internal
        from: OPEN
        required_inputs: [engineer]
        guards:
          active: deny
      add_part:
        kind: internal
        from: [OPEN, WORKING]
        inputs:
          part: { reference: Robot }
        effect:
          - call: { target: inputs.part, transition: reserve_for_service, inputs: { job: this } }
      remove_part:
        kind: internal
        from: [OPEN, WORKING]
        inputs:
          part: { reference: Robot }
        guards:
          ours: deny
        effect:
          - call: { target: inputs.part, transition: release }
      start:
        kind: external
        from: OPEN
        to: WORKING
        backdating_limit: 2 days
      finish:
        kind: external
        from: WORKING
        to: DONE
        effect:
          - foreach:
              item: p
              array: parts_used
              limit: 50
              steps:
                - call: { target: p, transition: consume, inputs: { buyer: customer } }
      cancel:
        kind: external
        from: [OPEN, WORKING]
        to: CANCELLED
        effect:
          - foreach:
              item: p
              array: parts_used
              limit: 50
              steps:
                - call: { target: p, transition: release }
      void:
        kind: external
        from: DONE
        to: CANCELLED
        effect:
          - foreach:
              item: p
              array: parts_used
              limit: 50
              steps:
                - call: { target: p, transition: unconsume }
      reopen:
        kind: external
        from: CANCELLED
        to: OPEN
        description: Reopens a cancelled job for an active engineer, while its unit is still one the job may service.
        required_inputs: [engineer]
        guards:
          active: deny
          still_serviceable: deny
      delete:
        kind: external
        from: CANCELLED
        to: DELETED

    metrics:
      repairs:
        description: The repairs opened, by the model of the unit repaired and by month.
        source: objects
        item: s
        filter: s.kind == ServiceKind.REPAIR
        dimensions:
          model: s.robot.model
          month: month(s.created_at)
        time_dimension: s.created_at
        expression: count()

  Engagement:
    description: A loan of units out of the pool, for a lease, an event or a development hold, until they come back.
    tracking: record

    attributes:
      kind:            { type: EngagementKind }
      expected_return: { type: timestamp, optional: true, indexed: true }
      lines:
        reference: "EngagementLine[]"
        aggregation: composite
        opposite: engagement
        cascade:
          - { on: [close, cancel, settle], transition: end, limit: 20 }
      lease: { reference: Lease, optional: true, opposite: engagement }

    states:
      SCHEDULED: { category: inbound }
      OUT:       { category: live }
      RETURNING: { category: live }
      CLOSED:    { category: closed, final: true }

    derived_attributes:
      overdue:
        description: The units are out after their expected return.
        expression: state == OUT and expected_return < now

    summary: [kind, expected_return, state]

    conditions:
      leasable:
        description: The unit to add is fit and on no open engagement.
        expression: inputs.robot.leasable
        remedy: dependent
      room_for_unit:
        description: The engagement has fewer than 20 units on it, the most its end reaches.
        expression: count(l in lines where l.open) < 20
        remedy: dependent
      incoming_leasable:
        description: The unit swapped in is fit and on no open engagement.
        expression: inputs.in_robot.leasable
        remedy: dependent
      swapping_out:
        description: The unit swapped out is out on this engagement.
        expression: any(l in lines where l.robot == inputs.out_robot and l.open)
        remedy: self_serviceable
      nonempty:
        description: The engagement has at least one unit on it.
        expression: count(l in lines where l.open) >= 1
        remedy: unreachable_from_here
      fit:
        description: Every unit on the engagement is fit to go out.
        expression: "all(l in lines where l.open: l.robot.fit)"
        remedy: dependent
      checked:
        description: Every unit coming back has a return check recorded since the return began.
        expression: >-
          all(l in lines where l.open:
              any(r in l.robot.return_checks where r.occurred_at >= entered_at(RETURNING)))
        remedy: dependent
      return_known:
        description: A lease has an expected return.
        expression: kind != EngagementKind.LEASE or expected_return is not null
        remedy: unreachable_from_here

    transitions:
      schedule:
        kind: initial
        to: SCHEDULED
        required_inputs: [kind]
        optional_inputs: [expected_return]
      add_unit:
        kind: internal
        from: SCHEDULED
        inputs:
          robot: { reference: Robot }
        guards:
          room_for_unit: deny
          leasable: deny
        effect:
          - create: { type: EngagementLine, transition: open, inputs: { for_engagement: this, robot: inputs.robot } }
      set_expected_return:
        kind: internal
        from: [SCHEDULED, OUT, RETURNING]
        description: Sets or moves the date the units are expected back.
        required_inputs: [expected_return]
      dispatch:
        kind: external
        from: SCHEDULED
        to: OUT
        backdating_limit: 1 days
        guards:
          nonempty: deny
          fit: deny
          return_known: deny
      swap_unit:
        kind: internal
        from: OUT
        inputs:
          out_robot: { reference: Robot }
          in_robot:  { reference: Robot }
        guards:
          swapping_out: deny
          incoming_leasable: deny
        effect:
          - foreach:
              item: l
              array: lines
              where: l.robot == inputs.out_robot and l.open
              limit: 1
              steps:
                - call: { target: l, transition: end }
          - create: { type: EngagementLine, transition: open, inputs: { for_engagement: this, robot: inputs.in_robot } }
      start_return:
        kind: external
        from: OUT
        to: RETURNING
        backdating_limit: 1 days
      close:
        kind: external
        from: RETURNING
        to: CLOSED
        guards:
          checked: deny
      cancel:
        kind: external
        from: SCHEDULED
        to: CLOSED
      settle:
        kind: external
        from: OUT
        to: CLOSED
        only_via: [Lease.convert]

    metrics:
      engagements_overdue:
        description: The engagements past their expected return, by kind.
        source: objects
        item: e
        filter: e.overdue
        dimensions:
          kind: e.kind
        expression: count()
        flag_when: { any_overdue: "value > 0" }

  EngagementLine:
    description: One unit on one engagement, open from when it is added until the unit leaves it, by a swap or its retirement, or the engagement ends, whether scheduled, out or returning; an assertion moving the unit does not end its line.
    tracking: record

    attributes:
      engagement: { reference: Engagement, opposite: lines }
      robot:      { reference: Robot, opposite: engagement_lines }

    states:
      OPEN:  { category: live }
      ENDED: { category: closed, final: true }

    transitions:
      open:
        kind: initial
        to: OPEN
        only_via: [Engagement.add_unit, Engagement.swap_unit]
        required_inputs: [robot]
        inputs:
          for_engagement: { reference: Engagement }
        effect:
          - assign: { location: engagement, expr: inputs.for_engagement }
      end:
        kind: external
        from: OPEN
        to: ENDED
        only_via: [Engagement.close, Engagement.cancel, Engagement.settle, Engagement.swap_unit, Robot.retire]

    metrics:
      time_on_loan:
        description: The time units have spent on engagements, from being added to leaving, by model, kind of engagement and the month the time began.
        source: intervals
        item: i
        filter: i.state == EngagementLine.OPEN
        dimensions:
          model: i.object.robot.model
          kind: i.object.engagement.kind
          month: month(i.entered_at)
        time_dimension: i.entered_at
        expression: sum(i.duration)

  Lease:
    description: The commercial agreement above one engagement, which ends once the engagement closes or converts into a sale.
    tracking: record

    attributes:
      customer:   { reference: Customer }
      engagement: { reference: Engagement, opposite: lease, stored: true }

    states:
      ACTIVE:    { category: live }
      CONVERTED: { category: closed, final: true }
      ENDED:     { category: closed, final: true }

    summary: [customer, state]

    conditions:
      is_lease:
        description: The engagement is a lease.
        expression: inputs.engagement.kind == EngagementKind.LEASE
        remedy: self_serviceable
      out:
        description: The leased units are out.
        expression: engagement.state == Engagement.OUT
        remedy: dependent
      returned:
        description: "The engagement is closed: its units returned, never sent out, or sold when the lease converted."
        expression: engagement.state == Engagement.CLOSED
        remedy: dependent

    transitions:
      sign:
        kind: initial
        to: ACTIVE
        required_inputs: [customer, engagement]
        guards:
          is_lease: deny
      convert:
        kind: external
        from: ACTIVE
        to: CONVERTED
        guards:
          out: deny
        effect:
          - foreach:
              item: l
              array: engagement.lines
              where: l.open
              limit: 20
              steps:
                - call: { target: l.robot, transition: convert_lease, inputs: { buyer: customer } }
          - call: { target: engagement, transition: settle }
      end:
        kind: external
        from: ACTIVE
        to: ENDED
        guards:
          returned: deny
```

```yaml
# inventory_journey, continued

metrics:
  pool_utilisation:
    description: "Time on engagements over time in the pool, by model, over the whole history: the share of a pooled unit's time on an engagement, scheduled, out or returning, which can exceed 1 where an assertion moves a unit out of the pool with its line still open."
    input_metrics:
      on_loan: EngagementLine.time_on_loan
      pool: Robot.time_in_pool
    group_by: [model]
    expression: on_loan / pool
    flag_when: { idle: "value < 0.300" }
```

# The unit's journey: one lifecycle, with custody and condition beside it

Status: **worked example**, 2026-09-24, read against the first consumer's production code at `wr` HEAD `4109939`. It writes every transition production registers for an inventory unit, and the engagement and leasing model production's ADR-0002 accepted but has not built, as one module. Since 2026-09-26 it is written in the flow description format (ADR-0116), and `scripts/check-flow-docs.py` checks it with all four steps of the flow checker, the model's implemented publish checks among them at step 4, and reports it clean (ADR-0121). The specification's own `UnitLifecycle` (`declaration-syntax.md` §4) stays the smaller fixture its mutations are written against; this module is what the first consumer's unit becomes. It supersedes the lifecycle sketch of [`first-consumer-walkthrough.md`](first-consumer-walkthrough.md) §2.1, which predates three production changes listed in §5. The decisions it takes are recorded in [ADR-0102](../adr/0102-the-units-journey-is-one-lifecycle-with-custody-and-condition-beside-it.md), and what reading it against the PRD changed in [ADR-0103](../adr/0103-what-the-production-audit-required-of-the-design.md).

**What it is for.** The module exists to test the design, not to specify the first consumer's unit (the author, 2026-09-24). A real lifecycle, written in full from production, is how the engine is made to meet cases nobody invented for it; each part earns its place by the mechanism or requirement it exercises and by whether it finds a defect or shows a strength. Where a transition's own logic differs from production, that matters only if the difference exposes something the engine cannot express or record.

## 1. The journey, and why it is three questions

A unit is **requested**, **shipped**, **received**, **inventorised**, **reserved**, and **delivered**: sold to a customer, or delivered to ourselves into the company-owned pool. After delivery it is **serviced**, and a pooled unit is **loaned** out and comes back, many times over.

The first four stages and delivery are one lifecycle: what the unit is to the business. Servicing and lending are not further states of it. Production's ADR-0002 (`wr:docs/adr/0002-unit-engagement-and-leasing-model.md`, "The three axes") separates three questions, and this module keeps them apart:

| Question | Where it lives | States |
|---|---|---|
| What is the unit to the business? | the unit's lifecycle, `UnitLifecycle` | REQUESTED, PROCUREMENT, MISSING, INTAKE, AVAILABLE, RESERVED, SOLD, DEVELOPMENT, RETIRED, CANCELLED, DELETED |
| Who physically has it, and until when? | an `Engagement` and its lines; a `Lease` above one | SCHEDULED, OUT, RETURNING, CLOSED |
| Does it work? | a `ServiceJob` naming the unit | OPEN, WORKING, DONE, CANCELLED, DELETED |

The reason is a unit that is two things at once. A leased robot that fails is in the customer's hands **and** under repair; a single state cannot hold both, and whichever one wins, the other question goes unanswered. Each loan and each repair also has its own duration, owner and history, and a state has none of those. So the unit carries its lifecycle, and derives the rest:

- `on_loan`: an open engagement line names the unit;
- `in_service`: an open service job names it;
- `fit`: it is in `DEVELOPMENT` with no open blocking service, `REPAIR` being the blocking kind;
- `leasable`: fit, and on no open engagement. This is ADR-0002's "leasable now", three clauses because there are three questions.

```
REQUESTED ─ship─▶ PROCUREMENT ─receive─▶ INTAKE ─inventorize─▶ AVAILABLE ─reserve─▶ RESERVED ─sell──────────▶ SOLD
   ▲  ◀─unship──     │   ▲  ◀─unreceive──   ▲    ◀─revert_intake─  │ ▲  ◀─release──    │     ─deliver_internal─▶ DEVELOPMENT
   │                 │   │                  │                     │ │                 │     ─consume (a service part)─▶ SOLD
   └─rerequest─ MISSING ◀┘flag_missing      │                     │ └─accept_return / unsell / unconsume── SOLD
                 │  └─restore─▶ PROCUREMENT │                     └─release_to_stock / recall_internal── DEVELOPMENT
                 └─discard─▶ CANCELLED ◀─cancel─ REQUESTED | PROCUREMENT | INTAKE
DEVELOPMENT ─retire─▶ RETIRED        DEVELOPMENT ─convert_lease (only via Lease.convert)─▶ SOLD
AVAILABLE | CANCELLED ─delete─▶ DELETED

beside it:  Engagement  SCHEDULED ─dispatch─▶ OUT ─start_return─▶ RETURNING ─close─▶ CLOSED   (swap_unit at OUT)
            ServiceJob  OPEN ─start─▶ WORKING ─finish─▶ DONE ─void─▶ CANCELLED ─reopen─▶ OPEN
```

## 2. The module

The module is written in the flow description format (`flow-format.md`, ADR-0116), in blocks that each continue it, with the reasoning between them. It is in the format's order: the vocabulary, the lifecycle every unit shares, then the types, each with the metrics that read its rows (ADR-0118), and last the one metric that combines two of them.

The vocabulary. `CancellationReason` loses production's `MISSING_FROM_SHIPMENT`, which becomes a state (§4). Production records a retirement reason as free text and has not settled the vocabulary (`wr:TODO.md:243`), so `RetirementReason` is the specification's placeholder. The module states no rule about requests. *(Corrected 2026-09-26: this paragraph said that the module line states production's request rule, more strictly than production, that an agent sends an expected version and an idempotency key with every request (ADR-0103 §1). ADR-0114 withdrew that line, `requests by`, with everything else that said who may do what, and removed it from the module, but left this sentence; the rule is the upper layer's (ADR-0114 §3).)*

```yaml
module: inventory_journey
imports: { inventory: [User, Customer, RetirementReason, OverrideReason] }
categories: [inbound, live, closed]

enumerations:
  CancellationReason: [ORDER_CANCELLED, DISCARDED, REJECTED_QA, RETURNED_SUPPLIER, OTHER]
  EngagementKind: [LEASE, EVENT, DEV_HOLD]
  ServiceKind: [REPAIR, MAINTENANCE, UPGRADE]

sequences:
  unit_serial:
    description: Numbers the robots, in one series for the type, as production's serial service does.
```

**The lifecycle.** Every transition production registers for a robot (`wr:app/core/state_registry.py:859-993`), and the ones its services perform around the registry, are here; §3 maps each one. Production's permission checks, its roles and each key's permissions, are not here: who may request each transition is the upper layer's (ADR-0114), so the operations platform keeps them at its own edge. *(Corrected 2026-09-25: this module declared production's permissions as capabilities, and `ADMIN` as one, until ADR-0114 withdrew capabilities from the language.)*

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
        binding:             { reference: Delivery, optional: true }
        used_in:             { reference: ServiceJob, optional: true }
        sold_to:             { reference: Customer, optional: true }
        engagement_lines:    { reference: "EngagementLine[]" }
      invariants: [one_open_engagement, labelled, mfr_serial]

    states:
      REQUESTED:   { category: inbound }
      PROCUREMENT: { category: inbound }
      MISSING:     { category: inbound }
      INTAKE:      { category: inbound }
      AVAILABLE:   { category: live }
      RESERVED:    { category: live }
      SOLD:        { category: closed }
      DEVELOPMENT: { category: live }
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
        remedy: unreachable_from_here
      photo:
        description: The unit has the label photo its model requires.
        expression: model.label_photo_required implies count(p in photos) >= 1
        remedy: unreachable_from_here
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
        only_via: [Shipment.receive_unit]
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
        from: [REQUESTED, PROCUREMENT, MISSING, INTAKE]
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
        effect:
          - clear: [binding]
      accept_return:
        kind: external
        from: SOLD
        to: AVAILABLE
        effect:
          - clear: [binding, sold_to]

      release_to_stock:
        kind: external
        from: DEVELOPMENT
        to: AVAILABLE
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
      retire:
        kind: external
        from: DEVELOPMENT
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

      correct_state:
        kind: assertion
        to: [AVAILABLE, DEVELOPMENT, RETIRED]
        inputs:
          reason: { type: OverrideReason }
          detail: { type: string, optional: true, personal: true }
        may_admit: [one_open_engagement]

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
      binding:             { reference: Delivery, optional: true, opposite: units }
      used_in:             { reference: ServiceJob, optional: true, opposite: parts_used }
      sold_to:             { reference: Customer, optional: true }
      services:            { reference: "ServiceJob[]", opposite: robot }
      engagement_lines:    { reference: "EngagementLine[]", opposite: robot }

    derived_attributes:
      fit:
        description: The unit is in the pool, and no open service job blocks it.
        expression: state == DEVELOPMENT and none(s in services where s.open and s.blocking)
      leasable:
        description: The unit is fit and on no open engagement, which is ADR-0002's "leasable now".
        expression: fit and none(l in engagement_lines where l.open)
      on_loan:
        description: An open engagement line names the unit.
        expression: any(l in engagement_lines where l.open)
      in_service:
        description: An open service job names the unit.
        expression: any(s in services where s.open)

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

    metrics:
      inbound_dwell:
        description: The median time units spend in each inbound stage, by model and month.
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
        description: The longest time a unit of each shipment has been missing.
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
```

**What every unit on offer carries is an invariant, and what intake checks is a guard** (ADR-0109). A unit in `AVAILABLE` carries a recorded label print, and the manufacturer serial its model requires, whichever way it got there. `labelled` and `mfr_serial` are invariants of `Robot`, and the machine requires every binder to declare them, so they bind `inventorize`, `add_opening_stock`, a return, a release from the pool, an override and a ported unit alike, which a guard on one transition cannot. Production states the serial rule the same way, for every unit reaching `AVAILABLE`, at its birth or at the intake gate (`wr:app/services/inventory_item_service.py:277-286`, `:410-416`), and enforces it where a unit is born with the serial in hand, at opening stock (`:287-293`) and at a walk-in intake (`wr:app/services/intake_batch_service.py:398-404`); the label is the module's intent, as §4 says. `inventorize` keeps guards of the same names, since `availability` evaluates guards and not invariants, and its remedy, to print the label first, is the one a caller needs before asking. The photo is different. Production checks it only for an intake batch (`wr:app/services/intake_batch_service.py:816-821`), so `photo` stays a guard of intake alone, and opening stock does not carry it.

Three consequences:
- a ported unit on offer with no label print, or without a serial its model requires, is a violation the port reports, and its disposition admits it or cleans it upstream. An admission is discharged when the unit leaves `AVAILABLE`, so a unit that comes back is labelled before it goes on offer again;
- turning on a model's `manufacturer_serial_required` is refused while a unit of that model is on offer without one, naming the units. `record_manufacturer_serial` records it at any state, as production lets an existing unit's serial be edited (`wr:app/schemas/inventory.py:59-63`);
- `record_manufacturer_serial` and `add_photo` are new. Nothing wrote either after a unit's creation, so a unit ordered through `request` whose model requires a serial or a photo could never have been inventorised (D379).

Publishing reports what each creation skips, and `scripts/check-flow-docs.py` verifies that this is the report the module produces:

```
Creations that land past their lifecycle's first state
Robot.add_to_intake -> INTAKE
  skips receive, only via Shipment.receive_unit: no clause of its own
  checked on landing: labelled, mfr_serial, one_claim, one_open_engagement
Robot.add_opening_stock -> AVAILABLE
  skips inventorize: held by invariants: labelled, mfr_serial; not carried: photo
  checked on landing: labelled, mfr_serial, one_claim, one_open_engagement
```

A unit added straight to intake skips a shipment's receipt, which is what that creation is for, and opening stock skips the photo, as production's does. Both are visible where the declaration is reviewed, rather than found later in the data.

**The model catalogue.** The unit's rules read its model — whether a manufacturer serial or a label photo is required, and the maker's code a serial carries — so the model is declared here with the unit. Production's `robot_models` holds a name, a manufacturer, a warranty period and the two flags (`wr:app/models/robot_models.py:25-47`). The reorder point, and the two derivations that read it, are not production's: they are added to test PRD L4 and M7, a threshold that is governed data and a business exception over one object, and production defers reorder logic (`wr:app/models/procurement.py:15`).

```yaml
# inventory_journey, continued
  RobotModel:
    description: A model of robot, whose flags decide what its units carry before they go on offer.
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
        required_inputs: [name, warranty_months, label_photo_required, manufacturer_serial_required]
        optional_inputs: [manufacturer, maker_code]
      edit:
        kind: internal
        from: ACTIVE
        required_inputs: [warranty_months, label_photo_required, manufacturer_serial_required]
        optional_inputs: [manufacturer, maker_code]
      set_reorder_point:
        kind: internal
        from: ACTIVE
        required_inputs: [reorder_point]
      discontinue:
        kind: external
        from: ACTIVE
        to: DISCONTINUED
```

**Supply and demand.** A shipment is the receiving batch production reconciles, and a delivery binds units and pegs inbound ones. Production keeps the hard bind and the soft peg as two links (`wr:app/services/allocation.py:50-84,104-150`), and so does this module: `binding` and `peg`.

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
        description: The unit is not the last one still in procurement, since the shipment would be left empty.
        expression: count(u in units where u.state == Robot.PROCUREMENT) >= 2
        remedy: self_serviceable
      empty:
        description: No unit of the shipment is still in procurement.
        expression: none(u in units where u.state == Robot.PROCUREMENT)
        remedy: self_serviceable
      reconciled:
        description: Every unit of the shipment has been received, flagged missing or taken off it.
        expression: none(u in units where u.state == Robot.PROCUREMENT)
        remedy: self_serviceable
      pristine:
        description: Every unit the commit put on offer is still on offer, so the commit can be undone.
        expression: >-
          all(u in units where u.state != Robot.MISSING
                           and u.state != Robot.CANCELLED:
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
      internal: { type: bool, default: false }

    states:
      PREPARATION: { category: live }
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
        required_inputs: [customer, internal]
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
      complete_sale:
        kind: external
        from: PREPARATION
        to: DELIVERED
        guards:
          not_internal: deny
          filled: deny
        effect:
          - foreach:
              item: u
              array: units
              limit: 500
              steps:
                - call: { target: u, transition: sell, inputs: { buyer: customer } }
      complete_internal:
        kind: external
        from: PREPARATION
        to: DELIVERED
        guards:
          is_internal: deny
          filled: deny
        effect:
          - foreach:
              item: u
              array: units
              limit: 500
              steps:
                - call: { target: u, transition: deliver_internal }
      cancel:
        kind: external
        from: PREPARATION
        to: CANCELLED
        effect:
          - foreach:
              item: u
              array: units
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

**Condition and custody.** A service job names the unit it services and reserves the units it consumes as parts. An engagement takes units out and brings them back, and a lease is the commercial agreement above one.

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
      incoming_leasable:
        description: The unit swapped in is fit and on no open engagement.
        expression: inputs.in_robot.leasable
        remedy: dependent
      nonempty:
        description: The engagement has at least one unit.
        expression: count(l in lines) >= 1
        remedy: self_serviceable
      fit:
        description: Every unit on the engagement is fit to go out.
        expression: "all(l in lines: l.robot.fit)"
        remedy: dependent
      return_known:
        description: A lease has an expected return.
        expression: kind != EngagementKind.LEASE or expected_return is not null
        remedy: self_serviceable

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
          leasable: deny
        effect:
          - create: { type: EngagementLine, transition: open, inputs: { for_engagement: this, robot: inputs.robot } }
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
    description: One unit on one engagement, open while the unit is out on it.
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
        description: The time units have spent out on engagements, by model, kind of engagement and month.
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
    description: The commercial agreement above one engagement, which ends when the units come back or converts into a sale.
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
        description: The leased units have come back.
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

**What becomes readable.** Every metric is computed on read from the events and intervals the transitions above already record; none needs a column, a job or a report. Each is declared with the type whose rows it reads:

| Metric | Type | What it reads |
|---|---|---|
| `inbound_dwell` | `Robot` | the median time in each inbound stage, flagged `slow` over 14 days |
| `missing_units` | `Robot` | the longest a unit of each shipment has been missing, flagged `unresolved` over 7 days |
| `retirements` | `Robot` | units retired, by model and reason |
| `time_in_pool` | `Robot` | time spent in `DEVELOPMENT`, by model and month |
| `repairs` | `ServiceJob` | repairs opened, by model and month |
| `engagements_overdue` | `Engagement` | engagements past their expected return, flagged `any_overdue` |
| `time_on_loan` | `EngagementLine` | time out on an engagement, by model, kind and month |

The one metric that crosses types is declared last:

```yaml
# inventory_journey, continued

metrics:
  pool_utilisation:
    description: The share of a pooled unit's time spent on loan, by model, over the whole history.
    input_metrics:
      on_loan: EngagementLine.time_on_loan
      pool: Robot.time_in_pool
    group_by: [model]
    expression: on_loan / pool
    flag_when: { idle: "value < 0.300" }
```

Pool utilisation, the share of a pooled unit's time spent on loan, is the question ADR-0002 asks first ("how much did we use it?"). `pool_utilisation` divides `time_on_loan` by `time_in_pool` per model, over the whole history. Writing this module found that a duration could not be divided by a duration (`design/defects.md` D275), and ADR-0103 made the quotient of two like quantities a decimal. It is not declared per month: a metric buckets a span by the month it began, so a unit that joined the pool in January would put all its pool time in January, and apportioning a span across months is a known limit (`edge-cases.md`, ADR-0103 §5).

## 3. Every transition, against production

`sr` is `wr:app/core/state_registry.py`; `sh` is `wr:app/services/shipment_service.py`; `ib` is `wr:app/services/intake_batch_service.py`; `sv` is `wr:app/services/service_service.py`.

| Transition | From → to | Production | Note |
|---|---|---|---|
| `request` | → REQUESTED | procurement order creation births units REQUESTED (`wr:app/services/procurement.py:150-162`) | |
| `add_to_intake` | → INTAKE | manual add (`wr:app/services/inventory_item_service.py:193-221`) | |
| `add_opening_stock` | → AVAILABLE | the opening-stock fast path, same predicate at creation (`wr:docs/proposals/operations-system-design.md:308`), refusing a unit with no manufacturer serial where its model requires one (`wr:app/services/inventory_item_service.py:277-293`) | a port of existing stock, and who may take it is the upper layer's (ADR-0114). *(Corrected 2026-09-26: this cell said it was gated on `ASSERT`, a capability ADR-0114 withdrew together with the guard that tested it.)* It copies none of intake's guards: the label and the serial are invariants that bind it like every route into `AVAILABLE`, and the photo, which production checks only at intake, is intake's alone; the publish report lists what it skips (D345, ADR-0109) |
| `ship` | REQUESTED → PROCUREMENT | `sr:861-865`; `sh:165-317` stamps `shipment_id` | only via the shipment |
| `unship` | PROCUREMENT → REQUESTED | `sr:867-875`, guarded to in-transit shipments in the service | only via `Shipment.remove_unit`, at `IN_TRANSIT` |
| `receive` | PROCUREMENT → INTAKE | `sr:884-888`; `sh:757-980` | only via `Shipment.receive_unit`, at `ARRIVED`, whose `backdating_limit` is `2 days`, so time in PROCUREMENT ends when the unit arrived (PRD UC-6) |
| `unreceive` | INTAKE → PROCUREMENT | `sr:890-898` | |
| `flag_missing` | PROCUREMENT → MISSING | `sh:985-1055`: PROCUREMENT → CANCELLED with reason `MISSING_FROM_SHIPMENT`, peg released | **a state here**, §4 |
| `restore` | MISSING → PROCUREMENT | `sr:907-915`; `sh:1099` | production leaves a terminal state by an explicit override |
| `rerequest` | MISSING → REQUESTED | `sr:917-925`; `sh:1153` | |
| `discard` | MISSING → CANCELLED | `sh:1202-1238`: the reason changes to `DISCARDED` | |
| `cancel` | REQUESTED, PROCUREMENT, INTAKE → CANCELLED | `sr:877-883,900-905,933-938`, each releasing the peg | one transition with a reason |
| `peg_to`, `unpeg` | internal | `wr:app/services/allocation.py:104-200` | |
| `record_label_print` | internal, from any state but a final one | production allows it in any state (`wr:app/services/label_print_service.py:174-197`) | `RETIRED` is `closed` and not terminal, so a retired unit can be relabelled, as in production; only `DELETED` is final (ADR-0103 §8) |
| `record_manufacturer_serial` | internal, from any state but a final one | editable on an intake item before commit (`ib:470-520`) and on an existing robot (`wr:app/schemas/inventory.py:59-63`) | added, since nothing wrote it after creation (D379) |
| `add_photo` | internal, at INTAKE | an intake item's confirmation photos, carried onto the unit at commit (`ib:874-880`, `ib:936`) | added, for the same reason (D379) |
| `inventorize` | INTAKE → AVAILABLE | `sr:926-931`; `ib:635-910`; the peg is released at commit (`ib:912-934`) | production gates on the label only, behind `LABEL_PRINTING_ENABLED`, default off (`sr:695-724`); photos are checked per batch; `labelled` and `mfr_serial` are also invariants of every unit on offer (ADR-0109) |
| `revert_intake` | AVAILABLE → INTAKE | `sr:940-945`; `ib:1006-1124` | only via `Shipment.revert_commit` |
| `reserve` | AVAILABLE → RESERVED | `sr:947-951`; `wr:app/services/inventory_helpers.py:40-80` | |
| `reserve_for_service` | AVAILABLE → RESERVED | `sv:585-600` reserves a part | production's registry names one transition for both |
| `release` | RESERVED → AVAILABLE | `sr:953-957` | four parents |
| `sell` | RESERVED → SOLD | `sr:959-963`; delivery completion (`sr:85-131`) | |
| `deliver_internal` | RESERVED → DEVELOPMENT | `sr:965-969` | |
| `consume` | RESERVED → SOLD | service completion sells parts (`sr:813-820`; `sv:858-943`) | a separate name, so a metric can tell a sale from a part used |
| `unconsume` | SOLD → AVAILABLE | cancelling a completed service (`sr:827-836`) | |
| `unsell`, `recall_internal` | SOLD, DEVELOPMENT → AVAILABLE | cancelling a completed delivery (`sr:782-790`) | refused here if the unit has moved on; production sets AVAILABLE unchecked (`sr:169-187`) |
| `accept_return` | SOLD → AVAILABLE | `sr:971-977`, admin | |
| `release_to_stock` | DEVELOPMENT → AVAILABLE | `sr:979-985`, admin | refused while on loan |
| `convert_lease` | DEVELOPMENT → SOLD | ADR-0002, "Leasing": guarded on an active lease being converted | not built in production |
| `retire` | DEVELOPMENT → RETIRED | `sr:987-993`; `wr:app/services/inventory_item_service.py:542-681` | ends an open engagement line, ADR-0002's close-with-reason. `RETIRED` has no exit, as in production, and needs none, being closed |
| `delete` | AVAILABLE, CANCELLED → DELETED | `sr:1003`, a soft delete | |
| `correct_state` | assertion | none; production writes status directly or by `force_transition` (`wr:app/core/state_transition.py:577-619`) | the one override, with a declared reason |

## 4. Where this module departs from production, and why

- **A missing unit is a state, not a cancelled one.** Production cancels a unit it flags missing and records the reason `MISSING_FROM_SHIPMENT` (`sh:985-1055`), then leaves `CANCELLED` by an explicit rule that overrides its terminal status (`sr:907-925`) when the unit is restored or re-requested. Its own design says a missing unit "stays expected" (`wr:docs/proposals/operations-system-design.md:154`). Here it is `MISSING`, category `inbound`, so it counts as open work, ages in `inbound_dwell` and `missing_units`, and `CANCELLED` means cancelled. The port maps `(CANCELLED, MISSING_FROM_SHIPMENT)` to `MISSING` and `(CANCELLED, DISCARDED)` to `CANCELLED`.
- **Closed is not terminal.** Production marks states terminal and then leaves them: `CANCELLED` (above), a delivered delivery (`sr:782-790`), a completed service (`sr:827-836`), and a committed intake batch, which revert leaves (`ib:1104`). Each is `closed` and not `terminal` here, with a terminal `DELETED` or `VOIDED` after it where production has one, as the specification's §4.2 advises. Work that is closed stops counting as open, whatever can still happen to it.
- **A cascade that cannot apply refuses.** Production's delivery revoke sets its units to AVAILABLE whatever they became since (`sr:169-187`); here `unsell` and `recall_internal` refuse, naming the unit, if it is no longer `SOLD` or `DEVELOPMENT`.
- **Engagements and leases exist.** Production has accepted them and not built them (ADR-0002, "Open questions"). Its open questions stay open here and are listed in §6.
- **The gates are production's intent, not its switch.** `inventorize` requires the label, the manufacturer serial where the model requires one, and a photo where the model requires one. Production enforces the label only, behind a flag that defaults to off. The equivalent of that flag is an `audit` enforcement of `inventorize`'s guard `labelled` (the model's `observe`), published to enforce once its would-be refusals are counted (ADR-0085), rather than an environment variable. The invariant `labelled` is published with the enforcement, since an invariant is not trialled itself; the trial runs on the guard of the same name (ADR-0109).
- **A unit records whom it was sold to.** Every route into `SOLD` — a sale, a service's consumption, a lease-to-own conversion — writes `sold_to`, and every route out clears it, and a service job opened on a sold unit checks that the customer is its buyer. Production checks instead that the unit appears on one of the customer's deliveries (`wr:app/services/service_service.py:442-486`), which a unit bought out of a lease never does, so its buyer could not have asked for a repair (D289). `sold_to` is tracked, so who owned a unit and when is in its intervals.
- **The serial is production's.** One sequence per type, formatted `RBT-{maker}-{NNNNNN}`, the maker's segment dropped where a model has no manufacturer (`wr:app/services/serial_number_service.py:37-41,117-196`). Production derives the maker's code from the manufacturer's name each time; here `RobotModel` holds it as an optional `maker_code`, which the port fills with production's derivation and an operator sets for a new model, since the language has no substring. The import carries the sequence's high-water mark, so the first serial minted after cutover follows production's last (ADR-0103 §4).
- **Simplifications.** A delivery's slots are not modelled, so `filled` reads the delivery's own units and pegs; a reopened service re-adds its parts, where production keeps part rows and re-reserves them; a reopened delivery is not modelled. Accessories and spare parts bind the same machine in production (`sr:1008-1300`) and would here, as further binders.

## 5. What changed since the walkthrough

[`first-consumer-walkthrough.md`](first-consumer-walkthrough.md) §2.1 and §5 were written against production as it stood on 2026-09-07. Three things have changed since:

1. **Commit no longer fills a peg.** Production retired the commit-time auto-fill: committing a batch releases each unit's peg and the unit joins free stock until an operator assigns it (`wr:docs/design/explicit-delivery-assignment.md`; `ib:912-934`). The walkthrough's second commit loop is gone here.
2. **A flagged-missing unit leaves `PROCUREMENT`.** The walkthrough has `flag_missing` leave it there; production cancels it, and this module moves it to `MISSING`.
3. **The bind and the peg are two links.** The walkthrough and the specification store one `binding`; production stores a hard bind and a soft peg separately.

## 6. What stays open

These are production's ADR-0002 open questions, unchanged by writing the module, and each is a publish when decided:

1. When a lease may convert to a sale: at any time while active, as here, or only at term end.
2. Whether a `RETURNING` engagement makes a unit unavailable, and whether an open `UPGRADE` blocks lending as `REPAIR` does: `blocking` is one derived line to change.
3. Whether a `SCHEDULED` engagement three weeks away makes a unit unavailable today. Here it does, since `leasable` reads any open line.
4. Whether an engagement may take a `SOLD` unit, for instance one in for repair. Here only `DEVELOPMENT` units are fit.
5. The retirement reasons.

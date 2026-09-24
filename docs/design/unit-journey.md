# The unit's journey: one lifecycle, with custody and condition beside it

Status: **worked example**, 2026-09-24, read against the first consumer's production code at `wr` HEAD `4109939`. It writes every transition production registers for an inventory unit, and the engagement and leasing model production's ADR-0002 accepted but has not built, as one module. `scripts/check-syntax-doc.py docs/design/unit-journey.md` checks it against the implemented checks and reports it clean. The specification's own `UnitLifecycle` (`declaration-syntax.md` §4) stays the smaller fixture its mutations are written against; this module is what the first consumer's unit becomes. It supersedes the lifecycle sketch of [`first-consumer-walkthrough.md`](first-consumer-walkthrough.md) §2.1, which predates three production changes listed in §5. The decisions it takes are recorded in [ADR-0102](../adr/0102-the-units-journey-is-one-lifecycle-with-custody-and-condition-beside-it.md), and what reading it against the PRD changed in [ADR-0103](../adr/0103-what-the-production-audit-required-of-the-design.md).

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

The vocabulary. `CancellationReason` loses production's `MISSING_FROM_SHIPMENT`, which becomes a state (§4). Production records a retirement reason as free text and has not settled the vocabulary (`wr:TODO.md:243`), so `RetirementReason` is the specification's placeholder. The module line states production's request rule once, and more strictly than production: an agent sends an expected version and an idempotency key with every request, where production requires the key only on an agent's POSTs and the version only where a service method checks one (`wr:app/services/base_service.py:236-251`; `wr:app/core/idempotency.py:149-179`; ADR-0103 §1).

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
  attr manufacturer_serial_required bool default false indexed
  attr reorder_point                int default 0

  ref  units : Robot[] inverse model

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

**The lifecycle.** Every transition production registers for a robot (`wr:app/core/state_registry.py:859-993`), and the ones its services perform around the registry, are here; §3 maps each one. The capabilities are production's permissions, and `ADMIN` is what production's `required_role="ADMIN"` becomes, since a guard reads capabilities, not roles (ADR-0079).

```text
machine UnitLifecycle version 3 {
  requires attr label_printed_at timestamp?
  requires attr manufacturer_serial string?
  requires attr photos file[]
  requires attr cancellation_reason CancellationReason?
  requires attr retirement_reason RetirementReason?
  requires ref  model : RobotModel
  requires ref  shipment : Shipment?
  requires ref  peg : Delivery?
  requires ref  binding : Delivery?
  requires ref  used_in : ServiceJob?
  requires ref  sold_to : Customer?
  requires ref  engagement_lines : EngagementLine[]
  requires invariant one_open_engagement
  requires invariant labelled
  requires invariant mfr_serial
  requires capability EDIT, ADMIN, DELETE, ASSERT, CANCEL

  state REQUESTED   category inbound
  state PROCUREMENT category inbound
  state MISSING     category inbound
  state INTAKE      category inbound
  state AVAILABLE   category live
  state RESERVED    category live
  state SOLD        category closed
  state DEVELOPMENT category live
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
    require may: actor.has(ASSERT) because delegable
  }

  do ship REQUESTED -> PROCUREMENT only via Shipment.dispatch, Shipment.add_unit {
    input via_shipment : Shipment
    set shipment := inputs.via_shipment
  }
  do unship PROCUREMENT -> REQUESTED only via Shipment.remove_unit {
    clear shipment
  }
  do receive PROCUREMENT -> INTAKE only via Shipment.receive_unit { }
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
  act record_manufacturer_serial at any accepts manufacturer_serial {
    require may: actor.has(EDIT) because delegable
  }
  act add_photo at INTAKE {
    input photo : file
    require may: actor.has(EDIT) because delegable
    add photos := inputs.photo
  }

  do inventorize INTAKE -> AVAILABLE {
    require may:        actor.has(EDIT)                            because delegable
    require labelled:   label_printed_at is not null               because unreachable_from_here
    require mfr_serial: model.manufacturer_serial_required
                        implies manufacturer_serial is not null    because unreachable_from_here
    require photo:      model.label_photo_required
                        implies count(p in photos) >= 1            because unreachable_from_here
    clear peg
  }
  do revert_intake AVAILABLE -> INTAKE only via Shipment.revert_commit { }

  do reserve AVAILABLE -> RESERVED only via Delivery.bind_slot {
    input slot : Delivery
    require open: inputs.slot.state == Delivery.PREPARATION because dependent
    set binding := inputs.slot
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
  do accept_return SOLD -> AVAILABLE {
    require may: actor.has(ADMIN) because delegable
    clear binding
    clear sold_to
  }

  do release_to_stock DEVELOPMENT -> AVAILABLE {
    require may:  actor.has(ADMIN) because delegable
    require home: none(l in engagement_lines where l.open) because dependent
    clear binding
  }
  do convert_lease DEVELOPMENT -> SOLD only via Lease.convert {
    input buyer : Customer
    set sold_to := inputs.buyer
  }
  do retire DEVELOPMENT -> RETIRED {
    input reason : RetirementReason
    require may:   actor.has(DELETE) because delegable
    require admin: actor.has(ADMIN)  because delegable
    set retirement_reason := inputs.reason
    for l in engagement_lines where l.open limit 1 { call l.end() }
  }
  do delete { AVAILABLE, CANCELLED } -> DELETED {
    require may: actor.has(DELETE) because delegable
  }

  assert correct_state -> { AVAILABLE, DEVELOPMENT, RETIRED } {
    input to : state
    input reason : OverrideReason
    input detail : string? personal
    input admits : invariant[]?
    require may: actor.has(ASSERT) because delegable
    may admit one_open_engagement
  }
}

type Robot version 4 {
  tracking serial
  machine  UnitLifecycle
  provides capability EDIT = INVENTORY_CREATE, ADMIN = ADMIN, DELETE = INVENTORY_DELETE,
                      ASSERT = INVENTORY_ASSERT, CANCEL = PROCUREMENT_CANCEL
  summary  serial, model, state

  attr serial string identifier from unit_serial
                     format "RBT-[{model.maker_code}-]{n:6}" indexed unique
  attr manufacturer_serial string? indexed
  attr label_printed_at    timestamp?
  attr photos              file[]
  attr cancellation_reason CancellationReason?
  attr retirement_reason   RetirementReason?

  ref  model            : RobotModel inverse units
  ref  shipment         : Shipment? inverse units
  ref  peg              : Delivery? inverse pegged
  ref  binding          : Delivery? inverse units
  ref  used_in          : ServiceJob? inverse parts_used
  ref  sold_to          : Customer?
  ref  services         : ServiceJob[] inverse robot
  ref  engagement_lines : EngagementLine[] inverse robot

  derive fit        = state == DEVELOPMENT
                  and none(s in services where s.open and s.blocking)
  derive leasable   = fit and none(l in engagement_lines where l.open)
  derive on_loan    = any(l in engagement_lines where l.open)
  derive in_service = any(s in services where s.open)

  invariant one_open_engagement: count(l in engagement_lines where l.open) <= 1
  invariant one_claim:           binding is null or used_in is null
  invariant labelled:            state != AVAILABLE or label_printed_at is not null
  invariant mfr_serial:          state != AVAILABLE or not model.manufacturer_serial_required
                                 or manufacturer_serial is not null
}
```

**What every unit on offer carries is an invariant, and what intake checks is a guard** (ADR-0109). A unit in `AVAILABLE` carries a recorded label print, and the manufacturer serial its model requires, whichever way it got there. `labelled` and `mfr_serial` are invariants of `Robot`, and the machine requires every binder to declare them, so they bind `inventorize`, `add_opening_stock`, a return, a release from the pool, an override and a ported unit alike, which a guard on one transition cannot. Production states the serial rule the same way, for every unit reaching `AVAILABLE`, at its birth or at the intake gate (`wr:app/services/inventory_item_service.py:277-286`, `:410-416`), and enforces it where a unit is born with the serial in hand, at opening stock (`:287-293`) and at a walk-in intake (`wr:app/services/intake_batch_service.py:398-404`); the label is the module's intent, as §4 says. `inventorize` keeps guards of the same names, since `availability` evaluates guards and not invariants, and its remedy, to print the label first, is the one a caller needs before asking. The photo is different. Production checks it only for an intake batch (`wr:app/services/intake_batch_service.py:816-821`), so `photo` stays a guard of intake alone, and opening stock does not carry it.

Three consequences:
- a ported unit on offer with no label print, or without a serial its model requires, is a violation the port reports, and its disposition admits it or cleans it upstream. An admission is discharged when the unit leaves `AVAILABLE`, so a unit that comes back is labelled before it goes on offer again;
- turning on a model's `manufacturer_serial_required` is refused while a unit of that model is on offer without one, naming the units. `record_manufacturer_serial` records it at any state, as production lets an existing unit's serial be edited (`wr:app/schemas/inventory.py:59-63`);
- `record_manufacturer_serial` and `add_photo` are new. Nothing wrote either after a unit's creation, so a unit ordered through `request` whose model requires a serial or a photo could never have been inventorised (D379).

Publishing reports what each creation skips, and `scripts/check-syntax-doc.py` verifies that this is the report the module produces:

```
Creations that land past their lifecycle's first state
Robot.add_to_intake -> INTAKE
  skips receive, only via Shipment.receive_unit: no clause of its own
  checked on landing: labelled, mfr_serial, one_claim, one_open_engagement
Robot.add_opening_stock -> AVAILABLE
  skips inventorize: carried: may; held by invariants: labelled, mfr_serial; not carried: photo
  checked on landing: labelled, mfr_serial, one_claim, one_open_engagement
```

A unit added straight to intake skips a shipment's receipt, which is what that creation is for, and opening stock skips the photo, as production's does. Both are visible where the declaration is reviewed, rather than found later in the data.

**Supply and demand.** A shipment is the receiving batch production reconciles, and a delivery binds units and pegs inbound ones. Production keeps the hard bind and the soft peg as two links (`wr:app/services/allocation.py:50-84,104-150`), and so does this module: `binding` and `peg`.

```text
type Shipment version 1 {
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

type Delivery version 3 {
  tracking record
  states   PREPARATION category live, DELIVERED category closed,
           CANCELLED category closed, DELETED category closed terminal
  summary  customer, state

  ref  customer : Customer
  ref  pegged   : Robot[] inverse peg
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
  do complete_sale PREPARATION -> DELIVERED {
    require not_internal: not internal                                  because unreachable_from_here
    require filled:       count(u in units) >= 1
                          and none(u in pegged)                         because dependent
    require may:          actor.has(DELIVERY_COMPLETE)                  because delegable
    for u in units limit 500 { call u.sell(buyer := customer) }
  }
  do complete_internal PREPARATION -> DELIVERED {
    require is_internal: internal                                       because unreachable_from_here
    require filled:      count(u in units) >= 1
                         and none(u in pegged)                          because dependent
    require may:         actor.has(DELIVERY_COMPLETE)                   because delegable
    for u in units limit 500 { call u.deliver_internal() }
  }
  do cancel PREPARATION -> CANCELLED {
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

**Condition and custody.** A service job names the unit it services and reserves the units it consumes as parts. An engagement takes units out and brings them back, and a lease is the commercial agreement above one.

```text
type ServiceJob version 3 {
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

type Engagement version 1 {
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
    require may: actor.has(LEASE_EDIT) because delegable
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

**What becomes readable.** Every metric below is computed on read from the events and intervals the transitions above already record; none needs a column, a job or a report.

```text
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

Pool utilisation, the share of a pooled unit's time spent on loan, is the question ADR-0002 asks first ("how much did we use it?"). `pool_utilisation` divides `time_on_loan` by `time_in_pool` per model, over the whole history. Writing this module found that a duration could not be divided by a duration (`design/defects.md` D275), and ADR-0103 made the quotient of two like quantities a decimal. It is not declared per month: a metric buckets a span by the month it began, so a unit that joined the pool in January would put all its pool time in January, and apportioning a span across months is a known limit (`edge-cases.md`, ADR-0103 §5).

## 3. Every transition, against production

`sr` is `wr:app/core/state_registry.py`; `sh` is `wr:app/services/shipment_service.py`; `ib` is `wr:app/services/intake_batch_service.py`; `sv` is `wr:app/services/service_service.py`.

| Transition | From → to | Production | Note |
|---|---|---|---|
| `request` | → REQUESTED | procurement order creation births units REQUESTED (`wr:app/services/procurement.py:150-162`) | |
| `add_to_intake` | → INTAKE | manual add (`wr:app/services/inventory_item_service.py:193-221`) | |
| `add_opening_stock` | → AVAILABLE | the opening-stock fast path, same predicate at creation (`wr:docs/proposals/operations-system-design.md:308`), refusing a unit with no manufacturer serial where its model requires one (`wr:app/services/inventory_item_service.py:277-293`) | gated on `ASSERT`, as a port of existing stock. It copies none of intake's guards: the label and the serial are invariants that bind it like every route into `AVAILABLE`, and the photo, which production checks only at intake, is intake's alone; the publish report lists what it skips (D345, ADR-0109) |
| `ship` | REQUESTED → PROCUREMENT | `sr:861-865`; `sh:165-317` stamps `shipment_id` | only via the shipment |
| `unship` | PROCUREMENT → REQUESTED | `sr:867-875`, guarded to in-transit shipments in the service | only via `Shipment.remove_unit`, at `IN_TRANSIT` |
| `receive` | PROCUREMENT → INTAKE | `sr:884-888`; `sh:757-980` | only via `Shipment.receive_unit`, at `ARRIVED`, which is `backdatable within 2 days`, so time in PROCUREMENT ends when the unit arrived (PRD UC-6) |
| `unreceive` | INTAKE → PROCUREMENT | `sr:890-898` | |
| `flag_missing` | PROCUREMENT → MISSING | `sh:985-1055`: PROCUREMENT → CANCELLED with reason `MISSING_FROM_SHIPMENT`, peg released | **a state here**, §4 |
| `restore` | MISSING → PROCUREMENT | `sr:907-915`; `sh:1099` | production leaves a terminal state by an explicit override |
| `rerequest` | MISSING → REQUESTED | `sr:917-925`; `sh:1153` | |
| `discard` | MISSING → CANCELLED | `sh:1202-1238`: the reason changes to `DISCARDED` | |
| `cancel` | REQUESTED, PROCUREMENT, INTAKE → CANCELLED | `sr:877-883,900-905,933-938`, each releasing the peg | one transition with a reason |
| `peg_to`, `unpeg` | act | `wr:app/services/allocation.py:104-200` | |
| `record_label_print` | act, any non-terminal state | production allows it in any state (`wr:app/services/label_print_service.py:174-197`) | `RETIRED` is `closed` and not terminal, so a retired unit can be relabelled, as in production; only `DELETED` is final (ADR-0103 §8) |
| `record_manufacturer_serial` | act, any non-terminal state | editable on an intake item before commit (`ib:470-520`) and on an existing robot (`wr:app/schemas/inventory.py:59-63`) | added, since nothing wrote it after creation (D379) |
| `add_photo` | act at INTAKE | an intake item's confirmation photos, carried onto the unit at commit (`ib:874-880`, `ib:936`) | added, for the same reason (D379) |
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
| `correct_state` | assert | none; production writes status directly or by `force_transition` (`wr:app/core/state_transition.py:577-619`) | the one override, with a declared reason |

## 4. Where this module departs from production, and why

- **A missing unit is a state, not a cancelled one.** Production cancels a unit it flags missing and records the reason `MISSING_FROM_SHIPMENT` (`sh:985-1055`), then leaves `CANCELLED` by an explicit rule that overrides its terminal status (`sr:907-925`) when the unit is restored or re-requested. Its own design says a missing unit "stays expected" (`wr:docs/proposals/operations-system-design.md:154`). Here it is `MISSING`, category `inbound`, so it counts as open work, ages in `inbound_dwell` and `missing_units`, and `CANCELLED` means cancelled. The port maps `(CANCELLED, MISSING_FROM_SHIPMENT)` to `MISSING` and `(CANCELLED, DISCARDED)` to `CANCELLED`.
- **Closed is not terminal.** Production marks states terminal and then leaves them: `CANCELLED` (above), a delivered delivery (`sr:782-790`), a completed service (`sr:827-836`), and a committed intake batch, which revert leaves (`ib:1104`). Each is `closed` and not `terminal` here, with a terminal `DELETED` or `VOIDED` after it where production has one, as the specification's §4.2 advises. Work that is closed stops counting as open, whatever can still happen to it.
- **A cascade that cannot apply refuses.** Production's delivery revoke sets its units to AVAILABLE whatever they became since (`sr:169-187`); here `unsell` and `recall_internal` refuse, naming the unit, if it is no longer `SOLD` or `DEVELOPMENT`.
- **Engagements and leases exist.** Production has accepted them and not built them (ADR-0002, "Open questions"). Its open questions stay open here and are listed in §6.
- **The gates are production's intent, not its switch.** `inventorize` requires the label, the manufacturer serial where the model requires one, and a photo where the model requires one. Production enforces the label only, behind a flag that defaults to off. The equivalent of that flag is an `observe` marking on `inventorize`'s `labelled`, published to enforce once its would-be refusals are counted (ADR-0085), rather than an environment variable. The invariant `labelled` is published with the enforcement, since an invariant is not trialled itself; the trial runs on the guard of the same name (ADR-0109).
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

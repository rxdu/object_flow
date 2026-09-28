# Metric scenarios: UC-1 to UC-21

Status: **worked scenarios**, 2026-09-27, written at the author's direction: "write the UC-1 to UC-3 checked scenarios, continue with your inference, as long as the inference is based clear requirements we've already discussed and the use case we've reviewed". The author then asked for the next three, "write the scenarios for UC-4 to UC-6" (§8 to §12), then "write the scenarios for UC-7 to UC-9" (§13 to §17), then "write the scenarios for UC-10 to UC-12" (§18 to §22), then "write the scenarios for UC-13 to UC-15" (§23 to §28), then "write the scenarios for UC-16 to UC-18" (§29 to §33), and then "write the scenarios for UC-19 to UC-21" (§34 to §39). Each of the PRD's first twenty-one use cases is written here as a fixed flow, a fixed history and the values its reads return, so a builder has a test and a reader has something to compute and compare (`TODO.md`, "Write the metric use cases as checked scenarios").

**Method.** One store and one history, in twelve parts, with each use case's questions asked of it. The flow is the robot inventory of [`unit-journey.md`](unit-journey.md) §2, which ten rounds of cold readers found determined in everything a request does ([`flow-recoverability-review.md`](flow-recoverability-review.md)), with the agent type that writing these scenarios added to it (§1). Every value below is derived by hand, a metric's from [`declaration-syntax.md`](declaration-syntax.md) §6.9 and §6.11 and every other read's from the read surface of `DESIGN.md` §10 and the rule set of `renderers.md` §2, and [`scripts/check-scenarios.py`](../../scripts/check-scenarios.py) recomputes each one independently. The script also replays the history against the modules. It refuses:
- a request whose actor the store does not hold;
- a transition the type does not declare, that is `only_via` others, or that is taken from a state it does not leave;
- a refusal on a guard the transition does not declare, with a remedy other than the declared one, or on a clause that is not the first to fail;
- an applied request whose guards do not hold, or that breaks an invariant;
- a recording its subject's kinds do not include, or one its generated guards refuse;
- a published version the flow checker refuses, checked against the version before it.

The script evaluates guards, effects and invariants from a transcription it holds, and checks each against the module's text before it runs. It is not an engine, and it refuses a history that reaches anything it has not transcribed. Its self-test plants twenty-two mistakes, from a wrong value to a completion that creates no warranty, and shows each caught.

**What is inferred.** The use cases speak of the first consumer's deliveries, and the reviewed flow is thinner in two places: its delivery has no product configuration and no checklist. §6 lists each inference with what it rests on. Two questions the specification left open were decided to write the values (§7, ADR-0123). UC-4 to UC-6 need inspection and availability datapoints the journey lacks, so the history's second part publishes a version that adds them (§8), and §12 lists what they inferred and found. UC-7 to UC-9 add a telemetry summary, three formulas and legacy history, in a third part (§13), and §17 lists theirs. UC-10 to UC-12 add a rule on a metric, an agent's escalation and the business exceptions, in a fourth (§18), and §22 lists theirs. UC-13 to UC-15 install the returns module's versions, put a rule on trial and enforce it by a governed change, in a fifth and a sixth (§23, §27), and §28 lists theirs. UC-16 to UC-18 split by the kind of actor, hand service jobs between engineers and erase a customer, in a seventh part (§29) and an eighth and a ninth (§32), and §33 lists theirs. UC-19 to UC-21 put a second rule on trial, install the operations review's declared proposals and create each unit's warranty in its sale, in a tenth, an eleventh and a twelfth part (§34, §36, §38), and §39 lists theirs.

## 1. The store

The store holds the modules `operations_shared` and `inventory_journey` of `unit-journey.md` §2. The agent Scout is an `Agent`, the type `operations_shared` declares for agents. *(Corrected 2026-09-27: this document first declared the type in a module of its own, `scenario_agents`, since the journey declared none and no agent could act on the store (PRD T6; `DESIGN.md` §6 step 1). At the author's direction the type moved into `operations_shared` (D445).)*

**The port.** At `2026-09-01T00:00Z` the store imported the legacy system's objects while every type was a mirror, and the publish of the journey cut them over (`publish-and-import.md` §7). `Customer` stays a mirror, which only the import writes. The history below begins after the cutover. The import's events are marked `imported` and keep provenance `asserted`, and each imported object's `state_source` is `imported` (`DESIGN.md` §8, §11).

**Reading.** Every read is taken at `now`, `2026-09-28T00:00Z`, a Monday. Weeks are ISO 8601 weeks in UTC: 2026-W37 runs from Monday 7 September, W38 from the 14th and W39 from the 21st (`declaration-syntax.md` §6.9). A read names the dimensions it keeps and aggregates over the rest, `version` and `actor_kind` included (`declaration-syntax.md` §6.9, "A reader reads a metric as a guard does"). The ids are labels, and no value below depends on their order.

## 2. The history

Two people and one agent act on the store:
- **Ana (`U-ANA`) and Ben (`U-BEN`)** are users ported from the legacy system.
- **The agent Scout (`A-SCOUT`)** is issued by Ana on the first day.

Eight deliveries matter here:
- **Three were ported.** D01 had been in preparation since 20 August, with its history whole. D07 came with no history. D08 had been in preparation since 15 August, with the legacy record silent from 25 to 28 August.
- **Five were opened after the port**, D02 to D06.

Ana and Ben bind units to deliveries and complete them. Scout opens D05 and keeps requesting its completion while its one unit has not arrived, as it did on D02 before. A new supplier's model, Kestrel K2, requires a label photo its parts do not carry, so Ana puts three of its units on offer by override. On 24 September the operations lead publishes a metric splitting time in state by customer and configuration (§3.5).

```yaml scenario
now: 2026-09-28T00:00Z
port: 2026-09-01T00:00Z

imported:
  - { object: U-ANA, type: User, state: ACTIVE, attributes: { login: ana } }
  - { object: U-BEN, type: User, state: ACTIVE, attributes: { login: ben } }
  - { object: U-CHEN, type: User, state: ACTIVE, attributes: { login: chen } }
  - { object: C-ACME, type: Customer, state: ACTIVE, attributes: { legacy_key: "C-1001", name: Acme Robotics } }
  - { object: C-BOREAL, type: Customer, state: ACTIVE, attributes: { legacy_key: "C-1002", name: Boreal Logistics } }
  - { object: M-ARM, type: RobotModel, state: ACTIVE, attributes: { name: Arm 7, manufacturer: Armtek, warranty_months: 24 } }
  - { object: M-ROVER, type: RobotModel, state: ACTIVE, attributes: { name: Rover 2, manufacturer: Roverline, warranty_months: 24 } }
  - { object: R01, type: Robot, state: AVAILABLE, attributes: { model: M-ARM, label_printed_at: 2026-08-03T00:00Z } }
  - { object: R02, type: Robot, state: AVAILABLE, attributes: { model: M-ARM, label_printed_at: 2026-08-03T00:00Z } }
  - { object: R03, type: Robot, state: AVAILABLE, attributes: { model: M-ARM, label_printed_at: 2026-08-03T00:00Z } }
  - { object: R04, type: Robot, state: INTAKE, attributes: { model: M-ROVER, label_printed_at: 2026-08-27T00:00Z } }
  - { object: D01, type: Delivery, state: PREPARATION, created: 2026-08-20T00:00Z, entered: 2026-08-20T00:00Z, entered_by: human, attributes: { customer: C-ACME } }
  - { object: D07, type: Delivery, state: PREPARATION, attributes: { customer: C-BOREAL } }
  - { object: D08, type: Delivery, state: PREPARATION, entered: 2026-08-15T00:00Z, entered_by: human, silent: [2026-08-25T00:00Z, 2026-08-28T00:00Z], attributes: { customer: C-ACME } }
  - { object: L01, type: Robot, state: AVAILABLE, created: 2025-10-06T00:00Z, entered: 2025-11-20T00:00Z, entered_by: human, legacy: [[REQUESTED, 2025-10-06T00:00Z, 2025-11-17T00:00Z], [INTAKE, 2025-11-17T00:00Z, 2025-11-20T00:00Z]], attributes: { model: M-ARM, label_printed_at: 2025-11-18T00:00Z } }
  - { object: L02, type: Robot, state: SOLD, created: 2026-01-12T00:00Z, entered: 2026-03-02T00:00Z, entered_by: human, legacy: [[REQUESTED, 2026-01-12T00:00Z, 2026-02-09T00:00Z], [INTAKE, 2026-02-09T00:00Z, 2026-02-11T00:00Z], [AVAILABLE, 2026-02-11T00:00Z, 2026-03-02T00:00Z]], attributes: { model: M-ARM, label_printed_at: 2026-02-10T00:00Z, sold_to: C-BOREAL } }
  - { object: L03, type: Robot, state: AVAILABLE, created: 2026-03-02T00:00Z, entered: 2026-04-29T00:00Z, entered_by: human, legacy: [[REQUESTED, 2026-03-02T00:00Z, 2026-04-27T00:00Z], [INTAKE, 2026-04-27T00:00Z, 2026-04-29T00:00Z]], attributes: { model: M-ROVER, label_printed_at: 2026-04-28T00:00Z } }
  - { object: L04, type: Robot, state: AVAILABLE, created: 2026-06-01T00:00Z, entered: 2026-07-08T00:00Z, entered_by: human, legacy: [[REQUESTED, 2026-06-01T00:00Z, 2026-07-06T00:00Z], [INTAKE, 2026-07-06T00:00Z, 2026-07-08T00:00Z]], attributes: { model: M-ROVER, label_printed_at: 2026-07-07T00:00Z } }
  - { object: L05, type: Robot, state: AVAILABLE, entered: 2026-05-10T00:00Z, entered_by: human, legacy: [[REQUESTED, 2026-04-06T00:00Z, 2026-05-04T00:00Z], [INTAKE, 2026-05-04T00:00Z, 2026-05-10T00:00Z]], attributes: { model: M-ARM, label_printed_at: 2026-05-05T00:00Z } }
  - { object: L06, type: Robot, state: SOLD, created: 2025-08-04T00:00Z, entered: 2025-10-01T00:00Z, entered_by: human, legacy: [[REQUESTED, 2025-08-04T00:00Z, 2025-09-10T00:00Z], [INTAKE, 2025-09-10T00:00Z, 2025-09-12T00:00Z], [AVAILABLE, 2025-09-12T00:00Z, 2025-10-01T00:00Z]], attributes: { model: M-ROVER, label_printed_at: 2025-09-11T00:00Z, sold_to: C-ACME } }
  - { object: L07, type: Robot, state: REQUESTED, created: 2026-07-20T00:00Z, entered: 2026-07-20T00:00Z, entered_by: human, attributes: { model: M-ARM } }
  - { object: E01, type: Engagement, state: OUT, entered: 2026-08-25T00:00Z, entered_by: human, attributes: { kind: LEASE, expected_return: 2026-11-10T00:00Z } }
  - { object: SJ1, type: ServiceJob, state: OPEN, created: 2026-08-24T00:00Z, created_by: human, entered: 2026-08-24T00:00Z, entered_by: human, legacy_members: { engineer: [[null, 2026-08-24T00:00Z, 2026-08-26T00:00Z], [U-BEN, 2026-08-26T00:00Z, 2026-08-29T00:00Z]] }, member_entered: { engineer: 2026-08-29T00:00Z }, attributes: { kind: REPAIR, robot: L06, customer: C-ACME, engineer: U-CHEN } }

requests:
  - { at: 2026-09-01T01:00Z, actor: U-ANA, create: Agent, object: A-SCOUT, transition: issue, inputs: { key_id: scout, name: Scout } }
  - { at: 2026-09-01T02:00Z, actor: U-ANA, create: Robot, object: R05, transition: add_opening_stock, inputs: { model: M-ARM, label_printed_at: 2026-09-01T02:00Z } }
  - { at: 2026-09-01T02:00Z, actor: U-ANA, create: Robot, object: R06, transition: add_opening_stock, inputs: { model: M-ARM, label_printed_at: 2026-09-01T02:00Z } }
  - { at: 2026-09-01T02:00Z, actor: U-ANA, create: Robot, object: R07, transition: add_opening_stock, inputs: { model: M-ROVER, label_printed_at: 2026-09-01T02:00Z } }
  - { at: 2026-09-01T02:00Z, actor: U-ANA, create: Robot, object: R08, transition: add_opening_stock, inputs: { model: M-ROVER, label_printed_at: 2026-09-01T02:00Z } }
  - { at: 2026-09-01T02:00Z, actor: U-ANA, create: Robot, object: R09, transition: add_opening_stock, inputs: { model: M-ARM, label_printed_at: 2026-09-01T02:00Z } }
  - { at: 2026-09-02T00:00Z, actor: U-ANA, object: D01, transition: bind_slot, inputs: { robot: R05 } }
  - { at: 2026-09-04T00:00Z, actor: U-ANA, object: D01, transition: complete_sale }
  - { at: 2026-09-05T00:00Z, actor: U-ANA, create: RobotModel, object: M-KESTREL, transition: add, inputs: { name: Kestrel K2, manufacturer: Kestrel Dynamics, warranty_months: 12, label_photo_required: true } }
  - { at: 2026-09-07T00:00Z, actor: U-BEN, create: Delivery, object: D02, transition: open, inputs: { customer: C-BOREAL } }
  - { at: 2026-09-07T05:00Z, actor: U-BEN, create: Robot, object: R10, transition: add_to_intake, inputs: { model: M-ROVER } }
  - { at: 2026-09-07T06:00Z, actor: U-BEN, object: D02, transition: peg_slot, inputs: { robot: R10 } }
  - { at: 2026-09-07T07:00Z, actor: U-BEN, object: D02, transition: bind_slot, inputs: { robot: R07 } }
  - { at: 2026-09-08T00:00Z, actor: U-ANA, create: Robot, object: R20, transition: add_to_intake, inputs: { model: M-KESTREL } }
  - { at: 2026-09-08T00:00Z, actor: U-ANA, create: Robot, object: R21, transition: add_to_intake, inputs: { model: M-KESTREL } }
  - { at: 2026-09-08T00:00Z, actor: U-ANA, create: Robot, object: R22, transition: add_to_intake, inputs: { model: M-KESTREL } }
  - { at: 2026-09-08T09:00Z, actor: A-SCOUT, object: D02, transition: complete_sale, refused: { verdict: unsatisfied, clause: filled, remedy: dependent } }
  - { at: 2026-09-08T10:00Z, actor: U-ANA, object: R20, transition: record_label_print }
  - { at: 2026-09-08T10:00Z, actor: U-ANA, object: R21, transition: record_label_print }
  - { at: 2026-09-08T10:00Z, actor: U-ANA, object: R22, transition: record_label_print }
  - { at: 2026-09-08T11:00Z, actor: U-ANA, object: R20, transition: inventorize, refused: { verdict: unsatisfied, clause: photo, remedy: dependent } }
  - { at: 2026-09-08T11:00Z, actor: U-ANA, object: R21, transition: inventorize, refused: { verdict: unsatisfied, clause: photo, remedy: dependent } }
  - { at: 2026-09-08T11:00Z, actor: U-ANA, object: R22, transition: inventorize, refused: { verdict: unsatisfied, clause: photo, remedy: dependent } }
  - { at: 2026-09-09T09:00Z, actor: A-SCOUT, object: D02, transition: complete_sale, refused: { verdict: unsatisfied, clause: filled, remedy: dependent } }
  - { at: 2026-09-09T12:00Z, actor: U-ANA, object: R20, transition: correct_state, inputs: { to: AVAILABLE, reason: SUPPLIER_EXCEPTION } }
  - { at: 2026-09-09T20:00Z, actor: U-BEN, object: R10, transition: record_label_print }
  - { at: 2026-09-10T00:00Z, actor: U-BEN, object: R10, transition: inventorize }
  - { at: 2026-09-10T01:00Z, actor: U-BEN, object: D02, transition: bind_slot, inputs: { robot: R10 } }
  - { at: 2026-09-10T06:00Z, actor: U-ANA, object: R21, transition: correct_state, inputs: { to: AVAILABLE, reason: SUPPLIER_EXCEPTION } }
  - { at: 2026-09-10T12:00Z, actor: U-BEN, object: D02, transition: complete_sale }
  - { at: 2026-09-11T00:00Z, actor: U-BEN, object: D02, transition: complete_sale, refused: { verdict: unavailable, remedy: unreachable_from_here } }
  - { at: 2026-09-14T00:00Z, actor: U-ANA, create: Delivery, object: D03, transition: open, inputs: { customer: C-ACME } }
  - { at: 2026-09-14T01:00Z, actor: U-ANA, object: D03, transition: bind_slot, inputs: { robot: R06 } }
  - { at: 2026-09-15T00:00Z, actor: U-ANA, object: D03, transition: complete_sale }
  - { at: 2026-09-15T02:00Z, actor: U-BEN, create: Robot, object: R23, transition: add_to_intake, inputs: { model: M-ROVER } }
  - { at: 2026-09-15T03:00Z, actor: U-ANA, object: R04, transition: correct_state, inputs: { to: AVAILABLE, reason: SUPPLIER_EXCEPTION } }
  - { at: 2026-09-16T00:00Z, actor: U-BEN, create: Delivery, object: D04, transition: open, inputs: { customer: C-BOREAL, internal: true } }
  - { at: 2026-09-16T01:00Z, actor: U-ANA, object: R22, transition: correct_state, inputs: { to: AVAILABLE, reason: SUPPLIER_EXCEPTION } }
  - { at: 2026-09-16T02:00Z, actor: U-BEN, object: D04, transition: bind_slot, inputs: { robot: R08 } }
  - { at: 2026-09-16T03:00Z, actor: U-BEN, object: R23, transition: record_label_print }
  - { at: 2026-09-17T00:00Z, actor: U-BEN, object: D04, transition: complete_sale, refused: { verdict: unsatisfied, clause: not_internal, remedy: unreachable_from_here } }
  - { at: 2026-09-17T01:00Z, actor: U-BEN, object: D04, transition: complete_internal }
  - { at: 2026-09-17T02:00Z, actor: U-BEN, object: R23, transition: correct_state, inputs: { to: AVAILABLE, reason: MIS_SCANNED } }
  - { at: 2026-09-19T00:00Z, actor: U-BEN, create: Robot, object: R11, transition: add_to_intake, inputs: { model: M-ROVER } }
  - { at: 2026-09-20T00:00Z, actor: A-SCOUT, create: Delivery, object: D05, transition: open, inputs: { customer: C-ACME } }
  - { at: 2026-09-20T01:00Z, actor: A-SCOUT, object: D05, transition: peg_slot, inputs: { robot: R11 } }
  - { at: 2026-09-20T12:00Z, actor: A-SCOUT, object: D05, transition: complete_sale, refused: { verdict: unsatisfied, clause: filled, remedy: dependent } }
  - { at: 2026-09-21T00:00Z, actor: U-ANA, object: D03, transition: revoke }
  - { at: 2026-09-21T12:00Z, actor: A-SCOUT, object: D05, transition: complete_sale, refused: { verdict: unsatisfied, clause: filled, remedy: dependent } }
  - { at: 2026-09-22T00:00Z, actor: A-SCOUT, object: R09, transition: correct_state, inputs: { to: DEVELOPMENT, reason: LEGACY_DATA } }
  - { at: 2026-09-22T12:00Z, actor: A-SCOUT, object: D05, transition: complete_sale, refused: { verdict: unsatisfied, clause: filled, remedy: dependent } }
  - { at: 2026-09-23T12:00Z, actor: A-SCOUT, object: D05, transition: complete_sale, refused: { verdict: unsatisfied, clause: filled, remedy: dependent } }
  - { at: 2026-09-25T00:00Z, actor: U-BEN, create: Delivery, object: D06, transition: open, inputs: { customer: C-BOREAL } }
  - { at: 2026-09-26T00:00Z, actor: U-BEN, object: D06, transition: bind_slot, inputs: { robot: R21 }, refused: { verdict: stale, remedy: self_serviceable } }
  - { at: 2026-09-26T01:00Z, actor: U-BEN, object: D06, transition: bind_slot, inputs: { robot: R21 } }
```

## 3. UC-1: where do deliveries get stuck?

> An operations lead asks which delivery states hold work longest, by customer and by product configuration, and which open deliveries are oldest in their current state. *Acceptance:* time in each state, current age and work in progress are available for every flow from the day it is published, with nothing declared; for imported objects, from as far back as their ported history allows; and a split by customer or configuration, once declared, covers the history recorded before it. (PRD §7)

### 3.1 The spans

Every value in this section is a function of the delivery's spans, one per stay in a state (`declaration-syntax.md` §6.9, source `<Type>.intervals`):
- **A span that has ended** lasts from its entry to its exit.
- **A current span** runs to `now` while its object is open.
- **On an object now in a closed state**, the current span, in that state, has no duration, so nothing accrues on a finished delivery.
- **A ported object's current span** begins when the mapping says the object entered its state, and at the port where it says nothing (`publish-and-import.md` §4, "Intervals").

| Delivery | Customer | Units at `now` (model) | Spans, and their durations | `recorded_from` |
|---|---|---|---|---|
| D01, ported whole | C-ACME | R05 (M-ARM), sold | PREPARATION 20 Aug → 4 Sep: **15d**; DELIVERED since 4 Sep: none, closed | 20 Aug, its legacy creation |
| D02 | C-BOREAL | R07, R10 (both M-ROVER), sold | PREPARATION 7 Sep 00:00 → 10 Sep 12:00: **3d 12h**; DELIVERED: none | 7 Sep |
| D03 | C-ACME | none: the revoke unsold R06 and cleared its binding | PREPARATION 14 → 15 Sep: **1d**; DELIVERED 15 → 21 Sep: **6d**; CANCELLED: none | 14 Sep |
| D04, internal | C-BOREAL | R08 (M-ROVER), in DEVELOPMENT | PREPARATION 16 Sep 00:00 → 17 Sep 01:00: **1d 1h**; DELIVERED: none | 16 Sep |
| D05, opened by Scout | C-ACME | none; R11 only pegged | PREPARATION since 20 Sep, open: **8d** | 20 Sep |
| D06 | C-BOREAL | R21 (M-KESTREL), reserved | PREPARATION since 25 Sep, open: **3d** | 25 Sep |
| D07, ported with no history | C-BOREAL | none | PREPARATION since the port, open: **27d** | the port |
| D08, ported with a silent span | C-ACME | none | PREPARATION since 15 Aug, open: **44d**, from before its record vouches | 28 Aug, the end of its silent span |

D03's DELIVERED span counts because D03 left DELIVERED, when the revoke cancelled it: a closed state holds no *current* time, but a stay that ended is a span like any other. D08's span began before its `recorded_from`, so every read over it names D08 among its `gaps` (`declaration-syntax.md` §6.9). D07's began at the port, where its record begins, so it has no gap: its wait is counted from the port, as far back as its ported history allows.

### 3.2 Time in each state, with nothing declared

Read: `metric(Delivery.time_in_state, keep: [state])`

| state | value | gaps |
|---|---|---|
| PREPARATION | 3d 12h | D08 |
| DELIVERED | 6d | |
| CANCELLED | absent | |

`time_in_state` is the median span per state. Preparation's eight spans in ascending order are 1d, 1d 1h, 3d, 3d 12h, 8d, 15d, 27d and 44d. The median is nearest-rank, the smallest value whose rank is at least half the count, which is the fourth: 3d 12h. Delivered has four spans and one duration, D03's 6d, so its median is 6d. The three current spans on delivered deliveries have none and are left out, as an aggregate leaves out a row whose body is absent. Cancelled has one span and no duration, so its value is absent over rows that exist (§7.2).

### 3.3 Current age and work in progress

Read: `metric(Delivery.oldest_open, keep: [state])`

| state | value | gaps |
|---|---|---|
| PREPARATION | 44d | D08 |

Read: `metric(Delivery.work_in_progress, keep: [state])`

| state | value | gaps |
|---|---|---|
| PREPARATION | 4 | |

Four deliveries are open, D05 to D08, all in preparation. `oldest_open` is `max(now - o.entered_at(state))`: D08's 44d, reported with its gap. `work_in_progress` reads only each open object's current state, which the record vouches for, so it has no gap.

### 3.4 Which open deliveries are oldest

`oldest_open` gives the age and not the delivery. The read surface's `query` lists objects, filtered on state and category and ordered by the entry time of a tracked value's current interval (`DESIGN.md` §10), so the lead lists open deliveries by the time they entered their current state. The read surface gives no grammar for `order`; the one written here is this document's (§7.3).

Read: `query(Delivery, filter: "state.category != closed", order: "entered_at(state)")`

| object | state | entered | gaps |
|---|---|---|---|
| D08 | PREPARATION | 2026-08-15T00:00Z | D08 |
| D07 | PREPARATION | 2026-09-01T00:00Z | |
| D05 | PREPARATION | 2026-09-20T00:00Z | |
| D06 | PREPARATION | 2026-09-25T00:00Z | |

### 3.5 The lead's split, declared on 24 September

On 24 September the lead publishes one metric on `Delivery`. A metric is declared under the type whose rows it reads (`flow-format.md` §4.9), so this is a new version of the journey's `Delivery`. A published version is written as the time it takes effect and what it adds, each at its path in the module. The script applies it at that time, and runs all four steps of the flow checker over the new version against the one before.

```yaml publish
at: 2026-09-24T00:00Z
add:
  types.Delivery.metrics.delivery_wait:
    description: The median time a delivery spends in each state, by customer and by the model of the units bound to it.
    source: intervals
    item: i
    dimensions:
      state: i.state
      customer: i.object.customer
      configuration: i.object.units.model
    expression: median(i.duration)
```

`configuration` is the model of the delivery's units, a path through the set `units` and one hop more, two hops from the row's object (§6.1). A metric is computed on read over every row its source selects, so the split covers every span recorded before it was declared, D01's and D08's ported ones included (`declaration-syntax.md` §6.9; PRD C4). A path reads the object's values at `now`: only `.held` reads a member as it was when the span began. So D03, whose revoke unbound its unit, has no configuration today, and its spans count under the absent value (§7.4).

Read: `metric(delivery_wait, keep: [state, customer, configuration])`

| state | customer | configuration | value | gaps |
|---|---|---|---|---|
| PREPARATION | C-ACME | M-ARM | 15d | |
| PREPARATION | C-ACME | absent | 8d | D08 |
| PREPARATION | C-BOREAL | M-ROVER | 1d 1h | |
| PREPARATION | C-BOREAL | M-KESTREL | 3d | |
| PREPARATION | C-BOREAL | absent | 27d | |
| DELIVERED | C-ACME | M-ARM | absent | |
| DELIVERED | C-ACME | absent | 6d | |
| DELIVERED | C-BOREAL | M-ROVER | absent | |
| CANCELLED | C-ACME | absent | absent | |

The groups, each a median:
- **C-ACME, absent:** D03's 1d, D05's 8d and D08's 44d. The median of three is the second, 8d, and D08's gap is named.
- **C-BOREAL, M-ROVER:** D02 and D04. D02 has two Rover units, and its span counts once under M-ROVER, since a row is counted once under each distinct value its path reaches (§7.1). The median of 1d 1h and 3d 12h is the lower, 1d 1h. Counted once per unit, D02 would appear twice and the value would be 3d 12h.
- **The delivered and cancelled groups** have rows whose spans hold no current time, so their value is absent, except D03's ended stay in DELIVERED.

## 4. UC-2: which rule is in the way?

> Deliveries are often refused at completion. The lead asks which rule refuses most, with which remedy, and whether the callers are people or agents — finding, for instance, an agent that keeps requesting completion before the checklist is done. *Acceptance:* every refusal is counted per rule, remedy and kind of actor, per week. (PRD §7)

The journey's delivery has no checklist. What stands in for one is `filled`: a delivery completes only once a unit is bound and no unit is still only pegged, one that has not arrived (§6.2). Scout requested D02's completion twice while R10 was still in intake, and D05's four times while R11 was. Every refusal is counted from the attempt log's daily counts, which are kept after its rows are pruned (`declaration-syntax.md` §6.9, `<Type>.attempt_counts`). The `refusals` metric counts every enforced refusal, whatever its verdict. A verdict with no clause, such as `unavailable` or `stale`, is counted under the absent clause with its fixed remedy (`DESIGN.md` §5.5).

Read: `metric(Delivery.refusals, keep: [transition, clause, remedy, actor_kind, week])`

| transition | clause | remedy | actor_kind | week | value | gaps |
|---|---|---|---|---|---|---|
| complete_sale | filled | dependent | agent | 2026-W37 | 2 | |
| complete_sale | absent | unreachable_from_here | human | 2026-W37 | 1 | |
| complete_sale | not_internal | unreachable_from_here | human | 2026-W38 | 1 | |
| complete_sale | filled | dependent | agent | 2026-W38 | 1 | |
| complete_sale | filled | dependent | agent | 2026-W39 | 3 | |
| bind_slot | absent | self_serviceable | human | 2026-W39 | 1 | |

Where each count comes from:
- **Scout's refusals by `filled`:** two in W37, from 8 and 9 September. One in W38, from Sunday 20 September. Three in W39, from 21, 22 and 23 September.
- **Ben's other refusals:**
  - an `unavailable` refusal when Ben asked to complete D02 after it was delivered (W37);
  - `not_internal` when Ben asked to sell D04, an internal delivery (W38);
  - a `stale` refusal when D06's binding request carried an old version (W39).

Read over every week, the answer to "which rule refuses most" is `filled`, and all of it is the agent's:

Read: `metric(Delivery.refusals, keep: [clause, remedy, actor_kind])`

| clause | remedy | actor_kind | value | gaps |
|---|---|---|---|---|
| filled | dependent | agent | 6 | |
| absent | unreachable_from_here | human | 1 | |
| not_internal | unreachable_from_here | human | 1 | |
| absent | self_serviceable | human | 1 | |

## 5. UC-3: overrides as a symptom

> Units are often put into `AVAILABLE` by override rather than through `INTAKE`, because, say, the label-and-photo rule does not fit one supplier's parts. *Acceptance:* override counts per target state and per reason, excluding the objects placed there by the data import, and each override reviewable. (PRD §7)

Kestrel K2 requires a label photo, and its units arrive without one. Ana's three requests to inventorize them were refused by `photo`, and Ana put each on offer by `correct_state`, with the reason `SUPPLIER_EXCEPTION`, and did the same for R04, a ported unit left in intake. Ben corrected R23, mis-scanned at intake, and Scout moved R09 into the development pool, the legacy record having had it wrong. The import placed R01 to R03 in `AVAILABLE`, and those events are marked `imported`, so the counts leave them out.

Read: `metric(Robot.override_counts, keep: [state, reason])`

| state | reason | value | gaps |
|---|---|---|---|
| AVAILABLE | SUPPLIER_EXCEPTION | 4 | |
| AVAILABLE | MIS_SCANNED | 1 | |
| DEVELOPMENT | LEGACY_DATA | 1 | |

Read: `metric(Robot.override_counts, keep: [state, reason, week])`

| state | reason | week | value | gaps |
|---|---|---|---|---|
| AVAILABLE | SUPPLIER_EXCEPTION | 2026-W37 | 2 | |
| AVAILABLE | SUPPLIER_EXCEPTION | 2026-W38 | 2 | |
| AVAILABLE | MIS_SCANNED | 2026-W38 | 1 | |
| DEVELOPMENT | LEGACY_DATA | 2026-W39 | 1 | |

R04's override is counted although the import placed R04: the import placed it in `INTAKE`, and Ana's override, after the cutover, is not the import's. The acceptance excludes what the import placed, and the count excludes the import's events. Here the two agree.

**Each override is reviewable.** `exceptions(Robot)` lists every unit whose current state an assertion set and nothing has moved since (`storage-schema.md` §5). R21 is not listed: its binding to D06 reserved it, an ordinary transition that changed its state, which returns it to the machine. The ported units are not listed, since the import's state is `imported`. The read surface does not order the page, so the script compares it as a set (§7.3).

Read: `exceptions(Robot)`

| object | state |
|---|---|
| R04 | AVAILABLE |
| R09 | DEVELOPMENT |
| R20 | AVAILABLE |
| R22 | AVAILABLE |
| R23 | AVAILABLE |

The rule in the way is visible beside the overrides. Every refusal of `inventorize` in this history was `photo`, on the supplier's three units:

Read: `metric(Robot.refusals, keep: [transition, clause, remedy])`

| transition | clause | remedy | value | gaps |
|---|---|---|---|---|
| inventorize | photo | dependent | 3 | |

## 6. What is inferred

The author allowed inference "based on clear requirements we've already discussed and the use case we've reviewed". Each inference below rests on one of those, and none decides anything the specification had settled.

1. **A product configuration is the model of the delivery's units** (UC-1). The journey's delivery has no configuration. What it delivers is units, and a unit's kind is its `RobotModel`, which the journey uses as the product: its flags hold what a unit carries, and its units are counted by it. The model's own example of a set-valued dimension is "a delivery's configurations" (`declaration-syntax.md` §6.9). Here the set is the delivery's units, and the dimension reaches one hop past it.
2. **The checklist not done is a unit not yet arrived** (UC-2). The journey's delivery has no checklist, and its completion waits on `filled`, whose failure means a pegged unit has not arrived. It is the same shape of refusal: a completion requested before the delivery's parts are ready, with the remedy `dependent`.
3. **An agent type**, which the journey lacked (§1). It rests on PRD T6, that every actor's kind is declared with the type that holds it, and on `declaration-syntax.md` §6.10's `Agent`, copied as it is. At the author's direction it is now in `operations_shared` (D445).
4. **The history's shape.** The port, the eight deliveries and the supplier are this document's. Each was chosen to put a rule under load:
   - an ended stay in a closed state;
   - an open stay, and a ported one;
   - a silent span;
   - a revoke that clears a binding;
   - a refusal on each side of a week boundary;
   - a ported unit overridden after the cutover;
   - an override the machine later undoes.

## 7. What the scenarios found

Writing the values found two questions the specification did not settle, and each value above depends on its answer. Both are decided by ADR-0123. The other two points below are settled, but a builder might miss them.

1. **A set-valued dimension counts a row once under each distinct value** (ADR-0123, D443). ADR-0098 and `declaration-syntax.md` §6.9 said "once under each member". That is the same thing only where the path ends at the set, as a delivery's configurations would. Where it reaches past the set, as `units.model` does, two units of one model would count D02's span twice in one group and move the median (§3.5).
2. **A group whose rows all lack a body is reported, with an absent value** (ADR-0123, D444). Nothing said whether a group appears when every row in it is present and its body absent, as the delivered deliveries' current spans are. Reporting it keeps "no delivery sits here" apart from "deliveries sit here and accrue nothing" (§3.2).
3. **A read's order and `query`'s grammar are unstated.** `exceptions` and a metric page have no stated order, and `query` has no grammar for `order`. The script compares unordered reads as sets, and orders `query` by entry time as its argument says. No value here depends on either. `TODO.md` holds the question.
4. **A split through a relationship the flow clears reads today's value.** D03's revoke unbound its unit, so its past spans lost their configuration. `.held` reads a member as it was when a span began, but only a tracked member, a single stored end, and `units` is a set read from its opposite ends. This is determined, and it may not be what a lead wants. `TODO.md` holds the question.

## 8. The second fortnight: a version adds inspection

The history continues from `now` of §2, and the reads below are taken at the new `now`, `2026-10-12T00:00Z`, a Monday. W40 runs from 28 September, W41 from 5 October. The reads of §3 to §5 were taken at the first `now`, before any of this happened, and hold as written.

On 1 October the operations lead publishes version 3 of the journey. It adds three things, each needed by a use case and each written the way the model writes it (`declaration-syntax.md` §6.8):
- **A checklist per model**, `PdiCheck`: a catalogue whose objects are the checks, so a checklist changes by creating and retiring checks, not by publishing (PRD D7).
- **Two kinds of datapoint on the delivery.** `pdi_results` holds a pre-delivery check's result for one unit (UC-4), and `availability_checks` holds what was in stock and what must be procured for one model when the delivery was committed (UC-5). Both are the delivery's, since the model advises declaring a kind "on the object whose gate reads it".
- **A gate on each completion**, `all_checked`: every unit bound has a result for every active check of its model, and a metric, `open_shortfall`, over the availability checks.

```yaml publish
at: 2026-10-01T00:00Z
add:
  enumerations.CheckOutcome: [PASS, FAIL, NOT_APPLICABLE]
  types.PdiCheck:
    description: One check of a model's pre-delivery inspection; a model's checklist is the active checks that name it, kept as data.
    tracking: record
    attributes:
      title: { type: string }
      model: { reference: RobotModel }
    states:
      ACTIVE: { category: live }
      RETIRED: { category: closed, final: true }
    transitions:
      add:
        kind: initial
        to: ACTIVE
        required_inputs: [title, model]
      retire:
        kind: external
        from: ACTIVE
        to: RETIRED
  types.Delivery.observations.pdi_results:
    kind: PdiResult
    description: One result of a pre-delivery check on one unit of the delivery.
    attributes:
      unit: { reference: Robot }
      check: { reference: PdiCheck }
      outcome: { type: CheckOutcome }
      reading: { type: "decimal(10,3)", optional: true, unit: V }
      photo: { type: file, optional: true }
      remark: { type: string, optional: true, personal: true }
    max_recording_delay: 3 days
  types.Delivery.observations.availability_checks:
    kind: AvailabilityCheck
    description: What was in stock and what must be procured for one model, when the delivery was committed.
    attributes:
      model: { reference: RobotModel }
      in_stock: { type: int }
      shortfall: { type: int }
  types.Delivery.conditions.all_checked:
    description: Every unit bound has a result for every active check of its model's checklist.
    expression: >-
      all(u in units: all(c in PdiCheck where c.model == u.model and c.state == PdiCheck.ACTIVE:
          any(r in pdi_results where r.unit == u and r.check == c)))
    remedy: unreachable_from_here
  types.Delivery.transitions.complete_sale.guards.all_checked: deny
  types.Delivery.transitions.complete_internal.guards.all_checked: deny
  types.Delivery.metrics.open_shortfall:
    description: The units short, by model, when each delivery still open was committed, by the week its availability was checked.
    source: availability_checks
    item: r
    filter: r.subject.open
    dimensions:
      model: r.model
      week: week(r.occurred_at)
    time_dimension: r.occurred_at
    expression: sum(r.shortfall)
```

`all_checked`'s remedy is `unreachable_from_here`: a missing result is written by a recording, another request, and a completion takes no input that could supply it (`flow-format.md` §4.7, ADR-0122 decision 30). The model's own example of this gate said `self_serviceable` until writing this scenario corrected it (§12).

**What happens.**
- **Checklists:** Ana writes the checklists: battery voltage (K1) and arm calibration (K2) for the Arm 7, and a drive test (K3) for the Rover 2.
- **D09 and UC-5:**
  - Ben opens D09 for Acme on 2 October, and Scout records the availability check: one Arm 7 in stock, one short.
  - Ben opens D10 for Boreal on the 3rd, and Scout records two Rover 2s short. It then corrects that to one in stock and one short, the first entry having been a mistake.
- **D09 and UC-4:**
  - Ben binds R01 and R02 to D09, and Scout asks to complete it before any result is recorded.
  - Ben records the results:
    - one result mistakenly entered as not applicable, and corrected;
    - a failed battery reading on R02, and a passing one after charging, which is a later result, not a correction;
    - one result recorded two days after the check, dated when the check was done.
  - D09 completes on 7 October.
- **The checklist changes, with no publish:**
  - On 8 October Ana adds a lidar check (K4) to the Rover 2's checklist, and on the 9th retires the arm calibration.
  - D10, whose Rover already has its drive test, is refused completion for the lidar check alone.
  - A lidar result dated four days back is refused.
- **Scout opens D11** on the 9th and records its availability: an Arm 7 in stock, a Rover 2 short.
- **Shipment S1 (UC-6)** left on 1 October with three Rover 2s, R30 to R32. R33 was added to it on the 6th, while it was still in transit. It arrived at 16:00 on the 5th, and Ben recorded the arrival and the receipts the next morning, dated when they happened.

```yaml scenario
now: 2026-10-12T00:00Z

requests:
  - { at: 2026-10-01T01:00Z, actor: U-ANA, create: PdiCheck, object: K1, transition: add, inputs: { title: Battery voltage, model: M-ARM } }
  - { at: 2026-10-01T01:00Z, actor: U-ANA, create: PdiCheck, object: K2, transition: add, inputs: { title: Arm calibration, model: M-ARM } }
  - { at: 2026-10-01T01:00Z, actor: U-ANA, create: PdiCheck, object: K3, transition: add, inputs: { title: Drive test, model: M-ROVER } }
  - { at: 2026-10-01T02:00Z, actor: U-BEN, create: Robot, object: R30, transition: request, inputs: { model: M-ROVER } }
  - { at: 2026-10-01T02:00Z, actor: U-BEN, create: Robot, object: R31, transition: request, inputs: { model: M-ROVER } }
  - { at: 2026-10-01T02:00Z, actor: U-BEN, create: Robot, object: R32, transition: request, inputs: { model: M-ROVER } }
  - { at: 2026-10-01T02:00Z, actor: U-BEN, create: Robot, object: R33, transition: request, inputs: { model: M-ROVER } }
  - { at: 2026-10-01T03:00Z, actor: U-BEN, create: Shipment, object: S1, transition: dispatch, inputs: { with_units: [R30, R31, R32], carrier: Northline } }
  - { at: 2026-10-02T00:00Z, actor: U-BEN, create: Delivery, object: D09, transition: open, inputs: { customer: C-ACME } }
  - { at: 2026-10-02T00:05Z, actor: A-SCOUT, record: availability_checks, subject: D09, object: AC1, fields: { model: M-ARM, in_stock: 1, shortfall: 1 } }
  - { at: 2026-10-03T00:00Z, actor: U-BEN, create: Delivery, object: D10, transition: open, inputs: { customer: C-BOREAL } }
  - { at: 2026-10-03T00:05Z, actor: A-SCOUT, record: availability_checks, subject: D10, object: AC2, fields: { model: M-ROVER, in_stock: 0, shortfall: 2 } }
  - { at: 2026-10-03T08:00Z, actor: A-SCOUT, record: availability_checks, subject: D10, object: AC3, fields: { model: M-ROVER, in_stock: 1, shortfall: 1 }, corrects: AC2 }
  - { at: 2026-10-05T09:00Z, actor: U-BEN, object: D09, transition: bind_slot, inputs: { robot: R01 } }
  - { at: 2026-10-05T09:01Z, actor: U-BEN, object: D09, transition: bind_slot, inputs: { robot: R02 } }
  - { at: 2026-10-06T08:00Z, actor: U-BEN, object: S1, transition: add_unit, inputs: { robot: R33 } }
  - { at: 2026-10-06T09:00Z, actor: U-BEN, object: S1, transition: arrive, occurred_at: 2026-10-05T16:00Z }
  - { at: 2026-10-06T09:05Z, actor: U-BEN, object: S1, transition: receive_unit, inputs: { robot: R30 }, occurred_at: 2026-10-05T16:00Z }
  - { at: 2026-10-06T09:10Z, actor: U-BEN, object: S1, transition: receive_unit, inputs: { robot: R31 } }
  - { at: 2026-10-06T09:20Z, actor: U-BEN, object: S1, transition: receive_unit, inputs: { robot: R33 }, occurred_at: 2026-10-05T16:00Z, refused: { verdict: unsatisfied, clause: occurred_within, remedy: self_serviceable, call: R33 } }
  - { at: 2026-10-06T09:30Z, actor: U-BEN, object: S1, transition: receive_unit, inputs: { robot: R33 } }
  - { at: 2026-10-06T10:00Z, actor: A-SCOUT, object: D09, transition: complete_sale, refused: { verdict: unsatisfied, clause: all_checked, remedy: unreachable_from_here } }
  - { at: 2026-10-06T11:00Z, actor: U-BEN, record: pdi_results, subject: D09, object: P1, fields: { unit: R01, check: K1, outcome: PASS, reading: 48.2 } }
  - { at: 2026-10-06T11:05Z, actor: U-BEN, record: pdi_results, subject: D09, object: P2, fields: { unit: R01, check: K2, outcome: NOT_APPLICABLE } }
  - { at: 2026-10-06T11:10Z, actor: U-BEN, record: pdi_results, subject: D09, object: P3, fields: { unit: R02, check: K1, outcome: FAIL, reading: 31.0 } }
  - { at: 2026-10-06T11:20Z, actor: U-BEN, record: pdi_results, subject: D09, object: P4, fields: { unit: R01, check: K2, outcome: PASS }, corrects: P2 }
  - { at: 2026-10-06T15:00Z, actor: U-BEN, record: pdi_results, subject: D09, object: P5, fields: { unit: R02, check: K1, outcome: PASS, reading: 47.9 } }
  - { at: 2026-10-07T08:00Z, actor: U-BEN, record: pdi_results, subject: D09, object: P6, fields: { unit: R02, check: K2, outcome: PASS }, occurred_at: 2026-10-05T14:00Z }
  - { at: 2026-10-07T09:00Z, actor: U-BEN, object: D09, transition: complete_sale }
  - { at: 2026-10-08T00:00Z, actor: U-ANA, create: PdiCheck, object: K4, transition: add, inputs: { title: Lidar clean, model: M-ROVER } }
  - { at: 2026-10-08T01:00Z, actor: U-BEN, object: D10, transition: bind_slot, inputs: { robot: R23 } }
  - { at: 2026-10-09T00:00Z, actor: U-ANA, object: K2, transition: retire }
  - { at: 2026-10-09T09:00Z, actor: U-BEN, record: pdi_results, subject: D10, object: P7, fields: { unit: R23, check: K3, outcome: PASS } }
  - { at: 2026-10-09T09:30Z, actor: A-SCOUT, create: Delivery, object: D11, transition: open, inputs: { customer: C-ACME } }
  - { at: 2026-10-09T09:35Z, actor: A-SCOUT, record: availability_checks, subject: D11, object: AC4, fields: { model: M-ARM, in_stock: 1, shortfall: 0 } }
  - { at: 2026-10-09T09:35Z, actor: A-SCOUT, record: availability_checks, subject: D11, object: AC5, fields: { model: M-ROVER, in_stock: 0, shortfall: 1 } }
  - { at: 2026-10-09T12:00Z, actor: U-BEN, object: S1, transition: receive_unit, inputs: { robot: R32 }, occurred_at: 2026-10-05T16:00Z, refused: { verdict: unsatisfied, clause: occurred_within, remedy: self_serviceable } }
  - { at: 2026-10-09T12:05Z, actor: U-BEN, object: S1, transition: receive_unit, inputs: { robot: R32 }, occurred_at: 2026-10-08T10:00Z }
  - { at: 2026-10-10T00:00Z, actor: U-BEN, object: D10, transition: complete_sale, refused: { verdict: unsatisfied, clause: all_checked, remedy: unreachable_from_here } }
  - { at: 2026-10-10T01:00Z, actor: U-BEN, record: pdi_results, subject: D10, object: P9, fields: { unit: R23, check: K4, outcome: PASS }, occurred_at: 2026-10-06T00:00Z, refused: { verdict: unsatisfied, clause: occurred_within, remedy: self_serviceable } }
```

## 9. UC-4: record a pre-delivery inspection

> An engineer inspects a configured robot against its configuration's checklist — each check pass, fail or not applicable, with a measured value where the check has one, a photo and a note — and delivery is gated on every check having a result. Operations staff maintain the checklists per configuration. *Acceptance:* results are recorded without a transition per check; the gate reads them; the record of each completed delivery shows which results the gate read; changing a checklist needs no new version of the flow. (PRD §7)

Each acceptance clause, and what shows it:
- **Results are recorded without a transition per check.** Each result is a `record` request on D09's `pdi_results`, a creation of a `PdiResult` that writes nothing on the delivery. The delivery advances no version for it and takes no transition (`declaration-syntax.md` §6.8).
- **The gate reads them.** Scout's request at 10:00 on 6 October was refused by `all_checked`, since no result had been recorded. Ben's on the 7th applied.
- **The record of each completed delivery shows which results the gate read.** The completion's event carries the read set of its guards (`DESIGN.md` §6, "The read set"). Among what `all_checked` read are the checks its scan matched and the results it examined, every result the delivery then held and nothing had corrected.
- **Changing a checklist needs no new version.** K4 was added on 8 October and K2 retired on the 9th, each a transition on the catalogue, with no publish after 1 October. D10 was refused for the lidar check the day after it was added.

Read: `read set of D09.complete_sale: PdiCheck, PdiResult`

| object | kind |
|---|---|
| K1 | PdiCheck |
| K2 | PdiCheck |
| P1 | PdiResult |
| P3 | PdiResult |
| P4 | PdiResult |
| P5 | PdiResult |
| P6 | PdiResult |

The read set contains:
- **K1 and K2**, the Arm 7's two checks, both still active on the 7th, since K2's retirement came two days later.
- **Every uncorrected result**:
  - P1 and P4 for R01. P4 corrects P2, the mistaken "not applicable", and a read sees only what nothing has corrected, so P2 is not read.
  - P3, P5 and P6 for R02. The failed battery reading P3 and the passing one P5 are both read: a later result is a new datapoint, not a correction, and the gate asks only that a result exist (§12).
  - P6, recorded on the 7th for a check done on the 5th, since `max_recording_delay` is 3 days and it was dated 1 day 18 hours back.

The refusals by the gate, and the one refusal of a recording:

Read: `metric(Delivery.refusals, keep: [clause, remedy, actor_kind, week], filter: "a.clause == \"all_checked\"")`

| clause | remedy | actor_kind | week | value | gaps |
|---|---|---|---|---|---|
| all_checked | unreachable_from_here | agent | 2026-W41 | 1 | |
| all_checked | unreachable_from_here | human | 2026-W41 | 1 | |

Read: `metric(PdiResult.refusals, keep: [transition, clause, remedy])`

| transition | clause | remedy | value | gaps |
|---|---|---|---|---|
| record | occurred_within | self_serviceable | 1 | |

A result kind is sugar for a type, so it has the standard metrics too, and its creation, `record`, is refused like any other. The lidar result was dated 4 days and 1 hour back, past the kind's 3 days, and was refused by `occurred_within` before anything else was checked. A refused creation has no object, so it counts under the absent object (`declaration-syntax.md` §6.8, §6.9).

## 10. UC-5: keep what used to be thrown away

> When an order is committed, the availability check's result — what is in stock and what must be procured — is recorded, and the shortfall over time becomes a metric. *Acceptance:* the check's result is a datapoint on the order, and "open shortfall by model" is a declared metric. (PRD §7)

The journey's delivery is its order, and it is committed when it is opened (§12). Scout records the availability check as an `availability_checks` datapoint on each delivery, one per model it needs, and `open_shortfall` sums the shortfall of every delivery still open. Its rows are the observations nothing has corrected, so D10's mistaken first entry, AC2, is not counted, and its correction, AC3, is. D09 completed on the 7th, so its shortfall no longer counts as open.

Read: `metric(open_shortfall, keep: [model])`

| model | value | gaps |
|---|---|---|
| M-ARM | 0 | |
| M-ROVER | 2 | |

Read: `metric(open_shortfall, keep: [model, week])`

| model | week | value | gaps |
|---|---|---|---|
| M-ROVER | 2026-W40 | 1 | |
| M-ARM | 2026-W41 | 0 | |
| M-ROVER | 2026-W41 | 1 | |

The Rover 2's open shortfall is D10's corrected 1, from week 40, and D11's 1, from week 41. The Arm 7's is D11's 0: a model checked and found in stock has a group of its own, with the value 0, rather than no row.

## 11. UC-6: recorded late

> An engineer records a shipment's receipt the morning after it arrived. *Acceptance:* time in `PROCUREMENT` uses when the receipt happened, within a bound the declaration sets, and both times are kept. (PRD §7)

The journey marks `Shipment.arrive` and `receive_unit` backdatable within 2 days, so a request may carry `occurred_at`, the time the change happened, and its called `receive` takes the same time (`flow-format.md` §4.8, ADR-0122 decisions 28 and 31). The requested transition checks the bound. Each transition that changes a state checks that the time does not precede the current stay of what it changes. The unit's stay in `PROCUREMENT` ends at the occurred time.

Read: `metric(Robot.time_in_state, keep: [state], filter: "i.state == Robot.PROCUREMENT")`

| state | value | gaps |
|---|---|---|
| PROCUREMENT | 4d 13h | |

Each unit's stay in procurement, by when it happened:
- **R30:** from its dispatch at 03:00 on 1 October to its receipt at 16:00 on the 5th, 4d 13h. Had it been dated when it was recorded, 09:05 on the 6th, it would have been 17 hours 5 minutes longer.
- **R31:** received without a date, so its stay runs to when the receipt was recorded, 5d 6h 10m.
- **R32:** 7d 7h. Its first receipt was dated back 3 days and 20 hours, past the bound, and refused. Its second was dated within it.
- **R33:** added on the 6th, so its stay is 1h 30m. Its first receipt was dated to the 5th, before its stay began, and the receipt's call to `receive` was refused. The request was refused with the call's verdict, the step and R33 (`DESIGN.md` §5.5, `CallRefused`).

The median of the four stays is the lower middle, 4d 13h.

**Both times are kept.** Every event carries the time it happened and the time it was recorded (`declaration-syntax.md` §4.2):

Read: `events(R30)`

| transition | occurred_at | recorded_at |
|---|---|---|
| request | 2026-10-01T02:00Z | 2026-10-01T02:00Z |
| ship | 2026-10-01T03:00Z | 2026-10-01T03:00Z |
| receive | 2026-10-05T16:00Z | 2026-10-06T09:05Z |

Read: `events(S1)`

| transition | occurred_at | recorded_at |
|---|---|---|
| dispatch | 2026-10-01T03:00Z | 2026-10-01T03:00Z |
| add_unit | 2026-10-06T08:00Z | 2026-10-06T08:00Z |
| arrive | 2026-10-05T16:00Z | 2026-10-06T09:00Z |
| receive_unit | 2026-10-05T16:00Z | 2026-10-06T09:05Z |
| receive_unit | 2026-10-06T09:10Z | 2026-10-06T09:10Z |
| receive_unit | 2026-10-06T09:30Z | 2026-10-06T09:30Z |
| receive_unit | 2026-10-08T10:00Z | 2026-10-09T12:05Z |

The two refused receipts, one past the bound and one before the unit's stay:

Read: `metric(Shipment.refusals, keep: [transition, clause, remedy])`

| transition | clause | remedy | value | gaps |
|---|---|---|---|---|
| receive_unit | occurred_within | self_serviceable | 2 | |

## 12. What is inferred, and what UC-4 to UC-6 found

**Inferences**, each resting on a requirement or the reviewed flow:
1. **Inspection and availability are a published version of the journey** (UC-4, UC-5). The journey, written from production, has neither, so the lead publishes them. Each construct is the one the model writes for these use cases:
   - `PdiCheck`, `PdiResult` and the gate follow `declaration-syntax.md` §6.8's pre-delivery inspection, with the checklist per model as §6.1 infers the configuration.
   - The datapoints are the delivery's, as §6.8 advises for a kind a gate reads.
2. **An order is committed when the delivery is opened** (UC-5). The journey's delivery has no commit, and it is what an order becomes. Its opening is where units are first bound or pegged, which is what an availability check decides.
3. **The gate asks that each check have a result, not a pass** (UC-4): "delivery is gated on every check having a result". A result that fails, like P3, satisfies it, and whether a failure should block a delivery is a question for the lead, not the scenario.

**Findings.**
1. **The model's inspection gate declares `self_serviceable` on a transition that takes no input** (D446). `declaration-syntax.md` §6.8's `hand_over` requires `all_checked ... because self_serviceable`. Since ADR-0122 decision 30, that class means correcting an input of the transition requested, and a missing result is written by a recording, another request, which is `unreachable_from_here`. The version above declares the corrected class, and the model's example is corrected in place.
2. **A refused call's attempt counts under the requested transition, with the call's clause** (determined). R33's receipt is counted as `receive_unit` refused by `occurred_within`, the clause of the call to `receive` that failed. The refusal carries the call's verdict, the step and the object (`flow-format.md` §4.8). The standard `refusals` has no dimension for a call, so the two causes of `occurred_within` above count together.
3. **A result may be recorded on a delivered delivery.** `subject_open` refuses a recording only on a final subject, and `DELIVERED` is closed but not final. A result recorded after a completion changes nothing the completion read, since its read set is fixed on its event. Determined; noted because a lead may expect the checklist to close with the delivery.

## 13. The end of October: a version adds formulas and a telemetry summary

The history continues from `now` of §8. The reads below are taken at the new `now`, `2026-11-02T00:00Z`, a Monday; W44 runs from Monday 26 October. The reads of §3 to §11 hold as written, at their own `now`.

**What the first part ported, and §3 to §11 did not read.** Six robots came from the legacy system at the port with their order and receipt history, L01 to L06. The legacy record has an "ordered" status and a "received" one, and no status for a unit in transit. So each ported robot's history is a stay in `REQUESTED` from its order and a stay in `INTAKE` from its receipt, as legacy intervals (`publish-and-import.md` §4, "Intervals"). L05's mapping gives no order date, so it is undated. The ported models name their manufacturers, Armtek and Roverline.

On 20 October the operations lead publishes version 4 of the journey:
- **A daily battery summary on each unit**, `battery_days`, which a telemetry service records from the readings the unit reports every second (UC-7).
- **Supplier lead time**, at the median and the eightieth percentile (UC-8, UC-9).
- **First-pass yield**, as two distinct counts over the pre-delivery results and one combined metric that divides them (UC-8).
- **A promised date on each delivery**, taken when it is opened, and **the on-time delivery rate** against it (UC-8).
- **A count of units with a weak day** (UC-7).

```yaml publish
at: 2026-10-20T00:00Z
add:
  types.Robot.observations.battery_days:
    kind: BatteryDay
    description: One day of a unit's battery health, summarised by the telemetry service from the readings the unit reports every second.
    attributes:
      min_health: { type: "decimal(4,1)", unit: "%" }
      mean_health: { type: "decimal(4,1)", unit: "%" }
      samples: { type: int }
    max_recording_delay: 2 days
  types.Robot.metrics.supplier_lead_time:
    description: The median time from ordering a unit to receiving it, by its model's manufacturer and the month it was received.
    source: objects
    item: o
    filter: o.entered_at(REQUESTED) is not null and o.entered_at(INTAKE) is not null
    dimensions:
      supplier: o.model.manufacturer
      month: month(o.entered_at(INTAKE))
    time_dimension: o.entered_at(INTAKE)
    expression: median(o.entered_at(INTAKE) - o.created_at)
  types.Robot.metrics.supplier_lead_time_p80:
    description: The time from ordering a unit to receiving it that four in five units beat, by manufacturer and month received.
    source: objects
    item: o
    filter: o.entered_at(REQUESTED) is not null and o.entered_at(INTAKE) is not null
    dimensions:
      supplier: o.model.manufacturer
      month: month(o.entered_at(INTAKE))
    time_dimension: o.entered_at(INTAKE)
    expression: percentile(0.8, o.entered_at(INTAKE) - o.created_at)
  types.Robot.metrics.weak_battery_units:
    description: The units whose battery fell below 80 % on some day, by model and week.
    source: battery_days
    item: b
    filter: b.min_health < 80.0
    dimensions:
      model: b.subject.model
      week: week(b.occurred_at)
    expression: count(distinct b.subject)
  types.Delivery.attributes.promised_date: { type: timestamp, optional: true }
  types.Delivery.transitions.open.optional_inputs: [internal, promised_date]
  types.Delivery.metrics.first_results_inspected:
    description: The units with a first recorded result for some pre-delivery check, by model and month.
    source: pdi_results
    item: r
    filter: >-
      none(s in r.subject.pdi_results where s.unit == r.unit and s.check == r.check
                                     and s.recorded_at < r.recorded_at)
    dimensions:
      model: r.unit.model
      month: month(r.occurred_at)
    expression: count(distinct r.unit)
  types.Delivery.metrics.first_results_failed:
    description: The units whose first recorded result for some pre-delivery check failed, by model and month.
    source: pdi_results
    item: r
    filter: >-
      r.outcome == CheckOutcome.FAIL
      and none(s in r.subject.pdi_results where s.unit == r.unit and s.check == r.check
                                         and s.recorded_at < r.recorded_at)
    dimensions:
      model: r.unit.model
      month: month(r.occurred_at)
    expression: count(distinct r.unit)
  types.Delivery.metrics.on_time_rate:
    description: The share of deliveries completed by their promised date, by the month they were completed and the customer.
    source: transitions
    item: t
    filter: t.to_state == Delivery.DELIVERED and t.object.promised_date is not null
    dimensions:
      month: month(t.occurred_at)
      customer: t.object.customer
    expression: count(where t.occurred_at <= t.object.promised_date) * 1.000 / count()
  metrics.first_pass_yield:
    description: The share of inspected units whose every check passed at its first recorded result, by model and month.
    input_metrics:
      inspected: Delivery.first_results_inspected
      failed: Delivery.first_results_failed
    group_by: [model, month]
    expression: (inspected - failed) * 1.000 / inspected
```

**What happens.**
- **Services:** Ana registers two services: the telemetry summariser, and the reporting service behind the weekly report.
- **D12 (Acme, promised 1 November):** Ben binds R03 and R06, two Arm 7s. R06 fails its battery check at first and passes after charging. D12 completes on 30 October, on time.
- **D13 (Boreal, promised 28 October):** R04, a Rover 2, passes its drive and lidar checks at 19:00 and 19:30 UTC on 31 October. D13 completes at 20:00 UTC, 3 days 20 hours late. In Singapore, eight hours ahead, that is already 1 November.
- **D14:** Scout opens it for Acme, promised 5 November.
- **Battery summaries:** from 27 October the telemetry service records each day's battery summary for R08 and R09, in the development pool, and for L03. Its summary for a day three days past is refused.

```yaml scenario
now: 2026-11-02T00:00Z

requests:
  - { at: 2026-10-20T01:00Z, actor: U-ANA, create: Service, object: S-TELEMETRY, transition: register, inputs: { name: telemetry } }
  - { at: 2026-10-20T01:00Z, actor: U-ANA, create: Service, object: S-REPORTS, transition: register, inputs: { name: reports } }
  - { at: 2026-10-26T00:00Z, actor: U-BEN, create: Delivery, object: D12, transition: open, inputs: { customer: C-ACME, promised_date: 2026-11-01T00:00Z } }
  - { at: 2026-10-26T01:00Z, actor: U-BEN, create: Delivery, object: D13, transition: open, inputs: { customer: C-BOREAL, promised_date: 2026-10-28T00:00Z } }
  - { at: 2026-10-26T02:00Z, actor: U-BEN, object: D12, transition: bind_slot, inputs: { robot: R03 } }
  - { at: 2026-10-26T02:01Z, actor: U-BEN, object: D12, transition: bind_slot, inputs: { robot: R06 } }
  - { at: 2026-10-26T03:00Z, actor: U-BEN, object: D13, transition: bind_slot, inputs: { robot: R04 } }
  - { at: 2026-10-27T00:00Z, actor: A-SCOUT, create: Delivery, object: D14, transition: open, inputs: { customer: C-ACME, promised_date: 2026-11-05T00:00Z } }
  - { at: 2026-10-27T01:00Z, actor: S-TELEMETRY, record: battery_days, subject: R08, object: B1, occurred_at: 2026-10-26T00:00Z, fields: { min_health: 91.0, mean_health: 94.2, samples: 86400 } }
  - { at: 2026-10-27T01:00Z, actor: S-TELEMETRY, record: battery_days, subject: R09, object: B2, occurred_at: 2026-10-26T00:00Z, fields: { min_health: 88.0, mean_health: 92.5, samples: 86400 } }
  - { at: 2026-10-27T01:00Z, actor: S-TELEMETRY, record: battery_days, subject: L03, object: B3, occurred_at: 2026-10-26T00:00Z, fields: { min_health: 95.0, mean_health: 97.1, samples: 86400 } }
  - { at: 2026-10-27T01:00Z, actor: S-TELEMETRY, record: battery_days, subject: R09, object: B4, occurred_at: 2026-10-24T00:00Z, fields: { min_health: 90.0, mean_health: 93.0, samples: 86400 }, refused: { verdict: unsatisfied, clause: occurred_within, remedy: self_serviceable } }
  - { at: 2026-10-28T01:00Z, actor: S-TELEMETRY, record: battery_days, subject: R08, object: B5, occurred_at: 2026-10-27T00:00Z, fields: { min_health: 78.5, mean_health: 90.3, samples: 86400 } }
  - { at: 2026-10-28T01:00Z, actor: S-TELEMETRY, record: battery_days, subject: R09, object: B6, occurred_at: 2026-10-27T00:00Z, fields: { min_health: 72.0, mean_health: 85.6, samples: 86400 } }
  - { at: 2026-10-28T09:00Z, actor: U-BEN, record: pdi_results, subject: D12, object: Q1, fields: { unit: R03, check: K1, outcome: PASS, reading: 49.0 } }
  - { at: 2026-10-28T09:10Z, actor: U-BEN, record: pdi_results, subject: D12, object: Q2, fields: { unit: R06, check: K1, outcome: FAIL, reading: 29.5 } }
  - { at: 2026-10-28T15:00Z, actor: U-BEN, record: pdi_results, subject: D12, object: Q3, fields: { unit: R06, check: K1, outcome: PASS, reading: 48.8 } }
  - { at: 2026-10-29T01:00Z, actor: S-TELEMETRY, record: battery_days, subject: R08, object: B7, occurred_at: 2026-10-28T00:00Z, fields: { min_health: 76.0, mean_health: 88.9, samples: 86400 } }
  - { at: 2026-10-30T10:00Z, actor: U-BEN, object: D12, transition: complete_sale }
  - { at: 2026-10-31T19:00Z, actor: U-BEN, record: pdi_results, subject: D13, object: Q4, fields: { unit: R04, check: K3, outcome: PASS } }
  - { at: 2026-10-31T19:30Z, actor: U-BEN, record: pdi_results, subject: D13, object: Q5, fields: { unit: R04, check: K4, outcome: PASS } }
  - { at: 2026-10-31T20:00Z, actor: U-BEN, object: D13, transition: complete_sale }
```

## 14. UC-7: where machine data stops

> A robot reports battery health every second. *Acceptance:* the design says where that data lives and what, if anything, is recorded here — a daily summary per unit, for instance. (PRD §7)

**Where the data lives.** The design says so in two places:
- ADR-0081 §4: "Machine telemetry at sensor rates stays outside; a service may record summaries of it as observations".
- `DESIGN.md` §12 lists high-rate machine telemetry as out of scope, and `data-driven-engine.md` §3.7 names "a daily battery-health reading per unit" as what a service records.

The per-second readings stay in the telemetry system, 86,400 a day per unit. What reaches the store is one `BatteryDay` per unit per day, recorded by a service, an actor whose kind the store knows (PRD T6). Each summary carries the number of readings it covers.

Read: `recorded(R08.battery_days)`

| object | occurred_at | min_health | samples |
|---|---|---|---|
| B1 | 2026-10-26T00:00Z | 91.0 | 86400 |
| B5 | 2026-10-27T00:00Z | 78.5 | 86400 |
| B7 | 2026-10-28T00:00Z | 76.0 | 86400 |

A summary is a datapoint like any other, so a metric reads it, and its recording has the kind's bound:

Read: `metric(weak_battery_units, keep: [model, week])`

| model | week | value | gaps |
|---|---|---|---|
| M-ROVER | 2026-W44 | 1 | |
| M-ARM | 2026-W44 | 1 | |

R08, a Rover 2, fell below 80 % on 27 and 28 October and counts once, as `count(distinct b.subject)` counts units, not days. R09, an Arm 7, fell below on the 27th. L03 stayed above.

Read: `metric(BatteryDay.refusals, keep: [transition, clause, remedy])`

| transition | clause | remedy | value | gaps |
|---|---|---|---|---|
| record | occurred_within | self_serviceable | 1 | |

The summary for R09's 24 October arrived 3 days and 1 hour later, past the kind's 2 days, and was refused.

## 15. UC-8: a formula, declared once

> A quality lead defines first-pass yield per robot model per month: the share of inspected configured robots whose every check passed at its first recorded result. A procurement lead defines supplier lead time, ordered to received, at the median and the eightieth percentile. Operations defines the on-time delivery rate against each delivery's promised date. *Acceptance:* each is declared once; a screen, an agent and a report reading it over the same data get identical values, and one an application narrows by a filter gets the value over the rows the filter selects; and each says it is computed in UTC calendar time. (PRD §7)

**First-pass yield** is one declaration read by name, `first_pass_yield`. A metric's value aggregates the rows of one group, and the yield is a share of *units*, so it combines two distinct counts over each unit's first results: the units inspected, and the units with a first result that failed. A first result is one nothing recorded earlier for the same unit and check, and a result a correction replaced is not read at all (`declaration-syntax.md` §6.8). The first results in October:
- **Arm 7:**
  - R01 passed both checks at first; P4, the correction, stands in for the mistaken P2.
  - R02 failed its battery check at first (P3).
  - R03 passed.
  - R06 failed its battery check at first (Q2).
  - Four units, two failed: 0.500.
- **Rover 2:**
  - R23's drive test (P7) passed.
  - R04's drive and lidar checks passed at 19:00 and 19:30 UTC on 31 October, October in UTC although November in Singapore.
  - Two units, none failed: 1.000.

A screen, an agent and a report read the same declaration over the same rows, and the engine filters no read by who is asking (PRD T5). So the three reads below differ only in their reader, and the script checks each reader is one the store holds:

Read: `metric(first_pass_yield, keep: [model, month]) as U-ANA`

| model | month | value | gaps |
|---|---|---|---|
| M-ARM | 2026-10 | 0.500 | |
| M-ROVER | 2026-10 | 1.000 | |

Read: `metric(first_pass_yield, keep: [model, month]) as A-SCOUT`

| model | month | value | gaps |
|---|---|---|---|
| M-ARM | 2026-10 | 0.500 | |
| M-ROVER | 2026-10 | 1.000 | |

Read: `metric(first_pass_yield, keep: [model, month]) as S-REPORTS`

| model | month | value | gaps |
|---|---|---|---|
| M-ARM | 2026-10 | 0.500 | |
| M-ROVER | 2026-10 | 1.000 | |

**The on-time delivery rate** reads each completion into `DELIVERED` of a delivery with a promised date:
- D12 completed on 30 October, before its 1 November: on time.
- D13 completed at 20:00 UTC on 31 October, after its 28 October: late. It is counted in October, the month of its completion in UTC.
- The deliveries completed before the promised date existed have none, and are not counted.

An application that narrows the read to one customer gets the value over the rows its filter selects.

Read: `metric(on_time_rate, keep: [month])`

| month | value | gaps |
|---|---|---|
| 2026-10 | 0.500 | |

Read: `metric(on_time_rate, keep: [month], filter: "t.object.customer == C-ACME")`

| month | value | gaps |
|---|---|---|
| 2026-10 | 1.000 | |

**Supplier lead time** is §16's.

**UTC calendar time.** Every time bucket is a UTC calendar bucket, whatever a session's time zone (`declaration-syntax.md` §6.9; PRD C6). Two of the values above depend on it: D13's late completion and R04's first results fall in October in UTC, and would fall in November for a reader in Singapore who bucketed by local time.

## 16. UC-9: a new metric over old data

> Supplier lead time is defined today, and the lead wants the last twelve months. *Acceptance:* the metric covers history recorded before it was defined, as far as the data exists. (PRD §7)

Supplier lead time was declared on 20 October. It reads each unit ordered and received, from its creation, when it was ordered, to its last entry into `INTAKE`, when it was received, by its model's manufacturer. A metric is computed on read over every row its source holds, so it covers every unit received before it was declared:
- the legacy robots, from their ported intervals;
- the four Rover 2s of §11.

Units taken in without an order, such as the Kestrel units and the opening stock, never entered `REQUESTED`, and are not counted. The ported units' lead times:

| Unit | Supplier | Ordered | Received | Lead time |
|---|---|---|---|---|
| L06 | Roverline | 4 Aug 2025 | 10 Sep 2025 | 37d |
| L01 | Armtek | 6 Oct 2025 | 17 Nov 2025 | 42d |
| L02 | Armtek | 12 Jan 2026 | 9 Feb 2026 | 28d |
| L03 | Roverline | 2 Mar 2026 | 27 Apr 2026 | 56d |
| L05 | Armtek | undated; its first known time is 6 Apr 2026 | 4 May 2026 | 28d, a gap |
| L04 | Roverline | 1 Jun 2026 | 6 Jul 2026 | 35d |

**As far as the data exists.** L05's mapping gave no order date, so its creation time is the earliest the port knew of it, and it is undated. Every read over it names L05 in its `gaps` (`declaration-syntax.md` §6.9). Its value is kept, and the reader is told the value may be too short.

Read: `metric(supplier_lead_time, keep: [supplier, month])`

| supplier | month | value | gaps |
|---|---|---|---|
| Roverline | 2025-09 | 37d | |
| Armtek | 2025-11 | 42d | |
| Armtek | 2026-02 | 28d | |
| Roverline | 2026-04 | 56d | |
| Armtek | 2026-05 | 28d | L05 |
| Roverline | 2026-07 | 35d | |
| Roverline | 2026-10 | 5d 7h 10m | |

October's Rover 2s took 4d 14h, 5d 7h 10m, 5d 7h 30m and 7d 8h, and the median of four is the lower middle.

**The last twelve months.** A month has no fixed length, so a window is not written in months (`declaration-syntax.md` §8.3). The lead reads the last 365 days by the time each unit was received, the metric's `time_dimension`, which leaves out L06, received in September 2025:

Read: `metric(supplier_lead_time, keep: [supplier], over: "365 days")`

| supplier | value | gaps |
|---|---|---|
| Armtek | 28d | L05 |
| Roverline | 5d 7h 30m | |

Read: `metric(supplier_lead_time_p80, keep: [supplier], over: "365 days")`

| supplier | value | gaps |
|---|---|---|
| Armtek | 42d | L05 |
| Roverline | 35d | |

The values for each supplier:
- **Armtek:** 28d, 28d and 42d. The median is the second, 28d. The eightieth percentile is the smallest value whose rank is at least 2.4, the third, 42d.
- **Roverline:** six units, 4d 14h to 56d. The median is the third, 5d 7h 30m. The eightieth percentile is the fifth, 35d. The legacy units' longer waits sit behind this year's short Rover shipments.

## 17. What is inferred, and what UC-7 to UC-9 found

**Inferences**, each resting on a requirement or the reviewed flow:
1. **A service records the telemetry summary** (UC-7). ADR-0081 §4 and `data-driven-engine.md` §3.7 say a service summarises telemetry and records the summary. The journey declared no actor type for a service, so `operations_shared` now declares `Service` as the format's `people.yaml` does, alongside `Agent` (D448).
2. **The supplier is the model's manufacturer** (UC-8, UC-9). The journey records no supplier; what it knows of where a unit comes from is its model's `manufacturer`.
3. **Ordered is created, received is entering `INTAKE`** (UC-8, UC-9). A unit is ordered by `request`, which creates it in `REQUESTED`, and received by `receive`, which puts it in `INTAKE`. The lead time is the time between, from the unit's own record.
4. **The legacy record had no in-transit status.** The ported robots' histories go from `REQUESTED` to `INTAKE`, as a legacy system with only "ordered" and "received" would give them.
5. **"The last twelve months" is the last 365 days** (UC-9), since a window is written in fixed units and a month is not one.
6. **A check not applicable is not a failure** (UC-8). First-pass yield counts a unit as failed only where a first result is `FAIL`, so R01, whose mistaken "not applicable" was corrected, and any unit with a check that does not apply to it, pass.
7. **A promised date is taken when the delivery is opened** (UC-8), as the order's commitment is (§12). Deliveries opened before version 4 have none and are not counted, as a formula over data that was never recorded cannot count it.

**Findings.**
1. **No actor type for a service** (D448). As with the agent (D445), the journey's shared module declared none, so the telemetry service could not act. Resolved in place, as `Agent` was.
2. **A per-unit share takes a combined metric of two distinct counts** (determined). A metric's value aggregates the rows of one group, and first-pass yield is a share of units over rows of results. So it is written as two metrics counting distinct units and a combined metric dividing them, which the format allows. A reader wanting it in one declaration would need `count(distinct … where …)`, which the model does not offer.
3. **A metric filter may read the subject's other observations.** `first_results_inspected`'s filter reads `r.subject.pdi_results`, each row's delivery's other results, to find a first one. The flow checker accepts it in all four steps. The model's text says a row's own flow data is a collection an aggregate may range over, and does not say whether another observation of the same subject is one (`declaration-syntax.md` §6.9). `TODO.md` holds the question.

## 18. November: a version adds a rule on a metric, and the business exceptions

The history continues from `now` of §13. The reads below are taken at the new `now`, `2026-11-16T00:00Z`, a Monday. The reads of §3 to §16 hold as written, at their own `now`.

**What the first part ported, and nothing had read.** Two more objects came from the legacy system at the port:
- **L07**, an Arm 7 ordered on 20 July and never received.
- **E01**, a lease out since August and due back on 10 November.

On 2 November the lead publishes version 5 of the journey:
- **A rule that reads a metric** (UC-10). Each model may carry a `yield_floor`, and completing a delivery needs, for every unit, its model's first-pass rate over the last 30 days at or above the floor, or a second sign-off. The rate is a metric with a `time_dimension`, so a guard can window it.
- **A norm held as data** (UC-12, ADR-0113). Each model may carry a `usual_lead_time`, which the reporting service refreshes from supplier lead time by a declared transition.
- **The three business exceptions the journey lacked** (UC-12), each a derived attribute over a stored operand and `now` (`data-driven-engine.md` §3.10):
  - backorder ageing, against the norm;
  - a delivery date at risk;
  - a warranty expiring, on a new `Warranty` type.

```yaml publish
at: 2026-11-02T00:00Z
add:
  types.RobotModel.attributes.yield_floor: { type: "decimal(4,3)", optional: true }
  types.RobotModel.attributes.usual_lead_time: { type: duration, optional: true }
  types.RobotModel.transitions.set_yield_floor:
    kind: internal
    from: ACTIVE
    description: Sets the first-pass rate below which a delivery of the model needs a second sign-off.
    required_inputs: [yield_floor]
  types.RobotModel.transitions.set_usual_lead_time:
    kind: internal
    from: ACTIVE
    description: Records the model's usual lead time, which the reporting service refreshes from supplier lead time.
    required_inputs: [usual_lead_time]
  types.Robot.derived_attributes.backorder_ageing:
    description: The unit is promised to a delivery and not yet received, longer than its model's usual lead time.
    expression: >-
      peg is not null and (state == REQUESTED or state == PROCUREMENT)
      and model.usual_lead_time is not null and created_at + model.usual_lead_time < now
  types.Delivery.observations.second_sign_offs:
    kind: SecondSignOff
    description: A second sign-off on the delivery, by someone other than whoever asked to complete it.
    attributes:
      remark: { type: string, optional: true, personal: true }
  types.Delivery.derived_attributes.date_at_risk:
    description: The delivery is still being prepared within two days of the date promised for it.
    expression: state == PREPARATION and promised_date is not null and promised_date < now + 2 days
  types.Delivery.conditions.yield_or_signed:
    description: Every unit's model passes its checks at first often enough over the last 30 days, or the delivery has a second sign-off.
    expression: >-
      count(s in second_sign_offs) >= 1
      or all(u in units: u.model.yield_floor is null
                         or metric(first_pass_rate, model := u.model, over last 30 days) >= u.model.yield_floor)
    remedy: delegable
  types.Delivery.transitions.complete_sale.guards.yield_or_signed: deny
  types.Delivery.transitions.complete_internal.guards.yield_or_signed: deny
  types.Delivery.metrics.first_pass_rate:
    description: The share of pre-delivery checks passed at their first recorded result, by model.
    source: pdi_results
    item: r
    filter: >-
      none(s in r.subject.pdi_results where s.unit == r.unit and s.check == r.check
                                     and s.recorded_at < r.recorded_at)
    dimensions:
      model: r.unit.model
    time_dimension: r.occurred_at
    expression: count(where r.outcome != CheckOutcome.FAIL) * 1.000 / count()
  types.Warranty:
    description: A unit's warranty, whose end the sales application computes from the sale and the model's warranty months.
    tracking: record
    attributes:
      unit: { reference: Robot }
      ends_at: { type: timestamp, indexed: true }
    states:
      ACTIVE: { category: live }
      ENDED: { category: closed, final: true }
    derived_attributes:
      expiring:
        description: The warranty is in force and ends within 30 days.
        expression: state == ACTIVE and ends_at < now + 30 days
    transitions:
      start:
        kind: initial
        to: ACTIVE
        required_inputs: [unit, ends_at]
      end:
        kind: external
        from: ACTIVE
        to: ENDED
```

**What happens.**
- **Thresholds.** Ana sets the Arm 7's yield floor to 0.800 and its reorder point to 2. The reporting service sets each model's usual lead time from supplier lead time's eightieth percentile: 42 days for the Arm 7, 35 for the Rover 2.
- **Subscriptions.** The reporting service subscribes to every binding of a unit to a delivery, and to every unit leaving intake, for "order ready".
- **D14 (Acme), UC-10:**
  - Ben binds L01 and L05, and both pass their battery checks.
  - Asked on 7 November to complete D14, the store refuses: the Arm 7's first-pass rate over the last 30 days is 0.750, below its floor. Ana records a second sign-off, and D14 completes.
  - D15, a Rover 2 for Boreal, completes on the 10th, the Rover 2 having no floor.
- **Orders and a shipment.** Ben orders three more units: R40 and R41, Rover 2s, and R42, an Arm 7. R40 leaves on shipment S2, due on 10 November and still in transit.
- **Two more deliveries:**
  - D16 pegs L07, the Arm 7 ordered in July.
  - D17 pegs R30, still in intake, binds L04, and is promised for 17 November. R30 leaves intake on the 14th, which fills D17.
- **Warranties.** Ana starts three: on L02, R05 and L06, the last ending on 1 December.
- **UC-11.** On 16 November the procurement agent, Buyer, reads supplier lead time and the open orders, ranks them, and escalates L07 with a label.

```yaml scenario
now: 2026-11-16T00:00Z

requests:
  - { at: 2026-11-02T01:00Z, actor: U-ANA, object: M-ARM, transition: set_yield_floor, inputs: { yield_floor: 0.800 } }
  - { at: 2026-11-02T01:05Z, actor: U-ANA, object: M-ARM, transition: set_reorder_point, inputs: { reorder_point: 2 } }
  - { at: 2026-11-02T02:00Z, actor: U-ANA, create: Agent, object: A-BUYER, transition: issue, inputs: { key_id: buyer, name: Buyer } }
  - { at: 2026-11-02T03:00Z, actor: U-BEN, create: Robot, object: R40, transition: request, inputs: { model: M-ROVER } }
  - { at: 2026-11-02T04:00Z, actor: S-REPORTS, object: M-ARM, transition: set_usual_lead_time, inputs: { usual_lead_time: 42 days } }
  - { at: 2026-11-02T04:00Z, actor: S-REPORTS, object: M-ROVER, transition: set_usual_lead_time, inputs: { usual_lead_time: 35 days } }
  - { at: 2026-11-02T05:00Z, actor: S-REPORTS, subscribe: SUB-BOUND, filter: { type: Delivery, transitions: [bind_slot] } }
  - { at: 2026-11-02T05:00Z, actor: S-REPORTS, subscribe: SUB-ARRIVED, filter: { type: Robot, transitions: [inventorize] } }
  - { at: 2026-11-03T00:00Z, actor: U-BEN, create: Robot, object: R42, transition: request, inputs: { model: M-ARM } }
  - { at: 2026-11-03T01:00Z, actor: U-BEN, create: Shipment, object: S2, transition: dispatch, inputs: { with_units: [R40], eta: 2026-11-10T00:00Z } }
  - { at: 2026-11-03T02:00Z, actor: U-BEN, create: Delivery, object: D16, transition: open, inputs: { customer: C-BOREAL, promised_date: 2026-12-01T00:00Z } }
  - { at: 2026-11-03T03:00Z, actor: U-BEN, object: D16, transition: peg_slot, inputs: { robot: L07 } }
  - { at: 2026-11-04T00:00Z, actor: U-BEN, object: D14, transition: bind_slot, inputs: { robot: L01 } }
  - { at: 2026-11-04T00:01Z, actor: U-BEN, object: D14, transition: bind_slot, inputs: { robot: L05 } }
  - { at: 2026-11-04T01:00Z, actor: U-BEN, record: pdi_results, subject: D14, object: Q6, fields: { unit: L01, check: K1, outcome: PASS } }
  - { at: 2026-11-04T01:05Z, actor: U-BEN, record: pdi_results, subject: D14, object: Q7, fields: { unit: L05, check: K1, outcome: PASS } }
  - { at: 2026-11-07T00:00Z, actor: U-BEN, object: D14, transition: complete_sale, refused: { verdict: unsatisfied, clause: yield_or_signed, remedy: delegable } }
  - { at: 2026-11-07T01:00Z, actor: U-ANA, record: second_sign_offs, subject: D14, object: SS1, fields: {} }
  - { at: 2026-11-07T02:00Z, actor: U-BEN, object: D14, transition: complete_sale }
  - { at: 2026-11-08T00:00Z, actor: U-BEN, create: Delivery, object: D15, transition: open, inputs: { customer: C-BOREAL, promised_date: 2026-11-20T00:00Z } }
  - { at: 2026-11-08T01:00Z, actor: U-BEN, object: D15, transition: bind_slot, inputs: { robot: L03 } }
  - { at: 2026-11-08T02:00Z, actor: U-BEN, record: pdi_results, subject: D15, object: Q8, fields: { unit: L03, check: K3, outcome: PASS } }
  - { at: 2026-11-08T02:05Z, actor: U-BEN, record: pdi_results, subject: D15, object: Q9, fields: { unit: L03, check: K4, outcome: PASS } }
  - { at: 2026-11-09T00:00Z, actor: U-BEN, create: Robot, object: R41, transition: request, inputs: { model: M-ROVER } }
  - { at: 2026-11-10T00:00Z, actor: U-BEN, object: D15, transition: complete_sale }
  - { at: 2026-11-12T00:00Z, actor: U-BEN, create: Delivery, object: D17, transition: open, inputs: { customer: C-ACME, promised_date: 2026-11-17T00:00Z } }
  - { at: 2026-11-12T01:00Z, actor: U-ANA, create: Warranty, object: W1, transition: start, inputs: { unit: L02, ends_at: 2027-03-02T00:00Z } }
  - { at: 2026-11-12T01:00Z, actor: U-ANA, create: Warranty, object: W2, transition: start, inputs: { unit: L06, ends_at: 2026-12-01T00:00Z } }
  - { at: 2026-11-12T01:00Z, actor: U-ANA, create: Warranty, object: W3, transition: start, inputs: { unit: R05, ends_at: 2027-09-04T00:00Z } }
  - { at: 2026-11-12T01:30Z, actor: U-BEN, object: D17, transition: peg_slot, inputs: { robot: R30 } }
  - { at: 2026-11-12T02:00Z, actor: U-BEN, object: D17, transition: bind_slot, inputs: { robot: L04 } }
  - { at: 2026-11-14T00:00Z, actor: U-BEN, object: R30, transition: record_label_print }
  - { at: 2026-11-14T01:00Z, actor: U-BEN, object: R30, transition: inventorize }
  - { at: 2026-11-16T00:00Z, actor: A-BUYER, label: late-risk, subject: L07, object: LB1 }
```

## 19. UC-10: a rule that reads a metric

> *Hypothetical; no first-consumer process asks for this yet.* Deliveries of a robot model whose first-pass yield over the last thirty days is below a threshold need a second sign-off. *Acceptance:* the refusal names the threshold and the value; the record of every completion holds the value it was decided on; the threshold changes only through a governed path. (PRD §7)

**The rule.** `yield_or_signed` passes if the delivery has a second sign-off, or if every unit's model has no floor or a first-pass rate at or above it over the last 30 days. A metric reference binds its dimension from the loop's unit, `model := u.model`. It is consulted before the transaction, and its value is recorded on the event or the refusal, whichever side of the `or` decides (`declaration-syntax.md` §6.9, "In a guard"). Its remedy is `delegable`: the way through is a second sign-off, which someone else records (`DESIGN.md` §5.5).

**Why a first-pass *rate*.** UC-10 names first-pass *yield*, and the yield of §15 is a combined metric. A combined metric has no `time_dimension`, so a guard cannot window it over the last 30 days (§22). The rule reads the share of checks passed at their first result, by model, which has one.

**The refusal names the threshold and the value.** On 7 November the rule read the Arm 7's first results since 8 October:
- Q1, R03's pass;
- Q2, R06's first failure;
- Q6 and Q7, L01's and L05's passes.

That is three in four, 0.750, below the floor of 0.800. D09's results of 5 and 6 October had fallen out of the window.

Read: `refusal of D14.complete_sale`

| clause | remedy | consulted | threshold |
|---|---|---|---|
| yield_or_signed | delegable | first_pass_rate(model: M-ARM) = 0.750 | M-ARM.yield_floor = 0.800 |

**The record of every completion holds the value it was decided on.** D14 completed with Ana's sign-off, and its event keeps the value the rule consulted, 0.750, though the sign-off decided it. D15's keeps the Rover 2's rate: 1.000, from Q4, Q5, Q8 and Q9, the Rover 2's first results since 11 October.

Read: `consulted of D14.complete_sale`

| reference | value |
|---|---|
| first_pass_rate(model: M-ARM) | 0.750 |

Read: `consulted of D15.complete_sale`

| reference | value |
|---|---|
| first_pass_rate(model: M-ROVER) | 1.000 |

**The threshold changes only through a governed path.** The floor is an attribute of the model, written only by `set_yield_floor`, a declared transition whose event records who set it and when (PRD L4). The Arm 7's history:

Read: `events(M-ARM)`

| transition | occurred_at | recorded_at |
|---|---|---|
| import | 2026-09-01T00:00Z | 2026-09-01T00:00Z |
| set_yield_floor | 2026-11-02T01:00Z | 2026-11-02T01:00Z |
| set_reorder_point | 2026-11-02T01:05Z | 2026-11-02T01:05Z |
| set_usual_lead_time | 2026-11-02T04:00Z | 2026-11-02T04:00Z |

Read: `metric(Delivery.refusals, keep: [clause, remedy, actor_kind], filter: "a.clause == \"yield_or_signed\"")`

| clause | remedy | actor_kind | value | gaps |
|---|---|---|---|---|
| yield_or_signed | delegable | human | 1 | |

## 20. UC-11: an agent decides with data

> A procurement agent ranks open purchase orders by risk of being late, from supplier lead-time metrics and each order's age, and escalates the riskiest. *Acceptance:* the agent reads the same metric definitions a person would, narrowed as the application it works through narrows any read; its escalations are recorded as its actions; the ranking itself is the agent's, not the engine's. (PRD §7)

An open purchase order is a unit ordered and not yet received, in `REQUESTED` or `PROCUREMENT` (§17). Buyer, an agent, reads two things through the engine:
- the eightieth percentile of supplier lead time over the last 365 days, the same declaration a person reads (§16);
- the open orders, oldest first.

Read: `metric(supplier_lead_time_p80, keep: [supplier], over: "365 days") as A-BUYER`

| supplier | value | gaps |
|---|---|---|
| Armtek | 42d | L05 |
| Roverline | 35d | |

The procurement application Buyer works through serves the Roverline desk, and narrows every read it passes on to Roverline's units. The narrowed read gives the value over the rows the filter selects:

Read: `metric(supplier_lead_time_p80, keep: [supplier], filter: "o.model.manufacturer == Roverline", over: "365 days") as A-BUYER`

| supplier | value | gaps |
|---|---|---|
| Roverline | 35d | |

Read: `query(Robot, filter: "state == REQUESTED or state == PROCUREMENT", order: "created_at")`

| object | state | created_at |
|---|---|---|
| L07 | REQUESTED | 2026-07-20T00:00Z |
| R40 | PROCUREMENT | 2026-11-02T03:00Z |
| R42 | REQUESTED | 2026-11-03T00:00Z |
| R41 | REQUESTED | 2026-11-09T00:00Z |

**The ranking is the agent's.** Buyer divides each order's age by its supplier's eightieth percentile, a rule of its own that no declaration holds:

| Order | Supplier | Age at `now` | Risk |
|---|---|---|---|
| L07 | Armtek | 119d | 2.83 |
| R40 | Roverline | 13d 21h | 0.40 |
| R42 | Armtek | 13d | 0.31 |
| R41 | Roverline | 7d | 0.20 |

The engine returned the values and the ages. The division, the order and the choice to escalate L07 are Buyer's, and no read of the engine gives them. A different agent reading the same values could rank by something else.

**Its escalations are recorded as its actions.** Buyer escalates L07 by labelling it `late-risk`. A label is a request, recorded with its actor and kind, and needs no declaration (`declaration-syntax.md` §6.8, PRD D6). Had the flow a transition for escalating, Buyer would request it; the journey has none, and a label is how information not yet modelled is recorded.

Read: `labels(L07)`

| object | name | recorded_by_kind | subject_state |
|---|---|---|---|
| LB1 | late-risk | agent | REQUESTED |

## 21. UC-12: business exceptions from declared conditions

> The first consumer's alert catalogue holds six business exceptions — low stock, procurement overdue, backorder ageing, delivery date at risk, lease overdue, warranty expiring — and one notice that is not an exception at all: order ready, which is an event, the last slot filled. *Acceptance:* each of the six conditions is declared once and answerable by a query a scheduler or an agent runs; a threshold one of them reads, such as a reorder point per model, is held on an object and changes only by that object's recorded transition; order ready is a subscription to the transition that fills the last slot; sending any alert is an upper-layer application's; and an ageing condition, such as backorder ageing, can compare against the norm observed for that kind of work in the flow's own history instead of a fixed number. (PRD §7)

**Each of the six is declared once and answered by a query.** Each is a derived attribute of the object it is about, over a stored operand and `now` (`data-driven-engine.md` §3.10). Three were in the journey already, and version 5 added three. A scheduler or an agent asks for the objects in which each holds:

| Exception | Declared as | Holds at `now` for |
|---|---|---|
| Low stock | `RobotModel.low_stock`: fewer units on offer than the model's reorder point | M-ARM: none on offer, reorder point 2 |
| Procurement overdue | `Shipment.overdue`: in transit past its expected arrival | S2, due on 10 November |
| Backorder ageing | `Robot.backorder_ageing`: promised to a delivery, not received, longer than its model's usual lead time | L07: pegged to D16, ordered 119 days ago against the Arm 7's 42 |
| Delivery date at risk | `Delivery.date_at_risk`: still being prepared within two days of its promised date | D17, promised for 17 November |
| Lease overdue | `Engagement.overdue`: out past its expected return | E01, due back on 10 November |
| Warranty expiring | `Warranty.expiring`: in force, ending within 30 days | W2, ending on 1 December |

Read: `query(RobotModel, filter: "low_stock")`

| object |
|---|
| M-ARM |

Read: `query(Shipment, filter: "overdue")`

| object |
|---|
| S2 |

Read: `query(Robot, filter: "backorder_ageing")`

| object |
|---|
| L07 |

Read: `query(Delivery, filter: "date_at_risk")`

| object |
|---|
| D17 |

Read: `query(Engagement, filter: "overdue")`

| object |
|---|
| E01 |

Read: `query(Warranty, filter: "expiring")`

| object |
|---|
| W2 |

**A threshold is held on an object.** The reorder point is the model's attribute, written only by `set_reorder_point`. The yield floor and the usual lead time are the same: each change is an event, and `events(M-ARM)` in §19 shows all three.

**An ageing condition compares against the norm observed in the flow's own history.** Backorder ageing does not compare against a fixed number. It compares against the model's `usual_lead_time`, which the reporting service set from supplier lead time's eightieth percentile by a declared transition (ADR-0113). The norm is observed, since it comes from the store's own metric; governed, since only a recorded transition writes it; and explainable, since the condition reads a value on the record. A condition cannot read the metric itself, since a metric is read only in a guard (`declaration-syntax.md` §6.9).

**Order ready is a subscription.** A delivery is ready when it is `filled`: at least one unit bound and no unit still pegged. The subscription filter names a type and transitions (`DESIGN.md` §7), and in the journey a delivery fills in either of two ways:
- by binding a unit;
- by its last pegged unit leaving intake, a Robot's transition.

So the reporting service subscribes to both and reads the delivery's `filled` after each event (§22).

Read: `pull(SUB-BOUND)`

| object | transition | occurred_at |
|---|---|---|
| D14 | bind_slot | 2026-11-04T00:00Z |
| D14 | bind_slot | 2026-11-04T00:01Z |
| D15 | bind_slot | 2026-11-08T01:00Z |
| D17 | bind_slot | 2026-11-12T02:00Z |

Read: `pull(SUB-ARRIVED)`

| object | transition | occurred_at |
|---|---|---|
| R30 | inventorize | 2026-11-14T01:00Z |

What the relay concludes from each event:
- **D14 is ready at its first binding**, of L01, since `filled` asks for one unit, not a number.
- **D15 is ready at its binding.**
- **D17 is not ready at its binding**, since R30 was still pegged.
- **D17 is ready when R30 leaves intake**, an event the first subscription could not see.

**Sending any alert is an upper layer's.** The engine answered six queries and delivered two subscriptions' events. Whether the reporting service posts to Slack, raises a Jira issue or does nothing is not in any declaration, and the engine posts nothing (`data-driven-engine.md` §3.10; ADR-0105).

## 22. What is inferred, and what UC-10 to UC-12 found

**Inferences**, each resting on a requirement or the reviewed flow:
1. **The threshold of UC-10 is a floor per model**, `RobotModel.yield_floor`, written by its own transition, as the reorder point is (PRD L4). A model with no floor never needs the sign-off.
2. **A second sign-off is a datapoint on the delivery.** It follows the model's approval pattern, "a recorded part of the approved object", with a gate counting it (`DESIGN.md` §5.5). Who may sign, and that it is someone other than who asked, are the upper layer's (ADR-0114).
3. **An open purchase order is a unit ordered and not received** (UC-11). The journey has no purchase-order type; a unit in `REQUESTED` or `PROCUREMENT` is what one is for.
4. **An escalation is a label** (UC-11), as the journey declares no transition for it and a label records what is not yet modelled.
5. **Backorder ageing is about the unit promised to a delivery**, since in the journey the delivery pegs a unit not yet received. "Its reference to a unit has been empty" is, here, a peg on a unit that has not arrived (`data-driven-engine.md` §3.10).
6. **The norm is supplier lead time's eightieth percentile, held per model** by the reporting service, and a warranty's end date is computed by the sales application, since a warranty is counted in months and a month has no fixed length (`declaration-syntax.md` §8.3).
7. **A delivery date is at risk within two days of the promise**, a fixed window written in the declaration, as `data-driven-engine.md` §3.10 says a fixed window is.

**Findings.**
1. **A combined metric cannot be read over a window** (UC-10). The first-pass yield of §15 is a combined metric, and a combined metric has no `time_dimension`, so a guard's `over last 30 days` cannot apply to it: step 4 refuses it by check 56, "'over last' on metric first_pass_yield, which declares no 'window on'", as a probe showed. The rule reads a per-check first-pass rate instead. `TODO.md` holds the question: give a combined metric a time dimension, or add `count(distinct … where …)` so the yield is one metric.
2. **In the journey, "order ready" is not one transition** (UC-12). A delivery fills when a unit is bound or when its last pegged unit leaves intake, and a subscription names transitions of one type. So order ready is two subscriptions and a read of `filled`, and `filled` knows no count of slots: a delivery is ready at its first binding. The first consumer's slots, one per unit ordered, have no counterpart in the journey. `TODO.md` holds the question.
3. **A metric's consulted value is recorded whichever side decides** (determined). D14 completed on the sign-off, and its event still holds 0.750, since every metric reference in a guard is consulted before the transaction.

## 23. The last weeks of 2026: returns start coarse, and a rule goes on trial

The history continues from `now` of §18. It has two more parts:
- the fifth, read at `2026-12-14T00:00Z` (§24 to §26);
- the sixth, read at `2027-01-11T00:00Z` (§27).

Each is a Monday. The reads of §3 to §21 hold as written, at their own `now`.

**Returns start coarse** (UC-13). UC-13 speaks of service jobs, and the journey's service job already has its working state. The design's worked example of a flow that starts coarse is returns: `returns-module.md` writes its first publish with no rules at all, and its second promoting a label into a state. The scenario installs that module as it is written, so each version the store publishes is one the document checker already checks (§28).

**A rule goes on trial** (UC-14). A photo is proposed as required before a Rover 2 goes on offer. The rule, `rover_photo`, is published as an audited guard on `inventorize`: evaluated on every request, refusing nothing, and each failure recorded (`flow-format.md` §4.8, `audit`). With it comes a count of the refusals a rule on trial would have made, by rule and by who would have been refused.

On 16 November the lead publishes version 6. It installs the returns module's first version and puts the rule on trial:

```yaml publish
at: 2026-11-16T01:00Z
modules:
  returns:
    text: returns-module.md 1
  inventory_journey:
    add:
      machines.UnitLifecycle.conditions.rover_photo:
        description: A Rover 2 has a label photo before it goes on offer.
        expression: 'model.name != "Rover 2" or count(p in photos) >= 1'
        remedy: unreachable_from_here
      machines.UnitLifecycle.transitions.inventorize.guards.rover_photo: audit
      types.Robot.metrics.trial_refusals:
        description: The refusals a rule on trial would have made, by the rule and by who would have been refused.
        source: attempts
        item: a
        filter: not a.enforced
        dimensions:
          clause: a.clause
          actor: a.actor_id
        expression: count()
```

**What happens.**
- **Returns T1 to T6.** Ben receives six returns, inspects each and labels four `missing-charger` as it happens: three while they are being inspected, and T4 on arrival. T1, T2 and T4 are resolved and closed. T6 is resolved and never closed.
- **The trial.** Rover 2 units leave intake while the rule is on trial:
  - Ben takes R31 through without a photo;
  - Scout takes R33 through without one;
  - Ben photographs R32 first.
- **Enforcing the rule (UC-15).** Scout drafts enforcing it, submits the change and attaches the evidence. Ana approves it on 7 December, as version 7. Two days later Ben's request to take R11 through without a photo is refused. Ben photographs it and it goes through.
- **Evidence for the label.** On 13 December, after a month of labels, the lead declares three metrics on the returns, as version 8, to see what the label says.

```yaml publish
at: 2026-12-13T00:00Z
modules:
  returns:
    add:
      types.Return.metrics.labelled_returns:
        description: The returns labelled missing-charger, by the state each was in when labelled.
        source: labels
        item: l
        filter: 'l.name == "missing-charger"'
        dimensions:
          state: l.subject_state
        expression: count(distinct l.subject)
      types.Return.metrics.labelled_wait:
        description: How long the returns labelled missing-charger spent being inspected, by the state each was in when labelled.
        source: labels
        item: l
        filter: 'l.name == "missing-charger"'
        dimensions:
          state: l.subject_state
        expression: median(l.subject.time_in(INSPECTING))
      types.Return.metrics.time_to_done:
        description: The time from receiving a return to closing it, by the month closed; every metric can be split by version.
        source: transitions
        item: t
        filter: t.to_state == Return.CLOSED and not t.migrated
        dimensions:
          month: month(t.occurred_at)
        expression: median(t.occurred_at - t.object.created_at)
```

```yaml scenario
now: 2026-12-14T00:00Z

requests:
  - { at: 2026-11-17T09:00Z, actor: U-BEN, create: Return, object: T1, transition: receive, inputs: { unit: R05, customer: C-ACME } }
  - { at: 2026-11-17T10:00Z, actor: U-BEN, object: T1, transition: inspect }
  - { at: 2026-11-18T09:00Z, actor: U-BEN, create: Return, object: T2, transition: receive, inputs: { unit: R07, customer: C-BOREAL } }
  - { at: 2026-11-18T10:00Z, actor: U-BEN, object: T2, transition: inspect }
  - { at: 2026-11-18T11:00Z, actor: U-BEN, label: missing-charger, subject: T1, object: LB2 }
  - { at: 2026-11-20T09:00Z, actor: U-BEN, create: Return, object: T3, transition: receive, inputs: { unit: R10, customer: C-BOREAL } }
  - { at: 2026-11-20T10:00Z, actor: U-BEN, object: T2, transition: resolve }
  - { at: 2026-11-21T09:00Z, actor: U-BEN, object: T3, transition: inspect }
  - { at: 2026-11-21T10:00Z, actor: U-BEN, label: missing-charger, subject: T3, object: LB3 }
  - { at: 2026-11-21T11:00Z, actor: U-BEN, object: T2, transition: close }
  - { at: 2026-11-23T09:00Z, actor: U-BEN, object: R31, transition: record_label_print }
  - { at: 2026-11-23T09:05Z, actor: U-BEN, object: R31, transition: inventorize }
  - { at: 2026-11-24T09:00Z, actor: U-BEN, create: Return, object: T4, transition: receive, inputs: { unit: L02, customer: C-BOREAL } }
  - { at: 2026-11-24T10:00Z, actor: U-BEN, label: missing-charger, subject: T4, object: LB4 }
  - { at: 2026-11-25T09:00Z, actor: U-BEN, object: T4, transition: inspect }
  - { at: 2026-11-26T09:00Z, actor: A-SCOUT, object: R33, transition: record_label_print }
  - { at: 2026-11-26T09:05Z, actor: A-SCOUT, object: R33, transition: inventorize }
  - { at: 2026-11-27T09:00Z, actor: U-BEN, object: T1, transition: resolve }
  - { at: 2026-11-28T09:00Z, actor: U-BEN, object: T1, transition: close }
  - { at: 2026-11-30T09:00Z, actor: U-BEN, object: R32, transition: add_photo, inputs: { photo: r32-label.jpg } }
  - { at: 2026-11-30T09:01Z, actor: U-BEN, object: R32, transition: record_label_print }
  - { at: 2026-11-30T09:05Z, actor: U-BEN, object: R32, transition: inventorize }
  - { at: 2026-12-01T09:00Z, actor: U-BEN, create: Return, object: T5, transition: receive, inputs: { unit: R02, customer: C-ACME } }
  - { at: 2026-12-01T10:00Z, actor: U-BEN, object: T5, transition: inspect }
  - { at: 2026-12-02T09:00Z, actor: U-BEN, label: missing-charger, subject: T5, object: LB5 }
  - { at: 2026-12-02T10:00Z, actor: U-BEN, object: T4, transition: resolve }
  - { at: 2026-12-03T09:00Z, actor: U-BEN, object: T4, transition: close }
  - { at: 2026-12-03T11:00Z, actor: U-BEN, create: Return, object: T6, transition: receive, inputs: { unit: R06, customer: C-ACME } }
  - { at: 2026-12-04T09:00Z, actor: U-BEN, object: T6, transition: inspect }
  - { at: 2026-12-05T09:00Z, actor: U-BEN, object: T6, transition: resolve }
  - { at: 2026-12-07T09:00Z, actor: A-SCOUT, draft: DC1, evidence: ["metric(trial_refusals, keep: [clause, actor])", "metric(Robot.override_counts, keep: [state, reason])"] }
  - { at: 2026-12-07T09:05Z, actor: A-SCOUT, submit: DC1 }
  - { at: 2026-12-07T15:00Z, actor: U-ANA, approve: DC1 }
  - { at: 2026-12-09T09:00Z, actor: U-BEN, object: R11, transition: record_label_print }
  - { at: 2026-12-09T09:05Z, actor: U-BEN, object: R11, transition: inventorize, refused: { verdict: unsatisfied, clause: rover_photo, remedy: unreachable_from_here } }
  - { at: 2026-12-09T10:00Z, actor: U-BEN, object: R11, transition: add_photo, inputs: { photo: r11-label.jpg } }
  - { at: 2026-12-09T10:05Z, actor: U-BEN, object: R11, transition: inventorize }
```

## 24. UC-13, the first month: a label, counted and related to states

> Service jobs start as `OPEN → DONE`. Engineers label jobs "waiting for parts" as it happens. After a month the engine shows how many jobs carry the label, in which state it was applied, and how long those jobs waited. […] *Acceptance:* the label is countable and relatable to states from the first day; […] (PRD §7, abridged; the rest is §27's)

The returns' first version has four states and no rules, no metrics and no datapoint kinds. The label needed none of them. Every type carries labels without declaring any, and a label records the state its subject was in (`declaration-syntax.md` §6.8). The lead's metrics were declared a month after the first label, and a metric is computed on read, so they count every label from the first day:

Read: `metric(labelled_returns, keep: [state])`

| state | value | gaps |
|---|---|---|
| INSPECTING | 3 | |
| RECEIVED | 1 | |

**How long those returns waited.** Each labelled return's total time in `INSPECTING`, read through the label's subject:
- **Labelled while inspecting:**
  - T1: 9d 23h, until it was resolved.
  - T3: 22d 15h, still being inspected at `now`.
  - T5: 12d 14h, still being inspected.
  - The median of the three is 12d 14h.
- **Labelled on arrival:** T4, 7d 1h.

Read: `metric(labelled_wait, keep: [state])`

| state | value | gaps |
|---|---|---|
| INSPECTING | 12d 14h | |
| RECEIVED | 7d 1h | |

Beside every return's time being inspected, with nothing declared:

Read: `metric(Return.time_in_state, keep: [state], filter: "i.state == Return.INSPECTING")`

| state | value | gaps |
|---|---|---|
| INSPECTING | 7d 1h | |

The six stays in `INSPECTING` are 1d, 2d, 7d 1h, 9d 23h, 12d 14h and 22d 15h, so the median is the third. A return labelled while inspecting waits a median 12d 14h against 7d 1h over all six. That is the evidence the team promotes the label on (§27).

## 25. UC-14: trial a rule

> A proposed rule — a photo is required before a unit of one model becomes `AVAILABLE` — runs for two weeks without enforcing. *Acceptance:* the refusals it would have made are recorded and countable, including who would have been refused; enforcing it is a flow change. (PRD §7)

**The refusals it would have made are recorded, and countable by who.** An audited clause is evaluated on every request, and where it fails the request goes through and the would-be refusal is recorded in the attempt log (`DESIGN.md` §5.5). R31's and R33's requests to leave intake went through, and each left a record naming the rule and who asked. R32, photographed first, left none.

Read: `metric(trial_refusals, keep: [clause, actor])`

| clause | actor | value | gaps |
|---|---|---|---|
| rover_photo | ben | 1 | |
| rover_photo | scout | 1 | |

`actor` names the person or agent who asked, by the identity they act under (ADR-0122 decision 21). A dimension that names an actor is marked in the declaration a reader reads, so an upper layer can decide who may see it (PRD T7).

**Enforcing it is a flow change.** The rule was enforced by version 7, a governed change (§26). From then it refuses: R11's request, without a photo, was refused on 9 December. The standard `refusals` counts enforced refusals only, so the trial's two are not in it:

Read: `metric(Robot.refusals, keep: [clause, remedy], filter: "a.clause == \"rover_photo\"")`

| clause | remedy | value | gaps |
|---|---|---|---|
| rover_photo | unreachable_from_here | 1 | |

## 26. UC-15: an agent drafts a change

> An agent reads UC-2's and UC-3's evidence and drafts a flow change. The impact report says what it would affect; a person approves it, the upper layer deciding who may; it is published and attributed to both. *Acceptance:* no flow change applies without passing the governed path; the evidence and the approval are part of the record. (PRD §7)

Scout drafts the change that enforces the trial rule. It attaches its evidence:
- the trial's would-be refusals (§25);
- UC-3's override counts, which show units of a model whose photo rule did not fit, put on offer by override (§5).

A flow changes only through a `DeclarationChange`: drafted, submitted, which attaches its impact report, and approved, which publishes it (`publish-and-import.md` §1). The change's source is the publish below, applied only when the change is approved.

```yaml publish
change: DC1
modules:
  inventory_journey:
    add:
      machines.UnitLifecycle.transitions.inventorize.guards.rover_photo: deny
```

**The impact report says what it would affect.** A submit runs the publish's checks as a dry run and reads the live objects. Among what it reads are "which a clause the change stops observing would now refuse" (`publish-and-import.md` §1, step 3). One unit was in intake without a photo, R11. At approval the report is computed again, and the approval is refused as `impact_unchanged` if it now names anything the attached one did not; it did not.

Read: `impact of DC1`

| object | transition | clause |
|---|---|---|
| R11 | inventorize | rover_photo |

**The evidence and the approval are part of the record.** The change records who drafted it, who approved it, the kind of each, and the version it published. It also keeps the evidence it was drafted with, the values as they stood on 7 December.

Read: `change DC1`

| drafted_by | drafted_kind | approved_by | approved_kind | version |
|---|---|---|---|---|
| scout | agent | ana | human | 7 |

Read: `evidence of DC1`

| read | group | value |
|---|---|---|
| metric(trial_refusals, keep: [clause, actor]) | clause = rover_photo, actor = ben | 1 |
| metric(trial_refusals, keep: [clause, actor]) | clause = rover_photo, actor = scout | 1 |
| metric(Robot.override_counts, keep: [state, reason]) | state = AVAILABLE, reason = SUPPLIER_EXCEPTION | 4 |
| metric(Robot.override_counts, keep: [state, reason]) | state = AVAILABLE, reason = MIS_SCANNED | 1 |
| metric(Robot.override_counts, keep: [state, reason]) | state = DEVELOPMENT, reason = LEGACY_DATA | 1 |

**No flow change applies without passing the governed path.** Version 7 exists only because Ana approved DC1: the publish above names its change and has no time of its own, and the script applies it at the approval and at nothing else. Who may approve, and that the approver is not the drafter, are the upper layer's (PRD F7, ADR-0114).

## 27. UC-13, refined: a state promoted, and a state dropped

> […] The team publishes a `WAITING_PARTS` state with its transitions, and moves the jobs in flight into it. Later a version drops a state nobody uses any more, and the few jobs still in it are moved by a declared mapping. *Acceptance:* […] the new version moves in-flight jobs by recorded transitions; the jobs in a removed state are moved by the declared mapping, and each move is recorded; time-to-done is comparable across the two versions. (PRD §7, abridged)

On 14 December the team publishes the returns' second version, as `returns-module.md` §2 writes it. It adds:
- `AWAITING_PARTS`, with `await_parts` and `parts_in`;
- who received each return;
- how often the unit came back;
- a rule on replacements.

The lead's three metrics are declared on it again, since a version is the whole module. This is version 9.

```yaml publish
at: 2026-12-14T01:00Z
modules:
  returns:
    text: returns-module.md 2
    add:
      types.Return.metrics.labelled_returns:
        description: The returns labelled missing-charger, by the state each was in when labelled.
        source: labels
        item: l
        filter: 'l.name == "missing-charger"'
        dimensions:
          state: l.subject_state
        expression: count(distinct l.subject)
      types.Return.metrics.labelled_wait:
        description: How long the returns labelled missing-charger spent being inspected, by the state each was in when labelled.
        source: labels
        item: l
        filter: 'l.name == "missing-charger"'
        dimensions:
          state: l.subject_state
        expression: median(l.subject.time_in(INSPECTING))
      types.Return.metrics.time_to_done:
        description: The time from receiving a return to closing it, by the month closed; every metric can be split by version.
        source: transitions
        item: t
        filter: t.to_state == Return.CLOSED and not t.migrated
        dimensions:
          month: month(t.occurred_at)
        expression: median(t.occurred_at - t.object.created_at)
```

**Version 10 drops `RESOLVED`.** A resolved return waited in `RESOLVED` until someone asked to close it, and T6 had waited three weeks. On 28 December version 10 drops the state: `resolve` closes a return at once. Its migration moves the returns still resolved into `CLOSED`, each by a recorded migration (`flow-format.md` §4.16).

```yaml publish
at: 2026-12-28T00:00Z
modules:
  returns:
    remove: [types.Return.states.RESOLVED, types.Return.transitions.close]
    add:
      types.Return.transitions.resolve.to: CLOSED
      migration:
        description: Version 10 drops RESOLVED, since a resolved return was only ever waiting to be closed, and closes the returns still in it.
        removed_states:
          Return: { RESOLVED: CLOSED }
```

**What happens.**
- **The in-flight returns move by recorded transitions.** T3 and T5, labelled and still being inspected, are moved into `AWAITING_PARTS` by `await_parts`. T3's parts come in on the 18th, and it is resolved the next day, into `RESOLVED`.
- **T7** arrives and is resolved and closed under version 9.
- **Version 10's mapping** moves T3 and T6 from `RESOLVED` into `CLOSED`.
- **Under version 10:** T8 is resolved straight into `CLOSED`, and so is T5, when its parts come in in January.

```yaml scenario
now: 2027-01-11T00:00Z

requests:
  - { at: 2026-12-14T02:00Z, actor: U-BEN, object: T3, transition: await_parts }
  - { at: 2026-12-14T02:05Z, actor: U-BEN, object: T5, transition: await_parts }
  - { at: 2026-12-18T09:00Z, actor: U-BEN, object: T3, transition: parts_in }
  - { at: 2026-12-19T09:00Z, actor: U-BEN, object: T3, transition: resolve, inputs: { outcome: REPAIRED } }
  - { at: 2026-12-20T09:00Z, actor: U-BEN, create: Return, object: T7, transition: receive, inputs: { unit: L06, customer: C-ACME } }
  - { at: 2026-12-21T09:00Z, actor: U-BEN, object: T7, transition: inspect }
  - { at: 2026-12-22T09:00Z, actor: U-BEN, object: T7, transition: resolve, inputs: { outcome: REPAIRED } }
  - { at: 2026-12-23T09:00Z, actor: U-BEN, object: T7, transition: close }
  - { at: 2026-12-30T09:00Z, actor: U-BEN, create: Return, object: T8, transition: receive, inputs: { unit: R03, customer: C-ACME } }
  - { at: 2026-12-30T10:00Z, actor: U-BEN, object: T8, transition: inspect }
  - { at: 2027-01-02T09:00Z, actor: U-BEN, object: T8, transition: resolve, inputs: { outcome: REPAIRED } }
  - { at: 2027-01-04T09:00Z, actor: U-BEN, object: T5, transition: parts_in }
  - { at: 2027-01-05T09:00Z, actor: U-BEN, object: T5, transition: resolve, inputs: { outcome: REPAIRED } }
```

**Each move of a removed state is recorded.** T6, resolved on 5 December and never closed, was moved by the mapping, and its history shows the move as an event of its own:

Read: `events(T6)`

| transition | occurred_at | recorded_at |
|---|---|---|
| receive | 2026-12-03T11:00Z | 2026-12-03T11:00Z |
| inspect | 2026-12-04T09:00Z | 2026-12-04T09:00Z |
| resolve | 2026-12-05T09:00Z | 2026-12-05T09:00Z |
| migrate | 2026-12-28T00:00Z | 2026-12-28T00:00Z |

**The new state is measured from its first day.**

Read: `metric(Return.time_in_state, keep: [state], filter: "i.state == Return.AWAITING_PARTS")`

| state | value | gaps |
|---|---|---|
| AWAITING_PARTS | 4d 7h | |

T3 waited 4d 7h for its parts, and T5 21d 6h 55m. The median of two is the lower.

**Time-to-done is comparable across the versions.** Every metric can be split by the declaration version its rows were recorded under (PRD M3). A declaration version counts every publish of the store, not each version of one module (`DESIGN.md` §5.9). So the returns' three versions are declaration versions 6, 9 and 10:
- version 6 is the first returns version, with the rule on trial;
- versions 7 and 8, in between, changed the robot's rule and the returns' metrics, and no return closed under them;
- the migration's moves are not the flow, and `time_to_done` leaves them out, as the standard metrics do (`declaration-syntax.md` §6.11).

Read: `metric(time_to_done, keep: [version])`

| version | value | gaps |
|---|---|---|
| 6 | 9d | |
| 9 | 3d | |
| 10 | 3d | |

The values for each version:
- **Version 6:** T2 took 3d 2h, T4 9d and T1 11d, so the median is 9d.
- **Version 9:** T7 took 3d.
- **Version 10:** T8 took 3d and T5, which waited for parts, 35d. The median of two is the lower, 3d.

## 28. What is inferred, and what UC-13 to UC-15 found

**Inferences**, each resting on a requirement or a reviewed or checked flow:
1. **UC-13's service jobs are the returns of `returns-module.md`** (UC-13). The journey's service job has its working state already. The returns module is the design's own worked example of a flow that starts with no rules and promotes a label, and `traceability.md` cites it for UC-13. Its two versions are installed exactly as written, `text: returns-module.md 1` and `2`, so the check that document's modules pass is the check these versions pass.
2. **The state nobody uses is `RESOLVED`** (UC-13). A resolved return only waits to be closed, and nothing reads the state. Dropping it closes a return at resolution.
3. **"The two versions" are the returns' declaration versions** (UC-13), 6, 9 and 10, since a declaration version counts the store's publishes, not a module's.
4. **The trial rule reads the model's name** (UC-14), `model.name != "Rover 2"`. The rule is about "a unit of one model", and the model's photo flag would enforce, not trial. Its remedy is `unreachable_from_here`, since the way through is another transition, `add_photo` (ADR-0122 decision 30).
5. **The agent's evidence is the trial's refusals and UC-3's override counts** (UC-15). UC-15 names UC-2's and UC-3's evidence, and the change drafted here is about a photo rule, which UC-3's overrides bear on and UC-2's refusals by `filled` do not.
6. **The earlier publishes were changes too.** `publish-and-import.md` §1 makes every publish the approval of a `DeclarationChange`. The scenario writes the earlier ones by their time alone, and writes this one with its drafting, since that is what UC-15 tests.

**Findings.**
1. **A version split separates every publish of the store** (determined). `time_to_done` by version distinguishes declaration versions, and a publish that changed another module starts a new one. Comparing "the two versions" of one flow means knowing which declaration versions changed it, which the publish history records. Nothing is missing, but a reader must know the split is by the store's version.
2. **A label metric can reach the subject's time in a state** (determined). `labelled_wait` reads `l.subject.time_in(INSPECTING)` from a label row, and the flow checker accepts it in all four steps. A label is readable only as a metric's source, and this is how a label is related to how long its subject waited without reading the label anywhere else.
3. **The impact report of a newly enforced clause** is "which a clause the change stops observing would now refuse" (`publish-and-import.md` §1, step 3). The scenario's dry run lists exactly those objects, R11, and checks at approval that the list has not changed. That is the only kind of change the scenario drafts, so the script computes no other kind of impact.

## 29. January 2027: service jobs change hands

The history continues from `now` of §27. The reads of §30 and §31 are taken at `2027-01-25T00:00Z`, a Monday. The reads of §3 to §27 hold as written, at their own `now`.

**What the first part ported, and nothing had read.**
- **Chen**, an engineer, a user of the legacy system.
- **SJ1**, a repair still open. The legacy audit trail shows it unassigned for its first two days, then Ben's, then Chen's from 29 August. The port brings those as legacy intervals of `engineer` (`publish-and-import.md` §4, "Intervals").

On 11 January the lead publishes version 11. It adds a split of delivery cycle time by who opened the delivery (UC-16), and makes a reassignment proposable, so an agent can propose one for a person to approve (UC-18).

```yaml publish
at: 2027-01-11T01:00Z
modules:
  inventory_journey:
    add:
      types.ServiceJob.transitions.reassign.proposable: true
      types.Delivery.metrics.delivery_cycle_time:
        description: The time from opening a delivery to delivering it, by the kind of actor that opened it.
        source: transitions
        item: t
        filter: t.to_state == Delivery.DELIVERED and t.from_state == Delivery.PREPARATION
        dimensions:
          creator: t.object.created_by_kind
        expression: median(t.occurred_at - t.object.created_at)
```

**What happens.**
- **Diya joins**, added by Ana.
- **Ana opens four service jobs:**
  - J1, a maintenance job for Ben, which Ben starts and finishes.
  - J2, a repair for Chen. It moves to Diya, then back to Chen; Chen starts it and Ana finishes it.
  - J3, an upgrade for Ben on a unit in the development pool, not yet started.
  - J4, a maintenance job for Diya.
- **A proposal:** Scout, balancing the workload, proposes moving J4 to Ben, and Ana approves.

```yaml scenario
now: 2027-01-25T00:00Z

requests:
  - { at: 2027-01-11T02:00Z, actor: U-ANA, create: User, object: U-DIYA, transition: add, inputs: { login: diya } }
  - { at: 2027-01-12T09:00Z, actor: U-ANA, create: ServiceJob, object: J1, transition: open, inputs: { kind: MAINTENANCE, robot: R05, customer: C-ACME, engineer: U-BEN } }
  - { at: 2027-01-12T10:00Z, actor: U-ANA, create: ServiceJob, object: J2, transition: open, inputs: { kind: REPAIR, robot: R07, customer: C-BOREAL, engineer: U-CHEN } }
  - { at: 2027-01-13T09:00Z, actor: U-BEN, object: J1, transition: start }
  - { at: 2027-01-13T10:00Z, actor: U-ANA, create: ServiceJob, object: J3, transition: open, inputs: { kind: UPGRADE, robot: R08, customer: C-ACME, engineer: U-BEN } }
  - { at: 2027-01-14T09:00Z, actor: U-ANA, object: J2, transition: reassign, inputs: { engineer: U-DIYA } }
  - { at: 2027-01-14T10:00Z, actor: U-ANA, create: ServiceJob, object: J4, transition: open, inputs: { kind: MAINTENANCE, robot: L03, customer: C-BOREAL, engineer: U-DIYA } }
  - { at: 2027-01-15T09:00Z, actor: U-BEN, object: J1, transition: finish }
  - { at: 2027-01-18T09:00Z, actor: U-ANA, object: J2, transition: reassign, inputs: { engineer: U-CHEN } }
  - { at: 2027-01-19T09:00Z, actor: U-CHEN, object: J2, transition: start }
  - { at: 2027-01-20T10:00Z, actor: U-ANA, object: J2, transition: finish }
  - { at: 2027-01-20T11:00Z, actor: A-SCOUT, propose: PR1, object: J4, transition: reassign, inputs: { engineer: U-BEN } }
  - { at: 2027-01-21T09:00Z, actor: U-ANA, approve_proposal: PR1 }
```

## 30. UC-16: people and agents on the same flow

> The lead compares cycle time, refusal rate and rework for deliveries created by agents with those created by people. *Acceptance:* every metric can be split by the kind of actor. (PRD §7)

**Every metric can be split by the kind of actor.** Every metric has an `actor_kind` dimension, taken from its rows (`declaration-syntax.md` §6.9). Which actor that is depends on the row:
- the object's creator, for a row that is an object;
- whoever entered a stay, for an interval;
- whoever requested it, for a transition or a refusal.

UC-16's lead asks about deliveries *created* by agents. A transition row's `actor_kind` is its requester's, so the lead's cycle time reads the creator through the row's object, `t.object.created_by_kind`.

**Cycle time, by who opened the delivery.**
- **People opened seven of the delivered ones.** Their times from opening to delivery were 1d, 1d 1h, 2d, 3d 12h, 4d 10h, 5d 9h and 5d 19h, with a median of 3d 12h.
- **Scout opened D14**, which took 11d 2h, held by the sign-off of §19.
- **D01's creator is `unknown`.** It was ported, and its mapping did not say who opened it (`publish-and-import.md` §4, "Created").

Read: `metric(delivery_cycle_time, keep: [creator])`

| creator | value | gaps |
|---|---|---|
| human | 3d 12h | |
| agent | 11d 2h | |
| unknown | 15d | |

**Refusal rate, by who asked.** The standard `refusal_rate` combines the refusals and the transitions taken (`declaration-syntax.md` §6.11). Bound to `complete_sale` and kept by `actor_kind`, each input is counted over the rows the binding selects and the kept dimension groups, and the two are then combined (§33):
- **Agents** asked to complete a delivery seven times and were refused each time, by `filled` six times and by `all_checked` once.
- **People** were refused four times and completed eight: 4 in 12, 0.333.

Read: `metric(Delivery.refusal_rate, bind: {transition: complete_sale}, keep: [actor_kind])`

| actor_kind | value | gaps |
|---|---|---|
| agent | 1.000 | |
| human | 0.333 | |

**Rework.** In the journey a delivery never returns to a state it held, so the standard `rework` has no rows to split. Kept by `actor_kind`, the read is empty rather than zero, since no row exists:

Read: `metric(Delivery.rework, keep: [actor_kind])`

| actor_kind | value | gaps |
|---|---|---|

**The same split on a standard metric.** Throughput counts completions by who requested them, and every delivery completion was a person's, including D14's, which Scout opened:

Read: `metric(Delivery.throughput, keep: [state, actor_kind])`

| state | actor_kind | value | gaps |
|---|---|---|---|
| DELIVERED | human | 9 | |

## 31. UC-18: who has what, and where do handoffs hurt?

> Service jobs have an engineer-of-record, which an agent may not be, and a user with active services must have them reassigned before being removed. A service lead asks how much open work each engineer holds, how long jobs wait before anyone is assigned, which jobs changed hands more than once or went back to an earlier engineer, whether cycle time differs by engineer, and how often a job is completed by someone other than its assignee. An agent balancing the workload reads the same numbers before proposing a reassignment, and a person makes it. *Acceptance:* every assignment from the first day the flow runs, including the one made at creation, is recorded with who made it; every question above is a standard metric for any type that declares an assignee; the legacy system's reassignments arrive with the port wherever its audit trail recorded them; choosing the engineer stays with the person or agent. (PRD §7, abridged)

The journey's `ServiceJob` marks `engineer` as its assignee, a reference to a `User`, so an agent, an `Agent`, cannot be one (`declaration-syntax.md` §6.10). Each question is one of the standard assignment metrics every type with an assignee has, read by name with the assignee's suffix (§6.11).

**Every assignment is recorded, with who made it, from the one at creation.** The interval index keeps each value `engineer` held, from the job's creation, and who set it:

Read: `intervals(J2.engineer)`

| value | entered_at | entered_by_kind | legacy |
|---|---|---|---|
| U-CHEN | 2027-01-12T10:00Z | human | false |
| U-DIYA | 2027-01-14T09:00Z | human | false |
| U-CHEN | 2027-01-18T09:00Z | human | false |

**The legacy system's reassignments arrive with the port.** SJ1's two days unassigned and its engineers before the port are legacy intervals. Its current engineer's interval begins when the legacy record says Chen took it, not at the port:

Read: `intervals(SJ1.engineer)`

| value | entered_at | entered_by_kind | legacy |
|---|---|---|---|
| absent | 2026-08-24T00:00Z | human | true |
| U-BEN | 2026-08-26T00:00Z | human | true |
| U-CHEN | 2026-08-29T00:00Z | human | false |

**How much open work each engineer holds.** SJ1 is Chen's. J3 and J4 are Ben's, J4 since the proposal:

Read: `metric(ServiceJob.open_work.engineer, keep: [assignee])`

| assignee | value | gaps |
|---|---|---|
| U-BEN | 2 | |
| U-CHEN | 1 | |

**How long jobs wait before anyone is assigned.** The journey requires an engineer when a job is opened, so every job opened in the store is assigned at once. Only SJ1, ported, waited: two days, in August.

Read: `metric(ServiceJob.time_to_first_assignment.engineer, keep: [month])`

| month | value | gaps |
|---|---|---|
| 2026-08 | 2d | |
| 2027-01 | 0 | |

**Which jobs changed hands more than once, or went back to an earlier engineer.** Handoffs are the engineers a job has had, less one. A return is a job going back to an engineer it had before. J2 changed hands twice and went back to Chen, so it is the one both flags name: `changed_hands_more_than_once` when the handoffs are more than 1, and `returned_to_earlier` when the returns are more than 0.

Read: `metric(ServiceJob.handoffs_by_object.engineer, keep: [object])`

| object | value | gaps |
|---|---|---|
| SJ1 | 1 | |
| J1 | 0 | |
| J2 | 2 | |
| J3 | 0 | |
| J4 | 1 | |

Read: `metric(ServiceJob.returns_by_object.engineer, keep: [object])`

| object | value | gaps |
|---|---|---|
| SJ1 | 0 | |
| J1 | 0 | |
| J2 | 1 | |
| J3 | 0 | |
| J4 | 0 | |

**Whether cycle time differs by engineer.** Cycle time is attributed to whoever held the job when it completed:
- Ben finished J1 three days after it was opened.
- J2 was Chen's when Ana finished it, eight days after it was opened.

Read: `metric(ServiceJob.cycle_time_by_assignee.engineer, keep: [assignee])`

| assignee | value | gaps |
|---|---|---|
| U-BEN | 3d | |
| U-CHEN | 8d | |

**How often someone other than the assignee acts.** Each transition on a job with an engineer counts as the assignee's or someone else's. The assignee is the engineer the job held before the transition's own writes, and a creation has none.
- **Reassignments:** Ana made all three, and none of the jobs was Ana's.
- **Starts:** each engineer started their own.
- **Finishes:** Ben finished J1, and Ana finished J2, which was Chen's.

Read: `metric(ServiceJob.acted_by_non_assignee.engineer, keep: [transition])`

| transition | value | gaps |
|---|---|---|
| reassign | 1.000 | |
| start | 0.000 | |
| finish | 0.500 | |

**An agent reads the same numbers and proposes; a person makes it.** Scout reads `open_work` as anyone does and proposes moving J4 from Diya to Ben. A proposal is a request filed for approval, and approving it executes the request as the approver (`DESIGN.md` §9). So the reassignment is Ana's, and the interval index records it as a person's. Which engineer, and whether to propose at all, are Scout's and Ana's, not the engine's.

Read: `proposal PR1`

| proposed_by | proposed_kind | approved_by | approved_kind | state |
|---|---|---|---|---|
| scout | agent | ana | human | executed |

## 32. UC-17: erase a customer

> A customer asks to be erased. *Acceptance:* their personal values are removed from objects, history and datapoints; metrics built from those datapoints keep their counts and expose no personal value. (PRD §7)

The journey's customer is a mirror of the legacy system's, and held no personal value. On 25 January the lead publishes version 12. It gives a customer who is a person a contact name and email, both `personal`, and an erasure, which a mirror may declare (`declaration-syntax.md` §2; `flow-format.md` §4.13):

```yaml publish
at: 2027-01-25T01:00Z
modules:
  operations_shared:
    add:
      types.Customer.attributes.contact_name: { type: string, optional: true, personal: true }
      types.Customer.attributes.contact_email: { type: string, optional: true, personal: true }
      types.Customer.transitions:
        forget:
          kind: erasure
          description: Erases a private customer's contact details, at their request.
          inputs:
            reason: { type: string }
```

**Dana Lim**, a private customer, arrives with the legacy system's next refresh of the customers, since only the import writes a mirror. Dana buys a Rover 2, R31, on delivery D18. Ben's first result for its drive test carries a remark naming Dana.

```yaml scenario
now: 2027-02-01T00:00Z

imported:
  - { object: C-DANA, type: Customer, state: ACTIVE, at: 2027-01-26T00:00Z, attributes: { legacy_key: "C-1003", name: Private customer C-1003, contact_name: Dana Lim, contact_email: dana@example.net } }

requests:
  - { at: 2027-01-26T09:00Z, actor: U-BEN, create: Delivery, object: D18, transition: open, inputs: { customer: C-DANA, promised_date: 2027-02-05T00:00Z } }
  - { at: 2027-01-26T10:00Z, actor: U-BEN, object: D18, transition: bind_slot, inputs: { robot: R31 } }
  - { at: 2027-01-27T09:00Z, actor: U-BEN, record: pdi_results, subject: D18, object: Q10, fields: { unit: R31, check: K3, outcome: PASS, remark: Dana Lim asked for the drive test to be filmed } }
  - { at: 2027-01-27T09:10Z, actor: U-BEN, record: pdi_results, subject: D18, object: Q11, fields: { unit: R31, check: K4, outcome: FAIL } }
  - { at: 2027-01-27T15:00Z, actor: U-BEN, record: pdi_results, subject: D18, object: Q12, fields: { unit: R31, check: K4, outcome: PASS } }
  - { at: 2027-01-28T09:00Z, actor: U-BEN, object: D18, transition: complete_sale }
```

**Before the erasure**, on 1 February:

Read: `get(C-DANA)`

| attribute | value |
|---|---|
| contact_email | dana@example.net |
| contact_name | Dana Lim |
| legacy_key | C-1003 |
| name | Private customer C-1003 |

Read: `results(D18)`

| object | unit | check | outcome | remark |
|---|---|---|---|---|
| Q10 | R31 | K3 | PASS | Dana Lim asked for the drive test to be filmed |
| Q11 | R31 | K4 | FAIL | absent |
| Q12 | R31 | K4 | PASS | absent |

The results are counted by first-pass yield (§15). R31 failed its lidar check at first, so January's Rover 2 yield is 0.000:

Read: `metric(first_pass_yield, keep: [model, month])`

| model | month | value | gaps |
|---|---|---|---|
| M-ARM | 2026-10 | 0.500 | |
| M-ROVER | 2026-10 | 1.000 | |
| M-ARM | 2026-11 | 1.000 | |
| M-ROVER | 2026-11 | 1.000 | |
| M-ROVER | 2027-01 | 0.000 | |

**The erasure.** Dana asks to be erased, and on 2 February Ana requests the customer's `forget`. The remark on D18's result is personal too, but it is a datapoint of the delivery, not of the customer, so erasing the customer does not reach it. Ana removes it with the `forget` every datapoint has, requested on that one result, "how a personal value recorded about someone other than the subject is removed without erasing the subject" (`declaration-syntax.md` §6.8).

```yaml scenario
now: 2027-02-08T00:00Z

requests:
  - { at: 2027-02-02T09:00Z, actor: U-ANA, object: C-DANA, transition: forget, inputs: { reason: At the customer's request } }
  - { at: 2027-02-02T09:05Z, actor: U-ANA, forget: Q10, inputs: { reason: The remark names the customer } }
```

**Their personal values are removed from the object, from history and from datapoints.** Erasure replaces each personal value with absence, on the object and in every event that carried it (`DESIGN.md` §8):
- **The object:** the customer keeps its legacy key and the legacy system's label, which are not personal.
- **History:** the import event that brought the customer's details now carries none.
- **The datapoint:** the result keeps its unit, check and outcome, and loses its remark.

Read: `get(C-DANA)`

| attribute | value |
|---|---|
| contact_email | absent |
| contact_name | absent |
| legacy_key | C-1003 |
| name | Private customer C-1003 |

Read: `payloads(C-DANA)`

| transition | attribute | value |
|---|---|---|
| import | contact_email | absent |
| import | contact_name | absent |
| import | legacy_key | C-1003 |
| import | name | Private customer C-1003 |

Read: `results(D18)`

| object | unit | check | outcome | remark |
|---|---|---|---|---|
| Q10 | R31 | K3 | PASS | absent |
| Q11 | R31 | K4 | FAIL | absent |
| Q12 | R31 | K4 | PASS | absent |

**Metrics keep their counts and expose no personal value.** A metric counts rows, and erasure removes no row and changes no value a metric reads. A metric reads no personal field, and step 4 refuses one that does (check 56), so no metric could have exposed the remark:

Read: `metric(first_pass_yield, keep: [model, month])`

| model | month | value | gaps |
|---|---|---|---|
| M-ARM | 2026-10 | 0.500 | |
| M-ROVER | 2026-10 | 1.000 | |
| M-ARM | 2026-11 | 1.000 | |
| M-ROVER | 2026-11 | 1.000 | |
| M-ROVER | 2027-01 | 0.000 | |

## 33. What is inferred, and what UC-16 to UC-18 found

**Inferences**, each resting on a requirement or the reviewed flow:
1. **"Created by agents" reads the creator** (UC-16), `t.object.created_by_kind`, since a transition's own `actor_kind` is its requester's. The lead's cycle time is declared for it; the standard metrics split by who requested.
2. **Cycle time is from opening to delivery** (UC-16), since the journey has no standard cycle time for a type without an assignee.
3. **The service job is the journey's, and its engineers are users** (UC-18), as the journey declares. Chen, the ported job's legacy history and Diya are this document's.
4. **An agent's reassignment is a proposal a person approves** (UC-18): "proposing a reassignment, and a person makes it". A proposal is the model's way to file a request for someone else's approval, so `reassign` is published as `proposable`.
5. **A private customer's contact details are personal, and a mirror's refresh brings them** (UC-17). The journey's customer is a company in the legacy system. A person as a customer needs personal attributes, which an erasure can reach, and only the import writes a mirror.
6. **A remark naming the customer is forgotten on its own** (UC-17), since it is the delivery's datapoint, and the model's way to remove a personal value about someone other than the subject is `forget` on that datapoint.

**Findings.**
1. **Reading a combined metric by fewer dimensions than it groups by, or with one bound, was unstated** (D449, ADR-0124). A reader keeps or binds dimensions, and "aggregating over the rest" cannot mean summing or averaging the combined values: a rate of rates is not a rate. The read of `refusal_rate` above counts each input over the rows the read selects and combines them after, as ADR-0122 decision 6 already does for a combined metric's own `group_by`. ADR-0124 decides it.
2. **Finding what names a person is not the engine's** (UC-17, determined). Erasing a customer reaches the customer's attributes and every event that carried them. A remark about the customer on another object's datapoint is reached only by a `forget` requested on it. Nothing in the store indexes which free text names whom, so whoever handles an erasure request finds those datapoints. The journey's remarks are free text, and a flow that records who a remark is about in a reference would let a request find them.
3. **In the journey, time unassigned exists only in ported history** (UC-18, determined). `ServiceJob.open` requires an engineer, so every job opened in the store is assigned at creation. `time_to_first_assignment` measures the legacy system's habit, not the store's.
4. **"Reassign before removal" is not in the journey** (UC-18). `User.leave` has no guard, and the journey says why: "a guard reading the journey's service jobs would make the two modules import each other" (`unit-journey.md` §2, "What the journey shares"), which ADR-0117 rejects as "the same cycle one level up". *(Corrected 2026-09-28: the version of this finding committed on 2026-09-27 said the specification does not say whether two modules may import each other, replacing a citation of the journey that a truncated search had failed to find. The citation was right, and is restored.)* UC-18's acceptance does not ask for it, and `TODO.md` holds the question.
5. **A quotient in a metric has no stated rounding** (UC-16). `refused * 1.000 / (refused + applied)` has the decimal's scale, three places (`declaration-syntax.md` §8.3). The model says a quotient rounds half to even only where it is `set` into an attribute, or where it divides two like quantities. The scenario's rates, 4 in 12 and 7 in 7, come out the same whether the fourth place is rounded or cut; 4 in 11 would not. `TODO.md` holds the question.

## 34. February 2027: a rule on trial, and an agent that skips steps

The history continues from `now` of §32. The reads of §35 are taken at `2027-02-22T00:00Z`, a Monday. The reads of §3 to §32 hold as written, at their own `now`.

**A second rule on trial.** §12 left open whether a failed check should block a delivery, since the gate asks only that every check have a result. On 8 February the lead puts the stricter rule on trial as version 13: every active check of every unit bound has a result that is not a failure, audited on `complete_sale`. A failure that a later result passes, as R02's battery did in §8, satisfies it. With it comes the count of the refusals a rule on trial would have made on a delivery, as §23 declared it for the unit, under a name of its own, since a module's metrics share one namespace (`flow-format.md` §4.9).

```yaml publish
at: 2027-02-08T01:00Z
modules:
  inventory_journey:
    add:
      types.Delivery.conditions.all_passed:
        description: Every active check of every unit bound has a result that is not a failure.
        expression: >-
          all(u in units: all(c in PdiCheck where c.model == u.model and c.state == PdiCheck.ACTIVE:
              any(r in pdi_results where r.unit == u and r.check == c and r.outcome != CheckOutcome.FAIL)))
        remedy: unreachable_from_here
      types.Delivery.transitions.complete_sale.guards.all_passed: audit
      types.Delivery.metrics.delivery_trial_refusals:
        description: The refusals a rule on trial would have made on a delivery, by the rule and by who would have been refused.
        source: attempts
        item: a
        filter: not a.enforced
        dimensions:
          clause: a.clause
          actor: a.actor_id
        expression: count()
```

**What happens.** Scout, the agent that kept asking to complete deliveries before their units had arrived (§4), tries each route UC-19 names:
- **D17 and the rule on trial.** Ben records L04's drive test as a pass and its lidar check as a failure. The next day Scout asks to complete D17. The gate holds, since every check has a result, and the rule on trial fails.
- **D10 and its lidar check.** D10 has waited since October for R23's lidar result (§8). Scout asks the floor application to record a pass for it, and then to retire the lidar check. The application refuses both, since at this site an engineer records a result and a person maintains the checklists. Neither request reaches the engine. Scout then records the pass through the procurement tool, a second application it works through, which forwards any recording, and asks to complete D10.
- **S2 and its receipt.** S2, due on 10 November (§21), arrives on 14 February. The next morning Buyer records its arrival and dates R40's receipt to 10 November, which would make R40's lead time 7d 21h. The engine refuses it, and Buyer records the receipt as it happened.

```yaml scenario
now: 2027-02-22T00:00Z

requests:
  - { at: 2027-02-08T09:00Z, actor: U-BEN, record: pdi_results, subject: D17, object: Q13, fields: { unit: L04, check: K3, outcome: PASS } }
  - { at: 2027-02-08T09:10Z, actor: U-BEN, record: pdi_results, subject: D17, object: Q14, fields: { unit: L04, check: K4, outcome: FAIL } }
  - { at: 2027-02-09T09:00Z, actor: A-SCOUT, object: D17, transition: complete_sale }
  - { at: 2027-02-10T09:00Z, actor: A-SCOUT, record: pdi_results, subject: D10, object: Q15, fields: { unit: R23, check: K4, outcome: PASS } }
  - { at: 2027-02-10T09:05Z, actor: A-SCOUT, object: D10, transition: complete_sale }
  - { at: 2027-02-15T09:00Z, actor: A-BUYER, object: S2, transition: arrive, occurred_at: 2027-02-14T16:00Z }
  - { at: 2027-02-15T09:05Z, actor: A-BUYER, object: S2, transition: receive_unit, inputs: { robot: R40 }, occurred_at: 2026-11-10T00:00Z, refused: { verdict: unsatisfied, clause: occurred_within, remedy: self_serviceable } }
  - { at: 2027-02-15T09:10Z, actor: A-BUYER, object: S2, transition: receive_unit, inputs: { robot: R40 }, occurred_at: 2027-02-14T16:00Z }
```

## 35. UC-19: the new paths do not open a way around the rules

> An agent that guesses and skips steps tries the routes this document adds. It records a passing inspection result it is not permitted to record, so that a delivery's gate would open. It backdates a receipt beyond the declared bound, to shorten a supplier's lead time. It completes a transition that a rule on trial would have refused, and then relies on that rule as if it were enforced. It retires a check from a configuration's checklist so that a delivery's gate opens. *Acceptance:* the first and the fourth are refused by the application the agent works through, since who may record a result or retire a check is its decision (N7); where one is let through, the engine records it as the agent's, attributed and counted like any request, and a later gate reads a result the record shows the agent recorded. The second is refused by the engine; the third proceeds, is recorded as a would-be refusal, and the printed rules show the trialled rule as not enforced. Governed state never reaches a condition an enforced rule forbids. (PRD §7)

The routes, in UC-19's order.

**1. A result the agent was not permitted to record.** Who may record a result is the application's decision, not the engine's (PRD D12, N7). The floor application refused Scout, and the engine never saw that request. The procurement tool let the same request through, and the engine treated it as any recording: it checked the values against the kind and recorded who made it (PRD D12). D10's gate read the result, and the record shows that the agent recorded it:

Read: `results(D10)`

| object | unit | check | outcome | remark |
|---|---|---|---|---|
| P7 | R23 | K3 | PASS | absent |
| Q15 | R23 | K4 | PASS | absent |

Read: `history(Q15)`

| transition | occurred_at | actor | actor_kind | cause |
|---|---|---|---|---|
| record | 2027-02-10T09:00Z | scout | agent | absent |

Read: `read set of D10.complete_sale: PdiCheck, PdiResult`

| object | kind |
|---|---|
| K3 | PdiCheck |
| K4 | PdiCheck |
| P7 | PdiResult |
| Q15 | PdiResult |

The completion is counted like any request, under the kind of actor that made it. Scout's completions of D17 and D10 are the first by an agent; the ten before them were people's, D18's among them (§32):

Read: `metric(Delivery.throughput, keep: [state, actor_kind])`

| state | actor_kind | value | gaps |
|---|---|---|---|
| DELIVERED | human | 10 | |
| DELIVERED | agent | 2 | |

**2. A receipt backdated beyond its bound.** The engine refuses it. `receive_unit` is backdatable within 2 days, and 10 November was more than 97 days before the request (`flow-format.md` §4.8). The refusal is counted as the agent's, beside Ben's two of October (§11):

Read: `metric(Shipment.refusals, keep: [transition, clause, remedy, actor_kind])`

| transition | clause | remedy | actor_kind | value | gaps |
|---|---|---|---|---|---|
| receive_unit | occurred_within | self_serviceable | human | 2 | |
| receive_unit | occurred_within | self_serviceable | agent | 1 | |

The refused receipt left no event. R40's receipt is dated when it happened, within the bound, and its lead time is its real one, 104d 13h from its order on 2 November to its receipt on 14 February:

Read: `events(R40)`

| transition | occurred_at | recorded_at |
|---|---|---|
| request | 2026-11-02T03:00Z | 2026-11-02T03:00Z |
| ship | 2026-11-03T01:00Z | 2026-11-03T01:00Z |
| receive | 2027-02-14T16:00Z | 2027-02-15T09:10Z |

Read: `metric(supplier_lead_time, keep: [supplier, month], filter: "o.model.manufacturer == Roverline")`

| supplier | month | value | gaps |
|---|---|---|---|
| Roverline | 2025-09 | 37d | |
| Roverline | 2026-04 | 56d | |
| Roverline | 2026-07 | 35d | |
| Roverline | 2026-10 | 5d 7h 10m | |
| Roverline | 2027-02 | 104d 13h | |

**3. A completion a rule on trial would have refused.** D17's completion went through, and the would-be refusal was recorded against Scout, not enforced (`DESIGN.md` §5.5):

Read: `history(D17)`

| transition | occurred_at | actor | actor_kind | cause |
|---|---|---|---|---|
| open | 2026-11-12T00:00Z | ben | human | absent |
| peg_slot | 2026-11-12T01:30Z | ben | human | absent |
| bind_slot | 2026-11-12T02:00Z | ben | human | absent |
| complete_sale | 2027-02-09T09:00Z | scout | agent | absent |

Read: `metric(delivery_trial_refusals, keep: [clause, actor])`

| clause | actor | value | gaps |
|---|---|---|---|
| all_passed | scout | 1 | |

Scout then relies on the rule as if it were enforced, telling Acme that every unit of D17 passed its checks. A rule on trial guarantees nothing (PRD T1), and the printed rules say so: the rule set prints an audited clause on its rule's line as `OBSERVING — NOT ENFORCED` (`renderers.md` §2), which `enforced` below reads. The engine cannot stop an agent from believing what a rule does not enforce. What it gives is a record that contradicts the belief, Q14's failure beside the would-be refusal, and a rule set that says the rule is on trial.

Read: `rules(Delivery.complete_sale)`

| clause | remedy | enforced |
|---|---|---|
| not_internal | unreachable_from_here | yes |
| filled | dependent | yes |
| all_checked | unreachable_from_here | yes |
| yield_or_signed | delegable | yes |
| all_passed | unreachable_from_here | no |

**4. A check retired so that a gate opens.** The floor application refused Scout's retirement of K4, and the engine never saw it: K4's history holds only its addition. Had an application let it through, the retirement would be a transition like any other, recorded as Scout's, and D10's gate would have read a checklist without K4. The gate's read set names the checks its scan matched, as D10's above names K3 and K4, so a reviewer would see K4 missing from it, and the checklist's history would say who retired it (`adversarial-harness.md` §2).

Read: `history(K4)`

| transition | occurred_at | actor | actor_kind | cause |
|---|---|---|---|---|
| add | 2026-10-08T00:00Z | ana | human | absent |

**Governed state never reaches a condition an enforced rule forbids.** Of the four routes, the engine refused the one its rules govern, the backdated receipt. D17's completion went through because the rule it broke was on trial, and D10's because its enforced gate held. What the rules could not know, that no one had made the check Scout's pass records, the record attributes to Scout.

## 36. Late February: an operations application runs the floor

The history continues from `now` of §34. The reads of §37 are taken at `2027-03-08T00:00Z`, a Monday. The reads of §3 to §35 hold as written, at their own `now`.

**The facts it asks about.** UC-20's application asks five questions each hour. Two are about facts the journey already records: leases run over are `Engagement.overdue` (§21), and open work is the standard `open_work`. The other three need facts the journey lacks, and the operations review declared two of them (`flow-review.md` §2.2, §2.4, §8). On 22 February the lead publishes version 14, which adds them as the review's appendix writes them, and a third:
- **`READY`**, entered by `mark_ready` once a unit is bound and none is still on order, and left by `back_to_preparation`. The completions now leave `READY`, and `filled` moves from them to `mark_ready`.
- **`procured_for`**, which `inventorize` writes from the earmark it clears and `reserve` clears, so a unit bought for an order keeps the fact when it arrives, and `awaiting_assignment` over it.
- **`missing_a_week`**, an exception over the unit's own stay in `MISSING`. The journey's `missing_units` metric gives the longest time missing per shipment, not the units, and a per-object ageing condition is a derived attribute over the entry time of the object's current stay (ADR-0084 §5).

```yaml publish
at: 2027-02-22T01:00Z
modules:
  inventory_journey:
    remove:
      - types.Delivery.transitions.complete_sale.guards.filled
      - types.Delivery.transitions.complete_internal.guards.filled
    add:
      machines.UnitLifecycle.requires.attributes.procured_for: { reference: Delivery, optional: true }
      machines.UnitLifecycle.transitions.inventorize.effect:
        - assign: { location: procured_for, expr: peg }
        - clear: [peg]
      machines.UnitLifecycle.transitions.reserve.effect:
        - assign: { location: binding, expr: inputs.slot }
        - clear: [procured_for]
      types.Robot.attributes.procured_for: { reference: Delivery, optional: true, opposite: procured }
      types.Robot.derived_attributes.awaiting_assignment:
        description: The unit is on offer and still carries the delivery it was procured for.
        expression: state == AVAILABLE and procured_for is not null
      types.Robot.derived_attributes.missing_a_week:
        description: The unit has been missing from its shipment for a week or more.
        expression: state == MISSING and entered_at(MISSING) + 7 days <= now
      types.Delivery.attributes.procured: { reference: "Robot[]", opposite: procured_for }
      types.Delivery.states:
        PREPARATION: { category: live }
        READY:       { category: live }
        DELIVERED:   { category: closed }
        CANCELLED:   { category: closed }
        DELETED:     { category: closed, final: true }
      types.Delivery.transitions.mark_ready:
        kind: external
        from: PREPARATION
        to: READY
        guards:
          filled: deny
      types.Delivery.transitions.back_to_preparation:
        kind: external
        from: READY
        to: PREPARATION
      types.Delivery.transitions.complete_sale.from: READY
      types.Delivery.transitions.complete_internal.from: READY
      types.Delivery.transitions.cancel.from: [PREPARATION, READY]
```

**What happens.**
- **The application.** Ana registers it on 22 February as a service, `operations`. Each hour it asks its five questions. It marks ready whatever may be marked, binds each unit awaiting assignment to the delivery it was procured for, and messages each engineer their open work. Its reads leave no trace in the store, and its messages are its own.
- **A race.** At its first run, at 03:00, D06 may be marked ready. At 03:02 Ben swaps D06's Kestrel unit, unbinding R21 before binding R20, and the application's request at 03:05 finds D06 with no unit. At its next run it marks D06 ready.
- **D20** is Boreal's order for two Rover 2s, which Ben orders for it and pegs on 22 February. R50 arrives on S3 on the 24th. R52, ordered for stock on the same shipment, is not among what arrives, and Ben flags it missing. R50 is photographed, labelled and inventorized, keeping D20 as the delivery it was procured for, and at its next run the application binds it. R51 is on S4, due on 3 March.
- **The application stops** on 3 March, when its host is moved, and is not restarted before `now`. Nothing in the store records that. R51 arrives on 4 March, and on the 5th Ben binds R32 to D11, Scout's Acme delivery that was a Rover 2 short (§10).

```yaml scenario
now: 2027-03-08T00:00Z

requests:
  - { at: 2027-02-22T02:00Z, actor: U-ANA, create: Service, object: S-OPS, transition: register, inputs: { name: operations } }
  - { at: 2027-02-22T03:02Z, actor: U-BEN, object: D06, transition: unbind_slot, inputs: { robot: R21 } }
  - { at: 2027-02-22T03:05Z, actor: S-OPS, object: D06, transition: mark_ready, refused: { verdict: unsatisfied, clause: filled, remedy: dependent } }
  - { at: 2027-02-22T03:10Z, actor: U-BEN, object: D06, transition: bind_slot, inputs: { robot: R20 } }
  - { at: 2027-02-22T04:05Z, actor: S-OPS, object: D06, transition: mark_ready }
  - { at: 2027-02-22T05:00Z, actor: U-BEN, create: Delivery, object: D20, transition: open, inputs: { customer: C-BOREAL, promised_date: 2027-03-12T00:00Z } }
  - { at: 2027-02-22T05:10Z, actor: U-BEN, create: Robot, object: R50, transition: request, inputs: { model: M-ROVER } }
  - { at: 2027-02-22T05:10Z, actor: U-BEN, create: Robot, object: R51, transition: request, inputs: { model: M-ROVER } }
  - { at: 2027-02-22T05:10Z, actor: U-BEN, create: Robot, object: R52, transition: request, inputs: { model: M-ROVER } }
  - { at: 2027-02-22T05:20Z, actor: U-BEN, object: D20, transition: peg_slot, inputs: { robot: R50 } }
  - { at: 2027-02-22T05:21Z, actor: U-BEN, object: D20, transition: peg_slot, inputs: { robot: R51 } }
  - { at: 2027-02-22T06:00Z, actor: U-BEN, create: Shipment, object: S3, transition: dispatch, inputs: { with_units: [R50, R52], carrier: Northline, eta: 2027-02-24T00:00Z } }
  - { at: 2027-02-22T06:05Z, actor: U-BEN, create: Shipment, object: S4, transition: dispatch, inputs: { with_units: [R51], carrier: Northline, eta: 2027-03-03T00:00Z } }
  - { at: 2027-02-24T10:00Z, actor: U-BEN, object: S3, transition: arrive }
  - { at: 2027-02-24T10:05Z, actor: U-BEN, object: S3, transition: receive_unit, inputs: { robot: R50 } }
  - { at: 2027-02-24T10:10Z, actor: U-BEN, object: S3, transition: flag_missing, inputs: { robot: R52 } }
  - { at: 2027-02-24T11:00Z, actor: U-BEN, object: R50, transition: add_photo, inputs: { photo: r50-label.jpg } }
  - { at: 2027-02-24T11:01Z, actor: U-BEN, object: R50, transition: record_label_print }
  - { at: 2027-02-24T11:05Z, actor: U-BEN, object: R50, transition: inventorize }
  - { at: 2027-02-24T12:00Z, actor: S-OPS, object: D20, transition: bind_slot, inputs: { robot: R50 } }
  - { at: 2027-03-04T09:00Z, actor: U-BEN, object: S4, transition: arrive }
  - { at: 2027-03-04T09:05Z, actor: U-BEN, object: S4, transition: receive_unit, inputs: { robot: R51 } }
  - { at: 2027-03-04T10:00Z, actor: U-BEN, object: R51, transition: add_photo, inputs: { photo: r51-label.jpg } }
  - { at: 2027-03-04T10:01Z, actor: U-BEN, object: R51, transition: record_label_print }
  - { at: 2027-03-04T10:05Z, actor: U-BEN, object: R51, transition: inventorize }
  - { at: 2027-03-05T09:00Z, actor: U-BEN, object: D11, transition: bind_slot, inputs: { robot: R32 } }
```

## 37. UC-20: an operations application runs the floor

> An application built on the engine manages the unit's journey day to day. Each hour it asks which deliveries may be marked ready, which units bought for an order still wait to be allocated to it, which units have been missing for a week, which leases have run over, and how much open work each person holds. It marks what may be marked, allocates where its own policy says to, and tells people what is theirs. The worklist, the schedule and the messages are the application's […]. *Acceptance:* given declarations that record the facts it asks about, each of those questions is answered by one query to the engine, not by a scan the application computes; every action it takes is a request by an actor of kind `service` or `agent`, refused by the same rules and recorded with its reason like anyone's; the engine keeps no schedule, sends nothing and takes no step of its own; and stopping the application stops the chasing and loses nothing from the record. (PRD §7, abridged)

**Each question is one query.** At `now`, the application's next run would get these answers, each from one read of the engine (`DESIGN.md` §10).

Which deliveries may be marked ready. `available` lists the objects for which a transition is available now, and `mark_ready` can be swept: its prefilter is the state, and its residual is `filled` (ADR-0048):

Read: `available(Delivery, mark_ready)`

| object |
|---|
| D11 |
| D20 |

Which units bought for an order still wait to be allocated to it:

Read: `query(Robot, filter: "awaiting_assignment")`

| object |
|---|
| R51 |

Which units have been missing for a week. R52 has been missing since 24 February, and the store answers by filtering on the entry time of the unit's current stay (ADR-0084 §5):

Read: `query(Robot, filter: "missing_a_week")`

| object |
|---|
| R52 |

Which leases have run over. E01 was due back on 10 November:

Read: `query(Engagement, filter: "overdue")`

| object |
|---|
| E01 |

How much open work each person holds:

Read: `metric(ServiceJob.open_work.engineer, keep: [assignee])`

| assignee | value | gaps |
|---|---|---|
| U-BEN | 2 | |
| U-CHEN | 1 | |

The first two answers disagree about D20. It may be marked ready with one of its two units bound, while R51, bought for it, waits to be assigned to it. `filled` asks that a unit be bound and none be on order, and R51 stopped being on order when it arrived: its earmark became `procured_for`, which `filled` does not read. Had the application been running, whether it marked D20 ready before binding R51 would have depended on the order of its own steps (§39).

**Every action is a request by a service, refused by the same rules and recorded with its reason.** The application has no path of its own (PRD N6). Its refused request is in D06's attempt log beside a person's refusal of September, with its rule and remedy:

Read: `attempts(D06)`

| transition | actor_kind | verdict | clause | remedy | call |
|---|---|---|---|---|---|
| bind_slot | human | stale | absent | self_serviceable | absent |
| mark_ready | service | unsatisfied | filled | dependent | absent |

What it did is counted under its kind:

Read: `metric(Delivery.transition_counts, keep: [transition, actor_kind], filter: "t.actor_kind == service")`

| transition | actor_kind | value | gaps |
|---|---|---|---|
| mark_ready | service | 1 | |
| bind_slot | service | 1 | |

**The engine keeps no schedule, sends nothing and takes no step of its own.** D20 has been ready to mark since R51 was put on offer on 4 March, and D11 since the 5th, and nothing has marked either. D20's history holds Ben's requests and the application's binding, and nothing since:

Read: `history(D20)`

| transition | occurred_at | actor | actor_kind | cause |
|---|---|---|---|---|
| open | 2027-02-22T05:00Z | ben | human | absent |
| peg_slot | 2027-02-22T05:20Z | ben | human | absent |
| peg_slot | 2027-02-22T05:21Z | ben | human | absent |
| bind_slot | 2027-02-24T12:00Z | operations | service | absent |

**Stopping the application stops the chasing and loses nothing.** Its requests are in the record like anyone's, and every question it asked is answered as before; only the asking stopped. The store cannot say that the application stopped, since a caller that stops calling leaves nothing to record. Noticing it is the application's own concern, as the operations review asked: "an alert for when the loop itself stops" (`flow-review.md` §2.1).

## 38. UC-21: completing a sale sells its units

> When sales ops completes a delivery, every unit bound to it becomes sold and gains its warranty, as one change. […] A reviewer reading only the unit's rules, and an agent about to complete a delivery, can each tell that it will happen. *Acceptance:* completing a delivery sells each bound unit and creates its warranty in one request, or refuses and changes nothing, naming the unit and the rule that refused; the unit's printed rules say that a delivery's completion sells it; the completion can be checked before it is requested, its cascades included; and each unit's sale records the completion as its cause. (PRD §7, abridged)

**What the journey already does, and what version 15 adds.** The journey's `complete_sale` calls `sell` on each unit bound, and `sell` is `only_via` the completion (`unit-journey.md` §2). The warranty is this document's own type (§18), started by hand. On 8 March the lead publishes version 15:
- **The completion creates each unit's warranty** in the loop that sells it, as the first consumer's completion does (`first-consumer-walkthrough.md` §3.3).
- **A warranty holds its model's months, and its end is recorded after.** The end is a number of calendar months from the sale, which the engine cannot compute, since a month has no fixed length (`declaration-syntax.md` §8.3). So the end becomes optional, and the sales application records it with `set_end`, as §22 inferred it computes it.
- **A unit is sold with the manufacturer serial its model requires.** The journey requires the serial of every unit on offer, and refuses to turn a model's requirement on while a unit on offer lacks one (`unit-journey.md` §2). A unit already reserved is not on offer, so nothing stops it being sold without one. The guard on `sell` closes that, and it is the rule a completion's cascade can fail.

```yaml publish
at: 2027-03-08T01:00Z
modules:
  inventory_journey:
    add:
      machines.UnitLifecycle.transitions.sell.guards.mfr_serial: deny
      types.Warranty.attributes.ends_at: { type: timestamp, optional: true, indexed: true }
      types.Warranty.attributes.months: { type: int, optional: true }
      types.Warranty.transitions.start.required_inputs: [unit]
      types.Warranty.transitions.start.optional_inputs: [ends_at, months]
      types.Warranty.transitions.set_end:
        kind: internal
        from: ACTIVE
        description: Records the warranty's end, which the sales application computes from the sale and the model's months.
        required_inputs: [ends_at]
      types.Delivery.transitions.complete_sale.effect:
        - foreach:
            item: u
            array: units
            limit: 500
            steps:
              - call: { target: u, transition: sell, inputs: { buyer: customer } }
              - create: { type: Warranty, transition: start, inputs: { unit: u, months: u.model.warranty_months } }
```

**What happens.**
- **Kestrel serials.** Kestrel starts printing serials. On 8 March Ana records them on R21 and R22, which are on offer, and turns the model's requirement on. R20, reserved for D06, is not on offer, so nothing refuses the change.
- **D06.** On the 9th Ben checks D06's completion and is told why it would fail, asks anyway and is refused. Ben then records R20's serial, checks again and completes D06.
- **D20.** The application is still stopped. Ana binds R51 to D20 and marks it ready, and Ben records the four results and completes D20 on the 10th.
- **The warranties' ends.** The sales application, registered as a service, records each warranty's end.

A creation's id is the engine's to assign. The history names the ids a request creates, in `creates`, in the order its loop runs, which is ascending id (`flow-format.md` §4.8). A `check` changes nothing, and the history records what it answered.

```yaml scenario
now: 2027-03-22T00:00Z

requests:
  - { at: 2027-03-08T02:00Z, actor: U-ANA, create: Service, object: S-SALES, transition: register, inputs: { name: sales } }
  - { at: 2027-03-08T02:10Z, actor: U-ANA, object: R21, transition: record_manufacturer_serial, inputs: { manufacturer_serial: KD-20417 } }
  - { at: 2027-03-08T02:11Z, actor: U-ANA, object: R22, transition: record_manufacturer_serial, inputs: { manufacturer_serial: KD-20418 } }
  - { at: 2027-03-08T02:20Z, actor: U-ANA, object: M-KESTREL, transition: edit, inputs: { warranty_months: 12, manufacturer_serial_required: true } }
  - { at: 2027-03-08T09:00Z, actor: U-ANA, object: D20, transition: bind_slot, inputs: { robot: R51 } }
  - { at: 2027-03-08T09:05Z, actor: U-ANA, object: D20, transition: mark_ready }
  - { at: 2027-03-09T09:00Z, actor: U-BEN, check: CK1, object: D06, transition: complete_sale }
  - { at: 2027-03-09T09:05Z, actor: U-BEN, object: D06, transition: complete_sale, refused: { verdict: unsatisfied, clause: mfr_serial, remedy: dependent, call: R20 } }
  - { at: 2027-03-09T10:00Z, actor: U-BEN, object: R20, transition: record_manufacturer_serial, inputs: { manufacturer_serial: KD-20416 } }
  - { at: 2027-03-09T10:05Z, actor: U-BEN, check: CK2, object: D06, transition: complete_sale, creates: [W4] }
  - { at: 2027-03-09T10:10Z, actor: U-BEN, object: D06, transition: complete_sale, creates: [W4] }
  - { at: 2027-03-09T11:00Z, actor: U-BEN, record: pdi_results, subject: D20, object: Q16, fields: { unit: R50, check: K3, outcome: PASS } }
  - { at: 2027-03-09T11:05Z, actor: U-BEN, record: pdi_results, subject: D20, object: Q17, fields: { unit: R50, check: K4, outcome: PASS } }
  - { at: 2027-03-09T11:10Z, actor: U-BEN, record: pdi_results, subject: D20, object: Q18, fields: { unit: R51, check: K3, outcome: PASS } }
  - { at: 2027-03-09T11:15Z, actor: U-BEN, record: pdi_results, subject: D20, object: Q19, fields: { unit: R51, check: K4, outcome: PASS } }
  - { at: 2027-03-10T09:00Z, actor: U-BEN, object: D20, transition: complete_sale, creates: [W5, W6] }
  - { at: 2027-03-10T10:00Z, actor: S-SALES, object: W4, transition: set_end, inputs: { ends_at: 2028-03-09T10:10Z } }
  - { at: 2027-03-10T10:00Z, actor: S-SALES, object: W5, transition: set_end, inputs: { ends_at: 2029-03-10T09:00Z } }
  - { at: 2027-03-10T10:00Z, actor: S-SALES, object: W6, transition: set_end, inputs: { ends_at: 2029-03-10T09:00Z } }
```

**The unit's printed rules say that a delivery's completion sells it.** Under each transition, the rule set lists every transition whose outcome can cause it, derived from the graph publishing builds to refuse a cycle (`renderers.md` §2, ADR-0108). `sell` is not requestable, so its causes print as `caused by`. A warranty is also started by hand, so the completion prints under `start` as `also caused by`:

Read: `causes(Robot.sell)`

| cause | through | where | limit | printed |
|---|---|---|---|---|
| Delivery.complete_sale | units | absent | 500 | caused by |

Read: `rules(Robot.sell)`

| clause | remedy | enforced |
|---|---|---|
| mfr_serial | dependent | yes |

Read: `causes(Warranty.start)`

| cause | through | where | limit | printed |
|---|---|---|---|---|
| Delivery.complete_sale | units | absent | 500 | also caused by |

**The completion can be checked before it is requested, its cascades included.** `check` simulates the request, and names the unit and the rule its cascade would fail on (`DESIGN.md` §10, `library-api.md` §6):

Read: `check CK1`

| verdict | clause | remedy | call |
|---|---|---|---|
| unsatisfied | mfr_serial | dependent | R20 |

**Or it refuses and changes nothing, naming the unit and the rule.** Ben's request was refused as the check said, with the call's verdict, its rule and R20 (`flow-format.md` §4.8). It wrote nothing: R20 stayed reserved and no warranty was started. Its attempt is in D06's log, after the application's refusal of §36:

Read: `attempts(D06)`

| transition | actor_kind | verdict | clause | remedy | call |
|---|---|---|---|---|---|
| bind_slot | human | stale | absent | self_serviceable | absent |
| mark_ready | service | unsatisfied | filled | dependent | absent |
| complete_sale | human | unsatisfied | mfr_serial | dependent | R20 |

With R20's serial recorded, the check passes:

Read: `check CK2`

| verdict | clause | remedy | call |
|---|---|---|---|
| satisfied | absent | absent | absent |

**Each unit's sale records the completion as its cause.** R20's history is its intake, its label, its override of September, its reservation by D06's binding, its serial and its sale. Each cascaded event names the event that caused it, and runs as the actor who requested it:

Read: `history(R20)`

| transition | occurred_at | actor | actor_kind | cause |
|---|---|---|---|---|
| add_to_intake | 2026-09-08T00:00Z | ana | human | absent |
| record_label_print | 2026-09-08T10:00Z | ana | human | absent |
| correct_state | 2026-09-09T12:00Z | ana | human | absent |
| reserve | 2027-02-22T03:10Z | ben | human | D06.bind_slot |
| record_manufacturer_serial | 2027-03-09T10:00Z | ben | human | absent |
| sell | 2027-03-09T10:10Z | ben | human | D06.complete_sale |

Read: `history(W4)`

| transition | occurred_at | actor | actor_kind | cause |
|---|---|---|---|---|
| start | 2027-03-09T10:10Z | ben | human | D06.complete_sale |
| set_end | 2027-03-10T10:00Z | sales | service | absent |

**Each bound unit is sold and gains its warranty, in one request.** D20's completion sold both its units and started both their warranties, in the order its loop ran:

Read: `caused by(D20.complete_sale)`

| object | transition |
|---|---|
| R50 | sell |
| W5 | start |
| R51 | sell |
| W6 | start |

Read: `get(W5)`

| attribute | value |
|---|---|
| ends_at | 2029-03-10T09:00Z |
| months | 24 |
| unit | R50 |

## 39. What is inferred, and what UC-19 to UC-21 found

**Inferences**, each resting on a requirement, the reviewed flow or the operations review:
1. **The rule on trial asks that no check has only failed** (UC-19). §12 left that question to the lead, and UC-19's third route needs a rule on trial that a completion can break. Its remedy is `unreachable_from_here`, as `all_checked`'s is, since a passing result is another request (D446).
2. **The applications' rules are this document's** (UC-19). That the floor application lets only an engineer record a result and only a person retire a check, and that the procurement tool forwards any recording, are the scenario's. PRD N7 makes both the applications' to decide, and the engine sees neither.
3. **UC-20's facts are the operations review's declared proposals** (UC-20): `READY`, `mark_ready` and `back_to_preparation`, and `procured_for` with `awaiting_assignment`, as `flow-review.md`'s appendix writes them. UC-20 draws its questions from that review (§2.1), and its acceptance asks for "declarations that record the facts it asks about". Whether the journey itself adopts them is still the author's decision (`flow-review.md` §7, questions 3 and 4).
4. **A unit missing for a week is a derived attribute** (UC-20). The journey's `missing_units` answers per shipment, and a per-object ageing condition is written over the entry time of the object's current stay (ADR-0084 §5).
5. **The application is a service, and its policy binds each unit awaiting assignment to the delivery it was procured for** (UC-20). The review keeps assignment with the people who assign stock. UC-20 gives it to the application's own policy, and this one's is the simplest the fact allows.
6. **The completion creates the warranty, holding its model's months, and the sales application records its end** (UC-21). The first consumer's completion creates the warranty (`first-consumer-walkthrough.md` §3.3), and §22 inferred that the sales application computes its end.
7. **A unit is sold with the serial its model requires** (UC-21). It is the journey's own rule for a unit on offer, extended to the sale of a reserved one. It also gives a completion's cascade a rule that can refuse, which the journey's `sell` did not have.

**Findings.**
1. **The review's `filled` does not read the units procured for a delivery** (UC-20). `mark_ready` requires a unit bound and none still on order. A unit bought for the delivery is on order until it arrives. From then it carries `procured_for`, and is neither bound nor on order. So D20 may be marked ready with one of its two units while the other waits to be assigned to it (§37). Adding `none(u in procured)` to `filled` would make the delivery wait for it. The two proposals are the author's to decide together, so `flow-review.md` §8 records the interaction, and `TODO.md` holds the question.
2. **A version that reroutes a transition changes what a metric filtering on its states counts, and nothing reports it** (UC-20, UC-21). The lead's `delivery_cycle_time` counts completions from `PREPARATION` (§29). Since version 14 a completion leaves `READY`, so D06, 165 days in preparation, and D20 are not counted. The people's median below is §30's, over ten completions that include D17's and D10's and not those two. Publishing checked version 14 and reported nothing of it, only the `audit` notice of version 13's rule on trial, as a probe of the flow checker's notices showed. The metric is still well formed, and its filter still means what it says. `TODO.md` holds the question whether a publish should name a declared metric whose filter compares a transition's states to one the version no longer routes that way.

Read: `metric(delivery_cycle_time, keep: [creator])`

| creator | value | gaps |
|---|---|---|
| human | 3d 12h | |
| agent | 11d 2h | |
| unknown | 15d | |

3. **A warranty counted in months has no end the engine can compute** (UC-21, determined). The language has no month, by design (`declaration-syntax.md` §8.3), so a cascade can start a warranty but cannot date its end. The warranty is created in the completion, as F8 asks, and its end arrives a request later. Until it does, `Warranty.expiring` cannot hold for it.
4. **`check` names the unit and the rule, not what the request would change** (UC-21, determined). ADR-0108 leaves open whether `check` should also return the objects and transitions a request would reach. UC-21 asks for the verdict, cascades included, and that is what `check` returns.
5. **The engine cannot tell a pass nobody checked from a real one** (UC-19, determined). What it gives is who recorded each result, and every gate's read set. A reviewer asking why D10 was delivered finds Q15, and that Scout recorded it (PRD D12, ADR-0114).
6. **A stopped application leaves nothing in the store** (UC-20, determined). The engine records requests, and an application that stops making them makes no event. Its absence shows only as work that waits, which the same queries still list.

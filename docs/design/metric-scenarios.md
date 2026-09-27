# Metric scenarios: UC-1 to UC-3

Status: **worked scenarios**, 2026-09-27, written at the author's direction: "write the UC-1 to UC-3 checked scenarios, continue with your inference, as long as the inference is based clear requirements we've already discussed and the use case we've reviewed". Each of the PRD's first three use cases is written here as a fixed flow, a fixed history and the values its reads return, so a builder has a test and a reader has something to compute and compare (`TODO.md`, "Write the metric use cases as checked scenarios").

**Method.** One store, one history, three questions. The flow is the robot inventory of [`unit-journey.md`](unit-journey.md) §2, which ten rounds of cold readers found determined in everything a request does ([`flow-recoverability-review.md`](flow-recoverability-review.md)), with the agent type that writing these scenarios added to it (§1). Every value below is derived by hand from [`declaration-syntax.md`](declaration-syntax.md) §6.9 and §6.11, and [`scripts/check-scenarios.py`](../../scripts/check-scenarios.py) recomputes each one independently. The script also replays the history against the modules. It refuses:
- a request whose actor the store does not hold;
- a transition the type does not declare, that is `only_via` others, or that is taken from a state it does not leave;
- a refusal on a guard the transition does not declare, with a remedy other than the declared one, or on a clause that is not the first to fail;
- an applied request whose guards do not hold, or that breaks an invariant.

The script evaluates guards, effects and invariants from a transcription it holds, and checks each against the module's text before it runs. It is not an engine, and it refuses a history that reaches anything it has not transcribed. Its self-test plants a wrong value, an illegal transition, a refusal on a clause that holds, an unknown agent, and two invalid published metrics, and shows each caught.

**What is inferred.** The use cases speak of the first consumer's deliveries, and the reviewed flow is thinner in two places: its delivery has no product configuration and no checklist. §6 lists each inference with what it rests on. Two questions the specification left open were decided to write the values (§7, ADR-0123).

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
  - { object: C-ACME, type: Customer, state: ACTIVE, attributes: { legacy_key: "C-1001", name: Acme Robotics } }
  - { object: C-BOREAL, type: Customer, state: ACTIVE, attributes: { legacy_key: "C-1002", name: Boreal Logistics } }
  - { object: M-ARM, type: RobotModel, state: ACTIVE, attributes: { name: Arm 7, warranty_months: 24 } }
  - { object: M-ROVER, type: RobotModel, state: ACTIVE, attributes: { name: Rover 2, warranty_months: 24 } }
  - { object: R01, type: Robot, state: AVAILABLE, attributes: { model: M-ARM, label_printed_at: 2026-08-03T00:00Z } }
  - { object: R02, type: Robot, state: AVAILABLE, attributes: { model: M-ARM, label_printed_at: 2026-08-03T00:00Z } }
  - { object: R03, type: Robot, state: AVAILABLE, attributes: { model: M-ARM, label_printed_at: 2026-08-03T00:00Z } }
  - { object: R04, type: Robot, state: INTAKE, attributes: { model: M-ROVER, label_printed_at: 2026-08-27T00:00Z } }
  - { object: D01, type: Delivery, state: PREPARATION, created: 2026-08-20T00:00Z, entered: 2026-08-20T00:00Z, entered_by: human, attributes: { customer: C-ACME } }
  - { object: D07, type: Delivery, state: PREPARATION, attributes: { customer: C-BOREAL } }
  - { object: D08, type: Delivery, state: PREPARATION, entered: 2026-08-15T00:00Z, entered_by: human, silent: [2026-08-25T00:00Z, 2026-08-28T00:00Z], attributes: { customer: C-ACME } }

requests:
  - { at: 2026-09-01T01:00Z, actor: U-ANA, create: Agent, object: A-SCOUT, transition: issue, inputs: { key_id: scout, name: Scout } }
  - { at: 2026-09-01T02:00Z, actor: U-ANA, create: Robot, object: R05, transition: add_opening_stock, inputs: { model: M-ARM, label_printed_at: 2026-09-01T02:00Z } }
  - { at: 2026-09-01T02:00Z, actor: U-ANA, create: Robot, object: R06, transition: add_opening_stock, inputs: { model: M-ARM, label_printed_at: 2026-09-01T02:00Z } }
  - { at: 2026-09-01T02:00Z, actor: U-ANA, create: Robot, object: R07, transition: add_opening_stock, inputs: { model: M-ROVER, label_printed_at: 2026-09-01T02:00Z } }
  - { at: 2026-09-01T02:00Z, actor: U-ANA, create: Robot, object: R08, transition: add_opening_stock, inputs: { model: M-ROVER, label_printed_at: 2026-09-01T02:00Z } }
  - { at: 2026-09-01T02:00Z, actor: U-ANA, create: Robot, object: R09, transition: add_opening_stock, inputs: { model: M-ARM, label_printed_at: 2026-09-01T02:00Z } }
  - { at: 2026-09-02T00:00Z, actor: U-ANA, object: D01, transition: bind_slot, inputs: { robot: R05 } }
  - { at: 2026-09-04T00:00Z, actor: U-ANA, object: D01, transition: complete_sale }
  - { at: 2026-09-05T00:00Z, actor: U-ANA, create: RobotModel, object: M-KESTREL, transition: add, inputs: { name: Kestrel K2, warranty_months: 12, label_photo_required: true } }
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

On 24 September the lead publishes one metric on `Delivery`. A metric is declared under the type whose rows it reads (`flow-format.md` §4.9), so this is a new version of the journey's `Delivery`. The script splices it into the module and runs all four steps of the flow checker over the result.

```yaml publish
delivery_wait:
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
  - an `unavailable` refusal when he asked to complete D02 after it was delivered (W37);
  - `not_internal` when he asked to sell D04, an internal delivery (W38);
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

Kestrel K2 requires a label photo, and its units arrive without one. Ana's three requests to inventorize them were refused by `photo`, and she put each on offer by `correct_state`, with the reason `SUPPLIER_EXCEPTION`. She did the same for R04, a ported unit left in intake. Ben corrected R23, mis-scanned at intake, and Scout moved R09 into the development pool, the legacy record having had it wrong. The import placed R01 to R03 in `AVAILABLE`, and those events are marked `imported`, so the counts leave them out.

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

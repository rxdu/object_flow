# Metric scenarios: UC-1 to UC-6

Status: **worked scenarios**, 2026-09-27, written at the author's direction: "write the UC-1 to UC-3 checked scenarios, continue with your inference, as long as the inference is based clear requirements we've already discussed and the use case we've reviewed". The author then asked for the next three: "write the scenarios for UC-4 to UC-6" (§8 to §12). Each of the PRD's first six use cases is written here as a fixed flow, a fixed history and the values its reads return, so a builder has a test and a reader has something to compute and compare (`TODO.md`, "Write the metric use cases as checked scenarios").

**Method.** One store and one history, in two parts, with each use case's questions asked of it. The flow is the robot inventory of [`unit-journey.md`](unit-journey.md) §2, which ten rounds of cold readers found determined in everything a request does ([`flow-recoverability-review.md`](flow-recoverability-review.md)), with the agent type that writing these scenarios added to it (§1). Every value below is derived by hand from [`declaration-syntax.md`](declaration-syntax.md) §6.9 and §6.11, and [`scripts/check-scenarios.py`](../../scripts/check-scenarios.py) recomputes each one independently. The script also replays the history against the modules. It refuses:
- a request whose actor the store does not hold;
- a transition the type does not declare, that is `only_via` others, or that is taken from a state it does not leave;
- a refusal on a guard the transition does not declare, with a remedy other than the declared one, or on a clause that is not the first to fail;
- an applied request whose guards do not hold, or that breaks an invariant;
- a recording its subject's kinds do not include, or one its generated guards refuse;
- a published version the flow checker refuses, checked against the version before it.

The script evaluates guards, effects and invariants from a transcription it holds, and checks each against the module's text before it runs. It is not an engine, and it refuses a history that reaches anything it has not transcribed. Its self-test plants nine mistakes, from a wrong value to a correction left out, and shows each caught.

**What is inferred.** The use cases speak of the first consumer's deliveries, and the reviewed flow is thinner in two places: its delivery has no product configuration and no checklist. §6 lists each inference with what it rests on. Two questions the specification left open were decided to write the values (§7, ADR-0123). UC-4 to UC-6 need inspection and availability datapoints the journey lacks, so the history's second part publishes a version that adds them (§8), and §12 lists what they inferred and found.

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

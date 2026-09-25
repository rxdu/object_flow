# Returns: a flow that starts with no rules, as a checked module

Status: **worked example**, 2026-09-24. The declarations behind the worked example's cases that the unit's journey ([`unit-journey.md`](unit-journey.md)) does not hold. `scripts/check-syntax-doc.py docs/design/returns-module.md` checks both publishes below against the implemented checks, resolving their `use` imports against the journey and the specification, and reports them clean.

**What it is for.** Like the journey, this module exists to test the design, not to specify a process. The first consumer handles customer returns by hand, around the unit's `accept_return`, and no production code models them as a flow. That is why returns make the case the PRD's convergence requirements describe: a flow published with no rules at all, measured from its first request, and tightened only where its own data says it waits (PRD V1, V2, V5, UC-13). The second publish then carries three mechanisms no other checked module exercises: a derived value that reads other objects (C1, C2), a rule that reads a metric across many objects (L5, UC-10), and the promotion of a label into a state (D6, V5). *(Corrected 2026-09-25: the first of these was a visibility rule the derived value was read under, until ADR-0114 withdrew visibility from the engine.)*

The reorder point and the low-stock condition the page's cases also use are on `RobotModel`, in the journey's module (`unit-journey.md` §2), since the unit's own rules read the model.

## 1. The first publish: no rules at all

Four states, transitions with no guards, no invariants, no datapoint kinds and no formulas. The publish checks still apply whole — every state has a category, a terminal state exists, every state is reachable, the creation writes every required reference — and none of them asks for a rule (PRD V1, DESIGN.md §5.9). From the first request every transition is captured with who, which kind of actor, when, and from and to, so time in each state and throughput exist before anyone writes a rule (PRD D1, M1). Labels need no declaration (PRD D6).

```text
module returns
use   inventory_journey.{Robot, UnitLifecycle}
use   inventory.{Customer}

type Return version 1 {
  tracking record
  states   RECEIVED category inbound, INSPECTING category live,
           RESOLVED category closed, CLOSED category closed terminal
  summary  unit, customer, state

  ref      unit     : Robot
  ref      customer : Customer

  create receive -> RECEIVED accepts unit, customer { }
  do inspect RECEIVED   -> INSPECTING { }
  do resolve INSPECTING -> RESOLVED   { }
  do close   RESOLVED   -> CLOSED     { }
}
```

## 2. The second publish: a label promoted, and rules where the data showed a need

A month of labels says where returns wait. Staff labelled fourteen returns `missing-charger`, and diagnostics showed the label clustered on returns in `INSPECTING` that waited longest there (PRD V2). This publish promotes it into a state, `AWAITING_PARTS`, with a way in and a way back. Returns still in `INSPECTING` with the label are moved by ordinary requests, so history says who moved each; the labels, and the time spent before the state existed, stay where they were recorded (PRD V5; ADR-0106 §14).

It also adds what the first month made worth adding:
- **Who handled it.** `receive` records who received the return, as data a reader can filter on. Who may act on a return, and who may see it, are the upper layer's, which can narrow a read to the returns someone handled by filtering on `handled_by` (PRD T5, ADR-0114).
- **`prior_returns`**, how many times the same unit came back before. It reads other returns, and counts every one wherever it is read, in a rule or by a reader (PRD C1, C2; ADR-0114).
- **`second_eye`**, a rule that reads a metric. A replacement needs a lead's approval, recorded by `approve_replacement`, once the model has had five or more returns in thirty days (PRD L5; the shape of UC-10). The rule reads that an approval was recorded, not who is asking: who counts as a lead, and so who may request `approve_replacement`, is the upper layer's (ADR-0114). The metric is computed over every return before the transaction, and its value is recorded on the event and on a refused attempt (ADR-0084).

```text
module returns
use   inventory_journey.{Robot, UnitLifecycle}
use   inventory.{Customer}


enum ReturnOutcome version 1 { REPAIRED, REPLACED, REFUNDED, NO_FAULT }

type Return version 2 {
  tracking record
  states   RECEIVED category inbound, INSPECTING category live,
           AWAITING_PARTS category live,
           RESOLVED category closed, CLOSED category closed terminal
  summary  unit, customer, state

  ref      unit       : Robot
  ref      customer   : Customer
  attr     handled_by identity? indexed
  attr     approved_by identity?
  attr     outcome    ReturnOutcome?

  derive prior_returns = count(r in Return where r.unit == unit and r.created_at < created_at)

  create receive -> RECEIVED accepts unit, customer {
    set handled_by := actor.id
  }
  do inspect     RECEIVED       -> INSPECTING     { }
  do await_parts INSPECTING     -> AWAITING_PARTS { }
  do parts_in    AWAITING_PARTS -> INSPECTING     { }
  act approve_replacement at INSPECTING {
    set approved_by := actor.id
  }
  do resolve     INSPECTING     -> RESOLVED {
    input outcome : ReturnOutcome
    require second_eye: inputs.outcome != ReturnOutcome.REPLACED
                        or approved_by is not null
                        or metric(returns_by_model, model := unit.model, over last 30 days) < 5
                                                                             because delegable
    set outcome := inputs.outcome
  }
  do close RESOLVED -> CLOSED { }
}

metric returns_by_model version 1 {
  from      r in Return
  by        model = r.unit.model
  window on r.created_at
  value     count()
}
```

## 3. What each case on the page tests here

| Case on the page | Declaration | What it puts under load | PRD |
|---|---|---|---|
| A new flow with no rules at all | `Return` version 1 | a flow publishes and runs with no rules, and is measured from its first request | V1, D1, M1, F5 |
| A label becomes a state | `Return` version 2's `AWAITING_PARTS` | a label counted, diagnosed and promoted, history kept and not rewritten | D6, V2, V5, F5 |
| Has this unit come back before? | `prior_returns` | a derived value over other objects, the same wherever it is read | C1, C2 |
| A replacement for a model that keeps coming back | `second_eye`, `approve_replacement` and `returns_by_model` | a rule that reads a metric and a recorded approval, the metric's value recorded on the event | L5, L2, C2 |
| Lowering the alarm instead of fixing the stock | `RobotModel.set_reorder_point`, `low_stock` (`unit-journey.md` §2) | a threshold that changes only by its object's own guarded transition | L4, M7, T3 |

## 4. What the checker says, and what it cannot

The checker verifies shape: that each name resolves, a state literal names a state of its type (check 19), no condition reads the actor (check 64), the metric reference binds a dimension the metric declares and has a window to apply `over last` to (check 56), and the rest of the implemented checks. It does not verify behaviour: that `second_eye` refuses a replacement with no recorded approval once the metric reaches five, and records the value it read, are properties of the runtime the adversarial harness is for (`adversarial-harness.md`).

The two publishes are one document so that both are checked, and the checker reads them as successive versions of one module. Publishing the second after the first needs no mapping, since it removes nothing, and its impact report would list the returns that would gain `await_parts` (`publish-and-import.md` §1).

# Returns: a flow that starts with no rules, as a checked module

Status: **worked example**, 2026-09-24. The declarations behind the worked example's cases that the unit's journey ([`unit-journey.md`](unit-journey.md)) does not hold. `scripts/check-syntax-doc.py docs/design/returns-module.md` checks both publishes below against the implemented checks, resolving their `use` imports against the journey and the specification, and reports them clean.

**What it is for.** Like the journey, this module exists to test the design, not to specify a process. The first consumer handles customer returns by hand, around the unit's `accept_return`, and no production code models them as a flow. That is why returns make the case the PRD's convergence requirements describe: a flow published with no rules at all, measured from its first request, and tightened only where its own data says it waits (PRD V1, V2, V5, UC-13). The second publish then carries three mechanisms no other checked module exercises: a visibility rule a derived value is read under (T5, C2), a rule that reads a metric across many objects (L5, UC-10), and the promotion of a label into a state (D6, V5).

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
- **Who may act, and who may see.** A return is visible to whoever handles it and to holders of `RETURNS_VIEW_ALL` (PRD T5).
- **`prior_returns`**, how many times the same unit came back before. It reads other returns, so a reader who sees only their own gets a count over those, marked partial, while a rule reading it counts every return (PRD C1, C2; ADR-0106 §5).
- **`second_eye`**, a rule that reads a metric. A replacement needs a lead once the model has had five or more returns in thirty days (PRD L5; the shape of UC-10). The metric is computed over every return before the transaction, since a verdict may not depend on who asked; its value is recorded on the event and on a refused attempt, and withheld from a requester whose view of it would be partial (ADR-0084, ADR-0096).

```text
module returns
use   inventory_journey.{Robot, UnitLifecycle}
use   inventory.{Customer}

capability RETURNS_EDIT, RETURNS_LEAD, RETURNS_VIEW_ALL

enum ReturnOutcome version 1 { REPAIRED, REPLACED, REFUNDED, NO_FAULT }

type Return version 2 {
  tracking record
  states   RECEIVED category inbound, INSPECTING category live,
           AWAITING_PARTS category live,
           RESOLVED category closed, CLOSED category closed terminal
  summary  unit, customer, state
  visible when actor.has(RETURNS_VIEW_ALL) or handled_by == actor.id

  ref      unit       : Robot
  ref      customer   : Customer
  attr     handled_by identity? indexed
  attr     outcome    ReturnOutcome?

  derive prior_returns = count(r in Return where r.unit == unit and r.created_at < created_at)

  create receive -> RECEIVED accepts unit, customer {
    require may: actor.has(RETURNS_EDIT) because delegable
    set handled_by := actor.id
  }
  do inspect     RECEIVED       -> INSPECTING     { require may: actor.has(RETURNS_EDIT) because delegable }
  do await_parts INSPECTING     -> AWAITING_PARTS { require may: actor.has(RETURNS_EDIT) because delegable }
  do parts_in    AWAITING_PARTS -> INSPECTING     { require may: actor.has(RETURNS_EDIT) because delegable }
  do resolve     INSPECTING     -> RESOLVED {
    input outcome : ReturnOutcome
    require may:        actor.has(RETURNS_EDIT) because delegable
    require second_eye: inputs.outcome != ReturnOutcome.REPLACED
                        or actor.has(RETURNS_LEAD)
                        or metric(returns_by_model, model := unit.model, over last 30 days) < 5
                                                                             because delegable
    set outcome := inputs.outcome
  }
  do close RESOLVED -> CLOSED { require may: actor.has(RETURNS_EDIT) because delegable }
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
| Has this unit come back before? | `prior_returns` under `visible when` | a derived value read under the reader's visibility, marked partial | T5, C2, M2, C1 |
| A replacement for a model that keeps coming back | `second_eye` and `returns_by_model` | a rule that reads a metric, its value recorded and withheld from a partial reader | L5, L2, T5, C2 |
| Lowering the alarm instead of fixing the stock | `RobotModel.set_reorder_point`, `low_stock` (`unit-journey.md` §2) | a threshold that changes only by its object's own guarded transition | L4, M7, T3 |

## 4. What the checker says, and what it cannot

The checker verifies shape: that each name resolves, a state literal names a state of its type (check 19), the visibility predicate reads only indexed members (check 7), the metric reference binds a dimension the metric declares and has a window to apply `over last` to (check 56), and the rest of the implemented checks. It does not verify behaviour: that `prior_returns` is marked partial for a partial reader, or that `second_eye`'s value is withheld from one, are properties of the runtime the adversarial harness is for (`adversarial-harness.md`).

The two publishes are one document so that both are checked, and the checker reads them as successive versions of one module. Publishing the second after the first needs no mapping, since it removes nothing, and its impact report would list the returns that would gain `await_parts` (`publish-and-import.md` §1).

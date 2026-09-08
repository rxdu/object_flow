# ADR-0066: A cascade clause carries arguments

- **Status:** Accepted — recommendation accepted by the author 2026-09-08
- **Date:** 2026-09-08
- **Refines:** ADR-0019, ADR-0056, ADR-0058
- **Answers:** open question 7

## Context

A `part … cascade` clause names the whole's transition, the part's transition and a bound. It carries no arguments, so a cascade can only drive a transition that needs none.

The evidence that this is wrong is older than the question. `design/first-consumer-walkthrough.md` §5, written before the syntax existed, expresses "retire an engaged unit" as `unit.retire` cascading `engagement_line.close(reason)` — a cascade passing a value. The production system's cascades carry behaviour for the same reason: cancelling a customer sets each delivery to cancelled **and releases its reserved inventory** (`wr:app/core/cascade_registry.py:64-95`), and cancelling a delivered delivery voids the warranties that delivery created (`wr:app/core/state_registry.py:443-460`). A reviewer modelling a clinical trial, with no sight of either, declared three transitions — `close_ongoing`, `close_with_parent`, `close_at_archive` — that exist only because a reason could not be passed to the part being disposed of.

Until iteration 14 `DESIGN.md` called the outcome-level step "cascade, with inputs", using one word for two constructs of which only one took them.

## Decision

A `cascade on` clause may carry arguments, in the same form an outcome-level `call` uses:

```text
cascade on { cancel, archive } to Note.file(reason := "the subject was archived") limit 100
```

Arguments are expressions over the **whole's** members and the transition's inputs, not the part's, since the whole is what is being transitioned. Publishing checks them against the part transition's declared inputs exactly as check 2 checks a `call`.

## Alternatives rejected

- **Leave it, and declare a transition per disposal reason.** This is what the clinical-trial reviewer was forced into, and it multiplies transitions on the part for reasons that belong to the whole. It also puts the reason in a transition *name*, where it cannot be read as data.
- **Let the part read the whole's state instead of being passed a value.** Rejected: it inverts the dependency, so a part type becomes coupled to every whole that disposes of it, and the reason a whole gives is frequently not derivable from its state.

## Consequences

- `docs/design/declaration-syntax.md` §3.2 gains the argument list in the cascade grammar, and check 2 covers it.
- The walkthrough's retire-an-engaged-unit row becomes expressible as written, which it has not been since the syntax was first drafted.
- A cascade remains a cascade: it still drives one transition per part, still skips a part in a terminal state, and still aborts the request when a part's guard fails.
